// Written for solidcorex. Apache-2.0.
//! The frame a profile wraps onto a cylindrical wall in, from a plane tangent to the wall
//! (research 74 §34, BW11).
//!
//! The wall's own normal points away from its axis outside (a convex wall) and towards it in a
//! bore. The wrapped profile's x then runs so that x, then the axis, turn about that normal:
//! the same handedness of a loop on either wall.
//!
//! | pin (`surface::cylinder_wall`) | here |
//! |---|---|
//! | `|n . axis| > 1e-8` leaves the plane "not parallel" | the plane is parallel to the axis when it leaves the axis within coincidence over the reach ([`Near`]) |
//! | touching within `10 tol` | within coincidence (the caller's `tol` was already the coincidence; the 10 is dropped) |
//! | `Option<WallFrame>` | [`Wrap`]: the two ways of not touching are named |
//! | arrays of `f64` | vectors, and the radius a length |

use scx_num::units::Length;
use scx_num::vector3d::Vec3;

use crate::error::BodyqError;
use crate::reach::{Near, positive, unit3};

/// The directions at the line where a plane touches a cylindrical wall.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct WallFrame {
    /// From the axis to the line of contact (unit).
    pub radial: Vec3,
    /// The direction a profile's x runs in on the wall (unit): the axis crossed with `radial`
    /// outside, `radial` crossed with the axis in a bore.
    pub tangent: Vec3,
}

/// Whether, and where, a plane touches a cylindrical wall.
#[derive(Debug, Clone, Copy, PartialEq)]
#[non_exhaustive]
pub enum Wrap {
    /// It touches along a line, and this is the wall's frame there.
    Frame(WallFrame),
    /// The plane is not parallel to the axis over the reach.
    NotParallel,
    /// The plane is parallel but not within coincidence of the wall (the wrong side of an
    /// outside wall, or clear of it).
    NotTouching,
}

/// The frame of the plane with `normal`, through the point `w` (the vector from a point of the
/// axis, direction `axis`, to a point of the plane), on the wall of radius `radius`. An
/// exterior wall (`convex`) needs the normal pointing away from the axis; a bore takes either
/// side. `near`'s reach is the wall's length along the axis.
///
/// # Errors
///
/// Refused for a vector that is not finite, a normal or axis of no length, or a radius not
/// above zero.
pub fn wall_frame(
    normal: Vec3,
    w: Vec3,
    axis: Vec3,
    radius: Length,
    convex: bool,
    near: &Near,
) -> Result<Wrap, BodyqError> {
    let (n, k) = (unit3(normal)?, unit3(axis)?);
    if !w.is_finite() {
        return Err(BodyqError::Refused(crate::error::BodyqRefusal::NotFinite));
    }
    let r = positive(radius)?;
    if !near.within(n.dot(k) * near.reach().mm()) {
        return Ok(Wrap::NotParallel);
    }
    let s = w.dot(n);
    let radial = if convex {
        if !near.within(s - r) {
            return Ok(Wrap::NotTouching);
        }
        n
    } else {
        if !near.within(s.abs() - r) {
            return Ok(Wrap::NotTouching);
        }
        if s < 0.0 { -n } else { n }
    };
    let radial = unit3(radial)?;
    let tangent = if convex {
        k.cross(radial)
    } else {
        radial.cross(k)
    };
    Ok(Wrap::Frame(WallFrame { radial, tangent }))
}
