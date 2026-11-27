// Written for solidcorex. Apache-2.0.
//! What `finish` refuses (01 §2.3): the draft comes back whole with the cause, so the
//! caller can repair it or report it.
//!
//! Three groups of checks run in order, and the first failure is the cause:
//!
//! - every curve is attached ([`FinishCause::MissingCurve`]);
//! - the structure is a body ([`StructureCause`]): references resolve (what the views rely
//!   on), then radial completeness, loops, sides, shells and regions;
//! - the cheap geometry check ([`GeometryCause`]): ends within their vertices' balls, pcurves
//!   on their edges, loops closed and oriented in uv, and the pole and seam rules
//!   (Edgeuse 7, Edgeuse 20, Face 24).
//!
//! Within a group the checks walk records in kind order (regions, shells, faces, edges,
//! vertices), then id order, so the cause is the same on every run.

use core::fmt;

use scx_num::error::ErrorKind;

use crate::draft::Draft;
use scx_num::frame::Sense;
use scx_num::units::Length;

use crate::id::{EdgeId, Entity, FaceId, RegionId, ShellId, VertexId};
use crate::record::Fin;

/// Why `finish` refused a draft.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub enum FinishCause {
    /// An edge's curve was never attached (63 D8). Only our own operations build such a
    /// draft: a caller's input without a curve is refused before `finish`.
    MissingCurve {
        /// The lowest such edge.
        edge: EdgeId,
    },
    /// The structure is not a body.
    Structure(StructureCause),
    /// The geometry does not fit the structure.
    Geometry(GeometryCause),
}

impl FinishCause {
    /// [`ErrorKind::Defect`] for a missing curve (D8); [`ErrorKind::Refused`] for a
    /// structure or geometry the draft's builder must change. An operation that reaches either from
    /// its own draft reports its own `Defect` (01 §6).
    #[must_use]
    pub const fn kind(&self) -> ErrorKind {
        match self {
            Self::MissingCurve { .. } => ErrorKind::Defect,
            Self::Structure(_) | Self::Geometry(_) => ErrorKind::Refused,
        }
    }

    /// The stable name.
    #[must_use]
    pub const fn name(&self) -> &'static str {
        match self {
            Self::MissingCurve { .. } => "missing_curve",
            Self::Structure(cause) => cause.name(),
            Self::Geometry(cause) => cause.name(),
        }
    }
}

impl fmt::Display for FinishCause {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingCurve { edge } => write!(f, "edge {edge} has no curve"),
            Self::Structure(cause) => cause.fmt(f),
            Self::Geometry(cause) => cause.fmt(f),
        }
    }
}

impl core::error::Error for FinishCause {}

/// How a draft's structure fails to be a body. The lowest failing id is named, so the
/// cause is the same on every run.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum StructureCause {
    /// The outside region is gone or solid.
    Outside,
    /// A record names an entity the draft does not hold.
    Dangling {
        /// The record.
        from: Entity,
        /// What it names.
        to: Entity,
    },
    /// An edge lists a fin whose face has no use of the edge in that sense.
    FinWithoutUse {
        /// The edge.
        edge: EdgeId,
        /// The fin.
        fin: Fin,
    },
    /// A shell lists as a wire an edge that has fins.
    WireWithFins {
        /// The shell.
        shell: ShellId,
        /// The edge.
        edge: EdgeId,
    },
    /// A face has no loop.
    FaceWithoutLoop {
        /// The face.
        face: FaceId,
    },
    /// A loop has no use.
    EmptyLoop {
        /// The face.
        face: FaceId,
        /// The loop's index in the face.
        lp: usize,
    },
    /// A use ends at a vertex other than the one the next use of its loop starts at.
    LoopNotConnected {
        /// The face.
        face: FaceId,
        /// The loop's index in the face.
        lp: usize,
        /// The use's position in the loop.
        at: usize,
    },
    /// A face uses one edge twice in the same sense: its two fins could not be told apart.
    RepeatedUse {
        /// The face.
        face: FaceId,
        /// The edge.
        edge: EdgeId,
        /// The sense.
        sense: Sense,
    },
    /// A use has no fin on its edge.
    UseWithoutFin {
        /// The face.
        face: FaceId,
        /// The edge.
        edge: EdgeId,
        /// The use's sense.
        sense: Sense,
    },
    /// An edge lists one fin twice.
    RepeatedFin {
        /// The edge.
        edge: EdgeId,
        /// The fin.
        fin: Fin,
    },
    /// A point use of an edge that has a curve: only a collapsed edge's use may be a point.
    PointUseOfCurve {
        /// The face.
        face: FaceId,
        /// The edge.
        edge: EdgeId,
    },
    /// A pcurve over an interval other than its edge's (pcurves share the edge's parameter).
    PcurveInterval {
        /// The face.
        face: FaceId,
        /// The edge.
        edge: EdgeId,
        /// The use's sense.
        sense: Sense,
    },
    /// One vertex on two loops of a face (the pin's Face 39): the loops touch, so they are
    /// one loop.
    VertexOnTwoLoops {
        /// The face.
        face: FaceId,
        /// The vertex.
        vertex: VertexId,
    },
    /// A collapsed edge whose two ends are different vertices.
    CollapsedEnds {
        /// The edge.
        edge: EdgeId,
    },
    /// An edge with no fins that no shell lists as a wire.
    LooseEdge {
        /// The edge.
        edge: EdgeId,
    },
    /// Two radially consecutive fins disagree about the shell between them: the side the
    /// first faces after it (front for a forward use, back for a reversed one, turning
    /// about the edge by the right-hand rule) is not the side the second faces before it.
    /// A lone fin is its own neighbour, so its face has one shell on both sides.
    SidesDisagree {
        /// The edge.
        edge: EdgeId,
        /// The two fins, in radial order.
        fins: [Fin; 2],
    },
    /// A vertex no edge ends at and no shell lists.
    LooseVertex {
        /// The vertex.
        vertex: VertexId,
    },
    /// A shell with no face side, wire or lone vertex.
    EmptyShell {
        /// The shell.
        shell: ShellId,
    },
    /// A shell in two or more pieces, joined neither across edges nor at vertices.
    ShellNotConnected {
        /// The shell.
        shell: ShellId,
    },
    /// A region with no shell: one other than the outside, or the outside of a body that
    /// has shells.
    RegionWithoutShell {
        /// The region.
        region: RegionId,
    },
}

impl StructureCause {
    /// The stable name.
    #[must_use]
    pub const fn name(&self) -> &'static str {
        match self {
            Self::Outside => "outside_region",
            Self::Dangling { .. } => "dangling_reference",
            Self::FinWithoutUse { .. } => "fin_without_use",
            Self::WireWithFins { .. } => "wire_with_fins",
            Self::FaceWithoutLoop { .. } => "face_without_loop",
            Self::EmptyLoop { .. } => "empty_loop",
            Self::LoopNotConnected { .. } => "loop_not_connected",
            Self::RepeatedUse { .. } => "repeated_use",
            Self::UseWithoutFin { .. } => "use_without_fin",
            Self::RepeatedFin { .. } => "repeated_fin",
            Self::PointUseOfCurve { .. } => "point_use_of_curve",
            Self::PcurveInterval { .. } => "pcurve_interval",
            Self::VertexOnTwoLoops { .. } => "vertex_on_two_loops",
            Self::CollapsedEnds { .. } => "collapsed_edge_ends",
            Self::LooseEdge { .. } => "loose_edge",
            Self::SidesDisagree { .. } => "sides_disagree",
            Self::LooseVertex { .. } => "loose_vertex",
            Self::EmptyShell { .. } => "empty_shell",
            Self::ShellNotConnected { .. } => "shell_not_connected",
            Self::RegionWithoutShell { .. } => "region_without_shell",
        }
    }
}

/// `forward` or `reversed`.
const fn sense_word(sense: Sense) -> &'static str {
    match sense {
        Sense::Forward => "forward",
        Sense::Reversed => "reversed",
    }
}

impl fmt::Display for StructureCause {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match *self {
            Self::Outside => f.write_str("the outside region is missing or solid"),
            Self::Dangling { from, to } => write!(f, "{from} names {to}, which is not there"),
            Self::FinWithoutUse { edge, fin } => write!(
                f,
                "edge {edge} has a fin on face {} with no use in that sense",
                fin.face
            ),
            Self::WireWithFins { shell, edge } => {
                write!(
                    f,
                    "shell {shell} lists edge {edge} as a wire, but it has fins"
                )
            }
            Self::FaceWithoutLoop { face } => write!(f, "face {face} has no loop"),
            Self::EmptyLoop { face, lp } => write!(f, "loop {lp} of face {face} is empty"),
            Self::LoopNotConnected { face, lp, at } => write!(
                f,
                "use {at} of loop {lp} of face {face} ends where the next use does not start"
            ),
            Self::RepeatedUse { face, edge, sense } => write!(
                f,
                "face {face} uses edge {edge} twice, {}",
                sense_word(sense)
            ),
            Self::UseWithoutFin { face, edge, sense } => write!(
                f,
                "face {face}'s {} use of edge {edge} has no fin",
                sense_word(sense)
            ),
            Self::RepeatedFin { edge, fin } => {
                write!(f, "edge {edge} lists its fin on face {} twice", fin.face)
            }
            Self::PointUseOfCurve { face, edge } => write!(
                f,
                "face {face} uses edge {edge} as a point, but the edge has a curve"
            ),
            Self::PcurveInterval { face, edge, sense } => write!(
                f,
                "face {face}'s {} pcurve of edge {edge} is not over the edge's interval",
                sense_word(sense)
            ),
            Self::VertexOnTwoLoops { face, vertex } => {
                write!(f, "vertex {vertex} is on two loops of face {face}")
            }
            Self::CollapsedEnds { edge } => {
                write!(f, "collapsed edge {edge} has two different end vertices")
            }
            Self::LooseEdge { edge } => {
                write!(f, "edge {edge} has no fin and is no shell's wire")
            }
            Self::SidesDisagree { edge, fins } => write!(
                f,
                "about edge {edge}, the fins on faces {} and {} name different shells \
                 between them",
                fins[0].face, fins[1].face
            ),
            Self::LooseVertex { vertex } => {
                write!(
                    f,
                    "vertex {vertex} ends no edge and is no shell's lone vertex"
                )
            }
            Self::EmptyShell { shell } => write!(f, "shell {shell} bounds nothing"),
            Self::ShellNotConnected { shell } => write!(f, "shell {shell} is in pieces"),
            Self::RegionWithoutShell { region } => write!(f, "region {region} has no shell"),
        }
    }
}

/// How a draft's geometry fails to fit its structure: the cheap check (research 60 V1),
/// at a fixed set of samples per use. Distances are in millimetres; `allowed` is the bound
/// the check held the distance to. `at` is a parameter of the edge (and of its pcurves).
#[derive(Debug, Clone, Copy, PartialEq)]
#[non_exhaustive]
pub enum GeometryCause {
    /// An edge's curve ends farther from its vertex than the two accuracies allow (the
    /// pin's Edge 16, 17).
    EdgeEndOffVertex {
        /// The edge.
        edge: EdgeId,
        /// The vertex.
        vertex: VertexId,
        /// The distance.
        gap: Length,
        /// The bound: the edge's reach plus the vertex's.
        allowed: Length,
    },
    /// An edge no longer than its own accuracy (the pin's Edge 7).
    EdgeTooShort {
        /// The edge.
        edge: EdgeId,
        /// The length of its sampled polygon.
        length: Length,
        /// The edge's reach.
        allowed: Length,
    },
    /// A use's sample leaves its surface's domain in a direction with an end.
    OutsideDomain {
        /// The face.
        face: FaceId,
        /// The edge.
        edge: EdgeId,
        /// The use's sense.
        sense: Sense,
        /// Where.
        at: f64,
    },
    /// A pcurve, lifted onto its surface, stands off its edge's curve by more than its
    /// recorded bound.
    PcurveOffEdge {
        /// The face.
        face: FaceId,
        /// The edge.
        edge: EdgeId,
        /// The use's sense.
        sense: Sense,
        /// Where.
        at: f64,
        /// The distance.
        gap: Length,
        /// The bound: the use's reach.
        allowed: Length,
    },
    /// A use, lifted onto its surface, ends farther from its vertex than its accuracies
    /// allow.
    UseEndOffVertex {
        /// The face.
        face: FaceId,
        /// The edge.
        edge: EdgeId,
        /// The use's sense.
        sense: Sense,
        /// The vertex.
        vertex: VertexId,
        /// The distance.
        gap: Length,
        /// The bound: the use's reach plus the edge's and the vertex's.
        allowed: Length,
    },
    /// A pcurve jumps across a period between two samples (the pin's Edgeuse 7): two
    /// consecutive samples are most of a period apart in a closed direction.
    PeriodJump {
        /// The face.
        face: FaceId,
        /// The edge.
        edge: EdgeId,
        /// The use's sense.
        sense: Sense,
        /// The later sample.
        at: f64,
    },
    /// A pcurve runs along a pole's iso line (the pin's Edgeuse 20, a dog-leg): travel
    /// along a singular side belongs to a collapsed edge of its own.
    DogLeg {
        /// The face.
        face: FaceId,
        /// The edge.
        edge: EdgeId,
        /// The use's sense.
        sense: Sense,
        /// A sample on the pole's line away from the end's `u`.
        at: f64,
    },
    /// A pole inside a use, not at a vertex (the pin's Face 24).
    PoleNotVertex {
        /// The face.
        face: FaceId,
        /// The edge.
        edge: EdgeId,
        /// The use's sense.
        sense: Sense,
        /// The sample at the pole.
        at: f64,
    },
    /// A loop is open in uv: a use ends at a uv the next one does not start at. Across a
    /// seam this means the loop lacks the seam edge; at a pole, the collapsed edge.
    LoopOpenInUv {
        /// The face.
        face: FaceId,
        /// The loop's index in the face.
        lp: usize,
        /// The use's position in the loop.
        at: usize,
        /// The larger of the two coordinate gaps.
        gap: f64,
        /// The bound, in uv.
        allowed: f64,
    },
    /// A loop turns the wrong way in uv: the first loop must enclose its area
    /// counter-clockwise seen from the front, and every other loop clockwise.
    LoopOrientation {
        /// The face.
        face: FaceId,
        /// The loop's index in the face.
        lp: usize,
    },
    /// A face's loops span more than a period in a closed direction: the face overlaps
    /// itself.
    FaceWraps {
        /// The face.
        face: FaceId,
        /// `0` for `u`, `1` for `v`.
        direction: usize,
    },
    /// An edge's fins are out of radial order: at a sample of the edge, the directions in
    /// which their faces leave it, taken in the fins' order, turn more than once about the
    /// edge's direction (the order is counter-clockwise, by the right-hand rule). Faces
    /// tangent along the edge are ordered by how they bend away from each other.
    RadialOrder {
        /// The edge.
        edge: EdgeId,
        /// The edge's parameter at the sample.
        at: f64,
    },
}

impl GeometryCause {
    /// The stable name.
    #[must_use]
    pub const fn name(&self) -> &'static str {
        match self {
            Self::EdgeEndOffVertex { .. } => "edge_end_off_vertex",
            Self::EdgeTooShort { .. } => "edge_too_short",
            Self::OutsideDomain { .. } => "outside_domain",
            Self::PcurveOffEdge { .. } => "pcurve_off_edge",
            Self::UseEndOffVertex { .. } => "use_end_off_vertex",
            Self::PeriodJump { .. } => "period_jump",
            Self::DogLeg { .. } => "dog_leg",
            Self::PoleNotVertex { .. } => "pole_not_vertex",
            Self::LoopOpenInUv { .. } => "loop_open_in_uv",
            Self::LoopOrientation { .. } => "loop_orientation",
            Self::FaceWraps { .. } => "face_wraps",
            Self::RadialOrder { .. } => "radial_order",
        }
    }
}

impl fmt::Display for GeometryCause {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match *self {
            Self::EdgeEndOffVertex {
                edge,
                vertex,
                gap,
                allowed,
            } => write!(
                f,
                "edge {edge} ends {} mm from vertex {vertex}, more than {} mm",
                gap.mm(),
                allowed.mm()
            ),
            Self::EdgeTooShort {
                edge,
                length,
                allowed,
            } => write!(
                f,
                "edge {edge} is {} mm long, no longer than its accuracy {} mm",
                length.mm(),
                allowed.mm()
            ),
            Self::OutsideDomain {
                face,
                edge,
                sense,
                at,
            } => write!(
                f,
                "face {face}'s {} use of edge {edge} leaves the surface's domain at {at}",
                sense_word(sense)
            ),
            Self::PcurveOffEdge {
                face,
                edge,
                sense,
                at,
                gap,
                allowed,
            } => write!(
                f,
                "face {face}'s {} pcurve of edge {edge} is {} mm off the edge at {at}, more \
                 than {} mm",
                sense_word(sense),
                gap.mm(),
                allowed.mm()
            ),
            Self::UseEndOffVertex {
                face,
                edge,
                sense,
                vertex,
                gap,
                allowed,
            } => write!(
                f,
                "face {face}'s {} use of edge {edge} ends {} mm from vertex {vertex}, more \
                 than {} mm",
                sense_word(sense),
                gap.mm(),
                allowed.mm()
            ),
            Self::PeriodJump {
                face,
                edge,
                sense,
                at,
            } => write!(
                f,
                "face {face}'s {} pcurve of edge {edge} jumps across a period before {at}",
                sense_word(sense)
            ),
            Self::DogLeg {
                face,
                edge,
                sense,
                at,
            } => write!(
                f,
                "face {face}'s {} pcurve of edge {edge} runs along a pole at {at}",
                sense_word(sense)
            ),
            Self::PoleNotVertex {
                face,
                edge,
                sense,
                at,
            } => write!(
                f,
                "face {face}'s {} use of edge {edge} passes a pole at {at}, not at a vertex",
                sense_word(sense)
            ),
            Self::LoopOpenInUv {
                face,
                lp,
                at,
                gap,
                allowed,
            } => write!(
                f,
                "loop {lp} of face {face} is open in uv after use {at}: {gap} apart, more \
                 than {allowed}"
            ),
            Self::LoopOrientation { face, lp } => {
                write!(f, "loop {lp} of face {face} turns the wrong way in uv")
            }
            Self::FaceWraps { face, direction } => write!(
                f,
                "face {face} spans more than a period in {}",
                if direction == 0 { "u" } else { "v" }
            ),
            Self::RadialOrder { edge, at } => write!(
                f,
                "the fins of edge {edge} are out of radial order at parameter {at}"
            ),
        }
    }
}

/// A draft `finish` refused, with the cause.
#[derive(Debug, Clone)]
pub struct Unfinished {
    /// The draft, unchanged.
    pub draft: Draft,
    /// Why.
    pub cause: FinishCause,
}

impl Unfinished {
    /// The cause's kind.
    #[must_use]
    pub const fn kind(&self) -> ErrorKind {
        self.cause.kind()
    }

    /// The cause's name.
    #[must_use]
    pub const fn name(&self) -> &'static str {
        self.cause.name()
    }
}

impl fmt::Display for Unfinished {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "the draft is not a body: {}", self.cause)
    }
}

impl core::error::Error for Unfinished {
    fn source(&self) -> Option<&(dyn core::error::Error + 'static)> {
        Some(&self.cause)
    }
}
