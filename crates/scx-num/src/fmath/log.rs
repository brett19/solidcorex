// Written for solidcorex. Apache-2.0.
//! `ln`, `log2` and `log10`, and the double-double logarithm `powf` builds on.
//!
//! Reduction: `x = 2^e m`, with `m` in `[1, 2)` folded to `m/2` above `1 + 53/128` so
//! that `m` lies in `[0.70, 1.42)`. The top 7 bits of the mantissa pick `c` near `1/m`
//! from a table, so `r = m c - 1` is exact (one two-product) with `|r| < 1/128`, and
//! `ln(x) = e ln(2) - ln(c) + log1p(r)`, the series to `r^11`. The two intervals next
//! to 1 use `c = 1`, so `ln` keeps its relative accuracy near 1.

use super::dd::{self, at};
use super::tables::{
    INV_LN10, INV_LN2, LN2_HI42, LN2_LO42, LN_FOLD, LN_TAB, LOG10_2_HI42, LOG10_2_LO42,
};
use crate::eft::{fast_two_sum, two_prod, two_sum};

/// `ln(x) = e ln(2) + lm` for a finite positive `x`, as `(e, lm)` with `lm` a
/// double-double and `|lm| < 0.35`. Relative error of `lm` below `2^-70` (absolute below
/// `2^-74`).
fn split(x: f64) -> (f64, (f64, f64)) {
    let (x, bias) = if x < f64::MIN_POSITIVE {
        (x * dd::pow2(54), -54)
    } else {
        (x, 0)
    };
    let bits = x.to_bits();
    let mut e = ((bits >> 52) as i32) - 1023 + bias;
    let i = ((bits >> 45) & 127) as usize;
    let mut m = f64::from_bits((bits & 0x000f_ffff_ffff_ffff) | 0x3ff0_0000_0000_0000);
    if i >= LN_FOLD {
        m *= 0.5;
        e += 1;
    }
    let (c, nlc_hi, nlc_lo) = at(&LN_TAB, i);
    // r = m c - 1 exactly: m c lies within 1/128 of 1, so p - 1 is exact (Sterbenz).
    let (p, pe) = two_prod(m, c);
    let (rh, rl) = fast_two_sum(p - 1.0, pe);
    // log1p(r) = r - r^2/2 + r^3 (1/3 - r/4 + ... + r^8/11), the polynomial by Estrin's
    // scheme (a shorter dependency chain than Horner's).
    let (s, se) = two_prod(rh, rh);
    let s_lo = se + 2.0 * rh * rl;
    let s2 = s * s;
    let p01 = 1.0 / 3.0 - 0.25 * rh;
    let p23 = 0.2 - (1.0 / 6.0) * rh;
    let p45 = 1.0 / 7.0 - 0.125 * rh;
    let p67 = 1.0 / 9.0 - 0.1 * rh;
    let poly = (p01 + s * p23) + s2 * ((p45 + s * p67) + s2 * (1.0 / 11.0));
    let tail = s * rh * poly;
    let (a, ae) = two_sum(nlc_hi, rh);
    let (b, be) = two_sum(a, -0.5 * s);
    let lm = fast_two_sum(b, ae + be + nlc_lo + rl - 0.5 * s_lo + tail);
    (f64::from(e), lm)
}

/// The special values every logarithm shares: `Some` for NaN, negative, zero or infinite
/// input.
fn special(x: f64) -> Option<f64> {
    if x.is_nan() || x < 0.0 {
        Some(f64::NAN)
    } else if x == 0.0 {
        Some(f64::NEG_INFINITY)
    } else if x == f64::INFINITY {
        Some(f64::INFINITY)
    } else {
        None
    }
}

/// `ln(x)` as a double-double for a finite positive `x`. Absolute error below `2^-74`
/// plus `2^-70` relative.
pub(super) fn ln_dd(x: f64) -> (f64, f64) {
    let (e, (lh, ll)) = split(x);
    // e LN2_HI42 is exact (|e| < 2^11).
    let (s, se) = two_sum(e * LN2_HI42, lh);
    fast_two_sum(s, se + ll + e * LN2_LO42)
}

/// `ln(x)`.
pub(super) fn ln(x: f64) -> f64 {
    special(x).unwrap_or_else(|| ln_dd(x).0)
}

/// `log2(x)`.
pub(super) fn log2(x: f64) -> f64 {
    if let Some(v) = special(x) {
        return v;
    }
    let (e, lm) = split(x);
    let (h, l) = dd::mul(lm, INV_LN2);
    let (s, se) = two_sum(e, h);
    s + (se + l)
}

/// `log10(x)`.
pub(super) fn log10(x: f64) -> f64 {
    if let Some(v) = special(x) {
        return v;
    }
    let (e, lm) = split(x);
    let (h, l) = dd::mul(lm, INV_LN10);
    let (s, se) = two_sum(e * LOG10_2_HI42, h);
    s + (se + l + e * LOG10_2_LO42)
}
