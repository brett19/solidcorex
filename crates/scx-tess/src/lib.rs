// Written for solidcorex. Apache-2.0.
//! Tessellation of a body (research 72 §6): a mesh whose shared edges are sampled once, as a
//! pure function of the edge, so a closed body gives a closed mesh, with a proved chord
//! bound rather than a target.
//!
//! Step 4 builds it in parts: the spec, the mesh, the errors and the edge sampling
//! ([`sample_edge`], [`wireframe`]) (TS1); planar faces by a constrained Delaunay
//! triangulation ([`tessellate`], [`tessellate_faces`]) (TS2); cylinder, cone, sphere and
//! torus faces triangulated in their own `uv` and refined to the chord (TS3).

mod cdt;
mod curved;
mod edge;
mod error;
mod face;
mod mesh;
mod spec;
mod wire;

pub use edge::{EdgeSamples, sample_edge};
pub use error::{TessDefect, TessError, TessRefusal, TessUnsupported};
pub use face::{tessellate, tessellate_faces};
pub use mesh::{EdgePolyline, Mesh};
pub use spec::TessSpec;
pub use wire::wireframe;
