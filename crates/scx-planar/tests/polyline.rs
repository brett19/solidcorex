// Written for solidcorex. Apache-2.0.
//! The pin's 2-D polyline and tangent helpers (research 74 §1.3, BW9) against closed forms.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

use scx_num::budget::{Budget, Stop};
use scx_num::units::{Length, Resolution};
use scx_num::vector2d::Point2;
use scx_planar::{
    PolylineError, polyline_closest, polyline_crossings, tangent_circles, tangent_segment_circle,
};

fn p(x: f64, y: f64) -> Point2 {
    Point2::new(x, y)
}

fn res() -> Resolution {
    Resolution::new(Length::from_mm(1e-6)).unwrap()
}

fn cross(a: &[Point2], b: &[Point2]) -> scx_planar::PolylineCrossings {
    polyline_crossings(a, b, res(), &mut Budget::unlimited(), &Stop::new()).unwrap()
}

#[test]
fn two_zigzags_cross_at_the_known_points() {
    // y = x on [0, 4] folded: (0,0)-(2,2)-(4,0); against the line y = 1 from (-1,1) to (5,1).
    let a = [p(0.0, 0.0), p(2.0, 2.0), p(4.0, 0.0)];
    let b = [p(-1.0, 1.0), p(5.0, 1.0)];
    let c = cross(&a, &b);
    assert_eq!(c.meets.len(), 2, "{c:?}");
    assert!(c.overlaps.is_empty());
    let at = |i: usize| c.meets[i].meet.at;
    assert!((at(0).x - 1.0).abs() < 1e-9 && (at(0).y - 1.0).abs() < 1e-9, "{c:?}");
    assert!((at(1).x - 3.0).abs() < 1e-9 && (at(1).y - 1.0).abs() < 1e-9, "{c:?}");
    assert_eq!(
        (c.meets[0].a_segment, c.meets[0].b_segment),
        (0, 0),
        "{c:?}"
    );
    assert_eq!((c.meets[1].a_segment, c.meets[1].b_segment), (1, 0));
    assert!(c.meets.iter().all(|m| m.meet.multiplicity == 1));
}

#[test]
fn polylines_that_miss_or_are_single_points_meet_nowhere() {
    let a = [p(0.0, 0.0), p(1.0, 0.0)];
    let b = [p(0.0, 1.0), p(1.0, 1.0)];
    assert!(cross(&a, &b).meets.is_empty());
    assert!(cross(&a, &[p(0.5, 0.0)]).meets.is_empty());
    assert!(cross(&[], &b).meets.is_empty());
}

#[test]
fn a_shared_stretch_is_an_overlap_not_a_point() {
    // Two collinear segments sharing x in [1, 2].
    let a = [p(0.0, 0.0), p(2.0, 0.0)];
    let b = [p(1.0, 0.0), p(3.0, 0.0)];
    let c = cross(&a, &b);
    assert_eq!(c.overlaps.len(), 1, "{c:?}");
    let o = c.overlaps[0].overlap;
    assert!((o.a[0] - 0.5).abs() < 1e-9 && (o.a[1] - 1.0).abs() < 1e-9, "{o:?}");
}

#[test]
fn a_segment_of_no_length_is_refused_not_skipped() {
    let a = [p(0.0, 0.0), p(0.0, 0.0)];
    let b = [p(-1.0, 0.0), p(1.0, 0.0)];
    let e = polyline_crossings(&a, &b, res(), &mut Budget::unlimited(), &Stop::new());
    assert!(e.is_err(), "{e:?}");
}

#[test]
fn the_nearest_point_of_a_polyline_is_on_a_segment_or_at_an_end() {
    let pts = [p(0.0, 0.0), p(4.0, 0.0), p(4.0, 3.0)];
    let c = polyline_closest(p(2.0, 5.0), &pts).unwrap();
    // Nearest is the corner-side: (4, 3) at distance sqrt(4 + 4)? Against segment 1 (x=4,
    // y in [0,3]) the foot of (2,5) is the end (4,3): sqrt(2^2 + 2^2); against segment 0 the
    // foot is (2,0) at 5. So (4,3).
    assert!((c.distance - 8.0_f64.sqrt()).abs() < 1e-12, "{c:?}");
    assert_eq!((c.segment, c.t), (1, 1.0));
    let c = polyline_closest(p(2.0, -1.0), &pts).unwrap();
    assert!((c.distance - 1.0).abs() < 1e-15 && c.at == p(2.0, 0.0), "{c:?}");
    assert_eq!((c.segment, c.t), (0, 0.5));
    // One point is its own nearest; none is a refusal.
    let c = polyline_closest(p(3.0, 4.0), &[p(0.0, 0.0)]).unwrap();
    assert_eq!((c.distance, c.segment), (5.0, 0));
    assert_eq!(polyline_closest(p(0.0, 0.0), &[]), Err(PolylineError::Empty));
    assert_eq!(
        polyline_closest(p(f64::NAN, 0.0), &pts),
        Err(PolylineError::NotFinite)
    );
}

#[test]
fn a_line_touches_a_circle_where_the_centre_drops_a_foot_of_the_radius() {
    // y = 2 against the circle of radius 2 about the origin: tangent at (0, 2).
    let t = tangent_segment_circle(p(-3.0, 2.0), p(3.0, 2.0), p(0.0, 0.0), 2.0, res())
        .unwrap()
        .unwrap();
    assert!(t.x.abs() < 1e-15 && (t.y - 2.0).abs() < 1e-15, "{t:?}");
    // It crosses (y = 1) or misses (y = 3): no tangent.
    for y in [1.0, 3.0] {
        assert_eq!(
            tangent_segment_circle(p(-3.0, y), p(3.0, y), p(0.0, 0.0), 2.0, res()),
            Ok(None)
        );
    }
    // The tangent foot is off the segment's end: a ray that stops short touches nothing.
    assert_eq!(
        tangent_segment_circle(p(1.0, 2.0), p(3.0, 2.0), p(0.0, 0.0), 2.0, res()),
        Ok(None)
    );
    // Refusals.
    assert_eq!(
        tangent_segment_circle(p(1.0, 1.0), p(1.0, 1.0), p(0.0, 0.0), 2.0, res()),
        Err(PolylineError::Degenerate)
    );
    assert_eq!(
        tangent_segment_circle(p(0.0, 0.0), p(1.0, 0.0), p(0.0, 0.0), 0.0, res()),
        Err(PolylineError::NotPositive)
    );
}

#[test]
fn circles_touch_outside_and_inside_at_the_known_points() {
    // Outside: radii 1 and 2, centres 3 apart on x.
    let t = tangent_circles(p(0.0, 0.0), 1.0, p(3.0, 0.0), 2.0, res())
        .unwrap()
        .unwrap();
    assert!((t.x - 1.0).abs() < 1e-15 && t.y.abs() < 1e-15, "{t:?}");
    // Inside, the smaller second: radii 3 and 1, centres 2 apart on x: contact (3, 0).
    let t = tangent_circles(p(0.0, 0.0), 3.0, p(2.0, 0.0), 1.0, res())
        .unwrap()
        .unwrap();
    assert!((t.x - 3.0).abs() < 1e-15 && t.y.abs() < 1e-15, "{t:?}");
    // Inside, the smaller first: the same circles swapped, the same point.
    let u = tangent_circles(p(2.0, 0.0), 1.0, p(0.0, 0.0), 3.0, res())
        .unwrap()
        .unwrap();
    assert!((u.x - 3.0).abs() < 1e-15 && u.y.abs() < 1e-15, "{u:?}");
    // Crossing, apart, nested without touching, and the same circle twice: none.
    for (c2, r2) in [(p(2.0, 0.0), 2.0), (p(5.0, 0.0), 1.0), (p(0.5, 0.0), 0.2), (p(0.0, 0.0), 1.0)]
    {
        assert_eq!(tangent_circles(p(0.0, 0.0), 1.0, c2, r2, res()), Ok(None), "{c2:?}");
    }
    assert_eq!(
        tangent_circles(p(0.0, 0.0), -1.0, p(1.0, 0.0), 1.0, res()),
        Err(PolylineError::NotPositive)
    );
}
