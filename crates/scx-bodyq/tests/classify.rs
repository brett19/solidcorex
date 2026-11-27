// Written for solidcorex. Apache-2.0.
//! `classify_point` judged as outcomes against T10 (`scx_testkit::csg`) and against each
//! face's own side.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

mod common;

use scx_bodyq::{BodyqError, BodyqRefusal, PointClass, classify_point};
use scx_brep::{Brep, Document, Entity, FaceSide, RegionId, UvSide};
use scx_num::budget::{Budget, Stop};
use scx_num::vector3d::Point3;
use scx_nurbs::Approach;
use scx_testkit::bodies::{Kind, generate};
use scx_testkit::csg::{Csg, FeatureKind, ProbeSpec, Side, box_points};

/// The coincidence distance the probes are spaced by: the default resolution.
const C: f64 = 1e-5;

fn pt(p: [f64; 3]) -> Point3 {
    Point3::new(p[0], p[1], p[2])
}

fn class(body: &Brep, p: [f64; 3]) -> PointClass {
    classify_point(body, pt(p), &mut Budget::unlimited(), &Stop::new())
        .unwrap_or_else(|e| panic!("{p:?}: {e}"))
}

/// Whether `got` is consistent with T10 at `p`: a point on the boundary is within 1.5
/// coincidences of it, a point in a region is not certainly on the other side.
fn agrees(csg: &Csg, solid: RegionId, p: [f64; 3], got: PointClass) -> bool {
    match got {
        PointClass::On(_) => csg.side(p, 1.5 * C) == Side::Band,
        PointClass::In(r) if r == solid => csg.side(p, 1e-9) != Side::Out,
        PointClass::In(r) if r == RegionId::OUTSIDE => csg.side(p, 1e-9) != Side::In,
        _ => false,
    }
}

/// Seeded points through and around each primitive's box, in each frame.
#[test]
fn the_primitives_agree_with_t10() {
    for (name, body, csg) in common::primitives() {
        let solid = common::solid(&body);
        let (lo, hi) = csg.bounds().unwrap();
        let mut ins = 0;
        for p in box_points(lo, hi, 0.2, 1000, 5) {
            let got = class(&body, p);
            assert!(agrees(&csg, solid, p, got), "{name} {p:?}: {got:?}");
            ins += usize::from(got == PointClass::In(solid));
        }
        assert!(ins > 20, "{name}: {ins} inside");
    }
}

/// T10's probes: on each face, edge, vertex, seam and pole, and `1/2`, `2` and `10`
/// coincidences off along each normal and their sums. A point on a feature is `On`, a
/// point two coincidences clear of every feature is `In`, and every answer is consistent
/// with T10.
#[test]
fn the_probes_agree_with_t10() {
    let mut edges = 0;
    for (name, body, csg) in common::primitives() {
        let solid = common::solid(&body);
        let probes = csg.probes(&ProbeSpec {
            seed: 9,
            per_face: 6,
            coincidence: C,
            extent: 10.0,
        });
        let mut on = 0;
        for pr in &probes {
            let got = class(&body, pr.point);
            if pr.offset == 0.0 {
                assert!(
                    matches!(got, PointClass::On(_)),
                    "{name} {:?} {:?}: {got:?}",
                    pr.feature.kind,
                    pr.point
                );
                on += 1;
            }
            // Clear of every feature by two coincidences, the point is in a region. (A probe
            // off an edge along one face's normal can stay on the other face.)
            if csg.side(pr.point, 2.0 * C) != Side::Band {
                assert!(
                    matches!(got, PointClass::In(_)),
                    "{name} {:?} {:?} off {}: {got:?}",
                    pr.feature.kind,
                    pr.point,
                    pr.offset
                );
            }
            assert!(
                agrees(&csg, solid, pr.point, got),
                "{name} {:?} {:?} off {}: {got:?}",
                pr.feature.kind,
                pr.point,
                pr.offset
            );
        }
        assert!(on > 5, "{name}: {on} on");
        edges += probes
            .iter()
            .filter(|p| matches!(p.feature.kind, FeatureKind::Edge | FeatureKind::Vertex))
            .count();
    }
    assert!(edges > 100, "{edges} probes at edges and vertices");
}

/// The pin's `brep_query_cache_lifetime` fixed points, the in-material booleans only:
/// inside the bar, inside the bar near the post, inside the post above the bar, inside
/// both, beyond the bar's open end, and above the bar beside the post.
#[test]
fn the_bar_into_post_fixed_points() {
    let body = common::bar_into_post();
    let solid = common::solid(&body);
    let csg = common::bar_into_post_csg();
    let points = [
        [5.0, 4.0, 3.15],
        [20.0, 8.0, 5.0],
        [40.0, 8.0, 15.0],
        [44.0, 8.0, 5.0],
        [-1.0, 8.0, 5.0],
        [20.0, 8.0, 11.0],
    ];
    let got: Vec<bool> = points
        .iter()
        .map(|&p| class(&body, p) == PointClass::In(solid))
        .collect();
    assert_eq!(got, [true, true, true, true, false, false]);
    let (lo, hi) = csg.bounds().unwrap();
    for p in box_points(lo, hi, 0.1, 600, 2) {
        let got = class(&body, p);
        assert!(agrees(&csg, solid, p, got), "{p:?}: {got:?}");
    }
    // On the rulings where the bar's sides touch the post; in the material where the
    // bar's top would run inside the post's wall.
    for p in [[40.0, 0.0, 5.0], [40.0, 16.0, 5.0], [36.0, 8.0, 10.0]] {
        let got = class(&body, p);
        assert!(agrees(&csg, solid, p, got), "{p:?}: {got:?}");
    }
    assert!(matches!(class(&body, [40.0, 0.0, 5.0]), PointClass::On(_)));
    assert_eq!(class(&body, [36.0, 8.0, 10.0]), PointClass::In(solid));
}

/// The pin's `query/brep.rs` unit-box interior and exterior case.
#[test]
fn the_unit_box_inside_and_out() {
    let body = common::unit_cube();
    assert_eq!(
        class(&body, [0.5, 0.5, 0.5]),
        PointClass::In(common::solid(&body))
    );
    assert_eq!(
        class(&body, [2.0, 2.0, 2.0]),
        PointClass::In(RegionId::OUTSIDE)
    );
}

/// Every testkit kind: just off the middle of each face (where the face's own foot is
/// the point's nearest boundary), each side is the region the face's shell there bounds.
#[test]
fn every_kind_reads_each_face_side() {
    let doc = Document::default();
    let mut checked = 0;
    for kind in Kind::ALL {
        for seed in [0, 1, 2] {
            for size in [1, 2] {
                let g = generate(kind, seed, size).unwrap();
                let body = g.draft.finish(&doc).map_err(|u| u.cause).unwrap().0;
                for face in body.faces() {
                    let Some(uvb) = face.uv_box() else { continue };
                    let uv = uvb.mid();
                    if face.contains_uv(uv) != Some(UvSide::Inside) {
                        continue;
                    }
                    let s = face.surface();
                    let q = s.point(uv);
                    let Some(n) = s.normal(uv, [Approach::Above; 2]) else {
                        continue;
                    };
                    // Only where the face is the point's own: q is on it alone.
                    if class(&body, [q.x, q.y, q.z]) != PointClass::On(Entity::Face(face.id())) {
                        continue;
                    }
                    let d = 1e-3;
                    for (side, k) in [(FaceSide::Front, d), (FaceSide::Back, -d)] {
                        let p = q + n.as_vec() * k;
                        let want = PointClass::In(face.shell(side).region().id());
                        assert_eq!(
                            class(&body, [p.x, p.y, p.z]),
                            want,
                            "{kind:?} seed {seed} size {size} face {:?} {side:?}",
                            face.id()
                        );
                    }
                    checked += 1;
                }
            }
        }
    }
    assert!(checked > 100, "{checked}");
}

#[test]
fn refusals_and_limits() {
    let body = common::unit_cube();
    let p = |x: f64| Point3::new(x, 0.5, 0.5);
    assert_eq!(
        classify_point(&body, p(f64::NAN), &mut Budget::unlimited(), &Stop::new()),
        Err(BodyqError::Refused(BodyqRefusal::NotFinite))
    );
    let stop = Stop::new();
    stop.set();
    assert_eq!(
        classify_point(&body, p(0.5), &mut Budget::unlimited(), &stop),
        Err(BodyqError::Cancelled)
    );
    assert!(matches!(
        classify_point(&body, p(0.5), &mut Budget::new(1), &Stop::new()),
        Err(BodyqError::Exhausted { .. })
    ));
    // Outside the box costs nothing.
    let mut b = Budget::new(0);
    assert_eq!(
        classify_point(&body, p(7.0), &mut b, &Stop::new()),
        Ok(PointClass::In(RegionId::OUTSIDE))
    );
}
