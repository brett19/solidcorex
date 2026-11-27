// Written for solidcorex. Apache-2.0.
//! Sine and cosine over a ring: the one evaluator of an analytic curve or surface runs in
//! `f64` for values and in [`Ivl`] for enclosures (research 63 §3.2, "the scalar-generic
//! form").
//!
//! - `f64`: [`crate::fmath::sin_cos`], correctly rounded.
//! - [`Ivl`]: an enclosure of `{sin x, cos x : x in the interval}` for any interval. The
//!   ring's `sin_cos_s` ([`crate::ring::TayScalar`]) is the fork's, valid on `[0, pi/2]`
//!   only; this one holds everywhere.
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

use core::ops::Neg;

use crate::interval::{Ivl, cmax, cmin};
use crate::math::HALF_PI;
use crate::ring::{Ring, wide};

/// A ring with sine and cosine: `f64` and [`Ivl`].
pub trait Trig: Ring + Neg<Output = Self> {
    /// `(sin x, cos x)`; for an interval, an enclosure of both over the interval.
    fn sin_cos(self) -> (Self, Self);
}

impl Trig for f64 {
    #[inline]
    fn sin_cos(self) -> (Self, Self) {
        crate::fmath::sin_cos(self)
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
            let w = (r.next_signed_unit().abs() * 8.0) * if r.next_signed_unit() > 0.0 { 1.0 } else { 1e-6 };
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
    fn wide_and_odd_intervals() {
        let unit = Ivl::new(-1.0, 1.0);
        assert_eq!(Trig::sin_cos(Ivl::new(0.0, 7.0)), (unit, unit));
        assert_eq!(Trig::sin_cos(Ivl::new(1e300, 1e300)), (unit, unit));
        assert_eq!(Trig::sin_cos(Ivl::new(0.0, f64::INFINITY)), (unit, unit));
        assert_eq!(Trig::sin_cos(Ivl::new(f64::NAN, 1.0)).0, Ivl::whole());
        assert_eq!(-Ivl::new(1.0, 2.0), Ivl::new(-2.0, -1.0));
    }
}
