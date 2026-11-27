// Written for solidcorex. Apache-2.0.
//! [`BodyqError`]: why a query did not answer, by [`ErrorKind`].
//!
//! No variant carries a solver status number; each names the entity it failed at.

use core::fmt;

use scx_brep::{Entity, FaceId};
use scx_num::budget::{Exhausted, Stopped};
use scx_num::error::ErrorKind;
use scx_surface::SurfaceError;

/// Why a query did not answer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum BodyqError {
    /// The request is not one a body can answer.
    Refused(BodyqRefusal),
    /// The body is valid and this method cannot answer for it yet.
    Unsupported(BodyqUnsupported),
    /// The budget ran out.
    Exhausted {
        /// The budget's limit.
        limit: u64,
    },
    /// The stop flag was set.
    Cancelled,
    /// The query failed on a valid body: a bug.
    Defect(BodyqDefect),
}

/// Why a request was refused.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum BodyqRefusal {
    /// A point with a coordinate that is not finite.
    NotFinite,
}

/// What this method does not answer yet.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum BodyqUnsupported {
    /// A NURBS face or edge the point or a ray came near: step 5 builds their
    /// intersections and inversions.
    Nurbs {
        /// The face or edge.
        at: Entity,
    },
    /// No ray was clean and the nearest boundary point is an edge or vertex where more than
    /// two regions meet, or where one face does not separate the two that do: the normal
    /// cone does not name one region there.
    NonManifold {
        /// The edge or vertex.
        at: Entity,
    },
    /// No side of `face` could be read where `other` meets it (BQ2): the crossing is too
    /// shallow at every sample, the surfaces agree to second order across the curve or
    /// round the touch point, or neither has a normal there. The caller falls back to
    /// classifying a point.
    NoClearSide {
        /// The face whose sides were asked for.
        face: FaceId,
        /// The other body's face.
        other: FaceId,
    },
}

/// The query's own failure on a valid body.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum BodyqDefect {
    /// A face of a finished body with no box: it reaches infinity, or cannot be enclosed.
    NoFaceBox {
        /// The face.
        face: FaceId,
    },
    /// A face whose uv region has no box.
    NoUvBox {
        /// The face.
        face: FaceId,
    },
    /// A surface refused an inversion or an intersection with a ray it should take.
    Surface {
        /// The face.
        face: FaceId,
    },
    /// The nearest boundary point has no normal to read a side from (a face's foot where
    /// its surface has none, or a vertex whose faces' sectors have no area).
    NoNormal {
        /// The face, edge or vertex.
        at: Entity,
    },
    /// The ray or its tube from a finite point was refused by the curve or surface kinds.
    Ray,
    /// No boundary point was found for a point inside the body's box: a body whose faces
    /// do not enclose its box.
    NoBoundary,
}

impl BodyqError {
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
            Self::Refused(BodyqRefusal::NotFinite) => "not_finite",
            Self::Unsupported(BodyqUnsupported::Nurbs { .. }) => "nurbs",
            Self::Unsupported(BodyqUnsupported::NonManifold { .. }) => "non_manifold",
            Self::Unsupported(BodyqUnsupported::NoClearSide { .. }) => "no_clear_side",
            Self::Exhausted { .. } => "exhausted",
            Self::Cancelled => "cancelled",
            Self::Defect(BodyqDefect::NoFaceBox { .. }) => "no_face_box",
            Self::Defect(BodyqDefect::NoUvBox { .. }) => "no_uv_box",
            Self::Defect(BodyqDefect::Surface { .. }) => "surface",
            Self::Defect(BodyqDefect::NoNormal { .. }) => "no_normal",
            Self::Defect(BodyqDefect::Ray) => "ray",
            Self::Defect(BodyqDefect::NoBoundary) => "no_boundary",
        }
    }

    /// The error a surface's inversion or intersection gave at `face`.
    pub(crate) const fn of_surface(e: SurfaceError, face: FaceId) -> Self {
        match e {
            SurfaceError::Exhausted(x) => Self::of_budget(x),
            SurfaceError::Stopped => Self::Cancelled,
            SurfaceError::NurbsIntersection => Self::Unsupported(BodyqUnsupported::Nurbs {
                at: Entity::Face(face),
            }),
            _ => Self::Defect(BodyqDefect::Surface { face }),
        }
    }

    /// The budget's exhaustion.
    pub(crate) const fn of_budget(e: Exhausted) -> Self {
        Self::Exhausted { limit: e.limit }
    }
}

impl From<Exhausted> for BodyqError {
    fn from(e: Exhausted) -> Self {
        Self::of_budget(e)
    }
}

impl From<Stopped> for BodyqError {
    fn from(_: Stopped) -> Self {
        Self::Cancelled
    }
}

impl fmt::Display for BodyqError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match *self {
            Self::Refused(BodyqRefusal::NotFinite) => f.write_str("the point is not finite"),
            Self::Unsupported(BodyqUnsupported::Nurbs { at }) => {
                write!(f, "{at}: NURBS geometry is not queried yet")
            }
            Self::Unsupported(BodyqUnsupported::NonManifold { at }) => {
                write!(f, "{at}: the normal cone names no one region")
            }
            Self::Unsupported(BodyqUnsupported::NoClearSide { face, other }) => {
                write!(f, "{face}: no clear side where {other} meets it")
            }
            Self::Exhausted { limit } => write!(f, "the budget of {limit} ran out"),
            Self::Cancelled => f.write_str("stopped"),
            Self::Defect(BodyqDefect::NoFaceBox { face }) => write!(f, "{face}: no box"),
            Self::Defect(BodyqDefect::NoUvBox { face }) => write!(f, "{face}: no uv box"),
            Self::Defect(BodyqDefect::Surface { face }) => {
                write!(f, "{face}: the surface refused a query")
            }
            Self::Defect(BodyqDefect::NoNormal { at }) => {
                write!(f, "{at}: no normal at the nearest point")
            }
            Self::Defect(BodyqDefect::Ray) => f.write_str("a ray from the point was refused"),
            Self::Defect(BodyqDefect::NoBoundary) => {
                f.write_str("no boundary point inside the body's box")
            }
        }
    }
}

impl core::error::Error for BodyqError {}
