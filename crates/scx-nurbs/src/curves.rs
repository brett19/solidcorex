// Written for solidcorex. Apache-2.0.
//! Compatible curves: NURBS curves of one knot vector, stored together and edited together.
//!
//! The lines of a NURBS surface's net along a direction are such curves, and so are the
//! sections a surface is skinned through (Piegl and Tiller §10.3). An edit of all of them
//! does its knot work once (the checks, the new knot vector, the elevation matrix) and then
//! runs the same per-polygon arithmetic as [`NurbsCurve`]'s edit, so curve `i` of the result
//! is bit for bit the edit of curve `i`.

use crate::bezier::elevation_matrix;
use crate::curve::{
    Elevation, Insertion, MISSING_POINT, check_finite, check_weights, split_weights,
};
use crate::point::{Control, Weighted};
use crate::{Approach, KnotVector, MAX_DEGREE, NurbsCurve, NurbsError, Result};

/// `count` NURBS curves on one [`KnotVector`] (compatible curves): with
/// `n = knots.point_count()`, curve `i` has the control points `i n .. (i + 1) n` of
/// [`points`](Self::points), and for rational curves the weights at the same indices.
///
/// Built only by [`CompatibleCurves::new`] or [`CompatibleCurves::from_homogeneous`], which
/// check what [`NurbsCurve::new`] and [`NurbsCurve::from_homogeneous`] check of each curve.
/// Errors about a point or weight give its index within its curve.
#[derive(Debug, Clone, PartialEq)]
pub struct CompatibleCurves<const D: usize> {
    knots: KnotVector,
    count: usize,
    points: Vec<[f64; D]>,
    /// The weights and the homogeneous points `w P`.
    rational: Option<(Vec<f64>, Vec<[f64; D]>)>,
}

/// The weights of rational [`CompatibleCurves`] and their homogeneous points `w P`, one curve
/// after another ([`CompatibleCurves::into_parts`]).
#[derive(Debug, Clone, PartialEq)]
pub struct Homogeneous<const D: usize> {
    /// The weights `w`.
    pub weights: Vec<f64>,
    /// The homogeneous points `w P`.
    pub weighted: Vec<[f64; D]>,
}

impl<const D: usize> CompatibleCurves<D> {
    /// `count` curves on `knots` with the control points `points`, one curve after another;
    /// rational with `weights`.
    pub fn new(
        knots: KnotVector,
        count: usize,
        points: Vec<[f64; D]>,
        weights: Option<Vec<f64>>,
    ) -> Result<Self> {
        knots.check_continuous()?;
        let n = knots.point_count();
        check_count(points.len(), count, n, |count, expected| {
            NurbsError::PointCount { count, expected }
        })?;
        let rational = match weights {
            None => {
                each_curve(&points, n, check_finite)?;
                None
            }
            Some(weights) => {
                check_count(weights.len(), count, n, |count, expected| {
                    NurbsError::WeightCount { count, expected }
                })?;
                let weighted: Vec<[f64; D]> = points
                    .iter()
                    .zip(&weights)
                    .map(|(p, &w)| p.map(|x| x * w))
                    .collect();
                for ((p, w), wp) in lines(&points, n)
                    .zip(lines(&weights, n))
                    .zip(lines(&weighted, n))
                {
                    check_finite(p)?;
                    check_weights(w, n)?;
                    check_finite(wp)?;
                }
                Some((weights, weighted))
            }
        };
        Ok(Self {
            knots,
            count,
            points,
            rational,
        })
    }

    /// `count` rational curves on `knots` with the homogeneous control points
    /// `(weighted[k], weights[k])`, one curve after another: `P[k] = weighted[k] / weights[k]`.
    pub fn from_homogeneous(
        knots: KnotVector,
        count: usize,
        weighted: Vec<[f64; D]>,
        weights: Vec<f64>,
    ) -> Result<Self> {
        knots.check_continuous()?;
        let n = knots.point_count();
        check_count(weighted.len(), count, n, |count, expected| {
            NurbsError::PointCount { count, expected }
        })?;
        check_count(weights.len(), count, n, |count, expected| {
            NurbsError::WeightCount { count, expected }
        })?;
        let points: Vec<[f64; D]> = weighted
            .iter()
            .zip(&weights)
            .map(|(p, &w)| p.map(|x| x / w))
            .collect();
        for ((wp, w), p) in lines(&weighted, n)
            .zip(lines(&weights, n))
            .zip(lines(&points, n))
        {
            check_finite(wp)?;
            check_weights(w, n)?;
            check_finite(p)?;
        }
        Ok(Self {
            knots,
            count,
            points,
            rational: Some((weights, weighted)),
        })
    }

    /// The knot vector.
    #[must_use]
    pub const fn knot_vector(&self) -> &KnotVector {
        &self.knots
    }

    /// The degree `p`.
    #[must_use]
    pub const fn degree(&self) -> usize {
        self.knots.degree()
    }

    /// The number of curves.
    #[must_use]
    pub const fn count(&self) -> usize {
        self.count
    }

    /// Whether the curves have weights.
    #[must_use]
    pub const fn is_rational(&self) -> bool {
        self.rational.is_some()
    }

    /// The control points of every curve, one curve after another.
    #[must_use]
    pub fn points(&self) -> &[[f64; D]] {
        &self.points
    }

    /// The weights of rational curves, one curve after another.
    #[must_use]
    pub fn weights(&self) -> Option<&[f64]> {
        self.rational.as_ref().map(|(w, _)| w.as_slice())
    }

    /// The homogeneous points `w P` of rational curves, one curve after another.
    #[must_use]
    pub fn weighted_points(&self) -> Option<&[[f64; D]]> {
        self.rational.as_ref().map(|(_, wp)| wp.as_slice())
    }

    /// Curve `i`, exactly as stored.
    #[must_use]
    pub fn curve(&self, i: usize) -> Option<NurbsCurve<D>> {
        let n = self.knots.point_count();
        let range = i.checked_mul(n)?..i.checked_add(1)?.checked_mul(n)?;
        let points = self.points.get(range.clone())?.to_vec();
        let rational = match &self.rational {
            None => None,
            Some((w, wp)) => Some((w.get(range.clone())?.to_vec(), wp.get(range)?.to_vec())),
        };
        Some(NurbsCurve::from_valid_parts(
            self.knots.clone(),
            points,
            rational,
        ))
    }

    /// The knot vector, the control points, and for rational curves the weights and the
    /// homogeneous points.
    #[must_use]
    pub fn into_parts(self) -> (KnotVector, Vec<[f64; D]>, Option<Homogeneous<D>>) {
        let rational = self
            .rational
            .map(|(weights, weighted)| Homogeneous { weights, weighted });
        (self.knots, self.points, rational)
    }

    /// Every curve with the knot `u` inserted `times` times: [`NurbsCurve::insert_knot`] of
    /// each, with the same conditions and errors.
    pub fn insert_knot(&self, u: f64, times: usize) -> Result<Self> {
        let multiplicity = self.knots.check_insertion(u, times)?;
        if times == 0 {
            return Ok(self.clone());
        }
        let knots = self.knots.inserted(u, times)?;
        let plan = Insertion {
            knots: self.knots.knots(),
            degree: self.degree(),
            span: self.knots.span_of(u, Approach::Above),
            multiplicity,
            times,
            u,
        };
        let n = self.knots.point_count();
        match &self.rational {
            None => {
                let points = insert_each(&plan, &self.points, n)?;
                Self::new(knots, self.count, points, None)
            }
            Some((weights, weighted)) => {
                let points = insert_each(&plan, &homogeneous(weighted, weights), n)?;
                let (weighted, weights) = split_weights(points);
                Self::from_homogeneous(knots, self.count, weighted, weights)
            }
        }
    }

    /// The pieces of every curve below and above `u`: [`NurbsCurve::split`] of each, with
    /// the same conditions and errors.
    pub fn split(&self, u: f64) -> Result<(Self, Self)> {
        self.knots.check_inside(u)?;
        let p = self.degree();
        let missing = p.saturating_sub(self.knots.multiplicity(u));
        let full = self.insert_knot(u, missing)?;
        let knots = full.knots.knots();
        let first = knots.partition_point(|&k| k < u);
        let after = knots.partition_point(|&k| k <= u);
        let ends = core::iter::repeat_n(u, p + 1);
        let (below, _) = knots.split_at(first);
        let (_, above) = knots.split_at(after);
        let left_knots = KnotVector::new(p, below.iter().copied().chain(ends.clone()).collect())?;
        let right_knots = KnotVector::new(p, ends.chain(above.iter().copied()).collect())?;
        let n = full.knots.point_count();
        let left_count = left_knots.point_count();
        let right_from = n - right_knots.point_count();
        let piece = |knots: KnotVector, range: core::ops::Range<usize>| match &full.rational {
            None => {
                let points = slice_each(&full.points, n, range)?;
                Self::new(knots, self.count, points, None)
            }
            Some((weights, weighted)) => {
                let weighted = slice_each(weighted, n, range.clone())?;
                let weights = slice_each(weights, n, range)?;
                Self::from_homogeneous(knots, self.count, weighted, weights)
            }
        };
        Ok((
            piece(left_knots, 0..left_count)?,
            piece(right_knots, right_from..n)?,
        ))
    }

    /// Every curve with its degree raised by `times`: [`NurbsCurve::elevate`] of each, with
    /// the same conditions, errors and accuracy.
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
        let n = self.knots.point_count();
        match &self.rational {
            None => {
                let (points, knots) = elevate_each(&plan, &self.points, n)?;
                Self::new(KnotVector::new(p + times, knots)?, self.count, points, None)
                    .map_err(lost)
            }
            Some((weights, weighted)) => {
                let (points, knots) = elevate_each(&plan, &homogeneous(weighted, weights), n)?;
                let (weighted, weights) = split_weights(points);
                let knots = KnotVector::new(p + times, knots)?;
                Self::from_homogeneous(knots, self.count, weighted, weights).map_err(lost)
            }
        }
    }
}

/// `len == count * n`, else the error `count_error(len, count * n)`.
fn check_count(
    len: usize,
    count: usize,
    n: usize,
    count_error: impl FnOnce(usize, usize) -> NurbsError,
) -> Result<()> {
    let expected = count.saturating_mul(n);
    if len == expected {
        Ok(())
    } else {
        Err(count_error(len, expected))
    }
}

/// The curves' runs of `n` values (`n >= 1` for every knot vector).
fn lines<T>(values: &[T], n: usize) -> core::slice::ChunksExact<'_, T> {
    values.chunks_exact(n.max(1))
}

/// `check` of each curve's points, in order.
fn each_curve<const D: usize>(
    points: &[[f64; D]],
    n: usize,
    check: impl Fn(&[[f64; D]]) -> Result<()>,
) -> Result<()> {
    lines(points, n).try_for_each(check)
}

/// The homogeneous points `(w P, w)`.
fn homogeneous<const D: usize>(weighted: &[[f64; D]], weights: &[f64]) -> Vec<Weighted<D>> {
    weighted
        .iter()
        .zip(weights)
        .map(|(&point, &weight)| Weighted { point, weight })
        .collect()
}

/// The insertion into each curve's polygon, the results one after another.
fn insert_each<P: Control>(plan: &Insertion<'_>, polygons: &[P], n: usize) -> Result<Vec<P>> {
    let m = n + plan.times;
    let mut out = vec![P::ZERO; polygons.len() / n.max(1) * m];
    let mut scratch = Vec::with_capacity(plan.degree + 1);
    for (polygon, edited) in lines(polygons, n).zip(out.chunks_exact_mut(m)) {
        plan.points_into(polygon, edited, &mut scratch)
            .ok_or(MISSING_POINT)?;
    }
    Ok(out)
}

/// The elevation of each curve's polygon, one after another, and the elevated knots (the
/// same for every polygon: A5.9's knots depend on the knots alone).
fn elevate_each<P: Control>(
    plan: &Elevation<'_>,
    polygons: &[P],
    n: usize,
) -> Result<(Vec<P>, Vec<f64>)> {
    let mut out = Vec::new();
    let mut knots = None;
    for polygon in lines(polygons, n) {
        let (points, k) = plan.points(polygon)?;
        out.extend(points);
        knots.get_or_insert(k);
    }
    let knots = match knots {
        Some(k) => k,
        // No curves: the knots of the elevation of any polygon.
        None => plan.points(&vec![P::ZERO; n])?.1,
    };
    Ok((out, knots))
}

/// The values `range` of each curve's run of `n`, one after another.
fn slice_each<T: Copy>(values: &[T], n: usize, range: core::ops::Range<usize>) -> Result<Vec<T>> {
    let mut out = Vec::with_capacity(values.len() / n.max(1) * range.len());
    for line in lines(values, n) {
        out.extend_from_slice(line.get(range.clone()).ok_or(MISSING_POINT)?);
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use scx_num::hash::Rng;
    use scx_testkit::sample::{self, Knots, Spec, Weights};

    /// Families of `count` random curves on one knot vector, every knot and weight kind.
    fn families(seed: u64) -> Vec<CompatibleCurves<3>> {
        let mut rng = Rng::new(seed);
        let mut out = Vec::new();
        for degree in [1, 2, 3, 5, 9] {
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
                    let first = sample::curve(&mut rng, spec, weights);
                    let Ok(kv) = KnotVector::new(degree, first.knots.clone()) else {
                        continue;
                    };
                    if kv.check_continuous().is_err() {
                        continue;
                    }
                    let mut points = first.points.clone();
                    let mut ws = first.weights.clone();
                    for _ in 0..3 {
                        let more = sample::curve(&mut rng, spec, weights);
                        points.extend(more.points);
                        if let (Some(w), Some(m)) = (ws.as_mut(), more.weights) {
                            w.extend(m);
                        }
                    }
                    out.extend(CompatibleCurves::new(kv, 4, points, ws));
                }
            }
        }
        // Unclamped, polynomial and rational.
        let kv = KnotVector::new(2, vec![0.0, 0.5, 1.0, 1.5, 2.0, 2.5, 3.0]).unwrap();
        let points: Vec<[f64; 3]> = (0..8).map(|i| [f64::from(i), 1.0, -0.5]).collect();
        out.extend(CompatibleCurves::new(kv.clone(), 2, points.clone(), None));
        out.extend(CompatibleCurves::new(kv, 2, points, Some(vec![1.5; 8])));
        out
    }

    /// Curve `i` of `edited` is `edit` of curve `i` of `family`, bit for bit, and the errors
    /// agree.
    fn agrees(
        family: &CompatibleCurves<3>,
        edited: Result<CompatibleCurves<3>>,
        edit: impl Fn(&NurbsCurve<3>) -> Result<NurbsCurve<3>>,
    ) {
        let each: Vec<_> = (0..family.count())
            .map(|i| edit(&family.curve(i).unwrap()))
            .collect();
        match edited {
            Ok(e) => {
                for (i, c) in each.into_iter().enumerate() {
                    assert_eq!(e.curve(i), Some(c.unwrap()), "curve {i}");
                }
            }
            Err(err) => {
                let first = each.into_iter().find_map(core::result::Result::err);
                assert_eq!(first, Some(err));
            }
        }
    }

    #[test]
    fn edits_are_the_edits_of_each_curve() {
        let families = families(41);
        assert!(families.len() > 40);
        for f in &families {
            let (a, b) = f.knot_vector().ends();
            let first_inner = f
                .knot_vector()
                .knots()
                .iter()
                .copied()
                .find(|&k| a < k && k < b);
            let mut params = vec![a + 0.37 * (b - a), a, b];
            params.extend(first_inner);
            params.extend(first_inner.map(|k| k + 1e-9));
            for &u in &params {
                for times in 0..4 {
                    agrees(f, f.insert_knot(u, times), |c| c.insert_knot(u, times));
                }
                let pieces = f.split(u);
                agrees(f, pieces.clone().map(|p| p.0), |c| c.split(u).map(|p| p.0));
                agrees(f, pieces.map(|p| p.1), |c| c.split(u).map(|p| p.1));
            }
            for times in 0..4 {
                agrees(f, f.elevate(times), |c| c.elevate(times));
            }
        }
    }

    #[test]
    fn construction_checks() {
        let kv = KnotVector::new(1, vec![0.0, 0.0, 1.0, 1.0]).unwrap();
        let pts = vec![[0.0; 3], [1.0; 3], [2.0; 3], [3.0; 3]];
        let two = CompatibleCurves::new(kv.clone(), 2, pts.clone(), None).unwrap();
        assert_eq!(two.count(), 2);
        assert_eq!(two.curve(1).unwrap().points(), &pts[2..]);
        assert_eq!(two.curve(2), None);
        assert_eq!(
            CompatibleCurves::new(kv.clone(), 3, pts.clone(), None),
            Err(NurbsError::PointCount {
                count: 4,
                expected: 6
            })
        );
        let mut bad = pts.clone();
        bad[3] = [f64::NAN; 3];
        assert_eq!(
            CompatibleCurves::new(kv.clone(), 2, bad, None),
            Err(NurbsError::PointNotFinite { index: 1 })
        );
        assert_eq!(
            CompatibleCurves::new(kv.clone(), 2, pts.clone(), Some(vec![1.0, 1.0, 1.0, -1.0])),
            Err(NurbsError::Weight {
                index: 1,
                weight: -1.0
            })
        );
        assert_eq!(
            CompatibleCurves::from_homogeneous(kv.clone(), 2, pts.clone(), vec![1.0; 3]),
            Err(NurbsError::WeightCount {
                count: 3,
                expected: 4
            })
        );
        let none = CompatibleCurves::<3>::new(kv, 0, Vec::new(), None).unwrap();
        assert_eq!(none.elevate(2).unwrap().degree(), 3);
        assert_eq!(none.split(0.5).unwrap().0.count(), 0);
    }
}
