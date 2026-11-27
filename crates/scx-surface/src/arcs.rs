// Written for solidcorex. Apache-2.0.
//! One direction of the pin's NURBS-compatible uv (research 63 D13): how a NURB parameter
//! maps to an angle and back, and the angle's cosine and sine with their derivatives.
//!
//! The pin builds every angle direction of its canonical analytic NURBs from quarter arcs
//! (research 64 §2): standard-form rational quadratics with end weights 1 and middle weight
//! `cos(pi/4)`, one knot span per arc, all spans the same length in the parameter. A full turn
//! is four arcs from the frame's `x`; the sphere's latitude is two arcs from the south pole.
//!
//! On the arc of span `k` (parameters `[k L, (k + 1) L]`), with `m_k` the parameter of its
//! middle, `a = 2 tan(pi/8) / L` and `tau = a (t - m_k)`, the angle from the arc's middle is
//! `psi = 2 atan(tau)`, so
//!
//! ```text
//! cos psi = (1 - tau^2) / (1 + tau^2),   sin psi = 2 tau / (1 + tau^2)
//! ```
//!
//! which is the rational quadratic arc itself. Only ring operations are used, so the same code
//! evaluates in `f64`, intervals and Taylor models. In complex form
//! `e^{i psi} = 2 q - 1` with `q = 1 / (1 - i tau)`, whose `n`-th derivative in `t` is
//! `2 n! (i a)^n q^{n + 1}`: the jets come from powers of `q`, with no trigonometry.

use core::f64::consts::{FRAC_1_SQRT_2, FRAC_PI_2, FRAC_PI_4};

use scx_num::fmath;
use scx_num::ring::Ring;
use scx_nurbs::Approach;

/// `tan(pi/8) = sqrt(2) - 1`, correctly rounded.
const TAN_PI_8: f64 = 0.414_213_562_373_095_05;

/// The cosine and sine of each arc's middle angle, for the four arcs of a turn from angle 0.
const TURN_MIDS: [(f64, f64); 4] = [
    (FRAC_1_SQRT_2, FRAC_1_SQRT_2),
    (-FRAC_1_SQRT_2, FRAC_1_SQRT_2),
    (-FRAC_1_SQRT_2, -FRAC_1_SQRT_2),
    (FRAC_1_SQRT_2, -FRAC_1_SQRT_2),
];

/// The same for the two arcs of a half turn from `-pi/2` (the sphere's latitude).
const HALF_MIDS: [(f64, f64); 2] = [
    (FRAC_1_SQRT_2, -FRAC_1_SQRT_2),
    (FRAC_1_SQRT_2, FRAC_1_SQRT_2),
];

/// Which arcs a direction has.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Sweep {
    /// Four arcs from angle 0, repeating every turn.
    Turn,
    /// Two arcs from `-pi/2` to `pi/2`; past the ends, the end arc's formula continues.
    Half,
}

/// An angle direction: quarter arcs each `span` long in the parameter.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Arcs {
    span: f64,
    sweep: Sweep,
}

/// The arc a parameter is evaluated on: its span index `k` (not reduced by the period, so
/// `k L` is the span's start) and its place `q` among the arcs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Piece {
    k: i64,
    q: usize,
}

/// The cosine and sine of an angle and their first `D` derivatives in the parameter.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Trig<const D: usize> {
    /// `(cos phi, sin phi)`.
    pub(crate) value: (f64, f64),
    /// `(d^n cos phi / dt^n, d^n sin phi / dt^n)` for `n = 1..=D`.
    pub(crate) derivs: [(f64, f64); D],
}

impl<const D: usize> Trig<D> {
    /// The `n`-th derivative pair, the value at `n = 0`.
    pub(crate) fn nth(&self, n: usize) -> (f64, f64) {
        match n.checked_sub(1) {
            None => self.value,
            Some(i) => self.derivs.get(i).copied().unwrap_or((0.0, 0.0)),
        }
    }
}

impl Arcs {
    /// A full turn over `[0, period]` (`period` finite and above zero, checked by the kind).
    pub(crate) const fn turn(period: f64) -> Self {
        Self {
            span: period / 4.0,
            sweep: Sweep::Turn,
        }
    }

    /// The latitude, `[-pi/2, pi/2]` over `[0, 1]`.
    pub(crate) const fn half() -> Self {
        Self {
            span: 0.5,
            sweep: Sweep::Half,
        }
    }

    /// The arcs of this direction.
    const fn count(self) -> usize {
        match self.sweep {
            Sweep::Turn => 4,
            Sweep::Half => 2,
        }
    }

    /// The angle at parameter 0.
    const fn start(self) -> f64 {
        match self.sweep {
            Sweep::Turn => 0.0,
            Sweep::Half => -FRAC_PI_2,
        }
    }

    /// The arc that `t` is on, from `side` at a break (research 63 X5).
    pub(crate) fn piece(self, t: f64, side: Approach) -> Piece {
        let x = (t / self.span).floor();
        // `t / L` rounds; step the index so that `k L <= t < (k + 1) L` holds as computed.
        let mut k = if x.is_finite() { x as i64 } else { 0 };
        if t < self.start_of(k) {
            k -= 1;
        } else if t >= self.start_of(k + 1) {
            k += 1;
        }
        if side == Approach::Below && t == self.start_of(k) {
            k -= 1;
        }
        let n = self.count() as i64;
        match self.sweep {
            Sweep::Turn => Piece {
                k,
                q: usize::try_from(k.rem_euclid(n)).unwrap_or(0),
            },
            Sweep::Half => {
                let k = k.clamp(0, n - 1);
                Piece {
                    k,
                    q: usize::try_from(k).unwrap_or(0),
                }
            }
        }
    }

    /// The parameter where span `k` starts.
    fn start_of(self, k: i64) -> f64 {
        k as f64 * self.span
    }

    /// `(cos, sin)` of arc `q`'s middle angle.
    fn mid(self, q: usize) -> (f64, f64) {
        let table: &[(f64, f64)] = match self.sweep {
            Sweep::Turn => &TURN_MIDS,
            Sweep::Half => &HALF_MIDS,
        };
        table.get(q).copied().unwrap_or((1.0, 0.0))
    }

    /// `a = 2 tan(pi/8) / L`, the rate of `tau` in `t`.
    fn rate(self) -> f64 {
        2.0 * TAN_PI_8 / self.span
    }

    /// `tau` at `t` on `piece`, in any ring.
    fn tau<T: Ring>(self, t: T, piece: Piece) -> T {
        let mid = (piece.k as f64 + 0.5) * self.span;
        (t - T::lift(mid)) * T::lift(self.rate())
    }

    /// `(cos phi, sin phi)` at `t` with the formula of `piece`, in any ring.
    pub(crate) fn cos_sin<T: Ring>(self, t: T, piece: Piece) -> (T, T) {
        let tau = self.tau(t, piece);
        let one = T::lift(1.0);
        let tt = tau * tau;
        let d = one + tt;
        let c = (one - tt) / d;
        let s = (tau + tau) / d;
        let (cm, sm) = self.mid(piece.q);
        let (cm, sm) = (T::lift(cm), T::lift(sm));
        (cm * c - sm * s, sm * c + cm * s)
    }

    /// `(cos phi, sin phi)` and `D` derivatives at `t`, on the arc `side` picks at a break.
    pub(crate) fn trig<const D: usize>(self, t: f64, side: Approach) -> Trig<D> {
        let piece = self.piece(t, side);
        let tau = self.tau(t, piece);
        let a = self.rate();
        // q = 1 / (1 - i tau) = (1 + i tau) / (1 + tau^2)
        let d = 1.0 + tau * tau;
        let q = (1.0 / d, tau / d);
        let (cm, sm) = self.mid(piece.q);
        let rot = |(re, im): (f64, f64)| (cm * re - sm * im, sm * re + cm * im);
        let value = rot((2.0 * q.0 - 1.0, 2.0 * q.1));
        let mut derivs = [(0.0, 0.0); D];
        // c_n = 2 n! (i a)^n q^(n + 1), built up as c_n = c_(n-1) * n * (i a) * q.
        let mut c = (2.0 * q.0, 2.0 * q.1);
        for (n, out) in derivs.iter_mut().enumerate() {
            let f = (n + 1) as f64 * a;
            // times i f
            let ic = (-c.1 * f, c.0 * f);
            c = (ic.0 * q.0 - ic.1 * q.1, ic.0 * q.1 + ic.1 * q.0);
            *out = rot(c);
        }
        Trig { value, derivs }
    }

    /// The angle (radians) at parameter `t`, continuous across breaks and turns.
    pub(crate) fn angle(self, t: f64) -> f64 {
        let piece = self.piece(t, Approach::Above);
        let tau = self.tau(t, piece);
        self.start() + piece.k as f64 * FRAC_PI_2 + FRAC_PI_4 + 2.0 * fmath::atan(tau)
    }

    /// The parameter at angle `phi` (radians): on the turn the one in the turn's copy of
    /// the parameter line that holds `phi`, on the half turn the end arcs' formulas past the
    /// ends (finite while `phi` is within `3 pi/4` past them).
    pub(crate) fn parameter(self, phi: f64) -> f64 {
        let rel = phi - self.start();
        let x = (rel / FRAC_PI_2).floor();
        let mut k = if x.is_finite() { x as i64 } else { 0 };
        if self.sweep == Sweep::Half {
            k = k.clamp(0, 1);
        }
        let psi = rel - (k as f64 * FRAC_PI_2 + FRAC_PI_4);
        let tau = fmath::tan(0.5 * psi);
        (k as f64 + 0.5) * self.span + tau / self.rate()
    }

    /// The parameter of each break in `[lo, hi]`, for tests and poles.
    #[cfg(test)]
    pub(crate) fn span(self) -> f64 {
        self.span
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn breaks_are_quarter_turns() {
        let a = Arcs::turn(1.0);
        for k in 0..=4 {
            let t = f64::from(k) * 0.25;
            let phi = a.angle(t);
            assert!((phi - f64::from(k) * FRAC_PI_2).abs() < 1e-15, "{k}: {phi}");
            let (c, s) = a.cos_sin(t, a.piece(t, Approach::Above));
            assert!((c - fmath::cos(phi)).abs() < 1e-15 && (s - fmath::sin(phi)).abs() < 1e-15);
        }
        let h = Arcs::half();
        assert!((h.angle(0.0) + FRAC_PI_2).abs() < 1e-15);
        assert!((h.angle(0.5)).abs() < 1e-15);
        assert!((h.angle(1.0) - FRAC_PI_2).abs() < 1e-15);
    }

    #[test]
    fn angle_and_parameter_invert() {
        for a in [Arcs::turn(1.0), Arcs::turn(7.3), Arcs::half()] {
            for i in 0..=200 {
                let t = a.span() * a.count() as f64 * (f64::from(i) / 200.0);
                let back = a.parameter(a.angle(t));
                assert!((back - t).abs() < 1e-14 * (1.0 + t.abs()), "{t} {back}");
            }
        }
    }

    #[test]
    fn sides_differ_only_at_breaks() {
        let a = Arcs::turn(1.0);
        assert_eq!(a.piece(0.25, Approach::Above).q, 1);
        assert_eq!(a.piece(0.25, Approach::Below).q, 0);
        assert_eq!(a.piece(0.0, Approach::Below).q, 3);
        assert_eq!(a.piece(0.3, Approach::Below).q, 1);
        let h = Arcs::half();
        assert_eq!(h.piece(0.0, Approach::Below).q, 0);
        assert_eq!(h.piece(1.0, Approach::Above).q, 1);
    }

    #[test]
    fn derivatives_match_differences() {
        let a = Arcs::turn(1.0);
        let t = 0.37;
        let j: Trig<3> = a.trig(t, Approach::Above);
        let h = 1e-5;
        for n in 1..=3 {
            let lo: Trig<3> = a.trig(t - h, Approach::Above);
            let hi: Trig<3> = a.trig(t + h, Approach::Above);
            let fd = (
                (hi.nth(n - 1).0 - lo.nth(n - 1).0) / (2.0 * h),
                (hi.nth(n - 1).1 - lo.nth(n - 1).1) / (2.0 * h),
            );
            let scale = 1.0 + j.nth(n).0.abs();
            assert!((fd.0 - j.nth(n).0).abs() < 1e-5 * scale * [1.0, 10.0, 100.0, 1000.0][n]);
            assert!((fd.1 - j.nth(n).1).abs() < 1e-5 * scale * [1.0, 10.0, 100.0, 1000.0][n]);
        }
    }
}
