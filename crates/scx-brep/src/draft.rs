// Written for solidcorex. Apache-2.0.
//! The editable body (01 §2.3): every write goes through a [`Draft`], and
//! [`Draft::finish`] checks it and gives back an immutable [`Brep`].
//!
//! A draft made by [`Brep::edit`] shares every table with its body until it writes one;
//! [`Brep::into_draft`] writes in place where the body held the only reference.
//!
//! The cell rules (01 §3.2, research 63 §6.9) live here: the mutable accessors are the only
//! way to write a record, and each resets what the write could make stale. A record's cells
//! read that record alone, so [`Draft::face_mut`] and [`Draft::edge_mut`] reset the cells
//! of the record they hand out and nothing else, `add_face` and `add_edge` reset the
//! incoming record's, and an adjacency or vertex write resets none. A draft has no body
//! cells; `finish` starts the body's empty, seeded with the boxes of the body it was made
//! from where the edit left them standing.
//!
//! A draft made from a finished body also notes, at each first write to a record that body
//! had, which check instances read it, so `finish` checks only those and the records the
//! draft made (research 63 §6.14).

mod transform;

use std::collections::BTreeSet;
use std::sync::Arc;

use scx_curve::Curve;
use scx_num::extent1d::Extent1d;
use scx_num::frame::Sense;
use scx_num::units::Resolution;

use crate::body::BodyCells;
use crate::brep::Brep;
use crate::changes::{Changes, LineageError, Log, Operand, Origin, Tracked};
use crate::check;
use crate::document::Document;
use crate::error::{Absent, TableFull};
use crate::finish::Unfinished;
use crate::id::{EdgeId, Entity, FaceId, RegionId, ShellId, VertexId};
use crate::record::{
    EdgeAdj, EdgeGeom, EdgeShape, FaceRecord, Fin, RegionRecord, ShellRecord, VertexRecord,
};
use crate::tables::Tables;
use crate::touched::Touched;
use crate::view::{At, Drafting, Edge, Face, Region, Shell, Vertex};

pub use transform::TransformError;

/// A body being edited.
#[derive(Debug, Clone)]
pub struct Draft {
    t: Tables,
    /// Edges whose curve is not yet attached (63 A10): their stored shape is a placeholder
    /// no view shows.
    pending: BTreeSet<EdgeId>,
    /// What the edit has done, for [`Changes`].
    log: Log,
    /// What the draft touched of the body it was made from, for `finish`.
    touched: Touched,
}

impl Default for Draft {
    fn default() -> Self {
        Self::new()
    }
}

impl Draft {
    /// An empty draft: the outside region alone.
    #[must_use]
    pub fn new() -> Self {
        let t = Tables::empty();
        Self {
            log: Log::new(&t),
            t,
            pending: BTreeSet::new(),
            touched: Touched::whole(),
        }
    }

    /// A draft of the finished body `t`, stamped `resolution`, with its body cells.
    pub(crate) fn from_body(t: Tables, resolution: Resolution, cells: Arc<BodyCells>) -> Self {
        Self {
            log: Log::new(&t),
            touched: Touched::of(&t, resolution, cells),
            t,
            pending: BTreeSet::new(),
        }
    }

    fn at(&self) -> At<'_, Drafting<'_>> {
        At::drafting(&self.t, &self.pending)
    }

    /// Checks the draft and stamps it with `doc`'s resolution, and says what the edit
    /// changed since the draft was made.
    ///
    /// A draft made by [`Brep::edit`] or [`Brep::into_draft`] and finished under that
    /// body's resolution checks only the records it wrote or made and those whose rules read
    /// them, and refuses exactly when checking the whole body would, with the same cause.
    ///
    /// # Errors
    ///
    /// [`Unfinished`], holding the draft unchanged and the [`FinishCause`](crate::FinishCause).
    pub fn finish(self, doc: &Document) -> Result<(Brep, Changes), Box<Unfinished>> {
        let res = doc.resolution();
        if !self.touched.applies(res) {
            return self.finish_checking_all(doc);
        }
        match check::run_touched(&self.t, &self.pending, res, &self.touched) {
            Ok(dirty) => {
                let seed = self.touched.seed(dirty);
                let changes = self.log.changes(&self.t);
                Ok((Brep::from_tables_seeded(self.t, res, seed), changes))
            }
            Err(cause) => Err(Box::new(Unfinished { draft: self, cause })),
        }
    }

    /// [`finish`](Self::finish) checking the whole body, however the draft was made: what
    /// `finish` must agree with, for tests.
    ///
    /// # Errors
    ///
    /// As [`finish`](Self::finish).
    #[doc(hidden)]
    pub fn finish_checking_all(self, doc: &Document) -> Result<(Brep, Changes), Box<Unfinished>> {
        match check::run(&self.t, &self.pending, doc.resolution()) {
            Ok(()) => {
                let changes = self.log.changes(&self.t);
                Ok((Brep::from_tables(self.t, doc.resolution()), changes))
            }
            Err(cause) => Err(Box::new(Unfinished { draft: self, cause })),
        }
    }

    /// [`finish`](Self::finish) checking only the references (BR1's check), for the tests of
    /// `Changes`, whose edits are bookkeeping and need not leave a valid body.
    #[cfg(test)]
    pub(crate) fn finish_references_only(
        self,
        doc: Document,
    ) -> Result<(Brep, Changes), Box<Unfinished>> {
        let checked = match self.pending.first() {
            Some(&edge) => Err(crate::FinishCause::MissingCurve { edge }),
            None => check::references(&self.t).map_err(crate::FinishCause::Structure),
        };
        match checked {
            Ok(()) => {
                let changes = self.log.changes(&self.t);
                Ok((Brep::from_tables(self.t, doc.resolution()), changes))
            }
            Err(cause) => Err(Box::new(Unfinished { draft: self, cause })),
        }
    }

    // Adding.

    /// Adds a region, solid or void.
    ///
    /// # Errors
    ///
    /// [`TableFull`] when the region ids have run out.
    pub fn add_region(&mut self, solid: bool) -> Result<RegionId, TableFull> {
        self.t.regions.push(RegionRecord::new(solid))
    }

    /// Adds a shell.
    ///
    /// # Errors
    ///
    /// [`TableFull`] when the shell ids have run out.
    pub fn add_shell(&mut self, shell: ShellRecord) -> Result<ShellId, TableFull> {
        self.t.shells.push(shell)
    }

    /// Adds a vertex.
    ///
    /// # Errors
    ///
    /// [`TableFull`] when the vertex ids have run out.
    pub fn add_vertex(&mut self, vertex: VertexRecord) -> Result<VertexId, TableFull> {
        self.t.vertices.push(vertex)
    }

    /// Adds an edge with no fins; [`Draft::add_face`] adds them.
    ///
    /// # Errors
    ///
    /// [`TableFull`] when the edge ids have run out.
    pub fn add_edge(&mut self, mut edge: EdgeGeom) -> Result<EdgeId, TableFull> {
        edge.cells.reset();
        let id = self.t.edges.push(edge)?;
        // The two parts are in step, so this takes the same slot.
        self.t.adjacency.push(EdgeAdj::default())?;
        Ok(id)
    }

    /// Adds an edge from `ends[0]` to `ends[1]` whose curve comes later, from
    /// [`Draft::attach_curve`]. Until then its view's shape is `None`, and `finish` refuses
    /// the draft with [`FinishCause::MissingCurve`](crate::FinishCause::MissingCurve).
    ///
    /// # Errors
    ///
    /// [`TableFull`] when the edge ids have run out.
    pub fn add_edge_pending(&mut self, ends: [VertexId; 2]) -> Result<EdgeId, TableFull> {
        let id = self.add_edge(EdgeGeom::new(EdgeShape::Collapsed, ends))?;
        self.pending.insert(id);
        Ok(id)
    }

    /// Gives `edge` its curve over `interval`, whether or not it had one.
    ///
    /// # Errors
    ///
    /// [`Absent`] when the draft holds no such edge.
    pub fn attach_curve(
        &mut self,
        edge: EdgeId,
        curve: Curve,
        interval: Extent1d,
    ) -> Result<(), Absent> {
        let geom = self.edge_mut(edge).ok_or_else(|| Absent {
            entity: edge.into(),
        })?;
        geom.shape = EdgeShape::Curve { curve, interval };
        self.pending.remove(&edge);
        Ok(())
    }

    /// Adds a face, and appends a fin for each of its uses to its edge's radial order, loop
    /// by loop. Where an edge has more than two fins, the caller puts them in radial order
    /// with [`Draft::adjacency_mut`].
    ///
    /// # Errors
    ///
    /// [`TableFull`] when the face ids have run out.
    pub fn add_face(&mut self, mut face: FaceRecord) -> Result<FaceId, TableFull> {
        face.cells.reset();
        let fins: Vec<_> = face
            .loops
            .iter()
            .flat_map(|l| &l.uses)
            .map(|u| (u.edge, u.sense))
            .collect();
        let id = self.t.faces.push(Arc::new(face))?;
        for (edge, sense) in fins {
            self.touched.fins(&self.t, edge);
            if let Some(adj) = self.t.adjacency.get_mut(edge) {
                adj.fins.push(Fin::new(id, sense));
            }
        }
        Ok(id)
    }

    // Writing.

    /// The region's record, to write.
    pub fn region_mut(&mut self, id: RegionId) -> Option<&mut RegionRecord> {
        self.t.regions.get_mut(id)
    }

    /// The shell's record, to write.
    pub fn shell_mut(&mut self, id: ShellId) -> Option<&mut ShellRecord> {
        self.touched.shell(&self.t, id);
        self.t.shells.get_mut(id)
    }

    /// The face's record, to write: the only mutable path to it. Resets its cells, and
    /// copies the record first if it is shared.
    ///
    /// The fins do not follow a loop edit: a caller that adds, removes or re-senses a use
    /// writes its edges' fins with [`Draft::adjacency_mut`] to match.
    ///
    /// The first write to a face the draft started with keeps its record as it was, for
    /// [`Changes`]: after [`Brep::into_draft`] that write copies the record once.
    pub fn face_mut(&mut self, id: FaceId) -> Option<&mut FaceRecord> {
        self.touched.face(&self.t, id);
        let face = self.t.faces.get_mut(id)?;
        self.log.face_written(id, face);
        let face = Arc::make_mut(face);
        face.cells.reset();
        Some(face)
    }

    /// Edits the face's record with `edit`, as [`Draft::face_mut`], and then keeps its
    /// edges' fins in step with its uses: a use removed takes one fin of its sense off its
    /// edge, and a use added appends one. Where an edge has more than two fins, the caller
    /// puts them in radial order with [`Draft::adjacency_mut`]. `None` when the draft holds
    /// no such face.
    pub fn edit_face<R>(
        &mut self,
        id: FaceId,
        edit: impl FnOnce(&mut FaceRecord) -> R,
    ) -> Option<R> {
        let mut was = fin_keys(self.t.faces.get(id)?);
        let face = self.face_mut(id)?;
        let out = edit(face);
        let mut now = fin_keys(face);
        was.sort_unstable();
        now.sort_unstable();
        let (mut i, mut j) = (0, 0);
        loop {
            match (was.get(i), now.get(j)) {
                (Some(a), Some(b)) if a == b => {
                    i += 1;
                    j += 1;
                }
                (Some(&(edge, forward)), b) if b.is_none_or(|b| (edge, forward) < *b) => {
                    let fin = Fin::new(id, sense(forward));
                    self.touched.fins(&self.t, edge);
                    if let Some(adj) = self.t.adjacency.get_mut(edge)
                        && let Some(at) = adj.fins.iter().position(|f| *f == fin)
                    {
                        adj.fins.remove(at);
                    }
                    i += 1;
                }
                (_, Some(&(edge, forward))) => {
                    self.touched.fins(&self.t, edge);
                    if let Some(adj) = self.t.adjacency.get_mut(edge) {
                        adj.fins.push(Fin::new(id, sense(forward)));
                    }
                    j += 1;
                }
                (_, None) => break,
            }
        }
        Some(out)
    }

    /// The edge's geometry part, to write. Resets its cells; no face's cell reads it.
    pub fn edge_mut(&mut self, id: EdgeId) -> Option<&mut EdgeGeom> {
        self.touched.geom(&self.t, id);
        let geom = self.t.edges.get_mut(id)?;
        self.log.edge_written(id, geom);
        geom.cells.reset();
        Some(geom)
    }

    /// The edge's adjacency part, to write: its radial order. Resets no cell.
    pub fn adjacency_mut(&mut self, id: EdgeId) -> Option<&mut EdgeAdj> {
        self.touched.fins(&self.t, id);
        self.t.adjacency.get_mut(id)
    }

    /// The vertex's record, to write. Resets no cell: no record's cell reads a vertex.
    pub fn vertex_mut(&mut self, id: VertexId) -> Option<&mut VertexRecord> {
        self.log.vertex_written(id, self.t.vertices.get(id)?);
        self.touched.vertex(id);
        self.t.vertices.get_mut(id)
    }

    // Removing.

    /// Removes the region. The outside region cannot be removed: `None`.
    pub fn remove_region(&mut self, id: RegionId) -> Option<RegionRecord> {
        if id == RegionId::OUTSIDE {
            return None;
        }
        let region = self.t.regions.remove(id)?;
        self.touched.region_removed(id);
        Some(region)
    }

    /// Removes the shell.
    pub fn remove_shell(&mut self, id: ShellId) -> Option<ShellRecord> {
        let shell = self.t.shells.remove(id)?;
        self.touched.shell_removed(id);
        Some(shell)
    }

    /// Removes the face, and its fins from its edges.
    pub fn remove_face(&mut self, id: FaceId) -> Option<FaceRecord> {
        let face = self.drop_face(id)?;
        self.log.removed(id.into());
        Some(face)
    }

    /// Removes the face and its fins, recording nothing.
    fn drop_face(&mut self, id: FaceId) -> Option<FaceRecord> {
        self.touched.face(&self.t, id);
        let face = self.t.faces.remove(id)?;
        let mut edges: Vec<EdgeId> = face
            .loops
            .iter()
            .flat_map(|l| &l.uses)
            .map(|u| u.edge)
            .collect();
        edges.sort_unstable();
        edges.dedup();
        for e in edges {
            self.touched.fins(&self.t, e);
            if let Some(adj) = self.t.adjacency.get_mut(e) {
                adj.fins.retain(|f| f.face != id);
            }
        }
        Some(Arc::unwrap_or_clone(face))
    }

    /// Removes the edge, both parts.
    pub fn remove_edge(&mut self, id: EdgeId) -> Option<(EdgeGeom, EdgeAdj)> {
        let removed = self.drop_edge(id)?;
        self.log.removed(id.into());
        Some(removed)
    }

    fn drop_edge(&mut self, id: EdgeId) -> Option<(EdgeGeom, EdgeAdj)> {
        self.touched.edge_removed(&self.t, id);
        let geom = self.t.edges.remove(id)?;
        let adj = self.t.adjacency.remove(id).unwrap_or_default();
        self.pending.remove(&id);
        Some((geom, adj))
    }

    /// Removes the vertex.
    pub fn remove_vertex(&mut self, id: VertexId) -> Option<VertexRecord> {
        let vertex = self.t.vertices.remove(id)?;
        self.touched.vertex_removed(id);
        self.log.removed(id.into());
        Some(vertex)
    }

    // Lineage (01 §2.6): what the operation declares for `Changes`.

    /// Records that the new entity `new` came from `origin`; an entity may have several.
    /// An origin naming another new entity stands for that entity's origins as declared so
    /// far. A new entity with none declared is [`Origin::Free`].
    ///
    /// # Errors
    ///
    /// [`LineageError::NotCreated`] when `new` is not live or the draft started with it;
    /// [`LineageError::UnknownOrigin`] when `origin` names this body's entity that the draft
    /// neither started with nor holds, or a region or shell of it.
    pub fn derive<I: Tracked>(&mut self, new: I, origin: Origin) -> Result<(), LineageError> {
        let entity: Entity = new.into();
        if self.log.started_with(entity) || !self.holds(entity) {
            return Err(LineageError::NotCreated { entity });
        }
        let own = match origin {
            Origin::Offset(e) => Some(e),
            o => o.part_of(),
        };
        if let Some(e) = own
            && (Origin::of(e).is_none() || !(self.log.started_with(e) || self.holds(e)))
        {
            return Err(LineageError::UnknownOrigin { origin });
        }
        let origins = self.log.resolve(origin);
        self.log.derive(entity, origins);
        Ok(())
    }

    /// Records that `operand`'s `entity` lies on `onto`, which takes its place.
    ///
    /// # Errors
    ///
    /// [`Absent`] when the draft holds no `onto`.
    pub fn coincident<I: Tracked>(
        &mut self,
        operand: Operand,
        entity: I,
        onto: I,
    ) -> Result<(), Absent> {
        let onto: Entity = onto.into();
        if !self.holds(onto) {
            return Err(Absent { entity: onto });
        }
        self.log.coincident(operand, entity.into(), onto);
        Ok(())
    }

    /// Merges the face `absorbed` into `survivor`: removes it, and its fins, and gives it
    /// back. The caller makes `survivor`'s loops whole (with [`Draft::edit_face`]). `None`
    /// when either is not live, or they are one face.
    pub fn merge_face(&mut self, absorbed: FaceId, survivor: FaceId) -> Option<FaceRecord> {
        self.t.faces.get(survivor)?;
        if absorbed == survivor {
            return None;
        }
        let face = self.drop_face(absorbed)?;
        self.log.absorbed(absorbed.into(), survivor.into());
        Some(face)
    }

    /// Merges the edge `absorbed` into `survivor`: removes it, both parts, and gives it
    /// back. `None` when either is not live, or they are one edge.
    pub fn merge_edge(
        &mut self,
        absorbed: EdgeId,
        survivor: EdgeId,
    ) -> Option<(EdgeGeom, EdgeAdj)> {
        self.t.edges.get(survivor)?;
        if absorbed == survivor {
            return None;
        }
        let edge = self.drop_edge(absorbed)?;
        self.log.absorbed(absorbed.into(), survivor.into());
        Some(edge)
    }

    /// Merges the vertex `absorbed` into `survivor`: removes it and gives it back. `None`
    /// when either is not live, or they are one vertex.
    pub fn merge_vertex(&mut self, absorbed: VertexId, survivor: VertexId) -> Option<VertexRecord> {
        self.t.vertices.get(survivor)?;
        if absorbed == survivor {
            return None;
        }
        let vertex = self.t.vertices.remove(absorbed)?;
        self.touched.vertex_removed(absorbed);
        self.log.absorbed(absorbed.into(), survivor.into());
        Some(vertex)
    }

    /// Whether the draft holds `entity`.
    fn holds(&self, entity: Entity) -> bool {
        match entity {
            Entity::Region(r) => self.t.regions.get(r).is_some(),
            Entity::Shell(s) => self.t.shells.get(s).is_some(),
            Entity::Face(f) => self.t.faces.get(f).is_some(),
            Entity::Edge(e) => self.t.edges.get(e).is_some(),
            Entity::Vertex(v) => self.t.vertices.get(v).is_some(),
        }
    }

    // Reading: the same views as a `Brep`'s, at the draft stage.

    /// The region `id`.
    #[must_use]
    pub fn region(&self, id: RegionId) -> Option<Region<'_, Drafting<'_>>> {
        self.at().region(id)
    }

    /// The shell `id`.
    #[must_use]
    pub fn shell(&self, id: ShellId) -> Option<Shell<'_, Drafting<'_>>> {
        self.at().shell(id)
    }

    /// The face `id`.
    #[must_use]
    pub fn face(&self, id: FaceId) -> Option<Face<'_, Drafting<'_>>> {
        self.at().face(id)
    }

    /// The edge `id`.
    #[must_use]
    pub fn edge(&self, id: EdgeId) -> Option<Edge<'_, Drafting<'_>>> {
        self.at().edge(id)
    }

    /// The vertex `id`.
    #[must_use]
    pub fn vertex(&self, id: VertexId) -> Option<Vertex<'_, Drafting<'_>>> {
        self.at().vertex(id)
    }

    /// The live regions in id order.
    pub fn regions(
        &self,
    ) -> impl ExactSizeIterator<Item = Region<'_, Drafting<'_>>> + DoubleEndedIterator + Clone {
        self.at().regions()
    }

    /// The live shells in id order.
    pub fn shells(
        &self,
    ) -> impl ExactSizeIterator<Item = Shell<'_, Drafting<'_>>> + DoubleEndedIterator + Clone {
        self.at().shells()
    }

    /// The live faces in id order.
    pub fn faces(
        &self,
    ) -> impl ExactSizeIterator<Item = Face<'_, Drafting<'_>>> + DoubleEndedIterator + Clone {
        self.at().faces()
    }

    /// The live edges in id order.
    pub fn edges(
        &self,
    ) -> impl ExactSizeIterator<Item = Edge<'_, Drafting<'_>>> + DoubleEndedIterator + Clone {
        self.at().edges()
    }

    /// The live vertices in id order.
    pub fn vertices(
        &self,
    ) -> impl ExactSizeIterator<Item = Vertex<'_, Drafting<'_>>> + DoubleEndedIterator + Clone {
        self.at().vertices()
    }

    #[cfg(test)]
    pub(crate) const fn tables(&self) -> &Tables {
        &self.t
    }
}

/// The `(edge, sense)` of each of `face`'s uses: the fins it needs.
/// The sense as `Forward`, to sort by (`Sense` has no order).
fn fin_keys(face: &FaceRecord) -> Vec<(EdgeId, bool)> {
    face.loops
        .iter()
        .flat_map(|l| &l.uses)
        .map(|u| (u.edge, u.sense == Sense::Forward))
        .collect()
}

const fn sense(forward: bool) -> Sense {
    if forward {
        Sense::Forward
    } else {
        Sense::Reversed
    }
}
