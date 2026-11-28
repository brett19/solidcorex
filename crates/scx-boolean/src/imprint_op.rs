// Written for solidcorex. Apache-2.0.
//! [`imprint`]: the public entry that stops after stage 5 (research 75 §2.6, BO13).
//!
//! The tool's intersection with the target is written into the target's faces and edges
//! and nothing is removed or selected: the target keeps its material and its outward
//! sense, its faces cut where the tool meets them, its edges cut at the new vertices. The
//! tool's own faces are not in the result; the pieces of the target are.

use std::collections::BTreeSet;

use scx_brep::op::{Cx, Done, Report};
use scx_brep::{Brep, EdgeId, RegionId, ShellId, VertexId};

use crate::boolean::{NoItems, interfere};
use crate::error::{BooleanDefect, BooleanError, BooleanRefusal};
use crate::faceface::face_face;
use crate::geom::{Operands, Side};
use crate::imprint::imprint as stage5;

/// What an [`imprint`] writes into the target.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Imprint {
    /// Only where the tool's surfaces cross the target's: each transversal section is a new
    /// edge, and the target's edges are cut where the tool's vertices and faces meet them.
    /// A tool edge that runs along a target face, and the outline of a tool face lying on
    /// one, cut nothing.
    Edges,
    /// The sections, and also the tool's edges lying in the target's faces (the outlines of
    /// the tool faces that lie on the target), so a target face a tool face lies on is split
    /// into the part under it and the rest.
    EdgesAndFaces,
}

/// Imprints `tool` on `target`, built in `target.edit()`: operand 0 of the result's lineage
/// is `target`, operand 1 is `tool`.
///
/// The result is the target cut by the tool and nothing else: `Done.changes` is empty
/// exactly when the tool leaves the target's faces and edges as they were (the pin's
/// returned flag is `!changes.is_empty()`), and the new section edges are in
/// `changes.edges` with the tool's entities among their origins. The target's volume and
/// outward sense are unchanged.
///
/// # Errors
///
/// As [`boolean`](crate::boolean): the refusals, unsupported cases and exhaustion of stages
/// 1 to 5, which are the only ones run.
pub fn imprint(
    target: &Brep,
    tool: &Brep,
    what: Imprint,
    cx: &mut Cx<'_>,
) -> Result<Done<NoItems>, BooleanError> {
    let res = cx.doc.resolution();
    for (side, body) in Side::BOTH.into_iter().zip([target, tool]) {
        if body.resolution() != res {
            return Err(BooleanError::Refused(BooleanRefusal::OtherDocument {
                operand: side.operand(),
            }));
        }
    }
    let mut budget = cx.budget();
    let ops = Operands { a: target, b: tool };
    let found = interfere(ops, &mut budget, cx.stop)?;
    let mut ff = face_face(ops, &found, &mut budget, cx.stop)?;
    if what == Imprint::Edges {
        // The tool's edges inside the target's faces are the outlines of tool faces lying
        // on them; an edge along a target edge cuts no face and stays.
        ff.lying
            .retain(|l| !(l.side == Side::B && l.along.is_none()));
    }
    let mut m = stage5(ops, &found, &ff, *cx.doc, &mut budget, cx.stop)?;
    // Leave the target's part: the tool's faces go, then what only they held.
    let tool_faces: BTreeSet<_> = m
        .pieces
        .iter()
        .filter(|((s, _), _)| *s == Side::B)
        .flat_map(|(_, p)| p.iter().copied())
        .collect();
    for f in tool_faces {
        m.draft.remove_face(f);
    }
    let bare: Vec<EdgeId> = m
        .draft
        .edges()
        .filter(|e| e.fins().len() == 0)
        .map(scx_brep::Edge::id)
        .collect();
    for e in bare {
        m.draft.remove_edge(e);
    }
    let ends: BTreeSet<VertexId> = m.draft.edges().flat_map(|e| e.geometry().ends).collect();
    let lone: BTreeSet<VertexId> = m
        .draft
        .shells()
        .flat_map(|s| s.record().lone_vertices.clone())
        .collect();
    let loose: Vec<VertexId> = m
        .draft
        .vertices()
        .map(scx_brep::Vertex::id)
        .filter(|v| !ends.contains(v) && !lone.contains(v))
        .collect();
    for v in loose {
        m.draft.remove_vertex(v);
    }
    let used: BTreeSet<ShellId> = m.draft.faces().flat_map(|f| f.record().sides).collect();
    let empty: Vec<ShellId> = m
        .draft
        .shells()
        .filter(|s| {
            !used.contains(&s.id())
                && s.record().wires.is_empty()
                && s.record().lone_vertices.is_empty()
        })
        .map(scx_brep::Shell::id)
        .collect();
    for s in empty {
        m.draft.remove_shell(s);
    }
    let used: BTreeSet<RegionId> = m.draft.shells().map(|s| s.record().region).collect();
    let empty: Vec<RegionId> = m
        .draft
        .regions()
        .map(scx_brep::Region::id)
        .filter(|r| *r != RegionId::OUTSIDE && !used.contains(r))
        .collect();
    for r in empty {
        m.draft.remove_region(r);
    }
    cx.stop.check()?;
    let (body, changes) = m
        .draft
        .finish(cx.doc)
        .map_err(|_| BooleanError::Defect(BooleanDefect::Build))?;
    Ok(Done::new(body, changes, Report::empty(budget.used())))
}
