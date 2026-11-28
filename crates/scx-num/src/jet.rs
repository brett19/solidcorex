// Written for solidcorex. Apache-2.0.
//! Fixed-order jets: a value and its derivatives to a compile-time order (research 63 X10,
//! ruling D3), shared by every curve and surface kind; and [`Jet2`], a value and its first two
//! derivatives in one variable enclosed over an interval, with the chord bound built on it
//! ([`vector_bound`]: SU6's gap and FT3's refit).

mod curve;
mod interval;
mod surface;

pub use curve::{CurveJet, CurveJet2};
pub use interval::{Jet2, chord, norm, square, vector_bound};
pub use surface::SurfaceJet;
