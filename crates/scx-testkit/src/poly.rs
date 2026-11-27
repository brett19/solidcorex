// Written for solidcorex. Apache-2.0.
//! Exact polynomials over the rationals, and every real root of one, certified: each root
//! is held in a rational interval that contains it and no other root, with its
//! multiplicity.
//!
//! The method is the textbook one, exact throughout so nothing is a judgement:
//!
//! - Yun's square-free factorisation `p = c f_1 f_2^2 f_3^3 ...` (each `f_i` square-free,
//!   pairwise coprime) gives the multiplicities, by exact `gcd`s;
//! - a Sturm sequence of each `f_i` counts its roots in any half-open interval `(a, b]`, so
//!   bisection from Cauchy's bound isolates them;
//! - an isolated root of a square-free factor changes its sign, so bisection on the sign
//!   alone refines it, to `2^-110` relative (a root met exactly at a bisection point is
//!   reported as a point interval).
//!
//! Slow and plain: Sturm sequences over the rationals grow their coefficients, which is
//! fine for the degrees here (at most eight) and the numbers of a few doubles each.

use crate::dd::Dd;
use crate::exact::Int;
use crate::rat::Rat;

/// A polynomial `sum c_i x^i` with rational coefficients, lowest first, with no trailing
/// zero (the zero polynomial has none).
#[derive(Clone, Debug, PartialEq, Eq, Hash, Default)]
pub struct Poly(Vec<Rat>);

/// A real root: it lies in `[lo, hi]`, which holds no other root; `lo == hi` when it was
/// met exactly.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Root {
    /// The interval's lower end.
    pub lo: Rat,
    /// The interval's upper end.
    pub hi: Rat,
    /// How many times the polynomial has it.
    pub multiplicity: u32,
}

impl Root {
    /// The midpoint, as a double-double (within `2^-104` relative of the root's interval).
    pub fn value(&self) -> Dd {
        to_dd(&(&self.lo + &self.hi).scale(-1))
    }

    /// Half the interval's width: a bound on `|root - value()|`, up to the rounding of
    /// [`Root::value`] itself.
    pub fn radius(&self) -> f64 {
        (&self.hi - &self.lo).scale(-1).to_f64()
    }
}

/// `r` as a double-double: the nearest double and the nearest double to the rest.
pub fn to_dd(r: &Rat) -> Dd {
    let hi = r.to_f64();
    let lo = Rat::from_f64(hi).map_or(0.0, |h| (r - &h).to_f64());
    Dd { hi, lo }
}

/// A double-double exactly, as a rational; `None` when not finite.
pub fn of_dd(x: Dd) -> Option<Rat> {
    Some(Rat::from_f64(x.hi)? + Rat::from_f64(x.lo)?)
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
        if a.lead().is_negative() {
            a.neg()
        } else {
            a
        }
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

    /// The roots of a square-free `p`, each in an isolating interval refined to the width.
    fn isolate(&self) -> Vec<(Rat, Rat)> {
        let s = self.sturm();
        let changes = |x: &Rat| -> u32 {
            let signs: Vec<i32> = s.iter().map(|p| p.sign_at(x)).filter(|&g| g != 0).collect();
            signs.windows(2).filter(|w| w.first() != w.get(1)).count() as u32
        };
        let b = self.bound();
        let mut out = Vec::new();
        // (a, b] with its count.
        let lo = -&b;
        let (cl, ch) = (changes(&lo), changes(&b));
        let mut stack = vec![(lo, b, cl.saturating_sub(ch), cl, ch)];
        while let Some((a, c, n, va, vc)) = stack.pop() {
            match n {
                0 => {}
                1 => out.push(self.refine(a, c)),
                _ => {
                    let m = (&a + &c).scale(-1);
                    let vm = changes(&m);
                    stack.push((a, m.clone(), va.saturating_sub(vm), va, vm));
                    stack.push((m, c, vm.saturating_sub(vc), vm, vc));
                }
            }
        }
        out
    }

    /// The one root of a square-free `p` in `(a, c]`, refined by the sign.
    fn refine(&self, mut a: Rat, mut c: Rat) -> (Rat, Rat) {
        let sc = self.sign_at(&c);
        if sc == 0 {
            return (c.clone(), c);
        }
        for _ in 0..4000 {
            let width = &c - &a;
            let size = if a.abs() > c.abs() { a.abs() } else { c.abs() };
            if width <= size.scale(-110) || width <= Rat::one().scale(-1080) {
                break;
            }
            let m = (&a + &c).scale(-1);
            let sm = self.sign_at(&m);
            if sm == 0 {
                return (m.clone(), m);
            }
            if sm == sc {
                c = m;
            } else {
                a = m;
            }
        }
        (a, c)
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

    /// The value at a double-double `x`, in double-double (the coefficients rounded to
    /// double-doubles first).
    pub fn eval_dd(&self, x: Dd) -> Dd {
        self.0
            .iter()
            .rev()
            .fold(Dd::new(0.0), |acc, c| acc * x + to_dd(c))
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
        self.div_rem(d).map(|(q, _)| q).unwrap_or_default()
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
            let r = a.div_rem(&b).map(|(_, r)| r).unwrap_or_default();
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

    /// Every real root, ascending, each isolated and refined to `2^-110` relative of its
    /// magnitude (or met exactly), with its multiplicity. Empty for the zero polynomial
    /// (every point is a root: the caller tells that case apart) and for a constant.
    pub fn real_roots(&self) -> Vec<Root> {
        let mut roots: Vec<Root> = Z::of(self)
            .square_free()
            .into_iter()
            .flat_map(|(f, m)| {
                f.isolate().into_iter().map(move |(lo, hi)| Root {
                    lo,
                    hi,
                    multiplicity: m,
                })
            })
            .collect();
        roots.sort_by(|a, b| a.lo.cmp(&b.lo));
        roots
    }
}
