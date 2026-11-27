// Derived from solidcore `solidcore/crates/solidcore/src/curve/parabola.rs` and
// `solidcore/crates/solidcore/src/curve/hyperbola.rs` at commit
// 03e16c9ee766de5b9b1ced845e5149d62ff493b8.
// Upstream record (verbatim from those files): Upstream: `source/SMLib/inc/SmParabola.h`,
// `source/SMLib/src/SmParabola.cpp`. Upstream: `source/SMLib/src/SmHyperbola.cpp`,
// `source/SMLib/inc/SmHyperbola.h`.
// Changes: the analytic parabola and hyperbola only, as plain data (63 X3, X4, §3.12): a
// `Frame`, the sizes as `Length` and a `Sense` replace the pin's placement, analytic domain
// and NURB twin. The parametrisations are the pin's (STEP's): `O + F s^2 X + 2 F s Y` and
// one branch `O + a cosh t X + b sinh t Y`, over every parameter (the pin clamped `t` to its
// analytic domain; an arc is a restriction, CU5). A mirrored curve is `Sense::Reversed`
// (the y term negated), where the pin flipped its y axis. The pin built any size; here a
// size not above `SM_EFF_ZERO` and non-finite data are refused, as for the circle.
// `evaluate_step` (three derivatives at most) becomes `point` and the fixed-order `jet::<D>`
// of any order on one scalar-generic evaluator; the parabola's `x` term is `F (s s)` (the
// pin's `(F s) s`: an equally exact rounding, and the interval form is tight), the
// hyperbola's sum keeps the pin's order on `fmath::sinh_cosh`. The 2-D twins are new.
// `step_inversion` (the parameter of a point on the curve) is not the nearest point; the
// true foot is in `invert.rs`.

//! The open conics: [`Parabola`] and [`Hyperbola`], with their 2-D twins [`Parabola2`] and
//! [`Hyperbola2`] (research 63 §3.12).
//!
//! Both are over every `t` ([`ParamDomain::Unbounded`]): an arc of one is an edge's
//! interval on it, as for a line. Evaluation is total over `f64` (an infinite parameter
//! gives infinite or NaN coordinates; a hyperbola's overflows past `|t| ~ 710`).

use scx_num::domain::ParamDomain;
use scx_num::fmath;
use scx_num::frame::{Frame, Frame2, Sense};
use scx_num::jet::{CurveJet, CurveJet2};
use scx_num::math::{Affine, EFF_ZERO};
use scx_num::trig::Trig;
use scx_num::units::Length;
use scx_num::vector2d::Point2;
use scx_num::vector3d::{Point3, Vec3};
use scx_nurbs::Approach;

use crate::conic::SIMILAR;
use crate::edit::ParamShift;
use crate::error::{CurveError, Degenerate, Part, SemiAxis};

/// The parabola `vertex + F t^2 x + 2 F t y` in a [`Frame`] (its vertex the origin, its
/// axis x, opening towards `+x`), `F` the focal distance: the focus is at `vertex + F x`.
/// [`Sense::Reversed`] negates the `y` term.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Parabola {
    frame: Frame,
    focal: Length,
    sense: Sense,
}

/// The branch `centre + a cosh t x + b sinh t y` of a hyperbola in a [`Frame`] (the branch
/// about `+x`), `a` the semi-axis along x and `b` the imaginary semi-axis along y.
/// [`Sense::Reversed`] negates the `y` term.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Hyperbola {
    frame: Frame,
    semi_x: Length,
    semi_y: Length,
    sense: Sense,
}

/// The parabola `vertex + F t^2 x + 2 F t y` in the plane, in a [`Frame2`].
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Parabola2 {
    frame: Frame2,
    focal: Length,
    sense: Sense,
}

/// The branch `centre + a cosh t x + b sinh t y` in the plane, in a [`Frame2`].
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Hyperbola2 {
    frame: Frame2,
    semi_x: Length,
    semi_y: Length,
    sense: Sense,
}

/// The accessors and evaluation every open conic shares: `$frame` is its frame type,
/// `$p` and `$j` its point and jet, `$eval`, `$jet` and `$fill` the evaluators it hands its
/// sizes (`a`, signed `b`).
macro_rules! common {
    ($ty:ident, $frame:ty, $p:ident, $j:ident, $eval:ident, $jet:ident, $fill:ident) => {
        impl $ty {
            /// The origin and axes.
            #[must_use]
            pub const fn frame(&self) -> &$frame {
                &self.frame
            }

            /// Which way it runs: [`Sense::Reversed`] negates the `y` term.
            #[must_use]
            pub const fn sense(&self) -> Sense {
                self.sense
            }

            /// Every `t`.
            #[must_use]
            pub const fn domain(&self) -> ParamDomain {
                ParamDomain::Unbounded
            }

            /// The point at `t`.
            #[must_use]
            pub fn point(&self, t: f64) -> $p {
                self.point_over(t)
            }

            /// The point and the first `D` derivatives at `t`. The curve is smooth, so
            /// `side` changes nothing.
            #[must_use]
            pub fn jet<const D: usize>(&self, t: f64, side: Approach) -> $j<D> {
                let _ = side;
                self.jet_over(t)
            }

            /// The point at `t` in the scalar `T` (an enclosure in
            /// [`Ivl`](scx_num::interval::Ivl)).
            #[must_use]
            pub fn point_over<T: Trig>(&self, t: T) -> $p<T> {
                let (a, b) = self.sizes();
                $eval(&self.frame, a, b, t)
            }

            /// The point and the first `D` derivatives at `t` in the scalar `T`.
            #[must_use]
            pub fn jet_over<T: Trig, const D: usize>(&self, t: T) -> $j<D, T> {
                let (a, b) = self.sizes();
                $jet(&self.frame, a, b, t)
            }

            /// [`Self::point`] at each of `ts`, into `out`, zipped (`Curve::eval_many`
            /// checks the lengths); each point has `point`'s bits.
            pub(crate) fn fill(&self, ts: &[f64], out: &mut [$p]) {
                let (a, b) = self.sizes();
                $fill(&self.frame, a, b, ts, out);
            }

            /// The same curve run the other way, `C'(t) = C(-t)`: the sense reversed, the
            /// rest kept. Exact: `C'(t)` has `C(-t)`'s bits (`t^2` and `cosh` are even and
            /// `sinh` odd to the bit).
            #[must_use]
            pub fn reversed(&self) -> Self {
                Self {
                    sense: self.sense.reversed(),
                    ..*self
                }
            }
        }
    };
}

common!(
    Parabola, Frame, Point3, CurveJet, para3, para_jet3, para_fill3
);
common!(
    Parabola2, Frame2, Point2, CurveJet2, para2, para_jet2, para_fill2
);
common!(
    Hyperbola, Frame, Point3, CurveJet, hyp3, hyp_jet3, hyp_fill3
);
common!(
    Hyperbola2, Frame2, Point2, CurveJet2, hyp2, hyp_jet2, hyp_fill2
);

impl Parabola {
    /// The parabola in `frame` (its vertex the origin, opening along x) of focal distance
    /// `focal`, its `y` term signed by `sense`.
    ///
    /// # Errors
    ///
    /// [`CurveError::NotFinite`] for an infinite or NaN focal distance;
    /// [`Degenerate::Focal`] for one not above [`EFF_ZERO`](scx_num::math::EFF_ZERO).
    pub fn new(frame: Frame, focal: Length, sense: Sense) -> crate::Result<Self> {
        focal_ok(focal)?;
        Ok(Self {
            frame,
            focal,
            sense,
        })
    }

    /// The focal distance `F`.
    #[must_use]
    pub const fn focal(&self) -> Length {
        self.focal
    }

    /// `(F, 2 F)`, the sense's sign on the second: the coefficients of `t^2 x` and `t y`.
    fn sizes(&self) -> (f64, f64) {
        let f = self.focal.mm();
        (f, 2.0 * f * self.sense.sign())
    }

    /// The image under `a`, and its parameter's map: with `(c, s)` the result,
    /// `c.point(s.apply(t))` is `a(self.point(t))` up to rounding.
    ///
    /// The image of `o + t^2 U + 2 t V` (`U = F A x`, `V = F A y` signed) is a parabola.
    /// Under a similarity on its plane (as [`Circle::transformed`](crate::Circle)'s test)
    /// `U` and `V` are square and of one length: the frame takes `A x`, the focal distance
    /// `|U|`, and the map is the identity. Otherwise the vertex moves to `t = mu`,
    /// `mu = -U.V / |U|^2`, where the tangent `W = V + mu U` is square to the axis `U`; with
    /// `lambda = |W| / |U|` the image is `o' + F' s^2 x' + 2 F' s y'` at
    /// `s = (t - mu) / lambda`, `F' = |W|^2 / |U|`. The frame's z is the right-handed
    /// normal of `A x` and `A y` and the sense the side of `W` about it, as for the
    /// ellipse.
    ///
    /// # Errors
    ///
    /// [`CurveError::NotFinite`] for a map that is not finite or an image that overflows;
    /// [`Degenerate::Focal`] or [`Degenerate::Plane`] where `a` flattens the parabola.
    pub fn transformed(&self, a: &Affine) -> crate::Result<(Self, ParamShift)> {
        let (origin, ax, ay) = map_frame(&self.frame, a)?;
        let (f, b) = self.sizes();
        let (u, v) = (ax * f, ay * (0.5 * b));
        let (p, q, r) = (ax.dot(ax), ay.dot(ay), ax.dot(ay));
        let similar = (p - q).abs() <= SIMILAR * (p + q) && r.abs() <= SIMILAR * (p + q);
        let lu = u.length();
        focal_ok(Length::from_mm(lu))?;
        let (o, w, focal, shift) = if similar {
            (origin, v, lu, ParamShift::NONE)
        } else {
            let mu = -u.dot(v) / (lu * lu);
            let w = v + u * mu;
            let lw = w.length();
            let o = origin + u * (mu * mu) + v * (2.0 * mu);
            (o, w, lw * (lw / lu), ParamShift::affine(-mu, lw / lu))
        };
        if !o.is_finite() {
            return Err(CurveError::NotFinite(Part::Origin));
        }
        let focal = Length::from_mm(focal);
        focal_ok(focal)?;
        let frame =
            Frame::new(o, ax.cross(ay), u).ok_or(CurveError::Degenerate(Degenerate::Plane))?;
        Ok((Self::new(frame, focal, side(w, &frame))?, shift))
    }
}

impl Hyperbola {
    /// The branch in `frame` (about its x) with semi-axis `semi_x` along x and imaginary
    /// semi-axis `semi_y` along y, its `y` term signed by `sense`. Either may be the longer.
    ///
    /// # Errors
    ///
    /// [`CurveError::NotFinite`] for an infinite or NaN semi-axis; [`Degenerate::SemiAxis`]
    /// for one not above [`EFF_ZERO`](scx_num::math::EFF_ZERO).
    pub fn new(frame: Frame, semi_x: Length, semi_y: Length, sense: Sense) -> crate::Result<Self> {
        semi_axis_ok(SemiAxis::X, semi_x)?;
        semi_axis_ok(SemiAxis::Y, semi_y)?;
        Ok(Self {
            frame,
            semi_x,
            semi_y,
            sense,
        })
    }

    /// The semi-axis along x: the vertex is at `centre + a x`.
    #[must_use]
    pub const fn semi_x(&self) -> Length {
        self.semi_x
    }

    /// The imaginary semi-axis along y: the asymptotes run along `a x +- b y`.
    #[must_use]
    pub const fn semi_y(&self) -> Length {
        self.semi_y
    }

    fn sizes(&self) -> (f64, f64) {
        (self.semi_x.mm(), self.semi_y.mm() * self.sense.sign())
    }

    /// The image under `a`, and its parameter's shift: with `(c, s)` the result,
    /// `c.point(s.apply(t))` is `a(self.point(t))` up to rounding.
    ///
    /// The image of `o + U cosh t + V sinh t` (`U = a A x`, `V = b A y`) is a branch. Under
    /// a similarity on its plane `U` and `V` stay square and the shift is zero. Otherwise its
    /// principal axes are at `t = tau`, `tanh 2 tau = -2 U.V / (|U|^2 + |V|^2)` (below one
    /// in size unless `A` flattens the plane), and `U cosh t + V sinh t =
    /// U' cosh (t - tau) + V' sinh (t - tau)` with `U' = U cosh tau + V sinh tau` and
    /// `V' = U sinh tau + V cosh tau`, square. The frame and sense are placed as for the
    /// ellipse.
    ///
    /// # Errors
    ///
    /// [`CurveError::NotFinite`] for a map that is not finite or an image that overflows;
    /// a degenerate semi-axis or [`Degenerate::Plane`] where `a` flattens the branch.
    pub fn transformed(&self, a: &Affine) -> crate::Result<(Self, ParamShift)> {
        let (origin, ax, ay) = map_frame(&self.frame, a)?;
        let (sa, sb) = self.sizes();
        let (u, v) = (ax * sa, ay * sb);
        let (p, q, r) = (ax.dot(ax), ay.dot(ay), ax.dot(ay));
        let similar = (p - q).abs() <= SIMILAR * (p + q) && r.abs() <= SIMILAR * (p + q);
        let (x, y, tau) = if similar {
            (u, v, 0.0)
        } else {
            let ratio = -2.0 * u.dot(v) / (u.dot(u) + v.dot(v));
            let tau = 0.5 * atanh(ratio);
            let (sh, ch) = fmath::sinh_cosh(tau);
            (u * ch + v * sh, u * sh + v * ch, tau)
        };
        let (lx, ly) = (x.length(), y.length());
        semi_axis_ok(SemiAxis::X, Length::from_mm(lx))?;
        semi_axis_ok(SemiAxis::Y, Length::from_mm(ly))?;
        let frame =
            Frame::new(origin, ax.cross(ay), x).ok_or(CurveError::Degenerate(Degenerate::Plane))?;
        let (lx, ly) = (Length::from_mm(lx), Length::from_mm(ly));
        Ok((
            Self::new(frame, lx, ly, side(y, &frame))?,
            ParamShift::new(-tau),
        ))
    }
}

impl Parabola2 {
    /// The parabola in `frame` of focal distance `focal`, its `y` term signed by `sense`.
    ///
    /// # Errors
    ///
    /// As [`Parabola::new`].
    pub fn new(frame: Frame2, focal: Length, sense: Sense) -> crate::Result<Self> {
        focal_ok(focal)?;
        Ok(Self {
            frame,
            focal,
            sense,
        })
    }

    /// The focal distance `F`.
    #[must_use]
    pub const fn focal(&self) -> Length {
        self.focal
    }

    fn sizes(&self) -> (f64, f64) {
        let f = self.focal.mm();
        (f, 2.0 * f * self.sense.sign())
    }
}

impl Hyperbola2 {
    /// The branch in `frame` with semi-axes `semi_x` and `semi_y`, its `y` term signed by
    /// `sense`.
    ///
    /// # Errors
    ///
    /// As [`Hyperbola::new`].
    pub fn new(frame: Frame2, semi_x: Length, semi_y: Length, sense: Sense) -> crate::Result<Self> {
        semi_axis_ok(SemiAxis::X, semi_x)?;
        semi_axis_ok(SemiAxis::Y, semi_y)?;
        Ok(Self {
            frame,
            semi_x,
            semi_y,
            sense,
        })
    }

    /// The semi-axis along x.
    #[must_use]
    pub const fn semi_x(&self) -> Length {
        self.semi_x
    }

    /// The imaginary semi-axis along y.
    #[must_use]
    pub const fn semi_y(&self) -> Length {
        self.semi_y
    }

    fn sizes(&self) -> (f64, f64) {
        (self.semi_x.mm(), self.semi_y.mm() * self.sense.sign())
    }
}

// ---------------------------------------------------------------- the evaluators

/// The coefficients of x and y in `C^(k)` of `(a t^2, b t)`.
fn para_coeffs<T: Trig>(k: usize, a: T, b: T, t: T) -> (T, T) {
    let zero = T::lift(0.0);
    match k {
        0 => (a * t.sqr(), b * t),
        1 => ((a + a) * t, b),
        2 => (a + a, zero),
        _ => (zero, zero),
    }
}

/// The coefficients of x and y in `C^(k)` of `(a cosh t, b sinh t)`: they alternate.
fn hyp_coeffs<T: Trig>(k: usize, a: T, b: T, sh: T, ch: T) -> (T, T) {
    if k.is_multiple_of(2) {
        (a * ch, b * sh)
    } else {
        (a * sh, b * ch)
    }
}

macro_rules! evaluators {
    ($frame:ty, $p:ident, $j:ident, $pe:ident, $pj:ident, $pf:ident, $he:ident, $hj:ident, $hf:ident) => {
        fn $pe<T: Trig>(f: &$frame, a: f64, b: f64, t: T) -> $p<T> {
            let (x, y) = (f.x().as_vec().map(T::lift), f.y().as_vec().map(T::lift));
            let (cx, cy) = para_coeffs(0, T::lift(a), T::lift(b), t);
            f.origin().map(T::lift) + x * cx + y * cy
        }

        fn $pj<T: Trig, const D: usize>(f: &$frame, a: f64, b: f64, t: T) -> $j<D, T> {
            let (x, y) = (f.x().as_vec().map(T::lift), f.y().as_vec().map(T::lift));
            let (a, b) = (T::lift(a), T::lift(b));
            let (cx, cy) = para_coeffs(0, a, b, t);
            $j {
                point: f.origin().map(T::lift) + x * cx + y * cy,
                d: core::array::from_fn(|k| {
                    let (cx, cy) = para_coeffs(k + 1, a, b, t);
                    x * cx + y * cy
                }),
            }
        }

        fn $pf(f: &$frame, a: f64, b: f64, ts: &[f64], out: &mut [$p]) {
            let (o, x, y) = (f.origin(), f.x().as_vec(), f.y().as_vec());
            for (p, &t) in out.iter_mut().zip(ts) {
                *p = o + x * (a * (t * t)) + y * (b * t);
            }
        }

        fn $he<T: Trig>(f: &$frame, a: f64, b: f64, t: T) -> $p<T> {
            let (sh, ch) = t.sinh_cosh();
            let (x, y) = (f.x().as_vec().map(T::lift), f.y().as_vec().map(T::lift));
            f.origin().map(T::lift) + x * (T::lift(a) * ch) + y * (T::lift(b) * sh)
        }

        fn $hj<T: Trig, const D: usize>(f: &$frame, a: f64, b: f64, t: T) -> $j<D, T> {
            let (sh, ch) = t.sinh_cosh();
            let (x, y) = (f.x().as_vec().map(T::lift), f.y().as_vec().map(T::lift));
            let (a, b) = (T::lift(a), T::lift(b));
            let (cx, cy) = hyp_coeffs(0, a, b, sh, ch);
            $j {
                point: f.origin().map(T::lift) + x * cx + y * cy,
                d: core::array::from_fn(|k| {
                    let (cx, cy) = hyp_coeffs(k + 1, a, b, sh, ch);
                    x * cx + y * cy
                }),
            }
        }

        fn $hf(f: &$frame, a: f64, b: f64, ts: &[f64], out: &mut [$p]) {
            let (o, x, y) = (f.origin(), f.x().as_vec(), f.y().as_vec());
            for (p, &t) in out.iter_mut().zip(ts) {
                let (sh, ch) = fmath::sinh_cosh(t);
                *p = o + x * (a * ch) + y * (b * sh);
            }
        }
    };
}

evaluators!(
    Frame, Point3, CurveJet, para3, para_jet3, para_fill3, hyp3, hyp_jet3, hyp_fill3
);
evaluators!(
    Frame2, Point2, CurveJet2, para2, para_jet2, para_fill2, hyp2, hyp_jet2, hyp_fill2
);

// ---------------------------------------------------------------- the affine image

/// The images of a frame's origin, x and y under `m`, finite.
fn map_frame(f: &Frame, m: &Affine) -> crate::Result<(Point3, Vec3, Vec3)> {
    if !m.is_finite() {
        return Err(CurveError::NotFinite(Part::Map));
    }
    let origin = m.apply(f.origin());
    if !origin.is_finite() {
        return Err(CurveError::NotFinite(Part::Origin));
    }
    Ok((
        origin,
        m.apply_vec(f.x().as_vec()),
        m.apply_vec(f.y().as_vec()),
    ))
}

/// The sense of an image whose `y` term is along `y`, in `frame`.
fn side(y: Vec3, frame: &Frame) -> Sense {
    if y.dot(frame.y().as_vec()) < 0.0 {
        Sense::Reversed
    } else {
        Sense::Forward
    }
}

// ---------------------------------------------------------------- inverse hyperbolics

/// `ln(1 + w)` for `w > -1`, to a few ulps near zero: `ln(1 + w) w / ((1 + w) - 1)`
/// corrects the rounding of `1 + w` (Goldberg 1991, Theorem 4).
fn ln1p(w: f64) -> f64 {
    let u = 1.0 + w;
    if u == 1.0 {
        w
    } else {
        fmath::ln(u) * w / (u - 1.0)
    }
}

/// `asinh x`, odd to the bit: `ln1p(|x| + x^2 / (1 + sqrt(1 + x^2)))`, and
/// `ln |x| + ln 2` where `x^2` would overflow.
pub(crate) fn asinh(x: f64) -> f64 {
    let a = x.abs();
    let r = if a > 1e150 {
        fmath::ln(a) + core::f64::consts::LN_2
    } else {
        ln1p(a + a * a / (1.0 + (1.0 + a * a).sqrt()))
    };
    r.copysign(x)
}

/// `acosh y` for `y >= 1`: `ln1p((y - 1) + sqrt((y - 1)(y + 1)))`, and `ln y + ln 2` where
/// that would overflow. NaN below one.
pub(crate) fn acosh(y: f64) -> f64 {
    if y.is_nan() || y < 1.0 {
        return f64::NAN;
    }
    if y > 1e150 {
        return fmath::ln(y) + core::f64::consts::LN_2;
    }
    let e = y - 1.0;
    ln1p(e + (e * (y + 1.0)).sqrt())
}

/// `atanh r` for `|r| < 1`, odd to the bit: `ln1p(2 |r| / (1 - |r|)) / 2`.
fn atanh(r: f64) -> f64 {
    let a = r.abs();
    (0.5 * ln1p(2.0 * a / (1.0 - a))).copysign(r)
}

// ---------------------------------------------------------------- validation

fn focal_ok(f: Length) -> crate::Result<()> {
    let f = f.mm();
    if !f.is_finite() {
        return Err(CurveError::NotFinite(Part::Focal));
    }
    if f <= EFF_ZERO {
        return Err(CurveError::Degenerate(Degenerate::Focal { focal: f }));
    }
    Ok(())
}

fn semi_axis_ok(axis: SemiAxis, len: Length) -> crate::Result<()> {
    let l = len.mm();
    if !l.is_finite() {
        return Err(CurveError::NotFinite(Part::SemiAxis(axis)));
    }
    if l <= EFF_ZERO {
        return Err(CurveError::Degenerate(Degenerate::SemiAxis {
            axis,
            length: l,
        }));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::truth::{U, holds, near};
    use scx_num::hash::Rng;
    use scx_num::interval::Ivl;
    use scx_num::vector2d::Vec2;
    use scx_testkit::analytic;
    use scx_testkit::dd::{self, Dd};
    use scx_testkit::sample;

    fn frame(rng: &mut Rng, scale: f64) -> Frame {
        let f = sample::frame(rng, scale);
        Frame::new(Point3::from(f.origin), Vec3::from(f.z), Vec3::from(f.x)).unwrap()
    }

    fn sense(i: usize) -> Sense {
        if i.is_multiple_of(2) {
            Sense::Forward
        } else {
            Sense::Reversed
        }
    }

    /// The testkit's frame as the curve runs: y times the sense's sign.
    fn kit(f: &Frame, s: Sense) -> analytic::Frame {
        analytic::Frame {
            origin: f.origin().to_array(),
            x: f.x().as_vec().to_array(),
            y: (f.y().as_vec() * s.sign()).to_array(),
            z: f.z().as_vec().to_array(),
        }
    }

    /// The curves of every scale and sense, each with its truth.
    struct Case {
        frame: Frame,
        parabola: Parabola,
        hyperbola: Hyperbola,
        p: analytic::Parabola,
        h: analytic::Hyperbola,
    }

    fn cases(seed: u64) -> Vec<Case> {
        let mut rng = Rng::new(seed);
        let mut out = Vec::new();
        for (scale, size) in [(1.0, 1.0), (1e3, 1e-3), (1e-3, 1e3), (0.0, 1.0)] {
            for i in 0..24 {
                let frame = frame(&mut rng, scale);
                let s = sense(i);
                let a = size * (0.25 + rng.next_f64());
                let b = if i % 3 == 0 {
                    a
                } else {
                    size * (0.25 + rng.next_f64())
                };
                let mm = Length::from_mm;
                out.push(Case {
                    frame,
                    parabola: Parabola::new(frame, mm(a), s).unwrap(),
                    hyperbola: Hyperbola::new(frame, mm(a), mm(b), s).unwrap(),
                    p: analytic::Parabola {
                        frame: kit(&frame, s),
                        focal: a,
                    },
                    h: analytic::Hyperbola {
                        frame: kit(&frame, s),
                        semi_x: a,
                        semi_y: b,
                    },
                });
            }
        }
        out
    }

    fn params(rng: &mut Rng) -> Vec<f64> {
        let mut t = vec![0.0, -0.0, 1.0, -1.0, 1e-300, 0.5, -3.0, 20.0, -40.0];
        t.extend((0..16).map(|_| 6.0 * rng.next_signed_unit()));
        t
    }

    /// The size of each coordinate's terms in `C^(k)`: what its rounding scales with.
    fn scale(f: &Frame, k: usize, i: usize, cx: f64, cy: f64) -> f64 {
        let o = if k == 0 {
            f.origin().to_array()[i].abs()
        } else {
            0.0
        };
        o + (cx * f.x().as_vec().to_array()[i]).abs() + (cy * f.y().as_vec().to_array()[i]).abs()
    }

    #[test]
    fn points_and_jets_match_the_double_double_truth() {
        let mut rng = Rng::new(61);
        let mut n = 0;
        for c in cases(1) {
            let (f, a) = (c.parabola.focal().mm(), c.hyperbola.semi_x().mm());
            let b = c.hyperbola.semi_y().mm();
            for t in params(&mut rng) {
                let j = c.parabola.jet::<4>(t, Approach::Below);
                assert_eq!(j, c.parabola.jet::<4>(t, Approach::Above));
                assert_eq!(j.point, c.parabola.point(t));
                let want = c.p.derivs(t, 4);
                // Each derivative's terms: F t^2 and 2 F t, then 2 F t and 2 F, then 2 F.
                let sizes = [
                    (f * t * t, 2.0 * f * t),
                    (2.0 * f * t, 2.0 * f),
                    (2.0 * f, 0.0),
                    (0.0, 0.0),
                    (0.0, 0.0),
                ];
                for i in 0..3 {
                    let (cx, cy) = sizes[0];
                    let s = scale(&c.frame, 0, i, cx, cy);
                    assert!(near(j.point.to_array()[i], want[0][i], s, 4.0), "{t} {i}");
                    for k in 0..4 {
                        let (cx, cy) = sizes[k + 1];
                        let s = scale(&c.frame, k + 1, i, cx, cy);
                        assert!(
                            near(j.d[k].to_array()[i], want[k + 1][i], s, 4.0),
                            "{t} {k}"
                        );
                    }
                }
                let j = c.hyperbola.jet::<5>(t, Approach::Below);
                assert_eq!(j.point, c.hyperbola.point(t));
                let want = c.h.derivs(t, 5);
                let (sh, ch) = (fmath::sinh(t).abs(), fmath::cosh(t));
                for (k, w) in want.iter().enumerate() {
                    let (cx, cy) = if k.is_multiple_of(2) {
                        (a * ch, b * sh)
                    } else {
                        (a * sh, b * ch)
                    };
                    let got = if k == 0 {
                        j.point.to_array()
                    } else {
                        j.d[k - 1].to_array()
                    };
                    for (i, (g, wv)) in got.into_iter().zip(w.iter()).enumerate() {
                        let s = scale(&c.frame, k, i, cx, cy);
                        // `sinh` and `cosh` are within an ulp or two (fmath); 8 ulps of the terms.
                        assert!(near(g, *wv, s, 8.0), "{t} {k} {i}");
                    }
                }
                n += 1;
            }
        }
        assert!(n > 2000, "{n}");
    }

    #[test]
    fn interval_evaluation_holds_the_truth() {
        let mut rng = Rng::new(62);
        for c in cases(2) {
            for t in params(&mut rng) {
                let jp = c.parabola.jet_over::<Ivl, 3>(Ivl::from(t));
                let jh = c.hyperbola.jet_over::<Ivl, 3>(Ivl::from(t));
                let (wp, wh) = (c.p.derivs(t, 3), c.h.derivs(t, 3));
                for i in 0..3 {
                    assert!(holds(jp.point.to_array()[i], wp[0][i]), "{t}");
                    assert!(holds(jh.point.to_array()[i], wh[0][i]), "{t}");
                    for k in 0..3 {
                        assert!(holds(jp.d[k].to_array()[i], wp[k + 1][i]), "{t} {k}");
                        assert!(holds(jh.d[k].to_array()[i], wh[k + 1][i]), "{t} {k}");
                    }
                }
                // A wide interval, across the vertex, holds every point over it.
                for w in [1e-6, 0.3, 2.0, 7.0] {
                    let ivl = Ivl::new(t - 0.5 * w, t + 0.5 * w);
                    let (pp, ph) = (
                        c.parabola.point_over(ivl).to_array(),
                        c.hyperbola.point_over(ivl).to_array(),
                    );
                    let span = Dd::new(ivl.hi) - Dd::new(ivl.lo);
                    for s in 0..=16 {
                        let ts = Dd::new(ivl.lo) + span * Dd::new(f64::from(s) / 16.0);
                        let (wp, wh) = (c.p.derivs(ts, 0), c.h.derivs(ts, 0));
                        for i in 0..3 {
                            assert!(holds(pp[i], wp[0][i]), "{t} {w} {s}");
                            assert!(holds(ph[i], wh[0][i]), "{t} {w} {s}");
                        }
                    }
                }
            }
        }
        // The parabola's square is tight: over [-1, 1] at the vertex, x is in [0, F].
        let p = Parabola::new(Frame::WORLD, Length::from_mm(2.0), Sense::Forward).unwrap();
        let q = p.point_over(Ivl::new(-1.0, 1.0));
        let tight = |got: f64, want: f64| (got - want).abs() <= 16.0 * U * want.abs().max(1.0);
        assert!(
            q.x.lo <= 0.0 && tight(q.x.lo, 0.0) && tight(q.x.hi, 2.0),
            "{q:?}"
        );
        assert!(tight(q.y.lo, -4.0) && tight(q.y.hi, 4.0), "{q:?}");
    }

    #[test]
    fn reversed_runs_the_other_way_to_the_bit() {
        let mut rng = Rng::new(63);
        for c in cases(3) {
            let (p, h) = (c.parabola.reversed(), c.hyperbola.reversed());
            for t in params(&mut rng) {
                assert_eq!(p.point(t), c.parabola.point(-t));
                assert_eq!(h.point(t), c.hyperbola.point(-t));
            }
            assert_eq!(p.reversed(), c.parabola);
        }
    }

    #[test]
    fn the_plane_twins_are_the_curves_at_z_zero() {
        let mut rng = Rng::new(64);
        for i in 0..40 {
            let o = Point2::new(rng.next_signed_unit(), rng.next_signed_unit());
            let x = Vec2::new(rng.next_signed_unit(), rng.next_signed_unit());
            let Some(f2) = Frame2::new(o, x) else {
                continue;
            };
            let f3 = Frame::new(
                Point3::new(o.x, o.y, 0.0),
                Vec3::new(0.0, 0.0, 1.0),
                Vec3::new(f2.x().as_vec().x, f2.x().as_vec().y, 0.0),
            )
            .unwrap();
            let (a, b) = (Length::from_mm(0.5 + rng.next_f64()), Length::from_mm(0.25));
            let s = sense(i);
            let (p2, p3) = (
                Parabola2::new(f2, a, s).unwrap(),
                Parabola::new(f3, a, s).unwrap(),
            );
            let (h2, h3) = (
                Hyperbola2::new(f2, a, b, s).unwrap(),
                Hyperbola::new(f3, a, b, s).unwrap(),
            );
            for t in params(&mut rng) {
                let (q2, q3) = (
                    p2.jet::<3>(t, Approach::Above),
                    p3.jet::<3>(t, Approach::Above),
                );
                let (r2, r3) = (
                    h2.jet::<3>(t, Approach::Above),
                    h3.jet::<3>(t, Approach::Above),
                );
                let close = |u: Vec2, v: Vec3| {
                    let m = v.length().max(1.0);
                    (u.x - v.x).abs() <= 8.0 * U * m && (u.y - v.y).abs() <= 8.0 * U * m
                };
                assert!(close(q2.point.to_vec(), q3.point.to_vec()), "{t}");
                assert!(close(r2.point.to_vec(), r3.point.to_vec()), "{t}");
                for k in 0..3 {
                    assert!(
                        close(q2.d[k], q3.d[k]) && close(r2.d[k], r3.d[k]),
                        "{t} {k}"
                    );
                }
                assert_eq!(p2.point(t), q2.point);
                assert_eq!(h2.point(t), r2.point);
            }
            assert_eq!((p2.focal(), p2.sense(), p2.frame()), (a, s, &f2));
            assert_eq!(
                (h2.semi_x(), h2.semi_y(), h2.domain()),
                (a, b, ParamDomain::Unbounded)
            );
        }
    }

    #[test]
    fn construction_refuses_what_the_pin_built() {
        let (f, mm) = (Frame::WORLD, Length::from_mm);
        let one = mm(1.0);
        assert_eq!(
            Parabola::new(f, mm(0.0), Sense::Forward),
            Err(CurveError::Degenerate(Degenerate::Focal { focal: 0.0 }))
        );
        assert_eq!(
            Parabola::new(f, mm(-1.0), Sense::Forward)
                .unwrap_err()
                .name(),
            "degenerate_focal"
        );
        assert_eq!(
            Parabola::new(f, mm(f64::NAN), Sense::Forward),
            Err(CurveError::NotFinite(Part::Focal))
        );
        assert!(Parabola::new(f, mm(EFF_ZERO.next_up()), Sense::Forward).is_ok());
        assert!(Parabola::new(f, mm(EFF_ZERO), Sense::Forward).is_err());
        assert_eq!(
            Hyperbola::new(f, one, mm(0.0), Sense::Forward),
            Err(CurveError::Degenerate(Degenerate::SemiAxis {
                axis: SemiAxis::Y,
                length: 0.0
            }))
        );
        assert_eq!(
            Hyperbola::new(f, mm(f64::INFINITY), one, Sense::Forward),
            Err(CurveError::NotFinite(Part::SemiAxis(SemiAxis::X)))
        );
        let f2 = Frame2::WORLD;
        assert!(Parabola2::new(f2, mm(0.0), Sense::Forward).is_err());
        assert!(Hyperbola2::new(f2, one, mm(f64::NAN), Sense::Forward).is_err());
        let h = Hyperbola::new(f, mm(2.0), mm(3.0), Sense::Reversed).unwrap();
        assert_eq!(
            (h.semi_x(), h.semi_y(), h.sense(), h.frame()),
            (mm(2.0), mm(3.0), Sense::Reversed, &f)
        );
        assert!(h.point(f64::NAN).x.is_nan());
        let p = Parabola::new(f, one, Sense::Forward).unwrap();
        assert_eq!(p.domain(), ParamDomain::Unbounded);
    }

    fn linear(x: [f64; 3], y: [f64; 3], z: [f64; 3]) -> Affine {
        Affine::from_axes(Vec3::from(x), Vec3::from(y), Vec3::from(z), Point3::ORIGIN)
    }

    #[test]
    fn transformed_open_conics_against_closed_forms() {
        let mm = Length::from_mm;
        for s in [Sense::Forward, Sense::Reversed] {
            let p = Parabola::new(Frame::WORLD, mm(2.0), s).unwrap();
            let h = Hyperbola::new(Frame::WORLD, mm(2.0), mm(1.0), s).unwrap();
            // A rotation and a uniform scale keep the parameter.
            let turn = Affine::rotation(Point3::new(1.0, 2.0, 3.0), Vec3::new(1.0, -2.0, 0.5), 0.7)
                .unwrap();
            let scaled = Affine::uniform_scaling(Point3::new(-1.0, 0.0, 4.0), 2.5).unwrap() * turn;
            let (q, shift) = p.transformed(&scaled).unwrap();
            assert_eq!(shift, ParamShift::NONE);
            assert!((q.focal().mm() - 5.0).abs() <= 8.0 * U * 5.0);
            assert_eq!(q.sense(), s);
            let (k, shift) = h.transformed(&scaled).unwrap();
            assert_eq!(shift, ParamShift::NONE);
            assert!((k.semi_x().mm() - 5.0).abs() <= 8.0 * U * 5.0);
            assert!((k.semi_y().mm() - 2.5).abs() <= 8.0 * U * 2.5);
            // Stretching x by 4: y^2 = 4 F x becomes y^2 = 4 (F / 4) x, at s' = 4 s.
            let stretch = linear([4.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]);
            let (q, shift) = p.transformed(&stretch).unwrap();
            assert_eq!(
                (q.focal().mm(), shift.by(), shift.stretch()),
                (0.5, 0.0, 0.25)
            );
            assert_eq!((q.frame().origin(), q.sense()), (Point3::ORIGIN, s));
            assert_eq!(shift.apply(1.5), 6.0);
            // The hyperbola's axes under the stretch stay square: no shift, a scaled.
            let (k, shift) = h.transformed(&stretch).unwrap();
            assert_eq!(
                (k.semi_x().mm(), k.semi_y().mm(), shift),
                (8.0, 1.0, ParamShift::NONE)
            );
            // A shear moves the parabola's vertex to s = mu and its axis stays x: the image of
            // (2 s^2, 4 s) under x += y is (2 s^2 + 4 s, 4 s), vertex at s = -1: (-2, -4).
            let shear = linear([1.0, 0.0, 0.0], [1.0, 1.0, 0.0], [0.0, 0.0, 1.0]);
            let (q, shift) = p.transformed(&shear).unwrap();
            let sign = s.sign();
            assert_eq!(shift.by(), sign);
            // Either sense meets the vertex at (-2, -4): s = -1 forward, s = 1 reversed.
            assert_eq!(q.frame().origin(), Point3::new(-2.0, -4.0, 0.0));
            assert_eq!(q.frame().x().as_vec(), Vec3::X);
            // W = (0, 2, 0) (times the sign) against U = (2, 0, 0): lambda 1, F' 2, so the
            // image is (2 u^2 - 2, 4 u - 4) at u = s + 1.
            assert_eq!((shift.stretch(), q.focal().mm()), (1.0, 2.0));
            for t in [-3.0, -1.0, 0.0, 0.5, 2.0] {
                let want = shear.apply(p.point(t));
                assert!(q.point(shift.apply(t)).distance(want) <= 64.0 * U * (1.0 + t * t));
            }
            // A sheared hyperbola is placed on its principal axes, shifted by tau.
            let (k, shift) = h.transformed(&shear).unwrap();
            assert_ne!(shift.by(), 0.0);
            assert_eq!(shift.stretch(), 1.0);
            let (x, y) = (k.frame().x().as_vec(), k.frame().y().as_vec());
            assert!(x.dot(y).abs() <= 4.0 * U);
            for t in [-3.0, -1.0, 0.0, 0.5, 2.0] {
                let want = shear.apply(h.point(t));
                let got = k.point(shift.apply(t));
                assert!(got.distance(want) <= 64.0 * U * fmath::cosh(t) * 4.0, "{t}");
            }
            // A map that flattens the plane is refused.
            let flat = linear([1.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 0.0, 1.0]);
            assert!(p.transformed(&flat).is_err());
            assert!(h.transformed(&flat).is_err());
        }
    }

    #[test]
    fn inverse_hyperbolics_are_accurate_and_odd() {
        let mut rng = Rng::new(65);
        let mut xs = vec![
            0.0,
            -0.0,
            1e-300,
            1e-8,
            0.5,
            1.0,
            3.0,
            1e10,
            1e200,
            f64::MAX,
        ];
        xs.extend((0..2000).map(|_| fmath::exp(40.0 * rng.next_signed_unit())));
        for x in xs {
            // asinh x = ln(x + sqrt(x^2 + 1)) in double-double where that does not overflow.
            let got = asinh(x);
            assert_eq!(asinh(-x).to_bits(), (-got).to_bits(), "{x}");
            if x < 1e150 {
                let d = Dd::new(x);
                let want = dd::ln_dd(d + (d * d + Dd::new(1.0)).sqrt());
                let want = if x < 1e-3 {
                    // Below, the truth's own `ln` loses digits: compare with the series,
                    // whose next term is below 1e-26 of x.
                    let d2 = d * d;
                    d * (Dd::new(1.0)
                        - d2 * (Dd::new(1.0) / Dd::new(6.0)
                            - d2 * (Dd::new(3.0) / Dd::new(40.0)
                                - d2 * (Dd::new(15.0) / Dd::new(336.0)))))
                } else {
                    want
                };
                assert!(near(got, want, got, 4.0), "{x} {got}");
                let (sh, _) = dd::sinh_cosh(Dd::new(got));
                assert!(near(x, sh, x, 8.0 * (1.0 + got)), "{x}");
            }
            let y = 1.0 + x;
            if y.is_finite() && y < 1e150 {
                let got = acosh(y);
                let (_, ch) = dd::sinh_cosh(Dd::new(got));
                // cosh is flat at zero: compare the cosh back, to the rounding of `got`.
                assert!(near(y, ch, y + got * fmath::sinh(got), 8.0), "{y} {got}");
            }
            let r = x / (1.0 + x);
            if r < 1.0 {
                let got = atanh(r);
                assert_eq!(atanh(-r).to_bits(), (-got).to_bits());
                let (sh, ch) = dd::sinh_cosh(Dd::new(got));
                assert!(near(r, sh / ch, r, 8.0 / (1.0 - r)), "{r} {got}");
            }
        }
        assert!(acosh(0.5).is_nan() && acosh(f64::NAN).is_nan());
        assert_eq!(acosh(1.0), 0.0);
    }
}
