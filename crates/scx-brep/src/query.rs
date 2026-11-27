// Written for solidcorex. Apache-2.0.
//! The reads cells serve (research 63 §6.9): a face's accuracy, uv box, box and
//! point-in-face test, an edge's box. Every box is a certified enclosure, grown by the
//! entity's accuracy and the body's resolution, so it holds every point of the exact entity
//! and every place within the resolution of one.

use scx_num::cert::Recorded;
use scx_num::extent2d::Extent2d;
use scx_num::extent3d::Extent3d;
use scx_num::interval::round_up;
use scx_num::units::{Accuracy, Resolution};
use scx_num::vector2d::Point2;

use crate::boundary::{Boundary, UvSide};
use crate::cells::{FaceCell, SurfaceBox, curve_box, grown};
use crate::record::{EdgeGeom, EdgeShape, FaceRecord, VertexRecord};
use crate::tables::Tables;
use crate::view::{Edge, Face, Finished, Stage};

impl<S: Stage> Face<'_, S> {
    /// A box holding the face's region in uv: the hull of its loops when it has an outer
    /// loop, one window of its surface's domain when it is holes in the whole domain, and
    /// `None` when that is unbounded or a pcurve cannot be enclosed. Filled on first read.
    #[must_use]
    pub fn uv_box(self) -> Option<Extent2d> {
        Boundary::of(self.rec).uv_box()
    }

    /// Whether the face's region is proved: its loops' signed area (enclosed, refined
    /// until its sign shows) decides which side of them the face lies on. Unproved for a
    /// loop that does not close in uv or a sliver whose area the enclosures cannot sign;
    /// [`Face::contains_uv`] then answers `None` everywhere. Filled on first read.
    #[must_use]
    pub fn is_region_proved(self) -> bool {
        Boundary::of(self.rec).is_decided()
    }

    /// Where `p` lies against the face's region in uv: inside, outside, or near the
    /// boundary, as the pcurves' enclosures prove. `None` when they cannot tell (a loop
    /// whose orientation does not show, a point level with a vertex on every ray tried).
    /// The boundary tree is filled on first read.
    #[must_use]
    pub fn contains_uv(self, p: Point2) -> Option<UvSide> {
        Boundary::of(self.rec).side(self.rec, p)
    }
}

impl Face<'_, Finished> {
    /// The face's accuracy: the largest recorded bound of its uses, its edges and their
    /// end vertices; `None` when all are within the body's resolution. Derived on each
    /// read, in time linear in its uses.
    #[must_use]
    pub fn accuracy(self) -> Option<Accuracy> {
        face_accuracy(self.at.t, self.rec)
    }

    /// A box holding the face, grown by its accuracy and the body's resolution; `None` for
    /// a face that reaches infinity or cannot be enclosed.
    #[must_use]
    pub fn bounding_box(self) -> Option<Extent3d> {
        face_box(self.at.t, self.rec, Finished::look(self.at.body).resolution)
    }
}

impl Edge<'_, Finished> {
    /// A box holding the edge, grown by its accuracy and the body's resolution (for a
    /// collapsed edge, its vertex's box); `None` for a curve that cannot be enclosed.
    #[must_use]
    pub fn bounding_box(self) -> Option<Extent3d> {
        edge_box(
            self.at.t,
            self.geom,
            Finished::look(self.at.body).resolution,
        )
    }
}

/// The larger of two recorded accuracies, either optional.
fn worse(a: Option<Accuracy>, r: Option<&Recorded>) -> Option<Accuracy> {
    match (a, r.map(|r| r.acc)) {
        (Some(a), Some(b)) => Some(if b > a { b } else { a }),
        (a, b) => a.or(b),
    }
}

/// The face's accuracy (see [`Face::accuracy`]).
pub(crate) fn face_accuracy(t: &Tables, rec: &FaceRecord) -> Option<Accuracy> {
    let mut acc = None;
    for u in rec.loops.iter().flat_map(|l| &l.uses) {
        acc = worse(acc, u.bound.as_deref());
        if let Some(g) = t.edges.get(u.edge) {
            acc = worse(acc, g.accuracy.as_deref());
            for v in g.ends {
                acc = worse(acc, t.vertices.get(v).and_then(|v| v.accuracy.as_deref()));
            }
        }
    }
    acc
}

/// How far to grow a box for `acc` under `resolution`, rounded up.
fn margin(acc: Option<Accuracy>, resolution: Resolution) -> f64 {
    round_up(acc.map_or(0.0, |a| a.length().mm()) + resolution.zone().mm())
}

/// The face's grown box (see [`Face::bounding_box`]).
pub(crate) fn face_box(t: &Tables, rec: &FaceRecord, resolution: Resolution) -> Option<Extent3d> {
    let bx = SurfaceBox::of(rec).get();
    grown(bx?, margin(face_accuracy(t, rec), resolution))
}

/// The edge's grown box (see [`Edge::bounding_box`]).
pub(crate) fn edge_box(t: &Tables, g: &EdgeGeom, resolution: Resolution) -> Option<Extent3d> {
    let own = g.accuracy.as_deref().map(|r| r.acc);
    match g.shape {
        EdgeShape::Curve { .. } => grown(curve_box(g)?, margin(own, resolution)),
        EdgeShape::Collapsed => {
            let v = t.vertices.get(g.ends[0])?;
            let acc = worse(own, v.accuracy.as_deref());
            grown(Extent3d::from_point(v.point)?, margin(acc, resolution))
        }
    }
}

/// The vertex's grown box.
pub(crate) fn vertex_box(v: &VertexRecord, resolution: Resolution) -> Option<Extent3d> {
    let acc = v.accuracy.as_deref().map(|r| r.acc);
    grown(Extent3d::from_point(v.point)?, margin(acc, resolution))
}
