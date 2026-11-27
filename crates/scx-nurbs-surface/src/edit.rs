// Written for solidcorex. Apache-2.0.
//! By-value edits of a NURBS surface: knot insertion, degree elevation and splitting in
//! either direction, reversal of a direction, swapping u and v, and affine transforms.
//!
//! Every edit reads `&self` and returns a new surface; nothing edits a surface in place
//! (research 63 §2.2). Insertion, elevation and splitting edit the lines of the net along the
//! direction (the rows along v, the columns along u) as `scx-nurbs`
//! [`CompatibleCurves`]: the knot work is done once, and each line gets the curve edit's
//! arithmetic bit for bit, so the surface versions share the curves' algorithms, checks and
//! accuracy. The edited lines share one knot vector and are checked as curves, so the edited
//! surface is assembled without checking again.
//!
//! | solidcore at the pin | here |
//! |---|---|
//! | `srf_tool::p1::insert_knot(&Surface, t, mt, Direction)` | [`NurbsSurface::insert_knot`] |
//! | `srf_tool::p2::elevate_degree(&Surface, t, Direction)` | [`NurbsSurface::elevate`] |
//! | `srf_tool::p1::split(&Surface, t, Direction)` | [`NurbsSurface::split`] |
//! | `srf_nurb::Surface::reversed(&self, Direction)`, `reverse(&mut self, Direction)` | [`NurbsSurface::reversed`] |
//! | `srf_basic::Surface::swap_uv(&mut self)` | [`NurbsSurface::swapped`] |
//! | `srf_nurb::Surface::transform(&mut self, &RealMatrix)` | [`NurbsSurface::transformed`], affine only |
//! | `Direction::UV` (both directions at once) | none: one call per direction |

use scx_num::math::Affine;
use scx_num::vector3d::Point3;
use scx_nurbs::{CompatibleCurves, KnotVector, NurbsError};

use crate::{Net, NetCause, NurbsSurface, NurbsSurfaceError, ParamAxis, Result};

/// A row of a valid surface was refused as a curve.
const LINE: NurbsSurfaceError = NurbsSurfaceError::Defect {
    why: "a line of a valid surface is not a valid curve",
};

/// The edited lines do not share a knot vector or a shape.
const PIECES: NurbsSurfaceError = NurbsSurfaceError::Defect {
    why: "the edited lines do not form one net",
};

impl<const D: usize> NurbsSurface<D> {
    /// The same surface with the knot `t` of `axis` inserted `times` times (the pin's
    /// `srf_tool::insert_knot`; A5.3, Boehm's insertion on each line of the net along
    /// `axis`).
    ///
    /// `t` must lie strictly inside the domain of `axis`, and its multiplicity after the
    /// insertion must not exceed the degree; otherwise this errs with
    /// [`NurbsSurfaceError::Edit`] over the curve's cause
    /// ([`NurbsCurve::insert_knot`]). `times == 0` checks `t` and returns a copy.
    pub fn insert_knot(&self, axis: ParamAxis, t: f64, times: usize) -> Result<Self> {
        let [s] = self.edit_lines(axis, |c| Ok([c.insert_knot(t, times)?]))?;
        Ok(if times == 0 { self.clone() } else { s })
    }

    /// The same surface with the degree of `axis` raised by `times` (the pin's
    /// `srf_tool::elevate_degree`; A5.10, A5.9 on each line along `axis`): every distinct
    /// knot of `axis` gains `times` in multiplicity, so the continuity at each is kept.
    ///
    /// Errs with [`NurbsSurfaceError::Edit`] over the curve's cause
    /// ([`NurbsCurve::elevate`]): `Exhausted` above [`scx_nurbs::MAX_DEGREE`], `Refused`
    /// for unclamped knots, `Unsupported` where a line's rounding bound would pass
    /// [`scx_num::consts::ELEVATION_ERROR`]. The accuracy is the curve's. `times == 0` checks
    /// and returns a copy.
    pub fn elevate(&self, axis: ParamAxis, times: usize) -> Result<Self> {
        let [s] = self.edit_lines(axis, |c| Ok([c.elevate(times)?]))?;
        Ok(if times == 0 { self.clone() } else { s })
    }

    /// The two pieces of the surface either side of `t` along `axis`, below and above (the
    /// pin's `srf_tool::split`): `t` is inserted until its multiplicity is the degree, and
    /// each piece gets end knots of multiplicity `degree + 1` at `t`
    /// ([`NurbsCurve::split`] on each line). The other direction is unchanged.
    ///
    /// `t` must lie strictly inside the domain of `axis` ([`NurbsSurfaceError::Edit`]). The
    /// pin first moves a `t` within `1e-6` of the domain length onto a knot; this splits at
    /// `t` as given.
    pub fn split(&self, axis: ParamAxis, t: f64) -> Result<(Self, Self)> {
        let [below, above] = self.edit_lines(axis, |c| {
            let (l, r) = c.split(t)?;
            Ok([l, r])
        })?;
        Ok((below, above))
    }

    /// The same surface traversed the other way along `axis`, on the same domain: with
    /// `[a, b]` the domain of `axis`, `S'(a + b - t, ·) = S(t, ·)` (the pin's
    /// `Surface::reversed`). The knots are reflected (`t` to `a + b - t`, the domain ends to
    /// each other exactly) and the net's lines along `axis` reversed. The normal
    /// `∂S/∂u × ∂S/∂v` flips.
    ///
    /// The points and weights are kept exactly; the reflected knots are rounded once each.
    /// Rounding can merge a knot within an ulp or so of a neighbour past the allowed
    /// multiplicity, or `a + b` can overflow: then the reversal has no double form and this
    /// errs with [`NurbsSurfaceError::Reversal`] (`Unsupported`). An interior knot is kept
    /// inside the domain. The pin reflects about its first and last knots, `U[0] + U[m]`,
    /// which moves the domain of unclamped knots; this reflects about the domain.
    pub fn reversed(&self, axis: ParamAxis) -> Result<Self> {
        let knots = reflect(self.knot_vector(axis))
            .map_err(|cause| NurbsSurfaceError::Reversal { axis, cause })?;
        let (u, v) = match axis {
            ParamAxis::U => (knots, self.knot_vector(ParamAxis::V).clone()),
            ParamAxis::V => (self.knot_vector(ParamAxis::U).clone(), knots),
        };
        let rational = self
            .weights()
            .zip(self.weighted_points())
            .map(|(w, wp)| (flip(w, axis), flip(wp, axis)));
        Ok(Self::from_valid_parts(
            u,
            v,
            flip(self.points(), axis),
            rational,
        ))
    }

    /// The same surface with u and v exchanged, `S'(v, u) = S(u, v)` exactly (the pin's
    /// `swap_uv`): the knot vectors swap and the net is transposed. The normal flips.
    #[must_use]
    pub fn swapped(&self) -> Self {
        let rational = self
            .weights()
            .zip(self.weighted_points())
            .map(|(w, wp)| (w.transposed(), wp.transposed()));
        Self::from_valid_parts(
            self.knot_vector(ParamAxis::V).clone(),
            self.knot_vector(ParamAxis::U).clone(),
            self.points().transposed(),
            rational,
        )
    }

    /// The surfaces `N` edits of the lines along `axis` give: the lines as
    /// [`CompatibleCurves`], edited together.
    fn edit_lines<const N: usize>(
        &self,
        axis: ParamAxis,
        edit: impl Fn(
            &CompatibleCurves<D>,
        ) -> core::result::Result<[CompatibleCurves<D>; N], NurbsError>,
    ) -> Result<[Self; N]> {
        let edited =
            edit(&self.lines(axis)?).map_err(|cause| NurbsSurfaceError::Edit { axis, cause })?;
        let mut out = Vec::with_capacity(N);
        for lines in edited {
            out.push(self.with_lines(axis, lines)?);
        }
        out.try_into().map_err(|_| PIECES)
    }

    /// The lines of the net along `axis` (the rows along v, the columns along u) as curves.
    fn lines(&self, axis: ParamAxis) -> Result<CompatibleCurves<D>> {
        let knots = self.knot_vector(axis).clone();
        let count = match axis {
            ParamAxis::U => self.points().ncols(),
            ParamAxis::V => self.points().nrows(),
        };
        match (self.weighted_points(), self.weights()) {
            (Some(wp), Some(w)) => CompatibleCurves::from_homogeneous(
                knots,
                count,
                line_values(wp, axis),
                line_values(w, axis),
            ),
            _ => CompatibleCurves::new(knots, count, line_values(self.points(), axis), None),
        }
        .map_err(|_| LINE)
    }

    /// This surface with its lines along `axis` replaced by `lines` (the knots of the other
    /// direction kept).
    fn with_lines(&self, axis: ParamAxis, lines: CompatibleCurves<D>) -> Result<Self> {
        let count = lines.count();
        let (knots, points, rational) = lines.into_parts();
        let n = knots.point_count();
        let points = line_net(count, n, points, axis)?;
        let rational = match rational {
            None => None,
            Some(h) => Some((
                line_net(count, n, h.weights, axis)?,
                line_net(count, n, h.weighted, axis)?,
            )),
        };
        let (u, v) = match axis {
            ParamAxis::U => (knots, self.knot_vector(ParamAxis::V).clone()),
            ParamAxis::V => (self.knot_vector(ParamAxis::U).clone(), knots),
        };
        Ok(Self::from_valid_parts(u, v, points, rational))
    }
}

impl NurbsSurface<3> {
    /// The surface mapped by the affine map `a`, `S'(u, v) = a(S(u, v))` (the pin's
    /// `Surface::transform`): each control point mapped, the weights and knots kept. A
    /// NURBS surface is an affine combination of its control points, so this is the mapped
    /// surface exactly, up to one rounding of each mapped point; the normal flips when `a`
    /// reverses orientation.
    ///
    /// Affine only: under a projective map the control points do not map the surface (the
    /// homogeneous points must be mapped, and the weights change). The pin takes a 4 × 4
    /// matrix and divides each mapped point by its own projective row, which is wrong for a
    /// projective matrix (21 §N).
    ///
    /// Errs with [`NurbsSurfaceError::AffineNotFinite`] (`Refused`) for a matrix with an
    /// infinite or NaN coefficient, and [`NurbsSurfaceError::Overflow`] (`Unsupported`)
    /// where a mapped point or its homogeneous point overflows.
    pub fn transformed(&self, a: &Affine) -> Result<Self> {
        if !a.is_finite() {
            return Err(NurbsSurfaceError::AffineNotFinite);
        }
        let points = self.points().map(|&[x, y, z]| {
            let q = a.apply(Point3::new(x, y, z));
            [q.x, q.y, q.z]
        });
        let not_finite = |p: &[f64; 3]| p.iter().any(|x| !x.is_finite());
        if let Some((row, col, _)) = points.find(not_finite) {
            return Err(NurbsSurfaceError::Overflow(NetCause::NotFinite {
                row,
                col,
            }));
        }
        let rational = match self.weights() {
            None => None,
            Some(w) => {
                let weighted = points.zip_map(w, |p, &w| p.map(|x| x * w));
                if let Some((row, col, _)) = weighted.find(not_finite) {
                    return Err(NurbsSurfaceError::Overflow(NetCause::WeightedNotFinite {
                        row,
                        col,
                    }));
                }
                Some((w.clone(), weighted))
            }
        };
        Ok(Self::from_valid_parts(
            self.knot_vector(ParamAxis::U).clone(),
            self.knot_vector(ParamAxis::V).clone(),
            points,
            rational,
        ))
    }
}

/// The values of `net` line after line along `axis`: its rows along v, its columns along u.
fn line_values<P: Clone>(net: &Net<P>, axis: ParamAxis) -> Vec<P> {
    match axis {
        ParamAxis::U => net.transposed().into_points(),
        ParamAxis::V => net.points().to_vec(),
    }
}

/// The net of `count` lines of `n` values each along `axis`, given line after line.
fn line_net<P: Clone>(count: usize, n: usize, values: Vec<P>, axis: ParamAxis) -> Result<Net<P>> {
    let lines = Net::new(count, n, values).map_err(|_| PIECES)?;
    Ok(match axis {
        ParamAxis::U => lines.transposed(),
        ParamAxis::V => lines,
    })
}

/// The net with its lines along `axis` reversed.
fn flip<P: Clone>(net: &Net<P>, axis: ParamAxis) -> Net<P> {
    match axis {
        ParamAxis::U => net.rows_reversed(),
        ParamAxis::V => net.columns_reversed(),
    }
}

/// The knots reflected about the domain `[a, b]`: in reverse order, `t` to `a + b - t`,
/// with the domain ends mapped to each other exactly and interior knots kept inside it.
fn reflect(knots: &KnotVector) -> core::result::Result<KnotVector, NurbsError> {
    let (a, b) = knots.ends();
    let c = a + b;
    if !c.is_finite() {
        return Err(NurbsError::KnotNotFinite { index: 0 });
    }
    let reflected = knots
        .knots()
        .iter()
        .rev()
        .map(|&t| {
            if t == a {
                b
            } else if t == b {
                a
            } else if a < t && t < b {
                (c - t).clamp(a, b)
            } else {
                c - t
            }
        })
        .collect();
    let out = KnotVector::new(knots.degree(), reflected)?;
    out.check_continuous()?;
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use scx_num::error::ErrorKind;
    use scx_num::hash::Rng;
    use scx_num::vector3d::Vec3;
    use scx_testkit::data::SurfaceData;
    use scx_testkit::nurbs::{self as tk, Side};
    use scx_testkit::sample::{self, Knots, Spec, Weights};

    const AXES: [ParamAxis; 2] = [ParamAxis::U, ParamAxis::V];
    const ABOVE: (Side, Side) = (Side::Above, Side::Above);

    /// The first interior knot repeated `p + 1` times.
    fn has_break_in(p: usize, knots: &[f64]) -> bool {
        knots
            .iter()
            .filter(|&&k| k > 0.0 && k < 1.0)
            .any(|&k| knots.iter().filter(|&&x| x == k).count() > p)
    }

    fn has_break(d: &SurfaceData<3>) -> bool {
        has_break_in(d.degrees.0, &d.knots_u) || has_break_in(d.degrees.1, &d.knots_v)
    }

    /// Random surfaces without breaks, of every knot and weight kind; degrees 1 to 3 and 9.
    fn surfaces(seed: u64) -> Vec<SurfaceData<3>> {
        let mut rng = Rng::new(seed);
        let mut out = Vec::new();
        for (pu, pv) in [(1, 2), (2, 3), (3, 1), (9, 2)] {
            for knots in [Knots::Uniform, Knots::Clustered, Knots::Multiple] {
                for weights in [Weights::Unit, Weights::Moderate, Weights::NearZero] {
                    let spec = |degree| Spec {
                        degree,
                        count: degree + 3,
                        knots,
                    };
                    out.push(sample::surface(&mut rng, (spec(pu), spec(pv)), weights));
                }
            }
        }
        out.retain(|d| !has_break(d));
        out
    }

    fn build(d: &SurfaceData<3>) -> NurbsSurface<3> {
        let u = KnotVector::new(d.degrees.0, d.knots_u.clone()).unwrap();
        let v = KnotVector::new(d.degrees.1, d.knots_v.clone()).unwrap();
        let (rows, cols) = (u.point_count(), v.point_count());
        let points = Net::new(rows, cols, d.points.clone()).unwrap();
        let weights = d.weights.clone().map(|w| Net::new(rows, cols, w).unwrap());
        NurbsSurface::new(u, v, points, weights).unwrap()
    }

    /// The exact surface our stored form denotes: the homogeneous points as stored.
    fn exact(s: &NurbsSurface<3>) -> tk::Surface<3> {
        let degrees = (s.degree(ParamAxis::U), s.degree(ParamAxis::V));
        let (ku, kv) = (s.knots(ParamAxis::U), s.knots(ParamAxis::V));
        match (s.weighted_points(), s.weights()) {
            (Some(wp), Some(w)) => {
                tk::Surface::from_homogeneous(degrees, ku, kv, wp.points(), w.points())
            }
            _ => tk::Surface::new(degrees, ku, kv, s.points().points(), None),
        }
        .unwrap()
    }

    /// `|got - want| <= tol (1 + |want|)` for every coordinate.
    fn close(got: &[f64; 3], want: &[f64; 3], tol: f64) -> bool {
        got.iter()
            .zip(want)
            .all(|(g, w)| (g - w).abs() <= tol * (1.0 + w.abs()))
    }

    /// Our edit has the exact edit's knots, and its points within `tol`: for a rational
    /// surface the homogeneous points `(w P, w)`, relative to the largest weight. (A point
    /// whose weight is many orders below its neighbours' is ill-conditioned, as its share
    /// of the surface is small: `P = w P / w` amplifies the rounding of the combination.)
    fn matches(got: &NurbsSurface<3>, want: &SurfaceData<3>, tol: f64, what: &str) {
        assert_eq!(got.knots(ParamAxis::U), want.knots_u.as_slice(), "{what}");
        assert_eq!(got.knots(ParamAxis::V), want.knots_v.as_slice(), "{what}");
        match (got.weighted_points(), got.weights(), &want.weights) {
            (Some(gwp), Some(gw), Some(w)) => {
                let scale = w.iter().fold(0.0_f64, |m, &x| m.max(x));
                let homogeneous = want.points.iter().zip(w);
                for ((gp, gw), (p, w)) in gwp.points().iter().zip(gw.points()).zip(homogeneous) {
                    let wp = p.map(|x| x * w);
                    let ok = gp
                        .iter()
                        .zip(&wp)
                        .all(|(g, x)| (g - x).abs() <= tol * scale)
                        && (gw - w).abs() <= tol * scale;
                    assert!(ok, "{what}: {gp:?} {gw} vs {wp:?} {w}");
                }
            }
            (None, None, None) => {
                for (g, w) in got.points().points().iter().zip(&want.points) {
                    assert!(close(g, w, tol), "{what}: {g:?} vs {w:?}");
                }
            }
            (_, g, w) => panic!("{what}: weights {g:?} vs {w:?}"),
        }
    }

    /// `edited` is `original` at a few parameters spread over its domain (evaluated exactly
    /// from each stored form).
    fn same_surface(edited: &NurbsSurface<3>, original: &tk::Surface<3>, tol: f64, what: &str) {
        let e = exact(edited);
        let ((u0, u1), (v0, v1)) = e.domain();
        for (fu, fv) in [(0.0, 0.0), (0.3, 1.0), (0.71, 0.45), (1.0, 0.8)] {
            let uv = (u0 + fu * (u1 - u0), v0 + fv * (v1 - v0));
            let got = e.point(uv, ABOVE).unwrap().rounded;
            let want = original.point(uv, ABOVE).unwrap().rounded;
            assert!(
                close(&got, &want, tol),
                "{what} at {uv:?}: {got:?} vs {want:?}"
            );
        }
    }

    /// The parameters `t` of `axis` to edit at: inside the domain, and a knot when there
    /// is an interior one.
    fn parameters(s: &NurbsSurface<3>, axis: ParamAxis) -> Vec<f64> {
        let mut out = vec![0.37];
        out.extend(s.knots(axis).iter().copied().find(|&k| k > 0.0 && k < 1.0));
        out
    }

    fn exact_edit<T>(
        axis: ParamAxis,
        u: impl FnOnce() -> core::result::Result<T, tk::Error>,
        v: impl FnOnce() -> core::result::Result<T, tk::Error>,
    ) -> T {
        match axis {
            ParamAxis::U => u(),
            ParamAxis::V => v(),
        }
        .unwrap()
    }

    #[test]
    fn insertion_is_the_exact_insertion() {
        let mut edits = 0;
        // Degree 9 is the exact references' slow case; elevation covers it.
        for d in surfaces(1).iter().filter(|d| d.degrees.0 < 9) {
            let (s, truth) = (build(d), d.exact().unwrap());
            for axis in AXES {
                for t in parameters(&s, axis) {
                    let p = s.degree(axis);
                    let times = p - s.knot_vector(axis).multiplicity(t).min(p);
                    if times == 0 {
                        continue;
                    }
                    let i = s.insert_knot(axis, t, times).unwrap();
                    let want = exact_edit(
                        axis,
                        || truth.insert_knot_u(t, times),
                        || truth.insert_knot_v(t, times),
                    );
                    let what = format!("{s} insert {t} ×{times} along {axis}");
                    matches(&i, &want.rounded(), 1e-13, &what);
                    same_surface(&i, &truth, 1e-12, &what);
                    edits += 1;
                }
            }
        }
        assert!(edits > 20, "{edits}");
    }

    #[test]
    fn split_pieces_are_the_exact_pieces() {
        let mut edits = 0;
        for d in surfaces(2).iter().filter(|d| d.degrees.0 < 9).step_by(2) {
            let (s, truth) = (build(d), d.exact().unwrap());
            for axis in AXES {
                for t in parameters(&s, axis) {
                    let (l, r) = s.split(axis, t).unwrap();
                    let (wl, wr) = exact_edit(axis, || truth.split_u(t), || truth.split_v(t));
                    let what = format!("{s} split at {t} along {axis}");
                    matches(&l, &wl.rounded(), 1e-13, &what);
                    matches(&r, &wr.rounded(), 1e-13, &what);
                    same_surface(&l, &truth, 1e-12, &what);
                    same_surface(&r, &truth, 1e-12, &what);
                    edits += 1;
                }
            }
        }
        assert!(edits > 20, "{edits}");
    }

    /// Near-coincident knots among them (the curve elevation's knot removal no longer
    /// loses them).
    #[test]
    fn elevation_is_the_exact_elevation() {
        let mut edits = 0;
        for d in surfaces(3).iter().step_by(4) {
            let (s, truth) = (build(d), d.exact().unwrap());
            for axis in AXES {
                let e = s.elevate(axis, 1).unwrap();
                assert_eq!(e.degree(axis), s.degree(axis) + 1);
                let want = exact_edit(axis, || truth.elevate_u(1), || truth.elevate_v(1));
                let what = format!("{s} elevated along {axis}");
                let tol = 1e-12;
                matches(&e, &want.rounded(), tol, &what);
                same_surface(&e, &truth, tol, &what);
                edits += 1;
            }
        }
        assert!(edits > 8, "{edits}");
    }

    #[test]
    fn zero_edits_are_copies() {
        for d in surfaces(4).iter().step_by(5) {
            let s = build(d);
            for axis in AXES {
                assert_eq!(s.insert_knot(axis, 0.37, 0).unwrap(), s);
                assert_eq!(s.elevate(axis, 0).unwrap(), s);
                // still checked
                assert!(s.insert_knot(axis, 1.5, 0).is_err());
            }
        }
    }

    /// Knots on a 1/64 grid reflect exactly (`1 - k/64`), so the reversed surface is the
    /// original exactly: `S'(1 - u, v) = S(u, v)` in rationals.
    #[test]
    fn reversal_is_exact_on_dyadic_knots() {
        let grid =
            |k: &[f64]| -> Vec<f64> { k.iter().map(|x| (x * 64.0).floor() / 64.0).collect() };
        let mut checked = 0;
        for mut d in surfaces(5).into_iter().step_by(2) {
            d.knots_u = grid(&d.knots_u);
            d.knots_v = grid(&d.knots_v);
            if has_break(&d) {
                continue;
            }
            let s = build(&d);
            let truth = exact(&s);
            for axis in AXES {
                let r = s.reversed(axis).unwrap();
                assert_eq!(r.domain(), s.domain());
                let back = r.reversed(axis).unwrap();
                assert_eq!(back, s, "reversed twice");
                let er = exact(&r);
                for (fu, fv) in [(0.0, 0.0), (0.25, 0.5), (0.671_875, 1.0), (1.0, 0.125)] {
                    let flipped = match axis {
                        ParamAxis::U => (1.0 - fu, fv),
                        ParamAxis::V => (fu, 1.0 - fv),
                    };
                    let got = er.point(flipped, ABOVE).unwrap();
                    let side = match axis {
                        ParamAxis::U => (Side::Below, Side::Above),
                        ParamAxis::V => (Side::Above, Side::Below),
                    };
                    let want = truth.point((fu, fv), side).unwrap();
                    assert_eq!(got.exact, want.exact, "{s} reversed along {axis}");
                }
            }
            checked += 1;
        }
        assert!(checked > 8, "{checked}");
    }

    #[test]
    fn reversal_of_unclamped_knots_keeps_the_domain() {
        // The domain is [0, 1]; the pin reflects about U[0] + U[m] = 1.5 instead.
        let u = KnotVector::new(2, vec![-1.0, -0.5, 0.0, 0.3, 1.0, 1.25, 2.5]).unwrap();
        let v = KnotVector::new(1, vec![0.0, 0.0, 1.0, 1.0]).unwrap();
        let pts = (0..8).map(|k| [f64::from(k), 0.5, -1.0]).collect();
        let s = NurbsSurface::new(u, v, Net::new(4, 2, pts).unwrap(), None).unwrap();
        let r = s.reversed(ParamAxis::U).unwrap();
        assert_eq!(
            r.knots(ParamAxis::U),
            [-1.5, -0.25, 0.0, 0.7, 1.0, 1.5, 2.0]
        );
        assert_eq!(r.domain(), s.domain());
        let (e, er) = (exact(&s), exact(&r));
        for t in [0.0, 0.25, 0.5, 1.0] {
            let want = e.point((t, 0.4), ABOVE).unwrap().rounded;
            let got = er
                .point((1.0 - t, 0.4), (Side::Below, Side::Above))
                .unwrap()
                .rounded;
            assert!(close(&got, &want, 1e-15), "{t}");
        }
    }

    /// A knot an ulp from the end reflects onto the end: the reflected knots have end
    /// multiplicity `p + 2`, which no knot vector has.
    #[test]
    fn reversal_without_a_double_form_is_unsupported() {
        let u = KnotVector::new(1, vec![0.0, 0.0, 1e-17, 1.0, 1.0]).unwrap();
        let v = KnotVector::new(1, vec![0.0, 0.0, 1.0, 1.0]).unwrap();
        let s = NurbsSurface::new(
            u,
            v.clone(),
            Net::new(3, 2, vec![[0.0; 3]; 6]).unwrap(),
            None,
        )
        .unwrap();
        let e = s.reversed(ParamAxis::U).unwrap_err();
        assert!(
            matches!(
                e,
                NurbsSurfaceError::Reversal {
                    axis: ParamAxis::U,
                    ..
                }
            ),
            "{e:?}"
        );
        assert_eq!(e.kind(), ErrorKind::Unsupported);
        let huge = KnotVector::new(1, vec![1e308, 1e308, 1.5e308, 1.5e308]).unwrap();
        let s =
            NurbsSurface::new(huge, v, Net::new(2, 2, vec![[0.0; 3]; 4]).unwrap(), None).unwrap();
        assert_eq!(
            s.reversed(ParamAxis::U).unwrap_err().kind(),
            ErrorKind::Unsupported
        );
    }

    #[test]
    fn swapping_is_exact() {
        for d in surfaces(6).iter().step_by(4) {
            let s = build(d);
            let w = s.swapped();
            assert_eq!(w.swapped(), s);
            assert_eq!(w.knots(ParamAxis::U), s.knots(ParamAxis::V));
            assert_eq!(w.knots(ParamAxis::V), s.knots(ParamAxis::U));
            let (e, ew) = (exact(&s), exact(&w));
            for uv in [(0.0, 0.0), (0.3, 0.8), (1.0, 0.45)] {
                assert_eq!(
                    ew.point((uv.1, uv.0), ABOVE).unwrap().exact,
                    e.point(uv, ABOVE).unwrap().exact,
                    "{s}"
                );
            }
        }
    }

    #[test]
    fn transform_maps_the_surface() {
        let a =
            Affine::rotation(Point3::new(0.5, -1.0, 2.0), Vec3::new(1.0, 2.0, -0.5), 0.7).unwrap();
        let shear = Affine::from_axes(
            Vec3::new(2.0, 0.0, 0.0),
            Vec3::new(0.5, 1.0, 0.0),
            Vec3::new(0.0, -0.25, 3.0),
            Point3::new(1.0, 2.0, 3.0),
        );
        for d in surfaces(7).iter().step_by(4) {
            let s = build(d);
            assert_eq!(s.transformed(&Affine::IDENTITY).unwrap(), s);
            let e = exact(&s);
            for m in [a, shear] {
                let t = s.transformed(&m).unwrap();
                assert_eq!(t.weights(), s.weights());
                let et = exact(&t);
                for uv in [(0.0, 0.0), (0.3, 0.8), (1.0, 0.45)] {
                    let [x, y, z] = e.point(uv, ABOVE).unwrap().rounded;
                    let q = m.apply(Point3::new(x, y, z));
                    let got = et.point(uv, ABOVE).unwrap().rounded;
                    assert!(close(&got, &[q.x, q.y, q.z], 1e-13), "{s} at {uv:?}");
                }
            }
        }
    }

    #[test]
    fn transform_errors() {
        let u = KnotVector::new(1, vec![0.0, 0.0, 1.0, 1.0]).unwrap();
        let net = Net::new(2, 2, vec![[1e300, 0.0, 0.0]; 4]).unwrap();
        let w = Some(Net::new(2, 2, vec![1.0, 1.0, 1.0, 4.0]).unwrap());
        let s = NurbsSurface::new(u.clone(), u, net, w).unwrap();
        let nan = Affine::translation(Vec3::new(f64::NAN, 0.0, 0.0));
        assert_eq!(s.transformed(&nan), Err(NurbsSurfaceError::AffineNotFinite));
        let big = Affine::from_axes(
            Vec3::new(1e8, 0.0, 0.0),
            Vec3::new(0.0, 1.0, 0.0),
            Vec3::new(0.0, 0.0, 1.0),
            Point3::new(0.0, 0.0, 0.0),
        );
        assert_eq!(
            s.transformed(&big),
            Err(NurbsSurfaceError::Overflow(NetCause::WeightedNotFinite {
                row: 1,
                col: 1
            }))
        );
        let shifted = Affine::translation(Vec3::new(f64::MAX, 0.0, 0.0));
        let e = s.transformed(&shifted).unwrap_err();
        assert_eq!(
            e,
            NurbsSurfaceError::Overflow(NetCause::NotFinite { row: 0, col: 0 })
        );
        assert_eq!(e.kind(), ErrorKind::Unsupported);
    }

    #[test]
    fn edit_errors_carry_the_curve_cause() {
        let d = &surfaces(8)[0];
        let s = build(d);
        let kinds = |r: Result<NurbsSurface<3>>| r.map(|_| ()).map_err(|e| e.kind());
        for axis in AXES {
            let e = s.insert_knot(axis, 1.5, 1).unwrap_err();
            assert!(
                matches!(e, NurbsSurfaceError::Edit { axis: a, cause: NurbsError::ParameterOutside { .. } } if a == axis),
                "{e:?}"
            );
            assert_eq!(e.kind(), ErrorKind::Refused);
            let p = s.degree(axis);
            assert_eq!(
                kinds(s.insert_knot(axis, 0.37, p + 1)),
                Err(ErrorKind::Refused)
            );
            assert_eq!(kinds(s.elevate(axis, 40)), Err(ErrorKind::Exhausted));
            assert_eq!(
                s.split(axis, 0.0).map(|_| ()).map_err(|e| e.kind()),
                Err(ErrorKind::Refused)
            );
            assert_eq!(
                s.split(axis, f64::NAN).map(|_| ()).map_err(|e| e.kind()),
                Err(ErrorKind::Refused)
            );
        }
        let unclamped = KnotVector::new(1, vec![-1.0, 0.0, 1.0, 2.0]).unwrap();
        let v = KnotVector::new(1, vec![0.0, 0.0, 1.0, 1.0]).unwrap();
        let s = NurbsSurface::new(
            unclamped,
            v,
            Net::new(2, 2, vec![[0.0; 3]; 4]).unwrap(),
            None,
        )
        .unwrap();
        assert_eq!(
            s.elevate(ParamAxis::U, 1),
            Err(NurbsSurfaceError::Edit {
                axis: ParamAxis::U,
                cause: NurbsError::Unclamped
            })
        );
        assert!(s.elevate(ParamAxis::V, 1).is_ok());
    }
}
