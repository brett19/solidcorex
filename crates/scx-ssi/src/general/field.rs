// Written for solidcorex. Apache-2.0.
//! The field path (research 73 §4.5, row SM6a): where one side has an implicit form `f`
//! ([`Implicit`]), the intersection is the zero set of `g = f(x(s, t))` over the other
//! side's pieces, a polynomial in rational Bernstein form ([`Net`], [`Implicit::field`]),
//! traced in that side's uv; the field side's uv comes from its own closed-form chart
//! inverse ([`Surface::invert`]).
//!
//! # The two proofs and the tolerance
//!
//! With `L` and `m` bounds of `|grad f|` above and below over the region (the piece's box
//! grown by twice the distance below) and `d = tol + shift + delta` (the coincidence, the
//! form's [`Implicit::shift`] and the net's [`Net::delta`](Net)):
//!
//! - **Apart** ([`apart`]): a net point within `d` of the form's zero set has `|g| <= L d
//!   W^k` (mean value along the segment), so `g - c W^k > 0` or `g + c W^k < 0` with `c =
//!   L d` (every Bernstein coefficient past its rounding bound) proves no point of the piece
//!   within the coincidence of the field side's surface.
//! - **Coincident**: `|g| <= m (tol - shift - delta) W^k` puts every point of the piece within
//!   the coincidence of that surface (`|f| / m` bounds the distance to the zero set where
//!   the gradient is at least `m` along the way).
//!
//! # The trace
//!
//! Each net piece (a Bézier cell of the other side that a surviving pair names) is cut into
//! dyadic squares of its unit square. A square is dropped by its box (more than the
//! coincidence from every box of the field side it was paired with) or by the sign proof,
//! taken whole when coincident (only a whole piece is: a smaller square within the
//! coincidence is a touch or a tangency, [`SsiError::Unresolved`] until row `SM6b`), kept as a
//! **graph cell** when `g_t` (or `g_s`) is strictly
//! signed (the zero set is then a graph over the other parameter: one root per column), and
//! otherwise split. A square at the resolution that must still split is
//! [`SsiError::Unresolved`] where it lies.
//!
//! The crossings of the zero set with the squares' edges are found once per **edge
//! segment** (the finest pieces the squares on both sides cut the line into), from the
//! piece's polynomial restricted to the segment ([`bernstein_roots`]), and their parity is
//! made to agree with one sign per grid point (zero counted positive), so the squares on the
//! two sides of a segment see the same crossings at the same bits. A graph cell's arcs then
//! follow from a sweep along the graph's parameter: each crossing of the bottom or top edge
//! toggles whether the arc is present. Roots within the coincidence of an edge's end, or a
//! cluster of roots no wider than that (an edge the curve touches), are taken at the end or
//! as one root per the parity: topology is claimed up to the coincidence (04 R73, Q3).
//!
//! Each arc is a polyline in the net side's uv: its vertices are column roots, as many as
//! the chord's sag needs to stay within half the coincidence (a bound of the curve's second
//! derivative from those of `g` and of the surface). The polylines link through their
//! crossings into chains, and the chains are cut where the field side's inverse leaves its
//! extent (bisection on the chord: [`Rim`]) and where it crosses a periodic seam
//! ([`NodeKind::Seam`]).
//!
//! A coincident piece is clipped to the field side's extent in the net side's uv, as the
//! closed forms clip an overlap: with positive area when the extents overlap, else a
//! zero-width piece along the edge where they abut (the field side's extent grown by the
//! uv distance its derivative bound gives the coincidence, then clamped back into it).

use std::collections::BTreeMap;
use std::sync::Arc;

use scx_algo::bernstein::{Bernstein2, Bounded, Half, RootLimits, bernstein_roots};
use scx_curve::Curve2;
use scx_num::budget::{Budget, Stop};
use scx_num::certified::gamma;
use scx_num::extent2d::Extent2d;
use scx_num::extent3d::Extent3d;
use scx_num::interval::Ivl;
use scx_num::invert::Inversion;
use scx_num::units::Length;
use scx_num::vector2d::Point2;
use scx_num::vector3d::{Point3, Vec3};
use scx_nurbs::{Approach, KnotVector, NurbsCurve};
use scx_nurbs_surface::{About, ParamAxis};
use scx_surface::Surface;

use super::implicit::Implicit;
use super::net::{Net, up};
use super::pieces::{Candidates, boxes_apart};
use super::traced::{Branch, Certificate, End, Ends, Node, NodeKind, OnSurface, Rim, Traced};
use crate::analytic::{Contact, Overlap};
use crate::error::SsiError;

/// The finest square: `2^-MAX_LEVEL` of the piece. Grid coordinates are integers at this
/// scale, so every square's corners are exact.
const MAX_LEVEL: u32 = 48;

/// The most vertices one arc of one square may need before the square is split instead.
const MAX_VERTICES: usize = 4096;

/// The steps of the bisection that places a clip point on a chord.
const CLIP_STEPS: usize = 40;

/// `2^MAX_LEVEL`, the grid's scale.
#[allow(clippy::cast_precision_loss)]
const SCALE: f64 = (1_u64 << MAX_LEVEL) as f64;

// ---------------------------------------------------------------------------------------
// Small Bernstein tools.

/// `+1` or `-1` when every value of `b` over the square is strictly of that sign, past its
/// rounding bound; `None` otherwise.
fn strict_sign(b: &Bounded) -> Option<i8> {
    let lo = (Ivl::from(b.f.min_coefficient()) - Ivl::from(b.err)).lo;
    let hi = (Ivl::from(b.f.max_coefficient()) + Ivl::from(b.err)).hi;
    if lo > 0.0 {
        Some(1)
    } else if hi < 0.0 {
        Some(-1)
    } else {
        None
    }
}

/// An interval holding every value of `b` over the square.
fn range(b: &Bounded) -> Ivl {
    Ivl::new(
        (Ivl::from(b.f.min_coefficient()) - Ivl::from(b.err)).lo,
        (Ivl::from(b.f.max_coefficient()) + Ivl::from(b.err)).hi,
    )
}

/// `b` with its parameters exchanged (the error bound is unchanged: no arithmetic).
fn transpose(b: &Bounded) -> Option<Bounded> {
    let (du, dv) = b.f.degrees();
    let c = b.f.coefficients();
    let mut out = Vec::with_capacity(c.len());
    for j in 0..=dv {
        for i in 0..=du {
            out.push(*c.get(i * (dv + 1) + j)?);
        }
    }
    Some(Bounded {
        f: Bernstein2::new(dv, du, out).ok()?,
        err: b.err,
    })
}

/// The value at `t` of the 1D Bernstein polynomial `c` (de Casteljau).
fn eval1(c: &[f64], t: f64) -> f64 {
    let mut w = c.to_vec();
    let n = w.len();
    for level in 1..n {
        for k in 0..n - level {
            if let (Some(&a), Some(&b)) = (w.get(k), w.get(k + 1))
                && let Some(x) = w.get_mut(k)
            {
                *x = (1.0 - t) * a + t * b;
            }
        }
    }
    w.first().copied().unwrap_or(0.0)
}

/// The two halves of the 1D polynomial `c` split at `t`, each on `[0, 1]`.
fn split1(c: &[f64], t: f64) -> (Vec<f64>, Vec<f64>) {
    let mut w = c.to_vec();
    let n = w.len();
    let mut left = Vec::with_capacity(n);
    let mut right = Vec::with_capacity(n);
    left.extend(w.first().copied());
    right.extend(w.last().copied());
    for level in 1..n {
        for k in 0..n - level {
            if let (Some(&a), Some(&b)) = (w.get(k), w.get(k + 1))
                && let Some(x) = w.get_mut(k)
            {
                *x = (1.0 - t) * a + t * b;
            }
        }
        left.extend(w.first().copied());
        right.extend(w.get(n - level - 1).copied());
    }
    right.reverse();
    (left, right)
}

/// The 1D polynomial `c` on `[a, b]` of `[0, 1]`, reparametrised to `[0, 1]`.
fn restrict1(c: &[f64], a: f64, b: f64) -> Vec<f64> {
    let mut p = c.to_vec();
    if a > 0.0 {
        p = split1(&p, a).1;
    }
    if b < 1.0 {
        p = split1(&p, (b - a) / (1.0 - a)).0;
    }
    p
}

/// The row `i` (fixed first parameter index) of a 2D coefficient array.
fn row(b: &Bernstein2, i: usize) -> Vec<f64> {
    let (_, dv) = b.degrees();
    b.coefficients()
        .iter()
        .skip(i * (dv + 1))
        .take(dv + 1)
        .copied()
        .collect()
}

/// The column `j` (fixed second parameter index).
fn column(b: &Bernstein2, j: usize) -> Vec<f64> {
    let (_, dv) = b.degrees();
    b.coefficients()
        .iter()
        .skip(j)
        .step_by(dv + 1)
        .copied()
        .collect()
}

/// The 1D polynomial in the second parameter at first parameter `s`.
fn at_first(b: &Bernstein2, s: f64) -> Vec<f64> {
    let (_, dv) = b.degrees();
    (0..=dv).map(|j| eval1(&column(b, j), s)).collect()
}

/// The 1D polynomial in the first parameter at second parameter `t`.
fn at_second(b: &Bernstein2, t: f64) -> Vec<f64> {
    let (du, _) = b.degrees();
    (0..=du).map(|i| eval1(&row(b, i), t)).collect()
}

/// A bound of the rounding of evaluating or restricting `b` by de Casteljau along `levels`
/// levels: each level is a convex combination with three roundings.
fn casteljau_err(b: &Bounded, levels: usize) -> f64 {
    up(b.err + gamma(3 * levels + 6) * b.f.max_abs_coefficient())
}

/// A box grown by `r` on every side, rounded outward.
fn grow(b: &Extent3d, r: f64) -> Option<Extent3d> {
    let dn = |x: f64| (Ivl::from(x) - Ivl::from(r)).lo;
    let upp = |x: f64| (Ivl::from(x) + Ivl::from(r)).hi;
    Extent3d::from_bounds(
        dn(b.x_min()),
        dn(b.y_min()),
        dn(b.z_min()),
        upp(b.x_max()),
        upp(b.y_max()),
        upp(b.z_max()),
    )
}

fn unresolved(b: &Extent3d) -> SsiError {
    SsiError::Unresolved {
        at: b.mid(),
        size: Length::from_mm(b.max_length()),
    }
}

fn unresolved_at(p: Point3, size: f64) -> SsiError {
    SsiError::Unresolved {
        at: p,
        size: Length::from_mm(size),
    }
}

// ---------------------------------------------------------------------------------------
// The constants of one net piece against the form.

/// The bounds the proofs of one piece use (module docs).
struct Bounds {
    /// `c` of the apart proof: `L (tol + shift + delta)`.
    drop: f64,
    /// `m (tol - shift - delta)`, or 0 when the coincidence is no larger than the shifts.
    coincident: f64,
    /// `L (shift + delta)`: how far `g` of the exact surfaces may be from the computed one's
    /// zero, in units of `W^k`.
    exact: f64,
}

/// The bounds over a piece with box `bbox` of a net with distance `delta` (module docs).
fn bounds(form: &Implicit, bbox: &Extent3d, delta: f64, tol: f64) -> Option<Bounds> {
    let pre = up(tol + delta);
    let wide = grow(bbox, 4.0 * pre)?;
    let shift = form.shift(form.reach(&wide));
    if shift > pre {
        return None;
    }
    let d = up(pre + shift);
    let (l, m) = form.gradient(&wide)?;
    let margin = (Ivl::from(tol) - Ivl::from(shift) - Ivl::from(delta)).lo;
    Some(Bounds {
        drop: up(l * d),
        coincident: if margin > 0.0 {
            (Ivl::from(m) * Ivl::from(margin)).lo * (1.0 - gamma(2))
        } else {
            0.0
        },
        exact: up(l * up(shift + delta)),
    })
}

/// Whether the piece of `s` over `uv` (box `bbox`) is proved more than `tol` from the zero
/// set of `form` by the sign of the field (module docs). `false` is "not proved".
pub(crate) fn apart(
    form: &Implicit,
    s: &Surface,
    uv: &Extent2d,
    bbox: &Extent3d,
    tol: f64,
) -> bool {
    let proved = || -> Option<bool> {
        let net = Net::of(s, uv)?;
        let g = form.field(&net)?;
        let wk = net.weight_power(form.degree())?;
        let b = bounds(form, bbox, net.delta, tol)?;
        Some(signed_past(&g, &wk, b.drop))
    };
    proved().unwrap_or(false)
}

/// Whether `g - c W^k > 0` or `g + c W^k < 0` over the square.
fn signed_past(g: &Bounded, wk: &Bounded, c: f64) -> bool {
    g.add_scaled(wk, -c)
        .is_ok_and(|p| strict_sign(&p) == Some(1))
        || g.add_scaled(wk, c)
            .is_ok_and(|p| strict_sign(&p) == Some(-1))
}

/// Whether `|g| < c W^k` over the square.
fn within(g: &Bounded, wk: &Bounded, c: f64) -> bool {
    c > 0.0
        && wk.scale(c).sub(g).is_ok_and(|p| strict_sign(&p) == Some(1))
        && g.add_scaled(wk, c)
            .is_ok_and(|p| strict_sign(&p) == Some(1))
}

// ---------------------------------------------------------------------------------------
// The side choice.

/// The field side and its form: of the sides with an implicit form, the one whose field
/// has the least degree (`k` times the other side's degrees); on a tie the form of least
/// degree `k`, then the first. `None` when neither side has a form.
pub(crate) fn field_side(s: [&Surface; 2], e: [&Extent2d; 2]) -> Option<(usize, Implicit)> {
    let [a, b] = s;
    let [ae, be] = e;
    let cost = |form: Option<Implicit>, other: &Surface, ext: &Extent2d| {
        form.map(|f| {
            let degrees = other.to_nurbs(ext).map_or(usize::MAX / 8, |n| {
                n.degree(ParamAxis::U) + n.degree(ParamAxis::V)
            });
            (f.degree().saturating_mul(degrees), f)
        })
    };
    match (cost(Implicit::of(a), b, be), cost(Implicit::of(b), a, ae)) {
        (Some((x, f)), Some((y, g))) => Some(if (y, g.degree()) < (x, f.degree()) {
            (1, g)
        } else {
            (0, f)
        }),
        (Some((_, f)), None) => Some((0, f)),
        (None, Some((_, g))) => Some((1, g)),
        (None, None) => None,
    }
}

// ---------------------------------------------------------------------------------------
// One net piece.

/// A square of a piece: corner `(i, j)` and side `2^-level`, in grid units of the piece's
/// unit square (`s` along the net's u).
#[derive(Clone, Debug)]
struct Square {
    i: u64,
    j: u64,
    level: u32,
    g: Bounded,
    wk: Bounded,
}

impl Square {
    /// The side in grid units.
    const fn side(&self) -> u64 {
        1_u64 << (MAX_LEVEL - self.level)
    }

    /// The corners in grid units: `[s0, s1, t0, t1]`.
    const fn grid(&self) -> [u64; 4] {
        let h = self.side();
        [self.i, self.i + h, self.j, self.j + h]
    }
}

/// How a square is taken.
enum Class {
    Drop,
    Coincident(Vec<Overlap>),
    /// A graph over `s` (`true`: `g_t` signed) or over `t`.
    Graph(bool),
    Split,
}

/// A port: one crossing of the zero set with an edge segment, shared by the squares on
/// both sides of the segment.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct PortKey {
    cell: usize,
    /// Along a line of constant `t` (`true`) or of constant `s`.
    horizontal: bool,
    line: u64,
    a: u64,
    b: u64,
    index: usize,
}

/// A crossing as an arc's end sees it: its port and its point in the piece's unit square.
#[derive(Clone, Copy, Debug)]
struct Crossing {
    key: PortKey,
    /// Along the edge, in the unit square.
    along: f64,
    st: [f64; 2],
}

/// A traced arc: its vertices in the net side's uv with each one's distance bound to the
/// intersection (3D), its ends' ports, and the bounds its chain's certificate needs.
#[derive(Clone, Debug)]
struct ArcOut {
    verts: Vec<(Point2, f64)>,
    ports: [PortKey; 2],
    /// The chord sag bound.
    sag: f64,
    /// The net side's second-partial bound over its piece.
    d2: f64,
}

/// The fixed data of one piece's trace.
struct Cell<'a> {
    id: usize,
    ns: &'a Surface,
    fs: &'a Surface,
    fe: &'a Extent2d,
    uv: Extent2d,
    partners: Vec<Extent3d>,
    tol: f64,
    floor: f64,
    g: Bounded,
    b: Bounds,
    /// `|S_u|` and `|S_v|` over the piece, upper bounds.
    j: [f64; 2],
    /// The largest second partial's length over the piece.
    d2: f64,
    /// The coincidence in the field side's uv (over an upper bound of its first partials),
    /// the slack of a piece of no width; 0 when no bound is found.
    field_slack: f64,
    /// Which of the piece's edges (`s = 0`, `s = 1`, `t = 0`, `t = 1`) lie on the rim of the
    /// net side's extent.
    rim: [bool; 4],
}

/// What one piece gives: arcs (with their ports) and coincident regions, in net-then-field
/// order.
struct CellOut {
    arcs: Vec<ArcOut>,
    overlaps: Vec<Overlap>,
    /// Each port's point in uv and its matching slack, for linking across pieces.
    ports: BTreeMap<PortKey, (Point2, f64)>,
}

impl Cell<'_> {
    fn u_at(&self, s: f64) -> f64 {
        lerp(self.uv.x_min(), self.uv.x_max(), s)
    }

    fn v_at(&self, t: f64) -> f64 {
        lerp(self.uv.y_min(), self.uv.y_max(), t)
    }

    fn uv_at(&self, st: [f64; 2]) -> Point2 {
        let [s, t] = st;
        Point2::new(self.u_at(s), self.v_at(t))
    }

    fn square_uv(&self, q: &Square) -> Option<Extent2d> {
        let [s0, s1, t0, t1] = q.grid().map(|x| x as f64 / SCALE);
        Extent2d::from_bounds(self.u_at(s0), self.v_at(t0), self.u_at(s1), self.v_at(t1))
    }

    /// Classifies a square (module docs).
    fn classify(
        &self,
        q: &Square,
        budget: &mut Budget,
        stop: &Stop,
    ) -> Result<(Class, Option<Extent3d>), SsiError> {
        budget.charge(1)?;
        stop.check()?;
        let Some(uv) = self.square_uv(q) else {
            return Ok((Class::Split, None));
        };
        let Some(bbox) = self.ns.box_over(uv) else {
            return Ok((Class::Split, None));
        };
        if self
            .partners
            .iter()
            .all(|p| boxes_apart(&bbox, p, self.tol))
            || signed_past(&q.g, &q.wk, self.b.drop)
        {
            return Ok((Class::Drop, Some(bbox)));
        }
        if within(&q.g, &q.wk, self.b.coincident) {
            // A whole piece within the coincidence is a coincident patch; a part of one is
            // where the surfaces touch or run tangent within it (row SM6b), refused here.
            if q.level > 0 {
                return Err(unresolved(&bbox));
            }
            if let Some(o) = self.overlap(&uv, budget, stop)? {
                return Ok((Class::Coincident(o), Some(bbox)));
            }
        }
        if self.runs_along_the_rim(q) {
            return Err(unresolved(&bbox));
        }
        let class = if strict_sign(&q.g.deriv_v()).is_some() {
            Class::Graph(true)
        } else if strict_sign(&q.g.deriv_u()).is_some() {
            Class::Graph(false)
        } else {
            Class::Split
        };
        Ok((class, Some(bbox)))
    }

    /// Whether `|g|` is within the coincidence all along an edge of `q` on the extent's
    /// rim: the curve then runs along the rim, where the sign at the grid points (zero
    /// counted positive) cannot see it, so the square is refused (a curve on the boundary,
    /// research 73 §19). The edge's coefficients are the square's own: no arithmetic.
    fn runs_along_the_rim(&self, q: &Square) -> bool {
        let c = self.b.coincident;
        let [s0, s1, t0, t1] = q.grid();
        let end = 1_u64 << MAX_LEVEL;
        let edge = |b: &Bounded, k: usize| -> Option<Bounded> {
            let (du, dv) = b.f.degrees();
            let f = match k {
                0 => Bernstein2::new(0, dv, row(&b.f, 0)),
                1 => Bernstein2::new(0, dv, row(&b.f, du)),
                2 => Bernstein2::new(du, 0, column(&b.f, 0)),
                _ => Bernstein2::new(du, 0, column(&b.f, dv)),
            }
            .ok()?;
            Some(Bounded { f, err: b.err })
        };
        let on = [s0 == 0, s1 == end, t0 == 0, t1 == end];
        (0..4).any(|k| {
            self.rim.get(k).copied().unwrap_or(false)
                && on.get(k).copied().unwrap_or(false)
                && edge(&q.g, k)
                    .zip(edge(&q.wk, k))
                    .is_some_and(|(g, wk)| within(&g, &wk, c))
        })
    }

    /// The coincident region over the square `uv` in both surfaces' uv, clipped to the
    /// field side's extent: the field side's uv is affine in the net side's over the square
    /// (checked at the corners, the edges' middles and the centre to half the coincidence),
    /// else `None` (the square is split). An empty list: the square lies beside the extent.
    fn overlap(
        &self,
        uv: &Extent2d,
        budget: &mut Budget,
        stop: &Stop,
    ) -> Result<Option<Vec<Overlap>>, SsiError> {
        let (lo, hi) = (uv.min(), uv.max());
        let foot = |p: Point2, budget: &mut Budget| -> Result<Point2, SsiError> {
            let x = self.ns.point(p);
            let inv = self
                .fs
                .invert(x, None, None, budget, stop)
                .map_err(|_| unresolved_at(x, 0.0))?;
            Ok(inv.into_foot().at)
        };
        let periods = [self.fs.domain().u.period(), self.fs.domain().v.period()];
        let unwrap = |q: Point2, near: Point2| {
            let fix =
                |x: f64, n: f64, p: Option<f64>| p.map_or(x, |p| x - p * fmath_round((x - n) / p));
            let [pu, pv] = periods;
            Point2::new(fix(q.x, near.x, pu), fix(q.y, near.y, pv))
        };
        let q0 = foot(lo, budget)?;
        let q1 = unwrap(foot(Point2::new(hi.x, lo.y), budget)?, q0);
        let q3 = unwrap(foot(Point2::new(lo.x, hi.y), budget)?, q0);
        let (e1, e3) = (q1 - q0, q3 - q0);
        let map = |s: f64, t: f64| q0 + e1 * s + e3 * t;
        for (s, t) in [
            (1.0, 1.0),
            (0.5, 0.0),
            (1.0, 0.5),
            (0.5, 1.0),
            (0.0, 0.5),
            (0.5, 0.5),
        ] {
            let p = self
                .ns
                .point(Point2::new(lerp(lo.x, hi.x, s), lerp(lo.y, hi.y, t)));
            if (self.fs.point(map(s, t)) - p).length() > 0.5 * self.tol {
                return Ok(None);
            }
        }
        let det = e1.x * e3.y - e1.y * e3.x;
        if det == 0.0 || !det.is_finite() {
            return Ok(None);
        }
        let to_n = |q: Point2| {
            let d = q - q0;
            let s = (d.x * e3.y - d.y * e3.x) / det;
            let t = (e1.x * d.y - e1.y * d.x) / det;
            Point2::new(
                lerp(lo.x, hi.x, s.clamp(0.0, 1.0)),
                lerp(lo.y, hi.y, t.clamp(0.0, 1.0)),
            )
        };
        let poly = [q0, map(1.0, 0.0), map(1.0, 1.0), map(0.0, 1.0)];
        let shifts =
            |p: Option<f64>| -> Vec<f64> { p.map_or_else(|| vec![0.0], |p| vec![-p, 0.0, p]) };
        let [pu, pv] = periods;
        let (su, sv) = (shifts(pu), shifts(pv));
        let moved = |du: f64, dv: f64| -> Vec<Point2> {
            poly.iter()
                .map(|q| Point2::new(q.x + du, q.y + dv))
                .collect()
        };
        let piece = |clipped: &[Point2], du: f64, dv: f64| Overlap {
            on_a: clipped
                .iter()
                .map(|q| to_n(Point2::new(q.x - du, q.y - dv)))
                .collect(),
            on_b: clipped.to_vec(),
        };
        let mut out = Vec::new();
        for &du in &su {
            for &dv in &sv {
                let clipped = clip_rect(&moved(du, dv), self.fe);
                if clipped.len() >= 3 && polygon_area(&clipped) > 0.0 {
                    out.push(piece(&clipped, du, dv));
                }
            }
        }
        // No overlap of positive area: where the square's image touches the extent within
        // the coincidence (a neighbour on the same surface), a piece of no width there, as
        // the closed forms give (its vertices clamped into the extent).
        if out.is_empty() && self.field_slack > 0.0 {
            let r = self.field_slack;
            let fe = self.fe;
            if let Some(grown) = Extent2d::from_bounds(
                fe.x_min() - r,
                fe.y_min() - r,
                fe.x_max() + r,
                fe.y_max() + r,
            ) {
                for &du in &su {
                    for &dv in &sv {
                        let clipped = clip_rect(&moved(du, dv), &grown);
                        if clipped.len() >= 3 {
                            let flat: Vec<Point2> =
                                clipped.iter().map(|q| clamp_to(*q, fe)).collect();
                            out.push(piece(&flat, du, dv));
                        }
                    }
                }
            }
        }
        Ok(Some(out))
    }
}

/// `x` rounded to the nearest integer (half away from zero), in plain arithmetic.
fn fmath_round(x: f64) -> f64 {
    x.round()
}

fn lerp(a: f64, b: f64, s: f64) -> f64 {
    if s == 0.0 {
        a
    } else if s == 1.0 {
        b
    } else {
        a + (b - a) * s
    }
}

/// The convex polygon `poly` clipped to the rectangle `r` (Sutherland-Hodgman).
fn clip_rect(poly: &[Point2], r: &Extent2d) -> Vec<Point2> {
    let mut out = poly.to_vec();
    let sides: [(usize, f64, bool); 4] = [
        (0, r.x_min(), true),
        (0, r.x_max(), false),
        (1, r.y_min(), true),
        (1, r.y_max(), false),
    ];
    for (axis, c, keep_above) in sides {
        let coord = |p: &Point2| if axis == 0 { p.x } else { p.y };
        let inside = |p: &Point2| {
            if keep_above {
                coord(p) >= c
            } else {
                coord(p) <= c
            }
        };
        let input = std::mem::take(&mut out);
        let n = input.len();
        for k in 0..n {
            let (Some(&a), Some(&b)) = (input.get(k), input.get((k + 1) % n)) else {
                continue;
            };
            let (ia, ib) = (inside(&a), inside(&b));
            if ia {
                out.push(a);
            }
            if ia != ib {
                let t = (c - coord(&a)) / (coord(&b) - coord(&a));
                let mut p = a + (b - a) * t;
                if axis == 0 {
                    p.x = c;
                } else {
                    p.y = c;
                }
                out.push(p);
            }
        }
    }
    out
}

fn polygon_area(p: &[Point2]) -> f64 {
    let n = p.len();
    let mut a = 0.0;
    for k in 0..n {
        if let (Some(x), Some(y)) = (p.get(k), p.get((k + 1) % n)) {
            a += x.x * y.y - y.x * x.y;
        }
    }
    0.5 * a.abs()
}

// ---------------------------------------------------------------------------------------
// Edge segments and their crossings.

/// The crossings of one edge segment, by position along it in the unit square; `None`
/// when they cannot be decided (the squares beside it are split).
type Segment = Option<Vec<f64>>;

/// The shared state of one piece's assembly: segment crossings and grid-point signs,
/// computed once each.
#[derive(Default)]
struct Memo {
    segments: BTreeMap<(bool, u64, u64, u64), Segment>,
    signs: BTreeMap<(u64, u64), bool>,
}

impl Cell<'_> {
    /// The sign of `g` at grid point `(s, t)` (zero counted positive), one value per point.
    fn sign(&self, memo: &mut Memo, s: u64, t: u64) -> bool {
        *memo
            .signs
            .entry((s, t))
            .or_insert_with(|| self.g.f.eval(s as f64 / SCALE, t as f64 / SCALE) >= 0.0)
    }

    /// The crossings of the segment `[a, b]` of the line `line` (grid units), along it.
    fn segment(&self, memo: &mut Memo, horizontal: bool, line: u64, a: u64, b: u64) -> Segment {
        if let Some(s) = memo.segments.get(&(horizontal, line, a, b)) {
            return s.clone();
        }
        let ends = if horizontal {
            [self.sign(memo, a, line), self.sign(memo, b, line)]
        } else {
            [self.sign(memo, line, a), self.sign(memo, line, b)]
        };
        let out = self.crossings(horizontal, line, a, b, ends);
        memo.segments.insert((horizontal, line, a, b), out.clone());
        out
    }

    fn crossings(&self, horizontal: bool, line: u64, a: u64, b: u64, ends: [bool; 2]) -> Segment {
        let c = line as f64 / SCALE;
        let (fa, fb) = (a as f64 / SCALE, b as f64 / SCALE);
        let full = if horizontal {
            at_second(&self.g.f, c)
        } else {
            at_first(&self.g.f, c)
        };
        let e = restrict1(&full, fa, fb);
        let (du, dv) = self.g.f.degrees();
        let err = casteljau_err(&self.g, 2 * (du + dv));
        let parity = ends.first() != ends.last();
        let lo = e.iter().copied().fold(f64::INFINITY, f64::min);
        let hi = e.iter().copied().fold(f64::NEG_INFINITY, f64::max);
        if lo > err || hi < -err {
            return (!parity).then(Vec::new);
        }
        // The window within which a root is taken at the segment's end: a quarter of the
        // coincidence along the edge.
        let (jac, len) = if horizontal {
            (self.j.first().copied()?, self.uv.x_length())
        } else {
            (self.j.get(1).copied()?, self.uv.y_length())
        };
        let span = (fb - fa) * len * jac;
        let w = if span > 0.0 {
            (0.25 * self.tol / span).min(0.125)
        } else {
            0.125
        };
        let roots = bernstein_roots(&e, 0.0, 1.0, RootLimits::DEFAULT);
        let mut near_end = [false; 2];
        let mut inner = Vec::new();
        for &x in &roots.places {
            if x < w {
                near_end[0] = true;
            } else if x > 1.0 - w {
                near_end[1] = true;
            } else {
                inner.push(x);
            }
        }
        let mut local = if roots.isolated {
            inner
        } else {
            match (inner.first(), inner.last()) {
                (Some(&p), Some(&q)) if q - p <= 2.0 * w => {
                    if parity {
                        vec![0.5 * (p + q)]
                    } else {
                        Vec::new()
                    }
                }
                (None, _) | (_, None) => Vec::new(),
                _ => return None,
            }
        };
        if local.len() % 2 != usize::from(parity) {
            let (c0, c1) = (e.first()?.abs(), e.last()?.abs());
            let at_start = match near_end {
                [true, false] => true,
                [false, true] => false,
                _ => c0 <= c1,
            };
            let small = if at_start { c0 } else { c1 };
            if !(near_end[0] || near_end[1] || small <= 4.0 * err) {
                return None;
            }
            if at_start {
                local.insert(0, 0.0);
            } else {
                local.push(1.0);
            }
        }
        Some(local.into_iter().map(|x| lerp(fa, fb, x)).collect())
    }
}

// ---------------------------------------------------------------------------------------
// Assembly.

/// Which squares failed in an assembly pass (to split), else the arcs.
enum Assembled {
    Arcs(Vec<ArcOut>, BTreeMap<PortKey, (Point2, f64)>),
    Failed(Vec<usize>),
}

impl Cell<'_> {
    /// The cut points of each grid line: every graph square's corners on it.
    fn cuts(graph: &[(Square, bool)]) -> BTreeMap<(bool, u64), Vec<u64>> {
        let mut cuts: BTreeMap<(bool, u64), Vec<u64>> = BTreeMap::new();
        for (q, _) in graph {
            let [s0, s1, t0, t1] = q.grid();
            for t in [t0, t1] {
                cuts.entry((true, t)).or_default().extend([s0, s1]);
            }
            for s in [s0, s1] {
                cuts.entry((false, s)).or_default().extend([t0, t1]);
            }
        }
        for v in cuts.values_mut() {
            v.sort_unstable();
            v.dedup();
        }
        cuts
    }

    /// The crossings of one square's edge `[a, b]` on `line`, in order along it.
    fn edge(
        &self,
        memo: &mut Memo,
        cuts: &BTreeMap<(bool, u64), Vec<u64>>,
        horizontal: bool,
        line: u64,
        a: u64,
        b: u64,
    ) -> Option<Vec<Crossing>> {
        let points = cuts.get(&(horizontal, line))?;
        let mut stops: Vec<u64> = points.iter().copied().filter(|&x| x > a && x < b).collect();
        stops.insert(0, a);
        stops.push(b);
        let mut out = Vec::new();
        for w in stops.windows(2) {
            let (&x, &y) = (w.first()?, w.last()?);
            let found = self.segment(memo, horizontal, line, x, y)?;
            let c = line as f64 / SCALE;
            for (index, along) in found.into_iter().enumerate() {
                out.push(Crossing {
                    key: PortKey {
                        cell: self.id,
                        horizontal,
                        line,
                        a: x,
                        b: y,
                        index,
                    },
                    along,
                    st: if horizontal { [along, c] } else { [c, along] },
                });
            }
        }
        Some(out)
    }

    /// Every graph square's arcs, or the squares that failed.
    fn assemble(&self, graph: &[(Square, bool)], memo: &mut Memo) -> Assembled {
        let cuts = Self::cuts(graph);
        let mut arcs = Vec::new();
        let mut failed = Vec::new();
        let mut ports = BTreeMap::new();
        for (k, (q, over_s)) in graph.iter().enumerate() {
            match self.square_arcs(q, *over_s, memo, &cuts) {
                Some(found) => {
                    for a in &found {
                        for (key, uv) in a
                            .ports
                            .iter()
                            .zip([a.verts.first().map(|v| v.0), a.verts.last().map(|v| v.0)])
                        {
                            if let Some(uv) = uv {
                                ports.insert(*key, (uv, self.slack(key, uv)));
                            }
                        }
                    }
                    arcs.extend(found);
                }
                None => failed.push(k),
            }
        }
        if failed.is_empty() {
            Assembled::Arcs(arcs, ports)
        } else {
            Assembled::Failed(failed)
        }
    }

    /// How far the same crossing seen from the next piece may lie: twice the end window,
    /// in uv, plus rounding.
    fn slack(&self, key: &PortKey, uv: Point2) -> f64 {
        let (jac, len) = if key.horizontal {
            (self.j.first().copied().unwrap_or(0.0), self.uv.x_length())
        } else {
            (self.j.get(1).copied().unwrap_or(0.0), self.uv.y_length())
        };
        let window = if jac > 0.0 {
            (0.5 * self.tol / jac).min(0.25 * len)
        } else {
            0.25 * len
        };
        window + 1e-12 * (1.0 + uv.x.abs() + uv.y.abs())
    }

    /// The arcs of one graph square (module docs), or `None` when its crossings do not
    /// make a graph (it is split).
    fn square_arcs(
        &self,
        q: &Square,
        over_s: bool,
        memo: &mut Memo,
        cuts: &BTreeMap<(bool, u64), Vec<u64>>,
    ) -> Option<Vec<ArcOut>> {
        let [s0, s1, t0, t1] = q.grid();
        // Bottom, top, left, right in the graph's orientation: bottom and top are the edges
        // at the low and high end of the root's parameter.
        let (bottom, top, left, right) = if over_s {
            (
                self.edge(memo, cuts, true, t0, s0, s1)?,
                self.edge(memo, cuts, true, t1, s0, s1)?,
                self.edge(memo, cuts, false, s0, t0, t1)?,
                self.edge(memo, cuts, false, s1, t0, t1)?,
            )
        } else {
            (
                self.edge(memo, cuts, false, s0, t0, t1)?,
                self.edge(memo, cuts, false, s1, t0, t1)?,
                self.edge(memo, cuts, true, t0, s0, s1)?,
                self.edge(memo, cuts, true, t1, s0, s1)?,
            )
        };
        if left.len() > 1 || right.len() > 1 {
            return None;
        }
        let mut events: Vec<Crossing> = bottom.into_iter().chain(top).collect();
        events.sort_by(|x, y| x.along.total_cmp(&y.along));
        // Two crossings of bottom and top at one place: not a graph that the sweep can
        // read.
        if events
            .windows(2)
            .any(|w| matches!(w, [x, y] if y.along - x.along <= 0.0 && x.key.horizontal == y.key.horizontal && x.key.line != y.key.line))
        {
            return None;
        }
        let mut pairs = Vec::new();
        let mut open = left.first().copied();
        for e in events {
            match open.take() {
                Some(start) => pairs.push((start, e)),
                None => open = Some(e),
            }
        }
        match (open, right.first().copied()) {
            (Some(start), Some(end)) => pairs.push((start, end)),
            (None, None) => {}
            _ => return None,
        }
        let p = if over_s {
            q.g.clone()
        } else {
            transpose(&q.g)?
        };
        let side = q.side() as f64 / SCALE;
        let origin = if over_s {
            [s0 as f64 / SCALE, t0 as f64 / SCALE]
        } else {
            [t0 as f64 / SCALE, s0 as f64 / SCALE]
        };
        let mut out = Vec::with_capacity(pairs.len());
        for (a, b) in pairs {
            out.push(self.arc(&p, &q.wk, over_s, side, origin, a, b)?);
        }
        Some(out)
    }

    /// The arc of a graph square from crossing `a` to crossing `b`: vertices at column
    /// roots of `p` (the square's `g` with the graph's parameter first).
    fn arc(
        &self,
        p: &Bounded,
        wk: &Bounded,
        over_s: bool,
        side: f64,
        origin: [f64; 2],
        a: Crossing,
        b: Crossing,
    ) -> Option<ArcOut> {
        let [o_sig, o_tau] = origin;
        // A crossing's place in the square, (sigma, tau) in [0, 1]^2.
        let local = |c: &Crossing| {
            let [s, t] = c.st;
            let (sig, tau) = if over_s { (s, t) } else { (t, s) };
            (
                ((sig - o_sig) / side).clamp(0.0, 1.0),
                ((tau - o_tau) / side).clamp(0.0, 1.0),
            )
        };
        let ((sa, _), (sb, _)) = (local(&a), local(&b));
        let ps = p.deriv_u();
        let pt = p.deriv_v();
        let gt = range(&pt);
        if !(gt.lo > 0.0 || gt.hi < 0.0) {
            return None;
        }
        let gs = range(&ps);
        let gss = range(&ps.deriv_u());
        let gst = range(&ps.deriv_v());
        let gtt = range(&pt.deriv_v());
        let zero = Ivl::from(0.0);
        let phi1 = zero - gs / gt;
        let phi2 = zero - (gss + Ivl::from(2.0) * gst * phi1 + gtt * phi1 * phi1) / gt;
        let [ju, jv] = self.j;
        let (wu, wv) = (self.uv.x_length() * side, self.uv.y_length() * side);
        let (h_sig, h_tau, j_tau) = if over_s { (wu, wv, jv) } else { (wv, wu, ju) };
        let lead = h_sig + phi1.mag() * h_tau;
        let k = up(phi2.mag() * h_tau * j_tau + lead * lead * self.d2);
        let target = 0.5 * self.tol;
        let length = (sb - sa).abs();
        let n = if k > 0.0 && length > 0.0 {
            (length * (k / (8.0 * target)).sqrt()).ceil()
        } else {
            1.0
        };
        if !n.is_finite() || n > MAX_VERTICES as f64 {
            return None;
        }
        let n = (n as usize).max(1);
        let step = length / n as f64;
        let sag = up(k * step * step / 8.0);
        let (du, dv) = p.f.degrees();
        let eval_err = casteljau_err(p, du + dv);
        let wk_max = range(wk).hi.max(0.0);
        let m1 = gt.mig();
        let err_at = |sig: f64, tau: f64| {
            let g = p.f.eval(sig, tau).abs();
            up((g + eval_err + self.b.exact * wk_max) / m1 * h_tau * j_tau)
        };
        let to_st = |sig: f64, tau: f64| {
            let (x, y) = (o_sig + sig * side, o_tau + tau * side);
            if over_s { [x, y] } else { [y, x] }
        };
        let mut verts = Vec::with_capacity(n + 1);
        let (la, lb) = (local(&a), local(&b));
        verts.push((self.uv_at(a.st), err_at(la.0, la.1)));
        for i in 1..n {
            let sig = sa + (sb - sa) * (i as f64 / n as f64);
            let col = at_first(&p.f, sig);
            let tau = root1(&col);
            verts.push((self.uv_at(to_st(sig, tau)), err_at(sig, tau)));
        }
        verts.push((self.uv_at(b.st), err_at(lb.0, lb.1)));
        Some(ArcOut {
            verts,
            ports: [a.key, b.key],
            sag,
            d2: self.d2,
        })
    }
}

/// The root in `[0, 1]` of a monotone 1D polynomial by bisection; the end of the smaller
/// value when its ends have one sign.
fn root1(c: &[f64]) -> f64 {
    let (f0, f1) = (
        c.first().copied().unwrap_or(0.0),
        c.last().copied().unwrap_or(0.0),
    );
    if f0 == 0.0 {
        return 0.0;
    }
    if f1 == 0.0 {
        return 1.0;
    }
    if (f0 > 0.0) == (f1 > 0.0) {
        return if f0.abs() <= f1.abs() { 0.0 } else { 1.0 };
    }
    let (mut lo, mut hi) = (0.0_f64, 1.0_f64);
    let low_positive = f0 > 0.0;
    for _ in 0..64 {
        let mid = 0.5 * (lo + hi);
        if !(mid > lo && mid < hi) {
            break;
        }
        let f = eval1(c, mid);
        if f == 0.0 {
            return mid;
        }
        if (f > 0.0) == low_positive {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    0.5 * (lo + hi)
}

/// Traces one piece: classifies squares, splitting as needed, and assembles the graph
/// squares' arcs (module docs).
fn trace_cell(
    cell: &Cell<'_>,
    wk: Bounded,
    budget: &mut Budget,
    stop: &Stop,
) -> Result<CellOut, SsiError> {
    let mut todo = vec![Square {
        i: 0,
        j: 0,
        level: 0,
        g: cell.g.clone(),
        wk,
    }];
    let mut graph: Vec<(Square, bool)> = Vec::new();
    let mut boxes: Vec<Option<Extent3d>> = Vec::new();
    let mut overlaps = Vec::new();
    let mut memo = Memo::default();
    loop {
        while let Some(q) = todo.pop() {
            let (class, bbox) = cell.classify(&q, budget, stop)?;
            match class {
                Class::Drop => {}
                Class::Coincident(o) => overlaps.extend(o),
                Class::Graph(over_s) => {
                    graph.push((q, over_s));
                    boxes.push(bbox);
                }
                Class::Split => split_into(cell, &q, bbox, &mut todo)?,
            }
        }
        // Squares in grid order, so the assembly does not depend on the work order.
        let mut order: Vec<usize> = (0..graph.len()).collect();
        order.sort_by_key(|&k| graph.get(k).map(|(q, _)| (q.i, q.j, q.level)));
        let sorted: Vec<(Square, bool)> = order
            .iter()
            .filter_map(|&k| graph.get(k).cloned())
            .collect();
        let sorted_boxes: Vec<Option<Extent3d>> = order
            .iter()
            .filter_map(|&k| boxes.get(k).copied())
            .collect();
        graph = sorted;
        boxes = sorted_boxes;
        match cell.assemble(&graph, &mut memo) {
            Assembled::Arcs(arcs, ports) => {
                return Ok(CellOut {
                    arcs,
                    overlaps,
                    ports,
                });
            }
            Assembled::Failed(failed) => {
                let mut keep = Vec::with_capacity(graph.len());
                let mut keep_boxes = Vec::with_capacity(graph.len());
                for (k, (sq, b)) in graph.drain(..).zip(boxes.drain(..)).enumerate() {
                    if failed.contains(&k) {
                        split_into(cell, &sq.0, b, &mut todo)?;
                    } else {
                        keep.push(sq);
                        keep_boxes.push(b);
                    }
                }
                graph = keep;
                boxes = keep_boxes;
                // The segments the split squares cut change: decide them again.
                memo.segments.clear();
            }
        }
    }
}

/// Pushes the four quarters of `q` (to be classified), or refuses at the resolution.
fn split_into(
    cell: &Cell<'_>,
    q: &Square,
    bbox: Option<Extent3d>,
    todo: &mut Vec<Square>,
) -> Result<(), SsiError> {
    let at_floor = bbox.is_none_or(|b| b.max_length() <= cell.floor);
    if at_floor || q.level >= MAX_LEVEL {
        return Err(match bbox {
            Some(b) => unresolved(&b),
            None => unresolved_at(cell.ns.point(cell.uv.mid()), cell.uv.max_dimension()),
        });
    }
    let h = q.side() / 2;
    // Pushed in reverse, so they are taken lower-left first.
    for (hu, hv, di, dj) in [
        (Half::Upper, Half::Upper, h, h),
        (Half::Lower, Half::Upper, 0, h),
        (Half::Upper, Half::Lower, h, 0),
        (Half::Lower, Half::Lower, 0, 0),
    ] {
        todo.push(Square {
            i: q.i + di,
            j: q.j + dj,
            level: q.level + 1,
            g: q.g.quarter(hu, hv),
            wk: q.wk.quarter(hu, hv),
        });
    }
    Ok(())
}

/// `|S_u|`, `|S_v|` and the largest second partial over `uv`, upper bounds.
fn derivative_bounds(s: &Surface, uv: &Extent2d) -> Option<([f64; 2], f64)> {
    let n = s.to_nurbs(uv).ok()?;
    let jet = n
        .partials_over::<2>(
            [
                Ivl::new(uv.x_min(), uv.x_max()),
                Ivl::new(uv.y_min(), uv.y_max()),
            ],
            0,
            About::Middle,
        )
        .ok()?;
    let norm = |v: Option<[Ivl; 3]>| v.map(|[x, y, z]| Vec3::new(x, y, z).norm().hi);
    let ju = norm(jet.get(1, 0))?;
    let jv = norm(jet.get(0, 1))?;
    let d2 = norm(jet.get(2, 0))?
        .max(norm(jet.get(1, 1))?)
        .max(norm(jet.get(0, 2))?);
    (ju.is_finite() && jv.is_finite() && d2.is_finite()).then_some(([up(ju), up(jv)], up(d2)))
}

// ---------------------------------------------------------------------------------------
// Linking and output.

/// How a chain ends before the field side is placed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Open {
    /// On the net side's extent boundary.
    Rim,
    /// Where a square beside it was dropped by its box: outside the field side's extent,
    /// which the clip must confirm.
    Loose,
}

/// A chain of arcs: vertices in the net side's uv with their distance bounds, closed or
/// with its ends.
struct Chain {
    verts: Vec<(Point2, f64)>,
    ends: Option<[Open; 2]>,
    sag: f64,
    d2: f64,
}

/// A union-find root.
fn root(parent: &mut [usize], mut x: usize) -> usize {
    while let Some(&p) = parent.get(x) {
        if p == x {
            break;
        }
        let gp = parent.get(p).copied().unwrap_or(p);
        if let Some(slot) = parent.get_mut(x) {
            *slot = gp;
        }
        x = p;
    }
    x
}

/// Links the arcs of every piece into chains through their ports: exactly within a piece,
/// by place (within both ports' slack, uniquely) across pieces.
fn link(
    ns: &Surface,
    ne: &Extent2d,
    arcs: &[ArcOut],
    ports: &BTreeMap<PortKey, (Point2, f64)>,
) -> Result<Vec<Chain>, SsiError> {
    let keys: Vec<PortKey> = ports.keys().copied().collect();
    let index: BTreeMap<PortKey, usize> = keys.iter().enumerate().map(|(i, k)| (*k, i)).collect();
    let mut degree = vec![0_usize; keys.len()];
    for a in arcs {
        for k in &a.ports {
            if let Some(d) = index.get(k).and_then(|&i| degree.get_mut(i)) {
                *d += 1;
            }
        }
    }
    let info = |i: usize| keys.get(i).and_then(|k| ports.get(k).map(|v| (*k, *v)));
    let on_rim = |uv: Point2| {
        uv.x == ne.x_min() || uv.x == ne.x_max() || uv.y == ne.y_min() || uv.y == ne.y_max()
    };
    let outer = |k: &PortKey| k.line == 0 || k.line == 1_u64 << MAX_LEVEL;
    let mut parent: Vec<usize> = (0..keys.len()).collect();
    let mut bridged = vec![false; keys.len()];
    for i in 0..keys.len() {
        let Some((ki, (pi, si))) = info(i) else {
            continue;
        };
        if degree.get(i) != Some(&1) || !outer(&ki) || on_rim(pi) || bridged.get(i) == Some(&true) {
            continue;
        }
        let mut found = Vec::new();
        for j in 0..keys.len() {
            let Some((kj, (pj, sj))) = info(j) else {
                continue;
            };
            if kj.cell == ki.cell || degree.get(j) != Some(&1) || !outer(&kj) {
                continue;
            }
            let d = (pi.x - pj.x).abs() + (pi.y - pj.y).abs();
            if d <= si.max(sj) {
                found.push(j);
            }
        }
        match found.as_slice() {
            [] => {}
            [j] => {
                if bridged.get(*j) == Some(&true) {
                    return Err(unresolved_at(ns.point(pi), si));
                }
                let (ri, rj) = (root(&mut parent, i), root(&mut parent, *j));
                if let Some(slot) = parent.get_mut(rj) {
                    *slot = ri;
                }
                for x in [i, *j] {
                    if let Some(b) = bridged.get_mut(x) {
                        *b = true;
                    }
                }
            }
            _ => return Err(unresolved_at(ns.point(pi), si)),
        }
    }
    // Incidence by root.
    let mut incident: BTreeMap<usize, Vec<(usize, usize)>> = BTreeMap::new();
    for (ai, a) in arcs.iter().enumerate() {
        for (end, k) in a.ports.iter().enumerate() {
            let Some(&i) = index.get(k) else { continue };
            let r = root(&mut parent, i);
            incident.entry(r).or_default().push((ai, end));
        }
    }
    for (r, list) in &incident {
        if list.len() > 2 {
            let at = info(*r).map_or_else(|| ne.mid(), |(_, (p, _))| p);
            return Err(unresolved_at(ns.point(at), 0.0));
        }
    }
    let port_root = |parent: &mut Vec<usize>, k: &PortKey| index.get(k).map(|&i| root(parent, i));
    let mut used = vec![false; arcs.len()];
    let mut chains = Vec::new();
    // The ends first (in port order), then the loops.
    let starts: Vec<(usize, usize, usize)> = incident
        .iter()
        .filter(|(_, l)| l.len() == 1)
        .filter_map(|(r, l)| l.first().map(|&(a, e)| (*r, a, e)))
        .collect();
    let end_kind = |r: usize| {
        info(r).map_or(
            Open::Loose,
            |(_, (p, _))| if on_rim(p) { Open::Rim } else { Open::Loose },
        )
    };
    for (r, a0, e0) in starts {
        if used.get(a0) == Some(&true) {
            continue;
        }
        let mut verts: Vec<(Point2, f64)> = Vec::new();
        let (mut a, mut from_end) = (a0, e0);
        let (mut sag, mut d2) = (0.0_f64, 0.0_f64);
        let last_root;
        loop {
            let Some(arc) = arcs.get(a) else {
                return Err(unresolved_at(ns.point(ne.mid()), 0.0));
            };
            if let Some(u) = used.get_mut(a) {
                *u = true;
            }
            sag = sag.max(arc.sag);
            d2 = d2.max(arc.d2);
            let mut vs = arc.verts.clone();
            if from_end == 1 {
                vs.reverse();
            }
            let skip = usize::from(!verts.is_empty());
            verts.extend(vs.into_iter().skip(skip));
            let other = arc.ports.get(1 - from_end).copied();
            let Some(rn) = other.and_then(|k| port_root(&mut parent, &k)) else {
                return Err(unresolved_at(ns.point(ne.mid()), 0.0));
            };
            let next = incident
                .get(&rn)
                .and_then(|l| l.iter().find(|&&(x, _)| x != a || l.len() == 1).copied());
            match next {
                Some((na, ne_)) if na != a && used.get(na) == Some(&false) => {
                    a = na;
                    from_end = ne_;
                }
                _ => {
                    last_root = rn;
                    break;
                }
            }
        }
        chains.push(Chain {
            verts,
            ends: Some([end_kind(r), end_kind(last_root)]),
            sag,
            d2,
        });
    }
    for a0 in 0..arcs.len() {
        if used.get(a0) != Some(&false) {
            continue;
        }
        let mut verts: Vec<(Point2, f64)> = Vec::new();
        let (mut a, mut from_end) = (a0, 0_usize);
        let (mut sag, mut d2) = (0.0_f64, 0.0_f64);
        while let Some(arc) = arcs.get(a) {
            if let Some(u) = used.get_mut(a) {
                *u = true;
            }
            sag = sag.max(arc.sag);
            d2 = d2.max(arc.d2);
            let mut vs = arc.verts.clone();
            if from_end == 1 {
                vs.reverse();
            }
            let skip = usize::from(!verts.is_empty());
            verts.extend(vs.into_iter().skip(skip));
            let other = arc.ports.get(1 - from_end).copied();
            let Some(rn) = other.and_then(|k| port_root(&mut parent, &k)) else {
                break;
            };
            let next = incident.get(&rn).and_then(|l| {
                l.iter()
                    .find(|&&(x, e)| (x, e) != (a, 1 - from_end))
                    .copied()
            });
            match next {
                Some((na, ne_)) if used.get(na) == Some(&false) => {
                    a = na;
                    from_end = ne_;
                }
                _ => break,
            }
        }
        // The loop's last vertex is its first.
        if verts.len() > 1 {
            verts.pop();
        }
        chains.push(Chain {
            verts,
            ends: None,
            sag,
            d2,
        });
    }
    Ok(chains)
}

/// A vertex of a chain with the field side placed.
#[derive(Clone, Copy, Debug)]
struct Placed {
    uv: Point2,
    p: Point3,
    q: Point2,
    inside: bool,
    /// The distance bound to the intersection (3D).
    err: f64,
    /// The foot's distance from the point on the field side.
    residual: f64,
}

/// How an output branch ends before it is oriented.
#[derive(Clone, Copy, Debug)]
enum Cut {
    RimNet,
    RimField,
    Seam(usize),
}

/// The context of the output stage.
struct Out<'a> {
    ns: &'a Surface,
    fs: &'a Surface,
    fe: &'a Extent2d,
    nside: usize,
}

impl Out<'_> {
    fn place(
        &self,
        uv: Point2,
        err: f64,
        budget: &mut Budget,
        stop: &Stop,
    ) -> Result<Placed, SsiError> {
        let p = self.ns.point(uv);
        let inv = self
            .fs
            .invert(p, None, Some(self.fe), budget, stop)
            .map_err(|_| unresolved_at(p, 0.0))?;
        let inside = matches!(inv, Inversion::Inside(_));
        let foot = inv.into_foot();
        Ok(Placed {
            uv,
            p,
            q: clamp_to(foot.at, self.fe),
            inside,
            err,
            residual: foot.distance,
        })
    }

    /// The last inside point on the chord from `a` (inside) to `b` (outside), by
    /// bisection; its field uv clamped to the extent.
    fn clip(
        &self,
        a: &Placed,
        b: &Placed,
        sag: f64,
        budget: &mut Budget,
        stop: &Stop,
    ) -> Result<Placed, SsiError> {
        let (mut lo, mut hi) = (0.0_f64, 1.0_f64);
        let at = |x: f64| a.uv + (b.uv - a.uv) * x;
        let err = up(a.err.max(b.err) + sag);
        let mut best = *a;
        for _ in 0..CLIP_STEPS {
            let mid = 0.5 * (lo + hi);
            let p = self.place(at(mid), err, budget, stop)?;
            if p.inside {
                lo = mid;
                best = p;
            } else {
                hi = mid;
            }
        }
        best.err = err;
        Ok(best)
    }
}

fn clamp_to(q: Point2, e: &Extent2d) -> Point2 {
    Point2::new(
        q.x.clamp(e.x_min(), e.x_max()),
        q.y.clamp(e.y_min(), e.y_max()),
    )
}

/// A run of placed vertices, how it starts and ends, and whether it closes on itself.
type Run = (Vec<Placed>, [Cut; 2], bool);

/// The pieces of a chain inside the field side's extent, cut at its seams: each a run of
/// placed vertices and how it starts and ends.
fn runs(
    out: &Out<'_>,
    chain: &Chain,
    nodes: &mut Vec<Node>,
    budget: &mut Budget,
    stop: &Stop,
) -> Result<Vec<Run>, SsiError> {
    let mut placed = Vec::with_capacity(chain.verts.len());
    for &(uv, err) in &chain.verts {
        budget.charge(1)?;
        stop.check()?;
        placed.push(out.place(uv, err, budget, stop)?);
    }
    let closed = chain.ends.is_none();
    // A loop with a vertex outside is read from that vertex, as an open chain.
    if closed && let Some(k) = placed.iter().position(|v| !v.inside) {
        placed.rotate_left(k);
        if let Some(&first) = placed.first() {
            placed.push(first);
        }
    }
    let all_inside = placed.iter().all(|v| v.inside);
    let ends = chain.ends.unwrap_or([Open::Loose; 2]);
    // Runs of inside vertices, clipped at each transition.
    let mut pieces: Vec<(Vec<Placed>, [Cut; 2])> = Vec::new();
    if closed && all_inside {
        pieces.push((placed.clone(), [Cut::RimField; 2]));
    } else {
        let mut cur: Vec<Placed> = Vec::new();
        let mut start = Cut::RimField;
        for (k, v) in placed.iter().enumerate() {
            if v.inside {
                if cur.is_empty() {
                    if k == 0 {
                        if ends.first() == Some(&Open::Loose) && !closed {
                            return Err(unresolved_at(v.p, 0.0));
                        }
                        start = Cut::RimNet;
                    } else if let Some(prev) = placed.get(k - 1) {
                        cur.push(out.clip(v, prev, chain.sag, budget, stop)?);
                        start = Cut::RimField;
                    }
                }
                cur.push(*v);
            } else if !cur.is_empty() {
                if let Some(last) = cur.last().copied() {
                    cur.push(out.clip(&last, v, chain.sag, budget, stop)?);
                }
                pieces.push((std::mem::take(&mut cur), [start, Cut::RimField]));
            }
        }
        if !cur.is_empty() {
            if ends.last() == Some(&Open::Loose) && !closed {
                let at = cur.last().map_or(Point3::new(0.0, 0.0, 0.0), |v| v.p);
                return Err(unresolved_at(at, 0.0));
            }
            pieces.push((cur, [start, Cut::RimNet]));
        }
    }
    // Seams.
    let periods = [out.fs.domain().u.period(), out.fs.domain().v.period()];
    let mut result = Vec::new();
    for (mut vs, ends) in pieces {
        let jumps = seam_jumps(&vs, periods);
        if jumps.is_empty() {
            result.push((vs, ends, closed && all_inside));
            continue;
        }
        if closed && all_inside {
            // Read the loop from its first seam: drop the repeated end.
            let first = jumps.first().copied().unwrap_or(0);
            vs.rotate_left(first + 1);
            if let Some(&f) = vs.first() {
                vs.push(f);
            }
        }
        let jumps = seam_jumps(&vs, periods);
        let mut cur: Vec<Placed> = Vec::new();
        let mut start = ends.first().copied().unwrap_or(Cut::RimField);
        let mut first_node = None;
        for (k, v) in vs.iter().enumerate() {
            cur.push(*v);
            if jumps.contains(&k)
                && let Some(next) = vs.get(k + 1)
            {
                let (end_v, start_v) = seam_cut(out, v, next, periods, chain.sag)?;
                let node = nodes.len();
                let uvs = if out.nside == 0 {
                    [end_v.uv, end_v.q]
                } else {
                    [end_v.q, end_v.uv]
                };
                nodes.push(Node {
                    point: end_v.p,
                    uv: uvs,
                    kind: NodeKind::Seam,
                });
                first_node.get_or_insert(node);
                cur.push(end_v);
                result.push((std::mem::take(&mut cur), [start, Cut::Seam(node)], false));
                cur.push(start_v);
                start = Cut::Seam(node);
            }
        }
        if !cur.is_empty() {
            let end = if closed && all_inside {
                first_node.map_or(Cut::RimField, Cut::Seam)
            } else {
                ends.last().copied().unwrap_or(Cut::RimField)
            };
            if closed
                && all_inside
                && let Some(f) = result.first_mut()
                && f.1.first().is_some_and(|c| matches!(c, Cut::RimField))
            {
                // The loop's first piece starts where the last ends: join them.
                let mut joined = std::mem::take(&mut cur);
                joined.extend(f.0.iter().skip(1).copied());
                f.0 = joined;
                f.1 = [start, f.1.last().copied().unwrap_or(end)];
            } else {
                result.push((cur, [start, end], false));
            }
        }
    }
    Ok(result)
}

/// The indices `k` where the field uv jumps by more than half a period from vertex `k` to
/// `k + 1`.
fn seam_jumps(vs: &[Placed], periods: [Option<f64>; 2]) -> Vec<usize> {
    let [pu, pv] = periods;
    vs.windows(2)
        .enumerate()
        .filter(|(_, w)| {
            matches!(w, [a, b] if pu.is_some_and(|p| (b.q.x - a.q.x).abs() > 0.5 * p)
                || pv.is_some_and(|p| (b.q.y - a.q.y).abs() > 0.5 * p))
        })
        .map(|(k, _)| k)
        .collect()
}

/// The two sides of the seam crossing between `a` and `b`: the end of the piece before (on
/// `a`'s side of the seam) and the start of the piece after, at one point.
fn seam_cut(
    out: &Out<'_>,
    a: &Placed,
    b: &Placed,
    periods: [Option<f64>; 2],
    sag: f64,
) -> Result<(Placed, Placed), SsiError> {
    let [pu, pv] = periods;
    let unwrap =
        |x: f64, near: f64, p: Option<f64>| p.map_or(x, |p| x - p * ((x - near) / p).round());
    let bq = Point2::new(unwrap(b.q.x, a.q.x, pu), unwrap(b.q.y, a.q.y, pv));
    let shift = b.q - bq;
    // The seam line crossed, in a's frame: the extent's boundary nearest the jump.
    let fe = out.fe;
    let lam = |x0: f64, x1: f64, lo: f64, hi: f64| -> Option<f64> {
        let bound = if x1 > x0 { hi } else { lo };
        let d = x1 - x0;
        (d != 0.0).then(|| ((bound - x0) / d).clamp(0.0, 1.0))
    };
    let t = if pu.is_some() && shift.x != 0.0 {
        lam(a.q.x, bq.x, fe.x_min(), fe.x_max())
    } else {
        lam(a.q.y, bq.y, fe.y_min(), fe.y_max())
    }
    .ok_or_else(|| unresolved_at(a.p, 0.0))?;
    let uv = a.uv + (b.uv - a.uv) * t;
    let q = a.q + (bq - a.q) * t;
    // Too large a step in the field's uv for its chord: a pole, not a seam.
    let step = (bq - a.q).length();
    if pu.is_some_and(|p| step > 0.125 * p) || pv.is_some_and(|p| step > 0.125 * p) {
        return Err(unresolved_at(a.p, (b.p - a.p).length()));
    }
    let p = out.ns.point(uv);
    let err = up(a.err.max(b.err) + sag);
    let residual = a.residual.max(b.residual);
    let end = Placed {
        uv,
        p,
        q: clamp_to(q, fe),
        inside: true,
        err,
        residual,
    };
    let start = Placed {
        q: clamp_to(q + shift, fe),
        ..end
    };
    Ok((end, start))
}

/// The second-partial bound of the field side over the uv box of `qs`.
fn field_d2(fs: &Surface, fe: &Extent2d, qs: &[Point2]) -> Option<f64> {
    let b = Extent2d::hull(qs.iter().copied())?;
    let n = fs.to_nurbs(fe).ok()?;
    let jet = n
        .partials_over::<2>(
            [
                Ivl::new(b.x_min(), b.x_max()),
                Ivl::new(b.y_min(), b.y_max()),
            ],
            0,
            About::Middle,
        )
        .ok()?;
    let norm = |v: Option<[Ivl; 3]>| v.map(|[x, y, z]| Vec3::new(x, y, z).norm().hi);
    let d2 = norm(jet.get(2, 0))?
        .max(norm(jet.get(1, 1))?)
        .max(norm(jet.get(0, 2))?);
    d2.is_finite().then_some(up(d2))
}

/// One output branch from a run (module docs): oriented along `n_a x n_b`, on a chord-length
/// parameter, with its certificate.
fn branch(
    out: &Out<'_>,
    run: Vec<Placed>,
    cuts: [Cut; 2],
    closed: bool,
    chain: &Chain,
) -> Result<Option<Branch>, SsiError> {
    let mut vs: Vec<Placed> = Vec::with_capacity(run.len());
    for v in run {
        if vs
            .last()
            .is_none_or(|l: &Placed| (v.p - l.p).length() > 0.0)
        {
            vs.push(v);
        }
    }
    if closed
        && vs.len() > 2
        && let Some(&f) = vs.first()
    {
        vs.push(f);
    }
    if vs.len() < 2 {
        return Ok(None);
    }
    let field_rim = if out.nside == 0 { Rim::B } else { Rim::A };
    let net_rim = if out.nside == 0 { Rim::A } else { Rim::B };
    let to_end = |c: Cut| match c {
        Cut::RimNet => End::Rim(net_rim),
        Cut::RimField => End::Rim(field_rim),
        Cut::Seam(n) => End::Node(n),
    };
    let mut ends = cuts.map(to_end);
    // The orientation, at the middle chord.
    let m = (vs.len() - 1) / 2;
    if let (Some(a), Some(b)) = (vs.get(m), vs.get(m + 1)) {
        let nn = out.ns.normal(a.uv, [Approach::Above; 2]);
        let nf = out.fs.normal(a.q, [Approach::Above; 2]);
        if let (Some(nn), Some(nf)) = (nn, nf) {
            let (na, nb) = if out.nside == 0 {
                (nn.as_vec(), nf.as_vec())
            } else {
                (nf.as_vec(), nn.as_vec())
            };
            if (b.p - a.p).dot(na.cross(nb)) < 0.0 {
                vs.reverse();
                ends.reverse();
            }
        }
    }
    let at = vs.first().map_or(Point3::new(0.0, 0.0, 0.0), |v| v.p);
    let bad = |_| unresolved_at(at, 0.0);
    let mut knots = vec![0.0, 0.0];
    let mut t = 0.0_f64;
    for w in vs.windows(2) {
        if let [a, b] = w {
            t += (b.p - a.p).length();
            knots.push(t);
        }
    }
    knots.push(t);
    let kv = KnotVector::new(1, knots).map_err(bad)?;
    let p3: Vec<[f64; 3]> = vs.iter().map(|v| [v.p.x, v.p.y, v.p.z]).collect();
    let pn: Vec<[f64; 2]> = vs.iter().map(|v| [v.uv.x, v.uv.y]).collect();
    let pf: Vec<[f64; 2]> = vs.iter().map(|v| [v.q.x, v.q.y]).collect();
    let curve = NurbsCurve::new(kv.clone(), p3, None).map_err(bad)?;
    let cn = NurbsCurve::new(kv.clone(), pn, None).map_err(bad)?;
    let cf = NurbsCurve::new(kv, pf, None).map_err(bad)?;
    let reach = vs.iter().fold(0.0_f64, |r, v| {
        r.max(v.p.x.abs()).max(v.p.y.abs()).max(v.p.z.abs())
    });
    let round = up(gamma(64) * reach);
    let err = vs.iter().fold(0.0_f64, |e, v| e.max(v.err));
    let step = |f: &dyn Fn(&Placed) -> Point2| {
        vs.windows(2).fold(0.0_f64, |m, w| match w {
            [a, b] => {
                let d = f(b) - f(a);
                m.max(d.x.abs() + d.y.abs())
            }
            _ => m,
        })
    };
    let hn = step(&|v| v.uv);
    let on_n = up(0.125 * hn * hn * chain.d2 + round);
    let qs: Vec<Point2> = vs.iter().map(|v| v.q).collect();
    let d2f = field_d2(out.fs, out.fe, &qs).ok_or_else(|| unresolved_at(at, 0.0))?;
    let residual = vs.iter().fold(0.0_f64, |e, v| e.max(v.residual));
    let hf = step(&|v| v.q);
    let on_f = up(residual + 0.125 * hf * hf * d2f + round);
    let to_intersection = up(err + chain.sag + round);
    let (pa, pb, oa, ob) = if out.nside == 0 {
        (cn, cf, on_n, on_f)
    } else {
        (cf, cn, on_f, on_n)
    };
    let ends = match ends {
        [a, b] if closed && matches!((a, b), (End::Rim(_), End::Rim(_))) => Ends::Closed,
        [start, end] => Ends::Open { start, end },
    };
    Ok(Some(Branch {
        curve: Arc::new(curve),
        pcurves: [Curve2::Nurbs(Arc::new(pa)), Curve2::Nurbs(Arc::new(pb))],
        contact: Contact::Transversal,
        ends,
        certificate: Certificate {
            to_intersection: Length::from_mm(to_intersection),
            on_surface: [
                OnSurface::Within(Length::from_mm(oa)),
                OnSurface::Within(Length::from_mm(ob)),
            ],
        },
    }))
}

/// Where the surfaces meet, by the field path (module docs), when a side has an implicit
/// form; `Ok(None)` when neither has one.
pub(crate) fn trace(
    found: &Candidates<'_>,
    surfaces: [&Surface; 2],
    extents: [&Extent2d; 2],
    floor: f64,
    budget: &mut Budget,
    stop: &Stop,
) -> Result<Option<Traced>, SsiError> {
    let Some((f, form)) = field_side(surfaces, extents) else {
        return Ok(None);
    };
    let nside = 1 - f;
    let ([sa, sb], [ea, eb]) = (surfaces, extents);
    let (ns, fs, ne, fe) = if nside == 0 {
        (sa, sb, ea, eb)
    } else {
        (sb, sa, eb, ea)
    };
    let tol = found.tolerance();
    // The pieces of the net side the pairs name, in order of first appearance, each with
    // its partners' boxes.
    let mut order: Vec<(Extent2d, Extent3d, Vec<Extent3d>)> = Vec::new();
    let mut at: BTreeMap<[u64; 4], usize> = BTreeMap::new();
    for pair in found.pairs() {
        let (np, fp) = if nside == 0 {
            (pair.a, pair.b)
        } else {
            (pair.b, pair.a)
        };
        let key = [
            np.uv.x_min().to_bits(),
            np.uv.y_min().to_bits(),
            np.uv.x_max().to_bits(),
            np.uv.y_max().to_bits(),
        ];
        let k = *at.entry(key).or_insert_with(|| {
            order.push((np.uv, np.bbox, Vec::new()));
            order.len() - 1
        });
        if let Some(c) = order.get_mut(k) {
            c.2.push(fp.bbox);
        }
    }
    let field_slack = derivative_bounds(fs, fe).map_or(0.0, |([ju, jv], _)| {
        let j = ju.max(jv);
        if j > 0.0 {
            (Ivl::from(tol) / Ivl::from(j)).lo
        } else {
            0.0
        }
    });
    let mut arcs = Vec::new();
    let mut ports = BTreeMap::new();
    let mut overlaps = Vec::new();
    for (id, (uv, bbox, partners)) in order.into_iter().enumerate() {
        let refuse = || unresolved(&bbox);
        let net = Net::of(ns, &uv).ok_or_else(refuse)?;
        let g = form.field(&net).ok_or_else(refuse)?;
        let wk = net.weight_power(form.degree()).ok_or_else(refuse)?;
        let b = bounds(&form, &bbox, net.delta, tol).ok_or_else(refuse)?;
        let (j, d2) = derivative_bounds(ns, &uv).ok_or_else(refuse)?;
        let cell = Cell {
            id,
            ns,
            fs,
            fe,
            uv,
            partners,
            tol,
            floor,
            g,
            b,
            j,
            d2,
            field_slack,
            rim: [
                uv.x_min() <= ne.x_min(),
                uv.x_max() >= ne.x_max(),
                uv.y_min() <= ne.y_min(),
                uv.y_max() >= ne.y_max(),
            ],
        };
        let o = trace_cell(&cell, wk, budget, stop)?;
        arcs.extend(o.arcs);
        ports.extend(o.ports);
        overlaps.extend(o.overlaps);
    }
    let chains = link(ns, ne, &arcs, &ports)?;
    let out = Out { ns, fs, fe, nside };
    let mut traced = Traced::default();
    for chain in &chains {
        if chain.verts.len() < 2 {
            continue;
        }
        for (run, cuts, closed) in runs(&out, chain, &mut traced.nodes, budget, stop)? {
            if let Some(b) = branch(&out, run, cuts, closed, chain)? {
                traced.branches.push(b);
            }
        }
    }
    traced.coincident = overlaps
        .into_iter()
        .map(|o| {
            if nside == 0 {
                o
            } else {
                Overlap {
                    on_a: o.on_b,
                    on_b: o.on_a,
                }
            }
        })
        .collect();
    Ok(Some(traced))
}
