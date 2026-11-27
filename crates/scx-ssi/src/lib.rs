// Written for solidcorex. Apache-2.0.
//! solidcorex layer 4: surface-surface intersection (research 63 §5, research 73).
//!
//! - [`analytic`]: the closed forms. [`analytic::intersect`] takes any pair of
//!   [`Surface`](scx_surface::Surface)s over their extents and answers
//!   [`Closed::Solved`](scx_surface::Closed) with the exact curves, or
//!   [`Closed::General`](scx_surface::Closed) when the pair has no closed form here and the
//!   general intersector must answer. [`analytic::section_by_plane`] is the plane section of
//!   every analytic kind (63 SS5).
//! - [`general`]: the general intersector (research 73, step 5), for the pairs the closed
//!   forms answer `General` for. [`general::intersect`] answers [`general::Traced`]
//!   (branches, nodes, touches) or a typed [`SsiError`].
//! - [`intersect`]: both in order, closed forms first.
//!
//! A solved answer is final, an empty one included: there is no grid search behind it
//! (04, research 68).
//!
//! # Uv curves
//!
//! A closed-form answer carries the 3D curves only. The uv curve of a section piece on
//! either surface is `scx_surface::pcurve_of(&piece.curve, surface, piece.span, tol)` (63
//! D10, §4.11: the curve's own parameter, an angle-direction uv kind where the surface's uv
//! is an angle), called by whoever needs it; nothing here builds one. A traced branch
//! carries its pcurves (73 §4.7b).

pub mod analytic;
mod error;
pub mod general;

pub use error::{SingularKind, SsiError};

use scx_num::budget::{Budget, Stop};
use scx_num::extent2d::Extent2d;
use scx_num::units::{Accuracy, Resolution};
use scx_surface::{Closed, Surface};

/// Where two surfaces meet within their extents: by a closed form, or traced by the
/// general method.
#[derive(Clone, Debug, PartialEq)]
pub enum Answer {
    /// The closed forms answered ([`analytic::intersect`]): exact curves and points.
    Closed(analytic::Intersection),
    /// The general method answered ([`general::intersect`]).
    Traced(general::Traced),
}

/// Where `a` over `ae` meets `b` over `be` (73 §4.11): [`analytic::intersect`] first, and
/// [`general::intersect`] when it answers `General`. The arguments are theirs.
///
/// The closed forms charge nothing to `budget` and do not poll `stop`; the general method
/// charges and polls as its docs say.
///
/// # Errors
///
/// [`general::intersect`]'s, for a pair with no closed form.
pub fn intersect(
    a: &Surface,
    ae: &Extent2d,
    b: &Surface,
    be: &Extent2d,
    acc: [Accuracy; 2],
    res: Resolution,
    budget: &mut Budget,
    stop: &Stop,
) -> Result<Answer, SsiError> {
    match analytic::intersect(a, ae, b, be, acc, res) {
        Closed::Solved(i) => Ok(Answer::Closed(i)),
        _ => general::intersect(a, ae, b, be, acc, res, budget, stop).map(Answer::Traced),
    }
}
