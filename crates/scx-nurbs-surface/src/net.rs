// Written for solidcorex. Apache-2.0.
//! A control net: a rectangle of values in one row-major buffer.

use crate::{NetCause, NurbsSurfaceError, Result};

/// A non-empty `rows × cols` rectangle of values in one row-major buffer: row `i` holds the
/// values of u index `i`, along v (the pin's `net[i][j]`, a `Vec<Vec<CPoint>>`).
///
/// The shape is fixed when the net is built, so a ragged net cannot exist and every row has
/// `cols` values. Reads go through [`Net::get`], the row and column iterators or the flat
/// buffer, never a checked index that errs.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Net<P> {
    rows: usize,
    cols: usize,
    points: Vec<P>,
}

impl<P> Net<P> {
    /// The net of `rows` rows of `cols` values from `points` in row-major order (value
    /// `[i][j]` at `i * cols + j`).
    ///
    /// Errs with [`NetCause::Size`] when `rows` or `cols` is zero or `points` does not hold
    /// exactly `rows * cols` values.
    pub fn new(rows: usize, cols: usize, points: Vec<P>) -> Result<Self> {
        let fills = rows > 0 && cols > 0 && rows.checked_mul(cols) == Some(points.len());
        if !fills {
            return Err(NurbsSurfaceError::BadNet(NetCause::Size {
                len: points.len(),
                rows,
                cols,
            }));
        }
        Ok(Self { rows, cols, points })
    }

    /// The number of rows (u indices).
    #[must_use]
    pub const fn nrows(&self) -> usize {
        self.rows
    }

    /// The number of columns (v indices), the length of each row.
    #[must_use]
    pub const fn ncols(&self) -> usize {
        self.cols
    }

    /// The value `[i][j]`, or `None` outside the net.
    #[must_use]
    pub fn get(&self, i: usize, j: usize) -> Option<&P> {
        if j >= self.cols {
            return None;
        }
        self.points.get(i.checked_mul(self.cols)?.checked_add(j)?)
    }

    /// Row `i`, the values `[i][0..cols]`, or `None` outside the net.
    #[must_use]
    pub fn row(&self, i: usize) -> Option<&[P]> {
        self.rows().nth(i)
    }

    /// The rows in order.
    pub fn rows(&self) -> core::slice::ChunksExact<'_, P> {
        // `cols > 0` by construction.
        self.points.chunks_exact(self.cols)
    }

    /// Column `j`, the values `[0..rows][j]` in row order, or `None` outside the net.
    #[must_use]
    pub fn column(&self, j: usize) -> Option<impl ExactSizeIterator<Item = &P> + '_> {
        (j < self.cols).then(|| self.points.iter().skip(j).step_by(self.cols))
    }

    /// The values in row-major order.
    #[must_use]
    pub fn points(&self) -> &[P] {
        &self.points
    }

    /// The values in row-major order, by value.
    #[must_use]
    pub fn into_points(self) -> Vec<P> {
        self.points
    }

    /// The net of the same shape with `f` applied to each value.
    #[must_use]
    pub fn map<Q>(&self, f: impl FnMut(&P) -> Q) -> Net<Q> {
        Net {
            rows: self.rows,
            cols: self.cols,
            points: self.points.iter().map(f).collect(),
        }
    }

    /// The `(row, column)` and value of the first value `bad` picks, in row-major order.
    pub(crate) fn find(&self, mut bad: impl FnMut(&P) -> bool) -> Option<(usize, usize, &P)> {
        let (k, p) = self.points.iter().enumerate().find(|(_, p)| bad(p))?;
        Some((k / self.cols, k % self.cols, p))
    }

    /// The net of the same shape combining each value with the one at the same place of
    /// `other`, which has the same shape.
    pub(crate) fn zip_map<Q, R>(&self, other: &Net<Q>, mut f: impl FnMut(&P, &Q) -> R) -> Net<R> {
        Net {
            rows: self.rows,
            cols: self.cols,
            points: self
                .points
                .iter()
                .zip(&other.points)
                .map(|(p, q)| f(p, q))
                .collect(),
        }
    }

    /// The net with rows and columns exchanged: value `[j][i]` is this net's `[i][j]`.
    pub(crate) fn transposed(&self) -> Self
    where
        P: Clone,
    {
        let mut points = Vec::with_capacity(self.points.len());
        for j in 0..self.cols {
            points.extend(self.points.iter().skip(j).step_by(self.cols).cloned());
        }
        Self {
            rows: self.cols,
            cols: self.rows,
            points,
        }
    }

    /// The net with its rows in reverse order (the u direction reversed).
    pub(crate) fn rows_reversed(&self) -> Self
    where
        P: Clone,
    {
        Self {
            rows: self.rows,
            cols: self.cols,
            points: self.rows().rev().flatten().cloned().collect(),
        }
    }

    /// The net with each row in reverse order (the v direction reversed).
    pub(crate) fn columns_reversed(&self) -> Self
    where
        P: Clone,
    {
        Self {
            rows: self.rows,
            cols: self.cols,
            points: self.rows().flat_map(|r| r.iter().rev()).cloned().collect(),
        }
    }

    /// Whether `other` has the same shape.
    pub(crate) const fn same_shape<Q>(&self, other: &Net<Q>) -> bool {
        self.rows == other.rows && self.cols == other.cols
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shape_rows_columns_and_get() {
        let n = Net::new(2, 3, vec![0, 1, 2, 10, 11, 12]).unwrap();
        assert_eq!((n.nrows(), n.ncols()), (2, 3));
        assert_eq!(n.get(1, 2), Some(&12));
        assert_eq!(n.get(0, 3), None);
        assert_eq!(n.get(2, 0), None);
        assert_eq!(n.get(usize::MAX, usize::MAX), None);
        assert_eq!(n.row(1), Some(&[10, 11, 12][..]));
        assert_eq!(n.row(2), None);
        let rows: Vec<&[i32]> = n.rows().collect();
        assert_eq!(rows, [&[0, 1, 2][..], &[10, 11, 12][..]]);
        assert_eq!(n.rows().len(), 2);
        assert_eq!(n.column(1).unwrap().copied().collect::<Vec<_>>(), [1, 11]);
        assert!(n.column(3).is_none());
        assert_eq!(n.find(|&x| x > 10), Some((1, 1, &11)));
        assert_eq!(n.map(|x| x * 2).points(), [0, 2, 4, 20, 22, 24]);
        assert_eq!(n.clone().into_points(), n.points());
        assert_eq!(n.transposed().points(), [0, 10, 1, 11, 2, 12]);
        assert_eq!((n.transposed().nrows(), n.transposed().ncols()), (3, 2));
        assert_eq!(n.transposed().transposed(), n);
        assert_eq!(n.rows_reversed().points(), [10, 11, 12, 0, 1, 2]);
        assert_eq!(n.columns_reversed().points(), [2, 1, 0, 12, 11, 10]);
    }

    #[test]
    fn a_net_must_be_a_full_rectangle() {
        for (rows, cols, len) in [(2, 3, 5), (2, 3, 7), (0, 3, 0), (3, 0, 0), (0, 0, 0)] {
            assert_eq!(
                Net::new(rows, cols, vec![0.0; len]),
                Err(NurbsSurfaceError::BadNet(NetCause::Size {
                    len,
                    rows,
                    cols
                }))
            );
        }
        // rows * cols overflows
        assert!(Net::new(usize::MAX, 2, vec![0u8; 2]).is_err());
    }
}
