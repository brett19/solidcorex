// Written for solidcorex. Apache-2.0.
//! The typed errors: what ends a call. What one face cannot answer is not an error; it is
//! named in the result ([`crate::Silhouettes::unsupported`], [`crate::OnFace::Undecided`]).

use core::fmt;

use scx_bodyq::BodyqError;
use scx_brep::FaceId;
use scx_curve::CurveError;
use scx_num::budget::{Exhausted, Stopped};
use scx_num::error::ErrorKind;
use scx_surface::SurfaceError;

/// Why a view cannot be built.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum BadView {
    /// The direction of sight has no direction: zero, infinite or NaN.
    Direction,
    /// The up direction has no direction: zero, infinite or NaN.
    Up,
    /// Up is parallel to the direction of sight, so the screen's axes are not fixed.
    Parallel,
}

/// Why hidden-line work ended.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub enum HlrError {
    /// The view was refused.
    BadView(BadView),
    /// A placement frame of the scene does not map a body into finite coordinates.
    BadPlacement {
        /// The body's position in the scene.
        body: usize,
    },
    /// The work budget ran out.
    Exhausted(Exhausted),
    /// The caller stopped the work.
    Stopped,
    /// A face's surface refused a query (other than for a NURBS curve, which is named in
    /// the result instead).
    Surface {
        /// The face.
        face: FaceId,
        /// What the surface said.
        error: SurfaceError,
    },
    /// A silhouette curve could not be built (a coordinate overflowed).
    Curve {
        /// The face.
        face: FaceId,
        /// What the curve said.
        error: CurveError,
    },
    /// A body query (the scene's tight box) failed.
    Bodyq(BodyqError),
    /// The kernel's own fault: a state its types should rule out.
    Defect {
        /// What went wrong.
        reason: &'static str,
    },
}

impl HlrError {
    /// The class of failure: what the caller does next.
    #[must_use]
    pub const fn kind(&self) -> ErrorKind {
        match self {
            Self::BadView(_) | Self::BadPlacement { .. } => ErrorKind::Refused,
            Self::Exhausted(_) => ErrorKind::Exhausted,
            Self::Stopped => ErrorKind::Cancelled,
            Self::Surface { error, .. } => error.kind(),
            Self::Curve { error, .. } => error.kind(),
            Self::Bodyq(e) => e.kind(),
            Self::Defect { .. } => ErrorKind::Defect,
        }
    }

    /// The cause's stable name.
    #[must_use]
    pub const fn name(&self) -> &'static str {
        match self {
            Self::BadView(BadView::Direction) => "hlr_view_direction",
            Self::BadView(BadView::Up) => "hlr_view_up",
            Self::BadView(BadView::Parallel) => "hlr_view_parallel",
            Self::BadPlacement { .. } => "hlr_bad_placement",
            Self::Exhausted(e) => e.name(),
            Self::Stopped => Stopped.name(),
            Self::Surface { error, .. } => error.name(),
            Self::Curve { error, .. } => error.name(),
            Self::Bodyq(e) => e.name(),
            Self::Defect { .. } => "hlr_defect",
        }
    }

    /// A surface's error on `face`, with its budget and stop causes lifted to this level.
    pub(crate) fn surface(face: FaceId, error: SurfaceError) -> Self {
        match error {
            SurfaceError::Exhausted(e) => Self::Exhausted(e),
            SurfaceError::Stopped => Self::Stopped,
            error => Self::Surface { face, error },
        }
    }
}

impl From<Exhausted> for HlrError {
    fn from(e: Exhausted) -> Self {
        Self::Exhausted(e)
    }
}

impl From<Stopped> for HlrError {
    fn from(_: Stopped) -> Self {
        Self::Stopped
    }
}

impl From<BodyqError> for HlrError {
    fn from(e: BodyqError) -> Self {
        Self::Bodyq(e)
    }
}

impl fmt::Display for HlrError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::BadView(BadView::Direction) => f.write_str("the view direction has no direction"),
            Self::BadView(BadView::Up) => f.write_str("the view's up has no direction"),
            Self::BadView(BadView::Parallel) => {
                f.write_str("the view's up is parallel to its direction")
            }
            Self::BadPlacement { body } => {
                write!(f, "body {body}'s placement leaves finite coordinates")
            }
            Self::Exhausted(e) => write!(f, "{e}"),
            Self::Stopped => f.write_str("stopped by the caller"),
            Self::Surface { face, error } => write!(f, "face {face:?}: {error}"),
            Self::Curve { face, error } => write!(f, "face {face:?}: {error}"),
            Self::Bodyq(e) => write!(f, "{e}"),
            Self::Defect { reason } => write!(f, "hidden-line defect: {reason}"),
        }
    }
}

impl std::error::Error for HlrError {}
