// Written for solidcorex. Apache-2.0.
//! What a fit is told about an end of the curve, and what it gives back.

use super::FitError;
use crate::{Approach, KnotVector, NurbsCurve, NurbsError};

/// The highest derivative order an end condition holds (the third, for G3).
pub const MAX_END_ORDER: usize = 3;

/// The condition at one end of a fit, beyond passing through the end data point.
///
/// The vectors are *parametric* derivatives `C'`, `C''`, `C'''` at the end, in the direction
/// of increasing parameter, in the fit's own parameterisation: a geometric condition (a unit
/// tangent, a curvature) becomes a derivative once the caller states the speed, as `C'' = s^2 k N`
/// for speed `s` and curvature `k` on a curve without acceleration along it. The conditions
/// are linear in the control points, so the fit solves them exactly. (The pin's
/// `NL_START`/`NL_END` flags and its estimate of a missing end derivative from the data have
/// no counterpart: every condition is stated.)
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum End<const N: usize> {
    /// G0: the end point only.
    Point,
    /// G1: the end point and the first derivative.
    Tangent {
        /// `C'` at the end.
        d1: [f64; N],
    },
    /// G2: the end point, the first and the second derivative.
    Curvature {
        /// `C'` at the end.
        d1: [f64; N],
        /// `C''` at the end.
        d2: [f64; N],
    },
    /// G3: the end point and the first three derivatives.
    Hermite {
        /// `C'` at the end.
        d1: [f64; N],
        /// `C''` at the end.
        d2: [f64; N],
        /// `C'''` at the end.
        d3: [f64; N],
    },
}

impl<const N: usize> End<N> {
    /// The number of derivatives held: 0 for [`End::Point`] to 3 for [`End::Hermite`].
    #[must_use]
    pub const fn order(&self) -> usize {
        match self {
            Self::Point => 0,
            Self::Tangent { .. } => 1,
            Self::Curvature { .. } => 2,
            Self::Hermite { .. } => 3,
        }
    }

    /// The derivatives held, `C'` first, in the first `order()` entries.
    pub(crate) fn derivatives(&self) -> [[f64; N]; MAX_END_ORDER] {
        let zero = [0.0; N];
        match *self {
            Self::Point => [zero; MAX_END_ORDER],
            Self::Tangent { d1 } => [d1, zero, zero],
            Self::Curvature { d1, d2 } => [d1, d2, zero],
            Self::Hermite { d1, d2, d3 } => [d1, d2, d3],
        }
    }

    /// Checks that every derivative held is finite.
    pub(crate) fn check_finite(&self) -> Result<(), FitError> {
        let held = self.derivatives();
        if held
            .iter()
            .take(self.order())
            .flatten()
            .any(|x| !x.is_finite())
        {
            return Err(FitError::NotFinite { index: usize::MAX });
        }
        Ok(())
    }
}

/// The result of a fit: a clamped knot vector and the control points on it.
///
/// Control points are in whatever space the data were: fit homogeneous points `(w x, w)` (an
/// `N` one above the curve's) and pass them to [`NurbsCurve::from_homogeneous`] for a
/// rational fit.
#[derive(Debug, Clone, PartialEq)]
pub struct Spline<const N: usize> {
    knots: KnotVector,
    points: Vec<[f64; N]>,
}

impl<const N: usize> Spline<N> {
    pub(crate) const fn new(knots: KnotVector, points: Vec<[f64; N]>) -> Self {
        Self { knots, points }
    }

    /// The knot vector.
    #[must_use]
    pub const fn knot_vector(&self) -> &KnotVector {
        &self.knots
    }

    /// The control points.
    #[must_use]
    pub fn points(&self) -> &[[f64; N]] {
        &self.points
    }

    /// The polynomial curve with these knots and control points.
    pub fn into_curve(self) -> Result<NurbsCurve<N>, NurbsError> {
        NurbsCurve::new(self.knots, self.points, None)
    }
}

/// The control points a derivative condition fixes at the start (`end = false`) or at the end of
/// the domain of the clamped `knots`: the point `q` and the first `order` derivatives pin the
/// first (last) `order + 1` control points, one at a time, since the `k`-th derivative at the
/// start depends on `P[0..=k]` with `N'^(k)[k] != 0`.
///
/// Returned in curve order: `P[0..=order]` for the start, `P[c - 1 - order..c]` for the end.
pub(crate) fn pinned_points<const N: usize>(
    knots: &KnotVector,
    q: [f64; N],
    cond: &End<N>,
    at_end: bool,
) -> Result<Vec<[f64; N]>, FitError> {
    let p = knots.degree();
    let (u0, u1) = knots.ends();
    let u = if at_end { u1 } else { u0 };
    let rows = knots.basis_derivs::<{ MAX_END_ORDER + 1 }>(u, Approach::Above)?;
    let held = cond.derivatives();
    // Controls in order of how far they are from the end: P[0], P[1], ... at the start,
    // P[c-1], P[c-2], ... at the end. `value(j)` is the coefficient row index of the j-th.
    let mut pinned: Vec<[f64; N]> = vec![q];
    for (k, d) in held.iter().take(cond.order()).enumerate() {
        let k = k + 1;
        let row = rows.row(k).ok_or(FitError::Defect {
            why: "a basis derivative row is missing",
        })?;
        let coeff = |j: usize| -> Option<f64> {
            // The j-th control from the end is function j (start) or p - j (end) of the window.
            row.get(if at_end { p.checked_sub(j)? } else { j }).copied()
        };
        let mut acc = *d;
        for (j, pj) in pinned.iter().enumerate() {
            let c = coeff(j).ok_or(FitError::Defect {
                why: "a pinned control has no basis coefficient",
            })?;
            for (a, x) in acc.iter_mut().zip(pj) {
                *a -= c * x;
            }
        }
        let own =
            coeff(k)
                .filter(|c| *c != 0.0 && c.is_finite())
                .ok_or(FitError::DegreeTooLow {
                    degree: p,
                    order: k,
                })?;
        pinned.push(acc.map(|a| a / own));
    }
    if at_end {
        pinned.reverse();
    }
    Ok(pinned)
}
