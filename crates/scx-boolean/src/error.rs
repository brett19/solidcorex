// Written for solidcorex. Apache-2.0.
//! [`BooleanError`]: why a Boolean did not finish, by [`ErrorKind`].
//!
//! No variant carries a solver status number. Each names the operand and the entity it
//! failed at: ids are per operand, so an id alone does not say which body it is in.

use core::fmt;

use scx_brep::{EdgeId, Entity, FaceId, Operand, VertexId};
use scx_num::budget::{Exhausted, Stopped};
use scx_num::error::ErrorKind;
use scx_surface::SurfaceError;

/// Why a Boolean did not finish.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum BooleanError {
    /// The operands are not ones a Boolean takes.
    Refused(BooleanRefusal),
    /// The operands are valid and this method cannot answer for them yet.
    Unsupported(BooleanUnsupported),
    /// The budget ran out.
    Exhausted {
        /// The budget's limit.
        limit: u64,
    },
    /// The stop flag was set.
    Cancelled,
    /// The method failed on valid operands: a bug.
    Defect(BooleanDefect),
}

/// Why the operands were refused. Each is checkable before the call.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum BooleanRefusal {
    /// Two vertices of one operand lie within coincidence of each other where the other
    /// operand meets them: the body breaks vertex clearance there (the 11-27 merged-points
    /// row, read per operand as 72 §12.4 row 4 rules). The lower id first.
    VerticesWithinCoincidence {
        /// The operand.
        operand: Operand,
        /// The two vertices.
        vertices: [VertexId; 2],
    },
    /// The operand was finished under another resolution than the operation's document:
    /// it belongs to another document (72 §12.4 row 4: one operation, one document).
    OtherDocument {
        /// The operand.
        operand: Operand,
    },
}

/// What this method does not answer yet.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum BooleanUnsupported {
    /// A NURBS face or edge where the operands meet: step 5 builds their intersections.
    Nurbs {
        /// The operand.
        operand: Operand,
        /// The face or edge.
        at: Entity,
    },
    /// Two vertices of one operand, farther apart than coincidence, joined through a chain
    /// of points each within coincidence of the next: the merged point has no one operand
    /// vertex to be (the 11-27 merged-points row's chain case). The lower id first.
    Chain {
        /// The operand.
        operand: Operand,
        /// The two vertices.
        vertices: [VertexId; 2],
    },
    /// A stage of the method not built yet: the operands got past every stage before it.
    /// Each later task removes the stage it builds (72 §5.8).
    Stage(Stage),
}

/// A stage of the Boolean (72 §5.4), in pipeline order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[non_exhaustive]
pub enum Stage {
    /// Face against face: the intersection curves of each meeting pair (BO2).
    FaceFace,
}

/// The method's own failure on valid operands.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum BooleanDefect {
    /// A face of a finished operand with no box: it reaches infinity or cannot be enclosed.
    NoFaceBox {
        /// The operand.
        operand: Operand,
        /// The face.
        face: FaceId,
    },
    /// A face whose uv region has no box.
    NoUvBox {
        /// The operand.
        operand: Operand,
        /// The face.
        face: FaceId,
    },
    /// A surface refused an inversion or an intersection it should take.
    Surface {
        /// The operand.
        operand: Operand,
        /// The face.
        face: FaceId,
    },
    /// A face's boundary test could not place a uv point where an edge of the other
    /// operand meets it.
    UvUndecided {
        /// The operand.
        operand: Operand,
        /// The face.
        face: FaceId,
    },
    /// An edge of the other operand meets the face `Near` its boundary, yet no edge or
    /// vertex of the face lies within coincidence there: on a valid body the face's
    /// accuracy covers its boundary, so this is a bug in the boundary test or the hit.
    NearWithoutBoundary {
        /// The operand.
        operand: Operand,
        /// The face.
        face: FaceId,
    },
    /// An edge the method reads is missing from its operand: a bug.
    NoEdge {
        /// The operand.
        operand: Operand,
        /// The edge.
        edge: EdgeId,
    },
}

impl BooleanError {
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
            Self::Refused(BooleanRefusal::VerticesWithinCoincidence { .. }) => {
                "vertices_within_coincidence"
            }
            Self::Refused(BooleanRefusal::OtherDocument { .. }) => "other_document",
            Self::Unsupported(BooleanUnsupported::Nurbs { .. }) => "nurbs",
            Self::Unsupported(BooleanUnsupported::Chain { .. }) => "chain",
            Self::Unsupported(BooleanUnsupported::Stage(Stage::FaceFace)) => "stage_face_face",
            Self::Exhausted { .. } => "exhausted",
            Self::Cancelled => "cancelled",
            Self::Defect(BooleanDefect::NoFaceBox { .. }) => "no_face_box",
            Self::Defect(BooleanDefect::NoUvBox { .. }) => "no_uv_box",
            Self::Defect(BooleanDefect::Surface { .. }) => "surface",
            Self::Defect(BooleanDefect::UvUndecided { .. }) => "uv_undecided",
            Self::Defect(BooleanDefect::NearWithoutBoundary { .. }) => "near_without_boundary",
            Self::Defect(BooleanDefect::NoEdge { .. }) => "no_edge",
        }
    }

    /// The error a surface's inversion or intersection gave at `face` of `operand`.
    pub(crate) const fn of_surface(e: SurfaceError, operand: Operand, face: FaceId) -> Self {
        match e {
            SurfaceError::Exhausted(x) => Self::of_budget(x),
            SurfaceError::Stopped => Self::Cancelled,
            SurfaceError::NurbsIntersection => Self::Unsupported(BooleanUnsupported::Nurbs {
                operand,
                at: Entity::Face(face),
            }),
            _ => Self::Defect(BooleanDefect::Surface { operand, face }),
        }
    }

    /// The budget's exhaustion.
    pub(crate) const fn of_budget(e: Exhausted) -> Self {
        Self::Exhausted { limit: e.limit }
    }
}

impl From<Exhausted> for BooleanError {
    fn from(e: Exhausted) -> Self {
        Self::of_budget(e)
    }
}

impl From<Stopped> for BooleanError {
    fn from(_: Stopped) -> Self {
        Self::Cancelled
    }
}

impl fmt::Display for BooleanError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match *self {
            Self::Refused(BooleanRefusal::VerticesWithinCoincidence {
                operand,
                vertices: [v, w],
            }) => write!(f, "{operand}: {v} and {w} lie within coincidence"),
            Self::Refused(BooleanRefusal::OtherDocument { operand }) => {
                write!(f, "{operand}: finished under another document's resolution")
            }
            Self::Unsupported(BooleanUnsupported::Nurbs { operand, at }) => {
                write!(f, "{operand}: {at}: NURBS geometry is not intersected yet")
            }
            Self::Unsupported(BooleanUnsupported::Chain {
                operand,
                vertices: [v, w],
            }) => write!(f, "{operand}: {v} and {w} are joined by a chain of points"),
            Self::Unsupported(BooleanUnsupported::Stage(Stage::FaceFace)) => {
                f.write_str("the face-face stage is not built yet")
            }
            Self::Exhausted { limit } => write!(f, "the budget of {limit} ran out"),
            Self::Cancelled => f.write_str("stopped"),
            Self::Defect(BooleanDefect::NoFaceBox { operand, face }) => {
                write!(f, "{operand}: {face}: no box")
            }
            Self::Defect(BooleanDefect::NoUvBox { operand, face }) => {
                write!(f, "{operand}: {face}: no uv box")
            }
            Self::Defect(BooleanDefect::Surface { operand, face }) => {
                write!(f, "{operand}: {face}: the surface refused a query")
            }
            Self::Defect(BooleanDefect::UvUndecided { operand, face }) => {
                write!(
                    f,
                    "{operand}: {face}: the boundary test could not place a hit"
                )
            }
            Self::Defect(BooleanDefect::NearWithoutBoundary { operand, face }) => {
                write!(
                    f,
                    "{operand}: {face}: a hit near the boundary with no edge there"
                )
            }
            Self::Defect(BooleanDefect::NoEdge { operand, edge }) => {
                write!(f, "{operand}: {edge}: missing")
            }
        }
    }
}

impl core::error::Error for BooleanError {}
