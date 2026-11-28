// Written for solidcorex. Apache-2.0.
//! [`planar`]: the 2-D Boolean of two coplanar sheets (research 75 §7 BO17).
//!
//! Each operand is read as regions on `scx-planar`: one [`Region`] per face, in the plane of
//! `a`'s first face (the result's front looks along that plane's normal). The operation is
//! the arrangement of the two boundaries, with no 3-D intersection stage:
//!
//! 1. **Atoms.** Every loop segment becomes pieces of a quarter turn or less (a line is one,
//!    an arc or a circle several), each with its exact conic form for `scx_planar::crossings`.
//! 2. **Cuts.** Every pair of atoms whose boxes meet is crossed; each meet, and the two ends
//!    of each shared stretch, cuts both atoms. Points within the resolution of each other
//!    are one vertex, and an operand's own ends are the vertices they sit at.
//! 3. **Sides.** The pieces between cuts are grouped where two lie on one curve. Each group
//!    knows, for each operand, the faces on its left and on its right: from the operand's own
//!    uses of it (a region's loops run with its material on the left), or, where only the
//!    other operand has it, the faces that contain it.
//! 4. **Select.** The operation picks the groups with the result on one side only, directed
//!    so it is on the left; every other group is interior or outside and dropped.
//! 5. **Loops and regions.** The kept pieces are traced into loops (at a vertex where
//!    several leave, the first met turning clockwise from the way back, ties broken by
//!    curvature), consecutive pieces on one line or circle joined where nothing else meets,
//!    and each outer loop takes the holes inside it as a [`Region`].
//! 6. **Faces.** Each region is a planar face of a sheet in `a`'s plane, its loops given
//!    exact pcurves by the draft ([`Draft::exact_loops`]); lineage names the faces of each
//!    operand that lie on its left.
//!
//! Nothing the operands do not already say is decided by a tolerance but the resolution
//! (vertex clustering and the planarity of the faces read); a tangent direction tie at a
//! vertex (two kept pieces leaving at one angle) falls to curvature.

use core::f64::consts::{FRAC_PI_2, PI, TAU};
use std::collections::{BTreeMap, BTreeSet};

use scx_brep::op::{Cx, Done, Report};
use scx_brep::{
    Brep, Changes, Document, Draft, EdgeGeom, EdgeId, EdgeShape, Entity, FaceId, FaceRecord,
    Origin, RegionId, ShellRecord, VertexId, VertexRecord,
};
use scx_curve::{Circle, Curve, Line};
use scx_num::budget::{Budget, Stop};
use scx_num::extent1d::Extent1d;
use scx_num::fmath;
use scx_num::frame::{Frame, Sense};
use scx_num::units::{Length, Resolution};
use scx_num::vector2d::Point2;
use scx_num::vector3d::Point3;
use scx_planar::{CrossError, Curve2 as Conic, Place, Region, Segment, crossings};
use scx_surface::{Plane, Surface};

use crate::boolean::{BooleanOp, NoItems};
use crate::error::{BooleanError, BooleanRefusal, BooleanUnsupported};
use crate::geom::Side;

#[cfg(test)]
mod tests;

/// The Boolean `op` of the coplanar sheets `a` and `b`, as one sheet of planar faces built
/// from a new draft: its faces lie in the plane of `a`'s first face (of `b`'s when `a` has
/// none), front along that face's normal, each with exact pcurves.
///
/// Both operands are read as sets of regions: every face must be a plane within the
/// resolution of the reference plane, bounded by lines and circular arcs, and a face's
/// loops must form a [`Region`] there. Faces of one operand may share edges (their union
/// is then what the operand covers). An empty answer (an intersection of sheets that do not
/// overlap, say) is a body with no faces. Lineage names, for each result face, the faces of
/// each operand that lie on its boundary's left.
///
/// # Errors
///
/// - [`BooleanRefusal::OtherDocument`] for an operand finished under another resolution
///   than `cx.doc`'s.
/// - [`BooleanRefusal::NotCoplanarSheet`] for a face that is not a plane in the reference
///   plane, and [`BooleanRefusal::BadProfile`] for a face whose loops are no region there.
/// - [`BooleanUnsupported::PlanarEdge`] for an edge that is neither a line nor a circle.
/// - [`BooleanUnsupported::PlanarUndecided`] where the certified crossings or a point test
///   cannot settle a meet within the resolution, or where the result's loops are no region
///   (a hole touching its outer loop at a point).
/// - [`BooleanError::Exhausted`], [`BooleanError::Cancelled`], and [`BooleanError::Defect`]
///   when the result's draft does not finish.
pub fn planar(
    a: &Brep,
    b: &Brep,
    op: BooleanOp,
    cx: &mut Cx<'_>,
) -> Result<Done<NoItems>, BooleanError> {
    let res = cx.doc.resolution();
    for (side, body) in Side::BOTH.into_iter().zip([a, b]) {
        if body.resolution() != res {
            return Err(BooleanError::Refused(BooleanRefusal::OtherDocument {
                operand: side.operand(),
            }));
        }
    }
    let mut budget = cx.budget();
    let frame = reference(a, b).unwrap_or(Frame::WORLD);
    let sides = Pair {
        a: read(a, Side::A, &frame, res)?,
        b: read(b, Side::B, &frame, res)?,
    };
    let atoms = atoms(&sides);
    let cuts = cut(&atoms, res, &mut budget, cx.stop)?;
    let arrangement = arrange(&atoms, &cuts, &sides, res)?;
    let kept = select(&arrangement, op);
    let loops = trace(&kept)?;
    let regions = regions(&loops, &kept, res)?;
    let (body, changes) = sheet(&regions, &sides, &frame, *cx.doc)?;
    Ok(Done::new(body, changes, Report::empty(budget.used())))
}

// ---------------------------------------------------------------------------------------
// Reading the operands.

/// One face of an operand as a region in the reference plane.
struct Piece {
    face: FaceId,
    region: Region,
}

/// The plane of the first face of `a`, else of `b`: the frame of the first plane found.
fn reference(a: &Brep, b: &Brep) -> Option<Frame> {
    [a, b].into_iter().find_map(|body| {
        body.faces().find_map(|f| match f.surface() {
            Surface::Plane(p) => Some(*p.frame()),
            _ => None,
        })
    })
}

/// `p` in the frame's plane: its coordinates and its height above.
pub(crate) fn local(frame: &Frame, p: Point3) -> (Point2, f64) {
    let d = p - frame.origin();
    (
        Point2::new(d.dot(*frame.x()), d.dot(*frame.y())),
        d.dot(*frame.z()),
    )
}

fn read(
    body: &Brep,
    side: Side,
    frame: &Frame,
    res: Resolution,
) -> Result<Vec<Piece>, BooleanError> {
    let mut out = Vec::new();
    for face in body.faces() {
        let region = face_region(face, side, frame, res)?;
        out.push(Piece {
            face: face.id(),
            region,
        });
    }
    Ok(out)
}

/// `face` of `side` as a region of the plane `frame`: each loop's uses as lines and
/// circular arcs, each in the plane within the zone.
pub(crate) fn face_region(
    face: scx_brep::Face<'_>,
    side: Side,
    frame: &Frame,
    res: Resolution,
) -> Result<Region, BooleanError> {
    let zone = res.zone().mm();
    let id = face.id();
    let off = |at| {
        BooleanError::Refused(BooleanRefusal::NotCoplanarSheet {
            operand: side.operand(),
            face: at,
        })
    };
    if !matches!(face.surface(), Surface::Plane(_)) {
        return Err(off(id));
    }
    let mut loops: Vec<Vec<Segment>> = Vec::new();
    for lp in face.loops() {
        let mut segs = Vec::with_capacity(lp.len());
        for u in lp.uses() {
            let (s, h) = local(frame, u.start().point());
            let (e, he) = local(frame, u.end().point());
            if h.abs() > zone || he.abs() > zone {
                return Err(off(id));
            }
            let unsupported = |edge| {
                BooleanError::Unsupported(BooleanUnsupported::PlanarEdge {
                    operand: side.operand(),
                    edge,
                })
            };
            let edge = u.edge();
            let EdgeShape::Curve { curve, interval } = edge.shape() else {
                return Err(unsupported(edge.id()));
            };
            match curve {
                Curve::Line(_) => segs.push(Segment::line(s, e)),
                Curve::Circle(c) => {
                    let f = c.frame();
                    let r = c.radius().mm();
                    let (centre, hc) = local(frame, f.origin());
                    let (px, hx) = local(frame, f.origin() + *f.x() * r);
                    let (_, hy) = local(frame, f.origin() + *f.y() * r);
                    if hc.abs() > zone || hx.abs() > zone || hy.abs() > zone {
                        return Err(off(id));
                    }
                    let _ = px;
                    // Counter-clockwise about the reference z along the use.
                    let flips = [
                        c.sense() == Sense::Reversed,
                        u.sense() == Sense::Reversed,
                        f.z().dot(*frame.z()) < 0.0,
                    ];
                    let ccw = flips.into_iter().filter(|&b| b).count() % 2 == 0;
                    let sense = if ccw { Sense::Forward } else { Sense::Reversed };
                    let [e0, e1] = edge.ends();
                    let whole = e0 == e1 && interval.max() - interval.min() > TAU - 1e-9;
                    segs.push(if whole {
                        Segment::circle(centre, r, sense)
                    } else {
                        Segment::arc(centre, s, e, sense)
                    });
                }
                _ => return Err(unsupported(edge.id())),
            }
        }
        loops.push(segs);
    }
    let mut it = loops.into_iter();
    let Some(outer) = it.next() else {
        return Err(off(id));
    };
    Region::new(outer, it.collect(), res).map_err(|_| {
        BooleanError::Refused(BooleanRefusal::BadProfile {
            operand: side.operand(),
            face: id,
        })
    })
}

// ---------------------------------------------------------------------------------------
// Atoms: pieces of a quarter turn or less, each with its conic.

/// A value for each operand.
#[derive(Debug, Clone, Default)]
pub(crate) struct Pair<T> {
    pub(crate) a: T,
    pub(crate) b: T,
}

impl<T> Pair<T> {
    pub(crate) const fn get(&self, side: Side) -> &T {
        match side {
            Side::A => &self.a,
            Side::B => &self.b,
        }
    }

    pub(crate) const fn get_mut(&mut self, side: Side) -> &mut T {
        match side {
            Side::A => &mut self.a,
            Side::B => &mut self.b,
        }
    }
}

/// Adds the faces of `from` not yet in `into`.
fn merge_faces(into: &mut Pair<Vec<usize>>, from: &Pair<Vec<usize>>) {
    for side in Side::BOTH {
        for &f in from.get(side) {
            let list = into.get_mut(side);
            if !list.contains(&f) {
                list.push(f);
            }
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub(crate) enum Geo {
    Line,
    Arc { c: Point2, r: f64, ccw: bool },
}

/// A box by its corners.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Bbox {
    pub(crate) lo: Point2,
    pub(crate) hi: Point2,
}

impl Bbox {
    pub(crate) fn of(points: &[Point2]) -> Self {
        let fold = |f: fn(f64, f64) -> f64, get: fn(&Point2) -> f64| {
            points.iter().map(get).reduce(f).unwrap_or(0.0)
        };
        Self {
            lo: Point2::new(fold(f64::min, |p| p.x), fold(f64::min, |p| p.y)),
            hi: Point2::new(fold(f64::max, |p| p.x), fold(f64::max, |p| p.y)),
        }
    }

    pub(crate) fn apart(&self, o: &Self, pad: f64) -> bool {
        self.lo.x > o.hi.x + pad
            || o.lo.x > self.hi.x + pad
            || self.lo.y > o.hi.y + pad
            || o.lo.y > self.hi.y + pad
    }
}

pub(crate) struct Atom {
    pub(crate) side: Side,
    pub(crate) face: usize,
    pub(crate) geo: Geo,
    pub(crate) a: Point2,
    pub(crate) b: Point2,
    pub(crate) conic: Conic,
    pub(crate) bbox: Bbox,
}

pub(crate) fn dist(p: Point2, q: Point2) -> f64 {
    let (dx, dy) = (p.x - q.x, p.y - q.y);
    (dx * dx + dy * dy).sqrt()
}

pub(crate) fn angle(c: Point2, p: Point2) -> f64 {
    fmath::atan2(p.y - c.y, p.x - c.x)
}

/// The counter-clockwise turn from `a` to `b` about `c`, in `(0, 2 pi]`.
fn turn(c: Point2, a: Point2, b: Point2) -> f64 {
    let t = (angle(c, b) - angle(c, a)).rem_euclid(TAU);
    if t <= 0.0 { TAU } else { t }
}

fn on(c: Point2, r: f64, ang: f64) -> Point2 {
    Point2::new(c.x + r * fmath::cos(ang), c.y + r * fmath::sin(ang))
}

fn atoms(sides: &Pair<Vec<Piece>>) -> Vec<Atom> {
    let mut out = Vec::new();
    for side in Side::BOTH {
        for (face, p) in sides.get(side).iter().enumerate() {
            let loops = core::iter::once(p.region.outer()).chain(p.region.holes());
            for l in loops {
                for s in l.segments() {
                    split(*s, side, face, &mut out);
                }
            }
        }
    }
    out
}

pub(crate) fn split(seg: Segment, side: Side, face: usize, out: &mut Vec<Atom>) {
    let piece = |geo: Geo, a: Point2, b: Point2, sweep: f64| {
        let (conic, bbox) = match geo {
            Geo::Line => (Conic::Line { start: a, end: b }, Bbox::of(&[a, b])),
            Geo::Arc { c, r, ccw } => {
                let (lo, hi) = if ccw { (a, b) } else { (b, a) };
                let h = 0.5 * sweep;
                let m = angle(c, lo) + h;
                let w = fmath::cos(h);
                let apex = on(c, r / w, m);
                (
                    Conic::Conic {
                        start: lo,
                        apex,
                        end: hi,
                        weight: w,
                    },
                    Bbox::of(&[lo, apex, hi]),
                )
            }
        };
        Atom {
            side,
            face,
            geo,
            a,
            b,
            conic,
            bbox,
        }
    };
    match seg {
        Segment::Line { start, end } => out.push(piece(Geo::Line, start, end, 0.0)),
        Segment::Arc {
            centre,
            start,
            end,
            sense,
        } => {
            let ccw = sense == Sense::Forward;
            let r = 0.5 * (dist(centre, start) + dist(centre, end));
            let total = if ccw {
                turn(centre, start, end)
            } else {
                turn(centre, end, start)
            };
            let n = ((total / FRAC_PI_2) - 1e-9).ceil().max(1.0) as usize;
            let a0 = angle(centre, start);
            let dir = if ccw { 1.0 } else { -1.0 };
            let at = |k: usize| {
                if k == 0 {
                    start
                } else if k == n {
                    end
                } else {
                    on(centre, r, a0 + dir * total * k as f64 / n as f64)
                }
            };
            for k in 0..n {
                let geo = Geo::Arc { c: centre, r, ccw };
                out.push(piece(geo, at(k), at(k + 1), total / n as f64));
            }
        }
        Segment::Circle {
            centre,
            radius,
            sense,
        } => {
            let d = if sense == Sense::Forward { 1.0 } else { -1.0 };
            let q = [
                Point2::new(centre.x + radius, centre.y),
                Point2::new(centre.x, centre.y + d * radius),
                Point2::new(centre.x - radius, centre.y),
                Point2::new(centre.x, centre.y - d * radius),
            ];
            let geo = Geo::Arc {
                c: centre,
                r: radius,
                ccw: sense == Sense::Forward,
            };
            for (from, to) in q.iter().zip(q.iter().cycle().skip(1)) {
                out.push(piece(geo, *from, *to, FRAC_PI_2));
            }
        }
        _ => {}
    }
}

// ---------------------------------------------------------------------------------------
// Cuts.

/// Where each atom is cut, by atom: the points, from meets and shared stretches.
struct Cuts {
    at: BTreeMap<usize, Vec<Point2>>,
}

/// A conic's point at `t` (rational Bezier).
pub(crate) fn eval(c: &Conic, t: f64) -> Point2 {
    match *c {
        Conic::Line { start, end } => Point2::new(
            start.x + (end.x - start.x) * t,
            start.y + (end.y - start.y) * t,
        ),
        Conic::Conic {
            start,
            apex,
            end,
            weight,
        } => {
            let (b0, b1, b2) = ((1.0 - t) * (1.0 - t), 2.0 * t * (1.0 - t) * weight, t * t);
            let w = b0 + b1 + b2;
            Point2::new(
                (b0 * start.x + b1 * apex.x + b2 * end.x) / w,
                (b0 * start.y + b1 * apex.y + b2 * end.y) / w,
            )
        }
        _ => start_of(c),
    }
}

fn start_of(c: &Conic) -> Point2 {
    match c {
        Conic::Line { start, .. } | Conic::Conic { start, .. } => *start,
        _ => Point2::new(0.0, 0.0),
    }
}

/// The meet `at` of two atoms moved onto both curves: the crossing nearest it, from the
/// closed forms of two lines, a line and a circle, or two circles. A touch, which has no
/// two roots, keeps the point the certified isolation gave.
pub(crate) fn polish(p: &Atom, q: &Atom, at: Point2) -> Point2 {
    let roots: Vec<Point2> = match (p.geo, q.geo) {
        (Geo::Line, Geo::Line) => {
            let (d, e) = (
                (p.b.x - p.a.x, p.b.y - p.a.y),
                (q.b.x - q.a.x, q.b.y - q.a.y),
            );
            let den = d.0 * e.1 - d.1 * e.0;
            if den == 0.0 {
                Vec::new()
            } else {
                let t = ((q.a.x - p.a.x) * e.1 - (q.a.y - p.a.y) * e.0) / den;
                vec![Point2::new(p.a.x + t * d.0, p.a.y + t * d.1)]
            }
        }
        (Geo::Line, Geo::Arc { c, r, .. }) => line_circle(p, c, r),
        (Geo::Arc { c, r, .. }, Geo::Line) => line_circle(q, c, r),
        (Geo::Arc { c: c1, r: r1, .. }, Geo::Arc { c: c2, r: r2, .. }) => {
            let (dx, dy) = (c2.x - c1.x, c2.y - c1.y);
            let d = (dx * dx + dy * dy).sqrt();
            let along = (d * d + r1 * r1 - r2 * r2) / (2.0 * d);
            let h2 = r1 * r1 - along * along;
            if d == 0.0 || h2 < 0.0 {
                Vec::new()
            } else {
                let h = h2.sqrt();
                let (ux, uy) = (dx / d, dy / d);
                let m = (c1.x + along * ux, c1.y + along * uy);
                vec![
                    Point2::new(m.0 - h * uy, m.1 + h * ux),
                    Point2::new(m.0 + h * uy, m.1 - h * ux),
                ]
            }
        }
    };
    roots
        .into_iter()
        .filter(|r| dist(*r, at) <= 1e-3)
        .min_by(|u, v| dist(*u, at).total_cmp(&dist(*v, at)))
        .unwrap_or(at)
}

/// Where the line of atom `l` meets the circle about `c` of radius `r`.
fn line_circle(l: &Atom, c: Point2, r: f64) -> Vec<Point2> {
    let (dx, dy) = (l.b.x - l.a.x, l.b.y - l.a.y);
    let len2 = dx * dx + dy * dy;
    let t0 = ((c.x - l.a.x) * dx + (c.y - l.a.y) * dy) / len2;
    let foot = Point2::new(l.a.x + t0 * dx, l.a.y + t0 * dy);
    let off = dist(foot, c);
    let h2 = r * r - off * off;
    if h2 < 0.0 {
        return Vec::new();
    }
    let s = (h2 / len2).sqrt();
    vec![
        Point2::new(foot.x - s * dx, foot.y - s * dy),
        Point2::new(foot.x + s * dx, foot.y + s * dy),
    ]
}

fn cut(
    atoms: &[Atom],
    res: Resolution,
    budget: &mut Budget,
    stop: &Stop,
) -> Result<Cuts, BooleanError> {
    let pad = res.zone().mm();
    let mut at: BTreeMap<usize, Vec<Point2>> = BTreeMap::new();
    for (i, p) in atoms.iter().enumerate() {
        for (j, q) in atoms.iter().enumerate().skip(i + 1) {
            if p.bbox.apart(&q.bbox, pad) {
                continue;
            }
            let found = crossings(&p.conic, &q.conic, res, budget, stop).map_err(|e| match e {
                CrossError::Exhausted(x) => BooleanError::of_budget(x),
                CrossError::Stopped(_) => BooleanError::Cancelled,
                _ => BooleanError::Unsupported(BooleanUnsupported::PlanarUndecided),
            })?;
            let meets = found.meets.iter().map(|m| polish(p, q, m.at));
            let ends = found
                .overlaps
                .iter()
                .flat_map(|o| o.a.map(|t| eval(&p.conic, t)));
            for pt in meets.chain(ends) {
                at.entry(i).or_default().push(pt);
                at.entry(j).or_default().push(pt);
            }
        }
    }
    Ok(Cuts { at })
}

// ---------------------------------------------------------------------------------------
// The arrangement: vertices, pieces between cuts, and groups of pieces on one curve.

/// A vertex of the arrangement: its number and where it is.
#[derive(Debug, Clone, Copy)]
struct Vtx {
    id: usize,
    at: Point2,
}

/// The vertices so far: a point within `tol` of one is that vertex (the earliest), else a
/// new one. An operand's own ends come first, so they name the vertices they sit at.
struct Vertices {
    reps: Vec<Point2>,
    tol: f64,
}

impl Vertices {
    fn of(&mut self, p: Point2) -> Vtx {
        let tol = self.tol;
        if let Some((id, at)) = self
            .reps
            .iter()
            .enumerate()
            .find(|(_, r)| dist(**r, p) <= tol)
        {
            return Vtx { id, at: *at };
        }
        self.reps.push(p);
        Vtx {
            id: self.reps.len() - 1,
            at: p,
        }
    }
}

/// A piece of an atom between two vertices.
#[derive(Debug, Clone, Copy)]
struct Sub {
    geo: Geo,
    side: Side,
    face: usize,
    from: Vtx,
    to: Vtx,
}

impl Sub {
    fn mid(&self) -> Point2 {
        mid(self.geo, self.from.at, self.to.at)
    }
}

/// Pieces on one curve between two vertices.
struct Group {
    from: Vtx,
    to: Vtx,
    geo: Geo,
    /// Per operand: the faces on the left and on the right of `from -> to`.
    left: Pair<Vec<usize>>,
    right: Pair<Vec<usize>>,
}

struct Arrangement {
    groups: Vec<Group>,
}

/// The place along an atom of a point on it, increasing along the atom.
fn param(a: &Atom, p: Point2) -> f64 {
    match a.geo {
        Geo::Line => {
            let (dx, dy) = (a.b.x - a.a.x, a.b.y - a.a.y);
            ((p.x - a.a.x) * dx + (p.y - a.a.y) * dy) / (dx * dx + dy * dy)
        }
        Geo::Arc { c, ccw, .. } => {
            let d = angle(c, p) - angle(c, a.a);
            (if ccw { d } else { -d }).rem_euclid(TAU)
        }
    }
}

fn mid(geo: Geo, p: Point2, q: Point2) -> Point2 {
    match geo {
        Geo::Line => Point2::new(0.5 * (p.x + q.x), 0.5 * (p.y + q.y)),
        Geo::Arc { c, r, .. } => {
            let (ux, uy) = (p.x + q.x - 2.0 * c.x, p.y + q.y - 2.0 * c.y);
            let n = (ux * ux + uy * uy).sqrt();
            Point2::new(c.x + r * ux / n, c.y + r * uy / n)
        }
    }
}

fn arrange(
    atoms: &[Atom],
    cuts: &Cuts,
    sides: &Pair<Vec<Piece>>,
    res: Resolution,
) -> Result<Arrangement, BooleanError> {
    let zone = res.zone().mm();
    let mut verts = Vertices {
        reps: Vec::new(),
        tol: zone,
    };
    let ends: Vec<(Vtx, Vtx)> = atoms
        .iter()
        .map(|a| (verts.of(a.a), verts.of(a.b)))
        .collect();
    let mut subs: Vec<Sub> = Vec::new();
    for (k, (a, &(start, end))) in atoms.iter().zip(&ends).enumerate() {
        let cuts_k = cuts.at.get(&k).map_or(&[][..], Vec::as_slice);
        let mut mids: Vec<(f64, Vtx)> = cuts_k
            .iter()
            .map(|&p| (param(a, p), verts.of(p)))
            .filter(|(_, v)| v.id != start.id && v.id != end.id)
            .collect();
        mids.sort_by(|p, q| p.0.total_cmp(&q.0).then(p.1.id.cmp(&q.1.id)));
        let mut chain = vec![start];
        for (_, v) in mids {
            if chain.last().is_some_and(|l| l.id != v.id) {
                chain.push(v);
            }
        }
        if chain.last().is_some_and(|l| l.id != end.id) {
            chain.push(end);
        }
        for w in chain.windows(2) {
            if let [from, to] = *w {
                subs.push(Sub {
                    geo: a.geo,
                    side: a.side,
                    face: a.face,
                    from,
                    to,
                });
            }
        }
    }
    // Group pieces on one curve: the same two vertices and the same midpoint.
    let mut by_ends: BTreeMap<(usize, usize), Vec<Vec<Sub>>> = BTreeMap::new();
    for sub in subs {
        let key = (sub.from.id.min(sub.to.id), sub.from.id.max(sub.to.id));
        let m = sub.mid();
        let list = by_ends.entry(key).or_default();
        match list
            .iter_mut()
            .find(|g| g.first().is_some_and(|r| dist(r.mid(), m) <= zone))
        {
            Some(g) => g.push(sub),
            None => list.push(vec![sub]),
        }
    }
    let mut groups = Vec::new();
    for members in by_ends.into_values().flatten() {
        let Some(rep) = members.first().copied() else {
            continue;
        };
        let mut g = Group {
            from: rep.from,
            to: rep.to,
            geo: rep.geo,
            left: Pair::default(),
            right: Pair::default(),
        };
        for sub in &members {
            let faces = if sub.from.id == g.from.id {
                g.left.get_mut(sub.side)
            } else {
                g.right.get_mut(sub.side)
            };
            if !faces.contains(&sub.face) {
                faces.push(sub.face);
            }
        }
        let m = rep.mid();
        for side in Side::BOTH {
            if members.iter().any(|s| s.side == side) {
                continue;
            }
            for (f, p) in sides.get(side).iter().enumerate() {
                match p.region.classify(m) {
                    Ok(Place::Inside) => {
                        g.left.get_mut(side).push(f);
                        g.right.get_mut(side).push(f);
                    }
                    Ok(Place::Outside) => {}
                    _ => {
                        return Err(BooleanError::Unsupported(
                            BooleanUnsupported::PlanarUndecided,
                        ));
                    }
                }
            }
        }
        groups.push(g);
    }
    Ok(Arrangement { groups })
}

// ---------------------------------------------------------------------------------------
// Selection and tracing.

/// A kept piece: from `from` to `to` with the result on its left.
struct Kept {
    from: Vtx,
    to: Vtx,
    geo: Geo,
    /// The faces of each operand on its left.
    faces: Pair<Vec<usize>>,
}

fn keeps(op: BooleanOp, a: bool, b: bool) -> bool {
    match op {
        BooleanOp::Union => a || b,
        BooleanOp::Subtract => a && !b,
        BooleanOp::Intersect => a && b,
    }
}

fn select(arr: &Arrangement, op: BooleanOp) -> Vec<Kept> {
    let mut out = Vec::new();
    for g in &arr.groups {
        let l = keeps(op, !g.left.a.is_empty(), !g.left.b.is_empty());
        let r = keeps(op, !g.right.a.is_empty(), !g.right.b.is_empty());
        if l == r {
            continue;
        }
        let geo = |flip: bool| match g.geo {
            Geo::Arc { c, r, ccw } if flip => Geo::Arc { c, r, ccw: !ccw },
            other => other,
        };
        out.push(if l {
            Kept {
                from: g.from,
                to: g.to,
                geo: geo(false),
                faces: g.left.clone(),
            }
        } else {
            Kept {
                from: g.to,
                to: g.from,
                geo: geo(true),
                faces: g.right.clone(),
            }
        });
    }
    out
}

/// A kept piece's tangent angle and signed curvature at its start (`at_end` false) or end.
fn heading(k: &Kept, at_end: bool) -> (f64, f64) {
    let (p, q) = (k.from.at, k.to.at);
    match k.geo {
        Geo::Line => (fmath::atan2(q.y - p.y, q.x - p.x), 0.0),
        Geo::Arc { c, r, ccw } => {
            let u = if at_end { q } else { p };
            let (ux, uy) = (u.x - c.x, u.y - c.y);
            let t = if ccw { (-uy, ux) } else { (uy, -ux) };
            (fmath::atan2(t.1, t.0), if ccw { 1.0 / r } else { -1.0 / r })
        }
    }
}

/// The traced loops: each a list of kept pieces, in order.
fn trace(kept: &[Kept]) -> Result<Vec<Vec<&Kept>>, BooleanError> {
    let defect = || BooleanError::Defect(crate::error::BooleanDefect::Build);
    let mut out_of: BTreeMap<usize, Vec<(usize, &Kept)>> = BTreeMap::new();
    for (i, k) in kept.iter().enumerate() {
        out_of.entry(k.from.id).or_default().push((i, k));
    }
    let mut used: BTreeSet<usize> = BTreeSet::new();
    let mut loops = Vec::new();
    for (start, first) in kept.iter().enumerate() {
        if used.contains(&start) {
            continue;
        }
        let mut lp = vec![first];
        used.insert(start);
        let mut last = first;
        loop {
            let (tangent, _) = heading(last, true);
            let back = tangent + PI;
            let cands = out_of.get(&last.to.id).ok_or_else(defect)?;
            let mut best: Option<(usize, &Kept, f64, f64)> = None;
            for &(c, k) in cands {
                if used.contains(&c) && c != start {
                    continue;
                }
                let (theta, kappa) = heading(k, false);
                let mut cw = (back - theta).rem_euclid(TAU);
                if !(1e-9..=TAU - 1e-9).contains(&cw) {
                    cw = TAU;
                }
                let better = match best {
                    None => true,
                    Some((_, _, bcw, bk)) => {
                        cw < bcw - 1e-9 || ((cw - bcw).abs() <= 1e-9 && kappa > bk)
                    }
                };
                if better {
                    best = Some((c, k, cw, kappa));
                }
            }
            let (next, k, _, _) = best.ok_or_else(defect)?;
            if next == start {
                break;
            }
            used.insert(next);
            lp.push(k);
            last = k;
        }
        loops.push(lp);
    }
    Ok(loops)
}

// ---------------------------------------------------------------------------------------
// Regions.

/// A result region with the faces of each operand on its boundary's left.
struct Built {
    region: Region,
    faces: Pair<Vec<usize>>,
}

fn same_curve(a: Geo, b: Geo, pa: (Point2, Point2), pb: (Point2, Point2), zone: f64) -> bool {
    match (a, b) {
        (Geo::Line, Geo::Line) => {
            // pb continues pa: all three points on one line, the same way.
            let (dx, dy) = (pa.1.x - pa.0.x, pa.1.y - pa.0.y);
            let (ex, ey) = (pb.1.x - pb.0.x, pb.1.y - pb.0.y);
            let n = (dx * dx + dy * dy).sqrt();
            let off = |p: Point2| ((p.x - pa.0.x) * dy - (p.y - pa.0.y) * dx).abs() / n;
            dx * ex + dy * ey > 0.0 && off(pb.1) <= zone
        }
        (
            Geo::Arc {
                c: c1,
                r: r1,
                ccw: w1,
            },
            Geo::Arc {
                c: c2,
                r: r2,
                ccw: w2,
            },
        ) => w1 == w2 && dist(c1, c2) <= zone && (r1 - r2).abs() <= zone,
        _ => false,
    }
}

fn segment_of(geo: Geo, p: Point2, q: Point2) -> Segment {
    match geo {
        Geo::Line => Segment::line(p, q),
        Geo::Arc { c, ccw, .. } => {
            Segment::arc(c, p, q, if ccw { Sense::Forward } else { Sense::Reversed })
        }
    }
}

/// The signed area a traced loop encloses, the circular segments included.
fn loop_area(segs: &[Segment]) -> f64 {
    let mut a = 0.0;
    for s in segs {
        match *s {
            Segment::Line { start, end } => a += 0.5 * (start.x * end.y - end.x * start.y),
            Segment::Arc {
                centre,
                start,
                end,
                sense,
            } => {
                a += 0.5 * (start.x * end.y - end.x * start.y);
                let r = 0.5 * (dist(centre, start) + dist(centre, end));
                let (phi, sign) = if sense == Sense::Forward {
                    (turn(centre, start, end), 1.0)
                } else {
                    (turn(centre, end, start), -1.0)
                };
                a += sign * 0.5 * r * r * (phi - fmath::sin(phi));
            }
            Segment::Circle { radius, sense, .. } => {
                a += sense.sign() * PI * radius * radius;
            }
            _ => {}
        }
    }
    a
}

fn probe(s: &Segment) -> Point2 {
    match *s {
        Segment::Line { start, end } => {
            Point2::new(0.5 * (start.x + end.x), 0.5 * (start.y + end.y))
        }
        Segment::Arc {
            centre,
            start,
            end,
            sense,
        } => {
            let r = 0.5 * (dist(centre, start) + dist(centre, end));
            let phi = if sense == Sense::Forward {
                turn(centre, start, end)
            } else {
                turn(centre, end, start)
            };
            let a0 = angle(centre, start);
            on(centre, r, a0 + sense.sign() * 0.5 * phi)
        }
        Segment::Circle { centre, radius, .. } => Point2::new(centre.x + radius, centre.y),
        _ => Point2::new(0.0, 0.0),
    }
}

/// A traced loop as segments, with the faces on its left.
struct Ring {
    segs: Vec<Segment>,
    faces: Pair<Vec<usize>>,
    area: f64,
}

fn ring(lp: &[&Kept], degree: &BTreeMap<usize, usize>, zone: f64) -> Result<Ring, BooleanError> {
    let defect = || BooleanError::Defect(crate::error::BooleanDefect::Build);
    let n = lp.len();
    // Whether each piece continues the one before it (cyclically) as one curve.
    let joined: Vec<bool> = lp
        .iter()
        .zip(lp.iter().cycle().skip(n.saturating_sub(1)))
        .map(|(cur, prev)| {
            degree.get(&cur.from.id) == Some(&1)
                && same_curve(
                    prev.geo,
                    cur.geo,
                    (prev.from.at, prev.to.at),
                    (cur.from.at, cur.to.at),
                    zone,
                )
        })
        .collect();
    let mut faces: Pair<Vec<usize>> = Pair::default();
    for k in lp {
        merge_faces(&mut faces, &k.faces);
    }
    let Some(first) = joined.iter().position(|j| !j) else {
        // One curve all the way round: a whole circle.
        let Some(Geo::Arc { c, r, ccw }) = lp.first().map(|k| k.geo) else {
            return Err(defect());
        };
        let sense = if ccw { Sense::Forward } else { Sense::Reversed };
        let segs = vec![Segment::circle(c, r, sense)];
        let area = loop_area(&segs);
        return Ok(Ring { segs, faces, area });
    };
    let mut runs: Vec<Vec<&Kept>> = Vec::new();
    for (k, &j) in lp.iter().zip(&joined).cycle().skip(first).take(n) {
        match runs.last_mut() {
            Some(run) if j => run.push(k),
            _ => runs.push(vec![k]),
        }
    }
    let mut segs = Vec::with_capacity(runs.len());
    for run in &runs {
        let (Some(head), Some(tail)) = (run.first(), run.last()) else {
            return Err(defect());
        };
        segs.push(segment_of(head.geo, head.from.at, tail.to.at));
    }
    let area = loop_area(&segs);
    Ok(Ring { segs, faces, area })
}

/// An outer loop, alone as a region, with the holes placed in it.
struct Placed<'a> {
    ring: &'a Ring,
    alone: Region,
    holes: Vec<&'a Ring>,
}

fn regions(
    loops: &[Vec<&Kept>],
    kept: &[Kept],
    res: Resolution,
) -> Result<Vec<Built>, BooleanError> {
    let undecided = || BooleanError::Unsupported(BooleanUnsupported::PlanarUndecided);
    let zone = res.zone().mm();
    let mut degree: BTreeMap<usize, usize> = BTreeMap::new();
    for k in kept {
        *degree.entry(k.from.id).or_default() += 1;
    }
    let rings = loops
        .iter()
        .map(|lp| ring(lp, &degree, zone))
        .collect::<Result<Vec<_>, _>>()?;
    let (outers, holes): (Vec<&Ring>, Vec<&Ring>) = rings.iter().partition(|r| r.area > 0.0);
    // Each outer loop alone, to place the holes in.
    let mut placed = outers
        .into_iter()
        .map(|ring| {
            Region::new(ring.segs.clone(), Vec::new(), res)
                .map(|alone| Placed {
                    ring,
                    alone,
                    holes: Vec::new(),
                })
                .map_err(|_| undecided())
        })
        .collect::<Result<Vec<_>, _>>()?;
    for h in holes {
        let p = probe(h.segs.first().ok_or_else(undecided)?);
        let mut best: Option<(usize, f64)> = None;
        for (i, pl) in placed.iter().enumerate() {
            if pl.alone.classify(p).map_err(|_| undecided())? == Place::Inside
                && best.is_none_or(|(_, area)| pl.ring.area < area)
            {
                best = Some((i, pl.ring.area));
            }
        }
        let (i, _) = best.ok_or_else(undecided)?;
        placed.get_mut(i).ok_or_else(undecided)?.holes.push(h);
    }
    let mut out = Vec::new();
    for pl in placed {
        let mut faces = pl.ring.faces.clone();
        for h in &pl.holes {
            merge_faces(&mut faces, &h.faces);
        }
        let region = Region::new(
            pl.ring.segs.clone(),
            pl.holes.iter().map(|h| h.segs.clone()).collect(),
            res,
        )
        .map_err(|_| undecided())?;
        out.push(Built { region, faces });
    }
    Ok(out)
}

// ---------------------------------------------------------------------------------------
// The sheet.

fn sheet(
    built: &[Built],
    sides: &Pair<Vec<Piece>>,
    frame: &Frame,
    doc: Document,
) -> Result<(Brep, Changes), BooleanError> {
    let defect = || BooleanError::Defect(crate::error::BooleanDefect::Draft);
    let mut draft = Draft::new();
    let surface = Surface::Plane(Plane::new(*frame));
    let mut vertices: BTreeMap<(u64, u64), VertexId> = BTreeMap::new();
    for b in built {
        let loops = core::iter::once(b.region.outer()).chain(b.region.holes());
        let mut uses_of = Vec::new();
        for l in loops {
            let mut uses = Vec::new();
            for s in l.segments() {
                uses.push(edge(&mut draft, &mut vertices, frame, s).ok_or_else(defect)?);
            }
            uses_of.push(uses);
        }
        let records = draft
            .exact_loops(&surface, &uses_of, &doc)
            .map_err(|_| BooleanError::Defect(crate::error::BooleanDefect::Build))?;
        // One shell for each face: the shell of a sheet is a connected set of faces.
        let outside = draft
            .add_shell(ShellRecord::new(RegionId::OUTSIDE))
            .map_err(|_| defect())?;
        let face = draft
            .add_face(FaceRecord::new(
                surface.clone(),
                [outside, outside],
                records,
            ))
            .map_err(|_| defect())?;
        for side in Side::BOTH {
            for &f in b.faces.get(side) {
                let piece = sides.get(side).get(f).ok_or_else(defect)?;
                let from = Origin::Operand(side.operand(), Entity::Face(piece.face));
                draft.derive(face, from).map_err(|_| defect())?;
            }
        }
    }
    draft
        .finish(&doc)
        .map_err(|_| BooleanError::Defect(crate::error::BooleanDefect::Build))
}

/// The place of a plane point in space.
fn place(frame: &Frame, p: Point2) -> Point3 {
    frame.origin() + (*frame.x() * p.x + *frame.y() * p.y)
}

fn vertex(
    draft: &mut Draft,
    vertices: &mut BTreeMap<(u64, u64), VertexId>,
    frame: &Frame,
    p: Point2,
) -> Option<VertexId> {
    let key = (p.x.to_bits(), p.y.to_bits());
    if let Some(&v) = vertices.get(&key) {
        return Some(v);
    }
    let v = draft.add_vertex(VertexRecord::new(place(frame, p))).ok()?;
    vertices.insert(key, v);
    Some(v)
}

/// The edge of `s` and the sense of the use that runs along it.
fn edge(
    draft: &mut Draft,
    vertices: &mut BTreeMap<(u64, u64), VertexId>,
    frame: &Frame,
    s: &Segment,
) -> Option<(EdgeId, Sense)> {
    match *s {
        Segment::Line { start, end } => {
            let (vs, ve) = (
                vertex(draft, vertices, frame, start)?,
                vertex(draft, vertices, frame, end)?,
            );
            let (p, q) = (place(frame, start), place(frame, end));
            let line = Line::new(p, q - p).ok()?;
            let unit = Extent1d::new(0.0, 1.0)?;
            let shape = EdgeShape::Curve {
                curve: Curve::Line(line),
                interval: unit,
            };
            Some((
                draft.add_edge(EdgeGeom::new(shape, [vs, ve])).ok()?,
                Sense::Forward,
            ))
        }
        Segment::Arc {
            centre,
            start,
            end,
            sense,
        } => {
            let (vs, ve) = (
                vertex(draft, vertices, frame, start)?,
                vertex(draft, vertices, frame, end)?,
            );
            let (first, last) = if sense == Sense::Forward {
                (start, end)
            } else {
                (end, start)
            };
            let r = 0.5 * (dist(centre, first) + dist(centre, last));
            let sweep = turn(centre, first, last);
            let from = *frame.x() * (first.x - centre.x) + *frame.y() * (first.y - centre.y);
            let f = Frame::new(place(frame, centre), *frame.z(), from)?;
            let circle = Circle::new(f, Length::from_mm(r), Sense::Forward).ok()?;
            let interval = Extent1d::new(0.0, sweep)?;
            let ends = if sense == Sense::Forward {
                [vs, ve]
            } else {
                [ve, vs]
            };
            let shape = EdgeShape::Curve {
                curve: Curve::Circle(circle),
                interval,
            };
            Some((draft.add_edge(EdgeGeom::new(shape, ends)).ok()?, sense))
        }
        Segment::Circle {
            centre,
            radius,
            sense,
        } => {
            let seam = Point2::new(centre.x + radius, centre.y);
            let v = vertex(draft, vertices, frame, seam)?;
            let f = Frame::new(place(frame, centre), *frame.z(), *frame.x())?;
            let circle = Circle::new(f, Length::from_mm(radius), Sense::Forward).ok()?;
            let interval = Extent1d::new(0.0, TAU)?;
            let shape = EdgeShape::Curve {
                curve: Curve::Circle(circle),
                interval,
            };
            Some((draft.add_edge(EdgeGeom::new(shape, [v, v])).ok()?, sense))
        }
        _ => None,
    }
}
