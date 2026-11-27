// Written for solidcorex. Apache-2.0.
//! Knot vectors: validated construction, multiplicity, span finding, insertion and removal.
//!
//! Span finding answers the cases of solidcore's `knots::find_span` (A2.1) at the pin, but
//! exactly: the pin moves a parameter at the domain ends inwards past knots closer than an
//! absolute `1e-12`, which picks a span of the wrong piece for short spans and for knot
//! values far from 1. Here a binary search (`partition_point`) gives the span the
//! definition names.

use core::fmt;

use scx_num::extent1d::Extent1d;

use crate::{MAX_DEGREE, NurbsError, Result};

/// Which of the two pieces meeting at a knot a parameter on it belongs to.
///
/// Away from knots both sides give the same span. At a knot `u`, [`Approach::Above`] takes the
/// span `[U[i], U[i+1])` that starts at `u`, and [`Approach::Below`] the span `(U[i], U[i+1]]`
/// that ends there. At a knot of full multiplicity the two sides give the two one-sided
/// limits. At the domain ends both sides take the end span.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Approach {
    /// The span that ends at the parameter: the limit from below.
    Below,
    /// The span that starts at the parameter: the limit from above.
    Above,
}

impl fmt::Display for Approach {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Below => "below",
            Self::Above => "above",
        })
    }
}

/// A non-decreasing knot vector `U[0..=m]` of a degree `p`, with its domain
/// `[U[p], U[m-p]]`.
///
/// Built only by [`KnotVector::new`], which checks that:
/// - `1 <= p <= MAX_DEGREE`;
/// - there are at least `2 (p + 1)` knots, so at least `p + 1` basis functions;
/// - the knots are finite and non-decreasing;
/// - no value repeats more than `p + 1` times;
/// - the domain is not a single value.
///
/// The end knots need not be repeated ([`KnotVector::is_clamped`]); outside the domain each
/// end piece continues as its polynomial.
#[derive(Debug, Clone, PartialEq)]
pub struct KnotVector {
    degree: usize,
    knots: Vec<f64>,
    /// `[U[p], U[m-p]]`.
    domain: Extent1d,
}

impl KnotVector {
    /// Validates `knots` for a basis of `degree`.
    pub fn new(degree: usize, knots: Vec<f64>) -> Result<Self> {
        if degree == 0 || degree > MAX_DEGREE {
            return Err(NurbsError::Degree {
                degree,
                max: MAX_DEGREE,
            });
        }
        let needed = 2 * (degree + 1);
        if knots.len() < needed {
            return Err(NurbsError::TooFewKnots {
                count: knots.len(),
                needed,
            });
        }
        if let Some(index) = knots.iter().position(|k| !k.is_finite()) {
            return Err(NurbsError::KnotNotFinite { index });
        }
        if let Some(index) = knots
            .iter()
            .zip(knots.iter().skip(1))
            .position(|(a, b)| b < a)
        {
            return Err(NurbsError::KnotsDecrease { index: index + 1 });
        }
        check_multiplicity(&knots, degree + 1)?;
        let (Some(&low), Some(&high)) = (knots.get(degree), knots.iter().rev().nth(degree)) else {
            // Unreachable: there are at least 2 (degree + 1) knots.
            return Err(NurbsError::TooFewKnots {
                count: knots.len(),
                needed,
            });
        };
        if low == high {
            return Err(NurbsError::EmptyDomain { at: low });
        }
        let Some(domain) = Extent1d::new(low, high) else {
            // Unreachable: the knots are finite and non-decreasing.
            return Err(NurbsError::EmptyDomain { at: low });
        };
        Ok(Self {
            degree,
            knots,
            domain,
        })
    }

    /// The degree `p`.
    #[must_use]
    pub const fn degree(&self) -> usize {
        self.degree
    }

    /// The knots `U[0..=m]`.
    #[must_use]
    pub fn knots(&self) -> &[f64] {
        &self.knots
    }

    /// The knots, by value.
    #[must_use]
    pub fn into_knots(self) -> Vec<f64> {
        self.knots
    }

    /// The number of basis functions, which is the number of control points of a curve
    /// on this knot vector: `m - p`.
    #[must_use]
    pub fn point_count(&self) -> usize {
        self.knots.len() - self.degree - 1
    }

    /// The domain `[U[p], U[m-p]]`.
    #[must_use]
    pub const fn domain(&self) -> Extent1d {
        self.domain
    }

    /// The domain's ends, `(U[p], U[m-p])`.
    #[must_use]
    pub const fn ends(&self) -> (f64, f64) {
        (self.domain.min(), self.domain.max())
    }

    /// Whether the first and the last `p + 1` knots are equal, so a curve on these knots
    /// starts and ends at its first and last control points.
    #[must_use]
    pub fn is_clamped(&self) -> bool {
        let p1 = self.degree + 1;
        self.knots.iter().take(p1).all(|&k| k == self.domain.min())
            && self
                .knots
                .iter()
                .rev()
                .take(p1)
                .all(|&k| k == self.domain.max())
    }

    /// How many knots equal `u` exactly.
    #[must_use]
    pub fn multiplicity(&self, u: f64) -> usize {
        self.knots.partition_point(|&k| k <= u) - self.knots.partition_point(|&k| k < u)
    }

    /// The number of non-empty spans in the domain (the Bezier pieces of a curve).
    #[must_use]
    pub fn span_count(&self) -> usize {
        let inner = self
            .knots
            .iter()
            .skip(self.degree)
            .take(self.knots.len() - 2 * self.degree);
        inner
            .clone()
            .zip(inner.skip(1))
            .filter(|(a, b)| b > a)
            .count()
    }

    /// The distinct knot values in `[U[p], U[m-p]]` with their multiplicities in the whole
    /// vector, in increasing order: the breakpoints of a curve.
    pub fn breakpoints(&self) -> impl Iterator<Item = (f64, usize)> + '_ {
        let (low, high) = (self.domain.min(), self.domain.max());
        let mut rest = self.knots.as_slice();
        core::iter::from_fn(move || {
            let (&first, _) = rest.split_first()?;
            let count = rest.partition_point(|&k| k <= first);
            let (_, tail) = rest.split_at(count);
            rest = tail;
            Some((first, count))
        })
        .filter(move |&(k, _)| low <= k && k <= high)
    }

    /// Checks that no knot strictly inside the domain repeats more than `p` times, so a curve
    /// on these knots is continuous (C0) there: a knot of multiplicity `p + 1` breaks the
    /// curve in two, which is a split, not a curve (04, 11-27 scx-nurbs ruling 2). This knot
    /// vector itself allows `p + 1` everywhere.
    ///
    /// Errs with [`NurbsError::Multiplicity`] (`max` the degree) at the first such knot.
    pub fn check_continuous(&self) -> Result<()> {
        let (low, high) = (self.domain.min(), self.domain.max());
        match self
            .breakpoints()
            .find(|&(k, mult)| low < k && k < high && mult > self.degree)
        {
            Some((knot, multiplicity)) => Err(NurbsError::Multiplicity {
                knot,
                multiplicity,
                max: self.degree,
            }),
            None => Ok(()),
        }
    }

    /// The index `i` of the knot span `[U[i], U[i+1]]` that `u` lies in, with `p <= i < m - p`
    /// and `U[i] < U[i+1]`.
    ///
    /// At a knot, `side` picks the span ([`Approach`]). At or past an end of the domain the end
    /// span is returned, so a parameter outside the domain evaluates the end piece.
    pub fn span(&self, u: f64, side: Approach) -> Result<usize> {
        if !u.is_finite() {
            return Err(NurbsError::ParameterNotFinite { parameter: u });
        }
        Ok(self.span_of(u, side))
    }

    /// [`KnotVector::span`] for a finite `u`.
    pub(crate) fn span_of(&self, u: f64, side: Approach) -> usize {
        let (low, high) = (self.domain.min(), self.domain.max());
        // Each count below is at least p + 1 (U[p] = low <= the bound) and at most m - p
        // (U[m-p] = high > the bound), so the span is in p..m-p.
        let count = if u <= low && (side == Approach::Below || u < low) {
            // The first non-empty span: the last knot equal to the low end.
            self.knots.partition_point(|&k| k <= low)
        } else if u >= high && (side == Approach::Above || u > high) {
            // The last non-empty span: the last knot below the high end.
            self.knots.partition_point(|&k| k < high)
        } else {
            match side {
                Approach::Above => self.knots.partition_point(|&k| k <= u),
                Approach::Below => self.knots.partition_point(|&k| k < u),
            }
        };
        count - 1
    }

    /// The knots `U[span+1-p..=span]` and `U[span+1..=span+p]` the basis at `span` reads:
    /// `p` each.
    pub(crate) fn window(&self, span: usize) -> Option<(&[f64], &[f64])> {
        let p = self.degree;
        let below = self.knots.get(span + 1 - p..=span)?;
        let above = self.knots.get(span + 1..=span + p)?;
        Some((below, above))
    }

    /// This knot vector with `u` inserted `times` times.
    ///
    /// `u` must lie inside the domain, and its multiplicity after the insertion must not
    /// exceed the degree (the curve stays continuous there).
    pub fn inserted(&self, u: f64, times: usize) -> Result<Self> {
        self.check_insertion(u, times)?;
        let at = self.knots.partition_point(|&k| k <= u);
        let (head, tail) = self.knots.split_at(at);
        let knots = head
            .iter()
            .copied()
            .chain(core::iter::repeat_n(u, times))
            .chain(tail.iter().copied())
            .collect();
        Ok(Self {
            degree: self.degree,
            knots,
            domain: self.domain,
        })
    }

    /// The multiplicity of `u`, after checking that `u` may be inserted `times` times.
    pub(crate) fn check_insertion(&self, u: f64, times: usize) -> Result<usize> {
        self.check_inside(u)?;
        let multiplicity = self.multiplicity(u);
        if multiplicity.saturating_add(times) > self.degree {
            return Err(NurbsError::InsertionMultiplicity {
                knot: u,
                multiplicity,
                times,
                degree: self.degree,
            });
        }
        Ok(multiplicity)
    }

    /// Checks that `u` is finite and strictly inside the domain.
    pub(crate) fn check_inside(&self, u: f64) -> Result<()> {
        if !u.is_finite() {
            return Err(NurbsError::ParameterNotFinite { parameter: u });
        }
        if u <= self.domain.min() || u >= self.domain.max() {
            return Err(NurbsError::ParameterOutside {
                parameter: u,
                domain: [self.domain.min(), self.domain.max()],
            });
        }
        Ok(())
    }

    /// This knot vector with `times` copies of the interior knot `u` removed.
    ///
    /// This changes the space of splines: a curve keeps its shape through a knot removal
    /// only when it is smooth enough there, which needs the control points as well (fitting,
    /// 01 §10.2 step 5).
    pub fn removed(&self, u: f64, times: usize) -> Result<Self> {
        self.check_inside(u)?;
        let multiplicity = self.multiplicity(u);
        if times > multiplicity {
            return Err(NurbsError::RemovalMultiplicity {
                knot: u,
                multiplicity,
                times,
            });
        }
        let at = self.knots.partition_point(|&k| k < u);
        let (head, tail) = self.knots.split_at(at);
        let knots = head
            .iter()
            .chain(tail.iter().skip(times))
            .copied()
            .collect();
        Self::new(self.degree, knots)
    }

    /// The knots mirrored about zero, `U'[i] = -U[m - i]`: the knot vector of a curve run
    /// the other way with the parameter `-t` ([`NurbsCurve::reversed`](crate::NurbsCurve::reversed)).
    /// Negation does not round, so this is exact and valid whenever `self` is; the domain is
    /// [`Extent1d::negated`].
    #[must_use]
    pub fn negated(&self) -> Self {
        Self {
            degree: self.degree,
            knots: self.knots.iter().rev().map(|&k| -k).collect(),
            domain: self.domain.negated(),
        }
    }
}

/// Checks that no value of the sorted `knots` repeats more than `max` times.
fn check_multiplicity(knots: &[f64], max: usize) -> Result<()> {
    let mut rest = knots;
    while let Some((&knot, _)) = rest.split_first() {
        let multiplicity = rest.partition_point(|&k| k <= knot);
        if multiplicity > max {
            return Err(NurbsError::Multiplicity {
                knot,
                multiplicity,
                max,
            });
        }
        let (_, tail) = rest.split_at(multiplicity);
        rest = tail;
    }
    Ok(())
}

impl fmt::Display for KnotVector {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "degree {} knots [", self.degree)?;
        for (i, k) in self.knots.iter().enumerate() {
            if i > 0 {
                f.write_str(", ")?;
            }
            write!(f, "{k}")?;
        }
        f.write_str("]")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kv(p: usize, k: &[f64]) -> KnotVector {
        KnotVector::new(p, k.to_vec()).unwrap()
    }

    #[test]
    fn validation() {
        assert!(matches!(
            KnotVector::new(0, vec![0.0, 1.0]),
            Err(NurbsError::Degree { degree: 0, .. })
        ));
        assert!(matches!(
            KnotVector::new(2, vec![0.0, 0.0, 0.0, 1.0, 1.0]),
            Err(NurbsError::TooFewKnots {
                count: 5,
                needed: 6
            })
        ));
        assert!(matches!(
            KnotVector::new(1, vec![0.0, 0.0, 2.0, 1.0]),
            Err(NurbsError::KnotsDecrease { index: 3 })
        ));
        assert!(matches!(
            KnotVector::new(1, vec![0.0, f64::NAN, 1.0, 1.0]),
            Err(NurbsError::KnotNotFinite { index: 1 })
        ));
        assert!(matches!(
            KnotVector::new(1, vec![0.0, 0.0, 0.0, 1.0]),
            Err(NurbsError::Multiplicity {
                multiplicity: 3,
                max: 2,
                ..
            })
        ));
        assert!(matches!(
            KnotVector::new(1, vec![0.0, 1.0, 1.0, 2.0]),
            Err(NurbsError::EmptyDomain { at: 1.0 })
        ));
        // Interior multiplicity p + 1 is a discontinuity, and allowed.
        let k = kv(2, &[0.0, 0.0, 0.0, 0.5, 0.5, 0.5, 1.0, 1.0, 1.0]);
        assert_eq!(k.point_count(), 6);
        assert!(k.is_clamped());
        assert_eq!(k.span_count(), 2);
        assert!(!kv(2, &[0.0, 1.0, 2.0, 3.0, 4.0, 5.0]).is_clamped());
    }

    #[test]
    fn spans_at_knots_and_ends() {
        let k = kv(2, &[0.0, 0.0, 0.0, 1.0, 2.0, 2.0, 3.0, 3.0, 3.0]);
        let span = |u, s| k.span(u, s).unwrap();
        assert_eq!(span(0.5, Approach::Above), 2);
        assert_eq!(span(0.5, Approach::Below), 2);
        assert_eq!(span(1.0, Approach::Above), 3);
        assert_eq!(span(1.0, Approach::Below), 2);
        assert_eq!(span(2.0, Approach::Above), 5);
        assert_eq!(span(2.0, Approach::Below), 3);
        for s in [Approach::Above, Approach::Below] {
            assert_eq!(span(0.0, s), 2);
            assert_eq!(span(-1.0, s), 2);
            assert_eq!(span(3.0, s), 5);
            assert_eq!(span(9.0, s), 5);
        }
        assert!(k.span(f64::NAN, Approach::Above).is_err());
        assert_eq!(k.multiplicity(2.0), 2);
        assert_eq!(k.multiplicity(2.5), 0);
        assert_eq!(
            k.breakpoints().collect::<Vec<_>>(),
            [(0.0, 3), (1.0, 1), (2.0, 2), (3.0, 3)]
        );
    }

    #[test]
    fn spans_of_tiny_end_spans() {
        // The pin's absolute 1e-12 moves u = 1 into the span below a last span 1e-13 long.
        let k = kv(1, &[0.0, 0.0, 1.0 - 1e-13, 1.0, 1.0]);
        assert_eq!(k.span(1.0, Approach::Above).unwrap(), 2);
        assert_eq!(k.span(1.0, Approach::Below).unwrap(), 2);
    }

    #[test]
    fn insertion_and_removal() {
        let k = kv(2, &[0.0, 0.0, 0.0, 1.0, 2.0, 2.0, 2.0]);
        let i = k.inserted(1.0, 1).unwrap();
        assert_eq!(i.knots(), [0.0, 0.0, 0.0, 1.0, 1.0, 2.0, 2.0, 2.0]);
        assert!(matches!(
            i.inserted(1.0, 1),
            Err(NurbsError::InsertionMultiplicity {
                multiplicity: 2,
                ..
            })
        ));
        assert!(matches!(
            k.inserted(2.0, 1),
            Err(NurbsError::ParameterOutside { .. })
        ));
        assert_eq!(
            i.removed(1.0, 2).unwrap().knots(),
            [0.0, 0.0, 0.0, 2.0, 2.0, 2.0]
        );
        assert!(matches!(
            k.removed(0.5, 1),
            Err(NurbsError::RemovalMultiplicity {
                multiplicity: 0,
                ..
            })
        ));
        assert_eq!(k.to_string(), "degree 2 knots [0, 0, 0, 1, 2, 2, 2]");
    }
}
