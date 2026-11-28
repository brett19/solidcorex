// Written for solidcorex. Apache-2.0.
//! Least-squares and weighted least-squares fits at given parameters and knots (The NURBS Book
//! §9.4.1), with the end point held and, where asked, the end derivatives.

use super::FitError;
use super::band::Band;
use super::ends::{End, Spline, pinned_points};
use super::knots::check_increasing;
use super::param::check_finite;
use crate::{Approach, KnotVector, MAX_DEGREE};

/// The curve on the clamped `knots` closest to `data` at the parameters `t` in the least-squares
/// sense, through the first and last point, with the conditions `start` and `end` held.
///
/// The first `start.order() + 1` and the last `end.order() + 1` control points are fixed by the
/// end conditions; the others solve the banded least-squares problem by rotations, with no
/// normal equations. Exact data on the curve's own knots give the curve back. Errors when the
/// system has no unique solution ([`FitError::Singular`]: fewer interior data than unknowns,
/// or a span without data), and as [`interpolate_on`](super::interpolate_on).
pub fn least_squares<const N: usize>(
    knots: &KnotVector,
    data: &[[f64; N]],
    t: &[f64],
    start: &End<N>,
    end: &End<N>,
) -> Result<Spline<N>, FitError> {
    fit(knots, data, t, None, start, end)
}

/// [`least_squares`] minimising `sum w[k] |C(t[k]) - data[k]|^2`, with a positive finite weight
/// for every point. The end points are held whatever their weights.
pub fn weighted_least_squares<const N: usize>(
    knots: &KnotVector,
    data: &[[f64; N]],
    t: &[f64],
    weights: &[f64],
    start: &End<N>,
    end: &End<N>,
) -> Result<Spline<N>, FitError> {
    fit(knots, data, t, Some(weights), start, end)
}

fn fit<const N: usize>(
    knots: &KnotVector,
    data: &[[f64; N]],
    t: &[f64],
    weights: Option<&[f64]>,
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
    if let Some(w) = weights {
        if w.len() != data.len() {
            return Err(FitError::WeightCount {
                count: w.len(),
                points: data.len(),
            });
        }
        if let Some((index, &weight)) = w
            .iter()
            .enumerate()
            .find(|(_, w)| !(w.is_finite() && **w > 0.0))
        {
            return Err(FitError::Weight { index, weight });
        }
    }
    let order = start.order().max(end.order());
    if order > p {
        return Err(FitError::DegreeTooLow { degree: p, order });
    }
    if !knots.is_clamped() {
        return Err(FitError::Unclamped);
    }
    let (lo, hi) = knots.ends();
    if t.first() != Some(&lo) || t.last() != Some(&hi) {
        return Err(FitError::EndsNotOnDomain);
    }
    let count = knots.point_count();
    let (head, tail) = (start.order() + 1, end.order() + 1);
    if count < head + tail {
        return Err(FitError::TooFewControlPoints {
            count,
            needed: head + tail,
        });
    }
    let (q0, qn) = data.first().zip(data.last()).ok_or(FitError::Defect {
        why: "data went missing",
    })?;
    let first_pins = pinned_points(knots, *q0, start, false)?;
    let last_pins = pinned_points(knots, *qn, end, true)?;
    let free = count - head - tail;
    let mut band = Band::<N>::new(free, p + 1)?;

    let weight = |k: usize| -> f64 { weights.and_then(|w| w.get(k)).map_or(1.0, |w| w.sqrt()) };
    for (k, (&u, q)) in t.iter().zip(data).enumerate().skip(1).take(data.len() - 2) {
        let basis = knots.basis(u, Approach::Above)?;
        let sw = weight(k);
        let mut rhs = *q;
        let mut row = [0.0; MAX_DEGREE + 1];
        let mut at = None;
        let mut len = 0;
        for (col, &v) in (basis.first()..).zip(basis.values()) {
            let fixed = if col < head {
                first_pins.get(col)
            } else if col >= count - tail {
                last_pins.get(col - (count - tail))
            } else {
                None
            };
            if let Some(pin) = fixed {
                for (r, x) in rhs.iter_mut().zip(pin) {
                    *r -= v * x;
                }
            } else {
                at.get_or_insert(col - head);
                if let Some(slot) = row.get_mut(len) {
                    *slot = v * sw;
                }
                len += 1;
            }
        }
        if let (Some(first), Some(coeffs)) = (at, row.get(..len)) {
            band.push(first, coeffs, rhs.map(|r| r * sw))?;
        }
    }
    let mut points = first_pins;
    points.extend(band.solve()?);
    points.extend(last_pins);
    Ok(Spline::new(knots.clone(), points))
}
