// Written for solidcorex. Apache-2.0.
//! The certified arithmetic the decisions use: intervals rounded one ulp outward
//! ([`Ivl`]'s `*_ulp` operations), verdicts from them, and the exact sign of a 2-D
//! orientation determinant.
//!
//! Every comparison answers [`Verdict::Yes`] only when it holds for every value in the
//! intervals, so a decision built from them holds for the exact inputs; a non-finite
//! interval decides nothing.

use core::ops::{Add, Mul, Neg, Sub};

use scx_num::certified::Verdict;
use scx_num::eft::{two_prod, two_sum};
use scx_num::interval::Ivl;
use scx_num::vector2d::Point2;

/// An interval with the one-ulp outward operations.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Iv(pub(crate) Ivl);

impl Iv {
    pub(crate) fn exact(x: f64) -> Self {
        Self(Ivl::from(x))
    }

    pub(crate) const fn hi(self) -> f64 {
        self.0.hi
    }

    /// `self^2`, never below 0.
    pub(crate) fn sq(self) -> Self {
        let p = self.0.mul_ulp(self.0);
        if self.0.lo <= 0.0 && self.0.hi >= 0.0 {
            Self(Ivl::new(0.0, p.hi))
        } else {
            Self(p)
        }
    }

    /// The square roots of its non-negative part.
    pub(crate) fn sqrt(self) -> Self {
        Self(self.0.sqrt())
    }

    /// The quotient, the whole line unless the divisor excludes 0.
    pub(crate) fn div(self, b: Self) -> Self {
        Self(self.0.div_ulp(b.0))
    }

    pub(crate) fn hull(self, b: Self) -> Self {
        Self(self.0.hull(b.0))
    }

    /// Every value is above 0.
    pub(crate) fn pos(self) -> Verdict {
        greater(self, Self::exact(0.0))
    }

    /// Every value is below 0.
    pub(crate) fn neg_sign(self) -> Verdict {
        greater(Self::exact(0.0), self)
    }
}

impl Add for Iv {
    type Output = Self;
    fn add(self, b: Self) -> Self {
        Self(exact_sum(self.0, b.0).unwrap_or_else(|| self.0.add_ulp(b.0)))
    }
}

impl Sub for Iv {
    type Output = Self;
    fn sub(self, b: Self) -> Self {
        Self(exact_sum(self.0, Ivl::new(-b.0.hi, -b.0.lo)).unwrap_or_else(|| self.0.sub_ulp(b.0)))
    }
}

impl Mul for Iv {
    type Output = Self;
    fn mul(self, b: Self) -> Self {
        Self(exact_product(self.0, b.0).unwrap_or_else(|| self.0.mul_ulp(b.0)))
    }
}

impl Neg for Iv {
    type Output = Self;
    fn neg(self) -> Self {
        Self(Ivl::new(-self.0.hi, -self.0.lo))
    }
}

/// The point of a point interval.
fn point(a: Ivl) -> Option<f64> {
    (a.lo == a.hi).then_some(a.lo)
}

/// The sum of two point intervals as a point when it is exact (no rounding error), so
/// exact values and zeros stay exact; `None` otherwise.
pub(crate) fn exact_sum(a: Ivl, b: Ivl) -> Option<Ivl> {
    let (s, e) = two_sum(point(a)?, point(b)?);
    (s.is_finite() && e == 0.0).then(|| Ivl::from(s))
}

/// The product of two point intervals as a point when it is exact; `None` otherwise or
/// near underflow, where the error-free split is not exact.
pub(crate) fn exact_product(a: Ivl, b: Ivl) -> Option<Ivl> {
    let (x, y) = (point(a)?, point(b)?);
    if x == 0.0 || y == 0.0 {
        return Some(Ivl::from(x * y));
    }
    let (p, e) = two_prod(x, y);
    (p.is_finite() && p.abs() >= TINY && e == 0.0).then(|| Ivl::from(p))
}

/// Below this, products are not trusted exact (2^-900, far above the subnormal range).
const TINY: f64 = f64::from_bits(0x07B0_0000_0000_0000);

/// `a > b` for every pair of values: `Yes`; `a <= b` for every pair: `No`.
pub(crate) fn greater(a: Iv, b: Iv) -> Verdict {
    if !a.0.is_finite() || !b.0.is_finite() {
        Verdict::Undecided
    } else if a.0.lo > b.0.hi {
        Verdict::Yes
    } else if a.0.hi <= b.0.lo {
        Verdict::No
    } else {
        Verdict::Undecided
    }
}

/// Either.
pub(crate) const fn or(a: Verdict, b: Verdict) -> Verdict {
    match (a, b) {
        (Verdict::Yes, _) | (_, Verdict::Yes) => Verdict::Yes,
        (Verdict::No, Verdict::No) => Verdict::No,
        _ => Verdict::Undecided,
    }
}

/// The opposite.
pub(crate) const fn not(a: Verdict) -> Verdict {
    match a {
        Verdict::Yes => Verdict::No,
        Verdict::No => Verdict::Yes,
        Verdict::Undecided => Verdict::Undecided,
    }
}

/// A point or a vector with interval coordinates.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct IP {
    pub(crate) x: Iv,
    pub(crate) y: Iv,
}

impl IP {
    pub(crate) fn exact(p: Point2) -> Self {
        Self {
            x: Iv::exact(p.x),
            y: Iv::exact(p.y),
        }
    }

    pub(crate) fn sub(self, o: Self) -> Self {
        Self {
            x: self.x - o.x,
            y: self.y - o.y,
        }
    }

    pub(crate) fn add(self, o: Self) -> Self {
        Self {
            x: self.x + o.x,
            y: self.y + o.y,
        }
    }

    pub(crate) fn scale(self, k: Iv) -> Self {
        Self {
            x: self.x * k,
            y: self.y * k,
        }
    }

    pub(crate) fn neg(self) -> Self {
        Self {
            x: -self.x,
            y: -self.y,
        }
    }

    /// Rotated a quarter turn counter-clockwise.
    pub(crate) fn perp(self) -> Self {
        Self {
            x: -self.y,
            y: self.x,
        }
    }

    pub(crate) fn dot(self, o: Self) -> Iv {
        self.x * o.x + self.y * o.y
    }

    pub(crate) fn cross(self, o: Self) -> Iv {
        self.x * o.y - self.y * o.x
    }

    pub(crate) fn norm2(self) -> Iv {
        self.x.sq() + self.y.sq()
    }
}

/// The sign of `(b - a) x (c - a)`, exactly: positive when `c` is left of the line from `a`
/// to `b`. A float filter (Shewchuk's first bound) answers most calls; the rest sum the
/// sixteen exact products of the error-free differences.
///
/// Exact for coordinates that are 0 or of magnitude in `[1e-100, 1e9]`: no product then
/// underflows, so every `two_prod` is error-free (the region's input check).
pub(crate) fn orient(a: Point2, b: Point2, c: Point2) -> i8 {
    let (bx, by) = (b.x - a.x, b.y - a.y);
    let (cx, cy) = (c.x - a.x, c.y - a.y);
    two_products_sign(bx * cy, by * cx, -1.0).unwrap_or_else(|| exact_pair(a, b, c, true))
}

/// The sign of `(b - a) . (c - a)`, exactly: positive when `c` is ahead of `a` along the
/// direction from `a` to `b`. The same filter and fallback as [`orient`], under the same
/// range of coordinates.
pub(crate) fn dot_sign(a: Point2, b: Point2, c: Point2) -> i8 {
    let (bx, by) = (b.x - a.x, b.y - a.y);
    let (cx, cy) = (c.x - a.x, c.y - a.y);
    two_products_sign(bx * cx, by * cy, 1.0).unwrap_or_else(|| exact_pair(a, b, c, false))
}

/// The sign of `l + s r` when the float filter (Shewchuk's first bound) settles it.
fn two_products_sign(l: f64, r: f64, s: f64) -> Option<i8> {
    let det = l + s * r;
    // ccwerrboundA = (3 + 16 eps) eps, eps = 2^-53.
    let bound = (3.0 + 16.0 * f64::EPSILON / 2.0) * (f64::EPSILON / 2.0) * (l.abs() + r.abs());
    if det > bound {
        Some(1)
    } else if -det > bound {
        Some(-1)
    } else {
        None
    }
}

/// The exact sign of `(b - a) x (c - a)` (`cross`) or `(b - a) . (c - a)`: the sixteen
/// exact products of the error-free differences, summed exactly.
fn exact_pair(a: Point2, b: Point2, c: Point2, cross: bool) -> i8 {
    let (bx, bxe) = two_sum(b.x, -a.x);
    let (by, bye) = two_sum(b.y, -a.y);
    let (cx, cxe) = two_sum(c.x, -a.x);
    let (cy, cye) = two_sum(c.y, -a.y);
    // cross: bx cy - by cx; dot: bx cx + by cy.
    let (p, q, s) = if cross {
        ((cy, cye), (cx, cxe), -1.0)
    } else {
        ((cx, cxe), (cy, cye), 1.0)
    };
    let mut terms = [0.0; 16];
    let pairs = [
        (bx, p.0, 1.0),
        (bx, p.1, 1.0),
        (bxe, p.0, 1.0),
        (bxe, p.1, 1.0),
        (by, q.0, s),
        (by, q.1, s),
        (bye, q.0, s),
        (bye, q.1, s),
    ];
    for ([t0, t1], (u, v, k)) in terms.as_chunks_mut::<2>().0.iter_mut().zip(pairs) {
        let (hi, lo) = two_prod(u, v);
        *t0 = k * hi;
        *t1 = k * lo;
    }
    exact_sign(&terms)
}

/// The sign of the exact sum: the terms grown into a non-overlapping expansion (Shewchuk's
/// Grow-Expansion), whose largest non-zero component carries the sign.
fn exact_sign(terms: &[f64]) -> i8 {
    let mut h: Vec<f64> = Vec::with_capacity(terms.len() + 1);
    for &t in terms {
        let mut q = t;
        for c in &mut h {
            let (s, e) = two_sum(q, *c);
            *c = e;
            q = s;
        }
        h.push(q);
    }
    match h.iter().rev().find(|x| **x != 0.0) {
        Some(x) if *x > 0.0 => 1,
        Some(_) => -1,
        None => 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn orientation_is_exact_where_the_float_determinant_is_not() {
        let a = Point2::new(0.5, 0.5);
        // c on the line y = x, b far along it: exactly collinear.
        assert_eq!(
            orient(a, Point2::new(12.0, 12.0), Point2::new(24.0, 24.0)),
            0
        );
        // A point one ulp off the diagonal through two near points.
        let x = 0.5 + f64::EPSILON;
        assert_eq!(orient(a, Point2::new(x, x), Point2::new(x, x.next_up())), 1);
        assert_eq!(
            orient(a, Point2::new(x, x), Point2::new(x, x.next_down())),
            -1
        );
        // Shewchuk's near-collinear grid: the exact answer by rationals is the sign of
        // (b - a) x (c - a) on dyadic inputs, checked here by an i128 product.
        let s = f64::from_bits(0x3CD0_0000_0000_0000);
        for i in 0..16 {
            for j in 0..16 {
                let p = Point2::new(0.5 + f64::from(i) * s, 0.5 + f64::from(j) * s);
                let b = Point2::new(12.0, 12.0);
                let c = Point2::new(24.0, 24.0);
                let want = {
                    let q = |v: f64| (v * f64::from_bits(0x4310_0000_0000_0000)).round() as i128;
                    let (ax, ay, bx, by, cx, cy) = (q(p.x), q(p.y), q(b.x), q(b.y), q(c.x), q(c.y));
                    ((bx - ax) * (cy - ay) - (by - ay) * (cx - ax)).signum() as i8
                };
                assert_eq!(orient(p, b, c), want, "{i} {j}");
            }
        }
    }

    #[test]
    fn exact_operations_stay_points() {
        let (a, b) = (Iv::exact(3.0), Iv::exact(0.25));
        assert_eq!((a * b).0, Ivl::from(0.75));
        assert_eq!((a - a).0, Ivl::from(0.0));
        assert_eq!((a + b).0, Ivl::from(3.25));
        // Inexact: widened to a bracket around the true value.
        let s = Iv::exact(1.0) + Iv::exact(f64::EPSILON / 4.0);
        assert!(s.0.lo <= 1.0 && s.0.hi > 1.0, "{s:?}");
        let p = Iv::exact(0.1) * Iv::exact(0.1);
        assert!(p.0.lo < p.0.hi, "{p:?}");
    }

    #[test]
    fn dot_sign_is_exact_at_a_right_angle() {
        let a = Point2::new(0.5, 0.5);
        let x = 0.5 + f64::EPSILON;
        // c straight above a, b to its right: exactly perpendicular.
        assert_eq!(dot_sign(a, Point2::new(x, 0.5), Point2::new(0.5, 7.0)), 0);
        assert_eq!(dot_sign(a, Point2::new(x, 0.5), Point2::new(x, 7.0)), 1);
        assert_eq!(
            dot_sign(a, Point2::new(x, 0.5), Point2::new(0.5f64.next_down(), 7.0)),
            -1
        );
        assert_eq!(
            dot_sign(a, Point2::new(3.0, 1.0), Point2::new(-1.0, 0.0)),
            -1
        );
    }

    #[test]
    fn verdicts_hold_for_every_value() {
        let a = Iv(Ivl::new(1.0, 2.0));
        let b = Iv(Ivl::new(1.5, 3.0));
        assert_eq!(greater(b, a), Verdict::Undecided);
        assert_eq!(greater(Iv::exact(2.5), a), Verdict::Yes);
        assert_eq!(greater(a, Iv::exact(2.0)), Verdict::No);
        assert_eq!(greater(Iv(Ivl::whole()), a), Verdict::Undecided);
        assert_eq!(Iv(Ivl::new(-1.0, 2.0)).sq().0.lo, 0.0);
    }
}
