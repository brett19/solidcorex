// Written for solidcorex. Apache-2.0.
//! [`Mesh`]: the triangles and edge polylines of a body, sharing their points.

use scx_brep::{EdgeId, FaceId};
use scx_num::units::Length;
use scx_num::vector3d::{Point3, Vec3};

/// A body's mesh.
///
/// `positions` lists the body's live vertices in id order, then each edge's interior samples
/// in edge id order, then each face's interior points. Every triangle and polyline names its
/// points by index into it, so two faces meeting at an edge share the edge's indices: a
/// closed body gives a closed mesh (research 72 §6.2).
#[derive(Debug, Clone, PartialEq)]
pub struct Mesh {
    /// The points.
    pub positions: Vec<Point3>,
    /// The triangles, by face in id order, each counterclockwise seen from outside.
    pub triangles: Vec<[u32; 3]>,
    /// Each triangle's corner normals: the face's surface normal at each corner, so an index
    /// shared across a sharp edge still carries each face's own normal.
    pub normals: Vec<[Vec3; 3]>,
    /// Each triangle's face.
    pub face_of: Vec<FaceId>,
    /// Each live edge's polyline, in edge id order.
    pub edges: Vec<EdgePolyline>,
    /// Each meshed face's proved chord bound, in face id order.
    pub chord_bound: Vec<(FaceId, Length)>,
}

/// An edge's polyline in a [`Mesh`].
#[derive(Debug, Clone, PartialEq)]
pub struct EdgePolyline {
    /// The edge.
    pub edge: EdgeId,
    /// The point indices, start vertex to end vertex in the edge's own sense; a closed edge
    /// starts and ends at the same index; a collapsed edge is its vertex twice.
    pub points: Vec<u32>,
    /// The proved bound on the distance between the polyline and the edge's curve, each
    /// way; zero for a line or a collapsed edge.
    pub chord: Length,
}
