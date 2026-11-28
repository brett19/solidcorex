// Written for solidcorex. Apache-2.0.
//! The plane geometry of a mid-sheet (research 74 §34, BW11): what lies inside a thin wall
//! (so a face no wider than the wall, its rim, is told from one that needs a sheet of its
//! own), a planar polygon's area, a point dropped onto a plane, and a point's coordinates in
//! a plane's frame and back.
//!
//! | pin (`surface::mid_sheet`) | here |
//! |---|---|
//! | `wall_holds`: `bool`, `MAX_POINTS` 1,000,000 | [`wall_holds`]: [`Holds`] naming the first point out; no cap (one pass over the caller's slice) |
//! | its own `Frame { origin, x, y }` without a checked orthonormality | [`scx_num::frame::Frame`], square and unit by construction |
//! | `in_frame`, `from_frame`: `[f64; 2]` | [`Point2`] |

use scx_num::frame::Frame;
use scx_num::units::Length;
use scx_num::vector2d::Point2;
use scx_num::vector3d::{Point3, Vec3};

use crate::error::{BodyqError, BodyqRefusal};
use crate::face_pair::OutwardPlane;
use crate::reach::{finite, finite_point, positive, unit3};

/// A thin wall, as two faces that face each other bound it.
#[derive(Debug, Clone, Copy, PartialEq)]
#[non_exhaustive]
pub enum Wall {
    /// A flat wall: the slab behind `face` (its outward normal points out of the wall),
    /// `thickness` deep.
    Flat {
        /// The wall's first face.
        face: OutwardPlane,
        /// The distance to the other face.
        thickness: Length,
    },
    /// A round wall: the shell between two radii about one axis.
    Round {
        /// A point on the axis.
        axis_point: Point3,
        /// The axis direction (any nonzero length).
        axis: Vec3,
        /// The inner face's radius.
        inner: Length,
        /// The outer face's radius (more than the inner).
        outer: Length,
    },
}

/// Whether points lie in a wall.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum Holds {
    /// Every point lies in the wall, within the slack (also for no points).
    Yes,
    /// The point at this index is the first that does not.
    No {
        /// Its index in the slice.
        point: usize,
    },
}

/// Whether every one of `points` lies in `wall`, within `slack`: no farther in front of the
/// first face or behind the second than `slack` (flat), or no nearer the axis than the inner
/// radius less `slack` nor farther than the outer radius and `slack` (round).
///
/// # Errors
///
/// Refused for a wall, point or slack that is not finite, a negative slack, a thickness or
/// radius not above zero, an outer radius no larger than the inner, or a normal or axis of
/// no length.
pub fn wall_holds(wall: &Wall, points: &[Point3], slack: Length) -> Result<Holds, BodyqError> {
    let slack = finite(slack.mm())?;
    if slack < 0.0 {
        return Err(BodyqError::Refused(BodyqRefusal::NotPositive));
    }
    for p in points {
        finite_point(*p)?;
    }
    match *wall {
        Wall::Flat { face, thickness } => {
            finite_point(face.point)?;
            let thickness = positive(thickness)?;
            let n = unit3(face.outward)?;
            // How deep behind the first face: 0 on it, the thickness on the other face.
            Ok(first_out(points, |p| {
                let depth = (face.point - p).dot(n);
                depth >= -slack && depth <= thickness + slack
            }))
        }
        Wall::Round {
            axis_point,
            axis,
            inner,
            outer,
        } => {
            finite_point(axis_point)?;
            let (inner, outer) = (positive(inner)?, positive(outer)?);
            if outer <= inner {
                return Err(BodyqError::Refused(BodyqRefusal::NotPositive));
            }
            let u = unit3(axis)?;
            Ok(first_out(points, |p| {
                let from = p - axis_point;
                let radial = (from - u * from.dot(u)).length();
                radial >= inner - slack && radial <= outer + slack
            }))
        }
    }
}

/// The first of `points` that is not `inside`.
fn first_out(points: &[Point3], inside: impl Fn(Point3) -> bool) -> Holds {
    points
        .iter()
        .position(|p| !inside(*p))
        .map_or(Holds::Yes, |point| Holds::No { point })
}

/// The area of the planar polygon through `corners` in order, in mm squared: the length of
/// half the sum of the cross products of consecutive corners, measured from the first corner
/// so the products follow the polygon's size and not its distance from the origin (for a
/// polygon that is not planar, the area of its projection onto its best plane). Zero for fewer
/// than three corners.
///
/// # Errors
///
/// Refused for a corner that is not finite.
pub fn polygon_area(corners: &[Point3]) -> Result<f64, BodyqError> {
    for c in corners {
        finite_point(*c)?;
    }
    let Some(&o) = corners.first() else {
        return Ok(0.0);
    };
    let next = corners.iter().cycle().skip(1);
    let sum = corners
        .iter()
        .zip(next)
        .fold(Vec3::ZERO, |s, (a, b)| s + (*a - o).cross(*b - o));
    Ok(sum.length() * 0.5)
}

/// `point` dropped onto `plane` along its normal.
///
/// # Errors
///
/// Refused for a point or plane that is not finite, or a normal of no length.
pub fn onto_plane(plane: &OutwardPlane, point: Point3) -> Result<Point3, BodyqError> {
    finite_point(point)?;
    finite_point(plane.point)?;
    let n = unit3(plane.outward)?;
    Ok(point - n * point.distance_along(plane.point, n))
}

/// `point`'s coordinates in `frame`: how far along each of its x and y axes it stands from the
/// origin (the point dropped onto the frame's plane, read in its axes).
///
/// # Errors
///
/// Refused for a point that is not finite.
pub fn in_frame(frame: &Frame, point: Point3) -> Result<Point2, BodyqError> {
    finite_point(point)?;
    Ok(Point2::new(
        point.distance_along(frame.origin(), *frame.x()),
        point.distance_along(frame.origin(), *frame.y()),
    ))
}

/// The point at coordinates `uv` of `frame`.
///
/// # Errors
///
/// Refused for coordinates that are not finite.
pub fn from_frame(frame: &Frame, uv: Point2) -> Result<Point3, BodyqError> {
    finite(uv.x)?;
    finite(uv.y)?;
    Ok(frame.origin() + *frame.x() * uv.x + *frame.y() * uv.y)
}
