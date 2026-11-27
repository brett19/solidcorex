// Written for solidcorex. Apache-2.0.
//! The accurate phase of every `fmath` function: the value recomputed in [`Big`] at 128
//! bits, then 512 ([`big::ziv`]), when the double-double result's rounding test fails, and
//! the exact checks that decide the cases no precision can (`hypot` and `cbrt` midpoints,
//! `powf`'s exact powers). Each function is `#[cold]`: it runs for about one input in
//! 10^3 to 10^4 (`docs/research/70-fmath-correct-rounding.md`).
//!
//! The formulas are chosen so nothing cancels badly: `e^z = 2^k (1 + t)` with `t` from the
//! series of `e^r - 1` at `r 2^-s` and `s` doublings; `ln` from `2 atanh((m-1)/(m+1))`;
//! `sinh`, `cosh` and `tanh` from `E = e^|x| - 1`; the trigonometric functions from an
//! exact Payne-Hanek reduction with as many bits of `2/pi` as the precision needs, then
//! their series; the inverse ones from one arctangent series after three halvings of the
//! argument. Every step loses at most a few units of `2^(1 - 64p)`, which the error
//! constant [`C`] (`2^C` such units) covers with room to spare.

use super::big::{self, Big, ziv};
use super::dd;
use super::tables::{INV_LN2, LN2_BIG, LN10_BIG, PIO2_BIG, TWO_OVER_PI_BITS};

/// The error constant of every accurate value: within `2^(C - 1 - 64p)` relative of the
/// true one, `2^47` units of the last place. The largest budget below is under `2^16`.
const C: i64 = 48;

pub(super) fn one(p: usize) -> Big {
    Big::from_u64(1, p)
}

/// Whether `term` no longer changes a sum of magnitude about `sum` at precision `p`.
fn negligible(term: &Big, sum: &Big, p: usize) -> bool {
    term.is_zero() || term.exponent() < sum.exponent() - 64 * p as i64 - 4
}

/// `k ln 2`.
fn ln2_times(k: i64, p: usize) -> Big {
    Big::constant(&LN2_BIG, p)
        .mul_small(k.unsigned_abs())
        .with_sign(k < 0)
}

/// `e^z = 2^k (1 + t)` as `(k, t)`, `|t| < 0.42`, for `|z| < 2^20`.
///
/// `k = round(z / ln 2)` and `r = z - k ln 2` (error `|k| 2^(1-64p)`, absolute), then
/// `e^r - 1` by its series at `r 2^-s` and `s` doublings `t (t + 2)`, each relative error
/// carried through once and a few units added (`3s` in all).
pub(super) fn exp_parts(z: Big, p: usize) -> (i64, Big) {
    let k = dd::round_int(z.to_f64() * INV_LN2.0) as i64;
    let r = z.sub(ln2_times(k, p));
    let s = 2 * p as i64 + 6;
    let r = r.mul_pow2(-s);
    let mut t = r;
    let mut term = r;
    let mut n = 2u64;
    loop {
        term = term.mul(r).div_small(n);
        if negligible(&term, &t, p) {
            break;
        }
        t = t.add(term);
        n += 1;
    }
    let two = Big::from_u64(2, p);
    for _ in 0..s {
        t = t.mul(t.add(two));
    }
    (k, t)
}

/// `e^z - 1`, relative error a few units, also for small `z`.
pub(super) fn expm1(z: Big, p: usize) -> Big {
    let (k, t) = exp_parts(z, p);
    if k == 0 {
        t
    } else {
        t.add(one(p)).mul_pow2(k).sub(one(p))
    }
}

/// `e^x`, correctly rounded, for finite `x`.
#[cold]
#[inline(never)]
pub(super) fn exp(x: f64) -> f64 {
    ziv(C, |p| {
        let (k, t) = exp_parts(Big::from_f64(x, p), p);
        t.add(one(p)).mul_pow2(k)
    })
}

/// `2^x`, correctly rounded, for `x` in `[-1100, 1100]`.
#[cold]
#[inline(never)]
pub(super) fn exp2(x: f64) -> f64 {
    let k = dd::round_int(x);
    if k == x {
        // An exact power of two (rounded once: 2^-1075 ties to 0).
        return Big::from_u64(1, 1).mul_pow2(k as i64).to_f64();
    }
    // x - k is exact.
    let f = x - k;
    ziv(C, |p| {
        let z = Big::from_f64(f, p).mul(Big::constant(&LN2_BIG, p));
        let (j, t) = exp_parts(z, p);
        t.add(one(p)).mul_pow2(k as i64 + j)
    })
}

/// `sinh x`, correctly rounded, for finite `x`: `E (E + 2) / (2 (E + 1))`.
#[cold]
#[inline(never)]
pub(super) fn sinh(x: f64) -> f64 {
    let r = ziv(C, |p| {
        let e = expm1(Big::from_f64(x.abs(), p), p);
        let two = Big::from_u64(2, p);
        e.mul(e.add(two)).div(e.add(one(p)).mul_pow2(1))
    });
    if x < 0.0 { -r } else { r }
}

/// `cosh x`, correctly rounded, for finite `x`: `1 + E^2 / (2 (E + 1))`.
#[cold]
#[inline(never)]
pub(super) fn cosh(x: f64) -> f64 {
    ziv(C, |p| {
        let e = expm1(Big::from_f64(x.abs(), p), p);
        one(p).add(e.mul(e).div(e.add(one(p)).mul_pow2(1)))
    })
}

/// `tanh x`, correctly rounded, for finite `x`: `F / (F + 2)` with `F = e^(2|x|) - 1`.
#[cold]
#[inline(never)]
pub(super) fn tanh(x: f64) -> f64 {
    let r = ziv(C, |p| {
        let f = expm1(Big::from_f64(2.0 * x.abs(), p), p);
        f.div(f.add(Big::from_u64(2, p)))
    });
    if x < 0.0 { -r } else { r }
}

/// `ln x = e ln 2 + ln m` for a finite positive `x`, as `(e, ln m)`, with `m` in
/// `[sqrt(2)/2, sqrt(2))`: `ln m = 2 atanh(s)`, `s = (m - 1)/(m + 1)`, `|s| < 0.172`.
pub(super) fn ln_parts(x: f64, p: usize) -> (i64, Big) {
    let mut e = i64::from(dd::exponent(x));
    let mut m = dd::mantissa(x);
    if m > core::f64::consts::SQRT_2 {
        m *= 0.5;
        e += 1;
    }
    let mb = Big::from_f64(m, p);
    let s = mb.sub(one(p)).div(mb.add(one(p)));
    let s2 = s.mul(s);
    let mut sum = s;
    let mut term = s;
    let mut n = 1u64;
    loop {
        term = term.mul(s2);
        let q = term.div_small(2 * n + 1);
        if negligible(&q, &sum, p) {
            break;
        }
        sum = sum.add(q);
        n += 1;
    }
    (e, sum.mul_pow2(1))
}

/// `ln x` in [`Big`], for a finite positive `x`.
pub(super) fn ln_big(x: f64, p: usize) -> Big {
    let (e, lm) = ln_parts(x, p);
    ln2_times(e, p).add(lm)
}

/// `ln x`, correctly rounded, for a finite positive `x`.
#[cold]
#[inline(never)]
pub(super) fn ln(x: f64) -> f64 {
    ziv(C, |p| ln_big(x, p))
}

/// `log2 x`, correctly rounded, for a finite positive `x`.
#[cold]
#[inline(never)]
pub(super) fn log2(x: f64) -> f64 {
    ziv(C, |p| {
        let (e, lm) = ln_parts(x, p);
        Big::from_i64(e, p).add(lm.div(Big::constant(&LN2_BIG, p)))
    })
}

/// `log10 x`, correctly rounded, for a finite positive `x`.
#[cold]
#[inline(never)]
pub(super) fn log10(x: f64) -> f64 {
    ziv(C, |p| ln_big(x, p).div(Big::constant(&LN10_BIG, p)))
}

/// Bits `b_(i-63) .. b_i` of `2/pi = 0.b_1 b_2 ...` as one word (`b_i` lowest), the bits
/// before `b_i0` cleared.
fn two_over_pi_chunk(i: i64, i0: i64) -> u64 {
    let start = i - 63;
    let pos = start - 1;
    let (w, off) = (pos.div_euclid(64), pos.rem_euclid(64) as u32);
    let word = |k: i64| -> u64 {
        if k < 0 {
            0
        } else {
            dd::at(&TWO_OVER_PI_BITS, k as usize)
        }
    };
    let mut c = if off == 0 {
        word(w)
    } else {
        (word(w) << off) | (word(w + 1) >> (64 - off))
    };
    if start < i0 {
        let drop = i0 - start;
        c = if drop >= 64 { 0 } else { c & (u64::MAX >> drop) };
    }
    c
}

/// `(n mod 4, y)` with `a = n pi/2 + y`, `|y| <= pi/4`, for a finite `a >= 0`.
///
/// Payne-Hanek, exactly: `a = mant 2^q` with `mant` an integer, and the bits `b_i` of
/// `2/pi` with `i < q - 1` only add multiples of 4 to `a 2/pi`, so the window starts there
/// and runs to `F = 64p + 80` bits past the binary point (the tail is below `2^-F`). No
/// double lies within `2^-62` of a multiple of `pi/2` (`docs/research/70`), so `y` keeps
/// `64p + 17` bits.
pub(super) fn reduce(a: f64, p: usize) -> (u32, Big) {
    if a < 0.78 {
        return (0, Big::from_f64(a, p));
    }
    let bits = a.to_bits();
    let mant = (bits & 0x000f_ffff_ffff_ffff) | (1 << 52);
    let q = ((bits >> 52) as i64) - 1075;
    let f_bits = 64 * p as i64 + 80;
    let i_end = q + 53 + f_bits;
    let i0 = (q - 1).max(1);
    let len = i_end - i0 + 1;
    let nl = len.div_euclid(64) as usize + 1;
    // W = sum b_i 2^(i_end - i), little-endian, then P = mant W.
    let mut prod = [0u64; 16];
    let mut carry = 0u128;
    for (j, s) in prod.iter_mut().take(nl + 1).enumerate() {
        let w = if j < nl {
            two_over_pi_chunk(i_end - 64 * j as i64, i0)
        } else {
            0
        };
        let t = u128::from(w) * u128::from(mant) + carry;
        *s = t as u64;
        carry = t >> 64;
    }
    // P 2^-sh is a 2/pi mod 4 (plus multiples of 4), sh = i_end - q.
    let sh = i_end - q;
    let bit = |k: i64| -> u64 { (big_limb(&prod, k.div_euclid(64)) >> k.rem_euclid(64)) & 1 };
    let mut n = (bit(sh) | (bit(sh + 1) << 1)) as u32;
    let half = bit(sh - 1) == 1;
    // The fraction: the low sh bits of P, as fl limbs.
    let fl = (sh as usize).div_ceil(64);
    let mut fr = [0u64; 16];
    for (d, s) in fr.iter_mut().zip(prod.iter()).take(fl) {
        *d = *s;
    }
    let top_bits = sh % 64;
    let mask_top = |fr: &mut [u64; 16]| {
        if top_bits != 0
            && let Some(t) = fr.get_mut(fl - 1)
        {
            *t &= (1u64 << top_bits) - 1;
        }
    };
    mask_top(&mut fr);
    if half {
        // f - 1 < 0: its magnitude is 2^sh - frac (two's complement in sh bits).
        n += 1;
        let mut borrow = true;
        for s in fr.iter_mut().take(fl) {
            let (v, b) = (!*s).overflowing_add(u64::from(borrow));
            *s = v;
            borrow = b;
        }
        mask_top(&mut fr);
    }
    let mut be = [0u64; 16];
    for (k, s) in be.iter_mut().take(fl).enumerate() {
        *s = big_limb(&fr, (fl - 1 - k) as i64);
    }
    let f = Big::from_parts(half, 64 * fl as i64 - sh, be.get(..fl).unwrap_or(&[]), p);
    (n & 3, f.mul(Big::constant(&PIO2_BIG, p)))
}

fn big_limb(s: &[u64; 16], i: i64) -> u64 {
    if i < 0 { 0 } else { s.get(i as usize).copied().unwrap_or(0) }
}

/// `sin y` by its series, for `|y| <= pi/4`.
pub(super) fn sin_series(y: Big, p: usize) -> Big {
    let y2 = y.mul(y);
    let mut s = y;
    let mut term = y;
    let mut k = 1u64;
    loop {
        term = term.mul(y2).div_small((2 * k) * (2 * k + 1)).neg();
        if negligible(&term, &s, p) {
            return s;
        }
        s = s.add(term);
        k += 1;
    }
}

/// `cos y` by its series, for `|y| <= pi/4`.
pub(super) fn cos_series(y: Big, p: usize) -> Big {
    let y2 = y.mul(y);
    let mut c = one(p);
    let mut term = one(p);
    let mut k = 1u64;
    loop {
        term = term.mul(y2).div_small((2 * k - 1) * (2 * k)).neg();
        if negligible(&term, &c, p) {
            return c;
        }
        c = c.add(term);
        k += 1;
    }
}

/// `sin(x + quarters pi/2)`, correctly rounded, for a finite nonzero `x`.
#[cold]
#[inline(never)]
pub(super) fn sin_quadrant(x: f64, quarters: u32) -> f64 {
    let neg = x < 0.0;
    ziv(C, |p| {
        let (n, y) = reduce(x.abs(), p);
        // sin(-a + q pi/2) = -sin(a - q pi/2) = -sin(a + (4 - q) pi/2)... by quadrant:
        let q = if neg {
            (n + 4 - (quarters & 3)) & 3
        } else {
            (n + quarters) & 3
        };
        let v = match q {
            0 => sin_series(y, p),
            1 => cos_series(y, p),
            2 => sin_series(y, p).neg(),
            _ => cos_series(y, p).neg(),
        };
        if neg { v.neg() } else { v }
    })
}

/// `tan x`, correctly rounded, for a finite nonzero `x`.
#[cold]
#[inline(never)]
pub(super) fn tan(x: f64) -> f64 {
    let r = ziv(C, |p| {
        let (n, y) = reduce(x.abs(), p);
        let (s, c) = (sin_series(y, p), cos_series(y, p));
        if n & 1 == 0 { s.div(c) } else { c.div(s).neg() }
    });
    if x < 0.0 { -r } else { r }
}

/// `atan t` for `t >= 0`: `pi/2 - atan(1/t)` above 1, three halvings
/// `t / (1 + sqrt(1 + t^2))` to below `tan(pi/32)`, then the series.
pub(super) fn atan_big(t: Big, p: usize) -> Big {
    if t.is_zero() {
        return t;
    }
    let inv = t.cmp_mag(&one(p)) == core::cmp::Ordering::Greater;
    let mut u = if inv { t.recip() } else { t };
    for _ in 0..3 {
        u = u.div(one(p).add(one(p).add(u.mul(u)).sqrt()));
    }
    let u2 = u.mul(u);
    let mut sum = u;
    let mut term = u;
    let mut k = 1u64;
    loop {
        term = term.mul(u2).neg();
        let q = term.div_small(2 * k + 1);
        if negligible(&q, &sum, p) {
            break;
        }
        sum = sum.add(q);
        k += 1;
    }
    let a = sum.mul_pow2(3);
    if inv {
        Big::constant(&PIO2_BIG, p).sub(a)
    } else {
        a
    }
}

/// `atan x`, correctly rounded, for a finite nonzero `x`.
#[cold]
#[inline(never)]
pub(super) fn atan(x: f64) -> f64 {
    let r = ziv(C, |p| atan_big(Big::from_f64(x.abs(), p), p));
    if x < 0.0 { -r } else { r }
}

/// `asin x`, correctly rounded, for `0 < |x| <= 1`: `atan(|x| / sqrt((1 - |x|)(1 + |x|)))`.
#[cold]
#[inline(never)]
pub(super) fn asin(x: f64) -> f64 {
    let a = x.abs();
    let r = ziv(C, |p| {
        if a == 1.0 {
            return Big::constant(&PIO2_BIG, p);
        }
        let ab = Big::from_f64(a, p);
        let c = one(p).sub(ab).mul(one(p).add(ab)).sqrt();
        atan_big(ab.div(c), p)
    });
    if x < 0.0 { -r } else { r }
}

/// `acos x`, correctly rounded, for `|x| <= 1`: `2 atan(sqrt((1 - x)/(1 + x)))`.
#[cold]
#[inline(never)]
pub(super) fn acos(x: f64) -> f64 {
    ziv(C, |p| {
        if x == -1.0 {
            return Big::constant(&PIO2_BIG, p).mul_pow2(1);
        }
        let xb = Big::from_f64(x, p);
        let t = one(p).sub(xb).div(one(p).add(xb)).sqrt();
        atan_big(t, p).mul_pow2(1)
    })
}

/// `atan2(y, x)`, correctly rounded, for finite nonzero `y` and `x`.
#[cold]
#[inline(never)]
pub(super) fn atan2(y: f64, x: f64) -> f64 {
    if x > 0.0 && y.abs() < x.abs() {
        // t = |y|/x below 2^-60: atan t lies in (t - t 2^-100, t), which holds no double
        // and no midpoint between two (their spacing near t is above t 2^-54 or, among
        // the subnormals, 2^-1075 > t 2^-100), as does t' (1 - 2^-100) for t' within
        // 2^-188 of t. That rounds as atan t; t itself may be a midpoint (a subnormal
        // quotient), which no ball around atan t decides.
        let t = Big::from_f64(y.abs(), 3).div(Big::from_f64(x, 3));
        if t.exponent() < -60 {
            let r = t.sub(t.mul_pow2(-100)).to_f64();
            return if y < 0.0 { -r } else { r };
        }
    }
    let r = ziv(C, |p| {
        let (a, b) = (Big::from_f64(y.abs(), p), Big::from_f64(x.abs(), p));
        let pio2 = Big::constant(&PIO2_BIG, p);
        let th = if y.abs() <= x.abs() {
            atan_big(a.div(b), p)
        } else {
            pio2.sub(atan_big(b.div(a), p))
        };
        if x < 0.0 { pio2.mul_pow2(1).sub(th) } else { th }
    });
    if y < 0.0 { -r } else { r }
}

/// `x^y` in [`Big`] for a finite positive `x`: `e^(y ln x)`, for `|y ln x| < 2^20`.
pub(super) fn pow_big(x: f64, y: f64, p: usize) -> Big {
    let z = Big::from_f64(y, p).mul(ln_big(x, p));
    let (k, t) = exp_parts(z, p);
    t.add(one(p)).mul_pow2(k)
}

/// `x = mant 2^e` with `mant` odd, for a finite nonzero `x` (sign dropped).
fn odd_split(x: f64) -> (u64, i64) {
    let b = x.to_bits() & 0x7fff_ffff_ffff_ffff;
    let field = (b >> 52) as i64;
    let frac = b & 0x000f_ffff_ffff_ffff;
    let (m, e) = if field == 0 {
        (frac, -1074)
    } else {
        (frac | (1 << 52), field - 1075)
    };
    let tz = m.trailing_zeros();
    (m >> tz, e + i64::from(tz))
}

/// `r` with `r^2 = m`, if `m` is a perfect square.
fn exact_sqrt(m: u64) -> Option<u64> {
    let r = (m as f64).sqrt() as u64;
    (r.saturating_sub(1)..=r + 1).find(|&c| u128::from(c) * u128::from(c) == u128::from(m))
}

/// `|x|^y` rounded, when it is a dyadic rational (so possibly exactly representable or
/// exactly half-way between two doubles, which no precision decides), for finite `x`,
/// `y` with `x` nonzero; `None` when it is irrational or not dyadic.
///
/// `|x| = M 2^E` with `M` odd. `M = 1`: `2^(E y)`, dyadic only for an integer `E y`. `M >=
/// 3`: `y` must be positive, and `y = N / 2^K` with `N` odd and `M` a perfect `2^K`-th
/// power `R^(2^K)` (`K <= 5`, as `3^64 > 2^53`) with `2^K | E`, giving `R^N 2^(E N / 2^K)`,
/// which needs `R^N < 2^64` to be close to a double at all.
fn exact_pow(x: f64, y: f64) -> Option<f64> {
    let (m, e) = odd_split(x);
    let (yn, yf) = odd_split(y);
    let y_neg = y < 0.0;
    if m == 1 {
        // 2^(e y), y = yn 2^yf with yn odd: an integer exactly when 2^-yf divides e.
        let g = if yf >= 0 {
            if yf > 12 && e != 0 {
                return None;
            }
            i128::from(e) * i128::from(yn) << yf.clamp(0, 12)
        } else {
            let k = -yf;
            if k > 11 || e % (1i64 << k) != 0 {
                return None;
            }
            i128::from(e >> k) * i128::from(yn)
        };
        let g = if y_neg { -g } else { g };
        if g.abs() > 3000 {
            return None;
        }
        return Some(Big::from_u64(1, 1).mul_pow2(g as i64).to_f64());
    }
    if y_neg {
        return None;
    }
    // y = N 2^yf; root of order 2^K for K = -yf.
    let (n, k) = if yf >= 0 {
        if yf > 6 {
            return None;
        }
        (yn << yf, 0)
    } else {
        (yn, (-yf) as u32)
    };
    if k > 5 || n > 64 {
        return None;
    }
    let mut r = m;
    for _ in 0..k {
        r = exact_sqrt(r)?;
    }
    if e % (1i64 << k) != 0 {
        return None;
    }
    let mut w = 1u128;
    for _ in 0..n {
        w = w.checked_mul(u128::from(r))?;
        if w >= 1 << 64 {
            return None;
        }
    }
    let g = e * n as i64 / (1i64 << k);
    Some(Big::from_u64(w as u64, 2).mul_pow2(g).to_f64())
}

/// `|x|^y`, correctly rounded, for finite nonzero `x` and `y` with `|y ln |x||` below
/// 750: the exact check, then the accurate phase.
#[cold]
#[inline(never)]
pub(super) fn pow(x: f64, y: f64) -> f64 {
    let a = x.abs();
    if let Some(r) = exact_pow(a, y) {
        return r;
    }
    ziv(C + 12, |p| pow_big(a, y, p))
}

/// `|x|^n`, correctly rounded, for a finite nonzero `x`: the exact check, then squaring
/// in [`Big`] (at most 62 products and a reciprocal, each one unit).
#[cold]
#[inline(never)]
pub(super) fn powi(x: f64, n: i32) -> f64 {
    let a = x.abs();
    if let Some(r) = exact_pow(a, f64::from(n)) {
        return r;
    }
    ziv(C, |p| {
        let mut base = Big::from_f64(a, p);
        let mut acc = one(p);
        let mut k = n.unsigned_abs();
        while k > 0 {
            if k & 1 == 1 {
                acc = acc.mul(base);
            }
            k >>= 1;
            if k > 0 {
                base = base.mul(base);
            }
        }
        if n < 0 { acc.recip() } else { acc }
    })
}

/// The double nearest `sqrt(a^2 + b^2)`, given `r` within one ulp of it, for finite
/// nonnegative `a` and `b` (not both zero): `r` moved to a neighbour while the square of
/// the midpoint between them says the root lies past it, a tie to even. Exact: the sum of
/// squares spans under 240 bits and the midpoints' squares 108, inside 320.
#[cold]
#[inline(never)]
pub(super) fn hypot_exact(a: f64, b: f64, r: f64) -> f64 {
    const P: usize = 5;
    let (ba, bb) = (Big::from_f64(a, P), Big::from_f64(b, P));
    let s = ba.mul(ba).add(bb.mul(bb));
    // s - mid^2 for the midpoint between r and its neighbour above (up) or below.
    let past = |r: f64, up: bool| -> Big {
        let mid = midpoint(r, up, P);
        s.sub(mid.mul(mid))
    };
    nearest_root(r, |r, up| past(r, up))
}

/// The double nearest `cbrt(x)` for a finite positive `x`, given `r` within one ulp of
/// it, as [`hypot_exact`]: the cube of a 54-bit midpoint is 162 bits, inside 256, and
/// never equals a double, so no tie arises.
#[cold]
#[inline(never)]
pub(super) fn cbrt_exact(x: f64, r: f64) -> f64 {
    const P: usize = 4;
    let bx = Big::from_f64(x, P);
    nearest_root(r, |r, up| {
        let mid = midpoint(r, up, P);
        bx.sub(mid.mul(mid).mul(mid))
    })
}

/// The midpoint between a positive finite `r` and its neighbour above (`up`) or below,
/// exactly (the one above `f64::MAX` is `2^1024 - 2^970`).
fn midpoint(r: f64, up: bool, p: usize) -> Big {
    let bits = r.to_bits();
    let field = (bits >> 52) as i64;
    let ulp_exp = field.max(1) - 1075;
    // Below a power of two (not the least normal) the gap is half as wide.
    let half = if !up && bits & 0x000f_ffff_ffff_ffff == 0 && field > 1 {
        ulp_exp - 2
    } else {
        ulp_exp - 1
    };
    let h = Big::from_u64(1, p).mul_pow2(half);
    let rb = Big::from_f64(r, p);
    if up { rb.add(h) } else { rb.sub(h) }
}

/// Steps `r` (positive, or infinite: read as `f64::MAX`) to the nearest double, where
/// `past(r, up)` is the value minus the midpoint's power (positive: the value lies above
/// that midpoint).
fn nearest_root(r: f64, past: impl Fn(f64, bool) -> Big) -> f64 {
    let mut r = if r.is_finite() { r } else { f64::MAX };
    for _ in 0..4 {
        let d = past(r, true);
        if d.is_zero() {
            // a tie between r and the next: to even
            let next = f64::from_bits(r.to_bits() + 1);
            return if r.to_bits() & 1 == 0 { r } else { next };
        }
        if !d.is_neg() {
            r = f64::from_bits(r.to_bits() + 1);
            if r.is_infinite() {
                return r;
            }
            continue;
        }
        if r.to_bits() > 0 {
            let d = past(r, false);
            if d.is_zero() {
                let prev = f64::from_bits(r.to_bits() - 1);
                return if r.to_bits() & 1 == 0 { r } else { prev };
            }
            if d.is_neg() {
                r = f64::from_bits(r.to_bits() - 1);
                continue;
            }
        }
        return r;
    }
    r
}

#[cfg(test)]
mod tests {
    use super::*;
    use scx_testkit::mp;

    /// The inputs of a CORE-MATH `.wc` file: `x` or `x,y` per line.
    fn inputs(text: &str) -> Vec<(f64, f64)> {
        let num = |s: &str| mp::parse_f64(s.split_whitespace().next().unwrap_or("")).unwrap();
        text.lines()
            .filter_map(|l| {
                let d = l.split('#').next()?.trim();
                if d.is_empty() {
                    return None;
                }
                let mut it = d.split(',');
                let x = num(it.next()?);
                Some((x, it.next().map_or(0.0, num)))
            })
            .collect()
    }

    macro_rules! wc {
        ($name:literal) => {
            inputs(include_str!(concat!("../../tests/data/core-math/", $name, ".wc")))
        };
    }

    /// Checks `got` against `want` at every input where `want` decides it.
    fn agree(
        name: &str,
        xs: &[(f64, f64)],
        ok: impl Fn(f64, f64) -> bool,
        got: impl Fn(f64, f64) -> f64,
        want: impl Fn(f64, f64) -> Option<f64>,
    ) {
        let mut bad = Vec::new();
        let mut n = 0;
        for &(x, y) in xs {
            if !ok(x, y) {
                continue;
            }
            let Some(w) = want(x, y) else { continue };
            n += 1;
            let g = got(x, y);
            if g.to_bits() != w.to_bits() {
                bad.push(format!("{name}({x:e}, {y:e}): got {g:e}, want {w:e}"));
            }
        }
        assert!(n > 0, "{name}: nothing checked");
        assert!(bad.is_empty(), "{} of {n} wrong:\n{}", bad.len(), bad.join("\n"));
    }

    fn fin(x: f64) -> bool {
        x.is_finite() && x != 0.0
    }

    #[test]
    fn exponentials_at_the_worst_cases() {
        agree("exp", &wc!("exp"), |x, _| fin(x) && x.abs() < 1000.0, |x, _| exp(x), |x, _| mp::exp(x));
        agree("exp2", &wc!("exp2"), |x, _| fin(x) && x.abs() < 1100.0, |x, _| exp2(x), |x, _| mp::exp2(x));
        let h = |x: f64, _| fin(x) && x.abs() < 1000.0;
        agree("sinh", &wc!("sinh"), h, |x, _| sinh(x), |x, _| mp::sinh(x));
        agree("cosh", &wc!("cosh"), h, |x, _| cosh(x), |x, _| mp::cosh(x));
        agree("tanh", &wc!("tanh"), h, |x, _| tanh(x), |x, _| mp::tanh(x));
    }

    #[test]
    fn logarithms_at_the_worst_cases() {
        let pos = |x: f64, _| fin(x) && x > 0.0;
        agree("ln", &wc!("log"), pos, |x, _| ln(x), |x, _| mp::ln(x));
        agree("log2", &wc!("log2"), pos, |x, _| log2(x), |x, _| mp::log2(x));
        agree("log10", &wc!("log10"), pos, |x, _| log10(x), |x, _| mp::log10(x));
    }

    #[test]
    fn trigonometric_at_the_worst_cases() {
        let f = |x: f64, _| fin(x);
        agree("sin", &wc!("sin"), f, |x, _| sin_quadrant(x, 0), |x, _| mp::sin(x));
        agree("cos", &wc!("cos"), f, |x, _| sin_quadrant(x, 1), |x, _| mp::cos(x));
        agree("tan", &wc!("tan"), f, |x, _| tan(x), |x, _| mp::tan(x));
    }

    #[test]
    fn inverse_at_the_worst_cases() {
        agree("atan", &wc!("atan"), |x, _| fin(x), |x, _| atan(x), |x, _| mp::atan(x));
        let unit = |x: f64, _| fin(x) && x.abs() <= 1.0;
        agree("asin", &wc!("asin"), unit, |x, _| asin(x), |x, _| mp::asin(x));
        agree("acos", &wc!("acos"), |x, _| x.abs() <= 1.0, |x, _| acos(x), |x, _| mp::acos(x));
        agree("atan2", &wc!("atan2"), |y, x| fin(x) && fin(y), atan2, mp::atan2);
    }

    #[test]
    fn roots_and_powers_at_the_worst_cases() {
        agree(
            "cbrt",
            &wc!("cbrt"),
            |x, _| fin(x),
            |x, _| {
                let r = cbrt_exact(x.abs(), crate::fmath::cbrt(x).abs());
                if x < 0.0 { -r } else { r }
            },
            |x, _| mp::cbrt(x),
        );
        agree(
            "hypot",
            &wc!("hypot"),
            |x, y| fin(x) && fin(y),
            |x, y| hypot_exact(x.abs(), y.abs(), crate::fmath::hypot(x, y)),
            mp::hypot,
        );
        agree(
            "pow",
            &wc!("pow"),
            |x, y| {
                fin(x) && x > 0.0 && fin(y) && (y * crate::fmath::ln(x)).abs() < 740.0
            },
            pow,
            mp::pow,
        );
        for (x, y) in [(1781.0, 5.0), (194701.5625, 2.5), (2f64.powi(-537), 2.0)] {
            assert_eq!(pow(x, y).to_bits(), mp::pow(x, y).unwrap().to_bits(), "{x} {y}");
        }
    }

    /// Random inputs over each function's whole range (bit patterns, so every binade),
    /// against the reference.
    #[test]
    fn random_inputs_over_the_ranges() {
        let mut st = 0x9e37_79b9_7f4a_7c15u64;
        let mut next = move || {
            st ^= st << 13;
            st ^= st >> 7;
            st ^= st << 17;
            st
        };
        let mut any = |lo: f64, hi: f64| -> f64 {
            loop {
                let x = f64::from_bits(next());
                if x.is_finite() && x != 0.0 && x.abs() >= lo && x.abs() <= hi {
                    return x;
                }
            }
        };
        let mut xs = Vec::new();
        for _ in 0..300 {
            xs.push((any(0.0, 1000.0), any(0.0, 1000.0)));
            xs.push((any(0.0, f64::MAX), any(0.0, f64::MAX)));
            xs.push((any(0.0, 1.0), any(0.0, 20.0)));
        }
        let f = |x: f64, _| fin(x);
        let small = |x: f64, _| fin(x) && x.abs() < 1000.0;
        let pos = |x: f64, _| fin(x) && x > 0.0;
        agree("exp", &xs, small, |x, _| exp(x), |x, _| mp::exp(x));
        agree("exp2", &xs, small, |x, _| exp2(x), |x, _| mp::exp2(x));
        agree("sinh", &xs, small, |x, _| sinh(x), |x, _| mp::sinh(x));
        agree("cosh", &xs, small, |x, _| cosh(x), |x, _| mp::cosh(x));
        agree("tanh", &xs, small, |x, _| tanh(x), |x, _| mp::tanh(x));
        agree("ln", &xs, pos, |x, _| ln(x), |x, _| mp::ln(x));
        agree("log2", &xs, pos, |x, _| log2(x), |x, _| mp::log2(x));
        agree("log10", &xs, pos, |x, _| log10(x), |x, _| mp::log10(x));
        agree("sin", &xs, f, |x, _| sin_quadrant(x, 0), |x, _| mp::sin(x));
        agree("cos", &xs, f, |x, _| sin_quadrant(x, 1), |x, _| mp::cos(x));
        agree("tan", &xs, f, |x, _| tan(x), |x, _| mp::tan(x));
        agree("atan", &xs, f, |x, _| atan(x), |x, _| mp::atan(x));
        let unit = |x: f64, _| fin(x) && x.abs() <= 1.0;
        agree("asin", &xs, unit, |x, _| asin(x), |x, _| mp::asin(x));
        agree("acos", &xs, unit, |x, _| acos(x), |x, _| mp::acos(x));
        agree("atan2", &xs, |y, x| fin(x) && fin(y), atan2, mp::atan2);
        agree(
            "pow",
            &xs,
            |x, y| fin(x) && x > 0.0 && fin(y) && (y * crate::fmath::ln(x)).abs() < 740.0,
            pow,
            mp::pow,
        );
    }
}
