// Written for solidcorex. Apache-2.0.
//! [`Curve`] and [`Curve2`]: every curve kind as one value, dispatched by `match` (63 §3.2,
//! 01 C10; enums, not trait objects, because a `dyn` interface cannot carry the
//! scalar-generic evaluator, 63 A7).
//!
//! Each method is the kind's own call under one `match`, so a dispatched result has the
//! kind's bits. [`Curve::eval_many`] matches once per batch, outside the loop.
//!
//! The `Nurbs` arm holds its [`NurbsCurve`] behind an `Arc` (63 A7): cloning a curve is a
//! count, and the enum keeps the size of its largest analytic arm. Its calls are the curve's
//! own: [`NurbsCurve::point`] on the piece above `t`, [`NurbsCurve::jet`] on the piece
//! `side` picks, and [`NurbsCurve::point_over`] and [`NurbsCurve::jet_over`] over the
//! scalar. The `f64` calls cannot fail here, so where the curve's call errs (a parameter
//! that is not finite, a value that overflows, a jet past [`scx_nurbs::MAX_ORDER`]) every
//! coordinate is NaN, as a conic's are at an infinite angle.
//!
//! The enums are `#[non_exhaustive]` (01 C10): `Parabola`, `Hyperbola` and `Procedural`
//! join in step 5, so a match outside this crate keeps a wildcard arm. They are not `Copy`:
//! the `Nurbs` arm holds an `Arc`.

use std::sync::Arc;

use scx_num::domain::ParamDomain;
use scx_num::jet::{CurveJet, CurveJet2};
use scx_num::trig::Trig;
use scx_num::vector2d::{Point2, Vec2};
use scx_num::vector3d::{Point3, Vec3};
use scx_nurbs::{Approach, NurbsCurve};

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
    /// A NURBS curve, over its knots' domain and continuing its end pieces past it.
    Nurbs(Arc<NurbsCurve<3>>),
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
    /// A NURBS curve, as [`Curve::Nurbs`].
    Nurbs(Arc<NurbsCurve<2>>),
}

// The largest arm (an ellipse: a frame of nine `f64`, two semi-axes and a sense) plus the
// tag; an arm that would grow past this goes behind an `Arc` (63 A7), as `Nurbs` does.
const _: () = assert!(size_of::<Curve>() <= 104);
const _: () = assert!(size_of::<Curve2>() <= 64);

/// Every method once, for both enums: each analytic arm calls the kind's method of the same
/// name, the `Nurbs` arm the second expression.
macro_rules! dispatch {
    ($self:expr, $c:ident => $call:expr, $n:ident => $nurbs:expr) => {
        match $self {
            Self::Line($c) => $call,
            Self::Circle($c) => $call,
            Self::Ellipse($c) => $call,
            Self::Nurbs($n) => $nurbs,
        }
    };
}

/// A NURBS curve's point on the piece above `t`, NaN where it errs.
#[inline]
fn nurbs_point<const N: usize>(c: &NurbsCurve<N>, t: f64) -> [f64; N] {
    c.point(t, Approach::Above).unwrap_or([f64::NAN; N])
}

/// A NURBS curve's point and first `D` derivatives on the piece `side` picks, NaN where it
/// errs.
#[inline]
fn nurbs_jet<const N: usize, const D: usize>(
    c: &NurbsCurve<N>,
    t: f64,
    side: Approach,
) -> ([f64; N], [[f64; N]; D]) {
    c.jet::<D>(t, side)
        .unwrap_or(([f64::NAN; N], [[f64::NAN; N]; D]))
}

fn point3<T>([x, y, z]: [T; 3]) -> Point3<T> {
    Point3::new(x, y, z)
}

fn jet3<T, const D: usize>((p, d): ([T; 3], [[T; 3]; D])) -> CurveJet<D, T> {
    CurveJet {
        point: point3(p),
        d: d.map(|[x, y, z]| Vec3::new(x, y, z)),
    }
}

fn point2<T>([x, y]: [T; 2]) -> Point2<T> {
    Point2::new(x, y)
}

fn jet2<T, const D: usize>((p, d): ([T; 2], [[T; 2]; D])) -> CurveJet2<D, T> {
    CurveJet2 {
        point: point2(p),
        d: d.map(|[x, y]| Vec2::new(x, y)),
    }
}

impl Curve {
    /// The parameter's domain: [`ParamDomain::Unbounded`] for a line,
    /// [`ParamDomain::TURN`] for a conic, [`ParamDomain::Bounded`] by its knots for a NURBS
    /// curve.
    #[inline]
    #[must_use]
    pub fn domain(&self) -> ParamDomain {
        dispatch!(self, c => c.domain(), n => ParamDomain::Bounded(n.domain()))
    }

    /// The point at `t` (on the piece above `t` for a NURBS curve, which is continuous).
    #[inline]
    #[must_use]
    pub fn point(&self, t: f64) -> Point3 {
        dispatch!(self, c => c.point(t), n => point3(nurbs_point(n, t)))
    }

    /// The point and the first `D` derivatives at `t`, from `side` where the curve is not
    /// smooth there (at a NURBS curve's knot; an analytic kind is smooth everywhere).
    #[inline]
    #[must_use]
    pub fn jet<const D: usize>(&self, t: f64, side: Approach) -> CurveJet<D> {
        dispatch!(self, c => c.jet(t, side), n => jet3(nurbs_jet(n, t, side)))
    }

    /// The point at `t` in the scalar `T` (an enclosure in
    /// [`Ivl`](scx_num::interval::Ivl)).
    #[inline]
    #[must_use]
    pub fn point_over<T: Trig>(&self, t: T) -> Point3<T> {
        dispatch!(self, c => c.point_over(t), n => point3(n.point_over(t, Approach::Above)))
    }

    /// The point and the first `D` derivatives at `t` in the scalar `T`, from `side` as
    /// [`Curve::jet`]. An interval holding a knot holds both sides' values there.
    #[inline]
    #[must_use]
    pub fn jet_over<T: Trig, const D: usize>(&self, t: T, side: Approach) -> CurveJet<D, T> {
        dispatch!(self, c => c.jet_over(t), n => jet3(n.jet_over::<T, D>(t, side)))
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
            Self::Nurbs(n) => {
                for (p, &t) in out.iter_mut().zip(ts) {
                    *p = point3(nurbs_point(n, t));
                }
            }
        }
    }
}

impl Curve2 {
    /// The parameter's domain, as [`Curve::domain`].
    #[inline]
    #[must_use]
    pub fn domain(&self) -> ParamDomain {
        dispatch!(self, c => c.domain(), n => ParamDomain::Bounded(n.domain()))
    }

    /// The point at `t`, as [`Curve::point`].
    #[inline]
    #[must_use]
    pub fn point(&self, t: f64) -> Point2 {
        dispatch!(self, c => c.point(t), n => point2(nurbs_point(n, t)))
    }

    /// The point and the first `D` derivatives at `t`, from `side` as [`Curve::jet`].
    #[inline]
    #[must_use]
    pub fn jet<const D: usize>(&self, t: f64, side: Approach) -> CurveJet2<D> {
        dispatch!(self, c => c.jet(t, side), n => jet2(nurbs_jet(n, t, side)))
    }

    /// The point at `t` in the scalar `T`.
    #[inline]
    #[must_use]
    pub fn point_over<T: Trig>(&self, t: T) -> Point2<T> {
        dispatch!(self, c => c.point_over(t), n => point2(n.point_over(t, Approach::Above)))
    }

    /// The point and the first `D` derivatives at `t` in the scalar `T`, from `side` as
    /// [`Curve::jet_over`].
    #[inline]
    #[must_use]
    pub fn jet_over<T: Trig, const D: usize>(&self, t: T, side: Approach) -> CurveJet2<D, T> {
        dispatch!(self, c => c.jet_over(t), n => jet2(n.jet_over::<T, D>(t, side)))
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
            Self::Nurbs(n) => {
                for (p, &t) in out.iter_mut().zip(ts) {
                    *p = point2(nurbs_point(n, t));
                }
            }
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

macro_rules! from_nurbs {
    ($enum:ident, $d:literal) => {
        impl From<Arc<NurbsCurve<$d>>> for $enum {
            fn from(c: Arc<NurbsCurve<$d>>) -> Self {
                Self::Nurbs(c)
            }
        }

        impl From<NurbsCurve<$d>> for $enum {
            fn from(c: NurbsCurve<$d>) -> Self {
                Self::Nurbs(Arc::new(c))
            }
        }
    };
}

from_nurbs!(Curve, 3);
from_nurbs!(Curve2, 2);

#[cfg(test)]
mod tests {
    use super::*;
    use scx_num::frame::{Frame, Frame2, Sense};
    use scx_num::hash::Rng;
    use scx_num::interval::Ivl;
    use scx_num::units::Length;
    use scx_nurbs::KnotVector;
    use scx_testkit::sample::{self, Knots, Spec, Weights};

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

    /// NURBS curves of every knot and weight kind, degrees 1..=4 and 9 (the wide kernel),
    /// without the picks whose interior knot repeats `p + 1` times, which are not curves.
    fn nurbs<const D: usize>(rng: &mut Rng) -> Vec<NurbsCurve<D>> {
        let mut out = Vec::new();
        for degree in [1, 2, 3, 4, 9] {
            for knots in [Knots::Uniform, Knots::Clustered, Knots::Multiple] {
                for weights in [Weights::Unit, Weights::Moderate, Weights::NearZero] {
                    let spec = Spec {
                        degree,
                        count: degree + 3,
                        knots,
                    };
                    let d = sample::curve::<D>(rng, spec, weights);
                    let c = KnotVector::new(d.degree, d.knots)
                        .and_then(|k| NurbsCurve::new(k, d.points, d.weights));
                    out.extend(c.ok());
                }
            }
        }
        assert!(out.len() > 30, "{}", out.len());
        out
    }

    fn curves(seed: u64) -> Vec<Curve> {
        let mut rng = Rng::new(seed);
        let mut out: Vec<Curve> = nurbs::<3>(&mut rng).into_iter().map(Curve::from).collect();
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
        let mut out: Vec<Curve2> = nurbs::<2>(&mut rng).into_iter().map(Curve2::from).collect();
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

    /// Ordinary parameters, large angles, signed zero, infinities and NaN, and parameters
    /// inside a NURBS curve's domain, `[0, 1]`.
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
        t.extend((0..12).map(|_| rng.next_f64()));
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
            // The curve's own calls: the jet as `derivs`, the order and the point together.
            Curve::Nurbs(n) => {
                let jet = |d: &[[f64; 3]]| CurveJet {
                    point: Point3::from_array(d[0]),
                    d: core::array::from_fn(|k| Vec3::from_array(d[k + 1])),
                };
                let nan = [[f64::NAN; 3]; 5];
                let (over, (q, d)) = (
                    n.point_over(w, Approach::Above),
                    n.jet_over::<Ivl, 2>(w, side),
                );
                Direct3 {
                    domain: ParamDomain::Bounded(n.domain()),
                    point: Point3::from_array(n.point(t, Approach::Above).unwrap_or(nan[0])),
                    jet: jet(&n.derivs::<5>(t, side).unwrap_or(nan)),
                    jet0: CurveJet {
                        point: Point3::from_array(n.derivs::<1>(t, side).map_or(nan[0], |d| d[0])),
                        d: [],
                    },
                    over: Point3::new(over[0], over[1], over[2]),
                    jet_over: CurveJet {
                        point: Point3::new(q[0], q[1], q[2]),
                        d: d.map(|[x, y, z]| Vec3::new(x, y, z)),
                    },
                }
            }
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
                        Curve2::Nurbs(n) => {
                            let nan = [[f64::NAN; 2]; 5];
                            let p = n.point(t, Approach::Above).unwrap_or(nan[0]);
                            let want = n.derivs::<5>(t, side).unwrap_or(nan);
                            let got = c.jet::<4>(t, side);
                            assert_eq!(c.domain(), ParamDomain::Bounded(n.domain()));
                            assert_eq!(bits2(c.point(t)), p.map(bits), "{c:?} {t}");
                            assert_eq!(bits2(got.point), want[0].map(bits));
                            for (g, w) in got.d.iter().zip(&want[1..]) {
                                assert_eq!(g.to_array().map(bits), w.map(bits));
                            }
                            let want0 = n.derivs::<1>(t, side).map_or(nan[0], |d| d[0]);
                            assert_eq!(bits2(c.jet::<0>(t, side).point), want0.map(bits));
                            let (got, exp) = (c.point_over(w), n.point_over(w, Approach::Above));
                            assert_eq!([ibits(got.x), ibits(got.y)], exp.map(ibits));
                            let got = c.jet_over::<Ivl, 1>(w, side);
                            let (q, d) = n.jet_over::<Ivl, 1>(w, side);
                            assert_eq!(
                                [got.point.x, got.point.y, got.d[0].x, got.d[0].y].map(ibits),
                                [q[0], q[1], d[0][0], d[0][1]].map(ibits)
                            );
                        }
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
        let mut rng = Rng::new(27);
        let n = nurbs::<3>(&mut rng).swap_remove(0);
        let shared = Arc::new(n.clone());
        assert_eq!(Curve::from(n.clone()), Curve::Nurbs(shared.clone()));
        assert!(matches!(Curve::from(shared.clone()), Curve::Nurbs(a) if Arc::ptr_eq(&a, &shared)));
        assert_eq!(
            Curve::from(n).domain(),
            ParamDomain::Bounded(shared.domain())
        );
        let n2 = nurbs::<2>(&mut rng).swap_remove(0);
        assert_eq!(Curve2::from(n2.clone()), Curve2::Nurbs(Arc::new(n2)));
    }

    /// The `f64` calls of a NURBS arm are NaN where the curve's own err, and a clone shares
    /// the curve.
    #[test]
    fn nurbs_failures_are_nan_and_clones_share() {
        let mut rng = Rng::new(28);
        let n = Arc::new(nurbs::<3>(&mut rng).swap_remove(3));
        let c = Curve::Nurbs(n.clone());
        for t in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
            assert!(c.point(t).to_array().iter().all(|x| x.is_nan()));
            let j = c.jet::<2>(t, Approach::Below);
            assert!(jbits(&j).iter().flatten().all(|&b| b == f64::NAN.to_bits()));
        }
        let j = c.jet::<32>(0.5, Approach::Above);
        assert!(j.d.iter().all(|v| v.to_array().iter().all(|x| x.is_nan())));
        let copy = c.clone();
        assert_eq!(Arc::strong_count(&n), 3);
        assert_eq!(copy, c);
    }
}
