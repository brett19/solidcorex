// Written for solidcorex. Apache-2.0.
//! Exact sums and products of doubles held as expansions (Shewchuk, "Adaptive Precision
//! Floating-Point Arithmetic and Fast Robust Geometric Predicates", 1997): a value is the
//! exact sum of nonoverlapping terms, smallest first, with zeros dropped (zero is the empty
//! expansion).
//!
//! A term is a double's significand with an exponent of its own (`m * 2^e`, `1 <= |m| < 2`),
//! so nothing overflows or underflows: the doubles `1e300` and `5e-324` and their products
//! of any degree are exact. The error-free transformations of [`crate::eft`] act on the
//! significands, brought to one frame where they are exact; two terms more than `2^1100`
//! apart cannot overlap and are left as they are, which is what an unbounded-exponent
//! `two_sum` returns for them.

use crate::eft;
use crate::fmath::dd;

/// `m * 2^e`, with `1 <= |m| < 2`.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Term {
    m: f64,
    e: i32,
}

/// `x * 2^e` as a term, `None` for zero (`x` finite).
fn term(x: f64, e: i32) -> Option<Term> {
    if x == 0.0 {
        return None;
    }
    let ex = dd::exponent(x);
    // Scaling to [1, 2) is exact, from a normal or a subnormal x.
    Some(Term {
        m: dd::scale(x, -ex),
        e: e + ex,
    })
}

impl Term {
    /// Whether `|self| < |other|`.
    fn below(self, other: Term) -> bool {
        (self.e, self.m.abs()) < (other.e, other.m.abs())
    }
}

/// `a + b` as the rounded sum and its exact error, as `eft::two_sum` would give with an
/// unbounded exponent.
fn two_sum(a: Option<Term>, b: Option<Term>) -> (Option<Term>, Option<Term>) {
    let (Some(a), Some(b)) = (a, b) else {
        return (a.or(b), None);
    };
    let (big, small) = if a.below(b) { (b, a) } else { (a, b) };
    let gap = big.e - small.e;
    if gap > 1100 {
        return (Some(big), Some(small));
    }
    // A frame where both significands are normal doubles (big near 2^500, small at least
    // 2^-600), so the transformation is exact.
    let f = big.e - 500;
    let (s, r) = eft::two_sum(dd::scale(big.m, 500), dd::scale(small.m, 500 - gap));
    (term(s, f), term(r, f))
}

/// `a * b` as the rounded product and its exact error.
fn two_prod(a: Term, b: Term) -> (Option<Term>, Option<Term>) {
    let (p, r) = eft::two_prod(a.m, b.m);
    (term(p, a.e + b.e), term(r, a.e + b.e))
}

/// An exact value: the sum of its terms, nonoverlapping, in increasing magnitude.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct Expansion(Vec<Term>);

impl Expansion {
    /// The finite double `x`, exactly.
    pub(crate) fn of(x: f64) -> Self {
        Expansion(term(x, 0).into_iter().collect())
    }

    /// `x - y` for finite doubles, exactly.
    pub(crate) fn difference(x: f64, y: f64) -> Self {
        let (s, e) = two_sum(term(x, 0), term(-y, 0));
        Expansion(e.into_iter().chain(s).collect())
    }

    /// `self + other`, exactly (Shewchuk's linear expansion sum, with zeros dropped).
    pub(crate) fn add(&self, other: &Expansion) -> Expansion {
        // Both lists merged by magnitude, smallest first.
        let mut g = Vec::with_capacity(self.0.len() + other.0.len());
        let (mut i, mut j) = (self.0.iter().peekable(), other.0.iter().peekable());
        while let (Some(&&a), Some(&&b)) = (i.peek(), j.peek()) {
            if a.below(b) {
                g.push(a);
                i.next();
            } else {
                g.push(b);
                j.next();
            }
        }
        g.extend(i);
        g.extend(j);
        let mut it = g.into_iter();
        let (Some(g0), Some(g1)) = (it.next(), it.next()) else {
            return Expansion(self.0.iter().chain(&other.0).copied().collect());
        };
        let mut h = Vec::new();
        let (mut big, mut small) = two_sum(Some(g1), Some(g0));
        for gi in it {
            let (r, h0) = two_sum(Some(gi), small);
            h.extend(h0);
            (big, small) = two_sum(big, r);
        }
        h.extend(small);
        h.extend(big);
        Expansion(h)
    }

    /// `-self`, exactly.
    pub(crate) fn neg(&self) -> Expansion {
        Expansion(self.0.iter().map(|t| Term { m: -t.m, e: t.e }).collect())
    }

    /// `self - other`, exactly.
    pub(crate) fn sub(&self, other: &Expansion) -> Expansion {
        self.add(&other.neg())
    }

    /// `self * 2^k`, exactly.
    pub(crate) fn pow2(&self, k: i32) -> Expansion {
        Expansion(self.0.iter().map(|t| Term { m: t.m, e: t.e + k }).collect())
    }

    /// `|self|`.
    pub(crate) fn abs(&self) -> Expansion {
        if self.sign() < 0 {
            self.neg()
        } else {
            self.clone()
        }
    }

    /// `self * b`, exactly (Shewchuk's scale expansion, with zeros dropped).
    fn scale(&self, b: Term) -> Expansion {
        let mut h = Vec::with_capacity(2 * self.0.len());
        let mut it = self.0.iter();
        let Some(&e0) = it.next() else {
            return Expansion(h);
        };
        let (mut q, h0) = two_prod(e0, b);
        h.extend(h0);
        for &e in it {
            let (p1, p0) = two_prod(e, b);
            let (sum, h1) = two_sum(q, p0);
            h.extend(h1);
            let (nq, h2) = two_sum(p1, sum);
            h.extend(h2);
            q = nq;
        }
        h.extend(q);
        Expansion(h)
    }

    /// `self * other`, exactly.
    pub(crate) fn mul(&self, other: &Expansion) -> Expansion {
        other
            .0
            .iter()
            .fold(Expansion::default(), |acc, &b| acc.add(&self.scale(b)))
    }

    /// The sign: `-1`, `0` or `1` (the largest term's, the others being smaller than its
    /// last bit).
    pub(crate) fn sign(&self) -> i32 {
        self.0.last().map_or(0, |t| if t.m < 0.0 { -1 } else { 1 })
    }

    /// The value as a double-double with an exponent, within a few units of `2^-104` of
    /// it (the terms are nonoverlapping, so the sum does not cancel).
    pub(crate) fn approx(&self) -> Approx {
        let Some(top) = self.0.last() else {
            return Approx::ZERO;
        };
        let s = self.0.iter().fold((0.0, 0.0), |acc, t| {
            dd::add(acc, (dd::scale(t.m, t.e - top.e), 0.0))
        });
        Approx::new(s, top.e)
    }
}

/// `(hi + lo) * 2^e` for a normalised double-double with `1 <= |hi| < 2`, or zero: values
/// far beyond the doubles' range, to a relative `2^-100` or so.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Approx {
    hi: f64,
    lo: f64,
    e: i32,
}

impl Approx {
    const ZERO: Approx = Approx {
        hi: 0.0,
        lo: 0.0,
        e: 0,
    };

    /// `(v.0 + v.1) * 2^e`, renormalised (`v` normalised, near 1 in magnitude).
    fn new(v: (f64, f64), e: i32) -> Approx {
        if v.0 == 0.0 || !v.0.is_finite() {
            return Approx::ZERO;
        }
        let k = dd::exponent(v.0);
        Approx {
            hi: dd::scale(v.0, -k),
            lo: dd::scale(v.1, -k),
            e: e + k,
        }
    }

    pub(crate) fn mul(self, o: Approx) -> Approx {
        Approx::new(dd::mul((self.hi, self.lo), (o.hi, o.lo)), self.e + o.e)
    }

    /// Whether `|self|` is clearly below (`Some(true)`) or above (`Some(false)`) `|o|`:
    /// `None` within a relative `2^-90`, nearer than the two approximations can tell.
    pub(crate) fn clearly_below(self, o: Approx) -> Option<bool> {
        let (x, y) = self.together(o);
        let (x, y) = (x.abs(), y.abs());
        let margin = dd::scale(x.max(y), -90);
        if x + margin < y {
            Some(true)
        } else if y + margin < x {
            Some(false)
        } else {
            None
        }
    }

    /// `self / o`, for `o` nonzero.
    pub(crate) fn div(self, o: Approx) -> Approx {
        Approx::new(dd::div((self.hi, self.lo), (o.hi, o.lo)), self.e - o.e)
    }

    /// The square root, for `self` at least zero.
    pub(crate) fn sqrt(self) -> Approx {
        if self.hi == 0.0 {
            return Approx::ZERO;
        }
        let odd = self.e.rem_euclid(2);
        let v = (dd::scale(self.hi, odd), dd::scale(self.lo, odd));
        Approx::new(dd::sqrt(v), (self.e - odd) / 2)
    }

    /// The nearest double (infinite beyond the doubles' range).
    pub(crate) fn round(self) -> f64 {
        if self.hi == 0.0 {
            0.0
        } else {
            dd::round_scaled(self.hi, self.lo, self.e)
        }
    }

    /// `self` and `o` as doubles in one scale (the larger near 1, the smaller as accurate as
    /// a double at that scale holds it): for a ratio of the two, such as `atan2`'s.
    pub(crate) fn together(self, o: Approx) -> (f64, f64) {
        let top = match (self.hi == 0.0, o.hi == 0.0) {
            (true, true) => return (0.0, 0.0),
            (true, false) => o.e,
            (false, true) => self.e,
            (false, false) => self.e.max(o.e),
        };
        (dd::scale(self.hi, self.e - top), dd::scale(o.hi, o.e - top))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `2^k` for a normal power of two.
    fn p2(k: i32) -> f64 {
        f64::from_bits(((1023 + k) as u64) << 52)
    }

    fn nonoverlapping(e: &Expansion) -> bool {
        e.0.windows(2).all(|w| w[0].below(w[1]))
    }

    fn value(e: &Expansion) -> (f64, f64) {
        let a = e.approx();
        (a.round(), dd::scale(a.lo, a.e))
    }

    #[test]
    fn sums_and_products_are_exact() {
        let a = Expansion::difference(1.0, p2(-60));
        let b = Expansion::difference(3.0, -p2(-58));
        assert_eq!(a.0.len(), 2);
        let s = a.add(&b);
        assert_eq!(value(&s), (4.0, p2(-58) - p2(-60)));
        assert!(a.sub(&a).0.is_empty());
        // (1 - 2^-60)(3 + 2^-58) = 3 + (2^-58 - 3 * 2^-60) - 2^-118: three terms.
        let p = a.mul(&b);
        assert!(nonoverlapping(&p));
        let want = [-p2(-118), p2(-58) - 3.0 * p2(-60), 3.0];
        assert_eq!(p.0, want.map(|x| term(x, 0).unwrap()));
        assert!(Expansion::of(0.0).mul(&p).0.is_empty());
        assert_eq!(p.sign(), 1);
        assert_eq!(p.neg().sign(), -1);
    }

    /// Cancellation leaves exactly the small remainder.
    #[test]
    fn cancellation_is_exact() {
        let x = Expansion::of(1.0e20).add(&Expansion::of(1.0));
        let y = x.sub(&Expansion::of(1.0e20));
        assert_eq!(value(&y), (1.0, 0.0));
        let sq = x
            .mul(&x)
            .sub(&Expansion::of(1.0e20).mul(&Expansion::of(1.0e20)));
        // (1e20 + 1)^2 - 1e40 = 2e20 + 1, as computed from the doubles 1e20 and 1e40's parts
        assert_eq!(value(&sq), (2.0e20, 1.0));
        assert!(nonoverlapping(&sq));
    }

    /// Far beyond the doubles' range: (1e300 + 5e-324)^2 - 1e600 = 1e-23 + 2.5e-647 exactly,
    /// and its square root and ratios round once.
    #[test]
    fn range_is_unbounded() {
        let tiny = f64::from_bits(1);
        let x = Expansion::of(1.0e300).add(&Expansion::of(tiny));
        let big = Expansion::of(1.0e300).mul(&Expansion::of(1.0e300));
        let d = x.mul(&x).sub(&big);
        assert!(nonoverlapping(&d));
        assert_eq!(d.approx().round(), 2.0 * 1.0e300 * tiny);
        assert_eq!(big.approx().div(d.approx()).round(), f64::INFINITY);
        assert_eq!(big.approx().sqrt().round(), 1.0e300);
        assert_eq!(d.approx().div(big.approx()).round(), 0.0);
        assert_eq!(x.sub(&Expansion::of(1.0e300)).approx().round(), tiny);
        let (u, v) = big.approx().together(d.approx());
        assert!((1.0..2.0).contains(&u));
        assert_eq!(v, 0.0);
    }
}
