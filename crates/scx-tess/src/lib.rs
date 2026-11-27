// Written for solidcorex. Apache-2.0.
//! Tessellation of a body (research 72 §6): a mesh whose shared edges are sampled once, as a
//! pure function of the edge, so a closed body gives a closed mesh, with a proved chord
//! bound rather than a target.
//!
//! Step 4 builds it in parts. This part (TS1) is the spec, the mesh, the errors and the edge
//! sampling ([`sample_edge`], [`wireframe`]); face triangulation follows.

mod edge;
mod error;
mod mesh;
mod spec;
mod wire;

pub use edge::{EdgeSamples, sample_edge};
pub use error::{TessDefect, TessError, TessRefusal, TessUnsupported};
pub use mesh::{EdgePolyline, Mesh};
pub use spec::TessSpec;
pub use wire::wireframe;
