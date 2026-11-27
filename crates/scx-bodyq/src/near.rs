// Written for solidcorex. Apache-2.0.
//! Step 3: the region on the side of the nearest boundary point, when no ray is clean.
//!
//! The nearest point `q` of the boundary to `p` is a face's interior foot, an edge's
//! interior foot or a vertex. At a face's foot, `p - q` lies along the face's normal, so its
//! sign names the side. At an edge or a vertex the faces round it meet, and `p - q` lies in
//! their normal cone; the angle-weighted pseudonormal (Baerentzen and Aanaes, 2005: the sum
//! of the faces' normals, each weighted by the face's angle at the point, oriented towards
//! one of the two regions) has a positive product with every direction of the cone on that
//! region's side and a negative one on the other's. An edge's two faces each have angle pi,
//! so its pseudonormal is the plain sum.

use std::collections::BTreeSet;

use core::f64::consts::TAU;

use scx_brep::{
    Brep, Edge, EdgeShape, Entity, Face, FaceId, FaceSide, RegionId, Use, UvSide, Vertex,
};
use scx_num::budget::{Budget, Stop};
use scx_num::extent3d::Extent3d;
use scx_num::fmath;
use scx_num::frame::Sense;
use scx_num::vector3d::{Point3, Vec3};
use scx_nurbs::Approach;

use crate::error::{BodyqDefect, BodyqError, BodyqUnsupported};
use crate::geom::{ABOVE, edge_foot, face_foot};

/// A boundary point near `p`.
#[derive(Debug, Clone, Copy)]
enum Place {
    Vertex,
    Edge,
    Face { normal: Vec3 },
}

/// The nearest boundary point so far: distance, then dimension, then id, so ties go the same
/// way every time.
#[derive(Debug, Clone, Copy)]
struct Best {
    distance: f64,
    rank: (u8, u32),
    at: Entity,
    point: Point3,
    place: Place,
}

impl Best {
    fn better(&self, other: &Self) -> bool {
        self.distance
            .total_cmp(&other.distance)
            .then(self.rank.cmp(&other.rank))
            .is_lt()
    }
}

/// The region `p` lies in, read from the nearest boundary point (see the module's note).
/// `p` lies inside the body's box `body_box` and on no boundary entity.
pub(crate) fn nearest_region(
    body: &Brep,
    body_box: &Extent3d,
    p: Point3,
    budget: &mut Budget,
    stop: &Stop,
) -> Result<RegionId, BodyqError> {
    let reach = body_box.max_distance_to_point(p);
    let mut r = (body_box.max_length() / 64.0).max(f64::MIN_POSITIVE);
    let mut seen: BTreeSet<FaceId> = BTreeSet::new();
    let mut edges_seen = BTreeSet::new();
    let mut verts_seen = BTreeSet::new();
    let mut best: Option<Best> = None;
    loop {
        let window = Extent3d::from_point(p)
            .and_then(|b| b.expanded(r))
            .ok_or(BodyqError::Defect(BodyqDefect::NoBoundary))?;
        for face in body.faces_meeting(window) {
            if !seen.insert(face.id()) {
                continue;
            }
            stop.check()?;
            let foot = face_foot(face, p, budget, stop)?;
            if foot.side == Some(UvSide::Inside) {
                let normal = face.surface().normal(foot.uv, ABOVE).ok_or_else(|| {
                    BodyqError::Defect(BodyqDefect::NoNormal {
                        at: Entity::Face(face.id()),
                    })
                })?;
                offer(
                    &mut best,
                    Best {
                        distance: foot.distance,
                        rank: (2, face.id().raw()),
                        at: Entity::Face(face.id()),
                        point: foot.point,
                        place: Place::Face {
                            normal: normal.as_vec(),
                        },
                    },
                );
            }
            for u in face.uses() {
                let edge = u.edge();
                if edges_seen.insert(edge.id())
                    && let Some((_, q, d)) = edge_foot(edge, p, budget)?
                {
                    offer(
                        &mut best,
                        Best {
                            distance: d,
                            rank: (1, edge.id().raw()),
                            at: Entity::Edge(edge.id()),
                            point: q,
                            place: Place::Edge,
                        },
                    );
                }
                for v in [edge.start(), edge.end()] {
                    if verts_seen.insert(v.id()) {
                        offer(
                            &mut best,
                            Best {
                                distance: v.point().distance(p),
                                rank: (0, v.id().raw()),
                                at: Entity::Vertex(v.id()),
                                point: v.point(),
                                place: Place::Vertex,
                            },
                        );
                    }
                }
            }
        }
        // Every boundary point within `r` of `p` lies in the window, so a nearest one found
        // within `r` is the nearest of all.
        match best {
            Some(b) if b.distance <= r => break,
            _ if r >= reach => break,
            _ => r *= 2.0,
        }
    }
    let best = best.ok_or(BodyqError::Defect(BodyqDefect::NoBoundary))?;
    let w = p - best.point;
    match (best.place, best.at) {
        (Place::Face { normal }, Entity::Face(id)) => {
            let face = face_of(body, id)?;
            let side = if w.dot(normal) > 0.0 {
                FaceSide::Front
            } else {
                FaceSide::Back
            };
            Ok(face.shell(side).region().id())
        }
        (Place::Edge, Entity::Edge(id)) => {
            let edge = body
                .edge(id)
                .ok_or(BodyqError::Defect(BodyqDefect::NoBoundary))?;
            edge_side(body, edge, best.point, w, budget, stop)
        }
        (Place::Vertex, Entity::Vertex(id)) => {
            let v = body
                .vertex(id)
                .ok_or(BodyqError::Defect(BodyqDefect::NoBoundary))?;
            vertex_side(body, v, w, budget, stop)
        }
        _ => Err(BodyqError::Defect(BodyqDefect::NoBoundary)),
    }
}

fn offer(best: &mut Option<Best>, c: Best) {
    if best.is_none_or(|b| c.better(&b)) {
        *best = Some(c);
    }
}

fn face_of(body: &Brep, id: FaceId) -> Result<Face<'_>, BodyqError> {
    body.face(id)
        .ok_or(BodyqError::Defect(BodyqDefect::NoBoundary))
}

/// The regions on the sides of the faces round a point.
enum Around {
    /// Every side is this one region.
    One(RegionId),
    /// Two regions, the first the front of the lowest face, each face having one on each
    /// side.
    Two([RegionId; 2]),
}

/// The regions the faces round a point separate; more than two, or a face with one region
/// on both sides among two, is `NonManifold`.
fn two_regions(faces: &[Face<'_>], at: Entity) -> Result<Around, BodyqError> {
    let mut regions: Vec<RegionId> = Vec::with_capacity(4);
    for f in faces {
        for side in [FaceSide::Front, FaceSide::Back] {
            let r = f.shell(side).region().id();
            if !regions.contains(&r) {
                regions.push(r);
            }
        }
    }
    let manifold = BodyqError::Unsupported(BodyqUnsupported::NonManifold { at });
    match regions[..] {
        [r] => Ok(Around::One(r)),
        [a, b] => {
            if faces.iter().any(|f| {
                f.shell(FaceSide::Front).region().id() == f.shell(FaceSide::Back).region().id()
            }) {
                return Err(manifold);
            }
            Ok(Around::Two([a, b]))
        }
        _ => Err(manifold),
    }
}

/// `n` turned to point into region `a` on `face`.
fn toward(face: Face<'_>, a: RegionId, n: Vec3) -> Vec3 {
    if face.shell(FaceSide::Front).region().id() == a {
        n
    } else {
        -n
    }
}

/// The side of the edge's pseudonormal at its foot `q` that `w = p - q` points to.
fn edge_side(
    body: &Brep,
    edge: Edge<'_>,
    q: Point3,
    w: Vec3,
    budget: &mut Budget,
    stop: &Stop,
) -> Result<RegionId, BodyqError> {
    let at = Entity::Edge(edge.id());
    let mut faces: Vec<Face<'_>> = Vec::new();
    for fin in edge.fins() {
        faces.push(face_of(body, fin.face)?);
    }
    let [a, b] = match two_regions(&faces, at)? {
        Around::One(r) => return Ok(r),
        Around::Two(ab) => ab,
    };
    let mut sum = Vec3::new(0.0, 0.0, 0.0);
    for face in &faces {
        let foot = face_foot(*face, q, budget, stop)?;
        let n = face
            .surface()
            .normal(foot.uv, ABOVE)
            .ok_or(BodyqError::Defect(BodyqDefect::NoNormal { at }))?;
        sum += toward(*face, a, n.as_vec());
    }
    side_of(sum.dot(w), a, b, at)
}

/// The region on `s`'s side: `a` when positive, `b` when negative.
fn side_of(s: f64, a: RegionId, b: RegionId, at: Entity) -> Result<RegionId, BodyqError> {
    if s > 0.0 {
        Ok(a)
    } else if s < 0.0 {
        Ok(b)
    } else {
        Err(BodyqError::Defect(BodyqDefect::NoNormal { at }))
    }
}

/// The side of the vertex's angle-weighted pseudonormal that `w = p - v` points to.
fn vertex_side(
    body: &Brep,
    v: Vertex<'_>,
    w: Vec3,
    budget: &mut Budget,
    stop: &Stop,
) -> Result<RegionId, BodyqError> {
    let at = Entity::Vertex(v.id());
    let mut ids: Vec<FaceId> = v
        .edges()
        .flat_map(|e| e.fins().map(|f| f.face).collect::<Vec<_>>())
        .collect();
    ids.sort_unstable();
    ids.dedup();
    let faces = ids
        .iter()
        .map(|&id| face_of(body, id))
        .collect::<Result<Vec<_>, _>>()?;
    let [a, b] = match two_regions(&faces, at)? {
        Around::One(r) => return Ok(r),
        Around::Two(ab) => ab,
    };
    let mut sum = Vec3::new(0.0, 0.0, 0.0);
    for face in &faces {
        let n = vertex_normal(*face, v, budget, stop)?;
        for u in face.uses() {
            if is_collapsed(u) || u.end().id() != v.id() {
                continue;
            }
            let Some(next) = next_solid(u) else { continue };
            let t_in =
                use_tangent(u, End::End).ok_or(BodyqError::Defect(BodyqDefect::NoNormal { at }))?;
            let t_out = use_tangent(next, End::Start)
                .ok_or(BodyqError::Defect(BodyqDefect::NoNormal { at }))?;
            // The face lies to the left of its loops, seen from the front: its corner runs
            // anticlockwise about the normal from the way out to the way back in.
            let back = -t_in;
            let mut angle = fmath::atan2(t_out.cross(back).dot(n), t_out.dot(back));
            if angle <= 0.0 {
                angle += TAU;
            }
            sum += toward(*face, a, n) * angle;
        }
    }
    side_of(sum.dot(w), a, b, at)
}

/// The face's normal at the vertex. At a cone's apex, where there is none, the mean of the
/// normals over four rulings a quarter of the face's `u` span apart: the cone's normal is
/// constant along a ruling, and over a full turn those four sum along the axis as the
/// normals round the apex do.
fn vertex_normal(
    face: Face<'_>,
    v: Vertex<'_>,
    budget: &mut Budget,
    stop: &Stop,
) -> Result<Vec3, BodyqError> {
    let foot = face_foot(face, v.point(), budget, stop)?;
    if let Some(n) = face.surface().normal(foot.uv, ABOVE) {
        return Ok(n.as_vec());
    }
    let uv = face
        .uv_box()
        .ok_or_else(|| BodyqError::Defect(BodyqDefect::NoUvBox { face: face.id() }))?;
    let (u0, u1) = (uv.min().x, uv.max().x);
    let vm = 0.5 * (uv.min().y + uv.max().y);
    let mut sum = Vec3::new(0.0, 0.0, 0.0);
    for k in 0..4 {
        let u = u0 + (u1 - u0) * (f64::from(k) + 0.5) / 4.0;
        let n = face
            .surface()
            .normal(scx_num::vector2d::Point2::new(u, vm), ABOVE)
            .ok_or_else(|| {
                BodyqError::Defect(BodyqDefect::NoNormal {
                    at: Entity::Vertex(v.id()),
                })
            })?;
        sum += n.as_vec();
    }
    Ok(sum * 0.25)
}

fn is_collapsed(u: Use<'_>) -> bool {
    matches!(u.edge().shape(), EdgeShape::Collapsed)
}

/// The next use of the loop after `u` that is not collapsed.
fn next_solid(u: Use<'_>) -> Option<Use<'_>> {
    let mut w = u.next();
    for _ in 0..u.lp().len() {
        if !is_collapsed(w) {
            return Some(w);
        }
        w = w.next();
    }
    None
}

#[derive(Clone, Copy)]
enum End {
    Start,
    End,
}

/// The direction the use runs in at its start or end.
fn use_tangent(u: Use<'_>, end: End) -> Option<Vec3> {
    let EdgeShape::Curve { curve, interval } = u.edge().shape() else {
        return None;
    };
    let forward = u.sense() == Sense::Forward;
    // The edge's parameter at that end of the use, and the side of it the use lies on.
    let (t, side) = match (end, forward) {
        (End::Start, true) | (End::End, false) => (interval.min(), Approach::Above),
        (End::End, true) | (End::Start, false) => (interval.max(), Approach::Below),
    };
    let d = curve.tangent_dir(t, side)?.as_vec();
    Some(if forward { d } else { -d })
}
