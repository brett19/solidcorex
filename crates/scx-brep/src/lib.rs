// Written for solidcorex. Apache-2.0.
//! The boundary representation (01 §2): typed ids, records with their geometry by value, the
//! immutable [`Brep`] and the editable [`Draft`], and the views and iterators over both.
//!
//! - A body is regions, shells, faces, edges and vertices, each in a table by id
//!   ([`id`], [`record`]). A face is one `Arc` record holding its loops of uses (C4); an
//!   edge is a geometry part and an adjacency part, the radial order of its fins.
//! - Reads take `&Brep`; every write goes through a [`Draft`], and [`Draft::finish`] checks
//!   it and stamps its resolution (01 §2.3). Both are `Send + Sync`, and cloning either
//!   bumps reference counts.
//! - A view ([`Face`], [`Edge`], ...) is a borrowed handle on one entity. Over a finished
//!   body a reference across records is the record itself; over a draft it is an `Option`
//!   ([`Stage`]).
//!
//! - Every draft write is recorded: [`Draft::finish`] says what the edit changed
//!   ([`Changes`]), and [`carry`] takes an application's data across it (01 §2.6).
//!
//! Seams for the tasks that follow (research 63 §6.5): the structure and geometry checks
//! (BR2) go in `check` and [`finish`]; cells (BR4, 63 §6.9) in `cells`, `boundary`, `body`
//! and the reads in `query`; pcurves (BR5) and `transform` (BR6) build records through
//! [`Draft`].

mod body;
mod boundary;
mod brep;
mod cells;
mod changes;
mod check;
mod document;
mod draft;
pub mod error;
pub mod finish;
pub mod id;
mod query;
pub mod record;
mod table;
mod tables;
mod view;

pub use boundary::UvSide;
pub use brep::Brep;
pub use changes::{
    Carry, Changes, Coincidence, EntityChanges, LineageError, Merge, Operand, Origin, Origins,
    Split, Touch, Tracked, carry,
};
pub use document::Document;
pub use draft::Draft;
pub use error::{Absent, TableFull};
pub use finish::{FinishCause, GeometryCause, StructureCause, Unfinished};
pub use id::{EdgeId, Entity, EntityKind, FaceId, IdMap, RegionId, ShellId, VertexId};
pub use record::{
    EdgeAdj, EdgeGeom, EdgeShape, EdgeUse, FaceRecord, FaceSide, Fin, LoopRecord, RegionRecord,
    ShellRecord, UseGeom, VertexRecord,
};
pub use view::{Drafting, Edge, Face, Finished, Loop, Region, Shell, Stage, Use, Vertex};

// A body crosses threads whole (01 §2.3, §8): every public type is `Send + Sync`.
const _: () = {
    const fn ok<T: Send + Sync>() {}
    ok::<Brep>();
    ok::<Draft>();
    ok::<Unfinished>();
    ok::<FinishCause>();
    ok::<GeometryCause>();
    ok::<Changes>();
    ok::<EntityChanges<FaceId>>();
    ok::<Origins>();
    ok::<Split<EdgeId>>();
    ok::<Merge<VertexId>>();
    ok::<Coincidence<FaceId>>();
    ok::<LineageError>();
    ok::<Document>();
    ok::<IdMap>();
    ok::<FaceRecord>();
    ok::<EdgeGeom>();
    ok::<EdgeAdj>();
    ok::<VertexRecord>();
    ok::<ShellRecord>();
    ok::<RegionRecord>();
    ok::<Face<'static>>();
    ok::<Loop<'static>>();
    ok::<Use<'static>>();
    ok::<Edge<'static>>();
    ok::<Vertex<'static>>();
    ok::<Shell<'static>>();
    ok::<Region<'static>>();
    ok::<Face<'static, Drafting<'static>>>();
    ok::<Edge<'static, Drafting<'static>>>();
};

#[cfg(test)]
mod tests;
