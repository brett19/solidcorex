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
//! **A lift with a series** (74 §35, `CF2b`). Where the pcurve is a constant-height or
//! constant-angle line in an analytic kind's angle uv, or a cylinder's or cone's wave, or a
//! circle or ellipse in a plane, and the edge's curve is a circle or ellipse, `S(p(t)) - C(t)`
//! is a trigonometric series in `t` of degree at most 2 and is built on its coefficients, as
//! the curve-to-surface gap is: a coaxial circle is then zero to rounding and closes at once.
//! The kinds' quarter-arc uv composed with the pcurve's angle map is the angle itself up to
//! the formulas' rounded constants (a fixed turn and scale of a few ulps per arc), which the
//! series' bound is padded by ([`angle_pad`]). Every other pcurve goes through
//! [`Surface::box_over`] and the jets as before.
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
use std::collections::BinaryHeap;

use scx_curve::arcs::Arcs;
use scx_curve::{AngleAxis, Curve, Curve2};
use scx_num::budget::{Budget, Exhausted, Stop};
use scx_num::error::ErrorKind;
use scx_num::extent1d::Extent1d;
use scx_num::extent2d::Extent2d;
use scx_num::interval::{Ivl, round_down, round_up};
use scx_num::jet::{Jet2, chord, norm, square as sq, vector_bound};
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
// Second-order forward differentiation in intervals: [`Jet2`] and the bounds from it live in
// `scx_num::jet` (moved there for FT3's fit certificate, 77 §1.1).

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

    /// `k0 + k1 cos t + k2 sin t`.
    fn trig(k0: Ivl, k1: Ivl, k2: Ivl) -> Self {
        let z = Ivl::from(0.0);
        Self::Trig([k0, k1, k2, z, z])
    }

    /// `k` times the series.
    fn scaled(&self, k: Ivl) -> Self {
        match *self {
            Self::Poly(p) => Self::Poly(p.map(|x| x * k)),
            Self::Trig(p) => Self::Trig(p.map(|x| x * k)),
        }
    }

    /// A bound on `|series|` over every `t`: the sum of the coefficients' magnitudes (a
    /// trigonometric series only; a polynomial's is infinite).
    fn sup(&self) -> f64 {
        match self {
            Self::Poly(_) => f64::INFINITY,
            Self::Trig(k) => k.iter().fold(0.0, |a, x| round_up(a + x.mag())),
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

/// `[c, a, b]` with `p(t) = c + a cos t + b sin t`, each enclosed from `p` at `0`, `pi`
/// and `pi/2` (for a circle or ellipse, whose formula is that).
fn trig_parts<const N: usize>(p: impl Fn(Ivl) -> [Ivl; N]) -> [[Ivl; N]; 3] {
    use core::f64::consts::{FRAC_PI_2, PI};
    let pi = Ivl::new(PI, round_up(PI));
    let half = Ivl::new(FRAC_PI_2, round_up(FRAC_PI_2));
    let (p0, pp, ph) = (p(Ivl::from(0.0)), p(pi), p(half));
    let h = Ivl::from(0.5);
    let c = each(p0, pp, |a, b| h * (a + b));
    [c, each(p0, c, |a, b| a - b), each(ph, c, |a, b| a - b)]
}

/// `f` of `a` and `b` coordinate by coordinate.
fn each<const N: usize>(a: [Ivl; N], b: [Ivl; N], f: impl Fn(Ivl, Ivl) -> Ivl) -> [Ivl; N] {
    let mut out = a;
    for (o, y) in out.iter_mut().zip(b) {
        *o = f(*o, y);
    }
    out
}

/// The edge's curve as series in its parameter, one per world coordinate: a circle or
/// ellipse only.
fn conic_series(curve: &Curve) -> Option<[Series; 3]> {
    if !matches!(curve, Curve::Circle(_) | Curve::Ellipse(_)) {
        return None;
    }
    let [c, a, b] = trig_parts(|t| ivl3(curve.point_over(t)));
    Some([
        Series::trig(c[0], a[0], b[0]),
        Series::trig(c[1], a[1], b[1]),
        Series::trig(c[2], a[2], b[2]),
    ])
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
                let [c, a, b] = trig_parts(at);
                Some((true, [sub3(c, o), a, b]))
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

    /// Whether the curve is a line (its series polynomial).
    fn is_line(&self) -> bool {
        matches!(
            self,
            Self::Series {
                z: Series::Poly(_),
                ..
            }
        )
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

/// `max |g|` over `[a, a + h]` for a convex `g` with the jets `ga`, `gb` at the ends: the
/// ends bound it above, and the tangents at the ends below (where they cross).
///
/// A line's signed distance across a cone's generator, `(rho - r - k z) / sqrt(1 + k^2)`, is
/// convex in the line's parameter: `rho` is the norm of an affine map of it and `z` is
/// affine. Its closed bound converges at the second order with no second derivative, where
/// the chord bound's enclosure of `rho''` (zero in truth along a generator, wide in
/// intervals) needs a piece per few microns (`CF2a` §28.8: 158k units for a generator).
fn convex_bound(ga: &Jet2, gb: &Jet2, h: f64) -> f64 {
    let (fa, fb, sa, sb) = (ga.f, gb.f, ga.d1, gb.d1);
    if ![fa, fb, sa, sb]
        .iter()
        .all(|x| x.lo.is_finite() && x.hi.is_finite())
    {
        // `max` passes a NaN over: an end the jets do not hold (on the axis) decides nothing.
        return f64::INFINITY;
    }
    let hi = fa.hi.max(fb.hi);
    let h_up = Ivl::from(h);
    // Each tangent's lowest point over the piece bounds `g` below.
    let along_a = fa + Ivl::from(sa.lo.min(0.0)) * h_up;
    let along_b = fb - Ivl::from(sb.hi.max(0.0)) * h_up;
    let mut lo = along_a.lo.max(along_b.lo);
    if sa.hi <= 0.0 && sb.lo >= 0.0 && sb.lo > sa.hi {
        // The tangents cross where `(fa sb - fb sa + sa sb h) / (sb - sa)`.
        let cross = (fa * sb - fb * sa + sa * sb * h_up) / (sb - sa);
        lo = lo.max(cross.lo);
    }
    let lo = if lo.is_nan() { f64::NEG_INFINITY } else { lo };
    round_up(hi.max(-lo))
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
                    let mut perp = chord((pa.f, pb.f), pi.d2.mag(), pi.f, h);
                    if self.src.is_line() {
                        perp = perp.min(convex_bound(&pa, &pb, h));
                    }
                    apex.min(perp)
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
    lift: Option<Lift>,
}

/// Arcs past this many turn-quarters from the start of their angle are left to the
/// general path (the pad grows with the arc's index).
const MAX_ARC_INDEX: f64 = 64.0;

/// The pad on a lift through quarter arcs whose index is at most `k` in size, per unit of
/// the radius it turns: at least the composed map's departure from the angle.
///
/// The pcurve's angle coordinate on arc `k` is `(k + 1/2) L + tan(psi / 2) / a` with
/// `psi = phi - s0 - c_k`, where `s0` is the rounded start of the arcs (`0` or `-pi/2`) and
/// `c_k` the rounded `k pi/2 + pi/4`; the kind's formula takes the same `(k + 1/2) L` and
/// `a` back to `tau = tan(psi / 2)`, so (in exact arithmetic on those constants) its
/// `(cos, sin)` is `(cos psi, sin psi)` turned by the arc's middle angle, whose rounded
/// `(+-1/sqrt 2, +-1/sqrt 2)` is the true turn scaled by `1 + d`, `|d| <= 2^-53`. The point is
/// then at angle `phi + e_k` with `|e_k| <= |s0 - true| + |c_k - true|
/// <= (|k| + 2) 2^-52 pi/2`, radius scaled by `1 + d`. Where the pcurve's arc and the kind's
/// differ (a parameter a few ulps past a break) both formulas meet at the break to the same
/// order. `(|k| + 4) 2^-48` covers the sum more than five times over.
fn angle_pad(k: f64) -> f64 {
    round_up((k + 4.0) * f64::EPSILON * 16.0)
}

/// The largest arc index `phi` meets over `t` in `[lo, hi]` with `phi = start + sign t` on
/// `arcs`, or `None` past [`MAX_ARC_INDEX`] or (on the half turn) where the angle leaves
/// `[-pi/2 - 1/2, pi/2 + 1/2]`, the end arcs' formulas continuing past the poles.
fn arc_index(arcs: Arcs, start: f64, sign: f64, (lo, hi): (f64, f64)) -> Option<f64> {
    use core::f64::consts::FRAC_PI_2;
    let ends = [start + sign * lo, start + sign * hi];
    if !ends.iter().all(|e| e.is_finite()) {
        return None;
    }
    let from = if arcs.is_turn() { 0.0 } else { -FRAC_PI_2 };
    if !arcs.is_turn() && ends.iter().any(|e| e.abs() > FRAC_PI_2 + 0.5) {
        return None;
    }
    let k = ends
        .iter()
        .map(|e| ((e - from).abs() / FRAC_PI_2).ceil() + 1.0)
        .fold(0.0, f64::max);
    (k <= MAX_ARC_INDEX).then_some(k)
}

/// `(cos phi, sin phi)` with `phi = start + sign t`, as series in `t`.
fn angle_series(start: f64, sign: f64) -> (Series, Series) {
    let (sn, cs) = Ivl::from(start).sin_cos();
    let s = Ivl::from(sign);
    let z = Ivl::from(0.0);
    (Series::trig(z, cs, z - s * sn), Series::trig(z, sn, s * cs))
}

/// `(cos, sin)` of `arcs` at the parameter `x`, the kind's own formula on either arc at a
/// break, hulled.
fn arc_cos_sin(arcs: Arcs, x: f64) -> (Ivl, Ivl) {
    let t = Ivl::from(x);
    let a = arcs.cos_sin(t, arcs.piece(x, Approach::Above));
    let b = arcs.cos_sin(t, arcs.piece(x, Approach::Below));
    (a.0.hull(b.0), a.1.hull(b.1))
}

/// `S(p(t)) - C(t)` as series in `t`, each world coordinate, and the pad its bound takes.
struct Lift {
    d: [Series; 3],
    pad: f64,
}

impl Lift {
    /// `O + f (cos X + sin Y) + g Z - C` with `f` and `g` and `(cos, sin)` series of the
    /// first degree, `C` the curve's.
    fn of(
        axes: &Axes,
        f: Series,
        g: Series,
        cs: (Series, Series),
        c: &[Series; 3],
        pad: f64,
    ) -> Self {
        let o = [axes.o.x, axes.o.y, axes.o.z];
        let (x, y, z) = (vec3(axes.x), vec3(axes.y), vec3(axes.z));
        let minus = Ivl::from(-1.0);
        let zero = Ivl::from(0.0);
        let at = |o: f64, x: Ivl, y: Ivl, z: Ivl, c: &Series| {
            let dir = cs.0.scaled(x).plus(&cs.1.scaled(y));
            Series::trig(Ivl::from(o), zero, zero)
                .plus(&f.times(&dir))
                .plus(&g.scaled(z))
                .plus(&c.scaled(minus))
        };
        let d = [
            at(o[0], x[0], y[0], z[0], &c[0]),
            at(o[1], x[1], y[1], z[1], &c[1]),
            at(o[2], x[2], y[2], z[2], &c[2]),
        ];
        Self { d, pad }
    }

    /// The lift of `pcurve` on `surface` against `curve` over `range`, where it has one.
    fn new(surface: &Surface, pcurve: &Curve2, curve: &Curve, range: (f64, f64)) -> Option<Self> {
        let c = conic_series(curve)?;
        let zero = Ivl::from(0.0);
        let konst = |k: Ivl| Series::trig(k, zero, zero);
        if let (Surface::Plane(p), Curve2::Circle(_) | Curve2::Ellipse(_)) = (surface, pcurve) {
            // `S = O + u X + v Y`, exact: no pad.
            let [k, a, b] = trig_parts(|t| {
                let q = pcurve.point_over(t);
                [q.x, q.y]
            });
            let (u, v) = (
                Series::trig(k[0], a[0], b[0]),
                Series::trig(k[1], a[1], b[1]),
            );
            let f = p.frame();
            let (o, x, y) = (f.origin(), vec3(*f.x()), vec3(*f.y()));
            let o = [o.x, o.y, o.z];
            let at = |o: f64, x: Ivl, y: Ivl, c: &Series| {
                konst(Ivl::from(o))
                    .plus(&u.scaled(x))
                    .plus(&v.scaled(y))
                    .plus(&c.scaled(Ivl::from(-1.0)))
            };
            let d = [
                at(o[0], x[0], y[0], &c[0]),
                at(o[1], x[1], y[1], &c[1]),
                at(o[2], x[2], y[2], &c[2]),
            ];
            return Some(Self { d, pad: 0.0 });
        }
        let (u_arcs, v_arcs) = arcs_of(surface);
        match pcurve {
            Curve2::AngleLine(l) if l.rate() == 0.0 => {
                let sign = l.sense().sign();
                let k = arc_index(l.arcs(), l.start(), sign, range)?;
                let cs = angle_series(l.start(), sign);
                let at = Ivl::from(l.other());
                match l.axis() {
                    AngleAxis::U if u_arcs == Some(l.arcs()) => {
                        // The angle runs round the axis at the fixed height `other`.
                        let (axes, f, g) = match surface {
                            Surface::Cylinder(s) => (s.axes(), Ivl::from(s.radius().mm()), at),
                            Surface::Cone(s) => (
                                s.axes(),
                                Ivl::from(s.radius().mm()) + Ivl::from(s.slope()) * at,
                                at,
                            ),
                            Surface::Sphere(s) => {
                                let (cl, sl) = arc_cos_sin(sphere::V, l.other());
                                let r = Ivl::from(s.radius().mm());
                                (s.axes(), r * cl, r * sl)
                            }
                            Surface::Torus(s) => {
                                let (cw, sw) = arc_cos_sin(torus::TURN, l.other());
                                let r = Ivl::from(s.minor().mm());
                                (s.axes(), Ivl::from(s.major().mm()) + r * cw, r * sw)
                            }
                            Surface::Plane(_) | Surface::Nurbs(_) => return None,
                        };
                        let pad = round_up(2.0 * f.mag() * angle_pad(k));
                        Some(Self::of(&axes, konst(f), konst(g), cs, &c, pad))
                    }
                    AngleAxis::V if v_arcs == Some(l.arcs()) => {
                        // The angle runs round the meridian at the fixed `u = other`.
                        let u_arcs = u_arcs?;
                        let (c0, s0) = arc_cos_sin(u_arcs, l.other());
                        let (axes, big, r) = match surface {
                            Surface::Sphere(s) => (s.axes(), 0.0, s.radius().mm()),
                            Surface::Torus(s) => (s.axes(), s.major().mm(), s.minor().mm()),
                            _ => return None,
                        };
                        let ri = Ivl::from(r);
                        let f = konst(Ivl::from(big)).plus(&cs.0.scaled(ri));
                        let g = cs.1.scaled(ri);
                        let turn = (konst(c0), konst(s0));
                        let pad = round_up(4.0 * r.abs() * angle_pad(k));
                        Some(Self::of(&axes, f, g, turn, &c, pad))
                    }
                    _ => None,
                }
            }
            Curve2::AngleWave(w) if u_arcs == Some(w.arcs()) => {
                let sign = w.sense().sign();
                let k = arc_index(w.arcs(), w.start(), sign, range)?;
                let (ac, as_) = w.amplitudes();
                let v = Series::trig(Ivl::from(w.mean()), Ivl::from(ac), Ivl::from(as_));
                let (axes, f) = match surface {
                    Surface::Cylinder(s) => (s.axes(), konst(Ivl::from(s.radius().mm()))),
                    Surface::Cone(s) => (
                        s.axes(),
                        konst(Ivl::from(s.radius().mm())).plus(&v.scaled(Ivl::from(s.slope()))),
                    ),
                    _ => return None,
                };
                let pad = round_up(2.0 * f.sup() * angle_pad(k));
                Some(Self::of(
                    &axes,
                    f,
                    v,
                    angle_series(w.start(), sign),
                    &c,
                    pad,
                ))
            }
            _ => None,
        }
    }

    /// `D` and its derivatives over `t`.
    fn jets(&self, t: Ivl) -> [Jet2; 3] {
        [self.d[0].jet(t), self.d[1].jet(t), self.d[2].jet(t)]
    }
}

fn vec3(v: Vec3) -> [Ivl; 3] {
    [Ivl::from(v.x), Ivl::from(v.y), Ivl::from(v.z)]
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
        if let Some(lift) = &self.lift {
            // The mark is the series' own `|D|`; the pad goes on the bracket.
            let d = lift.jets(Ivl::from(t));
            let g = norm([d[0].f, d[1].f, d[2].f]);
            let upper = round_up(g.hi + lift.pad);
            return Ok((
                Bracket {
                    lower: round_down(g.lo - lift.pad).max(0.0),
                    upper: if upper.is_nan() { f64::INFINITY } else { upper },
                },
                g,
            ));
        }
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
        if let Some(lift) = &self.lift {
            let bound = round_up(vector_bound(lift.jets(i), (ga, gb), b - a) + lift.pad);
            return if bound.is_nan() { f64::INFINITY } else { bound };
        }
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
        lift: Lift::new(surface, pcurve, curve, (lo, hi)),
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

/// The distance between two points, enclosed in intervals; no search, so no budget. The
/// vertex-to-vertex gap of 74 SU8 (`vertex_meet`): the same enclosure as [`vertex_curve_gap`].
///
/// # Errors
///
/// [`GapError::NotFinite`] when a coordinate is not finite or the distance overflows.
pub(crate) fn point_point_gap(a: Point3, b: Point3) -> Result<Bracket, GapError> {
    let at = |p: Point3| [Ivl::from(p.x), Ivl::from(p.y), Ivl::from(p.z)];
    if a == b && a.is_finite() {
        // The same point: the distance is exactly zero, not a widened enclosure of it.
        return Ok(Bracket {
            lower: 0.0,
            upper: 0.0,
        });
    }
    let (p, q) = (at(a), at(b));
    let g = norm([p[0].sub_ulp(q[0]), p[1].sub_ulp(q[1]), p[2].sub_ulp(q[2])]);
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
