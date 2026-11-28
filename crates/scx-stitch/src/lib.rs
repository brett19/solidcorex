// Written for solidcorex. Apache-2.0.
//! Stitching sheets into one body (74 §4.3, ST1 to ST3).
//!
//! [`stitch`] gathers finished sheets into one body: sheet 0 edited in place, every other
//! sheet copied in, and the vertices of different sheets that lie within coincidence
//! identified as one vertex, by 04's merged-points row as its step-4 amendment rules (each
//! sheet's own clearance refused per operand; a chain anchored to its sheet's vertices).
//!
//! The edge stage then cuts each edge at the other sheets' vertices within coincidence of
//! its inside, and joins each pair of edges of two sheets with the same ends whose
//! certified symmetric Hausdorff distance is within coincidence (both directed gaps from
//! `scx_surface`, SU6). A pair no budget share decides, or an edge with two possible
//! partners, is left open in [`StitchReport::left`] ([`Left`]), never guessed. A join or cut
//! that is proved but cannot be made yet answers [`StitchUnsupported::Edges`] with its
//! [`EdgePending`].
//!
//! For [`Want::Solid`] the stitched sheets are then oriented (ST3): each closed, connected
//! set of faces is labelled by parity across its edges, its material side read from MA4's
//! certified volume sign, and closed sheets inside others made cavities by point
//! classification (analytic faces; NURBS waits on step 5's ray). An open edge is refused
//! ([`StitchRefusal::NotSolid`]), or unsupported when the edge stage left it undecided.
//! Everything is deterministic: the same sheets give the same bits, and the merged points
//! and kept curves do not depend on the sheets' order.

mod edges;
mod error;
mod merge;
mod orient;
mod points;
mod report;
mod single;
mod stitch;

pub use error::{
    EdgePending, NestingPending, SignPending, SolidFault, StitchDefect, StitchError, StitchRefusal,
    StitchUnsupported,
};
pub use merge::{MergeLeft, MergeReport, Merged, NotOneLine, NotOneSurface, merge_faces};
pub use report::{Identified, Joined, Left, NoItems, Split, StitchReport, Stitched, Want};
pub use single::{Sewn, gather, open_edges, sew};
pub use stitch::stitch;
