// Written for solidcorex. Apache-2.0.
//! [`merge_faces`]: faces proved to lie on one surface become one face (74 §4.3 step 7,
//! ST5).
//!
//! `scx-brep`'s [`Draft::merge_faces`] and [`Draft::join_edges`] merge faces and join edges
//! whose surfaces and curves are *equal* (`==`: the same kind, frame and numbers), which
//! the faces of one cut face are. Faces that come from different places (two boxes laid
//! side by side, a body split by sheets) lie on the same plane with different frames and
//! are not `==`. This module proves them one surface *within the resolution*, makes them
//! `==`, and hands them to the draft's merge:
//!
//! 1. **Candidates.** Each edge of exactly two fins whose faces are both asked for, are two
//!    different faces, lie on the same shells and are used in opposite senses, and whose
//!    surfaces are not `==`: one test of the pair, near-linear in the edges.
//! 2. **Proof (planes).** Face `a` lies on face `b`'s plane when their normals agree in
//!    sense and `|f(q)| + sin(angle) * R` is at most `Resolution::approx` (half the
//!    zone): `q` the face's box centre moved onto its own plane, `f` its signed distance
//!    from `b`'s plane, `R` the box's half diagonal. That bounds the distance of every
//!    point of `a`'s box from `b`'s plane. Other kinds are merged only when `==` (the
//!    draft's own test).
//! 3. **Groups.** Pairs that pass are joined in a union-find, and the lowest face of each
//!    group is its root. Each member is then proved against the root alone, so a chain of
//!    small differences never adds up. A member that passes gets the root's surface and
//!    its pcurves written afresh (all or nothing; a face the writer refuses keeps its
//!    surface and is left).
//! 4. **Merge.** The draft merges every pair of `==` faces whose edges are not protected;
//!    an edge with a fin on a face that was not asked for is protected, so such a face is
//!    never touched.
//! 5. **Edges.** Two lines meeting at a vertex nothing else ends at, bound by the same
//!    faces, which run the same way and whose far end of the second lies within
//!    `Resolution::approx` of the first's line, are made one curve (the second takes the
//!    first's line over the interval that runs on, its faces' pcurves written afresh) and
//!    joined by the draft. Other curves are joined only when `==`.
//!
//! The solid is unchanged to within the resolution: a face moves by at most `approx` onto
//! its group's surface (zero for planes that are the same to the bit), and an edge by at
//! most `approx` onto its neighbour's line. Nothing is merged that is not proved.

use std::collections::{BTreeMap, BTreeSet, VecDeque};

use scx_brep::op::{Cx, Done, Report};
use scx_brep::{
    Brep, Document, Draft, EdgeId, EdgeJoin, EdgeShape, Entity, FaceId, FaceMerge,
    PcurveWriteError, Refusal, VertexId, Why,
};
use scx_curve::Curve;
use scx_num::budget::{Budget, Stop};
use scx_num::extent1d::Extent1d;
use scx_num::extent3d::Extent3d;
use scx_num::units::Length;
use scx_num::vector3d::Point3;
use scx_surface::{NotNear, Plane, Surface, plane_bound};

use crate::error::{StitchDefect, StitchError, StitchRefusal};
use crate::report::NoItems;
use crate::stitch::same_document;

/// Work charged per edge and per surface or curve test.
const UNIT: u64 = 1;

/// A merge's result: the body, and what was merged and what was left.
#[derive(Debug, Clone)]
pub struct Merged {
    /// The body and its [`Changes`](scx_brep::Changes): each absorbed face and edge is a
    /// merge of the lineage, onto its survivor.
    pub done: Done<NoItems>,
    /// What was merged and left.
    pub report: MergeReport,
}

/// What a [`merge_faces`] did.
#[derive(Debug, Clone, Default, PartialEq)]
#[non_exhaustive]
pub struct MergeReport {
    /// The face merges, in the order made: the absorbed face, the survivor (the lowest face
    /// of the group) and the edges removed.
    pub faces: Vec<FaceMerge>,
    /// The edge joins, in the order made.
    pub edges: Vec<EdgeJoin>,
    /// The candidates left as they are, with why, in edge order.
    pub left: Vec<MergeLeft>,
}

/// A candidate [`merge_faces`] left as it is.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub enum MergeLeft {
    /// The two faces across `edge` are not proved on one surface.
    Surfaces {
        /// An edge between them.
        edge: EdgeId,
        /// Why not.
        why: NotOneSurface,
    },
    /// The draft left them: two faces on one surface that `Draft::merge_faces` would not
    /// merge, or two edges `Draft::join_edges_at` would not join.
    Kept {
        /// The edge between the faces, or the vertex between the edges.
        at: Entity,
        /// Why.
        why: Why,
    },
    /// Two lines at `vertex` that lie on one line but that could not be made one curve.
    Lines {
        /// The vertex between them.
        vertex: VertexId,
        /// Why.
        why: NotOneLine,
    },
}

/// Why two faces are not proved to lie on one surface.
#[derive(Debug, Clone, Copy, PartialEq)]
#[non_exhaustive]
pub enum NotOneSurface {
    /// The surfaces are not equal and not both planes: only planes are compared within the
    /// resolution.
    Kind,
    /// Two planes whose normals point against each other: they would merge only by turning
    /// one face over.
    Opposite,
    /// Two planes that differ by more than `approx` somewhere on the face; the least
    /// bound proved on the farther one.
    Apart(Length),
    /// A face reaches infinity or cannot be boxed, so its extent is not known.
    NoBox,
    /// The face's pcurves could not be written on the other's surface.
    Pcurves(PcurveWriteError),
}

/// Why two lines that meet are not made one curve.
#[derive(Debug, Clone, Copy, PartialEq)]
#[non_exhaustive]
pub enum NotOneLine {
    /// They run against each other, or both away from the vertex: joining them would turn
    /// an edge over.
    Direction,
    /// The second's far end lies more than `approx` from the first's line.
    Apart(Length),
    /// A face of the second edge has no exact pcurve on the first's curve.
    Pcurves(PcurveWriteError),
}

fn defect() -> StitchError {
    StitchError::Defect(StitchDefect::Draft)
}

/// The faces `faces` of `body` merged where they are proved to lie on one surface, as the
/// [module](self) says; the other faces are not touched.
///
/// Pass the faces a merge may take in: a face left out is never merged, and keeps every
/// edge it has, so naming all but one face of a split keeps that split. Faces on a shared
/// surface across an edge of three or more fins (a partition between two solids) are not
/// merged: they are bound by different shells.
///
/// # Errors
///
/// [`StitchRefusal::OtherDocument`] for a body finished under another resolution than
/// `cx.doc`'s; [`StitchRefusal::NoSuchFace`] for a face the body does not hold;
/// [`StitchError::Exhausted`] and [`StitchError::Cancelled`] from `cx`; and
/// [`StitchError::Defect`] for the method's own failure (the merged draft did not finish).
pub fn merge_faces(body: &Brep, faces: &[FaceId], cx: &mut Cx<'_>) -> Result<Merged, StitchError> {
    same_document(&[body], cx)?;
    let mut set: BTreeSet<FaceId> = BTreeSet::new();
    for &f in faces {
        body.face(f)
            .ok_or(StitchError::Refused(StitchRefusal::NoSuchFace { face: f }))?;
        set.insert(f);
    }
    let mut budget = cx.budget();
    let doc = *cx.doc;
    let approx = doc.resolution().approx().mm();
    let mut report = MergeReport::default();
    let mut draft = body.edit();
    // An edge with a fin on a face that was not asked for is protected.
    let mut protected: BTreeSet<EdgeId> = BTreeSet::new();
    for e in body.edges() {
        budget.charge(UNIT)?;
        if e.fins().any(|f| !set.contains(&f.face)) {
            protected.insert(e.id());
        }
    }
    // 1. Candidates.
    let mut pairs: BTreeMap<(FaceId, FaceId), EdgeId> = BTreeMap::new();
    for e in body.edges() {
        budget.charge(UNIT)?;
        if !matches!(e.shape(), EdgeShape::Curve { .. }) || protected.contains(&e.id()) {
            continue;
        }
        let [f0, f1] = match e.fins().collect::<Vec<_>>().as_slice() {
            [a, b] => [*a, *b],
            _ => continue,
        };
        if f0.face == f1.face || f0.sense == f1.sense {
            continue;
        }
        let (a, b) = (f0.face.min(f1.face), f0.face.max(f1.face));
        let (Some(fa), Some(fb)) = (body.face(a), body.face(b)) else {
            continue;
        };
        if fa.record().sides != fb.record().sides || fa.surface() == fb.surface() {
            continue;
        }
        let id = e.id();
        pairs.entry((a, b)).or_insert(id);
    }
    // 2. Proof, pair by pair, then 3. groups by union-find on the proved pairs.
    let mut boxes: BTreeMap<FaceId, Option<Extent3d>> = BTreeMap::new();
    let mut parent: BTreeMap<FaceId, FaceId> = BTreeMap::new();
    for (&(a, b), &edge) in &pairs {
        budget.charge(UNIT)?;
        cx.stop.check()?;
        match plane_pair(body, a, b, approx, &mut boxes) {
            Ok(()) => {
                let (ra, rb) = (root(&mut parent, a), root(&mut parent, b));
                if ra != rb {
                    parent.insert(ra.max(rb), ra.min(rb));
                }
            }
            Err(NotOneSurface::Kind) => {}
            Err(why) => report.left.push(MergeLeft::Surfaces { edge, why }),
        }
    }
    let mut groups: BTreeMap<FaceId, Vec<FaceId>> = BTreeMap::new();
    let members: Vec<FaceId> = parent.keys().copied().collect();
    for f in members {
        let r = root(&mut parent, f);
        groups.entry(r).or_default().push(f);
    }
    // Snap each member onto its group's plane: the group's least by geometry (not by id,
    // so the result does not depend on the order the body was built in).
    for members in groups.values() {
        let mut planes: Vec<(FaceId, Plane)> = Vec::new();
        for &m in members {
            if let Some(Surface::Plane(p)) = body.face(m).map(|f| f.surface().clone()) {
                planes.push((m, p));
            }
        }
        let Some((rep_id, rep)) = planes
            .iter()
            .min_by(|(_, p), (_, q)| {
                plane_key(p)
                    .iter()
                    .zip(plane_key(q))
                    .map(|(a, b)| a.total_cmp(&b))
                    .find(|o| o.is_ne())
                    .unwrap_or(core::cmp::Ordering::Equal)
            })
            .copied()
        else {
            continue;
        };
        let rep_surface = Surface::Plane(rep);
        for &m in members {
            budget.charge(UNIT)?;
            if body.face(m).is_some_and(|f| *f.surface() == rep_surface) {
                continue;
            }
            let edge = pairs
                .iter()
                .find(|((a, b), _)| *a == m || *b == m)
                .map_or(EdgeId::from_raw(0), |(_, &e)| e);
            if m != rep_id
                && let Err(why) = plane_against(body, m, &rep_surface, approx, &mut boxes)
            {
                report.left.push(MergeLeft::Surfaces { edge, why });
                continue;
            }
            let old = draft.face(m).ok_or_else(defect)?.surface().clone();
            draft.edit_face(m, |rec| rec.surface = rep_surface.clone());
            if let Err(e) = draft.write_pcurves(m, &doc) {
                draft.edit_face(m, |rec| rec.surface = old);
                report.left.push(MergeLeft::Surfaces {
                    edge,
                    why: NotOneSurface::Pcurves(e),
                });
            }
        }
    }
    // 4. The draft's face merge, on every pair now `==` that no protected edge holds.
    let healed = draft.merge_faces(&protected, &doc).map_err(|_| defect())?;
    report.faces = healed.faces;
    for l in healed.left {
        if !matches!(l.at, Entity::Edge(e) if protected.contains(&e)) {
            report.left.push(MergeLeft::Kept {
                at: l.at,
                why: l.why,
            });
        }
    }
    // 5. Edges: runs of lines made one, then equal curves joined.
    protected.retain(|&e| draft.edge(e).is_some());
    join_edges(
        &mut draft,
        &protected,
        approx,
        doc,
        &mut budget,
        cx.stop,
        &mut report,
    )?;
    let (body, changes) = draft
        .finish(cx.doc)
        .map_err(|u| StitchError::Defect(StitchDefect::Finish(Box::new(u.cause))))?;
    Ok(Merged {
        done: Done::new(body, changes, Report::empty(budget.used())),
        report,
    })
}

fn root(parent: &mut BTreeMap<FaceId, FaceId>, f: FaceId) -> FaceId {
    let mut r = f;
    while let Some(&p) = parent.get(&r) {
        if p == r {
            break;
        }
        r = p;
    }
    parent.insert(f, r);
    r
}

/// The face's box, once.
fn face_box(
    body: &Brep,
    f: FaceId,
    boxes: &mut BTreeMap<FaceId, Option<Extent3d>>,
) -> Option<Extent3d> {
    *boxes
        .entry(f)
        .or_insert_with(|| body.face(f).and_then(scx_brep::Face::bounding_box))
}

/// Whether faces `a` and `b` lie on one plane, each against the other's.
fn plane_pair(
    body: &Brep,
    a: FaceId,
    b: FaceId,
    approx: f64,
    boxes: &mut BTreeMap<FaceId, Option<Extent3d>>,
) -> Result<(), NotOneSurface> {
    let sb = body
        .face(b)
        .map(|f| f.surface().clone())
        .ok_or(NotOneSurface::NoBox)?;
    plane_against(body, a, &sb, approx, boxes)?;
    let sa = body
        .face(a)
        .map(|f| f.surface().clone())
        .ok_or(NotOneSurface::NoBox)?;
    plane_against(body, b, &sa, approx, boxes)
}

/// Whether face `f`, over its box, lies within `approx` of `onto`, a plane with its normal
/// the same way.
fn plane_against(
    body: &Brep,
    f: FaceId,
    onto: &Surface,
    approx: f64,
    boxes: &mut BTreeMap<FaceId, Option<Extent3d>>,
) -> Result<(), NotOneSurface> {
    let own = body.face(f).map(|x| x.surface().clone());
    let (Some(Surface::Plane(a)), Surface::Plane(b)) = (own, onto) else {
        return Err(NotOneSurface::Kind);
    };
    let bx = face_box(body, f, boxes).ok_or(NotOneSurface::NoBox)?;
    // A crease between faces that are not near parallel is no near miss: not reported.
    let dev = plane_bound(&a, b, &bx).map_err(|e| match e {
        NotNear::Opposite => NotOneSurface::Opposite,
        _ => NotOneSurface::Kind,
    })?;
    if dev <= approx {
        Ok(())
    } else {
        Err(NotOneSurface::Apart(Length::from_mm(dev)))
    }
}

/// A key that orders planes by their frame.
fn plane_key(p: &Plane) -> [f64; 9] {
    let f = p.frame();
    let (o, z, x) = (f.origin(), f.z(), f.x());
    [o.x, o.y, o.z, z.x, z.y, z.z, x.x, x.y, x.z]
}

/// A line edge of two faces that can be one edge with its neighbours.
struct Seg {
    line: scx_curve::Line,
    ends: [VertexId; 2],
    faces: [FaceId; 2],
}

fn seg(draft: &Draft, e: EdgeId, protected: &BTreeSet<EdgeId>) -> Option<Seg> {
    if protected.contains(&e) {
        return None;
    }
    let edge = draft.edge(e)?;
    let g = edge.geometry();
    let EdgeShape::Curve {
        curve: Curve::Line(line),
        ..
    } = &g.shape
    else {
        return None;
    };
    let fins: Vec<_> = edge.fins().collect();
    let [f0, f1] = fins.as_slice() else {
        return None;
    };
    if f0.face == f1.face || g.ends[0] == g.ends[1] {
        return None;
    }
    let mut faces = [f0.face, f1.face];
    faces.sort_unstable();
    Some(Seg {
        line: *line,
        ends: g.ends,
        faces,
    })
}

/// The runs of line edges that meet at vertices nothing else ends at, bound by the same two
/// faces and lying on one line within `approx`, made one curve and joined; then every pair
/// of equal curves meeting so, joined. Each join is the draft's.
fn join_edges(
    draft: &mut Draft,
    protected: &BTreeSet<EdgeId>,
    approx: f64,
    doc: Document,
    budget: &mut Budget,
    stop: &Stop,
    report: &mut MergeReport,
) -> Result<(), StitchError> {
    let mut star: BTreeMap<VertexId, Vec<EdgeId>> = BTreeMap::new();
    let ids: Vec<EdgeId> = draft.edges().map(scx_brep::Edge::id).collect();
    for &e in &ids {
        budget.charge(UNIT)?;
        let ends = draft.edge(e).ok_or_else(defect)?.geometry().ends;
        star.entry(ends[0]).or_default().push(e);
        if ends[1] != ends[0] {
            star.entry(ends[1]).or_default().push(e);
        }
    }
    let other =
        |star: &BTreeMap<VertexId, Vec<EdgeId>>, v: VertexId, e: EdgeId| -> Option<EdgeId> {
            match star.get(&v)?.as_slice() {
                [a, b] if *a == e => Some(*b),
                [a, b] if *b == e => Some(*a),
                _ => None,
            }
        };
    let mut seen: BTreeSet<EdgeId> = BTreeSet::new();
    let mut interior: BTreeSet<VertexId> = BTreeSet::new();
    for &e in &ids {
        stop.check()?;
        if seen.contains(&e) {
            continue;
        }
        let Some(first) = seg(draft, e, protected) else {
            continue;
        };
        seen.insert(e);
        // Grow the run through free vertices both ways.
        let mut edges: VecDeque<EdgeId> = VecDeque::from([e]);
        let mut verts: VecDeque<VertexId> = VecDeque::from(first.ends);
        for back in [true, false] {
            loop {
                budget.charge(UNIT)?;
                let (Some(&v), Some(&last)) = (
                    if back { verts.back() } else { verts.front() },
                    if back { edges.back() } else { edges.front() },
                ) else {
                    break;
                };
                let Some(next) = other(&star, v, last) else {
                    break;
                };
                let Some(s) = seg(draft, next, protected) else {
                    break;
                };
                if seen.contains(&next) || s.faces != first.faces {
                    break;
                }
                let far = if s.ends[0] == v { s.ends[1] } else { s.ends[0] };
                if (s.ends[0] != v && s.ends[1] != v) || verts.contains(&far) {
                    break;
                }
                seen.insert(next);
                if back {
                    edges.push_back(next);
                    verts.push_back(far);
                } else {
                    edges.push_front(next);
                    verts.push_front(far);
                }
            }
        }
        if edges.len() < 2 {
            continue;
        }
        let edges: Vec<EdgeId> = edges.into();
        let verts: Vec<VertexId> = verts.into();
        match one_run(draft, &edges, &verts, approx, doc)? {
            Ok(()) => {}
            Err(why) => {
                report.left.push(MergeLeft::Lines {
                    vertex: verts.get(1).copied().ok_or_else(defect)?,
                    why,
                });
                continue;
            }
        }
        for &v in verts.iter().skip(1).take(edges.len() - 1) {
            interior.insert(v);
            join_at(draft, v, protected, report);
        }
    }
    // Equal curves that are not lines (two arcs of one circle).
    for (&v, es) in &star {
        budget.charge(UNIT)?;
        let [e1, e2] = es.as_slice() else { continue };
        if interior.contains(&v) || protected.contains(e1) || protected.contains(e2) {
            continue;
        }
        let (Some(a), Some(b)) = (draft.edge(*e1), draft.edge(*e2)) else {
            continue;
        };
        if matches!(&a.geometry().shape, EdgeShape::Curve { curve, .. } if !matches!(curve, Curve::Line(_)))
            && matches!((&a.geometry().shape, &b.geometry().shape), (EdgeShape::Curve { curve: c, .. }, EdgeShape::Curve { curve: d, .. }) if c == d)
        {
            join_at(draft, v, protected, report);
        }
    }
    Ok(())
}

fn join_at(draft: &mut Draft, v: VertexId, protected: &BTreeSet<EdgeId>, report: &mut MergeReport) {
    match draft.join_edges_at(v, protected) {
        Ok(j) => report.edges.push(j),
        Err(Refusal::Left(why)) => report.left.push(MergeLeft::Kept {
            at: Entity::Vertex(v),
            why,
        }),
        Err(_) => {}
    }
}

/// One run of line edges `edges` (each between its two neighbours' vertices, `verts[j]` to
/// `verts[j + 1]`) put on one line: oriented so the run starts at its least end, on the
/// line of the first edge, each edge over the span that runs on from the one before, and
/// their faces' pcurves written afresh. All or nothing.
fn one_run(
    draft: &mut Draft,
    edges: &[EdgeId],
    verts: &[VertexId],
    approx: f64,
    doc: Document,
) -> Result<Result<(), NotOneLine>, StitchError> {
    let point =
        |d: &Draft, v: VertexId| d.vertex(v).map(scx_brep::Vertex::point).ok_or_else(defect);
    let mut edges = edges.to_vec();
    let mut verts = verts.to_vec();
    let (p0, pk) = (
        point(draft, *verts.first().ok_or_else(defect)?)?,
        point(draft, *verts.last().ok_or_else(defect)?)?,
    );
    let lexicographic = |a: Point3, b: Point3| {
        a.x.total_cmp(&b.x)
            .then(a.y.total_cmp(&b.y))
            .then(a.z.total_cmp(&b.z))
    };
    if lexicographic(p0, pk).is_gt() {
        edges.reverse();
        verts.reverse();
    }
    let first = edges.first().copied().ok_or_else(defect)?;
    let Some(s0) = seg(draft, first, &BTreeSet::new()) else {
        return Err(defect());
    };
    // The canonical line: the first edge's, run the way the run runs.
    let start = verts.first().copied().ok_or_else(defect)?;
    let line = if s0.ends[0] == start {
        s0.line
    } else {
        s0.line.reversed()
    };
    let d = line.direction();
    let t_of = |p: Point3| (p - line.origin()).dot(d) / d.length_squared();
    let mut ts = Vec::with_capacity(verts.len());
    let mut prev = f64::NEG_INFINITY;
    for &v in &verts {
        let p = point(draft, v)?;
        if p.distance(line.point(t_of(p))) > approx {
            return Ok(Err(NotOneLine::Apart(Length::from_mm(
                p.distance(line.point(t_of(p))),
            ))));
        }
        let t = t_of(p);
        if t <= prev {
            return Ok(Err(NotOneLine::Direction));
        }
        prev = t;
        ts.push(t);
    }
    // The edges as they are, to put back.
    let mut before = Vec::new();
    let mut touched: BTreeSet<FaceId> = BTreeSet::new();
    for (j, &e) in edges.iter().enumerate() {
        let (Some(&a), Some(&b), Some(&lo), Some(&hi)) =
            (verts.get(j), verts.get(j + 1), ts.get(j), ts.get(j + 1))
        else {
            return Err(defect());
        };
        let lo = if j == 0 {
            lo
        } else {
            ts.get(j).copied().unwrap_or(lo)
        };
        let (old_geom, fins) = {
            let edge = draft.edge(e).ok_or_else(defect)?;
            (edge.geometry().clone(), edge.fins().collect::<Vec<_>>())
        };
        let flip = old_geom.ends != [a, b];
        let new_shape = EdgeShape::Curve {
            curve: Curve::Line(line),
            interval: Extent1d::new(lo, hi).ok_or_else(defect)?,
        };
        if new_shape == old_geom.shape && !flip {
            continue;
        }
        before.push((e, old_geom.shape, old_geom.ends));
        if let Some(g) = draft.edge_mut(e) {
            g.shape = new_shape;
            g.ends = [a, b];
        }
        for fin in fins {
            touched.insert(fin.face);
            if flip {
                draft.edit_face(fin.face, |rec| {
                    for u in rec.loops.iter_mut().flat_map(|l| l.uses.iter_mut()) {
                        if u.edge == e {
                            u.sense = u.sense.reversed();
                        }
                    }
                });
            }
        }
    }
    let mut failed = None;
    for &f in &touched {
        if let Err(e) = draft.write_pcurves(f, &doc) {
            failed = Some(e);
            break;
        }
    }
    if let Some(e) = failed {
        for (edge, shape, ends) in before {
            let flipped = draft.edge(edge).is_some_and(|x| x.geometry().ends != ends);
            let fins: Vec<_> = draft
                .edge(edge)
                .map_or_else(Vec::new, |x| x.fins().collect());
            if let Some(g) = draft.edge_mut(edge) {
                g.shape = shape;
                g.ends = ends;
            }
            if flipped {
                for fin in fins {
                    draft.edit_face(fin.face, |rec| {
                        for u in rec.loops.iter_mut().flat_map(|l| l.uses.iter_mut()) {
                            if u.edge == edge {
                                u.sense = u.sense.reversed();
                            }
                        }
                    });
                }
            }
        }
        for &f in &touched {
            draft.write_pcurves(f, &doc).map_err(|_| defect())?;
        }
        return Ok(Err(NotOneLine::Pcurves(e)));
    }
    Ok(Ok(()))
}
