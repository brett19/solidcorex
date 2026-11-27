// Written for solidcorex. Apache-2.0.
//! What the closed-form tests share: the surfaces with their testkit forms and extents,
//! where a point sits in an extent, and the truth's classes.

#![allow(
    dead_code,
    unused_imports,
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

/// The tolerance asked for: `2^-26`, tighter than the pin's default (`EFF_ZERO_SQRT`, 1e-6).
pub const TOL: f64 = 1.490_116_119_384_765_6e-8;
/// How far an answer's point may lie from a surface (the closed forms' rounding).
pub const ON: f64 = 1e-9;
/// How far inside an extent a truth point must be to be owed (away from the clipping).
pub const MARGIN: f64 = 1e-6;

pub fn v(x: f64, y: f64, z: f64) -> Vec3 {
    Vec3::new(x, y, z)
}

pub fn p(x: f64, y: f64, z: f64) -> Point3 {
    Point3::new(x, y, z)
}

pub fn arr(q: Point3) -> [f64; 3] {
    [q.x, q.y, q.z]
}

pub fn varr(q: Vec3) -> [f64; 3] {
    [q.x, q.y, q.z]
}

pub fn frame(o: Point3, z: Vec3, x: Vec3) -> Frame {
    Frame::new(o, z, x).unwrap()
}

/// The testkit frame of a kind: z times the sense (research 63 D14).
pub fn tframe(f: &Frame, sense: Sense) -> ta::Frame {
    ta::Frame {
        origin: arr(f.origin()),
        x: varr(*f.x()),
        y: varr(*f.y()),
        z: varr(*f.z() * sense.sign()),
    }
}

/// The surface, its testkit form and its extent in STEP's `u` and `v`.
pub struct Side {
    pub surface: Surface,
    pub kind: SurfaceKind,
    pub extent: Extent2d,
    pub step: ([f64; 2], [f64; 2]),
}

pub fn plane_side(f: Frame, u: [f64; 2], w: [f64; 2]) -> Side {
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

pub fn extent(from: impl Fn(Point2) -> Point2, u: [f64; 2], w: [f64; 2]) -> Extent2d {
    Extent2d::spanning(from(Point2::new(u[0], w[0])), from(Point2::new(u[1], w[1]))).unwrap()
}

pub fn cylinder(f: Frame, r: f64, sense: Sense, u: [f64; 2], w: [f64; 2]) -> Side {
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

pub fn cone(f: Frame, r: f64, a: f64, sense: Sense, u: [f64; 2], w: [f64; 2]) -> Side {
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

pub fn sphere(f: Frame, r: f64, sense: Sense, u: [f64; 2], w: [f64; 2]) -> Side {
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

pub fn torus(f: Frame, big: f64, small: f64, sense: Sense, u: [f64; 2], w: [f64; 2]) -> Side {
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

pub const TURN: [f64; 2] = [0.0, TAU];

pub fn world() -> Frame {
    frame(p(0.0, 0.0, 0.0), v(0.0, 0.0, 1.0), v(1.0, 0.0, 0.0))
}

/// A kind's axes (z with the sense) and sizes.
pub fn axes_of(s: &Surface) -> Option<(Point3, Vec3, Vec3, Vec3)> {
    let (f, sense) = match s {
        Surface::Cylinder(k) => (k.frame(), k.sense()),
        Surface::Cone(k) => (k.frame(), k.sense()),
        Surface::Sphere(k) => (k.frame(), k.sense()),
        Surface::Torus(k) => (k.frame(), k.sense()),
        _ => return None,
    };
    Some((f.origin(), *f.x(), *f.y(), *f.z() * sense.sign()))
}

pub fn in_range(x: f64, [lo, hi]: [f64; 2], tol: f64) -> bool {
    x >= lo - tol && x <= hi + tol
}

pub fn in_angle(t: f64, [a0, a1]: [f64; 2], tol: f64) -> bool {
    if a1 - a0 >= TAU - 1e-12 {
        return true;
    }
    let t = a0 + (t - a0).rem_euclid(TAU);
    t <= a1 + tol || t >= a0 + TAU - tol
}

/// STEP's `(u, v)` of a point on the side's surface.
pub fn step_of(side: &Side, q: Point3) -> (f64, f64) {
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
pub fn inside(side: &Side, q: Point3, tol: f64) -> bool {
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

pub fn normal_at(side: &Side, q: Point3) -> Option<Vec3> {
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

pub fn tpoint(q: [Dd; 3]) -> Point3 {
    p(q[0].hi, q[1].hi, q[2].hi)
}

pub fn tvec(q: [Dd; 3]) -> Vec3 {
    v(q[0].hi, q[1].hi, q[2].hi)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Class {
    Point,
    Line,
    Circle,
    Ellipse,
    Parabola,
    Hyperbola,
    Other,
}

pub fn truth_class(c: &ssi::Curve) -> Class {
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
        ssi::Curve::Hyperbola { .. } => Class::Hyperbola,
    }
}

pub fn class(c: &Curve) -> Class {
    match c {
        Curve::Line(_) => Class::Line,
        Curve::Circle(_) => Class::Circle,
        Curve::Ellipse(_) => Class::Ellipse,
        Curve::Parabola(_) => Class::Parabola,
        Curve::Hyperbola(_) => Class::Hyperbola,
        _ => Class::Other,
    }
}

pub fn contact(c: ssi::Contact) -> Contact {
    match c {
        ssi::Contact::Transversal => Contact::Transversal,
        ssi::Contact::Tangent => Contact::Tangent,
    }
}

/// The distance from `q` to the curve over `[t0, t1]`: the nearest of a coarse sampling,
/// then a golden-section search beside it.
pub fn distance_to(c: &Curve, t0: f64, t1: f64, q: Point3) -> f64 {
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
