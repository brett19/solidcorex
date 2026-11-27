// Written for solidcorex. Apache-2.0.
//! The Boolean of two finished bodies: union, subtraction and intersection (research 72
//! §5, as ruled in §12.4 rows 3 and 4).
//!
//! [`boolean`] reads two `&Brep` of one document and returns a [`Done`](scx_brep::op::Done)
//! whose body is built in `a.edit()`: `a` is operand 0, `b` operand 1. There is no
//! tolerance rewriting: where the operands meet is judged within
//! [`coincident`](scx_num::units::coincident) of the document's resolution and the two
//! entities' recorded accuracies.
//!
//! **Stages** (72 §5.4), in order:
//!
//! 1. **Pairs.** The face pairs whose boxes meet, from each body's box tree.
//! 2. **Interferences.** Over the pairs, every place a vertex, edge or face of one operand
//!    meets one of the other within coincidence: vertex-vertex, vertex-edge,
//!    vertex-face, edge-edge and edge-face, and the stretches where an edge lies on a face.
//! 3. **Merged points.** The places that are within coincidence of each other are one
//!    point. An operand vertex among them is the point; two vertices of one operand among
//!    them are refused when within coincidence of each other and unsupported when only a
//!    chain joins them (the 11-27 merged-points row).
//! 4. **Face-face** (BO2). Each pair's surfaces intersected in closed form
//!    (`scx_ssi::analytic`), the curves cut at the merged points through both faces and
//!    kept where inside both; the stage-2 spans cut the same way and kept where inside the
//!    face; coincident pairs with their overlap and sense; isolated touch points. A pair
//!    with no closed form is [`BooleanUnsupported::SsiPair`].
//! 5. **Imprint** (BO3), **classify** and **select and build** (BO4): not built yet.
//!    [`boolean`] runs stages 1 to 4 and then answers [`BooleanUnsupported::Stage`].
//!
//! **Reads only.** Both operands are `&Brep`; their box trees and boundary trees are filled
//! on first read by `scx-brep`, and the work charged does not depend on whether they were
//! warm.
//!
//! **Deterministic.** Faces, edges and vertices are visited in id order and every list is
//! sorted by ids before it is used, so the same operands give the same answer and work.

mod boolean;
mod contact;
mod error;
mod faceface;
mod geom;
mod pairs;
mod points;

pub use boolean::{BooleanOp, NoItems, boolean};
pub use error::{BooleanDefect, BooleanError, BooleanRefusal, BooleanUnsupported, Stage};

#[cfg(test)]
mod tests;
