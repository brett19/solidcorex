// Written for solidcorex. Apache-2.0.
//! What a stitch is asked for and what it says besides the body.

use scx_brep::op::Done;
use scx_brep::{Operand, VertexId};
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

/// A stitch has no items to skip: it finishes whole or fails.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NoItems {}

/// A finished stitch: the body with its lineage and report, and what the stitch joined.
#[derive(Debug, Clone)]
pub struct Stitched {
    /// The body, its [`Changes`](scx_brep::Changes) (sheet `i` is operand `i`; sheet 0 is
    /// the body edited, so its entities keep their ids) and its report: a
    /// [`Coincident`](scx_brep::op::WarningKind::Coincident) warning on each identified
    /// vertex, and a [`Raise`](scx_brep::op::Raise) where the identification grew one.
    pub done: Done<NoItems>,
    /// What the stitch joined.
    pub report: StitchReport,
}

/// What a stitch joined.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub struct StitchReport {
    /// Each vertex of the result that stands for vertices of two or more sheets, in the
    /// result's id order.
    pub identified: Vec<Identified>,
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
