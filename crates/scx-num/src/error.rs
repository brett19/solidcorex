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
// files build, with their classes and `Display` texts); `ErrorKind` keeps the four classes
// those causes take. Added from the blend error for the interval move: `NumFailure::Defect`
// (`BlendFailure::Defect`, StageFailed, the same text) and `NumFailure::VectorVanishes`
// (`BlendFailure::SectionDegenerate(SectionDegeneracy::VectorVanishes)`, Degenerate, the
// same text, class first). No `Failure` wrapper, site, fit, status numbers or upstream-status
// accessors. Written here over those variants: `ErrorKind::name` (the four names), the
// `Display` impls' `fmt`, `NumFailure::kind`, `NumError::invalid_input`, `NumError::failed`,
// `NumError::call`, `NumError::kind`, `NumError::operation`, `NumError::failure`,
// `NumError::cause`, `Error::source`, and the tests
// `a_call_failure_takes_its_class_and_message_from_the_cause` and
// `every_variant_prints_the_pins_text` (the texts of the pin's `error/geom.rs` and
// `error/blend.rs`, for the variants no public input reaches on both sides).
//! Why a layer-0 routine fails.
//!
//! Two shapes, as in solidcore: a check of the caller's argument that fails is
//! [`NumError::InvalidInput`]; a computation that cannot go on is [`NumError::Failed`],
//! with a typed [`NumFailure`] carrying the numbers the check computed. The operation
//! strings are solidcore's, unchanged, so a message reads as it did there.
//!
//! | solidcore at the pin                               | here                                  |
//! |----------------------------------------------------|---------------------------------------|
//! | `SolidError::InvalidInput { operation, reason }`   | [`NumError::InvalidInput`]            |
//! | `SolidError::Failed` with `Reason::Geom(g)`        | [`NumError::Failed`] with the same-named [`NumFailure`] |
//! | `SolidError::geom(op, g)`                          | [`NumError::failed`]                  |
//! | `SolidError::geom_call(op)`                        | [`NumError::call`]                    |
//! | `SolidError::blend(op, BlendFailure::Defect { why })` (series) | [`NumError::failed`] with [`NumFailure::Defect`] |
//! | `SolidError::blend(op, BlendFailure::SectionDegenerate(SectionDegeneracy::VectorVanishes))` (series) | [`NumError::failed`] with [`NumFailure::VectorVanishes`] |
//! | `ErrorKind` (the classes used here)                | [`ErrorKind`]                         |
//!
//! Every failure here has upstream status `SM_ERR` (1001) and every invalid input
//! `SM_ERR_INVALID_INPUT` (1007), as in solidcore; the status is not kept (only the
//! differential tests compare it, from the variant).

use core::fmt;

/// `Result` with [`NumError`].
pub type Result<T, E = NumError> = core::result::Result<T, E>;

/// The class of an error, as solidcore's `ErrorKind` names it (the classes layer 0 has).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ErrorKind {
    /// The caller's argument was out of range, malformed or inconsistent.
    InvalidInput,
    /// Geometry is degenerate where the routine needs it not to be.
    Degenerate,
    /// A stage failed through the kernel's own fault (an unconverged solve).
    StageFailed,
    /// A generic failure (`SM_ERR`).
    Failed,
}

impl ErrorKind {
    /// solidcore's name of the class.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::InvalidInput => "invalid_input",
            Self::Degenerate => "degenerate",
            Self::StageFailed => "stage_failed",
            Self::Failed => "failed",
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
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum NumFailure {
    /// A routine this one calls failed; the error keeps that routine's error as its
    /// cause and takes its class from it.
    CallFailed {
        /// The class of the callee's error, the cause.
        class: ErrorKind,
    },
    /// Two intervals to intersect (or to join) are disjoint.
    Disjoint {
        /// The first interval, `[min, max]`.
        a: [f64; 2],
        /// The second interval, `[min, max]`.
        b: [f64; 2],
    },
    /// A step failed through the kernel's own fault, with why that is so
    /// ([`ErrorKind::StageFailed`]). solidcore's `BlendFailure::Defect` (the series of
    /// [`crate::series`]).
    Defect {
        /// The step that failed, and why that is the kernel's fault.
        why: &'static str,
    },
    /// An index is not below the number of entries. [`ErrorKind::InvalidInput`].
    IndexOutOfRange {
        /// The index given.
        index: usize,
        /// The number of entries.
        len: usize,
    },
    /// The Jacobi eigen-solver stopped after `sweeps` sweeps with the off-diagonal sum
    /// still not below the kernel's zero ([`ErrorKind::StageFailed`]).
    JacobiDidNotConverge {
        /// The sweeps run.
        sweeps: usize,
        /// The off-diagonal sum measured at the last sweep.
        off_diagonal: f64,
    },
    /// No vector perpendicular to the given one was found. [`ErrorKind::Degenerate`].
    NoPerpendicular,
    /// A matrix the routine needs square is not. [`ErrorKind::InvalidInput`].
    NotSquare {
        /// The matrix's rows.
        rows: usize,
        /// The matrix's columns.
        cols: usize,
    },
    /// A matrix the routine needs symmetric is not. [`ErrorKind::InvalidInput`].
    NotSymmetric,
    /// A point to normalise lies outside the extent (or off a degenerate side of it).
    PointOutsideExtent {
        /// The point's `u`.
        u: f64,
        /// The point's `v`.
        v: f64,
    },
    /// A linear system or matrix is singular: its pivot or determinant, `measure`, is
    /// below the kernel's zero for it. [`ErrorKind::Degenerate`].
    Singular {
        /// The pivot or determinant that fell below the kernel's zero.
        measure: f64,
    },
    /// A span to add to a periodic extent does not lie in `[0, period]`.
    /// [`ErrorKind::InvalidInput`].
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
    /// ([`crate::series`]). [`ErrorKind::Degenerate`].
    VectorVanishes,
    /// A matrix to decompose by singular values is empty or has fewer rows than columns.
    /// [`ErrorKind::InvalidInput`].
    TooFewRows {
        /// The matrix's rows.
        rows: usize,
        /// The matrix's columns.
        cols: usize,
    },
}

impl NumFailure {
    /// The failure's class, as solidcore's `GeomFailure::kind`.
    #[must_use]
    pub const fn kind(&self) -> ErrorKind {
        match self {
            Self::IndexOutOfRange { .. }
            | Self::NotSquare { .. }
            | Self::NotSymmetric
            | Self::SpanOutsidePeriod { .. }
            | Self::TooFewRows { .. } => ErrorKind::InvalidInput,
            Self::JacobiDidNotConverge { .. } | Self::Defect { .. } => ErrorKind::StageFailed,
            Self::CallFailed { class } => *class,
            Self::NoPerpendicular | Self::Singular { .. } | Self::VectorVanishes => {
                ErrorKind::Degenerate
            }
            Self::Disjoint { .. } | Self::PointOutsideExtent { .. } => ErrorKind::Failed,
        }
    }
}

impl fmt::Display for NumFailure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::CallFailed { .. } => f.write_str("a routine it calls failed"),
            Self::Disjoint { a: [a0, a1], b: [b0, b1] } => {
                write!(f, "the intervals [{a0}, {a1}] and [{b0}, {b1}] are disjoint")
            }
            Self::Defect { why } => write!(f, "{}: {why}", ErrorKind::StageFailed),
            Self::IndexOutOfRange { index, len } => {
                write!(f, "the index {index} is not below the {len} entries")
            }
            Self::JacobiDidNotConverge {
                sweeps,
                off_diagonal,
            } => write!(
                f,
                "{}: the Jacobi sweeps did not converge after {sweeps} sweeps (off-diagonal sum {off_diagonal:e})",
                ErrorKind::StageFailed
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
            // As solidcore's `BlendFailure::SectionDegenerate`: its class, then what.
            Self::VectorVanishes => write!(
                f,
                "{}: a vector the section normalises vanishes (a face's normal at a foot, or the leader's tangent)",
                ErrorKind::Degenerate
            ),
            Self::TooFewRows { rows, cols } => write!(
                f,
                "the {rows}x{cols} matrix is empty or has fewer rows than columns"
            ),
        }
    }
}

/// A layer-0 error.
#[derive(Debug, Clone, PartialEq)]
pub enum NumError {
    /// An argument was out of range, malformed or inconsistent (upstream
    /// `SM_ERR_INVALID_INPUT`).
    InvalidInput {
        /// Operation that rejected the input.
        operation: &'static str,
        /// Human-readable description of what was wrong.
        reason: &'static str,
    },
    /// A computation could not go on (upstream `SM_ERR`).
    Failed {
        /// The solidcore path of the failing routine.
        operation: &'static str,
        /// What failed.
        failure: NumFailure,
        /// The callee's error, for [`NumFailure::CallFailed`].
        cause: Option<Box<NumError>>,
    },
}

impl NumError {
    /// [`NumError::InvalidInput`].
    #[must_use]
    pub const fn invalid_input(operation: &'static str, reason: &'static str) -> Self {
        Self::InvalidInput { operation, reason }
    }

    /// [`NumError::Failed`] of `operation` for `failure`, with no cause.
    #[must_use]
    pub const fn failed(operation: &'static str, failure: NumFailure) -> Self {
        Self::Failed {
            operation,
            failure,
            cause: None,
        }
    }

    /// A map from a callee's error to the failure of `operation` that keeps it as its
    /// cause ([`NumFailure::CallFailed`] with the callee's class). Use as
    /// `.map_err(NumError::call(OP))`.
    pub fn call(operation: &'static str) -> impl Fn(Self) -> Self {
        move |e| Self::Failed {
            operation,
            failure: NumFailure::CallFailed { class: e.kind() },
            cause: Some(Box::new(e)),
        }
    }

    /// The error's class.
    #[must_use]
    pub const fn kind(&self) -> ErrorKind {
        match self {
            Self::InvalidInput { .. } => ErrorKind::InvalidInput,
            Self::Failed { failure, .. } => failure.kind(),
        }
    }

    /// The operation that failed.
    #[must_use]
    pub const fn operation(&self) -> &'static str {
        match self {
            Self::InvalidInput { operation, .. } | Self::Failed { operation, .. } => operation,
        }
    }

    /// The typed failure, for [`NumError::Failed`].
    #[must_use]
    pub const fn failure(&self) -> Option<&NumFailure> {
        match self {
            Self::InvalidInput { .. } => None,
            Self::Failed { failure, .. } => Some(failure),
        }
    }

    /// The callee's error a [`NumFailure::CallFailed`] keeps.
    #[must_use]
    pub fn cause(&self) -> Option<&Self> {
        match self {
            Self::InvalidInput { .. } => None,
            Self::Failed { cause, .. } => cause.as_deref(),
        }
    }
}

impl fmt::Display for NumError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidInput { operation, reason } => {
                write!(f, "{operation}: invalid input: {reason}")
            }
            Self::Failed {
                operation,
                failure,
                cause,
            } => {
                write!(f, "{operation}: {failure}")?;
                if let Some(c) = cause {
                    write!(f, "; caused by {c}")?;
                }
                Ok(())
            }
        }
    }
}

impl std::error::Error for NumError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        self.cause()
            .map(|c| c as &(dyn std::error::Error + 'static))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_call_failure_takes_its_class_and_message_from_the_cause() {
        let inner = NumError::failed("a", NumFailure::Singular { measure: 0.0 });
        let outer = NumError::call("b")(inner.clone());
        assert_eq!(outer.kind(), ErrorKind::Degenerate);
        assert_eq!(outer.cause(), Some(&inner));
        assert_eq!(
            outer.to_string(),
            "b: a routine it calls failed; caused by a: the system is singular (its pivot or determinant is 0e0)"
        );
        let bad = NumError::invalid_input("c", "why");
        assert_eq!(bad.kind(), ErrorKind::InvalidInput);
        assert_eq!(bad.to_string(), "c: invalid input: why");
        assert_eq!(NumError::call("d")(bad).kind(), ErrorKind::InvalidInput);
    }

    /// Every variant's text, as the pin's `GeomFailure` / `BlendFailure` `Display` writes it
    /// (`error/geom.rs` and `error/blend.rs` at the pin), and the class names. The
    /// differential tests compare the texts of the variants public inputs reach; the pin
    /// builds its failures only inside its crate, so the rest are pinned here.
    #[test]
    fn every_variant_prints_the_pins_text() {
        let cases: [(NumFailure, ErrorKind, &str); 13] = [
            (
                NumFailure::CallFailed { class: ErrorKind::Failed },
                ErrorKind::Failed,
                "a routine it calls failed",
            ),
            (
                NumFailure::Disjoint { a: [1.0, 2.5], b: [-0.0, 3.0] },
                ErrorKind::Failed,
                "the intervals [1, 2.5] and [-0, 3] are disjoint",
            ),
            (
                NumFailure::Defect { why: "why" },
                ErrorKind::StageFailed,
                "stage_failed: why",
            ),
            (
                NumFailure::IndexOutOfRange { index: 4, len: 3 },
                ErrorKind::InvalidInput,
                "the index 4 is not below the 3 entries",
            ),
            (
                NumFailure::JacobiDidNotConverge { sweeps: 50, off_diagonal: 1.5e-3 },
                ErrorKind::StageFailed,
                "stage_failed: the Jacobi sweeps did not converge after 50 sweeps (off-diagonal sum 1.5e-3)",
            ),
            (
                NumFailure::NoPerpendicular,
                ErrorKind::Degenerate,
                "no perpendicular vector found",
            ),
            (
                NumFailure::NotSquare { rows: 2, cols: 3 },
                ErrorKind::InvalidInput,
                "the 2x3 matrix is not square",
            ),
            (
                NumFailure::NotSymmetric,
                ErrorKind::InvalidInput,
                "the matrix is not symmetric",
            ),
            (
                NumFailure::PointOutsideExtent { u: 0.5, v: -2.5 },
                ErrorKind::Failed,
                "the point (0.5, -2.5) lies outside the extent",
            ),
            (
                NumFailure::Singular { measure: 0.0 },
                ErrorKind::Degenerate,
                "the system is singular (its pivot or determinant is 0e0)",
            ),
            (
                NumFailure::SpanOutsidePeriod { from: -1.0, to: 7.0, period: 6.0 },
                ErrorKind::InvalidInput,
                "the span from -1 to 7 does not lie in the period [0, 6]",
            ),
            (
                NumFailure::VectorVanishes,
                ErrorKind::Degenerate,
                "degenerate: a vector the section normalises vanishes (a face's normal at a foot, or the leader's tangent)",
            ),
            (
                NumFailure::TooFewRows { rows: 1, cols: 2 },
                ErrorKind::InvalidInput,
                "the 1x2 matrix is empty or has fewer rows than columns",
            ),
        ];
        for (failure, class, text) in cases {
            assert_eq!(failure.kind(), class, "{text}");
            assert_eq!(failure.to_string(), text);
            let e = NumError::failed("op", failure);
            assert_eq!(e.failure(), Some(&failure));
            assert_eq!(e.to_string(), format!("op: {text}"));
            assert!(std::error::Error::source(&e).is_none());
        }
        let names: Vec<&str> = [
            ErrorKind::InvalidInput,
            ErrorKind::Degenerate,
            ErrorKind::StageFailed,
            ErrorKind::Failed,
        ]
        .into_iter()
        .map(ErrorKind::name)
        .collect();
        assert_eq!(
            names,
            ["invalid_input", "degenerate", "stage_failed", "failed"]
        );
        let caused = NumError::call("outer")(NumError::invalid_input("inner", "why"));
        assert!(std::error::Error::source(&caused).is_some());
        assert_eq!(NumError::invalid_input("x", "y").failure(), None);
    }
}
