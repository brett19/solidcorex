// Written for solidcorex. Apache-2.0.
//! [`pick_at`]: the faces, edges or vertices of a body within an aperture of points (research
//! 74 §44), and the distance to one entity of each kind that [`still_names`](crate::still_names)
//! shares.
//!
//! The pin's pick by points took the nearest entity of a kind, answered `Ambiguous` on a tie
//! and ended a batch at the first miss. Here every entity of the kind within the aperture of a
//! point is returned, sorted by (distance, id), and each point has its own answer (an empty
//! one is a point that picks nothing). The caller chooses.

use std::collections::BTreeSet;

use scx_brep::{Brep, Edge, EdgeId, Entity, EntityKind, Face, UvSide, VertexId};
use scx_num::budget::{Budget, Stop};
use scx_num::extent3d::Extent3d;
use scx_num::units::Length;
use scx_num::vector3d::Point3;

use crate::error::{BodyqDefect, BodyqError, BodyqRefusal};
use crate::geom::{coincidence, edge_foot, face_foot, recorded};

/// An entity within the aperture of a point.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Picked {
    /// The face, edge or vertex.
    pub entity: Entity,
    /// The distance from the point to the entity as it is trimmed: for a face, to the nearest
    /// point of the face itself, not of its surface.
    pub distance: f64,
    /// That nearest point.
    pub at: Point3,
}

/// For each of `points`, every entity of kind `want` (a face, edge or vertex) of `body` whose
/// distance from the point is within `aperture` of it (plus the entity's coincidence, so a
/// point on an entity is within an aperture of zero), sorted by (distance, id).
///
/// The distance is to the entity as the body holds it: a face's to its trimmed region
/// (the surface's foot when that lies in the face, else the nearest point of its boundary), an
/// edge's to its curve over its interval or to an end, a vertex's to its point. Faces come from
/// one window of the body's box tree for each point, grown by the aperture; edges and vertices
/// are those of the faces found, plus the body's wire edges and lone vertices, which no face
/// names and which are read in a scan over them.
///
/// A point on an edge of a cube is within every aperture of that edge, of both faces and of
/// its two ends when the aperture reaches them.
///
/// Work is charged to `budget` (one unit for each inversion) and `stop` is checked before each
/// entity; the same body and points charge the same work every time.
///
/// # Errors
///
/// - [`BodyqError::Refused`]: a point with a coordinate that is not finite
///   ([`NotFinite`](BodyqRefusal::NotFinite)); an aperture that is not finite or is negative
///   ([`Tolerance`](BodyqRefusal::Tolerance)); `want` a region or shell
///   ([`NotPickable`](BodyqRefusal::NotPickable)).
/// - [`BodyqError::Unsupported`] when a NURBS edge comes within the window (step 5 builds its
///   inversion).
/// - [`BodyqError::Exhausted`] and [`BodyqError::Cancelled`]; neither leaves anything behind.
/// - [`BodyqError::Defect`] on a body whose faces the method should take and cannot.
pub fn pick_at(
    body: &Brep,
    points: &[Point3],
    want: EntityKind,
    aperture: Length,
    budget: &mut Budget,
    stop: &Stop,
) -> Result<Vec<Vec<Picked>>, BodyqError> {
    if !matches!(
        want,
        EntityKind::Face | EntityKind::Edge | EntityKind::Vertex
    ) {
        return Err(BodyqError::Refused(BodyqRefusal::NotPickable {
            kind: want,
        }));
    }
    let aperture = aperture.mm();
    if !aperture.is_finite() || aperture < 0.0 {
        return Err(BodyqError::Refused(BodyqRefusal::Tolerance));
    }
    if points
        .iter()
        .any(|p| !(p.x.is_finite() && p.y.is_finite() && p.z.is_finite()))
    {
        return Err(BodyqError::Refused(BodyqRefusal::NotFinite));
    }
    points
        .iter()
        .map(|&p| pick_one(body, p, want, aperture, budget, stop))
        .collect()
}

fn pick_one(
    body: &Brep,
    p: Point3,
    want: EntityKind,
    aperture: f64,
    budget: &mut Budget,
    stop: &Stop,
) -> Result<Vec<Picked>, BodyqError> {
    // Every box is grown by its entity's accuracy and the resolution, so an entity within the
    // aperture (and its coincidence) of `p` has a box that meets this window.
    let reach = aperture + coincidence(body, None);
    let Some(window) = Extent3d::from_point(p).and_then(|b| b.expanded(reach)) else {
        return Ok(Vec::new());
    };
    let mut hits: Vec<Picked> = Vec::new();
    let mut edges: BTreeSet<EdgeId> = BTreeSet::new();
    let mut vertices: BTreeSet<VertexId> = BTreeSet::new();
    for face in body.faces_meeting(window) {
        stop.check()?;
        if want == EntityKind::Face {
            let (distance, at) = face_distance(body, face, p, budget, stop)?;
            if distance <= aperture + coincidence(body, face.accuracy()) {
                hits.push(Picked {
                    entity: Entity::Face(face.id()),
                    distance,
                    at,
                });
            }
        } else {
            edges.extend(face.uses().map(|u| u.edge().id()));
        }
    }
    if want != EntityKind::Face {
        for shell in body.shells() {
            edges.extend(shell.wires().map(|e| e.id()));
            vertices.extend(shell.lone_vertices().map(|v| v.id()));
        }
        for id in edges {
            stop.check()?;
            let Some(edge) = body.edge(id) else { continue };
            if edge
                .bounding_box()
                .is_some_and(|b| !b.contains_point(p, reach))
            {
                continue;
            }
            if want == EntityKind::Vertex {
                vertices.extend(edge.ends());
                continue;
            }
            let (distance, at) = edge_distance(body, edge, p, budget)?;
            if distance <= aperture + coincidence(body, recorded(edge.accuracy())) {
                hits.push(Picked {
                    entity: Entity::Edge(id),
                    distance,
                    at,
                });
            }
        }
        if want == EntityKind::Vertex {
            for id in vertices {
                let Some(v) = body.vertex(id) else { continue };
                let distance = v.point().distance(p);
                if distance <= aperture + coincidence(body, recorded(v.accuracy())) {
                    hits.push(Picked {
                        entity: Entity::Vertex(id),
                        distance,
                        at: v.point(),
                    });
                }
            }
        }
    }
    hits.sort_by(|a, b| {
        a.distance
            .total_cmp(&b.distance)
            .then(a.entity.cmp(&b.entity))
    });
    Ok(hits)
}

/// The distance from `p` to `edge` over its interval, and the nearest point: the curve's foot
/// when it lies inside the interval, else the nearer end; a collapsed edge is its vertex.
pub(crate) fn edge_distance(
    body: &Brep,
    edge: Edge<'_>,
    p: Point3,
    budget: &mut Budget,
) -> Result<(f64, Point3), BodyqError> {
    let mut best: Option<(f64, Point3)> = None;
    let mut offer = |d: f64, q: Point3| {
        if best.is_none_or(|(b, _)| d < b) {
            best = Some((d, q));
        }
    };
    if let Some((_, q, d)) = edge_foot(edge, p, budget)? {
        offer(d, q);
    }
    for id in edge.ends() {
        if let Some(v) = body.vertex(id) {
            offer(v.point().distance(p), v.point());
        }
    }
    best.ok_or(BodyqError::Defect(BodyqDefect::NoEdgeBox {
        edge: edge.id(),
    }))
}

/// The distance from `p` to `face` as it is trimmed, and the nearest point. The surface's
/// foot when its uv lies in the face; else the nearest point of the face's edges (the foot
/// itself too when its uv is only near the boundary, where it may be either side by less than
/// the boundary tree's width).
///
/// The foot is the surface's nearest point to `p`. The face is read as holding no other local
/// minimum of the distance, which is true of a plane, a cone, a sphere and a cylinder, and of a
/// torus except for a point inside its tube's hole.
pub(crate) fn face_distance(
    body: &Brep,
    face: Face<'_>,
    p: Point3,
    budget: &mut Budget,
    stop: &Stop,
) -> Result<(f64, Point3), BodyqError> {
    let foot = face_foot(face, p, budget, stop)?;
    let inside = match foot.side {
        Some(UvSide::Inside) => return Ok((foot.distance, foot.point)),
        Some(UvSide::Near) => true,
        Some(UvSide::Outside) => false,
        None => {
            return Err(BodyqError::Defect(BodyqDefect::NoUvSide {
                face: face.id(),
            }));
        }
    };
    let mut best = inside.then_some((foot.distance, foot.point));
    let mut seen: BTreeSet<EdgeId> = BTreeSet::new();
    for u in face.uses() {
        let edge = u.edge();
        if !seen.insert(edge.id()) {
            continue;
        }
        stop.check()?;
        let (d, q) = edge_distance(body, edge, p, budget)?;
        if best.is_none_or(|(b, _)| d < b) {
            best = Some((d, q));
        }
    }
    best.ok_or(BodyqError::Defect(BodyqDefect::NoBoundary))
}
