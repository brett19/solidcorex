// Written for solidcorex. Apache-2.0.
//! [`Chart`] and [`OnChart`]: an exact pcurve, a 3-D curve seen through the closed-form chart
//! inverse of an analytic surface (research 73 SM1, §13).
//!
//! A [`Chart`] is the part of an analytic surface its inverse needs: the frame, the sense
//! (the surfaces' `Z` is the frame's z times it, 63 D14) and the kind's sizes the uv depends
//! on. Its inverse is the surfaces' own (scx-surface `uv_in`), in the NURBS-compatible uv
//! (63 D13):
//!
//! | kind | `u` | `v` |
//! |---|---|---|
//! | plane | `x` | `y` |
//! | cylinder | the turn of period `2 pi r` at `(x, y)` | `z` |
//! | cone | the unit turn at `(x, y)` | `z` (the nappe with `r + k v > 0`) |
//! | sphere | the unit turn at `(x, y)` | the half turn at `(rho, z)` |
//! | torus | the unit turn at `(x, y)` | the unit turn at `(rho - R, z)` |
//!
//! with `(x, y, z)` the local coordinates and `rho = |(x, y)|`. "The turn at `(a, b)`" is
//! [`Arcs::of_direction`]: the arc is read from the signs and
//! `u = (q + 1/2) L + tau / a`, `tau = b' / (|(a, b)| + a')` with `(a', b')` turned back by
//! the arc's middle. No `atan2`: only ring operations and one square root.
//!
//! An [`OnChart`] is a 3-D curve `C` and a chart, `uv(t) = chart⁻¹(C(t))` at the edge's own
//! parameter. The jets are exact Taylor arithmetic through that formula: `C`'s jet gives the
//! local coordinates' series, a square root series gives `|(a, b)|`, and the arc formula's
//! quotient is a series quotient ([`Arcs::direction_series`]). So the value and every
//! derivative are the closed form's, in `f64` and in [`Ivl`](scx_num::interval::Ivl)
//! (an enclosure over every arc a box meets).
//!
//! The periodic coordinates are lifted by a reference line over the span the pcurve was
//! made over: `over` and the line's uv at its two ends. At `t`, each periodic coordinate is
//! the principal value plus the whole periods that bring it nearest the line at `t`. A
//! recorded turn count alone cannot do this: a closed curve's two ends are the same point
//! with different lifts. The maker (scx-surface `pcurve_of`) centres the line on the
//! curve's lifted values and checks that they stay within a quarter period of it, so the
//! nearest lift is the curve's own with a quarter period to spare.

use core::f64::consts::PI;

use scx_num::domain::ParamDomain;
use scx_num::extent1d::Extent1d;
use scx_num::fmath;
use scx_num::frame::{Frame, Sense};
use scx_num::jet::CurveJet2;
use scx_num::math::EFF_ZERO;
use scx_num::trig::Trig;
use scx_num::units::Length;
use scx_num::vector2d::{Point2, Vec2};
use scx_num::vector3d::{Point3, Vec3};
use scx_nurbs::Approach;

use crate::arcs::Arcs;
use crate::curve::Curve;
use crate::error::{CurveError, Degenerate, Part};

/// A component within this fraction of the other is on the axis between two arcs: the
/// side then picks the arc, so a break the rounded point misses by an ulp is still a break.
const NEAR: f64 = 4.0 * f64::EPSILON;

/// How many Taylor terms the evaluator carries: a jet of `D >= TERMS` is undefined.
const TERMS: usize = 16;

/// The unit turn, every angle direction but the cylinder's.
const TURN: Arcs = Arcs::turn(1.0);

/// The sphere's latitude.
const HALF: Arcs = Arcs::half();

/// Which analytic surface a [`Chart`] inverts, with the sizes its uv depends on.
#[derive(Debug, Clone, Copy, PartialEq)]
#[non_exhaustive]
pub enum ChartKind {
    /// A plane: `u = x`, `v = y`.
    Plane,
    /// A cylinder: `u` on the turn of period `2 pi radius`, `v = z`.
    Cylinder {
        /// The radius, which sets `u`'s period.
        radius: Length,
    },
    /// A cone: `u` on the unit turn, `v = z`.
    Cone,
    /// A sphere: `u` on the unit turn, `v` on the half turn of `(rho, z)`.
    Sphere,
    /// A ring torus: `u` on the unit turn, `v` on the unit turn of `(rho - major, z)`.
    Torus {
        /// The major radius, the centre circle's.
        major: Length,
    },
}

/// The closed-form uv inverse of an analytic surface; see the [module](self) docs.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Chart {
    frame: Frame,
    sense: Sense,
    kind: ChartKind,
}

/// The local frame, `Z` the frame's z times the sense.
#[derive(Debug, Clone, Copy)]
struct Local {
    o: Point3,
    x: Vec3,
    y: Vec3,
    z: Vec3,
}

/// The shape the evaluator dispatches on, the sizes as plain numbers.
#[derive(Debug, Clone, Copy)]
enum Shape {
    Plane,
    Cylinder(Arcs),
    Cone,
    Sphere,
    Torus(f64),
}

impl Chart {
    /// The chart of the `kind` surface about `frame` in `sense`. A plane has no sense (its
    /// uv is `x`, `y`), so a plane's is [`Sense::Forward`].
    ///
    /// # Errors
    ///
    /// [`CurveError::NotFinite`] ([`Part::Radius`]) for a radius or major radius that is not
    /// finite, [`Degenerate::Radius`] for one not above `EFF_ZERO`.
    pub fn new(frame: Frame, sense: Sense, kind: ChartKind) -> crate::Result<Self> {
        let size = match kind {
            ChartKind::Cylinder { radius } => Some(radius.mm()),
            ChartKind::Torus { major } => Some(major.mm()),
            ChartKind::Plane | ChartKind::Cone | ChartKind::Sphere => None,
        };
        if let Some(radius) = size {
            if !radius.is_finite() {
                return Err(CurveError::NotFinite(Part::Radius));
            }
            if radius <= EFF_ZERO {
                return Err(CurveError::Degenerate(Degenerate::Radius { radius }));
            }
        }
        let sense = match kind {
            ChartKind::Plane => Sense::Forward,
            _ => sense,
        };
        Ok(Self { frame, sense, kind })
    }

    /// The frame.
    #[must_use]
    pub const fn frame(&self) -> &Frame {
        &self.frame
    }

    /// The sense: the surfaces' `Z` is the frame's z times it.
    #[must_use]
    pub const fn sense(&self) -> Sense {
        self.sense
    }

    /// The kind and its sizes.
    #[must_use]
    pub const fn kind(&self) -> ChartKind {
        self.kind
    }

    /// The period of `u` and of `v`, `None` where the coordinate is not periodic: the
    /// cylinder's `u` is `2 pi r`, every other angle `1`.
    #[must_use]
    pub fn periods(&self) -> [Option<f64>; 2] {
        match self.shape() {
            Shape::Plane => [None, None],
            Shape::Cylinder(arcs) => [Some(arcs.length()), None],
            Shape::Cone | Shape::Sphere => [Some(1.0), None],
            Shape::Torus(_) => [Some(1.0), Some(1.0)],
        }
    }

    /// The principal uv of `p`, each periodic coordinate in `[0, period)`: the surfaces'
    /// own inverse, bit for bit (a point on the axis gets `u = 0`, a pole the pole's `v`).
    #[must_use]
    pub fn inverse(&self, p: Point3) -> Point2 {
        let f = self.local();
        let d = p - f.o;
        let (x, y, z) = (d.dot(f.x), d.dot(f.y), d.dot(f.z));
        let rho = || fmath::hypot(x, y);
        match self.shape() {
            Shape::Plane => Point2::new(x, y),
            Shape::Cylinder(arcs) => Point2::new(arcs.of_direction(x, y), z),
            Shape::Cone => Point2::new(TURN.of_direction(x, y), z),
            Shape::Sphere => Point2::new(TURN.of_direction(x, y), HALF.of_direction(rho(), z)),
            Shape::Torus(big) => {
                Point2::new(TURN.of_direction(x, y), TURN.of_direction(rho() - big, z))
            }
        }
    }

    fn shape(&self) -> Shape {
        match self.kind {
            ChartKind::Plane => Shape::Plane,
            ChartKind::Cylinder { radius } => Shape::Cylinder(Arcs::turn(2.0 * PI * radius.mm())),
            ChartKind::Cone => Shape::Cone,
            ChartKind::Sphere => Shape::Sphere,
            ChartKind::Torus { major } => Shape::Torus(major.mm()),
        }
    }

    fn local(&self) -> Local {
        Local {
            o: self.frame.origin(),
            x: *self.frame.x(),
            y: *self.frame.y(),
            z: *self.frame.z() * self.sense.sign(),
        }
    }
}

/// A 3-D curve through a [`Chart`]'s inverse at its own parameter, lifted by the span it
/// was made over; see the [module](self) docs.
#[derive(Debug, Clone, PartialEq)]
pub struct OnChart {
    curve: Curve,
    chart: Chart,
    over: Extent1d,
    reference: [Point2; 2],
}

impl OnChart {
    /// The pcurve of `curve` on `chart`, each periodic coordinate lifted to the copy
    /// nearest the straight line that is `reference` at `over`'s ends. The caller has
    /// checked that the curve lies on the surface and that its lifted values stay within a
    /// quarter period of that line (scx-surface `pcurve_of` does).
    ///
    /// # Errors
    ///
    /// [`Degenerate::Extent`] for an `over` of no length or one not finite,
    /// [`CurveError::NotFinite`] ([`Part::Origin`]) for a reference that is not finite.
    pub fn new(
        curve: Curve,
        chart: Chart,
        over: Extent1d,
        reference: [Point2; 2],
    ) -> crate::Result<Self> {
        if !(over.length() > 0.0 && over.length().is_finite()) {
            return Err(CurveError::Degenerate(Degenerate::Extent {
                at: over.min(),
            }));
        }
        if !reference.iter().all(|p| p.x.is_finite() && p.y.is_finite()) {
            return Err(CurveError::NotFinite(Part::Origin));
        }
        Ok(Self {
            curve,
            chart,
            over,
            reference,
        })
    }

    /// The 3-D curve.
    #[must_use]
    pub const fn curve(&self) -> &Curve {
        &self.curve
    }

    /// The chart.
    #[must_use]
    pub const fn chart(&self) -> &Chart {
        &self.chart
    }

    /// The span the lift was made over.
    #[must_use]
    pub const fn over(&self) -> Extent1d {
        self.over
    }

    /// The reference line's uv at `over`'s start and end.
    #[must_use]
    pub const fn reference(&self) -> [Point2; 2] {
        self.reference
    }

    /// The 3-D curve's domain.
    #[must_use]
    pub fn domain(&self) -> ParamDomain {
        self.curve.domain()
    }

    /// The uv at `t`.
    #[must_use]
    pub fn point(&self, t: f64) -> Point2 {
        self.jet_over::<f64, 0>(t, Approach::Above).point
    }

    /// The uv and its first `D` derivatives at `t`, from `side` at a break (of the 3-D
    /// curve's, or of an arc's in the uv).
    #[must_use]
    pub fn jet<const D: usize>(&self, t: f64, side: Approach) -> CurveJet2<D> {
        self.jet_over(t, side)
    }

    /// The uv at `t` in the scalar `T`.
    #[must_use]
    pub fn point_over<T: Trig>(&self, t: T) -> Point2<T> {
        self.jet_over::<T, 0>(t, Approach::Above).point
    }

    /// [`Self::point`] at each of `ts`, into `out`, zipped.
    pub(crate) fn fill(&self, ts: &[f64], out: &mut [Point2]) {
        for (p, &t) in out.iter_mut().zip(ts) {
            *p = self.point(t);
        }
    }

    /// The uv and its first `D` derivatives at `t` in the scalar `T`, from `side` at a
    /// break: the closed form's Taylor series through the 3-D curve's jet (the module
    /// docs). Undefined (NaN, or the whole line) on the axis of a turn, at a sphere's pole,
    /// and for `D >= 16`; an interval meeting several arcs holds each arc's values.
    #[must_use]
    pub fn jet_over<T: Trig, const D: usize>(&self, t: T, side: Approach) -> CurveJet2<D, T> {
        let n = D + 1;
        if n > TERMS {
            return undefined_jet();
        }
        let c = self.curve.jet_over::<T, D>(t, side);
        let f = self.chart.local();
        let l = T::lift;
        let zero = l(0.0);
        let mut x = [zero; TERMS];
        let mut y = [zero; TERMS];
        let mut z = [zero; TERMS];
        let at = |v: Vec3<T>, a: Vec3| v.x * l(a.x) + v.y * l(a.y) + v.z * l(a.z);
        let p0 = Vec3::new(
            c.point.x - l(f.o.x),
            c.point.y - l(f.o.y),
            c.point.z - l(f.o.z),
        );
        x[0] = at(p0, f.x);
        y[0] = at(p0, f.y);
        z[0] = at(p0, f.z);
        let mut fact = 1.0;
        for (k, d) in c.d.iter().enumerate() {
            fact *= (k + 1) as f64;
            let i = k + 1;
            if let (Some(xs), Some(ys), Some(zs)) = (x.get_mut(i), y.get_mut(i), z.get_mut(i)) {
                *xs = at(*d, f.x) / l(fact);
                *ys = at(*d, f.y) / l(fact);
                *zs = at(*d, f.z) / l(fact);
            }
        }
        let (x, y, z) = (head(&x, n), head(&y, n), head(&z, n));
        let mut u = [zero; TERMS];
        let mut v = [zero; TERMS];
        let (us, vs) = (head_mut(&mut u, n), head_mut(&mut v, n));
        let periods = self.chart.periods();
        let lift_u = |w: T| self.lift(t, 0, w, periods[0]);
        let lift_v = |w: T| self.lift(t, 1, w, periods[1]);
        match self.chart.shape() {
            Shape::Plane => {
                us.copy_from_slice(x);
                vs.copy_from_slice(y);
            }
            Shape::Cylinder(arcs) => {
                angle(arcs, x, y, side, &lift_u, us);
                vs.copy_from_slice(z);
            }
            Shape::Cone => {
                angle(TURN, x, y, side, &lift_u, us);
                vs.copy_from_slice(z);
            }
            Shape::Sphere => {
                angle(TURN, x, y, side, &lift_u, us);
                let mut rho = [zero; TERMS];
                let rho = head_mut(&mut rho, n);
                length(x, y, rho);
                angle(HALF, rho, z, side, &lift_v, vs);
            }
            Shape::Torus(big) => {
                angle(TURN, x, y, side, &lift_u, us);
                let mut off = [zero; TERMS];
                let off = head_mut(&mut off, n);
                length(x, y, off);
                if let Some(o) = off.first_mut() {
                    *o = *o - l(big);
                }
                angle(TURN, off, z, side, &lift_v, vs);
            }
        }
        let mut d = [Vec2::new(zero, zero); D];
        let mut fact = 1.0;
        for (k, slot) in d.iter_mut().enumerate() {
            fact *= (k + 1) as f64;
            let du = us.get(k + 1).copied().unwrap_or(zero);
            let dv = vs.get(k + 1).copied().unwrap_or(zero);
            *slot = Vec2::new(du * l(fact), dv * l(fact));
        }
        let point = Point2::new(
            us.first().copied().unwrap_or_else(T::undefined),
            vs.first().copied().unwrap_or_else(T::undefined),
        );
        CurveJet2 { point, d }
    }

    /// The principal value `w` of coordinate `axis` plus the whole periods bringing it
    /// nearest the reference line at `t`; an interval holds each
    /// of up to three such lifts, more is undefined.
    fn lift<T: Trig>(&self, t: T, axis: usize, w: T, period: Option<f64>) -> T {
        let Some(period) = period else {
            return w;
        };
        let pick = |p: Point2| if axis == 0 { p.x } else { p.y };
        let (t0, t1) = (self.over.min(), self.over.max());
        let (w0, w1) = (pick(self.reference[0]), pick(self.reference[1]));
        let l = T::lift;
        let line = l(w0) + (t - l(t0)) * l((w1 - w0) / (t1 - t0));
        let (lo, hi) = ((line - w) / l(period)).ends();
        let (klo, khi) = (lo.round(), hi.round());
        if !(klo.is_finite() && khi.is_finite()) || khi - klo > 2.0 {
            return T::undefined();
        }
        let mut out = w + l(klo) * l(period);
        let mut k = klo + 1.0;
        while k <= khi {
            out = out.join(w + l(k) * l(period));
            k += 1.0;
        }
        out
    }

    /// The same pcurve run the other way, `C'(t) = C(-t)`: the 3-D curve reversed, the
    /// span negated and the reference's ends swapped.
    #[must_use]
    pub fn reversed(&self) -> Self {
        Self {
            curve: self.curve.reversed(),
            chart: self.chart,
            over: self.over.negated(),
            reference: [self.reference[1], self.reference[0]],
        }
    }

    /// The same pcurve with the 3-D curve over `e` ([`Curve::restrict`]); the lift is
    /// unchanged.
    ///
    /// # Errors
    ///
    /// As [`Curve::restrict`].
    pub fn restrict(&self, e: Extent1d) -> crate::Result<Self> {
        Ok(Self {
            curve: self.curve.restrict(e)?,
            ..self.clone()
        })
    }

    /// The same pcurve moved by `turns` whole periods in `u` and `v` (a coordinate with no
    /// period ignores its count).
    ///
    /// # Errors
    ///
    /// [`CurveError::NotFinite`] ([`Part::Origin`]) for a shift that is not finite.
    pub fn shifted(&self, turns: [f64; 2]) -> crate::Result<Self> {
        let [pu, pv] = self.chart.periods();
        let step = Vec2::new(
            pu.map_or(0.0, |p| turns[0] * p),
            pv.map_or(0.0, |p| turns[1] * p),
        );
        let reference = self.reference.map(|e| e + step);
        Self::new(self.curve.clone(), self.chart, self.over, reference)
    }
}

/// An arc and the ranges of `a` and of `b` on it.
type Quad = (usize, (f64, f64), (f64, f64));

/// The first `n` of `s` (all of it when shorter).
fn head<T>(s: &[T], n: usize) -> &[T] {
    s.split_at(n.min(s.len())).0
}

/// The first `n` of `s`, mutably.
fn head_mut<T>(s: &mut [T], n: usize) -> &mut [T] {
    let k = n.min(s.len());
    s.split_at_mut(k).0
}

fn undefined_jet<T: Trig, const D: usize>() -> CurveJet2<D, T> {
    let u = T::undefined();
    CurveJet2 {
        point: Point2::new(u, u),
        d: [Vec2::new(u, u); D],
    }
}

/// The series of `sqrt(a^2 + b^2)` from the series of `a` and `b`, into `out`:
/// `r_0 = sqrt(a_0^2 + b_0^2)`, `r_n = (s_n - sum_{j=1..n-1} r_j r_{n-j}) / (2 r_0)` with
/// `s` the series of `a^2 + b^2`.
fn length<T: Trig>(a: &[T], b: &[T], out: &mut [T]) {
    let l = T::lift;
    let n = out.len().min(a.len()).min(b.len());
    let get = |s: &[T], i: usize| s.get(i).copied().unwrap_or_else(|| l(0.0));
    let r0 = (get(a, 0).sqr() + get(b, 0).sqr()).sqrt();
    let two_r0 = l(2.0) * r0;
    for i in 0..n {
        let value = if i == 0 {
            r0
        } else {
            let mut s = l(0.0);
            for j in 0..=i {
                s = s + get(a, j) * get(a, i - j) + get(b, j) * get(b, i - j);
            }
            for j in 1..i {
                s = s - get(out, j) * get(out, i - j);
            }
            s / two_r0
        };
        if let Some(slot) = out.get_mut(i) {
            *slot = value;
        }
    }
}

/// The parameter series of the direction `(a, b)` on `arcs` into `out` (as long as `a`
/// and `b`): [`Arcs::of_direction`] as a series. A point on one arc takes it; a point
/// within [`NEAR`] of a break takes the arc the curve is on from `side` (the break's exact
/// value when it is exactly on it; the arc's formula continues smoothly an ulp past); an
/// interval box takes every arc it meets, each with the box clipped to it, and joins them.
/// A box holding the centre (a turn's axis, a sphere's pole) is undefined. `lift` takes
/// each arc's value to the branch the caller wants, before arcs are joined.
fn angle<T: Trig>(
    arcs: Arcs,
    a: &[T],
    b: &[T],
    side: Approach,
    lift: &impl Fn(T) -> T,
    out: &mut [T],
) {
    // Each arc's value is lifted before any join: the arcs either side of the period's
    // break hold values a period apart, and their hull would hold the whole period.
    let on_arc = |arcs: Arcs, q: usize, a: &[T], b: &[T], exact: Option<f64>, out: &mut [T]| {
        on_arc(arcs, q, a, b, exact, out);
        if let Some(o) = out.first_mut() {
            *o = lift(*o);
        }
    };
    let ((alo, ahi), (blo, bhi)) = match (a.first(), b.first()) {
        (Some(a0), Some(b0)) => (a0.ends(), b0.ends()),
        _ => return,
    };
    let span = arcs.length() / if arcs.is_turn() { 4.0 } else { 2.0 };
    if alo == ahi && blo == bhi {
        let (a0, b0) = (alo, blo);
        let near = |v: f64, other: f64| v.abs() <= NEAR * other.abs();
        // Within rounding of a break: (the arc for the zero component below zero, its
        // break value), the same above, the zero component's series and its value.
        let on_break = if arcs.is_turn() {
            if a0 == 0.0 && b0 == 0.0 {
                None
            } else if near(b0, a0) {
                if a0 > 0.0 {
                    Some(((3, 4.0 * span), (0, 0.0), b, b0))
                } else {
                    Some(((2, 2.0 * span), (1, 2.0 * span), b, b0))
                }
            } else if near(a0, b0) {
                if b0 > 0.0 {
                    Some(((1, span), (0, span), a, a0))
                } else {
                    Some(((2, 3.0 * span), (3, 3.0 * span), a, a0))
                }
            } else {
                let q = match (a0 > 0.0, b0 > 0.0) {
                    (true, true) => 0,
                    (false, true) => 1,
                    (false, false) => 2,
                    (true, false) => 3,
                };
                return on_arc(arcs, q, a, b, None, out);
            }
        } else if a0 <= 0.0 {
            None
        } else if near(b0, a0) {
            Some(((0, 0.5), (1, 0.5), b, b0))
        } else {
            return on_arc(arcs, usize::from(b0 > 0.0), a, b, None, out);
        };
        let Some((below, above, zero, z0)) = on_break else {
            return out.fill(T::undefined());
        };
        let exact = |q: (usize, f64)| (z0 == 0.0).then_some(q.1);
        let pick = |q: (usize, f64), out: &mut [T]| on_arc(arcs, q.0, a, b, exact(q), out);
        let sign = match leaving_sign(zero, side) {
            Leaving::Sign(s) => Some(s),
            Leaving::Stays => Some(if z0 < 0.0 { -1.0 } else { 1.0 }),
            Leaving::Unknown => None,
        };
        match sign {
            Some(s) if s < 0.0 => pick(below, out),
            Some(_) => pick(above, out),
            None => {
                let mut other = [T::lift(0.0); TERMS];
                let other = head_mut(&mut other, out.len());
                pick(below, other);
                pick(above, out);
                for (o, w) in out.iter_mut().zip(other.iter()) {
                    *o = o.join(*w);
                }
            }
        }
        return;
    }
    if !(alo <= ahi && blo <= bhi) {
        return out.fill(T::undefined());
    }
    let inf = f64::INFINITY;
    let quads: &[Quad] = if arcs.is_turn() {
        &[
            (0, (0.0, inf), (0.0, inf)),
            (1, (-inf, 0.0), (0.0, inf)),
            (2, (-inf, 0.0), (-inf, 0.0)),
            (3, (0.0, inf), (-inf, 0.0)),
        ]
    } else {
        &[(0, (0.0, inf), (-inf, 0.0)), (1, (0.0, inf), (0.0, inf))]
    };
    let centre = alo <= 0.0 && ahi >= 0.0 && blo <= 0.0 && bhi >= 0.0;
    if centre || (!arcs.is_turn() && ahi <= 0.0) {
        return out.fill(T::undefined());
    }
    let n = out.len().min(TERMS);
    let mut met = false;
    for &(q, (amin, amax), (bmin, bmax)) in quads {
        if ahi < amin || alo > amax || bhi < bmin || blo > bmax {
            continue;
        }
        let mut ac = [T::lift(0.0); TERMS];
        let mut bc = [T::lift(0.0); TERMS];
        let (ac, bc) = (head_mut(&mut ac, n), head_mut(&mut bc, n));
        for (c, s) in ac.iter_mut().zip(a) {
            *c = *s;
        }
        for (c, s) in bc.iter_mut().zip(b) {
            *c = *s;
        }
        if let (Some(a0), Some(b0)) = (ac.first_mut(), bc.first_mut()) {
            *a0 = a0.clip(amin, amax);
            *b0 = b0.clip(bmin, bmax);
        }
        let mut here = [T::lift(0.0); TERMS];
        let here = head_mut(&mut here, n);
        on_arc(arcs, q, ac, bc, None, here);
        if met {
            for (o, w) in out.iter_mut().zip(here.iter()) {
                *o = o.join(*w);
            }
        } else {
            for (o, w) in out.iter_mut().zip(here.iter()) {
                *o = *w;
            }
            met = true;
        }
    }
    if !met {
        out.fill(T::undefined());
    }
}

/// The arc formula `q` on the direction series `(a, b)`, the value replaced by `exact`
/// when the point is on a break.
fn on_arc<T: Trig>(arcs: Arcs, q: usize, a: &[T], b: &[T], exact: Option<f64>, out: &mut [T]) {
    let mut r = [T::lift(0.0); TERMS];
    let n = out.len().min(TERMS);
    let r = head_mut(&mut r, n);
    length(a, b, r);
    arcs.direction_series(q, a, b, r, out);
    if let (Some(e), Some(o)) = (exact, out.first_mut()) {
        *o = T::lift(e);
    }
}

/// Where a series with `s_0` (about) zero goes just past its point.
enum Leaving {
    /// To this sign.
    Sign(f64),
    /// Nowhere: every later coefficient is zero.
    Stays,
    /// Not known: a coefficient's interval holds zero.
    Unknown,
}

/// Where the series `s` goes just past its point on `side`: its first nonzero later
/// coefficient's sign, times `(-1)^k` from below.
fn leaving_sign<T: Trig>(s: &[T], side: Approach) -> Leaving {
    for (k, c) in s.iter().enumerate().skip(1) {
        let (lo, hi) = c.ends();
        let sign = if lo > 0.0 {
            1.0
        } else if hi < 0.0 {
            -1.0
        } else if lo == 0.0 && hi == 0.0 {
            continue;
        } else {
            return Leaving::Unknown;
        };
        let flip = side == Approach::Below && k % 2 == 1;
        return Leaving::Sign(if flip { -sign } else { sign });
    }
    Leaving::Stays
}
#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::angle_line::{AngleAxis, AngleLine2, AngleWave2};
    use crate::conic::{Circle, Ellipse};
    use crate::curve::Curve2;
    use crate::line::Line;
    use scx_num::interval::Ivl;

    const TAU: f64 = 2.0 * PI;

    /// An oblique frame, so no coordinate is trivially zero.
    fn frame() -> Frame {
        Frame::new(
            Point3::new(0.7, -1.3, 2.1),
            Vec3::new(0.2, -0.3, 1.0),
            Vec3::new(1.0, 0.4, 0.1),
        )
        .unwrap()
    }

    /// `o + a X + b Y + c Z` in `f`.
    fn at(f: &Frame, a: f64, b: f64, c: f64) -> Point3 {
        f.origin() + *f.x() * a + *f.y() * b + *f.z() * c
    }

    fn dir(f: &Frame, a: f64, b: f64, c: f64) -> Vec3 {
        *f.x() * a + *f.y() * b + *f.z() * c
    }

    fn mm(x: f64) -> Length {
        Length::from_mm(x)
    }

    fn chart(kind: ChartKind, sense: Sense) -> Chart {
        Chart::new(frame(), sense, kind).unwrap()
    }

    fn whole_turn(curve: Curve, chart: Chart) -> OnChart {
        let start = chart.inverse(curve.point(0.0));
        OnChart::new(
            curve,
            chart,
            Extent1d::new(0.0, TAU).unwrap(),
            [start, start],
        )
        .unwrap()
    }

    /// The circle `centre + r (cos t x + sin t y)` with `x`, `y` given in `f`.
    fn circle(centre: Point3, x: Vec3, z: Vec3, r: f64) -> Curve {
        let cf = Frame::new(centre, z, x).unwrap();
        Curve::Circle(Circle::new(cf, mm(r), Sense::Forward).unwrap())
    }

    /// Pairs of an `OnChart` and the closed-form angle kind it must equal: circles about
    /// the axis of a cylinder, a cone, a sphere and a torus (angle lines), a torus's tube
    /// circle (an angle line in `v`), and a plane's oblique section of a cylinder (an angle
    /// wave), in both senses where the sense matters.
    pub(crate) fn pairs() -> Vec<(OnChart, Curve2)> {
        let f = frame();
        let (x, y, z) = (*f.x(), *f.y(), *f.z());
        let mut out = Vec::new();
        let lift = |c: Curve, ch: Chart, turns: [f64; 2]| {
            let [pu, pv] = ch.periods();
            let near0 = |w: f64, p: Option<f64>| match p {
                Some(p) if w > 0.75 * p => w - p,
                _ => w,
            };
            let start = ch.inverse(c.point(0.0));
            let start = Point2::new(near0(start.x, pu), near0(start.y, pv));
            let end = Point2::new(
                start.x + pu.map_or(0.0, |p| turns[0] * p),
                start.y + pv.map_or(0.0, |p| turns[1] * p),
            );
            OnChart::new(c, ch, Extent1d::new(0.0, TAU).unwrap(), [start, end]).unwrap()
        };
        for sense in [Sense::Forward, Sense::Reversed] {
            let s = sense.sign();
            // Cylinder radius 2: the circle at height 0.5, and the section z = 0.5 + 0.75 x.
            let cyl = chart(ChartKind::Cylinder { radius: mm(2.0) }, sense);
            let arcs = Arcs::turn(2.0 * PI * 2.0);
            let c = circle(at(&f, 0.0, 0.0, 0.5), x, z, 2.0);
            let want =
                AngleLine2::new(arcs, AngleAxis::U, 0.0, Sense::Forward, 0.5 * s, 0.0).unwrap();
            out.push((lift(c, cyl, [1.0, 0.0]), want.into()));
            let (h, k, r) = (0.5, 0.75, 2.0);
            let n = (1.0f64 + k * k).sqrt();
            let ef = Frame::new(
                at(&f, 0.0, 0.0, h),
                dir(&f, -k, 0.0, 1.0),
                dir(&f, 1.0, 0.0, k),
            )
            .unwrap();
            let e = Curve::Ellipse(Ellipse::new(ef, mm(r * n), mm(r), Sense::Forward).unwrap());
            let want = AngleWave2::new(arcs, 0.0, Sense::Forward, h * s, k * r * s, 0.0).unwrap();
            out.push((lift(e, cyl, [1.0, 0.0]), want.into()));
            // Cone r + k v with r = 1, k = 0.5: the circle at height 0.8 has radius 1.4.
            let cone = chart(ChartKind::Cone, sense);
            let c = circle(at(&f, 0.0, 0.0, 0.8 * s), x, z, 1.4);
            let want = AngleLine2::new(TURN, AngleAxis::U, 0.0, Sense::Forward, 0.8, 0.0).unwrap();
            out.push((lift(c, cone, [1.0, 0.0]), want.into()));
            // Sphere radius 2: the latitude at height 1.2 (radius 1.6).
            let sph = chart(ChartKind::Sphere, sense);
            let c = circle(at(&f, 0.0, 0.0, 1.2), x, z, 1.6);
            let v0 = HALF.parameter(fmath::atan2(1.2 * s, 1.6));
            let want = AngleLine2::new(TURN, AngleAxis::U, 0.0, Sense::Forward, v0, 0.0).unwrap();
            out.push((lift(c, sph, [1.0, 0.0]), want.into()));
            // Torus R = 3, a = 1: the outer circle at tube angle 0.4, and the tube circles at
            // azimuths 0.3 and 0 (the second on u's break).
            let tor = chart(ChartKind::Torus { major: mm(3.0) }, sense);
            let w: f64 = 0.4;
            let c = circle(at(&f, 0.0, 0.0, fmath::sin(w)), x, z, 3.0 + fmath::cos(w));
            let v0 = TURN.parameter(fmath::atan2(fmath::sin(w) * s, fmath::cos(w)));
            let want = AngleLine2::new(TURN, AngleAxis::U, 0.0, Sense::Forward, v0, 0.0).unwrap();
            out.push((lift(c, tor, [1.0, 0.0]), want.into()));
            for alpha in [0.3f64, 0.0] {
                let (ca, sa) = (fmath::cos(alpha), fmath::sin(alpha));
                let radial = x * ca + y * sa;
                let cf =
                    Frame::new(f.origin() + radial * 3.0, radial.cross(z * s), radial).unwrap();
                let c = Curve::Circle(Circle::new(cf, mm(1.0), Sense::Forward).unwrap());
                let u0 = TURN.parameter(alpha);
                let want =
                    AngleLine2::new(TURN, AngleAxis::V, 0.0, Sense::Forward, u0, 0.0).unwrap();
                out.push((lift(c, tor, [0.0, 1.0]), want.into()));
            }
        }
        out
    }

    fn close(a: f64, b: f64, tol: f64) -> bool {
        (a - b).abs() <= tol * (1.0 + a.abs().max(b.abs()))
    }

    #[test]
    fn an_on_chart_is_the_angle_kind_it_should_be() {
        let mut n = 0;
        for (ci, (oc, want)) in pairs().into_iter().enumerate() {
            for i in -40..=80 {
                let t = 0.1 * f64::from(i) + 0.013;
                for side in [Approach::Above, Approach::Below] {
                    let got = oc.jet::<4>(t, side);
                    let exp = want.jet::<4>(t, side);
                    let g = core::iter::once(got.point.to_array())
                        .chain(got.d.iter().map(|d| d.to_array()));
                    let e = core::iter::once(exp.point.to_array())
                        .chain(exp.d.iter().map(|d| d.to_array()));
                    for (k, (g, e)) in g.zip(e).enumerate() {
                        for (a, b) in g.iter().zip(&e) {
                            assert!(
                                close(*a, *b, 1e-11),
                                "case {ci} t {t} {side:?} order {k}: {a} vs {b}"
                            );
                        }
                    }
                    n += 1;
                }
            }
        }
        assert!(n > 2000, "{n}");
    }

    #[test]
    fn a_break_takes_the_side_asked_for() {
        // The cylinder's circle starts on u's break at t = 0, and the torus's tube circle
        // at azimuth 0 sits on one: each side's second derivative is the angle kind's.
        for (ci, (oc, want)) in pairs().into_iter().enumerate() {
            for t in [0.0, PI * 0.5, PI, TAU] {
                for side in [Approach::Above, Approach::Below] {
                    let (g, e) = (oc.jet::<3>(t, side), want.jet::<3>(t, side));
                    for (a, b) in g.d.iter().zip(&e.d) {
                        assert!(
                            close(a.x, b.x, 1e-11) && close(a.y, b.y, 1e-11),
                            "case {ci} {t} {side:?}: {g:?} {e:?}"
                        );
                    }
                    assert!(
                        close(g.point.x, e.point.x, 1e-14) && close(g.point.y, e.point.y, 1e-14)
                    );
                }
            }
        }
    }

    /// `(phi, phi', phi'')` of `phi = atan2(b, a)` from the jets of `a` and `b`.
    fn atan2_jet(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
        let s = a[0] * a[0] + b[0] * b[0];
        let c = a[0] * b[1] - b[0] * a[1];
        let dot = a[0] * a[1] + b[0] * b[1];
        let d1 = c / s;
        let d2 = (a[0] * b[2] - b[0] * a[2]) / s - 2.0 * c * dot / (s * s);
        [fmath::atan2(b[0], a[0]), d1, d2]
    }

    /// `(u, u', u'')` of `arcs.parameter(phi(t))` by the chain rule.
    fn through(arcs: Arcs, phi: [f64; 3], side: Approach) -> [f64; 3] {
        let (p, [p1, p2]) = arcs.parameter_jet::<f64, 2>(phi[0], side);
        [p, p1 * phi[1], p2 * phi[1] * phi[1] + p1 * phi[2]]
    }

    /// Off-axis circles (two spheres' intersection, a plane's tilted section of a cone and
    /// a torus) against `atan2` and the chain rule: values to a period, first and second
    /// derivatives, and the third against a difference of the second.
    #[test]
    fn off_axis_curves_match_the_chain_rule() {
        let f = frame();
        let (x, y, z) = (*f.x(), *f.y(), *f.z());
        let tilt = (z * 0.8 + x * 0.6).normalized().unwrap();
        let cases: Vec<(Curve, ChartKind)> = vec![
            // Sphere radius 2 cut by the plane at distance 0.6 along `tilt`.
            (
                circle(f.origin() + tilt * 0.6, y, *tilt, (4.0f64 - 0.36).sqrt()),
                ChartKind::Sphere,
            ),
            // A cylinder's section: on the cylinder of radius 2 about z, z = 0.3 + 0.5 x + 0.2 y.
            (
                Curve::Ellipse(
                    Ellipse::new(
                        Frame::new(
                            at(&f, 0.0, 0.0, 0.3),
                            dir(&f, -0.5, -0.2, 1.0),
                            dir(&f, 1.0, 0.0, 0.5),
                        )
                        .unwrap(),
                        mm(2.0 * (1.25f64).sqrt()),
                        mm(2.0),
                        Sense::Forward,
                    )
                    .unwrap(),
                ),
                ChartKind::Cylinder { radius: mm(2.0) },
            ),
            // A line on a plane.
            (
                Curve::Line(Line::new(at(&f, 0.4, -0.2, 0.0), dir(&f, 0.3, 0.9, 0.0)).unwrap()),
                ChartKind::Plane,
            ),
        ];
        let mut n = 0;
        for (curve, kind) in cases {
            for sense in [Sense::Forward, Sense::Reversed] {
                let ch = chart(kind, sense);
                let oc = whole_turn(curve.clone(), ch);
                let zs = *f.z() * sense.sign();
                for i in 0..120 {
                    let t = 0.05 * f64::from(i) + 0.007;
                    let c = curve.jet::<2>(t, Approach::Above);
                    let d0 = c.point - f.origin();
                    let loc = |a: Vec3| [d0.dot(a), c.d[0].dot(a), c.d[1].dot(a)];
                    let (lx, ly, lz) = (loc(x), loc(y), loc(zs));
                    let side = Approach::Above;
                    let (u, v) = match kind {
                        ChartKind::Plane => (lx, ly),
                        ChartKind::Cylinder { radius } => (
                            through(Arcs::turn(TAU * radius.mm()), atan2_jet(lx, ly), side),
                            lz,
                        ),
                        _ => {
                            let r = (lx[0] * lx[0] + ly[0] * ly[0]).sqrt();
                            let r1 = (lx[0] * lx[1] + ly[0] * ly[1]) / r;
                            let r2 =
                                (lx[1] * lx[1] + ly[1] * ly[1] + lx[0] * lx[2] + ly[0] * ly[2]) / r
                                    - r1 * r1 / r;
                            (
                                through(TURN, atan2_jet(lx, ly), side),
                                through(HALF, atan2_jet([r, r1, r2], lz), side),
                            )
                        }
                    };
                    let got = oc.jet::<3>(t, side);
                    let [pu, pv] = ch.periods();
                    let wrapped = |g: f64, e: f64, p: Option<f64>| match p {
                        Some(p) => g - e - ((g - e) / p).round() * p,
                        None => g - e,
                    };
                    assert!(wrapped(got.point.x, u[0], pu).abs() < 1e-12, "{kind:?} {t}");
                    assert!(wrapped(got.point.y, v[0], pv).abs() < 1e-12, "{kind:?} {t}");
                    for k in 0..2 {
                        let (gu, gv) = (got.d[k].x, got.d[k].y);
                        assert!(
                            close(gu, u[k + 1], 1e-10) && close(gv, v[k + 1], 1e-10),
                            "{kind:?} {t} {k}: {gu} {gv} vs {u:?} {v:?}"
                        );
                    }
                    let h = 1e-5;
                    let (a, b) = (oc.jet::<2>(t + h, side), oc.jet::<2>(t - h, side));
                    let du = (a.d[1].x - b.d[1].x) / (2.0 * h);
                    let dv = (a.d[1].y - b.d[1].y) / (2.0 * h);
                    assert!(
                        close(got.d[2].x, du, 1e-5) && close(got.d[2].y, dv, 1e-5),
                        "{kind:?} {t}"
                    );
                    n += 1;
                }
            }
        }
        assert!(n >= 600);
    }

    #[test]
    fn the_lift_follows_the_span_past_a_turn() {
        // A cylinder's circle over two turns: u runs on past the period.
        let f = frame();
        let ch = chart(ChartKind::Cylinder { radius: mm(1.5) }, Sense::Forward);
        let c = circle(at(&f, 0.0, 0.0, 0.0), *f.x(), *f.z(), 1.5);
        let p = TAU * 1.5;
        let oc = OnChart::new(
            c,
            ch,
            Extent1d::new(0.0, 2.0 * TAU).unwrap(),
            [Point2::new(0.0, 0.0), Point2::new(2.0 * p, 0.0)],
        )
        .unwrap();
        let arcs = Arcs::turn(p);
        for i in 0..=100 {
            let t = 2.0 * TAU * f64::from(i) / 100.0;
            let want = arcs.parameter(t);
            assert!(
                (oc.point(t).x - want).abs() < 1e-12 * p,
                "{t}: {:?} {want}",
                oc.point(t)
            );
        }
        // Shifted by a turn, and reversed.
        let s = oc.shifted([-1.0, 5.0]).unwrap();
        assert!((s.point(1.0).x - (oc.point(1.0).x - p)).abs() < 1e-12);
        assert_eq!(s.point(1.0).y.to_bits(), oc.point(1.0).y.to_bits());
        let r = oc.reversed();
        for t in [0.0, 1.0, 7.5, 2.0 * TAU] {
            let (a, b) = (r.point(-t), oc.point(t));
            assert!(
                (a.x - b.x).abs() < 1e-12 && (a.y - b.y).abs() < 1e-12,
                "{t}"
            );
        }
    }

    #[test]
    fn the_value_is_the_chart_inverse() {
        // Off the lift, the value is the surfaces' own inverse (of_direction) to the bit.
        let (mut exact, mut n) = (0, 0);
        for (oc, _) in pairs() {
            for i in 0..50 {
                let t = 0.12 * f64::from(i) + 0.05;
                let got = oc.point(t);
                let p = oc.chart().inverse(oc.curve().point(t));
                let [pu, pv] = oc.chart().periods();
                let m = |g: f64, e: f64, per: Option<f64>| {
                    per.map_or(g, |q| g - ((g - e) / q).round() * q)
                };
                // Off a break `u` is `of_direction`'s own arithmetic, to the bit; within an
                // ulp of one the side's arc may differ from the inverse's. The sphere's `v`
                // takes `rho` as a root of squares where the inverse takes `hypot`.
                let gu = m(got.x, p.x, pu);
                let tol = 4.0 * f64::EPSILON * pu.unwrap_or(1.0);
                assert!(
                    gu.to_bits() == p.x.to_bits() || (gu - p.x).abs() <= tol,
                    "{t}"
                );
                exact += usize::from(gu.to_bits() == p.x.to_bits());
                n += 1;
                assert!(
                    (m(got.y, p.y, pv) - p.y).abs() <= 4.0 * f64::EPSILON * (1.0 + p.y.abs()),
                    "{t}"
                );
            }
        }
        assert!(exact * 10 > n * 9, "{exact} of {n}");
    }

    /// Every box's jet holds the values at its points, and a small box's is small.
    #[test]
    fn intervals_enclose_the_values() {
        for (oc, _) in pairs() {
            for i in -10..60 {
                let lo = 0.11 * f64::from(i);
                for w in [0.0, 1e-9, 1e-3, 0.3, 2.0] {
                    let iv = oc.jet_over::<Ivl, 2>(Ivl::new(lo, lo + w), Approach::Above);
                    // Narrow too, across the period's break as anywhere: each arc is lifted
                    // before the arcs are joined.
                    if w <= 1e-3 {
                        let wide = |i: Ivl| i.hi - i.lo;
                        assert!(
                            wide(iv.point.x) < 0.05 && wide(iv.point.y) < 0.05,
                            "{oc:?} [{lo}, {}]: {iv:?}",
                            lo + w
                        );
                    }
                    for j in 0..=8 {
                        let t = lo + w * f64::from(j) / 8.0;
                        for side in [Approach::Above, Approach::Below] {
                            let p = oc.jet::<2>(t, side);
                            let holds = |i: Ivl, x: f64| i.lo <= x && x <= i.hi;
                            assert!(
                                holds(iv.point.x, p.point.x) && holds(iv.point.y, p.point.y),
                                "{oc:?} [{lo}, {}] {t}: {iv:?} {p:?}",
                                lo + w
                            );
                            for (a, b) in iv.d.iter().zip(&p.d) {
                                assert!(
                                    holds(a.x, b.x) && holds(a.y, b.y),
                                    "{oc:?} [{lo}, {}] {t}: {iv:?} {p:?}",
                                    lo + w
                                );
                            }
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn the_axis_and_a_pole_are_undefined() {
        let f = frame();
        // A line through the cylinder's axis (not on the cylinder, but the chart is total).
        let line = Curve::Line(Line::new(at(&f, -1.0, 0.0, 0.0), dir(&f, 1.0, 0.0, 0.0)).unwrap());
        let ch = chart(ChartKind::Cylinder { radius: mm(1.0) }, Sense::Forward);
        let oc = OnChart::new(
            line,
            ch,
            Extent1d::new(0.0, 2.0).unwrap(),
            [Point2::new(0.5 * TAU, 0.0), Point2::new(0.0, 0.0)],
        )
        .unwrap();
        assert_eq!(oc.point_over(Ivl::new(0.5, 1.5)).x, Ivl::whole());
        // A meridian through the sphere's north pole.
        let m = circle(f.origin(), *f.x(), *f.y() * -1.0, 1.0);
        let ch = chart(ChartKind::Sphere, Sense::Forward);
        let oc = whole_turn(m, ch);
        let box_ = Ivl::new(PI * 0.5 - 1e-3, PI * 0.5 + 1e-3);
        assert_eq!(oc.point_over(box_).x, Ivl::whole());
        let v = oc.point_over(box_).y;
        assert!(v.lo <= 1.0 && 1.0 <= v.hi && v.hi - v.lo < 1e-2, "{v:?}");
        assert!(!oc.point(PI * 0.25).x.is_nan());
        // A jet past the series' length.
        assert!(oc.jet::<16>(0.3, Approach::Above).point.x.is_nan());
    }

    #[test]
    fn bad_data_is_refused() {
        let f = frame();
        let cyl = |r: f64| Chart::new(f, Sense::Forward, ChartKind::Cylinder { radius: mm(r) });
        assert!(matches!(
            cyl(f64::NAN),
            Err(CurveError::NotFinite(Part::Radius))
        ));
        assert!(matches!(
            cyl(0.0),
            Err(CurveError::Degenerate(Degenerate::Radius { .. }))
        ));
        assert!(
            Chart::new(
                f,
                Sense::Forward,
                ChartKind::Torus {
                    major: mm(f64::INFINITY)
                }
            )
            .is_err()
        );
        let plane = Chart::new(f, Sense::Reversed, ChartKind::Plane).unwrap();
        assert_eq!(plane.sense(), Sense::Forward);
        let c = circle(f.origin(), *f.x(), *f.z(), 1.0);
        let ok = Point2::new(0.0, 0.0);
        assert!(OnChart::new(c.clone(), plane, Extent1d::new(1.0, 1.0).unwrap(), [ok; 2]).is_err());
        assert!(
            OnChart::new(
                c,
                plane,
                Extent1d::new(0.0, 1.0).unwrap(),
                [ok, Point2::new(f64::NAN, 0.0)]
            )
            .is_err()
        );
    }
}
