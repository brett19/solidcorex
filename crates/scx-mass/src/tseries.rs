// Written for solidcorex. Apache-2.0.
//! [`Ser`]: a truncated Taylor series in two variables `(dt, ds)` over [`Ivl`], the ring the
//! volume enclosure evaluates a surface patch in (research 72 §14.16).
//!
//! - **Degree.** A series holds the coefficients of `dt^i ds^j` for `i + j <= n`, `n` at
//!   most [`MAX`]; a sum or product keeps the lower degree of its operands, a lifted
//!   constant has degree [`MAX`] (it is exact at every degree).
//! - **Two readings.** Seeded at a point (the constant term a point interval), the
//!   coefficients enclose the Taylor coefficients there. Seeded over a box (the constant
//!   term the box's interval), they enclose the Taylor coefficients at every point of the
//!   box: the Lagrange remainder's coefficients.
//! - **Exact zeros.** A coefficient that is exactly zero stays zero: interval rounding would
//!   otherwise widen it to a few subnormals and every product after it.

use core::ops::{Add, Div, Mul, Sub};

use scx_num::interval::Ivl;
use scx_num::ring::Ring;

/// The highest degree a series holds.
pub(crate) const MAX: usize = 9;
/// The coefficients of a series of degree [`MAX`].
const LEN: usize = (MAX + 1) * (MAX + 2) / 2;

/// The coefficients of a series of degree `n`.
pub(crate) const fn len(n: usize) -> usize {
    (n + 1) * (n + 2) / 2
}

/// The index of `dt^i ds^j`: total degree first, then the power of `ds`.
pub(crate) const fn index(i: usize, j: usize) -> usize {
    let d = i + j;
    d * (d + 1) / 2 + j
}

/// The powers `(i, j)` of each index below `len(n)`, in index order.
fn powers(n: usize) -> impl Iterator<Item = (usize, usize)> {
    (0..=n).flat_map(|d| (0..=d).map(move |j| (d - j, j)))
}

const ZERO: Ivl = Ivl::new(0.0, 0.0);

fn is_zero(x: Ivl) -> bool {
    x.lo == 0.0 && x.hi == 0.0
}

fn neg(x: Ivl) -> Ivl {
    Ivl::new(-x.hi, -x.lo)
}

/// `a + b`, exact when either is exactly zero.
fn plus(a: Ivl, b: Ivl) -> Ivl {
    if is_zero(a) {
        b
    } else if is_zero(b) {
        a
    } else {
        a + b
    }
}

/// `a * b`, exactly zero when either is.
fn times(a: Ivl, b: Ivl) -> Ivl {
    if is_zero(a) || is_zero(b) {
        ZERO
    } else {
        a * b
    }
}

/// A truncated series in `(dt, ds)` over [`Ivl`].
#[derive(Debug, Clone, Copy)]
pub(crate) struct Ser {
    n: usize,
    c: [Ivl; LEN],
}

impl Ser {
    /// The constant `x` at degree `n`.
    pub(crate) fn constant(x: Ivl, n: usize) -> Self {
        let mut c = [ZERO; LEN];
        c[0] = x;
        Self { n: n.min(MAX), c }
    }

    /// `x + dt` at degree `n`.
    pub(crate) fn var_t(x: Ivl, n: usize) -> Self {
        let mut s = Self::constant(x, n);
        if s.n >= 1 {
            s.set(1, 0, Ivl::from(1.0));
        }
        s
    }

    /// `x + ds` at degree `n`.
    pub(crate) fn var_s(x: Ivl, n: usize) -> Self {
        let mut s = Self::constant(x, n);
        if s.n >= 1 {
            s.set(0, 1, Ivl::from(1.0));
        }
        s
    }

    /// `Σ_k coeffs[k] dt^k` at degree `n`.
    pub(crate) fn in_t(coeffs: &[Ivl], n: usize) -> Self {
        let mut s = Self::constant(ZERO, n);
        for (k, &x) in coeffs.iter().enumerate().take(s.n + 1) {
            s.set(k, 0, x);
        }
        s
    }

    /// Sets the coefficient of `dt^i ds^j`, at or below the degree.
    fn set(&mut self, i: usize, j: usize, x: Ivl) {
        if i + j <= self.n
            && let Some(c) = self.c.get_mut(index(i, j))
        {
            *c = x;
        }
    }

    /// The degree.
    #[cfg(test)]
    pub(crate) const fn degree(&self) -> usize {
        self.n
    }

    /// The coefficient of `dt^i ds^j`; zero past the degree.
    pub(crate) fn at(&self, i: usize, j: usize) -> Ivl {
        if i + j > self.n {
            return ZERO;
        }
        self.c.get(index(i, j)).copied().unwrap_or(ZERO)
    }

    /// The same series cut to degree `n`.
    pub(crate) fn cut(mut self, n: usize) -> Self {
        let n = n.min(self.n);
        for x in self.c.iter_mut().skip(len(n)) {
            *x = ZERO;
        }
        self.n = n;
        self
    }

    /// `d/dt`, one degree lower (degree 0 stays 0: nothing is known of its derivative, and
    /// no caller asks).
    pub(crate) fn deriv_t(&self) -> Self {
        let n = self.n.saturating_sub(1);
        let mut out = Self::constant(ZERO, n);
        for (i, j) in powers(n) {
            let x = self.at(i + 1, j);
            if !is_zero(x) {
                out.set(i, j, x * Ivl::from((i + 1) as f64));
            }
        }
        out
    }

    /// `d/ds`, one degree lower.
    pub(crate) fn deriv_s(&self) -> Self {
        let n = self.n.saturating_sub(1);
        let mut out = Self::constant(ZERO, n);
        for (i, j) in powers(n) {
            let x = self.at(i, j + 1);
            if !is_zero(x) {
                out.set(i, j, x * Ivl::from((j + 1) as f64));
            }
        }
        out
    }

    /// `1 / self`: `r0 = 1 / b0`, `r_a = -r0 Σ_{0 < g <= a} b_g r_{a - g}`.
    fn recip(&self) -> Self {
        let n = self.n;
        let mut r = Self::constant(Ivl::from(1.0) / self.c[0], n);
        let r0 = r.c[0];
        for (i, j) in powers(n).skip(1) {
            let mut sum = ZERO;
            for gi in 0..=i {
                for gj in 0..=j {
                    if gi + gj == 0 {
                        continue;
                    }
                    sum = plus(sum, times(self.at(gi, gj), r.at(i - gi, j - gj)));
                }
            }
            r.set(i, j, neg(times(r0, sum)));
        }
        r
    }
}

impl Add for Ser {
    type Output = Self;
    fn add(self, b: Self) -> Self {
        let n = self.n.min(b.n);
        let mut out = self.cut(n);
        for (o, &y) in out.c.iter_mut().zip(&b.c).take(len(n)) {
            *o = plus(*o, y);
        }
        out
    }
}

impl Sub for Ser {
    type Output = Self;
    fn sub(self, b: Self) -> Self {
        let n = self.n.min(b.n);
        let mut out = self.cut(n);
        for (o, &y) in out.c.iter_mut().zip(&b.c).take(len(n)) {
            *o = if is_zero(y) {
                *o
            } else if is_zero(*o) {
                neg(y)
            } else {
                *o - y
            };
        }
        out
    }
}

impl Mul for Ser {
    type Output = Self;
    fn mul(self, b: Self) -> Self {
        let n = self.n.min(b.n);
        let mut out = Self::constant(ZERO, n);
        for (ai, aj) in powers(n) {
            let x = self.at(ai, aj);
            if is_zero(x) {
                continue;
            }
            for (bi, bj) in powers(n - ai - aj) {
                let y = b.at(bi, bj);
                if is_zero(y) {
                    continue;
                }
                let k = index(ai + bi, aj + bj);
                if let Some(o) = out.c.get_mut(k) {
                    *o = plus(*o, x * y);
                }
            }
        }
        out
    }
}

impl Div for Ser {
    type Output = Self;
    #[allow(
        clippy::suspicious_arithmetic_impl,
        reason = "a quotient is the product by the reciprocal"
    )]
    fn div(self, b: Self) -> Self {
        self * b.recip()
    }
}

impl Ring for Ser {
    fn lift(d: f64) -> Self {
        Self::constant(Ivl::from(d), MAX)
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::float_cmp)]

    use super::*;

    fn point(x: f64) -> Ivl {
        Ivl::from(x)
    }

    fn holds(x: Ivl, v: f64) -> bool {
        x.lo <= v && v <= x.hi
    }

    /// The pin's `series_multiply_and_differentiate_as_polynomials`: `(1 + 2 dt + ds)(3 - ds)`
    /// is `3 + 6 dt + 2 ds - 2 dt ds - ds^2`; its `d/dt` is `6 - 2 ds`, its `d/ds`
    /// `2 - 2 dt - 2 ds`; and `(1 + dt)^-1` is `Σ (-dt)^k`.
    #[test]
    fn series_multiply_and_differentiate_as_polynomials() {
        let a = Ser::var_t(point(0.0), 4) * Ser::lift(2.0) + Ser::var_s(point(1.0), 4);
        let b = Ser::lift(3.0) - Ser::var_s(point(0.0), 4);
        let p = a * b;
        assert_eq!(p.degree(), 4);
        for (i, j, want) in [
            (0, 0, 3.0),
            (1, 0, 6.0),
            (0, 1, 2.0),
            (1, 1, -2.0),
            (0, 2, -1.0),
            (2, 0, 0.0),
            (3, 1, 0.0),
        ] {
            assert!(holds(p.at(i, j), want), "{i} {j}: {:?}", p.at(i, j));
        }
        let pt = p.deriv_t();
        assert!(holds(pt.at(0, 0), 6.0) && holds(pt.at(0, 1), -2.0) && pt.degree() == 3);
        let ps = p.deriv_s();
        assert!(holds(ps.at(0, 0), 2.0) && holds(ps.at(1, 0), -2.0) && holds(ps.at(0, 1), -2.0));
        let r = Ser::lift(1.0) / Ser::var_t(point(1.0), 6);
        for k in 0..=6 {
            let want = if k % 2 == 0 { 1.0 } else { -1.0 };
            assert!(holds(r.at(k, 0), want), "{k}: {:?}", r.at(k, 0));
        }
        // the exact zeros stay exact: a product of t-only series has no ds terms at all
        let t = Ser::var_t(point(0.5), 5);
        let q = t * t * t;
        assert_eq!(q.at(0, 1), Ivl::new(0.0, 0.0));
        assert_eq!(q.at(2, 1), Ivl::new(0.0, 0.0));
    }

    /// Over a box the coefficients enclose the Taylor coefficients at each of its points.
    #[test]
    fn a_box_series_encloses_every_points_coefficients() {
        let bx = Ser::var_t(Ivl::new(0.25, 0.75), 4);
        let cube = bx * bx * bx;
        for x in [0.25, 0.4, 0.75] {
            let p = Ser::var_t(point(x), 4);
            let c = p * p * p;
            for k in 0..=4 {
                let w = c.at(k, 0).mid();
                assert!(holds(cube.at(k, 0), w), "{x} {k}");
            }
        }
    }
}
