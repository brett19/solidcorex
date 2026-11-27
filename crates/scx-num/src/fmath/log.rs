// Written for solidcorex. Apache-2.0.
//! `ln`, `log2` and `log10`, and the double-double logarithm `powf` builds on.
//!
//! Reduction: `x = 2^e m`, with `m` in `[1, 2)` folded to `m/2` above `1 + 53/128` so
//! that `m` lies in `[0.70, 1.42)`. The top 7 bits of the mantissa pick `c` near `1/m`
//! from a table, so `r = m c - 1` is exact (one two-product) with `|r| < 1/128`, and
//! `ln(x) = e ln(2) - ln(c) + log1p(r)`, the series to `r^11`. The two intervals next
//! to 1 use `c = 1`, so `ln` keeps its relative accuracy near 1.

use super::accurate;
use super::dd::{self, at};
use super::tables::{
    INV_LN2, INV_LN10, LN_FOLD, LN_TAB, LN2_HI42, LN2_LO42, LOG10_2_HI42, LOG10_2_LO42,
};
use crate::eft::{fast_two_sum, two_prod, two_sum};

/// `ln(x) = e ln(2) + lm` for a finite positive `x`, as `(e, lm)` with `lm` a
/// double-double and `|lm| < 0.35`. Its error is below `2^-73.5` absolute plus `2^-100`
/// relative, and below `2^-66.5` relative for `c = 1` (where `|lm| < 2^-7`, the other
/// intervals giving `|lm| >= ln(1 + 1/128) > 2^-7.01`): the tail `r^3 (1/3 - ...)`,
/// at most `0.34 |r|^3`, is evaluated in doubles to `2^-51` of itself, so within
/// `2^-52.5 |r|^3`, below `2^-73.5` and below `2^-66.5 |r|` for `|r| < 2^-7` (the largest
/// seen: `2^-74.7` and `2^-66.95`).
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

/// `ln(x)` as a double-double for a finite positive `x`. Error as [`split`]'s, the `e ln 2`
/// part adding below `2^-86`.
pub(super) fn ln_dd(x: f64) -> (f64, f64) {
    let (e, lm) = split(x);
    ln_join(e, lm)
}

/// `e ln 2 + lm`.
#[inline]
fn ln_join(e: f64, (lh, ll): (f64, f64)) -> (f64, f64) {
    // e LN2_HI42 is exact (|e| < 2^11).
    let (s, se) = two_sum(e * LN2_HI42, lh);
    fast_two_sum(s, se + ll + e * LN2_LO42)
}

/// The rounded logarithm `h + l` (`e` from [`split`]), else `accurate()`, the test's radius
/// twice [`split`]'s bound carried through (`abs`: its `2^-73.5`, twice, in the
/// function's units), plus `2^-98 |h|` for the double-double products and sums.
///
/// For `e != 0`, `|h| >= 0.149` (`|lm| < 0.35`; log10's `log10(2) - 0.35/ln 10` is the
/// smallest), so `abs <= 2^-70.93 |h|` for each of the three, and the relative test with
/// `2^-70.75` holds it. For `e = 0`, `|h| < 2^-9` only for `c = 1` (`|lm| < 2^-7.8`,
/// the other intervals giving `|lm| > 2^-7.01`), where the bound is relative.
///
/// log2 passes `(s, se + l)` unnormalised: for `e = 0` it is `(h, l)` (the
/// `two_sum` with zero is exact), and for `e != 0` `|se + l| < 2^-51 |s|`. Neither
/// test needs a normalised pair: [`dd::rounds_to_hi`]'s derivation bounds `lo` from the
/// test itself, and the ball's `lo -+ err` rounds within `2^-104 |s|`, inside the
/// factor-two margin.
#[inline]
fn round_log(h: f64, l: f64, e: f64, abs: f64, accurate: impl FnOnce() -> f64) -> f64 {
    const FAR: f64 = 5.036488037582692e-22; // 2^-70.75
    const NEAR: f64 = 1.9166167708542177e-20; // 2^-65.5
    const REL: f64 = 3.1554436208840472e-30; // 2^-98
    if e != 0.0 {
        return dd::round_or(h, l, FAR, accurate);
    }
    let a = h.abs();
    let err = if a < 0.001953125 {
        NEAR * a
    } else {
        abs + REL * a
    };
    dd::round_ball_or(h, l, err, accurate)
}

/// `ln(x)`.
pub(super) fn ln(x: f64) -> f64 {
    if let Some(v) = special(x) {
        return v;
    }
    let (e, lm) = split(x);
    let (h, l) = ln_join(e, lm);
    round_log(h, l, e, LN_ABS, || accurate::ln(x))
}

/// Twice [`split`]'s `2^-73.5` for ln, and that divided by `ln 2` and `ln 10` (rounded
/// up) for log2 and log10.
const LN_ABS: f64 = 1.4973568522298576e-22; // 2^-72.5
const LOG2_ABS: f64 = 2.2e-22; // 2^-71.94
const LOG10_ABS: f64 = 6.6e-23; // 2^-73.68

/// `log2(x)`.
pub(super) fn log2(x: f64) -> f64 {
    if let Some(v) = special(x) {
        return v;
    }
    let (e, lm) = split(x);
    let (h, l) = dd::mul(lm, INV_LN2);
    let (s, se) = two_sum(e, h);
    round_log(s, se + l, e, LOG2_ABS, || accurate::log2(x))
}

/// `log10(x)`.
pub(super) fn log10(x: f64) -> f64 {
    if let Some(v) = special(x) {
        return v;
    }
    let (e, lm) = split(x);
    let (h, l) = dd::mul(lm, INV_LN10);
    let (s, se) = two_sum(e * LOG10_2_HI42, h);
    let (s, se) = fast_two_sum(s, se + l + e * LOG10_2_LO42);
    round_log(s, se, e, LOG10_ABS, || accurate::log10(x))
}
