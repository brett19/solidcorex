// Written for solidcorex. Apache-2.0.
//! A certified enclosure of a volume, and the sign it decides (the pin's `enclosure.rs`,
//! research 72 §14.16): the volume of a shell, or of the body's solid, as an interval that
//! holds the true value, refined until it is as narrow as asked or excludes zero.
//!
//! - **The flux.** A face's volume is the flux of `(0, 0, z - z0)` through it, `z0` one
//!   grid point for the whole set of faces (their edges' box's middle), so the sum is a volume
//!   whatever `z0` is. Green's theorem in the face's uv, as the estimate takes it (the
//!   `face` module): each trim piece over `t` and the span from the face's `base` to the
//!   piece's `v(t)` over `s in [0, 1]` is a cell `Φ(t, s) = S(u(t), a + s b(t))` whose
//!   integrand is `-(Φ_z - z0) (Φ_t x Φ_s)_z`.
//! - **A cell.** The integrand's Taylor polynomial of degree `n - 1` at the cell's middle,
//!   integrated exactly, and the Lagrange remainder from the degree-`n` coefficients over
//!   the whole cell ([`Ser`] seeded over the box), every step in [`Ivl`]. A cell over a
//!   surface break line (an arc join, a knot) is enclosed by the integrand's range, hulled
//!   over each patch it meets, times its area, and split until it is clear of the line.
//! - **The gaps.** The faces' trims and the edges' curves need not meet: each use adds a
//!   strip between `S(pcurve(t))` and the edge's `C(t)`, bounded by its area times the
//!   largest `|z - z0|`, and each corner of a loop a cap bounded by its perimeter. With the
//!   edges' uses balanced (each edge's weighted senses summing to zero, checked), faces,
//!   strips and caps close, so the enclosure holds the volume of the closed boundary.
//! - **Refinement.** The widest cell first (ties by age, so the order is deterministic): a
//!   cell at the first order whose remainder dominates is raised to [`ORDER`]; otherwise it
//!   is split, along `t` or `s` by its remainder's powers. A rigorous sum (outward
//!   rounding) confirms each decision, and the result is the intersection of every
//!   rigorous sum, so refining never widens it.
//! - **Outcomes.** As narrow as asked ([`Settle::Reached`]), or the cell cap or the floor
//!   of the arithmetic first; the sign of an enclosure that still holds zero is a typed
//!   refusal ([`MassUnsupported::SignUndecided`]), never a guess. The budget is charged per
//!   cell evaluation; running out is [`MassError::Exhausted`], never a sign.

use std::cmp::Ordering;
use std::collections::{BTreeMap, BinaryHeap};

use scx_brep::{Brep, EdgeShape, Entity, Face, FaceId, FaceSide, RegionId, ShellId, UseGeom};
use scx_curve::{AngleAxis, Curve, Curve2};
use scx_num::budget::{Budget, Stop};
use scx_num::extent2d::Extent2d;
use scx_num::extent3d::Extent3d;
use scx_num::frame::Sense;
use scx_num::interval::Ivl;
use scx_num::ring::Ring;
use scx_num::vector2d::{Point2, Vec2};
use scx_num::vector3d::Point3;
use scx_nurbs_surface::ParamAxis;
use scx_surface::Surface;

use crate::breaks::{Dir, crossings, curve_breaks, surface_breaks};
use crate::error::{MassDefect, MassError, MassRefusal, MassUnsupported};
use crate::face::{fail, reference};
use crate::kronrod::Meter;
use crate::patch::{self, P3, PatchFail};
use crate::tseries::{Ser, len};

/// The order a cell starts at.
const FIRST_ORDER: usize = 3;
/// The order a cell is raised to when it is the widest and its remainder dominates.
pub const ORDER: usize = 8;
/// The order of a strip's bound.
const STRIP_ORDER: usize = 4;
/// The cells an enclosure may hold before it settles.
pub const CELL_BUDGET: usize = 1 << 17;
/// The splits a cell may take.
const MAX_DEPTH: u32 = 60;
/// The patches a cell over a break line may meet before it is enclosed as the whole line.
const MAX_PATCHES: usize = 64;
/// The samples a piece is read at to find its crossings of the break lines.
const SAMPLES: u32 = 16;

/// Why an enclosure stopped refining.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Settle {
    /// It is as narrow as asked (or its sign is decided).
    Reached,
    /// It holds [`CELL_BUDGET`] cells.
    CellCap,
    /// No cell can be split further: the arithmetic's floor.
    Floor,
}

/// A volume enclosure: the true volume lies in `[lo, hi]`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct VolumeEnclosure {
    /// The lower bound.
    pub lo: f64,
    /// The upper bound.
    pub hi: f64,
    /// Why refinement stopped.
    pub settle: Settle,
    /// The cells it was refined to.
    pub cells: usize,
}

impl VolumeEnclosure {
    /// `hi - lo`.
    #[must_use]
    pub fn width(&self) -> f64 {
        self.hi - self.lo
    }

    /// Whether `v` lies in it.
    #[must_use]
    pub fn holds(&self, v: f64) -> bool {
        self.lo <= v && v <= self.hi
    }
}

/// A certified sign.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Sign {
    /// Above zero.
    Positive,
    /// Below zero.
    Negative,
    /// Exactly zero: a shell with no face counted once (sheets only).
    Zero,
}

/// What a piece of a loop runs along.
#[derive(Clone, Copy)]
enum Path<'a> {
    /// A pcurve.
    Curve(&'a Curve2),
    /// A straight path `a + t d`, `t` in `[0, 1]`.
    Segment { a: Point2, d: Vec2 },
}

/// A piece of a face's boundary over `[lo, hi]`, counted with `sign`.
#[derive(Clone, Copy)]
struct Piece<'a> {
    path: Path<'a>,
    lo: f64,
    hi: f64,
    sign: f64,
}

impl Piece<'_> {
    fn point(&self, t: f64) -> Point2 {
        match self.path {
            Path::Curve(c) => c.point(t),
            Path::Segment { a, d } => a + d * t,
        }
    }

    /// The exact value of coordinate `k` (`0` for `u`, `1` for `v`) when the path holds it
    /// constant: a line or join along the other coordinate, an angle line at rate zero.
    fn level(&self, k: usize) -> Option<f64> {
        let pick = |p: Point2, d: Vec2| {
            let (o, r) = if k == 0 { (p.x, d.x) } else { (p.y, d.y) };
            (r == 0.0).then_some(o)
        };
        match self.path {
            Path::Segment { a, d } => pick(a, d),
            Path::Curve(Curve2::Line(l)) => pick(l.origin(), l.direction()),
            Path::Curve(Curve2::AngleLine(a)) => {
                let other = match a.axis() {
                    AngleAxis::U => 1,
                    AngleAxis::V => 0,
                };
                (k == other && a.rate() == 0.0).then_some(a.other())
            }
            Path::Curve(_) => None,
        }
    }

    /// `[u, v]` Taylor coefficients at `t` (a point or a box) up to degree `n`; a
    /// coordinate the path holds constant is exact (rounding would blur it across a line
    /// it lies on).
    fn coeffs(&self, t: Ivl, n: usize) -> Vec<[Ivl; 2]> {
        let mut out = match self.path {
            Path::Curve(c) => patch::coeffs2(c, t, n),
            Path::Segment { a, d } => {
                let at = |o: f64, s: f64| Ivl::from(o) + t * Ivl::from(s);
                let mut out = vec![[at(a.x, d.x), at(a.y, d.y)]];
                if n >= 1 {
                    out.push([Ivl::from(d.x), Ivl::from(d.y)]);
                }
                out
            }
        };
        for k in 0..2 {
            if let Some(c) = self.level(k) {
                for (i, x) in out.iter_mut().enumerate() {
                    if let Some(slot) = x.get_mut(k) {
                        *slot = Ivl::from(if i == 0 { c } else { 0.0 });
                    }
                }
            }
        }
        out
    }
}

/// A use's strip: its pcurve against the edge's curve (or the pole's point).
struct Strip<'a> {
    pcurve: &'a Curve2,
    edge: Target<'a>,
    lo: f64,
    hi: f64,
    /// `max |z - z0|` over the boxes of the surface over the pcurve's and of the edge.
    reach: f64,
}

/// What a use's strip closes onto.
#[derive(Clone, Copy)]
enum Target<'a> {
    Curve(&'a Curve),
    Point(Point3),
}

/// A face as the enclosure reads it.
struct FaceData<'a> {
    face: Face<'a>,
    weight: f64,
    base: f64,
    pieces: Vec<Piece<'a>>,
    strips: Vec<Strip<'a>>,
}

/// A span end in `v`: a constant or the piece's `v(t)`.
#[derive(Clone, Copy, Debug)]
enum End {
    Const(f64),
    Curve,
}

/// What a cell covers.
#[derive(Clone, Copy, Debug)]
enum Kind {
    /// A Taylor cell: `s` over `[s0, s1]` of the span `[a, b]`.
    Span {
        face: usize,
        piece: usize,
        a: End,
        b: End,
        s0: f64,
        s1: f64,
        order: usize,
    },
    /// A strip's piece.
    Strip { face: usize, strip: usize },
}

/// A cell and its enclosure.
#[derive(Clone, Copy, Debug)]
struct Cell {
    kind: Kind,
    t0: f64,
    t1: f64,
    depth: u32,
    value: Ivl,
    /// The remainder's share of the width.
    rem: f64,
    /// Whether the remainder asks for a split along `t` (else `s`).
    split_t: bool,
}

/// A heap entry: the widest first, then the oldest.
#[derive(Clone, Copy)]
struct Entry {
    width: f64,
    seq: u64,
    cell: usize,
}

impl PartialEq for Entry {
    fn eq(&self, o: &Self) -> bool {
        self.cmp(o) == Ordering::Equal
    }
}
impl Eq for Entry {}
impl PartialOrd for Entry {
    fn partial_cmp(&self, o: &Self) -> Option<Ordering> {
        Some(self.cmp(o))
    }
}
impl Ord for Entry {
    fn cmp(&self, o: &Self) -> Ordering {
        self.width
            .total_cmp(&o.width)
            .then_with(|| o.seq.cmp(&self.seq))
    }
}

/// `x` rounded up past its rounding.
fn up(x: f64) -> f64 {
    Ivl::checked(x, x).hi
}

/// The magnitude bound of `x` as an interval `[0, |x|]`'s top.
fn mag(x: Ivl) -> Ivl {
    Ivl::from(x.mag())
}

/// `x` less the whole multiple of `period` nearest it.
fn reduce(x: f64, period: Option<f64>) -> f64 {
    match period {
        Some(p) => x - (x / p).round() * p,
        None => x,
    }
}

/// The weighted faces of a volume and their common `z0`.
struct Set<'a> {
    faces: Vec<FaceData<'a>>,
    z0: f64,
    /// The caps' bound.
    caps: Ivl,
}

/// The faces of `shell` with their weights: `+1` for a back side, `-1` for a front side, a
/// face with both sides in the shell (a sheet) cancelling.
fn shell_weights(body: &Brep, shell: ShellId) -> Result<Vec<(Face<'_>, f64)>, MassError> {
    let s = body
        .shell(shell)
        .ok_or(MassError::Refused(MassRefusal::NoShell { shell }))?;
    let mut out: BTreeMap<FaceId, (Face<'_>, f64)> = BTreeMap::new();
    for (f, side) in s.faces() {
        let w = match side {
            FaceSide::Back => 1.0,
            FaceSide::Front => -1.0,
        };
        out.entry(f.id()).or_insert((f, 0.0)).1 += w;
    }
    Ok(out.into_values().filter(|&(_, w)| w != 0.0).collect())
}

/// The body's faces weighted as the volume of its solid counts them: `+1` with the solid
/// behind, `-1` in front, `0` between two solids or two voids.
fn body_weights(body: &Brep) -> Vec<(Face<'_>, f64)> {
    body.faces()
        .filter_map(|f| {
            let solid = |side| f.shell(side).region().is_solid();
            let w = f64::from(i8::from(solid(FaceSide::Back)) - i8::from(solid(FaceSide::Front)));
            (w != 0.0).then_some((f, w))
        })
        .collect()
}

/// Refuses a weighted face set whose edges' uses do not balance: its boundary is open.
fn balanced(faces: &[(Face<'_>, f64)]) -> Result<(), MassError> {
    let mut sum: BTreeMap<scx_brep::EdgeId, i64> = BTreeMap::new();
    for &(f, w) in faces {
        for lp in f.loops() {
            for u in lp.uses() {
                let e = u.edge();
                if matches!(e.shape(), EdgeShape::Collapsed) {
                    continue;
                }
                let s: i64 = match u.sense() {
                    Sense::Forward => 1,
                    Sense::Reversed => -1,
                };
                *sum.entry(e.id()).or_insert(0) += s * w as i64;
            }
        }
    }
    match sum.into_iter().find(|&(_, s)| s != 0) {
        Some((edge, _)) => Err(MassError::Refused(MassRefusal::OpenBoundary { edge })),
        None => Ok(()),
    }
}

fn no_box(at: Entity) -> MassError {
    MassError::Unsupported(MassUnsupported::NoBox { at })
}

/// The box of the weighted faces' edges: it places `z0`, which any finite value would
/// serve (a closed boundary's flux does not depend on it). The edges' and not the faces'
/// boxes, so the enclosure reads the boundary alone: a face whose region's box the body
/// cannot prove (a thin planar annulus, its loops' boxes overlapping) still encloses.
fn set_box(faces: &[(Face<'_>, f64)]) -> Result<Option<Extent3d>, MassError> {
    let mut b: Option<Extent3d> = None;
    for &(f, _) in faces {
        for lp in f.loops() {
            for u in lp.uses() {
                let e = u.edge();
                let eb = e
                    .bounding_box()
                    .ok_or_else(|| no_box(Entity::from(e.id())))?;
                b = Some(b.map_or(eb, |x| x.union(eb)));
            }
        }
    }
    Ok(b)
}

/// The most halvings of a pcurve span whose enclosure is not finite: up to 64 pieces.
const SPLITS: u32 = 6;

/// A box holding `surface` over the uv image of `curve` on `[lo, hi]`: the whole span's
/// enclosure, or the hull of its halves' where that is not finite (a pcurve seen through a
/// chart's inverse, near the chart's pole, encloses only in pieces), to `depth` halvings.
fn strip_box(surface: &Surface, curve: &Curve2, lo: f64, hi: f64, depth: u32) -> Option<Extent3d> {
    let trim = curve.point_over(Ivl::new(lo, hi));
    let whole = Extent2d::from_bounds(trim.x.lo, trim.y.lo, trim.x.hi, trim.y.hi)
        .and_then(|uv| surface.box_over(uv));
    if whole.is_some() {
        return whole;
    }
    let mid = 0.5 * (lo + hi);
    if depth == 0 || !(lo < mid && mid < hi) {
        return None;
    }
    let a = strip_box(surface, curve, lo, mid, depth - 1)?;
    let b = strip_box(surface, curve, mid, hi, depth - 1)?;
    Some(a.union(b))
}

/// `max |z - z0|` over a box.
fn reach(b: &Extent3d, z0: f64) -> f64 {
    up((b.max().z - z0).abs().max((b.min().z - z0).abs()))
}

/// The surface's point on the patch holding `near`, at series `u` and `v`.
fn eval(face: Face<'_>, u: &Ser, v: &Ser, near: Point2) -> Result<P3, MassError> {
    patch::eval(face.surface(), u, v, near).map_err(|e| match e {
        PatchFail::Kind => MassError::Unsupported(MassUnsupported::NoSeries { face: face.id() }),
        PatchFail::Table => MassError::Defect(MassDefect::IntegrandNotFinite {
            at: Entity::from(face.id()),
        }),
    })
}

/// The lines across which `s` may not be smooth, strictly inside `(lo, hi)`: an analytic
/// kind's arc joins (its uv is the rational quarter arcs', C1 at a join: each arc's formula
/// continued past its end is the circle at other angles) and a NURBS surface's knots.
fn rough(s: &Surface, dir: Dir, lo: f64, hi: f64) -> Vec<f64> {
    surface_breaks(s, dir, lo, hi)
}

/// Whether `s` may be discontinuous across a line in `x` (its ends included): a NURBS
/// knot of full multiplicity inside the knot vector.
fn torn(s: &Surface, dir: Dir, x: Ivl) -> bool {
    let Surface::Nurbs(n) = s else {
        return false;
    };
    let kv = n.knot_vector(match dir {
        Dir::U => ParamAxis::U,
        Dir::V => ParamAxis::V,
    });
    let knots = kv.knots();
    let (Some(&first), Some(&last)) = (knots.first(), knots.last()) else {
        return false;
    };
    knots.chunk_by(|a, b| a == b).any(|run| {
        run.len() > kv.degree()
            && run
                .first()
                .is_some_and(|&k| first < k && k < last && x.lo <= k && k <= x.hi)
    })
}

/// The near point of every patch a box `[u] x [v]` meets; `None` when it meets more than
/// [`MAX_PATCHES`] or is not finite.
fn patches(s: &Surface, u: Ivl, v: Ivl) -> Option<Vec<Point2>> {
    if !(u.is_finite() && v.is_finite()) {
        return None;
    }
    let mids = |dir, x: Ivl| {
        let mut ends = vec![x.lo];
        ends.extend(rough(s, dir, x.lo, x.hi));
        ends.push(x.hi);
        ends.windows(2)
            .filter_map(|w| match *w {
                [a, b] => Some(a + (b - a) / 2.0),
                _ => None,
            })
            .collect::<Vec<f64>>()
    };
    let (us, vs) = (mids(Dir::U, u), mids(Dir::V, v));
    if us.len() * vs.len() > MAX_PATCHES {
        return None;
    }
    Some(
        us.iter()
            .flat_map(|&a| vs.iter().map(move |&b| Point2::new(a, b)))
            .collect(),
    )
}

/// Whether a line across which `s` may not be smooth lies strictly inside `x`.
fn crosses(s: &Surface, dir: Dir, x: Ivl) -> bool {
    !x.is_finite() || !rough(s, dir, x.lo, x.hi).is_empty()
}

/// A face's loops as pieces, joined straight where consecutive pcurves do not meet (the
/// estimate's walk, `face::pieces`); its strips; and its caps' perimeter.
fn walk(face: Face<'_>, z0: f64) -> Result<(Vec<Piece<'_>>, Vec<Strip<'_>>, Ivl), MassError> {
    let id = face.id();
    let surface = face.surface();
    let domain = surface.domain();
    let periods = (domain.u.period(), domain.v.period());
    let mut pieces = Vec::new();
    let mut strips = Vec::new();
    let mut perimeter = Ivl::from(0.0);
    let mut net_u = 0.0;
    for lp in face.loops() {
        let mut uses = Vec::new();
        for u in lp.uses() {
            let UseGeom::Pcurve { curve, interval } = u.geom() else {
                continue;
            };
            let edge = u.edge();
            let ebox = edge
                .bounding_box()
                .ok_or_else(|| no_box(Entity::from(edge.id())))?;
            let target = match edge.shape() {
                EdgeShape::Curve { curve: c, .. } => Target::Curve(c),
                EdgeShape::Collapsed => Target::Point(u.start().point()),
            };
            let sign = match u.sense() {
                Sense::Forward => 1.0,
                Sense::Reversed => -1.0,
            };
            let (lo, hi) = (interval.min(), interval.max());
            let sbox = strip_box(surface, curve, lo, hi, SPLITS)
                .ok_or_else(|| no_box(Entity::from(id)))?;
            strips.push(Strip {
                pcurve: curve,
                edge: target,
                lo,
                hi,
                reach: reach(&sbox.union(ebox), z0),
            });
            let (t_in, t_out) = if sign < 0.0 { (hi, lo) } else { (lo, hi) };
            uses.push((
                Piece {
                    path: Path::Curve(curve),
                    lo,
                    hi,
                    sign,
                },
                target,
                t_in,
                t_out,
                u.start().point(),
                u.end().point(),
            ));
        }
        let (mut wind_u, mut wind_v) = (0.0, 0.0);
        for (i, a) in uses.iter().enumerate() {
            let b = uses.get((i + 1) % uses.len()).unwrap_or(a);
            let (s, e) = (a.0.point(a.2), a.0.point(a.3));
            let to = b.0.point(b.2);
            let gap = to - e;
            let d = Vec2::new(reduce(gap.x, periods.0), reduce(gap.y, periods.1));
            wind_u += (e.x - s.x) + d.x;
            wind_v += (e.y - s.y) + d.y;
            pieces.push(a.0);
            if d.x != 0.0 {
                pieces.push(Piece {
                    path: Path::Segment { a: e, d },
                    lo: 0.0,
                    hi: 1.0,
                    sign: 1.0,
                });
            }
            // The corner's cap: from S at a's end along the join to S at b's start, to the
            // edges' ends and the vertex between them.
            perimeter = perimeter
                + gap_at(face, &a.0, a.1, a.3, a.5)?
                + gap_at(face, &b.0, b.1, b.2, b.4)?
                + join_length(face, e, d)?;
        }
        if let Some(pv) = periods.1
            && wind_v.abs() > 0.5 * pv
        {
            return Err(MassError::Unsupported(MassUnsupported::VWrap { face: id }));
        }
        net_u += wind_u;
    }
    if let Some(pu) = periods.0
        && net_u.abs() > 0.5 * pu
    {
        return Err(MassError::Unsupported(MassUnsupported::NetWinding {
            face: id,
        }));
    }
    Ok((pieces, strips, perimeter))
}

/// `|S(pcurve(t)) - C(t)| + |C(t) - vertex|` at a use's end.
fn gap_at(
    face: Face<'_>,
    p: &Piece<'_>,
    target: Target<'_>,
    t: f64,
    vertex: Point3,
) -> Result<Ivl, MassError> {
    let q = p.coeffs(Ivl::from(t), 0);
    let Some(&[u, v]) = q.first() else {
        return Ok(Ivl::whole());
    };
    let near = Point2::new(u.mid(), v.mid());
    let s = eval(face, &Ser::constant(u, 0), &Ser::constant(v, 0), near)?;
    let c = target_at(target, Ivl::from(t), 0);
    let first = |x: &Vec<[Ivl; 3]>| x.first().copied().unwrap_or([Ivl::whole(); 3]);
    let c0 = first(&c);
    let d = [0, 1, 2].map(|k| {
        s.get(k).map_or(Ivl::whole(), |x| x.at(0, 0)) - c0.get(k).copied().unwrap_or(Ivl::whole())
    });
    let w = [vertex.x, vertex.y, vertex.z];
    let e = [0, 1, 2].map(|k| {
        c0.get(k).copied().unwrap_or(Ivl::whole()) - Ivl::from(w.get(k).copied().unwrap_or(0.0))
    });
    Ok(norm(d) + norm(e))
}

/// A target's Taylor coefficients (a point's: itself, then zeros).
fn target_at(t: Target<'_>, at: Ivl, n: usize) -> Vec<[Ivl; 3]> {
    match t {
        Target::Curve(c) => patch::coeffs3(c, at, n),
        Target::Point(p) => {
            let mut out = vec![[Ivl::from(p.x), Ivl::from(p.y), Ivl::from(p.z)]];
            out.resize(n + 1, [Ivl::from(0.0); 3]);
            out
        }
    }
}

/// An upper bound of `|x|` as an interval.
fn norm(x: [Ivl; 3]) -> Ivl {
    let s = x.iter().fold(Ivl::from(0.0), |a, &c| {
        let m = mag(c);
        a + m * m
    });
    Ivl::from(s.sqrt().hi)
}

/// A bound on the length of the image of the uv segment from `e` by `d`.
fn join_length(face: Face<'_>, e: Point2, d: Vec2) -> Result<Ivl, MassError> {
    if d.x == 0.0 && d.y == 0.0 {
        return Ok(Ivl::from(0.0));
    }
    let u = Ivl::from(e.x).hull(Ivl::from(e.x) + Ivl::from(d.x));
    let v = Ivl::from(e.y).hull(Ivl::from(e.y) + Ivl::from(d.y));
    let Some(near) = patches(face.surface(), u, v) else {
        return Ok(Ivl::whole());
    };
    let (mut su, mut sv) = (Ivl::from(0.0), Ivl::from(0.0));
    for n in near {
        let p = eval(face, &Ser::var_t(u, 1), &Ser::var_s(v, 1), n)?;
        su = su.hull(norm(p.map(|c| c.at(1, 0))));
        sv = sv.hull(norm(p.map(|c| c.at(0, 1))));
    }
    Ok(mag(su) * mag(Ivl::from(d.x)) + mag(sv) * mag(Ivl::from(d.y)))
}

/// `∫ dt^i` over `[dl, dh]`, and `∫ |dt|^i`, for `i <= n`.
fn moments(dl: Ivl, dh: Ivl, n: usize) -> (Vec<Ivl>, Vec<Ivl>) {
    let (al, ah) = (mag(dl), mag(dh));
    let (mut pl, mut ph, mut ql, mut qh) = (dl, dh, al, ah);
    let mut plain = Vec::with_capacity(n + 1);
    let mut abs = Vec::with_capacity(n + 1);
    for i in 0..=n {
        let k = Ivl::from((i + 1) as f64);
        plain.push((ph - pl) / k);
        abs.push((qh + ql) / k);
        pl = pl * dl;
        ph = ph * dh;
        ql = ql * al;
        qh = qh * ah;
    }
    (plain, abs)
}

/// The work a cell evaluation at order `n` is charged: its series' coefficients.
fn work(n: usize) -> u64 {
    (len(n) + len(n + 1)) as u64
}

struct Engine<'a, 'm> {
    set: Set<'a>,
    meter: Meter<'m>,
    cells: Vec<Cell>,
    heap: BinaryHeap<Entry>,
    seq: u64,
    lo: f64,
    hi: f64,
    /// The cells whose enclosure is not finite, left out of `lo` and `hi`.
    open: usize,
}

impl<'a> Engine<'a, '_> {
    fn charge(&mut self, face: usize, n: usize) -> Result<(), MassError> {
        let at = self.set.faces.get(face).map_or_else(
            || Entity::from(RegionId::OUTSIDE),
            |f| Entity::from(f.face.id()),
        );
        self.meter.charge(work(n)).map_err(|f| fail(at, f))
    }

    fn face(&self, i: usize) -> Option<&FaceData<'a>> {
        self.set.faces.get(i)
    }

    /// The `u` and `v` ranges of a piece over `[t0, t1]`, and whether its `u` is constant.
    fn ranges(p: &Piece<'_>, t0: f64, t1: f64) -> (Ivl, Ivl, bool) {
        let c = p.coeffs(Ivl::new(t0, t1), 1);
        let [u, v] = c.first().copied().unwrap_or([Ivl::whole(); 2]);
        let du = c.get(1).map_or(Ivl::whole(), |x| x[0]);
        (u, v, du.lo == 0.0 && du.hi == 0.0)
    }

    /// The cells of a piece's column over `[t0, t1]`: the spans from the base to the
    /// piece, split at the lines across which the surface may not be smooth.
    fn column(
        &mut self,
        face: usize,
        piece: usize,
        t0: f64,
        t1: f64,
        depth: u32,
    ) -> Result<(), MassError> {
        let Some(f) = self.face(face) else {
            return Ok(());
        };
        let Some(p) = f.pieces.get(piece).copied() else {
            return Ok(());
        };
        let s = f.face.surface();
        let base = f.base;
        let (_, v, flat) = Self::ranges(&p, t0, t1);
        if flat {
            return Ok(());
        }
        // The lines between the base and the piece, from the base toward it: in
        // `(base, v.lo]` above it, `[v.hi, base)` below; a piece straddling a line has its
        // last span straddle it too, enclosed by its range until split clear.
        let ends: Vec<f64> = if v.lo >= base {
            rough(s, Dir::V, base, f64::next_up(v.lo))
        } else if v.hi <= base {
            let mut b = rough(s, Dir::V, f64::next_down(v.hi), base);
            b.reverse();
            b
        } else {
            Vec::new()
        };
        let mut a = End::Const(base);
        for b in ends.into_iter().map(End::Const).chain([End::Curve]) {
            self.add(
                Kind::Span {
                    face,
                    piece,
                    a,
                    b,
                    s0: 0.0,
                    s1: 1.0,
                    order: FIRST_ORDER,
                },
                t0,
                t1,
                depth,
            )?;
            a = b;
        }
        Ok(())
    }

    /// Evaluates and adds a cell.
    fn add(&mut self, kind: Kind, t0: f64, t1: f64, depth: u32) -> Result<(), MassError> {
        let mut cell = Cell {
            kind,
            t0,
            t1,
            depth,
            value: Ivl::whole(),
            rem: 0.0,
            split_t: true,
        };
        self.evaluate(&mut cell)?;
        let i = self.cells.len();
        self.cells.push(cell);
        self.tally(cell.value, true);
        self.push(i);
        Ok(())
    }

    /// Adds a cell's enclosure to the running sums (`add`), or takes it out.
    fn tally(&mut self, v: Ivl, add: bool) {
        if v.is_finite() {
            let k = if add { 1.0 } else { -1.0 };
            self.lo += k * v.lo;
            self.hi += k * v.hi;
        } else if add {
            self.open += 1;
        } else {
            self.open = self.open.saturating_sub(1);
        }
    }

    fn push(&mut self, i: usize) {
        if let Some(c) = self.cells.get(i) {
            self.seq += 1;
            self.heap.push(Entry {
                width: c.value.hi - c.value.lo,
                seq: self.seq,
                cell: i,
            });
        }
    }

    /// Replaces cell `i` by `c`, keeping the running sums.
    fn replace(&mut self, i: usize, c: Cell) {
        if let Some(slot) = self.cells.get_mut(i) {
            let old = slot.value;
            *slot = c;
            self.tally(old, false);
            self.tally(c.value, true);
        }
        self.push(i);
    }

    fn evaluate(&mut self, cell: &mut Cell) -> Result<(), MassError> {
        match cell.kind {
            Kind::Span {
                face,
                piece,
                a,
                b,
                s0,
                s1,
                order,
            } => self.span(cell, face, piece, (a, b), (s0, s1), order),
            Kind::Strip { face, strip } => self.strip(cell, face, strip),
        }
    }

    /// The span's `v(t, s) = a + s (b - a)` in series.
    fn span_v(ends: (End, End), vt: &Ser, s: &Ser) -> Ser {
        let at = |e: End| match e {
            End::Const(c) => Ser::lift(c),
            End::Curve => *vt,
        };
        let (a, b) = (at(ends.0), at(ends.1));
        a + *s * (b - a)
    }

    /// The integrand `-(Φ_z - z0)(Φ_t x Φ_s)_z` of `Φ`.
    fn integrand(phi: &P3, z0: f64) -> Ser {
        let [x, y, z] = phi;
        let (xt, xs, yt, ys) = (x.deriv_t(), x.deriv_s(), y.deriv_t(), y.deriv_s());
        let jac = xt * ys - yt * xs;
        (Ser::lift(z0) - *z) * jac
    }

    fn span(
        &mut self,
        cell: &mut Cell,
        face: usize,
        piece: usize,
        ends: (End, End),
        (s0, s1): (f64, f64),
        n: usize,
    ) -> Result<(), MassError> {
        self.charge(face, n)?;
        let Some(f) = self.face(face) else {
            return Ok(());
        };
        let Some(p) = f.pieces.get(piece).copied() else {
            return Ok(());
        };
        let (fc, w, z0) = (f.face, f.weight, self.set.z0);
        let (t0, t1) = (cell.t0, cell.t1);
        let (tm, sm) = (t0 + (t1 - t0) / 2.0, s0 + (s1 - s0) / 2.0);
        let (u, v, _) = Self::ranges(&p, t0, t1);
        let at = |e: End| match e {
            End::Const(c) => Ivl::from(c),
            End::Curve => v,
        };
        let (va, vb) = (at(ends.0), at(ends.1));
        let vr = va + Ivl::new(s0, s1) * (vb - va);
        let surface = fc.surface();
        let (on_u, on_v) = (crosses(surface, Dir::U, u), crosses(surface, Dir::V, vr));
        if on_u || on_v {
            // Over a line where the surface may not be smooth: the integrand's range,
            // hulled over each patch the cell meets, times the cell's area.
            // Along `t` when `u` is on the line, or when `v` is and its range over the cell
            // comes more from the curve's `v` moving along `t` than from `s`: splitting `s`
            // alone never takes a cell clear of a line the trim itself crosses.
            let dv = vb - va;
            let from_t = s0.abs().max(s1.abs()) * (dv.hi - dv.lo);
            let from_s = (s1 - s0) * dv.mag();
            cell.rem = 0.0;
            cell.split_t = on_u || (on_v && from_t >= from_s);
            let Some(near) = patches(surface, u, vr) else {
                cell.value = Ivl::whole();
                return Ok(());
            };
            let c = p.coeffs(Ivl::new(t0, t1), 1);
            let cu: Vec<Ivl> = c.iter().map(|x| x[0]).collect();
            let cv: Vec<Ivl> = c.iter().map(|x| x[1]).collect();
            let (ut, vt) = (Ser::in_t(&cu, 1), Ser::in_t(&cv, 1));
            let vv = Self::span_v(ends, &vt, &Ser::var_s(Ivl::new(s0, s1), 1));
            let mut range: Option<Ivl> = None;
            for q in &near {
                let phi = eval(fc, &ut, &vv, *q)?;
                let r = Self::integrand(&phi, z0).at(0, 0);
                range = Some(range.map_or(r, |x| x.hull(r)));
            }
            let area = (Ivl::from(t1) - Ivl::from(t0)) * (Ivl::from(s1) - Ivl::from(s0));
            let value = range.unwrap_or(Ivl::whole()) * area * Ivl::from(p.sign * w);
            cell.value = if value.is_finite() {
                value
            } else {
                Ivl::whole()
            };
            return Ok(());
        }
        let near = Point2::new(u.mid(), vr.mid());
        // The Taylor part at the middle, degree n - 1.
        let series = |t: Ivl, s: Ivl, n: usize| -> Result<Ser, MassError> {
            let c = p.coeffs(t, n);
            let cu: Vec<Ivl> = c.iter().map(|x| x[0]).collect();
            let cv: Vec<Ivl> = c.iter().map(|x| x[1]).collect();
            let (ut, vt) = (Ser::in_t(&cu, n), Ser::in_t(&cv, n));
            let sv = Ser::var_s(s, n);
            let phi = eval(fc, &ut, &Self::span_v(ends, &vt, &sv), near)?;
            Ok(Self::integrand(&phi, z0))
        };
        let mid = series(Ivl::from(tm), Ivl::from(sm), n)?;
        let over = series(Ivl::new(t0, t1), Ivl::new(s0, s1), n + 1)?;
        let (it, at) = moments(
            Ivl::from(t0) - Ivl::from(tm),
            Ivl::from(t1) - Ivl::from(tm),
            n,
        );
        let (is, as_) = moments(
            Ivl::from(s0) - Ivl::from(sm),
            Ivl::from(s1) - Ivl::from(sm),
            n,
        );
        let get = |v: &Vec<Ivl>, k: usize| v.get(k).copied().unwrap_or(Ivl::whole());
        let mut taylor = Ivl::from(0.0);
        for d in 0..n {
            for j in 0..=d {
                let i = d - j;
                let c = mid.at(i, j);
                if c.lo == 0.0 && c.hi == 0.0 {
                    continue;
                }
                taylor = taylor + c * get(&it, i) * get(&is, j);
            }
        }
        let mut rem = Ivl::from(0.0);
        let (mut wt, mut ws) = (0.0, 0.0);
        for j in 0..=n {
            let i = n - j;
            let c = over.at(i, j);
            if c.lo == 0.0 && c.hi == 0.0 {
                continue;
            }
            let m = c.mid();
            let r = up((c.hi - m).max(m - c.lo));
            let spread = Ivl::from(r) * get(&at, i) * get(&as_, j);
            let term = Ivl::from(m) * get(&it, i) * get(&is, j) + Ivl::new(-spread.hi, spread.hi);
            let e = spread.hi;
            wt += i as f64 * e;
            ws += j as f64 * e;
            rem = rem + term;
        }
        let value = (taylor + rem) * Ivl::from(p.sign * w);
        cell.value = if value.is_finite() {
            value
        } else {
            Ivl::whole()
        };
        cell.rem = rem.hi - rem.lo;
        cell.split_t = wt >= ws;
        Ok(())
    }

    /// A strip cell: its flux is at most `max |z - z0|` times its area, at most
    /// `int |D| (|C'| + |D'|) dt` with `D = S(pcurve) - C`.
    fn strip(&mut self, cell: &mut Cell, face: usize, strip: usize) -> Result<(), MassError> {
        self.charge(face, STRIP_ORDER)?;
        let Some(f) = self.face(face) else {
            return Ok(());
        };
        let Some(st) = f.strips.get(strip) else {
            return Ok(());
        };
        let (fc, w) = (f.face, f.weight);
        let (pc, target, reach) = (st.pcurve, st.edge, st.reach);
        let (t0, t1) = (cell.t0, cell.t1);
        let tm = t0 + (t1 - t0) / 2.0;
        let n = STRIP_ORDER;
        cell.rem = 0.0;
        cell.split_t = true;
        let boxed = Ivl::new(t0, t1);
        let path = Piece {
            path: Path::Curve(pc),
            lo: t0,
            hi: t1,
            sign: 1.0,
        };
        let c = path.coeffs(boxed, 1);
        let [u, v] = c.first().copied().unwrap_or([Ivl::whole(); 2]);
        let s = fc.surface();
        // D(t) = S(pcurve(t)) - C(t) in series.
        let diff = |t: Ivl, n: usize, near: Point2| -> Result<[Ser; 3], MassError> {
            let c = path.coeffs(t, n);
            let cu: Vec<Ivl> = c.iter().map(|x| x[0]).collect();
            let cv: Vec<Ivl> = c.iter().map(|x| x[1]).collect();
            let phi = eval(fc, &Ser::in_t(&cu, n), &Ser::in_t(&cv, n), near)?;
            let e = target_at(target, t, n);
            let mut out = phi;
            for (k, o) in out.iter_mut().enumerate() {
                let ck: Vec<Ivl> = e
                    .iter()
                    .map(|x| x.get(k).copied().unwrap_or(Ivl::whole()))
                    .collect();
                *o = *o - Ser::in_t(&ck, n);
            }
            Ok(out)
        };
        let coeff = |d: &[Ser; 3], k: usize| norm(d.map(|x| x.at(k, 0)));
        let (dl, dh) = (Ivl::from(t0) - Ivl::from(tm), Ivl::from(t1) - Ivl::from(tm));
        let (_, abs) = moments(dl, dh, n);
        let get = |k: usize| abs.get(k).copied().unwrap_or(Ivl::whole());
        let h = Ivl::from(mag(dl).hi.max(mag(dh).hi));
        // The Taylor bound on one patch: `int |D|` over the cell, and `max |D'|`.
        let taylor = |near: Point2| -> Result<(Ivl, Ivl), MassError> {
            let mid = diff(Ivl::from(tm), n, near)?;
            let over = diff(boxed, n, near)?;
            let (mut g, mut sl, mut hp) = (Ivl::from(0.0), Ivl::from(0.0), Ivl::from(1.0));
            for k in 0..n {
                let m = coeff(&mid, k);
                g = g + m * get(k);
                if k >= 1 {
                    sl = sl + m * Ivl::from(k as f64) * hp;
                    hp = hp * h;
                }
            }
            let last = coeff(&over, n);
            Ok((g + last * get(n), sl + last * Ivl::from(n as f64) * hp))
        };
        let (cu, cv) = (crosses(s, Dir::U, u), crosses(s, Dir::V, v));
        let (gap, slope) = if cu || cv {
            let Some(near) = patches(s, u, v) else {
                cell.value = Ivl::whole();
                return Ok(());
            };
            let (mut d0, mut d1) = (Ivl::from(0.0), Ivl::from(0.0));
            for q in near {
                let d = diff(boxed, 1, q)?;
                d0 = d0.hull(coeff(&d, 0));
                d1 = d1.hull(coeff(&d, 1));
            }
            if torn(s, Dir::U, u) || torn(s, Dir::V, v) {
                (d0 * get(0), d1)
            } else {
                // D is continuous on the cell, so |D(t)| <= |D(tm)| + |D'| |t - tm|.
                let at = path.coeffs(Ivl::from(tm), 0);
                let [um, vm] = at.first().copied().unwrap_or([Ivl::whole(); 2]);
                let dm = coeff(&diff(Ivl::from(tm), 0, Point2::new(um.mid(), vm.mid()))?, 0);
                (dm * get(0) + d1 * get(1), d1)
            }
        } else {
            taylor(Point2::new(u.mid(), v.mid()))?
        };
        let cp = match target {
            Target::Curve(c) => {
                let j = patch::coeffs3(c, boxed, 1);
                norm(j.get(1).copied().unwrap_or([Ivl::whole(); 3]))
            }
            Target::Point(_) => Ivl::from(0.0),
        };
        let width = Ivl::from(reach) * gap * (cp + slope) * Ivl::from(w.abs());
        let b = width.hi;
        cell.value = if b.is_finite() {
            Ivl::new(-b, b)
        } else {
            Ivl::whole()
        };
        Ok(())
    }

    /// Refines cell `i`: raise its order, or split it; `false` when it can do neither.
    fn refine(&mut self, i: usize) -> Result<bool, MassError> {
        let Some(c) = self.cells.get(i).copied() else {
            return Ok(false);
        };
        if let Kind::Span { order, .. } = c.kind
            && order < ORDER
            && c.rem > 0.5 * (c.value.hi - c.value.lo)
        {
            let mut d = c;
            if let Kind::Span { order: o, .. } = &mut d.kind {
                *o = ORDER;
            }
            self.evaluate(&mut d)?;
            self.replace(i, d);
            return Ok(true);
        }
        if c.depth >= MAX_DEPTH {
            return Ok(false);
        }
        let along_s = matches!(c.kind, Kind::Span { .. }) && !c.split_t;
        if along_s {
            let Kind::Span {
                face,
                piece,
                a,
                b,
                s0,
                s1,
                order,
            } = c.kind
            else {
                return Ok(false);
            };
            let sm = s0 + (s1 - s0) / 2.0;
            if !(s0 < sm && sm < s1) {
                return Ok(false);
            }
            let half = |lo, hi| Cell {
                kind: Kind::Span {
                    face,
                    piece,
                    a,
                    b,
                    s0: lo,
                    s1: hi,
                    order,
                },
                depth: c.depth + 1,
                ..c
            };
            let (mut x, mut y) = (half(s0, sm), half(sm, s1));
            self.evaluate(&mut x)?;
            self.evaluate(&mut y)?;
            self.replace(i, x);
            self.insert(y);
            return Ok(true);
        }
        let tm = c.t0 + (c.t1 - c.t0) / 2.0;
        if !(c.t0 < tm && tm < c.t1) {
            return Ok(false);
        }
        let (mut x, mut y) = (
            Cell {
                t1: tm,
                depth: c.depth + 1,
                ..c
            },
            Cell {
                t0: tm,
                depth: c.depth + 1,
                ..c
            },
        );
        self.evaluate(&mut x)?;
        self.evaluate(&mut y)?;
        self.replace(i, x);
        self.insert(y);
        Ok(true)
    }

    fn insert(&mut self, c: Cell) {
        let i = self.cells.len();
        self.cells.push(c);
        self.tally(c.value, true);
        self.push(i);
    }

    /// The rigorous sum: every cell and the caps, rounded outward.
    fn total(&self) -> Ivl {
        self.cells.iter().fold(self.set.caps, |a, c| a + c.value)
    }

    /// Refines until `done` holds of the enclosure, the cell cap, or the floor.
    fn run(&mut self, done: impl Fn(Ivl) -> bool) -> Result<(Ivl, Settle), MassError> {
        let mut best = self.total();
        let mut next_check = 0;
        loop {
            let quick = Ivl::new(self.lo + self.set.caps.lo, self.hi + self.set.caps.hi);
            if self.open == 0 && done(quick) && self.cells.len() >= next_check {
                best = intersect(best, self.total());
                if done(best) {
                    return Ok((best, Settle::Reached));
                }
                next_check = self.cells.len() + self.cells.len() / 8 + 1;
            }
            if self.cells.len() >= CELL_BUDGET {
                return Ok((intersect(best, self.total()), Settle::CellCap));
            }
            let Some(e) = self.heap.pop() else {
                best = intersect(best, self.total());
                let settle = if done(best) {
                    Settle::Reached
                } else {
                    Settle::Floor
                };
                return Ok((best, settle));
            };
            // A stale entry: the cell changed since it was pushed.
            let current = self
                .cells
                .get(e.cell)
                .is_some_and(|c| (c.value.hi - c.value.lo).total_cmp(&e.width) == Ordering::Equal);
            if !current {
                continue;
            }
            self.refine(e.cell)?;
        }
    }
}

/// The intersection of two enclosures of the same value (the second when they miss, which
/// only rounding past both could cause).
fn intersect(a: Ivl, b: Ivl) -> Ivl {
    let (lo, hi) = (a.lo.max(b.lo), a.hi.min(b.hi));
    if lo <= hi { Ivl::new(lo, hi) } else { b }
}

/// Sets up the enclosure of the weighted faces.
fn engine<'a, 'm>(
    faces: &[(Face<'a>, f64)],
    meter: Meter<'m>,
) -> Result<Engine<'a, 'm>, MassError> {
    balanced(faces)?;
    let z0 = match set_box(faces)? {
        Some(b) => reference(b.mid(), b.max_length()).z,
        None => 0.0,
    };
    let mut data = Vec::new();
    let mut perimeter = Ivl::from(0.0);
    let mut reach_all = 0.0_f64;
    for &(f, w) in faces {
        let (pieces, strips, per) = walk(f, z0)?;
        perimeter = perimeter + per * Ivl::from(w.abs());
        for s in &strips {
            reach_all = reach_all.max(s.reach);
        }
        // The base: the lowest sampled v.
        let mut base = f64::INFINITY;
        for p in &pieces {
            for k in 0..=SAMPLES {
                let t = p.lo + (p.hi - p.lo) * f64::from(k) / f64::from(SAMPLES);
                base = base.min(p.point(t).y);
            }
        }
        let base = if base.is_finite() { base } else { 0.0 };
        data.push(FaceData {
            face: f,
            weight: w,
            base,
            pieces,
            strips,
        });
    }
    let half = perimeter * Ivl::from(0.5);
    let caps = Ivl::from(reach_all) * half * half;
    let b = caps.hi;
    let caps = if b.is_finite() {
        Ivl::new(-b, b)
    } else {
        Ivl::whole()
    };
    let mut e = Engine {
        set: Set {
            faces: data,
            z0,
            caps,
        },
        meter,
        cells: Vec::new(),
        heap: BinaryHeap::new(),
        seq: 0,
        lo: 0.0,
        hi: 0.0,
        open: 0,
    };
    for fi in 0..e.set.faces.len() {
        let (columns, strips) = {
            let Some(f) = e.face(fi) else { continue };
            let s = f.face.surface();
            let mut columns = Vec::new();
            for (pi, p) in f.pieces.iter().enumerate() {
                for (a, b) in piece_columns(s, p) {
                    columns.push((pi, a, b));
                }
            }
            let strips: Vec<(usize, f64, f64)> = f
                .strips
                .iter()
                .enumerate()
                .flat_map(|(si, st)| {
                    let p = Piece {
                        path: Path::Curve(st.pcurve),
                        lo: st.lo,
                        hi: st.hi,
                        sign: 1.0,
                    };
                    piece_columns(s, &p)
                        .into_iter()
                        .map(move |(a, b)| (si, a, b))
                })
                .collect();
            (columns, strips)
        };
        for (pi, a, b) in columns {
            e.column(fi, pi, a, b, 0)?;
        }
        for (si, a, b) in strips {
            e.add(
                Kind::Strip {
                    face: fi,
                    strip: si,
                },
                a,
                b,
                0,
            )?;
        }
    }
    Ok(e)
}

/// The samples lying exactly on one of `lines` that a neighbour leaves (a sign change
/// through a sample, which [`crossings`] sees on neither side of it).
fn on_lines(samples: &[(f64, f64)], lines: &[f64]) -> Vec<f64> {
    let mut out = Vec::new();
    for (i, &(t, c)) in samples.iter().enumerate() {
        let off = |j: Option<usize>| j.and_then(|j| samples.get(j)).is_some_and(|&(_, d)| d != c);
        if lines.contains(&c) && (off(i.checked_sub(1)) || off(Some(i + 1))) {
            out.push(t);
        }
    }
    out
}

/// A piece's columns: split at its own breaks and where it crosses a break line.
fn piece_columns(s: &Surface, p: &Piece<'_>) -> Vec<(f64, f64)> {
    let pts: Vec<(f64, Point2)> = (0..=SAMPLES)
        .map(|i| {
            let t = p.lo + (p.hi - p.lo) * f64::from(i) / f64::from(SAMPLES);
            (t, p.point(t))
        })
        .collect();
    let (mut ulo, mut uhi, mut vlo, mut vhi) = (
        f64::INFINITY,
        f64::NEG_INFINITY,
        f64::INFINITY,
        f64::NEG_INFINITY,
    );
    for (_, q) in &pts {
        ulo = ulo.min(q.x);
        uhi = uhi.max(q.x);
        vlo = vlo.min(q.y);
        vhi = vhi.max(q.y);
    }
    let mut ends = vec![p.lo, p.hi];
    if let Path::Curve(c) = p.path {
        ends.extend(curve_breaks(c, p.lo, p.hi));
    }
    if ulo <= uhi && vlo <= vhi {
        let us: Vec<(f64, f64)> = pts.iter().map(|(t, q)| (*t, q.x)).collect();
        let vs: Vec<(f64, f64)> = pts.iter().map(|(t, q)| (*t, q.y)).collect();
        let (du, dv) = ((uhi - ulo).max(1.0), (vhi - vlo).max(1.0));
        let lu = surface_breaks(s, Dir::U, ulo - du, uhi + du);
        let lv = surface_breaks(s, Dir::V, vlo - dv, vhi + dv);
        ends.extend(crossings(&us, &lu, |t| p.point(t).x));
        ends.extend(crossings(&vs, &lv, |t| p.point(t).y));
        ends.extend(on_lines(&us, &lu));
        ends.extend(on_lines(&vs, &lv));
    }
    ends.retain(|t| p.lo <= *t && *t <= p.hi);
    ends.sort_by(f64::total_cmp);
    ends.dedup();
    ends.windows(2)
        .filter_map(|w| match *w {
            [a, b] if a < b => Some((a, b)),
            _ => None,
        })
        .collect()
}

/// Encloses the volume of the weighted faces to `width`.
fn enclose(
    faces: &[(Face<'_>, f64)],
    width: f64,
    budget: &mut Budget,
    stop: &Stop,
) -> Result<VolumeEnclosure, MassError> {
    let mut e = engine(faces, Meter::new(budget, stop))?;
    let (v, settle) = e.run(|x| x.hi - x.lo <= width)?;
    Ok(VolumeEnclosure {
        lo: v.lo,
        hi: v.hi,
        settle,
        cells: e.cells.len(),
    })
}

fn sign_of(
    faces: &[(Face<'_>, f64)],
    shell: ShellId,
    budget: &mut Budget,
    stop: &Stop,
) -> Result<Sign, MassError> {
    if faces.is_empty() {
        return Ok(Sign::Zero);
    }
    let mut e = engine(faces, Meter::new(budget, stop))?;
    let (v, _) = e.run(|x| x.lo > 0.0 || x.hi < 0.0)?;
    if v.lo > 0.0 {
        Ok(Sign::Positive)
    } else if v.hi < 0.0 {
        Ok(Sign::Negative)
    } else {
        Err(MassError::Unsupported(MassUnsupported::SignUndecided {
            shell,
        }))
    }
}

/// Checks a width: finite and at least zero.
fn checked_width(width: f64) -> Result<f64, MassError> {
    if width.is_finite() && width >= 0.0 {
        Ok(width)
    } else {
        Err(MassError::Refused(MassRefusal::BadWidth { value: width }))
    }
}

/// An enclosure of the volume of the body's solid, refined until it is at most `width`
/// wide (or the cell cap or the floor first: see [`VolumeEnclosure::settle`]).
///
/// # Errors
///
/// [`MassRefusal::BadWidth`] for a width that is not finite and at least zero;
/// [`MassRefusal::OpenBoundary`] when the edges' uses do not balance;
/// [`MassUnsupported::NoSeries`] for a face kind with no series form; a winding the trims
/// do not close, a face with no box; the budget and the stop flag.
pub fn enclose_volume(
    body: &Brep,
    width: f64,
    budget: &mut Budget,
    stop: &Stop,
) -> Result<VolumeEnclosure, MassError> {
    let width = checked_width(width)?;
    enclose(&body_weights(body), width, budget, stop)
}

/// An enclosure of the volume a shell bounds, its faces counted from the shell's side:
/// positive for a region's outer shell, negative for the outside's shell round a solid.
///
/// # Errors
///
/// As [`enclose_volume`], and [`MassRefusal::NoShell`] for a shell the body does not have.
pub fn enclose_shell(
    body: &Brep,
    shell: ShellId,
    width: f64,
    budget: &mut Budget,
    stop: &Stop,
) -> Result<VolumeEnclosure, MassError> {
    let width = checked_width(width)?;
    enclose(&shell_weights(body, shell)?, width, budget, stop)
}

/// The certified sign of the volume a shell bounds (as [`enclose_shell`] counts it): refined
/// until its enclosure excludes zero. A shell of sheets only is [`Sign::Zero`].
///
/// # Errors
///
/// [`MassUnsupported::SignUndecided`] when the enclosure still holds zero at the cell cap or
/// the floor (a gap between faces as large as the volume, or a volume below the rounding);
/// otherwise as [`enclose_shell`].
pub fn volume_sign(
    body: &Brep,
    shell: ShellId,
    budget: &mut Budget,
    stop: &Stop,
) -> Result<Sign, MassError> {
    sign_of(&shell_weights(body, shell)?, shell, budget, stop)
}

/// Checks that every shell faces the way its region says: the outside's shells are not
/// [`Sign::Positive`], and every other region has a positive (outer) shell. Each face is
/// between two shells, so a face set inside out shows on one of them.
///
/// # Errors
///
/// [`MassRefusal::NotOutward`] naming the first shell that faces the wrong way; otherwise
/// as [`volume_sign`].
pub fn outward(body: &Brep, budget: &mut Budget, stop: &Stop) -> Result<(), MassError> {
    for region in body.regions() {
        let mut positive = None;
        let mut first = None;
        for s in region.shells() {
            first = first.or_else(|| Some(s.id()));
            let sign = volume_sign(body, s.id(), budget, stop)?;
            if sign == Sign::Positive {
                if region.is_outside() {
                    return Err(MassError::Refused(MassRefusal::NotOutward {
                        shell: s.id(),
                    }));
                }
                positive = Some(s.id());
            }
        }
        if !region.is_outside()
            && positive.is_none()
            && let Some(shell) = first
        {
            return Err(MassError::Refused(MassRefusal::NotOutward { shell }));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::indexing_slicing, clippy::panic)]

    use super::*;

    /// The pin's `bounds_round_up`: an upper bound is never below the value it bounds.
    #[test]
    fn bounds_round_up() {
        for x in [1.0, 0.1, 1e-300, 3.0e300, -2.5] {
            assert!(up(x) >= x);
        }
        let n = norm([Ivl::from(3.0), Ivl::from(-4.0), Ivl::new(-1e-20, 1e-20)]);
        assert!(n.lo >= 5.0, "{n:?}");
    }

    fn block() -> Brep {
        let doc = scx_brep::Document::default();
        let d = scx_testkit::bodies::generate(scx_testkit::bodies::Kind::Block, 0, 1)
            .unwrap()
            .draft;
        d.finish(&doc).map_err(|u| u.cause).unwrap().0
    }

    /// The pin's `a_face_use_without_its_mate_is_refused_by_name`: a face set missing a
    /// face leaves its edges' uses unbalanced, and the first such edge is named.
    #[test]
    fn a_face_use_without_its_mate_is_refused_by_name() {
        let b = block();
        let all = body_weights(&b);
        assert!(balanced(&all).is_ok());
        let some: Vec<_> = all.iter().copied().skip(1).collect();
        let missing = all.first().unwrap().0;
        let edges: Vec<_> = missing
            .loops()
            .flat_map(|l| l.uses().map(|u| u.edge().id()).collect::<Vec<_>>())
            .collect();
        match balanced(&some) {
            Err(MassError::Refused(MassRefusal::OpenBoundary { edge })) => {
                assert!(edges.contains(&edge), "{edge}");
            }
            other => panic!("{other:?}"),
        }
    }

    /// The pin's `constant_u_is_exact_on_the_piece_control_points`: a join at constant `u`
    /// gives `u` exactly, over a box of `t` too, and no higher term.
    #[test]
    fn constant_u_is_exact_on_the_piece() {
        let u = core::f64::consts::FRAC_PI_2;
        let p = Piece {
            path: Path::Segment {
                a: Point2::new(u, 0.25),
                d: Vec2::new(0.0, 0.5),
            },
            lo: 0.0,
            hi: 1.0,
            sign: 1.0,
        };
        let c = p.coeffs(Ivl::new(0.0, 1.0), 4);
        assert_eq!(c[0][0], Ivl::from(u));
        assert_eq!(c[1][0], Ivl::from(0.0));
        assert!(c[0][1].lo <= 0.25 && c[0][1].hi >= 0.75);
    }

    /// The pin's `a_piece_of_constant_u_adds_no_cell`: a piece lying on a cylinder's arc
    /// join (`u = pi / 2`) is one column, a piece crossing it two.
    #[test]
    fn a_piece_on_a_break_line_adds_no_column() {
        use scx_num::frame::Frame;
        use scx_num::units::Length;
        use scx_num::vector3d::Vec3;
        let f = Frame::new(
            Point3::ORIGIN,
            Vec3::new(0.0, 0.0, 1.0),
            Vec3::new(1.0, 0.0, 0.0),
        )
        .unwrap();
        let s = Surface::Cylinder(
            scx_surface::Cylinder::new(f, Length::from_mm(1.0), Sense::Forward).unwrap(),
        );
        let u = core::f64::consts::FRAC_PI_2;
        let along = Piece {
            path: Path::Segment {
                a: Point2::new(u, 0.0),
                d: Vec2::new(0.0, 1.0),
            },
            lo: 0.0,
            hi: 1.0,
            sign: 1.0,
        };
        assert_eq!(piece_columns(&s, &along), vec![(0.0, 1.0)]);
        let across = Piece {
            path: Path::Segment {
                a: Point2::new(1.0, 0.5),
                d: Vec2::new(1.0, 0.0),
            },
            ..along
        };
        let cols = piece_columns(&s, &across);
        assert_eq!(cols.len(), 2, "{cols:?}");
        assert!((cols[0].1 - (u - 1.0)).abs() < 1e-12, "{cols:?}");
    }

    /// The moments of `dt^i` over `[-1, 2]` hold `(2^(i+1) - (-1)^(i+1)) / (i + 1)`, and
    /// those of `|dt|^i` `(2^(i+1) + 1) / (i + 1)`.
    #[test]
    fn moments_hold_their_closed_forms() {
        let (p, a) = moments(Ivl::from(-1.0), Ivl::from(2.0), 5);
        for i in 0..=5 {
            let k = (i + 1) as f64;
            let sign = if i % 2 == 0 { 1.0 } else { -1.0 };
            let two = (0..=i).fold(1.0, |a, _| a * 2.0);
            let want = (two + sign) / k;
            let abs = (two + 1.0) / k;
            assert!(p[i].lo <= want && want <= p[i].hi, "{i}: {:?}", p[i]);
            assert!(a[i].lo <= abs && abs <= a[i].hi, "{i}: {:?}", a[i]);
        }
    }
}
