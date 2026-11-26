//! Research prototype for report 55: a certified upper bound on the gap between a UV trim
//! and its edge, on offset surfaces and on a surface known only through a net with a
//! certified distance from its definition (the procedural-blend case).
//!
//! std only, plus `scx-num`'s outward-rounded intervals. Nothing here is solidcore code.

pub mod cases;
pub mod counted;
pub mod curve;
pub mod eps;
pub mod gap;
pub mod jet;
pub mod surface;
