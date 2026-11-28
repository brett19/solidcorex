// Written for solidcorex. Apache-2.0.
//! Where two mid-planes meet, and moving a mid-sheet's boundary edge onto that line
//! (research 74 §34, BW11): the geometry of a joint between two mid-surfaces. A mid-sheet is
//! the part of a wall's halfway plane that its two faces share; where it meets another wall's
//! sheet, its boundary edge there stands where the wall's faces end, not on the line the two
//! halfway planes meet in. Moving the edge onto that line, and meeting the edges beside it
//! again, extends the sheet at an inside corner (an L bracket) and trims it where it runs past
//! the other wall (a T junction).
//!
//! A sheet is a polygon of straight edges in one plane, as a list of 3-D corners in order.
//!
//! | pin (`surface::mid_joint`) | here |
//! |---|---|
//! | planes parallel within the kernel's half degree | parallel when the planes stay within coincidence over the [`Near`]'s reach |
//! | edge and neighbours "parallel" within the same angle | each over its own edge's length, with the caller's coincidence |
//! | the line's point nearest the origin, from offsets `a.point . ua` | from the first plane's own point: the same line, with no loss at a far origin |
//! | `Option` for every way of not meeting, `MAX_CORNERS` 100,000 | [`Meeting`] and [`Moved`] name each; no cap |
//! | skew lines `Err(InvalidInput)` past the zone | [`OffPlane`](BodyqRefusal::OffPlane) past coincidence |

use scx_num::units::Length;
use scx_num::vector3d::{Point3, Vec3};

use crate::error::{BodyqError, BodyqRefusal};
use crate::face_pair::OutwardPlane;
use crate::reach::{Near, finite, finite_point, unit3};

/// A line: a point on it and its unit direction.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Line3 {
    /// A point on the line.
    pub point: Point3,
    /// The line's unit direction.
    pub direction: Vec3,
}

/// Where two planes meet.
#[derive(Debug, Clone, Copy, PartialEq)]
#[non_exhaustive]
pub enum Meeting {
    /// The line they meet in.
    Line(Line3),
    /// They are parallel over the reach, whether they face the same way or opposite ways.
    Parallel,
    /// They are so near parallel that no point of their line can be placed within coincidence
    /// of both at this precision (or the point overflows).
    Unplaceable,
}

/// The line two planes meet in.
///
/// # Errors
///
/// Refused for a point or normal that is not finite, or a normal of no length.
pub fn planes_meeting(
    a: &OutwardPlane,
    b: &OutwardPlane,
    near: &Near,
) -> Result<Meeting, BodyqError> {
    finite_point(a.point)?;
    finite_point(b.point)?;
    let (ua, ub) = (unit3(a.outward)?, unit3(b.outward)?);
    let along = ua.cross(ub);
    let sine_squared = along.length_squared();
    if near.one_line(ua, ub) || sine_squared == 0.0 {
        return Ok(Meeting::Parallel);
    }
    // Measured from `a`'s own point: `(p - a.point) . ua = 0` and `(p - a.point) . ub = db`
    // make `p = a.point + (n x ua) db / |n|^2` with `n = ua x ub`. Built from `n` alone, not
    // from `1 - (ua . ub)^2`, which loses its digits as the planes near parallel.
    let db = b.point.distance_along(a.point, ub);
    let point = a.point + along.cross(ua) * (db / sine_squared);
    // A point that overflows (a NaN residual fails the test), or that rounding has moved off
    // either plane by more than coincidence, places no line.
    if !(point.is_finite()
        && near.within(point.distance_along(a.point, ua))
        && near.within(point.distance_along(b.point, ub)))
    {
        return Ok(Meeting::Unplaceable);
    }
    Ok(Meeting::Line(Line3 {
        point,
        direction: unit3(along)?,
    }))
}

/// What moving an edge of a polygon onto a line gave.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub enum Moved {
    /// The polygon with the edge's two corners slid along their neighbouring edges to the
    /// line: the sheet grows where the line lies outside the polygon and shrinks where it
    /// lies inside.
    Polygon(Vec<Point3>),
    /// The edge is not parallel to the line over its own length.
    EdgeNotParallel,
    /// A neighbouring edge (its index: from that corner to the next) is parallel to the line,
    /// so it meets it nowhere.
    NeighbourParallel {
        /// The neighbouring edge.
        edge: usize,
    },
}

/// The index after `i` among `n`, going round.
const fn next(i: usize, n: usize) -> usize {
    if i + 1 >= n { 0 } else { i + 1 }
}

/// The index before `i` among `n`, going round.
const fn before(i: usize, n: usize) -> usize {
    if i == 0 { n.saturating_sub(1) } else { i - 1 }
}

/// Where the line through `p` and `a` meets `target`, in their plane.
fn cross(
    (p, a): (Point3, Point3),
    target: (Point3, Vec3),
    coin: f64,
    edge: usize,
) -> Result<Result<Point3, Moved>, BodyqError> {
    let d = unit3(a - p)?;
    let len = p.distance(a);
    let (q, e) = target;
    let n = d.cross(e);
    let n2 = n.length_squared();
    if n.length() * len <= coin || n2 == 0.0 {
        return Ok(Err(Moved::NeighbourParallel { edge }));
    }
    let r = q - p;
    let (t, s) = (r.cross(e).dot(n) / n2, r.cross(d).dot(n) / n2);
    let (on_first, on_second) = (p + d * t, q + e * s);
    // Lines meant to cross in a plane do; skew ones are not this polygon's plane.
    if (on_first - on_second).length() > coin {
        return Err(BodyqError::Refused(BodyqRefusal::OffPlane));
    }
    Ok(Ok(on_first))
}

/// Moves edge `edge` of `polygon` (from corner `edge` to the corner after it) onto `line`, and
/// meets the edges beside it again there. `coincidence` is the distance within which two
/// places are one: it decides which edges are parallel (over their own length) and how far
/// the line may stand off the polygon's plane.
///
/// # Errors
///
/// Refused for fewer than three corners or an edge past them
/// ([`Corners`](BodyqRefusal::Corners)), a corner or line that is not finite, two corners at
/// one place where an edge's direction is needed, or a line not in the polygon's plane
/// ([`OffPlane`](BodyqRefusal::OffPlane): it does not meet a neighbouring edge within
/// coincidence).
pub fn move_edge_onto_line(
    polygon: &[Point3],
    edge: usize,
    line: &Line3,
    coincidence: Length,
) -> Result<Moved, BodyqError> {
    let n = polygon.len();
    if n < 3 || edge >= n {
        return Err(BodyqError::Refused(BodyqRefusal::Corners));
    }
    let coin = finite(coincidence.mm())?;
    for c in polygon {
        finite_point(*c)?;
    }
    finite_point(line.point)?;
    let along = unit3(line.direction)?;
    let corner = |i: usize| -> Result<Point3, BodyqError> {
        polygon
            .get(i)
            .copied()
            .ok_or(BodyqError::Refused(BodyqRefusal::Corners))
    };
    let (k, k1) = (edge, next(edge, n));
    let (a, b) = (corner(k)?, corner(k1)?);
    if unit3(b - a)?.cross(along).length() * a.distance(b) > coin {
        return Ok(Moved::EdgeNotParallel);
    }
    let (kp, kq) = (before(k, n), next(k1, n));
    let (p, q) = (corner(kp)?, corner(kq)?);
    let target = (line.point, along);
    let new_a = match cross((p, a), target, coin, kp)? {
        Ok(x) => x,
        Err(m) => return Ok(m),
    };
    let new_b = match cross((q, b), target, coin, k1)? {
        Ok(x) => x,
        Err(m) => return Ok(m),
    };
    Ok(Moved::Polygon(
        polygon
            .iter()
            .enumerate()
            .map(|(i, c)| {
                if i == k {
                    new_a
                } else if i == k1 {
                    new_b
                } else {
                    *c
                }
            })
            .collect(),
    ))
}
