// Written for solidcorex. Apache-2.0.
//! solidcorex layer 2: planar profiles of lines and circular arcs (research 72 §4).
//!
//! - [`Segment`]: a line or an arc between two given points, or a whole circle (a loop
//!   on its own);
//! - [`Region::new`]: an outer loop and holes, validated once: closed within the
//!   resolution, no two segments crossing or touching away from a shared joint, holes
//!   inside the outer loop and apart; the outer loop is made counter-clockwise and the
//!   holes clockwise;
//! - [`Loop`]: a closed loop of a region, with its signed area;
//! - [`Region::classify`]: a point inside, outside or on the boundary, by winding number;
//! - [`PlanarError`]: the one cause a profile is refused for;
//! - [`polyline_crossings`], [`polyline_closest`], [`tangent_segment_circle`] and
//!   [`tangent_circles`]: the pin's 2-D helpers (research 74 §1.3, BW9);
//! - [`crossings`]: every meet of two bounded lines or conic arcs, with multiplicity, and
//!   their shared stretches, certified (research 74 §31).
//!
//! Every decision is exact or certified: lines by the exact orientation sign, arcs by
//! intervals rounded outward over the band of radii an arc's two ends allow. There is no
//! fixed epsilon: the one length is the caller's resolution.

mod cert;
mod contact;
mod cross;
pub mod error;
mod polyline;
mod region;
mod segment;
mod winding;

pub use cross::{CrossError, Crossings, Curve2, Meet, Overlap, Pending, Which, crossings};
pub use polyline::{
    PolylineClosest, PolylineCrossings, PolylineError, PolylineMeet, PolylineOverlap,
    polyline_closest, polyline_crossings, tangent_circles, tangent_segment_circle,
};
pub use error::{Degenerate, LoopId, PlanarError, SegmentRef};
pub use region::{Loop, Place, Region};
pub use scx_num::frame::Sense;
pub use segment::Segment;
