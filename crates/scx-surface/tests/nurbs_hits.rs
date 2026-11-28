// Written for solidcorex. Apache-2.0.
//! An analytic curve against a NURBS surface through [`intersect_curve`] (research 77 §6.3,
//! RY2), judged against closed forms:
//! - a line through the exact rational sphere meets it twice, each at `|p - c| = r` and at
//!   the `t` the quadratic gives; one through the seam and the knot lines (the equator, the
//!   quarter meridians) gives no doubled or lost hit;
//! - a line tangent to the exact rational cylinder gives one tangent at the touching point;
//! - a line along a ruling of a bilinear patch gives one overlap over the ruling;
//! - a circle crossing the sphere gives its two crossings;
//! - a NURBS curve stays [`SurfaceError::NurbsIntersection`]; the budget and the stop end the
//!   search with their errors; a search that finishes gives the same bits under any budget.

#![allow(
    missing_docs,
    clippy::pedantic,
    clippy::indexing_slicing,
    clippy::unwrap_used,
    clippy::float_cmp,
    clippy::panic
)]

use core::f64::consts::TAU;

use scx_curve::{Circle, Curve, Line};
use scx_num::budget::{Budget, Stop};
use scx_num::domain::ParamDomain;
use scx_num::extent1d::Extent1d;
use scx_num::extent2d::Extent2d;
use scx_num::fmath;
use scx_num::frame::{Frame, Sense};
use scx_num::units::{Length, Resolution};
use scx_num::vector3d::{Point3, Vec3};
use scx_nurbs::KnotVector;
use scx_nurbs_surface::{Net, NurbsSurface};
use scx_surface::{
    Contact, CurveSurfaceHit, Cylinder, Sphere, Surface, SurfaceError, intersect_curve,
};

const TOL: f64 = 1e-6;

fn zone() -> Resolution {
    Resolution::new(Length::from_mm(TOL)).unwrap()
}

fn bounds(s: &Surface, v_far: f64) -> Extent2d {
    let side = |d: ParamDomain| match d {
        ParamDomain::Periodic { period, base, .. } => (base, base + period),
        ParamDomain::Bounded(e) => (e.min(), e.max()),
        _ => (-v_far, v_far),
    };
    let d = s.domain();
    let (u, v) = (side(d.u), side(d.v));
    Extent2d::from_bounds(u.0, v.0, u.1, v.1).unwrap()
}

/// The exact rational sphere of radius `r` about `c`, over its whole domain.
fn sphere(c: Point3, r: f64) -> (Surface, Extent2d) {
    let f = Frame::new(c, Vec3::new(0.0, 0.0, 1.0), Vec3::new(1.0, 0.0, 0.0)).unwrap();
    let s: Surface = Sphere::new(f, Length::from_mm(r), Sense::Forward)
        .unwrap()
        .into();
    let se = bounds(&s, 0.0);
    (Surface::from(s.to_nurbs(&se).unwrap()), se)
}

fn line(o: [f64; 3], d: [f64; 3]) -> Curve {
    Line::new(Point3::new(o[0], o[1], o[2]), Vec3::new(d[0], d[1], d[2]))
        .unwrap()
        .into()
}

fn hits(
    c: &Curve,
    ce: &Extent1d,
    s: &Surface,
    se: &Extent2d,
) -> Result<Vec<CurveSurfaceHit>, SurfaceError> {
    intersect_curve(c, ce, s, se, zone(), &mut Budget::unlimited(), &Stop::new())
}

fn points(h: &[CurveSurfaceHit]) -> Vec<(f64, Point3, Contact)> {
    h.iter()
        .map(|h| match h {
            CurveSurfaceHit::Point {
                t, point, contact, ..
            } => (*t, *point, *contact),
            CurveSurfaceHit::Overlap { t } => panic!("an overlap {t:?} in {h:?}"),
        })
        .collect()
}

/// The two `t` at which `o + t d` meets the sphere, from the quadratic.
fn roots(o: [f64; 3], d: [f64; 3], c: Point3, r: f64) -> Option<[f64; 2]> {
    let q = [o[0] - c.x, o[1] - c.y, o[2] - c.z];
    let a = d[0] * d[0] + d[1] * d[1] + d[2] * d[2];
    let b = 2.0 * (q[0] * d[0] + q[1] * d[1] + q[2] * d[2]);
    let k = q[0] * q[0] + q[1] * q[1] + q[2] * q[2] - r * r;
    let disc = b * b - 4.0 * a * k;
    (disc > 0.0).then(|| {
        let s = disc.sqrt();
        [(-b - s) / (2.0 * a), (-b + s) / (2.0 * a)]
    })
}

#[test]
fn a_line_through_the_sphere_meets_it_at_the_radius() {
    let (c, r) = (Point3::new(0.3, -0.2, 0.1), 2.0);
    let (s, se) = sphere(c, r);
    let ce = Extent1d::new(-50.0, 50.0).unwrap();
    let mut n = 0;
    // Off-centre lines in many directions, none through a pole.
    for i in 0..24 {
        let a = 0.37 + i as f64 * 0.61;
        let b = 0.11 + i as f64 * 0.29;
        let d = [
            fmath::cos(a) * fmath::cos(b),
            fmath::sin(a) * fmath::cos(b),
            0.8 * fmath::sin(b),
        ];
        let off = (i % 5) as f64 * 0.35;
        let o = [
            c.x + off * fmath::sin(a),
            c.y - off * fmath::cos(a),
            c.z + 0.05 * i as f64 - 0.4,
        ];
        let want = roots(o, d, c, r).unwrap();
        let got = points(&hits(&line(o, d), &ce, &s, &se).unwrap());
        assert_eq!(got.len(), 2, "line {i}: {got:?}");
        for ((t, p, contact), w) in got.iter().zip(want) {
            assert_eq!(*contact, Contact::Crossing, "line {i}");
            let dist = (*p - c).length();
            assert!((dist - r).abs() <= 4.0 * TOL, "line {i}: |p| = {dist}");
            let len = (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt();
            assert!((t - w).abs() * len <= 4.0 * TOL, "line {i}: t {t} vs {w}");
            n += 1;
        }
    }
    assert_eq!(n, 48);
}

#[test]
fn a_line_through_the_seam_and_the_knot_lines_meets_each_place_once() {
    let (c, r) = (Point3::new(0.0, 0.0, 0.0), 1.5);
    let (s, se) = sphere(c, r);
    let ce = Extent1d::new(-10.0, 10.0).unwrap();
    // Along x and y through the centre on the equator: the seam (u = 0), the quarter
    // meridians and the equator are all knot lines of the exact rational image.
    for d in [
        [1.0, 0.0, 0.0],
        [0.0, 1.0, 0.0],
        [-1.0, 0.0, 0.0],
        [1.0, 1.0, 0.0],
    ] {
        let got = points(&hits(&line([0.0; 3], d), &ce, &s, &se).unwrap());
        assert_eq!(got.len(), 2, "{d:?}: {got:?}");
        let len = (d[0] * d[0] + d[1] * d[1]).sqrt();
        for ((t, p, contact), w) in got.iter().zip([-r / len, r / len]) {
            assert_eq!(*contact, Contact::Crossing);
            assert!((t - w).abs() * len <= 4.0 * TOL, "{d:?}: {t} vs {w}");
            assert!(((*p - c).length() - r).abs() <= 4.0 * TOL);
        }
    }
    // Through a knot line off the equator, crossing the meridian u = a quarter.
    let got = points(&hits(&line([0.0, -5.0, 0.7], [0.0, 1.0, 0.0]), &ce, &s, &se).unwrap());
    assert_eq!(got.len(), 2, "{got:?}");
}

#[test]
fn a_line_tangent_to_the_cylinder_touches_it_once() {
    let f = Frame::new(
        Point3::new(0.0, 0.0, 0.0),
        Vec3::new(0.0, 0.0, 1.0),
        Vec3::new(1.0, 0.0, 0.0),
    )
    .unwrap();
    let cyl: Surface = Cylinder::new(f, Length::from_mm(1.0), Sense::Forward)
        .unwrap()
        .into();
    let se = bounds(&cyl, 5.0);
    let s = Surface::from(cyl.to_nurbs(&se).unwrap());
    let ce = Extent1d::new(-4.0, 4.0).unwrap();
    // x = 1 along y at height 0.3 (touching at u = 0, the seam), and x = -1 along y
    // (touching across a knot line), and a skew tangent at y = 1.
    for (o, d, at) in [
        ([1.0, -2.0, 0.3], [0.0, 1.0, 0.0], [1.0, 0.0, 0.3]),
        ([-1.0, 0.5, -1.2], [0.0, -1.0, 0.0], [-1.0, 0.0, -1.2]),
        ([-0.7, 1.0, 0.2], [1.0, 0.0, 0.4], [0.0, 1.0, 0.48]),
    ] {
        let got = points(&hits(&line(o, d), &ce, &s, &se).unwrap());
        assert_eq!(got.len(), 1, "{o:?}: {got:?}");
        let (_, p, contact) = got[0];
        assert_eq!(contact, Contact::Tangent, "{o:?}");
        let want = Point3::new(at[0], at[1], at[2]);
        // A tangent's place along the line is good to sqrt(tol r).
        assert!(
            (p - want).length() <= 4.0 * (TOL * 1.0f64).sqrt(),
            "{o:?}: {p:?}"
        );
    }
}

/// `z = u v` over the unit square: the hyperbolic paraboloid, straight along each parameter.
fn saddle() -> NurbsSurface<3> {
    let k = || KnotVector::new(1, vec![0.0, 0.0, 1.0, 1.0]).unwrap();
    let points = vec![
        [0.0, 0.0, 0.0],
        [0.0, 1.0, 0.0],
        [1.0, 0.0, 0.0],
        [1.0, 1.0, 1.0],
    ];
    NurbsSurface::new(k(), k(), Net::new(2, 2, points).unwrap(), None).unwrap()
}

#[test]
fn a_line_along_a_ruling_of_a_bilinear_patch_overlaps_it() {
    let s = Surface::from(saddle());
    let se = Extent2d::from_bounds(0.0, 0.0, 1.0, 1.0).unwrap();
    // The ruling u = 0.4: (0.4, v, 0.4 v) for v in [0, 1], at t = v.
    let c = line([0.4, 0.0, 0.0], [0.0, 1.0, 0.4]);
    let got = hits(&c, &Extent1d::new(-1.0, 2.0).unwrap(), &s, &se).unwrap();
    let [CurveSurfaceHit::Overlap { t }] = got.as_slice() else {
        panic!("{got:?}");
    };
    let len = (1.0f64 + 0.16).sqrt();
    assert!(t.min().abs() * len <= 4.0 * TOL, "{t:?}");
    assert!((t.max() - 1.0).abs() * len <= 4.0 * TOL, "{t:?}");
    // Clipped by the curve's own extent.
    let got = hits(&c, &Extent1d::new(0.25, 0.5).unwrap(), &s, &se).unwrap();
    assert_eq!(
        got,
        vec![CurveSurfaceHit::Overlap {
            t: Extent1d::new(0.25, 0.5).unwrap()
        }]
    );
    // A line across the saddle crosses it once: z = 0.5 over (u, 0.6) meets u v at u = 5/6.
    let c = line([0.0, 0.6, 0.5], [1.0, 0.0, 0.0]);
    let got = points(&hits(&c, &Extent1d::new(-1.0, 2.0).unwrap(), &s, &se).unwrap());
    assert_eq!(got.len(), 1, "{got:?}");
    assert!((got[0].0 - 5.0 / 6.0).abs() <= 4.0 * TOL);
}

#[test]
fn a_circle_crosses_the_sphere_twice() {
    let (c, r) = (Point3::new(0.0, 0.0, 0.0), 1.0);
    let (s, se) = sphere(c, r);
    // In the plane y = 0.2, radius 1 about (1, 0.2, 0): it crosses x^2 + z^2 = 0.96 where
    // (x - 1)^2 + z^2 = 1, at x = 0.48.
    let f = Frame::new(
        Point3::new(1.0, 0.2, 0.0),
        Vec3::new(0.0, 1.0, 0.0),
        Vec3::new(0.0, 0.0, 1.0),
    )
    .unwrap();
    let circle: Curve = Circle::new(f, Length::from_mm(1.0), Sense::Forward)
        .unwrap()
        .into();
    let got = points(&hits(&circle, &Extent1d::new(0.0, TAU).unwrap(), &s, &se).unwrap());
    assert_eq!(got.len(), 2, "{got:?}");
    for (_, p, contact) in got {
        assert_eq!(contact, Contact::Crossing);
        assert!((p.x - 0.48).abs() <= 4.0 * TOL, "{p:?}");
        assert!(
            (p.distance(Point3::new(0.0, 0.0, 0.0)) - r).abs() <= 4.0 * TOL,
            "{p:?}"
        );
    }
}

#[test]
fn a_nurbs_curve_is_named_and_the_budget_and_stop_end_the_search() {
    let (s, se) = sphere(Point3::new(0.0, 0.0, 0.0), 1.0);
    let k = KnotVector::new(1, vec![0.0, 0.0, 1.0, 1.0]).unwrap();
    let n = scx_nurbs::NurbsCurve::new(k, vec![[-2.0, 0.1, 0.0], [2.0, 0.1, 0.0]], None).unwrap();
    let e = hits(
        &Curve::Nurbs(std::sync::Arc::new(n)),
        &Extent1d::new(0.0, 1.0).unwrap(),
        &s,
        &se,
    )
    .unwrap_err();
    assert_eq!(e, SurfaceError::NurbsIntersection);
    let c = line([-3.0, 0.1, 0.2], [1.0, 0.05, 0.02]);
    let ce = Extent1d::new(-10.0, 10.0).unwrap();
    let mut budget = Budget::new(3);
    let e = intersect_curve(&c, &ce, &s, &se, zone(), &mut budget, &Stop::new()).unwrap_err();
    assert!(matches!(e, SurfaceError::Exhausted(_)), "{e:?}");
    let stop = Stop::new();
    stop.set();
    let e = intersect_curve(&c, &ce, &s, &se, zone(), &mut Budget::unlimited(), &stop).unwrap_err();
    assert_eq!(e, SurfaceError::Stopped);
    // The same bits under any budget that lets it finish.
    let mut free = Budget::unlimited();
    let want = intersect_curve(&c, &ce, &s, &se, zone(), &mut free, &Stop::new()).unwrap();
    let used = free.used();
    assert_eq!(want.len(), 2);
    for limit in [used, used + 1, 10 * used] {
        let got = intersect_curve(
            &c,
            &ce,
            &s,
            &se,
            zone(),
            &mut Budget::new(limit),
            &Stop::new(),
        )
        .unwrap();
        assert_eq!(got, want);
    }
    let e = intersect_curve(
        &c,
        &ce,
        &s,
        &se,
        zone(),
        &mut Budget::new(used - 1),
        &Stop::new(),
    )
    .unwrap_err();
    assert!(matches!(e, SurfaceError::Exhausted(_)), "{e:?}");
}

#[test]
fn a_line_through_a_pole_is_undecided_or_right() {
    // The exact sphere's poles are degenerate patch edges: the search either proves the two
    // hits or says it cannot decide, never an empty or a wrong answer.
    let (s, se) = sphere(Point3::new(0.0, 0.0, 0.0), 1.0);
    let ce = Extent1d::new(-3.0, 3.0).unwrap();
    match hits(&line([0.0; 3], [0.0, 0.0, 1.0]), &ce, &s, &se) {
        Ok(h) => {
            let got = points(&h);
            assert_eq!(got.len(), 2, "{got:?}");
            for ((t, ..), w) in got.iter().zip([-1.0, 1.0]) {
                assert!((t - w).abs() <= 4.0 * TOL, "{t}");
            }
        }
        Err(e) => assert!(matches!(e, SurfaceError::Undecided { .. }), "{e:?}"),
    }
}
