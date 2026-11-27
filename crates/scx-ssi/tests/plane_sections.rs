// Written for solidcorex. Apache-2.0.
//! The plane sections (research 63 SS5) against the exact sections (testkit T5,
//! `scx_testkit::ssi`): every answer's curves and points lie on both surfaces and inside
//! both extents, their kinds are the truth's, every truth point inside both extents lies on
//! an answer's piece (so an empty answer is checked, not trusted), a crossing curve runs
//! along `n_s x n_plane`, and `General` comes only where the truth is a hyperbola or a
//! parabola, or has no closed form.

#![allow(
    missing_docs,
    clippy::pedantic,
    clippy::indexing_slicing,
    clippy::unwrap_used,
    clippy::float_cmp,
    clippy::panic
)]

use core::f64::consts::{FRAC_PI_2, PI, TAU};

use scx_curve::Curve;
use scx_num::extent2d::Extent2d;
use scx_num::fmath;
use scx_num::frame::{Frame, Sense};
use scx_num::hash::Rng;
use scx_num::units::{Accuracy, Length, Resolution};
use scx_num::vector2d::Point2;
use scx_num::vector3d::{Point3, Vec3};
use scx_ssi::analytic::{self, Contact, Intersection};
use scx_surface::{Closed, Cone, Cylinder, Plane, Sphere, Surface, Torus};
use scx_testkit::analytic as ta;
use scx_testkit::curve_surface::{self as cs, SurfaceKind};
use scx_testkit::dd::Dd;
use scx_testkit::ssi::{self, Section};

/// The tolerance asked for: the pin's default (`EFF_ZERO_SQRT`).
const TOL: f64 = 1.490_116_119_384_765_6e-8;
/// How far an answer's point may lie from a surface (the closed forms' rounding).
const ON: f64 = 1e-9;
/// How far inside an extent a truth point must be to be owed (away from the clipping).
const MARGIN: f64 = 1e-6;

fn v(x: f64, y: f64, z: f64) -> Vec3 {
    Vec3::new(x, y, z)
}

fn p(x: f64, y: f64, z: f64) -> Point3 {
    Point3::new(x, y, z)
}

fn arr(q: Point3) -> [f64; 3] {
    [q.x, q.y, q.z]
}

fn varr(q: Vec3) -> [f64; 3] {
    [q.x, q.y, q.z]
}

fn frame(o: Point3, z: Vec3, x: Vec3) -> Frame {
    Frame::new(o, z, x).unwrap()
}

/// The testkit frame of a kind: z times the sense (research 63 D14).
fn tframe(f: &Frame, sense: Sense) -> ta::Frame {
    ta::Frame {
        origin: arr(f.origin()),
        x: varr(*f.x()),
        y: varr(*f.y()),
        z: varr(*f.z() * sense.sign()),
    }
}

/// The surface, its testkit form and its extent in STEP's `u` and `v`.
struct Side {
    surface: Surface,
    kind: SurfaceKind,
    extent: Extent2d,
    step: ([f64; 2], [f64; 2]),
}

fn plane_side(f: Frame, u: [f64; 2], w: [f64; 2]) -> Side {
    let kind = SurfaceKind::Plane(ta::Plane {
        frame: tframe(&f, Sense::Forward),
    });
    Side {
        surface: Surface::Plane(Plane::new(f)),
        kind,
        extent: Extent2d::from_bounds(u[0], w[0], u[1], w[1]).unwrap(),
        step: (u, w),
    }
}

fn extent(from: impl Fn(Point2) -> Point2, u: [f64; 2], w: [f64; 2]) -> Extent2d {
    Extent2d::spanning(from(Point2::new(u[0], w[0])), from(Point2::new(u[1], w[1]))).unwrap()
}

fn cylinder(f: Frame, r: f64, sense: Sense, u: [f64; 2], w: [f64; 2]) -> Side {
    let k = Cylinder::new(f, Length::from_mm(r), sense).unwrap();
    Side {
        kind: SurfaceKind::Cylinder(ta::Cylinder {
            frame: tframe(&f, sense),
            radius: r,
        }),
        extent: extent(|q| k.from_step(q), u, w),
        surface: Surface::Cylinder(k),
        step: (u, w),
    }
}

fn cone(f: Frame, r: f64, a: f64, sense: Sense, u: [f64; 2], w: [f64; 2]) -> Side {
    let k = Cone::new(f, Length::from_mm(r), a, sense).unwrap();
    Side {
        kind: SurfaceKind::Cone(ta::Cone {
            frame: tframe(&f, sense),
            radius: r,
            semi_angle: a,
        }),
        extent: extent(|q| k.from_step(q), u, w),
        surface: Surface::Cone(k),
        step: (u, w),
    }
}

fn sphere(f: Frame, r: f64, sense: Sense, u: [f64; 2], w: [f64; 2]) -> Side {
    let k = Sphere::new(f, Length::from_mm(r), sense).unwrap();
    Side {
        kind: SurfaceKind::Sphere(ta::Sphere {
            frame: tframe(&f, sense),
            radius: r,
        }),
        extent: extent(|q| k.from_step(q), u, w),
        surface: Surface::Sphere(k),
        step: (u, w),
    }
}

fn torus(f: Frame, big: f64, small: f64, sense: Sense, u: [f64; 2], w: [f64; 2]) -> Side {
    let k = Torus::new(f, Length::from_mm(big), Length::from_mm(small), sense).unwrap();
    Side {
        kind: SurfaceKind::Torus(ta::Torus {
            frame: tframe(&f, sense),
            major: big,
            minor: small,
        }),
        extent: extent(|q| k.from_step(q), u, w),
        surface: Surface::Torus(k),
        step: (u, w),
    }
}

const TURN: [f64; 2] = [0.0, TAU];

fn world() -> Frame {
    frame(p(0.0, 0.0, 0.0), v(0.0, 0.0, 1.0), v(1.0, 0.0, 0.0))
}

/// A kind's axes (z with the sense) and sizes.
fn axes_of(s: &Surface) -> Option<(Point3, Vec3, Vec3, Vec3)> {
    let (f, sense) = match s {
        Surface::Cylinder(k) => (k.frame(), k.sense()),
        Surface::Cone(k) => (k.frame(), k.sense()),
        Surface::Sphere(k) => (k.frame(), k.sense()),
        Surface::Torus(k) => (k.frame(), k.sense()),
        _ => return None,
    };
    Some((f.origin(), *f.x(), *f.y(), *f.z() * sense.sign()))
}

fn in_range(x: f64, [lo, hi]: [f64; 2], tol: f64) -> bool {
    x >= lo - tol && x <= hi + tol
}

fn in_angle(t: f64, [a0, a1]: [f64; 2], tol: f64) -> bool {
    if a1 - a0 >= TAU - 1e-12 {
        return true;
    }
    let t = a0 + (t - a0).rem_euclid(TAU);
    t <= a1 + tol || t >= a0 + TAU - tol
}

/// STEP's `(u, v)` of a point on the side's surface.
fn step_of(side: &Side, q: Point3) -> (f64, f64) {
    match &side.surface {
        Surface::Plane(k) => {
            let f = k.frame();
            let d = q - f.origin();
            (d.dot(*f.x()), d.dot(*f.y()))
        }
        s => {
            let (o, x, y, z) = axes_of(s).unwrap();
            let d = q - o;
            let (lx, ly, lz) = (d.dot(x), d.dot(y), d.dot(z));
            let rho = fmath::hypot(lx, ly);
            let u = fmath::atan2(ly, lx).rem_euclid(TAU);
            let w = match s {
                Surface::Sphere(_) => fmath::atan2(lz, rho),
                Surface::Torus(k) => fmath::atan2(lz, rho - k.major().mm()).rem_euclid(TAU),
                _ => lz,
            };
            (u, w)
        }
    }
}

/// Whether `q` (on the surface) is inside the side's extent within `tol` (negative: by
/// that much inside).
fn inside(side: &Side, q: Point3, tol: f64) -> bool {
    let (u, w) = step_of(side, q);
    let (ur, wr) = side.step;
    match &side.surface {
        Surface::Plane(_) => in_range(u, ur, tol) && in_range(w, wr, tol),
        s => {
            let (o, x, y, z) = axes_of(s).unwrap();
            let d = q - o;
            let rho = fmath::hypot(d.dot(x), d.dot(y));
            let ang = |r: f64| if r > 0.0 { tol / r } else { 0.0 };
            let around = if rho <= tol.abs() {
                tol >= 0.0
            } else {
                in_angle(u, ur, ang(rho))
            };
            around
                && match s {
                    Surface::Sphere(k) => in_range(w, wr, ang(k.radius().mm())),
                    Surface::Torus(k) => in_angle(w, wr, ang(k.minor().mm())),
                    Surface::Cone(k) => {
                        let kk = fmath::tan(k.half_angle());
                        in_range(w, wr, tol) && k.radius().mm() + kk * d.dot(z) >= -tol
                    }
                    _ => in_range(w, wr, tol),
                }
        }
    }
}

fn normal_at(side: &Side, q: Point3) -> Option<Vec3> {
    let (u, w) = step_of(side, q);
    let st = Point2::new(u, w);
    let uv = match &side.surface {
        Surface::Plane(k) => k.from_step(st),
        Surface::Cylinder(k) => k.from_step(st),
        Surface::Cone(k) => k.from_step(st),
        Surface::Sphere(k) => k.from_step(st),
        Surface::Torus(k) => k.from_step(st),
        _ => return None,
    };
    side.surface
        .normal(uv, [scx_nurbs::Approach::Above; 2])
        .map(Vec3::from)
}

fn tpoint(q: [Dd; 3]) -> Point3 {
    p(q[0].hi, q[1].hi, q[2].hi)
}

fn tvec(q: [Dd; 3]) -> Vec3 {
    v(q[0].hi, q[1].hi, q[2].hi)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Class {
    Point,
    Line,
    Circle,
    Ellipse,
    Other,
}

fn truth_class(c: &ssi::Curve) -> Class {
    match *c {
        ssi::Curve::Point(_) => Class::Point,
        ssi::Curve::Line { .. } => Class::Line,
        ssi::Curve::Circle { .. } => Class::Circle,
        ssi::Curve::Ellipse { a, b, .. } => {
            if (tvec(a).length() - tvec(b).length()).abs() <= 1e-9
                && tvec(a).dot(tvec(b)).abs() <= 1e-9
            {
                Class::Circle
            } else {
                Class::Ellipse
            }
        }
        ssi::Curve::Hyperbola { .. } => Class::Other,
    }
}

fn class(c: &Curve) -> Class {
    match c {
        Curve::Line(_) => Class::Line,
        Curve::Circle(_) => Class::Circle,
        Curve::Ellipse(_) => Class::Ellipse,
        _ => Class::Other,
    }
}

fn contact(c: ssi::Contact) -> Contact {
    match c {
        ssi::Contact::Transversal => Contact::Transversal,
        ssi::Contact::Tangent => Contact::Tangent,
    }
}

/// The distance from `q` to the curve over `[t0, t1]`: the nearest of a coarse sampling,
/// then a golden-section search beside it.
fn distance_to(c: &Curve, t0: f64, t1: f64, q: Point3) -> f64 {
    const N: usize = 256;
    let f = |t: f64| c.point(t).distance(q);
    let h = (t1 - t0) / N as f64;
    let mut best = (f(t0), t0);
    for i in 1..=N {
        let t = t0 + h * i as f64;
        let d = f(t);
        if d < best.0 {
            best = (d, t);
        }
    }
    // A whole turn wraps: search across its ends.
    let turn = t1 - t0 >= TAU - 1e-9;
    let (mut a, mut b) = if turn {
        (best.1 - h, best.1 + h)
    } else {
        ((best.1 - h).max(t0), (best.1 + h).min(t1))
    };
    let g = 0.5 * (5f64.sqrt() - 1.0);
    for _ in 0..80 {
        let (c1, c2) = (b - g * (b - a), a + g * (b - a));
        if f(c1) < f(c2) {
            b = c2;
        } else {
            a = c1;
        }
    }
    best.0.min(f(0.5 * (a + b)))
}

/// What the truth owes: its pieces' sample points inside both extents.
fn owed(s: &Side, pl: &Side, truth: &[ssi::Piece]) -> Vec<Point3> {
    let mut out = Vec::new();
    for piece in truth {
        let pts: Vec<Point3> = match piece.curve {
            ssi::Curve::Point(q) => vec![tpoint(q)],
            ssi::Curve::Line { origin, direction } => {
                let (o, d) = (tpoint(origin), tvec(direction));
                let t0 = -(o - pl_origin(pl)).dot(d) / d.dot(d);
                let span = 200.0 / d.length();
                (0..=1000)
                    .map(|i| o + d * (t0 - span + 2.0 * span * f64::from(i) / 1000.0))
                    .collect()
            }
            _ => (0..720)
                .map(|i| tpoint(piece.curve.point(Dd::new(TAU * f64::from(i) / 720.0))))
                .collect(),
        };
        out.extend(
            pts.into_iter()
                .filter(|&q| inside(s, q, -MARGIN) && inside(pl, q, -MARGIN)),
        );
    }
    out
}

fn n_of(pl: &Side) -> Vec3 {
    match &pl.surface {
        Surface::Plane(k) => *k.frame().z(),
        _ => unreachable!(),
    }
}

fn pl_origin(pl: &Side) -> Point3 {
    match &pl.surface {
        Surface::Plane(k) => k.frame().origin(),
        _ => unreachable!(),
    }
}

/// What an expected answer is, beyond the truth's checks.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Expect {
    /// Checked against the truth only.
    Truth,
    /// `General`.
    General,
    /// Torus meridians: the truth has no closed form; on both surfaces and inside only.
    Meridians(usize),
}

/// Checks the section of `s` by `pl` and returns it.
fn check(name: &str, s: &Side, pl: &Side, expect: Expect) -> Closed<Intersection> {
    let Surface::Plane(plane) = &pl.surface else {
        unreachable!()
    };
    let got = analytic::section_by_plane(
        &s.surface,
        &s.extent,
        plane,
        &pl.extent,
        Length::from_mm(TOL),
    );
    // Through `intersect` both ways, the same answer (the extents swap for coincident).
    let acc = Accuracy::new(Length::from_mm(TOL)).unwrap();
    let res = Resolution::new(Length::from_mm(TOL)).unwrap();
    let via = analytic::intersect(
        &s.surface,
        &s.extent,
        &pl.surface,
        &pl.extent,
        [acc, acc],
        res,
    );
    assert_eq!(via, got, "{name}: intersect differs from section_by_plane");
    let truth = ssi::intersect(&s.kind, &pl.kind);
    if expect == Expect::General {
        if let (Some(Intersection::Empty), Surface::Torus(k)) = (got.as_solved(), &s.surface) {
            // Past the bounding sphere, where the truth has no closed form.
            let d = n_of(pl).dot(pl_origin(pl) - k.frame().origin());
            assert!(
                d.abs() > k.major().mm() + k.minor().mm(),
                "{name}: torus empty inside the bound"
            );
            return got;
        }
        assert!(got.is_general(), "{name}: expected General, got {got:?}");
        match &truth {
            Err(ssi::Error::NoClosedForm | ssi::Error::Undecided) => {}
            Ok(Section::Curves(t)) => assert!(
                t.iter().any(|q| truth_class(&q.curve) == Class::Other),
                "{name}: General where the truth is closed: {t:?}"
            ),
            other => panic!("{name}: General against truth {other:?}"),
        }
        return got;
    }
    let Some(answer) = got.as_solved() else {
        panic!("{name}: General, truth {truth:?}");
    };
    let n_p = *plane.frame().z();
    let (curves, points) = match answer {
        Intersection::Coincident { .. } => {
            assert_eq!(truth, Ok(Section::Coincident), "{name}");
            return got;
        }
        Intersection::Empty => (&[][..], &[][..]),
        Intersection::Pieces { curves, points } => (&curves[..], &points[..]),
        _ => unreachable!(),
    };
    let tpieces: Vec<ssi::Piece> = match (&truth, expect) {
        (_, Expect::Meridians(n)) => {
            assert_eq!(curves.len(), n, "{name}: meridians {answer:?}");
            Vec::new()
        }
        (Ok(Section::Curves(t)), _) => t.clone(),
        (Ok(Section::Empty), _) => Vec::new(),
        (other, _) => panic!("{name}: truth {other:?}"),
    };
    let on = |q: Point3| {
        let (a, b) = (
            cs::distance(&s.kind, arr(q)).unwrap(),
            cs::distance(&pl.kind, arr(q)).unwrap(),
        );
        let scale = 1.0 + q.to_vec().length();
        assert!(
            a <= ON * scale && b <= ON * scale,
            "{name}: {q:?} off the surfaces by {a:e}, {b:e}"
        );
        assert!(
            inside(s, q, 1e-7) && inside(pl, q, 1e-7),
            "{name}: {q:?} outside an extent"
        );
    };
    for c in curves {
        let (t0, t1) = (c.span.min(), c.span.max());
        for i in 0..=32 {
            on(c.curve.point(t0 + (t1 - t0) * f64::from(i) / 32.0));
        }
        if expect == Expect::Truth {
            let k = class(&c.curve);
            assert!(
                tpieces
                    .iter()
                    .any(|q| truth_class(&q.curve) == k && contact(q.contact) == c.contact),
                "{name}: no truth piece of kind {k:?} contact {:?}: {tpieces:?}",
                c.contact
            );
        }
        if c.contact == Contact::Transversal {
            let tm = 0.5 * (t0 + t1);
            let q = c.curve.point(tm);
            let h = 1e-6 * (1.0 + (t1 - t0));
            let tangent = c.curve.point(tm + h) - c.curve.point(tm - h);
            if let Some(n_s) = normal_at(s, q) {
                let want = n_s.cross(n_p);
                if want.length() > 1e-6 {
                    assert!(tangent.dot(want) > 0.0, "{name}: runs against n_s x n_p");
                }
            }
        }
    }
    let tiny = |t: &ssi::Piece| match t.curve {
        ssi::Curve::Point(_) => true,
        ssi::Curve::Circle { a, b, .. } | ssi::Curve::Ellipse { a, b, .. } => {
            tvec(a).length().max(tvec(b).length()) <= TOL
        }
        _ => false,
    };
    for q in points {
        on(q.point);
        if expect == Expect::Truth {
            assert!(
                tpieces
                    .iter()
                    .any(|t| contact(t.contact) == q.contact || tiny(t)),
                "{name}: point {q:?} against {tpieces:?}"
            );
        }
    }
    for q in owed(s, pl, &tpieces) {
        let near_curve = curves
            .iter()
            .map(|c| distance_to(&c.curve, c.span.min(), c.span.max(), q))
            .fold(f64::INFINITY, f64::min);
        let near_point = points
            .iter()
            .map(|x| x.point.distance(q))
            .fold(f64::INFINITY, f64::min);
        assert!(
            near_curve.min(near_point) <= 1e-7 * (1.0 + q.to_vec().length()),
            "{name}: truth point {q:?} missed by {}; got {got:?}; truth {tpieces:?}; extent {:?} step {:?}",
            near_curve.min(near_point),
            s.extent,
            s.step
        );
    }
    got
}

fn counts(got: &Closed<Intersection>) -> (usize, usize) {
    match got.as_solved() {
        Some(Intersection::Pieces { curves, points }) => (curves.len(), points.len()),
        _ => (0, 0),
    }
}

fn kinds(got: &Closed<Intersection>) -> Vec<(Class, Contact)> {
    match got.as_solved() {
        Some(Intersection::Pieces { curves, .. }) => curves
            .iter()
            .map(|c| (class(&c.curve), c.contact))
            .collect(),
        _ => Vec::new(),
    }
}

fn big_plane(o: Point3, n: Vec3, x: Vec3) -> Side {
    plane_side(frame(o, n, x), [-50.0, 50.0], [-50.0, 50.0])
}

// ---------------------------------------------------------------- planes

#[test]
fn plane_plane() {
    let a = plane_side(world(), [-2.0, 3.0], [-1.0, 4.0]);
    let tilted = big_plane(p(0.5, 0.0, 0.0), v(1.0, 0.0, 0.3), v(0.0, 1.0, 0.0));
    let got = check("oblique", &a, &tilted, Expect::Truth);
    assert_eq!(kinds(&got), vec![(Class::Line, Contact::Transversal)]);
    let above = big_plane(p(0.0, 0.0, 1.0), v(0.0, 0.0, 1.0), v(1.0, 0.0, 0.0));
    assert_eq!(
        check("parallel", &a, &above, Expect::Truth).solved(),
        Some(Intersection::Empty)
    );
    let same = plane_side(
        frame(p(1.0, 1.0, 0.0), v(0.0, 0.0, -1.0), v(0.0, 1.0, 0.0)),
        [-1.0, 1.0],
        [-1.0, 1.0],
    );
    assert!(matches!(
        check("coincident", &a, &same, Expect::Truth).solved(),
        Some(Intersection::Coincident { .. })
    ));
    let far = plane_side(
        frame(p(40.0, 0.0, 0.0), v(0.0, 0.0, 1.0), v(1.0, 0.0, 0.0)),
        [-1.0, 1.0],
        [-1.0, 1.0],
    );
    // Coincident planes whose extents miss: the truth (complete planes) says coincident;
    // the extents make it empty.
    let got = analytic::section_by_plane(
        &far.surface,
        &far.extent,
        match &a.surface {
            Surface::Plane(k) => k,
            _ => unreachable!(),
        },
        &a.extent,
        Length::from_mm(TOL),
    );
    assert_eq!(got.solved(), Some(Intersection::Empty));
    // A plane crossing the plane outside the extent: empty, checked.
    let off = big_plane(p(10.0, 0.0, 0.0), v(1.0, 0.0, 0.0), v(0.0, 1.0, 0.0));
    assert_eq!(
        check("off", &a, &off, Expect::Truth).solved(),
        Some(Intersection::Empty)
    );
}

// ---------------------------------------------------------------- cylinders

#[test]
fn cylinder_sections() {
    for sense in [Sense::Forward, Sense::Reversed] {
        let c = cylinder(world(), 2.0, sense, TURN, [-3.0, 3.0]);
        let across = big_plane(p(0.0, 0.0, 1.0), v(0.0, 0.0, 1.0), v(1.0, 0.0, 0.0));
        assert_eq!(
            kinds(&check("circle", &c, &across, Expect::Truth)),
            vec![(Class::Circle, Contact::Transversal)]
        );
        let oblique = big_plane(p(0.0, 0.0, 0.5), v(0.3, 0.2, 1.0), v(1.0, 0.0, -0.3));
        assert_eq!(
            kinds(&check("ellipse", &c, &oblique, Expect::Truth)),
            vec![(Class::Ellipse, Contact::Transversal)]
        );
        let steep = big_plane(p(0.0, 0.0, 0.0), v(1.0, 0.0, 0.6), v(0.0, 1.0, 0.0));
        let got = check("clipped ellipse", &c, &steep, Expect::Truth);
        assert_eq!(counts(&got), (2, 0));
        let along = big_plane(p(1.0, 0.0, 0.0), v(1.0, 0.0, 0.0), v(0.0, 1.0, 0.0));
        assert_eq!(
            kinds(&check("two lines", &c, &along, Expect::Truth)),
            vec![(Class::Line, Contact::Transversal); 2]
        );
        let touching = big_plane(p(2.0, 0.0, 0.0), v(1.0, 0.0, 0.0), v(0.0, 1.0, 0.0));
        assert_eq!(
            kinds(&check("tangent line", &c, &touching, Expect::Truth)),
            vec![(Class::Line, Contact::Tangent)]
        );
        let apart = big_plane(p(2.5, 0.0, 0.0), v(1.0, 0.0, 0.0), v(0.0, 1.0, 0.0));
        assert_eq!(
            check("apart", &c, &apart, Expect::Truth).solved(),
            Some(Intersection::Empty)
        );
        let above = big_plane(p(0.0, 0.0, 4.0), v(0.0, 0.0, 1.0), v(1.0, 0.0, 0.0));
        assert_eq!(
            check("above", &c, &above, Expect::Truth).solved(),
            Some(Intersection::Empty)
        );
    }
    // Half a cylinder: the circle's arc, the lines on one side only.
    let half = cylinder(world(), 2.0, Sense::Forward, [0.0, PI], [-3.0, 3.0]);
    let across = big_plane(p(0.0, 0.0, 1.0), v(0.0, 0.0, 1.0), v(1.0, 0.0, 0.0));
    assert_eq!(
        counts(&check("half circle", &half, &across, Expect::Truth)),
        (1, 0)
    );
    let along = big_plane(p(0.0, 1.0, 0.0), v(0.0, 1.0, 0.0), v(1.0, 0.0, 0.0));
    assert_eq!(
        counts(&check("half lines", &half, &along, Expect::Truth)),
        (2, 0)
    );
    let below = big_plane(p(0.0, -1.0, 0.0), v(0.0, 1.0, 0.0), v(1.0, 0.0, 0.0));
    assert_eq!(
        check("half empty", &half, &below, Expect::Truth).solved(),
        Some(Intersection::Empty)
    );
    // A small plane extent clips the circle to an arc.
    let small = plane_side(
        frame(p(2.0, 0.0, 1.0), v(0.0, 0.0, 1.0), v(1.0, 0.0, 0.0)),
        [-0.5, 0.5],
        [-0.5, 0.5],
    );
    let full = cylinder(world(), 2.0, Sense::Forward, TURN, [-3.0, 3.0]);
    assert_eq!(counts(&check("arc", &full, &small, Expect::Truth)), (1, 0));
}

// ---------------------------------------------------------------- cones

#[test]
fn cone_sections() {
    for sense in [Sense::Forward, Sense::Reversed] {
        let c = cone(world(), 1.0, 0.4, sense, TURN, [-1.5, 3.0]);
        let across = big_plane(p(0.0, 0.0, 1.0), v(0.0, 0.0, 1.0), v(1.0, 0.0, 0.0));
        assert_eq!(
            kinds(&check("circle", &c, &across, Expect::Truth)),
            vec![(Class::Circle, Contact::Transversal)]
        );
        let oblique = big_plane(p(0.0, 0.0, 1.0), v(0.2, 0.1, 1.0), v(1.0, 0.0, -0.2));
        assert_eq!(
            kinds(&check("ellipse", &c, &oblique, Expect::Truth)),
            vec![(Class::Ellipse, Contact::Transversal)]
        );
        // The apex's height along the axis, and in the world (the axis is z times the sense).
        let apex_z = -1.0 / fmath::tan(0.4);
        let apex_w = apex_z * sense.sign();
        let low = cone(world(), 1.0, 0.4, sense, TURN, [apex_z, 3.0]);
        let shallow = big_plane(p(0.0, 0.0, apex_w), v(0.1, 0.0, 1.0), v(1.0, 0.0, -0.1));
        let got = check("apex", &low, &shallow, Expect::Truth);
        assert_eq!(counts(&got), (0, 1));
        let steep = big_plane(p(0.0, 0.0, apex_w), v(1.0, 0.0, 0.0), v(0.0, 1.0, 0.0));
        assert_eq!(
            kinds(&check("generators", &low, &steep, Expect::Truth)),
            vec![(Class::Line, Contact::Transversal); 2]
        );
        let hyperbola = big_plane(p(0.3, 0.0, 0.0), v(1.0, 0.0, 0.0), v(0.0, 1.0, 0.0));
        check("hyperbola", &c, &hyperbola, Expect::General);
        // Square to the axis past the apex: only the other nappe, outside the domain.
        let beyond = big_plane(
            p(0.0, 0.0, (apex_z - 1.0) * sense.sign()),
            v(0.0, 0.0, 1.0),
            v(1.0, 0.0, 0.0),
        );
        let wide = cone(world(), 1.0, 0.4, sense, TURN, [apex_z - 2.0, 3.0]);
        assert_eq!(analytic_empty(&wide, &beyond), Some(Intersection::Empty));
    }
}

/// The answer without the truth's completeness (the truth's cone has both nappes; the
/// kind has one).
fn analytic_empty(s: &Side, pl: &Side) -> Option<Intersection> {
    let Surface::Plane(plane) = &pl.surface else {
        unreachable!()
    };
    analytic::section_by_plane(
        &s.surface,
        &s.extent,
        plane,
        &pl.extent,
        Length::from_mm(TOL),
    )
    .solved()
}

// ---------------------------------------------------------------- spheres

#[test]
fn sphere_sections() {
    for sense in [Sense::Forward, Sense::Reversed] {
        // Axis-aligned: a rounded frame is exactly a slight ellipsoid to the truth.
        let s = sphere(
            frame(p(1.0, -2.0, 0.5), v(0.0, 0.0, 1.0), v(1.0, 0.0, 0.0)),
            3.0,
            sense,
            TURN,
            [-FRAC_PI_2, FRAC_PI_2],
        );
        let cut = big_plane(p(1.0, -2.0, 1.5), v(0.2, 0.0, 1.0), v(1.0, 0.0, -0.2));
        assert_eq!(
            kinds(&check("circle", &s, &cut, Expect::Truth)),
            vec![(Class::Circle, Contact::Transversal)]
        );
        let top = big_plane(p(1.0, -2.0, 3.5), v(0.0, 0.0, 1.0), v(1.0, 0.0, 0.0));
        assert_eq!(counts(&check("touch", &s, &top, Expect::Truth)), (0, 1));
        let miss = big_plane(p(1.0, -2.0, 3.6), v(0.0, 0.0, 1.0), v(1.0, 0.0, 0.0));
        assert_eq!(
            check("miss", &s, &miss, Expect::Truth).solved(),
            Some(Intersection::Empty)
        );
    }
    // A cap: latitudes clip a meridian plane's circle.
    let cap = sphere(world(), 2.0, Sense::Forward, TURN, [0.5, FRAC_PI_2]);
    let meridian = big_plane(p(0.0, 0.0, 0.0), v(1.0, 0.0, 0.0), v(0.0, 1.0, 0.0));
    assert_eq!(
        counts(&check("cap meridian", &cap, &meridian, Expect::Truth)),
        (1, 0)
    );
    let band = sphere(world(), 2.0, Sense::Forward, [0.5, 4.0], [-0.5, 0.7]);
    let oblique = big_plane(p(0.0, 0.0, 0.3), v(0.4, -0.3, 1.0), v(1.0, 0.0, -0.4));
    check("band", &band, &oblique, Expect::Truth);
}

// ---------------------------------------------------------------- tori

#[test]
fn torus_sections() {
    for sense in [Sense::Forward, Sense::Reversed] {
        let t = torus(world(), 3.0, 1.0, sense, TURN, TURN);
        let across = big_plane(p(0.0, 0.0, 0.5), v(0.0, 0.0, 1.0), v(1.0, 0.0, 0.0));
        assert_eq!(
            kinds(&check("two circles", &t, &across, Expect::Truth)),
            vec![(Class::Circle, Contact::Transversal); 2]
        );
        let top = big_plane(p(0.0, 0.0, 1.0), v(0.0, 0.0, 1.0), v(1.0, 0.0, 0.0));
        assert_eq!(
            kinds(&check("top circle", &t, &top, Expect::Truth)),
            vec![(Class::Circle, Contact::Tangent)]
        );
        let miss = big_plane(p(0.0, 0.0, 1.2), v(0.0, 0.0, 1.0), v(1.0, 0.0, 0.0));
        assert_eq!(
            check("miss", &t, &miss, Expect::Truth).solved(),
            Some(Intersection::Empty)
        );
        let meridian = big_plane(p(0.0, 0.0, 0.0), v(1.0, 1.0, 0.0), v(0.0, 0.0, 1.0));
        check("meridians", &t, &meridian, Expect::Meridians(2));
        let oblique = big_plane(p(0.0, 0.0, 0.2), v(0.3, 0.0, 1.0), v(1.0, 0.0, -0.3));
        check("spiric", &t, &oblique, Expect::General);
    }
    // The outer half of the tube: each meridian loses its inner half; across the axis the
    // inner circle goes.
    let outer = torus(
        world(),
        3.0,
        1.0,
        Sense::Forward,
        TURN,
        [-FRAC_PI_2, FRAC_PI_2],
    );
    let meridian = big_plane(p(0.0, 0.0, 0.0), v(0.0, 1.0, 0.0), v(1.0, 0.0, 0.0));
    let got = check("outer meridians", &outer, &meridian, Expect::Meridians(2));
    if let Some(Intersection::Pieces { curves, .. }) = got.as_solved() {
        for c in curves {
            assert!(
                (c.span.max() - c.span.min() - PI).abs() < 1e-9,
                "{:?}",
                c.span
            );
        }
    }
    let across = big_plane(p(0.0, 0.0, 0.5), v(0.0, 0.0, 1.0), v(1.0, 0.0, 0.0));
    assert_eq!(
        counts(&check("outer circle", &outer, &across, Expect::Truth)),
        (1, 0)
    );
}

// ---------------------------------------------------------------- random

fn unit(rng: &mut Rng) -> Vec3 {
    loop {
        let q = v(
            rng.next_signed_unit(),
            rng.next_signed_unit(),
            rng.next_signed_unit(),
        );
        if q.length() > 0.2 && q.length() <= 1.0 {
            return *q.normalized().unwrap();
        }
    }
}

fn random_frame(rng: &mut Rng, spread: f64) -> Frame {
    let o = p(
        spread * rng.next_signed_unit(),
        spread * rng.next_signed_unit(),
        spread * rng.next_signed_unit(),
    );
    loop {
        let (z, x) = (unit(rng), unit(rng));
        if z.cross(x).length() > 0.3 {
            return frame(o, z, x);
        }
    }
}

fn random_plane_near(rng: &mut Rng, at: Point3, reach: f64) -> Side {
    let q = at + unit(rng) * (reach * rng.next_f64());
    let n = unit(rng);
    let mut x = unit(rng);
    while n.cross(x).length() < 0.3 {
        x = unit(rng);
    }
    plane_side(
        frame(q, n, x),
        [-30.0 * rng.next_f64() - 1.0, 30.0 * rng.next_f64() + 1.0],
        [-30.0 * rng.next_f64() - 1.0, 30.0 * rng.next_f64() + 1.0],
    )
}

fn random_angles(rng: &mut Rng) -> [f64; 2] {
    if rng.next_f64() < 0.5 {
        TURN
    } else {
        let a = TAU * rng.next_f64();
        [a, a + 0.3 + 5.5 * rng.next_f64()]
    }
}

fn sense_of(rng: &mut Rng) -> Sense {
    if rng.next_f64() < 0.5 {
        Sense::Forward
    } else {
        Sense::Reversed
    }
}

/// Random kinds and planes: each answer checked against the truth; `General` only where
/// the truth is a hyperbola or parabola, or has no closed form.
#[test]
fn random_sections() {
    let mut rng = Rng::new(0x5ec7_1011);
    let mut tally = [0usize; 3];
    for i in 0..400 {
        let f = random_frame(&mut rng, 5.0);
        let sense = sense_of(&mut rng);
        let u = random_angles(&mut rng);
        let side = match i % 5 {
            0 => plane_side(f, [-3.0, 4.0], [-2.0, 2.5]),
            1 => cylinder(
                f,
                0.5 + 3.0 * rng.next_f64(),
                sense,
                u,
                [-4.0, 2.0 + 3.0 * rng.next_f64()],
            ),
            2 => {
                let a = 0.1 + 1.2 * rng.next_f64();
                let r = 0.2 + 2.0 * rng.next_f64();
                let apex = -r / fmath::tan(a);
                cone(
                    f,
                    r,
                    a,
                    sense,
                    u,
                    [apex.max(-3.0), 1.0 + 3.0 * rng.next_f64()],
                )
            }
            3 => {
                let a = -FRAC_PI_2 + 1.5 * rng.next_f64();
                sphere(
                    f,
                    0.5 + 3.0 * rng.next_f64(),
                    sense,
                    u,
                    [a, (a + 0.5 + 2.0 * rng.next_f64()).min(FRAC_PI_2)],
                )
            }
            _ => {
                let small = 0.3 + 1.0 * rng.next_f64();
                torus(f, small + 0.5 + 2.0 * rng.next_f64(), small, sense, u, TURN)
            }
        };
        let pl = random_plane_near(&mut rng, f.origin(), 3.0);
        let truth = ssi::intersect(&side.kind, &pl.kind);
        let expect = match (&truth, &side.surface) {
            (Err(_), _) => Expect::General,
            (Ok(Section::Curves(t)), _)
                if t.iter().any(|q| truth_class(&q.curve) == Class::Other) =>
            {
                Expect::General
            }
            // The truth's cone has both nappes; a parabola is its own error-free case.
            _ => Expect::Truth,
        };
        let name = format!("random {i}");
        if matches!(side.surface, Surface::Cone(_)) && expect == Expect::Truth {
            // Owed points on the other nappe are outside the kind's domain ("inside" checks
            // `r + k v >= 0`), so the full check applies.
        }
        let got = check(&name, &side, &pl, expect);
        tally[match got.as_solved() {
            None => 0,
            Some(Intersection::Empty) => 1,
            _ => 2,
        }] += 1;
    }
    // The mix exercises every outcome.
    assert!(tally.iter().all(|&n| n > 10), "{tally:?}");
}
