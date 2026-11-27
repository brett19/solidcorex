// Written for solidcorex. Apache-2.0.
//! Bezier helpers: binomial coefficients, Bernstein polynomials, degree elevation and
//! subdivision.
//!
//! These follow solidcore's `nurbs::bezier` at the pin (`binomial`, `eval_one_basis`,
//! `band_matrix_into`, `elevate_degree`, `split`) in the same arithmetic order. The pin's
//! binomials come from an 18-row Pascal table; here they are computed exactly up to the
//! largest that a double holds, so elevation reaches [`MAX_DEGREE`](crate::MAX_DEGREE).

use crate::point::Control;
use crate::{NurbsError, Result, MAX_DEGREE};

/// The binomial coefficient `C(n, k)`, exactly; `0` for `k > n`. `None` when it is above
/// `2^53`, where not every integer is a double.
#[must_use]
pub fn binomial(n: usize, k: usize) -> Option<f64> {
    if k > n {
        return Some(0.0);
    }
    let k = k.min(n - k);
    let mut c: u64 = 1;
    for i in 1..=k {
        // C(n-k+i, i) = C(n-k+i-1, i-1) (n-k+i) / i, an integer at every step.
        c = c.checked_mul((n - k + i) as u64)? / i as u64;
    }
    (c <= 1 << 53).then_some(c as f64)
}

/// The Bernstein polynomial `B[i, degree](u)` (A1.2); `0` for `i > degree`.
#[must_use]
pub fn bernstein(degree: usize, i: usize, u: f64) -> f64 {
    if i > degree {
        return 0.0;
    }
    // A1.2: temp[j] = u1 temp[j] + u temp[j-1] for j = degree down to k, written as a
    // forward walk that carries the old temp[j-1].
    let mut temp = vec![0.0; degree + 1];
    if let Some(t) = temp.get_mut(degree - i) {
        *t = 1.0;
    }
    let u1 = 1.0 - u;
    for k in 1..=degree {
        let mut prev = None;
        for t in temp.iter_mut().skip(k - 1) {
            let here = *t;
            if let Some(prev) = prev {
                *t = u1 * here + u * prev;
            }
            prev = Some(here);
        }
    }
    temp.last().copied().unwrap_or(0.0)
}

/// The degree elevation matrix from `p` to `p + t`, rows `0..=p+t` of `p + 1` entries:
/// `[i][j] = C(p, j) C(t, i - j) / C(p + t, i)` on the band `max(0, i - t) <= j <= min(p, i)`,
/// zero off it. The lower half mirrors the upper (`[n-i][p-j]`), as the pin builds it.
pub(crate) fn elevation_matrix(p: usize, t: usize) -> Result<Vec<Vec<f64>>> {
    let n = p + t;
    if n > MAX_DEGREE {
        return Err(NurbsError::Degree {
            degree: n,
            max: MAX_DEGREE,
        });
    }
    let c = |a, b| {
        binomial(a, b).ok_or(NurbsError::Degree {
            degree: n,
            max: MAX_DEGREE,
        })
    };
    let mut m = vec![vec![0.0; p + 1]; n + 1];
    let half = n / 2;
    for (i, row) in m.iter_mut().enumerate().take(half + 1) {
        if i == 0 {
            if let Some(v) = row.first_mut() {
                *v = 1.0;
            }
            continue;
        }
        let inv = 1.0 / c(n, i)?;
        for (j, v) in row
            .iter_mut()
            .enumerate()
            .take(i.min(p) + 1)
            .skip(i.saturating_sub(t))
        {
            *v = inv * c(p, j)? * c(t, i - j)?;
        }
    }
    for i in half + 1..=n {
        let (upper, lower) = m.split_at_mut(i);
        let (Some(row), Some(mirror)) = (lower.first_mut(), upper.get(n - i)) else {
            continue;
        };
        for (v, &w) in row.iter_mut().zip(mirror.iter().rev()) {
            *v = w;
        }
    }
    Ok(m)
}

/// `out[i] = sum_j dm[i][j] pw[j]` for `i` in `first..out.len()`: the control points of the
/// Bezier piece `pw` elevated by the matrix `dm` (`N_BezElevateDegree`).
pub(crate) fn elevate_into<P: Control>(
    pw: &[P],
    t: usize,
    dm: &[Vec<f64>],
    first: usize,
    out: &mut [P],
) {
    for (i, (slot, row)) in out.iter_mut().zip(dm).enumerate().skip(first) {
        let lo = i.saturating_sub(t);
        let mut c = P::ZERO;
        for (&m, &q) in row.iter().zip(pw).skip(lo).take(i + 1 - lo) {
            c = c.blend(m, q);
        }
        *slot = c;
    }
}

/// The control points of the Bezier curve `points` (degree `points.len() - 1`) with its
/// degree raised by `times`.
///
/// Errors when there are no points, or when the degree would exceed
/// [`MAX_DEGREE`](crate::MAX_DEGREE).
pub fn elevate<const D: usize>(points: &[[f64; D]], times: usize) -> Result<Vec<[f64; D]>> {
    let Some(p) = points.len().checked_sub(1) else {
        return Err(NurbsError::PointCount {
            count: 0,
            expected: 1,
        });
    };
    let dm = elevation_matrix(p, times)?;
    let mut out = vec![[0.0; D]; p + times + 1];
    elevate_into(points, times, &dm, 0, &mut out);
    Ok(out)
}

/// The control points of two Bezier pieces.
pub type Halves<P> = (Vec<P>, Vec<P>);

/// The two halves of the Bezier curve `points` at `u` by de Casteljau's algorithm: the
/// control points of the pieces over `[0, u]` and `[u, 1]`, reparametrised to `[0, 1]`.
/// A `u` outside `[0, 1]` extends the curve.
///
/// Errors when there are no points or `u` is not finite.
pub fn split<const D: usize>(points: &[[f64; D]], u: f64) -> Result<Halves<[f64; D]>> {
    if !u.is_finite() {
        return Err(NurbsError::ParameterNotFinite { parameter: u });
    }
    split_points(points, u).ok_or(NurbsError::PointCount {
        count: 0,
        expected: 1,
    })
}

/// [`split`] for any control point; `None` for no points.
pub(crate) fn split_points<P: Control>(points: &[P], u: f64) -> Option<Halves<P>> {
    let mut work = points.to_vec();
    let mut left = Vec::with_capacity(points.len());
    let mut right = Vec::with_capacity(points.len());
    left.push(*work.first()?);
    right.push(*work.last()?);
    for k in 1..points.len() {
        let live = points.len() - k;
        for i in 0..live {
            let (Some(&a), Some(&b)) = (work.get(i), work.get(i + 1)) else {
                return None;
            };
            *work.get_mut(i)? = P::combine(1.0 - u, a, u, b);
        }
        left.push(*work.first()?);
        right.push(*work.get(live - 1)?);
    }
    right.reverse();
    Some((left, right))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn binomials() {
        assert_eq!(binomial(0, 0), Some(1.0));
        assert_eq!(binomial(5, 2), Some(10.0));
        assert_eq!(binomial(31, 15), Some(300_540_195.0));
        assert_eq!(binomial(3, 4), Some(0.0));
        assert_eq!(binomial(60, 30), None);
    }

    #[test]
    fn bernstein_values() {
        // B[1,3](u) = 3 u (1-u)^2.
        let u: f64 = 0.25;
        assert!((bernstein(3, 1, u) - 3.0 * u * (1.0 - u) * (1.0 - u)).abs() < 1e-16);
        let sum: f64 = (0..=5).map(|i| bernstein(5, i, 0.3)).sum();
        assert!((sum - 1.0).abs() < 1e-15);
        assert_eq!(bernstein(2, 3, 0.5), 0.0);
        assert_eq!(bernstein(0, 0, 0.7), 1.0);
    }

    #[test]
    fn elevation_rows_sum_to_one_and_keep_the_curve() {
        let m = elevation_matrix(3, 2).unwrap();
        for row in &m {
            assert!((row.iter().sum::<f64>() - 1.0).abs() < 1e-15);
        }
        let pts = [[0.0, 0.0], [1.0, 2.0], [3.0, 2.0], [4.0, 0.0]];
        let up = elevate(&pts, 2).unwrap();
        assert_eq!(up.len(), 6);
        assert_eq!(up.first(), pts.first());
        assert_eq!(up.last(), pts.last());
        let at = |p: &[[f64; 2]], u: f64| {
            let d = p.len() - 1;
            p.iter().enumerate().fold([0.0, 0.0], |a, (i, q)| {
                let b = bernstein(d, i, u);
                [a[0] + b * q[0], a[1] + b * q[1]]
            })
        };
        for u in [0.1, 0.5, 0.8] {
            let (a, b) = (at(&pts, u), at(&up, u));
            assert!((a[0] - b[0]).abs() < 1e-14 && (a[1] - b[1]).abs() < 1e-14);
        }
        assert!(elevate(&pts, 29).is_err());
        assert!(elevate::<2>(&[], 1).is_err());
    }

    #[test]
    fn split_halves() {
        let pts = [[0.0], [1.0], [3.0]];
        let (l, r) = split(&pts, 0.5).unwrap();
        assert_eq!(l, [[0.0], [0.5], [1.25]]);
        assert_eq!(r, [[1.25], [2.0], [3.0]]);
        assert!(split(&pts, f64::NAN).is_err());
    }
}
