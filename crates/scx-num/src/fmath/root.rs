// Written for solidcorex. Apache-2.0.
//! `cbrt` and `hypot`.

use super::dd::{self, round_scaled};
use super::tables::{CBRT2, CBRT4};
use crate::eft::{fast_two_sum, two_prod};

/// `cbrt(x)`: `x = 2^(3q + r) m` with `m` in `[1, 2)`, `cbrt(m)` from a quadratic start
/// and two Halley steps in doubles, then one Newton step in double-double, times
/// `2^(r/3)` from the table.
pub(super) fn cbrt(x: f64) -> f64 {
    if x.is_nan() {
        return f64::NAN;
    }
    if x == 0.0 || x.is_infinite() {
        return x;
    }
    let a = x.abs();
    let (a, bias) = if a < f64::MIN_POSITIVE {
        (a * dd::pow2(54), -18)
    } else {
        (a, 0)
    };
    let e = dd::exponent(a);
    // The mantissa, by its bits (2^-e is not a normal double for e = 1023).
    let m = f64::from_bits((a.to_bits() & 0x000f_ffff_ffff_ffff) | 0x3ff0_0000_0000_0000);
    let (q, r) = (e.div_euclid(3), e.rem_euclid(3));
    // m^(1/3) within 1e-3, then Halley's step twice (cubic: 1e-9, then rounding).
    let mut y = 0.6221 + m * (0.4369 - 0.059 * m);
    for _ in 0..2 {
        let y3 = y * y * y;
        y *= (y3 + 2.0 * m) / (2.0 * y3 + m);
    }
    // One Newton step on the exact residual: y + d with d = -(y^3 - m) / (3 y^2).
    let y2 = two_prod(y, y);
    let y3 = dd::mul(y2, (y, 0.0));
    let res = dd::add(y3, (-m, 0.0));
    let d = -(res.0 + res.1) / (3.0 * y2.0);
    let root = fast_two_sum(y, d);
    let root = match r {
        0 => root,
        1 => dd::mul(root, CBRT2),
        _ => dd::mul(root, CBRT4),
    };
    (root.0 * dd::pow2(q + bias)).copysign(x)
}

/// `sqrt(x^2 + y^2)` without overflow or underflow on the way, rounded once.
pub(super) fn hypot(x: f64, y: f64) -> f64 {
    let (a, b) = (x.abs(), y.abs());
    if a == f64::INFINITY || b == f64::INFINITY {
        return f64::INFINITY;
    }
    if a.is_nan() || b.is_nan() {
        return f64::NAN;
    }
    let (mn, mx) = if a > b { (b, a) } else { (a, b) };
    // Below 2^-60 of the larger, the smaller changes nothing (and covers mn == 0).
    if mn <= mx * dd::pow2(-60) {
        return mx;
    }
    // Keep the squares and their error terms inside the normal range.
    let k = dd::exponent(mx);
    let s = if (-450..=450).contains(&k) { 0 } else { -k };
    let (mx, mn) = (dd::scale(mx, s), dd::scale(mn, s));
    let sum = dd::add(two_prod(mx, mx), two_prod(mn, mn));
    let (h, l) = dd::sqrt(sum);
    round_scaled(h, l, -s)
}
