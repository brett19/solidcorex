// Written for solidcorex. Apache-2.0.
//! Why a knot vector, a basis or a curve cannot be built or evaluated.
//!
//! One error type for the crate, with a typed cause per variant (01 §5.1). Every check runs
//! once, where a value is built: a [`KnotVector`](crate::KnotVector) or a
//! [`NurbsCurve`](crate::NurbsCurve) that exists is valid, so evaluation fails only for a
//! parameter or an order the caller passes, or for arithmetic that cannot go on.
//!
//! | solidcore at the pin | here |
//! |---|---|
//! | `NurbsFailure::Knots`, `KnotsDecrease { index }` | [`NurbsError::KnotsDecrease`], [`NurbsError::Multiplicity`], [`NurbsError::TooFewKnots`], [`NurbsError::EmptyDomain`] |
//! | `NurbsFailure::Numerical`, `SolidError::Degenerate` (a knot difference below `1e-24`) | [`NurbsError::Degenerate`] (a difference that is exactly zero or not finite) |
//! | `NurbsFailure::Parameter`, `ParameterOutside` | [`NurbsError::ParameterOutside`], [`NurbsError::ParameterNotFinite`] |
//! | `NurbsFailure::DerivativeOrder { given, max }` | [`NurbsError::DerivativeOrder`] |
//! | `NurbsFailure::Degree`, `InvalidInput` (`p >= MAX_DEGREE`) | [`NurbsError::Degree`] |
//! | `NurbsFailure::Index`, `ControlPointsShort`, `InvalidInput` (out of bounds) | none: a built curve has the right counts ([`NurbsError::PointCount`] at construction) |
//! | none: `elevate_degree` returns zero or negative weights | [`NurbsError::Unstable`] |
//! | `SolidError::Unsupported` at a domain end (`basis_eval`, `crv_eval`) | none: the ends evaluate |
//! | none: an index past a slice the built values' counts rule out | [`NurbsError::Defect`] |
//!
//! The kinds (research 62 §1.1): a degree above [`MAX_DEGREE`](crate::MAX_DEGREE) and a
//! derivative order above [`MAX_ORDER`](crate::MAX_ORDER) are fixed caps, so
//! [`ErrorKind::Exhausted`] (62 M1); [`NurbsError::Unstable`] is valid input the elevation
//! cannot finish, [`ErrorKind::Unsupported`]; the rest refuse the caller's input, including
//! [`NurbsError::Degenerate`], whose degenerate span or parameter is the caller's (62 §3.1, D2).

use core::fmt;

use scx_num::error::ErrorKind;

/// Why a knot vector, a basis or a curve cannot be built or evaluated.
#[derive(Debug, Clone, Copy, PartialEq)]
#[non_exhaustive]
pub enum NurbsError {
    /// The degree is zero or above [`MAX_DEGREE`](crate::MAX_DEGREE).
    Degree {
        /// The degree given.
        degree: usize,
        /// The highest degree supported.
        max: usize,
    },
    /// Fewer knots than a basis of this degree needs (`2 (degree + 1)`).
    TooFewKnots {
        /// The knots given.
        count: usize,
        /// The fewest allowed.
        needed: usize,
    },
    /// A knot is infinite or NaN.
    KnotNotFinite {
        /// Its index.
        index: usize,
    },
    /// The knots decrease from `index - 1` to `index`.
    KnotsDecrease {
        /// The index of the smaller knot.
        index: usize,
    },
    /// A knot value is repeated more than `degree + 1` times.
    Multiplicity {
        /// The repeated value.
        knot: f64,
        /// How many times it appears.
        multiplicity: usize,
        /// The most allowed.
        max: usize,
    },
    /// The domain `[U[p], U[m-p]]` is a single value.
    EmptyDomain {
        /// That value.
        at: f64,
    },
    /// The control point count does not match the knots (`knots - degree - 1`).
    PointCount {
        /// The points given.
        count: usize,
        /// The count the knots need.
        expected: usize,
    },
    /// The weight count is not the point count.
    WeightCount {
        /// The weights given.
        count: usize,
        /// The point count.
        expected: usize,
    },
    /// A control point coordinate is infinite or NaN.
    PointNotFinite {
        /// The control point's index.
        index: usize,
    },
    /// A weight is not a positive finite number.
    Weight {
        /// The control point's index.
        index: usize,
        /// The weight given.
        weight: f64,
    },
    /// A parameter is infinite or NaN.
    ParameterNotFinite {
        /// The parameter given.
        parameter: f64,
    },
    /// A parameter that must lie inside the domain does not.
    ParameterOutside {
        /// The parameter given.
        parameter: f64,
        /// The open interval it must lie in, `[low, high]`.
        domain: [f64; 2],
    },
    /// Inserting a knot would raise its multiplicity above the degree.
    InsertionMultiplicity {
        /// The knot to insert.
        knot: f64,
        /// Its multiplicity now.
        multiplicity: usize,
        /// The insertions asked for.
        times: usize,
        /// The degree, the highest multiplicity allowed.
        degree: usize,
    },
    /// A knot removal asks for more copies than the knot has.
    RemovalMultiplicity {
        /// The knot to remove.
        knot: f64,
        /// Its multiplicity.
        multiplicity: usize,
        /// The removals asked for.
        times: usize,
    },
    /// A derivative order above [`MAX_ORDER`](crate::MAX_ORDER).
    DerivativeOrder {
        /// The order given.
        order: usize,
        /// The highest order supported.
        max: usize,
    },
    /// Degree elevation needs end knots of multiplicity `degree + 1`.
    Unclamped,
    /// A knot difference the basis divides by rounded to zero or overflowed (a span far
    /// shorter than the parameter's magnitude, or a parameter far outside the domain).
    Degenerate {
        /// The parameter being evaluated.
        parameter: f64,
    },
    /// Degree elevation lost the curve: removing the knots it inserted extrapolates across
    /// neighbouring spans, and across near-coincident knots the rounding grew until a weight
    /// was no longer positive or a point no longer finite. (Where it stays finite the answer
    /// may still be inaccurate: see [`NurbsCurve::elevate`](crate::NurbsCurve::elevate).)
    Unstable,
    /// A step failed through the crate's own fault: an index the built values' invariants
    /// rule out was missing. A bug.
    Defect {
        /// The step that failed.
        why: &'static str,
    },
}

impl NurbsError {
    /// What the caller can do about the error (research 62 §1.1).
    #[must_use]
    pub const fn kind(&self) -> ErrorKind {
        match self {
            // valid input the elevation cannot finish
            Self::Unstable => ErrorKind::Unsupported,
            // fixed caps on the input (research 62 M1); a degree of zero is refused
            Self::Degree { degree, .. } if *degree > 0 => ErrorKind::Exhausted,
            Self::DerivativeOrder { .. } => ErrorKind::Exhausted,
            Self::Defect { .. } => ErrorKind::Defect,
            Self::Degree { .. }
            | Self::TooFewKnots { .. }
            | Self::KnotNotFinite { .. }
            | Self::KnotsDecrease { .. }
            | Self::Multiplicity { .. }
            | Self::EmptyDomain { .. }
            | Self::PointCount { .. }
            | Self::WeightCount { .. }
            | Self::PointNotFinite { .. }
            | Self::Weight { .. }
            | Self::ParameterNotFinite { .. }
            | Self::ParameterOutside { .. }
            | Self::InsertionMultiplicity { .. }
            | Self::RemovalMultiplicity { .. }
            | Self::Unclamped
            | Self::Degenerate { .. } => ErrorKind::Refused,
        }
    }

    /// The cause's stable name, unique in the crate (research 62 D6).
    #[must_use]
    pub const fn name(&self) -> &'static str {
        match self {
            Self::Degree { .. } => "degree",
            Self::TooFewKnots { .. } => "too_few_knots",
            Self::KnotNotFinite { .. } => "knot_not_finite",
            Self::KnotsDecrease { .. } => "knots_decrease",
            Self::Multiplicity { .. } => "multiplicity",
            Self::EmptyDomain { .. } => "empty_domain",
            Self::PointCount { .. } => "point_count",
            Self::WeightCount { .. } => "weight_count",
            Self::PointNotFinite { .. } => "point_not_finite",
            Self::Weight { .. } => "weight",
            Self::ParameterNotFinite { .. } => "parameter_not_finite",
            Self::ParameterOutside { .. } => "parameter_outside",
            Self::InsertionMultiplicity { .. } => "insertion_multiplicity",
            Self::RemovalMultiplicity { .. } => "removal_multiplicity",
            Self::DerivativeOrder { .. } => "derivative_order",
            Self::Unclamped => "unclamped",
            Self::Degenerate { .. } => "degenerate",
            Self::Unstable => "unstable",
            Self::Defect { .. } => "defect",
        }
    }
}

impl fmt::Display for NurbsError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match *self {
            Self::Degree { degree, max } => {
                write!(f, "degree {degree} is not in 1..={max}")
            }
            Self::TooFewKnots { count, needed } => {
                write!(f, "{count} knots, at least {needed} needed")
            }
            Self::KnotNotFinite { index } => write!(f, "knot {index} is not finite"),
            Self::KnotsDecrease { index } => {
                write!(f, "the knots decrease at index {index}")
            }
            Self::Multiplicity {
                knot,
                multiplicity,
                max,
            } => write!(
                f,
                "knot {knot} is repeated {multiplicity} times, at most {max} allowed"
            ),
            Self::EmptyDomain { at } => write!(f, "the domain is the single value {at}"),
            Self::PointCount { count, expected } => {
                write!(f, "{count} control points, the knots need {expected}")
            }
            Self::WeightCount { count, expected } => {
                write!(f, "{count} weights for {expected} control points")
            }
            Self::PointNotFinite { index } => {
                write!(f, "control point {index} is not finite")
            }
            Self::Weight { index, weight } => {
                write!(f, "weight {index} is {weight}, not positive and finite")
            }
            Self::ParameterNotFinite { parameter } => {
                write!(f, "parameter {parameter} is not finite")
            }
            Self::ParameterOutside { parameter, domain } => write!(
                f,
                "parameter {parameter} is not inside the domain ({}, {})",
                domain[0], domain[1]
            ),
            Self::InsertionMultiplicity {
                knot,
                multiplicity,
                times,
                degree,
            } => write!(
                f,
                "inserting knot {knot} (multiplicity {multiplicity}) {times} times exceeds the degree {degree}"
            ),
            Self::RemovalMultiplicity {
                knot,
                multiplicity,
                times,
            } => write!(
                f,
                "cannot remove knot {knot} {times} times: its multiplicity is {multiplicity}"
            ),
            Self::DerivativeOrder { order, max } => {
                write!(f, "derivative order {order} is above {max}")
            }
            Self::Unclamped => {
                f.write_str("the end knots are not of multiplicity degree + 1")
            }
            Self::Degenerate { parameter } => write!(
                f,
                "a knot difference rounded to zero or the result overflowed evaluating at {parameter}"
            ),
            Self::Unstable => {
                f.write_str("degree elevation lost the curve across near-coincident knots")
            }
            Self::Defect { why } => f.write_str(why),
        }
    }
}

impl std::error::Error for NurbsError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kinds_and_texts() {
        let e = NurbsError::Degenerate { parameter: 0.5 };
        assert_eq!(e.kind(), ErrorKind::Refused);
        assert_eq!(
            e.to_string(),
            "a knot difference rounded to zero or the result overflowed evaluating at 0.5"
        );
        assert_eq!(NurbsError::Unstable.kind(), ErrorKind::Unsupported);
        let e = NurbsError::ParameterOutside {
            parameter: 2.0,
            domain: [0.0, 1.0],
        };
        assert_eq!(e.kind(), ErrorKind::Refused);
        assert_eq!(e.to_string(), "parameter 2 is not inside the domain (0, 1)");
        let cap = |degree| NurbsError::Degree { degree, max: 3 }.kind();
        assert_eq!((cap(0), cap(4)), (ErrorKind::Refused, ErrorKind::Exhausted));
        let order = NurbsError::DerivativeOrder { order: 9, max: 8 };
        assert_eq!(order.kind(), ErrorKind::Exhausted);
        assert_eq!(NurbsError::Defect { why: "" }.kind(), ErrorKind::Defect);
    }

    /// Every cause's name is unique in the crate (research 62 D6).
    #[test]
    fn cause_names_are_unique() {
        let all = [
            NurbsError::Degree { degree: 0, max: 0 },
            NurbsError::TooFewKnots {
                count: 0,
                needed: 0,
            },
            NurbsError::KnotNotFinite { index: 0 },
            NurbsError::KnotsDecrease { index: 0 },
            NurbsError::Multiplicity {
                knot: 0.0,
                multiplicity: 0,
                max: 0,
            },
            NurbsError::EmptyDomain { at: 0.0 },
            NurbsError::PointCount {
                count: 0,
                expected: 0,
            },
            NurbsError::WeightCount {
                count: 0,
                expected: 0,
            },
            NurbsError::PointNotFinite { index: 0 },
            NurbsError::Weight {
                index: 0,
                weight: 0.0,
            },
            NurbsError::ParameterNotFinite { parameter: 0.0 },
            NurbsError::ParameterOutside {
                parameter: 0.0,
                domain: [0.0; 2],
            },
            NurbsError::InsertionMultiplicity {
                knot: 0.0,
                multiplicity: 0,
                times: 0,
                degree: 0,
            },
            NurbsError::RemovalMultiplicity {
                knot: 0.0,
                multiplicity: 0,
                times: 0,
            },
            NurbsError::DerivativeOrder { order: 0, max: 0 },
            NurbsError::Unclamped,
            NurbsError::Degenerate { parameter: 0.0 },
            NurbsError::Unstable,
            NurbsError::Defect { why: "" },
        ];
        let names: Vec<&str> = all.iter().map(NurbsError::name).collect();
        let mut sorted = names.clone();
        sorted.sort_unstable();
        sorted.dedup();
        assert_eq!(sorted.len(), names.len(), "{names:?}");
    }
}
