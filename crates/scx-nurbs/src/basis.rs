// Written for solidcorex. Apache-2.0.
//! The non-vanishing B-spline basis functions at a parameter, and their derivatives.
//!
//! The kernels follow solidcore's `nurbs_crv::basis_eval_into` (A2.2) and
//! `nurbs_crv::basis_derivs_into` (A2.3) at the pin, in the same arithmetic order, on scratch
//! arrays of width `N` on the stack: [`SMALL`] for degrees below it, else
//! [`MAX_DEGREE`]` + 1`. The pin fails where a knot difference is below `1e-24` and at the
//! domain ends; here the kernels run everywhere and the caller checks that what they
//! produced is finite, since a difference that rounds to zero divides into an infinity or a
//! NaN that reaches every value it touches.

use crate::{Approach, KnotVector, MAX_DEGREE, MAX_ORDER, NurbsError, Result};

/// The scratch width for degrees below it (the pin's `SMALL_DEGREE`).
pub(crate) const SMALL: usize = 8;

/// The scratch width for every degree.
pub(crate) const FULL: usize = MAX_DEGREE + 1;

/// A knot or scratch index the span from [`KnotVector::span`] and `p < N` rule out.
pub(crate) const MISSING_KNOT: NurbsError = NurbsError::Defect {
    why: "a basis read a knot or scratch index its span rules out",
};

/// The `p + 1` basis values `N[span-p..=span]` at `u` into `n[..=p]`, for `p < N` and the
/// window of [`KnotVector::window`]. The rest of `n` is left as it is.
pub(crate) fn values_into<const N: usize>(window: (&[f64], &[f64]), u: f64, n: &mut [f64; N]) {
    // The scratch is [`SMALL`] wide below that degree whatever `N` is (a `Basis` holds `FULL`
    // values): the arithmetic is the same, and clearing `FULL`-wide scratch for a cubic costs
    // more than the cubic.
    if window.1.len() < SMALL {
        values_with::<SMALL, N>(window, u, n);
    } else {
        values_with::<N, N>(window, u, n);
    }
}

/// [`values_into`] with scratch arrays of `M > p` entries.
fn values_with<const M: usize, const N: usize>(
    (below, above): (&[f64], &[f64]),
    u: f64,
    n: &mut [f64; N],
) {
    // left[j-1] = u - U[span+1-j], right[j-1] = U[span+j] - u, for j in 1..=p.
    let mut left = [0.0; M];
    let mut right = [0.0; M];
    for (l, k) in left.iter_mut().zip(below.iter().rev()) {
        *l = u - k;
    }
    for (r, k) in right.iter_mut().zip(above) {
        *r = k - u;
    }
    // n[..=p] is written below before it is read; the rest is left as it is.
    if let Some(first) = n.first_mut() {
        *first = 1.0;
    }
    // Indexed rather than zipped: a cubic's loop is short enough that the adapters cost more
    // than the arithmetic. Every index is below `p + 1 <= min(M, N)`.
    for j in 1..=above.len() {
        let mut saved = 0.0;
        for r in 0..j {
            let (Some(&rr), Some(&ll), Some(value)) =
                (right.get(r), left.get(j - 1 - r), n.get_mut(r))
            else {
                break;
            };
            let delta = rr + ll;
            let temp = *value;
            *value = saved + temp * (rr / delta);
            saved = temp * (ll / delta);
        }
        if let Some(value) = n.get_mut(j) {
            *value = saved;
        }
    }
}

/// The `p + 1` basis values at `u` into `values[..=p]` and their `k`-th derivatives into
/// `derivs[k - 1][..=p]`, for every row of `derivs`. Rows above `p` are zero. `None` only for
/// an index past a scratch array, which `p < N` and a span from [`KnotVector::span`] rule out.
///
/// The values and the derivatives are separate so that a caller can hold `D` derivative rows
/// beside the values, which stable Rust cannot size as one `[_; D + 1]`.
pub(crate) fn derivs_into<const N: usize>(
    window: (&[f64], &[f64]),
    u: f64,
    values: &mut [f64; N],
    derivs: &mut [[f64; N]],
) -> Option<()> {
    // As in `values_into`: a `FULL × FULL` table is 8 KiB to clear.
    if window.1.len() < SMALL {
        derivs_with::<SMALL, N>(window, u, values, derivs)
    } else {
        derivs_with::<N, N>(window, u, values, derivs)
    }
}

/// [`derivs_into`] with scratch arrays of `M > p` entries.
fn derivs_with<const M: usize, const N: usize>(
    (below, above): (&[f64], &[f64]),
    u: f64,
    values: &mut [f64; N],
    derivs: &mut [[f64; N]],
) -> Option<()> {
    let p = above.len();
    let mut left = [0.0; M];
    let mut right = [0.0; M];
    for (l, k) in left.iter_mut().skip(1).zip(below.iter().rev()) {
        *l = u - k;
    }
    for (r, k) in right.iter_mut().skip(1).zip(above) {
        *r = k - u;
    }

    // ndu[r][j] (r <= j): the basis functions of degree j. ndu[j][r] (j > r): the knot
    // differences they divide by.
    let mut ndu = [[0.0; M]; M];
    put(&mut ndu, 0, 0, 1.0)?;
    for j in 1..=p {
        let mut saved = 0.0;
        for r in 0..j {
            let rr = *right.get(r + 1)?;
            let ll = *left.get(j - r)?;
            let den = rr + ll;
            put(&mut ndu, j, r, den)?;
            let prev = at(&ndu, r, j - 1)?;
            put(&mut ndu, r, j, saved + prev * (rr / den))?;
            saved = prev * (ll / den);
        }
        put(&mut ndu, j, j, saved)?;
    }

    *values = [0.0; N];
    derivs.fill([0.0; N]);
    for (j, v) in values.iter_mut().enumerate().take(p + 1) {
        *v = at(&ndu, j, p)?;
    }

    let order = p.min(derivs.len());
    for r in 0..=p {
        let mut rows = [[0.0; M]; 2];
        let [first, second] = &mut rows;
        let (mut s1, mut s2) = (first, second);
        *s1.first_mut()? = 1.0;
        for k in 1..=order {
            let mut d = 0.0;
            let pk = p - k;
            if r >= k {
                let rk = r - k;
                let v = *s1.first()? / at(&ndu, pk + 1, rk)?;
                *s2.first_mut()? = v;
                d = v * at(&ndu, rk, pk)?;
            }
            let j1 = if r + 1 >= k { 1 } else { k - r };
            let j2 = if r <= pk + 1 { k - 1 } else { p - r };
            for j in j1..=j2 {
                let rkj = r + j - k;
                let v = (*s1.get(j)? - *s1.get(j - 1)?) / at(&ndu, pk + 1, rkj)?;
                *s2.get_mut(j)? = v;
                d += v * at(&ndu, rkj, pk)?;
            }
            if r <= pk {
                let v = -*s1.get(k - 1)? / at(&ndu, pk + 1, r)?;
                *s2.get_mut(k)? = v;
                d += v * at(&ndu, r, pk)?;
            }
            *derivs.get_mut(k - 1)?.get_mut(r)? = d;
            (s1, s2) = (s2, s1);
        }
    }

    // Multiply through by p! / (p - k)!.
    let mut factor = p;
    for (k, row) in (1..).zip(derivs.iter_mut().take(order)) {
        for v in row.iter_mut().take(p + 1) {
            *v *= factor as f64;
        }
        factor *= p - k;
    }
    Some(())
}

fn at<const N: usize>(m: &[[f64; N]; N], r: usize, c: usize) -> Option<f64> {
    m.get(r)?.get(c).copied()
}

fn put<const N: usize>(m: &mut [[f64; N]; N], r: usize, c: usize, v: f64) -> Option<()> {
    *m.get_mut(r)?.get_mut(c)? = v;
    Some(())
}

/// The non-vanishing basis functions of a knot vector at a parameter: `values()[j]` is
/// `N[first() + j]`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Basis {
    span: usize,
    degree: usize,
    values: [f64; FULL],
}

impl Basis {
    /// The basis functions of `knots` at `u`, in the span `side` picks ([`KnotVector::span`]).
    #[inline]
    pub(crate) fn at(knots: &KnotVector, u: f64, side: Approach) -> Result<Self> {
        let span = knots.span(u, side)?;
        let window = knots.window(span).ok_or(MISSING_KNOT)?;
        let mut basis = Self {
            span,
            degree: knots.degree(),
            values: [0.0; FULL],
        };
        values_into(window, u, &mut basis.values);
        if basis.values().iter().any(|v| !v.is_finite()) {
            return Err(NurbsError::Degenerate { parameter: u });
        }
        Ok(basis)
    }

    /// The span index `i` ([`KnotVector::span`]).
    #[must_use]
    pub const fn span(&self) -> usize {
        self.span
    }

    /// The index of the first non-vanishing function, `span - degree`.
    #[must_use]
    #[inline]
    pub const fn first(&self) -> usize {
        self.span - self.degree
    }

    /// The `degree + 1` values. They are non-negative and sum to one inside the domain.
    #[must_use]
    #[inline]
    pub fn values(&self) -> &[f64] {
        // degree <= MAX_DEGREE < FULL, by KnotVector::new.
        self.values.split_at(self.degree + 1).0
    }
}

/// The non-vanishing basis functions of a knot vector at a parameter and their derivatives
/// up to order `K - 1`: `row(k)[j]` is the `k`-th derivative of `N[first() + j]`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BasisDerivs<const K: usize> {
    span: usize,
    degree: usize,
    rows: [[f64; FULL]; K],
}

impl<const K: usize> BasisDerivs<K> {
    /// The basis functions of `knots` at `u` and their derivatives.
    pub(crate) fn at(knots: &KnotVector, u: f64, side: Approach) -> Result<Self> {
        check_order(K)?;
        let span = knots.span(u, side)?;
        let window = knots.window(span).ok_or(MISSING_KNOT)?;
        let mut rows = [[0.0; FULL]; K];
        if let Some((values, derivs)) = rows.split_first_mut() {
            derivs_into(window, u, values, derivs).ok_or(MISSING_KNOT)?;
        }
        if rows.iter().flatten().any(|v| !v.is_finite()) {
            return Err(NurbsError::Degenerate { parameter: u });
        }
        Ok(Self {
            span,
            degree: knots.degree(),
            rows,
        })
    }

    /// The span index `i` ([`KnotVector::span`]).
    #[must_use]
    pub const fn span(&self) -> usize {
        self.span
    }

    /// The index of the first non-vanishing function, `span - degree`.
    #[must_use]
    pub const fn first(&self) -> usize {
        self.span - self.degree
    }

    /// The `k`-th derivatives of the `degree + 1` functions, for `k < K`.
    #[must_use]
    pub fn row(&self, k: usize) -> Option<&[f64]> {
        self.rows.get(k)?.get(..=self.degree)
    }
}

/// The non-vanishing basis functions of a knot vector at a parameter and their derivatives
/// up to order `D` inclusive: `row(k)[j]` is the `k`-th derivative of `N[first() + j]`, for
/// `k <= D`.
///
/// [`BasisDerivs`] with the order counted as a surface jet's is (`D` is the highest order, not
/// the number of rows), so that a jet of order `D` can hold its `D + 1` rows: stable Rust has
/// no `BasisDerivs<{ D + 1 }>`. The rows are bit for bit those of `BasisDerivs<{ D + 1 }>`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BasisJet<const D: usize> {
    span: usize,
    degree: usize,
    values: [f64; FULL],
    derivs: [[f64; FULL]; D],
}

impl<const D: usize> BasisJet<D> {
    /// The basis functions of `knots` at `u` and their derivatives.
    pub(crate) fn at(knots: &KnotVector, u: f64, side: Approach) -> Result<Self> {
        check_order(D.saturating_add(1))?;
        let span = knots.span(u, side)?;
        let window = knots.window(span).ok_or(MISSING_KNOT)?;
        let mut values = [0.0; FULL];
        let mut derivs = [[0.0; FULL]; D];
        derivs_into(window, u, &mut values, &mut derivs).ok_or(MISSING_KNOT)?;
        if values
            .iter()
            .chain(derivs.iter().flatten())
            .any(|v| !v.is_finite())
        {
            return Err(NurbsError::Degenerate { parameter: u });
        }
        Ok(Self {
            span,
            degree: knots.degree(),
            values,
            derivs,
        })
    }

    /// The span index `i` ([`KnotVector::span`]).
    #[must_use]
    pub const fn span(&self) -> usize {
        self.span
    }

    /// The index of the first non-vanishing function, `span - degree`.
    #[must_use]
    pub const fn first(&self) -> usize {
        self.span - self.degree
    }

    /// The `k`-th derivatives of the `degree + 1` functions (`k = 0`: the values), for
    /// `k <= D`.
    #[must_use]
    pub fn row(&self, k: usize) -> Option<&[f64]> {
        let row = match k.checked_sub(1) {
            None => &self.values,
            Some(k) => self.derivs.get(k)?,
        };
        row.get(..=self.degree)
    }
}

/// Checks that `K` rows (orders `0..K`) are supported.
pub(crate) const fn check_order(rows: usize) -> Result<()> {
    if rows > MAX_ORDER + 1 {
        return Err(NurbsError::DerivativeOrder {
            order: rows - 1,
            max: MAX_ORDER,
        });
    }
    Ok(())
}

impl KnotVector {
    /// The non-vanishing basis functions at `u` (A2.2), in the span `side` picks.
    ///
    /// Errors when `u` is not finite, or when a knot difference rounds to zero or a value
    /// overflows ([`NurbsError::Degenerate`]).
    #[inline]
    pub fn basis(&self, u: f64, side: Approach) -> Result<Basis> {
        Basis::at(self, u, side)
    }

    /// The non-vanishing basis functions at `u` and their derivatives up to order `K - 1`
    /// (A2.3). Errors as [`KnotVector::basis`], and when `K - 1 > MAX_ORDER`.
    pub fn basis_derivs<const K: usize>(&self, u: f64, side: Approach) -> Result<BasisDerivs<K>> {
        BasisDerivs::at(self, u, side)
    }

    /// The non-vanishing basis functions at `u` and their derivatives up to order `D`
    /// inclusive (A2.3): [`KnotVector::basis_derivs`]`::<{ D + 1 }>`. Errors as it does.
    pub fn basis_jet<const D: usize>(&self, u: f64, side: Approach) -> Result<BasisJet<D>> {
        BasisJet::at(self, u, side)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use scx_testkit::nurbs as tk;

    /// The exact basis functions `N[span-p..=span]` at `u`: the curves whose control values
    /// are the unit vectors, evaluated by the testkit in rationals.
    fn exact(p: usize, knots: &[f64], u: f64, side: Approach) -> Vec<f64> {
        let kv = KnotVector::new(p, knots.to_vec()).unwrap();
        let first = kv.basis(u, side).unwrap().first();
        let n = kv.point_count();
        let tside = match side {
            Approach::Below => tk::Side::Below,
            Approach::Above => tk::Side::Above,
        };
        (first..=first + p)
            .map(|i| {
                let pts: Vec<[f64; 1]> = (0..n).map(|j| [f64::from(u8::from(i == j))]).collect();
                let c = tk::Curve::new(p, knots, &pts, None).unwrap();
                c.point(u, tside).unwrap().rounded[0]
            })
            .collect()
    }

    /// The narrow scratch below [`SMALL`] gives the bits of the `FULL`-wide one: values and
    /// every derivative row, for each degree it serves, inside, at knots and outside.
    #[test]
    fn small_scratch_is_full_scratch_bit_for_bit() {
        for p in 1..SMALL {
            let mut knots = vec![0.0; p + 1];
            knots.extend([0.2, 0.45, 0.45, 0.8]);
            knots.extend(vec![1.0; p + 1]);
            let kv = KnotVector::new(p, knots).unwrap();
            for u in [-0.3, 0.0, 0.1, 0.45, 0.6, 1.0, 1.4] {
                for side in [Approach::Below, Approach::Above] {
                    let window = kv.window(kv.span(u, side).unwrap()).unwrap();
                    let (mut small, mut full) = ([0.0; FULL], [0.0; FULL]);
                    values_with::<SMALL, FULL>(window, u, &mut small);
                    values_with::<FULL, FULL>(window, u, &mut full);
                    assert_eq!(
                        small.map(f64::to_bits),
                        full.map(f64::to_bits),
                        "p {p} u {u}"
                    );
                    let mut ds = [[0.0; FULL]; MAX_ORDER];
                    let mut df = [[0.0; FULL]; MAX_ORDER];
                    derivs_with::<SMALL, FULL>(window, u, &mut small, &mut ds).unwrap();
                    derivs_with::<FULL, FULL>(window, u, &mut full, &mut df).unwrap();
                    assert_eq!(
                        small.map(f64::to_bits),
                        full.map(f64::to_bits),
                        "p {p} u {u}"
                    );
                    for (a, b) in ds.iter().zip(&df) {
                        assert_eq!(a.map(f64::to_bits), b.map(f64::to_bits), "p {p} u {u}");
                    }
                }
            }
        }
    }

    #[test]
    fn values_match_exact_including_ends() {
        let knots = [0.0, 0.0, 0.0, 0.0, 0.3, 0.3, 0.7, 1.0, 1.0, 1.0, 1.0];
        let kv = KnotVector::new(3, knots.to_vec()).unwrap();
        for u in [0.0, 0.1, 0.3, 0.5, 0.7, 0.9, 1.0, -0.25, 1.5] {
            for side in [Approach::Above, Approach::Below] {
                let b = kv.basis(u, side).unwrap();
                let want = exact(3, &knots, u, side);
                for (got, want) in b.values().iter().zip(&want) {
                    assert!(
                        (got - want).abs() <= 1e-15 * want.abs().max(1.0),
                        "{u} {side}"
                    );
                }
                let d = kv.basis_derivs::<1>(u, side).unwrap();
                assert_eq!(d.row(0).unwrap(), b.values());
            }
        }
        assert_eq!(
            kv.basis(0.0, Approach::Above).unwrap().values(),
            [1.0, 0.0, 0.0, 0.0]
        );
        assert_eq!(
            kv.basis(1.0, Approach::Below).unwrap().values(),
            [0.0, 0.0, 0.0, 1.0]
        );
    }

    #[test]
    fn derivatives_of_a_cubic_bezier() {
        // N = Bernstein; N'(0) = [-3, 3, 0, 0], N''(0) = [6, -12, 6, 0], N''' = [-6, 18, -18, 6].
        let kv = KnotVector::new(3, vec![0.0, 0.0, 0.0, 0.0, 1.0, 1.0, 1.0, 1.0]).unwrap();
        let d = kv.basis_derivs::<5>(0.0, Approach::Above).unwrap();
        assert_eq!(d.row(1).unwrap(), [-3.0, 3.0, 0.0, 0.0]);
        assert_eq!(d.row(2).unwrap(), [6.0, -12.0, 6.0, 0.0]);
        assert_eq!(d.row(3).unwrap(), [-6.0, 18.0, -18.0, 6.0]);
        assert_eq!(d.row(4).unwrap(), [0.0; 4]);
        assert!(d.row(5).is_none());
    }

    #[test]
    fn basis_jet_is_basis_derivs_one_row_longer() {
        let kv = KnotVector::new(
            3,
            vec![0.0, 0.0, 0.0, 0.0, 0.3, 0.3, 0.7, 1.0, 1.0, 1.0, 1.0],
        )
        .unwrap();
        for u in [0.0, 0.1, 0.3, 0.55, 0.7, 1.0] {
            for side in [Approach::Below, Approach::Above] {
                let d = kv.basis_derivs::<5>(u, side).unwrap();
                let j = kv.basis_jet::<4>(u, side).unwrap();
                assert_eq!((j.span(), j.first()), (d.span(), d.first()));
                for k in 0..=5 {
                    assert_eq!(j.row(k), d.row(k), "u {u}, row {k}");
                }
                let j0 = kv.basis_jet::<0>(u, side).unwrap();
                assert_eq!(j0.row(0), Some(kv.basis(u, side).unwrap().values()));
                assert!(j0.row(1).is_none());
            }
        }
        assert!(matches!(
            kv.basis_jet::<{ MAX_ORDER + 1 }>(0.5, Approach::Above),
            Err(NurbsError::DerivativeOrder { .. })
        ));
    }

    #[test]
    fn tiny_spans_evaluate_and_zero_differences_fail() {
        // A span 1e-30 long: the pin fails below 1e-24.
        let kv = KnotVector::new(1, vec![0.0, 0.0, 1e-30, 2e-30, 2e-30]).unwrap();
        let b = kv.basis(0.5e-30, Approach::Above).unwrap();
        assert_eq!(b.values(), [0.5, 0.5]);
        // Far outside the domain the differences round away.
        let kv = KnotVector::new(1, vec![0.0, 0.0, 1e-300, 1.0, 1.0]).unwrap();
        assert!(matches!(
            kv.basis(-1e300, Approach::Above),
            Ok(_) | Err(NurbsError::Degenerate { .. })
        ));
        assert!(kv.basis_derivs::<40>(0.5, Approach::Above).is_err());
    }
}
