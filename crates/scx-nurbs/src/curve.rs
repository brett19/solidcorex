// Written for solidcorex. Apache-2.0.
//! NURBS curves as plain data: validated construction, evaluation with derivatives, knot
//! insertion, splitting and degree elevation.
//!
//! The algorithms and their arithmetic order follow solidcore at the pin:
//! - `nurbs_crv::crv_eval` (the point, A4.1);
//! - `nurbs_crv::crv_derivs` with `rational_basis_derivs_into` (derivatives, A3.2 and the
//!   rational basis functions' quotient rule);
//! - `crv_tool::p1::Curve::insert_knot` (A5.1) and `nurbs_crv::crv_split`;
//! - `crv_tool::p1::Curve::elevate_degree` (A5.9) with `bezier::elevate_degree`.
//!
//! Where the pin's knot insertion combines two control points with `CPoint::combine2`, this
//! uses the affine form (`CPoint::affine2`, [`Control::affine`]), which keeps a coordinate
//! equal in both points exactly, so insertion and splitting keep a plane exactly. Degree
//! elevation keeps the pin's `combine2`: its Bezier sums lose that exactness anyway, and its
//! knot removal amplifies rounding so much across near-coincident knots that any change of
//! arithmetic there changes the (equally inaccurate) answer.

use core::fmt;

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
    /// Accuracy: A5.9 removes the knots it inserted by extrapolating across neighbouring
    /// spans, so rounding error grows with the ratio of a span's neighbours to its length.
    /// Spans many orders of magnitude shorter than their neighbours (near-coincident knots)
    /// lose most or all accuracy, as at the pin; where a weight stops being positive or a
    /// point finite this errs with [`NurbsError::Unstable`] (the pin returns the curve).
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
                let (points, knots) = plan.points(&self.points).ok_or(MISSING_POINT)?;
                Self::new(KnotVector::new(p + times, knots)?, points, None).map_err(lost)
            }
            Some(r) => {
                let (points, knots) = plan.points(&r.homogeneous()).ok_or(MISSING_POINT)?;
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

/// The control points and knots of a degree elevation (A5.9).
pub(crate) struct Elevation<'a> {
    pub(crate) knots: &'a [f64],
    pub(crate) degree: usize,
    pub(crate) times: usize,
    /// The number of non-empty spans.
    pub(crate) spans: usize,
    pub(crate) matrix: &'a [Vec<f64>],
}

impl Elevation<'_> {
    #[allow(clippy::many_single_char_names, clippy::similar_names)] // A5.9's names
    pub(crate) fn points<P: Control>(&self, pw: &[P]) -> Option<(Vec<P>, Vec<f64>)> {
        let up = self.knots;
        let to = |x: usize| isize::try_from(x).ok();
        let (p, t) = (to(self.degree)?, to(self.times)?);
        let q = p + t;
        let mp = to(up.len())? - 1;
        let mut qw = vec![P::ZERO; pw.len() + self.times * self.spans];
        let mut uq = vec![0.0; up.len() + self.times * (self.spans + 1)];
        let mut bw = vec![P::ZERO; self.degree + 1];
        let mut nw = vec![P::ZERO; self.degree + 1];
        let mut dw = vec![P::ZERO; self.degree + self.times + 1];
        let mut alfs = vec![0.0; self.degree + 1];

        let mut a = p;
        let mut b = p + 1;
        let mut r: isize = -1;
        let mut cind: isize = 1;
        let mut kind = q + 1;
        for i in 0..=q {
            set(&mut uq, i, get(up, a)?)?;
        }
        set(&mut qw, 0, get(pw, 0)?)?;
        for i in 0..=p {
            set(&mut bw, i, get(pw, i)?)?;
        }

        while b < mp {
            // The multiplicity of the knot.
            let i0 = b;
            while b < mp && get(up, b)? == get(up, b + 1)? {
                b += 1;
            }
            let mlt = b - i0 + 1;
            let oldr = r;
            r = p - mlt;
            let lbz = if oldr > 0 { (oldr + 2) / 2 } else { 1 };
            let rbz = if r > 0 { q - (r + 1) / 2 } else { q };

            // Insert the knot to get the Bezier piece.
            if r > 0 {
                let num = get(up, b)? - get(up, a)?;
                let mut k = p;
                while k > mlt {
                    let al = num / (get(up, a + k)? - get(up, a)?);
                    set(&mut alfs, k - mlt - 1, al)?;
                    k -= 1;
                }
                for j in 1..=r {
                    let save = r - j;
                    let s = mlt + j;
                    let mut k = p;
                    while k >= s {
                        let c = {
                            let al = get(&alfs, k - s)?;
                            P::combine(al, get(&bw, k)?, 1.0 - al, get(&bw, k - 1)?)
                        };
                        set(&mut bw, k, c)?;
                        k -= 1;
                    }
                    set(&mut nw, save, get(&bw, p)?)?;
                }
            }

            // Elevate the Bezier piece.
            elevate_into(
                &bw,
                self.times,
                self.matrix,
                usize::try_from(lbz).ok()?,
                &mut dw,
            );

            // Remove the knot U[a].
            if oldr > 1 {
                let mut first = kind - 2;
                let den = get(up, b)? - get(up, a)?;
                let bet = (get(up, b)? - get(&uq, kind - 1)?) / den;
                for (tr, last) in (1..oldr).zip(kind..) {
                    let mut i = first;
                    let mut j = last;
                    let mut l = j - kind + 1;
                    while j - i > tr {
                        if i < cind {
                            let alf = (get(up, b)? - get(&uq, i)?) / (get(up, a)? - get(&uq, i)?);
                            let c = P::combine(alf, get(&qw, i)?, 1.0 - alf, get(&qw, i - 1)?);
                            set(&mut qw, i, c)?;
                        }
                        if j >= lbz {
                            let g = if j - tr <= kind - q + oldr {
                                (get(up, b)? - get(&uq, j - tr)?) / den
                            } else {
                                bet
                            };
                            let c = P::combine(g, get(&dw, l)?, 1.0 - g, get(&dw, l + 1)?);
                            set(&mut dw, l, c)?;
                        }
                        i += 1;
                        j -= 1;
                        l -= 1;
                    }
                    first -= 1;
                }
            }

            // Load the knots and control points.
            if a != p {
                for _ in 0..q - oldr {
                    set(&mut uq, kind, get(up, a)?)?;
                    kind += 1;
                }
            }
            for i in lbz..=rbz {
                set(&mut qw, cind, get(&dw, i)?)?;
                cind += 1;
            }

            // Set up the next piece.
            if b < mp {
                for i in 0..r {
                    set(&mut bw, i, get(&nw, i)?)?;
                }
                for i in r..=p {
                    set(&mut bw, i, get(pw, b - p + i)?)?;
                }
                a = b;
                b += 1;
            } else {
                for i in 0..=q {
                    set(&mut uq, kind + i, get(up, b)?)?;
                }
            }
        }
        Some((qw, uq))
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
            // Near-coincident knots: see `elevation_of_near_coincident_knots`.
            if well_spaced(&c) {
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
    }

    /// No span is shorter than 1e-6 of the domain.
    fn well_spaced<const D: usize>(c: &NurbsCurve<D>) -> bool {
        c.knots()
            .windows(2)
            .all(|w| w[1] == w[0] || w[1] - w[0] >= 1e-6)
    }

    /// A5.9 removes each knot after elevating the Bezier pieces by extrapolating across the
    /// neighbouring spans, which multiplies rounding error by the ratio of a span's
    /// neighbours to it: a span a few ulps long next to long spans loses everything (the
    /// pin's `elevate_degree` is the same algorithm). Kept as the truth check for the
    /// stable elevation this needs; see 57 §N.
    #[test]
    #[ignore = "known: A5.9 knot removal is ill-conditioned for near-coincident knots"]
    fn elevation_of_near_coincident_knots() {
        for data in curves(3).into_iter().step_by(2).filter(|d| !has_break(d)) {
            let c = ours(&data);
            if well_spaced(&c) {
                continue;
            }
            let exact = data.exact().unwrap();
            let e = c.elevate(2).unwrap();
            for v in [0.03, 0.31, 0.5, 0.77, 0.97] {
                let want = exact.point(v, tk::Side::Above).unwrap().rounded;
                assert!(
                    close(&e.point(v, Approach::Above).unwrap(), &want, 1e-12),
                    "{c} at {v}"
                );
            }
        }
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
