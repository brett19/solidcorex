// Written for solidcorex. Apache-2.0.
//! A surface over one piece in rational Bernstein form (research 73 §4.5, row SM6a): the
//! homogeneous control net `(H, W)` of the piece's single Bézier patch, read from
//! [`Surface::to_nurbs`], and `delta`, a bound of the distance between the net's point and
//! the surface's own point at the same uv.
//!
//! An analytic kind's image keeps the kind's uv (its quarter arcs are the kind's own
//! parameter, research 63 D13) and differs from the kind only by the rounding of its control
//! data; a NURBS surface's is its restriction to the piece, by knot insertion, which rounds
//! each control point by a few convex combinations. `delta` bounds both (see [`Net::of`]).

use scx_algo::bernstein::{Bernstein2, Bounded};
use scx_num::certified::gamma;
use scx_num::extent2d::Extent2d;
use scx_nurbs_surface::ParamAxis;
use scx_surface::Surface;

/// The rational Bernstein form of a surface over one piece: `x(s, t) = H(s, t) / W(s, t)` on
/// the unit square, `(s, t)` mapped affinely onto `uv` (`s` along u).
#[derive(Clone, Debug)]
pub(crate) struct Net {
    /// The homogeneous coordinates `w P`, one polynomial per axis.
    pub(crate) h: [Bernstein2; 3],
    /// The weight, positive over the square.
    pub(crate) w: Bernstein2,
    /// A bound of `|H / W - S|` at every uv of the piece, `S` the surface's own point.
    pub(crate) delta: f64,
}

impl Net {
    /// The net of `s` over `uv`, when its Bézier form there is one patch on exactly `uv`
    /// with positive weights. `None` otherwise (several spans, a domain that moved, a weight
    /// not positive or a value not finite): the field path then does not apply to the piece.
    ///
    /// `delta` is `gamma(32 + 8 (p + q)) (sqrt(3) max|H| + reach max W) / min W`, with
    /// `reach` the largest control coordinate: the control data are within that many
    /// roundings of the exact ones relative to their size (an analytic kind's weights and
    /// points are one or two roundings each; a restriction is at most `p + q` insertions per
    /// end, each a convex combination of two roundings), and a rational point moves by at
    /// most the homogeneous change over the least weight plus the weight's change times the
    /// point's size, over the least weight.
    pub(crate) fn of(s: &Surface, uv: &Extent2d) -> Option<Self> {
        let n = s.to_nurbs(uv).ok()?;
        if n.domain() != *uv {
            return None;
        }
        let (p, q) = (n.degree(ParamAxis::U), n.degree(ParamAxis::V));
        if !single_span(n.knots(ParamAxis::U), p) || !single_span(n.knots(ParamAxis::V), q) {
            return None;
        }
        let points = n.points();
        if points.nrows() != p + 1 || points.ncols() != q + 1 {
            return None;
        }
        let (hs, ws): (Vec<[f64; 3]>, Vec<f64>) = match (n.weighted_points(), n.weights()) {
            (Some(h), Some(w)) => (h.points().to_vec(), w.points().to_vec()),
            _ => (points.points().to_vec(), vec![1.0; (p + 1) * (q + 1)]),
        };
        let coord = |k: usize| -> Option<Bernstein2> {
            Bernstein2::new(
                p,
                q,
                hs.iter()
                    .map(|x| x.get(k).copied())
                    .collect::<Option<_>>()?,
            )
            .ok()
        };
        let h = [coord(0)?, coord(1)?, coord(2)?];
        let w = Bernstein2::new(p, q, ws.clone()).ok()?;
        let w_min = ws.iter().copied().fold(f64::INFINITY, f64::min);
        let w_max = ws.iter().copied().fold(0.0_f64, f64::max);
        if w_min.is_nan() || w_min <= 0.0 || !w_max.is_finite() {
            return None;
        }
        let h_max = h
            .iter()
            .fold(0.0_f64, |m, b| m.max(b.max_abs_coefficient()));
        let reach = points
            .points()
            .iter()
            .flatten()
            .fold(0.0_f64, |m, x| m.max(x.abs()));
        let size = up(3.0_f64.sqrt() * h_max * (1.0 + gamma(2)) + reach * w_max);
        let delta = up(gamma(32 + 8 * (p + q)) * size / w_min);
        delta.is_finite().then_some(Self { h, w, delta })
    }

    /// The weight raised to `k`, with its rounding bound.
    pub(crate) fn weight_power(&self, k: usize) -> Option<Bounded> {
        let w = Bounded::exact(self.w.clone());
        let mut out = w.clone();
        for _ in 1..k {
            out = out.mul(&w).ok()?;
        }
        Some(out)
    }
}

/// A knot vector of one span over its domain: `p + 1` equal knots, then `p + 1` equal knots.
fn single_span(knots: &[f64], p: usize) -> bool {
    knots.len() == 2 * (p + 1)
        && knots
            .split_at(p + 1)
            .0
            .windows(2)
            .all(|w| w.first() == w.last())
        && knots
            .split_at(p + 1)
            .1
            .windows(2)
            .all(|w| w.first() == w.last())
}

/// `x` raised past the rounding of the few operations that computed it from nonnegative
/// terms (at most 8, each within the unit roundoff), and past underflow: the bound
/// `scx_algo::bernstein` uses for its own error terms.
pub(crate) fn up(x: f64) -> f64 {
    x * (1.0 + gamma(16)) + f64::MIN_POSITIVE
}

/// `x b`, where the exact constant is within `x_err` of `x`: the product's own bound
/// ([`Bounded::scale`]) plus `x_err` times the polynomial's size.
pub(crate) fn scale_inexact(b: &Bounded, x: f64, x_err: f64) -> Bounded {
    let mut out = b.scale(x);
    out.err = up(out.err + x_err * (b.f.max_abs_coefficient() + b.err));
    out
}
