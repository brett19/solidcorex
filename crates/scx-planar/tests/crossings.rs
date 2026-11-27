// Written for solidcorex. Apache-2.0.
//! Crossings of lines and conic arcs (research 74 §31) against closed forms: two circles,
//! a line tangent to an ellipse, a near tangency at 1e-9, an osculating circle (contact
//! of order 4), coincident arcs, shared joints, and the refusals and limits.
#![allow(
    clippy::unwrap_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::float_cmp
)]

use core::f64::consts::{FRAC_1_SQRT_2, PI};

use scx_num::budget::{Budget, Stop};
use scx_num::error::ErrorKind;
use scx_num::fmath::{cos, hypot, sin};
use scx_num::units::{Length, Resolution};
use scx_num::vector2d::Point2;
use scx_planar::{CrossError, Crossings, Curve2, Meet, Pending, Which, crossings};

const RES: f64 = 1e-6;

fn res() -> Resolution {
    Resolution::new(Length::from_mm(RES)).unwrap()
}

fn p(x: f64, y: f64) -> Point2 {
    Point2::new(x, y)
}

fn line(a: Point2, b: Point2) -> Curve2 {
    Curve2::Line { start: a, end: b }
}

fn cross(a: &Curve2, b: &Curve2) -> Crossings {
    crossings(a, b, res(), &mut Budget::unlimited(), &Stop::new()).unwrap()
}

fn dist(a: Point2, b: Point2) -> f64 {
    hypot(a.x - b.x, a.y - b.y)
}

/// The arc of the ellipse `c + u cos th + v sin th` over `[t0, t1]`, in pieces of at most
/// a quarter turn: each a rational quadratic with apex `c + (u cos m + v sin m) / cos h`
/// and weight `cos h`, `h` the half-angle and `m` the middle.
fn ellipse_arc(c: Point2, u: [f64; 2], v: [f64; 2], t0: f64, t1: f64) -> Vec<Curve2> {
    let n = ((t1 - t0).abs() / (PI / 2.0)).ceil().max(1.0) as usize;
    let at = |t: f64| {
        p(
            c.x + u[0] * cos(t) + v[0] * sin(t),
            c.y + u[1] * cos(t) + v[1] * sin(t),
        )
    };
    let mut out = Vec::new();
    let mut start = at(t0);
    for i in 0..n {
        let a = t0 + (t1 - t0) * i as f64 / n as f64;
        let b = t0 + (t1 - t0) * (i + 1) as f64 / n as f64;
        let (h, m) = (0.5 * (b - a), 0.5 * (a + b));
        let end = at(b);
        let apex = p(
            c.x + (u[0] * cos(m) + v[0] * sin(m)) / cos(h),
            c.y + (u[1] * cos(m) + v[1] * sin(m)) / cos(h),
        );
        out.push(Curve2::Conic {
            start,
            apex,
            end,
            weight: cos(h),
        });
        start = end;
    }
    out
}

fn circle(c: Point2, r: f64, t0: f64, t1: f64) -> Vec<Curve2> {
    ellipse_arc(c, [r, 0.0], [0.0, r], t0, t1)
}

/// The point at parameter `t` of a line or conic.
fn eval(c: &Curve2, t: f64) -> Point2 {
    match c {
        Curve2::Line { start, end } => p(
            start.x + t * (end.x - start.x),
            start.y + t * (end.y - start.y),
        ),
        Curve2::Conic {
            start,
            apex,
            end,
            weight,
        } => {
            let (b0, b1, b2) = ((1.0 - t) * (1.0 - t), 2.0 * weight * t * (1.0 - t), t * t);
            let d = b0 + b1 + b2;
            p(
                (b0 * start.x + b1 * apex.x + b2 * end.x) / d,
                (b0 * start.y + b1 * apex.y + b2 * end.y) / d,
            )
        }
        _ => unreachable!(),
    }
}

fn mid(t: [f64; 2]) -> f64 {
    0.5 * (t[0] + t[1])
}

/// Every meet of every pair of pieces, with the pieces' meets at their shared joints kept.
fn all_meets(a: &[Curve2], b: &[Curve2]) -> Vec<(usize, usize, Meet)> {
    let mut out = Vec::new();
    for (i, x) in a.iter().enumerate() {
        for (j, y) in b.iter().enumerate() {
            let c = cross(x, y);
            assert!(c.overlaps.is_empty(), "{i} {j}: {:?}", c.overlaps);
            out.extend(c.meets.into_iter().map(|m| (i, j, m)));
        }
    }
    out
}

/// A meet is where both curves are, to the resolution.
fn check_on_both(a: &Curve2, b: &Curve2, m: &Meet) {
    let pa = eval(a, mid(m.a));
    let pb = eval(b, mid(m.b));
    assert!(dist(pa, m.at) <= 2.0 * RES, "{m:?}");
    assert!(dist(pa, pb) <= 4.0 * RES, "{m:?}: {pa:?} {pb:?}");
}

#[test]
fn two_circles_cross_where_the_closed_form_says() {
    // r1 = 1 at the origin, r2 = 0.9 at (1.2, 0.3): the radical line gives the two points.
    let (c2, r1, r2) = (p(1.2, 0.3), 1.0_f64, 0.9_f64);
    let d = hypot(c2.x, c2.y);
    let a = (r1 * r1 - r2 * r2 + d * d) / (2.0 * d);
    let h = (r1 * r1 - a * a).sqrt();
    let (ex, ey) = (c2.x / d, c2.y / d);
    let truth = [
        p(a * ex - h * ey, a * ey + h * ex),
        p(a * ex + h * ey, a * ey - h * ex),
    ];
    // Pieces rotated off the crossings, so no crossing is at a joint.
    let one = circle(p(0.0, 0.0), r1, 0.1, 0.1 + 2.0 * PI);
    let two = circle(c2, r2, 0.2, 0.2 + 2.0 * PI);
    let meets = all_meets(&one, &two);
    assert_eq!(meets.len(), 2, "{meets:?}");
    for (i, j, m) in &meets {
        assert_eq!(m.multiplicity, 1);
        assert!(m.exact && m.crosses());
        check_on_both(&one[*i], &two[*j], m);
        assert!(truth.iter().any(|t| dist(*t, m.at) <= 2.0 * RES), "{m:?}");
    }
    assert!(dist(meets[0].2.at, meets[1].2.at) > 0.1);
}

#[test]
fn a_line_tangent_to_an_ellipse_touches_with_multiplicity_two() {
    // The quarter of x^2/4 + y^2 = 1 from (2, 0) to (0, 1): its ends are exact and its end
    // tangent points at the apex (2, 1), so y = 1 touches it at (0, 1) whatever the rounded
    // weight 1/sqrt 2 does to the inside.
    let q = Curve2::Conic {
        start: p(2.0, 0.0),
        apex: p(2.0, 1.0),
        end: p(0.0, 1.0),
        weight: FRAC_1_SQRT_2,
    };
    let l = line(p(-3.0, 1.0), p(3.0, 1.0));
    let c = cross(&q, &l);
    assert_eq!(c.meets.len(), 1, "{c:?}");
    let m = c.meets[0];
    assert_eq!(
        (m.a, m.multiplicity, m.exact),
        ([1.0, 1.0], 2, true),
        "{m:?}"
    );
    assert!(!m.crosses());
    assert_eq!(m.at, p(0.0, 1.0));
    assert!((mid(m.b) - 0.5).abs() <= RES, "{m:?}");
    // The piece [pi/4, 3pi/4] built from rounded cosines: its top is within 1e-16 of
    // (0, 1), on whichever side the rounding put it. Above: a double meet there (two
    // crossings closer than the resolution); below: certified apart, no meet.
    let e = ellipse_arc(
        p(0.0, 0.0),
        [2.0, 0.0],
        [0.0, 1.0],
        PI / 4.0,
        3.0 * PI / 4.0,
    );
    let c = cross(&e[0], &l);
    let total: u32 = c.meets.iter().map(|m| m.multiplicity).sum();
    assert!(total == 0 || total == 2, "{c:?}");
    for m in &c.meets {
        assert!(dist(m.at, p(0.0, 1.0)) <= 2.0 * RES, "{m:?}");
    }
}

#[test]
fn an_exact_tangency_is_exact() {
    // (-1, 0), apex (0, 3), (1, 0), weight 1/2: the arc's middle is exactly (0, 1), with a
    // horizontal tangent; every number is a double and the zero is exact.
    let e = Curve2::Conic {
        start: p(-1.0, 0.0),
        apex: p(0.0, 3.0),
        end: p(1.0, 0.0),
        weight: 0.5,
    };
    let l = line(p(-2.0, 1.0), p(2.0, 1.0));
    let c = cross(&e, &l);
    assert_eq!(c.meets.len(), 1, "{c:?}");
    let m = c.meets[0];
    assert_eq!(
        (m.a, m.multiplicity, m.exact),
        ([0.5, 0.5], 2, true),
        "{m:?}"
    );
    assert_eq!(m.at, p(0.0, 1.0));
    assert!(m.b[0] <= 0.5 && 0.5 <= m.b[1], "{m:?}");
    // Swapped, the same meet with the brackets swapped.
    let s = cross(&l, &e);
    assert_eq!(s.meets.len(), 1);
    assert_eq!((s.meets[0].a, s.meets[0].b), (m.b, m.a));
}

#[test]
fn a_near_tangency_at_1e_9_is_two_crossings_and_a_near_miss_is_none() {
    let e = ellipse_arc(
        p(0.0, 0.0),
        [2.0, 0.0],
        [0.0, 1.0],
        PI / 4.0,
        3.0 * PI / 4.0,
    );
    // y = 1 - 1e-9 cuts x^2/4 + y^2 = 1 at x = +-2 sqrt(1 - y^2), about +-8.9e-5.
    let y: f64 = 1.0 - 1e-9;
    let x = 2.0 * (1.0 - y * y).sqrt();
    let c = cross(&e[0], &line(p(-3.0, y), p(3.0, y)));
    assert_eq!(c.meets.len(), 2, "{c:?}");
    for (m, want) in c.meets.iter().zip([p(x, y), p(-x, y)]) {
        assert_eq!((m.multiplicity, m.exact), (1, true));
        assert!(dist(m.at, want) <= 2.0 * RES, "{m:?} {want:?}");
    }
    // 1e-9 above the top: certified apart, no meet though closer than the resolution.
    let c = cross(&e[0], &line(p(-3.0, 1.0 + 1e-9), p(3.0, 1.0 + 1e-9)));
    assert!(c.meets.is_empty(), "{c:?}");
    // 1e-15 below: two crossings 1e-7 apart, closer than the resolution: two meets, or
    // one of multiplicity 2.
    let y = 1.0 - 1e-15;
    let c = cross(&e[0], &line(p(-3.0, y), p(3.0, y)));
    let total: u32 = c.meets.iter().map(|m| m.multiplicity).sum();
    assert_eq!(total, 2, "{c:?}");
    for m in &c.meets {
        assert!(dist(m.at, p(0.0, 1.0)) <= 1e-6, "{m:?}");
    }
}

#[test]
fn an_osculating_circle_has_contact_of_order_four() {
    // x^2/4 + y^2 = 1 has radius of curvature 1/2 at its vertex (2, 0): the circle of
    // radius 1/2 about (1.5, 0) meets it there only, with multiplicity 4.
    let e = ellipse_arc(p(0.0, 0.0), [2.0, 0.0], [0.0, 1.0], -PI / 4.0, PI / 4.0);
    let o = circle(p(1.5, 0.0), 0.5, -PI / 4.0, PI / 4.0);
    let c = cross(&e[0], &o[0]);
    assert_eq!(c.meets.len(), 1, "{c:?}");
    let m = c.meets[0];
    assert_eq!(m.multiplicity, 4, "{m:?}");
    assert!(!m.crosses());
    assert!(dist(m.at, p(2.0, 0.0)) <= 1e-3, "{m:?}");
}

#[test]
fn one_arc_with_itself_is_one_overlap() {
    let a = &circle(p(0.3, -0.2), 1.7, 0.1, 1.2)[0];
    let c = cross(a, a);
    assert!(c.meets.is_empty(), "{c:?}");
    assert_eq!(c.overlaps.len(), 1, "{c:?}");
    let o = c.overlaps[0];
    assert_eq!(o.a, [0.0, 1.0]);
    assert!(
        (o.b[0] - 0.0).abs() <= RES && (o.b[1] - 1.0).abs() <= RES,
        "{o:?}"
    );
}

#[test]
fn arcs_of_one_circle_split_differently_overlap_where_they_share() {
    // [0, pi/2] and [pi/4, 3pi/4] of one circle share [pi/4, pi/2].
    let a = &circle(p(0.0, 0.0), 2.0, 0.0, PI / 2.0)[0];
    let b = &circle(p(0.0, 0.0), 2.0, PI / 4.0, 3.0 * PI / 4.0)[0];
    let c = cross(a, b);
    assert_eq!(c.overlaps.len(), 1, "{c:?}");
    let o = c.overlaps[0];
    let start = eval(a, o.a[0]);
    let end = eval(a, o.a[1]);
    let want = p(2.0 * cos(PI / 4.0), 2.0 * sin(PI / 4.0));
    assert!(dist(start, want) <= 4.0 * RES, "{o:?} {start:?}");
    assert!((o.a[1] - 1.0).abs() <= 1e-12, "{o:?}");
    assert!(dist(end, eval(b, o.b[1])) <= 4.0 * RES, "{o:?}");
    assert!(o.b[0] <= RES && o.b[1] > o.b[0], "{o:?}");
    // The other way round, the second's stretch: running the same way.
    let s = cross(b, a);
    assert_eq!(s.overlaps.len(), 1, "{s:?}");
    assert!(
        s.overlaps[0].a[0] <= RES && s.overlaps[0].b[1] >= 1.0 - 1e-12,
        "{s:?}"
    );
    // Reversed, the second runs the other way: its parameters fall.
    let Curve2::Conic {
        start,
        apex,
        end,
        weight,
    } = b.clone()
    else {
        unreachable!()
    };
    let r = Curve2::Conic {
        start: end,
        apex,
        end: start,
        weight,
    };
    let c = cross(a, &r);
    assert_eq!(c.overlaps.len(), 1, "{c:?}");
    assert!(c.overlaps[0].b[0] > c.overlaps[0].b[1], "{c:?}");
}

#[test]
fn a_tangent_joint_is_one_exact_double_meet() {
    // y = 1 from (-1, 1) to (0, 1), then the quarter circle about the origin from (0, 1)
    // with apex (1, 1), exactly on the line: contact of order 2 at the joint.
    let l = line(p(-1.0, 1.0), p(0.0, 1.0));
    let arc = Curve2::Conic {
        start: p(0.0, 1.0),
        apex: p(1.0, 1.0),
        end: p(1.0, 0.0),
        weight: FRAC_1_SQRT_2,
    };
    let c = cross(&l, &arc);
    assert_eq!(c.meets.len(), 1, "{c:?}");
    let m = c.meets[0];
    assert_eq!(
        (m.a, m.b, m.multiplicity, m.exact),
        ([1.0, 1.0], [0.0, 0.0], 2, true)
    );
    assert_eq!(m.at, p(0.0, 1.0));
}

#[test]
fn a_corner_joint_is_one_simple_meet() {
    let a = line(p(0.0, 0.0), p(1.0, 0.0));
    let b = line(p(1.0, 0.0), p(1.0, 1.0));
    let c = cross(&a, &b);
    assert_eq!(c.meets.len(), 1, "{c:?}");
    assert_eq!(
        (c.meets[0].a, c.meets[0].b, c.meets[0].multiplicity),
        ([1.0, 1.0], [0.0, 0.0], 1)
    );
    // An arc's end on a line's middle.
    let arc = &circle(p(0.5, 1.0), 1.0, -PI / 2.0, 0.0)[0];
    let c = cross(&a, arc);
    assert_eq!(c.meets.len(), 1, "{c:?}");
    assert_eq!(c.meets[0].b, [0.0, 0.0]);
    assert!((mid(c.meets[0].a) - 0.5).abs() <= RES, "{c:?}");
}

#[test]
fn consecutive_arcs_of_one_circle_meet_only_at_their_joint() {
    let arcs = circle(p(0.0, 0.0), 1.0, 0.0, PI);
    let c = cross(&arcs[0], &arcs[1]);
    // On one supporting conic: a stretch no longer than the resolution at the joint.
    for o in &c.overlaps {
        assert!(o.a[0] >= 1.0 - 1e-5 && o.b[1] <= 1e-5, "{c:?}");
    }
    for m in &c.meets {
        assert!(m.a[0] >= 1.0 - 1e-5 && m.b[1] <= 1e-5, "{c:?}");
    }
    assert!(!c.meets.is_empty() || !c.overlaps.is_empty(), "{c:?}");
}

#[test]
fn lines_cross_overlap_and_miss() {
    let a = line(p(0.0, 0.0), p(4.0, 2.0));
    let c = cross(&a, &line(p(0.0, 2.0), p(4.0, 0.0)));
    assert_eq!(c.meets.len(), 1);
    assert_eq!(c.meets[0].a, [0.5, 0.5]);
    assert_eq!(c.meets[0].at, p(2.0, 1.0));
    // Collinear, sharing [2, 4] of x.
    let c = cross(&a, &line(p(2.0, 1.0), p(6.0, 3.0)));
    assert!(c.meets.is_empty(), "{c:?}");
    assert_eq!(c.overlaps.len(), 1, "{c:?}");
    let o = c.overlaps[0];
    assert!((o.a[0] - 0.5).abs() <= RES && o.a[1] == 1.0, "{o:?}");
    assert!(o.b[0].abs() <= RES && (o.b[1] - 0.5).abs() <= RES, "{o:?}");
    // Parallel 1e-9 apart: no meet.
    let c = cross(&a, &line(p(0.0, 1e-9), p(4.0, 2.0 + 1e-9)));
    assert_eq!(c, Crossings::default());
    // Nearly parallel, crossing at x = 2.
    let c = cross(&a, &line(p(0.0, -1e-9), p(4.0, 2.0 + 1e-9)));
    assert_eq!(c.meets.len(), 1, "{c:?}");
    assert!(dist(c.meets[0].at, p(2.0, 1.0)) <= 2.0 * RES, "{c:?}");
    // Apart.
    assert!(cross(&a, &line(p(5.0, 0.0), p(6.0, 1.0))).meets.is_empty());
}

#[test]
fn a_line_through_a_circle_crosses_twice_and_a_secant_beyond_the_arc_not_at_all() {
    let arcs = circle(p(0.0, 0.0), 1.0, 0.3, 0.3 + 2.0 * PI);
    let l = line(p(-2.0, 0.5), p(2.0, 0.5));
    let mut found: Vec<Point2> = arcs
        .iter()
        .flat_map(|a| cross(&l, a).meets.into_iter().map(|m| m.at))
        .collect();
    found.sort_by(|a, b| a.x.total_cmp(&b.x));
    let x = (1.0_f64 - 0.25).sqrt();
    assert_eq!(found.len(), 2, "{found:?}");
    assert!(dist(found[0], p(-x, 0.5)) <= 2.0 * RES && dist(found[1], p(x, 0.5)) <= 2.0 * RES);
    // The same line against the quarter below it: its supporting circle is cut, the arc not.
    let below = &circle(p(0.0, 0.0), 1.0, PI + 0.3, PI + 0.3 + PI / 2.0)[0];
    assert!(cross(&l, below).meets.is_empty());
}

#[test]
fn a_parabola_and_a_hyperbola_cross_their_chords() {
    // Weight 1 is a parabola, 2 a hyperbola: each crosses the line through its ends' mid
    // height twice, at the closed-form points of y(t) = 2 w t (1 - t) / D(t) = 1/4.
    for w in [1.0, 2.0] {
        let c = Curve2::Conic {
            start: p(0.0, 0.0),
            apex: p(1.0, 1.0),
            end: p(2.0, 0.0),
            weight: w,
        };
        let m = cross(&c, &line(p(-1.0, 0.25), p(3.0, 0.25))).meets;
        assert_eq!(m.len(), 2, "{w} {m:?}");
        for x in &m {
            assert!((x.at.y - 0.25).abs() <= 2.0 * RES, "{x:?}");
        }
    }
}

#[test]
fn a_flat_conic_crosses_a_line_and_two_flat_conics_wait() {
    let flat = Curve2::Conic {
        start: p(0.0, 0.0),
        apex: p(1.0, 1.0),
        end: p(2.0, 2.0),
        weight: 0.7,
    };
    let c = cross(&flat, &line(p(0.0, 2.0), p(2.0, 0.0)));
    assert_eq!(c.meets.len(), 1, "{c:?}");
    assert!(dist(c.meets[0].at, p(1.0, 1.0)) <= 2.0 * RES);
    let e = crossings(&flat, &flat, res(), &mut Budget::unlimited(), &Stop::new());
    assert_eq!(e, Err(CrossError::Pending(Pending::FlatPair)));
}

#[test]
fn a_bezier_curve_waits_on_the_planar_widening() {
    let bz = Curve2::Bezier {
        points: vec![p(0.0, 0.0), p(1.0, 2.0), p(2.0, -2.0), p(3.0, 0.0)],
        weights: vec![1.0; 4],
    };
    let e = crossings(
        &bz,
        &line(p(0.0, 0.0), p(1.0, 0.0)),
        res(),
        &mut Budget::unlimited(),
        &Stop::new(),
    )
    .unwrap_err();
    assert_eq!(e, CrossError::Pending(Pending::Bezier { degree: 3 }));
    assert_eq!((e.kind(), e.name()), (ErrorKind::Unsupported, "pending"));
}

#[test]
fn bad_numbers_are_refused() {
    let ok = line(p(0.0, 0.0), p(1.0, 0.0));
    let go = |a: &Curve2, b: &Curve2| {
        crossings(a, b, res(), &mut Budget::unlimited(), &Stop::new()).unwrap_err()
    };
    let nan = line(p(f64::NAN, 0.0), p(1.0, 0.0));
    assert_eq!(go(&ok, &nan), CrossError::NotFinite { curve: Which::B });
    let far = line(p(0.0, 0.0), p(1e10, 0.0));
    assert_eq!(go(&far, &ok), CrossError::OutOfRange { curve: Which::A });
    let tiny = line(p(0.0, 0.0), p(1e-101, 1.0));
    assert_eq!(go(&ok, &tiny), CrossError::OutOfRange { curve: Which::B });
    let point = line(p(1.0, 1.0), p(1.0, 1.0));
    assert_eq!(go(&point, &ok), CrossError::Degenerate { curve: Which::A });
    let zero_weight = Curve2::Conic {
        start: p(0.0, 0.0),
        apex: p(1.0, 1.0),
        end: p(2.0, 0.0),
        weight: 0.0,
    };
    let e = go(&ok, &zero_weight);
    assert_eq!(e, CrossError::Degenerate { curve: Which::B });
    assert_eq!((e.kind(), e.name()), (ErrorKind::Refused, "degenerate"));
}

#[test]
fn the_budget_and_the_stop_end_the_call() {
    let a = &circle(p(0.0, 0.0), 1.0, 0.1, 1.6)[0];
    let b = &circle(p(1.0, 0.2), 1.0, 1.5, 3.0)[0];
    let full = crossings(a, b, res(), &mut Budget::unlimited(), &Stop::new()).unwrap();
    assert_eq!(full.meets.len(), 1, "{full:?}");
    let mut budget = Budget::new(2);
    let e = crossings(a, b, res(), &mut budget, &Stop::new()).unwrap_err();
    assert_eq!(
        (e.kind(), e.name()),
        (ErrorKind::Exhausted, "budget_exhausted")
    );
    let stop = Stop::new();
    stop.set();
    let e = crossings(a, b, res(), &mut Budget::unlimited(), &stop).unwrap_err();
    assert_eq!((e.kind(), e.name()), (ErrorKind::Cancelled, "stopped"));
}

#[test]
fn swapping_the_curves_swaps_the_brackets_and_repeats_bit_for_bit() {
    let one = circle(p(0.0, 0.0), 1.0, 0.1, 0.1 + 2.0 * PI);
    let two = circle(p(1.2, 0.3), 0.9, 0.2, 0.2 + 2.0 * PI);
    let l = line(p(-2.0, 0.4), p(2.0, 0.45));
    let curves: Vec<Curve2> = one.into_iter().chain(two).chain([l]).collect();
    for x in &curves {
        for y in &curves {
            if x == y {
                continue;
            }
            let ab = cross(x, y);
            let ba = cross(y, x);
            assert_eq!(ab, cross(x, y));
            let mut back: Vec<_> = ba
                .meets
                .iter()
                .map(|n| (n.b, n.a, n.multiplicity))
                .collect();
            back.sort_by(|u, v| u.0[0].total_cmp(&v.0[0]));
            let fore: Vec<_> = ab
                .meets
                .iter()
                .map(|m| (m.a, m.b, m.multiplicity))
                .collect();
            assert_eq!(fore, back);
        }
    }
}
