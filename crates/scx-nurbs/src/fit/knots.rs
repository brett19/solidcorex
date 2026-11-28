// Written for solidcorex. Apache-2.0.
//! Knot vectors for interpolation (averaging) and for least squares (The NURBS Book §9.2.1
//! eq. 9.8, §9.2.2, §9.4.1 eq. 9.68-9.69). All are clamped.

use super::FitError;
use crate::KnotVector;

/// Checks `t` increases strictly and is finite; `Ok(())` for the empty and single cases too.
pub(crate) fn check_increasing(t: &[f64]) -> Result<(), FitError> {
    if let Some(index) = t.iter().position(|x| !x.is_finite()) {
        return Err(FitError::NotFinite { index });
    }
    match t.windows(2).position(|w| matches!(w, [a, b] if b <= a)) {
        Some(i) => Err(FitError::ParametersNotIncreasing { index: i + 1 }),
        None => Ok(()),
    }
}

/// The clamped knot vector of `degree` by averaging the parameters `t[0..=n]`, for an
/// interpolation whose start has `start_order` and whose end has `end_order` derivative
/// conditions (0 for a point alone, 1 for a tangent, ...; [`End::order`](super::End::order)).
///
/// The curve has `n + 1 + start_order + end_order` control points. The interior knots are the
/// averages of `degree` consecutive entries of the parameters with the first repeated
/// `start_order` more times and the last `end_order` more: for no derivative condition
/// `U[j + p] = (t[j] + ... + t[j + p - 1]) / p` (eq. 9.8), and for one at each end the
/// book's eq. for end derivatives, which this reproduces. Every system with distinct
/// parameters then satisfies the Schoenberg-Whitney condition.
///
/// Errors when the parameters do not increase, when there are too few control points for the
/// degree, and when an end's order is not below the degree.
pub fn averaging(
    t: &[f64],
    degree: usize,
    start_order: usize,
    end_order: usize,
) -> Result<KnotVector, FitError> {
    check_increasing(t)?;
    let (&first, &last) =
        t.first()
            .zip(t.last())
            .filter(|_| t.len() >= 2)
            .ok_or(FitError::TooFewPoints {
                count: t.len(),
                needed: 2,
            })?;
    let order = start_order.max(end_order);
    if degree == 0 || (order > 0 && order >= degree) {
        return Err(FitError::DegreeTooLow { degree, order });
    }
    let n = t.len() - 1;
    // The last control point index.
    let r = n + start_order + end_order;
    if r < degree {
        return Err(FitError::TooFewControlPoints {
            count: r + 1,
            needed: degree + 1,
        });
    }
    let site = |i: usize| -> f64 {
        let k = i.saturating_sub(start_order).min(n);
        t.get(k).copied().unwrap_or(last)
    };
    let mut knots = vec![first; degree + 1];
    for j in 1..=r - degree {
        let sum: f64 = (j..j + degree).map(site).sum();
        knots.push(sum / degree as f64);
    }
    knots.extend(core::iter::repeat_n(last, degree + 1));
    Ok(KnotVector::new(degree, knots)?)
}

/// The clamped knot vector of `degree` for a least-squares fit of `points` control points to
/// data at the parameters `t` (eq. 9.68-9.69): the interior knots interpolate the parameters
/// at spacing `t.len() / (points - degree)`, so that every span has data (the
/// Schoenberg-Whitney condition for these knots).
///
/// Errors when the parameters do not increase, when `points` is not in
/// `degree + 1..t.len()`.
pub fn approximation(t: &[f64], degree: usize, points: usize) -> Result<KnotVector, FitError> {
    check_increasing(t)?;
    let (&first, &last) = t.first().zip(t.last()).ok_or(FitError::TooFewPoints {
        count: 0,
        needed: 2,
    })?;
    if degree == 0 {
        return Err(FitError::DegreeTooLow { degree, order: 0 });
    }
    if points <= degree || points >= t.len() {
        return Err(FitError::TooFewControlPoints {
            count: points,
            needed: degree + 1,
        });
    }
    let spans = points - degree;
    let d = t.len() as f64 / spans as f64;
    let mut knots = vec![first; degree + 1];
    for j in 1..spans {
        let at = j as f64 * d;
        let i = at.floor() as usize;
        let alpha = at - i as f64;
        let (below, above) = t.get(i - 1).zip(t.get(i)).ok_or(FitError::Defect {
            why: "an approximation knot fell outside the parameters",
        })?;
        knots.push((1.0 - alpha) * below + alpha * above);
    }
    knots.extend(core::iter::repeat_n(last, degree + 1));
    Ok(KnotVector::new(degree, knots)?)
}
