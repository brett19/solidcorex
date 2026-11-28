// Written for solidcorex. Apache-2.0.
//! A sphere touching a cylinder or a ring torus off their axis (BO22; the table in
//! [`super`]): each surface is the set of points at its radius from a spine (the sphere's
//! centre, the cylinder's axis, the torus's centre circle), so the sphere meets the other
//! where its points stand at the other's radius from the other's spine.
//!
//! With `q` the spine's point nearest the centre `c` (unique off the axis) and `d` their
//! distance, the sphere's points stand between `d - r_s` and `d + r_s` from the spine, the
//! nearest only along `c - q`. So the sphere outside touches the other once where
//! `d = r_s + r` and the sphere inside (`r_s < r`) once where `d = r - r_s`, both at
//! `q + r (c - q) / d`; past either by more than the tolerance it is apart by more
//! (`Empty`), and a miss within the tolerance is the same touch (the depth rule of
//! [`super`]). Every other place is a crossing, a closed curve of degree four here
//! ([`Closed::General`]).

use scx_num::vector3d::Point3;
use scx_surface::Closed;

use super::Contact::Tangent;
use super::Intersection;
use super::clip::Clip;
use super::coaxial::{Form, Revolution};

/// A spine at a radius.
#[derive(Clone, Copy, Debug)]
enum Offset {
    /// A cylinder: points at `r` from its axis.
    Line { r: f64 },
    /// A ring torus: points at `r` from the circle of radius `big` about its axis.
    Circle { big: f64, r: f64 },
}

/// The sphere of `rs` about `c` against `other` (module docs): `None` when the pair is not
/// a sphere and a cylinder or ring torus, or the centre is on the other's axis (the
/// coaxial sections answer that).
pub(super) fn touch(
    ra: &Revolution,
    rb: &Revolution,
    clip: &Clip<'_>,
    tol: f64,
) -> Option<Closed<Intersection>> {
    let ((c, rs), other) = match (ra.form, rb.form) {
        (Form::Sphere { r }, _) => ((ra.axes.o, r), rb),
        (_, Form::Sphere { r }) => ((rb.axes.o, r), ra),
        _ => return None,
    };
    let offset = match other.form {
        Form::Cylinder { r } => Offset::Line { r },
        Form::Torus { big, small } if big > small => Offset::Circle { big, r: small },
        _ => return None,
    };
    let axes = other.axes;
    let [x, y, h] = axes.local(c);
    let rho = scx_num::fmath::hypot(x, y);
    // The centre's height on the axis, and its distance from it, rounded no worse than a
    // few steps of the coordinates' size.
    let size = (c - Point3::ORIGIN).length() + (axes.o - Point3::ORIGIN).length() + other.reach;
    let rounding = 64.0 * f64::EPSILON * size;
    if rho <= rounding {
        return None;
    }
    let (q, r) = match offset {
        Offset::Line { r } => (axes.point([0.0, 0.0, h]), r),
        Offset::Circle { big, r } => (axes.point([big * x / rho, big * y / rho, 0.0]), r),
    };
    let d = (c - q).length();
    if d <= rounding {
        return None;
    }
    let at = q + (c - q) * (r / d);
    let slack = tol + rounding;
    // Outside: `d - (rs + r)`; inside, a smaller sphere: `(r - rs) - d`.
    let outside = d - (rs + r);
    let inside = (rs < r).then_some(r - rs - d);
    let answer = if outside.abs() <= tol || inside.is_some_and(|e| e.abs() <= tol) {
        let mut out = Vec::new();
        clip.point(at, Tangent, &mut out);
        Intersection::pieces(Vec::new(), out)
    } else if outside > slack || inside.is_some_and(|e| e > slack) {
        Intersection::Empty
    } else {
        return Some(Closed::General);
    };
    Some(Closed::Solved(answer))
}
