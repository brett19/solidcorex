// Written for solidcorex.
//! Why a read refuses a file. A body that cannot be read is not an error: it is
//! [`Skipped`](crate::Skipped) with its reason, and the rest of the file is read.

use core::fmt;

use scx_num::error::{ErrorKind, chain};
use scx_p21::P21Error;

use crate::units::MissingUnits;

/// Work a read option asks for that a later task builds.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum Pending {
    /// [`ReadOptions::closing_gaps`](crate::ReadOptions::closing_gaps): gap closing through
    /// `scx-stitch` (SP4).
    CloseGaps,
}

impl Pending {
    /// The cause's stable name.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::CloseGaps => "close_gaps_pending",
        }
    }
}

/// Why a file cannot be read.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum StepError {
    /// A representation that lists a body has a context that names no length unit, or no plane
    /// angle unit, and the caller assumed none for what is missing.
    NoUnits {
        /// The first such representation (entity id), in id order.
        representation: u64,
        /// What its context lacks.
        missing: MissingUnits,
    },
    /// A representation context names two different length units, or two different plane angle
    /// units, so the file does not say which one its numbers are in.
    ConflictingUnits {
        /// The context (entity id).
        context: u64,
        /// Which unit is named twice.
        which: MissingUnits,
    },
    /// The units the caller assumed are not finite and positive.
    BadAssumedUnits,
    /// The file's product structure cannot be read.
    Structure(Box<P21Error>),
    /// An option asks for work a later task builds.
    Pending(Pending),
}

impl StepError {
    /// The class of the error (01 §5).
    #[must_use]
    pub fn kind(&self) -> ErrorKind {
        match self {
            Self::NoUnits { .. } | Self::ConflictingUnits { .. } | Self::BadAssumedUnits => {
                ErrorKind::Refused
            }
            Self::Structure(e) => e.kind(),
            Self::Pending(_) => ErrorKind::Unsupported,
        }
    }

    /// The stable name of the cause, unique in the crate: with the kind, the error's signature.
    #[must_use]
    pub fn name(&self) -> &'static str {
        match self {
            Self::NoUnits { .. } => "no_units",
            Self::ConflictingUnits { .. } => "conflicting_units",
            Self::BadAssumedUnits => "bad_assumed_units",
            Self::Structure(e) => e.name(),
            Self::Pending(p) => p.name(),
        }
    }

    /// The error with its causes on one line.
    pub fn one_line(&self) -> impl fmt::Display + '_ {
        chain(self)
    }
}

impl fmt::Display for StepError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NoUnits {
                representation,
                missing,
            } => write!(
                f,
                "the representation context of #{representation} names no {}; \
                 assume units explicitly to read it",
                missing.describe()
            ),
            Self::ConflictingUnits { context, which } => write!(
                f,
                "the representation context #{context} names two different {}",
                which.describe_plural()
            ),
            Self::BadAssumedUnits => f.write_str("the assumed units are not finite and positive"),
            Self::Structure(_) => f.write_str("the product structure cannot be read"),
            Self::Pending(Pending::CloseGaps) => {
                f.write_str("closing gaps through stitching is not built yet")
            }
        }
    }
}

impl std::error::Error for StepError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Structure(e) => Some(e.as_ref()),
            _ => None,
        }
    }
}

impl From<P21Error> for StepError {
    fn from(e: P21Error) -> Self {
        Self::Structure(Box::new(e))
    }
}
