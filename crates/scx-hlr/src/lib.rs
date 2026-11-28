// Written for solidcorex. Apache-2.0.
//! Hidden-line removal on scx's own evaluators (research 74 §5, task HL1): views, scenes of
//! placed bodies, and every face's silhouette in closed form.
//!
//! - [`View`]: a direction of sight and the screen's axes.
//! - [`prepare`]: a [`Scene`] of bodies placed by rigid frames, its box from
//!   `scx_bodyq`'s tight boxes.
//! - [`silhouettes`] and [`Scene::silhouettes`]: each face's silhouette curves (exact for
//!   the plane, cylinder, cone, sphere and torus, and for a NURBS face recognised as one by
//!   `scx_surface::recognize`) with the part of each curve on the face, proved by its
//!   boundary crossings; a freeform face is named in [`Silhouettes::unsupported`].
//!
//! Tolerances are named in one module ([`PARALLEL`], [`ROUNDING`] and a face's crossing
//! tolerance); none of them decides what is kept, only where a curve may be split. Work is metered by the
//! context's budget, stopped by its flag and traced by its sink.
//!
//! - [`Scene::hidden_lines`] (research 74 HL2): every drawn edge and silhouette cut at all
//!   its image crossings ([`scx_planar::crossings`]) and its piercings of other bodies'
//!   faces, each span classified once as [`Visibility::Visible`] or
//!   [`Visibility::Hidden`] by its occluding face, [`Visibility::Uncertain`] only when the
//!   budget ran out, and [`Visibility::Pending`] where step 5 is waited on.

mod contour;
mod error;
mod project;
mod scene;
mod silhouette;
mod tolerance;
mod view;
mod visibility;

pub use contour::{Branch, TorusContour};
pub use error::{BadView, HlrError};
pub use scene::{Scene, prepare};
pub use silhouette::{
    Exactness, OnFace, Silhouette, SilhouetteCurve, Silhouettes, Undecided, Unsupported, Why,
    silhouettes,
};
pub use tolerance::{PARALLEL, ROUNDING};
pub use view::View;
pub use visibility::{Drawing, DrawnLine, Pending, Source, Span, Visibility};
