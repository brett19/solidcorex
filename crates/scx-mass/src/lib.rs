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
//!   ([`Estimate`], [`MassErrors`]) with a first-order bound of its rounding added (the
//!   `rounding` module; sums are compensated).
//! - **Which faces.** The volume counts each face between a solid and a void region,
//!   signed so the solid is inside; the area every face not between two solids. A finished
//!   body's fins agree about their sides (`scx_brep`'s `SidesDisagree` check), so a solid
//!   region's boundary is always closed and no open-shell case arises.
//! - **Certified volumes.** [`enclose_volume`] and [`enclose_shell`] enclose a volume in
//!   proved bounds (interval Taylor series over each face's uv, strips over the gaps
//!   between trims and edges); [`volume_sign`] is a shell's certified sign and [`outward`]
//!   checks every shell against its region. See the `enclose` module.
//! - **The guard.** [`volume`] and [`mass`] keep the region sign, and read their estimate
//!   per shell: a shell whose sum contradicts its region by more than its error calls
//!   [`outward`], which refuses the body ([`MassRefusal::NotOutward`]) or, when it is
//!   outward after all, reports [`MassDefect::EstimateMissed`]. A valid body never pays
//!   for an enclosure.
//! - **Gap closure.** A use whose pcurve carries a recorded accuracy is closed onto its
//!   edge's curve by a strip and a corner fan (the `closure` module), so the faces bound one
//!   closed surface; a use with none costs nothing extra. The closure's integration error
//!   and the tube it cannot count are in the stated error.
//! - **Not yet.** A loop winding round a `v` period or `u` windings that do not cancel
//!   ([`MassUnsupported`]).
//!
//! Every per-piece evaluation is charged to the budget, the stop flag checked first.

mod body;
mod breaks;
mod closure;
mod edge;
mod enclose;
mod error;
mod face;
mod guard;
mod integrand;
mod kronrod;
mod patch;
mod rounding;
mod spec;
mod tseries;

pub use body::{area, face_area, mass, volume};
pub use edge::edge_length;
pub use enclose::{
    CELL_BUDGET, ORDER, Settle, Sign, VolumeEnclosure, enclose_shell, enclose_volume, outward,
    volume_sign,
};
pub use error::{MassDefect, MassError, MassRefusal, MassUnsupported};
pub use spec::{Estimate, Mass, MassErrors, MassSpec, RelAccuracy, Sym3};
