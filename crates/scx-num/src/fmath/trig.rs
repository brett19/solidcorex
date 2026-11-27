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

/// The pieces shared by `sin y` and `cos y`, for `|y| <= 0.8`: `y = ±(a + h)` with
/// `a = i/64` from the table and `|h| <= 1/128`.
struct Kernel {
    neg: bool,
    h: f64,
    /// `sin a` and `cos a` as double-doubles.
    sh: f64,
    sl: f64,
    ch: f64,
    cl: f64,
    /// `sin(h + yl) - h`, and `cos(h + yl) - 1` as `dch + dcl` (sum `dc`), to about 2^-75.
    ds: f64,
    dch: f64,
    dcl: f64,
    dc: f64,
}

impl Kernel {
    fn new(y: (f64, f64)) -> Self {
        let neg = y.0 < 0.0;
        let (yh, yl) = if neg { (-y.0, -y.1) } else { y };
        let i = round_int(yh * 64.0);
        let h = yh - i * (1.0 / 64.0);
        let (sh, sl, ch, cl) = at(&SINCOS_TAB, i as usize);
        let (h2, h2e) = two_prod(h, h);
        let ds = h * h2 * (-1.0 / 6.0 + h2 * (1.0 / 120.0 - h2 * (1.0 / 5040.0)))
            + yl * (1.0 - 0.5 * h2);
        let dch = -0.5 * h2;
        let dcl = -0.5 * h2e + h2 * h2 * (1.0 / 24.0 - h2 * (1.0 / 720.0)) - h * yl;
        let dc = dch + dcl;
        Self {
            neg,
            h,
            sh,
            sl,
            ch,
            cl,
            ds,
            dch,
            dcl,
            dc,
        }
    }

    /// `sin y`: `sin(a + h) = sin a + sin a (cos h - 1) + cos a sin h`.
    fn sin(&self) -> (f64, f64) {
        let Self {
            neg,
            h,
            sh,
            sl,
            ch,
            cl,
            ds,
            dch,
            dcl,
            dc,
        } = *self;
        let (p, pe) = two_prod(ch, h);
        let (s, se) = two_sum(sh, p);
        let (q, qe) = two_prod(sh, dch);
        let (s2, s2e) = two_sum(s, q);
        let s_lo = se + pe + s2e + qe + sl + cl * h + ch * ds + sh * dcl + sl * dc;
        let sin = fast_two_sum(s2, s_lo);
        if neg { (-sin.0, -sin.1) } else { sin }
    }

    /// `cos y`: `cos(a + h) = cos a + cos a (cos h - 1) - sin a sin h`.
    fn cos(&self) -> (f64, f64) {
        let Self {
            h,
            sh,
            sl,
            ch,
            cl,
            ds,
            dch,
            dcl,
            dc,
            ..
        } = *self;
        let (p, pe) = two_prod(sh, -h);
        let (c, ce) = two_sum(ch, p);
        let (q, qe) = two_prod(ch, dch);
        let (c2, c2e) = two_sum(c, q);
        let c_lo = ce + pe + c2e + qe + cl - sl * h - sh * ds + ch * dcl + cl * dc;
        fast_two_sum(c2, c_lo)
    }
}

fn neg(v: (f64, f64)) -> (f64, f64) {
    (-v.0, -v.1)
}

/// `(sin x, cos x)` as double-doubles, for finite `x`.
fn sin_cos_dd(x: f64) -> ((f64, f64), (f64, f64)) {
    let (n, y) = reduce(x);
    let k = Kernel::new(y);
    let (s, c) = (k.sin(), k.cos());
    match n {
        0 => (s, c),
        1 => (c, neg(s)),
        2 => (neg(s), neg(c)),
        _ => (neg(c), s),
    }
}

/// `sin(x + quarters pi/2)` as a double-double, for finite `x`: only the half of the
/// kernel the quadrant picks is computed (`cos x` is `quarters = 1`).
fn sin_dd(x: f64, quarters: i32) -> (f64, f64) {
    let (n, y) = reduce(x);
    let k = Kernel::new(y);
    match (n + quarters) & 3 {
        0 => k.sin(),
        1 => k.cos(),
        2 => neg(k.sin()),
        _ => neg(k.cos()),
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
    sin_dd(x, 0).0
}

/// `cos(x)`: the fast kernel when its rounding test passes, else the double-double one,
/// on the same reduced argument; the bits are the double-double path's either way.
pub(super) fn cos(x: f64) -> f64 {
    if !x.is_finite() {
        return f64::NAN;
    }
    if x.abs() < TINY {
        return 1.0;
    }
    let (n, y) = reduce(x);
    // cos x = sin(x + pi/2): quadrant n + 1 of the sine.
    let q = (n + 1) & 3;
    let (hi, lo) = if q & 1 == 0 { fast_sin(y) } else { fast_cos(y) };
    if dd::rounds_to_hi(hi, lo, FAST_TEST) {
        return if q >= 2 { -hi } else { hi };
    }
    let k = Kernel::new(y);
    match q {
        0 => k.sin().0,
        1 => k.cos().0,
        2 => -k.sin().0,
        _ => -k.cos().0,
    }
}

/// The rounding-test constant for [`fast_sin`] and [`fast_cos`]: `1 + 2^-6`, good for
/// `eps < 2^-54 (1 - (1 + 2^-53)/(1 + 2^-6)) = 2^-60.02` ([`dd::rounds_to_hi`]). Their
/// `eps` is at most `2^-62` for the fast kernel plus `2^-63` for [`Kernel`], both against
/// `sin` or `cos` of the same double-double `y`, so `2^-61.4`: a margin of 2.6 over that,
/// and of about 7 over the sums of the error budgets below. The test fails, and the
/// double-double kernel runs, for about 1.5% of arguments in `[-4, 4]`.
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
/// in doubles but for one exact product: `sin(a + h) = sin a + cos a h + sin a dc
/// + cos a ds`, with `cos a h` exact and the rest, below `2^-13.7 |sin y|`, summed
/// plainly. Relative error at most `2^-62`: `2^-64.1` from the eight roundings of the
/// small terms, `2^-64.9` from `dc` (times `sin a <= 2 |sin y|`), `2^-66.7` from `ds`,
/// `2^-67` from the dropped `sl dc`, and the table's `2^-106`.
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

/// `cos y` as [`fast_sin`] gives `sin y`: `cos(a + h) = cos a - sin a h + cos a dc
/// - sin a ds`, with `sin a h` exact. Relative error at most `2^-63`: the small terms
/// are below `2^-14.5 |cos y|` (`cos y >= 0.7`), their six roundings `2^-64.9`, `dc`
/// `2^-65.4`, the rest below `2^-68`.
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

#[cfg(test)]
mod tests {
    use super::super::fast_check::{Stats, count, hard_cases, log_uniform, ulps, uniform};
    use super::*;
    use crate::hash::Rng;

    /// `cos` against its double-double path alone, bit for bit.
    fn check(x: f64, st: &mut Stats) {
        let slow = if !x.is_finite() {
            f64::NAN
        } else if x.abs() < TINY {
            1.0
        } else {
            sin_dd(x, 1).0
        };
        assert_eq!(cos(x).to_bits(), slow.to_bits(), "cos({x:e})");
        if x.is_finite() && x.abs() >= TINY {
            let (n, y) = reduce(x);
            let k = Kernel::new(y);
            let ((fh, fl), dd) = if n & 1 == 0 {
                (fast_cos(y), k.cos())
            } else {
                (fast_sin(y), k.sin())
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
