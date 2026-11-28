// Written for solidcorex. Apache-2.0.
//! Why a fit cannot be made.
//!
//! One error type for the module, a typed cause per variant (01 §5.1). Every cause is the
//! caller's data or request, except [`FitError::Defect`]; none is a retry or a looser answer
//! (R2), and the pin's `NL_*_ERR` numbers have no counterpart.
//!
//! | solidcore at the pin | here |
//! |---|---|
//! | `InvalidInput` for no or one point, a degree too high for the points | [`FitError::TooFewPoints`], [`FitError::TooFewControlPoints`] |
//! | interior parameters left `0.0` when the chord total is at most `PTOL` | [`FitError::ZeroLength`] |
//! | a decomposition failing on a pivot below `LU_TOL` | [`FitError::Singular`] |
//! | parameters or knots out of order, caught (or not) by the decomposition | [`FitError::ParametersNotIncreasing`], [`FitError::EndsNotOnDomain`] |
//! | `fill_end_derivs` estimating a missing end derivative from the data | none: every end condition is stated by the caller |

use core::fmt;

use scx_num::error::ErrorKind;

use crate::NurbsError;

/// Why a fit cannot be made.
#[derive(Debug, Clone, Copy, PartialEq)]
#[non_exhaustive]
pub enum FitError {
    /// Fewer data points than the request needs.
    TooFewPoints {
        /// The points given.
        count: usize,
        /// The fewest allowed.
        needed: usize,
    },
    /// A data point, an end derivative or a weight is infinite or NaN.
    NotFinite {
        /// Which datum: the point's index, or `usize::MAX` for an end derivative.
        index: usize,
    },
    /// The chord (or centripetal) parameterisation of points that all coincide.
    ZeroLength,
    /// The number of parameters differs from the number of points.
    ParameterCount {
        /// The parameters given.
        count: usize,
        /// The points given.
        points: usize,
    },
    /// The number of weights differs from the number of points.
    WeightCount {
        /// The weights given.
        count: usize,
        /// The points given.
        points: usize,
    },
    /// A weight is not positive and finite.
    Weight {
        /// The point it belongs to.
        index: usize,
        /// Its value.
        weight: f64,
    },
    /// The parameters do not increase strictly from `index - 1` to `index`.
    ParametersNotIncreasing {
        /// The index of the later parameter.
        index: usize,
    },
    /// The first and last parameter are not the two ends of the knot vector's domain.
    EndsNotOnDomain,
    /// The knots are not clamped, so the curve does not start and end at its end control points.
    Unclamped,
    /// An end condition of derivative order `order` needs a degree above it (knot vectors by
    /// averaging) or at least equal to it (given knots).
    DegreeTooLow {
        /// The degree.
        degree: usize,
        /// The highest derivative order asked at one end.
        order: usize,
    },
    /// The number of knots does not give the number of control points the data and the end
    /// conditions determine.
    KnotCount {
        /// The control points the knots give.
        points: usize,
        /// The control points the data determine.
        expected: usize,
    },
    /// The end conditions fix more control points than the knots give.
    TooFewControlPoints {
        /// The control points the knots give.
        count: usize,
        /// The fewest that hold both ends' conditions and one more.
        needed: usize,
    },
    /// The system has no unique solution: column `column` has no pivot above rounding. The
    /// parameters and knots break the Schoenberg-Whitney condition (a span without data, or
    /// fewer data than control points), or a derivative the degree cannot express.
    Singular {
        /// The unknown with no pivot.
        column: usize,
    },
    /// A knot vector, basis or curve step failed.
    Nurbs(NurbsError),
    /// A step failed through the module's own fault: an index its invariants rule out was
    /// missing. A bug.
    Defect {
        /// The step that failed.
        why: &'static str,
    },
}

impl FitError {
    /// What the caller can do about the error (research 62 §1.1).
    #[must_use]
    pub const fn kind(&self) -> ErrorKind {
        match self {
            Self::Nurbs(e) => e.kind(),
            Self::Defect { .. } => ErrorKind::Defect,
            Self::TooFewPoints { .. }
            | Self::NotFinite { .. }
            | Self::ZeroLength
            | Self::ParameterCount { .. }
            | Self::WeightCount { .. }
            | Self::Weight { .. }
            | Self::ParametersNotIncreasing { .. }
            | Self::EndsNotOnDomain
            | Self::Unclamped
            | Self::DegreeTooLow { .. }
            | Self::KnotCount { .. }
            | Self::TooFewControlPoints { .. }
            | Self::Singular { .. } => ErrorKind::Refused,
        }
    }

    /// The cause's stable name, unique in the module (research 62 D6).
    #[must_use]
    pub const fn name(&self) -> &'static str {
        match self {
            Self::TooFewPoints { .. } => "too_few_points",
            Self::NotFinite { .. } => "not_finite",
            Self::ZeroLength => "zero_length",
            Self::ParameterCount { .. } => "parameter_count",
            Self::WeightCount { .. } => "weight_count",
            Self::Weight { .. } => "weight",
            Self::ParametersNotIncreasing { .. } => "parameters_not_increasing",
            Self::EndsNotOnDomain => "ends_not_on_domain",
            Self::Unclamped => "unclamped",
            Self::DegreeTooLow { .. } => "degree_too_low",
            Self::KnotCount { .. } => "knot_count",
            Self::TooFewControlPoints { .. } => "too_few_control_points",
            Self::Singular { .. } => "singular",
            Self::Nurbs(_) => "nurbs",
            Self::Defect { .. } => "defect",
        }
    }
}

impl From<NurbsError> for FitError {
    fn from(e: NurbsError) -> Self {
        Self::Nurbs(e)
    }
}

impl fmt::Display for FitError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match *self {
            Self::TooFewPoints { count, needed } => {
                write!(f, "{count} data points, at least {needed} needed")
            }
            Self::NotFinite { index } => write!(f, "datum {index} is not finite"),
            Self::ZeroLength => f.write_str("the data points all coincide"),
            Self::ParameterCount { count, points } => {
                write!(f, "{count} parameters for {points} points")
            }
            Self::WeightCount { count, points } => write!(f, "{count} weights for {points} points"),
            Self::Weight { index, weight } => {
                write!(f, "weight {index} is {weight}, not positive and finite")
            }
            Self::ParametersNotIncreasing { index } => {
                write!(f, "parameter {index} is not above the one before it")
            }
            Self::EndsNotOnDomain => {
                f.write_str("the first and last parameter are not the ends of the domain")
            }
            Self::Unclamped => f.write_str("the end knots are not of multiplicity degree + 1"),
            Self::DegreeTooLow { degree, order } => {
                write!(
                    f,
                    "degree {degree} cannot take a derivative of order {order} at an end"
                )
            }
            Self::KnotCount { points, expected } => {
                write!(
                    f,
                    "the knots give {points} control points, the data determine {expected}"
                )
            }
            Self::TooFewControlPoints { count, needed } => {
                write!(
                    f,
                    "{count} control points, the end conditions need {needed}"
                )
            }
            Self::Singular { column } => {
                write!(
                    f,
                    "no unique fit: the system has no pivot for unknown {column}"
                )
            }
            Self::Nurbs(e) => write!(f, "{e}"),
            Self::Defect { why } => f.write_str(why),
        }
    }
}

impl std::error::Error for FitError {}
