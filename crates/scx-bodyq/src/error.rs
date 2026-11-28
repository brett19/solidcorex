// Written for solidcorex. Apache-2.0.
//! [`BodyqError`]: why a query did not answer, by [`ErrorKind`].
//!
//! No variant carries a solver status number; each names the entity it failed at.

use core::fmt;

use scx_brep::{EdgeId, Entity, FaceId, ShellId, VertexId};
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
    /// A shell the body does not have.
    NoShell {
        /// The shell asked for.
        shell: ShellId,
    },
    /// An entity the body does not have.
    NoEntity {
        /// The entity asked for.
        entity: Entity,
    },
    /// A body with no face, edge or vertex to box.
    EmptyBody,
    /// A frame so far from the body that its coordinates carry less than the body's
    /// coincidence: the section would be read at a precision it does not have
    /// (`prism_section`).
    FrameTooFar,
    /// A request option that is not a finite length above zero (`hole_stacks`).
    BadOption {
        /// Which option.
        name: &'static str,
    },
    /// A tolerance that is not finite or is negative, or an angle not below pi.
    Tolerance,
    /// An edge with fewer than two faces: nothing to compare across it.
    OneSided {
        /// The edge asked about.
        edge: EdgeId,
    },
    /// An edge collapsed to a point: there is no curve to run along.
    Collapsed {
        /// The edge asked about.
        edge: EdgeId,
    },
    /// A normal, axis or direction of no length (BW11).
    NoDirection,
    /// A radius, thickness or length that is not above zero (BW11).
    NotPositive,
    /// A point on the axis, where no direction leads away from it (BW11).
    OnAxis,
    /// A line that does not lie in the polygon's plane (BW11).
    OffPlane,
    /// An index past the corners, or a polygon with fewer than three (BW11).
    Corners,
    /// A vertex that is not an end of the edge asked about (BW11).
    NotAnEnd {
        /// The edge.
        edge: EdgeId,
        /// The vertex.
        vertex: VertexId,
    },
    /// A seat whose plane's axes span no plane (BW11).
    Seat,
    /// A face of a surface kind the question does not take (BW11): a plane or a cylinder
    /// was asked for.
    WrongSurface {
        /// The face.
        face: FaceId,
    },
    /// A face that does not separate material from air: both its sides are solid, or
    /// neither is, so it has no outward side (BW11).
    NoMaterialSide {
        /// The face.
        face: FaceId,
    },
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
    /// A probe's answer ([`Met::Unread`](crate::Met::Unread)) could not be read by the caller
    /// that made it (BW11).
    Probe,
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
    /// An edge whose curve over its interval cannot be enclosed: a value that is not
    /// finite.
    NoEdgeBox {
        /// The edge.
        edge: EdgeId,
    },
    /// The body could not be moved into a seat's frame (BW11): the move is a rigid map, so
    /// this is a bug.
    Moved,
    /// A shell whose cells give a negative first Betti number (`betti`).
    Betti {
        /// The shell.
        shell: ShellId,
    },
    /// A coincidence or accuracy that is not a finite length: a body whose recorded
    /// accuracy is not valid.
    Tolerance,
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
            Self::Refused(BodyqRefusal::NoShell { .. }) => "no_shell",
            Self::Refused(BodyqRefusal::NoEntity { .. }) => "no_entity",
            Self::Refused(BodyqRefusal::EmptyBody) => "empty_body",
            Self::Refused(BodyqRefusal::FrameTooFar) => "frame_too_far",
            Self::Refused(BodyqRefusal::BadOption { .. }) => "bad_option",
            Self::Refused(BodyqRefusal::Tolerance) => "tolerance",
            Self::Refused(BodyqRefusal::OneSided { .. }) => "one_sided",
            Self::Refused(BodyqRefusal::Collapsed { .. }) => "collapsed",
            Self::Refused(BodyqRefusal::NoDirection) => "no_direction",
            Self::Refused(BodyqRefusal::NotPositive) => "not_positive",
            Self::Refused(BodyqRefusal::OnAxis) => "on_axis",
            Self::Refused(BodyqRefusal::OffPlane) => "off_plane",
            Self::Refused(BodyqRefusal::Corners) => "corners",
            Self::Refused(BodyqRefusal::NotAnEnd { .. }) => "not_an_end",
            Self::Refused(BodyqRefusal::Seat) => "seat",
            Self::Refused(BodyqRefusal::WrongSurface { .. }) => "wrong_surface",
            Self::Refused(BodyqRefusal::NoMaterialSide { .. }) => "no_material_side",
            Self::Unsupported(BodyqUnsupported::Nurbs { .. }) => "nurbs",
            Self::Unsupported(BodyqUnsupported::NonManifold { .. }) => "non_manifold",
            Self::Unsupported(BodyqUnsupported::NoClearSide { .. }) => "no_clear_side",
            Self::Unsupported(BodyqUnsupported::Probe) => "probe",
            Self::Defect(BodyqDefect::Moved) => "moved",
            Self::Exhausted { .. } => "exhausted",
            Self::Cancelled => "cancelled",
            Self::Defect(BodyqDefect::NoFaceBox { .. }) => "no_face_box",
            Self::Defect(BodyqDefect::NoUvBox { .. }) => "no_uv_box",
            Self::Defect(BodyqDefect::Surface { .. }) => "surface",
            Self::Defect(BodyqDefect::NoNormal { .. }) => "no_normal",
            Self::Defect(BodyqDefect::Ray) => "ray",
            Self::Defect(BodyqDefect::NoBoundary) => "no_boundary",
            Self::Defect(BodyqDefect::NoEdgeBox { .. }) => "no_edge_box",
            Self::Defect(BodyqDefect::Betti { .. }) => "betti",
            Self::Defect(BodyqDefect::Tolerance) => "bad_accuracy",
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
            Self::Refused(BodyqRefusal::NoShell { shell }) => write!(f, "{shell}: no such shell"),
            Self::Refused(BodyqRefusal::NoEntity { entity }) => {
                write!(f, "{entity}: no such entity")
            }
            Self::Refused(BodyqRefusal::EmptyBody) => f.write_str("the body has nothing to box"),
            Self::Refused(BodyqRefusal::FrameTooFar) => {
                f.write_str("the frame is too far from the body for its coordinates to hold")
            }
            Self::Refused(BodyqRefusal::BadOption { name }) => {
                write!(f, "the option `{name}` is not a finite length above zero")
            }
            Self::Refused(BodyqRefusal::Tolerance) => {
                f.write_str("a tolerance is not finite or negative, or an angle is not below pi")
            }
            Self::Refused(BodyqRefusal::OneSided { edge }) => {
                write!(f, "{edge}: fewer than two faces meet there")
            }
            Self::Refused(BodyqRefusal::Collapsed { edge }) => {
                write!(f, "{edge}: collapsed to a point")
            }
            Self::Refused(BodyqRefusal::NoDirection) => {
                f.write_str("a normal, axis or direction has no length")
            }
            Self::Refused(BodyqRefusal::NotPositive) => {
                f.write_str("a radius, thickness or length is not above zero")
            }
            Self::Refused(BodyqRefusal::OnAxis) => f.write_str("the point lies on the axis"),
            Self::Refused(BodyqRefusal::OffPlane) => {
                f.write_str("the line is not in the polygon's plane")
            }
            Self::Refused(BodyqRefusal::Corners) => {
                f.write_str("an index past the corners, or fewer than three corners")
            }
            Self::Refused(BodyqRefusal::NotAnEnd { edge, vertex }) => {
                write!(f, "{vertex} is not an end of {edge}")
            }
            Self::Refused(BodyqRefusal::Seat) => f.write_str("the seat's plane axes span no plane"),
            Self::Refused(BodyqRefusal::WrongSurface { face }) => {
                write!(f, "{face}: not a surface of the kind asked for")
            }
            Self::Refused(BodyqRefusal::NoMaterialSide { face }) => {
                write!(
                    f,
                    "{face}: has no outward side, material is on both or neither"
                )
            }
            Self::Unsupported(BodyqUnsupported::Probe) => {
                f.write_str("what a probe met of the body was not read")
            }
            Self::Defect(BodyqDefect::Moved) => {
                f.write_str("the body could not be moved into the seat's frame")
            }
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
            Self::Defect(BodyqDefect::NoEdgeBox { edge }) => {
                write!(f, "{edge}: the curve cannot be enclosed")
            }
            Self::Defect(BodyqDefect::Tolerance) => {
                f.write_str("a coincidence or accuracy is not a finite length")
            }
            Self::Defect(BodyqDefect::Betti { shell }) => {
                write!(f, "{shell}: the cells give a negative first Betti number")
            }
            Self::Defect(BodyqDefect::NoBoundary) => {
                f.write_str("no boundary point inside the body's box")
            }
        }
    }
}

impl core::error::Error for BodyqError {}
