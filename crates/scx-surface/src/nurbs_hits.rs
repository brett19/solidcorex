// Written for solidcorex. Apache-2.0.
//! Where a line, circle, ellipse, parabola or hyperbola meets a NURBS surface (research 77
//! §6.3, row RY2): certified boxes in `(u, v, t)` over the surface's Bézier patches, through
//! [`scx_algo::global_solve`] as [`crate::nurbs_invert`] is. Nothing samples a grid.
//!
//! - **Cull.** Each patch's certified hull ([`Surface::hulls`]) grown by `tol` is a box; the
//!   patches form a balanced tree of them, and a node the curve's slab (a line's `t` range
//!   through the box, a conic's interval box over its range) misses is dropped with its
//!   patches. A line's range on a surviving patch is its slab through the patch's box and
//!   through the slab of the hull along the patch's normal axis.
//! - **Boxes.** A box `X = U x V x T` is enclosed by the patch's polynomial
//!   ([`NurbsSurface::partials_over_piece`], about the curve's point at the middle of `T`) and
//!   the curve's interval jet. It is dropped when a component of `F = S(u, v) - C(t)` is
//!   farther than `tol` from 0. Where `g = (S_u x S_v) . C'` excludes 0 over it the curve
//!   meets the surface transversally there, and Krawczyk's test on `F` over the box grown by
//!   an eighth (as `scx-ssi`'s general solve) drops it, proves one crossing in it (refined by
//!   Newton and a tight Krawczyk box), or leaves it open.
//! - **Grazing.** Where `g` holds 0 the box is tested for the foot of the curve point on the
//!   patch, before the box's size is checked against the floor: `G = ((S - C) . S_u,
//!   (S - C) . S_v) = 0` by Krawczyk over `Y` for every `t` in `T` at once, where `Y` is
//!   first a box about the Newton foot of the curve's middle point (hulled with the box's
//!   `U x V`) and then `U x V` grown by an eighth, and the proved box is re-proved over its own
//!   growth while that halves it. Proved, every `t` in `T` has exactly one foot in `Y` (and
//!   every point where the curve meets the patch in the box is one), so the curve's signed
//!   distance `delta(t) = (C - S) . n` to that sheet is a smooth function of `t` with
//!   `delta' = C' . n`. `T` is then cut, each piece searching only the growth of its parent's
//!   proved foot box, until each piece is far (`|delta| > tol` by the mean-value form about
//!   its middle, met with `delta`'s direct enclosure), monotone (`delta'` excludes 0: at most
//!   one crossing, found by Newton on `F` and proved by Krawczyk where the ends' signs differ,
//!   and the stretch within `tol` bounded by the slope), or turning (`delta'` holds 0 and
//!   either `delta` is within `tol` over all of it or its curve length is down to the floor:
//!   a possible turning point). A foot that leaves the patch is followed to the floor; one
//!   that leaves the surface's extent marks an edge. A box whose `(U, V, T)` lies in one
//!   already proved is skipped.
//! - **Undecided.** A box whose image is down to `tol` and is neither dropped, proved nor
//!   grazing-proved ends the call with [`SurfaceError::Undecided`] at its `t`: the caller
//!   reads it as unclean (a ray tries its next direction), never as an empty answer. A ray
//!   exactly through a pole of a degenerate patch edge is one.
//!
//! **Reading.** As the analytic arms ([`crate::intersect`]): the stretches within `tol`
//! (turning and monotone pieces, and a tolerance-wide stretch about each proved crossing) that
//! touch in `t` form runs. A run reaching the curve's ends or the surface extent's edge at
//! both of its ends is an [`CurveSurfaceHit::Overlap`]; otherwise a run with a turning piece is
//! one [`Contact::Tangent`] at its nearest possible turning point (the crossings inside are
//! the same touch at the tolerance); otherwise its crossings are [`Contact::Crossing`], a run
//! that is only a curve end a crossing at that end. Crossings found twice (a knot line, the
//! seam of a closed surface, the next patch's grown box) are the same point within `tol` and
//! kept once.
//!
//! The work is charged to the [`Budget`]: one unit for each tree node, each box, each piece of
//! `T` and each foot or crossing refined; the [`Stop`] is polled at the same points. A search
//! that finishes gives the same bits whatever the budget's limit.

use core::cmp::Ordering;
use core::convert::Infallible;
use core::f64::consts::TAU;

use scx_algo::error::{SolveError, SolveFailure};
use scx_algo::global_solve::{
    self, Accumulator, Branch, BranchSolver, ChildOrder, Next, Operation, SolveOptions, Split,
    Unfinished,
};
use scx_algo::krawczyk::{Invert, Linearised, contraction, k_operator, krawczyk_region, newton};
use scx_algo::solution::SolutionObject;
use scx_algo::split_tree::{NodeId, SplitTree};
use scx_curve::Curve;
use scx_num::budget::{Budget, Exhausted, Stop};
use scx_num::extent1d::Extent1d;
use scx_num::extent2d::Extent2d;
use scx_num::extent3d::Extent3d;
use scx_num::fmath;
use scx_num::interval::Ivl;
use scx_num::jet::SurfaceJet;
use scx_num::units::Resolution;
use scx_num::vector2d::Point2;
use scx_num::vector3d::{Point3, Vec3};
use scx_nurbs::Approach;
use scx_nurbs_surface::{About, NurbsSurface, ParamAxis};

use crate::error::SurfaceError;
use crate::hull::Hulls;
use crate::intersect::{Contact, CurveSurfaceHit};
use crate::surface::Surface;

/// A box's image size, in tolerances, at which it is undecided if nothing proved it.
const FLOOR_TOLS: f64 = 1.0;

/// The most bisections that place a crossing or a turning point by sign.
const BISECTIONS: u32 = 64;

/// The most re-proofs that tighten a foot's box.
const TIGHTEN_STEPS: u32 = 3;

/// The relative gap, in `t`, at which two stretches still touch (their ends are computed).
const TOUCH_REL: f64 = 64.0 * f64::EPSILON;

/// An interval 3-vector.
type V3 = [Ivl; 3];

/// Why a search stopped.
#[derive(Debug, Clone, Copy, PartialEq)]
enum Halt {
    Exhausted(Exhausted),
    Stopped,
    Undecided(f64),
    Defect(&'static str),
}

impl From<Halt> for SurfaceError {
    fn from(h: Halt) -> Self {
        match h {
            Halt::Exhausted(e) => Self::Exhausted(e),
            Halt::Stopped => Self::Stopped,
            Halt::Undecided(t) => Self::Undecided { t },
            Halt::Defect(reason) => Self::Defect { reason },
        }
    }
}

/// Every place the analytic curve `c` over `ce` meets the NURBS surface `n` (`s`'s arm) in
/// `se`, as [`crate::intersect::intersect_curve`] gives them (module docs).
pub(crate) fn intersect(
    c: &Curve,
    ce: &Extent1d,
    s: &Surface,
    n: &NurbsSurface<3>,
    se: &Extent2d,
    res: Resolution,
    budget: &mut Budget,
    stop: &Stop,
) -> Result<Vec<CurveSurfaceHit>, SurfaceError> {
    stop.check().map_err(|_| SurfaceError::Stopped)?;
    let tol = res.zone().mm();
    let Some(clip) = se.intersect(n.domain()) else {
        return Ok(Vec::new());
    };
    let Some(hulls) = s.hulls() else {
        return Err(SurfaceError::Undecided { t: ce.mid() });
    };
    let Some(mut search) = Search::new(c, ce, n, &hulls, clip, tol, stop) else {
        return Ok(Vec::new());
    };
    search.run(budget).map_err(SurfaceError::from)?;
    search.answer(budget).map_err(SurfaceError::from)
}

// ---------------------------------------------------------------- interval vectors

fn v3(v: Vec3<Ivl>) -> V3 {
    [v.x, v.y, v.z]
}

fn sub(a: V3, b: V3) -> V3 {
    let [a0, a1, a2] = a;
    let [b0, b1, b2] = b;
    [a0 - b0, a1 - b1, a2 - b2]
}

fn dot(a: V3, b: V3) -> Ivl {
    let [a0, a1, a2] = a;
    let [b0, b1, b2] = b;
    a0 * b0 + a1 * b1 + a2 * b2
}

fn cross(a: V3, b: V3) -> V3 {
    let [a0, a1, a2] = a;
    let [b0, b1, b2] = b;
    [a1 * b2 - a2 * b1, a2 * b0 - a0 * b2, a0 * b1 - a1 * b0]
}

/// The largest magnitude of a component.
fn mag(a: V3) -> f64 {
    a.iter().fold(0.0_f64, |m, x| m.max(x.mag()))
}

fn finite<const N: usize>(a: &[Ivl; N]) -> bool {
    a.iter().all(|x| x.is_finite())
}

fn width(x: Ivl) -> f64 {
    x.hi - x.lo
}

fn excludes_zero(x: Ivl) -> bool {
    x.lo > 0.0 || x.hi < 0.0
}

/// Whether `x` lies in `y`.
fn within<const N: usize>(x: &[Ivl; N], y: &[Ivl; N]) -> bool {
    x.iter().zip(y).all(|(a, b)| a.lo >= b.lo && a.hi <= b.hi)
}

/// Whether `x` and `y` meet.
fn meets<const N: usize>(x: &[Ivl; N], y: &[Ivl; N]) -> bool {
    x.iter().zip(y).all(|(a, b)| a.lo <= b.hi && b.lo <= a.hi)
}

/// The overlap of two intervals that meet (`a` where they do not).
fn meet(a: Ivl, b: Ivl) -> Ivl {
    let (lo, hi) = (a.lo.max(b.lo), a.hi.min(b.hi));
    if lo <= hi { Ivl::new(lo, hi) } else { a }
}

/// `x` grown by an eighth of each side and a little more, so a root on a face is inside.
fn grow<const N: usize>(x: &[Ivl; N]) -> [Ivl; N] {
    x.map(|i| {
        let pad = 0.125 * width(i) + 1e-15 * (1.0 + i.lo.abs().max(i.hi.abs()));
        (Ivl::from(i.lo) - Ivl::from(pad)).hull(Ivl::from(i.hi) + Ivl::from(pad))
    })
}

fn mids<const N: usize>(columns: &[[Ivl; N]; N]) -> [[f64; N]; N] {
    columns.map(|c| c.map(Ivl::mid))
}

/// The Jacobian by rows from its columns.
fn rows<const N: usize>(columns: &[[Ivl; N]; N]) -> [[Ivl; N]; N] {
    core::array::from_fn(|r| {
        core::array::from_fn(|c| {
            columns
                .get(c)
                .and_then(|col| col.get(r))
                .copied()
                .unwrap_or_else(Ivl::whole)
        })
    })
}

/// What Krawczyk's operator says of a box.
enum Test<const N: usize> {
    /// No root in it.
    None,
    /// Exactly one root in the box, held by the operator's image (met with the box).
    Unique([Ivl; N]),
    /// Neither.
    Open,
}

/// Krawczyk's operator over `x` about `mid`, with the inverse `c` (rows) of the Jacobian
/// there, the system `f` at `mid` and its Jacobian `columns` over `x`.
fn k_test<const N: usize>(
    mid: &[f64; N],
    c: &[[f64; N]; N],
    f: &[Ivl; N],
    columns: &[[Ivl; N]; N],
    x: &[Ivl; N],
) -> Test<N> {
    let (cm, _) = contraction(c, &rows(columns));
    let k = k_operator(mid, c, f, &cm, x);
    if k.iter().zip(x).any(|(a, b)| a.hi < b.lo || a.lo > b.hi) {
        return Test::None;
    }
    if !k.iter().zip(x).all(|(a, b)| a.lo > b.lo && a.hi < b.hi) {
        return Test::Open;
    }
    let mut out = k;
    for (o, b) in out.iter_mut().zip(x) {
        *o = meet(*o, *b);
    }
    Test::Unique(out)
}

/// The two halves of `x` across the side whose image is widest: the side's width times the
/// largest magnitude in its column. `None` when that side cannot be split in `f64`.
fn halve<const N: usize>(x: &[Ivl; N], reach: &[f64; N]) -> Option<[[Ivl; N]; 2]> {
    let mut best: Option<(usize, f64)> = None;
    for (i, (side, &s)) in x.iter().zip(reach).enumerate() {
        let w = width(*side);
        let r = if s.is_finite() && s > 0.0 { w * s } else { w };
        if best.is_none_or(|(_, b)| r > b) {
            best = Some((i, r));
        }
    }
    let (best, _) = best?;
    let side = x.get(best)?;
    let [lo, hi] = halves(*side)?;
    let (mut a, mut b) = (*x, *x);
    *a.get_mut(best)? = lo;
    *b.get_mut(best)? = hi;
    Some([a, b])
}

/// An interval's two halves, `None` when it holds no double inside.
fn halves(x: Ivl) -> Option<[Ivl; 2]> {
    let m = 0.5 * (x.lo + x.hi);
    (m > x.lo && m < x.hi).then(|| [Ivl::new(x.lo, m), Ivl::new(m, x.hi)])
}

// ---------------------------------------------------------------- the curve's range

/// The curve's parameter range to search: `ce`, one turn of a closed conic from
/// `ce.min()`, a line's `t` through `bbox`, and an open conic's `t` past which it is
/// farther from `bbox`'s middle than its corners are. `None` where nothing is left.
fn range(c: &Curve, ce: &Extent1d, bbox: &Extent3d) -> Option<(Ivl, bool)> {
    let full = Ivl::new(ce.min(), ce.max());
    let mid = bbox.mid();
    let reach = 0.5 * bbox.size().length();
    let out = |a: f64, b: f64| {
        let r = meet_opt(full, Ivl::new(a, b))?;
        r.is_finite().then_some(r)
    };
    match c {
        Curve::Line(l) => {
            let [a, b] = slab(bbox, l.origin(), l.direction())?;
            out(a, b).map(|r| (r, false))
        }
        Curve::Circle(_) | Curve::Ellipse(_) if ce.length() >= TAU => {
            Some((Ivl::new(ce.min(), ce.min() + TAU), true))
        }
        Curve::Circle(_) | Curve::Ellipse(_) => full.is_finite().then_some((full, false)),
        Curve::Parabola(p) => {
            // |P(s) - V| >= F s^2.
            let f = p.focal().mm();
            let d = (p.frame().origin() - mid).length() + reach;
            let s = (d / f).sqrt() * (1.0 + 1e-12) + 1e-12;
            out(-s, s).map(|r| (r, false))
        }
        Curve::Hyperbola(h) => {
            // |H(t) - C| >= a cosh t.
            let a = h.semi_x().mm();
            let d = (h.frame().origin() - mid).length() + reach;
            // acosh(x) <= ln(2x) for x >= 1: a range a little long is still a bound
            let s = fmath::ln(2.0 * (d / a).max(1.0)) * (1.0 + 1e-12) + 1e-12;
            out(-s, s).map(|r| (r, false))
        }
        _ => None,
    }
}

fn meet_opt(a: Ivl, b: Ivl) -> Option<Ivl> {
    let (lo, hi) = (a.lo.max(b.lo), a.hi.min(b.hi));
    (lo <= hi).then(|| Ivl::new(lo, hi))
}

/// The `t` range over which `o + t d` lies in `b`, widened by a few ulps for the rounding.
fn slab(b: &Extent3d, o: Point3, d: Vec3) -> Option<[f64; 2]> {
    let (lo, hi) = (b.min(), b.max());
    let mut t = [f64::NEG_INFINITY, f64::INFINITY];
    for (o, d, lo, hi) in [
        (o.x, d.x, lo.x, hi.x),
        (o.y, d.y, lo.y, hi.y),
        (o.z, d.z, lo.z, hi.z),
    ] {
        clip_slab(&mut t, o, d, lo, hi)?;
    }
    Some(t)
}

/// `t` clipped to where `o + t d` is in `[lo, hi]`; `None` where it is nowhere.
fn clip_slab(t: &mut [f64; 2], o: f64, d: f64, lo: f64, hi: f64) -> Option<()> {
    let [t0, t1] = t;
    if d == 0.0 {
        return (lo <= o && o <= hi).then_some(());
    }
    let (a, b) = ((lo - o) / d, (hi - o) / d);
    let (a, b) = if a <= b { (a, b) } else { (b, a) };
    let pad =
        4.0 * f64::EPSILON * (a.abs().max(b.abs()) + (o.abs() + lo.abs().max(hi.abs())) / d.abs());
    *t0 = t0.max(a - pad);
    *t1 = t1.min(b + pad);
    (*t0 <= *t1).then_some(())
}

// ---------------------------------------------------------------- the search

/// One patch to search: its hull's index, its rectangle clipped to the extent, and its
/// span indices.
#[derive(Debug, Clone, Copy)]
struct Job {
    hull: usize,
    clip: [Ivl; 2],
    span: [usize; 2],
}

/// The item of a tree node: the first patch below it.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Item {
    job: usize,
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct NoObject;

impl SolutionObject for NoObject {
    fn parameter_count(&self) -> Option<usize> {
        Some(3)
    }
    fn parameter_interval(&self, _axis: usize) -> Option<Extent1d> {
        None
    }
    fn tolerance(&self) -> f64 {
        0.0
    }
}

/// A crossing found: its `t` and `uv`.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Root {
    t: f64,
    uv: Point2,
}

/// A stretch of `t` over which the curve is within `tol` of the surface (or may be).
#[derive(Debug, Clone, Copy, PartialEq)]
struct Stretch {
    lo: f64,
    hi: f64,
    /// `delta'` may vanish on it: a possible turning point.
    turning: bool,
    /// The least `|delta|` at its middle, for the tangent's place.
    near: f64,
    /// A `t` and a `uv` near the foot there.
    at: f64,
    uv: Point2,
    /// Its feet reach past the surface's extent.
    edge: bool,
}

/// The surface and patch enclosed over a box, for Krawczyk on `F = S - C`.
struct Enclosed {
    f: V3,
    columns: [V3; 3],
    /// `(S_u x S_v) . C'`.
    g: Ivl,
    /// `|S_u x S_v|`.
    normal: Ivl,
    /// The side's width times the largest magnitude of its column.
    reach: [f64; 3],
}

struct Search<'a> {
    c: &'a Curve,
    ce: Extent1d,
    n: &'a NurbsSurface<3>,
    hulls: &'a Hulls,
    clip: [Ivl; 2],
    tol: f64,
    stop: &'a Stop,
    range: Ivl,
    closed: bool,
    jobs: Vec<Job>,
    tree: SplitTree<Item>,
    /// A conic's interval box over the range (a line's slab is exact).
    curve_box: Option<Extent3d>,
    roots: Vec<Root>,
    stretches: Vec<Stretch>,
}

impl<'a> Search<'a> {
    fn new(
        c: &'a Curve,
        ce: &Extent1d,
        n: &'a NurbsSurface<3>,
        hulls: &'a Hulls,
        clip: Extent2d,
        tol: f64,
        stop: &'a Stop,
    ) -> Option<Self> {
        let clip_ivl = [
            Ivl::new(clip.x_min(), clip.x_max()),
            Ivl::new(clip.y_min(), clip.y_max()),
        ];
        let mut jobs = Vec::new();
        let mut boxes = Vec::new();
        let (ku, kv) = (n.knot_vector(ParamAxis::U), n.knot_vector(ParamAxis::V));
        for k in 0.. {
            let Some(hull) = hulls.get(k) else { break };
            let uv = hull.uv();
            let Some(part) = uv.intersect(clip) else {
                continue;
            };
            let m = uv.mid();
            let (Ok(su), Ok(sv)) = (ku.span(m.x, Approach::Above), kv.span(m.y, Approach::Above))
            else {
                continue;
            };
            let Some(b) = hull.bbox().and_then(|b| b.expanded(tol)) else {
                continue;
            };
            jobs.push(Job {
                hull: k,
                clip: [
                    Ivl::new(part.x_min(), part.x_max()),
                    Ivl::new(part.y_min(), part.y_max()),
                ],
                span: [su, sv],
            });
            boxes.push(b);
        }
        let all = boxes.iter().copied().reduce(Extent3d::union)?;
        let (range, closed) = range(c, ce, &all)?;
        let curve_box = if let Curve::Line(_) = c {
            None
        } else {
            let p = c.point_over(range);
            Extent3d::from_bounds(p.x.lo, p.y.lo, p.z.lo, p.x.hi, p.y.hi, p.z.hi)
        };
        let tree = tree(&boxes)?;
        Some(Self {
            c,
            ce: *ce,
            n,
            hulls,
            clip: clip_ivl,
            tol,
            stop,
            range,
            closed,
            jobs,
            tree,
            curve_box,
            roots: Vec::new(),
            stretches: Vec::new(),
        })
    }

    fn tick(&self, budget: &mut Budget) -> Result<(), Halt> {
        budget.charge(1).map_err(Halt::Exhausted)?;
        self.stop.check().map_err(|_| Halt::Stopped)
    }

    fn run(&mut self, budget: &mut Budget) -> Result<(), Halt> {
        let tree = core::mem::take(&mut self.tree);
        let options = SolveOptions::new(Operation::Intersect, 0.0);
        let stop = self.stop;
        let out = global_solve::solve(
            [&tree],
            Accumulator::<NoObject>::new(options),
            self,
            budget,
            stop,
        );
        self.tree = tree;
        match out {
            Ok(_) => Ok(()),
            Err(Unfinished { error, .. }) => Err(match error {
                SolveError::Eval(h) => h,
                SolveError::Failed {
                    failure: SolveFailure::BranchesExhausted { limit },
                    ..
                } => Halt::Exhausted(Exhausted { limit }),
                SolveError::Failed {
                    failure: SolveFailure::Cancelled,
                    ..
                } => Halt::Stopped,
                _ => Halt::Defect("the patch search failed"),
            }),
        }
    }

    /// Whether the curve may come within `tol` of `b` (grown by `tol` already).
    fn may_meet(&self, b: &Extent3d) -> bool {
        match (self.c, &self.curve_box) {
            (Curve::Line(l), _) => slab(b, l.origin(), l.direction())
                .and_then(|[a, z]| meet_opt(self.range, Ivl::new(a, z)))
                .is_some(),
            (_, Some(cb)) => cb.intersect(*b).is_some(),
            _ => true,
        }
    }

    /// The `t` range to search on a patch: a line's slab through the patch's box and its
    /// hull's slab along its axis, a conic's whole range.
    fn job_range(&self, job: &Job) -> Option<Ivl> {
        let hull = self.hulls.get(job.hull)?;
        let Curve::Line(l) = self.c else {
            return Some(self.range);
        };
        let b = hull.bbox()?.expanded(self.tol)?;
        let [a, z] = slab(&b, l.origin(), l.direction())?;
        let mut t = [a, z];
        if let Some(axis) = hull.axis() {
            let r = hull.range_along(axis);
            let (o, d) = (l.origin().to_vec().dot(axis), l.direction().dot(axis));
            let pad = self.tol + 4.0 * f64::EPSILON * (r.lo.abs().max(r.hi.abs()) + o.abs());
            clip_slab(&mut t, o, d, r.lo - pad, r.hi + pad)?;
        }
        let [a, z] = t;
        meet_opt(self.range, Ivl::new(a, z))
    }

    fn origin(&self, t: f64) -> [f64; 3] {
        let p = self.c.point(t);
        if p.x.is_finite() && p.y.is_finite() && p.z.is_finite() {
            [p.x, p.y, p.z]
        } else {
            [0.0; 3]
        }
    }

    /// `S - o` and its partials to order `N` over `uv` on the patch's piece.
    fn surface<const N: usize>(
        &self,
        job: &Job,
        uv: [Ivl; 2],
        o: [f64; 3],
    ) -> Option<SurfaceJet<[Ivl; 3], N>> {
        self.n
            .partials_over_piece::<N>(job.span, uv, About::Point(o))
            .ok()
    }

    /// `C - o` and `C'` over `t`.
    fn curve(&self, t: Ivl, o: [f64; 3]) -> (V3, V3) {
        let j = self.c.jet_over::<Ivl, 1>(t, Approach::Above);
        let [o0, o1, o2] = o;
        let p = j.point;
        let c = [
            p.x - Ivl::from(o0),
            p.y - Ivl::from(o1),
            p.z - Ivl::from(o2),
        ];
        let [d] = j.d;
        (c, v3(d))
    }

    // ------------------------------------------------------------ F = S - C

    fn system(&self, job: &Job, x: &[Ivl; 3]) -> Option<Enclosed> {
        let [u, v, t] = *x;
        let o = self.origin(t.mid());
        let j = self.surface::<1>(job, [u, v], o)?;
        let (s, su, sv) = (j.point(), j.get(1, 0)?, j.get(0, 1)?);
        let (c, dc) = self.curve(t, o);
        let f = sub(s, c);
        let nn = cross(su, sv);
        let g = dot(nn, dc);
        let normal = dot(nn, nn).sqrt();
        let minus = dc.map(|x| Ivl::from(0.0) - x);
        let reach = [width(u) * mag(su), width(v) * mag(sv), width(t) * mag(dc)];
        (finite(&f) && finite(&su) && finite(&sv) && finite(&dc)).then_some(Enclosed {
            f,
            columns: [su, sv, minus],
            g,
            normal,
            reach,
        })
    }

    /// Krawczyk's test on `F` over `x`, refined to a tight box where it proves a root.
    fn test_f(&self, job: &Job, x: &[Ivl; 3]) -> Test<3> {
        let m = x.map(Ivl::mid);
        let (Some(at), Some(over)) = (self.system(job, &m.map(Ivl::from)), self.system(job, x))
        else {
            return Test::Open;
        };
        let Some(c) = mids(&at.columns).inverse_of_columns() else {
            return Test::Open;
        };
        match k_test(&m, &c, &at.f, &over.columns, x) {
            Test::Unique(coarse) => Test::Unique(self.tighten(job, m, x).unwrap_or(coarse)),
            other => other,
        }
    }

    /// Newton's root of `F` from `start` and a tight Krawczyk box about it, where that box
    /// lies in `x`.
    fn tighten(&self, job: &Job, start: [f64; 3], x: &[Ivl; 3]) -> Option<[Ivl; 3]> {
        let point = |p: [f64; 3]| -> Result<Linearised<3>, Infallible> {
            Ok(self
                .system(job, &p.map(Ivl::from))
                .map(|e| (e.f.map(Ivl::mid), mids(&e.columns))))
        };
        let r = newton(start, point).ok().flatten()?;
        let cr = self
            .system(job, &r.map(Ivl::from))
            .and_then(|e| mids(&e.columns).inverse_of_columns())?;
        let value =
            |y: [Ivl; 3]| -> Result<_, Infallible> { Ok(self.system(job, &y).map(|e| e.f)) };
        let jacobian =
            |y: [Ivl; 3]| -> Result<_, Infallible> { Ok(self.system(job, &y).map(|e| e.columns)) };
        let t = krawczyk_region(r, cr, value, jacobian).ok().flatten()?;
        within(&t.root, x).then_some(t.root)
    }

    // ------------------------------------------------------------ the foot

    /// `G = ((S - C) . S_u, (S - C) . S_v)` over `y` for `t` in `ts`, and its columns.
    fn foot_system(
        &self,
        job: &Job,
        y: [Ivl; 2],
        ts: Ivl,
        o: [f64; 3],
    ) -> Option<([Ivl; 2], [[Ivl; 2]; 2])> {
        let j = self.surface::<2>(job, y, o)?;
        let (s, su, sv) = (j.point(), j.get(1, 0)?, j.get(0, 1)?);
        let (suu, suv, svv) = (j.get(2, 0)?, j.get(1, 1)?, j.get(0, 2)?);
        let (c, _) = self.curve(ts, o);
        let r = sub(s, c);
        let g = [dot(r, su), dot(r, sv)];
        let uv = dot(su, sv) + dot(r, suv);
        let cols = [
            [dot(su, su) + dot(r, suu), uv],
            [uv, dot(sv, sv) + dot(r, svv)],
        ];
        (finite(&g) && cols.iter().all(finite)).then_some((g, cols))
    }

    /// The box holding the one foot in `y` of every `t` in `ts`, proved by Krawczyk (module
    /// docs); `None` where the test does not prove it.
    fn foot_over(&self, job: &Job, y: [Ivl; 2], ts: Ivl) -> Option<[Ivl; 2]> {
        let o = self.origin(ts.mid());
        let m = y.map(Ivl::mid);
        let point = m.map(Ivl::from);
        let (gm, _) = self.foot_system(job, point, ts, o)?;
        let (_, cm) = self.foot_system(job, point, Ivl::from(ts.mid()), o)?;
        let c = mids(&cm).inverse_of_columns()?;
        let (_, cols) = self.foot_system(job, y, ts, o)?;
        match k_test(&m, &c, &gm, &cols, &y) {
            Test::Unique(k) => Some(k),
            _ => None,
        }
    }

    /// [`Search::foot_over`], then tightened: the one foot in `y` lies in `k`, so a proof
    /// over `k` grown says where in it (the operator over a wide `y` keeps that width).
    fn foot_tight(&self, job: &Job, y: [Ivl; 2], ts: Ivl) -> Option<[Ivl; 2]> {
        let mut k = self.foot_over(job, y, ts)?;
        for _ in 0..TIGHTEN_STEPS {
            let Some(next) = self.foot_over(job, grow(&k), ts) else {
                break;
            };
            let [a, b] = k;
            let [c, d] = next;
            let shrunk = width(c) < 0.5 * width(a) || width(d) < 0.5 * width(b);
            k = [meet(c, a), meet(d, b)];
            if !shrunk {
                break;
            }
        }
        Some(k)
    }

    /// A tight box about the foot in `y` of the point at `t`, from `hint`.
    fn foot_at(&self, job: &Job, y: [Ivl; 2], t: f64, hint: [f64; 2]) -> Option<[Ivl; 2]> {
        let ts = Ivl::from(t);
        let tight = self.newton_foot(job, t, hint).and_then(|r| {
            let b = r.map(|x| {
                let pad = 1e-9 * (1.0 + x.abs());
                Ivl::new(x - pad, x + pad)
            });
            within(&b, &y).then(|| self.foot_over(job, b, ts)).flatten()
        });
        tight.or_else(|| self.foot_tight(job, y, ts))
    }

    /// Newton's foot of the curve's point at `t` on the patch's polynomial, from `hint`.
    fn newton_foot(&self, job: &Job, t: f64, hint: [f64; 2]) -> Option<[f64; 2]> {
        let o = self.origin(t);
        let ts = Ivl::from(t);
        let at = |uv: [f64; 2]| -> Result<Linearised<2>, Infallible> {
            Ok(self
                .foot_system(job, uv.map(Ivl::from), ts, o)
                .map(|(g, c)| (g.map(Ivl::mid), mids(&c))))
        };
        newton(hint, at).ok().flatten()
    }

    /// The box `Y` to prove the feet of `t` in for a box `U x V x T`: `U x V` grown by an
    /// eighth, then that box's hull with a box about Newton's foot of the curve's point at
    /// `T`'s middle as wide as `U x V` each way (the feet move with `t`, so a box about the
    /// middle's foot holds them where `U x V` itself is off-centre). Each holds `U x V`, so a
    /// proof over it accounts for every root in the box.
    fn foot_boxes(&self, job: &Job, x: &[Ivl; 3]) -> [Option<[Ivl; 2]>; 2] {
        let [u, v, t] = *x;
        let plain = grow(&[u, v]);
        let centred = self
            .newton_foot(job, t.mid(), [u.mid(), v.mid()])
            .map(|[fu, fv]| {
                let side = |f: f64, i: Ivl, g: Ivl| {
                    let w = width(i);
                    Ivl::new(f - w, f + w).hull(g)
                };
                let [pu, pv] = plain;
                [side(fu, u, pu), side(fv, v, pv)]
            })
            .filter(finite);
        [centred, Some(plain)]
    }

    /// The unit normal's components over `k`, about `o`, with `S - o`.
    fn normal_over(&self, job: &Job, k: [Ivl; 2], o: [f64; 3]) -> Option<(V3, V3)> {
        let j = self.surface::<1>(job, k, o)?;
        let nn = cross(j.get(1, 0)?, j.get(0, 1)?);
        let len = dot(nn, nn).sqrt();
        excludes_zero(len).then(|| (j.point(), nn.map(|x| x / len)))
    }

    /// `delta = (C - S) . n` at `t` with its foot in `k`.
    fn delta_at(&self, job: &Job, k: [Ivl; 2], t: f64) -> Option<Ivl> {
        let o = self.origin(t);
        let (s, n) = self.normal_over(job, k, o)?;
        let (c, _) = self.curve(Ivl::from(t), o);
        Some(dot(sub(c, s), n))
    }

    /// `delta = (C - S) . n` over `ts` with the feet in `k`, directly.
    fn delta_over(&self, job: &Job, k: [Ivl; 2], ts: Ivl) -> Option<Ivl> {
        let o = self.origin(ts.mid());
        let (s, n) = self.normal_over(job, k, o)?;
        let (c, _) = self.curve(ts, o);
        Some(dot(sub(c, s), n))
    }

    /// `delta' = C' . n` over `ts` with the feet in `k`.
    fn slope_over(&self, job: &Job, k: [Ivl; 2], ts: Ivl) -> Option<Ivl> {
        let o = self.origin(ts.mid());
        let (_, n) = self.normal_over(job, k, o)?;
        let (_, dc) = self.curve(ts, o);
        Some(dot(dc, n))
    }

    // ------------------------------------------------------------ one patch

    fn search(&mut self, job: &Job, budget: &mut Budget) -> Result<(), Halt> {
        let Some(t0) = self.job_range(job) else {
            return Ok(());
        };
        let [cu, cv] = job.clip;
        let start = [cu, cv, t0];
        let floor = FLOOR_TOLS * self.tol;
        let mut proved: Vec<[Ivl; 3]> = Vec::new();
        let mut stack = vec![start];
        while let Some(x) = stack.pop() {
            self.tick(budget)?;
            if proved.iter().any(|r| within(&x, r)) {
                continue;
            }
            let [u, v, t] = x;
            let Some(e) = self.system(job, &x) else {
                // too wide for the piece's form: smaller boxes may succeed
                let reach = [width(u), width(v), width(t)];
                let halves = halve(&x, &reach).ok_or_else(|| Halt::Undecided(t.mid()))?;
                let [lo, hi] = halves;
                stack.push(hi);
                stack.push(lo);
                continue;
            };
            if e.f.iter().any(|f| f.lo > self.tol || f.hi < -self.tol) {
                continue;
            }
            if excludes_zero(e.g) {
                let grown = grow(&x);
                match self.test_f(job, &grown) {
                    Test::None => continue,
                    Test::Unique(root) => {
                        self.take_root(job, &start, &root, &grown);
                        proved.push(grown);
                        if within(&x, &grown) {
                            continue;
                        }
                    }
                    Test::Open => {}
                }
            } else {
                let found = self
                    .foot_boxes(job, &x)
                    .into_iter()
                    .flatten()
                    .find_map(|y| self.foot_tight(job, y, t).map(|k| (y, k)));
                if let Some((y, k)) = found {
                    if meets(&k, &[u, v]) {
                        self.feet(job, y, t, budget)?;
                    }
                    // Every root with `(u, v)` in `y` and `t` in `T` is a foot it analysed.
                    let [yu, yv] = y;
                    proved.push([yu, yv, t]);
                    continue;
                }
            }
            if e.reach.iter().fold(0.0_f64, |m, &r| m.max(r)) <= floor {
                return Err(Halt::Undecided(t.mid()));
            }
            let [lo, hi] = halve(&x, &e.reach).ok_or_else(|| Halt::Undecided(t.mid()))?;
            stack.push(hi);
            stack.push(lo);
        }
        Ok(())
    }

    /// Keeps a root Krawczyk proved in `region` where it lies on the patch's clip (a root
    /// just past a curve end, within the stretch it is near over, is taken at that end).
    fn take_root(&mut self, job: &Job, start: &[Ivl; 3], root: &[Ivl; 3], region: &[Ivl; 3]) {
        let [ru, rv, rt] = *root;
        let [su, sv, _] = *start;
        if !meets(&[ru, rv], &[su, sv]) {
            return;
        }
        let t = rt.mid();
        let uv = Point2::new(ru.mid(), rv.mid());
        // The stretch within `tol`: `|delta'| >= mig(g) / mag(|N|)` over the region.
        let w = self.system(job, region).map_or(0.0, |e| {
            let slope = e.g.mig() / e.normal.mag();
            if slope > 0.0 {
                self.tol / slope
            } else {
                f64::INFINITY
            }
        });
        let [_, _, gt] = *region;
        let (lo, hi) = ((t - w).max(gt.lo), (t + w).min(gt.hi));
        let r = self.range;
        if hi < r.lo || lo > r.hi {
            return;
        }
        let t = t.clamp(r.lo, r.hi);
        self.roots.push(Root { t, uv });
        self.stretches.push(Stretch {
            lo: lo.max(r.lo),
            hi: hi.min(r.hi),
            turning: false,
            near: 0.0,
            at: t,
            uv,
            edge: false,
        });
    }

    /// Cuts `t0` until each piece is far, monotone or turning, with the feet in `y` (proved
    /// unique there for every `t` in `t0`).
    fn feet(&mut self, job: &Job, y: [Ivl; 2], t0: Ivl, budget: &mut Budget) -> Result<(), Halt> {
        let floor = FLOOR_TOLS * self.tol;
        let mut stack = vec![(t0, y)];
        while let Some((t, y)) = stack.pop() {
            self.tick(budget)?;
            let Some(k) = self.foot_tight(job, y, t) else {
                // proved over `t0`, so over `t`: the operator's own looseness
                let [lo, hi] = halves(t).ok_or_else(|| Halt::Undecided(t.mid()))?;
                stack.push((hi, y));
                stack.push((lo, y));
                continue;
            };
            // the one foot over `t` lies in `k`: a sub-piece searches only
            // the box around it, not the whole of `y`
            let [gu, gv] = grow(&k);
            let [yu, yv] = y;
            let sub = [meet(yu, gu), meet(yv, gv)];
            if !meets(&k, &job.clip) {
                continue;
            }
            let tm = t.mid();
            let hint = k.map(Ivl::mid);
            let (Some(km), Some(slope)) =
                (self.foot_at(job, y, tm, hint), self.slope_over(job, k, t))
            else {
                return Err(Halt::Undecided(tm));
            };
            let dm = self.delta_at(job, km, tm).ok_or(Halt::Undecided(tm))?;
            let delta = dm + slope * (t - Ivl::from(tm));
            let delta = self.delta_over(job, k, t).map_or(delta, |d| meet(delta, d));
            if delta.lo > self.tol || delta.hi < -self.tol {
                continue;
            }
            let (_, dc) = self.curve(t, self.origin(tm));
            let at_floor = width(t) * mag(dc) <= floor;
            if !within(&k, &job.clip) && !at_floor {
                let [lo, hi] = halves(t).ok_or(Halt::Undecided(tm))?;
                stack.push((hi, sub));
                stack.push((lo, sub));
                continue;
            }
            let edge = !within(&k, &self.clip);
            let uv = Point2::new(km[0].mid(), km[1].mid());
            if excludes_zero(slope) {
                self.monotone(job, y, t, k, slope, edge, budget)?;
                continue;
            }
            let near_all = delta.lo >= -self.tol && delta.hi <= self.tol;
            if near_all || at_floor {
                self.stretches.push(Stretch {
                    lo: t.lo,
                    hi: t.hi,
                    turning: true,
                    near: dm.mig(),
                    at: tm,
                    uv,
                    edge,
                });
                continue;
            }
            let [lo, hi] = halves(t).ok_or(Halt::Undecided(tm))?;
            stack.push((hi, sub));
            stack.push((lo, sub));
        }
        Ok(())
    }

    /// A piece of `t` over which `delta` is monotone (`slope` excludes 0): its crossing
    /// where the ends' signs may differ, and its stretch within `tol`.
    #[expect(clippy::too_many_arguments, reason = "the piece and its proof, taken")]
    fn monotone(
        &mut self,
        job: &Job,
        y: [Ivl; 2],
        t: Ivl,
        k: [Ivl; 2],
        slope: Ivl,
        edge: bool,
        budget: &mut Budget,
    ) -> Result<(), Halt> {
        let hint = k.map(Ivl::mid);
        let end = |s: &Self, at: f64| {
            s.foot_at(job, y, at, hint)
                .and_then(|kk| s.delta_at(job, kk, at))
                .ok_or(Halt::Undecided(at))
        };
        let (dlo, dhi) = (end(self, t.lo)?, end(self, t.hi)?);
        // Monotone, so `delta` lies between its ends' values over the piece.
        let span = dlo.hull(dhi);
        if span.lo > self.tol || span.hi < -self.tol {
            return Ok(());
        }
        let (m, big) = (slope.mig(), slope.mag());
        let tol = self.tol;
        // `delta` from its low end: between `dlo + m s` and `dlo + M s` rising (falling
        // mirrored), `s = t - t.lo`; within `tol` only between these.
        let (a, b) = if slope.lo > 0.0 {
            (t.lo + (-tol - dlo.hi) / big, t.lo + (tol - dlo.lo) / m)
        } else {
            (t.lo + (dlo.lo - tol) / big, t.lo + (dlo.hi + tol) / m)
        };
        let pad = TOUCH_REL * (t.lo.abs().max(t.hi.abs()) + 1.0);
        let (a, b) = ((a - pad).max(t.lo), (b + pad).min(t.hi));
        if a > b {
            return Ok(());
        }
        let positive = dlo.lo > 0.0 && dhi.lo > 0.0;
        let negative = dlo.hi < 0.0 && dhi.hi < 0.0;
        if !(positive || negative) {
            self.crossing(job, y, t, hint, (dlo, dhi), budget)?;
        }
        self.stretches.push(Stretch {
            lo: a,
            hi: b,
            turning: false,
            near: dlo.mig().min(dhi.mig()),
            at: 0.5 * (a + b),
            uv: Point2::new(hint[0], hint[1]),
            edge,
        });
        Ok(())
    }

    /// The crossing in a monotone piece whose ends' signs may differ: Newton on `F` and
    /// Krawczyk where they prove it, else by the sign of `delta` (monotone, so one root).
    fn crossing(
        &mut self,
        job: &Job,
        y: [Ivl; 2],
        t: Ivl,
        hint: [f64; 2],
        (dlo, dhi): (Ivl, Ivl),
        budget: &mut Budget,
    ) -> Result<(), Halt> {
        self.tick(budget)?;
        let [hu, hv] = hint;
        let x = [y[0], y[1], t];
        if let Some(r) = self.tighten(job, [hu, hv, t.mid()], &grow(&x)) {
            let [ru, rv, rt] = r;
            if meets(&[ru, rv], &job.clip) && rt.lo <= t.hi && rt.hi >= t.lo {
                self.roots.push(Root {
                    t: rt.mid().clamp(self.range.lo, self.range.hi),
                    uv: Point2::new(ru.mid(), rv.mid()),
                });
            }
            return Ok(());
        }
        // A sign change proved (or a root at an end): bisect on the sign.
        let rising = dhi.mid() > dlo.mid();
        let (mut lo, mut hi) = (t.lo, t.hi);
        let mut uv = hint;
        for _ in 0..BISECTIONS {
            let m = 0.5 * (lo + hi);
            if !(m > lo && m < hi) {
                break;
            }
            self.tick(budget)?;
            let Some(km) = self.foot_at(job, y, m, uv) else {
                return Err(Halt::Undecided(m));
            };
            uv = km.map(Ivl::mid);
            let Some(d) = self.delta_at(job, km, m) else {
                return Err(Halt::Undecided(m));
            };
            if !excludes_zero(d) {
                lo = m;
                hi = m;
                break;
            }
            if (d.lo > 0.0) == rising {
                hi = m;
            } else {
                lo = m;
            }
        }
        let tc = 0.5 * (lo + hi);
        let [u, v] = uv;
        if meets(&[Ivl::from(u), Ivl::from(v)], &job.clip) {
            self.roots.push(Root {
                t: tc,
                uv: Point2::new(u, v),
            });
        }
        Ok(())
    }

    // ------------------------------------------------------------ the reading

    /// The hits from the roots and stretches (module docs).
    fn answer(&mut self, budget: &mut Budget) -> Result<Vec<CurveSurfaceHit>, Halt> {
        let mut stretches = core::mem::take(&mut self.stretches);
        stretches.sort_by(|a, b| a.lo.total_cmp(&b.lo).then(a.hi.total_cmp(&b.hi)));
        let mut runs: Vec<Vec<Stretch>> = Vec::new();
        for s in stretches {
            let touch = TOUCH_REL * (s.lo.abs() + 1.0);
            match runs.last_mut() {
                Some(run) if run.iter().any(|r| s.lo <= r.hi + touch) => run.push(s),
                _ => runs.push(vec![s]),
            }
        }
        let (lo_end, hi_end) = (self.range.lo, self.range.hi);
        let reaches = |run: &[Stretch], lo: bool| {
            let touch = TOUCH_REL * (lo_end.abs().max(hi_end.abs()) + 1.0);
            if lo {
                run.iter().any(|s| s.lo <= lo_end + touch)
            } else {
                run.iter().any(|s| s.hi >= hi_end - touch)
            }
        };
        // A closed curve's runs at both ends of the turn are one run across its start.
        let mut wrapped = false;
        if self.closed && runs.len() > 1 {
            let first = runs.first().is_some_and(|r| reaches(r, true));
            let last = runs.last().is_some_and(|r| reaches(r, false));
            if first
                && last
                && let (Some(tail), false) = (runs.pop(), runs.is_empty())
                && let Some(head) = runs.first_mut()
            {
                let shifted = head.iter().map(|s| Stretch {
                    lo: s.lo + TAU,
                    hi: s.hi + TAU,
                    at: s.at + TAU,
                    ..*s
                });
                let mut merged = tail;
                merged.extend(shifted);
                *head = merged;
                wrapped = true;
            }
        }
        let mut roots = core::mem::take(&mut self.roots);
        let mut absorbed = vec![false; roots.len()];
        let mut hits = Vec::new();
        for (i, run) in runs.iter().enumerate() {
            let lo = run.iter().map(|s| s.lo).fold(f64::INFINITY, f64::min);
            let hi = run.iter().map(|s| s.hi).fold(f64::NEG_INFINITY, f64::max);
            let wraps = wrapped && i == 0;
            let side = |at_lo: bool| {
                let bound = if at_lo { lo } else { hi };
                let end = !self.closed && reaches(run, at_lo);
                let edge = run
                    .iter()
                    .filter(|s| if at_lo { s.lo <= bound } else { s.hi >= bound })
                    .any(|s| s.edge);
                end || edge
            };
            let whole = self.closed && !wraps && reaches(run, true) && reaches(run, false);
            let inside = |t: f64| {
                let t2 = t + TAU;
                (lo..=hi).contains(&t) || (wraps && (lo..=hi).contains(&t2))
            };
            if whole || (side(true) && side(false)) {
                for (r, a) in roots.iter().zip(absorbed.iter_mut()) {
                    *a |= inside(r.t);
                }
                let (a, b) = if whole {
                    (lo_end, hi_end)
                } else {
                    (lo.max(lo_end), if wraps { hi } else { hi.min(hi_end) })
                };
                if let Some(t) = Extent1d::new(a, b) {
                    hits.push(CurveSurfaceHit::Overlap { t });
                }
                continue;
            }
            let turning = run
                .iter()
                .filter(|s| s.turning)
                .min_by(|a, b| a.near.total_cmp(&b.near).then(a.at.total_cmp(&b.at)));
            if let Some(best) = turning {
                for (r, a) in roots.iter().zip(absorbed.iter_mut()) {
                    *a |= inside(r.t);
                }
                if let Some(h) = self.tangent(best, budget)? {
                    hits.push(h);
                }
                continue;
            }
            let any = roots.iter().any(|r| inside(r.t));
            if !any {
                let open_end = !self.closed && (reaches(run, true) || reaches(run, false));
                let edge = run.iter().any(|s| s.edge);
                if open_end && !edge {
                    let t = if reaches(run, true) { lo_end } else { hi_end };
                    if let Some(r) = run.first() {
                        roots.push(Root { t, uv: r.uv });
                        absorbed.push(false);
                    }
                } else if !edge {
                    return Err(Halt::Undecided(0.5 * (lo + hi)));
                }
            }
        }
        for (r, a) in roots.iter().zip(&absorbed) {
            if !a && let Some(h) = self.point_hit(r.t, r.uv, Contact::Crossing) {
                hits.push(h);
            }
        }
        hits.sort_by(order);
        dedup(&mut hits, self.tol);
        Ok(hits)
    }

    /// The tangent of a run at its nearest possible turning point: `delta'` bisected by sign
    /// over the stretch where its ends' signs differ, else the stretch's middle.
    fn tangent(
        &self,
        best: &Stretch,
        budget: &mut Budget,
    ) -> Result<Option<CurveSurfaceHit>, Halt> {
        let wrap = |t: f64| if t > self.range.hi { t - TAU } else { t };
        let (lo, hi) = (wrap(best.lo), wrap(best.hi).max(wrap(best.lo)));
        let slope = |t: f64, uv: [f64; 2]| -> Option<(f64, [f64; 2])> {
            let foot = self.float_foot(t, uv)?;
            let (n, _) = self.float_normal(foot)?;
            let d = self.c.jet::<1>(t, Approach::Above);
            let [dc] = d.d;
            Some((n.dot(dc), foot))
        };
        let start = [best.uv.x, best.uv.y];
        let mut t = wrap(best.at);
        let mut uv = start;
        if let (Some((sa, ua)), Some((sb, _))) = (slope(lo, start), slope(hi, start))
            && sa != 0.0
            && sb != 0.0
            && (sa > 0.0) != (sb > 0.0)
        {
            let (mut a, mut b) = (lo, hi);
            uv = ua;
            for _ in 0..BISECTIONS {
                let m = 0.5 * (a + b);
                if !(m > a && m < b) {
                    break;
                }
                self.tick(budget)?;
                let Some((sm, um)) = slope(m, uv) else { break };
                uv = um;
                if (sm > 0.0) == (sa > 0.0) {
                    a = m;
                } else {
                    b = m;
                }
            }
            t = 0.5 * (a + b);
        }
        let foot = self.float_foot(t, uv).unwrap_or(uv);
        Ok(self.point_hit(t, Point2::new(foot[0], foot[1]), Contact::Tangent))
    }

    /// The surface's normal (unnormalised, then unit) at `uv`, in floating point.
    fn float_normal(&self, uv: [f64; 2]) -> Option<(Vec3, Vec3)> {
        let j = self.n.jet::<1>(uv, [Approach::Above; 2]).ok()?;
        let p = |i, k| j.get(i, k).map(|a: [f64; 3]| Vec3::new(a[0], a[1], a[2]));
        let nn = p(1, 0)?.cross(p(0, 1)?);
        let len = nn.length();
        (len > 0.0 && len.is_finite()).then(|| (nn * (1.0 / len), nn))
    }

    /// The foot of the curve's point at `t` from `uv`, by Newton in floating point on the
    /// surface itself, clamped to the extent.
    fn float_foot(&self, t: f64, uv: [f64; 2]) -> Option<[f64; 2]> {
        let q = self.c.point(t);
        let [cu, cv] = self.clip;
        let mut uv = uv;
        for _ in 0..scx_algo::krawczyk::NEWTON_STEPS {
            let j = self.n.jet::<2>(uv, [Approach::Above; 2]).ok()?;
            let p = |i, k| j.get(i, k).map(|a: [f64; 3]| Vec3::new(a[0], a[1], a[2]));
            let (s, su, sv) = (p(0, 0)?, p(1, 0)?, p(0, 1)?);
            let (suu, suv, svv) = (p(2, 0)?, p(1, 1)?, p(0, 2)?);
            let r = s - q.to_vec();
            let (g0, g1) = (r.dot(su), r.dot(sv));
            let (a, b, c) = (
                su.dot(su) + r.dot(suu),
                su.dot(sv) + r.dot(suv),
                sv.dot(sv) + r.dot(svv),
            );
            let det = a * c - b * b;
            if !(det.is_finite() && det != 0.0) {
                break;
            }
            let (du, dv) = ((c * g0 - b * g1) / det, (a * g1 - b * g0) / det);
            let next = [
                (uv[0] - du).clamp(cu.lo, cu.hi),
                (uv[1] - dv).clamp(cv.lo, cv.hi),
            ];
            let step = (next[0] - uv[0]).abs().max((next[1] - uv[1]).abs());
            uv = next;
            if step.partial_cmp(&(4.0 * f64::EPSILON * (1.0 + uv[0].abs().max(uv[1].abs()))))
                != Some(Ordering::Greater)
            {
                break;
            }
        }
        (uv[0].is_finite() && uv[1].is_finite()).then_some(uv)
    }

    /// The hit at `t` with its foot `uv`, `None` where the foot is outside the extent past
    /// rounding (else clamped into it); a closed curve's `t` back in its turn.
    fn point_hit(&self, t: f64, uv: Point2, contact: Contact) -> Option<CurveSurfaceHit> {
        let [cu, cv] = self.clip;
        let slack = |i: Ivl| 1e-9 * (1.0 + i.lo.abs().max(i.hi.abs()));
        let (su, sv) = (slack(cu), slack(cv));
        if uv.x < cu.lo - su || uv.x > cu.hi + su || uv.y < cv.lo - sv || uv.y > cv.hi + sv {
            return None;
        }
        let uv = Point2::new(uv.x.clamp(cu.lo, cu.hi), uv.y.clamp(cv.lo, cv.hi));
        let t = if self.closed && t >= self.range.hi {
            t - TAU
        } else {
            t
        };
        let t = t.clamp(self.ce.min(), self.ce.max());
        Some(CurveSurfaceHit::Point {
            t,
            uv,
            point: self.c.point(t),
            contact,
        })
    }
}

/// The hits' order: `t`, then `u`, then `v` (an overlap before a point at its start).
fn order(a: &CurveSurfaceHit, b: &CurveSurfaceHit) -> Ordering {
    let key = |h: &CurveSurfaceHit| match h {
        CurveSurfaceHit::Point { t, uv, .. } => (*t, 1, uv.x, uv.y),
        CurveSurfaceHit::Overlap { t } => (t.min(), 0, f64::NEG_INFINITY, f64::NEG_INFINITY),
    };
    let (ka, kb) = (key(a), key(b));
    ka.0.total_cmp(&kb.0)
        .then(ka.1.cmp(&kb.1))
        .then(ka.2.total_cmp(&kb.2))
        .then(ka.3.total_cmp(&kb.3))
}

/// Drops a point within `tol` of an earlier point (a knot line's, the seam's, the next
/// patch's grown box's copy of it), keeping the first in order; a tangent wins over a
/// crossing at the same place.
fn dedup(hits: &mut Vec<CurveSurfaceHit>, tol: f64) {
    let mut out: Vec<CurveSurfaceHit> = Vec::with_capacity(hits.len());
    for h in hits.drain(..) {
        if let CurveSurfaceHit::Point { point, contact, .. } = h {
            let near = out.iter_mut().find(|o| {
                matches!(o, CurveSurfaceHit::Point { point: p, .. } if (*p - point).length() <= tol)
            });
            if let Some(CurveSurfaceHit::Point { contact: c, .. }) = near {
                if contact == Contact::Tangent {
                    *c = Contact::Tangent;
                }
                continue;
            }
        }
        out.push(h);
    }
    *hits = out;
}

/// A balanced tree over the patches' boxes, in their order.
fn tree(boxes: &[Extent3d]) -> Option<SplitTree<Item>> {
    fn build(
        t: &mut SplitTree<Item>,
        boxes: &[Extent3d],
        lo: usize,
        hi: usize,
    ) -> Option<(NodeId, Extent3d)> {
        if hi - lo == 1 {
            let b = *boxes.get(lo)?;
            return Some((t.leaf(b, Item { job: lo })?, b));
        }
        let mid = lo + (hi - lo) / 2;
        let (a, ba) = build(t, boxes, lo, mid)?;
        let (b, bb) = build(t, boxes, mid, hi)?;
        let bbox = ba.union(bb);
        Some((t.split(bbox, Item { job: lo }, [a, b])?, bbox))
    }
    if boxes.is_empty() {
        return None;
    }
    let mut t = SplitTree::with_capacity(2 * boxes.len());
    build(&mut t, boxes, 0, boxes.len())?;
    Some(t)
}

impl BranchSolver<Item, NoObject, 1> for Search<'_> {
    type Error = Halt;

    fn local_solve(
        &mut self,
        branch: &Branch<'_, Item, 1>,
        _found: &mut Accumulator<NoObject>,
        budget: &mut Budget,
    ) -> Result<Next, SolveError<Halt>> {
        let [node] = branch.nodes();
        let job = *self
            .jobs
            .get(node.item().job)
            .ok_or(SolveError::Eval(Halt::Defect("a patch is missing")))?;
        self.search(&job, budget).map_err(SolveError::Eval)?;
        Ok(Next::Done)
    }

    fn may_contain(
        &mut self,
        branch: &Branch<'_, Item, 1>,
        _found: &Accumulator<NoObject>,
    ) -> Result<bool, SolveError<Halt>> {
        let [node] = branch.nodes();
        Ok(self.may_meet(node.bbox()))
    }

    fn split_choice(
        &self,
        _branch: &Branch<'_, Item, 1>,
        _found: &Accumulator<NoObject>,
    ) -> Result<Split, SolveError<Halt>> {
        Ok(Split {
            tree: 0,
            order: ChildOrder::Given,
        })
    }
}
