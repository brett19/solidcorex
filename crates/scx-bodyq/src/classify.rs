// Written for solidcorex. Apache-2.0.
//! [`classify_point`]: the three steps in order.

use scx_brep::{Brep, Entity, RegionId};
use scx_num::budget::{Budget, Stop};
use scx_num::vector3d::{Point3, Vec3};

use crate::error::{BodyqDefect, BodyqError, BodyqRefusal};
use crate::near::nearest_region;
use crate::on::on_entity;
use crate::rays::{Cast, RAYS, cast};

/// Where a point lies against a body.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum PointClass {
    /// Inside this region, away from the boundary: [`RegionId::OUTSIDE`] or one of the
    /// body's own.
    In(RegionId),
    /// On this face, edge or vertex, within coincidence of it.
    On(Entity),
}

/// Where `p` lies against `body`: in a region, or on a face, edge or vertex (see the
/// crate's note for the method).
///
/// The work is charged to `budget`, one unit for each inversion and two for each piece
/// of curve an intersection isolates, and `stop` is checked before each face; the same
/// body and point charge the same work every time.
///
/// # Errors
///
/// - [`BodyqError::Refused`] for a point with a coordinate that is not finite.
/// - [`BodyqError::Unsupported`] when a NURBS face or edge comes near the point or a ray
///   (step 5 builds their intersections), or when no ray is clean and the nearest boundary
///   point is one where more than two regions meet.
/// - [`BodyqError::Exhausted`] when `budget` runs out and [`BodyqError::Cancelled`] when
///   `stop` is set; neither leaves anything behind.
/// - [`BodyqError::Defect`] on a body whose geometry the method should take and cannot,
///   among them [`BodyqDefect::NoFaceBox`] for a body with a face that cannot be enclosed.
pub fn classify_point(
    body: &Brep,
    p: Point3,
    budget: &mut Budget,
    stop: &Stop,
) -> Result<PointClass, BodyqError> {
    classify_with(body, p, &RAYS, budget, stop)
}

/// [`classify_point`] with the rays `rays` tried in order before the fallback.
pub(crate) fn classify_with(
    body: &Brep,
    p: Point3,
    rays: &[Vec3],
    budget: &mut Budget,
    stop: &Stop,
) -> Result<PointClass, BodyqError> {
    if !(p.x.is_finite() && p.y.is_finite() && p.z.is_finite()) {
        return Err(BodyqError::Refused(BodyqRefusal::NotFinite));
    }
    stop.check()?;
    let outside = PointClass::In(body.outside().id());
    // A body with no box is empty, all outside, unless a face has no box: then no
    // answer can be proved (a face that cannot be enclosed is no evidence of absence).
    let Some(body_box) = body.bounding_box() else {
        return match body.faces().find(|f| f.bounding_box().is_none()) {
            Some(f) => Err(BodyqError::Defect(BodyqDefect::NoFaceBox { face: f.id() })),
            None => Ok(outside),
        };
    };
    // Every box is grown by its entity's accuracy and the resolution, so a point outside
    // the body's box is on nothing and in the outside region.
    if !body_box.contains_point(p, 0.0) {
        return Ok(outside);
    }
    if let Some(e) = on_entity(body, p, budget, stop)? {
        return Ok(PointClass::On(e));
    }
    // Each ray runs past the far side of the body's box.
    let len = body_box.max_distance_to_point(p) * 1.0625 + 1.0;
    for &d in rays {
        if let Cast::Clean(r) = cast(body, p, d, len, budget, stop)? {
            return Ok(PointClass::In(r));
        }
    }
    nearest_region(body, &body_box, p, budget, stop).map(PointClass::In)
}

/// What each ray alone tells, in order: for the determinism test.
#[cfg(test)]
pub(crate) fn each_ray(
    body: &Brep,
    p: Point3,
    budget: &mut Budget,
    stop: &Stop,
) -> Result<Vec<Option<RegionId>>, BodyqError> {
    let body_box = body.bounding_box().expect("a bounded body");
    let len = body_box.max_distance_to_point(p) * 1.0625 + 1.0;
    RAYS.iter()
        .map(|&d| {
            cast(body, p, d, len, budget, stop).map(|c| match c {
                Cast::Clean(r) => Some(r),
                Cast::Unclean => None,
            })
        })
        .collect()
}
