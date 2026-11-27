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
//! - [`Surface`]: every kind as one value (the five and a NURBS surface behind an `Arc`),
//!   dispatched by `match` with the kind's bits (63 §4.7).
//! - Inversion: each analytic kind's `closest` (a [`Closest`]: one foot or a set) and
//!   `invert` against a uv domain ([`Inversion`] of a [`Foot`]) are closed forms and the
//!   global nearest; [`Surface::invert`] adds a NURBS surface's, a certified search over its
//!   Bezier patches that takes a work budget and a stop flag (63 §4.2, research 66).
//! - Edits by value: `reversed` and `transformed` on each kind and on [`Surface`], with the
//!   [`UvMap`] from the original's parameters to the edit's, and [`Surface::restrict`]
//!   (63 SU7).
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
mod arcs;
mod common;
mod cone;
mod cylinder;
mod edit;
mod error;
mod invert;
mod nurbs_invert;
mod plane;
mod sphere;
mod surface;
mod to_nurbs;
mod torus;

pub use any_curve::{AnyCurve, CurveOnSurface};
pub use cone::Cone;
pub use cylinder::Cylinder;
pub use edit::UvMap;
pub use error::{Size, SurfaceError};
pub use invert::{Closest, Foot, Inversion};
pub use plane::Plane;
pub use sphere::Sphere;
pub use surface::Surface;
pub use torus::Torus;

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
    ok::<Cylinder>();
    ok::<Cone>();
    ok::<Sphere>();
    ok::<Torus>();
    ok::<Surface>();
    ok::<Pole>();
    ok::<UvMap>();
    ok::<SurfaceError>();
    ok::<Size>();
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
