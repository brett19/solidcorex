// Written for solidcorex. Apache-2.0.
//! The certificates against closed forms: exact rational forms proved, perturbed ones
//! refuted with a witness whose bound is at most its true distance, and every proved bound
//! at least the largest sampled distance (research 75 §8.2, MK10's row).

#![allow(
    clippy::unwrap_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::float_cmp
)]

use std::sync::Arc;

use scx_curve::Circle;
use scx_num::budget::{Budget, Stop};
use scx_num::extent1d::Extent1d;
use scx_num::extent2d::Extent2d;
use scx_num::frame::{Frame, Sense};
use scx_num::units::{Accuracy, Length};
use scx_num::vector2d::Point2;
use scx_num::vector3d::{Point3, Vec3};
use scx_nurbs::{Approach, KnotVector, NurbsCurve};
use scx_nurbs_surface::{Net, NurbsSurface, ParamAxis};

use super::*;
use crate::cone::Cone;
use crate::cylinder::Cylinder;
use crate::nurbs_invert::invert;
use crate::plane::Plane;
use crate::sphere::Sphere;
use crate::surface::Surface;
use crate::torus::Torus;

fn mm(x: f64) -> Length {
    Length::from_mm(x)
}

fn acc(x: f64) -> Accuracy {
    Accuracy::new(mm(x)).unwrap()
}

fn frame(o: [f64; 3], z: [f64; 3]) -> Frame {
    Frame::new(
        Point3::new(o[0], o[1], o[2]),
        Vec3::new(z[0], z[1], z[2]),
        Vec3::new(1.0, 0.2, 0.1),
    )
    .unwrap()
}

fn curve(points: &[[f64; 4]], degree: usize, knots: &[f64]) -> NurbsCurve<3> {
    let pts = points.iter().map(|&[x, y, z, _]| [x, y, z]).collect();
    let w = points.iter().map(|p| p[3]).collect();
    NurbsCurve::new(KnotVector::new(degree, knots.to_vec()).unwrap(), pts, Some(w)).unwrap()
}

fn point(c: &NurbsCurve<3>, t: f64) -> Point3 {
    let p = c.point(t, Approach::Above).unwrap();
    Point3::new(p[0], p[1], p[2])
}

/// The distance of `p` from the circle in closed form.
fn off_circle(circle: &Circle, p: Point3) -> f64 {
    let f = circle.frame();
    let d = p - f.origin();
    let (x, y, z) = (d.dot(f.x().as_vec()), d.dot(f.y().as_vec()), d.dot(f.z().as_vec()));
    (x.hypot(y) - circle.radius().mm()).hypot(z)
}

fn largest_off(c: &NurbsCurve<3>, circle: &Circle) -> f64 {
    (0..=1000)
        .map(|i| off_circle(circle, point(c, f64::from(i) / 1000.0)))
        .fold(0.0, f64::max)
}

// ---------------------------------------------------------------- on_circle

/// The quarter circle r 2 about (5, 0, 1) in the plane z = 1, one rational quadratic arc, its
/// middle control point moved out by `bulge` of its distance from the centre (solidcore's
/// `certificate.rs` circle tests).
fn quarter(bulge: f64) -> NurbsCurve<3> {
    let h = core::f64::consts::FRAC_1_SQRT_2;
    let m = 2.0 * (1.0 + bulge);
    curve(
        &[
            [7.0, 0.0, 1.0, 1.0],
            [5.0 + m, m, 1.0, h],
            [5.0, 2.0, 1.0, 1.0],
        ],
        2,
        &[0.0, 0.0, 0.0, 1.0, 1.0, 1.0],
    )
}

fn circle(o: [f64; 3], z: [f64; 3], r: f64) -> Circle {
    Circle::new(frame(o, z), mm(r), Sense::Forward).unwrap()
}

fn on(c: &NurbsCurve<3>, k: &Circle, tol: Accuracy) -> Proof<f64> {
    on_circle(c, k, tol, &mut Budget::unlimited(), &Stop::new()).unwrap()
}

const C: [f64; 3] = [5.0, 0.0, 1.0];
const Z: [f64; 3] = [0.0, 0.0, 1.0];

/// Solidcore `certificate.rs:1658`: an exact arc is proved on its circle at once; at a
/// tolerance of zero (its rounding is never below it, and no point is off by more) it is
/// neither proved nor refuted.
#[test]
fn an_exact_arc_is_proved_on_its_circle() {
    let c = quarter(0.0);
    let k = circle(C, Z, 2.0);
    let Proof::Proved { bound } = on(&c, &k, acc(1e-9)) else {
        panic!("the exact arc is proved");
    };
    assert!(bound.mm() <= 1e-12, "{bound:?}");
    let k3 = circle(C, [0.0, 0.0, 3.0], 2.0);
    assert!(matches!(on(&c, &k3, acc(1e-12)), Proof::Proved { .. }));
    assert!(matches!(
        on(&c, &k, Accuracy::ZERO),
        Proof::Undecided { bound: Some(_) }
    ));
}

/// Solidcore `certificate.rs:1678`: a bulge of 1e-4 of the radius is found past a tolerance
/// of 1e-7 with its point as a witness, and proved within a tolerance above the bulge's
/// height; another radius, centre or plane is refuted. The witness's bound is at most its
/// true distance, and the proved bound at least the largest sampled one.
#[test]
fn a_bulge_or_another_circle_is_refuted() {
    let c = quarter(1e-4);
    let k = circle(C, Z, 2.0);
    let Proof::Refuted { at, by } = on(&c, &k, acc(1e-7)) else {
        panic!("the bulge is a witness");
    };
    assert!((0.0..=1.0).contains(&at), "{at}");
    let truth = off_circle(&k, point(&c, at));
    assert!(1e-7 < by.mm() && by.mm() <= truth, "{by:?} {truth}");
    let Proof::Proved { bound } = on(&c, &k, acc(1e-3)) else {
        panic!("the bulge is within 1e-3");
    };
    let seen = largest_off(&c, &k);
    assert!(seen <= bound.mm() && bound.mm() <= 1e-3, "{seen} {bound:?}");
    let exact = quarter(0.0);
    for other in [
        circle(C, Z, 2.001),
        circle([5.0, 0.0, 1.01], Z, 2.0),
        circle(C, [0.0, 0.1, 1.0], 2.0),
    ] {
        let Proof::Refuted { at, by } = on(&exact, &other, acc(1e-7)) else {
            panic!("{other:?} is refuted");
        };
        assert!(by.mm() <= off_circle(&other, point(&exact, at)));
    }
}

/// Solidcore `certificate.rs:1709`: an exact arc far from the origin is still proved at a
/// tolerance of 1e-9 (the translation about the centre rounds as a length).
#[test]
fn an_exact_arc_far_from_the_origin_is_proved() {
    let h = core::f64::consts::FRAC_1_SQRT_2;
    let f = 1e4;
    let c = curve(
        &[
            [7.0 + f, f, 1.0, 1.0],
            [7.0 + f, 2.0 + f, 1.0, h],
            [5.0 + f, 2.0 + f, 1.0, 1.0],
        ],
        2,
        &[0.0, 0.0, 0.0, 1.0, 1.0, 1.0],
    );
    let k = circle([5.0 + f, f, 1.0], Z, 2.0);
    assert!(matches!(on(&c, &k, acc(1e-9)), Proof::Proved { .. }));
}

/// A budget of one piece runs out on a curve that needs halving; a set stop stops it.
#[test]
fn on_circle_charges_the_budget_and_polls_the_stop() {
    let c = quarter(1e-4);
    let k = circle(C, Z, 2.0);
    // At 1.5e-4 the whole arc's bound (2e-4) is too wide and its middle (1e-4 off) no
    // witness, so it is halved: a second piece.
    let r = on_circle(&c, &k, acc(1.5e-4), &mut Budget::new(1), &Stop::new());
    assert!(matches!(r, Err(SurfaceError::Exhausted(_))), "{r:?}");
    let stop = Stop::new();
    stop.set();
    let r = on_circle(&c, &k, acc(1e-3), &mut Budget::unlimited(), &stop);
    assert!(matches!(r, Err(SurfaceError::Stopped)), "{r:?}");
}

// ---------------------------------------------------------------- turn_about_axis

fn interval(a: f64, b: f64) -> Extent1d {
    Extent1d::new(a, b).unwrap()
}

/// Solidcore `certificate.rs:1575`: a quarter circle about the z axis turns one way, the
/// other way about the opposite axis or run backwards.
#[test]
fn a_quarter_circle_turns_one_way_round_its_axis() {
    let h = core::f64::consts::FRAC_1_SQRT_2;
    let pts = [
        [1.0, 0.0, 3.0, 1.0],
        [1.0, 1.0, 3.0, h],
        [0.0, 1.0, 3.0, 1.0],
    ];
    let knots = [0.0, 0.0, 0.0, 1.0, 1.0, 1.0];
    let c = curve(&pts, 2, &knots);
    let o = Point3::new(0.0, 0.0, 0.0);
    let z = Vec3::new(0.0, 0.0, 1.0).normalized().unwrap();
    let down = Vec3::new(0.0, 0.0, -2.0).normalized().unwrap();
    let turn = |c: &NurbsCurve<3>, a, over| turn_about_axis(c, o, a, over).unwrap();
    assert_eq!(turn(&c, z, interval(0.0, 1.0)), Turn::Positive);
    assert_eq!(turn(&c, down, interval(0.0, 1.0)), Turn::Negative);
    assert_eq!(turn(&c, z, interval(0.25, 0.5)), Turn::Positive);
    let mut back = pts;
    back.reverse();
    assert_eq!(
        turn(&curve(&back, 2, &knots), z, interval(0.0, 1.0)),
        Turn::Negative
    );
}

/// Solidcore `certificate.rs:1601`: a curve that swings round the axis and comes back is not
/// proved, nor is a line through the axis's plane that stands still about it; the part
/// before the turn is.
#[test]
fn a_curve_that_turns_back_or_stands_still_is_not_proved() {
    let o = Point3::new(0.0, 0.0, 0.0);
    let z = Vec3::new(0.0, 0.0, 1.0).normalized().unwrap();
    let knots = [0.0, 0.0, 0.0, 1.0, 1.0, 1.0];
    let back = curve(
        &[
            [1.0, 0.0, 0.0, 1.0],
            [0.0, 3.0, 0.0, 1.0],
            [1.0, 0.5, 0.0, 1.0],
        ],
        2,
        &knots,
    );
    let turn = |c: &NurbsCurve<3>, over| turn_about_axis(c, o, z, over).unwrap();
    assert_eq!(turn(&back, interval(0.0, 1.0)), Turn::Undecided);
    assert_eq!(turn(&back, interval(0.0, 0.2)), Turn::Positive);
    let radial = curve(
        &[[1.0, 0.0, 0.0, 1.0], [2.0, 0.0, 1.0, 1.0]],
        1,
        &[0.0, 0.0, 1.0, 1.0],
    );
    assert_eq!(turn(&radial, interval(0.0, 1.0)), Turn::Undecided);
}

// ---------------------------------------------------------------- on_analytic

fn analytic(n: &NurbsSurface<3>, k: &Analytic, tol: Accuracy) -> Proof<Point2> {
    on_analytic(n, k, tol, &mut Budget::unlimited(), &Stop::new()).unwrap()
}

fn ext(u: (f64, f64), v: (f64, f64)) -> Extent2d {
    Extent2d::from_bounds(u.0, v.0, u.1, v.1).unwrap()
}

/// The largest closed-form distance from `kind` over a 41 x 41 grid of `n`'s domain.
fn largest_from(n: &NurbsSurface<3>, kind: &Surface) -> f64 {
    let s = Surface::Nurbs(Arc::new(n.clone()));
    let (tu, tv) = (n.knots(ParamAxis::U), n.knots(ParamAxis::V));
    let (u0, u1) = (tu[0], tu[tu.len() - 1]);
    let (v0, v1) = (tv[0], tv[tv.len() - 1]);
    let mut worst = 0.0_f64;
    for i in 0..=40 {
        for j in 0..=40 {
            let uv = Point2::new(
                u0 + (u1 - u0) * f64::from(i) / 40.0,
                v0 + (v1 - v0) * f64::from(j) / 40.0,
            );
            worst = worst.max(from_kind(kind, s.point(uv)));
        }
    }
    worst
}

/// The distance of `p` from the cylinder `kind`, in closed form.
fn from_kind(kind: &Surface, p: Point3) -> f64 {
    let Surface::Cylinder(c) = kind else {
        panic!("a cylinder");
    };
    let f = c.frame();
    let d = p - f.origin();
    let z = f.z().as_vec();
    let radial = d - z * d.dot(z);
    (radial.length() - c.radius().mm()).abs()
}

/// Every analytic kind's exact rational image is proved on the kind, its bound rounding;
/// another radius is refuted.
#[test]
fn every_kind_s_exact_image_is_proved() {
    let f = frame([1.5, -2.0, 0.7], [0.3, -0.4, 0.86]);
    let kinds = [
        (
            Surface::Plane(Plane::new(f)),
            ext((-3.0, 4.0), (-1.0, 2.0)),
        ),
        (
            Surface::Cylinder(Cylinder::new(f, mm(2.5), Sense::Forward).unwrap()),
            ext((0.2, 3.5), (-1.0, 4.0)),
        ),
        (
            Surface::Cylinder(Cylinder::new(f, mm(2.5), Sense::Reversed).unwrap()),
            ext((0.2, 3.5), (-1.0, 4.0)),
        ),
        (
            Surface::Cone(Cone::new(f, mm(2.0), 0.4, Sense::Forward).unwrap()),
            ext((0.0, 0.4), (-1.0, 3.0)),
        ),
        (
            Surface::Cone(Cone::new(f, mm(2.0), 0.4, Sense::Reversed).unwrap()),
            ext((0.0, 0.4), (-1.0, 3.0)),
        ),
        (
            Surface::Sphere(Sphere::new(f, mm(3.0), Sense::Forward).unwrap()),
            ext((0.05, 0.3), (0.3, 0.7)),
        ),
        (
            Surface::Torus(Torus::new(f, mm(5.0), mm(1.5), Sense::Forward).unwrap()),
            ext((0.1, 0.3), (-0.1, 0.3)),
        ),
    ];
    for (s, over) in kinds {
        let n = s.to_nurbs(&over).unwrap();
        let k = Analytic::of(&s).unwrap();
        let Proof::Proved { bound } = analytic(&n, &k, acc(1e-9)) else {
            panic!("{s:?} is proved");
        };
        assert!(bound.mm() <= 1e-9, "{s:?}: {bound:?}");
    }
    assert!(Analytic::of(&Surface::Nurbs(Arc::new(
        Surface::Plane(Plane::new(f))
            .to_nurbs(&ext((0.0, 1.0), (0.0, 1.0)))
            .unwrap()
    )))
    .is_none());
    let c = Surface::Cylinder(Cylinder::new(f, mm(2.5), Sense::Forward).unwrap());
    let n = c.to_nurbs(&ext((0.2, 1.5), (-1.0, 4.0))).unwrap();
    let wider = Surface::Cylinder(Cylinder::new(f, mm(2.51), Sense::Forward).unwrap());
    let Proof::Refuted { by, .. } = analytic(&n, &Analytic::of(&wider).unwrap(), acc(1e-6))
    else {
        panic!("another radius is refuted");
    };
    assert!(1e-6 < by.mm() && by.mm() <= 0.01 * (1.0 + 1e-9), "{by:?}");
}

/// A rational cylinder with one interior control point moved out by 1e-3: refuted at 1e-6,
/// the witness's bound above the tolerance and at most its true distance; proved at 1e-2, the
/// bound at least the largest sampled distance.
#[test]
fn a_perturbed_cylinder_is_refuted_and_bounded() {
    let f = frame([1.5, -2.0, 0.7], [0.3, -0.4, 0.86]);
    let s = Surface::Cylinder(Cylinder::new(f, mm(2.5), Sense::Forward).unwrap());
    let base = s.to_nurbs(&ext((0.2, 1.5), (-1.0, 4.0))).unwrap();
    let (rows, cols) = (base.points().nrows(), base.points().ncols());
    let mut pts = base.points().points().to_vec();
    // An interior point of the net, moved along its radial direction.
    let at = (rows / 2) * cols + cols / 2;
    let p = Point3::new(pts[at][0], pts[at][1], pts[at][2]);
    let z = f.z().as_vec();
    let d = p - f.origin();
    let out = (d - z * d.dot(z)).normalized().unwrap().as_vec() * 1e-3;
    pts[at] = [p.x + out.x, p.y + out.y, p.z + out.z];
    let n = NurbsSurface::new(
        base.knot_vector(ParamAxis::U).clone(),
        base.knot_vector(ParamAxis::V).clone(),
        Net::new(rows, cols, pts).unwrap(),
        base.weights().cloned(),
    )
    .unwrap();
    let k = Analytic::of(&s).unwrap();
    let Proof::Refuted { at, by } = analytic(&n, &k, acc(1e-6)) else {
        panic!("the bump is refuted");
    };
    let truth = from_kind(&s, Surface::Nurbs(Arc::new(n.clone())).point(at));
    assert!(1e-6 < by.mm() && by.mm() <= truth, "{by:?} {truth}");
    let Proof::Proved { bound } = analytic(&n, &k, acc(1e-2)) else {
        panic!("the bump is within 1e-2");
    };
    let seen = largest_from(&n, &s);
    assert!(
        seen > 1e-5 && seen <= bound.mm() && bound.mm() <= 1e-2,
        "{seen} {bound:?}"
    );
}

// ---------------------------------------------------------------- span_bound

/// The circle of radius `r` about the z axis at height `z`, a quarter turn from the x axis:
/// one rational quadratic arc.
fn rail_arc(r: f64, z: f64) -> NurbsCurve<3> {
    let h = core::f64::consts::FRAC_1_SQRT_2;
    curve(
        &[[r, 0.0, z, 1.0], [r, r, z, h], [0.0, r, z, 1.0]],
        2,
        &[0.0, 0.0, 0.0, 1.0, 1.0, 1.0],
    )
}

/// The cylinder of radius 10 about the z axis, as its exact NURBS image over a half turn
/// (`u` is arc length) and heights 0 to 6.
fn wall() -> (Surface, NurbsSurface<3>) {
    let f = Frame::new(
        Point3::new(0.0, 0.0, 0.0),
        Vec3::new(0.0, 0.0, 1.0),
        Vec3::new(1.0, 0.0, 0.0),
    )
    .unwrap();
    let s = Surface::Cylinder(Cylinder::new(f, mm(10.0), Sense::Forward).unwrap());
    let u = (0.0, 10.0 * core::f64::consts::PI);
    let n = s.to_nurbs(&ext(u, (0.0, 6.0))).unwrap();
    (s, n)
}

fn node(n: &NurbsSurface<3>, rail: &NurbsCurve<3>, t: f64) -> Node {
    let foot = invert(
        n,
        point(rail, t),
        None,
        None,
        &mut Budget::unlimited(),
        &Stop::new(),
    )
    .unwrap()
    .into_foot();
    Node { t, uv: foot.at }
}

fn bound_of(n: &NurbsSurface<3>, rail: &NurbsCurve<3>, tol: f64) -> f64 {
    let (a, b) = (node(n, rail, 0.0), node(n, rail, 1.0));
    match span_bound(n, rail, a, b, acc(tol), &mut Budget::unlimited(), &Stop::new()).unwrap() {
        SpanBound::Bounded(b) => b.mm(),
        SpanBound::Unbounded => panic!("a bound holds"),
    }
}

/// A rail circle 0.5 outside the cylinder: the bound is at least the exact distance 0.5 and
/// within the tolerance just above it; a rail on the cylinder is bounded by rounding; a free
/// cubic rail's bound is at least its largest sampled distance from the cylinder.
#[test]
fn span_bound_lies_above_the_exact_distance() {
    let (s, n) = wall();
    let off = bound_of(&n, &rail_arc(10.5, 3.0), 0.5005);
    assert!((0.5..=0.5005).contains(&off), "{off}");
    let on = bound_of(&n, &rail_arc(10.0, 3.0), 1e-6);
    assert!(on <= 1e-6, "{on}");
    let free = curve(
        &[
            [10.2, 0.0, 1.0, 1.0],
            [10.2, 5.6, 2.0, 1.0],
            [5.6, 10.2, 3.0, 1.0],
            [0.0, 10.2, 4.0, 1.0],
        ],
        3,
        &[0.0, 0.0, 0.0, 0.0, 1.0, 1.0, 1.0, 1.0],
    );
    let b = bound_of(&n, &free, 0.3);
    let seen = (0..=200)
        .map(|i| from_kind(&s, point(&free, f64::from(i) / 200.0)))
        .fold(0.0, f64::max);
    assert!(seen <= b && b < 1.0, "{seen} {b}");
}

/// A node off the rail's domain or the surface's is refused, typed.
#[test]
fn span_bound_refuses_nodes_off_the_domains() {
    let (_, n) = wall();
    let rail = rail_arc(10.5, 3.0);
    let a = node(&n, &rail, 0.0);
    let b = node(&n, &rail, 1.0);
    let run = |a: Node, b: Node| {
        span_bound(&n, &rail, a, b, acc(0.6), &mut Budget::unlimited(), &Stop::new())
    };
    assert!(matches!(
        run(Node { t: 1.5, ..b }, a),
        Err(SurfaceError::OffRail { at }) if at == 1.5
    ));
    assert!(matches!(
        run(a, Node { uv: Point2::new(a.uv.x, 99.0), ..b }),
        Err(SurfaceError::OutsideDomain { axis: ParamAxis::V, .. })
    ));
}
