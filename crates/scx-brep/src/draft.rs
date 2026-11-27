// Written for solidcorex. Apache-2.0.
//! The editable body (01 §2.3): every write goes through a [`Draft`], and
//! [`Draft::finish`] checks it and gives back an immutable [`Brep`].
//!
//! A draft made by [`Brep::edit`] shares every table with its body until it writes one;
//! [`Brep::into_draft`] writes in place where the body held the only reference.
//!
//! The cell rules (01 §3.2) live here: the mutable accessors are the only way to write a
//! record, and each resets what the write could make stale. A reset of a cell that holds
//! nothing is skipped, so an edge write does not copy the records of faces with no filled
//! cell.

use std::collections::BTreeSet;
use std::sync::Arc;

use scx_curve::Curve;
use scx_num::extent1d::Extent1d;
use scx_num::frame::Sense;

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
use crate::view::{At, Drafting, Edge, Face, Region, Shell, Vertex};

/// A body being edited.
#[derive(Debug, Clone)]
pub struct Draft {
    t: Tables,
    /// Edges whose curve is not yet attached (63 A10): their stored shape is a placeholder
    /// no view shows.
    pending: BTreeSet<EdgeId>,
    /// What the edit has done, for [`Changes`].
    log: Log,
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
        Self::from_tables(Tables::empty())
    }

    pub(crate) fn from_tables(t: Tables) -> Self {
        Self {
            log: Log::new(&t),
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
    /// # Errors
    ///
    /// [`Unfinished`], holding the draft unchanged and the [`FinishCause`](crate::FinishCause).
    pub fn finish(self, doc: &Document) -> Result<(Brep, Changes), Box<Unfinished>> {
        match check::run(&self.t, &self.pending) {
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
    pub fn add_edge(&mut self, edge: EdgeGeom) -> Result<EdgeId, TableFull> {
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
    pub fn add_face(&mut self, face: FaceRecord) -> Result<FaceId, TableFull> {
        let fins: Vec<_> = face
            .loops
            .iter()
            .flat_map(|l| &l.uses)
            .map(|u| (u.edge, u.sense))
            .collect();
        let id = self.t.faces.push(Arc::new(face))?;
        for (edge, sense) in fins {
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
                    if let Some(adj) = self.t.adjacency.get_mut(edge)
                        && let Some(at) = adj.fins.iter().position(|f| *f == fin)
                    {
                        adj.fins.remove(at);
                    }
                    i += 1;
                }
                (_, Some(&(edge, forward))) => {
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

    /// The edge's geometry part, to write. Resets its cells and those of every face it has
    /// a fin on.
    pub fn edge_mut(&mut self, id: EdgeId) -> Option<&mut EdgeGeom> {
        self.t.edges.get(id)?;
        if let Some(adj) = self.t.adjacency.get(id) {
            reset_faces(&mut self.t.faces, adj);
        }
        let geom = self.t.edges.get_mut(id)?;
        self.log.edge_written(id, geom);
        geom.cells.reset();
        Some(geom)
    }

    /// The edge's adjacency part, to write: its radial order. Resets no cell.
    pub fn adjacency_mut(&mut self, id: EdgeId) -> Option<&mut EdgeAdj> {
        self.t.adjacency.get_mut(id)
    }

    /// The vertex's record, to write. Resets the cells of every edge that starts or ends
    /// at it and of their faces. Scans the edge table.
    pub fn vertex_mut(&mut self, id: VertexId) -> Option<&mut VertexRecord> {
        self.log.vertex_written(id, self.t.vertices.get(id)?);
        let Tables {
            faces,
            edges,
            adjacency,
            ..
        } = &mut self.t;
        let star: Vec<(EdgeId, bool)> = edges
            .iter()
            .filter(|(_, g)| g.ends.contains(&id))
            .map(|(e, g)| (e, !g.cells.is_empty()))
            .collect();
        for (e, filled) in star {
            if let (true, Some(g)) = (filled, edges.get_mut(e)) {
                g.cells.reset();
            }
            if let Some(adj) = adjacency.get(e) {
                reset_faces(faces, adj);
            }
        }
        self.t.vertices.get_mut(id)
    }

    // Removing.

    /// Removes the region. The outside region cannot be removed: `None`.
    pub fn remove_region(&mut self, id: RegionId) -> Option<RegionRecord> {
        if id == RegionId::OUTSIDE {
            return None;
        }
        self.t.regions.remove(id)
    }

    /// Removes the shell.
    pub fn remove_shell(&mut self, id: ShellId) -> Option<ShellRecord> {
        self.t.shells.remove(id)
    }

    /// Removes the face, and its fins from its edges.
    pub fn remove_face(&mut self, id: FaceId) -> Option<FaceRecord> {
        let face = self.drop_face(id)?;
        self.log.removed(id.into());
        Some(face)
    }

    /// Removes the face and its fins, recording nothing.
    fn drop_face(&mut self, id: FaceId) -> Option<FaceRecord> {
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
        let geom = self.t.edges.remove(id)?;
        let adj = self.t.adjacency.remove(id).unwrap_or_default();
        self.pending.remove(&id);
        Some((geom, adj))
    }

    /// Removes the vertex.
    pub fn remove_vertex(&mut self, id: VertexId) -> Option<VertexRecord> {
        let vertex = self.t.vertices.remove(id)?;
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

/// Resets the cells of each face `adj` has a fin on, skipping faces with none filled.
fn reset_faces(faces: &mut crate::table::Table<FaceId, Arc<FaceRecord>>, adj: &EdgeAdj) {
    for fin in &adj.fins {
        if faces.get(fin.face).is_some_and(|f| !f.cells.is_empty())
            && let Some(face) = faces.get_mut(fin.face)
        {
            Arc::make_mut(face).cells.reset();
        }
    }
}
