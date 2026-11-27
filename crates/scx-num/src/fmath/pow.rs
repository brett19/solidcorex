// Written for solidcorex. Apache-2.0.
//! `powi` and `powf`.

use super::dd::{self, round_scaled};
use super::exp::exp_core;
use super::log::ln_dd;
use crate::eft::{fast_two_sum, two_prod};

/// `2^-400` and `2^400`, the band `powi` keeps its running products in.
const BELOW: f64 = 3.8725919148493183e-121;
const ABOVE: f64 = 2.5822498780869086e120;

/// `x^n` by squaring, the running products kept as double-doubles with their exponents
/// apart, so nothing overflows or underflows before the one final rounding.
pub(super) fn powi(x: f64, n: i32) -> f64 {
    if n == 0 {
        return 1.0;
    }
    if x.is_nan() {
        return f64::NAN;
    }
    match n {
        1 => return x,
        2 => return x * x,
        -1 => return 1.0 / x,
        _ => {}
    }
    let odd = n & 1 == 1;
    let neg = x.is_sign_negative() && odd;
    let a = x.abs();
    if a == 0.0 || a == f64::INFINITY {
        let big = (a == 0.0) == (n < 0);
        let r = if big { f64::INFINITY } else { 0.0 };
        return if neg { -r } else { r };
    }
    // a = m 2^e with m in [1, 2), exactly (also for e = 1023 and the subnormals).
    let mut base = (dd::mantissa(a), 0.0);
    let mut base_e = i64::from(dd::exponent(a));
    let mut acc = (1.0, 0.0);
    let mut acc_e = 0i64;
    let mut k = n.unsigned_abs();
    // Renormalise a double-double to hi in [1, 2), moving the power of two out, once it
    // leaves [2^-400, 2^400] (a product of two such stays far inside the normal range).
    let norm = |v: (f64, f64), ve: i64| -> ((f64, f64), i64) {
        if (BELOW..=ABOVE).contains(&v.0) {
            return (v, ve);
        }
        let s = dd::exponent(v.0);
        let p = dd::pow2(-s);
        ((v.0 * p, v.1 * p), ve + i64::from(s))
    };
    loop {
        if k & 1 == 1 {
            (acc, acc_e) = norm(dd::mul(acc, base), acc_e + base_e);
        }
        k >>= 1;
        if k == 0 {
            break;
        }
        (base, base_e) = norm(dd::mul(base, base), 2 * base_e);
    }
    if n < 0 {
        // 1/(acc 2^acc_e), with 1/acc inside the band again.
        acc = dd::recip(acc);
        acc_e = -acc_e;
    }
    let r = round_scaled(acc.0, acc.1, acc_e.clamp(-4000, 4000) as i32);
    if neg { -r } else { r }
}

/// Whether a finite `y` is an integer, and whether it is odd.
fn integer_parity(y: f64) -> (bool, bool) {
    if y != y.trunc() {
        (false, false)
    } else if y.abs() >= 9_007_199_254_740_992.0 {
        (true, false)
    } else {
        (true, (y as i64) & 1 == 1)
    }
}

/// `x^y`, with C99's special values.
pub(super) fn powf(x: f64, y: f64) -> f64 {
    if y == 0.0 || x == 1.0 {
        return 1.0;
    }
    if x.is_nan() || y.is_nan() {
        return f64::NAN;
    }
    let a = x.abs();
    if y.is_infinite() {
        return if a == 1.0 {
            1.0
        } else if (a < 1.0) == (y > 0.0) {
            0.0
        } else {
            f64::INFINITY
        };
    }
    let (int, odd) = integer_parity(y);
    let neg = x.is_sign_negative() && odd;
    let sign = |r: f64| if neg { -r } else { r };
    if a == 0.0 || a == f64::INFINITY {
        let big = (a == 0.0) == (y < 0.0);
        return sign(if big { f64::INFINITY } else { 0.0 });
    }
    if x < 0.0 && !int {
        return f64::NAN;
    }
    // z = y ln(a) as a double-double, then e^z.
    let (lh, ll) = ln_dd(a);
    // Range first, so no product below overflows: e^710 > MAX and e^-746 < 2^-1075.
    let z = y * lh;
    if z > 710.0 {
        return sign(f64::INFINITY);
    }
    if z < -746.0 {
        return sign(0.0);
    }
    let (zh, ze) = two_prod(y, lh);
    let (zh, zl) = fast_two_sum(zh, ze + y * ll);
    let ((h, l), m) = exp_core(zh, zl);
    sign(round_scaled(h, l, m))
}
