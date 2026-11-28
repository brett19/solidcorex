// Written for solidcorex. Apache-2.0.
//! Stage 5: imprint (BO3, 72 §14.26).
//!
//! Into `a.edit()`: every face of both operands split by the sections and lying pieces
//! stage 4 found, every new edge with its exact pcurve on each face it bounds, and a copy
//! of `b`'s faces sharing the new edges and vertices with `a`'s. Nothing is classified or
//! removed: the draft holds both operands whole, cut where they meet, for BO4.
//!
//! - **Vertices.** Each merged point is one draft vertex: `a`'s vertex when it is one (a
//!   `b` vertex there is recorded `coincident` onto it), a copy of `b`'s when only `b` has
//!   one, else a new vertex at the point, `Measured` by its gap when above the zone. Every
//!   other vertex of `b` is copied.
//! - **Edges.** Each operand edge is cut at the merged points inside it, in parameter
//!   order. An edge of `a` keeps its id for the first piece; the others are new, from it.
//!   Every piece of `b`'s is new, except one that runs along a piece of `a`'s (a lying
//!   piece `along` an edge): that piece is `a`'s, `b`'s edge recorded `coincident` onto
//!   it. Each section is a new edge between its merged points (a closed one no point lies
//!   on gets a new vertex at its start), from both faces. An edge whose curve ends farther
//!   from its vertex than the two reaches allow records the gap as its accuracy.
//! - **Faces.** A face with no cut keeps its loops; a use of a split edge becomes the
//!   pieces' uses with the same pcurve over each piece's interval, so the bits stay (72 §9
//!   row 5). A use of a piece shared with `a` gets the exact pcurve on the face.
//!   A face with cuts is traced: each boundary piece and each cut, both ways, is a
//!   half-edge, and at each vertex the next half-edge is the first one clockwise from the
//!   way back, about the surface's normal there (`S_u x S_v`, so the face lies on the
//!   left as its uv loops run counter-clockwise). Each cycle is written by
//!   [`Draft::exact_loops`]; a counter-clockwise one in uv bounds a piece, a clockwise one
//!   is a hole of the smallest piece around it. A pole the cycle passes gets the face's
//!   collapsed edge there. `a`'s piece holding the face's first boundary use keeps its
//!   id.
//! - **Lineage** (72 §5.6). `derive` from the face or edge cut for each new piece of `a`,
//!   from `b`'s entity (as operand 1) for each copy, and from both faces for a section;
//!   a new vertex from every entity through its point.
//!
//! Not here: a seam the cut faces' pieces cross stays an edge between two pieces (BO4's
//! merge joins them), and an edge with more than two fins keeps them in the order they
//! were added (BO4 removes faces before the body is finished).

use core::f64::consts::TAU;
use std::collections::BTreeMap;
use std::sync::Arc;

use scx_brep::{
    Document, Draft, EdgeGeom, EdgeId, EdgeShape, EdgeUse, Entity, ExactLoop, FaceId, FaceRecord,
    LoopRecord, Origin, RegionId, ShellId, ShellRecord, UseGeom, VertexId, VertexRecord,
};
use scx_num::budget::{Budget, Stop};
use scx_num::cert::{Basis, Recorded};
use scx_num::extent1d::Extent1d;
use scx_num::fmath::atan2;
use scx_num::frame::Sense;
use scx_num::units::{Accuracy, Length};
use scx_num::vector2d::{Point2, Vec2};
use scx_num::vector3d::{Point3, UnitVec3, Vec3};
use scx_nurbs::Approach;
use scx_surface::Surface;

use crate::boolean::Interference;
use crate::contact::On;
use crate::error::{BooleanDefect, BooleanError, BooleanUnsupported};
use crate::faceface::FaceFace;
use crate::geom::{Operands, Side};

/// Two directions closer than this, in radians, are one: the chords decide.
const TIE: f64 = 1e-7;

/// The fraction of a half-edge its chord runs over, for ties.
const CHORD: f64 = 1.0 / 64.0;

/// Samples per use when a loop is measured in uv.
const SAMPLES: usize = 16;

/// What the imprint built: the draft, and where each operand face went.
#[derive(Debug)]
pub(crate) struct Imprint {
    /// `a.edit()`, cut, with `b`'s faces copied in.
    pub(crate) draft: Draft,
    /// Each operand face's pieces in the draft, by side and face, ascending.
    pub(crate) pieces: BTreeMap<(Side, FaceId), Vec<FaceId>>,
    /// The new edges where the operands cross: the sections and the lying pieces inside a
    /// face, ascending. BO4's classification starts from them.
    pub(crate) cuts: Vec<EdgeId>,
    /// Each stage-4 section's edge, in the sections' order: it runs along the section's
    /// curve over its span, so a use of it in its own sense runs along the curve.
    pub(crate) sections: Vec<EdgeId>,
}

/// `b`'s pieces that run along `a`'s, by `b`'s edge and piece index: `a`'s draft edge,
/// and whether it runs against `b`'s.
type Shared = BTreeMap<(EdgeId, usize), (EdgeId, bool)>;

/// A traced cycle: its loop as written alone, its uv samples and twice its signed area.
type Ring = (ExactLoop, Vec<Point2>, f64);

/// One piece of an operand edge in the draft.
#[derive(Debug, Clone, Copy)]
struct Piece {
    /// The draft edge.
    edge: EdgeId,
    /// Whether the draft edge runs against the operand edge (a piece of `b` shared with
    /// `a`).
    reversed: bool,
    /// Whether it is `b`'s piece written as one of `a`'s.
    shared: bool,
    /// The operand edge's parameters over the piece; `None` for a collapsed edge.
    t: Option<Extent1d>,
    /// The draft vertices at the piece's start and end, along the operand edge.
    ends: [VertexId; 2],
}

/// Stage 5 over `ops`, from stages 1 to 4.
pub(crate) fn imprint(
    ops: Operands<'_>,
    found: &Interference,
    ff: &FaceFace,
    doc: Document,
    budget: &mut Budget,
    stop: &Stop,
) -> Result<Imprint, BooleanError> {
    let mut b = Build {
        ops,
        doc,
        draft: ops.a.edit(),
        at: Vec::with_capacity(found.points.len()),
        bv: BTreeMap::new(),
        edges: BTreeMap::new(),
        cut: BTreeMap::new(),
        cuts: Vec::new(),
        sections: Vec::new(),
        made: Vec::new(),
        pieces: BTreeMap::new(),
    };
    b.vertices(found)?;
    b.split(found)?;
    b.sections(ff)?;
    b.lying(ff)?;
    b.reach()?;
    let shells = b.shells()?;
    for side in Side::BOTH {
        for face in ops.of(side).faces() {
            stop.check()?;
            b.face(side, face.id(), &shells, budget, stop)?;
        }
    }
    let mut cuts = b.cuts;
    cuts.sort_unstable();
    cuts.dedup();
    Ok(Imprint {
        draft: b.draft,
        pieces: b.pieces,
        cuts,
        sections: b.sections,
    })
}

/// The draft's own failure: an id table full, or a lineage call refused.
pub(crate) fn draft_defect<E>(_: E) -> BooleanError {
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

/// The origin an entity of `side` stands for in the draft's lineage.
const fn origin(side: Side, on: &On) -> Origin {
    let entity = match *on {
        On::Vertex(v) => Entity::Vertex(v),
        On::Edge { edge, .. } => Entity::Edge(edge),
        On::Face { face, .. } => Entity::Face(face),
    };
    match (side, entity) {
        (Side::A, Entity::Edge(e)) => Origin::Edge(e),
        (Side::A, Entity::Face(f)) => Origin::Face(f),
        (Side::A, Entity::Vertex(v)) => Origin::Vertex(v),
        (_, e) => Origin::Operand(side.operand(), e),
    }
}

struct Build<'a> {
    ops: Operands<'a>,
    doc: Document,
    draft: Draft,
    /// The draft vertex of each merged point.
    at: Vec<VertexId>,
    /// `b`'s vertices in the draft.
    bv: BTreeMap<VertexId, VertexId>,
    /// Each operand edge's pieces, along it.
    edges: BTreeMap<(Side, EdgeId), Vec<Piece>>,
    /// The cuts of each operand face: draft edges inside it.
    cut: BTreeMap<(Side, FaceId), Vec<EdgeId>>,
    /// Every cut.
    cuts: Vec<EdgeId>,
    /// Each section's edge, in order.
    sections: Vec<EdgeId>,
    /// The draft edges made or rewritten here, whose ends `reach` checks.
    made: Vec<EdgeId>,
    pieces: BTreeMap<(Side, FaceId), Vec<FaceId>>,
}

impl Build<'_> {
    /// The draft vertex of each merged point, and a copy of each other vertex of `b`.
    fn vertices(&mut self, found: &Interference) -> Result<(), BooleanError> {
        let zone = self.doc.resolution().zone().mm();
        let one = Side::B.operand();
        for p in &found.points {
            let [on_a, on_b] = &p.on;
            let v = match p.anchor {
                Some((Side::A, va)) => {
                    for on in on_b {
                        if let On::Vertex(vb) = *on {
                            self.draft.coincident(one, vb, va).map_err(draft_defect)?;
                            self.bv.insert(vb, va);
                        }
                    }
                    va
                }
                Some((Side::B, vb)) => {
                    let rec = self
                        .ops
                        .b
                        .vertex(vb)
                        .ok_or(BooleanError::Defect(BooleanDefect::Draft))?
                        .record()
                        .clone();
                    let v = self.draft.add_vertex(rec).map_err(draft_defect)?;
                    self.bv.insert(vb, v);
                    v
                }
                None => {
                    let mut rec = VertexRecord::new(p.at);
                    if p.gap > zone {
                        rec.accuracy = measured(p.gap);
                    }
                    self.draft.add_vertex(rec).map_err(draft_defect)?
                }
            };
            if !matches!(p.anchor, Some((Side::A, _))) {
                for (side, on) in [(Side::A, on_a), (Side::B, on_b)] {
                    for o in on {
                        self.draft
                            .derive(v, origin(side, o))
                            .map_err(draft_defect)?;
                    }
                }
            }
            self.at.push(v);
        }
        for vb in self.ops.b.vertices() {
            if self.bv.contains_key(&vb.id()) {
                continue;
            }
            let v = self
                .draft
                .add_vertex(vb.record().clone())
                .map_err(draft_defect)?;
            self.draft
                .derive(
                    v,
                    Origin::Operand(Side::B.operand(), Entity::Vertex(vb.id())),
                )
                .map_err(draft_defect)?;
            self.bv.insert(vb.id(), v);
        }
        Ok(())
    }

    /// The draft vertex of `side`'s vertex `v`.
    fn vertex(&self, side: Side, v: VertexId) -> Result<VertexId, BooleanError> {
        match side {
            Side::A => Ok(v),
            Side::B => self
                .bv
                .get(&v)
                .copied()
                .ok_or(BooleanError::Defect(BooleanDefect::Draft)),
        }
    }

    /// Cuts every operand edge at the merged points inside it: `a`'s in the draft, `b`'s
    /// as copies, sharing `a`'s pieces where a lying piece runs along one.
    fn split(&mut self, found: &Interference) -> Result<(), BooleanError> {
        let mut inside: [BTreeMap<EdgeId, Vec<(f64, usize)>>; 2] = Default::default();
        for (i, p) in found.points.iter().enumerate() {
            for (k, on) in inside.iter_mut().zip(&p.on) {
                for o in on {
                    if let On::Edge { edge, t } = *o {
                        k.entry(edge).or_default().push((t, i));
                    }
                }
            }
        }
        for k in &mut inside {
            for cuts in k.values_mut() {
                cuts.sort_by(|p, q| p.0.total_cmp(&q.0).then(p.1.cmp(&q.1)));
            }
        }
        let [in_a, in_b] = inside;
        for side in Side::BOTH {
            let k = if side == Side::A { &in_a } else { &in_b };
            for edge in self.ops.of(side).edges() {
                let cuts = k.get(&edge.id()).map_or(&[][..], Vec::as_slice);
                let planned = self.plan(side, edge.geometry(), cuts)?;
                self.edges.insert((side, edge.id()), planned);
            }
        }
        Ok(())
    }

    /// The pieces of an edge of `side` with geometry `g` cut at `cuts`, with no draft
    /// edge yet for `b` (its id is a placeholder until `b_edges`); `a`'s are written.
    fn plan(
        &mut self,
        side: Side,
        g: &EdgeGeom,
        cuts: &[(f64, usize)],
    ) -> Result<Vec<Piece>, BooleanError> {
        let ends = [self.vertex(side, g.ends[0])?, self.vertex(side, g.ends[1])?];
        let EdgeShape::Curve { interval, .. } = g.shape else {
            return Ok(vec![Piece {
                edge: EdgeId::from_raw(u32::MAX),
                reversed: false,
                shared: false,
                t: None,
                ends,
            }]);
        };
        let mut out = Vec::with_capacity(cuts.len() + 1);
        let mut from = (interval.min(), ends[0]);
        for &(t, i) in cuts {
            let v = *self
                .at
                .get(i)
                .ok_or(BooleanError::Defect(BooleanDefect::Draft))?;
            if let Some(span) = Extent1d::new(from.0, t).filter(|s| s.length() > 0.0) {
                out.push(Piece {
                    edge: EdgeId::from_raw(u32::MAX),
                    reversed: false,
                    shared: false,
                    t: Some(span),
                    ends: [from.1, v],
                });
                from = (t, v);
            }
        }
        let last = Extent1d::new(from.0, interval.max())
            .ok_or(BooleanError::Defect(BooleanDefect::Draft))?;
        out.push(Piece {
            edge: EdgeId::from_raw(u32::MAX),
            reversed: false,
            shared: false,
            t: Some(last),
            ends: [from.1, ends[1]],
        });
        Ok(out)
    }

    /// Writes `a`'s pieces and copies `b`'s, after the shared pieces are known.
    fn write_edges(&mut self, side: Side, shared: &Shared) -> Result<(), BooleanError> {
        let one = Side::B.operand();
        let body = self.ops.of(side);
        for edge in body.edges() {
            let id = edge.id();
            let g = edge.geometry();
            let Some(planned) = self.edges.get(&(side, id)).cloned() else {
                continue;
            };
            let mut done = Vec::with_capacity(planned.len());
            for (k, mut p) in planned.into_iter().enumerate() {
                let shape = match (&g.shape, p.t) {
                    (EdgeShape::Curve { curve, .. }, Some(t)) => EdgeShape::Curve {
                        curve: curve.clone(),
                        interval: t,
                    },
                    _ => EdgeShape::Collapsed,
                };
                match side {
                    Side::A if k == 0 => {
                        p.edge = id;
                        if p.t.is_some() && p.t != interval_of(g) {
                            let e = self
                                .draft
                                .edge_mut(id)
                                .ok_or(BooleanError::Defect(BooleanDefect::Draft))?;
                            e.shape = shape;
                            e.ends = p.ends;
                            self.made.push(id);
                        }
                    }
                    Side::A => {
                        let mut e = EdgeGeom::new(shape, p.ends);
                        e.accuracy.clone_from(&g.accuracy);
                        p.edge = self.draft.add_edge(e).map_err(draft_defect)?;
                        self.draft
                            .derive(p.edge, Origin::Edge(id))
                            .map_err(draft_defect)?;
                        self.made.push(p.edge);
                    }
                    Side::B => {
                        if let Some(&(onto, reversed)) = shared.get(&(id, k)) {
                            p.edge = onto;
                            p.reversed = reversed;
                            p.shared = true;
                            self.draft.coincident(one, id, onto).map_err(draft_defect)?;
                        } else {
                            let mut e = EdgeGeom::new(shape, p.ends);
                            e.accuracy.clone_from(&g.accuracy);
                            p.edge = self.draft.add_edge(e).map_err(draft_defect)?;
                            self.draft
                                .derive(p.edge, Origin::Operand(one, Entity::Edge(id)))
                                .map_err(draft_defect)?;
                            self.made.push(p.edge);
                        }
                    }
                }
                done.push(p);
            }
            self.edges.insert((side, id), done);
        }
        Ok(())
    }

    /// The piece of `side`'s `edge` holding the parameter `t`, by index.
    fn piece_at(&self, side: Side, edge: EdgeId, t: f64) -> Result<(usize, Piece), BooleanError> {
        let pieces = self.edges.get(&(side, edge)).ok_or_else(|| {
            BooleanError::Defect(BooleanDefect::NoEdge {
                operand: side.operand(),
                edge,
            })
        })?;
        pieces
            .iter()
            .enumerate()
            .find(|(_, p)| p.t.is_none_or(|s| s.min() <= t && t <= s.max()))
            .map(|(k, p)| (k, *p))
            .ok_or_else(|| {
                BooleanError::Defect(BooleanDefect::Piece {
                    operand: side.operand(),
                    edge,
                })
            })
    }

    /// A point of `side`'s `edge` at `t`.
    fn edge_point(&self, side: Side, edge: EdgeId, t: f64) -> Option<Point3> {
        match self.ops.of(side).edge(edge)?.shape() {
            EdgeShape::Curve { curve, .. } => Some(curve.point(t)),
            EdgeShape::Collapsed => None,
        }
    }

    /// The sections as new edges, from both faces; and, before them, the operand edges'
    /// pieces in the draft, `b`'s sharing `a`'s along each lying piece `along` an edge.
    fn sections(&mut self, ff: &FaceFace) -> Result<(), BooleanError> {
        self.write_edges(Side::A, &BTreeMap::new())?;
        let shared = self.shared(ff)?;
        self.write_edges(Side::B, &shared)?;
        let one = Side::B.operand();
        for s in &ff.sections {
            let [fa, fb] = s.faces;
            let ends = if let Some([i, j]) = s.ends {
                [i, j].map(|k| self.at.get(k).copied())
            } else {
                let v = self
                    .draft
                    .add_vertex(VertexRecord::new(s.curve.point(s.t.min())))
                    .map_err(draft_defect)?;
                self.draft
                    .derive(v, Origin::Face(fa))
                    .map_err(draft_defect)?;
                self.draft
                    .derive(v, Origin::Operand(one, Entity::Face(fb)))
                    .map_err(draft_defect)?;
                [Some(v); 2]
            };
            let [Some(v0), Some(v1)] = ends else {
                return Err(BooleanError::Defect(BooleanDefect::Draft));
            };
            let e = self
                .draft
                .add_edge(EdgeGeom::new(
                    EdgeShape::Curve {
                        curve: s.curve.clone(),
                        interval: s.t,
                    },
                    [v0, v1],
                ))
                .map_err(draft_defect)?;
            self.draft
                .derive(e, Origin::Face(fa))
                .map_err(draft_defect)?;
            // A cut with no tool face (BO13b's curves on one face) has no second origin.
            if self.ops.b.face(fb).is_some() {
                self.draft
                    .derive(e, Origin::Operand(one, Entity::Face(fb)))
                    .map_err(draft_defect)?;
            }
            self.made.push(e);
            self.cuts.push(e);
            self.sections.push(e);
            self.cut.entry((Side::A, fa)).or_default().push(e);
            self.cut.entry((Side::B, fb)).or_default().push(e);
        }
        Ok(())
    }

    /// `b`'s pieces that run along `a`'s, by `b`'s edge and piece index: `a`'s draft edge,
    /// and whether it runs against `b`'s.
    fn shared(&self, ff: &FaceFace) -> Result<Shared, BooleanError> {
        let mut out = BTreeMap::new();
        for l in &ff.lying {
            let Some(along) = l.along else { continue };
            let (k, lp) = self.piece_at(l.side, l.edge, l.t.mid())?;
            let other = l.side.other();
            let mid = self.edge_point(l.side, l.edge, l.t.mid()).ok_or_else(|| {
                BooleanError::Defect(BooleanDefect::Piece {
                    operand: l.side.operand(),
                    edge: l.edge,
                })
            })?;
            // The other edge's piece between the same two vertices, nearest the middle.
            let pieces = self.edges.get(&(other, along)).ok_or_else(|| {
                BooleanError::Defect(BooleanDefect::NoEdge {
                    operand: other.operand(),
                    edge: along,
                })
            })?;
            let mut lens = lp.ends;
            lens.sort_unstable();
            let mut best: Option<(f64, usize, bool)> = None;
            for (j, q) in pieces.iter().enumerate() {
                let mut qe = q.ends;
                qe.sort_unstable();
                if qe != lens {
                    continue;
                }
                let Some(t) = q.t else { continue };
                let Some(qm) = self.edge_point(other, along, t.mid()) else {
                    continue;
                };
                let d = qm.distance(mid);
                let reversed = if lp.ends[0] == lp.ends[1] {
                    // A closed piece: the directions at the middles decide.
                    let (Some(da), Some(db)) = (
                        self.tangent(l.side, l.edge, l.t.mid()),
                        self.tangent(other, along, t.mid()),
                    ) else {
                        continue;
                    };
                    da.dot(db) < 0.0
                } else {
                    q.ends[0] != lp.ends[0]
                };
                if best.is_none_or(|(bd, _, _)| d < bd) {
                    best = Some((d, j, reversed));
                }
            }
            let Some((_, j, reversed)) = best else {
                return Err(BooleanError::Defect(BooleanDefect::Piece {
                    operand: l.side.operand(),
                    edge: l.edge,
                }));
            };
            let (key, onto) = match l.side {
                Side::A => ((along, j), lp.edge),
                Side::B => {
                    let q = pieces
                        .get(j)
                        .ok_or(BooleanError::Defect(BooleanDefect::Draft))?;
                    ((l.edge, k), q.edge)
                }
            };
            out.entry(key).or_insert((onto, reversed));
        }
        Ok(out)
    }

    /// The unit tangent of `side`'s `edge` at `t`.
    fn tangent(&self, side: Side, edge: EdgeId, t: f64) -> Option<Vec3> {
        match self.ops.of(side).edge(edge)?.shape() {
            EdgeShape::Curve { curve, .. } => {
                curve.tangent_dir(t, Approach::Above).map(UnitVec3::as_vec)
            }
            EdgeShape::Collapsed => None,
        }
    }

    /// The lying pieces inside a face, as cuts of that face.
    fn lying(&mut self, ff: &FaceFace) -> Result<(), BooleanError> {
        for l in &ff.lying {
            if l.along.is_some() {
                continue;
            }
            let (_, p) = self.piece_at(l.side, l.edge, l.t.mid())?;
            self.cuts.push(p.edge);
            self.cut
                .entry((l.side.other(), l.face))
                .or_default()
                .push(p.edge);
        }
        for c in self.cut.values_mut() {
            c.sort_unstable();
            c.dedup();
        }
        Ok(())
    }

    /// Records as an edge's accuracy how far its curve ends from a vertex, where that is
    /// beyond the two reaches.
    fn reach(&mut self) -> Result<(), BooleanError> {
        let zone = self.doc.resolution().zone().mm();
        let reach = |r: Option<&Recorded>| r.map_or(zone, |r| r.acc.length().mm().max(zone));
        let mut made = core::mem::take(&mut self.made);
        made.sort_unstable();
        made.dedup();
        for &e in &made {
            let Some(view) = self.draft.edge(e) else {
                continue;
            };
            let g = view.geometry();
            let EdgeShape::Curve { curve, interval } = &g.shape else {
                continue;
            };
            let re = reach(g.accuracy.as_deref());
            let mut need: f64 = 0.0;
            for (v, t) in g.ends.iter().zip([interval.min(), interval.max()]) {
                let Some(vv) = self.draft.vertex(*v) else {
                    continue;
                };
                let gap = curve.point(t).distance(vv.point());
                if gap > re + reach(vv.accuracy()) {
                    need = need.max(gap);
                }
            }
            if need > 0.0 {
                let g = self
                    .draft
                    .edge_mut(e)
                    .ok_or(BooleanError::Defect(BooleanDefect::Draft))?;
                g.accuracy = measured(need);
            }
        }
        self.made = made;
        Ok(())
    }

    /// A shell in the draft for each of `b`'s, in a region for each of `b`'s solid ones.
    fn shells(&mut self) -> Result<BTreeMap<ShellId, ShellId>, BooleanError> {
        let mut regions: BTreeMap<RegionId, RegionId> = BTreeMap::new();
        regions.insert(RegionId::OUTSIDE, RegionId::OUTSIDE);
        let mut out = BTreeMap::new();
        for s in self.ops.b.shells() {
            let r = s.record().region;
            let region = if let Some(&x) = regions.get(&r) {
                x
            } else {
                let solid = self.ops.b.regions().any(|g| g.id() == r && g.is_solid());
                let x = self.draft.add_region(solid).map_err(draft_defect)?;
                regions.insert(r, x);
                x
            };
            let id = self
                .draft
                .add_shell(ShellRecord::new(region))
                .map_err(draft_defect)?;
            out.insert(s.id(), id);
        }
        Ok(out)
    }

    /// The draft uses of an operand use: its edge's pieces in its sense.
    fn uses_of(
        &self,
        side: Side,
        edge: EdgeId,
        sense: Sense,
    ) -> Result<Vec<(Piece, Sense)>, BooleanError> {
        let pieces = self.edges.get(&(side, edge)).ok_or_else(|| {
            BooleanError::Defect(BooleanDefect::NoEdge {
                operand: side.operand(),
                edge,
            })
        })?;
        let mut out: Vec<(Piece, Sense)> = pieces
            .iter()
            .map(|p| (*p, if p.reversed { sense.reversed() } else { sense }))
            .collect();
        if sense == Sense::Reversed {
            out.reverse();
        }
        Ok(out)
    }

    /// Imprints `side`'s face `f`.
    fn face(
        &mut self,
        side: Side,
        f: FaceId,
        shells: &BTreeMap<ShellId, ShellId>,
        budget: &mut Budget,
        stop: &Stop,
    ) -> Result<(), BooleanError> {
        let operand = side.operand();
        let view = self
            .ops
            .of(side)
            .face(f)
            .ok_or(BooleanError::Defect(BooleanDefect::Draft))?;
        let rec = view.record();
        let sides = match side {
            Side::A => rec.sides,
            Side::B => {
                let map = |s: ShellId| {
                    shells
                        .get(&s)
                        .copied()
                        .ok_or(BooleanError::Defect(BooleanDefect::Draft))
                };
                [map(rec.sides[0])?, map(rec.sides[1])?]
            }
        };
        let cuts = self.cut.get(&(side, f)).cloned().unwrap_or_default();
        // The face's uses in the draft, per loop.
        let mut loops: Vec<Vec<(Piece, Sense, &EdgeUse)>> = Vec::with_capacity(rec.loops.len());
        let mut changed = side == Side::B;
        let mut shared = false;
        for l in &rec.loops {
            let mut out = Vec::with_capacity(l.uses.len());
            for u in &l.uses {
                let ps = self.uses_of(side, u.edge, u.sense)?;
                changed |= ps.len() > 1 || ps.iter().any(|(p, _)| p.edge != u.edge);
                shared |= ps.iter().any(|(p, _)| p.shared);
                out.extend(ps.into_iter().map(|(p, s)| (p, s, u)));
            }
            loops.push(out);
        }
        if cuts.is_empty() && !changed {
            self.pieces.insert((side, f), vec![f]);
            return Ok(());
        }
        budget.charge(1)?;
        let written: Vec<(Vec<LoopRecord>, bool)> = if cuts.is_empty() {
            let lr = if shared {
                let seq: Vec<Vec<(EdgeId, Sense)>> = loops
                    .iter()
                    .map(|l| l.iter().map(|(p, s, _)| (p.edge, *s)).collect())
                    .collect();
                self.draft
                    .exact_loops(view.surface(), &seq, &self.doc)
                    .map_err(|e| loops_error(e.kind(), operand, f))?
            } else {
                loops
                    .iter()
                    .map(|l| {
                        LoopRecord::new(l.iter().map(|(p, s, u)| restricted(p, *s, u)).collect())
                    })
                    .collect()
            };
            vec![(lr, true)]
        } else {
            let mut poles = BTreeMap::new();
            for l in &loops {
                for (p, s, _) in l {
                    if p.t.is_none() {
                        poles.insert(p.ends[0], (p.edge, *s));
                    }
                }
            }
            let boundary: Vec<(EdgeId, Sense)> = loops
                .iter()
                .flatten()
                .filter(|(p, _, _)| p.t.is_some())
                .map(|(p, s, _)| (p.edge, *s))
                .collect();
            self.trace(
                view.surface(),
                &boundary,
                &cuts,
                &poles,
                operand,
                f,
                budget,
                stop,
            )?
        };
        let surface = view.surface().clone();
        let mut ids = Vec::with_capacity(written.len());
        for (lr, keeps) in written {
            let id = if side == Side::A && keeps {
                self.draft
                    .edit_face(f, |r| r.loops = lr)
                    .ok_or(BooleanError::Defect(BooleanDefect::Draft))?;
                f
            } else {
                let id = self
                    .draft
                    .add_face(FaceRecord::new(surface.clone(), sides, lr))
                    .map_err(draft_defect)?;
                let from = match side {
                    Side::A => Origin::Face(f),
                    Side::B => Origin::Operand(operand, Entity::Face(f)),
                };
                self.draft.derive(id, from).map_err(draft_defect)?;
                id
            };
            ids.push(id);
        }
        ids.sort_unstable();
        self.pieces.insert((side, f), ids);
        Ok(())
    }

    /// The pieces of a face on `surface` whose boundary runs through `boundary`'s uses
    /// (the first the face's first) and whose cuts are `cuts`, each with whether it holds
    /// the first boundary use.
    #[expect(clippy::too_many_arguments, reason = "the face's read-only context")]
    fn trace(
        &self,
        surface: &Surface,
        boundary: &[(EdgeId, Sense)],
        cuts: &[EdgeId],
        poles: &BTreeMap<VertexId, (EdgeId, Sense)>,
        operand: scx_brep::Operand,
        f: FaceId,
        budget: &mut Budget,
        stop: &Stop,
    ) -> Result<Vec<(Vec<LoopRecord>, bool)>, BooleanError> {
        let trace_err = || BooleanError::Defect(BooleanDefect::Trace { operand, face: f });
        // Cuts that dangle (an end no other half-edge reaches) bound nothing: dropped.
        let mut live: Vec<EdgeId> = cuts
            .iter()
            .copied()
            .filter(|c| !boundary.iter().any(|(e, _)| e == c))
            .collect();
        loop {
            let mut degree: BTreeMap<VertexId, usize> = BTreeMap::new();
            for &(e, s) in boundary {
                let g = self.geometry(e).ok_or_else(trace_err)?;
                *degree.entry(g.start(s)).or_default() += 1;
                *degree.entry(g.end(s)).or_default() += 1;
            }
            for &c in &live {
                let g = self.geometry(c).ok_or_else(trace_err)?;
                *degree.entry(g.ends[0]).or_default() += 1;
                *degree.entry(g.ends[1]).or_default() += 1;
            }
            let before = live.len();
            live.retain(|&c| {
                self.geometry(c).is_some_and(|g| {
                    g.ends
                        .iter()
                        .all(|v| degree.get(v).copied().unwrap_or(0) > 1)
                })
            });
            if live.len() == before {
                break;
            }
        }
        let mut halves = Vec::with_capacity(boundary.len() + 2 * live.len());
        for &(e, s) in boundary {
            halves.push(self.half(e, s).ok_or_else(trace_err)?);
        }
        for &c in &live {
            for s in [Sense::Forward, Sense::Reversed] {
                halves.push(self.half(c, s).ok_or_else(trace_err)?);
            }
        }
        let mut out: BTreeMap<VertexId, Vec<usize>> = BTreeMap::new();
        for (i, h) in halves.iter().enumerate() {
            out.entry(h.from).or_default().push(i);
        }
        let mut normals: BTreeMap<VertexId, Vec3> = BTreeMap::new();
        for &v in out.keys() {
            let n = self.normal(surface, v, &halves, &out, budget, stop)?;
            normals.insert(v, n);
        }
        // The cycles.
        let mut used = vec![false; halves.len()];
        let mut cycles: Vec<Vec<usize>> = Vec::new();
        for start in 0..halves.len() {
            if used.get(start).copied().unwrap_or(true) {
                continue;
            }
            let mut cycle = Vec::new();
            let mut h = start;
            loop {
                match used.get_mut(h) {
                    Some(u) if !*u => *u = true,
                    Some(_) if h == start => break,
                    _ => return Err(trace_err()),
                }
                cycle.push(h);
                let at = halves.get(h).ok_or_else(trace_err)?;
                let cand = out.get(&at.to).ok_or_else(trace_err)?;
                let n = normals.get(&at.to).copied().ok_or_else(trace_err)?;
                h = next(&halves, h, cand, n).ok_or_else(trace_err)?;
                if cycle.len() > halves.len() {
                    return Err(trace_err());
                }
            }
            cycles.push(cycle);
        }
        // Each cycle in uv: its uses (a pole it passes gets the face's collapsed edge).
        let mut rings: Vec<Ring> = Vec::new();
        for cycle in &cycles {
            let mut uses = Vec::with_capacity(cycle.len() + 1);
            for &h in cycle {
                let x = halves.get(h).ok_or_else(trace_err)?;
                uses.push((x.edge, x.sense));
                if let Some(&pole) = poles.get(&x.to) {
                    uses.push(pole);
                }
            }
            let lr = self
                .draft
                .exact_loop(surface, &uses, &self.doc)
                .map_err(|e| loops_error(e.kind(), operand, f))?;
            let ring = ring(lr.record());
            let area = shoelace(&ring);
            rings.push((lr, ring, area));
        }
        // Pieces: the counter-clockwise cycles; each clockwise one is a hole of the
        // smallest piece around a point just left of it.
        let domain = surface.domain();
        let periods = [domain.u.period(), domain.v.period()];
        let mut holes: BTreeMap<usize, Vec<usize>> = BTreeMap::new();
        for (i, (_, ring, area)) in rings.iter().enumerate() {
            if *area > 0.0 {
                holes.entry(i).or_default();
                continue;
            }
            let q = left_of(ring).ok_or_else(trace_err)?;
            let around = rings
                .iter()
                .enumerate()
                .filter(|(_, (_, r, a))| *a > 0.0 && contains(r, q, periods))
                .min_by(|x, y| x.1.2.total_cmp(&y.1.2))
                .map(|(j, _)| j)
                .ok_or_else(trace_err)?;
            holes.entry(around).or_default().push(i);
        }
        // Each cycle is in one piece, as its outer loop or a hole: each loop is taken once.
        let mut loops: Vec<Option<ExactLoop>> = rings.into_iter().map(|r| Some(r.0)).collect();
        let mut take = |i: usize| {
            loops
                .get_mut(i)
                .and_then(Option::take)
                .ok_or_else(trace_err)
        };
        let mut pieces = Vec::with_capacity(holes.len());
        for (outer, inner) in holes {
            let keeps = outer == 0 || inner.contains(&0);
            // A piece with no hole is its cycle's loop as written.
            if inner.is_empty() {
                pieces.push((vec![take(outer)?.into_record()], keeps));
                continue;
            }
            // Each cycle was written alone above; the piece's loops are those, placed.
            let seq: Vec<ExactLoop> = core::iter::once(outer)
                .chain(inner)
                .map(&mut take)
                .collect::<Result<_, _>>()?;
            let lr = scx_brep::pcurve::placed(surface, seq)
                .map_err(|e| loops_error(e.kind(), operand, f))?;
            pieces.push((lr, keeps));
        }
        Ok(pieces)
    }

    fn geometry(&self, e: EdgeId) -> Option<&EdgeGeom> {
        self.draft.edge(e).map(scx_brep::Edge::geometry)
    }

    /// The half-edge of draft edge `e` in `sense`.
    fn half(&self, e: EdgeId, sense: Sense) -> Option<Half> {
        let g = self.geometry(e)?;
        let EdgeShape::Curve { curve, interval } = &g.shape else {
            return None;
        };
        let (s0, s1) = (interval.min(), interval.max());
        let step = interval.length() * CHORD;
        let dir = |t: f64, side: Approach| curve.tangent_dir(t, side).map(UnitVec3::as_vec);
        let (d0, d1) = (dir(s0, Approach::Above)?, dir(s1, Approach::Below)?);
        let (p0, p1) = (curve.point(s0), curve.point(s1));
        let (c0, c1) = (curve.point(s0 + step) - p0, curve.point(s1 - step) - p1);
        Some(match sense {
            Sense::Forward => Half {
                edge: e,
                sense,
                from: g.ends[0],
                to: g.ends[1],
                out: d0,
                out_chord: c0,
                back: -d1,
                back_chord: c1,
            },
            Sense::Reversed => Half {
                edge: e,
                sense,
                from: g.ends[1],
                to: g.ends[0],
                out: -d1,
                out_chord: c1,
                back: d0,
                back_chord: c0,
            },
        })
    }

    /// The surface's normal at vertex `v` (`S_u x S_v`); where it has none (a cone's
    /// apex), the normal a little along the first half-edge leaving `v`.
    fn normal(
        &self,
        surface: &Surface,
        v: VertexId,
        halves: &[Half],
        out: &BTreeMap<VertexId, Vec<usize>>,
        budget: &mut Budget,
        stop: &Stop,
    ) -> Result<Vec3, BooleanError> {
        let p = self
            .draft
            .vertex(v)
            .ok_or(BooleanError::Defect(BooleanDefect::Draft))?
            .point();
        let near = out
            .get(&v)
            .and_then(|o| o.first())
            .and_then(|&h| halves.get(h))
            .map(|h| p + h.out_chord);
        for q in core::iter::once(p).chain(near) {
            budget.charge(1)?;
            let uv = surface
                .invert(q, None, None, budget, stop)
                .map_err(|_| BooleanError::Defect(BooleanDefect::Draft))?
                .into_foot()
                .at;
            if let Some(n) = surface.normal(uv, [Approach::Above; 2]) {
                return Ok(n.as_vec());
            }
        }
        Err(BooleanError::Defect(BooleanDefect::Draft))
    }
}

/// A use of piece `p` in `sense` with the operand use `u`'s pcurve over the piece.
fn restricted(p: &Piece, sense: Sense, u: &EdgeUse) -> EdgeUse {
    let geom = match (&u.geom, p.t) {
        (UseGeom::Pcurve { curve, .. }, Some(t)) => UseGeom::Pcurve {
            curve: curve.clone(),
            interval: t,
        },
        (g, _) => g.clone(),
    };
    let mut out = EdgeUse::new(p.edge, sense, geom);
    out.bound.clone_from(&u.bound);
    out
}

/// The interval of a curve edge.
fn interval_of(g: &EdgeGeom) -> Option<Extent1d> {
    match g.shape {
        EdgeShape::Curve { interval, .. } => Some(interval),
        EdgeShape::Collapsed => None,
    }
}

/// The error the pcurve writer's refusal of kind `kind` is at `operand`'s face `face`.
pub(crate) const fn loops_error(
    kind: scx_num::error::ErrorKind,
    operand: scx_brep::Operand,
    face: FaceId,
) -> BooleanError {
    match kind {
        scx_num::error::ErrorKind::Unsupported => {
            BooleanError::Unsupported(BooleanUnsupported::Pcurve { operand, face })
        }
        _ => BooleanError::Defect(BooleanDefect::Loops { operand, face }),
    }
}

/// A half-edge: a draft edge in one sense, with its directions at both ends.
#[derive(Debug, Clone, Copy)]
struct Half {
    edge: EdgeId,
    sense: Sense,
    from: VertexId,
    to: VertexId,
    /// The unit direction leaving `from`.
    out: Vec3,
    /// From `from` to the point `CHORD` of the way along.
    out_chord: Vec3,
    /// The unit direction back along the half-edge at `to`.
    back: Vec3,
    /// From `to` to the point `CHORD` of the way back.
    back_chord: Vec3,
}

/// The clockwise angle about `n` from `r` to `x`, in `(0, 2 pi]`; `None` when either has
/// no direction across `n`.
fn clockwise(n: Vec3, r: Vec3, x: Vec3) -> Option<f64> {
    let flat = |v: Vec3| v - n * v.dot(n);
    let e1 = flat(r).normalized()?.as_vec();
    let e2 = n.cross(e1);
    let xf = flat(x);
    if xf.length() == 0.0 {
        return None;
    }
    let ccw = atan2(xf.dot(e2), xf.dot(e1));
    let ccw = if ccw < 0.0 { ccw + TAU } else { ccw };
    Some(if ccw <= 0.0 { TAU } else { TAU - ccw })
}

/// The half-edge after `h` among `cand` (leaving `h`'s end), about the normal `n` there:
/// the first clockwise from the way back, its twin only when it is the only one.
fn next(halves: &[Half], h: usize, cand: &[usize], n: Vec3) -> Option<usize> {
    let at = halves.get(h)?;
    let twin = |c: &Half| c.edge == at.edge && c.sense != at.sense;
    let mut best: Option<(usize, f64, f64)> = None;
    for &c in cand {
        let x = halves.get(c)?;
        if twin(x) {
            continue;
        }
        let tan = clockwise(n, at.back, x.out).unwrap_or(TAU);
        let chord = clockwise(n, at.back_chord, x.out_chord).unwrap_or(TAU);
        // Along the way back, the tangent decides nothing: the chord does.
        let tan = if tan < TIE || tan > TAU - TIE {
            chord
        } else {
            tan
        };
        let better = match best {
            None => true,
            Some((_, bt, bc)) if (tan - bt).abs() < TIE => chord < bc,
            Some((_, bt, _)) => tan < bt,
        };
        if better {
            best = Some((c, tan, chord));
        }
    }
    best.map(|(c, _, _)| c).or_else(|| {
        cand.iter()
            .copied()
            .find(|&c| halves.get(c).is_some_and(twin))
    })
}

/// A loop's samples in uv, in its own direction.
pub(crate) fn ring(lp: &LoopRecord) -> Vec<Point2> {
    let mut out = Vec::with_capacity(lp.uses.len() * SAMPLES);
    for u in &lp.uses {
        match &u.geom {
            UseGeom::Pcurve { curve, interval } => {
                for k in 0..SAMPLES {
                    let f = k as f64 / SAMPLES as f64;
                    let t = match u.sense {
                        Sense::Forward => interval.min() + f * interval.length(),
                        Sense::Reversed => interval.max() - f * interval.length(),
                    };
                    out.push(curve.point(t));
                }
            }
            UseGeom::Point(p) => out.push(*p),
        }
    }
    out
}

/// Twice the signed area of the closed polygon `ring`.
pub(crate) fn shoelace(ring: &[Point2]) -> f64 {
    let n = ring.len();
    (0..n)
        .filter_map(|i| Some((ring.get(i)?, ring.get((i + 1) % n)?)))
        .map(|(a, b)| a.x * b.y - b.x * a.y)
        .sum()
}

/// A point just left of the longest step of `ring`: inside the region the loop bounds on
/// its left.
fn left_of(ring: &[Point2]) -> Option<Point2> {
    let n = ring.len();
    let (a, b) = (0..n)
        .filter_map(|i| Some((*ring.get(i)?, *ring.get((i + 1) % n)?)))
        .max_by(|x, y| (x.1 - x.0).length().total_cmp(&(y.1 - y.0).length()))?;
    let d = b - a;
    let len = d.length();
    if len == 0.0 {
        return None;
    }
    let m = a + d * 0.5;
    Some(m + Vec2::new(-d.y, d.x) * 1e-4)
}

/// Whether `q`, or `q` moved by a whole period, lies inside the closed polygon `ring`.
fn contains(ring: &[Point2], q: Point2, periods: [Option<f64>; 2]) -> bool {
    let shifts = |p: Option<f64>| -> Vec<f64> { p.map_or_else(|| vec![0.0], |p| vec![0.0, -p, p]) };
    for du in shifts(periods[0]) {
        for dv in shifts(periods[1]) {
            if crossings(ring, Point2::new(q.x + du, q.y + dv)) {
                return true;
            }
        }
    }
    false
}

/// Even-odd: whether `q` is inside the closed polygon `ring`.
fn crossings(ring: &[Point2], q: Point2) -> bool {
    let n = ring.len();
    let mut inside = false;
    for i in 0..n {
        let (Some(a), Some(b)) = (ring.get(i), ring.get((i + 1) % n)) else {
            continue;
        };
        if (a.y > q.y) != (b.y > q.y) {
            let x = a.x + (q.y - a.y) / (b.y - a.y) * (b.x - a.x);
            if q.x < x {
                inside = !inside;
            }
        }
    }
    inside
}
