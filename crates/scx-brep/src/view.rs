// Written for solidcorex. Apache-2.0.
//! Views: borrowed handles on one entity of a [`Brep`](crate::Brep) or a
//! [`Draft`](crate::Draft), with its record and the way to its neighbours.
//!
//! A view is `Copy` and its methods take it by value, so an iterator it returns borrows the
//! body, not the view. The same views serve both, told apart by their [`Stage`]:
//!
//! - Over a finished body ([`Finished`]) a reference across records is the record:
//!   `finish` proved that every id resolves, and the body is immutable after.
//! - Over a draft ([`Drafting`]) it is an `Option`, since a draft may hold an id it has not
//!   yet filled, and [`Edge::shape`] is `None` for an edge whose curve is not yet attached
//!   (63 A10).
//!
//! Iteration is in id order over the live entities only, so a walk costs the live count,
//! not the slot count (research 66 §14). What is found by scanning, not stored, says so:
//! [`Vertex::edges`], [`Shell::faces`] and [`Region::shells`] walk a table, and an edge's
//! uses scan its faces' loops (C4). BR4's body cells replace the scans where one shows.

use core::fmt;
use std::collections::BTreeSet;

use scx_num::cert::Recorded;
use scx_num::frame::Sense;
use scx_num::vector3d::Point3;
use scx_surface::Surface;

use crate::id::{EdgeId, FaceId, RegionId, ShellId, VertexId};
use crate::record::{
    EdgeAdj, EdgeGeom, EdgeShape, EdgeUse, FaceRecord, FaceSide, Fin, LoopRecord, RegionRecord,
    ShellRecord, UseGeom, VertexRecord,
};
use crate::tables::Tables;

mod sealed {
    pub trait Sealed {}
    impl Sealed for super::Finished {}
    impl Sealed for super::Drafting<'_> {}
}

/// Whether the body under a view is finished or a draft; sealed.
pub trait Stage: Copy + sealed::Sealed {
    /// What a reference across records answers: `T` over a finished body, `Option<T>`
    /// over a draft.
    type Look<T>;

    #[doc(hidden)]
    fn look<T>(found: Option<T>) -> Self::Look<T>;

    #[doc(hidden)]
    fn is_pending(self, edge: EdgeId) -> bool;
}

/// The stage of a finished body: every reference resolves.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Finished;

impl Stage for Finished {
    type Look<T> = T;

    fn look<T>(found: Option<T>) -> T {
        match found {
            Some(found) => found,
            // `Draft::finish` checks every reference (`check::references`), and a `Brep`'s
            // tables are not written after.
            None => unreachable!("a finished body's references all resolve"),
        }
    }

    fn is_pending(self, _: EdgeId) -> bool {
        false
    }
}

/// The stage of a draft: a reference may name nothing yet.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Drafting<'a> {
    pending: &'a BTreeSet<EdgeId>,
}

impl Stage for Drafting<'_> {
    type Look<T> = Option<T>;

    fn look<T>(found: Option<T>) -> Option<T> {
        found
    }

    fn is_pending(self, edge: EdgeId) -> bool {
        self.pending.contains(&edge)
    }
}

/// A body's tables seen at a stage: where every view starts.
#[derive(Clone, Copy)]
pub(crate) struct At<'a, S> {
    t: &'a Tables,
    s: S,
}

impl<'a> At<'a, Finished> {
    pub(crate) const fn finished(t: &'a Tables) -> Self {
        Self { t, s: Finished }
    }
}

impl<'a> At<'a, Drafting<'a>> {
    pub(crate) const fn drafting(t: &'a Tables, pending: &'a BTreeSet<EdgeId>) -> Self {
        Self {
            t,
            s: Drafting { pending },
        }
    }
}

impl<'a, S: Stage> At<'a, S> {
    pub(crate) fn region(self, id: RegionId) -> Option<Region<'a, S>> {
        let rec = self.t.regions.get(id)?;
        Some(Region { at: self, id, rec })
    }

    pub(crate) fn shell(self, id: ShellId) -> Option<Shell<'a, S>> {
        let rec = self.t.shells.get(id)?;
        Some(Shell { at: self, id, rec })
    }

    pub(crate) fn face(self, id: FaceId) -> Option<Face<'a, S>> {
        let rec = self.t.faces.get(id)?;
        Some(Face { at: self, id, rec })
    }

    pub(crate) fn edge(self, id: EdgeId) -> Option<Edge<'a, S>> {
        let geom = self.t.edges.get(id)?;
        let adj = self.t.adjacency.get(id)?;
        Some(Edge {
            at: self,
            id,
            geom,
            adj,
        })
    }

    pub(crate) fn vertex(self, id: VertexId) -> Option<Vertex<'a, S>> {
        let rec = self.t.vertices.get(id)?;
        Some(Vertex { at: self, id, rec })
    }

    pub(crate) fn regions(
        self,
    ) -> impl ExactSizeIterator<Item = Region<'a, S>> + DoubleEndedIterator + Clone {
        self.t
            .regions
            .iter()
            .map(move |(id, rec)| Region { at: self, id, rec })
    }

    pub(crate) fn shells(
        self,
    ) -> impl ExactSizeIterator<Item = Shell<'a, S>> + DoubleEndedIterator + Clone {
        self.t
            .shells
            .iter()
            .map(move |(id, rec)| Shell { at: self, id, rec })
    }

    pub(crate) fn faces(
        self,
    ) -> impl ExactSizeIterator<Item = Face<'a, S>> + DoubleEndedIterator + Clone {
        self.t.faces.iter().map(move |(id, rec)| Face {
            at: self,
            id,
            rec: rec.as_ref(),
        })
    }

    pub(crate) fn edges(
        self,
    ) -> impl ExactSizeIterator<Item = Edge<'a, S>> + DoubleEndedIterator + Clone {
        // The two parts are in step slot for slot; `zip` walks both live lists together.
        self.t
            .edges
            .iter()
            .zip(self.t.adjacency.iter())
            .map(move |((id, geom), (_, adj))| Edge {
                at: self,
                id,
                geom,
                adj,
            })
    }

    pub(crate) fn vertices(
        self,
    ) -> impl ExactSizeIterator<Item = Vertex<'a, S>> + DoubleEndedIterator + Clone {
        self.t
            .vertices
            .iter()
            .map(move |(id, rec)| Vertex { at: self, id, rec })
    }

    /// The use a fin of `edge` stands for: the first use of `edge` in `fin.sense` in the
    /// face's loops.
    fn fin_use(self, edge: EdgeId, fin: Fin) -> Option<Use<'a, S>> {
        self.face(fin.face)?
            .uses()
            .find(|u| u.rec.edge == edge && u.rec.sense == fin.sense)
    }
}

/// A region.
#[derive(Clone, Copy)]
pub struct Region<'a, S: Stage = Finished> {
    at: At<'a, S>,
    id: RegionId,
    rec: &'a RegionRecord,
}

impl<'a, S: Stage> Region<'a, S> {
    /// The id.
    #[must_use]
    pub const fn id(self) -> RegionId {
        self.id
    }

    /// The record.
    #[must_use]
    pub const fn record(self) -> &'a RegionRecord {
        self.rec
    }

    /// Whether it is material.
    #[must_use]
    pub const fn is_solid(self) -> bool {
        self.rec.solid
    }

    /// Whether it is the infinite region outside everything.
    #[must_use]
    pub fn is_outside(self) -> bool {
        self.id == RegionId::OUTSIDE
    }

    /// The shells that bound it, in id order. Scans the shell table.
    pub fn shells(self) -> impl DoubleEndedIterator<Item = Shell<'a, S>> + Clone {
        let id = self.id;
        self.at.shells().filter(move |s| s.rec.region == id)
    }
}

/// A shell.
#[derive(Clone, Copy)]
pub struct Shell<'a, S: Stage = Finished> {
    at: At<'a, S>,
    id: ShellId,
    rec: &'a ShellRecord,
}

impl<'a, S: Stage> Shell<'a, S> {
    /// The id.
    #[must_use]
    pub const fn id(self) -> ShellId {
        self.id
    }

    /// The record.
    #[must_use]
    pub const fn record(self) -> &'a ShellRecord {
        self.rec
    }

    /// The region it bounds.
    pub fn region(self) -> S::Look<Region<'a, S>> {
        S::look(self.at.region(self.rec.region))
    }

    /// Its face sides, in face id order, front before back: a face with both sides in this
    /// shell (a sheet) comes twice. Scans the face table.
    pub fn faces(self) -> impl DoubleEndedIterator<Item = (Face<'a, S>, FaceSide)> + Clone {
        let id = self.id;
        self.at.faces().flat_map(move |f| {
            FaceSide::BOTH
                .into_iter()
                .filter(move |&side| f.rec.side(side) == id)
                .map(move |side| (f, side))
        })
    }

    /// Its wire edges, in the record's order.
    pub fn wires(
        self,
    ) -> impl ExactSizeIterator<Item = S::Look<Edge<'a, S>>> + DoubleEndedIterator + Clone {
        let at = self.at;
        self.rec.wires.iter().map(move |&e| S::look(at.edge(e)))
    }

    /// Its lone vertices, in the record's order.
    pub fn lone_vertices(
        self,
    ) -> impl ExactSizeIterator<Item = S::Look<Vertex<'a, S>>> + DoubleEndedIterator + Clone {
        let at = self.at;
        self.rec
            .lone_vertices
            .iter()
            .map(move |&v| S::look(at.vertex(v)))
    }
}

/// A face.
#[derive(Clone, Copy)]
pub struct Face<'a, S: Stage = Finished> {
    at: At<'a, S>,
    id: FaceId,
    rec: &'a FaceRecord,
}

impl<'a, S: Stage> Face<'a, S> {
    /// The id.
    #[must_use]
    pub const fn id(self) -> FaceId {
        self.id
    }

    /// The record.
    #[must_use]
    pub const fn record(self) -> &'a FaceRecord {
        self.rec
    }

    /// The surface.
    #[must_use]
    pub const fn surface(self) -> &'a Surface {
        &self.rec.surface
    }

    /// The shell on `side`.
    pub fn shell(self, side: FaceSide) -> S::Look<Shell<'a, S>> {
        S::look(self.at.shell(self.rec.side(side)))
    }

    /// The loops, outer first.
    pub fn loops(self) -> impl ExactSizeIterator<Item = Loop<'a, S>> + DoubleEndedIterator + Clone {
        self.rec
            .loops
            .iter()
            .enumerate()
            .map(move |(index, rec)| Loop {
                face: self,
                index,
                rec,
            })
    }

    /// Every use of every loop, loop by loop.
    pub fn uses(self) -> impl DoubleEndedIterator<Item = Use<'a, S>> + Clone {
        self.loops().flat_map(Loop::uses)
    }
}

/// A loop of a face.
#[derive(Clone, Copy)]
pub struct Loop<'a, S: Stage = Finished> {
    face: Face<'a, S>,
    index: usize,
    rec: &'a LoopRecord,
}

impl<'a, S: Stage> Loop<'a, S> {
    /// Its face.
    #[must_use]
    pub const fn face(self) -> Face<'a, S> {
        self.face
    }

    /// Its position in the face's loops: 0 is the outer loop when there is one.
    #[must_use]
    pub const fn index(self) -> usize {
        self.index
    }

    /// The record.
    #[must_use]
    pub const fn record(self) -> &'a LoopRecord {
        self.rec
    }

    /// How many uses.
    #[must_use]
    pub const fn len(self) -> usize {
        self.rec.uses.len()
    }

    /// Whether it has no use (never, in a finished body).
    #[must_use]
    pub const fn is_empty(self) -> bool {
        self.rec.uses.is_empty()
    }

    /// The uses, in loop order.
    pub fn uses(self) -> impl ExactSizeIterator<Item = Use<'a, S>> + DoubleEndedIterator + Clone {
        self.rec
            .uses
            .iter()
            .enumerate()
            .map(move |(position, rec)| Use {
                lp: self,
                position,
                rec,
            })
    }
}

/// One use of an edge in a face's loop.
#[derive(Clone, Copy)]
pub struct Use<'a, S: Stage = Finished> {
    lp: Loop<'a, S>,
    position: usize,
    rec: &'a EdgeUse,
}

impl<'a, S: Stage> Use<'a, S> {
    /// Its loop.
    #[must_use]
    pub const fn lp(self) -> Loop<'a, S> {
        self.lp
    }

    /// Its face.
    #[must_use]
    pub const fn face(self) -> Face<'a, S> {
        self.lp.face
    }

    /// Its position in the loop.
    #[must_use]
    pub const fn position(self) -> usize {
        self.position
    }

    /// The record.
    #[must_use]
    pub const fn record(self) -> &'a EdgeUse {
        self.rec
    }

    /// Whether the loop runs with the edge or against it.
    #[must_use]
    pub const fn sense(self) -> Sense {
        self.rec.sense
    }

    /// The image in the face's uv.
    #[must_use]
    pub const fn geom(self) -> &'a UseGeom {
        &self.rec.geom
    }

    /// The pcurve's recorded bound; `None` within the body's resolution.
    #[must_use]
    pub fn bound(self) -> Option<&'a Recorded> {
        self.rec.bound.as_deref()
    }

    /// Its fin in the edge's radial order.
    #[must_use]
    pub const fn fin(self) -> Fin {
        Fin::new(self.lp.face.id, self.rec.sense)
    }

    /// The edge.
    pub fn edge(self) -> S::Look<Edge<'a, S>> {
        S::look(self.lp.face.at.edge(self.rec.edge))
    }

    /// The vertex the use starts at, in its own sense.
    pub fn start(self) -> S::Look<Vertex<'a, S>> {
        let at = self.lp.face.at;
        let found = at.t.edges.get(self.rec.edge);
        S::look(found.and_then(|g| at.vertex(g.start(self.rec.sense))))
    }

    /// The vertex the use ends at, in its own sense.
    pub fn end(self) -> S::Look<Vertex<'a, S>> {
        let at = self.lp.face.at;
        let found = at.t.edges.get(self.rec.edge);
        S::look(found.and_then(|g| at.vertex(g.end(self.rec.sense))))
    }

    /// The next use round the loop.
    #[must_use]
    pub fn next(self) -> Self {
        self.step(1)
    }

    /// The previous use round the loop.
    #[must_use]
    pub fn prev(self) -> Self {
        self.step(self.lp.len().saturating_sub(1))
    }

    fn step(self, by: usize) -> Self {
        // The loop holds this use, so it is not empty.
        let position = (self.position + by) % self.lp.len().max(1);
        match self.lp.rec.uses.get(position) {
            Some(rec) => Self {
                lp: self.lp,
                position,
                rec,
            },
            None => self,
        }
    }
}

/// An edge: its geometry part and its adjacency part.
#[derive(Clone, Copy)]
pub struct Edge<'a, S: Stage = Finished> {
    at: At<'a, S>,
    id: EdgeId,
    geom: &'a EdgeGeom,
    adj: &'a EdgeAdj,
}

impl<'a, S: Stage> Edge<'a, S> {
    /// The id.
    #[must_use]
    pub const fn id(self) -> EdgeId {
        self.id
    }

    /// The geometry part.
    #[must_use]
    pub const fn geometry(self) -> &'a EdgeGeom {
        self.geom
    }

    /// The adjacency part.
    #[must_use]
    pub const fn adjacency(self) -> &'a EdgeAdj {
        self.adj
    }

    /// The shape: total over a finished body; `None` in a draft until the curve is attached.
    pub fn shape(self) -> S::Look<&'a EdgeShape> {
        S::look((!self.at.s.is_pending(self.id)).then_some(&self.geom.shape))
    }

    /// The start and end vertex ids.
    #[must_use]
    pub const fn ends(self) -> [VertexId; 2] {
        self.geom.ends
    }

    /// The start vertex.
    pub fn start(self) -> S::Look<Vertex<'a, S>> {
        S::look(self.at.vertex(self.geom.ends[0]))
    }

    /// The end vertex.
    pub fn end(self) -> S::Look<Vertex<'a, S>> {
        S::look(self.at.vertex(self.geom.ends[1]))
    }

    /// The recorded accuracy; `None` within the body's resolution.
    #[must_use]
    pub fn accuracy(self) -> Option<&'a Recorded> {
        self.geom.accuracy.as_deref()
    }

    /// Whether no face uses it.
    #[must_use]
    pub const fn is_wire(self) -> bool {
        self.adj.fins.is_empty()
    }

    /// The fins, in radial order.
    pub fn fins(self) -> impl ExactSizeIterator<Item = Fin> + DoubleEndedIterator + Clone {
        self.adj.fins.iter().copied()
    }

    /// The use behind each fin, in radial order: each found by scanning its face's loops.
    pub fn uses(
        self,
    ) -> impl ExactSizeIterator<Item = S::Look<Use<'a, S>>> + DoubleEndedIterator + Clone {
        let (at, id) = (self.at, self.id);
        self.adj
            .fins
            .iter()
            .map(move |&fin| S::look(at.fin_use(id, fin)))
    }
}

/// A vertex.
#[derive(Clone, Copy)]
pub struct Vertex<'a, S: Stage = Finished> {
    at: At<'a, S>,
    id: VertexId,
    rec: &'a VertexRecord,
}

impl<'a, S: Stage> Vertex<'a, S> {
    /// The id.
    #[must_use]
    pub const fn id(self) -> VertexId {
        self.id
    }

    /// The record.
    #[must_use]
    pub const fn record(self) -> &'a VertexRecord {
        self.rec
    }

    /// The point.
    #[must_use]
    pub const fn point(self) -> Point3 {
        self.rec.point
    }

    /// The recorded accuracy; `None` within the body's resolution.
    #[must_use]
    pub fn accuracy(self) -> Option<&'a Recorded> {
        self.rec.accuracy.as_deref()
    }

    /// The edges that start or end here, each once, in id order. Scans the edge table
    /// (vertex stars are a BR4 body cell).
    pub fn edges(self) -> impl DoubleEndedIterator<Item = Edge<'a, S>> + Clone {
        let id = self.id;
        self.at.edges().filter(move |e| e.geom.ends.contains(&id))
    }
}

macro_rules! debug_view {
    ($view:ident, $($field:ident),+) => {
        impl<S: Stage> fmt::Debug for $view<'_, S> {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.debug_struct(stringify!($view))
                    $(.field(stringify!($field), &self.$field))+
                    .finish()
            }
        }
    };
}

debug_view!(Region, id, rec);
debug_view!(Shell, id, rec);
debug_view!(Face, id, rec);
debug_view!(Edge, id, geom, adj);
debug_view!(Vertex, id, rec);

impl<S: Stage> fmt::Debug for Loop<'_, S> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Loop")
            .field("face", &self.face.id)
            .field("index", &self.index)
            .field("rec", self.rec)
            .finish()
    }
}

impl<S: Stage> fmt::Debug for Use<'_, S> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Use")
            .field("face", &self.lp.face.id)
            .field("loop", &self.lp.index)
            .field("position", &self.position)
            .field("rec", self.rec)
            .finish()
    }
}
