// Written for solidcorex. Apache-2.0.
//! The nearest point of a surface to a point, and its inversion against a uv domain (63 §4.2,
//! X8, D7; the curves' form, `scx_curve::invert`, in two parameters).
//!
//! - Each analytic kind's `closest(p)` is a [`Closest`]: one foot, or a whole set of equally
//!   near points. Every kind is a surface of revolution or a plane, so the nearest points lie
//!   in the query's meridian plane and are a closed form there: the projection (plane), the
//!   radial foot (cylinder, sphere), the foot on the generator of the query's half-plane or
//!   the apex (cone), and the foot on the near meridian circle (torus). Each is the global
//!   nearest over the kind's whole domain, not a local one: see each kind's `closest`.
//! - `invert(p, domain)` picks one foot and says whether it lies [`Inversion::Inside`] the
//!   domain, or [`Inversion::Outside`]; `None` is the kind's own domain. The foot is the
//!   kind's nearest point either way: an outside foot is not moved into the domain. A periodic
//!   parameter is wrapped by whole periods to start at the domain's min, and a domain that
//!   spans a whole period holds every value. Of a set of feet, the one at the domain's start
//!   in the free parameter.
//! - [`Surface::invert`](crate::Surface::invert) dispatches, with a work budget and a stop
//!   flag for the NURBS arm's search ([`crate::nurbs_invert`]).
//!
//! The uv is the kind's own (63 D13), read from the query's direction by
//! [`Arcs`](crate::arcs::Arcs)' inverse with no transcendental function, so a foot's `at`
//! and its `point` agree: `point` is the kind's `point(at)`. An analytic arm is a closed form:
//! it takes no budget and cannot fail. A non-finite query gives a foot with NaN parts, which
//! is never inside.

use scx_num::extent1d::Extent1d;
use scx_num::extent2d::Extent2d;
pub use scx_num::invert::{Foot, Inversion};
use scx_num::vector2d::Point2;
use scx_num::vector3d::Point3;

/// Every nearest point of an analytic surface to a query.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Closest {
    /// One nearest point.
    One(Foot<Point2>),
    /// Every `u` at this `v` is nearest: a circle of feet for a query on the axis of a
    /// cylinder, of a cone inside its opening, or of a torus; a pole (a sphere's, for a query
    /// on its axis; a cone's apex), where every `u` is the one point.
    AlongU {
        /// The `v` of the feet.
        v: f64,
        /// Their common distance.
        distance: f64,
    },
    /// Every `v` at this `u` is nearest: a query on a torus's tube centre circle.
    AlongV {
        /// The `u` of the feet.
        u: f64,
        /// Their common distance.
        distance: f64,
    },
    /// Every point is nearest: a query at a sphere's centre.
    Every {
        /// The common distance (the radius).
        distance: f64,
    },
}

impl Closest {
    /// The distance from the query to the surface.
    #[must_use]
    pub const fn distance(&self) -> f64 {
        match self {
            Self::One(f) => f.distance,
            Self::AlongU { distance, .. }
            | Self::AlongV { distance, .. }
            | Self::Every { distance } => *distance,
        }
    }
}

/// The periods of a kind's `u` and `v`, `None` for a parameter that is not periodic.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Periods {
    pub(crate) u: Option<f64>,
    pub(crate) v: Option<f64>,
}

impl Periods {
    pub(crate) const NONE: Self = Self { u: None, v: None };
}

/// The foot at `at`, whose point is `point`, for the query `p`.
pub(crate) fn foot(p: Point3, at: Point2, point: Point3) -> Foot<Point2> {
    Foot {
        at,
        point,
        distance: (p - point).length(),
    }
}

/// Picks one of `closest`'s feet and places it against `domain` (`None`: the kind's own
/// domain, which holds every foot `closest` gives): `foot_at` builds the foot at a parameter,
/// for the sets of feet.
pub(crate) fn invert(
    closest: Closest,
    periods: Periods,
    domain: Option<&Extent2d>,
    foot_at: impl Fn(Point2) -> Foot<Point2>,
) -> Inversion<Point2> {
    let start = domain.map_or(Point2::new(0.0, 0.0), Extent2d::min);
    let (pu, pv) = (periods.u, periods.v);
    let (du, dv) = (
        domain.map(Extent2d::x_interval),
        domain.map(Extent2d::y_interval),
    );
    let wrap = |t: f64, from: f64, period: Option<f64>| match (domain, period) {
        (Some(_), Some(p)) if t.is_finite() => from + (t - from).rem_euclid(p),
        _ => t,
    };
    let inside = |t: f64, ivl: Option<Extent1d>, period: Option<f64>| {
        if t.is_nan() {
            return false;
        }
        ivl.is_none_or(|e| {
            period.is_some_and(|p| e.length() >= p) || (e.min() <= t && t <= e.max())
        })
    };
    let (f, u_free, v_free) = match closest {
        Closest::One(f) => (f, false, false),
        Closest::AlongU { v, .. } => (
            foot_at(Point2::new(start.x, wrap(v, start.y, pv))),
            true,
            false,
        ),
        Closest::AlongV { u, .. } => (
            foot_at(Point2::new(wrap(u, start.x, pu), start.y)),
            false,
            true,
        ),
        Closest::Every { .. } => (foot_at(start), true, true),
    };
    let at = Point2::new(
        if u_free {
            f.at.x
        } else {
            wrap(f.at.x, start.x, pu)
        },
        if v_free {
            f.at.y
        } else {
            wrap(f.at.y, start.y, pv)
        },
    );
    // A wrapped parameter is a new point to evaluate: `point` stays the kind's `point(at)`.
    let f = if at == f.at { f } else { foot_at(at) };
    let ok_u = (u_free && !at.x.is_nan()) || inside(at.x, du, pu);
    let ok_v = (v_free && !at.y.is_nan()) || inside(at.y, dv, pv);
    if ok_u && ok_v {
        Inversion::Inside(f)
    } else {
        Inversion::Outside(f)
    }
}
