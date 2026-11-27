// Written for solidcorex. Apache-2.0.
//! `sin`, `cos`, `sin_cos` and `tan`.
//!
//! Reduction to `x = n pi/2 + y` with `|y| <= pi/4` (a hair over is harmless): below
//! `2^20` by Cody-Waite with `pi/2` in four pieces (152 bits); above, by Payne-Hanek with
//! a 192-bit window of the bits of `2/pi` and exact integer products. Either way `y` is
//! a double-double good to about `2^-120` absolute, and no double lies closer than about
//! `2^-62` to a multiple of `pi/2`, so `y` keeps over 60 bits of relative accuracy.
//!
//! Kernel: `y = i/64 + h` with `|h| <= 1/128`; `sin` and `cos` of `i/64` come from a
//! double-double table, and of `h` from short series.

use super::dd::{self, at, round_int};
use super::tables::{INV_PIO2, PIO2, PIO2_1, PIO2_2, PIO2_3, PIO2_4, SINCOS_TAB, TWO_OVER_PI};
use crate::eft::{fast_two_sum, two_prod, two_sum};

/// `(n mod 4, y)` with `x = n pi/2 + y`, for finite `x`.
pub(super) fn reduce(x: f64) -> (i32, (f64, f64)) {
    if x.abs() < 1_048_576.0 {
        // k * PIO2_1, k * PIO2_2 and k * PIO2_3 are exact (33 + 20 bits), and so is
        // x - k * PIO2_1 (Sterbenz).
        let k = round_int(x * INV_PIO2);
        let y = x - k * PIO2_1;
        let (a, ae) = two_sum(y, -k * PIO2_2);
        let (b, be) = two_sum(a, -k * PIO2_3);
        let yy = two_sum(b, ae + be - k * PIO2_4);
        return ((k as i64 & 3) as i32, yy);
    }
    let (n, y) = payne_hanek(x.abs());
    if x < 0.0 {
        (-n & 3, (-y.0, -y.1))
    } else {
        (n, y)
    }
}

/// Payne-Hanek reduction of a finite `x >= 2^20`.
fn payne_hanek(x: f64) -> (i32, (f64, f64)) {
    // x = mant * 2^ex, mant an integer below 2^53, ex >= -32.
    let bits = x.to_bits();
    let ex = ((bits >> 52) as i32) - 1075;
    let mant = u128::from((bits & 0x000f_ffff_ffff_ffff) | 0x0010_0000_0000_0000);
    // x * 2/pi mod 4 needs the bits of 2/pi of weight 2^(1 - ex) down: global bit index
    // g = ex + 62 counts from the top of word 0 (weight 2^63).
    let g = (ex + 62) as usize;
    let (q, sh) = (g / 64, (g % 64) as u32);
    let word = |i: usize| -> u64 {
        let hi = at(&TWO_OVER_PI, q + i);
        if sh == 0 {
            hi
        } else {
            (hi << sh) | (at(&TWO_OVER_PI, q + i + 1) >> (64 - sh))
        }
    };
    let (w2, w1, w0) = (word(0), word(1), word(2));
    // mant * (w2:w1:w0) mod 2^192, as r2:r1:r0; the value is that times 2^-190.
    let p0 = mant * u128::from(w0);
    let p1 = mant * u128::from(w1) + (p0 >> 64);
    let p2 = mant * u128::from(w2) + (p1 >> 64);
    let (r2, r1, r0) = (p2 as u64, p1 as u64, p0 as u64);
    // The top two bits are n mod 4; the next 128 are the fraction, read as signed so a
    // fraction of a half or more rounds n up.
    let top = (u128::from(r2) << 64) | u128::from(r1);
    let frac = ((top << 2) | u128::from(r0 >> 62)) as i128;
    let n = ((top >> 126) as i32 + i32::from(frac < 0)) & 3;
    let hi = frac as f64;
    let lo = (frac - hi as i128) as f64;
    let s = dd::pow2(-128);
    (n, dd::mul((hi * s, lo * s), PIO2))
}

/// `(sin y, cos y)` as double-doubles, for `|y| <= 0.8`.
fn kernel(y: (f64, f64)) -> ((f64, f64), (f64, f64)) {
    let neg = y.0 < 0.0;
    let (yh, yl) = if neg { (-y.0, -y.1) } else { y };
    let i = round_int(yh * 64.0);
    let h = yh - i * (1.0 / 64.0);
    let (sh, sl, ch, cl) = at(&SINCOS_TAB, i as usize);
    // sin(h + yl) - h and cos(h + yl) - 1, to about 2^-75.
    let (h2, h2e) = two_prod(h, h);
    let ds =
        h * h2 * (-1.0 / 6.0 + h2 * (1.0 / 120.0 - h2 * (1.0 / 5040.0))) + yl * (1.0 - 0.5 * h2);
    let dch = -0.5 * h2;
    let dcl = -0.5 * h2e + h2 * h2 * (1.0 / 24.0 - h2 * (1.0 / 720.0)) - h * yl;
    let dc = dch + dcl;
    // sin(a + h) = sin a + sin a (cos h - 1) + cos a sin h.
    let (p, pe) = two_prod(ch, h);
    let (s, se) = two_sum(sh, p);
    let (q, qe) = two_prod(sh, dch);
    let (s2, s2e) = two_sum(s, q);
    let s_lo = se + pe + s2e + qe + sl + cl * h + ch * ds + sh * dcl + sl * dc;
    let sin = fast_two_sum(s2, s_lo);
    // cos(a + h) = cos a + cos a (cos h - 1) - sin a sin h.
    let (p, pe) = two_prod(sh, -h);
    let (c, ce) = two_sum(ch, p);
    let (q, qe) = two_prod(ch, dch);
    let (c2, c2e) = two_sum(c, q);
    let c_lo = ce + pe + c2e + qe + cl - sl * h - sh * ds + ch * dcl + cl * dc;
    let cos = fast_two_sum(c2, c_lo);
    if neg {
        ((-sin.0, -sin.1), cos)
    } else {
        (sin, cos)
    }
}

/// `(sin x, cos x)` as double-doubles, for finite `x`.
fn sin_cos_dd(x: f64) -> ((f64, f64), (f64, f64)) {
    let (n, y) = reduce(x);
    let (s, c) = kernel(y);
    let neg = |v: (f64, f64)| (-v.0, -v.1);
    match n {
        0 => (s, c),
        1 => (c, neg(s)),
        2 => (neg(s), neg(c)),
        _ => (neg(c), s),
    }
}

/// Below this, `sin x` rounds to `x` and `cos x` to 1.
const TINY: f64 = 7.450580596923828e-9; // 2^-27

/// `sin(x)`.
pub(super) fn sin(x: f64) -> f64 {
    if !x.is_finite() {
        return f64::NAN;
    }
    if x.abs() < TINY {
        return x;
    }
    sin_cos_dd(x).0.0
}

/// `cos(x)`.
pub(super) fn cos(x: f64) -> f64 {
    if !x.is_finite() {
        return f64::NAN;
    }
    if x.abs() < TINY {
        return 1.0;
    }
    sin_cos_dd(x).1.0
}

/// `(sin x, cos x)`.
pub(super) fn sin_cos(x: f64) -> (f64, f64) {
    if !x.is_finite() {
        return (f64::NAN, f64::NAN);
    }
    if x.abs() < TINY {
        return (x, 1.0);
    }
    let (s, c) = sin_cos_dd(x);
    (s.0, c.0)
}

/// `tan(x)`.
pub(super) fn tan(x: f64) -> f64 {
    if !x.is_finite() {
        return f64::NAN;
    }
    if x.abs() < TINY {
        return x;
    }
    let (s, c) = sin_cos_dd(x);
    dd::div(s, c).0
}
