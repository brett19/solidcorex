// Written for solidcorex. Apache-2.0.
//! `scx-surface`'s analytic kinds against the pin's, function by function: `point`,
//! `normal`, `jet` (to order 2, both sides of a break), `to_step` and `from_step`.
//!
//! The pin's side is the canonical surface with its NURB made, read through
//! `surface::surface::p2::Surface`: `evaluate` (with `non_zero_tangents` off, so a vanishing
//! tangent is reported as it is), `evaluate_normal`, `convert_uv_from_nurbs_to_step` and
//! `convert_uv_from_step_to_nurbs` (angles in degrees there, radians here):
//! - plane: `Plane::new` over `[-10, 10]^2` (its NURB parameter is STEP's);
//! - cylinder and cone: `Cone::new_cylinder_bounded` and `Cone::new_bounded` over a full
//!   turn and heights `[0, h]`, whose NURB parameter is the canonical one (`u` four arcs,
//!   `v` the height). The unbounded canonical forms store their height over
//!   `[-1234567, 1234567]` and lose about `1e-10` there (a pin precision limit, not a
//!   layout difference), so they are not the reference;
//! - sphere and torus: `create_canonical`.
//!
//! A [`Sense::Reversed`] kind is the mirror image in its frame's xy plane of the forward
//! one at the same uv (D14): the pin's side mirrors the forward surface's point and
//! derivatives, and reverses its mirrored normal (a mirror turns `S_u x S_v`).
//!
//! Floats agree to `1e-12` relative or `1e-11` absolute: the inputs are of size 1 to 30,
//! and a point's coordinate near zero carries the rounding of the others.

use crate::harness::{Gen, SplitMix64, Tol, run_defect_tol, run_tol};

use scx_num::frame::{Frame, Sense};
use scx_num::hash::Rng;
use scx_num::units::Length;
use scx_num::vector2d::Point2;
use scx_num::vector3d::{Point3, Vec3};
use scx_nurbs::Approach;
use scx_surface::{Cone, Cylinder, Plane, Sphere, Torus};

pub const TOL: Tol = Tol {
    rel: 1e-12,
    abs: 1e-11,
};

/// The plane's sampled square, and the bounded cylinders' and cones' height.
const PLANE: f64 = 10.0;
const HEIGHT: f64 = 6.0;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Kind {
    Plane,
    Cylinder {
        r: f64,
    },
    /// Radius at `v = 0`, half-angle in radians, height of the pin's bounded cone.
    Cone {
        r: f64,
        a: f64,
        h: f64,
    },
    Sphere {
        r: f64,
    },
    Torus {
        big: f64,
        small: f64,
    },
}

/// One input: a kind in a frame (origin, z, x), a sense, a parameter and the side at a
/// break in each direction (`true`: from above).
#[derive(Clone, Debug)]
pub struct Spec {
    pub kind: Kind,
    pub frame: [[f64; 3]; 3],
    pub reversed: bool,
    pub uv: (f64, f64),
    pub above: (bool, bool),
}

impl Spec {
    fn frame(&self) -> Frame {
        let [o, z, x] = self.frame;
        Frame::new(
            Point3::new(o[0], o[1], o[2]),
            Vec3::new(z[0], z[1], z[2]),
            Vec3::new(x[0], x[1], x[2]),
        )
        .unwrap()
    }

    fn sense(&self) -> Sense {
        if self.reversed {
            Sense::Reversed
        } else {
            Sense::Forward
        }
    }

    fn uv(&self) -> Point2 {
        Point2::new(self.uv.0, self.uv.1)
    }

    /// STEP's parameter at the same place in the domain, as the pin takes it: angles in
    /// degrees.
    fn step_degrees(&self) -> (f64, f64) {
        let (fu, fv) = self.fractions();
        match self.kind {
            Kind::Plane => self.uv,
            Kind::Cylinder { .. } | Kind::Cone { .. } => (360.0 * fu, self.uv.1),
            Kind::Sphere { .. } => (360.0 * fu, 180.0 * fv - 90.0),
            Kind::Torus { .. } => (360.0 * fu, 360.0 * fv),
        }
    }

    /// The same jet across the seam, when `uv` is on a periodic direction's seam and its
    /// side looks off the domain there (from below at the start, from above at the end):
    /// the spec at the seam's other end, whose side looks onto the domain.
    pub fn across_seam(&self) -> Option<Spec> {
        let period_u = match self.kind {
            Kind::Plane => return None,
            Kind::Cylinder { r } => 2.0 * core::f64::consts::PI * r,
            Kind::Cone { .. } | Kind::Sphere { .. } | Kind::Torus { .. } => 1.0,
        };
        let period_v = matches!(self.kind, Kind::Torus { .. }).then_some(1.0);
        let across = |t: f64, period: f64, above: bool| match (t, above) {
            (0.0, false) => Some(period),
            (t, true) if t == period => Some(0.0),
            _ => None,
        };
        let u = across(self.uv.0, period_u, self.above.0);
        let v = period_v.and_then(|p| across(self.uv.1, p, self.above.1));
        if u.is_none() && v.is_none() {
            return None;
        }
        let mut t = self.clone();
        t.uv = (u.unwrap_or(self.uv.0), v.unwrap_or(self.uv.1));
        Some(t)
    }

    /// Where `uv` is in the sampled ranges, `[0, 1]` each.
    fn fractions(&self) -> (f64, f64) {
        let (u, v) = self.uv;
        match self.kind {
            Kind::Plane => ((u + PLANE) / (2.0 * PLANE), (v + PLANE) / (2.0 * PLANE)),
            Kind::Cylinder { r } => (u / (2.0 * core::f64::consts::PI * r), v / HEIGHT),
            Kind::Cone { h, .. } => (u, v / h),
            Kind::Sphere { .. } | Kind::Torus { .. } => (u, v),
        }
    }
}

/// A fraction of `[0, 1]`, on one of `n` breaks a time in three.
fn frac(rng: &mut SplitMix64, n: usize) -> f64 {
    if rng.below(3) == 0 {
        rng.below(n + 1) as f64 / n as f64
    } else {
        rng.uniform(0.0, 1.0)
    }
}

impl Gen for Spec {
    fn pool(rng: &mut SplitMix64) -> Vec<Self> {
        let mut out = Vec::new();
        let mut frames = Rng::new(rng.next_u64());
        for i in 0..250 {
            let f = scx_testkit::sample::frame(&mut frames, 10.0);
            let kind = match i % 5 {
                0 => Kind::Plane,
                1 => Kind::Cylinder {
                    r: rng.uniform(0.5, 5.0),
                },
                2 => {
                    let r = rng.uniform(0.5, 5.0);
                    let a = rng.uniform(0.1, 1.3) * if rng.below(2) == 0 { 1.0 } else { -1.0 };
                    // A height whose top circle has a positive radius.
                    let k = scx_num::fmath::tan(a);
                    let h = if r + k * HEIGHT > 0.2 {
                        HEIGHT
                    } else {
                        0.8 * r / -k
                    };
                    Kind::Cone { r, a, h }
                }
                3 => Kind::Sphere {
                    r: rng.uniform(0.5, 5.0),
                },
                _ => {
                    let big = rng.uniform(2.0, 5.0);
                    Kind::Torus {
                        big,
                        small: big * rng.uniform(0.1, 0.9),
                    }
                }
            };
            let (fu, fv) = match kind {
                Kind::Plane => (rng.uniform(0.0, 1.0), rng.uniform(0.0, 1.0)),
                Kind::Cylinder { .. } | Kind::Cone { .. } => (frac(rng, 4), frac(rng, 1)),
                Kind::Sphere { .. } => (frac(rng, 4), frac(rng, 2)),
                Kind::Torus { .. } => (frac(rng, 4), frac(rng, 4)),
            };
            let uv = match kind {
                Kind::Plane => (PLANE * (2.0 * fu - 1.0), PLANE * (2.0 * fv - 1.0)),
                Kind::Cylinder { r } => (2.0 * core::f64::consts::PI * r * fu, HEIGHT * fv),
                Kind::Cone { h, .. } => (fu, h * fv),
                Kind::Sphere { .. } | Kind::Torus { .. } => (fu, fv),
            };
            out.push(Spec {
                kind,
                frame: [f.origin, f.z, f.x],
                reversed: rng.below(2) == 0,
                uv,
                above: (rng.below(2) == 0, rng.below(2) == 0),
            });
        }
        out
    }
}

/// scx's side.
pub mod x {
    use super::{Approach, Cone, Cylinder, Kind, Length, Plane, Point2, Spec, Sphere, Torus};

    pub enum S {
        Plane(Plane),
        Cylinder(Cylinder),
        Cone(Cone),
        Sphere(Sphere),
        Torus(Torus),
    }

    pub fn build(s: &Spec) -> S {
        let (f, sense, mm) = (s.frame(), s.sense(), Length::from_mm);
        match s.kind {
            Kind::Plane => S::Plane(Plane::new(f)),
            Kind::Cylinder { r } => S::Cylinder(Cylinder::new(f, mm(r), sense).unwrap()),
            Kind::Cone { r, a, .. } => S::Cone(Cone::new(f, mm(r), a, sense).unwrap()),
            Kind::Sphere { r } => S::Sphere(Sphere::new(f, mm(r), sense).unwrap()),
            Kind::Torus { big, small } => {
                S::Torus(Torus::new(f, mm(big), mm(small), sense).unwrap())
            }
        }
    }

    macro_rules! each {
        ($s:expr, $k:ident => $e:expr) => {
            match $s {
                S::Plane($k) => $e,
                S::Cylinder($k) => $e,
                S::Cone($k) => $e,
                S::Sphere($k) => $e,
                S::Torus($k) => $e,
            }
        };
    }

    pub fn point(s: &Spec) -> [f64; 3] {
        let p = each!(build(s), k => k.point(s.uv()));
        [p.x, p.y, p.z]
    }

    pub fn normal(s: &Spec) -> Option<[f64; 3]> {
        let n = match build(s) {
            S::Plane(k) => Some(*k.normal(s.uv())),
            S::Cylinder(k) => Some(*k.normal(s.uv())),
            S::Cone(k) => k.normal(s.uv()).map(|n| *n),
            S::Sphere(k) => Some(*k.normal(s.uv())),
            S::Torus(k) => Some(*k.normal(s.uv())),
        };
        n.map(|n| [n.x, n.y, n.z])
    }

    fn side(above: bool) -> Approach {
        if above {
            Approach::Above
        } else {
            Approach::Below
        }
    }

    /// The partials `(i, j)`, `i + j <= 2`, `i` outer.
    pub fn jet(s: &Spec) -> Vec<[f64; 3]> {
        let sides = [side(s.above.0), side(s.above.1)];
        let j = each!(build(s), k => k.jet::<2>(s.uv(), sides));
        let mut out = Vec::new();
        for i in 0..=2 {
            for k in 0..=2 - i {
                let d = j.get(i, k).unwrap_or_default();
                out.push([d.x, d.y, d.z]);
            }
        }
        out
    }

    fn degrees(s: &Spec, p: Point2) -> (f64, f64) {
        let d = 180.0 / core::f64::consts::PI;
        match s.kind {
            Kind::Plane => (p.x, p.y),
            Kind::Cylinder { .. } | Kind::Cone { .. } => (p.x * d, p.y),
            Kind::Sphere { .. } | Kind::Torus { .. } => (p.x * d, p.y * d),
        }
    }

    pub fn to_step(s: &Spec) -> (f64, f64) {
        let st = each!(build(s), k => k.to_step(s.uv()));
        degrees(s, st)
    }

    pub fn from_step(s: &Spec) -> (f64, f64) {
        let (a, b) = s.step_degrees();
        let r = core::f64::consts::PI / 180.0;
        let st = match s.kind {
            Kind::Plane => Point2::new(a, b),
            Kind::Cylinder { .. } | Kind::Cone { .. } => Point2::new(a * r, b),
            Kind::Sphere { .. } | Kind::Torus { .. } => Point2::new(a * r, b * r),
        };
        let uv = each!(build(s), k => k.from_step(st));
        (uv.x, uv.y)
    }
}

/// The pin's side.
pub mod s {
    use super::{HEIGHT, Kind, PLANE, Spec};
    use solidcore::geom::axis2_placement::Axis2Placement;
    use solidcore::geom::extent2d::Extent2d;
    use solidcore::nurbs::b_spline_surface_n_lib::EvaluateFlags;
    use solidcore::nurbs::knots::Side;
    use solidcore::surface::cone::p1::Cone;
    use solidcore::surface::plane::Plane;
    use solidcore::surface::sphere::Sphere;
    use solidcore::surface::surface::p2::Surface;
    use solidcore::surface::torus::{Torus, TorusOptions};
    use solidcore::{Kernel, Point2, Point3, Vec2, Vec3};

    /// The forward surface, the frame's origin and z (for the mirror), and the uv to
    /// evaluate it at: the spec's, with a cylinder's `u` on a break moved onto the pin's
    /// knot there. The pin's arc-length knots and scx's breaks (`k (2 pi r / 4)`) are each
    /// within a few ulps of the true `k pi r / 2`, not always the same double; a break is
    /// compared as the break on both sides, not as a point an ulp to one side of it.
    fn build(s: &Spec, k: &Kernel) -> (Surface, Point3, Vec3, Point2) {
        let mut at = Point2::new(s.uv.0, s.uv.1);
        let f = s.frame();
        let o = f.origin();
        let (x, y, z) = (*f.x(), *f.y(), *f.z());
        let (o, x, y, z) = (
            Point3::new(o.x, o.y, o.z),
            Vec3::new(x.x, x.y, x.z),
            Vec3::new(y.x, y.y, y.z),
            Vec3::new(z.x, z.y, z.z),
        );
        let place = Axis2Placement::new(o, x, y);
        let surface = match s.kind {
            Kind::Plane => {
                let ext = Extent2d::from_bounds(-PLANE, -PLANE, PLANE, PLANE);
                let mut p = Plane::new(o, x, y, Vec2::new(1.0, 1.0), &ext);
                p.make_nurb().unwrap();
                Surface::from(p)
            }
            Kind::Cylinder { r } => {
                let mut c =
                    Cone::new_cylinder_bounded(o, x, y, r, 0.0, 360.0, HEIGHT, false, false, true);
                c.make_nurb(k).unwrap();
                let knots = c.base_mut().gw_nurb().unwrap().u_knots();
                if let Some(&t) = knots.iter().find(|&&t| (t - at.x).abs() <= 1e-13 * r) {
                    at.x = t;
                }
                Surface::from(c)
            }
            Kind::Cone { r, a, h } => {
                let top = r + scx_num::fmath::tan(a) * h;
                let mut c = Cone::new_bounded(o, x, y, r, top, 0.0, 360.0, h, false, false, true);
                c.make_nurb(k).unwrap();
                Surface::from(c)
            }
            Kind::Sphere { r } => Surface::from(Sphere::create_canonical(k, &place, r).unwrap()),
            Kind::Torus { big, small } => {
                let opts = TorusOptions {
                    swap_uv: false,
                    inside_out: false,
                    circle_nurb: None,
                };
                Surface::from(Torus::create_canonical(k, &place, big, small, opts, None).unwrap())
            }
        };
        (surface, o, z, at)
    }

    fn side(above: bool) -> Side {
        if above { Side::Left } else { Side::Right }
    }

    /// The mirror of a vector in the frame's xy plane, for a reversed kind.
    fn mirror(s: &Spec, z: Vec3, v: Vec3) -> [f64; 3] {
        let v = if s.reversed {
            v - z * (2.0 * v.dot(z))
        } else {
            v
        };
        [v.x, v.y, v.z]
    }

    fn rows(s: &Spec, order: usize) -> (Vec<Vec<Vec3>>, Point3, Vec3) {
        let k = Kernel::default();
        let (surface, o, z, at) = build(s, &k);
        let flags = EvaluateFlags {
            u_side: side(s.above.0),
            v_side: side(s.above.1),
            only_upper_half: false,
            non_zero_tangents: false,
        };
        let rows = surface.evaluate(&k, at, order, order, flags).unwrap();
        (rows, o, z)
    }

    pub fn point(s: &Spec) -> [f64; 3] {
        let (rows, o, z) = rows(s, 0);
        let p = rows[0][0];
        let m = mirror(s, z, Vec3::new(p.x - o.x, p.y - o.y, p.z - o.z));
        [o.x + m[0], o.y + m[1], o.z + m[2]]
    }

    pub fn normal(s: &Spec) -> Option<[f64; 3]> {
        let k = Kernel::default();
        let (surface, _, z, at) = build(s, &k);
        let n = surface
            .evaluate_normal(&k, at, side(s.above.0), side(s.above.1))
            .ok()?;
        let m = mirror(s, z, n);
        Some(if s.reversed { m.map(|c| -c) } else { m })
    }

    pub fn jet(s: &Spec) -> Vec<[f64; 3]> {
        let (rows, o, z) = rows(s, 2);
        let mut out = Vec::new();
        for i in 0..=2 {
            for j in 0..=2 - i {
                let d = rows[i][j];
                if i == 0 && j == 0 {
                    let m = mirror(s, z, Vec3::new(d.x - o.x, d.y - o.y, d.z - o.z));
                    out.push([o.x + m[0], o.y + m[1], o.z + m[2]]);
                } else {
                    out.push(mirror(s, z, d));
                }
            }
        }
        out
    }

    pub fn to_step(s: &Spec) -> (f64, f64) {
        let k = Kernel::default();
        let (surface, _, _, at) = build(s, &k);
        let p = surface.convert_uv_from_nurbs_to_step(&k, at).unwrap();
        (p.x, p.y)
    }

    pub fn from_step(s: &Spec) -> (f64, f64) {
        let k = Kernel::default();
        let (a, b) = s.step_degrees();
        let p = build(s, &k)
            .0
            .convert_uv_from_step_to_nurbs(&k, Point2::new(a, b))
            .unwrap();
        (p.x, p.y)
    }
}

macro_rules! surface_diff {
    ($($name:ident: $f:ident;)*) => {$(
        #[test]
        fn $name() {
            run_tol::<(Spec,), _, _>(
                concat!(module_path!(), "::", stringify!($name)),
                TOL,
                |(s,)| x::$f(&s),
                |(s,)| s::$f(&s),
            );
        }
    )*};
}

#[test]
fn surface_jet() {
    run_defect_tol::<(Spec,), _, _>(
        concat!(module_path!(), "::surface_jet"),
        TOL,
        &crate::pin_defects::surface_jet(),
        |(s,)| x::jet(&s),
        |(s,)| s::jet(&s),
    );
}

surface_diff! {
    surface_point: point;
    surface_normal: normal;
    surface_to_step: to_step;
    surface_from_step: from_step;
}
