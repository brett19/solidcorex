// Written for solidcorex. Apache-2.0.
//! `sinh`, `cosh`, both at once, and `tanh`, from the double-double exponential; near zero,
//! from their series.

use super::dd::{self, round_scaled};
use super::exp::exp_core;
use crate::eft::{fast_two_sum, two_prod};

/// Below this, `sinh x` and `tanh x` round to `x` and `cosh x` to 1.
const TINY: f64 = 7.450580596923828e-9; // 2^-27

/// Below this, the series; above, `e^x` and `e^-x` (whose difference then loses at most
/// three bits).
const SERIES: f64 = 0.125;

/// Above this, `e^-x` is below `2^-104` of `e^x`.
const ONE_SIDED: f64 = 36.0;

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

/// `e^a / 2` rounded, for `a > ONE_SIDED`.
fn half_exp(a: f64) -> f64 {
    if a > 711.0 {
        return f64::INFINITY;
    }
    let ((h, l), m) = exp_core(a, 0.0);
    round_scaled(h, l, m - 1)
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
        sinh_series(a).0
    } else if a <= ONE_SIDED {
        let e = exp_dd(a);
        let r = dd::recip(e);
        0.5 * dd::add(e, (-r.0, -r.1)).0
    } else {
        half_exp(a)
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
        1.0
    } else if a <= ONE_SIDED {
        let e = exp_dd(a);
        0.5 * dd::add(e, dd::recip(e)).0
    } else {
        half_exp(a)
    }
}

/// `(sinh(x), cosh(x))`, each with the bits of [`sinh`] and [`cosh`], the exponential
/// and its reciprocal taken once for both.
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
        let c = 0.5 * dd::add(e, r).0;
        let s = if a < SERIES {
            sinh_series(a).0
        } else {
            0.5 * dd::add(e, (-r.0, -r.1)).0
        };
        (s, c)
    } else {
        let h = half_exp(a);
        (h, h)
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
        dd::div(sinh_series(a), cosh_series(a)).0
    } else {
        // 1 - 2 / (e^(2a) + 1).
        let e = exp_dd(2.0 * a);
        let d = dd::add(e, (1.0, 0.0));
        let q = dd::recip(d);
        dd::add((1.0, 0.0), (-2.0 * q.0, -2.0 * q.1)).0
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
