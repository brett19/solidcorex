// Written for solidcorex. Apache-2.0.
//! The certified checks behind an [`OnChart`] pcurve (research 73 §13.3 and §21): that the
//! curve stays off the surface's axis over the span, and that its continuous lift stays
//! under half a period from the reference line, so the lift `OnChart` picks (the copy
//! nearest the line) is the curve's own everywhere, not only at samples.
//!
//! One walk from the span's start to its end over boxes `B = [a, b]`, halving a box that
//! is not decided and moving on from a box that is:
//!
//! - **Axis.** The local `x` and `y` over `B` are enclosed by the centred form
//!   `q(m) + q'(B) (B - m)`, met with the plain enclosure `q(B)`, both in [`Ivl`] through the
//!   3-D curve's own `point_over` and `jet_over`. `B` is clear when the enclosure of
//!   `x^2 + y^2` lies above `tol^2`. A box whose middle is itself within `tol` of the axis
//!   holds a crossing, refined by golden section as the sampled check's are.
//! - **Band.** For each periodic coordinate, with `D(t) = w(t) - line(t)` the continuous lift
//!   less the reference line (slope `s`): by the mean value theorem
//!   `D(B) ⊂ D(a) + (w'(B) - s) [0, b - a]`, where `w'(B)` is the chart pcurve's derivative
//!   over `B` (a lift does not change a derivative). `B` is clear when that lies inside half
//!   a period less a margin of [`MARGIN`]; then the lift `OnChart` picks at `b` is the
//!   continuous one, so `D(b)` is met with `OnChart`'s own value there and the walk goes on.
//!   At the start, `D(a)` is `OnChart`'s value: the continuous lift is defined from it. When
//!   `D(a) + (w'(B) - s)(b - a)` lies wholly at or past half a period, the lift provably
//!   leaves the band at `b`: the band rule refuses, as the sampled one does.
//!
//! A box is halved at its middle until it is decided; the halves of a box clear of the axis
//! are not checked again. One that holds no `f64` inside, or
//! the [`BOXES`]th box tried, is [`PcurveError::Undecided`], never a pass. The walk takes no
//! choice from the budget, so a span decided within it gives the same answer every time.

use scx_curve::{Curve, OnChart};
use scx_num::extent1d::Extent1d;
use scx_num::interval::{Ivl, round_up};
use scx_num::trig::Ranged;
use scx_num::vector3d::{Point3, Vec3};
use scx_nurbs::Approach;

use crate::common::Axes;
use crate::pcurve::{Check, PcurveError};

/// The most boxes one walk tries. A span clear of the axis by a margin `g` (and of the
/// band's edge) takes about `log(span / g)` boxes at each near approach, so this is reached
/// only by a curve that runs within rounding of `tol` from the axis along a stretch.
pub(crate) const BOXES: usize = 1 << 14;

/// The band check accepts `|D| < (1 - MARGIN) p / 2`: the margin covers the rounding of the
/// line `OnChart` lifts by at a point (a few ulps of the coordinate).
const MARGIN: f64 = 1e-9;

/// What the walk found.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum Verdict {
    /// Every box is clear of the axis and the lift stays in the band.
    Clear,
    /// The lift leaves the band (the band rule's refusal).
    Band,
}

/// The walk over `pc`'s span `over`: [`Verdict::Clear`] or [`Verdict::Band`];
/// [`PcurveError::CrossesPole`] at a crossing of the axis, within `tol`, that the samples
/// missed; [`PcurveError::Undecided`] where neither can be proved. `axis` is whether the
/// surface has an axis to keep off (not a plane's or a cylinder's), `periods` the chart's.
pub(crate) fn certify(
    curve: &Curve,
    pc: &OnChart,
    axes: &Axes,
    over: Extent1d,
    tol: f64,
    axis: bool,
    periods: [Option<f64>; 2],
) -> Result<Verdict, PcurveError> {
    if !axis && periods.iter().all(Option::is_none) {
        return Ok(Verdict::Clear);
    }
    let (lo, hi) = (over.min(), over.max());
    let [r0, r1] = pc.reference();
    let lines = [(r0.x, r1.x), (r0.y, r1.y)].map(|(w0, w1)| (w0, (w1 - w0) / (hi - lo)));
    let line = |k: usize, t: Ivl| {
        let (w0, s) = lines.get(k).copied().unwrap_or((f64::NAN, f64::NAN));
        Ivl::from(w0) + (t - Ivl::from(lo)) * Ivl::from(s)
    };
    let direct = |k: usize, t: f64| {
        let p = pc.point_over::<Ivl>(Ivl::from(t));
        let w = if k == 0 { p.x } else { p.y };
        w - line(k, Ivl::from(t))
    };
    let tol2 = round_up(tol * tol);
    let mut d_a = [direct(0, lo), direct(1, lo)];
    let mut a = lo;
    // Each right end, with whether a box holding `[a, end]` was already proved off the axis
    // (a sub-box of a clear box is clear: the enclosures are inclusion-monotone).
    let mut ends = vec![(hi, !axis)];
    let mut tried = 0_usize;
    while let Some(&(b, known)) = ends.last() {
        tried += 1;
        let m = 0.5 * (a + b);
        let inner = a < m && m < b;
        let bx = Ivl::new(a, b);
        let mut clear = true;
        let mut check = Check::Band;
        let axis_clear = known || off_axis(curve, axes, bx, m, tol2);
        if !axis_clear {
            clear = false;
            check = Check::Axis;
            if inner && rho(curve, axes, m) <= tol {
                return Err(PcurveError::CrossesPole {
                    at: crossing(curve, axes, a, b, m, tol),
                });
            }
        }
        let mut next = d_a;
        if clear {
            let slope = pc.jet_over::<Ivl, 1>(bx, Approach::Above).d[0];
            let width = Ivl::from(b) - Ivl::from(a);
            for (k, p) in periods.iter().enumerate() {
                let Some(p) = *p else { continue };
                let (Some(&dk), Some(n)) = (d_a.get(k), next.get_mut(k)) else {
                    continue;
                };
                let s = lines.get(k).map_or(f64::NAN, |l| l.1);
                let rate = (if k == 0 { slope.x } else { slope.y }) - Ivl::from(s);
                let half = 0.5 * p;
                let band = dk + rate * Ivl::new(0.0, width.hi);
                if band.lo > -half * (1.0 - MARGIN) && band.hi < half * (1.0 - MARGIN) {
                    *n = meet(dk + rate * width, direct(k, b));
                    continue;
                }
                clear = false;
                let at_b = dk + rate * width;
                if at_b.lo >= half || at_b.hi <= -half {
                    return Ok(Verdict::Band);
                }
            }
        }
        if clear {
            ends.pop();
            a = b;
            d_a = next;
            continue;
        }
        if !inner || tried >= BOXES {
            return Err(PcurveError::Undecided { at: a, check });
        }
        ends.push((m, axis_clear));
    }
    Ok(Verdict::Clear)
}

/// The box `t` (middle `m`) is clear of the axis: `x^2 + y^2 > tol2` over it.
fn off_axis(curve: &Curve, axes: &Axes, t: Ivl, m: f64, tol2: f64) -> bool {
    let jet = curve.jet_over::<Ivl, 1>(t, Approach::Above);
    let at_m = curve.point_over::<Ivl>(Ivl::from(m));
    let dt = t - Ivl::from(m);
    let [d] = jet.d;
    let mut r2 = Ivl::from(0.0);
    for e in [axes.x, axes.y] {
        let plain = local(jet.point, axes.o, e);
        let centred = local(at_m, axes.o, e) + along(d, e) * dt;
        r2 = r2 + meet(plain, centred).sqr();
    }
    r2.lo > tol2
}

/// `(p - o) . e` enclosed.
fn local(p: Point3<Ivl>, o: Point3, e: Vec3) -> Ivl {
    (p.x - Ivl::from(o.x)) * Ivl::from(e.x)
        + (p.y - Ivl::from(o.y)) * Ivl::from(e.y)
        + (p.z - Ivl::from(o.z)) * Ivl::from(e.z)
}

/// `d . e` enclosed.
fn along(d: Vec3<Ivl>, e: Vec3) -> Ivl {
    d.x * Ivl::from(e.x) + d.y * Ivl::from(e.y) + d.z * Ivl::from(e.z)
}

/// The common part of two enclosures of one value; the first where they miss (which
/// rounding cannot cause, but a NaN can).
fn meet(a: Ivl, b: Ivl) -> Ivl {
    let (lo, hi) = (a.lo.max(b.lo), a.hi.min(b.hi));
    if lo <= hi { Ivl::new(lo, hi) } else { a }
}

/// The distance of the curve's point at `t` from the axis, in `f64`.
fn rho(curve: &Curve, axes: &Axes, t: f64) -> f64 {
    let (x, y, _) = axes.local(curve.point(t));
    scx_num::fmath::hypot(x, y)
}

/// Where in `[a, b]` the curve meets the axis, its middle `m` being within `tol`: the least
/// of the distance by golden section when that is within `tol`, else `m`.
fn crossing(curve: &Curve, axes: &Axes, a: f64, b: f64, m: f64, tol: f64) -> f64 {
    let (at, r) = crate::pcurve::golden_min(&|t| rho(curve, axes, t), a, b);
    if r <= tol { at } else { m }
}
