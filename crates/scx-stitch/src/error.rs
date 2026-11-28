// Written for solidcorex. Apache-2.0.
//! [`StitchError`]: why a stitch did not finish, by [`ErrorKind`].
//!
//! Each cause names the operand (the sheet's place in the input) and the entity, since ids
//! are per operand. No variant carries a solver status number.

use core::fmt;

use scx_brep::{
    EdgeId, Entity, FaceId, FinishCause, Operand, PcurveWriteError, TableFull, VertexId,
};
use scx_num::budget::{Exhausted, Stopped};
use scx_num::error::ErrorKind;

/// Why a stitch did not finish.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub enum StitchError {
    /// The sheets are not ones a stitch takes.
    Refused(StitchRefusal),
    /// The sheets are valid and this method cannot answer for them yet.
    Unsupported(StitchUnsupported),
    /// The budget ran out.
    Exhausted {
        /// The budget's limit.
        limit: u64,
    },
    /// The stop flag was set.
    Cancelled,
    /// The method failed on valid sheets: a bug.
    Defect(StitchDefect),
}

/// Why the sheets were refused. Each is checkable before the call.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum StitchRefusal {
    /// Two vertices of one sheet lie within coincidence of each other: the sheet breaks
    /// vertex clearance (04's merged-points row, per operand as its step-4 amendment rules),
    /// and the check reports the same pair. The lower id first.
    VerticesWithinCoincidence {
        /// The sheet.
        operand: Operand,
        /// The two vertices.
        vertices: [VertexId; 2],
    },
    /// The sheet was finished under another resolution than the operation's document: it
    /// belongs to another document (one operation, one document).
    OtherDocument {
        /// The sheet.
        operand: Operand,
    },
}

/// What this method does not answer yet.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum StitchUnsupported {
    /// Two vertices of one sheet, farther apart than coincidence, joined through a chain of
    /// vertices of other sheets, where anchoring each of those to its nearest vertex of the
    /// sheet does not settle it: a vertex of the chain lies beyond coincidence of its
    /// nearest anchor, two vertices of one other sheet anchor to one vertex, or more than
    /// one sheet has two vertices in the chain (04's merged-points row, chain case). The
    /// lower id first.
    Chain {
        /// The sheet.
        operand: Operand,
        /// The two vertices.
        vertices: [VertexId; 2],
    },
    /// A join or split the edge stage proved but cannot make yet; the cause says which part
    /// waits. The sheet edge named is the one cut or kept (for a piece, the edge it came
    /// from).
    Edges {
        /// The sheet.
        operand: Operand,
        /// The edge.
        edge: EdgeId,
        /// What waits.
        why: EdgePending,
    },
    /// A closed result ([`Want::Solid`](crate::Want::Solid)): it needs orientation, not
    /// built yet.
    Solid,
    /// More sheets than an [`Operand`] numbers, or more entities than a body's ids hold.
    TooLarge,
}

/// The part of the edge stage a proved join or split waits on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum EdgePending {
    /// The cut point on a NURBS edge: its inversion waits on step 5's solve.
    NurbsFoot,
    /// Two vertices of other sheets meet the edge at one parameter: the piece between
    /// them would have no length.
    SameFoot,
    /// The join would leave the edge with more than two fins, whose radial order the
    /// stitch does not build yet.
    Radial,
    /// A face of the absorbed edge has no exact pcurve of the kept edge's curve (it needs
    /// the fitter, BR9), or the curve lies beyond the edge's reach of the face.
    Fit,
    /// Two closed edges at one vertex whose start directions do not decide whether they
    /// run the same way.
    Direction,
    /// A wire edge (no face uses it) to join or cut: the stitch joins faces' edges.
    Wire,
    /// A vertex within coincidence of the edge whose nearest point is at or past an end
    /// it is not identified with: the edges' ends meet within coincidence where the
    /// vertices are farther apart, which needs the vertex merge reopened.
    NearEnd,
    /// The edges' faces bound different regions: joining them needs the orientation
    /// step (ST3).
    Region,
}

impl EdgePending {
    /// A stable name.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::NurbsFoot => "nurbs_foot",
            Self::SameFoot => "same_foot",
            Self::Radial => "radial",
            Self::Fit => "fit",
            Self::Direction => "direction",
            Self::Wire => "wire",
            Self::NearEnd => "near_end",
            Self::Region => "region",
        }
    }
}

/// The method's own failure on valid sheets.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub enum StitchDefect {
    /// The draft refused an entity or a lineage record the stitch wrote, or lost one it
    /// read.
    Draft,
    /// A finished sheet's entity could not be boxed, or a query refused it (a gap of a
    /// finite edge that could not be enclosed among them).
    Query {
        /// The sheet.
        operand: Operand,
        /// The entity.
        at: Entity,
    },
    /// The pcurve writer refused a face the stitch rerouted onto a kept edge, for a cause
    /// other than [`EdgePending::Fit`]: a loop it could not place in uv.
    Pcurve {
        /// The result's face.
        face: FaceId,
        /// The writer's cause.
        cause: PcurveWriteError,
    },
    /// The stitched draft did not finish: the identification left an end off its vertex,
    /// or a rewritten loop did not close.
    Finish(Box<FinishCause>),
}

impl StitchError {
    /// The kind.
    #[must_use]
    pub const fn kind(&self) -> ErrorKind {
        match self {
            Self::Refused(_) => ErrorKind::Refused,
            Self::Unsupported(_) => ErrorKind::Unsupported,
            Self::Exhausted { .. } => ErrorKind::Exhausted,
            Self::Cancelled => ErrorKind::Cancelled,
            Self::Defect(_) => ErrorKind::Defect,
        }
    }

    /// A stable name.
    #[must_use]
    pub const fn name(&self) -> &'static str {
        match self {
            Self::Refused(StitchRefusal::VerticesWithinCoincidence { .. }) => {
                "vertices_within_coincidence"
            }
            Self::Refused(StitchRefusal::OtherDocument { .. }) => "other_document",
            Self::Unsupported(StitchUnsupported::Chain { .. }) => "chain",
            Self::Unsupported(StitchUnsupported::Edges { .. }) => "edges_pending",
            Self::Unsupported(StitchUnsupported::Solid) => "solid_pending",
            Self::Unsupported(StitchUnsupported::TooLarge) => "too_large",
            Self::Exhausted { .. } => "exhausted",
            Self::Cancelled => "cancelled",
            Self::Defect(StitchDefect::Draft) => "draft",
            Self::Defect(StitchDefect::Query { .. }) => "query",
            Self::Defect(StitchDefect::Pcurve { .. }) => "pcurve",
            Self::Defect(StitchDefect::Finish(_)) => "finish",
        }
    }
}

/// A full table: more entities than a body's ids hold.
pub(crate) const fn too_large(_: TableFull) -> StitchError {
    StitchError::Unsupported(StitchUnsupported::TooLarge)
}

/// The draft refused a lineage record or lost an entity the stitch read: a bug.
pub(crate) fn draft_defect<E>(_: E) -> StitchError {
    StitchError::Defect(StitchDefect::Draft)
}

impl From<Exhausted> for StitchError {
    fn from(e: Exhausted) -> Self {
        Self::Exhausted { limit: e.limit }
    }
}

impl From<Stopped> for StitchError {
    fn from(_: Stopped) -> Self {
        Self::Cancelled
    }
}

impl fmt::Display for StitchError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Refused(StitchRefusal::VerticesWithinCoincidence {
                operand,
                vertices: [v, w],
            }) => write!(f, "{operand}: {v} and {w} lie within coincidence"),
            Self::Refused(StitchRefusal::OtherDocument { operand }) => {
                write!(f, "{operand}: finished under another document's resolution")
            }
            Self::Unsupported(StitchUnsupported::Chain {
                operand,
                vertices: [v, w],
            }) => write!(
                f,
                "{operand}: {v} and {w} are joined by a chain of vertices"
            ),
            Self::Unsupported(StitchUnsupported::Edges { operand, edge, why }) => {
                write!(
                    f,
                    "{operand}: {edge}: a proved join or split waits on {}",
                    why.name()
                )
            }
            Self::Unsupported(StitchUnsupported::Solid) => {
                f.write_str("a closed result needs orientation, not built yet")
            }
            Self::Unsupported(StitchUnsupported::TooLarge) => {
                f.write_str("more sheets or entities than ids hold")
            }
            Self::Exhausted { limit } => write!(f, "the budget of {limit} ran out"),
            Self::Cancelled => f.write_str("stopped"),
            Self::Defect(StitchDefect::Draft) => f.write_str("the draft refused a record"),
            Self::Defect(StitchDefect::Query { operand, at }) => {
                write!(f, "{operand}: {at}: a query failed on a finished sheet")
            }
            Self::Defect(StitchDefect::Pcurve { face, .. }) => {
                write!(f, "{face}: the pcurve writer refused the rerouted face")
            }
            Self::Defect(StitchDefect::Finish(cause)) => {
                write!(f, "the stitched body did not finish: {cause}")
            }
        }
    }
}

impl std::error::Error for StitchError {}
