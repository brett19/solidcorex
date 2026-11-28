// Written for solidcorex. Apache-2.0.
//! The sheet blends' one error (research 62 shape): each has a `kind()` and a stable
//! `name()`; a refusal names the input that is wrong, an unsupported case names the piece of
//! the kernel that is missing for it.

use core::fmt;

use scx_boolean::BooleanError;
use scx_brep::EdgeId;
use scx_num::error::ErrorKind;
use scx_nurbs_surface::NurbsSurfaceError;
use scx_surface::SurfaceError;

use crate::blend::BlendError;
use crate::error::MakeError;

/// Which input is wrong.
#[derive(Debug, Clone, Copy, PartialEq)]
#[non_exhaustive]
pub enum SheetBlendRefusal {
    /// No edge was given for a chain.
    NoChain,
    /// The edge is not the body's.
    NotEdge(EdgeId),
    /// Edge `at` of a chain does not start or end where the one before it ends.
    NotChained {
        /// The edge's place in the chain.
        at: usize,
    },
    /// No face holds every edge of the chain.
    NoFace,
    /// No face round the chain has a tangent plane keeping any of the leave direction.
    NoSupport,
    /// Fewer than two stations, or fewer than two points of a polyline.
    Stations {
        /// How many were asked for or given.
        count: usize,
    },
    /// A point or direction that is not finite, or a polyline of no length.
    NotFinite,
    /// A body with a solid region: the operation takes sheets.
    NotSheet {
        /// The operand, 0 or 1.
        operand: usize,
    },
    /// A body of another document's resolution.
    OtherDocument {
        /// The operand, 0 or 1.
        operand: usize,
    },
    /// A fillet radius or chord that is not finite and positive.
    Size {
        /// The value (mm).
        value: f64,
    },
    /// The two sheets share no edge (none coincides within the document's resolution).
    NoSharedEdge,
    /// The sheets meet at an angle a round cannot take: folded onto each other (no room) or
    /// continuing flat (nothing to round). The angle is between their interior directions.
    Angle {
        /// The angle (radians).
        angle: f64,
    },
    /// The operation takes a sheet of one face.
    NotOneFace {
        /// The operand, 0 or 1.
        operand: usize,
    },
    /// The two sheets do not cross.
    NoCrossing,
    /// More than one boundary edge of one sheet coincides with one of the other: which to
    /// round is not given.
    SharedEdges {
        /// How many pairs coincide.
        count: usize,
    },
    /// The contact does not cut a strip off a sheet: the setback reaches past the face.
    Setback {
        /// The setback (mm).
        setback: f64,
    },
}

/// Why a case is not built yet, naming what it waits for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum SheetBlendUnsupported {
    /// A NURBS sheet: the round's contact curves on it come from the NURBS surface-surface
    /// intersector's output (SM9).
    NurbsSheet,
    /// Two sheets whose round is not a cylinder, torus or sphere (the section is not two
    /// lines swept along a line or round an axis): the procedural rolling-ball surface
    /// (scx-blend) it needs is not built.
    RollingBall,
    /// A curvature-continuous section: the G2 section of the one blend section generator
    /// (scx-surface, research 61 §7) is not built.
    Curvature,
    /// A round on a torus that would cross its own axis (minor radius not below the major).
    SpindleTorus,
    /// Trimming or splitting a sheet that is not a plane: the curve imprint on a curved face
    /// waits for the curve-curve intersector (`scx_boolean::CutUnsupported::Surface`).
    CurvedImprint,
    /// Section `section`'s ends are one point within the resolution: matching a closed
    /// section's winding and seam to the one before is not a turn, and is not built.
    ClosedSection {
        /// The section.
        section: usize,
    },
}

/// What went wrong.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub enum SheetBlendError {
    /// The input is wrong.
    Refused(SheetBlendRefusal),
    /// The case is not built yet.
    Unsupported(SheetBlendUnsupported),
    /// The section blend failed. A context frame.
    Blend(BlendError),
    /// Writing a body failed. A context frame.
    Make(MakeError),
    /// Imprinting a sheet failed. A context frame.
    Boolean(BooleanError),
    /// A surface's evaluation or certificate failed. A context frame.
    Surface(SurfaceError),
    /// A definition's motion `index` could not be applied to the rebuilt surface.
    Motion {
        /// The motion's place in the definition.
        index: usize,
        /// Why.
        cause: NurbsSurfaceError,
    },
    /// A step failed through the module's own fault. A bug.
    Defect {
        /// The step.
        why: &'static str,
    },
}

impl SheetBlendError {
    /// The kind.
    #[must_use]
    pub fn kind(&self) -> ErrorKind {
        match self {
            Self::Refused(_) => ErrorKind::Refused,
            Self::Unsupported(_) => ErrorKind::Unsupported,
            Self::Blend(e) => e.kind(),
            Self::Make(e) => e.kind(),
            Self::Boolean(e) => e.kind(),
            Self::Surface(e) => e.kind(),
            Self::Motion { cause, .. } => cause.kind(),
            Self::Defect { .. } => ErrorKind::Defect,
        }
    }

    /// The stable name.
    #[must_use]
    pub const fn name(&self) -> &'static str {
        match self {
            Self::Refused(r) => match r {
                SheetBlendRefusal::NoChain => "sheet_blend_no_chain",
                SheetBlendRefusal::NotEdge(_) => "sheet_blend_not_edge",
                SheetBlendRefusal::NotChained { .. } => "sheet_blend_not_chained",
                SheetBlendRefusal::NoFace => "sheet_blend_no_face",
                SheetBlendRefusal::NoSupport => "sheet_blend_no_support",
                SheetBlendRefusal::Stations { .. } => "sheet_blend_stations",
                SheetBlendRefusal::NotFinite => "sheet_blend_not_finite",
                SheetBlendRefusal::NotSheet { .. } => "sheet_blend_not_sheet",
                SheetBlendRefusal::OtherDocument { .. } => "sheet_blend_other_document",
                SheetBlendRefusal::Size { .. } => "sheet_blend_size",
                SheetBlendRefusal::NoSharedEdge => "sheet_blend_no_shared_edge",
                SheetBlendRefusal::Angle { .. } => "sheet_blend_angle",
                SheetBlendRefusal::NotOneFace { .. } => "sheet_blend_not_one_face",
                SheetBlendRefusal::NoCrossing => "sheet_blend_no_crossing",
                SheetBlendRefusal::SharedEdges { .. } => "sheet_blend_shared_edges",
                SheetBlendRefusal::Setback { .. } => "sheet_blend_setback",
            },
            Self::Unsupported(u) => match u {
                SheetBlendUnsupported::NurbsSheet => "sheet_blend_nurbs_sheet",
                SheetBlendUnsupported::RollingBall => "sheet_blend_rolling_ball",
                SheetBlendUnsupported::Curvature => "sheet_blend_curvature",
                SheetBlendUnsupported::SpindleTorus => "sheet_blend_spindle_torus",
                SheetBlendUnsupported::CurvedImprint => "sheet_blend_curved_imprint",
                SheetBlendUnsupported::ClosedSection { .. } => "sheet_blend_closed_section",
            },
            Self::Blend(_) => "sheet_blend_blend",
            Self::Make(_) => "sheet_blend_make",
            Self::Boolean(_) => "sheet_blend_boolean",
            Self::Surface(_) => "sheet_blend_surface",
            Self::Motion { .. } => "sheet_blend_motion",
            Self::Defect { .. } => "sheet_blend_defect",
        }
    }
}

impl fmt::Display for SheetBlendRefusal {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NoChain => f.write_str("no edge was given"),
            Self::NotEdge(e) => write!(f, "{e:?} is not the body's"),
            Self::NotChained { at } => {
                write!(
                    f,
                    "edge {at} of the chain does not run on from the one before"
                )
            }
            Self::NoFace => f.write_str("no face holds every edge of the chain"),
            Self::NoSupport => {
                f.write_str("no face round the chain keeps any of the leave direction")
            }
            Self::Stations { count } => write!(f, "{count} stations or points; at least 2"),
            Self::NotFinite => f.write_str("a point or direction is not finite, or has no length"),
            Self::NotSheet { operand } => write!(f, "operand {operand} is not a sheet"),
            Self::OtherDocument { operand } => {
                write!(f, "operand {operand} is of another resolution")
            }
            Self::Size { value } => write!(f, "the size {value} mm is not finite and positive"),
            Self::NoSharedEdge => f.write_str("the sheets share no edge"),
            Self::Angle { angle } => write!(
                f,
                "the sheets meet at {angle} rad between their interiors: no round fits"
            ),
            Self::NotOneFace { operand } => write!(f, "operand {operand} is not one face"),
            Self::NoCrossing => f.write_str("the sheets do not cross"),
            Self::SharedEdges { count } => {
                write!(f, "{count} edge pairs coincide; one is needed")
            }
            Self::Setback { setback } => {
                write!(
                    f,
                    "the setback {setback} mm does not cut a strip off the sheet"
                )
            }
        }
    }
}

impl fmt::Display for SheetBlendUnsupported {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Self::ClosedSection { section } = self {
            return write!(
                f,
                "section {section} is closed: its winding is not matched yet"
            );
        }
        f.write_str(match self {
            Self::ClosedSection { .. } => "a closed section",
            Self::NurbsSheet => {
                "a round on a NURBS sheet needs the NURBS surface-surface intersection (SM9)"
            }
            Self::RollingBall => {
                "this round is not a cylinder, torus or sphere: it needs the rolling-ball surface"
            }
            Self::Curvature => {
                "a curvature-continuous round needs the G2 blend section generator (scx-surface)"
            }
            Self::SpindleTorus => "the round's torus would cross its own axis",
            Self::CurvedImprint => {
                "cutting a sheet that is not a plane needs the curve imprint on curved faces"
            }
        })
    }
}

impl fmt::Display for SheetBlendError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Refused(r) => write!(f, "sheet blend refused: {r}"),
            Self::Unsupported(u) => write!(f, "sheet blend unsupported: {u}"),
            Self::Blend(_) => f.write_str("the section blend failed"),
            Self::Make(_) => f.write_str("writing the body failed"),
            Self::Boolean(_) => f.write_str("imprinting a sheet failed"),
            Self::Surface(_) => f.write_str("a surface evaluation failed"),
            Self::Motion { index, .. } => write!(f, "motion {index} could not be applied"),
            Self::Defect { why } => write!(f, "sheet blend defect: {why}"),
        }
    }
}

impl std::error::Error for SheetBlendError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Blend(e) => Some(e),
            Self::Make(e) => Some(e),
            Self::Boolean(e) => Some(e),
            Self::Surface(e) => Some(e),
            Self::Motion { cause, .. } => Some(cause),
            Self::Refused(_) | Self::Unsupported(_) | Self::Defect { .. } => None,
        }
    }
}

impl From<BlendError> for SheetBlendError {
    fn from(e: BlendError) -> Self {
        Self::Blend(e)
    }
}

impl From<MakeError> for SheetBlendError {
    fn from(e: MakeError) -> Self {
        Self::Make(e)
    }
}

impl From<BooleanError> for SheetBlendError {
    fn from(e: BooleanError) -> Self {
        Self::Boolean(e)
    }
}

impl From<SurfaceError> for SheetBlendError {
    fn from(e: SurfaceError) -> Self {
        Self::Surface(e)
    }
}

pub(super) const fn refused(r: SheetBlendRefusal) -> SheetBlendError {
    SheetBlendError::Refused(r)
}

pub(super) const fn unsupported(u: SheetBlendUnsupported) -> SheetBlendError {
    SheetBlendError::Unsupported(u)
}
