// Written for solidcorex. Apache-2.0.
//! Stitching sheets into one body (74 §4.3, ST1).
//!
//! [`stitch`] gathers finished sheets into one body: sheet 0 edited in place, every other
//! sheet copied in, and the vertices of different sheets that lie within coincidence
//! identified as one vertex, by 04's merged-points row as its step-4 amendment rules (each
//! sheet's own clearance refused per operand; a chain anchored to its sheet's vertices).
//!
//! The edge stage (joining edges along, splitting an edge at another sheet's vertex) and
//! closing into a solid are not built yet: a stitch that needs either answers
//! [`StitchUnsupported::Edges`] or [`StitchUnsupported::Solid`] rather than leaving a join
//! undone. Everything is deterministic: the same sheets give the same bits, and the merged
//! points do not depend on the sheets' order.

mod error;
mod points;
mod report;
mod stitch;

pub use error::{StitchDefect, StitchError, StitchRefusal, StitchUnsupported};
pub use report::{Identified, NoItems, StitchReport, Stitched, Want};
pub use stitch::stitch;
