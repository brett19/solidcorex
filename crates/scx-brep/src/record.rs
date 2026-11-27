// Written for solidcorex. Apache-2.0.
//! The records (01 §2.1, C4): what a body stores per entity, geometry by value.
//!
//! Every record is `#[non_exhaustive]` with public fields: read anywhere, built only by its
//! constructor and written only through a [`Draft`](crate::Draft), whose mutable accessors
//! keep the cell rules (01 §3.2). No record holds a pointer to another: references are ids.
//!
//! Orientation, kept from the pin's stored form (`solidcore-record/src/brep.rs`, READ):
//!
//! - A face's loops are taken about its surface's natural normal. The outer loop runs
//!   counter-clockwise in uv seen from the front, the inner loops clockwise.
//! - A use's pcurve runs over the edge's own interval, min to max, whatever the use's sense.
//! - An edge's fins are in radial order: counter-clockwise about the edge's direction (the
//!   right-hand rule).
//!
//! Accuracy (01 §4.1): an edge's, a vertex's and a use's recorded bound are each an
//! `Option<Arc<Recorded>>`, `None` meaning "within the resolution stamped on the body". The
//! `Arc` keeps the common `None` at 8 bytes: a `Recorded` is 72, which inline would nearly
//! double a use record. A face has no stored accuracy: it is derived from its uses' bounds,
//! and is a cell (BR4).

use std::sync::Arc;

use scx_curve::{Curve, Curve2};
use scx_num::cert::Recorded;
use scx_num::extent1d::Extent1d;
use scx_num::frame::Sense;
use scx_num::vector2d::Point2;
use scx_num::vector3d::Point3;
use scx_surface::Surface;

use crate::cells::{EdgeCells, FaceCells};
use crate::id::{EdgeId, FaceId, RegionId, ShellId, VertexId};

/// A region: a volume, solid or void. [`RegionId::OUTSIDE`] is never solid.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub struct RegionRecord {
    /// Whether the region is material.
    pub solid: bool,
}

impl RegionRecord {
    /// A region, solid or void.
    #[must_use]
    pub const fn new(solid: bool) -> Self {
        Self { solid }
    }
}

/// A shell: the region it bounds, its wire edges and its lone vertices. Its face sides are
/// the faces whose `sides` name it; they are found, not stored, so that a face's sides are
/// the one record of which shell it bounds.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub struct ShellRecord {
    /// The region the shell bounds.
    pub region: RegionId,
    /// Edges in the shell with no face: their adjacency is empty.
    pub wires: Vec<EdgeId>,
    /// Vertices in the shell on no edge.
    pub lone_vertices: Vec<VertexId>,
}

impl ShellRecord {
    /// A shell bounding `region`, with no wires or lone vertices.
    #[must_use]
    pub const fn new(region: RegionId) -> Self {
        Self {
            region,
            wires: Vec::new(),
            lone_vertices: Vec::new(),
        }
    }
}

/// Which side of a face: the front is the side its surface's natural normal points into.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum FaceSide {
    /// The side the natural normal points into.
    Front,
    /// The other side.
    Back,
}

impl FaceSide {
    /// The other side.
    #[must_use]
    pub const fn other(self) -> Self {
        match self {
            Self::Front => Self::Back,
            Self::Back => Self::Front,
        }
    }

    /// Both sides, front first: the order of [`FaceRecord::sides`].
    pub const BOTH: [Self; 2] = [Self::Front, Self::Back];
}

/// A face: its surface, the shell on each side, and its loops.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub struct FaceRecord {
    /// The surface, by value; its sense fixes the natural normal.
    pub surface: Surface,
    /// The shell on each side: front, then back.
    pub sides: [ShellId; 2],
    /// The loops: the outer loop first, when the face has one.
    pub loops: Vec<LoopRecord>,
    pub(crate) cells: FaceCells,
}

impl FaceRecord {
    /// A face on `surface` with `sides` (front, back) and `loops`.
    #[must_use]
    pub fn new(surface: Surface, sides: [ShellId; 2], loops: Vec<LoopRecord>) -> Self {
        Self {
            surface,
            sides,
            loops,
            cells: FaceCells::default(),
        }
    }

    /// The shell on `side`.
    #[must_use]
    pub const fn side(&self, side: FaceSide) -> ShellId {
        match side {
            FaceSide::Front => self.sides[0],
            FaceSide::Back => self.sides[1],
        }
    }
}

/// A loop: a closed cycle of uses, each ending where the next starts.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub struct LoopRecord {
    /// The uses, in order.
    pub uses: Vec<EdgeUse>,
}

impl LoopRecord {
    /// A loop of `uses`.
    #[must_use]
    pub const fn new(uses: Vec<EdgeUse>) -> Self {
        Self { uses }
    }
}

/// One pass of a face's loop along an edge: the edge, which way, and its image in the
/// face's uv. A seam edge has two uses in one face, of opposite sense.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub struct EdgeUse {
    /// The edge.
    pub edge: EdgeId,
    /// Whether the loop runs with the edge's direction or against it.
    pub sense: Sense,
    /// The edge's image in the face's uv.
    pub geom: UseGeom,
    /// The pcurve's recorded bound from the surface and the edge's curve; `None` within the
    /// body's resolution.
    pub bound: Option<Arc<Recorded>>,
}

impl EdgeUse {
    /// A use of `edge` in `sense` with image `geom`, within the resolution.
    #[must_use]
    pub const fn new(edge: EdgeId, sense: Sense, geom: UseGeom) -> Self {
        Self {
            edge,
            sense,
            geom,
            bound: None,
        }
    }
}

/// A use's image in its face's uv.
#[derive(Debug, Clone, PartialEq)]
pub enum UseGeom {
    /// A pcurve over the edge's own interval.
    Pcurve {
        /// The curve in uv.
        curve: Curve2,
        /// Its parameter interval, the edge's.
        interval: Extent1d,
    },
    /// A point: the use of a collapsed edge (a lone vertex inside a face, or a pole).
    Point(Point2),
}

/// An edge's shape (63 D8): a curve over an interval, or collapsed to its vertex.
#[derive(Debug, Clone, PartialEq)]
pub enum EdgeShape {
    /// A curve over a finite interval.
    Curve {
        /// The curve.
        curve: Curve,
        /// The interval of it the edge runs over, start to end.
        interval: Extent1d,
    },
    /// No curve: the edge is collapsed to a point, its two ends the same vertex.
    Collapsed,
}

/// An edge's geometry part: its shape, end vertices and accuracy. A write to it resets the
/// cells of every face that uses the edge.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub struct EdgeGeom {
    /// The shape.
    pub shape: EdgeShape,
    /// The start and end vertices; the same vertex for a closed or collapsed edge.
    pub ends: [VertexId; 2],
    /// The edge's recorded accuracy; `None` within the body's resolution.
    pub accuracy: Option<Arc<Recorded>>,
    pub(crate) cells: EdgeCells,
}

impl EdgeGeom {
    /// An edge of `shape` from `ends[0]` to `ends[1]`, within the resolution.
    #[must_use]
    pub fn new(shape: EdgeShape, ends: [VertexId; 2]) -> Self {
        Self {
            shape,
            ends,
            accuracy: None,
            cells: EdgeCells::default(),
        }
    }

    /// The vertex a use in `sense` starts at.
    #[must_use]
    pub const fn start(&self, sense: Sense) -> VertexId {
        match sense {
            Sense::Forward => self.ends[0],
            Sense::Reversed => self.ends[1],
        }
    }

    /// The vertex a use in `sense` ends at.
    #[must_use]
    pub const fn end(&self, sense: Sense) -> VertexId {
        self.start(sense.reversed())
    }
}

/// One face's pass through an edge, in the edge's radial order: the face and the sense of
/// its use. A seam lists its face twice, once per sense.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Fin {
    /// The face.
    pub face: FaceId,
    /// The sense of the face's use of the edge.
    pub sense: Sense,
}

impl Fin {
    /// The pass of `face`'s use in `sense`.
    #[must_use]
    pub const fn new(face: FaceId, sense: Sense) -> Self {
        Self { face, sense }
    }
}

/// An edge's adjacency part: its fins in radial order. A wire edge has none. A write to it
/// resets no cell, so a merge can reorder fins without touching geometry.
#[derive(Debug, Clone, Default, PartialEq)]
#[non_exhaustive]
pub struct EdgeAdj {
    /// The fins, counter-clockwise about the edge's direction.
    pub fins: Vec<Fin>,
}

/// A vertex: its point and accuracy. A write to it resets the cells of its edges and of
/// their faces.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub struct VertexRecord {
    /// The point.
    pub point: Point3,
    /// The vertex's recorded accuracy; `None` within the body's resolution.
    pub accuracy: Option<Arc<Recorded>>,
}

impl VertexRecord {
    /// A vertex at `point`, within the resolution.
    #[must_use]
    pub const fn new(point: Point3) -> Self {
        Self {
            point,
            accuracy: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ends_follow_the_sense() {
        let (a, b) = (VertexId::from_raw(1), VertexId::from_raw(2));
        let g = EdgeGeom::new(EdgeShape::Collapsed, [a, b]);
        assert_eq!((g.start(Sense::Forward), g.end(Sense::Forward)), (a, b));
        assert_eq!((g.start(Sense::Reversed), g.end(Sense::Reversed)), (b, a));
        assert_eq!(FaceSide::Front.other(), FaceSide::Back);
    }
}
