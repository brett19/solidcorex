// Written for solidcorex. Apache-2.0.
//! Why a control net or a NURBS surface cannot be built.
//!
//! Errors follow research 62 §10: every cause has a kind computed from it ([`kind`]), a
//! stable [`name`], and a Display of one frame. All of this crate's causes are the caller's
//! data, kind `Refused` (63 X13); [`NurbsSurfaceError::BadKnots`] is a context frame over the
//! `scx-nurbs` error and inherits its kind (62 D11). The pin's `InvalidInput` is [`ErrorKind`]'s
//! `Refused`.
//!
//! | solidcore at the pin | here |
//! |---|---|
//! | `Surface::new`: `InvalidInput` (empty net, empty row, ragged rows, empty knots) | [`NetCause::Size`]; a ragged net cannot be built |
//! | `Surface::new`: `InvalidInput` (`MIXED_RATIONALITY`) | none: rationality is the type (`Option` of weights) |
//! | `counts_are_valid`: `SurfaceDefinition` (`n + p + 1 != r`, `m + q + 1 != s`) | [`NetCause::Count`] |
//! | `counts_are_valid`: `DegreeTooHigh` (above 17) | `scx_nurbs::NurbsError::Degree` (above 31) when the knot vector is built |
//! | `weights_are_valid`: `Weight` (outside `[1e-3, 100]`) | [`WeightCause::NotPositive`] (not positive and finite; any positive scale is a surface) |
//! | `KnotVector::validate`: `Knots`, `KnotsDecrease` | `scx_nurbs::NurbsError` when the knot vector is built; [`NurbsSurfaceError::BadKnots`] for a break |
//! | none: non-finite points and weights pass | [`NetCause::NotFinite`], [`NetCause::WeightedNotFinite`], [`WeightCause::NotPositive`] |
//!
//! [`kind`]: NurbsSurfaceError::kind
//! [`name`]: NurbsSurfaceError::name

use core::fmt;

use scx_num::error::ErrorKind;
use scx_nurbs::NurbsError;

use crate::ParamAxis;

/// Why a control net or a NURBS surface cannot be built.
#[derive(Debug, Clone, Copy, PartialEq)]
#[non_exhaustive]
pub enum NurbsSurfaceError {
    /// The control net does not fit the knots, or a point is not finite.
    BadNet(NetCause),
    /// The knots of one direction cannot carry a surface. A context frame: the cause is the
    /// `scx-nurbs` error, through [`std::error::Error::source`]. In this version it is
    /// [`NurbsError::Multiplicity`], an interior knot repeated `p + 1` times
    /// ([`KnotVector::check_continuous`](scx_nurbs::KnotVector::check_continuous)).
    BadKnots {
        /// The direction.
        axis: ParamAxis,
        /// Why.
        cause: NurbsError,
    },
    /// The weights do not fit the net, or one is not positive and finite.
    BadWeights(WeightCause),
}

/// Why a control net does not fit.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum NetCause {
    /// `len` values do not fill `rows` rows of `cols` (or a side is zero).
    Size {
        /// The values given.
        len: usize,
        /// The rows asked for.
        rows: usize,
        /// The columns asked for.
        cols: usize,
    },
    /// The net has `count` points along `axis` (rows for u, columns for v); the knots of
    /// that direction need `expected`.
    Count {
        /// The direction.
        axis: ParamAxis,
        /// The points the net has along it.
        count: usize,
        /// The points the knots need (`m - p`).
        expected: usize,
    },
    /// A coordinate of the control point `P[row][col]` is infinite or NaN (for a surface from
    /// homogeneous points: of `w P / w`).
    NotFinite {
        /// The row (u index).
        row: usize,
        /// The column (v index).
        col: usize,
    },
    /// A coordinate of the homogeneous point `w P` at `[row][col]` is infinite or NaN.
    WeightedNotFinite {
        /// The row (u index).
        row: usize,
        /// The column (v index).
        col: usize,
    },
}

/// Why the weights of a rational surface do not fit.
#[derive(Debug, Clone, Copy, PartialEq)]
#[non_exhaustive]
pub enum WeightCause {
    /// The weight net is `rows × cols`; the control net is `net_rows × net_cols`.
    Shape {
        /// The weight net's rows.
        rows: usize,
        /// The weight net's columns.
        cols: usize,
        /// The control net's rows.
        net_rows: usize,
        /// The control net's columns.
        net_cols: usize,
    },
    /// The weight at `[row][col]` is zero, negative, infinite or NaN.
    NotPositive {
        /// The row (u index).
        row: usize,
        /// The column (v index).
        col: usize,
        /// The weight.
        weight: f64,
    },
}

impl NurbsSurfaceError {
    /// The error's kind: [`ErrorKind::Refused`] (the net and weights are the caller's), or
    /// the `scx-nurbs` cause's under [`Self::BadKnots`] (62 D11).
    #[must_use]
    pub const fn kind(&self) -> ErrorKind {
        match self {
            Self::BadKnots { cause, .. } => cause.kind(),
            Self::BadNet(_) | Self::BadWeights(_) => ErrorKind::Refused,
        }
    }

    /// The cause's stable name (62 D6).
    #[must_use]
    pub const fn name(&self) -> &'static str {
        match self {
            Self::BadNet(c) => c.name(),
            Self::BadKnots { .. } => "knots_break_surface",
            Self::BadWeights(c) => c.name(),
        }
    }
}

impl NetCause {
    /// The cause's stable name.
    #[must_use]
    pub const fn name(&self) -> &'static str {
        match self {
            Self::Size { .. } => "net_size",
            Self::Count { .. } => "net_count",
            Self::NotFinite { .. } => "point_not_finite",
            Self::WeightedNotFinite { .. } => "weighted_point_not_finite",
        }
    }
}

impl WeightCause {
    /// The cause's stable name.
    #[must_use]
    pub const fn name(&self) -> &'static str {
        match self {
            Self::Shape { .. } => "weights_shape",
            Self::NotPositive { .. } => "weight_not_positive",
        }
    }
}

impl fmt::Display for NurbsSurfaceError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::BadNet(c) => c.fmt(f),
            Self::BadKnots { axis, .. } => write!(f, "the {axis} knots cannot carry a surface"),
            Self::BadWeights(c) => c.fmt(f),
        }
    }
}

impl fmt::Display for NetCause {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match *self {
            Self::Size { len, rows, cols } if rows == 0 || cols == 0 => {
                write!(f, "a net of {rows} × {cols} is empty ({len} values)")
            }
            Self::Size { len, rows, cols } => {
                write!(f, "{len} values do not fill a net of {rows} × {cols}")
            }
            Self::Count {
                axis,
                count,
                expected,
            } => write!(
                f,
                "the net has {count} control points along {axis}, the knots need {expected}"
            ),
            Self::NotFinite { row, col } => {
                write!(f, "control point [{row}][{col}] is not finite")
            }
            Self::WeightedNotFinite { row, col } => {
                write!(f, "homogeneous control point [{row}][{col}] is not finite")
            }
        }
    }
}

impl fmt::Display for WeightCause {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match *self {
            Self::Shape {
                rows,
                cols,
                net_rows,
                net_cols,
            } => write!(
                f,
                "{rows} × {cols} weights for a net of {net_rows} × {net_cols}"
            ),
            Self::NotPositive { row, col, weight } => {
                write!(
                    f,
                    "weight [{row}][{col}] = {weight} is not positive and finite"
                )
            }
        }
    }
}

impl std::error::Error for NurbsSurfaceError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::BadKnots { cause, .. } => Some(cause),
            Self::BadNet(_) | Self::BadWeights(_) => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;
    use std::error::Error as _;

    fn all() -> Vec<NurbsSurfaceError> {
        let at = (1, 2);
        vec![
            NurbsSurfaceError::BadNet(NetCause::Size {
                len: 5,
                rows: 2,
                cols: 3,
            }),
            NurbsSurfaceError::BadNet(NetCause::Count {
                axis: ParamAxis::V,
                count: 3,
                expected: 4,
            }),
            NurbsSurfaceError::BadNet(NetCause::NotFinite {
                row: at.0,
                col: at.1,
            }),
            NurbsSurfaceError::BadNet(NetCause::WeightedNotFinite {
                row: at.0,
                col: at.1,
            }),
            NurbsSurfaceError::BadKnots {
                axis: ParamAxis::U,
                cause: NurbsError::Multiplicity {
                    knot: 0.5,
                    multiplicity: 3,
                    max: 2,
                },
            },
            NurbsSurfaceError::BadWeights(WeightCause::Shape {
                rows: 2,
                cols: 2,
                net_rows: 2,
                net_cols: 3,
            }),
            NurbsSurfaceError::BadWeights(WeightCause::NotPositive {
                row: at.0,
                col: at.1,
                weight: -1.0,
            }),
        ]
    }

    #[test]
    fn names_are_unique_and_snake_case() {
        let names: Vec<&str> = all().iter().map(NurbsSurfaceError::name).collect();
        assert_eq!(names.iter().collect::<HashSet<_>>().len(), names.len());
        for n in names {
            assert!(
                n.bytes().all(|b| b.is_ascii_lowercase() || b == b'_'),
                "{n}"
            );
        }
    }

    /// One frame per Display: the knots' frame prints only its own text and hands the
    /// `scx-nurbs` error on as the source; the others have none.
    #[test]
    fn display_is_one_frame() {
        for e in all() {
            let text = e.to_string();
            assert!(!text.is_empty() && !text.contains('\n'), "{text}");
            assert_eq!(e.kind(), ErrorKind::Refused);
            match e {
                NurbsSurfaceError::BadKnots { cause, .. } => {
                    assert_eq!(text, "the u knots cannot carry a surface");
                    assert!(!text.contains(&cause.to_string()));
                    assert_eq!(e.source().map(ToString::to_string), Some(cause.to_string()));
                }
                _ => assert!(e.source().is_none()),
            }
        }
    }
}
