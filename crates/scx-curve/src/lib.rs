// Written for solidcorex. Apache-2.0.
//! solidcorex layer 2: the analytic curves as plain data (research 63 §3).
//!
//! - [`Line`] and [`Line2`]: `origin + t direction`, over every `t`;
//! - [`Circle`] and [`Circle2`]: `centre + r (cos t x + sin t y)`, `t` in radians from the
//!   frame's x axis, period `2 pi`;
//! - [`Ellipse`] and [`Ellipse2`]: `centre + a cos t x + b sin t y`, likewise;
//! - [`CurveError`]: why one cannot be built.
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
//!
//! Inversion, restriction, reversal and transformation build on these types (63 §3.5, CU2
//! to CU5).

mod conic;
mod error;
mod line;

pub use conic::{Circle, Circle2, Ellipse, Ellipse2};
pub use error::{CurveError, Degenerate, Part, SemiAxis};
pub use line::{Line, Line2};

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
    ok::<CurveError>();
};
