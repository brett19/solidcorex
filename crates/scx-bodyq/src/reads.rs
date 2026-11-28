// Written for solidcorex. Apache-2.0.
//! The query and measure map's missing functions (research 74 §1.3, BW9): what a body can
//! be asked about an edge, a face or a point, as closed forms on `&Brep`.
//!
//! Every function takes the body by `&Brep`, charges its work to the caller's `Budget` and
//! polls the `Stop`, and answers with a typed error; none takes an `accept` or `tol`
//! parameter. The one length is coincidence: `scx_num::units::coincident` of the body's
//! resolution and the entity's accuracy, as `classify_point` reads it. The map from the
//! pin's functions to these and to the rest of the crate is the table in research 74's
//! BW9 section.
//!
//! | pin | here |
//! |---|---|
//! | `Query::edge_at(e, t)`: `None` for a traced curve | [`edge_at`]: every curve has a jet; a collapsed edge is refused |
//! | `Query::edge_distance(e, p, sag)`: a sampled polyline | [`edge_distance`]: the closed-form foot and both ends |
//! | `Query::face_uv(f, p, accept)`: `None` beyond `accept` | [`face_uv`]: the foot and its gap; the caller compares |
//! | `Query::face_normal(f, uv)`: `forward` flag | [`outward_normal`]: out of the material, by the regions |
//! | `Query::face_contains`, `faces_at` | [`face_contains`], [`faces_at`] |
//! | `Query::faces_against` | [`faces_against`] |
//! | `closest_to_point` (`closest_to_members`) | [`closest_to_point`], with the proof stated |
//! | `face_line_hits`, `face_line_nearest` | [`face_line_hits`], [`face_line_nearest`] |
//! | `rim_edges` | [`rim_edges`] |
//! | `turn_to_parallel`: a 1e-6 square test and 1e-9 along-axis test | [`turn_to_parallel`] against a [`Near`] |

use std::collections::{BTreeMap, BTreeSet};

use scx_brep::{Brep, EdgeId, EdgeShape, Entity, Face, FaceId, FaceSide, UvSide};
use scx_curve::{Curve, Line};
use scx_num::budget::{Budget, Stop};
use scx_num::extent1d::Extent1d;
use scx_num::extent3d::Extent3d;
use scx_num::units::Length;
use scx_num::vector2d::Point2;
use scx_num::vector3d::{Point3, Vec3};
use scx_nurbs::Approach;
use scx_surface::{Contact, CurveSurfaceHit, Surface, intersect_curve};

use crate::error::{BodyqDefect, BodyqError, BodyqRefusal};
use crate::geom::{ABOVE, coincidence, edge_foot, face_foot, recorded};
use crate::reach::{Near, finite, finite_point, unit3};

fn face_of(body: &Brep, id: FaceId) -> Result<Face<'_>, BodyqError> {
    body.face(id)
        .ok_or(BodyqError::Refused(BodyqRefusal::NoEntity {
            entity: Entity::Face(id),
        }))
}

fn no_edge(id: EdgeId) -> BodyqError {
    BodyqError::Refused(BodyqRefusal::NoEntity {
        entity: Entity::Edge(id),
    })
}

/// A point of an edge's curve and the curve's derivative there.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct EdgePoint {
    /// The point.
    pub point: Point3,
    /// The first derivative with respect to the curve's parameter (not unit; it points
    /// the way the parameter grows, which is from the edge's start to its end).
    pub tangent: Vec3,
}

/// The point and derivative of edge `edge`'s curve at parameter `t`. `t` is the curve's
/// own parameter, as in the edge's interval; a conic's is an angle in radians. A NURBS
/// curve's knot is read from above.
///
/// # Errors
///
/// Refused for an edge the body does not have, a collapsed edge (it has no curve), or a `t`
/// that is not finite.
pub fn edge_at(body: &Brep, edge: EdgeId, t: f64) -> Result<EdgePoint, BodyqError> {
    finite(t)?;
    let e = body.edge(edge).ok_or_else(|| no_edge(edge))?;
    let EdgeShape::Curve { curve, .. } = e.shape() else {
        return Err(BodyqError::Refused(BodyqRefusal::Collapsed { edge }));
    };
    let jet = curve.jet::<1>(t, Approach::Above);
    Ok(EdgePoint {
        point: jet.point,
        tangent: jet.d[0],
    })
}

/// The point of an edge's curve nearest a point.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct EdgeClosest {
    /// The nearest point.
    pub point: Point3,
    /// Its parameter on the curve (an end vertex's is the interval's end it is at).
    pub parameter: f64,
    /// Its distance from the point.
    pub distance: Length,
}

fn closest_on(
    body: &Brep,
    edge: EdgeId,
    p: Point3,
    budget: &mut Budget,
) -> Result<EdgeClosest, BodyqError> {
    let e = body.edge(edge).ok_or_else(|| no_edge(edge))?;
    let EdgeShape::Curve { interval, .. } = e.shape() else {
        return Err(BodyqError::Refused(BodyqRefusal::Collapsed { edge }));
    };
    let mut best: Option<EdgeClosest> = None;
    let mut keep = |point: Point3, parameter: f64, d: f64| {
        if best.is_none_or(|b| d < b.distance.mm()) {
            best = Some(EdgeClosest {
                point,
                parameter,
                distance: Length::from_mm(d),
            });
        }
    };
    // The start vertex is at the interval's start, the end vertex at its end.
    keep(
        e.start().point(),
        interval.min(),
        e.start().point().distance(p),
    );
    keep(e.end().point(), interval.max(), e.end().point().distance(p));
    if let Some((t, q, d)) = edge_foot(e, p, budget)? {
        keep(q, t, d);
    }
    best.ok_or(BodyqError::Defect(BodyqDefect::NoEdgeBox { edge }))
}

/// The point of edge `edge`'s curve over its interval nearest `p` (the foot in closed form on
/// every analytic curve, else an end), with its parameter. One unit of work for the foot.
///
/// # Errors
///
/// Refused for an edge the body does not have, a collapsed edge or a `p` that is not finite;
/// Unsupported for a NURBS curve the inversion does not take; Exhausted.
pub fn edge_closest_point(
    body: &Brep,
    edge: EdgeId,
    p: Point3,
    budget: &mut Budget,
) -> Result<EdgeClosest, BodyqError> {
    finite_point(p)?;
    closest_on(body, edge, p, budget)
}

/// The nearest point of edge `edge` to `p` for the members of a set: a collapsed edge is its
/// vertex.
fn edge_nearest(
    body: &Brep,
    edge: EdgeId,
    p: Point3,
    budget: &mut Budget,
) -> Result<(Point3, f64), BodyqError> {
    let e = body.edge(edge).ok_or_else(|| no_edge(edge))?;
    if matches!(e.shape(), EdgeShape::Collapsed) {
        let q = e.start().point();
        return Ok((q, q.distance(p)));
    }
    let c = closest_on(body, edge, p, budget)?;
    Ok((c.point, c.distance.mm()))
}

/// How far `p` is from edge `edge`: the least distance to its curve over its interval, the
/// foot in closed form on every analytic curve and the two end vertices (a collapsed edge is
/// its vertex). One unit of work for the foot.
///
/// # Errors
///
/// Refused for an edge the body does not have or a `p` that is not finite; Unsupported for a
/// NURBS curve the inversion does not take; Exhausted.
pub fn edge_distance(
    body: &Brep,
    edge: EdgeId,
    p: Point3,
    budget: &mut Budget,
) -> Result<Length, BodyqError> {
    finite_point(p)?;
    edge_nearest(body, edge, p, budget).map(|(_, d)| Length::from_mm(d))
}

/// The foot of a point on a face's surface, and where it lies against the face.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FaceUv {
    /// The foot's (u, v), placed in the face's uv box.
    pub uv: Point2,
    /// The foot.
    pub point: Point3,
    /// The distance from the point to the foot.
    pub gap: Length,
    /// Where the foot lies against the face's region: `None` when the face's boundary
    /// cannot say ([`Face::contains_uv`]).
    pub side: Option<UvSide>,
}

/// The nearest point of face `face`'s surface to `p`, its uv and where that lies against the
/// face. One unit of work.
///
/// # Errors
///
/// Refused for a face the body does not have or a `p` that is not finite; Unsupported for a
/// NURBS surface the inversion does not take; Exhausted or Cancelled.
pub fn face_uv(
    body: &Brep,
    face: FaceId,
    p: Point3,
    budget: &mut Budget,
    stop: &Stop,
) -> Result<FaceUv, BodyqError> {
    finite_point(p)?;
    let f = face_of(body, face)?;
    let foot = face_foot(f, p, budget, stop)?;
    Ok(FaceUv {
        uv: foot.uv,
        point: foot.point,
        gap: Length::from_mm(foot.distance),
        side: foot.side,
    })
}

/// The unit normal of face `face` at `uv`, out of its material: the natural normal when the
/// front side is air, its reverse when the front is solid. `None` where the surface has no
/// normal (a pole or an apex).
///
/// # Errors
///
/// Refused for a face the body does not have, a `uv` that is not finite, or a face with
/// material on both sides or neither ([`NoMaterialSide`](BodyqRefusal::NoMaterialSide)).
pub fn outward_normal(body: &Brep, face: FaceId, uv: Point2) -> Result<Option<Vec3>, BodyqError> {
    finite(uv.x)?;
    finite(uv.y)?;
    let f = face_of(body, face)?;
    let (front, back) = (
        f.shell(FaceSide::Front).region().is_solid(),
        f.shell(FaceSide::Back).region().is_solid(),
    );
    if front == back {
        return Err(BodyqError::Refused(BodyqRefusal::NoMaterialSide { face }));
    }
    let sign = if front { -1.0 } else { 1.0 };
    Ok(f.surface().normal(uv, ABOVE).map(|n| n.as_vec() * sign))
}

/// Whether `p` is on face `face`: within coincidence of its surface and inside its loops, or
/// within coincidence of one of its edges (a face holds its own edges). A foot the face's
/// boundary cannot place (`side` is `None`) counts only by its edges.
///
/// # Errors
///
/// As [`face_uv`].
pub fn face_contains(
    body: &Brep,
    face: FaceId,
    p: Point3,
    budget: &mut Budget,
    stop: &Stop,
) -> Result<bool, BodyqError> {
    let f = face_uv(body, face, p, budget, stop)?;
    on_face(body, face, p, &f, budget)
}

fn on_face(
    body: &Brep,
    face: FaceId,
    p: Point3,
    foot: &FaceUv,
    budget: &mut Budget,
) -> Result<bool, BodyqError> {
    let f = face_of(body, face)?;
    if foot.gap.mm() > coincidence(body, f.accuracy()) {
        return Ok(false);
    }
    if foot.side == Some(UvSide::Inside) {
        return Ok(true);
    }
    let mut edges: Vec<EdgeId> = f.uses().map(|u| u.edge().id()).collect();
    edges.sort_unstable();
    edges.dedup();
    for id in edges {
        let Some(e) = body.edge(id) else { continue };
        let (_, d) = edge_nearest(body, id, p, budget)?;
        if d <= coincidence(body, recorded(e.accuracy())) {
            return Ok(true);
        }
    }
    Ok(false)
}

/// The faces `p` lies on ([`face_contains`]), nearest first (ties by id), each with how far
/// `p` is from its surface. Only faces whose box holds `p` are read.
///
/// # Errors
///
/// As [`face_uv`] for the faces read.
pub fn faces_at(
    body: &Brep,
    p: Point3,
    budget: &mut Budget,
    stop: &Stop,
) -> Result<Vec<(Length, FaceId)>, BodyqError> {
    finite_point(p)?;
    let Some(at) = Extent3d::from_point(p) else {
        return Ok(Vec::new());
    };
    let mut out: Vec<(Length, FaceId)> = Vec::new();
    for f in body.faces_meeting(at) {
        stop.check()?;
        if f.bounding_box().is_some_and(|b| !b.contains_point(p, 0.0)) {
            continue;
        }
        let foot = face_uv(body, f.id(), p, budget, stop)?;
        if on_face(body, f.id(), p, &foot, budget)? {
            out.push((foot.gap, f.id()));
        }
    }
    out.sort_by(|a, b| a.0.mm().total_cmp(&b.0.mm()).then(a.1.cmp(&b.1)));
    Ok(out)
}

/// The faces that face against the curves `samples` lie on, in id order. A sample is a point
/// on a curve and its direction there; a face through it is meant to face along that
/// direction crossed with `normal` (as an extruded sheet's walls face, out of a region whose
/// outer loop runs counter-clockwise about `normal`). Each face a sample lies on
/// ([`faces_at`]) is judged there by its normal out of the material
/// ([`outward_normal`]); a face's judgements are summed, and a face whose sum is below zero
/// faces against them. A sample whose direction runs along `normal`, a face with no normal or
/// no material side there, and a face no sample lies on say nothing.
///
/// # Errors
///
/// Refused for a `normal` or sample that is not finite or a `normal` of no length; those of
/// [`faces_at`] and [`face_uv`].
pub fn faces_against(
    body: &Brep,
    normal: Vec3,
    samples: &[(Point3, Vec3)],
    budget: &mut Budget,
    stop: &Stop,
) -> Result<Vec<FaceId>, BodyqError> {
    let normal = unit3(normal)?;
    let mut votes: BTreeMap<FaceId, f64> = BTreeMap::new();
    for &(p, d) in samples {
        finite_point(p)?;
        if !d.is_finite() {
            return Err(BodyqError::Refused(BodyqRefusal::NotFinite));
        }
        let Some((_, want)) = d.cross(normal).length_and_unit() else {
            continue;
        };
        let want = want.as_vec();
        for (_, f) in faces_at(body, p, budget, stop)? {
            let foot = face_uv(body, f, p, budget, stop)?;
            match outward_normal(body, f, foot.uv) {
                Ok(Some(n)) => *votes.entry(f).or_insert(0.0) += n.dot(want),
                Ok(None) | Err(BodyqError::Refused(BodyqRefusal::NoMaterialSide { .. })) => {}
                Err(e) => return Err(e),
            }
        }
    }
    Ok(votes
        .into_iter()
        .filter(|&(_, v)| v < 0.0)
        .map(|(f, _)| f)
        .collect())
}

/// The nearest point of a set of faces and edges to a point.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Nearest {
    /// The member (a face or an edge of those asked for) it lies on.
    pub member: Entity,
    /// The point.
    pub point: Point3,
    /// Its distance from the point.
    pub distance: Length,
    /// Whether `distance` is the least over the whole member: always for an edge and for a
    /// face whose foot is inside it or that is a plane; a face of another kind whose foot
    /// lies outside it gives its boundary's nearest point, an upper bound (the member may
    /// have a nearer interior point at another local minimum of the distance).
    pub exact: bool,
}

/// The member of `faces` and `edges` nearest `p`, with the point on it, or `None` for no
/// members. A face's nearest point is its surface's foot when that lies inside the face,
/// else the nearest point of its boundary edges. Ties go to the lower entity (faces after
/// edges; lower id first).
///
/// # Errors
///
/// Refused for a face or edge the body does not have or a `p` that is not finite;
/// Unsupported for a NURBS member the inversion does not take; Exhausted or Cancelled.
pub fn closest_to_point(
    body: &Brep,
    faces: &[FaceId],
    edges: &[EdgeId],
    p: Point3,
    budget: &mut Budget,
    stop: &Stop,
) -> Result<Option<Nearest>, BodyqError> {
    finite_point(p)?;
    let (faces, edges): (BTreeSet<_>, BTreeSet<_>) = (
        faces.iter().copied().collect(),
        edges.iter().copied().collect(),
    );
    let mut best: Option<Nearest> = None;
    let mut keep = |c: Nearest| {
        if best.is_none_or(|b| c.distance.mm() < b.distance.mm()) {
            best = Some(c);
        }
    };
    for id in edges {
        stop.check()?;
        let (point, d) = edge_nearest(body, id, p, budget)?;
        keep(Nearest {
            member: Entity::Edge(id),
            point,
            distance: Length::from_mm(d),
            exact: true,
        });
    }
    for id in faces {
        stop.check()?;
        let f = face_of(body, id)?;
        let foot = face_uv(body, id, p, budget, stop)?;
        if foot.side == Some(UvSide::Inside) {
            keep(Nearest {
                member: Entity::Face(id),
                point: foot.point,
                distance: foot.gap,
                exact: true,
            });
            continue;
        }
        // The foot is off the face, or the face cannot say: the nearest boundary point.
        let mut loop_edges: Vec<EdgeId> = f.uses().map(|u| u.edge().id()).collect();
        loop_edges.sort_unstable();
        loop_edges.dedup();
        let mut near: Option<(Point3, f64)> = None;
        for e in loop_edges {
            let (q, d) = edge_nearest(body, e, p, budget)?;
            if near.is_none_or(|(_, b)| d < b) {
                near = Some((q, d));
            }
        }
        if let Some((point, d)) = near {
            keep(Nearest {
                member: Entity::Face(id),
                point,
                distance: Length::from_mm(d),
                exact: foot.side.is_some() && matches!(f.surface(), Surface::Plane(_)),
            });
        }
    }
    Ok(best)
}

/// A place a line meets a face.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LineHit {
    /// The distance from the line's `through` point along its direction (negative behind).
    pub t: f64,
    /// The point.
    pub point: Point3,
    /// The face's (u, v) there.
    pub uv: Point2,
    /// Whether the line crosses the surface or only touches it.
    pub contact: Contact,
}

/// Every place the whole line through `through` along `direction` (both ways, as far as the
/// body's box reaches) meets face `face` within its trim, ordered along the line, each place
/// once (a place on a periodic surface's seam too). A stretch of the line lying on the face is
/// no single place and is left out. A place within coincidence of the face's boundary
/// ([`UvSide::Near`]) is kept.
///
/// # Errors
///
/// Refused for a face the body does not have, a point that is not finite or a direction of no
/// length; Unsupported for a NURBS face; Exhausted or Cancelled.
pub fn face_line_hits(
    body: &Brep,
    face: FaceId,
    through: Point3,
    direction: Vec3,
    budget: &mut Budget,
    stop: &Stop,
) -> Result<Vec<LineHit>, BodyqError> {
    finite_point(through)?;
    let d = unit3(direction)?;
    let f = face_of(body, face)?;
    let Some(bx) = body.bounding_box() else {
        return Ok(Vec::new());
    };
    let reach = bx.max_distance_to_point(through) * 1.0625 + 1.0;
    let ray = BodyqError::Defect(BodyqDefect::Ray);
    let curve = Curve::Line(Line::new(through, d).map_err(|_| ray)?);
    let ce = Extent1d::new(-reach, reach).ok_or(ray)?;
    let se = f
        .uv_box()
        .ok_or(BodyqError::Defect(BodyqDefect::NoUvBox { face }))?;
    let found = intersect_curve(
        &curve,
        &ce,
        f.surface(),
        &se,
        body.resolution(),
        budget,
        stop,
    )
    .map_err(|e| BodyqError::of_surface(e, face))?;
    let mut out = Vec::new();
    for h in found {
        if let CurveSurfaceHit::Point {
            t,
            uv,
            point,
            contact,
        } = h
            && f.contains_uv(uv) != Some(UvSide::Outside)
        {
            out.push(LineHit {
                t,
                point,
                uv,
                contact,
            });
        }
    }
    out.sort_by(|a, b| a.t.total_cmp(&b.t));
    Ok(out)
}

/// Of the places the line meets face `face` ([`face_line_hits`]), the nearest to `through` on
/// either side. Meetings as near as each other within `tie` are a tie, which goes to the one
/// furthest along `direction` (ahead rather than behind: a line through a cylinder's axis
/// meets its wall ahead). `None` where the line misses the face or only runs along it.
///
/// # Errors
///
/// As [`face_line_hits`]; refused for a `tie` that is negative or not finite.
pub fn face_line_nearest(
    body: &Brep,
    face: FaceId,
    through: Point3,
    direction: Vec3,
    tie: Length,
    budget: &mut Budget,
    stop: &Stop,
) -> Result<Option<Point3>, BodyqError> {
    let tie = finite(tie.mm())?;
    if tie < 0.0 {
        return Err(BodyqError::Refused(BodyqRefusal::Tolerance));
    }
    let hits = face_line_hits(body, face, through, direction, budget, stop)?;
    let Some(least) = hits.iter().map(|h| h.t.abs()).min_by(f64::total_cmp) else {
        return Ok(None);
    };
    Ok(hits
        .iter()
        .filter(|h| h.t.abs() <= least + tie)
        .max_by(|a, b| a.t.total_cmp(&b.t))
        .map(|h| h.point))
}

/// The rim of the faces `faces`: the edges with a face of the set on one side and a face
/// outside it on another, in id order. A seam (the same face on both sides) and an edge
/// between two faces of the set are not on it. Empty where the faces close up on their own.
///
/// # Errors
///
/// Refused for a face the body does not have.
pub fn rim_edges(body: &Brep, faces: &[FaceId]) -> Result<Vec<EdgeId>, BodyqError> {
    let mut set = BTreeSet::new();
    for &f in faces {
        face_of(body, f)?;
        set.insert(f);
    }
    let mut out = Vec::new();
    for e in body.edges() {
        let (mut within, mut outside) = (false, false);
        for u in e.uses() {
            if set.contains(&u.lp().face().id()) {
                within = true;
            } else {
                outside = true;
            }
        }
        if within && outside {
            out.push(e.id());
        }
    }
    Ok(out)
}

/// Why [`turn_to_parallel`] cannot answer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum ParallelTurnError {
    /// The normal is not square to the axis over the reach: it is not the normal of a side
    /// of the axis.
    NotASide,
    /// The line runs along the axis over the reach: turning about the axis never makes a
    /// direction square to it parallel to the line.
    AlongAxis,
    /// A vector that is not finite or of no length.
    Refused(BodyqRefusal),
}

/// The turn about `axis`, in radians in `(-pi/2, pi/2]`, that makes a direction square to
/// `axis` (the side whose normal is `face_normal`) parallel to `line`, a line having no
/// sense. The three vectors need not be unit.
///
/// The normal is square to the axis when `|n . a|` times the reach is within coincidence
/// (the normal tilts off the square by no more than coincidence over the reach), and the line
/// is along the axis when `|a x l|` times the reach is.
///
/// # Errors
///
/// [`ParallelTurnError`], see its variants.
pub fn turn_to_parallel(
    axis: Vec3,
    face_normal: Vec3,
    line: Vec3,
    near: &Near,
) -> Result<f64, ParallelTurnError> {
    let u = |v: Vec3| unit3(v).map_err(|_| refusal(v));
    let (a, n, l) = (u(axis)?, u(face_normal)?, u(line)?);
    let (c, r) = (near.coincidence().mm(), near.reach().mm());
    if n.dot(a).abs() * r > c {
        return Err(ParallelTurnError::NotASide);
    }
    if a.cross(l).length() * r <= c {
        return Err(ParallelTurnError::AlongAxis);
    }
    a.square_turn(n, l)
        .map_err(|_| ParallelTurnError::Refused(BodyqRefusal::NoDirection))
}

fn refusal(v: Vec3) -> ParallelTurnError {
    ParallelTurnError::Refused(if v.is_finite() {
        BodyqRefusal::NoDirection
    } else {
        BodyqRefusal::NotFinite
    })
}
