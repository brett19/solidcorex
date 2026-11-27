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

use crate::brep::Brep;
use crate::changes::Changes;
use crate::check;
use crate::document::Document;
use crate::error::{Absent, TableFull};
use crate::finish::Unfinished;
use crate::id::{EdgeId, FaceId, RegionId, ShellId, VertexId};
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

    pub(crate) const fn from_tables(t: Tables) -> Self {
        Self {
            t,
            pending: BTreeSet::new(),
        }
    }

    fn at(&self) -> At<'_, Drafting<'_>> {
        At::drafting(&self.t, &self.pending)
    }

    /// Checks the draft and stamps it with `doc`'s resolution.
    ///
    /// # Errors
    ///
    /// [`Unfinished`], holding the draft unchanged and the [`FinishCause`](crate::FinishCause).
    pub fn finish(self, doc: &Document) -> Result<(Brep, Changes), Box<Unfinished>> {
        match check::run(&self.t, &self.pending, doc.resolution()) {
            Ok(()) => Ok((
                Brep::from_tables(self.t, doc.resolution()),
                Changes::default(),
            )),
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
    pub fn face_mut(&mut self, id: FaceId) -> Option<&mut FaceRecord> {
        let face = Arc::make_mut(self.t.faces.get_mut(id)?);
        face.cells.reset();
        Some(face)
    }

    /// The edge's geometry part, to write. Resets its cells and those of every face it has
    /// a fin on.
    pub fn edge_mut(&mut self, id: EdgeId) -> Option<&mut EdgeGeom> {
        self.t.edges.get(id)?;
        if let Some(adj) = self.t.adjacency.get(id) {
            reset_faces(&mut self.t.faces, adj);
        }
        let geom = self.t.edges.get_mut(id)?;
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
        self.t.vertices.get(id)?;
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
        let geom = self.t.edges.remove(id)?;
        let adj = self.t.adjacency.remove(id).unwrap_or_default();
        self.pending.remove(&id);
        Some((geom, adj))
    }

    /// Removes the vertex.
    pub fn remove_vertex(&mut self, id: VertexId) -> Option<VertexRecord> {
        self.t.vertices.remove(id)
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
