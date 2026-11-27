// Written for solidcorex. Apache-2.0.
//! Mass properties of a body by boundary integrals over its faces (research 72 §2): the
//! volume, the boundary's area, the first and second moments, one face's area and one
//! edge's length.
//!
//! - **Method.** The pin's: Green's theorem in each face's uv reduces the divergence
//!   theorem's surface integrals to integrals along the trims, taken by adaptive
//!   Gauss-Kronrod quadrature (the 7-point rule outside, the 15-point rule inside, the
//!   pin's nodes and weights). See the `face` module.
//! - **Reads only.** Each function takes the body (or a face, an edge), the accuracy,
//!   `&mut Scratch` for the face memo (research 72's MA3; not filled yet), the budget and
//!   the stop flag: the pieces of a `Cx` it reads, no more.
//! - **The face's own reference.** Each face is integrated about its box's middle rounded
//!   to a power-of-two grid, so its result depends on the face alone; it is moved to the
//!   body's reference by an exact binomial shift and summed in face id order. A result is
//!   deterministic: the same body gives the same bits.
//! - **Accuracy.** A [`RelAccuracy`] is requested, not proved: each piece's Kronrod-Gauss
//!   difference is held to a tenth of it, and every result returns the summed estimate
//!   ([`Estimate`], [`MassErrors`]).
//! - **Which faces.** The volume counts each face between a solid and a void region,
//!   signed so the solid is inside; the area every face not between two solids. A finished
//!   body's fins agree about their sides (`scx_brep`'s `SidesDisagree` check), so a solid
//!   region's boundary is always closed and no open-shell case arises.
//! - **Not yet.** Gap closure onto an edge's curve for a use with a recorded accuracy
//!   (MA2, at `face::join`); a loop winding round a `v` period or `u` windings that do not
//!   cancel ([`MassUnsupported`]).
//!
//! Every per-piece evaluation is charged to the budget, the stop flag checked first.

mod body;
mod breaks;
mod edge;
mod error;
mod face;
mod integrand;
mod kronrod;
mod spec;

pub use body::{area, face_area, mass, volume};
pub use edge::edge_length;
pub use error::{MassDefect, MassError, MassRefusal, MassUnsupported};
pub use spec::{Estimate, Mass, MassErrors, MassSpec, RelAccuracy, Sym3};
