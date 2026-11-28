// Written for solidcorex. Apache-2.0.
//! What a stitch is asked for and what it says besides the body.

use scx_brep::op::Done;
use scx_brep::{EdgeId, Entity, Operand, VertexId};
use scx_num::units::Length;

/// What the caller wants of the stitch.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum Want {
    /// The sheets joined where they meet, open edges left open and reported.
    Sheets,
    /// A closed, oriented body: refused when an edge is left open.
    Solid,
}

/// A stitch has no items to skip: it finishes whole or fails. An edge it leaves open is
/// not a skipped item but a fact about the result, so it is in [`StitchReport::left`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NoItems {}

/// A finished stitch: the body with its lineage and report, and what the stitch joined.
#[derive(Debug, Clone)]
pub struct Stitched {
    /// The body, its [`Changes`](scx_brep::Changes) (sheet `i` is operand `i`; sheet 0 is
    /// the body edited, so its entities keep their ids) and its report: a
    /// [`Coincident`](scx_brep::op::WarningKind::Coincident) warning on each identified
    /// vertex and joined edge, and a [`Raise`](scx_brep::op::Raise) where an
    /// identification or a split grew a vertex's accuracy.
    pub done: Done<NoItems>,
    /// What the stitch joined.
    pub report: StitchReport,
}

/// What a stitch joined.
#[derive(Debug, Clone, Default, PartialEq)]
#[non_exhaustive]
pub struct StitchReport {
    /// Each vertex of the result that stands for vertices of two or more sheets, in the
    /// result's id order.
    pub identified: Vec<Identified>,
    /// Each edge of one sheet cut at another sheet's vertex that lies within coincidence of
    /// its inside, in the order of the result's (cut) edge, then the vertex.
    pub split: Vec<Split>,
    /// Each edge of the result that stands for an edge of two sheets, in the result's id
    /// order.
    pub joined: Vec<Joined>,
    /// What the stitch left open because it could not prove the answer, in the order of
    /// the result's ids.
    pub left: Vec<Left>,
}

impl StitchReport {
    /// The pairs and touches left [`Undecided`](Left::Undecided) at their edge's budget
    /// share or by precision: neither proved within coincidence nor beyond it.
    #[must_use]
    pub fn undecided(&self) -> usize {
        self.left
            .iter()
            .filter(|l| matches!(l, Left::Undecided { .. }))
            .count()
    }
}

/// An edge cut at another sheet's vertex.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub struct Split {
    /// The result's edge from the edge's start to the vertex (the cut edge keeps its id
    /// for this piece; the rest are new edges, each derived from it).
    pub edge: EdgeId,
    /// The result's vertex the edge was cut at.
    pub vertex: VertexId,
    /// The sheet edge that was cut.
    pub from: (Operand, EdgeId),
    /// The certified upper bound on the vertex's distance from the edge's curve.
    pub gap: Length,
}

/// An edge of the result that stands for an edge of each of two sheets.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub struct Joined {
    /// The result's edge: the kept edge (or a piece of it, after a split).
    pub edge: EdgeId,
    /// The certified upper bound on the symmetric Hausdorff distance of the two curves.
    pub gap: Length,
}

/// What a stitch left open, typed: never guessed.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub enum Left {
    /// An edge with two or more partners that may be within coincidence of it (proved
    /// within, or undecided): none of them is joined to it, since a choice would be a
    /// guess. Three sheets along one line give this.
    Ambiguous {
        /// The result's edge.
        edge: EdgeId,
        /// Its partners, in id order.
        partners: Vec<EdgeId>,
    },
    /// A pair of edges, or a vertex and an edge, whose gap's bracket straddles the
    /// coincidence where the edge's budget share ran out or floating point could cut the
    /// search no finer. Neither joined nor split.
    Undecided {
        /// The result's edge (for a touch, the cut edge's first piece).
        edge: EdgeId,
        /// The partner: another edge or a vertex.
        with: Entity,
        /// The gap is at least this.
        lower: Length,
        /// The gap is at most this.
        upper: Length,
        /// The coincidence it was tested against.
        allowance: Length,
    },
}

impl Left {
    /// The result's edge it names first.
    #[must_use]
    pub const fn edge(&self) -> EdgeId {
        match self {
            Self::Ambiguous { edge, .. } | Self::Undecided { edge, .. } => *edge,
        }
    }
}

/// A vertex of the result that stands for vertices of two or more sheets.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub struct Identified {
    /// The result's vertex.
    pub vertex: VertexId,
    /// The sheets' vertices it stands for, by sheet then id; one per sheet.
    pub members: Vec<(Operand, VertexId)>,
    /// The sheet vertex whose record it keeps: its point is the merged point.
    pub kept: (Operand, VertexId),
    /// The farthest member's distance from the merged point.
    pub spread: Length,
}
