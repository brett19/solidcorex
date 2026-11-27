// Written for solidcorex. Apache-2.0.
//! The scorer (research 73 §16): runs `scx_ssi::intersect` on a case and judges the answer
//! against the truth stored with the case. The verdicts follow the corpus's own (CRASH,
//! ERROR, INCOMPLETE, WRONG TOPOLOGY, INACCURATE, PASS), with two changes: a typed refusal
//! is its own verdict, counted by kind and not as a failure, and a certificate that claims
//! more than the truth allows is `BAD CERT`.

use std::time::Instant;

use scx_curve::Curve;
use scx_num::budget::{Budget, Stop};
use scx_num::units::{Accuracy, Length, Resolution};
use scx_num::vector2d::Point2;
use scx_ssi::analytic::Intersection;
use scx_ssi::general::{Branch, OnSurface, Traced};
use scx_ssi::{Answer, SsiError, intersect};

use crate::case::{Case, P3};

/// The tolerance a case is run at (mm), unless its feature is finer ([`Case::tol`]): the
/// pin's default zone. `acc_tol` is ten times it.
pub const TOL: f64 = 1e-6;
/// The work bound of one case (budget units). Fixed, so a refusal for it is reproducible.
pub const BUDGET: u64 = 2_000_000;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Verdict {
    Pass,
    Incomplete,
    WrongTopology,
    BadCert,
    Inaccurate,
    /// A typed refusal, by its stable name.
    Refused(String),
    /// The case could not be set up (a surface the loader cannot build).
    Setup,
}

impl Verdict {
    pub fn label(&self) -> String {
        match self {
            Self::Pass => "PASS".into(),
            Self::Incomplete => "INCOMPLETE".into(),
            Self::WrongTopology => "WRONG TOPOLOGY".into(),
            Self::BadCert => "BAD CERT".into(),
            Self::Inaccurate => "INACCURATE".into(),
            Self::Refused(n) => format!("REFUSED {n}"),
            Self::Setup => "SETUP".into(),
        }
    }
}

/// What an answer holds, as sampled points.
#[derive(Default)]
pub struct Output {
    pub curves: Vec<Vec<P3>>,
    /// Touches and nodes.
    pub points: Vec<P3>,
    /// Curves, nodes, touches.
    pub counts: [usize; 3],
    pub coincident: bool,
    /// The boundary of each coincident piece, on the first surface.
    pub regions: Vec<Vec<P3>>,
    pub certs: Vec<BranchCert>,
}

/// What a traced branch claims, and what was observed.
pub struct BranchCert {
    pub samples: Vec<P3>,
    /// The claimed distance to the true intersection.
    pub to_intersection: f64,
    /// Per pcurve: the observed residual on its surface, and the claim (`None` = exact).
    pub residual: [(f64, Option<f64>); 2],
}

pub struct Row {
    pub id: String,
    pub verdict: Verdict,
    pub truth: [usize; 3],
    pub out: Option<[usize; 3]>,
    pub h_out: Option<f64>,
    pub h_tru: Option<f64>,
    /// Branches whose certificate held, of the branches that carry one.
    pub certs: (usize, usize),
    /// How far the truth's own points lie from the two built surfaces (setup check).
    pub surf: Option<f64>,
    pub ms: f64,
}

// ---------------------------------------------------------------- geometry

fn sub(a: P3, b: P3) -> P3 {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}
fn dot(a: P3, b: P3) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}
fn dist(a: P3, b: P3) -> f64 {
    let d = sub(a, b);
    dot(d, d).sqrt()
}
fn finite(p: P3) -> bool {
    p.iter().all(|x| x.is_finite())
}

pub fn seg_dist(p: P3, a: P3, b: P3) -> f64 {
    let ab = sub(b, a);
    let l2 = dot(ab, ab);
    let t = if l2 > 0.0 {
        (dot(sub(p, a), ab) / l2).clamp(0.0, 1.0)
    } else {
        0.0
    };
    dist(p, [a[0] + t * ab[0], a[1] + t * ab[1], a[2] + t * ab[2]])
}

/// The distance from `p` to a polyline (a point when it has one vertex).
fn poly_dist(p: P3, poly: &[P3]) -> f64 {
    match poly {
        [] => f64::INFINITY,
        [a] => dist(p, *a),
        _ => poly
            .windows(2)
            .map(|w| match w {
                [a, b] => seg_dist(p, *a, *b),
                _ => f64::INFINITY,
            })
            .fold(f64::INFINITY, f64::min),
    }
}

fn set_dist(p: P3, set: &[Vec<P3>]) -> f64 {
    set.iter()
        .map(|s| poly_dist(p, s))
        .fold(f64::INFINITY, f64::min)
}

/// The largest chord error of the truth's polylines (sagitta from the turn at each vertex).
fn chord_slack(polys: &[&[P3]]) -> f64 {
    let mut worst = 0.0f64;
    for poly in polys {
        for w in poly.windows(3) {
            if let [a, b, c] = w {
                let (u, v) = (sub(*b, *a), sub(*c, *b));
                let (lu, lv) = (dot(u, u).sqrt(), dot(v, v).sqrt());
                if lu > 0.0 && lv > 0.0 {
                    let cos = (dot(u, v) / (lu * lv)).clamp(-1.0, 1.0);
                    worst = worst.max(0.5 * (lu + lv) * scx_num::fmath::acos(cos) / 8.0);
                }
            }
        }
    }
    worst
}

// ---------------------------------------------------------------- the engine's answer

fn sample_curve(c: &Curve, lo: f64, hi: f64, n: usize) -> Vec<P3> {
    (0..=n)
        .map(|i| {
            let t = if i == n {
                hi
            } else {
                lo + (hi - lo) * (i as f64) / (n as f64)
            };
            let p = c.point(t);
            [p.x, p.y, p.z]
        })
        .collect()
}

/// A NURBS branch sampled over its knots' domain: 32 points a span.
fn branch_times(b: &Branch) -> Vec<f64> {
    let d = b.curve.domain();
    let mut ks: Vec<f64> = b
        .curve
        .knots()
        .iter()
        .copied()
        .filter(|k| *k >= d.min() && *k <= d.max())
        .collect();
    ks.dedup();
    let mut ts = Vec::new();
    for w in ks.windows(2) {
        if let [a, c] = w {
            for i in 0..32 {
                ts.push(a + (c - a) * f64::from(i) / 32.0);
            }
        }
    }
    ts.push(d.max());
    ts
}

fn branch_output(b: &Branch, case: &Case) -> (Vec<P3>, BranchCert) {
    let curve = Curve::Nurbs(b.curve.clone());
    let ts = branch_times(b);
    let samples: Vec<P3> = ts
        .iter()
        .map(|&t| {
            let p = curve.point(t);
            [p.x, p.y, p.z]
        })
        .collect();
    let mut residual = [(0.0, None), (0.0, None)];
    for (k, slot) in residual.iter_mut().enumerate() {
        let (Some(pc), Some(side)) = (b.pcurves.get(k), case.sides.get(k)) else {
            continue;
        };
        let mut worst = 0.0f64;
        for (&t, s) in ts.iter().zip(&samples) {
            let uv: Point2 = pc.point(t);
            let q = side.surface.point(uv);
            worst = worst.max(dist([q.x, q.y, q.z], *s));
        }
        let claim = match b.certificate.on_surface.get(k) {
            Some(OnSurface::Within(l)) => Some(l.mm()),
            _ => None,
        };
        *slot = (worst, claim);
    }
    let cert = BranchCert {
        samples: samples.clone(),
        to_intersection: b.certificate.to_intersection.mm(),
        residual,
    };
    (samples, cert)
}

fn traced_output(t: &Traced, case: &Case) -> Output {
    let mut o = Output {
        counts: [t.branches.len(), t.nodes.len(), t.touches.len()],
        ..Output::default()
    };
    for b in &t.branches {
        let (poly, cert) = branch_output(b, case);
        o.curves.push(poly);
        o.certs.push(cert);
    }
    o.points
        .extend(t.nodes.iter().map(|n| [n.point.x, n.point.y, n.point.z]));
    o.points
        .extend(t.touches.iter().map(|n| [n.point.x, n.point.y, n.point.z]));
    o
}

/// A polygon of uv on `side`'s surface, each edge in 32 steps, as 3D points.
fn polygon_points(side: &crate::case::Side, uv: &[Point2]) -> Vec<P3> {
    let mut out = Vec::new();
    for (k, a) in uv.iter().enumerate() {
        let b = uv.get((k + 1) % uv.len()).copied().unwrap_or(*a);
        for i in 0..32 {
            let t = f64::from(i) / 32.0;
            let q = side
                .surface
                .point(Point2::new(a.x + t * (b.x - a.x), a.y + t * (b.y - a.y)));
            out.push([q.x, q.y, q.z]);
        }
    }
    out.extend(out.first().copied());
    out
}

fn closed_output(i: &Intersection, case: &Case) -> Output {
    let mut o = Output::default();
    match i {
        Intersection::Empty => {}
        Intersection::Coincident(pieces) => {
            o.coincident = true;
            o.regions = pieces
                .iter()
                .map(|p| polygon_points(&case.sides[0], &p.on_a))
                .collect();
        }
        Intersection::Pieces { curves, points } => {
            for c in curves {
                let n = if matches!(c.curve, Curve::Line(_)) {
                    2
                } else {
                    720
                };
                o.curves
                    .push(sample_curve(&c.curve, c.span.min(), c.span.max(), n));
            }
            o.points
                .extend(points.iter().map(|p| [p.point.x, p.point.y, p.point.z]));
            o.counts = [curves.len(), 0, points.len()];
        }
        _ => {}
    }
    o
}

fn output_of(answer: &Answer, case: &Case) -> Output {
    match answer {
        Answer::Closed(i) => closed_output(i, case),
        Answer::Traced(t) => traced_output(t, case),
    }
}

// ---------------------------------------------------------------- the judgement

/// The closed-form truth (`scx_testkit::ssi_truth`, SM0) of the case where it has one: two
/// spheres. Its Hausdorff measure replaces the polyline distances there.
fn closed_truth(case: &Case) -> Option<scx_testkit::ssi_truth::Truth> {
    let [Some((c1, r1)), Some((c2, r2))] = [case.sides[0].sphere, case.sides[1].sphere] else {
        return None;
    };
    scx_testkit::ssi_truth::sphere_sphere(c1, r1, c2, r2)
}

/// The components of curves (with their free-end counts) and points joined within `cov`,
/// as a sorted list of `(free ends, holds a curve)`.
fn signature(curves: &[(Vec<P3>, bool)], points: &[P3], cov: f64) -> Vec<(usize, bool)> {
    // A curve no larger than the coverage is a point.
    let mut pts: Vec<P3> = points.to_vec();
    let mut cs: Vec<(&[P3], bool)> = Vec::new();
    for (poly, closed) in curves {
        let Some(&first) = poly.first() else { continue };
        if poly.iter().all(|q| dist(*q, first) <= cov) {
            pts.push(first);
        } else {
            cs.push((poly, *closed));
        }
    }
    let n = cs.len() + pts.len();
    let mut parent: Vec<usize> = (0..n).collect();
    fn root(p: &mut [usize], mut i: usize) -> usize {
        while p[i] != i {
            p[i] = p[p[i]];
            i = p[i];
        }
        i
    }
    fn join(p: &mut [usize], a: usize, b: usize) {
        let (ra, rb) = (root(p, a), root(p, b));
        if let Some(slot) = p.get_mut(ra.max(rb)) {
            *slot = ra.min(rb);
        }
    }
    // Endpoint clusters (single linkage) over the open curves.
    let mut ends: Vec<(P3, usize)> = Vec::new();
    for (i, (poly, closed)) in cs.iter().enumerate() {
        if !closed {
            if let (Some(&a), Some(&b)) = (poly.first(), poly.last()) {
                ends.push((a, i));
                ends.push((b, i));
            }
        }
    }
    let mut ep: Vec<usize> = (0..ends.len()).collect();
    for i in 0..ends.len() {
        for j in 0..i {
            if let (Some(a), Some(b)) = (ends.get(i), ends.get(j)) {
                if dist(a.0, b.0) <= cov {
                    join(&mut ep, i, j);
                    join(&mut parent, a.1, b.1);
                }
            }
        }
    }
    let mut degree = vec![0usize; ends.len()];
    for i in 0..ends.len() {
        let r = root(&mut ep, i);
        if let Some(d) = degree.get_mut(r) {
            *d += 1;
        }
    }
    // Points join the curves they lie on, and each other.
    for (k, p) in pts.iter().enumerate() {
        let me = cs.len() + k;
        for (i, (poly, _)) in cs.iter().enumerate() {
            if poly_dist(*p, poly) <= cov {
                join(&mut parent, me, i);
            }
        }
        for (j, q) in pts.iter().enumerate().take(k) {
            if dist(*p, *q) <= cov {
                join(&mut parent, me, cs.len() + j);
            }
        }
    }
    let mut comps: std::collections::BTreeMap<usize, (usize, bool)> = Default::default();
    for i in 0..n {
        let r = root(&mut parent, i);
        let e = comps.entry(r).or_default();
        if i < cs.len() {
            e.1 = true;
        }
    }
    for i in 0..ends.len() {
        let r = root(&mut ep, i);
        if degree.get(r).copied() == Some(1) && r == i {
            if let Some((_, ci)) = ends.get(i) {
                let comp = root(&mut parent, *ci);
                comps.entry(comp).or_default().0 += 1;
            }
        }
    }
    let mut v: Vec<(usize, bool)> = comps.into_values().collect();
    v.sort_unstable();
    v
}

/// Judge `out` against the truth of `case`: the row, without the timing and setup fields.
pub fn judge(case: &Case, out: &Output) -> Row {
    let acc_tol = 10.0 * case.tol();
    let cov = case.cov_tol.max((2.0 * acc_tol * case.ill_cond).min(0.5));
    let truth_counts = [
        case.branches.len(),
        case.singular.len(),
        case.isolated.len(),
    ];
    let mut row = Row {
        id: case.id.clone(),
        verdict: Verdict::Pass,
        truth: truth_counts,
        out: Some(out.counts),
        h_out: None,
        h_tru: None,
        certs: (0, out.certs.len()),
        surf: None,
        ms: 0.0,
    };
    let has_region = !case.coincident.is_empty() || out.coincident;
    let truth_polys: Vec<Vec<P3>> = case
        .branches
        .iter()
        .map(|b| b.points.clone())
        .chain(case.isolated.iter().map(|p| vec![*p]))
        .chain(case.singular.iter().map(|p| vec![*p]))
        .chain(case.coincident.iter().cloned())
        .collect();
    // A coincident piece's edges include the cuts between pieces, which are not on the
    // truth's boundary: they count toward coverage only, not toward `h_out`.
    let mut out_polys: Vec<Vec<P3>> = out
        .curves
        .iter()
        .cloned()
        .chain(out.points.iter().map(|p| vec![*p]))
        .collect();
    // Where the truth has no region, an answer's pieces are held to the truth (a piece of
    // no width along a seam is a curve).
    if case.coincident.is_empty() {
        out_polys.extend(out.regions.iter().cloned());
    }
    let on_truth_polys = out_polys.clone();
    if !case.coincident.is_empty() {
        out_polys.extend(out.regions.iter().cloned());
    }
    let all_finite = on_truth_polys.iter().flatten().all(|p| finite(*p));
    let slack = chord_slack(&truth_polys.iter().map(Vec::as_slice).collect::<Vec<_>>()).min(cov);

    // Truth covered by the output.
    let mut h_tru = 0.0f64;
    for p in case
        .branches
        .iter()
        .flat_map(|b| b.points.iter())
        .chain(&case.isolated)
        .chain(case.coincident.iter().flatten())
    {
        h_tru = h_tru.max(set_dist(*p, &out_polys));
    }
    // Output on the truth.
    let mut h_out = 0.0f64;
    for p in on_truth_polys.iter().flatten() {
        h_out = h_out.max(set_dist(*p, &truth_polys));
    }
    // Where SM0 has the exact curve, its Hausdorff measure judges the curves (the points
    // keep the corpus's polylines).
    if let Some(truth) = closed_truth(case).filter(|t| !t.curves.is_empty()) {
        if !out.curves.is_empty() {
            h_out = truth.to_truth(&out.curves);
            h_tru = truth.from_truth(&out.curves, 720);
        }
    }
    row.h_tru = Some(h_tru);
    row.h_out = Some(h_out);

    // The certificates, against the truth.
    let mut certs_ok = 0;
    for c in &out.certs {
        let observed = c
            .samples
            .iter()
            .map(|p| set_dist(*p, &truth_polys))
            .fold(0.0f64, f64::max);
        let resid_ok = c.residual.iter().all(|(seen, claim)| match claim {
            Some(b) => *seen <= b * (1.0 + 1e-9) + 1e-12,
            None => *seen <= 1e-9,
        });
        if c.to_intersection.is_finite()
            && c.to_intersection >= 0.0
            && observed <= c.to_intersection + slack + 1e-12
            && resid_ok
        {
            certs_ok += 1;
        }
    }
    row.certs.0 = certs_ok;

    let truth_curves: Vec<(Vec<P3>, bool)> = case
        .branches
        .iter()
        .map(|b| (b.points.clone(), b.closed))
        .collect();
    let out_curves: Vec<(Vec<P3>, bool)> = out.curves.iter().map(|c| (c.clone(), false)).collect();
    let singular_ok = case
        .singular
        .iter()
        .all(|s| set_dist(*s, &out_polys) <= cov);
    // Components are compared where no coincident piece is involved. The singular points
    // join both sides' curves through them: curves crossing at a node are one connected
    // set, whether an answer runs each through it or cuts them there (`singular_ok` holds
    // the output to passing through each).
    let with_singular = |points: &[P3]| [points, &case.singular[..]].concat();
    let same = has_region
        || signature(&truth_curves, &with_singular(&case.isolated), cov)
            == signature(&out_curves, &with_singular(&out.points), cov);

    row.verdict = if !truth_polys.iter().any(|p| !p.is_empty()) && out_polys.is_empty() {
        Verdict::Pass
    } else if h_tru > cov || (out_polys.is_empty() && !truth_polys.is_empty()) {
        Verdict::Incomplete
    } else if !all_finite || h_out > cov || !same || !singular_ok {
        Verdict::WrongTopology
    } else if certs_ok < out.certs.len() {
        Verdict::BadCert
    } else if h_out > acc_tol + slack {
        Verdict::Inaccurate
    } else {
        Verdict::Pass
    };
    row
}

// ---------------------------------------------------------------- running a case

/// The largest distance from a few of the truth's points to either built surface.
fn surface_fit(case: &Case) -> Option<f64> {
    let pts: Vec<P3> = case
        .branches
        .iter()
        .flat_map(|b| {
            let step = (b.points.len() / 4).max(1);
            b.points.iter().step_by(step).copied().collect::<Vec<_>>()
        })
        .chain(case.isolated.iter().copied())
        .collect();
    let stop = Stop::new();
    let mut worst = 0.0f64;
    for p in pts {
        for side in &case.sides {
            let mut budget = Budget::new(BUDGET);
            let q = scx_num::vector3d::Point3::new(p[0], p[1], p[2]);
            let foot = side
                .surface
                .invert(q, None, None, &mut budget, &stop)
                .ok()?;
            worst = worst.max(foot.foot().distance);
        }
    }
    Some(worst)
}

/// Run `scx_ssi::intersect` on the case and judge it.
pub fn run_case(case: &Case) -> Row {
    let [a, b] = &case.sides;
    let acc = Accuracy::new(Length::from_mm(case.tol())).map(|a| [a, a]);
    let res = Resolution::new(Length::from_mm(case.tol()));
    let (Some(acc), Some(res)) = (acc, res) else {
        return setup_row(case);
    };
    let mut budget = Budget::new(BUDGET);
    let stop = Stop::new();
    let started = Instant::now();
    let answer = intersect(
        &a.surface,
        &a.extent,
        &b.surface,
        &b.extent,
        acc,
        res,
        &mut budget,
        &stop,
    );
    let ms = started.elapsed().as_secs_f64() * 1e3;
    let mut row = match answer {
        Ok(ans) => judge(case, &output_of(&ans, case)),
        Err(e) => refusal_row(case, &e),
    };
    row.ms = ms;
    row.surf = surface_fit(case);
    row
}

fn refusal_row(case: &Case, e: &SsiError) -> Row {
    Row {
        id: case.id.clone(),
        verdict: Verdict::Refused(e.name().to_string()),
        truth: [
            case.branches.len(),
            case.singular.len(),
            case.isolated.len(),
        ],
        out: None,
        h_out: None,
        h_tru: None,
        certs: (0, 0),
        surf: None,
        ms: 0.0,
    }
}

pub fn setup_row(case: &Case) -> Row {
    let mut r = refusal_row(case, &SsiError::Stopped);
    r.verdict = Verdict::Setup;
    r
}
