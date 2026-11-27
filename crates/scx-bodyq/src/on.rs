// Written for solidcorex. Apache-2.0.
//! Step 1: the boundary entity a point lies on, if any.

use scx_brep::{Brep, EdgeId, Entity, UvSide, VertexId};
use scx_num::budget::{Budget, Stop};
use scx_num::extent3d::Extent3d;
use scx_num::vector3d::Point3;

use crate::error::BodyqError;
use crate::geom::{coincidence, edge_foot, face_foot, recorded};

/// The vertex, else the edge, else the face `p` lies on within coincidence, each the lowest
/// id of its dimension; `None` when it lies on none.
///
/// The candidates are the faces whose box holds `p` (every box is grown by the entity's
/// accuracy and the resolution, and a face's accuracy covers its edges' and vertices'), and
/// their edges and vertices. A face counts when its foot's uv is `Inside` it. A foot only
/// `Near` the face's boundary gives `On` through that boundary's edge, when the edge is
/// within coincidence (72 §12.4 row 2); on a valid body it is, since the face's accuracy
/// covers its edges. A wire edge or a lone vertex bounds no face and is not found.
pub(crate) fn on_entity(
    body: &Brep,
    p: Point3,
    budget: &mut Budget,
    stop: &Stop,
) -> Result<Option<Entity>, BodyqError> {
    let Some(pt) = Extent3d::from_point(p) else {
        return Ok(None);
    };
    let mut on_face = None;
    let mut edges: Vec<EdgeId> = Vec::new();
    for face in body.faces_meeting(pt) {
        stop.check()?;
        if face
            .bounding_box()
            .is_some_and(|b| !b.contains_point(p, 0.0))
        {
            continue;
        }
        edges.extend(face.uses().map(|u| u.edge().id()));
        if on_face.is_some() {
            continue;
        }
        let foot = face_foot(face, p, budget, stop)?;
        if foot.distance <= coincidence(body, face.accuracy()) && foot.side == Some(UvSide::Inside)
        {
            // Faces come in id order, so the first is the lowest.
            on_face = Some(face.id());
        }
    }
    edges.sort_unstable();
    edges.dedup();
    let mut on_edge = None;
    let mut vertices: Vec<VertexId> = Vec::new();
    for id in edges {
        let Some(edge) = body.edge(id) else { continue };
        if edge
            .bounding_box()
            .is_some_and(|b| !b.contains_point(p, 0.0))
        {
            continue;
        }
        vertices.extend(edge.ends());
        if on_edge.is_some() {
            continue;
        }
        if let Some((_, _, d)) = edge_foot(edge, p, budget)?
            && d <= coincidence(body, recorded(edge.accuracy()))
        {
            on_edge = Some(id);
        }
    }
    vertices.sort_unstable();
    vertices.dedup();
    for id in vertices {
        let Some(v) = body.vertex(id) else { continue };
        if v.point().distance(p) <= coincidence(body, recorded(v.accuracy())) {
            return Ok(Some(Entity::Vertex(id)));
        }
    }
    Ok(on_edge
        .map(Entity::Edge)
        .or_else(|| on_face.map(Entity::Face)))
}
