// Written for solidcorex. Apache-2.0.
//! Stage 2: the interferences, every place an entity of one operand meets one of the other
//! within coincidence.
//!
//! The candidates come from the face pairs: for a pair `(fa, fb)`, each vertex of one face
//! against each vertex, edge and the face of the other, and each edge of one face against
//! the other face. They are deduplicated and sorted before any is tested, so a vertex or
//! edge shared by many pairs is tested once, and the order (and work) does not depend on
//! how the pairs named it.
//!
//! An edge against a face is a curve-surface intersection. A point hit within coincidence
//! of a boundary vertex of the face is left to the vertex-edge test; one within
//! coincidence of boundary edges is an edge-edge contact with each such edge; otherwise one
//! `Inside` the face is an edge-face contact. A hit `Near` the boundary with no boundary
//! entity within coincidence is a defect: `Near` is a few rounding errors wide. A stretch
//! where the edge lies on the surface is a [`Span`], left to BO2 and BO3.

use std::collections::BTreeMap;

use scx_brep::{EdgeId, EdgeShape, Entity, Face, FaceId, UvSide, VertexId};
use scx_curve::Curve;
use scx_num::budget::{Budget, Stop};
use scx_num::extent1d::Extent1d;
use scx_num::extent3d::Extent3d;
use scx_num::units::{Accuracy, Length, Resolution};
use scx_num::vector2d::Point2;
use scx_num::vector3d::Point3;
use scx_surface::{Contact as HitContact, CurveSurfaceHit, SurfaceError, intersect_curve};

use crate::error::{BooleanDefect, BooleanError, BooleanUnsupported};
use crate::geom::{Operands, Side, edge_foot, face_foot, recorded};

/// Where on its operand a member of a contact lies.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum On {
    /// At a vertex.
    Vertex(VertexId),
    /// On an edge, inside it, at curve parameter `t`.
    Edge {
        /// The edge.
        edge: EdgeId,
        /// The parameter.
        t: f64,
    },
    /// On a face, inside it, at `uv`.
    Face {
        /// The face.
        face: FaceId,
        /// The surface parameters.
        uv: Point2,
    },
}

impl On {
    /// The order key: lowest dimension first, then id.
    pub(crate) const fn key(&self) -> (u8, u32) {
        match *self {
            Self::Vertex(v) => (0, v.raw()),
            Self::Edge { edge, .. } => (1, edge.raw()),
            Self::Face { face, .. } => (2, face.raw()),
        }
    }
}

/// One side of a contact: the entity and its own point there.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Member {
    /// The entity.
    pub(crate) on: On,
    /// Its point: the vertex's, or the foot or hit on the edge or face.
    pub(crate) point: Point3,
}

/// An entity of `a` and one of `b` within coincidence of each other.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Contact {
    /// `a`'s member, then `b`'s.
    pub(crate) members: [Member; 2],
    /// The coincidence distance of the two entities, in mm.
    pub(crate) reach: f64,
    /// Whether an edge touches a surface here without crossing it.
    pub(crate) tangent: bool,
}

impl Contact {
    /// The contact with `mine` on `side` and `theirs` on the other.
    const fn of(side: Side, mine: Member, theirs: Member, reach: f64, tangent: bool) -> Self {
        let members = match side {
            Side::A => [mine, theirs],
            Side::B => [theirs, mine],
        };
        Self {
            members,
            reach,
            tangent,
        }
    }

    /// The midpoint of the two members' points.
    pub(crate) fn mid(&self) -> Point3 {
        let [p, q] = self.members;
        p.point + (q.point - p.point) * 0.5
    }
}

/// An edge of `side` lying on a face of the other side over a parameter range: the curve
/// stays within coincidence of the face's surface there. The range is of the surface's
/// uv box, not trimmed to the face.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Span {
    /// The edge's side.
    pub(crate) side: Side,
    /// The edge.
    pub(crate) edge: EdgeId,
    /// The face, of the other side.
    pub(crate) face: FaceId,
    /// The edge's parameters.
    pub(crate) t: Extent1d,
}

/// What stage 2 found: contacts and spans, each in candidate order.
#[derive(Debug, Default)]
pub(crate) struct Found {
    /// The contacts.
    pub(crate) contacts: Vec<Contact>,
    /// The spans.
    pub(crate) spans: Vec<Span>,
    /// The faces the pairs name, with their parts.
    pub(crate) touched: Touched,
}

/// A test to run. `side` is the side of the first entity named; the second is of the other.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Candidate {
    /// A vertex of `a` and a vertex of `b`.
    Vv(VertexId, VertexId),
    /// A vertex and an edge.
    Ve(Side, VertexId, EdgeId),
    /// A vertex and a face.
    Vf(Side, VertexId, FaceId),
    /// An edge and a face.
    Ef(Side, EdgeId, FaceId),
}

/// What the tests read of a face, read once: its vertices and edges, each once in id
/// order, its accuracy and its box (each derived in time linear in the face's uses).
#[derive(Debug, Default)]
pub(crate) struct Parts {
    pub(crate) vertices: Vec<VertexId>,
    pub(crate) edges: Vec<EdgeId>,
    pub(crate) accuracy: Option<Accuracy>,
    pub(crate) at: Option<Extent3d>,
}

impl Parts {
    pub(crate) fn of(face: Face<'_>) -> Self {
        let mut vertices: Vec<VertexId> = face.uses().map(|u| u.start().id()).collect();
        let mut edges: Vec<EdgeId> = face.uses().map(|u| u.edge().id()).collect();
        vertices.sort_unstable();
        vertices.dedup();
        edges.sort_unstable();
        edges.dedup();
        Self {
            vertices,
            edges,
            accuracy: face.accuracy(),
            at: face.bounding_box(),
        }
    }
}

/// The faces the pairs name on each side, with their parts: built once, read by every
/// test and by the merge.
#[derive(Debug, Default)]
pub(crate) struct Touched([BTreeMap<FaceId, Parts>; 2]);

impl Touched {
    /// The faces of `pairs` and their parts.
    pub(crate) fn of(ops: Operands<'_>, pairs: &[(FaceId, FaceId)]) -> Self {
        let mut t = Self::default();
        let [pa, pb] = &mut t.0;
        for &(fa, fb) in pairs {
            if let Some(f) = ops.a.face(fa) {
                pa.entry(fa).or_insert_with(|| Parts::of(f));
            }
            if let Some(f) = ops.b.face(fb) {
                pb.entry(fb).or_insert_with(|| Parts::of(f));
            }
        }
        t
    }

    /// The faces of `side`.
    const fn side(&self, side: Side) -> &BTreeMap<FaceId, Parts> {
        let [pa, pb] = &self.0;
        match side {
            Side::A => pa,
            Side::B => pb,
        }
    }

    /// The parts of `face` of `side`; a face not touched has none.
    pub(crate) fn parts(&self, side: Side, face: FaceId) -> Option<&Parts> {
        self.side(side).get(&face)
    }
}

/// An entity of one side, for the sweep.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Ent {
    Vertex(VertexId),
    Edge(EdgeId),
    Face(FaceId),
}

impl Ent {
    const fn dim(self) -> u8 {
        match self {
            Self::Vertex(_) => 0,
            Self::Edge(_) => 1,
            Self::Face(_) => 2,
        }
    }
}

/// An entity and its box: a vertex's is its point grown by its coincidence, an edge's
/// and a face's their own (grown by accuracy and resolution). Two entities within
/// coincidence of each other have boxes that meet.
#[derive(Debug, Clone, Copy)]
struct Boxed {
    side: Side,
    ent: Ent,
    at: Extent3d,
}

/// The touched faces of both sides and their edges and vertices, each once, with boxes.
fn boxed(ops: Operands<'_>, touched: &Touched) -> Vec<Boxed> {
    let mut out = Vec::new();
    for side in Side::BOTH {
        let body = ops.of(side);
        let faces = touched.side(side);
        let mut vertices: Vec<VertexId> = faces.values().flat_map(|p| p.vertices.clone()).collect();
        let mut edges: Vec<EdgeId> = faces.values().flat_map(|p| p.edges.clone()).collect();
        vertices.sort_unstable();
        vertices.dedup();
        edges.sort_unstable();
        edges.dedup();
        let vb = vertices.into_iter().filter_map(|v| {
            let vx = body.vertex(v)?;
            let c = ops.coincidence(recorded(vx.accuracy()), None);
            let at = Extent3d::from_point(vx.point())?.expanded(c)?;
            Some(Boxed {
                side,
                ent: Ent::Vertex(v),
                at,
            })
        });
        out.extend(vb);
        out.extend(edges.into_iter().filter_map(|e| {
            Some(Boxed {
                side,
                ent: Ent::Edge(e),
                at: body.edge(e)?.bounding_box()?,
            })
        }));
        out.extend(faces.iter().filter_map(|(&f, p)| {
            Some(Boxed {
                side,
                ent: Ent::Face(f),
                at: p.at?,
            })
        }));
    }
    out
}

/// The test two entities of opposite sides whose boxes meet call for: a vertex against
/// anything, an edge against a face. Edge against edge is found by edge against face (at
/// a hit near the face's boundary); face against face is BO2's.
const fn candidate(p: Boxed, q: Boxed) -> Option<Candidate> {
    let (p, q) = if p.ent.dim() <= q.ent.dim() {
        (p, q)
    } else {
        (q, p)
    };
    match (p.ent, q.ent) {
        (Ent::Vertex(v), Ent::Vertex(w)) => Some(match p.side {
            Side::A => Candidate::Vv(v, w),
            Side::B => Candidate::Vv(w, v),
        }),
        (Ent::Vertex(v), Ent::Edge(e)) => Some(Candidate::Ve(p.side, v, e)),
        (Ent::Vertex(v), Ent::Face(f)) => Some(Candidate::Vf(p.side, v, f)),
        (Ent::Edge(e), Ent::Face(f)) => Some(Candidate::Ef(p.side, e, f)),
        _ => None,
    }
}

/// The candidates among the touched entities: every opposite-side pair whose boxes meet,
/// by a sweep along x, deduplicated and sorted.
fn candidates(ops: Operands<'_>, touched: &Touched) -> Vec<Candidate> {
    let mut items = boxed(ops, touched);
    items.sort_by(|p, q| {
        p.at.x_min()
            .total_cmp(&q.at.x_min())
            .then((p.side, p.ent).cmp(&(q.side, q.ent)))
    });
    let mut out = Vec::new();
    let mut active: [Vec<Boxed>; 2] = [Vec::new(), Vec::new()];
    for &it in &items {
        let [aa, ab] = &mut active;
        aa.retain(|j| j.at.x_max() >= it.at.x_min());
        ab.retain(|j| j.at.x_max() >= it.at.x_min());
        let (mine, theirs) = match it.side {
            Side::A => (aa, &*ab),
            Side::B => (ab, &*aa),
        };
        out.extend(
            theirs
                .iter()
                .filter(|j| !j.at.are_disjoint(&it.at, 0.0))
                .filter_map(|&j| candidate(it, j)),
        );
        mine.push(it);
    }
    out.sort_unstable();
    out.dedup();
    out
}

/// Stage 2 over `pairs` (from [`crate::pairs::face_pairs`]).
///
/// # Errors
///
/// [`BooleanError::Exhausted`] and [`BooleanError::Cancelled`]; a NURBS face or edge where
/// the operands meet is [`BooleanError::Unsupported`]; a surface's or boundary test's
/// failure on valid operands is [`BooleanError::Defect`].
pub(crate) fn interferences(
    ops: Operands<'_>,
    pairs: &[(FaceId, FaceId)],
    budget: &mut Budget,
    stop: &Stop,
) -> Result<Found, BooleanError> {
    let touched = Touched::of(ops, pairs);
    let mut found = Found::default();
    for c in candidates(ops, &touched) {
        stop.check()?;
        match c {
            Candidate::Vv(va, vb) => found.contacts.extend(vv(ops, va, vb)),
            Candidate::Ve(s, v, e) => found.contacts.extend(ve(ops, s, v, e, budget)?),
            Candidate::Vf(s, v, f) => {
                found
                    .contacts
                    .extend(vf(ops, &touched, s, v, f, budget, stop)?);
            }
            Candidate::Ef(s, e, f) => {
                let (contacts, spans) = ef(ops, &touched, s, e, f, budget, stop)?;
                found.contacts.extend(contacts);
                found.spans.extend(spans);
            }
        }
    }
    found.touched = touched;
    Ok(found)
}

/// A vertex of `a` against a vertex of `b`: a distance, no work charged.
fn vv(ops: Operands<'_>, va: VertexId, vb: VertexId) -> Option<Contact> {
    let (p, q) = (ops.a.vertex(va)?, ops.b.vertex(vb)?);
    let reach = ops.coincidence(recorded(p.accuracy()), recorded(q.accuracy()));
    (p.point().distance(q.point()) <= reach).then(|| {
        Contact::of(
            Side::A,
            Member {
                on: On::Vertex(va),
                point: p.point(),
            },
            Member {
                on: On::Vertex(vb),
                point: q.point(),
            },
            reach,
            false,
        )
    })
}

/// A vertex of `side` against an edge of the other: the edge's foot, inside the edge.
fn ve(
    ops: Operands<'_>,
    side: Side,
    v: VertexId,
    e: EdgeId,
    budget: &mut Budget,
) -> Result<Option<Contact>, BooleanError> {
    let (Some(vx), Some(ed)) = (ops.of(side).vertex(v), ops.of(side.other()).edge(e)) else {
        return Ok(None);
    };
    let p = vx.point();
    let reach = ops.coincidence(recorded(vx.accuracy()), recorded(ed.accuracy()));
    if !ed
        .bounding_box()
        .is_some_and(|b| b.contains_point(p, reach))
    {
        return Ok(None);
    }
    let Some((t, foot, d)) = edge_foot(ed, side.other(), p, budget)? else {
        return Ok(None);
    };
    Ok((d <= reach).then(|| {
        Contact::of(
            side,
            Member {
                on: On::Vertex(v),
                point: p,
            },
            Member {
                on: On::Edge { edge: e, t },
                point: foot,
            },
            reach,
            false,
        )
    }))
}

/// A vertex of `side` against a face of the other: the face's foot, inside the face. A
/// foot `Near` the boundary is left to the vertex-vertex and vertex-edge tests, which
/// must then find a boundary entity within coincidence.
fn vf(
    ops: Operands<'_>,
    touched: &Touched,
    side: Side,
    v: VertexId,
    f: FaceId,
    budget: &mut Budget,
    stop: &Stop,
) -> Result<Option<Contact>, BooleanError> {
    let (Some(vx), Some(fc), Some(parts)) = (
        ops.of(side).vertex(v),
        ops.of(side.other()).face(f),
        touched.parts(side.other(), f),
    ) else {
        return Ok(None);
    };
    let p = vx.point();
    let acc = recorded(vx.accuracy());
    let reach = ops.coincidence(acc, parts.accuracy);
    if !parts.at.is_some_and(|b| b.contains_point(p, reach)) {
        return Ok(None);
    }
    let foot = face_foot(fc, side.other(), p, budget, stop)?;
    if foot.distance > reach {
        return Ok(None);
    }
    let fside = side.other();
    match foot.side {
        None => Err(undecided(fside, f)),
        Some(UvSide::Outside) => Ok(None),
        Some(s) => {
            let near = boundary_near(ops, touched, fside, f, foot.point, acc, budget)?;
            match near {
                Near::Clear if s == UvSide::Inside => Ok(Some(Contact::of(
                    side,
                    Member {
                        on: On::Vertex(v),
                        point: p,
                    },
                    Member {
                        on: On::Face {
                            face: f,
                            uv: foot.uv,
                        },
                        point: foot.point,
                    },
                    reach,
                    false,
                ))),
                Near::Clear => Err(near_without(fside, f)),
                Near::Vertex | Near::Edges(_) => Ok(None),
            }
        }
    }
}

/// The contacts and spans of an edge of `side` against a face of the other.
fn ef(
    ops: Operands<'_>,
    touched: &Touched,
    side: Side,
    e: EdgeId,
    f: FaceId,
    budget: &mut Budget,
    stop: &Stop,
) -> Result<(Vec<Contact>, Vec<Span>), BooleanError> {
    let mut contacts = Vec::new();
    let mut spans = Vec::new();
    let fside = side.other();
    let (Some(ed), Some(fc), Some(parts)) = (
        ops.of(side).edge(e),
        ops.of(fside).face(f),
        touched.parts(fside, f),
    ) else {
        return Ok((contacts, spans));
    };
    let EdgeShape::Curve { curve, interval } = ed.shape() else {
        return Ok((contacts, spans));
    };
    let acc = recorded(ed.accuracy());
    let reach = ops.coincidence(acc, parts.accuracy);
    let (Some(eb), Some(fb)) = (ed.bounding_box(), parts.at) else {
        return Ok((contacts, spans));
    };
    if eb.are_disjoint(&fb, reach) {
        return Ok((contacts, spans));
    }
    let operand = fside.operand();
    let uv_box = fc
        .uv_box()
        .ok_or(BooleanError::Defect(BooleanDefect::NoUvBox {
            operand,
            face: f,
        }))?;
    let res = Resolution::new(Length::from_mm(reach)).unwrap_or_else(|| ops.a.resolution());
    let hits = intersect_curve(curve, interval, fc.surface(), &uv_box, res, budget, stop).map_err(
        |x| {
            match (x, curve) {
                // The NURBS one named: the edge when its curve is, else the face.
                (SurfaceError::NurbsIntersection, Curve::Nurbs(_)) => {
                    BooleanError::Unsupported(BooleanUnsupported::Nurbs {
                        operand: side.operand(),
                        at: Entity::Edge(e),
                    })
                }
                (x, _) => BooleanError::of_surface(x, operand, f),
            }
        },
    )?;
    for h in hits {
        match h {
            CurveSurfaceHit::Point {
                t,
                uv,
                point,
                contact,
            } => {
                let s = fc.contains_uv(uv).ok_or_else(|| undecided(fside, f))?;
                if s == UvSide::Outside {
                    continue;
                }
                let tangent = contact == HitContact::Tangent;
                let mine = Member {
                    on: On::Edge { edge: e, t },
                    point,
                };
                match boundary_near(ops, touched, fside, f, point, acc, budget)? {
                    Near::Vertex => {}
                    Near::Edges(near) => {
                        contacts.extend(near.into_iter().map(|(edge, t2, foot, r)| {
                            Contact::of(
                                side,
                                mine,
                                Member {
                                    on: On::Edge { edge, t: t2 },
                                    point: foot,
                                },
                                r,
                                tangent,
                            )
                        }));
                    }
                    Near::Clear if s == UvSide::Inside => contacts.push(Contact::of(
                        side,
                        mine,
                        Member {
                            on: On::Face { face: f, uv },
                            point: fc.surface().point(uv),
                        },
                        reach,
                        tangent,
                    )),
                    Near::Clear => return Err(near_without(fside, f)),
                }
            }
            CurveSurfaceHit::Overlap { t } => spans.push(Span {
                side,
                edge: e,
                face: f,
                t,
            }),
        }
    }
    Ok((contacts, spans))
}

/// What of a face's boundary lies within coincidence of a point.
enum Near {
    /// A boundary vertex: the vertex tests answer there.
    Vertex,
    /// Boundary edges, inside them, and no vertex: each edge, its parameter, foot and the
    /// coincidence distance.
    Edges(Vec<(EdgeId, f64, Point3, f64)>),
    /// Nothing.
    Clear,
}

/// The boundary of `face` of `side` near `p`, a point on an entity of the other side with
/// accuracy `acc`.
fn boundary_near(
    ops: Operands<'_>,
    touched: &Touched,
    side: Side,
    face: FaceId,
    p: Point3,
    acc: Option<Accuracy>,
    budget: &mut Budget,
) -> Result<Near, BooleanError> {
    let Some(parts) = touched.parts(side, face) else {
        return Ok(Near::Clear);
    };
    let body = ops.of(side);
    for v in parts.vertices.iter().filter_map(|&v| body.vertex(v)) {
        if v.point().distance(p) <= ops.coincidence(recorded(v.accuracy()), acc) {
            return Ok(Near::Vertex);
        }
    }
    let mut near = Vec::new();
    for ed in parts.edges.iter().filter_map(|&e| body.edge(e)) {
        let reach = ops.coincidence(recorded(ed.accuracy()), acc);
        if !ed
            .bounding_box()
            .is_some_and(|b| b.contains_point(p, reach))
        {
            continue;
        }
        if let Some((t, foot, d)) = edge_foot(ed, side, p, budget)?
            && d <= reach
        {
            near.push((ed.id(), t, foot, reach));
        }
    }
    Ok(if near.is_empty() {
        Near::Clear
    } else {
        Near::Edges(near)
    })
}

const fn undecided(side: Side, face: FaceId) -> BooleanError {
    BooleanError::Defect(BooleanDefect::UvUndecided {
        operand: side.operand(),
        face,
    })
}

const fn near_without(side: Side, face: FaceId) -> BooleanError {
    BooleanError::Defect(BooleanDefect::NearWithoutBoundary {
        operand: side.operand(),
        face,
    })
}
