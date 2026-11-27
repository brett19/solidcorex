// Written for solidcorex. Apache-2.0.
//! Where a line, circle or ellipse meets a plane, cylinder, cone, sphere or torus (research
//! 63 T2): every intersection, certified, with its multiplicity, its parameter on the
//! curve and its uv on the surface (STEP's and the pin's, D13). Design note
//! docs/research/64-intersection-truths.md §3.
//!
//! **The method.** The shapes are taken as [`crate::analytic`] gives them, frame vectors as
//! given. With `M = [X Y Z]` the surface frame's axes as columns (the plane's third column
//! `X x Y`), a world point `P` has frame coordinates `L = M^-1 (P - C)`, exact in
//! rationals, and the surface is exactly the zero set of
//!
//! - plane `L_z = 0`; cylinder `L_x^2 + L_y^2 - r^2`;
//! - cone `L_x^2 + L_y^2 - (r + k L_z)^2` (both nappes: STEP's parameterisation reaches
//!   the far one with `r + k v < 0`), `k` the semi-angle's tangent held as the rational
//!   value of its double-double (within `2^-104` relative: the one rounding);
//! - sphere `|L|^2 - r^2`; torus `(|L|^2 + R^2 - r^2)^2 - 4 R^2 (L_x^2 + L_y^2)` for
//!   `R > r` (the ring torus, where this is exactly the torus).
//!
//! The line `P + t D` is linear in `t`. The circle and ellipse are rational quadratics in
//! `w = tan(t/2)`: `C (1 + w^2) + a X (1 - w^2) + b Y 2 w` over `1 + w^2`, every turn but
//! `t = pi`, which is the root at infinity (the degree's drop). Substituted, the equation is
//! an exact polynomial in the curve parameter, of degree 1 or 2 for a line against a
//! quadric, 4 against the torus; 2 or 4 for a conic against a quadric, 8 against the torus.
//! [`crate::poly`] gives its real roots, isolated and refined in rational intervals, with
//! their multiplicities. A zero polynomial is a curve on the surface.
//!
//! A root of multiplicity 1 is a crossing; more, a tangency (multiplicity 2 a touch, 3 an
//! inflectional crossing, and so on). The parameter and uv come from the root in
//! double-double: within the root's interval (about `2^-110` relative) and the
//! double-double rounding.

use crate::analytic::{Circle, Cone, Cylinder, Ellipse, Line, Plane, Sphere, Torus, V3};
use crate::dd::{self, Dd};
use crate::pin_uv;
use crate::poly::{of_dd, to_dd, Poly, Root};
use crate::rat::Rat;

/// A curve the truths take.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum CurveKind {
    /// `P + t D`.
    Line(Line),
    /// `C + r (cos t X + sin t Y)`.
    Circle(Circle),
    /// `C + a cos t X + b sin t Y`.
    Ellipse(Ellipse),
}

/// A surface the truths take.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum SurfaceKind {
    /// `P + u X + v Y`.
    Plane(Plane),
    /// `C + r (cos u X + sin u Y) + v Z`.
    Cylinder(Cylinder),
    /// `C + (r + v tan a) (cos u X + sin u Y) + v Z`.
    Cone(Cone),
    /// `C + r cos v (cos u X + sin u Y) + r sin v Z`.
    Sphere(Sphere),
    /// `C + (R + r cos v) (cos u X + sin u Y) + r sin v Z`, `R > r`.
    Torus(Torus),
}

/// Why a truth has no answer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error {
    /// A value is an infinity or NaN.
    NonFinite,
    /// A frame's axes (or a plane's `X x Y` with them) are dependent.
    DegenerateFrame,
    /// A line's direction is zero, or a radius or semi-axis is not above zero.
    DegenerateCurve,
    /// A radius is negative, or a torus is not a ring torus (`R > r > 0`).
    DegenerateSurface,
    /// The exact predicate that decides the answer's kind is within the rounding of a
    /// transcendental input (a cone's slope) of zero: the doubles do not decide it.
    Undecided,
}

impl core::fmt::Display for Error {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(match self {
            Error::NonFinite => "a value is not finite",
            Error::DegenerateFrame => "a frame's axes are dependent",
            Error::DegenerateCurve => "the curve is degenerate",
            Error::DegenerateSurface => "the surface is degenerate",
            Error::Undecided => "the inputs' rounding does not decide the kind",
        })
    }
}

impl std::error::Error for Error {}

/// How the curve meets the surface at a point.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Contact {
    /// A simple root: the curve crosses.
    Crossing,
    /// A multiple root: the curve is tangent to the surface there.
    Tangent,
}

/// One intersection point.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Hit {
    /// The curve parameter: the line's `t`, the conic's angle in `[0, 2 pi)`.
    pub t: Dd,
    /// A bound on `|t_true - t|` from the root's interval (the double-double rounding
    /// aside).
    pub t_radius: f64,
    /// The point, the curve's at `t`.
    pub point: V3,
    /// The surface's STEP parameters: angles in radians, `u` in `[0, 2 pi)`; `u` is zero on
    /// a sphere's pole, a cone's apex.
    pub uv: (Dd, Dd),
    /// The pin's uv (D13, [`crate::pin_uv`]); `None` on a cone's far nappe, which the pin's
    /// surface does not reach.
    pub pin_uv: Option<(Dd, Dd)>,
    /// The root's multiplicity.
    pub multiplicity: u32,
    /// Crossing or tangent.
    pub contact: Contact,
}

/// Where a curve meets a surface.
#[derive(Clone, Debug, PartialEq)]
pub enum Meeting {
    /// At isolated points (none, perhaps), ascending in the curve parameter.
    Points(Vec<Hit>),
    /// The curve lies on the surface.
    OnSurface,
}

// ---------------------------------------------------------------- exact frame coordinates

/// A vector of three exact rationals.
pub(crate) type R3 = [Rat; 3];

pub(crate) fn r3(v: [f64; 3]) -> Result<R3, Error> {
    let f = |x: f64| Rat::from_f64(x).ok_or(Error::NonFinite);
    Ok([f(v[0])?, f(v[1])?, f(v[2])?])
}

pub(crate) fn rf(x: f64) -> Result<Rat, Error> {
    Rat::from_f64(x).ok_or(Error::NonFinite)
}

pub(crate) fn rdot(a: &R3, b: &R3) -> Rat {
    a.iter().zip(b).fold(Rat::zero(), |s, (x, y)| s + x * y)
}

pub(crate) fn rcross(a: &R3, b: &R3) -> R3 {
    let [a0, a1, a2] = a;
    let [b0, b1, b2] = b;
    [
        &(a1 * b2) - &(a2 * b1),
        &(a2 * b0) - &(a0 * b2),
        &(a0 * b1) - &(a1 * b0),
    ]
}

pub(crate) fn rsub(a: &R3, b: &R3) -> R3 {
    let [a0, a1, a2] = a;
    let [b0, b1, b2] = b;
    [a0 - b0, a1 - b1, a2 - b2]
}

/// The exact frame coordinates `L = M^-1 (P - C)`: `L_i = (P - C) . row_i / det`.
pub(crate) struct Local {
    pub(crate) origin: R3,
    /// The rows of `det M^-1`: `Y x Z`, `Z x X`, `X x Y`.
    pub(crate) rows: [R3; 3],
    pub(crate) det: Rat,
}

impl Local {
    pub(crate) fn new(
        origin: [f64; 3],
        x: [f64; 3],
        y: [f64; 3],
        z: [f64; 3],
    ) -> Result<Local, Error> {
        Local::of(r3(origin)?, &r3(x)?, &r3(y)?, &r3(z)?)
    }

    pub(crate) fn of(origin: R3, x: &R3, y: &R3, z: &R3) -> Result<Local, Error> {
        let yz = rcross(y, z);
        let det = rdot(x, &yz);
        if det.is_zero() {
            return Err(Error::DegenerateFrame);
        }
        let rows = [yz, rcross(z, x), rcross(x, y)];
        Ok(Local { origin, rows, det })
    }

    /// The coordinates of a homogeneous point `(p(t), w(t))`, each over `w(t)`, as
    /// polynomials `L_i(t) w(t)`.
    fn poly(&self, p: &[Poly; 3], w: &Poly) -> [Poly; 3] {
        let inv = self.det.recip().unwrap_or_default();
        let d: Vec<Poly> = (0..3)
            .map(|i| {
                let pi = p.get(i).cloned().unwrap_or_default();
                let oi = self.origin.get(i).cloned().unwrap_or_default();
                pi.sub(&w.scale(&oi))
            })
            .collect();
        self.rows.clone().map(|row| {
            d.iter()
                .zip(&row)
                .fold(Poly::default(), |s, (di, ri)| s.add(&di.scale(ri)))
                .scale(&inv)
        })
    }

    /// The coordinates of a double-double point, in double-double.
    pub(crate) fn of_dd(&self, p: V3) -> [Dd; 3] {
        let o = self.origin.each_ref().map(to_dd);
        let [p0, p1, p2] = p;
        let [o0, o1, o2] = o;
        let d = [p0 - o0, p1 - o1, p2 - o2];
        let inv = to_dd(&self.det.recip().unwrap_or_default());
        self.rows
            .each_ref()
            .map(|row| dd::dot(&d, &row.each_ref().map(to_dd)) * inv)
    }

    /// The gradient in world coordinates of a function of the frame coordinates whose
    /// gradient there is `g`: `M^-T g`.
    pub(crate) fn gradient(&self, g: [Dd; 3]) -> V3 {
        let inv = to_dd(&self.det.recip().unwrap_or_default());
        let rows = self.rows.each_ref().map(|r| r.each_ref().map(to_dd));
        let [g0, g1, g2] = g;
        let [r0, r1, r2] = rows;
        core::array::from_fn(|i| {
            let c = |r: &[Dd; 3]| r.get(i).copied().unwrap_or(Dd::new(0.0));
            (g0 * c(&r0) + g1 * c(&r1) + g2 * c(&r2)) * inv
        })
    }

    /// The coordinates of a point, exactly.
    pub(crate) fn of_point(&self, p: &R3) -> R3 {
        let d = rsub(p, &self.origin);
        let inv = self.det.recip().unwrap_or_default();
        self.rows.clone().map(|row| &rdot(&d, &row) * &inv)
    }
}

// ---------------------------------------------------------------- the surfaces

/// A surface's exact implicit form in its frame coordinates.
pub(crate) enum Implicit {
    Plane,
    Cylinder { r2: Rat },
    Cone { r: Rat, k: Rat },
    Sphere { r2: Rat },
    Torus { big2: Rat, small2: Rat },
}

/// A cone's slope `tan a` in double-double.
pub(crate) fn slope(semi_angle: f64) -> Dd {
    let (s, c) = dd::sin_cos(Dd::new(semi_angle));
    s / c
}

/// The frame coordinates and implicit form of a surface.
pub(crate) fn implicit(s: &SurfaceKind) -> Result<(Local, Implicit), Error> {
    let pos = |r: f64| {
        if r >= 0.0 {
            Ok(())
        } else {
            Err(Error::DegenerateSurface)
        }
    };
    Ok(match s {
        SurfaceKind::Plane(p) => {
            let f = &p.frame;
            let (x, y) = (r3(f.x)?, r3(f.y)?);
            let n = rcross(&x, &y);
            (Local::of(r3(f.origin)?, &x, &y, &n)?, Implicit::Plane)
        }
        SurfaceKind::Cylinder(c) => {
            pos(c.radius)?;
            let f = &c.frame;
            let r = rf(c.radius)?;
            (
                Local::new(f.origin, f.x, f.y, f.z)?,
                Implicit::Cylinder { r2: &r * &r },
            )
        }
        SurfaceKind::Cone(c) => {
            pos(c.radius)?;
            if !(c.semi_angle > 0.0 && c.semi_angle < core::f64::consts::FRAC_PI_2) {
                return Err(Error::DegenerateSurface);
            }
            let f = &c.frame;
            let k = of_dd(slope(c.semi_angle)).ok_or(Error::NonFinite)?;
            (
                Local::new(f.origin, f.x, f.y, f.z)?,
                Implicit::Cone {
                    r: rf(c.radius)?,
                    k,
                },
            )
        }
        SurfaceKind::Sphere(c) => {
            pos(c.radius)?;
            let f = &c.frame;
            let r = rf(c.radius)?;
            (
                Local::new(f.origin, f.x, f.y, f.z)?,
                Implicit::Sphere { r2: &r * &r },
            )
        }
        SurfaceKind::Torus(t) => {
            if !(t.minor > 0.0 && t.major > t.minor) {
                return Err(Error::DegenerateSurface);
            }
            let f = &t.frame;
            let (a, b) = (rf(t.major)?, rf(t.minor)?);
            (
                Local::new(f.origin, f.x, f.y, f.z)?,
                Implicit::Torus {
                    big2: &a * &a,
                    small2: &b * &b,
                },
            )
        }
    })
}

impl Implicit {
    /// `w^k F(L / w)` for homogeneous coordinates `L w` and weight `w`.
    fn homogeneous(&self, l: &[Poly; 3], w: &Poly) -> Poly {
        let [x, y, z] = l;
        let rho2 = x.mul(x).add(&y.mul(y));
        let w2 = w.mul(w);
        match self {
            Implicit::Plane => z.clone(),
            Implicit::Cylinder { r2 } => rho2.sub(&w2.scale(r2)),
            Implicit::Cone { r, k } => {
                let g = w.scale(r).add(&z.scale(k));
                rho2.sub(&g.mul(&g))
            }
            Implicit::Sphere { r2 } => rho2.add(&z.mul(z)).sub(&w2.scale(r2)),
            Implicit::Torus { big2, small2 } => {
                let s = rho2.add(&z.mul(z)).add(&w2.scale(&(big2 - small2)));
                s.mul(&s).sub(&w2.mul(&rho2).scale(&big2.scale(2)))
            }
        }
    }

    /// `F` at exact frame coordinates.
    pub(crate) fn at(&self, l: &R3) -> Rat {
        let [x, y, z] = l;
        let rho2 = &(x * x) + &(y * y);
        match self {
            Implicit::Plane => z.clone(),
            Implicit::Cylinder { r2 } => &rho2 - r2,
            Implicit::Cone { r, k } => {
                let g = r + &(k * z);
                &rho2 - &(&g * &g)
            }
            Implicit::Sphere { r2 } => &(&rho2 + &(z * z)) - r2,
            Implicit::Torus { big2, small2 } => {
                let s = &(&rho2 + &(z * z)) + &(big2 - small2);
                &(&s * &s) - &(&rho2 * &big2.scale(2))
            }
        }
    }

    /// The gradient of `F` at frame coordinates, in double-double.
    pub(crate) fn grad(&self, l: [Dd; 3]) -> [Dd; 3] {
        let [x, y, z] = l;
        let zero = Dd::new(0.0);
        match self {
            Implicit::Plane => [zero, zero, Dd::new(1.0)],
            Implicit::Cylinder { .. } => [x * 2.0, y * 2.0, zero],
            Implicit::Cone { r, k } => {
                let k = to_dd(k);
                let g = to_dd(r) + k * z;
                [x * 2.0, y * 2.0, -(k * g * 2.0)]
            }
            Implicit::Sphere { .. } => [x * 2.0, y * 2.0, z * 2.0],
            Implicit::Torus { big2, small2 } => {
                let s = x * x + y * y + z * z + to_dd(&(big2 - small2));
                let b = to_dd(big2) * 8.0;
                [x * (s * 4.0 - b), y * (s * 4.0 - b), z * s * 4.0]
            }
        }
    }

    /// The nominal degree `k` of `F`.
    fn degree(&self) -> usize {
        match self {
            Implicit::Plane => 1,
            Implicit::Torus { .. } => 4,
            _ => 2,
        }
    }
}

/// The STEP uv of frame coordinates on the surface.
pub(crate) fn step_uv(s: &SurfaceKind, l: [Dd; 3]) -> (Dd, Dd) {
    let [x, y, z] = l;
    let angle = |y: Dd, x: Dd| pin_uv::turn(dd::atan2(y, x));
    let rho = (x * x + y * y).sqrt();
    match s {
        SurfaceKind::Plane(_) => (x, y),
        SurfaceKind::Cylinder(_) => (angle(y, x), z),
        SurfaceKind::Cone(c) => {
            let g = Dd::new(c.radius) + slope(c.semi_angle) * z;
            if g.hi < 0.0 {
                (angle(-y, -x), z)
            } else {
                (angle(y, x), z)
            }
        }
        SurfaceKind::Sphere(_) => (angle(y, x), dd::atan2(z, rho)),
        SurfaceKind::Torus(t) => {
            let v = dd::atan2(z, rho - Dd::new(t.major));
            (angle(y, x), pin_uv::turn(v))
        }
    }
}

/// The pin's uv (D13) of a STEP uv on the surface; `None` on the cone's far nappe.
pub fn pin_uv_of(s: &SurfaceKind, (u, v): (Dd, Dd)) -> Option<(Dd, Dd)> {
    match s {
        SurfaceKind::Plane(_) => Some((u, v)),
        SurfaceKind::Cylinder(_) => Some((pin_uv::cylinder_u(u), v)),
        SurfaceKind::Cone(c) => {
            let g = Dd::new(c.radius) + slope(c.semi_angle) * v;
            (g.hi >= 0.0).then(|| (pin_uv::turn_fraction(u), v))
        }
        SurfaceKind::Sphere(_) => Some((pin_uv::turn_fraction(u), pin_uv::latitude_fraction(v))),
        SurfaceKind::Torus(_) => Some((pin_uv::turn_fraction(u), pin_uv::turn_fraction(v))),
    }
}

/// The STEP uv of a pin uv (D13) on the surface.
pub fn step_uv_of(s: &SurfaceKind, (u, v): (Dd, Dd)) -> (Dd, Dd) {
    match s {
        SurfaceKind::Plane(_) => (u, v),
        SurfaceKind::Cylinder(_) => (pin_uv::cylinder_angle(u), v),
        SurfaceKind::Cone(_) => (pin_uv::fraction_turn(u), v),
        SurfaceKind::Sphere(_) => (pin_uv::fraction_turn(u), pin_uv::fraction_latitude(v)),
        SurfaceKind::Torus(_) => (pin_uv::fraction_turn(u), pin_uv::fraction_turn(v)),
    }
}

/// The surface's point at a STEP uv, through [`crate::analytic`].
pub fn surface_point(s: &SurfaceKind, uv: (Dd, Dd)) -> V3 {
    let d = match s {
        SurfaceKind::Plane(p) => p.derivs(uv, 0),
        SurfaceKind::Cylinder(c) => c.derivs(uv, 0),
        SurfaceKind::Cone(c) => c.derivs(uv, 0),
        SurfaceKind::Sphere(c) => c.derivs(uv, 0),
        SurfaceKind::Torus(c) => c.derivs(uv, 0),
    };
    d.get(0, 0).copied().unwrap_or([Dd::new(0.0); 3])
}

/// The curve's point at its parameter, through [`crate::analytic`].
pub fn curve_point(c: &CurveKind, t: Dd) -> V3 {
    let d = match c {
        CurveKind::Line(l) => l.derivs(t, 0),
        CurveKind::Circle(k) => k.derivs(t, 0),
        CurveKind::Ellipse(e) => e.derivs(t, 0),
    };
    d.into_iter().next().unwrap_or([Dd::new(0.0); 3])
}

// ---------------------------------------------------------------- the curves

/// What the polynomials' variable `s` is.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Variable {
    /// The line's parameter itself.
    Parameter,
    /// A conic's half-angle tangent, `s = tan(t / 2)`.
    HalfAngle,
}

/// The curve as exact homogeneous polynomials `(P(s), W(s))` in its variable.
fn homogeneous(c: &CurveKind) -> Result<([Poly; 3], Poly, Variable), Error> {
    let conic = |centre: [f64; 3], a: f64, x: [f64; 3], b: f64, y: [f64; 3]| {
        if !(a > 0.0 && b > 0.0) {
            return Err(Error::DegenerateCurve);
        }
        let (c, x, y) = (r3(centre)?, r3(x)?, r3(y)?);
        let (a, b) = (rf(a)?, rf(b)?);
        let one = Rat::one();
        let p = [0, 1, 2].map(|i| {
            let ci = c.get(i).cloned().unwrap_or_default();
            let xi = &x.get(i).cloned().unwrap_or_default() * &a;
            let yi = &y.get(i).cloned().unwrap_or_default() * &b;
            // c (1 + w^2) + xi (1 - w^2) + yi 2 w
            Poly::new(vec![&ci + &xi, yi.scale(1), &ci - &xi])
        });
        Ok((
            p,
            Poly::new(vec![one.clone(), Rat::zero(), one]),
            Variable::HalfAngle,
        ))
    };
    match c {
        CurveKind::Line(l) => {
            let (p, d) = (r3(l.origin)?, r3(l.direction)?);
            if d.iter().all(Rat::is_zero) {
                return Err(Error::DegenerateCurve);
            }
            let poly = [0, 1, 2].map(|i| {
                Poly::linear(
                    p.get(i).cloned().unwrap_or_default(),
                    d.get(i).cloned().unwrap_or_default(),
                )
            });
            Ok((poly, Poly::constant(Rat::one()), Variable::Parameter))
        }
        CurveKind::Circle(k) => {
            let f = &k.frame;
            conic(f.origin, k.radius, f.x, k.radius, f.y)
        }
        CurveKind::Ellipse(e) => {
            let f = &e.frame;
            conic(f.origin, e.semi_x, f.x, e.semi_y, f.y)
        }
    }
}

/// Where `curve` meets `surface`: every point, or that the curve lies on the surface.
pub fn intersect(curve: &CurveKind, surface: &SurfaceKind) -> Result<Meeting, Error> {
    let (local, f) = implicit(surface)?;
    let (p, w, var) = homogeneous(curve)?;
    let l = local.poly(&p, &w);
    let eq = f.homogeneous(&l, &w);
    if eq.is_zero() {
        return Ok(Meeting::OnSurface);
    }
    let coords = |s: Dd| -> [Dd; 3] {
        let ws = w.eval_dd(s);
        [0, 1, 2].map(|i| l.get(i).map_or(Dd::new(0.0), |q| q.eval_dd(s)) / ws)
    };
    let mut hits = Vec::new();
    let mut add = |t: Dd, t_radius: f64, lc: [Dd; 3], multiplicity: u32| {
        let uv = step_uv(surface, lc);
        hits.push(Hit {
            t,
            t_radius,
            point: curve_point(curve, t),
            uv,
            pin_uv: pin_uv_of(surface, uv),
            multiplicity,
            contact: if multiplicity == 1 {
                Contact::Crossing
            } else {
                Contact::Tangent
            },
        });
    };
    for root in eq.real_roots() {
        let s = root.value();
        let (t, rad) = match var {
            Variable::HalfAngle => (pin_uv::turn(dd::atan(s) * 2.0), 2.0 * root.radius()),
            Variable::Parameter => (s, root.radius()),
        };
        add(t, rad, coords(s), root.multiplicity);
    }
    if var == Variable::HalfAngle {
        // The degree's drop below its nominal 2k is the root at infinity, t = pi, with that
        // multiplicity; the coordinates there are the leading coefficients' ratios.
        let nominal = 2 * f.degree();
        let deg = eq.degree().unwrap_or(0);
        if deg < nominal {
            let lead = |q: &Poly| q.coeffs().get(2).cloned().unwrap_or_default();
            let wl = lead(&w);
            let lc = [0, 1, 2].map(|i| {
                l.get(i).map_or(Dd::new(0.0), |q| {
                    to_dd(&lead(q).checked_div(&wl).unwrap_or_default())
                })
            });
            add(dd::consts().pi, 0.0, lc, (nominal - deg) as u32);
        }
    }
    hits.sort_by(|a, b| a.t.hi.total_cmp(&b.t.hi).then(a.t.lo.total_cmp(&b.t.lo)));
    Ok(Meeting::Points(hits))
}

/// The exact equation in the curve parameter (`t` for a line, `tan(t/2)` for a conic)
/// whose real roots [`intersect`] reports: for the self-tests.
pub fn equation(curve: &CurveKind, surface: &SurfaceKind) -> Result<Poly, Error> {
    let (local, f) = implicit(surface)?;
    let (p, w, _) = homogeneous(curve)?;
    Ok(f.homogeneous(&local.poly(&p, &w), &w))
}

/// The real roots of [`equation`], for the self-tests.
pub fn roots(curve: &CurveKind, surface: &SurfaceKind) -> Result<Vec<Root>, Error> {
    Ok(equation(curve, surface)?.real_roots())
}

/// The surface's implicit value at a world point, exactly (zero exactly on the surface):
/// for the self-tests and generators.
pub fn residual(surface: &SurfaceKind, p: [f64; 3]) -> Result<Rat, Error> {
    let (local, f) = implicit(surface)?;
    Ok(f.at(&local.of_point(&r3(p)?)))
}

/// A world point's first-order distance from the surface, `|F(p)| / |grad F(p)|`: `F`
/// exact at `p`, the gradient in double-double. It is the distance to within a relative
/// `O(d / size)` (zero exactly on the surface); at a point where the gradient vanishes
/// (a cone's apex) it is `|F|`.
pub fn distance(surface: &SurfaceKind, p: [f64; 3]) -> Result<f64, Error> {
    let (local, f) = implicit(surface)?;
    let value = to_dd(&f.at(&local.of_point(&r3(p)?))).abs();
    let g = local.gradient(f.grad(local.of_dd(p.map(Dd::new))));
    let norm = dd::dot(&g, &g).sqrt();
    Ok(if norm.hi > 0.0 { value / norm } else { value }.to_f64())
}
