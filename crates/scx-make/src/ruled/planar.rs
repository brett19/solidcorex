// Written for solidcorex. Apache-2.0.
//! The exact case: lines and arcs that lie in one plane (module docs of [`super`]).

use scx_brep::{Document, FaceId};
use scx_curve::Curve;
use scx_num::frame::{Frame, Sense};
use scx_num::vector2d::Point2;
use scx_num::vector3d::{Point3, Vec3};
use scx_nurbs::Approach;
use scx_planar::{Region, Segment};

use super::{Join, RuledError, RuledRefusal, RuledSide};
use crate::Made;
use crate::error::{Defect, MakeError};
use crate::sheet::sheet;

/// A side of the region: a line or an arc, with the points that define it.
fn lines_or_arcs(a: &RuledSide, b: &RuledSide) -> bool {
    [a, b]
        .iter()
        .all(|s| matches!(s.curve, Curve::Line(_) | Curve::Circle(_)))
}

/// The face on the plane of `a` and `b`, if they are lines or arcs in one plane that bound a
/// simple region; `None` when the case does not apply.
pub(super) fn ruled(
    a: &RuledSide,
    b: &RuledSide,
    join: Join,
    doc: &Document,
) -> Result<Option<Made<FaceId>>, RuledError> {
    if !lines_or_arcs(a, b) {
        return Ok(None);
    }
    let zone = doc.resolution().zone().mm();
    let at = |s: &RuledSide, t: f64| s.curve.point(t);
    let mid = |s: &RuledSide| at(s, 0.5 * (s.interval.min() + s.interval.max()));
    let (a0, a1) = (at(a, a.interval.min()), at(a, a.interval.max()));
    let (bs, be) = (at(b, b.interval.min()), at(b, b.interval.max()));
    let (b0, b1) = match join {
        Join::Same => (bs, be),
        Join::Reversed => (be, bs),
    };
    // The loop a0 -> a1 -> b1 -> b0, its normal by Newell's sum over the sampled points.
    let ring = [a0, mid(a), a1, b1, mid(b), b0];
    let mut sum = Vec3::new(0.0, 0.0, 0.0);
    for (i, p) in ring.iter().enumerate() {
        let q = ring.get((i + 1) % ring.len()).copied().unwrap_or(*p);
        let (p, q) = (p.to_vec(), q.to_vec());
        sum = sum + p.cross(q);
    }
    let Some(n) = sum.normalized() else {
        // No net area: both lines on one line, or a bow tie whose two halves cancel.
        let on_a = |p: Point3| {
            let d = a1 - a0;
            (p - a0).cross(d).length() <= zone * d.length()
        };
        let collinear = matches!(a.curve, Curve::Line(_))
            && matches!(b.curve, Curve::Line(_))
            && on_a(b0)
            && on_a(b1);
        return if collinear {
            Err(RuledError::Refused(RuledRefusal::NoArea))
        } else {
            Ok(None)
        };
    };
    let n_vec: Vec3 = n.into();
    // Every sampled point in the plane, and each arc's own plane that plane.
    if ring.iter().any(|p| ((*p - a0).dot(n_vec)).abs() > zone) {
        return Ok(None);
    }
    for s in [a, b] {
        if let Curve::Circle(c) = &s.curve {
            let tilt = Vec3::from(*c.frame().z()).cross(n_vec).length() * c.radius().mm();
            let off = (c.frame().origin() - a0).dot(n_vec).abs();
            if tilt > zone || off > zone {
                return Ok(None);
            }
        }
    }
    let Some(frame) = Frame::new(a0, n_vec, a1 - a0) else {
        return Ok(None);
    };
    let (fx, fy): (Vec3, Vec3) = ((*frame.x()).into(), (*frame.y()).into());
    let flat = |p: Point3| {
        let d = p - a0;
        Point2::new(d.dot(fx), d.dot(fy))
    };
    let segment = |s: &RuledSide| -> Option<Segment> {
        let (p, q) = (flat(at(s, s.interval.min())), flat(at(s, s.interval.max())));
        match &s.curve {
            Curve::Line(_) => Some(Segment::line(p, q)),
            Curve::Circle(c) => {
                let t0 = s.interval.min();
                let start = at(s, t0);
                let radial = start - c.frame().origin();
                let tangent =
                    Vec3::from(s.curve.jet::<1>(t0, Approach::Above).d.first()?.to_array());
                let sense = if radial.cross(tangent).dot(n_vec) > 0.0 {
                    Sense::Forward
                } else {
                    Sense::Reversed
                };
                Some(Segment::arc(flat(c.frame().origin()), p, q, sense))
            }
            _ => None,
        }
    };
    let (Some(sa), Some(sb)) = (segment(a), segment(b)) else {
        return Ok(None);
    };
    // b in the loop runs from b1 to b0.
    let sb = match join {
        Join::Same => sb.reversed(),
        Join::Reversed => sb,
    };
    let mut loop_ = vec![sa];
    if a1.distance(b1) > zone {
        loop_.push(Segment::line(flat(a1), flat(b1)));
    }
    loop_.push(sb);
    if b0.distance(a0) > zone {
        loop_.push(Segment::line(flat(b0), flat(a0)));
    }
    let Ok(region) = Region::new(loop_, Vec::new(), doc.resolution()) else {
        return Ok(None);
    };
    let made = match sheet(core::slice::from_ref(&region), &frame, doc) {
        Ok(m) => m,
        Err(MakeError::Refused(_) | MakeError::Unsupported(_)) => return Ok(None),
        Err(e) => return Err(e.into()),
    };
    let face = made
        .faces
        .faces
        .first()
        .copied()
        .ok_or(RuledError::Make(MakeError::Defect(Defect::Interval)))?;
    Ok(Some(Made {
        body: made.body,
        faces: face,
    }))
}
