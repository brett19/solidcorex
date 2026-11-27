// Written for solidcorex. Apache-2.0.
//! Evaluation: the point `S(u, v)`, the partial derivatives up to a fixed total order, and a
//! batch of points.
//!
//! The shapes follow `NurbsCurve<D>`'s `point` and `derivs::<K>`: a parameter and an
//! [`Approach`] per direction for the piece at a knot. The jet is
//! [`scx_num::jet::SurfaceJet`], the triangle `i + j <= N` of partials shared with the
//! analytic surfaces, so `N` is the highest total order (the curve's `K` counts rows,
//! `0..K`). Outside the domain the end pieces continue.
//!
//! Every value is the tensor product of the homogeneous net with the two bases: per local row
//! `i`, `t = Σ_j N_j(v) H[i][j]`, then `S += N_i(u) t`. A rational surface then divides by the
//! weight function, and its derivatives come from the homogeneous ones by the two-variable
//! quotient rule (The NURBS Book A4.4), not from rational basis derivatives per control point
//! as the pin computes them: the sums are taken once over the net, the quotient once per
//! partial.

use scx_num::jet::SurfaceJet;
use scx_nurbs::{Approach, Basis, BasisJet};

use crate::{Net, NurbsSurface, NurbsSurfaceError, ParamAxis, Result};

impl<const D: usize> NurbsSurface<D> {
    /// The point `S(u, v)` at `uv = [u, v]`, on the pieces `side` picks at a knot of each
    /// direction (`[u side, v side]`).
    ///
    /// Errors when a parameter is not finite or its basis cannot be evaluated
    /// ([`NurbsSurfaceError::Basis`]), and when the point is not finite
    /// ([`NurbsSurfaceError::Degenerate`]: a value overflows).
    pub fn point(&self, uv: [f64; 2], side: [Approach; 2]) -> Result<[f64; D]> {
        let [u, v] = uv;
        let bu = self.basis_at(ParamAxis::U, u, side[0])?;
        let bv = self.basis_at(ParamAxis::V, v, side[1])?;
        self.point_with(uv, &bu, &bv)
    }

    /// `S(u, v)` and its partials `S^(i,j)` for every `i + j <= N`, on the pieces `side`
    /// picks.
    ///
    /// `jet::<0>` is [`NurbsSurface::point`] bit for bit, and so is the point of every jet;
    /// `jet::<2>` holds what the fundamental forms need.
    ///
    /// Errors as [`NurbsSurface::point`], and with [`NurbsSurfaceError::Basis`] over
    /// `NurbsError::DerivativeOrder` when `N` is above [`scx_nurbs::MAX_ORDER`].
    pub fn jet<const N: usize>(
        &self,
        uv: [f64; 2],
        side: [Approach; 2],
    ) -> Result<SurfaceJet<[f64; D], N>> {
        let [u, v] = uv;
        let nu = self.basis_jet_at::<N>(ParamAxis::U, u, side[0])?;
        let nv = self.basis_jet_at::<N>(ParamAxis::V, v, side[1])?;
        let jet = match self.weighted_points().zip(self.weights()) {
            None => tensor_jet(self.points(), &nu, &nv),
            Some((wp, w)) => quotient(&tensor_jet(wp, &nu, &nv), &tensor_jet(w, &nu, &nv)),
        };
        if jet.iter().all(|(_, p)| p.iter().all(|x| x.is_finite())) {
            Ok(jet)
        } else {
            Err(NurbsSurfaceError::Degenerate { uv })
        }
    }

    /// [`NurbsSurface::point`] at each of `uv`, in order, on the same pieces: the same bits as
    /// one call each. A parameter repeated from the previous point (a row or column of a
    /// grid) reuses its basis.
    ///
    /// Errors with the first point's error.
    pub fn eval_many(&self, uv: &[[f64; 2]], side: [Approach; 2]) -> Result<Vec<[f64; D]>> {
        let mut out = Vec::with_capacity(uv.len());
        let mut cache: Option<([f64; 2], Basis, Basis)> = None;
        for &[u, v] in uv {
            let (bu, bv) = match cache.take() {
                Some(([cu, cv], bu, bv)) => {
                    let bu = if cu.to_bits() == u.to_bits() {
                        bu
                    } else {
                        self.basis_at(ParamAxis::U, u, side[0])?
                    };
                    let bv = if cv.to_bits() == v.to_bits() {
                        bv
                    } else {
                        self.basis_at(ParamAxis::V, v, side[1])?
                    };
                    (bu, bv)
                }
                None => (
                    self.basis_at(ParamAxis::U, u, side[0])?,
                    self.basis_at(ParamAxis::V, v, side[1])?,
                ),
            };
            out.push(self.point_with([u, v], &bu, &bv)?);
            cache = Some(([u, v], bu, bv));
        }
        Ok(out)
    }

    /// The basis of `axis` at `t`.
    fn basis_at(&self, axis: ParamAxis, t: f64, side: Approach) -> Result<Basis> {
        self.knot_vector(axis)
            .basis(t, side)
            .map_err(|cause| NurbsSurfaceError::Basis { axis, cause })
    }

    /// The basis of `axis` at `t` with its derivatives to order `N`.
    fn basis_jet_at<const N: usize>(
        &self,
        axis: ParamAxis,
        t: f64,
        side: Approach,
    ) -> Result<BasisJet<N>> {
        self.knot_vector(axis)
            .basis_jet::<N>(t, side)
            .map_err(|cause| NurbsSurfaceError::Basis { axis, cause })
    }

    /// The point from the two bases at `uv`.
    fn point_with(&self, uv: [f64; 2], bu: &Basis, bv: &Basis) -> Result<[f64; D]> {
        let at = (bu.first(), bv.first());
        let (nu, nv) = (bu.values(), bv.values());
        let p = match self.weighted_points().zip(self.weights()) {
            None => tensor_point(self.points(), at, nu, nv),
            Some((wp, w)) => {
                let w = tensor_point(w, at, nu, nv);
                tensor_point(wp, at, nu, nv).map(|x| x / w)
            }
        };
        if p.iter().all(|x| x.is_finite()) {
            Ok(p)
        } else {
            Err(NurbsSurfaceError::Degenerate { uv })
        }
    }
}

/// A value the tensor product sums: a point or a weight.
trait Lin: Copy {
    const ZERO: Self;
    /// `self + a x`, coordinate by coordinate.
    #[must_use]
    fn axpy(self, a: f64, x: Self) -> Self;
    /// `self / w`, coordinate by coordinate.
    #[must_use]
    fn div(self, w: f64) -> Self;
}

impl Lin for f64 {
    const ZERO: Self = 0.0;
    fn axpy(self, a: f64, x: Self) -> Self {
        self + a * x
    }
    fn div(self, w: f64) -> Self {
        self / w
    }
}

impl<const D: usize> Lin for [f64; D] {
    const ZERO: Self = [0.0; D];
    fn axpy(mut self, a: f64, x: Self) -> Self {
        for (s, x) in self.iter_mut().zip(x) {
            *s += a * x;
        }
        self
    }
    fn div(self, w: f64) -> Self {
        self.map(|x| x / w)
    }
}

/// Row `k` of a basis jet, `k <= N` (empty past it, which no caller asks for).
fn row<const N: usize>(b: &BasisJet<N>, k: usize) -> &[f64] {
    b.row(k).unwrap_or_default()
}

/// The local rows of `net`: `nu.len()` rows from `first.0`, each cut to `nv.len()` columns
/// from `first.1` (the span invariants make every one present).
fn local<P>(
    net: &Net<P>,
    (fu, fv): (usize, usize),
    nu: usize,
    nv: usize,
) -> impl Iterator<Item = &[P]> {
    net.rows().skip(fu).take(nu).map(move |row| {
        row.get(fv..)
            .unwrap_or_default()
            .get(..nv)
            .unwrap_or_default()
    })
}

/// `Σ_i nu[i] Σ_j nv[j] net[first.0 + i][first.1 + j]`, rows inner first.
fn tensor_point<P: Lin>(net: &Net<P>, first: (usize, usize), nu: &[f64], nv: &[f64]) -> P {
    let mut s = P::ZERO;
    for (&a, row) in nu.iter().zip(local(net, first, nu.len(), nv.len())) {
        let mut t = P::ZERO;
        for (&b, &q) in nv.iter().zip(row) {
            t = t.axpy(b, q);
        }
        s = s.axpy(a, t);
    }
    s
}

/// `A^(k,l) = Σ_i nu[k][i] Σ_j nv[l][j] net[..]` for `k + l <= N`, in [`tensor_point`]'s
/// order, so `A^(0,0)` is its value bit for bit.
fn tensor_jet<P: Lin, const N: usize>(
    net: &Net<P>,
    nu: &BasisJet<N>,
    nv: &BasisJet<N>,
) -> SurfaceJet<P, N> {
    let first = (nu.first(), nv.first());
    let (cu, cv) = (row(nu, 0).len(), row(nv, 0).len());
    let mut a = SurfaceJet::from_fn(|_, _| P::ZERO);
    for (i, h) in local(net, first, cu, cv).enumerate() {
        // t[l] = Σ_j nv[l][j] H[i][j], for l = 0..=N: t[0] and the rest.
        let sum = |l: usize| {
            let mut t = P::ZERO;
            for (&b, &q) in row(nv, l).iter().zip(h) {
                t = t.axpy(b, q);
            }
            t
        };
        let t0 = sum(0);
        let rest: [P; N] = core::array::from_fn(|l| sum(l + 1));
        let t = |l: usize| l.checked_sub(1).map_or(Some(t0), |l| rest.get(l).copied());
        for k in 0..=N {
            let b = row(nu, k).get(i).copied().unwrap_or(0.0);
            for l in 0..=N - k {
                if let (Some(e), Some(tl)) = (a.get_mut(k, l), t(l)) {
                    *e = e.axpy(b, tl);
                }
            }
        }
    }
    a
}

/// The partials of `A / w` from those of the homogeneous points `A` and the weight function
/// `w`, for `k + l <= N` (A4.4):
///
/// `S^(k,l) = (A^(k,l) - Σ_{(i,j) ≠ (0,0)} C(k,i) C(l,j) w^(i,j) S^(k-i,l-j)) / w^(0,0)`,
/// summed in A4.4's order.
fn quotient<const D: usize, const N: usize>(
    a: &SurfaceJet<[f64; D], N>,
    w: &SurfaceJet<f64, N>,
) -> SurfaceJet<[f64; D], N> {
    let w00 = w.point();
    let wt = |i: usize, j: usize| w.get(i, j).unwrap_or(0.0);
    let mut s = SurfaceJet::from_fn(|_, _| [0.0; D]);
    for k in 0..=N {
        for l in 0..=N - k {
            let st = |i: usize, j: usize| s.get(i, j).unwrap_or([0.0; D]);
            let mut v = a.get(k, l).unwrap_or([0.0; D]);
            for j in 1..=l {
                v = v.axpy(-(binomial(l, j) * wt(0, j)), st(k, l - j));
            }
            for i in 1..=k {
                let cki = binomial(k, i);
                v = v.axpy(-(cki * wt(i, 0)), st(k - i, l));
                let mut v2 = [0.0; D];
                for j in 1..=l {
                    v2 = v2.axpy(binomial(l, j) * wt(i, j), st(k - i, l - j));
                }
                v = v.axpy(-cki, v2);
            }
            if let Some(e) = s.get_mut(k, l) {
                *e = v.div(w00);
            }
        }
    }
    s
}

/// The binomial `C(n, k)`, exact: every partial product is an integer below `2^53` for
/// `n <= MAX_ORDER`.
fn binomial(n: usize, k: usize) -> f64 {
    let mut c = 1.0;
    for i in 0..k.min(n.saturating_sub(k)) {
        c = c * (n - i) as f64 / (i + 1) as f64;
    }
    c
}

#[cfg(test)]
mod tests {
    use super::*;
    use scx_nurbs::{KnotVector, NurbsError};
    use scx_testkit::data::SurfaceData;
    use scx_testkit::nurbs as tk;

    fn tk_side(a: Approach) -> tk::Side {
        match a {
            Approach::Below => tk::Side::Below,
            Approach::Above => tk::Side::Above,
        }
    }

    fn build(d: &SurfaceData<3>) -> NurbsSurface<3> {
        let (p, q) = d.degrees;
        let u = KnotVector::new(p, d.knots_u.clone()).unwrap();
        let v = KnotVector::new(q, d.knots_v.clone()).unwrap();
        let cols = d.knots_v.len() - q - 1;
        let rows = d.points.len() / cols;
        let net = Net::new(rows, cols, d.points.clone()).unwrap();
        let w = d
            .weights
            .as_ref()
            .map(|w| Net::new(rows, cols, w.clone()).unwrap());
        NurbsSurface::new(u, v, net, w).unwrap()
    }

    /// A biquadratic-by-cubic patch with an interior knot each way.
    fn data(rational: bool) -> SurfaceData<3> {
        let knots_u = vec![0.0, 0.0, 0.0, 0.5, 1.0, 1.0, 1.0];
        let knots_v = vec![0.0, 0.0, 0.0, 0.0, 0.25, 1.0, 1.0, 1.0, 1.0];
        let (rows, cols) = (4, 5);
        let mut points = Vec::new();
        let mut weights = Vec::new();
        for i in 0..rows {
            for j in 0..cols {
                let (x, y) = (f64::from(i), f64::from(j));
                points.push([x, y, 0.25 * x * y - 0.5 * y + f64::from((i + j) % 3)]);
                weights.push(1.0 + 0.5 * f64::from((i * 3 + j) % 4));
            }
        }
        SurfaceData {
            degrees: (2, 3),
            knots_u,
            knots_v,
            points,
            weights: rational.then_some(weights),
        }
    }

    fn params() -> Vec<[f64; 2]> {
        vec![
            [0.0, 0.0],
            [1.0, 1.0],
            [0.5, 0.25],
            [0.3, 0.7],
            [0.5_f64.next_down(), 0.25_f64.next_up()],
            [0.125, 0.0625],
            [-0.25, 1.5],
        ]
    }

    const SIDES: [[Approach; 2]; 2] = [
        [Approach::Above, Approach::Below],
        [Approach::Below, Approach::Above],
    ];

    fn close(got: &[f64; 3], want: &[f64; 3], tol: f64) -> bool {
        got.iter().zip(want).all(|(g, w)| (g - w).abs() <= tol)
    }

    #[test]
    fn point_is_the_exact_point() {
        for rational in [false, true] {
            let d = data(rational);
            let (s, t) = (build(&d), d.exact().unwrap());
            for uv in params() {
                for side in SIDES {
                    let got = s.point(uv, side).unwrap();
                    let want = t
                        .point((uv[0], uv[1]), (tk_side(side[0]), tk_side(side[1])))
                        .unwrap()
                        .rounded;
                    assert!(close(&got, &want, 1e-14 * 8.0), "{uv:?} {got:?} {want:?}");
                }
            }
        }
    }

    /// Every partial of total order up to 4 against the exact partials, at a tolerance
    /// growing tenfold per order with the largest exact coordinate of that order.
    #[test]
    fn jet_is_the_exact_partials_to_order_four() {
        for rational in [false, true] {
            let d = data(rational);
            let (s, t) = (build(&d), d.exact().unwrap());
            for uv in params() {
                for side in SIDES {
                    let jet = s.jet::<4>(uv, side).unwrap();
                    let want = t
                        .derivs((uv[0], uv[1]), (tk_side(side[0]), tk_side(side[1])), 4)
                        .unwrap();
                    for k in 0..5 {
                        for l in 0..5 - k {
                            let w = want.get(k, l).unwrap().rounded;
                            let scale = w.iter().fold(8.0_f64, |m, x| m.max(x.abs()));
                            let tol = 1e-14 * [1.0, 1e1, 1e2, 1e3, 1e4][k + l];
                            let g = jet.get(k, l).unwrap();
                            assert!(
                                close(&g, &w, tol * scale),
                                "{rational} {uv:?} ({k},{l}) {g:?} {w:?}"
                            );
                        }
                    }
                    assert_eq!(jet.get(0, 5), None);
                    assert_eq!(jet.get(3, 2), None);
                    assert_eq!(jet.iter().count(), 15);
                }
            }
        }
    }

    #[test]
    fn every_jet_point_is_the_point_bit_for_bit() {
        for rational in [false, true] {
            let s = build(&data(rational));
            for uv in params() {
                for side in SIDES {
                    let p = s.point(uv, side).unwrap();
                    assert_eq!(s.jet::<0>(uv, side).unwrap().point(), p);
                    assert_eq!(s.jet::<1>(uv, side).unwrap().point(), p);
                    assert_eq!(s.jet::<3>(uv, side).unwrap().point(), p);
                }
            }
        }
    }

    #[test]
    fn eval_many_is_point_each() {
        for rational in [false, true] {
            let s = build(&data(rational));
            let mut uv = params();
            // a grid row: repeated u
            uv.extend([[0.3, 0.1], [0.3, 0.2], [0.3, 0.2], [0.6, 0.2]]);
            for side in SIDES {
                let many = s.eval_many(&uv, side).unwrap();
                let each: Vec<_> = uv.iter().map(|&p| s.point(p, side).unwrap()).collect();
                assert_eq!(many, each);
            }
            assert_eq!(s.eval_many(&[], SIDES[0]).unwrap(), Vec::<[f64; 3]>::new());
        }
    }

    #[test]
    fn refusals() {
        let s = build(&data(true));
        let side = SIDES[0];
        for (uv, axis) in [
            ([f64::NAN, 0.5], ParamAxis::U),
            ([0.5, f64::INFINITY], ParamAxis::V),
        ] {
            let want = NurbsSurfaceError::Basis {
                axis,
                cause: NurbsError::ParameterNotFinite {
                    parameter: if axis == ParamAxis::U { uv[0] } else { uv[1] },
                },
            };
            let e = s.point(uv, side).unwrap_err();
            assert_eq!(e.name(), want.name());
            assert!(matches!(e, NurbsSurfaceError::Basis { axis: a, .. } if a == axis));
            assert!(matches!(
                s.jet::<2>(uv, side).unwrap_err(),
                NurbsSurfaceError::Basis { axis: a, .. } if a == axis
            ));
            assert!(s.eval_many(&[[0.5, 0.5], uv], side).is_err());
        }
        assert!(matches!(
            s.jet::<{ scx_nurbs::MAX_ORDER + 1 }>([0.5, 0.5], side)
                .unwrap_err(),
            NurbsSurfaceError::Basis {
                cause: NurbsError::DerivativeOrder { .. },
                ..
            }
        ));
        // far outside the domain, the end pieces overflow
        assert!(matches!(
            s.point([1e300, 0.5], side).unwrap_err(),
            NurbsSurfaceError::Degenerate { .. } | NurbsSurfaceError::Basis { .. }
        ));
    }

    #[test]
    fn binomials() {
        let row = |n: usize| (0..=n).map(|k| binomial(n, k)).collect::<Vec<_>>();
        assert_eq!(row(5), [1.0, 5.0, 10.0, 10.0, 5.0, 1.0]);
        assert_eq!(row(0), [1.0]);
        assert_eq!(binomial(32, 16), 601_080_390.0);
    }
}
