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
// fold into `[0, 360]` and its domain clamp are gone (an arc is a restriction, CU5). A
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
    use crate::truth::{holds, near};
    use scx_num::hash::Rng;
    use scx_num::interval::Ivl;
    use scx_num::math::{HALF_PI, PI, TWO_PI};
    use scx_testkit::analytic;
    use scx_testkit::dd::Dd;
    use scx_testkit::sample;

    /// A conic as the evaluator sees it: frame, `a`, signed `b`.
    struct Case {
        frame: Frame,
        a: f64,
        b: f64,
        ellipse: Ellipse,
    }

    fn frame(rng: &mut Rng, scale: f64) -> Frame {
        let f = sample::frame(rng, scale);
        Frame::new(Point3::from(f.origin), Vec3::from(f.z), Vec3::from(f.x)).unwrap()
    }

    fn cases(seed: u64) -> Vec<Case> {
        let mut rng = Rng::new(seed);
        let mut out = Vec::new();
        for (scale, size) in [(1.0, 1.0), (1e3, 1e-3), (1e-3, 1e3), (0.0, 1.0)] {
            for i in 0..30 {
                let frame = frame(&mut rng, scale);
                let a = size * (0.25 + rng.next_f64());
                let b = if i % 3 == 0 {
                    a
                } else {
                    size * (0.25 + rng.next_f64())
                };
                let sense = if i % 2 == 0 {
                    Sense::Forward
                } else {
                    Sense::Reversed
                };
                let ellipse =
                    Ellipse::new(frame, Length::from_mm(a), Length::from_mm(b), sense).unwrap();
                out.push(Case {
                    frame,
                    a,
                    b: b * sense.sign(),
                    ellipse,
                });
            }
        }
        out
    }

    fn params(rng: &mut Rng) -> Vec<f64> {
        let mut t = vec![
            0.0,
            -0.0,
            HALF_PI,
            PI,
            -PI,
            TWO_PI,
            HALF_PI.next_up(),
            PI.next_down(),
            1e3,
            -1e5,
            1e-300,
        ];
        t.extend((0..16).map(|_| 4.0 * PI * rng.next_signed_unit()));
        t
    }

    fn truth(c: &Case) -> analytic::Ellipse {
        let f = &c.frame;
        analytic::Ellipse {
            frame: analytic::Frame {
                origin: f.origin().to_array(),
                x: f.x().as_vec().to_array(),
                y: f.y().as_vec().to_array(),
                z: f.z().as_vec().to_array(),
            },
            semi_x: c.a,
            semi_y: c.b,
        }
    }

    /// The size of each coordinate's terms: what its rounding errors scale with.
    fn scale(c: &Case, k: usize, i: usize) -> f64 {
        let o = if k == 0 {
            c.frame.origin().to_array()[i].abs()
        } else {
            0.0
        };
        o + (c.a * c.frame.x().as_vec().to_array()[i]).abs()
            + (c.b * c.frame.y().as_vec().to_array()[i]).abs()
    }

    #[test]
    fn points_and_jets_match_the_double_double_truth() {
        let mut rng = Rng::new(11);
        let mut n = 0;
        for c in cases(1) {
            let r = truth(&c);
            for t in params(&mut rng) {
                let j = c.ellipse.jet::<5>(t, Approach::Below);
                assert_eq!(j, c.ellipse.jet::<5>(t, Approach::Above));
                assert_eq!(j.point, c.ellipse.point(t));
                let want = r.derivs(t, 5);
                for i in 0..3 {
                    let p = j.point.to_array()[i];
                    assert!(near(p, want[0][i], scale(&c, 0, i), 6.0), "{t} {i} {p}");
                    for k in 0..5 {
                        let d = j.d[k].to_array()[i];
                        assert!(near(d, want[k + 1][i], scale(&c, k + 1, i), 6.0), "{t} {k}");
                    }
                }
                n += 1;
            }
        }
        assert!(n > 3000);
    }

    #[test]
    fn a_circle_is_the_ellipse_of_equal_radii() {
        let mut rng = Rng::new(12);
        for c in cases(2) {
            let r = Length::from_mm(c.a);
            let circle = Circle::new(c.frame, r, c.ellipse.sense()).unwrap();
            let e = Ellipse::new(c.frame, r, r, c.ellipse.sense()).unwrap();
            for t in params(&mut rng) {
                assert_eq!(
                    circle.jet::<3>(t, Approach::Below),
                    e.jet::<3>(t, Approach::Below)
                );
            }
            assert_eq!(circle.domain(), ParamDomain::TURN);
            assert_eq!(circle.radius(), r);
        }
    }

    #[test]
    fn reversed_runs_the_other_way() {
        let mut rng = Rng::new(13);
        let f = frame(&mut rng, 1.0);
        let r = Length::from_mm(2.0);
        let fwd = Circle::new(f, r, Sense::Forward).unwrap();
        let rev = Circle::new(f, r, Sense::Reversed).unwrap();
        for t in params(&mut rng) {
            let (p, q) = (rev.point(t), fwd.point(-t));
            assert!(p.distance(q) <= 1e-15, "{t}");
            // Counter-clockwise about z when forward: the tangent turns toward y at t = 0.
        }
        let d = fwd.jet::<1>(0.0, Approach::Above).d[0];
        assert!(d.dot(f.y().as_vec()) > 0.0);
        let d = rev.jet::<1>(0.0, Approach::Above).d[0];
        assert!(d.dot(f.y().as_vec()) < 0.0);
    }

    #[test]
    fn interval_evaluation_holds_the_truth() {
        let mut rng = Rng::new(14);
        for c in cases(3) {
            let r = truth(&c);
            for t in params(&mut rng) {
                let j = c.ellipse.jet_over::<Ivl, 3>(Ivl::from(t));
                let want = r.derivs(t, 3);
                for i in 0..3 {
                    assert!(holds(j.point.to_array()[i], want[0][i]), "{t}");
                    for k in 0..3 {
                        assert!(holds(j.d[k].to_array()[i], want[k + 1][i]), "{t} {k}");
                    }
                }
                // A wide angle interval, across critical points, holds every point over it.
                for w in [1e-6, 0.3, 2.0, 7.0] {
                    let ivl = Ivl::new(t, t + w);
                    let p = c.ellipse.point_over(ivl).to_array();
                    let span = Dd::new(ivl.hi) - Dd::new(t);
                    for s in 0..=16 {
                        let ts = Dd::new(t) + span * Dd::new(f64::from(s) / 16.0);
                        let want = r.derivs(ts, 0);
                        for i in 0..3 {
                            assert!(holds(p[i], want[0][i]), "{t} {w} {s}");
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn construction_refuses_sizes_the_pin_refused_or_built() {
        let f = Frame::WORLD;
        let mm = Length::from_mm;
        let one = mm(1.0);
        let name = |r: crate::Result<Circle>| r.unwrap_err().name();
        assert_eq!(
            name(Circle::new(f, mm(0.0), Sense::Forward)),
            "degenerate_radius"
        );
        assert_eq!(
            name(Circle::new(f, mm(1e-12), Sense::Forward)),
            "degenerate_radius"
        );
        assert_eq!(
            name(Circle::new(f, mm(-1.0), Sense::Forward)),
            "degenerate_radius"
        );
        assert!(Circle::new(f, mm(1e-12_f64.next_up()), Sense::Forward).is_ok());
        assert_eq!(
            Circle::new(f, mm(f64::NAN), Sense::Forward),
            Err(CurveError::NotFinite(Part::Radius))
        );
        assert_eq!(
            Circle::new(f, mm(f64::INFINITY), Sense::Forward),
            Err(CurveError::NotFinite(Part::Radius))
        );
        assert_eq!(
            Ellipse::new(f, one, mm(0.0), Sense::Forward),
            Err(CurveError::Degenerate(Degenerate::SemiAxis {
                axis: SemiAxis::Y,
                length: 0.0
            }))
        );
        assert_eq!(
            Ellipse::new(f, mm(f64::NAN), mm(0.0), Sense::Forward),
            Err(CurveError::NotFinite(Part::SemiAxis(SemiAxis::X)))
        );
        let f2 = Frame2::WORLD;
        assert!(Circle2::new(f2, mm(0.0), Sense::Forward).is_err());
        assert!(Ellipse2::new(f2, one, mm(f64::INFINITY), Sense::Forward).is_err());
        let e = Ellipse::new(f, mm(2.0), mm(3.0), Sense::Reversed).unwrap();
        assert_eq!(
            (e.semi_x(), e.semi_y(), e.sense()),
            (mm(2.0), mm(3.0), Sense::Reversed)
        );
        assert_eq!(e.frame(), &f);
        assert!(e.point(f64::NAN).x.is_nan());
        assert!(e.point(f64::INFINITY).x.is_nan());
    }

    fn want_x(r: &analytic::Ellipse, t: f64) -> Dd {
        r.derivs(t, 0)[0][0]
    }

    #[test]
    fn the_plane_conics_match_the_truth() {
        let mut rng = Rng::new(15);
        for i in 0..60 {
            let o = Point2::new(rng.next_signed_unit(), rng.next_signed_unit());
            let x = Vec2::new(rng.next_signed_unit(), rng.next_signed_unit());
            let Some(f2) = Frame2::new(o, x) else {
                continue;
            };
            let lift = |v: Vec2| [v.x, v.y, 0.0];
            let truth_frame = analytic::Frame {
                origin: [o.x, o.y, 0.0],
                x: lift(f2.x().as_vec()),
                y: lift(f2.y().as_vec()),
                z: [0.0, 0.0, 1.0],
            };
            let sense = if i % 2 == 0 {
                Sense::Forward
            } else {
                Sense::Reversed
            };
            let (a, b) = (Length::from_mm(0.5 + rng.next_f64()), Length::from_mm(0.25));
            let e2 = Ellipse2::new(f2, a, b, sense).unwrap();
            let c2 = Circle2::new(f2, a, sense).unwrap();
            let (sx, sy) = (a.mm(), b.mm() * sense.sign());
            let cases = [
                (
                    analytic::Ellipse {
                        frame: truth_frame,
                        semi_x: sx,
                        semi_y: sy,
                    },
                    sx,
                    sy,
                ),
                (
                    analytic::Ellipse {
                        frame: truth_frame,
                        semi_x: sx,
                        semi_y: sx * sense.sign(),
                    },
                    sx,
                    sx,
                ),
            ];
            for t in params(&mut rng) {
                let jets = [
                    e2.jet::<4>(t, Approach::Below),
                    c2.jet::<4>(t, Approach::Below),
                ];
                for (j, (r, sa, sb)) in jets.iter().zip(&cases) {
                    let want = r.derivs(t, 4);
                    let scale = |k: usize, i: usize| {
                        let oi = if k == 0 { [o.x, o.y][i].abs() } else { 0.0 };
                        oi + sa.abs() + sb.abs()
                    };
                    let p = j.point.to_array();
                    for i in 0..2 {
                        assert!(near(p[i], want[0][i], scale(0, i), 6.0), "{t}");
                        for k in 0..4 {
                            assert!(near(j.d[k].to_array()[i], want[k + 1][i], scale(1, i), 6.0));
                        }
                    }
                }
                let p = e2.point_over(Ivl::from(t));
                assert!(holds(p.x, want_x(&cases[0].0, t)));
                assert_eq!(e2.point(t), jets[0].point);
                assert_eq!(c2.point(t), jets[1].point);
            }
            assert_eq!((c2.radius(), c2.sense(), c2.frame()), (a, sense, &f2));
            assert_eq!(
                (e2.semi_x(), e2.semi_y(), e2.domain()),
                (a, b, ParamDomain::TURN)
            );
        }
    }
}
