// Written for solidcorex. Apache-2.0.
//! [`CompatibleCurves::from_curves`]: curves of any knot vectors made compatible exactly.
//!
//! The pin's `make_compatible` moves knots of different curves that lie within a tolerance of
//! each other to their average, which changes the geometry by an amount nobody measures, and
//! it edits its inputs in place, leaving them half done on an error (77 §4.2). Here nothing
//! is moved and nothing is borrowed mutably:
//!
//! 1. every curve's parameter is mapped affinely onto the first curve's domain (a relabelling
//!    of the knots; the curve is the same set of points);
//! 2. every curve is elevated to the highest degree ([`NurbsCurve::elevate`]);
//! 3. the knot union is inserted into every curve (the largest multiplicity of each knot over
//!    the curves, after elevation), by Boehm's insertion, which is exact up to rounding;
//! 4. if any curve is rational, the polynomial ones get unit weights (every curve then shares
//!    one weight layout; a rational curve's weights are made compatible by the same
//!    insertions and elevations in homogeneous coordinates).
//!
//! Two knots a hair apart stay two knots. A case that cannot be done exactly is refused with
//! its cause: unclamped curves of different knots ([`NurbsError::Unclamped`]), a knot the
//! mapping rounds onto its neighbour ([`NurbsError::Multiplicity`]), an elevation past its
//! rounding bound ([`NurbsError::Unstable`]).

use std::borrow::Cow;

use crate::{CompatibleCurves, KnotVector, NurbsCurve, NurbsError, Result};

impl<const D: usize> CompatibleCurves<D> {
    /// The curves of `curves` made compatible: one knot vector (the knot union, at the
    /// highest degree) over the first curve's domain, curve `i` of the result the same
    /// geometry as `curves[i]` with its parameter mapped affinely onto that domain.
    ///
    /// The result is rational if any input is, with unit weights on the polynomial ones.
    /// Inputs that already share a knot vector come back unchanged, bit for bit.
    ///
    /// # Errors
    ///
    /// [`NurbsError::NoCurves`] for an empty slice; [`NurbsError::Unclamped`] when work is
    /// needed and a curve's end knots are not of multiplicity `degree + 1`;
    /// [`NurbsError::Unstable`] or [`NurbsError::Degree`] from the degree elevation; the
    /// knot vector's own errors if the mapping rounds two knots together.
    pub fn from_curves(curves: &[NurbsCurve<D>]) -> Result<Self> {
        let first = curves.first().ok_or(NurbsError::NoCurves)?;
        let (low, high) = (first.domain().min(), first.domain().max());
        let mut work: Vec<Cow<'_, NurbsCurve<D>>> = curves
            .iter()
            .map(|c| map_domain(c, low, high).map(Cow::from))
            .collect::<Result<_>>()?;
        let degree = work.iter().map(|c| c.degree()).max().unwrap_or(1);
        for c in &mut work {
            let times = degree - c.degree();
            if times > 0 {
                *c = Cow::Owned(c.elevate(times)?);
            }
        }
        let union = knot_union(&work);
        for c in &mut work {
            for &(knot, multiplicity) in &union {
                let have = c.knot_vector().multiplicity(knot);
                if have < multiplicity {
                    *c = Cow::Owned(c.insert_knot(knot, multiplicity - have)?);
                }
            }
        }
        assemble(&work)
    }
}

/// `curve` with its knots `a + (k - c) / (d - c) (b - a)` for its domain `[c, d]`, the end
/// knots exactly `a` and `b`; the curve itself when its domain is `[a, b]` already.
fn map_domain<const D: usize>(
    curve: &NurbsCurve<D>,
    a: f64,
    b: f64,
) -> Result<Cow<'_, NurbsCurve<D>>> {
    let (c, d) = (curve.domain().min(), curve.domain().max());
    if (c, d) == (a, b) {
        return Ok(Cow::Borrowed(curve));
    }
    let knots = curve
        .knots()
        .iter()
        .map(|&k| {
            if k == c {
                a
            } else if k == d {
                b
            } else if c < k && k < d {
                // Rounding must not carry an interior knot onto or past an end.
                (a + (k - c) / (d - c) * (b - a)).clamp(a, b)
            } else {
                a + (k - c) / (d - c) * (b - a)
            }
        })
        .collect();
    let knots = KnotVector::new(curve.degree(), knots)?;
    let weights = curve.weights().map(<[f64]>::to_vec);
    Ok(Cow::Owned(NurbsCurve::new(
        knots,
        curve.points().to_vec(),
        weights,
    )?))
}

/// The knots strictly inside the domain with the largest multiplicity any curve has, in
/// increasing order.
fn knot_union<const D: usize>(curves: &[Cow<'_, NurbsCurve<D>>]) -> Vec<(f64, usize)> {
    let mut all: Vec<(f64, usize)> = Vec::new();
    for c in curves {
        let (a, b) = (c.domain().min(), c.domain().max());
        all.extend(
            c.knot_vector()
                .breakpoints()
                .filter(|&(k, _)| a < k && k < b),
        );
    }
    all.sort_by(|x, y| x.0.total_cmp(&y.0).then(y.1.cmp(&x.1)));
    all.dedup_by(|later, kept| later.0 == kept.0);
    all
}

/// The curves, which must share one knot vector, as [`CompatibleCurves`].
fn assemble<const D: usize>(curves: &[Cow<'_, NurbsCurve<D>>]) -> Result<CompatibleCurves<D>> {
    let first = curves.first().ok_or(NurbsError::NoCurves)?;
    let knots = first.knot_vector();
    if curves.iter().any(|c| c.knot_vector() != knots) {
        // Unclamped curves with different knots: insertion cannot reach the ends.
        return if curves.iter().all(|c| c.knot_vector().is_clamped()) {
            Err(NurbsError::Defect {
                why: "compatible curves' knot vectors differ after the union",
            })
        } else {
            Err(NurbsError::Unclamped)
        };
    }
    let rational = curves.iter().any(|c| c.is_rational());
    let n = knots.point_count();
    let points: Vec<[f64; D]> = curves
        .iter()
        .flat_map(|c| c.points().iter().copied())
        .collect();
    let weights = rational.then(|| {
        curves
            .iter()
            .flat_map(|c| match c.weights() {
                Some(w) => w.to_vec(),
                None => vec![1.0; n],
            })
            .collect()
    });
    CompatibleCurves::new(knots.clone(), curves.len(), points, weights)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Approach;
    use scx_num::hash::Rng;
    use scx_testkit::sample::{self, Knots, Spec, Weights};

    /// A random curve of the family, its knots mapped onto `[lo, hi]`.
    fn source(
        rng: &mut Rng,
        degree: usize,
        knots: Knots,
        w: Weights,
        lo: f64,
        hi: f64,
    ) -> Option<NurbsCurve<3>> {
        let spec = Spec {
            degree,
            count: degree + 4,
            knots,
        };
        let d = sample::curve::<3>(rng, spec, w);
        let (a, b) = (d.knots[degree], d.knots[d.knots.len() - 1 - degree]);
        let ks = d
            .knots
            .iter()
            .map(|&k| lo + (k - a) / (b - a) * (hi - lo))
            .collect();
        let kv = KnotVector::new(degree, ks).ok()?;
        NurbsCurve::new(kv, d.points, d.weights).ok()
    }

    fn families() -> Vec<Vec<NurbsCurve<3>>> {
        let mut rng = Rng::new(77);
        let mut out = Vec::new();
        let kinds = [
            Knots::Uniform,
            Knots::Random,
            Knots::Clustered,
            Knots::Multiple,
        ];
        for (i, &k1) in kinds.iter().enumerate() {
            for (j, &k2) in kinds.iter().enumerate() {
                for (p1, p2) in [(1, 3), (2, 2), (3, 5), (4, 1)] {
                    let w = |n: usize| match (i + j + n) % 3 {
                        0 => Weights::Unit,
                        _ => Weights::Moderate,
                    };
                    let family: Option<Vec<_>> = [
                        source(&mut rng, p1, k1, w(0), 0.0, 1.0),
                        source(&mut rng, p2, k2, w(1), -2.0, 5.5),
                        source(&mut rng, p1, k2, w(2), 3.0, 4.0),
                    ]
                    .into_iter()
                    .collect();
                    out.extend(family);
                }
            }
        }
        out
    }

    #[test]
    fn evaluate_as_their_sources() {
        let mut totals = (0, 0, 0, 0);
        let all = families();
        assert!(all.len() > 30, "{}", all.len());
        for family in all {
            let Ok(cc) = CompatibleCurves::from_curves(&family) else {
                panic!("sampled family refused");
            };
            let top = family.iter().map(NurbsCurve::degree).max().unwrap();
            assert_eq!(cc.degree(), top);
            let (a, b) = (family[0].domain().min(), family[0].domain().max());
            for (i, src) in family.iter().enumerate() {
                let c = cc.curve(i).unwrap();
                assert_eq!(c.knots(), cc.knot_vector().knots());
                assert_eq!(c.degree(), top);
                let (sa, sb) = (src.domain().min(), src.domain().max());
                for j in 0..1000 {
                    let t = (f64::from(j) + 0.5) / 1000.0;
                    let want = src.point(sa + t * (sb - sa), Approach::Above).unwrap();
                    let got = c.point(a + t * (b - a), Approach::Above).unwrap();
                    for k in 0..3 {
                        let tol = 1e-9 * (1.0 + want[k].abs());
                        assert!((got[k] - want[k]).abs() <= tol, "{got:?} {want:?}");
                    }
                }
                totals.0 += src.knots().len();
                totals.1 += src.points().len();
            }
            totals.2 += cc.knot_vector().knots().len() * cc.count();
            totals.3 += cc.points().len();
        }
        // Work counts, in then out (knots and control points, summed over curves).
        eprintln!(
            "CN1 counts: knots in {} out {}; points in {} out {}",
            totals.0, totals.2, totals.1, totals.3
        );
    }

    #[test]
    fn same_knots_come_back_unchanged() {
        let mut rng = Rng::new(5);
        let c = source(&mut rng, 3, Knots::Random, Weights::Moderate, 0.0, 1.0).unwrap();
        let cc = CompatibleCurves::from_curves(&[c.clone(), c.clone()]).unwrap();
        assert_eq!(cc.curve(0).unwrap(), c);
        assert_eq!(cc.curve(1).unwrap(), c);
    }

    #[test]
    fn near_knots_stay_two_and_mixed_weights_become_unit() {
        let line = |inner: f64, weights| {
            let kv = KnotVector::new(1, vec![0.0, 0.0, inner, 1.0, 1.0]).unwrap();
            NurbsCurve::new(
                kv,
                vec![[0.0; 3], [1.0, 1.0, 0.0], [2.0, 0.0, 0.0]],
                weights,
            )
            .unwrap()
        };
        let a = line(0.5, None);
        let b = line(0.5 + 1e-12, Some(vec![1.0, 2.0, 1.0]));
        let cc = CompatibleCurves::from_curves(&[a.clone(), b]).unwrap();
        assert_eq!(cc.knot_vector().knots().len(), 6);
        assert!(cc.is_rational());
        let w = cc.weights().unwrap();
        assert!(
            w[..cc.knot_vector().point_count()]
                .iter()
                .all(|&x| x == 1.0)
        );
        // The input is untouched.
        assert_eq!(a, line(0.5, None));
    }

    #[test]
    fn refusals() {
        assert_eq!(
            CompatibleCurves::<3>::from_curves(&[]),
            Err(NurbsError::NoCurves)
        );
        let free = |inner: f64| {
            let kv = KnotVector::new(1, vec![0.0, 1.0, inner, 3.0]).unwrap();
            NurbsCurve::new(kv, vec![[0.0; 3], [1.0; 3]], None).unwrap()
        };
        assert_eq!(
            CompatibleCurves::from_curves(&[free(2.0), free(2.5)]),
            Err(NurbsError::Unclamped)
        );
    }
}
