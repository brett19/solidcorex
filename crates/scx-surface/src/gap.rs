// Written for solidcorex. Apache-2.0.
//! Certified gaps (research 74 §17): how far a curve strays from a surface, a pcurve's image
//! from its edge's curve, a vertex from a curve's end and one curve from another, each as a
//! [`Bracket`] that holds the true gap, never a sampled guess (74 R2).
//!
//! Every gap is a largest distance over a parameter range, `sup_t d(t)`, found by one branch
//! and bound over pieces of the range: a max-heap of pieces keyed by an upper bound on `d`
//! over the piece, the lower bound the largest certified `d` at a piece end. A piece's upper
//! bound is the chord bound: for `g` with a bounded second derivative on `[a, b]`,
//! `|g(t)| <= max(|g(a)|, |g(b)|) + (b - a)^2 / 8 sup |g''|`, the second derivative enclosed
//! by forward differentiation in intervals ([`Jet2`]), and the smaller of that and the
//! direct interval enclosure of `|g|` over the piece. Pieces never straddle a NURBS knot,
//! where the second derivative may not be bounded. The search ends when the [`Aim`] is met;
//! otherwise it is [`GapError::Undecided`] with the bracket it reached: the budget ran out
//! (74 R6), the caller stopped it, or the pieces cannot be cut finer in floating point.
//!
//! The gap of a curve from a surface is from the kind's whole surface: the plane, the
//! cylinder and the cone's nappe unbounded, the sphere and torus whole; the face's trim is
//! not this crate's (it sits below the Brep). The distance is the kind's closed form in its
//! own frame (`z` for a plane, `|p| - r` for a sphere, `rho - r` for a cylinder, the torus's
//! tube, the cone's generator or apex), so `d` is smooth where the curve is. A line, circle
//! or ellipse is first written as a polynomial or trigonometric series in its own parameter
//! with interval coefficients, and the squares the closed forms need are taken on the
//! coefficients: a circle coaxial with a cylinder is then a constant to rounding, and its
//! bracket closes at once. A pcurve shares its edge's parameter, so the pcurve gap is
//! `sup_t |S(p(t)) - C(t)|` at the same `t`, and it is an upper bound on the edge's gap from
//! the face too (the pcurve lies in it).
//!
//! | solidcore at the pin | here |
//! |---|---|
//! | `distance_bound::surface_distance_lower_bound`, `face_distance_lower_bound`: a lower bound on a point's distance to a surface piece by Bezier hulls | [`point_curve_gap`] for curves (a bracket); a point to an analytic surface is its closed form ([`Surface::invert`]); to a NURBS surface, [`Surface::invert`]'s certified nearest |
//! | `curve_surface_gap::curve_face_gap_bound(_within)`: `Option<f64>`, an upper bound through a 257-point correspondence and the chord bound, `None` when it gives up | [`curve_surface_gap`]: a bracket to the kind's surface, or a typed [`GapError`]; the face's gap is bounded above by [`pcurve_edge_gap`] |
//! | `curve_surface_gap::trim_same_parameter_gap_bound_within` | [`pcurve_edge_gap`] |
//! | `curve_gap::curve_face_gap_bound` (ops' `GapCurve` wrapper) | [`curve_surface_gap`] on a [`Curve`] directly |
//! | none: vertex distances are sampled in the checker | [`vertex_curve_gap`] |
//! | none: curve-to-curve distance (the stitcher's pairing) by samples | [`curve_curve_gap`]: the one-sided Hausdorff distance, bracketed |
//! | `seam_bound::seam_angle_*`: a seam's largest normal angle | not here yet (74 §17) |
//! | `MAX_PIECES = 1 << 15`, `MAX_PATCHES = 4096` | the caller's [`Budget`]: one unit per point evaluated |
//!
//! NURBS surfaces are [`GapError::Pending`]: their point distance is [`Surface::invert`]'s, but
//! the bound over a piece (the chain rule through `partials_over_piece`) is not built.

use core::cmp::Ordering;
use core::fmt;
use core::ops::{Add, Div, Mul, Sub};
use std::collections::BinaryHeap;

use scx_curve::arcs::Arcs;
use scx_curve::{Curve, Curve2};
use scx_num::budget::{Budget, Exhausted, Stop};
use scx_num::error::ErrorKind;
use scx_num::extent1d::Extent1d;
use scx_num::extent2d::Extent2d;
use scx_num::interval::{Ivl, round_down, round_up};
use scx_num::ring::Ring;
use scx_num::trig::Trig;
use scx_num::vector2d::Point2;
use scx_num::vector3d::{Point3, Vec3};
use scx_nurbs::Approach;

use crate::common::Axes;
use crate::surface::Surface;
use crate::{cone, sphere, torus};

/// Pieces each knot span or range starts as.
const FIRST: usize = 4;

/// An enclosure of a gap: the true gap is at least `lower` and at most `upper`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Bracket {
    lower: f64,
    upper: f64,
}

impl Bracket {
    /// The bracket `[lower, upper]` of a distance a caller enclosed by its own means (an
    /// interval box): `None` unless `0 <= lower <= upper`, `lower` finite (`upper` may be
    /// infinite). It is only as certified as the caller's enclosure.
    #[must_use]
    pub fn new(lower: f64, upper: f64) -> Option<Self> {
        (lower.is_finite() && lower >= 0.0 && upper >= lower).then_some(Self { lower, upper })
    }

    /// The gap is at least this.
    #[must_use]
    pub const fn lower(&self) -> f64 {
        self.lower
    }

    /// The gap is at most this.
    #[must_use]
    pub const fn upper(&self) -> f64 {
        self.upper
    }

    /// `upper - lower`.
    #[must_use]
    pub fn width(&self) -> f64 {
        self.upper - self.lower
    }

    /// Whether the gap exceeds `allowance`: `Some(true)` when the lower bound is above it,
    /// `Some(false)` when the upper bound is not, `None` when the bracket straddles it.
    #[must_use]
    pub fn exceeds(&self, allowance: f64) -> Option<bool> {
        if self.lower > allowance {
            Some(true)
        } else if self.upper <= allowance {
            Some(false)
        } else {
            None
        }
    }
}

/// When a gap search is done.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Aim {
    /// When `upper - lower` is at most this (finite, above zero).
    Width(f64),
    /// When the bracket decides [`Bracket::exceeds`] for this allowance (finite, not
    /// negative): the check's question. A gap equal to the allowance is never decided, so
    /// the search then ends only on the budget or on [`Short::Precision`]; a caller with
    /// an unlimited budget asks [`Aim::Decide`].
    Against(f64),
    /// As [`Aim::Against`], or when the bracket still straddles the allowance but is at
    /// most `width` wide: the gap is then the allowance to within `width`, and the caller
    /// reads [`Bracket::exceeds`] as `None` (74 §28). `allowance` finite and not negative,
    /// `width` finite and above zero.
    Decide {
        /// The allowance.
        allowance: f64,
        /// The width at which a straddling bracket ends the search.
        width: f64,
    },
}

impl Aim {
    fn check(self) -> Result<Self, GapError> {
        match self {
            Self::Width(w) if w.is_finite() && w > 0.0 => Ok(self),
            Self::Against(a) if a.is_finite() && a >= 0.0 => Ok(self),
            Self::Decide { allowance, .. } if !(allowance.is_finite() && allowance >= 0.0) => {
                Err(GapError::BadAim { value: allowance })
            }
            Self::Decide { width, .. } if !(width.is_finite() && width > 0.0) => {
                Err(GapError::BadAim { value: width })
            }
            Self::Decide { .. } => Ok(self),
            Self::Width(value) | Self::Against(value) => Err(GapError::BadAim { value }),
        }
    }

    fn met(self, b: Bracket) -> bool {
        match self {
            Self::Width(w) => b.width() <= w,
            Self::Against(a) => b.exceeds(a).is_some(),
            Self::Decide { allowance, width } => {
                b.exceeds(allowance).is_some() || b.width() <= width
            }
        }
    }

    /// The width an inner search (a point's distance to a curve) aims for.
    fn inner(self, scale: f64) -> f64 {
        let floor = 1e-12 * (1.0 + scale);
        match self {
            Self::Width(w) => (w / 4.0).max(floor),
            Self::Against(a) => (a * 1e-3).max(floor),
            Self::Decide { allowance, width } => (allowance * 1e-3).min(width / 4.0).max(floor),
        }
    }
}

/// Why a gap search ended before its [`Aim`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum Short {
    /// The budget ran out.
    Exhausted(Exhausted),
    /// The caller stopped it.
    Stopped,
    /// The pieces that keep the bracket open cannot be cut finer in floating point.
    Precision,
}

/// A geometry whose gap bound is not built yet.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum Pending {
    /// A NURBS surface: the bound over a piece through its partials.
    NurbsSurface,
}

/// Why a gap has no bracket that meets its aim.
#[derive(Debug, Clone, Copy, PartialEq)]
#[non_exhaustive]
pub enum GapError {
    /// The search ended first; `reached` holds the gap.
    Undecided {
        /// The bracket reached.
        reached: Bracket,
        /// Why it ended.
        why: Short,
    },
    /// The geometry's bound is not built (74 analytic first).
    Pending(Pending),
    /// The range is not finite, or the geometry over it cannot be enclosed (a value
    /// overflows).
    NotFinite,
    /// The aim's value is not finite, or not above zero (a width) or negative (an
    /// allowance).
    BadAim {
        /// The value given.
        value: f64,
    },
}

impl GapError {
    /// The cause's kind (research 62 §10).
    #[must_use]
    pub const fn kind(&self) -> ErrorKind {
        match self {
            Self::Undecided {
                why: Short::Exhausted(_),
                ..
            } => ErrorKind::Exhausted,
            Self::Undecided {
                why: Short::Stopped,
                ..
            } => ErrorKind::Cancelled,
            Self::Undecided {
                why: Short::Precision,
                ..
            }
            | Self::Pending(_) => ErrorKind::Unsupported,
            Self::NotFinite | Self::BadAim { .. } => ErrorKind::Refused,
        }
    }

    /// A stable name.
    #[must_use]
    pub const fn name(&self) -> &'static str {
        match self {
            Self::Undecided {
                why: Short::Exhausted(_),
                ..
            } => "gap_exhausted",
            Self::Undecided {
                why: Short::Stopped,
                ..
            } => "gap_stopped",
            Self::Undecided {
                why: Short::Precision,
                ..
            } => "gap_precision",
            Self::Pending(Pending::NurbsSurface) => "gap_pending_nurbs_surface",
            Self::NotFinite => "gap_not_finite",
            Self::BadAim { .. } => "gap_bad_aim",
        }
    }
}

impl fmt::Display for GapError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match *self {
            Self::Undecided { reached, why } => {
                let why = match why {
                    Short::Exhausted(_) => "the budget ran out",
                    Short::Stopped => "it was stopped",
                    Short::Precision => "floating point cannot cut finer",
                };
                write!(
                    f,
                    "the gap is undecided in [{}, {}]: {why}",
                    reached.lower, reached.upper
                )
            }
            Self::Pending(Pending::NurbsSurface) => {
                f.write_str("a gap to a NURBS surface is not bounded yet")
            }
            Self::NotFinite => f.write_str("the range or the geometry over it is not finite"),
            Self::BadAim { value } => {
                write!(f, "the aim {value} is not a valid width or allowance")
            }
        }
    }
}

impl std::error::Error for GapError {}

// ---------------------------------------------------------------------------------------
// Second-order forward differentiation in intervals.

/// A value and its first two derivatives in one variable, each enclosed over an interval
/// of it: a [`Ring`], so the kinds' `eval` runs in it (the chain rule for free).
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Jet2 {
    pub(crate) f: Ivl,
    pub(crate) d1: Ivl,
    pub(crate) d2: Ivl,
}

impl Jet2 {
    fn new(f: Ivl, d1: Ivl, d2: Ivl) -> Self {
        Self { f, d1, d2 }
    }

    /// Encloses both.
    fn hull(self, o: Self) -> Self {
        Self::new(self.f.hull(o.f), self.d1.hull(o.d1), self.d2.hull(o.d2))
    }

    /// A constant enclosed by `f`.
    fn lift_ivl(f: Ivl) -> Self {
        Self::new(f, Ivl::from(0.0), Ivl::from(0.0))
    }

    fn scale(self, k: f64) -> Self {
        let k = Ivl::from(k);
        Self::new(self.f * k, self.d1 * k, self.d2 * k)
    }

    /// `sqrt(self)`: the derivatives are whole where the value may be zero.
    fn sqrt(self) -> Self {
        let s = self.f.sqrt();
        let two_s = Ivl::from(2.0) * s;
        let d1 = self.d1 / two_s;
        let d2 = (self.d2 - Ivl::from(2.0) * d1 * d1) / two_s;
        Self::new(s, d1, d2)
    }
}

impl Add for Jet2 {
    type Output = Self;
    fn add(self, b: Self) -> Self {
        Self::new(self.f + b.f, self.d1 + b.d1, self.d2 + b.d2)
    }
}

impl Sub for Jet2 {
    type Output = Self;
    fn sub(self, b: Self) -> Self {
        Self::new(self.f - b.f, self.d1 - b.d1, self.d2 - b.d2)
    }
}

impl Mul for Jet2 {
    type Output = Self;
    fn mul(self, b: Self) -> Self {
        let two = Ivl::from(2.0);
        Self::new(
            self.f * b.f,
            self.d1 * b.f + self.f * b.d1,
            self.d2 * b.f + two * self.d1 * b.d1 + self.f * b.d2,
        )
    }
}

impl Div for Jet2 {
    type Output = Self;
    fn div(self, b: Self) -> Self {
        let q = self.f / b.f;
        let d1 = (self.d1 - q * b.d1) / b.f;
        let d2 = (self.d2 - Ivl::from(2.0) * d1 * b.d1 - q * b.d2) / b.f;
        Self::new(q, d1, d2)
    }
}

impl Ring for Jet2 {
    fn lift(d: f64) -> Self {
        Self::new(Ivl::from(d), Ivl::from(0.0), Ivl::from(0.0))
    }
}

/// `[x^2]`, not below zero.
fn sq(x: Ivl) -> Ivl {
    let (m, g) = (x.mig(), x.mag());
    Ivl::new(round_down(m * m).max(0.0), round_up(g * g))
}

/// `[|v|]` of an interval vector.
fn norm(v: [Ivl; 3]) -> Ivl {
    (sq(v[0]) + sq(v[1]) + sq(v[2])).sqrt()
}

/// `|g|` over a piece by the chord bound and the direct enclosure, the smaller; infinite
/// when neither is finite.
fn chord(ends: (Ivl, Ivl), d2: f64, over: Ivl, h: f64) -> f64 {
    let ends = ends.0.mag().max(ends.1.mag());
    let bound = round_up(ends + round_up(round_up(h * h) * d2 / 8.0));
    let direct = over.mag();
    let b = bound.min(direct);
    if b.is_nan() { f64::INFINITY } else { b }
}

/// `|D|` over a piece from the jets of a vector `D` over it and `|D|` at its ends: the chord
/// bound on `D` itself, the chord bound on `q = |D|^2` (whose second derivative
/// `2 (|D'|^2 + D . D'')` stays small where `|D|` is near constant though `D` turns), and
/// the direct enclosure, the smallest.
fn vector_bound(d: [Jet2; 3], ends: (Ivl, Ivl), h: f64) -> f64 {
    let natural = norm([d[0].f, d[1].f, d[2].f]);
    let by_d = chord(ends, norm([d[0].d2, d[1].d2, d[2].d2]).mag(), natural, h);
    let q = d[0] * d[0] + d[1] * d[1] + d[2] * d[2];
    let end = ends.0.mag().max(ends.1.mag());
    let qb = round_up(round_up(end * end) + round_up(round_up(h * h) * q.d2.mag() / 8.0));
    let by_q = round_up(qb.sqrt());
    if by_q.is_nan() { by_d } else { by_d.min(by_q) }
}

fn jets3(p: Point3<Ivl>, d1: Vec3<Ivl>, d2: Vec3<Ivl>) -> [Jet2; 3] {
    [
        Jet2::new(p.x, d1.x, d2.x),
        Jet2::new(p.y, d1.y, d2.y),
        Jet2::new(p.z, d1.z, d2.z),
    ]
}

// ---------------------------------------------------------------------------------------
// The search.

/// What a search maximizes: `d` at a point (with a mark the piece bound may use) and an
/// upper bound on `d` over a piece between two marked points.
trait Field {
    type Mark: Copy;
    fn at(&self, t: f64, budget: &mut Budget, stop: &Stop) -> Result<(Bracket, Self::Mark), Short>;
    fn over(&self, a: (f64, Self::Mark), b: (f64, Self::Mark)) -> f64;
}

#[derive(Clone, Copy)]
struct Piece<M> {
    upper: f64,
    a: (f64, M),
    b: (f64, M),
}

impl<M> PartialEq for Piece<M> {
    fn eq(&self, o: &Self) -> bool {
        self.cmp(o) == Ordering::Equal
    }
}
impl<M> Eq for Piece<M> {}
impl<M> PartialOrd for Piece<M> {
    fn partial_cmp(&self, o: &Self) -> Option<Ordering> {
        Some(self.cmp(o))
    }
}
impl<M> Ord for Piece<M> {
    /// The larger bound first, then the earlier piece: deterministic.
    fn cmp(&self, o: &Self) -> Ordering {
        self.upper
            .total_cmp(&o.upper)
            .then_with(|| o.a.0.total_cmp(&self.a.0))
    }
}

/// `sup_t d(t)` over the pieces between consecutive `cuts`, to `aim`, and the `t` of the
/// largest lower bound at a point. The bracket is of `d` as the field gives it (a negated
/// distance for a nearest): the callers clamp.
fn search<F: Field>(
    field: &F,
    cuts: &[f64],
    aim: Aim,
    budget: &mut Budget,
    stop: &Stop,
) -> Result<(Bracket, f64), GapError> {
    let mut lower = (f64::NEG_INFINITY, cuts.first().copied().unwrap_or(0.0));
    let mut upper_pts = f64::NEG_INFINITY;
    let mut stuck = f64::NEG_INFINITY;
    let mut heap = BinaryHeap::new();
    let mut marks = Vec::with_capacity(cuts.len());
    for &t in cuts {
        let (b, m) = field
            .at(t, budget, stop)
            .map_err(|why| GapError::Undecided {
                reached: Bracket {
                    lower: lower.0,
                    upper: f64::INFINITY,
                },
                why,
            })?;
        if b.lower > lower.0 {
            lower = (b.lower, t);
        }
        upper_pts = upper_pts.max(b.upper);
        marks.push((t, m));
    }
    for (&a, &b) in marks.iter().zip(marks.iter().skip(1)) {
        heap.push(Piece {
            upper: field.over(a, b),
            a,
            b,
        });
    }
    loop {
        let top = heap
            .peek()
            .map_or(f64::NEG_INFINITY, |p| p.upper)
            .max(upper_pts)
            .max(stuck)
            .max(lower.0);
        let now = Bracket {
            lower: lower.0,
            upper: top,
        };
        if aim.met(now) {
            return Ok((now, lower.1));
        }
        let undecided = |why| GapError::Undecided { reached: now, why };
        let Some(p) = heap.pop() else {
            return Err(undecided(Short::Precision));
        };
        if p.upper <= stuck {
            return Err(undecided(Short::Precision));
        }
        let (a, b) = (p.a.0, p.b.0);
        let m = a + (b - a) / 2.0;
        if !(m > a && m < b) {
            stuck = stuck.max(p.upper);
            continue;
        }
        stop.check().map_err(|_| undecided(Short::Stopped))?;
        let (bm, mark) = field.at(m, budget, stop).map_err(undecided)?;
        if bm.lower > lower.0 {
            lower = (bm.lower, m);
        }
        upper_pts = upper_pts.max(bm.upper);
        for (x, y) in [(p.a, (m, mark)), ((m, mark), p.b)] {
            let upper = field.over(x, y);
            if upper > lower.0 {
                heap.push(Piece { upper, a: x, b: y });
            }
        }
    }
}

/// A distance's bracket from [`search`], its lower bound not below zero.
fn distance(r: Result<(Bracket, f64), GapError>) -> Result<Bracket, GapError> {
    let clamp = |b: Bracket| Bracket {
        lower: b.lower.max(0.0),
        upper: b.upper,
    };
    match r {
        Ok((b, _)) => Ok(clamp(b)),
        Err(GapError::Undecided { reached, why }) => Err(GapError::Undecided {
            reached: clamp(reached),
            why,
        }),
        Err(e) => Err(e),
    }
}

/// `[lo, hi]` cut at the given breaks strictly inside it, each part in [`FIRST`].
fn cuts_of(lo: f64, hi: f64, breaks: &[f64]) -> Vec<f64> {
    let mut ends = vec![lo];
    ends.extend(breaks.iter().copied().filter(|&k| k > lo && k < hi));
    ends.push(hi);
    ends.sort_by(f64::total_cmp);
    ends.dedup();
    let mut out = vec![lo];
    for (&a, &b) in ends.iter().zip(ends.iter().skip(1)) {
        for i in 1..=FIRST {
            let x = if i == FIRST {
                b
            } else {
                a + (b - a) * (i as f64 / FIRST as f64)
            };
            if x > *out.last().unwrap_or(&lo) {
                out.push(x);
            }
        }
    }
    out
}

/// The knots of a NURBS curve (where its second derivative may jump), else none.
fn knots3(c: &Curve) -> Vec<f64> {
    match c {
        Curve::Nurbs(n) => n.knots().to_vec(),
        _ => Vec::new(),
    }
}

fn knots2(c: &Curve2) -> Vec<f64> {
    match c {
        Curve2::Nurbs(n) => n.knots().to_vec(),
        _ => Vec::new(),
    }
}

fn range(over: &Extent1d) -> Result<(f64, f64), GapError> {
    let (lo, hi) = (over.min(), over.max());
    if lo.is_finite() && hi.is_finite() {
        Ok((lo, hi))
    } else {
        Err(GapError::NotFinite)
    }
}

fn ivl3(p: Point3<Ivl>) -> [Ivl; 3] {
    [p.x, p.y, p.z]
}

fn ivl3v(p: Vec3<Ivl>) -> [Ivl; 3] {
    [p.x, p.y, p.z]
}

fn sub3(a: [Ivl; 3], b: [Ivl; 3]) -> [Ivl; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

// ---------------------------------------------------------------------------------------
// A curve in a surface's frame.

/// A coordinate as a series in the curve's parameter, interval coefficients: `k0 + k1 t +
/// k2 t^2`, or `k0 + k1 cos t + k2 sin t + k3 cos 2t + k4 sin 2t`.
#[derive(Clone, Copy, Debug)]
enum Series {
    Poly([Ivl; 3]),
    Trig([Ivl; 5]),
}

impl Series {
    fn jet(&self, t: Ivl) -> Jet2 {
        let (two, four) = (Ivl::from(2.0), Ivl::from(4.0));
        match *self {
            Self::Poly(k) => Jet2::new(
                k[0] + (k[1] + k[2] * t) * t,
                k[1] + two * k[2] * t,
                two * k[2],
            ),
            Self::Trig(k) => {
                let (s1, c1) = t.sin_cos();
                let (s2, c2) = (two * t).sin_cos();
                Jet2::new(
                    k[0] + k[1] * c1 + k[2] * s1 + k[3] * c2 + k[4] * s2,
                    k[2] * c1 - k[1] * s1 + two * (k[4] * c2 - k[3] * s2),
                    Ivl::from(0.0) - (k[1] * c1 + k[2] * s1 + four * (k[3] * c2 + k[4] * s2)),
                )
            }
        }
    }

    /// The product of two first-degree series of the same basis.
    fn times(&self, b: &Self) -> Self {
        let h = Ivl::from(0.5);
        match (*self, *b) {
            (Self::Poly(p), Self::Poly(q)) => {
                Self::Poly([p[0] * q[0], p[0] * q[1] + p[1] * q[0], p[1] * q[1]])
            }
            (Self::Trig(p), Self::Trig(q)) => Self::Trig([
                p[0] * q[0] + h * (p[1] * q[1] + p[2] * q[2]),
                p[0] * q[1] + p[1] * q[0],
                p[0] * q[2] + p[2] * q[0],
                h * (p[1] * q[1] - p[2] * q[2]),
                h * (p[1] * q[2] + p[2] * q[1]),
            ]),
            _ => unreachable!("one curve's series share their basis"),
        }
    }

    fn plus(&self, b: &Self) -> Self {
        match (*self, *b) {
            (Self::Poly(p), Self::Poly(q)) => Self::Poly([p[0] + q[0], p[1] + q[1], p[2] + q[2]]),
            (Self::Trig(p), Self::Trig(q)) => Self::Trig([
                p[0] + q[0],
                p[1] + q[1],
                p[2] + q[2],
                p[3] + q[3],
                p[4] + q[4],
            ]),
            _ => unreachable!("one curve's series share their basis"),
        }
    }
}

/// The local coordinates of a curve point and the squares the closed forms take.
#[derive(Clone, Copy, Debug)]
struct Loc {
    z: Jet2,
    /// `x^2 + y^2`.
    qxy: Jet2,
    /// `x^2 + y^2 + z^2`.
    q: Jet2,
}

/// A curve in a surface's frame: its series where the kind has one, else its jet.
enum Source<'a> {
    Series { z: Series, qxy: Series, q: Series },
    Jet { curve: &'a Curve, axes: Axes },
}

impl<'a> Source<'a> {
    fn new(curve: &'a Curve, axes: Axes) -> Self {
        let dot = |v: [Ivl; 3], w: Vec3| {
            v[0] * Ivl::from(w.x) + v[1] * Ivl::from(w.y) + v[2] * Ivl::from(w.z)
        };
        let o = [
            Ivl::from(axes.o.x),
            Ivl::from(axes.o.y),
            Ivl::from(axes.o.z),
        ];
        let at = |t: Ivl| ivl3(curve.point_over(t));
        // The coefficient vectors, each an enclosure from the curve's own formula at exact or
        // enclosed parameters (a line is affine, a circle or ellipse `c + a cos + b sin`).
        let coeffs: Option<(bool, [[Ivl; 3]; 3])> = match curve {
            Curve::Line(_) => {
                let (p0, p1) = (at(Ivl::from(0.0)), at(Ivl::from(1.0)));
                Some((false, [sub3(p0, o), sub3(p1, p0), [Ivl::from(0.0); 3]]))
            }
            Curve::Circle(_) | Curve::Ellipse(_) => {
                let pi = Ivl::new(core::f64::consts::PI, round_up(core::f64::consts::PI));
                let half = Ivl::new(
                    core::f64::consts::FRAC_PI_2,
                    round_up(core::f64::consts::FRAC_PI_2),
                );
                let (p0, pp, ph) = (at(Ivl::from(0.0)), at(pi), at(half));
                let h = Ivl::from(0.5);
                let c = [
                    h * (p0[0] + pp[0]),
                    h * (p0[1] + pp[1]),
                    h * (p0[2] + pp[2]),
                ];
                Some((true, [sub3(c, o), sub3(p0, c), sub3(ph, c)]))
            }
            _ => None,
        };
        let Some((trig, [c, a, b])) = coeffs else {
            return Self::Jet { curve, axes };
        };
        let along = |w: Vec3| {
            let k = [dot(c, w), dot(a, w), dot(b, w)];
            if trig {
                let z = Ivl::from(0.0);
                Series::Trig([k[0], k[1], k[2], z, z])
            } else {
                Series::Poly(k)
            }
        };
        let (x, y, z) = (along(axes.x), along(axes.y), along(axes.z));
        let qxy = x.times(&x).plus(&y.times(&y));
        let q = qxy.plus(&z.times(&z));
        Self::Series { z, qxy, q }
    }

    fn loc(&self, t: Ivl) -> Loc {
        match self {
            Self::Series { z, qxy, q } => Loc {
                z: z.jet(t),
                qxy: qxy.jet(t),
                q: q.jet(t),
            },
            Self::Jet { curve, axes } => {
                let j = curve.jet_over::<Ivl, 2>(t, Approach::Above);
                let o = [
                    Ivl::from(axes.o.x),
                    Ivl::from(axes.o.y),
                    Ivl::from(axes.o.z),
                ];
                let d = sub3(ivl3(j.point), o);
                let (d1, d2) = (ivl3v(j.d[0]), ivl3v(j.d[1]));
                let along = |w: Vec3| {
                    let w = [Ivl::from(w.x), Ivl::from(w.y), Ivl::from(w.z)];
                    let dot = |v: [Ivl; 3]| v[0] * w[0] + v[1] * w[1] + v[2] * w[2];
                    Jet2::new(dot(d), dot(d1), dot(d2))
                };
                let (x, y, z) = (along(axes.x), along(axes.y), along(axes.z));
                let qxy = x * x + y * y;
                Loc {
                    z,
                    qxy,
                    q: qxy + z * z,
                }
            }
        }
    }
}

/// A kind's distance in its frame.
#[derive(Clone, Copy, Debug)]
enum Kind {
    Plane,
    Sphere {
        r: f64,
    },
    Cylinder {
        r: f64,
    },
    /// `rho = r + k v`.
    Cone {
        r: f64,
        k: f64,
    },
    Torus {
        major: f64,
        minor: f64,
    },
}

/// A distance as the kind gives it.
enum Dist {
    /// `d = |g|`.
    Exact(Jet2),
    /// The cone: `d = |perp|` where the foot `w > 0` is on the nappe, else `apex`; always
    /// `|perp| <= d <= apex`.
    Cone { perp: Jet2, apex: Jet2, w: Ivl },
}

impl Kind {
    fn dist(self, l: &Loc) -> Dist {
        match self {
            Self::Plane => Dist::Exact(l.z),
            Self::Sphere { r } => Dist::Exact(l.q.sqrt() - Jet2::lift(r)),
            Self::Cylinder { r } => Dist::Exact(l.qxy.sqrt() - Jet2::lift(r)),
            Self::Torus { major, minor } => {
                let rho = l.qxy.sqrt();
                let tube = l.q - rho.scale(2.0 * major) + Jet2::lift(major * major);
                Dist::Exact(tube.sqrt() - Jet2::lift(minor))
            }
            Self::Cone { r, k } => {
                let rho = l.qxy.sqrt();
                let s = (1.0 + k * k).sqrt();
                let perp = (rho - Jet2::lift(r) - l.z.scale(k)) / Jet2::lift(s);
                let v = ((rho - Jet2::lift(r)).scale(k) + l.z) / Jet2::lift(k * k + 1.0);
                let w = Ivl::from(r) + Ivl::from(k) * v.f;
                let a = -r / k;
                let apex = (l.q - l.z.scale(2.0 * a) + Jet2::lift(a * a)).sqrt();
                Dist::Cone { perp, apex, w }
            }
        }
    }
}

fn bracket_of(d: &Dist) -> Bracket {
    let (lower, upper) = match d {
        Dist::Exact(g) => (g.f.mig(), g.f.mag()),
        Dist::Cone { perp, apex, w } => {
            let mut lo = perp.f.mig();
            let mut hi = apex.f.mag();
            if w.lo > 0.0 {
                hi = hi.min(perp.f.mag());
            }
            if w.hi < 0.0 {
                lo = lo.max(apex.f.mig());
            }
            (lo, hi)
        }
    };
    Bracket {
        lower,
        upper: if upper.is_nan() { f64::INFINITY } else { upper },
    }
}

struct ToSurface<'a> {
    src: Source<'a>,
    kind: Kind,
}

impl Field for ToSurface<'_> {
    type Mark = ();

    fn at(&self, t: f64, budget: &mut Budget, _: &Stop) -> Result<(Bracket, ()), Short> {
        budget.charge(1).map_err(Short::Exhausted)?;
        Ok((bracket_of(&self.kind.dist(&self.src.loc(Ivl::from(t)))), ()))
    }

    fn over(&self, (a, ()): (f64, ()), (b, ()): (f64, ())) -> f64 {
        let h = b - a;
        let d = |t: Ivl| self.kind.dist(&self.src.loc(t));
        match (d(Ivl::from(a)), d(Ivl::from(b)), d(Ivl::new(a, b))) {
            (Dist::Exact(ga), Dist::Exact(gb), Dist::Exact(gi)) => {
                chord((ga.f, gb.f), gi.d2.mag(), gi.f, h)
            }
            (
                Dist::Cone {
                    perp: pa, apex: aa, ..
                },
                Dist::Cone {
                    perp: pb, apex: ab, ..
                },
                Dist::Cone {
                    perp: pi,
                    apex: ai,
                    w,
                },
            ) => {
                let apex = chord((aa.f, ab.f), ai.d2.mag(), ai.f, h);
                if w.lo > 0.0 {
                    apex.min(chord((pa.f, pb.f), pi.d2.mag(), pi.f, h))
                } else {
                    apex
                }
            }
            _ => f64::INFINITY,
        }
    }
}

/// The kind of an analytic surface and its frame's axes, or pending.
fn kind_of(s: &Surface) -> Result<(Kind, Axes), GapError> {
    Ok(match s {
        Surface::Plane(p) => (
            Kind::Plane,
            Axes::new(p.frame(), scx_num::frame::Sense::Forward),
        ),
        Surface::Sphere(k) => (Kind::Sphere { r: k.radius().mm() }, k.axes()),
        Surface::Cylinder(k) => (Kind::Cylinder { r: k.radius().mm() }, k.axes()),
        Surface::Cone(k) => (
            Kind::Cone {
                r: k.radius().mm(),
                k: k.slope(),
            },
            k.axes(),
        ),
        Surface::Torus(k) => (
            Kind::Torus {
                major: k.major().mm(),
                minor: k.minor().mm(),
            },
            k.axes(),
        ),
        Surface::Nurbs(_) => return Err(GapError::Pending(Pending::NurbsSurface)),
    })
}

/// The largest distance of `curve` over `over` from `surface` (the kind's whole surface; see
/// the module), to `aim`, one budget unit per point evaluated.
///
/// # Errors
///
/// [`GapError::Undecided`] with the bracket reached when the budget runs out, `stop` is
/// set or floating point cannot cut finer; [`GapError::Pending`] for a NURBS surface;
/// [`GapError::NotFinite`] for an infinite range; [`GapError::BadAim`].
pub fn curve_surface_gap(
    curve: &Curve,
    over: &Extent1d,
    surface: &Surface,
    aim: Aim,
    budget: &mut Budget,
    stop: &Stop,
) -> Result<Bracket, GapError> {
    let aim = aim.check()?;
    let (lo, hi) = range(over)?;
    let (kind, axes) = kind_of(surface)?;
    let field = ToSurface {
        src: Source::new(curve, axes),
        kind,
    };
    distance(search(
        &field,
        &cuts_of(lo, hi, &knots3(curve)),
        aim,
        budget,
        stop,
    ))
}

// ---------------------------------------------------------------------------------------
// A pcurve against its edge's curve.

/// The arcs of a kind's `u` and `v`, where its formula changes.
fn arcs_of(s: &Surface) -> (Option<Arcs>, Option<Arcs>) {
    match s {
        Surface::Cylinder(c) => (Some(c.arcs()), None),
        Surface::Cone(_) => (Some(cone::U), None),
        Surface::Sphere(_) => (Some(sphere::U), Some(sphere::V)),
        Surface::Torus(_) => (Some(torus::TURN), Some(torus::TURN)),
        Surface::Plane(_) | Surface::Nurbs(_) => (None, None),
    }
}

/// `i` cut at the breaks of `a` strictly inside it, each part on one arc; `None` past a few
/// (a piece that long gains nothing from the jets). A part may come from rounding alone, as
/// a seam's `u` enclosed a few ulps either side of its break.
fn on_arcs(a: Option<Arcs>, i: Ivl) -> Option<Vec<Ivl>> {
    let Some(a) = a else {
        return Some(vec![i]);
    };
    let breaks = a.breaks(i.lo, i.hi)?;
    if breaks.len() > 4 {
        return None;
    }
    let mut cuts = Vec::with_capacity(breaks.len() + 2);
    cuts.push(i.lo);
    cuts.extend(breaks);
    cuts.push(i.hi);
    Some(
        cuts.iter()
            .zip(cuts.iter().skip(1))
            .map(|(&a, &b)| Ivl::new(a, b))
            .collect(),
    )
}

fn eval_jet(s: &Surface, u: Jet2, v: Jet2, near: Point2) -> Option<[Jet2; 3]> {
    let p = match s {
        Surface::Plane(k) => k.eval(u, v, near),
        Surface::Cylinder(k) => k.eval(u, v, near),
        Surface::Cone(k) => k.eval(u, v, near),
        Surface::Sphere(k) => k.eval(u, v, near),
        Surface::Torus(k) => k.eval(u, v, near),
        Surface::Nurbs(_) => return None,
    };
    Some([p.x, p.y, p.z])
}

/// `[|S over the uv box - C over t|]`: the direct bound, through [`Surface::box_over`].
fn direct(s: &Surface, uv: Point2<Ivl>, c: [Ivl; 3]) -> Ivl {
    let b = Extent2d::from_corners(Point2::new(uv.x.lo, uv.y.lo), Point2::new(uv.x.hi, uv.y.hi))
        .and_then(|e| s.box_over(e));
    match b {
        Some(b) => {
            let (lo, hi) = (b.min(), b.max());
            norm(sub3(
                [
                    Ivl::new(lo.x, hi.x),
                    Ivl::new(lo.y, hi.y),
                    Ivl::new(lo.z, hi.z),
                ],
                c,
            ))
        }
        None => Ivl::whole(),
    }
}

struct PcurveField<'a> {
    surface: &'a Surface,
    pcurve: &'a Curve2,
    curve: &'a Curve,
    arcs: (Option<Arcs>, Option<Arcs>),
}

impl PcurveField<'_> {
    fn gap_at(&self, t: f64) -> Ivl {
        let ti = Ivl::from(t);
        direct(
            self.surface,
            self.pcurve.point_over(ti),
            ivl3(self.curve.point_over(ti)),
        )
    }
}

impl Field for PcurveField<'_> {
    /// The gap at the point, kept for the chord bound of the pieces it ends.
    type Mark = Ivl;

    fn at(&self, t: f64, budget: &mut Budget, _: &Stop) -> Result<(Bracket, Ivl), Short> {
        budget.charge(1).map_err(Short::Exhausted)?;
        let g = self.gap_at(t);
        let upper = if g.hi.is_nan() { f64::INFINITY } else { g.hi };
        Ok((
            Bracket {
                lower: g.lo.max(0.0),
                upper,
            },
            g,
        ))
    }

    fn over(&self, (a, ga): (f64, Ivl), (b, gb): (f64, Ivl)) -> f64 {
        let i = Ivl::new(a, b);
        let p = self.pcurve.jet_over::<Ivl, 2>(i, Approach::Above);
        let c = self.curve.jet_over::<Ivl, 2>(i, Approach::Above);
        let natural = direct(self.surface, p.point, ivl3(c.point)).mag();
        let natural = if natural.is_nan() {
            f64::INFINITY
        } else {
            natural
        };
        // The jets on each arc the uv box meets, hulled: at every `t` the point lies on one
        // of them, so the hull encloses `S(p(t))` and its derivatives there.
        let (Some(us), Some(vs)) = (
            on_arcs(self.arcs.0, p.point.x),
            on_arcs(self.arcs.1, p.point.y),
        ) else {
            return natural;
        };
        let mut s: Option<[Jet2; 3]> = None;
        for &ui in &us {
            for &vi in &vs {
                let u = Jet2::new(ui, p.d[0].x, p.d[1].x);
                let v = Jet2::new(vi, p.d[0].y, p.d[1].y);
                let Some(part) = eval_jet(self.surface, u, v, Point2::new(ui.lo, vi.lo)) else {
                    return natural;
                };
                s = Some(match s {
                    None => part,
                    Some(h) => [h[0].hull(part[0]), h[1].hull(part[1]), h[2].hull(part[2])],
                });
            }
        }
        let Some(s) = s else {
            return natural;
        };
        let cj = jets3(c.point, c.d[0], c.d[1]);
        let d = [s[0] - cj[0], s[1] - cj[1], s[2] - cj[2]];
        vector_bound(d, (ga, gb), b - a).min(natural)
    }
}

/// The largest distance `|S(p(t)) - C(t)|` over `over` between a pcurve's image on
/// `surface` and its edge's `curve` at the same parameter (a pcurve shares its edge's
/// parameter), to `aim`, one budget unit per point evaluated. It is also an upper bound on
/// the edge's gap from the face the pcurve lies in.
///
/// # Errors
///
/// As [`curve_surface_gap`].
pub fn pcurve_edge_gap(
    surface: &Surface,
    pcurve: &Curve2,
    curve: &Curve,
    over: &Extent1d,
    aim: Aim,
    budget: &mut Budget,
    stop: &Stop,
) -> Result<Bracket, GapError> {
    let aim = aim.check()?;
    let (lo, hi) = range(over)?;
    if matches!(surface, Surface::Nurbs(_)) {
        return Err(GapError::Pending(Pending::NurbsSurface));
    }
    let mut breaks = knots3(curve);
    breaks.extend(knots2(pcurve));
    let field = PcurveField {
        surface,
        pcurve,
        curve,
        arcs: arcs_of(surface),
    };
    distance(search(&field, &cuts_of(lo, hi, &breaks), aim, budget, stop))
}

// ---------------------------------------------------------------------------------------
// Points and curves.

/// The distance from `point` to `curve` at `t` (a vertex against its edge's end), enclosed
/// in intervals; no search, so no budget.
///
/// # Errors
///
/// [`GapError::NotFinite`] when `t` or the curve's point there is not finite.
pub fn vertex_curve_gap(point: Point3, curve: &Curve, t: f64) -> Result<Bracket, GapError> {
    if !t.is_finite() {
        return Err(GapError::NotFinite);
    }
    let c = ivl3(curve.point_over(Ivl::from(t)));
    let g = norm(sub3(
        c,
        [Ivl::from(point.x), Ivl::from(point.y), Ivl::from(point.z)],
    ));
    if g.is_finite() {
        Ok(Bracket {
            lower: g.lo.max(0.0),
            upper: g.hi,
        })
    } else {
        Err(GapError::NotFinite)
    }
}

/// The nearest distance from the points of a box `p` to `curve` over its parameters, as
/// `sup -d`: a mark is the parameter, so the nearest's place comes back with it.
struct Nearest<'a> {
    p: [Ivl; 3],
    curve: &'a Curve,
}

impl Nearest<'_> {
    fn q(&self, t: Ivl) -> (Jet2, Ivl) {
        let j = self.curve.jet_over::<Ivl, 2>(t, Approach::Above);
        let e = sub3(ivl3(j.point), self.p);
        let (d1, d2) = (ivl3v(j.d[0]), ivl3v(j.d[1]));
        let mut q = Jet2::lift(0.0);
        for ((&f, &g), &h) in e.iter().zip(&d1).zip(&d2) {
            let c = Jet2::new(f, g, h);
            q = q + c * c;
        }
        (q, norm(e))
    }
}

impl Field for Nearest<'_> {
    type Mark = ();

    fn at(&self, t: f64, budget: &mut Budget, _: &Stop) -> Result<(Bracket, ()), Short> {
        budget.charge(1).map_err(Short::Exhausted)?;
        let d = self.q(Ivl::from(t)).1;
        Ok((
            Bracket {
                lower: -d.hi,
                upper: -d.lo.max(0.0),
            },
            (),
        ))
    }

    /// `-` a lower bound on `d` over the piece: the chord bound on `q = d^2` from below,
    /// `min q >= min(q(a), q(b)) - h^2/8 sup |q''|`, and the box's distance, the larger.
    fn over(&self, (a, ()): (f64, ()), (b, ()): (f64, ())) -> f64 {
        let h = b - a;
        let (qi, di) = self.q(Ivl::new(a, b));
        let ends = self.q(Ivl::from(a)).0.f.lo.min(self.q(Ivl::from(b)).0.f.lo);
        let low_q = round_down(ends - round_up(round_up(h * h) * qi.d2.mag() / 8.0));
        let by_chord = if low_q > 0.0 {
            round_down(low_q.sqrt())
        } else {
            0.0
        };
        let low = by_chord.max(di.lo);
        if low.is_nan() { 0.0 } else { -low }
    }
}

/// The nearest distance from every point of the box `p` to `curve` over `[lo, hi]`, and the
/// parameter where its upper bound is reached. A search floating point cannot finish still
/// gives its (valid, wider) bracket.
fn nearest(
    p: [Ivl; 3],
    curve: &Curve,
    (lo, hi): (f64, f64),
    width: f64,
    budget: &mut Budget,
    stop: &Stop,
) -> Result<(Bracket, f64), Short> {
    let field = Nearest { p, curve };
    let cuts = cuts_of(lo, hi, &knots3(curve));
    let flip = |b: Bracket| Bracket {
        lower: (-b.upper).max(0.0),
        upper: -b.lower,
    };
    match search(&field, &cuts, Aim::Width(width), budget, stop) {
        Ok((b, t)) => Ok((flip(b), t)),
        Err(GapError::Undecided {
            reached,
            why: Short::Precision,
        }) => Ok((flip(reached), lo)),
        Err(GapError::Undecided { why, .. }) => Err(why),
        Err(_) => Err(Short::Precision),
    }
}

/// The distance from `point` to `curve` over `over`: the nearest, bracketed to `aim`, one
/// budget unit per point evaluated.
///
/// # Errors
///
/// As [`curve_surface_gap`] (no surface, so never pending).
pub fn point_curve_gap(
    point: Point3,
    curve: &Curve,
    over: &Extent1d,
    aim: Aim,
    budget: &mut Budget,
    stop: &Stop,
) -> Result<Bracket, GapError> {
    let aim = aim.check()?;
    let r = range(over)?;
    let p = [Ivl::from(point.x), Ivl::from(point.y), Ivl::from(point.z)];
    let width = match aim {
        Aim::Width(w) => w,
        Aim::Against(_) | Aim::Decide { .. } => {
            aim.inner(point.x.abs().max(point.y.abs()).max(point.z.abs()))
        }
    };
    let undecided = |reached, why| GapError::Undecided { reached, why };
    let (b, _) = nearest(p, curve, r, width, budget, stop).map_err(|why| {
        undecided(
            Bracket {
                lower: 0.0,
                upper: f64::INFINITY,
            },
            why,
        )
    })?;
    if aim.met(b) {
        Ok(b)
    } else {
        Err(undecided(b, Short::Precision))
    }
}

/// One curve's largest distance from another: `sup_t min_s |a(t) - b(s)|`, each `t`'s
/// nearest by an inner search, a piece bounded through the correspondence `s(t)` linear
/// between its ends' nearest parameters (the chord bound on `a(t) - b(s(t))`).
struct CurveCurve<'a> {
    a: &'a Curve,
    b: &'a Curve,
    b_range: (f64, f64),
    b_knots: Vec<f64>,
    width: f64,
}

impl CurveCurve<'_> {
    fn gap(&self, t: f64, s: f64) -> Ivl {
        norm(sub3(
            ivl3(self.a.point_over(Ivl::from(t))),
            ivl3(self.b.point_over(Ivl::from(s))),
        ))
    }
}

impl Field for CurveCurve<'_> {
    type Mark = f64;

    fn at(&self, t: f64, budget: &mut Budget, stop: &Stop) -> Result<(Bracket, f64), Short> {
        let p = ivl3(self.a.point_over(Ivl::from(t)));
        nearest(p, self.b, self.b_range, self.width, budget, stop)
    }

    /// The smallest of three correspondences' bounds: `s` linear between the ends'
    /// nearest parameters (where `b` has no knot between them), and `s` held at either.
    fn over(&self, (ta, sa): (f64, f64), (tb, sb): (f64, f64)) -> f64 {
        let h = tb - ta;
        let aj = self.a.jet_over::<Ivl, 2>(Ivl::new(ta, tb), Approach::Above);
        let a = jets3(aj.point, aj.d[0], aj.d[1]);
        let minus = |b: [Jet2; 3]| [a[0] - b[0], a[1] - b[1], a[2] - b[2]];
        let held = |s: f64| {
            let p = ivl3(self.b.point_over(Ivl::from(s)));
            let b = p.map(Jet2::lift_ivl);
            vector_bound(minus(b), (self.gap(ta, s), self.gap(tb, s)), h)
        };
        let mut best = held(sa).min(held(sb));
        let (s0, s1) = (sa.min(sb), sa.max(sb));
        if !self.b_knots.iter().any(|&k| k > s0 && k < s1) {
            let bj = self.b.jet_over::<Ivl, 2>(Ivl::new(s0, s1), Approach::Above);
            let k = (Ivl::from(sb) - Ivl::from(sa)) / (Ivl::from(tb) - Ivl::from(ta));
            let k2 = sq(k);
            let b = jets3(bj.point, bj.d[0], bj.d[1]).map(|j| Jet2::new(j.f, j.d1 * k, j.d2 * k2));
            best = best.min(vector_bound(
                minus(b),
                (self.gap(ta, sa), self.gap(tb, sb)),
                h,
            ));
        }
        best
    }
}

/// How far `a` over `a_over` strays from `b` over `b_over`: the one-sided Hausdorff
/// distance `sup_t min_s |a(t) - b(s)|`, to `aim`, one budget unit per point of either
/// search. The symmetric distance is the larger of both ways.
///
/// # Errors
///
/// As [`curve_surface_gap`] (never pending).
pub fn curve_curve_gap(
    a: &Curve,
    a_over: &Extent1d,
    b: &Curve,
    b_over: &Extent1d,
    aim: Aim,
    budget: &mut Budget,
    stop: &Stop,
) -> Result<Bracket, GapError> {
    let aim = aim.check()?;
    let (lo, hi) = range(a_over)?;
    let b_range = range(b_over)?;
    let scale = [a.point(lo), a.point(hi)]
        .iter()
        .map(|p| p.x.abs().max(p.y.abs()).max(p.z.abs()))
        .fold(0.0, f64::max);
    let field = CurveCurve {
        a,
        b,
        b_range,
        b_knots: knots3(b),
        width: aim.inner(scale),
    };
    distance(search(
        &field,
        &cuts_of(lo, hi, &knots3(a)),
        aim,
        budget,
        stop,
    ))
}

#[cfg(test)]
mod tests;
