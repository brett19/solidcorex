// Written for solidcorex. Apache-2.0.
//! B-spline ends (research 75 §8.2, RP2), on constructed polyhedra whose straight edges are
//! NURBS curves (control points along the segment, non-uniform knots, rational or not), so
//! the healed forms are still closed forms. Each heal is held to §8.2's truth: the check
//! passes, no accuracy grew, the volume moved by at most the area times the largest bound,
//! the counts are the closed form's.
//!
//! - **Hip roof with NURBS hips.** RP1's hip roof (a square base `a x a`, a ridge `delta`
//!   at height `h` whose two vertices are within coincidence) with its four slanted edges
//!   NURBS: healed, the square pyramid, its hips still NURBS ending at the apex.
//! - **An empty piece.** One hip carries an interior Bezier piece between C0 joints whose
//!   control points are within `1e-8` of each other: cut, the hip's domain is that piece's
//!   length shorter, nothing else changes.

#![allow(
    missing_docs,
    clippy::pedantic,
    clippy::unwrap_used,
    clippy::panic,
    clippy::indexing_slicing
)]

use std::collections::BTreeMap;
use std::sync::Arc;

use scx_brep::op::{Cx, NoTrace, Scratch, Settings};
use scx_brep::{
    Brep, Document, Draft, EdgeGeom, EdgeId, EdgeShape, EdgeUse, FaceRecord, LoopRecord, RegionId,
    ShellRecord, UseGeom, VertexId, VertexRecord,
};
use scx_check::{Findings, Scope};
use scx_curve::{Curve, Line};
use scx_num::budget::Stop;
use scx_num::cert::{Basis, Recorded};
use scx_num::error::ErrorKind;
use scx_num::extent1d::Extent1d;
use scx_num::frame::{Frame, Sense};
use scx_num::units::{Accuracy, Length};
use scx_num::vector3d::{Point3, Vec3};
use scx_nurbs::{KnotVector, NurbsCurve};
use scx_repair::{HealCause, Item, OnItemFailure, RepairCause, RepairError, heal, items};
use scx_surface::{Plane, Surface, pcurve_of};

// ---------------------------------------------------------------------------------------
// Edges as NURBS segments.

/// How an edge from `p` to `q` is drawn.
#[derive(Clone, Copy, PartialEq, Debug)]
enum Draw {
    Line,
    /// Degree 3 on knots `0^4 .3 .7 1^4`, control points at uneven steps along the segment.
    Cubic,
    /// As `Cubic`, rational with uneven weights.
    Rational,
    /// Degree 2 with C0 joints at `.4` and `.6` and the piece between them within `1e-8`
    /// of a point.
    Empty,
}

fn segment(p: Point3, q: Point3, steps: &[f64]) -> Vec<[f64; 3]> {
    steps
        .iter()
        .map(|&s| {
            let x = p + (q - p) * s;
            [x.x, x.y, x.z]
        })
        .collect()
}

fn drawn(draw: Draw, p: Point3, q: Point3) -> Curve {
    let cubic = || KnotVector::new(3, vec![0., 0., 0., 0., 0.3, 0.7, 1., 1., 1., 1.]).unwrap();
    let steps = [0.0, 0.1, 0.35, 0.6, 0.85, 1.0];
    let n = match draw {
        Draw::Line => return Curve::Line(Line::new(p, q - p).unwrap()),
        Draw::Cubic => NurbsCurve::new(cubic(), segment(p, q, &steps), None),
        Draw::Rational => NurbsCurve::new(
            cubic(),
            segment(p, q, &steps),
            Some(vec![1.0, 0.8, 1.5, 0.7, 1.2, 1.0]),
        ),
        Draw::Empty => {
            let knots = vec![0., 0., 0., 0.4, 0.4, 0.6, 0.6, 1., 1., 1.];
            let steps = [0.0, 0.2, 0.5, 0.5 + 5e-10, 0.5 + 1e-9, 0.8, 1.0];
            NurbsCurve::new(
                KnotVector::new(2, knots).unwrap(),
                segment(p, q, &steps),
                None,
            )
        }
    };
    Curve::Nurbs(Arc::new(n.unwrap()))
}

fn domain(c: &Curve) -> Extent1d {
    match c {
        Curve::Nurbs(n) => n.domain(),
        _ => Extent1d::new(0.0, 1.0).unwrap(),
    }
}

// ---------------------------------------------------------------------------------------
// A polyhedron builder (RP1's, with each edge drawn as asked).

fn recorded(mm: f64) -> Option<Arc<Recorded>> {
    Some(Arc::new(Recorded {
        acc: Accuracy::new(Length::from_mm(mm)).unwrap(),
        basis: Basis::Measured,
    }))
}

fn newell(pts: &[Point3]) -> Vec3 {
    let mut n = Vec3::ZERO;
    for (k, p) in pts.iter().enumerate() {
        let q = pts[(k + 1) % pts.len()];
        n += Vec3::new(
            (p.y - q.y) * (p.z + q.z),
            (p.z - q.z) * (p.x + q.x),
            (p.x - q.x) * (p.y + q.y),
        );
    }
    n
}

#[derive(Default)]
struct Widen {
    vertices: BTreeMap<usize, f64>,
    edges: BTreeMap<(usize, usize), f64>,
    draws: BTreeMap<(usize, usize), Draw>,
}

fn polyhedron(pts: &[Point3], rings: &[Vec<usize>], widen: &Widen) -> Brep {
    let mut d = Draft::new();
    let outer = d.add_shell(ShellRecord::new(RegionId::OUTSIDE)).unwrap();
    let solid = d.add_region(true).unwrap();
    let inner = d.add_shell(ShellRecord::new(solid)).unwrap();
    let vs: Vec<VertexId> = pts
        .iter()
        .enumerate()
        .map(|(i, &p)| {
            let mut rec = VertexRecord::new(p);
            rec.accuracy = widen.vertices.get(&i).copied().and_then(recorded);
            d.add_vertex(rec).unwrap()
        })
        .collect();
    let mut edges: BTreeMap<(usize, usize), (EdgeId, Curve)> = BTreeMap::new();
    for ring in rings {
        for k in 0..ring.len() {
            let (a, b) = (ring[k], ring[(k + 1) % ring.len()]);
            let key = (a.min(b), a.max(b));
            edges.entry(key).or_insert_with(|| {
                let draw = widen.draws.get(&key).copied().unwrap_or(Draw::Line);
                let curve = drawn(draw, pts[key.0], pts[key.1]);
                let mut g = EdgeGeom::new(
                    EdgeShape::Curve {
                        curve: curve.clone(),
                        interval: domain(&curve),
                    },
                    [vs[key.0], vs[key.1]],
                );
                g.accuracy = widen.edges.get(&key).copied().and_then(recorded);
                (d.add_edge(g).unwrap(), curve)
            });
        }
    }
    for ring in rings {
        let rp: Vec<Point3> = ring.iter().map(|&i| pts[i]).collect();
        let frame = Frame::new(rp[0], newell(&rp), rp[1] - rp[0]).unwrap();
        let surface = Surface::Plane(Plane::new(frame));
        let mut uses = Vec::new();
        for k in 0..ring.len() {
            let (a, b) = (ring[k], ring[(k + 1) % ring.len()]);
            let key = (a.min(b), a.max(b));
            let (id, curve) = &edges[&key];
            let sense = if a < b {
                Sense::Forward
            } else {
                Sense::Reversed
            };
            let over = domain(curve);
            let c2 = pcurve_of(curve, &surface, over, Document::default().resolution()).unwrap();
            let mut u = EdgeUse::new(
                *id,
                sense,
                UseGeom::Pcurve {
                    curve: c2,
                    interval: over,
                },
            );
            u.bound = widen.edges.get(&key).copied().and_then(recorded);
            uses.push(u);
        }
        d.add_face(FaceRecord::new(
            surface,
            [outer, inner],
            vec![LoopRecord::new(uses)],
        ))
        .unwrap();
    }
    d.finish(&Document::default())
        .map_err(|u| u.cause)
        .unwrap()
        .0
}

// ---------------------------------------------------------------------------------------
// Running and measuring.

fn run_heal(
    b: &Brep,
    mode: OnItemFailure,
    budget: u64,
) -> Result<scx_brep::op::Done<HealCause>, RepairError> {
    let doc = Document::with_resolution(b.resolution());
    let settings = Settings::new().with_budget(budget);
    let mut scratch = Scratch::new();
    let stop = Stop::new();
    let mut cx = Cx::new(&doc, &settings, &mut scratch, &stop, &NoTrace);
    heal(b, mode, &mut cx)
}

fn check(b: &Brep) -> Findings {
    let doc = Document::with_resolution(b.resolution());
    let settings = Settings::new();
    let mut scratch = Scratch::new();
    let stop = Stop::new();
    let mut cx = Cx::new(&doc, &settings, &mut scratch, &stop, &NoTrace);
    scx_check::check(b, &mut cx, Scope::Import).unwrap()
}

// scx-mass cannot measure these bodies yet: a face bounded by NURBS pcurves has no box (the
// interval jet of a NURBS pcurve is too wide for the boundary's area sign to show, so
// `Face::bounding_box` is `None` and scx-mass refuses with `NoBox`). The faces are planar,
// so each face's vector area is half the sum of `x × dx` round its loop, taken on a dense
// polyline of each edge's curve, and the volume a third of the sum of `n · x0` times it.
// For a straight NURBS segment the polyline is the curve; healed, a hip bends by under
// its bound, and the polyline's error is that times `1 / SAMPLES^2`.

const SAMPLES: usize = 256;

/// Each face's point on it and its vector area (the outward normal times the area).
fn face_areas(b: &Brep) -> Vec<(Point3, Vec3)> {
    b.faces()
        .map(|f| {
            let mut pts = Vec::new();
            for u in f.uses() {
                let EdgeShape::Curve { curve, interval } = u.edge().shape() else {
                    panic!("a collapsed edge");
                };
                let (lo, hi) = (interval.min(), interval.max());
                let mut ts: Vec<f64> = (0..SAMPLES)
                    .map(|i| lo + (hi - lo) * i as f64 / SAMPLES as f64)
                    .collect();
                if u.sense() == Sense::Reversed {
                    ts = (1..=SAMPLES)
                        .map(|i| lo + (hi - lo) * i as f64 / SAMPLES as f64)
                        .rev()
                        .collect();
                }
                pts.extend(ts.into_iter().map(|t| curve.point(t)));
            }
            let mut twice = Vec3::ZERO;
            for (k, p) in pts.iter().enumerate() {
                let q = pts[(k + 1) % pts.len()];
                twice += (*p - Point3::ORIGIN).cross(q - Point3::ORIGIN);
            }
            (pts[0], twice * 0.5)
        })
        .collect()
}

fn volume(b: &Brep) -> f64 {
    face_areas(b)
        .iter()
        .map(|(p, a)| (*p - Point3::ORIGIN).dot(*a) / 3.0)
        .sum()
}

fn area(b: &Brep) -> f64 {
    face_areas(b).iter().map(|(_, a)| a.length()).sum()
}

fn largest_bound(b: &Brep) -> f64 {
    let zone = b.resolution().zone().mm();
    let acc = |r: Option<&Recorded>| r.map_or(zone, |r| r.acc.length().mm());
    let v = b.vertices().map(|v| acc(v.accuracy())).fold(zone, f64::max);
    let e = b.edges().map(|e| acc(e.accuracy())).fold(zone, f64::max);
    let u = b
        .faces()
        .flat_map(|f| f.uses().map(|u| acc(u.bound())).collect::<Vec<_>>())
        .fold(zone, f64::max);
    v.max(e).max(u)
}

fn no_accuracy_grew(before: &Brep, after: &Brep) {
    let mm = |r: Option<&Recorded>| r.map(|r| r.acc.length().mm());
    for v in after.vertices() {
        let was = before.vertex(v.id()).unwrap();
        assert_eq!(mm(v.accuracy()), mm(was.accuracy()), "{}", v.id());
    }
    for e in after.edges() {
        let was = before.edge(e.id()).unwrap();
        assert_eq!(mm(e.accuracy()), mm(was.accuracy()), "{}", e.id());
    }
}

fn counts(b: &Brep) -> (usize, usize, usize) {
    (b.vertices().len(), b.edges().len(), b.faces().len())
}

fn nurbs_of(b: &Brep, e: EdgeId) -> Arc<NurbsCurve<3>> {
    match b.edge(e).unwrap().shape() {
        EdgeShape::Curve {
            curve: Curve::Nurbs(n),
            interval,
        } => {
            assert_eq!(*interval, n.domain());
            Arc::clone(n)
        }
        other => panic!("{other:?}"),
    }
}

// ---------------------------------------------------------------------------------------
// The recipes.

const A: f64 = 10.0;
const H: f64 = 5.0;
const DELTA: f64 = 1e-4;
const HIPS: [(usize, usize); 4] = [(0, 4), (3, 4), (1, 5), (2, 5)];

/// The hip roof with ridge `delta`, the ridge's vertices of accuracy `vacc`, the hips (and
/// their uses) `eacc`, the hips drawn `hips`, the ridge `ridge`, the hip `(0, 4)` `first`.
fn hip_roof(delta: f64, vacc: f64, eacc: f64, hips: Draw, ridge: Draw, first: Draw) -> Brep {
    let pts = [
        Point3::new(0.0, 0.0, 0.0),
        Point3::new(A, 0.0, 0.0),
        Point3::new(A, A, 0.0),
        Point3::new(0.0, A, 0.0),
        Point3::new(0.5 * (A - delta), 0.5 * A, H),
        Point3::new(0.5 * (A + delta), 0.5 * A, H),
    ];
    let rings = vec![
        vec![0, 3, 2, 1],
        vec![0, 1, 5, 4],
        vec![1, 2, 5],
        vec![2, 3, 4, 5],
        vec![3, 0, 4],
    ];
    let mut w = Widen::default();
    w.vertices.insert(4, vacc);
    w.vertices.insert(5, vacc);
    for e in HIPS {
        w.edges.insert(e, eacc);
        w.draws.insert(e, hips);
    }
    w.draws.insert((0, 4), first);
    w.draws.insert((4, 5), ridge);
    polyhedron(&pts, &rings, &w)
}

/// The hip `(0, 4)`: the edge from the first base corner to the ridge.
fn first_hip(b: &Brep) -> EdgeId {
    let v0 = b.vertices().next().unwrap().id();
    let apex = |e: &scx_brep::Edge<'_>| b.vertex(e.ends()[1]).unwrap().point().z > 0.0;
    b.vertex(v0)
        .unwrap()
        .edges()
        .find(|e| apex(e))
        .map(|e| e.id())
        .unwrap()
}

/// Heals `b` under every mode to the pyramid: the check passes, no accuracy grew, the
/// volume is the closed form's within the area times the largest bound, the hips still
/// NURBS ending at the apex.
fn heals_to_the_pyramid(b: &Brep) {
    let v0 = volume(b);
    for mode in [
        OnItemFailure::Stop,
        OnItemFailure::Collect,
        OnItemFailure::Skip,
    ] {
        let done = run_heal(b, mode, u64::MAX).unwrap();
        let h = &done.body;
        assert!(done.report.is_clean());
        assert_eq!(counts(h), (5, 8, 5));
        let after = check(h);
        assert!(after.is_clean(), "{:?} {:?}", after.list, after.undecided);
        no_accuracy_grew(b, h);
        let apex = Point3::new(0.5 * A, 0.5 * A, H);
        let mut hips = 0;
        for e in h.edges() {
            if let EdgeShape::Curve {
                curve: Curve::Nurbs(n),
                ..
            } = e.shape()
            {
                hips += 1;
                let ends = [n.points()[0], *n.points().last().unwrap()];
                let at = |&[x, y, z]: &[f64; 3]| (Point3::new(x, y, z) - apex).length() <= 1e-12;
                assert!(ends.iter().any(at));
            }
        }
        assert_eq!(hips, 4);
        let v1 = volume(h);
        let bound = area(b) * largest_bound(b);
        assert!((v1 - A * A * H / 3.0).abs() <= bound, "{v1}");
        assert!((v1 - v0).abs() <= bound, "{v0} {v1} {bound}");
        assert!(items(h).is_empty());
        // Deterministic: the same body twice.
        let again = run_heal(b, mode, u64::MAX).unwrap();
        assert_eq!(counts(&again.body), counts(h));
        for e in h.edges() {
            assert_eq!(again.body.edge(e.id()).unwrap().shape(), e.shape());
        }
    }
}

// ---------------------------------------------------------------------------------------
// The truths.

#[test]
fn a_hip_roof_with_cubic_hips_heals_to_the_pyramid() {
    let b = hip_roof(
        DELTA,
        2.0 * DELTA,
        DELTA,
        Draw::Cubic,
        Draw::Line,
        Draw::Cubic,
    );
    assert_eq!(check(&b).list.len(), 1);
    let its = items(&b);
    assert_eq!(its.len(), 1);
    assert!(matches!(its[0], Item::Cluster(_)));
    heals_to_the_pyramid(&b);
}

#[test]
fn a_hip_roof_with_rational_hips_heals_to_the_pyramid() {
    let b = hip_roof(
        DELTA,
        2.0 * DELTA,
        DELTA,
        Draw::Rational,
        Draw::Line,
        Draw::Rational,
    );
    heals_to_the_pyramid(&b);
}

#[test]
fn a_nurbs_ridge_is_squeezed() {
    let b = hip_roof(
        DELTA,
        2.0 * DELTA,
        DELTA,
        Draw::Cubic,
        Draw::Cubic,
        Draw::Cubic,
    );
    heals_to_the_pyramid(&b);
}

#[test]
fn a_nurbs_hip_needing_more_accuracy_is_refused() {
    // As RP1's lines: the re-ended hips are off the end faces by about 0.35 delta.
    let b = hip_roof(
        DELTA,
        2.0 * DELTA,
        1e-9,
        Draw::Cubic,
        Draw::Line,
        Draw::Cubic,
    );
    let err = run_heal(&b, OnItemFailure::Stop, u64::MAX).unwrap_err();
    let RepairError::Cause(RepairCause::Heal(cause)) = &err else {
        panic!("{err:?}")
    };
    assert!(matches!(cause, HealCause::OffFace { .. }), "{cause:?}");
    assert_eq!(err.kind(), ErrorKind::Refused);
    let done = run_heal(&b, OnItemFailure::Skip, u64::MAX).unwrap();
    assert_eq!(done.report.skipped.len(), 1);
    assert_eq!(counts(&done.body), counts(&b));
}

#[test]
fn an_empty_piece_is_cut() {
    let b = hip_roof(1.0, DELTA, DELTA, Draw::Cubic, Draw::Line, Draw::Empty);
    assert!(check(&b).is_clean());
    let e = first_hip(&b);
    let its = items(&b);
    assert_eq!(its.len(), 1);
    let Item::Pieces(p) = &its[0] else {
        panic!("{its:?}")
    };
    assert_eq!(p.edge, e);
    assert_eq!(p.pieces, vec![Extent1d::new(0.4, 0.6).unwrap()]);
    let v0 = volume(&b);
    let done = run_heal(&b, OnItemFailure::Stop, u64::MAX).unwrap();
    let h = &done.body;
    assert_eq!(counts(h), counts(&b));
    let after = check(h);
    assert!(after.is_clean(), "{:?} {:?}", after.list, after.undecided);
    no_accuracy_grew(&b, h);
    let n = nurbs_of(h, e);
    assert_eq!(n.domain().min(), 0.0);
    assert!((n.domain().max() - 0.8).abs() <= 1e-15);
    assert_eq!(n.points().len(), 5);
    let bound = area(&b) * largest_bound(&b);
    assert!((volume(h) - v0).abs() <= bound);
    assert!(items(h).is_empty());
}

#[test]
fn an_empty_piece_and_a_cluster_on_one_hip() {
    let b = hip_roof(
        DELTA,
        2.0 * DELTA,
        DELTA,
        Draw::Cubic,
        Draw::Line,
        Draw::Empty,
    );
    let its = items(&b);
    assert_eq!(its.len(), 2);
    assert!(matches!(its[0], Item::Pieces(_)));
    assert!(matches!(its[1], Item::Cluster(_)));
    let e = first_hip(&b);
    heals_to_the_pyramid(&b);
    let done = run_heal(&b, OnItemFailure::Stop, u64::MAX).unwrap();
    assert!((nurbs_of(&done.body, e).domain().max() - 0.8).abs() <= 1e-15);
}

#[test]
fn an_empty_piece_past_the_budget_is_exhausted() {
    let b = hip_roof(1.0, DELTA, DELTA, Draw::Cubic, Draw::Line, Draw::Empty);
    let full = run_heal(&b, OnItemFailure::Stop, u64::MAX)
        .unwrap()
        .report
        .work;
    let err = run_heal(&b, OnItemFailure::Stop, full - 1).unwrap_err();
    assert!(
        matches!(err, RepairError::Exhausted { item: 0, .. }),
        "{err:?}"
    );
    assert!(run_heal(&b, OnItemFailure::Stop, full).is_ok());
}
