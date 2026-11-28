// Written for solidcorex. Apache-2.0.
//! The query and measure map's ports (research 74 §1.3, BW9) against closed forms: a block
//! and a cylinder of known faces, edges and normals.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

mod along_common;

use core::f64::consts::{FRAC_PI_2, FRAC_PI_4};

use along_common::doc;
use scx_bodyq::{
    BodyqError, BodyqRefusal, Near, ParallelTurnError, closest_to_point, edge_at, edge_distance,
    face_contains, face_line_hits, face_line_nearest, face_uv, faces_against, faces_at,
    outward_normal, rim_edges, turn_to_parallel,
};
use scx_brep::{Brep, EdgeId, EdgeShape, Entity, FaceId, UvSide};
use scx_num::budget::{Budget, Stop};
use scx_num::frame::Frame;
use scx_num::units::Length;
use scx_num::vector2d::Point2;
use scx_num::vector3d::{Point3, Vec3};
use scx_testkit::bodies::made::{block, cylinder};

fn mm(x: f64) -> Length {
    Length::from_mm(x)
}

fn pt(x: f64, y: f64, z: f64) -> Point3 {
    Point3::new(x, y, z)
}

fn v(x: f64, y: f64, z: f64) -> Vec3 {
    Vec3::new(x, y, z)
}

fn near(a: f64, b: f64) -> bool {
    (a - b).abs() < 1e-12
}

fn b() -> Budget {
    Budget::unlimited()
}

fn the_block() -> (Brep, scx_testkit::bodies::made::BlockFaces) {
    let m = block(&Frame::WORLD, [mm(10.0), mm(6.0), mm(4.0)], &doc()).unwrap();
    (m.body, m.faces)
}

/// The edge of the block between two corners.
fn edge_between(body: &Brep, p: Point3, q: Point3) -> EdgeId {
    body.edges()
        .find(|e| {
            let [s, t] = e.ends();
            let (s, t) = (
                body.vertex(s).unwrap().point(),
                body.vertex(t).unwrap().point(),
            );
            (s == p && t == q) || (s == q && t == p)
        })
        .unwrap()
        .id()
}

#[test]
fn an_edge_of_a_circle_has_its_radius_and_a_tangent_square_to_it() {
    let m = cylinder(&Frame::WORLD, mm(3.0), mm(7.0), &doc()).unwrap();
    let rim = m
        .body
        .edges()
        .find(|e| matches!(e.shape(), EdgeShape::Curve { curve, .. } if matches!(curve, scx_curve::Curve::Circle(_))))
        .unwrap()
        .id();
    for t in [0.0, 0.7, 2.0, 5.5] {
        let e = edge_at(&m.body, rim, t).unwrap();
        let r = (e.point.x.powi(2) + e.point.y.powi(2)).sqrt();
        assert!(near(r, 3.0), "{e:?}");
        // The derivative of a circle in its angle is radius long and square to the radius.
        assert!(near(e.tangent.length(), 3.0), "{e:?}");
        assert!((e.tangent.x * e.point.x + e.tangent.y * e.point.y).abs() < 1e-12);
    }
    assert_eq!(
        edge_at(&m.body, rim, f64::NAN),
        Err(BodyqError::Refused(BodyqRefusal::NotFinite))
    );
}

#[test]
fn an_edges_distance_is_to_its_interior_or_its_nearer_end() {
    let (body, _) = the_block();
    let e = edge_between(&body, pt(0.0, 0.0, 0.0), pt(10.0, 0.0, 0.0));
    // Over the middle: the distance to the line, sqrt(3^2 + 4^2).
    let d = edge_distance(&body, e, pt(5.0, -3.0, 4.0), &mut b()).unwrap();
    assert!(near(d.mm(), 5.0), "{d:?}");
    // Past the end: the distance to the end, 3.
    let d = edge_distance(&body, e, pt(13.0, 0.0, 0.0), &mut b()).unwrap();
    assert!(near(d.mm(), 3.0), "{d:?}");
    assert!(edge_distance(&body, EdgeId::from_raw(999), pt(0.0, 0.0, 0.0), &mut b()).is_err());
}

#[test]
fn a_faces_foot_gap_and_side_are_those_of_the_plane() {
    let (body, faces) = the_block();
    let stop = Stop::new();
    let on = face_uv(&body, faces.top, pt(3.0, 2.0, 4.5), &mut b(), &stop).unwrap();
    assert!(near(on.gap.mm(), 0.5) && near(on.point.z, 4.0), "{on:?}");
    assert_eq!(on.side, Some(UvSide::Inside));
    let off = face_uv(&body, faces.top, pt(15.0, 2.0, 4.0), &mut b(), &stop).unwrap();
    assert!(near(off.gap.mm(), 0.0), "{off:?}");
    assert_eq!(off.side, Some(UvSide::Outside));
    // On the plane but off the face is not on the face; above it is not either.
    assert!(!face_contains(&body, faces.top, pt(15.0, 2.0, 4.0), &mut b(), &stop).unwrap());
    assert!(!face_contains(&body, faces.top, pt(3.0, 2.0, 4.5), &mut b(), &stop).unwrap());
    assert!(face_contains(&body, faces.top, pt(3.0, 2.0, 4.0), &mut b(), &stop).unwrap());
    // On the face's own edge: it holds its edges.
    assert!(face_contains(&body, faces.top, pt(10.0, 3.0, 4.0), &mut b(), &stop).unwrap());
}

#[test]
fn faces_at_names_the_faces_a_point_is_on_nearest_first() {
    let (body, faces) = the_block();
    let stop = Stop::new();
    let at = |p| faces_at(&body, p, &mut b(), &stop).unwrap();
    // The middle of the top: one face, gap 0.
    let h = at(pt(3.0, 2.0, 4.0));
    assert_eq!(h.len(), 1);
    assert_eq!(h[0].1, faces.top);
    // A corner: the top and the two sides there, each at gap 0, by id.
    let h = at(pt(0.0, 0.0, 4.0));
    assert_eq!(h.len(), 3, "{h:?}");
    assert!(h.iter().all(|x| x.0.mm() < 1e-12));
    assert!(h.windows(2).all(|w| w[0].1 < w[1].1));
    // In the air or inside: none.
    assert!(at(pt(3.0, 2.0, 4.5)).is_empty());
    assert!(at(pt(3.0, 2.0, 2.0)).is_empty());
    assert!(at(pt(f64::MAX, 0.0, 0.0)).is_empty());
}

#[test]
fn the_outward_normal_of_each_block_face_is_its_axis() {
    let (body, faces) = the_block();
    let uv = Point2::new(1.0, 1.0);
    let n = |f: FaceId| outward_normal(&body, f, uv).unwrap().unwrap();
    assert!((n(faces.top) - v(0.0, 0.0, 1.0)).length() < 1e-15);
    assert!((n(faces.bottom) - v(0.0, 0.0, -1.0)).length() < 1e-15);
    // y = 0, x = size, y = size, x = 0 in the testkit's order.
    for (f, want) in faces.sides.iter().zip([
        v(0.0, -1.0, 0.0),
        v(1.0, 0.0, 0.0),
        v(0.0, 1.0, 0.0),
        v(-1.0, 0.0, 0.0),
    ]) {
        assert!((n(*f) - want).length() < 1e-15, "{f:?}");
    }
    assert!(outward_normal(&body, FaceId::from_raw(99), uv).is_err());
}

#[test]
fn a_face_against_the_run_of_a_curve_is_found_by_the_sum_of_its_votes() {
    let (body, faces) = the_block();
    let stop = Stop::new();
    let z = v(0.0, 0.0, 1.0);
    // A counter-clockwise run about +z along the bottom of the y = 0 wall: the wall faces
    // along d x z = -y, with it. Run the other way and it faces against.
    let on_wall = pt(5.0, 0.0, 2.0);
    let with = faces_against(&body, z, &[(on_wall, v(1.0, 0.0, 0.0))], &mut b(), &stop).unwrap();
    assert!(with.is_empty(), "{with:?}");
    let against =
        faces_against(&body, z, &[(on_wall, v(-1.0, 0.0, 0.0))], &mut b(), &stop).unwrap();
    assert_eq!(against, vec![faces.sides[0]]);
    // A sample whose direction is along the normal says nothing.
    let none = faces_against(&body, z, &[(on_wall, z)], &mut b(), &stop).unwrap();
    assert!(none.is_empty());
}

#[test]
fn the_closest_member_is_a_foot_inside_or_the_nearest_boundary_point() {
    let (body, faces) = the_block();
    let stop = Stop::new();
    let top = [faces.top];
    // Over the middle of the top: the foot, 5 above.
    let n = closest_to_point(&body, &top, &[], pt(3.0, 2.0, 9.0), &mut b(), &stop)
        .unwrap()
        .unwrap();
    assert_eq!(n.member, Entity::Face(faces.top));
    assert!(near(n.distance.mm(), 5.0) && n.exact, "{n:?}");
    // Beside the top: the nearest of its edge x = 10 at (10, 2, 4), sqrt(5^2 + 5^2); a plane
    // is exact.
    let n = closest_to_point(&body, &top, &[], pt(15.0, 2.0, 9.0), &mut b(), &stop)
        .unwrap()
        .unwrap();
    assert!(near(n.distance.mm(), 50.0_f64.sqrt()) && n.exact, "{n:?}");
    assert!((n.point - pt(10.0, 2.0, 4.0)).length() < 1e-12);
    // An edge and a face together: the nearer.
    let e = edge_between(&body, pt(0.0, 0.0, 0.0), pt(10.0, 0.0, 0.0));
    let n = closest_to_point(&body, &top, &[e], pt(5.0, -1.0, 0.0), &mut b(), &stop)
        .unwrap()
        .unwrap();
    assert_eq!(n.member, Entity::Edge(e));
    assert!(near(n.distance.mm(), 1.0));
    // Nothing asked: nothing.
    assert!(closest_to_point(&body, &[], &[], pt(0.0, 0.0, 0.0), &mut b(), &stop)
        .unwrap()
        .is_none());
}

#[test]
fn a_cylinder_wall_beyond_its_height_is_bounded_not_proved() {
    let m = cylinder(&Frame::WORLD, mm(3.0), mm(7.0), &doc()).unwrap();
    let stop = Stop::new();
    let wall = [m.faces.wall];
    // Level with the wall: the foot is inside, 17 away.
    let n = closest_to_point(&m.body, &wall, &[], pt(20.0, 0.0, 5.0), &mut b(), &stop)
        .unwrap()
        .unwrap();
    assert!(near(n.distance.mm(), 17.0) && n.exact, "{n:?}");
    // Above the rim: the true nearest is the rim point (3, 0, 7) at sqrt(17^2 + 3^2); the
    // method says only that it is a bound.
    let n = closest_to_point(&m.body, &wall, &[], pt(20.0, 0.0, 10.0), &mut b(), &stop)
        .unwrap()
        .unwrap();
    assert!(near(n.distance.mm(), (17.0_f64.powi(2) + 9.0).sqrt()), "{n:?}");
    assert!(!n.exact);
}

#[test]
fn a_line_meets_a_face_where_the_geometry_says() {
    let (body, faces) = the_block();
    let stop = Stop::new();
    // Up through the top at (3, 2): one place, 4 along from z = 0, inside.
    let h = face_line_hits(&body, faces.top, pt(3.0, 2.0, 0.0), v(0.0, 0.0, 2.0), &mut b(), &stop)
        .unwrap();
    assert_eq!(h.len(), 1, "{h:?}");
    assert!(near(h[0].t, 4.0) && (h[0].point - pt(3.0, 2.0, 4.0)).length() < 1e-12);
    // Through the plane of the top but off the face: none; along the face: none.
    assert!(face_line_hits(&body, faces.top, pt(15.0, 2.0, 0.0), v(0.0, 0.0, 1.0), &mut b(), &stop)
        .unwrap()
        .is_empty());
    assert!(face_line_hits(&body, faces.top, pt(3.0, 2.0, 4.0), v(1.0, 0.0, 0.0), &mut b(), &stop)
        .unwrap()
        .is_empty());
    assert_eq!(
        face_line_hits(&body, faces.top, pt(0.0, 0.0, 0.0), v(0.0, 0.0, 0.0), &mut b(), &stop),
        Err(BodyqError::Refused(BodyqRefusal::NoDirection))
    );
}

#[test]
fn a_line_through_a_cylinders_axis_meets_the_wall_both_ways_and_ties_go_ahead() {
    let m = cylinder(&Frame::WORLD, mm(3.0), mm(7.0), &doc()).unwrap();
    let stop = Stop::new();
    let from = pt(0.0, 0.0, 3.5);
    let h = face_line_hits(&m.body, m.faces.wall, from, v(0.0, 1.0, 0.0), &mut b(), &stop).unwrap();
    assert_eq!(h.len(), 2, "{h:?}");
    assert!(near(h[0].t, -3.0) && near(h[1].t, 3.0), "{h:?}");
    assert!((h[0].point - pt(0.0, -3.0, 3.5)).length() < 1e-12);
    // Both are 3 away: the tie goes ahead.
    let p = face_line_nearest(
        &m.body,
        m.faces.wall,
        from,
        v(0.0, 1.0, 0.0),
        mm(1e-9),
        &mut b(),
        &stop,
    )
    .unwrap()
    .unwrap();
    assert!((p - pt(0.0, 3.0, 3.5)).length() < 1e-12, "{p:?}");
    // Off the axis toward +y by 1: behind is 2 away, ahead 4: the nearer, behind.
    let p = face_line_nearest(
        &m.body,
        m.faces.wall,
        pt(0.0, 1.0, 3.5),
        v(0.0, 1.0, 0.0),
        mm(1e-9),
        &mut b(),
        &stop,
    )
    .unwrap()
    .unwrap();
    assert!((p - pt(0.0, 3.0, 3.5)).length() < 1e-12, "{p:?}");
    // A line outside the wall's height misses it.
    assert!(face_line_nearest(
        &m.body,
        m.faces.wall,
        pt(0.0, 0.0, 9.0),
        v(0.0, 1.0, 0.0),
        mm(1e-9),
        &mut b(),
        &stop
    )
    .unwrap()
    .is_none());
}

#[test]
fn the_rim_of_a_set_of_faces_is_the_edges_between_it_and_the_rest() {
    let (body, faces) = the_block();
    let rim = |fs: &[FaceId]| rim_edges(&body, fs).unwrap();
    assert_eq!(rim(&[faces.top]).len(), 4);
    // Top and bottom: their eight edges.
    assert_eq!(rim(&[faces.top, faces.bottom]).len(), 8);
    // One wall and the top: 4 + 4 - 2 (the shared edge counted twice) = 6.
    assert_eq!(rim(&[faces.top, faces.sides[0]]).len(), 6);
    // Every face: none. No face: none.
    let all: Vec<FaceId> = body.faces().map(|f| f.id()).collect();
    assert!(rim(&all).is_empty() && rim(&[]).is_empty());
    assert!(rim_edges(&body, &[FaceId::from_raw(99)]).is_err());
    // A cylinder's wall has its two rims; its seam is used by the wall alone, twice.
    let m = cylinder(&Frame::WORLD, mm(3.0), mm(7.0), &doc()).unwrap();
    assert_eq!(rim_edges(&m.body, &[m.faces.wall]).unwrap().len(), 2);
    assert!(rim_edges(&m.body, &[m.faces.wall, m.faces.top, m.faces.bottom])
        .unwrap()
        .is_empty());
}

#[test]
fn the_turn_that_makes_a_square_direction_parallel_is_the_angle_between_them() {
    let n = Near::new(mm(1e-6), mm(10.0)).unwrap();
    let (axis, side) = (v(0.0, 0.0, 1.0), v(1.0, 0.0, 0.0));
    let turn = |line| turn_to_parallel(axis, side, line, &n);
    let at = |t: Result<f64, ParallelTurnError>, want: f64| {
        let t = t.unwrap();
        assert!((t - want).abs() < 1e-12, "{t} vs {want}");
    };
    at(turn(v(1.0, 0.0, 0.0)), 0.0);
    at(turn(v(1.0, 1.0, 0.0)), FRAC_PI_4);
    at(turn(v(1.0, -1.0, 0.0)), -FRAC_PI_4);
    // A line has no sense: (-1, 0, 0) is (1, 0, 0), and +-y are both a quarter turn, folded
    // into (-pi/2, pi/2].
    at(turn(v(-1.0, 0.0, 0.0)), 0.0);
    at(turn(v(0.0, 1.0, 0.0)), FRAC_PI_2);
    at(turn(v(0.0, -1.0, 0.0)), FRAC_PI_2);
    // The line's height off the plane does not matter, only its direction square to the axis.
    at(turn(v(2.0, 2.0, 5.0)), FRAC_PI_4);
    // Vectors need not be unit.
    at(turn_to_parallel(v(0.0, 0.0, 7.0), v(3.0, 0.0, 0.0), v(4.0, 4.0, 0.0), &n), FRAC_PI_4);
    // Each refusal by name.
    assert_eq!(turn(v(0.0, 0.0, 3.0)), Err(ParallelTurnError::AlongAxis));
    assert_eq!(
        turn_to_parallel(axis, v(0.0, 0.0, 1.0), v(1.0, 0.0, 0.0), &n),
        Err(ParallelTurnError::NotASide)
    );
    assert_eq!(
        turn_to_parallel(axis, side, v(0.0, 0.0, 0.0), &n),
        Err(ParallelTurnError::Refused(BodyqRefusal::NoDirection))
    );
    assert_eq!(
        turn_to_parallel(axis, side, v(f64::NAN, 0.0, 1.0), &n),
        Err(ParallelTurnError::Refused(BodyqRefusal::NotFinite))
    );
}

#[test]
fn the_square_and_along_axis_tests_scale_with_the_reach() {
    // A normal tilted 1e-4 off square: square over a short reach (1e-4 * 1 <= 1e-3) and
    // not square over a long one (1e-4 * 100 > 1e-3).
    let (axis, side) = (v(0.0, 0.0, 1.0), v(1.0, 0.0, 1e-4));
    let line = v(1.0, 1.0, 0.0);
    let short = Near::new(mm(1e-3), mm(1.0)).unwrap();
    let long = Near::new(mm(1e-3), mm(100.0)).unwrap();
    assert!(turn_to_parallel(axis, side, line, &short).is_ok());
    assert_eq!(
        turn_to_parallel(axis, side, line, &long),
        Err(ParallelTurnError::NotASide)
    );
}
