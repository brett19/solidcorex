// Written for solidcorex. Apache-2.0.
//! A face on an analytic surface, bounded by a box of its parameters or by a planar region
//! in its uv (research 75 MK15; the pin's `face_on` and `sheet_on`).
//!
//! The result is a sheet of one face. Every edge is an iso-line of the surface and so an
//! exact line or circle: a `u` line of a cylinder or cone is a ruling, of a sphere a
//! meridian circle, of a torus a tube circle; a `v` line of any of the four turned kinds
//! is a circle about the axis. A plane's iso-lines are lines.
//!
//! - **A box** may span a whole period (the seam is one edge used twice), reach a pole (a
//!   sphere's `v = 0` or `1`, a cone's apex: a collapsed edge) and wrap both ways (a torus
//!   is one face on its box `[0, 1] x [0, 1]`), so the kinds' whole faces are boxes too.
//! - **A region** is exact where its segments are iso-lines. On a plane every line and arc
//!   of the region is exact (the plane's uv is metric), so it is the sheet of the region. On
//!   the other kinds only the lines along `u` or `v` are, so the region must be made of them
//!   (an L, a notched rectangle, a frame with a rectangular hole), lie within one period
//!   and touch no pole; anything else is [`Unsupported::Segment`].
//!
//! A NURBS surface goes to `face_nurbs`: the same boxes and regions of iso-lines, every edge
//! an exact iso-curve and every pcurve a line in uv.
//!
//! The face's front is the side of the surface's normal: its outer loop runs
//! counter-clockwise in uv.

use core::f64::consts::TAU;

use scx_brep::{Document, EdgeId, FaceId, VertexId};
use scx_curve::arcs::Arcs;
use scx_curve::{Circle, Curve, Line};
use scx_num::extent1d::Extent1d;
use scx_num::extent2d::Extent2d;
use scx_num::fmath;
use scx_num::frame::{Frame, Sense};
use scx_num::units::Length;
use scx_num::vector2d::Point2;
use scx_num::vector3d::{Point3, Vec3};
use scx_planar::{Region, Segment};
use scx_surface::Surface;

use crate::Made;
use crate::assembly::Assembly;
use crate::error::{Defect, MakeError, Refusal, Unsupported};
use crate::extrude::{given, loops_of};
use crate::face_nurbs::nurbs_face;
use crate::prism::check;
use crate::sheet::sheet;

/// What bounds a [`face`]'s outer loop, in the surface's uv.
#[derive(Debug, Clone)]
pub enum Bound {
    /// The box `[u0, u1] x [v0, v1]`.
    Box(Extent2d),
    /// A planar region in `(u, v)`: its outer loop counter-clockwise, its holes clockwise.
    Region(Region),
}

/// The face of `surface` inside `bound`, as a sheet.
///
/// # Errors
///
/// - [`Refusal::UvBox`] for a box that is empty, longer than a period of the surface or past
///   its domain (a sphere's `v` outside `[0, 1]`, a cone's beyond its apex), and likewise a
///   region whose extent in `u` or `v` is.
/// - [`Unsupported::NurbsFace`] for a NURBS surface whose boundary meets itself (a seam, a
///   side collapsed to a point) or has an end no knot insertion reaches (unclamped).
/// - [`Unsupported::Segment`] for a region's segment that is not an iso-line of the surface
///   (anything but a line along `u` or `v`, off a plane), or that lies on a pole.
/// - [`Refusal::Resolution`] and the region's other refusals, as for [`sheet`].
/// - A [`Defect`] when the body does not finish, or [`MakeError::Full`].
pub fn face(surface: &Surface, bound: &Bound, doc: &Document) -> Result<Made<FaceId>, MakeError> {
    if let Surface::Nurbs(n) = surface {
        return nurbs_face(surface, n, bound, *doc);
    }
    let shape = Shape::of(surface)?;
    match bound {
        Bound::Box(b) => in_box(surface, &shape, b, *doc),
        Bound::Region(r) => {
            if let Surface::Plane(p) = surface {
                let made = sheet(core::slice::from_ref(r), p.frame(), doc)?;
                let face = made
                    .faces
                    .faces
                    .first()
                    .copied()
                    .ok_or(MakeError::Defect(Defect::Interval))?;
                return Ok(Made {
                    body: made.body,
                    faces: face,
                });
            }
            in_region(surface, &shape, r, *doc)
        }
    }
}

/// Where a `v` stands against the surface's domain.
#[derive(Clone, Copy, PartialEq)]
enum Reach {
    /// On the surface, away from a pole.
    Inside,
    /// On a pole's line (within rounding).
    Pole,
    /// Past the surface's domain.
    Outside,
}

#[derive(Clone, Copy)]
enum Kind {
    Plane,
    Cylinder { radius: f64 },
    Cone { radius: f64, slope: f64 },
    Sphere { radius: f64 },
    Torus { major: f64, minor: f64 },
}

/// The kinds' common placement: the frame and its z axis times the sense.
struct Shape {
    frame: Frame,
    z: Vec3,
    kind: Kind,
}

/// Relative slack for a parameter at a period or a pole.
const SNAP: f64 = 1e-12;

impl Shape {
    fn of(surface: &Surface) -> Result<Self, MakeError> {
        let (frame, sense, kind) = match surface {
            Surface::Plane(p) => (*p.frame(), Sense::Forward, Kind::Plane),
            Surface::Cylinder(c) => (
                *c.frame(),
                c.sense(),
                Kind::Cylinder {
                    radius: c.radius().mm(),
                },
            ),
            Surface::Cone(c) => (
                *c.frame(),
                c.sense(),
                Kind::Cone {
                    radius: c.radius().mm(),
                    slope: fmath::tan(c.half_angle()),
                },
            ),
            Surface::Sphere(s) => (
                *s.frame(),
                s.sense(),
                Kind::Sphere {
                    radius: s.radius().mm(),
                },
            ),
            Surface::Torus(t) => (
                *t.frame(),
                t.sense(),
                Kind::Torus {
                    major: t.major().mm(),
                    minor: t.minor().mm(),
                },
            ),
            _ => return Err(MakeError::Unsupported(Unsupported::NurbsFace)),
        };
        Ok(Self {
            frame,
            z: *frame.z() * sense.sign(),
            kind,
        })
    }

    /// The period of `u`, if it has one.
    fn period_u(&self) -> Option<f64> {
        match self.kind {
            Kind::Plane => None,
            Kind::Cylinder { radius } => Some(TAU * radius),
            _ => Some(1.0),
        }
    }

    /// The angle of `u` about the axis: the surface's parameter is the pin's, a rational
    /// quadratic arc to each quarter turn, so it is not proportional to the angle.
    fn angle_u(&self, u: f64) -> f64 {
        self.period_u().map_or(0.0, |p| Arcs::turn(p).angle(u))
    }

    /// The angle of `v` round a tube or up a sphere (from the south pole, `-pi/2` at 0).
    fn angle_v(&self, v: f64) -> f64 {
        match self.kind {
            Kind::Sphere { .. } => Arcs::half().angle(v),
            Kind::Torus { .. } => Arcs::turn(1.0).angle(v),
            _ => 0.0,
        }
    }

    /// The period of `v`, if it has one.
    fn period_v(&self) -> Option<f64> {
        matches!(self.kind, Kind::Torus { .. }).then_some(1.0)
    }

    /// The apex of a cone, `-r / k`.
    fn apex(&self) -> Option<f64> {
        match self.kind {
            Kind::Cone { radius, slope } => Some(-radius / slope),
            _ => None,
        }
    }

    fn reach(&self, v: f64) -> Reach {
        match self.kind {
            Kind::Sphere { .. } => {
                if v < -SNAP || v > 1.0 + SNAP {
                    Reach::Outside
                } else if v <= SNAP || v >= 1.0 - SNAP {
                    Reach::Pole
                } else {
                    Reach::Inside
                }
            }
            Kind::Cone { slope, .. } => {
                let apex = self.apex().unwrap_or(0.0);
                let tol = SNAP * (1.0 + apex.abs());
                // Past the apex is the other nappe: `v` on the far side of it.
                let past = if slope > 0.0 { apex - v } else { v - apex };
                if past > tol {
                    Reach::Outside
                } else if past >= -tol {
                    Reach::Pole
                } else {
                    Reach::Inside
                }
            }
            _ => Reach::Inside,
        }
    }

    /// `v` made the pole's exactly where it is within rounding of it.
    fn snapped(&self, v: f64) -> f64 {
        match (self.kind, self.reach(v)) {
            (Kind::Sphere { .. }, Reach::Pole) => {
                if v < 0.5 {
                    0.0
                } else {
                    1.0
                }
            }
            (Kind::Cone { .. }, Reach::Pole) => self.apex().unwrap_or(v),
            _ => v,
        }
    }

    /// The point of the pole at `v` (snapped), the surface's own formula giving only
    /// roundings of it.
    fn pole_point(&self, v: f64) -> Point3 {
        let o = self.frame.origin();
        match self.kind {
            Kind::Sphere { radius } => o + self.z * (if v < 0.5 { -radius } else { radius }),
            _ => o + self.z * v,
        }
    }

    /// The direction `D(u)` in the frame's `x, y` plane.
    fn dir(&self, u: f64) -> Vec3 {
        let (s, c) = fmath::sin_cos(self.angle_u(u));
        *self.frame.x() * c + *self.frame.y() * s
    }

    /// The circle of the `v` line at `v`: its centre's height along the axis and its
    /// radius.
    fn ring(&self, v: f64) -> Option<(f64, f64)> {
        match self.kind {
            Kind::Plane => None,
            Kind::Cylinder { radius } => Some((v, radius)),
            Kind::Cone { radius, slope } => Some((v, radius + slope * v)),
            Kind::Sphere { radius } => {
                let (s, c) = fmath::sin_cos(self.angle_v(v));
                Some((radius * s, radius * c))
            }
            Kind::Torus { major, minor } => {
                let (s, c) = fmath::sin_cos(self.angle_v(v));
                Some((minor * s, major + minor * c))
            }
        }
    }

    /// The edge along `v = v` over `u` from `u[0]` to `u[1]`, from `ends[0]` to `ends[1]`:
    /// a line on a plane, a circle about the axis elsewhere.
    fn v_line(
        &self,
        surface: &Surface,
        asm: &mut Assembly<'_>,
        v: f64,
        u: [f64; 2],
        ends: [VertexId; 2],
    ) -> Result<EdgeId, MakeError> {
        let Some((height, radius)) = self.ring(v) else {
            return straight(
                surface,
                asm,
                [Point2::new(u[0], v), Point2::new(u[1], v)],
                ends,
            );
        };
        let centre = self.frame.origin() + self.z * height;
        let frame = Frame::new(centre, *self.frame.z(), self.dir(u[0]))
            .ok_or(MakeError::Defect(Defect::PlaneFrame))?;
        let circle = Circle::new(frame, Length::from_mm(radius), Sense::Forward)?;
        let sweep = self.angle_u(u[1]) - self.angle_u(u[0]);
        let interval = Extent1d::new(0.0, sweep).ok_or(MakeError::Defect(Defect::Interval))?;
        asm.edge(Curve::Circle(circle), interval, ends)
    }

    /// The edge along `u = u` over `v` from `v[0]` to `v[1]`, from `ends[0]` to `ends[1]`:
    /// a line on a plane, cylinder or cone, a circle through the poles' axis on a sphere
    /// or about the tube's centre on a torus.
    fn u_line(
        &self,
        surface: &Surface,
        asm: &mut Assembly<'_>,
        u: f64,
        v: [f64; 2],
        ends: [VertexId; 2],
    ) -> Result<EdgeId, MakeError> {
        let (centre, radius, angle) = match self.kind {
            Kind::Sphere { radius } => (
                self.frame.origin(),
                radius,
                [self.angle_v(v[0]), self.angle_v(v[1])],
            ),
            Kind::Torus { major, minor } => (
                self.frame.origin() + self.dir(u) * major,
                minor,
                [self.angle_v(v[0]), self.angle_v(v[1])],
            ),
            _ => {
                return straight(
                    surface,
                    asm,
                    [Point2::new(u, v[0]), Point2::new(u, v[1])],
                    ends,
                );
            }
        };
        let d = self.dir(u);
        let (s, c) = fmath::sin_cos(angle[0]);
        // Counter-clockwise from `D` toward the axis: `v` runs up the sphere and round
        // the tube.
        let frame = Frame::new(centre, d.cross(self.z), d * c + self.z * s)
            .ok_or(MakeError::Defect(Defect::PlaneFrame))?;
        let circle = Circle::new(frame, Length::from_mm(radius), Sense::Forward)?;
        let interval =
            Extent1d::new(0.0, angle[1] - angle[0]).ok_or(MakeError::Defect(Defect::Interval))?;
        asm.edge(Curve::Circle(circle), interval, ends)
    }
}

/// The straight edge between the surface's points at `uv`, over `[0, 1]`.
fn straight(
    surface: &Surface,
    asm: &mut Assembly<'_>,
    uv: [Point2; 2],
    ends: [VertexId; 2],
) -> Result<EdgeId, MakeError> {
    let (p, q) = (surface.point(uv[0]), surface.point(uv[1]));
    let line = Line::new(p, q - p)?;
    let unit = Extent1d::new(0.0, 1.0).ok_or(MakeError::Defect(Defect::Interval))?;
    asm.edge(Curve::Line(line), unit, ends)
}

fn uv_box(u: [f64; 2], v: [f64; 2]) -> MakeError {
    MakeError::Refused(Refusal::UvBox { u, v })
}

/// How a parameter range sits against its direction's period: `Some(true)` when it is a
/// whole period, `Some(false)` when it is less, `None` when it is longer.
fn wraps(length: f64, period: Option<f64>) -> Option<bool> {
    match period {
        None => Some(false),
        Some(p) if length > p * (1.0 + SNAP) => None,
        Some(p) => Some(length >= p * (1.0 - SNAP)),
    }
}

fn in_box(
    surface: &Surface,
    shape: &Shape,
    bx: &Extent2d,
    doc: Document,
) -> Result<Made<FaceId>, MakeError> {
    let (ui, vi) = (bx.x_interval(), bx.y_interval());
    let (u0, v0, v1) = (ui.min(), vi.min(), vi.max());
    let refused = || uv_box([ui.min(), ui.max()], [vi.min(), vi.max()]);
    let (du, dv) = (ui.length(), vi.length());
    if !(du > 0.0 && dv > 0.0 && du.is_finite() && dv.is_finite()) {
        return Err(refused());
    }
    let full_u = wraps(du, shape.period_u()).ok_or_else(refused)?;
    let full_v = wraps(dv, shape.period_v()).ok_or_else(refused)?;
    let (r0, r1) = (shape.reach(v0), shape.reach(v1));
    if r0 == Reach::Outside || r1 == Reach::Outside {
        return Err(refused());
    }
    let (pole0, pole1) = (r0 == Reach::Pole, r1 == Reach::Pole);
    // A whole period is exactly one, so the seam is one parameter.
    let u1 = match (full_u, shape.period_u()) {
        (true, Some(p)) => u0 + p,
        _ => ui.max(),
    };
    let (v0, v1) = match (full_v, shape.period_v()) {
        (true, Some(p)) => (v0, v0 + p),
        _ => (shape.snapped(v0), shape.snapped(v1)),
    };
    let place = |u: f64, v: f64, pole: bool| {
        if pole {
            shape.pole_point(v)
        } else {
            surface.point(Point2::new(u, v))
        }
    };
    let mut asm = Assembly::sheets(&doc);
    // The corners, with the ones a period, a seam or a pole makes one made once.
    let c00 = asm.vertex(place(u0, v0, pole0))?;
    let c10 = if full_u || pole0 {
        c00
    } else {
        asm.vertex(place(u1, v0, false))?
    };
    let c01 = if full_v {
        c00
    } else {
        asm.vertex(place(u0, v1, pole1))?
    };
    let c11 = if full_v {
        c10
    } else if full_u || pole1 {
        c01
    } else {
        asm.vertex(place(u1, v1, false))?
    };
    let bottom = if pole0 {
        asm.collapsed(c00)?
    } else {
        shape.v_line(surface, &mut asm, v0, [u0, u1], [c00, c10])?
    };
    let top = if full_v {
        bottom
    } else if pole1 {
        asm.collapsed(c01)?
    } else {
        shape.v_line(surface, &mut asm, v1, [u0, u1], [c01, c11])?
    };
    let left = shape.u_line(surface, &mut asm, u0, [v0, v1], [c00, c01])?;
    let right = if full_u {
        left
    } else {
        shape.u_line(surface, &mut asm, u1, [v0, v1], [c10, c11])?
    };
    // Counter-clockwise in uv: along the bottom, up the right, back along the top, down
    // the left. A collapsed edge is used as it is.
    let top_sense = if pole1 {
        Sense::Forward
    } else {
        Sense::Reversed
    };
    let face = asm.sheet_face(
        surface.clone(),
        &[vec![
            (bottom, Sense::Forward),
            (right, Sense::Forward),
            (top, top_sense),
            (left, Sense::Reversed),
        ]],
    )?;
    Ok(Made {
        body: asm.finish()?,
        faces: face,
    })
}

fn in_region(
    surface: &Surface,
    shape: &Shape,
    region: &Region,
    doc: Document,
) -> Result<Made<FaceId>, MakeError> {
    check(core::slice::from_ref(region), doc.resolution())?;
    // The ends of every segment, to check the region against the surface before building.
    let mut ends = Vec::new();
    for (l, turned, in_loop) in loops_of(region) {
        let n = l.segments().len();
        for (i, s) in l.segments().iter().enumerate() {
            let at = given(in_loop, turned, n, i);
            let Segment::Line { start, end } = *s else {
                return Err(MakeError::Unsupported(Unsupported::Segment(at)));
            };
            if start.x != end.x && start.y != end.y {
                return Err(MakeError::Unsupported(Unsupported::Segment(at)));
            }
            ends.push(start);
            if shape.reach(start.y) == Reach::Pole || shape.reach(end.y) == Reach::Pole {
                return Err(MakeError::Unsupported(Unsupported::Segment(at)));
            }
        }
    }
    let bx = Extent2d::hull(ends).ok_or(MakeError::Defect(Defect::Interval))?;
    let (ui, vi) = (bx.x_interval(), bx.y_interval());
    let refused = || uv_box([ui.min(), ui.max()], [vi.min(), vi.max()]);
    let within = |length: f64, period: Option<f64>| period.is_none_or(|p| length < p);
    if !(within(ui.length(), shape.period_u())
        && within(vi.length(), shape.period_v())
        && shape.reach(vi.min()) != Reach::Outside
        && shape.reach(vi.max()) != Reach::Outside)
    {
        return Err(refused());
    }
    let mut asm = Assembly::sheets(&doc);
    let mut loops = Vec::with_capacity(1 + region.holes().len());
    for (l, _, _) in loops_of(region) {
        let segments = l.segments();
        let n = segments.len();
        let mut vertices = Vec::with_capacity(n);
        for s in segments {
            vertices.push(asm.vertex(surface.point(s.start()))?);
        }
        let mut uses = Vec::with_capacity(n);
        for (i, (s, &from)) in segments.iter().zip(&vertices).enumerate() {
            let to = vertices
                .get((i + 1) % n)
                .copied()
                .ok_or(MakeError::Defect(Defect::Interval))?;
            let Segment::Line { start, end } = *s else {
                return Err(MakeError::Defect(Defect::Interval));
            };
            // The edge runs up the parameter; the use runs the segment's way.
            let (sense, lo, hi, ends) = if start.x < end.x || start.y < end.y {
                (Sense::Forward, start, end, [from, to])
            } else {
                (Sense::Reversed, end, start, [to, from])
            };
            let edge = if start.y == end.y {
                shape.v_line(surface, &mut asm, start.y, [lo.x, hi.x], ends)?
            } else {
                shape.u_line(surface, &mut asm, start.x, [lo.y, hi.y], ends)?
            };
            uses.push((edge, sense));
        }
        loops.push(uses);
    }
    let face = asm.sheet_face(surface.clone(), &loops)?;
    Ok(Made {
        body: asm.finish()?,
        faces: face,
    })
}
