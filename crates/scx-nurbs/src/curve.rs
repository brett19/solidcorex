// Written for solidcorex. Apache-2.0.
//! NURBS curves as plain data: validated construction, evaluation with derivatives, knot
//! insertion, splitting and degree elevation.
//!
//! The algorithms and their arithmetic order follow solidcore at the pin:
//! - `nurbs_crv::crv_eval` (the point, A4.1);
//! - `nurbs_crv::crv_derivs` with `rational_basis_derivs_into` (derivatives, A3.2 and the
//!   rational basis functions' quotient rule);
//! - `crv_tool::p1::Curve::insert_knot` (A5.1) and `nurbs_crv::crv_split`;
//! - `crv_tool::p1::Curve::elevate_degree` (A5.9) with `bezier::elevate_degree`, up to its
//!   knot removal, which differs (below).
//!
//! Where the pin's knot insertion combines two control points with `CPoint::combine2`, this
//! uses the affine form (`CPoint::affine2`, [`Control::affine`]), which keeps a coordinate
//! equal in both points exactly, so insertion and splitting keep a plane exactly. Degree
//! elevation keeps the pin's `combine2`: its Bezier sums lose that exactness anyway.
//!
//! Degree elevation does not keep the pin's knot removal. A5.9 solves each removal's
//! equations from both ends to a fixed middle one; across a span many times shorter than
//! its neighbours that divides by a ratio near zero at every step, and the result can be
//! anything (`1e99` off at degree 15). Here the equation dropped is the one that minimises
//! a tracked bound on the rounding, and an elevation whose bound passes
//! [`ELEVATION_ERROR`] is [`NurbsError::Unstable`]. The answers differ from the pin's where
//! the pin's are wrong (`pin_defects` in `scx-difftest`).

use core::fmt;

use scx_num::consts::ELEVATION_ERROR;
use scx_num::extent1d::Extent1d;

use crate::basis::{FULL, SMALL, check_order, derivs_into, values_into};
use crate::bezier::{elevate_into, elevation_matrix};
use crate::point::{Control, Weighted};
use crate::{Approach, KnotVector, MAX_DEGREE, NurbsError, Result};

/// A control point, knot or scratch index the built curve's invariants rule out: a bug.
pub(crate) const MISSING_POINT: NurbsError = NurbsError::Defect {
    why: "a curve routine read an index its counts rule out",
};

/// A NURBS curve in `D` dimensions: a [`KnotVector`] of degree `p`, `m - p` control points,
/// and for a rational curve one positive weight per point.
///
/// Built only by [`NurbsCurve::new`] or [`NurbsCurve::from_homogeneous`], which check that
/// no interior knot repeats more than `p` times (the curve is C0,
/// [`KnotVector::check_continuous`]), the counts, that every coordinate is finite and that
/// every weight is positive and finite.
/// Outside the domain the curve continues each end piece.
#[derive(Debug, Clone, PartialEq)]
pub struct NurbsCurve<const D: usize> {
    knots: KnotVector,
    /// The control points `P[i]`.
    points: Vec<[f64; D]>,
    rational: Option<Rational<D>>,
}

/// The weights of a rational curve, and its homogeneous points `w P`.
#[derive(Debug, Clone, PartialEq)]
struct Rational<const D: usize> {
    weights: Vec<f64>,
    weighted: Vec<[f64; D]>,
}

impl<const D: usize> NurbsCurve<D> {
    /// The curve with control points `points` on `knots`; rational with `weights`.
    pub fn new(
        knots: KnotVector,
        points: Vec<[f64; D]>,
        weights: Option<Vec<f64>>,
    ) -> Result<Self> {
        knots.check_continuous()?;
        check_points(&knots, &points)?;
        let rational = match weights {
            None => None,
            Some(weights) => {
                check_weights(&weights, points.len())?;
                let weighted: Vec<[f64; D]> = points
                    .iter()
                    .zip(&weights)
                    .map(|(p, &w)| p.map(|x| x * w))
                    .collect();
                check_finite(&weighted)?;
                Some(Rational { weights, weighted })
            }
        };
        Ok(Self {
            knots,
            points,
            rational,
        })
    }

    /// The rational curve with homogeneous control points `(weighted[i], weights[i])`, that
    /// is `P[i] = weighted[i] / weights[i]`.
    pub fn from_homogeneous(
        knots: KnotVector,
        weighted: Vec<[f64; D]>,
        weights: Vec<f64>,
    ) -> Result<Self> {
        knots.check_continuous()?;
        check_points(&knots, &weighted)?;
        check_weights(&weights, weighted.len())?;
        let points: Vec<[f64; D]> = weighted
            .iter()
            .zip(&weights)
            .map(|(p, &w)| p.map(|x| x / w))
            .collect();
        check_finite(&points)?;
        Ok(Self {
            knots,
            points,
            rational: Some(Rational { weights, weighted }),
        })
    }

    /// The curve of parts that already satisfy every invariant [`NurbsCurve::new`] checks,
    /// `weighted[i] = points[i] * weights[i]` for a rational curve.
    pub(crate) fn from_valid_parts(
        knots: KnotVector,
        points: Vec<[f64; D]>,
        rational: Option<(Vec<f64>, Vec<[f64; D]>)>,
    ) -> Self {
        let rational = rational.map(|(weights, weighted)| Rational { weights, weighted });
        Self {
            knots,
            points,
            rational,
        }
    }

    /// The knot vector.
    #[must_use]
    pub const fn knot_vector(&self) -> &KnotVector {
        &self.knots
    }

    /// The knots.
    #[must_use]
    pub fn knots(&self) -> &[f64] {
        self.knots.knots()
    }

    /// The degree `p`.
    #[must_use]
    pub const fn degree(&self) -> usize {
        self.knots.degree()
    }

    /// The domain `[U[p], U[m-p]]`.
    #[must_use]
    pub fn domain(&self) -> Extent1d {
        self.knots.domain()
    }

    /// The control points `P[i]`.
    #[must_use]
    pub fn points(&self) -> &[[f64; D]] {
        &self.points
    }

    /// The weights of a rational curve.
    #[must_use]
    pub fn weights(&self) -> Option<&[f64]> {
        self.rational.as_ref().map(|r| r.weights.as_slice())
    }

    /// The homogeneous points `w P` of a rational curve.
    #[must_use]
    pub fn weighted_points(&self) -> Option<&[[f64; D]]> {
        self.rational.as_ref().map(|r| r.weighted.as_slice())
    }

    /// Whether the curve has weights.
    #[must_use]
    pub const fn is_rational(&self) -> bool {
        self.rational.is_some()
    }

    /// The point `C(u)`, on the piece `side` picks at a knot ([`Approach`]).
    ///
    /// Errors when `u` is not finite, or when the evaluation is not finite
    /// ([`NurbsError::Degenerate`]: a knot difference rounds to zero, which needs a span far
    /// shorter than `u`'s magnitude, or a value overflows).
    pub fn point(&self, u: f64, side: Approach) -> Result<[f64; D]> {
        let span = self.knots.span(u, side)?;
        let point = if self.degree() < SMALL {
            self.point_n::<SMALL>(u, span)
        } else {
            self.point_n::<FULL>(u, span)
        };
        Some(point.ok_or(MISSING_POINT)?)
            .filter(|c| c.iter().all(|x| x.is_finite()))
            .ok_or(NurbsError::Degenerate { parameter: u })
    }

    /// [`NurbsCurve::point`] with scratch width `N > p`; `None` only past an index the span
    /// invariants rule out.
    fn point_n<const N: usize>(&self, u: f64, span: usize) -> Option<[f64; D]> {
        let p = self.degree();
        let mut n = [0.0; N];
        values_into(self.knots.window(span)?, u, &mut n);
        let local = span - p..=span;
        match &self.rational {
            None => {
                let mut c = [0.0; D];
                for (&alpha, q) in n.iter().zip(self.points.get(local)?) {
                    c = c.blend(alpha, *q);
                }
                Some(c)
            }
            Some(r) => {
                let mut c = [0.0; D];
                let mut w = 0.0;
                let pairs = r
                    .weighted
                    .get(local.clone())?
                    .iter()
                    .zip(r.weights.get(local)?);
                for (&alpha, (q, &wq)) in n.iter().zip(pairs) {
                    c = c.blend(alpha, *q);
                    w += alpha * wq;
                }
                Some(c.map(|x| x / w))
            }
        }
    }

    /// `C(u)` and its derivatives `C^(k)(u)` for `k < K`, on the piece `side` picks.
    ///
    /// Errors as [`NurbsCurve::point`], and when `K - 1` is above
    /// [`MAX_ORDER`](crate::MAX_ORDER).
    pub fn derivs<const K: usize>(&self, u: f64, side: Approach) -> Result<[[f64; D]; K]> {
        check_order(K)?;
        let span = self.knots.span(u, side)?;
        let derivs = if self.degree() < SMALL {
            self.derivs_n::<SMALL, K>(u, span)
        } else {
            self.derivs_n::<FULL, K>(u, span)
        };
        Some(derivs.ok_or(MISSING_POINT)?)
            .filter(|c| c.iter().flatten().all(|x| x.is_finite()))
            .ok_or(NurbsError::Degenerate { parameter: u })
    }

    /// [`NurbsCurve::derivs`] with scratch width `N > p`.
    fn derivs_n<const N: usize, const K: usize>(
        &self,
        u: f64,
        span: usize,
    ) -> Option<[[f64; D]; K]> {
        let p = self.degree();
        let mut nd = [[0.0; N]; K];
        let window = self.knots.window(span)?;
        if let Some((values, derivs)) = nd.split_first_mut() {
            derivs_into(window, u, values, derivs)?;
        }
        let local = span - p..=span;
        let points = self.points.get(local.clone())?;
        let mut out = [[0.0; D]; K];
        match &self.rational {
            None => {
                // C^(k) = sum_i N^(k)[i] P[i].
                for (c, row) in out.iter_mut().zip(&nd) {
                    for (&b, q) in row.iter().zip(points) {
                        *c = c.blend(b, *q);
                    }
                }
            }
            Some(r) => {
                let weights = r.weights.get(local)?;
                // d[k] = sum_j w[j] N^(k)[j], the derivatives of the weight function.
                let mut d = [0.0; K];
                for (dk, row) in d.iter_mut().zip(&nd) {
                    for (&b, &w) in row.iter().zip(weights) {
                        *dk += w * b;
                    }
                }
                let d0 = *d.first()?;
                let binomials = pascal::<K>();
                // R^(k)[i] = (w[i] N^(k)[i] - sum_{j=1..=k} C(k, j) d[j] R^(k-j)[i]) / d[0],
                // then C^(k) = sum_i R^(k)[i] P[i].
                for (i, (&w, q)) in weights.iter().zip(points).enumerate() {
                    let mut col = [0.0; K];
                    for (k, (row, brow)) in nd.iter().zip(&binomials).enumerate() {
                        let (done, rest) = col.split_at_mut(k);
                        let mut v = w * *row.get(i)?;
                        for ((&b, &dj), &rk) in brow
                            .iter()
                            .skip(1)
                            .zip(d.iter().skip(1))
                            .zip(done.iter().rev())
                        {
                            v -= b * dj * rk;
                        }
                        *rest.first_mut()? = v / d0;
                    }
                    for (c, &rk) in out.iter_mut().zip(&col) {
                        *c = c.blend(rk, *q);
                    }
                }
            }
        }
        Some(out)
    }

    /// The same curve with the knot `u` inserted `times` times (A5.1).
    ///
    /// `u` must lie strictly inside the domain, and its multiplicity after the insertion
    /// must not exceed the degree.
    pub fn insert_knot(&self, u: f64, times: usize) -> Result<Self> {
        let multiplicity = self.knots.check_insertion(u, times)?;
        if times == 0 {
            return Ok(self.clone());
        }
        let knots = self.knots.inserted(u, times)?;
        let span = self.knots.span_of(u, Approach::Above);
        let plan = Insertion {
            knots: self.knots.knots(),
            degree: self.degree(),
            span,
            multiplicity,
            times,
            u,
        };
        match &self.rational {
            None => {
                let points = plan.points(&self.points).ok_or(MISSING_POINT)?;
                Self::new(knots, points, None)
            }
            Some(r) => {
                let (weighted, weights) = plan
                    .points(&r.homogeneous())
                    .map(split_weights)
                    .ok_or(MISSING_POINT)?;
                Self::from_homogeneous(knots, weighted, weights)
            }
        }
    }

    /// The two pieces of the curve over `[U[p], u]` and `[u, U[m-p]]`, for `u` strictly
    /// inside the domain, as `crv_split` at the pin: `u` is inserted until its multiplicity
    /// is the degree, and each piece gets end knots of multiplicity `p + 1`.
    ///
    /// The pin's `Curve::split` first moves a `u` within `1e-6` of the domain length onto a
    /// knot; this splits at `u` as given.
    pub fn split(&self, u: f64) -> Result<(Self, Self)> {
        self.knots.check_inside(u)?;
        let p = self.degree();
        let missing = p.saturating_sub(self.knots.multiplicity(u));
        let full = self.insert_knot(u, missing)?;
        let knots = full.knots();
        let first = knots.partition_point(|&k| k < u);
        let after = knots.partition_point(|&k| k <= u);
        let ends = core::iter::repeat_n(u, p + 1);
        let (below, _) = knots.split_at(first);
        let (_, above) = knots.split_at(after);
        let left_knots = KnotVector::new(p, below.iter().copied().chain(ends.clone()).collect())?;
        let right_knots = KnotVector::new(p, ends.chain(above.iter().copied()).collect())?;
        let left_count = left_knots.point_count();
        let right_from = full.points.len() - right_knots.point_count();
        let piece = |knots: KnotVector, range: core::ops::Range<usize>| match &full.rational {
            None => {
                let points = full.points.get(range).ok_or(MISSING_POINT)?;
                Self::new(knots, points.to_vec(), None)
            }
            Some(r) => {
                let weighted = r.weighted.get(range.clone()).ok_or(MISSING_POINT)?;
                let weights = r.weights.get(range).ok_or(MISSING_POINT)?;
                Self::from_homogeneous(knots, weighted.to_vec(), weights.to_vec())
            }
        };
        Ok((
            piece(left_knots, 0..left_count)?,
            piece(right_knots, right_from..full.points.len())?,
        ))
    }

    /// The same curve with its degree raised by `times` (A5.9): every distinct knot gains
    /// `times` in multiplicity, so the continuity at each is kept.
    ///
    /// The end knots must have multiplicity `p + 1` ([`NurbsError::Unclamped`]), and
    /// `p + times` must not exceed
    /// [`MAX_DEGREE`](crate::MAX_DEGREE).
    ///
    /// Accuracy: the knots A5.9 inserts are removed again by extrapolation, each removal
    /// solved the way that bounds its rounding least (not A5.9's fixed middle), so
    /// near-coincident knots cost nothing. The bound still grows with the degree, fastest
    /// on uniform knots; when it passes [`ELEVATION_ERROR`] of the largest homogeneous
    /// coordinate, or a weight stops being positive or a point finite, this errs with
    /// [`NurbsError::Unstable`] (the pin returns the curve). Seeded curves up to degree 23
    /// come within `4e-11` of the exact curve; uniform knots raised to degree 28 or more are
    /// refused.
    pub fn elevate(&self, times: usize) -> Result<Self> {
        let p = self.degree();
        if p.saturating_add(times) > MAX_DEGREE {
            return Err(NurbsError::Degree {
                degree: p.saturating_add(times),
                max: MAX_DEGREE,
            });
        }
        if !self.knots.is_clamped() {
            return Err(NurbsError::Unclamped);
        }
        if times == 0 {
            return Ok(self.clone());
        }
        let dm = elevation_matrix(p, times)?;
        let plan = Elevation {
            knots: self.knots.knots(),
            degree: p,
            times,
            spans: self.knots.span_count(),
            matrix: &dm,
        };
        let lost = |e| match e {
            NurbsError::PointNotFinite { .. } | NurbsError::Weight { .. } => NurbsError::Unstable,
            e => e,
        };
        match &self.rational {
            None => {
                let (points, knots) = plan.points(&self.points)?;
                Self::new(KnotVector::new(p + times, knots)?, points, None).map_err(lost)
            }
            Some(r) => {
                let (points, knots) = plan.points(&r.homogeneous())?;
                let (weighted, weights) = split_weights(points);
                Self::from_homogeneous(KnotVector::new(p + times, knots)?, weighted, weights)
                    .map_err(lost)
            }
        }
    }
}

impl<const D: usize> Rational<D> {
    fn homogeneous(&self) -> Vec<Weighted<D>> {
        self.weighted
            .iter()
            .zip(&self.weights)
            .map(|(&point, &weight)| Weighted { point, weight })
            .collect()
    }
}

pub(crate) fn split_weights<const D: usize>(points: Vec<Weighted<D>>) -> (Vec<[f64; D]>, Vec<f64>) {
    points.into_iter().map(|q| (q.point, q.weight)).unzip()
}

fn check_points<const D: usize>(knots: &KnotVector, points: &[[f64; D]]) -> Result<()> {
    let expected = knots.point_count();
    if points.len() != expected {
        return Err(NurbsError::PointCount {
            count: points.len(),
            expected,
        });
    }
    check_finite(points)
}

pub(crate) fn check_finite<const D: usize>(points: &[[f64; D]]) -> Result<()> {
    match points.iter().position(|p| p.iter().any(|x| !x.is_finite())) {
        Some(index) => Err(NurbsError::PointNotFinite { index }),
        None => Ok(()),
    }
}

pub(crate) fn check_weights(weights: &[f64], count: usize) -> Result<()> {
    if weights.len() != count {
        return Err(NurbsError::WeightCount {
            count: weights.len(),
            expected: count,
        });
    }
    match weights.iter().position(|&w| !(w > 0.0 && w.is_finite())) {
        Some(index) => Err(NurbsError::Weight {
            index,
            weight: weights.get(index).copied().unwrap_or(f64::NAN),
        }),
        None => Ok(()),
    }
}

/// The rows `C(k, 0..K)` of Pascal's triangle for `k < K`, exact (`K <= MAX_ORDER + 1`).
fn pascal<const K: usize>() -> [[f64; K]; K] {
    let mut rows = [[0.0; K]; K];
    let mut prev = [0.0; K];
    for (k, row) in rows.iter_mut().enumerate() {
        if k == 0 {
            if let Some(v) = row.first_mut() {
                *v = 1.0;
            }
        } else {
            let mut left = 0.0;
            for (v, &pv) in row.iter_mut().zip(&prev).take(k + 1) {
                *v = left + pv;
                left = pv;
            }
        }
        prev = *row;
    }
    rows
}

/// `v[i]`.
fn get<T: Copy>(v: &[T], i: isize) -> Option<T> {
    v.get(usize::try_from(i).ok()?).copied()
}

/// `v[i] = x`.
fn set<T>(v: &mut [T], i: isize, x: T) -> Option<()> {
    *v.get_mut(usize::try_from(i).ok()?)? = x;
    Some(())
}

/// The control points of a knot insertion (A5.1).
pub(crate) struct Insertion<'a> {
    pub(crate) knots: &'a [f64],
    pub(crate) degree: usize,
    /// The span of `u` from above.
    pub(crate) span: usize,
    pub(crate) multiplicity: usize,
    pub(crate) times: usize,
    pub(crate) u: f64,
}

impl Insertion<'_> {
    pub(crate) fn points<P: Control>(&self, pw: &[P]) -> Option<Vec<P>> {
        let mut qw = vec![P::ZERO; pw.len() + self.times];
        self.points_into(pw, &mut qw, &mut Vec::with_capacity(self.degree + 1))?;
        Some(qw)
    }

    /// [`Insertion::points`] into `qw` (`pw.len() + times` long), with the scratch `rw`.
    pub(crate) fn points_into<P: Control>(
        &self,
        pw: &[P],
        qw: &mut [P],
        rw: &mut Vec<P>,
    ) -> Option<()> {
        let (up, u) = (self.knots, self.u);
        let to = |x: usize| isize::try_from(x).ok();
        let (p, k, s, r) = (
            to(self.degree)?,
            to(self.span)?,
            to(self.multiplicity)?,
            to(self.times)?,
        );
        let n = to(pw.len())? - 1;
        if qw.len() != pw.len() + self.times {
            return None;
        }
        rw.clear();
        for i in 0..=p - s {
            rw.push(get(pw, k - p + i)?);
        }
        // The unaltered control points.
        for i in 0..=k - p {
            set(qw, i, get(pw, i)?)?;
        }
        for i in k - s..=n {
            set(qw, i + r, get(pw, i)?)?;
        }
        // The new ones.
        let mut l = k - p;
        for i in 1..=r {
            l = k - p + i;
            for j in 0..=p - i - s {
                let alf = (u - get(up, l + j)?) / (get(up, k + j + 1)? - get(up, l + j)?);
                let c = P::affine(alf, get(rw, j + 1)?, get(rw, j)?);
                set(rw, j, c)?;
            }
            set(qw, l, get(rw, 0)?)?;
            set(qw, k + r - i - s, get(rw, p - i - s)?)?;
        }
        for i in l + 1..k - s {
            set(qw, i, get(rw, i - l)?)?;
        }
        Some(())
    }
}

/// The control points and knots of a degree elevation (A5.9's plan, with a stable knot
/// removal).
pub(crate) struct Elevation<'a> {
    pub(crate) knots: &'a [f64],
    pub(crate) degree: usize,
    pub(crate) times: usize,
    /// The number of non-empty spans.
    pub(crate) spans: usize,
    pub(crate) matrix: &'a [Vec<f64>],
}

impl Elevation<'_> {
    /// The elevated control points and knots.
    ///
    /// The curve is cut into Bezier pieces (A5.9's knot insertion: every ratio in
    /// `[0, 1]`), each piece is elevated by the matrix (positive weights that sum to one),
    /// and then each joint's surplus knots are removed one at a time by
    /// [`Joined::remove_last_knot`]. The removal tracks a first-order bound on each point's
    /// rounding error; the elevation is [`NurbsError::Unstable`] when the bound passes
    /// [`ELEVATION_ERROR`] of the largest coordinate. Errs with [`MISSING_POINT`] for an
    /// index the plan's counts rule out.
    pub(crate) fn points<P: Control>(&self, pw: &[P]) -> Result<(Vec<P>, Vec<f64>)> {
        let (points, knots, bound) = self.join(pw).ok_or(MISSING_POINT)?;
        // NaN refuses.
        if bound * (f64::EPSILON / 2.0) <= ELEVATION_ERROR {
            Ok((points, knots))
        } else {
            Err(NurbsError::Unstable)
        }
    }

    /// The points, the knots and the largest error bound, in units of the rounding of the
    /// largest coordinate.
    fn join<P: Control>(&self, pw: &[P]) -> Option<(Vec<P>, Vec<f64>, f64)> {
        let up = self.knots;
        let (p, t) = (self.degree, self.times);
        let q = p + t;
        let m = up.len().checked_sub(1)?;
        let count = pw.len() + t * self.spans;
        let mut out = Joined {
            points: Vec::with_capacity(count + q),
            knots: Vec::with_capacity(up.len() + t * (self.spans + 1)),
            err: Vec::with_capacity(count + q),
            steps: Vec::with_capacity(q),
        };
        // Each Bezier point starts at one rounding: the extraction and the matrix are convex
        // sums, whose few roundings the removal's growth swamps (the bound is first order).
        let start = 1.0;
        let mut bw: Vec<P> = pw.get(..=p)?.to_vec();
        let mut nw = vec![P::ZERO; p + 1];
        let mut dw = vec![P::ZERO; q + 1];
        let mut alfs = vec![0.0; p + 1];

        let mut a = p;
        let mut b = p + 1;
        // The multiplicity of the joint at `up[a]` (none before the first piece).
        let mut joint: Option<usize> = None;
        while b < m {
            // The multiplicity of the knot `up[b]` that ends this piece.
            let i0 = b;
            while b < m && up.get(b)? == up.get(b + 1)? {
                b += 1;
            }
            let mlt = b - i0 + 1;
            let (ua, ub) = (*up.get(a)?, *up.get(b)?);

            // Insert `up[b]` until its multiplicity is `p`, for the Bezier piece on
            // `[ua, ub]`.
            let r = p.saturating_sub(mlt);
            if r > 0 {
                let num = ub - ua;
                for k in mlt + 1..=p {
                    *alfs.get_mut(k - mlt - 1)? = num / (up.get(a + k)? - ua);
                }
                for j in 1..=r {
                    let s = mlt + j;
                    for k in (s..=p).rev() {
                        let al = *alfs.get(k - s)?;
                        let c = P::combine(al, *bw.get(k)?, 1.0 - al, *bw.get(k - 1)?);
                        *bw.get_mut(k)? = c;
                    }
                    *nw.get_mut(r - j)? = *bw.get(p)?;
                }
            }

            // Elevate it, and join it to the curve so far.
            elevate_into(&bw, t, self.matrix, 0, &mut dw);
            match joint {
                None => {
                    out.points.extend_from_slice(&dw);
                    out.err.extend(core::iter::repeat_n(start, q + 1));
                    out.knots.extend(core::iter::repeat_n(ua, q + 1));
                }
                Some(joined) => {
                    // The pieces share the point at the joint.
                    out.points.extend_from_slice(dw.get(1..)?);
                    out.err.extend(core::iter::repeat_n(start, q));
                    out.knots.extend(core::iter::repeat_n(ua, q));
                    // The joint holds `q` copies of `ua`, one fewer after each removal.
                    for removed in 0..p.saturating_sub(joined) {
                        out.remove_last_knot(q, q - 1 - removed, ub)?;
                    }
                }
            }

            // Set up the next piece.
            if b < m {
                for i in 0..r {
                    *bw.get_mut(i)? = *nw.get(i)?;
                }
                for i in r..=p {
                    *bw.get_mut(i)? = *pw.get(b - p + i)?;
                }
                joint = Some(mlt);
                a = b;
                b += 1;
            } else {
                out.knots.extend(core::iter::repeat_n(ub, q + 1));
            }
        }
        let bound = out.err.iter().fold(0.0_f64, |m, &x| m.max(x));
        Some((out.points, out.knots, bound))
    }
}

/// One equation of a knot removal, solved either way: forward
/// `P[e] = f_q Q[e] - f_p P[e - 1]`, backward `P[e - 1] = b_q Q[e] - b_p P[e]`, with the
/// error bounds of the point each gives.
#[derive(Clone, Copy)]
struct Step {
    forward: (f64, f64),
    backward: (f64, f64),
    forward_err: f64,
    /// The largest `forward_err` of the equations before this one.
    forward_before: f64,
    backward_err: f64,
}

/// The elevated curve as it is built: its points, its knots so far, and for each point a
/// first-order bound on its rounding error, in units of the rounding of the largest
/// coordinate (every point of every stage is a convex combination of the control points,
/// so none is larger).
struct Joined<P> {
    points: Vec<P>,
    knots: Vec<f64>,
    err: Vec<f64>,
    steps: Vec<Step>,
}

impl<P: Control> Joined<P> {
    /// Removes one copy of `u`, the last knot so far and one of its `s + 1` copies, from
    /// the degree-`q` spline, whose knots past those so far are all `right` (at least `q`
    /// of them).
    ///
    /// Inserting `u` back would give the old points `Q` from the new ones `P` (Boehm):
    /// `Q[e] = alpha[e] P[e] + (1 - alpha[e]) P[e - 1]` with
    /// `alpha[e] = (u - T[e]) / (T[e + q] - T[e])` for `e` from `k - q + 1` to `k - s`,
    /// where `T` is the new knot vector, `k` its last copy of `u` and `s` its count of
    /// them; outside that range `P` is `Q` shifted. That is one equation more than there
    /// are new points, so one equation `d` is dropped: those before it are solved forward
    /// (dividing by `alpha`), those after it backward (dividing by `1 - alpha`). A5.9 drops
    /// the middle one by position; across a span many times shorter than its neighbours
    /// that divides by a ratio near zero at every step, and the rounding grows by its
    /// reciprocal (`1e81` at degree 15). Here `d` is the one that gives the smallest bound
    /// on the new points' error, the bound carried through every removal before.
    fn remove_last_knot(&mut self, q: usize, s: usize, right: f64) -> Option<()> {
        let u = *self.knots.last()?;
        let k = self.knots.len().checked_sub(2)?;
        let (lo, hi) = (k.checked_sub(q - 1)?, k.checked_sub(s)?);

        // The equations' coefficients and the bounds either way: forward_err of e is the
        // bound on P[e] solved forward from lo, backward_err of e that on P[e - 1] solved
        // backward from hi. Every `T[e + q]` here is past `k`, so `right`.
        let lows = self.knots.get(lo..=hi)?;
        let errs = self.err.get(lo..=hi)?;
        let above = right - u;
        self.steps.clear();
        let mut prev = *self.err.get(lo - 1)?;
        let mut forward_worst = 0.0_f64;
        for (&low, &here) in lows.iter().zip(errs) {
            let (below, width) = (u - low, right - low);
            let forward = (width / below, above / below);
            let backward = (width / above, below / above);
            let forward_before = forward_worst;
            prev = forward.0 * (here + 1.0) + forward.1 * (prev + 1.0);
            forward_worst = forward_worst.max(prev);
            self.steps.push(Step {
                forward,
                backward,
                forward_err: prev,
                forward_before,
                backward_err: 0.0,
            });
        }

        // The drop d: forward for e < d gives P[lo..d], backward for e > d gives
        // P[d..=hi - 1]. Scanned from the top with the backward bounds, keeping the worst
        // either way (the bounds need not grow monotonically: a step may divide by more
        // than one or less). Ties go to the lower d.
        let mut best = (f64::INFINITY, hi);
        let mut worst_backward = 0.0_f64;
        let mut next = *self.err.get(hi + 1)?;
        for (i, (step, &here)) in self.steps.iter_mut().zip(errs).enumerate().rev() {
            let worst = step.forward_before.max(worst_backward);
            if worst <= best.0 {
                best = (worst, lo + i);
            }
            next = step.backward.0 * (here + 1.0) + step.backward.1 * (next + 1.0);
            step.backward_err = next;
            worst_backward = worst_backward.max(next);
        }
        let drop = best.1;

        // Forward: P[e] = (Q[e] - (1 - alpha) P[e - 1]) / alpha, in place.
        let split = drop - lo;
        let (forward, rest) = self.steps.split_at(split);
        let mut prev = *self.points.get(lo - 1)?;
        let points = self.points.get_mut(lo..drop)?;
        let errs = self.err.get_mut(lo..drop)?;
        for ((point, err), step) in points.iter_mut().zip(errs).zip(forward) {
            let (fq, fp) = step.forward;
            prev = P::combine(fq, *point, -fp, prev);
            *point = prev;
            *err = step.forward_err;
        }
        // Backward: P[e - 1] = (Q[e] - alpha P[e]) / (1 - alpha), with P[e] held at e + 1.
        let mut next = *self.points.get(hi + 1)?;
        let points = self.points.get_mut(drop + 1..=hi)?;
        let errs = self.err.get_mut(drop + 1..=hi)?;
        let backward = rest.get(1..)?;
        for ((point, err), step) in points.iter_mut().zip(errs).zip(backward).rev() {
            let (bq, bp) = step.backward;
            next = P::combine(bq, *point, -bp, next);
            *point = next;
            *err = step.backward_err;
        }
        if drop >= self.points.len() {
            return None;
        }
        self.points.remove(drop);
        self.err.remove(drop);
        self.knots.pop();
        Some(())
    }
}

impl<const D: usize> fmt::Display for NurbsCurve<D> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let (low, high) = self.knots.ends();
        write!(
            f,
            "{} curve of degree {} in {D} dimensions, {} control points on [{low}, {high}]",
            if self.is_rational() {
                "rational"
            } else {
                "polynomial"
            },
            self.degree(),
            self.points.len()
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use scx_num::hash::Rng;
    use scx_testkit::nurbs as tk;
    use scx_testkit::sample::{self, Knots, Spec, Weights};

    const SIDES: [(Approach, tk::Side); 2] = [
        (Approach::Below, tk::Side::Below),
        (Approach::Above, tk::Side::Above),
    ];

    fn ours(d: &scx_testkit::data::CurveData<3>) -> NurbsCurve<3> {
        let knots = KnotVector::new(d.degree, d.knots.clone()).unwrap();
        NurbsCurve::new(knots, d.points.clone(), d.weights.clone()).unwrap()
    }

    /// Whether an interior knot repeats `p + 1` times (`Knots::Multiple` makes some).
    fn has_break(d: &scx_testkit::data::CurveData<3>) -> bool {
        d.knots
            .iter()
            .filter(|&&k| k > 0.0 && k < 1.0)
            .any(|&k| d.knots.iter().filter(|&&x| x == k).count() > d.degree)
    }

    /// Random curves of every knot and weight kind, degrees 1..=5 and 9 (the wide kernel);
    /// the tests skip those [`has_break`] picks, which are not curves.
    fn curves(seed: u64) -> Vec<scx_testkit::data::CurveData<3>> {
        let mut rng = Rng::new(seed);
        let mut out = Vec::new();
        for degree in [1, 2, 3, 4, 5, 9] {
            for knots in [
                Knots::Uniform,
                Knots::Random,
                Knots::Clustered,
                Knots::Multiple,
            ] {
                for weights in [Weights::Unit, Weights::Moderate, Weights::NearZero] {
                    let spec = Spec {
                        degree,
                        count: degree + 4,
                        knots,
                    };
                    out.push(sample::curve(&mut rng, spec, weights));
                }
            }
        }
        out
    }

    /// `|got - want| <= tol (1 + |want|)` for every coordinate.
    fn close(got: &[f64; 3], want: &[f64; 3], tol: f64) -> bool {
        got.iter()
            .zip(want)
            .all(|(g, w)| (g - w).abs() <= tol * (1.0 + w.abs()))
    }

    #[test]
    fn points_and_derivatives_match_the_exact_curve() {
        let mut rng = Rng::new(7);
        for data in curves(1).into_iter().filter(|d| !has_break(d)) {
            let c = ours(&data);
            let exact = data.exact().unwrap();
            for _ in 0..3 {
                let u = sample::parameter(&mut rng, &data.knots, (-0.1, 1.1));
                for (side, tside) in SIDES {
                    let want = exact.derivs(u, tside, 3).unwrap();
                    let d = c.derivs::<4>(u, side).unwrap();
                    let p = c.point(u, side).unwrap();
                    assert!(close(&p, &want[0].rounded, 1e-14), "{c} at {u} {side}");
                    // Derivatives lose accuracy with the span lengths (clustered knots).
                    let scale = want
                        .iter()
                        .flat_map(|e| e.rounded)
                        .fold(1.0_f64, |m, x| m.max(x.abs()));
                    for (k, (got, w)) in d.iter().zip(&want).enumerate() {
                        let tol = 1e-12 * [1.0, 10.0, 100.0, 1000.0][k];
                        let ok = got
                            .iter()
                            .zip(&w.rounded)
                            .all(|(g, x)| (g - x).abs() <= tol * scale);
                        assert!(
                            ok,
                            "{c} derivative {k} at {u} {side}: {got:?} vs {:?}",
                            w.rounded
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn domain_ends_are_the_end_points() {
        for data in curves(2).into_iter().filter(|d| !has_break(d)) {
            let c = ours(&data);
            let (first, last) = (c.points()[0], *c.points().last().unwrap());
            for side in [Approach::Below, Approach::Above] {
                assert!(close(&c.point(0.0, side).unwrap(), &first, 1e-15));
                assert!(close(&c.point(1.0, side).unwrap(), &last, 1e-15));
            }
        }
    }

    #[test]
    fn insertion_split_and_elevation_keep_the_curve() {
        let mut rng = Rng::new(9);
        for data in curves(3).into_iter().step_by(2) {
            // Drawn before the skip, so every curve meets the same parameter as before.
            let u = sample::parameter(&mut rng, &data.knots, (0.05, 0.95));
            if has_break(&data) {
                continue;
            }
            let c = ours(&data);
            let exact = data.exact().unwrap();
            let free = c.degree() - c.knot_vector().multiplicity(u).min(c.degree());
            let check = |d: &NurbsCurve<3>, what: &str, tol: f64| {
                for v in [0.03, 0.31, 0.5, 0.77, 0.97] {
                    let want = exact.point(v, tk::Side::Above).unwrap().rounded;
                    let got = d.point(v, Approach::Above).unwrap();
                    assert!(
                        close(&got, &want, tol),
                        "{what} of {c} at {u}: {v} {got:?} {want:?} {:?} tol {tol}",
                        d.knots()
                    );
                }
            };
            if free > 0 {
                let i = c.insert_knot(u, free).unwrap();
                let want = exact.insert_knot(u, free).unwrap().rounded();
                assert_eq!(i.knots(), want.knots.as_slice());
                for (g, w) in i.points().iter().zip(&want.points) {
                    assert!(close(g, w, 1e-13), "insertion of {c} at {u}");
                }
                check(&i, "insertion", 1e-12);
            }
            let (l, r) = c.split(u).unwrap();
            for v in [0.0, 0.25, 0.5, 0.75, 1.0] {
                let (piece, at) = if v < 0.5 {
                    (&l, u * v * 2.0)
                } else {
                    (&r, u + (1.0 - u) * (v - 0.5) * 2.0)
                };
                let want = exact.point(at, tk::Side::Above).unwrap().rounded;
                assert!(
                    close(&piece.point(at, Approach::Below).unwrap(), &want, 1e-12),
                    "split of {c} at {u}"
                );
            }
            let e = c.elevate(2).unwrap();
            let want = exact.elevate(2).unwrap().rounded();
            assert_eq!(e.knots(), want.knots.as_slice());
            let tol = 1e-12;
            check(&e, "elevation", tol);
            for (g, w) in e.points().iter().zip(&want.points) {
                assert!(close(g, w, tol), "elevation of {c}: {g:?} vs {w:?}");
            }
        }
    }

    /// Random curves of degree `degree` with the given knot kinds, unit and moderate
    /// weights, `each` of every pair, those [`has_break`] picks left out.
    fn high_degree(
        rng: &mut Rng,
        degree: usize,
        kinds: &[Knots],
        each: usize,
    ) -> Vec<scx_testkit::data::CurveData<3>> {
        let mut out = Vec::new();
        for &knots in kinds {
            for weights in [Weights::Unit, Weights::Moderate] {
                for _ in 0..each {
                    let spec = Spec {
                        degree,
                        count: degree + 4,
                        knots,
                    };
                    out.push(sample::curve(rng, spec, weights));
                }
            }
        }
        out.retain(|d| !has_break(d));
        out
    }

    /// The largest coordinate of the curve's homogeneous control points: the scale
    /// [`ELEVATION_ERROR`] is relative to.
    fn scale(c: &NurbsCurve<3>) -> f64 {
        let weights = c.weights().map(<[f64]>::to_vec);
        c.points()
            .iter()
            .enumerate()
            .map(|(i, x)| {
                let w = weights.as_ref().map_or(1.0, |w| w[i]);
                x.iter().fold(w.abs(), |m, v| m.max((v * w).abs()))
            })
            .fold(0.0, f64::max)
    }

    /// Degree 15 raised by 3, clustered knots among them: A5.9 dropped the middle equation of
    /// each knot removal and across a span many times shorter than its neighbours divided by
    /// a ratio near zero at every step (`1e99` off on clustered knots, `0.46` on random ones,
    /// the rational ones mostly refused as `Unstable` by a negative weight). The truth is the
    /// exact curve (the exact elevation is the same curve; computing it takes seconds a
    /// curve): every distinct knot gains 3 in multiplicity, and the elevated curve is within
    /// `1e-12` of the scale of the exact one, inside the shortest span too.
    #[test]
    fn elevation_at_degree_15_keeps_the_exact_curve() {
        let mut rng = Rng::new(15);
        let mut clustered = 0;
        let kinds = [
            Knots::Uniform,
            Knots::Random,
            Knots::Clustered,
            Knots::Multiple,
        ];
        for data in high_degree(&mut rng, 15, &kinds, 1) {
            let c = ours(&data);
            let exact = data.exact().unwrap();
            let e = c.elevate(3).unwrap();
            let mut want = Vec::new();
            for (k, m) in c.knot_vector().breakpoints() {
                want.extend(core::iter::repeat_n(k, m + 3));
            }
            assert_eq!(e.knots(), want.as_slice(), "{c}");
            let shortest = c
                .knots()
                .windows(2)
                .filter(|w| w[0] < w[1])
                .min_by(|a, b| (a[1] - a[0]).total_cmp(&(b[1] - b[0])))
                .map(|w| (w[0] + w[1]) / 2.0)
                .unwrap();
            let tol = 1e-12 * scale(&c);
            for v in [0.03, 0.31, 0.5, 0.77, 0.97, shortest] {
                let want = exact.point(v, tk::Side::Above).unwrap().rounded;
                let got = e.point(v, Approach::Above).unwrap();
                assert!(close(&got, &want, tol), "{c} at {v}: {got:?} vs {want:?}");
            }
            clustered += usize::from(
                c.knots()
                    .windows(2)
                    .any(|w| w[0] < w[1] && w[1] - w[0] < 1e-6),
            );
        }
        assert!(clustered >= 1, "no curve with near-coincident knots");
    }

    /// Above degree 20 the removal's error bound can pass [`ELEVATION_ERROR`] (uniform knots
    /// first, where every removal extrapolates as far as it can): such an elevation is
    /// `Unstable`, and every one it answers is within that of the exact curve.
    #[test]
    fn elevation_with_an_unbounded_error_is_unstable() {
        let mut rng = Rng::new(28);
        let mut refused = 0;
        for degree in [25, 28] {
            for data in high_degree(&mut rng, degree, &[Knots::Uniform, Knots::Clustered], 1) {
                let c = ours(&data);
                match c.elevate(3) {
                    Err(e) => {
                        assert_eq!(e, NurbsError::Unstable, "{c}");
                        refused += 1;
                    }
                    Ok(e) => {
                        let exact = data.exact().unwrap();
                        let tol = ELEVATION_ERROR * scale(&c);
                        for v in [0.31, 0.77] {
                            let want = exact.point(v, tk::Side::Above).unwrap().rounded;
                            let got = e.point(v, Approach::Above).unwrap();
                            assert!(close(&got, &want, tol), "{c} at {v}: {got:?} vs {want:?}");
                        }
                    }
                }
            }
        }
        assert!(refused >= 2, "only {refused} refused");
    }

    /// A knot of multiplicity `p + 1` inside the domain is a break, not a curve: both
    /// constructors refuse it, while the knot vector itself is valid (04, 11-27 scx-nurbs
    /// ruling 2). The truth: the exact curve's limits from either side of the knot are two
    /// different control points.
    #[test]
    fn curves_with_a_break_are_refused() {
        let mut broken = 0;
        for data in (1..=5).flat_map(curves).filter(has_break) {
            let knots = KnotVector::new(data.degree, data.knots.clone()).unwrap();
            let (knot, p) = (
                knots
                    .breakpoints()
                    .find(|&(k, m)| k > 0.0 && k < 1.0 && m > data.degree),
                data.degree,
            );
            let (knot, _) = knot.unwrap();
            let exact = data.exact().unwrap();
            let below = exact.point(knot, tk::Side::Below).unwrap().rounded;
            let above = exact.point(knot, tk::Side::Above).unwrap().rounded;
            assert_ne!(below, above, "{data:?} is continuous at {knot}");
            let want = NurbsError::Multiplicity {
                knot,
                multiplicity: p + 1,
                max: p,
            };
            assert_eq!(knots.check_continuous(), Err(want));
            let weighted = data.points.clone();
            let weights = vec![1.0; weighted.len()];
            assert_eq!(
                NurbsCurve::new(knots.clone(), data.points.clone(), data.weights.clone()),
                Err(want)
            );
            assert_eq!(
                NurbsCurve::from_homogeneous(knots, weighted, weights),
                Err(want)
            );
            broken += 1;
        }
        assert!(broken > 10, "only {broken} curves with a break");
        // Multiplicity p at an interior knot (C0) and p + 1 at the ends are curves.
        let k = KnotVector::new(2, vec![0.0, 0.0, 0.0, 0.5, 0.5, 1.0, 1.0, 1.0]).unwrap();
        assert_eq!(k.check_continuous(), Ok(()));
        assert!(NurbsCurve::new(k, vec![[0.0]; 5], None).is_ok());
        let k = KnotVector::new(2, vec![0.0, 0.0, 0.0, 0.5, 0.5, 0.5, 1.0, 1.0, 1.0]).unwrap();
        assert!(NurbsCurve::new(k, vec![[0.0]; 6], None).is_err());
        // Unclamped knots with an interior knot of multiplicity p.
        let k = KnotVector::new(2, vec![-2.0, -1.0, 0.0, 0.5, 0.5, 1.0, 2.0, 3.0]).unwrap();
        assert_eq!(k.check_continuous(), Ok(()));
    }

    #[test]
    fn construction_checks() {
        let k = KnotVector::new(1, vec![0.0, 0.0, 1.0, 1.0]).unwrap();
        let pts = vec![[0.0, 0.0], [1.0, 1.0]];
        assert!(matches!(
            NurbsCurve::new(k.clone(), vec![[0.0, 0.0]], None),
            Err(NurbsError::PointCount {
                count: 1,
                expected: 2
            })
        ));
        assert!(matches!(
            NurbsCurve::new(k.clone(), vec![[0.0, f64::INFINITY], [1.0, 1.0]], None),
            Err(NurbsError::PointNotFinite { index: 0 })
        ));
        assert!(matches!(
            NurbsCurve::new(k.clone(), pts.clone(), Some(vec![1.0, 0.0])),
            Err(NurbsError::Weight { index: 1, .. })
        ));
        assert!(matches!(
            NurbsCurve::new(k.clone(), pts.clone(), Some(vec![1.0])),
            Err(NurbsError::WeightCount {
                count: 1,
                expected: 2
            })
        ));
        let c = NurbsCurve::new(k, pts, Some(vec![1.0, 3.0])).unwrap();
        assert_eq!(c.weighted_points().unwrap(), [[0.0, 0.0], [3.0, 3.0]]);
        // (3 u, 3 u) / (1 + 2 u) at u = 1/2: (3/4, 3/4).
        assert_eq!(c.point(0.5, Approach::Above).unwrap(), [0.75, 0.75]);
        assert!(matches!(
            c.split(1.0),
            Err(NurbsError::ParameterOutside { .. })
        ));
        assert!(matches!(
            c.derivs::<33>(0.5, Approach::Above),
            Err(NurbsError::DerivativeOrder { order: 32, .. })
        ));
        assert_eq!(
            c.to_string(),
            "rational curve of degree 1 in 2 dimensions, 2 control points on [0, 1]"
        );
    }

    #[test]
    fn equal_coordinates_stay_exact() {
        // A cubic in the plane y = 0.1: insertion and splitting keep y exactly.
        let k = KnotVector::new(3, vec![0.0, 0.0, 0.0, 0.0, 0.4, 1.0, 1.0, 1.0, 1.0]).unwrap();
        let pts = vec![[0.0, 0.1], [0.3, 0.1], [0.7, 0.1], [0.9, 0.1], [1.3, 0.1]];
        let c = NurbsCurve::new(k, pts, None).unwrap();
        let all = |d: &NurbsCurve<2>| d.points().iter().all(|p| p[1] == 0.1);
        assert!(all(&c.insert_knot(0.7, 2).unwrap()));
        let (l, r) = c.split(0.3).unwrap();
        assert!(all(&l) && all(&r));
    }
}
