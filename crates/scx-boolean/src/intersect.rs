// Written for solidcorex. Apache-2.0.
//! [`intersect`]: where two bodies meet, as a wire body (BO18; 75 §7).
//!
//! Stages 1 to 4 of [`boolean`](crate::boolean) and nothing after: no imprint, no
//! classification, no selection. The answer is a body of no faces:
//!
//! - every kept section of stage 4 is a wire edge on its curve, between the vertices of its
//!   merged points (a closed curve no merged point lies on gets one vertex of its own);
//! - every operand edge lying on a face of the other is a wire edge on the edge's curve over
//!   the piece, written once even where the other operand's edge runs along it (a piece
//!   both operands' edges lie on is one edge);
//! - every isolated point where the surfaces touch, and every merged point no edge ends
//!   at, is a lone vertex.
//!
//! A coincident pair is a region, not a curve or a point: only its boundary, the operand
//! edges lying on the other face, is part of the answer.
//!
//! Each connected piece of the answer is one shell of the outside region (a shell is
//! connected, so two circles apart are two shells); a lone vertex is a shell of its own.
//! Bodies that do not meet give a body with no shells. Entities are written in stage-4
//! order (sections, then lying pieces, then touches, then merged points), so the same
//! operands give the same body.

use std::collections::BTreeMap;
use std::sync::Arc;

use scx_brep::op::{Cx, Done, Report};
use scx_brep::{
    Brep, Draft, EdgeGeom, EdgeId, EdgeShape, Entity, Origin, RegionId, ShellRecord, VertexId,
    VertexRecord,
};
use scx_num::cert::{Basis, Recorded};
use scx_num::units::{Accuracy, Length};
use scx_num::vector3d::Point3;

use crate::boolean::{Interference, NoItems, interfere};
use crate::contact::On;
use crate::error::{BooleanDefect, BooleanError, BooleanRefusal};
use crate::faceface::{FaceFace, face_face};
use crate::geom::{Operands, Side};
use crate::points::Point;

/// The curves and points where `a` and `b` meet, as a wire body of `cx.doc`: stages 1 to 4
/// of [`boolean`](crate::boolean), the sections and operand edges lying on the other body
/// as edges, isolated touches as lone vertices. The body has no faces; entities derive
/// from the operands' entities they lie on (`Origin::Operand`, operand 0 for `a`).
///
/// # Errors
///
/// As [`boolean`](crate::boolean) up to its face-face stage: [`BooleanRefusal::OtherDocument`],
/// [`BooleanRefusal::VerticesWithinCoincidence`], the `Unsupported` causes of stages 2 to 4,
/// [`BooleanError::Exhausted`], [`BooleanError::Cancelled`] and [`BooleanError::Defect`].
pub fn intersect(a: &Brep, b: &Brep, cx: &mut Cx<'_>) -> Result<Done<NoItems>, BooleanError> {
    let res = cx.doc.resolution();
    for (side, body) in Side::BOTH.into_iter().zip([a, b]) {
        if body.resolution() != res {
            return Err(BooleanError::Refused(BooleanRefusal::OtherDocument {
                operand: side.operand(),
            }));
        }
    }
    let mut budget = cx.budget();
    let ops = Operands { a, b };
    let found = interfere(ops, &mut budget, cx.stop)?;
    let ff = face_face(ops, &found, &mut budget, cx.stop)?;
    let (body, changes) = wire(ops, &found, &ff, *cx.doc)?;
    Ok(Done::new(body, changes, Report::empty(budget.used())))
}

/// The wire body of stage 4's answer `ff` over `found`, of `doc` ([`intersect`]'s body).
pub(crate) fn wire(
    ops: Operands<'_>,
    found: &Interference,
    ff: &FaceFace,
    doc: scx_brep::Document,
) -> Result<(Brep, scx_brep::Changes), BooleanError> {
    let mut w = Wire {
        ops,
        zone: doc.resolution().zone().mm(),
        draft: Draft::new(),
        points: &found.points,
        at: BTreeMap::new(),
        verts: Vec::new(),
        edges: Vec::new(),
        lone: Vec::new(),
    };
    w.write(ff)?;
    w.finish(doc)
}

fn draft_defect<E>(_: E) -> BooleanError {
    BooleanError::Defect(BooleanDefect::Draft)
}

/// A `Measured` record of `mm`.
fn measured(mm: f64) -> Option<Arc<Recorded>> {
    Accuracy::new(Length::from_mm(mm)).map(|acc| {
        Arc::new(Recorded {
            acc,
            basis: Basis::Measured,
        })
    })
}

struct Wire<'a> {
    ops: Operands<'a>,
    /// The resolution's zone, in mm.
    zone: f64,
    draft: Draft,
    points: &'a [Point],
    /// The draft vertex of each merged point used so far.
    at: BTreeMap<usize, VertexId>,
    /// Every vertex written, by position, in order.
    verts: Vec<(VertexId, Point3)>,
    /// Every edge written, with the vertices it joins and a point inside it (for dedupe).
    edges: Vec<(EdgeId, [VertexId; 2], Point3)>,
    /// Vertices that are no edge's end.
    lone: Vec<VertexId>,
}

impl Wire<'_> {
    fn write(&mut self, ff: &FaceFace) -> Result<(), BooleanError> {
        let one = Side::B.operand();
        for s in &ff.sections {
            let [fa, fb] = s.faces;
            let ends = if let Some([i, j]) = s.ends {
                [self.point(i)?, self.point(j)?]
            } else {
                let v = self.fresh(s.curve.point(s.t.min()), 0.0)?;
                self.derive(v, Origin::Operand(Side::A.operand(), Entity::Face(fa)))?;
                self.derive(v, Origin::Operand(one, Entity::Face(fb)))?;
                [v, v]
            };
            let mid = s.curve.point(s.t.mid());
            let e = self.edge(
                EdgeShape::Curve {
                    curve: s.curve.clone(),
                    interval: s.t,
                },
                ends,
                mid,
            )?;
            if let Some(e) = e {
                self.derive(e, Origin::Operand(Side::A.operand(), Entity::Face(fa)))?;
                self.derive(e, Origin::Operand(one, Entity::Face(fb)))?;
            }
        }
        for l in &ff.lying {
            let edge = self
                .ops
                .of(l.side)
                .edge(l.edge)
                .ok_or(BooleanError::Defect(BooleanDefect::Draft))?;
            let EdgeShape::Curve { curve, .. } = edge.shape() else {
                continue;
            };
            let ends = [self.point(l.ends[0])?, self.point(l.ends[1])?];
            let mid = curve.point(l.t.mid());
            let shape = EdgeShape::Curve {
                curve: curve.clone(),
                interval: l.t,
            };
            if let Some(e) = self.edge(shape, ends, mid)? {
                self.derive(e, Origin::Operand(l.side.operand(), Entity::Edge(l.edge)))?;
                self.derive(
                    e,
                    Origin::Operand(l.side.other().operand(), Entity::Face(l.face)),
                )?;
            }
        }
        for t in &ff.touches {
            let v = match t.point {
                Some(i) => self.point(i)?,
                None => {
                    if let Some(&(v, _)) = self
                        .verts
                        .iter()
                        .find(|(_, p)| p.distance(t.at) <= self.zone)
                    {
                        v
                    } else {
                        let v = self.fresh(t.at, 0.0)?;
                        self.derive(
                            v,
                            Origin::Operand(Side::A.operand(), Entity::Face(t.faces[0])),
                        )?;
                        self.derive(
                            v,
                            Origin::Operand(Side::B.operand(), Entity::Face(t.faces[1])),
                        )?;
                        v
                    }
                }
            };
            self.lone.push(v);
        }
        // Merged points nothing above used: a place the operands meet and no curve ends at.
        for i in 0..self.points.len() {
            if !self.at.contains_key(&i) {
                let v = self.point(i)?;
                self.lone.push(v);
            }
        }
        Ok(())
    }

    fn derive(&mut self, new: impl Into<Tracked>, origin: Origin) -> Result<(), BooleanError> {
        match new.into() {
            Tracked::Vertex(v) => self.draft.derive(v, origin),
            Tracked::Edge(e) => self.draft.derive(e, origin),
        }
        .map_err(draft_defect)
    }

    /// A new vertex at `at`, recording `gap` as its accuracy when above the zone.
    fn fresh(&mut self, at: Point3, gap: f64) -> Result<VertexId, BooleanError> {
        let mut rec = VertexRecord::new(at);
        if gap > self.zone {
            rec.accuracy = measured(gap);
        }
        let v = self.draft.add_vertex(rec).map_err(draft_defect)?;
        self.verts.push((v, at));
        Ok(v)
    }

    /// The draft vertex of merged point `i`, made on first use.
    fn point(&mut self, i: usize) -> Result<VertexId, BooleanError> {
        if let Some(&v) = self.at.get(&i) {
            return Ok(v);
        }
        let p = self
            .points
            .get(i)
            .ok_or(BooleanError::Defect(BooleanDefect::Draft))?;
        let v = self.fresh(p.at, p.gap)?;
        for (side, on) in Side::BOTH.into_iter().zip(&p.on) {
            for o in on {
                let entity = match *o {
                    On::Vertex(v) => Entity::Vertex(v),
                    On::Edge { edge, .. } => Entity::Edge(edge),
                    On::Face { face, .. } => Entity::Face(face),
                };
                self.derive(v, Origin::Operand(side.operand(), entity))?;
            }
        }
        self.at.insert(i, v);
        Ok(v)
    }

    /// An edge of `shape` between `ends`, unless an edge between the same vertices through
    /// the same midpoint (within the zone) is already written: the one piece two operand
    /// edges both lie along.
    fn edge(
        &mut self,
        shape: EdgeShape,
        ends: [VertexId; 2],
        mid: Point3,
    ) -> Result<Option<EdgeId>, BooleanError> {
        let same = |p: [VertexId; 2]| p == ends || p == [ends[1], ends[0]];
        if self
            .edges
            .iter()
            .any(|&(_, p, m)| same(p) && m.distance(mid) <= self.zone)
        {
            return Ok(None);
        }
        let e = self
            .draft
            .add_edge(EdgeGeom::new(shape, ends))
            .map_err(draft_defect)?;
        self.edges.push((e, ends, mid));
        Ok(Some(e))
    }

    /// One shell of the outside region per connected piece: edges joined at vertices, and
    /// each lone vertex that no edge ends at alone.
    fn finish(
        mut self,
        doc: scx_brep::Document,
    ) -> Result<(Brep, scx_brep::Changes), BooleanError> {
        let n = self.verts.len();
        let index: BTreeMap<VertexId, usize> = self
            .verts
            .iter()
            .enumerate()
            .map(|(k, &(v, _))| (v, k))
            .collect();
        let mut parent: Vec<usize> = (0..n).collect();
        for &(_, [v0, v1], _) in &self.edges {
            let (Some(&i), Some(&j)) = (index.get(&v0), index.get(&v1)) else {
                return Err(BooleanError::Defect(BooleanDefect::Draft));
            };
            let (ri, rj) = (find(&mut parent, i), find(&mut parent, j));
            let (lo, hi) = (ri.min(rj), ri.max(rj));
            if let Some(slot) = parent.get_mut(hi) {
                *slot = lo;
            }
        }
        let mut shells: BTreeMap<usize, ShellRecord> = BTreeMap::new();
        for &(e, [v0, _], _) in &self.edges {
            let root = index.get(&v0).map_or(0, |&i| find(&mut parent, i));
            shells
                .entry(root)
                .or_insert_with(|| ShellRecord::new(RegionId::OUTSIDE))
                .wires
                .push(e);
        }
        let on_edge: std::collections::BTreeSet<VertexId> =
            self.edges.iter().flat_map(|&(_, p, _)| p).collect();
        for &(v, _) in &self.verts {
            if !on_edge.contains(&v) && self.lone.contains(&v) {
                let root = index.get(&v).map_or(0, |&i| find(&mut parent, i));
                shells
                    .entry(root)
                    .or_insert_with(|| ShellRecord::new(RegionId::OUTSIDE))
                    .lone_vertices
                    .push(v);
            }
        }
        for (_, s) in shells {
            self.draft.add_shell(s).map_err(draft_defect)?;
        }
        self.draft
            .finish(&doc)
            .map_err(|_| BooleanError::Defect(BooleanDefect::Build))
    }
}

/// The root of `i` in a union-find over indices; the root of a set is its lowest index.
fn find(parent: &mut [usize], mut i: usize) -> usize {
    while let Some(&p) = parent.get(i) {
        if p == i {
            break;
        }
        i = p;
    }
    i
}

/// A new entity of the wire body.
enum Tracked {
    Vertex(VertexId),
    Edge(EdgeId),
}

impl From<VertexId> for Tracked {
    fn from(v: VertexId) -> Self {
        Self::Vertex(v)
    }
}

impl From<EdgeId> for Tracked {
    fn from(e: EdgeId) -> Self {
        Self::Edge(e)
    }
}
