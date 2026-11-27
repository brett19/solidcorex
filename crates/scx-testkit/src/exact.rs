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
        let (Some(za), Some(zb)) = (self.trailing_zeros(), o.trailing_zeros()) else {
            return if self.is_zero() { o.abs() } else { self.abs() };
        };
        // A unit divides everything; the loop below would take one step per bit for it.
        if self.mag == [1] || o.mag == [1] {
            return Int::from(1u64);
        }
        let mut a = self.abs().shr(za);
        let mut b = o.abs().shr(zb);
        // Binary gcd on odd numbers: each step clears at least one bit of the larger.
        loop {
            if let ([x], [y]) = (a.mag.as_slice(), b.mag.as_slice()) {
                return Int::from(gcd_u64(*x, *y)).shl(za.min(zb));
            }
            match Int::cmp_mag(&a.mag, &b.mag) {
                Ordering::Equal => return a.shl(za.min(zb)),
                Ordering::Greater => {
                    sub_in_place(&mut a.mag, &b.mag);
                    a = a.shr(a.trailing_zeros().unwrap_or(0));
                }
                Ordering::Less => {
                    sub_in_place(&mut b.mag, &a.mag);
                    b = b.shr(b.trailing_zeros().unwrap_or(0));
                }
            }
        }
    }

    /// `self / d` for a `d` known to divide `self` (the quotient rounded toward zero
    /// otherwise); zero when `d` is zero.
    #[must_use]
    pub fn div_exact(&self, d: &Int) -> Int {
        self.div_rem(d).map(|(q, _)| q).unwrap_or_default()
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

/// Magnitudes `(n / d, n % d)`; `None` when `d` is zero. One limb at a time for a one-limb
/// divisor, else one bit at a time (the quotients here are short or the numbers small).
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
        _ => {
            let bits = 64 * n.len() as u64;
            let mut q = vec![0u64; n.len()];
            let mut r: Vec<u64> = Vec::with_capacity(d.len() + 1);
            for bit in (0..bits).rev() {
                let b = n
                    .get((bit / 64) as usize)
                    .map_or(0, |w| (w >> (bit % 64)) & 1);
                // r = 2r + b
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
            Some((q, r))
        }
    }
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
