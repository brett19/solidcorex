// Written for solidcorex. Apache-2.0.
//! The Bernstein form of one polynomial piece of a surface, for certificates (CN3).
//!
//! A certificate bounds an expression of the surface (a distance, a normal's sign) over a
//! box of parameters from the coefficients of its Bernstein form: the products, derivatives
//! and restrictions are `scx_algo::bernstein`'s, on the forms this module gives. The pin's
//! `Bernstein`/`Bezier` extraction lived in each certificate (`certificate.rs`,
//! `surface/nurbs_invert.rs`) beside its own rounding; here the piece is exact: the surface
//! restricted to the box ([`NurbsSurface::restrict`]) is one Bezier piece, whose control
//! points are its Bernstein coefficients, so no coefficient is computed beyond the splits'.

use scx_algo::bernstein::Bernstein2;
use scx_num::extent2d::Extent2d;

use crate::{NurbsSurface, NurbsSurfaceError, ParamAxis, RestrictCause, Result};

/// The numerators and the weight of one polynomial piece of a surface over `[0, 1]^2`, the box
/// mapped to the unit square: `S(s, t) = coordinates(s, t) / weight(s, t)` for a rational
/// piece and `coordinates(s, t)` for a polynomial one.
///
/// The coordinates are the Bernstein forms of `w P` (or `P`); every operation of
/// [`Bernstein2`] (products, derivatives, sums) applies to them directly.
#[derive(Debug, Clone, PartialEq)]
pub struct BernsteinForm<const D: usize> {
    coordinates: Vec<Bernstein2>,
    weight: Option<Bernstein2>,
}

impl<const D: usize> BernsteinForm<D> {
    /// The forms of the `D` coordinates (of `w P` when rational), in order.
    #[must_use]
    pub fn coordinates(&self) -> &[Bernstein2] {
        &self.coordinates
    }

    /// The weight's form, for a rational piece.
    #[must_use]
    pub const fn weight(&self) -> Option<&Bernstein2> {
        self.weight.as_ref()
    }

    /// The degrees `(p, q)` of the piece.
    #[must_use]
    pub fn degrees(&self) -> (usize, usize) {
        self.coordinates.first().map_or((0, 0), Bernstein2::degrees)
    }

    /// The point at `(s, t)` of the unit square (de Casteljau).
    #[must_use]
    pub fn point(&self, s: f64, t: f64) -> [f64; D] {
        let w = self.weight.as_ref().map_or(1.0, |w| w.eval(s, t));
        let mut out = [0.0; D];
        for (o, c) in out.iter_mut().zip(&self.coordinates) {
            *o = c.eval(s, t) / w;
        }
        out
    }

    /// The piece over `[s0, s1] x [t0, t1]` of the unit square, reparametrised to it (the
    /// forms' [`Bernstein2::restrict`]).
    #[must_use]
    pub fn restrict(&self, s0: f64, s1: f64, t0: f64, t1: f64) -> Self {
        Self {
            coordinates: self
                .coordinates
                .iter()
                .map(|c| c.restrict(s0, s1, t0, t1))
                .collect(),
            weight: self.weight.as_ref().map(|w| w.restrict(s0, s1, t0, t1)),
        }
    }
}

const FORM: NurbsSurfaceError = NurbsSurfaceError::Defect {
    why: "a restricted piece is not a Bezier patch",
};

impl<const D: usize> NurbsSurface<D> {
    /// The Bernstein form of the surface over the box `uv`, which must lie within one span of
    /// each direction (no knot strictly inside it): the box is mapped to the unit square, so
    /// `form.point(s, t)` is `S(u0 + s (u1 - u0), v0 + t (v1 - v0))`. Exact up to the rounding
    /// of the splits at the box's ends (none for a box that is a span).
    ///
    /// # Errors
    ///
    /// [`NurbsSurfaceError::Restrict`] over [`RestrictCause::Interior`] for a knot inside the
    /// box, and over `Outside` or `Empty` as [`NurbsSurface::restrict`]; its splits' errors
    /// (an unclamped end) as they come.
    pub fn bernstein_over(&self, uv: &Extent2d) -> Result<BernsteinForm<D>> {
        self.check_box(uv)?;
        for (axis, lo, hi) in [
            (ParamAxis::U, uv.x_min(), uv.x_max()),
            (ParamAxis::V, uv.y_min(), uv.y_max()),
        ] {
            if let Some(&knot) = self.knots(axis).iter().find(|&&k| lo < k && k < hi) {
                return Err(NurbsSurfaceError::Restrict(RestrictCause::Interior {
                    axis,
                    knot,
                }));
            }
        }
        let piece = self.restrict(uv)?;
        let (p, q) = (piece.degree(ParamAxis::U), piece.degree(ParamAxis::V));
        let net = piece.points();
        if net.nrows() != p + 1 || net.ncols() != q + 1 {
            return Err(FORM);
        }
        let form = |data: &[[f64; D]], k: usize| -> Result<Bernstein2> {
            Bernstein2::new(
                p,
                q,
                data.iter().filter_map(|x| x.get(k).copied()).collect(),
            )
            .map_err(|_| FORM)
        };
        let (numerators, weight) = match (piece.weighted_points(), piece.weights()) {
            (Some(wp), Some(w)) => (
                wp.points(),
                Some(Bernstein2::new(p, q, w.points().to_vec()).map_err(|_| FORM)?),
            ),
            _ => (net.points(), None),
        };
        let coordinates = (0..D)
            .map(|k| form(numerators, k))
            .collect::<Result<Vec<_>>>()?;
        Ok(BernsteinForm {
            coordinates,
            weight,
        })
    }
}
