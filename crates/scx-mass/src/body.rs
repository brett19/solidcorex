// Written for solidcorex. Apache-2.0.
//! The body's sums: each face integrated about its own reference, moved to the body's
//! reference by the exact binomial shift, and added in face id order.

use scx_brep::op::Scratch;
use scx_brep::{Brep, Entity, Face, FaceSide};
use scx_num::budget::{Budget, Stop};
use scx_num::vector3d::{Point3, Vec3};

use crate::error::{MassError, MassRefusal, MassUnsupported};
use crate::face::{self, reference};
use crate::integrand::{FIRST_MONOMIAL, Set, monomial, shift};
use crate::kronrod::{MAX_COMPONENTS, Meter};
use crate::spec::{Estimate, Mass, MassErrors, MassSpec, RelAccuracy, Sym3};

/// Whether the region on `side` of `face` is solid.
fn solid(face: Face<'_>, side: FaceSide) -> bool {
    face.shell(side).region().is_solid()
}

/// The sign a face's flux counts with in the volume: `+1` with the solid behind it (the
/// natural normal points out of the solid), `-1` with it in front, `0` between two solids or
/// two voids.
fn volume_sign(face: Face<'_>) -> f64 {
    f64::from(i8::from(solid(face, FaceSide::Back)) - i8::from(solid(face, FaceSide::Front)))
}

/// Whether a face counts in the area: every face but one between two solid regions.
fn counts_in_area(face: Face<'_>) -> bool {
    !(solid(face, FaceSide::Back) && solid(face, FaceSide::Front))
}

/// The body's reference: its box's middle on the grid, or `about`.
fn body_reference(body: &Brep, about: Option<Point3>) -> Result<Point3, MassError> {
    if let Some(p) = about {
        return if p.x.is_finite() && p.y.is_finite() && p.z.is_finite() {
            Ok(p)
        } else {
            Err(MassError::Refused(MassRefusal::BadReference))
        };
    }
    if let Some(b) = body.bounding_box() {
        return Ok(reference(b.mid(), b.max_length()));
    }
    // No box: name the face that has none (the first face when the body's box fails
    // otherwise); a body with no face is measured about the origin.
    let mut faces = body.faces();
    let first = faces.clone().next();
    match faces.find(|f| f.bounding_box().is_none()).or(first) {
        Some(f) => Err(MassError::Unsupported(MassUnsupported::NoBox {
            at: Entity::from(f.id()),
        })),
        None => Ok(Point3::new(0.0, 0.0, 0.0)),
    }
}

/// The volume enclosed by the solid regions, with its error estimate.
///
/// `scratch` is the memo of face integrals (not filled yet: research 72's MA3).
///
/// # Errors
///
/// [`MassError`]: a face with no box or a winding this method does not close
/// (`Unsupported`), the budget, the stop flag, and the integrator's defects.
pub fn volume(
    body: &Brep,
    accuracy: RelAccuracy,
    scratch: &mut Scratch,
    budget: &mut Budget,
    stop: &Stop,
) -> Result<Estimate, MassError> {
    let _ = scratch;
    let about = body_reference(body, None)?;
    let mut meter = Meter::new(budget, stop);
    let (mut value, mut error) = (0.0, 0.0);
    for f in body.faces() {
        let sign = volume_sign(f);
        if sign == 0.0 {
            continue;
        }
        let mut r = face::integrate(f, Set::Volume, accuracy.get(), &mut meter)?;
        shift(Set::Volume, r.reference - about, &mut r.value, &mut r.error);
        value += sign * r.value[1];
        error += r.error[1];
    }
    Ok(Estimate { value, error })
}

/// The boundary's area: every face but those between two solid regions, sheets included.
///
/// # Errors
///
/// As [`volume`].
pub fn area(
    body: &Brep,
    accuracy: RelAccuracy,
    scratch: &mut Scratch,
    budget: &mut Budget,
    stop: &Stop,
) -> Result<Estimate, MassError> {
    let _ = scratch;
    let mut meter = Meter::new(budget, stop);
    let (mut value, mut error) = (0.0, 0.0);
    for f in body.faces().filter(|&f| counts_in_area(f)) {
        let r = face::integrate(f, Set::Area, accuracy.get(), &mut meter)?;
        value += r.value[0];
        error += r.error[0];
    }
    Ok(Estimate { value, error })
}

/// One face's area.
///
/// # Errors
///
/// As [`volume`], for the face.
pub fn face_area(
    face: Face<'_>,
    accuracy: RelAccuracy,
    scratch: &mut Scratch,
    budget: &mut Budget,
    stop: &Stop,
) -> Result<Estimate, MassError> {
    let _ = scratch;
    let mut meter = Meter::new(budget, stop);
    let r = face::integrate(face, Set::Area, accuracy.get(), &mut meter)?;
    Ok(Estimate {
        value: r.value[0],
        error: r.error[0],
    })
}

/// The body's mass properties ([`Mass`]) as `spec` asks.
///
/// # Errors
///
/// [`MassRefusal::BadDensity`] and [`MassRefusal::BadReference`] for the spec; otherwise
/// as [`volume`].
pub fn mass(
    body: &Brep,
    spec: &MassSpec,
    scratch: &mut Scratch,
    budget: &mut Budget,
    stop: &Stop,
) -> Result<Mass, MassError> {
    let _ = scratch;
    let density = spec.density;
    if !(density.is_finite() && density > 0.0) {
        return Err(MassError::Refused(MassRefusal::BadDensity {
            value: density,
        }));
    }
    let about = body_reference(body, spec.about)?;
    let mut meter = Meter::new(budget, stop);
    let mut sum = [0.0; MAX_COMPONENTS];
    let mut sum_err = [0.0; MAX_COMPONENTS];
    for f in body.faces() {
        let sign = volume_sign(f);
        let in_area = counts_in_area(f);
        if sign == 0.0 && !in_area {
            continue;
        }
        let mut r = face::integrate(f, Set::Full, spec.accuracy.get(), &mut meter)?;
        shift(Set::Full, r.reference - about, &mut r.value, &mut r.error);
        for (i, ((s, e), (v, ve))) in sum
            .iter_mut()
            .zip(sum_err.iter_mut())
            .zip(r.value.iter().zip(&r.error))
            .enumerate()
        {
            let w = if i < FIRST_MONOMIAL {
                f64::from(u8::from(in_area))
            } else {
                sign
            };
            *s += w * v;
            *e += w.abs() * ve;
        }
    }
    let m = |a: [u8; 3], c: f64| {
        let i = monomial(a);
        let at = |a: &[f64; MAX_COMPONENTS]| a.get(i).copied().unwrap_or(0.0);
        (at(&sum) / c, at(&sum_err) / c)
    };
    let (volume, e_volume) = m([0, 0, 1], 1.0);
    let (fx, efx) = m([1, 0, 1], 1.0);
    let (fy, efy) = m([0, 1, 1], 1.0);
    let (fz, efz) = m([0, 0, 2], 2.0);
    let (xx, exx) = m([2, 0, 1], 1.0);
    let (yy, eyy) = m([0, 2, 1], 1.0);
    let (zz, ezz) = m([0, 0, 3], 3.0);
    let (xy, exy) = m([1, 1, 1], 1.0);
    let (yz, eyz) = m([0, 1, 2], 2.0);
    let (zx, ezx) = m([1, 0, 2], 2.0);
    Ok(Mass {
        about,
        density,
        volume,
        first: Vec3::new(fx, fy, fz),
        second: Sym3 {
            xx,
            yy,
            zz,
            yz,
            zx,
            xy,
        },
        area: sum[0],
        area_first: Vec3::new(sum[1], sum[2], sum[3]),
        error: MassErrors {
            volume: e_volume,
            first: Vec3::new(efx, efy, efz),
            second: Sym3 {
                xx: exx,
                yy: eyy,
                zz: ezz,
                yz: eyz,
                zx: ezx,
                xy: exy,
            },
            area: sum_err[0],
            area_first: Vec3::new(sum_err[1], sum_err[2], sum_err[3]),
        },
    })
}
