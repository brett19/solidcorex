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
use scx_num::trig::Trig;
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
pub enum Sweep {
    /// Four arcs from angle 0, repeating every turn.
    Turn,
    /// Two arcs from `-pi/2` to `pi/2`; past the ends, the end arc's formula continues.
    Half,
}

/// An angle direction: quarter arcs each `span` long in the parameter.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Arcs {
    span: f64,
    sweep: Sweep,
}

/// The arc a parameter is evaluated on: its span index `k` (not reduced by the period, so
/// `k L` is the span's start) and its place `q` among the arcs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Piece {
    k: i64,
    q: usize,
}

/// The cosine and sine of an angle and their first `D` derivatives in the parameter.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ArcTrig<const D: usize> {
    /// `(cos phi, sin phi)`.
    pub value: (f64, f64),
    /// `(d^n cos phi / dt^n, d^n sin phi / dt^n)` for `n = 1..=D`.
    pub derivs: [(f64, f64); D],
}

impl<const D: usize> ArcTrig<D> {
    /// The `n`-th derivative pair, the value at `n = 0`.
    pub fn nth(&self, n: usize) -> (f64, f64) {
        match n.checked_sub(1) {
            None => self.value,
            Some(i) => self.derivs.get(i).copied().unwrap_or((0.0, 0.0)),
        }
    }
}

impl Arcs {
    /// A full turn over `[0, period]` (`period` finite and above zero, checked by the kind).
    pub const fn turn(period: f64) -> Self {
        Self {
            span: period / 4.0,
            sweep: Sweep::Turn,
        }
    }

    /// The latitude, `[-pi/2, pi/2]` over `[0, 1]`.
    pub const fn half() -> Self {
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
    pub fn piece(self, t: f64, side: Approach) -> Piece {
        let x = (t / self.span).floor();
        // `t / L` rounds; step the index so that `k L <= t < (k + 1) L` holds as computed.
        let mut k = if x.is_finite() { x as i64 } else { 0 };
        if t < self.start_of(k) {
            k = k.saturating_sub(1);
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
    pub fn cos_sin<T: Ring>(self, t: T, piece: Piece) -> (T, T) {
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
    pub fn trig<const D: usize>(self, t: f64, side: Approach) -> ArcTrig<D> {
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
        ArcTrig { value, derivs }
    }

    /// The angle (radians) at parameter `t`, continuous across breaks and turns.
    pub fn angle(self, t: f64) -> f64 {
        let piece = self.piece(t, Approach::Above);
        let tau = self.tau(t, piece);
        self.start() + piece.k as f64 * FRAC_PI_2 + FRAC_PI_4 + 2.0 * fmath::atan(tau)
    }

    /// The parameter at angle `phi` (radians): on the turn the one in the turn's copy of
    /// the parameter line that holds `phi`, on the half turn the end arcs' formulas past the
    /// ends (finite while `phi` is within `3 pi/4` past them).
    pub fn parameter(self, phi: f64) -> f64 {
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

    /// The span index of the arc that angle `phi` is on, as [`Self::parameter`] picks it,
    /// from `side` at a break.
    fn arc_of_angle(self, phi: f64, side: Approach) -> i64 {
        let rel = phi - self.start();
        let x = (rel / FRAC_PI_2).floor();
        let mut k = if x.is_finite() { x as i64 } else { 0 };
        if side == Approach::Below && rel == k as f64 * FRAC_PI_2 {
            k -= 1;
        }
        if self.sweep == Sweep::Half {
            k = k.clamp(0, 1);
        }
        k
    }

    /// The parameter at angle `phi` and its first `D` derivatives in the angle, in any
    /// scalar with sine and cosine (in [`Ivl`](scx_num::interval::Ivl) an enclosure over
    /// every arc `phi` meets), on the arc `side` picks at a break: [`Self::parameter`] in
    /// closed form for the 2-D kind whose coordinate is an angle's parameter (research 63
    /// §4.11).
    ///
    /// On arc `k`, with `psi` the angle from its middle, the parameter is
    /// `(k + 1/2) L + tau / a` with `tau = tan(psi / 2) = sin psi / (1 + cos psi)` (the
    /// denominator at least `1 + cos(pi/4)`). Then `tau' = (1 + tau^2) / 2` and, for
    /// `n >= 1`, `tau^(n+1) = (1/2) sum_j C(n, j) tau^(j) tau^(n-j)`: no allocation, `D^2`
    /// products. The map is `C^1` at a break and not `C^2` (`tau''` changes sign there).
    #[must_use]
    pub fn parameter_jet<T: Trig, const D: usize>(self, phi: T, side: Approach) -> (T, [T; D]) {
        /// Past this many arcs an interval's enclosure is the whole line.
        const MAX_ARCS: i64 = 8;
        let (lo, hi) = phi.ends();
        let (first, last) = if lo == hi {
            let k = self.arc_of_angle(lo, side);
            (k, k)
        } else {
            let first = self.arc_of_angle(lo, Approach::Above);
            (first, self.arc_of_angle(hi, Approach::Below).max(first))
        };
        if last.saturating_sub(first) > MAX_ARCS {
            return (T::undefined(), [T::undefined(); D]);
        }
        let mut out: Option<(T, [T; D])> = None;
        for k in first..=last {
            let one = self.on_arc::<T, D>(phi, k, first == last);
            out = Some(match out {
                None => one,
                Some((v, d)) => {
                    let mut joined = d;
                    for (j, x) in joined.iter_mut().zip(one.1) {
                        *j = j.join(x);
                    }
                    (v.join(one.0), joined)
                }
            });
        }
        out.unwrap_or_else(|| (T::undefined(), [T::undefined(); D]))
    }

    /// [`Self::parameter_jet`] with arc `k`'s formula, `phi` clipped to the arc unless it is
    /// the only one.
    fn on_arc<T: Trig, const D: usize>(self, phi: T, k: i64, only: bool) -> (T, [T; D]) {
        let l = T::lift;
        let phi = if only {
            phi
        } else {
            let low = self.start() + k as f64 * FRAC_PI_2;
            // The half turn's end arcs continue past its ends.
            let half_end = |end: i64| self.sweep == Sweep::Half && k == end;
            let lo = if half_end(0) { f64::NEG_INFINITY } else { low };
            let hi = if half_end(1) {
                f64::INFINITY
            } else {
                low + FRAC_PI_2
            };
            phi.clip(lo, hi)
        };
        let psi = (phi - l(self.start())) - l(k as f64 * FRAC_PI_2 + FRAC_PI_4);
        let (s, c) = psi.sin_cos();
        let one = l(1.0);
        let tau = s / (one + c);
        let rate = l(self.rate());
        let value = l((k as f64 + 0.5) * self.span) + tau / rate;
        // tau^(n) for n = 1..=D, in d[n - 1].
        let mut d = [l(0.0); D];
        for n in 0..D {
            let next = if n == 0 {
                (one + tau * tau) * l(0.5)
            } else {
                let nth = |j: usize| match j.checked_sub(1) {
                    None => tau,
                    Some(i) => d.get(i).copied().unwrap_or_else(|| l(0.0)),
                };
                let mut sum = l(0.0);
                let mut binom = 1.0;
                for j in 0..=n {
                    sum = sum + l(binom) * nth(j) * nth(n - j);
                    binom = binom * (n - j) as f64 / (j + 1) as f64;
                }
                sum * l(0.5)
            };
            if let Some(slot) = d.get_mut(n) {
                *slot = next;
            }
        }
        (value, d.map(|x| x / rate))
    }

    /// Whether this is a periodic turn (else the half turn).
    pub fn is_turn(self) -> bool {
        self.sweep == Sweep::Turn
    }

    /// The length of the parameter this direction is defined over: a turn's period, or
    /// the half turn's `[0, 1]`.
    pub fn length(self) -> f64 {
        self.count() as f64 * self.span
    }

    /// `(cos, sin)` at the break where span `k` starts, exactly: a quarter turn from the
    /// start angle times `k`.
    fn at_break(self, k: i64) -> (f64, f64) {
        let quarter = match self.sweep {
            Sweep::Turn => k,
            Sweep::Half => k - 1,
        };
        match quarter.rem_euclid(4) {
            0 => (1.0, 0.0),
            1 => (0.0, 1.0),
            2 => (-1.0, 0.0),
            _ => (0.0, -1.0),
        }
    }

    /// The arcs over `[lo, hi]` as a clamped rational quadratic (`to_nurbs`, 63 §4.8):
    /// the knots, `lo` and `hi` three times and every break strictly between them twice,
    /// and per control point the unit circle's point `(c, s)` and its weight.
    ///
    /// Each knot span is one arc, or the part of it inside `[lo, hi]`, with the arc's own
    /// formula, so the image at `t` is `cos_sin(t)` of the arc that holds `t`: the same
    /// parameter, no reparameterisation. In `tau` (the module docs) the arc's homogeneous
    /// form `(1 - tau^2, 2 tau, 1 + tau^2)` is a quadratic polynomial, and `tau` is affine in
    /// `t`, so the part over `[tau_a, tau_b]` has the control points of its blossom
    /// `(1 - x y, x + y, 1 + x y)` at `(tau_a, tau_a)`, `(tau_a, tau_b)` and `(tau_b, tau_b)`,
    /// turned to the arc's middle. The weights `1 + x y` are positive (`|tau| <= tan(pi/8)`),
    /// and at a break the point is the axis exactly.
    ///
    /// `lo < hi`, both finite and inside one copy of the direction's domain (checked by the
    /// caller): at most five spans.
    pub fn bezier(self, lo: f64, hi: f64) -> (Vec<f64>, Vec<(f64, f64, f64)>) {
        let first = self.piece(lo, Approach::Above).k;
        let last = self.piece(hi, Approach::Below).k.max(first);
        let spans = usize::try_from(last - first + 1).unwrap_or(1);
        let mut knots = Vec::with_capacity(2 * spans + 4);
        let mut ctrl = Vec::with_capacity(2 * spans + 1);
        knots.extend([lo; 3]);
        let one = |x: f64, y: f64, rot: (f64, f64)| {
            let w = 1.0 + x * y;
            let (a, b) = ((1.0 - x * y) / w, (x + y) / w);
            (rot.0 * a - rot.1 * b, rot.1 * a + rot.0 * b, w)
        };
        let w_break = 1.0 + TAN_PI_8 * TAN_PI_8;
        for k in first..=last {
            let piece = Piece {
                k,
                q: match self.sweep {
                    Sweep::Turn => usize::try_from(k.rem_euclid(4)).unwrap_or(0),
                    Sweep::Half => usize::try_from(k.clamp(0, 1)).unwrap_or(0),
                },
            };
            let (a, b) = (self.start_of(k), self.start_of(k + 1));
            let ta = if k == first { lo } else { a };
            let tb = if k == last { hi } else { b };
            let xa = if ta == a {
                -TAN_PI_8
            } else {
                self.tau(ta, piece)
            };
            let xb = if tb == b {
                TAN_PI_8
            } else {
                self.tau(tb, piece)
            };
            let rot = self.mid(piece.q);
            if k == first {
                ctrl.push(if ta == a {
                    let (c, s) = self.at_break(k);
                    (c, s, w_break)
                } else {
                    one(xa, xa, rot)
                });
            }
            ctrl.push(one(xa, xb, rot));
            ctrl.push(if tb == b {
                let (c, s) = self.at_break(k + 1);
                (c, s, w_break)
            } else {
                one(xb, xb, rot)
            });
            if k != last {
                knots.extend([b; 2]);
            }
        }
        knots.extend([hi; 3]);
        (knots, ctrl)
    }

    /// The parameter of each break in `[lo, hi]`, for tests and poles.
    #[cfg(test)]
    pub fn span(self) -> f64 {
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
        let j: ArcTrig<3> = a.trig(t, Approach::Above);
        let h = 1e-5;
        for n in 1..=3 {
            let lo: ArcTrig<3> = a.trig(t - h, Approach::Above);
            let hi: ArcTrig<3> = a.trig(t + h, Approach::Above);
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
