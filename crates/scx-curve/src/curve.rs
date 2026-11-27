// Written for solidcorex. Apache-2.0.
//! [`Curve`] and [`Curve2`]: every curve kind as one value, dispatched by `match` (63 §3.2,
//! 01 C10; enums, not trait objects, because a `dyn` interface cannot carry the
//! scalar-generic evaluator, 63 A7).
//!
//! Each method is the kind's own call under one `match`, so a dispatched result has the
//! kind's bits. [`Curve::eval_many`] matches once per batch, outside the loop.
//!
//! The enums are `#[non_exhaustive]` (01 C10): the `Nurbs` arm joins in CU3, and `Parabola`,
//! `Hyperbola` and `Procedural` in step 5, so a match outside this crate keeps a wildcard
//! arm. They are not `Copy`, although every arm today is: the `Nurbs` arm holds an `Arc`.

use scx_num::domain::ParamDomain;
use scx_num::jet::{CurveJet, CurveJet2};
use scx_num::trig::Trig;
use scx_num::vector2d::Point2;
use scx_num::vector3d::Point3;
use scx_nurbs::Approach;

use crate::conic::{Circle, Circle2, Ellipse, Ellipse2};
use crate::line::{Line, Line2};

/// A curve in space: one of the kinds.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub enum Curve {
    /// A line, over every `t`.
    Line(Line),
    /// A circle, `t` the angle in radians.
    Circle(Circle),
    /// An ellipse, `t` the angle in radians.
    Ellipse(Ellipse),
}

/// A curve in the plane (a parameter-space curve): one of the kinds.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub enum Curve2 {
    /// A line, over every `t`.
    Line(Line2),
    /// A circle, `t` the angle in radians.
    Circle(Circle2),
    /// An ellipse, `t` the angle in radians.
    Ellipse(Ellipse2),
}

// The largest arm (an ellipse: a frame of nine `f64`, two semi-axes and a sense) plus the
// tag; an arm that would grow past this goes behind an `Arc` (63 A7).
const _: () = assert!(size_of::<Curve>() <= 104);
const _: () = assert!(size_of::<Curve2>() <= 64);

/// Every method once, for both enums: each arm calls the kind's method of the same name.
macro_rules! dispatch {
    ($self:expr, $c:ident => $call:expr) => {
        match $self {
            Self::Line($c) => $call,
            Self::Circle($c) => $call,
            Self::Ellipse($c) => $call,
        }
    };
}

impl Curve {
    /// The parameter's domain: [`ParamDomain::Unbounded`] for a line,
    /// [`ParamDomain::TURN`] for a conic.
    #[inline]
    #[must_use]
    pub fn domain(&self) -> ParamDomain {
        dispatch!(self, c => c.domain())
    }

    /// The point at `t`.
    #[inline]
    #[must_use]
    pub fn point(&self, t: f64) -> Point3 {
        dispatch!(self, c => c.point(t))
    }

    /// The point and the first `D` derivatives at `t`, from `side` where the curve is not
    /// smooth there (no kind here: `side` matters from the `Nurbs` arm on).
    #[inline]
    #[must_use]
    pub fn jet<const D: usize>(&self, t: f64, side: Approach) -> CurveJet<D> {
        dispatch!(self, c => c.jet(t, side))
    }

    /// The point at `t` in the scalar `T` (an enclosure in
    /// [`Ivl`](scx_num::interval::Ivl)).
    #[inline]
    #[must_use]
    pub fn point_over<T: Trig>(&self, t: T) -> Point3<T> {
        dispatch!(self, c => c.point_over(t))
    }

    /// The point and the first `D` derivatives at `t` in the scalar `T`, from `side` as
    /// [`Curve::jet`].
    #[inline]
    #[must_use]
    pub fn jet_over<T: Trig, const D: usize>(&self, t: T, side: Approach) -> CurveJet<D, T> {
        let _ = side;
        dispatch!(self, c => c.jet_over(t))
    }

    /// The point at each of `ts`, into `out`: `out[i]` is [`Curve::point`]`(ts[i])`, bit
    /// for bit (a NaN is a NaN; Rust leaves its sign and payload open). The kind is matched
    /// once, and a conic reads its frame's axes once.
    ///
    /// # Panics
    ///
    /// If `ts` and `out` differ in length.
    pub fn eval_many(&self, ts: &[f64], out: &mut [Point3]) {
        assert_eq!(
            ts.len(),
            out.len(),
            "eval_many: ts and out differ in length"
        );
        match self {
            Self::Line(l) => {
                for (p, &t) in out.iter_mut().zip(ts) {
                    *p = l.point(t);
                }
            }
            Self::Circle(c) => c.fill(ts, out),
            Self::Ellipse(e) => e.fill(ts, out),
        }
    }
}

impl Curve2 {
    /// The parameter's domain, as [`Curve::domain`].
    #[inline]
    #[must_use]
    pub fn domain(&self) -> ParamDomain {
        dispatch!(self, c => c.domain())
    }

    /// The point at `t`.
    #[inline]
    #[must_use]
    pub fn point(&self, t: f64) -> Point2 {
        dispatch!(self, c => c.point(t))
    }

    /// The point and the first `D` derivatives at `t`, from `side` as [`Curve::jet`].
    #[inline]
    #[must_use]
    pub fn jet<const D: usize>(&self, t: f64, side: Approach) -> CurveJet2<D> {
        dispatch!(self, c => c.jet(t, side))
    }

    /// The point at `t` in the scalar `T`.
    #[inline]
    #[must_use]
    pub fn point_over<T: Trig>(&self, t: T) -> Point2<T> {
        dispatch!(self, c => c.point_over(t))
    }

    /// The point and the first `D` derivatives at `t` in the scalar `T`, from `side`.
    #[inline]
    #[must_use]
    pub fn jet_over<T: Trig, const D: usize>(&self, t: T, side: Approach) -> CurveJet2<D, T> {
        let _ = side;
        dispatch!(self, c => c.jet_over(t))
    }

    /// The point at each of `ts`, into `out`, as [`Curve::eval_many`].
    ///
    /// # Panics
    ///
    /// If `ts` and `out` differ in length.
    pub fn eval_many(&self, ts: &[f64], out: &mut [Point2]) {
        assert_eq!(
            ts.len(),
            out.len(),
            "eval_many: ts and out differ in length"
        );
        match self {
            Self::Line(l) => {
                for (p, &t) in out.iter_mut().zip(ts) {
                    *p = l.point(t);
                }
            }
            Self::Circle(c) => c.fill(ts, out),
            Self::Ellipse(e) => e.fill(ts, out),
        }
    }
}

macro_rules! from_kind {
    ($enum:ident: $($kind:ident => $arm:ident),*) => {$(
        impl From<$kind> for $enum {
            fn from(c: $kind) -> Self {
                Self::$arm(c)
            }
        }
    )*};
}

from_kind!(Curve: Line => Line, Circle => Circle, Ellipse => Ellipse);
from_kind!(Curve2: Line2 => Line, Circle2 => Circle, Ellipse2 => Ellipse);

#[cfg(test)]
mod tests {
    use super::*;
    use scx_num::frame::{Frame, Frame2, Sense};
    use scx_num::hash::Rng;
    use scx_num::interval::Ivl;
    use scx_num::units::Length;
    use scx_num::vector2d::Vec2;
    use scx_num::vector3d::Vec3;

    const SIDES: [Approach; 2] = [Approach::Below, Approach::Above];

    fn signed(rng: &mut Rng, scale: f64) -> f64 {
        scale * rng.next_signed_unit()
    }

    fn sense(i: usize) -> Sense {
        if i.is_multiple_of(2) {
            Sense::Forward
        } else {
            Sense::Reversed
        }
    }

    fn curves(seed: u64) -> Vec<Curve> {
        let mut rng = Rng::new(seed);
        let mut out = Vec::new();
        for (i, scale) in [1.0, 1e-3, 1e4].into_iter().cycle().take(60).enumerate() {
            let p = Point3::from(core::array::from_fn(|_| signed(&mut rng, scale)));
            let v = Vec3::from(core::array::from_fn(|_| signed(&mut rng, scale)));
            let x = Vec3::from(core::array::from_fn(|_| signed(&mut rng, 1.0)));
            let (a, b) = (
                Length::from_mm(scale * (0.25 + rng.next_f64())),
                Length::from_mm(scale * (0.25 + rng.next_f64())),
            );
            if let Ok(l) = Line::new(p, v) {
                out.push(l.into());
            }
            if let Some(f) = Frame::new(p, v, x) {
                out.push(Circle::new(f, a, sense(i)).unwrap().into());
                out.push(Ellipse::new(f, a, b, sense(i)).unwrap().into());
            }
        }
        out
    }

    fn curves2(seed: u64) -> Vec<Curve2> {
        let mut rng = Rng::new(seed);
        let mut out = Vec::new();
        for (i, scale) in [1.0, 1e-3, 1e4].into_iter().cycle().take(60).enumerate() {
            let p = Point2::new(signed(&mut rng, scale), signed(&mut rng, scale));
            let v = Vec2::new(signed(&mut rng, scale), signed(&mut rng, scale));
            let (a, b) = (
                Length::from_mm(scale * (0.25 + rng.next_f64())),
                Length::from_mm(scale * (0.25 + rng.next_f64())),
            );
            if let Ok(l) = Line2::new(p, v) {
                out.push(l.into());
            }
            if let Some(f) = Frame2::new(p, v) {
                out.push(Circle2::new(f, a, sense(i)).unwrap().into());
                out.push(Ellipse2::new(f, a, b, sense(i)).unwrap().into());
            }
        }
        out
    }

    /// Ordinary parameters, large angles, signed zero, infinities and NaN.
    fn params(rng: &mut Rng) -> Vec<f64> {
        let mut t = vec![
            0.0,
            -0.0,
            1.0,
            -1.0,
            1e-300,
            1e6,
            -3.75e5,
            f64::INFINITY,
            f64::NAN,
        ];
        t.extend((0..24).map(|_| 10.0 * rng.next_signed_unit()));
        t
    }

    /// The bits, with every NaN one pattern: Rust leaves a NaN's sign and payload
    /// unspecified (an optimised loop and a single call may differ), so no caller can key on
    /// them. Signed zeros stay apart.
    fn bits(x: f64) -> u64 {
        if x.is_nan() {
            f64::NAN.to_bits()
        } else {
            x.to_bits()
        }
    }

    fn bits3(p: Point3) -> [u64; 3] {
        p.to_array().map(bits)
    }

    fn bits2(p: Point2) -> [u64; 2] {
        p.to_array().map(bits)
    }

    fn ibits(i: Ivl) -> [u64; 2] {
        [bits(i.lo), bits(i.hi)]
    }

    fn jbits<const D: usize>(j: &CurveJet<D>) -> Vec<[u64; 3]> {
        core::iter::once(bits3(j.point))
            .chain(j.d.iter().map(|v| v.to_array().map(bits)))
            .collect()
    }

    fn jbits2<const D: usize>(j: &CurveJet2<D>) -> Vec<[u64; 2]> {
        core::iter::once(bits2(j.point))
            .chain(j.d.iter().map(|v| v.to_array().map(bits)))
            .collect()
    }

    /// The kind's own calls, for comparison with the dispatch.
    struct Direct3 {
        domain: ParamDomain,
        point: Point3,
        jet: CurveJet<4>,
        jet0: CurveJet<0>,
        over: Point3<Ivl>,
        jet_over: CurveJet<2, Ivl>,
    }

    fn direct3(c: &Curve, t: f64, side: Approach) -> Direct3 {
        let w = Ivl::new(t, t + 0.25);
        macro_rules! of {
            ($k:expr) => {
                Direct3 {
                    domain: $k.domain(),
                    point: $k.point(t),
                    jet: $k.jet::<4>(t, side),
                    jet0: $k.jet::<0>(t, side),
                    over: $k.point_over(w),
                    jet_over: $k.jet_over::<Ivl, 2>(w),
                }
            };
        }
        match c {
            Curve::Line(k) => of!(k),
            Curve::Circle(k) => of!(k),
            Curve::Ellipse(k) => of!(k),
        }
    }

    #[test]
    fn dispatch_has_the_bits_of_the_kind() {
        let mut rng = Rng::new(21);
        let mut n = 0;
        for c in curves(20) {
            for t in params(&mut rng) {
                for side in SIDES {
                    let want = direct3(&c, t, side);
                    let w = Ivl::new(t, t + 0.25);
                    assert_eq!(c.domain(), want.domain);
                    assert_eq!(bits3(c.point(t)), bits3(want.point), "{c:?} {t}");
                    assert_eq!(jbits(&c.jet::<4>(t, side)), jbits(&want.jet));
                    assert_eq!(jbits(&c.jet::<0>(t, side)), jbits(&want.jet0));
                    let (got, exp) = (c.point_over(w).to_array(), want.over.to_array());
                    assert_eq!(got.map(ibits), exp.map(ibits));
                    let (got, exp) = (c.jet_over::<Ivl, 2>(w, side), want.jet_over);
                    assert_eq!(
                        got.point.to_array().map(ibits),
                        exp.point.to_array().map(ibits)
                    );
                    for k in 0..2 {
                        assert_eq!(
                            got.d[k].to_array().map(ibits),
                            exp.d[k].to_array().map(ibits)
                        );
                    }
                    n += 1;
                }
            }
        }
        assert!(n > 10_000, "{n}");
    }

    #[test]
    fn plane_dispatch_has_the_bits_of_the_kind() {
        let mut rng = Rng::new(23);
        let mut n = 0;
        for c in curves2(22) {
            for t in params(&mut rng) {
                let w = Ivl::new(t, t + 0.25);
                for side in SIDES {
                    macro_rules! check {
                        ($k:expr) => {{
                            assert_eq!(c.domain(), $k.domain());
                            assert_eq!(bits2(c.point(t)), bits2($k.point(t)), "{c:?} {t}");
                            assert_eq!(jbits2(&c.jet::<4>(t, side)), jbits2(&$k.jet::<4>(t, side)));
                            assert_eq!(jbits2(&c.jet::<0>(t, side)), jbits2(&$k.jet::<0>(t, side)));
                            let (got, exp) = (c.point_over(w), $k.point_over(w));
                            assert_eq!([ibits(got.x), ibits(got.y)], [ibits(exp.x), ibits(exp.y)]);
                            let (got, exp) =
                                (c.jet_over::<Ivl, 1>(w, side), $k.jet_over::<Ivl, 1>(w));
                            let ivls = |j: CurveJet2<1, Ivl>| {
                                [j.point.x, j.point.y, j.d[0].x, j.d[0].y].map(ibits)
                            };
                            assert_eq!(ivls(got), ivls(exp));
                        }};
                    }
                    match &c {
                        Curve2::Line(k) => check!(k),
                        Curve2::Circle(k) => check!(k),
                        Curve2::Ellipse(k) => check!(k),
                    }
                    n += 1;
                }
            }
        }
        assert!(n > 10_000, "{n}");
    }

    #[test]
    fn eval_many_has_the_bits_of_point() {
        let mut rng = Rng::new(25);
        for c in curves(24) {
            let ts = params(&mut rng);
            let mut out = vec![Point3::ORIGIN; ts.len()];
            c.eval_many(&ts, &mut out);
            for (p, &t) in out.iter().zip(&ts) {
                assert_eq!(bits3(*p), bits3(c.point(t)), "{c:?} {t}");
            }
            c.eval_many(&[], &mut []);
        }
        for c in curves2(26) {
            let ts = params(&mut rng);
            let mut out = vec![Point2::ORIGIN; ts.len()];
            c.eval_many(&ts, &mut out);
            for (p, &t) in out.iter().zip(&ts) {
                assert_eq!(bits2(*p), bits2(c.point(t)), "{c:?} {t}");
            }
        }
    }

    #[test]
    #[should_panic(expected = "differ in length")]
    fn eval_many_refuses_slices_of_different_lengths() {
        let c = Curve::from(Line::new(Point3::ORIGIN, Vec3::new(1.0, 0.0, 0.0)).unwrap());
        c.eval_many(&[0.0, 1.0], &mut [Point3::ORIGIN]);
    }

    #[test]
    fn every_kind_converts_into_its_arm() {
        let f = Frame::new(
            Point3::ORIGIN,
            Vec3::new(0.0, 0.0, 1.0),
            Vec3::new(1.0, 0.0, 0.0),
        )
        .unwrap();
        let r = Length::from_mm(2.0);
        let l = Line::new(Point3::ORIGIN, Vec3::new(1.0, 0.0, 0.0)).unwrap();
        let c = Circle::new(f, r, Sense::Forward).unwrap();
        let e = Ellipse::new(f, r, Length::from_mm(1.0), Sense::Reversed).unwrap();
        assert_eq!(Curve::from(l), Curve::Line(l));
        assert_eq!(Curve::from(c), Curve::Circle(c));
        assert_eq!(Curve::from(e), Curve::Ellipse(e));
        assert_eq!(Curve::from(l).domain(), ParamDomain::Unbounded);
        assert_eq!(Curve::from(e).domain(), ParamDomain::TURN);
        let f2 = Frame2::new(Point2::ORIGIN, Vec2::new(1.0, 0.0)).unwrap();
        let l2 = Line2::new(Point2::ORIGIN, Vec2::new(0.0, 1.0)).unwrap();
        let c2 = Circle2::new(f2, r, Sense::Forward).unwrap();
        let e2 = Ellipse2::new(f2, r, Length::from_mm(1.0), Sense::Forward).unwrap();
        assert_eq!(Curve2::from(l2), Curve2::Line(l2));
        assert_eq!(Curve2::from(c2), Curve2::Circle(c2));
        assert_eq!(Curve2::from(e2), Curve2::Ellipse(e2));
        assert_ne!(
            Curve::from(c),
            Curve::from(Ellipse::new(f, r, r, Sense::Forward).unwrap())
        );
    }
}
