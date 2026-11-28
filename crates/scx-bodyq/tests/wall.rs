// Written for solidcorex. Apache-2.0.
//! The thin-wall helpers (research 74 §34, BW11): facing planes and cylinders, a mid-sheet's
//! geometry, a joint's line and moving a sheet's edge onto it, a cylindrical wall's frame, and
//! the cap tests. The truths are closed forms, the pin's test values re-derived by hand (a
//! plate 2 thick, a tube of radii 10 and 8, an L bracket's areas), and, where a pin test
//! turned on a fixed half degree, the same case at the angle the reach gives.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::float_cmp
)]

mod along_common;

use along_common::doc;
use scx_bodyq::{
    Apart, BodyqError, BodyqRefusal, CylinderFace, Facing, FacingCylinders, FacingPlanes, Holds,
    Line3, Meeting, Moved, Near, OutwardPlane, PlaneAt, Wall, Wrap, axial_range_in, cylinder_face,
    facing_cylinders, facing_planes, from_frame, in_frame, is_face_on_plane, is_point_on_plane,
    is_square_plane_at_height, material_toward_axis, move_edge_onto_line, onto_plane,
    outward_plane, planes_meeting, polygon_area, round_directions, wall_frame, wall_holds,
};
use scx_num::fmath;
use scx_num::frame::Frame;
use scx_num::units::{Accuracy, Length};
use scx_num::vector2d::Point2;
use scx_num::vector3d::{Point3, Vec3};
use scx_testkit::bodies::made::{block, cylinder};

fn mm(x: f64) -> Length {
    Length::from_mm(x)
}

fn v(x: f64, y: f64, z: f64) -> Vec3 {
    Vec3::new(x, y, z)
}

fn p(x: f64, y: f64, z: f64) -> Point3 {
    Point3::new(x, y, z)
}

/// The document's coincidence (its resolution: no entity is less accurate) held over `reach`.
fn near(reach: f64) -> Near {
    Near::of(
        doc().resolution(),
        Accuracy::ZERO,
        Accuracy::ZERO,
        mm(reach),
    )
    .unwrap()
}

fn plane(point: [f64; 3], n: [f64; 3]) -> OutwardPlane {
    OutwardPlane {
        point: p(point[0], point[1], point[2]),
        outward: v(n[0], n[1], n[2]),
    }
}

fn pair(f: Facing<FacingPlanes>) -> FacingPlanes {
    match f {
        Facing::Pair(x) => x,
        other => panic!("{other:?}"),
    }
}

// ---- facing planes ------------------------------------------------------------------------

#[test]
fn a_plates_two_faces_are_its_thickness_apart_with_the_mid_plane_between() {
    // A plate 2 thick: top at z = 2 facing up, bottom at z = 0 facing down. The points need
    // not be above one another.
    let n = near(100.0);
    let top = plane([0.0, 0.0, 2.0], [0.0, 0.0, 1.0]);
    let bottom = plane([5.0, -3.0, 0.0], [0.0, 0.0, -1.0]);
    let f = pair(facing_planes(&top, &bottom, &n).unwrap());
    assert!((f.thickness.mm() - 2.0).abs() < 1e-12, "{f:?}");
    assert!((f.mid_point.z - 1.0).abs() < 1e-12 && (f.toward_first.z - 1.0).abs() < 1e-12);
    assert_eq!(f.skew.mm(), 0.0);
    // The other way round: the same wall, the mid plane at the same place.
    let back = pair(facing_planes(&bottom, &top, &n).unwrap());
    assert!((back.thickness.mm() - 2.0).abs() < 1e-12);
    assert!((back.mid_point.z - 1.0).abs() < 1e-12 && (back.toward_first.z + 1.0).abs() < 1e-12);
}

#[test]
fn the_answer_does_not_depend_on_the_normals_length() {
    // A wall 3 thick across the diagonal (1, 1, 0)/sqrt 2, normals not unit.
    let s = 0.5_f64.sqrt();
    let a = plane([3.0 * s, 3.0 * s, 0.0], [10.0 * s, 10.0 * s, 0.0]);
    let b = plane([0.0, 0.0, 7.0], [-s, -s, 0.0]);
    let f = pair(facing_planes(&a, &b, &near(100.0)).unwrap());
    assert!((f.thickness.mm() - 3.0).abs() < 1e-12, "{f:?}");
    assert!((f.mid_point.to_vec().dot(v(s, s, 0.0)) - 1.5).abs() < 1e-12);
}

#[test]
fn planes_that_bound_air_or_nothing_do_not_pair_and_say_why() {
    let n = near(100.0);
    // Normals pointing toward each other: a gap, not a wall.
    let low = plane([0.0, 0.0, 0.0], [0.0, 0.0, 1.0]);
    let high = plane([0.0, 0.0, 2.0], [0.0, 0.0, -1.0]);
    assert_eq!(
        facing_planes(&low, &high, &n).unwrap(),
        Facing::Apart(Apart::Air)
    );
    // Opposite and at the same place: nothing between them.
    let a = plane([0.0, 0.0, 1.0], [0.0, 0.0, 1.0]);
    let b = plane([4.0, 4.0, 1.0], [0.0, 0.0, -1.0]);
    assert_eq!(
        facing_planes(&a, &b, &n).unwrap(),
        Facing::Apart(Apart::Thin)
    );
    // Just past the coincidence apart pairs; at it does not.
    let zone = n.coincidence().mm();
    let floor = plane([0.0, 0.0, 0.0], [0.0, 0.0, -1.0]);
    let thin = plane([0.0, 0.0, 2.0 * zone], [0.0, 0.0, 1.0]);
    assert!(matches!(
        facing_planes(&thin, &floor, &n).unwrap(),
        Facing::Pair(_)
    ));
    let at = plane([0.0, 0.0, zone], [0.0, 0.0, 1.0]);
    assert_eq!(
        facing_planes(&at, &floor, &n).unwrap(),
        Facing::Apart(Apart::Thin)
    );
    // Along the same normals (not opposite) never pairs; a rim and a face never do.
    let top = plane([0.0, 0.0, 2.0], [0.0, 0.0, 1.0]);
    assert_eq!(
        facing_planes(&top, &plane([0.0; 3], [0.0, 0.0, 1.0]), &n).unwrap(),
        Facing::Apart(Apart::SameWay)
    );
    assert_eq!(
        facing_planes(&top, &plane([0.0; 3], [1.0, 0.0, 0.0]), &n).unwrap(),
        Facing::Apart(Apart::Tilted)
    );
}

#[test]
fn the_angle_that_pairs_is_the_coincidence_over_the_reach() {
    // The pin paired within half a degree at any size. Here planes are one line when they
    // stay within coincidence (1e-5 mm) over the reach: 1e-7 rad over 100 mm, 1e-8 over a
    // metre, and a half degree only on a face a millimetre across (coincidence 1e-5 over
    // 1e-3 mm is 1e-2 rad).
    let top = plane([0.0, 0.0, 2.0], [0.0, 0.0, 1.0]);
    let tilted = |t: f64| plane([0.0, 0.0, 0.0], [fmath::sin(t), 0.0, -fmath::cos(t)]);
    let (small, large) = (0.5e-7, 2.0e-7);
    assert!(matches!(
        facing_planes(&top, &tilted(small), &near(100.0)).unwrap(),
        Facing::Pair(_)
    ));
    assert_eq!(
        facing_planes(&top, &tilted(large), &near(100.0)).unwrap(),
        Facing::Apart(Apart::Tilted)
    );
    // The same small tilt does not pair across a metre.
    assert_eq!(
        facing_planes(&top, &tilted(small), &near(1000.0)).unwrap(),
        Facing::Apart(Apart::Tilted)
    );
    // The skew reports the deviation over the reach, within coincidence.
    let f = pair(facing_planes(&top, &tilted(small), &near(100.0)).unwrap());
    assert!((f.skew.mm() - small * 100.0).abs() < 1e-12, "{f:?}");
    // A small face tolerates the half degree the pin used.
    assert!(matches!(
        facing_planes(&top, &tilted(0.4_f64.to_radians()), &near(1e-3)).unwrap(),
        Facing::Pair(_)
    ));
}

#[test]
fn a_tilted_second_plane_gives_one_thickness_whichever_point_represents_it() {
    // `a` is z = 2 facing up; `b` passes through the origin with normal (sin t, 0, -cos t),
    // t = 4e-9 rad. The normal line through (0, 0, 2) is the z axis, which meets `b` at the
    // origin: thickness 2. The point (1000, 0, 1000 tan t) is on `b` too and gives the same.
    let n = near(1000.0);
    let t = 4.0e-9_f64;
    let top = plane([0.0, 0.0, 2.0], [0.0, 0.0, 1.0]);
    let nb = [fmath::sin(t), 0.0, -fmath::cos(t)];
    for pt in [[0.0, 0.0, 0.0], [1000.0, 0.0, 1000.0 * fmath::tan(t)]] {
        let f = pair(facing_planes(&top, &plane(pt, nb), &n).unwrap());
        assert!((f.thickness.mm() - 2.0).abs() < 1e-9, "{pt:?}: {f:?}");
        assert!((f.mid_point - p(0.0, 0.0, 1.0)).length() < 1e-9, "{f:?}");
    }
}

#[test]
fn hostile_planes_are_refused_not_answered() {
    let n = near(10.0);
    let good = plane([0.0; 3], [0.0, 0.0, 1.0]);
    for (bad, why) in [
        (
            plane([f64::NAN, 0.0, 0.0], [0.0, 0.0, -1.0]),
            BodyqRefusal::NotFinite,
        ),
        (
            plane([0.0, f64::INFINITY, 0.0], [0.0, 0.0, -1.0]),
            BodyqRefusal::NotFinite,
        ),
        (
            plane([0.0; 3], [0.0, f64::NAN, -1.0]),
            BodyqRefusal::NotFinite,
        ),
        (plane([0.0; 3], [0.0; 3]), BodyqRefusal::NoDirection),
    ] {
        assert_eq!(
            facing_planes(&good, &bad, &n),
            Err(BodyqError::Refused(why))
        );
        assert_eq!(
            facing_planes(&bad, &good, &n),
            Err(BodyqError::Refused(why))
        );
    }
    // A reach or coincidence that is not one.
    assert!(Near::new(mm(1e-5), mm(0.0)).is_err());
    assert!(Near::new(mm(-1.0), mm(1.0)).is_err());
    assert!(Near::new(mm(f64::NAN), mm(1.0)).is_err());
}

#[test]
fn a_blocks_top_and_bottom_face_each_other_and_its_sides_do_not() {
    let m = block(&Frame::WORLD, [mm(10.0), mm(6.0), mm(4.0)], &doc()).unwrap();
    let (f, b) = (m.faces, &m.body);
    let n = near(12.0);
    let top = outward_plane(b, f.top).unwrap();
    let bottom = outward_plane(b, f.bottom).unwrap();
    assert!((top.outward.z - 1.0).abs() < 1e-15 && (bottom.outward.z + 1.0).abs() < 1e-15);
    let w = pair(facing_planes(&top, &bottom, &n).unwrap());
    assert!((w.thickness.mm() - 4.0).abs() < 1e-12, "{w:?}");
    let (s0, s2) = (
        outward_plane(b, f.sides[0]).unwrap(),
        outward_plane(b, f.sides[2]).unwrap(),
    );
    let w = pair(facing_planes(&s0, &s2, &n).unwrap());
    assert!((w.thickness.mm() - 6.0).abs() < 1e-12, "{w:?}");
    assert_eq!(
        facing_planes(&top, &s0, &n).unwrap(),
        Facing::Apart(Apart::Tilted)
    );
    // A cylinder's wall is no plane.
    let c = cylinder(&Frame::WORLD, mm(3.0), mm(7.0), &doc()).unwrap();
    assert_eq!(
        outward_plane(&c.body, c.faces.wall),
        Err(BodyqError::Refused(BodyqRefusal::WrongSurface {
            face: c.faces.wall
        }))
    );
}

// ---- facing cylinders and a cylinder's readings --------------------------------------------

fn cyl(point: [f64; 3], axis: [f64; 3], r: f64, toward: bool) -> CylinderFace {
    CylinderFace {
        axis_point: p(point[0], point[1], point[2]),
        axis: v(axis[0], axis[1], axis[2]),
        radius: mm(r),
        material_toward_axis: toward,
    }
}

fn pair_c(f: Facing<FacingCylinders>) -> FacingCylinders {
    match f {
        Facing::Pair(x) => x,
        other => panic!("{other:?}"),
    }
}

#[test]
fn a_tubes_two_faces_are_its_wall_apart_with_the_mid_cylinder_between() {
    // A tube of outer radius 10 and inner radius 8 about the z axis: wall 2, mid radius 9.
    let n = near(100.0);
    let outer = cyl([0.0; 3], [0.0, 0.0, 1.0], 10.0, true);
    let bore = cyl([0.0, 0.0, 50.0], [0.0, 0.0, -3.0], 8.0, false);
    let f = pair_c(facing_cylinders(&outer, &bore, &n).unwrap());
    assert!((f.thickness.mm() - 2.0).abs() < 1e-12 && (f.mid_radius.mm() - 9.0).abs() < 1e-12);
    let back = pair_c(facing_cylinders(&bore, &outer, &n).unwrap());
    assert!(
        (back.thickness.mm() - 2.0).abs() < 1e-12 && (back.mid_radius.mm() - 9.0).abs() < 1e-12
    );
    // A 2 mm sheet bent on an inside radius of 3: outside radius 5, mid radius 4.
    let outside = cyl([0.0, 5.0, 5.0], [1.0, 0.0, 0.0], 5.0, true);
    let inside = cyl([7.0, 5.0, 5.0], [1.0, 0.0, 0.0], 3.0, false);
    let f = pair_c(facing_cylinders(&outside, &inside, &n).unwrap());
    assert!((f.thickness.mm() - 2.0).abs() < 1e-12 && (f.mid_radius.mm() - 4.0).abs() < 1e-12);
}

#[test]
fn cylinders_that_are_not_a_wall_say_why() {
    let n = near(100.0);
    let outer = cyl([0.0; 3], [0.0, 0.0, 1.0], 10.0, true);
    let apart = |b: CylinderFace| match facing_cylinders(&outer, &b, &n).unwrap() {
        Facing::Apart(a) => a,
        other => panic!("{other:?}"),
    };
    assert_eq!(
        apart(cyl([0.5, 0.0, 0.0], [0.0, 0.0, 1.0], 8.0, false)),
        Apart::OffAxis
    );
    assert_eq!(
        apart(cyl([0.0; 3], [0.02, 0.0, 1.0], 8.0, false)),
        Apart::Tilted
    );
    assert_eq!(
        apart(cyl([0.0; 3], [1.0, 0.0, 0.0], 8.0, false)),
        Apart::Tilted
    );
    assert_eq!(
        apart(cyl([0.0; 3], [0.0, 0.0, 1.0], 8.0, true)),
        Apart::SameSide
    );
    // A bore wider than the outside: air between the faces. The same radius: no wall.
    assert_eq!(
        apart(cyl([0.0; 3], [0.0, 0.0, 1.0], 12.0, false)),
        Apart::Air
    );
    assert_eq!(
        apart(cyl([0.0; 3], [0.0, 0.0, 1.0], 10.0, false)),
        Apart::Thin
    );
    let bore = cyl([0.0; 3], [0.0, 0.0, 1.0], 12.0, false);
    let bore_too = cyl([0.0; 3], [0.0, 0.0, 1.0], 8.0, false);
    assert_eq!(
        facing_cylinders(&bore, &bore_too, &n).unwrap(),
        Facing::Apart(Apart::SameSide)
    );
}

#[test]
fn a_tilted_second_axis_pairs_whichever_of_its_points_represents_it() {
    // b's axis runs along (4e-9, 0, 1) through the origin: it pairs as a wall 2 thick over
    // 100 mm (the lines stay 4e-7 apart). (4e-6, 0, 1000) is on the same line, and gives the
    // same answer.
    let n = near(100.0);
    let outer = cyl([0.0; 3], [0.0, 0.0, 1.0], 10.0, true);
    for pt in [[0.0; 3], [4e-6, 0.0, 1000.0]] {
        let bore = cyl(pt, [4e-9, 0.0, 1.0], 8.0, false);
        let f = pair_c(facing_cylinders(&outer, &bore, &n).unwrap());
        assert!((f.thickness.mm() - 2.0).abs() < 1e-12, "{pt:?}: {f:?}");
    }
}

#[test]
fn hostile_cylinders_are_refused() {
    let n = near(10.0);
    let good = cyl([0.0; 3], [0.0, 0.0, 1.0], 10.0, true);
    for (bad, why) in [
        (
            cyl([f64::NAN, 0.0, 0.0], [0.0, 0.0, 1.0], 8.0, false),
            BodyqRefusal::NotFinite,
        ),
        (
            cyl([0.0; 3], [0.0, f64::INFINITY, 1.0], 8.0, false),
            BodyqRefusal::NotFinite,
        ),
        (
            cyl([0.0; 3], [0.0; 3], 8.0, false),
            BodyqRefusal::NoDirection,
        ),
        (
            cyl([0.0; 3], [0.0, 0.0, 1.0], 0.0, false),
            BodyqRefusal::NotPositive,
        ),
        (
            cyl([0.0; 3], [0.0, 0.0, 1.0], -3.0, false),
            BodyqRefusal::NotPositive,
        ),
        (
            cyl([0.0; 3], [0.0, 0.0, 1.0], f64::NAN, false),
            BodyqRefusal::NotFinite,
        ),
    ] {
        assert_eq!(
            facing_cylinders(&good, &bad, &n),
            Err(BodyqError::Refused(why))
        );
        assert_eq!(
            facing_cylinders(&bad, &good, &n),
            Err(BodyqError::Refused(why))
        );
    }
}

#[test]
fn a_cylinder_face_of_a_body_knows_its_material_side() {
    let c = cylinder(&Frame::WORLD, mm(3.0), mm(7.0), &doc()).unwrap();
    let wall = cylinder_face(&c.body, c.faces.wall).unwrap();
    assert_eq!(wall.radius.mm(), 3.0);
    assert!(wall.material_toward_axis, "the outside of a solid cylinder");
    assert!((wall.axis.dot(v(0.0, 0.0, 1.0)).abs() - 1.0).abs() < 1e-15);
    assert_eq!(
        cylinder_face(&c.body, c.faces.top),
        Err(BodyqError::Refused(BodyqRefusal::WrongSurface {
            face: c.faces.top
        }))
    );
}

#[test]
fn the_material_side_follows_the_normal_against_the_radial_direction() {
    let n = near(10.0);
    // Axis z through the origin. At (10, 0, 5) the direction away from the axis is +x: an
    // outward normal +x is a tube's outside (material toward the axis), -x a bore's.
    let (o, z) = (p(0.0, 0.0, 0.0), v(0.0, 0.0, 1.0));
    assert!(material_toward_axis(o, z, p(10.0, 0.0, 5.0), v(1.0, 0.0, 0.0), &n).unwrap());
    assert!(!material_toward_axis(o, z, p(10.0, 0.0, 5.0), v(-1.0, 0.0, 0.0), &n).unwrap());
    // The same axis given by another point on it and run the other way, any length: at
    // (0, -8, 7) away from the axis is -y, so a normal -y (any length) is an outside.
    let (o2, down) = (p(0.0, 0.0, 50.0), v(0.0, 0.0, -3.0));
    assert!(material_toward_axis(o2, down, p(0.0, -8.0, 7.0), v(0.0, -2.0, 0.0), &n).unwrap());
    assert!(!material_toward_axis(o2, down, p(0.0, -8.0, 7.0), v(0.0, 0.5, 0.0), &n).unwrap());
    // Hostile queries.
    let (pt, nrm) = (p(10.0, 0.0, 0.0), v(1.0, 0.0, 0.0));
    let refused = |r| Err(BodyqError::Refused(r));
    assert_eq!(
        material_toward_axis(p(f64::NAN, 0.0, 0.0), z, pt, nrm, &n),
        refused(BodyqRefusal::NotFinite)
    );
    assert_eq!(
        material_toward_axis(o, Vec3::ZERO, pt, nrm, &n),
        refused(BodyqRefusal::NoDirection)
    );
    assert_eq!(
        material_toward_axis(o, z, pt, Vec3::ZERO, &n),
        refused(BodyqRefusal::NoDirection)
    );
    // A point on the axis (within coincidence of it) has no direction away from it.
    assert_eq!(
        material_toward_axis(o, z, p(0.0, 0.0, 9.0), nrm, &n),
        refused(BodyqRefusal::OnAxis)
    );
    assert_eq!(
        material_toward_axis(o, z, p(1e-7, 0.0, 9.0), nrm, &n),
        refused(BodyqRefusal::OnAxis)
    );
    assert!(material_toward_axis(o, z, p(1e-3, 0.0, 9.0), nrm, &n).unwrap());
}

#[test]
fn directions_round_the_axis_are_the_curves_direction_off_the_axis() {
    // Axis z (length 2), at (10, 0, 0) of the cylinder of radius 10, an edge running along +y
    // (the circle z = 0): the face's tangent plane there is spanned by z and y.
    let n = near(10.0);
    let d = round_directions(v(0.0, 0.0, 2.0), v(0.0, 5.0, 0.0), &n).unwrap();
    assert_eq!(d.axial, [v(0.0, 0.0, 1.0), v(0.0, 0.0, -1.0)]);
    assert_eq!(d.round, Some([v(0.0, 1.0, 0.0), v(0.0, -1.0, 0.0)]));
    let radial = v(1.0, 0.0, 0.0);
    assert!(
        d.axial
            .iter()
            .chain(d.round.iter().flatten())
            .all(|s| s.dot(radial) == 0.0)
    );
    // A helix through the same point, along (0, 3, 4): its part off the axis is (0, 3, 0).
    let h = round_directions(v(0.0, 0.0, 1.0), v(0.0, 3.0, 4.0), &n).unwrap();
    assert_eq!(h.round, Some([v(0.0, 1.0, 0.0), v(0.0, -1.0, 0.0)]));
    // The direction alone counts, not its length.
    let huge = round_directions(v(0.0, 0.0, 1.0), v(0.0, f64::MAX, f64::MAX), &n).unwrap();
    assert!((huge.round.unwrap()[0] - v(0.0, 1.0, 0.0)).length() < 1e-15);
    let tiny = round_directions(v(0.0, 0.0, 1.0), v(0.0, 1e-200, 0.0), &n).unwrap();
    assert!((tiny.round.unwrap()[0] - v(0.0, 1.0, 0.0)).length() < 1e-15);
    // An edge along the axis (a straight seam), or within coincidence of it over its length,
    // or no direction: only the two along the axis.
    for along in [v(0.0, 0.0, -4.0), v(1e-7, 0.0, 1.0), Vec3::ZERO] {
        let d = round_directions(v(0.0, 0.0, 1.0), along, &n).unwrap();
        assert_eq!(d.round, None, "{along:?}");
        assert_eq!(d.axial, [v(0.0, 0.0, 1.0), v(0.0, 0.0, -1.0)]);
    }
    // The same slant over a longer curve does leave the axis: 1e-7 over 10 mm is 1e-6, a
    // tenth of the coincidence; over 1000 mm it is 1e-4, ten times it.
    assert!(
        round_directions(v(0.0, 0.0, 1.0), v(1e-7, 0.0, 1.0), &near(1000.0))
            .unwrap()
            .round
            .is_some()
    );
    // Hostile: not finite, or no axis.
    assert!(round_directions(v(f64::NAN, 0.0, 1.0), v(0.0, 1.0, 0.0), &n).is_err());
    assert!(round_directions(v(0.0, 0.0, 1.0), v(0.0, f64::INFINITY, 0.0), &n).is_err());
    assert!(round_directions(Vec3::ZERO, v(0.0, 1.0, 0.0), &n).is_err());
}

#[test]
fn an_axial_range_reads_in_the_other_cylinders_coordinates() {
    let a = cyl([0.0; 3], [0.0, 0.0, 1.0], 10.0, true);
    // b's axis point at z = 50 and its axis run down (length 3): its coordinate x is the
    // height 50 - x, so its range 10..30 is the heights 20..40 (low end first).
    let down = cyl([0.0, 0.0, 50.0], [0.0, 0.0, -3.0], 8.0, false);
    assert_eq!(
        axial_range_in(&a, &down, [mm(10.0), mm(30.0)]).unwrap(),
        [mm(20.0), mm(40.0)]
    );
    // b's axis point at z = 5 (given off the line: only its height counts), same way up.
    let up = cyl([3.0, 4.0, 5.0], [0.0, 0.0, 1.0], 8.0, false);
    assert_eq!(
        axial_range_in(&a, &up, [mm(0.0), mm(10.0)]).unwrap(),
        [mm(5.0), mm(15.0)]
    );
    assert!(axial_range_in(&a, &up, [mm(f64::NAN), mm(1.0)]).is_err());
    assert!(axial_range_in(&a, &up, [mm(0.0), mm(f64::INFINITY)]).is_err());
    let none = cyl([0.0; 3], [0.0; 3], 8.0, false);
    assert!(axial_range_in(&a, &none, [mm(0.0), mm(1.0)]).is_err());
    assert!(axial_range_in(&none, &a, [mm(0.0), mm(1.0)]).is_err());
}

// ---- a mid-sheet's plane geometry ----------------------------------------------------------

#[test]
fn a_plates_end_face_lies_in_its_wall_and_a_boss_does_not() {
    // A plate 2 thick: top face z = 2 facing up, so the wall is 0 <= z <= 2.
    let plate = Wall::Flat {
        face: plane([7.0, -3.0, 2.0], [0.0, 0.0, 5.0]),
        thickness: mm(2.0),
    };
    let end = [
        p(40.0, 0.0, 0.0),
        p(40.0, 30.0, 0.0),
        p(40.0, 30.0, 2.0),
        p(40.0, 0.0, 2.0),
    ];
    assert_eq!(wall_holds(&plate, &end, mm(0.0)).unwrap(), Holds::Yes);
    // A boss standing 14 above the top: its top corner (index 1) is 14 out of the wall.
    let boss = [p(40.0, 25.0, 2.0), p(40.0, 25.0, 16.0)];
    assert_eq!(
        wall_holds(&plate, &boss, mm(1e-6)).unwrap(),
        Holds::No { point: 1 }
    );
    // Just past the face by less than the slack is in; by more is out.
    assert_eq!(
        wall_holds(&plate, &[p(0.0, 0.0, 2.0 + 1e-7)], mm(1e-6)).unwrap(),
        Holds::Yes
    );
    assert_eq!(
        wall_holds(&plate, &[p(0.0, 0.0, -2e-6)], mm(1e-6)).unwrap(),
        Holds::No { point: 0 }
    );
    assert_eq!(wall_holds(&plate, &[], mm(0.0)).unwrap(), Holds::Yes);
}

#[test]
fn a_tubes_end_ring_lies_in_its_wall_and_its_axis_does_not() {
    // Radii 8 and 10 about the z axis through (1, 2, 0).
    let tube = Wall::Round {
        axis_point: p(1.0, 2.0, 0.0),
        axis: v(0.0, 0.0, -3.0),
        inner: mm(8.0),
        outer: mm(10.0),
    };
    let s = 9.0 * 0.5_f64.sqrt();
    let ring = [
        p(9.0, 2.0, 50.0),
        p(11.0, 2.0, 50.0),
        p(1.0 + s, 2.0 + s, 50.0),
    ];
    assert_eq!(wall_holds(&tube, &ring, mm(0.0)).unwrap(), Holds::Yes);
    assert_eq!(
        wall_holds(&tube, &[p(1.0, 2.0, 50.0)], mm(1e-6)).unwrap(),
        Holds::No { point: 0 }
    );
    assert_eq!(
        wall_holds(&tube, &[p(12.0, 2.0, 0.0)], mm(1e-6)).unwrap(),
        Holds::No { point: 0 }
    );
}

#[test]
fn nonsense_walls_and_points_are_refused() {
    let good = Wall::Flat {
        face: plane([0.0; 3], [0.0, 0.0, 1.0]),
        thickness: mm(1.0),
    };
    let pt = [p(0.0, 0.0, 0.5)];
    assert!(wall_holds(&good, &pt, mm(f64::NAN)).is_err());
    assert!(wall_holds(&good, &pt, mm(-1.0)).is_err());
    assert!(wall_holds(&good, &[p(f64::INFINITY, 0.0, 0.0)], mm(0.0)).is_err());
    let round = |axis_point: Point3, inner: f64, outer: f64| Wall::Round {
        axis_point,
        axis: v(0.0, 0.0, 1.0),
        inner: mm(inner),
        outer: mm(outer),
    };
    for wall in [
        Wall::Flat {
            face: plane([0.0; 3], [0.0; 3]),
            thickness: mm(1.0),
        },
        Wall::Flat {
            face: plane([0.0; 3], [0.0, 0.0, 1.0]),
            thickness: mm(-1.0),
        },
        round(p(0.0, 0.0, 0.0), 5.0, 5.0),
        round(p(0.0, 0.0, 0.0), 0.0, 5.0),
        round(p(0.0, f64::NAN, 0.0), 1.0, 5.0),
    ] {
        assert!(wall_holds(&wall, &pt, mm(0.0)).is_err(), "{wall:?}");
    }
}

#[test]
fn a_polygons_area_is_its_closed_form_in_any_plane() {
    // A 38 x 20 rectangle in the plane y = 1: 760.
    let rect = [
        p(2.0, 1.0, 0.0),
        p(40.0, 1.0, 0.0),
        p(40.0, 1.0, 20.0),
        p(2.0, 1.0, 20.0),
    ];
    assert!((polygon_area(&rect).unwrap() - 760.0).abs() < 1e-9);
    // An L of legs 40 and 30, 2 wide, in z = 5: 40 * 2 + 28 * 2 = 136, either winding.
    let l = [
        p(0.0, 0.0, 5.0),
        p(40.0, 0.0, 5.0),
        p(40.0, 2.0, 5.0),
        p(2.0, 2.0, 5.0),
        p(2.0, 30.0, 5.0),
        p(0.0, 30.0, 5.0),
    ];
    assert!((polygon_area(&l).unwrap() - 136.0).abs() < 1e-9);
    let back: Vec<Point3> = l.iter().rev().copied().collect();
    assert!((polygon_area(&back).unwrap() - 136.0).abs() < 1e-9);
    // A triangle tilted 45 degrees: area 2 sqrt 2.
    let tri = [p(0.0, 0.0, 0.0), p(2.0, 0.0, 0.0), p(0.0, 2.0, 2.0)];
    assert!((polygon_area(&tri).unwrap() - 2.0 * 2.0_f64.sqrt()).abs() < 1e-12);
    assert_eq!(polygon_area(&rect[..2]).unwrap(), 0.0);
    assert_eq!(polygon_area(&[]).unwrap(), 0.0);
    // A unit square 2^27 from the origin has area 1, as it has at the origin.
    let square = |l: f64| {
        [
            p(l, l, 0.0),
            p(l + 1.0, l, 0.0),
            p(l + 1.0, l + 1.0, 0.0),
            p(l, l + 1.0, 0.0),
        ]
    };
    assert_eq!(polygon_area(&square(0.0)).unwrap(), 1.0);
    assert_eq!(polygon_area(&square(134_217_728.0)).unwrap(), 1.0);
    assert!(polygon_area(&[p(f64::NAN, 0.0, 0.0), rect[1], rect[2]]).is_err());
}

#[test]
fn a_point_drops_onto_a_plane_and_reads_back_in_its_frame() {
    // The plane x + y = 2: (3, 3, 7) is 4/sqrt 2 in front of it and drops to (1, 1, 7).
    let pl = plane([2.0, 0.0, 0.0], [4.0, 4.0, 0.0]);
    let d = onto_plane(&pl, p(3.0, 3.0, 7.0)).unwrap();
    assert!((d - p(1.0, 1.0, 7.0)).length() < 1e-12, "{d:?}");
    assert!(onto_plane(&pl, p(f64::NAN, 0.0, 0.0)).is_err());
    // A frame at (1, 1, 1) with axes x and z (so its normal is -y): (4, 9, -2) reads (3, -3),
    // and back to (4, 1, -2) (the point dropped onto the plane y = 1).
    let frame = Frame::new(p(1.0, 1.0, 1.0), v(0.0, -1.0, 0.0), v(1.0, 0.0, 0.0)).unwrap();
    assert!((frame.y().as_vec() - v(0.0, 0.0, 1.0)).length() < 1e-15);
    let uv = in_frame(&frame, p(4.0, 9.0, -2.0)).unwrap();
    assert!(
        (uv.x - 3.0).abs() < 1e-12 && (uv.y + 3.0).abs() < 1e-12,
        "{uv:?}"
    );
    let back = from_frame(&frame, uv).unwrap();
    assert!((back - p(4.0, 1.0, -2.0)).length() < 1e-12, "{back:?}");
    assert!(from_frame(&frame, Point2::new(f64::INFINITY, 0.0)).is_err());
    assert!(in_frame(&frame, p(f64::NAN, 0.0, 0.0)).is_err());
}

// ---- a joint of mid-planes ----------------------------------------------------------------

#[test]
fn two_perpendicular_planes_meet_in_the_line_through_both() {
    let n = near(100.0);
    // y = 1 and x = 1 meet along the z direction through (1, 1, anything).
    let a = plane([5.0, 1.0, 3.0], [0.0, 1.0, 0.0]);
    let b = plane([1.0, -4.0, 9.0], [2.0, 0.0, 0.0]);
    let Meeting::Line(line) = planes_meeting(&a, &b, &n).unwrap() else {
        panic!()
    };
    assert!(line.direction.z.abs() > 1.0 - 1e-12 && line.direction.x.abs() < 1e-12);
    assert!((line.point.x - 1.0).abs() < 1e-12 && (line.point.y - 1.0).abs() < 1e-12);
    // At an angle: x + y = 2 and x - y = 0 meet where x = y = 1.
    let s = 0.5_f64.sqrt();
    let c = plane([1.0, 1.0, 0.0], [s, s, 0.0]);
    let d = plane([7.0, 7.0, 0.0], [s, -s, 0.0]);
    let Meeting::Line(line) = planes_meeting(&c, &d, &n).unwrap() else {
        panic!()
    };
    assert!((line.point.x - 1.0).abs() < 1e-12 && (line.point.y - 1.0).abs() < 1e-12);
}

#[test]
fn a_joint_far_from_the_origin_is_as_exact_as_one_at_it() {
    // The pin built the line's point from the planes' offsets from the origin, which
    // cancels at a far corner. Measured from the first plane's own point it does not.
    let far = 1.0e9;
    let n = near(100.0);
    let a = plane([far + 5.0, 2.0, 3.0], [0.0, 1.0, 0.0]);
    let b = plane([far + 1.0, -4.0, 9.0], [2.0, 0.0, 0.0]);
    let Meeting::Line(line) = planes_meeting(&a, &b, &n).unwrap() else {
        panic!()
    };
    assert!(
        (line.point.x - (far + 1.0)).abs() < 1e-6 && (line.point.y - 2.0).abs() < 1e-12,
        "{line:?}"
    );
}

#[test]
fn parallel_planes_do_not_meet_whichever_way_they_face() {
    let n = near(100.0);
    let a = plane([0.0; 3], [0.0, 0.0, 1.0]);
    for z in [1.0, -1.0] {
        assert_eq!(
            planes_meeting(&a, &plane([0.0, 0.0, 2.0], [0.0, 0.0, z]), &n).unwrap(),
            Meeting::Parallel
        );
    }
}

#[test]
fn nearly_parallel_planes_meet_on_both_or_not_at_all() {
    // Whatever the tilt down to the last bits, a line given lies on both planes within the
    // coincidence, and a tilt the reach cannot see is parallel.
    let n = Near::new(mm(1e-9), mm(1.0)).unwrap();
    let mut lines = 0;
    for e in 20..=60 {
        let t = (0..e).fold(1.0, |x, _| x * 0.5);
        let c = plane([3.0, -7.0, 2.0], [1.0, 1.0, 0.0]);
        let d = plane([-5.0, 4.0, 9.0], [1.0 + t, 1.0, 0.0]);
        match planes_meeting(&c, &d, &n).unwrap() {
            Meeting::Line(l) => {
                lines += 1;
                for pl in [&c, &d] {
                    let u = pl.outward.length_and_unit().unwrap().1.as_vec();
                    assert!(
                        l.point.distance_along(pl.point, u).abs() <= 1e-9,
                        "2^-{e}: {l:?}"
                    );
                }
            }
            Meeting::Parallel | Meeting::Unplaceable => {}
            other => panic!("{other:?}"),
        }
    }
    assert!(lines > 0);
    // A tilt of 2^-27 over a metre is 7e-9 mm: within a coincidence of 1e-8, parallel.
    let a = plane([1.0, 0.0, 0.0], [1.0, 0.0, 0.0]);
    let b = plane([1.0, 0.0, 0.0], [1.0, (1.0 / 134_217_728.0), 0.0]);
    assert_eq!(
        planes_meeting(&a, &b, &Near::new(mm(1e-8), mm(1.0)).unwrap()).unwrap(),
        Meeting::Parallel
    );
    // Seen by a coincidence of 1e-10, they meet, in the line through (1, 0, 0) along z.
    let Meeting::Line(l) = planes_meeting(&a, &b, &Near::new(mm(1e-10), mm(1.0)).unwrap()).unwrap()
    else {
        panic!()
    };
    assert!((l.point - p(1.0, 0.0, 0.0)).length() < 1e-12 && l.direction.z.abs() > 1.0 - 1e-12);
}

fn sheet_a() -> [Point3; 4] {
    // Leg A's halfway plane y = 1: its faces y = 0 (x 0..40) and y = 2 (x 2..40) share x 2..40.
    [
        p(2.0, 1.0, 0.0),
        p(40.0, 1.0, 0.0),
        p(40.0, 1.0, 20.0),
        p(2.0, 1.0, 20.0),
    ]
}

fn sheet_b() -> [Point3; 4] {
    // Leg B's is x = 1: faces x = 0 (y 0..30) and x = 2 (y 2..30) share y 2..30.
    [
        p(1.0, 2.0, 0.0),
        p(1.0, 30.0, 0.0),
        p(1.0, 30.0, 20.0),
        p(1.0, 2.0, 20.0),
    ]
}

fn polygon(m: Moved) -> Vec<Point3> {
    match m {
        Moved::Polygon(c) => c,
        other => panic!("{other:?}"),
    }
}

#[test]
fn an_l_brackets_two_sheets_meet_on_their_joint_line_and_total_the_closed_form() {
    // A 2 mm L bracket, legs 40 and 30 mm outside, 20 mm wide.
    let n = near(50.0);
    let Meeting::Line(joint) = planes_meeting(
        &plane([0.0, 1.0, 0.0], [0.0, 1.0, 0.0]),
        &plane([1.0, 0.0, 0.0], [1.0, 0.0, 0.0]),
        &n,
    )
    .unwrap() else {
        panic!()
    };
    let (a0, b0) = (sheet_a(), sheet_b());
    // Before: 38 x 20 and 28 x 20.
    assert!((polygon_area(&a0).unwrap() - 760.0).abs() < 1e-9);
    assert!((polygon_area(&b0).unwrap() - 560.0).abs() < 1e-9);
    // The edges at x = 2 and y = 2 (corner 3 to corner 0) move on.
    let a = polygon(move_edge_onto_line(&a0, 3, &joint, mm(1e-5)).unwrap());
    let b = polygon(move_edge_onto_line(&b0, 3, &joint, mm(1e-5)).unwrap());
    assert!(
        a.iter().any(|q| (*q - p(1.0, 1.0, 0.0)).length() < 1e-12),
        "{a:?}"
    );
    assert!(
        b.iter().any(|q| (*q - p(1.0, 1.0, 20.0)).length() < 1e-12),
        "{b:?}"
    );
    assert!((polygon_area(&a).unwrap() - 780.0).abs() < 1e-9);
    assert!((polygon_area(&b).unwrap() - 580.0).abs() < 1e-9);
    assert!((polygon_area(&a).unwrap() + polygon_area(&b).unwrap() - 1360.0).abs() < 1e-9);
}

#[test]
fn an_edge_past_the_line_is_trimmed_back_to_it() {
    // A sheet 40 x 20 in z = 0; a wall stands on the line x = 5: the sheet is trimmed to 35.
    let sheet = [
        p(0.0, 0.0, 0.0),
        p(40.0, 0.0, 0.0),
        p(40.0, 20.0, 0.0),
        p(0.0, 20.0, 0.0),
    ];
    let wall = Line3 {
        point: p(5.0, 0.0, 0.0),
        direction: v(0.0, 1.0, 0.0),
    };
    let trimmed = polygon(move_edge_onto_line(&sheet, 3, &wall, mm(1e-5)).unwrap());
    assert!(
        (polygon_area(&trimmed).unwrap() - 700.0).abs() < 1e-9,
        "{trimmed:?}"
    );
}

#[test]
fn an_edge_that_cannot_move_onto_the_line_says_so_and_nonsense_is_refused() {
    let sheet = [
        p(0.0, 0.0, 0.0),
        p(40.0, 0.0, 0.0),
        p(40.0, 20.0, 0.0),
        p(0.0, 20.0, 0.0),
    ];
    let coin = mm(1e-5);
    let on = Line3 {
        point: p(5.0, 0.0, 0.0),
        direction: v(0.0, 1.0, 0.0),
    };
    // The edge at x = 0 is not parallel to a line along x.
    let across = Line3 {
        point: p(5.0, 0.0, 0.0),
        direction: v(1.0, 0.0, 0.0),
    };
    assert_eq!(
        move_edge_onto_line(&sheet, 3, &across, coin).unwrap(),
        Moved::EdgeNotParallel
    );
    // A line out of the sheet's plane does not meet its neighbouring edges.
    let off = Line3 {
        point: p(5.0, 0.0, 3.0),
        direction: v(0.0, 1.0, 0.0),
    };
    assert_eq!(
        move_edge_onto_line(&sheet, 3, &off, coin),
        Err(BodyqError::Refused(BodyqRefusal::OffPlane))
    );
    // A neighbouring edge parallel to the line: corners 0, 1, 2 collinear along x, and the
    // line along x at y = 3 beside the edge 1 -> 2.
    let flat = [
        p(0.0, 0.0, 0.0),
        p(10.0, 0.0, 0.0),
        p(20.0, 0.0, 0.0),
        p(20.0, 5.0, 0.0),
    ];
    let along_x = Line3 {
        point: p(0.0, 3.0, 0.0),
        direction: v(1.0, 0.0, 0.0),
    };
    assert_eq!(
        move_edge_onto_line(&flat, 1, &along_x, coin).unwrap(),
        Moved::NeighbourParallel { edge: 0 }
    );
    // Index past the corners, too few corners, non-finite input.
    let corners = Err(BodyqError::Refused(BodyqRefusal::Corners));
    assert_eq!(move_edge_onto_line(&sheet, 4, &on, coin), corners);
    assert_eq!(move_edge_onto_line(&sheet, usize::MAX, &on, coin), corners);
    assert_eq!(move_edge_onto_line(&sheet[..2], 0, &on, coin), corners);
    // The last edge (corner 3 back to corner 0) wraps round and is the one that moves.
    assert!(matches!(
        move_edge_onto_line(&sheet, 3, &on, coin).unwrap(),
        Moved::Polygon(_)
    ));
    let nan = [p(f64::NAN, 0.0, 0.0), sheet[1], sheet[2], sheet[3]];
    assert!(move_edge_onto_line(&nan, 3, &on, coin).is_err());
    let nan_line = Line3 {
        point: p(0.0, f64::INFINITY, 0.0),
        direction: v(0.0, 1.0, 0.0),
    };
    assert!(move_edge_onto_line(&sheet, 3, &nan_line, coin).is_err());
}

// ---- where a plane touches a cylindrical wall ---------------------------------------------

#[test]
fn an_outside_wall_and_a_bore_wrap_the_same_way_about_their_own_normals() {
    let z = v(0.0, 0.0, 1.0);
    let n = near(30.0);
    let frame = |w: Wrap| match w {
        Wrap::Frame(f) => f,
        other => panic!("{other:?}"),
    };
    let outside =
        frame(wall_frame(v(0.0, 1.0, 0.0), v(0.0, 10.0, 0.0), z, mm(10.0), true, &n).unwrap());
    assert_eq!(outside.radial, v(0.0, 1.0, 0.0));
    assert_eq!(outside.tangent, v(-1.0, 0.0, 0.0));
    // A bore takes the normal either way; the contact line is the same.
    for s in [1.0, -1.0] {
        let bore =
            frame(wall_frame(v(0.0, s, 0.0), v(0.0, 8.0, 0.0), z, mm(8.0), false, &n).unwrap());
        assert_eq!(bore.radial, v(0.0, 1.0, 0.0));
        assert_eq!(bore.tangent, v(1.0, 0.0, 0.0));
    }
}

#[test]
fn a_plane_that_does_not_touch_or_leans_has_no_frame_and_says_which() {
    let z = v(0.0, 0.0, 1.0);
    let n = near(30.0);
    let ask = |normal, w, r, convex| wall_frame(normal, w, z, mm(r), convex, &n).unwrap();
    // The wrong side of an outside wall, a plane clear of the wall.
    assert_eq!(
        ask(v(0.0, 1.0, 0.0), v(0.0, -10.0, 0.0), 10.0, true),
        Wrap::NotTouching
    );
    assert_eq!(
        ask(v(0.0, 1.0, 0.0), v(0.0, 9.0, 0.0), 10.0, true),
        Wrap::NotTouching
    );
    // A plane across the axis, and one leaning by more than the reach lets through
    // (1e-6 of 30 mm is 3e-5 mm, three coincidences).
    assert_eq!(
        ask(v(0.0, 0.0, 1.0), v(0.0, 0.0, 10.0), 8.0, false),
        Wrap::NotParallel
    );
    assert_eq!(
        ask(v(0.0, 1.0, 1e-6), v(0.0, 10.0, 0.0), 10.0, true),
        Wrap::NotParallel
    );
    assert!(matches!(
        ask(v(0.0, 1.0, 1e-7), v(0.0, 10.0, 0.0), 10.0, true),
        Wrap::Frame(_)
    ));
    // Touching within coincidence counts; past it does not.
    assert!(matches!(
        ask(v(0.0, 1.0, 0.0), v(0.0, 10.000_005, 0.0), 10.0, true),
        Wrap::Frame(_)
    ));
    assert_eq!(
        ask(v(0.0, 1.0, 0.0), v(0.0, 10.000_05, 0.0), 10.0, true),
        Wrap::NotTouching
    );
    assert!(wall_frame(v(0.0, 0.0, 0.0), Vec3::ZERO, z, mm(8.0), true, &n).is_err());
}

// ---- the caps ----------------------------------------------------------------------------

#[test]
fn a_plane_is_a_cap_at_a_height_when_square_to_it_and_there() {
    let n = near(100.0);
    let face = PlaneAt {
        origin: p(1.0, 1.0, 5.0),
        normal: v(0.0, 0.0, 1.0),
    };
    let cap = |h: f64| {
        is_square_plane_at_height(&face, p(0.0, 0.0, 0.0), v(0.0, 0.0, 1.0), mm(h), &n).unwrap()
    };
    assert!(cap(5.0) && cap(5.0 + 1e-6) && !cap(5.1) && !cap(0.0));
    // Either way round counts.
    let down = PlaneAt {
        normal: v(0.0, 0.0, -2.0),
        ..face
    };
    assert!(
        is_square_plane_at_height(&down, p(0.0, 0.0, 0.0), v(0.0, 0.0, 1.0), mm(5.0), &n).unwrap()
    );
    // A tilt the reach can see is not square: 1e-6 over 100 mm is 1e-4 mm.
    let tilted = PlaneAt {
        normal: v(1e-6, 0.0, 1.0),
        ..face
    };
    assert!(
        !is_square_plane_at_height(&tilted, p(0.0, 0.0, 0.0), v(0.0, 0.0, 1.0), mm(5.0), &n)
            .unwrap()
    );
    assert!(
        is_square_plane_at_height(
            &tilted,
            p(0.0, 0.0, 0.0),
            v(0.0, 0.0, 1.0),
            mm(5.0),
            &near(0.1)
        )
        .unwrap()
    );
    // Not a number, or no direction.
    assert!(
        is_square_plane_at_height(&face, p(f64::NAN, 0.0, 0.0), v(0.0, 0.0, 1.0), mm(5.0), &n)
            .is_err()
    );
    assert!(is_square_plane_at_height(&face, p(0.0, 0.0, 0.0), Vec3::ZERO, mm(5.0), &n).is_err());
    assert!(
        is_square_plane_at_height(&face, p(0.0, 0.0, 0.0), v(0.0, 0.0, 1.0), mm(f64::NAN), &n)
            .is_err()
    );
}

#[test]
fn a_point_or_a_face_is_on_a_plane_by_its_corners() {
    let n = near(1000.0);
    let z = v(0.0, 0.0, 1.0);
    assert!(is_point_on_plane(p(3.0, 4.0, 7.0 + 1e-6), z, mm(7.0), &n).unwrap());
    assert!(!is_point_on_plane(p(3.0, 4.0, 7.1), z, mm(7.0), &n).unwrap());
    let flat = [p(0.0, 0.0, 7.0), p(900.0, 0.0, 7.0), p(900.0, 900.0, 7.0)];
    assert!(is_face_on_plane(z, &flat, z, mm(7.0), &n).unwrap());
    // The normals alone do not settle a large face: two 4e-5 rad apart part by 0.036 across
    // 900 mm. Here the corners show it, and so does the reach.
    let lean = 4.0e-5;
    let leaning = [
        p(0.0, 0.0, 7.0),
        p(900.0, 0.0, 7.0 + 900.0 * lean),
        p(900.0, 900.0, 7.0),
    ];
    assert!(!is_face_on_plane(v(lean, 0.0, 1.0), &leaning, z, mm(7.0), &n).unwrap());
    // Facing the other way is not the same face.
    assert!(!is_face_on_plane(-z, &flat, z, mm(7.0), &n).unwrap());
    // No corners is no face.
    assert_eq!(
        is_face_on_plane(z, &[], z, mm(7.0), &n),
        Err(BodyqError::Refused(BodyqRefusal::Corners))
    );
}
