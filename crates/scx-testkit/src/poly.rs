// Written for solidcorex. Apache-2.0.
//! Polynomials with exact rational coefficients, and their real roots isolated with
//! certainty: square-free factors by Yun's algorithm, then Sturm sequences and bisection.
//!
//! Every real root comes back once, as an interval sure to hold it and no other root, with
//! its multiplicity; a root that is a bisection point comes back as that exact point.

use crate::bound::Bound;
use crate::exact::Int;
use crate::rat::Rat;

/// `c[0] + c[1] x + ... + c[n] x^n`, with `c[n]` not zero (the zero polynomial is empty).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Poly {
    c: Vec<Rat>,
}

/// A real root: an interval holding it and no other root, and how many times it repeats.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Root {
    /// The enclosure; a single point when the root is that point exactly.
    pub at: Bound,
    /// Its multiplicity, at least one.
    pub multiplicity: u32,
}

impl Poly {
    /// From the coefficients, lowest degree first; trailing zeros are dropped.
    pub fn new(mut c: Vec<Rat>) -> Poly {
        while c.last().is_some_and(Rat::is_zero) {
            c.pop();
        }
        Poly { c }
    }

    /// The coefficients, lowest degree first.
    pub fn coeffs(&self) -> &[Rat] {
        &self.c
    }

    /// The degree; `None` for the zero polynomial.
    pub fn degree(&self) -> Option<usize> {
        self.c.len().checked_sub(1)
    }

    /// Whether this is the zero polynomial.
    pub fn is_zero(&self) -> bool {
        self.c.is_empty()
    }

    /// The value at `x`, exactly (Horner).
    pub fn eval(&self, x: &Rat) -> Rat {
        self.c
            .iter()
            .rev()
            .fold(Rat::zero(), |acc, c| &(&acc * x) + c)
    }

    /// The sign of the value at `x`.
    pub fn sign_at(&self, x: &Rat) -> i32 {
        self.eval(x).signum()
    }

    /// The derivative.
    #[must_use]
    pub fn derivative(&self) -> Poly {
        Poly::new(
            self.c
                .iter()
                .enumerate()
                .skip(1)
                .map(|(i, c)| c * &Rat::from(i as i64))
                .collect(),
        )
    }

    /// `self * k` for a rational `k`.
    #[must_use]
    pub fn scaled(&self, k: &Rat) -> Poly {
        Poly::new(self.c.iter().map(|c| c * k).collect())
    }

    /// `self - o`.
    #[must_use]
    pub fn minus(&self, o: &Poly) -> Poly {
        let n = self.c.len().max(o.c.len());
        Poly::new(
            (0..n)
                .map(|i| {
                    let a = self.c.get(i).cloned().unwrap_or_default();
                    let b = o.c.get(i).cloned().unwrap_or_default();
                    a - b
                })
                .collect(),
        )
    }

    /// `(q, r)` with `self = q d + r` and `deg r < deg d`; `None` when `d` is zero.
    pub fn div_rem(&self, d: &Poly) -> Option<(Poly, Poly)> {
        let lead = d.c.last()?.recip()?;
        let dn = d.c.len() - 1;
        let mut r = self.c.clone();
        let mut q = vec![Rat::zero(); r.len().saturating_sub(dn)];
        while r.len() > dn && !r.is_empty() {
            let top = r.len() - 1;
            let k = r.last().map_or_default(|t| t * &lead);
            let shift = top - dn;
            for (j, dc) in d.c.iter().enumerate() {
                if let Some(rj) = r.get_mut(shift + j) {
                    *rj = &*rj - &(&k * dc);
                }
            }
            if let Some(qs) = q.get_mut(shift) {
                *qs = k;
            }
            r.pop(); // its leading coefficient is now zero exactly
            while r.last().is_some_and(Rat::is_zero) {
                r.pop();
            }
        }
        Some((Poly::new(q), Poly::new(r)))
    }

    /// The monic greatest common divisor (zero when both are zero).
    #[must_use]
    pub fn gcd(&self, o: &Poly) -> Poly {
        let (mut a, mut b) = (self.clone(), o.clone());
        while !b.is_zero() {
            let Some((_, r)) = a.div_rem(&b) else { break };
            (a, b) = (b, r);
        }
        a.monic()
    }

    /// Divided by its leading coefficient (zero stays zero).
    #[must_use]
    pub fn monic(&self) -> Poly {
        match self.c.last().and_then(Rat::recip) {
            Some(k) => self.scaled(&k),
            None => self.clone(),
        }
    }

    /// A positive multiple with integer coefficients sharing no factor: the same signs
    /// everywhere, with short numbers.
    #[must_use]
    pub fn primitive(&self) -> Poly {
        let Some(e) = self
            .c
            .iter()
            .filter(|c| !c.is_zero())
            .map(|c| c.parts().2)
            .min()
        else {
            return self.clone();
        };
        let mut lcm = Int::from(1u64);
        for c in &self.c {
            let d = c.parts().1;
            let g = lcm.gcd(d);
            lcm = lcm.mul(&d.div_exact(&g));
        }
        let ints: Vec<Rat> = self
            .c
            .iter()
            .map(|c| c * &Rat::from_int(&lcm, -e))
            .collect();
        let mut content = Int::default();
        for c in &ints {
            content = content.gcd(c.parts().0);
        }
        match Rat::from_int(&content, 0).recip() {
            Some(k) => Poly::new(ints.iter().map(|c| c * &k).collect()),
            None => Poly::new(ints),
        }
    }

    /// The square-free factors with their multiplicities (Yun): `self` is a constant times
    /// the product of `f^m`, the factors pairwise coprime and none constant.
    pub fn square_free(&self) -> Vec<(Poly, u32)> {
        let mut out = Vec::new();
        if self.degree().unwrap_or(0) == 0 {
            return out;
        }
        let d = self.derivative();
        let a = self.gcd(&d);
        let (Some((mut b, _)), Some((c, _))) = (self.div_rem(&a), d.div_rem(&a)) else {
            return out;
        };
        let mut dd = c.minus(&b.derivative());
        let mut m = 1;
        while b.degree().unwrap_or(0) > 0 {
            let a = b.gcd(&dd);
            let (Some((nb, _)), Some((c, _))) = (b.div_rem(&a), dd.div_rem(&a)) else {
                break;
            };
            if a.degree().unwrap_or(0) > 0 {
                out.push((a, m));
            }
            dd = c.minus(&nb.derivative());
            b = nb;
            m += 1;
        }
        out
    }

    /// The Sturm sequence of a square-free `self`, each term made primitive (a positive
    /// multiple, so the sign changes are the same).
    fn sturm(&self) -> Vec<Poly> {
        let mut seq = vec![self.primitive(), self.derivative().primitive()];
        while let (Some(a), Some(b)) = (seq.get(seq.len() - 2), seq.last()) {
            if b.degree().unwrap_or(0) == 0 {
                break;
            }
            let Some((_, r)) = a.div_rem(b) else { break };
            if r.is_zero() {
                break;
            }
            seq.push(r.scaled(&Rat::from(-1)).primitive());
        }
        seq
    }

    /// Every real root, each enclosed in an interval of width at most `width` (or an exact
    /// point), sorted, the intervals pairwise disjoint. Empty for the zero polynomial.
    pub fn real_roots(&self, width: &Rat) -> Vec<Root> {
        let mut found: Vec<Isolated> = Vec::new();
        for (f, m) in self.square_free() {
            let f = f.primitive();
            let seq = f.sturm();
            for (lo, hi) in isolate(&f, &seq) {
                found.push(Isolated {
                    f: f.clone(),
                    lo,
                    hi,
                    multiplicity: m,
                });
            }
        }
        for r in &mut found {
            while !r.is_exact() && &r.hi - &r.lo > *width {
                r.halve();
            }
        }
        found.sort_by(|a, b| a.lo.cmp(&b.lo));
        // Roots of different factors are distinct: narrow any two that overlap.
        loop {
            let mut moved = false;
            for i in 1..found.len() {
                let overlap = matches!((found.get(i - 1), found.get(i)),
                    (Some(a), Some(b)) if a.hi >= b.lo);
                if overlap {
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

/// A root of the square-free `f` alone in `[lo, hi]`: `lo == hi` exactly, or the open
/// interval with `f` of opposite non-zero signs at the ends.
struct Isolated {
    f: Poly,
    lo: Rat,
    hi: Rat,
    multiplicity: u32,
}

impl Isolated {
    fn is_exact(&self) -> bool {
        self.lo == self.hi
    }

    /// One bisection step.
    fn halve(&mut self) {
        if self.is_exact() {
            return;
        }
        let m = self.lo.midpoint(&self.hi);
        let s = self.f.sign_at(&m);
        if s == 0 {
            (self.lo, self.hi) = (m.clone(), m);
        } else if s == self.f.sign_at(&self.lo) {
            self.lo = m;
        } else {
            self.hi = m;
        }
    }
}

/// Sign changes of the sequence at `x`, zeros skipped.
fn changes(seq: &[Poly], x: &Rat) -> u32 {
    let mut last = 0;
    let mut n = 0;
    for p in seq {
        let s = p.sign_at(x);
        if s != 0 {
            if last != 0 && s != last {
                n += 1;
            }
            last = s;
        }
    }
    n
}

/// A power of two above every root's magnitude (Cauchy: `1 + max |c_i / c_n|`).
fn root_bound(f: &Poly) -> Rat {
    let Some(lead) = f.c.last().and_then(Rat::recip) else {
        return Rat::one();
    };
    let big =
        f.c.iter()
            .map(|c| (c * &lead).abs())
            .max()
            .unwrap_or_default();
    let l = (&big + &Rat::one()).log2_approx().unwrap_or(0);
    Rat::one().scale(l + 1)
}

/// Each root of the square-free `f` in its own interval: `(m, m)` for an exact root `m`,
/// otherwise `(a, b)` with one root inside and `f(a) f(b) < 0`.
fn isolate(f: &Poly, seq: &[Poly]) -> Vec<(Rat, Rat)> {
    let b = root_bound(f);
    let a = -&b;
    let mut out = Vec::new();
    let mut work = vec![(changes(seq, &a), a, changes(seq, &b), b)];
    while let Some((va, a, vb, b)) = work.pop() {
        let n = va.saturating_sub(vb);
        if n == 0 {
            continue;
        }
        if n == 1 {
            out.push((a, b));
            continue;
        }
        let m = a.midpoint(&b);
        if f.sign_at(&m) != 0 {
            let vm = changes(seq, &m);
            work.push((va, a, vm, m.clone()));
            work.push((vm, m, vb, b));
            continue;
        }
        // An exact root: step off it both ways until the ends are not roots and only it lies
        // between them.
        let mut delta = (&b - &a).scale(-2);
        loop {
            let (l, r) = (&m - &delta, &m + &delta);
            if f.sign_at(&l) != 0 && f.sign_at(&r) != 0 {
                let (vl, vr) = (changes(seq, &l), changes(seq, &r));
                if vl.saturating_sub(vr) == 1 {
                    out.push((m.clone(), m.clone()));
                    work.push((va, a.clone(), vl, l));
                    work.push((vr, r, vb, b.clone()));
                    break;
                }
            }
            delta = delta.scale(-1);
        }
    }
    out
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
        let prod = |a: &Poly, b: &Poly| {
            let mut c = vec![Rat::zero(); a.c.len() + b.c.len() - 1];
            for (i, x) in a.c.iter().enumerate() {
                for (j, y) in b.c.iter().enumerate() {
                    c[i + j] = &c[i + j] + &(x * y);
                }
            }
            Poly::new(c)
        };
        let f2 = prod(&prod(&g, &h), &k);
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
