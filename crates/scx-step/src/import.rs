// Written for solidcorex (docs/research/74-step6-idiom-prep.md §6.3, §29).
//! What a build gives: the body with what the import did to the file's data, or why the
//! body is not built.

use core::fmt;

use scx_brep::{Brep, EdgeId, FaceId, FinishCause, PcurveWriteError, TableFull, VertexId};
use scx_check::{CheckError, Findings};
use scx_curve::CurveError;
use scx_mass::MassError;
use scx_num::error::ErrorKind;
use scx_p21::P21Error;
use scx_surface::{Bracket, GapError, SurfaceError};

use crate::{GapKind, Imported, Raise, SolidOrSheet};

/// What the analytic build leaves to a later task, by name (research 74 analytic first).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum Waits {
    /// A `B_SPLINE_CURVE` edge (SP5).
    BSplineCurve,
    /// A `B_SPLINE_SURFACE` face (SP5).
    BSplineSurface,
    /// A `POLYLINE` edge (SP5, as a degree-1 spline).
    Polyline,
    /// An `OFFSET_CURVE_3D` edge (SP6).
    OffsetCurve,
    /// A curve kind not listed here.
    OtherCurve,
    /// A `SURFACE_OF_REVOLUTION` face (SP6).
    Revolution,
    /// A `SURFACE_OF_LINEAR_EXTRUSION` face (SP6).
    Extrusion,
    /// An `OFFSET_SURFACE` face (SP6).
    OffsetSurface,
    /// A `DEGENERATE_TOROIDAL_SURFACE` face.
    DegenerateTorus,
    /// A `RECTANGULAR_TRIMMED_SURFACE` face.
    TrimmedSurface,
    /// A surface kind not listed here.
    OtherSurface,
    /// A face with a loop that winds round its surface and no seam edge (the writer
    /// needs a seam; adding one is a change to the file's topology).
    Seamless,
    /// A `VERTEX_LOOP` bound.
    VertexLoop,
    /// An edge with no closed-form pcurve on its face (BR9's fitter).
    PcurveFit,
    /// An edge used by more than two faces.
    NonManifold,
    /// A gap whose bound is not built (a NURBS surface).
    NurbsGap,
}

impl Waits {
    /// The cause's stable name.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::BSplineCurve => "step_waits_bspline_curve",
            Self::BSplineSurface => "step_waits_bspline_surface",
            Self::Polyline => "step_waits_polyline",
            Self::OffsetCurve => "step_waits_offset_curve",
            Self::OtherCurve => "step_waits_other_curve",
            Self::Revolution => "step_waits_revolution",
            Self::Extrusion => "step_waits_extrusion",
            Self::OffsetSurface => "step_waits_offset_surface",
            Self::DegenerateTorus => "step_waits_degenerate_torus",
            Self::TrimmedSurface => "step_waits_trimmed_surface",
            Self::OtherSurface => "step_waits_other_surface",
            Self::Seamless => "step_waits_seamless",
            Self::VertexLoop => "step_waits_vertex_loop",
            Self::PcurveFit => "step_waits_pcurve_fit",
            Self::NonManifold => "step_waits_non_manifold",
            Self::NurbsGap => "step_waits_nurbs_gap",
        }
    }
}

/// Why a curve or surface entity's numbers do not make its kind.
#[derive(Clone, Copy, Debug, PartialEq)]
#[non_exhaustive]
pub enum GeomCause {
    /// A placement with a direction of no length, or a reference direction along its axis.
    Placement,
    /// The curve constructor's refusal.
    Curve(CurveError),
    /// The surface constructor's refusal.
    Surface(SurfaceError),
}

/// Why a body is not built. Nothing of it is kept.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum BuildError {
    /// The body uses a kind a later task reads.
    Pending {
        /// The entity that uses it (curve, surface, face or edge).
        entity: u64,
        /// What it waits on.
        waits: Waits,
    },
    /// An entity is not what its place needs.
    Malformed(Box<P21Error>),
    /// A curve or surface whose numbers do not make its kind.
    Geometry {
        /// The curve or surface entity.
        entity: u64,
        /// Why.
        cause: GeomCause,
    },
    /// An edge whose vertices are the same point of a curve that does not close, or two
    /// vertices at the same parameter of one: an edge of no length.
    Degenerate {
        /// The `EDGE_CURVE`.
        edge: u64,
    },
    /// A gap the budget did not decide: the body is not built rather than built on a
    /// sampled guess (R2).
    Undecided {
        /// The edge or vertex.
        entity: u64,
        /// Which gap.
        gap: GapKind,
        /// The bracket reached.
        reached: Bracket,
    },
    /// A gap that cannot be measured (its range is not finite).
    Gap {
        /// The edge or vertex.
        entity: u64,
        /// Why.
        cause: GapError,
    },
    /// A face whose loops the exact pcurve writer cannot place.
    Pcurve {
        /// The face.
        face: u64,
        /// Why.
        cause: PcurveWriteError,
    },
    /// A shell whose faces cannot all point one way: its edges' uses give an odd cycle.
    NotOrientable {
        /// The shell.
        shell: u64,
    },
    /// A closed shell that bounds no volume (the certified sign is zero).
    Flat {
        /// The shell.
        shell: u64,
    },
    /// The records do not make a valid body.
    Unfinished(FinishCause),
    /// A shell's volume sign could not be certified.
    Mass {
        /// The shell.
        shell: u64,
        /// Why.
        cause: MassError,
    },
    /// The import check failed to run.
    Check(CheckError),
    /// The body has more entities than a table holds.
    TableFull(TableFull),
}

impl BuildError {
    /// The class of the error (01 §5).
    #[must_use]
    pub fn kind(&self) -> ErrorKind {
        match self {
            Self::Pending { .. } => ErrorKind::Unsupported,
            Self::Malformed(e) => e.kind(),
            Self::Geometry { .. }
            | Self::Degenerate { .. }
            | Self::NotOrientable { .. }
            | Self::Flat { .. }
            | Self::Unfinished(_) => ErrorKind::Refused,
            Self::Undecided { .. } => ErrorKind::Exhausted,
            Self::Gap { cause, .. } => cause.kind(),
            Self::Pcurve { cause, .. } => cause.kind(),
            Self::Mass { cause, .. } => cause.kind(),
            Self::Check(e) => e.kind(),
            Self::TableFull(e) => e.kind(),
        }
    }

    /// The stable name of the cause.
    #[must_use]
    pub fn name(&self) -> &'static str {
        match self {
            Self::Pending { waits, .. } => waits.name(),
            Self::Malformed(e) => e.name(),
            Self::Geometry { .. } => "step_bad_geometry",
            Self::Degenerate { .. } => "step_degenerate_edge",
            Self::Undecided { .. } => "step_gap_undecided",
            Self::Gap { cause, .. } => cause.name(),
            Self::Pcurve { cause, .. } => cause.name(),
            Self::NotOrientable { .. } => "step_not_orientable",
            Self::Flat { .. } => "step_flat_shell",
            Self::Unfinished(c) => c.name(),
            Self::Mass { cause, .. } => cause.name(),
            Self::Check(e) => e.name(),
            Self::TableFull(e) => e.name(),
        }
    }
}

impl fmt::Display for BuildError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Pending { entity, waits } => {
                write!(f, "#{entity} needs {}, not built yet", waits.name())
            }
            Self::Malformed(e) => write!(f, "{e}"),
            Self::Geometry { entity, cause } => match cause {
                GeomCause::Placement => write!(f, "#{entity} has an unusable placement"),
                GeomCause::Curve(e) => write!(f, "curve #{entity}: {e}"),
                GeomCause::Surface(e) => write!(f, "surface #{entity}: {e}"),
            },
            Self::Degenerate { edge } => write!(f, "edge #{edge} has no length"),
            Self::Undecided {
                entity,
                gap,
                reached,
            } => write!(
                f,
                "the {gap:?} gap of #{entity} is undecided in [{}, {}] mm",
                reached.lower(),
                reached.upper()
            ),
            Self::Gap { entity, cause } => write!(f, "the gap of #{entity}: {cause}"),
            Self::Pcurve { face, cause } => write!(f, "face #{face}: {cause}"),
            Self::NotOrientable { shell } => write!(f, "shell #{shell} is not orientable"),
            Self::Flat { shell } => write!(f, "shell #{shell} bounds no volume"),
            Self::Unfinished(c) => write!(f, "the body is not valid: {c}"),
            Self::Mass { shell, cause } => write!(f, "shell #{shell}: {cause}"),
            Self::Check(e) => write!(f, "{e}"),
            Self::TableFull(e) => write!(f, "{e}"),
        }
    }
}

impl std::error::Error for BuildError {}

impl From<TableFull> for BuildError {
    fn from(e: TableFull) -> Self {
        Self::TableFull(e)
    }
}

impl From<P21Error> for BuildError {
    fn from(e: P21Error) -> Self {
        Self::Malformed(Box::new(e))
    }
}

/// What the build changed in the file's data, besides the raises.
#[derive(Clone, Copy, Debug, PartialEq)]
#[non_exhaustive]
pub enum Note {
    /// The face's `same_sense` disagreed with its loops (its outer loop runs clockwise about
    /// the normal it names): the loops are kept and the normal taken from them.
    TurnedFace {
        /// The face.
        face: u64,
    },
    /// The edge's `same_sense` disagreed with its vertices on a curve that does not close:
    /// the vertices are kept and the edge runs the way they say.
    TurnedEdge {
        /// The `EDGE_CURVE`.
        edge: u64,
    },
    /// Every face of the shell pointed into the volume: the certified volume sign turned
    /// them.
    TurnedShell {
        /// The shell.
        shell: u64,
    },
    /// A raise past `MAX_ACCURACY_RATIO` of the resolution: made, but the entity is
    /// coarser than the kernel's accuracy policy keeps.
    PastRatio {
        /// The edge or vertex.
        entity: u64,
        /// The accuracy over the resolution's zone.
        ratio: f64,
    },
}

/// One body built.
#[derive(Clone, Debug)]
#[non_exhaustive]
pub struct Built {
    /// The body entity.
    pub entity: u64,
    /// Its name.
    pub name: String,
    /// A solid or a sheet.
    pub kind: SolidOrSheet,
    /// The body.
    pub body: Brep,
    /// The accuracies raised to enclosed gaps, in entity order.
    pub raised: Vec<Raise>,
    /// What else the build changed in the file's data.
    pub notes: Vec<Note>,
    /// The import check's findings: a body with findings is still given.
    pub findings: Findings,
    /// Each face entity and its face, in the file's order.
    pub faces: Vec<(u64, FaceId)>,
    /// Each `EDGE_CURVE` and its edge, in the order first used.
    pub edges: Vec<(u64, EdgeId)>,
    /// Each `VERTEX_POINT` and its vertex, in the order first used.
    pub vertices: Vec<(u64, VertexId)>,
}

/// A body read but not built.
#[derive(Clone, Debug, PartialEq)]
pub struct Unbuilt {
    /// The body entity.
    pub entity: u64,
    /// Why.
    pub why: BuildError,
}

/// A read and the build of each of its bodies.
#[derive(Clone, Debug)]
#[non_exhaustive]
pub struct Import {
    /// The read: plans, products, skips and the report, whose
    /// [`raised`](crate::ImportReport::raised) holds every built body's raises in entity
    /// order.
    pub read: Imported,
    /// The bodies built, in the read's order. Each is built once; the product tree places
    /// it ([`ProductTree::placed`](crate::ProductTree::placed)).
    pub built: Vec<Built>,
    /// The bodies read but not built, in the read's order.
    pub unbuilt: Vec<Unbuilt>,
}
