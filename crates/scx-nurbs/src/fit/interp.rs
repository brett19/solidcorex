// Written for solidcorex. Apache-2.0.
//! Interpolation at given parameters, with end conditions (The NURBS Book §9.2.1, §9.2.2).

use super::FitError;
use super::band::Band;
use super::ends::{End, MAX_END_ORDER, Spline};
use super::knots::{averaging, check_increasing};
use super::param::check_finite;
use crate::{Approach, KnotVector};

/// The curve of `degree` through `data` at the parameters `t`, with the conditions `start`
/// and `end`, on the knot vector [`averaging`] of `t` gives.
///
/// It has `n + 1 + start.order() + end.order()` control points for `n + 1` data points, and
/// every datum is met to rounding: the system is solved by rotations with no pivoting and no
/// normal equations, in `O(n p^2)` work and `O(n p)` storage.
///
/// Errors for fewer than two points, data or parameters that are not finite or not strictly
/// increasing, parameter and point counts that differ, and an end condition the degree
/// cannot hold (an order not below the degree). For any one parameterisation of the same
/// points, see [`parameters`](super::parameters).
pub fn interpolate<const N: usize>(
    data: &[[f64; N]],
    t: &[f64],
    degree: usize,
    start: &End<N>,
    end: &End<N>,
) -> Result<Spline<N>, FitError> {
    if t.len() != data.len() {
        return Err(FitError::ParameterCount {
            count: t.len(),
            points: data.len(),
        });
    }
    let knots = averaging(t, degree, start.order(), end.order())?;
    interpolate_on(&knots, data, t, start, end)
}

/// The curve through `data` at the parameters `t`, with the conditions `start` and `end`, on
/// the given `knots`.
///
/// The first and last parameter must be the ends of the knots' domain, and the knots must
/// give exactly `n + 1 + start.order() + end.order()` control points. A system without a
/// unique solution (the parameters and knots break the Schoenberg-Whitney condition) is
/// [`FitError::Singular`].
pub fn interpolate_on<const N: usize>(
    knots: &KnotVector,
    data: &[[f64; N]],
    t: &[f64],
    start: &End<N>,
    end: &End<N>,
) -> Result<Spline<N>, FitError> {
    let p = knots.degree();
    if data.len() < 2 {
        return Err(FitError::TooFewPoints {
            count: data.len(),
            needed: 2,
        });
    }
    if t.len() != data.len() {
        return Err(FitError::ParameterCount {
            count: t.len(),
            points: data.len(),
        });
    }
    check_finite(data)?;
    check_increasing(t)?;
    start.check_finite()?;
    end.check_finite()?;
    let order = start.order().max(end.order());
    if order > p {
        return Err(FitError::DegreeTooLow { degree: p, order });
    }
    let (lo, hi) = knots.ends();
    if t.first() != Some(&lo) || t.last() != Some(&hi) {
        return Err(FitError::EndsNotOnDomain);
    }
    let expected = data.len() + start.order() + end.order();
    if knots.point_count() != expected {
        return Err(FitError::KnotCount {
            points: knots.point_count(),
            expected,
        });
    }

    let mut band = Band::<N>::new(expected, p + 1)?;
    let derivative_rows = |at: f64, cond: &End<N>, band: &mut Band<N>| -> Result<(), FitError> {
        if cond.order() == 0 {
            return Ok(());
        }
        let rows = knots.basis_derivs::<{ MAX_END_ORDER + 1 }>(at, Approach::Above)?;
        for (k, d) in cond.derivatives().iter().take(cond.order()).enumerate() {
            let row = rows.row(k + 1).ok_or(FitError::Defect {
                why: "a basis derivative row is missing",
            })?;
            band.push(rows.first(), row, *d)?;
        }
        Ok(())
    };
    let point_row = |u: f64, q: [f64; N], band: &mut Band<N>| -> Result<(), FitError> {
        let basis = knots.basis(u, Approach::Above)?;
        band.push(basis.first(), basis.values(), q)
    };

    let ((q0, qn), (&t0, &tn)) = (
        data.first().zip(data.last()).ok_or(FitError::Defect {
            why: "data went missing",
        })?,
        t.first().zip(t.last()).ok_or(FitError::Defect {
            why: "parameters went missing",
        })?,
    );
    point_row(t0, *q0, &mut band)?;
    derivative_rows(t0, start, &mut band)?;
    for (&u, q) in t.iter().zip(data).skip(1).take(data.len() - 2) {
        point_row(u, *q, &mut band)?;
    }
    derivative_rows(tn, end, &mut band)?;
    point_row(tn, *qn, &mut band)?;
    Ok(Spline::new(knots.clone(), band.solve()?))
}
