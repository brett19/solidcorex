// Written for solidcorex. Apache-2.0.
//! The sweep of a planar profile along a path of lines and circular arcs, with no law: the
//! profile carried by the path's rotation-minimising frame ([`frames`]), its walls built by
//! kind and its mitres in closed form, all in one draft (research 75 §7 MK5).
//!
//! The profile is a [`Region`] in a frame at the path's start whose z axis is the start's
//! tangent. A station's placement takes a profile point `(x, y)` to `at + x R X + y R Y`,
//! `R` the station's [`Rotation`](crate::Rotation) and `X, Y` the frame's axes.
//!
//! - **Walls by kind** (75 R4, R5: chosen before anything is built). On a line piece the
//!   sweep is an extrusion: a profile line's wall is a plane, an arc's a cylinder. On an arc
//!   piece it is a partial revolve about the arc's axis, which lies in the profile's plane:
//!   a line's wall is a plane normal to the axis, a cylinder or a cone (the revolve's snap
//!   rules), an arc's a sphere, a ring torus or a horn torus. A helix piece needs the
//!   procedural kind and is [`Unsupported::HelixPiece`].
//! - **Sections shared.** Each joint's section (the profile placed there) is one set of
//!   vertices and edges, used by the walls either side and by a cap. The edge joining a
//!   profile vertex across a piece is a line on a line piece and a circle about the axis on
//!   an arc piece. A closed path (it ends where it starts) has no caps; its last section is
//!   its first, and a hole's walls line a hollow of their own.
//! - **Mitres in closed form.** At a corner between two lines the section is the mitre: the
//!   profile in the bisecting plane, stretched by [`Corner::stretch`] along the turn. Its
//!   lines are lines and its arcs ellipses on the cylinders either side (the principal axes
//!   from the stretch). A corner next to an arc piece would cut a cone, sphere or torus in a
//!   curve that is not a conic: [`Unsupported::CornerOnArc`]. A closed path may close at a
//!   corner between lines; its first section is then that mitre.
//! - **Refused.** An arc piece whose radius does not exceed the profile's reach towards its
//!   axis by more than the resolution ([`Refusal::TightArc`]); a line piece too short for
//!   its two mitres ([`Refusal::MitresMeet`]); an arc of a whole turn or more on a path that
//!   is not that one turn ([`Refusal::ArcTurn`]); a closed path that brings the profile back
//!   turned ([`Refusal::ClosedTurned`], a twist law's case); a frame not at the path's start
//!   along its tangent ([`Refusal::Placement`]).
//!
//! What is not checked: the swept body meeting itself across pieces that are not
//! neighbours (a path that comes back near itself).

use core::f64::consts::TAU;
use std::collections::BTreeMap;

use scx_brep::{Document, EdgeId, FaceId, ShellId, VertexId};
use scx_curve::{Circle, Curve, Ellipse};
use scx_num::extent1d::Extent1d;
use scx_num::fmath;
use scx_num::frame::{Frame, Sense};
use scx_num::units::Length;
use scx_num::vector2d::Point2;
use scx_num::vector3d::{Point3, UnitVec3, Vec3};
use scx_planar::{LoopId, Region, Segment, SegmentRef};
use scx_surface::{Cone, Cylinder, Plane, Sphere, Surface, Torus};

use crate::Made;
use crate::assembly::Assembly;
use crate::error::{Defect, MakeError, Refusal, Unsupported};
use crate::extrude::{given, loops_of};
use crate::frames::{Corner, FrameError, G1_TANGENTS, Path, Piece, Rotation, Station, Stop};

/// The faces of a [`sweep`](crate::sweep), by role.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SweepFaces {
    /// The cap at the path's start, its front facing back along the path; none for a closed
    /// path.
    pub start: Option<FaceId>,
    /// The cap at the path's end, its front facing on along it; none for a closed path.
    pub end: Option<FaceId>,
    /// The walls, `[piece][loop][segment]`: each path piece's in order; loop 0 is the
    /// profile's outer loop and loop `1 + k` hole `k`, each segment at its index as given to
    /// [`Region::new`], whichever way the region turned the loop.
    pub walls: Vec<Vec<Vec<FaceId>>>,
}

/// The plain sweep ([`sweep`](crate::sweep) with [`Shape::PLAIN`](crate::Shape::PLAIN)):
/// `profile`, placed in `frame`'s `x, y` plane, along `path` with no law: the profile
/// carried by the path's rotation-minimising frame, mitred at corners. `frame`'s origin is
/// the path's start and its z axis the start's tangent.
///
/// # Errors
///
/// - [`Refusal::Resolution`] when the profile was built at a resolution other than `doc`'s.
/// - [`Refusal::Placement`] when `frame` is not at the path's start along its tangent.
/// - [`Refusal::TightArc`], [`Refusal::MitresMeet`], [`Refusal::ArcTurn`] and
///   [`Refusal::ClosedTurned`] when the swept profile would meet itself or its ends not
///   meet, at the first piece that does; [`Refusal::Path`] for a closed path that doubles
///   back where it closes.
/// - [`Unsupported::HelixPiece`], [`Unsupported::CornerOnArc`], [`Unsupported::Segment`] for
///   a profile segment that is not a line, arc or circle, and [`Unsupported::SpindleTorus`]
///   for a profile arc whose circle crosses an arc piece's axis outside it.
/// - A [`Defect`] when the body does not finish, or [`MakeError::Full`].
pub(crate) fn build(
    profile: &Region,
    frame: &Frame,
    path: &Path,
    doc: &Document,
) -> Result<Made<SweepFaces>, MakeError> {
    let plan = Plan::new(profile, frame, path, *doc)?;
    let mut build = Build::new(&plan, Assembly::new(doc)?);
    let walls = build.walls()?;
    let (start, end) = if plan.closed {
        (None, None)
    } else {
        (Some(build.cap(false)?), Some(build.cap(true)?))
    };
    Ok(Made {
        body: build.asm.finish()?,
        faces: SweepFaces { start, end, walls },
    })
}

/// Where a section puts the profile: `(x, y)` at `at + x ex + y ey`. A station's is a
/// rotation of the frame's axes (`rigid`); a mitre's is stretched along the turn.
#[derive(Debug, Clone, Copy)]
struct Place {
    at: Point3,
    ex: Vec3,
    ey: Vec3,
    rigid: bool,
}

impl Place {
    fn station(s: &Station, x: Vec3, y: Vec3) -> Self {
        Self {
            at: s.at,
            ex: s.turn.apply(x),
            ey: s.turn.apply(y),
            rigid: true,
        }
    }

    /// The mitre at `at` of the profile turned by `half` into the bisecting plane, then
    /// stretched by `stretch` along the unit `d`, the turn's direction in that plane.
    fn mitre(at: Point3, half: &Rotation, d: Vec3, stretch: f64, x: Vec3, y: Vec3) -> Self {
        let s = |v: Vec3| v + d * (v.dot(d) * (stretch - 1.0));
        Self {
            at,
            ex: s(half.apply(x)),
            ey: s(half.apply(y)),
            rigid: false,
        }
    }

    fn point(&self, q: Point2) -> Point3 {
        self.at + (self.ex * q.x + self.ey * q.y)
    }

    fn dir(&self, (x, y): (f64, f64)) -> Vec3 {
        self.ex * x + self.ey * y
    }

    /// The profile coordinates of a direction `v`, for a rigid place.
    fn coords(&self, v: Vec3) -> (f64, f64) {
        (v.dot(self.ex), v.dot(self.ey))
    }
}

/// A path piece as the sweep builds it.
#[derive(Debug, Clone, Copy)]
enum Run {
    Line,
    /// About the unit `axis` through `foot`, the radial direction `u` at the start, of
    /// `radius`, over `angle` (a whole turn exactly for a closed path of this one piece).
    Arc {
        foot: Point3,
        axis: Vec3,
        u: Vec3,
        angle: f64,
    },
}

/// A profile loop as stored, with whether the region turned it and its id.
struct Lp {
    segments: Vec<Segment>,
    turned: bool,
    id: LoopId,
}

impl Lp {
    fn at(&self, i: usize) -> SegmentRef {
        given(self.id, self.turned, self.segments.len(), i)
    }
}

/// The sweep decided before anything is built: every section and every wall's surface.
struct Plan {
    loops: Vec<Lp>,
    runs: Vec<Run>,
    /// The section at each joint, the first piece's start first: one per piece and the end
    /// for an open path, one per piece for a closed one.
    sections: Vec<Place>,
    /// The surface of each wall, `[piece][loop][segment]` in the stored order.
    surfaces: Vec<Vec<Vec<Surface>>>,
    /// The start's and the end's tangent.
    tangents: [Vec3; 2],
    closed: bool,
}

/// The unit bisector of `a` and `b`.
fn bisector(a: Vec3, b: Vec3) -> Result<Vec3, MakeError> {
    (a + b)
        .normalized()
        .map(UnitVec3::as_vec)
        .ok_or(MakeError::Defect(Defect::PlaneFrame))
}

impl Plan {
    #[allow(clippy::too_many_lines)]
    fn new(profile: &Region, frame: &Frame, path: &Path, doc: Document) -> Result<Self, MakeError> {
        if profile.resolution() != doc.resolution() {
            return Err(MakeError::Refused(Refusal::Resolution {
                region: profile.resolution(),
                document: doc.resolution(),
            }));
        }
        let zone = doc.resolution().zone().mm();
        let loops: Vec<Lp> = loops_of(profile)
            .map(|(l, turned, id)| Lp {
                segments: l.segments().to_vec(),
                turned,
                id,
            })
            .collect();
        for lp in &loops {
            for (i, s) in lp.segments.iter().enumerate() {
                if !matches!(
                    s,
                    Segment::Line { .. } | Segment::Arc { .. } | Segment::Circle { .. }
                ) {
                    return Err(MakeError::Unsupported(Unsupported::Segment(lp.at(i))));
                }
            }
        }
        let outer: &[Segment] = loops.first().map_or(&[], |l| &l.segments);
        let joints: Vec<(Piece, f64, Option<Corner>)> =
            path.joints().map(|(p, f, c)| (*p, f, c.copied())).collect();
        let n = joints.len();
        let is_arc = |i: usize| matches!(joints.get(i), Some((Piece::Arc { .. }, ..)));
        for (i, (p, ..)) in joints.iter().enumerate() {
            if matches!(p, Piece::Helix { .. }) {
                return Err(MakeError::Unsupported(Unsupported::HelixPiece { piece: i }));
            }
        }
        let mut corners = 0;
        for (i, (_, _, c)) in joints.iter().enumerate() {
            if c.is_some() {
                if is_arc(i - 1) || is_arc(i) {
                    return Err(MakeError::Unsupported(Unsupported::CornerOnArc {
                        corner: corners,
                    }));
                }
                corners += 1;
            }
        }

        // The stations: each piece's start, the end, then each corner's mitre.
        let mut stops: Vec<Stop> = joints.iter().map(|j| Stop::At(j.1)).collect();
        stops.push(Stop::At(1.0));
        stops.extend((0..corners).map(Stop::Corner));
        let stations = crate::frames::frames(path, &stops)
            .map_err(|e| MakeError::Defect(Defect::Station(e)))?;
        let station = |k: usize| {
            stations
                .get(k)
                .copied()
                .ok_or(MakeError::Defect(Defect::Interval))
        };
        let (first, last) = (station(0)?, station(n)?);
        let offset = frame.origin().distance(first.at);
        let tilt = (*frame.z() - first.tangent.as_vec()).length();
        if !(offset <= zone && tilt <= G1_TANGENTS) {
            return Err(MakeError::Refused(Refusal::Placement { offset, tilt }));
        }
        let (x, y) = (*frame.x(), *frame.y());
        let farthest = farthest(outer);

        // A closed path closes smoothly or at a corner between two lines, and must bring
        // the profile back to itself.
        let closed = path.is_closed(&doc);
        let (t_start, t_end) = (first.tangent.as_vec(), last.tangent.as_vec());
        let mut closing: Option<(UnitVec3, f64)> = None;
        if closed {
            let mut back = last.turn;
            if (t_end - t_start).length() > G1_TANGENTS {
                if is_arc(0) || is_arc(n - 1) {
                    return Err(MakeError::Unsupported(Unsupported::CornerOnArc {
                        corner: corners,
                    }));
                }
                let cross = t_end.cross(t_start);
                let axis = cross.normalized().ok_or(MakeError::Refused(Refusal::Path(
                    FrameError::DoublesBack { index: 0 },
                )))?;
                let angle = fmath::atan2(cross.length(), t_end.dot(t_start));
                back = Rotation::about(axis, angle).after(&back);
                closing = Some((axis, angle));
            }
            let off = farthest
                * (back.apply(x) - x)
                    .length()
                    .max((back.apply(y) - y).length());
            if off.is_nan() || off > zone {
                return Err(MakeError::Refused(Refusal::ClosedTurned { off }));
            }
        }

        // Each piece's own rigid placement at its start, its run and its tangent.
        let mut rigid = Vec::with_capacity(n);
        let mut runs = Vec::with_capacity(n);
        for (i, (piece, ..)) in joints.iter().enumerate() {
            let s = station(i)?;
            let place = Place::station(&s, x, y);
            let run = match *piece {
                Piece::Line { .. } => Run::Line,
                Piece::Arc {
                    centre,
                    axis,
                    start,
                    angle,
                } => {
                    let a = axis.as_vec();
                    let d = start - centre;
                    let radial = d - a * a.dot(d);
                    let radius = radial.length();
                    let u = radial
                        .normalized()
                        .ok_or(MakeError::Defect(Defect::PlaneFrame))?
                        .as_vec();
                    let inward = reach(outer, place.coords(u * -1.0));
                    if (radius - inward).is_nan() || radius - inward <= zone {
                        return Err(MakeError::Refused(Refusal::TightArc {
                            piece: i,
                            radius,
                            reach: inward,
                        }));
                    }
                    let widest = radius + reach(outer, place.coords(u));
                    let whole = closed && n == 1;
                    let fits = if whole {
                        (angle - TAU).abs() * widest <= zone
                    } else {
                        angle < TAU && (TAU - angle) * widest > zone
                    };
                    if !fits {
                        return Err(MakeError::Refused(Refusal::ArcTurn { piece: i, angle }));
                    }
                    Run::Arc {
                        foot: start - radial,
                        axis: a,
                        u,
                        angle: if whole { TAU } else { angle },
                    }
                }
                Piece::Helix { .. } => {
                    return Err(MakeError::Unsupported(Unsupported::HelixPiece { piece: i }));
                }
            };
            rigid.push((place, s.tangent.as_vec()));
            runs.push(run);
        }

        // The mitre planes' normals either side of each line piece, and its room for them.
        let closing_normal = match closing {
            Some(_) => Some(bisector(t_end, t_start)?),
            None => None,
        };
        let mut corner_normals = Vec::with_capacity(n);
        for (i, (_, _, c)) in joints.iter().enumerate() {
            corner_normals.push(match c {
                Some(c) => Some(bisector(c.from.as_vec(), c.to.as_vec())?),
                None if i == 0 => closing_normal,
                None => None,
            });
        }
        for (i, (piece, ..)) in joints.iter().enumerate() {
            let Piece::Line { from, to } = *piece else {
                continue;
            };
            let (place, t) = rigid
                .get(i)
                .copied()
                .ok_or(MakeError::Defect(Defect::Interval))?;
            let slant = |m: Option<Vec3>| m.map_or(t, |m| m * (1.0 / t.dot(m)));
            let n_in = slant(corner_normals.get(i).copied().flatten());
            let n_out = slant(if i + 1 == n {
                closing_normal
            } else {
                corner_normals.get(i + 1).copied().flatten()
            });
            let g = n_out - n_in;
            let room = from.distance(to) - reach(outer, place.coords(g));
            if room.is_nan() || room <= zone {
                return Err(MakeError::Refused(Refusal::MitresMeet { piece: i }));
            }
        }

        // The sections: a mitre after each corner (and at a closed path's closing corner),
        // otherwise the station's placement.
        let count = if closed { n } else { n + 1 };
        let mut sections = Vec::with_capacity(count);
        let mut corner = 0;
        for k in 0..count {
            let place = if k == n {
                Place::station(&last, x, y)
            } else if let Some((_, _, Some(c))) = joints.get(k) {
                let s = station(n + 1 + corner)?;
                corner += 1;
                let d = (c.to.as_vec() - c.from.as_vec())
                    .normalized()
                    .ok_or(MakeError::Defect(Defect::PlaneFrame))?;
                Place::mitre(s.at, &s.turn, d.as_vec(), c.stretch, x, y)
            } else if let (0, Some((axis, angle))) = (k, closing) {
                let d = (t_start - t_end)
                    .normalized()
                    .ok_or(MakeError::Defect(Defect::PlaneFrame))?;
                let half = Rotation::about(axis, -0.5 * angle);
                let stretch = 1.0 / fmath::cos(0.5 * angle);
                Place::mitre(first.at, &half, d.as_vec(), stretch, x, y)
            } else {
                rigid.get(k).ok_or(MakeError::Defect(Defect::Interval))?.0
            };
            sections.push(place);
        }

        // Every wall's surface.
        let mut surfaces = Vec::with_capacity(n);
        for (&run, &(place, t)) in runs.iter().zip(&rigid) {
            let mut per_loop = Vec::with_capacity(loops.len());
            for lp in &loops {
                let mut walls = Vec::with_capacity(lp.segments.len());
                for (i, s) in lp.segments.iter().enumerate() {
                    walls.push(wall(run, &place, t, s, lp.at(i), zone)?);
                }
                per_loop.push(walls);
            }
            surfaces.push(per_loop);
        }
        Ok(Self {
            loops,
            runs,
            sections,
            surfaces,
            tangents: [t_start, t_end],
            closed,
        })
    }
}

/// The wall surface of profile segment `s` on a piece of `run` from the rigid `place`
/// (tangent `t`), its front away from the solid: right of the segment's travel, which is
/// outward for the counter-clockwise outer loop and into the hole for a clockwise hole.
fn wall(
    run: Run,
    place: &Place,
    t: Vec3,
    s: &Segment,
    at: SegmentRef,
    zone: f64,
) -> Result<Surface, MakeError> {
    let frame = |o: Point3, z: Vec3, x: Vec3| {
        Frame::new(o, z, x).ok_or(MakeError::Defect(Defect::PlaneFrame))
    };
    let sense = |forward: bool| {
        if forward {
            Sense::Forward
        } else {
            Sense::Reversed
        }
    };
    match run {
        Run::Line => match (*s, arc2(s)) {
            (Segment::Line { start: a, end: b }, _) => {
                let (dx, dy) = (b.x - a.x, b.y - a.y);
                Ok(Surface::Plane(Plane::new(frame(
                    place.point(a),
                    place.dir((dy, -dx)),
                    place.dir((dx, dy)),
                )?)))
            }
            (_, Some(g)) => Ok(Surface::Cylinder(Cylinder::new(
                frame(place.point(g.centre), t, place.dir(g.from))?,
                Length::from_mm(g.radius),
                g.sense,
            )?)),
            _ => Err(MakeError::Unsupported(Unsupported::Segment(at))),
        },
        Run::Arc { foot, axis, u, .. } => {
            let rh = |q: Point2| {
                let v = place.point(q) - foot;
                (v.dot(u), v.dot(axis))
            };
            let axial = |h: f64| frame(foot + axis * h, axis, u);
            match (*s, arc2(s)) {
                (Segment::Line { start: a, end: b }, _) => {
                    let ((ra, ha), (rb, hb)) = (rh(a), rh(b));
                    // The outward normal of the line in the profile's plane.
                    let out = place.dir((b.y - a.y, a.x - b.x));
                    if (ha - hb).abs() <= zone {
                        let z = axis * out.dot(axis).signum();
                        return Ok(Surface::Plane(Plane::new(frame(
                            foot + axis * (0.5 * (ha + hb)),
                            z,
                            u,
                        )?)));
                    }
                    let s = sense(out.dot(u) > 0.0);
                    if (ra - rb).abs() <= zone {
                        return Ok(Surface::Cylinder(Cylinder::new(
                            axial(ha)?,
                            Length::from_mm(0.5 * (ra + rb)),
                            s,
                        )?));
                    }
                    // The frame at the wider end; the cone's height runs along the axis
                    // times the sense (as the revolve's).
                    let ((br, bh), (sr, sh)) = if ra >= rb {
                        ((ra, ha), (rb, hb))
                    } else {
                        ((rb, hb), (ra, ha))
                    };
                    let half_angle = fmath::atan((sr - br) / (s.sign() * (sh - bh)));
                    Ok(Surface::Cone(Cone::new(
                        axial(bh)?,
                        Length::from_mm(br),
                        half_angle,
                        s,
                    )?))
                }
                (_, Some(g)) => {
                    let (cr, ch) = rh(g.centre);
                    let r = g.radius;
                    if cr.abs() <= zone {
                        Ok(Surface::Sphere(Sphere::new(
                            axial(ch)?,
                            Length::from_mm(r),
                            g.sense,
                        )?))
                    } else if cr > r + zone {
                        Ok(Surface::Torus(Torus::new(
                            axial(ch)?,
                            Length::from_mm(cr),
                            Length::from_mm(r),
                            g.sense,
                        )?))
                    } else if (cr - r).abs() <= zone {
                        Ok(Surface::Torus(Torus::horn(
                            axial(ch)?,
                            Length::from_mm(0.5 * (cr + r)),
                            g.sense,
                        )?))
                    } else {
                        Err(MakeError::Unsupported(Unsupported::SpindleTorus { at }))
                    }
                }
                _ => Err(MakeError::Unsupported(Unsupported::Segment(at))),
            }
        }
    }
}

/// A profile arc's or circle's circle: counter-clockwise from `from` (the offset of its
/// counter-clockwise-first end from the centre, at angle `a0`) over `sweep`, the radius
/// the mean of its ends' distances; `sense` the segment's.
struct Arc2 {
    centre: Point2,
    radius: f64,
    from: (f64, f64),
    a0: f64,
    sweep: f64,
    sense: Sense,
}

fn arc2(s: &Segment) -> Option<Arc2> {
    match *s {
        Segment::Arc {
            centre,
            start,
            end,
            sense,
        } => {
            let (first, last) = match sense {
                Sense::Forward => (start, end),
                Sense::Reversed => (end, start),
            };
            let u = (first.x - centre.x, first.y - centre.y);
            let w = (last.x - centre.x, last.y - centre.y);
            let radius = 0.5 * (fmath::hypot(u.0, u.1) + fmath::hypot(w.0, w.1));
            let mut sweep = fmath::atan2(u.0 * w.1 - u.1 * w.0, u.0 * w.0 + u.1 * w.1);
            if sweep <= 0.0 {
                sweep += TAU;
            }
            Some(Arc2 {
                centre,
                radius,
                from: u,
                a0: fmath::atan2(u.1, u.0),
                sweep,
                sense,
            })
        }
        Segment::Circle {
            centre,
            radius,
            sense,
        } => Some(Arc2 {
            centre,
            radius,
            from: (radius, 0.0),
            a0: 0.0,
            sweep: TAU,
            sense,
        }),
        _ => None,
    }
}

/// Whether the direction at angle `phi` lies strictly inside the counter-clockwise arc from
/// angle `a0` over `sweep`.
fn within(phi: f64, a0: f64, sweep: f64) -> bool {
    let off = (phi - a0).rem_euclid(TAU);
    off > 0.0 && off < sweep
}

/// The most `d . q` over the points `q` of the loop `segments` (a region's outer loop
/// bounds it): at a vertex, or inside an arc where its normal is along `d`.
fn reach(segments: &[Segment], d: (f64, f64)) -> f64 {
    let len = fmath::hypot(d.0, d.1);
    let phi = fmath::atan2(d.1, d.0);
    let mut most = f64::MIN;
    for s in segments {
        let p = s.start();
        most = most.max(d.0 * p.x + d.1 * p.y);
        if let Some(g) = arc2(s)
            && len > 0.0
            && within(phi, g.a0, g.sweep)
        {
            most = most.max(d.0 * g.centre.x + d.1 * g.centre.y + g.radius * len);
        }
    }
    most
}

/// The farthest a point of the loop `segments` is from the profile's origin.
fn farthest(segments: &[Segment]) -> f64 {
    let mut most = 0.0_f64;
    for s in segments {
        let p = s.start();
        most = most.max(fmath::hypot(p.x, p.y));
        if let Some(g) = arc2(s) {
            let c = fmath::hypot(g.centre.x, g.centre.y);
            if c == 0.0 || within(fmath::atan2(g.centre.y, g.centre.x), g.a0, g.sweep) {
                most = most.max(c + g.radius);
            }
        }
    }
    most
}

/// The body being written from a plan.
struct Build<'p, 'd> {
    plan: &'p Plan,
    asm: Assembly<'d>,
    /// By `(section, loop, segment)`: the vertex at the segment's start.
    vertices: BTreeMap<(usize, usize, usize), VertexId>,
    /// By `(section, loop, segment)`: the segment's edge, with the sense of a use along it.
    rims: BTreeMap<(usize, usize, usize), (EdgeId, Sense)>,
    /// By `(piece, loop, segment)`: the edge along the piece from the segment's start, with
    /// the sense of a use along the path.
    rails: BTreeMap<(usize, usize, usize), (EdgeId, Sense)>,
    /// By loop: the shells in front of and behind its walls.
    sides: Vec<[ShellId; 2]>,
}

impl<'p, 'd> Build<'p, 'd> {
    const fn new(plan: &'p Plan, asm: Assembly<'d>) -> Self {
        Self {
            plan,
            asm,
            vertices: BTreeMap::new(),
            rims: BTreeMap::new(),
            rails: BTreeMap::new(),
            sides: Vec::new(),
        }
    }

    fn segment(&self, l: usize, i: usize) -> Result<Segment, MakeError> {
        self.plan
            .loops
            .get(l)
            .and_then(|lp| lp.segments.get(i))
            .copied()
            .ok_or(MakeError::Defect(Defect::Interval))
    }

    fn count(&self, l: usize) -> usize {
        self.plan.loops.get(l).map_or(0, |lp| lp.segments.len())
    }

    fn section(&self, k: usize) -> Result<(usize, Place), MakeError> {
        let k = k % self.plan.sections.len().max(1);
        let place = self
            .plan
            .sections
            .get(k)
            .copied()
            .ok_or(MakeError::Defect(Defect::Interval))?;
        Ok((k, place))
    }

    fn vertex(&mut self, k: usize, l: usize, i: usize) -> Result<VertexId, MakeError> {
        let (k, place) = self.section(k)?;
        let i = i % self.count(l).max(1);
        if let Some(&v) = self.vertices.get(&(k, l, i)) {
            return Ok(v);
        }
        let v = self.asm.vertex(place.point(self.segment(l, i)?.start()))?;
        self.vertices.insert((k, l, i), v);
        Ok(v)
    }

    /// Segment `i`'s edge in section `k`: a line, a circle in a station's section, an
    /// ellipse in a mitre's.
    fn rim(&mut self, k: usize, l: usize, i: usize) -> Result<(EdgeId, Sense), MakeError> {
        let (k, place) = self.section(k)?;
        if let Some(&e) = self.rims.get(&(k, l, i)) {
            return Ok(e);
        }
        let s = self.segment(l, i)?;
        let (va, vb) = (self.vertex(k, l, i)?, self.vertex(k, l, i + 1)?);
        let e = match arc2(&s) {
            None => self.asm.line(va, vb)?,
            Some(g) => {
                let ends = match g.sense {
                    Sense::Forward => [va, vb],
                    Sense::Reversed => [vb, va],
                };
                let centre = place.point(g.centre);
                let curve = if place.rigid {
                    let frame = Frame::new(centre, place.ex.cross(place.ey), place.dir(g.from))
                        .ok_or(MakeError::Defect(Defect::PlaneFrame))?;
                    let interval =
                        Extent1d::new(0.0, g.sweep).ok_or(MakeError::Defect(Defect::Interval))?;
                    (
                        Curve::Circle(Circle::new(
                            frame,
                            Length::from_mm(g.radius),
                            Sense::Forward,
                        )?),
                        interval,
                    )
                } else {
                    // The principal axes: the profile directions the placement keeps
                    // square, the eigenvectors of its Gram matrix.
                    let (g11, g12, g22) = (
                        place.ex.dot(place.ex),
                        place.ex.dot(place.ey),
                        place.ey.dot(place.ey),
                    );
                    let psi = 0.5 * fmath::atan2(2.0 * g12, g11 - g22);
                    let (c, s) = (fmath::cos(psi), fmath::sin(psi));
                    let (ax, ay) = (place.dir((c, s)), place.dir((-s, c)));
                    let frame = Frame::new(centre, ax.cross(ay), ax)
                        .ok_or(MakeError::Defect(Defect::PlaneFrame))?;
                    let t0 = g.a0 - psi;
                    let interval = Extent1d::new(t0, t0 + g.sweep)
                        .ok_or(MakeError::Defect(Defect::Interval))?;
                    (
                        Curve::Ellipse(Ellipse::new(
                            frame,
                            Length::from_mm(g.radius * ax.length()),
                            Length::from_mm(g.radius * ay.length()),
                            Sense::Forward,
                        )?),
                        interval,
                    )
                };
                (self.asm.edge(curve.0, curve.1, ends)?, g.sense)
            }
        };
        self.rims.insert((k, l, i), e);
        Ok(e)
    }

    /// The edge along piece `p` from segment `i`'s start: a line, or a circle about an arc
    /// piece's axis.
    fn rail(&mut self, p: usize, l: usize, i: usize) -> Result<(EdgeId, Sense), MakeError> {
        let i = i % self.count(l).max(1);
        if let Some(&e) = self.rails.get(&(p, l, i)) {
            return Ok(e);
        }
        let (va, vb) = (self.vertex(p, l, i)?, self.vertex(p + 1, l, i)?);
        let run = self
            .plan
            .runs
            .get(p)
            .copied()
            .ok_or(MakeError::Defect(Defect::Interval))?;
        let e = match run {
            Run::Line => self.asm.line(va, vb)?,
            Run::Arc {
                foot, axis, angle, ..
            } => {
                let (_, place) = self.section(p)?;
                let q = place.point(self.segment(l, i)?.start());
                let centre = foot + axis * (q - foot).dot(axis);
                let radial = q - centre;
                let frame = Frame::new(centre, axis, radial)
                    .ok_or(MakeError::Defect(Defect::PlaneFrame))?;
                let circle = Circle::new(frame, Length::from_mm(radial.length()), Sense::Forward)?;
                let interval =
                    Extent1d::new(0.0, angle).ok_or(MakeError::Defect(Defect::Interval))?;
                (
                    self.asm.edge(Curve::Circle(circle), interval, [va, vb])?,
                    Sense::Forward,
                )
            }
        };
        self.rails.insert((p, l, i), e);
        Ok(e)
    }

    /// Every wall, in the record's order.
    fn walls(&mut self) -> Result<Vec<Vec<Vec<FaceId>>>, MakeError> {
        for l in 0..self.plan.loops.len() {
            // A closed path's holes line hollows of their own; an open one's caps join them
            // to the outer loop's shell.
            let sides = if l == 0 || !self.plan.closed {
                self.asm.sides()?
            } else {
                self.asm.hollow()?
            };
            self.sides.push(sides);
        }
        let mut out = Vec::with_capacity(self.plan.runs.len());
        for p in 0..self.plan.runs.len() {
            let mut per_loop = Vec::with_capacity(self.plan.loops.len());
            for l in 0..self.plan.loops.len() {
                let n = self.count(l);
                let mut faces = vec![None; n];
                for i in 0..n {
                    let face = self.wall(p, l, i)?;
                    let at = self
                        .plan
                        .loops
                        .get(l)
                        .map(|lp| lp.at(i))
                        .ok_or(MakeError::Defect(Defect::Interval))?;
                    let slot = faces
                        .get_mut(at.segment)
                        .ok_or(MakeError::Defect(Defect::Interval))?;
                    *slot = Some(face);
                }
                per_loop.push(
                    faces
                        .into_iter()
                        .collect::<Option<Vec<_>>>()
                        .ok_or(MakeError::Defect(Defect::Interval))?,
                );
            }
            out.push(per_loop);
        }
        Ok(out)
    }

    /// The wall of segment `i` of loop `l` along piece `p`: seen from its front, along the
    /// segment in the piece's start section, along the piece from its end, back along the
    /// segment in the end section, back along the piece to its start.
    fn wall(&mut self, p: usize, l: usize, i: usize) -> Result<FaceId, MakeError> {
        let surface = self
            .plan
            .surfaces
            .get(p)
            .and_then(|w| w.get(l))
            .and_then(|w| w.get(i))
            .cloned()
            .ok_or(MakeError::Defect(Defect::Interval))?;
        let sides = self
            .sides
            .get(l)
            .copied()
            .ok_or(MakeError::Defect(Defect::Interval))?;
        let whole = self.plan.sections.len() == 1;
        if whole && matches!(surface, Surface::Plane(_)) {
            // A whole turn's plane is an annulus: a ring per end, the wider first.
            let (_, place) = self.section(0)?;
            let s = self.segment(l, i)?;
            let Some(Run::Arc { foot, axis, .. }) = self.plan.runs.first().copied() else {
                return Err(MakeError::Defect(Defect::Interval));
            };
            let radius = |q: Point2| {
                let v = place.point(q) - foot;
                (v - axis * v.dot(axis)).length()
            };
            let mut rings = [
                (radius(s.end()), vec![self.rail(p, l, i + 1)?]),
                (radius(s.start()), {
                    let (e, s) = self.rail(p, l, i)?;
                    vec![(e, s.reversed())]
                }),
            ];
            rings.sort_by(|a, b| b.0.total_cmp(&a.0));
            let loops: Vec<_> = rings.into_iter().map(|(_, u)| u).collect();
            return self.asm.face_between(surface, &loops, sides);
        }
        let (back, back_sense) = self.rim(p + 1, l, i)?;
        let (down, down_sense) = self.rail(p, l, i)?;
        let uses = vec![
            self.rim(p, l, i)?,
            self.rail(p, l, i + 1)?,
            (back, back_sense.reversed()),
            (down, down_sense.reversed()),
        ];
        self.asm.face_between(surface, &[uses], sides)
    }

    /// An open path's cap at its start or its end: the section's loops, the outer one
    /// first, its front facing out of the path.
    fn cap(&mut self, end: bool) -> Result<FaceId, MakeError> {
        let k = if end { self.plan.runs.len() } else { 0 };
        let (_, place) = self.section(k)?;
        let [t_start, t_end] = self.plan.tangents;
        let normal = if end { t_end } else { t_start * -1.0 };
        let frame =
            Frame::new(place.at, normal, place.ex).ok_or(MakeError::Defect(Defect::PlaneFrame))?;
        let mut loops = Vec::with_capacity(self.plan.loops.len());
        for l in 0..self.plan.loops.len() {
            let mut uses = Vec::with_capacity(self.count(l));
            for i in 0..self.count(l) {
                uses.push(self.rim(k, l, i)?);
            }
            if !end {
                uses.reverse();
                for u in &mut uses {
                    u.1 = u.1.reversed();
                }
            }
            loops.push(uses);
        }
        self.asm.face(Surface::Plane(Plane::new(frame)), &loops)
    }
}
