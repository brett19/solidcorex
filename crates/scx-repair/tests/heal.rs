// Written for solidcorex. Apache-2.0.
//! The certified heal of short-edge clusters (research 75 §8.2, RP1), on constructed
//! defective polyhedra whose healed forms are closed forms. Each heal is held to the truth
//! of §8.2: the check passes (or reports only what was left), no accuracy grew, the volume
//! moved by at most the area times the largest bound, and the counts are the closed form's.
//!
//! - **Hip roof.** A square base `a x a` and a ridge of length `delta` at height `h`, its two
//!   vertices within coincidence. Healed, it is the square pyramid: 5 vertices, 8 edges,
//!   5 faces. Its volume was `a h (2a + delta) / 6` and the pyramid's is `a^2 h / 3`.
//! - **Chamfered prism.** A square prism with corners cut by a chamfer of length `delta`;
//!   each chamfer's bottom and top pairs are clusters. A bottom pair heals (the chamfer wall
//!   becomes a triangle); the top pair then would leave a two-line wall, a sliver, which is
//!   a later heal's item: it fails `Unsupported`, and the three failure modes say so.

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
use scx_check::{Finding, Findings, Scope};
use scx_curve::{Curve, Line};
use scx_mass::RelAccuracy;
use scx_num::budget::{Budget, Stop};
use scx_num::cert::{Basis, Recorded};
use scx_num::error::ErrorKind;
use scx_num::extent1d::Extent1d;
use scx_num::frame::{Frame, Sense};
use scx_num::units::{Accuracy, Length};
use scx_num::vector3d::{Point3, Vec3};
use scx_repair::{HealCause, Item, OnItemFailure, RepairCause, RepairError, heal, items};
use scx_surface::{Plane, Surface, pcurve_of};

// ---------------------------------------------------------------------------------------
// A polyhedron builder: straight edges over [0, 1], planar faces through their rings.

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

/// The accuracies a recipe gives: per vertex index, per edge (index pair, lower first), and
/// to every use of those edges.
#[derive(Default)]
struct Widen {
    vertices: BTreeMap<usize, f64>,
    edges: BTreeMap<(usize, usize), f64>,
}

/// The solid bounded by `rings` (each counter-clockwise seen from outside) through `pts`.
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
    let unit = Extent1d::new(0.0, 1.0).unwrap();
    let mut edges: BTreeMap<(usize, usize), (EdgeId, Curve)> = BTreeMap::new();
    for ring in rings {
        for k in 0..ring.len() {
            let (a, b) = (ring[k], ring[(k + 1) % ring.len()]);
            let key = (a.min(b), a.max(b));
            edges.entry(key).or_insert_with(|| {
                let (p, q) = (pts[key.0], pts[key.1]);
                let curve = Curve::Line(Line::new(p, q - p).unwrap());
                let mut g = EdgeGeom::new(
                    EdgeShape::Curve {
                        curve: curve.clone(),
                        interval: unit,
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
            let c2 = pcurve_of(curve, &surface, unit, Document::default().resolution()).unwrap();
            let mut u = EdgeUse::new(
                *id,
                sense,
                UseGeom::Pcurve {
                    curve: c2,
                    interval: unit,
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

fn volume(b: &Brep) -> f64 {
    scx_mass::volume(
        b,
        RelAccuracy::new(1e-12).unwrap(),
        &mut Scratch::new(),
        &mut Budget::unlimited(),
        &Stop::new(),
    )
    .unwrap()
    .value
}

fn area(b: &Brep) -> f64 {
    scx_mass::area(
        b,
        RelAccuracy::new(1e-12).unwrap(),
        &mut Scratch::new(),
        &mut Budget::unlimited(),
        &Stop::new(),
    )
    .unwrap()
    .value
}

/// The largest recorded accuracy or use bound of `b`.
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

/// Every vertex's and edge's accuracy in `after` is at most what the entity had in
/// `before` (the survivor keeps its own; a rebuilt edge keeps its own).
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

fn coincident_pairs(f: &Findings) -> usize {
    f.list
        .iter()
        .filter(|x| matches!(x, Finding::VerticesWithinCoincidence { .. }))
        .count()
}

// ---------------------------------------------------------------------------------------
// The recipes.

const A: f64 = 10.0;
const H: f64 = 5.0;

/// The hip roof with ridge `delta`; the ridge's vertices get `vacc`, the four slanted
/// edges (and their uses) `eacc`.
fn hip_roof(delta: f64, vacc: f64, eacc: f64) -> Brep {
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
    for e in [(0, 4), (3, 4), (1, 5), (2, 5)] {
        w.edges.insert(e, eacc);
    }
    polyhedron(&pts, &rings, &w)
}

/// A square prism `A x A x H` with the corners at `(A, A)` and, if `both`, `(A, 0)` cut
/// by chamfers of length `delta`. The chamfer vertices get `2 delta`, every edge at them
/// (and its uses) `delta / 2`: under the chamfer's length, which finish requires.
fn chamfered(delta: f64, both: bool) -> Brep {
    let c = delta / 2f64.sqrt();
    let mut profile = vec![(0.0, 0.0)];
    let mut cut = Vec::new();
    if both {
        cut.extend([profile.len(), profile.len() + 1]);
        profile.extend([(A - c, 0.0), (A, c)]);
    } else {
        profile.push((A, 0.0));
    }
    cut.extend([profile.len(), profile.len() + 1]);
    profile.extend([(A, A - c), (A - c, A), (0.0, A)]);
    let n = profile.len();
    let mut pts: Vec<Point3> = profile
        .iter()
        .map(|&(x, y)| Point3::new(x, y, 0.0))
        .collect();
    pts.extend(profile.iter().map(|&(x, y)| Point3::new(x, y, H)));
    let mut rings = vec![(0..n).rev().collect::<Vec<_>>(), (n..2 * n).collect()];
    for i in 0..n {
        let j = (i + 1) % n;
        rings.push(vec![i, j, j + n, i + n]);
    }
    let mut w = Widen::default();
    for &i in &cut {
        for v in [i, i + n] {
            w.vertices.insert(v, 2.0 * delta);
        }
    }
    for ring in &rings {
        for k in 0..ring.len() {
            let (a, b) = (ring[k], ring[(k + 1) % ring.len()]);
            if w.vertices.contains_key(&a) || w.vertices.contains_key(&b) {
                w.edges.insert((a.min(b), a.max(b)), 0.5 * delta);
            }
        }
    }
    polyhedron(&pts, &rings, &w)
}

const DELTA: f64 = 1e-4;

// ---------------------------------------------------------------------------------------
// The truths.

#[test]
fn a_hip_roof_heals_to_the_pyramid() {
    let b = hip_roof(DELTA, 2.0 * DELTA, DELTA);
    let before = check(&b);
    assert_eq!(coincident_pairs(&before), 1, "{:?}", before.list);
    assert_eq!(before.list.len(), 1);
    assert_eq!(counts(&b), (6, 9, 5));
    let v0 = volume(&b);
    let truth0 = A * H * (2.0 * A + DELTA) / 6.0;
    assert!((v0 - truth0).abs() <= 1e-9 * truth0, "{v0} {truth0}");

    let its = items(&b);
    assert_eq!(its.len(), 1);
    let Item::Cluster(c) = &its[0] else {
        panic!("one cluster")
    };
    assert_eq!(c.vertices.len(), 2);
    assert_eq!(c.edges.len(), 1);
    assert!((c.target - Point3::new(0.5 * A, 0.5 * A, H)).length() <= 1e-12);

    for mode in [
        OnItemFailure::Stop,
        OnItemFailure::Collect,
        OnItemFailure::Skip,
    ] {
        let done = run_heal(&b, mode, u64::MAX).unwrap();
        let h = &done.body;
        assert!(done.report.is_clean());
        assert!(done.report.work > 0);
        assert_eq!(counts(h), (5, 8, 5));
        let after = check(h);
        assert!(after.is_clean(), "{:?} {:?}", after.list, after.undecided);
        no_accuracy_grew(&b, h);
        assert_eq!(h.vertex(c.survivor).unwrap().point(), c.target);
        // The healed solid against the pyramid's closed form, and the change against the
        // area times the largest bound.
        let v1 = volume(h);
        let bound = area(&b) * largest_bound(&b);
        assert!((v1 - A * A * H / 3.0).abs() <= bound, "{v1}");
        assert!((v1 - v0).abs() <= bound, "{v0} {v1} {bound}");
        // Healing again finds nothing.
        assert!(items(h).is_empty());
    }
}

#[test]
fn a_ridge_past_the_accuracy_is_no_item() {
    let b = hip_roof(DELTA, DELTA / 10.0, DELTA);
    assert!(check(&b).is_clean());
    assert!(items(&b).is_empty());
    let done = run_heal(&b, OnItemFailure::Stop, u64::MAX).unwrap();
    assert_eq!(counts(&done.body), counts(&b));
    assert!(done.report.is_clean());
    assert!(check(&done.body).is_clean());
}

#[test]
fn a_cluster_needing_more_accuracy_is_refused() {
    // The re-ended slanted edges are off the end faces by about 0.35 delta: at zone
    // accuracy they cannot follow, and no accuracy is grown to let them.
    let b = hip_roof(DELTA, 2.0 * DELTA, 1e-9);
    let err = run_heal(&b, OnItemFailure::Stop, u64::MAX).unwrap_err();
    let RepairError::Cause(RepairCause::Heal(cause)) = &err else {
        panic!("{err:?}")
    };
    assert!(matches!(cause, HealCause::OffFace { .. }), "{cause:?}");
    assert_eq!(err.kind(), ErrorKind::Refused);
    assert_eq!(err.name(), "off_face");
}

#[test]
fn a_chamfer_heals_its_bottom_and_leaves_its_sliver_top() {
    let b = chamfered(DELTA, false);
    assert_eq!(coincident_pairs(&check(&b)), 2);
    let its = items(&b);
    assert_eq!(its.len(), 2);

    // Stop: the lowest failing item, no body.
    let err = run_heal(&b, OnItemFailure::Stop, u64::MAX).unwrap_err();
    assert!(
        matches!(
            err,
            RepairError::Cause(RepairCause::Heal(HealCause::Sliver { .. }))
        ),
        "{err:?}"
    );
    assert_eq!(err.kind(), ErrorKind::Unsupported);

    // Collect: every failing item, no body.
    let err = run_heal(&b, OnItemFailure::Collect, u64::MAX).unwrap_err();
    let RepairError::Items(failed) = &err else {
        panic!("{err:?}")
    };
    assert_eq!(failed.len(), 1);
    assert_eq!(failed[0].item, 1);
    assert_eq!(failed[0].round, 1);

    // Skip: the bottom healed, the top left as it was and reported.
    let done = run_heal(&b, OnItemFailure::Skip, u64::MAX).unwrap();
    assert_eq!(done.report.skipped.len(), 1);
    assert_eq!(done.report.skipped[0].item, 1);
    let h = &done.body;
    assert_eq!(
        counts(h),
        (counts(&b).0 - 1, counts(&b).1 - 1, counts(&b).2)
    );
    let after = check(h);
    assert_eq!(coincident_pairs(&after), 1);
    assert_eq!(after.list.len(), 1, "{:?}", after.list);
    assert!(after.undecided.is_empty(), "{:?}", after.undecided);
    no_accuracy_grew(&b, h);
    let bound = area(&b) * largest_bound(&b);
    assert!((volume(h) - volume(&b)).abs() <= bound);
    let truth = (A * A - DELTA * DELTA / 4.0) * H;
    assert!((volume(&b) - truth).abs() <= 1e-9 * truth);
}

#[test]
fn two_clusters_on_one_edge_certify_it_with_both_ends() {
    // The bottom side between the two chamfers is re-ended by both clusters; the later
    // one certifies its final line, both ends moved.
    let b = chamfered(DELTA, true);
    assert_eq!(items(&b).len(), 4);
    let done = run_heal(&b, OnItemFailure::Skip, u64::MAX).unwrap();
    let skipped: Vec<usize> = done.report.skipped.iter().map(|s| s.item).collect();
    assert_eq!(skipped.len(), 2, "{:?}", done.report.skipped);
    let h = &done.body;
    assert_eq!(
        counts(h),
        (counts(&b).0 - 2, counts(&b).1 - 2, counts(&b).2)
    );
    let after = check(h);
    assert_eq!(coincident_pairs(&after), 2);
    assert_eq!(after.list.len(), 2, "{:?}", after.list);
    assert!(after.undecided.is_empty(), "{:?}", after.undecided);
    no_accuracy_grew(&b, h);
    let bound = area(&b) * largest_bound(&b);
    assert!((volume(h) - volume(&b)).abs() <= bound);
}

#[test]
fn the_budget_and_stop_end_a_heal_typed() {
    let b = chamfered(DELTA, true);
    let full = run_heal(&b, OnItemFailure::Skip, u64::MAX)
        .unwrap()
        .report
        .work;
    let err = run_heal(&b, OnItemFailure::Skip, full - 1).unwrap_err();
    let RepairError::Exhausted { item, work } = err else {
        panic!("{err:?}")
    };
    assert!(item < 4 && work < full);
    assert_eq!(
        run_heal(&b, OnItemFailure::Skip, full).unwrap().report.work,
        full
    );

    let doc = Document::with_resolution(b.resolution());
    let settings = Settings::new();
    let mut scratch = Scratch::new();
    let stop = Stop::new();
    stop.set();
    let mut cx = Cx::new(&doc, &settings, &mut scratch, &stop, &NoTrace);
    assert_eq!(
        heal(&b, OnItemFailure::Stop, &mut cx).unwrap_err(),
        RepairError::Cancelled
    );
}

#[test]
fn a_body_of_another_document_is_refused() {
    let b = hip_roof(DELTA, 2.0 * DELTA, DELTA);
    let doc =
        Document::with_resolution(scx_num::units::Resolution::new(Length::from_mm(1e-7)).unwrap());
    let settings = Settings::new();
    let mut scratch = Scratch::new();
    let stop = Stop::new();
    let mut cx = Cx::new(&doc, &settings, &mut scratch, &stop, &NoTrace);
    assert_eq!(
        heal(&b, OnItemFailure::Stop, &mut cx).unwrap_err(),
        RepairError::Cause(RepairCause::OtherDocument)
    );
}

#[test]
fn heal_is_deterministic() {
    let b = chamfered(DELTA, true);
    let one = run_heal(&b, OnItemFailure::Skip, u64::MAX).unwrap();
    let two = run_heal(&b, OnItemFailure::Skip, u64::MAX).unwrap();
    assert_eq!(one.report, two.report);
    let pts = |h: &Brep| {
        h.vertices()
            .map(|v| (v.id(), v.point()))
            .collect::<Vec<_>>()
    };
    assert_eq!(pts(&one.body), pts(&two.body));
}
