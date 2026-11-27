// Written for solidcorex. Apache-2.0.
//! The analytic surfaces as exact implicit equations; an exact decision whether a rational
//! surface lies on one; and the NURBS-compatible uv parameterisation (research 63 D13) in
//! closed form (design note docs/research/59-truth-references.md §9).
//!
//! **Implicit forms.** Each kind is parameterised as [`crate::analytic`] does (STEP's form,
//! angles in radians, the frame's vectors used as given). With `M = [X Y Z]` (the frame's
//! axes as columns, taken exactly) and `L = M^-1 (P - C)` the frame-local point:
//!
//! - plane `L_z = 0`;
//! - cylinder `L_x^2 + L_y^2 = r^2`;
//! - cone `L_x^2 + L_y^2 = (r + k L_z)^2`, `k` the slope (the semi-angle's tangent) held as
//!   a double, exactly;
//! - sphere `|L|^2 = r^2`;
//! - torus `(|L|^2 + R^2 - r^2)^2 = 4 R^2 (L_x^2 + L_y^2)`.
//!
//! A frame that is not orthonormal describes the affine image these equations give, the
//! same shape its parameterisation traces, so the equation and the parameterisation agree
//! for any frame with independent axes.
//!
//! **The decision.** On one span patch of a rational surface of degrees `(p, q)`, the
//! homogeneous coordinates are polynomials of degrees `(p, q)`, and `w^k F(P / w)` for an
//! equation `F` of degree `k` (1 for the plane, 2 for the quadrics, 4 for the torus) is a
//! polynomial of degrees `(k p, k q)`. A polynomial of those degrees that vanishes on a grid
//! of `(k p + 1) x (k q + 1)` distinct points is zero. [`Implicit::check`] evaluates the
//! surface exactly on such a grid inside every patch and `F` exactly there: all zero means
//! the surface lies exactly on the kind (weights positive, so `w` never vanishes). This is
//! exact in rationals: a NURB of doubles whose middle weights are irrational in truth (the
//! standard form's `cos(pi/4)`, rounded) is off by its rounding, and is reported so, with a
//! first-order distance `|F| / |grad F|` per point (relative error of the order of the
//! distance over the size).
//!
//! **The uv map (D13).** A NURB of an analytic kind has its own parameter: per direction
//! either linear in the STEP parameter, or equal standard-form rational quadratic arcs
//! (end weights 1, middle weight `cos(h)` for an arc of `2 h`), one per knot span. On an arc
//! of `2 h` the angle from its start at the span's local parameter `s` in `[0, 1]` is
//! `h + 2 atan((2 s - 1) tan(h / 2))`, and back, `s = (1 + tan((phi - h) / 2) / tan(h / 2)) / 2`.
//! [`Param`] and [`Layout`] hold that map; [`Implicit::point_at`] is the closed-form point
//! at a NURB parameter.

use core::fmt;

use crate::analytic::{self, Frame, V3};
use crate::dd::{self, lift, Dd};
use crate::nurbs::{self, Side, Surface};
use crate::rat::Rat;

/// Why a check could not be made.
#[derive(Clone, Debug, PartialEq)]
pub enum Error {
    /// The surface could not be evaluated.
    Nurbs(nurbs::Error),
    /// A frame value or size is infinite or NaN.
    NonFinite,
    /// The frame's axes are linearly dependent.
    SingularFrame,
    /// A weight is zero or negative, so the weight function may vanish.
    NonPositiveWeight,
    /// A span too narrow to hold the distinct sample parameters the decision needs.
    NarrowSpan,
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::Nurbs(e) => write!(f, "the surface could not be evaluated: {e}"),
            Error::NonFinite => write!(f, "a frame value or size is infinite or NaN"),
            Error::SingularFrame => write!(f, "the frame's axes are linearly dependent"),
            Error::NonPositiveWeight => write!(f, "a weight is not positive"),
            Error::NarrowSpan => write!(f, "a span is too narrow for the sample parameters"),
        }
    }
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Error::Nurbs(e) => Some(e),
            _ => None,
        }
    }
}

impl From<nurbs::Error> for Error {
    fn from(e: nurbs::Error) -> Error {
        Error::Nurbs(e)
    }
}

/// A plane, cylinder, cone, sphere or torus, parameterised as [`crate::analytic`] does.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Implicit {
    /// `C + u X + v Y`.
    Plane {
        /// `C`, `X`, `Y`, and `Z` off the plane (it fixes `L_z`).
        frame: Frame,
    },
    /// `C + r (cos u X + sin u Y) + v Z`.
    Cylinder {
        /// The axis frame.
        frame: Frame,
        /// `r`.
        radius: f64,
    },
    /// `C + (r + k v) (cos u X + sin u Y) + v Z`.
    Cone {
        /// The axis frame (`C` at the circle of radius `r`).
        frame: Frame,
        /// `r`, the radius at `v = 0`.
        radius: f64,
        /// `k`, the change of radius per unit along the axis (the semi-angle's tangent).
        slope: f64,
    },
    /// `C + r cos v (cos u X + sin u Y) + r sin v Z`.
    Sphere {
        /// The centre and axes.
        frame: Frame,
        /// `r`.
        radius: f64,
    },
    /// `C + (R + r cos v) (cos u X + sin u Y) + r sin v Z`.
    Torus {
        /// The centre and axes.
        frame: Frame,
        /// `R`.
        major: f64,
        /// `r`.
        minor: f64,
    },
}

/// The verdict of [`Implicit::check`].
#[derive(Clone, Debug, PartialEq)]
pub struct Verdict {
    /// The sample points evaluated.
    pub samples: usize,
    /// The samples not exactly on the surface.
    pub off: usize,
    /// The farthest of those, if any.
    pub worst: Option<Off>,
}

impl Verdict {
    /// Whether the surface lies exactly on the kind (no sample off it).
    pub fn on(&self) -> bool {
        self.off == 0
    }
}

/// A sample point off the surface.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Off {
    /// Its parameters on the checked surface.
    pub at: (f64, f64),
    /// Its first-order distance `|F| / |grad F|` (infinite where the gradient is zero).
    pub distance: f64,
}

/// The kind's equation in exact rationals: `L = inverse (P - origin)`, then `F(L)`.
struct Exactly {
    origin: [Rat; 3],
    /// The rows of `M^-1`.
    inverse: [[Rat; 3]; 3],
    shape: Shape,
}

enum Shape {
    Plane,
    Cylinder { r2: Rat },
    Cone { r: Rat, k: Rat },
    Sphere { r2: Rat },
    Torus { big2: Rat, small2: Rat },
}

fn rat(x: f64) -> Result<Rat, Error> {
    Rat::from_f64(x).ok_or(Error::NonFinite)
}

fn rat3(v: [f64; 3]) -> Result<[Rat; 3], Error> {
    Ok([rat(v[0])?, rat(v[1])?, rat(v[2])?])
}

fn dot(a: &[Rat; 3], b: &[Rat; 3]) -> Rat {
    &(&(&a[0] * &b[0]) + &(&a[1] * &b[1])) + &(&a[2] * &b[2])
}

fn cross(a: &[Rat; 3], b: &[Rat; 3]) -> [Rat; 3] {
    [
        &(&a[1] * &b[2]) - &(&a[2] * &b[1]),
        &(&a[2] * &b[0]) - &(&a[0] * &b[2]),
        &(&a[0] * &b[1]) - &(&a[1] * &b[0]),
    ]
}

fn scale3(v: &[Rat; 3], k: &Rat) -> [Rat; 3] {
    v.each_ref().map(|e| e * k)
}

fn add3(a: &[Rat; 3], b: &[Rat; 3]) -> [Rat; 3] {
    let [a0, a1, a2] = a;
    let [b0, b1, b2] = b;
    [a0 + b0, a1 + b1, a2 + b2]
}

fn sq(x: &Rat) -> Rat {
    x * x
}

impl Exactly {
    fn residual_and_gradient(&self, p: &[Rat; 3]) -> (Rat, [Rat; 3]) {
        let d = [
            &p[0] - &self.origin[0],
            &p[1] - &self.origin[1],
            &p[2] - &self.origin[2],
        ];
        let [a, b, c] = &self.inverse;
        let (x, y, z) = (dot(a, &d), dot(b, &d), dot(c, &d));
        let two = Rat::from(2);
        let zero = Rat::zero();
        // F and its gradient in L
        let (f, g) = match &self.shape {
            Shape::Plane => (z.clone(), [zero.clone(), zero, Rat::one()]),
            Shape::Cylinder { r2 } => (&(&sq(&x) + &sq(&y)) - r2, [&two * &x, &two * &y, zero]),
            Shape::Cone { r, k } => {
                let rho = r + &(k * &z);
                (
                    &(&sq(&x) + &sq(&y)) - &sq(&rho),
                    [&two * &x, &two * &y, -&(&(&two * k) * &rho)],
                )
            }
            Shape::Sphere { r2 } => (
                &(&(&sq(&x) + &sq(&y)) + &sq(&z)) - r2,
                [&two * &x, &two * &y, &two * &z],
            ),
            Shape::Torus { big2, small2 } => {
                let s = &(&(&(&sq(&x) + &sq(&y)) + &sq(&z)) + big2) - small2;
                let four = Rat::from(4);
                let f = &sq(&s) - &(&(&four * big2) * &(&sq(&x) + &sq(&y)));
                let gs = &four * &s;
                let gr = &gs - &(&Rat::from(8) * big2);
                (f, [&gr * &x, &gr * &y, &gs * &z])
            }
        };
        // grad_P F = M^-T grad_L F
        let [g0, g1, g2] = &g;
        let grad = add3(&add3(&scale3(a, g0), &scale3(b, g1)), &scale3(c, g2));
        (f, grad)
    }
}

impl Implicit {
    /// The frame.
    pub fn frame(&self) -> &Frame {
        match self {
            Implicit::Plane { frame }
            | Implicit::Cylinder { frame, .. }
            | Implicit::Cone { frame, .. }
            | Implicit::Sphere { frame, .. }
            | Implicit::Torus { frame, .. } => frame,
        }
    }

    /// The degree of the equation: 1 for the plane, 2 for the quadrics, 4 for the torus.
    pub fn degree(&self) -> usize {
        match self {
            Implicit::Plane { .. } => 1,
            Implicit::Torus { .. } => 4,
            _ => 2,
        }
    }

    fn exactly(&self) -> Result<Exactly, Error> {
        let f = self.frame();
        let (x, y, z) = (rat3(f.x)?, rat3(f.y)?, rat3(f.z)?);
        let det = dot(&x, &cross(&y, &z));
        let inv = det.recip().ok_or(Error::SingularFrame)?;
        let row = |v: [Rat; 3]| v.map(|e| &e * &inv);
        let shape = match *self {
            Implicit::Plane { .. } => Shape::Plane,
            Implicit::Cylinder { radius, .. } => Shape::Cylinder {
                r2: sq(&rat(radius)?),
            },
            Implicit::Cone { radius, slope, .. } => Shape::Cone {
                r: rat(radius)?,
                k: rat(slope)?,
            },
            Implicit::Sphere { radius, .. } => Shape::Sphere {
                r2: sq(&rat(radius)?),
            },
            Implicit::Torus { major, minor, .. } => Shape::Torus {
                big2: sq(&rat(major)?),
                small2: sq(&rat(minor)?),
            },
        };
        Ok(Exactly {
            origin: rat3(f.origin)?,
            inverse: [row(cross(&y, &z)), row(cross(&z, &x)), row(cross(&x, &y))],
            shape,
        })
    }

    /// `F(P)` exactly: zero exactly when `P` lies on the surface.
    pub fn residual(&self, p: &[Rat; 3]) -> Result<Rat, Error> {
        Ok(self.exactly()?.residual_and_gradient(p).0)
    }

    /// The first-order distance `|F| / |grad F|` of `P` from the surface (zero on it,
    /// infinite where the gradient is zero off it).
    pub fn distance(&self, p: &[Rat; 3]) -> Result<f64, Error> {
        let (f, g) = self.exactly()?.residual_and_gradient(p);
        Ok(first_order(&f, &g))
    }

    /// Whether the rational surface lies exactly on the kind, by exact evaluation on a grid
    /// of `(k p + 1) x (k q + 1)` parameters inside every span patch (module docs). The
    /// weights must be positive.
    pub fn check(&self, s: &Surface<3>) -> Result<Verdict, Error> {
        if s.weights().iter().any(|w| w.signum() <= 0) {
            return Err(Error::NonPositiveWeight);
        }
        let eq = self.exactly()?;
        let (pu, pv) = s.degrees();
        let (ku, kv) = s.knots();
        let (du, dv) = s.domain();
        let us = grid(ku, du, self.degree() * pu + 1)?;
        let vs = grid(kv, dv, self.degree() * pv + 1)?;
        let mut verdict = Verdict {
            samples: 0,
            off: 0,
            worst: None,
        };
        for &u in &us {
            for &v in &vs {
                let p = s.point((u, v), (Side::Above, Side::Above))?;
                let (f, g) = eq.residual_and_gradient(&p.exact);
                verdict.samples += 1;
                if f.is_zero() {
                    continue;
                }
                verdict.off += 1;
                let distance = first_order(&f, &g);
                if verdict.worst.is_none_or(|w| distance > w.distance) {
                    verdict.worst = Some(Off {
                        at: (u, v),
                        distance,
                    });
                }
            }
        }
        Ok(verdict)
    }

    /// The point at STEP parameters `(u, v)` (radians for angles), in double-double.
    pub fn point(&self, (u, v): (Dd, Dd)) -> V3 {
        let at = |p: crate::Partials<V3>| p.get(0, 0).copied().unwrap_or([Dd::new(0.0); 3]);
        match *self {
            Implicit::Plane { frame } => at(analytic::Plane { frame }.derivs((u, v), 0)),
            Implicit::Cylinder { frame, radius } => {
                at(analytic::Cylinder { frame, radius }.derivs((u, v), 0))
            }
            Implicit::Cone {
                frame,
                radius,
                slope,
            } => {
                let (s, c) = dd::sin_cos(u);
                let rho = Dd::new(radius) + v * Dd::new(slope);
                let (o, x, y, z) = (
                    lift(frame.origin),
                    lift(frame.x),
                    lift(frame.y),
                    lift(frame.z),
                );
                let mut p = o;
                for (((p, x), y), z) in p.iter_mut().zip(x).zip(y).zip(z) {
                    *p = *p + rho * (c * x + s * y) + v * z;
                }
                p
            }
            Implicit::Sphere { frame, radius } => {
                at(analytic::Sphere { frame, radius }.derivs((u, v), 0))
            }
            Implicit::Torus {
                frame,
                major,
                minor,
            } => at(analytic::Torus {
                frame,
                major,
                minor,
            }
            .derivs((u, v), 0)),
        }
    }

    /// The closed-form point at NURB parameters `(t_u, t_v)` under `layout` (D13).
    pub fn point_at(&self, layout: &Layout, t: (f64, f64)) -> V3 {
        self.point(layout.to_step(t))
    }
}

fn first_order(f: &Rat, g: &[Rat; 3]) -> f64 {
    let g2 = dot(g, g).to_f64();
    if g2 == 0.0 {
        return if f.is_zero() { 0.0 } else { f64::INFINITY };
    }
    f.abs().to_f64() / g2.sqrt()
}

/// `n` distinct doubles strictly inside each span of the domain, all spans together.
fn grid(knots: &[f64], (lo, hi): (f64, f64), n: usize) -> Result<Vec<f64>, Error> {
    let mut breaks: Vec<f64> = knots
        .iter()
        .copied()
        .filter(|&k| lo <= k && k <= hi)
        .collect();
    breaks.dedup();
    let mut out = Vec::new();
    for w in breaks.windows(2) {
        let [a, b] = [w.first().copied(), w.get(1).copied()].map(|x| x.unwrap_or(0.0));
        let span: Vec<f64> = (1..=n)
            .map(|i| a + (b - a) * (i as f64 / (n as f64 + 1.0)))
            .collect();
        let inside = span.first().is_some_and(|&t| t > a) && span.last().is_some_and(|&t| t < b);
        if !inside || span.windows(2).any(|w| matches!(w, [x, y] if x >= y)) {
            return Err(Error::NarrowSpan);
        }
        out.extend(span);
    }
    Ok(out)
}

// ---------------------------------------------------------------- the uv map (D13)

/// One direction of a NURB's parameterisation of an analytic kind: how the NURB parameter
/// `t` maps to the STEP parameter (an angle in radians, or a length).
#[derive(Clone, Debug, PartialEq)]
pub enum Param {
    /// `step = s0 + (t - t0) (s1 - s0) / (t1 - t0)`.
    Linear {
        /// `(t0, t1)`.
        nurb: (f64, f64),
        /// `(s0, s1)`.
        step: (f64, f64),
    },
    /// Equal standard-form rational quadratic arcs sweeping `sweep` radians from `start`,
    /// one per span between consecutive `breaks` (increasing NURB parameters, the arc
    /// count plus one). A negative sweep runs against the angle.
    Arcs {
        /// The span ends.
        breaks: Vec<f64>,
        /// The angle at the first break, radians.
        start: Dd,
        /// The signed angle swept, radians.
        sweep: Dd,
    },
}

/// Which NURB direction carries which STEP parameter.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Order {
    /// The NURB's `u` is STEP's `u`.
    Same,
    /// The NURB's `u` is STEP's `v` (the pin's `swap_uv`).
    Swapped,
}

/// A NURB's parameterisation of an analytic kind, both directions.
#[derive(Clone, Debug, PartialEq)]
pub struct Layout {
    /// The map of the NURB's `u`.
    pub u: Param,
    /// The map of the NURB's `v`.
    pub v: Param,
    /// Which STEP parameter each carries.
    pub order: Order,
}

impl Layout {
    /// STEP's `(u, v)` at NURB parameters `(t_u, t_v)`.
    pub fn to_step(&self, (tu, tv): (f64, f64)) -> (Dd, Dd) {
        let (a, b) = (self.u.to_step(tu), self.v.to_step(tv));
        match self.order {
            Order::Same => (a, b),
            Order::Swapped => (b, a),
        }
    }

    /// The NURB parameters `(t_u, t_v)` at STEP's `(u, v)`.
    pub fn to_nurb(&self, (su, sv): (Dd, Dd)) -> (Dd, Dd) {
        let (a, b) = match self.order {
            Order::Same => (su, sv),
            Order::Swapped => (sv, su),
        };
        (self.u.to_nurb(a), self.v.to_nurb(b))
    }
}

/// `tan(x)`.
fn tan(x: Dd) -> Dd {
    let (s, c) = dd::sin_cos(x);
    s / c
}

impl Param {
    /// The arcs the pin makes for a sweep (`conic_data`): one below 91 degrees, two below
    /// 181, three below 271, else four.
    pub fn arc_count(sweep: Dd) -> usize {
        let deg = (sweep.abs() * 180.0 / dd::consts().pi).hi;
        match deg {
            d if d < 91.0 => 1,
            d if d < 181.0 => 2,
            d if d < 271.0 => 3,
            _ => 4,
        }
    }

    /// [`Param::Arcs`] over `[t0, t1]` with [`Param::arc_count`] equal spans, the breaks
    /// `t0 + (t1 - t0) i / n` rounded.
    pub fn arcs(nurb: (f64, f64), start: Dd, sweep: Dd) -> Param {
        let n = Param::arc_count(sweep);
        let (t0, t1) = nurb;
        let breaks = (0..=n)
            .map(|i| match i {
                0 => t0,
                i if i == n => t1,
                i => t0 + (t1 - t0) * (i as f64 / n as f64),
            })
            .collect();
        Param::Arcs {
            breaks,
            start,
            sweep,
        }
    }

    /// The STEP parameter at NURB parameter `t` (past the ends, the end span's formula).
    pub fn to_step(&self, t: f64) -> Dd {
        match self {
            Param::Linear { nurb, step } => {
                let f = Dd::diff(t, nurb.0) / Dd::diff(nurb.1, nurb.0);
                Dd::new(step.0) + f * Dd::diff(step.1, step.0)
            }
            Param::Arcs {
                breaks,
                start,
                sweep,
            } => {
                let n = breaks.len().saturating_sub(1).max(1);
                let i = breaks.iter().take(n).rposition(|&b| b <= t).unwrap_or(0);
                let (b0, b1) = (
                    breaks.get(i).copied().unwrap_or(0.0),
                    breaks.get(i + 1).copied().unwrap_or(1.0),
                );
                let s = Dd::diff(t, b0) / Dd::diff(b1, b0);
                let arc = *sweep / n as f64;
                let h = arc * 0.5;
                let local = h + dd::atan((s * 2.0 - Dd::new(1.0)) * tan(h * 0.5)) * 2.0;
                *start + arc * i as f64 + local
            }
        }
    }

    /// The NURB parameter at STEP parameter `x` (an angle past the sweep: the end span's
    /// formula, while it is defined).
    pub fn to_nurb(&self, x: Dd) -> Dd {
        match self {
            Param::Linear { nurb, step } => {
                let f = (x - Dd::new(step.0)) / Dd::diff(step.1, step.0);
                Dd::new(nurb.0) + f * Dd::diff(nurb.1, nurb.0)
            }
            Param::Arcs {
                breaks,
                start,
                sweep,
            } => {
                let n = breaks.len().saturating_sub(1).max(1);
                let arc = *sweep / n as f64;
                let rel = x - *start;
                let k = (rel / arc).hi.floor();
                let i = if k < 0.0 { 0 } else { (k as usize).min(n - 1) };
                let (b0, b1) = (
                    breaks.get(i).copied().unwrap_or(0.0),
                    breaks.get(i + 1).copied().unwrap_or(1.0),
                );
                let h = arc * 0.5;
                let local = rel - arc * i as f64;
                let s = (Dd::new(1.0) + tan((local - h) * 0.5) / tan(h * 0.5)) * 0.5;
                Dd::new(b0) + s * Dd::diff(b1, b0)
            }
        }
    }
}
