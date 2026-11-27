// Written for solidcorex. Apache-2.0.
//! The checks `finish` runs (01 §2.3), in order: every curve attached, [`references`]
//! (what the views' totality needs), [`structure`] (the body invariants), then the cheap
//! [`geometry`] check. Each walks records in kind order, then id order, and stops at the
//! first failure, so the cause is the same on every run.
//!
//! The whole body is checked. 01 §2.3 checks what the draft touched; that needs the
//! draft's events (BR3), and until then a whole-body check is the same answer at a cost
//! linear in the body.

mod geometry;

use std::collections::BTreeSet;

use scx_num::frame::Sense;
use scx_num::units::Resolution;

use crate::finish::{FinishCause, StructureCause};
use crate::id::{EdgeId, Entity, FaceId, RegionId, ShellId, VertexId};
use crate::record::{EdgeShape, FaceSide, Fin, UseGeom};
use crate::tables::Tables;

/// Every check, in order; the first failure is the cause.
pub(crate) fn run(
    t: &Tables,
    pending: &BTreeSet<EdgeId>,
    res: Resolution,
) -> Result<(), FinishCause> {
    if let Some(&edge) = pending.first() {
        return Err(FinishCause::MissingCurve { edge });
    }
    references(t).map_err(FinishCause::Structure)?;
    structure(t).map_err(FinishCause::Structure)?;
    geometry::check(t, res).map_err(FinishCause::Geometry)
}

/// The outside region is there and void, every id a record names is live, and every fin has
/// its use: what lets a finished body's views look across records without an `Option`.
/// Records are walked in kind order, then id order.
pub(crate) fn references(t: &Tables) -> Result<(), StructureCause> {
    if t.regions.get(RegionId::OUTSIDE).is_none_or(|r| r.solid) {
        return Err(StructureCause::Outside);
    }
    let dangling = |from: Entity, to: Entity| StructureCause::Dangling { from, to };
    for (id, shell) in t.shells.iter() {
        if t.regions.get(shell.region).is_none() {
            return Err(dangling(id.into(), shell.region.into()));
        }
        if let Some(&e) = shell.wires.iter().find(|&&e| t.edges.get(e).is_none()) {
            return Err(dangling(id.into(), e.into()));
        }
        if let Some(&v) = shell
            .lone_vertices
            .iter()
            .find(|&&v| t.vertices.get(v).is_none())
        {
            return Err(dangling(id.into(), v.into()));
        }
    }
    for (id, face) in t.faces.iter() {
        if let Some(&s) = face.sides.iter().find(|&&s| t.shells.get(s).is_none()) {
            return Err(dangling(id.into(), s.into()));
        }
        let uses = face.loops.iter().flat_map(|l| &l.uses);
        if let Some(u) = uses.clone().find(|u| t.edges.get(u.edge).is_none()) {
            return Err(dangling(id.into(), u.edge.into()));
        }
    }
    for (id, edge) in t.edges.iter() {
        if let Some(&v) = edge.ends.iter().find(|&&v| t.vertices.get(v).is_none()) {
            return Err(dangling(id.into(), v.into()));
        }
    }
    for (id, adj) in t.adjacency.iter() {
        for &fin in &adj.fins {
            let Some(face) = t.faces.get(fin.face) else {
                return Err(dangling(id.into(), fin.face.into()));
            };
            let mut uses = face.loops.iter().flat_map(|l| &l.uses);
            if !uses.any(|u| u.edge == id && u.sense == fin.sense) {
                return Err(StructureCause::FinWithoutUse { edge: id, fin });
            }
        }
    }
    Ok(())
}

/// The side of a face a fin's use faces next, turning about the edge by the right-hand
/// rule: a forward use has its face on the left of the edge seen from the front, so the
/// front comes next; a reversed use, the back.
const fn after(sense: Sense) -> FaceSide {
    match sense {
        Sense::Forward => FaceSide::Front,
        Sense::Reversed => FaceSide::Back,
    }
}

/// The body invariants, after [`references`] has made every id resolve. First what each
/// record must hold with its neighbours, in kind order (shells' wires, faces' loops and
/// uses, edges' fins, vertices), then what holds of shells and regions as wholes, which
/// relies on the sides the edge pass checked.
pub(crate) fn structure(t: &Tables) -> Result<(), StructureCause> {
    let mut wire = vec![false; t.edges.slots()];
    for (id, shell) in t.shells.iter() {
        for &e in &shell.wires {
            if t.adjacency.get(e).is_some_and(|a| !a.fins.is_empty()) {
                return Err(StructureCause::WireWithFins { shell: id, edge: e });
            }
            if let Some(w) = wire.get_mut(e.index()) {
                *w = true;
            }
        }
    }
    let mut ends = vec![false; t.vertices.slots()];
    for (id, face) in t.faces.iter() {
        faces(t, id, face)?;
    }
    for (id, edge) in t.edges.iter() {
        if edge.shape == EdgeShape::Collapsed && edge.ends[0] != edge.ends[1] {
            return Err(StructureCause::CollapsedEnds { edge: id });
        }
        for v in edge.ends {
            if let Some(e) = ends.get_mut(v.index()) {
                *e = true;
            }
        }
        let fins = t.adjacency.get(id).map_or(&[][..], |a| &a.fins);
        if fins.is_empty() && !wire.get(id.index()).copied().unwrap_or(false) {
            return Err(StructureCause::LooseEdge { edge: id });
        }
        for (i, &fin) in fins.iter().enumerate() {
            let next = fins.get(i + 1).copied().unwrap_or(fins[0]);
            let shell = |f: Fin, side: FaceSide| t.faces.get(f.face).map(|r| r.side(side));
            if shell(fin, after(fin.sense)) != shell(next, after(next.sense).other()) {
                return Err(StructureCause::SidesDisagree {
                    edge: id,
                    fins: [fin, next],
                });
            }
        }
    }
    for (_, shell) in t.shells.iter() {
        for &v in &shell.lone_vertices {
            if let Some(e) = ends.get_mut(v.index()) {
                *e = true;
            }
        }
    }
    if let Some((vertex, _)) = t
        .vertices
        .iter()
        .find(|(v, _)| !ends.get(v.index()).copied().unwrap_or(false))
    {
        return Err(StructureCause::LooseVertex { vertex });
    }
    shells(t)?;
    regions(t)
}

/// One face's loops and uses: loops present, non-empty and connected end to start; each
/// use's geometry of its edge's kind, over its interval, with exactly one fin; no edge used
/// twice in one sense; no vertex on two loops.
fn faces(t: &Tables, id: FaceId, face: &crate::record::FaceRecord) -> Result<(), StructureCause> {
    if face.loops.is_empty() {
        return Err(StructureCause::FaceWithoutLoop { face: id });
    }
    // (edge, sense) of every use, sorted, to find a repeat without a map.
    let mut seen: Vec<(EdgeId, bool)> = Vec::new();
    // (vertex, loop) of every use's start, for Face 39.
    let mut on: Vec<(VertexId, usize)> = Vec::new();
    for (lp, l) in face.loops.iter().enumerate() {
        let Some(last) = l.uses.last() else {
            return Err(StructureCause::EmptyLoop { face: id, lp });
        };
        let mut prev_end = t.edges.get(last.edge).map(|e| e.end(last.sense));
        let n = l.uses.len();
        for (i, u) in l.uses.iter().enumerate() {
            let Some(edge) = t.edges.get(u.edge) else {
                continue;
            };
            let start = edge.start(u.sense);
            if prev_end != Some(start) {
                // The previous use, cyclically, is the one whose end is wrong.
                return Err(StructureCause::LoopNotConnected {
                    face: id,
                    lp,
                    at: (i + n - 1) % n,
                });
            }
            prev_end = Some(edge.end(u.sense));
            on.push((start, lp));
            match (&edge.shape, &u.geom) {
                (EdgeShape::Curve { .. }, UseGeom::Point(_)) => {
                    return Err(StructureCause::PointUseOfCurve {
                        face: id,
                        edge: u.edge,
                    });
                }
                (EdgeShape::Curve { interval, .. }, UseGeom::Pcurve { interval: p, .. })
                    if p != interval =>
                {
                    return Err(StructureCause::PcurveInterval {
                        face: id,
                        edge: u.edge,
                        sense: u.sense,
                    });
                }
                _ => {}
            }
            seen.push((u.edge, u.sense == Sense::Forward));
        }
    }
    seen.sort_unstable();
    if let Some(w) = seen.windows(2).find(|w| w[0] == w[1]) {
        let (edge, forward) = w[0];
        let sense = if forward {
            Sense::Forward
        } else {
            Sense::Reversed
        };
        return Err(StructureCause::RepeatedUse {
            face: id,
            edge,
            sense,
        });
    }
    for u in face.loops.iter().flat_map(|l| &l.uses) {
        let fin = Fin::new(id, u.sense);
        let fins = t.adjacency.get(u.edge).map_or(&[][..], |a| &a.fins);
        match fins.iter().filter(|&&f| f == fin).count() {
            0 => {
                return Err(StructureCause::UseWithoutFin {
                    face: id,
                    edge: u.edge,
                    sense: u.sense,
                });
            }
            1 => {}
            _ => return Err(StructureCause::RepeatedFin { edge: u.edge, fin }),
        }
    }
    on.sort_unstable();
    on.dedup();
    if let Some(w) = on.windows(2).find(|w| w[0].0 == w[1].0) {
        return Err(StructureCause::VertexOnTwoLoops {
            face: id,
            vertex: w[0].0,
        });
    }
    Ok(())
}

/// Disjoint sets over slots, with path halving.
struct Sets(Vec<u32>);

impl Sets {
    fn new(n: usize) -> Self {
        Self((0..n).map(|i| u32::try_from(i).unwrap_or(u32::MAX)).collect())
    }

    fn root(&mut self, mut i: usize) -> usize {
        while let Some(&p) = self.0.get(i) {
            let p = p as usize;
            if p == i {
                break;
            }
            let gp = self.0.get(p).copied().unwrap_or(u32::MAX);
            if let Some(slot) = self.0.get_mut(i) {
                *slot = gp;
            }
            i = gp as usize;
        }
        i
    }

    fn join(&mut self, a: usize, b: usize) {
        let (ra, rb) = (self.root(a), self.root(b));
        let (lo, hi) = (ra.min(rb), ra.max(rb));
        if let Some(slot) = self.0.get_mut(hi) {
            *slot = u32::try_from(lo).unwrap_or(u32::MAX);
        }
    }
}

/// Every shell bounds something and is one piece. The pieces are face sides, wires and
/// lone vertices; two face sides are joined across an edge where they meet radially (the
/// sides [`structure`] checked), and any two pieces of one shell at a common vertex.
fn shells(t: &Tables) -> Result<(), StructureCause> {
    let nf = t.faces.slots() * 2;
    let ne = t.edges.slots();
    let side_slot = |f: FaceId, side: FaceSide| f.index() * 2 + side as usize;
    let wire_slot = |e: EdgeId| nf + e.index();
    let lone_slot = |v: VertexId| nf + ne + v.index();
    let mut sets = Sets::new(nf + ne + t.vertices.slots());
    for (id, adj) in t.adjacency.iter() {
        let _ = id;
        for (i, &fin) in adj.fins.iter().enumerate() {
            let next = adj.fins.get(i + 1).copied().unwrap_or(adj.fins[0]);
            sets.join(
                side_slot(fin.face, after(fin.sense)),
                side_slot(next.face, after(next.sense).other()),
            );
        }
    }
    // (shell, vertex, piece) for every piece at a vertex; pieces of one shell at one
    // vertex are joined.
    let mut at: Vec<(ShellId, VertexId, usize)> = Vec::new();
    for (id, face) in t.faces.iter() {
        for side in FaceSide::BOTH {
            let shell = face.side(side);
            for u in face.loops.iter().flat_map(|l| &l.uses) {
                if let Some(e) = t.edges.get(u.edge) {
                    at.push((shell, e.start(u.sense), side_slot(id, side)));
                }
            }
        }
    }
    for (id, shell) in t.shells.iter() {
        for &e in &shell.wires {
            if let Some(edge) = t.edges.get(e) {
                for v in edge.ends {
                    at.push((id, v, wire_slot(e)));
                }
            }
        }
        for &v in &shell.lone_vertices {
            at.push((id, v, lone_slot(v)));
        }
    }
    at.sort_unstable();
    for w in at.windows(2) {
        if (w[0].0, w[0].1) == (w[1].0, w[1].1) {
            sets.join(w[0].2, w[1].2);
        }
    }
    // The root of each shell's first piece; another root is a second piece.
    let mut root: Vec<Option<usize>> = vec![None; t.shells.slots()];
    let mut split: Option<ShellId> = None;
    let mut piece = |shell: ShellId, slot: usize, sets: &mut Sets| {
        let r = sets.root(slot);
        if let Some(seen) = root.get_mut(shell.index()) {
            match *seen {
                None => *seen = Some(r),
                Some(s) if s != r => split = Some(split.map_or(shell, |x| x.min(shell))),
                Some(_) => {}
            }
        }
    };
    for (id, face) in t.faces.iter() {
        for side in FaceSide::BOTH {
            piece(face.side(side), side_slot(id, side), &mut sets);
        }
    }
    for (id, shell) in t.shells.iter() {
        for &e in &shell.wires {
            piece(id, wire_slot(e), &mut sets);
        }
        for &v in &shell.lone_vertices {
            piece(id, lone_slot(v), &mut sets);
        }
    }
    if let Some((shell, _)) = t
        .shells
        .iter()
        .find(|(s, _)| root.get(s.index()).is_none_or(Option::is_none))
    {
        return Err(StructureCause::EmptyShell { shell });
    }
    match split {
        Some(shell) => Err(StructureCause::ShellNotConnected { shell }),
        None => Ok(()),
    }
}

/// Every region but the outside has a shell, and so does the outside once any region has.
fn regions(t: &Tables) -> Result<(), StructureCause> {
    let mut has = vec![false; t.regions.slots()];
    for (_, shell) in t.shells.iter() {
        if let Some(h) = has.get_mut(shell.region.index()) {
            *h = true;
        }
    }
    let any = has.iter().any(|&h| h);
    let bare = t.regions.iter().find(|(id, _)| {
        let h = has.get(id.index()).copied().unwrap_or(false);
        !h && (*id != RegionId::OUTSIDE || any)
    });
    match bare {
        Some((region, _)) => Err(StructureCause::RegionWithoutShell { region }),
        None => Ok(()),
    }
}
