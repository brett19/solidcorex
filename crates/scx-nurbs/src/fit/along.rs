// Written for solidcorex. Apache-2.0.
//! A least-squares fit that leaves its start along a given direction and reaches its end along
//! another, how far along each (the *reach*) left free (the pin's `approx_end_directions`).
//!
//! With unit directions `T0` and `T1`, the second control point is `P1 = P0 + s T0` and the
//! last but one `P[n-1] = P[n] - e T1`, the end points held at the first and last datum. The
//! fit is linear in the free control points `X` and the reaches `s`, `e`: each interior datum
//! gives the row `sum_j N_j(t) X_j + s N_1(t) T0 - e N_(n-1)(t) T1 = q - (fixed terms)`.
//!
//! The pin forms the normal equations of the whole system and solves them densely. Here the
//! reaches are projected out instead (variable projection): one banded rotation solve
//! ([`least_squares`](super::least_squares)' solver) gives the free points' least-squares
//! answer `X_q` for the data and, with the same rows, `X_a` and `X_b` for the two reach
//! columns `N_1` and `N_(n-1)`; with the residuals `r_q`, `r_a`, `r_b` of those three, the
//! reaches solve the 2 x 2 system
//!
//! ```text
//! [ sum r_a^2           -c sum r_a r_b ] [s]   [  sum r_a (T0 . r_q) ]
//! [ -c sum r_a r_b      sum r_b^2      ] [e] = [ -sum r_b (T1 . r_q) ]      c = T0 . T1
//! ```
//!
//! and `X = X_q - s X_a T0 + e X_b T1`. No normal equations of the points are formed, so the
//! condition stays the problem's.

use super::FitError;
use super::band::Band;
use super::ends::Spline;
use super::knots::check_increasing;
use super::param::check_finite;
use crate::{Approach, KnotVector, MAX_DEGREE};

/// A least-squares fit with its end directions held ([`least_squares_along`]).
#[derive(Debug, Clone, PartialEq)]
pub struct Directed<const N: usize> {
    /// The fit.
    pub spline: Spline<N>,
    /// How far the second control point is from the first along the start direction, where
    /// one was given: negative when the fit leaves the start against it.
    pub start_reach: Option<f64>,
    /// How far the last control point but one is from the last against the end direction,
    /// where one was given: negative when the fit reaches the end against it.
    pub end_reach: Option<f64>,
}

/// The curve on the clamped `knots` closest to `data` at the parameters `t` in the least-squares
/// sense, through the first and last point, its first leg along `start` and its last along
/// `end` where they are given, each leg's length free.
///
/// The directions are read as unit vectors: any finite non-zero length gives the same fit.
/// With no direction it is [`least_squares`](super::least_squares) with
/// [`End::Point`](super::End::Point) at both ends.
///
/// # Errors
///
/// As [`least_squares`](super::least_squares), and [`FitError::Directions`] for a direction on
/// a degree below 2, [`FitError::Direction`] for one not finite or of no length,
/// [`FitError::TooFewControlPoints`] when the knots give fewer control points than the held
/// ones and one more (four with both directions, three with one), [`FitError::Reaches`] when
/// the reaches have no unique value.
pub fn least_squares_along<const N: usize>(
    knots: &KnotVector,
    data: &[[f64; N]],
    t: &[f64],
    start: Option<[f64; N]>,
    end: Option<[f64; N]>,
) -> Result<Directed<N>, FitError> {
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
    if (start.is_some() || end.is_some()) && p < 2 {
        return Err(FitError::Directions { degree: p });
    }
    let t0 = start.map(unit).transpose()?;
    let t1 = end.map(unit).transpose()?;
    if !knots.is_clamped() {
        return Err(FitError::Unclamped);
    }
    let (lo, hi) = knots.ends();
    if t.first() != Some(&lo) || t.last() != Some(&hi) {
        return Err(FitError::EndsNotOnDomain);
    }
    let count = knots.point_count();
    let head = 1 + usize::from(t0.is_some());
    let tail = 1 + usize::from(t1.is_some());
    if count < head + tail {
        return Err(FitError::TooFewControlPoints {
            count,
            needed: head + tail,
        });
    }
    let (&q0, &qn) = data.first().zip(data.last()).ok_or(FitError::Defect {
        why: "data went missing",
    })?;
    let free = count - head - tail;
    // The columns of the reaches, where held: P[1] and P[count - 2].
    let a_col = t0.map(|_| 1);
    let b_col = t1.map(|_| count - 2);

    let mut rows = Vec::with_capacity(data.len().saturating_sub(2));
    let mut band_q = Band::<N>::new(free, p + 1)?;
    let mut band_ab = Band::<2>::new(free, p + 1)?;
    for (&u, q) in t.iter().zip(data).skip(1).take(data.len() - 2) {
        let basis = knots.basis(u, Approach::Above)?;
        let mut row = Row {
            first: 0,
            coeffs: [0.0; MAX_DEGREE + 1],
            len: 0,
            rhs: *q,
            ab: [0.0; 2],
        };
        let mut at = None;
        for (col, &v) in (basis.first()..).zip(basis.values()) {
            if col == 0 || Some(col) == a_col {
                // P[0], and the fixed part of P[1] = P[0] + s T0.
                sub_scaled(&mut row.rhs, v, &q0);
                if Some(col) == a_col {
                    row.ab[0] += v;
                }
            } else if col == count - 1 || Some(col) == b_col {
                sub_scaled(&mut row.rhs, v, &qn);
                if Some(col) == b_col {
                    row.ab[1] += v;
                }
            } else {
                at.get_or_insert(col - head);
                if let Some(slot) = row.coeffs.get_mut(row.len) {
                    *slot = v;
                }
                row.len += 1;
            }
        }
        if let Some(first) = at {
            row.first = first;
            let coeffs = row.coeffs.get(..row.len).ok_or(FitError::Defect {
                why: "a row longer than the band",
            })?;
            band_q.push(first, coeffs, row.rhs)?;
            band_ab.push(first, coeffs, row.ab)?;
        }
        rows.push(row);
    }
    let x_q = band_q.solve()?;
    let x_ab = band_ab.solve()?;

    // The residuals of the three projections, and the 2 x 2 system of the reaches.
    let (mut saa, mut sbb, mut sab, mut ya, mut yb) = (0.0, 0.0, 0.0, 0.0, 0.0);
    let zero = [0.0; N];
    for row in &rows {
        let mut rq = row.rhs;
        let mut rab = row.ab;
        let cols = row.coeffs.iter().take(row.len);
        for (j, &c) in (row.first..).zip(cols) {
            let (xq, xab) = x_q.get(j).zip(x_ab.get(j)).ok_or(FitError::Defect {
                why: "a free control point is missing",
            })?;
            sub_scaled(&mut rq, c, xq);
            sub_scaled(&mut rab, c, xab);
        }
        let [ra, rb] = rab;
        saa += ra * ra;
        sbb += rb * rb;
        sab += ra * rb;
        ya += ra * dot(&t0.unwrap_or(zero), &rq);
        yb += rb * dot(&t1.unwrap_or(zero), &rq);
    }
    let (s, e) = match (t0, t1) {
        (Some(a), Some(b)) => {
            let c = dot(&a, &b);
            let off = c * sab;
            let det = saa * sbb - off * off;
            if !(det.is_finite() && det > REACH_UNITS * f64::EPSILON * saa * sbb) {
                return Err(FitError::Reaches);
            }
            (
                Some((ya * sbb - off * yb) / det),
                Some((off * ya - saa * yb) / det),
            )
        }
        (Some(_), None) => (Some(reach(ya, saa)?), None),
        (None, Some(_)) => (None, Some(reach(-yb, sbb)?)),
        (None, None) => (None, None),
    };

    let mut points = Vec::with_capacity(count);
    points.push(q0);
    if let (Some(a), Some(s)) = (t0, s) {
        points.push(add_scaled(q0, s, &a));
    }
    for (xq, &[xa, xb]) in x_q.iter().zip(&x_ab) {
        let mut x = *xq;
        if let (Some(a), Some(s)) = (t0, s) {
            sub_scaled(&mut x, s * xa, &a);
        }
        if let (Some(b), Some(e)) = (t1, e) {
            sub_scaled(&mut x, -e * xb, &b);
        }
        points.push(x);
    }
    if let (Some(b), Some(e)) = (t1, e) {
        points.push(add_scaled(qn, -e, &b));
    }
    points.push(qn);
    Ok(Directed {
        spline: Spline::new(knots.clone(), points),
        start_reach: s,
        end_reach: e,
    })
}

/// A 2 x 2 (or 1 x 1) reach system whose determinant is at most this many units of rounding
/// of its diagonal's product has no unique solution: its two columns are parallel to working
/// precision.
const REACH_UNITS: f64 = 16.0;

/// One interior datum's row: the free columns from `first`, the reach columns' coefficients,
/// and the datum less the held points' terms.
struct Row<const N: usize> {
    first: usize,
    coeffs: [f64; MAX_DEGREE + 1],
    len: usize,
    rhs: [f64; N],
    ab: [f64; 2],
}

/// `y / d` for the one-direction system `d s = y`.
fn reach(y: f64, d: f64) -> Result<f64, FitError> {
    let s = y / d;
    if d > 0.0 && s.is_finite() {
        Ok(s)
    } else {
        Err(FitError::Reaches)
    }
}

/// `v / |v|`, or [`FitError::Direction`] for a `v` not finite or of no length.
pub(super) fn unit<const N: usize>(v: [f64; N]) -> Result<[f64; N], FitError> {
    // Scaled by the largest coordinate first, so no square overflows or underflows.
    let big = v.iter().fold(0.0_f64, |m, x| m.max(x.abs()));
    if !(big.is_finite() && big > 0.0) {
        return Err(FitError::Direction);
    }
    let w = v.map(|x| x / big);
    let len = dot(&w, &w).sqrt();
    Ok(w.map(|x| x / len))
}

pub(super) fn dot<const N: usize>(a: &[f64; N], b: &[f64; N]) -> f64 {
    a.iter().zip(b).map(|(x, y)| x * y).sum()
}

/// `x -= k v`.
fn sub_scaled<const N: usize>(x: &mut [f64; N], k: f64, v: &[f64; N]) {
    for (a, b) in x.iter_mut().zip(v) {
        *a -= k * b;
    }
}

/// `x + k v`.
fn add_scaled<const N: usize>(mut x: [f64; N], k: f64, v: &[f64; N]) -> [f64; N] {
    for (a, b) in x.iter_mut().zip(v) {
        *a += k * b;
    }
    x
}
