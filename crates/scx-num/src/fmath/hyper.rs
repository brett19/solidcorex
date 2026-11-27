// Written for solidcorex. Apache-2.0.
//! `sinh`, `cosh`, both at once, and `tanh`, from the double-double exponential; near zero,
//! from their series.

use super::accurate;
use super::dd::{self, round_ball_or, round_or, round_scaled_or};
use super::exp::{EXP_ERR, exp_core};
use crate::eft::{fast_two_sum, two_prod};

/// Below this, `sinh x` and `tanh x` round to `x` and `cosh x` to 1.
const TINY: f64 = 7.450580596923828e-9; // 2^-27

/// Below this, the series; above, `e^x` and `e^-x` (whose difference then loses at most
/// three bits).
const SERIES: f64 = 0.125;

/// Above this, `e^-x` is below `2^-104` of `e^x`.
const ONE_SIDED: f64 = 36.0;

/// The rounding test's error radii (`docs/research/70-fmath-correct-rounding.md`), each
/// at least twice its bound: the series below `2^-69` relative (the doubles' tail, about
/// `2^-19` of the result, to `2^-52` of itself), their quotient below `2^-67.5`, and `cosh`
/// through `e^a` below `2^-68` (it adds and loses nothing). `sinh` and `tanh` through
/// `e^a` state theirs absolutely, from `e^a`'s `2^-68` (`EXP_ERR` is twice it): their
/// subtraction cancels, by up to `coth a < 8.1` and `2q / tanh a < 7.1`.
const SERIES_ERR: f64 = 6.776263578034403e-21; // 2^-67
const TANH_SERIES_ERR: f64 = 1.3552527156068805e-20; // 2^-66
const COSH_ERR: f64 = 6.776263578034403e-21; // 2^-67
/// The double-double sums' and reciprocals' part, `2^-100`, twice over.
const DD_ERR: f64 = 1.5777218104420236e-30; // 2^-99

/// `1/6` as a double-double.
const SIXTH: (f64, f64) = (1.0 / 6.0, 9.251_858_538_542_97e-18);

/// `sinh(x)` for `|x| < 1/8` as a double-double: `x + x^3/6` in double-double, the
/// rest of the series to `x^13/13!` in doubles (truncation below `2^-68` relative).
fn sinh_series(x: f64) -> (f64, f64) {
    let x2 = two_prod(x, x);
    let x3 = dd::mul(x2, (x, 0.0));
    let lead = dd::mul(x3, SIXTH);
    let t = x2.0;
    let rest = x3.0
        * t
        * (1.0 / 120.0
            + t * (1.0 / 5040.0
                + t * (1.0 / 362_880.0 + t * (1.0 / 39_916_800.0 + t * (1.0 / 6_227_020_800.0)))));
    let (s, e) = fast_two_sum(x, lead.0);
    fast_two_sum(s, e + lead.1 + rest)
}

/// `cosh(x)` for `|x| < 1/8` as a double-double: `1 + x^2/2` in double-double, the rest
/// of the series to `x^12/12!` in doubles.
fn cosh_series(x: f64) -> (f64, f64) {
    let (th, tl) = two_prod(x, x);
    let rest = th
        * th
        * (1.0 / 24.0
            + th * (1.0 / 720.0
                + th * (1.0 / 40_320.0 + th * (1.0 / 3_628_800.0 + th * (1.0 / 479_001_600.0)))));
    let (s, e) = fast_two_sum(1.0, 0.5 * th);
    fast_two_sum(s, e + 0.5 * tl + rest)
}

/// `e^a` as a double-double, for `0 <= a <= 64`.
fn exp_dd(a: f64) -> (f64, f64) {
    let ((h, l), m) = exp_core(a, 0.0);
    let p = dd::pow2(m);
    (h * p, l * p)
}

/// `e^a / 2` rounded, for `a > ONE_SIDED` (`cosh a`, and `sinh a` for `a > 0`).
fn half_exp(a: f64, accurate: impl FnOnce() -> f64) -> f64 {
    if a > 711.0 {
        return f64::INFINITY;
    }
    let ((h, l), m) = exp_core(a, 0.0);
    round_scaled_or(h, l, EXP_ERR, m - 1, accurate)
}

/// `sinh(x)`.
pub(super) fn sinh(x: f64) -> f64 {
    if x.is_nan() {
        return f64::NAN;
    }
    if x.is_infinite() {
        return x;
    }
    let a = x.abs();
    if a < TINY {
        return x;
    }
    let r = if a < SERIES {
        let (h, l) = sinh_series(a);
        round_or(h, l, SERIES_ERR, || accurate::sinh(a))
    } else if a <= ONE_SIDED {
        let e = exp_dd(a);
        let r = dd::recip(e);
        let (h, l) = dd::add(e, (-r.0, -r.1));
        // e^a and e^-a each within 2^-68 (relative), the sum within 2^-104.
        let err = EXP_ERR * 0.5 * (e.0 + r.0) + DD_ERR * h;
        round_ball_or(0.5 * h, 0.5 * l, err, || accurate::sinh(a))
    } else {
        half_exp(a, || accurate::sinh(a))
    };
    r.copysign(x)
}

/// `cosh(x)`.
pub(super) fn cosh(x: f64) -> f64 {
    if x.is_nan() {
        return f64::NAN;
    }
    let a = x.abs();
    if a < TINY {
        return 1.0;
    }
    if a <= ONE_SIDED {
        let e = exp_dd(a);
        let (h, l) = dd::add(e, dd::recip(e));
        round_or(0.5 * h, 0.5 * l, COSH_ERR, || accurate::cosh(a))
    } else {
        half_exp(a, || accurate::cosh(a))
    }
}

/// `(sinh(x), cosh(x))`, each with the bits of [`sinh`] and [`cosh`] (the same rounding
/// tests and fallbacks), the exponential and its reciprocal taken once for both.
pub(super) fn sinh_cosh(x: f64) -> (f64, f64) {
    if x.is_nan() {
        return (f64::NAN, f64::NAN);
    }
    if x.is_infinite() {
        return (x, f64::INFINITY);
    }
    let a = x.abs();
    if a < TINY {
        return (x, 1.0);
    }
    let (s, c) = if a <= ONE_SIDED {
        let e = exp_dd(a);
        let r = dd::recip(e);
        let (h, l) = dd::add(e, r);
        let c = round_or(0.5 * h, 0.5 * l, COSH_ERR, || accurate::cosh(a));
        let s = if a < SERIES {
            let (h, l) = sinh_series(a);
            round_or(h, l, SERIES_ERR, || accurate::sinh(a))
        } else {
            let (h, l) = dd::add(e, (-r.0, -r.1));
            let err = EXP_ERR * 0.5 * (e.0 + r.0) + DD_ERR * h;
            round_ball_or(0.5 * h, 0.5 * l, err, || accurate::sinh(a))
        };
        (s, c)
    } else if a > 711.0 {
        (f64::INFINITY, f64::INFINITY)
    } else {
        // Both `e^a / 2` ([`half_exp`]), each with its own fallback.
        let ((h, l), m) = exp_core(a, 0.0);
        (
            round_scaled_or(h, l, EXP_ERR, m - 1, || accurate::sinh(a)),
            round_scaled_or(h, l, EXP_ERR, m - 1, || accurate::cosh(a)),
        )
    };
    (s.copysign(x), c)
}

/// `tanh(x)`.
pub(super) fn tanh(x: f64) -> f64 {
    if x.is_nan() {
        return f64::NAN;
    }
    let a = x.abs();
    if a < TINY {
        return x;
    }
    let r = if a > 22.0 {
        // 1 - tanh(a) < 2e^-44 < 2^-54.
        1.0
    } else if a < SERIES {
        // sinh / cosh, both from their series.
        let (h, l) = dd::div(sinh_series(a), cosh_series(a));
        round_or(h, l, TANH_SERIES_ERR, || accurate::tanh(a))
    } else {
        // 1 - 2 / (e^(2a) + 1).
        let e = exp_dd(2.0 * a);
        let d = dd::add(e, (1.0, 0.0));
        let q = dd::recip(d);
        let (h, l) = dd::add((1.0, 0.0), (-2.0 * q.0, -2.0 * q.1));
        // q within 2^-68 of 1/(e^(2a) + 1) (relative), the rest within 2^-100.
        let err = EXP_ERR * 2.0 * q.0 + DD_ERR * h;
        round_ball_or(h, l, err, || accurate::tanh(a))
    };
    r.copysign(x)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hash::Rng;

    fn same(x: f64) {
        let (s, c) = sinh_cosh(x);
        assert_eq!(s.to_bits(), sinh(x).to_bits(), "sinh {x:e}");
        assert_eq!(c.to_bits(), cosh(x).to_bits(), "cosh {x:e}");
    }

    #[test]
    fn sinh_cosh_has_the_bits_of_each() {
        let edges = [TINY, SERIES, ONE_SIDED, 711.0, 710.0];
        for e in edges {
            for x in [e, e.next_up(), e.next_down()] {
                same(x);
                same(-x);
            }
        }
        for x in [
            0.0,
            -0.0,
            1e-300,
            5e-324,
            f64::INFINITY,
            -f64::INFINITY,
            1e300,
        ] {
            same(x);
        }
        assert!(sinh_cosh(f64::NAN).0.is_nan() && sinh_cosh(f64::NAN).1.is_nan());
        let mut r = Rng::new(3);
        for _ in 0..20_000 {
            let x = r.next_signed_unit() * crate::fmath::exp2(r.next_signed_unit() * 12.0);
            same(x);
        }
    }
}
