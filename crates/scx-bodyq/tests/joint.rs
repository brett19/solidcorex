// Written for solidcorex. Apache-2.0.
//! Vertex joints (research 74 §34, BW11), against closed forms: the turn of two edges at a
//! corner, a line running on into its tangent arc, the sign of convexity at an L section's
//! corners, and the angle between two planar directions. These are the pin's
//! `a_box_corner_turns_a_right_angle`, `a_line_runs_on_into_its_tangent_arc_without_a_turn`,
//! `an_l_section_s_inside_corner_is_concave`, `joint_turn_is_the_angle_between_the_directions`
//! and `a_zero_or_non_finite_joint_direction_is_refused`, whose bodies are testkit's.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

mod along_common;

use std::f64::consts::{FRAC_PI_2, PI};

use along_common::{between, doc, extrude_polygon, nurbs_crease, stadium};
use scx_bodyq::{
    BodyqError, BodyqRefusal, Convexity, Leaving, Missing, Turn, convex_at, joint_turn, leaving,
    turn_at,
};
use scx_brep::{Brep, EdgeId, VertexId};
use scx_num::frame::Frame;
use scx_num::units::Length;
use scx_testkit::bodies::made::{block, cylinder};

/// Half a degree: the smooth angle of the pin's tests.
const SMOOTH: f64 = 0.5 * PI / 180.0;

fn shared(body: &Brep, e: EdgeId, f: EdgeId) -> VertexId {
    let a = body.edge(e).unwrap().ends();
    let b = body.edge(f).unwrap().ends();
    *a.iter().find(|v| b.contains(v)).expect("a shared vertex")
}

fn mm(x: f64) -> Length {
    Length::from_mm(x)
}

#[test]
fn a_box_corner_turns_a_right_angle() {
    let m = block(&Frame::WORLD, [mm(10.0), mm(6.0), mm(4.0)], &doc()).unwrap();
    let f = m.faces;
    // The top's front edge (y = 0, z = 4) and left edge (x = 0, z = 4) meet at (0, 0, 4).
    let e = between(&m.body, f.top, f.sides[0])[0];
    let g = between(&m.body, f.top, f.sides[3])[0];
    let v = shared(&m.body, e, g);
    assert_eq!(
        m.body.vertex(v).unwrap().point().to_array(),
        [0.0, 0.0, 4.0]
    );
    let Turn::Angle(t) = turn_at(&m.body, e, g, v).unwrap() else {
        panic!("both directions")
    };
    assert!((t - FRAC_PI_2).abs() < 1e-12, "{t}");
    // The turn is the same either way round.
    let Turn::Angle(back) = turn_at(&m.body, g, e, v).unwrap() else {
        panic!()
    };
    assert!((back - t).abs() < 1e-15);
}

#[test]
fn a_line_runs_on_into_its_tangent_arc_without_a_turn() {
    let (body, f) = stadium();
    // The line y = 5 (segment 0) and the arc (segment 1) meet the end cap in the top edges;
    // they share the vertex (6, 5, 3).
    let line = between(&body, f.walls[0][0], f.end)[0];
    let arc = between(&body, f.walls[0][1], f.end)[0];
    let v = shared(&body, line, arc);
    assert_eq!(body.vertex(v).unwrap().point().to_array(), [6.0, 5.0, 3.0]);
    let Turn::Angle(t) = turn_at(&body, line, arc, v).unwrap() else {
        panic!("both directions")
    };
    assert!(t < 1e-12, "{t}");
    // and the wall's vertical edge there turns square from both
    let wall = between(&body, f.walls[0][0], f.walls[0][1])[0];
    for from in [line, arc] {
        let Turn::Angle(t) = turn_at(&body, from, wall, v).unwrap() else {
            panic!("both directions")
        };
        assert!((t - FRAC_PI_2).abs() < 1e-12, "{t}");
    }
}

#[test]
fn an_l_section_s_inside_corner_is_concave() {
    // The L section (0, 0), (10, 0), (10, 4), (4, 4), (4, 10), (0, 10), 5 high: its vertical
    // edge at (4, 4) is the inside corner, the one at (10, 0) an outside one.
    let (body, f) = extrude_polygon(
        &[
            [0.0, 0.0],
            [10.0, 0.0],
            [10.0, 4.0],
            [4.0, 4.0],
            [4.0, 10.0],
            [0.0, 10.0],
        ],
        5.0,
    );
    for (a, b, convex) in [(2, 3, false), (0, 1, true)] {
        let e = between(&body, f.walls[0][a], f.walls[0][b])[0];
        for v in body.edge(e).unwrap().ends() {
            assert_eq!(
                convex_at(&body, e, v, SMOOTH).unwrap(),
                if convex {
                    Convexity::Convex
                } else {
                    Convexity::Concave
                },
                "walls {a} {b} at {v:?}"
            );
        }
    }
    // Every other long edge of the L is convex or concave by its corner: the other four
    // vertical edges are convex, and every edge of a cap is convex.
    let mut convex = 0;
    let mut concave = 0;
    for e in body.edges() {
        for v in e.ends() {
            match convex_at(&body, e.id(), v, SMOOTH).unwrap() {
                Convexity::Convex => convex += 1,
                Convexity::Concave => concave += 1,
                other => panic!("{other:?}"),
            }
        }
    }
    // 18 edges, 2 ends each: only the one vertical edge at (4, 4) is concave, and also the
    // two cap edges along the inside corner are not (they meet the cap square, convex).
    assert_eq!((convex, concave), (34, 2));
}

#[test]
fn a_box_is_convex_all_round_and_tangent_faces_have_no_convexity() {
    let m = block(&Frame::WORLD, [mm(10.0), mm(6.0), mm(4.0)], &doc()).unwrap();
    for e in m.body.edges() {
        for v in e.ends() {
            assert_eq!(
                convex_at(&m.body, e.id(), v, SMOOTH).unwrap(),
                Convexity::Convex
            );
        }
    }
    // The stadium's wall edge between a line and its arc: the faces are tangent.
    let (body, f) = stadium();
    let wall = between(&body, f.walls[0][0], f.walls[0][1])[0];
    let v = body.edge(wall).unwrap().ends()[0];
    assert_eq!(
        convex_at(&body, wall, v, SMOOTH).unwrap(),
        Convexity::Tangent
    );
    // A smooth angle that is not an angle is refused.
    for bad in [0.0, -1.0, FRAC_PI_2, f64::NAN] {
        assert_eq!(
            convex_at(&body, wall, v, bad),
            Err(BodyqError::Refused(BodyqRefusal::Tolerance))
        );
    }
}

#[test]
fn a_sheet_has_no_inside_corner() {
    // Both sides of a sheet's face are the outside: no material.
    let (sheet, crease) = nurbs_crease();
    let v = sheet.edge(crease).unwrap().ends()[0];
    assert!(matches!(
        convex_at(&sheet, crease, v, SMOOTH).unwrap(),
        Convexity::NoMaterial { .. }
    ));
}

#[test]
fn leaving_is_the_tangent_away_from_the_vertex() {
    let m = block(&Frame::WORLD, [mm(10.0), mm(6.0), mm(4.0)], &doc()).unwrap();
    let f = m.faces;
    let e = between(&m.body, f.top, f.sides[0])[0];
    let [a, b] = m.body.edge(e).unwrap().ends();
    let (pa, pb) = (
        m.body.vertex(a).unwrap().point(),
        m.body.vertex(b).unwrap().point(),
    );
    let along = (pb - pa) * (1.0 / pa.distance(pb));
    let Leaving::Direction(d) = leaving(&m.body, e, a).unwrap() else {
        panic!()
    };
    assert!((d - along).length() < 1e-15, "{d:?}");
    let Leaving::Direction(d) = leaving(&m.body, e, b).unwrap() else {
        panic!()
    };
    assert!((d + along).length() < 1e-15, "{d:?}");
    // A vertex that is no end of the edge.
    let other = m
        .body
        .vertices()
        .map(scx_brep::Vertex::id)
        .find(|v| *v != a && *v != b)
        .unwrap();
    assert_eq!(
        leaving(&m.body, e, other),
        Err(BodyqError::Refused(BodyqRefusal::NotAnEnd {
            edge: e,
            vertex: other
        }))
    );
    // A rim circle starts and ends at one vertex: which end is meant is not said.
    let c = cylinder(&Frame::WORLD, mm(3.0), mm(7.0), &doc()).unwrap();
    let rim = between(&c.body, c.faces.wall, c.faces.top)[0];
    let v = c.body.edge(rim).unwrap().ends()[0];
    assert_eq!(
        leaving(&c.body, rim, v).unwrap(),
        Leaving::Missing(Missing::Closed)
    );
    assert!(matches!(
        turn_at(&c.body, rim, rim, v).unwrap(),
        Turn::Undefined {
            why: Missing::Closed,
            ..
        }
    ));
    assert_eq!(
        convex_at(&c.body, rim, v, SMOOTH).unwrap(),
        Convexity::NoEnd(Missing::Closed)
    );
}

#[test]
fn joint_turn_is_the_angle_between_the_directions() {
    let close = |a: f64, b: f64| (a - b).abs() < 1e-12;
    assert!(close(joint_turn([1.0, 0.0], [2.0, 0.0]).unwrap(), 0.0));
    assert!(close(
        joint_turn([1.0, 0.0], [0.0, 3.0]).unwrap(),
        FRAC_PI_2
    ));
    assert!(close(
        joint_turn([0.0, 1.0], [1.0, 0.0]).unwrap(),
        FRAC_PI_2
    ));
    assert!(close(joint_turn([1.0, 0.0], [-1.0, 0.0]).unwrap(), PI));
    // Scale plays no part, at any size: a direction of 1e-200 and one of 1e200.
    assert!(close(
        joint_turn([1e-200, 0.0], [0.0, 1e200]).unwrap(),
        FRAC_PI_2
    ));
    // A small turn is exact to its own size, not to the arithmetic's.
    let t = 1e-9_f64;
    let turn = joint_turn([1.0, 0.0], [1.0, t]).unwrap();
    assert!((turn - t).abs() < 1e-24, "{turn}");
    // Near pi, too.
    let turn = joint_turn([1.0, 0.0], [-1.0, t]).unwrap();
    assert!((turn - (PI - t)).abs() < 1e-15, "{turn}");
}

#[test]
fn a_zero_or_non_finite_joint_direction_is_refused() {
    assert_eq!(
        joint_turn([0.0, 0.0], [1.0, 0.0]),
        Err(BodyqError::Refused(BodyqRefusal::NoDirection))
    );
    assert_eq!(
        joint_turn([1.0, 0.0], [0.0, -0.0]),
        Err(BodyqError::Refused(BodyqRefusal::NoDirection))
    );
    assert_eq!(
        joint_turn([f64::NAN, 0.0], [1.0, 0.0]),
        Err(BodyqError::Refused(BodyqRefusal::NotFinite))
    );
    assert_eq!(
        joint_turn([1.0, 0.0], [f64::INFINITY, 0.0]),
        Err(BodyqError::Refused(BodyqRefusal::NotFinite))
    );
}
