// Written for solidcorex. Apache-2.0.
//! The small closed forms the three steps share: coincidence, a face's foot, an edge's
//! foot, the ray against a box.

use scx_brep::{Brep, Edge, EdgeShape, Entity, Face, UvSide};
use scx_num::budget::{Budget, Stop};
use scx_num::cert::Recorded;
use scx_num::extent3d::Extent3d;
use scx_num::units::{Accuracy, coincident};
use scx_num::vector2d::Point2;
use scx_num::vector3d::{Point3, Vec3};
use scx_nurbs::Approach;

use crate::error::{BodyqDefect, BodyqError, BodyqUnsupported};

/// The pieces an analytic normal is read from: every analytic kind's is the same on both.
pub(crate) const ABOVE: [Approach; 2] = [Approach::Above, Approach::Above];

/// The coincidence distance of an entity with accuracy `acc` on `body`, in mm.
pub(crate) fn coincidence(body: &Brep, acc: Option<Accuracy>) -> f64 {
    coincident(
        body.resolution(),
        acc.unwrap_or(Accuracy::ZERO),
        Accuracy::ZERO,
    )
    .mm()
}

/// The accuracy of a recorded bound.
pub(crate) fn recorded(r: Option<&Recorded>) -> Option<Accuracy> {
    r.map(|r| r.acc)
}

/// The nearest point of a face's surface to `p`, with its uv placed in the face's uv box,
/// and where that uv lies against the face (`None` when the face's test cannot tell).
pub(crate) struct FaceFoot {
    /// The uv.
    pub(crate) uv: Point2,
    /// The point.
    pub(crate) point: Point3,
    /// The distance from `p`.
    pub(crate) distance: f64,
    /// Where the uv lies against the face.
    pub(crate) side: Option<UvSide>,
}

/// The face's foot of `p` (see [`FaceFoot`]): a closed form on every analytic kind, one
/// unit of work.
pub(crate) fn face_foot(
    face: Face<'_>,
    p: Point3,
    budget: &mut Budget,
    stop: &Stop,
) -> Result<FaceFoot, BodyqError> {
    budget.charge(1)?;
    let uv_box = face
        .uv_box()
        .ok_or_else(|| BodyqError::Defect(BodyqDefect::NoUvBox { face: face.id() }))?;
    let foot = face
        .surface()
        .invert(p, None, Some(&uv_box), budget, stop)
        .map_err(|e| BodyqError::of_surface(e, face.id()))?
        .into_foot();
    Ok(FaceFoot {
        uv: foot.at,
        point: foot.point,
        distance: foot.distance,
        side: face.contains_uv(foot.at),
    })
}

/// The edge's foot of `p` inside its interval: its parameter, point and distance; `None`
/// for a collapsed edge or a foot outside the interval (the nearest point is then an end).
/// One unit of work.
pub(crate) fn edge_foot(
    edge: Edge<'_>,
    p: Point3,
    budget: &mut Budget,
) -> Result<Option<(f64, Point3, f64)>, BodyqError> {
    let EdgeShape::Curve { curve, interval } = edge.shape() else {
        return Ok(None);
    };
    budget.charge(1)?;
    let inv = curve.invert(p, Some(interval)).map_err(|_| {
        BodyqError::Unsupported(BodyqUnsupported::Nurbs {
            at: Entity::Edge(edge.id()),
        })
    })?;
    Ok(inv.is_inside().then(|| {
        let f = inv.foot();
        (f.at, f.point, f.distance)
    }))
}

/// Where the ray `p + t d`, `t` in `[0, len]`, is inside `b`: `[t_in, t_out]`, or `None`
/// when it misses. A component of `d` that is zero keeps the ray in or out of that slab.
pub(crate) fn slab(b: &Extent3d, p: Point3, d: Vec3, len: f64) -> Option<(f64, f64)> {
    let (mut lo, mut hi) = (0.0_f64, len);
    for (min, max, o, k) in [
        (b.x_min(), b.x_max(), p.x, d.x),
        (b.y_min(), b.y_max(), p.y, d.y),
        (b.z_min(), b.z_max(), p.z, d.z),
    ] {
        if k == 0.0 {
            if o < min || o > max {
                return None;
            }
            continue;
        }
        let (a, z) = ((min - o) / k, (max - o) / k);
        let (a, z) = if a <= z { (a, z) } else { (z, a) };
        lo = lo.max(a);
        hi = hi.min(z);
    }
    (lo <= hi).then_some((lo, hi))
}

/// The distance from `q` to the segment `p + t d`, `t` in `[0, len]` (`d` unit), and the
/// `t` of its nearest point.
pub(crate) fn segment_distance(p: Point3, d: Vec3, len: f64, q: Point3) -> (f64, f64) {
    let t = (q - p).dot(d).clamp(0.0, len);
    ((p + d * t).distance(q), t)
}
