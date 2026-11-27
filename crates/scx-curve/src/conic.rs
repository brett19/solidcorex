// Derived from solidcore `solidcore/crates/solidcore/src/curve/ellipse/p1.rs` and
// `solidcore/crates/solidcore/src/curve/ellipse/p2.rs` at commit
// 03e16c9ee766de5b9b1ced845e5149d62ff493b8.
// Upstream record (verbatim from those files): Upstream: `source/SMLib/src/SmEllipse.cpp`,
// `source/SMLib/src/SmCircle.cpp`, `source/SMLib/inc/SmCircle.h`. Upstream:
// `source/SMLib/src/SmEllipse.cpp`, `source/SMLib/inc/SmEllipse.h`.
// Changes: the analytic circle and ellipse only, as plain data (63 X3, X4): a `Frame`, the
// radii as `Length` and a `Sense` replace `SmEllipse`'s placement, STEP domain, polar
// conversion table and NURB twin. The angle is in radians (63 X11) from the frame's x axis
// (the pin's STEP parameter in degrees, 63 D13), period `2 pi`, over every `t`: the pin's
// fold into `[0, 360]` and its domain clamp are gone (an arc is a restriction, CU3). A
// mirrored conic is `Sense::Reversed` (63 D14) where the pin flipped its y axis. The pin's
// circle rule (`create_canonical_circle` refuses a radius not above `SM_EFF_ZERO`) holds for
// both radii of an ellipse too, where `Ellipse::new` built any; non-finite data is refused.
// `evaluate_step` (three derivatives at most, `cos`/`sin` of the degree angle) becomes
// `point` and the fixed-order `jet::<D>` of any order, one scalar-generic evaluator
// (`point_over`, `jet_over`) on `fmath::sin_cos`. The point keeps the pin's sum order
// (`o + a cos x + b sin y`). The 2-D twins (`Circle2`, `Ellipse2`) are new, where the pin
// used a 3-D conic of dimension 2.

use scx_num::domain::ParamDomain;
use scx_num::frame::{Frame, Frame2, Sense};
use scx_num::jet::{CurveJet, CurveJet2};
use scx_num::math::EFF_ZERO;
use scx_num::trig::Trig;
use scx_num::units::Length;
use scx_num::vector2d::{Point2, Vec2};
use scx_num::vector3d::{Point3, Vec3};
use scx_nurbs::Approach;

use crate::error::{CurveError, Degenerate, Part, SemiAxis};

/// The circle `centre + r (cos t x + sin t y)` in a [`Frame`], `t` the angle in radians
/// from x, period `2 pi`. [`Sense::Reversed`] runs it the other way about z
/// (`centre + r (cos t x - sin t y)`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Circle {
    frame: Frame,
    radius: Length,
    sense: Sense,
}

/// The ellipse `centre + a cos t x + b sin t y` in a [`Frame`], `a` along x and `b` along
/// y, `t` in radians, period `2 pi`. [`Sense::Reversed`] negates the `y` term.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Ellipse {
    frame: Frame,
    semi_x: Length,
    semi_y: Length,
    sense: Sense,
}

/// The circle `centre + r (cos t x + sin t y)` in the plane, in a [`Frame2`]
/// (counter-clockwise when [`Sense::Forward`]).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Circle2 {
    frame: Frame2,
    radius: Length,
    sense: Sense,
}

/// The ellipse `centre + a cos t x + b sin t y` in the plane, in a [`Frame2`].
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Ellipse2 {
    frame: Frame2,
    semi_x: Length,
    semi_y: Length,
    sense: Sense,
}

impl Circle {
    /// The circle of `radius` in `frame`, run in `sense` about its z.
    ///
    /// # Errors
    ///
    /// [`CurveError::NotFinite`] for an infinite or NaN radius; [`Degenerate::Radius`] for
    /// one not above [`EFF_ZERO`](scx_num::math::EFF_ZERO).
    pub fn new(frame: Frame, radius: Length, sense: Sense) -> crate::Result<Self> {
        radius_ok(radius)?;
        Ok(Self {
            frame,
            radius,
            sense,
        })
    }

    /// The centre and axes.
    #[must_use]
    pub const fn frame(&self) -> &Frame {
        &self.frame
    }

    /// The radius.
    #[must_use]
    pub const fn radius(&self) -> Length {
        self.radius
    }

    /// Which way it runs about its frame's z.
    #[must_use]
    pub const fn sense(&self) -> Sense {
        self.sense
    }

    /// A turn: period `2 pi` from `0`.
    #[must_use]
    pub const fn domain(&self) -> ParamDomain {
        ParamDomain::TURN
    }

    /// The point at angle `t`.
    #[must_use]
    pub fn point(&self, t: f64) -> Point3 {
        self.point_over(t)
    }

    /// The point and the first `D` derivatives at angle `t`. A circle is smooth, so `side`
    /// changes nothing.
    #[must_use]
    pub fn jet<const D: usize>(&self, t: f64, side: Approach) -> CurveJet<D> {
        let _ = side;
        self.jet_over(t)
    }

    /// The point at angle `t` in the scalar `T` (an enclosure in
    /// [`Ivl`](scx_num::interval::Ivl)).
    #[must_use]
    pub fn point_over<T: Trig>(&self, t: T) -> Point3<T> {
        let r = self.radius.mm();
        point3(&self.frame, r, r * self.sense.sign(), t)
    }

    /// The point and the first `D` derivatives at angle `t` in the scalar `T`.
    #[must_use]
    pub fn jet_over<T: Trig, const D: usize>(&self, t: T) -> CurveJet<D, T> {
        let r = self.radius.mm();
        jet3(&self.frame, r, r * self.sense.sign(), t)
    }
}

impl Ellipse {
    /// The ellipse in `frame` with semi-axis `semi_x` along its x and `semi_y` along its y,
    /// run in `sense` about its z. Either may be the longer.
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

    /// The centre and axes.
    #[must_use]
    pub const fn frame(&self) -> &Frame {
        &self.frame
    }

    /// The semi-axis along the frame's x.
    #[must_use]
    pub const fn semi_x(&self) -> Length {
        self.semi_x
    }

    /// The semi-axis along the frame's y.
    #[must_use]
    pub const fn semi_y(&self) -> Length {
        self.semi_y
    }

    /// Which way it runs about its frame's z.
    #[must_use]
    pub const fn sense(&self) -> Sense {
        self.sense
    }

    /// A turn: period `2 pi` from `0`.
    #[must_use]
    pub const fn domain(&self) -> ParamDomain {
        ParamDomain::TURN
    }

    /// The point at angle `t`.
    #[must_use]
    pub fn point(&self, t: f64) -> Point3 {
        self.point_over(t)
    }

    /// The point and the first `D` derivatives at angle `t`; `side` changes nothing.
    #[must_use]
    pub fn jet<const D: usize>(&self, t: f64, side: Approach) -> CurveJet<D> {
        let _ = side;
        self.jet_over(t)
    }

    /// The point at angle `t` in the scalar `T`.
    #[must_use]
    pub fn point_over<T: Trig>(&self, t: T) -> Point3<T> {
        point3(
            &self.frame,
            self.semi_x.mm(),
            self.semi_y.mm() * self.sense.sign(),
            t,
        )
    }

    /// The point and the first `D` derivatives at angle `t` in the scalar `T`.
    #[must_use]
    pub fn jet_over<T: Trig, const D: usize>(&self, t: T) -> CurveJet<D, T> {
        jet3(
            &self.frame,
            self.semi_x.mm(),
            self.semi_y.mm() * self.sense.sign(),
            t,
        )
    }
}

impl Circle2 {
    /// The circle of `radius` in `frame`, counter-clockwise when `sense` is forward.
    ///
    /// # Errors
    ///
    /// As [`Circle::new`].
    pub fn new(frame: Frame2, radius: Length, sense: Sense) -> crate::Result<Self> {
        radius_ok(radius)?;
        Ok(Self {
            frame,
            radius,
            sense,
        })
    }

    /// The centre and axes.
    #[must_use]
    pub const fn frame(&self) -> &Frame2 {
        &self.frame
    }

    /// The radius.
    #[must_use]
    pub const fn radius(&self) -> Length {
        self.radius
    }

    /// Counter-clockwise ([`Sense::Forward`]) or clockwise.
    #[must_use]
    pub const fn sense(&self) -> Sense {
        self.sense
    }

    /// A turn: period `2 pi` from `0`.
    #[must_use]
    pub const fn domain(&self) -> ParamDomain {
        ParamDomain::TURN
    }

    /// The point at angle `t`.
    #[must_use]
    pub fn point(&self, t: f64) -> Point2 {
        self.point_over(t)
    }

    /// The point and the first `D` derivatives at angle `t`; `side` changes nothing.
    #[must_use]
    pub fn jet<const D: usize>(&self, t: f64, side: Approach) -> CurveJet2<D> {
        let _ = side;
        self.jet_over(t)
    }

    /// The point at angle `t` in the scalar `T`.
    #[must_use]
    pub fn point_over<T: Trig>(&self, t: T) -> Point2<T> {
        let r = self.radius.mm();
        point2(&self.frame, r, r * self.sense.sign(), t)
    }

    /// The point and the first `D` derivatives at angle `t` in the scalar `T`.
    #[must_use]
    pub fn jet_over<T: Trig, const D: usize>(&self, t: T) -> CurveJet2<D, T> {
        let r = self.radius.mm();
        jet2(&self.frame, r, r * self.sense.sign(), t)
    }
}

impl Ellipse2 {
    /// The ellipse in `frame` with semi-axes `semi_x` and `semi_y`, counter-clockwise when
    /// `sense` is forward.
    ///
    /// # Errors
    ///
    /// As [`Ellipse::new`].
    pub fn new(
        frame: Frame2,
        semi_x: Length,
        semi_y: Length,
        sense: Sense,
    ) -> crate::Result<Self> {
        semi_axis_ok(SemiAxis::X, semi_x)?;
        semi_axis_ok(SemiAxis::Y, semi_y)?;
        Ok(Self {
            frame,
            semi_x,
            semi_y,
            sense,
        })
    }

    /// The centre and axes.
    #[must_use]
    pub const fn frame(&self) -> &Frame2 {
        &self.frame
    }

    /// The semi-axis along the frame's x.
    #[must_use]
    pub const fn semi_x(&self) -> Length {
        self.semi_x
    }

    /// The semi-axis along the frame's y.
    #[must_use]
    pub const fn semi_y(&self) -> Length {
        self.semi_y
    }

    /// Counter-clockwise ([`Sense::Forward`]) or clockwise.
    #[must_use]
    pub const fn sense(&self) -> Sense {
        self.sense
    }

    /// A turn: period `2 pi` from `0`.
    #[must_use]
    pub const fn domain(&self) -> ParamDomain {
        ParamDomain::TURN
    }

    /// The point at angle `t`.
    #[must_use]
    pub fn point(&self, t: f64) -> Point2 {
        self.point_over(t)
    }

    /// The point and the first `D` derivatives at angle `t`; `side` changes nothing.
    #[must_use]
    pub fn jet<const D: usize>(&self, t: f64, side: Approach) -> CurveJet2<D> {
        let _ = side;
        self.jet_over(t)
    }

    /// The point at angle `t` in the scalar `T`.
    #[must_use]
    pub fn point_over<T: Trig>(&self, t: T) -> Point2<T> {
        point2(
            &self.frame,
            self.semi_x.mm(),
            self.semi_y.mm() * self.sense.sign(),
            t,
        )
    }

    /// The point and the first `D` derivatives at angle `t` in the scalar `T`.
    #[must_use]
    pub fn jet_over<T: Trig, const D: usize>(&self, t: T) -> CurveJet2<D, T> {
        jet2(
            &self.frame,
            self.semi_x.mm(),
            self.semi_y.mm() * self.sense.sign(),
            t,
        )
    }
}

// ---------------------------------------------------------------- the one evaluator

/// The coefficients of x and y in `C^(k)` of `(a cos t, b sin t)`: the derivatives turn
/// with period four.
fn coeffs<T: Trig>(k: usize, a: T, b: T, s: T, c: T) -> (T, T) {
    match k % 4 {
        0 => (a * c, b * s),
        1 => (-(a * s), b * c),
        2 => (-(a * c), -(b * s)),
        _ => (a * s, -(b * c)),
    }
}

/// `o + a cos t x + b sin t y`, summed in the pin's order.
fn point3<T: Trig>(f: &Frame, a: f64, b: f64, t: T) -> Point3<T> {
    let (s, c) = t.sin_cos();
    let (a, b) = (T::lift(a), T::lift(b));
    let (x, y) = (lift3::<T>(f.x().as_vec()), lift3::<T>(f.y().as_vec()));
    f.origin().map(T::lift) + x * (a * c) + y * (b * s)
}

fn jet3<T: Trig, const D: usize>(f: &Frame, a: f64, b: f64, t: T) -> CurveJet<D, T> {
    let (s, c) = t.sin_cos();
    let (a, b) = (T::lift(a), T::lift(b));
    let (x, y) = (lift3::<T>(f.x().as_vec()), lift3::<T>(f.y().as_vec()));
    let (cx, cy) = coeffs(0, a, b, s, c);
    CurveJet {
        point: f.origin().map(T::lift) + x * cx + y * cy,
        d: core::array::from_fn(|k| {
            let (cx, cy) = coeffs(k + 1, a, b, s, c);
            x * cx + y * cy
        }),
    }
}

fn point2<T: Trig>(f: &Frame2, a: f64, b: f64, t: T) -> Point2<T> {
    let (s, c) = t.sin_cos();
    let (a, b) = (T::lift(a), T::lift(b));
    let (x, y) = (lift2::<T>(f.x().as_vec()), lift2::<T>(f.y().as_vec()));
    f.origin().map(T::lift) + x * (a * c) + y * (b * s)
}

fn jet2<T: Trig, const D: usize>(f: &Frame2, a: f64, b: f64, t: T) -> CurveJet2<D, T> {
    let (s, c) = t.sin_cos();
    let (a, b) = (T::lift(a), T::lift(b));
    let (x, y) = (lift2::<T>(f.x().as_vec()), lift2::<T>(f.y().as_vec()));
    let (cx, cy) = coeffs(0, a, b, s, c);
    CurveJet2 {
        point: f.origin().map(T::lift) + x * cx + y * cy,
        d: core::array::from_fn(|k| {
            let (cx, cy) = coeffs(k + 1, a, b, s, c);
            x * cx + y * cy
        }),
    }
}

fn lift3<T: Trig>(v: Vec3) -> Vec3<T> {
    v.map(T::lift)
}

fn lift2<T: Trig>(v: Vec2) -> Vec2<T> {
    v.map(T::lift)
}

// ---------------------------------------------------------------- validation

/// The pin's circle rule (`RadiusNotPositive`: not above `SM_EFF_ZERO`).
fn radius_ok(r: Length) -> crate::Result<()> {
    let r = r.mm();
    if !r.is_finite() {
        return Err(CurveError::NotFinite(Part::Radius));
    }
    if r <= EFF_ZERO {
        return Err(CurveError::Degenerate(Degenerate::Radius { radius: r }));
    }
    Ok(())
}

/// The circle rule, for each semi-axis.
fn semi_axis_ok(axis: SemiAxis, len: Length) -> crate::Result<()> {
    let l = len.mm();
    if !l.is_finite() {
        return Err(CurveError::NotFinite(Part::SemiAxis(axis)));
    }
    if l <= EFF_ZERO {
        return Err(CurveError::Degenerate(Degenerate::SemiAxis { axis, length: l }));
    }
    Ok(())
}
