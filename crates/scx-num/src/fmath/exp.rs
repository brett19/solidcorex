// Written for solidcorex. Apache-2.0.
//! `exp` and `exp2`, and the double-double exponential that `powf` and the hyperbolic
//! functions build on.
//!
//! Reduction: `x = k ln(2)/128 + r` with `k` an integer and `|r| <= ln(2)/256`, so
//! `e^x = 2^(k/128) e^r`; the table holds `2^(j/128)` for `j = k mod 128` as a
//! double-double, and `e^r - 1` is a polynomial to `r^6` (truncation below `2^-71`).

use super::accurate;
use super::dd::{at, round_int, round_scaled_or};
use super::tables::{EXP2_J, INV_LN2_128, LN2, LN2_128_HI, LN2_128_LO};
use crate::eft::{fast_two_sum, two_prod, two_sum};

/// `e^(rh + rl) * 2^(k/128)` as `((hi, lo), m)`, meaning `(hi + lo) * 2^m`, for
/// `|rh| <= ln(2)/256` (a little over is harmless) and any `k`. `hi` lies in `[0.99, 2.01]`.
pub(super) fn exp_reduced(k: i32, rh: f64, rl: f64) -> ((f64, f64), i32) {
    // e^r - 1 = r + r^2/2 + r^3/6 + ... + r^6/720 (the next term is below 2^-71).
    let q = rh
        * rh
        * (0.5 + rh * (1.0 / 6.0 + rh * (1.0 / 24.0 + rh * (1.0 / 120.0 + rh * (1.0 / 720.0)))));
    let (ph, pl) = fast_two_sum(rh, rl + q + rh * rl);
    let (th, tl) = at(&EXP2_J, (k & 127) as usize);
    // 2^(j/128) (1 + p) = th + (tl + t p).
    let (a, ae) = two_prod(th, ph);
    let lo = tl + th * pl + tl * ph + ae;
    let (hi, e1) = fast_two_sum(th, a);
    (fast_two_sum(hi, e1 + lo), k >> 7)
}

/// `e^(xh + xl)` as `((hi, lo), m)`, meaning `(hi + lo) * 2^m`, for a double-double with
/// `|xh| < 1400`. Relative error below `2^-68` (the largest seen is `2^-69.2`).
pub(super) fn exp_core(xh: f64, xl: f64) -> ((f64, f64), i32) {
    let k = round_int(xh * INV_LN2_128);
    // k * LN2_128_HI is exact (35 + 18 bits) and xh - k * LN2_128_HI is exact
    // (Sterbenz), so r = xh + xl - k ln(2)/128 to about 2^-78.
    let r1 = xh - k * LN2_128_HI;
    let (p, pe) = two_prod(k, LN2_128_LO);
    let (s, e) = two_sum(r1, -p);
    let (rh, rl) = fast_two_sum(s, e - pe + xl);
    exp_reduced(k as i32, rh, rl)
}

/// `e^x`.
pub(super) fn exp(x: f64) -> f64 {
    if x.is_nan() {
        return f64::NAN;
    }
    if x > 710.0 {
        return f64::INFINITY;
    }
    if x < -746.0 {
        return 0.0;
    }
    let ((hi, lo), m) = exp_core(x, 0.0);
    round_scaled_or(hi, lo, EXP_ERR, m, || accurate::exp(x))
}

/// The error radius of [`exp_core`] and [`exp_reduced`] for the rounding test, relative
/// (`hi` is positive): `2^-67`, twice the bound.
pub(super) const EXP_ERR: f64 = 6.776263578034403e-21; // 2^-67

/// `2^x`.
pub(super) fn exp2(x: f64) -> f64 {
    if x.is_nan() {
        return f64::NAN;
    }
    if x >= 1024.0 {
        return f64::INFINITY;
    }
    if x < -1076.0 {
        return 0.0;
    }
    // x = k/128 + f exactly, |f| <= 1/256; then r = f ln(2) as a double-double.
    let k = round_int(x * 128.0);
    let f = x - k * (1.0 / 128.0);
    let (rh, e) = two_prod(f, LN2.0);
    let (rh, rl) = fast_two_sum(rh, e + f * LN2.1);
    let ((hi, lo), m) = exp_reduced(k as i32, rh, rl);
    round_scaled_or(hi, lo, EXP_ERR, m, || accurate::exp2(x))
}
