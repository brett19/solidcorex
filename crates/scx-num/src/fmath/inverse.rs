// Written for solidcorex. Apache-2.0.
//! `atan`, `atan2`, `asin` and `acos`, all on one double-double arctangent of a ratio
//! in `[0, 1]`.
//!
//! Kernel: `t = c + (t - c)` with `c = i/64` the nearest table point, and
//! `atan(t) = atan(c) + atan(d)`, `d = (t - c)/(1 + t c)`, `|d| <= 1/128`; `atan(d)` is
//! its series to `d^9` (truncation below `2^-73`).

use super::dd::{self, at, round_int};
use super::tables::{ATAN_TAB, PI, PI3O4, PIO2, PIO4};
use crate::eft::{fast_two_sum, two_prod, two_sum};

/// `atan(t)` as a double-double for a double-double `t` in `[0, 1 + 1/128)`.
fn atan_dd(t: (f64, f64)) -> (f64, f64) {
    let (th, tl) = t;
    let i = round_int(th * 64.0);
    let c = i * (1.0 / 64.0);
    // d = (t - c) / (1 + t c); th - c is exact.
    let num = fast_two_sum(th - c, tl);
    let (p, pe) = two_prod(th, c);
    let (s, se) = fast_two_sum(1.0, p);
    let den = fast_two_sum(s, se + pe + tl * c);
    finish(i, dd::div(num, den))
}

/// `atan(n/d)` as a double-double for double-doubles with `n/d` in `[0, 1 + 1/128)`
/// and `|d| >= 2^-500`, without forming the ratio: `d' = (n - c d)/(d + c n)`, one
/// division where `atan_dd(n/d)` would take two.
fn atan_ratio(n: (f64, f64), d: (f64, f64)) -> (f64, f64) {
    let i = round_int(n.0 / d.0 * 64.0);
    let c = i * (1.0 / 64.0);
    let (p, pe) = two_prod(c, d.0);
    let num = dd::add(n, (-p, -(pe + c * d.1)));
    let (q, qe) = two_prod(c, n.0);
    let den = dd::add(d, (q, qe + c * n.1));
    finish(i, dd::div(num, den))
}

/// `atan(i/64) + atan(d)` for `|d| <= 1/128`.
fn finish(i: f64, (dh, dl): (f64, f64)) -> (f64, f64) {
    let d2 = dh * dh;
    let tail = dh * d2 * (-1.0 / 3.0 + d2 * (0.2 + d2 * (-1.0 / 7.0 + d2 * (1.0 / 9.0))));
    let (ah, al) = at(&ATAN_TAB, i as usize);
    let (s, se) = two_sum(ah, dh);
    fast_two_sum(s, se + al + dl + tail)
}

/// `a - b` for two double-doubles.
fn sub(a: (f64, f64), b: (f64, f64)) -> (f64, f64) {
    dd::add(a, (-b.0, -b.1))
}

/// Below this, `atan x` and `asin x` round to `x`.
const TINY: f64 = 7.450580596923828e-9; // 2^-27

/// Below this, `atan2(y, x)` for `x > 0` is `y/x` rounded: `y/x` is itself rounded, so
/// the series' next term must be far below an ulp, not just below half of one.
const RATIO_TINY: f64 = 8.673_617_379_884_035e-19; // 2^-60

/// Above this, `atan x` rounds to `pi/2`.
const BIG: f64 = 1_152_921_504_606_846_976.0; // 2^60

/// `atan(x)`.
pub(super) fn atan(x: f64) -> f64 {
    if x.is_nan() {
        return f64::NAN;
    }
    let a = x.abs();
    if a < TINY {
        return x;
    }
    let r = if a > BIG {
        // pi/2 - 1/a rounds to pi/2.
        PIO2.0
    } else if a > 1.0 {
        // atan(a) = pi/2 - atan(1/a).
        sub(PIO2, atan_ratio((1.0, 0.0), (a, 0.0))).0
    } else {
        atan_dd((a, 0.0)).0
    };
    r.copysign(x)
}

/// `atan2(y, x)`, with C99's special values.
pub(super) fn atan2(y: f64, x: f64) -> f64 {
    if x.is_nan() || y.is_nan() {
        return f64::NAN;
    }
    let (ax, ay) = (x.abs(), y.abs());
    let x_neg = x.is_sign_negative();
    let r = if ay == 0.0 {
        if x_neg { PI.0 } else { 0.0 }
    } else if ax == 0.0 {
        PIO2.0
    } else if ay == f64::INFINITY {
        if ax == f64::INFINITY {
            if x_neg { PI3O4.0 } else { PIO4.0 }
        } else {
            PIO2.0
        }
    } else if ax == f64::INFINITY {
        if x_neg { PI.0 } else { 0.0 }
    } else if !x_neg && ay < ax * RATIO_TINY {
        // atan(t) = t (1 - t^2/3 + ...) with t^2/3 below 2^-121: atan(y/x) rounds as y/x
        // does (and y/x is correctly rounded, subnormal or not).
        return y / x;
    } else {
        atan2_finite(ay, ax, x_neg)
    };
    r.copysign(y)
}

/// `atan2(ay, ax)` for finite positive `ay` and `ax`, mirrored to `pi - ...` when `x`
/// was negative, before the sign of `y`.
fn atan2_finite(ay: f64, ax: f64, x_neg: bool) -> f64 {
    let (mn, mx) = if ay > ax { (ax, ay) } else { (ay, ax) };
    // Far from 1, scale the larger into [1, 2), so the error terms of the products cannot
    // underflow. The smaller may then round, below 2^-1022 of the larger, which is far
    // below what reaches the result.
    let e = dd::exponent(mx);
    let (mn, mx) = if (-300..=300).contains(&e) {
        (mn, mx)
    } else {
        (dd::scale(mn, -e), dd::scale(mx, -e))
    };
    let mut th = atan_ratio((mn, 0.0), (mx, 0.0));
    if ay > ax {
        th = sub(PIO2, th);
    }
    if x_neg {
        th = sub(PI, th);
    }
    th.0
}

/// `sqrt(1 - a^2)` as a double-double for `a` in `[0, 1]`, with `a^2` exact.
fn cos_of_sin(a: f64) -> ((f64, f64), f64) {
    let (p, pe) = two_prod(a, a);
    let w = dd::add(two_sum(1.0, -p), (-pe, 0.0));
    (if w.0 <= 0.0 { (0.0, 0.0) } else { dd::sqrt(w) }, p)
}

/// `asin(x)`.
pub(super) fn asin(x: f64) -> f64 {
    let a = x.abs();
    if x.is_nan() || a > 1.0 {
        return f64::NAN;
    }
    if a < TINY {
        return x;
    }
    let (c, p) = cos_of_sin(a);
    let r = if p <= 0.5 {
        atan_ratio((a, 0.0), c)
    } else {
        sub(PIO2, atan_ratio(c, (a, 0.0)))
    };
    r.0.copysign(x)
}

/// `acos(x)`.
pub(super) fn acos(x: f64) -> f64 {
    let a = x.abs();
    if x.is_nan() || a > 1.0 {
        return f64::NAN;
    }
    let (c, p) = cos_of_sin(a);
    // acos(a) for a = |x|, then pi - that for negative x.
    let r = if p <= 0.5 {
        sub(PIO2, atan_ratio((a, 0.0), c))
    } else {
        atan_ratio(c, (a, 0.0))
    };
    if x < 0.0 { sub(PI, r).0 } else { r.0 }
}
