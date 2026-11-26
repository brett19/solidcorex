// Moved from solidcore `solidcore/crates/solidcore/src/geom/geom_utility/p1.rs` (l.1652-1758)
// and `solidcore/crates/solidcore/src/geom/geom_utility/p2.rs` (l.318-323, l.1041-1102) at
// commit 03e16c9ee766de5b9b1ced845e5149d62ff493b8.
// Upstream record (verbatim from those files): p1.rs: Upstream:
// `source/SMLib/src/SmGeomUtility.cpp`, `source/SMLib/inc/SmGeomUtility.h`. p2.rs: Upstream:
// `source/SMLib/src/SmGeomUtility.cpp`, `source/SMLib/inc/SmGeomUtility.h` (enums from
// `source/SMLib/inc/SmTypes.h`).
// Changes: only the 3x3 and 4x4 helpers the transform needs, gathered in one file;
// `SolidError::geom` became `NumError::failed`; operation strings kept as solidcore's.
//! Small fixed-size matrix helpers of `SmGeomUtility` (`smgu_VecTripleProduct`,
//! `smgu_3x3Determinate`, `smgu_3x3Inverse`, `smgu_4x4Determinate`, `smgu_4x4Mult`,
//! `smgu_4x4Inverse`), on row-major arrays.
//!
//! Upstream: `source/SMLib/src/SmGeomUtility.cpp`, `source/SMLib/inc/SmGeomUtility.h`.

use crate::error::{NumError, NumFailure, Result};
use crate::tol::is_zero;

/// `a · (b × c)` of three 3-vectors given as arrays (`smgu_VecTripleProduct`).
#[must_use]
pub fn vec_triple_product(a: &[f64; 3], b: &[f64; 3], c: &[f64; 3]) -> f64 {
    let [a0, a1, a2] = *a;
    let [b0, b1, b2] = *b;
    let [c0, c1, c2] = *c;
    a0 * (b1 * c2 - c1 * b2) + a1 * (b2 * c0 - c2 * b0) + a2 * (b0 * c1 - c0 * b1)
}

/// Determinant of the 3x3 matrix with rows `(a00 a01 a02)`, `(a10 a11 a12)`,
/// `(a20 a21 a22)` (`smgu_3x3Determinate`, coefficient overload).
#[must_use]
#[allow(clippy::too_many_arguments)]
pub fn determinate_3x3(
    a00: f64,
    a01: f64,
    a02: f64,
    a10: f64,
    a11: f64,
    a12: f64,
    a20: f64,
    a21: f64,
    a22: f64,
) -> f64 {
    a00 * (a11 * a22 - a12 * a21) + a01 * (a12 * a20 - a10 * a22) + a02 * (a10 * a21 - a11 * a20)
}

/// Inverse of the 3x3 matrix with rows `a0`, `a1`, `a2` (`smgu_3x3Inverse`).
///
/// # Errors
///
/// [`NumFailure::Singular`] (`SM_ERR`, 1001) when the determinant is zero (`SM_IS_ZERO`).
pub fn inverse_3x3(a0: &[f64; 3], a1: &[f64; 3], a2: &[f64; 3]) -> Result<[[f64; 3]; 3]> {
    let det = vec_triple_product(a0, a1, a2);

    if is_zero(det) {
        return Err(NumError::failed(
            "geom::geom_utility::p1::inverse_3x3",
            NumFailure::Singular { measure: det },
        ));
    }

    let [a00, a01, a02] = *a0;
    let [a10, a11, a12] = *a1;
    let [a20, a21, a22] = *a2;

    Ok([
        [
            (a22 * a11 - a21 * a12) / det,
            -(a22 * a01 - a21 * a02) / det,
            (a12 * a01 - a11 * a02) / det,
        ],
        [
            -(a22 * a10 - a20 * a12) / det,
            (a22 * a00 - a20 * a02) / det,
            -(a12 * a00 - a10 * a02) / det,
        ],
        [
            (a21 * a10 - a20 * a11) / det,
            -(a21 * a00 - a20 * a01) / det,
            (a11 * a00 - a10 * a01) / det,
        ],
    ])
}

/// Determinant of a 4x4 matrix by cofactor expansion along the first row
/// (`smgu_4x4Determinate`).
#[must_use]
pub fn determinate_4x4(a: &[[f64; 4]; 4]) -> f64 {
    let [[a00, a01, a02, a03], [a10, a11, a12, a13], [a20, a21, a22, a23], [a30, a31, a32, a33]] =
        *a;
    a00 * (a11 * (a22 * a33 - a23 * a32)
        + a12 * (a23 * a31 - a21 * a33)
        + a13 * (a21 * a32 - a22 * a31))
        - a01
            * (a10 * (a22 * a33 - a23 * a32)
                + a12 * (a23 * a30 - a20 * a33)
                + a13 * (a20 * a32 - a22 * a30))
        + a02
            * (a10 * (a21 * a33 - a23 * a31)
                + a11 * (a23 * a30 - a20 * a33)
                + a13 * (a20 * a31 - a21 * a30))
        - a03
            * (a10 * (a21 * a32 - a22 * a31)
                + a11 * (a22 * a30 - a20 * a32)
                + a12 * (a20 * a31 - a21 * a30))
}

/// `smgu_3x3Determinate` (coefficient overload) called with the nine entries of `m`
/// row by row.
fn determinant_3x3(m: &[[f64; 3]; 3]) -> f64 {
    let [[a00, a01, a02], [a10, a11, a12], [a20, a21, a22]] = *m;
    determinate_3x3(a00, a01, a02, a10, a11, a12, a20, a21, a22)
}

/// `a * b` for 4x4 row-major matrices (`smgu_4x4Mult`).
#[must_use]
pub fn mult_4x4(a: &[[f64; 4]; 4], b: &[[f64; 4]; 4]) -> [[f64; 4]; 4] {
    let mut out = [[0.0; 4]; 4];
    for (out_row, a_row) in out.iter_mut().zip(a) {
        for (jj, cell) in out_row.iter_mut().enumerate() {
            let mut sum = 0.0;
            for (a_k, b_row) in a_row.iter().zip(b) {
                if let Some(b_kj) = b_row.get(jj) {
                    sum += a_k * b_kj;
                }
            }
            *cell = sum;
        }
    }
    out
}

/// Inverse of a 4x4 matrix through cofactors (`smgu_4x4Inverse`).
///
/// # Errors
///
/// [`NumFailure::Singular`] (`SM_ERR`) when the determinant is within `SM_EFF_ZERO` of
/// zero.
pub fn inverse_4x4(a: &[[f64; 4]; 4]) -> Result<[[f64; 4]; 4]> {
    let det = determinate_4x4(a);
    if is_zero(det) {
        return Err(NumError::failed(
            "geom::geom_utility::p2::inverse_4x4",
            NumFailure::Singular { measure: det },
        ));
    }

    // inv[r][c] = (-1)^(r+c) * det(minor without row c, column r) / det
    let mut inv = [[0.0; 4]; 4];
    for (r, inv_row) in inv.iter_mut().enumerate() {
        for (c, cell) in inv_row.iter_mut().enumerate() {
            let minor = minor_3x3(a, c, r);
            let cofactor = determinant_3x3(&minor);
            let signed = if (r + c) % 2 == 1 {
                -cofactor
            } else {
                cofactor
            };
            *cell = signed / det;
        }
    }
    Ok(inv)
}

/// `a` without row `row` and column `col`, remaining entries in order.
fn minor_3x3(a: &[[f64; 4]; 4], row: usize, col: usize) -> [[f64; 3]; 3] {
    let mut m = [[0.0; 3]; 3];
    let rows = a.iter().enumerate().filter(|&(i, _)| i != row);
    for (m_row, (_, a_row)) in m.iter_mut().zip(rows) {
        let cols = a_row.iter().enumerate().filter(|&(j, _)| j != col);
        for (cell, (_, &v)) in m_row.iter_mut().zip(cols) {
            *cell = v;
        }
    }
    m
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn small_matrices() {
        let m = [[2.0, 0.0, 0.0], [0.0, 4.0, 0.0], [0.0, 0.0, 8.0]];
        assert_eq!(determinate_3x3(2., 0., 0., 0., 4., 0., 0., 0., 8.), 64.0);
        let inv = inverse_3x3(&m[0], &m[1], &m[2]).unwrap();
        assert_eq!(inv, [[0.5, 0.0, 0.0], [0.0, 0.25, 0.0], [0.0, 0.0, 0.125]]);
        assert!(inverse_3x3(&[1., 0., 0.], &[1., 0., 0.], &[0., 0., 1.]).is_err());
        let mut id4 = [[0.0; 4]; 4];
        for (i, row) in id4.iter_mut().enumerate() {
            row[i] = 2.0;
        }
        assert_eq!(determinate_4x4(&id4), 16.0);
        assert_eq!(
            vec_triple_product(&[1., 0., 0.], &[0., 1., 0.], &[0., 0., 1.]),
            1.0
        );
    }

    #[test]
    fn mult_4x4_identity_and_product() {
        let id = [
            [1.0, 0.0, 0.0, 0.0],
            [0.0, 1.0, 0.0, 0.0],
            [0.0, 0.0, 1.0, 0.0],
            [0.0, 0.0, 0.0, 1.0],
        ];
        let a = [
            [1.0, 2.0, 3.0, 4.0],
            [5.0, 6.0, 7.0, 8.0],
            [9.0, 10.0, 11.0, 12.0],
            [13.0, 14.0, 15.0, 16.0],
        ];
        assert_eq!(mult_4x4(&a, &id), a);
        assert_eq!(mult_4x4(&id, &a), a);
        let sq = mult_4x4(&a, &a);
        assert_eq!(sq[0], [90.0, 100.0, 110.0, 120.0]);
        assert_eq!(sq[3], [426.0, 484.0, 542.0, 600.0]);
        let zero = mult_4x4(&[[1.0; 4]; 4], &[[0.0; 4]; 4]);
        assert!(inverse_4x4(&zero).is_err());
    }

    #[test]
    fn minor_skips_row_and_column() {
        let a = [
            [1.0, 2.0, 3.0, 4.0],
            [5.0, 6.0, 7.0, 8.0],
            [9.0, 10.0, 11.0, 12.0],
            [13.0, 14.0, 15.0, 16.0],
        ];
        assert_eq!(
            minor_3x3(&a, 1, 0),
            [[2.0, 3.0, 4.0], [10.0, 11.0, 12.0], [14.0, 15.0, 16.0]]
        );
    }
}
