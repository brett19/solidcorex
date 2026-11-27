// Written for solidcorex. Apache-2.0.
//! [`ContractError`]: why a body was not proved, by [`ErrorKind`].
//!
//! No variant carries a status number; each names the entity it failed at.

use core::fmt;

use scx_brep::{EdgeId, FaceId, ShellId, VertexId};
use scx_mass::MassError;
use scx_num::error::ErrorKind;

/// Why [`contract`](crate::contract) did not prove a body.
#[derive(Debug, Clone, Copy, PartialEq)]
#[non_exhaustive]
pub enum ContractError {
    /// The body breaks the contract.
    Broken(Breach),
    /// The face's loops do not sign its region: a sliver whose enclosed area the
    /// enclosures cannot tell from zero, or a loop that does not close in uv.
    Undecided {
        /// The face.
        face: FaceId,
    },
    /// The certified volume sign did not finish (its kind says why).
    Mass(MassError),
}

/// The test a body fails.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum Breach {
    /// The body has no face.
    Empty,
    /// Both sides of the face are solid, or neither is: a sheet, or a face inside or
    /// between solids.
    FaceNotBounding {
        /// The face.
        face: FaceId,
    },
    /// The edge does not have exactly two fins: a wire edge (none), a sheet's border (one)
    /// or an edge where solids meet (more).
    NonManifoldEdge {
        /// The edge.
        edge: EdgeId,
        /// Its fins.
        fins: usize,
    },
    /// An edge of an even number of fins over two, which [`Contacts::Radial`](crate::Contacts)
    /// admits only in strict radial order with solid and void sectors by turns, does not
    /// keep it: its fins are out of order, two are tangent, or two solid (or two void)
    /// sectors are adjacent.
    EdgeNotRadial {
        /// The edge.
        edge: EdgeId,
    },
    /// A shell holds a vertex on no edge.
    LoneVertex {
        /// The vertex.
        vertex: VertexId,
    },
    /// The face has no box: its region is the unbounded rest of its surface.
    FaceUnbounded {
        /// The face.
        face: FaceId,
    },
    /// The vertex's faces fall into more than one piece: two solids or sheets meet at it
    /// (or a face touches itself there) and no edge joins them.
    VertexNotManifold {
        /// The vertex.
        vertex: VertexId,
        /// The pieces round it, at least two.
        pieces: usize,
    },
    /// The shell faces the wrong way: a positive shell of the outside, or the shell of a
    /// solid region with no positive shell.
    NotOutward {
        /// The shell.
        shell: ShellId,
    },
}

impl ContractError {
    /// The kind.
    #[must_use]
    pub const fn kind(&self) -> ErrorKind {
        match self {
            Self::Broken(_) => ErrorKind::Refused,
            Self::Undecided { .. } => ErrorKind::Unsupported,
            Self::Mass(e) => e.kind(),
        }
    }

    /// A stable name.
    #[must_use]
    pub const fn name(&self) -> &'static str {
        match self {
            Self::Broken(b) => b.name(),
            Self::Undecided { .. } => "region_undecided",
            Self::Mass(e) => e.name(),
        }
    }
}

impl Breach {
    /// A stable name.
    #[must_use]
    pub const fn name(&self) -> &'static str {
        match self {
            Self::Empty => "empty",
            Self::FaceNotBounding { .. } => "face_not_bounding",
            Self::NonManifoldEdge { .. } => "non_manifold_edge",
            Self::EdgeNotRadial { .. } => "edge_not_radial",
            Self::LoneVertex { .. } => "lone_vertex",
            Self::FaceUnbounded { .. } => "face_unbounded",
            Self::VertexNotManifold { .. } => "vertex_not_manifold",
            Self::NotOutward { .. } => "not_outward",
        }
    }
}

impl fmt::Display for Breach {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match *self {
            Self::Empty => f.write_str("the body has no face"),
            Self::FaceNotBounding { face } => {
                write!(f, "{face}: it does not lie between a solid and a void")
            }
            Self::NonManifoldEdge { edge, fins } => {
                write!(f, "{edge}: {fins} fins where a manifold edge has 2")
            }
            Self::EdgeNotRadial { edge } => {
                write!(f, "{edge}: its fins are not in strict radial order")
            }
            Self::LoneVertex { vertex } => write!(f, "{vertex}: a vertex on no edge"),
            Self::FaceUnbounded { face } => {
                write!(f, "{face}: no box, its region is unbounded")
            }
            Self::VertexNotManifold { vertex, pieces } => {
                write!(f, "{vertex}: its faces fall into {pieces} pieces")
            }
            Self::NotOutward { shell } => {
                write!(f, "{shell}: its faces do not face out of its region")
            }
        }
    }
}

impl fmt::Display for ContractError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Broken(b) => b.fmt(f),
            Self::Undecided { face } => {
                write!(
                    f,
                    "{face}: the loops' enclosed area does not sign its region"
                )
            }
            Self::Mass(e) => e.fmt(f),
        }
    }
}

impl std::error::Error for ContractError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Mass(e) => Some(e),
            Self::Broken(_) | Self::Undecided { .. } => None,
        }
    }
}

/// Why [`check`](crate::check) did not finish. A body's defects are not errors: they are the
/// [`Findings`](crate::Findings).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum CheckError {
    /// The budget ran out.
    Exhausted {
        /// The budget's limit.
        limit: u64,
    },
    /// The stop flag was set.
    Cancelled,
}

impl CheckError {
    /// The kind.
    #[must_use]
    pub const fn kind(&self) -> ErrorKind {
        match self {
            Self::Exhausted { .. } => ErrorKind::Exhausted,
            Self::Cancelled => ErrorKind::Cancelled,
        }
    }

    /// A stable name.
    #[must_use]
    pub const fn name(&self) -> &'static str {
        match self {
            Self::Exhausted { .. } => "budget_exhausted",
            Self::Cancelled => "stopped",
        }
    }
}

impl fmt::Display for CheckError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Exhausted { limit } => write!(f, "the work budget of {limit} units ran out"),
            Self::Cancelled => f.write_str("the caller stopped the check"),
        }
    }
}

impl std::error::Error for CheckError {}
