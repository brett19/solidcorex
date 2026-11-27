// Written for solidcorex. Apache-2.0.
//! Polynomials with exact rational coefficients, and their real roots isolated with
//! certainty.
//!
//! The method is the textbook one, exact throughout so nothing is a judgement:
//!
//! - Yun's square-free factorisation `p = c f_1 f_2^2 f_3^3 ...` (each `f_i` square-free,
//!   pairwise coprime) gives the multiplicities, by exact `gcd`s;
//! - a Sturm sequence of each `f_i` counts its roots in any half-open interval `(a, b]`, so
//!   bisection from Cauchy's bound isolates them;
//! - an isolated root of a square-free factor changes its sign, so bisection on the sign
//!   alone refines it to the width asked for; roots of different factors are distinct, so
//!   any two enclosures that meet are refined until they do not.
//!
//! The work is done on integer polynomials with the primitive pseudo-remainder sequence
//! (see `Z`), since reducing rationals at every step dominates the time otherwise. Every
//! real root comes back once, as an interval sure to hold it and no other root, with its
//! multiplicity; a root met at a bisection point comes back as that exact point.

use crate::bound::{Bound, rat_to_dd};
use crate::dd::Dd;
use crate::exact::Int;
use crate::rat::Rat;

/// A polynomial `sum c_i x^i` with rational coefficients, lowest first, with no trailing
/// zero (the zero polynomial has none).
#[derive(Clone, Debug, PartialEq, Eq, Hash, Default)]
pub struct Poly(Vec<Rat>);

/// A real root: an interval holding it and no other root, and how many times it repeats.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Root {
    /// The enclosure; a single point when the root is that point exactly.
    pub at: Bound,
    /// Its multiplicity, at least one.
    pub multiplicity: u32,
}

/// A polynomial with integer coefficients, lowest first, no trailing zero: the working
/// form of [`Poly::real_roots`]. Remainders are pseudo-remainders with the content divided
/// out (the primitive remainder sequence), so no step reduces a rational: the rationals'
/// gcds dominate the time otherwise.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
struct Z(Vec<Int>);

impl Z {
    fn new(mut c: Vec<Int>) -> Z {
        while c.last().is_some_and(Int::is_zero) {
            c.pop();
        }
        Z(c)
    }

    /// A positive multiple of `p`: times the odd denominators' lcm and the least power of
    /// two.
    fn of(p: &Poly) -> Z {
        let lo = p.0.iter().map(|c| c.parts().2).min().unwrap_or(0);
        let lcm = p.0.iter().fold(Int::from(1u64), |l, c| {
            let d = c.parts().1;
            let g = l.gcd(d);
            l.mul(&d.div_exact(&g))
        });
        Z::new(
            p.0.iter()
                .map(|c| {
                    let (n, d, e) = c.parts();
                    n.mul(&lcm.div_exact(d)).shl(e.abs_diff(lo))
                })
                .collect(),
        )
    }

    fn degree(&self) -> usize {
        self.0.len().saturating_sub(1)
    }

    fn lead(&self) -> Int {
        self.0.last().cloned().unwrap_or_default()
    }

    fn is_zero(&self) -> bool {
        self.0.is_empty()
    }

    fn derivative(&self) -> Z {
        Z::new(
            self.0
                .iter()
                .enumerate()
                .skip(1)
                .map(|(i, c)| c.mul(&Int::from(i as u64)))
                .collect(),
        )
    }

    fn neg(&self) -> Z {
        Z(self.0.iter().map(Int::neg).collect())
    }

    fn sub(&self, o: &Z) -> Z {
        let n = self.0.len().max(o.0.len());
        let at = |v: &[Int], i: usize| v.get(i).cloned().unwrap_or_default();
        Z::new((0..n).map(|i| at(&self.0, i).sub(&at(&o.0, i))).collect())
    }

    /// Divided by the (positive) gcd of its coefficients.
    fn primitive(&self) -> Z {
        let g = self.0.iter().fold(Int::default(), |g, c| g.gcd(c));
        if g.is_zero() || g.is_one() {
            return self.clone();
        }
        Z(self.0.iter().map(|c| c.div_exact(&g)).collect())
    }

    /// `|lc(b)|^k a - q b` of degree below `b`'s, for the steps `k`: a positive multiple of
    /// the remainder.
    fn prem(&self, b: &Z) -> Z {
        let lb = b.lead();
        let (mag, sign) = (lb.abs(), lb.signum());
        let mut r = self.clone();
        while !r.is_zero() && r.degree() >= b.degree() {
            let shift = r.degree() - b.degree();
            let f = if sign < 0 { r.lead().neg() } else { r.lead() };
            let mut next: Vec<Int> = r.0.iter().map(|c| c.mul(&mag)).collect();
            for (i, c) in b.0.iter().enumerate() {
                if let Some(slot) = next.get_mut(i + shift) {
                    *slot = slot.sub(&f.mul(c));
                }
            }
            next.pop();
            r = Z::new(next);
        }
        r
    }

    /// `a / b` for a primitive `b` dividing `a` over the rationals (so over the integers,
    /// by Gauss's lemma).
    fn div_exact(&self, b: &Z) -> Z {
        let lb = b.lead();
        let mut r = self.0.clone();
        let (n, m) = (self.0.len(), b.0.len());
        if m == 0 || n < m {
            return Z::default();
        }
        let mut q = vec![Int::default(); n - m + 1];
        for k in (0..=n - m).rev() {
            let f = r.get(k + m - 1).cloned().unwrap_or_default().div_exact(&lb);
            for (i, c) in b.0.iter().enumerate() {
                if let Some(slot) = r.get_mut(k + i) {
                    *slot = slot.sub(&f.mul(c));
                }
            }
            if let Some(slot) = q.get_mut(k) {
                *slot = f;
            }
        }
        Z::new(q)
    }

    /// The primitive gcd, leading coefficient positive.
    fn gcd(&self, o: &Z) -> Z {
        let (mut a, mut b) = (self.primitive(), o.primitive());
        if a.degree() < b.degree() {
            core::mem::swap(&mut a, &mut b);
        }
        while !b.is_zero() {
            let r = a.prem(&b).primitive();
            a = b;
            b = r;
        }
        if a.lead().is_negative() { a.neg() } else { a }
    }

    /// Yun's square-free factorisation, as [`Poly::square_free`], each factor primitive.
    fn square_free(&self) -> Vec<(Z, u32)> {
        let mut out = Vec::new();
        if self.degree() == 0 {
            return out;
        }
        let d = self.derivative();
        let a = self.gcd(&d);
        let mut b = self.div_exact(&a);
        let mut c = d.div_exact(&a);
        let mut i = 1;
        loop {
            let dd = c.sub(&b.derivative());
            let f = b.gcd(&dd);
            if f.degree() > 0 {
                out.push((f.clone(), i));
            }
            b = b.div_exact(&f);
            if b.degree() == 0 {
                return out;
            }
            c = dd.div_exact(&f);
            i += 1;
        }
    }

    /// The Sturm sequence `p, p', -rem, ...`, each scaled by a positive factor.
    fn sturm(&self) -> Vec<Z> {
        let mut s = vec![self.clone(), self.derivative().primitive()];
        loop {
            let (Some(a), Some(b)) = (s.get(s.len() - 2), s.last()) else {
                return s;
            };
            if b.degree() == 0 {
                return s;
            }
            let r = a.prem(b);
            if r.is_zero() {
                return s;
            }
            s.push(r.neg().primitive());
        }
    }

    /// The sign at `x`: of `d^n p(n / d)` by homogeneous Horner, `d > 0`.
    fn sign_at(&self, x: &Rat) -> i32 {
        let (num, den, e) = x.parts();
        let (n, d) = if e >= 0 {
            (num.shl(e.unsigned_abs()), den.clone())
        } else {
            (num.clone(), den.shl(e.unsigned_abs()))
        };
        let mut acc = Int::default();
        let mut dk = Int::from(1u64);
        for c in self.0.iter().rev() {
            acc = acc.mul(&n).add(&c.mul(&dk));
            dk = dk.mul(&d);
        }
        acc.signum()
    }

    /// A power of two above Cauchy's bound `1 + max |c_i / c_n|`: every root is inside
    /// `(-b, b)`.
    fn bound(&self) -> Rat {
        let lead = self.lead().bit_len();
        let top = self
            .0
            .iter()
            .rev()
            .skip(1)
            .map(Int::bit_len)
            .max()
            .unwrap_or(0);
        // |c_i / c_n| < 2^(top - lead + 1), so the bound is below 2^(that + 1).
        Rat::one().scale(i64::try_from(top.saturating_sub(lead)).unwrap_or(0) + 2)
    }

    /// The roots of a square-free `p`, each alone in `(a, c]` with `p(c)` not zero, or
    /// met exactly at a bisection point.
    fn isolate(&self) -> Vec<Isolated> {
        let s = self.sturm();
        let changes = |x: &Rat| -> u32 {
            let signs: Vec<i32> = s.iter().map(|p| p.sign_at(x)).filter(|&g| g != 0).collect();
            signs.windows(2).filter(|w| w.first() != w.get(1)).count() as u32
        };
        let b = self.bound();
        let mut out = Vec::new();
        // (a, c] with its count and the counts at its ends.
        let lo = -&b;
        let (cl, ch) = (changes(&lo), changes(&b));
        let mut stack = vec![(lo, b, cl.saturating_sub(ch), cl, ch)];
        while let Some((a, c, n, va, vc)) = stack.pop() {
            match n {
                0 => {}
                1 => out.push(Isolated::new(self, a, c)),
                _ => {
                    let m = a.midpoint(&c);
                    let vm = changes(&m);
                    stack.push((a, m.clone(), va.saturating_sub(vm), va, vm));
                    stack.push((m, c, vm.saturating_sub(vc), vm, vc));
                }
            }
        }
        out
    }
}

/// One root of the square-free `f`, alone in `(lo, hi]`: `lo == hi` when it was met exactly,
/// otherwise `f(hi)` has the sign `high`, not zero, and the root is inside `(lo, hi)`.
struct Isolated {
    f: Z,
    lo: Rat,
    hi: Rat,
    high: i32,
    multiplicity: u32,
}

impl Isolated {
    fn new(f: &Z, lo: Rat, hi: Rat) -> Isolated {
        let high = f.sign_at(&hi);
        let lo = if high == 0 { hi.clone() } else { lo };
        Isolated {
            f: f.clone(),
            lo,
            hi,
            high,
            multiplicity: 0,
        }
    }

    fn is_exact(&self) -> bool {
        self.lo == self.hi
    }

    /// Whether `[lo, hi]`, closed, is certain to hold this root alone: an exact point, or
    /// `f` not zero at `lo` either (`lo` may be another root of `f`, met exactly).
    fn is_closed(&self) -> bool {
        self.is_exact() || self.f.sign_at(&self.lo) != 0
    }

    /// One bisection step on the sign.
    fn halve(&mut self) {
        if self.is_exact() {
            return;
        }
        let m = self.lo.midpoint(&self.hi);
        match self.f.sign_at(&m) {
            0 => (self.lo, self.hi) = (m.clone(), m),
            s if s == self.high => self.hi = m,
            _ => self.lo = m,
        }
    }
}

impl Poly {
    /// From coefficients, lowest first (trailing zeros dropped).
    pub fn new(mut c: Vec<Rat>) -> Poly {
        while c.last().is_some_and(Rat::is_zero) {
            c.pop();
        }
        Poly(c)
    }

    /// The constant `c`.
    pub fn constant(c: Rat) -> Poly {
        Poly::new(vec![c])
    }

    /// `a + b x`.
    pub fn linear(a: Rat, b: Rat) -> Poly {
        Poly::new(vec![a, b])
    }

    /// The coefficients, lowest first.
    pub fn coeffs(&self) -> &[Rat] {
        &self.0
    }

    /// Whether this is the zero polynomial.
    pub fn is_zero(&self) -> bool {
        self.0.is_empty()
    }

    /// The degree; `None` for the zero polynomial.
    pub fn degree(&self) -> Option<usize> {
        self.0.len().checked_sub(1)
    }

    fn lead(&self) -> Rat {
        self.0.last().cloned().unwrap_or_default()
    }

    /// The value at `x`, exactly (Horner).
    pub fn eval(&self, x: &Rat) -> Rat {
        self.0
            .iter()
            .rev()
            .fold(Rat::zero(), |acc, c| &(&acc * x) + c)
    }

    /// The sign of the value at `x`.
    pub fn sign_at(&self, x: &Rat) -> i32 {
        self.eval(x).signum()
    }

    /// The value at a double-double `x`, in double-double (the coefficients rounded to
    /// double-doubles first).
    pub fn eval_dd(&self, x: Dd) -> Dd {
        self.0
            .iter()
            .rev()
            .fold(Dd::new(0.0), |acc, c| acc * x + rat_to_dd(c))
    }

    /// The derivative.
    #[must_use]
    pub fn derivative(&self) -> Poly {
        Poly::new(
            self.0
                .iter()
                .enumerate()
                .skip(1)
                .map(|(i, c)| c * &Rat::from(i as i64))
                .collect(),
        )
    }

    /// `c p`.
    #[must_use]
    pub fn scale(&self, c: &Rat) -> Poly {
        Poly::new(self.0.iter().map(|x| x * c).collect())
    }

    /// `p + q`.
    #[must_use]
    pub fn add(&self, q: &Poly) -> Poly {
        let n = self.0.len().max(q.0.len());
        Poly::new(
            (0..n)
                .map(|i| match (self.0.get(i), q.0.get(i)) {
                    (Some(a), Some(b)) => a + b,
                    (Some(a), None) | (None, Some(a)) => a.clone(),
                    (None, None) => Rat::zero(),
                })
                .collect(),
        )
    }

    /// `p - q`.
    #[must_use]
    pub fn sub(&self, q: &Poly) -> Poly {
        self.add(&q.scale(&Rat::from(-1)))
    }

    /// `p q`.
    #[must_use]
    pub fn mul(&self, q: &Poly) -> Poly {
        if self.is_zero() || q.is_zero() {
            return Poly::default();
        }
        let mut c = vec![Rat::zero(); self.0.len() + q.0.len() - 1];
        for (i, a) in self.0.iter().enumerate() {
            for (j, b) in q.0.iter().enumerate() {
                if let Some(slot) = c.get_mut(i + j) {
                    *slot = &*slot + &(a * b);
                }
            }
        }
        Poly::new(c)
    }

    /// `p^k`.
    #[must_use]
    pub fn pow(&self, k: u32) -> Poly {
        (0..k).fold(Poly::constant(Rat::one()), |acc, _| acc.mul(self))
    }

    /// `(quotient, remainder)` of the division by `d`; `None` when `d` is zero.
    pub fn div_rem(&self, d: &Poly) -> Option<(Poly, Poly)> {
        let dn = d.degree()?;
        let inv = d.lead().recip()?;
        let mut r = self.0.clone();
        let mut q = vec![Rat::zero(); self.0.len().saturating_sub(dn)];
        while let Some(rn) = Poly::new(r.clone()).degree() {
            if rn < dn {
                break;
            }
            r.truncate(rn + 1);
            let f = &r.last().cloned().unwrap_or_default() * &inv;
            for (j, c) in d.0.iter().enumerate() {
                if let Some(slot) = r.get_mut(rn - dn + j) {
                    *slot = &*slot - &(&f * c);
                }
            }
            r.pop(); // cancelled exactly
            if let Some(slot) = q.get_mut(rn - dn) {
                *slot = f;
            }
        }
        Some((Poly::new(q), Poly::new(r)))
    }

    /// `p / d` for a `d` that divides `p` exactly (the quotient otherwise); zero for a
    /// zero `d`.
    #[must_use]
    pub fn div_exact(&self, d: &Poly) -> Poly {
        self.div_rem(d).map_or_else(Poly::default, |(q, _)| q)
    }

    /// `p` divided by its leading coefficient's magnitude: the same roots and signs.
    #[must_use]
    pub fn normalised(&self) -> Poly {
        match self.lead().abs().recip() {
            Some(s) => self.scale(&s),
            None => Poly::default(),
        }
    }

    /// The monic greatest common divisor (zero when both are zero).
    #[must_use]
    pub fn gcd(&self, o: &Poly) -> Poly {
        let (mut a, mut b) = (self.clone(), o.clone());
        while !b.is_zero() {
            let r = a.div_rem(&b).map_or_else(Poly::default, |(_, r)| r);
            a = b;
            b = r.normalised();
        }
        match a.lead().recip() {
            Some(s) => a.scale(&s),
            None => a,
        }
    }

    /// Yun's square-free factorisation: `(f_i, i)` with `p = c prod f_i^i`, each `f_i`
    /// monic, square-free, of positive degree, and coprime to the others. Empty for a
    /// constant (or zero) polynomial.
    pub fn square_free(&self) -> Vec<(Poly, u32)> {
        let mut out = Vec::new();
        if self.degree().unwrap_or(0) == 0 {
            return out;
        }
        let d = self.derivative();
        let a = self.gcd(&d);
        let mut b = self.div_exact(&a);
        let mut c = d.div_exact(&a);
        let mut i = 1;
        loop {
            let dd = c.sub(&b.derivative());
            let f = b.gcd(&dd);
            if f.degree().unwrap_or(0) > 0 {
                out.push((f.clone(), i));
            }
            b = b.div_exact(&f);
            if b.degree().unwrap_or(0) == 0 {
                return out;
            }
            c = dd.div_exact(&f);
            i += 1;
        }
    }

    /// Every real root, ascending, each enclosed in an interval of width at most `width` (or
    /// an exact point) holding no other root, the intervals pairwise disjoint, with its
    /// multiplicity. Empty for a constant, and for the zero polynomial (every point is a
    /// root: the caller tells that case apart).
    pub fn real_roots(&self, width: &Rat) -> Vec<Root> {
        let mut found: Vec<Isolated> = Vec::new();
        for (f, m) in Z::of(self).square_free() {
            found.extend(f.isolate().into_iter().map(|mut r| {
                r.multiplicity = m;
                r
            }));
        }
        for r in &mut found {
            while !r.is_exact() && (&r.hi - &r.lo > *width || !r.is_closed()) {
                r.halve();
            }
        }
        found.sort_by(|a, b| a.lo.cmp(&b.lo));
        // Roots of different factors are distinct: narrow any two that meet.
        loop {
            let mut moved = false;
            for i in 1..found.len() {
                let meet = matches!((found.get(i - 1), found.get(i)),
                    (Some(a), Some(b)) if a.hi >= b.lo);
                if meet {
                    for j in [i - 1, i] {
                        if let Some(r) = found.get_mut(j) {
                            r.halve();
                        }
                    }
                    moved = true;
                }
            }
            if !moved {
                break;
            }
            found.sort_by(|a, b| a.lo.cmp(&b.lo));
        }
        found
            .into_iter()
            .filter_map(|r| {
                Some(Root {
                    at: Bound::new(r.lo, r.hi)?,
                    multiplicity: r.multiplicity,
                })
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(c: &[i64]) -> Poly {
        Poly::new(c.iter().map(|&x| Rat::from(x)).collect())
    }

    fn width() -> Rat {
        Rat::one().scale(-60)
    }

    #[test]
    fn roots_of_known_polynomials() {
        // (x - 1)^2 (x + 2) (x^2 - 2) = roots 1 (twice), -2, +-sqrt 2.
        let g = p(&[1, -2, 1]); // (x-1)^2
        let h = p(&[2, 1]); // x + 2
        let k = p(&[-2, 0, 1]); // x^2 - 2
        let f2 = g.mul(&h).mul(&k);
        let roots = f2.real_roots(&width());
        assert_eq!(roots.len(), 4);
        assert_eq!(roots[0].at, Bound::exact(Rat::from(-2)));
        assert_eq!(roots[1].multiplicity, 1);
        assert!(roots[1].at.hi() < &Rat::from(-1));
        assert_eq!(roots[2].at, Bound::exact(Rat::from(1)));
        assert_eq!(roots[2].multiplicity, 2);
        assert!(roots[3].at.width() <= width());
        // the enclosure holds sqrt 2: lo^2 < 2 < hi^2
        assert!(&roots[3].at.lo().clone() * roots[3].at.lo() < Rat::from(2));
        assert!(&roots[3].at.hi().clone() * roots[3].at.hi() > Rat::from(2));
    }

    #[test]
    fn no_real_roots_and_constants() {
        assert!(p(&[1, 0, 1]).real_roots(&width()).is_empty());
        assert!(p(&[3]).real_roots(&width()).is_empty());
        assert!(Poly::new(vec![]).real_roots(&width()).is_empty());
        let r = p(&[0, 0, 0, 1]).real_roots(&width()); // x^3
        assert_eq!(r.len(), 1);
        assert_eq!(r[0].multiplicity, 3);
        assert_eq!(r[0].at, Bound::exact(Rat::zero()));
    }

    #[test]
    fn close_roots_separate() {
        // (x - 1)(x - 1 - 2^-80): two simple roots 2^-80 apart.
        let e = Rat::one().scale(-80);
        let a = Rat::one();
        let b = &a + &e;
        let f = Poly::new(vec![&a * &b, -(&a + &b), Rat::one()]);
        let r = f.real_roots(&Rat::one().scale(-100));
        assert_eq!(r.len(), 2);
        assert!(r[0].at.below(&r[1].at));
        assert!(r[0].at.contains(&a) && r[1].at.contains(&b));
    }
}
