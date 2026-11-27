// Written for solidcorex. Apache-2.0.
//! NURBS surfaces as plain data: validated construction and the stored form.
//!
//! The pin's `nurbs::srf_basic::Surface` checks almost nothing when built (`Surface::new`
//! refuses only empty or ragged nets and mixed rationality) and leaves the rest to
//! `is_valid`. Here the only constructors check everything once, so a surface that exists
//! is valid, and nothing later checks it again.

use core::fmt;

use scx_num::extent2d::Extent2d;
use scx_nurbs::KnotVector;

use crate::{Net, NetCause, NurbsSurfaceError, Result, WeightCause};

/// A parameter direction of a surface: `U` along the rows of its net, `V` along the
/// columns.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ParamAxis {
    /// The first parameter; a [`Net`]'s row index.
    U,
    /// The second parameter; a [`Net`]'s column index.
    V,
}

impl fmt::Display for ParamAxis {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::U => "u",
            Self::V => "v",
        })
    }
}

/// A tensor-product NURBS surface in `D` dimensions: a [`KnotVector`] per direction, of
/// degrees `p` (u) and `q` (v), a net of `(m_u - p) × (m_v - q)` control points, and for a
/// rational surface one positive weight per point.
///
/// Built only by [`NurbsSurface::new`] or [`NurbsSurface::from_homogeneous`], which check
/// that no interior knot of either direction repeats more than its degree (the surface is
/// C0, as every `scx-nurbs` curve), the net's shape against the knots, that every
/// coordinate is finite and that every weight is positive and finite. The degree cap and
/// the knots' own checks are [`KnotVector::new`]'s. Unclamped knots are allowed, as for
/// curves.
///
/// A rational surface keeps both its points `P` and its homogeneous points `w P`, as
/// `scx-nurbs` curves do.
#[derive(Debug, Clone, PartialEq)]
pub struct NurbsSurface<const D: usize> {
    u: KnotVector,
    v: KnotVector,
    /// The control points `P[i][j]`, row `i` along v.
    points: Net<[f64; D]>,
    rational: Option<Rational<D>>,
}

/// The weights of a rational surface, and its homogeneous points `w P`, each the shape of
/// the control net.
#[derive(Debug, Clone, PartialEq)]
struct Rational<const D: usize> {
    weights: Net<f64>,
    weighted: Net<[f64; D]>,
}

impl<const D: usize> NurbsSurface<D> {
    /// The surface with control points `points` on the knots `u` (rows) and `v` (columns);
    /// rational with `weights`, a net of the same shape.
    pub fn new(
        u: KnotVector,
        v: KnotVector,
        points: Net<[f64; D]>,
        weights: Option<Net<f64>>,
    ) -> Result<Self> {
        check_knots(&u, &v)?;
        check_net(&u, &v, &points, |row, col| NetCause::NotFinite { row, col })?;
        let rational = match weights {
            None => None,
            Some(weights) => {
                check_weights(&weights, &points)?;
                let weighted = points.zip_map(&weights, |p, &w| p.map(|x| x * w));
                check_finite(&weighted, |row, col| NetCause::WeightedNotFinite {
                    row,
                    col,
                })?;
                Some(Rational { weights, weighted })
            }
        };
        Ok(Self {
            u,
            v,
            points,
            rational,
        })
    }

    /// The rational surface with homogeneous control points `(weighted[i][j],
    /// weights[i][j])`, that is `P[i][j] = weighted[i][j] / weights[i][j]`.
    pub fn from_homogeneous(
        u: KnotVector,
        v: KnotVector,
        weighted: Net<[f64; D]>,
        weights: Net<f64>,
    ) -> Result<Self> {
        check_knots(&u, &v)?;
        check_net(&u, &v, &weighted, |row, col| NetCause::WeightedNotFinite {
            row,
            col,
        })?;
        check_weights(&weights, &weighted)?;
        let points = weighted.zip_map(&weights, |p, &w| p.map(|x| x / w));
        check_finite(&points, |row, col| NetCause::NotFinite { row, col })?;
        Ok(Self {
            u,
            v,
            points,
            rational: Some(Rational { weights, weighted }),
        })
    }

    /// The surface from parts an edit built from a valid surface, which satisfy every check
    /// of [`Self::new`] by construction: nothing is checked again. `rational` is the weights
    /// and the homogeneous points `w P`, each the shape of `points`.
    pub(crate) fn from_valid_parts(
        u: KnotVector,
        v: KnotVector,
        points: Net<[f64; D]>,
        rational: Option<(Net<f64>, Net<[f64; D]>)>,
    ) -> Self {
        Self {
            u,
            v,
            points,
            rational: rational.map(|(weights, weighted)| Rational { weights, weighted }),
        }
    }

    /// The knot vector of a direction.
    #[must_use]
    pub const fn knot_vector(&self, axis: ParamAxis) -> &KnotVector {
        match axis {
            ParamAxis::U => &self.u,
            ParamAxis::V => &self.v,
        }
    }

    /// The knots of a direction.
    #[must_use]
    pub fn knots(&self, axis: ParamAxis) -> &[f64] {
        self.knot_vector(axis).knots()
    }

    /// The degree of a direction (`p` for u, `q` for v).
    #[must_use]
    pub const fn degree(&self, axis: ParamAxis) -> usize {
        self.knot_vector(axis).degree()
    }

    /// The domain `[U[p], U[m_u - p]] × [V[q], V[m_v - q]]`, u along x.
    #[must_use]
    pub fn domain(&self) -> Extent2d {
        Extent2d::from_intervals(self.u.domain(), self.v.domain())
    }

    /// The control points `P[i][j]`.
    #[must_use]
    pub const fn points(&self) -> &Net<[f64; D]> {
        &self.points
    }

    /// The weights of a rational surface.
    #[must_use]
    pub fn weights(&self) -> Option<&Net<f64>> {
        self.rational.as_ref().map(|r| &r.weights)
    }

    /// The homogeneous points `w P` of a rational surface.
    #[must_use]
    pub fn weighted_points(&self) -> Option<&Net<[f64; D]>> {
        self.rational.as_ref().map(|r| &r.weighted)
    }

    /// Whether the surface has weights.
    #[must_use]
    pub const fn is_rational(&self) -> bool {
        self.rational.is_some()
    }
}

/// Each direction's knots are continuous inside the domain.
fn check_knots(u: &KnotVector, v: &KnotVector) -> Result<()> {
    for (axis, knots) in [(ParamAxis::U, u), (ParamAxis::V, v)] {
        knots
            .check_continuous()
            .map_err(|cause| NurbsSurfaceError::BadKnots { axis, cause })?;
    }
    Ok(())
}

/// The net has the shape the knots need, and every coordinate is finite (`not_finite`
/// names the cause).
fn check_net<const D: usize>(
    u: &KnotVector,
    v: &KnotVector,
    net: &Net<[f64; D]>,
    not_finite: fn(usize, usize) -> NetCause,
) -> Result<()> {
    for (axis, count, expected) in [
        (ParamAxis::U, net.nrows(), u.point_count()),
        (ParamAxis::V, net.ncols(), v.point_count()),
    ] {
        if count != expected {
            return Err(NurbsSurfaceError::BadNet(NetCause::Count {
                axis,
                count,
                expected,
            }));
        }
    }
    check_finite(net, not_finite)
}

fn check_finite<const D: usize>(
    net: &Net<[f64; D]>,
    not_finite: fn(usize, usize) -> NetCause,
) -> Result<()> {
    match net.find(|p| p.iter().any(|x| !x.is_finite())) {
        Some((row, col, _)) => Err(NurbsSurfaceError::BadNet(not_finite(row, col))),
        None => Ok(()),
    }
}

/// The weights have the net's shape and are positive and finite.
fn check_weights<P>(weights: &Net<f64>, net: &Net<P>) -> Result<()> {
    if !weights.same_shape(net) {
        return Err(NurbsSurfaceError::BadWeights(WeightCause::Shape {
            rows: weights.nrows(),
            cols: weights.ncols(),
            net_rows: net.nrows(),
            net_cols: net.ncols(),
        }));
    }
    match weights.find(|&w| !(w > 0.0 && w.is_finite())) {
        Some((row, col, &weight)) => Err(NurbsSurfaceError::BadWeights(WeightCause::NotPositive {
            row,
            col,
            weight,
        })),
        None => Ok(()),
    }
}

impl<const D: usize> fmt::Display for NurbsSurface<D> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let ((u0, u1), (v0, v1)) = (self.u.ends(), self.v.ends());
        write!(
            f,
            "{} surface of degrees {} × {} in {D} dimensions, {} × {} control points on \
             [{u0}, {u1}] × [{v0}, {v1}]",
            if self.is_rational() {
                "rational"
            } else {
                "polynomial"
            },
            self.u.degree(),
            self.v.degree(),
            self.points.nrows(),
            self.points.ncols(),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use scx_num::hash::Rng;
    use scx_nurbs::NurbsError;
    use scx_testkit::data::SurfaceData;
    use scx_testkit::nurbs::Side;
    use scx_testkit::sample::{self, Knots, Spec, Weights};

    fn kv(p: usize, k: &[f64]) -> KnotVector {
        KnotVector::new(p, k.to_vec()).unwrap()
    }

    /// The first interior knot repeated `p + 1` times.
    fn break_of(p: usize, knots: &[f64]) -> Option<f64> {
        knots
            .iter()
            .copied()
            .filter(|&k| k > 0.0 && k < 1.0)
            .find(|&k| knots.iter().filter(|&&x| x == k).count() > p)
    }

    /// Random surfaces of every knot and weight kind, degrees 1..=4 and 9 in each direction.
    fn surfaces(seed: u64) -> Vec<SurfaceData<3>> {
        let mut rng = Rng::new(seed);
        let mut out = Vec::new();
        for (pu, pv) in [(1, 1), (1, 3), (2, 2), (3, 4), (9, 2)] {
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
        out
    }

    fn build(d: &SurfaceData<3>) -> Result<NurbsSurface<3>> {
        let u = kv(d.degrees.0, &d.knots_u);
        let v = kv(d.degrees.1, &d.knots_v);
        let (rows, cols) = (u.point_count(), v.point_count());
        let points = Net::new(rows, cols, d.points.clone())?;
        let weights = d
            .weights
            .clone()
            .map(|w| Net::new(rows, cols, w))
            .transpose()?;
        NurbsSurface::new(u, v, points, weights)
    }

    fn has_break(d: &SurfaceData<3>) -> bool {
        break_of(d.degrees.0, &d.knots_u).is_some() || break_of(d.degrees.1, &d.knots_v).is_some()
    }

    #[test]
    fn construction_keeps_the_data() {
        let mut built = 0;
        for d in surfaces(1).iter().filter(|d| !has_break(d)) {
            let s = build(d).unwrap();
            assert_eq!(s.knots(ParamAxis::U), d.knots_u.as_slice());
            assert_eq!(s.knots(ParamAxis::V), d.knots_v.as_slice());
            assert_eq!((s.degree(ParamAxis::U), s.degree(ParamAxis::V)), d.degrees);
            assert_eq!(s.points().points(), d.points.as_slice());
            assert_eq!(s.weights().map(Net::points), d.weights.as_deref());
            assert_eq!(s.is_rational(), d.weights.is_some());
            let dom = s.domain();
            assert_eq!(
                (dom.x_min(), dom.x_max(), dom.y_min(), dom.y_max()),
                (0.0, 1.0, 0.0, 1.0)
            );
            if let (Some(w), Some(wp)) = (s.weights(), s.weighted_points()) {
                for ((p, &w), q) in s.points().points().iter().zip(w.points()).zip(wp.points()) {
                    assert_eq!(*q, p.map(|x| x * w));
                }
                // From the homogeneous points back: the same weighted net and weights, and
                // points within one rounding of the originals.
                let h = NurbsSurface::from_homogeneous(
                    s.knot_vector(ParamAxis::U).clone(),
                    s.knot_vector(ParamAxis::V).clone(),
                    wp.clone(),
                    w.clone(),
                )
                .unwrap();
                assert_eq!(h.weighted_points(), Some(wp));
                assert_eq!(h.weights(), Some(w));
                for (a, b) in h.points().points().iter().zip(s.points().points()) {
                    for (x, y) in a.iter().zip(b) {
                        assert!((x - y).abs() <= 2.0 * f64::EPSILON * y.abs(), "{s}");
                    }
                }
            } else {
                assert!(s.weighted_points().is_none());
            }
            built += 1;
        }
        assert!(built > 20, "only {built} surfaces");
    }

    /// A knot of multiplicity `p + 1` inside either domain breaks the surface in two along
    /// a whole isoparametric line: both constructors refuse it. The truth: the exact
    /// surface's limits from either side of the knot differ.
    #[test]
    fn surfaces_with_a_break_are_refused() {
        let mut broken = 0;
        for d in (1..=3).flat_map(surfaces).filter(has_break) {
            let exact = d.exact().unwrap();
            let (axis, at) = match break_of(d.degrees.0, &d.knots_u) {
                Some(k) => (ParamAxis::U, k),
                None => (ParamAxis::V, break_of(d.degrees.1, &d.knots_v).unwrap()),
            };
            let differs = [0.0, 0.37, 1.0].iter().any(|&t| {
                let (below, above) = match axis {
                    ParamAxis::U => (
                        exact.point((at, t), (Side::Below, Side::Above)),
                        exact.point((at, t), (Side::Above, Side::Above)),
                    ),
                    ParamAxis::V => (
                        exact.point((t, at), (Side::Above, Side::Below)),
                        exact.point((t, at), (Side::Above, Side::Above)),
                    ),
                };
                below.unwrap().rounded != above.unwrap().rounded
            });
            assert!(differs, "{d:?} is continuous across {at}");
            let e = build(&d).unwrap_err();
            assert!(
                matches!(e, NurbsSurfaceError::BadKnots { axis: a, cause: NurbsError::Multiplicity { knot, .. } } if a == axis && knot == at),
                "{e:?}"
            );
            broken += 1;
        }
        assert!(broken > 10, "only {broken} surfaces with a break");
    }

    #[test]
    fn construction_checks() {
        let u = kv(1, &[0.0, 0.0, 1.0, 1.0]);
        let v = kv(2, &[0.0, 0.0, 0.0, 1.0, 1.0, 1.0]);
        let net = |rows, cols| Net::new(rows, cols, vec![[0.5, 1.0]; rows * cols]).unwrap();
        let ok = NurbsSurface::new(u.clone(), v.clone(), net(2, 3), None).unwrap();
        assert_eq!(
            ok.to_string(),
            "polynomial surface of degrees 1 × 2 in 2 dimensions, 2 × 3 control points on \
             [0, 1] × [0, 1]"
        );
        let bad_net = |c| Err(NurbsSurfaceError::BadNet(c));
        let bad_w = |c| Err(NurbsSurfaceError::BadWeights(c));
        assert_eq!(
            NurbsSurface::new(u.clone(), v.clone(), net(3, 3), None),
            bad_net(NetCause::Count {
                axis: ParamAxis::U,
                count: 3,
                expected: 2
            })
        );
        assert_eq!(
            NurbsSurface::new(u.clone(), v.clone(), net(2, 2), None),
            bad_net(NetCause::Count {
                axis: ParamAxis::V,
                count: 2,
                expected: 3
            })
        );
        let mut pts = vec![[0.0, 0.0]; 6];
        pts[4] = [0.0, f64::NAN];
        let nan = Net::new(2, 3, pts).unwrap();
        assert_eq!(
            NurbsSurface::new(u.clone(), v.clone(), nan.clone(), None),
            bad_net(NetCause::NotFinite { row: 1, col: 1 })
        );
        assert_eq!(
            NurbsSurface::from_homogeneous(
                u.clone(),
                v.clone(),
                nan,
                Net::new(2, 3, vec![1.0; 6]).unwrap()
            ),
            bad_net(NetCause::WeightedNotFinite { row: 1, col: 1 })
        );
        let weights = |w: Vec<f64>| Some(Net::new(2, 3, w).unwrap());
        assert_eq!(
            NurbsSurface::new(
                u.clone(),
                v.clone(),
                net(2, 3),
                Some(Net::new(3, 2, vec![1.0; 6]).unwrap())
            ),
            bad_w(WeightCause::Shape {
                rows: 3,
                cols: 2,
                net_rows: 2,
                net_cols: 3
            })
        );
        for w in [0.0, -1.0, f64::NAN, f64::INFINITY, -0.0] {
            let mut ws = vec![1.0; 6];
            ws[5] = w;
            let e = NurbsSurface::new(u.clone(), v.clone(), net(2, 3), weights(ws)).unwrap_err();
            assert!(
                matches!(e, NurbsSurfaceError::BadWeights(WeightCause::NotPositive { row: 1, col: 2, weight }) if weight.to_bits() == w.to_bits()),
                "{e:?}"
            );
        }
        // Any positive finite weight is a surface: the pin's [1e-3, 100] is not a rule here.
        let tiny = weights(vec![1e-300, 1.0, 1e-4, 1e3, 1e300, 1.0]);
        let huge_points = Net::new(2, 3, vec![[1e10, 1.0]; 6]).unwrap();
        assert!(NurbsSurface::new(u.clone(), v.clone(), net(2, 3), tiny.clone()).is_ok());
        // w P overflows, or P = (w P) / w does.
        assert_eq!(
            NurbsSurface::new(u.clone(), v.clone(), huge_points.clone(), tiny.clone()),
            bad_net(NetCause::WeightedNotFinite { row: 1, col: 1 })
        );
        assert_eq!(
            NurbsSurface::from_homogeneous(u.clone(), v.clone(), huge_points, tiny.unwrap()),
            bad_net(NetCause::NotFinite { row: 0, col: 0 })
        );
        // Unclamped knots, and interior multiplicity p, are surfaces.
        let unclamped = kv(1, &[-1.0, 0.0, 1.0, 2.0]);
        let c0 = kv(2, &[0.0, 0.0, 0.0, 0.5, 0.5, 1.0, 1.0, 1.0]);
        assert!(NurbsSurface::new(unclamped, c0.clone(), net(2, 5), None).is_ok());
        // A break, in v.
        let broken = kv(2, &[0.0, 0.0, 0.0, 0.5, 0.5, 0.5, 1.0, 1.0, 1.0]);
        assert_eq!(
            NurbsSurface::new(u, broken, net(2, 6), None),
            Err(NurbsSurfaceError::BadKnots {
                axis: ParamAxis::V,
                cause: NurbsError::Multiplicity {
                    knot: 0.5,
                    multiplicity: 3,
                    max: 2
                }
            })
        );
    }
}
