// Written for solidcorex. Apache-2.0.
//! The revolve: a planar region turned about an axis in its plane, one wall face per
//! segment of each loop (two where a segment touches the axis inside itself), and a planar
//! cap at each end of a partial turn (research 75 §3.6, Q3, Q10; task MK4).
//!
//! The region is in the frame's `x, y` plane and the axis is the frame's y axis through its
//! origin. A point `(x, y)` of the region is at radius `rho = |x|` and height `h = y`; the
//! turn is right-handed about the axis, from the region's own plane at angle zero.
//!
//! - **Built exactly, chosen before building** (75 R4, R5). A line revolves to a plane
//!   normal to the axis when its ends' heights are within the resolution of each other
//!   (the plane snap rule, Q10), to a cylinder when their radii are, and otherwise to a
//!   cone however low; an arc revolves to a sphere when its centre is on the axis, a ring
//!   torus when its circle stays off the axis, and a horn torus when its circle touches the
//!   axis. A spindle torus (the circle crosses the axis outside the arc) is not made yet.
//! - **Poles.** A profile vertex on the axis is one vertex of the body: each face round it
//!   (a cone's apex, a sphere's or a horn torus's pole) has its own collapsed edge there,
//!   and a plane's loop passes through it. A segment along the axis is no face; its line is
//!   shared by the two caps of a partial turn.
//! - **Touches and crossings** (Q3). An arc whose circle touches the axis inside the arc
//!   (a horn torus) is split at the touch, which is a pole vertex of both pieces; a whole
//!   circle touching the axis is seamed at the touch instead, so its one face has a pole at
//!   each end. A profile that reaches more than the resolution across the axis crosses it
//!   and is [`Refusal::CrossesAxis`].
//! - **Faces.** Every wall's front faces away from the solid. The loops are given in the
//!   orientation the front needs and the writer gives each use its exact pcurve.

use core::f64::consts::{PI, TAU};
use std::collections::BTreeMap;

use scx_brep::{Document, EdgeId, FaceId, ShellId, VertexId};
use scx_curve::{Circle, Curve};
use scx_num::extent1d::Extent1d;
use scx_num::fmath;
use scx_num::frame::{Frame, Sense};
use scx_num::units::Length;
use scx_num::vector2d::Point2;
use scx_num::vector3d::{Point3, Vec3};
use scx_planar::{LoopId, Region, Segment, SegmentRef};
use scx_surface::{Cone, Cylinder, Plane, Sphere, Surface, Torus};

use crate::Made;
use crate::assembly::Assembly;
use crate::error::{Defect, MakeError, Refusal, Unsupported};

/// How far a revolve turns about its axis.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Turn {
    /// The whole turn: a closed solid with no caps.
    Full,
    /// From angle `from` to angle `to` (radians, right-handed about the axis, zero at the
    /// region's own plane), `from < to` and less than a whole turn apart.
    Partial {
        /// The start cap's angle.
        from: f64,
        /// The end cap's angle.
        to: f64,
    },
}

/// What one profile segment revolved to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Wall {
    /// The segment lies on the axis: it sweeps no face.
    OnAxis,
    /// The segment's one face.
    Face(FaceId),
    /// The segment touches the axis inside itself and is split there: the face of the part
    /// from its start (as given) to the touch, and of the part from the touch to its end.
    Split {
        /// The part from the segment's start to the touch.
        before: FaceId,
        /// The part from the touch to the segment's end.
        after: FaceId,
    },
}

/// The faces of a [`revolve`], by role.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RevolveFaces {
    /// The cap at the turn's start, its front facing back along the turn; none for a full
    /// turn.
    pub start: Option<FaceId>,
    /// The cap at the turn's end; none for a full turn.
    pub end: Option<FaceId>,
    /// The wall of each segment, `[loop][segment]`: loop 0 is the outer loop and loop
    /// `1 + k` hole `k`, each segment at its index as given to [`Region::new`], whichever way
    /// the region turned the loop.
    pub walls: Vec<Vec<Wall>>,
}

/// The faces of a [`revolve_sheet`]: its walls, as [`RevolveFaces::walls`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RevolveSheetFaces {
    /// The wall of each segment, `[loop][segment]`.
    pub walls: Vec<Vec<Wall>>,
}

/// Revolves `region`, placed in `frame`'s `x, y` plane, about `frame`'s y axis by `turn`:
/// a solid whose walls are the segments' surfaces of revolution and, for a partial turn,
/// whose caps are the region at each end.
///
/// The region lies on one side of the axis, touching it at most: a vertex, a segment or an
/// arc's inside point within the resolution of the axis is on it.
///
/// # Errors
///
/// - [`Refusal::Resolution`] when the region was built at a resolution other than `doc`'s.
/// - [`Refusal::Turn`] for a partial turn whose angles are not finite, not increasing, a
///   whole turn or more apart, or whose caps the region's farthest point does not take
///   more than the resolution apart (either way round).
/// - [`Refusal::CrossesAxis`] when the region reaches more than the resolution across the
///   axis, at the first segment that does.
/// - [`Unsupported::Segment`] for a segment kind other than a line, an arc or a circle, and
///   [`Unsupported::SpindleTorus`] for an arc whose circle crosses the axis outside it.
/// - A [`Defect`] when the body does not finish, or [`MakeError::Full`].
pub fn revolve(
    region: &Region,
    frame: &Frame,
    turn: Turn,
    doc: &Document,
) -> Result<Made<RevolveFaces>, MakeError> {
    let plan = Plan::new(region, frame, turn, *doc)?;
    let mut build = Build::new(&plan, Assembly::new(doc)?);
    let walls = build.walls()?;
    let (start, end) = if plan.partial {
        (Some(build.cap(false)?), Some(build.cap(true)?))
    } else {
        (None, None)
    };
    Ok(Made {
        body: build.asm.finish()?,
        faces: RevolveFaces { start, end, walls },
    })
}

/// Revolves `region` as [`revolve`] does but makes only its walls, as a sheet: each face has
/// the outside on both sides, and a partial turn has no caps.
///
/// # Errors
///
/// As [`revolve`].
pub fn revolve_sheet(
    region: &Region,
    frame: &Frame,
    turn: Turn,
    doc: &Document,
) -> Result<Made<RevolveSheetFaces>, MakeError> {
    let plan = Plan::new(region, frame, turn, *doc)?;
    let mut build = Build::new(&plan, Assembly::sheet(doc)?);
    let walls = build.walls()?;
    Ok(Made {
        body: build.asm.finish()?,
        faces: RevolveSheetFaces { walls },
    })
}

/// A point of the profile in the half-plane: its radius from the axis and its height.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Rh {
    r: f64,
    h: f64,
}

impl Rh {
    fn minus(self, o: Self) -> (f64, f64) {
        (self.r - o.r, self.h - o.h)
    }

    fn distance(self, o: Self) -> f64 {
        let (dr, dh) = self.minus(o);
        fmath::hypot(dr, dh)
    }
}

/// A piece's path from its start to its end.
#[derive(Debug, Clone, Copy)]
enum Shape {
    Line,
    /// Round the circle about `c` of `radius`, counter-clockwise in `(rho, h)` when `ccw`,
    /// over `sweep` (a whole turn for a whole circle, whose ends are one point).
    Arc {
        c: Rh,
        radius: f64,
        ccw: bool,
        sweep: f64,
    },
}

/// The surface a piece revolves to.
#[derive(Debug, Clone, Copy, PartialEq)]
enum Kind {
    OnAxis,
    /// A plane normal to the axis at height `h`.
    Plane {
        h: f64,
    },
    /// A cylinder of radius `r`.
    Cylinder {
        r: f64,
    },
    Cone,
    Sphere,
    /// A ring torus, or a horn torus whose tube meets the axis.
    Torus {
        horn: bool,
    },
}

/// Which part of its segment a piece is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Part {
    Whole,
    /// The part before (0) or after (1) the touch, along the loop as stored.
    Of(usize),
}

/// A run of the profile that revolves to one face: a segment, or part of one split at an
/// axis touch, from `a` to `b` along the loop as the region stores it.
#[derive(Debug, Clone, Copy)]
struct Piece {
    at: SegmentRef,
    part: Part,
    a: Rh,
    b: Rh,
    shape: Shape,
    kind: Kind,
}

/// The revolve decided before anything is built: the placement, the turn and every piece.
struct Plan {
    origin: Point3,
    /// The axis, the frame's y.
    axis: Vec3,
    /// The radial direction at the start angle and at the end angle (the same for a full
    /// turn).
    radial: [Vec3; 2],
    /// Whether the turn is partial, and its sweep (a whole turn for a full one).
    partial: bool,
    sweep: f64,
    /// `+1` when the region is on the frame's `+x` side, `-1` on its `-x` side: then the
    /// half-plane is the mirror image and every loop runs the other way in it.
    side: f64,
    /// The pieces of each loop, in the region's stored order.
    loops: Vec<Vec<Piece>>,
    /// Each loop's segment count and whether the region turned it.
    given: Vec<(usize, bool)>,
}

impl Plan {
    fn new(region: &Region, frame: &Frame, turn: Turn, doc: Document) -> Result<Self, MakeError> {
        if region.resolution() != doc.resolution() {
            return Err(MakeError::Refused(Refusal::Resolution {
                region: region.resolution(),
                document: doc.resolution(),
            }));
        }
        let zone = doc.resolution().zone().mm();
        let loops: Vec<_> = core::iter::once(region.outer())
            .chain(region.holes())
            .collect();
        let turned: Vec<bool> = core::iter::once(region.outer_reversed())
            .chain(region.holes_reversed().iter().copied())
            .collect();
        let ids: Vec<LoopId> = core::iter::once(LoopId::Outer)
            .chain((0..region.holes().len()).map(LoopId::Hole))
            .collect();
        let refs = |l: usize, i: usize| {
            let n = loops.get(l).map_or(0, |lp| lp.segments().len());
            SegmentRef {
                in_loop: ids.get(l).copied().unwrap_or(LoopId::Outer),
                segment: if turned.get(l).copied().unwrap_or(false) {
                    n - 1 - i
                } else {
                    i
                },
            }
        };
        // Which side of the axis the region is on, and whether it reaches across.
        let mut reaches = Vec::new();
        for (l, lp) in loops.iter().enumerate() {
            for (i, s) in lp.segments().iter().enumerate() {
                let at = refs(l, i);
                reaches.push((
                    at,
                    reach_x(s).ok_or(MakeError::Unsupported(Unsupported::Segment(at)))?,
                ));
            }
        }
        let far = reaches.iter().fold(f64::MIN, |m, (_, (_, hi))| m.max(*hi));
        let side = if far > zone { 1.0 } else { -1.0 };
        let mut rho_max = 0.0_f64;
        for &(at, (lo, hi)) in &reaches {
            let (near, out) = if side > 0.0 { (lo, hi) } else { (-hi, -lo) };
            if near < -zone {
                return Err(MakeError::Refused(Refusal::CrossesAxis { at }));
            }
            rho_max = rho_max.max(out);
        }
        let (partial, from, sweep) = match turn {
            Turn::Full => (false, 0.0, TAU),
            Turn::Partial { from, to } => {
                let sweep = to - from;
                let apart = |a: f64| a * rho_max > zone;
                if !(from.is_finite()
                    && to.is_finite()
                    && sweep > 0.0
                    && sweep < TAU
                    && apart(sweep)
                    && apart(TAU - sweep))
                {
                    return Err(MakeError::Refused(Refusal::Turn { from, to }));
                }
                (true, from, sweep)
            }
        };
        let axis = *frame.y();
        let d0 = *frame.x() * side;
        let w0 = axis.cross(d0);
        let start = d0 * fmath::cos(from) + w0 * fmath::sin(from);
        let end = if partial {
            start * fmath::cos(sweep) + axis.cross(start) * fmath::sin(sweep)
        } else {
            start
        };
        let mut pieces = Vec::with_capacity(loops.len());
        for (l, lp) in loops.iter().enumerate() {
            let mut out = Vec::with_capacity(lp.segments().len());
            for (i, s) in lp.segments().iter().enumerate() {
                pieces_of(s, refs(l, i), side, zone, &mut out)?;
            }
            pieces.push(out);
        }
        let given = loops
            .iter()
            .zip(&turned)
            .map(|(lp, &t)| (lp.segments().len(), t))
            .collect();
        Ok(Self {
            origin: frame.origin(),
            axis,
            radial: [start, end],
            partial,
            sweep,
            side,
            loops: pieces,
            given,
        })
    }

    /// Where `p` lands at the start angle (`k = 0`) or the end angle (`k = 1`).
    fn at(&self, p: Rh, k: usize) -> Point3 {
        if p.r == 0.0 {
            return self.origin + self.axis * p.h;
        }
        self.origin + (self.axis * p.h + self.radial_at(k) * p.r)
    }

    /// The direction `(dr, dh)` of the half-plane at angle `k`, in space.
    fn dir(&self, (dr, dh): (f64, f64), k: usize) -> Vec3 {
        self.radial_at(k) * dr + self.axis * dh
    }

    fn radial_at(&self, k: usize) -> Vec3 {
        self.radial.get(k).copied().unwrap_or(self.radial[0])
    }

    /// The angle indices of a wall's start and end meridians.
    const fn ends(&self) -> [usize; 2] {
        if self.partial { [0, 1] } else { [0, 0] }
    }

    /// A frame at height `h` on the axis, its z the axis and its x the start's radial.
    fn axial(&self, h: f64) -> Result<Frame, MakeError> {
        Frame::new(self.origin + self.axis * h, self.axis, self.radial[0])
            .ok_or(MakeError::Defect(Defect::PlaneFrame))
    }

    /// The surface `p` revolves to, its front away from the solid: the solid lies left of
    /// the loop's travel in the half-plane when `side` is `+1`, right of it otherwise.
    fn surface(&self, p: &Piece) -> Result<Surface, MakeError> {
        let o = self.side;
        let sense = |forward: bool| {
            if forward {
                Sense::Forward
            } else {
                Sense::Reversed
            }
        };
        let (dr, dh) = p.b.minus(p.a);
        Ok(match (p.kind, p.shape) {
            (Kind::Plane { h }, _) => {
                // The front is right of the travel: `-o dr` along the axis.
                let z = self.axis * (-o * dr.signum());
                let frame = Frame::new(self.origin + self.axis * h, z, self.radial[0])
                    .ok_or(MakeError::Defect(Defect::PlaneFrame))?;
                Surface::Plane(Plane::new(frame))
            }
            (Kind::Cylinder { r }, _) => Surface::Cylinder(Cylinder::new(
                self.axial(p.a.h)?,
                Length::from_mm(r),
                sense(o * dh > 0.0),
            )?),
            (Kind::Cone, _) => {
                // The frame at the wider end; the cone's height runs along the axis times
                // the sense, so the half-angle is measured against that.
                let s = sense(o * dh > 0.0);
                let (big, small) = if p.a.r >= p.b.r {
                    (p.a, p.b)
                } else {
                    (p.b, p.a)
                };
                let half_angle = fmath::atan((small.r - big.r) / (s.sign() * (small.h - big.h)));
                Surface::Cone(Cone::new(
                    self.axial(big.h)?,
                    Length::from_mm(big.r),
                    half_angle,
                    s,
                )?)
            }
            (Kind::Sphere, Shape::Arc { c, radius, ccw, .. }) => Surface::Sphere(Sphere::new(
                self.axial(c.h)?,
                Length::from_mm(radius),
                sense(o * if ccw { 1.0 } else { -1.0 } > 0.0),
            )?),
            (Kind::Torus { horn }, Shape::Arc { c, radius, ccw, .. }) => {
                let frame = self.axial(c.h)?;
                let s = sense(o * if ccw { 1.0 } else { -1.0 } > 0.0);
                Surface::Torus(if horn {
                    Torus::horn(frame, Length::from_mm(radius), s)?
                } else {
                    Torus::new(frame, Length::from_mm(c.r), Length::from_mm(radius), s)?
                })
            }
            _ => return Err(MakeError::Defect(Defect::Interval)),
        })
    }
}

/// The least and greatest `x` a segment reaches; `None` for a kind not swept here.
fn reach_x(s: &Segment) -> Option<(f64, f64)> {
    match *s {
        Segment::Line { start, end } => Some((start.x.min(end.x), start.x.max(end.x))),
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
            let r = 0.5 * (dist2(first, centre) + dist2(last, centre));
            let a0 = fmath::atan2(first.y - centre.y, first.x - centre.x);
            let sweep = ccw_sweep(
                (first.x - centre.x, first.y - centre.y),
                (last.x - centre.x, last.y - centre.y),
            );
            let mut lo = first.x.min(last.x);
            let mut hi = first.x.max(last.x);
            if within(0.0, a0, sweep) {
                hi = hi.max(centre.x + r);
            }
            if within(PI, a0, sweep) {
                lo = lo.min(centre.x - r);
            }
            Some((lo, hi))
        }
        Segment::Circle { centre, radius, .. } => Some((centre.x - radius, centre.x + radius)),
        _ => None,
    }
}

fn dist2(p: Point2, q: Point2) -> f64 {
    fmath::hypot(p.x - q.x, p.y - q.y)
}

/// The counter-clockwise angle from `u` to `w`, in `(0, 2 pi]`.
fn ccw_sweep(u: (f64, f64), w: (f64, f64)) -> f64 {
    let s = fmath::atan2(u.0 * w.1 - u.1 * w.0, u.0 * w.0 + u.1 * w.1);
    if s <= 0.0 { s + TAU } else { s }
}

/// Whether the direction at angle `phi` lies strictly inside the counter-clockwise arc from
/// angle `a0` over `sweep`.
fn within(phi: f64, a0: f64, sweep: f64) -> bool {
    let off = (phi - a0).rem_euclid(TAU);
    off > 0.0 && off < sweep
}

/// The pieces of segment `s` (`at`) in the half-plane of `side`, its points within `zone`
/// of the axis put on it, appended to `out`.
fn pieces_of(
    s: &Segment,
    at: SegmentRef,
    side: f64,
    zone: f64,
    out: &mut Vec<Piece>,
) -> Result<(), MakeError> {
    let snap = |r: f64| if r.abs() <= zone { 0.0 } else { r };
    let rh = |p: Point2| Rh {
        r: snap(side * p.x),
        h: p.y,
    };
    // A mirror turns an arc's sense in the half-plane.
    let ccw_of = |sense: Sense| (sense == Sense::Forward) == (side > 0.0);
    match *s {
        Segment::Line { start, end } => {
            let (a, b) = (rh(start), rh(end));
            let kind = if a.r == 0.0 && b.r == 0.0 {
                Kind::OnAxis
            } else if (a.h - b.h).abs() <= zone {
                Kind::Plane {
                    h: 0.5 * (a.h + b.h),
                }
            } else if (a.r - b.r).abs() <= zone {
                Kind::Cylinder {
                    r: 0.5 * (a.r + b.r),
                }
            } else {
                Kind::Cone
            };
            out.push(Piece {
                at,
                part: Part::Whole,
                a,
                b,
                shape: Shape::Line,
                kind,
            });
        }
        Segment::Arc {
            centre,
            start,
            end,
            sense,
        } => {
            let (a, b) = (rh(start), rh(end));
            let ccw = ccw_of(sense);
            let c = rh(centre);
            let radius = 0.5 * (a.distance(c) + b.distance(c));
            let (c, radius, kind) = arc_kind(c, radius, zone, at)?;
            let sweep_of = |p: Rh, q: Rh| {
                let (first, last) = if ccw { (p, q) } else { (q, p) };
                ccw_sweep(first.minus(c), last.minus(c))
            };
            let touch = Rh { r: 0.0, h: c.h };
            let inside = kind == Kind::Torus { horn: true } && a.r != 0.0 && b.r != 0.0 && {
                let first = if ccw { a } else { b };
                let (fr, fh) = first.minus(c);
                within(PI, fmath::atan2(fh, fr), sweep_of(a, b))
            };
            let arc = |p: Rh, q: Rh| Shape::Arc {
                c,
                radius,
                ccw,
                sweep: sweep_of(p, q),
            };
            if inside {
                for (k, (p, q)) in [(a, touch), (touch, b)].into_iter().enumerate() {
                    out.push(Piece {
                        at,
                        part: Part::Of(k),
                        a: p,
                        b: q,
                        shape: arc(p, q),
                        kind,
                    });
                }
            } else {
                out.push(Piece {
                    at,
                    part: Part::Whole,
                    a,
                    b,
                    shape: arc(a, b),
                    kind,
                });
            }
        }
        Segment::Circle {
            centre,
            radius,
            sense,
        } => {
            let c = rh(centre);
            let (c, radius, kind) = arc_kind(c, radius, zone, at)?;
            // The seam where the region puts it, or at the touch of a horn torus.
            let seam = if kind == (Kind::Torus { horn: true }) {
                Rh { r: 0.0, h: c.h }
            } else {
                Rh {
                    r: c.r + radius,
                    h: c.h,
                }
            };
            out.push(Piece {
                at,
                part: Part::Whole,
                a: seam,
                b: seam,
                shape: Shape::Arc {
                    c,
                    radius,
                    ccw: ccw_of(sense),
                    sweep: TAU,
                },
                kind,
            });
        }
        _ => return Err(MakeError::Unsupported(Unsupported::Segment(at))),
    }
    Ok(())
}

/// An arc's surface from its circle: a sphere (the centre on the axis, put there), a ring
/// torus, or a horn torus (the circle touching the axis, its centre's radius and its own
/// made one, their mean); otherwise [`Unsupported::SpindleTorus`].
fn arc_kind(c: Rh, radius: f64, zone: f64, at: SegmentRef) -> Result<(Rh, f64, Kind), MakeError> {
    if c.r == 0.0 {
        Ok((c, radius, Kind::Sphere))
    } else if c.r > radius + zone {
        Ok((c, radius, Kind::Torus { horn: false }))
    } else if (c.r - radius).abs() <= zone {
        let m = 0.5 * (c.r + radius);
        Ok((Rh { r: m, h: c.h }, m, Kind::Torus { horn: true }))
    } else {
        Err(MakeError::Unsupported(Unsupported::SpindleTorus { at }))
    }
}

/// The body being written from a plan: the vertices, parallels and meridians made so far.
struct Build<'p, 'd> {
    plan: &'p Plan,
    asm: Assembly<'d>,
    /// By `(loop, piece, angle)`: the vertex at a piece's start. One on the axis is at
    /// angle 0 only.
    vertices: BTreeMap<(usize, usize, usize), VertexId>,
    /// By `(loop, piece)`: the parallel through a piece's start, running with the turn.
    parallels: BTreeMap<(usize, usize), EdgeId>,
    /// By `(loop, piece, angle)`: a piece's meridian edge, with the sense of a use along
    /// the piece.
    meridians: BTreeMap<(usize, usize, usize), (EdgeId, Sense)>,
    /// By loop: the shells in front of and behind its walls. A hole of a full turn sweeps
    /// a closed hollow of its own, apart from the outer loop's shell.
    sides: Vec<[ShellId; 2]>,
}

impl<'p, 'd> Build<'p, 'd> {
    const fn new(plan: &'p Plan, asm: Assembly<'d>) -> Self {
        Self {
            plan,
            asm,
            vertices: BTreeMap::new(),
            parallels: BTreeMap::new(),
            meridians: BTreeMap::new(),
            sides: Vec::new(),
        }
    }

    fn piece(&self, l: usize, j: usize) -> Result<Piece, MakeError> {
        self.plan
            .loops
            .get(l)
            .and_then(|lp| lp.get(j))
            .copied()
            .ok_or(MakeError::Defect(Defect::Interval))
    }

    fn count(&self, l: usize) -> usize {
        self.plan.loops.get(l).map_or(0, Vec::len)
    }

    /// The vertex at piece `j`'s start, at angle `k`.
    fn vertex(&mut self, l: usize, j: usize, k: usize) -> Result<VertexId, MakeError> {
        let p = self.piece(l, j)?.a;
        let k = if p.r == 0.0 { 0 } else { k };
        if let Some(&v) = self.vertices.get(&(l, j, k)) {
            return Ok(v);
        }
        let v = self.asm.vertex(self.plan.at(p, k))?;
        self.vertices.insert((l, j, k), v);
        Ok(v)
    }

    /// The parallel through piece `j`'s start: the arc round the axis over the turn, or the
    /// whole circle of a full turn.
    fn parallel(&mut self, l: usize, j: usize) -> Result<EdgeId, MakeError> {
        if let Some(&e) = self.parallels.get(&(l, j)) {
            return Ok(e);
        }
        let p = self.piece(l, j)?.a;
        let [k0, k1] = self.plan.ends();
        let ends = [self.vertex(l, j, k0)?, self.vertex(l, j, k1)?];
        let circle = Circle::new(self.plan.axial(p.h)?, Length::from_mm(p.r), Sense::Forward)?;
        let interval =
            Extent1d::new(0.0, self.plan.sweep).ok_or(MakeError::Defect(Defect::Interval))?;
        let e = self.asm.edge(Curve::Circle(circle), interval, ends)?;
        self.parallels.insert((l, j), e);
        Ok(e)
    }

    /// Piece `j`'s meridian edge at angle `k`, with the sense of a use from its start to its
    /// end.
    fn meridian(&mut self, l: usize, j: usize, k: usize) -> Result<(EdgeId, Sense), MakeError> {
        if let Some(&m) = self.meridians.get(&(l, j, k)) {
            return Ok(m);
        }
        let p = self.piece(l, j)?;
        let next = (j + 1) % self.count(l);
        let (va, vb) = (self.vertex(l, j, k)?, self.vertex(l, next, k)?);
        let m = match p.shape {
            Shape::Line => self.asm.line(va, vb)?,
            Shape::Arc {
                c,
                radius,
                ccw,
                sweep,
            } => {
                let (first, ends, sense) = if ccw {
                    (p.a, [va, vb], Sense::Forward)
                } else {
                    (p.b, [vb, va], Sense::Reversed)
                };
                let plan = self.plan;
                let normal = plan.radial_at(k).cross(plan.axis);
                let frame = Frame::new(plan.at(c, k), normal, plan.dir(first.minus(c), k))
                    .ok_or(MakeError::Defect(Defect::PlaneFrame))?;
                let circle = Circle::new(frame, Length::from_mm(radius), Sense::Forward)?;
                let interval =
                    Extent1d::new(0.0, sweep).ok_or(MakeError::Defect(Defect::Interval))?;
                (self.asm.edge(Curve::Circle(circle), interval, ends)?, sense)
            }
        };
        self.meridians.insert((l, j, k), m);
        Ok(m)
    }

    /// Every wall, in the record's order.
    fn walls(&mut self) -> Result<Vec<Vec<Wall>>, MakeError> {
        let mut walls: Vec<Vec<Wall>> = self
            .plan
            .given
            .iter()
            .map(|&(n, _)| vec![Wall::OnAxis; n])
            .collect();
        for l in 0..self.plan.loops.len() {
            let sides = if l == 0 || self.plan.partial {
                self.asm.sides()
            } else {
                self.asm.hollow()?
            };
            self.sides.push(sides);
        }
        for l in 0..self.plan.loops.len() {
            let turned = self.plan.given.get(l).is_some_and(|&(_, t)| t);
            let mut split: Option<FaceId> = None;
            for j in 0..self.count(l) {
                let p = self.piece(l, j)?;
                if p.kind == Kind::OnAxis {
                    continue;
                }
                let face = self.wall(l, j, &p)?;
                let slot = walls
                    .get_mut(l)
                    .and_then(|w| w.get_mut(p.at.segment))
                    .ok_or(MakeError::Defect(Defect::Interval))?;
                match p.part {
                    Part::Whole => *slot = Wall::Face(face),
                    Part::Of(0) => split = Some(face),
                    Part::Of(_) => {
                        let first = split.take().ok_or(MakeError::Defect(Defect::Interval))?;
                        // The stored order is the given one unless the region turned the
                        // loop.
                        let (before, after) = if turned { (face, first) } else { (first, face) };
                        *slot = Wall::Split { before, after };
                    }
                }
            }
        }
        Ok(walls)
    }

    /// The wall of piece `j` of loop `l`.
    fn wall(&mut self, l: usize, j: usize, p: &Piece) -> Result<FaceId, MakeError> {
        let surface = self.plan.surface(p)?;
        let next = (j + 1) % self.count(l);
        let flip = self.plan.side < 0.0;
        let turn = |s: Sense| if flip { s.reversed() } else { s };
        let plane = matches!(p.kind, Kind::Plane { .. });
        if plane && !self.plan.partial {
            // A whole annulus or disc: a circle per end off the axis, the wider first.
            let mut rings = Vec::with_capacity(2);
            for (v, r, s) in [(j, p.a.r, Sense::Forward), (next, p.b.r, Sense::Reversed)] {
                if r != 0.0 {
                    rings.push((r, vec![(self.parallel(l, v)?, turn(s))]));
                }
            }
            rings.sort_by(|x, y| y.0.total_cmp(&x.0));
            let loops: Vec<_> = rings.into_iter().map(|(_, u)| u).collect();
            return self.asm.face_between(surface, &loops, self.sides(l)?);
        }
        // For a loop with the solid on its left in the half-plane: along the start's
        // parallel with the turn, out along the end meridian, back along the end's
        // parallel, back along the start meridian. A pole is the face's own collapsed
        // edge; a plane passes through it.
        let [k0, k1] = self.plan.ends();
        let mut uses = Vec::with_capacity(4);
        self.rim(&mut uses, l, j, p.a, Sense::Forward, plane)?;
        uses.push(self.meridian(l, j, k1)?);
        self.rim(&mut uses, l, next, p.b, Sense::Reversed, plane)?;
        let (e, s) = self.meridian(l, j, k0)?;
        uses.push((e, s.reversed()));
        if flip {
            uses.reverse();
            for u in &mut uses {
                u.1 = u.1.reversed();
            }
        }
        self.asm.face_between(surface, &[uses], self.sides(l)?)
    }

    fn sides(&self, l: usize) -> Result<[ShellId; 2], MakeError> {
        self.sides
            .get(l)
            .copied()
            .ok_or(MakeError::Defect(Defect::Interval))
    }

    /// The use at vertex `j` (at `p`) of a wall: its parallel in `sense`, or on the axis a
    /// collapsed edge of its own (none for a plane).
    fn rim(
        &mut self,
        uses: &mut Vec<(EdgeId, Sense)>,
        l: usize,
        j: usize,
        p: Rh,
        sense: Sense,
        plane: bool,
    ) -> Result<(), MakeError> {
        if p.r != 0.0 {
            uses.push((self.parallel(l, j)?, sense));
        } else if !plane {
            let v = self.vertex(l, j, 0)?;
            uses.push((self.asm.collapsed(v)?, Sense::Forward));
        }
        Ok(())
    }

    /// A partial turn's cap at its start or its end: the region's own loops at that angle,
    /// the outer one first, its front facing out of the turn.
    fn cap(&mut self, end: bool) -> Result<FaceId, MakeError> {
        let k = usize::from(end);
        let plan = self.plan;
        let radial = plan.radial_at(k);
        let normal = if end {
            plan.axis.cross(radial)
        } else {
            radial.cross(plan.axis)
        };
        let frame =
            Frame::new(plan.origin, normal, radial).ok_or(MakeError::Defect(Defect::PlaneFrame))?;
        // Seen from the start cap's front the half-plane's `(rho, h)` turn counter-clockwise,
        // so the loops run as the region's on the `+x` side; the end cap is the other way.
        let along = (plan.side > 0.0) != end;
        let mut loops = Vec::with_capacity(plan.loops.len());
        for l in 0..plan.loops.len() {
            let mut uses = Vec::with_capacity(self.count(l));
            for j in 0..self.count(l) {
                uses.push(self.meridian(l, j, k)?);
            }
            if !along {
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
