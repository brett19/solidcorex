// Written for solidcorex. Apache-2.0.
//! The checks `finish` runs (01 §2.3). BR1 runs what the views' totality needs; BR2 adds
//! the structure invariants and the cheap geometry check after [`references`].

use std::collections::BTreeSet;

use crate::finish::{FinishCause, StructureCause};
use crate::id::{EdgeId, Entity, RegionId};
use crate::tables::Tables;

/// Every check, in order; the first failure is the cause.
pub(crate) fn run(t: &Tables, pending: &BTreeSet<EdgeId>) -> Result<(), FinishCause> {
    if let Some(&edge) = pending.first() {
        return Err(FinishCause::MissingCurve { edge });
    }
    references(t).map_err(FinishCause::Structure)
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
