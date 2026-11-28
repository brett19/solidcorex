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
    /// A face named to merge that the body does not hold ([`merge_faces`](crate::merge_faces)).
    NoSuchFace {
        /// The face.
        face: FaceId,
    },
    /// A join of two edges whose faces lie in different regions: one shell cannot bound
    /// both, which only a face inside a solid operand gives. Each sheet with its edge.
    RegionsMeet {
        /// The edge kept, then the edge absorbed.
        edges: [(Operand, EdgeId); 2],
    },
    /// The stitched sheets do not bound a solid ([`Want::Solid`](crate::Want::Solid)).
    NotSolid {
        /// The sheet the entity is of.
        operand: Operand,
        /// The sheet entity: an edge for [`SolidFault::Open`], [`SolidFault::Wire`] and
        /// [`SolidFault::NotOrientable`], a vertex for [`SolidFault::LoneVertex`].
        at: Entity,
        /// Why.
        why: SolidFault,
    },
}

/// Why stitched sheets do not bound a solid.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum SolidFault {
    /// An edge left with one fin (or three): the sheets do not close there.
    Open,
    /// A wire edge: no face uses it.
    Wire,
    /// A vertex no edge uses.
    LoneVertex,
    /// The faces round the edge cannot all have the material on one side: the closed
    /// sheet is one-sided (a Klein bottle's kind), or a face meets itself in one sense.
    NotOrientable,
}

impl SolidFault {
    /// A stable name.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Open => "open",
            Self::Wire => "wire",
            Self::LoneVertex => "lone_vertex",
            Self::NotOrientable => "not_orientable",
        }
    }
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
    /// A sheet given for [`Want::Solid`](crate::Want::Solid) that is already (part of) a
    /// solid: a region other than the outside, or a face with two shells. The stitch
    /// orients sheets; merging solids is a boolean.
    SolidOperand {
        /// The sheet.
        operand: Operand,
    },
    /// An open edge that the edge stage left undecided (a test of it straddled the
    /// coincidence at its budget share): a larger budget may close it.
    Undecided {
        /// The sheet.
        operand: Operand,
        /// The sheet edge.
        edge: EdgeId,
    },
    /// The certified volume sign of a closed sheet, which says which side its material is
    /// on, waits on the volume integral; the face is the sheet face it names.
    Sign {
        /// The sheet.
        operand: Operand,
        /// The face.
        face: FaceId,
        /// What waits.
        why: SignPending,
    },
    /// Whether one closed sheet lies inside another could not be read yet.
    Nesting {
        /// The sheet of the inner closed sheet.
        operand: Operand,
        /// The inner closed sheet's least face.
        at: Entity,
        /// What waits.
        why: NestingPending,
    },
    /// A body with a wire or a lone vertex given to [`sew`](crate::sew): the sheets the
    /// body is cut into hold faces only, and a wire or a lone vertex would be lost.
    Loose {
        /// The wire edge or the lone vertex.
        at: Entity,
    },
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
        }
    }
}

/// What a closed sheet's volume sign waits on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum SignPending {
    /// The enclosure still held zero at its cell cap: a gap between faces as large as
    /// the volume, or a volume below the rounding.
    Undecided,
    /// A face kind with no series form for the enclosure.
    NoSeries,
    /// A loop winding round a period, which the boundary integral does not close yet.
    Winding,
    /// A face with no box.
    NoBox,
}

impl SignPending {
    /// A stable name.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Undecided => "undecided",
            Self::NoSeries => "no_series",
            Self::Winding => "winding",
            Self::NoBox => "no_box",
        }
    }
}

/// What reading one closed sheet as inside another waits on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum NestingPending {
    /// The classifying ray came near a NURBS face or edge: step 5's ray.
    Nurbs,
    /// The point classifier could not name a region for another cause (no clean ray at a
    /// non-manifold point).
    Unclear,
    /// Every vertex of the inner sheet lies on the outer one, or the containers do not
    /// nest: the sheets touch.
    Touching,
}

impl NestingPending {
    /// A stable name.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Nurbs => "nurbs",
            Self::Unclear => "unclear",
            Self::Touching => "touching",
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
            Self::Refused(StitchRefusal::NoSuchFace { .. }) => "no_such_face",
            Self::Refused(StitchRefusal::RegionsMeet { .. }) => "regions_meet",
            Self::Refused(StitchRefusal::NotSolid { .. }) => "not_solid",
            Self::Unsupported(StitchUnsupported::Chain { .. }) => "chain",
            Self::Unsupported(StitchUnsupported::Edges { .. }) => "edges_pending",
            Self::Unsupported(StitchUnsupported::SolidOperand { .. }) => "solid_operand",
            Self::Unsupported(StitchUnsupported::Undecided { .. }) => "undecided",
            Self::Unsupported(StitchUnsupported::Sign { .. }) => "sign_pending",
            Self::Unsupported(StitchUnsupported::Nesting { .. }) => "nesting_pending",
            Self::Unsupported(StitchUnsupported::Loose { .. }) => "loose",
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
            Self::Refused(StitchRefusal::NoSuchFace { face }) => {
                write!(f, "{face}: the body holds no such face")
            }
            Self::Refused(StitchRefusal::RegionsMeet {
                edges: [(o0, e0), (o1, e1)],
            }) => write!(
                f,
                "{o0}: {e0} and {o1}: {e1}: their faces lie in different regions"
            ),
            Self::Refused(StitchRefusal::NotSolid { operand, at, why }) => write!(
                f,
                "{operand}: {at}: the sheets do not bound a solid ({})",
                why.name()
            ),
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
            Self::Unsupported(StitchUnsupported::SolidOperand { operand }) => {
                write!(f, "{operand}: already a solid, not a sheet to orient")
            }
            Self::Unsupported(StitchUnsupported::Undecided { operand, edge }) => {
                write!(f, "{operand}: {edge}: open, its join left undecided")
            }
            Self::Unsupported(StitchUnsupported::Sign { operand, face, why }) => write!(
                f,
                "{operand}: {face}: the closed sheet's volume sign waits on {}",
                why.name()
            ),
            Self::Unsupported(StitchUnsupported::Nesting { operand, at, why }) => write!(
                f,
                "{operand}: {at}: the closed sheet's nesting waits on {}",
                why.name()
            ),
            Self::Unsupported(StitchUnsupported::Loose { at }) => {
                write!(f, "{at}: a wire or lone vertex, which sewing would lose")
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
