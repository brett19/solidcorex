// Written for solidcorex. Apache-2.0.
//! The finished body (01 §2.3): immutable, shared, stamped with its resolution.
//!
//! A `Brep` is reached only through [`Draft::finish`], which proved every reference
//! resolves, so its views answer records where a draft's answer options. Cloning one bumps
//! reference counts; editing one starts a [`Draft`] that shares every table until it writes.

use std::sync::Arc;

use scx_num::extent3d::Extent3d;
use scx_num::units::Resolution;

use crate::body::{BodyCells, Seed};
use crate::draft::Draft;
use crate::id::{EdgeId, FaceId, IdMap, RegionId, ShellId, VertexId};
use crate::record::{EdgeAdj, EdgeGeom, FaceRecord, ShellRecord};
use crate::table::{Id, Table};
use crate::tables::Tables;
use crate::view::{At, Body, Edge, Face, Finished, Region, Shell, Stage, Vertex};

/// A finished body.
///
/// Beside its tables it keeps its body cells (research 63 §6.9): vertex stars, shells'
/// faces, boxes, filled on first read and shared by its clones. Reads take `&Brep`.
#[derive(Debug, Clone)]
pub struct Brep {
    t: Tables,
    resolution: Resolution,
    cells: Arc<BodyCells>,
}

impl Brep {
    pub(crate) fn from_tables(t: Tables, resolution: Resolution) -> Self {
        Self {
            t,
            resolution,
            cells: Arc::default(),
        }
    }

    /// A body finished from a draft of another, whose boxes start from `seed`.
    pub(crate) fn from_tables_seeded(
        t: Tables,
        resolution: Resolution,
        seed: Option<Seed>,
    ) -> Self {
        Self {
            t,
            resolution,
            cells: Arc::new(BodyCells::seeded(seed)),
        }
    }

    fn at(&self) -> At<'_, Finished> {
        At::finished(
            &self.t,
            Body {
                cells: &self.cells,
                resolution: self.resolution,
            },
        )
    }

    /// The resolution the body was finished under: an accuracy of `None` means within it.
    #[must_use]
    pub const fn resolution(&self) -> Resolution {
        self.resolution
    }

    /// A draft sharing every table with this body until it writes one.
    #[must_use]
    pub fn edit(&self) -> Draft {
        Draft::from_body(self.t.clone(), self.resolution, Arc::clone(&self.cells))
    }

    /// A draft of this body that writes in place wherever the body held the only reference.
    #[must_use]
    pub fn into_draft(self) -> Draft {
        Draft::from_body(self.t, self.resolution, self.cells)
    }

    /// The same body with its ids renumbered densely, keeping their order, and how. A body
    /// with no empty slot keeps every id. Never done implicitly (01 §2.2).
    #[must_use]
    pub fn compact(&self) -> (Self, IdMap) {
        let map = IdMap {
            regions: renumber(&self.t.regions),
            shells: renumber(&self.t.shells),
            faces: renumber(&self.t.faces),
            edges: renumber(&self.t.edges),
            vertices: renumber(&self.t.vertices),
        };
        if map.is_identity() {
            return (self.clone(), map);
        }
        let region = |r| Finished::look(map.region(r));
        let shell = |s| Finished::look(map.shell(s));
        let face = |f| Finished::look(map.face(f));
        let edge = |e| Finished::look(map.edge(e));
        let vertex = |v| Finished::look(map.vertex(v));
        let t = Tables {
            regions: rebuilt(&self.t.regions, Clone::clone),
            shells: rebuilt(&self.t.shells, |s: &ShellRecord| {
                let mut s = s.clone();
                s.region = region(s.region);
                s.wires.iter_mut().for_each(|e| *e = edge(*e));
                s.lone_vertices.iter_mut().for_each(|v| *v = vertex(*v));
                s
            }),
            faces: rebuilt(&self.t.faces, |f: &Arc<FaceRecord>| {
                let mut f = FaceRecord::clone(f);
                f.sides = f.sides.map(shell);
                for u in f.loops.iter_mut().flat_map(|l| &mut l.uses) {
                    u.edge = edge(u.edge);
                }
                Arc::new(f)
            }),
            edges: rebuilt(&self.t.edges, |g: &EdgeGeom| {
                let mut g = g.clone();
                g.ends = g.ends.map(vertex);
                g
            }),
            adjacency: rebuilt(&self.t.adjacency, |a: &EdgeAdj| {
                let mut a = a.clone();
                a.fins.iter_mut().for_each(|fin| fin.face = face(fin.face));
                a
            }),
            vertices: rebuilt(&self.t.vertices, Clone::clone),
        };
        (Self::from_tables(t, self.resolution), map)
    }

    /// The region `id`.
    #[must_use]
    pub fn region(&self, id: RegionId) -> Option<Region<'_>> {
        self.at().region(id)
    }

    /// The shell `id`.
    #[must_use]
    pub fn shell(&self, id: ShellId) -> Option<Shell<'_>> {
        self.at().shell(id)
    }

    /// The face `id`.
    #[must_use]
    pub fn face(&self, id: FaceId) -> Option<Face<'_>> {
        self.at().face(id)
    }

    /// The edge `id`.
    #[must_use]
    pub fn edge(&self, id: EdgeId) -> Option<Edge<'_>> {
        self.at().edge(id)
    }

    /// The vertex `id`.
    #[must_use]
    pub fn vertex(&self, id: VertexId) -> Option<Vertex<'_>> {
        self.at().vertex(id)
    }

    /// The outside region: always there.
    #[must_use]
    pub fn outside(&self) -> Region<'_> {
        Finished::look(self.at().region(RegionId::OUTSIDE))
    }

    /// The live regions in id order, the outside first.
    pub fn regions(
        &self,
    ) -> impl ExactSizeIterator<Item = Region<'_>> + DoubleEndedIterator + Clone {
        self.at().regions()
    }

    /// The live shells in id order.
    pub fn shells(&self) -> impl ExactSizeIterator<Item = Shell<'_>> + DoubleEndedIterator + Clone {
        self.at().shells()
    }

    /// The live faces in id order.
    pub fn faces(&self) -> impl ExactSizeIterator<Item = Face<'_>> + DoubleEndedIterator + Clone {
        self.at().faces()
    }

    /// The live edges in id order.
    pub fn edges(&self) -> impl ExactSizeIterator<Item = Edge<'_>> + DoubleEndedIterator + Clone {
        self.at().edges()
    }

    /// The live vertices in id order.
    pub fn vertices(
        &self,
    ) -> impl ExactSizeIterator<Item = Vertex<'_>> + DoubleEndedIterator + Clone {
        self.at().vertices()
    }

    /// A box holding the whole body (every face, edge and vertex, each grown by its
    /// accuracy and the resolution); `None` for an empty body or one that reaches infinity.
    /// Filled with every other box on first read.
    #[must_use]
    pub fn bounding_box(&self) -> Option<Extent3d> {
        self.cells.boxes(&self.t, self.resolution).body()
    }

    /// The faces whose box ([`Face::bounding_box`]) meets `b`, and every face with no box,
    /// in id order: every face with a point within the resolution of `b` is among them.
    /// Walks a box tree over the faces, built on first read.
    pub fn faces_meeting(&self, b: Extent3d) -> impl ExactSizeIterator<Item = Face<'_>> {
        let at = self.at();
        self.cells
            .boxes(&self.t, self.resolution)
            .faces_meeting(b)
            .into_iter()
            .map(move |f| Finished::look(at.face(f)))
    }

    /// Empties every cell: the body cells, and the cells of each record this body alone
    /// holds. What is shed is filled again on the next read that needs it.
    pub fn shed_cells(&mut self) {
        self.cells = Arc::default();
        for f in self.t.faces.unique_mut() {
            if let Some(f) = Arc::get_mut(f) {
                f.cells.reset();
            }
        }
        for g in self.t.edges.unique_mut() {
            g.cells.reset();
        }
    }

    #[cfg(test)]
    pub(crate) fn body_cells(&self) -> &BodyCells {
        &self.cells
    }

    /// One past the highest region id ever given, the outside's included.
    #[must_use]
    pub fn region_slots(&self) -> usize {
        self.t.regions.slots()
    }

    /// One past the highest shell id ever given.
    #[must_use]
    pub fn shell_slots(&self) -> usize {
        self.t.shells.slots()
    }

    /// One past the highest face id ever given: the length of a dense side table or bitset
    /// keyed by face id.
    #[must_use]
    pub fn face_slots(&self) -> usize {
        self.t.faces.slots()
    }

    /// One past the highest edge id ever given.
    #[must_use]
    pub fn edge_slots(&self) -> usize {
        self.t.edges.slots()
    }

    /// One past the highest vertex id ever given.
    #[must_use]
    pub fn vertex_slots(&self) -> usize {
        self.t.vertices.slots()
    }

    #[cfg(test)]
    pub(crate) const fn tables(&self) -> &Tables {
        &self.t
    }
}

/// Each slot's new number: live slots numbered in order, empty ones `None`.
fn renumber<I: Id, T>(table: &Table<I, T>) -> Vec<Option<u32>> {
    let mut map = vec![None; table.slots()];
    for (new, (old, _)) in table.iter().enumerate() {
        if let Some(slot) = map.get_mut(old.slot()) {
            *slot = Some(new as u32);
        }
    }
    map
}

/// The live records of `table`, each rewritten by `f`, in fresh dense slots.
fn rebuilt<I: Id, T: Clone>(table: &Table<I, T>, f: impl Fn(&T) -> T) -> Table<I, T> {
    let mut out = Table::default();
    for (_, record) in table.iter() {
        // Fewer slots than the source, which fit in a `u32`.
        let _ = out.push(f(record));
    }
    out
}
