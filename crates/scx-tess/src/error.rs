// Written for solidcorex. Apache-2.0.
//! [`TessError`]: why a tessellation did not finish, by [`ErrorKind`].
//!
//! No variant carries a string; each names the value or entity it failed at.

use core::fmt;

use scx_brep::{EdgeId, FaceId};
use scx_num::error::ErrorKind;
use scx_surface::{CurveKind, SurfaceKind};

/// Why a tessellation did not finish.
#[derive(Debug, Clone, Copy, PartialEq)]
#[non_exhaustive]
pub enum TessError {
    /// The request is not one a tessellation can answer.
    Refused(TessRefusal),
    /// The body is valid and this mesher cannot bound it yet.
    Unsupported(TessUnsupported),
    /// The budget ran out.
    Exhausted {
        /// The budget's limit.
        limit: u64,
    },
    /// The stop flag was set.
    Cancelled,
    /// The mesher failed on a valid body: a bug.
    Defect(TessDefect),
}

/// Why a request was refused.
#[derive(Debug, Clone, Copy, PartialEq)]
#[non_exhaustive]
pub enum TessRefusal {
    /// A chord that is not finite or not above the resolution's zone.
    Chord {
        /// The value asked for, in mm.
        value: f64,
    },
    /// A turning bound that is not finite or not above zero.
    Angle {
        /// The value asked for, in radians.
        value: f64,
    },
    /// A longest segment that is not finite or not above the resolution's zone.
    EdgeLength {
        /// The value asked for, in mm.
        value: f64,
    },
    /// A body of another resolution than the spec was checked against.
    Resolution {
        /// The spec's zone, in mm.
        spec: f64,
        /// The body's zone, in mm.
        body: f64,
    },
    /// An edge would need more segments than a mesh can index.
    TooFine {
        /// The edge.
        edge: EdgeId,
        /// The segments it would need.
        needed: f64,
    },
    /// A face id that is not a live face of the body.
    NoFace {
        /// The id.
        face: FaceId,
    },
}

/// What this mesher does not bound yet.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum TessUnsupported {
    /// An edge on a curve kind with no sampling rule (a parabola, a hyperbola).
    EdgeKind {
        /// The edge.
        edge: EdgeId,
        /// Its curve's kind.
        kind: CurveKind,
    },
    /// A face on a surface kind with no curvature bound (a NURBS surface before step 5).
    FaceKind {
        /// The face.
        face: FaceId,
        /// Its surface's kind.
        kind: SurfaceKind,
    },
    /// A NURBS edge whose derivative enclosures bound no count: a speed that may vanish, or
    /// an enclosure that is not finite.
    NoBound {
        /// The edge.
        edge: EdgeId,
    },
}

/// The mesher's own failure on a valid body.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum TessDefect {
    /// The closed-form count missed the chord in its own check: a rounding the count's
    /// margin did not cover.
    Sagitta {
        /// The edge.
        edge: EdgeId,
    },
    /// An edge's end vertex is not among the body's live vertices.
    Unindexed {
        /// The edge.
        edge: EdgeId,
    },
    /// Two of a face's boundary segments cross in its `uv`: two edges closer than their
    /// chords, or a face whose loops cross.
    Crossing {
        /// The face.
        face: FaceId,
    },
    /// Two of a face's boundary points coincide in its `uv`.
    Duplicate {
        /// The face.
        face: FaceId,
    },
    /// The triangulation lost its own structure, or its constraints bound no region.
    Triangulation {
        /// The face.
        face: FaceId,
    },
}

impl TessError {
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
            Self::Refused(TessRefusal::Chord { .. }) => "chord",
            Self::Refused(TessRefusal::Angle { .. }) => "angle",
            Self::Refused(TessRefusal::EdgeLength { .. }) => "edge_length",
            Self::Refused(TessRefusal::Resolution { .. }) => "resolution",
            Self::Refused(TessRefusal::TooFine { .. }) => "too_fine",
            Self::Refused(TessRefusal::NoFace { .. }) => "no_face",
            Self::Unsupported(TessUnsupported::EdgeKind { .. }) => "edge_kind",
            Self::Unsupported(TessUnsupported::FaceKind { .. }) => "face_kind",
            Self::Unsupported(TessUnsupported::NoBound { .. }) => "no_bound",
            Self::Exhausted { .. } => "exhausted",
            Self::Cancelled => "cancelled",
            Self::Defect(TessDefect::Sagitta { .. }) => "sagitta",
            Self::Defect(TessDefect::Unindexed { .. }) => "unindexed",
            Self::Defect(TessDefect::Crossing { .. }) => "crossing",
            Self::Defect(TessDefect::Duplicate { .. }) => "duplicate",
            Self::Defect(TessDefect::Triangulation { .. }) => "triangulation",
        }
    }
}

impl fmt::Display for TessError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match *self {
            Self::Refused(TessRefusal::Chord { value }) => {
                write!(f, "chord {value} mm is not finite and above the resolution")
            }
            Self::Refused(TessRefusal::Angle { value }) => {
                write!(f, "angle {value} rad is not finite and above zero")
            }
            Self::Refused(TessRefusal::EdgeLength { value }) => {
                write!(
                    f,
                    "edge length {value} mm is not finite and above the resolution"
                )
            }
            Self::Refused(TessRefusal::Resolution { spec, body }) => {
                write!(
                    f,
                    "the spec is for resolution {spec} mm and the body's is {body} mm"
                )
            }
            Self::Refused(TessRefusal::TooFine { edge, needed }) => {
                write!(f, "{edge}: {needed} segments are more than a mesh indexes")
            }
            Self::Refused(TessRefusal::NoFace { face }) => write!(f, "{face} is not a live face"),
            Self::Unsupported(TessUnsupported::EdgeKind { edge, kind }) => {
                write!(f, "{edge}: no sampling rule for a {kind} edge")
            }
            Self::Unsupported(TessUnsupported::FaceKind { face, kind }) => {
                write!(f, "{face}: no curvature bound for a {kind} face")
            }
            Self::Unsupported(TessUnsupported::NoBound { edge }) => {
                write!(f, "{edge}: the derivative enclosures bound no count")
            }
            Self::Exhausted { limit } => write!(f, "the budget of {limit} ran out"),
            Self::Cancelled => f.write_str("stopped"),
            Self::Defect(TessDefect::Sagitta { edge }) => {
                write!(f, "{edge}: the closed-form count missed the chord")
            }
            Self::Defect(TessDefect::Unindexed { edge }) => {
                write!(f, "{edge}: an end vertex is not live")
            }
            Self::Defect(TessDefect::Crossing { face }) => {
                write!(f, "{face}: two boundary segments cross in uv")
            }
            Self::Defect(TessDefect::Duplicate { face }) => {
                write!(f, "{face}: two boundary points coincide in uv")
            }
            Self::Defect(TessDefect::Triangulation { face }) => {
                write!(f, "{face}: the triangulation failed")
            }
        }
    }
}

impl std::error::Error for TessError {}

impl From<scx_num::budget::Exhausted> for TessError {
    fn from(e: scx_num::budget::Exhausted) -> Self {
        Self::Exhausted { limit: e.limit }
    }
}

impl From<scx_num::budget::Stopped> for TessError {
    fn from(_: scx_num::budget::Stopped) -> Self {
        Self::Cancelled
    }
}
