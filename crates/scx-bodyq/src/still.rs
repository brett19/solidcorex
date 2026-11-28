// Written for solidcorex. Apache-2.0.
//! [`still_names`]: whether stored references to the faces, edges and vertices of an earlier
//! body still name an entity of this one, and which (research 74 §44).
//!
//! A reference is the entity's id in the earlier body and a point of it. Ids are dense and
//! never reused along a body's line of descent, and every edit says what became of each
//! entity ([`Changes`], from [`Draft::finish`](scx_brep::Draft::finish); [`IdMap`], from
//! [`Brep::compact`](scx_brep::Brep::compact)). So the descendants of a reference are read
//! from that lineage, by [`carry`], the one rule the application's own data follows: a kept
//! entity is its own descendant, the pieces of a split are the descendants of what was cut,
//! a survivor takes in what it absorbed, a new entity descends from its origins, and a
//! removed entity has none. An offset is not a descendant. Then the geometry decides: the
//! point must lie on the descendant, within coincidence of it.
//!
//! The pin matched by an allowance of a thousandth of the body's diagonal; that scaled with
//! the body and is not ported. There is no allowance: the point names a descendant when it is
//! within the entity's own coincidence, a distance the document sets.
//!
//! The body holds no history of its own, so the caller gives the steps from the earlier body
//! to this one; with none, the references are read against the same body.

use std::collections::{BTreeMap, BTreeSet};

use scx_brep::{
    Brep, Carry, Changes, EdgeId, Entity, EntityChanges, FaceId, IdMap, Tracked, VertexId, carry,
};
use scx_num::budget::{Budget, Stop};
use scx_num::vector3d::Point3;

use crate::error::{BodyqError, BodyqRefusal};
use crate::geom::{coincidence, recorded};
use crate::pick_at::{edge_distance, face_distance};

/// A stored reference: an entity of an earlier body, and a point on it when it was made.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Stored {
    /// The face, edge or vertex, by its id in the earlier body.
    pub entity: Entity,
    /// A point of the entity, in the coordinates of the body the reference is read against:
    /// a caller that moved the body moves the point the same way.
    pub at: Point3,
}

/// One step from an earlier body to the next.
#[derive(Debug, Clone, Copy)]
#[non_exhaustive]
pub enum Step<'a> {
    /// An edit, by what it changed.
    Edit(&'a Changes),
    /// A compaction, by its renumbering.
    Compact(&'a IdMap),
}

/// What a stored reference names in the body.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum Standing {
    /// The entities of the body that descend from the stored one and carry its point, within
    /// their coincidence; ascending, never empty. Several for a point on the shared edge of
    /// two pieces of a split face, or a split edge's end.
    Names(Vec<Entity>),
    /// Entities of the body descend from the stored one, but its point lies on none: the
    /// entity was moved or reshaped past the point. Ascending, never empty.
    Moved(Vec<Entity>),
    /// Nothing of the body descends from the stored entity: it was removed, or the body has
    /// no entity of its id.
    Gone,
}

/// The rule that keeps every value: a descendant is a descendant however it came.
struct Descends;

impl Carry<()> for Descends {
    fn split(&self, (): &()) -> Option<()> {
        Some(())
    }

    fn merge(&self, _: &[&()]) -> Option<()> {
        Some(())
    }

    fn coincident(&self, _: Option<&()>, (): &()) -> Option<()> {
        Some(())
    }

    fn offset(&self, (): &()) -> Option<()> {
        None
    }
}

/// The ids descending from `start` across `history`, one kind at a time.
#[allow(
    clippy::zero_sized_map_values,
    reason = "`carry` takes a table of values; only the keys, the descendants, matter here"
)]
fn descendants<I: Tracked>(
    start: I,
    history: &[Step<'_>],
    of: impl Fn(&Changes) -> &EntityChanges<I>,
) -> BTreeSet<I> {
    let mut table: BTreeMap<I, ()> = BTreeMap::from([(start, ())]);
    for step in history {
        table = match step {
            Step::Edit(changes) => carry(of(changes), &table, &[], &Descends),
            Step::Compact(map) => map.renumber(table),
        };
    }
    table.into_keys().collect()
}

/// For each stored reference, what it names in `body`, after the steps `history` that lead
/// from the body the references were made on to `body` (in order; none when it is that body).
///
/// Answers come in the order of `stored`. See [`Standing`].
///
/// # Errors
///
/// - [`BodyqError::Refused`]: a point with a coordinate that is not finite
///   ([`NotFinite`](BodyqRefusal::NotFinite)); a stored region or shell
///   ([`NotPickable`](BodyqRefusal::NotPickable)).
/// - [`BodyqError::Unsupported`] when a NURBS edge is a descendant (step 5 builds its
///   inversion).
/// - [`BodyqError::Exhausted`] and [`BodyqError::Cancelled`]; neither leaves anything behind.
/// - [`BodyqError::Defect`] on a body whose faces the method should take and cannot.
pub fn still_names(
    body: &Brep,
    stored: &[Stored],
    history: &[Step<'_>],
    budget: &mut Budget,
    stop: &Stop,
) -> Result<Vec<Standing>, BodyqError> {
    stored
        .iter()
        .map(|s| standing(body, s, history, budget, stop))
        .collect()
}

fn standing(
    body: &Brep,
    s: &Stored,
    history: &[Step<'_>],
    budget: &mut Budget,
    stop: &Stop,
) -> Result<Standing, BodyqError> {
    let p = s.at;
    if !(p.x.is_finite() && p.y.is_finite() && p.z.is_finite()) {
        return Err(BodyqError::Refused(BodyqRefusal::NotFinite));
    }
    let live: Vec<Entity> = match s.entity {
        Entity::Face(f) => descendants(f, history, |c| &c.faces)
            .into_iter()
            .filter(|&id: &FaceId| body.face(id).is_some())
            .map(Entity::Face)
            .collect(),
        Entity::Edge(e) => descendants(e, history, |c| &c.edges)
            .into_iter()
            .filter(|&id: &EdgeId| body.edge(id).is_some())
            .map(Entity::Edge)
            .collect(),
        Entity::Vertex(v) => descendants(v, history, |c| &c.vertices)
            .into_iter()
            .filter(|&id: &VertexId| body.vertex(id).is_some())
            .map(Entity::Vertex)
            .collect(),
        e @ (Entity::Region(_) | Entity::Shell(_)) => {
            return Err(BodyqError::Refused(BodyqRefusal::NotPickable {
                kind: e.kind(),
            }));
        }
    };
    if live.is_empty() {
        return Ok(Standing::Gone);
    }
    let mut names = Vec::new();
    for &e in &live {
        stop.check()?;
        if lies_on(body, e, p, budget, stop)? {
            names.push(e);
        }
    }
    Ok(if names.is_empty() {
        Standing::Moved(live)
    } else {
        Standing::Names(names)
    })
}

/// Whether `p` lies on the live entity `e` within its coincidence.
fn lies_on(
    body: &Brep,
    e: Entity,
    p: Point3,
    budget: &mut Budget,
    stop: &Stop,
) -> Result<bool, BodyqError> {
    match e {
        Entity::Face(id) => match body.face(id) {
            Some(f) => {
                let (d, _) = face_distance(body, f, p, budget, stop)?;
                Ok(d <= coincidence(body, f.accuracy()))
            }
            None => Ok(false),
        },
        Entity::Edge(id) => match body.edge(id) {
            Some(ed) => {
                let (d, _) = edge_distance(body, ed, p, budget)?;
                Ok(d <= coincidence(body, recorded(ed.accuracy())))
            }
            None => Ok(false),
        },
        Entity::Vertex(id) => Ok(body
            .vertex(id)
            .is_some_and(|v| v.point().distance(p) <= coincidence(body, recorded(v.accuracy())))),
        Entity::Region(_) | Entity::Shell(_) => Ok(false),
    }
}
