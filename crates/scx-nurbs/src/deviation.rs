// Written for solidcorex. Apache-2.0.
//! A certified bound of how far apart two B-spline curves are at the same parameters, over
//! their common domain or span by span, and of how far a curve misses points it was fitted
//! through (FT1; 77 §2.3).
//!
//! | solidcore at the pin (`nurbs::crv_deviation`) | here |
//! |---|---|
//! | `certified_distance` | [`distance`] |
//! | `certified_distance_by_span`, `SpanDistance` | [`distance_by_span`], [`SpanDistance`] |
//! | `certified_distance_floor` | [`distance_floor`] |
//! | `interpolation_bound` | [`interpolation_bound`] |
//! | `certified_hausdorff` | not here: FT1's row names the same-parameter distance; the Hausdorff bound has no row yet |
//! | `norm_up` | `norm_up` (private), for any dimension |
//! | `InvalidInput` for a rational or 2D curve, a number not finite, knots not clamped | rational curves are bounded (below), any dimension `D`, no clamped-knot requirement (a built [`NurbsCurve`] is finite), [`DeviationError`] for the rest |
//! | `QueryFailure::BoundOverflows` | [`DeviationError::BoundOverflows`] |
//!
//! # The bound
//!
//! Both curves are cut at the union of their breakpoints, so each *common span* is one
//! polynomial piece of each. A piece's Bezier control points are read straight off the
//! B-spline's local control points by the blossom (de Boor's recurrence with the arguments
//! `s` and `e` of the span; `p` convex combinations deep for every point), so no knot is
//! inserted into a copy of a curve and the work is linear in the span count.
//!
//! * **Polynomial curves.** The lower-degree piece is raised to the higher degree (Bezier
//!   elevation); the difference of two pieces of one degree is the Bezier piece of the
//!   differences of their control points, which lies in the hull of those (Farouki and
//!   Rajan 1987; Piegl and Tiller §5.6), so the largest distance between corresponding
//!   points bounds `|a(t) - b(t)|` on the span.
//! * **Rational curves** (either curve, the other taken with unit weights), through
//!   homogeneous points `(w P, w)`. With the numerators `A`, `B` and weight functions `Wa`,
//!   `Wb` of the two pieces, `a - b = (A Wb - B Wa) / (Wa Wb)`. The numerator is a Bezier
//!   piece of degree `pa + pb` with control points `n_k = sum_{i+j=k} C(pa,i) C(pb,j) /
//!   C(pa+pb,k) (A_i wb_j - B_j wa_i)`, so it lies in their hull, and the denominator is at
//!   least the product of the least weights (a Bezier weight function is a convex combination
//!   of its control weights). The bound is `max |n_k| / (min wa * min wb)`. It is a true bound
//!   but looser than the polynomial one (the least weights, a hull of degree `pa + pb`); it is
//!   exactly the polynomial bound's order where the weights are near 1.
//!
//! # Rounding
//!
//! Every rounding is counted, as the pin counts them (docs/definitions-and-certificates §4.4,
//! restated): both curves are first moved into their own frame (minus the first curve's first
//! control point; each moved coordinate rounds once, `gamma_2` of the size `M`, the largest
//! coordinate of the two moved nets, homogeneous ones included). Each Bezier control point is
//! a chain of `p` convex combinations, each adding at most `gamma_12 M`, so `gamma_(12 p)` of
//! `2 M` per coordinate; an elevation to degree `q` forms each point as a convex combination
//! of at most `p + 1` points with binomial quotients as coefficients: `gamma_(8 q)` of `M`.
//! The norm of each difference is rounded up by `gamma_8` ([`norm_up`]) and a coordinate error
//! `e` moves a point by at most `ceil(sqrt(D)) e`. The rational numerators add the rounding of
//! the products, sums and coefficients, carried as written in `rational_bound`.
//!
//! # Differences from the pin
//!
//! * The pin's error count follows the number of knots inserted into the whole curve (so a
//!   curve with many knots carries a larger term at every span). A point here depends only on
//!   the `p` combinations of its own span, so a span's bound does not grow with the other
//!   spans' knots: a refinement is no longer charged for its insertions.
//! * The pin refuses rational curves; the rational bound above is new, and certified the same
//!   way (its terms are in `rational_bound`). The floor of a rational curve is not defined
//!   ([`DeviationError::RationalFloor`]): the other curve's weights are in the denominator, so
//!   no number is a lower bound of every bound.
//! * The pin evaluated through its own basis count `gamma_(12 p + 4)`; evaluation here is the
//!   same blossom, `p` combinations deep, `gamma_(12 p)` of `2 M` (rational: divided by the
//!   weight, so a weight below the error is refused).
//! * Clamped knots are not required: the local control points of any span of a valid curve
//!   give its Bezier piece.

use core::fmt;

use scx_num::certified::gamma;
use scx_num::error::ErrorKind;

use crate::{NurbsCurve, NurbsError, bezier};

/// Combinations one control point of a Bezier piece is chained through, per degree, times
/// this many `gamma` terms: each combination is a quotient of two differences (`gamma_3`),
/// the complement, two products and a sum, and the pin's `gamma_12` covers it with room.
const LEVEL_UNITS: usize = 12;
/// `gamma` terms per degree of a Bezier elevation (the pin's `gamma_(8 q)`).
const ELEVATION_UNITS: usize = 8;
/// `gamma` terms of one coordinate moved into the curves' frame (one subtraction, and for a
/// homogeneous coordinate the weight's product).
const FRAME_UNITS: usize = 2;
/// `gamma` terms of the rounding of the rational numerators beyond `gamma_(degree)`: the
/// coefficient (`gamma_3`), its product with the term (`gamma_1`), the term (`gamma_4`).
const NUMERATOR_UNITS: usize = 10;
/// `gamma` terms of the divisions and products that form a bound from its parts.
const FORM_UNITS: usize = 4;

/// How far two curves are apart over one common span ([`distance_by_span`]).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SpanDistance {
    /// Where the span starts.
    pub from: f64,
    /// Where the span ends.
    pub to: f64,
    /// A bound of `|a(t) - b(t)|` for every `t` of the span, every rounding included.
    pub bound: f64,
}

/// Why a distance or a bound cannot be given.
#[derive(Debug, Clone, Copy, PartialEq)]
#[non_exhaustive]
pub enum DeviationError {
    /// A curve operation failed (a parameter outside the domain, a degree above the cap).
    Curve(NurbsError),
    /// The curves are on different domains: the distance is of the same parameter.
    Domains {
        /// The first curve's domain.
        a: [f64; 2],
        /// The second curve's domain.
        b: [f64; 2],
    },
    /// As many parameters as points are needed.
    Count {
        /// The parameters given.
        params: usize,
        /// The points given.
        points: usize,
    },
    /// A parameter or point is infinite or NaN.
    NotFinite,
    /// The curve has no points to measure from, or no span of non-zero length.
    Empty,
    /// A rational curve's weight is no longer provably positive after the rounding the bound
    /// counts (its weights are within that rounding of zero).
    WeightsLost,
    /// The bound overflows the floating range (curves too far apart or too large).
    BoundOverflows,
    /// The floor is the rounding every bound carries whatever the second curve is; a
    /// rational curve's bound divides by the second curve's weights, so it has none.
    RationalFloor,
    /// A step failed through the module's own fault: an index its invariants rule out was
    /// missing. A bug.
    Defect {
        /// The step that failed.
        why: &'static str,
    },
}

impl DeviationError {
    /// What the caller can do about the error (research 62 §1.1).
    #[must_use]
    pub const fn kind(&self) -> ErrorKind {
        match self {
            Self::Curve(e) => e.kind(),
            Self::Domains { .. }
            | Self::Count { .. }
            | Self::NotFinite
            | Self::Empty
            | Self::WeightsLost
            | Self::RationalFloor => ErrorKind::Refused,
            Self::BoundOverflows => ErrorKind::Unsupported,
            Self::Defect { .. } => ErrorKind::Defect,
        }
    }

    /// The cause's stable name.
    #[must_use]
    pub const fn name(&self) -> &'static str {
        match self {
            Self::Curve(e) => e.name(),
            Self::Domains { .. } => "domains",
            Self::Count { .. } => "count",
            Self::NotFinite => "not_finite",
            Self::Empty => "empty",
            Self::WeightsLost => "weights_lost",
            Self::BoundOverflows => "bound_overflows",
            Self::RationalFloor => "rational_floor",
            Self::Defect { .. } => "defect",
        }
    }
}

impl fmt::Display for DeviationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match *self {
            Self::Curve(e) => e.fmt(f),
            Self::Domains { a, b } => write!(
                f,
                "the curves are on different domains: [{}, {}] and [{}, {}]",
                a[0], a[1], b[0], b[1]
            ),
            Self::Count { params, points } => {
                write!(f, "{params} parameters for {points} points")
            }
            Self::NotFinite => f.write_str("a parameter or point is not finite"),
            Self::Empty => f.write_str("a curve with no span of non-zero length"),
            Self::WeightsLost => f.write_str("a weight is within the counted rounding of zero"),
            Self::BoundOverflows => f.write_str("the bound overflows the floating range"),
            Self::RationalFloor => f.write_str("a rational curve has no floor"),
            Self::Defect { why } => f.write_str(why),
        }
    }
}

impl std::error::Error for DeviationError {}

impl From<NurbsError> for DeviationError {
    fn from(e: NurbsError) -> Self {
        Self::Curve(e)
    }
}

type Result<T> = core::result::Result<T, DeviationError>;

const fn defect(why: &'static str) -> DeviationError {
    DeviationError::Defect { why }
}

/// A bound `d` with `|a(t) - b(t)| <= d` at every parameter `t` of the curves' common domain,
/// every rounding of computing it included: the largest of [`distance_by_span`]'s bounds. A
/// curve and itself, with a knot inserted or its degree raised, are within rounding.
///
/// # Errors
///
/// [`DeviationError::Domains`] for curves on different domains;
/// [`DeviationError::BoundOverflows`]; [`DeviationError::WeightsLost`] (rational).
pub fn distance<const D: usize>(a: &NurbsCurve<D>, b: &NurbsCurve<D>) -> Result<f64> {
    distance_by_span(a, b)?
        .iter()
        .map(|s| s.bound)
        .reduce(f64::max)
        .ok_or(DeviationError::Empty)
}

/// [`distance`] span by span: for each common span of the two curves (the union of their
/// breakpoints), in order from the domain's start to its end, each once and without a gap, a
/// bound `d` with `|a(t) - b(t)| <= d` at every parameter `t` of the span. The work is that of
/// the spans (module docs): `O(spans (p^3 + q^2))` for degrees `p` and `q`, plus the knots.
///
/// # Errors
///
/// As [`distance`].
pub fn distance_by_span<const D: usize>(
    a: &NurbsCurve<D>,
    b: &NurbsCurve<D>,
) -> Result<Vec<SpanDistance>> {
    let (da, db) = (domain(a), domain(b));
    if da != db {
        return Err(DeviationError::Domains { a: da, b: db });
    }
    let rational = a.is_rational() || b.is_rational();
    let origin = a.points().first().copied().ok_or(DeviationError::Empty)?;
    let (na, nb) = (Net::new(a, origin, rational), Net::new(b, origin, rational));
    let size = na.size.max(nb.size);
    let spans = common_spans(a, b, da)?;
    if rational {
        rational_spans(&na, &nb, size, &spans)
    } else {
        polynomial_spans::<D>(&na, &nb, size, &spans)
    }
}

/// A number that no bound of [`distance`] for `a` and another (polynomial) curve, and no
/// span's bound of [`distance_by_span`], is below, `a` being the first of the two: the
/// rounding the bounds count on `a`'s own size. A caller refining a curve towards a tolerance
/// below it can stop, since no refinement brings a span's bound within it.
///
/// The bound is `(g + e) (1 + gamma_2)` with `g` the largest difference of control points,
/// not negative, and `e` the sum of the two curves' errors times `ceil(sqrt(D))`. The floor
/// forms `e` from `a`'s own size and the least degree for the other curve (the errors are
/// not below that, whatever it is), by the operations the bounds are formed by on numbers that
/// are never larger: each rounds monotonically, so no bound is below it, to the bit.
///
/// # Errors
///
/// [`DeviationError::RationalFloor`] for a rational curve.
pub fn distance_floor<const D: usize>(a: &NurbsCurve<D>) -> Result<f64> {
    if a.is_rational() {
        return Err(DeviationError::RationalFloor);
    }
    let origin = a.points().first().copied().ok_or(DeviationError::Empty)?;
    let net = Net::new(a, origin, false);
    let ea = piece_error(a.degree(), a.degree(), net.size);
    let eb = piece_error(1, 1, net.size);
    let err = norm_factor::<D>() * (ea + eb);
    Ok(err * (1.0 + gamma(FORM_UNITS / 2)))
}

/// A bound `d` with `|c(u_k) - q_k| <= d` for every point `q_k` of `points` and its parameter
/// `u_k` of `params`, every rounding of computing it included (module docs): how far the
/// curve fitted through the points misses them.
///
/// # Errors
///
/// [`DeviationError::Count`] for other counts, [`DeviationError::NotFinite`] for a number not
/// finite, [`DeviationError::Curve`] for a parameter outside the domain,
/// [`DeviationError::BoundOverflows`], [`DeviationError::WeightsLost`] (rational).
pub fn interpolation_bound<const D: usize>(
    c: &NurbsCurve<D>,
    params: &[f64],
    points: &[[f64; D]],
) -> Result<f64> {
    if params.len() != points.len() {
        return Err(DeviationError::Count {
            params: params.len(),
            points: points.len(),
        });
    }
    if !params.iter().all(|u| u.is_finite()) || !points.iter().flatten().all(|x| x.is_finite()) {
        return Err(DeviationError::NotFinite);
    }
    let rational = c.is_rational();
    let net = Net::new(c, [0.0; D], rational);
    let p = c.degree();
    let eval_err = level_error(p, net.size);
    // the affine error of a rational point: the division by a weight at least `lo`
    let affine_err = if rational {
        let least = net.least_weight()?;
        let lo = least - eval_err;
        if lo <= 0.0 || lo.is_nan() {
            return Err(DeviationError::WeightsLost);
        }
        let reach = net.reach;
        (eval_err * (1.0 + reach) / lo + gamma(2) * reach) * (1.0 + gamma(FORM_UNITS))
    } else {
        eval_err
    };
    let (lo_knot, hi_knot) = (c.domain().min(), c.domain().max());
    let mut largest = 0.0_f64;
    for (&u, q) in params.iter().zip(points) {
        if !(lo_knot..=hi_knot).contains(&u) {
            return Err(NurbsError::ParameterOutside {
                parameter: u,
                domain: [lo_knot, hi_knot],
            }
            .into());
        }
        let k = span_at(c.knots(), p, net.rows.len(), u);
        let row = net.point_at(k, u)?;
        let at: Vec<f64> = if rational {
            let (w, num) = row
                .split_last()
                .ok_or(defect("a homogeneous row has a weight"))?;
            num.iter().map(|x| x / w).collect()
        } else {
            row
        };
        let diff: Vec<f64> = at.iter().zip(q).map(|(x, y)| x - y).collect();
        let d = norm_up(&diff).ok_or(DeviationError::BoundOverflows)?;
        largest = largest.max(d);
    }
    let bound = (largest + norm_factor::<D>() * affine_err) * (1.0 + gamma(2));
    finite(bound)
}

/// A curve's control points as the blossom reads them: rows of `D` coordinates (a
/// polynomial curve) or `D + 1` (a rational curve: `w (P - origin)` and `w`), the knots and
/// the degree.
struct Net<'a> {
    knots: &'a [f64],
    degree: usize,
    rows: Vec<Vec<f64>>,
    /// The largest absolute coordinate of the rows, weights included.
    size: f64,
    /// The largest absolute coordinate of the affine points (before the weights).
    reach: f64,
}

impl<'a> Net<'a> {
    fn new<const D: usize>(c: &'a NurbsCurve<D>, origin: [f64; D], homogeneous: bool) -> Self {
        let weights = c.weights();
        let mut size = 0.0_f64;
        let mut reach = 0.0_f64;
        let mut rows = Vec::with_capacity(c.points().len());
        for (i, p) in c.points().iter().enumerate() {
            let w = if homogeneous {
                weights.and_then(|ws| ws.get(i)).copied().unwrap_or(1.0)
            } else {
                1.0
            };
            let mut row: Vec<f64> = Vec::with_capacity(D + 1);
            for (x, o) in p.iter().zip(&origin) {
                let moved = x - o;
                reach = reach.max(moved.abs());
                let coord = if homogeneous { w * moved } else { moved };
                size = size.max(coord.abs());
                row.push(coord);
            }
            if homogeneous {
                size = size.max(w.abs());
                row.push(w);
            }
            rows.push(row);
        }
        Self {
            knots: c.knots(),
            degree: c.degree(),
            rows,
            size,
            reach,
        }
    }

    fn least_weight(&self) -> Result<f64> {
        self.rows
            .iter()
            .filter_map(|r| r.last().copied())
            .reduce(f64::min)
            .ok_or(DeviationError::Empty)
    }

    /// The Bezier control points of the piece over `[s, e]`, inside the span `k`.
    fn piece(&self, k: usize, s: f64, e: f64) -> Result<Vec<Vec<f64>>> {
        let p = self.degree;
        (0..=p)
            .map(|i| {
                let args: Vec<f64> = (0..p).map(|j| if j < p - i { s } else { e }).collect();
                self.blossom(k, &args)
            })
            .collect()
    }

    /// The curve's (homogeneous) point at `u`, inside the span `k`.
    fn point_at(&self, k: usize, u: f64) -> Result<Vec<f64>> {
        self.blossom(k, &vec![u; self.degree])
    }

    /// The blossom of the span `k`'s local polynomial at `args` (`degree` of them, each in
    /// the span): de Boor's recurrence with the arguments in turn.
    fn blossom(&self, k: usize, args: &[f64]) -> Result<Vec<f64>> {
        let p = self.degree;
        let first = k.checked_sub(p).ok_or(defect(
            "a span of a curve has its degree of points before it",
        ))?;
        let mut cur: Vec<Vec<f64>> = self
            .rows
            .get(first..=k)
            .ok_or(defect("a span's points are inside the net"))?
            .to_vec();
        let knot = |i: usize| {
            self.knots
                .get(i)
                .copied()
                .ok_or(defect("a span's knots are inside the knot vector"))
        };
        for (r, &x) in (1..).zip(args) {
            let mut next = Vec::with_capacity(cur.len().saturating_sub(1));
            for (t, pair) in cur.windows(2).enumerate() {
                let j = first + r + t;
                let (lo, hi) = (knot(j)?, knot(j + p + 1 - r)?);
                if hi <= lo {
                    return Err(defect("a span of non-zero length has distinct end knots"));
                }
                let alpha = ((x - lo) / (hi - lo)).clamp(0.0, 1.0);
                let (Some(u), Some(v)) = (pair.first(), pair.get(1)) else {
                    return Err(defect("a window holds two points"));
                };
                next.push(
                    u.iter()
                        .zip(v)
                        .map(|(&a, &b)| (1.0 - alpha) * a + alpha * b)
                        .collect(),
                );
            }
            cur = next;
        }
        cur.into_iter()
            .next()
            .ok_or(defect("the recurrence leaves a point"))
    }
}

/// The domain's two ends.
fn domain<const D: usize>(c: &NurbsCurve<D>) -> [f64; 2] {
    [c.domain().min(), c.domain().max()]
}

/// The span index of `u` of a curve with `n_points` points: `knots[k] <= u < knots[k + 1]`,
/// the last non-empty span at the domain's end.
fn span_at(knots: &[f64], degree: usize, n_points: usize, u: f64) -> usize {
    let above = knots.partition_point(|&x| x <= u);
    above
        .saturating_sub(1)
        .clamp(degree, n_points.saturating_sub(1))
}

/// The common spans `(from, to, span of a, span of b)`: the breakpoints of both curves
/// inside the domain, once each, in order.
fn common_spans<const D: usize>(
    a: &NurbsCurve<D>,
    b: &NurbsCurve<D>,
    [lo, hi]: [f64; 2],
) -> Result<Vec<(f64, f64, usize, usize)>> {
    let mut breaks: Vec<f64> = a
        .knots()
        .iter()
        .chain(b.knots())
        .copied()
        .filter(|&u| u > lo && u < hi)
        .collect();
    breaks.push(lo);
    breaks.push(hi);
    breaks.sort_by(f64::total_cmp);
    breaks.dedup();
    let spans: Vec<_> = breaks
        .windows(2)
        .filter_map(|w| Some((*w.first()?, *w.get(1)?)))
        .map(|(s, e)| {
            (
                s,
                e,
                span_at(a.knots(), a.degree(), a.points().len(), s),
                span_at(b.knots(), b.degree(), b.points().len(), s),
            )
        })
        .collect();
    if spans.is_empty() {
        return Err(DeviationError::Empty);
    }
    Ok(spans)
}

/// `ceil(sqrt(D))`: a vector of `D` coordinates each off by `e` is off by at most this times
/// `e` (two for the plane and for space, as the pin counts).
fn norm_factor<const D: usize>() -> f64 {
    (D as f64).sqrt().ceil()
}

/// Every coordinate error of a Bezier control point read from a net of size `size` by the
/// blossom: `gamma_(12 p)` of `2 size`.
fn level_error(degree: usize, size: f64) -> f64 {
    gamma(degree.saturating_mul(LEVEL_UNITS)) * 2.0 * size
}

/// The error of a coordinate of a polynomial piece of degree `degree` raised to `target`,
/// moved into the frame: the blossom's, the elevation's and the frame's.
fn piece_error(degree: usize, target: usize, size: f64) -> f64 {
    let mut err = level_error(degree, size);
    if target > degree {
        err += gamma(target.saturating_mul(ELEVATION_UNITS)) * (size + err);
    }
    err + gamma(FRAME_UNITS) * size
}

fn finite(bound: f64) -> Result<f64> {
    if bound.is_finite() {
        Ok(bound)
    } else {
        Err(DeviationError::BoundOverflows)
    }
}

fn polynomial_spans<const D: usize>(
    a: &Net<'_>,
    b: &Net<'_>,
    size: f64,
    spans: &[(f64, f64, usize, usize)],
) -> Result<Vec<SpanDistance>> {
    let q = a.degree.max(b.degree);
    let err =
        norm_factor::<D>() * (piece_error(a.degree, q, size) + piece_error(b.degree, q, size));
    let grow = 1.0 + gamma(FORM_UNITS / 2);
    let mut out = Vec::with_capacity(spans.len());
    for &(from, to, ka, kb) in spans {
        let pa = raised::<D>(a.piece(ka, from, to)?, q)?;
        let pb = raised::<D>(b.piece(kb, from, to)?, q)?;
        let mut gap = 0.0_f64;
        for (x, y) in pa.iter().zip(&pb) {
            let diff: Vec<f64> = x.iter().zip(y).map(|(u, v)| u - v).collect();
            gap = gap.max(norm_up(&diff).ok_or(DeviationError::BoundOverflows)?);
        }
        let bound = finite((gap + err) * grow)?;
        out.push(SpanDistance { from, to, bound });
    }
    Ok(out)
}

/// A Bezier piece's points raised to degree `q` (the points as they are for `q` its degree).
fn raised<const D: usize>(piece: Vec<Vec<f64>>, q: usize) -> Result<Vec<Vec<f64>>> {
    let p = piece.len().saturating_sub(1);
    if q == p {
        return Ok(piece);
    }
    let arrays = piece
        .iter()
        .map(|r| <[f64; D]>::try_from(r.as_slice()).map_err(|_| defect("a row has D coordinates")))
        .collect::<Result<Vec<_>>>()?;
    Ok(bezier::elevate(&arrays, q - p)?
        .into_iter()
        .map(Vec::from)
        .collect())
}

/// `C(n, k)` exactly, `n` at most twice [`MAX_DEGREE`].
fn binomial(n: usize, k: usize) -> u128 {
    let k = k.min(n.saturating_sub(k));
    let mut c: u128 = 1;
    for i in 1..=k {
        c = c * (n - k + i) as u128 / i as u128;
    }
    c
}

fn rational_spans(
    a: &Net<'_>,
    b: &Net<'_>,
    size: f64,
    spans: &[(f64, f64, usize, usize)],
) -> Result<Vec<SpanDistance>> {
    // every homogeneous coordinate carries `e` (the blossom's and the frame's)
    let ea = level_error(a.degree, size) + gamma(FRAME_UNITS) * size;
    let eb = level_error(b.degree, size) + gamma(FRAME_UNITS) * size;
    let e = ea.max(eb);
    // a computed or exact coordinate is at most `s`
    let s = size * (1.0 + gamma(FRAME_UNITS)) + e;
    let (pa, pb) = (a.degree, b.degree);
    let terms = pa.min(pb) + 1;
    // the exact numerator's control points from the computed nets' (each product off by
    // `2 s e`, two of them), and the rounding of the products, sums and coefficients, whose
    // terms are each at most `2 s^2` and whose coefficients sum to one
    let numerator_err =
        4.0 * s * e + 2.0 * s * s * gamma(terms + NUMERATOR_UNITS) * (1.0 + gamma(FORM_UNITS));
    let dims = a.rows.first().map_or(0, Vec::len);
    let factor = ((dims.saturating_sub(1)) as f64).sqrt();
    let mut out = Vec::with_capacity(spans.len());
    for &(from, to, ka, kb) in spans {
        let xa = a.piece(ka, from, to)?;
        let xb = b.piece(kb, from, to)?;
        let lo_a = least_weight(&xa)? - ea;
        let lo_b = least_weight(&xb)? - eb;
        if lo_a.is_nan() || lo_b.is_nan() || lo_a <= 0.0 || lo_b <= 0.0 {
            return Err(DeviationError::WeightsLost);
        }
        let mut largest = 0.0_f64;
        for k in 0..=(pa + pb) {
            let mut n = vec![0.0_f64; dims.saturating_sub(1)];
            let den = binomial(pa + pb, k) as f64;
            for (i, ai) in xa.iter().enumerate().take(k + 1) {
                let Some(j) = k.checked_sub(i) else { continue };
                let Some(bj) = xb.get(j) else { continue };
                let coef = (binomial(pa, i) * binomial(pb, j)) as f64 / den;
                let (wa, wb) = (last(ai)?, last(bj)?);
                for ((acc, &x), &y) in n.iter_mut().zip(ai).zip(bj) {
                    *acc += coef * (x * wb - y * wa);
                }
            }
            largest = largest.max(norm_up(&n).ok_or(DeviationError::BoundOverflows)?);
        }
        let numerator = (largest + factor.ceil() * numerator_err) * (1.0 + gamma(FORM_UNITS));
        let denominator = lo_a * lo_b * (1.0 - gamma(FORM_UNITS));
        let bound = finite(numerator / denominator * (1.0 + gamma(FORM_UNITS)))?;
        out.push(SpanDistance { from, to, bound });
    }
    Ok(out)
}

fn last(row: &[f64]) -> Result<f64> {
    row.last().copied().ok_or(defect("a row has a weight"))
}

fn least_weight(piece: &[Vec<f64>]) -> Result<f64> {
    piece
        .iter()
        .map(|r| last(r))
        .try_fold(f64::INFINITY, |m, w| Ok(m.min(w?)))
}

/// An upper bound of the length of the vector `v`, every rounding included and nothing lost to
/// underflow: the components are divided by the largest before they are squared (each
/// quotient rounds by `gamma_1`, the squares, sums and root by `gamma_5`, the product by
/// `gamma_1`, the subtractions that formed them by `gamma_1`: `gamma_8` in all), and the
/// smallest subnormals a product with a subnormal largest can lose are added back. `None` for
/// a vector that overflows.
fn norm_up(v: &[f64]) -> Option<f64> {
    let s = v.iter().fold(0.0_f64, |m, x| m.max(x.abs()));
    if !s.is_finite() {
        return None;
    }
    if s == 0.0 {
        return Some(0.0);
    }
    let sum: f64 = v.iter().map(|x| (x / s) * (x / s)).sum();
    let bound = s * sum.sqrt() * (1.0 + gamma(8 + v.len())) + 4.0 * f64::from_bits(1);
    bound.is_finite().then_some(bound)
}

#[cfg(test)]
mod tests;
