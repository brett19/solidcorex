// Written for solidcorex. Apache-2.0.
//! [`wireframe`]: a body's vertices and edge polylines, the part of its [`Mesh`] every face
//! shares.

use scx_brep::{Brep, EdgeId};
use scx_num::budget::{Budget, Stop};

use crate::edge::sample_edge;
use crate::error::{TessDefect, TessError, TessRefusal};
use crate::mesh::{EdgePolyline, Mesh};
use crate::spec::TessSpec;

/// The vertices and edge polylines of `body` under `spec`, with no triangles: the points a
/// face's triangulation must use on its boundary.
///
/// `positions` holds the live vertices in id order, then each live edge's samples
/// ([`sample_edge`]) in edge id order. Each polyline starts at its start vertex's index and
/// ends at its end vertex's, so a closed edge's starts and ends at one index.
///
/// # Errors
///
/// [`TessRefusal::Resolution`] for a body of another resolution than the spec's;
/// [`TessRefusal::TooFine`] when the points outgrow a `u32` index; otherwise as
/// [`sample_edge`].
pub fn wireframe(
    body: &Brep,
    spec: &TessSpec,
    budget: &mut Budget,
    stop: &Stop,
) -> Result<Mesh, TessError> {
    spec.check(body.resolution())?;
    let mut index: Vec<Option<u32>> = vec![None; body.vertex_slots()];
    let mut positions = Vec::with_capacity(body.vertices().len());
    for v in body.vertices() {
        // Vertex ids are `u32`, so a rank among the live ones fits too.
        let at = positions.len() as u32;
        if let Some(slot) = index.get_mut(v.id().index()) {
            *slot = Some(at);
        }
        positions.push(v.point());
    }
    let mut edges = Vec::with_capacity(body.edges().len());
    for e in body.edges() {
        let samples = sample_edge(e, spec, budget, stop)?;
        let id = e.id();
        let [start, end] = e.ends().map(|v| index.get(v.index()).copied().flatten());
        let (Some(start), Some(end)) = (start, end) else {
            return Err(TessError::Defect(TessDefect::Unindexed { edge: id }));
        };
        let base = positions.len();
        let after = base + samples.points().len();
        if u32::try_from(after).is_err() {
            return Err(too_fine(id, after));
        }
        let mut points = Vec::with_capacity(samples.points().len() + 2);
        points.push(start);
        points.extend((base..after).map(|i| i as u32));
        points.push(end);
        positions.extend_from_slice(samples.points());
        edges.push(EdgePolyline {
            edge: id,
            points,
            chord: samples.chord(),
        });
    }
    Ok(Mesh {
        positions,
        triangles: Vec::new(),
        normals: Vec::new(),
        face_of: Vec::new(),
        edges,
        chord_bound: Vec::new(),
    })
}

fn too_fine(edge: EdgeId, needed: usize) -> TessError {
    TessError::Refused(TessRefusal::TooFine {
        edge,
        needed: needed as f64,
    })
}
