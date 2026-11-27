// Written for solidcorex. Apache-2.0.
//! solidcorex layer 4: surface-surface intersection (research 63 §5).
//!
//! - [`analytic`]: the closed forms. [`analytic::intersect`] takes any pair of
//!   [`Surface`](scx_surface::Surface)s over their extents and answers
//!   [`Closed::Solved`](scx_surface::Closed) with the exact curves, or
//!   [`Closed::General`](scx_surface::Closed) when the pair has no closed form here and the
//!   general intersector must answer. [`analytic::section_by_plane`] is the plane section of
//!   every analytic kind (63 SS5).
//!
//! A solved answer is final, an empty one included: there is no grid search behind it
//! (04, research 68). The general marching intersector joins in step 5.
//!
//! # Uv curves
//!
//! An answer carries the 3D curves only. The uv curve of a section piece on either surface
//! is `scx_surface::pcurve_of(&piece.curve, surface, piece.span, tol)` (63 D10, §4.11: the
//! curve's own parameter, an angle-direction uv kind where the surface's uv is an angle),
//! called by whoever needs it; nothing here builds one.

pub mod analytic;
