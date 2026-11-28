// Written for solidcorex. Apache-2.0.
//! `tangent_all_along` against closed forms (research 74 §25, BW4): the pin's tangent-edge
//! cases, each judged against the dihedral angle the body's construction gives (not the
//! pin's output), and the refusals, budget and determinism.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

mod along_common;

use std::f64::consts::{FRAC_PI_2, PI};

use along_common::{
    between, deg, doc, extrude_polygon, seams, span_of, stadium, tangency, tilted_sheet,
};
use scx_bodyq::{BodyqError, BodyqRefusal, BodyqUnsupported, Tangency, tangent_all_along};
use scx_brep::{EdgeId, Entity};
use scx_num::budget::{Budget, Stop};
use scx_num::fmath;
use scx_num::frame::Frame;
use scx_num::units::Length;
use scx_testkit::bodies::made::{block, cone, cylinder, sphere, torus};

/// The pin's "smooth" angle: half a degree.
const SMOOTH: f64 = 0.5 * PI / 180.0;

/// Asserts the edge's angle is `want` to within `tol`: not tangent at `want - tol`, tangent
/// at `want + tol` (with a bound in between).
fn parts_by(body: &scx_brep::Brep, e: EdgeId, want: f64, tol: f64) {
    match tangency(body, e, want - tol) {
        Tangency::NotTangent { angle, .. } => {
            assert!(
                angle.lo <= want + tol && angle.hi >= want - tol,
                "{angle:?} vs {want}"
            );
        }
        other => panic!("{e}: at {} expected NotTangent, got {other:?}", want - tol),
    }
    match tangency(body, e, (want + tol).min(PI - 1e-15)) {
        Tangency::Tangent { bound } => {
            assert!(
                bound >= want - tol && bound <= want + tol,
                "{bound} vs {want}"
            );
        }
        other => panic!("{e}: at {} expected Tangent, got {other:?}", want + tol),
    }
}

#[test]
fn box_edges_are_square() {
    let m = block(
        &Frame::WORLD,
        [
            Length::from_mm(10.0),
            Length::from_mm(6.0),
            Length::from_mm(4.0),
        ],
        &doc(),
    )
    .unwrap();
    assert_eq!(m.body.edges().len(), 12);
    for e in m.body.edges() {
        parts_by(&m.body, e.id(), FRAC_PI_2, 1e-12);
        assert!(matches!(
            tangency(&m.body, e.id(), SMOOTH),
            Tangency::NotTangent { .. }
        ));
    }
}

#[test]
fn wedge_prism_edges_turn_by_their_angles() {
    // an equilateral prism: walls 120 degrees apart, caps square to them
    let h = 3.0_f64.sqrt() * 5.0;
    let (body, f) = extrude_polygon(&[[0.0, 0.0], [10.0, 0.0], [5.0, h]], 6.0);
    for (i, &w) in f.walls[0].iter().enumerate() {
        let next = f.walls[0][(i + 1) % 3];
        for e in between(&body, w, next) {
            parts_by(&body, e, deg(120.0), 1e-12);
        }
        for cap in [f.start, f.end] {
            for e in between(&body, w, cap) {
                parts_by(&body, e, FRAC_PI_2, 1e-12);
            }
        }
    }
}

#[test]
fn thin_wedge_ridge_and_base() {
    // apex half-angle 2.5 degrees: the ridge turns by 175, each base edge by 92.5
    let half = deg(2.5);
    let tip = [0.0, 20.0];
    let x = 20.0 * fmath::tan(half);
    let (body, f) = extrude_polygon(&[[-x, 0.0], [x, 0.0], tip], 3.0);
    let [base, right, left] = [f.walls[0][0], f.walls[0][1], f.walls[0][2]];
    for e in between(&body, right, left) {
        parts_by(&body, e, deg(175.0), 1e-9);
    }
    for e in between(&body, base, right)
        .into_iter()
        .chain(between(&body, left, base))
    {
        parts_by(&body, e, deg(92.5), 1e-9);
    }
}

#[test]
fn cylinder_caps_are_square_and_the_seam_tangent() {
    let m = cylinder(
        &Frame::WORLD,
        Length::from_mm(3.0),
        Length::from_mm(7.0),
        &doc(),
    )
    .unwrap();
    let f = m.faces;
    for cap in [f.bottom, f.top] {
        let es = between(&m.body, f.wall, cap);
        assert!(!es.is_empty());
        for e in es {
            parts_by(&m.body, e, FRAC_PI_2, 1e-9);
            assert!(matches!(
                tangency(&m.body, e, SMOOTH),
                Tangency::NotTangent { .. }
            ));
        }
    }
    let s = seams(&m.body);
    assert!(!s.is_empty());
    for e in s {
        assert_eq!(tangency(&m.body, e, 0.0), Tangency::Tangent { bound: 0.0 });
    }
}

#[test]
fn frustum_rims_turn_by_the_slant() {
    // radii 5 and 2 over height 4: the slant's normal is (4, 3) / 5
    let m = cone(
        &Frame::WORLD,
        [Length::from_mm(5.0), Length::from_mm(2.0)],
        Length::from_mm(4.0),
        &doc(),
    )
    .unwrap();
    let f = m.faces;
    for e in between(&m.body, f.wall, f.bottom.unwrap()) {
        parts_by(&m.body, e, fmath::acos(-0.6), 1e-9);
    }
    for e in between(&m.body, f.wall, f.top.unwrap()) {
        parts_by(&m.body, e, fmath::acos(0.6), 1e-9);
    }
}

#[test]
fn shallow_frustum_by_its_slope() {
    // a frustum whose top rim turns by `a` from the top: tangent at any angle above `a`
    for a in [deg(1.0), deg(10.0), deg(45.0), deg(80.0)] {
        let h = 3.0 * fmath::tan(a);
        let m = cone(
            &Frame::WORLD,
            [Length::from_mm(5.0), Length::from_mm(2.0)],
            Length::from_mm(h),
            &doc(),
        )
        .unwrap();
        let f = m.faces;
        // top normal z, the wall's (h, 3) / |(h, 3)| in (radial, z): the angle is atan(h / 3)
        let want = fmath::atan2(h, 3.0);
        for e in between(&m.body, f.wall, f.top.unwrap()) {
            parts_by(&m.body, e, want, 1e-9);
            let verdict = tangency(&m.body, e, SMOOTH);
            assert!(
                matches!(verdict, Tangency::NotTangent { .. }),
                "{a}: {verdict:?}"
            );
        }
    }
}

#[test]
fn oblique_wall_angle_varies_along() {
    // a cylinder tilted 30 degrees, cut by z = 0: along the ellipse the wall meets the plane
    // between 60 and 120 degrees
    let (body, rim) = tilted_sheet(deg(30.0), 2.0);
    parts_by(&body, rim, deg(120.0), 1e-6);
    for (theta, want) in [
        (125.0, true),
        (119.0, false),
        (90.0, false),
        (60.0, false),
        (59.0, false),
    ] {
        let got = tangency(&body, rim, deg(theta));
        assert_eq!(
            matches!(got, Tangency::Tangent { .. }),
            want,
            "{theta}: {got:?}"
        );
    }
}

#[test]
fn stadium_line_arc_edges_are_tangent() {
    let (body, f) = stadium();
    let walls = &f.walls[0];
    for (i, &w) in walls.iter().enumerate() {
        let next = walls[(i + 1) % walls.len()];
        for e in between(&body, w, next) {
            match tangency(&body, e, SMOOTH) {
                Tangency::Tangent { bound } => assert!(bound < 1e-9, "{bound}"),
                other => panic!("{e}: {other:?}"),
            }
        }
        for cap in [f.start, f.end] {
            for e in between(&body, w, cap) {
                parts_by(&body, e, FRAC_PI_2, 1e-9);
            }
        }
    }
}

#[test]
fn kinked_walls_by_their_kink() {
    // two walls 0.3 degrees apart
    let k = deg(0.3);
    let (body, f) = extrude_polygon(
        &[
            [0.0, 0.0],
            [10.0, 0.0],
            [20.0, 10.0 * fmath::tan(k)],
            [20.0, 10.0],
            [0.0, 10.0],
        ],
        2.0,
    );
    let es = between(&body, f.walls[0][0], f.walls[0][1]);
    assert_eq!(es.len(), 1);
    let e = es[0];
    assert!(matches!(
        tangency(&body, e, SMOOTH),
        Tangency::Tangent { .. }
    ));
    assert!(matches!(
        tangency(&body, e, deg(0.2)),
        Tangency::NotTangent { .. }
    ));
    let (lo, hi) = span_of(&body, e);
    match tangency(&body, e, k) {
        Tangency::Undecided { at, angle } => {
            assert!(at > lo && at < hi);
            assert!(angle.lo <= k && angle.hi >= k, "{angle:?}");
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn closed_surface_seams_are_tangent() {
    let s = sphere(&Frame::WORLD, Length::from_mm(2.0), &doc()).unwrap();
    let t = torus(
        &Frame::WORLD,
        Length::from_mm(5.0),
        Length::from_mm(1.5),
        &doc(),
    )
    .unwrap();
    let c = cone(
        &Frame::WORLD,
        [Length::from_mm(3.0), Length::from_mm(0.0)],
        Length::from_mm(4.0),
        &doc(),
    )
    .unwrap();
    for body in [&s.body, &t.body, &c.body] {
        let es = seams(body);
        assert!(!es.is_empty());
        for e in es {
            assert_eq!(tangency(body, e, 0.0), Tangency::Tangent { bound: 0.0 });
        }
    }
}

#[test]
fn refusals() {
    let m = block(&Frame::WORLD, [Length::from_mm(1.0); 3], &doc()).unwrap();
    let e = m.body.edges().next().unwrap().id();
    let run = |b: &scx_brep::Brep, e, a| {
        tangent_all_along(b, e, a, &mut Budget::unlimited(), &Stop::new())
    };
    for bad in [-1.0, f64::NAN, f64::INFINITY, PI, 4.0] {
        assert_eq!(
            run(&m.body, e, bad),
            Err(BodyqError::Refused(BodyqRefusal::Tolerance)),
            "{bad}"
        );
    }
    let gone = EdgeId::from_raw(999);
    assert_eq!(
        run(&m.body, gone, SMOOTH),
        Err(BodyqError::Refused(BodyqRefusal::NoEntity {
            entity: Entity::Edge(gone)
        }))
    );
    let (sheet, rim) = tilted_sheet(deg(30.0), 2.0);
    let open = sheet
        .edges()
        .find(|x| x.fins().len() == 1)
        .map(scx_brep::Edge::id)
        .unwrap();
    assert_eq!(
        run(&sheet, open, SMOOTH),
        Err(BodyqError::Refused(BodyqRefusal::OneSided { edge: open }))
    );
    assert!(run(&sheet, rim, SMOOTH).is_ok());
}

#[test]
fn nurbs_face_is_pending() {
    let (body, e, face) = along_common::nurbs_meets_plane();
    assert_eq!(
        tangent_all_along(&body, e, SMOOTH, &mut Budget::unlimited(), &Stop::new()),
        Err(BodyqError::Unsupported(BodyqUnsupported::Nurbs {
            at: Entity::Face(face)
        }))
    );
}

#[test]
fn nurbs_edge_curve_between_planes() {
    // a NURBS curve with interior knots, along the crease of two planes 90 degrees apart
    let (body, e) = along_common::nurbs_crease();
    parts_by(&body, e, FRAC_PI_2, 1e-9);
}

#[test]
fn budget_and_stop() {
    let (body, rim) = tilted_sheet(deg(30.0), 2.0);
    let mut tiny = Budget::new(3);
    assert!(matches!(
        tangent_all_along(&body, rim, deg(121.0), &mut tiny, &Stop::new()),
        Err(BodyqError::Exhausted { .. })
    ));
    let stop = Stop::new();
    stop.set();
    assert_eq!(
        tangent_all_along(&body, rim, deg(121.0), &mut Budget::unlimited(), &stop),
        Err(BodyqError::Cancelled)
    );
}

#[test]
fn deterministic() {
    let (body, rim) = tilted_sheet(deg(30.0), 2.0);
    let go = || {
        let mut b = Budget::unlimited();
        let t = tangent_all_along(&body, rim, deg(121.0), &mut b, &Stop::new()).unwrap();
        (t, b.used())
    };
    let first = go();
    assert!(matches!(first.0, Tangency::Tangent { .. }));
    for _ in 0..3 {
        assert_eq!(go(), first);
    }
}
