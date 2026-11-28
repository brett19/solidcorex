// Written for solidcorex. Apache-2.0.
//! Facing faces (research 74 §34, BW11): whether two planar faces, or two cylindrical ones,
//! look at each other across material (the two sides of a thin wall), how thick the wall is,
//! and the plane or cylinder halfway between them; with the small readings a round wall needs
//! (the material's side, the directions that stay on the face, one cylinder's axial range in
//! another's coordinates).
//!
//! | pin (`surface::face_pair`, `face_pair_cylinder`) | here |
//! |---|---|
//! | the kernel's half degree: opposite normals "within `ang_tol`" | two directions are one line when the lines stay within coincidence over the reach ([`Near`]): the angle is coincidence over size |
//! | the zone tolerance: thickness must exceed it | the [`Near`]'s coincidence (`coincident(res, acc, acc)`) |
//! | `Ok(None)` for "not a pair", whatever the reason | [`Facing::Apart`] with the [`Apart`] reason |
//! | the thickness at the first plane's point | the same, with the planes' [`skew`](FacingPlanes::skew) over the reach reported |
//! | a face's plane or cylinder handed over by the application | also read from the body: [`outward_plane`], [`cylinder_face`] |
//! | `material_toward_axis`: a point on the axis refused by an absolute `EFF_ZERO_SQ` | refused within coincidence of the axis |
//! | `round_directions`: "along the axis" within a billionth of the direction | within coincidence over the curve's length |

use scx_brep::{Brep, FaceId, FaceSide};
use scx_num::frame::Frame;
use scx_num::units::Length;
use scx_num::vector3d::{Point3, Vec3};
use scx_surface::Surface;

use crate::error::{BodyqError, BodyqRefusal};
use crate::reach::{Near, finite_point, positive, unit3};

/// A planar face's plane: a point on it and the face's outward normal (any nonzero length).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct OutwardPlane {
    /// A point on the plane.
    pub point: Point3,
    /// The face's outward normal, as its shell has the face's side.
    pub outward: Vec3,
}

/// Why two faces are not a wall.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum Apart {
    /// The planes or the axes do not make one line over the reach.
    Tilted,
    /// Planes whose outward normals run the same way.
    SameWay,
    /// The axes are parallel but offset by more than coincidence.
    OffAxis,
    /// The material lies the same way on both cylinders (two outsides, two bores).
    SameSide,
    /// The material is not between them: outward normals toward each other, or a bore wider
    /// than the outside.
    Air,
    /// No farther apart than coincidence: nothing between them.
    Thin,
}

/// The answer of a pairing.
#[derive(Debug, Clone, Copy, PartialEq)]
#[non_exhaustive]
pub enum Facing<T> {
    /// A wall.
    Pair(T),
    /// Not a wall, and why.
    Apart(Apart),
}

/// Two planes that face each other across material.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FacingPlanes {
    /// The distance between the planes along the first plane's normal at its point: the
    /// wall's thickness. More than coincidence.
    pub thickness: Length,
    /// A point on the plane halfway between them.
    pub mid_point: Point3,
    /// The first plane's unit normal: the halfway plane's side toward the first plane.
    pub toward_first: Vec3,
    /// How far the planes come from parallel over the reach (at most coincidence): the
    /// thickness varies by this much across the faces.
    pub skew: Length,
}

/// Whether `a` and `b` face each other across material, and if so across how much.
///
/// The outward normals must run opposite ways, the two planes make one line over the reach
/// ([`Near`]), each lies behind the other (the material is between them), and they are
/// farther apart than coincidence. The thickness is measured along the first plane's normal
/// from its point to where that line meets the second plane, so any point of the second plane
/// gives it.
///
/// # Errors
///
/// Refused for a point or normal that is not finite, or a normal of no length.
pub fn facing_planes(
    a: &OutwardPlane,
    b: &OutwardPlane,
    near: &Near,
) -> Result<Facing<FacingPlanes>, BodyqError> {
    finite_point(a.point)?;
    finite_point(b.point)?;
    let (ua, ub) = (unit3(a.outward)?, unit3(b.outward)?);
    let along = ua.dot(ub);
    if !near.one_line(ua, ub) {
        return Ok(Facing::Apart(Apart::Tilted));
    }
    if along >= 0.0 {
        return Ok(Facing::Apart(Apart::SameWay));
    }
    // `(a.point - t ua - b.point) . ub = 0`: how far behind `a.point`, along `ua`, the normal
    // line meets `b`'s plane. Positive when `b` lies behind `a`.
    let thickness = a.point.distance_along(b.point, ub) / along;
    let coin = near.coincidence().mm();
    if thickness < -coin {
        return Ok(Facing::Apart(Apart::Air));
    }
    if thickness <= coin {
        return Ok(Facing::Apart(Apart::Thin));
    }
    Ok(Facing::Pair(FacingPlanes {
        thickness: Length::from_mm(thickness),
        mid_point: a.point - ua * (thickness * 0.5),
        toward_first: ua,
        skew: Length::from_mm(ua.cross(ub).length() * near.reach().mm()),
    }))
}

/// A cylindrical face's cylinder.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CylinderFace {
    /// A point on the axis.
    pub axis_point: Point3,
    /// The axis direction (any nonzero length).
    pub axis: Vec3,
    /// The radius.
    pub radius: Length,
    /// Whether the face's material lies toward the axis (the outside of a tube) and not away
    /// from it (a bore).
    pub material_toward_axis: bool,
}

/// Two cylinders that face each other across material.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FacingCylinders {
    /// The wall's thickness: the outer radius less the inner. More than coincidence.
    pub thickness: Length,
    /// The radius of the cylinder halfway between them.
    pub mid_radius: Length,
    /// A point on the shared axis (the first cylinder's).
    pub axis_point: Point3,
    /// The shared axis's unit direction (the first cylinder's).
    pub axis: Vec3,
}

fn check_cylinder(c: &CylinderFace) -> Result<(Vec3, f64), BodyqError> {
    finite_point(c.axis_point)?;
    Ok((unit3(c.axis)?, positive(c.radius)?))
}

/// Whether `a` and `b` face each other across material: one axis (parallel over the reach,
/// the second axis line through the first axis's point within coincidence), the outer one's
/// material toward the axis and the inner's away, and radii farther apart than coincidence.
///
/// # Errors
///
/// Refused for a point, axis or radius that is not finite, a radius not above zero, or an
/// axis of no length.
pub fn facing_cylinders(
    a: &CylinderFace,
    b: &CylinderFace,
    near: &Near,
) -> Result<Facing<FacingCylinders>, BodyqError> {
    let ((ua, ra), (ub, rb)) = (check_cylinder(a)?, check_cylinder(b)?);
    if !near.one_line(ua, ub) {
        return Ok(Facing::Apart(Apart::Tilted));
    }
    if !near.within((a.axis_point - b.axis_point).cross(ub).length()) {
        return Ok(Facing::Apart(Apart::OffAxis));
    }
    let ((outer, ro), (inner, ri)) = match (a.material_toward_axis, b.material_toward_axis) {
        (true, false) => ((a, ra), (b, rb)),
        (false, true) => ((b, rb), (a, ra)),
        _ => return Ok(Facing::Apart(Apart::SameSide)),
    };
    let _ = (outer, inner);
    let thickness = ro - ri;
    let coin = near.coincidence().mm();
    if thickness < -coin {
        return Ok(Facing::Apart(Apart::Air));
    }
    if thickness <= coin {
        return Ok(Facing::Apart(Apart::Thin));
    }
    Ok(Facing::Pair(FacingCylinders {
        thickness: Length::from_mm(thickness),
        mid_radius: Length::from_mm(ri + thickness * 0.5),
        axis_point: a.axis_point,
        axis: ua,
    }))
}

/// Which way face `face`'s material lies, as `(outward sign, surface)`: the sign to give its
/// natural normal to point out of the material.
fn outward_of(body: &Brep, face: FaceId) -> Result<(f64, &Surface), BodyqError> {
    let f = body
        .face(face)
        .ok_or(BodyqError::Refused(BodyqRefusal::NoEntity {
            entity: scx_brep::Entity::Face(face),
        }))?;
    let (front, back) = (
        f.shell(FaceSide::Front).region().is_solid(),
        f.shell(FaceSide::Back).region().is_solid(),
    );
    if front == back {
        return Err(BodyqError::Refused(BodyqRefusal::NoMaterialSide { face }));
    }
    // The natural normal points into the front side: out of the material when that is air.
    Ok((if front { -1.0 } else { 1.0 }, f.surface()))
}

/// The plane of planar face `face` of `body` with the normal out of its material.
///
/// # Errors
///
/// Refused for a face the body does not have, one that is not planar, or one with material on
/// both sides or neither.
pub fn outward_plane(body: &Brep, face: FaceId) -> Result<OutwardPlane, BodyqError> {
    let (sign, surface) = outward_of(body, face)?;
    let Surface::Plane(p) = surface else {
        return Err(BodyqError::Refused(BodyqRefusal::WrongSurface { face }));
    };
    let frame: &Frame = p.frame();
    Ok(OutwardPlane {
        point: frame.origin(),
        outward: frame.z().as_vec() * sign,
    })
}

/// The cylinder of cylindrical face `face` of `body`, with the side its material lies on.
///
/// # Errors
///
/// As [`outward_plane`], for a cylinder.
pub fn cylinder_face(body: &Brep, face: FaceId) -> Result<CylinderFace, BodyqError> {
    let (sign, surface) = outward_of(body, face)?;
    let Surface::Cylinder(c) = surface else {
        return Err(BodyqError::Refused(BodyqRefusal::WrongSurface { face }));
    };
    // The natural normal points away from the axis when the sense is forward; the outward one
    // is it times `sign`. The material is toward the axis when the outward normal points away.
    let outward_away = sign * c.sense().sign() > 0.0;
    Ok(CylinderFace {
        axis_point: c.frame().origin(),
        axis: c.frame().z().as_vec(),
        radius: c.radius(),
        material_toward_axis: outward_away,
    })
}

/// Whether a cylindrical face's material lies toward its axis (through `axis_point` along
/// `axis`), read at the face's `point` from its `outward` normal there: the normal points away
/// from the axis exactly when the material lies toward it (the outside of a tube), toward the
/// axis on a bore.
///
/// # Errors
///
/// Refused for a point or vector that is not finite, an axis or normal of no length, or a
/// point within coincidence of the axis ([`OnAxis`](BodyqRefusal::OnAxis)).
pub fn material_toward_axis(
    axis_point: Point3,
    axis: Vec3,
    point: Point3,
    outward: Vec3,
    near: &Near,
) -> Result<bool, BodyqError> {
    finite_point(axis_point)?;
    finite_point(point)?;
    let u = unit3(axis)?;
    let n = unit3(outward)?;
    let from = point - axis_point;
    let away = from - u * from.dot(u);
    if near.within(away.length()) {
        return Err(BodyqError::Refused(BodyqRefusal::OnAxis));
    }
    Ok(n.dot(away) > 0.0)
}

/// The unit directions that stay on a cylindrical face from one of its points.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Directions {
    /// Both ways along the axis.
    pub axial: [Vec3; 2],
    /// Both ways round the axis, unless the curve runs along it (then it follows from the
    /// curve's direction nothing).
    pub round: Option<[Vec3; 2]>,
}

/// The unit directions to step along from a point of a cylindrical face to stay on the face,
/// where `along` is the direction of a curve of length `near.reach()` on the face through the
/// point (an edge's tangent). The round one is `along` with its part along the axis taken
/// away; the curve runs along the axis when that remainder, over its length, is within
/// coincidence, and then no round direction follows from it.
///
/// # Errors
///
/// Refused for a vector that is not finite, or an axis of no length. A curve direction of no
/// length gives the axial directions alone.
pub fn round_directions(axis: Vec3, along: Vec3, near: &Near) -> Result<Directions, BodyqError> {
    let u = unit3(axis)?;
    if !along.is_finite() {
        return Err(BodyqError::Refused(BodyqRefusal::NotFinite));
    }
    let axial = [u, -u];
    // The direction alone counts, not its length: neither the projection overflows nor the
    // remainder underflows.
    let Some((_, dir)) = along.length_and_unit() else {
        return Ok(Directions { axial, round: None });
    };
    let round = dir.as_vec() - u * dir.dot(u);
    if round.length() * near.reach().mm() <= near.coincidence().mm() {
        return Ok(Directions { axial, round: None });
    }
    let r = unit3(round)?;
    Ok(Directions {
        axial,
        round: Some([r, -r]),
    })
}

/// The axial range `range` of cylinder `b` (distances from its axis point along its unit axis)
/// read in cylinder `a`'s axial coordinates, low end first. A coordinate `x` on `b`'s axis is
/// the point `pb + x ub`; on `a`'s axis it is `shift + x turn`, with `shift = (pb - pa) . ua`
/// and `turn = ub . ua` (plus or minus one for one axis run either way). Radii and sides play
/// no part.
///
/// # Errors
///
/// Refused for an axis point, axis or range end that is not finite, or an axis of no length.
pub fn axial_range_in(
    a: &CylinderFace,
    b: &CylinderFace,
    range: [Length; 2],
) -> Result<[Length; 2], BodyqError> {
    finite_point(a.axis_point)?;
    finite_point(b.axis_point)?;
    let (ua, ub) = (unit3(a.axis)?, unit3(b.axis)?);
    let [lo, hi] = range.map(Length::mm);
    if !(lo.is_finite() && hi.is_finite()) {
        return Err(BodyqError::Refused(BodyqRefusal::NotFinite));
    }
    let shift = (b.axis_point - a.axis_point).dot(ua);
    let turn = ub.dot(ua);
    let ends = [shift + lo * turn, shift + hi * turn];
    Ok([
        Length::from_mm(ends[0].min(ends[1])),
        Length::from_mm(ends[0].max(ends[1])),
    ])
}
