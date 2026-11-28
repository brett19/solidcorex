// Written for solidcorex. Apache-2.0.
//! [`imprint_curves`]: curves lying on one face are imprinted and the regions they bound
//! returned (research 75 §13, `BO13b`; the pin's `split_face_by_curves`).
//!
//! The curves are read in the plane of the face as lines and circular arcs (a curve with no
//! exact pcurve there waits for the fitter). Each curve is crossed with the face's boundary
//! and with every other curve by `scx_planar::crossings`, so every meet is certified and
//! moved onto both curves by the closed form; the meets within coincidence of each other are
//! one point. Between its consecutive points a curve is kept where its middle lies inside the
//! face, and each kept piece is a [`Section`] of stage 5 over merged points naming the face's
//! vertices and edges, so the imprint is the one [`imprint`](crate::imprint) writes: the
//! face's edges cut, each piece a new edge with its exact pcurve, the face traced into its
//! regions. Nothing is classified; the body's volume and outward sense are unchanged.

use core::f64::consts::TAU;
use std::collections::BTreeMap;

use scx_brep::op::{Cx, Done};
use scx_brep::{Brep, Draft, FaceId, LoopRecord, VertexId};
use scx_curve::Curve;
use scx_num::budget::{Budget, Stop};
use scx_num::extent1d::Extent1d;
use scx_num::frame::{Frame, Sense};
use scx_num::units::Resolution;
use scx_num::vector2d::Point2;
use scx_num::vector3d::Point3;
use scx_planar::{CrossError, Place, Region, Segment, crossings};
use scx_surface::{OnSurface, Surface, curve_on_surface};

use crate::boolean::{Interference, NoItems};
use crate::contact::On;
use crate::error::{BooleanError, BooleanRefusal, BooleanUnsupported};
use crate::faceface::{FaceFace, Section};
use crate::geom::{Operands, Side, edge_foot, face_foot, recorded};
use crate::imprint::{imprint as stage5, ring};
use crate::imprint_op::finish_target;
use crate::planar::{Atom, face_region, local, polish, split};
use crate::points::Point;

/// A curve to imprint on a face, with the part of it that counts.
#[derive(Debug, Clone, PartialEq)]
pub struct Cut {
    curve: Curve,
    interval: Option<Extent1d>,
}

impl Cut {
    /// The curve over all of it: a line is cut where the face ends, a circle is a whole
    /// circle.
    #[must_use]
    pub const fn whole(curve: Curve) -> Self {
        Self {
            curve,
            interval: None,
        }
    }

    /// The curve over `interval` of its parameter (a curve of a projection, say): each end
    /// of it inside the face must meet the face's boundary or another curve.
    #[must_use]
    pub const fn over(curve: Curve, interval: Extent1d) -> Self {
        Self {
            curve,
            interval: Some(interval),
        }
    }

    /// The curve.
    #[must_use]
    pub const fn curve(&self) -> &Curve {
        &self.curve
    }

    /// The part of the curve that counts, `None` for all of it.
    #[must_use]
    pub const fn interval(&self) -> Option<Extent1d> {
        self.interval
    }
}

/// A region the curves bound: a face of the imprinted body, and a point inside it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CutRegion {
    /// The face in the result.
    pub face: FaceId,
    /// A point of the face, more than the resolution from its boundary, for a caller to pick
    /// the region by (the nearest region to a point it holds).
    pub inside: Point3,
}

/// The imprinted body and the regions the curves bound.
#[derive(Debug, Clone)]
pub struct Imprinted {
    /// The body (the face cut), what changed and the work done.
    pub done: Done<NoItems>,
    /// The regions the face became, ascending by face; one for a face the curves only slit.
    pub regions: Vec<CutRegion>,
}

/// Why curves were refused for a face.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum CutRefusal {
    /// No curve was given.
    NoCurves,
    /// The face is not the body's.
    NoFace(FaceId),
    /// The curve (by index) does not lie on the face's surface.
    OffFace(usize),
    /// The curve (by index) lies on the surface but cuts nothing of the face: it is outside
    /// it or along its boundary.
    Misses(usize),
    /// Two curves (by index) share a stretch.
    Overlap([usize; 2]),
    /// The curve (by index) has an end inside the face that meets neither the boundary nor
    /// another curve.
    Dangles(usize),
}

/// Why a curve is not imprinted yet.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum CutUnsupported {
    /// The face's surface is not a plane: crossings on a curved face wait for the curve-curve
    /// intersector of step 5.
    Surface(FaceId),
    /// The curve (by index) is not a line or a circle: it has no exact pcurve here (the
    /// fitter's).
    Curve(usize),
}

/// One cut curve read in the face's plane.
struct Read {
    curve: Curve,
    /// The parameters the cut covers.
    span: Extent1d,
    /// A whole circle: no ends, every point of it the same as the next turn's.
    closed: bool,
    atoms: Vec<Atom>,
}

/// Curves `curves` imprinted on `face` of `body`, built in `body.edit()`.
///
/// The face's edges are cut where a curve meets them (and so the neighbouring faces' uses
/// of them), each stretch of a curve inside the face is a new edge with its exact pcurve,
/// and the face is split into the regions the curves bound: the pieces of `face` in
/// [`Done::changes`], and as [`Imprinted::regions`] each with a point inside it. A
/// curve is read over its [`Cut`]: a line of no interval is cut where the face ends, a
/// circle is whole. Where two curves cross, both are cut there; a stretch along the face's
/// boundary cuts nothing.
///
/// # Errors
///
/// - [`BooleanRefusal::Cut`]: no curve, a face that is not the body's, a curve off the
///   surface, outside the face or along its boundary only, two curves sharing a stretch, or
///   a curve with an end inside the face that meets nothing;
/// - [`BooleanUnsupported::Cut`]: a face that is not a plane, a curve that is not a line or
///   a circle (no exact pcurve; the fitter's), and as [`planar`](crate::planar) crossings
///   the certified arithmetic cannot settle;
/// - [`BooleanRefusal::OtherDocument`], exhaustion and stop as the other entries.
pub fn imprint_curves(
    body: &Brep,
    face: FaceId,
    curves: &[Cut],
    cx: &mut Cx<'_>,
) -> Result<Imprinted, BooleanError> {
    let res = cx.doc.resolution();
    if body.resolution() != res {
        return Err(BooleanError::Refused(BooleanRefusal::OtherDocument {
            operand: Side::A.operand(),
        }));
    }
    let refuse = |c| BooleanError::Refused(BooleanRefusal::Cut(c));
    if curves.is_empty() {
        return Err(refuse(CutRefusal::NoCurves));
    }
    let view = body
        .face(face)
        .ok_or_else(|| refuse(CutRefusal::NoFace(face)))?;
    let Surface::Plane(plane) = view.surface() else {
        return Err(BooleanError::Unsupported(BooleanUnsupported::Cut(
            CutUnsupported::Surface(face),
        )));
    };
    let frame = *plane.frame();
    let mut budget = cx.budget();
    let stop = cx.stop;
    let region = face_region(view, Side::A, &frame, res)?;
    let mut boundary: Vec<Atom> = Vec::new();
    for lp in core::iter::once(region.outer()).chain(region.holes()) {
        for s in lp.segments() {
            split(*s, Side::A, 0, &mut boundary);
        }
    }
    let reach = reach_of(&boundary);
    let cuts = read(body, view, curves, &frame, reach, res, &mut budget, stop)?;
    let coin = coincidence(body, res);
    let mut pts: Vec<Point2> = Vec::new();
    let mut marks: Vec<Vec<(usize, Point2)>> = vec![Vec::new(); cuts.len()];
    // Each cut against the boundary, then the later cuts.
    for (i, c) in cuts.iter().enumerate() {
        for p in &c.atoms {
            for q in &boundary {
                for at in meets(p, q, res, &mut budget, stop, None)? {
                    note(&mut marks, &mut pts, i, at, coin);
                }
            }
        }
        for (j, d) in cuts.iter().enumerate().skip(i + 1) {
            for p in &c.atoms {
                for q in &d.atoms {
                    for at in meets(p, q, res, &mut budget, stop, Some([i, j]))? {
                        note(&mut marks, &mut pts, i, at, coin);
                        note(&mut marks, &mut pts, j, at, coin);
                    }
                }
            }
        }
    }
    // The pieces each cut keeps, between its consecutive points.
    let mut kept: Vec<Piece> = Vec::new();
    for (i, c) in cuts.iter().enumerate() {
        let before = kept.len();
        let ms = marks.get(i).map_or(&[][..], Vec::as_slice);
        let at = pieces(i, c, ms, &pts, &region, &frame, coin)?;
        kept.extend(at);
        if kept.len() == before {
            return Err(refuse(CutRefusal::Misses(i)));
        }
    }
    // Merged points for stage 5: each with what of the face passes through it.
    let empty = Draft::new()
        .finish(cx.doc)
        .map_err(|_| BooleanError::Defect(crate::error::BooleanDefect::Draft))?
        .0;
    let ops = Operands { a: body, b: &empty };
    let points = merged(ops, view, &pts, &frame, &mut budget, stop)?;
    // A piece's end inside the face that nothing else reaches is a hanging curve.
    let mut degree: BTreeMap<usize, usize> = BTreeMap::new();
    for k in &kept {
        if let Some([p, q]) = k.ends {
            *degree.entry(p).or_default() += 1;
            *degree.entry(q).or_default() += 1;
        }
    }
    for k in &kept {
        let Some(ends) = k.ends else { continue };
        for p in ends {
            let on_face_only = points
                .get(p)
                .is_some_and(|x| x.on[0].iter().all(|o| matches!(o, On::Face { .. })));
            if on_face_only && degree.get(&p).copied().unwrap_or(0) < 2 {
                return Err(refuse(CutRefusal::Dangles(k.cut)));
            }
        }
    }
    let found = Interference {
        pairs: Vec::new(),
        points,
        spans: Vec::new(),
        touched: crate::contact::Touched::default(),
    };
    let ff = FaceFace {
        sections: kept
            .iter()
            .map(|k| Section {
                faces: [face, face],
                curve: k.curve.clone(),
                t: k.t,
                contact: scx_ssi::analytic::Contact::Transversal,
                ends: k.ends,
            })
            .collect(),
        ..FaceFace::default()
    };
    let m = stage5(ops, &found, &ff, *cx.doc, &mut budget, stop)?;
    let ids = m.pieces.get(&(Side::A, face)).cloned().unwrap_or_default();
    let done = finish_target(m, cx, &budget)?;
    let mut regions = Vec::with_capacity(ids.len());
    for id in ids {
        let f = done
            .body
            .face(id)
            .ok_or(BooleanError::Defect(crate::error::BooleanDefect::Draft))?;
        let uv = inside(&f.record().loops).ok_or(BooleanError::Defect(
            crate::error::BooleanDefect::Trace {
                operand: Side::A.operand(),
                face: id,
            },
        ))?;
        regions.push(CutRegion {
            face: id,
            inside: f.surface().point(uv),
        });
    }
    regions.sort_by_key(|r| r.face);
    Ok(Imprinted { done, regions })
}

/// The coincidence distance of the document.
fn coincidence(body: &Brep, res: Resolution) -> f64 {
    let ops = Operands { a: body, b: body };
    let _ = res;
    ops.coincidence(None, None)
}

/// The size of the face's boundary: a box over every atom, for a line's chord.
fn reach_of(boundary: &[Atom]) -> [Point2; 2] {
    let boxes: Vec<_> = boundary.iter().map(|a| a.bbox).collect();
    let fold = |f: fn(f64, f64) -> f64, get: fn(&crate::planar::Bbox) -> f64| {
        boxes.iter().map(get).reduce(f).unwrap_or(0.0)
    };
    [
        Point2::new(fold(f64::min, |b| b.lo.x), fold(f64::min, |b| b.lo.y)),
        Point2::new(fold(f64::max, |b| b.hi.x), fold(f64::max, |b| b.hi.y)),
    ]
}

/// The point of `p` in the plane `frame`, in space.
fn place(frame: &Frame, p: Point2) -> Point3 {
    frame.origin() + *frame.x() * p.x + *frame.y() * p.y
}

/// Reads each curve: proved on the face's surface, in the plane as atoms.
#[expect(clippy::too_many_arguments, reason = "the call's read-only context")]
fn read(
    body: &Brep,
    view: scx_brep::Face<'_>,
    curves: &[Cut],
    frame: &Frame,
    reach: [Point2; 2],
    res: Resolution,
    budget: &mut Budget,
    stop: &Stop,
) -> Result<Vec<Read>, BooleanError> {
    let _ = body;
    let refuse = |c| BooleanError::Refused(BooleanRefusal::Cut(c));
    let unsupported =
        |i| BooleanError::Unsupported(BooleanUnsupported::Cut(CutUnsupported::Curve(i)));
    let uv_box = view.uv_box().ok_or_else(|| {
        BooleanError::Defect(crate::error::BooleanDefect::NoUvBox {
            operand: Side::A.operand(),
            face: view.id(),
        })
    })?;
    // The corners of the face's box in space: a line's chord is cut long enough to leave it.
    let [lo, hi] = reach;
    let corners = [
        place(frame, lo),
        place(frame, Point2::new(hi.x, lo.y)),
        place(frame, hi),
        place(frame, Point2::new(lo.x, hi.y)),
    ];
    let mut out = Vec::with_capacity(curves.len());
    for (i, c) in curves.iter().enumerate() {
        let (span, closed) = match (&c.curve, c.interval) {
            (Curve::Line(l), given) => {
                let d = l.direction();
                let dd = d.dot(d);
                let along = |p: Point3| (p - l.origin()).dot(d) / dd;
                let (a, b) = corners
                    .iter()
                    .map(|p| along(*p))
                    .fold((f64::MAX, f64::MIN), |(a, b), t| (a.min(t), b.max(t)));
                let wide = (b - a).abs().max(1.0);
                let span = given.or_else(|| Extent1d::new(a - wide, b + wide));
                (span, false)
            }
            (Curve::Circle(_), Some(t)) if t.length() < TAU - 1e-9 => (Some(t), false),
            (Curve::Circle(_), Some(t)) => (Extent1d::new(t.min(), t.min() + TAU), true),
            (Curve::Circle(_), None) => (Extent1d::new(0.0, TAU), true),
            _ => return Err(unsupported(i)),
        };
        let span = span.ok_or_else(|| unsupported(i))?;
        let on = curve_on_surface(&c.curve, &span, view.surface(), &uv_box, res, budget, stop)
            .map_err(|e| BooleanError::of_surface(e, Side::A.operand(), view.id()))?;
        match on {
            OnSurface::Leaves(_) => return Err(refuse(CutRefusal::OffFace(i))),
            OnSurface::Proved(p) | OnSurface::Isolated(p) if p.is_empty() => {
                return Err(refuse(CutRefusal::Misses(i)));
            }
            _ => {}
        }
        let seg = segment(&c.curve, &span, closed, frame);
        let mut atoms = Vec::new();
        split(seg, Side::A, i, &mut atoms);
        out.push(Read {
            curve: c.curve.clone(),
            span,
            closed,
            atoms,
        });
    }
    Ok(out)
}

/// The curve over `span` as a plane segment, running with the parameter.
fn segment(curve: &Curve, span: &Extent1d, closed: bool, frame: &Frame) -> Segment {
    let at = |t: f64| local(frame, curve.point(t)).0;
    match curve {
        Curve::Circle(c) => {
            let f = c.frame();
            let flips = [c.sense() == Sense::Reversed, f.z().dot(*frame.z()) < 0.0];
            let ccw = flips.into_iter().filter(|&b| b).count() % 2 == 0;
            let sense = if ccw { Sense::Forward } else { Sense::Reversed };
            let centre = local(frame, f.origin()).0;
            if closed {
                Segment::circle(centre, c.radius().mm(), sense)
            } else {
                Segment::arc(centre, at(span.min()), at(span.max()), sense)
            }
        }
        _ => Segment::line(at(span.min()), at(span.max())),
    }
}

/// The places atoms `p` and `q` meet, on both: a meet moved onto both curves, a shared
/// stretch's two ends. A shared stretch of two cuts (`cuts`) is refused.
fn meets(
    p: &Atom,
    q: &Atom,
    res: Resolution,
    budget: &mut Budget,
    stop: &Stop,
    cuts: Option<[usize; 2]>,
) -> Result<Vec<Point2>, BooleanError> {
    if p.bbox.apart(&q.bbox, res.zone().mm()) {
        return Ok(Vec::new());
    }
    let found = crossings(&p.conic, &q.conic, res, budget, stop).map_err(|e| match e {
        CrossError::Exhausted(x) => BooleanError::of_budget(x),
        CrossError::Stopped(_) => BooleanError::Cancelled,
        _ => BooleanError::Unsupported(BooleanUnsupported::PlanarUndecided),
    })?;
    if let (Some(pair), false) = (cuts, found.overlaps.is_empty()) {
        return Err(BooleanError::Refused(BooleanRefusal::Cut(
            CutRefusal::Overlap(pair),
        )));
    }
    let ends = found
        .overlaps
        .iter()
        .flat_map(|o| o.a.map(|t| crate::planar::eval(&p.conic, t)));
    Ok(found
        .meets
        .iter()
        .map(|m| polish(p, q, m.at))
        .chain(ends)
        .collect())
}

/// Records `at` as a point (the one within `coin` of it, else a new one) of cut `i`.
fn note(
    marks: &mut [Vec<(usize, Point2)>],
    pts: &mut Vec<Point2>,
    i: usize,
    at: Point2,
    coin: f64,
) {
    let k = pts
        .iter()
        .position(|q| crate::planar::dist(*q, at) <= coin)
        .unwrap_or_else(|| {
            pts.push(at);
            pts.len() - 1
        });
    if let Some(m) = marks.get_mut(i)
        && !m.iter().any(|(j, _)| *j == k)
    {
        m.push((k, at));
    }
}

/// A kept piece of a cut: a section for stage 5.
struct Piece {
    cut: usize,
    curve: Curve,
    t: Extent1d,
    /// The points at its start and end; `None` for a whole circle no point lies on.
    ends: Option<[usize; 2]>,
}

/// The parameter of the curve at `p`, inside `span`'s turn when it is a circle.
fn param(c: &Read, p: Point2, frame: &Frame) -> f64 {
    let inv = c.curve.invert(place(frame, p), None);
    let at = inv.map_or_else(|_| c.span.min(), |i| i.foot().at);
    if matches!(c.curve, Curve::Circle(_)) && c.span.length() >= TAU - 1e-9 {
        c.span.min() + (at - c.span.min()).rem_euclid(TAU)
    } else {
        at
    }
}

/// The pieces of cut `c` between its points `marks` that lie inside the face.
fn pieces(
    i: usize,
    c: &Read,
    marks: &[(usize, Point2)],
    pts: &[Point2],
    region: &Region,
    frame: &Frame,
    coin: f64,
) -> Result<Vec<Piece>, BooleanError> {
    let mut ms: Vec<(f64, usize)> = marks
        .iter()
        .map(|(k, p)| (param(c, *p, frame), *k))
        .collect();
    ms.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)));
    let inside = |t: f64| {
        let p = local(frame, c.curve.point(t)).0;
        region.classify(p).is_ok_and(|pl| pl == Place::Inside)
    };
    let mut out = Vec::new();
    let mut push = |a: f64, b: f64, ends: Option<[usize; 2]>| {
        if let Some(t) = Extent1d::new(a, b)
            && inside(t.mid())
        {
            out.push(Piece {
                cut: i,
                curve: c.curve.clone(),
                t,
                ends,
            });
        }
    };
    let (Some(first), Some(last)) = (ms.first().copied(), ms.last().copied()) else {
        // No meet at all: a whole circle inside the face stands alone, anything else is
        // wholly inside (hanging) or outside.
        if c.closed {
            push(c.span.min(), c.span.max(), None);
        } else if inside(c.span.mid()) {
            return Err(BooleanError::Refused(BooleanRefusal::Cut(
                CutRefusal::Dangles(i),
            )));
        }
        return Ok(out);
    };
    for w in ms.windows(2) {
        if let [(ta, pa), (tb, pb)] = *w {
            push(ta, tb, Some([pa, pb]));
        }
    }
    if c.closed {
        push(last.0, first.0 + TAU, Some([last.1, first.1]));
    } else {
        // The stretches beyond the first and the last point: inside the face, an end hangs.
        let far = |t: f64, k: usize| {
            pts.get(k)
                .is_some_and(|q| crate::planar::dist(*q, local(frame, c.curve.point(t)).0) > coin)
        };
        let hang = |a: f64, b: f64, at: f64, k: usize| {
            Extent1d::new(a.min(b), a.max(b)).is_some_and(|t| far(at, k) && inside(t.mid()))
        };
        if hang(c.span.min(), first.0, c.span.min(), first.1)
            || hang(last.0, c.span.max(), c.span.max(), last.1)
        {
            return Err(BooleanError::Refused(BooleanRefusal::Cut(
                CutRefusal::Dangles(i),
            )));
        }
    }
    Ok(out)
}

/// The merged points of stage 5: each of `pts` moved onto the face's vertex it is within
/// coincidence of, else on the edges of the face through it, else inside the face.
fn merged(
    ops: Operands<'_>,
    view: scx_brep::Face<'_>,
    pts: &[Point2],
    frame: &Frame,
    budget: &mut Budget,
    stop: &Stop,
) -> Result<Vec<Point>, BooleanError> {
    let mut edges: Vec<scx_brep::Edge<'_>> = Vec::new();
    for lp in view.loops() {
        for u in lp.uses() {
            let e = u.edge();
            if !edges.iter().any(|x| x.id() == e.id()) {
                edges.push(e);
            }
        }
    }
    edges.sort_by_key(|e| e.id());
    let mut verts: Vec<VertexId> = edges.iter().flat_map(|e| e.ends()).collect();
    verts.sort_unstable();
    verts.dedup();
    let mut out = Vec::with_capacity(pts.len());
    for &p in pts {
        let at = place(frame, p);
        let near = verts.iter().copied().find_map(|v| {
            let vv = ops.a.vertex(v)?;
            let c = ops.coincidence(recorded(vv.record().accuracy.as_deref()), None);
            (vv.point().distance(at) <= c).then_some((v, vv.point()))
        });
        if let Some((v, vp)) = near {
            out.push(Point {
                at: vp,
                anchor: Some((Side::A, v)),
                gap: 0.0,
                on: [vec![On::Vertex(v)], Vec::new()],
                tangent: false,
            });
            continue;
        }
        let mut on = Vec::new();
        for e in &edges {
            if let Some((t, _, d)) = edge_foot(*e, Side::A, at, budget)?
                && d <= ops.coincidence(recorded(e.accuracy()), None)
            {
                on.push(On::Edge { edge: e.id(), t });
            }
        }
        if on.is_empty() {
            let foot = face_foot(view, Side::A, at, budget, stop)?;
            on.push(On::Face {
                face: view.id(),
                uv: foot.uv,
            });
        }
        out.push(Point {
            at,
            anchor: None,
            gap: 0.0,
            on: [on, Vec::new()],
            tangent: false,
        });
    }
    Ok(out)
}

/// A point inside the region the loops bound (the first the outer loop, the rest holes):
/// the middle of the widest stretch inside it on a horizontal line through its box.
fn inside(loops: &[LoopRecord]) -> Option<Point2> {
    let rings: Vec<Vec<Point2>> = loops.iter().map(ring).collect();
    let outer = rings.first()?;
    let (lo, hi) = outer
        .iter()
        .fold((f64::MAX, f64::MIN), |(a, b), p| (a.min(p.y), b.max(p.y)));
    for f in [0.5, 0.37, 0.63, 0.25, 0.75, 0.13, 0.87] {
        let y = lo + f * (hi - lo);
        let mut xs: Vec<f64> = Vec::new();
        for r in &rings {
            let n = r.len();
            for k in 0..n {
                let (Some(a), Some(b)) = (r.get(k), r.get((k + 1) % n)) else {
                    continue;
                };
                if (a.y > y) != (b.y > y) {
                    xs.push(a.x + (y - a.y) / (b.y - a.y) * (b.x - a.x));
                }
            }
        }
        xs.sort_by(f64::total_cmp);
        let best = xs
            .as_chunks::<2>()
            .0
            .iter()
            .filter_map(|c| Some((*c.first()?, *c.get(1)?)))
            .max_by(|p, q| (p.1 - p.0).total_cmp(&(q.1 - q.0)));
        if let Some((a, b)) = best
            && b > a
        {
            return Some(Point2::new(0.5 * (a + b), y));
        }
    }
    None
}
