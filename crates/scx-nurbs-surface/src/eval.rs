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
    /// `N` is the highest total order, inclusive: `jet::<2>` holds the second partials (what
    /// the fundamental forms need), where the curve's `derivs::<K>` counts its `K` rows.
    /// `jet::<0>` is [`NurbsSurface::point`] bit for bit, and so is the point of every jet.
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

    /// [`NurbsSurface::point`] at each of `uv`, into `out`, on the same pieces: `out[k]` is
    /// `point(uv[k], side)` bit for bit. A parameter repeated from the previous point (a row
    /// or column walked in order) reuses its basis; for a whole grid,
    /// [`NurbsSurface::eval_grid`] computes each basis once.
    ///
    /// Errors with the first point's error; `out` then holds the points before it, and the
    /// rest is left as it was.
    ///
    /// # Panics
    ///
    /// If `uv` and `out` differ in length.
    pub fn eval_many(
        &self,
        uv: &[[f64; 2]],
        side: [Approach; 2],
        out: &mut [[f64; D]],
    ) -> Result<()> {
        assert_eq!(
            uv.len(),
            out.len(),
            "eval_many: uv and out differ in length"
        );
        // The bases of the previous point, updated in place (a `Basis` is 264 bytes).
        let mut prev: Option<([u64; 2], Basis, Basis)> = None;
        for (&[u, v], p) in uv.iter().zip(out) {
            let key = [u.to_bits(), v.to_bits()];
            let (bu, bv) = if let Some((at, bu, bv)) = &mut prev {
                if at[0] != key[0] {
                    *bu = self.basis_at(ParamAxis::U, u, side[0])?;
                }
                if at[1] != key[1] {
                    *bv = self.basis_at(ParamAxis::V, v, side[1])?;
                }
                *at = key;
                (&*bu, &*bv)
            } else {
                let bu = self.basis_at(ParamAxis::U, u, side[0])?;
                let bv = self.basis_at(ParamAxis::V, v, side[1])?;
                let (_, bu, bv) = prev.insert((key, bu, bv));
                (&*bu, &*bv)
            };
            *p = self.point_with([u, v], bu, bv)?;
        }
        Ok(())
    }

    /// The points of the grid `us × vs`, into `out` row by row: `out[i * vs.len() + j]` is
    /// `point([us[i], vs[j]], side)` bit for bit. Each basis is computed once, `us.len() +
    /// vs.len()` of them for `us.len() · vs.len()` points (65 §3.1).
    ///
    /// Errors with the first point's error in that order; `out` then holds the points before
    /// it, and the rest is left as it was.
    ///
    /// # Panics
    ///
    /// If `out.len()` is not `us.len() * vs.len()`.
    pub fn eval_grid(
        &self,
        us: &[f64],
        vs: &[f64],
        side: [Approach; 2],
        out: &mut [[f64; D]],
    ) -> Result<()> {
        assert_eq!(
            Some(out.len()),
            us.len().checked_mul(vs.len()),
            "eval_grid: out is not us.len() * vs.len() long"
        );
        if vs.is_empty() {
            return Ok(());
        }
        // A failed `v` basis is kept as `None` and its error rebuilt when its first point is
        // reached, so the error is the one `point` gives there.
        let bvs: Vec<Option<Basis>> = vs
            .iter()
            .map(|&v| self.basis_at(ParamAxis::V, v, side[1]).ok())
            .collect();
        for (&u, row) in us.iter().zip(out.chunks_exact_mut(vs.len())) {
            let bu = self.basis_at(ParamAxis::U, u, side[0])?;
            for ((&v, bv), p) in vs.iter().zip(&bvs).zip(row) {
                let fresh;
                let bv = if let Some(bv) = bv {
                    bv
                } else {
                    fresh = self.basis_at(ParamAxis::V, v, side[1])?;
                    &fresh
                };
                *p = self.point_with([u, v], &bu, bv)?;
            }
        }
        Ok(())
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
                let (a, w) = tensor_point_rational(wp, w, at, nu, nv);
                a.map(|x| x / w)
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

/// [`tensor_point`] of the weighted points and of the weights in one pass over the net: each
/// sum is the one `tensor_point` takes, so both are its values bit for bit.
fn tensor_point_rational<const D: usize>(
    wp: &Net<[f64; D]>,
    w: &Net<f64>,
    first: (usize, usize),
    nu: &[f64],
    nv: &[f64],
) -> ([f64; D], f64) {
    let (mut s, mut sw) = ([0.0; D], 0.0);
    let rows = local(wp, first, nu.len(), nv.len()).zip(local(w, first, nu.len(), nv.len()));
    for (&a, (row, wrow)) in nu.iter().zip(rows) {
        let (mut t, mut tw) = ([0.0; D], 0.0);
        for ((&b, &q), &wq) in nv.iter().zip(row).zip(wrow) {
            t = t.axpy(b, q);
            tw = tw.axpy(b, wq);
        }
        s = s.axpy(a, t);
        sw = sw.axpy(a, tw);
    }
    (s, sw)
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
                let mut many = vec![[0.0; 3]; uv.len()];
                s.eval_many(&uv, side, &mut many).unwrap();
                let each: Vec<_> = uv.iter().map(|&p| s.point(p, side).unwrap()).collect();
                assert_eq!(many, each);
            }
            s.eval_many(&[], SIDES[0], &mut []).unwrap();
        }
    }

    #[test]
    #[should_panic(expected = "uv and out differ in length")]
    fn eval_many_refuses_slices_of_different_lengths() {
        let _ = build(&data(false)).eval_many(&[[0.5, 0.5]], SIDES[0], &mut []);
    }

    #[test]
    fn eval_grid_is_point_each() {
        // inside, at knots, at the ends and outside the domain, repeated
        let ts = [0.0, 0.3, 0.5, 0.5, 0.75, 1.0, -0.2, 1.3];
        for rational in [false, true] {
            let s = build(&data(rational));
            for side in SIDES {
                for (nu, nv) in [(8, 8), (3, 8), (8, 1), (1, 1), (0, 4), (4, 0)] {
                    let (us, vs) = (&ts[..nu], &ts[8 - nv..]);
                    let mut grid = vec![[0.0; 3]; nu * nv];
                    s.eval_grid(us, vs, side, &mut grid).unwrap();
                    let each: Vec<_> = us
                        .iter()
                        .flat_map(|&u| vs.iter().map(move |&v| [u, v]))
                        .map(|p| s.point(p, side).unwrap())
                        .collect();
                    assert_eq!(grid, each);
                }
            }
        }
    }

    #[test]
    fn eval_grid_errors_as_the_first_failing_point() {
        let s = build(&data(true));
        let side = SIDES[0];
        let mut out = [[7.0; 3]; 6];
        // row 0 is fine; row 1 fails on u at its first point
        let e = s
            .eval_grid(&[0.5, f64::NAN], &[0.1, 0.2, 0.3], side, &mut out)
            .unwrap_err();
        // (the cause holds the NaN, so compare the debug forms)
        let want = s.point([f64::NAN, 0.1], side).unwrap_err();
        assert_eq!(format!("{e:?}"), format!("{want:?}"));
        assert_eq!(out[2], s.point([0.5, 0.3], side).unwrap());
        assert_eq!(out[3], [7.0; 3]);
        // a bad v fails at its column in row 0, after the points before it
        let mut out = [[7.0; 3]; 6];
        let e = s
            .eval_grid(&[0.5, 0.6], &[0.1, f64::INFINITY, 0.3], side, &mut out)
            .unwrap_err();
        assert_eq!(e, s.point([0.5, f64::INFINITY], side).unwrap_err());
        assert_eq!(out[0], s.point([0.5, 0.1], side).unwrap());
        assert_eq!(out[1], [7.0; 3]);
    }

    #[test]
    #[should_panic(expected = "out is not us.len() * vs.len() long")]
    fn eval_grid_refuses_a_wrong_length() {
        let _ = build(&data(false)).eval_grid(&[0.5, 0.6], &[0.5], SIDES[0], &mut [[0.0; 3]]);
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
            assert!(
                s.eval_many(&[[0.5, 0.5], uv], side, &mut [[0.0; 3]; 2])
                    .is_err()
            );
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
