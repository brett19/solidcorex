// Written for solidcorex. Apache-2.0.
//! Certified enclosures: closed intervals `[lo, hi]` of exact rationals, rounded outward.
//!
//! A value that is not rational (a square root, a sine) is held as an interval certain to
//! contain it. Sums, differences and products of intervals are exact; square roots,
//! reciprocals and [`Bound::rounded`] round outward onto a grid of `2^-k`, so an enclosure
//! is never wrong, only wider. `bits` is the relative width aimed at: about `2^-bits` of the
//! value's magnitude.

use crate::dd::Dd;
use crate::rat::Rat;

/// A closed interval of rationals, `lo <= hi`, certain to hold the value it stands for.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Bound {
    lo: Rat,
    hi: Rat,
}

impl From<Rat> for Bound {
    fn from(x: Rat) -> Bound {
        Bound::exact(x)
    }
}

impl Bound {
    /// The point `[x, x]`.
    pub fn exact(x: Rat) -> Bound {
        Bound {
            lo: x.clone(),
            hi: x,
        }
    }

    /// `[lo, hi]`; `None` when `lo > hi`.
    pub fn new(lo: Rat, hi: Rat) -> Option<Bound> {
        (lo <= hi).then_some(Bound { lo, hi })
    }

    /// The finite double `x` as a point; `None` for an infinity or NaN.
    pub fn of_f64(x: f64) -> Option<Bound> {
        Rat::from_f64(x).map(Bound::exact)
    }

    /// The lower end.
    pub fn lo(&self) -> &Rat {
        &self.lo
    }

    /// The upper end.
    pub fn hi(&self) -> &Rat {
        &self.hi
    }

    /// `hi - lo`.
    pub fn width(&self) -> Rat {
        &self.hi - &self.lo
    }

    /// The midpoint.
    pub fn mid(&self) -> Rat {
        self.lo.midpoint(&self.hi)
    }

    /// Whether this is a single point.
    pub fn is_exact(&self) -> bool {
        self.lo == self.hi
    }

    /// Whether `x` is inside.
    pub fn contains(&self, x: &Rat) -> bool {
        &self.lo <= x && x <= &self.hi
    }

    /// Whether every value here is below every value of `o`.
    pub fn below(&self, o: &Bound) -> bool {
        self.hi < o.lo
    }

    /// Whether the two share a value.
    pub fn meets(&self, o: &Bound) -> bool {
        self.lo <= o.hi && o.lo <= self.hi
    }

    /// The sign of every value here: `Some(0)` only for the point zero, `None` when the
    /// interval holds values of two signs.
    pub fn signum(&self) -> Option<i32> {
        let (a, b) = (self.lo.signum(), self.hi.signum());
        (a == b).then_some(a)
    }

    /// `-self`.
    #[must_use]
    pub fn neg(&self) -> Bound {
        Bound {
            lo: -&self.hi,
            hi: -&self.lo,
        }
    }

    /// `self + o`, exactly.
    #[must_use]
    pub fn add(&self, o: &Bound) -> Bound {
        Bound {
            lo: &self.lo + &o.lo,
            hi: &self.hi + &o.hi,
        }
    }

    /// `self - o`, exactly.
    #[must_use]
    pub fn sub(&self, o: &Bound) -> Bound {
        Bound {
            lo: &self.lo - &o.hi,
            hi: &self.hi - &o.lo,
        }
    }

    /// `self * o`, exactly.
    #[must_use]
    pub fn mul(&self, o: &Bound) -> Bound {
        if self.is_exact() && o.is_exact() {
            return Bound::exact(&self.lo * &o.lo);
        }
        let p = [
            &self.lo * &o.lo,
            &self.lo * &o.hi,
            &self.hi * &o.lo,
            &self.hi * &o.hi,
        ];
        let lo = p.iter().min().cloned().unwrap_or_default();
        let hi = p.iter().max().cloned().unwrap_or_default();
        Bound { lo, hi }
    }

    /// `self * x` for a rational `x`, exactly.
    #[must_use]
    pub fn mul_rat(&self, x: &Rat) -> Bound {
        let (a, b) = (&self.lo * x, &self.hi * x);
        if x.signum() < 0 {
            Bound { lo: b, hi: a }
        } else {
            Bound { lo: a, hi: b }
        }
    }

    /// `self^2`, exactly (never below zero).
    #[must_use]
    pub fn sqr(&self) -> Bound {
        let (a, b) = (&self.lo * &self.lo, &self.hi * &self.hi);
        if self.lo.signum() <= 0 && self.hi.signum() >= 0 {
            Bound {
                lo: Rat::zero(),
                hi: a.max(b),
            }
        } else {
            Bound {
                lo: a.clone().min(b.clone()),
                hi: a.max(b),
            }
        }
    }

    /// `|self|`, exactly.
    #[must_use]
    pub fn abs(&self) -> Bound {
        if self.lo.signum() >= 0 {
            self.clone()
        } else if self.hi.signum() <= 0 {
            self.neg()
        } else {
            Bound {
                lo: Rat::zero(),
                hi: self.hi.clone().max(-&self.lo),
            }
        }
    }

    /// The smaller of the two, value by value.
    #[must_use]
    pub fn min(&self, o: &Bound) -> Bound {
        Bound {
            lo: self.lo.clone().min(o.lo.clone()),
            hi: self.hi.clone().min(o.hi.clone()),
        }
    }

    /// The grid exponent `k` (a grid of `2^-k`) giving about `bits` bits of `x`.
    fn grid(x: &Rat, bits: u32) -> i64 {
        i64::from(bits) - x.log2_approx().unwrap_or(0)
    }

    /// Outward onto a grid of about `2^-bits` relative: wider, never narrower, and with
    /// short numbers.
    #[must_use]
    pub fn rounded(&self, bits: u32) -> Bound {
        let big = self.lo.abs().max(self.hi.abs());
        if big.is_zero() {
            return self.clone();
        }
        let k = Bound::grid(&big, bits);
        Bound {
            lo: self.lo.floor_at(k),
            hi: self.hi.ceil_at(k),
        }
    }

    /// `sqrt(self)` (negative values clamped to zero), rounded outward; `None` when every
    /// value is negative.
    pub fn sqrt(&self, bits: u32) -> Option<Bound> {
        if self.hi.signum() < 0 {
            return None;
        }
        let lo = self.lo.clone().max(Rat::zero());
        // The grid from the upper end; for an upper end of zero, any grid.
        let k = (Bound::grid(&self.hi, bits) + 1) / 2 + i64::from(bits) / 2;
        let lo_root = lo.sqrt_floor_at(k)?;
        let hi_root = self.hi.sqrt_floor_at(k)?;
        // floor(sqrt(hi) 2^k) 2^-k is below sqrt(hi) by less than 2^-k; exact squares stay.
        let hi_root = if &hi_root * &hi_root == self.hi {
            hi_root
        } else {
            &hi_root + &Rat::one().scale(-k)
        };
        Some(Bound {
            lo: lo_root,
            hi: hi_root,
        })
    }

    /// `1 / self`, rounded outward; `None` when the interval holds zero.
    pub fn recip(&self, bits: u32) -> Option<Bound> {
        if self.lo.signum() * self.hi.signum() <= 0 {
            return None;
        }
        let (a, b) = (self.hi.recip()?, self.lo.recip()?);
        Some(Bound { lo: a, hi: b }.rounded(bits))
    }

    /// `self / o`, rounded outward; `None` when `o` holds zero.
    pub fn div(&self, o: &Bound, bits: u32) -> Option<Bound> {
        Some(self.mul(&o.recip(bits)?).rounded(bits))
    }

    /// The double-double nearest the midpoint (a reference value, not an enclosure).
    pub fn to_dd(&self) -> Dd {
        rat_to_dd(&self.mid())
    }

    /// A double at least half the width: `|x - mid| <= radius()` for every `x` inside.
    pub fn radius(&self) -> f64 {
        up(&self.width().scale(-1))
    }

    /// `[lo, hi]` as doubles rounded outward.
    pub fn to_f64s(&self) -> (f64, f64) {
        (down(&self.lo), up(&self.hi))
    }
}

/// `x` as a double-double, `hi` the nearest double and `lo` the nearest to the rest.
pub fn rat_to_dd(x: &Rat) -> Dd {
    let hi = x.to_f64();
    let lo = Rat::from_f64(hi).map_or(0.0, |h| (x - &h).to_f64());
    Dd::of((hi, lo))
}

/// The next double up from a finite `x`.
fn next_up(x: f64) -> f64 {
    if x == 0.0 {
        return f64::from_bits(1);
    }
    let b = x.to_bits();
    f64::from_bits(if x > 0.0 { b + 1 } else { b - 1 })
}

/// A double at least `x`.
pub fn up(x: &Rat) -> f64 {
    let f = x.to_f64();
    match Rat::from_f64(f) {
        Some(r) if &r < x => next_up(f),
        _ => f,
    }
}

/// A double at most `x`.
pub fn down(x: &Rat) -> f64 {
    -up(&-x)
}

/// `(sin x, cos x)` for a double `|x| <= 4`, certified: the Taylor series to a term below
/// `2^-(bits + 8)`, and that term's magnitude (a bound on the rest, Lagrange's) added both
/// ways. `None` for a larger or non-finite `x`.
pub fn sin_cos(x: f64, bits: u32) -> Option<(Bound, Bound)> {
    if x.is_nan() || x.abs() > 4.0 {
        return None;
    }
    let x = Bound::of_f64(x)?;
    let small = Rat::one().scale(-i64::from(bits) - 8);
    let (mut s, mut c) = (Bound::exact(Rat::zero()), Bound::exact(Rat::zero()));
    let mut term = Bound::exact(Rat::one()); // x^n / n!
    let mut n: i64 = 0;
    loop {
        // the rest past degree n - 1 is at most |x|^n / n! = |term|
        if n >= 10 && term.abs().hi < small {
            let big = term.abs().hi;
            let rest = Bound { lo: -&big, hi: big };
            return Some((s.add(&rest), c.add(&rest)));
        }
        let sign = if (n / 2) % 2 == 0 { 1 } else { -1 };
        let signed = if sign > 0 { term.clone() } else { term.neg() };
        if n % 2 == 0 {
            c = c.add(&signed);
        } else {
            s = s.add(&signed);
        }
        n += 1;
        term = term.mul(&x).div(&Bound::exact(Rat::from(n)), bits + 16)?;
    }
}

/// `tan x` for a double `x` in `(0, pi/2)`, certified ([`sin_cos`]); `None` outside.
pub fn tan(x: f64, bits: u32) -> Option<Bound> {
    if x.is_nan() || x <= 0.0 || x >= core::f64::consts::FRAC_PI_2 {
        return None;
    }
    let (s, c) = sin_cos(x, bits + 8)?;
    s.div(&c, bits)
}
