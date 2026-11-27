// Written for solidcorex. Apache-2.0.
//! What `finish` refuses (01 §2.3): the draft comes back whole with the cause, so the
//! caller can repair it or report it.
//!
//! BR1 checks what its views rely on (every reference resolves, every fin has its use, the
//! outside region is there and not solid) and that every curve is attached. BR2 adds the
//! rest of the structure check (Euler, radial completeness, sides, the pole and seam rules)
//! as more [`StructureCause`]s, and the cheap geometry check as a `Geometry` cause.

use core::fmt;

use scx_num::error::ErrorKind;

use crate::draft::Draft;
use crate::id::{EdgeId, Entity};
use crate::record::Fin;

/// Why `finish` refused a draft.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub enum FinishCause {
    /// An edge's curve was never attached (63 D8). Only our own operations build such a
    /// draft: a caller's input without a curve is refused before `finish`.
    MissingCurve {
        /// The lowest such edge.
        edge: EdgeId,
    },
    /// The structure is not a body.
    Structure(StructureCause),
}

impl FinishCause {
    /// [`ErrorKind::Defect`] for a missing curve (D8); [`ErrorKind::Refused`] for a
    /// structure the draft's builder must change. An operation that reaches either from
    /// its own draft reports its own `Defect` (01 §6).
    #[must_use]
    pub const fn kind(&self) -> ErrorKind {
        match self {
            Self::MissingCurve { .. } => ErrorKind::Defect,
            Self::Structure(_) => ErrorKind::Refused,
        }
    }

    /// The stable name.
    #[must_use]
    pub const fn name(&self) -> &'static str {
        match self {
            Self::MissingCurve { .. } => "missing_curve",
            Self::Structure(cause) => cause.name(),
        }
    }
}

impl fmt::Display for FinishCause {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingCurve { edge } => write!(f, "edge {edge} has no curve"),
            Self::Structure(cause) => cause.fmt(f),
        }
    }
}

impl core::error::Error for FinishCause {}

/// How a draft's structure fails to be a body. The lowest failing id is named, so the
/// cause is the same on every run.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum StructureCause {
    /// The outside region is gone or solid.
    Outside,
    /// A record names an entity the draft does not hold.
    Dangling {
        /// The record.
        from: Entity,
        /// What it names.
        to: Entity,
    },
    /// An edge lists a fin whose face has no use of the edge in that sense.
    FinWithoutUse {
        /// The edge.
        edge: EdgeId,
        /// The fin.
        fin: Fin,
    },
}

impl StructureCause {
    /// The stable name.
    #[must_use]
    pub const fn name(&self) -> &'static str {
        match self {
            Self::Outside => "outside_region",
            Self::Dangling { .. } => "dangling_reference",
            Self::FinWithoutUse { .. } => "fin_without_use",
        }
    }
}

impl fmt::Display for StructureCause {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Outside => f.write_str("the outside region is missing or solid"),
            Self::Dangling { from, to } => write!(f, "{from} names {to}, which is not there"),
            Self::FinWithoutUse { edge, fin } => write!(
                f,
                "edge {edge} has a fin on face {} with no use in that sense",
                fin.face
            ),
        }
    }
}

/// A draft `finish` refused, with the cause.
#[derive(Debug, Clone)]
pub struct Unfinished {
    /// The draft, unchanged.
    pub draft: Draft,
    /// Why.
    pub cause: FinishCause,
}

impl Unfinished {
    /// The cause's kind.
    #[must_use]
    pub const fn kind(&self) -> ErrorKind {
        self.cause.kind()
    }

    /// The cause's name.
    #[must_use]
    pub const fn name(&self) -> &'static str {
        self.cause.name()
    }
}

impl fmt::Display for Unfinished {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "the draft is not a body: {}", self.cause)
    }
}

impl core::error::Error for Unfinished {
    fn source(&self) -> Option<&(dyn core::error::Error + 'static)> {
        Some(&self.cause)
    }
}
