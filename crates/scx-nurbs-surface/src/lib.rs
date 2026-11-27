// Written for solidcorex. Apache-2.0.
//! solidcorex layer 2: NURBS surfaces as plain data over `scx-nurbs`.
//!
//! - [`Net`]: a control net in one row-major buffer, row `i` the points of u index `i`;
//! - [`NurbsSurface`]: a validated tensor-product surface in `D` dimensions, polynomial or
//!   rational, on one [`KnotVector`](scx_nurbs::KnotVector) per direction ([`ParamAxis`]);
//! - [`NurbsSurfaceError`]: why a net or a surface cannot be built.
//!
//! The knot vectors, their degree cap ([`scx_nurbs::MAX_DEGREE`]), [`scx_nurbs::Approach`]
//! and the homogeneous storage of rational points are `scx-nurbs`'s; this crate adds the
//! tensor product and nothing parallel (research 63 §2.2). A surface that exists is valid:
//! every check runs once, in [`NurbsSurface::new`] or [`NurbsSurface::from_homogeneous`].
//!
//! Evaluation, the by-value edits (insertion, elevation, splitting, reversal, swapping,
//! transformation), enclosures and IGES 128 build on these types (research 63 §2.5, NS2 to
//! NS5).

mod edit;
mod error;
mod net;
mod surface;

pub use error::{NetCause, NurbsSurfaceError, WeightCause};
pub use net::Net;
pub use surface::{NurbsSurface, ParamAxis};

/// `Result` with [`NurbsSurfaceError`].
pub type Result<T> = core::result::Result<T, NurbsSurfaceError>;

// Every public type is shareable across threads (57 §5.2).
const _: () = {
    const fn ok<T: Send + Sync>() {}
    ok::<Net<[f64; 3]>>();
    ok::<NurbsSurface<3>>();
    ok::<ParamAxis>();
    ok::<NurbsSurfaceError>();
    ok::<NetCause>();
    ok::<WeightCause>();
};
