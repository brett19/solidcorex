// Written for solidcorex. Apache-2.0.
//! [`principal_curvatures`]: how a face bends at a point, from its surface's first and second
//! fundamental forms (research 74 §52, the pin's `query::continuity::principal_curvatures`).
//!
//! **Convention.** Signed convex positive against the face's normal, which is the one out of
//! its material (the natural normal when the front side is air, its reverse when the front is
//! solid) or, for a sheet with air on both sides, the natural normal. A sphere seen from
//! outside reads `+1 / r` twice; a cylinder `1 / r` round and `0` along; the inner equator of a
//! torus `+1 / r` round the tube and `-1 / (R - r)` along the equator. `k1 >= k2`.
//! (BW4's `continuity` reads the opposite sign, a normal section's bend *away* from the
//! outward normal; that is the drop's convention and is unchanged.)
//!
//! **Method.** The foot of the point on the surface (`face_uv`: closed form on every analytic
//! kind) and the surface's jet to the second order there. In the orthonormal tangent frame
//! `e1 = S_u / |S_u|`, `e2 = n x e1` the second form is the symmetric 2 x 2 matrix `-II(e_i,
//! e_j)`; its eigenvalues are `k1`, `k2` and its eigenvectors the directions, with no
//! generalised eigenproblem and no ordering of the two fundamental forms to get wrong. At an
//! umbilic (`k1 == k2`) the directions are the frame's own.

use scx_brep::{Brep, FaceId, FaceSide};
use scx_num::budget::{Budget, Stop};
use scx_num::fmath::{atan2, hypot, sin_cos};
use scx_num::vector2d::Point2;
use scx_num::vector3d::{Point3, Vec3};

use crate::error::{BodyqError, BodyqRefusal};
use crate::geom::ABOVE;
use crate::reads::face_uv;

/// The principal curvatures of a face at a point ([`principal_curvatures`]).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Principal {
    /// The foot of the point on the face's surface.
    pub point: Point3,
    /// The foot's `(u, v)`.
    pub uv: Point2,
    /// The unit normal the curvatures are signed against.
    pub normal: Vec3,
    /// The larger principal curvature, per mm, positive where the surface bends away from
    /// `normal` (convex seen from where it points).
    pub k1: f64,
    /// The smaller.
    pub k2: f64,
    /// Gaussian curvature `k1 * k2`, per mm^2: independent of the normal's sign.
    pub gaussian: f64,
    /// Mean curvature `(k1 + k2) / 2`, per mm, signed as `k1`.
    pub mean: f64,
    /// The unit tangent direction of `k1`.
    pub d1: Vec3,
    /// The unit tangent direction of `k2`.
    pub d2: Vec3,
}

/// Whether the partials `su` and `sv` fail to span a tangent plane to a rounding of the
/// surface's own: one is not finite, or one is shorter than the other by more than `1e-12`
/// (a sphere's pole, whose `cos(pi / 2)` is not 0, a cone's apex) or they are parallel.
pub(crate) fn pole(su: Vec3, sv: Vec3) -> bool {
    let (e, g, f) = (su.dot(su), sv.dot(sv), su.dot(sv));
    !(e.is_finite() && g.is_finite() && f.is_finite())
        || e.min(g) <= 1e-24 * e.max(g)
        || e * g - f * f <= 1e-24 * (e * g)
}

/// The principal curvatures of `face` where `p` drops on its surface (the surface's natural
/// domain, untrimmed: the foot need not lie in the face, and its `uv` is reported).
///
/// To read a sheet at a point, name the face with
/// [`closest_to_point`](crate::closest_to_point) first.
///
/// One unit of work is charged for the foot.
///
/// # Errors
///
/// Refused for a face the body does not have, a point that is not finite, a face with
/// material on both sides ([`NoMaterialSide`](BodyqRefusal::NoMaterialSide)) and a foot where
/// the surface has no normal or its partials are parallel ([`Singular`](BodyqRefusal::Singular):
/// a pole, an apex). Exhausted or cancelled by `budget` and `stop`; Unsupported for a NURBS
/// surface whose inversion is pending.
pub fn principal_curvatures(
    body: &Brep,
    face: FaceId,
    p: Point3,
    budget: &mut Budget,
    stop: &Stop,
) -> Result<Principal, BodyqError> {
    let foot = face_uv(body, face, p, budget, stop)?;
    let f = body
        .face(face)
        .ok_or(BodyqError::Refused(BodyqRefusal::NoEntity {
            entity: scx_brep::Entity::Face(face),
        }))?;
    let (front, back) = (
        f.shell(FaceSide::Front).region().is_solid(),
        f.shell(FaceSide::Back).region().is_solid(),
    );
    if front && back {
        return Err(BodyqError::Refused(BodyqRefusal::NoMaterialSide { face }));
    }
    let sign = if front { -1.0 } else { 1.0 };
    let singular = BodyqError::Refused(BodyqRefusal::Singular { face });
    let j = f.surface().jet::<2>(foot.uv, ABOVE);
    let at = |i, k| j.get(i, k).filter(|v| v.is_finite()).ok_or(singular);
    let (su, sv) = (at(1, 0)?, at(0, 1)?);
    let (suu, suv, svv) = (at(2, 0)?, at(1, 1)?, at(0, 2)?);
    let (e, g, ff) = (su.dot(su), sv.dot(sv), su.dot(sv));
    let det = e * g - ff * ff;
    if pole(su, sv) || det <= 1e-24 * (e * g) {
        return Err(singular);
    }
    let n = su.cross(sv).normalized().ok_or(singular)?.as_vec() * sign;
    let e1 = su.normalized().ok_or(singular)?.as_vec();
    let e2 = n.cross(e1);
    // a tangent vector's coordinates in (S_u, S_v)
    let coords = |t: Vec3| {
        let (a, b) = (t.dot(su), t.dot(sv));
        [(g * a - ff * b) / det, (e * b - ff * a) / det]
    };
    // -II(s, t): convex positive against `n`
    let bend = |s: Vec3, t: Vec3| {
        let ([s0, s1], [t0, t1]) = (coords(s), coords(t));
        -(s0 * t0 * suu.dot(n) + (s0 * t1 + s1 * t0) * suv.dot(n) + s1 * t1 * svv.dot(n))
    };
    let (a, b, d) = (bend(e1, e1), bend(e1, e2), bend(e2, e2));
    let (mean, r) = (0.5 * (a + d), hypot(0.5 * (a - d), b));
    let (k1, k2) = (mean + r, mean - r);
    let (s, c) = sin_cos(0.5 * atan2(2.0 * b, a - d));
    let (d1, d2) = (e1 * c + e2 * s, e2 * c - e1 * s);
    if ![k1, k2, mean].iter().all(|x| x.is_finite()) {
        return Err(singular);
    }
    Ok(Principal {
        point: foot.point,
        uv: foot.uv,
        normal: n,
        k1,
        k2,
        gaussian: k1 * k2,
        mean,
        d1,
        d2,
    })
}
