// Written for solidcorex. Apache-2.0.
//! Fixed-order jets: a value and its derivatives to a compile-time order (research 63 X10,
//! ruling D3), shared by every curve and surface kind.

mod curve;
mod surface;

pub use curve::{CurveJet, CurveJet2};
pub use surface::SurfaceJet;
