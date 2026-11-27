// Written for solidcorex. Apache-2.0.
//! What an edit changed (01 §2.6), returned by [`Draft::finish`](crate::Draft::finish), and
//! [`carry`], which takes an application's data across it.
//!
//! The draft records as it edits; nothing is reconstructed afterwards (01 §2.3):
//!
//! - **Created** entities are found from the ids: ids are dense and never reused, so an
//!   entity live at `finish` whose id is past the draft's first slot count is new. Adding
//!   records nothing.
//! - **Replaced** records are found by comparison: the first write through a `*_mut` call to
//!   an entity the draft started with keeps its record as it was, and `finish` compares.
//!   A write that changes nothing reports nothing, and an edge whose curve is attached anew
//!   is a [`Touch::Geometry`].
//! - **Removed** entities are listed as they are removed.
//! - **Lineage** is declared by the operation: [`Draft::derive`](crate::Draft::derive) names
//!   where a new entity came from, `merge_*` names a survivor, and
//!   [`Draft::coincident`](crate::Draft::coincident) an operand's entity landing on one of
//!   this body's. Splits follow from the origins: the pieces of `f` are `f`, if it is still
//!   there, and the new entities of `f`'s kind whose origins name it, or that a kept
//!   survivor took in ([`Merge::taken`]). A merge keeps lineage whichever entity survives:
//!   a new entity absorbed into a kept one hands it its origins, a kept one absorbed into a
//!   new one becomes an origin of it (and is listed removed).
//! - **Within one edit**, `carry` applies splits before merges; an edit whose result
//!   depends on the order (a quantity a split shares out) is two edits, carried in turn.
//!
//! The pin's notify events and its attribute store (`geom/attribute.rs`,
//! `solidcore-ops/src/provenance.rs`, READ) have no counterpart; [`Carry`] is the
//! application's rule in place of the provenance hook.
//!
//! Regions and shells are not reported: no consumer reads them yet, and [`Changes`] is
//! `#[non_exhaustive]`.

use core::cmp::Ordering;
use core::fmt;
use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use scx_num::error::ErrorKind;

use crate::id::{EdgeId, Entity, FaceId, IdMap, VertexId};
use crate::record::{EdgeGeom, EdgeShape, FaceRecord, UseGeom, VertexRecord};
use crate::table::{Id, Table};
use crate::tables::Tables;

#[cfg(test)]
mod tests;

/// How a kept entity's record changed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Touch {
    /// Its boundary: a face's loops or sides, an edge's end vertices.
    Boundary,
    /// Its geometry or recorded accuracy: a face's surface, an edge's shape, a vertex's
    /// point. A record whose geometry and boundary both changed is `Geometry`.
    Geometry,
}

/// One of the operation's operands, by its place in the operation's own list of them. A
/// boolean's tool is commonly `Operand::from_raw(1)`, the body edited being operand 0.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Operand(u32);

impl Operand {
    /// The operand numbered `raw`.
    #[must_use]
    pub const fn from_raw(raw: u32) -> Self {
        Self(raw)
    }

    /// Its number.
    #[must_use]
    pub const fn raw(self) -> u32 {
        self.0
    }

    /// Its number as an index into the operation's list.
    #[must_use]
    pub const fn index(self) -> usize {
        self.0 as usize
    }
}

impl fmt::Display for Operand {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "operand {}", self.0)
    }
}

/// Where a new entity came from. The ids are the edited body's as it was before the edit,
/// except in `Operand`, which names an entity of another body the operation took.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Origin {
    /// Part of this face: a piece of it when its kind is a face.
    Face(FaceId),
    /// Part of this edge, or made along it (a blend face, a wall). For a new edge it always
    /// means a piece: an edge made along `e` but not lying on it (a blend's rail) names the
    /// faces it lies on, or nothing.
    Edge(EdgeId),
    /// Part of this vertex, or made at it. For a new vertex it always means a piece.
    Vertex(VertexId),
    /// An offset of this entity: a shell's inner wall, a thickened sheet's other side. Not a
    /// piece of it, so never a split.
    Offset(Entity),
    /// This entity of an operand.
    Operand(Operand, Entity),
    /// Nothing the edit started with: an intersection edge, a primitive's faces.
    Free,
}

impl Origin {
    /// The edited body's entity this origin names as a part of it: `Face`, `Edge` and
    /// `Vertex`.
    #[must_use]
    pub const fn part_of(self) -> Option<Entity> {
        match self {
            Self::Face(f) => Some(Entity::Face(f)),
            Self::Edge(e) => Some(Entity::Edge(e)),
            Self::Vertex(v) => Some(Entity::Vertex(v)),
            Self::Offset(_) | Self::Operand(..) | Self::Free => None,
        }
    }

    /// `entity` of the edited body as an origin.
    #[must_use]
    pub const fn of(entity: Entity) -> Option<Self> {
        match entity {
            Entity::Face(f) => Some(Self::Face(f)),
            Entity::Edge(e) => Some(Self::Edge(e)),
            Entity::Vertex(v) => Some(Self::Vertex(v)),
            Entity::Region(_) | Entity::Shell(_) => None,
        }
    }
}

/// A new entity's origins: sorted, without repeats, never empty. `[Free]` when nothing was
/// declared; `Free` never stands beside another origin.
#[derive(Clone, PartialEq, Eq, Hash)]
pub struct Origins(Vec<Origin>);

impl fmt::Debug for Origins {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("Origins").field(&self.as_slice()).finish()
    }
}

impl PartialOrd for Origins {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Origins {
    fn cmp(&self, other: &Self) -> Ordering {
        self.as_slice().cmp(other.as_slice())
    }
}

impl Origins {
    /// From `list` in any order: sorted, deduplicated, `Free` dropped beside others.
    fn normal(mut list: Vec<Origin>) -> Self {
        list.sort_unstable();
        list.dedup();
        // Held empty for `[Free]`, so a free entity costs no allocation.
        list.retain(|o| *o != Origin::Free);
        Self(list)
    }

    /// `[Free]`.
    const FREE: Self = Self(Vec::new());

    /// The origins in order.
    #[must_use]
    pub fn as_slice(&self) -> &[Origin] {
        if self.0.is_empty() {
            &[Origin::Free]
        } else {
            &self.0
        }
    }

    /// The origins in order.
    pub fn iter(&self) -> core::slice::Iter<'_, Origin> {
        self.as_slice().iter()
    }

    /// Whether `origin` is one of them.
    #[must_use]
    pub fn contains(&self, origin: Origin) -> bool {
        self.as_slice().binary_search(&origin).is_ok()
    }
}

impl<'a> IntoIterator for &'a Origins {
    type Item = &'a Origin;
    type IntoIter = core::slice::Iter<'a, Origin>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

/// Entities merged into one that keeps its id (57 §5.1).
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Merge<I> {
    /// The entity that stays.
    pub survivor: I,
    /// The entities it took in, ascending. They are gone, and are not listed as removed.
    pub absorbed: Vec<I>,
    /// What the entities this edit made and then merged into it came from, sorted: such an
    /// entity has no id before or after the edit, so its lineage is handed on here. A
    /// survivor that took in only new entities has `absorbed` empty.
    pub taken: Vec<Origin>,
}

/// An operand's entity found lying on one of the body's, which takes its place (57 §5.1).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Coincidence<I> {
    /// The operand it belongs to.
    pub operand: Operand,
    /// Its id in the operand.
    pub entity: I,
    /// The body's entity it lies on.
    pub onto: I,
}

/// An entity cut into pieces.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Split<I> {
    /// The entity cut.
    pub from: I,
    /// Two or more pieces, `from` among them if it kept its id, in the order of 01 §2.6
    /// (research 60 P1): by the edited body's entities that bound each piece, then by where
    /// it lies.
    pub pieces: Vec<I>,
}

/// What an edit did to the entities of one kind. Every list is sorted by id.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct EntityChanges<I> {
    /// Entities that kept their id and had their record changed.
    pub replaced: Vec<(I, Touch)>,
    /// New entities, with where each came from.
    pub created: Vec<(I, Origins)>,
    /// Merges, by survivor.
    pub merged: Vec<Merge<I>>,
    /// Coincidences, by operand then operand entity.
    pub coincident: Vec<Coincidence<I>>,
    /// Entities the edit started with that are gone, other than those a kept survivor
    /// absorbed (`merged`); one merged into a new entity is here, and an origin of it.
    pub removed: Vec<I>,
    /// Splits, by the entity cut.
    pub split: Vec<Split<I>>,
}

impl<I> Default for EntityChanges<I> {
    fn default() -> Self {
        Self {
            replaced: Vec::new(),
            created: Vec::new(),
            merged: Vec::new(),
            coincident: Vec::new(),
            removed: Vec::new(),
            split: Vec::new(),
        }
    }
}

impl<I: Tracked> EntityChanges<I> {
    /// Whether nothing of this kind changed.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.replaced.is_empty()
            && self.created.is_empty()
            && self.merged.is_empty()
            && self.coincident.is_empty()
            && self.removed.is_empty()
            && self.split.is_empty()
    }

    /// The origins of the new entity `id`.
    #[must_use]
    pub fn origins(&self, id: I) -> Option<&Origins> {
        let at = self.created.binary_search_by(|(c, _)| c.cmp(&id)).ok()?;
        self.created.get(at).map(|(_, o)| o)
    }

    /// How the kept entity `id` was touched.
    #[must_use]
    pub fn touch(&self, id: I) -> Option<Touch> {
        let at = self.replaced.binary_search_by(|(r, _)| r.cmp(&id)).ok()?;
        self.replaced.get(at).map(|&(_, t)| t)
    }

    /// The split of `from`.
    #[must_use]
    pub fn split_of(&self, from: I) -> Option<&Split<I>> {
        let at = self.split.binary_search_by(|s| s.from.cmp(&from)).ok()?;
        self.split.get(at)
    }
}

/// What an edit changed, by kind.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
#[non_exhaustive]
pub struct Changes {
    /// The faces.
    pub faces: EntityChanges<FaceId>,
    /// The edges.
    pub edges: EntityChanges<EdgeId>,
    /// The vertices.
    pub vertices: EntityChanges<VertexId>,
}

impl Changes {
    /// Whether the edit changed nothing.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.faces.is_empty() && self.edges.is_empty() && self.vertices.is_empty()
    }
}

mod sealed {
    pub trait Sealed {}
    impl Sealed for crate::id::FaceId {}
    impl Sealed for crate::id::EdgeId {}
    impl Sealed for crate::id::VertexId {}
}

/// The id of a kind [`Changes`] reports: [`FaceId`], [`EdgeId`] or [`VertexId`].
pub trait Tracked:
    Copy + Ord + fmt::Debug + Send + Sync + Into<Entity> + sealed::Sealed + 'static
{
    /// `entity` if it is of this kind.
    fn of(entity: Entity) -> Option<Self>;

    /// The id as an origin of a new entity.
    fn origin(self) -> Origin;

    /// The id after `map`'s renumbering: `None` for one it dropped.
    fn renumbered(self, map: &IdMap) -> Option<Self>;
}

macro_rules! tracked {
    ($id:ident, $kind:ident, $map:ident) => {
        impl Tracked for $id {
            fn of(entity: Entity) -> Option<Self> {
                match entity {
                    Entity::$kind(id) => Some(id),
                    _ => None,
                }
            }

            fn origin(self) -> Origin {
                Origin::$kind(self)
            }

            fn renumbered(self, map: &IdMap) -> Option<Self> {
                map.$map(self)
            }
        }
    };
}

tracked!(FaceId, Face, face);
tracked!(EdgeId, Edge, edge);
tracked!(VertexId, Vertex, vertex);

impl IdMap {
    /// An application table keyed by old ids, keyed by the new: what lineage across
    /// [`Brep::compact`](crate::Brep::compact) needs, after [`carry`] across the edit.
    #[must_use]
    pub fn renumber<I: Tracked, T>(&self, table: BTreeMap<I, T>) -> BTreeMap<I, T> {
        table
            .into_iter()
            .filter_map(|(id, v)| Some((id.renumbered(self)?, v)))
            .collect()
    }
}

/// An application's rule for its data at each change: what the entity named ends up
/// holding, `None` for nothing.
pub trait Carry<T> {
    /// What a piece of an entity holding `from` holds. Every piece of a split gets it, the
    /// piece that kept the id too.
    fn split(&self, from: &T) -> Option<T>;

    /// What a merge of entities holding `values` holds: the survivor's first when it held
    /// one, then the absorbed entities' in id order. Also what a new entity with several
    /// origins holds. Never called with fewer than two.
    fn merge(&self, values: &[&T]) -> Option<T>;

    /// What `onto`, holding `onto` (if anything), holds after an operand's entity holding
    /// `from` is found lying on it.
    fn coincident(&self, onto: Option<&T>, from: &T) -> Option<T>;

    /// What an offset of an entity holding `from` holds: a copy beside it, not a piece of it
    /// (a temporary mark is not copied, a shared reference is).
    fn offset(&self, from: &T) -> Option<T>;
}

/// `own`, an application table over the edited body's entities of one kind, carried across
/// `changes` by `rule`. `operands` are the tables over the operands' entities, by
/// [`Operand`] index.
///
/// - A removed or absorbed entity's value goes; a kept one's stays, replaced or not.
/// - Each piece of a split gets `rule.split` of what was cut.
/// - A new entity takes the values of its origins of its own kind: a piece's through
///   `rule.split` if that origin was split, an offset's through `rule.offset`, an operand
///   entity's as it is. One value is taken as it is, several are merged. Origins of another
///   kind carry nothing.
/// - A survivor takes `rule.merge` of its own value, the absorbed entities', and those of
///   [`Merge::taken`] (handed on as a new entity's would be).
/// - Coincidences come last.
#[must_use]
pub fn carry<I: Tracked, T: Clone>(
    changes: &EntityChanges<I>,
    own: &BTreeMap<I, T>,
    operands: &[&BTreeMap<I, T>],
    rule: &impl Carry<T>,
) -> BTreeMap<I, T> {
    let absorbed: BTreeSet<I> = changes
        .merged
        .iter()
        .flat_map(|m| m.absorbed.iter().copied())
        .collect();
    let gone = |id: &I| changes.removed.binary_search(id).is_ok() || absorbed.contains(id);
    let split = |id: I| changes.split_of(id).is_some();
    // What an origin of the edited body hands on.
    let from_own = |id: I| -> Option<T> {
        let v = own.get(&id)?;
        if split(id) {
            rule.split(v)
        } else {
            Some(v.clone())
        }
    };
    // What an origin hands on: a part's value, an offset's, an operand entity's.
    let from = |o: Origin| -> Option<T> {
        match o {
            Origin::Offset(e) => rule.offset(&from_own(I::of(e)?)?),
            Origin::Operand(k, e) => operands.get(k.index())?.get(&I::of(e)?).cloned(),
            o => from_own(I::of(o.part_of()?)?),
        }
    };
    let mut out: BTreeMap<I, T> = own
        .iter()
        .filter(|(id, _)| !gone(id))
        .filter_map(|(&id, v)| {
            let v = if split(id) { rule.split(v)? } else { v.clone() };
            Some((id, v))
        })
        .collect();
    for (id, origins) in &changes.created {
        let values: Vec<T> = origins.into_iter().filter_map(|&o| from(o)).collect();
        let value = match values.as_slice() {
            [] => None,
            [one] => Some(one.clone()),
            many => rule.merge(&many.iter().collect::<Vec<_>>()),
        };
        if let Some(v) = value {
            out.insert(*id, v);
        }
    }
    for m in &changes.merged {
        let taken: Vec<T> = m.taken.iter().filter_map(|&o| from(o)).collect();
        let mut values: Vec<&T> = out.get(&m.survivor).into_iter().collect();
        let had_own = !values.is_empty();
        values.extend(m.absorbed.iter().filter_map(|a| own.get(a)));
        values.extend(&taken);
        if values.len() < 2 && had_own {
            continue;
        }
        let merged = match values.as_slice() {
            [] => None,
            [one] => Some((*one).clone()),
            many => rule.merge(many),
        };
        match merged {
            Some(v) => out.insert(m.survivor, v),
            None => out.remove(&m.survivor),
        };
    }
    for c in &changes.coincident {
        let Some(from) = operands
            .get(c.operand.index())
            .and_then(|t| t.get(&c.entity))
        else {
            continue;
        };
        match rule.coincident(out.get(&c.onto), from) {
            Some(v) => out.insert(c.onto, v),
            None => out.remove(&c.onto),
        };
    }
    out
}

/// Why [`Draft::derive`](crate::Draft::derive) refused.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum LineageError {
    /// The entity given an origin is not one the draft made and still holds: the lineage of
    /// an entity the edit started with is its own id.
    NotCreated {
        /// The entity.
        entity: Entity,
    },
    /// The origin names an entity of this body the draft neither started with nor made and
    /// still holds, or a region or shell.
    UnknownOrigin {
        /// The origin.
        origin: Origin,
    },
}

impl LineageError {
    /// [`ErrorKind::Refused`]: the operation named the wrong entity.
    #[must_use]
    pub const fn kind(&self) -> ErrorKind {
        ErrorKind::Refused
    }

    /// The stable name.
    #[must_use]
    pub const fn name(&self) -> &'static str {
        match self {
            Self::NotCreated { .. } => "not_created",
            Self::UnknownOrigin { .. } => "unknown_origin",
        }
    }
}

impl fmt::Display for LineageError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotCreated { entity } => {
                write!(f, "{entity} is not an entity this draft made and holds")
            }
            Self::UnknownOrigin { origin } => {
                write!(f, "the origin {origin:?} names no entity of the edit")
            }
        }
    }
}

impl core::error::Error for LineageError {}

/// The slot counts a draft started with: an id at or past its kind's count is new.
#[derive(Debug, Clone, Copy)]
struct Start {
    faces: usize,
    edges: usize,
    vertices: usize,
}

/// The records, as they were, of the entities written so far: a bit per slot the draft
/// started with, and the records in the order first written.
#[derive(Debug, Clone)]
struct Snaps<I, S> {
    seen: Vec<u64>,
    list: Vec<(I, S)>,
}

impl<I, S> Default for Snaps<I, S> {
    fn default() -> Self {
        Self {
            seen: Vec::new(),
            list: Vec::new(),
        }
    }
}

impl<I: Id + Ord, S> Snaps<I, S> {
    /// Keeps `was()` for `id` (below `start`) unless it was kept already.
    fn keep(&mut self, id: I, start: usize, was: impl FnOnce() -> S) {
        if self.seen.is_empty() {
            self.seen = vec![0; start.div_ceil(64)];
        }
        let slot = id.slot();
        let Some(word) = self.seen.get_mut(slot / 64) else {
            return;
        };
        let bit = 1u64 << (slot % 64);
        if *word & bit == 0 {
            *word |= bit;
            self.list.push((id, was()));
        }
    }

    /// Into id order.
    fn sort(&mut self) {
        self.list.sort_unstable_by_key(|(id, _)| *id);
    }
}

/// What a draft records as it edits, for [`Changes`].
#[derive(Debug, Clone)]
pub(crate) struct Log {
    start: Start,
    /// The records, as they were, of the kept entities written so far.
    faces: Snaps<FaceId, Arc<FaceRecord>>,
    edges: Snaps<EdgeId, EdgeGeom>,
    vertices: Snaps<VertexId, VertexRecord>,
    /// Entities the draft started with, removed (not absorbed), in the order removed.
    removed: Vec<Entity>,
    /// Declared origins of new entities, already resolved to the edit's own.
    origins: BTreeMap<Entity, Vec<Origin>>,
    /// Absorbed entity to survivor.
    heirs: BTreeMap<Entity, Entity>,
    /// What each entity the draft started with has taken in by merges so far, as origins:
    /// the entities it absorbed and the origins of new entities it absorbed.
    held: BTreeMap<Entity, Vec<Origin>>,
    coincident: Vec<(Operand, Entity, Entity)>,
}

impl Log {
    pub(crate) fn new(t: &Tables) -> Self {
        Self {
            start: Start {
                faces: t.faces.slots(),
                edges: t.edges.slots(),
                vertices: t.vertices.slots(),
            },
            faces: Snaps::default(),
            edges: Snaps::default(),
            vertices: Snaps::default(),
            removed: Vec::new(),
            origins: BTreeMap::new(),
            heirs: BTreeMap::new(),
            held: BTreeMap::new(),
            coincident: Vec::new(),
        }
    }

    /// Whether the draft started with this slot of `entity`'s kind (it may since be gone).
    /// Regions and shells are not tracked: never.
    pub(crate) const fn started_with(&self, entity: Entity) -> bool {
        match entity {
            Entity::Face(f) => f.index() < self.start.faces,
            Entity::Edge(e) => e.index() < self.start.edges,
            Entity::Vertex(v) => v.index() < self.start.vertices,
            Entity::Region(_) | Entity::Shell(_) => false,
        }
    }

    /// Before the first write to a face the draft started with: keeps the record.
    pub(crate) fn face_written(&mut self, id: FaceId, face: &Arc<FaceRecord>) {
        if id.index() < self.start.faces {
            self.faces.keep(id, self.start.faces, || Arc::clone(face));
        }
    }

    /// Before the first write to an edge's geometry part: keeps it, without its cells.
    pub(crate) fn edge_written(&mut self, id: EdgeId, geom: &EdgeGeom) {
        if id.index() < self.start.edges {
            self.edges.keep(id, self.start.edges, || EdgeGeom {
                shape: geom.shape.clone(),
                ends: geom.ends,
                accuracy: geom.accuracy.clone(),
                cells: crate::cells::EdgeCells::default(),
            });
        }
    }

    /// Before the first write to a vertex.
    pub(crate) fn vertex_written(&mut self, id: VertexId, vertex: &VertexRecord) {
        if id.index() < self.start.vertices {
            self.vertices
                .keep(id, self.start.vertices, || vertex.clone());
        }
    }

    /// `entity` was removed.
    pub(crate) fn removed(&mut self, entity: Entity) {
        if self.started_with(entity) {
            self.removed.push(entity);
        }
    }

    /// `absorbed` was merged into `survivor`, which takes what it held: a new entity's
    /// origins, or an old one and what it took in.
    pub(crate) fn absorbed(&mut self, absorbed: Entity, survivor: Entity) {
        let what = if self.started_with(absorbed) {
            let mut what: Vec<Origin> = Origin::of(absorbed).into_iter().collect();
            what.extend(self.held.remove(&absorbed).unwrap_or_default());
            what
        } else {
            self.origins.remove(&absorbed).unwrap_or_default()
        };
        let into = if self.started_with(survivor) {
            &mut self.held
        } else {
            &mut self.origins
        };
        into.entry(survivor).or_default().extend(what);
        self.heirs.insert(absorbed, survivor);
    }

    /// `origin`, resolved: an origin naming a new entity stands for that entity's origins
    /// as they are now, and one naming a survivor of a merge for it and what it took in.
    pub(crate) fn resolve(&self, origin: Origin) -> Vec<Origin> {
        let own = |e: Entity| {
            if self.started_with(e) {
                let mut own = vec![Origin::of(e).unwrap_or(Origin::Free)];
                own.extend(self.held.get(&e).into_iter().flatten());
                own
            } else {
                self.origins
                    .get(&e)
                    .cloned()
                    .unwrap_or_else(|| vec![Origin::Free])
            }
        };
        match origin {
            Origin::Offset(e) => own(e)
                .into_iter()
                .map(|o| match o {
                    o @ (Origin::Face(_) | Origin::Edge(_) | Origin::Vertex(_)) => {
                        o.part_of().map_or(Origin::Free, Origin::Offset)
                    }
                    other => other,
                })
                .collect(),
            o => match o.part_of() {
                Some(e) => own(e),
                None => vec![o],
            },
        }
    }

    /// Gives the new `entity` the resolved `origins`.
    pub(crate) fn derive(&mut self, entity: Entity, origins: Vec<Origin>) {
        self.origins.entry(entity).or_default().extend(origins);
    }

    /// An operand's `entity` lies on `onto`.
    pub(crate) fn coincident(&mut self, operand: Operand, entity: Entity, onto: Entity) {
        self.coincident.push((operand, entity, onto));
    }

    /// The changes from the draft's start to `t`.
    pub(crate) fn changes(mut self, t: &Tables) -> Changes {
        self.faces.sort();
        self.edges.sort();
        self.vertices.sort();
        let vertices = self.lineage(&t.vertices, self.start.vertices, |v| PieceKey {
            bound: Vec::new(),
            at: t
                .vertices
                .get(v)
                .map_or_default(|r| vec![r.point.x, r.point.y, r.point.z]),
            sides: Vec::new(),
        });
        let vertices = with_touches(vertices, &self.vertices, &t.vertices, vertex_touch);
        // A face's lineage as declared: the faces' own lineage is found after the edges'.
        let face = |f: FaceId| {
            if f.index() < self.start.faces {
                vec![Origin::Face(f)]
            } else {
                self.origins
                    .get(&f.into())
                    .cloned()
                    .unwrap_or_else(|| vec![Origin::Free])
            }
        };
        let edges = self.lineage(&t.edges, self.start.edges, |e| {
            edge_key(t, self.start.vertices, &vertices, &face, e)
        });
        let edges = with_touches(edges, &self.edges, &t.edges, edge_touch);
        let faces = self.lineage(&t.faces, self.start.faces, |f| {
            face_key(t, self.start.edges, &edges, f)
        });
        let faces = with_touches(faces, &self.faces, &t.faces, face_touch);
        Changes {
            faces,
            edges,
            vertices,
        }
    }

    /// Everything but the touches, for one kind.
    fn lineage<I: Tracked + Id, R>(
        &self,
        table: &Table<I, R>,
        start: usize,
        key: impl Fn(I) -> PieceKey,
    ) -> EntityChanges<I> {
        let live = |id: I| table.get(id).is_some();
        let before = |id: I| id.slot() < start;
        let mut origins: BTreeMap<I, Vec<Origin>> = self
            .origins
            .iter()
            .filter_map(|(&e, o)| Some((I::of(e)?, o.clone())))
            .collect();
        let heir = |mut id: I| {
            while let Some(next) = self.heirs.get(&id.into()).and_then(|&e| I::of(e)) {
                id = next;
            }
            id
        };
        let mut removed: Vec<I> = self.removed.iter().filter_map(|&e| I::of(e)).collect();
        let mut merged: BTreeMap<I, Vec<I>> = BTreeMap::new();
        for &a in self.heirs.keys() {
            let Some(a) = I::of(a).filter(|&a| before(a)) else {
                continue;
            };
            let t = heir(a);
            if !live(t) {
                removed.push(a);
            } else if before(t) {
                merged.entry(t).or_default().push(a);
            } else {
                // Gone into a new entity, which names it as an origin: no merge lists it.
                removed.push(a);
                origins.entry(t).or_default().push(a.origin());
            }
        }
        removed.sort_unstable();
        let created: Vec<(I, Origins)> = (start..table.slots())
            .filter_map(|slot| {
                let id = I::from_slot(u32::try_from(slot).ok()?);
                live(id).then(|| {
                    let o = if origins.is_empty() {
                        Origins::FREE
                    } else {
                        origins.remove(&id).map_or(Origins::FREE, Origins::normal)
                    };
                    (id, o)
                })
            })
            .collect();
        // What new entities merged into a kept survivor came from: its holdings less the
        // old entities it absorbed, which `absorbed` lists.
        let mut taken: BTreeMap<I, Vec<Origin>> = BTreeMap::new();
        for (&s, held) in &self.held {
            let Some(s) = I::of(s).filter(|&s| live(s) && !self.heirs.contains_key(&s.into()))
            else {
                continue;
            };
            let absorbed = merged.get(&s);
            let mut new: Vec<Origin> = held
                .iter()
                .filter(|&&o| {
                    o != Origin::Free
                        && o != s.origin()
                        && !absorbed.is_some_and(|a| a.iter().any(|a| a.origin() == o))
                })
                .copied()
                .collect();
            new.sort_unstable();
            new.dedup();
            if !new.is_empty() {
                merged.entry(s).or_default();
                taken.insert(s, new);
            }
        }
        let merged: Vec<Merge<I>> = merged
            .into_iter()
            .map(|(survivor, mut absorbed)| {
                absorbed.sort_unstable();
                Merge {
                    survivor,
                    absorbed,
                    taken: taken.remove(&survivor).unwrap_or_default(),
                }
            })
            .collect();
        let mut coincident: Vec<Coincidence<I>> = self
            .coincident
            .iter()
            .filter_map(|&(operand, entity, onto)| {
                let onto = heir(I::of(onto)?);
                live(onto).then_some(Coincidence {
                    operand,
                    entity: I::of(entity)?,
                    onto,
                })
            })
            .collect();
        coincident.sort_unstable();
        coincident.dedup();
        // The pieces of each entity cut: created entities naming it as a part, and kept
        // survivors that took in a new entity naming it.
        let mut pieces: BTreeMap<I, Vec<I>> = BTreeMap::new();
        let named = created
            .iter()
            .map(|(id, o)| (*id, o.as_slice()))
            .chain(merged.iter().map(|m| (m.survivor, m.taken.as_slice())));
        for (id, o) in named {
            for f in o.iter().filter_map(|o| I::of(o.part_of()?)) {
                if f != id {
                    pieces.entry(f).or_default().push(id);
                }
            }
        }
        let split = pieces
            .into_iter()
            .filter_map(|(from, mut pieces)| {
                if live(from) {
                    pieces.push(from);
                }
                (pieces.len() > 1).then(|| {
                    let mut keyed: Vec<(PieceKey, I)> =
                        pieces.into_iter().map(|p| (key(p), p)).collect();
                    keyed.sort_by(|a, b| a.0.cmp(&b.0).then(a.1.cmp(&b.1)));
                    Split {
                        from,
                        pieces: keyed.into_iter().map(|(_, p)| p).collect(),
                    }
                })
            })
            .collect();
        EntityChanges {
            replaced: Vec::new(),
            created,
            merged,
            coincident,
            removed,
            split,
        }
    }
}

/// `changes` with the touches of the kept entities written, from their records as they were.
fn with_touches<I: Tracked + Id, R, S>(
    mut changes: EntityChanges<I>,
    before: &Snaps<I, S>,
    table: &Table<I, R>,
    touch: impl Fn(&S, &R) -> Option<Touch>,
) -> EntityChanges<I> {
    changes.replaced = before
        .list
        .iter()
        .filter_map(|(id, was)| Some((*id, touch(was, table.get(*id)?)?)))
        .collect();
    changes
}

fn face_touch(was: &Arc<FaceRecord>, now: &Arc<FaceRecord>) -> Option<Touch> {
    if was.surface != now.surface {
        Some(Touch::Geometry)
    } else if was.loops != now.loops || was.sides != now.sides {
        Some(Touch::Boundary)
    } else {
        None
    }
}

fn edge_touch(was: &EdgeGeom, now: &EdgeGeom) -> Option<Touch> {
    if was.shape != now.shape || was.accuracy != now.accuracy {
        Some(Touch::Geometry)
    } else if was.ends != now.ends {
        Some(Touch::Boundary)
    } else {
        None
    }
}

fn vertex_touch(was: &VertexRecord, now: &VertexRecord) -> Option<Touch> {
    (was != now).then_some(Touch::Geometry)
}

/// A piece's place among its split's pieces (01 §2.6, research 60 P1): the edited body's
/// entities that bound it, then where it lies, compared exactly, then (for an edge) the
/// lineage of the faces using it, which tells apart pieces of a non-manifold edge with the
/// same ends and interval.
#[derive(Debug, Default)]
struct PieceKey {
    bound: Vec<Origin>,
    at: Vec<f64>,
    sides: Vec<Origin>,
}

impl PartialEq for PieceKey {
    fn eq(&self, other: &Self) -> bool {
        self.cmp(other) == Ordering::Equal
    }
}

impl Eq for PieceKey {}

impl PartialOrd for PieceKey {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for PieceKey {
    fn cmp(&self, other: &Self) -> Ordering {
        self.bound
            .cmp(&other.bound)
            .then_with(|| {
                let pairs = self.at.iter().zip(&other.at);
                pairs
                    .map(|(a, b)| a.total_cmp(b))
                    .find(|o| o.is_ne())
                    .unwrap_or_else(|| self.at.len().cmp(&other.at.len()))
            })
            .then_with(|| self.sides.cmp(&other.sides))
    }
}

/// What `id` stands for in a piece key: itself if the edit started with it, else its
/// origins.
fn bounding<I: Tracked>(id: I, start: usize, slot: usize, kind: &EntityChanges<I>) -> Vec<Origin> {
    if slot < start {
        vec![id.origin()]
    } else {
        kind.origins(id).map_or_default(|o| o.as_slice().to_vec())
    }
}

/// A face piece's key: the lineage of its edges, then its uses' uv midpoints, sorted.
fn face_key(t: &Tables, edge_start: usize, edges: &EntityChanges<EdgeId>, f: FaceId) -> PieceKey {
    let Some(face) = t.faces.get(f) else {
        return PieceKey::default();
    };
    let uses = || face.loops.iter().flat_map(|l| &l.uses);
    let mut bound: Vec<Origin> = uses()
        .flat_map(|u| bounding(u.edge, edge_start, u.edge.index(), edges))
        .collect();
    bound.sort_unstable();
    bound.dedup();
    let mut points: Vec<[f64; 2]> = uses()
        .map(|u| {
            let p = match &u.geom {
                UseGeom::Pcurve { curve, interval } => curve.point(interval.mid()),
                UseGeom::Point(p) => *p,
            };
            [p.x, p.y]
        })
        .collect();
    points.sort_by(|a, b| a[0].total_cmp(&b[0]).then(a[1].total_cmp(&b[1])));
    PieceKey {
        bound,
        at: points.into_iter().flatten().collect(),
        sides: Vec::new(),
    }
}

/// An edge piece's key: the lineage of its ends, then its interval, then the lineage of
/// its fins' faces.
fn edge_key(
    t: &Tables,
    vertex_start: usize,
    vertices: &EntityChanges<VertexId>,
    face: &impl Fn(FaceId) -> Vec<Origin>,
    e: EdgeId,
) -> PieceKey {
    let Some(geom) = t.edges.get(e) else {
        return PieceKey::default();
    };
    let mut bound: Vec<Origin> = geom
        .ends
        .iter()
        .flat_map(|&v| bounding(v, vertex_start, v.index(), vertices))
        .collect();
    bound.sort_unstable();
    bound.dedup();
    let at = match &geom.shape {
        EdgeShape::Curve { interval, .. } => vec![interval.min(), interval.max()],
        EdgeShape::Collapsed => Vec::new(),
    };
    let mut sides: Vec<Origin> = t
        .adjacency
        .get(e)
        .map_or_default(|a| a.fins.iter().flat_map(|fin| face(fin.face)).collect());
    sides.sort_unstable();
    sides.dedup();
    PieceKey { bound, at, sides }
}
