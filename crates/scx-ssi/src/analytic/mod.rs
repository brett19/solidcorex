// Written for solidcorex. Apache-2.0.
//! The closed-form intersections (research 63 §5): the answer types, the pair dispatch and
//! the plane sections (63 SS5).
//!
//! # The answer
//!
//! [`Intersection`] is where two surfaces meet within their extents: [`Intersection::Empty`],
//! [`Intersection::Coincident`] (two planes, one surface over the overlap), or
//! [`Intersection::Pieces`], the curves and isolated points. Each [`IntersectionCurve`] is one
//! connected piece: an exact [`Curve`] kind (a line, a circle or an ellipse) and the span of
//! its parameter inside both extents. A curve runs along `n_a x n_b`, the first surface's
//! normal across the second's (each with its sense); where the surfaces are tangent along
//! it that product vanishes, and the direction is the construction's (fixed for a given
//! input).
//!
//! # The pairs
//!
//! [`intersect`] puts the pair in order (a plane second when there is one) and answers:
//!
//! - a plane against a plane, cylinder, cone, sphere or torus: [`section_by_plane`];
//! - two spheres, or two of cylinder, cone, sphere and torus on one axis: circles about the
//!   axis (the coaxial sections below);
//! - every other pair: [`Closed::General`] for now (63 SS2).
//!
//! # The plane sections
//!
//! | surface | solved | `General` |
//! |---|---|---|
//! | plane | a line, coincident or empty | never |
//! | cylinder | an ellipse, a circle, two lines or one tangent line, or empty | an ellipse too long for its precision |
//! | cone | an ellipse, a circle, two generators or one tangent generator, the apex, or empty | a parabola or hyperbola (the plane parallel to a generator or steeper, off the apex), an ellipse too long for its precision |
//! | sphere | a circle or a touch point, or empty | never |
//! | torus | one or two circles across the axis, two meridian circles along it, or empty | every other plane (a spiric section) |
//!
//! A section within the tolerance of a simpler one is that one: a cylinder or cone cut
//! nearly square to its axis is a circle when every point of the true ellipse lies within
//! the tolerance of it along the surface, a plane nearly along a cylinder's axis gives its
//! lines when they lie within the tolerance of the true ellipse over the extent's height,
//! and two lines or circles within the tolerance of each other are one tangent curve.
//!
//! Each curve is clipped to both extents: every side of an extent is a linear function of
//! the point (a plane's side, a revolution's half-plane at an angle, a height, a latitude's
//! height, a tube angle's half-line on a meridian), so its crossings are the roots of
//! `a cos t + b sin t + c` on a conic and of a linear function on a line. The spans between
//! crossings are kept where their middles lie in both extents within the tolerance, and
//! neighbours merge (across the turn's start on a conic). A piece no longer than the
//! tolerance is a point.
//!
//! # The coaxial sections
//!
//! Two surfaces of revolution on one axis meet where their meridians (the half-plane
//! sections, as functions of the distance from the axis and the height along it) meet: each
//! meeting point is a circle about the axis, or a touch point on it. Two spheres share the
//! line of their centres; a sphere shares any axis through its centre.
//!
//! | meridians | solved | `General` |
//! |---|---|---|
//! | two lines (cylinder, cone) | one circle, the surfaces coincident over their shared heights, or empty | a gap between the tolerance and the tolerance widened by the axes' offset |
//! | a line and a circle (sphere, torus) | one or two circles, a tangent circle, or empty | as above |
//! | two circles (spheres, tori) | one or two circles, a tangent circle, the surfaces coincident, or empty | as above |
//!
//! A crossing past an apex (a negative distance from the axis) is on the cone's other
//! nappe, outside any extent: the pair is empty when the apex is farther than the widened
//! tolerance from the other meridian, and `General` otherwise.
//!
//! The axes are one when the second's tilt and offset, carried over its reach, are within
//! the tolerance; past it the pair is [`Closed::General`], however far apart. Everything
//! else (the heights' overlap, a meridian's slack) widens the tolerance by that offset, so
//! an `Empty` is only answered where every point is apart by more. The circles are clipped
//! to both extents like the plane sections'.

mod clip;
mod coaxial;
mod section;

use section::Order;

use scx_curve::Curve;
use scx_num::extent1d::Extent1d;
use scx_num::extent2d::Extent2d;
use scx_num::units::{Accuracy, Length, Resolution, coincident};
use scx_num::vector3d::Point3;
use scx_surface::{Closed, Plane, Surface};

/// How the surfaces meet along a piece.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Contact {
    /// They cross.
    Transversal,
    /// They touch: a tangent line or circle, a sphere's touch point, a cone's apex.
    Tangent,
}

/// One connected piece of an intersection: a curve over a span of its parameter.
#[derive(Clone, Debug, PartialEq)]
pub struct IntersectionCurve {
    /// The curve, an exact kind (it holds no interval; the span is here).
    pub curve: Curve,
    /// The span of the curve's parameter inside both extents (a conic's may pass `2 pi`).
    pub span: Extent1d,
    /// How the surfaces meet along it.
    pub contact: Contact,
}

/// An isolated point of an intersection.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct IntersectionPoint {
    /// The point.
    pub point: Point3,
    /// How the surfaces meet there: `Tangent` for a touch, `Transversal` for a crossing
    /// curve whose piece inside the extents is no longer than the tolerance.
    pub contact: Contact,
}

/// Where two surfaces meet within their extents.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum Intersection {
    /// Along these curves and at these points (at least one of them).
    Pieces {
        /// The curves, in the order of construction.
        curves: Vec<IntersectionCurve>,
        /// The isolated points.
        points: Vec<IntersectionPoint>,
    },
    /// The surfaces are one surface, within the tolerance, over the overlap of these
    /// extents (on the first surface and on the second): two planes, two concentric spheres
    /// of one radius, or two surfaces of revolution on one axis with one meridian.
    Coincident {
        /// The first surface's extent.
        on_a: Extent2d,
        /// The second surface's extent.
        on_b: Extent2d,
    },
    /// Nowhere within the extents. Final: no other algorithm is asked.
    Empty,
}

impl Intersection {
    /// The pieces, or [`Intersection::Empty`] when there are none.
    fn pieces(curves: Vec<IntersectionCurve>, points: Vec<IntersectionPoint>) -> Self {
        if curves.is_empty() && points.is_empty() {
            Self::Empty
        } else {
            Self::Pieces { curves, points }
        }
    }

    /// The same intersection seen from the other surface: the extents swapped. The curves
    /// are built for the order asked, so nothing else changes here.
    fn swapped(self) -> Self {
        match self {
            Self::Coincident { on_a, on_b } => Self::Coincident {
                on_a: on_b,
                on_b: on_a,
            },
            other => other,
        }
    }
}

/// Where `a` over `ae` meets `b` over `be`, in closed form (module docs), the extents in
/// each surface's uv (research 63 D13). The tolerance is
/// [`coincident`]`(res, acc[0], acc[1])`: the distance within which two places on the
/// surfaces are one.
///
/// [`Closed::General`] when the pair has no closed form here: the general intersector
/// answers it.
#[must_use]
pub fn intersect(
    a: &Surface,
    ae: &Extent2d,
    b: &Surface,
    be: &Extent2d,
    acc: [Accuracy; 2],
    res: Resolution,
) -> Closed<Intersection> {
    let [acc_a, acc_b] = acc;
    let tol = coincident(res, acc_a, acc_b);
    match (a, b) {
        (_, Surface::Plane(p)) => section::section(a, ae, p, be, tol, Order::PlaneSecond),
        (Surface::Plane(p), _) => {
            section::section(b, be, p, ae, tol, Order::PlaneFirst).map(Intersection::swapped)
        }
        _ => coaxial::coaxial(a, ae, b, be, tol),
    }
}

/// The section of `s` over `se` by `plane` over `pe` (module docs), within `tol`: the
/// curves run along `n_s x n_plane`. [`Closed::General`] for a NURBS surface and the cases
/// in the module docs' table.
#[must_use]
pub fn section_by_plane(
    s: &Surface,
    se: &Extent2d,
    plane: &Plane,
    pe: &Extent2d,
    tol: Length,
) -> Closed<Intersection> {
    section::section(s, se, plane, pe, tol, Order::PlaneSecond)
}

#[cfg(test)]
mod tests;
