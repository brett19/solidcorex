// Written for solidcorex. Apache-2.0.
//! The body generators (research 63 §6.4, BR8): every valid body has its closed-form
//! counts, breaks no invariant, finishes and runs its loops counter-clockwise from its
//! front; every broken body breaks its invariant alone; the same seed builds the same
//! body.

#![cfg(feature = "bodies")]
#![allow(clippy::unwrap_used, clippy::indexing_slicing, clippy::panic)]

use std::collections::BTreeSet;

use scx_brep::{Document, Draft, EdgeShape, UseGeom};
use scx_num::frame::Sense;
use scx_num::vector2d::Point2;
use scx_num::vector3d::Point3;
use scx_nurbs::Approach;
use scx_testkit::bodies::check::{Tolerance, violations};
use scx_testkit::bodies::{Counts, Invariant, Kind, broken, generate};

const SEEDS: [u64; 5] = [0, 1, 2, 7, 0x5eed];
const SIZES: [usize; 4] = [1, 2, 3, 5];

fn counted(d: &Draft) -> Counts {
    let loops: usize = d.faces().map(|f| f.loops().len()).sum();
    let uses: usize = d.faces().map(|f| f.uses().count()).sum();
    let collapsed = d
        .edges()
        .filter(|e| matches!(e.shape(), Some(EdgeShape::Collapsed)))
        .count();
    let (v, e, f) = (d.vertices().len(), d.edges().len(), d.faces().len());
    Counts {
        regions: d.regions().len(),
        shells: d.shells().len(),
        faces: f,
        loops,
        uses,
        edges: e,
        collapsed,
        vertices: v,
        euler: v as i64 - (e - collapsed) as i64 + f as i64 - (loops - f) as i64,
    }
}

#[test]
fn valid_bodies_hold_their_counts_and_every_invariant() {
    for kind in Kind::ALL {
        for seed in SEEDS {
            for size in SIZES {
                let g = generate(kind, seed, size).unwrap();
                let at = format!("{} seed {seed} size {size}", kind.name());
                assert_eq!(counted(&g.draft), g.counts, "{at}");
                assert_eq!(kind.faces(size), g.counts.faces, "{at}");
                let found = violations(&g.draft, Tolerance::default());
                assert!(found.is_empty(), "{at}: {found:?}");
                g.draft.finish(&Document::default()).unwrap();
            }
        }
    }
}

#[test]
fn euler_characteristics_are_the_topology() {
    let euler = |kind| generate(kind, 0, 2).unwrap().counts.euler;
    for kind in [
        Kind::Block,
        Kind::Prism,
        Kind::Cylinder,
        Kind::Cone,
        Kind::Sphere,
    ] {
        assert_eq!(euler(kind), 2, "{}", kind.name());
    }
    assert_eq!(euler(Kind::Torus), 0);
    assert_eq!(euler(Kind::Holed), 0);
    assert_eq!(euler(Kind::QuadSheet), 1);
    assert_eq!(euler(Kind::TriSheet), 1);
    assert_eq!(euler(Kind::Cavity), 4);
    // Three blocks: the complex of three cells.
    assert_eq!(euler(Kind::Stack), 4);
    // Five blocks, two blocks to each of four shared edges: 2 x 5 - 4.
    assert_eq!(euler(Kind::Checker), 6);
    assert_eq!(euler(Kind::Touching), 6);
}

/// The signed area of a face's outer loop in uv, its pcurves sampled over their edges'
/// intervals.
fn outer_area(d: &Draft, face: scx_brep::Face<'_, scx_brep::Drafting<'_>>) -> f64 {
    let mut pts: Vec<Point2> = Vec::new();
    let lp = face.loops().next().unwrap();
    for u in lp.uses() {
        let edge = u.edge().unwrap();
        let (lo, hi) = match edge.shape().unwrap() {
            EdgeShape::Curve { interval, .. } => (interval.min(), interval.max()),
            EdgeShape::Collapsed => (0.0, 1.0),
        };
        let UseGeom::Pcurve { curve, .. } = u.geom() else {
            panic!("a point use")
        };
        for k in 0..16 {
            let s = f64::from(k) / 16.0;
            let t = match u.sense() {
                Sense::Forward => lo + (hi - lo) * s,
                Sense::Reversed => hi - (hi - lo) * s,
            };
            pts.push(curve.point(t));
        }
    }
    let _ = d;
    let n = pts.len();
    (0..n)
        .map(|k| {
            let (a, b) = (pts[k], pts[(k + 1) % n]);
            a.x * b.y - b.x * a.y
        })
        .sum::<f64>()
        / 2.0
}

#[test]
fn outer_loops_run_counter_clockwise_from_the_front() {
    for kind in Kind::ALL {
        for seed in [0, 3] {
            for size in [1, 2, 4] {
                let d = generate(kind, seed, size).unwrap().draft;
                for f in d.faces() {
                    let area = outer_area(&d, f);
                    assert!(
                        area > 0.0,
                        "{} seed {seed} size {size} face {:?}: {area}",
                        kind.name(),
                        f.id()
                    );
                }
            }
        }
    }
}

#[test]
fn convex_bodies_face_outward() {
    for kind in [
        Kind::Block,
        Kind::Prism,
        Kind::Cylinder,
        Kind::Cone,
        Kind::Sphere,
    ] {
        for seed in [0, 3, 11] {
            for size in [1, 3] {
                let d = generate(kind, seed, size).unwrap().draft;
                let n = d.vertices().len() as f64;
                let c = d.vertices().fold(Point3::ORIGIN, |c, v| {
                    c + (v.point() - Point3::ORIGIN) * (1.0 / n)
                });
                for f in d.faces() {
                    // The middle of the outer loop's uv samples, inside a convex patch.
                    let lp = f.loops().next().unwrap();
                    let mut uv = (0.0, 0.0, 0.0);
                    for u in lp.uses() {
                        let edge = u.edge().unwrap();
                        let (lo, hi) = match edge.shape().unwrap() {
                            EdgeShape::Curve { interval, .. } => (interval.min(), interval.max()),
                            EdgeShape::Collapsed => (0.0, 1.0),
                        };
                        let UseGeom::Pcurve { curve, .. } = u.geom() else {
                            panic!()
                        };
                        let p = curve.point(0.5 * (lo + hi));
                        uv = (uv.0 + p.x, uv.1 + p.y, uv.2 + 1.0);
                    }
                    let mid = Point2::new(uv.0 / uv.2, uv.1 / uv.2);
                    let p = f.surface().point(mid);
                    let normal = f
                        .surface()
                        .normal(mid, [Approach::Above, Approach::Above])
                        .unwrap();
                    assert!(
                        (p - c).dot(*normal) > 0.0,
                        "{} seed {seed} size {size} face {:?}",
                        kind.name(),
                        f.id()
                    );
                }
            }
        }
    }
}

#[test]
fn each_broken_body_breaks_its_invariant_alone() {
    for inv in Invariant::ALL {
        for seed in [0, 1, 5] {
            for size in [1, 2, 3] {
                let Some(d) = broken(inv, seed, size).unwrap() else {
                    assert_eq!(inv, Invariant::UvGap);
                    continue;
                };
                let found = violations(&d, Tolerance::default());
                assert_eq!(
                    found,
                    BTreeSet::from([inv]),
                    "{inv:?} seed {seed} size {size}"
                );
            }
        }
    }
}

/// `finish`'s cause for each broken invariant (63 §6.7, §6.15). Shell openness and
/// misorientation show as the sides of neighbouring faces disagreeing; a period jump and a
/// pole gap as a loop open in uv. The broken radial order is one the sides cannot show
/// (the touching blocks' edge reversed), so the geometry's rule refuses it.
const FINISH_CAUSE: [(Invariant, &str); 17] = [
    (Invariant::OutsideVoid, "outside_region"),
    (Invariant::LiveReferences, "dangling_reference"),
    (Invariant::FinWithoutUse, "fin_without_use"),
    (Invariant::UseWithoutFin, "use_without_fin"),
    (Invariant::RepeatedFin, "repeated_fin"),
    (Invariant::SameSenseTwice, "repeated_use"),
    (Invariant::LoopChained, "loop_not_connected"),
    (Invariant::PeriodJump, "loop_open_in_uv"),
    (Invariant::PoleGap, "loop_open_in_uv"),
    (Invariant::UvGap, "loop_open_in_uv"),
    (Invariant::IntervalMismatch, "pcurve_interval"),
    (Invariant::PcurveOffEdge, "pcurve_off_edge"),
    (Invariant::VertexOffEdge, "edge_end_off_vertex"),
    (Invariant::ShellOpen, "sides_disagree"),
    (Invariant::Misoriented, "sides_disagree"),
    (Invariant::RadialOrder, "radial_order"),
    (Invariant::OrphanVertex, "loose_vertex"),
];

#[test]
fn finish_refuses_every_break_with_its_cause() {
    assert_eq!(FINISH_CAUSE.map(|(inv, _)| inv), Invariant::ALL);
    for (inv, name) in FINISH_CAUSE {
        for seed in 0..4 {
            for size in 1..5 {
                let Some(d) = broken(inv, seed, size).unwrap() else {
                    continue;
                };
                let refused = d.finish(&Document::default()).unwrap_err();
                assert_eq!(
                    refused.cause.name(),
                    name,
                    "{inv:?} seed {seed} size {size}"
                );
            }
        }
    }
}

#[test]
fn a_seed_builds_the_same_body() {
    for kind in Kind::ALL {
        let a = generate(kind, 9, 3).unwrap();
        let b = generate(kind, 9, 3).unwrap();
        assert_eq!(
            format!("{:?}", a.draft),
            format!("{:?}", b.draft),
            "{}",
            kind.name()
        );
        let points = |d: &Draft| {
            d.vertices()
                .map(scx_brep::Vertex::point)
                .collect::<Vec<_>>()
        };
        let c = generate(kind, 10, 3).unwrap();
        assert_ne!(points(&a.draft), points(&c.draft), "{}", kind.name());
    }
}

#[test]
fn seed_zero_is_canonical() {
    let g = generate(Kind::Block, 0, 1).unwrap();
    let mut corners: Vec<[f64; 3]> = g
        .draft
        .vertices()
        .map(|v| {
            let p = v.point();
            [p.x, p.y, p.z]
        })
        .collect();
    corners.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let mut want: Vec<[f64; 3]> = (0..8)
        .map(|i| [i & 1, (i >> 1) & 1, (i >> 2) & 1].map(f64::from))
        .collect();
    want.sort_by(|a, b| a.partial_cmp(b).unwrap());
    assert_eq!(corners, want);
}
