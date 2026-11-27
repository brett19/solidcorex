// Derived from solidcore `solidcore/crates/solidcore/src/error.rs`,
// `solidcore/crates/solidcore/src/error/geom.rs` and
// `solidcore/crates/solidcore/src/error/blend.rs` at commit
// 03e16c9ee766de5b9b1ced845e5149d62ff493b8.
// Upstream record (verbatim from those files): error.rs: Upstream:
// `source/SMLib/inc/SmMessages.h` (`SmStatus`, `SM_SUCCESS`, `SM_ERR_*` codes and the
// `SE`/`SER`/`NER` error-propagation macros). error/geom.rs: Upstream: none. These describe
// failures of the geometry utilities (`crate::geom`: vectors, matrices and transforms, boxes
// and extents, the point-set and plane utilities, the one-dimensional and global solvers,
// grids, tolerance relations, attributes and object caches). Each also records, for difftest
// only, the status the C++ reports for it (crate-private `comparison_status`, read through
// [`crate::SolidError::sm_status`]). error/blend.rs: Upstream: none. These describe failures
// of the blend subsystem (`crate::blend`: the fillet and feature executives, the fillet
// geometry, solvers, sections, corners, overflows, intersectors, outlines, removal and the
// exact and rolling-ball fillets).
// Changes: only the variants layer 0 builds, renamed: `SolidError` -> `NumError` (its
// `InvalidInput` and `Failed` shapes), `GeomFailure` -> `NumFailure` (the causes the moved
// files build). `GeomFailure::CallFailed { class }` became the context frame
// `NumError::Call`, whose kind is read from its cause. Added from the blend error for the
// interval move: `NumFailure::Defect` (`BlendFailure::Defect`) and
// `NumFailure::VectorVanishes` (`BlendFailure::SectionDegenerate(SectionDegeneracy::VectorVanishes)`).
// No `Failure` wrapper, site, fit, status numbers or upstream-status accessors. Rewritten
// for solidcorex (research 62): `ErrorKind` is the five kinds of 01 §5 (`Refused`,
// `Unsupported`, `Exhausted`, `Cancelled`, `Defect`), not the pin's classes, and each cause's
// kind is its own (62 §3.1); the pin's class and text of each cause are the difftest
// facade's mapping, not this file's. Written here: `ErrorKind::name`, the `Display` impls'
// `fmt`, `NumFailure::kind`, `NumError::invalid_input`, `NumError::failed`,
// `NumError::call`, `Limit` and `NumError::Exhausted` (62 T13: the fixed caps and soft
// allocation failures solidcore reports as `InvalidInput`), `NumError::kind`,
// `NumError::limit`, `NumError::operation`, `NumError::failure`,
// `NumError::cause`, `Error::source`, `chain` (research 62 §2.4: each `Display` prints
// one frame, where solidcore's `CallFailed` printed its cause too), `Invalid` (62 T4: the
// typed reason of an invalid input, where solidcore's is text), each cause's `name` and
// `NumError::name` (62 D6), and the tests.
//! Why a layer-0 routine fails.
//!
//! A check of the caller's argument that fails is [`NumError::InvalidInput`]; a
//! computation that cannot go on is [`NumError::Failed`], with a typed [`NumFailure`]
//! carrying the numbers the check computed; a routine that reports a callee's error as its
//! own wraps it in the context frame [`NumError::Call`]. The kind ([`ErrorKind`]) is read
//! from the cause, never stored. The operation strings are solidcore's, unchanged; the
//! difftest compares them.
//!
//! | solidcore at the pin                               | here                                  |
//! |----------------------------------------------------|---------------------------------------|
//! | `SolidError::InvalidInput { operation, reason }` (text reason) | [`NumError::InvalidInput`] with a typed [`Invalid`] |
//! | `SolidError::InvalidInput` for a fixed cap or a failed allocation | [`NumError::Exhausted`] with a [`Limit`] |
//! | `SolidError::Failed` with `Reason::Geom(g)`        | [`NumError::Failed`] with the same-named [`NumFailure`] |
//! | `SolidError::geom(op, g)`                          | [`NumError::failed`]                  |
//! | `SolidError::geom_call(op)` (`GeomFailure::CallFailed`) | [`NumError::call`] ([`NumError::Call`]) |
//! | `SolidError::blend(op, BlendFailure::Defect { why })` (series) | [`NumError::failed`] with [`NumFailure::Defect`] |
//! | `SolidError::blend(op, BlendFailure::SectionDegenerate(SectionDegeneracy::VectorVanishes))` (series) | [`NumError::failed`] with [`NumFailure::VectorVanishes`] |
//! | `ErrorKind` (the pin's classes)                    | [`ErrorKind`] (the five kinds; the difftest facade maps each cause to the pin's class) |
//!
//! Every failure here has upstream status `SM_ERR` (1001) and every invalid input
//! `SM_ERR_INVALID_INPUT` (1007), as in solidcore; the status is not kept.

use core::fmt;

/// `Result` with [`NumError`].
pub type Result<T, E = NumError> = core::result::Result<T, E>;

/// What the caller can do about an error (01 §5, research 62 §1.1). Every error of every
/// crate has one; it is read from the cause, never stored.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ErrorKind {
    /// The input is invalid, or cannot hold as asked. The caller changes the input.
    Refused,
    /// The input is valid, and the method cannot finish it.
    Unsupported,
    /// A limit the caller or the machine sets ran out: work, allocation, depth, count or
    /// memory. The caller gives more, or stops.
    Exhausted,
    /// The caller stopped the operation.
    Cancelled,
    /// The kernel's own fault: a bug.
    Defect,
}

impl ErrorKind {
    /// The kind's stable name.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Refused => "refused",
            Self::Unsupported => "unsupported",
            Self::Exhausted => "exhausted",
            Self::Cancelled => "cancelled",
            Self::Defect => "defect",
        }
    }
}

impl fmt::Display for ErrorKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name())
    }
}

/// What failed, with the numbers the check computed (the `GeomFailure` variants the
/// layer-0 files build, same names and fields).
#[non_exhaustive]
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum NumFailure {
    /// Two intervals to intersect (or to join) are disjoint. [`ErrorKind::Refused`]
    /// (provisional: research 62 T5 reads each caller first).
    Disjoint {
        /// The first interval, `[min, max]`.
        a: [f64; 2],
        /// The second interval, `[min, max]`.
        b: [f64; 2],
    },
    /// A step failed through the kernel's own fault, with why that is so
    /// ([`ErrorKind::Defect`]). solidcore's `BlendFailure::Defect` (the series of
    /// [`crate::series`]).
    Defect {
        /// The step that failed, and why that is the kernel's fault.
        why: &'static str,
    },
    /// An index is not below the number of entries. [`ErrorKind::Refused`].
    IndexOutOfRange {
        /// The index given.
        index: usize,
        /// The number of entries.
        len: usize,
    },
    /// The Jacobi eigen-solver stopped after `sweeps` sweeps on a finite symmetric matrix
    /// with the off-diagonal sum still not below the kernel's zero ([`ErrorKind::Defect`]:
    /// non-finite input is refused before the sweeps).
    JacobiDidNotConverge {
        /// The sweeps run.
        sweeps: usize,
        /// The off-diagonal sum measured at the last sweep.
        off_diagonal: f64,
    },
    /// No vector perpendicular to the given one was found. [`ErrorKind::Refused`].
    NoPerpendicular,
    /// A matrix the routine needs square is not. [`ErrorKind::Refused`].
    NotSquare {
        /// The matrix's rows.
        rows: usize,
        /// The matrix's columns.
        cols: usize,
    },
    /// A matrix the routine needs symmetric is not. [`ErrorKind::Refused`].
    NotSymmetric,
    /// A point to normalise lies outside the extent (or off a degenerate side of it).
    /// [`ErrorKind::Refused`] (provisional: research 62 T5 reads each caller first).
    PointOutsideExtent {
        /// The point's `u`.
        u: f64,
        /// The point's `v`.
        v: f64,
    },
    /// A linear system or matrix is singular: its pivot or determinant, `measure`, is
    /// below the kernel's zero for it. [`ErrorKind::Refused`]: at layer 0 the singular
    /// matrix is the caller's argument.
    Singular {
        /// The pivot or determinant that fell below the kernel's zero.
        measure: f64,
    },
    /// A span to add to a periodic extent does not lie in `[0, period]`.
    /// [`ErrorKind::Refused`].
    SpanOutsidePeriod {
        /// The span's start.
        from: f64,
        /// The span's end.
        to: f64,
        /// The extent's period.
        period: f64,
    },
    /// A vector a series normalises vanishes (a face's normal at a foot, or the leader's
    /// tangent): solidcore's `BlendFailure::SectionDegenerate(SectionDegeneracy::VectorVanishes)`
    /// ([`crate::series`]). [`ErrorKind::Refused`].
    VectorVanishes,
    /// A matrix to decompose by singular values is empty or has fewer rows than columns.
    /// [`ErrorKind::Refused`].
    TooFewRows {
        /// The matrix's rows.
        rows: usize,
        /// The matrix's columns.
        cols: usize,
    },
}

impl NumFailure {
    /// The cause's stable name, unique in the crate (research 62 D6).
    #[must_use]
    pub fn name(&self) -> &'static str {
        match self {
            Self::Disjoint { .. } => "disjoint",
            Self::Defect { .. } => "defect",
            Self::IndexOutOfRange { .. } => "index_out_of_range",
            Self::JacobiDidNotConverge { .. } => "jacobi_did_not_converge",
            Self::NoPerpendicular => "no_perpendicular",
            Self::NotSquare { .. } => "not_square",
            Self::NotSymmetric => "not_symmetric",
            Self::PointOutsideExtent { .. } => "point_outside_extent",
            Self::Singular { .. } => "singular",
            Self::SpanOutsidePeriod { .. } => "span_outside_period",
            Self::VectorVanishes => "vector_vanishes",
            Self::TooFewRows { .. } => "too_few_rows",
        }
    }
}

impl NumFailure {
    /// The failure's kind (research 62 §3.1).
    #[must_use]
    pub const fn kind(&self) -> ErrorKind {
        match self {
            Self::IndexOutOfRange { .. }
            | Self::NotSquare { .. }
            | Self::NotSymmetric
            | Self::SpanOutsidePeriod { .. }
            | Self::TooFewRows { .. }
            | Self::NoPerpendicular
            | Self::Singular { .. }
            | Self::VectorVanishes
            | Self::Disjoint { .. }
            | Self::PointOutsideExtent { .. } => ErrorKind::Refused,
            Self::JacobiDidNotConverge { .. } | Self::Defect { .. } => ErrorKind::Defect,
        }
    }
}

impl fmt::Display for NumFailure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Disjoint { a: [a0, a1], b: [b0, b1] } => {
                write!(f, "the intervals [{a0}, {a1}] and [{b0}, {b1}] are disjoint")
            }
            Self::Defect { why } => f.write_str(why),
            Self::IndexOutOfRange { index, len } => {
                write!(f, "the index {index} is not below the {len} entries")
            }
            Self::JacobiDidNotConverge {
                sweeps,
                off_diagonal,
            } => write!(
                f,
                "the Jacobi sweeps did not converge after {sweeps} sweeps (off-diagonal sum {off_diagonal:e})"
            ),
            Self::NoPerpendicular => f.write_str("no perpendicular vector found"),
            Self::NotSquare { rows, cols } => {
                write!(f, "the {rows}x{cols} matrix is not square")
            }
            Self::NotSymmetric => f.write_str("the matrix is not symmetric"),
            Self::PointOutsideExtent { u, v } => {
                write!(f, "the point ({u}, {v}) lies outside the extent")
            }
            Self::Singular { measure } => write!(
                f,
                "the system is singular (its pivot or determinant is {measure:e})"
            ),
            Self::SpanOutsidePeriod { from, to, period } => write!(
                f,
                "the span from {from} to {to} does not lie in the period [0, {period}]"
            ),
            Self::VectorVanishes => f.write_str(
                "a vector the section normalises vanishes (a face's normal at a foot, or the leader's tangent)",
            ),
            Self::TooFewRows { rows, cols } => write!(
                f,
                "the {rows}x{cols} matrix is empty or has fewer rows than columns"
            ),
        }
    }
}

/// A limit that ran out, with what was asked and the limit ([`ErrorKind::Exhausted`],
/// research 62 M1: a limit the caller or the machine sets, fixed caps on input size
/// included).
#[non_exhaustive]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Limit {
    /// `count` values could not be allocated: they would take more than
    /// [`crate::limits::MAX_ALLOC_BYTES`], or the machine refused the reservation.
    Allocation {
        /// The values asked for.
        count: usize,
    },
    /// A linear system has more rows than the solver's fixed scale buffer holds
    /// ([`crate::matrix::MAX_MATRIX_SIZE`]).
    MatrixSize {
        /// The system's rows.
        rows: usize,
        /// The most rows solved.
        limit: usize,
    },
    /// A derivative order above the highest the routine evaluates
    /// ([`crate::transform::MAX_DERIV`]).
    DerivativeOrder {
        /// The order asked for.
        order: usize,
        /// The highest order evaluated.
        limit: usize,
    },
    /// A series truncation keeps more orders than a series stores
    /// ([`crate::series::MAX_S`], [`crate::series::MAX_E`]).
    SeriesOrder {
        /// The `sigma` order asked for.
        s: usize,
        /// The `eps` order asked for.
        e: usize,
        /// The highest `sigma` order stored.
        limit_s: usize,
        /// The highest `eps` order stored at that `s`.
        limit_e: usize,
    },
}

impl Limit {
    /// The cause's stable name, unique in the crate (research 62 D6).
    #[must_use]
    pub fn name(&self) -> &'static str {
        match self {
            Self::Allocation { .. } => "allocation",
            Self::MatrixSize { .. } => "matrix_size",
            Self::DerivativeOrder { .. } => "derivative_order",
            Self::SeriesOrder { .. } => "series_order",
        }
    }
}

impl fmt::Display for Limit {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Allocation { count } => write!(f, "{count} values could not be allocated"),
            Self::MatrixSize { rows, limit } => {
                write!(
                    f,
                    "the system has {rows} rows, more than the {limit} solved"
                )
            }
            Self::DerivativeOrder { order, limit } => write!(
                f,
                "derivative order {order} is above the highest evaluated, {limit}"
            ),
            Self::SeriesOrder {
                s,
                e,
                limit_s,
                limit_e,
            } => write!(
                f,
                "series orders ({s}, {e}) exceed the stored orders ({limit_s}, {limit_e})"
            ),
        }
    }
}

/// What was wrong with an argument a routine refused ([`NumError::InvalidInput`]).
#[non_exhaustive]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Invalid {
    /// A number in the argument is infinite or NaN.
    NotFinite,
    /// A vector or axis that needs a length has none.
    ZeroLength,
    /// A direction is of no length or not finite.
    NoDirection,
    /// A scale's numbers are not finite, or a factor is zero.
    ScaleFactor,
    /// A plane's axes are of no length or along one line.
    DependentAxes,
    /// The map built from the arguments is too large to represent.
    Unrepresentable,
    /// An axis has no finite point and direction.
    NoFiniteAxis,
    /// A plane's axes or the motion applied to it are not usable.
    PlaneAxesOrMotion,
    /// A map has a projective column where an affine one is needed.
    Projective,
    /// A set that must hold something is empty.
    Empty,
    /// A value computed from finite arguments overflows or is not finite.
    Overflows,
    /// The matrix is not square.
    MatrixNotSquare {
        /// The matrix's rows.
        rows: usize,
        /// The matrix's columns.
        cols: usize,
    },
    /// The matrix is not 3x3.
    MatrixNot3x3 {
        /// The matrix's rows.
        rows: usize,
        /// The matrix's columns.
        cols: usize,
    },
    /// The element count is not `rows * cols`.
    ElementCount {
        /// The rows asked for.
        rows: usize,
        /// The columns asked for.
        cols: usize,
        /// The elements given.
        len: usize,
    },
    /// A right-hand side's length differs from the matrix's rows.
    RhsSize {
        /// The matrix's rows.
        rows: usize,
        /// The right-hand side's length.
        len: usize,
    },
    /// Two arguments' sizes do not match.
    SizeMismatch,
    /// An index is outside the container.
    IndexOutside,
    /// An interval is not within `[0, period]`.
    OutsidePeriod,
    /// A vector lies along the axis it must not.
    AlongAxis,
    /// Directions are not unit and perpendicular to a unit normal.
    NotPerpendicular,
    /// A line of no length, or an undefined box or line.
    UndefinedLine,
    /// A voxel model has no voxels.
    NoVoxels,
}

impl Invalid {
    /// The cause's stable name, unique in the crate (research 62 D6).
    #[must_use]
    pub fn name(&self) -> &'static str {
        match self {
            Self::NotFinite => "not_finite",
            Self::ZeroLength => "zero_length",
            Self::NoDirection => "no_direction",
            Self::ScaleFactor => "scale_factor",
            Self::DependentAxes => "dependent_axes",
            Self::Unrepresentable => "unrepresentable",
            Self::NoFiniteAxis => "no_finite_axis",
            Self::PlaneAxesOrMotion => "plane_axes_or_motion",
            Self::Projective => "projective",
            Self::Empty => "empty",
            Self::Overflows => "overflows",
            Self::MatrixNotSquare { .. } => "matrix_not_square",
            Self::MatrixNot3x3 { .. } => "matrix_not_3x3",
            Self::ElementCount { .. } => "element_count",
            Self::RhsSize { .. } => "rhs_size",
            Self::SizeMismatch => "size_mismatch",
            Self::IndexOutside => "index_outside",
            Self::OutsidePeriod => "outside_period",
            Self::AlongAxis => "along_axis",
            Self::NotPerpendicular => "not_perpendicular",
            Self::UndefinedLine => "undefined_line",
            Self::NoVoxels => "no_voxels",
        }
    }
}

impl fmt::Display for Invalid {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotFinite => f.write_str("a number is not finite"),
            Self::ZeroLength => f.write_str("a vector of no length"),
            Self::NoDirection => f.write_str("a direction of no length or not finite"),
            Self::ScaleFactor => f.write_str("a scale not finite or of factor zero"),
            Self::DependentAxes => f.write_str("axes of no length or along one line"),
            Self::Unrepresentable => f.write_str("a map too large to represent"),
            Self::NoFiniteAxis => f.write_str("the axis has no finite point and direction"),
            Self::PlaneAxesOrMotion => f.write_str("invalid plane axes or motion"),
            Self::Projective => f.write_str("a projective column"),
            Self::Empty => f.write_str("an empty set"),
            Self::Overflows => f.write_str("a computed value overflows"),
            Self::MatrixNotSquare { rows, cols } => {
                write!(f, "the {rows}x{cols} matrix is not square")
            }
            Self::MatrixNot3x3 { rows, cols } => write!(f, "the {rows}x{cols} matrix is not 3x3"),
            Self::ElementCount { rows, cols, len } => {
                write!(f, "{len} elements for a {rows}x{cols} matrix")
            }
            Self::RhsSize { rows, len } => {
                write!(f, "a right-hand side of {len} for {rows} rows")
            }
            Self::SizeMismatch => f.write_str("sizes do not match"),
            Self::IndexOutside => f.write_str("an index out of range"),
            Self::OutsidePeriod => f.write_str("min and max are not both within [0, period]"),
            Self::AlongAxis => f.write_str("a vector along the axis"),
            Self::NotPerpendicular => {
                f.write_str("expected unit directions perpendicular to a unit normal")
            }
            Self::UndefinedLine => f.write_str("a line of no length, or an undefined box or line"),
            Self::NoVoxels => f.write_str("no voxels"),
        }
    }
}

/// A layer-0 error.
#[derive(Debug, Clone, PartialEq)]
pub enum NumError {
    /// An argument was out of range, malformed or inconsistent (upstream
    /// `SM_ERR_INVALID_INPUT`). [`ErrorKind::Refused`].
    InvalidInput {
        /// Operation that rejected the input.
        operation: &'static str,
        /// What was wrong.
        reason: Invalid,
    },
    /// A computation could not go on (upstream `SM_ERR`). Its kind is the failure's.
    Failed {
        /// The solidcore path of the failing routine.
        operation: &'static str,
        /// What failed.
        failure: NumFailure,
    },
    /// A limit ran out (upstream `SM_ERR_INVALID_INPUT` for a fixed cap, or a soft
    /// allocation failure). [`ErrorKind::Exhausted`].
    Exhausted {
        /// The solidcore path of the routine whose limit ran out.
        operation: &'static str,
        /// The limit, and what was asked.
        limit: Limit,
    },
    /// A context frame: `operation` reports a callee's error as its own (solidcore
    /// `GeomFailure::CallFailed`). Its kind is the cause's (research 62 §2.3 rule 1:
    /// the frame passed the caller's data through).
    Call {
        /// The solidcore path of the calling routine.
        operation: &'static str,
        /// The callee's error.
        cause: Box<NumError>,
    },
}

impl NumError {
    /// [`NumError::InvalidInput`].
    #[must_use]
    pub const fn invalid_input(operation: &'static str, reason: Invalid) -> Self {
        Self::InvalidInput { operation, reason }
    }

    /// [`NumError::Failed`] of `operation` for `failure`.
    #[must_use]
    pub const fn failed(operation: &'static str, failure: NumFailure) -> Self {
        Self::Failed { operation, failure }
    }

    /// A map from a callee's error to a [`NumError::Call`] frame of `operation` that keeps
    /// it as its cause. Use as `.map_err(NumError::call(OP))`.
    pub fn call(operation: &'static str) -> impl Fn(Self) -> Self {
        move |e| Self::Call {
            operation,
            cause: Box::new(e),
        }
    }

    /// The error's kind: the cause's, through [`NumError::Call`] frames.
    #[must_use]
    pub fn kind(&self) -> ErrorKind {
        match self {
            Self::InvalidInput { .. } => ErrorKind::Refused,
            Self::Exhausted { .. } => ErrorKind::Exhausted,
            Self::Failed { failure, .. } => failure.kind(),
            Self::Call { cause, .. } => cause.kind(),
        }
    }

    /// The stable name of the cause at the end of the chain, unique in the crate
    /// (research 62 D6): with the kind and the call, the error's signature.
    #[must_use]
    pub fn name(&self) -> &'static str {
        match self {
            Self::InvalidInput { reason, .. } => reason.name(),
            Self::Exhausted { limit, .. } => limit.name(),
            Self::Failed { failure, .. } => failure.name(),
            Self::Call { cause, .. } => cause.name(),
        }
    }

    /// The operation that failed.
    #[must_use]
    pub const fn operation(&self) -> &'static str {
        match self {
            Self::InvalidInput { operation, .. }
            | Self::Exhausted { operation, .. }
            | Self::Failed { operation, .. }
            | Self::Call { operation, .. } => operation,
        }
    }

    /// The typed failure, for [`NumError::Failed`].
    #[must_use]
    pub const fn failure(&self) -> Option<&NumFailure> {
        match self {
            Self::Failed { failure, .. } => Some(failure),
            Self::InvalidInput { .. } | Self::Exhausted { .. } | Self::Call { .. } => None,
        }
    }

    /// The limit that ran out, for [`NumError::Exhausted`].
    #[must_use]
    pub const fn limit(&self) -> Option<&Limit> {
        match self {
            Self::Exhausted { limit, .. } => Some(limit),
            Self::InvalidInput { .. } | Self::Failed { .. } | Self::Call { .. } => None,
        }
    }

    /// The callee's error a [`NumError::Call`] frame keeps.
    #[must_use]
    pub fn cause(&self) -> Option<&Self> {
        match self {
            Self::Call { cause, .. } => Some(cause),
            Self::InvalidInput { .. } | Self::Exhausted { .. } | Self::Failed { .. } => None,
        }
    }
}

impl fmt::Display for NumError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidInput { operation, reason } => {
                write!(f, "{operation}: invalid input: {reason}")
            }
            Self::Exhausted { operation, limit } => write!(f, "{operation}: {limit}"),
            Self::Failed { operation, failure } => write!(f, "{operation}: {failure}"),
            Self::Call { operation, .. } => write!(f, "{operation}: a routine it calls failed"),
        }
    }
}

impl std::error::Error for NumError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        self.cause()
            .map(|c| c as &(dyn std::error::Error + 'static))
    }
}

/// The error and its sources, one frame each, joined by ": " (research 62 §2.4). Each
/// `Display` prints its own frame only; this prints the chain a report shows.
///
/// ```
/// use scx_num::error::{chain, NumError, NumFailure};
/// let e = NumError::call("outer")(NumError::failed("inner", NumFailure::NotSymmetric));
/// assert_eq!(
///     chain(&e).to_string(),
///     "outer: a routine it calls failed: inner: the matrix is not symmetric"
/// );
/// ```
#[must_use]
pub fn chain<'a>(e: &'a (dyn std::error::Error + 'static)) -> impl fmt::Display + 'a {
    Chain(e)
}

struct Chain<'a>(&'a (dyn std::error::Error + 'static));

impl fmt::Display for Chain<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)?;
        let mut next = self.0.source();
        while let Some(e) = next {
            write!(f, ": {e}")?;
            next = e.source();
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_call_frame_takes_its_kind_from_the_cause() {
        let inner = NumError::failed("a", NumFailure::Singular { measure: 0.0 });
        let outer = NumError::call("b")(inner.clone());
        assert_eq!(outer.kind(), ErrorKind::Refused);
        assert_eq!(outer.cause(), Some(&inner));
        assert_eq!(outer.operation(), "b");
        assert_eq!(outer.failure(), None);
        let defect = NumError::failed("a", NumFailure::Defect { why: "why" });
        assert_eq!(NumError::call("b")(defect).kind(), ErrorKind::Defect);
        let bad = NumError::invalid_input("c", Invalid::Empty);
        assert_eq!(bad.kind(), ErrorKind::Refused);
        assert_eq!(NumError::call("d")(bad).kind(), ErrorKind::Refused);
    }

    /// Every failure's kind (research 62 §3.1).
    #[test]
    fn every_failure_has_its_kind() {
        let cases: [(NumFailure, ErrorKind); 12] = [
            (
                NumFailure::Disjoint {
                    a: [1.0, 2.5],
                    b: [-0.0, 3.0],
                },
                ErrorKind::Refused,
            ),
            (NumFailure::Defect { why: "why" }, ErrorKind::Defect),
            (
                NumFailure::IndexOutOfRange { index: 4, len: 3 },
                ErrorKind::Refused,
            ),
            (
                NumFailure::JacobiDidNotConverge {
                    sweeps: 50,
                    off_diagonal: 1.5e-3,
                },
                ErrorKind::Defect,
            ),
            (NumFailure::NoPerpendicular, ErrorKind::Refused),
            (
                NumFailure::NotSquare { rows: 2, cols: 3 },
                ErrorKind::Refused,
            ),
            (NumFailure::NotSymmetric, ErrorKind::Refused),
            (
                NumFailure::PointOutsideExtent { u: 0.5, v: -2.5 },
                ErrorKind::Refused,
            ),
            (NumFailure::Singular { measure: 0.0 }, ErrorKind::Refused),
            (
                NumFailure::SpanOutsidePeriod {
                    from: -1.0,
                    to: 7.0,
                    period: 6.0,
                },
                ErrorKind::Refused,
            ),
            (NumFailure::VectorVanishes, ErrorKind::Refused),
            (
                NumFailure::TooFewRows { rows: 1, cols: 2 },
                ErrorKind::Refused,
            ),
        ];
        for (failure, kind) in cases {
            assert_eq!(failure.kind(), kind, "{failure:?}");
            let e = NumError::failed("op", failure);
            assert_eq!(e.kind(), kind);
            assert_eq!(e.failure(), Some(&failure));
            assert!(std::error::Error::source(&e).is_none());
        }
        let names: Vec<&str> = [
            ErrorKind::Refused,
            ErrorKind::Unsupported,
            ErrorKind::Exhausted,
            ErrorKind::Cancelled,
            ErrorKind::Defect,
        ]
        .into_iter()
        .map(ErrorKind::name)
        .collect();
        assert_eq!(
            names,
            ["refused", "unsupported", "exhausted", "cancelled", "defect"]
        );
        let full = NumError::Exhausted {
            operation: "op",
            limit: Limit::Allocation { count: 7 },
        };
        assert_eq!(full.kind(), ErrorKind::Exhausted);
        assert_eq!(full.limit(), Some(&Limit::Allocation { count: 7 }));
        assert_eq!(NumError::call("outer")(full).kind(), ErrorKind::Exhausted);
        let caused = NumError::call("outer")(NumError::invalid_input("inner", Invalid::Empty));
        assert!(std::error::Error::source(&caused).is_some());
        // One frame per Display; `chain` joins them.
        assert_eq!(caused.to_string(), "outer: a routine it calls failed");
        assert_eq!(
            chain(&NumError::call("top")(caused)).to_string(),
            "top: a routine it calls failed: outer: a routine it calls failed: inner: invalid input: an empty set"
        );
        assert_eq!(NumError::invalid_input("x", Invalid::Empty).failure(), None);
    }

    /// Every cause's name is unique in the crate (research 62 D6), and the error's name is
    /// its last cause's.
    #[test]
    fn cause_names_are_unique() {
        let failures = [
            NumFailure::Disjoint {
                a: [0.0; 2],
                b: [0.0; 2],
            },
            NumFailure::IndexOutOfRange { index: 0, len: 0 },
            NumFailure::JacobiDidNotConverge {
                sweeps: 0,
                off_diagonal: 0.0,
            },
            NumFailure::NotSquare { rows: 0, cols: 0 },
            NumFailure::NotSymmetric,
            NumFailure::SpanOutsidePeriod {
                from: 0.0,
                to: 0.0,
                period: 0.0,
            },
            NumFailure::TooFewRows { rows: 0, cols: 0 },
            NumFailure::NoPerpendicular,
            NumFailure::Singular { measure: 0.0 },
            NumFailure::VectorVanishes,
            NumFailure::PointOutsideExtent { u: 0.0, v: 0.0 },
            NumFailure::Defect { why: "" },
        ];
        let limits = [
            Limit::Allocation { count: 0 },
            Limit::MatrixSize { rows: 0, limit: 0 },
            Limit::DerivativeOrder { order: 0, limit: 0 },
            Limit::SeriesOrder {
                s: 0,
                e: 0,
                limit_s: 0,
                limit_e: 0,
            },
        ];
        let invalid = [
            Invalid::NotFinite,
            Invalid::ZeroLength,
            Invalid::NoDirection,
            Invalid::ScaleFactor,
            Invalid::DependentAxes,
            Invalid::Unrepresentable,
            Invalid::NoFiniteAxis,
            Invalid::PlaneAxesOrMotion,
            Invalid::Projective,
            Invalid::Empty,
            Invalid::Overflows,
            Invalid::MatrixNotSquare { rows: 0, cols: 0 },
            Invalid::MatrixNot3x3 { rows: 0, cols: 0 },
            Invalid::ElementCount {
                rows: 0,
                cols: 0,
                len: 0,
            },
            Invalid::RhsSize { rows: 0, len: 0 },
            Invalid::SizeMismatch,
            Invalid::IndexOutside,
            Invalid::OutsidePeriod,
            Invalid::AlongAxis,
            Invalid::NotPerpendicular,
            Invalid::UndefinedLine,
            Invalid::NoVoxels,
        ];
        let names: Vec<&str> = failures
            .iter()
            .map(NumFailure::name)
            .chain(limits.iter().map(Limit::name))
            .chain(invalid.iter().map(Invalid::name))
            .collect();
        let mut sorted = names.clone();
        sorted.sort_unstable();
        sorted.dedup();
        assert_eq!(sorted.len(), names.len(), "{names:?}");
        assert!(names.iter().all(|n| {
            n.bytes()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_')
        }));
        let inner = NumError::failed("b", NumFailure::NotSymmetric);
        assert_eq!(NumError::call("a")(inner).name(), "not_symmetric");
        assert_eq!(NumError::invalid_input("a", Invalid::Empty).name(), "empty");
    }
}
