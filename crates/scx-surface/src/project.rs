// Written for solidcorex. Apache-2.0.
//! A curve projected onto a surface along a direction ([`project_curve`]), and dropped onto
//! it square to the surface ([`drop_curve`]), in closed form (research 75 SF1; the pin's
//! `project_curves` and `drop_curves`, `solidcore-ops/src/sweep.rs`).
//!
//! The map is curve against surface: no face, no trim and no view. A face's trims cut the
//! image where `scx-bodyq` projects a curve onto a body; a view's visibility is `scx-hlr`'s.
//!
//! **Projection** along a unit direction `D` takes a point `p` to the points `p + s D` that
//! lie on the surface, for every `s`, either sign (the whole projection line, so a plane's
//! image is one map and nothing splits where the curve passes through the surface). **The
//! drop** takes `p` to its nearest point on the surface: the kind's `closest`, which for every
//! analytic kind is the foot in `p`'s meridian half-plane (a plane's is the projection along
//! its normal).
//!
//! The closed forms:
//!
//! | surface | curve | projection along `D` | drop |
//! |---|---|---|---|
//! | plane | every kind | the curve's image under the affine projection: a line, a conic of the same kind (a circle to its exact ellipse, on principal axes), a NURBS curve's control points | the projection along the normal |
//! | cylinder | a line parallel to the axis | the rulings where the plane of the line and `D` meets the cylinder: none, one (grazing) or two | — |
//! | cylinder | a line in a plane through the axis | — | its ruling on each side of the axis: a line |
//! | cylinder | a line square to the axis, off it | — | an arc of the circle at the line's height |
//! | cone | a line in a plane through the axis, `D` along the axis | a generator on each side of the axis: a line | — |
//! | cone | a line in a plane through the axis | — | a generator on each side, where its feet are not the apex: a line |
//!
//! Every other pair is [`ProjectError::NeedsFitter`] (Unsupported): its image is no curve
//! kind (a line projected along a slant onto a cylinder is an arc of an ellipse run at a
//! non-uniform speed; a circle dropped onto a cylinder off its axis is a space quartic), and
//! the step-5 fitter answers it. The name says which path replaces the refusal; it is never
//! an empty answer.
//!
//! **Pieces.** The answer is one [`Projected`] per piece, ordered along the curve (and for a
//! cylinder's two rulings, by `s`). A line through the axis of a cylinder or cone has a piece
//! on each side; a cone's drop leaves out the part whose feet are the apex alone. A piece
//! whose image has no length is left out; when that leaves none, the answer is
//! [`ProjectError::Flattened`]. A projection that misses the surface is the empty answer,
//! which is final.
//!
//! **Tolerance.** The predicates that choose a closed form ("parallel to the axis", "in a
//! plane through the axis", "square to the axis") hold within `res.zone()` over the curve's
//! extent, so each image stands within the zone of the true image of its piece (away from a
//! grazing hit, where the hit moves as the square root of the offset). The images are built
//! on the surface: a ruling at the radius, a generator on the cone.
//!
//! | solidcore at the pin | here |
//! |---|---|
//! | `project_curves(body, order, face, curves, direction, tolerance)`: SMLib's parallel projection onto one record face, every result a NURBS within `tolerance` | [`project_curve`] onto a [`Surface`]: the exact image as its own kind with its extent and the source extent it came from; the face's trim is `scx-bodyq`'s |
//! | `drop_curves(body, order, face, curves, tolerance)`: `DropAndTrimCurve` over the face's parameter box, then `LiftCurve`, as NURBS | [`drop_curve`]: the exact foot curve; no parameter box (the analytic kinds are unbounded but the cone's nappe) and no seam split (a face's seam is the body's) |
//! | `Error::Argument` for a direction of no length or not finite | none: `D` is a [`UnitVec3`], valid by construction |
//! | `Error::Argument` for a tolerance outside `[1e-9, 1e3)` | none: a [`Resolution`] is finite and above zero by construction |
//! | `Error::Empty` when nothing lands | `Ok(vec![])`: a projection that misses is a final, empty answer |
//! | curve kinds other than line, arc and B-spline: `Error::Argument` | every kind is taken; a pair with no closed form is [`ProjectError::NeedsFitter`] |
//! | every result fitted within `tolerance` | closed forms only; [`Projected::kept`] says where the parameter is kept |

use core::fmt;

use scx_curve::{Circle, Curve, CurveError, Line, ParamShift, Part};
use scx_num::error::ErrorKind;
use scx_num::extent1d::Extent1d;
use scx_num::fmath;
use scx_num::frame::{Frame, Sense};
use scx_num::math::Affine;
use scx_num::units::{Length, Resolution};
use scx_num::vector3d::{Point3, UnitVec3, Vec3};

use crate::common::Axes;
use crate::pcurve::{CurveKind, SurfaceKind};
use crate::{Cone, Cylinder, Plane, Surface};

/// One piece of a projected or dropped curve.
#[derive(Clone, Debug, PartialEq)]
pub struct Projected {
    /// The image's curve.
    pub curve: Curve,
    /// The image over [`Self::from`], on [`Self::curve`]'s parameter, run the same way: the
    /// image of the source's point at `from.min()` is `curve`'s at `extent.min()`.
    pub extent: Extent1d,
    /// The part of the source's extent this piece is the image of.
    pub from: Extent1d,
    /// `Some(shift)` where the image of the source's point at `t` is `curve`'s point at
    /// `shift.apply(t)` (every affine image: the plane's, the rulings and the generators);
    /// `None` where only the ends and the order are kept (a cylinder's arc).
    pub kept: Option<ParamShift>,
}

/// Why a curve cannot be projected or dropped onto a surface.
#[derive(Clone, Copy, Debug, PartialEq)]
#[non_exhaustive]
pub enum ProjectError {
    /// The pair has no closed form here; the fitter (step 5) answers it. Named so that the
    /// fitter's path replaces exactly this refusal.
    NeedsFitter {
        /// The curve's kind.
        curve: CurveKind,
        /// The surface's kind.
        surface: SurfaceKind,
    },
    /// The direction lies in the plane, or runs along the cylinder's axis: a projection line
    /// lies on the surface or misses it.
    Along,
    /// The image is a point, or a conic seen edge-on (a segment run twice): not a curve.
    Flattened,
    /// The line lies on the axis of a cylinder or cone: every point of a circle is a foot.
    OnAxis,
    /// The image could not be built as a curve (an overflow): the curve's own cause.
    Image(CurveError),
}

impl ProjectError {
    /// The cause's kind: [`ErrorKind::Unsupported`] for [`ProjectError::NeedsFitter`], the
    /// curve's own for [`ProjectError::Image`], [`ErrorKind::Refused`] for the rest.
    #[must_use]
    pub const fn kind(&self) -> ErrorKind {
        match self {
            Self::NeedsFitter { .. } => ErrorKind::Unsupported,
            Self::Along | Self::Flattened | Self::OnAxis => ErrorKind::Refused,
            Self::Image(e) => e.kind(),
        }
    }

    /// The cause's stable name (62 D6).
    #[must_use]
    pub const fn name(&self) -> &'static str {
        match self {
            Self::NeedsFitter { .. } => "projection_needs_fitter",
            Self::Along => "projection_along_surface",
            Self::Flattened => "projection_flattened",
            Self::OnAxis => "projection_on_axis",
            Self::Image(e) => e.name(),
        }
    }
}

impl fmt::Display for ProjectError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NeedsFitter { curve, surface } => write!(
                f,
                "projecting a {curve:?} curve onto a {surface:?} surface has no closed form; \
                 it needs the fitter"
            ),
            Self::Along => f.write_str("the projection direction runs along the surface"),
            Self::Flattened => f.write_str("the curve's image is a point or a segment"),
            Self::OnAxis => f.write_str("the curve lies on the surface's axis"),
            Self::Image(e) => write!(f, "the image could not be built: {e}"),
        }
    }
}

impl std::error::Error for ProjectError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Image(e) => Some(e),
            _ => None,
        }
    }
}

/// `c` over `ce` projected onto `s` along `along`, both ways along each projection line
/// (module docs: the closed forms, the pieces and the tolerance).
///
/// # Errors
///
/// [`ProjectError::NeedsFitter`] for a pair with no closed form; [`ProjectError::Along`]
/// where `along` lies in the plane or along the cylinder's axis; [`ProjectError::Flattened`]
/// where the image has no length or is a conic seen edge-on; [`ProjectError::Image`] on
/// overflow.
pub fn project_curve(
    c: &Curve,
    ce: Extent1d,
    s: &Surface,
    along: UnitVec3,
    res: Resolution,
) -> Result<Vec<Projected>, ProjectError> {
    let zone = res.zone().mm();
    match (s, c) {
        (Surface::Plane(p), _) => onto_plane(c, ce, p, *along).map(|p| vec![p]),
        (Surface::Cylinder(cy), Curve::Line(l)) => project_cylinder(l, ce, cy, *along, zone)
            .and_then(|r| r.ok_or_else(|| needs_fitter(c, s))),
        (Surface::Cone(co), Curve::Line(l)) => {
            project_cone(l, ce, co, *along, zone).and_then(|r| r.ok_or_else(|| needs_fitter(c, s)))
        }
        _ => Err(needs_fitter(c, s)),
    }
}

/// `c` over `ce` dropped onto `s`: each point to its nearest point on the surface (module
/// docs: the closed forms, the pieces and the tolerance).
///
/// # Errors
///
/// [`ProjectError::NeedsFitter`] for a pair with no closed form; [`ProjectError::OnAxis`]
/// for a line on a cylinder's or cone's axis; [`ProjectError::Flattened`] where the image
/// has no length or is a conic seen edge-on; [`ProjectError::Image`] on overflow.
pub fn drop_curve(
    c: &Curve,
    ce: Extent1d,
    s: &Surface,
    res: Resolution,
) -> Result<Vec<Projected>, ProjectError> {
    let zone = res.zone().mm();
    match (s, c) {
        (Surface::Plane(p), _) => onto_plane(c, ce, p, *p.frame().z()).map(|p| vec![p]),
        (Surface::Cylinder(cy), Curve::Line(l)) => {
            drop_cylinder(l, ce, cy, zone).and_then(|r| r.ok_or_else(|| needs_fitter(c, s)))
        }
        (Surface::Cone(co), Curve::Line(l)) => {
            drop_cone(l, ce, co, zone).and_then(|r| r.ok_or_else(|| needs_fitter(c, s)))
        }
        _ => Err(needs_fitter(c, s)),
    }
}

fn needs_fitter(c: &Curve, s: &Surface) -> ProjectError {
    ProjectError::NeedsFitter {
        curve: CurveKind::of(c),
        surface: SurfaceKind::of(s),
    }
}

// ---------------------------------------------------------------- the plane

/// The image of `c` under the projection along `d` onto `p`: the affine map
/// `x -> x - D ((x - o) . n) / (D . n)`, whose linear part has the columns
/// `e_i - D n_i / (D . n)`. `Curve::transformed` places a conic's image on its principal
/// axes (a circle's is its exact ellipse, or a circle where the map is a similarity on its
/// plane) and maps a NURBS curve's control points, so every kind's image is exact.
fn onto_plane(c: &Curve, ce: Extent1d, p: &Plane, d: Vec3) -> Result<Projected, ProjectError> {
    let n = *p.frame().z();
    let dn = d.dot(n);
    if dn == 0.0 {
        return Err(ProjectError::Along);
    }
    let column = |e: Vec3, ni: f64| e - d * (ni / dn);
    let shift = d * (p.frame().origin().to_vec().dot(n) / dn);
    let map = Affine::from_axes(
        column(Vec3::X, n.x),
        column(Vec3::Y, n.y),
        column(Vec3::Z, n.z),
        Point3::new(shift.x, shift.y, shift.z),
    );
    let (curve, kept) = c.transformed(&map).map_err(|e| match e {
        CurveError::Degenerate(_) => ProjectError::Flattened,
        e => ProjectError::Image(e),
    })?;
    let extent = kept
        .extent(ce)
        .ok_or(ProjectError::Image(CurveError::NotFinite(Part::Map)))?;
    Ok(Projected {
        curve,
        extent,
        from: ce,
        kept: Some(kept),
    })
}

// ---------------------------------------------------------------- revolved kinds

/// A line in a plane through an axis: `c(t) = O + (s0 + t s1) e + (h0 + t h1) Z` within the
/// zone, `e` the unit radial direction of the plane (signed distance from the axis
/// `sigma = s0 + t s1`).
#[derive(Clone, Copy, Debug)]
struct Meridian {
    e: Vec3,
    s0: f64,
    s1: f64,
    h0: f64,
    h1: f64,
}

impl Meridian {
    /// The pieces of `ce` on each side of the axis (`sigma > 0` and `sigma < 0`), each with
    /// its side's sign, in `t` order.
    fn sides(&self, ce: Extent1d) -> Vec<(Extent1d, f64)> {
        let mut out: Vec<(Extent1d, f64)> = [1.0, -1.0]
            .into_iter()
            .filter_map(|sg| positive_part(ce, sg * self.s0, sg * self.s1).map(|e| (e, sg)))
            .collect();
        out.sort_by(|a, b| a.0.min().total_cmp(&b.0.min()));
        out
    }
}

/// The line `l` over `ce` in `a`'s local terms when it lies in a plane through the axis
/// within `zone`; `None` when it does not.
///
/// # Errors
///
/// [`ProjectError::OnAxis`] for a line parallel to the axis and within the zone of it.
fn meridian(a: &Axes, l: &Line, ce: Extent1d, zone: f64) -> Result<Option<Meridian>, ProjectError> {
    let (ox, oy, h0) = a.local(l.origin());
    let d = l.direction();
    let (dx, dy, h1) = (d.dot(a.x), d.dot(a.y), d.dot(a.z));
    let lxy = (dx * dx + dy * dy).sqrt();
    if lxy * ce.length() <= zone {
        // Parallel to the axis: every point at the distance of the middle's, within the zone.
        let (qx, qy, _) = a.local(l.point(ce.mid()));
        let q = (qx * qx + qy * qy).sqrt();
        if q <= zone {
            return Err(ProjectError::OnAxis);
        }
        let e = a.x * (qx / q) + a.y * (qy / q);
        return Ok(Some(Meridian {
            e,
            s0: q,
            s1: 0.0,
            h0,
            h1,
        }));
    }
    let off = (ox * dy - oy * dx).abs() / lxy;
    if off > zone {
        return Ok(None);
    }
    let (ex, ey) = (dx / lxy, dy / lxy);
    Ok(Some(Meridian {
        e: a.x * ex + a.y * ey,
        s0: ox * ex + oy * ey,
        s1: lxy,
        h0,
        h1,
    }))
}

/// The part of `e` where `f0 + t f1 > 0`, `None` where that is empty or one value.
fn positive_part(e: Extent1d, f0: f64, f1: f64) -> Option<Extent1d> {
    if f1 == 0.0 {
        return (f0 > 0.0).then_some(e);
    }
    let root = -f0 / f1;
    let part = if f1 > 0.0 {
        e.with_min(root.max(e.min()))
    } else {
        e.with_max(root.min(e.max()))
    }?;
    (part.length() > 0.0).then_some(part)
}

/// The pieces built so far, or [`ProjectError::Flattened`] where every piece had no length.
fn kept_or_flat(out: Vec<Projected>, collapsed: bool) -> Result<Vec<Projected>, ProjectError> {
    if out.is_empty() && collapsed {
        Err(ProjectError::Flattened)
    } else {
        Ok(out)
    }
}

/// The line `origin + t dir` over `piece`, the image of the source over `piece` on the same
/// parameter; `None` where it has no length (within `zone`).
fn line_piece(
    origin: Point3,
    dir: Vec3,
    piece: Extent1d,
    zone: f64,
) -> Result<Option<Projected>, ProjectError> {
    if dir.length() * piece.length() <= zone {
        return Ok(None);
    }
    let curve = Line::new(origin, dir).map_err(ProjectError::Image)?;
    Ok(Some(Projected {
        curve: curve.into(),
        extent: piece,
        from: piece,
        kept: Some(ParamShift::NONE),
    }))
}

/// A line parallel to the cylinder's axis, projected along `d`: the rulings where the
/// projection lines through the line meet it. `None` for any other line.
fn project_cylinder(
    l: &Line,
    ce: Extent1d,
    cy: &Cylinder,
    d: Vec3,
    zone: f64,
) -> Result<Option<Vec<Projected>>, ProjectError> {
    let a = cy.axes();
    let dir = l.direction();
    let (dx, dy, h1) = (dir.dot(a.x), dir.dot(a.y), dir.dot(a.z));
    if (dx * dx + dy * dy).sqrt() * ce.length() > zone {
        return Ok(None);
    }
    let (ux, uy, uz) = (d.dot(a.x), d.dot(a.y), d.dot(a.z));
    let dxy = (ux * ux + uy * uy).sqrt();
    if dxy == 0.0 {
        return Err(ProjectError::Along);
    }
    let (ux, uy) = (ux / dxy, uy / dxy);
    let r = cy.radius().mm();
    let (qx, qy, _) = a.local(l.point(ce.mid()));
    let (_, _, h0) = a.local(l.origin());
    // The projection line in the cross-section: q + w u, w = s dxy; it passes the axis at
    // the signed distance `off`.
    let beta = qx * ux + qy * uy;
    let off = qx * uy - qy * ux;
    if off.abs() > r + zone {
        return Ok(Some(Vec::new()));
    }
    let half = if (off.abs() - r).abs() <= zone {
        0.0
    } else {
        (r * r - off * off).sqrt()
    };
    let ws: &[f64] = if half == 0.0 {
        &[-beta]
    } else {
        &[-beta - half, -beta + half]
    };
    let mut out = Vec::with_capacity(ws.len());
    let mut collapsed = false;
    for &w in ws {
        let (px, py) = (qx + w * ux, qy + w * uy);
        let rho = (px * px + py * py).sqrt();
        let (px, py) = (px * (r / rho), py * (r / rho));
        let s = w / dxy;
        let origin = a.o + a.x * px + a.y * py + a.z * (h0 + s * uz);
        match line_piece(origin, a.z * h1, ce, zone)? {
            Some(p) => out.push(p),
            None => collapsed = true,
        }
    }
    kept_or_flat(out, collapsed).map(Some)
}

/// A line in a plane through the cone's axis, projected along the axis: each side's
/// generator, at the radius of the source point (`r + k v = rho`). `None` for another line
/// or a direction off the axis by more than the zone over the projection's reach.
fn project_cone(
    l: &Line,
    ce: Extent1d,
    co: &Cone,
    d: Vec3,
    zone: f64,
) -> Result<Option<Vec<Projected>>, ProjectError> {
    let a = co.axes();
    let dz = d.dot(a.z);
    let off_axis = (d - a.z * dz).length();
    let Some(m) = meridian(&a, l, ce, zone)? else {
        return Ok(None);
    };
    let (r, k) = (co.radius().mm(), co.slope());
    let mut out = Vec::new();
    let mut collapsed = false;
    for (piece, sg) in m.sides(ce) {
        // v(t) = (sg sigma(t) - r) / k; the point o + sigma e + v Z.
        let v0 = (sg * m.s0 - r) / k;
        let v1 = sg * m.s1 / k;
        // How far the projection runs at the piece's ends: off-axis by `off_axis` per unit.
        let reach = [piece.min(), piece.max()]
            .into_iter()
            .map(|t| ((v0 + t * v1) - (m.h0 + t * m.h1)).abs())
            .fold(0.0, f64::max);
        if off_axis * reach > zone {
            return Ok(None);
        }
        let origin = a.o + m.e * m.s0 + a.z * v0;
        let dir = m.e * m.s1 + a.z * v1;
        match line_piece(origin, dir, piece, zone)? {
            Some(p) => out.push(p),
            None => collapsed = true,
        }
    }
    kept_or_flat(out, collapsed).map(Some)
}

/// A line dropped onto a cylinder: in a plane through the axis, each side's ruling (the
/// foot keeps the height); square to the axis and off it, the arc at its height. `None`
/// for any other line.
fn drop_cylinder(
    l: &Line,
    ce: Extent1d,
    cy: &Cylinder,
    zone: f64,
) -> Result<Option<Vec<Projected>>, ProjectError> {
    let a = cy.axes();
    let r = cy.radius().mm();
    if let Some(m) = meridian(&a, l, ce, zone)? {
        let mut out = Vec::new();
        let mut collapsed = false;
        for (piece, sg) in m.sides(ce) {
            let origin = a.o + m.e * (sg * r) + a.z * m.h0;
            match line_piece(origin, a.z * m.h1, piece, zone)? {
                Some(p) => out.push(p),
                None => collapsed = true,
            }
        }
        return kept_or_flat(out, collapsed).map(Some);
    }
    let h1 = l.direction().dot(a.z);
    if h1.abs() * ce.length() > zone {
        return Ok(None);
    }
    // Square to the axis and off it: the feet are the circle of radius r at the height of
    // the middle, from the angle of one end to the other's, less than a half turn apart.
    let (_, _, h) = a.local(l.point(ce.mid()));
    let yy = a.z.cross(a.x);
    let at = |t: f64| {
        let p = l.point(t) - a.o;
        (p.dot(a.x), p.dot(yy))
    };
    let ((xa, ya), (xb, yb)) = (at(ce.min()), at(ce.max()));
    let start = fmath::atan2(ya, xa);
    let turn = fmath::atan2(xa * yb - ya * xb, xa * xb + ya * yb);
    if r * turn.abs() <= zone {
        return Err(ProjectError::Flattened);
    }
    let frame = Frame::new(a.o + a.z * h, a.z, a.x)
        .ok_or(ProjectError::Image(CurveError::NotFinite(Part::Origin)))?;
    let (sense, lo, hi) = if turn > 0.0 {
        (Sense::Forward, start, start + turn)
    } else {
        (Sense::Reversed, -start, -start - turn)
    };
    let circle = Circle::new(frame, Length::from_mm(r), sense).map_err(ProjectError::Image)?;
    let extent =
        Extent1d::new(lo, hi).ok_or(ProjectError::Image(CurveError::NotFinite(Part::Map)))?;
    Ok(Some(vec![Projected {
        curve: circle.into(),
        extent,
        from: ce,
        kept: None,
    }]))
}

/// A line in a plane through the cone's axis, dropped onto it: on each side, the foot on
/// the generator, `v = ((rho - r) k + h) / (k^2 + 1)`, where it lies on the nappe
/// (`r + k v > 0`, that is `r + k^2 rho + k h > 0`); the part whose feet are the apex is
/// left out. `None` for any other line.
fn drop_cone(
    l: &Line,
    ce: Extent1d,
    co: &Cone,
    zone: f64,
) -> Result<Option<Vec<Projected>>, ProjectError> {
    let a = co.axes();
    let Some(m) = meridian(&a, l, ce, zone)? else {
        return Ok(None);
    };
    let (r, k) = (co.radius().mm(), co.slope());
    let kk = k * k + 1.0;
    let mut out = Vec::new();
    let mut collapsed = false;
    for (side, sg) in m.sides(ce) {
        // rho = sg sigma = sg s0 + t sg s1 on this side.
        let (p0, p1) = (sg * m.s0, sg * m.s1);
        let Some(piece) = positive_part(side, r + k * k * p0 + k * m.h0, k * k * p1 + k * m.h1)
        else {
            collapsed = true;
            continue;
        };
        let v0 = ((p0 - r) * k + m.h0) / kk;
        let v1 = (p1 * k + m.h1) / kk;
        // The foot o + sg (r + k v) e + v Z.
        let origin = a.o + m.e * (sg * (r + k * v0)) + a.z * v0;
        let dir = m.e * (sg * k * v1) + a.z * v1;
        match line_piece(origin, dir, piece, zone)? {
            Some(p) => out.push(p),
            None => collapsed = true,
        }
    }
    kept_or_flat(out, collapsed).map(Some)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kinds_and_names() {
        let n = ProjectError::NeedsFitter {
            curve: CurveKind::Circle,
            surface: SurfaceKind::Torus,
        };
        assert_eq!(
            (n.kind(), n.name()),
            (ErrorKind::Unsupported, "projection_needs_fitter")
        );
        assert!(n.to_string().contains("fitter"));
        for (e, name) in [
            (ProjectError::Along, "projection_along_surface"),
            (ProjectError::Flattened, "projection_flattened"),
            (ProjectError::OnAxis, "projection_on_axis"),
        ] {
            assert_eq!((e.kind(), e.name()), (ErrorKind::Refused, name));
        }
    }

    #[test]
    fn positive_parts() {
        let e = Extent1d::new(0.0, 4.0).unwrap();
        assert_eq!(positive_part(e, -1.0, 1.0), Extent1d::new(1.0, 4.0));
        assert_eq!(positive_part(e, 1.0, -1.0), Extent1d::new(0.0, 1.0));
        assert_eq!(positive_part(e, 1.0, 0.0), Some(e));
        assert_eq!(positive_part(e, -1.0, 0.0), None);
        assert_eq!(positive_part(e, 4.0, -1.0), Some(e));
        assert_eq!(positive_part(e, 0.0, -1.0), None);
    }
}
