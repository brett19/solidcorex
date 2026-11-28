// Written for solidcorex. Apache-2.0.
//! Parameter values for data points: uniform, chord length or centripetal (The NURBS Book,
//! §9.2.1), on `[0, 1]`.

use super::FitError;

/// How data points are given parameters (the pin's `ParamMethod`, upstream `par` flag).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Param {
    /// Equally spaced parameters.
    Uniform,
    /// Parameters proportional to the chord lengths between neighbouring points.
    ChordLength,
    /// Parameters proportional to the square roots of the chord lengths.
    Centripetal,
}

/// The distance between two points.
pub(crate) fn distance<const N: usize>(a: &[f64; N], b: &[f64; N]) -> f64 {
    a.iter()
        .zip(b)
        .map(|(x, y)| (x - y) * (x - y))
        .sum::<f64>()
        .sqrt()
}

/// Checks that every coordinate of every point is finite.
pub(crate) fn check_finite<const N: usize>(points: &[[f64; N]]) -> Result<(), FitError> {
    match points.iter().position(|p| p.iter().any(|x| !x.is_finite())) {
        Some(index) => Err(FitError::NotFinite { index }),
        None => Ok(()),
    }
}

/// The parameters `t[0..=n]` of the points, with `t[0] = 0` and `t[n] = 1` exactly.
///
/// Errors for fewer than two points or a coordinate that is not finite, and for chord length
/// or centripetal parameters of points that all coincide ([`FitError::ZeroLength`]; the pin
/// leaves the interior parameters at zero). Neighbouring points that coincide give equal
/// parameters, which [`interpolate`](super::interpolate) refuses where it is given them.
pub fn parameters<const N: usize>(
    points: &[[f64; N]],
    method: Param,
) -> Result<Vec<f64>, FitError> {
    if points.len() < 2 {
        return Err(FitError::TooFewPoints {
            count: points.len(),
            needed: 2,
        });
    }
    check_finite(points)?;
    let n = points.len() - 1;
    if method == Param::Uniform {
        return Ok((0..=n)
            .map(|i| if i == n { 1.0 } else { i as f64 / n as f64 })
            .collect());
    }
    let steps: Vec<f64> = points
        .windows(2)
        .map(|w| match w {
            [a, b] if method == Param::Centripetal => distance(a, b).sqrt(),
            [a, b] => distance(a, b),
            _ => 0.0,
        })
        .collect();
    let total: f64 = steps.iter().sum();
    if !(total > 0.0 && total.is_finite()) {
        return Err(FitError::ZeroLength);
    }
    let mut t = Vec::with_capacity(n + 1);
    t.push(0.0);
    let mut run = 0.0;
    for step in steps.iter().take(n - 1) {
        run += step;
        t.push(run / total);
    }
    t.push(1.0);
    Ok(t)
}
