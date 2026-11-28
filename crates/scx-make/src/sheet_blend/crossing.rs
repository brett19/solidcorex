// Written for solidcorex. Apache-2.0.
//! [`fillet_crossing`]: the round between two sheets that cross, on the sides chosen.

use scx_boolean::{Cut, imprint_curves};
use scx_brep::op::Cx;
use scx_brep::{Brep, FaceId};
use scx_curve::{Curve, Line};
use scx_num::vector3d::{Point3, Vec3};
use scx_surface::{Plane, Surface};

use super::error::{
    SheetBlendError, SheetBlendRefusal, SheetBlendUnsupported, refused, unsupported,
};
use super::fillet::{FilletSection, FilletSheets, fillet_sheets};
use super::trim::keep_faces;

/// Which side of the other sheet's plane a sheet's kept part lies on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SheetSide {
    /// The side the other plane's normal points to.
    Front,
    /// The other side.
    Back,
}

/// Two crossing sheets cut on their crossing, the parts kept, and the round between those.
#[derive(Debug, Clone)]
pub struct FilletCrossing {
    /// The part of each sheet kept, cut on the crossing line, before the round.
    pub parts: [Brep; 2],
    /// The round between the parts and the parts cut back to it.
    pub fillet: FilletSheets,
}

/// The round of `section` between one-face plane sheets `a` and `b` that cross: each is cut
/// on the line of the two planes, the part of `a` on `sides[0]` of `b`'s plane and the part
/// of `b` on `sides[1]` of `a`'s plane kept, and the two parts rounded along the edge the cut
/// gave both ([`fillet_sheets`]).
///
/// # Errors
///
/// - [`SheetBlendRefusal::NotOneFace`], [`SheetBlendRefusal::NotSheet`];
/// - [`SheetBlendUnsupported::NurbsSheet`] for a NURBS sheet (its crossing is the NURBS
///   surface-surface intersection's, SM9), [`SheetBlendUnsupported::CurvedImprint`] for
///   another curved one;
/// - [`SheetBlendRefusal::NoCrossing`] for parallel planes or a line that misses a sheet;
/// - [`SheetBlendRefusal::NoSharedEdge`] when the parts' cut edges are not the same stretch
///   of the line (the sheets cross over different spans);
/// - as [`fillet_sheets`].
pub fn fillet_crossing(
    a: &Brep,
    b: &Brep,
    sides: [SheetSide; 2],
    section: FilletSection,
    cx: &mut Cx<'_>,
) -> Result<FilletCrossing, SheetBlendError> {
    let mut planes = Vec::with_capacity(2);
    for (operand, s) in [a, b].into_iter().enumerate() {
        if s.regions().any(scx_brep::Region::is_solid) {
            return Err(refused(SheetBlendRefusal::NotSheet { operand }));
        }
        let mut faces = s.faces();
        let (Some(f), None) = (faces.next(), faces.next()) else {
            return Err(refused(SheetBlendRefusal::NotOneFace { operand }));
        };
        match f.surface() {
            Surface::Plane(p) => planes.push((f.id(), *p)),
            Surface::Nurbs(_) => return Err(unsupported(SheetBlendUnsupported::NurbsSheet)),
            _ => return Err(unsupported(SheetBlendUnsupported::CurvedImprint)),
        }
    }
    let [(fa, pa), (fb, pb)] = planes[..] else {
        return Err(SheetBlendError::Defect { why: "two planes" });
    };
    let line = crossing(&pa, &pb).ok_or(refused(SheetBlendRefusal::NoCrossing))?;
    let part = |body: &Brep, f: FaceId, other: &Plane, side: SheetSide, cx: &mut Cx<'_>| {
        let done = imprint_curves(body, f, &[Cut::whole(Curve::Line(line))], cx)?;
        if done.regions.len() < 2 {
            return Err(refused(SheetBlendRefusal::NoCrossing));
        }
        let o = other.frame().origin();
        let n = Vec3::from(other.frame().z());
        let front = |q: Point3| (q - o).dot(n) > 0.0;
        let gone: Vec<FaceId> = done
            .regions
            .iter()
            .filter(|r| front(r.inside) != (side == SheetSide::Front))
            .map(|r| r.face)
            .collect();
        keep_faces(&done.done.body, &gone, *cx.doc)
    };
    let parts = [
        part(a, fa, &pb, sides[0], cx)?,
        part(b, fb, &pa, sides[1], cx)?,
    ];
    let fillet = fillet_sheets(&parts[0], &parts[1], section, cx)?;
    Ok(FilletCrossing { parts, fillet })
}

/// The line the planes `a` and `b` share, through the point nearest `a`'s origin; `None`
/// when they are parallel.
fn crossing(a: &Plane, b: &Plane) -> Option<Line> {
    let (na, nb) = (Vec3::from(a.frame().z()), Vec3::from(b.frame().z()));
    let d = na.cross(nb);
    let dd = d.length_squared();
    if dd.is_nan() || dd <= f64::EPSILON * f64::EPSILON {
        return None;
    }
    // The point on both planes nearest the origin of `a`: `x = p_a + s na + t nb` solved for
    // the two plane equations (unit normals, so `na . na = nb . nb = 1`).
    let pa = a.frame().origin();
    let hb = (b.frame().origin() - pa).dot(nb);
    let c = na.dot(nb);
    // s + c t = 0, c s + t = hb
    let det = 1.0 - c * c;
    let (s, t) = (-c * hb / det, hb / det);
    Line::new(pa + na * s + nb * t, d).ok()
}
