// Written for solidcorex. Apache-2.0.
//! The crate's own tests: the ray table, one ray at a time (the pin's ray-hit cases as
//! outcomes), determinism over rays, and the fallback judged alone.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

use core::f64::consts::PI;

use scx_brep::{Brep, Document, RegionId};
use scx_make::{block, cone, cylinder, sphere, torus};
use scx_num::budget::{Budget, Stop};
use scx_num::fmath;
use scx_num::frame::Frame;
use scx_num::units::Length;
use scx_num::vector3d::{Point3, Vec3};
use scx_testkit::bodies::{Kind, generate};
use scx_testkit::csg::{Side, box_points};

use crate::classify::{classify_with, each_ray};
use crate::error::{BodyqError, BodyqUnsupported};
use crate::near::nearest_region;
use crate::rays::{Cast, RAYS, cast};
use crate::{PointClass, classify_point};

#[path = "../tests/common/mod.rs"]
mod common;

const C: f64 = 1e-5;

fn pt(p: [f64; 3]) -> Point3 {
    Point3::new(p[0], p[1], p[2])
}

fn mm(x: f64) -> Length {
    Length::from_mm(x)
}

fn world_at(o: [f64; 3]) -> Frame {
    Frame::new(pt(o), Vec3::new(0.0, 0.0, 1.0), Vec3::new(1.0, 0.0, 0.0)).unwrap()
}

fn doc() -> Document {
    Document::default()
}

/// One ray from `p` along `d` (made unit), past the body's box.
fn ray(body: &Brep, p: [f64; 3], d: [f64; 3]) -> Cast {
    let p = pt(p);
    let d = Vec3::new(d[0], d[1], d[2]).normalized().unwrap().as_vec();
    let b = body.bounding_box().unwrap();
    let len = b.max_distance_to_point(p) * 1.0625 + 1.0;
    cast(body, p, d, len, &mut Budget::unlimited(), &Stop::new()).unwrap()
}

fn solid(body: &Brep) -> RegionId {
    common::solid(body)
}

#[test]
fn the_ray_table_is_unit_and_in_general_position() {
    let deg = |r: f64| r * 180.0 / PI;
    let s = 1.0 / 2.0_f64.sqrt();
    let t = 1.0 / 3.0_f64.sqrt();
    let planes = [
        [1.0, 0.0, 0.0],
        [0.0, 1.0, 0.0],
        [0.0, 0.0, 1.0],
        [s, s, 0.0],
        [s, -s, 0.0],
        [0.0, s, s],
        [0.0, s, -s],
        [s, 0.0, s],
        [s, 0.0, -s],
    ];
    let lines = [
        [1.0, 0.0, 0.0],
        [0.0, 1.0, 0.0],
        [0.0, 0.0, 1.0],
        [s, s, 0.0],
        [s, -s, 0.0],
        [0.0, s, s],
        [0.0, s, -s],
        [s, 0.0, s],
        [s, 0.0, -s],
        [t, t, t],
        [t, t, -t],
        [t, -t, t],
        [-t, t, t],
    ];
    let v = |a: [f64; 3]| Vec3::new(a[0], a[1], a[2]);
    for (i, d) in RAYS.iter().enumerate() {
        assert!((d.length() - 1.0).abs() <= 4.0 * f64::EPSILON, "ray {i}");
        for n in planes {
            let a = deg(fmath::asin(d.dot(v(n)).abs()));
            assert!(
                a >= 10.0,
                "ray {i} is {a} degrees from the plane normal to {n:?}"
            );
        }
        for l in lines {
            let a = deg(fmath::acos(d.dot(v(l)).abs().min(1.0)));
            assert!(a >= 15.0, "ray {i} is {a} degrees from the line {l:?}");
        }
        for (j, e) in RAYS.iter().enumerate().skip(i + 1) {
            let a = deg(fmath::acos(d.dot(*e).abs().min(1.0)));
            assert!(a >= 25.0, "rays {i} and {j} are {a} degrees from one line");
        }
    }
}

/// The pin's `a_ray_entering_near_an_edge_hits_the_face`: 1e-3 from the cube's edge is far
/// outside coincidence, so the hit is clean.
#[test]
fn a_ray_entering_near_an_edge_is_clean() {
    let cube = block(&world_at([0.0; 3]), [mm(100.0); 3], &doc())
        .unwrap()
        .body;
    let c = ray(&cube, [-30.0, 30.001, 50.0], [1.0, -1.0, 0.0]);
    assert_eq!(c, Cast::Clean(RegionId::OUTSIDE));
}

/// The pin's `the_first_face_a_ray_meets_is_the_nearer_of_its_hits` and
/// `every_face_a_ray_meets_comes_nearest_first`: the nearest hit decides, from outside and
/// from the axis, and two bodies along one ray give the nearer one's side.
#[test]
fn the_nearest_hit_decides() {
    let tube = cylinder(&world_at([0.0; 3]), mm(5.0), mm(20.0), &doc())
        .unwrap()
        .body;
    let x = [1.0, 0.0, 0.0];
    assert_eq!(
        ray(&tube, [-20.0, 3.0, 10.0], x),
        Cast::Clean(RegionId::OUTSIDE)
    );
    assert_eq!(ray(&tube, [0.0, 3.0, 10.0], x), Cast::Clean(solid(&tube)));
    assert_eq!(
        ray(&tube, [20.0, 3.0, 10.0], x),
        Cast::Clean(RegionId::OUTSIDE)
    );
    // The bar and post: from inside the bar up through its top and into the post.
    let bar = common::bar_into_post();
    assert_eq!(
        ray(&bar, [5.0, 4.0, 3.15], [0.0, 0.0, 1.0]),
        Cast::Clean(solid(&bar))
    );
    assert_eq!(
        ray(&bar, [20.0, 8.0, 11.0], [1.0, 0.0, 0.0]),
        Cast::Clean(RegionId::OUTSIDE)
    );
}

/// The pin's `a_ray_through_a_seam_pole_edge_or_corner_hits_each_face_there_once`: a
/// nearest hit on a seam, a pole, an edge or a corner decides nothing, and a ray past a
/// face by 5e-4 meets nothing; a ray within coincidence of an edge it misses decides
/// nothing either.
#[test]
fn a_ray_through_a_seam_pole_edge_or_corner_is_not_clean() {
    let tube = cylinder(&world_at([0.0; 3]), mm(5.0), mm(20.0), &doc())
        .unwrap()
        .body;
    // The seam is at x = 5 (the frame's x axis).
    assert_eq!(
        ray(&tube, [-20.0, 0.0, 10.0], [1.0, 0.0, 0.0]),
        Cast::Clean(RegionId::OUTSIDE)
    );
    assert_eq!(
        ray(&tube, [20.0, 0.0, 10.0], [-1.0, 0.0, 0.0]),
        Cast::Unclean
    );
    assert_eq!(ray(&tube, [0.0, 0.0, 10.0], [1.0, 0.0, 0.0]), Cast::Unclean);
    assert_eq!(
        ray(&tube, [0.0, 0.0, 10.0], [-1.0, 0.0, 0.0]),
        Cast::Clean(solid(&tube))
    );
    let ball = sphere(&world_at([0.0, 0.0, 20.0]), mm(2.0), &doc())
        .unwrap()
        .body;
    assert_eq!(ray(&ball, [0.0, 0.0, 0.0], [0.0, 0.0, 1.0]), Cast::Unclean);
    assert_eq!(ray(&ball, [0.0, 0.0, 20.0], [0.0, 0.0, 1.0]), Cast::Unclean);
    // The seam meridian passes through the frame's x axis.
    assert_eq!(ray(&ball, [0.0, 0.0, 20.0], [1.0, 0.0, 0.0]), Cast::Unclean);
    assert_eq!(
        ray(&ball, [0.0, 0.0, 20.0], [-1.0, 0.0, 0.0]),
        Cast::Clean(solid(&ball))
    );
    let cube = block(&world_at([-5.0; 3]), [mm(10.0); 3], &doc())
        .unwrap()
        .body;
    assert_eq!(ray(&cube, [1.0, 1.0, 1.0], [1.0, 1.0, 1.0]), Cast::Unclean);
    assert_eq!(ray(&cube, [1.0, 1.0, 0.0], [1.0, 1.0, 0.0]), Cast::Unclean);
    assert_eq!(
        ray(&cube, [-20.0, 5.0005, 0.0], [1.0, 0.0, 0.0]),
        Cast::Clean(RegionId::OUTSIDE)
    );
    // Past the face by less than coincidence: the edges x = -5, 5 at y = 5 are within it.
    assert_eq!(
        ray(&cube, [-20.0, 5.000_001, 0.0], [1.0, 0.0, 0.0]),
        Cast::Unclean
    );
    let apex = cone(&world_at([0.0; 3]), [mm(2.0), mm(0.0)], mm(3.0), &doc())
        .unwrap()
        .body;
    assert_eq!(ray(&apex, [0.0, 0.0, 1.0], [0.0, 0.0, 1.0]), Cast::Unclean);
}

/// The pin's grazing cases (`a_grazing_or_close_pair_of_crossings_are_hits_and_a_stretch_
/// along_a_face_is_none`, `a_ray_grazing_a_seam_or_a_rim_or_crossing_a_wall_shallowly_meets_
/// it`): a shallow crossing decides, two crossings 8e-4 apart decide, a ray within the
/// zone of a wall or along a face does not.
#[test]
fn grazing_and_close_crossings() {
    let x = [1.0, 0.0, 0.0];
    let tube = cylinder(&world_at([0.0; 3]), mm(5.0), mm(20.0), &doc())
        .unwrap()
        .body;
    assert_eq!(
        ray(&tube, [-20.0, 4.9999, 10.0], x),
        Cast::Clean(RegionId::OUTSIDE)
    );
    assert_eq!(
        ray(&tube, [0.0, 4.9999, 10.0], x),
        Cast::Clean(solid(&tube))
    );
    assert_eq!(ray(&tube, [-20.0, 4.999_999, 10.0], x), Cast::Unclean);
    // Shallowly through the bottom and up the wall, from below and from inside.
    assert_eq!(
        ray(&tube, [0.0, 4.97, -20.0], [0.0, 0.001, 1.0]),
        Cast::Clean(RegionId::OUTSIDE)
    );
    assert_eq!(
        ray(&tube, [0.0, 4.97, 5.0], [0.0, 0.001, 1.0]),
        Cast::Clean(solid(&tube))
    );
    let ball = sphere(&world_at([0.0, 0.0, 20.0]), mm(2.0), &doc())
        .unwrap()
        .body;
    assert_eq!(
        ray(&ball, [-20.0, 1.999, 20.0], x),
        Cast::Clean(RegionId::OUTSIDE)
    );
    let thin = cylinder(&world_at([1.0, 0.0, 0.0]), mm(4e-4), mm(1.0), &doc())
        .unwrap()
        .body;
    assert_eq!(
        ray(&thin, [-19.0, 0.0, 0.5], x),
        Cast::Clean(RegionId::OUTSIDE)
    );
    assert_eq!(
        ray(&thin, [1.0, 0.0, 0.5], [-1.0, 0.0, 0.0]),
        Cast::Clean(solid(&thin))
    );
    let ring = torus(&world_at([0.0; 3]), mm(10.0), mm(2.0), &doc())
        .unwrap()
        .body;
    assert_eq!(
        ray(&ring, [0.0, 0.0, 0.0], [0.0, 1.0, 0.0]),
        Cast::Clean(RegionId::OUTSIDE)
    );
    assert_eq!(
        ray(&ring, [-20.0, 7.0, 0.5], x),
        Cast::Clean(RegionId::OUTSIDE)
    );
    assert_eq!(
        ray(&ring, [0.0, 10.0, 0.5], [0.0, 1.0, 0.0]),
        Cast::Clean(solid(&ring))
    );
    let cube = block(&world_at([-5.0; 3]), [mm(10.0); 3], &doc())
        .unwrap()
        .body;
    assert_eq!(ray(&cube, [-20.0, 0.0, 5.0], x), Cast::Unclean);
}

/// A rhombic dodecahedron: from its centre every axis and body diagonal runs through a
/// vertex and every table ray is clean.
#[test]
fn the_table_beats_a_rhombic_dodecahedron() {
    let rd = common::rhombic_dodecahedron();
    let inside = Cast::Clean(solid(&rd));
    for d in [
        [1.0, 0.0, 0.0],
        [0.0, -1.0, 0.0],
        [0.0, 0.0, 1.0],
        [1.0, 1.0, 1.0],
        [-1.0, 1.0, -1.0],
    ] {
        assert_eq!(ray(&rd, [0.0; 3], d), Cast::Unclean, "{d:?}");
    }
    for d in RAYS {
        assert_eq!(ray(&rd, [0.0; 3], [d.x, d.y, d.z]), inside, "{d:?}");
    }
    let o = pt([0.0; 3]);
    assert_eq!(
        classify_point(&rd, o, &mut Budget::unlimited(), &Stop::new()).unwrap(),
        PointClass::In(solid(&rd))
    );
    // With only the axes and diagonals the fallback decides, and agrees.
    let axes: Vec<Vec3> = [
        [1.0, 0.0, 0.0],
        [0.0, 1.0, 0.0],
        [0.0, 0.0, 1.0],
        [1.0, 1.0, 1.0],
    ]
    .iter()
    .map(|a| Vec3::new(a[0], a[1], a[2]).normalized().unwrap().as_vec())
    .collect();
    assert_eq!(
        classify_with(&rd, o, &axes, &mut Budget::unlimited(), &Stop::new()).unwrap(),
        PointClass::In(solid(&rd))
    );
}

fn box_of(body: &Brep) -> ([f64; 3], [f64; 3]) {
    let b = body.bounding_box().unwrap();
    let (lo, hi) = (b.min(), b.max());
    ([lo.x, lo.y, lo.z], [hi.x, hi.y, hi.z])
}

/// The fallback alone, on points through each primitive's box, against T10.
#[test]
fn the_fallback_alone_agrees_with_t10() {
    for (name, body, csg) in common::primitives() {
        let (lo, hi) = box_of(&body);
        let bb = body.bounding_box().unwrap();
        let mut checked = 0;
        for p in box_points(lo, hi, 0.0, 300, 7) {
            let want = match csg.side(p, 4.0 * C) {
                Side::In => solid(&body),
                Side::Out => RegionId::OUTSIDE,
                Side::Band => continue,
            };
            let got = nearest_region(&body, &bb, pt(p), &mut Budget::unlimited(), &Stop::new())
                .unwrap_or_else(|e| panic!("{name} {p:?}: {e}"));
            assert_eq!(got, want, "{name} {p:?}");
            checked += 1;
        }
        assert!(checked > 200, "{name}: {checked}");
    }
}

/// The fallback at the edges and vertices: points just off a block's corners and edges,
/// and off the cone's apex and the sphere's poles, where the nearest boundary point is the
/// vertex or the edge.
#[test]
fn the_fallback_reads_edges_and_vertices() {
    let cube = block(&world_at([0.0; 3]), [mm(1.0); 3], &doc())
        .unwrap()
        .body;
    let bb = cube.bounding_box().unwrap();
    let side = |body: &Brep, p: [f64; 3]| {
        nearest_region(
            body,
            &body.bounding_box().unwrap(),
            pt(p),
            &mut Budget::unlimited(),
            &Stop::new(),
        )
        .unwrap()
    };
    let _ = bb;
    let e = 1e-3;
    for (p, inside) in [
        ([-e, -e, -e], false),
        ([e, e, e], true),
        ([1.0 + e, 1.0 + e, 0.5], false),
        ([1.0 - e, 1.0 - e, 0.5], true),
        ([1.0 + e, 0.5, 1.0 + e], false),
        ([0.5, e, e], true),
    ] {
        let want = if inside {
            solid(&cube)
        } else {
            RegionId::OUTSIDE
        };
        assert_eq!(side(&cube, p), want, "{p:?}");
    }
    let apex = cone(&world_at([0.0; 3]), [mm(2.0), mm(0.0)], mm(3.0), &doc())
        .unwrap()
        .body;
    assert_eq!(side(&apex, [0.0, 0.0, 3.0 + e]), RegionId::OUTSIDE);
    assert_eq!(side(&apex, [0.0, 0.0, 3.0 - e]), solid(&apex));
    let ball = sphere(&world_at([0.0; 3]), mm(2.0), &doc()).unwrap().body;
    assert_eq!(side(&ball, [0.0, 0.0, 2.0 + e]), RegionId::OUTSIDE);
    assert_eq!(side(&ball, [0.0, 0.0, -2.0 + e]), solid(&ball));
    let tube = cylinder(&world_at([0.0; 3]), mm(1.0), mm(2.0), &doc())
        .unwrap()
        .body;
    // Off the rim at the seam's foot, and on the seam.
    assert_eq!(side(&tube, [1.0 + e, 0.0, -e]), RegionId::OUTSIDE);
    assert_eq!(side(&tube, [1.0 - e, 0.0, e]), solid(&tube));
    assert_eq!(side(&tube, [1.0 - e, 0.0, 1.0]), solid(&tube));
}

/// Every clean ray alone gives the answer `classify_point` gives, the fallback gives it
/// too, and two runs charge the same work: over every testkit kind at two seeds.
#[test]
fn every_clean_ray_and_the_fallback_agree() {
    let mut rays = 0usize;
    let mut points = 0usize;
    for kind in Kind::ALL {
        for seed in [0, 3] {
            let g = generate(kind, seed, 2).unwrap();
            let body = g.draft.finish(&doc()).map_err(|u| u.cause).unwrap().0;
            let (lo, hi) = box_of(&body);
            let bb = body.bounding_box().unwrap();
            for p in box_points(lo, hi, 0.1, 60, seed + 11) {
                let p = pt(p);
                let mut b1 = Budget::unlimited();
                let got = classify_point(&body, p, &mut b1, &Stop::new()).unwrap();
                let mut b2 = Budget::unlimited();
                assert_eq!(
                    classify_point(&body, p, &mut b2, &Stop::new()).unwrap(),
                    got
                );
                assert_eq!(b1.used(), b2.used(), "{kind:?} {p:?}");
                let PointClass::In(r) = got else { continue };
                if !bb.contains_point(p, 0.0) {
                    continue;
                }
                points += 1;
                let each = each_ray(&body, p, &mut Budget::unlimited(), &Stop::new()).unwrap();
                for (i, c) in each.iter().enumerate() {
                    if let Some(c) = c {
                        rays += 1;
                        assert_eq!(*c, r, "{kind:?} seed {seed} {p:?} ray {i}");
                    }
                }
                match nearest_region(&body, &bb, p, &mut Budget::unlimited(), &Stop::new()) {
                    Ok(n) => assert_eq!(n, r, "{kind:?} seed {seed} {p:?} fallback"),
                    Err(BodyqError::Unsupported(BodyqUnsupported::NonManifold { .. }))
                        if matches!(kind, Kind::Stack | Kind::Checker | Kind::Touching) => {}
                    Err(e) => panic!("{kind:?} seed {seed} {p:?}: {e}"),
                }
            }
        }
    }
    assert!(
        points > 1000 && rays > 10 * points,
        "{points} points, {rays} rays"
    );
}
