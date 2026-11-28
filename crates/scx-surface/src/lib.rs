// Written for solidcorex. Apache-2.0.
//! solidcorex layer 3: the analytic surface kinds, in closed form (research 63 §4).
//!
//! - [`Plane`], [`Cylinder`], [`Cone`], [`Sphere`] and [`Torus`]: small value types holding a
//!   [`Frame`](scx_num::frame::Frame), their sizes and (except the plane) a
//!   [`Sense`](scx_num::frame::Sense), with private fields; validated once when built, with a
//!   [`SurfaceError`] that has a kind. They carry no NURB (63 §4.2).
//! - Each has `point`, `eval` (any [`Ring`](scx_num::ring::Ring): `f64`, intervals, Taylor
//!   models), `eval_many`, `jet::<D>` (true partials, an [`Approach`](scx_nurbs::Approach) per
//!   direction at a break), `normal`, `domain`, and `to_step` / `from_step`. The cone and
//!   sphere have poles ([`Pole`]). The sphere is smooth at its poles, so every kind but the
//!   cone has a normal everywhere and returns it directly; the cone's is `None` at its apex.
//! - [`Surface`]: every kind as one value (the five, and a NURBS surface and a
//!   [`Procedural`] behind an `Arc`), dispatched by `match` with the kind's bits (63 §4.7).
//! - [`Procedural`]: a generator revolved ([`Revolution`]), extruded ([`Extrusion`]) or
//!   swept by a screw ([`Sweep`], [`Motion::Screw`], [`Screw`], [`RadialLaw`]), its
//!   [`Definition`] evaluated in closed form and its rational net built once, within a
//!   stated bound: the rounding, and for a sweep the certified gap of its net (research 75
//!   §13.27, §13.29).
//! - Inversion: each analytic kind's `closest` (a [`Closest`]: one foot or a set) and
//!   `invert` against a uv domain ([`Inversion`] of a [`Foot`]) are closed forms and the
//!   global nearest; [`Surface::invert`] adds a NURBS surface's, a certified search over its
//!   Bezier patches that takes a work budget and a stop flag (63 §4.2, research 66).
//! - Edits by value: `reversed` and `transformed` on each kind and on [`Surface`], with the
//!   [`UvMap`] from the original's parameters to the edit's, and [`Surface::restrict`]
//!   (63 SU7).
//! - [`Closed`]: a closed-form answer or [`Closed::General`] (research 63 D6), shared with
//!   `scx-ssi`.
//! - [`intersect_curve`]: where a line, circle or ellipse meets a kind, every point in
//!   the curve's `t` and the kind's uv, crossing or tangent at the tolerance, or the curve
//!   lying on it ([`CurveSurfaceHit`]); closed forms, a NURBS pair `Unsupported` (63 SU6).
//!   [`curve_on_surface`] is its answer for a curve stated to lie on the kind, read as
//!   whether it does ([`OnSurface`]), the roots skipped where a closed-form bound proves it.
//! - [`project_curve`] and [`drop_curve`]: a curve projected onto a kind along a direction,
//!   or dropped onto it square to it, in closed form, or a typed refusal naming the fitter
//!   ([`ProjectError::NeedsFitter`]) (research 75 SF1).
//! - [`pcurve_of`]: the exact pcurve of a curve lying on a surface, checked by lifting, or a
//!   typed [`PcurveError`] (63 D10, §4.11).
//! - [`certificate`]: proofs that a NURBS curve or surface is within a bound of a circle or
//!   an analytic kind, a curve's turn about an axis, and the loft's span bound, each a typed
//!   [`certificate::Proof`] or bound (research 75 §4, MK10).
//! - [`AnyCurve`]: a free [`Curve`](scx_curve::Curve), or a [`CurveOnSurface`] (a uv curve on
//!   a shared [`Surface`]) with its point and its jet by the chain rule (63 §4.10).
//!
//! # The uv (research 63 D13)
//!
//! The uv is the pin's NURB parameter, not STEP's angle, computed in closed form with no
//! NURB. The pin's canonical constructors (no `swap_uv`, no `inside_out`, a full turn) build
//! every angle direction from standard rational quarter arcs, one knot span each (research
//! 64 §2):
//!
//! | kind | `u` | `v` |
//! |---|---|---|
//! | plane | STEP's `u` | STEP's `v` |
//! | cylinder | four arcs over `[0, 2 pi r]` (arc length) | the height |
//! | cone | four arcs over `[0, 1]` | the height |
//! | sphere | four arcs over `[0, 1]` | latitude, two arcs over `[0, 1]` from the south pole |
//! | torus | four arcs over `[0, 1]` | four arcs over `[0, 1]` from the outer equator |
//!
//! On a quarter arc of parameter length `L` centred at `m`, with `tau = 2 tan(pi/8) (t - m) /
//! L`, the angle from the arc's middle is `2 atan(tau)`, and its cosine and sine are the
//! rational `(1 - tau^2, 2 tau) / (1 + tau^2)`. `to_step` and `from_step` map to STEP's
//! parameters (angles in radians) and back.
//!
//! The pin's other layouts (swapped or reversed directions, part turns, reparameterised
//! knots) are not kinds here; reading them is the reader's map, as STEP's is.

mod any_curve;
mod axial;
pub mod certificate;
mod chart_check;
mod closed;
mod common;
mod cone;
mod cylinder;
mod edit;
mod enclose;
mod error;
mod gap;
mod hull;
mod intersect;
mod invert;
mod meet;
mod nurbs_hits;
mod nurbs_invert;
mod pcurve;
mod plane;
mod procedural;
mod project;
mod recognize;
mod sphere;
mod surface;
mod to_nurbs;
mod torus;
mod trough;
mod volume_map;

pub use any_curve::{AnyCurve, CurveOnSurface};
pub use axial::{AxialError, AxialPiece, Holds, axial_section, holds_axis};
pub use closed::Closed;
pub use cone::Cone;
pub use cylinder::Cylinder;
pub use edit::UvMap;
pub use enclose::Along;
pub use error::{Size, SurfaceError};
pub use gap::{
    Aim, Bracket, GapError, Pending, Short, curve_curve_gap, curve_surface_gap, pcurve_edge_gap,
    point_curve_gap, vertex_curve_gap,
};
pub use hull::{Hull, Hulls};
pub use intersect::{Contact, CurveSurfaceHit, OnSurface, curve_on_surface, intersect_curve};
pub use invert::{Closest, Foot, Inversion};
pub use meet::{Meet, MeetError, points_meet};
pub use pcurve::{Check, CurveKind, PcurveError, SurfaceKind, pcurve_of};
pub use plane::Plane;
pub use procedural::{
    Definition, Extrusion, Motion, Procedural, ProceduralError, RadialLaw, Revolution, Screw, Sweep,
};
pub use project::{ProjectError, Projected, drop_curve, project_curve};
pub use recognize::{Recognized, Written, recognize, recognize_written};
pub use sphere::Sphere;
pub use surface::Surface;
pub use torus::Torus;
pub use trough::{Half, MARGIN, Trough, TroughError, facing_trough};
pub use volume_map::{Bend, Image, MapError, NotExact, Polar, Twist, Uv, VolumeMap};

use scx_num::vector3d::Point3;

/// `Result` with [`SurfaceError`].
pub type Result<T> = core::result::Result<T, SurfaceError>;

/// A pole: the iso line at `v` collapses to `point`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Pole {
    /// The `v` of the collapsed iso line.
    pub v: f64,
    /// Where it collapses.
    pub point: Point3,
}

// Every public type is shareable across threads (57 §5.2).
const _: () = {
    const fn ok<T: Send + Sync>() {}
    ok::<Plane>();
    ok::<PcurveError>();
    ok::<Cylinder>();
    ok::<Cone>();
    ok::<Sphere>();
    ok::<Torus>();
    ok::<Procedural>();
    ok::<ProceduralError>();
    ok::<Sweep>();
    ok::<Surface>();
    ok::<Recognized>();
    ok::<Written>();
    ok::<Pole>();
    ok::<CurveSurfaceHit>();
    ok::<UvMap>();
    ok::<SurfaceError>();
    ok::<Size>();
    ok::<Projected>();
    ok::<ProjectError>();
    ok::<scx_num::jet::SurfaceJet<scx_num::vector3d::Vec3, 2>>();
};

// The kinds stay small, so the `Surface` enum stays within 128 bytes (63 §4.3).
const _: () = {
    use core::mem::size_of;
    assert!(size_of::<Plane>() <= 120);
    assert!(size_of::<Cylinder>() <= 120);
    assert!(size_of::<Cone>() <= 120);
    assert!(size_of::<Sphere>() <= 120);
    assert!(size_of::<Torus>() <= 120);
};
