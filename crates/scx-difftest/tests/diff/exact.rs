// Written for solidcorex. Apache-2.0.
//! Exact integers for the true values the pin defects check against: sign and magnitude in
//! 64-bit limbs, with the few operations the references need. Every finite double is an
//! integer times a power of two, so a computation on doubles brought to one power of two is
//! exact here.

use core::cmp::Ordering;

/// An integer: `neg` and the magnitude's limbs, least significant first, with no high zero
/// limb (zero is no limbs and not negative).
#[derive(Clone, Debug, Default)]
pub struct Int {
    neg: bool,
    mag: Vec<u64>,
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
        // x = m * 2^q, m an integer below 2^53
        let parts = xs.map(|x| {
            let b = x.to_bits();
            let field = ((b >> 52) & 0x7ff) as i64;
            let frac = b & ((1 << 52) - 1);
            let (m, q) = if field == 0 {
                (frac, -1074)
            } else {
                (frac | (1 << 52), field - 1075)
            };
            (x < 0.0, m, q)
        });
        let e = parts
            .iter()
            .filter(|p| p.1 != 0)
            .map(|p| p.2)
            .min()
            .unwrap_or(0);
        (
            parts.map(|(neg, m, q)| Int::new(neg, vec![m]).shl((q - e).max(0) as usize)),
            e,
        )
    }

    pub fn is_zero(&self) -> bool {
        self.mag.is_empty()
    }

    pub fn signum(&self) -> i32 {
        if self.is_zero() {
            0
        } else if self.neg {
            -1
        } else {
            1
        }
    }

    fn shl(&self, n: usize) -> Int {
        let (limbs, bits) = (n / 64, n % 64);
        let mut mag = vec![0; limbs];
        let mut carry = 0;
        for &w in &self.mag {
            mag.push((w << bits) | carry);
            carry = if bits == 0 { 0 } else { w >> (64 - bits) };
        }
        mag.push(carry);
        Int::new(self.neg, mag)
    }

    fn cmp_mag(a: &[u64], b: &[u64]) -> Ordering {
        a.len()
            .cmp(&b.len())
            .then_with(|| a.iter().rev().cmp(b.iter().rev()))
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
        let mut out = Vec::with_capacity(a.len());
        let mut borrow = 0u64;
        for (i, &x) in a.iter().enumerate() {
            let (d1, b1) = x.overflowing_sub(b.get(i).copied().unwrap_or(0));
            let (d2, b2) = d1.overflowing_sub(borrow);
            out.push(d2);
            borrow = u64::from(b1) + u64::from(b2);
        }
        out
    }

    pub fn add(&self, o: &Int) -> Int {
        if self.neg == o.neg {
            return Int::new(self.neg, Int::add_mag(&self.mag, &o.mag));
        }
        match Int::cmp_mag(&self.mag, &o.mag) {
            Ordering::Less => Int::new(o.neg, Int::sub_mag(&o.mag, &self.mag)),
            _ => Int::new(self.neg, Int::sub_mag(&self.mag, &o.mag)),
        }
    }

    pub fn neg(&self) -> Int {
        Int::new(!self.neg, self.mag.clone())
    }

    pub fn sub(&self, o: &Int) -> Int {
        self.add(&o.neg())
    }

    pub fn mul(&self, o: &Int) -> Int {
        let mut out = vec![0u64; self.mag.len() + o.mag.len() + 1];
        for (i, &x) in self.mag.iter().enumerate() {
            let mut carry = 0u128;
            for (j, &y) in o.mag.iter().enumerate() {
                let t = u128::from(x) * u128::from(y) + u128::from(out[i + j]) + carry;
                out[i + j] = t as u64;
                carry = t >> 64;
            }
            let mut k = i + o.mag.len();
            while carry != 0 {
                let t = u128::from(out[k]) + carry;
                out[k] = t as u64;
                carry = t >> 64;
                k += 1;
            }
        }
        Int::new(self.neg != o.neg, out)
    }

    /// `|self| * 2^se` against `|o| * 2^oe`.
    pub fn cmp_scaled(&self, se: i64, o: &Int, oe: i64) -> Ordering {
        let (a, b) = if se >= oe {
            (self.shl((se - oe) as usize), o.clone())
        } else {
            (self.clone(), o.shl((oe - se) as usize))
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
        let next = if n >= 2 { self.mag[n - 2] } else { 0 };
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

/// `(a * 2^ka) / (b * 2^kb)` as a double, for `b` not zero.
pub fn ratio(a: &Int, b: &Int, k: i64) -> f64 {
    let ((ma, ea), (mb, eb)) = (a.approx(), b.approx());
    ldexp(ma / mb, ea - eb + k)
}

/// `sqrt((a / b) * 2^k)` as a double, for `a` not negative and `b` positive.
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
}
