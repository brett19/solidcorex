// Written for solidcorex. Apache-2.0.
//! Exact integers: sign and magnitude in 64-bit limbs. Every finite double is an integer
//! times a power of two, so a computation on doubles brought to one power of two is exact
//! here. [`crate::rat::Rat`] builds the exact rationals on these.
//!
//! Plain schoolbook arithmetic: the references work on numbers of a few hundred to a few
//! thousand bits, where nothing cleverer pays for itself.

use core::cmp::Ordering;

/// An integer: `neg` and the magnitude's limbs, least significant first, with no high zero
/// limb (zero is no limbs and not negative), so equal values are equal structs.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct Int {
    neg: bool,
    mag: Vec<u64>,
}

impl From<u64> for Int {
    fn from(m: u64) -> Int {
        Int::new(false, vec![m])
    }
}

impl From<i64> for Int {
    fn from(m: i64) -> Int {
        Int::new(m < 0, vec![m.unsigned_abs()])
    }
}

impl Int {
    fn new(neg: bool, mut mag: Vec<u64>) -> Int {
        while mag.last() == Some(&0) {
            mag.pop();
        }
        let neg = neg && !mag.is_empty();
        Int { neg, mag }
    }

    /// The finite doubles `xs` as integers in units of `2^e`, with `e` the lowest exponent
    /// any of them needs.
    pub fn of_doubles<const N: usize>(xs: [f64; N]) -> ([Int; N], i64) {
        let parts = xs.map(split);
        let e = parts
            .iter()
            .filter(|p| p.1 != 0)
            .map(|p| p.2)
            .min()
            .unwrap_or(0);
        (
            parts.map(|(neg, m, q)| Int::new(neg, vec![m]).shl((q - e).max(0) as u64)),
            e,
        )
    }

    /// Whether this is zero.
    pub fn is_zero(&self) -> bool {
        self.mag.is_empty()
    }

    /// Whether this is below zero.
    pub fn is_negative(&self) -> bool {
        self.neg
    }

    /// -1, 0 or 1.
    pub fn signum(&self) -> i32 {
        if self.is_zero() {
            0
        } else if self.neg {
            -1
        } else {
            1
        }
    }

    /// `|self|`.
    #[must_use]
    pub fn abs(&self) -> Int {
        Int::new(false, self.mag.clone())
    }

    /// Whether this is 1.
    pub fn is_one(&self) -> bool {
        !self.neg && self.mag == [1]
    }

    /// The low 64 bits of the magnitude.
    pub fn low_u64(&self) -> u64 {
        self.mag.first().copied().unwrap_or(0)
    }

    /// The magnitude's bit length (0 for zero).
    pub fn bit_len(&self) -> u64 {
        match self.mag.last() {
            None => 0,
            Some(&top) => 64 * (self.mag.len() as u64) - u64::from(top.leading_zeros()),
        }
    }

    /// The number of trailing zero bits, `None` for zero.
    pub fn trailing_zeros(&self) -> Option<u64> {
        let (i, w) = self.mag.iter().enumerate().find(|(_, w)| **w != 0)?;
        Some(64 * i as u64 + u64::from(w.trailing_zeros()))
    }

    /// Whether the magnitude is a power of two (one is `2^0`).
    pub fn mag_is_power_of_two(&self) -> bool {
        match self.mag.split_last() {
            None => false,
            Some((top, rest)) => top.is_power_of_two() && rest.iter().all(|w| *w == 0),
        }
    }

    /// `self * 2^n`.
    #[must_use]
    pub fn shl(&self, n: u64) -> Int {
        let (limbs, bits) = ((n / 64) as usize, (n % 64) as u32);
        let mut mag = vec![0; limbs];
        let mut carry = 0;
        for &w in &self.mag {
            mag.push((w << bits) | carry);
            carry = if bits == 0 { 0 } else { w >> (64 - bits) };
        }
        mag.push(carry);
        Int::new(self.neg, mag)
    }

    /// `self / 2^n`, rounded toward zero (exact when the low `n` bits are zero).
    #[must_use]
    pub fn shr(&self, n: u64) -> Int {
        let (limbs, bits) = ((n / 64) as usize, (n % 64) as u32);
        let src = self.mag.get(limbs..).unwrap_or(&[]);
        let mag = if bits == 0 {
            src.to_vec()
        } else {
            let above = src.iter().skip(1).map(Some).chain(core::iter::once(None));
            src.iter()
                .zip(above)
                .map(|(&w, hi)| (w >> bits) | hi.map_or(0, |&h| h << (64 - bits)))
                .collect()
        };
        Int::new(self.neg, mag)
    }

    fn cmp_mag(a: &[u64], b: &[u64]) -> Ordering {
        a.len()
            .cmp(&b.len())
            .then_with(|| a.iter().rev().cmp(b.iter().rev()))
    }

    /// `floor(sqrt(self))` for `self >= 0`; `None` for a negative value.
    pub fn isqrt(&self) -> Option<Int> {
        if self.neg {
            return None;
        }
        if self.is_zero() {
            return Some(Int::default());
        }
        // Newton's method from above: x = 2^ceil(len / 2) >= sqrt(self), and the iterates
        // fall to the floor and stop there.
        let mut x = Int::from(1u64).shl(self.bit_len().div_ceil(2));
        loop {
            let (q, _) = self.div_rem(&x)?;
            let y = x.add(&q).shr(1);
            if y.cmp_abs(&x) != Ordering::Less {
                return Some(x);
            }
            x = y;
        }
    }

    /// `|self|` against `|o|`.
    pub fn cmp_abs(&self, o: &Int) -> Ordering {
        Int::cmp_mag(&self.mag, &o.mag)
    }

    fn add_mag(a: &[u64], b: &[u64]) -> Vec<u64> {
        let (a, b) = if a.len() >= b.len() { (a, b) } else { (b, a) };
        let mut out = Vec::with_capacity(a.len() + 1);
        let mut carry = 0u64;
        for (i, &x) in a.iter().enumerate() {
            let (s1, c1) = x.overflowing_add(b.get(i).copied().unwrap_or(0));
            let (s2, c2) = s1.overflowing_add(carry);
            out.push(s2);
            carry = u64::from(c1) + u64::from(c2);
        }
        out.push(carry);
        out
    }

    /// `a - b` for `|a| >= |b|`.
    fn sub_mag(a: &[u64], b: &[u64]) -> Vec<u64> {
        let mut out = a.to_vec();
        sub_in_place(&mut out, b);
        out
    }

    /// `self + o`.
    #[must_use]
    pub fn add(&self, o: &Int) -> Int {
        if self.neg == o.neg {
            return Int::new(self.neg, Int::add_mag(&self.mag, &o.mag));
        }
        match Int::cmp_mag(&self.mag, &o.mag) {
            Ordering::Less => Int::new(o.neg, Int::sub_mag(&o.mag, &self.mag)),
            _ => Int::new(self.neg, Int::sub_mag(&self.mag, &o.mag)),
        }
    }

    /// `-self`.
    #[must_use]
    pub fn neg(&self) -> Int {
        Int::new(!self.neg, self.mag.clone())
    }

    /// `self - o`.
    #[must_use]
    pub fn sub(&self, o: &Int) -> Int {
        self.add(&o.neg())
    }

    /// `self * o`.
    #[must_use]
    pub fn mul(&self, o: &Int) -> Int {
        let mut out = vec![0u64; self.mag.len() + o.mag.len() + 1];
        for (i, &x) in self.mag.iter().enumerate() {
            let mut carry = 0u128;
            for (slot, &y) in out.iter_mut().skip(i).zip(&o.mag) {
                let t = u128::from(x) * u128::from(y) + u128::from(*slot) + carry;
                *slot = t as u64;
                carry = t >> 64;
            }
            for slot in out.iter_mut().skip(i + o.mag.len()) {
                if carry == 0 {
                    break;
                }
                let t = u128::from(*slot) + carry;
                *slot = t as u64;
                carry = t >> 64;
            }
        }
        Int::new(self.neg != o.neg, out)
    }

    /// `(q, r)` with `self = q d + r`, `q` rounded toward zero and `r` of `self`'s sign;
    /// `None` when `d` is zero.
    pub fn div_rem(&self, d: &Int) -> Option<(Int, Int)> {
        let (q, r) = div_rem_mag(&self.mag, &d.mag)?;
        Some((Int::new(self.neg != d.neg, q), Int::new(self.neg, r)))
    }

    /// The greatest common divisor of the magnitudes (`|o|` when `self` is zero).
    #[must_use]
    pub fn gcd(&self, o: &Int) -> Int {
        if self.is_zero() || o.is_zero() {
            return if self.is_zero() { o.abs() } else { self.abs() };
        }
        // A unit divides everything.
        if self.mag == [1] || o.mag == [1] {
            return Int::from(1u64);
        }
        let (a, b) = (self.mag.clone(), o.mag.clone());
        Int::new(false, gcd_mag(a, b))
    }

    /// `self / d` for a `d` known to divide `self` (the quotient rounded toward zero
    /// otherwise); zero when `d` is zero.
    #[must_use]
    pub fn div_exact(&self, d: &Int) -> Int {
        self.div_rem(d).map_or_default(|(q, _)| q)
    }

    /// `|self| * 2^se` against `|o| * 2^oe`.
    pub fn cmp_scaled(&self, se: i64, o: &Int, oe: i64) -> Ordering {
        let (a, b) = if se >= oe {
            (self.shl((se - oe) as u64), o.clone())
        } else {
            (self.clone(), o.shl((oe - se) as u64))
        };
        Int::cmp_mag(&a.mag, &b.mag)
    }

    /// `(m, k)` with the value within `2^-52` relative of `m * 2^k`, `m` a double of
    /// magnitude in `[2^63, 2^64]`; `(0, 0)` for zero.
    pub fn approx(&self) -> (f64, i64) {
        let n = self.mag.len();
        let Some(&top) = self.mag.last() else {
            return (0.0, 0);
        };
        let next = self.mag.iter().rev().nth(1).copied().unwrap_or(0);
        let z = top.leading_zeros();
        let word = if z == 0 {
            top
        } else {
            (top << z) | (next >> (64 - z))
        };
        let m = word as f64;
        (
            if self.neg { -m } else { m },
            64 * (n as i64 - 1) - i64::from(z),
        )
    }
}

/// `(negative, m, q)` with the finite `x = ±m 2^q`, `m` below `2^53`.
pub(crate) fn split(x: f64) -> (bool, u64, i64) {
    let b = x.to_bits();
    let field = ((b >> 52) & 0x7ff) as i64;
    let frac = b & ((1 << 52) - 1);
    let (m, q) = if field == 0 {
        (frac, -1074)
    } else {
        (frac | (1 << 52), field - 1075)
    };
    (x < 0.0, m, q)
}

fn gcd_u64(mut a: u64, mut b: u64) -> u64 {
    while b != 0 {
        (a, b) = (b, a % b);
    }
    a
}

/// `x >> k`, low 64 bits.
fn shr_low64(x: &[u64], k: u64) -> u64 {
    let (i, r) = ((k / 64) as usize, (k % 64) as u32);
    let lo = x.get(i).copied().unwrap_or(0);
    let hi = x.get(i + 1).copied().unwrap_or(0);
    ((u128::from(hi) << 64 | u128::from(lo)) >> r) as u64
}

/// `cx * x + cy * y` for cofactors under 2^62 in magnitude, where the result is known not to
/// be negative.
fn lincomb(x: &[u64], cx: i128, y: &[u64], cy: i128) -> Vec<u64> {
    let n = x.len().max(y.len());
    let mut out = Vec::with_capacity(n);
    let mut carry = 0i128;
    for i in 0..n {
        let xi = i128::from(x.get(i).copied().unwrap_or(0));
        let yi = i128::from(y.get(i).copied().unwrap_or(0));
        let t = cx * xi + cy * yi + carry;
        out.push(t as u64);
        carry = t >> 64;
    }
    while out.last() == Some(&0) {
        out.pop();
    }
    out
}

/// The gcd of two magnitudes, neither zero: Lehmer's algorithm (TAOCP 4.5.2 L). The leading
/// 62 bits of the pair decide a run of quotients at once, and a pass over the limbs applies
/// them, instead of one pass per quotient.
fn gcd_mag(mut a: Vec<u64>, mut b: Vec<u64>) -> Vec<u64> {
    if Int::cmp_mag(&a, &b) == Ordering::Less {
        core::mem::swap(&mut a, &mut b);
    }
    while b.len() > 1 {
        let bits =
            64 * (a.len() as u64 - 1) + u64::from(64 - a.last().map_or(0, |w| w.leading_zeros()));
        let k = bits.saturating_sub(62);
        let (mut ah, mut bh) = (i128::from(shr_low64(&a, k)), i128::from(shr_low64(&b, k)));
        let (mut ca, mut cb, mut cc, mut cd) = (1i128, 0i128, 0i128, 1i128);
        loop {
            if bh + cc == 0 || bh + cd == 0 {
                break;
            }
            let q = (ah + ca) / (bh + cc);
            if q != (ah + cb) / (bh + cd) {
                break;
            }
            (ca, cc) = (cc, ca - q * cc);
            (cb, cd) = (cd, cb - q * cd);
            (ah, bh) = (bh, ah - q * bh);
        }
        if cb == 0 {
            // No quotient was certain from the leading bits: one full division.
            let Some((_, r)) = div_rem_mag(&a, &b) else {
                return a;
            };
            a = core::mem::replace(&mut b, r);
        } else {
            let na = lincomb(&a, ca, &b, cb);
            let nb = lincomb(&a, cc, &b, cd);
            (a, b) = (na, nb);
        }
        if b.is_empty() {
            return a;
        }
    }
    // b is one limb: one more division brings a down to a limb, then plain Euclid.
    let y = b.first().copied().unwrap_or(0);
    let x = match a.as_slice() {
        [x] => *x,
        _ => div_rem_mag(&a, &b)
            .and_then(|(_, r)| r.first().copied())
            .unwrap_or(0),
    };
    vec![gcd_u64(y, x)]
}

/// `a -= b` for `a >= b`, high zero limbs dropped.
fn sub_in_place(a: &mut Vec<u64>, b: &[u64]) {
    let mut borrow = 0u64;
    let mut rest = b.iter();
    for x in a.iter_mut() {
        let y = rest.next().copied().unwrap_or(0);
        if y == 0 && borrow == 0 && rest.len() == 0 {
            break;
        }
        let (d1, b1) = x.overflowing_sub(y);
        let (d2, b2) = d1.overflowing_sub(borrow);
        *x = d2;
        borrow = u64::from(b1) + u64::from(b2);
    }
    while a.last() == Some(&0) {
        a.pop();
    }
}

/// Magnitudes `(n / d, n % d)`; `None` when `d` is zero. A one-limb divisor goes one limb at a
/// time; a longer one by Knuth's algorithm D (TAOCP 4.3.1), one quotient limb per step.
fn div_rem_mag(n: &[u64], d: &[u64]) -> Option<(Vec<u64>, Vec<u64>)> {
    match d {
        [] => None,
        [one] => {
            let one = u128::from(*one);
            let mut q = vec![0u64; n.len()];
            let mut r = 0u128;
            for (slot, &w) in q.iter_mut().zip(n).rev() {
                let cur = (r << 64) | u128::from(w);
                *slot = (cur / one) as u64;
                r = cur % one;
            }
            Some((q, vec![r as u64]))
        }
        _ if Int::cmp_mag(n, d) == Ordering::Less => Some((Vec::new(), n.to_vec())),
        _ => Some(knuth_d(n, d)),
    }
}

/// Algorithm D for `n >= d` and `d` of two or more limbs, no high zero limb in either.
#[allow(clippy::indexing_slicing)] // the indices are bounded by the lengths m and n.len()
fn knuth_d(n: &[u64], d: &[u64]) -> (Vec<u64>, Vec<u64>) {
    const B: u128 = 1 << 64;
    let m = d.len();
    // Normalise: shift both so the divisor's top bit is set.
    let s = d[m - 1].leading_zeros();
    let shl = |x: &[u64], extra: bool| -> Vec<u64> {
        let mut out = Vec::with_capacity(x.len() + 1);
        let mut carry = 0u64;
        for &w in x {
            out.push((w << s) | carry);
            carry = if s == 0 { 0 } else { w >> (64 - s) };
        }
        if extra {
            out.push(carry);
        }
        out
    };
    let v = shl(d, false);
    let mut u = shl(n, true);
    let mut q = vec![0u64; n.len() - m + 1];
    let (vt, vs) = (u128::from(v[m - 1]), u128::from(v[m - 2]));
    for j in (0..=n.len() - m).rev() {
        let top = (u128::from(u[j + m]) << 64) | u128::from(u[j + m - 1]);
        let mut qhat = top / vt;
        let mut rhat = top % vt;
        while qhat >= B || qhat * vs > ((rhat << 64) | u128::from(u[j + m - 2])) {
            qhat -= 1;
            rhat += vt;
            if rhat >= B {
                break;
            }
        }
        // u[j..=j+m] -= qhat * v
        let mut borrow = 0i128;
        let mut carry = 0u128;
        for i in 0..m {
            let p = qhat * u128::from(v[i]) + carry;
            carry = p >> 64;
            let t = i128::from(u[j + i]) - borrow - i128::from(p as u64);
            u[j + i] = t as u64;
            borrow = i128::from(t < 0);
        }
        let t = i128::from(u[j + m]) - borrow - carry as i128;
        u[j + m] = t as u64;
        if t < 0 {
            // qhat was one too large: add the divisor back.
            qhat -= 1;
            let mut c = 0u128;
            for i in 0..m {
                let sum = u128::from(u[j + i]) + u128::from(v[i]) + c;
                u[j + i] = sum as u64;
                c = sum >> 64;
            }
            u[j + m] = u[j + m].wrapping_add(c as u64);
        }
        q[j] = qhat as u64;
    }
    // The remainder is u[..m] shifted back down.
    let mut r = Vec::with_capacity(m);
    for i in 0..m {
        let hi = if s == 0 { 0 } else { u[i + 1] << (64 - s) };
        r.push((u[i] >> s) | hi);
    }
    while q.last() == Some(&0) {
        q.pop();
    }
    while r.last() == Some(&0) {
        r.pop();
    }
    (q, r)
}

/// `m * 2^k`, rounded (twice at most, in the subnormals): infinite past the doubles.
pub fn ldexp(m: f64, k: i64) -> f64 {
    let p2 = |k: i64| f64::from_bits(((1023 + k) as u64) << 52);
    let mut x = m;
    let mut k = k.clamp(-4000, 4000);
    while k > 1000 {
        x *= p2(1000);
        k -= 1000;
    }
    while k < -1000 {
        x *= p2(-1000);
        k += 1000;
    }
    x * p2(k)
}

/// `(a * 2^ka) / (b * 2^kb)` as a double, within a few ulps, for `b` not zero
/// ([`crate::rat::Rat::to_f64`] rounds correctly).
pub fn ratio(a: &Int, b: &Int, k: i64) -> f64 {
    let ((ma, ea), (mb, eb)) = (a.approx(), b.approx());
    ldexp(ma / mb, ea - eb + k)
}

/// `sqrt((a / b) * 2^k)` as a double, within a few ulps, for `a` not negative and `b`
/// positive.
pub fn sqrt_ratio(a: &Int, b: &Int, k: i64) -> f64 {
    let ((ma, ea), (mb, eb)) = (a.approx(), b.approx());
    let e = ea - eb + k;
    let (m, e) = if e % 2 == 0 {
        (ma / mb, e)
    } else {
        (2.0 * ma / mb, e - 1)
    };
    ldexp(m.sqrt(), e / 2)
}

#[cfg(test)]
mod tests {
    use super::*;
    use scx_num::hash::Rng;

    #[test]
    fn arithmetic_is_exact() {
        let ([a, b, one], e) = Int::of_doubles([1.0e20, -3.0, 1.0]);
        let p = a.mul(&a).add(&b.mul(&one)); // 1e40 - 3, in units of 2^(2e)
        assert_eq!(ratio(&p, &one.mul(&one), 0), 1.0e40);
        assert_eq!(p.sub(&a.mul(&a)).signum(), -1);
        assert_eq!(b.cmp_scaled(e, &one, e), Ordering::Greater);
        assert_eq!(one.cmp_scaled(e, &one, e + 1), Ordering::Less);
        assert!((sqrt_ratio(&a.mul(&a), &one.mul(&one), 0) / 1.0e20 - 1.0).abs() < 1e-15);
        assert_eq!(ldexp(1.0, -1074), f64::from_bits(1));
        assert_eq!(ldexp(1.0, 1024), f64::INFINITY);
    }

    fn random(rng: &mut Rng, limbs: usize) -> Int {
        let mag = (0..limbs).map(|_| rng.next_u64()).collect();
        Int::new(rng.next_u64() & 1 == 1, mag)
    }

    /// The bit-at-a-time division `knuth_d` replaced, as a reference.
    fn div_rem_bitwise(n: &[u64], d: &[u64]) -> (Vec<u64>, Vec<u64>) {
        let mut q = vec![0u64; n.len()];
        let mut r: Vec<u64> = Vec::with_capacity(d.len() + 1);
        for bit in (0..64 * n.len() as u64).rev() {
            let b = n
                .get((bit / 64) as usize)
                .map_or(0, |w| (w >> (bit % 64)) & 1);
            let mut carry = b;
            for w in &mut r {
                let top = *w >> 63;
                *w = (*w << 1) | carry;
                carry = top;
            }
            if carry != 0 {
                r.push(carry);
            }
            if Int::cmp_mag(&r, d) != Ordering::Less {
                sub_in_place(&mut r, d);
                if let Some(w) = q.get_mut((bit / 64) as usize) {
                    *w |= 1 << (bit % 64);
                }
            }
        }
        while q.last() == Some(&0) {
            q.pop();
        }
        (q, r)
    }

    /// Algorithm D against the bitwise division, on limbs that are random, all ones, zero or
    /// a power of two, which hit the quotient-digit correction and the add-back.
    #[test]
    fn long_division_matches_the_bitwise_one() {
        let mut rng = Rng::new(11);
        let limb = |rng: &mut Rng| match rng.next_u64() % 6 {
            0 => u64::MAX,
            1 => 0,
            2 => 1 << 63,
            3 => 1,
            _ => rng.next_u64(),
        };
        for i in 0..6000 {
            let nl = 2 + i % 7;
            let dl = 2 + (i / 7) % 4;
            let mut n: Vec<u64> = (0..nl).map(|_| limb(&mut rng)).collect();
            let mut d: Vec<u64> = (0..dl).map(|_| limb(&mut rng)).collect();
            while n.last() == Some(&0) {
                n.pop();
            }
            while d.last() == Some(&0) {
                d.pop();
            }
            if d.len() < 2 || Int::cmp_mag(&n, &d) == Ordering::Less {
                continue;
            }
            assert_eq!(knuth_d(&n, &d), div_rem_bitwise(&n, &d), "{n:x?} / {d:x?}");
        }
    }

    /// Lehmer's gcd against Euclid's with exact division, on limbs that are random, all ones,
    /// zero or a power of two, and on multiples of a common factor.
    #[test]
    fn lehmer_gcd_matches_euclid() {
        fn euclid(mut a: Vec<u64>, mut b: Vec<u64>) -> Vec<u64> {
            while !b.is_empty() {
                let (_, r) = div_rem_mag(&a, &b).unwrap();
                let mut r = r;
                while r.last() == Some(&0) {
                    r.pop();
                }
                (a, b) = (b, r);
            }
            a
        }
        let mut rng = Rng::new(23);
        let limb = |rng: &mut Rng| match rng.next_u64() % 7 {
            0 => u64::MAX,
            1 => 0,
            2 => 1 << 63,
            3 => 1,
            _ => rng.next_u64(),
        };
        for i in 0..4000 {
            let mk = |rng: &mut Rng, n: usize| {
                let mut v: Vec<u64> = (0..n).map(|_| limb(rng)).collect();
                while v.last() == Some(&0) {
                    v.pop();
                }
                v
            };
            let (x, y) = (mk(&mut rng, 1 + i % 9), mk(&mut rng, 1 + (i / 9) % 9));
            let g = mk(&mut rng, 1 + i % 3);
            let (x, y) = (Int::new(false, x), Int::new(false, y));
            let g = Int::new(false, g);
            for (p, q) in [(x.clone(), y.clone()), (x.mul(&g), y.mul(&g))] {
                if p.is_zero() || q.is_zero() {
                    continue;
                }
                let want = Int::new(false, euclid(p.mag.clone(), q.mag.clone()));
                assert_eq!(p.gcd(&q), want, "{:x?} {:x?}", p.mag, q.mag);
            }
        }
    }

    /// Division and gcd against their definitions on random multi-limb numbers.
    #[test]
    fn division_and_gcd_hold_their_definitions() {
        let mut rng = Rng::new(7);
        for i in 0..300 {
            let (a, d) = (random(&mut rng, 1 + i % 7), random(&mut rng, 1 + i % 4));
            if d.is_zero() {
                continue;
            }
            let (q, r) = a.div_rem(&d).unwrap();
            assert_eq!(q.mul(&d).add(&r), a);
            assert_eq!(r.cmp_abs(&d), Ordering::Less);
            assert!(r.is_zero() || r.is_negative() == a.is_negative());
            let g = random(&mut rng, 1 + i % 3);
            let (x, y) = (a.mul(&g), d.mul(&g));
            let h = x.gcd(&y);
            assert!(x.div_rem(&h).unwrap().1.is_zero());
            assert!(y.div_rem(&h).unwrap().1.is_zero());
            // Coprime after dividing out: no common factor left.
            assert!(x.div_exact(&h).gcd(&y.div_exact(&h)).is_one());
            assert!(h.div_rem(&g.abs()).unwrap().1.is_zero());
            assert_eq!(a.shl(77).shr(77), a);
            assert_eq!(
                a.shl(64).trailing_zeros(),
                a.trailing_zeros().map(|z| z + 64)
            );
        }
        assert!(Int::default().div_rem(&Int::default()).is_none());
    }
}
