// Written for solidcorex. Apache-2.0.
//! Double-double helpers for `fmath`: a value `hi + lo` with `|lo| <= ulp(hi)/2` once
//! normalised, built only from the error-free transformations in [`crate::eft`].
//!
//! Each operation's relative error is a few units of `2^-104` unless noted; none is a
//! correctly rounded double-double operation, which `fmath` does not need.

use crate::eft::{fast_two_sum, two_prod, two_sum};

/// `1.5 * 2^52`: adding and subtracting it rounds a double of magnitude below `2^51` to
/// the nearest integer, ties to even.
pub(super) const ROUND_SHIFT: f64 = 6_755_399_441_055_744.0;

/// `x` rounded to the nearest integer (ties to even), for `|x| < 2^51`.
#[inline]
pub(super) fn round_int(x: f64) -> f64 {
    (x + ROUND_SHIFT) - ROUND_SHIFT
}

/// `2^k` for `k` in `-1022..=1023` (a normal power of two).
#[inline]
pub(super) fn pow2(k: i32) -> f64 {
    f64::from_bits(((1023 + i64::from(k)) as u64) << 52)
}

/// `x * 2^k` for any `k`, rounding once: exact whenever the result is normal, correctly
/// rounded when it is subnormal, infinite when it overflows. Zero, infinities and NaN come
/// back unchanged (no sign is read from a NaN).
pub(crate) fn scale(x: f64, k: i32) -> f64 {
    if let Some(r) = scale_normal(x, k) {
        return r;
    }
    if x == 0.0 || !x.is_finite() {
        return x;
    }
    // x = m * 2^ex with |m| in [1, 2), exactly.
    let ex = exponent(x);
    let m = mantissa(x);
    let t = ex.saturating_add(k);
    if t > 1023 {
        f64::INFINITY.copysign(x)
    } else if t >= -1022 {
        m * pow2(t)
    } else if t >= -1080 {
        // m * 2^-1022 is exact; the second product rounds once into the subnormals.
        m * pow2(-1022) * pow2(t + 1022)
    } else {
        0.0f64.copysign(x)
    }
}

/// `x * 2^k` by moving the exponent field, when `x` and the result are both normal (the
/// common case, and exact); `None` otherwise.
#[inline]
fn scale_normal(x: f64, k: i32) -> Option<f64> {
    let b = x.to_bits();
    let field = ((b >> 52) & 0x7ff) as i32;
    let t = field.wrapping_add(k);
    if field != 0 && field != 0x7ff && (1..=0x7fe).contains(&t) {
        Some(f64::from_bits(b.wrapping_add((i64::from(k) as u64) << 52)))
    } else {
        None
    }
}

/// The double nearest `(hi + lo) * 2^k` (ties to even), for a normalised pair
/// (`hi = fl(hi + lo)`) with `hi` finite and nonzero and any `k`: one rounding, also
/// when the result is subnormal or overflows.
pub(crate) fn round_scaled(hi: f64, lo: f64, k: i32) -> f64 {
    if let Some(r) = scale_normal(hi, k) {
        return r;
    }
    let e = exponent(hi).saturating_add(k);
    if e >= -1022 {
        // Normal or overflowing: hi already is the rounded sum, and the scaling is exact.
        return scale(hi, k);
    }
    if e < -1076 {
        // Below half the least subnormal: rounds to zero.
        return 0.0f64.copysign(hi);
    }
    // Subnormal. Round hi alone onto the subnormal grid, then correct the one case where
    // lo decides: hi exactly half-way between two grid points.
    let r = scale(hi, k);
    let d = hi - scale(r, -k);
    let half = scale(1.0, -1075 - k).copysign(hi);
    let step = f64::from_bits(1).copysign(hi);
    if d == half && lo * hi > 0.0 {
        // hi lay half-way and rounded toward zero; lo is beyond the half.
        r + step
    } else if d == -half && lo * hi < 0.0 {
        // hi lay half-way and rounded away from zero; lo is short of the half.
        r - step
    } else {
        r
    }
}

/// `x / 2^exponent(x)`, the significand in `[1, 2)` with the sign of `x`, exactly, for a
/// finite nonzero `x` (also subnormal). Read from the bits: `2^-exponent(x)` is no normal
/// double for exponents 1023 and below -1022, so no multiplication by [`pow2`] reaches it.
#[inline]
pub(crate) fn mantissa(x: f64) -> f64 {
    const KEEP: u64 = 0x800f_ffff_ffff_ffff; // sign and fraction
    const ONE: u64 = 0x3ff0_0000_0000_0000;
    let b = if x.to_bits() & 0x7ff0_0000_0000_0000 == 0 {
        // Subnormal: x 2^54 is normal and exact, with the same significand.
        (x * pow2(54)).to_bits()
    } else {
        x.to_bits()
    };
    f64::from_bits((b & KEEP) | ONE)
}

/// The unbiased binary exponent of a finite nonzero double (`floor(log2 |x|)`), also for
/// subnormals.
#[inline]
pub(crate) fn exponent(x: f64) -> i32 {
    let b = x.to_bits() & 0x7fff_ffff_ffff_ffff;
    let biased = (b >> 52) as i32;
    if biased == 0 {
        // Subnormal: position of the leading bit.
        -1011 - b.leading_zeros() as i32
    } else {
        biased - 1023
    }
}

/// `(a + b)` for two double-doubles, normalised.
#[inline]
pub(crate) fn add(a: (f64, f64), b: (f64, f64)) -> (f64, f64) {
    let (s, e) = two_sum(a.0, b.0);
    fast_two_sum(s, e + a.1 + b.1)
}

/// `a * b` for two double-doubles, normalised.
#[inline]
pub(crate) fn mul(a: (f64, f64), b: (f64, f64)) -> (f64, f64) {
    let (p, e) = two_prod(a.0, b.0);
    fast_two_sum(p, e + (a.0 * b.1 + a.1 * b.0))
}

/// `a / b` for two double-doubles, normalised.
#[inline]
pub(crate) fn div(a: (f64, f64), b: (f64, f64)) -> (f64, f64) {
    let q = a.0 / b.0;
    let (p, e) = two_prod(q, b.0);
    let r = (a.0 - p) - e + a.1 - q * b.1;
    fast_two_sum(q, r / b.0)
}

/// `1 / b` for a double-double, normalised.
#[inline]
pub(super) fn recip(b: (f64, f64)) -> (f64, f64) {
    let q = 1.0 / b.0;
    let (p, e) = two_prod(q, b.0);
    let r = (1.0 - p) - e - q * b.1;
    fast_two_sum(q, r / b.0)
}

/// `sqrt(a)` for a positive double-double, normalised.
#[inline]
pub(crate) fn sqrt(a: (f64, f64)) -> (f64, f64) {
    let s = a.0.sqrt();
    let (p, e) = two_prod(s, s);
    let r = (a.0 - p) - e + a.1;
    fast_two_sum(s, r / (2.0 * s))
}

/// Ziv's rounding test, in the form `CRlibm` uses (Ziv, "Fast evaluation of elementary
/// mathematical functions with correctly rounded last bit", ACM TOMS 17, 1991; de
/// Dinechin, Lauter and Muller, "Fast and correctly rounded logarithms in
/// double-precision", RAIRO-ITA 41, 2007), with its constant derived here: whether every
/// real within `eps |hi|` of `hi + lo` rounds to `hi`, for a normal nonzero `hi`, a `lo`
/// whose product with `e` does not underflow, and `e > (1 + 2^-53) / (1 - 2^54 eps)`.
///
/// Derivation (round to nearest). Let `w = fl(lo e)`, so `|lo| e <= |w| (1 + 2^-53)`.
/// The test `hi + w == hi` holds only if `|w|` is at most half the gap `g` between `hi`
/// and its neighbour on the side of `lo`, so `|lo| <= (g/2)(1 + 2^-53)/e`. For
/// `hi` in `[2^k, 2^(k+1))` that half-gap is `2^(k-53)`, or `2^(k-54)` below an exact
/// power of two, and so never below `2^-54 |hi|`. A real `v` with `|v - hi - lo| <= eps
/// |hi|` then has `|v - hi| <= (g/2)(1 + 2^-53)/e + eps |hi| < g/2` exactly when
/// `eps |hi| < (g/2)(1 - (1 + 2^-53)/e)`, which `eps < 2^-54 (1 - (1 + 2^-53)/e)`
/// guarantees: `v` is strictly inside the rounding interval of `hi`, no tie included.
///
/// Each fast path below states its own `eps` and the `e` it uses. The `eps` covers the
/// fast path's error and the double-double path's together, both against the same exact
/// value, so a passing test means the double-double path rounds to the same `hi`: the
/// fast path changes no result, only how often the slow path runs.
#[inline]
pub(super) fn rounds_to_hi(hi: f64, lo: f64, e: f64) -> bool {
    hi + lo * e == hi
}

/// The rounded `hi + lo` when every real within `r |hi|` of it rounds the same way, else
/// `accurate()`. For a normalised pair with a finite nonzero `hi`, a normal result, and
/// `r` at least twice the pair's proven relative error bound (the margin absorbs the
/// tests' own roundings below), `r <= 2^-56`.
///
/// First [`rounds_to_hi`] with `e = 1 + 2^54 r`, whose `eps = 2^-54 (1 - (1 + 2^-53)/e)`
/// is above `r (1 - 2^54 r) - 2^-107`, so above half of `r` and the bound: three
/// operations. When it fails, [`round_ball`] (whether the ends' bits agree, a little
/// sharper), then the accurate phase.
#[inline]
pub(super) fn round_or(hi: f64, lo: f64, r: f64, accurate: impl FnOnce() -> f64) -> f64 {
    if rounds_to_hi(hi, lo, 1.0 + r * TWO_54) {
        return hi;
    }
    ball_or(hi, lo, r * hi.abs(), accurate)
}

/// [`round_ball`], else `accurate()`, for an error stated absolutely (`err` at least twice
/// the bound).
#[inline]
pub(super) fn round_ball_or(hi: f64, lo: f64, err: f64, accurate: impl FnOnce() -> f64) -> f64 {
    match round_ball(hi, lo, err) {
        Some(v) => v,
        None => cold(accurate),
    }
}

#[cold]
#[inline(never)]
fn ball_or(hi: f64, lo: f64, err: f64, accurate: impl FnOnce() -> f64) -> f64 {
    round_ball(hi, lo, err).unwrap_or_else(accurate)
}

#[cold]
#[inline(never)]
fn cold(accurate: impl FnOnce() -> f64) -> f64 {
    accurate()
}

/// [`round_or`] for `(hi + lo) 2^k`, any `k`, also when the result is subnormal or
/// overflows (the test is on the unscaled pair; [`round_ball_scaled`] then rounds the
/// ends as scaled reals).
#[inline]
pub(super) fn round_scaled_or(
    hi: f64,
    lo: f64,
    r: f64,
    k: i32,
    accurate: impl FnOnce() -> f64,
) -> f64 {
    if rounds_to_hi(hi, lo, 1.0 + r * TWO_54)
        && let Some(v) = scale_normal(hi, k)
    {
        return v;
    }
    round_ball_scaled(hi, lo, r * hi.abs(), k).unwrap_or_else(accurate)
}

const TWO_54: f64 = 18_014_398_509_481_984.0;

/// `hi` rounded with `lo`, if every real within `err` of `hi + lo` rounds to the same
/// double; `None` when one might not. For a normalised pair with `hi` finite and `err`
/// well below `ulp(hi)`.
///
/// Rounding is monotone, so it suffices that the two ends round alike. The ends are
/// computed: `lo -/+ err` is rounded once (by at most `2^-53 (|lo| + err)`, below `2^-104
/// |hi|`), and each caller's `err` exceeds its proven bound by more than that (each
/// constant is at least twice its bound), so the computed ends lie outside the exact
/// ones; `hi + (lo -/+ err)` then rounds them as the reals they are.
#[inline]
pub(super) fn round_ball(hi: f64, lo: f64, err: f64) -> Option<f64> {
    let a = hi + (lo - err);
    let b = hi + (lo + err);
    (a == b).then_some(a)
}

/// [`round_ball`] for `(hi + lo) 2^k`, also when the result is subnormal or overflows
/// (`err` for the unscaled pair, any `k`): then each end is rounded by [`round_scaled`],
/// the one rounding of the scaled real.
#[cold]
#[inline(never)]
pub(super) fn round_ball_scaled(hi: f64, lo: f64, err: f64, k: i32) -> Option<f64> {
    let (l0, l1) = (lo - err, lo + err);
    let (a, b) = (hi + l0, hi + l1);
    if a == b
        && let Some(r) = scale_normal(a, k)
    {
        return Some(r);
    }
    let (ah, al) = fast_two_sum(hi, l0);
    let (bh, bl) = fast_two_sum(hi, l1);
    let (ra, rb) = (round_scaled(ah, al, k), round_scaled(bh, bl, k));
    (ra.to_bits() == rb.to_bits()).then_some(ra)
}

/// A table entry, with a zero fallback that a masked index never reaches (library code
/// may not index unchecked).
#[inline]
pub(super) fn at<T: Copy + Default, const N: usize>(table: &[T; N], i: usize) -> T {
    table.get(i).copied().unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rounding_and_powers() {
        assert_eq!(round_int(2.5), 2.0);
        assert_eq!(round_int(3.5), 4.0);
        assert_eq!(round_int(-2.5), -2.0);
        assert_eq!(pow2(-1022), f64::MIN_POSITIVE);
        assert_eq!(pow2(0), 1.0);
        assert_eq!(exponent(1.0), 0);
        assert_eq!(exponent(f64::from_bits(1)), -1074);
        assert_eq!(exponent(f64::MIN_POSITIVE), -1022);
        assert_eq!(exponent(-3.0), 1);
        // The significand from the bits, at both ends of the exponent range.
        assert_eq!(mantissa(f64::MAX), 2.0 - f64::EPSILON);
        assert_eq!(mantissa(-f64::MAX), -(2.0 - f64::EPSILON));
        assert_eq!(mantissa(f64::from_bits(1)), 1.0);
        assert_eq!(mantissa(f64::from_bits(3)), 1.5);
        assert_eq!(mantissa(f64::MIN_POSITIVE), 1.0);
        assert_eq!(mantissa(-0.75), -1.5);
    }

    #[test]
    fn scaling_rounds_once() {
        let tiny = f64::from_bits(1);
        assert_eq!(scale(1.0, -1074), tiny);
        assert_eq!(scale(1.5, -1075), tiny);
        assert_eq!(scale(f64::from_bits(3), 1074), 3.0);
        assert_eq!(scale(f64::MAX, -2099), 0.0);
        assert_eq!(scale(1.0, -1076), 0.0);
        assert_eq!(scale(1.0, 1024), f64::INFINITY);
        assert_eq!(scale(0.75, 1024), 0.75 * pow2(1023) * 2.0);
        assert_eq!(scale(3.0, -2000), 0.0);
        // The largest exponent (2^-1023 is no normal double) scaled into the subnormals:
        // (2 - 2^-52) 2^-1023 is half-way below 2^-1022 and ties to it (even); the others
        // round to the nearest grid point.
        assert_eq!(scale(f64::MAX, -2046), f64::MIN_POSITIVE);
        assert_eq!(scale(f64::MAX, -2050), f64::from_bits(1 << 48));
        assert_eq!(scale(f64::MAX, -2097), 2.0 * tiny);
        assert_eq!(scale(-f64::MAX, -2098), -tiny);
        assert_eq!(scale(f64::MAX, -2099), 0.0);
        assert_eq!(scale(f64::MAX, -1000), f64::MAX * pow2(-1000));
        // Non-finite values pass through.
        assert!(scale(f64::NAN, -2000).is_nan());
        assert_eq!(scale(f64::INFINITY, -2000), f64::INFINITY);
        assert_eq!(scale(f64::NEG_INFINITY, 2000), f64::NEG_INFINITY);
        // A half-way hi that lo pushes up, and one it pushes down.
        assert_eq!(round_scaled(1.0, 1e-20, -1075), tiny);
        assert_eq!(round_scaled(1.0, -1e-20, -1075), 0.0);
        assert_eq!(round_scaled(1.0, 0.0, -1075), 0.0);
        assert_eq!(round_scaled(1.5, 0.0, -1074), 2.0 * tiny);
        assert_eq!(round_scaled(1.5, -1e-20, -1074), tiny);
        assert_eq!(round_scaled(-1.5, 1e-20, -1074), -tiny);
        assert_eq!(round_scaled(1.25, 1e-20, -1073), 3.0 * tiny);
        assert_eq!(round_scaled(1.0, 0.0, 1024), f64::INFINITY);
        assert_eq!(round_scaled(1.5, 0.0, 1023), 1.5 * pow2(1023));
        assert_eq!(round_scaled(1.0, 0.0, -1022), f64::MIN_POSITIVE);
    }
}
