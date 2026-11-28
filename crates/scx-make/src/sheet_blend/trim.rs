// Written for solidcorex. Apache-2.0.
//! Cutting a sheet back: faces removed with the edges and vertices only they used.

use scx_brep::{Brep, Document, EdgeId, FaceId};

use super::error::SheetBlendError;
use crate::error::{Defect, MakeError};

/// `body` without the faces `gone`, nor the edges no other face uses, nor the vertices left
/// on no edge.
pub(super) fn keep_faces(
    body: &Brep,
    gone: &[FaceId],
    doc: Document,
) -> Result<Brep, SheetBlendError> {
    let orphan: Vec<EdgeId> = body
        .edges()
        .filter(|e| e.fins().all(|f| gone.contains(&f.face)))
        .map(scx_brep::Edge::id)
        .collect();
    let lone: Vec<_> = body
        .vertices()
        .filter(|v| v.edges().all(|e| orphan.contains(&e.id())))
        .map(scx_brep::Vertex::id)
        .collect();
    let mut draft = body.edit();
    for &f in gone {
        draft.remove_face(f);
    }
    for e in orphan {
        draft.remove_edge(e);
    }
    for v in lone {
        draft.remove_vertex(v);
    }
    match draft.finish(&doc) {
        Ok((b, _)) => Ok(b),
        Err(un) => Err(SheetBlendError::Make(MakeError::Defect(Defect::Finish(
            un.cause,
        )))),
    }
}
