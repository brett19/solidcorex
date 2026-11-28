// Written for solidcorex. Apache-2.0.
//! A body moved to a new resolution (74 §4.3 step 8, ST6): [`rebase`].
//!
//! A body's records say "within the resolution" by holding no accuracy, so a new stamp
//! changes what they claim. [`rebase`] makes that claim honest at the new resolution:
//!
//! - an entity (vertex, edge, or a face's pcurve bound) with no recorded accuracy is
//!   within the old resolution. When that is past the new one (a finer resolution), the old
//!   resolution is recorded as its accuracy, [`Basis::Stated`], and the entity gets an
//!   [`AccuracyPastResolution`](WarningKind::AccuracyPastResolution) warning. It is never
//!   kept silently as "within" a resolution it is not within;
//! - an entity with a recorded accuracy past the new resolution keeps it, with the same
//!   warning;
//! - everything else is kept as it is: the topology, the geometry and the ids do not
//!   change, so the changes are empty and a coarse-to-fine round trip leaves the geometry
//!   as it was (it adds only the accuracies recorded on the way).
//!
//! Coarser, the coincidence grows. Two vertices of the body that come within the new
//! coincidence break vertex clearance: [`StitchRefusal::VerticesWithinCoincidence`]. No
//! vertex or edge of one body is identified with another (that is [`stitch`](crate::stitch)
//! and [`sew`](crate::sew), which the caller may run after).

use std::sync::Arc;

use scx_brep::op::{Cx, Done, Report, Warning, WarningKind};
use scx_brep::{Brep, Document, Entity, Operand, VertexId};
use scx_num::cert::{Basis, Recorded};
use scx_num::units::{Accuracy, Length, Resolution};

use crate::error::{StitchDefect, StitchError};
use crate::points::{Item, identify};
use crate::report::NoItems;

/// Work charged per entity read.
const UNIT: u64 = 1;

/// A rebased body.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct Rebased {
    /// The body, stamped with the new resolution, and its empty
    /// [`Changes`](scx_brep::Changes): every id is kept. Its report has an
    /// [`AccuracyPastResolution`](WarningKind::AccuracyPastResolution) warning on each
    /// entity in [`RebaseReport::past`].
    pub done: Done<NoItems>,
    /// What the new resolution made of the body's accuracies.
    pub report: RebaseReport,
}

/// What a rebase says about the body's accuracies.
#[derive(Debug, Clone, Default, PartialEq)]
#[non_exhaustive]
pub struct RebaseReport {
    /// The resolution the body was stamped with.
    pub from: Option<Resolution>,
    /// The resolution it is stamped with now.
    pub to: Option<Resolution>,
    /// The entities whose accuracy is past the new resolution, in entity order.
    pub past: Vec<PastResolution>,
    /// The entities that had no recorded accuracy and now hold the old resolution as one
    /// (a subset of the vertices and edges in `past`, and the faces whose pcurve bounds
    /// were set), in entity order.
    pub recorded: Vec<Entity>,
}

/// An entity whose accuracy is past the resolution it lives under.
#[derive(Debug, Clone, Copy, PartialEq)]
#[non_exhaustive]
pub struct PastResolution {
    /// The entity: a vertex, an edge, or a face (a pcurve bound of one of its uses).
    pub entity: Entity,
    /// Its accuracy.
    pub accuracy: Accuracy,
}

/// `body` moved to resolution `to` (see the [module](self)).
///
/// The result is finished under `to`: the document to run the next operation in is
/// [`Document::with_resolution`]`(to)`. `cx.doc` is not read. A body already at `to` is
/// returned as it is.
///
/// # Errors
///
/// [`StitchRefusal::VerticesWithinCoincidence`](crate::StitchRefusal::VerticesWithinCoincidence)
/// (operand 0) for two vertices of the body within the coincidence at `to`, the lower id
/// first; [`StitchError::Exhausted`] and [`StitchError::Cancelled`] from `cx`;
/// [`StitchError::Defect`] for the method's own failure.
pub fn rebase(body: &Brep, to: Resolution, cx: &mut Cx<'_>) -> Result<Rebased, StitchError> {
    let from = body.resolution();
    let mut budget = cx.budget();
    let bound = from.zone().mm();
    let zone = to.zone().mm();
    let eff = |r: Option<&Recorded>| r.map_or(bound, |r| r.acc.length().mm());
    let mut items = Vec::new();
    for v in body.vertices() {
        budget.charge(UNIT)?;
        items.push(Item {
            operand: Operand::from_raw(0),
            vertex: v.id(),
            point: v.point(),
            reach: eff(v.accuracy()).max(zone),
        });
    }
    identify(&items, &mut budget, cx.stop)?;

    let mut draft = body.edit();
    let mut past = Vec::new();
    let mut recorded = Vec::new();
    let stated = |acc: f64| -> Result<Arc<Recorded>, StitchError> {
        let acc =
            Accuracy::new(Length::from_mm(acc)).ok_or(StitchError::Defect(StitchDefect::Draft))?;
        Ok(Arc::new(Recorded {
            acc,
            basis: Basis::Stated,
        }))
    };
    let vs: Vec<(VertexId, Option<Recorded>)> = body
        .vertices()
        .map(|v| (v.id(), v.accuracy().copied()))
        .collect();
    for (id, acc) in vs {
        budget.charge(UNIT)?;
        let a = eff(acc.as_ref());
        if a > zone {
            let e = Entity::Vertex(id);
            if acc.is_none() {
                draft
                    .vertex_mut(id)
                    .ok_or(StitchError::Defect(StitchDefect::Draft))?
                    .accuracy = Some(stated(a)?);
                recorded.push(e);
            }
            past.push(PastResolution {
                entity: e,
                accuracy: accuracy(a)?,
            });
        }
    }
    let es: Vec<_> = body
        .edges()
        .map(|e| (e.id(), e.accuracy().copied()))
        .collect();
    for (id, acc) in es {
        budget.charge(UNIT)?;
        let a = eff(acc.as_ref());
        if a > zone {
            let e = Entity::Edge(id);
            if acc.is_none() {
                draft
                    .edge_mut(id)
                    .ok_or(StitchError::Defect(StitchDefect::Draft))?
                    .accuracy = Some(stated(a)?);
                recorded.push(e);
            }
            past.push(PastResolution {
                entity: e,
                accuracy: accuracy(a)?,
            });
        }
    }
    let faces: Vec<_> = body.faces().map(scx_brep::Face::id).collect();
    for id in faces {
        budget.charge(UNIT)?;
        let worst = draft
            .face(id)
            .ok_or(StitchError::Defect(StitchDefect::Draft))?
            .record()
            .loops
            .iter()
            .flat_map(|l| l.uses.iter())
            .map(|u| eff(u.bound.as_deref()))
            .fold(0.0_f64, f64::max);
        if worst > zone {
            let new = stated(bound)?;
            let set = draft
                .edit_face(id, |f| {
                    let mut set = false;
                    for u in f.loops.iter_mut().flat_map(|l| l.uses.iter_mut()) {
                        if u.bound.is_none() {
                            u.bound = Some(Arc::clone(&new));
                            set = true;
                        }
                    }
                    set
                })
                .ok_or(StitchError::Defect(StitchDefect::Draft))?;
            if set {
                recorded.push(Entity::Face(id));
            }
            past.push(PastResolution {
                entity: Entity::Face(id),
                accuracy: accuracy(worst)?,
            });
        }
    }
    past.sort_by_key(|p| p.entity);
    recorded.sort();
    let warnings = past
        .iter()
        .map(|p| Warning {
            entity: p.entity,
            kind: WarningKind::AccuracyPastResolution,
        })
        .collect();
    let (out, changes) = draft
        .finish(&Document::with_resolution(to))
        .map_err(|u| StitchError::Defect(StitchDefect::Finish(Box::new(u.cause))))?;
    let report = Report::new(warnings, Vec::new(), Vec::new(), budget.used());
    Ok(Rebased {
        done: Done::new(out, changes, report),
        report: RebaseReport {
            from: Some(from),
            to: Some(to),
            past,
            recorded,
        },
    })
}

fn accuracy(mm: f64) -> Result<Accuracy, StitchError> {
    Accuracy::new(Length::from_mm(mm)).ok_or(StitchError::Defect(StitchDefect::Draft))
}
