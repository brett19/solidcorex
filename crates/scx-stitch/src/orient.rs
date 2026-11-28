// Written for solidcorex. Apache-2.0.
//! The orientation stage (74 §4.3 step 5, ST3): for [`Want::Solid`](crate::Want::Solid), the
//! stitched sheets made the boundary of solids.
//!
//! In this model a face's orientation is its sides record (front, back), not a flag on the
//! surface: the front is the side the surface's natural normal points into. A sheet's
//! faces have one shell on both sides, so orienting a closed sheet is choosing, for each
//! face, which side the material is on, and giving that side a new shell of a solid
//! region. No surface is flipped.
//!
//! 1. **Closed.** Every curve edge of a sheet shell has two fins (a collapsed edge, a pole,
//!    has one and bounds nothing). An open edge is [`SolidFault::Open`], or
//!    [`StitchUnsupported::Undecided`] when a budget share left it undecided; a wire or a
//!    lone vertex is refused too.
//! 2. **Parity.** Each connected set of faces (a unit) is labelled from its least face:
//!    across an edge, two uses in opposite senses put the material on the same side of
//!    both faces, two in one sense on opposite sides. A contradiction is
//!    [`SolidFault::NotOrientable`], naming the edge.
//! 3. **Outward.** A trial body gives each unit its own solid region on the side parity
//!    chose from the least face's back, and MA4's certified [`volume_sign`] of the region's
//!    shell decides: negative means the material is on the other side.
//! 4. **Nesting.** A unit whose box holds another's may hold it: a point of the inner
//!    unit is classified ([`classify_point`]) against a body of the outer unit alone, made
//!    solid on its outward side. Each unit's parent is its deepest container. Units at
//!    even depth bound solids; at odd depth, cavities (a void region, its outer side a
//!    shell of the parent's solid). Near a NURBS face this waits on step 5's ray
//!    ([`NestingPending::Nurbs`]).
//!
//! The trial body is the answer when no sign is negative and nothing nests, so the common
//! case finishes once; otherwise the stitched draft is labelled again and finished.

use std::collections::{BTreeMap, BTreeSet, VecDeque};

use scx_algo::box_tree::BoxTree;
use scx_bodyq::{BodyqError, BodyqUnsupported, PointClass, classify_point};
use scx_brep::{
    Brep, Changes, Document, Draft, EdgeId, EdgeShape, Entity, FaceId, Operand, RegionId, ShellId,
    ShellRecord, VertexId,
};
use scx_mass::{MassError, MassUnsupported, Sign, volume_sign};
use scx_num::budget::{Budget, Stop};
use scx_num::extent3d::Extent3d;
use scx_num::vector3d::Axis;

use crate::error::{
    NestingPending, SignPending, SolidFault, StitchDefect, StitchError, StitchRefusal,
    StitchUnsupported, too_large,
};
use crate::report::Left;
use crate::stitch::Build;

/// A connected set of faces of one sheet shell, closed and labelled by parity.
struct Unit {
    /// The sheet shell its faces had.
    shell: ShellId,
    /// Each face, ascending, with whether parity puts the material in front of it.
    faces: Vec<(FaceId, bool)>,
}

/// How each unit is labelled: the material flipped from parity's first choice, and the
/// unit it lies directly inside.
struct Plan {
    flipped: Vec<bool>,
    parent: Vec<Option<usize>>,
    depth: Vec<usize>,
}

impl Plan {
    fn trivial(n: usize) -> Self {
        Self {
            flipped: vec![false; n],
            parent: vec![None; n],
            depth: vec![0; n],
        }
    }

    fn is_trivial(&self) -> bool {
        self.flipped.iter().all(|f| !f) && self.parent.iter().all(Option::is_none)
    }
}

/// A finish failure on a draft the stitch labelled: a bug.
fn finish_defect(u: Box<scx_brep::Unfinished>) -> StitchError {
    StitchError::Defect(StitchDefect::Finish(Box::new(u.cause)))
}

fn lost() -> StitchError {
    StitchError::Defect(StitchDefect::Draft)
}

impl Build<'_> {
    /// The stitched draft as solids: checked closed, labelled by parity, made outward by
    /// the certified volume sign, and nested. `left` is the edge stage's report.
    pub(crate) fn solid(
        &mut self,
        left: &[Left],
        doc: &Document,
        budget: &mut Budget,
        stop: &Stop,
    ) -> Result<(Brep, Changes), StitchError> {
        let units = self.units(left)?;
        let mut trial = self.draft.clone();
        let made = label(&mut trial, &units, &Plan::trivial(units.len()))?;
        let (body, changes) = trial.finish(doc).map_err(finish_defect)?;
        let plan = self.plan(&body, &units, &made, doc, budget, stop)?;
        if plan.is_trivial() {
            return Ok((body, changes));
        }
        let mut draft = std::mem::replace(&mut self.draft, Draft::new());
        label(&mut draft, &units, &plan)?;
        draft.finish(doc).map_err(finish_defect)
    }

    /// The units: each sheet shell's connected face sets, checked closed and labelled by
    /// parity.
    fn units(&self, left: &[Left]) -> Result<Vec<Unit>, StitchError> {
        let d = &self.draft;
        let mut by_shell: BTreeMap<ShellId, Vec<FaceId>> = BTreeMap::new();
        for f in d.faces() {
            let [front, back] = f.record().sides;
            if front != back {
                // The solid operands are refused before the stitch starts.
                return Err(lost());
            }
            by_shell.entry(front).or_default().push(f.id());
        }
        for s in d.shells() {
            let rec = s.record();
            if let Some(&e) = rec.wires.first() {
                let (operand, edge) = self.name_edge(e);
                return Err(not_solid(operand, Entity::Edge(edge), SolidFault::Wire));
            }
            if let Some(&v) = rec.lone_vertices.first() {
                let (operand, vertex) = self.name_vertex(v);
                return Err(not_solid(
                    operand,
                    Entity::Vertex(vertex),
                    SolidFault::LoneVertex,
                ));
            }
        }
        let mut units = Vec::new();
        for (shell, faces) in by_shell {
            let mut label: BTreeMap<FaceId, bool> = BTreeMap::new();
            for &seed in &faces {
                if label.contains_key(&seed) {
                    continue;
                }
                label.insert(seed, false);
                let mut unit = vec![seed];
                let mut queue = VecDeque::from([seed]);
                while let Some(f) = queue.pop_front() {
                    let mine = label.get(&f).copied().ok_or_else(lost)?;
                    let rec = d.face(f).ok_or_else(lost)?.record();
                    let mut edges: Vec<EdgeId> = rec
                        .loops
                        .iter()
                        .flat_map(|l| &l.uses)
                        .map(|u| u.edge)
                        .collect();
                    edges.sort_unstable();
                    edges.dedup();
                    for e in edges {
                        let edge = d.edge(e).ok_or_else(lost)?;
                        if matches!(edge.shape(), Some(EdgeShape::Collapsed)) {
                            continue;
                        }
                        let fins: Vec<_> = edge.fins().collect();
                        let [a, b] = fins.as_slice() else {
                            return Err(self.open(e, left));
                        };
                        // Opposite senses: the material on the same side of both faces.
                        let flip = a.sense == b.sense;
                        if a.face == b.face {
                            // A seam: one face on both sides of the edge.
                            if flip {
                                return Err(self.not_orientable(e));
                            }
                            continue;
                        }
                        let other = if a.face == f { b.face } else { a.face };
                        let want = mine ^ flip;
                        match label.get(&other) {
                            Some(&has) if has != want => return Err(self.not_orientable(e)),
                            Some(_) => {}
                            None => {
                                label.insert(other, want);
                                unit.push(other);
                                queue.push_back(other);
                            }
                        }
                    }
                }
                unit.sort_unstable();
                let faces = unit
                    .into_iter()
                    .map(|f| Ok((f, !label.get(&f).copied().ok_or_else(lost)?)))
                    .collect::<Result<_, StitchError>>()?;
                units.push(Unit { shell, faces });
            }
        }
        Ok(units)
    }

    /// The open edge `e`: undecided when the edge stage left a test of it open, refused
    /// otherwise.
    fn open(&self, e: EdgeId, left: &[Left]) -> StitchError {
        let (operand, edge) = self.name_edge(e);
        let undecided = left.iter().any(|l| match l {
            Left::Undecided { edge: x, with, .. } => *x == e || *with == Entity::Edge(e),
            Left::Ambiguous { .. } => false,
        });
        if undecided {
            StitchError::Unsupported(StitchUnsupported::Undecided { operand, edge })
        } else {
            not_solid(operand, Entity::Edge(edge), SolidFault::Open)
        }
    }

    fn not_orientable(&self, e: EdgeId) -> StitchError {
        let (operand, edge) = self.name_edge(e);
        not_solid(operand, Entity::Edge(edge), SolidFault::NotOrientable)
    }

    /// The labelling the trial body's signs and nesting give.
    fn plan(
        &self,
        body: &Brep,
        units: &[Unit],
        made: &[(RegionId, ShellId)],
        doc: &Document,
        budget: &mut Budget,
        stop: &Stop,
    ) -> Result<Plan, StitchError> {
        let mut plan = Plan::trivial(units.len());
        for ((u, &(_, shell)), flipped) in units.iter().zip(made).zip(&mut plan.flipped) {
            stop.check()?;
            *flipped = match volume_sign(body, shell, budget, stop) {
                Ok(Sign::Positive) => false,
                Ok(Sign::Negative) => true,
                // Every face of a closed unit has its new shell on one side only.
                Ok(Sign::Zero) => return Err(lost()),
                Err(e) => return Err(self.sign_error(e, u)),
            };
        }
        if units.len() < 2 {
            return Ok(plan);
        }
        // Candidate pairs: one unit's box inside another's.
        let mut boxes = Vec::with_capacity(units.len());
        for u in units {
            let mut bx: Option<Extent3d> = None;
            for &(f, _) in &u.faces {
                let fb = body
                    .face(f)
                    .and_then(|f| f.bounding_box())
                    .ok_or_else(|| self.face_query(f))?;
                bx = Some(bx.map_or(fb, |b| b.union(fb)));
            }
            boxes.push(bx.ok_or_else(lost)?);
        }
        let mut holds: BTreeMap<usize, Vec<usize>> = BTreeMap::new();
        for (i, j) in BoxTree::new(&boxes).self_pairs(0.0) {
            budget.charge(1)?;
            let (Some(bi), Some(bj)) = (boxes.get(i), boxes.get(j)) else {
                return Err(lost());
            };
            if inside(bj, bi) {
                holds.entry(i).or_default().push(j);
            }
            if inside(bi, bj) {
                holds.entry(j).or_default().push(i);
            }
        }
        let mut within: Vec<Vec<usize>> = vec![Vec::new(); units.len()];
        for (&outer, inner) in &holds {
            stop.check()?;
            let (alone, region) = self.alone(body, units, outer, &plan, doc)?;
            for &k in inner {
                if self.held(&alone, region, body, units, k, budget, stop)? {
                    within.get_mut(k).ok_or_else(lost)?.push(outer);
                }
            }
        }
        // A unit's containers nest (closed surfaces that do not cross), so the parent is
        // the container held by the most others.
        for (k, outers) in within.iter().enumerate() {
            let depth = outers.len();
            let parent = outers
                .iter()
                .copied()
                .find(|&o| within.get(o).is_some_and(|w| w.len() + 1 == depth));
            if depth > 0 && parent.is_none() {
                let (operand, face) = self.first_face(units, k)?;
                return Err(StitchError::Unsupported(StitchUnsupported::Nesting {
                    operand,
                    at: Entity::Face(face),
                    why: NestingPending::Touching,
                }));
            }
            *plan.depth.get_mut(k).ok_or_else(lost)? = depth;
            *plan.parent.get_mut(k).ok_or_else(lost)? = parent;
        }
        Ok(plan)
    }

    /// The trial body's unit `k` alone, its material on its outward side: a body for
    /// nesting, and its solid region.
    fn alone(
        &self,
        body: &Brep,
        units: &[Unit],
        k: usize,
        plan: &Plan,
        doc: &Document,
    ) -> Result<(Brep, RegionId), StitchError> {
        let u = units.get(k).ok_or_else(lost)?;
        let flip = plan.flipped.get(k).copied().ok_or_else(lost)?;
        let mut d = Draft::new();
        let region = d.add_region(true).map_err(too_large)?;
        let inner = d.add_shell(ShellRecord::new(region)).map_err(too_large)?;
        let outer = d
            .add_shell(ShellRecord::new(RegionId::OUTSIDE))
            .map_err(too_large)?;
        let mut vertices: BTreeMap<VertexId, VertexId> = BTreeMap::new();
        let mut edges: BTreeMap<EdgeId, EdgeId> = BTreeMap::new();
        for &(f, front) in &u.faces {
            let mut rec = body.face(f).ok_or_else(lost)?.record().clone();
            for use_ in rec.loops.iter_mut().flat_map(|l| l.uses.iter_mut()) {
                let e = use_.edge;
                if let Some(&x) = edges.get(&e) {
                    use_.edge = x;
                    continue;
                }
                let mut g = body.edge(e).ok_or_else(lost)?.geometry().clone();
                for end in &mut g.ends {
                    let v = *end;
                    *end = match vertices.get(&v) {
                        Some(&x) => x,
                        None => {
                            let rec = body.vertex(v).ok_or_else(lost)?.record().clone();
                            let x = d.add_vertex(rec).map_err(too_large)?;
                            vertices.insert(v, x);
                            x
                        }
                    };
                }
                let x = d.add_edge(g).map_err(too_large)?;
                edges.insert(e, x);
                use_.edge = x;
            }
            rec.sides = if front ^ flip {
                [inner, outer]
            } else {
                [outer, inner]
            };
            d.add_face(rec).map_err(too_large)?;
        }
        let (alone, _) = d.finish(doc).map_err(finish_defect)?;
        Ok((alone, region))
    }

    /// Whether unit `k` lies inside unit `outer`, by a vertex of `k` classified against
    /// `alone` (unit `outer` alone, `region` its solid). A vertex on `outer` gives way to
    /// the next.
    fn held(
        &self,
        alone: &Brep,
        region: RegionId,
        body: &Brep,
        units: &[Unit],
        k: usize,
        budget: &mut Budget,
        stop: &Stop,
    ) -> Result<bool, StitchError> {
        let u = units.get(k).ok_or_else(lost)?;
        let mut seen: BTreeSet<VertexId> = BTreeSet::new();
        for &(f, _) in &u.faces {
            let face = body.face(f).ok_or_else(lost)?;
            for use_ in face.record().loops.iter().flat_map(|l| &l.uses) {
                let edge = body.edge(use_.edge).ok_or_else(lost)?;
                for v in edge.ends() {
                    if !seen.insert(v) {
                        continue;
                    }
                    let p = body.vertex(v).ok_or_else(lost)?.point();
                    match classify_point(alone, p, budget, stop) {
                        Ok(PointClass::In(r)) => return Ok(r == region),
                        Ok(PointClass::On(_)) => {}
                        Ok(_) => {
                            let (operand, face) = self.first_face(units, k)?;
                            return Err(StitchError::Unsupported(StitchUnsupported::Nesting {
                                operand,
                                at: Entity::Face(face),
                                why: NestingPending::Unclear,
                            }));
                        }
                        Err(e) => return Err(self.nesting_error(e, units, k, v)),
                    }
                }
            }
        }
        let (operand, face) = self.first_face(units, k)?;
        Err(StitchError::Unsupported(StitchUnsupported::Nesting {
            operand,
            at: Entity::Face(face),
            why: NestingPending::Touching,
        }))
    }

    /// The sheet and sheet edge a draft edge stands for (a piece names the edge it was
    /// cut from).
    fn name_edge(&self, e: EdgeId) -> (Operand, EdgeId) {
        self.edge_name
            .get(&e)
            .copied()
            .unwrap_or((Operand::from_raw(0), e))
    }

    /// The sheet and sheet face a draft face stands for.
    fn name_face(&self, f: FaceId) -> (Operand, FaceId) {
        self.face_name
            .get(&f)
            .copied()
            .unwrap_or((Operand::from_raw(0), f))
    }

    /// The first sheet vertex a draft vertex stands for.
    fn name_vertex(&self, v: VertexId) -> (Operand, VertexId) {
        self.ix
            .items
            .iter()
            .zip(&self.at)
            .find(|(_, at)| **at == Some(v))
            .map_or((Operand::from_raw(0), v), |(it, _)| (it.operand, it.vertex))
    }

    fn first_face(&self, units: &[Unit], k: usize) -> Result<(Operand, FaceId), StitchError> {
        let &(f, _) = units
            .get(k)
            .and_then(|u| u.faces.first())
            .ok_or_else(lost)?;
        Ok(self.name_face(f))
    }

    fn face_query(&self, f: FaceId) -> StitchError {
        let (operand, face) = self.name_face(f);
        StitchError::Defect(StitchDefect::Query {
            operand,
            at: Entity::Face(face),
        })
    }

    /// A volume sign error on unit `u`, named by the sheet's face.
    fn sign_error(&self, e: MassError, u: &Unit) -> StitchError {
        let first = u.faces.first().map(|&(f, _)| f);
        let (face, why) = match e {
            MassError::Exhausted { limit } => return StitchError::Exhausted { limit },
            MassError::Cancelled => return StitchError::Cancelled,
            MassError::Unsupported(MassUnsupported::SignUndecided { .. }) => {
                (first, SignPending::Undecided)
            }
            MassError::Unsupported(MassUnsupported::NoSeries { face }) => {
                (Some(face), SignPending::NoSeries)
            }
            MassError::Unsupported(
                MassUnsupported::VWrap { face } | MassUnsupported::NetWinding { face },
            ) => (Some(face), SignPending::Winding),
            MassError::Unsupported(MassUnsupported::NoBox { at }) => (
                match at {
                    Entity::Face(f) => Some(f),
                    _ => first,
                },
                SignPending::NoBox,
            ),
            _ => {
                return match first {
                    Some(f) => self.face_query(f),
                    None => lost(),
                };
            }
        };
        match face {
            Some(f) => {
                let (operand, face) = self.name_face(f);
                StitchError::Unsupported(StitchUnsupported::Sign { operand, face, why })
            }
            None => lost(),
        }
    }

    /// A classification error for a vertex `v` of unit `k`.
    fn nesting_error(&self, e: BodyqError, units: &[Unit], k: usize, v: VertexId) -> StitchError {
        let why = match e {
            BodyqError::Exhausted { limit } => return StitchError::Exhausted { limit },
            BodyqError::Cancelled => return StitchError::Cancelled,
            BodyqError::Unsupported(BodyqUnsupported::Nurbs { .. }) => NestingPending::Nurbs,
            BodyqError::Unsupported(_) => NestingPending::Unclear,
            _ => {
                let (operand, vertex) = self.name_vertex(v);
                return StitchError::Defect(StitchDefect::Query {
                    operand,
                    at: Entity::Vertex(vertex),
                });
            }
        };
        match self.first_face(units, k) {
            Ok((operand, face)) => StitchError::Unsupported(StitchUnsupported::Nesting {
                operand,
                at: Entity::Face(face),
                why,
            }),
            Err(e) => e,
        }
    }
}

/// `a` within `b` on every axis.
fn inside(a: &Extent3d, b: &Extent3d) -> bool {
    Axis::ALL
        .iter()
        .all(|&x| a.min_at(x) >= b.min_at(x) && a.max_at(x) <= b.max_at(x))
}

fn not_solid(operand: Operand, at: Entity, why: SolidFault) -> StitchError {
    StitchError::Refused(StitchRefusal::NotSolid { operand, at, why })
}

/// Labels `d`'s units by `plan`: for each unit in order, a new region (solid at even depth,
/// a void at odd) and a new shell of it on the material side; the other side keeps the
/// sheet shell for the unit holding its least face and a new shell otherwise, in the
/// region of the parent's inside (the outside at depth 0). Gives each unit's region and
/// inside shell.
fn label(
    d: &mut Draft,
    units: &[Unit],
    plan: &Plan,
) -> Result<Vec<(RegionId, ShellId)>, StitchError> {
    let mut made: Vec<(RegionId, ShellId)> = Vec::with_capacity(units.len());
    let mut outside: Vec<ShellId> = Vec::with_capacity(units.len());
    let mut kept: BTreeSet<ShellId> = BTreeSet::new();
    for (k, u) in units.iter().enumerate() {
        let depth = plan.depth.get(k).copied().ok_or_else(lost)?;
        let region = d.add_region(depth % 2 == 0).map_err(too_large)?;
        let inner = d.add_shell(ShellRecord::new(region)).map_err(too_large)?;
        let outer = if kept.insert(u.shell) {
            u.shell
        } else {
            d.add_shell(ShellRecord::new(RegionId::OUTSIDE))
                .map_err(too_large)?
        };
        made.push((region, inner));
        outside.push(outer);
    }
    for (k, u) in units.iter().enumerate() {
        let flip = plan.flipped.get(k).copied().ok_or_else(lost)?;
        let (_, inner) = made.get(k).copied().ok_or_else(lost)?;
        let outer = outside.get(k).copied().ok_or_else(lost)?;
        let around = match plan.parent.get(k).copied().ok_or_else(lost)? {
            Some(p) => made.get(p).ok_or_else(lost)?.0,
            None => RegionId::OUTSIDE,
        };
        d.shell_mut(outer).ok_or_else(lost)?.region = around;
        for &(f, front) in &u.faces {
            d.face_mut(f).ok_or_else(lost)?.sides = if front ^ flip {
                [inner, outer]
            } else {
                [outer, inner]
            };
        }
    }
    Ok(made)
}
