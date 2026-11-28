// Written for solidcorex. Apache-2.0.
//! Constructors of bodies (research 72 §4, tasks MK1 and MK3): each takes a [`Frame`] and
//! sizes, or a planar region ([`extrude`]), and returns a [`Made`], the finished body with a
//! typed record of its faces by role.
//!
//! - **Built with the topology known.** A constructor writes every vertex, edge and face of
//!   its body into one [`Draft`](scx_brep::Draft), the shared edges made once and each
//!   face's loops given in the orientation its side needs, with both regions decided at
//!   construction. There is no stitching, no orientation pass and no ray fire (72 §1.1,
//!   S7).
//! - **Placed by the frame.** A body is built at its frame, so its vertices round once;
//!   there is no build-then-transform.
//! - **Exact pcurves.** Every use's pcurve is the closed form, written by
//!   [`Draft::exact_loops`](scx_brep::Draft::exact_loops) (BR5), with no recorded accuracy.
//! - **Checked by `finish`.** The structure and geometry checks of
//!   [`Draft::finish`](scx_brep::Draft::finish) run on the result; a refusal there is a
//!   [`Defect`] of the constructor (72 S8: the result contract is the caller's).
//! - **Sizes are validated by type.** Each is a [`Length`](scx_num::units::Length) that must
//!   be finite and above the document's resolution; otherwise [`Refusal::Size`].
//!
//! - **Seams and poles.** A periodic surface (the cylinder's, cone's, sphere's and torus's)
//!   is one face whose loop runs round a seam edge used twice in opposite senses; a sphere's
//!   poles and a cone's apex are collapsed edges.
//!
//! The pattern the next constructors reuse is the one in `block`: a table of roles over
//! vertex corners, the [`Assembly`](crate::assembly) that writes them, and a record struct
//! of [`FaceId`](scx_brep::FaceId)s by role.

mod assembly;
mod block;
mod cone;
mod cylinder;
mod error;
mod extrude;
mod prism;
mod sheet;
mod radial;
mod sphere;
mod torus;

pub use block::{BlockFaces, block};
pub use cone::{ConeFaces, cone};
pub use cylinder::{CylinderFaces, cylinder};
pub use error::{Defect, Dimension, MakeError, Refusal, Unsupported};
pub use extrude::{ExtrudeFaces, Span, extrude};
pub use prism::{PrismFaces, prism};
pub use radial::{RadialCone, RadialFaces, radial};
pub use sheet::{SheetFaces, sheet};
pub use sphere::{SphereFaces, sphere};
pub use torus::{TorusFaces, torus};

use scx_brep::Brep;

/// A body just made, with its faces by role.
///
/// `R` is the constructor's record ([`BlockFaces`] for [`block`]). A new body has no inputs,
/// so every entity in it is new and there is no `Changes` to give (72 S6).
#[derive(Debug, Clone)]
pub struct Made<R> {
    /// The finished body.
    pub body: Brep,
    /// Its faces, by role.
    pub faces: R,
}
