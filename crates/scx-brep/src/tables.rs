// Written for solidcorex. Apache-2.0.
//! The tables a body is made of, shared by a [`Brep`](crate::Brep) and its
//! [`Draft`](crate::Draft)s: cloning bumps six reference counts (01 §2.3).

use std::sync::Arc;

use crate::id::{EdgeId, FaceId, RegionId, ShellId, VertexId};
use crate::record::{EdgeAdj, EdgeGeom, FaceRecord, RegionRecord, ShellRecord, VertexRecord};
use crate::table::Table;

/// Every table of one body.
///
/// Faces are one `Arc` each, so that a write copies one face, not every face (research 51).
/// An edge's two parts are two tables in step, slot for slot, so that a write to the radial
/// order copies no geometry (01 §3.2).
#[derive(Debug, Clone)]
pub(crate) struct Tables {
    pub(crate) regions: Table<RegionId, RegionRecord>,
    pub(crate) shells: Table<ShellId, ShellRecord>,
    pub(crate) faces: Table<FaceId, Arc<FaceRecord>>,
    pub(crate) edges: Table<EdgeId, EdgeGeom>,
    pub(crate) adjacency: Table<EdgeId, EdgeAdj>,
    pub(crate) vertices: Table<VertexId, VertexRecord>,
}

impl Tables {
    /// The tables of an empty body: the outside region alone.
    pub(crate) fn empty() -> Self {
        Self {
            regions: Table::with_first(RegionRecord::new(false)),
            shells: Table::default(),
            faces: Table::default(),
            edges: Table::default(),
            adjacency: Table::default(),
            vertices: Table::default(),
        }
    }
}
