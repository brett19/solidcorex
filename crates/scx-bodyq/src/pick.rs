// Written for solidcorex. Apache-2.0.
//! [`pick`]: the faces, edges or vertices of a body within an aperture of a ray (research 74
//! §51), each place with its depth along the ray.
//!
//! The pin's ray picks took the nearest hit (`face_hit_by_ray`), a list of face hits without
//! an aperture (`face_hits_by_ray`), or the nearest edge to the ray (`edge_closest_to_ray`),
//! each with its own tolerance argument. Here every place within the aperture of the ray is
//! returned, sorted by (depth, id), and the caller chooses; it is [`pick_at`](crate::pick_at)
//! for a ray, with the same aperture and the same ordering rule.
//!
//! **What is a hit.** The ray is the half-line from its origin, as far as the body's box
//! reaches. A face has a hit at each place the ray crosses or touches the face as trimmed
//! (distance zero), at the ray's depth there. A face the ray does not cross but passes within
//! the aperture of has one hit, at the depth of the ray's nearest approach to it. An edge has
//! one hit for each stretch of its curve within the aperture of the ray, at the nearest
//! approach. A vertex has one hit at its nearest approach. The aperture is widened by the
//! entity's coincidence, as `pick_at`'s is.
//!
//! **How an approach is found.** The edge's curve meets the thin cylinder of the aperture's
//! radius about the ray (`intersect_curve`, closed form on every analytic curve); the nearest
//! approach inside each entry and exit is a bracketed minimum of the distance from the curve
//! to the ray, which has one dip between them. A face's nearest approach is its boundary
//! edges' (a ray not crossing a trimmed plane or a curved face's trim comes nearest on its
//! boundary), or, for a sphere, cylinder, cone or torus, the dip of the distance to the
//! surface inside the ray's crossing of the surface offset by the aperture (an offset of each
//! of those is a surface of the same kind).
//!
//! **Not covered.** A ray running parallel to a face at a standoff within the aperture, with
//! no crossing, is picked at its boundary edge's nearest approach (where it leaves the face),
//! not where it first comes over it.
//!
//! **NURBS.** The ray's intersection with a NURBS surface or curve is step 5's; a NURBS face
//! or edge within the ray's reach is [`BodyqUnsupported::Nurbs`] naming it, never a sample.

use core::f64::consts::PI;
use std::collections::BTreeSet;

use scx_brep::{Brep, Edge, EdgeId, EdgeShape, Entity, EntityKind, Face, UvSide, Vertex, VertexId};
use scx_curve::{Curve, Line};
use scx_num::budget::{Budget, Stop};
use scx_num::extent1d::Extent1d;
use scx_num::extent2d::Extent2d;
use scx_num::extent3d::Extent3d;
use scx_num::frame::{Frame, Sense};
use scx_num::units::Length;
use scx_num::vector3d::{Point3, Vec3};
use scx_surface::{
    Cone, CurveSurfaceHit, Cylinder, Sphere, Surface, SurfaceError, Torus, intersect_curve,
};

use crate::error::{BodyqDefect, BodyqError, BodyqRefusal, BodyqUnsupported};
use crate::geom::{coincidence, face_foot, recorded, segment_distance, slab};
use crate::reach::{finite_point, unit3};

/// A place on a ray within the aperture of an entity.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RayPicked {
    /// The face, edge or vertex.
    pub entity: Entity,
    /// How far along the ray the place is, from its origin.
    pub depth: f64,
    /// The distance from the ray to the entity there: zero where the ray crosses a face.
    pub distance: f64,
    /// The point of the entity: where the ray crosses a face, else the entity's nearest point
    /// to the ray.
    pub at: Point3,
}

/// The ray: from `p` along the unit `d`, `t` in `[0, len]`.
#[derive(Clone, Copy)]
struct Ray {
    p: Point3,
    d: Vec3,
    len: f64,
}

impl Ray {
    fn at(&self, t: f64) -> Point3 {
        self.p + self.d * t
    }

    /// The distance from `q` to the ray and the depth of its nearest point.
    fn near(&self, q: Point3) -> (f64, f64) {
        segment_distance(self.p, self.d, self.len, q)
    }

    /// The thin cylinder of radius `r` about the ray, with the uv box that spans the ray.
    fn tube(&self, r: f64) -> Result<(Surface, Extent2d), BodyqError> {
        let ray = BodyqError::Defect(BodyqDefect::Ray);
        // The axis the ray runs least along is a hint well off it.
        let hint = if self.d.x.abs() <= self.d.y.abs() && self.d.x.abs() <= self.d.z.abs() {
            Vec3::new(1.0, 0.0, 0.0)
        } else if self.d.y.abs() <= self.d.z.abs() {
            Vec3::new(0.0, 1.0, 0.0)
        } else {
            Vec3::new(0.0, 0.0, 1.0)
        };
        let frame = Frame::new(self.p, self.d, hint).ok_or(ray)?;
        let cyl = Cylinder::new(frame, Length::from_mm(r), Sense::Forward).map_err(|_| ray)?;
        let se = Extent2d::from_bounds(0.0, -r, 2.0 * PI * r, self.len + r).ok_or(ray)?;
        Ok((Surface::Cylinder(cyl), se))
    }
}

/// Every face, edge or vertex of `body` (as `want` says) whose distance from the ray from
/// `origin` along `direction` is within `aperture` of it (plus the entity's coincidence),
/// sorted by (depth, id). See the module for what a hit is.
///
/// Work is charged to `budget` and `stop` is checked before each entity; the same body and
/// ray charge the same work every time. A ray that picks nothing is an empty answer.
///
/// # Errors
///
/// - [`BodyqError::Refused`]: an origin or direction with a coordinate that is not finite
///   ([`NotFinite`](BodyqRefusal::NotFinite)); a direction of no length
///   ([`NoDirection`](BodyqRefusal::NoDirection)); an aperture that is not finite or is
///   negative ([`Tolerance`](BodyqRefusal::Tolerance)); `want` a region or shell
///   ([`NotPickable`](BodyqRefusal::NotPickable)).
/// - [`BodyqError::Unsupported`] ([`Nurbs`](BodyqUnsupported::Nurbs), naming the face or
///   edge) when a NURBS face or edge lies within the ray's reach: step 5 builds the ray's
///   intersection with it.
/// - [`BodyqError::Exhausted`] and [`BodyqError::Cancelled`]; neither leaves anything behind.
/// - [`BodyqError::Defect`] on a body whose faces the method should take and cannot.
pub fn pick(
    body: &Brep,
    origin: Point3,
    direction: Vec3,
    want: EntityKind,
    aperture: Length,
    budget: &mut Budget,
    stop: &Stop,
) -> Result<Vec<RayPicked>, BodyqError> {
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
    finite_point(origin)?;
    let d = unit3(direction)?;
    let Some(bx) = body.bounding_box() else {
        return Ok(Vec::new());
    };
    let len = (bx.max_distance_to_point(origin) + aperture) * 1.0625 + 1.0;
    let ray = Ray { p: origin, d, len };
    let reach = aperture + coincidence(body, None);
    let far = ray.at(len);
    let Some(window) = Extent3d::spanning(origin, far).and_then(|b| b.expanded(reach)) else {
        return Ok(Vec::new());
    };
    let near_ray = |b: &Extent3d| {
        b.expanded(reach)
            .is_none_or(|b| slab(&b, origin, d, len).is_some())
    };
    let mut hits: Vec<RayPicked> = Vec::new();
    let mut edges: BTreeSet<EdgeId> = BTreeSet::new();
    let mut vertices: BTreeSet<VertexId> = BTreeSet::new();
    for face in body.faces_meeting(window) {
        if face.bounding_box().is_some_and(|b| !near_ray(&b)) {
            continue;
        }
        stop.check()?;
        if want == EntityKind::Face {
            face_hits(body, face, &ray, aperture, &mut hits, budget, stop)?;
        } else {
            edges.extend(face.uses().map(|u| u.edge().id()));
        }
    }
    if want != EntityKind::Face {
        for shell in body.shells() {
            edges.extend(shell.wires().map(Edge::id));
            vertices.extend(shell.lone_vertices().map(Vertex::id));
        }
        for id in edges {
            let Some(edge) = body.edge(id) else { continue };
            if edge.bounding_box().is_some_and(|b| !near_ray(&b)) {
                continue;
            }
            stop.check()?;
            if want == EntityKind::Vertex {
                vertices.extend(edge.ends());
                continue;
            }
            for (depth, distance, at) in edge_approaches(body, edge, &ray, aperture, budget, stop)?
            {
                hits.push(RayPicked {
                    entity: Entity::Edge(id),
                    depth,
                    distance,
                    at,
                });
            }
        }
        for id in vertices {
            let Some(v) = body.vertex(id) else { continue };
            let (distance, depth) = ray.near(v.point());
            if distance <= aperture + coincidence(body, recorded(v.accuracy())) {
                hits.push(RayPicked {
                    entity: Entity::Vertex(id),
                    depth,
                    distance,
                    at: v.point(),
                });
            }
        }
    }
    hits.sort_by(|a, b| a.depth.total_cmp(&b.depth).then(a.entity.cmp(&b.entity)));
    Ok(hits)
}

/// The hits of one face: each crossing, else the nearest approach.
fn face_hits(
    body: &Brep,
    face: Face<'_>,
    ray: &Ray,
    aperture: f64,
    out: &mut Vec<RayPicked>,
    budget: &mut Budget,
    stop: &Stop,
) -> Result<(), BodyqError> {
    let entity = Entity::Face(face.id());
    let reach = aperture + coincidence(body, face.accuracy());
    let ray_err = BodyqError::Defect(BodyqDefect::Ray);
    let curve = Curve::Line(Line::new(ray.p, ray.d).map_err(|_| ray_err)?);
    let ce = Extent1d::new(0.0, ray.len).ok_or(ray_err)?;
    let se = face
        .uv_box()
        .ok_or_else(|| BodyqError::Defect(BodyqDefect::NoUvBox { face: face.id() }))?;
    let found = intersect_curve(
        &curve,
        &ce,
        face.surface(),
        &se,
        body.resolution(),
        budget,
        stop,
    )
    .map_err(|e| BodyqError::of_surface(e, face.id()))?;
    let before = out.len();
    for h in found {
        match h {
            CurveSurfaceHit::Point { t, uv, point, .. } => {
                if face.contains_uv(uv) != Some(UvSide::Outside) {
                    out.push(RayPicked {
                        entity,
                        depth: t,
                        distance: 0.0,
                        at: point,
                    });
                }
            }
            CurveSurfaceHit::Overlap { t } => out.push(RayPicked {
                entity,
                depth: t.min(),
                distance: 0.0,
                at: ray.at(t.min()),
            }),
        }
    }
    if out.len() > before {
        return Ok(());
    }
    // No crossing: the ray's dips toward the surface itself, else its nearest approach to the
    // boundary.
    let dips = dip_toward_surface(body, face, ray, reach, budget, stop)?;
    if !dips.is_empty() {
        out.extend(dips.into_iter().map(|(depth, distance, at)| RayPicked {
            entity,
            depth,
            distance,
            at,
        }));
        return Ok(());
    }
    let mut best: Option<(f64, f64, Point3)> = None;
    let mut seen: BTreeSet<EdgeId> = BTreeSet::new();
    for u in face.uses() {
        let edge = u.edge();
        if !seen.insert(edge.id()) {
            continue;
        }
        stop.check()?;
        for (depth, distance, at) in edge_approaches(body, edge, ray, aperture, budget, stop)? {
            if best.is_none_or(|(_, d, _)| distance < d) {
                best = Some((depth, distance, at));
            }
        }
    }
    if let Some((depth, distance, at)) = best {
        out.push(RayPicked {
            entity,
            depth,
            distance,
            at,
        });
    }
    Ok(())
}

/// The ray's nearest approaches to a sphere, cylinder, cone or torus face's own surface, where
/// the ray does not cross it, inside the face: each dip of the foot's distance where the ray
/// meets the surface offset by `reach`. None for a plane (a ray not crossing a trimmed plane
/// comes nearest on its boundary) or where a dip's foot is outside the face. A NURBS or
/// procedural face has no offset to meet: it is refused, naming the face (its crossing was
/// found, its dip is step 5's).
fn dip_toward_surface(
    body: &Brep,
    face: Face<'_>,
    ray: &Ray,
    reach: f64,
    budget: &mut Budget,
    stop: &Stop,
) -> Result<Vec<(f64, f64, Point3)>, BodyqError> {
    let off = |e: SurfaceError| BodyqError::of_surface(e, face.id());
    let offset = match face.surface() {
        Surface::Sphere(s) => Sphere::new(
            *s.frame(),
            Length::from_mm(s.radius().mm() + reach),
            s.sense(),
        )
        .map(Surface::Sphere),
        Surface::Cylinder(s) => Cylinder::new(
            *s.frame(),
            Length::from_mm(s.radius().mm() + reach),
            s.sense(),
        )
        .map(Surface::Cylinder),
        Surface::Cone(s) => Cone::new(
            *s.frame(),
            Length::from_mm(s.radius().mm() + reach / scx_num::fmath::cos(s.half_angle())),
            s.half_angle(),
            s.sense(),
        )
        .map(Surface::Cone),
        Surface::Torus(s) => Torus::new(
            *s.frame(),
            s.major(),
            Length::from_mm(s.minor().mm() + reach),
            s.sense(),
        )
        .map(Surface::Torus),
        Surface::Plane(_) => return Ok(Vec::new()),
        _ => {
            return Err(BodyqError::Unsupported(BodyqUnsupported::Nurbs {
                at: Entity::Face(face.id()),
            }));
        }
    };
    // An offset that is no surface (a torus whose tube grows past its ring) is not tried.
    let Ok(offset) = offset else {
        return Ok(Vec::new());
    };
    let ray_err = BodyqError::Defect(BodyqDefect::Ray);
    let curve = Curve::Line(Line::new(ray.p, ray.d).map_err(|_| ray_err)?);
    let ce = Extent1d::new(0.0, ray.len).ok_or(ray_err)?;
    let se = face
        .uv_box()
        .ok_or_else(|| BodyqError::Defect(BodyqDefect::NoUvBox { face: face.id() }))?;
    let found =
        intersect_curve(&curve, &ce, &offset, &se, body.resolution(), budget, stop).map_err(off)?;
    let mut ts: Vec<f64> = found
        .iter()
        .filter_map(|h| match h {
            CurveSurfaceHit::Point { t, .. } => Some(*t),
            CurveSurfaceHit::Overlap { .. } => None,
        })
        .collect();
    ts.sort_by(f64::total_cmp);
    let mut out: Vec<(f64, f64, Point3)> = Vec::new();
    for (a, b, inner) in brackets(&ts, 0.0, ray.len) {
        let (t, _) = minimize(
            a,
            b,
            |t| Ok(face_foot(face, ray.at(t), budget, stop)?.distance),
            inner,
        )?;
        let Some(t) = t else { continue };
        let foot = face_foot(face, ray.at(t), budget, stop)?;
        if foot.side != Some(UvSide::Outside) && foot.distance <= reach {
            out.push((t, foot.distance, foot.point));
        }
    }
    merge_equal(&mut out, |h| h.2);
    Ok(out)
}

/// Each stretch of `edge` within `aperture` (plus its coincidence) of the ray: the depth of
/// the nearest approach, the distance there and the edge's point. A collapsed edge has none
/// (its vertex does).
fn edge_approaches(
    body: &Brep,
    edge: Edge<'_>,
    ray: &Ray,
    aperture: f64,
    budget: &mut Budget,
    stop: &Stop,
) -> Result<Vec<(f64, f64, Point3)>, BodyqError> {
    let EdgeShape::Curve { curve, interval } = edge.shape() else {
        return Ok(Vec::new());
    };
    let reach = aperture + coincidence(body, recorded(edge.accuracy()));
    let (tube, se) = ray.tube(reach)?;
    let nurbs = BodyqError::Unsupported(BodyqUnsupported::Nurbs {
        at: Entity::Edge(edge.id()),
    });
    let found = intersect_curve(curve, interval, &tube, &se, body.resolution(), budget, stop)
        .map_err(|e| match e {
            SurfaceError::Exhausted(x) => BodyqError::of_budget(x),
            SurfaceError::Stopped => BodyqError::Cancelled,
            SurfaceError::NurbsIntersection | SurfaceError::Undecided { .. } => nurbs,
            _ => BodyqError::Defect(BodyqDefect::Ray),
        })?;
    let gap = |s: f64| ray.near(curve.point(s)).0;
    let mut ss: Vec<f64> = Vec::new();
    for h in found {
        match h {
            CurveSurfaceHit::Point { t, .. } => ss.push(t),
            CurveSurfaceHit::Overlap { t } => {
                ss.push(t.min());
                ss.push(t.max());
            }
        }
    }
    // An end inside the tube has no entry or exit there.
    for s in [interval.min(), interval.max()] {
        if gap(s) <= reach {
            ss.push(s);
        }
    }
    ss.sort_by(f64::total_cmp);
    ss.dedup();
    let mut out = Vec::new();
    for (a, b, inner) in brackets(&ss, interval.min(), interval.max()) {
        budget.charge(1)?;
        let (s, distance) = minimize(a, b, |s| Ok(gap(s)), inner)?;
        if let Some(s) = s {
            let at = curve.point(s);
            out.push((ray.near(at).1, distance, at));
        }
    }
    merge_equal(&mut out, |h| h.2);
    Ok(out)
}

/// Where a dip of a distance may lie, for each of the places `ts` where it met a thin tube:
/// the bracket from the middle before it to the middle after (the ends `lo` and `hi` outside
/// the first and last), and the two flags that say whether each end is such a middle. The
/// solver gives an entry and an exit as two places or, where the tube is thinner than its
/// tolerance, as one, so no pairing is assumed: a dip lies in exactly one bracket, and a
/// bracket whose least is at a middle holds no dip (its neighbour's has it).
fn brackets(ts: &[f64], lo: f64, hi: f64) -> Vec<(f64, f64, [bool; 2])> {
    ts.iter()
        .enumerate()
        .map(|(i, &t)| {
            let before = i.checked_sub(1).and_then(|j| ts.get(j));
            let after = ts.get(i + 1);
            let a = before.map_or(lo, |b| 0.5 * (b + t));
            let b = after.map_or(hi, |n| 0.5 * (n + t));
            (a, b, [before.is_some(), after.is_some()])
        })
        .collect()
}

/// The least of `f` over `[a, b]`: its place and value, golden-section search for a fixed
/// number of steps (so the work is the same every time), the ends tried too. The place is
/// `None` when the least is at an end that `inner` says is shared with a neighbouring
/// bracket and is not a dip there. A dip exactly on the shared end is found by both brackets,
/// so the caller merges equal places.
fn minimize<F>(a: f64, b: f64, mut f: F, inner: [bool; 2]) -> Result<(Option<f64>, f64), BodyqError>
where
    F: FnMut(f64) -> Result<f64, BodyqError>,
{
    const INV_PHI: f64 = 0.618_033_988_749_894_9;
    let (mut lo, mut hi) = (a, b);
    let mut c = hi - INV_PHI * (hi - lo);
    let mut d = lo + INV_PHI * (hi - lo);
    let (mut fc, mut fd) = (f(c)?, f(d)?);
    for _ in 0..60 {
        if fc <= fd {
            hi = d;
            d = c;
            fd = fc;
            c = hi - INV_PHI * (hi - lo);
            fc = f(c)?;
        } else {
            lo = c;
            c = d;
            fc = fd;
            d = lo + INV_PHI * (hi - lo);
            fd = f(d)?;
        }
    }
    let mut best = if fc <= fd { (c, fc) } else { (d, fd) };
    for s in [a, b] {
        let v = f(s)?;
        if v < best.1 {
            best = (s, v);
        }
    }
    // At an end shared with a neighbour the least may be that neighbour's dip seen from the
    // side: it is a dip of this bracket only if the distance rises on both sides of it.
    let slack = 1e-6 * (b - a);
    let step = 1e-4 * (b - a);
    let mut at_shared = false;
    for (shared, end) in [(inner[0], a), (inner[1], b)] {
        if shared && (best.0 - end).abs() <= slack {
            let rises = f(best.0 - step)? >= best.1 && f(best.0 + step)? >= best.1;
            at_shared = !rises;
        }
    }
    Ok(((!at_shared).then_some(best.0), best.1))
}

/// How close two places are to be one: a dip of a distance is flat at its least, so two
/// searches of it agree only to about the square root of the rounding.
const MERGE: f64 = 1e-6;

/// Drops each place that is the same point as the one before it (two brackets found one dip
/// on their shared end).
fn merge_equal<T>(v: &mut Vec<T>, point: impl Fn(&T) -> Point3) {
    let mut kept: Vec<T> = Vec::with_capacity(v.len());
    for h in v.drain(..) {
        if kept.iter().all(|k| point(k).distance(point(&h)) > MERGE) {
            kept.push(h);
        }
    }
    *v = kept;
}
