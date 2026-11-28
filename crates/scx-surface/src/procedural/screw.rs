// Written for solidcorex. Apache-2.0.
//! The screw motion of a [`super::Sweep`] (research 75 §13.29): a profile point `P` turned
//! by `theta` about the axis through `o` along the unit `k`, lifted `lambda theta` along it
//! (`lambda = pitch / (2 pi)`), and first moved `delta(theta)` along the unit radial
//! direction `e` (the radial law, piecewise linear in `theta`, held beyond its ends):
//!
//! `S(t, v) = o + k (k . (P - o) + lambda theta) + R(theta) (r + e delta(theta))`,
//! `r = P - o - k (k . (P - o))`.
//!
//! The net's knots across the turns are the stations `t_0 < .. < t_n` (radians). On the
//! span `[t_i, t_i+1]` of width `w` and middle `m`, with `tau = tan(w / 4)` and
//! `x = (2 tau / w) (t - m)`, the definition's angle is `theta = m + 2 atan x`: the angle
//! the net's rational quadratic arc (weights `1, cos(w / 2), 1`) turns a point to at the
//! same parameter, so the definition and the net are compared point for point and
//! `theta(t_i) = t_i`.
//!
//! The motion at a fixed angle is affine in the point, so the net is the profile's net
//! carried by the arc, and its gap from the definition is the same for every profile point:
//! per span, along `k` the lift's `|lambda| E_l(x)` and square to it the offset's
//! `|sigma| (w / 2) E_o(x)` (`sigma` the law's slope on the span). With
//! `K = 2 (1 + tau^2) atan(tau) / tau`,
//!
//! - `E_l(x) = K x / (1 + x^2) - 2 atan x`, odd, nought at `0, +-tau`, its extreme at
//!   `x*^2 = (K - 2) / (K + 2)` ([`lift_error_bound`]);
//! - `E_o(x) = (x (1 - tau^2) / tau - A (1 - x^2), tau + x^2 / tau - 2 x A) / (1 + x^2)`,
//!   `A = atan x / atan tau`, its largest length by interval branch and bound
//!   ([`offset_error_bound`]).
//!
//! Both are enclosed in rounded interval arithmetic (`atan` by its alternating series with
//! its remainder), never sampled, over an interval of `tau` that holds every span of a
//! group. Ported from the pin's `surface::procedural_sweep`; its one slope over a whole
//! range is the law's slope on each span here.

use core::f64::consts::{FRAC_PI_4, TAU};
use std::collections::BTreeMap;

use scx_num::extent1d::Extent1d;
use scx_num::fmath;
use scx_num::interval::Ivl;
use scx_num::ring::Ring;
use scx_num::trig::{Ranged, Trig};
use scx_num::units::Resolution;
use scx_num::vector3d::{Point3, UnitVec3, Vec3};
use scx_nurbs::Approach;

use super::{FLAT, ProceduralError};

/// The largest half angle of a span: a span turns at most `pi / 4`, so `tau <= tan(pi / 16)`.
pub(crate) const MAX_HALF_ANGLE: f64 = TAU / 16.0;

/// The most spans a screw's turn is cut into.
pub(crate) const MAX_SPANS: usize = 12_000;

/// The most terms of an alternating series.
const SERIES_TERMS: u32 = 64;

/// How close (relative) the offset bound's branch and bound comes to a value it takes.
const SUP_REL: f64 = 1e-3;

/// The most splits of the offset bound's branch and bound.
const SUP_STEPS: usize = 4096;

/// The most rounds of the station choice.
const CHOICE_ROUNDS: usize = 64;

/// `2^-64`.
const TINY_REL: f64 = 5.421010862427522e-20;

// ---------------------------------------------------------------- the bounds

/// `sum_{n >= 0} (-1)^n c(n) p0 r^n` enclosed, for `0 <= p0`, `0 <= r` and terms that fall
/// in size from the first: the partial sum until a term is below `2^-64` of the sum or after
/// [`SERIES_TERMS`] terms, widened by the first term left out, which bounds the rest. `p0`
/// and `r` are intervals, so the sum holds for every series they hold.
fn alternating(p0: Ivl, r: Ivl, c: impl Fn(u32) -> Ivl) -> Ivl {
    let mut power = p0;
    let mut sum = Ivl::from(0.0);
    let mut n = 0;
    while n < SERIES_TERMS {
        let term = c(n) * power;
        sum = if n % 2 == 0 { sum + term } else { sum - term };
        power = power * r;
        n += 1;
        if (c(n) * power).mag() <= sum.mig() * TINY_REL {
            break;
        }
    }
    let rest = (c(n) * power).mag();
    sum + Ivl::new(-rest, rest)
}

/// `1 / m` enclosed.
fn reciprocal(m: u32) -> Ivl {
    Ivl::from(1.0) / Ivl::from(f64::from(m))
}

/// `atan(x)` enclosed for `0 <= x <= 1/2` by its series (the whole line outside).
fn atan_point(x: f64) -> Ivl {
    if !(0.0..=0.5).contains(&x) {
        return Ivl::whole();
    }
    let xi = Ivl::from(x);
    alternating(xi, xi * xi, |n| reciprocal(2 * n + 1))
}

/// `atan` over `x`, `0 <= x <= 1/2` (it rises).
fn atan_ivl(x: Ivl) -> Ivl {
    Ivl::new(atan_point(x.lo).lo, atan_point(x.hi).hi)
}

/// `max |E_l|` over a span, for every `tau` in `tau` (`0 < tau <= 1/2`), enclosed from
/// above (infinite outside): how far, in radians of turn, the net's rational quadratic lift
/// leads or lags the true one. `E_l` is a difference of numbers far larger than it, so it
/// and `K - 2` are taken as their own series, which do not cancel:
///
/// - `K - 2 = 4 sum_{n >= 1} (-1)^(n-1) tau^(2n) / (4 n^2 - 1)`;
/// - `E_l(x) = (K - 2) x / (1 + x^2) + 2 sum_{i >= 1} (-1)^i 2i / (2i + 1) x^(2i+1)`,
///
/// both alternating with falling terms; `E_l` is taken at an interval holding `x*`.
pub(crate) fn lift_error_bound(tau: Ivl) -> f64 {
    if !(tau.lo > 0.0 && tau.hi <= 0.5) {
        return f64::INFINITY;
    }
    let one = Ivl::from(1.0);
    let four = Ivl::from(4.0);
    let t2 = tau * tau;
    let kappa = four * alternating(t2, t2, |n| reciprocal((2 * n + 1) * (2 * n + 3)));
    let x2 = kappa / (four + kappa);
    let x = x2.sqrt();
    let x3 = x * x2;
    let series = alternating(x3, x2, |n| {
        Ivl::from(f64::from(2 * n + 2)) * reciprocal(2 * n + 3)
    });
    let e = kappa * x / (one + x2) - Ivl::from(2.0) * series;
    e.mag()
}

/// `|E_o|^2` over `x`, for every `tau` in `tau`.
pub(crate) fn offset_error_sq(tau: Ivl, x: Ivl) -> Ivl {
    let one = Ivl::from(1.0);
    let two = Ivl::from(2.0);
    let a = atan_ivl(x) / atan_ivl(tau);
    let x2 = x * x;
    let den = one + x2;
    let ex = (x * (one - tau * tau) / tau - a * (one - x2)) / den;
    let ey = (tau + x2 / tau - two * x * a) / den;
    ex * ex + ey * ey
}

/// The largest `|E_o|` over a span, for every `tau` in `tau`, enclosed from above by
/// interval branch and bound over `[0, tau]` (`|E_o|` is even): the box of the largest upper
/// end is split until it is within [`SUP_REL`] of a value the function takes, or after
/// [`SUP_STEPS`] splits; the answer is that box's upper end, which no box exceeds.
pub(crate) fn offset_error_bound(tau: Ivl) -> f64 {
    if !(tau.lo > 0.0 && tau.hi <= 0.5) {
        return f64::INFINITY;
    }
    let top = tau.hi;
    let f = |lo: f64, hi: f64| offset_error_sq(tau, Ivl::new(lo, hi));
    let mut boxes: Vec<(f64, f64, f64)> = vec![(f(0.0, top).hi, 0.0, top)];
    let mut lower = f(0.0, 0.0).lo.max(f(top, top).lo);
    for _ in 0..SUP_STEPS {
        let Some((at, &(up, lo, hi))) = boxes
            .iter()
            .enumerate()
            .max_by(|x, y| x.1.0.total_cmp(&y.1.0))
        else {
            break;
        };
        let mid = 0.5 * (lo + hi);
        let open = up > lower + SUP_REL * lower.abs();
        if !(open && lo < mid && mid < hi) {
            return Ivl::new(0.0, up).sqrt().hi;
        }
        boxes.swap_remove(at);
        lower = lower.max(f(mid, mid).lo);
        boxes.push((f(lo, mid).hi, lo, mid));
        boxes.push((f(mid, hi).hi, mid, hi));
    }
    let up = boxes.iter().fold(0.0_f64, |m, b| m.max(b.0));
    Ivl::new(0.0, up).sqrt().hi
}

/// `2 pi` enclosed, in `T`.
fn tau_in<T: Ring>() -> T {
    T::lift(TAU).join_padded(T::lift(TAU), TAU * f64::EPSILON)
}

/// `atan` over `x` in `T`: the ends' `fmath::atan` (it rises), padded by its rounding.
fn atan_in<T: Ranged>(x: T) -> T {
    let (lo, hi) = x.ends();
    let (a, b) = (fmath::atan(lo), fmath::atan(hi));
    let pad = 2.0 * f64::EPSILON * a.abs().max(b.abs()) + f64::MIN_POSITIVE;
    T::lift(a).join_padded(T::lift(b), pad)
}

// ---------------------------------------------------------------- the radial law

/// The radial law of a screw: an offset along a direction square to the axis, piecewise
/// linear in the turn through `(angles, offsets)` and held beyond its ends.
#[derive(Debug, Clone, PartialEq)]
pub struct RadialLaw {
    direction: UnitVec3,
    angles: Vec<f64>,
    offsets: Vec<f64>,
}

/// Where a turn lies in a law: on a held stretch, or on the segment from `angles[i]`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum Segment {
    Held(f64),
    Linear(usize),
}

impl RadialLaw {
    /// The law moving the profile `offsets[i]` along `direction` at the turn `angles[i]`
    /// (radians, strictly ascending), linear between and held beyond.
    ///
    /// # Errors
    ///
    /// [`ProceduralError::Law`] (`Refused`) for a zero or non-finite direction, no angles,
    /// angles and offsets of different counts, a value that is not finite, or angles not
    /// strictly ascending.
    pub fn new(
        direction: Vec3,
        angles: Vec<f64>,
        offsets: Vec<f64>,
    ) -> Result<Self, ProceduralError> {
        let bad = ProceduralError::Law;
        if !direction.is_finite() || angles.is_empty() || angles.len() != offsets.len() {
            return Err(bad);
        }
        if !angles.iter().chain(&offsets).all(|x| x.is_finite()) {
            return Err(bad);
        }
        if angles.windows(2).any(|w| !matches!(w, [a, b] if a < b)) {
            return Err(bad);
        }
        let direction = direction.normalized().ok_or(bad)?;
        Ok(Self {
            direction,
            angles,
            offsets,
        })
    }

    /// The direction the offsets move along.
    #[must_use]
    pub const fn direction(&self) -> UnitVec3 {
        self.direction
    }

    /// The angles of the law's points, ascending.
    #[must_use]
    pub fn angles(&self) -> &[f64] {
        &self.angles
    }

    /// The offsets at [`RadialLaw::angles`].
    #[must_use]
    pub fn offsets(&self) -> &[f64] {
        &self.offsets
    }

    /// The offset at the turn `a`.
    #[must_use]
    pub fn offset(&self, a: f64) -> f64 {
        self.offset_on(self.segment(a), a)
    }

    /// The stretch that holds `at`: past the ends, held; inside, the segment it lies on (at
    /// a law point, the segment that starts there; at the last, the one that ends there).
    pub(crate) fn segment(&self, at: f64) -> Segment {
        let (first, last) = match (self.offsets.first(), self.offsets.last()) {
            (Some(&f), Some(&l)) => (f, l),
            _ => return Segment::Held(0.0),
        };
        let k = self.angles.partition_point(|&a| a <= at);
        if k == 0 {
            Segment::Held(first)
        } else if k >= self.angles.len() {
            if self.angles.last() == Some(&at) && k >= 2 {
                Segment::Linear(k - 2)
            } else {
                Segment::Held(last)
            }
        } else {
            Segment::Linear(k - 1)
        }
    }

    /// The ends of segment `i`: `(a0, o0, a1, o1)`.
    fn ends(&self, i: usize) -> Option<(f64, f64, f64, f64)> {
        let (&a0, &a1) = (self.angles.get(i)?, self.angles.get(i + 1)?);
        let (&o0, &o1) = (self.offsets.get(i)?, self.offsets.get(i + 1)?);
        Some((a0, o0, a1, o1))
    }

    /// The offset at `theta` on the stretch `seg`, in `T`.
    pub(crate) fn offset_on<T: Ring>(&self, seg: Segment, theta: T) -> T {
        match seg {
            Segment::Held(v) => T::lift(v),
            Segment::Linear(i) => self.ends(i).map_or_else(
                || T::lift(0.0),
                |(a0, o0, a1, o1)| {
                    let (a0, o0) = (T::lift(a0), T::lift(o0));
                    (T::lift(o1) - o0) * (theta - a0) / (T::lift(a1) - a0) + o0
                },
            ),
        }
    }

    /// The slope of the stretch `seg` (`0` held), in `f64` and enclosed.
    pub(crate) fn slope(&self, seg: Segment) -> (f64, Ivl) {
        match seg {
            Segment::Held(_) => (0.0, Ivl::from(0.0)),
            Segment::Linear(i) => self
                .ends(i)
                .map_or((0.0, Ivl::from(0.0)), |(a0, o0, a1, o1)| {
                    let x = (Ivl::from(o1) - Ivl::from(o0)) / (Ivl::from(a1) - Ivl::from(a0));
                    ((o1 - o0) / (a1 - a0), x)
                }),
        }
    }

    /// Every offset nought: the law moves nothing.
    pub(crate) fn is_nought(&self) -> bool {
        self.offsets.iter().all(|&o| o == 0.0)
    }

    /// The law of the turn run the other way, `delta'(-a) = delta(a)`, exactly.
    pub(crate) fn reversed(&self) -> Self {
        Self {
            direction: self.direction,
            angles: self.angles.iter().rev().map(|&a| -a).collect(),
            offsets: self.offsets.iter().rev().copied().collect(),
        }
    }
}

// ---------------------------------------------------------------- the screw

/// A screw motion over a turn: the axis, the pitch (the rise per whole turn, signed: right
/// handed about the axis where positive), the radial law, and the stations the turn is cut
/// at, chosen so the net is within the resolution's approximation tolerance of the
/// definition.
#[derive(Debug, Clone, PartialEq)]
pub struct Screw {
    origin: Point3,
    axis: UnitVec3,
    pitch: f64,
    law: Option<RadialLaw>,
    /// The law's direction made square to the axis (`0` without a law).
    radial: Vec3,
    stations: Vec<f64>,
    turn: Extent1d,
    approx: f64,
}

/// One span's data: its ends, middle, the `x` scale `2 tau / w`, and the law's stretch.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Span {
    pub(crate) t0: f64,
    pub(crate) t1: f64,
    pub(crate) seg: Segment,
}

/// A complex number `(re, im)`.
type C = (f64, f64);

const fn cmul(a: C, b: C) -> C {
    (a.0 * b.0 - a.1 * b.1, a.0 * b.1 + a.1 * b.0)
}

/// The largest gap of the net from the definition over a set of stations, split into its
/// lift and offset parts (each the largest over the spans).
#[derive(Debug, Clone, Copy, PartialEq)]
struct Gap {
    total: f64,
    lift: f64,
    offset: f64,
}

/// `[b0, b1]` cut into `n` equal spans, the last station `b1` exactly.
fn split(b0: f64, b1: f64, n: usize) -> impl Iterator<Item = f64> {
    let w = b1 - b0;
    (0..n)
        .map(move |k| b0 + w * (k as f64 / n as f64))
        .chain([b1])
}

/// A screw's checked motion before its stations are chosen.
#[derive(Debug, Clone, PartialEq)]
struct Unstationed {
    origin: Point3,
    axis: UnitVec3,
    pitch: f64,
    law: Option<RadialLaw>,
    radial: Vec3,
}

impl Unstationed {
    /// The checked motion.
    fn new(
        origin: Point3,
        axis: Vec3,
        pitch: f64,
        law: Option<RadialLaw>,
    ) -> Result<Self, ProceduralError> {
        if !(origin.is_finite() && axis.is_finite() && pitch.is_finite()) {
            return Err(ProceduralError::NotFinite);
        }
        let axis = axis.normalized().ok_or(ProceduralError::ZeroDirection)?;
        if pitch == 0.0 && law.is_none() {
            return Err(ProceduralError::ZeroPitch);
        }
        let radial = match &law {
            None => Vec3::splat(0.0),
            Some(l) => {
                let (k, e) = (axis.as_vec(), l.direction.as_vec());
                if e.dot(k).abs() > FLAT {
                    return Err(ProceduralError::Law);
                }
                (e - k * e.dot(k))
                    .normalized()
                    .ok_or(ProceduralError::Law)?
                    .as_vec()
            }
        };
        Ok(Self {
            origin,
            axis,
            pitch,
            law,
            radial,
        })
    }

    /// The screw over `stations` after checking them, its gap bounded over them.
    fn stationed(self, stations: Vec<f64>) -> Result<Screw, ProceduralError> {
        let (lo, hi) = match (stations.first(), stations.last()) {
            (Some(&a), Some(&b)) => (a, b),
            _ => {
                return Err(ProceduralError::Range {
                    min: f64::NAN,
                    max: f64::NAN,
                });
            }
        };
        let bad = ProceduralError::Range { min: lo, max: hi };
        let ok = stations.len() >= 2
            && stations.iter().all(|t| t.is_finite())
            && stations
                .windows(2)
                .all(|w| matches!(w, [a, b] if a < b && b - a <= 2.0 * MAX_HALF_ANGLE));
        if !ok {
            return Err(bad);
        }
        if stations.len() > MAX_SPANS + 1 {
            return Err(ProceduralError::TooFine {
                spans: stations.len() - 1,
            });
        }
        let gap = self.gap(&stations);
        if !gap.total.is_finite() {
            return Err(ProceduralError::NotFinite);
        }
        let turn = Extent1d::new(lo, hi).ok_or(bad)?;
        Ok(Screw {
            origin: self.origin,
            axis: self.axis,
            pitch: self.pitch,
            law: self.law,
            radial: self.radial,
            stations,
            turn,
            approx: gap.total,
        })
    }

    /// How many equal spans the piece `[b0, b1]` (one stretch of the law) needs.
    fn piece(&self, b0: f64, b1: f64, goal: f64) -> Result<usize, ProceduralError> {
        let w = b1 - b0;
        let mut n = ((w / FRAC_PI_4) * (1.0 + 1e-12)).ceil().max(1.0) as usize;
        for _ in 0..CHOICE_ROUNDS {
            if n > MAX_SPANS {
                break;
            }
            let st: Vec<f64> = split(b0, b1, n).collect();
            let g = self.gap(&st);
            if g.total <= goal {
                return Ok(n);
            }
            let r = g.total / goal;
            // The lift's gap falls as `n^-3`, the offset's as `n^-2`.
            let grow = if g.lift >= g.offset {
                fmath::cbrt(r)
            } else {
                r.sqrt()
            };
            let next = (n as f64 * grow * 1.01).ceil();
            n = if next.is_finite() && next < (MAX_SPANS + 1) as f64 {
                (next as usize).max(n + 1)
            } else {
                MAX_SPANS + 1
            };
        }
        Err(ProceduralError::TooFine { spans: n })
    }

    /// The largest gap over `stations`, each span's lift and offset gaps enclosed over the
    /// group of spans of one law stretch and of nearly one width (`tau` over the hull of
    /// the group's widths, so any grouping is sound).
    fn gap(&self, stations: &[f64]) -> Gap {
        let lambda = (Ivl::from(self.pitch) / tau_in::<Ivl>()).mag();
        let mut groups: BTreeMap<(Option<usize>, u64), Ivl> = BTreeMap::new();
        for w in stations.windows(2) {
            let &[t0, t1] = w else { continue };
            let seg = self
                .law
                .as_ref()
                .map_or(Segment::Held(0.0), |l| l.segment(0.5 * (t0 + t1)));
            let flat = self.law.as_ref().is_none_or(|l| l.slope(seg).0 == 0.0);
            let key = match seg {
                Segment::Linear(i) if !flat => Some(i),
                _ => None,
            };
            let width = Ivl::from(t1) - Ivl::from(t0);
            let bucket = (t1 - t0).to_bits() >> 20;
            groups
                .entry((key, bucket))
                .and_modify(|h| *h = h.hull(width))
                .or_insert(width);
        }
        let mut gap = Gap {
            total: 0.0,
            lift: 0.0,
            offset: 0.0,
        };
        for (&(key, _), &width) in &groups {
            let (s, c) = (width * Ivl::from(0.25)).sin_cos();
            let tau = s / c;
            let lift = if lambda == 0.0 {
                Ivl::from(0.0)
            } else {
                Ivl::from(lambda) * Ivl::from(lift_error_bound(tau))
            };
            let sigma = match (key, &self.law) {
                (Some(i), Some(l)) => l.slope(Segment::Linear(i)).1.mag(),
                _ => 0.0,
            };
            let offset = if sigma == 0.0 {
                Ivl::from(0.0)
            } else {
                Ivl::from(sigma)
                    * Ivl::from((width * Ivl::from(0.5)).hi)
                    * Ivl::from(offset_error_bound(tau))
            };
            let total = (lift * lift + offset * offset).sqrt().hi;
            gap.total = gap.total.max(total);
            gap.lift = gap.lift.max(lift.hi);
            gap.offset = gap.offset.max(offset.hi);
        }
        gap
    }
}

impl Screw {
    /// The screw about the line through `origin` along `axis`, rising `pitch` per whole turn
    /// (`theta` right handed about `axis`), over `turn` (radians), with the radial law
    /// `law`; its stations cut the turn into spans of at most `pi / 4`, the law's angles
    /// inside the turn among them, as few as hold the net within `res.approx()` of the
    /// definition.
    ///
    /// # Errors
    ///
    /// `Refused`: [`ProceduralError::NotFinite`], [`ProceduralError::ZeroDirection`] for a
    /// zero axis, [`ProceduralError::ZeroPitch`] for no pitch and no law (that is a
    /// [`super::Revolution`]), [`ProceduralError::Range`] for a turn that is a single value
    /// or not finite, [`ProceduralError::Law`] for a law direction not square to the axis;
    /// [`ProceduralError::TooFine`] (`Exhausted`) past [`MAX_SPANS`] spans.
    pub fn new(
        origin: Point3,
        axis: Vec3,
        pitch: f64,
        turn: Extent1d,
        law: Option<RadialLaw>,
        res: Resolution,
    ) -> Result<Self, ProceduralError> {
        let (lo, hi) = (turn.min(), turn.max());
        if !(lo.is_finite() && hi.is_finite()) || lo >= hi {
            return Err(ProceduralError::Range { min: lo, max: hi });
        }
        let s = Unstationed::new(origin, axis, pitch, law)?;
        let goal = res.approx().mm();
        let mut breaks = vec![lo];
        if let Some(l) = &s.law {
            breaks.extend(l.angles.iter().copied().filter(|&a| lo < a && a < hi));
        }
        breaks.push(hi);
        let mut stations = vec![lo];
        for w in breaks.windows(2) {
            let &[b0, b1] = w else { continue };
            let n = s.piece(b0, b1, goal)?;
            stations.extend(split(b0, b1, n).skip(1));
            if stations.len() > MAX_SPANS + 1 {
                return Err(ProceduralError::TooFine {
                    spans: stations.len() - 1,
                });
            }
        }
        s.stationed(stations)
    }

    /// The screw with `stations` as given (ascending, spans of at most `pi / 4`), its gap
    /// bounded: for transforms, reversals and tests.
    pub(crate) fn with_stations(
        origin: Point3,
        axis: Vec3,
        pitch: f64,
        law: Option<RadialLaw>,
        stations: Vec<f64>,
    ) -> Result<Self, ProceduralError> {
        Unstationed::new(origin, axis, pitch, law)?.stationed(stations)
    }

    /// A point of the axis.
    #[must_use]
    pub const fn origin(&self) -> Point3 {
        self.origin
    }

    /// The axis: the turn is right handed about it.
    #[must_use]
    pub const fn axis(&self) -> UnitVec3 {
        self.axis
    }

    /// The rise per whole turn along the axis (signed).
    #[must_use]
    pub const fn pitch(&self) -> f64 {
        self.pitch
    }

    /// The radial law.
    #[must_use]
    pub const fn law(&self) -> Option<&RadialLaw> {
        self.law.as_ref()
    }

    /// The stations: the net's knots across the turn, radians, ascending.
    #[must_use]
    pub fn stations(&self) -> &[f64] {
        &self.stations
    }

    /// The turn, from the first station to the last.
    #[must_use]
    pub const fn turn(&self) -> Extent1d {
        self.turn
    }

    /// The bound of the net's gap from the definition, before the net's rounding.
    #[must_use]
    pub const fn approx(&self) -> f64 {
        self.approx
    }

    /// The definition's angle at the net's parameter `t` (`theta(t_i) = t_i`).
    #[must_use]
    pub fn turn_at(&self, t: f64) -> f64 {
        let sp = self.span(self.span_of(t, Approach::Above));
        let (m, a) = sp.middle_scale();
        m + 2.0 * fmath::atan(a * (t - m))
    }

    /// The number of spans.
    pub(crate) fn spans(&self) -> usize {
        self.stations.len().saturating_sub(1)
    }

    /// The span holding `t`, `side` picking at a station; the end spans reach past the turn.
    pub(crate) fn span_of(&self, t: f64, side: Approach) -> usize {
        let n = self.spans();
        let inner = self.stations.get(1..n).unwrap_or(&[]);
        match side {
            Approach::Above => inner.partition_point(|&s| s <= t),
            Approach::Below => inner.partition_point(|&s| s < t),
        }
    }

    /// Span `i`.
    pub(crate) fn span(&self, i: usize) -> Span {
        let t0 = self.stations.get(i).copied().unwrap_or(f64::NAN);
        let t1 = self.stations.get(i + 1).copied().unwrap_or(f64::NAN);
        let seg = self
            .law
            .as_ref()
            .map_or(Segment::Held(0.0), |l| l.segment(0.5 * (t0 + t1)));
        Span { t0, t1, seg }
    }

    /// The part of `w` square to the axis.
    fn radial_of<T: Ring>(&self, w: Vec3<T>) -> Vec3<T> {
        let k = self.axis.as_vec().map(T::lift);
        w - k * w.dot(k)
    }

    /// The screw of `p` at the angle `theta` on the stretch `seg`, `(s, c)` its sine and
    /// cosine, divided by `corner` in its radial part (`1`, or a span's `cos(w / 2)` for the
    /// net's middle row) and lifted by `lift` (`lambda theta`).
    fn place<T: Ring>(
        &self,
        p: Point3<T>,
        theta: T,
        sc: (T, T),
        corner: T,
        lift: T,
        seg: Segment,
    ) -> Point3<T> {
        let k = self.axis.as_vec().map(T::lift);
        let o = self.origin.map(T::lift);
        let w = p - o;
        let r = self.radial_of(w);
        let (s, c) = sc;
        let mut turned = r * c + k.cross(r) * s;
        if let Some(l) = &self.law {
            let e = self.radial.map(T::lift);
            let d = l.offset_on(seg, theta);
            turned = turned + (e * c + k.cross(e) * s) * d;
        }
        let one = T::lift(1.0);
        o + k * (w.dot(k) + lift) + turned * (one / corner)
    }

    /// `S` at `u` (the net's parameter, any [`Trig`] scalar) for the profile point `p`: each
    /// span `u` meets, joined.
    pub(crate) fn eval<T: Trig>(&self, u: T, p: Point3<T>) -> Point3<T> {
        let (lo, hi) = u.ends();
        let i0 = self.span_of(lo, Approach::Above);
        let i1 = self.span_of(hi, Approach::Below).max(i0);
        let last = self.spans().saturating_sub(1);
        let lambda = T::lift(self.pitch) / tau_in::<T>();
        let mut out: Option<Point3<T>> = None;
        for i in i0..=i1 {
            let sp = self.span(i);
            let uc = if i0 == i1 {
                u
            } else {
                let a = if i == 0 { f64::NEG_INFINITY } else { sp.t0 };
                let b = if i >= last { f64::INFINITY } else { sp.t1 };
                u.clip(a, b)
            };
            let (t0, t1) = (T::lift(sp.t0), T::lift(sp.t1));
            let half = T::lift(0.5);
            let m = (t0 + t1) * half;
            let (s, c) = ((t1 - t0) * T::lift(0.25)).sin_cos();
            let scale = s / c * T::lift(2.0) / (t1 - t0);
            let theta = m + T::lift(2.0) * atan_in(scale * (uc - m));
            let q = self.place(
                p,
                theta,
                theta.sin_cos(),
                T::lift(1.0),
                lambda * theta,
                sp.seg,
            );
            out = Some(match out {
                None => q,
                Some(o) => Point3::new(o.x.join(q.x), o.y.join(q.y), o.z.join(q.z)),
            });
        }
        out.unwrap_or_else(|| Point3::new(T::undefined(), T::undefined(), T::undefined()))
    }

    /// The net's rows over the profile's control points `pts` (weights `w`): `2 n + 1` rows,
    /// the stations' and the spans' middles, with the `u` knots (degree 2).
    pub(crate) fn rows<T: Trig>(
        &self,
        pts: &[Point3<T>],
        w: &[T],
    ) -> (Vec<f64>, Vec<Point3<T>>, Vec<T>) {
        let n = self.spans();
        let lambda = T::lift(self.pitch) / tau_in::<T>();
        let one = T::lift(1.0);
        let mut out_p = Vec::with_capacity((2 * n + 1) * pts.len());
        let mut out_w = Vec::with_capacity(out_p.capacity());
        let mut knots = Vec::with_capacity(2 * n + 4);
        let station = |t: f64, seg: Segment, out_p: &mut Vec<Point3<T>>, out_w: &mut Vec<T>| {
            let th = T::lift(t);
            let sc = th.sin_cos();
            for (&p, &wj) in pts.iter().zip(w) {
                out_p.push(self.place(p, th, sc, one, lambda * th, seg));
                out_w.push(wj);
            }
        };
        for i in 0..n {
            let sp = self.span(i);
            knots.extend(core::iter::repeat_n(sp.t0, if i == 0 { 3 } else { 2 }));
            station(sp.t0, sp.seg, &mut out_p, &mut out_w);
            let (t0, t1) = (T::lift(sp.t0), T::lift(sp.t1));
            let m = (t0 + t1) * T::lift(0.5);
            let ch = ((t1 - t0) * T::lift(0.5)).sin_cos().1;
            let sc = m.sin_cos();
            for (&p, &wj) in pts.iter().zip(w) {
                out_p.push(self.place(p, m, sc, ch, lambda * m, sp.seg));
                out_w.push(wj * ch);
            }
        }
        let end = self.span(n.saturating_sub(1));
        station(end.t1, end.seg, &mut out_p, &mut out_w);
        knots.extend([end.t1; 3]);
        (knots, out_p, out_w)
    }

    /// The `u` derivatives at `t` (span by `side`): `(E^(i), theta^(i), delta^(i))` for
    /// `E = e^(i theta)`.
    pub(crate) fn turn_jet(&self, t: f64, side: Approach, i: usize) -> (C, f64, f64) {
        let sp = self.span(self.span_of(t, side));
        let (m, a) = sp.middle_scale();
        let x = a * (t - m);
        let d = 1.0 + x * x;
        let q: C = (1.0 / d, x / d);
        let (sm, cm) = fmath::sin_cos(m);
        let em: C = (cm, sm);
        let ia: C = (0.0, a);
        // n! (ia)^n q^(n+1)
        let term = |n: usize| {
            let mut z = q;
            let mut f = 1.0;
            for j in 1..=n {
                z = cmul(cmul(z, ia), q);
                f *= j as f64;
            }
            (z.0 * f, z.1 * f)
        };
        let e = if i == 0 {
            cmul(em, (2.0 * q.0 - 1.0, 2.0 * q.1))
        } else {
            let z = term(i);
            cmul(em, (2.0 * z.0, 2.0 * z.1))
        };
        let theta = if i == 0 {
            m + 2.0 * fmath::atan(x)
        } else {
            2.0 * a * term(i - 1).0
        };
        let delta = match &self.law {
            None => 0.0,
            Some(l) if i == 0 => l.offset_on(sp.seg, theta),
            Some(l) => l.slope(sp.seg).0 * theta,
        };
        (e, theta, delta)
    }

    /// The jet's term `S^(i, j)` for `j > 0` or `i = 0`, given the profile's `P^(j)` (`P - o`
    /// for `j = 0`).
    pub(crate) fn partial(&self, t: f64, side: Approach, i: usize, j: usize, pj: Vec3) -> Vec3 {
        let k = self.axis.as_vec();
        let r = self.radial_of(pj);
        let kr = k.cross(r);
        let (e, _, _) = self.turn_jet(t, side, i);
        let mut out = r * e.0 + kr * e.1;
        if i == 0 {
            out = out + k * pj.dot(k);
        }
        if j == 0 {
            let lambda = self.pitch / TAU;
            let (_, th, _) = self.turn_jet(t, side, i);
            out = out + k * (lambda * th);
            if self.law.is_some() {
                let ke = k.cross(self.radial);
                let mut binom = 1.0;
                for l in 0..=i {
                    let (_, _, dl) = self.turn_jet(t, side, l);
                    let (el, _, _) = self.turn_jet(t, side, i - l);
                    out = out + (self.radial * el.0 + ke * el.1) * (binom * dl);
                    binom = binom * (i - l) as f64 / (l + 1) as f64;
                }
            }
            if i == 0 {
                out = out + self.origin.to_vec();
            }
        }
        out
    }

    /// The same screw run the other way, `S'(-u) = S(u)`, exactly: the axis negated, the
    /// stations and the law's angles negated and reversed; the pitch and the gap kept.
    pub(crate) fn reversed(&self) -> Self {
        Self {
            origin: self.origin,
            axis: -self.axis,
            pitch: self.pitch,
            law: self.law.as_ref().map(RadialLaw::reversed),
            radial: self.radial,
            stations: self.stations.iter().rev().map(|&t| -t).collect(),
            turn: self.turn.negated(),
            approx: self.approx,
        }
    }

    /// The image under a similarity of scale `scale`, mirrored where `det < 0`: the origin
    /// and law direction `map`ped, the axis `axis` (the image's, turned for a mirror), the
    /// pitch and offsets scaled; the stations kept, the gap bounded again.
    pub(crate) fn transformed(
        &self,
        origin: Point3,
        axis: Vec3,
        scale: f64,
        mirror: bool,
        law_dir: impl Fn(Vec3) -> Vec3,
    ) -> Result<Self, ProceduralError> {
        let sign = if mirror { -1.0 } else { 1.0 };
        let law = match &self.law {
            None => None,
            Some(l) => Some(RadialLaw::new(
                law_dir(l.direction.as_vec()),
                l.angles.clone(),
                l.offsets.iter().map(|&o| o * scale).collect(),
            )?),
        };
        Self::with_stations(
            origin,
            axis,
            self.pitch * scale * sign,
            law,
            self.stations.clone(),
        )
    }
}

impl Span {
    /// The middle and the `x` scale `2 tan(w / 4) / w`, in `f64`.
    pub(crate) fn middle_scale(&self) -> (f64, f64) {
        let w = self.t1 - self.t0;
        (0.5 * (self.t0 + self.t1), 2.0 * fmath::tan(0.25 * w) / w)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The pin's `atan_is_enclosed`: the series encloses `atan` tightly on its range and is
    /// the whole line outside.
    #[test]
    fn atan_is_enclosed() {
        for x in [0.0, 1e-9, 0.05, 0.0985, 0.19891, 0.5] {
            let y = fmath::atan(x);
            let e = atan_point(x);
            assert!(e.lo <= y && y <= e.hi, "{x}: {e:?} misses {y}");
            assert!(e.hi - e.lo <= 1e-13 * y + 1e-290, "{x}: {e:?} is wide");
        }
        assert_eq!(atan_point(0.6), Ivl::whole());
        assert_eq!(atan_point(-0.1), Ivl::whole());
    }

    /// `E_l` in `f64`, from its definition.
    fn lift_error(tau: f64, x: f64) -> f64 {
        let k = 2.0 * (1.0 + tau * tau) * fmath::atan(tau) / tau;
        k * x / (1.0 + x * x) - 2.0 * fmath::atan(x)
    }

    /// The pin's `the_lift_error_bound_is_its_maximum`, judged against `E_l` sampled from its
    /// definition: no sample exceeds the bound, and the largest is within `1e-6` of it.
    #[test]
    fn the_lift_error_bound_is_its_maximum() {
        for half in [MAX_HALF_ANGLE, 0.1, 1e-3] {
            let tau = fmath::tan(half / 2.0);
            let bound = lift_error_bound(Ivl::from(tau));
            let mut most = 0.0_f64;
            for i in 0..=20000 {
                let x = tau * f64::from(i) / 20000.0;
                most = most.max(lift_error(tau, x).abs());
            }
            assert!(most <= bound, "{half}: sample {most} over {bound}");
            assert!(
                most >= bound * (1.0 - 1e-6),
                "{half}: {bound} loose over {most}"
            );
        }
    }

    /// The pin's `the_offset_error_bound_holds_and_is_tight`.
    #[test]
    fn the_offset_error_bound_holds_and_is_tight() {
        for half in [MAX_HALF_ANGLE, 0.05] {
            let tau = fmath::tan(half / 2.0);
            let bound = offset_error_bound(Ivl::from(tau));
            let mut most = 0.0_f64;
            for i in 0..=20000 {
                let x = tau * f64::from(i) / 20000.0;
                most = most.max(offset_error_sq(Ivl::from(tau), Ivl::from(x)).mid().sqrt());
            }
            assert!(most <= bound, "{half}: sample {most} over {bound}");
            assert!(
                most >= bound * (1.0 - 2.0 * SUP_REL),
                "{half}: {bound} loose over {most}"
            );
        }
    }

    /// A wider `tau` interval bounds every `tau` it holds.
    #[test]
    fn a_tau_interval_bounds_its_members() {
        let (a, b) = (fmath::tan(0.05), fmath::tan(0.1));
        let wide = Ivl::new(a, b);
        for t in [a, 0.5 * (a + b), b] {
            assert!(lift_error_bound(Ivl::from(t)) <= lift_error_bound(wide));
            assert!(offset_error_bound(Ivl::from(t)) <= offset_error_bound(wide));
        }
        assert_eq!(lift_error_bound(Ivl::new(0.0, 0.1)), f64::INFINITY);
        assert_eq!(offset_error_bound(Ivl::new(0.1, 0.6)), f64::INFINITY);
    }

    /// The pin's `hostile_definitions_are_refused`, its motion and law cases, by name.
    #[test]
    fn hostile_definitions_are_refused() {
        let o = Point3::ORIGIN;
        let res = Resolution::DEFAULT;
        let turn = Extent1d::new(0.0, 1.0).unwrap();
        let name = |r: Result<Screw, ProceduralError>| r.unwrap_err().name();
        assert_eq!(
            name(Screw::new(o, Vec3::splat(0.0), 1.0, turn, None, res)),
            "procedural_zero_direction"
        );
        assert_eq!(
            name(Screw::new(o, Vec3::Z, 0.0, turn, None, res)),
            "procedural_zero_pitch"
        );
        assert_eq!(
            name(Screw::new(o, Vec3::Z, f64::INFINITY, turn, None, res)),
            "procedural_not_finite"
        );
        let law =
            |d: Vec3, a: Vec<f64>, off: Vec<f64>| RadialLaw::new(d, a, off).unwrap_err().name();
        assert_eq!(law(Vec3::X, vec![], vec![]), "procedural_law");
        assert_eq!(
            law(Vec3::X, vec![1.0, 0.0], vec![0.0, 0.0]),
            "procedural_law"
        );
        assert_eq!(
            law(Vec3::X, vec![0.0, 1.0], vec![0.0, f64::NAN]),
            "procedural_law"
        );
        assert_eq!(
            law(Vec3::splat(0.0), vec![0.0], vec![0.0]),
            "procedural_law"
        );
        assert_eq!(law(Vec3::X, vec![0.0, 1.0], vec![0.0]), "procedural_law");
        // A law direction along the axis.
        let along = RadialLaw::new(Vec3::Z, vec![0.0], vec![1.0]).unwrap();
        assert_eq!(
            name(Screw::new(o, Vec3::Z, 1.0, turn, Some(along), res)),
            "procedural_law"
        );
        // A turn that is one value; stations not ascending or too wide.
        let point = Extent1d::point(1.0).unwrap();
        assert_eq!(
            name(Screw::new(o, Vec3::Z, 1.0, point, None, res)),
            "procedural_range"
        );
        assert_eq!(
            name(Screw::with_stations(o, Vec3::Z, 1.0, None, vec![0.0, 0.0])),
            "procedural_range"
        );
        assert_eq!(
            name(Screw::with_stations(o, Vec3::Z, 1.0, None, vec![0.0, 1.0])),
            "procedural_range"
        );
        assert_eq!(
            name(Screw::with_stations(o, Vec3::Z, 1.0, None, vec![0.0])),
            "procedural_range"
        );
        // Pitch nought with a law is a screw, not a revolution.
        let law = RadialLaw::new(Vec3::X, vec![0.0, 1.0], vec![0.0, 1.0]).unwrap();
        assert!(Screw::new(o, Vec3::Z, 0.0, turn, Some(law), res).is_ok());
    }

    /// Too many spans for the resolution is `Exhausted`, by name.
    #[test]
    fn too_fine_is_exhausted() {
        let tiny = Resolution::new(scx_num::units::Length::from_mm(1e-15)).unwrap();
        let turn = Extent1d::new(0.0, 200.0 * TAU).unwrap();
        let e = Screw::new(Point3::ORIGIN, Vec3::Z, 50.0, turn, None, tiny).unwrap_err();
        assert_eq!(e.name(), "procedural_too_fine");
        assert_eq!(e.kind(), scx_num::error::ErrorKind::Exhausted);
    }

    /// The pin's `each_span_keeps_its_own_half_angle`: the angle at each station is the
    /// station; inside a span it is `m + 2 atan(tan(h / 2) s)` of that span's half angle; the
    /// gap of mixed spans is the widest's.
    #[test]
    fn each_span_keeps_its_own_half_angle() {
        let law = || Some(RadialLaw::new(Vec3::X, vec![0.0, 2.0], vec![0.0, 0.5]).unwrap());
        let o = Point3::ORIGIN;
        let mixed = Screw::with_stations(o, Vec3::Z, 1.5, law(), vec![0.0, 0.2, 0.3, 0.4]).unwrap();
        for &t in mixed.stations() {
            assert!((mixed.turn_at(t) - t).abs() <= 1e-15, "{t}");
        }
        for (t0, t1) in [(0.0, 0.2), (0.2, 0.3), (0.3, 0.4)] {
            let (half, m) = (0.5 * (t1 - t0), 0.5 * (t0 + t1));
            let t = t0 + 0.3 * (t1 - t0);
            let s = 2.0 * (t - t0) / (t1 - t0) - 1.0;
            let want = m + 2.0 * fmath::atan(fmath::tan(half / 2.0) * s);
            assert!(
                (mixed.turn_at(t) - want).abs() <= 1e-15,
                "{t0}: {}",
                mixed.turn_at(t)
            );
        }
        let narrow = Screw::with_stations(o, Vec3::Z, 1.5, law(), vec![0.2, 0.3, 0.4]).unwrap();
        let wide = Screw::with_stations(o, Vec3::Z, 1.5, law(), vec![0.0, 0.2]).unwrap();
        assert!(narrow.approx() < wide.approx());
        assert_eq!(mixed.approx(), wide.approx());
    }

    /// The chosen stations hold the goal, the law's angles among them, spans of at most
    /// `pi / 4`; a coarser resolution needs no more.
    #[test]
    fn stations_hold_the_goal() {
        let law = RadialLaw::new(Vec3::X, vec![0.5, 3.0, 7.0], vec![0.0, 2.0, -1.0]).unwrap();
        let turn = Extent1d::new(0.0, 4.0 * TAU).unwrap();
        let res = Resolution::DEFAULT;
        let s = Screw::new(Point3::ORIGIN, Vec3::Z, 3.0, turn, Some(law.clone()), res).unwrap();
        assert!(s.approx() <= res.approx().mm());
        for a in [0.5, 3.0, 7.0] {
            assert!(s.stations().contains(&a), "{a}");
        }
        assert!(s.stations().windows(2).all(|w| w[1] - w[0] <= FRAC_PI_4));
        assert_eq!(s.stations().first(), Some(&0.0));
        assert_eq!(s.stations().last(), Some(&(4.0 * TAU)));
        let coarse = Resolution::new(scx_num::units::Length::from_mm(1e-3)).unwrap();
        let c = Screw::new(Point3::ORIGIN, Vec3::Z, 3.0, turn, Some(law), coarse).unwrap();
        assert!(c.spans() <= s.spans());
        assert!(c.approx() <= coarse.approx().mm());
    }
}
