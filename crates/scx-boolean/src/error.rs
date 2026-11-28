// Written for solidcorex. Apache-2.0.
//! [`BooleanError`]: why a Boolean did not finish, by [`ErrorKind`].
//!
//! No variant carries a solver status number. Each names the operand and the entity it
//! failed at: ids are per operand, so an id alone does not say which body it is in.

use core::fmt;

use scx_brep::{EdgeId, Entity, FaceId, Operand, VertexId};
use scx_num::budget::{Exhausted, Stopped};
use scx_num::error::ErrorKind;
use scx_ssi::SsiError;
use scx_surface::{SurfaceError, SurfaceKind};

use crate::given::Fact;
use crate::imprint_curves::{CutRefusal, CutUnsupported};

/// Why a Boolean did not finish.
///
/// `PartialEq` only: an [`SsiPair`](BooleanUnsupported::SsiPair) cause names a point.
#[derive(Debug, Clone, Copy, PartialEq)]
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
    /// A face of a planar Boolean's operand that is not a plane within the resolution of
    /// the operands' common plane.
    NotCoplanarSheet {
        /// The operand.
        operand: Operand,
        /// The face.
        face: FaceId,
    },
    /// A face of a planar Boolean's operand whose loops are not a region of the plane
    /// (they cross, touch, or leave a hole outside).
    BadProfile {
        /// The operand.
        operand: Operand,
        /// The face.
        face: FaceId,
    },
    /// A fact stated to [`boolean_given`](crate::boolean_given) is not so: the Boolean's
    /// own test of it disagrees, or it names an entity its operand does not have.
    FalseFact {
        /// The fact.
        fact: Fact,
    },
    /// Curves given to [`imprint_curves`](crate::imprint_curves) that cannot be imprinted on
    /// the face.
    Cut(CutRefusal),
}

/// What this method does not answer yet.
#[derive(Debug, Clone, Copy, PartialEq)]
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
    /// A pair of analytic faces whose surfaces meet in a curve with no closed form here
    /// (72 §5.2, a quartic say): step 5's marcher answers it. The faces are `a`'s, then
    /// `b`'s, and so are the kinds.
    SsiPair {
        /// `a`'s face.
        a: FaceId,
        /// `b`'s face.
        b: FaceId,
        /// The two surfaces' kinds.
        kinds: [SurfaceKind; 2],
        /// Why the intersector did not answer, and where: one of its refusals (kind
        /// `Unsupported`), never its budget or stop, which are
        /// [`BooleanError::Exhausted`] and [`BooleanError::Cancelled`]
        /// ([`BooleanError::of_ssi`]).
        cause: SsiError,
    },
    /// A face whose new loops need a pcurve with no exact form here (a fitted one, say:
    /// two spheres' circle on either sphere): step 5's fitter writes it.
    Pcurve {
        /// The operand.
        operand: Operand,
        /// The face.
        face: FaceId,
    },
    /// A place the build or a query does not order (BO25 names them; the result's walls,
    /// laminas, pinches and touches inside a face are built):
    /// - two of the result's faces leaving an edge of three or more fins in one direction
    ///   (tangent along it): their radial order needs their bends, which nothing measures
    ///   yet; or one with no direction there (no normal at the edge's midpoint);
    /// - a touch inside a face whose point the face's region test puts on a piece's
    ///   boundary rather than inside one piece;
    /// - a point classification of an operand that met an edge or vertex where more than
    ///   two regions meet (scx-bodyq's own `NonManifold`).
    NonManifold {
        /// The result's edge, or the operand's entity the classification met.
        at: Entity,
    },
    /// A face of a horn torus that the other operand cuts: its pole is one vertex with a
    /// collapsed edge at each end of the face's `v` range (two lobes meeting), and the cut
    /// face's trace keeps one collapsed edge per vertex. The trace would need the vertex's
    /// star in two sheets of the face's uv.
    HornPole {
        /// The operand.
        operand: Operand,
        /// The face.
        face: FaceId,
    },
    /// An edge of a planar Boolean's operand that is neither a line nor a circle.
    PlanarEdge {
        /// The operand.
        operand: Operand,
        /// The edge.
        edge: EdgeId,
    },
    /// A planar Boolean the certified arithmetic cannot settle: crossings closer than the
    /// resolution, a point on an operand's boundary the other's pieces do not name, or a
    /// result whose loops are no region (a hole touching its outer loop at a point).
    PlanarUndecided,
    /// Curves given to [`imprint_curves`](crate::imprint_curves) that wait for the fitter or
    /// for crossings on curved faces.
    Cut(CutUnsupported),
    /// Two or more of the result's surfaces enclose each other in a way the shell
    /// placement does not decide: more than one outer surface with a cavity among them.
    Nesting,
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
    /// A sub-span of an intersection curve or of a lying edge that no sample decides against
    /// the face: every sample stayed near its boundary with no edge of it within
    /// coincidence, after every refinement.
    UndecidedSpan {
        /// The operand.
        operand: Operand,
        /// The face.
        face: FaceId,
    },
    /// A kept piece of an intersection curve ends at no merged point: the faces' boundaries
    /// cross where stage 2 found no contact. `a`'s face, then `b`'s.
    LooseSection {
        /// The faces.
        faces: [FaceId; 2],
    },
    /// A kept piece of an edge lying on a face of the other operand ends at no merged point.
    LooseSpan {
        /// The edge's operand.
        operand: Operand,
        /// The edge.
        edge: EdgeId,
    },
    /// An edge the method reads is missing from its operand: a bug.
    NoEdge {
        /// The operand.
        operand: Operand,
        /// The edge.
        edge: EdgeId,
    },
    /// The imprint's walk around a cut face's half-edges found no next one, or came back
    /// to a half-edge it had left.
    Trace {
        /// The operand.
        operand: Operand,
        /// The face.
        face: FaceId,
    },
    /// The pcurve writer refused a loop the imprint traced on a face.
    Loops {
        /// The operand.
        operand: Operand,
        /// The face.
        face: FaceId,
    },
    /// A piece of an edge the imprint looked for (by parameter, or along the other
    /// operand's edge) is not there.
    Piece {
        /// The operand.
        operand: Operand,
        /// The edge.
        edge: EdgeId,
    },
    /// The draft refused an entity or a lineage record the imprint wrote, or lost one it
    /// read.
    Draft,
    /// One component of the imprinted faces (faces joined across edges where the operands
    /// do not meet) took two answers: two seeds disagree, or a coincident piece is joined
    /// to one that is not. The lowest face of the component.
    Seeds {
        /// The operand.
        operand: Operand,
        /// The draft's face.
        face: FaceId,
    },
    /// A face's piece the classification could not place against the other operand: no
    /// point inside it, or a point inside it on the other operand's boundary away from any
    /// face coincident with it.
    Classify {
        /// The operand.
        operand: Operand,
        /// The draft's face.
        face: FaceId,
    },
    /// The result's draft did not finish, or an edge of it bounds one face only: the
    /// classification or selection left it open.
    Build,
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
            Self::Refused(BooleanRefusal::NotCoplanarSheet { .. }) => "not_coplanar_sheet",
            Self::Refused(BooleanRefusal::BadProfile { .. }) => "bad_profile",
            Self::Unsupported(BooleanUnsupported::PlanarEdge { .. }) => "planar_edge",
            Self::Unsupported(BooleanUnsupported::PlanarUndecided) => "planar_undecided",
            Self::Refused(BooleanRefusal::FalseFact { .. }) => "false_fact",
            Self::Refused(BooleanRefusal::Cut(_)) => "cut_refused",
            Self::Unsupported(BooleanUnsupported::Cut(_)) => "cut_unsupported",
            Self::Unsupported(BooleanUnsupported::Nurbs { .. }) => "nurbs",
            Self::Unsupported(BooleanUnsupported::Chain { .. }) => "chain",
            Self::Unsupported(BooleanUnsupported::SsiPair { .. }) => "ssi_pair",
            Self::Unsupported(BooleanUnsupported::NonManifold { .. }) => "non_manifold",
            Self::Unsupported(BooleanUnsupported::Nesting) => "nesting",
            Self::Unsupported(BooleanUnsupported::Pcurve { .. }) => "no_exact_pcurve",
            Self::Unsupported(BooleanUnsupported::HornPole { .. }) => "horn_pole",
            Self::Exhausted { .. } => "exhausted",
            Self::Cancelled => "cancelled",
            Self::Defect(BooleanDefect::NoFaceBox { .. }) => "no_face_box",
            Self::Defect(BooleanDefect::NoUvBox { .. }) => "no_uv_box",
            Self::Defect(BooleanDefect::Surface { .. }) => "surface",
            Self::Defect(BooleanDefect::UvUndecided { .. }) => "uv_undecided",
            Self::Defect(BooleanDefect::NearWithoutBoundary { .. }) => "near_without_boundary",
            Self::Defect(BooleanDefect::UndecidedSpan { .. }) => "undecided_span",
            Self::Defect(BooleanDefect::LooseSection { .. }) => "loose_section",
            Self::Defect(BooleanDefect::LooseSpan { .. }) => "loose_span",
            Self::Defect(BooleanDefect::NoEdge { .. }) => "no_edge",
            Self::Defect(BooleanDefect::Trace { .. }) => "imprint_trace",
            Self::Defect(BooleanDefect::Loops { .. }) => "imprint_loops",
            Self::Defect(BooleanDefect::Piece { .. }) => "imprint_piece",
            Self::Defect(BooleanDefect::Draft) => "imprint_draft",
            Self::Defect(BooleanDefect::Seeds { .. }) => "classify_seeds",
            Self::Defect(BooleanDefect::Classify { .. }) => "classify_point",
            Self::Defect(BooleanDefect::Build) => "build",
        }
    }

    /// The error a surface's inversion or intersection gave at `face` of `operand`.
    pub(crate) const fn of_surface(e: SurfaceError, operand: Operand, face: FaceId) -> Self {
        match e {
            SurfaceError::Exhausted(x) => Self::of_budget(x),
            SurfaceError::Stopped => Self::Cancelled,
            SurfaceError::NurbsIntersection | SurfaceError::Undecided { .. } => {
                Self::Unsupported(BooleanUnsupported::Nurbs {
                    operand,
                    at: Entity::Face(face),
                })
            }
            _ => Self::Defect(BooleanDefect::Surface { operand, face }),
        }
    }

    /// The error the intersector gave for faces `a` and `b`, of these kinds: its budget
    /// and stop as the Boolean's own, a refusal as [`BooleanUnsupported::SsiPair`] with it
    /// as the cause.
    pub(crate) const fn of_ssi(
        cause: SsiError,
        a: FaceId,
        b: FaceId,
        kinds: [SurfaceKind; 2],
    ) -> Self {
        match cause {
            SsiError::Budget(e) => Self::of_budget(e),
            SsiError::Stopped => Self::Cancelled,
            _ => Self::Unsupported(BooleanUnsupported::SsiPair { a, b, kinds, cause }),
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
            Self::Refused(BooleanRefusal::NotCoplanarSheet { operand, face }) => {
                write!(f, "{operand}: {face}: not a plane in the operands' plane")
            }
            Self::Refused(BooleanRefusal::BadProfile { operand, face }) => {
                write!(f, "{operand}: {face}: its loops are not a planar region")
            }
            Self::Unsupported(BooleanUnsupported::PlanarEdge { operand, edge }) => {
                write!(f, "{operand}: {edge}: neither a line nor a circle")
            }
            Self::Unsupported(BooleanUnsupported::PlanarUndecided) => {
                f.write_str("the planar Boolean's crossings or result cannot be settled")
            }
            Self::Refused(BooleanRefusal::FalseFact { fact }) => {
                write!(f, "the stated {fact} is not so")
            }
            Self::Refused(BooleanRefusal::Cut(c)) => match c {
                CutRefusal::NoCurves => f.write_str("no curve to imprint"),
                CutRefusal::NoFace(face) => write!(f, "{face} is not a face of the body"),
                CutRefusal::OffFace(i) => write!(f, "curve {i} does not lie on the face"),
                CutRefusal::Misses(i) => write!(f, "curve {i} cuts nothing of the face"),
                CutRefusal::Overlap([i, j]) => write!(f, "curves {i} and {j} share a stretch"),
                CutRefusal::Dangles(i) => {
                    write!(f, "curve {i} has an end inside the face that meets nothing")
                }
            },
            Self::Unsupported(BooleanUnsupported::Cut(c)) => match c {
                CutUnsupported::Surface(face) => write!(f, "{face} is not a plane"),
                CutUnsupported::Curve(i) => {
                    write!(f, "curve {i} is neither a line nor a circle")
                }
            },
            Self::Unsupported(BooleanUnsupported::Nurbs { operand, at }) => {
                write!(f, "{operand}: {at}: NURBS geometry is not intersected yet")
            }
            Self::Unsupported(BooleanUnsupported::Chain {
                operand,
                vertices: [v, w],
            }) => write!(f, "{operand}: {v} and {w} are joined by a chain of points"),
            Self::Unsupported(BooleanUnsupported::SsiPair {
                a,
                b,
                kinds: [ka, kb],
                cause,
            }) => write!(
                f,
                "operand 0 {a} ({ka}) and operand 1 {b} ({kb}) meet in no closed form yet: {cause}"
            ),
            Self::Unsupported(BooleanUnsupported::Pcurve { operand, face }) => {
                write!(f, "{operand}: {face}: a new loop needs a fitted pcurve")
            }
            Self::Unsupported(BooleanUnsupported::NonManifold { at }) => {
                write!(f, "{at}: a non-manifold result is not built yet")
            }
            Self::Unsupported(BooleanUnsupported::HornPole { operand, face }) => write!(
                f,
                "{operand}: {face}: a horn torus's face cut across its pole is not built yet"
            ),
            Self::Unsupported(BooleanUnsupported::Nesting) => {
                f.write_str("the result's surfaces nest in a way not placed yet")
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
            Self::Defect(BooleanDefect::UndecidedSpan { operand, face }) => {
                write!(
                    f,
                    "{operand}: {face}: no sample decides a span near the boundary"
                )
            }
            Self::Defect(BooleanDefect::LooseSection { faces: [a, b] }) => {
                write!(f, "a section of {a} and {b} ends at no merged point")
            }
            Self::Defect(BooleanDefect::LooseSpan { operand, edge }) => {
                write!(
                    f,
                    "{operand}: {edge}: a lying piece ends at no merged point"
                )
            }
            Self::Defect(BooleanDefect::NoEdge { operand, edge }) => {
                write!(f, "{operand}: {edge}: missing")
            }
            Self::Defect(BooleanDefect::Trace { operand, face }) => {
                write!(f, "{operand}: {face}: the imprint's walk lost its way")
            }
            Self::Defect(BooleanDefect::Loops { operand, face }) => {
                write!(
                    f,
                    "{operand}: {face}: the pcurve writer refused a traced loop"
                )
            }
            Self::Defect(BooleanDefect::Piece { operand, edge }) => {
                write!(f, "{operand}: {edge}: a piece the imprint needs is missing")
            }
            Self::Defect(BooleanDefect::Draft) => {
                f.write_str("the draft refused or lost an entity the imprint wrote")
            }
            Self::Defect(BooleanDefect::Seeds { operand, face }) => {
                write!(f, "{operand}: {face}: a component took two answers")
            }
            Self::Defect(BooleanDefect::Classify { operand, face }) => {
                write!(f, "{operand}: {face}: a piece could not be placed")
            }
            Self::Defect(BooleanDefect::Build) => f.write_str("the result's draft did not close"),
        }
    }
}

impl core::error::Error for BooleanError {}
