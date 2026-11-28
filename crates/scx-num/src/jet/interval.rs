// Written for solidcorex. Apache-2.0.
//! A value and its first two derivatives in one variable, each enclosed over an interval of
//! it ([`Jet2`]), and the bounds of `|g|` over a piece that SU6's gap and FT3's fit
//! certificate take from such jets ([`chord`], [`vector_bound`]).
//!
//! [`Jet2`] is a [`Ring`], [`Ranged`] and [`Trig`], so any evaluator written over a scalar
//! (a NURBS curve's `point_over`, an analytic curve's `eval`) runs in it and gives, over an
//! interval `t` of its parameter, enclosures of the value and of its first two derivatives
//! (forward differentiation: the chain rule in every operation). Seed it with
//! [`Jet2::variable`].
//!
//! Moved here from `scx-surface`'s gap (SU6) for FT3, whose fitter in `scx-nurbs` needs it
//! below `scx-surface` (77 §1.1). The arithmetic is unchanged, so SU6's bounds keep their
//! bits; [`Ranged`], [`Trig`] and [`Neg`] are new.

use core::ops::{Add, Div, Mul, Neg, Sub};

use crate::interval::{Ivl, round_down, round_up};
use crate::ring::Ring;
use crate::trig::{Ranged, Trig};

/// A value and its first two derivatives in one variable, each enclosed over an interval
/// of it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Jet2 {
    /// The value.
    pub f: Ivl,
    /// The first derivative.
    pub d1: Ivl,
    /// The second derivative.
    pub d2: Ivl,
}

impl Jet2 {
    /// The jet of the three enclosures.
    #[must_use]
    pub const fn new(f: Ivl, d1: Ivl, d2: Ivl) -> Self {
        Self { f, d1, d2 }
    }

    /// The variable itself over `t`: derivative one, second derivative zero.
    #[must_use]
    pub fn variable(t: Ivl) -> Self {
        Self::new(t, Ivl::from(1.0), Ivl::from(0.0))
    }

    /// Encloses both.
    #[must_use]
    pub fn hull(self, o: Self) -> Self {
        Self::new(self.f.hull(o.f), self.d1.hull(o.d1), self.d2.hull(o.d2))
    }

    /// A constant enclosed by `f`.
    #[must_use]
    pub fn lift_ivl(f: Ivl) -> Self {
        Self::new(f, Ivl::from(0.0), Ivl::from(0.0))
    }

    /// `self * k`.
    #[must_use]
    pub fn scale(self, k: f64) -> Self {
        let k = Ivl::from(k);
        Self::new(self.f * k, self.d1 * k, self.d2 * k)
    }

    /// `sqrt(self)`: the derivatives are whole where the value may be zero.
    #[must_use]
    pub fn sqrt(self) -> Self {
        let s = self.f.sqrt();
        let two_s = Ivl::from(2.0) * s;
        let d1 = self.d1 / two_s;
        let d2 = (self.d2 - Ivl::from(2.0) * d1 * d1) / two_s;
        Self::new(s, d1, d2)
    }
}

impl Add for Jet2 {
    type Output = Self;
    fn add(self, b: Self) -> Self {
        Self::new(self.f + b.f, self.d1 + b.d1, self.d2 + b.d2)
    }
}

impl Sub for Jet2 {
    type Output = Self;
    fn sub(self, b: Self) -> Self {
        Self::new(self.f - b.f, self.d1 - b.d1, self.d2 - b.d2)
    }
}

impl Mul for Jet2 {
    type Output = Self;
    fn mul(self, b: Self) -> Self {
        let two = Ivl::from(2.0);
        Self::new(
            self.f * b.f,
            self.d1 * b.f + self.f * b.d1,
            self.d2 * b.f + two * self.d1 * b.d1 + self.f * b.d2,
        )
    }
}

impl Div for Jet2 {
    type Output = Self;
    fn div(self, b: Self) -> Self {
        let q = self.f / b.f;
        let d1 = (self.d1 - q * b.d1) / b.f;
        let d2 = (self.d2 - Ivl::from(2.0) * d1 * b.d1 - q * b.d2) / b.f;
        Self::new(q, d1, d2)
    }
}

/// Exact: every part negated.
impl Neg for Jet2 {
    type Output = Self;
    fn neg(self) -> Self {
        Self::new(-self.f, -self.d1, -self.d2)
    }
}

impl Ring for Jet2 {
    fn lift(d: f64) -> Self {
        Self::new(Ivl::from(d), Ivl::from(0.0), Ivl::from(0.0))
    }
}

/// The value's range is the jet's: a piecewise evaluator picks its pieces by it and clips
/// it to each, the derivatives (with respect to the one variable) kept as they are.
impl Ranged for Jet2 {
    fn ends(self) -> (f64, f64) {
        self.f.ends()
    }

    fn clip(self, lo: f64, hi: f64) -> Self {
        Self::new(self.f.clip(lo, hi), self.d1, self.d2)
    }

    fn join(self, other: Self) -> Self {
        self.hull(other)
    }

    fn undefined() -> Self {
        Self::new(Ivl::whole(), Ivl::whole(), Ivl::whole())
    }

    /// `high - low` is a constant: its derivatives are zero.
    fn gap(_right: Self, _left: Self, high: f64, low: f64) -> Self {
        Self::lift_ivl(Ivl::from(high) - Ivl::from(low))
    }

    /// `(f^2, 2 f f', 2 (f'^2 + f f''))`, the value's square tight ([`Ranged::sqr`]).
    fn sqr(self) -> Self {
        let two = Ivl::from(2.0);
        Self::new(
            self.f.sqr(),
            two * self.f * self.d1,
            two * (self.d1.sqr() + self.f * self.d2),
        )
    }
}

/// The chain rule on [`Ivl`]'s enclosures.
impl Trig for Jet2 {
    fn sin_cos(self) -> (Self, Self) {
        let (s, c) = self.f.sin_cos();
        let d1s = self.d1.sqr();
        let sin = Self::new(s, c * self.d1, c * self.d2 - s * d1s);
        let cos = Self::new(c, -(s * self.d1), -(s * self.d2) - c * d1s);
        (sin, cos)
    }

    fn sinh_cosh(self) -> (Self, Self) {
        let (s, c) = self.f.sinh_cosh();
        let d1s = self.d1.sqr();
        let sinh = Self::new(s, c * self.d1, c * self.d2 + s * d1s);
        let cosh = Self::new(c, s * self.d1, s * self.d2 + c * d1s);
        (sinh, cosh)
    }

    fn sqrt(self) -> Self {
        Self::sqrt(self)
    }

    /// `theta' = (x y' - y x') / (x^2 + y^2)` and its derivative; whole where the box may
    /// hold the origin, and the value `[-pi, pi]` where it may meet the cut.
    fn atan2(y: Self, x: Self) -> Self {
        let f = <Ivl as Trig>::atan2(y.f, x.f);
        let r2 = x.f.sqr() + y.f.sqr();
        let cross = x.f * y.d1 - y.f * x.d1;
        let d1 = cross / r2;
        let cross1 = x.f * y.d2 - y.f * x.d2;
        let d2 = (cross1 - d1 * Ivl::from(2.0) * (x.f * x.d1 + y.f * y.d1)) / r2;
        Self::new(f, d1, d2)
    }
}

/// `[x^2]`, not below zero.
#[must_use]
pub fn square(x: Ivl) -> Ivl {
    let (m, g) = (x.mig(), x.mag());
    Ivl::new(round_down(m * m).max(0.0), round_up(g * g))
}

/// `[|v|]` of an interval vector (zero for no coordinates).
#[must_use]
pub fn norm<const D: usize>(v: [Ivl; D]) -> Ivl {
    v.into_iter()
        .map(square)
        .reduce(|a, b| a + b)
        .unwrap_or_else(|| Ivl::from(0.0))
        .sqrt()
}

/// `|g|` over a piece of length at most `h` by the chord bound
/// `max(|g(a)|, |g(b)|) + h^2 / 8 sup |g''|` (`ends` the enclosures of `|g|` at the two ends,
/// `d2` the bound of `|g''|`) and the direct enclosure `over` of `|g|`, the smaller; infinite
/// when neither is finite.
#[must_use]
pub fn chord(ends: (Ivl, Ivl), d2: f64, over: Ivl, h: f64) -> f64 {
    let ends = ends.0.mag().max(ends.1.mag());
    let bound = round_up(ends + round_up(round_up(h * h) * d2 / 8.0));
    let direct = over.mag();
    let b = bound.min(direct);
    if b.is_nan() { f64::INFINITY } else { b }
}

/// `|D|` over a piece of length at most `h` from the jets of a vector `D` over it and `|D|`
/// at its ends: the chord bound on `D` itself, the chord bound on `q = |D|^2` (whose second
/// derivative `2 (|D'|^2 + D . D'')` stays small where `|D|` is near constant though `D`
/// turns), and the direct enclosure, the smallest.
#[must_use]
pub fn vector_bound<const D: usize>(d: [Jet2; D], ends: (Ivl, Ivl), h: f64) -> f64 {
    let natural = norm(d.map(|j| j.f));
    let by_d = chord(ends, norm(d.map(|j| j.d2)).mag(), natural, h);
    let q = d
        .into_iter()
        .map(|j| j * j)
        .reduce(|a, b| a + b)
        .unwrap_or_else(|| Jet2::lift(0.0));
    let end = ends.0.mag().max(ends.1.mag());
    let qb = round_up(round_up(end * end) + round_up(round_up(h * h) * q.d2.mag() / 8.0));
    let by_q = round_up(qb.sqrt());
    if by_q.is_nan() { by_d } else { by_d.min(by_q) }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Encloses `x`.
    fn holds(i: Ivl, x: f64) -> bool {
        i.lo <= x && x <= i.hi
    }

    /// `f`, `f'` and `f''` of the jet over a box of `t` enclose the closed forms at points of it.
    fn check(jet: impl Fn(Jet2) -> Jet2, f: impl Fn(f64) -> [f64; 3], lo: f64, hi: f64) {
        let j = jet(Jet2::variable(Ivl::new(lo, hi)));
        for k in 0..=16 {
            let t = lo + (hi - lo) * f64::from(k) / 16.0;
            let [v, d1, d2] = f(t);
            assert!(holds(j.f, v), "f at {t}: {v} not in {:?}", j.f);
            assert!(holds(j.d1, d1), "f' at {t}: {d1} not in {:?}", j.d1);
            assert!(holds(j.d2, d2), "f'' at {t}: {d2} not in {:?}", j.d2);
        }
    }

    #[test]
    fn trig_and_ranged_enclose_closed_forms() {
        // sin(3t), cos(3t) over a box crossing the extrema.
        check(
            |t| (t.scale(3.0)).sin_cos().0,
            |t| {
                let (s, c) = crate::fmath::sin_cos(3.0 * t);
                [s, 3.0 * c, -9.0 * s]
            },
            0.2,
            0.9,
        );
        check(
            |t| (t.scale(3.0)).sin_cos().1,
            |t| {
                let (s, c) = crate::fmath::sin_cos(3.0 * t);
                [c, -3.0 * s, -9.0 * c]
            },
            -0.4,
            1.3,
        );
        check(
            |t| (t * t).sinh_cosh().0,
            |t| {
                let u = t * t;
                [
                    crate::fmath::sinh(u),
                    2.0 * t * crate::fmath::cosh(u),
                    2.0 * crate::fmath::cosh(u) + 4.0 * t * t * crate::fmath::sinh(u),
                ]
            },
            -0.5,
            0.7,
        );
        check(
            |t| t.sinh_cosh().1,
            |t| {
                let (s, c) = crate::fmath::sinh_cosh(t);
                [c, s, c]
            },
            -0.5,
            0.7,
        );
        check(
            |t| (t + Jet2::lift(2.0)).sqrt(),
            |t| {
                let r = (t + 2.0).sqrt();
                [r, 0.5 / r, -0.25 / (r * r * r)]
            },
            0.0,
            1.0,
        );
        // The square across zero: f^2 from zero, and its derivatives.
        check(Ranged::sqr, |t| [t * t, 2.0 * t, 2.0], -0.5, 0.75);
        check(|t| -t.sqr(), |t| [-t * t, -2.0 * t, -2.0], -0.5, 0.75);
        let s = Jet2::variable(Ivl::new(-0.5, 0.75)).sqr();
        assert!(s.f.lo >= 0.0, "{:?}", s.f);
    }

    #[test]
    fn ranged_gap_is_a_constant_and_clip_keeps_the_derivatives() {
        let t = Jet2::variable(Ivl::new(0.0, 4.0));
        let g = Jet2::gap(t, t, 5.0, 1.0);
        assert!(holds(g.f, 4.0) && g.d1 == Ivl::from(0.0) && g.d2 == Ivl::from(0.0));
        let c = t.clip(1.0, 2.0);
        assert_eq!(c.f.ends(), (1.0, 2.0));
        assert_eq!((c.d1, c.d2), (t.d1, t.d2));
        assert_eq!(t.ends(), (0.0, 4.0));
        let u = Jet2::undefined();
        assert!(u.f.lo.is_infinite() && u.d2.hi.is_infinite());
    }

    #[test]
    fn vector_bound_of_a_circle_chord() {
        // D(t) = (cos t, sin t) - (1, 0) over [0, h]: |D| is 2 sin(t / 2), at most 2 sin(h / 2).
        let h = 0.25;
        let t = Jet2::variable(Ivl::new(0.0, h));
        let (s, c) = t.sin_cos();
        let d = [c - Jet2::lift(1.0), s];
        let at = |x: f64| {
            let (s, c) = Ivl::from(x).sin_cos();
            norm([c - Ivl::from(1.0), s])
        };
        let b = vector_bound(d, (at(0.0), at(h)), h);
        let truth = 2.0 * crate::fmath::sin(h / 2.0);
        assert!(b >= truth && b <= truth * 1.05, "{b} against {truth}");
    }
}
