// Written for solidcorex. Apache-2.0.
//! A curve refitted to a distance: the clamped spline of a given degree through a source's two
//! end points, leaving each given end along its direction, with the fewest spans the pin's
//! halving search finds whose *certified* same-parameter distance from the source is within
//! the aim at every parameter (FT3; 77 §2, the pin's `refit_within`).
//!
//! # The search
//!
//! The pin's, unchanged. It starts with one span (two even spans for degree 2 with both
//! directions held: one span of degree 2 has a single inner control point, which cannot lie
//! on both end lines). Each round samples the source, fits the samples by least squares at
//! their parameters with the end directions held and their reaches free
//! ([`least_squares_along`]), and certifies the fit span by span. While a span is past the
//! aim, the span with the largest bound is halved (a simple knot at its middle); while the
//! start's reach is not positive the first span is halved instead, and the last while the
//! end's is not. One knot a round. It stops when every span is within the aim and every reach
//! is positive ([`Ended::Within`]), when the next knots would need more than
//! [`MAX_CONTROL_POINTS`] ([`Ended::Cap`]), when no certificate can come within the aim
//! ([`Ended::Floor`]), or when a middle or a sample has no floating-point room inside its
//! span ([`Ended::NoRoom`]). The result is the last fit made, whatever stopped it, with every
//! span's bound: the caller reads [`Fitted::ended`] or compares the bounds with its aim.
//!
//! The samples: the source's two end points, and in every span `2 (p + 1)` parameters at even
//! steps strictly inside it and its right break, so every span holds at least `p + 1`
//! distinct parameters and the fit is unique. A sample seeds the fit; it never decides that a
//! fit is good (77 §1.3).
//!
//! Which span is halved does not depend on the aim, so a looser aim stops on a fit that a
//! tighter one passes through: its knots are among the tighter one's. That holds of the
//! certificates below because a span past the aim carries a bound that does not depend on the
//! aim, and a span within it carries one at most the aim (so below every span past it).
//!
//! # The certificate of a span
//!
//! Every bound counts every rounding; none is sampled.
//!
//! - **A polynomial B-spline source** ([`CurveFn::as_nurbs`]): FT1's Bezier-hull bound
//!   ([`distance_by_span`]) on the common spans, the largest of those inside each span of
//!   the fit. Under [`distance_floor`] of the source no span can come within the aim, so the
//!   search then stops after one fit ([`Ended::Floor`]), as the pin's does.
//! - **A rational B-spline source**: FT1's rational bound first (the hull of the numerator of
//!   `a - b` over the least weights, about 5 times loose on a circle's coarse spans, nearly
//!   tight on fine ones, where the weights hardly vary). The search halves by FT1's bound
//!   alone; the branch and bound below only tries to end it sooner, bringing within the aim the
//!   spans FT1 leaves past it, the closest first, until one cannot be (its `|g|` at a point past
//!   the aim, or FT1's bound within [`CLOSE`] of that `|g|`). Its jets are those of the
//!   numerator and the weight in Taylor form, divided ([`Homogeneous`]).
//! - **Any other source**: the branch and bound of SU6's gap on `g = F - C`, the source less the
//!   fit, over the span cut at the source's breaks: `|g|` enclosed at points in intervals, and
//!   over a piece by [`vector_bound`] from the jets ([`Jet2`]) of `F` and `C` over it (the chord
//!   bound `max |g(ends)| + h^2 / 8 sup |g''|`, the same on `|g|^2`, and the direct enclosure, the
//!   smallest). The piece with the largest bound is halved, a child never above its parent,
//!   until the span's bound is within the aim, or within [`CLOSE`] of the largest `|g|` found
//!   at a point (the bound is then the distance to that much, whatever the aim), or the span
//!   has [`MAX_PIECES`] pieces. A span past the aim whose bound is within [`FLOOR_UNITS`]
//!   widths of its point enclosures is at the evaluation's rounding: no fit can be proved
//!   closer, and the search stops there ([`Ended::Floor`]). So it does at once when the aim is
//!   under half the width of the source's own enclosure at an end of the domain, which no fit
//!   can come under. The fit's jets over a piece are its Taylor form about the piece's middle,
//!   not its basis sum, so the bound falls as the cube of the span's width when it is halved.
//!
//! # Against the pin
//!
//! - Any [`CurveFn`] source, not a polynomial 3D B-spline only; rational sources are bounded.
//! - The span cap is the named [`MAX_CONTROL_POINTS`] (the pin's `most`, which its one caller
//!   passes as 4096), not an argument.
//! - `ends_held` and its `EFF_ZERO_RAD` are not ported: each held end's reach and the turn of
//!   its stored leg are given ([`Held`]), and the caller, who knows the angle it needs, judges
//!   them.
//! - The pin's bound of the whole fit is the largest of [`Fitted::spans`]' ([`Fitted::bound`]).
//!
//! [`distance_by_span`]: crate::deviation::distance_by_span
//! [`distance_floor`]: crate::deviation::distance_floor

use core::cmp::Ordering;
use std::collections::BinaryHeap;

use scx_num::budget::{Budget, Stop};
use scx_num::cert::Method;
use scx_num::extent1d::Extent1d;
use scx_num::interval::Ivl;
use scx_num::jet::{Jet2, norm, vector_bound};
use scx_num::trig::Trig;

use super::FitError;
use super::along::{dot, least_squares_along, unit};
use crate::deviation::{SpanDistance, distance_by_span, distance_floor};
use crate::{Approach, KnotVector, MAX_DEGREE, NurbsCurve};

/// The most control points a fit may have: the search stops ([`Ended::Cap`]) rather than add
/// a knot past it. The pin's `most`, as its caller passes it.
pub const MAX_CONTROL_POINTS: usize = 4096;

/// The most pieces the branch and bound cuts one span of a fit into before it gives the bound
/// it has reached (still a bound; past the aim, the span is halved).
pub const MAX_PIECES: usize = 256;

/// A span's bound within this fraction above the largest `|g|` found at a point of it is the
/// span's distance to that much: its branch and bound stops there.
pub const CLOSE: f64 = 0.125;

/// A span past the aim whose bound is at most this many widths of its point enclosures is at
/// the rounding of the evaluation: no fit is proved closer.
pub const FLOOR_UNITS: f64 = 4.0;

/// A curve a fit can follow: `D` coordinates of one parameter, evaluated over any scalar with
/// sine and cosine ([`Trig`]): `f64` for samples, [`Ivl`] for enclosures at points, [`Jet2`]
/// for the enclosures of the value and its first two derivatives over a piece.
pub trait CurveFn<const D: usize> {
    /// The parameter range: the fit has the same.
    fn domain(&self) -> Extent1d;

    /// Parameters where the curve may fail to be twice differentiable, in increasing order
    /// (repeats allowed): no piece of the certificate's search crosses one. Empty for a curve
    /// smooth over its domain.
    fn breaks(&self) -> &[f64];

    /// `C(t)`; over an interval (or a jet), an enclosure over it.
    fn eval<T: Trig>(&self, t: T) -> [T; D];

    /// The curve as a B-spline, when it is one, for FT1's Bezier-hull bound.
    fn as_nurbs(&self) -> Option<&NurbsCurve<D>> {
        None
    }
}

impl<const D: usize> CurveFn<D> for NurbsCurve<D> {
    fn domain(&self) -> Extent1d {
        Self::domain(self)
    }

    fn breaks(&self) -> &[f64] {
        self.knots()
    }

    fn eval<T: Trig>(&self, t: T) -> [T; D] {
        self.point_over(t, Approach::Above)
    }

    fn as_nurbs(&self) -> Option<&NurbsCurve<D>> {
        Some(self)
    }
}

/// What a fit is asked for.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FitAim<const D: usize> {
    /// The distance from the source, at the same parameter, every span is to be within: in the
    /// source's units.
    pub within: f64,
    /// The fit's degree, `1..=MAX_DEGREE`.
    pub degree: usize,
    /// The direction the fit leaves its start along, if held (any finite length; degree 2 up).
    pub start: Option<[f64; D]>,
    /// The direction the fit reaches its end along, if held.
    pub end: Option<[f64; D]>,
}

/// A span of the fit and its certified bound.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SpanBound {
    /// The span, between two breaks of the fit.
    pub span: Extent1d,
    /// A bound of `|source(t) - fit(t)|` for every `t` of the span, every rounding included.
    pub bound: f64,
    /// How it was proved: [`Method::Bernstein`] for FT1's hull, [`Method::Interval`] for the
    /// branch and bound.
    pub method: Method,
}

/// A held end of the fit.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Held {
    /// How far the end's leg runs along the direction: positive when the fit leaves (or
    /// reaches) the end along it.
    pub reach: f64,
    /// The angle, in radians, between the leg as it is stored (each coordinate of the inner
    /// control point rounded once) and the direction: next to nothing on a leg many roundings
    /// long, as much as the direction is lost on a leg a few roundings long. `pi` for a leg of
    /// no length.
    pub turn: f64,
}

/// Why the search stopped.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Ended {
    /// Every span within the aim and every reach positive.
    Within,
    /// The next fit would need more than [`MAX_CONTROL_POINTS`].
    Cap,
    /// No fit can be proved within the aim: the aim is under the rounding of the source's own
    /// certificate.
    Floor,
    /// A middle or a sample had no floating-point room inside its span.
    NoRoom,
}

/// A fit and its certificate ([`fit_within`]).
#[derive(Debug, Clone, PartialEq)]
pub struct Fitted<const D: usize> {
    /// The last fit made: clamped, of the aim's degree, on the source's domain, its interior
    /// knots simple.
    pub curve: NurbsCurve<D>,
    /// Every span's bound, in order.
    pub spans: Vec<SpanBound>,
    /// The largest of the spans' bounds.
    pub bound: f64,
    /// How many fits were made.
    pub fits: usize,
    /// The start, where its direction was held.
    pub start: Option<Held>,
    /// The end, where its direction was held.
    pub end: Option<Held>,
    /// Why the search stopped.
    pub ended: Ended,
}

/// `source` refitted as a clamped spline of the aim's degree on the same domain, through the
/// source's end points, along the aim's end directions where held, with the fewest spans the
/// halving search of the module docs finds within the aim of the source at every parameter.
///
/// The result is always the last fit made, with its spans' certified bounds and why the search
/// stopped: [`Ended::Within`] unless it reached [`MAX_CONTROL_POINTS`], the aim is under the
/// certificate's floor, or a span has no floating-point room.
///
/// # Errors
///
/// [`FitError::Aim`], [`FitError::Degree`], [`FitError::Directions`], [`FitError::Direction`]
/// for the aim; [`FitError::StartAgainst`], [`FitError::EndAgainst`] for a direction against
/// the source's tangent at its end; [`FitError::Domain`] for a domain not finite or too short
/// to sample; [`FitError::SourceNotFinite`]; those of the fit ([`least_squares_along`]) and of
/// the B-spline certificate ([`FitError::Deviation`]); [`FitError::Exhausted`] and
/// [`FitError::Stopped`] (one unit of the budget per evaluation of the source).
pub fn fit_within<const D: usize>(
    source: &impl CurveFn<D>,
    aim: &FitAim<D>,
    budget: &mut Budget,
    stop: &Stop,
) -> Result<Fitted<D>, FitError> {
    search(source, aim, MAX_CONTROL_POINTS, budget, stop)
}

/// The source's certificate: FT1's alone, FT1's then the branch and bound, or the branch and
/// bound alone.
enum Certify<'a, const D: usize> {
    Polynomial(&'a NurbsCurve<D>),
    Rational(&'a NurbsCurve<D>, Box<Homogeneous<D>>),
    General,
}

/// A rational source as the polynomials it is the quotient of: `F = N / w`, `N` the weighted
/// control points and `w` the weights on the same knots, so the branch and bound takes the jets
/// of each over a piece in Taylor form ([`fit_over`]) and divides them, not the basis form of
/// the quotient, whose excess would not fall as the spans are halved.
struct Homogeneous<const D: usize> {
    numerator: NurbsCurve<D>,
    weight: NurbsCurve<1>,
}

impl<const D: usize> Homogeneous<D> {
    /// `None` for a curve with no weights.
    fn of(c: &NurbsCurve<D>) -> Result<Option<Self>, FitError> {
        let Some(weights) = c.weights() else {
            return Ok(None);
        };
        let numerator = c
            .points()
            .iter()
            .zip(weights)
            .map(|(p, &w)| p.map(|x| x * w))
            .collect();
        let numerator = NurbsCurve::new(c.knot_vector().clone(), numerator, None)?;
        let weight = weights.iter().map(|&w| [w]).collect();
        let weight = NurbsCurve::new(c.knot_vector().clone(), weight, None)?;
        Ok(Some(Self { numerator, weight }))
    }
}

/// The source as the branch and bound reads it: points by [`CurveFn::eval`], and jets over a
/// piece by the same unless it is a rational B-spline ([`Homogeneous`]).
#[derive(Clone, Copy)]
struct Source<'a, S, const D: usize> {
    curve: &'a S,
    homogeneous: Option<&'a Homogeneous<D>>,
}

impl<S: CurveFn<D>, const D: usize> Source<'_, S, D> {
    fn over(&self, a: f64, b: f64) -> [Jet2; D] {
        match self.homogeneous {
            Some(h) => {
                let [w] = fit_over(&h.weight, a, b);
                fit_over(&h.numerator, a, b).map(|n| n / w)
            }
            None => self.curve.eval(Jet2::variable(Ivl::new(a, b))),
        }
    }
}

/// [`fit_within`] with at most `most` control points.
pub(super) fn search<const D: usize>(
    source: &impl CurveFn<D>,
    aim: &FitAim<D>,
    most: usize,
    budget: &mut Budget,
    stop: &Stop,
) -> Result<Fitted<D>, FitError> {
    let within = aim.within;
    if !(within.is_finite() && within > 0.0) {
        return Err(FitError::Aim { within });
    }
    let p = aim.degree;
    if p == 0 || p > MAX_DEGREE {
        return Err(FitError::Degree { degree: p });
    }
    if (aim.start.is_some() || aim.end.is_some()) && p < 2 {
        return Err(FitError::Directions { degree: p });
    }
    let start = aim.start.map(unit).transpose()?;
    let end = aim.end.map(unit).transpose()?;
    let domain = source.domain();
    let (lo, hi) = (domain.min(), domain.max());
    let short = FitError::Domain { lo, hi };
    if !(lo.is_finite() && hi.is_finite() && lo < hi) {
        return Err(short);
    }
    // A direction against the source's own tangent cannot be held: the reach would stay
    // negative however short the end span became. Unit against the tangent's value.
    let against = |d: Option<[f64; D]>, t: f64| {
        d.is_some_and(|d| {
            let jet = source.eval(Jet2::variable(Ivl::from(t)));
            let tangent = jet.map(|j| j.d1.mid());
            unit(tangent).is_ok_and(|u| dot(&u, &d) < 0.0)
        })
    };
    if against(start, lo) {
        return Err(FitError::StartAgainst);
    }
    if against(end, hi) {
        return Err(FitError::EndAgainst);
    }
    let (certify, floor) = match source.as_nurbs() {
        Some(c) if c.is_rational() => match Homogeneous::of(c)? {
            Some(h) => (Certify::Rational(c, Box::new(h)), None),
            None => (Certify::Polynomial(c), Some(distance_floor(c)?)),
        },
        Some(c) => (Certify::Polynomial(c), Some(distance_floor(c)?)),
        None => (Certify::General, Some(rounding_floor(source, lo, hi))),
    };
    let mut cuts: Vec<f64> = source
        .breaks()
        .iter()
        .copied()
        .filter(|&b| lo < b && b < hi)
        .collect();
    cuts.sort_by(f64::total_cmp);
    cuts.dedup();

    let both_on_quadratic = p == 2 && start.is_some() && end.is_some();
    let first_spans = if both_on_quadratic { 2 } else { 1 };
    if most < p + first_spans {
        return Err(FitError::TooFewControlPoints {
            count: most,
            needed: p + first_spans,
        });
    }
    let mut breaks = if both_on_quadratic {
        vec![lo, middle(lo, hi).ok_or(short)?, hi]
    } else {
        vec![lo, hi]
    };
    let mut params = samples(&breaks, p).ok_or(short)?;
    let mut fits = 0_usize;
    loop {
        stop.check()?;
        let knots = KnotVector::new(p, knots_of(&breaks, p))?;
        budget.charge(params.len() as u64)?;
        let mut data = Vec::with_capacity(params.len());
        for &t in &params {
            let q = source.eval(t);
            if !q.iter().all(|x| x.is_finite()) {
                return Err(FitError::SourceNotFinite { t });
            }
            data.push(q);
        }
        let fit = least_squares_along(&knots, &data, &params, start, end)?;
        let curve = fit.spline.into_curve()?;
        fits += 1;
        // Each span's bound, and the rank the search halves by: the same but for a rational
        // source, ranked by FT1's bound alone (see the rational arm).
        let (spans, ranks): (Vec<Span>, Vec<f64>) = match &certify {
            Certify::Polynomial(c) => {
                let spans = by_hull(&breaks, &distance_by_span(c, &curve)?)?;
                let ranks = spans.iter().map(|s| s.bound).collect();
                (spans, ranks)
            }
            Certify::Rational(c, h) => {
                let source = Source {
                    curve: source,
                    homogeneous: Some(h),
                };
                let mut spans = by_hull(&breaks, &distance_by_span(c, &curve)?)?;
                let ranks: Vec<f64> = spans.iter().map(|s| s.bound).collect();
                // The spans past the aim by FT1, the closest first: the branch and bound tries
                // to bring each within, and the first it cannot ends the try, since the search
                // then goes on whatever the rest come to.
                let mut past: Vec<(f64, usize)> = ranks
                    .iter()
                    .copied()
                    .zip(0..)
                    .filter(|&(r, _)| r > within)
                    .collect();
                past.sort_by(|x, y| x.0.total_cmp(&y.0).then(x.1.cmp(&y.1)));
                for (_, j) in past {
                    let Some(s) = spans.get_mut(j) else { continue };
                    let enough = Enough {
                        within,
                        known: s.bound,
                        past: true,
                    };
                    let b =
                        branch_and_bound(&source, &curve, s.extent, &cuts, enough, budget, stop)?;
                    let brought = b.bound <= within;
                    if b.bound < s.bound {
                        *s = b;
                    }
                    if !brought {
                        break;
                    }
                }
                (spans, ranks)
            }
            Certify::General => {
                let source = Source {
                    curve: source,
                    homogeneous: None,
                };
                let mut spans = Vec::with_capacity(breaks.len());
                for w in breaks.windows(2) {
                    let &[a, b] = w else { continue };
                    let span = Extent1d::new(a, b).ok_or(DEFECT_SPAN)?;
                    let enough = Enough {
                        within,
                        known: f64::INFINITY,
                        past: false,
                    };
                    spans.push(branch_and_bound(
                        &source, &curve, span, &cuts, enough, budget, stop,
                    )?);
                }
                let ranks = spans.iter().map(|s| s.bound).collect();
                (spans, ranks)
            }
        };
        let points = curve.points();
        let held = |reach: Option<f64>, from: Option<&[f64; D]>, to: Option<&[f64; D]>, t| {
            reach.zip(t).map(|(reach, t)| Held {
                reach,
                turn: turn(from, to, t),
            })
        };
        let before_last = points.len().checked_sub(2).and_then(|i| points.get(i));
        let held_start = held(fit.start_reach, points.first(), points.get(1), start);
        let held_end = held(fit.end_reach, before_last, points.last(), end);
        let start_short = fit.start_reach.is_some_and(|r| !positive(r));
        let end_short = fit.end_reach.is_some_and(|r| !positive(r));
        let all_within = spans.iter().all(|s| s.bound <= within);
        let at = if start_short {
            0
        } else if end_short {
            spans.len().saturating_sub(1)
        } else {
            // The span with the largest rank, the first of equals.
            let mut at = 0;
            let mut largest = f64::NEG_INFINITY;
            for (j, &r) in ranks.iter().enumerate() {
                if r > largest {
                    largest = r;
                    at = j;
                }
            }
            at
        };
        let rounding = !start_short
            && !end_short
            && spans
                .get(at)
                .is_some_and(|s| s.bound <= FLOOR_UNITS * s.width);
        let done = |ended| Fitted {
            curve: curve.clone(),
            bound: spans.iter().fold(0.0, |m: f64, s| m.max(s.bound)),
            spans: spans.iter().map(Span::bound).collect(),
            fits,
            start: held_start,
            end: held_end,
            ended,
        };
        if !start_short && !end_short && all_within {
            return Ok(done(Ended::Within));
        }
        if floor.is_some_and(|f| f > within) || rounding {
            return Ok(done(Ended::Floor));
        }
        let mut next = Vec::with_capacity(breaks.len() + 1);
        for (j, w) in breaks.windows(2).enumerate() {
            let &[a, b] = w else { continue };
            next.push(a);
            if j == at {
                let Some(m) = middle(a, b) else {
                    return Ok(done(Ended::NoRoom));
                };
                next.push(m);
            }
        }
        next.push(hi);
        if next.len() - 1 + p > most {
            return Ok(done(Ended::Cap));
        }
        let Some(u) = samples(&next, p) else {
            return Ok(done(Ended::NoRoom));
        };
        breaks = next;
        params = u;
    }
}

const DEFECT_SPAN: FitError = FitError::Defect {
    why: "the breaks of the search increase strictly",
};

/// A span's bound in the making: its certified bound, how it was proved, and the largest width
/// of the point enclosures the branch and bound took in it (zero for FT1's hull).
#[derive(Debug, Clone, Copy)]
struct Span {
    extent: Extent1d,
    bound: f64,
    method: Method,
    width: f64,
}

impl Span {
    const fn bound(&self) -> SpanBound {
        SpanBound {
            span: self.extent,
            bound: self.bound,
            method: self.method,
        }
    }
}

/// Whether `x` is finite and positive.
fn positive(x: f64) -> bool {
    x.is_finite() && x > 0.0
}

/// The angle between the leg from `from` to `to` and the unit direction `t`: the arc tangent of
/// the leg's part across `t` over its part along it, exact near zero. `pi` for a leg of no
/// length, not finite, or missing.
fn turn<const D: usize>(from: Option<&[f64; D]>, to: Option<&[f64; D]>, t: [f64; D]) -> f64 {
    let (Some(a), Some(b)) = (from, to) else {
        return core::f64::consts::PI;
    };
    let mut leg = *b;
    for (x, y) in leg.iter_mut().zip(a) {
        *x -= y;
    }
    let Ok(leg) = unit(leg) else {
        return core::f64::consts::PI;
    };
    let along = dot(&leg, &t);
    let mut across = leg;
    for (x, y) in across.iter_mut().zip(&t) {
        *x -= along * y;
    }
    scx_num::fmath::atan2(dot(&across, &across).sqrt(), along)
}

/// The middle of `[a, b]`, `None` when it is not strictly inside (no floating-point room).
fn middle(a: f64, b: f64) -> Option<f64> {
    let m = 0.5 * a + 0.5 * b;
    (a < m && m < b).then_some(m)
}

/// The clamped knots of degree `p` on `breaks`: the first and last `p + 1` times each, every
/// interior break once.
fn knots_of(breaks: &[f64], p: usize) -> Vec<f64> {
    let mut knots = Vec::with_capacity(breaks.len() + 2 * p);
    if let (Some(&lo), Some(&hi)) = (breaks.first(), breaks.last()) {
        knots.extend(core::iter::repeat_n(lo, p + 1));
        knots.extend(breaks.iter().skip(1).take(breaks.len().saturating_sub(2)));
        knots.extend(core::iter::repeat_n(hi, p + 1));
    }
    knots
}

/// The sample parameters on `breaks`: the first break, then in every span `2 (p + 1)`
/// parameters at even steps strictly inside it and its right break. `None` when a parameter is
/// not strictly between the one before it and its span's end, or for no span.
fn samples(breaks: &[f64], p: usize) -> Option<Vec<f64>> {
    let inside = 2 * (p + 1);
    let steps = (inside + 1) as f64;
    let &first = breaks.first()?;
    if breaks.len() < 2 {
        return None;
    }
    let mut u = Vec::with_capacity((breaks.len() - 1) * (inside + 1) + 1);
    u.push(first);
    let mut last = first;
    for w in breaks.windows(2) {
        let &[a, b] = w else { return None };
        for i in 1..=inside {
            let s = i as f64 / steps;
            let t = a * (1.0 - s) + b * s;
            if !(t > last && t < b) {
                return None;
            }
            u.push(t);
            last = t;
        }
        u.push(b);
        last = b;
    }
    Some(u)
}

/// The largest of FT1's common-span bounds inside each span of the fit on `breaks`, in order.
/// The common spans are cut at every break of the fit, so each lies in one span of it.
fn by_hull(breaks: &[f64], by_span: &[SpanDistance]) -> Result<Vec<Span>, FitError> {
    const NESTED: FitError = FitError::Defect {
        why: "the certificate cuts at every break of the fit, so its spans nest in them",
    };
    let mut rest = by_span.iter().peekable();
    let mut out = Vec::with_capacity(breaks.len());
    for w in breaks.windows(2) {
        let &[a, b] = w else { continue };
        let mut largest: Option<f64> = None;
        while let Some(s) = rest.next_if(|s| s.from >= a && s.to <= b) {
            largest = Some(largest.map_or(s.bound, |m| m.max(s.bound)));
        }
        out.push(Span {
            extent: Extent1d::new(a, b).ok_or(DEFECT_SPAN)?,
            bound: largest.ok_or(NESTED)?,
            method: Method::Bernstein,
            width: 0.0,
        });
    }
    if rest.next().is_some() {
        return Err(NESTED);
    }
    Ok(out)
}

/// A piece of the branch and bound: its ends, `|g|` enclosed at each, and its bound.
#[derive(Debug, Clone, Copy)]
struct Piece {
    a: f64,
    b: f64,
    ga: Ivl,
    gb: Ivl,
    upper: f64,
}

impl PartialEq for Piece {
    fn eq(&self, o: &Self) -> bool {
        self.cmp(o) == Ordering::Equal
    }
}

impl Eq for Piece {}

impl PartialOrd for Piece {
    fn partial_cmp(&self, o: &Self) -> Option<Ordering> {
        Some(self.cmp(o))
    }
}

/// The largest bound first, then the lowest start: a total order, so the search is the same
/// on every run.
impl Ord for Piece {
    fn cmp(&self, o: &Self) -> Ordering {
        self.upper
            .total_cmp(&o.upper)
            .then_with(|| o.a.total_cmp(&self.a))
    }
}

/// `|F(t) - C(t)|` enclosed at the point `t`.
fn at_point<const D: usize>(
    source: &impl CurveFn<D>,
    fit: &NurbsCurve<D>,
    t: f64,
    budget: &mut Budget,
) -> Result<Ivl, FitError> {
    budget.charge(1)?;
    let t = Ivl::from(t);
    let f = source.eval(t);
    let c = fit.point_over(t, Approach::Above);
    let mut g = f;
    for (x, y) in g.iter_mut().zip(c) {
        *x = *x - y;
    }
    Ok(norm(g))
}

/// Half the widest of the source's enclosures at the domain's ends: no fit's branch and bound
/// comes under it. The ends are breaks of every fit, the bound is at least the upper end of
/// `|g|` enclosed there, and `g = F - C` is at least as wide as `F` in every coordinate, so
/// its upper end is at least half that width whatever the fit.
fn rounding_floor<const D: usize>(source: &impl CurveFn<D>, lo: f64, hi: f64) -> f64 {
    [lo, hi].iter().fold(0.0_f64, |m, &t| {
        source
            .eval(Ivl::from(t))
            .iter()
            .fold(m, |m, x| m.max(0.5 * (x.hi - x.lo)))
    })
}

/// A bound of `|F - C|` over `[a, b]` from the jets of both over it and `|g|` at its ends.
fn over_piece<S: CurveFn<D>, const D: usize>(
    source: &Source<'_, S, D>,
    fit: &NurbsCurve<D>,
    (a, b): (f64, f64),
    ends: (Ivl, Ivl),
    budget: &mut Budget,
) -> Result<f64, FitError> {
    budget.charge(1)?;
    let f = source.over(a, b);
    let c = fit_over(fit, a, b);
    let mut g = f;
    for (x, y) in g.iter_mut().zip(c) {
        *x = *x - y;
    }
    let h = (Ivl::from(b) - Ivl::from(a)).hi;
    Ok(vector_bound(g, ends, h))
}

/// The jets of the fit over `[a, b]`, a piece inside one of its spans, in its Taylor form about
/// the piece's middle `m`: `C(m + s) = sum_k C^(k)(m) s^k / k!`, exact on one polynomial span,
/// each `C^(k)(m)` enclosed at the point. The basis form ([`NurbsCurve::point_over`]) sums the
/// absolute control points times the basis functions' jets, so over a piece of width `w` in a
/// span of width `h` its second derivative is about `|P| w / h^3` too wide, and the chord
/// bound's excess, about `|P| (w / h)^3`, would not fall as the search halves spans; in the
/// Taylor form the excess is about `w^3 |C'''|`, the curve's own. Degrees above
/// [`TAYLOR_DEGREE`] keep the basis form.
fn fit_over<const D: usize>(fit: &NurbsCurve<D>, a: f64, b: f64) -> [Jet2; D] {
    match fit.degree() {
        0..=3 => taylor::<D, 3>(fit, a, b),
        4..=7 => taylor::<D, 7>(fit, a, b),
        8..=TAYLOR_DEGREE => taylor::<D, TAYLOR_DEGREE>(fit, a, b),
        _ => None,
    }
    .unwrap_or_else(|| fit.point_over(Jet2::variable(Ivl::new(a, b)), Approach::Above))
}

/// The highest degree [`fit_over`] puts in Taylor form: the basis derivatives' integer factor
/// `p! / (p - k)!` is an exact `f64` to here (`18! < 2^53`; past 20 it overflows `usize`).
const TAYLOR_DEGREE: usize = 18;

/// [`fit_over`]'s Taylor form, `K` at least the degree; `None` when the middle is not finite.
fn taylor<const D: usize, const K: usize>(
    fit: &NurbsCurve<D>,
    a: f64,
    b: f64,
) -> Option<[Jet2; D]> {
    let m = a + 0.5 * (b - a);
    if !m.is_finite() {
        return None;
    }
    let (c0, ck) = fit.jet_over::<Ivl, K>(Ivl::from(m), Approach::Above);
    let s = Jet2::variable(Ivl::new(a, b)) - Jet2::lift_ivl(Ivl::from(m));
    let p = fit.degree().min(K);
    let mut out = [Jet2::lift_ivl(Ivl::from(0.0)); D];
    for (x, o) in out.iter_mut().enumerate() {
        // Horner's rule: acc = D_p, then acc = D_(k-1) + (s / k) acc for k = p, ..., 1.
        let mut acc = Jet2::lift_ivl(*ck.get(p.checked_sub(1)?)?.get(x)?);
        for k in (1..=p).rev() {
            let below = if k == 1 {
                *c0.get(x)?
            } else {
                *ck.get(k - 2)?.get(x)?
            };
            let k = f64::from(u8::try_from(k).ok()?);
            acc = Jet2::lift_ivl(below) + s * acc / Jet2::lift_ivl(Ivl::from(k));
        }
        *o = acc;
    }
    Some(out)
}

/// When [`branch_and_bound`] may stop short of its cap: its bound within the aim; a bound
/// already `known` (FT1's) no more than [`CLOSE`] past the largest `|g|` it has found at a
/// point, which no bound can come under; or, when `past` (a rational source, where it only
/// tries to end the search), that `|g|` past the aim. Only the last depends on the aim past
/// it, and a rational source's search is ranked by FT1's bound alone, so the knots still nest.
#[derive(Debug, Clone, Copy)]
struct Enough {
    within: f64,
    known: f64,
    past: bool,
}

/// SU6's branch and bound on `|F - C|` over the span, cut first at the source's breaks inside
/// it (module docs).
fn branch_and_bound<S: CurveFn<D>, const D: usize>(
    source: &Source<'_, S, D>,
    fit: &NurbsCurve<D>,
    span: Extent1d,
    cuts: &[f64],
    enough: Enough,
    budget: &mut Budget,
    stop: &Stop,
) -> Result<Span, FitError> {
    let Enough {
        within,
        known,
        past,
    } = enough;
    let (lo, hi) = (span.min(), span.max());
    let mut ts = vec![lo];
    ts.extend(cuts.iter().copied().filter(|&c| lo < c && c < hi));
    ts.push(hi);
    let mut gs = Vec::with_capacity(ts.len());
    for &t in &ts {
        gs.push(at_point(source.curve, fit, t, budget)?);
    }
    let mut largest = gs.iter().fold(0.0_f64, |m, g| m.max(g.mag()));
    let mut width = gs.iter().fold(0.0_f64, |m, g| m.max(g.hi - g.lo));
    let mut heap = BinaryHeap::with_capacity(2 * MAX_PIECES);
    for (t, g) in ts.windows(2).zip(gs.windows(2)) {
        let (&[a, b], &[ga, gb]) = (t, g) else {
            continue;
        };
        let upper = over_piece(source, fit, (a, b), (ga, gb), budget)?;
        heap.push(Piece {
            a,
            b,
            ga,
            gb,
            upper,
        });
    }
    let mut pieces = heap.len();
    loop {
        let Some(&top) = heap.peek() else {
            return Err(FitError::Defect {
                why: "a span has at least one piece",
            });
        };
        let close = (1.0 + CLOSE) * largest;
        let shown_past = past && largest > within;
        if top.upper <= within
            || top.upper <= close
            || known <= close
            || shown_past
            || pieces >= MAX_PIECES
        {
            break;
        }
        stop.check()?;
        let Some(m) = middle(top.a, top.b) else {
            break;
        };
        heap.pop();
        let gm = at_point(source.curve, fit, m, budget)?;
        largest = largest.max(gm.mag());
        width = width.max(gm.hi - gm.lo);
        let left = over_piece(source, fit, (top.a, m), (top.ga, gm), budget)?;
        let right = over_piece(source, fit, (m, top.b), (gm, top.gb), budget)?;
        heap.push(Piece {
            b: m,
            gb: gm,
            upper: left.min(top.upper),
            ..top
        });
        heap.push(Piece {
            a: m,
            ga: gm,
            upper: right.min(top.upper),
            ..top
        });
        pieces += 1;
    }
    let bound = heap.peek().map_or(f64::INFINITY, |p| p.upper);
    Ok(Span {
        extent: span,
        bound,
        method: Method::Interval,
        width,
    })
}
