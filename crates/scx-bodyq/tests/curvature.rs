// Written for solidcorex. Apache-2.0.
//! Combs and principal curvatures against the closed forms of the quadrics (research 74 §52):
//! a cylinder of radius r bends 1 / r round and 0 along, a sphere 1 / r both ways, and the
//! saddle on a torus's inner equator reads +1 / r round the tube and -1 / (R - r) along it.
#![allow(clippy::unwrap_used, clippy::indexing_slicing, clippy::float_cmp)]

mod along_common;

use along_common::doc;
use scx_bodyq::{BodyqError, BodyqRefusal, Running, edge_comb, iso_comb, principal_curvatures};
use scx_brep::{Brep, EdgeId, EdgeShape, FaceId};
use scx_curve::Curve;
use scx_num::budget::{Budget, Stop};
use scx_num::frame::Frame;
use scx_num::units::Length;
use scx_num::vector3d::{Point3, Vec3};
use scx_testkit::bodies::made::{cylinder, sphere, torus};

const R: f64 = 3.0;

fn unlimited() -> (Budget, Stop) {
    (Budget::unlimited(), Stop::new())
}

fn mm(x: f64) -> Length {
    Length::from_mm(x)
}

fn near(a: f64, b: f64, tol: f64) {
    assert!((a - b).abs() <= tol, "{a} vs {b}");
}

fn can() -> (Brep, scx_testkit::bodies::made::CylinderFaces) {
    let m = cylinder(&Frame::WORLD, mm(R), mm(7.0), &doc()).unwrap();
    (m.body, m.faces)
}

fn circles(body: &Brep) -> Vec<EdgeId> {
    body.edges()
        .filter(|e| {
            matches!(
                e.shape(),
                EdgeShape::Curve {
                    curve: Curve::Circle(_),
                    ..
                }
            )
        })
        .map(scx_brep::Edge::id)
        .collect()
}

#[test]
fn a_circle_s_comb_points_at_its_centre_with_length_one_over_r() {
    let (body, _) = can();
    let (mut b, s) = unlimited();
    let rims = circles(&body);
    assert_eq!(rims.len(), 2);
    for e in rims {
        let comb = edge_comb(&body, e, 12, &mut b, &s).unwrap();
        // a closed edge's end is its start: twelve teeth, twelve distinct places
        assert_eq!(comb.len(), 12);
        let ats: Vec<f64> = comb.iter().map(|t| t.at).collect();
        assert!(ats.windows(2).all(|w| w[0] < w[1]));
        let EdgeShape::Curve { interval, .. } = body.edge(e).unwrap().shape() else {
            panic!()
        };
        assert_eq!(ats[0], interval.min());
        assert!(ats[11] < interval.max());
        for t in &comb {
            near(t.curvature.length(), 1.0 / R, 1e-12);
            // towards the axis: the point's xy reversed, scaled
            let (x, y) = (t.point.x, t.point.y);
            near(t.curvature.x, -x / (R * R), 1e-12);
            near(t.curvature.y, -y / (R * R), 1e-12);
            near(t.curvature.z, 0.0, 1e-12);
        }
    }
}

#[test]
fn a_straight_edge_combs_to_nothing_and_includes_both_ends() {
    let (body, _) = can();
    let seam = along_common::seams(&body);
    assert_eq!(seam.len(), 1);
    let (mut b, s) = unlimited();
    let comb = edge_comb(&body, seam[0], 5, &mut b, &s).unwrap();
    assert_eq!(comb.len(), 5);
    let EdgeShape::Curve { interval, .. } = body.edge(seam[0]).unwrap().shape() else {
        panic!()
    };
    assert_eq!(comb[0].at, interval.min());
    assert_eq!(comb[4].at, interval.max());
    for t in comb {
        assert_eq!(t.curvature, Vec3::new(0.0, 0.0, 0.0));
    }
}

fn wall_comb(running: Running, fixed: f64, span: [f64; 2], teeth: usize) -> Vec<scx_bodyq::Tooth> {
    let (body, f) = can();
    let (mut b, s) = unlimited();
    iso_comb(&body, f.wall, running, fixed, span, teeth, &mut b, &s).unwrap()
}

#[test]
fn a_cylinder_s_parameter_lines_bend_by_their_closed_forms() {
    // u runs round the axis: normal curvature 1 / r, along the normal; v runs along it: 0
    let round = wall_comb(Running::U, 2.0, [0.0, 4.0], 9);
    assert_eq!(round.len(), 9);
    assert_eq!(round[0].at, 0.0);
    assert_eq!(round[8].at, 4.0);
    for t in &round {
        near(t.curvature.length(), 1.0 / R, 1e-12);
        near(t.point.z, 2.0, 1e-12);
        // radial, inward or outward by the surface's sense, never along z
        near(t.curvature.z, 0.0, 1e-12);
        let radial = (t.curvature.x * t.point.x + t.curvature.y * t.point.y).abs();
        near(radial, 1.0 / R * R, 1e-12);
    }
    let along = wall_comb(Running::V, 1.0, [0.0, 7.0], 4);
    assert_eq!(along.len(), 4);
    for t in along {
        assert_eq!(t.curvature.length(), 0.0);
    }
}

#[test]
fn a_sphere_s_poles_have_no_tooth_and_the_rest_bend_by_one_over_r() {
    let m = sphere(&Frame::WORLD, mm(2.0), &doc()).unwrap();
    let (mut b, s) = unlimited();
    // the sphere's v runs over [0, 1] from the south pole to the north: both are poles.
    // v runs along a meridian over the whole range: its ends are the poles
    let comb = iso_comb(
        &m.body,
        m.faces.face,
        Running::V,
        0.5,
        [0.0, 1.0],
        5,
        &mut b,
        &s,
    )
    .unwrap();
    assert_eq!(comb.len(), 3, "{comb:?}");
    for t in &comb {
        near(t.curvature.length(), 0.5, 1e-12);
    }
    let comb = iso_comb(
        &m.body,
        m.faces.face,
        Running::U,
        0.0,
        [0.0, 1.0],
        3,
        &mut b,
        &s,
    )
    .unwrap();
    assert!(comb.is_empty(), "{comb:?}");
    let comb = iso_comb(
        &m.body,
        m.faces.face,
        Running::U,
        0.3,
        [0.0, 1.0],
        7,
        &mut b,
        &s,
    )
    .unwrap();
    assert_eq!(comb.len(), 7);
    // a parallel of latitude 0.3 has radius 2 cos 0.3; its normal curvature is 1 / r_sphere
    for t in comb {
        near(t.curvature.length(), 0.5, 1e-12);
    }
}

#[test]
fn comb_refusals_and_work() {
    let (body, f) = can();
    let e = circles(&body)[0];
    let (mut b, s) = unlimited();
    assert_eq!(
        edge_comb(&body, e, 1, &mut b, &s),
        Err(BodyqError::Refused(BodyqRefusal::Count))
    );
    assert_eq!(
        iso_comb(&body, f.wall, Running::U, 0.0, [0.0, 1.0], 0, &mut b, &s),
        Err(BodyqError::Refused(BodyqRefusal::Count))
    );
    assert_eq!(
        iso_comb(
            &body,
            f.wall,
            Running::U,
            f64::NAN,
            [0.0, 1.0],
            3,
            &mut b,
            &s
        ),
        Err(BodyqError::Refused(BodyqRefusal::NotFinite))
    );
    let sphere = sphere(&Frame::WORLD, mm(1.0), &doc()).unwrap();
    let collapsed = sphere
        .body
        .edges()
        .find(|e| matches!(e.shape(), EdgeShape::Collapsed))
        .map(scx_brep::Edge::id);
    if let Some(c) = collapsed {
        assert_eq!(
            edge_comb(&sphere.body, c, 4, &mut b, &s),
            Err(BodyqError::Refused(BodyqRefusal::Collapsed { edge: c }))
        );
    }
    // one unit a station
    let mut one = Budget::unlimited();
    edge_comb(&body, e, 6, &mut one, &s).unwrap();
    assert_eq!(one.used(), 6);
    let mut tight = Budget::new(5);
    assert!(matches!(
        edge_comb(&body, e, 6, &mut tight, &s),
        Err(BodyqError::Exhausted { .. })
    ));
    let stop = Stop::new();
    stop.set();
    assert_eq!(
        edge_comb(&body, e, 6, &mut Budget::unlimited(), &stop),
        Err(BodyqError::Cancelled)
    );
}

fn principal(body: &Brep, f: FaceId, p: [f64; 3]) -> scx_bodyq::Principal {
    let (mut b, s) = unlimited();
    principal_curvatures(body, f, Point3::new(p[0], p[1], p[2]), &mut b, &s).unwrap()
}

#[test]
fn a_cylinder_bends_round_and_not_along() {
    let (body, f) = can();
    // a point off the wall: the foot is the radial projection
    let p = principal(&body, f.wall, [4.0 * 0.6, 4.0 * 0.8, 2.0]);
    // convex positive against the outward normal: round the axis +1 / r, along it 0
    near(p.k1, 1.0 / R, 1e-12);
    near(p.k2, 0.0, 1e-12);
    near(p.gaussian, 0.0, 1e-12);
    near(p.mean, 0.5 / R, 1e-12);
    near(p.d1.z, 0.0, 1e-12);
    near(p.d2.z.abs(), 1.0, 1e-12);
    near(p.normal.x, 0.6, 1e-12);
    near(p.normal.y, 0.8, 1e-12);
    near(p.point.x, 0.6 * R, 1e-12);
}

#[test]
fn a_sphere_seen_from_outside_bends_by_plus_one_over_r_twice() {
    let m = sphere(&Frame::WORLD, mm(2.5), &doc()).unwrap();
    let p = principal(&m.body, m.faces.face, [1.0, 1.0, 1.0]);
    near(p.k1, 0.4, 1e-12);
    near(p.k2, 0.4, 1e-12);
    near(p.gaussian, 0.16, 1e-12);
    near(p.mean, 0.4, 1e-12);
    near(p.d1.dot(p.d2), 0.0, 1e-12);
    near(p.d1.dot(p.normal), 0.0, 1e-12);
}

#[test]
fn a_poles_curvature_is_refused() {
    let m = sphere(&Frame::WORLD, mm(2.0), &doc()).unwrap();
    let (mut b, s) = unlimited();
    let got = principal_curvatures(
        &m.body,
        m.faces.face,
        Point3::new(0.0, 0.0, 5.0),
        &mut b,
        &s,
    );
    assert_eq!(
        got,
        Err(BodyqError::Refused(BodyqRefusal::Singular {
            face: m.faces.face
        }))
    );
    assert_eq!(
        principal_curvatures(
            &m.body,
            m.faces.face,
            Point3::new(f64::NAN, 0.0, 0.0),
            &mut b,
            &s
        ),
        Err(BodyqError::Refused(BodyqRefusal::NotFinite))
    );
}

#[test]
fn a_torus_is_a_saddle_inside_and_convex_outside() {
    let (big, small) = (5.0, 1.5);
    let m = torus(&Frame::WORLD, mm(big), mm(small), &doc()).unwrap();
    let f = m.faces.face;
    // the inner equator: round the tube +1 / r, along the equator -1 / (R - r)
    let inner = principal(&m.body, f, [big - small, 0.0, 0.0]);
    near(inner.k1, 1.0 / small, 1e-12);
    near(inner.k2, -1.0 / (big - small), 1e-12);
    near(inner.gaussian, -1.0 / (small * (big - small)), 1e-12);
    // k1's direction is round the tube: in the xz plane; k2's along the equator: y
    near(inner.d1.y, 0.0, 1e-12);
    near(inner.d2.y.abs(), 1.0, 1e-12);
    // the outer equator: both convex
    let outer = principal(&m.body, f, [big + small + 0.5, 0.0, 0.0]);
    near(outer.k1, 1.0 / small, 1e-12);
    near(outer.k2, 1.0 / (big + small), 1e-12);
    near(outer.d1.y, 0.0, 1e-12);
}
