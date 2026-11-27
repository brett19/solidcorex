// Written for solidcorex. Apache-2.0.
//! Stage 4: face against face (72 §5.4 stage 3; BO2, 72 §14.24).
//!
//! For each face pair of stage 1, [`analytic::intersect`] gives where the two surfaces meet
//! over the faces' uv boxes: curve pieces and isolated points, a coincident overlap, or
//! nothing. Nothing here searches: an answer without a closed form is
//! [`BooleanUnsupported::SsiPair`], never a grid or a marcher, unless the two faces are
//! proved apart (an enclosure of each surface's distance over boxes holding both faces,
//! bisected a bounded number of times): such a pair has nothing to answer and is dropped.
//!
//! - **Curves.** Each piece is cut at the merged points of stage 3 that pass through both
//!   faces (an edge or vertex of the face, or the face itself) and lie on the curve, found
//!   by the curve's inversion. Between two cuts the curve crosses neither face's boundary
//!   where it is inside the other face, so one place decides each sub-span: it is kept as a
//!   [`Section`] when its sample lies inside both faces, dropped when outside either, and
//!   dropped as well when it runs along a boundary edge of either face: that stretch is an
//!   operand edge lying on the other face, a [`Lying`] piece. A sample near a boundary that
//!   decides nothing is refined by halving, at most [`REFINE`] levels, then it is
//!   [`BooleanDefect::UndecidedSpan`].
//! - **Edges on faces.** The spans of stage 2 (an edge within coincidence of the other
//!   operand's surface over the surface's uv box) are cut the same way, at the merged points
//!   on the edge, and each sub-span is kept as a [`Lying`] piece when inside the face or
//!   along one of its boundary edges (`along`), dropped when outside it.
//! - **Coincident faces** (SS4). The overlap's pieces as `analytic::intersect` gives them,
//!   without the pieces of no width (patches that only touch along a line), and whether the
//!   two faces' normals agree: the same-or-opposite rule BO4 classifies by. The region the
//!   faces share is bounded by their [`Lying`] pieces; the overlap is of their uv boxes.
//! - **Touch points.** An isolated point of the answer on both faces and inside at least
//!   one is a [`Touch`], the merged point there named when there is one (BO7's). A point
//!   on both boundaries is where their edges meet: stage 3 has it.
//!
//! Every kept piece ends at merged points, by index into the stage-3 list, except a closed
//! curve no merged point lies on, which is kept whole. A kept piece with an end at no merged
//! point is a defect: a face's region reaches its uv box only along its boundary, and every
//! place the boundary crosses the other face is a stage-2 contact.

use core::f64::consts::TAU;
use std::collections::BTreeMap;

use scx_algo::box_tree::BoxTree;
use scx_brep::{EdgeId, Face, FaceId, UvSide, VertexId};
use scx_curve::Curve;
use scx_num::budget::{Budget, Stop};
use scx_num::extent1d::Extent1d;
use scx_num::extent3d::Extent3d;
use scx_num::fmath;
use scx_num::frame::Frame;
use scx_num::units::{Accuracy, Length};
use scx_num::vector2d::Point2;
use scx_num::vector3d::{Point3, Vec3};
use scx_ssi::SsiError;
use scx_ssi::analytic::{self, Contact, Intersection, IntersectionCurve, Overlap};
use scx_surface::{Closed, Surface, SurfaceKind};

use crate::boolean::Interference;
use crate::contact::{On, Span, Touched};
use crate::error::{BooleanDefect, BooleanError, BooleanUnsupported};
use crate::geom::{Operands, Side, edge_foot, face_foot, recorded};
use crate::points::Point;

/// The most halvings a sub-span's sample is refined by: `2^REFINE - 1` samples at most.
pub(crate) const REFINE: u32 = 3;

/// A closed conic's span is a whole turn when this long: the closed forms give `[0, 2 pi]`.
const FULL_TURN: f64 = TAU - 1e-9;

/// A kept piece of an intersection curve: inside both faces, a new edge for BO3.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Section {
    /// `a`'s face, then `b`'s.
    pub(crate) faces: [FaceId; 2],
    /// The curve, an exact kind; it runs along `n_a x n_b`.
    pub(crate) curve: Curve,
    /// The curve's parameters over the piece (a closed conic's may pass `2 pi`).
    pub(crate) t: Extent1d,
    /// How the surfaces meet along it.
    pub(crate) contact: Contact,
    /// The merged points at the start and the end, by index into the stage-3 list; `None`
    /// for a closed curve no merged point lies on, kept whole.
    pub(crate) ends: Option<[usize; 2]>,
}

/// An operand edge lying on a face of the other operand over a piece inside that face.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Lying {
    /// The edge's side.
    pub(crate) side: Side,
    /// The edge.
    pub(crate) edge: EdgeId,
    /// The face, of the other side.
    pub(crate) face: FaceId,
    /// The edge's parameters over the piece.
    pub(crate) t: Extent1d,
    /// The face's boundary edge it runs along, when it does (collinear edges); `None`
    /// inside the face.
    pub(crate) along: Option<EdgeId>,
    /// The merged points at the piece's start and end.
    pub(crate) ends: [usize; 2],
}

/// Two faces on one surface, within coincidence, over pieces of their uv boxes' overlap.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Coincidence {
    /// `a`'s face, then `b`'s.
    pub(crate) faces: [FaceId; 2],
    /// Whether the faces' normals agree there (else they are opposite).
    pub(crate) same: bool,
    /// The overlap's pieces of some width, as SS4 gives them (in each surface's uv).
    pub(crate) overlaps: Vec<Overlap>,
}

/// An isolated point where the surfaces touch, on both faces and inside one.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Touch {
    /// `a`'s face, then `b`'s.
    pub(crate) faces: [FaceId; 2],
    /// The point.
    pub(crate) at: Point3,
    /// The merged point there, if any.
    pub(crate) point: Option<usize>,
}

/// What stage 4 found, each list in pair order (then the order of construction).
#[derive(Debug, Clone, Default, PartialEq)]
pub(crate) struct FaceFace {
    /// The kept curve pieces.
    pub(crate) sections: Vec<Section>,
    /// The operand edges lying on faces of the other, by side, edge, face, start.
    pub(crate) lying: Vec<Lying>,
    /// The coincident pairs.
    pub(crate) coincident: Vec<Coincidence>,
    /// The touch points.
    pub(crate) touches: Vec<Touch>,
}

/// Which merged points pass through which faces and edges, per side: read once.
struct Index {
    /// The points through each face (its interior, an edge or a vertex of it), ascending.
    faces: [BTreeMap<FaceId, Vec<usize>>; 2],
    /// The points on each edge inside it, with the edge's parameter there.
    edges: [BTreeMap<EdgeId, Vec<(f64, usize)>>; 2],
    /// The point at each vertex.
    vertices: [BTreeMap<VertexId, usize>; 2],
    /// Each touched face's boundary edges with their boxes, in a tree.
    rims: [BTreeMap<FaceId, Rim>; 2],
}

/// A face's boundary edges with their boxes and accuracies (each derived once per edge),
/// in a box tree: a sample's boundary test reads only the edges whose box it is in.
struct Rim {
    edges: Vec<(EdgeId, Option<Accuracy>)>,
    tree: BoxTree<Extent3d>,
    /// The least accurate edge's accuracy: the tree query's reach.
    worst: Option<Accuracy>,
}

impl Rim {
    /// The boundary edges whose box holds `p` within coincidence of `acc`, ascending.
    fn near(&self, ops: Operands<'_>, p: Point3, acc: Option<Accuracy>) -> Vec<(EdgeId, f64)> {
        let Some(q) = Extent3d::from_point(p) else {
            return Vec::new();
        };
        self.tree
            .overlapping(&q, ops.coincidence(self.worst, acc))
            .into_iter()
            .filter_map(|i| {
                let &(e, ea) = self.edges.get(i)?;
                Some((e, ops.coincidence(ea, acc)))
            })
            .collect()
    }
}

/// The rims of the touched faces of `side`, each edge's box derived once.
fn rims(ops: Operands<'_>, touched: &Touched, side: Side) -> BTreeMap<FaceId, Rim> {
    let body = ops.of(side);
    let mut boxes: BTreeMap<EdgeId, Option<(Extent3d, Option<Accuracy>)>> = BTreeMap::new();
    let mut out = BTreeMap::new();
    for (&f, parts) in touched.faces(side) {
        let mut edges = Vec::new();
        let mut at = Vec::new();
        for &e in &parts.edges {
            let b = *boxes.entry(e).or_insert_with(|| {
                let ed = body.edge(e)?;
                Some((ed.bounding_box()?, recorded(ed.accuracy())))
            });
            if let Some((bx, acc)) = b {
                edges.push((e, acc));
                at.push(bx);
            }
        }
        let worst = edges
            .iter()
            .map(|&(_, a)| a)
            .fold(None, |w, a| if a > w { a } else { w });
        out.insert(
            f,
            Rim {
                tree: BoxTree::new(&at),
                edges,
                worst,
            },
        );
    }
    out
}

impl Index {
    fn of(ops: Operands<'_>, touched: &Touched, points: &[Point]) -> Self {
        let mut ix = Self {
            faces: [BTreeMap::new(), BTreeMap::new()],
            edges: [BTreeMap::new(), BTreeMap::new()],
            vertices: [BTreeMap::new(), BTreeMap::new()],
            rims: Side::BOTH.map(|side| rims(ops, touched, side)),
        };
        for (k, side) in Side::BOTH.into_iter().enumerate() {
            // The faces each vertex and edge bounds, among the touched.
            let mut of_vertex: BTreeMap<VertexId, Vec<FaceId>> = BTreeMap::new();
            let mut of_edge: BTreeMap<EdgeId, Vec<FaceId>> = BTreeMap::new();
            for (&f, parts) in touched.faces(side) {
                for &v in &parts.vertices {
                    of_vertex.entry(v).or_default().push(f);
                }
                for &e in &parts.edges {
                    of_edge.entry(e).or_default().push(f);
                }
            }
            let (Some(faces), Some(edges), Some(vertices)) = (
                ix.faces.get_mut(k),
                ix.edges.get_mut(k),
                ix.vertices.get_mut(k),
            ) else {
                continue;
            };
            for (i, p) in points.iter().enumerate() {
                let Some(on) = p.on.get(k) else {
                    continue;
                };
                let mut through: Vec<FaceId> = Vec::new();
                for o in on {
                    match *o {
                        On::Vertex(v) => {
                            vertices.insert(v, i);
                            through.extend(of_vertex.get(&v).into_iter().flatten());
                        }
                        On::Edge { edge, t } => {
                            edges.entry(edge).or_default().push((t, i));
                            through.extend(of_edge.get(&edge).into_iter().flatten());
                        }
                        On::Face { face, .. } => through.push(face),
                    }
                }
                through.sort_unstable();
                through.dedup();
                for f in through {
                    faces.entry(f).or_default().push(i);
                }
            }
        }
        ix
    }

    /// The points through both `fa` and `fb`, ascending.
    fn through(&self, fa: FaceId, fb: FaceId) -> Vec<usize> {
        let [ma, mb] = &self.faces;
        let (Some(pa), Some(pb)) = (ma.get(&fa), mb.get(&fb)) else {
            return Vec::new();
        };
        // The shorter list looked up in the longer: a cap face of an N-gon prism is in N
        // pairs with N points, each wall in one with a few.
        let (short, long) = if pa.len() <= pb.len() {
            (pa, pb)
        } else {
            (pb, pa)
        };
        short
            .iter()
            .copied()
            .filter(|i| long.binary_search(i).is_ok())
            .collect()
    }
}

/// Where a sample lies against a face.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Place {
    /// Inside, away from the boundary.
    Inside,
    /// Outside.
    Outside,
    /// Within coincidence of this boundary edge, inside it.
    Boundary(EdgeId),
    /// Near the boundary to the boundary test, with no edge within coincidence.
    Near,
}

/// A face of one side, with what the tests read of it.
#[derive(Clone, Copy)]
struct Of<'a> {
    side: Side,
    face: Face<'a>,
}

/// Where `p` (on an entity of accuracy `acc` of the other side) lies against `of`.
fn place(
    ops: Operands<'_>,
    ix: &Index,
    of: Of<'_>,
    p: Point3,
    acc: Option<Accuracy>,
    budget: &mut Budget,
    stop: &Stop,
) -> Result<Place, BooleanError> {
    let id = of.face.id();
    // Along the boundary first: a curve within coincidence of an edge runs on it, whatever
    // the rounding of the uv test says.
    let k = usize::from(of.side == Side::B);
    if let Some(rim) = ix.rims.get(k).and_then(|m| m.get(&id)) {
        let body = ops.of(of.side);
        for (e, reach) in rim.near(ops, p, acc) {
            let Some(ed) = body.edge(e) else {
                continue;
            };
            if let Some((_, _, d)) = edge_foot(ed, of.side, p, budget)?
                && d <= reach
            {
                return Ok(Place::Boundary(e));
            }
        }
    }
    let foot = face_foot(of.face, of.side, p, budget, stop)?;
    match foot.side {
        Some(UvSide::Inside) => Ok(Place::Inside),
        Some(UvSide::Outside) => Ok(Place::Outside),
        Some(UvSide::Near) => Ok(Place::Near),
        None => Err(BooleanError::Defect(BooleanDefect::UvUndecided {
            operand: of.side.operand(),
            face: id,
        })),
    }
}

/// What a sub-span is, from its samples.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Verdict {
    /// Inside every face.
    Keep,
    /// Outside one.
    Drop,
    /// Along this boundary edge of a face of this side, inside or along the others.
    Along(Side, EdgeId),
}

/// The fractions of a sub-span sampled, in order: the middle, then each halving.
fn fractions() -> impl Iterator<Item = f64> {
    (1..=REFINE).flat_map(|level| {
        let n = 1_u32 << level;
        (1..n).step_by(2).map(move |k| f64::from(k) / f64::from(n))
    })
}

/// The verdict on the piece of `curve` over `t` against `faces`.
#[expect(clippy::too_many_arguments, reason = "the stage's read-only context")]
fn judge(
    ops: Operands<'_>,
    ix: &Index,
    faces: &[Of<'_>],
    curve: &Curve,
    t: Extent1d,
    acc: Option<Accuracy>,
    budget: &mut Budget,
    stop: &Stop,
) -> Result<Verdict, BooleanError> {
    let mut along = None;
    let mut near = None;
    for f in fractions() {
        stop.check()?;
        let p = curve.point(t.min() + f * t.length());
        let mut verdict = Some(Verdict::Keep);
        let mut sample_along = None;
        for of in faces {
            match place(ops, ix, *of, p, acc, budget, stop)? {
                Place::Inside => {}
                Place::Outside => return Ok(Verdict::Drop),
                Place::Boundary(e) => {
                    sample_along.get_or_insert((of.side, e));
                    verdict = None;
                }
                Place::Near => {
                    near.get_or_insert(*of);
                    verdict = None;
                }
            }
        }
        if let Some(v) = verdict {
            return Ok(v);
        }
        if along.is_none() {
            along = sample_along;
        }
    }
    // Every sample on a boundary or near it: along an edge if one was found everywhere
    // nothing was only near.
    match (near, along) {
        (None, Some((side, e))) => Ok(Verdict::Along(side, e)),
        (Some(of), _) => Err(undecided(of)),
        (None, None) => faces
            .first()
            .map_or(Ok(Verdict::Drop), |of| Err(undecided(*of))),
    }
}

/// The defect of a sub-span no sample decides against `of`.
fn undecided(of: Of<'_>) -> BooleanError {
    BooleanError::Defect(BooleanDefect::UndecidedSpan {
        operand: of.side.operand(),
        face: of.face.id(),
    })
}

/// The pieces of the curve over `span` between the cuts (parameter, point index), each
/// with its end points; `closed` for a whole turn of a conic. A stub at an open end that is
/// within `reach` of the next cut is dropped (the cut is the end).
fn pieces(
    curve: &Curve,
    span: Extent1d,
    closed: bool,
    cuts: &[(f64, usize)],
    reach: f64,
) -> Vec<(Extent1d, [Option<usize>; 2])> {
    let mut out = Vec::new();
    let piece = |a: f64, b: f64| Extent1d::new(a, b);
    if closed {
        match (cuts.first(), cuts.last()) {
            (Some(&(t0, i0)), Some(&(tn, i_n))) => {
                for w in cuts.windows(2) {
                    if let [(a, i), (b, j)] = *w
                        && let Some(e) = piece(a, b)
                    {
                        out.push((e, [Some(i), Some(j)]));
                    }
                }
                if let Some(e) = piece(tn, t0 + TAU) {
                    out.push((e, [Some(i_n), Some(i0)]));
                }
            }
            _ => out.push((span, [None, None])),
        }
        return out;
    }
    let mut at = (span.min(), None);
    for &(t, i) in cuts {
        let stub = at.1.is_none() && curve.point(at.0).distance(curve.point(t)) <= reach;
        if !stub && let Some(e) = piece(at.0, t) {
            out.push((e, [at.1, Some(i)]));
        }
        at = (t, Some(i));
    }
    let end = span.max();
    let stub = at.1.is_some() && curve.point(at.0).distance(curve.point(end)) <= reach;
    if !stub && let Some(e) = piece(at.0, end) {
        out.push((e, [at.1, None]));
    }
    out
}

/// The parameter on `c` over `span` of a point within `reach` of it, or `None`: the
/// inversion's foot inside the span, or an end of the span the point is at.
fn cut_at(c: &Curve, span: &Extent1d, p: Point3, reach: f64) -> Option<f64> {
    let inv = c.invert(p, Some(span)).ok()?;
    let f = inv.foot();
    if inv.is_inside() {
        return (f.distance <= reach).then_some(f.at);
    }
    [span.min(), span.max()]
        .into_iter()
        .find(|&t| c.point(t).distance(p) <= reach)
}

/// The oriented normal of `s` at `uv`; `None` at a cone's apex and on a NURBS surface.
fn normal(s: &Surface, uv: Point2) -> Option<Vec3> {
    Some(
        match s {
            Surface::Plane(k) => k.normal(uv),
            Surface::Cylinder(k) => k.normal(uv),
            Surface::Cone(k) => k.normal(uv)?,
            Surface::Sphere(k) => k.normal(uv),
            Surface::Torus(k) => k.normal(uv),
            _ => return None,
        }
        .as_vec(),
    )
}

/// The centroid of a polygon's vertices.
fn centroid(poly: &[Point2]) -> Option<Point2> {
    let n = poly.len();
    if n == 0 {
        return None;
    }
    let (su, sv) = poly
        .iter()
        .fold((0.0, 0.0), |(su, sv), p| (su + p.x, sv + p.y));
    #[expect(
        clippy::cast_precision_loss,
        reason = "a polygon has at most eight vertices"
    )]
    let n = n as f64;
    Some(Point2::new(su / n, sv / n))
}

/// Whether an overlap piece has width on `s` beyond `reach`: its centroid's point is
/// farther than `reach` from the point at every side's middle. A piece the clip made
/// within the tolerance of a line (patches that only touch) has none.
fn has_width(s: &Surface, poly: &[Point2], reach: f64) -> bool {
    let Some(c) = centroid(poly) else {
        return false;
    };
    let pc = s.point(c);
    let n = poly.len();
    (0..n).all(|i| {
        let (Some(&p), Some(&q)) = (poly.get(i), poly.get((i + 1) % n)) else {
            return false;
        };
        let m = Point2::new((p.x + q.x) * 0.5, (p.y + q.y) * 0.5);
        s.point(m).distance(pc) > reach
    })
}

/// Whether the faces' normals agree over `o`: at the first of its centroid and vertices
/// where both have one (the polygons are matched vertex for vertex, so their centroids are
/// one point too).
fn same_sense(sa: &Surface, sb: &Surface, o: &Overlap) -> bool {
    let at = centroid(&o.on_a)
        .into_iter()
        .zip(centroid(&o.on_b))
        .chain(o.on_a.iter().copied().zip(o.on_b.iter().copied()));
    for (ua, ub) in at {
        if let (Some(na), Some(nb)) = (normal(sa, ua), normal(sb, ub)) {
            return na.dot(nb) > 0.0;
        }
    }
    true
}

/// The deepest bisection of a pair's shared box in [`apart`]: at most
/// `2^(APART_DEPTH + 1) - 1` boxes, one work unit each.
const APART_DEPTH: u32 = 8;

/// The least and greatest `|t|` over `[lo, hi]`.
fn abs_range(lo: f64, hi: f64) -> [f64; 2] {
    let near = if lo <= 0.0 && 0.0 <= hi {
        0.0
    } else {
        lo.abs().min(hi.abs())
    };
    [near, lo.abs().max(hi.abs())]
}

/// The box `x` in `f`'s coordinates, `[lo, hi]` per axis: the box of its corners'
/// coordinates, which holds every point of `x` (a box is the hull of its corners and the
/// map is affine). With a bound on the coordinates' size, for the rounding slack.
fn local(f: &Frame, x: &Extent3d) -> ([[f64; 2]; 3], f64) {
    let o = f.origin();
    let axes = [f.x().as_vec(), f.y().as_vec(), f.z().as_vec()];
    let mut out = [[f64::INFINITY, f64::NEG_INFINITY]; 3];
    let mut size: f64 = 0.0;
    for c in x.corners() {
        let d = c - o;
        size = size.max(c.x.abs() + o.x.abs() + c.y.abs() + o.y.abs() + c.z.abs() + o.z.abs());
        for (r, a) in out.iter_mut().zip(axes) {
            let t = d.dot(a);
            r[0] = r[0].min(t);
            r[1] = r[1].max(t);
        }
    }
    (out, size)
}

/// An enclosure `[lo, hi]` of `g` over the box `x`, where `g` is a function zero exactly on
/// the surface `s` and 1-Lipschitz (so `|g(p)|` is at most `p`'s distance from `s`), with
/// the size of the numbers it is computed from; `None` for a NURBS surface.
///
/// In the kind's frame, with `rho` the distance from the z axis and `h` the height: a plane
/// `h`; a cylinder `rho - r`; a sphere `|p| - r`; a cone `rho cos(a) - h sin(a) - r cos(a)`
/// (the signed distance in the half-plane of `(rho, h)` from the line `rho = r + h tan(a)`,
/// whose part at `rho >= 0` is the one nappe the cone has); a torus
/// `hypot(rho - R, h) - r` (a ring torus, `r < R`, which the kind enforces: a spindle's
/// inner part is not this zero set). Each is monotone in `|x|`, `|y|`, `rho`, `h` or `|rho - R|`,
/// so the ends of those ranges over the box give its range.
fn level_range(s: &Surface, x: &Extent3d) -> Option<([f64; 2], f64)> {
    let frame = match s {
        Surface::Plane(k) => k.frame(),
        Surface::Cylinder(k) => k.frame(),
        Surface::Cone(k) => k.frame(),
        Surface::Sphere(k) => k.frame(),
        Surface::Torus(k) => k.frame(),
        _ => return None,
    };
    let ([lx, ly, lz], size) = local(frame, x);
    let [ax, bx] = abs_range(lx[0], lx[1]);
    let [ay, by] = abs_range(ly[0], ly[1]);
    let rho = [fmath::hypot(ax, ay), fmath::hypot(bx, by)];
    Some(match s {
        Surface::Plane(_) => (lz, size),
        Surface::Cylinder(k) => {
            let r = k.radius().mm();
            ([rho[0] - r, rho[1] - r], size + r)
        }
        Surface::Sphere(k) => {
            let r = k.radius().mm();
            let [az, bz] = abs_range(lz[0], lz[1]);
            let d = [fmath::hypot(rho[0], az), fmath::hypot(rho[1], bz)];
            ([d[0] - r, d[1] - r], size + r)
        }
        Surface::Cone(k) => {
            let r = k.radius().mm();
            let (sin, cos) = fmath::sin_cos(k.half_angle());
            let [hs0, hs1] = [lz[0] * sin, lz[1] * sin];
            let (hs_lo, hs_hi) = (hs0.min(hs1), hs0.max(hs1));
            (
                [
                    rho[0] * cos - hs_hi - r * cos,
                    rho[1] * cos - hs_lo - r * cos,
                ],
                size + r,
            )
        }
        Surface::Torus(k) => {
            let (big, r) = (k.major().mm(), k.minor().mm());
            let [ar, br] = abs_range(rho[0] - big, rho[1] - big);
            let [az, bz] = abs_range(lz[0], lz[1]);
            (
                [fmath::hypot(ar, az) - r, fmath::hypot(br, bz) - r],
                size + big + r,
            )
        }
        _ => return None,
    })
}

/// Whether `s` has no point within `reach` of the box `x`, by [`level_range`] outside
/// `[-reach, reach]` with a slack for the rounding of the frame's coordinates.
fn clear_of(s: &Surface, x: &Extent3d, reach: f64) -> bool {
    level_range(s, x).is_some_and(|([lo, hi], size)| {
        let slack = 64.0 * f64::EPSILON * (1.0 + size);
        lo > reach + slack || hi < -(reach + slack)
    })
}

/// The two halves of `x` across its longest side.
fn halves(x: Extent3d) -> Option<[Extent3d; 2]> {
    let (lo, hi) = (x.min(), x.max());
    let s = x.size();
    let (mut a, mut b) = (hi, lo);
    if s.x >= s.y && s.x >= s.z {
        let m = 0.5 * (lo.x + hi.x);
        a.x = m;
        b.x = m;
    } else if s.y >= s.z {
        let m = 0.5 * (lo.y + hi.y);
        a.y = m;
        b.y = m;
    } else {
        let m = 0.5 * (lo.z + hi.z);
        a.z = m;
        b.z = m;
    }
    Some([
        Extent3d::from_corners(lo, a)?,
        Extent3d::from_corners(b, hi)?,
    ])
}

/// Whether two faces provably have no points within `reach` of each other (72
/// §14.30): asked of a pair with no closed form before refusing it, so a pair whose
/// boxes meet but whose faces do not is dropped instead.
///
/// The faces lie in their boxes `xa` and `xb`. Two points `p` of `a` and `q` of `b` within
/// `reach` both lie in the boxes each grown by `reach`, and there `p` is on `sa` and within
/// `reach` of `sb`. So if every piece of a cover of the grown boxes' overlap is clear of one
/// of the surfaces by more than `reach` ([`clear_of`]), there are no such points. The
/// overlap is bisected across its longest side, depth first, down to [`APART_DEPTH`]; the
/// first piece at that depth clear of neither is the answer (it may touch). `None` is the
/// proof that the faces do not touch; a piece is only "not proved", the region the refusal
/// names.
fn apart(
    sa: &Surface,
    xa: Extent3d,
    sb: &Surface,
    xb: Extent3d,
    reach: f64,
    budget: &mut Budget,
) -> Result<Option<Extent3d>, BooleanError> {
    let (Some(ga), Some(gb)) = (xa.expanded(reach), xb.expanded(reach)) else {
        return Ok(Some(xa.intersect(xb).unwrap_or_else(|| xa.union(xb))));
    };
    let Some(shared) = ga.intersect(gb) else {
        return Ok(None);
    };
    let mut stack = vec![(shared, 0)];
    while let Some((x, depth)) = stack.pop() {
        budget.charge(1)?;
        if clear_of(sa, &x, reach) || clear_of(sb, &x, reach) {
            continue;
        }
        if depth >= APART_DEPTH {
            return Ok(Some(x));
        }
        let Some([lo, hi]) = halves(x) else {
            return Ok(Some(x));
        };
        stack.push((hi, depth + 1));
        stack.push((lo, depth + 1));
    }
    Ok(None)
}

/// Stage 4 over the stages before it.
///
/// # Errors
///
/// [`BooleanUnsupported::SsiPair`] for a pair of analytic faces whose intersection has no
/// closed form here (72 §5.2); [`BooleanUnsupported::Nurbs`] for a NURBS face in a pair;
/// [`BooleanDefect::UndecidedSpan`], [`BooleanDefect::LooseSection`] and
/// [`BooleanDefect::LooseSpan`] for the method's own failures; [`BooleanError::Exhausted`]
/// and [`BooleanError::Cancelled`].
pub(crate) fn face_face(
    ops: Operands<'_>,
    found: &Interference,
    budget: &mut Budget,
    stop: &Stop,
) -> Result<FaceFace, BooleanError> {
    let touched = &found.touched;
    let ix = Index::of(ops, touched, &found.points);
    let res = ops.a.resolution();
    let mut out = FaceFace::default();
    for &(fa, fb) in &found.pairs {
        stop.check()?;
        let (Some(face_a), Some(face_b), Some(pa), Some(pb)) = (
            ops.a.face(fa),
            ops.b.face(fb),
            touched.parts(Side::A, fa),
            touched.parts(Side::B, fb),
        ) else {
            continue;
        };
        let of = [
            Of {
                side: Side::A,
                face: face_a,
            },
            Of {
                side: Side::B,
                face: face_b,
            },
        ];
        for o in &of {
            if matches!(o.face.surface(), Surface::Nurbs(_)) {
                return Err(BooleanError::Unsupported(BooleanUnsupported::Nurbs {
                    operand: o.side.operand(),
                    at: scx_brep::Entity::Face(o.face.id()),
                }));
            }
        }
        let uv = |o: &Of<'_>| {
            o.face.uv_box().ok_or_else(|| {
                BooleanError::Defect(BooleanDefect::NoUvBox {
                    operand: o.side.operand(),
                    face: o.face.id(),
                })
            })
        };
        let [oa, ob] = of;
        let (ua, ub) = (uv(&oa)?, uv(&ob)?);
        let (sa, sb) = (face_a.surface(), face_b.surface());
        budget.charge(1)?;
        let acc = [
            pa.accuracy.unwrap_or(Accuracy::ZERO),
            pb.accuracy.unwrap_or(Accuracy::ZERO),
        ];
        let reach = ops.coincidence(pa.accuracy, pb.accuracy);
        let kinds = [SurfaceKind::of(sa), SurfaceKind::of(sb)];
        // No closed form here (`General`, or an answer of a kind this stage does not read):
        // the pair is dropped when its faces provably cannot touch, else refused, naming
        // the first region `apart` could not clear. The general intersector (73 row SM9)
        // replaces this.
        let undecided =
            |budget: &mut Budget| -> Result<Option<BooleanError>, BooleanError> {
                let xa = face_a.bounding_box().ok_or(BooleanError::Defect(
                    BooleanDefect::NoFaceBox {
                        operand: Side::A.operand(),
                        face: fa,
                    },
                ))?;
                let xb = face_b.bounding_box().ok_or(BooleanError::Defect(
                    BooleanDefect::NoFaceBox {
                        operand: Side::B.operand(),
                        face: fb,
                    },
                ))?;
                Ok(apart(sa, xa, sb, xb, reach, budget)?.map(|x| {
                    let cause = SsiError::Unresolved {
                        at: x.mid(),
                        size: Length::from_mm(x.max_length()),
                    };
                    BooleanError::of_ssi(cause, fa, fb, kinds)
                }))
            };
        let Closed::Solved(answer) = analytic::intersect(sa, &ua, sb, &ub, acc, res) else {
            if let Some(e) = undecided(budget)? {
                return Err(e);
            }
            continue;
        };
        let worst = if pa.accuracy >= pb.accuracy {
            pa.accuracy
        } else {
            pb.accuracy
        };
        match answer {
            Intersection::Empty => {}
            Intersection::Coincident(pieces) => {
                let overlaps: Vec<Overlap> = pieces
                    .into_iter()
                    .filter(|o| has_width(sa, &o.on_a, reach))
                    .collect();
                if let Some(first) = overlaps.first() {
                    out.coincident.push(Coincidence {
                        faces: [fa, fb],
                        same: same_sense(sa, sb, first),
                        overlaps,
                    });
                }
            }
            Intersection::Pieces { curves, points } => {
                let near = ix.through(fa, fb);
                for c in &curves {
                    sections(
                        ops,
                        &ix,
                        &found.points,
                        &near,
                        &of,
                        c,
                        reach,
                        worst,
                        budget,
                        stop,
                        &mut out.sections,
                    )?;
                }
                for p in points {
                    // Kept when on both faces and inside one: a point on both
                    // boundaries is where their edges meet, a stage-3 point already.
                    let (mut outside, mut boundary) = (false, 0);
                    for o in &of {
                        match place(ops, &ix, *o, p.point, worst, budget, stop)? {
                            Place::Outside => outside = true,
                            Place::Boundary(_) => boundary += 1,
                            Place::Inside | Place::Near => {}
                        }
                    }
                    if !outside && boundary < of.len() {
                        let point = near.iter().copied().find(|&i| {
                            found
                                .points
                                .get(i)
                                .is_some_and(|q| q.at.distance(p.point) <= reach + q.gap)
                        });
                        out.touches.push(Touch {
                            faces: [fa, fb],
                            at: p.point,
                            point,
                        });
                    }
                }
            }
            // A kind of answer this stage does not read yet.
            _ => {
                if let Some(e) = undecided(budget)? {
                    return Err(e);
                }
            }
        }
    }
    for span in &found.spans {
        lying(ops, touched, &ix, span, budget, stop, &mut out.lying)?;
    }
    out.lying.sort_by(|p, q| {
        (p.side, p.edge, p.face)
            .cmp(&(q.side, q.edge, q.face))
            .then(p.t.min().total_cmp(&q.t.min()))
    });
    out.lying.dedup();
    Ok(out)
}

/// The kept pieces of one curve of a pair, into `out`.
#[expect(clippy::too_many_arguments, reason = "the stage's read-only context")]
fn sections(
    ops: Operands<'_>,
    ix: &Index,
    points: &[Point],
    near: &[usize],
    of: &[Of<'_>; 2],
    c: &IntersectionCurve,
    reach: f64,
    acc: Option<Accuracy>,
    budget: &mut Budget,
    stop: &Stop,
    out: &mut Vec<Section>,
) -> Result<(), BooleanError> {
    let [oa, ob] = *of;
    let faces = [oa.face.id(), ob.face.id()];
    let closed =
        matches!(c.curve, Curve::Circle(_) | Curve::Ellipse(_)) && c.span.length() >= FULL_TURN;
    let mut cuts: Vec<(f64, usize)> = Vec::new();
    for &i in near {
        let Some(p) = points.get(i) else {
            continue;
        };
        budget.charge(1)?;
        if let Some(t) = cut_at(&c.curve, &c.span, p.at, reach + p.gap) {
            cuts.push((t, i));
        }
    }
    cuts.sort_by(|p, q| p.0.total_cmp(&q.0).then(p.1.cmp(&q.1)));
    cuts.dedup_by_key(|c| c.1);
    for (t, ends) in pieces(&c.curve, c.span, closed, &cuts, reach) {
        match judge(ops, ix, of, &c.curve, t, acc, budget, stop)? {
            Verdict::Drop | Verdict::Along(..) => {}
            Verdict::Keep => {
                let ends = match ends {
                    [Some(i), Some(j)] => Some([i, j]),
                    [None, None] if closed => None,
                    _ => {
                        return Err(BooleanError::Defect(BooleanDefect::LooseSection { faces }));
                    }
                };
                out.push(Section {
                    faces,
                    curve: c.curve.clone(),
                    t,
                    contact: c.contact,
                    ends,
                });
            }
        }
    }
    Ok(())
}

/// The kept pieces of one stage-2 span, into `out`.
fn lying(
    ops: Operands<'_>,
    touched: &Touched,
    ix: &Index,
    span: &Span,
    budget: &mut Budget,
    stop: &Stop,
    out: &mut Vec<Lying>,
) -> Result<(), BooleanError> {
    let side = span.side;
    let fside = side.other();
    let k = usize::from(side == Side::B);
    let (Some(ed), Some(face), Some(parts)) = (
        ops.of(side).edge(span.edge),
        ops.of(fside).face(span.face),
        touched.parts(fside, span.face),
    ) else {
        return Ok(());
    };
    let scx_brep::EdgeShape::Curve { curve, interval } = ed.shape() else {
        return Ok(());
    };
    let acc = recorded(ed.accuracy());
    let reach = ops.coincidence(acc, parts.accuracy);
    let lo = span.t.min();
    let hi = span.t.max();
    // The points on the edge: inside it by parameter, and at its ends.
    let mut cuts: Vec<(f64, usize)> = ix
        .edges
        .get(k)
        .and_then(|m| m.get(&span.edge))
        .into_iter()
        .flatten()
        .copied()
        .collect();
    let ends = ed.ends();
    for (v, t) in ends.into_iter().zip([interval.min(), interval.max()]) {
        if let Some(&i) = ix.vertices.get(k).and_then(|m| m.get(&v)) {
            cuts.push((t, i));
        }
    }
    let slack = |t: f64| curve.point(t);
    cuts.retain(|&(t, _)| {
        (lo..=hi).contains(&t)
            || slack(t).distance(slack(lo)) <= reach
            || slack(t).distance(slack(hi)) <= reach
    });
    cuts.sort_by(|p, q| p.0.total_cmp(&q.0).then(p.1.cmp(&q.1)));
    // One point named twice (inside the edge and at its end) is one cut; the two ends of a
    // closed edge at one vertex are two, a curve's length apart.
    cuts.dedup_by(|q, p| q.1 == p.1 && slack(0.5 * (p.0 + q.0)).distance(slack(p.0)) <= reach);
    let of = [Of { side: fside, face }];
    let worst = if acc >= parts.accuracy {
        acc
    } else {
        parts.accuracy
    };
    for (t, ends) in pieces(curve, span.t, false, &cuts, reach) {
        let along = match judge(ops, ix, &of, curve, t, worst, budget, stop)? {
            Verdict::Drop => continue,
            Verdict::Keep => None,
            Verdict::Along(_, e) => Some(e),
        };
        let [Some(i), Some(j)] = ends else {
            return Err(BooleanError::Defect(BooleanDefect::LooseSpan {
                operand: side.operand(),
                edge: span.edge,
            }));
        };
        out.push(Lying {
            side,
            edge: span.edge,
            face: span.face,
            t,
            along,
            ends: [i, j],
        });
    }
    Ok(())
}
