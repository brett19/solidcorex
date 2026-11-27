// Written for solidcorex. Apache-2.0.
//! Binary floating point with up to 512 bits of significand, for the accurate phase of
//! Ziv's strategy: what every `fmath` function falls back to when the rounding test of
//! its double-double result fails (`docs/research/70-fmath-correct-rounding.md`).
//!
//! A [`Big`] is `±0.m 2^exp` with `m` a significand of `p` 64-bit limbs, most significant
//! first, its top bit set (`m` in `[1/2, 1)`), or zero. Every operation truncates its
//! exact result toward zero to `p` limbs, so its relative error is below `2^(1 - 64p)`,
//! and is exact when the exact result fits (sums and products of a few doubles, which the
//! exact fallbacks of `hypot`, `cbrt` and `powf` rely on). [`Big::to_f64`] rounds
//! correctly (to nearest, ties to even, subnormals and overflow included), and
//! [`round_ball`] rounds only when every value within a stated error does so the same way.
//!
//! Integer arithmetic only, so the bits do not depend on the platform; the exponent is an
//! `i64`, so nothing overflows or underflows on the way (`sinh 710` squares `e^710`).

use core::cmp::Ordering;

use super::dd;

/// The most limbs a [`Big`] carries.
pub(super) const LIMBS: usize = 8;

/// The working buffer of a sum: one limb of carry, the larger operand, and room for the
/// smaller one shifted by up to `64 (BUF - 1 - LIMBS)` bits before its tail is dropped.
const BUF: usize = 2 * LIMBS + 2;

/// The precisions of the accurate phase: 128 bits, which decides all but the inputs within
/// about `2^-80` of a midpoint, then 512, which decides every input known.
pub(super) const LADDER: [usize; 2] = [2, 8];

/// `±0.m 2^exp` (see the module notes).
#[derive(Clone, Copy, Debug)]
pub(super) struct Big {
    neg: bool,
    /// The value lies in `[2^(exp - 1), 2^exp)` in magnitude.
    exp: i64,
    /// The significand, most significant limb first; the limbs from `p` on are zero.
    m: [u64; LIMBS],
    /// Limbs in use, `1..=LIMBS`.
    p: usize,
}

/// Limb `i` of `s`, zero past its end.
#[inline]
fn limb(s: &[u64], i: usize) -> u64 {
    s.get(i).copied().unwrap_or(0)
}

impl Big {
    /// Zero at precision `p`.
    pub(super) fn zero(p: usize) -> Self {
        Self {
            neg: false,
            exp: 0,
            m: [0; LIMBS],
            p: p.clamp(1, LIMBS),
        }
    }

    /// `±0.buf 2^exp` for limbs `buf` (most significant first, any length, leading zeros
    /// allowed), truncated to `p` limbs.
    pub(super) fn from_parts(neg: bool, exp: i64, buf: &[u64], p: usize) -> Self {
        let z = buf.iter().take_while(|&&w| w == 0).count();
        let rest = buf.get(z..).unwrap_or(&[]);
        let Some(&first) = rest.first() else {
            return Self::zero(p);
        };
        let lz = first.leading_zeros();
        let mut out = Self::zero(p);
        for (i, slot) in out.m.iter_mut().take(out.p).enumerate() {
            let hi = limb(rest, i);
            *slot = if lz == 0 {
                hi
            } else {
                (hi << lz) | (limb(rest, i + 1) >> (64 - lz))
            };
        }
        out.neg = neg;
        out.exp = exp - 64 * z as i64 - i64::from(lz);
        out
    }

    /// `x` exactly, for a finite `x`.
    pub(super) fn from_f64(x: f64, p: usize) -> Self {
        if x == 0.0 {
            return Self::zero(p);
        }
        let b = x.to_bits();
        let field = ((b >> 52) & 0x7ff) as i64;
        let frac = b & 0x000f_ffff_ffff_ffff;
        // x = mant 2^e with mant an integer below 2^53.
        let (mant, e) = if field == 0 {
            (frac, -1074)
        } else {
            (frac | (1 << 52), field - 1075)
        };
        Self::from_parts(x < 0.0, e + 64, &[mant], p)
    }

    /// `k`.
    pub(super) fn from_u64(k: u64, p: usize) -> Self {
        Self::from_parts(false, 64, &[k], p)
    }

    /// `k`.
    pub(super) fn from_i64(k: i64, p: usize) -> Self {
        Self::from_u64(k.unsigned_abs(), p).with_sign(k < 0)
    }

    /// A constant from the tables (`0.m 2^e` to 640 bits), truncated to `p` limbs.
    pub(super) fn constant(c: &(i64, [u64; 10]), p: usize) -> Self {
        Self::from_parts(false, c.0, &c.1, p)
    }

    /// Whether the value is zero.
    pub(super) fn is_zero(&self) -> bool {
        self.m.first().is_none_or(|&w| w == 0)
    }

    /// Whether the value is below zero.
    pub(super) fn is_neg(&self) -> bool {
        self.neg && !self.is_zero()
    }

    /// The value lies in `[2^(e - 1), 2^e)` in magnitude, for `e` this (zero: 0).
    pub(super) fn exponent(&self) -> i64 {
        self.exp
    }

    /// `-self`.
    #[must_use]
    pub(super) fn neg(mut self) -> Self {
        self.neg = !self.neg;
        self
    }

    /// `|self|`, negated when `neg`.
    #[must_use]
    pub(super) fn with_sign(mut self, neg: bool) -> Self {
        self.neg = neg;
        self
    }

    /// `self 2^k`, exactly.
    #[must_use]
    pub(super) fn mul_pow2(mut self, k: i64) -> Self {
        if !self.is_zero() {
            self.exp += k;
        }
        self
    }

    /// `|self|` against `|o|`.
    pub(super) fn cmp_mag(&self, o: &Self) -> Ordering {
        match (self.is_zero(), o.is_zero()) {
            (true, true) => Ordering::Equal,
            (true, false) => Ordering::Less,
            (false, true) => Ordering::Greater,
            (false, false) => self.exp.cmp(&o.exp).then(self.m.cmp(&o.m)),
        }
    }

    /// `self + o`.
    #[must_use]
    pub(super) fn add(self, o: Self) -> Self {
        if o.is_zero() {
            return self;
        }
        if self.is_zero() {
            return o;
        }
        let (a, b) = if self.cmp_mag(&o) == Ordering::Less {
            (o, self)
        } else {
            (self, o)
        };
        let sub = a.neg != b.neg;
        // 0.x 2^(a.exp + 64): a's limbs after one limb of carry, b's shifted right by d.
        let mut x = [0u64; BUF];
        for (s, &w) in x.iter_mut().skip(1).zip(a.m.iter().take(a.p)) {
            *s = w;
        }
        let d = a.exp - b.exp;
        if d >= 64 * (BUF as i64 - 1) {
            // b is below the buffer's last bit: below 2^-1088 of a.
            return a;
        }
        let (q, r) = ((d / 64) as usize, (d % 64) as u32);
        let mut y = [0u64; BUF];
        for (j, &w) in b.m.iter().take(b.p).enumerate() {
            if let Some(s) = y.get_mut(1 + q + j) {
                *s |= w >> r;
            }
            if r > 0
                && let Some(s) = y.get_mut(2 + q + j)
            {
                *s |= w << (64 - r);
            }
        }
        let mut carry = false;
        for (xi, &yi) in x.iter_mut().zip(y.iter()).rev() {
            if sub {
                let (v, b1) = xi.overflowing_sub(yi);
                let (v, b2) = v.overflowing_sub(u64::from(carry));
                *xi = v;
                carry = b1 || b2;
            } else {
                let (v, c1) = xi.overflowing_add(yi);
                let (v, c2) = v.overflowing_add(u64::from(carry));
                *xi = v;
                carry = c1 || c2;
            }
        }
        Self::from_parts(a.neg, a.exp + 64, &x, a.p)
    }

    /// `self - o`.
    #[must_use]
    pub(super) fn sub(self, o: Self) -> Self {
        self.add(o.neg())
    }

    /// `self o`.
    #[must_use]
    pub(super) fn mul(self, o: Self) -> Self {
        if self.is_zero() || o.is_zero() {
            return Self::zero(self.p);
        }
        let p = self.p;
        // Little-endian schoolbook product of the two significands, 2p limbs.
        let mut r = [0u64; 2 * LIMBS];
        for i in 0..p {
            let ai = u128::from(limb(&self.m, p - 1 - i));
            let mut carry = 0u128;
            for j in 0..p {
                let bj = u128::from(limb(&o.m, p - 1 - j));
                let t = ai * bj + u128::from(limb(&r, i + j)) + carry;
                if let Some(s) = r.get_mut(i + j) {
                    *s = t as u64;
                }
                carry = t >> 64;
            }
            if let Some(s) = r.get_mut(i + p) {
                *s = carry as u64;
            }
        }
        let mut be = [0u64; 2 * LIMBS];
        for (k, s) in be.iter_mut().take(2 * p).enumerate() {
            *s = limb(&r, 2 * p - 1 - k);
        }
        Self::from_parts(
            self.neg != o.neg,
            self.exp + o.exp,
            be.get(..2 * p).unwrap_or(&[]),
            p,
        )
    }

    /// `self k`.
    #[must_use]
    pub(super) fn mul_small(self, k: u64) -> Self {
        let p = self.p;
        let mut out = [0u64; LIMBS + 1];
        let mut carry = 0u128;
        for i in (0..p).rev() {
            let t = u128::from(limb(&self.m, i)) * u128::from(k) + carry;
            if let Some(s) = out.get_mut(i + 1) {
                *s = t as u64;
            }
            carry = t >> 64;
        }
        if let Some(s) = out.first_mut() {
            *s = carry as u64;
        }
        Self::from_parts(
            self.neg,
            self.exp + 64,
            out.get(..=p).unwrap_or(&[]),
            p,
        )
    }

    /// `self / k`, for `k >= 1`.
    #[must_use]
    pub(super) fn div_small(self, k: u64) -> Self {
        let p = self.p;
        let k = u128::from(k.max(1));
        let mut out = [0u64; LIMBS + 2];
        let mut rem = 0u128;
        for (i, s) in out.iter_mut().take(p + 2).enumerate() {
            let cur = (rem << 64) | u128::from(limb(&self.m, i));
            *s = (cur / k) as u64;
            rem = cur % k;
        }
        Self::from_parts(self.neg, self.exp, out.get(..p + 2).unwrap_or(&[]), p)
    }

    /// The value as `f 2^e` with `f` in `[1/2, 1)` (signed) good to 53 bits, for a
    /// starting guess.
    fn approx(&self) -> (f64, i64) {
        let f = (limb(&self.m, 0) >> 11) as f64 * dd::pow2(-53);
        (if self.neg { -f } else { f }, self.exp)
    }

    /// `1 / self`, for a nonzero value: Newton's iteration `y + y (1 - self y)` from a
    /// double, each step doubling the correct bits (53, 106, 212, 424, 848), to `2^(4-64p)`.
    #[must_use]
    pub(super) fn recip(self) -> Self {
        let p = self.p;
        let (f, e) = self.approx();
        let one = Self::from_u64(1, p);
        let mut y = Self::from_f64(1.0 / f, p).mul_pow2(-e);
        let mut good = 52;
        while good < 64 * p + 4 {
            y = y.add(y.mul(one.sub(self.mul(y))));
            good *= 2;
        }
        y
    }

    /// `self / o`, for a nonzero `o`.
    #[must_use]
    pub(super) fn div(self, o: Self) -> Self {
        self.mul(o.recip())
    }

    /// `sqrt(self)`, for a positive value: Newton's iteration for the inverse root,
    /// `y + y (1 - self y^2) / 2`, then `self y`.
    #[must_use]
    pub(super) fn sqrt(self) -> Self {
        let p = self.p;
        if self.is_zero() {
            return self;
        }
        let (f, e) = self.approx();
        let f = f.abs();
        // self = g 2^h with h even.
        let (g, h) = if e % 2 == 0 { (f, e) } else { (2.0 * f, e - 1) };
        let one = Self::from_u64(1, p);
        let mut y = Self::from_f64(1.0 / g.sqrt(), p).mul_pow2(-h / 2);
        let mut good = 50;
        while good < 64 * p + 4 {
            let r = one.sub(self.mul(y).mul(y));
            y = y.add(y.mul(r).mul_pow2(-1));
            good *= 2;
        }
        self.mul(y)
    }

    /// The double nearest the value (ties to even), with subnormals and overflow to
    /// infinity as IEEE 754 rounds them.
    pub(super) fn to_f64(&self) -> f64 {
        if self.is_zero() {
            return if self.neg { -0.0 } else { 0.0 };
        }
        let sign = |r: f64| if self.neg { -r } else { r };
        // Bits kept: 53 for a normal result, down to the weight 2^-1074 for a subnormal.
        let q = (self.exp + 1074).min(53);
        if q < 0 {
            // Below 2^-1075: rounds to zero.
            return sign(0.0);
        }
        let q = q as u32;
        let m0 = limb(&self.m, 0);
        let mut k = if q == 0 { 0 } else { m0 >> (64 - q) };
        let round = (m0 >> (63 - q)) & 1 == 1;
        let below = m0 & ((1u64 << (63 - q)) - 1);
        let sticky = below != 0 || self.m.iter().skip(1).any(|&w| w != 0);
        if round && (sticky || k & 1 == 1) {
            k += 1;
        }
        // k 2^(exp - q), exact when representable (k <= 2^53), infinite past the top.
        let shift = (self.exp - i64::from(q)).clamp(-3000, 3000) as i32;
        sign(dd::scale(k as f64, shift))
    }
}

/// The rounded value of `v`, if every real within `2^(c - 1 - 64p) |v|` of it rounds to
/// the same double (`c >= 2`); `None` when some do not, and a more precise `v` is needed.
///
/// The ends `v ∓ 2^(c - 64p) |v|` are summed with truncation, which moves each by below
/// one unit of `2^(1 - 64p) |v|`, so the interval they bound holds every real within
/// `2^(c - 1 - 64p) |v|` of `v`; rounding is monotone, so when both ends round to the same
/// double so does all of it.
pub(super) fn round_ball(v: &Big, c: i64) -> Option<f64> {
    if v.is_zero() {
        return Some(v.to_f64());
    }
    let r = v.with_sign(false).mul_pow2(c - 64 * v.p as i64);
    let lo = v.sub(r).to_f64();
    let hi = v.add(r).to_f64();
    (lo.to_bits() == hi.to_bits()).then_some(lo)
}

/// Ziv's accurate phase: `f(p)` at each precision of [`LADDER`] until [`round_ball`]
/// decides it with error constant `c`, else the last one rounded (see the research note:
/// no input known needs it).
pub(super) fn ziv(c: i64, f: impl Fn(usize) -> Big) -> f64 {
    let mut last = Big::zero(1);
    for p in LADDER {
        last = f(p);
        if let Some(r) = round_ball(&last, c) {
            return r;
        }
    }
    last.to_f64()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn b(x: f64) -> Big {
        Big::from_f64(x, 3)
    }

    #[test]
    fn exact_round_trip_and_ops() {
        for x in [
            1.0,
            -1.5,
            f64::MAX,
            f64::MIN_POSITIVE,
            f64::from_bits(1),
            f64::from_bits(0x000f_ffff_ffff_ffff),
            3.0e-310,
            0.1,
            -7.25e200,
        ] {
            assert_eq!(b(x).to_f64().to_bits(), x.to_bits(), "{x:e}");
        }
        assert_eq!(b(0.1).add(b(0.2)).to_f64(), 0.1 + 0.2);
        assert_eq!(b(0.1).mul(b(0.2)).to_f64(), 0.1 * 0.2);
        assert_eq!(b(1.0).sub(b(1.0)).is_zero(), true);
        assert_eq!(b(3.0).sub(b(5.0)).to_f64(), -2.0);
        assert_eq!(b(1.0).div(b(3.0)).to_f64(), 1.0 / 3.0);
        assert_eq!(b(2.0).sqrt().to_f64(), 2.0f64.sqrt());
        assert_eq!(b(7.0).div_small(3).to_f64(), 7.0 / 3.0);
        assert_eq!(b(7.0).mul_small(3).to_f64(), 21.0);
        assert_eq!(b(1e300).mul(b(1e300)).to_f64(), f64::INFINITY);
        assert_eq!(b(1e-300).mul(b(1e-300)).to_f64(), 0.0);
        // Exact: (2^53 + 1)^2 needs 107 bits.
        let t = 9_007_199_254_740_993u64;
        let sq = Big::from_u64(t, 3).mul(Big::from_u64(t, 3));
        let back = sq.sub(Big::from_u64(t, 3).mul_small(t));
        assert!(back.is_zero());
    }

    #[test]
    fn rounding_ties_and_subnormals() {
        // 1 + 2^-53 is a tie: to even (1); a hair more rounds up.
        let one = b(1.0);
        let tie = one.add(b(1.0).mul_pow2(-53));
        assert_eq!(tie.to_f64(), 1.0);
        let up = tie.add(b(1.0).mul_pow2(-150));
        assert_eq!(up.to_f64(), 1.0 + f64::EPSILON);
        // 1.5 2^-1074 ties to 2^-1073; 2^-1075 ties to 0; a hair more is 2^-1074.
        assert_eq!(b(1.5).mul_pow2(-1074).to_f64(), f64::from_bits(2));
        assert_eq!(b(1.0).mul_pow2(-1075).to_f64(), 0.0);
        assert_eq!(
            b(1.0).mul_pow2(-1075).add(b(1.0).mul_pow2(-1200)).to_f64(),
            f64::from_bits(1)
        );
        // MAX + half an ulp ties to even: infinity.
        let half_ulp = b(1.0).mul_pow2(970);
        assert_eq!(b(f64::MAX).add(half_ulp).to_f64(), f64::INFINITY);
        assert_eq!(
            b(f64::MAX).add(half_ulp).sub(b(1.0)).to_f64(),
            f64::MAX
        );
        assert_eq!(round_ball(&tie, 8), None);
        assert_eq!(round_ball(&up, 8), Some(1.0 + f64::EPSILON));
    }
}
