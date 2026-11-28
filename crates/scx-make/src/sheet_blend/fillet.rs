// Written for solidcorex. Apache-2.0.
//! [`fillet_sheets`]: the round between two sheets along the edge they share, built exactly.
//!
//! In the plane across the edge the two sheets leave it along their interior directions
//! `i_a` and `i_b` (each `n x T` of the face's use, the side the face lies on), at the angle
//! `theta` between them. The circle of radius `r` touching both lines has its centre on the
//! bisector at `r / sin(theta/2)` from the edge, touches each at the setback
//! `r / tan(theta/2)`, spans `pi - theta`, and has the chord `2 r cos(theta/2)` between its
//! contacts. Swept along a line (two planes meeting on a line) it is a cylinder; turned round
//! an axis (two surfaces of revolution about the axis of the circle they share) a torus, or a
//! sphere when the centre is on the axis. Every piece is that closed form: the round's
//! surface, its uv box and the contact curves on the sheets, so nothing is sampled or fitted.

use scx_boolean::{Cut, imprint_curves};
use scx_brep::op::Cx;
use scx_brep::{Brep, Document, EdgeId, EdgeShape, FaceId, UseGeom};
use scx_curve::{Circle, Curve, Line};
use scx_num::extent1d::Extent1d;
use scx_num::extent2d::Extent2d;
use scx_num::fmath;
use scx_num::frame::{Frame, Sense};
use scx_num::units::Length;
use scx_num::vector2d::Point2;
use scx_num::vector3d::{Point3, Vec3};
use scx_nurbs::Approach;
use scx_surface::{Cylinder, Sphere, Surface, Torus};

use super::error::{
    SheetBlendError, SheetBlendRefusal, SheetBlendUnsupported, refused, unsupported,
};
use super::trim::keep_faces;
use crate::Made;
use crate::face::{Bound, face};

/// The size and shape of a round's section.
#[derive(Debug, Clone, Copy, PartialEq)]
#[non_exhaustive]
pub enum FilletSection {
    /// A circular section of this radius.
    Radius(Length),
    /// A circular section whose contacts are this far apart: the radius is
    /// `chord / (2 cos(theta/2))` at the sheets' angle `theta`.
    Chord(Length),
    /// A curvature-continuous section of this setback: not built
    /// ([`SheetBlendUnsupported::Curvature`]).
    Curvature(Length),
}

/// The surface a round is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum Round {
    /// Two planes meeting on a line: a cylinder about the line through the centres.
    Cylinder,
    /// Two surfaces of revolution about the axis of the circle they share: a torus round
    /// that axis.
    Torus,
    /// As [`Round::Torus`] with the centre on the axis.
    Sphere,
}

/// Where a round touches a sheet: a curve over an interval of its parameter, the sheet's
/// edge's own (the contact is the edge moved by the setback, on the same parameter).
#[derive(Debug, Clone, PartialEq)]
pub struct Contact {
    /// The curve.
    pub curve: Curve,
    /// Its part that touches.
    pub interval: Extent1d,
}

/// A round between two sheets, as a one-face sheet of its own.
#[derive(Debug, Clone)]
pub struct Fillet {
    /// The round's sheet and its face.
    pub made: Made<FaceId>,
    /// Its surface's kind.
    pub round: Round,
    /// The section's radius.
    pub radius: Length,
    /// How far each contact is from the shared edge, along each sheet.
    pub setback: Length,
    /// Where it touches the first and the second sheet.
    pub contacts: [Contact; 2],
}

/// The round and the two sheets cut back to it.
#[derive(Debug, Clone)]
pub struct FilletSheets {
    /// The round.
    pub fillet: Fillet,
    /// The first and second sheet, each with the strip between the shared edge and the
    /// contact removed: the contact is their new boundary edge.
    pub sheets: [Brep; 2],
}

/// The shared edge and what the section is built from.
struct Across {
    /// The face of each sheet along the shared edge.
    faces: [FaceId; 2],
    /// The edge's curve and interval (the first sheet's).
    curve: Curve,
    interval: Extent1d,
    /// The point at the middle of the interval, and the interior directions there.
    p: Point3,
    inward: [Vec3; 2],
}

/// The section's circle in the plane across the edge at `p`.
struct Section {
    radius: f64,
    setback: f64,
    centre: Point3,
    contacts: [Point3; 2],
}

const SIDES: [Approach; 2] = [Approach::Above; 2];

impl Fillet {
    /// The round of `section` between sheets `a` and `b` along the edge they share, without
    /// cutting the sheets.
    ///
    /// # Errors
    ///
    /// - [`SheetBlendRefusal::NotSheet`], [`SheetBlendRefusal::OtherDocument`];
    /// - [`SheetBlendRefusal::NoSharedEdge`] when no boundary edge of one coincides with one
    ///   of the other (ends and middle within the resolution, both lines or both circles),
    ///   [`SheetBlendRefusal::SharedEdges`] when more than one does;
    /// - [`SheetBlendRefusal::Angle`] for sheets folded onto each other or continuing flat
    ///   (the angle below the resolution at the round's radius), [`SheetBlendRefusal::Size`];
    /// - [`SheetBlendUnsupported::Curvature`], [`SheetBlendUnsupported::NurbsSheet`],
    ///   [`SheetBlendUnsupported::RollingBall`] for a round that is not a cylinder, torus or
    ///   sphere, [`SheetBlendUnsupported::SpindleTorus`];
    /// - [`SheetBlendError::Make`] when the round's face does not build.
    pub fn between(
        a: &Brep,
        b: &Brep,
        section: FilletSection,
        doc: &Document,
    ) -> Result<Self, SheetBlendError> {
        let res = doc.resolution().zone().mm();
        for (operand, s) in [a, b].into_iter().enumerate() {
            if s.resolution() != doc.resolution() {
                return Err(refused(SheetBlendRefusal::OtherDocument { operand }));
            }
            if s.regions().any(scx_brep::Region::is_solid) {
                return Err(refused(SheetBlendRefusal::NotSheet { operand }));
            }
        }
        let across = shared(a, b, res)?;
        let surfaces = [
            a.face(across.faces[0]).map(|f| f.surface().clone()),
            b.face(across.faces[1]).map(|f| f.surface().clone()),
        ];
        let [Some(sa), Some(sb)] = surfaces else {
            return Err(SheetBlendError::Defect {
                why: "the faces along the shared edge",
            });
        };
        if matches!(sa, Surface::Nurbs(_)) || matches!(sb, Surface::Nurbs(_)) {
            return Err(unsupported(SheetBlendUnsupported::NurbsSheet));
        }
        let sec = circle_across(&across, section, res)?;
        match &across.curve {
            Curve::Line(line) if is_plane(&sa) && is_plane(&sb) => {
                cylinder_round(&across, line, &sec, *doc)
            }
            Curve::Circle(circle) if about(&sa, circle, res) && about(&sb, circle, res) => {
                revolved_round(&across, circle, &sec, res, *doc)
            }
            _ => Err(unsupported(SheetBlendUnsupported::RollingBall)),
        }
    }
}

/// The round of `section` between sheets `a` and `b` along the edge they share
/// ([`Fillet::between`]), and the sheets cut back to its contacts.
///
/// # Errors
///
/// As [`Fillet::between`]; [`SheetBlendUnsupported::CurvedImprint`] when a sheet's face along
/// the edge is not a plane; [`SheetBlendRefusal::Setback`] when the contact leaves no strip
/// to remove or nothing to keep (a setback past the face); [`SheetBlendError::Boolean`] for
/// the imprint's own refusals; [`SheetBlendError::Make`] when a cut sheet does not finish.
pub fn fillet_sheets(
    a: &Brep,
    b: &Brep,
    section: FilletSection,
    cx: &mut Cx<'_>,
) -> Result<FilletSheets, SheetBlendError> {
    let fillet = Fillet::between(a, b, section, cx.doc)?;
    let res = cx.doc.resolution().zone().mm();
    let across = shared(a, b, res)?;
    let setback = fillet.setback.mm();
    let mut cut = |f: FaceId, inward: Vec3, contact: &Contact, body: &Brep| {
        let plane = body.face(f).is_some_and(|f| is_plane(f.surface()));
        if !plane {
            return Err(unsupported(SheetBlendUnsupported::CurvedImprint));
        }
        let whole = match &contact.curve {
            Curve::Line(_) => true,
            _ => contact.interval.length() >= core::f64::consts::TAU - res,
        };
        let c = if whole {
            Cut::whole(contact.curve.clone())
        } else {
            Cut::over(contact.curve.clone(), contact.interval)
        };
        let done = imprint_curves(body, f, &[c], cx)?;
        let strip = |q: Point3| in_strip(&across, inward, setback, q);
        let gone: Vec<FaceId> = done
            .regions
            .iter()
            .filter(|r| strip(r.inside))
            .map(|r| r.face)
            .collect();
        if gone.is_empty() || gone.len() == done.regions.len() {
            return Err(refused(SheetBlendRefusal::Setback { setback }));
        }
        keep_faces(&done.done.body, &gone, *cx.doc)
    };
    let ([fa, fb], [ia, ib]) = (across.faces, across.inward);
    let [ca, cb] = &fillet.contacts;
    let sheets = [cut(fa, ia, ca, a)?, cut(fb, ib, cb, b)?];
    Ok(FilletSheets { fillet, sheets })
}

/// Whether `q` (a point of the sheet whose interior lies along `i` from the edge) lies
/// between the shared edge and the contact.
fn in_strip(across: &Across, i: Vec3, setback: f64, q: Point3) -> bool {
    if let Curve::Circle(c) = &across.curve {
        // The radial distance from the axis, against the edge's and the contact's.
        let o = c.frame().origin();
        let z = Vec3::from(c.frame().z());
        let radial = |p: Point3| {
            let d = p - o;
            (d - z * d.dot(z)).length()
        };
        // A plane across the axis (the only sheet cut here): the strip is the annulus
        // between the edge's circle and the contact's.
        let edge = c.radius().mm();
        let reach = radial(across.p + i * setback);
        let r = radial(q);
        r > edge.min(reach) && r < edge.max(reach)
    } else {
        let s = (q - across.p).dot(i);
        s > 0.0 && s < setback
    }
}

fn is_plane(s: &Surface) -> bool {
    matches!(s, Surface::Plane(_))
}

/// Whether `s` is a surface of revolution about `circle`'s axis (within `res`): a plane
/// across it, or a cylinder, cone, sphere or torus round it.
fn about(s: &Surface, circle: &Circle, res: f64) -> bool {
    let o = circle.frame().origin();
    let z = Vec3::from(circle.frame().z());
    let on_axis = |p: Point3| {
        let d = p - o;
        (d - z * d.dot(z)).length() <= res
    };
    let parallel = |w: Vec3| w.cross(z).length() <= res / circle.radius().mm().max(res);
    let frame_about = |f: &Frame| parallel(Vec3::from(f.z())) && on_axis(f.origin());
    match s {
        Surface::Plane(p) => parallel(Vec3::from(p.frame().z())),
        Surface::Cylinder(c) => frame_about(c.frame()),
        Surface::Cone(c) => frame_about(c.frame()),
        Surface::Torus(t) => frame_about(t.frame()),
        Surface::Sphere(sp) => on_axis(sp.frame().origin()),
        _ => false,
    }
}

/// The boundary edge `a` and `b` share, with the interior directions of their faces at its
/// middle.
fn shared(a: &Brep, b: &Brep, res: f64) -> Result<Across, SheetBlendError> {
    let boundary = |s: &Brep| -> Vec<EdgeId> {
        s.edges()
            .filter(|e| e.fins().len() == 1)
            .filter(|e| {
                matches!(
                    e.shape(),
                    EdgeShape::Curve {
                        curve: Curve::Line(_) | Curve::Circle(_),
                        ..
                    }
                )
            })
            .map(scx_brep::Edge::id)
            .collect()
    };
    let points = |s: &Brep, e: EdgeId| -> Option<[Point3; 3]> {
        let edge = s.edge(e)?;
        let EdgeShape::Curve { curve, interval } = edge.shape() else {
            return None;
        };
        let [v0, v1] = edge.ends();
        let mid = curve.point(0.5 * (interval.min() + interval.max()));
        Some([s.vertex(v0)?.point(), s.vertex(v1)?.point(), mid])
    };
    let mut found = Vec::new();
    for ea in boundary(a) {
        let Some([a0, a1, am]) = points(a, ea) else {
            continue;
        };
        for eb in boundary(b) {
            let Some([b0, b1, bm]) = points(b, eb) else {
                continue;
            };
            let ends = (a0.distance(b0) <= res && a1.distance(b1) <= res)
                || (a0.distance(b1) <= res && a1.distance(b0) <= res);
            let kind = |s: &Brep, e| {
                s.edge(e).map(|e| match e.shape() {
                    EdgeShape::Curve { curve, .. } => matches!(curve, Curve::Line(_)),
                    EdgeShape::Collapsed => false,
                })
            };
            if ends && am.distance(bm) <= res && kind(a, ea) == kind(b, eb) {
                found.push([ea, eb]);
            }
        }
    }
    let [edges] = found[..] else {
        return Err(refused(if found.is_empty() {
            SheetBlendRefusal::NoSharedEdge
        } else {
            SheetBlendRefusal::SharedEdges { count: found.len() }
        }));
    };
    let edge = a
        .edge(edges[0])
        .ok_or_else(|| refused(SheetBlendRefusal::NotEdge(edges[0])))?;
    let EdgeShape::Curve { curve, interval } = edge.shape() else {
        return Err(refused(SheetBlendRefusal::NotEdge(edges[0])));
    };
    let t = 0.5 * (interval.min() + interval.max());
    let p = curve.point(t);
    let (fa, ia) = interior(a, edges[0], p, res)?;
    let (fb, ib) = interior(b, edges[1], p, res)?;
    Ok(Across {
        faces: [fa, fb],
        curve: curve.clone(),
        interval: *interval,
        p,
        inward: [ia, ib],
    })
}

/// The face of `body` along boundary edge `e`, and its interior direction at `p` on the
/// edge: `n x T` of the face's use of it, the side the face lies on.
fn interior(
    body: &Brep,
    e: EdgeId,
    p: Point3,
    res: f64,
) -> Result<(FaceId, Vec3), SheetBlendError> {
    let defect = |why| SheetBlendError::Defect { why };
    let edge = body
        .edge(e)
        .ok_or_else(|| refused(SheetBlendRefusal::NotEdge(e)))?;
    let EdgeShape::Curve { curve, interval } = edge.shape() else {
        return Err(refused(SheetBlendRefusal::NotEdge(e)));
    };
    // The edge's parameter at `p`: its own middle, or the other sheet's (within `res`).
    let t = 0.5 * (interval.min() + interval.max());
    if curve.point(t).distance(p) > res {
        return Err(refused(SheetBlendRefusal::NoSharedEdge));
    }
    let fin = edge
        .fins()
        .next()
        .ok_or_else(|| defect("a boundary edge's fin"))?;
    let f = body
        .face(fin.face)
        .ok_or_else(|| defect("the fin's face"))?;
    let u = f
        .uses()
        .find(|u| u.record().edge == e)
        .ok_or_else(|| defect("the face's use of the edge"))?;
    let UseGeom::Pcurve { curve: pc, .. } = u.geom() else {
        return Err(defect("a pcurve of the edge"));
    };
    let uv: Point2 = pc.point(t);
    let jet = curve.jet::<1>(t, Approach::Above);
    let tangent = match u.sense() {
        Sense::Forward => jet.d[0],
        Sense::Reversed => -jet.d[0],
    };
    let n = f
        .surface()
        .normal(uv, SIDES)
        .ok_or_else(|| defect("the face's normal at the edge"))?;
    let i = Vec3::from(n)
        .cross(tangent)
        .normalized()
        .ok_or_else(|| refused(SheetBlendRefusal::NotFinite))?;
    Ok((f.id(), Vec3::from(i)))
}

/// The section's circle across the edge at its middle.
fn circle_across(
    across: &Across,
    section: FilletSection,
    res: f64,
) -> Result<Section, SheetBlendError> {
    let [ia, ib] = across.inward;
    let cos = ia.dot(ib).clamp(-1.0, 1.0);
    let theta = fmath::acos(cos);
    let half = 0.5 * theta;
    let radius = match section {
        FilletSection::Radius(r) => r.mm(),
        FilletSection::Chord(c) => {
            let c = c.mm();
            if !(c.is_finite() && c > res) {
                return Err(refused(SheetBlendRefusal::Size { value: c }));
            }
            c / (2.0 * fmath::cos(half))
        }
        FilletSection::Curvature(_) => {
            return Err(unsupported(SheetBlendUnsupported::Curvature));
        }
    };
    if !(radius.is_finite() && radius > res) {
        return Err(refused(SheetBlendRefusal::Size { value: radius }));
    }
    // Folded (no room between the sheets) or flat (nothing to round), as seen at the
    // round's own size.
    if radius * theta <= res || radius * (core::f64::consts::PI - theta) <= res {
        return Err(refused(SheetBlendRefusal::Angle { angle: theta }));
    }
    let (s, c) = fmath::sin_cos(half);
    let setback = radius * c / s;
    let bisector = (ia + ib)
        .normalized()
        .ok_or_else(|| refused(SheetBlendRefusal::Angle { angle: theta }))?;
    let centre = across.p + Vec3::from(bisector) * (radius / s);
    Ok(Section {
        radius,
        setback,
        centre,
        contacts: [across.p + ia * setback, across.p + ib * setback],
    })
}

fn make(e: impl Into<crate::MakeError>) -> SheetBlendError {
    SheetBlendError::Make(e.into())
}

fn interval(lo: f64, hi: f64) -> Result<Extent1d, SheetBlendError> {
    Extent1d::new(lo.min(hi), lo.max(hi)).ok_or(SheetBlendError::Defect {
        why: "an interval of the round",
    })
}

/// Two planes along a line: the cylinder about the line through the centre.
fn cylinder_round(
    across: &Across,
    line: &Line,
    sec: &Section,
    doc: Document,
) -> Result<Fillet, SheetBlendError> {
    let defect = |why| SheetBlendError::Defect { why };
    let d = line.direction();
    let axis = Vec3::from(d.normalized().ok_or_else(|| defect("the round's axis"))?);
    let shift = sec.centre - across.p;
    let x = (sec.contacts[0] - sec.centre)
        .normalized()
        .ok_or_else(|| defect("the round's first contact"))?;
    let to_b = sec.contacts[1] - sec.centre;
    // The round turns from the first contact to the second about `z`, the short way.
    let turn = |z: Vec3| {
        let y = z.cross(Vec3::from(x));
        fmath::atan2(to_b.dot(y), to_b.dot(Vec3::from(x)))
    };
    // `axis` is a unit vector: a scaled one would scale the sine against the cosine.
    let z = if turn(axis) >= 0.0 { axis } else { -axis };
    let span = turn(z);
    let origin = line.origin() + shift;
    let frame = Frame::new(origin, z, Vec3::from(x)).ok_or_else(|| defect("the round's frame"))?;
    let cyl = Cylinder::new(frame, Length::from_mm(sec.radius), Sense::Forward)?;
    let zu = Vec3::from(frame.z());
    let height = |t: f64| (line.point(t) - line.origin()).dot(zu);
    let (h0, h1) = (height(across.interval.min()), height(across.interval.max()));
    let lo = cyl.from_step(Point2::new(0.0, h0.min(h1)));
    let hi = cyl.from_step(Point2::new(span, h0.max(h1)));
    let bx = Extent2d::from_intervals(interval(lo.x, hi.x)?, interval(lo.y, hi.y)?);
    let made = face(&Surface::Cylinder(cyl), &Bound::Box(bx), &doc).map_err(make)?;
    let [ia, ib] = across.inward;
    let contact = |inward: Vec3| -> Result<Contact, SheetBlendError> {
        let l = Line::new(line.origin() + inward * sec.setback, d)
            .map_err(|_| defect("a contact line"))?;
        Ok(Contact {
            curve: Curve::Line(l),
            interval: across.interval,
        })
    };
    Ok(Fillet {
        made,
        round: Round::Cylinder,
        radius: Length::from_mm(sec.radius),
        setback: Length::from_mm(sec.setback),
        contacts: [contact(ia)?, contact(ib)?],
    })
}

/// Two surfaces of revolution along a circle of their axis: the torus (or sphere) the
/// section's circle turns into round the axis.
fn revolved_round(
    across: &Across,
    circle: &Circle,
    sec: &Section,
    res: f64,
    doc: Document,
) -> Result<Fillet, SheetBlendError> {
    let defect = |why| SheetBlendError::Defect { why };
    let o = circle.frame().origin();
    let k = Vec3::from(circle.frame().z());
    let foot = |p: Point3| o + k * (p - o).dot(k);
    let centre_foot = foot(sec.centre);
    let major = (sec.centre - centre_foot).length();
    let outside = (sec.centre - centre_foot).dot(Vec3::from(
        (across.p - foot(across.p))
            .normalized()
            .ok_or_else(|| defect("the edge's radial direction"))?,
    ));
    // The meridian frame at the edge's middle: `x` away from the axis, `z` along it in the
    // sense the edge turns.
    let x = (across.p - foot(across.p))
        .normalized()
        .ok_or_else(|| defect("the edge's radial direction"))?;
    let t_mid = 0.5 * (across.interval.min() + across.interval.max());
    let tangent = circle.jet::<1>(t_mid, Approach::Above).d[0];
    let z = if tangent.dot(k.cross(Vec3::from(x))) >= 0.0 {
        k
    } else {
        -k
    };
    let xv = Vec3::from(x);
    let half_span = 0.5 * across.interval.length();
    let meridian = |p: Point3, c: Point3| {
        let w = p - c;
        fmath::atan2(w.dot(z), w.dot(xv))
    };
    let radius = Length::from_mm(sec.radius);
    let (surface, round, v) = if major <= res {
        let frame = Frame::new(centre_foot, z, xv).ok_or_else(|| defect("the round's frame"))?;
        let sphere = Sphere::new(frame, radius, Sense::Forward)?;
        let lat = sec.contacts.map(|p| meridian(p, sec.centre));
        if lat.iter().any(|l| l.abs() > core::f64::consts::FRAC_PI_2) {
            return Err(unsupported(SheetBlendUnsupported::SpindleTorus));
        }
        let lo = sphere.from_step(Point2::new(-half_span, lat[0].min(lat[1])));
        let hi = sphere.from_step(Point2::new(half_span, lat[0].max(lat[1])));
        (Surface::Sphere(sphere), Round::Sphere, [lo, hi])
    } else {
        if outside < 0.0 || major <= sec.radius + res {
            return Err(unsupported(SheetBlendUnsupported::SpindleTorus));
        }
        let frame = Frame::new(centre_foot, z, xv).ok_or_else(|| defect("the round's frame"))?;
        let torus = Torus::new(frame, Length::from_mm(major), radius, Sense::Forward)?;
        // The tube's angle about the centre's circle, in the meridian at `u = 0`.
        let ring = centre_foot + xv * major;
        let va = meridian(sec.contacts[0], ring);
        let mut vb = meridian(sec.contacts[1], ring);
        // The short way round from the first contact to the second.
        while vb - va > core::f64::consts::PI {
            vb -= core::f64::consts::TAU;
        }
        while va - vb > core::f64::consts::PI {
            vb += core::f64::consts::TAU;
        }
        let lo = torus.from_step(Point2::new(-half_span, va.min(vb)));
        let hi = torus.from_step(Point2::new(half_span, va.max(vb)));
        (Surface::Torus(torus), Round::Torus, [lo, hi])
    };
    let bx = Extent2d::from_intervals(interval(v[0].x, v[1].x)?, interval(v[0].y, v[1].y)?);
    let made = face(&surface, &Bound::Box(bx), &doc).map_err(make)?;
    let [pa, pb] = sec.contacts;
    let contact = |p: Point3| -> Result<Contact, SheetBlendError> {
        let at = foot(p);
        let f = circle.frame();
        let frame = Frame::new(at, Vec3::from(f.z()), Vec3::from(f.x()))
            .ok_or_else(|| defect("a contact circle's frame"))?;
        let r = (p - at).length();
        let curve = Circle::new(frame, Length::from_mm(r), circle.sense())
            .map_err(|_| unsupported(SheetBlendUnsupported::SpindleTorus))?;
        Ok(Contact {
            curve: Curve::Circle(curve),
            interval: across.interval,
        })
    };
    Ok(Fillet {
        made,
        round,
        radius,
        setback: Length::from_mm(sec.setback),
        contacts: [contact(pa)?, contact(pb)?],
    })
}
