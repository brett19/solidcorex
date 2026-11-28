// Written for solidcorex. Apache-2.0.
//! A NURBS curve against a plane, cylinder, cone, sphere or torus (research 77 §6.3, RY1).
//!
//! **The equation.** Each Bézier span of the curve, in homogeneous form `(X, Y, Z, H)` in the
//! surface's axes, is substituted into the kind's implicit form, as the analytic arms do
//! ([`Target::equation`](super::Target)): a polynomial `g(u)` of degree `d p` on the span's
//! `[0, 1]` (`d` 1 for the plane, 2 for the quadrics, 4 for the torus), `t` affine in `u`.
//! Every coefficient carries a running error bound ([`Bounded`]) from the curve's own data
//! through the blossoms of its spans and the frame change. The two spans at a knot share
//! that knot's point to the bit, so `g` is the same number at the knot from both sides and
//! a crossing there is seen by one span, or reported once at the knot by both.
//!
//! **The roots.** As the analytic arms: the sign changes of `g` and of the turning-point
//! polynomial (`g'`, or `H g' - d H' g` on a rational span) are isolated by
//! [`bernstein_roots`] and read as events, with the interior knots (which stand for
//! nothing in a run, as a closed conic's probe), the extent's ends, and a knot where the
//! one-sided slopes of `g / H^d` have opposite signs (a turning point no derivative's root
//! shows). Runs of near events are read by the analytic arms' [`classify`]: one tangent at
//! the nearest turning point, else a crossing at the one root, else a crossing at a lone end.
//!
//! **Overlaps.** A span whose `g` is zero within its coefficients' bound lies on the surface
//! to rounding; the certified gap ([`curve_surface_gap`]) proves it within `tol` before it
//! is read so. A run of near events holding such a span is one
//! [`CurveSurfaceHit::Overlap`] from its first event to its last (a NURBS curve can lie on a
//! surface over some spans and leave it over others). Every event near is the whole extent,
//! as the analytic arms. An overlap is clipped by `se` where the curve meets the planes that
//! hold `se`'s sides (each a degree-`p` polynomial per span, isolated the same way).
//!
//! **Certified, or typed.** Between two far events of a span `g` has one sign: its
//! coefficients restricted there are all beyond their bound (halved a fixed few times if
//! not). Where they are not, or where the isolation ran out on a piece whose ends are far
//! from the surface, a root may hide that rounding cannot place: the answer is
//! [`SurfaceError::Undecided`] at that place, never a lost hit or an empty answer.
//!
//! **Open curves.** The parameter is not periodic. On a closed curve (its ends the same
//! point) over its whole domain, a hit at each end of the extent is the same place, kept
//! once at the low end; an overlap is not joined across the ends.

use scx_algo::bernstein::{Bernstein2, Bounded, RootLimits, bernstein_roots};
use scx_curve::Curve;
use scx_num::budget::{Budget, Stop};
use scx_num::certified::gamma;
use scx_num::extent1d::Extent1d;
use scx_num::extent2d::Extent2d;
use scx_num::units::Resolution;
use scx_nurbs::{Approach, NurbsCurve};

use super::{Contact, CurveSurfaceHit, Event, Kind, OnSurface, Shape, Target, classify};
use crate::certificate::curve_spans;
use crate::error::SurfaceError;
use crate::gap::{Aim, GapError, Short, curve_surface_gap};
use crate::nurbs_invert::{Hb, up};
use crate::surface::Surface;

const EPS: f64 = f64::EPSILON;
const DEFECT: &str = "a NURBS curve's span equation could not be built";
/// The halvings that may certify the sign of `g` between two far events.
const FAR_DEPTH: u32 = 4;

/// One span of the curve over its part of the extent: `t = (1 - u) lo + u hi`, and the
/// homogeneous Bézier points over it in the target's axes.
struct Span {
    lo: f64,
    hi: f64,
    net: Vec<Hb>,
}

impl Span {
    fn t(&self, u: f64) -> f64 {
        match u {
            0.0 => self.lo,
            1.0 => self.hi,
            _ => (1.0 - u) * self.lo + u * self.hi,
        }
    }
}

/// [`super::intersect_curve`]'s reading for a NURBS curve: [`OnSurface::Isolated`] when every
/// event is near, else [`OnSurface::Leaves`] with the points and the overlaps.
#[expect(
    clippy::too_many_arguments,
    reason = "intersect_curve's arguments, taken"
)]
pub(super) fn isolate(
    n: &NurbsCurve<3>,
    c: &Curve,
    ce: &Extent1d,
    s: &Surface,
    target: &Target,
    se: &Extent2d,
    res: Resolution,
    budget: &mut Budget,
    stop: &Stop,
) -> Result<OnSurface, SurfaceError> {
    let tol = res.zone().mm();
    let spans = spans_over(n, ce, target);
    let d = match target.shape {
        Shape::Plane => 1.0,
        Shape::Torus { .. } => 4.0,
        _ => 2.0,
    };
    let rational = n.weights().is_some();
    let mut events: Vec<Event> = Vec::with_capacity(8 * spans.len() + 2);
    // Per span: whether it lies on the surface, and the end signs of its slope polynomial.
    let mut flat = Vec::with_capacity(spans.len());
    let mut slopes: Vec<(f64, f64)> = Vec::with_capacity(spans.len());
    for span in &spans {
        stop.check().map_err(|_| SurfaceError::Stopped)?;
        budget.charge(2).map_err(SurfaceError::Exhausted)?;
        let [x, y, z, h] = forms(&span.net)?;
        let g = equation(target.shape, &x, &y, &z, &h)?;
        if zero_within(&g) {
            prove_flat(c, span, s, tol, budget, stop)?;
            flat.push(true);
            slopes.push((0.0, 0.0));
            continue;
        }
        flat.push(false);
        let dg = if rational {
            defect(
                h.mul(&g.deriv_u())
                    .and_then(|a| h.deriv_u().mul(&g).and_then(|b| a.add_scaled(&b, -d))),
            )?
        } else {
            g.deriv_u()
        };
        let k = dg.f.coefficients();
        slopes.push((
            k.first().copied().unwrap_or(0.0),
            k.last().copied().unwrap_or(0.0),
        ));
        let mut mine: Vec<(f64, Event)> = Vec::new();
        let roots = bernstein_roots(g.f.coefficients(), 0.0, 1.0, RootLimits::DEFAULT);
        for &u in &roots.places {
            let t = polish(c, target, span, span.t(u));
            let e = Event::new(c, target, t, Kind::Root, tol);
            if !roots.isolated && e.distance > tol {
                // An undecided piece's end, off the surface: it holds roots rounding
                // cannot place.
                return Err(SurfaceError::Undecided { t });
            }
            mine.push((u, e));
        }
        if !zero_within(&dg) {
            let turns = bernstein_roots(dg.f.coefficients(), 0.0, 1.0, RootLimits::DEFAULT);
            for &u in &turns.places {
                mine.push((u, Event::new(c, target, span.t(u), Kind::Turn, tol)));
            }
        }
        certify_far(c, target, span, &g, &mut mine, tol)?;
        events.extend(mine.into_iter().map(|(_, e)| e));
    }
    // The interior knots, and those where the curve turns at a kink: the one-sided slopes
    // of `g / H^d` (the slope polynomial's end coefficients) of opposite signs.
    for i in 1..spans.len() {
        let (Some(left), Some(&(_, a)), Some(&(b, _))) =
            (spans.get(i - 1), slopes.get(i - 1), slopes.get(i))
        else {
            continue;
        };
        let t = left.hi;
        events.push(Event::new(c, target, t, Kind::Probe, tol));
        let curved = |k: usize| !flat.get(k).copied().unwrap_or(true);
        if curved(i - 1) && curved(i) && a * b < 0.0 {
            events.push(Event::new(c, target, t, Kind::Turn, tol));
        }
    }
    for t in [ce.min(), ce.max()] {
        events.push(Event::new(c, target, t, Kind::End, tol));
    }
    // A flat span's ends are on the surface: its gap is proved within `tol`.
    for (span, &f) in spans.iter().zip(&flat) {
        if f {
            for e in events
                .iter_mut()
                .filter(|e| e.t == span.lo || e.t == span.hi)
            {
                e.near = true;
            }
        }
    }
    events.sort_by(|a, b| a.t.total_cmp(&b.t).then(a.kind.cmp(&b.kind)));
    events.dedup_by(|b, a| a.kind == b.kind && (b.t - a.t).abs() <= 8.0 * EPS * a.t.abs().max(1.0));
    if events.iter().all(|e| e.near) {
        return Ok(OnSurface::Isolated(clip(
            c,
            &spans,
            target,
            se,
            ce.min(),
            ce.max(),
        )));
    }
    let flats: Vec<(f64, f64)> = spans
        .iter()
        .zip(&flat)
        .filter(|(_, f)| **f)
        .map(|(s, _)| (s.lo, s.hi))
        .collect();
    let mut hits = Vec::new();
    for run in events.split(|e| !e.near) {
        let (Some(first), Some(last)) = (run.first(), run.last()) else {
            continue;
        };
        if flats.iter().any(|&(a, b)| first.t <= a && b <= last.t) {
            for t in clip(c, &spans, target, se, first.t, last.t) {
                hits.push(CurveSurfaceHit::Overlap { t });
            }
        } else if let Some((t, contact)) = classify(run) {
            let point = c.point(t);
            if let Some(uv) = target.uv_in(point, se) {
                hits.push(CurveSurfaceHit::Point {
                    t,
                    uv,
                    point,
                    contact,
                });
            }
        }
    }
    if closed(n, ce) {
        merge_ends(&mut hits, ce);
    }
    hits.sort_by(|a, b| {
        let key = |h: &CurveSurfaceHit| match h {
            CurveSurfaceHit::Point { uv, .. } => (uv.x, uv.y),
            CurveSurfaceHit::Overlap { .. } => (f64::NEG_INFINITY, f64::NEG_INFINITY),
        };
        let (ka, kb) = (key(a), key(b));
        a.order_t()
            .total_cmp(&b.order_t())
            .then(ka.0.total_cmp(&kb.0))
            .then(ka.1.total_cmp(&kb.1))
    });
    Ok(OnSurface::Leaves(hits))
}

/// On a plane, a bound on the curve's height above it over `ce` and the rounding of an
/// event's distance (as [`super::curve_on_surface`] reads them): the largest height of the
/// spans' Bézier points over `ce` in the plane's axes, each `|Z| + err` over `H - err` (the
/// curve lies in their hull while every `H` is positive; infinite when one is not), and
/// `64 eps` of the largest coordinate. `None` on any other surface.
pub(super) fn plane_height(
    n: &NurbsCurve<3>,
    ce: &Extent1d,
    target: &Target,
) -> Option<(f64, f64)> {
    if !matches!(target.shape, Shape::Plane) {
        return None;
    }
    let mut height = 0.0_f64;
    let mut big = 0.0_f64;
    for span in spans_over(n, ce, target) {
        for &([x, y, z, h], [ex, ey, ez, eh]) in &span.net {
            let w = (h - eh) * (1.0 - 2.0 * EPS);
            if w.is_nan() || w <= 0.0 {
                return Some((f64::INFINITY, 0.0));
            }
            height = height.max(up((z.abs() + ez) / w));
            big = big.max(up((x.abs() + ex) / w)).max(up((y.abs() + ey) / w));
        }
    }
    big = big.max(height);
    let o = target.axes.o;
    let o = o.x.abs().max(o.y.abs()).max(o.z.abs());
    Some((height, 64.0 * EPS * (big + o)))
}

/// The clip of the whole of `ce` by `se`, as an overlap over every span reads it.
pub(super) fn clip_whole(
    c: &Curve,
    n: &NurbsCurve<3>,
    ce: &Extent1d,
    target: &Target,
    se: &Extent2d,
) -> Vec<Extent1d> {
    clip(
        c,
        &spans_over(n, ce, target),
        target,
        se,
        ce.min(),
        ce.max(),
    )
}

/// The spans of `n` over `ce` in the target's axes, each cut to its part of `ce` (the
/// extent may run past the domain, where the end spans continue). Neighbouring spans share
/// their knot's point to the bit: the larger error of the two, plus their difference.
fn spans_over(n: &NurbsCurve<3>, ce: &Extent1d, target: &Target) -> Vec<Span> {
    let ax = &target.axes;
    let mut all = curve_spans(n, ax.o, [ax.x, ax.y, ax.z]);
    for i in 1..all.len() {
        let prev = all.get(i - 1).and_then(|(_, _, net)| net.last().copied());
        if let (Some(p), Some((_, _, net))) = (prev, all.get_mut(i))
            && let Some(first) = net.first_mut()
        {
            let mut e = [0.0; 4];
            for (k, slot) in e.iter_mut().enumerate() {
                let (a, b) = (p.0.get(k).copied(), first.0.get(k).copied());
                let (ea, eb) = (p.1.get(k).copied(), first.1.get(k).copied());
                if let (Some(a), Some(b), Some(ea), Some(eb)) = (a, b, ea, eb) {
                    *slot = up(ea.max(eb) + (a - b).abs());
                }
            }
            *first = (p.0, e);
        }
    }
    let count = all.len();
    let (lo, hi) = (ce.min(), ce.max());
    let mut out = Vec::with_capacity(count);
    for (i, (a, b, net)) in all.into_iter().enumerate() {
        let from = if i == 0 { lo } else { lo.max(a) };
        let to = if i + 1 == count { hi } else { hi.min(b) };
        if from >= to && count > 1 {
            continue;
        }
        let (s0, s1) = ((from - a) / (b - a), (to - a) / (b - a));
        let net = if s0 == 0.0 && s1 == 1.0 {
            net
        } else {
            restrict(&net, s0, s1)
        };
        out.push(Span {
            lo: from,
            hi: to,
            net,
        });
    }
    out
}

/// The Bézier points of the same polynomial over `[s0, s1]` (anywhere on the line): each
/// the blossom at `s0` repeated `p - k` times and `s1` `k` times. A step `(1 - x) a + x b`
/// grows the carried error by `|1 - x| + |x|` (one inside `[0, 1]`) and adds three roundings
/// of the size it reaches.
fn restrict(net: &[Hb], s0: f64, s1: f64) -> Vec<Hb> {
    let p = net.len().saturating_sub(1);
    (0..=p)
        .map(|k| {
            let mut d = net.to_vec();
            for r in 1..=p {
                let x = if r <= p - k { s0 } else { s1 };
                let grow = (1.0 - x).abs() + x.abs();
                for i in 0..=p - r {
                    let (Some(a), Some(b)) = (d.get(i).copied(), d.get(i + 1).copied()) else {
                        continue;
                    };
                    let mut out = ([0.0; 4], [0.0; 4]);
                    for j in 0..4 {
                        let (va, vb) = (a.0.get(j).copied(), b.0.get(j).copied());
                        let (ea, eb) = (a.1.get(j).copied(), b.1.get(j).copied());
                        if let (Some(va), Some(vb), Some(ea), Some(eb)) = (va, vb, ea, eb)
                            && let (Some(v), Some(e)) = (out.0.get_mut(j), out.1.get_mut(j))
                        {
                            *v = (1.0 - x) * va + x * vb;
                            let size = grow * va.abs().max(vb.abs());
                            *e = up(grow * ea.max(eb) + 3.0 * EPS * size);
                        }
                    }
                    if let Some(slot) = d.get_mut(i) {
                        *slot = out;
                    }
                }
            }
            d.first().copied().unwrap_or(([f64::NAN; 4], [f64::NAN; 4]))
        })
        .collect()
}

/// The four forms `(X, Y, Z, H)` of a span, each its largest coefficient error.
fn forms(net: &[Hb]) -> Result<[Bounded; 4], SurfaceError> {
    let p = net.len().saturating_sub(1);
    let form = |k: usize| -> Result<Bounded, SurfaceError> {
        let c = net
            .iter()
            .map(|(h, _)| h.get(k).copied().unwrap_or(f64::NAN))
            .collect();
        let err = net.iter().fold(0.0_f64, |m, (_, e)| {
            m.max(e.get(k).copied().unwrap_or(f64::NAN))
        });
        defect(Bernstein2::new(p, 0, c)).map(|f| Bounded { f, err })
    };
    Ok([form(0)?, form(1)?, form(2)?, form(3)?])
}

/// The kind's implicit form on the span, homogeneous (as [`Target::equation`]).
fn equation(
    shape: Shape,
    x: &Bounded,
    y: &Bounded,
    z: &Bounded,
    h: &Bounded,
) -> Result<Bounded, SurfaceError> {
    let sq = |a: &Bounded| defect(a.mul(a));
    if matches!(shape, Shape::Plane) {
        return Ok(z.clone());
    }
    let rad2 = defect(sq(x)?.add(&sq(y)?))?;
    let h2 = sq(h)?;
    match shape {
        Shape::Plane => Ok(z.clone()),
        Shape::Cylinder { r, .. } => defect(rad2.add_scaled(&h2, -(r * r))),
        Shape::Cone { r, k } => {
            let w = defect(h.scale(r).add(&z.scale(k)))?;
            defect(rad2.sub(&sq(&w)?))
        }
        Shape::Sphere { r } => defect(defect(rad2.add(&sq(z)?))?.add_scaled(&h2, -(r * r))),
        Shape::Torus { big, small } => {
            let q = (big - small) * (big + small);
            let s = defect(defect(rad2.add(&sq(z)?))?.add_scaled(&h2, q))?;
            let b = defect(h2.mul(&rad2))?;
            defect(sq(&s)?.add_scaled(&b, -(4.0 * big * big)))
        }
    }
}

/// Whether every coefficient of `f` is within its error bound of zero: the exact form may
/// be zero.
fn zero_within(f: &Bounded) -> bool {
    f.f.coefficients().iter().all(|c| c.abs() <= f.err)
}

/// A span whose equation is zero within its bound: lies on the surface within `tol` by the
/// certified gap, or is undecided at its middle.
fn prove_flat(
    c: &Curve,
    span: &Span,
    s: &Surface,
    tol: f64,
    budget: &mut Budget,
    stop: &Stop,
) -> Result<(), SurfaceError> {
    let mid = 0.5 * (span.lo + span.hi);
    let Some(over) = Extent1d::new(span.lo, span.hi) else {
        return Ok(());
    };
    match curve_surface_gap(c, &over, s, Aim::Against(tol), budget, stop) {
        Ok(b) if b.exceeds(tol) == Some(false) => Ok(()),
        Err(GapError::Undecided {
            why: Short::Exhausted(x),
            ..
        }) => Err(SurfaceError::Exhausted(x)),
        Err(GapError::Undecided {
            why: Short::Stopped,
            ..
        }) => Err(SurfaceError::Stopped),
        Ok(_) | Err(_) => Err(SurfaceError::Undecided { t: mid }),
    }
}

/// Certifies that `g` keeps one sign between each two neighbouring far events of the span
/// that are not roots (a root of the cone's far nappe is one): its coefficients there all
/// beyond their bound, the piece halved up to [`FAR_DEPTH`] times. Sorts `mine` by `u`.
fn certify_far(
    c: &Curve,
    target: &Target,
    span: &Span,
    g: &Bounded,
    mine: &mut [(f64, Event)],
    tol: f64,
) -> Result<(), SurfaceError> {
    if signed(g) {
        mine.sort_by(|a, b| a.0.total_cmp(&b.0));
        return Ok(());
    }
    let end = |u: f64| (u, Event::new(c, target, span.t(u), Kind::Probe, tol));
    mine.sort_by(|a, b| a.0.total_cmp(&b.0));
    let mut marks: Vec<(f64, bool)> = Vec::with_capacity(mine.len() + 2);
    let far = |e: &Event| !e.near && e.kind != Kind::Root;
    let (u0, e0) = end(0.0);
    marks.push((u0, far(&e0)));
    marks.extend(mine.iter().map(|(u, e)| (*u, far(e))));
    let (u1, e1) = end(1.0);
    marks.push((u1, far(&e1)));
    for w in marks.windows(2) {
        let &[(a, fa), (b, fb)] = w else { continue };
        if fa && fb && b > a && !signed_on(g, a, b, FAR_DEPTH) {
            return Err(SurfaceError::Undecided {
                t: span.t(0.5 * (a + b)),
            });
        }
    }
    Ok(())
}

/// Whether `f`'s coefficients all lie beyond its bound on one side of zero: `f` has that
/// sign throughout.
fn signed(f: &Bounded) -> bool {
    let k = f.f.coefficients();
    k.iter().all(|c| *c > f.err) || k.iter().all(|c| *c < -f.err)
}

/// [`signed`] on `[a, b]` of `f`'s `[0, 1]`, halving up to `depth` times: the restriction's
/// rounding is two de Casteljau splits within `gamma(2n + 4)` of the largest coefficient.
fn signed_on(f: &Bounded, a: f64, b: f64, depth: u32) -> bool {
    let n = f.f.degrees().0;
    let add = 2.0 * gamma(2 * n + 4) * (f.f.max_abs_coefficient() + f.err);
    let piece = Bounded {
        f: f.f.restrict(a, b, 0.0, 1.0),
        err: up(f.err + add),
    };
    if signed(&piece) {
        return true;
    }
    if depth == 0 {
        return false;
    }
    let m = 0.5 * (a + b);
    signed_on(f, a, m, depth - 1) && signed_on(f, m, b, depth - 1)
}

/// A root `t` refined on the surface's own equation at the curve's point, as the analytic
/// arms' polish: Newton steps kept only while each lowers `|f|`, moves `t` by less than a
/// millionth of the span and stays in it.
fn polish(c: &Curve, target: &Target, span: &Span, mut t: f64) -> f64 {
    let reach = 1e-6 * (span.hi - span.lo);
    let local = |t: f64| {
        let j = c.jet::<1>(t, Approach::Above);
        let ax = &target.axes;
        let d = j.d.first().copied().unwrap_or_default();
        let (x, y, z) = ax.local(j.point);
        ([x, y, z], [d.dot(ax.x), d.dot(ax.y), d.dot(ax.z)])
    };
    let (l, mut d) = local(t);
    let (mut f, mut grad) = target.implicit(l);
    for _ in 0..3 {
        let slope = grad[0] * d[0] + grad[1] * d[1] + grad[2] * d[2];
        let step = f / slope;
        if !step.is_finite() || step.abs() > reach || step == 0.0 {
            break;
        }
        let next = t - step;
        if !(span.lo..=span.hi).contains(&next) {
            break;
        }
        let (l, d2) = local(next);
        let (g, grad2) = target.implicit(l);
        if g.abs() >= f.abs() {
            break;
        }
        (t, f, grad, d) = (next, g, grad2, d2);
    }
    t
}

/// Whether the curve is closed over `ce`: its ends the same point and `ce` its domain.
fn closed(n: &NurbsCurve<3>, ce: &Extent1d) -> bool {
    let dom = n.domain();
    if ce.min() != dom.min() || ce.max() != dom.max() {
        return false;
    }
    let (Some(a), Some(b)) = (n.points().first(), n.points().last()) else {
        return false;
    };
    a == b
}

/// On a closed curve, a point hit at each end of the extent is one place: the low one is
/// kept, tangent when either is.
fn merge_ends(hits: &mut Vec<CurveSurfaceHit>, ce: &Extent1d) {
    let at =
        |h: &CurveSurfaceHit, t0: f64| matches!(h, CurveSurfaceHit::Point { t, .. } if *t == t0);
    let lo = hits.iter().position(|h| at(h, ce.min()));
    let hi = hits.iter().position(|h| at(h, ce.max()));
    if let (Some(i), Some(j)) = (lo, hi)
        && i != j
    {
        let tangent = hits.get(j).is_some_and(|h| {
            matches!(
                h,
                CurveSurfaceHit::Point {
                    contact: Contact::Tangent,
                    ..
                }
            )
        });
        if tangent && let Some(CurveSurfaceHit::Point { contact, .. }) = hits.get_mut(i) {
            *contact = Contact::Tangent;
        }
        hits.remove(j);
    }
}

/// The pieces of `[a, b]` whose uv lies in `se`, sorted by `t` (as the analytic clip): cut
/// where the curve meets the planes that hold `se`'s sides, each piece kept when its
/// middle's uv is in `se`, neighbours inside merged, a piece of no length dropped.
fn clip(
    c: &Curve,
    spans: &[Span],
    target: &Target,
    se: &Extent2d,
    a: f64,
    b: f64,
) -> Vec<Extent1d> {
    let mut cuts = vec![a, b];
    let sides = target.sides(se);
    for span in spans.iter().filter(|s| s.hi > a && s.lo < b) {
        for &(normal, offset) in &sides {
            let k: Vec<f64> = span
                .net
                .iter()
                .map(|(h, _)| {
                    normal[0] * h[0] + normal[1] * h[1] + normal[2] * h[2] - offset * h[3]
                })
                .collect();
            for u in bernstein_roots(&k, 0.0, 1.0, RootLimits::DEFAULT).places {
                let t = span.t(u);
                if t > a && t < b {
                    cuts.push(t);
                }
            }
        }
    }
    cuts.sort_by(f64::total_cmp);
    cuts.dedup();
    let mut pieces: Vec<(f64, f64)> = Vec::new();
    for w in cuts.windows(2) {
        let &[x, y] = w else { continue };
        if target.uv_in(c.point(0.5 * (x + y)), se).is_none() {
            continue;
        }
        match pieces.last_mut() {
            Some(last) if last.1 == x => last.1 = y,
            _ => pieces.push((x, y)),
        }
    }
    pieces
        .into_iter()
        .filter_map(|(x, y)| Extent1d::new(x, y))
        .collect()
}

fn defect<T, E>(r: Result<T, E>) -> Result<T, SurfaceError> {
    r.map_err(|_| SurfaceError::Defect { reason: DEFECT })
}
