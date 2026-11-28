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
//! 5. **Imprint** (BO3). Into `a.edit()`: each operand edge cut at the merged points in
//!    it, each section a new edge, each face split by its cuts with exact pcurves, and
//!    `b`'s faces copied in sharing the new edges, with lineage from both operands. A
//!    loop that needs a fitted pcurve is [`BooleanUnsupported::Pcurve`].
//! 6. **Classify** (BO4). Each piece in, out of or on the other operand, one answer per
//!    set of pieces joined where the operands do not meet: seeded along each section by
//!    `scx_bodyq::sides_along`, the rest by `scx_bodyq::classify_point`; a piece of a
//!    coincident face is on it, with its material on the same side or the other.
//! 7. **Select and build** (BO4). The pieces the operation keeps (`b`'s turned over for a
//!    subtraction), merged where two of them are one face (pieces of one face, or coplanar
//!    faces of the two operands), edges and vertices no face needs removed, two edges on one
//!    curve joined, and each connected set of faces given its region and shells.
//!
//! **Reads only.** Both operands are `&Brep`; their box trees and boundary trees are filled
//! on first read by `scx-brep`, and the work charged does not depend on whether they were
//! warm.
//!
//! **Deterministic.** Faces, edges and vertices are visited in id order and every list is
//! sorted by ids before it is used, so the same operands give the same answer and work.

mod boolean;
mod build;
mod cells;
mod classify;
mod contact;
mod error;
mod faceface;
mod geom;
mod given;
mod imprint;
mod imprint_curves;
mod imprint_op;
mod intersect;
mod pairs;
mod pinch;
mod planar;
mod points;

pub use boolean::{BooleanOp, NoItems, boolean, combine};
pub use cells::{Faces, Membership, Rule};
pub use error::{BooleanDefect, BooleanError, BooleanRefusal, BooleanUnsupported};
pub use given::{Fact, Incidences, boolean_given};
pub use imprint_curves::{Cut, CutRefusal, CutRegion, CutUnsupported, Imprinted, imprint_curves};
pub use imprint_op::{Imprint, imprint};
pub use intersect::intersect;
pub use planar::planar;

#[cfg(test)]
mod tests;
