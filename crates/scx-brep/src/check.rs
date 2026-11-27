// Written for solidcorex. Apache-2.0.
//! The checks `finish` runs (01 §2.3), in order: every curve attached, [`references`]
//! (what the views' totality needs), [`structure`] (the body invariants), then the cheap
//! [`geometry`] check. Each walks records in kind order, then id order, and stops at the
//! first failure, so the cause is the same on every run.
//!
//! A draft made from a finished body checks only the records its edit could have changed
//! ([`run_touched`], research 63 §6.14): every rule is a pass over records of one kind, each
//! record's instance reading a few others, and the body it was made from passed every
//! instance. An instance none of whose inputs changed passes again, so walking the changed
//! instances in the same order finds the same first failure. [`Touched`] says which they
//! are.

mod geometry;
mod radial;

use std::collections::BTreeSet;

use scx_num::frame::Sense;
use scx_num::units::Resolution;

use crate::finish::{FinishCause, StructureCause};
use crate::id::{EdgeId, Entity, FaceId, RegionId, ShellId, VertexId};
use crate::record::{
    EdgeAdj, EdgeGeom, EdgeShape, FaceRecord, FaceSide, Fin, ShellRecord, UseGeom,
};
use crate::table::{Id, Table};
use crate::tables::Tables;
use crate::touched::Touched;

/// The records of one kind a pass walks: every live one, or the live ones of a sorted list.
#[derive(Debug, Clone, Copy)]
pub(crate) enum Sel<'a, I> {
    All,
    Only(&'a [I]),
}

/// `f` on each selected live record, in id order, up to the first failure.
fn each<I: Id, T, E>(
    table: &Table<I, T>,
    sel: Sel<'_, I>,
    mut f: impl FnMut(I, &T) -> Result<(), E>,
) -> Result<(), E> {
    match sel {
        Sel::All => {
            for (id, r) in table.iter() {
                f(id, r)?;
            }
        }
        Sel::Only(ids) => {
            for &id in ids {
                if let Some(r) = table.get(id) {
                    f(id, r)?;
                }
            }
        }
    }
    Ok(())
}

/// Every check over the whole body, in order; the first failure is the cause.
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
    geometry::check(t, res, Sel::All, Sel::All).map_err(FinishCause::Geometry)?;
    radial::check(t, res, Sel::All).map_err(FinishCause::Geometry)
}

/// Every check over the instances `touched` names, in the order [`run`] takes them, with
/// the same first failure. On success, the faces whose geometry check ran: those whose
/// boxes may differ from the body the draft was made from.
pub(crate) fn run_touched(
    t: &Tables,
    pending: &BTreeSet<EdgeId>,
    res: Resolution,
    touched: &Touched,
) -> Result<Vec<FaceId>, FinishCause> {
    if let Some(&edge) = pending.first() {
        return Err(FinishCause::MissingCurve { edge });
    }
    let plan = touched.plan(t);
    let some_shells = plan.shells.as_deref().map_or(Sel::All, Sel::Only);
    let s = |e| FinishCause::Structure(e);
    outside(t).map_err(s)?;
    each(&t.shells, some_shells, |id, r| shell_refs(t, id, r)).map_err(s)?;
    each(&t.faces, Sel::Only(&plan.faces), |id, r| {
        face_refs(t, id, r)
    })
    .map_err(s)?;
    each(&t.edges, Sel::Only(&plan.edges), |id, r| {
        edge_refs(t, id, r)
    })
    .map_err(s)?;
    // The uses of every face a selected fin names, sorted as `references` sorts them all.
    let mut named: Vec<FaceId> = plan
        .edges
        .iter()
        .filter_map(|&e| t.adjacency.get(e))
        .flat_map(|a| a.fins.iter().map(|f| f.face))
        .collect();
    named.sort_unstable();
    named.dedup();
    let uses = use_keys(
        named
            .iter()
            .filter_map(|&f| Some((f, t.faces.get(f)?.as_ref()))),
    );
    each(&t.adjacency, Sel::Only(&plan.edges), |id, a| {
        fin_refs(t, id, a, &uses)
    })
    .map_err(s)?;
    each(&t.shells, some_shells, |id, r| shell_wires(t, id, r)).map_err(s)?;
    each(&t.faces, Sel::Only(&plan.faces), |id, r| faces(t, id, r)).map_err(s)?;
    let mut wires: Option<Vec<EdgeId>> = None;
    each(&t.edges, Sel::Only(&plan.edges), |id, r| {
        edge_structure(t, id, r, |e| {
            let w = wires.get_or_insert_with(|| {
                let mut w: Vec<EdgeId> = t
                    .shells
                    .iter()
                    .flat_map(|(_, s)| s.wires.iter().copied())
                    .collect();
                w.sort_unstable();
                w
            });
            w.binary_search(&e).is_ok()
        })
    })
    .map_err(s)?;
    if let Some(vertex) = touched.first_loose(t, &plan) {
        return Err(s(StructureCause::LooseVertex { vertex }));
    }
    if !touched.shells_unchanged(t) {
        shells(t).map_err(s)?;
    }
    regions(t).map_err(s)?;
    let (edges, faces) = touched.geometry(t, &plan);
    geometry::check(t, res, Sel::Only(&edges), Sel::Only(&faces)).map_err(FinishCause::Geometry)?;
    // The radial order reads an edge's curve and fins and its faces' surfaces and uses:
    // a write to any of them marks the edge (research 63 §6.15).
    radial::check(t, res, Sel::Only(&plan.edges)).map_err(FinishCause::Geometry)?;
    Ok(faces)
}

/// The outside region is there and void.
fn outside(t: &Tables) -> Result<(), StructureCause> {
    if t.regions.get(RegionId::OUTSIDE).is_none_or(|r| r.solid) {
        return Err(StructureCause::Outside);
    }
    Ok(())
}

fn dangling(from: Entity, to: Entity) -> StructureCause {
    StructureCause::Dangling { from, to }
}

/// The shell's region, wires and lone vertices are live.
fn shell_refs(t: &Tables, id: ShellId, shell: &ShellRecord) -> Result<(), StructureCause> {
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
    Ok(())
}

/// The face's sides and its uses' edges are live.
fn face_refs(t: &Tables, id: FaceId, face: &FaceRecord) -> Result<(), StructureCause> {
    if let Some(&s) = face.sides.iter().find(|&&s| t.shells.get(s).is_none()) {
        return Err(dangling(id.into(), s.into()));
    }
    let mut uses = face.loops.iter().flat_map(|l| &l.uses);
    if let Some(u) = uses.find(|u| t.edges.get(u.edge).is_none()) {
        return Err(dangling(id.into(), u.edge.into()));
    }
    Ok(())
}

/// The edge's end vertices are live.
fn edge_refs(t: &Tables, id: EdgeId, edge: &EdgeGeom) -> Result<(), StructureCause> {
    if let Some(&v) = edge.ends.iter().find(|&&v| t.vertices.get(v).is_none()) {
        return Err(dangling(id.into(), v.into()));
    }
    Ok(())
}

/// Every use of the faces given as `(face, edge, forward)`, sorted, so each fin finds its
/// use by a binary search: linear in the faces' uses up to a log, where scanning the face's
/// uses for each fin is quadratic in a long loop's length.
fn use_keys<'a>(
    faces: impl Iterator<Item = (FaceId, &'a FaceRecord)>,
) -> Vec<(FaceId, EdgeId, bool)> {
    let mut uses: Vec<(FaceId, EdgeId, bool)> = faces
        .flat_map(|(id, face)| {
            face.loops
                .iter()
                .flat_map(|l| &l.uses)
                .map(move |u| (id, u.edge, u.sense == Sense::Forward))
        })
        .collect();
    uses.sort_unstable();
    uses
}

/// Each of the edge's fins names a live face that uses the edge in the fin's sense; `uses`
/// holds the uses of every face the fins name.
fn fin_refs(
    t: &Tables,
    id: EdgeId,
    adj: &EdgeAdj,
    uses: &[(FaceId, EdgeId, bool)],
) -> Result<(), StructureCause> {
    for &fin in &adj.fins {
        if t.faces.get(fin.face).is_none() {
            return Err(dangling(id.into(), fin.face.into()));
        }
        let key = (fin.face, id, fin.sense == Sense::Forward);
        if uses.binary_search(&key).is_err() {
            return Err(StructureCause::FinWithoutUse { edge: id, fin });
        }
    }
    Ok(())
}

/// The outside region is there and void, every id a record names is live, and every fin has
/// its use: what lets a finished body's views look across records without an `Option`.
/// Records are walked in kind order, then id order.
pub(crate) fn references(t: &Tables) -> Result<(), StructureCause> {
    outside(t)?;
    each(&t.shells, Sel::All, |id, r| shell_refs(t, id, r))?;
    each(&t.faces, Sel::All, |id, r| face_refs(t, id, r))?;
    each(&t.edges, Sel::All, |id, r| edge_refs(t, id, r))?;
    let uses = use_keys(t.faces.iter().map(|(id, f)| (id, f.as_ref())));
    each(&t.adjacency, Sel::All, |id, a| fin_refs(t, id, a, &uses))
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

/// The shell's wires have no fins.
fn shell_wires(t: &Tables, id: ShellId, shell: &ShellRecord) -> Result<(), StructureCause> {
    for &e in &shell.wires {
        if t.adjacency.get(e).is_some_and(|a| !a.fins.is_empty()) {
            return Err(StructureCause::WireWithFins { shell: id, edge: e });
        }
    }
    Ok(())
}

/// One edge: a collapsed edge's ends are one vertex, the edge has a fin or is a wire, and
/// each pair of fins meeting across a sector agrees on the shell between them.
fn edge_structure(
    t: &Tables,
    id: EdgeId,
    edge: &EdgeGeom,
    mut is_wire: impl FnMut(EdgeId) -> bool,
) -> Result<(), StructureCause> {
    if edge.shape == EdgeShape::Collapsed && edge.ends[0] != edge.ends[1] {
        return Err(StructureCause::CollapsedEnds { edge: id });
    }
    let fins = t.adjacency.get(id).map_or(&[][..], |a| &a.fins);
    if fins.is_empty() && !is_wire(id) {
        return Err(StructureCause::LooseEdge { edge: id });
    }
    for (fin, next) in radial(t, id) {
        let shell = |f: Fin, side: FaceSide| t.faces.get(f.face).map(|r| r.side(side));
        if shell(fin, after(fin.sense)) != shell(next, after(next.sense).other()) {
            return Err(StructureCause::SidesDisagree {
                edge: id,
                fins: [fin, next],
            });
        }
    }
    Ok(())
}

/// The body invariants, after [`references`] has made every id resolve. First what each
/// record must hold with its neighbours, in kind order (shells' wires, faces' loops and
/// uses, edges' fins, vertices), then what holds of shells and regions as wholes, which
/// relies on the sides the edge pass checked.
pub(crate) fn structure(t: &Tables) -> Result<(), StructureCause> {
    each(&t.shells, Sel::All, |id, r| shell_wires(t, id, r))?;
    let mut wire = vec![false; t.edges.slots()];
    for (_, shell) in t.shells.iter() {
        for &e in &shell.wires {
            if let Some(w) = wire.get_mut(e.index()) {
                *w = true;
            }
        }
    }
    each(&t.faces, Sel::All, |id, r| faces(t, id, r))?;
    each(&t.edges, Sel::All, |id, r| {
        edge_structure(t, id, r, |e| wire.get(e.index()).copied().unwrap_or(false))
    })?;
    let mut ends = vec![false; t.vertices.slots()];
    let lone = t
        .shells
        .iter()
        .flat_map(|(_, s)| s.lone_vertices.iter().copied());
    for v in t.edges.iter().flat_map(|(_, e)| e.ends).chain(lone) {
        if let Some(e) = ends.get_mut(v.index()) {
            *e = true;
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
fn faces(t: &Tables, id: FaceId, face: &FaceRecord) -> Result<(), StructureCause> {
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
    if let Some(&(edge, forward)) = seen.windows(2).find_map(|w| match w {
        [a, b] if a == b => Some(a),
        _ => None,
    }) {
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
    if let Some(vertex) = on.windows(2).find_map(|w| match *w {
        [(a, _), (b, _)] if a == b => Some(a),
        _ => None,
    }) {
        return Err(StructureCause::VertexOnTwoLoops { face: id, vertex });
    }
    Ok(())
}

/// Disjoint sets over slots, with path halving.
pub(crate) struct Sets(Vec<u32>);

impl Sets {
    pub(crate) fn new(n: usize) -> Self {
        Self(
            (0..n)
                .map(|i| u32::try_from(i).unwrap_or(u32::MAX))
                .collect(),
        )
    }

    pub(crate) fn root(&mut self, mut i: usize) -> usize {
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

    pub(crate) fn join(&mut self, a: usize, b: usize) {
        let (ra, rb) = (self.root(a), self.root(b));
        let (lo, hi) = (ra.min(rb), ra.max(rb));
        if let Some(slot) = self.0.get_mut(hi) {
            *slot = u32::try_from(lo).unwrap_or(u32::MAX);
        }
    }
}

/// Each fin of the edge with the next, cyclically: the pairs that meet across a sector.
/// A collapsed edge has no length to turn about, so no radial order and no pairs: a pole's
/// single fin has its face on both sides.
fn radial(t: &Tables, id: EdgeId) -> impl Iterator<Item = (Fin, Fin)> + '_ {
    let collapsed = t
        .edges
        .get(id)
        .is_none_or(|e| e.shape == EdgeShape::Collapsed);
    let fins = match t.adjacency.get(id) {
        Some(a) if !collapsed => &a.fins[..],
        _ => &[][..],
    };
    fins.iter()
        .copied()
        .zip(fins.iter().copied().cycle().skip(1))
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
    for (id, _) in t.edges.iter() {
        for (fin, next) in radial(t, id) {
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
        if let [(s0, v0, a), (s1, v1, b)] = *w
            && (s0, v0) == (s1, v1)
        {
            sets.join(a, b);
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
