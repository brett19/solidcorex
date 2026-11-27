// Derived from solidcore `solidcore/crates/solidcore/src/curve/line.rs`,
// `solidcore/crates/solidcore/src/curve/ellipse/p2.rs` and
// `solidcore/crates/solidcore/src/geom/geom_utility/p1.rs` at commit
// 03e16c9ee766de5b9b1ced845e5149d62ff493b8.
// Upstream record (verbatim from those files): Upstream: `source/SMLib/src/SmLine.cpp`.
// Upstream: `source/SMLib/src/SmEllipse.cpp`, `source/SMLib/src/SmCircle.cpp`. Upstream:
// `source/SMLib/src/SmGeomUtility.cpp`.
// Changes: one closest-point routine per kind and one inversion against a domain, as values
// (63 X8, D7): `Closest` and 57 F6's `Inversion { Inside, Outside }` of a `Foot`, with no
// `Failed`, status number or out-parameter. The line keeps `line_closest_point`'s projection
// `t = (p - o).d / d.d`; a degenerate line cannot be built (CU1), so its `t = 0` fallback is
// gone. The circle keeps `drop_point_fast`'s angle and its axis lesson (no unique angle
// there), now exact: only a query on the axis itself has every point nearest, where the pin
// used a band of `SM_EFF_ZERO` scaled by the model and returned the domain's two ends. The
// ellipse's `step_inversion` (`atan2(rx/ry dy, dx)`) is the foot only of points on the curve;
// here the true foot is solved in the first quadrant (one stationary point there: the
// evolute's double roots lie in the other quadrants), from `scx-algo`'s quartic in
// `tan(t/2)` polished by a bracketed Newton on the angle. The axis queries inside the evolute
// (two symmetric feet) and the centre are closed forms. Angles are radians (63 X11); the
// seam and the domain's ends are read from `at` against the domain, not classified.

//! The nearest point of a curve to a point, and its inversion against a domain (63 §3.2).
//!
//! - [`Line::closest`] is one [`Foot`]: the projection onto the line.
//! - The conics' `closest` is a [`Closest`]: one foot, two equally near (a query on an
//!   ellipse's axis inside its evolute, or at its centre), or every point (a circle's axis).
//! - `invert(p, domain)` picks one foot and says whether it lies [`Inversion::Inside`] the
//!   domain, or [`Inversion::Outside`]; `None` is the kind's own domain. The foot is the
//!   kind's nearest point either way: an outside foot is not moved to the domain's end.
//! - [`Curve::invert`] and [`Curve2::invert`] dispatch; a NURBS curve's inversion is
//!   [`Unsupported`](crate::CurveError::Unsupported) until its solve is built.
//!
//! Every analytic arm is a closed form or a bounded polish (at most [`POLISH_STEPS`]
//! Newton or bisection steps), so it takes no budget and cannot fail. A non-finite query
//! gives a foot with NaN parts, as evaluation does.

use scx_algo::polynomial::solve_quartic;
use scx_num::extent1d::Extent1d;
use scx_num::fmath::{acos, asin, atan, atan2, sin_cos};
pub use scx_num::invert::{Foot, Inversion};
use scx_num::math::{EFF_ZERO, HALF_PI, PI, TWO_PI};
use scx_num::vector2d::{Point2, Vec2};
use scx_num::vector3d::{Point3, Vec3};

use crate::conic::{Circle, Circle2, Ellipse, Ellipse2};
use crate::curve::{Curve, Curve2};
use crate::error::{CurveError, Unsupported};
use crate::line::{Line, Line2};

/// The most steps the ellipse's polish takes. Bisection alone closes the quarter turn's
/// bracket to below an ulp of the angle in under 60.
pub const POLISH_STEPS: u32 = 64;

/// Every nearest point of a curve to a query.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Closest<A = f64, Q = Point3> {
    /// One nearest point.
    One(Foot<A, Q>),
    /// Two, equally near, in increasing `at` within the kind's period.
    Two([Foot<A, Q>; 2]),
    /// Every point of the curve is at `distance`: a query on a circle's axis, or at the
    /// centre of an ellipse with equal semi-axes.
    Every {
        /// The common distance.
        distance: f64,
    },
}

// ---------------------------------------------------------------- the kinds

impl Line {
    /// The nearest point to `p`: the projection `t = (p - o).d / d.d`.
    #[must_use]
    pub fn closest(&self, p: Point3) -> Foot {
        let d = self.direction();
        let at = (p - self.origin()).dot(d) / d.dot(d);
        foot3(p, at, self.point(at))
    }

    /// The nearest point to `p`, inside `domain` or not (`None`: every `t`).
    #[must_use]
    pub fn invert(&self, p: Point3, domain: Option<&Extent1d>) -> Inversion {
        place(self.closest(p), domain)
    }
}

impl Line2 {
    /// The nearest point to `p`: the projection `t = (p - o).d / d.d`.
    #[must_use]
    pub fn closest(&self, p: Point2) -> Foot<f64, Point2> {
        let d = self.direction();
        let at = (p - self.origin()).dot(d) / d.dot(d);
        foot2(p, at, self.point(at))
    }

    /// The nearest point to `p`, inside `domain` or not (`None`: every `t`).
    #[must_use]
    pub fn invert(&self, p: Point2, domain: Option<&Extent1d>) -> Inversion<f64, Point2> {
        place(self.closest(p), domain)
    }
}

impl Circle {
    /// The nearest points to `p`: one, at the angle of `p`'s projection into the plane, or
    /// every point for `p` on the axis.
    #[must_use]
    pub fn closest(&self, p: Point3) -> Closest {
        let (u, v) = local3(
            self.frame().origin(),
            self.frame().x().as_vec(),
            self.y3(),
            p,
        );
        conic_feet(circle_angles(u, v), |t| foot3(p, t, self.point(t)))
    }

    /// The nearest point to `p`, inside `domain` (an angle range, wrapped by the period)
    /// or not (`None`: the whole turn). On the axis the foot is at the domain's start.
    #[must_use]
    pub fn invert(&self, p: Point3, domain: Option<&Extent1d>) -> Inversion {
        invert_periodic(self.closest(p), domain, |t| foot3(p, t, self.point(t)))
    }

    fn y3(&self) -> Vec3 {
        self.frame().y().as_vec() * self.sense().sign()
    }
}

impl Circle2 {
    /// The nearest points to `p`: one, at `p`'s angle about the centre, or every point for
    /// `p` at the centre.
    #[must_use]
    pub fn closest(&self, p: Point2) -> Closest<f64, Point2> {
        let (u, v) = local2(
            self.frame().origin(),
            self.frame().x().as_vec(),
            self.y2(),
            p,
        );
        conic_feet(circle_angles(u, v), |t| foot2(p, t, self.point(t)))
    }

    /// The nearest point to `p`, inside `domain` or not; see [`Circle::invert`].
    #[must_use]
    pub fn invert(&self, p: Point2, domain: Option<&Extent1d>) -> Inversion<f64, Point2> {
        invert_periodic(self.closest(p), domain, |t| foot2(p, t, self.point(t)))
    }

    fn y2(&self) -> Vec2 {
        self.frame().y().as_vec() * self.sense().sign()
    }
}

impl Ellipse {
    /// The nearest points to `p`: one, or two symmetric ones for `p` at the centre or on
    /// the major axis inside the evolute (`|u| < (a^2 - b^2) / a`), or every point at the
    /// centre of an ellipse with equal semi-axes. Measured in the plane: `p`'s offset along
    /// the frame's z changes the distance, not the feet.
    #[must_use]
    pub fn closest(&self, p: Point3) -> Closest {
        let (u, v) = local3(
            self.frame().origin(),
            self.frame().x().as_vec(),
            self.y3(),
            p,
        );
        let angles = ellipse_angles(self.semi_x().mm(), self.semi_y().mm(), u, v);
        conic_feet(angles, |t| foot3(p, t, self.point(t)))
    }

    /// The nearest point to `p`, inside `domain` or not; see [`Circle::invert`]. Of two
    /// equally near feet, the first inside the domain, else the first.
    #[must_use]
    pub fn invert(&self, p: Point3, domain: Option<&Extent1d>) -> Inversion {
        invert_periodic(self.closest(p), domain, |t| foot3(p, t, self.point(t)))
    }

    fn y3(&self) -> Vec3 {
        self.frame().y().as_vec() * self.sense().sign()
    }
}

impl Ellipse2 {
    /// The nearest points to `p`; see [`Ellipse::closest`].
    #[must_use]
    pub fn closest(&self, p: Point2) -> Closest<f64, Point2> {
        let (u, v) = local2(
            self.frame().origin(),
            self.frame().x().as_vec(),
            self.y2(),
            p,
        );
        let angles = ellipse_angles(self.semi_x().mm(), self.semi_y().mm(), u, v);
        conic_feet(angles, |t| foot2(p, t, self.point(t)))
    }

    /// The nearest point to `p`, inside `domain` or not; see [`Ellipse::invert`].
    #[must_use]
    pub fn invert(&self, p: Point2, domain: Option<&Extent1d>) -> Inversion<f64, Point2> {
        invert_periodic(self.closest(p), domain, |t| foot2(p, t, self.point(t)))
    }

    fn y2(&self) -> Vec2 {
        self.frame().y().as_vec() * self.sense().sign()
    }
}

// ---------------------------------------------------------------- the enums

impl Curve {
    /// The nearest points to `p`; see each kind's `closest`. A line's foot is
    /// [`Closest::One`].
    ///
    /// # Errors
    ///
    /// [`Unsupported::NurbsInversion`] for a NURBS curve.
    #[inline]
    pub fn closest(&self, p: Point3) -> Result<Closest, CurveError> {
        match self {
            Self::Line(c) => Ok(Closest::One(c.closest(p))),
            Self::Circle(c) => Ok(c.closest(p)),
            Self::Ellipse(c) => Ok(c.closest(p)),
            Self::Nurbs(_) => Err(CurveError::Unsupported(Unsupported::NurbsInversion)),
        }
    }

    /// The nearest point to `p`, inside `domain` or not (`None`: the kind's own domain);
    /// see each kind's `invert`.
    ///
    /// # Errors
    ///
    /// [`Unsupported::NurbsInversion`] for a NURBS curve.
    #[inline]
    pub fn invert(&self, p: Point3, domain: Option<&Extent1d>) -> Result<Inversion, CurveError> {
        match self {
            Self::Line(c) => Ok(c.invert(p, domain)),
            Self::Circle(c) => Ok(c.invert(p, domain)),
            Self::Ellipse(c) => Ok(c.invert(p, domain)),
            Self::Nurbs(_) => Err(CurveError::Unsupported(Unsupported::NurbsInversion)),
        }
    }
}

impl Curve2 {
    /// The nearest points to `p`; see [`Curve::closest`].
    ///
    /// # Errors
    ///
    /// [`Unsupported::NurbsInversion`] for a NURBS curve.
    #[inline]
    pub fn closest(&self, p: Point2) -> Result<Closest<f64, Point2>, CurveError> {
        match self {
            Self::Line(c) => Ok(Closest::One(c.closest(p))),
            Self::Circle(c) => Ok(c.closest(p)),
            Self::Ellipse(c) => Ok(c.closest(p)),
            Self::Nurbs(_) => Err(CurveError::Unsupported(Unsupported::NurbsInversion)),
        }
    }

    /// The nearest point to `p`, inside `domain` or not; see [`Curve::invert`].
    ///
    /// # Errors
    ///
    /// [`Unsupported::NurbsInversion`] for a NURBS curve.
    #[inline]
    pub fn invert(
        &self,
        p: Point2,
        domain: Option<&Extent1d>,
    ) -> Result<Inversion<f64, Point2>, CurveError> {
        match self {
            Self::Line(c) => Ok(c.invert(p, domain)),
            Self::Circle(c) => Ok(c.invert(p, domain)),
            Self::Ellipse(c) => Ok(c.invert(p, domain)),
            Self::Nurbs(_) => Err(CurveError::Unsupported(Unsupported::NurbsInversion)),
        }
    }
}

// ---------------------------------------------------------------- shared

fn foot3(p: Point3, at: f64, point: Point3) -> Foot {
    Foot {
        at,
        point,
        distance: (p - point).length(),
    }
}

fn foot2(p: Point2, at: f64, point: Point2) -> Foot<f64, Point2> {
    Foot {
        at,
        point,
        distance: (p - point).length(),
    }
}

/// `p`'s coordinates along the conic's x and signed y (the direction its angle turns).
fn local3(o: Point3, x: Vec3, y: Vec3, p: Point3) -> (f64, f64) {
    let d = p - o;
    (d.dot(x), d.dot(y))
}

fn local2(o: Point2, x: Vec2, y: Vec2, p: Point2) -> (f64, f64) {
    let d = p - o;
    (d.dot(x), d.dot(y))
}

/// A conic's nearest angles, each in `[0, 2 pi)`.
#[derive(Debug, Clone, Copy, PartialEq)]
enum Angles {
    One(f64),
    Two(f64, f64),
    Every,
}

/// The feet at the angles (`foot` builds one at a parameter); [`Angles::Every`]'s distance
/// is the foot's at angle 0.
fn conic_feet<Q>(angles: Angles, foot: impl Fn(f64) -> Foot<f64, Q>) -> Closest<f64, Q> {
    match angles {
        Angles::One(t) => Closest::One(foot(t)),
        Angles::Two(s, t) => Closest::Two([foot(s), foot(t)]),
        Angles::Every => Closest::Every {
            distance: foot(0.0).distance,
        },
    }
}

/// An angle from `atan2` (in `[-pi, pi]`) into `[0, 2 pi)`; `-0` becomes `0`.
fn turn(t: f64) -> f64 {
    if t < 0.0 {
        let w = t + TWO_PI;
        if w < TWO_PI { w } else { 0.0 }
    } else {
        t + 0.0
    }
}

/// The circle's angle of `(u, v)`; every angle only at the centre itself (zero is zero).
fn circle_angles(u: f64, v: f64) -> Angles {
    if u == 0.0 && v == 0.0 {
        Angles::Every
    } else {
        Angles::One(turn(atan2(v, u)))
    }
}

/// The ellipse `(a cos t, b sin t)`'s nearest angles to `(u, v)`, `a, b > 0`.
fn ellipse_angles(a: f64, b: f64, u: f64, v: f64) -> Angles {
    if !(u.is_finite() && v.is_finite()) {
        return Angles::One(f64::NAN);
    }
    // Scale by a power of two (exact) so the larger semi-axis is in [1, 2).
    let e = 1.0 / pow2_floor(a.max(b));
    let (a, b, uu, vv) = (a * e, b * e, (u * e).abs(), (v * e).abs());
    if uu == 0.0 && vv == 0.0 {
        return if a > b {
            Angles::Two(HALF_PI, 3.0 * HALF_PI)
        } else if a < b {
            Angles::Two(0.0, PI)
        } else {
            Angles::Every
        };
    }
    // The quadrant of (u, v) maps a first-quadrant angle `s` to the answer.
    let map = |s: f64| -> f64 {
        match (u < 0.0, v < 0.0) {
            (false, false) => s + 0.0,
            (true, false) => PI - s,
            (true, true) => PI + s,
            (false, true) => {
                let w = TWO_PI - s;
                if w < TWO_PI { w } else { 0.0 }
            }
        }
    };
    if vv == 0.0 {
        // On the x axis: two feet inside the evolute's span of the major axis.
        if a > b {
            let c = a * uu / ((a - b) * (a + b));
            if c < 1.0 {
                let s = acos(c);
                return sorted(map(s), map(-s));
            }
        }
        return Angles::One(map(0.0));
    }
    if uu == 0.0 {
        if b > a {
            let c = b * vv / ((b - a) * (b + a));
            if c < 1.0 {
                let s = asin(c);
                return sorted(map(s), map(PI - s));
            }
        }
        return Angles::One(map(HALF_PI));
    }
    Angles::One(map(quadrant_root(a, b, uu, vv)))
}

fn sorted(s: f64, t: f64) -> Angles {
    let (s, t) = (turn_wrap(s), turn_wrap(t));
    if s <= t {
        Angles::Two(s, t)
    } else {
        Angles::Two(t, s)
    }
}

/// Any angle within one turn of `[0, 2 pi)` into it.
fn turn_wrap(t: f64) -> f64 {
    if t < 0.0 {
        turn(t)
    } else if t >= TWO_PI {
        t - TWO_PI
    } else {
        t + 0.0
    }
}

/// The largest power of two not above `x` (`x` positive and finite).
fn pow2_floor(x: f64) -> f64 {
    f64::from_bits(x.to_bits() & 0xfff0_0000_0000_0000).max(f64::MIN_POSITIVE)
}

/// The one root in `(0, pi/2)` of `g(s) = k sin s cos s + a u sin s - b v cos s`,
/// `k = b^2 - a^2`, for `u, v > 0`: the first-quadrant stationary point of the distance
/// from `(u, v)` to `(a cos s, b sin s)`, which is the nearest. `g(0) = -b v < 0` and
/// `g(pi/2) = a u > 0` bracket it.
fn quadrant_root(a: f64, b: f64, u: f64, v: f64) -> f64 {
    let k = (b - a) * (b + a);
    let (au, bv) = (a * u, b * v);
    let g = |s: f64| -> (f64, f64) {
        let (sn, cs) = sin_cos(s);
        let val = k * sn * cs + au * sn - bv * cs;
        let der = k * (cs - sn) * (cs + sn) + au * cs + bv * sn;
        (val, der)
    };
    // The seed: with w = tan(s/2), g (1 + w^2)^2 is the quartic
    // b v w^4 + 2 (a u - k) w^3 + 2 (a u + k) w - b v.
    let c = [-bv, 2.0 * (au + k), 0.0, 2.0 * (au - k), bv];
    let m = c.iter().fold(0.0_f64, |m, x| m.max(x.abs()));
    let c = c.map(|x| x / m);
    let mut best = (f64::INFINITY, HALF_PI * 0.5);
    for &w in solve_quartic(&c, EFF_ZERO).iter() {
        if (0.0..=1.0).contains(&w) {
            let s = 2.0 * atan(w);
            let r = g(s).0.abs();
            if r < best.0 {
                best = (r, s);
            }
        }
    }
    let (mut lo, mut hi, mut s) = (0.0, HALF_PI, best.1);
    for _ in 0..POLISH_STEPS {
        let (val, der) = g(s);
        if val == 0.0 {
            break;
        }
        if val < 0.0 {
            lo = s;
        } else {
            hi = s;
        }
        let newton = s - val / der;
        let next = if newton > lo && newton < hi {
            newton
        } else {
            0.5 * (lo + hi)
        };
        if next == s || (next - s).abs() <= f64::EPSILON * s {
            s = next;
            break;
        }
        s = next;
    }
    s
}

/// Picks one of `closest`'s feet and places it against a periodic `domain` (`at` wrapped by
/// whole turns to start at the domain's `min`); `foot` builds the foot at the domain's
/// start for [`Closest::Every`].
fn invert_periodic<Q>(
    closest: Closest<f64, Q>,
    domain: Option<&Extent1d>,
    foot: impl Fn(f64) -> Foot<f64, Q>,
) -> Inversion<f64, Q> {
    let wrap = |f: Foot<f64, Q>| -> Foot<f64, Q> {
        match domain {
            Some(e) if f.at.is_finite() => Foot {
                at: e.min() + (f.at - e.min()).rem_euclid(TWO_PI),
                ..f
            },
            _ => f,
        }
    };
    match closest {
        Closest::One(f) => place_periodic(wrap(f), domain),
        Closest::Two([f, g]) => {
            let (f, g) = (
                place_periodic(wrap(f), domain),
                place_periodic(wrap(g), domain),
            );
            if f.is_inside() || !g.is_inside() {
                f
            } else {
                g
            }
        }
        Closest::Every { .. } => Inversion::Inside(foot(domain.map_or(0.0, Extent1d::min))),
    }
}

/// A periodic foot already wrapped to the domain: inside when `at <= max` or the domain
/// spans a whole turn.
fn place_periodic<Q>(f: Foot<f64, Q>, domain: Option<&Extent1d>) -> Inversion<f64, Q> {
    let inside = match domain {
        _ if f.at.is_nan() => false,
        None => true,
        Some(e) => e.length() >= TWO_PI || (e.min() <= f.at && f.at <= e.max()),
    };
    if inside {
        Inversion::Inside(f)
    } else {
        Inversion::Outside(f)
    }
}

/// A line's foot against its domain.
fn place<Q>(f: Foot<f64, Q>, domain: Option<&Extent1d>) -> Inversion<f64, Q> {
    let inside = match domain {
        _ if f.at.is_nan() => false,
        None => true,
        Some(e) => e.min() <= f.at && f.at <= e.max(),
    };
    if inside {
        Inversion::Inside(f)
    } else {
        Inversion::Outside(f)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::truth::{U, near};
    use scx_num::frame::{Frame, Frame2, Sense};
    use scx_num::hash::Rng;
    use scx_num::units::Length;
    use scx_nurbs::{KnotVector, NurbsCurve};
    use scx_testkit::analytic;
    use scx_testkit::dd::Dd;
    use scx_testkit::project::{Locus, Nearest};
    use scx_testkit::sample;
    use std::sync::Arc;

    fn frame(rng: &mut Rng, scale: f64) -> Frame {
        let f = sample::frame(rng, scale);
        Frame::new(Point3::from(f.origin), Vec3::from(f.z), Vec3::from(f.x)).unwrap()
    }

    fn sense(i: usize) -> Sense {
        if i.is_multiple_of(2) {
            Sense::Forward
        } else {
            Sense::Reversed
        }
    }

    /// The testkit's frame for a conic as it runs: x, and y times the sense's sign.
    fn kit_frame(f: &Frame, s: Sense) -> analytic::Frame {
        analytic::Frame {
            origin: f.origin().to_array(),
            x: f.x().as_vec().to_array(),
            y: (f.y().as_vec() * s.sign()).to_array(),
            z: f.z().as_vec().to_array(),
        }
    }

    /// Queries about a conic of size `r` in `f`: general, near and on the curve, on and
    /// near its axes, near the centre, far, and off the plane.
    fn queries(rng: &mut Rng, f: &Frame, r: f64) -> Vec<Point3> {
        let (o, x, y, z) = (f.origin(), f.x().as_vec(), f.y().as_vec(), f.z().as_vec());
        let mut v = Vec::new();
        let mut at = |u: f64, w: f64, h: f64| v.push(o + x * u + y * w + z * h);
        for _ in 0..8 {
            let (u, w) = (
                rng.next_signed_unit() * 2.0 * r,
                rng.next_signed_unit() * 2.0 * r,
            );
            at(u, w, 0.0);
            at(u, w, rng.next_signed_unit() * r);
            at(u * 1e-9, w * 1e-9, 0.0);
            at(u * 1e6, w * 1e6, 0.0);
            at(u, w * 1e-12, 0.0);
            at(u * 1e-12, w, 0.0);
            at(u, 0.0, 0.0);
            at(0.0, w, 0.0);
        }
        at(0.0, 0.0, 0.0);
        at(0.0, 0.0, r);
        v
    }

    /// The truth's nearest distance and, when unique, its angle in `[0, 2 pi)`.
    fn kit_nearest(e: &analytic::Ellipse, q: Point3) -> (Dd, Option<Dd>) {
        let p = e.stationary(q.to_array()).unwrap();
        let near = p.nearest_locus().unwrap();
        let d = near.distance().to_dd();
        let at = match (&p.nearest, near) {
            (Nearest::Unique(_), Locus::Point(s)) => {
                let t = s.param.mid;
                Some(if t.hi < 0.0 { t + Dd::new(TWO_PI) } else { t })
            }
            _ => None,
        };
        (d, at)
    }

    fn feet(c: &Closest) -> Vec<Foot> {
        match *c {
            Closest::One(f) => vec![f],
            Closest::Two(f) => f.to_vec(),
            Closest::Every { .. } => Vec::new(),
        }
    }

    #[test]
    fn a_line_foot_is_the_exact_projection() {
        let mut rng = Rng::new(41);
        for scale in [1.0, 1e3, 1e-3] {
            for _ in 0..200 {
                let o = sample::direction(&mut rng).map(|c| c * scale * rng.next_f64());
                let d = sample::direction(&mut rng).map(|c| c * (0.01 + rng.next_f64()));
                let q = sample::direction(&mut rng).map(|c| c * scale * 3.0 * rng.next_f64());
                let line = Line::new(Point3::from(o), Vec3::from(d)).unwrap();
                let kit = analytic::Line {
                    origin: o,
                    direction: d,
                };
                let (t, _) = kit.closest_exact(q).unwrap();
                let f = line.closest(Point3::from(q));
                let t = scx_testkit::bound::rat_to_dd(&t);
                assert!(near(f.at, t, t.hi.abs() + scale, 8.0), "{} {t:?}", f.at);
                let truth = kit.derivs(t, 0)[0];
                let dist = (0..3)
                    .map(|i| (Dd::new(q[i]) - truth[i]) * (Dd::new(q[i]) - truth[i]))
                    .fold(Dd::new(0.0), |s, x| s + x)
                    .sqrt();
                assert!(near(f.distance, dist, 4.0 * scale, 16.0));
                assert!(line.invert(Point3::from(q), None).is_inside());
            }
        }
    }

    #[test]
    fn a_line_inverts_against_its_domain() {
        let line = Line::new(Point3::new(0.0, 0.0, 0.0), Vec3::new(2.0, 0.0, 0.0)).unwrap();
        let e = Extent1d::new(0.0, 1.0).unwrap();
        let i = line.invert(Point3::new(1.0, 5.0, 0.0), Some(&e));
        assert_eq!(
            i,
            Inversion::Inside(Foot {
                at: 0.5,
                point: Point3::new(1.0, 0.0, 0.0),
                distance: 5.0
            })
        );
        let o = line.invert(Point3::new(4.0, 0.0, 0.0), Some(&e));
        assert_eq!(
            o,
            Inversion::Outside(Foot {
                at: 2.0,
                point: Point3::new(4.0, 0.0, 0.0),
                distance: 0.0
            })
        );
        assert!(o.foot().is_on(Length::from_mm(0.0)));
        let l2 = Line2::new(Point2::new(1.0, 1.0), Vec2::new(0.0, 4.0)).unwrap();
        let f = l2.closest(Point2::new(3.0, 3.0));
        assert_eq!((f.at, f.distance), (0.5, 2.0));
    }

    #[test]
    fn a_circle_foot_matches_the_reference_and_its_axis_is_every_point() {
        let mut rng = Rng::new(43);
        for (i, (scale, r)) in [(1.0, 1.0), (1e3, 1e-3), (1e-3, 1e3)]
            .into_iter()
            .enumerate()
        {
            for k in 0..20 {
                let f = frame(&mut rng, scale);
                let s = sense(i + k);
                let c = Circle::new(f, Length::from_mm(r * (0.25 + rng.next_f64())), s).unwrap();
                let kit = analytic::Circle {
                    frame: kit_frame(&f, s),
                    radius: c.radius().mm(),
                };
                for q in queries(&mut rng, &f, c.radius().mm()) {
                    let got = c.closest(q);
                    let size =
                        (q - f.origin()).length() + c.radius().mm() + f.origin().to_vec().length();
                    let rho = (q - f.origin()).cross(f.z().as_vec()).length();
                    if let Some(want) = kit.closest(q.to_array()) {
                        let Closest::One(g) = got else {
                            panic!("{got:?}")
                        };
                        let t = if want.param.hi < 0.0 {
                            want.param + Dd::new(TWO_PI)
                        } else {
                            want.param
                        };
                        assert!((0.0..TWO_PI).contains(&g.at));
                        // The angle's conditioning is 1 / the distance from the axis.
                        let err = (Dd::new(g.at) - t).hi.abs();
                        let wrap = err.min((err - TWO_PI).abs());
                        assert!(
                            wrap <= 64.0 * U * (1.0 + size / rho),
                            "{} {t:?} {wrap}",
                            g.at
                        );
                        assert!(near(g.distance, want.distance, size, 16.0));
                    } else {
                        // On the axis to the reference's rounding: ours is one foot, or
                        // every point if exactly on it, the distance within the query's
                        // offset `rho` from the axis.
                        for g in feet(&got) {
                            let err = (g.distance - want_axis(&c, q)).abs();
                            assert!(err <= 16.0 * U * size + rho, "{err} {rho}");
                        }
                    }
                }
            }
        }
        let c = Circle::new(
            Frame::new(
                Point3::new(1.0, 2.0, 3.0),
                Vec3::new(0.0, 0.0, 1.0),
                Vec3::new(1.0, 0.0, 0.0),
            )
            .unwrap(),
            Length::from_mm(3.0),
            Sense::Forward,
        )
        .unwrap();
        assert_eq!(
            c.closest(Point3::new(1.0, 2.0, 7.0)),
            Closest::Every { distance: 5.0 }
        );
        let e = Extent1d::new(1.0, 2.0).unwrap();
        let i = c.invert(Point3::new(1.0, 2.0, 7.0), Some(&e));
        assert_eq!(
            (i.is_inside(), i.foot().at, i.foot().distance),
            (true, 1.0, 5.0)
        );
    }

    fn want_axis(c: &Circle, q: Point3) -> f64 {
        let h = (q - c.frame().origin()).dot(c.frame().z().as_vec());
        scx_num::fmath::hypot(h, c.radius().mm())
    }

    #[test]
    fn an_ellipse_foot_is_the_certified_nearest() {
        let mut rng = Rng::new(45);
        let mut checked = (0, 0);
        for (i, (scale, size)) in [(1.0, 1.0), (1e3, 1e-3), (1e-3, 1e3), (0.0, 1.0)]
            .into_iter()
            .enumerate()
        {
            for k in 0..12 {
                let f = frame(&mut rng, scale);
                let s = sense(i + k);
                let a = size * (0.25 + rng.next_f64());
                let b = match k % 4 {
                    0 => a,
                    1 => a * (1.0 + 1e-9),
                    2 => a * 0.05,
                    _ => size * (0.25 + rng.next_f64()),
                };
                let e = Ellipse::new(f, Length::from_mm(a), Length::from_mm(b), s).unwrap();
                let kit = analytic::Ellipse {
                    frame: kit_frame(&f, s),
                    semi_x: a,
                    semi_y: b,
                };
                for q in queries(&mut rng, &f, a.max(b)) {
                    let got = e.closest(q);
                    let sz = (q - f.origin()).length() + a.max(b) + f.origin().to_vec().length();
                    let (d, at) = kit_nearest(&kit, q);
                    if let Closest::Every { distance } = got {
                        assert_eq!(a, b);
                        assert!(near(distance, d, sz, 16.0));
                        continue;
                    }
                    for g in feet(&got) {
                        assert!((0.0..TWO_PI).contains(&g.at), "{}", g.at);
                        // Measured worst: 2.3 ulps of the query's size.
                        assert!(near(g.distance, d, sz, 8.0), "{g:?} {d:?} {q:?} {a} {b}");
                    }
                    checked.0 += 1;
                    // The angle agrees where the truth's nearest is unique, except where the
                    // query's offset from an axis or the centre is below the rounding of its
                    // local coordinates: the mirrored feet are then tied to that rounding
                    // (measured: every such case is a query 1e-12 or less off an axis).
                    if let (Closest::One(g), Some(t)) = (got, at) {
                        let err = (Dd::new(g.at) - t).hi.abs();
                        if err.min((err - TWO_PI).abs()) <= 1e-6 {
                            checked.1 += 1;
                        }
                    }
                }
            }
        }
        assert!(checked.0 > 2500 && checked.1 > 1800, "{checked:?}");
    }

    #[test]
    fn an_ellipse_foot_near_its_evolute_and_when_thin_is_the_certified_nearest() {
        let f = Frame::new(
            Point3::new(0.5, -0.25, 2.0),
            Vec3::new(0.0, 0.6, 0.8),
            Vec3::new(1.0, 0.0, 0.0),
        )
        .unwrap();
        let (o, x, y) = (f.origin(), f.x().as_vec(), f.y().as_vec());
        let mut rng = Rng::new(49);
        for (a, b) in [(2.0, 1.0), (1.0, 3.0), (1.0, 1e-4), (5.0, 4.999_999)] {
            let e =
                Ellipse::new(f, Length::from_mm(a), Length::from_mm(b), Sense::Forward).unwrap();
            let kit = analytic::Ellipse {
                frame: kit_frame(&f, Sense::Forward),
                semi_x: a,
                semi_y: b,
            };
            let (big, small) = if a > b { (a, b) } else { (b, a) };
            let cusp = (big - small) * (big + small) / big;
            for _ in 0..60 {
                // About the cusps, the evolute's arcs (the astroid), and the axis inside it.
                let t = rng.next_f64() * TWO_PI;
                let (sn, cs) = sin_cos(t);
                let k = (a - b) * (a + b);
                let (ex, ey) = (k / a * cs * cs * cs, -k / b * sn * sn * sn);
                let jitter = 1e-9 * rng.next_signed_unit();
                for (u, w) in [
                    (ex + jitter, ey - jitter),
                    (ex, ey),
                    if a > b {
                        (cusp * (1.0 + jitter), jitter * 1e-6)
                    } else {
                        (jitter * 1e-6, cusp * (1.0 + jitter))
                    },
                    if a > b {
                        (cusp * rng.next_signed_unit(), 1e-300)
                    } else {
                        (1e-300, cusp * rng.next_signed_unit())
                    },
                ] {
                    let q = o + x * u + y * w;
                    let (d, _) = kit_nearest(&kit, q);
                    let sz = (q - o).length() + big + o.to_vec().length();
                    for g in feet(&e.closest(q)) {
                        assert!(near(g.distance, d, sz, 8.0), "{a} {b} {u} {w} {g:?} {d:?}");
                    }
                }
            }
        }
    }

    #[test]
    fn an_ellipse_has_two_feet_at_its_centre_and_inside_its_evolute_on_an_axis() {
        let f = Frame::new(
            Point3::new(0.0, 0.0, 0.0),
            Vec3::new(0.0, 0.0, 1.0),
            Vec3::new(1.0, 0.0, 0.0),
        )
        .unwrap();
        let e = Ellipse::new(
            f,
            Length::from_mm(2.0),
            Length::from_mm(1.0),
            Sense::Forward,
        )
        .unwrap();
        let Closest::Two([g, h]) = e.closest(Point3::new(0.0, 0.0, 0.0)) else {
            panic!()
        };
        assert_eq!(
            (g.at, h.at, g.distance, h.distance),
            (HALF_PI, 3.0 * HALF_PI, 1.0, 1.0)
        );
        // (a^2 - b^2) / a = 1.5: inside on the major axis, two feet at +-acos(a u / 3).
        let Closest::Two([g, h]) = e.closest(Point3::new(0.75, 0.0, 0.0)) else {
            panic!()
        };
        assert!((g.at - PI / 3.0).abs() <= 4.0 * U * PI, "{}", g.at);
        assert!((h.at - (TWO_PI - g.at)).abs() <= 8.0 * U * TWO_PI);
        assert_eq!(g.distance, h.distance);
        // At the evolute's cusp and beyond, one foot at the vertex.
        assert_eq!(
            e.closest(Point3::new(1.5, 0.0, 0.0)),
            Closest::One(Foot {
                at: 0.0,
                point: Point3::new(2.0, 0.0, 0.0),
                distance: 0.5
            })
        );
        assert!(matches!(
            e.closest(Point3::new(-0.75, 0.0, 0.0)),
            Closest::Two(_)
        ));
        // On the minor axis, one foot; of an ellipse taller than wide, two.
        assert!(
            matches!(e.closest(Point3::new(0.0, 0.5, 0.0)), Closest::One(Foot { at, .. }) if at == HALF_PI)
        );
        let tall = Ellipse::new(
            f,
            Length::from_mm(1.0),
            Length::from_mm(2.0),
            Sense::Reversed,
        )
        .unwrap();
        let Closest::Two([g, h]) = tall.closest(Point3::new(0.0, -0.75, 0.0)) else {
            panic!()
        };
        // Reversed: the local v is 0.75, so asin(b v / (b^2 - a^2)) = pi / 6 and 5 pi / 6.
        assert!(
            (g.at - PI / 6.0).abs() <= 4.0 * U * PI
                && (h.at - 5.0 * PI / 6.0).abs() <= 4.0 * U * PI,
            "{} {}",
            g.at,
            h.at
        );
        let Closest::Two([g, h]) = tall.closest(Point3::new(0.0, 0.0, 1.0)) else {
            panic!()
        };
        assert_eq!((g.at, h.at), (0.0, PI));
        // A domain picks the foot inside it.
        let d = Extent1d::new(PI, TWO_PI).unwrap();
        let i = e.invert(Point3::new(0.0, 0.0, 0.0), Some(&d));
        assert_eq!((i.is_inside(), i.foot().at), (true, 3.0 * HALF_PI));
        let d = Extent1d::new(0.1, 1.0).unwrap();
        let i = e.invert(Point3::new(0.0, 0.0, 0.0), Some(&d));
        assert_eq!((i.is_inside(), i.foot().at), (false, HALF_PI));
    }

    #[test]
    fn a_periodic_domain_wraps_the_angle_by_whole_turns() {
        let f = Frame::new(
            Point3::new(0.0, 0.0, 0.0),
            Vec3::new(0.0, 0.0, 1.0),
            Vec3::new(1.0, 0.0, 0.0),
        )
        .unwrap();
        let c = Circle::new(f, Length::from_mm(1.0), Sense::Forward).unwrap();
        let q = Point3::new(0.0, -2.0, 0.0); // angle 3 pi / 2
        let d = Extent1d::new(-PI, 0.0).unwrap();
        let i = c.invert(q, Some(&d));
        assert!(i.is_inside());
        assert!((i.foot().at + HALF_PI).abs() <= 4.0 * U * PI);
        let d = Extent1d::new(4.0 * PI, 5.0 * PI).unwrap();
        let i = c.invert(q, Some(&d));
        assert!(!i.is_inside() && i.foot().at > 5.0 * PI && i.foot().at < 6.0 * PI);
        let d = Extent1d::new(1.0, 1.0 + TWO_PI).unwrap();
        assert!(c.invert(q, Some(&d)).is_inside());
    }

    #[test]
    fn the_2d_kinds_agree_with_the_3d_kinds_in_the_plane() {
        let mut rng = Rng::new(47);
        for k in 0..40 {
            let (o, x) = (
                [rng.next_signed_unit(), rng.next_signed_unit()],
                sample::direction(&mut rng),
            );
            let f2 = Frame2::new(Point2::new(o[0], o[1]), Vec2::new(x[0], x[1])).unwrap();
            let f3 = Frame::new(
                Point3::new(o[0], o[1], 0.0),
                Vec3::new(0.0, 0.0, 1.0),
                Vec3::new(f2.x().as_vec().x, f2.x().as_vec().y, 0.0),
            )
            .unwrap();
            let (a, b) = (0.25 + rng.next_f64(), 0.25 + rng.next_f64());
            let e2 = Ellipse2::new(f2, Length::from_mm(a), Length::from_mm(b), sense(k)).unwrap();
            let e3 = Ellipse::new(f3, Length::from_mm(a), Length::from_mm(b), sense(k)).unwrap();
            let c2 = Circle2::new(f2, Length::from_mm(a), sense(k)).unwrap();
            let c3 = Circle::new(f3, Length::from_mm(a), sense(k)).unwrap();
            for _ in 0..20 {
                let q = [2.0 * rng.next_signed_unit(), 2.0 * rng.next_signed_unit()];
                let (q2, q3) = (Point2::new(q[0], q[1]), Point3::new(q[0], q[1], 0.0));
                let (Closest::One(g2), Closest::One(g3)) = (e2.closest(q2), e3.closest(q3)) else {
                    panic!()
                };
                assert!(
                    (g2.at - g3.at).abs() <= 64.0 * U * TWO_PI
                        && (g2.distance - g3.distance).abs() <= 64.0 * U * 4.0
                );
                let (Closest::One(g2), Closest::One(g3)) = (c2.closest(q2), c3.closest(q3)) else {
                    panic!()
                };
                assert!(
                    (g2.at - g3.at).abs() <= 64.0 * U * TWO_PI
                        && (g2.distance - g3.distance).abs() <= 64.0 * U * 4.0
                );
            }
        }
        let c2 = Circle2::new(
            Frame2::new(Point2::new(1.0, 1.0), Vec2::new(1.0, 0.0)).unwrap(),
            Length::from_mm(2.0),
            Sense::Forward,
        )
        .unwrap();
        assert_eq!(
            c2.closest(Point2::new(1.0, 1.0)),
            Closest::Every { distance: 2.0 }
        );
    }

    #[test]
    fn a_non_finite_query_gives_a_nan_foot_and_nurbs_is_unsupported() {
        let f = Frame::new(
            Point3::new(0.0, 0.0, 0.0),
            Vec3::new(0.0, 0.0, 1.0),
            Vec3::new(1.0, 0.0, 0.0),
        )
        .unwrap();
        let e = Ellipse::new(
            f,
            Length::from_mm(2.0),
            Length::from_mm(1.0),
            Sense::Forward,
        )
        .unwrap();
        for q in [
            Point3::new(f64::NAN, 0.0, 0.0),
            Point3::new(f64::INFINITY, 1.0, 0.0),
        ] {
            let Closest::One(g) = e.closest(q) else {
                panic!()
            };
            assert!(g.at.is_nan() && !g.is_on(Length::from_mm(1e300)));
            assert!(!e.invert(q, None).is_inside());
        }
        let c = Circle::new(f, Length::from_mm(1.0), Sense::Forward).unwrap();
        assert!(!c.invert(Point3::new(f64::NAN, 0.0, 0.0), None).is_inside());
        let k = KnotVector::new(1, vec![0.0, 0.0, 1.0, 1.0]).unwrap();
        let n = NurbsCurve::new(k, vec![[0.0; 3], [1.0, 0.0, 0.0]], None).unwrap();
        let err = Curve::Nurbs(Arc::new(n))
            .invert(Point3::new(0.0, 0.0, 0.0), None)
            .unwrap_err();
        assert_eq!(err, CurveError::Unsupported(Unsupported::NurbsInversion));
        let line =
            Curve::Line(Line::new(Point3::new(0.0, 0.0, 0.0), Vec3::new(1.0, 0.0, 0.0)).unwrap());
        assert_eq!(
            line.closest(Point3::new(2.0, 1.0, 0.0)).unwrap(),
            Closest::One(Foot {
                at: 2.0,
                point: Point3::new(2.0, 0.0, 0.0),
                distance: 1.0
            })
        );
    }
}
