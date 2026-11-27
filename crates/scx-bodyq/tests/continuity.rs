// Written for solidcorex. Apache-2.0.
//! `continuity` against closed forms (research 74 §25, BW4): the proved order kept apart
//! from the drops measured, on the pin's box and round cases (the round as a stadium's
//! tangent arcs), two arcs tangent inside one another (the curvature's sign), a cylinder's
//! caps and seam, and a kink at the angle tolerance.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

mod along_common;

use std::f64::consts::FRAC_PI_2;

use along_common::{between, deg, doc, extrude_polygon, nested_arcs, seams, stadium};
use scx_bodyq::{
    BodyqError, BodyqRefusal, Continuity, ContinuityTolerances, Limit, Order, continuity,
};
use scx_brep::{Brep, EdgeId};
use scx_num::budget::{Budget, Stop};
use scx_num::fmath;
use scx_num::frame::Frame;
use scx_num::units::Length;
use scx_testkit::bodies::made::{block, cylinder};

fn tol() -> ContinuityTolerances {
    ContinuityTolerances {
        gap: Length::from_mm(1e-6),
        angle: deg(0.5),
        curvature: 1e-6,
        curvature_rate: 1e-6,
        curvature_accel: 1e-6,
    }
}

fn run(body: &Brep, e: EdgeId) -> Continuity {
    continuity(body, e, &tol(), &mut Budget::unlimited(), &Stop::new()).unwrap()
}

/// The drop's bracket holds `want` to within `slack`.
fn near(c: &Continuity, order: Order, want: f64, slack: f64) {
    let d = c.measured[order as usize];
    assert!(
        d.bracket.lo <= want + slack && d.bracket.hi >= want - slack,
        "{order:?}: {:?} vs {want}",
        d.bracket
    );
    assert!(
        d.bracket.hi - d.bracket.lo <= 2.0 * slack,
        "{order:?}: {:?}",
        d.bracket
    );
}

fn refuted_at(c: &Continuity) -> Order {
    match c.limit {
        Some(Limit::Refuted { order, .. }) => order,
        other => panic!("not refuted: {other:?}"),
    }
}

#[test]
fn box_edges_meet_square_with_flat_faces() {
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
    for e in m.body.edges() {
        let c = run(&m.body, e.id());
        assert_eq!(c.proved, Some(Order::G0));
        assert_eq!(refuted_at(&c), Order::G1);
        near(&c, Order::G0, 0.0, 1e-12);
        near(&c, Order::G1, FRAC_PI_2, 1e-12);
        for o in [Order::G2, Order::G3, Order::G4] {
            near(&c, o, 0.0, 1e-12);
        }
    }
}

#[test]
fn a_line_into_its_tangent_arc_jumps_in_curvature_by_the_inverse_radius() {
    let (body, f) = stadium();
    let walls = &f.walls[0];
    let mut seen = 0;
    for (i, &w) in walls.iter().enumerate() {
        for e in between(&body, w, walls[(i + 1) % walls.len()]) {
            let c = run(&body, e);
            assert_eq!(c.proved, Some(Order::G1), "{c:?}");
            assert_eq!(refuted_at(&c), Order::G2);
            near(&c, Order::G0, 0.0, 1e-12);
            near(&c, Order::G1, 0.0, 1e-12);
            near(&c, Order::G2, 0.5, 1e-9);
            near(&c, Order::G3, 0.0, 1e-9);
            near(&c, Order::G4, 0.0, 1e-9);
            seen += 1;
        }
    }
    assert_eq!(seen, 4);
}

#[test]
fn arcs_tangent_inside_one_another_jump_by_the_difference_of_curvatures() {
    // radii 3 and 2, both bending the same way at (3, 0): the jump is 1/2 - 1/3, not the sum
    let (body, f) = nested_arcs();
    let [big, _, small] = [f.walls[0][0], f.walls[0][1], f.walls[0][2]];
    let es = between(&body, big, small);
    assert_eq!(es.len(), 1);
    let c = run(&body, es[0]);
    assert_eq!(c.proved, Some(Order::G1));
    assert_eq!(refuted_at(&c), Order::G2);
    near(&c, Order::G2, 1.0 / 6.0, 1e-9);
    near(&c, Order::G3, 0.0, 1e-9);
    near(&c, Order::G4, 0.0, 1e-9);
}

#[test]
fn a_cylinder_s_caps_are_g0_and_its_seam_g4() {
    let m = cylinder(
        &Frame::WORLD,
        Length::from_mm(3.0),
        Length::from_mm(7.0),
        &doc(),
    )
    .unwrap();
    for cap in [m.faces.bottom, m.faces.top] {
        for e in between(&m.body, m.faces.wall, cap) {
            let c = run(&m.body, e);
            assert_eq!(c.proved, Some(Order::G0));
            assert_eq!(refuted_at(&c), Order::G1);
            near(&c, Order::G1, FRAC_PI_2, 1e-9);
        }
    }
    for e in seams(&m.body) {
        let c = run(&m.body, e);
        assert_eq!(c.proved, Some(Order::G4));
        assert_eq!(c.limit, None);
        for d in c.measured {
            assert_eq!(d.bracket.hi, 0.0);
        }
    }
}

#[test]
fn a_kink_at_the_angle_tolerance_is_undecided_at_g1() {
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
    let e = between(&body, f.walls[0][0], f.walls[0][1])[0];
    let t = ContinuityTolerances { angle: k, ..tol() };
    let c = continuity(&body, e, &t, &mut Budget::unlimited(), &Stop::new()).unwrap();
    assert_eq!(c.proved, Some(Order::G0));
    assert!(
        matches!(
            c.limit,
            Some(Limit::Undecided {
                order: Order::G1,
                ..
            })
        ),
        "{c:?}"
    );
    // the drops are measured all the same
    near(&c, Order::G1, k, 1e-12);
}

#[test]
fn bad_tolerances_are_refused() {
    let m = block(&Frame::WORLD, [Length::from_mm(1.0); 3], &doc()).unwrap();
    let e = m.body.edges().next().unwrap().id();
    let bad = [
        ContinuityTolerances {
            gap: Length::from_mm(-1.0),
            ..tol()
        },
        ContinuityTolerances {
            angle: f64::NAN,
            ..tol()
        },
        ContinuityTolerances {
            curvature: f64::INFINITY,
            ..tol()
        },
        ContinuityTolerances {
            curvature_rate: -1.0,
            ..tol()
        },
        ContinuityTolerances {
            curvature_accel: f64::NAN,
            ..tol()
        },
    ];
    for t in bad {
        assert_eq!(
            continuity(&m.body, e, &t, &mut Budget::unlimited(), &Stop::new()),
            Err(BodyqError::Refused(BodyqRefusal::Tolerance)),
            "{t:?}"
        );
    }
}

#[test]
fn one_budget_for_every_order() {
    let (body, f) = stadium();
    let e = between(&body, f.walls[0][0], f.walls[0][1])[0];
    let mut b = Budget::unlimited();
    let c = continuity(&body, e, &tol(), &mut b, &Stop::new()).unwrap();
    let used = b.used();
    assert!(used > 0);
    let mut short = Budget::new(used - 1);
    assert!(matches!(
        continuity(&body, e, &tol(), &mut short, &Stop::new()),
        Err(BodyqError::Exhausted { .. })
    ));
    let mut exact = Budget::new(used);
    assert_eq!(
        continuity(&body, e, &tol(), &mut exact, &Stop::new()),
        Ok(c)
    );
}

#[test]
fn past_the_curvature_jump_the_rates_are_proved_equal() {
    // with the curvature's tolerance above 1/2, the stadium's tangent edges are proved G4:
    // both sections are circles or lines, whose rates are 0
    let (body, f) = stadium();
    let e = between(&body, f.walls[0][0], f.walls[0][1])[0];
    let t = ContinuityTolerances {
        curvature: 0.6,
        ..tol()
    };
    let c = continuity(&body, e, &t, &mut Budget::unlimited(), &Stop::new()).unwrap();
    assert_eq!(c.proved, Some(Order::G4), "{c:?}");
    assert_eq!(c.limit, None);
    near(&c, Order::G2, 0.5, 1e-9);
}
