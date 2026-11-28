// Written for solidcorex. Apache-2.0.
//! The march (research 73 §4.5, row SM7): NURBS against NURBS, from the survey's seeds and
//! nodes to [`Branch`]es.
//!
//! # The step
//!
//! From a point `x` of the intersection (the middle of `S_A(u, v)` and `S_B(s, t)`), the
//! predictor moves `h` along the tangent `T = n_A x n_B / |n_A x n_B|`. The corrector is
//! Newton on the four unknowns `(u, v, s, t)` of
//!
//! ```text
//! S_A(u, v) - S_B(s, t) = 0,    (m - x0) . T = 0,
//! ```
//!
//! `m` the middle of the two points and `x0 = x + h T`: the plane across the curve at the
//! step. It is converged when the gap `|S_A - S_B|` is within a quarter of the coincidence
//! times `sin(angle(n_A, n_B))` (the conditioning bound: the point is then within a quarter
//! of the coincidence of the curve) or within the rounding of the points' size, and the plane
//! within a quarter of the coincidence. It iterates while each correction is at most half the
//! last after the first two, with no fixed count: a correction that stops contracting fails
//! the step.
//!
//! `h` is the least of three analytic bounds at `x` (73 §4.5), from the curve's curvature
//! `kappa`: the normal curvatures `k_A`, `k_B` of the two surfaces along `T` give the
//! curvature vector `k = a n_A + b n_B` with `k . n_A = k_A`, `k . n_B = k_B`:
//!
//! 1. turn: `h kappa <= THETA_MAX`;
//! 2. chord deviation: `kappa h^2 / 8 <= tol`, the coincidence;
//! 3. a tenth of the extents' size (a plane against a plane has no curvature to bound it);
//!
//! and at most twice the last step taken. A step is rejected and halved when the corrector
//! fails, when it travels less than a quarter or more than twice `h`, or when the tangent
//! turns by more than `2 THETA_MAX`; below the resolution the march is
//! [`SsiError::Stalled`] there. One unit of the budget is charged per step tried, and the
//! stop polled after it.
//!
//! # Where an arc ends (linking by identity)
//!
//! Every curve of the intersection within the extents meets a seed (73 §4.4): it ends on the
//! extents' rim at a seed, or crosses a piece edge or a ray at one. The seeds are the
//! survey's, in its order, kept once (`Seed::same_as`). Each step's chord is matched against
//! them: a seed within four times the coincidence of the chord, with its uv between the
//! chord's ends on both surfaces, is on it and is used. An arc ends
//!
//! - at the extents' rim: the step whose prediction leaves an extent is solved on that edge
//!   (the fourth equation fixing the coordinate) and must meet a seed there, or the march is
//!   `Stalled` (it has left the curve it was on);
//! - at a periodic seam (an extent edge whose two sides are one curve of points): the arc is
//!   cut at a [`NodeKind::Seam`] node and continues from the other side;
//! - at a node ([`CriticalKind::Node`](super::CriticalKind::Node)): when the node is within
//!   two steps ahead;
//! - at its start seed: a closed loop.
//!
//! A used seed met again (other than the one just passed and the start) is a curve traced
//! twice: `Stalled` there, never a duplicate branch.
//!
//! # The order
//!
//! Arcs leave each node along the four null directions of the relative form (the difference
//! of the two second fundamental forms in the common tangent plane: two crossing branches),
//! node by node; then each unused rim seed, inward; then each unused seed left, which is on a
//! closed loop (or, if the march from it reaches an end, on an arc marched both ways and
//! joined). Each branch is oriented along `n_A x n_B` at its middle chord.
//!
//! # The answer
//!
//! A branch is degree 1 on chord length, as the field path's (row `SM6a`); row `SM8` refits and
//! certifies per span. Its [`Certificate`] holds, per surface, the points' residual plus the
//! chord bound `h^2 d2 / 8` with `d2` the surface's second partials bounded over each run of
//! spans, and, to the intersection, the points' first-order distance `gap / sin` plus each
//! span's sag estimate from its turn: an estimate, not row SM8's Krawczyk proof.

use std::sync::Arc;

use scx_curve::Curve2;
use scx_num::budget::{Budget, Stop};
use scx_num::certified::gamma;
use scx_num::extent2d::Extent2d;
use scx_num::interval::Ivl;
use scx_num::units::Length;
use scx_num::vector2d::{Point2, Vec2};
use scx_num::vector3d::{Point3, Vec3};
use scx_nurbs::{Approach, KnotVector, NurbsCurve};
use scx_nurbs_surface::About;
use scx_surface::Surface;

use super::loops::{Critical, Seed};
use super::net::up;
use super::traced::{Branch, Certificate, End, Ends, Node, NodeKind, OnSurface, Rim};
use crate::analytic::Contact;
use crate::error::SsiError;

/// The largest turn of the tangent over one step (radians), as the pin's (`max_turn`).
const THETA_MAX: f64 = 0.06;

/// `cos(2 THETA_MAX)`: a step whose tangent turns by more is rejected.
const COS_TURN: f64 = 0.992_808_635_853_866_3;

/// The extents' size over this is the longest step.
const LONGEST: f64 = 10.0;

/// A seed within this many coincidences of a step's chord is on it.
const NEAR: f64 = 4.0;

/// Spans per bound of the surfaces' second partials in the certificate.
const CHUNK: usize = 64;

/// The march's answer: branches and the nodes their ends name.
pub(crate) struct Marched {
    pub branches: Vec<Branch>,
    pub nodes: Vec<Node>,
}

/// One surface as the march reads it.
struct Side<'a> {
    s: &'a Surface,
    e: Extent2d,
    /// Whether the extent's `u` (then `v`) edges are one curve of points: a periodic seam.
    seam: [bool; 2],
}

impl<'a> Side<'a> {
    fn new(s: &'a Surface, e: &Extent2d, tol: f64) -> Self {
        Self {
            s,
            e: *e,
            seam: [seam(s, e, 0, tol), seam(s, e, 1, tol)],
        }
    }

    fn lo(&self, axis: usize) -> f64 {
        if axis == 0 {
            self.e.x_min()
        } else {
            self.e.y_min()
        }
    }

    fn hi(&self, axis: usize) -> f64 {
        if axis == 0 {
            self.e.x_max()
        } else {
            self.e.y_max()
        }
    }

    /// The second-order jet at `uv`, from the span inside the extent at its upper ends.
    fn eval(&self, uv: Point2) -> Option<Eval> {
        let side = [
            if uv.x >= self.e.x_max() {
                Approach::Below
            } else {
                Approach::Above
            },
            if uv.y >= self.e.y_max() {
                Approach::Below
            } else {
                Approach::Above
            },
        ];
        let j = self.s.jet::<2>(uv, side);
        let e = Eval {
            p: j.point().to_point(),
            su: j.get(1, 0)?,
            sv: j.get(0, 1)?,
            suu: j.get(2, 0)?,
            suv: j.get(1, 1)?,
            svv: j.get(0, 2)?,
        };
        let ok = e.p.is_finite()
            && [e.su, e.sv, e.suu, e.suv, e.svv]
                .iter()
                .all(|v| v.is_finite());
        ok.then_some(e)
    }
}

/// Whether the extent's two edges across `axis` are one curve of points within `tol`.
fn seam(s: &Surface, e: &Extent2d, axis: usize, tol: f64) -> bool {
    (0..=4).all(|i| {
        let f = f64::from(i) / 4.0;
        let (lo, hi) = if axis == 0 {
            let v = e.y_min() + (e.y_max() - e.y_min()) * f;
            (Point2::new(e.x_min(), v), Point2::new(e.x_max(), v))
        } else {
            let u = e.x_min() + (e.x_max() - e.x_min()) * f;
            (Point2::new(u, e.y_min()), Point2::new(u, e.y_max()))
        };
        (s.point(lo) - s.point(hi)).length() <= tol
    })
}

#[derive(Clone, Copy, Debug)]
struct Eval {
    p: Point3,
    su: Vec3,
    sv: Vec3,
    suu: Vec3,
    suv: Vec3,
    svv: Vec3,
}

impl Eval {
    /// The uv velocity of the unit tangent `t` (least squares on `[S_u S_v]`).
    fn velocity(&self, t: Vec3) -> Option<Vec2> {
        let (a, b, c) = (
            self.su.dot(self.su),
            self.su.dot(self.sv),
            self.sv.dot(self.sv),
        );
        let (r, s) = (self.su.dot(t), self.sv.dot(t));
        let det = a * c - b * b;
        (det > 0.0 && det.is_finite())
            .then(|| Vec2::new((c * r - b * s) / det, (a * s - b * r) / det))
    }

    /// The second derivative along the uv direction `w`.
    fn second(&self, w: Vec2) -> Vec3 {
        self.suu * (w.x * w.x) + self.suv * (2.0 * w.x * w.y) + self.svv * (w.y * w.y)
    }

    fn normal(&self) -> Option<Vec3> {
        self.su.cross(self.sv).normalized().map(Vec3::from)
    }
}

/// A point of the intersection: the two surfaces' uv and jets.
#[derive(Clone, Copy, Debug)]
struct State {
    uv: [Point2; 2],
    j: [Eval; 2],
    /// The middle of the two points.
    p: Point3,
    /// `|S_A - S_B|`.
    gap: f64,
}

impl State {
    fn new(uv: [Point2; 2], j: &[Eval; 2]) -> Self {
        let [ja, jb] = *j;
        Self {
            uv,
            j: *j,
            p: ja.p.midpoint(jb.p),
            gap: (ja.p - jb.p).length(),
        }
    }
}

/// The curve's local frame at a [`State`].
#[derive(Clone, Copy, Debug)]
struct Frame {
    /// `n_A x n_B`, unit.
    t: Vec3,
    /// `|n_A x n_B|`.
    sin: f64,
    /// The curvature.
    kappa: f64,
    /// The uv velocities of `t`.
    vel: [Vec2; 2],
}

fn frame(st: &State) -> Option<Frame> {
    let [ja, jb] = st.j;
    let (na, nb) = (ja.normal()?, jb.normal()?);
    let c = na.cross(nb);
    let sin = c.length();
    if sin.partial_cmp(&1e-12) != Some(core::cmp::Ordering::Greater) {
        return None;
    }
    let t = c * (1.0 / sin);
    let (va, vb) = (ja.velocity(t)?, jb.velocity(t)?);
    let (ka, kb) = (na.dot(ja.second(va)), nb.dot(jb.second(vb)));
    // |k|^2 = (a + b)^2 (1 + c) / 2 + (a - b)^2 (1 - c) / 2 with a + b = (k_A + k_B) / (1 + c)
    // and a - b = (k_A - k_B) (1 + c) / sin^2: no cancellation as the normals close up
    // near a node (`n_B` taken as `-n_B` when they point apart).
    let (kb, cos) = if na.dot(nb) < 0.0 {
        (-kb, -na.dot(nb))
    } else {
        (kb, na.dot(nb))
    };
    let (sum, diff) = (ka + kb, ka - kb);
    let k2 = sum * sum / (2.0 * (1.0 + cos)) + diff * diff * (1.0 + cos) / (2.0 * sin * sin);
    let kappa = k2.max(0.0).sqrt();
    kappa.is_finite().then_some(Frame {
        t,
        sin,
        kappa,
        vel: [va, vb],
    })
}

/// The corrector's fourth equation.
#[derive(Clone, Copy, Debug)]
enum Fix {
    /// `(m - x0) . n = 0`.
    Plane { x0: Point3, n: Vec3 },
    /// Coordinate `axis` of surface `k` is `at`.
    Edge { k: usize, axis: usize, at: f64 },
}

enum Corr {
    Done(Box<State>),
    /// An iterate left surface `k`'s extent across `axis`, at the edge `at`.
    Left {
        k: usize,
        axis: usize,
        at: f64,
        uv: [Point2; 2],
    },
    Failed,
}

/// `x` of the 4 x 4 system `[m | r]`, by Gaussian elimination with partial pivoting.
fn solve4(mut m: [[f64; 5]; 4]) -> Option<[f64; 4]> {
    for c in 0..4 {
        let (pr, _) = m
            .iter()
            .enumerate()
            .skip(c)
            .map(|(r, row)| (r, row.get(c).map_or(0.0, |x| x.abs())))
            .fold(None, |best: Option<(usize, f64)>, (r, x)| match best {
                Some((_, b)) if b >= x => best,
                _ => Some((r, x)),
            })?;
        m.swap(c, pr);
        let (top, rest) = m.split_at_mut(c + 1);
        let piv = top.last()?;
        let p = *piv.get(c)?;
        if !(p.abs() > 0.0 && p.is_finite()) {
            return None;
        }
        for row in rest.iter_mut() {
            let f = *row.get(c)? / p;
            for (x, y) in row.iter_mut().zip(piv.iter()) {
                *x -= f * y;
            }
        }
    }
    let mut x = [0.0; 4];
    for c in (0..4).rev() {
        let row = m.get(c)?;
        let mut s = *row.get(4)?;
        for (j, xj) in x.iter().enumerate().skip(c + 1) {
            s -= row.get(j)? * xj;
        }
        *x.get_mut(c)? = s / row.get(c)?;
    }
    x.iter().all(|v| v.is_finite()).then_some(x)
}

fn coord(uv: &[Point2; 2], k: usize, axis: usize) -> f64 {
    let [a, b] = uv;
    let p = if k == 0 { a } else { b };
    if axis == 0 { p.x } else { p.y }
}

fn set_coord(uv: &mut [Point2; 2], k: usize, axis: usize, x: f64) {
    let [a, b] = uv;
    let p = if k == 0 { a } else { b };
    if axis == 0 {
        p.x = x;
    } else {
        p.y = x;
    }
}

/// A vertex of an arc.
#[derive(Clone, Copy, Debug)]
struct Vert {
    p: Point3,
    uv: [Point2; 2],
    /// The distance to the intersection (an estimate: `gap / sin`, or the coincidence at a
    /// proved point).
    err: f64,
    /// The unit tangent (oriented along the march), or zero at a node.
    t: Vec3,
}

/// How an arc ends.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Cut {
    Rim(Rim),
    Node(usize),
    /// The seed the march started from (an interior one), to be joined.
    Start,
}

struct Run {
    verts: Vec<Vert>,
    start: Cut,
    end: Cut,
}

/// A seed as the march uses it.
struct SeedRec {
    point: Point3,
    uv: [Point2; 2],
    rim: Option<Rim>,
    used: bool,
}

/// A node the survey proved: the four directions its branches leave along.
struct NodeRec {
    point: Point3,
    uv: [Point2; 2],
    slots: [(Vec3, bool); 4],
}

/// The marcher's state across arcs.
struct March<'a, 'b> {
    sides: [Side<'a>; 2],
    tol: f64,
    res: f64,
    hmax: f64,
    seeds: Vec<SeedRec>,
    /// Seed indices by x.
    by_x: Vec<usize>,
    nodes: Vec<NodeRec>,
    /// Seam nodes made, after the crossing nodes.
    seams: Vec<Node>,
    budget: &'b mut Budget,
    stop: &'b Stop,
}

fn stalled(at: Point3, h: f64) -> SsiError {
    SsiError::Stalled {
        at,
        step: Length::from_mm(h.max(0.0)),
    }
}

/// Branches through `seeds` and `nodes` (in the survey's order) of `surfaces` over
/// `extents`, at the coincidence `tol` and resolution `res` (module docs). With `swap`, the
/// answer is for the operands the other way round: pcurves and rims swapped and every
/// branch reversed, from the same points, so it is the same bits either way.
pub(crate) fn march(
    surfaces: [&Surface; 2],
    extents: [&Extent2d; 2],
    seeds: &[Seed],
    nodes: &[&Critical],
    tol: f64,
    res: f64,
    swap: bool,
    budget: &mut Budget,
    stop: &Stop,
) -> Result<Marched, SsiError> {
    let [sa, sb] = surfaces;
    let [ea, eb] = extents;
    let sides = [Side::new(sa, ea, tol), Side::new(sb, eb, tol)];
    let mut m = March {
        hmax: longest(&sides),
        sides,
        tol,
        res,
        seeds: Vec::new(),
        by_x: Vec::new(),
        nodes: Vec::new(),
        seams: Vec::new(),
        budget,
        stop,
    };
    for s in seeds {
        let st = m.state(s.uv).ok_or_else(|| stalled(s.point, 0.0))?;
        let rim = m.rim_of(&st);
        m.seeds.push(SeedRec {
            point: s.point,
            uv: s.uv,
            rim,
            used: false,
        });
    }
    let mut by_x: Vec<usize> = (0..m.seeds.len()).collect();
    by_x.sort_by(|&i, &j| {
        let x = |k: usize| m.seeds.get(k).map_or(0.0, |s| s.point.x);
        x(i).total_cmp(&x(j)).then(i.cmp(&j))
    });
    m.by_x = by_x;
    for c in nodes {
        let st = m.state(c.uv).ok_or_else(|| stalled(c.point, 0.0))?;
        let dirs = null_directions(&st).ok_or_else(|| stalled(c.point, 0.0))?;
        m.nodes.push(NodeRec {
            point: c.point,
            uv: c.uv,
            slots: dirs.map(|d| (d, false)),
        });
    }
    let mut runs = Vec::new();
    // Arcs from the nodes, slot by slot.
    for n in 0..m.nodes.len() {
        for slot in 0..4 {
            let Some(&(d, used)) = m.nodes.get(n).and_then(|r| r.slots.get(slot)) else {
                continue;
            };
            if used {
                continue;
            }
            m.use_slot(n, slot);
            runs.extend(m.leave_node(n, d)?);
        }
    }
    // Arcs from the rim, inward.
    for i in 0..m.seeds.len() {
        let Some(s) = m.seeds.get(i) else { continue };
        let (Some(rim), false) = (s.rim, s.used) else {
            continue;
        };
        let (point, uv) = (s.point, s.uv);
        m.use_seed(i);
        let st = m.state(uv).ok_or_else(|| stalled(point, 0.0))?;
        if let Some(sign) = m.inward(&st, point)? {
            runs.extend(m.walk(st, sign, Cut::Rim(rim), Some(i), None)?);
        }
    }
    // The seeds left: closed loops (or arcs between nodes, marched both ways).
    for i in 0..m.seeds.len() {
        let Some(s) = m.seeds.get(i) else { continue };
        if s.used {
            continue;
        }
        let (point, uv) = (s.point, s.uv);
        m.use_seed(i);
        let st = m.state(uv).ok_or_else(|| stalled(point, 0.0))?;
        let fwd = m.walk(st, 1.0, Cut::Start, Some(i), None)?;
        if fwd.last().is_some_and(|r| r.end == Cut::Start) {
            runs.extend(close(fwd));
        } else {
            let bwd = m.walk(st, -1.0, Cut::Start, Some(i), None)?;
            if bwd.last().is_some_and(|r| r.end == Cut::Start) {
                return Err(stalled(point, 0.0));
            }
            runs.extend(join(bwd, fwd));
        }
    }
    let mut out_nodes: Vec<Node> = m
        .nodes
        .iter()
        .map(|n| Node {
            point: n.point,
            uv: n.uv,
            kind: NodeKind::Crossing,
        })
        .collect();
    let crossing = out_nodes.len();
    out_nodes.extend(m.seams.iter().copied());
    let mut branches = Vec::with_capacity(runs.len());
    for r in runs {
        if let Some(b) = m.branch(r, crossing, swap)? {
            branches.push(b);
        }
    }
    if swap {
        for n in &mut out_nodes {
            n.uv.swap(0, 1);
        }
    }
    Ok(Marched {
        branches,
        nodes: out_nodes,
    })
}

/// A tenth of the size of the extents' images (sampled), the longest step.
fn longest(sides: &[Side<'_>; 2]) -> f64 {
    let mut lo = [f64::INFINITY; 3];
    let mut hi = [f64::NEG_INFINITY; 3];
    for sd in sides {
        for i in 0..=4 {
            for j in 0..=4 {
                let (fu, fv) = (f64::from(i) / 4.0, f64::from(j) / 4.0);
                let uv = Point2::new(
                    sd.e.x_min() + (sd.e.x_max() - sd.e.x_min()) * fu,
                    sd.e.y_min() + (sd.e.y_max() - sd.e.y_min()) * fv,
                );
                let p = sd.s.point(uv).to_array();
                for ((l, h), x) in lo.iter_mut().zip(hi.iter_mut()).zip(p) {
                    *l = l.min(x);
                    *h = h.max(x);
                }
            }
        }
    }
    let d = lo.iter().zip(hi).fold(0.0_f64, |m, (l, h)| m.max(h - l));
    if d.is_finite() && d > 0.0 {
        d / LONGEST
    } else {
        1.0
    }
}

/// The four directions two branches leave a node along: the null directions of the
/// difference of the surfaces' second fundamental forms (both along `n_A`) in the common
/// tangent plane, each both ways.
fn null_directions(st: &State) -> Option<[Vec3; 4]> {
    let [ja, jb] = st.j;
    let n = ja.normal()?;
    let e1 = ja.su.normalized().map(Vec3::from)?;
    let e2 = n.cross(e1);
    let form = |j: &Eval, w: Vec3| -> Option<f64> { Some(n.dot(j.second(j.velocity(w)?))) };
    let w = (e1 + e2) * core::f64::consts::FRAC_1_SQRT_2;
    let (a1, a2, a12) = (form(&ja, e1)?, form(&ja, e2)?, form(&ja, w)?);
    let (b1, b2, b12) = (form(&jb, e1)?, form(&jb, e2)?, form(&jb, w)?);
    // D(x e1 + y e2) = a x^2 + 2 b x y + c y^2; D(w) = (a + 2 b + c) / 2.
    let a = a1 - b1;
    let c = a2 - b2;
    let b = (a12 - b12) - 0.5 * (a + c);
    let disc = b * b - a * c;
    if disc.partial_cmp(&0.0) != Some(core::cmp::Ordering::Greater) {
        return None;
    }
    let r = disc.sqrt();
    let dir = |x: f64, y: f64| (e1 * x + e2 * y).normalized().map(Vec3::from);
    let (d1, d2) = if a.abs() >= c.abs() {
        // a x^2 + 2 b x + c = 0 with y = 1, by the stable pair of roots.
        let q = -(b + b.signum() * r);
        (dir(q / a, 1.0)?, dir(c / q, 1.0)?)
    } else {
        let q = -(b + b.signum() * r);
        (dir(1.0, q / c)?, dir(1.0, a / q)?)
    };
    Some([d1, -d1, d2, -d2])
}

/// The runs of a closed march (its last run ends at its start): one closed run, or the last
/// joined to the first across the start.
fn close(mut runs: Vec<Run>) -> Vec<Run> {
    if runs.len() <= 1 {
        return runs;
    }
    let first = runs.remove(0);
    if let Some(last) = runs.last_mut() {
        last.verts.extend(first.verts);
        last.end = first.end;
    }
    runs
}

/// A march both ways from one seed, as runs in one direction: `bwd` reversed, then `fwd`,
/// the two joined at the seed.
fn join(bwd: Vec<Run>, fwd: Vec<Run>) -> Vec<Run> {
    let mut out: Vec<Run> = bwd
        .into_iter()
        .rev()
        .map(|r| {
            let mut verts = r.verts;
            verts.reverse();
            for v in &mut verts {
                v.t = -v.t;
            }
            Run {
                verts,
                start: r.end,
                end: r.start,
            }
        })
        .collect();
    let mut fwd = fwd.into_iter();
    if let (Some(last), Some(f0)) = (out.last_mut(), fwd.next()) {
        last.verts.extend(f0.verts.into_iter().skip(1));
        last.end = f0.end;
    }
    out.extend(fwd);
    out
}

impl March<'_, '_> {
    fn side(&self, k: usize) -> &Side<'_> {
        let [a, b] = &self.sides;
        if k == 0 { a } else { b }
    }

    fn state(&self, uv: [Point2; 2]) -> Option<State> {
        let [ua, ub] = uv;
        let ja = self.side(0).eval(ua)?;
        let jb = self.side(1).eval(ub)?;
        Some(State::new(uv, &[ja, jb]))
    }

    fn use_seed(&mut self, i: usize) {
        if let Some(s) = self.seeds.get_mut(i) {
            s.used = true;
        }
    }

    fn use_slot(&mut self, n: usize, slot: usize) {
        if let Some(s) = self.nodes.get_mut(n).and_then(|r| r.slots.get_mut(slot)) {
            s.1 = true;
        }
    }

    /// The uv distance on surface `k` along `axis` that is `tol` in 3D at `st`.
    fn uv_tol(&self, st: &State, k: usize, axis: usize) -> f64 {
        let [ja, jb] = &st.j;
        let j = if k == 0 { ja } else { jb };
        let d = if axis == 0 { j.su } else { j.sv };
        let len = d.length();
        if len > 0.0 {
            NEAR * self.tol / len
        } else {
            0.0
        }
    }

    /// Whether `st` is on surface `k`'s extent edge across `axis` (not a seam).
    fn on_edge(&self, st: &State, k: usize, axis: usize) -> Option<bool> {
        let sd = self.side(k);
        if sd.seam.get(axis).copied().unwrap_or(false) {
            return None;
        }
        let x = coord(&st.uv, k, axis);
        let e = self.uv_tol(st, k, axis);
        if (x - sd.lo(axis)).abs() <= e {
            Some(true)
        } else if (sd.hi(axis) - x).abs() <= e {
            Some(false)
        } else {
            None
        }
    }

    /// Which rims `st` is on.
    fn rim_of(&self, st: &State) -> Option<Rim> {
        let on = |k| (0..2).any(|axis| self.on_edge(st, k, axis).is_some());
        match (on(0), on(1)) {
            (true, true) => Some(Rim::Both),
            (true, false) => Some(Rim::A),
            (false, true) => Some(Rim::B),
            (false, false) => None,
        }
    }

    /// The sign of `n_A x n_B` that enters every extent the rim seed at `st` is on; `None`
    /// when it enters one and leaves another (the curve only grazes a corner).
    fn inward(&self, st: &State, at: Point3) -> Result<Option<f64>, SsiError> {
        let fr = frame(st).ok_or_else(|| stalled(at, 0.0))?;
        let (mut plus, mut minus) = (false, false);
        let [va, vb] = fr.vel;
        for (k, v) in [(0, va), (1, vb)] {
            for axis in 0..2 {
                let Some(low) = self.on_edge(st, k, axis) else {
                    continue;
                };
                let c = if axis == 0 { v.x } else { v.y };
                let c = if low { c } else { -c };
                // Along the edge within the rounding of the velocity: tangent to the rim.
                if c.abs() <= 1e-9 * (v.x.abs() + v.y.abs()) {
                    return Err(stalled(at, 0.0));
                }
                if c > 0.0 {
                    plus = true;
                } else {
                    minus = true;
                }
            }
        }
        Ok(match (plus, minus) {
            (true, false) => Some(1.0),
            (false, true) => Some(-1.0),
            _ => None,
        })
    }

    /// The step bound at a point (module docs).
    fn bound(&self, fr: &Frame) -> f64 {
        let k = fr.kappa;
        if k > 0.0 {
            self.hmax
                .min(THETA_MAX / k)
                .min((8.0 * self.tol / k).sqrt())
        } else {
            self.hmax
        }
    }

    /// Newton from `uv` on the system with `fix` (module docs).
    fn correct(&self, mut uv: [Point2; 2], fix: Fix) -> Corr {
        if let Fix::Edge { k, axis, at } = fix {
            set_coord(&mut uv, k, axis, at);
        }
        let mut last = f64::INFINITY;
        let mut iter = 0_usize;
        let mut tiny = false;
        loop {
            let Some(st) = self.state(uv) else {
                return Corr::Failed;
            };
            let [ja, jb] = st.j;
            let d = ja.p - jb.p;
            let reach = ja.p.max_dimension().max(jb.p.max_dimension());
            let floor = 16.0 * f64::EPSILON * reach;
            let sin = match (ja.normal(), jb.normal()) {
                (Some(a), Some(b)) => a.cross(b).length(),
                _ => return Corr::Failed,
            };
            let gtol = (0.25 * self.tol * sin).max(floor);
            let (res4, row4) = match fix {
                Fix::Plane { x0, n } => (
                    (st.p - x0).dot(n),
                    [
                        0.5 * ja.su.dot(n),
                        0.5 * ja.sv.dot(n),
                        0.5 * jb.su.dot(n),
                        0.5 * jb.sv.dot(n),
                    ],
                ),
                Fix::Edge { k, axis, .. } => {
                    let mut r = [0.0; 4];
                    if let Some(x) = r.get_mut(2 * k + axis) {
                        *x = 1.0;
                    }
                    (0.0, r)
                }
            };
            let fix_ok = match fix {
                Fix::Plane { .. } => res4.abs() <= 0.25 * self.tol,
                Fix::Edge { .. } => true,
            };
            if st.gap <= gtol && fix_ok {
                return Corr::Done(Box::new(st));
            }
            if tiny {
                // The corrections reached the rounding of the uv: done if the gap is within
                // the rounding of the points' size, else no root here.
                return if st.gap <= 64.0 * floor.max(f64::MIN_POSITIVE) && fix_ok {
                    Corr::Done(Box::new(st))
                } else {
                    Corr::Failed
                };
            }
            let row = |a: f64, b: f64, c: f64, e: f64, r: f64| [a, b, -c, -e, -r];
            let m = [
                row(ja.su.x, ja.sv.x, jb.su.x, jb.sv.x, d.x),
                row(ja.su.y, ja.sv.y, jb.su.y, jb.sv.y, d.y),
                row(ja.su.z, ja.sv.z, jb.su.z, jb.sv.z, d.z),
                {
                    let [a, b, c, e] = row4;
                    [a, b, c, e, -res4]
                },
            ];
            let Some([du, dv, ds, dt]) = solve4(m) else {
                return Corr::Failed;
            };
            let size = du.abs().max(dv.abs()).max(ds.abs()).max(dt.abs());
            if iter >= 2 && size > 0.5 * last {
                return Corr::Failed;
            }
            let [ua, ub] = uv;
            uv = [
                Point2::new(ua.x + du, ua.y + dv),
                Point2::new(ub.x + ds, ub.y + dt),
            ];
            let scale = ua.x.abs().max(ua.y.abs()).max(ub.x.abs()).max(ub.y.abs()) + 1.0;
            tiny = size <= 4.0 * f64::EPSILON * scale;
            match fix {
                Fix::Plane { .. } => {
                    if let Some(left) = self.left(&uv) {
                        return left;
                    }
                }
                Fix::Edge { k, axis, at } => {
                    uv = self.clamp(uv);
                    set_coord(&mut uv, k, axis, at);
                }
            }
            last = size;
            iter += 1;
        }
    }

    /// The first extent edge `uv` is past, as [`Corr::Left`], with `uv` clamped.
    fn left(&self, uv: &[Point2; 2]) -> Option<Corr> {
        for k in 0..2 {
            let sd = self.side(k);
            for axis in 0..2 {
                let x = coord(uv, k, axis);
                let at = if x < sd.lo(axis) {
                    sd.lo(axis)
                } else if x > sd.hi(axis) {
                    sd.hi(axis)
                } else {
                    continue;
                };
                return Some(Corr::Left {
                    k,
                    axis,
                    at,
                    uv: self.clamp(*uv),
                });
            }
        }
        None
    }

    fn clamp(&self, uv: [Point2; 2]) -> [Point2; 2] {
        let [a, b] = uv;
        let c = |p: Point2, sd: &Side<'_>| {
            Point2::new(
                p.x.clamp(sd.e.x_min(), sd.e.x_max()),
                p.y.clamp(sd.e.y_min(), sd.e.y_max()),
            )
        };
        [c(a, self.side(0)), c(b, self.side(1))]
    }

    fn vert(&self, st: &State, fr: Option<&Frame>, sign: f64) -> Vert {
        Vert {
            p: st.p,
            uv: st.uv,
            err: fr.map_or(self.tol, |f| st.gap / f.sin + 0.5 * st.gap),
            t: fr.map_or(Vec3::new(0.0, 0.0, 0.0), |f| f.t * sign),
        }
    }

    /// Whether `uv` is within the chord's uv box from `a` to `b` on both surfaces (on a seam
    /// axis, also when shifted by the period).
    fn uv_between(&self, uv: [Point2; 2], a: [Point2; 2], b: [Point2; 2], shift: bool) -> bool {
        let ok = |k: usize| {
            let sd = self.side(k);
            (0..2).all(|axis| {
                let (x0, x1) = (coord(&a, k, axis), coord(&b, k, axis));
                let x = coord(&uv, k, axis);
                let w = sd.hi(axis) - sd.lo(axis);
                let m = (x1 - x0).abs().max(1e-9 * w);
                let inside = |x: f64| x >= x0.min(x1) - m && x <= x0.max(x1) + m;
                inside(x)
                    || (shift
                        && sd.seam.get(axis).copied().unwrap_or(false)
                        && (inside(x + w) || inside(x - w)))
            })
        };
        ok(0) && ok(1)
    }

    /// The unused or used seeds on the chord from `a` to `b`, by their place along it.
    fn seeds_on(&self, a: &State, b: &State) -> Vec<(f64, usize)> {
        let r = NEAR * self.tol;
        let (x0, x1) = (a.p.x.min(b.p.x) - r, a.p.x.max(b.p.x) + r);
        let from = self
            .by_x
            .partition_point(|&i| self.seeds.get(i).is_some_and(|s| s.point.x < x0));
        let d = b.p - a.p;
        let dd = d.dot(d);
        let mut out = Vec::new();
        for &i in self.by_x.iter().skip(from) {
            let Some(s) = self.seeds.get(i) else { continue };
            if s.point.x > x1 {
                break;
            }
            let lam = if dd > 0.0 {
                ((s.point - a.p).dot(d) / dd).clamp(0.0, 1.0)
            } else {
                0.0
            };
            let q = a.p + d * lam;
            if (s.point - q).length() <= r && self.uv_between(s.uv, a.uv, b.uv, true) {
                out.push((lam, i));
            }
        }
        out.sort_by(|x, y| x.0.total_cmp(&y.0).then(x.1.cmp(&y.1)));
        out
    }

    /// The seam node at `p`, made if new.
    fn seam_node(&mut self, st: &State) -> usize {
        let at = self.nodes.len();
        if let Some(i) = self
            .seams
            .iter()
            .position(|n| (n.point - st.p).length() <= NEAR * self.tol)
        {
            return at + i;
        }
        self.seams.push(Node {
            point: st.p,
            uv: st.uv,
            kind: NodeKind::Seam,
        });
        at + self.seams.len() - 1
    }

    /// The arcs from node `n` along `d`.
    fn leave_node(&mut self, n: usize, d: Vec3) -> Result<Vec<Run>, SsiError> {
        let Some(node) = self.nodes.get(n) else {
            return Ok(Vec::new());
        };
        let (np, nuv) = (node.point, node.uv);
        let st = self.state(nuv).ok_or_else(|| stalled(np, 0.0))?;
        let mut h = self.hmax;
        let (q, sign) = loop {
            self.budget.charge(1)?;
            self.stop.check()?;
            if h < self.res {
                return Err(stalled(np, h));
            }
            let x0 = np + d * h;
            let [ja, jb] = st.j;
            let guess = match (ja.velocity(d), jb.velocity(d)) {
                (Some(va), Some(vb)) => {
                    let [ua, ub] = nuv;
                    [ua + va * h, ub + vb * h]
                }
                _ => return Err(stalled(np, h)),
            };
            if let Corr::Done(q) = self.correct(guess, Fix::Plane { x0, n: d })
                && let q = *q
                && let Some(fr) = frame(&q)
            {
                let travel = (q.p - np).length();
                let sign = if fr.t.dot(d) >= 0.0 { 1.0 } else { -1.0 };
                if travel >= 0.25 * h
                    && travel <= 2.0 * h
                    && (fr.t * sign).dot(d) >= COS_TURN
                    && h <= 2.0 * self.bound(&fr)
                {
                    break (q, sign);
                }
            }
            h *= 0.5;
        };
        // The seeds the first step passes.
        for (_, i) in self.seeds_on(&st, &q) {
            self.use_seed(i);
        }
        let start = Vert {
            p: np,
            uv: nuv,
            err: self.tol,
            t: d,
        };
        self.walk(q, sign, Cut::Node(n), None, Some((start, h)))
    }

    /// The node ahead of `st` within two steps `h` along `dir`, and the slot it arrives by.
    fn node_ahead(&self, st: &State, dir: Vec3, h: f64) -> Option<(usize, usize)> {
        for (n, node) in self.nodes.iter().enumerate() {
            let d = node.point - st.p;
            let dist = d.length();
            if dist > 0.0 && dist <= 2.0 * h && d.dot(dir) >= 0.7 * dist {
                let back = d * (-1.0 / dist);
                let slot = node
                    .slots
                    .iter()
                    .enumerate()
                    .fold((0, f64::NEG_INFINITY), |b, (i, (s, _))| {
                        let c = s.dot(back);
                        if c > b.1 { (i, c) } else { b }
                    })
                    .0;
                return Some((n, slot));
            }
        }
        None
    }

    /// The march from `st` along `sign` times `n_A x n_B` until it ends (module docs), as
    /// runs cut at seams. `first` is a vertex before `st` (a node it left) with the step
    /// taken from it.
    fn walk(
        &mut self,
        mut st: State,
        sign: f64,
        start: Cut,
        start_seed: Option<usize>,
        first: Option<(Vert, f64)>,
    ) -> Result<Vec<Run>, SsiError> {
        let mut runs = Vec::new();
        let mut verts = Vec::new();
        let mut last_h = None;
        if let Some((v, h)) = first {
            verts.push(v);
            last_h = Some(h);
        }
        let fr0 = frame(&st);
        verts.push(self.vert(&st, fr0.as_ref(), sign));
        let mut run_start = start;
        let mut steps = 0_usize;
        loop {
            let fr = frame(&st).ok_or_else(|| stalled(st.p, last_h.unwrap_or(0.0)))?;
            let dir = fr.t * sign;
            let bound = self.bound(&fr);
            let mut h = last_h.map_or(bound, |l: f64| bound.min(2.0 * l));
            if let Some((n, slot)) = self.node_ahead(&st, dir, h) {
                if self
                    .nodes
                    .get(n)
                    .and_then(|r| r.slots.get(slot))
                    .is_none_or(|s| s.1)
                {
                    return Err(stalled(st.p, h));
                }
                self.use_slot(n, slot);
                if let Some(node) = self.nodes.get(n) {
                    verts.push(Vert {
                        p: node.point,
                        uv: node.uv,
                        err: self.tol,
                        t: dir,
                    });
                }
                runs.push(Run {
                    verts,
                    start: run_start,
                    end: Cut::Node(n),
                });
                return Ok(runs);
            }
            // One step: a point inside, or the point where the curve leaves an extent.
            let (next, exit) = loop {
                self.budget.charge(1)?;
                self.stop.check()?;
                if h.partial_cmp(&self.res)
                    .is_none_or(core::cmp::Ordering::is_lt)
                {
                    return Err(stalled(st.p, h));
                }
                match self.step(&st, &fr, sign, h) {
                    Some(r) => break r,
                    None => h *= 0.5,
                }
            };
            // The seeds on the chord, in order.
            for (_, i) in self.seeds_on(&st, &next) {
                let Some(s) = self.seeds.get(i) else { continue };
                if s.used {
                    // The seed this chord starts at (met by the last chord, or the start).
                    if (s.point - st.p).length() <= NEAR * self.tol {
                        continue;
                    }
                    if Some(i) == start_seed && steps > 0 && start == Cut::Start {
                        // Closed: the branch repeats its first vertex.
                        runs.push(Run {
                            verts,
                            start: run_start,
                            end: Cut::Start,
                        });
                        return Ok(runs);
                    }
                    if exit.is_some() && s.rim.is_some() {
                        // The rim seed the exit is at, used by an arc that ended there: met
                        // again below.
                        continue;
                    }
                    return Err(stalled(s.point, h));
                }
                let (p, uv, rim) = (s.point, s.uv, s.rim);
                if exit.is_some() && rim.is_some() {
                    continue;
                }
                self.use_seed(i);
                if self.uv_between(uv, st.uv, next.uv, false) {
                    verts.push(Vert {
                        p,
                        uv,
                        err: self.tol,
                        t: dir,
                    });
                }
            }
            let nfr = frame(&next);
            match exit {
                None => {
                    verts.push(self.vert(&next, nfr.as_ref(), sign));
                }
                Some((k, axis)) => {
                    let seam = self.side(k).seam.get(axis).copied().unwrap_or(false);
                    if seam {
                        verts.push(self.vert(&next, nfr.as_ref(), sign));
                        let n = self.seam_node(&next);
                        runs.push(Run {
                            verts: std::mem::take(&mut verts),
                            start: run_start,
                            end: Cut::Node(n),
                        });
                        run_start = Cut::Node(n);
                        let sd = self.side(k);
                        let x = coord(&next.uv, k, axis);
                        let other = if (x - sd.lo(axis)).abs() <= (sd.hi(axis) - x).abs() {
                            sd.hi(axis)
                        } else {
                            sd.lo(axis)
                        };
                        let mut uv = next.uv;
                        set_coord(&mut uv, k, axis, other);
                        let moved = self.state(uv).ok_or_else(|| stalled(next.p, h))?;
                        verts.push(self.vert(&moved, frame(&moved).as_ref(), sign));
                        st = moved;
                        last_h = Some(h);
                        steps += 1;
                        continue;
                    }
                    // The rim: the exit must be at a seed.
                    let Some(i) = self.seed_at(&next) else {
                        return Err(stalled(next.p, h));
                    };
                    let Some(s) = self.seeds.get(i) else {
                        return Err(stalled(next.p, h));
                    };
                    if s.used {
                        return Err(stalled(s.point, h));
                    }
                    let p = s.point;
                    self.use_seed(i);
                    let rim = self
                        .rim_of(&next)
                        .unwrap_or(if k == 0 { Rim::A } else { Rim::B });
                    verts.push(Vert {
                        p,
                        uv: next.uv,
                        err: self.tol,
                        t: dir,
                    });
                    runs.push(Run {
                        verts,
                        start: run_start,
                        end: Cut::Rim(rim),
                    });
                    return Ok(runs);
                }
            }
            st = next;
            last_h = Some(h);
            steps += 1;
        }
    }

    /// The seed nearest the exit point `e`, within `NEAR` coincidences.
    fn seed_at(&self, e: &State) -> Option<usize> {
        let r = NEAR * self.tol;
        let from = self
            .by_x
            .partition_point(|&i| self.seeds.get(i).is_some_and(|s| s.point.x < e.p.x - r));
        let mut best: Option<(f64, usize)> = None;
        for &i in self.by_x.iter().skip(from) {
            let Some(s) = self.seeds.get(i) else { continue };
            if s.point.x > e.p.x + r {
                break;
            }
            let d = (s.point - e.p).length();
            if d <= r && best.is_none_or(|(b, j)| d < b || (d == b && i < j)) {
                best = Some((d, i));
            }
        }
        best.map(|(_, i)| i)
    }

    /// One step of length `h` from `st`: the next point, and the extent edge (surface, axis)
    /// when it is where the curve leaves; `None` to reject the step.
    fn step(
        &self,
        st: &State,
        fr: &Frame,
        sign: f64,
        h: f64,
    ) -> Option<(State, Option<(usize, usize)>)> {
        let dir = fr.t * sign;
        let [va, vb] = fr.vel;
        let [ua, ub] = st.uv;
        let pred = [ua + va * (h * sign), ub + vb * (h * sign)];
        let x0 = st.p + dir * h;
        let edge = match self.first_edge(&st.uv, &pred) {
            Some((k, axis, at, lam)) => {
                let [pa, pb] = pred;
                let guess = [ua + (pa - ua) * lam, ub + (pb - ub) * lam];
                Some((k, axis, at, guess))
            }
            None => match self.correct(pred, Fix::Plane { x0, n: dir }) {
                Corr::Done(q) => {
                    let q = *q;
                    let nfr = frame(&q)?;
                    let travel = (q.p - st.p).length();
                    let ok = travel >= 0.25 * h
                        && travel <= 2.0 * h
                        && (nfr.t * sign).dot(dir) >= COS_TURN;
                    // A point that lands on an edge exactly is where the curve leaves.
                    return ok.then(|| (q, self.on_bound(&q.uv)));
                }
                Corr::Left { k, axis, at, uv } => Some((k, axis, at, uv)),
                Corr::Failed => None,
            },
        };
        let (k, axis, at, guess) = edge?;
        let Corr::Done(e) = self.correct(guess, Fix::Edge { k, axis, at }) else {
            return None;
        };
        let e = *e;
        let d = e.p - st.p;
        let travel = d.length();
        // Ahead; or, on a seam, where the step starts (a march from a point on it).
        let seam = self.side(k).seam.get(axis).copied().unwrap_or(false);
        let ahead = d.dot(dir) > 0.0 || (seam && travel <= self.res);
        let ok = travel <= 2.0 * h && ahead && self.inside(&e.uv);
        ok.then_some((e, Some((k, axis))))
    }

    /// The first extent edge `uv` is exactly on: surface, axis.
    fn on_bound(&self, uv: &[Point2; 2]) -> Option<(usize, usize)> {
        (0..2)
            .flat_map(|k| (0..2).map(move |axis| (k, axis)))
            .find(|&(k, axis)| {
                let sd = self.side(k);
                let x = coord(uv, k, axis);
                x == sd.lo(axis) || x == sd.hi(axis)
            })
    }

    /// Whether `uv` is within both extents.
    fn inside(&self, uv: &[Point2; 2]) -> bool {
        self.clamp(*uv) == *uv
    }

    /// The first extent edge the uv line from `a` to `b` crosses: surface, axis, the edge's
    /// value and the fraction along the line.
    fn first_edge(&self, a: &[Point2; 2], b: &[Point2; 2]) -> Option<(usize, usize, f64, f64)> {
        let mut best: Option<(usize, usize, f64, f64)> = None;
        for k in 0..2 {
            let sd = self.side(k);
            for axis in 0..2 {
                let (x0, x1) = (coord(a, k, axis), coord(b, k, axis));
                let at = if x1 < sd.lo(axis) {
                    sd.lo(axis)
                } else if x1 > sd.hi(axis) {
                    sd.hi(axis)
                } else {
                    continue;
                };
                let lam = if x1 == x0 {
                    0.0
                } else {
                    ((at - x0) / (x1 - x0)).clamp(0.0, 1.0)
                };
                if best.is_none_or(|(.., l)| lam < l) {
                    best = Some((k, axis, at, lam));
                }
            }
        }
        best
    }

    /// One output branch of a run (module docs), `None` when it has no length.
    fn branch(&self, run: Run, crossing: usize, swap: bool) -> Result<Option<Branch>, SsiError> {
        let _ = crossing;
        let closed = run.start == Cut::Start && run.end == Cut::Start;
        let mut vs: Vec<Vert> = Vec::with_capacity(run.verts.len());
        for v in run.verts {
            // Points within the resolution are one point.
            if vs
                .last()
                .is_none_or(|l: &Vert| (v.p - l.p).length() > self.res)
            {
                vs.push(v);
            }
        }
        if closed && let (Some(&f), true) = (vs.first(), vs.len() > 2) {
            if vs.last().is_some_and(|l| (l.p - f.p).length() > self.res) {
                vs.push(f);
            } else if let Some(l) = vs.last_mut() {
                *l = f;
            }
        }
        if vs.len() < 2 {
            return Ok(None);
        }
        let to_end = |c: Cut| match c {
            Cut::Rim(r) => End::Rim(r),
            Cut::Node(n) => End::Node(n),
            Cut::Start => End::Rim(Rim::Both),
        };
        let mut ends = [to_end(run.start), to_end(run.end)];
        // Along n_A x n_B at the middle chord.
        let m = (vs.len() - 1) / 2;
        if let (Some(a), Some(b)) = (vs.get(m), vs.get(m + 1))
            && let Some(st) = self.state(a.uv)
            && let Some(fr) = frame(&st)
            && (b.p - a.p).dot(fr.t) < 0.0
        {
            vs.reverse();
            ends.reverse();
        }
        let at = vs.first().map_or(Point3::new(0.0, 0.0, 0.0), |v| v.p);
        let mut knots = vec![0.0, 0.0];
        let mut t = 0.0_f64;
        for w in vs.windows(2) {
            if let [a, b] = w {
                t += (b.p - a.p).length();
                knots.push(t);
            }
        }
        knots.push(t);
        let certificate = self.certificate(&vs, at)?;
        if swap {
            vs.reverse();
            ends.reverse();
            for v in &mut vs {
                v.uv.swap(0, 1);
            }
            knots = knots.iter().rev().map(|k| t - k).collect();
            for e in &mut ends {
                if let End::Rim(r) = e {
                    *r = match r {
                        Rim::A => Rim::B,
                        Rim::B => Rim::A,
                        Rim::Both => Rim::Both,
                    };
                }
            }
        }
        let bad = |_| stalled(at, 0.0);
        let kv = KnotVector::new(1, knots).map_err(bad)?;
        let p3: Vec<[f64; 3]> = vs.iter().map(|v| v.p.to_array()).collect();
        let pa: Vec<[f64; 2]> = vs.iter().map(|v| v.uv[0].to_array()).collect();
        let pb: Vec<[f64; 2]> = vs.iter().map(|v| v.uv[1].to_array()).collect();
        let curve = NurbsCurve::new(kv.clone(), p3, None).map_err(bad)?;
        let ca = NurbsCurve::new(kv.clone(), pa, None).map_err(bad)?;
        let cb = NurbsCurve::new(kv, pb, None).map_err(bad)?;
        let certificate = if swap {
            let [a, b] = certificate.on_surface;
            Certificate {
                on_surface: [b, a],
                ..certificate
            }
        } else {
            certificate
        };
        let ends = match ends {
            _ if closed => Ends::Closed,
            [start, end] => Ends::Open { start, end },
        };
        Ok(Some(Branch {
            curve: Arc::new(curve),
            pcurves: [Curve2::Nurbs(Arc::new(ca)), Curve2::Nurbs(Arc::new(cb))],
            contact: Contact::Transversal,
            ends,
            certificate,
        }))
    }

    /// The branch's certificate (module docs), in the march's operand order.
    fn certificate(&self, vs: &[Vert], at: Point3) -> Result<Certificate, SsiError> {
        let reach = vs.iter().fold(0.0_f64, |r, v| r.max(v.p.max_dimension()));
        let round = up(gamma(64) * reach);
        let mut on = [0.0_f64; 2];
        for (k, o) in on.iter_mut().enumerate() {
            let sd = self.side(k);
            let n = sd.s.to_nurbs(&sd.e).map_err(|_| stalled(at, 0.0))?;
            let mut worst = 0.0_f64;
            let mut start = 0;
            while start + 1 < vs.len() {
                let end = (start + CHUNK).min(vs.len() - 1);
                let chunk = vs.get(start..=end).unwrap_or(&[]);
                let uv = |v: &Vert| if k == 0 { v.uv[0] } else { v.uv[1] };
                let hull = Extent2d::hull(chunk.iter().map(uv)).ok_or_else(|| stalled(at, 0.0))?;
                let jet = n
                    .partials_over::<2>(
                        [
                            Ivl::new(hull.x_min(), hull.x_max()),
                            Ivl::new(hull.y_min(), hull.y_max()),
                        ],
                        0,
                        About::Middle,
                    )
                    .map_err(|_| stalled(at, 0.0))?;
                let norm = |v: Option<[Ivl; 3]>| v.map(|[x, y, z]| Vec3::new(x, y, z).norm().hi);
                let d2 = norm(jet.get(2, 0))
                    .zip(norm(jet.get(1, 1)))
                    .zip(norm(jet.get(0, 2)))
                    .map(|((a, b), c)| a.max(b).max(c))
                    .filter(|d| d.is_finite())
                    .ok_or_else(|| stalled(at, 0.0))?;
                for w in chunk.windows(2) {
                    if let [a, b] = w {
                        let d = uv(b) - uv(a);
                        let h = d.x.abs() + d.y.abs();
                        worst = worst.max(0.125 * h * h * up(d2));
                    }
                }
                start = end;
            }
            let residual = vs.iter().fold(0.0_f64, |r, v| {
                let p = sd.s.point(if k == 0 { v.uv[0] } else { v.uv[1] });
                r.max((p - v.p).length())
            });
            *o = up(residual + worst + round);
        }
        let err = vs.iter().fold(0.0_f64, |e, v| e.max(v.err));
        let sag = vs.windows(2).fold(0.0_f64, |s, w| match w {
            // The arc's sag over its chord, from the turn between the end tangents.
            [a, b] => s.max(0.125 * (b.p - a.p).length() * 1.01 * (b.t - a.t).length()),
            _ => s,
        });
        let [oa, ob] = on;
        Ok(Certificate {
            to_intersection: Length::from_mm(up(err + sag + round)),
            on_surface: [
                OnSurface::Within(Length::from_mm(oa)),
                OnSurface::Within(Length::from_mm(ob)),
            ],
        })
    }
}
