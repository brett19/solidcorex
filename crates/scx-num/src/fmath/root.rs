// Written for solidcorex. Apache-2.0.
//! `cbrt` and `hypot`.

use super::dd::{self, round_scaled};
use super::tables::{CBRT2, CBRT4};
use crate::eft::{fast_two_sum, two_prod};

/// `cbrt(x)`: `x = 2^(3q + r) m` with `m` in `[1, 2)`, and `cbrt(m 2^r)` from
/// [`fast_cbrt`] when its rounding test passes, else from [`cbrt_dd`]; the bits are
/// [`cbrt_dd`]'s either way.
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
    let (hi, lo) = fast_cbrt(m, r);
    let root = if dd::rounds_to_hi(hi, lo, FAST_TEST) {
        hi
    } else {
        cbrt_dd(m, r).0
    };
    (root * dd::pow2(q + bias)).copysign(x)
}

/// `cbrt(m 2^r)` as a double-double whose high part is the rounded result, for `m` in
/// `[1, 2)` and `r` in `0..3`: `cbrt(m)` from a quadratic start and two Halley steps in
/// doubles, then one Newton step in double-double, times `2^(r/3)` from the table.
fn cbrt_dd(m: f64, r: i32) -> (f64, f64) {
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
    match r {
        0 => root,
        1 => dd::mul(root, CBRT2),
        _ => dd::mul(root, CBRT4),
    }
}

/// The rounding-test constant for [`fast_cbrt`]: `1 + 2^-9`, good for
/// `eps < 2^-54 (1 - (1 + 2^-53)/(1 + 2^-9)) = 2^-63.01` ([`dd::rounds_to_hi`]). The
/// fast path's error is at most `2^-65` and the double-double path's below `2^-95`
/// (Newton's step from a double good to `2^-52` leaves `2^-103`, and its double-double
/// residual and product a few `2^-104`), so `eps <= 2^-64.99`: a margin of 4 (and of
/// about 10 over the fast path's budget). The test fails for about 0.2% of arguments.
const FAST_TEST: f64 = 1.001_953_125;

/// `cbrt(m 2^r)` for `m` in `[1, 2)` and `r` in `0..3`, as an unevaluated `hi + lo` with
/// relative error at most `2^-65`, in doubles alone. A start `y0` on the grid `2^-16`
/// has at most 17 bits, so `y0^3` (51 bits) and `m 2^r - y0^3` (Sterbenz) are exact, and
/// `cbrt(m 2^r) = y0 (1 + t)^(1/3)` with `t = (m 2^r - y0^3)/y0^3` is `y0` plus a
/// correction from four terms of the binomial series.
///
/// Error: the start (a degree-4 fit to `cbrt` on `[1, 2]`, relative error `2^-16.2`
/// at most, sampled at 10^5 points, times `2^(r/3)` in a double) then the rounding to the
/// grid (`2^-17` absolute, `y0 >= 1 - 2^-16`) leave `|t| <= 3 (2^-16.2 + 2^-17) < 2^-13.9`. The
/// series' truncation is `22/729 |t|^5 < 2^-74`. The correction is below
/// `2^-15.5 y0`, and its roundings (`t`'s division, the coefficient `1/3`, the Horner
/// sum and two products: five units of `2^-53`) are within `2^-66.2 y0`.
#[inline]
fn fast_cbrt(m: f64, r: i32) -> (f64, f64) {
    // Rounding to the grid 2^-16: ROUND has ulp 2^-16 (1.5 2^36), y below 2^35.
    const ROUND: f64 = 103_079_215_104.0;
    let (scale, cr) = match r {
        0 => (1.0, 1.0),
        1 => (2.0, CBRT2.0),
        _ => (4.0, CBRT4.0),
    };
    let y = cr
        * (0.509_248_133_549_24
            + m * (0.711_742_386_602_599_6
                + m * (-0.293_954_118_084_835_07
                    + m * (0.083_079_035_479_642_42 - m * 0.010_102_212_336_340_12))));
    let y0 = (y + ROUND) - ROUND;
    let y3 = y0 * y0 * y0;
    let t = (m * scale - y3) / y3;
    let p = 1.0 / 3.0 + t * (-1.0 / 9.0 + t * (5.0 / 81.0 - t * (10.0 / 243.0)));
    fast_two_sum(y0, y0 * (t * p))
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

#[cfg(test)]
mod tests {
    use super::super::fast_check::{Stats, count, hard_cases, log_uniform, ulps};
    use super::*;
    use crate::hash::Rng;

    /// `cbrt` against its double-double path alone, bit for bit.
    fn check(x: f64, st: &mut Stats) {
        let got = cbrt(x);
        if x.is_nan() || x == 0.0 || x.is_infinite() {
            assert_eq!(
                got.to_bits(),
                if x.is_nan() { f64::NAN } else { x }.to_bits()
            );
            return;
        }
        // The same split as cbrt's.
        let a = x.abs();
        let (a, bias) = if a < f64::MIN_POSITIVE {
            (a * dd::pow2(54), -18)
        } else {
            (a, 0)
        };
        let e = dd::exponent(a);
        let m = f64::from_bits((a.to_bits() & 0x000f_ffff_ffff_ffff) | 0x3ff0_0000_0000_0000);
        let (q, r) = (e.div_euclid(3), e.rem_euclid(3));
        let slow = cbrt_dd(m, r);
        let want = (slow.0 * dd::pow2(q + bias)).copysign(x);
        assert_eq!(got.to_bits(), want.to_bits(), "cbrt({x:e})");
        let (hi, lo) = fast_cbrt(m, r);
        st.add((hi, lo), dd::rounds_to_hi(hi, lo, FAST_TEST), slow);
    }

    fn run(n: u64) {
        let mut st = Stats::default();
        let mut r = Rng::new(0x2195_CB27);
        for i in 0..n {
            let x = match i % 4 {
                // every finite double, subnormals included
                0 => loop {
                    let v = f64::from_bits(r.next_u64());
                    if v.is_finite() {
                        break v;
                    }
                },
                // the cubic's scales
                1 => log_uniform(&mut r, -60.0, 60.0),
                // near exact cubes, where the root sits on or next to a double
                2 => {
                    let k = (r.next_u64() % (1 << 17)) as f64 + 1.0;
                    let c = dd::scale(k * k * k, (r.next_u64() % 600) as i32 - 300);
                    ulps(c, (r.next_u64() % 9) as i64 - 4)
                }
                // near the ends of each binade of m and of 2^r
                _ => {
                    let e = (r.next_u64() % 2000) as i32 - 1000;
                    ulps(dd::pow2(e), (r.next_u64() % 64) as i64 - 32)
                }
            };
            check(x, &mut st);
        }
        for x in hard_cases("cbrt") {
            check(x, &mut st);
        }
        st.report("cbrt");
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
