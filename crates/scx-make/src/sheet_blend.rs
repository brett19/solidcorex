// Derived from solidcore `solidcore/crates/solidcore-ops/src/blend_surface.rs` and the
// definition and frame builders of `solidcore/crates/solidcore/src/sweep/section_blend.rs`
// at commit 03e16c9ee766de5b9b1ced845e5149d62ff493b8.
// Upstream record of `blend_surface.rs` (verbatim): Surfaces between two sheets: a blend
// sheet between an edge of one sheet and an edge of another, each side held at its own
// continuity (G0 to G3), with the seam it makes on each side measured ([`blend_edges`]); a
// fillet where two sheets meet, round, chordal or curvature-continuous, the sheets cut back
// to it on request ([`fillet_sheets`]); and the same fillet between two sheets that cross, in
// the quadrant asked ([`fillet_crossing`]).
// Upstream record of `section_blend.rs` (verbatim): Upstream: none (not in SMLib).
// SolidCraft's `loft_blend` (`crates/kernel/src/loft_blend.rs` at 52afe21) and the Hermite
// rows of its `bridge::section`, moved into the kernel so no application builds the surface.
// Changes: one typed error for the sheet blends; the definition keeps its motions as affine
// maps and recertifies by rebuilding; frames read the uv from each use's pcurve, not by
// projection; the fillets between sheets are built exactly (a cylinder, torus or sphere from
// the two-dimensional fillet in the section) instead of through the kernel's sampled
// rolling-ball blend; a fillet the exact construction does not reach is a typed refusal
// naming what is missing; no station caps.

//! Sheets between sheets (research 75 §7, task MK12b): the section blend's definition record
//! and its frame builders, the blend between two sheet edges, and the round between two
//! sheets that share an edge or cross.
//!
//! - [`SectionBlendDefinition`] keeps what a [`section_blend`](crate::section_blend) was
//!   built from and the motions applied since, so the surface can be rebuilt and recertified.
//! - [`point_frames`], [`section_frames`], [`section_frames_outward`] and
//!   [`open_section_turns`] make the sections a blend takes: from points, or from a chain of
//!   a body's edges with the derivatives of the face the blend leaves.
//! - [`blend_edges`] is the blend between an edge of one sheet and an edge of another, a
//!   one-face NURBS sheet holding each side's order.
//! - [`fillet_sheets`] is the round between two sheets along the edge they share, and
//!   [`fillet_crossing`] between two sheets that cross, from the sides chosen.

mod crossing;
mod definition;
mod edges;
mod error;
mod fillet;
mod frames;
mod nurbs_face;
mod trim;

pub use crossing::{FilletCrossing, SheetSide, fillet_crossing};
pub use definition::SectionBlendDefinition;
pub use edges::{BlendSide, EdgeBlend, Seam, SeamGap, blend_edges};
pub use error::{SheetBlendError, SheetBlendRefusal, SheetBlendUnsupported};
pub use fillet::{Contact, Fillet, FilletSection, FilletSheets, Round, fillet_sheets};
pub use frames::{open_section_turns, point_frames, section_frames, section_frames_outward};

#[cfg(test)]
mod tests;
