// Written for solidcorex. Apache-2.0.
//! `atan`, `atan2`, `asin` and `acos`, all on one double-double arctangent of a ratio
//! in `[0, 1]`.
//!
//! Kernel: `t = c + (t - c)` with `c = i/64` the nearest table point, and
//! `atan(t) = atan(c) + atan(d)`, `d = (t - c)/(1 + t c)`, `|d| <= 1/128`; `atan(d)` is
//! its series to `d^9` (truncation below `2^-73`).
//!
//! Rounding: each function rounds that kernel's double-double with a Ziv test
//! ([`FAST_ERR`]); when it fails, again with the series carried in double-double
//! ([`PRECISE_ERR`]); then the multi-precision fallback.

use super::accurate;
use super::dd::{self, at, round_int};
use super::tables::{ATAN_TAB, PI, PI3O4, PIO2, PIO4};
use crate::eft::{fast_two_sum, two_prod, two_sum};

/// `atan(t)` as a double-double for a double-double `t` in `[0, 1 + 1/128)`, the series by
/// [`finish`] (`P` false) or [`finish_precise`] (`P` true).
fn atan_dd<const P: bool>(t: (f64, f64)) -> (f64, f64) {
    let (th, tl) = t;
    let i = round_int(th * 64.0);
    let c = i * (1.0 / 64.0);
    // d = (t - c) / (1 + t c); th - c is exact.
    let num = fast_two_sum(th - c, tl);
    let (p, pe) = two_prod(th, c);
    let (s, se) = fast_two_sum(1.0, p);
    let den = fast_two_sum(s, se + pe + tl * c);
    finish_any::<P>(i, dd::div(num, den))
}

/// `atan(n/d)` as a double-double for double-doubles with `n/d` in `[0, 1 + 1/128)`
/// and `|d| >= 2^-500`, without forming the ratio: `d' = (n - c d)/(d + c n)`, one
/// division where `atan_dd(n/d)` would take two.
fn atan_ratio<const P: bool>(n: (f64, f64), d: (f64, f64)) -> (f64, f64) {
    let i = round_int(n.0 / d.0 * 64.0);
    let c = i * (1.0 / 64.0);
    let (p, pe) = two_prod(c, d.0);
    let num = dd::add(n, (-p, -(pe + c * d.1)));
    let (q, qe) = two_prod(c, n.0);
    let den = dd::add(d, (q, qe + c * n.1));
    finish_any::<P>(i, dd::div(num, den))
}

#[allow(clippy::inline_always)] // a const generic per phase: inlined, `P = false` drops the precise tail
#[inline(always)]
fn finish_any<const P: bool>(i: f64, d: (f64, f64)) -> (f64, f64) {
    if P {
        finish_precise(i, d)
    } else {
        finish(i, d)
    }
}

/// The relative error of the double-double results with [`finish`], twice its bound. The
/// series' tail, below `|d|^3/3 <= 2^-15.6 |d|`, is in doubles to `2^-50.7` of itself
/// (five roundings): `2^-66.3 |d|`. The dropped `-dl d^2` of `atan(dh + dl)` is below
/// `2^-67 |d|`, the final low sum's roundings below `2^-68.6 |d|`, the truncation
/// (`d^11/11`) below `2^-73.5 |d|`, the ratio `d` (one corrected division of
/// double-double sums) and the table within `2^-100`. So `2^-65.3 |d|`, and `|d| <=
/// 1.0003 atan(t)`; the `pi/2 -` and `pi -` steps take a value at most the result and
/// add `2^-104`.
const FAST_ERR: f64 = 4.4032290636799467e-20; // 2^-64.3

/// [`FAST_ERR`] with [`finish_precise`]: twice `2^-79`, its `2^-81` plus the steps'
/// `2^-100` and, for `asin` and `acos`, `sqrt(1 - a^2)`'s `2^-103`.
const PRECISE_ERR: f64 = 3.308722450212111e-24; // 2^-78

/// `1/3` as a double-double.
const THIRD: (f64, f64) = (
    f64::from_bits(0x3fd5555555555555),
    f64::from_bits(0x3c75555555555555),
);

/// [`finish`] with the series' `d^3 (-1/3 + w)` in double-double: `w` (below `2^-16.3`)
/// within `2^-67.3`, so the tail within `2^-81.3 |d|`; the truncation (`d^13/13`) below
/// `2^-87.7 |d|`; the low sum adds only values below `2^-51 |d|`, and `dl` enters with
/// its `1 - d^2`.
#[inline(never)]
fn finish_precise(i: f64, (dh, dl): (f64, f64)) -> (f64, f64) {
    let (d2, d2e) = two_prod(dh, dh);
    let w = d2 * (0.2 + d2 * (-1.0 / 7.0 + d2 * (1.0 / 9.0 - d2 * (1.0 / 11.0))));
    let (ph, pe) = two_sum(-THIRD.0, w);
    let (c3, c3e) = two_prod(d2, dh);
    let tail = dd::mul((c3, c3e + d2e * dh), (ph, pe - THIRD.1));
    let (ah, al) = at(&ATAN_TAB, i as usize);
    let (s, se) = two_sum(ah, dh);
    let (s2, s2e) = two_sum(s, tail.0);
    fast_two_sum(s2, se + s2e + al + tail.1 + dl * (1.0 - d2))
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
    } else {
        let (h, l) = atan_pos::<false>(a);
        dd::round_or(h, l, FAST_ERR, || atan_slow(a))
    };
    r.copysign(x)
}

/// `atan(a)` for `a` in `[2^-27, 2^60]`.
#[allow(clippy::inline_always)] // inlined into each phase, so `P` folds away
#[inline(always)]
fn atan_pos<const P: bool>(a: f64) -> (f64, f64) {
    if a > 1.0 {
        // atan(a) = pi/2 - atan(1/a).
        sub(PIO2, atan_ratio::<P>((1.0, 0.0), (a, 0.0)))
    } else {
        atan_dd::<P>((a, 0.0))
    }
}

#[inline(never)]
fn atan_slow(a: f64) -> f64 {
    let (h, l) = atan_pos::<true>(a);
    dd::round_or(h, l, PRECISE_ERR, || accurate::atan(a))
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
        // atan(t) = t (1 - t^2/3 + ...) with t^2/3 below 2^-121. A normal y/x is no
        // midpoint, and no closer to one than 2^-108 of itself (a midpoint m has 54
        // significant bits, so y - m x is a nonzero multiple of 2^-106 ulp-units of
        // m x): atan(y/x) rounds as y/x does. A subnormal ratio can be a midpoint, and
        // one just below 2^-1022 can round up to it, so only a quotient above it passes.
        let q = y / x;
        return if q.abs() > f64::MIN_POSITIVE {
            q
        } else {
            accurate::atan2(y, x)
        };
    } else {
        let (h, l) = atan2_finite::<false>(ay, ax, x_neg);
        dd::round_or(h, l, FAST_ERR, || atan2_slow(ay, ax, x_neg, y, x))
    };
    r.copysign(y)
}

#[inline(never)]
fn atan2_slow(ay: f64, ax: f64, x_neg: bool, y: f64, x: f64) -> f64 {
    let (h, l) = atan2_finite::<true>(ay, ax, x_neg);
    dd::round_or(h, l, PRECISE_ERR, || accurate::atan2(y, x).abs())
}

/// `atan2(ay, ax)` for finite positive `ay` and `ax`, mirrored to `pi - ...` when `x`
/// was negative, before the sign of `y`: a result of at least `2^-60` (the tiny ratios
/// are handled before), so normal.
#[allow(clippy::inline_always)] // inlined into each phase, so `P` folds away
#[inline(always)]
fn atan2_finite<const P: bool>(ay: f64, ax: f64, x_neg: bool) -> (f64, f64) {
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
    let mut th = atan_ratio::<P>((mn, 0.0), (mx, 0.0));
    if ay > ax {
        th = sub(PIO2, th);
    }
    if x_neg {
        th = sub(PI, th);
    }
    th
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
    let (h, l) = asin_pos::<false>(a);
    dd::round_or(h, l, FAST_ERR, || asin_slow(a)).copysign(x)
}

/// `asin(a)` for `a` in `[2^-27, 1]`.
#[allow(clippy::inline_always)] // inlined into each phase, so `P` folds away
#[inline(always)]
fn asin_pos<const P: bool>(a: f64) -> (f64, f64) {
    let (c, p) = cos_of_sin(a);
    if p <= 0.5 {
        atan_ratio::<P>((a, 0.0), c)
    } else {
        sub(PIO2, atan_ratio::<P>(c, (a, 0.0)))
    }
}

#[inline(never)]
fn asin_slow(a: f64) -> f64 {
    let (h, l) = asin_pos::<true>(a);
    dd::round_or(h, l, PRECISE_ERR, || accurate::asin(a))
}

/// `acos(x)`: [`fast_acos`] when its rounding test passes, else [`acos_dd`] with the
/// precise series and [`PRECISE_ERR`], else the multi-precision fallback.
pub(super) fn acos(x: f64) -> f64 {
    let a = x.abs();
    if x.is_nan() || a > 1.0 {
        return f64::NAN;
    }
    if a < 1.0 {
        let (hi, lo) = fast_acos(x, a);
        if dd::rounds_to_hi(hi, lo, FAST_TEST) {
            return hi;
        }
    }
    acos_slow(x, a)
}

#[inline(never)]
fn acos_slow(x: f64, a: f64) -> f64 {
    let (h, l) = acos_dd(x, a);
    // acos(1) is 0 exactly, which passes; otherwise the result is above 2^-27.
    dd::round_or(h, l, PRECISE_ERR, || accurate::acos(x))
}

/// `acos(x)` for `x` in `[-1, 1]` (`a = |x|`) in double-double, with the precise series.
fn acos_dd(x: f64, a: f64) -> (f64, f64) {
    let (c, p) = cos_of_sin(a);
    // acos(a) for a = |x|, then pi - that for negative x.
    let r = if p <= 0.5 {
        sub(PIO2, atan_ratio::<true>((a, 0.0), c))
    } else {
        atan_ratio::<true>(c, (a, 0.0))
    };
    if x < 0.0 { sub(PI, r) } else { r }
}

/// The rounding-test constant for [`fast_acos`]: `1 + 2^-7`, good for
/// `eps < 2^-54 (1 - (1 + 2^-53)/(1 + 2^-7)) = 2^-61.01` ([`dd::rounds_to_hi`]). The fast
/// path's error against the true value is at most `2^-64` (`2^-65.8` by the budget
/// below): a margin of 8. The test fails for about 0.8% of arguments in `[-1, 1]`.
const FAST_TEST: f64 = 1.007_812_5;

/// `acos(x)` for `|x| < 1` (`a = |x|`) as an unevaluated `hi + lo`, relative error at
/// most `2^-64`, by the double-double path's formulas (`pi/2 - atan(x/c)` for `x^2 <= 1/2`,
/// else `atan(c/a)` or `pi - atan(c/a)`, `c = sqrt(1 - x^2)`) but with each value carried
/// only as far as the result needs: one exact product per step, the low parts summed
/// plainly and never renormalised between steps.
///
/// Error: `1 - a^2` is exact as `wh + wl`, and `c = s + cl` with `s` the root of
/// `wh + wl` rounded is good to `2^-103` with `|cl|` within two ulps of `s` (a root of
/// `wh` alone leaves `cl` up to `2^-29 s` near `|x| = 1`, whose square, and whose
/// product with `d^2` in the series below, which sees only the ratio's high part,
/// reach `2^-59`: both found by the check in this file's tests); the ratio's
/// offset `d = (n - k d)/(d + k n)` from the table point `k = i/64` is good to `2^-101`
/// (its numerator and denominator exact but for roundings of their low parts, then one
/// corrected division). The rest is `atan(d) - d`, below `|d|^3/3 <= 2^-15.6 |d|`, with
/// its truncation (`2^-73.5 |d|`) and roundings (`2^-66.6 |d|`), and three roundings of
/// the low sum (`2^-67`); `|d|` is below `1.0003 atan(t)`, so these are relative to the
/// arctangent, which is relative to the result in the second case and at most the result
/// in the others: `2^-65.8` all told.
fn fast_acos(x: f64, a: f64) -> (f64, f64) {
    let (p, pe) = two_prod(a, a);
    let (wh, wl) = if p > 0.5 {
        // Sterbenz: 1 - p is exact.
        (1.0 - p, -pe)
    } else {
        let (h, e) = fast_two_sum(1.0, -p);
        (h, e - pe)
    };
    // The root of the rounded sum, not of wh alone: near |x| = 1, wl reaches 2^-28 wh,
    // and one correction step from sqrt(wh) would leave its square, 2^-59.
    let s = (wh + wl).sqrt();
    let (s2, s2e) = two_prod(s, s);
    // wh - s2 is exact (Sterbenz).
    let cl = ((wh - s2) - s2e + wl) / (2.0 * s);
    if p <= 0.5 {
        // pi/2 - atan(x/c), the arctangent at most pi/4.
        let (t, tl) = fast_atan_ratio((a, 0.0), (s, cl));
        let (t, tl) = if x < 0.0 { (-t, -tl) } else { (t, tl) };
        let (r, re) = fast_two_sum(PIO2.0, -t);
        fast_two_sum(r, re + (PIO2.1 - tl))
    } else {
        let (t, tl) = fast_atan_ratio((s, cl), (a, 0.0));
        if x < 0.0 {
            let (r, re) = fast_two_sum(PI.0, -t);
            fast_two_sum(r, re + (PI.1 - tl))
        } else {
            fast_two_sum(t, tl)
        }
    }
}

/// `atan(n/d)` for `n/d` in `[0, 1]`, as [`atan_ratio`] computes it but in the form
/// [`fast_acos`] describes: an unevaluated `hi + lo` with `|lo| <= 2^-15 |hi|`.
#[inline]
fn fast_atan_ratio((nh, nl): (f64, f64), (dh, dl): (f64, f64)) -> (f64, f64) {
    let i = round_int(nh / dh * 64.0);
    let k = i * (1.0 / 64.0);
    // n - k d and d + k n, each a double and a low part. With |n/d - k| <= 1/128 and
    // k = 0 or k >= 1/64, k d lies in [2n/3, 2n] (or is 0), so nh - p is exact
    // (Sterbenz); k n <= n <= d, so the sum needs only the fast form.
    let (p, pe) = two_prod(k, dh);
    let n1 = nh - p;
    let n_lo = nl - (pe + k * dl);
    let (q, qe) = two_prod(k, nh);
    let (d1, d1e) = fast_two_sum(dh, q);
    let d_lo = d1e + (qe + dl + k * nl);
    // d = n1/d1 corrected once; n1 - r is exact (Sterbenz).
    let dq = n1 / d1;
    let (r, re) = two_prod(dq, d1);
    let dq_lo = ((n1 - r) - re + n_lo - dq * d_lo) / d1;
    let d2 = dq * dq;
    let tail = dq * d2 * (-1.0 / 3.0 + d2 * (0.2 + d2 * (-1.0 / 7.0 + d2 * (1.0 / 9.0))));
    let (ah, al) = at(&ATAN_TAB, i as usize);
    // ah >= atan(1/64) > 1/128 >= |dq| unless ah is 0.
    let (h, he) = fast_two_sum(ah, dq);
    (h, he + al + dq_lo + tail)
}

#[cfg(test)]
mod tests {
    use super::super::fast_check::{Stats, count, hard_cases, log_uniform, ulps, uniform};
    use super::*;
    use crate::hash::Rng;

    /// `acos` against its slow path alone, bit for bit.
    fn check(x: f64, st: &mut Stats) {
        let a = x.abs();
        let slow = if x.is_nan() || a > 1.0 {
            (f64::NAN, 0.0)
        } else {
            acos_dd(x, a)
        };
        let want = if x.is_nan() || a > 1.0 {
            f64::NAN
        } else {
            acos_slow(x, a)
        };
        assert_eq!(acos(x).to_bits(), want.to_bits(), "acos({x:e})");
        if a < 1.0 {
            let (hi, lo) = fast_acos(x, a);
            st.add((hi, lo), dd::rounds_to_hi(hi, lo, FAST_TEST), slow);
        }
    }

    fn run(n: u64) {
        let mut st = Stats::default();
        let mut r = Rng::new(0x2195_AC05);
        let half = core::f64::consts::FRAC_1_SQRT_2;
        for i in 0..n {
            let x = match i % 4 {
                // the cubic's arguments
                0 => uniform(&mut r, -1.0, 1.0),
                // near ±1, where acos x is about sqrt(2 (1 - |x|))
                1 => {
                    let v = 1.0 - dd::scale(r.next_f64(), -((r.next_u64() % 54) as i32));
                    if r.next_u64() & 1 == 0 { v } else { -v }
                }
                // near the switch at x^2 = 1/2, and near 0
                2 => {
                    let v = ulps(half, (r.next_u64() % 4097) as i64 - 2048);
                    if r.next_u64() & 1 == 0 { v } else { -v }
                }
                _ => log_uniform(&mut r, -60.0, 0.0),
            };
            check(x, &mut st);
        }
        for x in hard_cases("acos") {
            check(x, &mut st);
        }
        st.report("acos");
    }

    #[test]
    fn fast_path_changes_no_bit() {
        run(count(20_000));
    }

    /// The long run (release): `FMATH_FAST_N` inputs and the hard cases.
    #[test]
    #[ignore = "long: run in release with FMATH_FAST_N and FMATH_HARD_DIR"]
    fn fast_path_changes_no_bit_long() {
        run(count(100_000_000));
    }
}
