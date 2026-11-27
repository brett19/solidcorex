// Written for solidcorex. Apache-2.0.
//! The pin's `prism/tests.rs` cases on line-and-arc regions, as outcomes (research 72
//! §14.6). The pin's loops take one arc for a full circle; here a circle is two half arcs.
//! The pin's `holds` answers `None` only exactly on the loop; [`Region::classify`] answers
//! [`Place::Boundary`] within the resolution, so the ports use a resolution below each
//! case's distance from the loop.

use core::f64::consts::PI;

use scx_num::error::ErrorKind;
use scx_num::fmath;
use scx_num::units::{Length, Resolution};
use scx_num::vector2d::Point2;
use scx_planar::{Degenerate, LoopId, Place, PlanarError, Region, Segment, Sense};

fn p(x: f64, y: f64) -> Point2 {
    Point2::new(x, y)
}

fn res(r: f64) -> Resolution {
    Resolution::new(Length::from_mm(r)).unwrap_or(Resolution::DEFAULT)
}

fn polygon(pts: &[Point2]) -> Vec<Segment> {
    pts.iter()
        .zip(pts.iter().cycle().skip(1))
        .map(|(a, b)| Segment::line(*a, *b))
        .collect()
}

fn square10() -> Vec<Segment> {
    polygon(&[p(0.0, 0.0), p(10.0, 0.0), p(10.0, 10.0), p(0.0, 10.0)])
}

/// The circle about `c` of radius `r`, counter-clockwise, as two half arcs.
fn circle(c: Point2, r: f64) -> Vec<Segment> {
    let (e, w) = (p(c.x + r, c.y), p(c.x - r, c.y));
    vec![
        Segment::arc(c, e, w, Sense::Forward),
        Segment::arc(c, w, e, Sense::Forward),
    ]
}

fn reversed(l: &[Segment]) -> Vec<Segment> {
    l.iter().rev().map(Segment::reversed).collect()
}

fn region(outer: Vec<Segment>, holes: Vec<Vec<Segment>>, r: f64) -> Region {
    Region::new(outer, holes, res(r)).unwrap_or_else(|e| unreachable!("{e}"))
}

fn place(r: &Region, x: f64, y: f64) -> Place {
    r.classify(p(x, y)).unwrap_or_else(|e| unreachable!("{e}"))
}

/// `areas_in_closed_form_are_enclosed_by_their_bounds`, its line and arc cases: the square
/// and the circle, given either way round.
#[test]
fn areas_in_closed_form() {
    let sq = region(square10(), vec![], 1e-9);
    assert!((sq.area() - 100.0).abs() <= 1e-12 * 100.0);
    for (l, rev) in [
        (circle(p(3.0, -2.0), 6.0), false),
        (reversed(&circle(p(3.0, -2.0), 6.0)), true),
    ] {
        let c = region(l, vec![], 1e-9);
        assert!(
            (c.area() - 36.0 * PI).abs() <= 1e-12 * 36.0 * PI,
            "{}",
            c.area()
        );
        assert_eq!(c.outer_reversed(), rev);
        assert!(c.outer().signed_area() > 0.0);
    }
}

/// `holds_decides_exactly_or_refuses_on_the_boundary`, its circle and square cases.
#[test]
fn classify_decides_or_answers_boundary() {
    let c = region(circle(p(0.0, 0.0), 10.0), vec![], 1e-9);
    // Inside the arc where a 1e-3 chord would have cut it off.
    assert_eq!(place(&c, 9.99999, 0.001), Place::Inside);
    assert_eq!(place(&c, 10.00001, 0.0), Place::Outside);
    assert_eq!(place(&c, 10.0, 0.0), Place::Boundary);
    let sq = region(square10(), vec![], 1e-9);
    assert_eq!(place(&sq, 5.0, 0.0), Place::Boundary);
    assert_eq!(place(&sq, 5.0, 5.0), Place::Inside);
    assert_eq!(place(&sq, -1.0, 5.0), Place::Outside);
}

/// `holds_decides_a_point_off_the_loop_on_an_edge_line_by_the_half_open_rule`: points
/// level with edges and vertices, where a ray-casting count must take a side. The winding
/// number has no ray, so these are plain cases here.
#[test]
fn classify_off_the_loop_on_an_edge_line() {
    let sq = region(square10(), vec![], 1e-9);
    for (x, y) in [
        (-1.0, 0.0),
        (11.0, 0.0),
        (-1.0, 10.0),
        (5.0, -1.0),
        (-1e-3, 10.0),
    ] {
        assert_eq!(place(&sq, x, y), Place::Outside, "({x}, {y})");
    }
    let diamond = region(
        polygon(&[p(0.0, -5.0), p(5.0, 0.0), p(0.0, 5.0), p(-5.0, 0.0)]),
        vec![],
        1e-9,
    );
    assert_eq!(place(&diamond, 0.0, 0.0), Place::Inside);
    assert_eq!(place(&diamond, -6.0, 0.0), Place::Outside);
    assert_eq!(place(&sq, 0.0, 0.0), Place::Boundary);
    assert_eq!(place(&sq, 10.0, 5.0), Place::Boundary);
    // The pin's hole loop, here a hole in the plate: in the hole is outside the region. The
    // point 1e-9 from the hole's edge needs a resolution below that.
    let hole = polygon(&[p(2.0, 2.0), p(2.0, 8.0), p(8.0, 8.0), p(8.0, 2.0)]);
    let plate = region(square10(), vec![hole], 1e-12);
    assert_eq!(plate.holes_reversed(), &[false]);
    assert_eq!(place(&plate, 1.0, 2.0), Place::Inside);
    assert_eq!(place(&plate, 5.0, 2.0 + 1e-9), Place::Outside);
    let coarse = region(
        square10(),
        vec![polygon(&[
            p(2.0, 2.0),
            p(2.0, 8.0),
            p(8.0, 8.0),
            p(8.0, 2.0),
        ])],
        1e-9,
    );
    // In floats 2 + 1e-9 is 1.00000008e-9 above 2: outside the resolution, just.
    assert_eq!(place(&coarse, 5.0, 2.0 + 1e-9), Place::Outside);
    assert_eq!(place(&coarse, 5.0, 2.0 + 5e-10), Place::Boundary);
}

/// `hostile_profiles_are_refused_by_kind`, its line cases: a NaN, an open plate of three
/// of its four edges, and the plate itself.
#[test]
fn hostile_profiles_are_refused_by_kind() {
    let mut nan = square10();
    if let Some(s) = nan.first_mut() {
        *s = Segment::line(p(f64::NAN, 0.0), p(10.0, 0.0));
    }
    let e = Region::new(nan, vec![], res(1e-7)).err();
    assert!(matches!(e, Some(PlanarError::NotFinite { .. })), "{e:?}");
    assert_eq!(e.map(|e| e.kind()), Some(ErrorKind::Refused));
    let open: Vec<Segment> = square10().into_iter().take(3).collect();
    let e = Region::new(open, vec![], res(1e-7)).err();
    assert!(
        matches!(
            e,
            Some(PlanarError::Open {
                in_loop: LoopId::Outer,
                joint: 2,
                ..
            })
        ),
        "{e:?}"
    );
    assert!(Region::new(square10(), vec![], res(1e-7)).is_ok());
}

/// `the_profile_check_holds_the_building_kernels_zone_tolerance`: an arc of radius 1 and
/// sweep 5e-6, closed by a line, is refused at a resolution of 1e-5 (its ends are within
/// it) and accepted at 1e-7. A resolution that is not finite and positive is not a
/// [`Resolution`].
#[test]
fn a_short_arc_against_the_resolution() {
    let at = |t: f64| p(fmath::cos(t), fmath::sin(t));
    let (s, e) = (at(0.5), at(0.5 + 5e-6));
    let profile = || {
        vec![
            Segment::arc(p(0.0, 0.0), s, e, Sense::Forward),
            Segment::line(e, s),
        ]
    };
    let e1 = Region::new(profile(), vec![], res(1e-5)).err();
    assert!(
        matches!(
            e1,
            Some(PlanarError::Degenerate {
                why: Degenerate::Short,
                ..
            })
        ),
        "{e1:?}"
    );
    let ok = Region::new(profile(), vec![], res(1e-7));
    assert!(ok.is_ok(), "{ok:?}");
    for zone in [0.0, -1.0, f64::NAN, f64::INFINITY] {
        assert!(Resolution::new(Length::from_mm(zone)).is_none(), "{zone}");
    }
}

/// `an_arc_on_a_circle_within_the_zone_tolerance_is_refused`: a circle of radius 1e-7 at
/// the default resolution.
#[test]
fn an_arc_within_the_resolution_is_refused() {
    let e = Region::new(circle(p(0.0, 0.0), 1e-7), vec![], Resolution::DEFAULT).err();
    assert!(
        matches!(
            e,
            Some(PlanarError::Degenerate {
                why: Degenerate::Radius,
                ..
            })
        ),
        "{e:?}"
    );
}
