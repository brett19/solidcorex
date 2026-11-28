// Written for solidcorex. Apache-2.0.
//! `intersect_curve` and `curve_on_surface` for a NURBS curve against the analytic kinds
//! (research 77 §6.3, RY1), judged against closed forms:
//! - the exact NURBS circle against a plane, a cylinder and a sphere (crossings at its knots
//!   and between them, an overlap), and the same answers as the analytic circle;
//! - a rational quadratic arc against a cone;
//! - a cubic grazing a cylinder: one tangent, two crossings or none past the tolerance;
//! - NURBS lines lying in a plane, wholly and over one span (an overlap from knot to knot);
//! - polylines crossing and touching a plane at a knot: one hit, never two or none;
//! - a line through a torus (the degree-4 equation);
//! - the budget, the stop, and a NURBS surface (still not built).

#![allow(
    missing_docs,
    clippy::pedantic,
    clippy::indexing_slicing,
    clippy::unwrap_used,
    clippy::float_cmp,
    clippy::panic
)]

use core::f64::consts::{FRAC_1_SQRT_2, PI, TAU};

use scx_curve::{Circle, Curve};
use scx_num::budget::{Budget, Stop};
use scx_num::domain::ParamDomain;
use scx_num::extent1d::Extent1d;
use scx_num::extent2d::Extent2d;
use scx_num::fmath;
use scx_num::frame::{Frame, Sense};
use scx_num::units::{Length, Resolution};
use scx_num::vector3d::{Point3, Vec3};
use scx_nurbs::{KnotVector, NurbsCurve};
use scx_surface::{
    Cone, Contact, CurveSurfaceHit, Cylinder, OnSurface, Plane, Sphere, Surface, SurfaceError,
    Torus, curve_on_surface, intersect_curve,
};

const TOL: f64 = 1e-6;
/// A crossing's point is a root to its rounding: a few ulps of the model.
const ROOT: f64 = 1e-11;

fn len(x: f64) -> Length {
    Length::from_mm(x)
}

fn zone(mm: f64) -> Resolution {
    Resolution::new(len(mm)).unwrap()
}

fn frame(o: [f64; 3], z: [f64; 3], x: [f64; 3]) -> Frame {
    let v = |a: [f64; 3]| Vec3::new(a[0], a[1], a[2]);
    Frame::new(Point3::new(o[0], o[1], o[2]), v(z), v(x)).unwrap()
}

/// The surface's whole domain, the unbounded ends at `1e6`.
fn full(s: &Surface) -> Extent2d {
    let side = |d: ParamDomain| match d {
        ParamDomain::Periodic { period, base, .. } => (base, base + period),
        ParamDomain::Bounded(e) => (e.min(), e.max()),
        ParamDomain::AtLeast { from, .. } => (from, 1e6),
        ParamDomain::AtMost { to, .. } => (-1e6, to),
        _ => (-1e6, 1e6),
    };
    let d = s.domain();
    let (u, v) = (side(d.u), side(d.v));
    Extent2d::from_bounds(u.0, v.0, u.1, v.1).unwrap()
}

fn nurbs(degree: usize, knots: Vec<f64>, points: Vec<[f64; 3]>, w: Option<Vec<f64>>) -> Curve {
    let k = KnotVector::new(degree, knots).unwrap();
    NurbsCurve::new(k, points, w).unwrap().into()
}

/// The exact NURBS circle of radius `r` about `o` in the plane spanned by `a` and `b` (unit,
/// orthogonal): nine points, quarter spans, knots at `0, 1/4, 1/2, 3/4, 1` passing through
/// `o + r a`, `o + r b`, `o - r a`, `o - r b`.
fn nurbs_circle(o: [f64; 3], a: [f64; 3], b: [f64; 3], r: f64) -> Curve {
    let at = |x: f64, y: f64| [0, 1, 2].map(|k| o[k] + r * (x * a[k] + y * b[k]));
    let points = vec![
        at(1.0, 0.0),
        at(1.0, 1.0),
        at(0.0, 1.0),
        at(-1.0, 1.0),
        at(-1.0, 0.0),
        at(-1.0, -1.0),
        at(0.0, -1.0),
        at(1.0, -1.0),
        at(1.0, 0.0),
    ];
    let h = FRAC_1_SQRT_2;
    let knots = vec![
        0.0, 0.0, 0.0, 0.25, 0.25, 0.5, 0.5, 0.75, 0.75, 1.0, 1.0, 1.0,
    ];
    nurbs(
        2,
        knots,
        points,
        Some(vec![1.0, h, 1.0, h, 1.0, h, 1.0, h, 1.0]),
    )
}

fn domain(c: &Curve) -> Extent1d {
    let Curve::Nurbs(n) = c else { panic!() };
    n.domain()
}

fn hits(c: &Curve, s: &Surface) -> Result<Vec<CurveSurfaceHit>, SurfaceError> {
    hits_tol(c, s, TOL)
}

fn hits_tol(c: &Curve, s: &Surface, tol: f64) -> Result<Vec<CurveSurfaceHit>, SurfaceError> {
    intersect_curve(
        c,
        &domain(c),
        s,
        &full(s),
        zone(tol),
        &mut Budget::unlimited(),
        &Stop::new(),
    )
}

fn dist(p: Point3, q: [f64; 3]) -> f64 {
    let d = [p.x - q[0], p.y - q[1], p.z - q[2]];
    (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt()
}

/// The point hits are exactly `want` (each within `ROOT` of one, in any order), each with
/// `contact`, at its `t` on the curve, its uv's surface point its own.
fn points_are(
    c: &Curve,
    s: &Surface,
    got: &[CurveSurfaceHit],
    want: &[[f64; 3]],
    contact: Contact,
    reach: f64,
) {
    assert_eq!(got.len(), want.len(), "{got:?}");
    for w in want {
        let hit = got
            .iter()
            .find(
                |h| matches!(h, CurveSurfaceHit::Point { point, .. } if dist(*point, *w) <= reach),
            )
            .unwrap_or_else(|| panic!("no hit at {w:?}: {got:?}"));
        let CurveSurfaceHit::Point {
            t,
            uv,
            point,
            contact: k,
        } = *hit
        else {
            panic!()
        };
        assert_eq!(k, contact, "{hit:?}");
        assert!(
            dist(c.point(t), [point.x, point.y, point.z]) <= ROOT,
            "{hit:?}"
        );
        assert!(
            dist(s.point(uv), [point.x, point.y, point.z]) <= TOL,
            "{hit:?}"
        );
    }
}

#[test]
fn a_nurbs_circle_crosses_a_plane_at_its_knots_and_between_them() {
    let c = nurbs_circle([0.0; 3], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0], 2.0);
    // Through the centre: (0, +-2, 0), at the knots 1/4 and 3/4.
    let s: Surface = Plane::new(frame([0.0; 3], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0])).into();
    let got = hits(&c, &s).unwrap();
    points_are(
        &c,
        &s,
        &got,
        &[[0.0, 2.0, 0.0], [0.0, -2.0, 0.0]],
        Contact::Crossing,
        ROOT,
    );
    let ts: Vec<f64> = got
        .iter()
        .map(|h| match h {
            CurveSurfaceHit::Point { t, .. } => *t,
            CurveSurfaceHit::Overlap { .. } => panic!(),
        })
        .collect();
    assert!(
        (ts[0] - 0.25).abs() < 1e-12 && (ts[1] - 0.75).abs() < 1e-12,
        "{ts:?}"
    );
    // Off the centre: x = 0.6, y = +-sqrt(4 - 0.36), inside the spans.
    let s: Surface = Plane::new(frame([0.6, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0])).into();
    let y = (4.0_f64 - 0.36).sqrt();
    points_are(
        &c,
        &s,
        &hits(&c, &s).unwrap(),
        &[[0.6, y, 0.0], [0.6, -y, 0.0]],
        Contact::Crossing,
        ROOT,
    );
    // Through the start (and end) point: one crossing, kept once at t = 0.
    let s: Surface = Plane::new(frame([0.0; 3], [0.0, 1.0, 0.0], [1.0, 0.0, 0.0])).into();
    let got = hits(&c, &s).unwrap();
    points_are(
        &c,
        &s,
        &got,
        &[[2.0, 0.0, 0.0], [-2.0, 0.0, 0.0]],
        Contact::Crossing,
        ROOT,
    );
    // Touching at x = 2: one tangent.
    let s: Surface = Plane::new(frame([2.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0])).into();
    points_are(
        &c,
        &s,
        &hits(&c, &s).unwrap(),
        &[[2.0, 0.0, 0.0]],
        Contact::Tangent,
        1e-9,
    );
    // Its own plane: the whole circle, proved by the bound.
    let s: Surface = Plane::new(frame([0.0; 3], [0.0, 0.0, 1.0], [1.0, 0.0, 0.0])).into();
    let got = hits(&c, &s).unwrap();
    assert!(
        matches!(got[..], [CurveSurfaceHit::Overlap { t }] if t == domain(&c)),
        "{got:?}"
    );
    let mut budget = Budget::unlimited();
    let on = curve_on_surface(
        &c,
        &domain(&c),
        &s,
        &full(&s),
        zone(TOL),
        &mut budget,
        &Stop::new(),
    )
    .unwrap();
    assert_eq!(on, OnSurface::Proved(vec![domain(&c)]));
    assert_eq!(budget.used(), 1);
}

#[test]
fn a_nurbs_circle_crosses_a_cylinder_and_a_sphere() {
    let r = 2.0;
    let c = nurbs_circle([0.0; 3], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0], r);
    // The cylinder about x of radius 1.2: y = +-1.2, x = +-1.6.
    let s: Surface = Cylinder::new(
        frame([0.0; 3], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]),
        len(1.2),
        Sense::Forward,
    )
    .unwrap()
    .into();
    let want = [
        [1.6, 1.2, 0.0],
        [-1.6, 1.2, 0.0],
        [-1.6, -1.2, 0.0],
        [1.6, -1.2, 0.0],
    ];
    points_are(
        &c,
        &s,
        &hits(&c, &s).unwrap(),
        &want,
        Contact::Crossing,
        1e-10,
    );
    // The sphere about (1, 0, 0) of radius 2: x = (4 + 1 - 4) / 2.
    let s: Surface = Sphere::new(
        frame([1.0, 0.0, 0.0], [0.0, 0.0, 1.0], [1.0, 0.0, 0.0]),
        len(2.0),
        Sense::Forward,
    )
    .unwrap()
    .into();
    let x = 0.5;
    let y = (r * r - x * x).sqrt();
    points_are(
        &c,
        &s,
        &hits(&c, &s).unwrap(),
        &[[x, y, 0.0], [x, -y, 0.0]],
        Contact::Crossing,
        1e-10,
    );
    // The sphere about (0, 0, 1.5) through the circle: the whole of it.
    let s: Surface = Sphere::new(
        frame([0.0, 0.0, 1.5], [0.0, 0.0, 1.0], [1.0, 0.0, 0.0]),
        len(2.5),
        Sense::Forward,
    )
    .unwrap()
    .into();
    let got = hits(&c, &s).unwrap();
    assert!(
        matches!(got[..], [CurveSurfaceHit::Overlap { t }] if t == domain(&c)),
        "{got:?}"
    );
    // A coaxial cylinder of radius 2 holds it; of radius 3, misses it.
    let axis = frame([0.0; 3], [0.0, 0.0, 1.0], [1.0, 0.0, 0.0]);
    let s: Surface = Cylinder::new(axis, len(2.0), Sense::Forward)
        .unwrap()
        .into();
    assert!(matches!(
        hits(&c, &s).unwrap()[..],
        [CurveSurfaceHit::Overlap { .. }]
    ));
    let s: Surface = Cylinder::new(axis, len(3.0), Sense::Forward)
        .unwrap()
        .into();
    assert!(hits(&c, &s).unwrap().is_empty());
}

#[test]
fn the_nurbs_circle_answers_as_the_analytic_circle() {
    // The same circle as a NURBS curve and as a conic, tilted, against each kind: the same
    // points with the same contacts.
    let o = [0.3, -0.2, 0.5];
    let (a, b) = ([0.6, 0.8, 0.0], [0.0, 0.0, 1.0]);
    let r = 1.7;
    let c = nurbs_circle(o, a, b, r);
    let f = frame(o, [0.8, -0.6, 0.0], a);
    let conic: Curve = Circle::new(f, len(r), Sense::Forward).unwrap().into();
    let turn = Extent1d::new(0.0, TAU).unwrap();
    let surfaces: Vec<Surface> = vec![
        Plane::new(frame([0.5, 0.1, 0.2], [0.3, 0.4, 0.866], [0.8, -0.6, 0.0])).into(),
        Cylinder::new(
            frame([0.1, 0.0, 0.7], [1.0, 0.2, 0.1], [0.0, 0.1, -0.2]),
            len(0.9),
            Sense::Forward,
        )
        .unwrap()
        .into(),
        Sphere::new(
            frame([0.9, 0.4, 0.0], [0.0, 0.0, 1.0], [1.0, 0.0, 0.0]),
            len(1.1),
            Sense::Reversed,
        )
        .unwrap()
        .into(),
        Cone::new(
            frame([0.0, 0.0, -0.4], [0.1, 1.0, 0.2], [1.0, -0.1, 0.0]),
            len(0.8),
            0.4,
            Sense::Forward,
        )
        .unwrap()
        .into(),
        Torus::new(
            frame([0.2, 0.3, 0.6], [0.2, -0.3, 1.0], [1.0, 0.0, -0.2]),
            len(1.2),
            len(0.4),
            Sense::Forward,
        )
        .unwrap()
        .into(),
    ];
    for s in &surfaces {
        let se = full(s);
        let want = intersect_curve(
            &conic,
            &turn,
            s,
            &se,
            zone(TOL),
            &mut Budget::unlimited(),
            &Stop::new(),
        )
        .unwrap();
        let got = hits(&c, s).unwrap();
        let want: Vec<[f64; 3]> = want
            .iter()
            .map(|h| match h {
                CurveSurfaceHit::Point { point, .. } => [point.x, point.y, point.z],
                CurveSurfaceHit::Overlap { .. } => panic!("{h:?}"),
            })
            .collect();
        assert!(!want.is_empty(), "{s:?}");
        points_are(&c, s, &got, &want, Contact::Crossing, 1e-9);
    }
}

#[test]
fn a_rational_quadratic_arc_crosses_a_cone() {
    // The cone of radius 1 at z = 0, slope k = tan(0.5); the arc a quarter of the circle
    // about (2, 0, 0) of radius 1.5 in the xz plane, from (3.5, 0, 0) to (2, 0, 1.5). On the
    // cone where 2 + 1.5 cos(th) = 1 + 1.5 k sin(th): cos(th + phi) = -1 / (1.5 hypot(1, k)).
    let (r0, rho, k) = (2.0, 1.5, fmath::tan(0.5));
    let h = FRAC_1_SQRT_2;
    let arc = nurbs(
        2,
        vec![0.0, 0.0, 0.0, 1.0, 1.0, 1.0],
        vec![[r0 + rho, 0.0, 0.0], [r0 + rho, 0.0, rho], [r0, 0.0, rho]],
        Some(vec![1.0, h, 1.0]),
    );
    let s: Surface = Cone::new(
        frame([0.0; 3], [0.0, 0.0, 1.0], [1.0, 0.0, 0.0]),
        len(1.0),
        0.5,
        Sense::Forward,
    )
    .unwrap()
    .into();
    // The quarter th in [0, pi/2] misses (x >= 2 > 1 + k z there); the half does not.
    assert!(hits(&arc, &s).unwrap().is_empty());
    let half = nurbs(
        2,
        vec![0.0, 0.0, 0.0, 1.0, 1.0, 2.0, 2.0, 2.0],
        vec![
            [r0 + rho, 0.0, 0.0],
            [r0 + rho, 0.0, rho],
            [r0, 0.0, rho],
            [r0 - rho, 0.0, rho],
            [r0 - rho, 0.0, 0.0],
        ],
        Some(vec![1.0, h, 1.0, h, 1.0]),
    );
    let phi = fmath::atan(k);
    let th = fmath::acos(-1.0 / (rho * fmath::hypot(k, 1.0))) - phi;
    assert!(th > 0.0 && th < PI);
    let want = [r0 + rho * fmath::cos(th), 0.0, rho * fmath::sin(th)];
    points_are(
        &half,
        &s,
        &hits(&half, &s).unwrap(),
        &[want],
        Contact::Crossing,
        1e-10,
    );
}

/// The cubic `x = rad + a s^2 + off`, `y = s + s^3 / 2` for `s` in `[-1, 1]`, in Bézier form.
fn grazing_cubic(rad: f64, a: f64, off: f64) -> Curve {
    let x = [1.0, -1.0 / 3.0, -1.0 / 3.0, 1.0].map(|c| rad + off + a * c);
    let y = [-1.5, 1.0 / 6.0, -1.0 / 6.0, 1.5];
    let points = (0..4).map(|i| [x[i], y[i], 0.25]).collect();
    nurbs(
        3,
        vec![0.0, 0.0, 0.0, 0.0, 1.0, 1.0, 1.0, 1.0],
        points,
        None,
    )
}

#[test]
fn a_cubic_grazing_a_cylinder_is_one_tangent() {
    let s: Surface = Cylinder::new(
        frame([0.0; 3], [0.0, 0.0, 1.0], [1.0, 0.0, 0.0]),
        len(2.0),
        Sense::Forward,
    )
    .unwrap()
    .into();
    let c = grazing_cubic(2.0, 0.7, 0.0);
    points_are(
        &c,
        &s,
        &hits(&c, &s).unwrap(),
        &[[2.0, 0.0, 0.25]],
        Contact::Tangent,
        1e-9,
    );
    // Within the tolerance either side: still the one tangent.
    for off in [-0.5 * TOL, 0.5 * TOL] {
        let c = grazing_cubic(2.0, 0.7, off);
        let got = hits(&c, &s).unwrap();
        assert!(
            matches!(
                got[..],
                [CurveSurfaceHit::Point {
                    contact: Contact::Tangent,
                    ..
                }]
            ),
            "{off}: {got:?}"
        );
    }
    // Past it outside: nothing; inside: two crossings at s = +-sqrt(-off / a) to first order.
    assert!(
        hits(&grazing_cubic(2.0, 0.7, 4.0 * TOL), &s)
            .unwrap()
            .is_empty()
    );
    let off = -100.0 * TOL;
    let got = hits(&grazing_cubic(2.0, 0.7, off), &s).unwrap();
    assert_eq!(got.len(), 2, "{got:?}");
    for h in &got {
        let CurveSurfaceHit::Point { point, contact, .. } = h else {
            panic!()
        };
        assert_eq!(*contact, Contact::Crossing);
        assert!(
            (fmath::hypot(point.x, point.y) - 2.0).abs() <= ROOT,
            "{h:?}"
        );
    }
}

#[test]
fn nurbs_lines_in_a_plane_overlap_it() {
    let s: Surface = Plane::new(frame([0.0, 0.0, 1.0], [0.0, 0.0, 1.0], [1.0, 0.0, 0.0])).into();
    // A quadratic in the plane: the whole of it, proved by the bound alone.
    let c = nurbs(
        2,
        vec![0.0, 0.0, 0.0, 1.0, 2.0, 2.0, 2.0],
        vec![
            [0.0, 0.0, 1.0],
            [1.0, 2.0, 1.0],
            [3.0, -1.0, 1.0],
            [4.0, 0.0, 1.0],
        ],
        None,
    );
    let got = hits(&c, &s).unwrap();
    assert!(
        matches!(got[..], [CurveSurfaceHit::Overlap { t }] if t == domain(&c)),
        "{got:?}"
    );
    let mut budget = Budget::unlimited();
    let on = curve_on_surface(
        &c,
        &domain(&c),
        &s,
        &full(&s),
        zone(TOL),
        &mut budget,
        &Stop::new(),
    )
    .unwrap();
    assert_eq!(on, OnSurface::Proved(vec![domain(&c)]));
    // A polyline down to the plane, along it from t = 1 to 2, and up: one overlap from knot
    // to knot.
    let c = nurbs(
        1,
        vec![0.0, 0.0, 1.0, 2.0, 3.0, 3.0],
        vec![
            [-1.0, 0.0, 2.0],
            [0.0, 0.0, 1.0],
            [2.0, 0.0, 1.0],
            [3.0, 0.0, 2.0],
        ],
        None,
    );
    let got = hits(&c, &s).unwrap();
    assert!(
        matches!(got[..], [CurveSurfaceHit::Overlap { t }] if t.min() == 1.0 && t.max() == 2.0),
        "{got:?}"
    );
    let on = curve_on_surface(
        &c,
        &domain(&c),
        &s,
        &full(&s),
        zone(TOL),
        &mut Budget::unlimited(),
        &Stop::new(),
    )
    .unwrap();
    assert_eq!(on, OnSurface::Leaves(got));
    // Clipped by the plane's extent at x = 1.5: from t = 1 to 1.75.
    let se = Extent2d::from_bounds(-10.0, -10.0, 1.5, 10.0).unwrap();
    let got = intersect_curve(
        &c,
        &domain(&c),
        &s,
        &se,
        zone(TOL),
        &mut Budget::unlimited(),
        &Stop::new(),
    )
    .unwrap();
    assert!(
        matches!(got[..], [CurveSurfaceHit::Overlap { t }] if t.min() == 1.0 && (t.max() - 1.75).abs() < 1e-12),
        "{got:?}"
    );
}

#[test]
fn a_polyline_meets_a_plane_at_its_knot_once() {
    let s: Surface = Plane::new(frame([0.0; 3], [0.0, 0.0, 1.0], [1.0, 0.0, 0.0])).into();
    let knots = vec![0.0, 0.0, 1.0, 2.0, 2.0];
    // Down through the plane at the knot: one crossing.
    let c = nurbs(
        1,
        knots.clone(),
        vec![[-1.0, 0.0, -1.0], [0.0, 0.0, 0.0], [1.0, 0.5, 1.0]],
        None,
    );
    let got = hits(&c, &s).unwrap();
    assert!(
        matches!(
            got[..],
            [CurveSurfaceHit::Point {
                t: 1.0,
                contact: Contact::Crossing,
                ..
            }]
        ),
        "{got:?}"
    );
    // A V touching it at the knot: one tangent (the turn no derivative's root shows).
    let c = nurbs(
        1,
        knots.clone(),
        vec![[-1.0, 0.0, 1.0], [0.0, 0.0, 0.0], [1.0, 0.5, 1.0]],
        None,
    );
    let got = hits(&c, &s).unwrap();
    assert!(
        matches!(
            got[..],
            [CurveSurfaceHit::Point {
                t: 1.0,
                contact: Contact::Tangent,
                ..
            }]
        ),
        "{got:?}"
    );
    // A cubic spline crossing at an interior knot, both spans with roots near it.
    let c = nurbs(
        3,
        vec![0.0, 0.0, 0.0, 0.0, 0.5, 1.0, 1.0, 1.0, 1.0],
        vec![
            [0.0, 0.0, -1.0],
            [1.0, 0.0, -0.5],
            [2.0, 0.0, 0.0],
            [3.0, 0.0, 0.5],
            [4.0, 0.0, 1.0],
        ],
        None,
    );
    let got = hits(&c, &s).unwrap();
    assert_eq!(got.len(), 1, "{got:?}");
    let CurveSurfaceHit::Point { t, point, .. } = got[0] else {
        panic!()
    };
    assert!(point.z.abs() <= ROOT && (t - 0.5).abs() < 1e-9, "{got:?}");
}

#[test]
fn a_nurbs_line_crosses_a_torus_four_times() {
    let s: Surface = Torus::new(
        frame([0.0; 3], [0.0, 0.0, 1.0], [1.0, 0.0, 0.0]),
        len(3.0),
        len(1.0),
        Sense::Forward,
    )
    .unwrap()
    .into();
    let c = nurbs(
        2,
        vec![0.0, 0.0, 0.0, 0.5, 1.0, 1.0, 1.0],
        vec![
            [-5.0, 0.0, 0.0],
            [-1.0, 0.0, 0.0],
            [1.0, 0.0, 0.0],
            [5.0, 0.0, 0.0],
        ],
        None,
    );
    let want = [
        [-4.0, 0.0, 0.0],
        [-2.0, 0.0, 0.0],
        [2.0, 0.0, 0.0],
        [4.0, 0.0, 0.0],
    ];
    points_are(
        &c,
        &s,
        &hits(&c, &s).unwrap(),
        &want,
        Contact::Crossing,
        1e-10,
    );
    // Along the top of the tube's ring at z = 1: one tangent at each crossing of the ring.
    let c = nurbs(
        1,
        vec![0.0, 0.0, 1.0, 1.0],
        vec![[-5.0, 0.0, 1.0], [5.0, 0.0, 1.0]],
        None,
    );
    points_are(
        &c,
        &s,
        &hits(&c, &s).unwrap(),
        &[[-3.0, 0.0, 1.0], [3.0, 0.0, 1.0]],
        Contact::Tangent,
        1e-6,
    );
}

#[test]
fn nurbs_budget_stop_and_surface_are_typed() {
    let c = nurbs_circle([0.0; 3], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0], 2.0);
    let s: Surface = Sphere::new(
        frame([1.0, 0.0, 0.0], [0.0, 0.0, 1.0], [1.0, 0.0, 0.0]),
        len(2.0),
        Sense::Forward,
    )
    .unwrap()
    .into();
    let se = full(&s);
    let run = |budget: &mut Budget, stop: &Stop| {
        intersect_curve(&c, &domain(&c), &s, &se, zone(TOL), budget, stop)
    };
    // Two units a span: four spans.
    let mut budget = Budget::new(8);
    run(&mut budget, &Stop::new()).unwrap();
    assert_eq!(budget.used(), 8);
    let e = run(&mut Budget::new(7), &Stop::new()).unwrap_err();
    assert!(matches!(e, SurfaceError::Exhausted(_)), "{e:?}");
    let stop = Stop::new();
    stop.set();
    assert_eq!(
        run(&mut Budget::unlimited(), &stop).unwrap_err(),
        SurfaceError::Stopped
    );
    // A NURBS curve on a NURBS surface: not built here.
    let nurbs_s = Surface::from(s.to_nurbs(&se).unwrap());
    let e = intersect_curve(
        &c,
        &domain(&c),
        &nurbs_s,
        &se,
        zone(TOL),
        &mut Budget::unlimited(),
        &Stop::new(),
    )
    .unwrap_err();
    assert_eq!(e, SurfaceError::NurbsIntersection);
}
