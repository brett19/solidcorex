//! Piecewise cubic Hermite curves (C^1 across their knots): the fitted UV trim (two
//! coordinates) and the fitted edge (three).

use scx_num::ring::Ring;

use crate::jet::Jet;
use crate::surface::hermite;

/// A C^1 piecewise cubic through `p[k]` with slope `m[k]` at knot `t[k]`.
#[derive(Clone, Debug)]
pub struct Hermite<const D: usize> {
    /// Knots, increasing.
    pub t: Vec<f64>,
    /// Points at the knots.
    pub p: Vec<[f64; D]>,
    /// Slopes at the knots.
    pub m: Vec<[f64; D]>,
}

impl<const D: usize> Hermite<D> {
    /// The interpolant of `f` (point and slope) at `n` equal spans of `[t0, t1]`.
    pub fn fit(f: impl Fn(f64) -> ([f64; D], [f64; D]), t0: f64, t1: f64, n: usize) -> Self {
        let t: Vec<f64> = (0..=n).map(|i| t0 + (t1 - t0) * i as f64 / n as f64).collect();
        let (p, m) = t.iter().map(|&x| f(x)).unzip();
        Self { t, p, m }
    }

    /// The span holding `[a, b]` (the last span for the end).
    pub fn span(&self, a: f64, b: f64) -> usize {
        let last = self.t.len() - 2;
        (0..=last).find(|&k| self.t[k] <= a && b <= self.t[k + 1]).unwrap_or(last)
    }

    /// The span holding `t`.
    pub fn span_at(&self, t: f64) -> usize {
        let last = self.t.len() - 2;
        (0..=last).find(|&k| t <= self.t[k + 1]).unwrap_or(last)
    }

    /// The curve on span `k` at `t`.
    pub fn eval<T: Ring>(&self, k: usize, t: Jet<T>) -> [Jet<T>; D] {
        let h = T::lift(self.t[k + 1]) - T::lift(self.t[k]);
        let s = (t - Jet::cst(self.t[k])) / Jet::of(h);
        let (b, _) = hermite(s);
        let jh = Jet::of(h);
        let mut out = [Jet::cst(0.0); D];
        for (i, o) in out.iter_mut().enumerate() {
            *o = b[0] * Jet::cst(self.p[k][i])
                + b[1] * jh * Jet::cst(self.m[k][i])
                + b[2] * Jet::cst(self.p[k + 1][i])
                + b[3] * jh * Jet::cst(self.m[k + 1][i]);
        }
        out
    }
}
