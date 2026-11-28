// Written for solidcorex. Apache-2.0.
//! solidcorex layer 2: NURBS surfaces as plain data over `scx-nurbs`.
//!
//! - [`Net`]: a control net in one row-major buffer, row `i` the points of u index `i`;
//! - [`NurbsSurface`]: a validated tensor-product surface in `D` dimensions, polynomial or
//!   rational, on one [`KnotVector`](scx_nurbs::KnotVector) per direction ([`ParamAxis`]);
//! - [`NurbsSurfaceError`]: why a net or a surface cannot be built or evaluated;
//! - evaluation: [`NurbsSurface::point`], [`NurbsSurface::jet`] (the partials to a fixed
//!   total order, a [`scx_num::jet::SurfaceJet`]) and [`NurbsSurface::eval_many`];
//! - enclosures: [`NurbsSurface::partials_over`] and [`NurbsSurface::partials_over_piece`],
//!   every partial to a total order over a box of parameters as intervals, about the point
//!   [`About`] picks;
//! - [`iges`]: IGES entity 128 to and from a surface.
//!
//! The knot vectors, their degree cap ([`scx_nurbs::MAX_DEGREE`]), [`scx_nurbs::Approach`]
//! and the homogeneous storage of rational points are `scx-nurbs`'s; this crate adds the
//! tensor product and nothing parallel (research 63 §2.2). A surface that exists is valid:
//! every check runs once, in [`NurbsSurface::new`] or [`NurbsSurface::from_homogeneous`].
//!
//! - [`NurbsSurface::iso`], [`NurbsSurface::restrict`]: the exact iso-curve and the exact piece
//!   over a box of parameters;
//! - [`CompatibleSurfaces`]: surfaces sharing one knot vector and degree along an axis.
//!
//! The by-value edits (insertion, elevation, splitting, reversal, swapping, transformation)
//! and the enclosures build on these types (research 63 §2.5 and §2.6, NS3 and NS4).

mod compatible;
#[cfg(test)]
mod construct_tests;
mod edit;
mod enclose;
mod error;
mod eval;
pub mod iges;
mod iso;
mod net;
mod surface;

pub use compatible::CompatibleSurfaces;
pub use enclose::About;
pub use error::{EnclosureCause, NetCause, NurbsSurfaceError, RestrictCause, WeightCause};
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
    ok::<EnclosureCause>();
    ok::<About<3>>();
    ok::<CompatibleSurfaces<3>>();
    ok::<RestrictCause>();
};
