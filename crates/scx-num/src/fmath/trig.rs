// Written for solidcorex. Apache-2.0.
//! `sin`, `cos`, `sin_cos` and `tan`.
//!
//! Reduction to `x = n pi/2 + y` with `|y| <= pi/4` (a hair over is harmless): below
//! `2^20` by Cody-Waite with `pi/2` in four pieces (152 bits); above, by Payne-Hanek with
//! a 192-bit window of the bits of `2/pi` and exact integer products. `y` is a
//! double-double within `2^-104 |y|` plus `2^-127.3` of the true reduced argument
//! (Cody-Waite: the one rounded sum `ae + be - k PIO2_4`, below `2^-52 |y| + 2^-84`, and
//! the `2^-137` of `k` times `PIO2_4`'s rounding; Payne-Hanek: the fraction truncated to
//! 128 bits of a quadrant, and the window's end). No double lies closer than `2^-60.89`
//! to a multiple of `pi/2` (at `6381956970095103 2^797`; Muller, "Elementary Functions",
//! ch. 11), so `y` is within `2^-66.4 |y|` (the largest seen: `2^-68.2`).
//!
//! Kernels: `y = i/64 + h` with `|h| <= 1/128`; `sin` and `cos` of `i/64` come from a
//! double-double table, and of `h` from short series.
//!
//! Rounding: a fast kernel in doubles with a Ziv test, then the double-double kernel
//! with its own, then the multi-precision fallback.

use super::accurate;
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

/// `1/6` as a double-double.
const SIXTH: (f64, f64) = (f64::from_bits(0x3fc5555555555555), f64::from_bits(0x3c65555555555555));

/// `(sin y, cos y)` as double-doubles, for the reduced `|y| <= 0.8`: `|y| = a + t` with
/// `a = i/64` from the table and `t = h + yl` (`|h| <= 1/128`), and
/// `sin(a + t) = sin a + sin a (cos t - 1) + cos a sin t`,
/// `cos(a + t) = cos a + cos a (cos t - 1) - sin a sin t`.
///
/// Error, relative to `sin y` and `cos y` of the double-double `y`, below `2^-82.6`.
/// With `u = fl(t^2)`, `sin t - t = t^3 (-1/6 + w)` and `cos t - 1 = t^2 (-1/2 + v)`,
/// `w` and `v` in doubles: both within `2^-51.2` of themselves (four roundings), so
/// `sin t` within `2^-85.9 |t|` (`|w| <= 2^-20.9`, `t^2/6 <= 2^-16.6`) and `cos t - 1`
/// within `2^-83.8` (`|v| <= 2^-18.6`, `t^2 <= 2^-14`). The series stop at `t^9/9!` and
/// `t^8/8!` (dropped: `2^-95.2 |t|` and `2^-91.8`). `sin a <= 2 |sin y|` and `cos y >=
/// 0.69`, so the `cos t - 1` term gives at most `2^-82.8` of either result and `sin t`
/// `2^-85.4`; the double-double products and sums (about `2^-104` each, of terms below
/// `2 |sin y|`) and the table's `2^-106` add below `2^-97`.
fn precise(y: (f64, f64)) -> ((f64, f64), (f64, f64)) {
    let p = Precise::new(y);
    (p.sin(), p.cos())
}

/// The parts [`precise`] shares between `sin y` and `cos y`.
struct Precise {
    neg: bool,
    /// `sin a`, `cos a`, `sin t` and `cos t - 1`.
    sa: (f64, f64),
    ca: (f64, f64),
    st: (f64, f64),
    cm: (f64, f64),
}

impl Precise {
    fn new(y: (f64, f64)) -> Self {
        let neg_y = y.0 < 0.0;
        let (yh, yl) = if neg_y { neg(y) } else { y };
        let i = round_int(yh * 64.0);
        let t = two_sum(yh - i * (1.0 / 64.0), yl);
        let (sh, sl, ch, cl) = at(&SINCOS_TAB, i as usize);
        let t2 = dd::mul(t, t);
        let u = t2.0;
        let w = u * (1.0 / 120.0 + u * (-1.0 / 5040.0 + u * (1.0 / 362_880.0)));
        let (ps, pse) = two_sum(-SIXTH.0, w);
        let st = dd::add(t, dd::mul(dd::mul(t2, t), (ps, pse - SIXTH.1)));
        let v = u * (1.0 / 24.0 + u * (-1.0 / 720.0 + u * (1.0 / 40_320.0)));
        let cm = dd::mul(t2, two_sum(-0.5, v));
        Self {
            neg: neg_y,
            sa: (sh, sl),
            ca: (ch, cl),
            st,
            cm,
        }
    }

    fn sin(&self) -> (f64, f64) {
        let (sa, ca) = (self.sa, self.ca);
        let sin = dd::add(sa, dd::add(dd::mul(sa, self.cm), dd::mul(ca, self.st)));
        if self.neg { neg(sin) } else { sin }
    }

    fn cos(&self) -> (f64, f64) {
        let (sa, ca) = (self.sa, self.ca);
        dd::add(ca, dd::add(dd::mul(ca, self.cm), neg(dd::mul(sa, self.st))))
    }
}

fn neg(v: (f64, f64)) -> (f64, f64) {
    (-v.0, -v.1)
}

/// `sin(x + quarters pi/2)` rounded, for the reduction `(n, y)` of a finite `x`: the
/// fast kernel when its test passes, else [`slow`].
#[inline(always)]
fn sin_quadrant(x: f64, quarters: i32, n: i32, y: (f64, f64)) -> f64 {
    let q = (n + quarters) & 3;
    let (hi, lo) = if q & 1 == 0 { fast_sin(y) } else { fast_cos(y) };
    if dd::rounds_to_hi(hi, lo, FAST_TEST) {
        return if q >= 2 { -hi } else { hi };
    }
    slow(x, quarters, q, y)
}

/// [`sin_quadrant`] by [`precise`], else the multi-precision fallback. The test's radius is
/// twice the bound: [`precise`]'s `2^-82.6`, with the reduction's `2^-104 |y|` relative
/// (moving `sin y` and `cos y` by no more, relatively) and `2^-127.3` absolute (by no
/// more, absolutely).
#[inline(never)]
fn slow(x: f64, quarters: i32, q: i32, y: (f64, f64)) -> f64 {
    let p = Precise::new(y);
    let v = if q & 1 == 0 { p.sin() } else { p.cos() };
    let (h, l) = if q >= 2 { neg(v) } else { v };
    let err = PRECISE_ERR * h.abs() + RED_ABS;
    dd::round_ball_or(h, l, err, || accurate::sin_quadrant(x, quarters as u32))
}

/// `2^-81.5`.
const PRECISE_ERR: f64 = 2.9245251020114406e-25;
/// `2^-126`, over twice the reduction's absolute error.
const RED_ABS: f64 = 1.1754943508222875e-38;

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
    let (n, y) = reduce(x);
    sin_quadrant(x, 0, n, y)
}

/// `cos(x)`, as `sin(x + pi/2)`.
pub(super) fn cos(x: f64) -> f64 {
    if !x.is_finite() {
        return f64::NAN;
    }
    if x.abs() < TINY {
        return 1.0;
    }
    let (n, y) = reduce(x);
    sin_quadrant(x, 1, n, y)
}

/// The rounding-test constant for [`fast_sin`] and [`fast_cos`]: `1 + 2^-6`, good for
/// `eps < 2^-54 (1 - (1 + 2^-53)/(1 + 2^-6)) = 2^-60.02` ([`dd::rounds_to_hi`]). Against
/// the true value their error is at most `2^-62` for the fast kernel plus the reduction's
/// `2^-66.4`, so `2^-61.93`: a margin of 3.8 (the largest seen: `2^-65.4`). The test
/// fails, and the double-double kernel runs, for about 1.5% of arguments in `[-4, 4]`.
const FAST_TEST: f64 = 1.015_625;

/// The table index, the offset and the two short series shared by [`fast_sin`] and
/// [`fast_cos`]: `|y| = i/64 + h + yl` with `|h| <= 1/128`, `ds = sin(h + yl) - h` and
/// `dc = cos(h + yl) - 1`, in doubles.
///
/// Error, absolute: `ds` has its truncation (`h^9/9!`, below `2^-81`), the dropped
/// `yl (cos h - 1)` (below `2^-68 |y|`) and four roundings of a value below
/// `h^3/6 <= 2^-16.6 |h|`, so is within `2^-67 |h|`. `dc` has its truncation (`h^8/8!`,
/// below `2^-71.3`) and three roundings of a value below `h^2/2 <= 2^-15`: within
/// `2^-65.9`.
#[inline]
fn fast_parts(y: (f64, f64)) -> (usize, f64, f64, f64) {
    let (yh, yl) = if y.0 < 0.0 { (-y.0, -y.1) } else { y };
    let i = round_int(yh * 64.0);
    let h = yh - i * (1.0 / 64.0);
    let h2 = h * h;
    let ds = h * h2 * (-1.0 / 6.0 + h2 * (1.0 / 120.0 - h2 * (1.0 / 5040.0))) + yl;
    let dc = h2 * (-0.5 + h2 * (1.0 / 24.0 - h2 * (1.0 / 720.0))) - h * yl;
    (i as usize, h, ds, dc)
}

/// `sin y` as an unevaluated `hi + lo` (`|lo|` near `ulp(hi)/2`), for the reduced `y`,
/// in doubles but for one exact product:
/// `sin(a + h) = sin a + cos a h + sin a dc + cos a ds`, with `cos a h` exact and the rest, below
/// `2^-13.7 |sin y|`, summed plainly. Relative error at most `2^-62`: `2^-64.1` from the eight
/// roundings of the small terms, `2^-64.9` from `dc` (times `sin a <= 2 |sin y|`), `2^-66.7` from
/// `ds`, `2^-67` from the dropped `sl dc`, and the table's `2^-106`.
#[inline]
fn fast_sin(y: (f64, f64)) -> (f64, f64) {
    let (i, h, ds, dc) = fast_parts(y);
    let (sh, sl, ch, cl) = at(&SINCOS_TAB, i);
    let (p, pe) = two_prod(ch, h);
    // |sh| >= sin(1/64) > 1/128 >= |p| unless sh is 0.
    let (s, se) = fast_two_sum(sh, p);
    let lo = se + pe + (sl + cl * h + sh * dc + ch * ds);
    let (hi, lo) = fast_two_sum(s, lo);
    if y.0 < 0.0 { (-hi, -lo) } else { (hi, lo) }
}

/// `cos y` as [`fast_sin`] gives `sin y`:
/// `cos(a + h) = cos a - sin a h + cos a dc - sin a ds`, with `sin a h` exact. Relative error at
/// most `2^-63`: the small terms are below `2^-14.5 |cos y|` (`cos y >= 0.7`), their six roundings
/// `2^-64.9`, `dc` `2^-65.4`, the rest below `2^-68`.
#[inline]
fn fast_cos(y: (f64, f64)) -> (f64, f64) {
    let (i, h, ds, dc) = fast_parts(y);
    let (sh, sl, ch, cl) = at(&SINCOS_TAB, i);
    let (p, pe) = two_prod(sh, h);
    // ch >= cos(0.8) > |p|.
    let (c, ce) = fast_two_sum(ch, -p);
    let lo = ce - pe + (cl - sl * h + ch * dc - sh * ds);
    fast_two_sum(c, lo)
}

/// `(sin x, cos x)`.
pub(super) fn sin_cos(x: f64) -> (f64, f64) {
    if !x.is_finite() {
        return (f64::NAN, f64::NAN);
    }
    if x.abs() < TINY {
        return (x, 1.0);
    }
    let (n, y) = reduce(x);
    (sin_quadrant(x, 0, n, y), sin_quadrant(x, 1, n, y))
}

/// `tan(x)`: `sin y / cos y` or `-cos y / sin y` by the fast kernel when its test passes
/// ([`FAST_TEST`]: the fast pair's `2^-62` and `2^-63`, the division's `2^-104` and the
/// reduction's `2^-66.4` give `2^-61.2`, a margin of 2.3), else by [`precise`], else the
/// multi-precision fallback.
pub(super) fn tan(x: f64) -> f64 {
    if !x.is_finite() {
        return f64::NAN;
    }
    if x.abs() < TINY {
        return x;
    }
    let (n, y) = reduce(x);
    let (s, c) = (fast_sin(y), fast_cos(y));
    let (hi, lo) = if n & 1 == 0 { dd::div(s, c) } else { neg(dd::div(c, s)) };
    if dd::rounds_to_hi(hi, lo, FAST_TEST) {
        return hi;
    }
    tan_slow(x, n, y)
}

/// [`tan`] by [`precise`]: each half within `2^-82.6`, the quotient within `2^-81.6`; the
/// reduction's error moves `tan` by at most `2.2` times its relative error in `y`, which
/// `2^-127.3` absolute makes at most `2.2 2^-127.3 (1 + tan^2)` absolute (`|tan y|/|y| <=
/// 1 + tan^2 y` and `|cot y|/|y| <= 1 + cot^2 y`). Twice each. `|tan x|` lies in
/// `[2^-61, 2^61]`, so the result is normal.
#[inline(never)]
fn tan_slow(x: f64, n: i32, y: (f64, f64)) -> f64 {
    const REL: f64 = 5.849050204022881e-25; // 2^-80.5
    const ABS: f64 = 2.350988701644575e-38; // 2^-125
    let (s, c) = precise(y);
    let (h, l) = if n & 1 == 0 { dd::div(s, c) } else { neg(dd::div(c, s)) };
    let err = REL * h.abs() + ABS * (1.0 + h * h);
    dd::round_ball_or(h, l, err, || accurate::tan(x))
}

#[cfg(test)]
mod tests {
    use super::super::fast_check::{Stats, count, hard_cases, log_uniform, ulps, uniform};
    use super::*;
    use crate::hash::Rng;

    /// `cos`, `sin` and `tan` against their slow paths alone, bit for bit.
    fn check(x: f64, st: &mut Stats) {
        for (quarters, got) in [(1, cos(x)), (0, sin(x))] {
            let want = if !x.is_finite() {
                f64::NAN
            } else if x.abs() < TINY {
                if quarters == 1 { 1.0 } else { x }
            } else {
                let (n, y) = reduce(x);
                slow(x, quarters, (n + quarters) & 3, y)
            };
            assert_eq!(got.to_bits(), want.to_bits(), "sin(x + {quarters} pi/2), x = {x:e}");
        }
        let want = if !x.is_finite() {
            f64::NAN
        } else if x.abs() < TINY {
            x
        } else {
            let (n, y) = reduce(x);
            tan_slow(x, n, y)
        };
        assert_eq!(tan(x).to_bits(), want.to_bits(), "tan({x:e})");
        if x.is_finite() && x.abs() >= TINY {
            let (n, y) = reduce(x);
            let (ks, kc) = precise(y);
            let ((fh, fl), dd) = if n & 1 == 0 {
                (fast_cos(y), kc)
            } else {
                (fast_sin(y), ks)
            };
            st.add((fh, fl), dd::rounds_to_hi(fh, fl, FAST_TEST), dd);
        }
    }

    fn run(n: u64) {
        let mut st = Stats::default();
        let mut r = Rng::new(0x2195_C051);
        let pio2 = PIO2.0;
        for i in 0..n {
            let x = match i % 4 {
                // the cubic's arguments, and every scale Cody-Waite covers
                0 => uniform(&mut r, -4.0, 4.0),
                1 => log_uniform(&mut r, -27.0, 20.0),
                // near a multiple of pi/2, where y loses its leading bits
                2 => {
                    let k = log_uniform(&mut r, 0.0, 20.0).round();
                    ulps(k * pio2, (r.next_u64() % 17) as i64 - 8)
                }
                // Payne-Hanek's range
                _ => log_uniform(&mut r, 20.0, 1024.0),
            };
            check(x, &mut st);
        }
        for x in hard_cases("cos") {
            check(x, &mut st);
        }
        st.report("cos");
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
