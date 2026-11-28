// Written for solidcorex. Apache-2.0.
//! The nearest point of a NURBS surface to a point: a certified branch and bound over the
//! surface's Bezier patches, through [`scx_algo::global_solve`] (63 §4.2, research 65 row 7,
//! research 66).
//!
//! - **Patches.** Each knot span pair is one Bezier patch, its homogeneous points
//!   `(w P, w)` read off the B-spline by blossoming (each Bezier point is the blossom
//!   `f(a^(p - k), b^k)` of the span `[a, b]`, by de Boor's levels), in `u` and then in `v`.
//!   Unclamped knots need nothing more. Every coefficient carries its own bound of its
//!   distance from the exact one, through the blossoming, the shift to `q` and each halving:
//!   a step at a knot is exact, and a step is taken from its nearer end, so a tiny weight's
//!   coefficient keeps a tiny error beside a large one.
//! - **Bounds.** On a piece, `w (S - q)` is a polynomial `D` with Bernstein coefficients
//!   `D_k`, `w` has `w_k`, all positive. Two lower bounds of the squared distance; the piece
//!   takes the larger:
//!   - `|S - q|^2 = N / W` with `N = D . D` and `W = w^2`, of degrees `(2p, 2q)`
//!     ([`scx_algo::bernstein`]), and `N / W >= min_k N_k / W_k`, made certain by the
//!     [`Bounded`] running error (each form seeded with its coefficients' largest error).
//!     It is exact where the distance is constant (a query at a sphere's centre bounds every
//!     patch by the radius at once) and tightens as `h^2` under subdivision elsewhere.
//!   - `(S - q) . n` for the unit `n` toward the corners: a mean of the `(D_k . n) / w_k`
//!     with the positive weights `w_k B_k`, so no less than their least, each taken at the
//!     low side of its own error. It takes no product of the forms, so it stays sharp where
//!     tiny weights make `N`'s coefficients tiny against `N`'s largest.
//!
//!   The corner coefficients are surface points (to rounding, far under `eps`), so their
//!   distances are free upper bounds.
//! - **Search.** The incumbent starts at the best corner of every patch, polished by Newton
//!   from it and from `near`. [`global_solve::solve`] walks a balanced tree of the patches,
//!   nearer half first, and prunes a node whose bound is not below the incumbent less the
//!   gap `eps`; a leaf it reaches is refined best first (a heap keyed by the bound, then a
//!   sequence number: a total order), each piece halved by de Casteljau at 1/2 in the
//!   parameter along which its control polygon is more than twice as long, else in both
//!   (a tiny weight crowds the surface into a thin strip, which quartering would need about
//!   `1 / width` pieces to cover), until the least bound left is not below the incumbent
//!   less `eps`. A corner that beats the incumbent by more than `eps` starts a Newton run
//!   there. A parameter whose interval holds no floating point value inside (a knot span
//!   of one ulp, whose patch can still be long) is not halved: the piece becomes its two
//!   edges there, the only `uv`s that exist. Every region dropped is no nearer than the
//!   answer less `eps`, so the foot is the global nearest over the floating point `uv`s to
//!   `eps`: [`GAP_REL`] of the control net's diagonal plus the first incumbent's distance.
//! - **Newton** on `|S - q|^2 / 2`, projected onto the domain, with a Gauss-Newton step
//!   where the Hessian is not positive and a gradient step where that is singular; a step
//!   is kept only when it lowers the distance (halved up to [`HALVINGS`] times), so it never
//!   loses the incumbent. It sets the foot's `uv` to rounding: the search alone fixes the
//!   distance to `eps`, but the place only to about `sqrt(eps)`.
//!
//! The work is charged to the [`Budget`]: one unit for each tree node tested, each piece
//! refined and each surface evaluation of Newton. The [`Stop`] is polled at the same points
//! and by the solve before every branch. Both end the search with an error, never a partial
//! answer; a search that finishes returns the same bits whatever the budget's limit and
//! whatever the flag did not do. The solve takes the same `&mut Budget` and `&Stop`, which it
//! charges one unit a branch (each tree node tested) and hands the local solves: the budget
//! is the one bound.
//!
//! The foot is the surface's own point at its `uv` ([`NurbsSurface::point`], above), its
//! distance from the query, and its `uv` is placed against the domain as the analytic kinds'
//! are, with no period.

use core::cmp::Ordering;
use std::collections::BinaryHeap;

use scx_algo::bernstein::{Bernstein2, Bounded};
use scx_algo::error::{SolveError, SolveFailure};
use scx_algo::global_solve::{
    self, Accumulator, Branch, BranchSolver, ChildOrder, Next, Operation, SolveOptions, Split,
    Unfinished,
};
use scx_algo::solution::SolutionObject;
use scx_algo::split_tree::{NodeId, SplitTree};
use scx_num::budget::{Budget, Exhausted, Stop};
use scx_num::extent1d::Extent1d;
use scx_num::extent2d::Extent2d;
use scx_num::extent3d::Extent3d;
use scx_num::vector2d::Point2;
use scx_num::vector3d::{Point3, Vec3};
use scx_nurbs::Approach;
use scx_nurbs_surface::{NurbsSurface, ParamAxis};

use crate::error::SurfaceError;
use crate::invert::{self, Closest, Inversion, Periods};

/// The certified gap, relative to the size of the problem: the foot is within
/// `GAP_REL * (diagonal + distance)` of the true nearest distance.
pub(crate) const GAP_REL: f64 = 1e-12;

/// The most steps of one Newton run.
const NEWTON_STEPS: u32 = 40;

/// The most times a Newton step is halved before the run stops.
const HALVINGS: u32 = 30;

const ABOVE: [Approach; 2] = [Approach::Above, Approach::Above];

/// A homogeneous point `(w x, w y, w z, w)`.
pub(crate) type H = [f64; 4];

/// Why a search stopped.
#[derive(Debug, Clone, Copy, PartialEq)]
enum Halt {
    Exhausted(Exhausted),
    Stopped,
    Defect(&'static str),
}

impl From<Halt> for SurfaceError {
    fn from(h: Halt) -> Self {
        match h {
            Halt::Exhausted(e) => Self::Exhausted(e),
            Halt::Stopped => Self::Stopped,
            Halt::Defect(reason) => Self::Defect { reason },
        }
    }
}

/// The nearest point of `n` to `p`, placed against `domain` (`None`: the surface's own).
pub(crate) fn invert(
    n: &NurbsSurface<3>,
    p: Point3,
    near: Option<Point2>,
    domain: Option<&Extent2d>,
    budget: &mut Budget,
    stop: &Stop,
) -> Result<Inversion<Point2>, SurfaceError> {
    stop.check().map_err(|_| SurfaceError::Stopped)?;
    if !(p.x.is_finite() && p.y.is_finite() && p.z.is_finite()) {
        let nan = Point2::new(f64::NAN, f64::NAN);
        let point = Point3::new(f64::NAN, f64::NAN, f64::NAN);
        return Ok(Inversion::Outside(invert::foot(p, nan, point)));
    }
    let uv = nearest(n, p, near, budget, stop)?;
    let point_at = |uv: Point2| {
        let s = n.point(uv.to_array(), ABOVE).unwrap_or([f64::NAN; 3]);
        invert::foot(p, uv, Point3::new(s[0], s[1], s[2]))
    };
    let foot = point_at(uv);
    Ok(invert::invert(
        Closest::One(foot),
        Periods::NONE,
        domain,
        point_at,
    ))
}

/// The `uv` of the nearest point of `n` to `p` (finite).
fn nearest(
    n: &NurbsSurface<3>,
    p: Point3,
    near: Option<Point2>,
    budget: &mut Budget,
    stop: &Stop,
) -> Result<Point2, Halt> {
    let patches = patches(n);
    let mut pieces = Vec::with_capacity(patches.len());
    let mut hull: Option<Extent3d> = None;
    for patch in &patches {
        let piece = Piece::of_patch(patch, p)?;
        hull = Some(hull.map_or(patch.bbox, |h| h.union(patch.bbox)));
        pieces.push(piece);
    }
    let diagonal = hull.map_or(0.0, |h| h.size().length());
    let mut search = Search {
        surface: n,
        domain: n.domain(),
        q: p,
        best_sq: f64::INFINITY,
        best_uv: n.domain().min(),
        eps: 0.0,
        stop,
        seq: 0,
        leaves: Vec::new(),
    };
    // The first incumbent: every patch's corners, then Newton from the best and from `near`.
    for piece in &pieces {
        search.take_corners(piece);
    }
    let seed = search.best_uv;
    search.newton(seed, budget)?;
    if let Some(near) = near.filter(|uv| uv.x.is_finite() && uv.y.is_finite()) {
        let near = search.domain.clamp_point(near);
        search.newton(near, budget)?;
    }
    search.eps = GAP_REL * (diagonal + search.best_sq.sqrt());
    let tree = tree(&patches, &pieces)?;
    search.leaves = pieces.into_iter().map(Some).collect();
    let options = SolveOptions::new(Operation::Minimize, 0.0);
    match global_solve::solve(
        [&tree],
        Accumulator::<NoObject>::new(options),
        &mut search,
        budget,
        stop,
    ) {
        Ok(_) => {}
        Err(Unfinished { error, .. }) => {
            return Err(match error {
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
            });
        }
    }
    // Polish the place: the search fixed the distance, Newton fixes the `uv`.
    let best = search.best_uv;
    search.newton(best, budget)?;
    Ok(search.best_uv)
}

// ---------------------------------------------------------------- the Bezier patches

/// One Bezier patch: its uv rectangle, its homogeneous points with their error bounds
/// (`(p + 1) x (q + 1)`, row `i` along `u`), and the box of its control points.
pub(crate) struct Patch {
    pub(crate) lo: Point2,
    pub(crate) hi: Point2,
    pub(crate) degrees: (usize, usize),
    pub(crate) net: Vec<Hb>,
    bbox: Extent3d,
}

/// A homogeneous point as computed, with a bound of each coordinate's error.
pub(crate) type Hb = (H, H);

/// `x (1 + 4 eps)` and a little more: an error bound rounded up, underflow included.
pub(crate) fn up(x: f64) -> f64 {
    x * (1.0 + 4.0 * f64::EPSILON) + 4.0 * f64::MIN_POSITIVE
}

fn knot(t: &[f64], i: usize) -> f64 {
    t.get(i).copied().unwrap_or(f64::NAN)
}

/// The coordinate `k` of a homogeneous point.
fn at(h: &H, k: usize) -> f64 {
    h.get(k).copied().unwrap_or(f64::NAN)
}

/// The blossom step at `x` between the knots `tj < tk`: `(1 - s) a + s b` with
/// `s = (x - tj) / (tk - tj)`, and its error.
///
/// It is computed from the nearer end, `a + s (b - a)` or `b + r (a - b)` with
/// `r = (tk - x) / (tk - tj)` taken from the knots itself, so the small fraction carries its
/// own `3 eps` relative error and the result stays accurate where it is tiny beside a large
/// end (a tiny weight beside a clustered knot). The error is the inputs' errors mixed as the
/// values are (`r e_a + s e_b`, with `8 eps` to spare for the rounding of `r` and `s`) and
/// the rounding of the fraction, the difference, the product and the sum, under
/// `5 eps t |b - a| + eps |out|` for the fraction `t` used, taken as
/// `6 eps (t |b - a| + |out|)`; rounded up. `x` lies in `[tj, tk]`, so `r` and `s` are in
/// `[0, 1]`.
fn lerp(a: &Hb, b: &Hb, x: f64, tj: f64, tk: f64) -> Hb {
    let (s, r) = ((x - tj) / (tk - tj), (tk - x) / (tk - tj));
    let mut out = ([0.0; 4], [0.0; 4]);
    for k in 0..4 {
        let (va, vb) = (at(&a.0, k), at(&b.0, k));
        let (v, t) = if s <= r {
            (va + s * (vb - va), s)
        } else {
            (vb + r * (va - vb), r)
        };
        let mixed = r * at(&a.1, k) + s * at(&b.1, k);
        let e = mixed * (1.0 + 8.0 * f64::EPSILON)
            + 6.0 * f64::EPSILON * (t * (vb - va).abs() + v.abs());
        if let (Some(slot), Some(err)) = (out.0.get_mut(k), out.1.get_mut(k)) {
            *slot = v;
            *err = up(e);
        }
    }
    out
}

/// The midpoint `(a + b) / 2` and its error: the mean of the errors and one rounding of
/// the sum (the halving is exact), as `eps |m|`, rounded up.
fn mid(a: &Hb, b: &Hb) -> Hb {
    let mut out = ([0.0; 4], [0.0; 4]);
    for k in 0..4 {
        let m = 0.5 * (at(&a.0, k) + at(&b.0, k));
        let e = 0.5 * (at(&a.1, k) + at(&b.1, k)) + f64::EPSILON * m.abs();
        if let (Some(slot), Some(err)) = (out.0.get_mut(k), out.1.get_mut(k)) {
            *slot = m;
            *err = up(e);
        }
    }
    out
}

/// The control points of a Bezier curve's two halves, by de Casteljau at 1/2.
fn halves(c: &[Hb]) -> (Vec<Hb>, Vec<Hb>) {
    let mut level = c.to_vec();
    let (mut lower, mut upper) = (Vec::with_capacity(c.len()), Vec::with_capacity(c.len()));
    while let (Some(&first), Some(&last)) = (level.first(), level.last()) {
        lower.push(first);
        upper.push(last);
        level = level
            .windows(2)
            .filter_map(|w| match w {
                [a, b] => Some(mid(a, b)),
                _ => None,
            })
            .collect();
    }
    upper.reverse();
    (lower, upper)
}

/// The Bezier points of each span of the B-spline of degree `p` on knots `t` with points
/// `c`: `(a, b, points)` for every span `[a, b]` of positive length, in order.
fn spans(t: &[f64], p: usize, c: &[Hb]) -> Vec<(f64, f64, Vec<Hb>)> {
    let mut out = Vec::new();
    for s in p..c.len() {
        let (a, b) = (knot(t, s), knot(t, s + 1));
        if a.partial_cmp(&b) != Some(Ordering::Less) {
            continue;
        }
        let Some(window) = c.get(s - p..=s) else {
            continue;
        };
        let points = (0..=p)
            .map(|k| {
                // The blossom f(a^(p - k), b^k): de Boor's levels, level r at its argument.
                let mut d = window.to_vec();
                for r in 1..=p {
                    let x = if r <= p - k { a } else { b };
                    for j in (s - p + r..=s).rev() {
                        let i = j - (s - p);
                        let (tj, tk) = (knot(t, j), knot(t, j + p + 1 - r));
                        let (Some(&prev), Some(&cur)) = (d.get(i - 1), d.get(i)) else {
                            continue;
                        };
                        // At a knot the blossom is a point itself: exact, so a tiny
                        // weight keeps its own relative error.
                        if let Some(slot) = d.get_mut(i) {
                            *slot = if x == tj || tk <= tj {
                                prev
                            } else if x == tk {
                                cur
                            } else {
                                lerp(&prev, &cur, x, tj, tk)
                            };
                        }
                    }
                }
                d.get(p).copied().unwrap_or(([f64::NAN; 4], [f64::NAN; 4]))
            })
            .collect();
        out.push((a, b, points));
    }
    out
}

/// Every Bezier patch of `n`, `u` spans outer.
pub(crate) fn patches(n: &NurbsSurface<3>) -> Vec<Patch> {
    let (p, q) = (n.degree(ParamAxis::U), n.degree(ParamAxis::V));
    let (tu, tv) = (n.knots(ParamAxis::U), n.knots(ParamAxis::V));
    let points = n.points();
    let nv = points.ncols();
    // The weighted points are the net's own (exact) data: no error yet.
    let hom: Vec<Hb> = match (n.weighted_points(), n.weights()) {
        (Some(wp), Some(w)) => wp
            .points()
            .iter()
            .zip(w.points())
            .map(|(a, &w)| ([a[0], a[1], a[2], w], [0.0; 4]))
            .collect(),
        _ => points
            .points()
            .iter()
            .map(|a| ([a[0], a[1], a[2], 1.0], [0.0; 4]))
            .collect(),
    };
    // Along u, column by column: per u span, its (p + 1) x nv points.
    let columns: Vec<Vec<(f64, f64, Vec<Hb>)>> = (0..nv)
        .map(|j| {
            let col: Vec<Hb> = hom.iter().skip(j).step_by(nv.max(1)).copied().collect();
            spans(tu, p, &col)
        })
        .collect();
    let u_spans = columns.first().map_or(0, Vec::len);
    let mut out = Vec::new();
    for su in 0..u_spans {
        let Some(&(ua, ub, _)) = columns.first().and_then(|c| c.get(su)) else {
            continue;
        };
        // Along v, Bezier row by row: per v span, its q + 1 points.
        let rows: Vec<Vec<(f64, f64, Vec<Hb>)>> = (0..=p)
            .map(|k| {
                let row: Vec<Hb> = columns
                    .iter()
                    .map(|c| {
                        c.get(su)
                            .and_then(|s| s.2.get(k))
                            .copied()
                            .unwrap_or(([f64::NAN; 4], [f64::NAN; 4]))
                    })
                    .collect();
                spans(tv, q, &row)
            })
            .collect();
        let v_spans = rows.first().map_or(0, Vec::len);
        for sv in 0..v_spans {
            let Some(&(va, vb, _)) = rows.first().and_then(|r| r.get(sv)) else {
                continue;
            };
            let bounded: Vec<Hb> = rows
                .iter()
                .flat_map(|r| r.get(sv).map_or_else(Vec::new, |s| s.2.clone()))
                .collect();
            let euclid = bounded
                .iter()
                .map(|(h, _)| Point3::new(h[0] / h[3], h[1] / h[3], h[2] / h[3]));
            let Some(bbox) = Extent3d::hull(euclid) else {
                continue;
            };
            out.push(Patch {
                lo: Point2::new(ua, va),
                hi: Point2::new(ub, vb),
                degrees: (p, q),
                net: bounded,
                bbox,
            });
        }
    }
    out
}

// ---------------------------------------------------------------- the bound

/// A piece of a patch: its uv rectangle, the Bernstein coefficients of `w (S - q)` and `w`
/// as `(D_x, D_y, D_z, w)` with a bound of each one's error (row `i` along `u`), and the
/// certified lower bound of `|S - q|^2` over it.
#[derive(Debug, Clone)]
struct Piece {
    lo: Point2,
    hi: Point2,
    degrees: (usize, usize),
    net: Vec<Hb>,
    lb_sq: f64,
}

const DEFECT_BERNSTEIN: &str = "a patch's Bernstein form could not be built";

impl Piece {
    /// `D = w P - q w`: each coordinate's error is its inputs' (`q` exact) and the rounding
    /// of the product and the difference, under `2 eps (|w P| + |q w|)`, rounded up.
    fn of_patch(patch: &Patch, q: Point3) -> Result<Self, Halt> {
        let qs = [q.x, q.y, q.z];
        let net = patch
            .net
            .iter()
            .map(|(h, e)| {
                let (w, ew) = (at(h, 3), at(e, 3));
                let mut out = ([0.0; 4], [0.0; 4]);
                for (k, &qk) in qs.iter().enumerate() {
                    let (pw, qw) = (at(h, k), qk * w);
                    let err = at(e, k) + qk.abs() * ew + 2.0 * f64::EPSILON * (pw.abs() + qw.abs());
                    if let (Some(slot), Some(se)) = (out.0.get_mut(k), out.1.get_mut(k)) {
                        *slot = pw - qw;
                        *se = up(err);
                    }
                }
                if let (Some(slot), Some(se)) = (out.0.get_mut(3), out.1.get_mut(3)) {
                    *slot = w;
                    *se = ew;
                }
                out
            })
            .collect();
        Self::new(patch.lo, patch.hi, patch.degrees, net)
    }

    fn new(lo: Point2, hi: Point2, degrees: (usize, usize), net: Vec<Hb>) -> Result<Self, Halt> {
        let (du, dv) = degrees;
        let form = |k: usize| -> Result<Bounded, Halt> {
            let c = net.iter().map(|(h, _)| at(h, k)).collect();
            let err = net.iter().fold(0.0, |m: f64, (_, e)| m.max(at(e, k)));
            Bernstein2::new(du, dv, c)
                .map(|f| Bounded { f, err })
                .map_err(|_| Halt::Defect(DEFECT_BERNSTEIN))
        };
        let d = [form(0)?, form(1)?, form(2)?];
        let quotient =
            lower_bound_sq(&d, &form(3)?).map_err(|()| Halt::Defect(DEFECT_BERNSTEIN))?;
        let mut piece = Self {
            lo,
            hi,
            degrees,
            net,
            lb_sq: 0.0,
        };
        piece.lb_sq = quotient.max(piece.hull_bound_sq());
        Ok(piece)
    }

    /// The Euclidean `S - q` of coefficient `k`, `D_k / w_k`.
    fn rel(&self, k: usize) -> Vec3 {
        let h = self.net.get(k).map_or([f64::NAN; 4], |c| c.0);
        Vec3::new(h[0] / h[3], h[1] / h[3], h[2] / h[3])
    }

    /// The index of coefficient `(i, j)`.
    const fn index(&self, i: usize, j: usize) -> usize {
        i * (self.degrees.1 + 1) + j
    }

    /// The squared distance at each corner and its `uv`: the corner coefficients are surface
    /// points (to rounding, far under `eps`).
    fn corners(&self) -> [(f64, Point2); 4] {
        let (du, dv) = self.degrees;
        let sq = |i, j| self.rel(self.index(i, j)).length_squared();
        [
            (sq(0, 0), self.lo),
            (sq(du, 0), Point2::new(self.hi.x, self.lo.y)),
            (sq(0, dv), Point2::new(self.lo.x, self.hi.y)),
            (sq(du, dv), self.hi),
        ]
    }

    /// The length of the control polygon along each parameter, longest over the other's
    /// rows, of the Euclidean points `D_k / w_k` (positive weights).
    fn extents(&self) -> (f64, f64) {
        let (du, dv) = self.degrees;
        let e = |i, j| self.rel(self.index(i, j));
        let along_u = (0..=dv)
            .map(|j| {
                (0..du)
                    .map(|i| (e(i + 1, j) - e(i, j)).length())
                    .sum::<f64>()
            })
            .fold(0.0, f64::max);
        let along_v = (0..=du)
            .map(|i| {
                (0..dv)
                    .map(|j| (e(i, j + 1) - e(i, j)).length())
                    .sum::<f64>()
            })
            .fold(0.0, f64::max);
        (along_u, along_v)
    }

    /// The square of `min_k (D_k . n) / w_k` for the unit `n` toward the mean of the
    /// corners' `S - q`, each term taken at the side of its error bound that lowers it,
    /// rounded down; 0 where a term is not positive (no bound).
    ///
    /// `S - q = sum_k D_k B_k / sum_k w_k B_k`, so `(S - q) . n` is a mean of the
    /// `(D_k . n) / w_k` with the positive weights `w_k B_k`, and `|S - q| >= (S - q) . n`.
    /// It takes no product of the forms and each coefficient carries its own error, so a
    /// tiny weight, whose coefficients are tiny against the largest, leaves it sharp.
    fn hull_bound_sq(&self) -> f64 {
        let (du, dv) = self.degrees;
        let sum = [(0, 0), (du, 0), (0, dv), (du, dv)]
            .into_iter()
            .fold(Vec3::new(0.0, 0.0, 0.0), |s, (i, j)| {
                s + self.rel(self.index(i, j))
            });
        let len = sum.length();
        if !(len > 0.0 && len.is_finite()) {
            return 0.0;
        }
        // `|n| <= 1` whatever the rounding of the length and the quotients.
        let n = sum * (1.0 / (len * (1.0 + 4.0 * f64::EPSILON)));
        let mut least = f64::INFINITY;
        for (h, e) in &self.net {
            let terms = [h[0] * n.x, h[1] * n.y, h[2] * n.z];
            let dot = terms[0] + terms[1] + terms[2];
            let size = terms[0].abs() + terms[1].abs() + terms[2].abs();
            // `|err . n| <= sum err` for `|n| <= 1`; the dot's rounding is under `4 eps size`.
            let slack = up(e[0] + e[1] + e[2] + 4.0 * f64::EPSILON * size);
            let num = (dot - slack) * (1.0 - 2.0 * f64::EPSILON);
            let den = up(h[3] + e[3]);
            if !(num > 0.0 && den > 0.0 && den.is_finite()) {
                return 0.0;
            }
            least = least.min(num / den);
        }
        if !least.is_finite() {
            return 0.0;
        }
        let lb = least * (1.0 - 2.0 * f64::EPSILON);
        lb * lb * (1.0 - 4.0 * f64::EPSILON)
    }

    /// The piece's two edges at its ends in `u` (`along_u`) or `v`, each of degree 0 there.
    fn edges(&self, along_u: bool) -> Result<[Self; 2], Halt> {
        let (du, dv) = self.degrees;
        let line = |end: usize| -> Vec<Hb> {
            if along_u {
                (0..=dv)
                    .filter_map(|j| self.net.get(self.index(end, j)).copied())
                    .collect()
            } else {
                (0..=du)
                    .filter_map(|i| self.net.get(self.index(i, end)).copied())
                    .collect()
            }
        };
        let (deg, (lo, hi)) = if along_u {
            (
                (0, dv),
                (
                    Point2::new(self.hi.x, self.lo.y),
                    Point2::new(self.lo.x, self.hi.y),
                ),
            )
        } else {
            (
                (du, 0),
                (
                    Point2::new(self.lo.x, self.hi.y),
                    Point2::new(self.hi.x, self.lo.y),
                ),
            )
        };
        let end = if along_u { du } else { dv };
        Ok([
            Self::new(self.lo, hi, deg, line(0))?,
            Self::new(lo, self.hi, deg, line(end))?,
        ])
    }

    /// The piece halved in the parameter along which its control polygon is more than twice
    /// as long as along the other, else quartered. A parameter whose interval holds no
    /// floating point value inside is not halved: the piece is its two edges in that
    /// parameter instead, the only `uv`s floating point holds there (a knot span of one
    /// ulp, whose patch can be long). A piece that is a point gives none.
    fn split(&self) -> Result<Vec<Self>, Halt> {
        let m = Point2::new(0.5 * (self.lo.x + self.hi.x), 0.5 * (self.lo.y + self.hi.y));
        let mut su = self.lo.x < m.x && m.x < self.hi.x;
        let mut sv = self.lo.y < m.y && m.y < self.hi.y;
        if !su && self.lo.x < self.hi.x {
            return Ok(self.edges(true)?.into());
        }
        if !sv && self.lo.y < self.hi.y {
            return Ok(self.edges(false)?.into());
        }
        let (lu, lv) = self.extents();
        if su && sv {
            if lu > 2.0 * lv {
                sv = false;
            } else if lv > 2.0 * lu {
                su = false;
            }
        }
        if !(su || sv) {
            return Ok(Vec::new());
        }
        let mut parts = vec![(self.lo, self.hi, self.net.clone())];
        if su {
            parts = parts
                .into_iter()
                .flat_map(|(lo, hi, net)| {
                    let (a, b) = halved(&net, self.degrees, true);
                    [
                        (lo, Point2::new(m.x, hi.y), a),
                        (Point2::new(m.x, lo.y), hi, b),
                    ]
                })
                .collect();
        }
        if sv {
            parts = parts
                .into_iter()
                .flat_map(|(lo, hi, net)| {
                    let (a, b) = halved(&net, self.degrees, false);
                    [
                        (lo, Point2::new(hi.x, m.y), a),
                        (Point2::new(lo.x, m.y), hi, b),
                    ]
                })
                .collect();
        }
        parts
            .into_iter()
            .map(|(lo, hi, net)| Self::new(lo, hi, self.degrees, net))
            .collect()
    }
}

/// A net of degrees `(du, dv)` halved in `u` (`along_u`) or in `v`.
pub(crate) fn halved(net: &[Hb], (du, dv): (usize, usize), along_u: bool) -> (Vec<Hb>, Vec<Hb>) {
    let (mut lower, mut upper) = (net.to_vec(), net.to_vec());
    let (lines, len) = if along_u {
        (dv + 1, du + 1)
    } else {
        (du + 1, dv + 1)
    };
    for line in 0..lines {
        let idx = |t: usize| {
            if along_u {
                t * (dv + 1) + line
            } else {
                line * (dv + 1) + t
            }
        };
        let c: Vec<Hb> = (0..len).filter_map(|t| net.get(idx(t)).copied()).collect();
        let (a, b) = halves(&c);
        for (t, (x, y)) in a.into_iter().zip(b).enumerate() {
            if let (Some(l), Some(u)) = (lower.get_mut(idx(t)), upper.get_mut(idx(t))) {
                *l = x;
                *u = y;
            }
        }
    }
    (lower, upper)
}

/// `min_k N_k / W_k` of `N = D . D`, `W = w^2`, each coefficient taken at the side of its
/// error bound that lowers the quotient, the quotient rounded down; 0 where a form is not
/// finite (no bound).
fn lower_bound_sq(d: &[Bounded; 3], w: &Bounded) -> Result<f64, ()> {
    let mut n = d[0].mul(&d[0]).map_err(|_| ())?;
    for c in d.iter().skip(1) {
        n = n.add(&c.mul(c).map_err(|_| ())?).map_err(|_| ())?;
    }
    let ww = w.mul(w).map_err(|_| ())?;
    if !(n.is_finite() && ww.is_finite()) {
        return Ok(0.0);
    }
    let lb =
        n.f.coefficients()
            .iter()
            .zip(ww.f.coefficients())
            .map(|(&nk, &wk)| (nk - n.err).max(0.0) / (wk + ww.err))
            .fold(f64::INFINITY, f64::min);
    Ok((lb * (1.0 - 4.0 * f64::EPSILON)).max(0.0))
}

// ---------------------------------------------------------------- the tree

/// A tree node's item: its first patch and the least bound of its patches.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Item {
    patch: usize,
    lb_sq: f64,
}

/// A balanced tree over the patches in order, each node bounding its patches.
fn tree(patches: &[Patch], pieces: &[Piece]) -> Result<SplitTree<Item>, Halt> {
    fn build(
        t: &mut SplitTree<Item>,
        patches: &[Patch],
        pieces: &[Piece],
        lo: usize,
        hi: usize,
    ) -> Option<(NodeId, Extent3d, f64)> {
        if hi - lo == 1 {
            let (patch, piece) = (patches.get(lo)?, pieces.get(lo)?);
            let item = Item {
                patch: lo,
                lb_sq: piece.lb_sq,
            };
            return Some((t.leaf(patch.bbox, item)?, patch.bbox, piece.lb_sq));
        }
        let mid = lo + (hi - lo) / 2;
        let (a, ba, la) = build(t, patches, pieces, lo, mid)?;
        let (b, bb, lb) = build(t, patches, pieces, mid, hi)?;
        let (bbox, lb_sq) = (ba.union(bb), la.min(lb));
        let id = t.split(bbox, Item { patch: lo, lb_sq }, [a, b])?;
        Some((id, bbox, lb_sq))
    }
    let mut t = SplitTree::with_capacity(2 * patches.len());
    if patches.is_empty() {
        return Err(Halt::Defect("the surface has no patch"));
    }
    build(&mut t, patches, pieces, 0, patches.len())
        .ok_or(Halt::Defect("the patch tree could not be built"))?;
    Ok(t)
}

/// The solve keeps no solutions: the hooks hold the incumbent.
#[derive(Debug, Clone, Copy, PartialEq)]
struct NoObject;

impl SolutionObject for NoObject {
    fn parameter_count(&self) -> Option<usize> {
        Some(2)
    }
    fn parameter_interval(&self, _axis: usize) -> Option<Extent1d> {
        None
    }
    fn tolerance(&self) -> f64 {
        0.0
    }
}

// ---------------------------------------------------------------- the search

/// A piece waiting in a leaf's refinement, ordered so the heap pops the least bound first
/// (then the earliest pushed).
struct Queued {
    seq: u64,
    piece: Piece,
}

impl PartialEq for Queued {
    fn eq(&self, other: &Self) -> bool {
        self.cmp(other) == Ordering::Equal
    }
}

impl Eq for Queued {}

impl PartialOrd for Queued {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Queued {
    fn cmp(&self, other: &Self) -> Ordering {
        // reversed: `BinaryHeap` pops the greatest
        other
            .piece
            .lb_sq
            .total_cmp(&self.piece.lb_sq)
            .then(other.seq.cmp(&self.seq))
    }
}

struct Search<'a> {
    surface: &'a NurbsSurface<3>,
    domain: Extent2d,
    q: Point3,
    /// The incumbent: the least squared distance found, and where.
    best_sq: f64,
    best_uv: Point2,
    /// The certified gap.
    eps: f64,
    stop: &'a Stop,
    seq: u64,
    /// Each patch's whole piece, taken when its leaf is solved.
    leaves: Vec<Option<Piece>>,
}

impl Search<'_> {
    /// Charges one unit to `budget` and polls the stop flag.
    fn tick(&self, budget: &mut Budget) -> Result<(), Halt> {
        budget.charge(1).map_err(Halt::Exhausted)?;
        self.stop.check().map_err(|_| Halt::Stopped)
    }

    /// The squared distance a region must beat to matter: `(best - eps)^2`, 0 once the
    /// incumbent is within `eps` of the query.
    fn target_sq(&self) -> f64 {
        let t = self.best_sq.sqrt() - self.eps;
        if t > 0.0 { t * t } else { 0.0 }
    }

    /// Whether a region bounded below by `lb_sq` may hold a nearer point.
    fn open(&self, lb_sq: f64) -> bool {
        lb_sq < self.target_sq()
    }

    fn take(&mut self, sq: f64, uv: Point2) {
        if sq < self.best_sq {
            self.best_sq = sq;
            self.best_uv = uv;
        }
    }

    fn take_corners(&mut self, piece: &Piece) {
        for (sq, uv) in piece.corners() {
            self.take(sq, uv);
        }
    }

    /// The squared distance at `uv`, from the surface.
    fn eval_sq(&self, uv: Point2) -> Option<f64> {
        let s = self.surface.point(uv.to_array(), ABOVE).ok()?;
        let r = Vec3::new(s[0] - self.q.x, s[1] - self.q.y, s[2] - self.q.z);
        Some(r.length_squared())
    }

    /// Projected Newton on `|S - q|^2 / 2` from `start`, keeping only decreases; the
    /// incumbent takes every point it reaches.
    fn newton(&mut self, start: Point2, budget: &mut Budget) -> Result<(), Halt> {
        let mut uv = self.domain.clamp_point(start);
        for _ in 0..NEWTON_STEPS {
            self.tick(budget)?;
            let Ok(j) = self.surface.jet::<2>(uv.to_array(), ABOVE) else {
                return Ok(());
            };
            let v = |i, k| {
                j.get(i, k)
                    .map_or(Vec3::splat(f64::NAN), |a| Vec3::new(a[0], a[1], a[2]))
            };
            let s = v(0, 0);
            let r = s - Vec3::new(self.q.x, self.q.y, self.q.z);
            let f = r.length_squared();
            self.take(f, uv);
            let (su, sv) = (v(1, 0), v(0, 1));
            let (g0, g1) = (su.dot(r), sv.dot(r));
            let (ga, gb, gc) = (su.dot(su), su.dot(sv), sv.dot(sv));
            let (ha, hb, hc) = (
                ga + v(2, 0).dot(r),
                gb + v(1, 1).dot(r),
                gc + v(0, 2).dot(r),
            );
            // A parameter at its bound with the descent pointing out is held there (the
            // active set), and the step is the one-dimensional Newton in the other.
            let (lo, hi) = (self.domain.min(), self.domain.max());
            let held =
                |t: f64, g: f64, lo: f64, hi: f64| (t <= lo && g > 0.0) || (t >= hi && g < 0.0);
            let one = |h: f64, g2: f64, g: f64| {
                if h > 0.0 {
                    Some(-g / h)
                } else {
                    (g2 > 0.0).then(|| -g / g2)
                }
            };
            let along_v = one(hc, gc, g1).map(|s| Point2::new(0.0, s));
            let along_u = one(ha, ga, g0).map(|s| Point2::new(s, 0.0));
            // Where the full step finds no decrease (a crease at a knot, where the one-sided
            // partials do not see across), the steps that hold one parameter.
            let steps = match (held(uv.x, g0, lo.x, hi.x), held(uv.y, g1, lo.y, hi.y)) {
                (true, true) => [None, None, None],
                (true, false) => [along_v, None, None],
                (false, true) => [along_u, None, None],
                (false, false) => {
                    let solve = |a: f64, b: f64, c: f64| {
                        let det = a * c - b * b;
                        (a > 0.0 && det > 1e-14 * a * c).then(|| {
                            Point2::new(-(c * g0 - b * g1) / det, -(a * g1 - b * g0) / det)
                        })
                    };
                    let full = solve(ha, hb, hc).or_else(|| solve(ga, gb, gc)).or_else(|| {
                        let s = ga + gc;
                        (s > 0.0).then(|| Point2::new(-g0 / s, -g1 / s))
                    });
                    [full, along_v, along_u]
                }
            };
            let mut moved = None;
            for step in steps.into_iter().flatten() {
                if !(step.x.is_finite() && step.y.is_finite()) {
                    continue;
                }
                moved = self.line_search(uv, step, f, budget)?;
                if moved.is_some() {
                    break;
                }
            }
            let Some((next, fn_)) = moved else {
                return Ok(());
            };
            self.take(fn_, next);
            let size = 1.0 + uv.x.abs().max(uv.y.abs());
            let small =
                (next.x - uv.x).abs().max((next.y - uv.y).abs()) <= 4.0 * f64::EPSILON * size;
            uv = next;
            if small {
                return Ok(());
            }
        }
        Ok(())
    }

    /// The first of `uv + step`, `uv + step / 2`, ... (clamped to the domain, at most
    /// [`HALVINGS`] halvings) whose squared distance is below `f`, and that distance.
    fn line_search(
        &mut self,
        uv: Point2,
        step: Point2,
        f: f64,
        budget: &mut Budget,
    ) -> Result<Option<(Point2, f64)>, Halt> {
        let mut t = 1.0;
        for _ in 0..HALVINGS {
            let next = self
                .domain
                .clamp_point(Point2::new(uv.x + t * step.x, uv.y + t * step.y));
            if next == uv {
                return Ok(None);
            }
            self.tick(budget)?;
            if let Some(fn_) = self.eval_sq(next).filter(|&x| x < f) {
                return Ok(Some((next, fn_)));
            }
            t *= 0.5;
        }
        Ok(None)
    }

    /// Refines one patch best first until no piece of it may beat the incumbent by `eps`.
    fn refine(&mut self, piece: Piece, budget: &mut Budget) -> Result<(), Halt> {
        let mut heap = BinaryHeap::new();
        heap.push(Queued { seq: 0, piece });
        while let Some(Queued { piece, .. }) = heap.pop() {
            if !self.open(piece.lb_sq) {
                break; // the least bound left: every other is no lower
            }
            self.tick(budget)?;
            for (sq, uv) in piece.corners() {
                if sq < self.best_sq {
                    let gain = self.best_sq.sqrt() - sq.sqrt();
                    self.take(sq, uv);
                    if gain > self.eps {
                        self.newton(uv, budget)?;
                    }
                }
            }
            if !self.open(piece.lb_sq) {
                continue;
            }
            for child in piece.split()? {
                if self.open(child.lb_sq) {
                    self.seq += 1;
                    heap.push(Queued {
                        seq: self.seq,
                        piece: child,
                    });
                }
            }
        }
        Ok(())
    }
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
        let piece = self
            .leaves
            .get_mut(node.item().patch)
            .and_then(Option::take)
            .ok_or(SolveError::Eval(Halt::Defect("a patch was solved twice")))?;
        self.refine(piece, budget).map_err(SolveError::Eval)?;
        Ok(Next::Done)
    }

    fn may_contain(
        &mut self,
        branch: &Branch<'_, Item, 1>,
        _found: &Accumulator<NoObject>,
    ) -> Result<bool, SolveError<Halt>> {
        let [node] = branch.nodes();
        Ok(self.open(node.item().lb_sq))
    }

    fn split_choice(
        &self,
        branch: &Branch<'_, Item, 1>,
        _found: &Accumulator<NoObject>,
    ) -> Result<Split, SolveError<Halt>> {
        let [tree] = branch.trees();
        let [node] = branch.nodes();
        let bound = |id| tree.get(id).map_or(f64::INFINITY, |n| n.item().lb_sq);
        let [a, b] = node
            .children()
            .ok_or(SolveError::Eval(Halt::Defect("a leaf was split")))?;
        let order = if bound(b) < bound(a) {
            ChildOrder::Swapped
        } else {
            ChildOrder::Given
        };
        Ok(Split { tree: 0, order })
    }
}
