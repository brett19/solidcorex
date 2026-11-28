// Written for solidcorex. Apache-2.0.
//! Edge sampling: the points an edge's polyline runs through, as a pure function of the edge
//! (research 72 §6.2, 66 §12.1).
//!
//! The count is the largest of the curve's own need and each adjacent face's need. A maximum
//! is the same in any order, so every face sees the same samples, bit for bit, and the mesh
//! is watertight by construction. The pin's face-order sampling (and Fornjot's face-order
//! cache) is the shape avoided.
//!
//! **The curve's own need** is in closed form for a line, a circle and an ellipse, and from
//! interval enclosures of the first two derivatives for a NURBS curve:
//!
//! - **Circle**, radius `r`: a segment over an angle `h` stays within `2 r sin²(h/4)` of the
//!   arc (the sagitta), each way. The step is `4 asin(sqrt(c / 2r))`.
//! - **Ellipse**, semi-axes `a ≥ b`: the affine image of a unit circle, so every point of the
//!   arc lies within `a (1 − cos(h/2))` of its chord, each way (the circle's pairing, scaled by
//!   at most `a`), tight at the major axis. The tangent turns at most `a/b` per radian of `t`.
//! - **Line**: no chord error; only the length bounds count.
//! - **NURBS**: split at every distinct knot inside the interval; on each piece, enclosures
//!   of `|C'|` and `|C''|` give `M1 ≥ |C'|`, `m1 ≤ |C'|` and `M2 ≥ |C''|`, and a chord over a
//!   parameter step `s` deviates by at most `M2 s² / 8`. A piece is bisected while that
//!   lowers its count.
//!
//! **A face's need** is a longest segment `L_f = 0.8 sqrt(3) sqrt(c (2ρ − c))` (and
//! `angle / κ_f` when a turning bound is set), `ρ = 1/κ_f` the face's least radius of
//! curvature: the curved faces' lattice side, 0.8 of the equilateral side whose enclosing
//! circle is the cap of sag `c` on a sphere of radius `ρ`. The cap's full chord
//! `2 sqrt(c (2ρ − c))` is the limit itself: a triangle on such a segment passes the
//! per-triangle test only once it is obtuse, which took the sphere's seam many refinement
//! rounds (72 §12 row 6, §14.23; TS1 had the second-order `sqrt(8 c / κ_f)`, which a torus
//! equator's own triangles could never meet). A plane, cylinder or cone gives
//! none: developable, its curvature across a segment is at most the curve's own along it
//! (the triangle test there must use the developable bound). A sphere gives `1/r`, a torus
//! `max(1/r, 1/(R − r))`.
//!
//! Every non-line edge has at least two segments and a closed one at least three, so no face
//! closes into a sliver. No conic segment turns by more than a quarter (the pin's
//! `QUARTER_TURN`); a NURBS segment's turn is bounded only when the spec asks for an angle,
//! since its enclosure of the turn is too loose to be a free floor (research 72 §14.15).

use scx_brep::{Edge, EdgeId, EdgeShape};
use scx_curve::Curve;
use scx_num::budget::{Budget, Stop};
use scx_num::fmath::{asin, sin};
use scx_num::interval::Ivl;
use scx_num::math::HALF_PI;
use scx_num::units::Length;
use scx_num::vector3d::{Point3, sqrt};
use scx_nurbs::{Approach, NurbsCurve};
use scx_surface::{CurveKind, Surface, SurfaceKind};

use crate::error::{TessDefect, TessError, TessRefusal, TessUnsupported};
use crate::spec::TessSpec;

/// The most segments one edge may have: what a mesh's `u32` indices leave room for, with
/// the rest of the body beside it.
const MAX_SEGMENTS: f64 = (u32::MAX / 4) as f64;

/// How deep a NURBS knot span is bisected for tighter enclosures: a 4096th of it.
const MAX_DEPTH: u32 = 12;

/// A bisection is kept only when its halves need less than this share of their parent's
/// segments: past that the enclosure is tight, and further halving costs without gain.
const GAIN: f64 = 0.95;

/// A piece needing this many segments or fewer is not bisected: its enclosure spans a few
/// segments, so a tighter one saves little and costs two more.
const LEAF: f64 = 2.0;

/// How many times a closed-form count is raised by one before its own check is a defect.
const MAX_BUMPS: u32 = 4;

/// An edge's samples: the parameters strictly between its ends, ascending, and the curve's
/// points at them. Its end vertices are not in it; a polyline runs from the start vertex,
/// through `points`, to the end vertex.
#[derive(Debug, Clone, PartialEq)]
pub struct EdgeSamples {
    params: Vec<f64>,
    points: Vec<Point3>,
    chord: Length,
}

impl EdgeSamples {
    /// The parameters, the edge's own (each face's pcurve is evaluated at these).
    #[must_use]
    pub fn params(&self) -> &[f64] {
        &self.params
    }

    /// The curve's points at [`EdgeSamples::params`].
    #[must_use]
    pub fn points(&self) -> &[Point3] {
        &self.points
    }

    /// The proved bound on the distance between the polyline and the curve, each way.
    #[must_use]
    pub const fn chord(&self) -> Length {
        self.chord
    }

    /// The number of segments: one more than the samples.
    #[must_use]
    pub fn segments(&self) -> usize {
        self.params.len() + 1
    }

    fn collapsed() -> Self {
        Self {
            params: Vec::new(),
            points: Vec::new(),
            chord: Length::ZERO,
        }
    }
}

/// The samples of `edge` that hold `spec`, a function of the edge and the surfaces of its
/// faces alone.
///
/// Charges one unit per segment, and one per NURBS enclosure.
///
/// # Errors
///
/// - [`TessUnsupported::EdgeKind`] for a parabola or hyperbola edge.
/// - [`TessUnsupported::FaceKind`] for an adjacent face with no curvature bound.
/// - [`TessUnsupported::NoBound`] for a NURBS edge whose enclosures bound no count.
/// - [`TessRefusal::TooFine`] for more segments than a mesh indexes.
/// - [`TessError::Exhausted`], [`TessError::Cancelled`].
/// - [`TessDefect::Sagitta`] if a closed-form count fails its own check.
pub fn sample_edge(
    edge: Edge<'_>,
    spec: &TessSpec,
    budget: &mut Budget,
    stop: &Stop,
) -> Result<EdgeSamples, TessError> {
    stop.check()?;
    let EdgeShape::Curve { curve, interval } = edge.shape() else {
        return Ok(EdgeSamples::collapsed());
    };
    let id = edge.id();
    let mut face_len = f64::INFINITY;
    for u in edge.uses() {
        face_len = face_len.min(face_need(u.face().surface(), spec).map_err(|kind| {
            TessError::Unsupported(TessUnsupported::FaceKind {
                face: u.face().id(),
                kind,
            })
        })?);
    }
    let [start, end] = edge.ends();
    let need = Need {
        chord: spec.chord().mm(),
        angle: spec.angle(),
        edge_length: spec.edge_length().map(Length::mm),
        face_len,
        least: if start == end { 3 } else { 2 },
    };
    samples(id, curve, interval.min(), interval.max(), &need, budget)
}

/// The samples of `curve` over `[lo, hi]` holding `need`.
fn samples(
    id: EdgeId,
    curve: &Curve,
    lo: f64,
    hi: f64,
    need: &Need,
    budget: &mut Budget,
) -> Result<EdgeSamples, TessError> {
    let plan = match curve {
        Curve::Line(l) => line(id, l.direction().length() * (hi - lo), lo, hi, need)?,
        Curve::Circle(c) => conic(id, c.radius().mm(), 1.0, lo, hi, need)?,
        Curve::Ellipse(e) => {
            let (x, y) = (e.semi_x().mm(), e.semi_y().mm());
            let (a, b) = if x >= y { (x, y) } else { (y, x) };
            conic(id, a, b / a, lo, hi, need)?
        }
        Curve::Nurbs(n) => nurbs(id, n, lo, hi, need, budget)?,
        other => {
            return Err(TessError::Unsupported(TessUnsupported::EdgeKind {
                edge: id,
                kind: CurveKind::of(other),
            }));
        }
    };
    let segments = plan.pieces.iter().map(|p| p.count).sum::<u64>();
    budget.charge(segments)?;
    let mut params = Vec::with_capacity(usize::try_from(segments).unwrap_or(0));
    for (k, p) in plan.pieces.iter().enumerate() {
        if k > 0 {
            params.push(p.lo);
        }
        let n = p.count as f64;
        for i in 1..p.count {
            params.push(p.lo + (p.hi - p.lo) * (i as f64 / n));
        }
    }
    let points = params.iter().map(|&t| curve.point(t)).collect();
    Ok(EdgeSamples {
        params,
        points,
        chord: Length::from_mm(plan.chord),
    })
}

/// The largest face curvature's longest segment: `Err` names a kind with no bound.
fn face_need(surface: &Surface, spec: &TessSpec) -> Result<f64, SurfaceKind> {
    let kappa = match surface {
        Surface::Plane(_) | Surface::Cylinder(_) | Surface::Cone(_) => return Ok(f64::INFINITY),
        Surface::Sphere(s) => 1.0 / s.radius().mm(),
        Surface::Torus(t) => {
            let (major, minor) = (t.major().mm(), t.minor().mm());
            // A horn torus's tube meets the axis: its parallels' curvature has no bound, but
            // its boundary is the meridian and the pole's line, so the tube's is the one that
            // bounds an edge's segments (the face's own triangles are bounded one by one).
            if t.pole().is_some() {
                1.0 / minor
            } else {
                (1.0 / minor).max(1.0 / (major - minor))
            }
        }
        other => return Err(SurfaceKind::of(other)),
    };
    // The curved faces' lattice side (`SIDE` of the equilateral side whose enclosing circle
    // the chord allows on a sphere of radius `rho`): a boundary segment no longer leaves a
    // triangle on it room to pass without being obtuse.
    let rho = 1.0 / kappa;
    let c = spec.chord().mm().min(rho);
    let chord = crate::curved::SIDE * sqrt(3.0) * sqrt(c * (2.0 * rho - c));
    Ok(spec.angle().map_or(chord, |a| chord.min(a / kappa)))
}

/// What one edge's segments must hold.
struct Need {
    chord: f64,
    angle: Option<f64>,
    edge_length: Option<f64>,
    face_len: f64,
    least: u64,
}

/// A run of equal parameter steps.
#[derive(Debug, Clone, Copy)]
struct Piece {
    lo: f64,
    hi: f64,
    count: u64,
    /// The bound on `|C''|` over the piece (a NURBS piece's); unused for a conic.
    m2: f64,
}

struct Plan {
    pieces: Vec<Piece>,
    chord: f64,
}

/// `x` segments rounded up, refused past [`MAX_SEGMENTS`].
fn whole(edge: EdgeId, x: f64) -> Result<u64, TessError> {
    if x.is_finite() && x <= MAX_SEGMENTS {
        Ok(x.ceil().max(1.0) as u64)
    } else {
        Err(TessError::Refused(TessRefusal::TooFine { edge, needed: x }))
    }
}

fn line(edge: EdgeId, length: f64, lo: f64, hi: f64, need: &Need) -> Result<Plan, TessError> {
    let by_edge = need.edge_length.map_or(1.0, |l| length / l);
    let count = whole(edge, by_edge.max(length / need.face_len))?;
    Ok(Plan {
        pieces: vec![Piece {
            lo,
            hi,
            count,
            m2: 0.0,
        }],
        chord: 0.0,
    })
}

/// A circle (`ratio` 1) or ellipse over `[lo, hi]`: `a` the largest radius, `ratio` = `b/a`
/// the least turning factor.
fn conic(
    edge: EdgeId,
    a: f64,
    ratio: f64,
    lo: f64,
    hi: f64,
    need: &Need,
) -> Result<Plan, TessError> {
    let span = hi - lo;
    let mut step = HALF_PI * ratio;
    if need.chord < 2.0 * a {
        step = step.min(4.0 * asin(sqrt(need.chord / (2.0 * a))));
    }
    if let Some(angle) = need.angle {
        step = step.min(angle * ratio);
    }
    if let Some(l) = need.edge_length {
        step = step.min(l / a);
    }
    step = step.min(need.face_len / a);
    let mut count = whole(edge, span / step)?.max(need.least);
    let sag = |n: u64| {
        let s = sin(span / n as f64 / 4.0);
        2.0 * a * s * s
    };
    let mut bumps = 0;
    while sag(count) > need.chord {
        if bumps == MAX_BUMPS {
            return Err(TessError::Defect(TessDefect::Sagitta { edge }));
        }
        bumps += 1;
        count += 1;
    }
    Ok(Plan {
        pieces: vec![Piece {
            lo,
            hi,
            count,
            m2: 0.0,
        }],
        chord: sag(count),
    })
}

/// A NURBS curve over `[lo, hi]`: split at its interior knots; each knot span bisected
/// while its derivative enclosures tighten, and the leaves merged back into runs while a run
/// needs no more segments than its parts apart. A segment stays inside one knot span, where
/// the curve is twice differentiable and the chord's `M2 s² / 8` holds.
fn nurbs(
    edge: EdgeId,
    curve: &NurbsCurve<3>,
    lo: f64,
    hi: f64,
    need: &Need,
    budget: &mut Budget,
) -> Result<Plan, TessError> {
    let mut cuts = vec![lo];
    for &k in curve.knots() {
        if k > lo && k < hi && cuts.last().is_some_and(|&last| k > last) {
            cuts.push(k);
        }
    }
    cuts.push(hi);
    let mut pieces: Vec<Piece> = Vec::new();
    for w in cuts.windows(2) {
        if let &[a, b] = w {
            let span = enclose(curve, a, b, budget)?;
            for run in merge(grow(curve, span, need, 0, budget)?, need) {
                let raw = run.raw(need);
                if !raw.is_finite() {
                    return Err(TessError::Unsupported(TessUnsupported::NoBound { edge }));
                }
                pieces.push(Piece {
                    lo: run.lo,
                    hi: run.hi,
                    count: whole(edge, raw)?,
                    m2: run.m2,
                });
            }
        }
    }
    let mut total = pieces.iter().map(|p| p.count).sum::<u64>();
    while total < need.least {
        let widest = pieces
            .iter_mut()
            .reduce(|w, p| {
                if (p.hi - p.lo) / p.count as f64 > (w.hi - w.lo) / w.count as f64 {
                    p
                } else {
                    w
                }
            })
            .ok_or(TessError::Unsupported(TessUnsupported::NoBound { edge }))?;
        widest.count += 1;
        total += 1;
    }
    if total as f64 > MAX_SEGMENTS {
        return Err(TessError::Refused(TessRefusal::TooFine {
            edge,
            needed: total as f64,
        }));
    }
    let chord = pieces
        .iter()
        .map(|p| {
            let s = (p.hi - p.lo) / p.count as f64;
            p.m2 * s * s / 8.0
        })
        .fold(0.0, f64::max);
    Ok(Plan { pieces, chord })
}

/// Enclosures of a NURBS curve's derivatives over `[lo, hi]`.
#[derive(Debug, Clone, Copy)]
struct Bound {
    lo: f64,
    hi: f64,
    /// `sup |C'|`.
    m1: f64,
    /// `inf |C'|`, 0 where the enclosure holds 0.
    least_speed: f64,
    /// `sup |C''|`.
    m2: f64,
}

impl Bound {
    /// The segments the bounds ask for over the whole interval, unrounded: infinite where
    /// they bound none (an enclosure that is not finite, or a turning bound asked of a speed
    /// that may vanish).
    fn raw(&self, need: &Need) -> f64 {
        let (m1, m2) = (self.m1, self.m2);
        if !(m1.is_finite() && m2.is_finite()) {
            return f64::INFINITY;
        }
        let h = self.hi - self.lo;
        // A tangent turns by at most `|C''| / |C'|` per unit of parameter.
        let turn = |limit: f64| {
            if m2 == 0.0 {
                0.0
            } else {
                h * m2 / (self.least_speed * limit)
            }
        };
        let mut k = h * sqrt(m2 / (8.0 * need.chord));
        k = k.max(h * m1 / need.face_len);
        if let Some(l) = need.edge_length {
            k = k.max(h * m1 / l);
        }
        if let Some(angle) = need.angle {
            k = k.max(turn(angle));
        }
        k
    }

    fn join(self, o: Self) -> Self {
        Self {
            lo: self.lo,
            hi: o.hi,
            m1: self.m1.max(o.m1),
            least_speed: self.least_speed.min(o.least_speed),
            m2: self.m2.max(o.m2),
        }
    }
}

/// One enclosure, charged one unit.
fn enclose(
    curve: &NurbsCurve<3>,
    lo: f64,
    hi: f64,
    budget: &mut Budget,
) -> Result<Bound, TessError> {
    budget.charge(1)?;
    let (_, [d1, d2]) = curve.jet_over::<Ivl, 2>(Ivl::new(lo, hi), Approach::Above);
    let speed = scx_num::vector3d::Vec3::new(d1[0], d1[1], d1[2]).norm();
    let bend = scx_num::vector3d::Vec3::new(d2[0], d2[1], d2[2]).norm();
    let nan_to_inf = |x: f64| if x.is_nan() { f64::INFINITY } else { x };
    Ok(Bound {
        lo,
        hi,
        m1: nan_to_inf(speed.hi),
        least_speed: if speed.lo > 0.0 { speed.lo } else { 0.0 },
        m2: nan_to_inf(bend.hi),
    })
}

/// `own` as leaves: bisected while its halves need clearly fewer segments, down to
/// [`MAX_DEPTH`], and not once it is [`LEAF`] segments or less wide.
fn grow(
    curve: &NurbsCurve<3>,
    own: Bound,
    need: &Need,
    depth: u32,
    budget: &mut Budget,
) -> Result<Vec<Bound>, TessError> {
    let raw = own.raw(need);
    let mid = 0.5 * (own.lo + own.hi);
    if depth == MAX_DEPTH || raw <= LEAF || !(own.lo < mid && mid < own.hi) {
        return Ok(vec![own]);
    }
    let left = enclose(curve, own.lo, mid, budget)?;
    let right = enclose(curve, mid, own.hi, budget)?;
    // An infinite own count always splits; a finite one only for a clear gain (an infinite
    // half keeps the parent). `raw` is never NaN: its enclosures are finite or it is
    // infinite.
    if raw.is_finite() && left.raw(need) + right.raw(need) >= GAIN * raw {
        return Ok(vec![own]);
    }
    let mut leaves = grow(curve, left, need, depth + 1, budget)?;
    leaves.extend(grow(curve, right, need, depth + 1, budget)?);
    Ok(leaves)
}

/// Adjacent leaves joined, left to right, while the joined run needs no more segments than
/// the run and the next leaf apart.
fn merge(leaves: Vec<Bound>, need: &Need) -> Vec<Bound> {
    let mut runs: Vec<Bound> = Vec::with_capacity(leaves.len());
    for leaf in leaves {
        match runs.last_mut() {
            Some(run) => {
                let joined = run.join(leaf);
                if joined.raw(need).ceil() <= run.raw(need).ceil() + leaf.raw(need).ceil() {
                    *run = joined;
                } else {
                    runs.push(leaf);
                }
            }
            None => runs.push(leaf),
        }
    }
    runs
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::indexing_slicing, clippy::panic)]

    use std::sync::Arc;

    use scx_curve::{Circle, Ellipse, Parabola};
    use scx_num::frame::{Frame, Sense};
    use scx_num::math::{PI, TWO_PI};
    use scx_num::vector3d::Vec3;
    use scx_nurbs::KnotVector;

    use super::*;

    const R2: f64 = core::f64::consts::FRAC_1_SQRT_2;

    fn id() -> EdgeId {
        EdgeId::from_raw(0)
    }

    fn need(chord: f64, least: u64) -> Need {
        Need {
            chord,
            angle: None,
            edge_length: None,
            face_len: f64::INFINITY,
            least,
        }
    }

    fn frame() -> Frame {
        Frame::new(
            Point3::new(1.0, -2.0, 3.0),
            Vec3::new(0.3, -0.5, 0.8),
            Vec3::new(1.0, 0.2, 0.1),
        )
        .unwrap()
    }

    fn circle(r: f64) -> Curve {
        Curve::Circle(Circle::new(frame(), Length::from_mm(r), Sense::Forward).unwrap())
    }

    fn run(curve: &Curve, lo: f64, hi: f64, need: &Need) -> Result<EdgeSamples, TessError> {
        samples(id(), curve, lo, hi, need, &mut Budget::unlimited())
    }

    /// The polyline through the curve's ends and `s`.
    fn polyline(curve: &Curve, lo: f64, hi: f64, s: &EdgeSamples) -> Vec<Point3> {
        let mut p = vec![curve.point(lo)];
        p.extend_from_slice(s.points());
        p.push(curve.point(hi));
        p
    }

    /// The largest distance from the circle of radius `r` about `o` to a polyline in its
    /// plane, each way: for a chord of a circle, its midpoint's radial gap (exact).
    fn circle_gap(o: Point3, r: f64, poly: &[Point3]) -> f64 {
        poly.windows(2)
            .map(|w| {
                let m = Point3::new(
                    0.5 * (w[0].x + w[1].x),
                    0.5 * (w[0].y + w[1].y),
                    0.5 * (w[0].z + w[1].z),
                );
                r - m.distance(o)
            })
            .fold(0.0, f64::max)
    }

    fn segment_distance(p: Point3, a: Point3, b: Point3) -> f64 {
        let ab = b - a;
        let t = ((p - a).dot(ab) / ab.dot(ab)).clamp(0.0, 1.0);
        p.distance(a + ab * t)
    }

    /// The largest distance from the curve over each segment's parameter span to that
    /// segment, by `k` samples per segment: a lower estimate of the true gap, which the
    /// proved bound must not be below.
    fn sampled_gap(curve: &Curve, lo: f64, hi: f64, s: &EdgeSamples, k: usize) -> f64 {
        let mut ts = vec![lo];
        ts.extend_from_slice(s.params());
        ts.push(hi);
        ts.windows(2)
            .map(|w| {
                let (a, b) = (curve.point(w[0]), curve.point(w[1]));
                (1..k)
                    .map(|j| {
                        let t = w[0] + (w[1] - w[0]) * (j as f64 / k as f64);
                        segment_distance(curve.point(t), a, b)
                    })
                    .fold(0.0, f64::max)
            })
            .fold(0.0, f64::max)
    }

    #[test]
    fn circle_counts_hold_the_chord_and_one_fewer_would_not() {
        for r in [0.5, 1.0, 10.0, 1000.0] {
            for c in [1e-4, 1e-3, 0.01, 0.1] {
                for span in [PI / 3.0, PI, TWO_PI] {
                    let curve = circle(r);
                    let s = run(&curve, 0.25, 0.25 + span, &need(c, 2)).unwrap();
                    let n = s.segments();
                    let h = span / n as f64;
                    let truth = 2.0 * r * sin(h / 4.0) * sin(h / 4.0);
                    assert_eq!(s.chord().mm(), truth);
                    assert!(truth <= c, "r {r} c {c}: {truth}");
                    let gap = circle_gap(
                        frame().origin(),
                        r,
                        &polyline(&curve, 0.25, 0.25 + span, &s),
                    );
                    assert!(gap <= c * (1.0 + 1e-9) + 1e-12, "r {r} c {c}: gap {gap}");
                    // Minimal: one fewer segment misses the chord, unless a floor set the count.
                    let fewer = span / (n - 1) as f64;
                    let floor = n <= 2 || fewer > HALF_PI * (1.0 + 1e-12);
                    if !floor {
                        let s1 = sin(fewer / 4.0);
                        assert!(
                            2.0 * r * s1 * s1 > c,
                            "r {r} c {c} span {span}: {n} not least"
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn a_circle_segment_turns_at_most_a_quarter() {
        let s = run(&circle(1.0), 0.0, TWO_PI, &need(10.0, 3)).unwrap();
        assert_eq!(s.segments(), 4);
        let s = run(&circle(1.0), 0.0, 0.1, &need(10.0, 2)).unwrap();
        assert_eq!(s.segments(), 2);
    }

    #[test]
    fn angle_and_length_bounds_refine_a_circle() {
        let mut n = need(1.0, 2);
        n.angle = Some(0.1);
        assert_eq!(run(&circle(5.0), 0.0, 1.0, &n).unwrap().segments(), 10);
        let mut n = need(1.0, 2);
        n.edge_length = Some(0.5);
        assert_eq!(run(&circle(5.0), 0.0, 1.0, &n).unwrap().segments(), 10);
    }

    #[test]
    fn ellipse_counts_hold_the_chord() {
        for (x, y) in [(5.0, 2.0), (2.0, 5.0), (30.0, 1.0)] {
            let curve = Curve::Ellipse(
                Ellipse::new(
                    frame(),
                    Length::from_mm(x),
                    Length::from_mm(y),
                    Sense::Forward,
                )
                .unwrap(),
            );
            for c in [1e-3, 0.01] {
                let s = run(&curve, 0.0, TWO_PI, &need(c, 3)).unwrap();
                let gap = sampled_gap(&curve, 0.0, TWO_PI, &s, 64);
                assert!(gap <= s.chord().mm() * (1.0 + 1e-9), "{x}x{y} c {c}: {gap}");
                assert!(s.chord().mm() <= c);
                // The bound is the circle of the major radius: tight within the sampling.
                assert!(gap >= 0.5 * s.chord().mm(), "{x}x{y} c {c}: {gap} slack");
            }
        }
    }

    fn rational_circle(r: f64) -> Curve {
        let k = vec![0.0, 0.0, 0.0, 1.0, 1.0, 2.0, 2.0, 3.0, 3.0, 4.0, 4.0, 4.0];
        let p = [
            [r, 0.0],
            [r, r],
            [0.0, r],
            [-r, r],
            [-r, 0.0],
            [-r, -r],
            [0.0, -r],
            [r, -r],
            [r, 0.0],
        ]
        .map(|[x, y]| [x, y, 0.0])
        .to_vec();
        let w = vec![1.0, R2, 1.0, R2, 1.0, R2, 1.0, R2, 1.0];
        Curve::Nurbs(Arc::new(
            NurbsCurve::new(KnotVector::new(2, k).unwrap(), p, Some(w)).unwrap(),
        ))
    }

    #[test]
    fn a_rational_circle_holds_the_chord_against_the_exact_circle() {
        for r in [1.0, 10.0, 100.0] {
            for c in [1e-3, 0.01] {
                let curve = rational_circle(r);
                let s = run(&curve, 0.0, 4.0, &need(c, 3)).unwrap();
                assert!(s.chord().mm() <= c);
                let gap = circle_gap(Point3::ORIGIN, r, &polyline(&curve, 0.0, 4.0, &s));
                assert!(gap <= s.chord().mm() * (1.0 + 1e-9), "r {r} c {c}: {gap}");
                let exact = run(&circle(r), 0.0, TWO_PI, &need(c, 3)).unwrap();
                // The enclosure's overhead against the closed form: `|C''|` holds the
                // parametrisation's own acceleration, not only the curvature (1.2 to 2.1
                // measured, 72 §14.15).
                let ratio = s.segments() as f64 / exact.segments() as f64;
                assert!(
                    ratio < 2.5,
                    "r {r} c {c}: {} vs {}",
                    s.segments(),
                    exact.segments()
                );
            }
        }
    }

    fn cubic() -> Curve {
        let k = vec![0.0, 0.0, 0.0, 0.0, 0.4, 1.0, 1.0, 1.0, 1.0];
        let p = vec![
            [0.0, 0.0, 0.0],
            [3.0, 5.0, 1.0],
            [6.0, -4.0, 2.0],
            [9.0, 2.0, -1.0],
            [12.0, 0.0, 0.0],
        ];
        Curve::Nurbs(Arc::new(
            NurbsCurve::new(KnotVector::new(3, k).unwrap(), p, None).unwrap(),
        ))
    }

    #[test]
    fn a_cubic_holds_the_chord_by_dense_sampling() {
        let curve = cubic();
        for c in [1e-3, 0.01, 0.1] {
            let s = run(&curve, 0.0, 1.0, &need(c, 2)).unwrap();
            assert!(s.chord().mm() <= c);
            let gap = sampled_gap(&curve, 0.0, 1.0, &s, 64);
            assert!(gap <= s.chord().mm() * (1.0 + 1e-9), "c {c}: {gap}");
            // The interior knot is a sample.
            assert!(s.params().contains(&0.4));
            assert!(s.params().windows(2).all(|w| w[0] < w[1]));
        }
    }

    #[test]
    fn a_stationary_nurbs_end_bounds_the_chord_but_not_a_turn() {
        // A cubic whose speed vanishes at its start: the chord's `M2` needs no speed, and the
        // quarter-turn floor holds only where the speed is bounded away from zero.
        let p = vec![
            [0.0, 0.0, 0.0],
            [0.0, 0.0, 0.0],
            [1.0, 0.0, 0.0],
            [1.0, 1.0, 0.0],
        ];
        let k = vec![0.0, 0.0, 0.0, 0.0, 1.0, 1.0, 1.0, 1.0];
        let curve = Curve::Nurbs(Arc::new(
            NurbsCurve::new(KnotVector::new(3, k).unwrap(), p, None).unwrap(),
        ));
        let s = run(&curve, 0.0, 1.0, &need(0.01, 2)).unwrap();
        assert!(sampled_gap(&curve, 0.0, 1.0, &s, 64) <= s.chord().mm() * (1.0 + 1e-9));
        // A turning bound asks for the speed, which may vanish at the start: no bound.
        let mut n = need(0.01, 2);
        n.angle = Some(0.2);
        let e = run(&curve, 0.0, 1.0, &n).unwrap_err();
        assert_eq!(
            e,
            TessError::Unsupported(TessUnsupported::NoBound { edge: id() })
        );
    }

    #[test]
    fn a_count_past_the_index_space_is_refused() {
        let e = run(&circle(1e12), 0.0, TWO_PI, &need(1e-9, 3)).unwrap_err();
        assert!(
            matches!(e, TessError::Refused(TessRefusal::TooFine { .. })),
            "{e}"
        );
    }

    #[test]
    fn a_parabola_is_unsupported() {
        let curve =
            Curve::Parabola(Parabola::new(frame(), Length::from_mm(1.0), Sense::Forward).unwrap());
        let e = run(&curve, 0.0, 1.0, &need(0.01, 2)).unwrap_err();
        assert_eq!(
            e,
            TessError::Unsupported(TessUnsupported::EdgeKind {
                edge: id(),
                kind: CurveKind::Parabola
            })
        );
    }

    #[test]
    fn a_line_is_one_segment_unless_a_length_asks() {
        let curve =
            Curve::Line(scx_curve::Line::new(Point3::ORIGIN, Vec3::new(2.0, 0.0, 0.0)).unwrap());
        let s = run(&curve, 0.0, 5.0, &need(0.01, 2)).unwrap();
        assert_eq!((s.segments(), s.chord()), (1, Length::ZERO));
        let mut n = need(0.01, 2);
        n.edge_length = Some(3.0);
        assert_eq!(run(&curve, 0.0, 5.0, &n).unwrap().segments(), 4);
        let mut n = need(0.01, 2);
        n.face_len = 2.5;
        assert_eq!(run(&curve, 0.0, 5.0, &n).unwrap().segments(), 4);
    }

    #[test]
    fn the_budget_is_charged_per_segment_and_per_enclosure() {
        let mut b = Budget::new(1000);
        let s = samples(id(), &circle(10.0), 0.0, TWO_PI, &need(0.01, 3), &mut b).unwrap();
        assert_eq!(b.used(), s.segments() as u64);
        let mut b = Budget::new(5);
        let e = samples(id(), &circle(10.0), 0.0, TWO_PI, &need(0.01, 3), &mut b).unwrap_err();
        assert_eq!(e, TessError::Exhausted { limit: 5 });
        let mut b = Budget::new(3);
        let e = samples(id(), &cubic(), 0.0, 1.0, &need(1e-3, 2), &mut b).unwrap_err();
        assert_eq!(e, TessError::Exhausted { limit: 3 });
    }

    #[test]
    fn a_face_need_is_the_same_in_either_order() {
        use scx_surface::{Plane, Sphere, Torus};
        let spec = TessSpec::new(Length::from_mm(0.01), scx_num::units::Resolution::DEFAULT)
            .unwrap()
            .with_angle(0.3)
            .unwrap();
        let surfaces = [
            Surface::Plane(Plane::new(frame())),
            Surface::Sphere(Sphere::new(frame(), Length::from_mm(4.0), Sense::Forward).unwrap()),
            Surface::Torus(
                Torus::new(
                    frame(),
                    Length::from_mm(10.0),
                    Length::from_mm(3.0),
                    Sense::Forward,
                )
                .unwrap(),
            ),
        ];
        let fold = |order: &[usize]| {
            order
                .iter()
                .map(|&i| face_need(&surfaces[i], &spec).unwrap())
                .fold(f64::INFINITY, f64::min)
        };
        let all = fold(&[0, 1, 2]);
        for order in [[0, 2, 1], [1, 0, 2], [1, 2, 0], [2, 0, 1], [2, 1, 0]] {
            assert_eq!(fold(&order).to_bits(), all.to_bits());
        }
        // The torus's meridian bounds it: 1/r = 1/3 against 1/(R - r) = 1/7 and the sphere's
        // 1/4; the angle bound 0.3 * 3 = 0.9 is above 2 sqrt(0.01 (6 - 0.01)) = 0.49.
        assert_eq!(all, 0.8 * sqrt(3.0) * sqrt(0.01 * (2.0 * 3.0 - 0.01)));
    }
}
