// Written for solidcorex. Apache-2.0.
//! Sine and cosine, their hyperbolic twins and the square over a ring: the one evaluator of
//! an analytic curve or surface runs in `f64` for values and in [`Ivl`] for enclosures
//! (research 63 §3.2, "the scalar-generic form").
//!
//! - `f64`: [`crate::fmath::sin_cos`] and [`crate::fmath::sinh_cosh`], correctly rounded;
//!   the square `x * x`.
//! - [`Ivl`]: an enclosure of `{sin x, cos x : x in the interval}` for any interval. The
//!   ring's `sin_cos_s` ([`crate::ring::TayScalar`]) is the fork's, valid on `[0, pi/2]`
//!   only; this one holds everywhere. `sinh` is increasing, so its enclosure is its two
//!   ends' values widened; `cosh` falls to 1 at zero and rises either side, so its
//!   enclosure is the ends' values widened, its low end 1 when the interval holds zero and
//!   never below 1. The square of an interval holding zero starts at zero, where the
//!   ring's product `x * x` would start at `lo * hi < 0` (a parabola's `s^2`).
//!
//! The interval enclosure takes each function's values at the two ends, widened by
//! [`crate::ring::wide`] (`1e-15` relative, past [`crate::fmath`]'s half ulp), and adds the
//! extreme `±1` of every critical point `n pi/2` the interval may hold. Which `n` it may hold
//! is decided with a margin of `4 eps |x / (pi/2)|` on each side, past the two roundings of
//! that quotient, so a critical point is never missed (one taken in error only widens the
//! result to its true bound). An interval reaching four critical points, holding a NaN or
//! with an infinite end gives `[-1, 1]` (the whole line for a NaN).
//!
//! Soundness rests on `fmath`'s accuracy, which is measured (`fmath`'s module doc), not
//! proved, as for the rest of the interval functions here.
//!
//! [`Ranged`], which [`Trig`] requires, is what a piecewise evaluator (a NURBS curve's) needs
//! besides: the range a value spans, to pick the pieces it meets, and the knot difference a
//! basis divides by, which an interval takes exactly rather than as a sum of two intervals
//! that grows with the parameter's width.

use core::ops::Neg;

use crate::interval::{Ivl, cmax, cmin};
use crate::math::HALF_PI;
use crate::ring::{Ring, wide};

/// A ring whose values span a range of the line: `f64` (one point) and [`Ivl`].
pub trait Ranged: Ring + Neg<Output = Self> {
    /// The lowest and highest values: `(x, x)` for an `f64`.
    fn ends(self) -> (f64, f64);

    /// The part of `self` inside `[lo, hi]`, for a `self` that meets it: `self` for an `f64`.
    #[must_use]
    fn clip(self, lo: f64, hi: f64) -> Self;

    /// The smallest value holding both: for an `f64`, `self` if the two are the same value
    /// and NaN otherwise (an `f64` holds one point).
    #[must_use]
    fn join(self, other: Self) -> Self;

    /// The value standing for an undefined result: NaN, or the whole line.
    fn undefined() -> Self;

    /// The knot difference `high - low` a basis divides by, given `right = high - u` and
    /// `left = u - low`: `right + left` for an `f64` (the pin's arithmetic, with its
    /// rounding), and for an interval an enclosure of `high - low` itself, which holds the
    /// true difference however wide `u` is.
    #[must_use]
    fn gap(right: Self, left: Self, high: f64, low: f64) -> Self;

    /// `self * self`: for an `f64` the product, for an interval the tight enclosure of the
    /// squares of its points (from zero when it holds zero).
    #[must_use]
    fn sqr(self) -> Self;
}

impl Ranged for f64 {
    #[inline]
    fn ends(self) -> (f64, f64) {
        (self, self)
    }
    #[inline]
    fn clip(self, _lo: f64, _hi: f64) -> Self {
        self
    }
    #[inline]
    fn join(self, other: Self) -> Self {
        if self.to_bits() == other.to_bits() {
            self
        } else {
            f64::NAN
        }
    }
    #[inline]
    fn undefined() -> Self {
        f64::NAN
    }
    #[inline]
    fn gap(right: Self, left: Self, _high: f64, _low: f64) -> Self {
        right + left
    }
    #[inline]
    fn sqr(self) -> Self {
        self * self
    }
}

impl Ranged for Ivl {
    #[inline]
    fn ends(self) -> (f64, f64) {
        (self.lo, self.hi)
    }
    #[inline]
    fn clip(self, lo: f64, hi: f64) -> Self {
        Ivl::new(self.lo.max(lo), self.hi.min(hi))
    }
    #[inline]
    fn join(self, other: Self) -> Self {
        self.hull(other)
    }
    #[inline]
    fn undefined() -> Self {
        Ivl::whole()
    }
    #[inline]
    fn gap(_right: Self, _left: Self, high: f64, low: f64) -> Self {
        Ivl::from(high) - Ivl::from(low)
    }
    fn sqr(self) -> Self {
        if self.lo >= 0.0 {
            self * self
        } else if self.hi <= 0.0 {
            (-self) * (-self)
        } else {
            let m = Ivl::from(cmax(-self.lo, self.hi));
            Ivl::new(0.0, (m * m).hi)
        }
    }
}

/// A ring with sine and cosine and their hyperbolic twins: `f64` and [`Ivl`].
pub trait Trig: Ranged {
    /// `(sin x, cos x)`; for an interval, an enclosure of both over the interval.
    fn sin_cos(self) -> (Self, Self);

    /// `(sinh x, cosh x)`; for an interval, an enclosure of both over the interval.
    fn sinh_cosh(self) -> (Self, Self);
}

impl Trig for f64 {
    #[inline]
    fn sin_cos(self) -> (Self, Self) {
        crate::fmath::sin_cos(self)
    }
    #[inline]
    fn sinh_cosh(self) -> (Self, Self) {
        crate::fmath::sinh_cosh(self)
    }
}

/// `-[lo, hi] = [-hi, -lo]`, exact.
impl Neg for Ivl {
    type Output = Self;
    fn neg(self) -> Self {
        Self::new(-self.hi, -self.lo)
    }
}

impl Trig for Ivl {
    fn sin_cos(self) -> (Self, Self) {
        let unit = Ivl::new(-1.0, 1.0);
        if self.lo.is_nan() || self.hi.is_nan() || self.lo > self.hi {
            return (Ivl::whole(), Ivl::whole());
        }
        if !(self.lo.is_finite() && self.hi.is_finite()) {
            return (unit, unit);
        }
        // The critical points n pi/2 that may lie in [lo, hi].
        let q = |x: f64, up: bool| {
            let q = x / HALF_PI;
            let m = q.abs() * (4.0 * f64::EPSILON) + f64::MIN_POSITIVE;
            if up { (q + m).floor() } else { (q - m).ceil() }
        };
        let (n0, n1) = (q(self.lo, false), q(self.hi, true));
        if n1 - n0 >= 4.0 || n0.abs() > 4_503_599_627_370_496.0 {
            return (unit, unit);
        }
        let (sl, cl) = crate::fmath::sin_cos(self.lo);
        let (sh, ch) = crate::fmath::sin_cos(self.hi);
        let mut s = wide(cmin(sl, sh), cmax(sl, sh));
        let mut c = wide(cmin(cl, ch), cmax(cl, ch));
        // At most four values of n: each as an integer, its residue mod 4 the extremum.
        let mut n = n0;
        while n <= n1 {
            #[allow(clippy::cast_possible_truncation)] // an integer, |n| <= 2^52 + 4
            match (n as i64).rem_euclid(4) {
                0 => c.hi = 1.0,
                1 => s.hi = 1.0,
                2 => c.lo = -1.0,
                _ => s.lo = -1.0,
            }
            n += 1.0;
        }
        let clamp = |i: Ivl| Ivl::new(i.lo.max(-1.0), i.hi.min(1.0));
        (clamp(s), clamp(c))
    }

    fn sinh_cosh(self) -> (Self, Self) {
        if self.lo.is_nan() || self.hi.is_nan() || self.lo > self.hi {
            return (Ivl::whole(), Ivl::whole());
        }
        let (sl, cl) = crate::fmath::sinh_cosh(self.lo);
        let (sh, ch) = crate::fmath::sinh_cosh(self.hi);
        let s = wide(sl, sh);
        let c = if self.lo >= 0.0 {
            wide(cl, ch)
        } else if self.hi <= 0.0 {
            wide(ch, cl)
        } else {
            wide(1.0, cmax(cl, ch))
        };
        (s, Ivl::new(c.lo.max(1.0), c.hi))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hash::Rng;

    fn holds(i: Ivl, x: f64) -> bool {
        i.lo <= x && x <= i.hi
    }

    #[test]
    fn f64_is_fmath() {
        for x in [0.0, -0.0, 1.0, -2.5, 1e6, 1e300] {
            assert_eq!(Trig::sin_cos(x), crate::fmath::sin_cos(x));
        }
        assert!(Trig::sin_cos(f64::NAN).0.is_nan());
    }

    #[test]
    fn enclosure_holds_samples_inside() {
        let mut r = Rng::new(7);
        for _ in 0..4000 {
            let a = r.next_signed_unit() * 20.0;
            let w = (r.next_signed_unit().abs() * 8.0)
                * if r.next_signed_unit() > 0.0 {
                    1.0
                } else {
                    1e-6
                };
            let i = Ivl::new(a, a + w);
            let (s, c) = Trig::sin_cos(i);
            assert!(s.lo >= -1.0 && s.hi <= 1.0 && c.lo >= -1.0 && c.hi <= 1.0);
            for k in 0..=16 {
                let x = a + w * f64::from(k) / 16.0;
                if x > i.hi {
                    continue;
                }
                let (sx, cx) = crate::fmath::sin_cos(x);
                assert!(holds(s, sx) && holds(c, cx), "{i:?} at {x}: {s:?} {c:?}");
            }
        }
    }

    #[test]
    fn extremes_are_caught_at_the_critical_points() {
        for n in -8_i32..=8 {
            let x = f64::from(n) * HALF_PI;
            for i in [Ivl::new(x.next_down(), x.next_up()), Ivl::from(x)] {
                let (s, c) = Trig::sin_cos(i);
                match n.rem_euclid(4) {
                    0 => assert_eq!(c.hi, 1.0),
                    1 => assert_eq!(s.hi, 1.0),
                    2 => assert_eq!(c.lo, -1.0),
                    _ => assert_eq!(s.lo, -1.0),
                }
            }
        }
        // A tight interval away from the critical points stays tight.
        let (s, c) = Trig::sin_cos(Ivl::new(0.5, 0.5 + 1e-9));
        assert!(s.hi - s.lo < 2e-9 && c.hi - c.lo < 2e-9);
    }

    #[test]
    fn ranged_ends_clip_join_gap() {
        assert_eq!(Ranged::ends(2.5), (2.5, 2.5));
        assert_eq!(Ranged::clip(2.5, 3.0, 4.0), 2.5);
        assert_eq!(Ranged::join(2.5, 2.5), 2.5);
        assert!(Ranged::join(2.5, 3.0).is_nan());
        assert!(<f64 as Ranged>::undefined().is_nan());
        // The pin's sum, rounding and all.
        assert_eq!(
            <f64 as Ranged>::gap(0.7 - 0.1, 0.1 - 0.3, 0.7, 0.3),
            (0.7 - 0.1) + (0.1 - 0.3)
        );
        let i = Ivl::new(-1.0, 5.0);
        assert_eq!(i.ends(), (-1.0, 5.0));
        assert_eq!(i.clip(0.0, 2.0), Ivl::new(0.0, 2.0));
        assert_eq!(i.join(Ivl::new(6.0, 7.0)), Ivl::new(-1.0, 7.0));
        assert_eq!(<Ivl as Ranged>::undefined(), Ivl::whole());
        // Exact however wide u is: here the two sums would hold zero.
        let u = Ivl::new(0.0, 1.0);
        let g = Ivl::gap(Ivl::from(1.0) - u, u - Ivl::from(0.0), 1.0, 0.0);
        assert!(g.lo <= 1.0 && 1.0 <= g.hi && g.hi - g.lo < 1e-15);
        let g = Ivl::gap(u, u, 0.7, 0.1);
        assert!(g.lo <= 0.6 && 0.6 <= g.hi && g.lo > 0.59);
    }

    #[test]
    fn hyperbolic_enclosures_hold_samples_inside() {
        assert_eq!(Trig::sinh_cosh(0.7), crate::fmath::sinh_cosh(0.7));
        let mut r = Rng::new(8);
        for _ in 0..4000 {
            let a = r.next_signed_unit() * 20.0;
            let w = r.next_signed_unit().abs() * if r.next_f64() > 0.5 { 8.0 } else { 1e-6 };
            let i = Ivl::new(a, a + w);
            let (s, c) = Trig::sinh_cosh(i);
            assert!(c.lo >= 1.0);
            for k in 0..=16 {
                let x = a + w * f64::from(k) / 16.0;
                if x > i.hi {
                    continue;
                }
                let (sx, cx) = crate::fmath::sinh_cosh(x);
                assert!(holds(s, sx) && holds(c, cx), "{i:?} at {x}: {s:?} {c:?}");
            }
        }
        // Across zero cosh reaches 1; a tight interval stays tight.
        let (_, c) = Trig::sinh_cosh(Ivl::new(-1.0, 2.0));
        assert_eq!(c.lo, 1.0);
        let (s, c) = Trig::sinh_cosh(Ivl::new(0.5, 0.5 + 1e-9));
        assert!(s.hi - s.lo < 3e-9 && c.hi - c.lo < 3e-9);
        let (s, c) = Trig::sinh_cosh(Ivl::new(f64::NAN, 1.0));
        assert_eq!((s, c), (Ivl::whole(), Ivl::whole()));
        let (s, _) = Trig::sinh_cosh(Ivl::new(1.0, f64::INFINITY));
        assert_eq!(s.hi, f64::INFINITY);
    }

    #[test]
    fn the_square_is_tight() {
        assert_eq!(Ranged::sqr(-3.0), 9.0);
        assert_eq!(
            Ivl::new(2.0, 3.0).sqr(),
            Ivl::new(2.0, 3.0) * Ivl::new(2.0, 3.0)
        );
        assert_eq!(Ivl::new(-3.0, -2.0).sqr(), Ivl::new(2.0, 3.0).sqr());
        let s = Ivl::new(-2.0, 3.0).sqr();
        assert!(s.lo == 0.0 && s.hi >= 9.0 && s.hi < 9.0 + 1e-14);
        let mut r = Rng::new(9);
        for _ in 0..2000 {
            let a = r.next_signed_unit() * 5.0;
            let i = Ivl::new(a, a + r.next_f64() * 4.0);
            let q = i.sqr();
            for k in 0..=8 {
                let x = i.lo + (i.hi - i.lo) * f64::from(k) / 8.0;
                if x <= i.hi {
                    assert!(holds(q, x * x), "{i:?} {x}");
                }
            }
        }
    }

    #[test]
    fn wide_and_odd_intervals() {
        let unit = Ivl::new(-1.0, 1.0);
        assert_eq!(Trig::sin_cos(Ivl::new(0.0, 7.0)), (unit, unit));
        assert_eq!(Trig::sin_cos(Ivl::new(1e300, 1e300)), (unit, unit));
        assert_eq!(Trig::sin_cos(Ivl::new(0.0, f64::INFINITY)), (unit, unit));
        assert_eq!(Trig::sin_cos(Ivl::new(f64::NAN, 1.0)).0, Ivl::whole());
        assert_eq!(-Ivl::new(1.0, 2.0), Ivl::new(-2.0, -1.0));
    }
}
