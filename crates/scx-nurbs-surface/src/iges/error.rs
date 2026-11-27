// Written for solidcorex. Apache-2.0.
//! Why IGES entity 128 data cannot become a surface.
//!
//! Errors follow research 62 §10: a kind computed from the cause, a stable name, one frame per
//! Display. IGES data is the caller's, so every cause is [`ErrorKind::Refused`] except
//! [`IgesError::Defect`], an index past an array the checks before it rule out.
//!
//! | solidcore at the pin (`N_Iges128Srf`) | here |
//! |---|---|
//! | `NurbsFailure::Degree` (outside `1..=MAX_DEGREE`, 17) | [`IgesError::Degree`] (cap 31) |
//! | `InvalidInput` (an array shorter than the counts say) | none for the nets (a [`Net`](crate::Net) has its shape); [`KnotsCause::Count`] for the knots, which must match exactly |
//! | `NurbsFailure::Weight` (outside `[1e-3, 100]`) | [`IgesError::Surface`] over [`WeightCause::NotPositive`](crate::WeightCause) (positive and finite only) |
//! | `NurbsFailure::Knots` from `validate` | [`KnotsCause::Bounds`], [`KnotsCause::Decrease`] |
//! | none: NaN knots, bounds, points and weights pass | [`KnotsCause::NotFinite`], [`IgesError::Surface`] |
//! | `SolidError::Degenerate` (clamping leaves no control points) | [`IgesError::Degenerate`] |
//! | `InvalidInput` (an index outside an array) | [`IgesError::Defect`] |
//! | `Surface::new` failures after the repairs | [`IgesError::Knots`] over [`KnotsCause::Basis`], [`IgesError::Surface`] |

use core::fmt;

use scx_num::error::ErrorKind;
use scx_nurbs::NurbsError;

use crate::{NurbsSurfaceError, ParamAxis};

/// Why IGES entity 128 data cannot become a surface.
#[derive(Debug, Clone, Copy, PartialEq)]
#[non_exhaustive]
pub enum IgesError {
    /// The degree `M1` or `M2` is zero or above [`scx_nurbs::MAX_DEGREE`].
    Degree {
        /// The direction.
        axis: ParamAxis,
        /// The degree given.
        degree: usize,
    },
    /// The knots of a direction cannot carry the entity.
    Knots {
        /// The direction.
        axis: ParamAxis,
        /// Why.
        cause: KnotsCause,
    },
    /// Clamping to the parameter bounds leaves no control point along `axis`.
    Degenerate {
        /// The direction.
        axis: ParamAxis,
    },
    /// The points, weights or the surface they make are refused. A context frame: the cause
    /// is the [`NurbsSurfaceError`], through [`std::error::Error::source`].
    Surface(NurbsSurfaceError),
    /// An index outside an array, which the checks before it rule out: a defect.
    Defect {
        /// The direction.
        axis: ParamAxis,
    },
}

/// Why the knots of one direction cannot carry an entity.
#[derive(Debug, Clone, Copy, PartialEq)]
#[non_exhaustive]
pub enum KnotsCause {
    /// `len` knots; the entity's counts need `expected` (`K + M + 2`).
    Count {
        /// The knots given.
        len: usize,
        /// The knots the counts need.
        expected: usize,
    },
    /// The knot at `index` is infinite or NaN.
    NotFinite {
        /// Its index.
        index: usize,
    },
    /// The knots decrease from `index - 1` to `index`.
    Decrease {
        /// The index of the smaller knot.
        index: usize,
    },
    /// The parameter bounds `[low, high]` are not finite and increasing, or not inside
    /// `[T[M], T[K + 1]]` (the pin's `validate`).
    Bounds {
        /// The lower bound given.
        low: f64,
        /// The upper bound given.
        high: f64,
    },
    /// The repaired knots are not a basis: the `scx-nurbs` error, through
    /// [`std::error::Error::source`].
    Basis(NurbsError),
}

impl IgesError {
    /// The error's kind: [`ErrorKind::Refused`], [`ErrorKind::Defect`] for [`Self::Defect`],
    /// the [`NurbsSurfaceError`]'s under [`Self::Surface`].
    #[must_use]
    pub const fn kind(&self) -> ErrorKind {
        match self {
            Self::Surface(e) => e.kind(),
            Self::Defect { .. } => ErrorKind::Defect,
            Self::Degree { .. } | Self::Knots { .. } | Self::Degenerate { .. } => {
                ErrorKind::Refused
            }
        }
    }

    /// The cause's stable name (62 D6).
    #[must_use]
    pub const fn name(&self) -> &'static str {
        match self {
            Self::Degree { .. } => "iges_degree",
            Self::Knots { cause, .. } => cause.name(),
            Self::Degenerate { .. } => "iges_no_control_points",
            Self::Surface(_) => "iges_surface_refused",
            Self::Defect { .. } => "iges_index_defect",
        }
    }
}

impl KnotsCause {
    /// The cause's stable name.
    #[must_use]
    pub const fn name(&self) -> &'static str {
        match self {
            Self::Count { .. } => "iges_knot_count",
            Self::NotFinite { .. } => "iges_knot_not_finite",
            Self::Decrease { .. } => "iges_knots_decrease",
            Self::Bounds { .. } => "iges_bounds",
            Self::Basis(_) => "iges_knots_not_a_basis",
        }
    }
}

impl From<NurbsSurfaceError> for IgesError {
    fn from(e: NurbsSurfaceError) -> Self {
        Self::Surface(e)
    }
}

impl fmt::Display for IgesError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Degree { axis, degree } => write!(f, "the {axis} degree {degree} is not allowed"),
            Self::Knots { axis, cause } => write!(f, "the {axis} knots: {cause}"),
            Self::Degenerate { axis } => {
                write!(f, "clamping to the bounds leaves no {axis} control points")
            }
            Self::Surface(_) => f.write_str("the control net and weights make no surface"),
            Self::Defect { axis } => write!(f, "an {axis} index is outside its array"),
        }
    }
}

impl fmt::Display for KnotsCause {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match *self {
            Self::Count { len, expected } => write!(f, "{len} knots, the counts need {expected}"),
            Self::NotFinite { index } => write!(f, "knot {index} is not finite"),
            Self::Decrease { index } => write!(f, "knot {index} is below knot {}", index - 1),
            Self::Bounds { low, high } => {
                write!(f, "the bounds [{low}, {high}] are not inside the domain")
            }
            Self::Basis(_) => f.write_str("the repaired knots are not a basis"),
        }
    }
}

impl std::error::Error for IgesError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Surface(e) => Some(e),
            Self::Knots {
                cause: KnotsCause::Basis(e),
                ..
            } => Some(e),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{NetCause, WeightCause};
    use std::collections::HashSet;
    use std::error::Error as _;

    fn all() -> Vec<IgesError> {
        let k = |cause| IgesError::Knots {
            axis: ParamAxis::V,
            cause,
        };
        vec![
            IgesError::Degree {
                axis: ParamAxis::U,
                degree: 0,
            },
            k(KnotsCause::Count {
                len: 3,
                expected: 8,
            }),
            k(KnotsCause::NotFinite { index: 2 }),
            k(KnotsCause::Decrease { index: 3 }),
            k(KnotsCause::Bounds {
                low: 0.0,
                high: 9.0,
            }),
            k(KnotsCause::Basis(NurbsError::EmptyDomain { at: 1.0 })),
            IgesError::Degenerate { axis: ParamAxis::U },
            IgesError::Surface(NurbsSurfaceError::BadNet(NetCause::NotFinite {
                row: 0,
                col: 1,
            })),
            IgesError::Surface(NurbsSurfaceError::BadWeights(WeightCause::NotPositive {
                row: 0,
                col: 1,
                weight: 0.0,
            })),
            IgesError::Defect { axis: ParamAxis::V },
        ]
    }

    #[test]
    fn names_are_snake_case_and_one_frame() {
        let mut seen = HashSet::new();
        for e in all() {
            let n = e.name();
            assert!(
                n.bytes().all(|b| b.is_ascii_lowercase() || b == b'_'),
                "{n}"
            );
            seen.insert(n);
            let text = e.to_string();
            assert!(!text.is_empty() && !text.contains('\n'), "{text}");
            let want = if matches!(e, IgesError::Defect { .. }) {
                ErrorKind::Defect
            } else {
                ErrorKind::Refused
            };
            assert_eq!(e.kind(), want);
            if let Some(src) = e.source() {
                assert!(!text.contains(&src.to_string()), "{text}");
            }
        }
        // Two Surface causes share a name; the rest are distinct.
        assert_eq!(seen.len(), all().len() - 1);
    }
}
