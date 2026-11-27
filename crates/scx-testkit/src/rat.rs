// Written for solidcorex. Apache-2.0.
//! Exact rationals over the doubles: `num / den * 2^exp`, kept reduced, with a correctly
//! rounded conversion back to `f64`. Every finite double converts exactly, and sums,
//! differences, products and quotients stay exact, so a reference computed here has no
//! rounding error until [`Rat::to_f64`] rounds it once.

use core::cmp::Ordering;
use core::ops::{Add, Mul, Neg, Sub};

use crate::exact::{split, Int};

/// An exact rational `num / den * 2^exp`. Reduced: zero is `0 / 1 * 2^0`; otherwise `num`
/// is odd, `den` odd and positive, and they share no factor. Equal values are equal
/// structs, so `==` and the derived hash are exact.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Rat {
    num: Int,
    den: Int,
    exp: i64,
}

impl Default for Rat {
    fn default() -> Rat {
        Rat::zero()
    }
}

impl From<i64> for Rat {
    fn from(n: i64) -> Rat {
        Rat::reduce(&Int::from(n), &Int::from(1u64), 0)
    }
}

impl Rat {
    /// Zero.
    pub fn zero() -> Rat {
        Rat {
            num: Int::default(),
            den: Int::from(1u64),
            exp: 0,
        }
    }

    /// One.
    pub fn one() -> Rat {
        Rat::from(1)
    }

    /// The finite `x` exactly; `None` for an infinity or NaN.
    pub fn from_f64(x: f64) -> Option<Rat> {
        if !x.is_finite() {
            return None;
        }
        let (neg, m, q) = split(x);
        let num = Int::from(m);
        Some(Rat::reduce(
            &if neg { num.neg() } else { num },
            &Int::from(1u64),
            q,
        ))
    }

    /// `n / d` reduced, for `d` not zero.
    fn reduce(num: &Int, den: &Int, exp: i64) -> Rat {
        let (Some(zn), Some(zd)) = (num.trailing_zeros(), den.trailing_zeros()) else {
            return Rat::zero();
        };
        let (mut num, mut den) = (num.shr(zn), den.shr(zd));
        if den.is_negative() {
            (num, den) = (num.neg(), den.neg());
        }
        let g = num.gcd(&den);
        if !g.is_one() {
            num = num.div_exact(&g);
            den = den.div_exact(&g);
        }
        Rat {
            num,
            den,
            exp: exp + zn as i64 - zd as i64,
        }
    }

    /// Whether this is zero.
    pub fn is_zero(&self) -> bool {
        self.num.is_zero()
    }

    /// -1, 0 or 1.
    pub fn signum(&self) -> i32 {
        self.num.signum()
    }

    /// `|self|`.
    #[must_use]
    pub fn abs(&self) -> Rat {
        Rat {
            num: self.num.abs(),
            ..self.clone()
        }
    }

    /// `self * 2^k`, exactly.
    #[must_use]
    pub fn scale(&self, k: i64) -> Rat {
        if self.is_zero() {
            return Rat::zero();
        }
        Rat {
            exp: self.exp + k,
            ..self.clone()
        }
    }

    /// `1 / self`; `None` for zero.
    pub fn recip(&self) -> Option<Rat> {
        if self.is_zero() {
            return None;
        }
        let num = if self.num.is_negative() {
            self.den.neg()
        } else {
            self.den.clone()
        };
        Some(Rat {
            num,
            den: self.num.abs(),
            exp: -self.exp,
        })
    }

    /// `self / o`; `None` when `o` is zero.
    pub fn checked_div(&self, o: &Rat) -> Option<Rat> {
        Some(self * &o.recip()?)
    }

    /// The bits a double would need to hold this exactly, a measure of its size: the
    /// numerator's and denominator's lengths.
    pub fn bits(&self) -> u64 {
        self.num.bit_len() + self.den.bit_len()
    }

    /// The nearest double, ties to even (the subnormals and the overflow to infinity
    /// included): the one rounding a reference makes.
    pub fn to_f64(&self) -> f64 {
        if self.is_zero() {
            return 0.0;
        }
        let neg = self.num.is_negative();
        let num = self.num.abs();
        // q = floor(num 2^k / den) in [2^55, 2^57), the rest as a sticky bit.
        let k = 56 - num.bit_len() as i64 + self.den.bit_len() as i64;
        let (n, d) = if k >= 0 {
            (num.shl(k as u64), self.den.clone())
        } else {
            (num, self.den.shl(k.unsigned_abs()))
        };
        let Some((q, r)) = n.div_rem(&d) else {
            return f64::NAN; // den is never zero
        };
        let (q, sticky) = (q.low_u64(), !r.is_zero());
        let len = i64::from(64 - q.leading_zeros());
        // The value is (q + sticky) 2^(exp - k); its leading bit is 2^lead.
        let lead = self.exp - k + len - 1;
        let prec = if lead >= -1022 { 53 } else { lead + 1075 };
        let magnitude = if lead > 1023 {
            f64::INFINITY
        } else if prec < 0 {
            0.0 // below half the least subnormal
        } else {
            let drop = (len - prec) as u32; // in [3, 57]
            let (mut m, rest, half) = (q >> drop, q & ((1 << drop) - 1), 1u64 << (drop - 1));
            if rest > half || (rest == half && (sticky || m & 1 == 1)) {
                m += 1;
            }
            if prec < 53 {
                // m 2^-1074, m at most 2^52 (the least normal when it is).
                f64::from_bits(m)
            } else {
                let (m, e) = if m == 1 << 53 {
                    (m >> 1, lead + 1)
                } else {
                    (m, lead)
                };
                if e > 1023 {
                    f64::INFINITY
                } else {
                    f64::from_bits((((e + 1023) as u64) << 52) | (m & ((1 << 52) - 1)))
                }
            }
        };
        if neg {
            -magnitude
        } else {
            magnitude
        }
    }
}

impl Ord for Rat {
    fn cmp(&self, o: &Rat) -> Ordering {
        let (a, b) = (self.signum(), o.signum());
        if a != b || a == 0 {
            return a.cmp(&b);
        }
        // Same sign: |n1| d2 2^e1 against |n2| d1 2^e2.
        let by = self
            .num
            .mul(&o.den)
            .cmp_scaled(self.exp, &o.num.mul(&self.den), o.exp);
        if a > 0 {
            by
        } else {
            by.reverse()
        }
    }
}

impl PartialOrd for Rat {
    fn partial_cmp(&self, o: &Rat) -> Option<Ordering> {
        Some(self.cmp(o))
    }
}

impl Add for &Rat {
    type Output = Rat;
    fn add(self, o: &Rat) -> Rat {
        if self.is_zero() {
            return o.clone();
        }
        if o.is_zero() {
            return self.clone();
        }
        let e = self.exp.min(o.exp);
        let a = self.num.shl((self.exp - e) as u64);
        let b = o.num.shl((o.exp - e) as u64);
        if self.den == o.den {
            return Rat::reduce(&a.add(&b), &self.den, e);
        }
        Rat::reduce(
            &a.mul(&o.den).add(&b.mul(&self.den)),
            &self.den.mul(&o.den),
            e,
        )
    }
}

impl Sub for &Rat {
    type Output = Rat;
    fn sub(self, o: &Rat) -> Rat {
        self + &(-o)
    }
}

impl Neg for &Rat {
    type Output = Rat;
    fn neg(self) -> Rat {
        Rat {
            num: self.num.neg(),
            ..self.clone()
        }
    }
}

impl Mul for &Rat {
    type Output = Rat;
    fn mul(self, o: &Rat) -> Rat {
        if self.is_zero() || o.is_zero() {
            return Rat::zero();
        }
        // Both reduced: only the cross factors can cancel.
        let (g1, g2) = (self.num.gcd(&o.den), o.num.gcd(&self.den));
        let (n1, d2) = (self.num.div_exact(&g1), o.den.div_exact(&g1));
        let (n2, d1) = (o.num.div_exact(&g2), self.den.div_exact(&g2));
        Rat {
            num: n1.mul(&n2),
            den: d1.mul(&d2),
            exp: self.exp + o.exp,
        }
    }
}

macro_rules! owned_ops {
    ($($tr:ident $f:ident),*) => {$(
        impl $tr for Rat {
            type Output = Rat;
            fn $f(self, o: Rat) -> Rat {
                (&self).$f(&o)
            }
        }
        impl $tr<&Rat> for Rat {
            type Output = Rat;
            fn $f(self, o: &Rat) -> Rat {
                (&self).$f(o)
            }
        }
    )*};
}
owned_ops!(Add add, Sub sub, Mul mul);

impl Neg for Rat {
    type Output = Rat;
    fn neg(self) -> Rat {
        -&self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn r(x: f64) -> Rat {
        Rat::from_f64(x).unwrap()
    }

    #[test]
    fn known_values() {
        let third = Rat::one().checked_div(&Rat::from(3)).unwrap();
        assert_eq!(&third * &Rat::from(3), Rat::one());
        assert_eq!(third.to_f64(), 1.0 / 3.0);
        // 0.1 + 0.2 exactly is 0.3000000000000000166..., nearest 0.30000000000000004.
        assert_eq!((r(0.1) + r(0.2)).to_f64(), 0.30000000000000004);
        assert!(r(0.1) + r(0.2) > r(0.3));
        assert_eq!(r(-0.0), Rat::zero());
        assert_eq!(r(f64::MAX).to_f64(), f64::MAX);
        assert_eq!((r(f64::MAX) + r(f64::MAX)).to_f64(), f64::INFINITY);
        assert_eq!(r(f64::from_bits(1)).scale(-1).to_f64(), 0.0); // the tie to even
        assert_eq!(
            (r(f64::from_bits(1)).scale(-1) + r(f64::from_bits(1)).scale(-60)).to_f64(),
            f64::from_bits(1)
        );
        assert_eq!(r(-2.5).cmp(&r(-3.0)), Ordering::Greater);
        assert!(Rat::zero().recip().is_none());
        assert!(Rat::from_f64(f64::NAN).is_none());
    }
}
