// Written for solidcorex. Apache-2.0.
//! [`stitch`]: the sheets gathered into `sheets[0].edit()`, their vertices identified.

use std::collections::BTreeMap;
use std::sync::Arc;

use scx_bodyq::tight_box_charged;
use scx_brep::op::{Cx, Done, Raise, Report, Warning, WarningKind};
use scx_brep::{
    Brep, Draft, EdgeId, EdgeShape, Entity, FaceId, Operand, Origin, RegionId, ShellId, UseGeom,
    VertexId,
};
use scx_num::budget::{Budget, Stop};
use scx_num::cert::{Basis, Recorded};
use scx_num::extent3d::Extent3d;
use scx_num::units::{Accuracy, Length};
use scx_num::vector3d::Point3;

use crate::error::{StitchDefect, StitchError, StitchUnsupported, draft_defect, too_large};
use crate::points::{Group, Item, identify};
use crate::report::{Identified, StitchReport, Stitched, Want};

/// Work charged per entity copied or end measured.
const UNIT: u64 = 1;

/// The sheets stitched into one body (74 §4.3).
///
/// Sheet `i` is operand `i` of the result's lineage. Sheet 0 is edited in place, so its
/// entities keep their ids; every other sheet's entities are copied in, each derived from
/// its operand's entity. Any finished bodies are taken, solids among them.
///
/// **Vertices** are identified by 04's merged-points row (see [`crate`]): vertices of
/// different sheets within coincidence are one vertex of the result, recorded as a
/// [`Coincident`](WarningKind::Coincident) warning on it and in
/// [`StitchReport::identified`]; the merged point is one of their own points, chosen by
/// geometry alone. Its accuracy is raised, as `Measured`, to the farthest edge or pcurve
/// end the merge moves (and the spread), where that is beyond its reach; each raise is a
/// [`Raise`] with an [`AccuracyRaised`](WarningKind::AccuracyRaised) warning.
///
/// **Edges** are not joined yet: where an edge of one sheet and an edge or vertex of
/// another come within coincidence anywhere but at identified ends, the answer is
/// [`StitchUnsupported::Edges`], so no result leaves a join undone silently. Sheets that
/// meet only at vertices, or not at all, stitch whole.
///
/// # Errors
///
/// - [`StitchRefusal::OtherDocument`](crate::StitchRefusal::OtherDocument) for a sheet
///   finished under another resolution than `cx.doc`'s.
/// - [`StitchRefusal::VerticesWithinCoincidence`](crate::StitchRefusal::VerticesWithinCoincidence)
///   for two vertices of one sheet within coincidence of each other.
/// - [`StitchUnsupported::Chain`] for a chain anchoring does not settle;
///   [`StitchUnsupported::Edges`] as above; [`StitchUnsupported::Solid`] for
///   [`Want::Solid`]; [`StitchUnsupported::TooLarge`] past the ids.
/// - [`StitchError::Exhausted`] when `cx`'s budget runs out, [`StitchError::Cancelled`]
///   when `cx.stop` is set, and [`StitchError::Defect`] for the method's own failure.
pub fn stitch(sheets: &[&Brep], want: Want, cx: &mut Cx<'_>) -> Result<Stitched, StitchError> {
    let res = cx.doc.resolution();
    let n = u32::try_from(sheets.len())
        .map_err(|_| StitchError::Unsupported(StitchUnsupported::TooLarge))?;
    for (i, s) in (0..n).zip(sheets) {
        if s.resolution() != res {
            return Err(StitchError::Refused(crate::StitchRefusal::OtherDocument {
                operand: Operand::from_raw(i),
            }));
        }
    }
    if want == Want::Solid {
        return Err(StitchError::Unsupported(StitchUnsupported::Solid));
    }
    let mut budget = cx.budget();
    let Some(first) = sheets.first() else {
        let (body, changes) = Draft::new()
            .finish(cx.doc)
            .map_err(|u| StitchError::Defect(StitchDefect::Finish(Box::new(u.cause))))?;
        return Ok(Stitched {
            done: Done::new(body, changes, Report::empty(budget.used())),
            report: StitchReport {
                identified: Vec::new(),
            },
        });
    };
    let zone = res.zone().mm();
    let reach = |r: Option<&Recorded>| r.map_or(zone, |r| r.acc.length().mm().max(zone));
    let mut items = Vec::new();
    let mut start = Vec::with_capacity(sheets.len());
    for (i, s) in (0..n).zip(sheets) {
        start.push(items.len());
        items.extend(s.vertices().map(|v| Item {
            operand: Operand::from_raw(i),
            vertex: v.id(),
            point: v.point(),
            reach: reach(v.accuracy()),
        }));
    }
    start.push(items.len());
    let groups = identify(&items, &mut budget, cx.stop)?;
    let ix = Index {
        items: &items,
        start: &start,
        group_of: group_of(items.len(), &groups),
    };
    edges_pending(sheets, &ix, cx, &mut budget)?;
    let mut b = Build {
        sheets,
        ix: &ix,
        groups: &groups,
        draft: first.edit(),
        at: vec![None; items.len()],
        group_at: vec![None; groups.len()],
    };
    b.vertices()?;
    let (warnings, raised, identified) = b.accuracy(zone, &mut budget, cx.stop)?;
    b.copy(&mut budget, cx.stop)?;
    let (body, changes) = b
        .draft
        .finish(cx.doc)
        .map_err(|u| StitchError::Defect(StitchDefect::Finish(Box::new(u.cause))))?;
    Ok(Stitched {
        done: Done::new(
            body,
            changes,
            Report::new(warnings, Vec::new(), raised, budget.used()),
        ),
        report: StitchReport { identified },
    })
}

/// The items, where each sheet's start, and each item's group.
struct Index<'a> {
    items: &'a [Item],
    /// Sheet `i`'s items are `start[i]..start[i + 1]`, in id order.
    start: &'a [usize],
    group_of: Vec<Option<usize>>,
}

impl Index<'_> {
    /// The item of sheet `i`'s vertex `v`.
    fn item(&self, i: usize, v: VertexId) -> Option<usize> {
        let (lo, hi) = (*self.start.get(i)?, *self.start.get(i + 1)?);
        let slice = self.items.get(lo..hi)?;
        slice
            .binary_search_by_key(&v, |it| it.vertex)
            .ok()
            .map(|k| lo + k)
    }

    /// The group of sheet `i`'s vertex `v`, if it has one.
    fn group(&self, i: usize, v: VertexId) -> Option<usize> {
        self.item(i, v)
            .and_then(|k| self.group_of.get(k).copied().flatten())
    }
}

fn group_of(n: usize, groups: &[Group]) -> Vec<Option<usize>> {
    let mut out = vec![None; n];
    for (g, group) in groups.iter().enumerate() {
        for &m in &group.members {
            if let Some(slot) = out.get_mut(m) {
                *slot = Some(g);
            }
        }
    }
    out
}

/// Refuses, as [`StitchUnsupported::Edges`], sheets whose edges the edge stage would join or
/// split: an edge of one sheet with both ends identified with an edge of another's, or a
/// vertex of another sheet inside the edge's certified box (grown by the vertex's reach
/// beyond the edge's) that is not identified with one of its ends. A box is conservative:
/// an edge that only passes near a vertex is refused too, until the edge stage measures the
/// gap.
fn edges_pending(
    sheets: &[&Brep],
    ix: &Index<'_>,
    cx: &mut Cx<'_>,
    budget: &mut Budget,
) -> Result<(), StitchError> {
    let pending = |operand: Operand, edge: EdgeId| {
        Err(StitchError::Unsupported(StitchUnsupported::Edges {
            operand,
            edge,
        }))
    };
    let mut ends: BTreeMap<[usize; 2], (Operand, EdgeId)> = BTreeMap::new();
    for (i, s) in sheets.iter().enumerate() {
        let operand = Operand::from_raw(i as u32);
        for e in s.edges() {
            budget.charge(UNIT)?;
            let [a, b] = e.ends().map(|v| ix.group(i, v));
            if let (Some(a), Some(b)) = (a, b) {
                let key = [a.min(b), a.max(b)];
                match ends.get(&key) {
                    Some(&(o, first)) if o != operand => return pending(o, first),
                    Some(_) => {}
                    None => {
                        ends.insert(key, (operand, e.id()));
                    }
                }
            }
        }
    }
    let axes = Axes::new(ix.items);
    let zone = cx.doc.resolution().zone().mm();
    for (i, s) in sheets.iter().enumerate() {
        let operand = Operand::from_raw(i as u32);
        cx.stop.check()?;
        for e in s.edges() {
            let own = e.ends().map(|v| ix.group(i, v));
            let near = |bx: &Extent3d, budget: &mut Budget| -> Result<bool, StitchError> {
                for k in axes.slab(bx) {
                    budget.charge(UNIT)?;
                    let Some(it) = ix.items.get(k) else {
                        continue;
                    };
                    if it.operand == operand {
                        continue;
                    }
                    let g = ix.group_of.get(k).copied().flatten();
                    if g.is_some() && own.contains(&g) {
                        continue;
                    }
                    // The box is grown by the edge's accuracy and the zone, so it holds every
                    // point within the edge's reach; a vertex reaching farther grows it by
                    // the rest.
                    if bx.contains_point(it.point, (it.reach - zone).max(0.0)) {
                        return Ok(true);
                    }
                }
                Ok(false)
            };
            let at = Entity::Edge(e.id());
            let loose = e
                .bounding_box()
                .ok_or(StitchError::Defect(StitchDefect::Query { operand, at }))?;
            if !near(&loose, budget)? {
                continue;
            }
            let tight = tight_box_charged(s, at, cx.scratch, budget, cx.stop)
                .map_err(|err| StitchError::of_query(err, operand, at))?;
            if near(&tight, budget)? {
                return pending(operand, e.id());
            }
        }
    }
    Ok(())
}

/// The items sorted along each axis, for box queries: a box reads the slab of the axis
/// that holds the fewest items.
struct Axes {
    /// Per axis, `(coordinate, item)` in coordinate then item order.
    sorted: [Vec<(f64, usize)>; 3],
    /// The largest reach: a slab is grown by it.
    grow: f64,
}

impl Axes {
    fn new(items: &[Item]) -> Self {
        let along = |c: fn(Point3) -> f64| {
            let mut v: Vec<(f64, usize)> = items
                .iter()
                .enumerate()
                .map(|(k, it)| (c(it.point), k))
                .collect();
            v.sort_unstable_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)));
            v
        };
        Self {
            sorted: [along(|p| p.x), along(|p| p.y), along(|p| p.z)],
            grow: items.iter().map(|it| it.reach).fold(0.0, f64::max),
        }
    }

    /// The items whose coordinate on the thinnest axis lies in the box's span there, grown
    /// by the largest reach.
    fn slab(&self, bx: &Extent3d) -> impl Iterator<Item = usize> + '_ {
        let spans = [
            (bx.x_min(), bx.x_max()),
            (bx.y_min(), bx.y_max()),
            (bx.z_min(), bx.z_max()),
        ];
        let ranges = self.sorted.iter().zip(spans).map(|(v, (lo, hi))| {
            let (lo, hi) = (lo - self.grow, hi + self.grow);
            let a = v.partition_point(|&(c, _)| c < lo);
            let b = v.partition_point(|&(c, _)| c <= hi);
            (v, a, b.max(a))
        });
        let best = ranges.min_by_key(|&(_, a, b)| b - a);
        best.into_iter()
            .flat_map(|(v, a, b)| v.get(a..b).unwrap_or_default().iter().map(|&(_, k)| k))
    }
}

/// The stitched draft as it is built.
struct Build<'a> {
    sheets: &'a [&'a Brep],
    ix: &'a Index<'a>,
    groups: &'a [Group],
    draft: Draft,
    /// Each item's draft vertex, once made.
    at: Vec<Option<VertexId>>,
    /// Each group's draft vertex, once made.
    group_at: Vec<Option<VertexId>>,
}

/// What [`Build::accuracy`] gives: the warnings, the raises and the report's entries.
type Accounts = (Vec<Warning>, Vec<Raise>, Vec<Identified>);

/// A `Measured` record of `mm`.
fn measured(mm: f64) -> Option<Arc<Recorded>> {
    Accuracy::new(Length::from_mm(mm)).map(|acc| {
        Arc::new(Recorded {
            acc,
            basis: Basis::Measured,
        })
    })
}

impl Build<'_> {
    fn item(&self, k: usize) -> Result<&Item, StitchError> {
        self.ix
            .items
            .get(k)
            .ok_or(StitchError::Defect(StitchDefect::Draft))
    }

    fn sheet(&self, i: usize) -> Result<&Brep, StitchError> {
        self.sheets
            .get(i)
            .copied()
            .ok_or(StitchError::Defect(StitchDefect::Draft))
    }

    /// The kept record of `group`, from its sheet.
    fn kept_record(&self, group: &Group) -> Result<scx_brep::VertexRecord, StitchError> {
        let kept = self.item(group.kept)?;
        Ok(self
            .sheet(kept.operand.index())?
            .vertex(kept.vertex)
            .ok_or(StitchError::Defect(StitchDefect::Draft))?
            .record()
            .clone())
    }

    /// Every vertex in the draft: sheet 0's kept (a group's record written over its own
    /// where another member is kept), a group's first copy where sheet 0 has none, and a
    /// copy of each other vertex.
    fn vertices(&mut self) -> Result<(), StitchError> {
        let groups = self.groups;
        for (g, group) in groups.iter().enumerate() {
            let Some(&m0) = group.members.first() else {
                continue;
            };
            let first = self.item(m0)?;
            if first.operand.index() != 0 {
                continue;
            }
            let v0 = first.vertex;
            if group.kept != m0 {
                let rec = self.kept_record(group)?;
                *self
                    .draft
                    .vertex_mut(v0)
                    .ok_or(StitchError::Defect(StitchDefect::Draft))? = rec;
            }
            if let Some(slot) = self.group_at.get_mut(g) {
                *slot = Some(v0);
            }
        }
        for k in 0..self.ix.items.len() {
            let it = *self.item(k)?;
            if it.operand.index() == 0 {
                if let Some(slot) = self.at.get_mut(k) {
                    *slot = Some(it.vertex);
                }
                continue;
            }
            let origin = Origin::Operand(it.operand, Entity::Vertex(it.vertex));
            let v = if let Some(g) = self.ix.group_of.get(k).copied().flatten() {
                self.member(g, it, origin)?
            } else {
                let rec = self
                    .sheet(it.operand.index())?
                    .vertex(it.vertex)
                    .ok_or(StitchError::Defect(StitchDefect::Draft))?
                    .record()
                    .clone();
                let v = self.draft.add_vertex(rec).map_err(too_large)?;
                self.draft.derive(v, origin).map_err(draft_defect)?;
                v
            };
            if let Some(slot) = self.at.get_mut(k) {
                *slot = Some(v);
            }
        }
        Ok(())
    }

    /// The draft vertex of `it`, a member of group `g` from sheet 1 on: sheet 0's member
    /// (with `it` coincident onto it), or the group's first copy (with `it` derived into it).
    fn member(&mut self, g: usize, it: Item, origin: Origin) -> Result<VertexId, StitchError> {
        let group = self
            .groups
            .get(g)
            .ok_or(StitchError::Defect(StitchDefect::Draft))?;
        match self.group_at.get(g).copied().flatten() {
            Some(v) if self.has_sheet0(group)? => {
                self.draft
                    .coincident(it.operand, it.vertex, v)
                    .map_err(draft_defect)?;
                Ok(v)
            }
            Some(v) => {
                self.draft.derive(v, origin).map_err(draft_defect)?;
                Ok(v)
            }
            None => {
                let rec = self.kept_record(group)?;
                let v = self.draft.add_vertex(rec).map_err(too_large)?;
                self.draft.derive(v, origin).map_err(draft_defect)?;
                if let Some(slot) = self.group_at.get_mut(g) {
                    *slot = Some(v);
                }
                Ok(v)
            }
        }
    }

    fn has_sheet0(&self, group: &Group) -> Result<bool, StitchError> {
        let Some(&m0) = group.members.first() else {
            return Ok(false);
        };
        Ok(self.item(m0)?.operand.index() == 0)
    }

    /// Each group's warnings, raise and report entry: its accuracy grown, as `Measured`,
    /// to the farthest edge or pcurve end the merge moves and to its spread, where that is
    /// beyond its reach.
    fn accuracy(
        &mut self,
        zone: f64,
        budget: &mut Budget,
        stop: &Stop,
    ) -> Result<Accounts, StitchError> {
        let reach = |r: Option<&Recorded>| r.map_or(zone, |r| r.acc.length().mm().max(zone));
        let (mut warnings, mut raised, mut identified) = (Vec::new(), Vec::new(), Vec::new());
        let groups = self.groups;
        for (g, group) in groups.iter().enumerate() {
            stop.check()?;
            let v = self
                .group_at
                .get(g)
                .copied()
                .flatten()
                .ok_or(StitchError::Defect(StitchDefect::Draft))?;
            let kept = *self.item(group.kept)?;
            let p = kept.point;
            let mut spread: f64 = 0.0;
            let mut need: f64 = 0.0;
            let mut members = Vec::with_capacity(group.members.len());
            for &m in &group.members {
                let it = *self.item(m)?;
                members.push((it.operand, it.vertex));
                spread = spread.max(it.point.distance(p));
                need = need.max(self.moved(it, p, budget)?);
            }
            need = need.max(spread);
            let before = self
                .draft
                .vertex(v)
                .ok_or(StitchError::Defect(StitchDefect::Draft))?
                .accuracy()
                .copied();
            let entity = Entity::Vertex(v);
            if need > reach(before.as_ref()) {
                let after = measured(need);
                let (Some(a), b) = (after.as_ref().map(|r| r.acc), before.as_ref()) else {
                    return Err(StitchError::Defect(StitchDefect::Draft));
                };
                raised.push(Raise {
                    entity,
                    before: b.map_or(Accuracy::ZERO, |r| r.acc),
                    after: a,
                });
                warnings.push(Warning {
                    entity,
                    kind: WarningKind::AccuracyRaised,
                });
                self.draft
                    .vertex_mut(v)
                    .ok_or(StitchError::Defect(StitchDefect::Draft))?
                    .accuracy = after;
            }
            warnings.push(Warning {
                entity,
                kind: WarningKind::Coincident,
            });
            identified.push(Identified {
                vertex: v,
                members,
                kept: (kept.operand, kept.vertex),
                spread: Length::from_mm(spread),
            });
        }
        identified.sort_by_key(|i| i.vertex);
        Ok((warnings, raised, identified))
    }

    /// How far from `p` the merge moves the ends of `it`'s edges and pcurves: each edge
    /// curve's end at it, and each use's surface point there.
    fn moved(&self, it: Item, p: Point3, budget: &mut Budget) -> Result<f64, StitchError> {
        let sheet = self.sheet(it.operand.index())?;
        let vertex = sheet
            .vertex(it.vertex)
            .ok_or(StitchError::Defect(StitchDefect::Draft))?;
        let mut far: f64 = 0.0;
        for e in vertex.edges() {
            let g = e.geometry();
            // A pcurve runs over the edge's interval min to max, whatever its use's sense:
            // its min end is at `ends[0]`, its max end at `ends[1]`.
            let at = [g.ends[0] == it.vertex, g.ends[1] == it.vertex];
            if let EdgeShape::Curve { curve, interval } = &g.shape {
                for (hit, t) in at.into_iter().zip([interval.min(), interval.max()]) {
                    if hit {
                        far = far.max(curve.point(t).distance(p));
                    }
                }
            }
            for u in e.uses() {
                budget.charge(UNIT)?;
                let surface = u.face().surface();
                match u.geom() {
                    UseGeom::Point(uv) => {
                        if at.contains(&true) {
                            far = far.max(surface.point(*uv).distance(p));
                        }
                    }
                    UseGeom::Pcurve { curve, interval } => {
                        for (hit, t) in at.into_iter().zip([interval.min(), interval.max()]) {
                            if hit {
                                far = far.max(surface.point(curve.point(t)).distance(p));
                            }
                        }
                    }
                }
            }
        }
        Ok(far)
    }

    /// Every other sheet's regions, edges, shells and faces, copied in that order with its
    /// ids mapped; each edge, face and vertex derived from its operand's.
    fn copy(&mut self, budget: &mut Budget, stop: &Stop) -> Result<(), StitchError> {
        let sheets = self.sheets;
        for (i, s) in sheets.iter().enumerate().skip(1) {
            stop.check()?;
            let operand = Operand::from_raw(i as u32);
            let mut regions: BTreeMap<RegionId, RegionId> = BTreeMap::new();
            regions.insert(RegionId::OUTSIDE, RegionId::OUTSIDE);
            for r in s.regions().filter(|r| !r.is_outside()) {
                let x = self.draft.add_region(r.is_solid()).map_err(too_large)?;
                regions.insert(r.id(), x);
            }
            let vertex = |v: VertexId| -> Result<VertexId, StitchError> {
                self.ix
                    .item(i, v)
                    .and_then(|k| self.at.get(k).copied().flatten())
                    .ok_or(StitchError::Defect(StitchDefect::Draft))
            };
            let mut ends = Vec::with_capacity(s.edges().len());
            for e in s.edges() {
                ends.push(e.ends().map(vertex));
            }
            let mut edges: BTreeMap<EdgeId, EdgeId> = BTreeMap::new();
            for (e, [a, b]) in s.edges().zip(ends) {
                budget.charge(UNIT)?;
                let mut g = e.geometry().clone();
                g.ends = [a?, b?];
                let x = self.draft.add_edge(g).map_err(too_large)?;
                self.draft
                    .derive(x, Origin::Operand(operand, Entity::Edge(e.id())))
                    .map_err(draft_defect)?;
                edges.insert(e.id(), x);
            }
            let edge = |e: EdgeId| {
                edges
                    .get(&e)
                    .copied()
                    .ok_or(StitchError::Defect(StitchDefect::Draft))
            };
            let mut shells: BTreeMap<ShellId, ShellId> = BTreeMap::new();
            for sh in s.shells() {
                let mut rec = sh.record().clone();
                rec.region = regions
                    .get(&rec.region)
                    .copied()
                    .ok_or(StitchError::Defect(StitchDefect::Draft))?;
                rec.wires = rec
                    .wires
                    .iter()
                    .map(|&e| edge(e))
                    .collect::<Result<_, _>>()?;
                rec.lone_vertices = rec
                    .lone_vertices
                    .iter()
                    .map(|&v| vertex(v))
                    .collect::<Result<_, _>>()?;
                let x = self.draft.add_shell(rec).map_err(too_large)?;
                shells.insert(sh.id(), x);
            }
            let mut faces: BTreeMap<FaceId, FaceId> = BTreeMap::new();
            for f in s.faces() {
                budget.charge(UNIT)?;
                let mut rec = f.record().clone();
                for side in &mut rec.sides {
                    *side = shells
                        .get(side)
                        .copied()
                        .ok_or(StitchError::Defect(StitchDefect::Draft))?;
                }
                for lp in &mut rec.loops {
                    for u in &mut lp.uses {
                        u.edge = edge(u.edge)?;
                    }
                }
                let x = self.draft.add_face(rec).map_err(too_large)?;
                self.draft
                    .derive(x, Origin::Operand(operand, Entity::Face(f.id())))
                    .map_err(draft_defect)?;
                faces.insert(f.id(), x);
            }
            // `add_face` appends fins in face order; an edge of three or more fins takes
            // its sheet's radial order back.
            for e in s.edges().filter(|e| e.fins().len() > 2) {
                let fins = e
                    .fins()
                    .map(|mut fin| {
                        fin.face = faces
                            .get(&fin.face)
                            .copied()
                            .ok_or(StitchError::Defect(StitchDefect::Draft))?;
                        Ok(fin)
                    })
                    .collect::<Result<Vec<_>, StitchError>>()?;
                self.draft
                    .adjacency_mut(edge(e.id())?)
                    .ok_or(StitchError::Defect(StitchDefect::Draft))?
                    .fins = fins;
            }
        }
        Ok(())
    }
}
