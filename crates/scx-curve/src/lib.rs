// Written for solidcorex. Apache-2.0.
//! solidcorex layer 2: the analytic curves as plain data (research 63 §3).
//!
//! - [`Line`] and [`Line2`]: `origin + t direction`, over every `t`;
//! - [`Circle`] and [`Circle2`]: `centre + r (cos t x + sin t y)`, `t` in radians from the
//!   frame's x axis, period `2 pi`;
//! - [`Ellipse`] and [`Ellipse2`]: `centre + a cos t x + b sin t y`, likewise;
//! - [`Parabola`] and [`Parabola2`]: `vertex + F t^2 x + 2 F t y`, over every `t`;
//! - [`Hyperbola`] and [`Hyperbola2`]: the branch `centre + a cosh t x + b sinh t y`, over
//!   every `t`;
//! - [`AngleLine2`]: a line in angle and height in the NURBS-compatible uv, one coordinate
//!   the [`arcs::Arcs`] parameter of the angle (a circle's pcurve on a surface of revolution,
//!   63 §4.11);
//! - [`arcs`]: that uv's angle directions, shared with the surfaces (63 D13);
//! - [`Curve`] and [`Curve2`]: any of these or a NURBS curve, dispatched by `match`, with the batched
//!   [`Curve::eval_many`];
//! - [`Line::closest`], [`Circle::closest`], [`Ellipse::closest`], [`Parabola::closest`],
//!   [`Hyperbola::closest`] and their `invert` against a domain: the nearest points as values ([`Foot`], [`Closest`], [`Inversion`]);
//! - [`Curve::tangent_dir`] and [`Curve2::tangent_dir`]: the direction of travel, defined
//!   where the first derivative vanishes;
//! - edits by value: [`Curve::restrict`], [`Curve::reversed`] and [`Curve::transformed`]
//!   (with the parameter's [`ParamShift`]), and `restrict` and `reversed` on [`Curve2`];
//! - [`CurveError`]: why one cannot be built, restricted or transformed, or a query not
//!   answered.
//!
//! Every kind is validated once, when built, and is then plain data (`Clone`, `Send`,
//! `Sync`, `PartialEq`; 63 X3) with no cache (X4). Each has one evaluator (X12), generic
//! over the scalar ([`scx_num::ring::Ring`] for lines, [`scx_num::trig::Trig`] for conics):
//! `point_over` and `jet_over` in `f64` give values and in [`scx_num::interval::Ivl`]
//! enclosures; `point` and `jet` are their `f64` forms. A jet is fixed-order
//! ([`scx_num::jet::CurveJet`], X10) and takes the [`Approach`](scx_nurbs::Approach) side every
//! curve kind shares; an analytic kind is smooth, so the side changes nothing. Parameters
//! are the pin's (63 D13), angles in radians (X11). A conic's orientation is a
//! [`Sense`](scx_num::frame::Sense): reversed runs it clockwise about its frame's z (D14).

mod angle_line;
pub mod arcs;
mod conic;
mod curve;
mod edit;
mod error;
mod invert;
mod line;
mod open_conic;
mod tangent;

pub use angle_line::{AngleAxis, AngleLine2};
pub use conic::{Circle, Circle2, Ellipse, Ellipse2};
pub use curve::{Curve, Curve2};
pub use edit::ParamShift;
pub use error::{CurveError, Degenerate, Part, SemiAxis, Unsupported};
pub use invert::{Closest, Foot, Inversion, POLISH_STEPS};
pub use line::{Line, Line2};
pub use open_conic::{Hyperbola, Hyperbola2, Parabola, Parabola2};

/// `Result` with [`CurveError`].
pub type Result<T> = core::result::Result<T, CurveError>;

// Every public type is shareable across threads (57 §5.2).
const _: () = {
    const fn ok<T: Send + Sync>() {}
    ok::<Line>();
    ok::<Line2>();
    ok::<Circle>();
    ok::<Circle2>();
    ok::<Ellipse>();
    ok::<Ellipse2>();
    ok::<Parabola>();
    ok::<Parabola2>();
    ok::<Hyperbola>();
    ok::<Hyperbola2>();
    ok::<Curve>();
    ok::<Curve2>();
    ok::<AngleLine2>();
    ok::<arcs::Arcs>();
    ok::<CurveError>();
    ok::<Foot>();
    ok::<Inversion>();
    ok::<Closest>();
    ok::<ParamShift>();
};

#[cfg(test)]
mod truth;
