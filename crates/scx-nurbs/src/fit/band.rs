// Written for solidcorex. Apache-2.0.
//! A banded least-squares solver by Givens rotations, for any number of right-hand sides.
//!
//! A B-spline collocation row has at most `p + 1` neighbouring nonzeros, so the matrix of an
//! interpolation or a fit has bandwidth `p`, and its triangular factor `R` does too. Rows are
//! pushed one at a time (in any order; the cost is lowest in order of first column) and each
//! is rotated into `R`, touching at most `p + 1` rows of `R` of `p + 1` entries: `O(p^2)` per
//! row plus `O(p N)` for `N` right-hand sides, never a function of the number of columns.
//! The solve is a back substitution over the band, `O(columns * p)`.
//!
//! This replaces the pin's dense `RealMatrix` LU (`O(n^2)` storage and `O(n^3)` time for `n`
//! control points) and, for least squares, its normal equations `N^T N`, whose condition
//! number is the square of the problem's: a rotation never squares it. A square system is
//! solved the same way (the rotations are orthogonal, so no pivoting is needed).

use scx_num::fmath::hypot;

use super::FitError;
use crate::MAX_DEGREE;

/// The widest row: `MAX_DEGREE + 1` entries.
const WIDEST: usize = MAX_DEGREE + 1;

/// A pivot at most this many units of `columns * EPSILON * scale` is zero, where `scale` is the
/// largest entry pushed: a rank test of the kind LAPACK's rank-revealing routines use. A
/// genuinely rank-deficient system leaves a pivot of the order of one unit.
const PIVOT_UNITS: f64 = 1.0;

/// The triangular factor of the rows pushed so far, and the rotated right-hand sides.
pub(crate) struct Band<const N: usize> {
    columns: usize,
    width: usize,
    /// Row `j` holds `R[j][j..j + width]`, `width` entries.
    r: Vec<f64>,
    y: Vec<[f64; N]>,
    filled: Vec<bool>,
    scale: f64,
}

impl<const N: usize> Band<N> {
    /// An empty system of `columns` unknowns whose rows have at most `width` entries
    /// (`1..=MAX_DEGREE + 1`).
    pub(crate) fn new(columns: usize, width: usize) -> Result<Self, FitError> {
        if width == 0 || width > WIDEST {
            return Err(FitError::Defect {
                why: "a band wider than the highest degree supports",
            });
        }
        Ok(Self {
            columns,
            width,
            r: vec![0.0; columns * width],
            y: vec![[0.0; N]; columns],
            filled: vec![false; columns],
            scale: 0.0,
        })
    }

    /// Adds the equation `sum_k coeffs[k] x[first + k] = rhs`.
    pub(crate) fn push(
        &mut self,
        first: usize,
        coeffs: &[f64],
        rhs: [f64; N],
    ) -> Result<(), FitError> {
        if coeffs.len() > self.width || first + coeffs.len() > self.columns {
            return Err(FitError::Defect {
                why: "a row outside the band",
            });
        }
        let mut w = [0.0; WIDEST];
        for (dst, &src) in w.iter_mut().zip(coeffs) {
            *dst = src;
            self.scale = self.scale.max(src.abs());
        }
        let mut y = rhs;
        let mut lead = first;
        while lead < self.columns {
            let Some((head, _)) = w.split_first() else {
                break;
            };
            let head = *head;
            if head != 0.0 {
                let at = lead * self.width;
                let row = self
                    .r
                    .get_mut(at..at + self.width)
                    .ok_or(FitError::Defect {
                        why: "a band row is missing",
                    })?;
                if !self.filled.get(lead).copied().unwrap_or(false) {
                    for (dst, &src) in row.iter_mut().zip(&w) {
                        *dst = src;
                    }
                    *self.y.get_mut(lead).ok_or(FitError::Defect {
                        why: "a band right-hand side is missing",
                    })? = y;
                    if let Some(f) = self.filled.get_mut(lead) {
                        *f = true;
                    }
                    return Ok(());
                }
                let pivot = row.first().copied().unwrap_or(0.0);
                let h = hypot(pivot, head);
                let (c, s) = (pivot / h, head / h);
                for (r, w) in row.iter_mut().zip(w.iter_mut()) {
                    let (a, b) = (*r, *w);
                    *r = c * a + s * b;
                    *w = c * b - s * a;
                }
                let yr = self.y.get_mut(lead).ok_or(FitError::Defect {
                    why: "a band right-hand side is missing",
                })?;
                for (r, w) in yr.iter_mut().zip(y.iter_mut()) {
                    let (a, b) = (*r, *w);
                    *r = c * a + s * b;
                    *w = c * b - s * a;
                }
            }
            // The leading entry is now zero: shift the window one column right.
            w.rotate_left(1);
            if let Some(last) = w.last_mut() {
                *last = 0.0;
            }
            lead += 1;
        }
        Ok(())
    }

    /// The solution of the system by back substitution over the band.
    ///
    /// Errors with [`FitError::Singular`] at the last unknown without a pivot.
    pub(crate) fn solve(self) -> Result<Vec<[f64; N]>, FitError> {
        let tolerance = PIVOT_UNITS * self.columns as f64 * f64::EPSILON * self.scale;
        let mut x = vec![[0.0; N]; self.columns];
        for j in (0..self.columns).rev() {
            let at = j * self.width;
            let row = self.r.get(at..at + self.width).ok_or(FitError::Defect {
                why: "a band row is missing",
            })?;
            let (&pivot, rest) = row.split_first().ok_or(FitError::Defect {
                why: "a band row is empty",
            })?;
            let filled = self.filled.get(j).copied().unwrap_or(false);
            if !filled || pivot.abs().is_nan() || pivot.abs() <= tolerance {
                return Err(FitError::Singular { column: j });
            }
            let mut acc = self.y.get(j).copied().ok_or(FitError::Defect {
                why: "a band right-hand side is missing",
            })?;
            let (head, tail) = x.split_at_mut(j + 1);
            for (&c, xk) in rest.iter().zip(tail.iter()) {
                for (a, &v) in acc.iter_mut().zip(xk) {
                    *a -= c * v;
                }
            }
            let xj = head.last_mut().ok_or(FitError::Defect {
                why: "a band solution is missing",
            })?;
            *xj = acc.map(|a| a / pivot);
        }
        Ok(x)
    }
}
