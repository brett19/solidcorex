// Written for solidcorex. Apache-2.0.
//! Why a control net or a NURBS surface cannot be built or evaluated, or an edit cannot be
//! made.
//!
//! Errors follow research 62 §10: every cause has a kind computed from it ([`kind`]), a
//! stable [`name`], and a Display of one frame. The construction causes are the caller's
//! data, kind `Refused` (63 X13); [`NurbsSurfaceError::BadKnots`], [`NurbsSurfaceError::Basis`]
//! and [`NurbsSurfaceError::Edit`] are context frames over the `scx-nurbs` error and inherit
//! its kind (62 D11). An edit whose exact answer has no double form is `Unsupported`
//! ([`NurbsSurfaceError::Overflow`]). The pin's
//! `InvalidInput` is [`ErrorKind`]'s `Refused`.
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
//! | `Surface::eval`, `Surface::derivs`: the errors of `KnotVector::basis_eval` (a parameter outside the domain, a knot difference below `1e-24`) | [`NurbsSurfaceError::Basis`] over `NurbsError::ParameterNotFinite`, `Degenerate` or `DerivativeOrder`; outside the domain the end pieces continue |
//! | none: a non-finite point is returned | [`NurbsSurfaceError::Degenerate`] |
//! | `nurbs_partials_over_box`: `Unsupported` (a box past the knots, a crossed knot below the continuity) | [`NurbsSurfaceError::Enclosure`] over [`EnclosureCause::BoxOutside`], [`EnclosureCause::BelowContinuity`]: `Refused` |
//! | `nurbs_partials_over_piece`: `SectionNotProved(FaceWeightPositive)` | [`EnclosureCause::WeightsNotPositive`]: `Unsupported` |
//! | none: a span index and a box are trusted | [`EnclosureCause::NotAPiece`], [`EnclosureCause::BoxNotFinite`] |
//!
//! [`kind`]: NurbsSurfaceError::kind
//! [`name`]: NurbsSurfaceError::name

use core::fmt;

use scx_num::error::ErrorKind;
use scx_num::interval::Ivl;
use scx_nurbs::NurbsError;

use crate::ParamAxis;

/// Why a control net or a NURBS surface cannot be built or evaluated, or an edit cannot be
/// made.
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
    /// A knot insertion, degree elevation or split along `axis` failed on the curves of the
    /// net's lines along it. A context frame: the cause is the `scx-nurbs` curve edit's
    /// error, through [`std::error::Error::source`], and the kind is its kind (a parameter
    /// outside the domain is `Refused`, a degree above the cap `Exhausted`, an elevation that
    /// lost the surface `Unsupported`).
    Edit {
        /// The direction edited.
        axis: ParamAxis,
        /// Why.
        cause: NurbsError,
    },
    /// The basis of one direction cannot be evaluated at the parameter given. A context frame
    /// over the `scx-nurbs` error: `NurbsError::ParameterNotFinite`, `Degenerate` (a parameter
    /// so far outside the domain that a knot difference rounds away) or `DerivativeOrder`.
    Basis {
        /// The direction.
        axis: ParamAxis,
        /// Why.
        cause: NurbsError,
    },
    /// The evaluation at `uv` is not finite: a value overflowed (far outside the domain, or
    /// control points near the largest double).
    Degenerate {
        /// The parameters `[u, v]`.
        uv: [f64; 2],
    },
    /// The transform's matrix has a coefficient that is infinite or NaN.
    AffineNotFinite,
    /// A transformed control point, or its homogeneous point, overflowed to infinity.
    Overflow(NetCause),
    /// An enclosure over a box of parameters cannot be taken
    /// ([`NurbsSurface::partials_over`](crate::NurbsSurface::partials_over)).
    Enclosure(EnclosureCause),
    /// A step failed through the crate's own fault: values that the edited surface's
    /// invariants guarantee were refused or missing. A bug.
    Defect {
        /// The step that failed.
        why: &'static str,
    },
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

/// Why an enclosure over a box cannot be taken.
#[derive(Debug, Clone, Copy, PartialEq)]
#[non_exhaustive]
pub enum EnclosureCause {
    /// `span` is not a non-empty span of the `axis` knots (`p <= span < count`, `U[span] <
    /// U[span + 1]`).
    NotAPiece {
        /// The direction.
        axis: ParamAxis,
        /// The span index given.
        span: usize,
    },
    /// The `axis` range of the box has an end that is infinite or NaN, or its ends are out
    /// of order.
    BoxNotFinite {
        /// The direction.
        axis: ParamAxis,
        /// The range `[lo, hi]` given.
        range: [f64; 2],
    },
    /// The `axis` range of the box is not inside the domain (where the surface is not
    /// defined, no piece is the surface).
    BoxOutside {
        /// The direction.
        axis: ParamAxis,
        /// The range `[lo, hi]` given.
        range: [f64; 2],
        /// The domain of that direction.
        domain: [f64; 2],
    },
    /// The box crosses the `axis` knot line at `knot`, where the surface is continuous to
    /// order `continuity` (the degree minus the knot's multiplicity), below the `needed`
    /// asked for. Cut the box there.
    BelowContinuity {
        /// The direction.
        axis: ParamAxis,
        /// The knot crossed.
        knot: f64,
        /// The surface's continuity there.
        continuity: usize,
        /// The continuity asked for.
        needed: usize,
    },
    /// The weight function's enclosure over the box, `weight`, is not positive: the box is
    /// too wide for the form to prove the weights positive, and a smaller one may.
    WeightsNotPositive {
        /// The enclosure of the weight function.
        weight: Ivl,
    },
}

impl EnclosureCause {
    /// The cause's kind: [`ErrorKind::Refused`] for the caller's spans and boxes,
    /// [`ErrorKind::Unsupported`] for weights the form cannot prove positive.
    #[must_use]
    pub const fn kind(&self) -> ErrorKind {
        match self {
            Self::WeightsNotPositive { .. } => ErrorKind::Unsupported,
            Self::NotAPiece { .. }
            | Self::BoxNotFinite { .. }
            | Self::BoxOutside { .. }
            | Self::BelowContinuity { .. } => ErrorKind::Refused,
        }
    }

    /// The cause's stable name.
    #[must_use]
    pub const fn name(&self) -> &'static str {
        match self {
            Self::NotAPiece { .. } => "span_not_a_piece",
            Self::BoxNotFinite { .. } => "box_not_finite",
            Self::BoxOutside { .. } => "box_outside_domain",
            Self::BelowContinuity { .. } => "box_crosses_rough_knot",
            Self::WeightsNotPositive { .. } => "weights_not_proved_positive",
        }
    }
}

impl fmt::Display for EnclosureCause {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match *self {
            Self::NotAPiece { axis, span } => {
                write!(f, "{span} is not a span of the {axis} knots")
            }
            Self::BoxNotFinite {
                axis,
                range: [lo, hi],
            } => write!(
                f,
                "the {axis} range [{lo}, {hi}] is not finite and in order"
            ),
            Self::BoxOutside {
                axis,
                range: [lo, hi],
                domain: [a, b],
            } => write!(
                f,
                "the {axis} range [{lo}, {hi}] is not inside the domain [{a}, {b}]"
            ),
            Self::BelowContinuity {
                axis,
                knot,
                continuity,
                needed,
            } => write!(
                f,
                "the box crosses the {axis} knot {knot}, of continuity {continuity}, below {needed}"
            ),
            Self::WeightsNotPositive { weight } => write!(
                f,
                "the weights are not proved positive over the box: [{}, {}]",
                weight.lo, weight.hi
            ),
        }
    }
}

impl NurbsSurfaceError {
    /// The error's kind: [`ErrorKind::Refused`] (the net, weights, matrix and parameters are
    /// the caller's), the `scx-nurbs` cause's under [`Self::BadKnots`], [`Self::Basis`] and
    /// [`Self::Edit`] (62 D11), [`ErrorKind::Unsupported`] for a result with no double form, and
    /// [`ErrorKind::Defect`]; an enclosure's cause's ([`EnclosureCause::kind`]).
    #[must_use]
    pub const fn kind(&self) -> ErrorKind {
        match self {
            Self::BadKnots { cause, .. } | Self::Basis { cause, .. } | Self::Edit { cause, .. } => {
                cause.kind()
            }
            Self::BadNet(_)
            | Self::BadWeights(_)
            | Self::Degenerate { .. }
            | Self::AffineNotFinite => ErrorKind::Refused,
            Self::Overflow(_) => ErrorKind::Unsupported,
            Self::Enclosure(c) => c.kind(),
            Self::Defect { .. } => ErrorKind::Defect,
        }
    }

    /// The cause's stable name (62 D6).
    #[must_use]
    pub const fn name(&self) -> &'static str {
        match self {
            Self::BadNet(c) => c.name(),
            Self::BadKnots { .. } => "knots_break_surface",
            Self::BadWeights(c) => c.name(),
            Self::Basis { .. } => "basis_unevaluable",
            Self::Degenerate { .. } => "evaluation_not_finite",
            Self::Edit { .. } => "line_edit_failed",
            Self::AffineNotFinite => "affine_not_finite",
            Self::Overflow(_) => "transform_overflow",
            Self::Enclosure(c) => c.name(),
            Self::Defect { .. } => "defect",
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
            Self::Basis { axis, .. } => write!(f, "the {axis} basis cannot be evaluated"),
            Self::Degenerate { uv: [u, v] } => {
                write!(f, "the surface is not finite at ({u}, {v})")
            }
            Self::Edit { axis, .. } => write!(f, "the edit along {axis} failed"),
            Self::AffineNotFinite => f.write_str("the transform is not finite"),
            Self::Overflow(c) => write!(f, "the transform overflows: {c}"),
            Self::Enclosure(c) => c.fmt(f),
            Self::Defect { why } => write!(f, "internal defect: {why}"),
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
            Self::BadKnots { cause, .. } | Self::Basis { cause, .. } | Self::Edit { cause, .. } => {
                Some(cause)
            }
            Self::BadNet(_)
            | Self::BadWeights(_)
            | Self::Degenerate { .. }
            | Self::AffineNotFinite
            | Self::Overflow(_)
            | Self::Enclosure(_)
            | Self::Defect { .. } => None,
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
            NurbsSurfaceError::Basis {
                axis: ParamAxis::V,
                cause: NurbsError::ParameterNotFinite {
                    parameter: f64::NAN,
                },
            },
            NurbsSurfaceError::Degenerate { uv: [0.5, 1e300] },
            NurbsSurfaceError::Edit {
                axis: ParamAxis::U,
                cause: NurbsError::ParameterOutside {
                    parameter: 2.0,
                    domain: [0.0, 1.0],
                },
            },
            NurbsSurfaceError::AffineNotFinite,
            NurbsSurfaceError::Overflow(NetCause::NotFinite {
                row: at.0,
                col: at.1,
            }),
            NurbsSurfaceError::Enclosure(EnclosureCause::NotAPiece {
                axis: ParamAxis::U,
                span: 1,
            }),
            NurbsSurfaceError::Enclosure(EnclosureCause::BoxNotFinite {
                axis: ParamAxis::V,
                range: [0.5, f64::NAN],
            }),
            NurbsSurfaceError::Enclosure(EnclosureCause::BoxOutside {
                axis: ParamAxis::U,
                range: [-1.0, 0.5],
                domain: [0.0, 1.0],
            }),
            NurbsSurfaceError::Enclosure(EnclosureCause::BelowContinuity {
                axis: ParamAxis::V,
                knot: 0.5,
                continuity: 0,
                needed: 1,
            }),
            NurbsSurfaceError::Enclosure(EnclosureCause::WeightsNotPositive {
                weight: Ivl::new(-1.0, 2.0),
            }),
            NurbsSurfaceError::Defect { why: "a test" },
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

    /// One frame per Display: the context frames print only their own text and hand the
    /// `scx-nurbs` error on as the source; the others have none.
    #[test]
    fn display_is_one_frame() {
        for e in all() {
            let text = e.to_string();
            assert!(!text.is_empty() && !text.contains('\n'), "{text}");
            let expected = match e {
                NurbsSurfaceError::BadKnots { .. } | NurbsSurfaceError::Edit { .. } => {
                    ErrorKind::Refused
                }
                NurbsSurfaceError::Overflow(_)
                | NurbsSurfaceError::Enclosure(EnclosureCause::WeightsNotPositive { .. }) => {
                    ErrorKind::Unsupported
                }
                NurbsSurfaceError::Defect { .. } => ErrorKind::Defect,
                _ => ErrorKind::Refused,
            };
            assert_eq!(e.kind(), expected, "{e:?}");
            match e {
                NurbsSurfaceError::BadKnots { cause, .. }
                | NurbsSurfaceError::Edit { cause, .. } => {
                    assert!(!text.contains(&cause.to_string()));
                    assert_eq!(e.source().map(ToString::to_string), Some(cause.to_string()));
                }
                NurbsSurfaceError::Basis { cause, .. } => {
                    assert_eq!(text, "the v basis cannot be evaluated");
                    assert_eq!(e.source().map(ToString::to_string), Some(cause.to_string()));
                }
                _ => assert!(e.source().is_none()),
            }
        }
        assert_eq!(all()[4].to_string(), "the u knots cannot carry a surface");
    }
}
