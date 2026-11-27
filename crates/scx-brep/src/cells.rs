// Written for solidcorex. Apache-2.0.
//! Cells: data derived from records and cached beside them (01 §3.2). Empty here; BR4
//! fills them (face, uv and body boxes, `FaceCell::fill`, boundary trees, the derived face
//! accuracy, the rectangular-trim flag).
//!
//! The rules BR1 already keeps, so that BR4 adds fields and nothing else:
//!
//! - [`Draft::face_mut`](crate::Draft::face_mut) is the only mutable path to a face record,
//!   and it resets the face's cells on every call.
//! - A write to an edge's geometry part, or to a vertex, resets the cells of every face that
//!   uses it, and a vertex write the cells of its edges as well.
//! - A write to an edge's adjacency part resets nothing.
//!
//! A cell never takes part in a comparison: two records with the same fields are equal
//! whatever their cells hold, since a cell is a function of the fields.

/// The cells of one face.
#[derive(Debug, Clone, Default)]
pub(crate) struct FaceCells {
    // BR4: `OnceLock` fields, filled by `FaceCell::fill(rec, &EdgeGeomTable, &VertexTable)`.
}

impl FaceCells {
    /// Whether no cell is filled: a reset would change nothing.
    #[expect(clippy::unused_self, reason = "BR4's cells read it")]
    pub(crate) const fn is_empty(&self) -> bool {
        true
    }

    /// Empties every cell.
    pub(crate) fn reset(&mut self) {
        *self = Self::default();
    }
}

impl PartialEq for FaceCells {
    fn eq(&self, _: &Self) -> bool {
        true
    }
}

/// The cells of one edge's geometry part.
#[derive(Debug, Clone, Default)]
pub(crate) struct EdgeCells {
    // BR4: the edge's box and the like.
}

impl EdgeCells {
    /// Whether no cell is filled.
    #[expect(clippy::unused_self, reason = "BR4's cells read it")]
    pub(crate) const fn is_empty(&self) -> bool {
        true
    }

    /// Empties every cell.
    pub(crate) fn reset(&mut self) {
        *self = Self::default();
    }
}

impl PartialEq for EdgeCells {
    fn eq(&self, _: &Self) -> bool {
        true
    }
}
