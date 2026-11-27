// Written for solidcorex. Apache-2.0.
//! Whole circles (`Segment::Circle`): a loop of one circle is accepted with area `pi r^2`
//! and oriented like any loop; circles meet other segments under the same contact rules,
//! and a circle sharing its loop with other segments is refused.
#![allow(
    clippy::unwrap_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::float_cmp
)]

use core::f64::consts::PI;

use scx_num::units::{Length, Resolution};
use scx_num::vector2d::Point2;
use scx_planar::{Degenerate, LoopId, Place, PlanarError, Region, Segment, Sense};

fn res() -> Resolution {
    Resolution::new(Length::from_mm(1e-6)).unwrap()
}

fn p(x: f64, y: f64) -> Point2 {
    Point2::new(x, y)
}

fn circle(cx: f64, cy: f64, r: f64, sense: Sense) -> Segment {
    Segment::circle(p(cx, cy), r, sense)
}

fn square(lo: f64, hi: f64) -> Vec<Segment> {
    let c = [p(lo, lo), p(hi, lo), p(hi, hi), p(lo, hi)];
    (0..4)
        .map(|i| Segment::line(c[i], c[(i + 1) % 4]))
        .collect()
}

fn region(outer: Vec<Segment>, holes: Vec<Vec<Segment>>) -> Result<Region, PlanarError> {
    Region::new(outer, holes, res())
}

fn close(a: f64, b: f64) -> bool {
    (a - b).abs() <= 1e-12 * b.abs()
}

#[test]
fn a_circle_alone_is_a_disc() {
    let r = region(vec![circle(1.0, 2.0, 3.0, Sense::Forward)], vec![]).unwrap();
    assert!(close(r.area(), 9.0 * PI));
    assert!(!r.outer_reversed());
    assert_eq!(
        r.outer().segments(),
        &[circle(1.0, 2.0, 3.0, Sense::Forward)]
    );
    assert_eq!(r.classify(p(1.0, 2.0)), Ok(Place::Inside));
    assert_eq!(r.classify(p(3.9, 2.0)), Ok(Place::Inside));
    assert_eq!(r.classify(p(4.1, 2.0)), Ok(Place::Outside));
    assert_eq!(r.classify(p(4.0, 2.0)), Ok(Place::Boundary));
    assert_eq!(r.classify(p(1.0, -1.0)), Ok(Place::Boundary));
}

#[test]
fn a_clockwise_circle_is_turned_and_flagged() {
    let r = region(vec![circle(0.0, 0.0, 2.0, Sense::Reversed)], vec![]).unwrap();
    assert!(r.outer_reversed());
    assert_eq!(
        r.outer().segments(),
        &[circle(0.0, 0.0, 2.0, Sense::Forward)]
    );
    assert!(close(r.area(), 4.0 * PI));
}

#[test]
fn circle_holes_in_a_square() {
    let holes = vec![
        vec![circle(2.0, 2.0, 1.0, Sense::Reversed)],
        vec![circle(6.0, 6.0, 1.5, Sense::Forward)],
    ];
    let r = region(square(0.0, 10.0), holes).unwrap();
    assert_eq!(r.holes_reversed(), &[false, true]);
    assert!(close(r.area(), 100.0 - PI - 2.25 * PI));
    assert!(r.holes().iter().all(|h| h.signed_area() < 0.0));
    assert_eq!(r.classify(p(2.0, 2.0)), Ok(Place::Outside));
    assert_eq!(r.classify(p(4.0, 4.0)), Ok(Place::Inside));
    assert_eq!(r.classify(p(7.5, 6.0)), Ok(Place::Boundary));
}

#[test]
fn a_ring_of_two_circles() {
    let r = region(
        vec![circle(0.0, 0.0, 2.0, Sense::Forward)],
        vec![vec![circle(0.0, 0.0, 1.0, Sense::Reversed)]],
    )
    .unwrap();
    assert!(close(r.area(), 3.0 * PI));
    assert_eq!(r.classify(p(0.0, 0.0)), Ok(Place::Outside));
    assert_eq!(r.classify(p(0.0, 1.5)), Ok(Place::Inside));
    assert_eq!(r.classify(p(-1.5, 0.0)), Ok(Place::Inside));
}

#[test]
fn circles_that_touch_or_cross_are_refused() {
    let touching = region(
        vec![circle(0.0, 0.0, 2.0, Sense::Forward)],
        vec![vec![circle(1.0, 0.0, 1.0, Sense::Reversed)]],
    );
    assert!(
        matches!(touching, Err(PlanarError::Touching { .. })),
        "{touching:?}"
    );
    let crossing = region(
        square(0.0, 10.0),
        vec![
            vec![circle(4.0, 5.0, 1.0, Sense::Reversed)],
            vec![circle(5.5, 5.0, 1.0, Sense::Reversed)],
        ],
    );
    assert!(
        matches!(crossing, Err(PlanarError::Crossing { .. })),
        "{crossing:?}"
    );
    let near = region(
        square(0.0, 10.0),
        vec![vec![circle(5.0, 5.0, 5.0 - 1e-7, Sense::Reversed)]],
    );
    assert!(
        matches!(near, Err(PlanarError::Touching { .. })),
        "{near:?}"
    );
}

#[test]
fn lines_through_or_tangent_to_a_circle_are_refused() {
    let crossing = region(
        square(0.0, 10.0),
        vec![vec![circle(0.0, 5.0, 1.0, Sense::Reversed)]],
    );
    assert!(
        matches!(crossing, Err(PlanarError::Crossing { .. })),
        "{crossing:?}"
    );
    let tangent = region(
        square(0.0, 10.0),
        vec![vec![circle(5.0, 1.0, 1.0, Sense::Reversed)]],
    );
    assert!(
        matches!(tangent, Err(PlanarError::Touching { .. })),
        "{tangent:?}"
    );
    // Its seam at (6, 5) is far from the line; the circle's far side is not.
    let far_side = region(
        square(0.0, 10.0),
        vec![vec![circle(1.0, 5.0, 1.0, Sense::Reversed)]],
    );
    assert!(
        matches!(far_side, Err(PlanarError::Touching { .. })),
        "{far_side:?}"
    );
}

#[test]
fn a_circle_with_other_segments_is_refused() {
    let mut outer = square(0.0, 10.0);
    outer.push(circle(5.0, 5.0, 1.0, Sense::Forward));
    let got = region(outer, vec![]);
    assert!(
        matches!(got, Err(PlanarError::CircleWithOthers { at }) if at.in_loop == LoopId::Outer && at.segment == 4),
        "{got:?}"
    );
}

#[test]
fn a_bad_radius_is_refused() {
    for r in [0.0, -1.0, 1e-7] {
        let got = region(vec![circle(0.0, 0.0, r, Sense::Forward)], vec![]);
        assert!(
            matches!(
                got,
                Err(PlanarError::Degenerate {
                    why: Degenerate::Radius,
                    ..
                })
            ),
            "{r}: {got:?}"
        );
    }
    for r in [f64::NAN, f64::INFINITY] {
        let got = region(vec![circle(0.0, 0.0, r, Sense::Forward)], vec![]);
        assert!(
            matches!(got, Err(PlanarError::NotFinite { .. })),
            "{r}: {got:?}"
        );
    }
}

#[test]
fn circle_holes_outside_or_in_a_hole_are_refused() {
    let outside = region(
        square(0.0, 10.0),
        vec![vec![circle(20.0, 5.0, 1.0, Sense::Reversed)]],
    );
    assert_eq!(outside, Err(PlanarError::HoleOutside { hole: 0 }));
    let nested = region(
        square(0.0, 10.0),
        vec![
            vec![circle(5.0, 5.0, 3.0, Sense::Reversed)],
            vec![circle(5.0, 5.0, 1.0, Sense::Reversed)],
        ],
    );
    assert_eq!(nested, Err(PlanarError::HoleInHole { outer: 0, inner: 1 }));
    let around = region(
        vec![circle(0.0, 0.0, 1.0, Sense::Forward)],
        vec![vec![circle(0.0, 0.0, 2.0, Sense::Reversed)]],
    );
    assert_eq!(around, Err(PlanarError::HoleOutside { hole: 0 }));
}

#[test]
fn arcs_meet_circles_by_the_same_rules() {
    // A slot (two lines, two half-circle arcs) around a circle hole, clear of it.
    let outer = vec![
        Segment::line(p(0.0, 0.0), p(4.0, 0.0)),
        Segment::arc(p(4.0, 2.0), p(4.0, 0.0), p(4.0, 4.0), Sense::Forward),
        Segment::line(p(4.0, 4.0), p(0.0, 4.0)),
        Segment::arc(p(0.0, 2.0), p(0.0, 4.0), p(0.0, 0.0), Sense::Forward),
    ];
    let hole = |r: f64| vec![vec![circle(5.0, 2.0, r, Sense::Reversed)]];
    let r = region(outer.clone(), hole(0.5)).unwrap();
    assert!(close(r.area(), 16.0 + 4.0 * PI - 0.25 * PI));
    let touching = region(outer, hole(1.0));
    assert!(
        matches!(touching, Err(PlanarError::Touching { .. })),
        "{touching:?}"
    );
}
