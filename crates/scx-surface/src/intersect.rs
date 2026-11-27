// Written for solidcorex. Apache-2.0.
//! Where a line, circle or ellipse meets a plane, cylinder, cone, sphere or torus (research
//! 63 §4.2, SU6; §4.11 has the decisions).
//!
//! **The equation.** The curve is taken into the surface's axes (its frame, z times its
//! sense) as a polynomial in a parameter `u` on `[0, 1]`, in Bernstein form and homogeneous:
//! coordinates `X, Y, Z` over a weight `H`.
//!
//! - A line over its extent `[lo, hi]` is the segment's two ends (`H = 1`), `t = (1 - u) lo +
//!   u hi`.
//! - A conic `C + a cos t X + b sin t Y` is two half turns, each the rational quadratic
//!   `[C - B, A, C + B]` over `[1, 0, 1]` (`A = a X`, `B = b Y`, signs flipped for the second
//!   half): `w = 2u - 1 = tan((t - t0) / 2)`, `t0` zero or `pi`. The halves meet at the
//!   same control point, so an end coefficient is the same bits in both and a sign change at
//!   the seam is seen by exactly one of them.
//!
//! The surface is the zero set of a homogeneous polynomial in the axes' coordinates: plane
//! `Z`; cylinder `X^2 + Y^2 - r^2 H^2`; cone `X^2 + Y^2 - (r H + k Z)^2` (both nappes; the
//! far one is discarded); sphere `X^2 + Y^2 + Z^2 - r^2 H^2`; torus
//! `(X^2 + Y^2 + Z^2 + (R^2 - r^2) H^2)^2 - 4 R^2 H^2 (X^2 + Y^2)`. Substituted, the
//! equation `g(u)` has degree 1, 2 or 4 for a line and 2, 4 or 8 for a conic, the degrees
//! of 63 §4.2 and testkit T2.
//!
//! **The roots.** `scx-algo`'s [`bernstein_roots`] isolates the sign changes of `g` (its
//! crossings) and of the derivative of `g / H^d` (its turning points: `g'` for a line,
//! `H g' - d H' g` for a conic), each certified by a sign change and refined to rounding.
//! Nothing samples a grid: where neither changes sign there is no root, and an empty answer
//! is final (04, research 68).
//!
//! **Tangency at the tolerance.** The roots, the turning points and the extent's ends are
//! events in `t`; between two neighbours `g` is monotone and keeps one sign, so the curve
//! moves steadily toward or away from the surface there (exactly for the plane, cylinder
//! and sphere, whose `g` is a function of the distance; at the tolerance for the cone and
//! torus). An event is near when the curve is within `tol` of the surface there (a root
//! always is, but the cone's far nappe). A run of
//! near events is one stretch within `tol` of the surface:
//!
//! - every event near: the curve lies on the surface, [`CurveSurfaceHit::Overlap`] over
//!   the whole extent;
//! - a run with a turning point: one [`Contact::Tangent`] at its nearest turning point
//!   (an exact double root is a turning point of `g`; a near miss or a pair of crossings
//!   closer than `tol` allows is the same touch at the tolerance);
//! - a run with one root and no turning point: a [`Contact::Crossing`] at the root;
//! - a run that is only an end: a crossing at that end (the root lies just past it).
//!
//! **The uv** is the foot of the hit's point on the surface in D13's parameters, with no
//! transcendental function (the arcs' `of_direction`), then wrapped by the surface's periods
//! into `se`; a point whose uv is outside `se` is dropped. An overlap is not clipped by `se`
//! (§4.11).

use core::f64::consts::{PI, TAU};

use scx_algo::bernstein::{RootLimits, bernstein_roots};
use scx_curve::Curve;
use scx_num::budget::{Budget, Stop};
use scx_num::extent1d::Extent1d;
use scx_num::extent2d::Extent2d;
use scx_num::fmath;
use scx_num::frame::{Frame, Sense};
use scx_num::units::Length;
use scx_num::vector2d::Point2;
use scx_num::vector3d::{Point3, Vec3};

use scx_curve::arcs::Arcs;
use crate::common::Axes;
use crate::error::SurfaceError;
use crate::surface::Surface;

/// How the curve meets the surface at a point.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Contact {
    /// The curve crosses the surface.
    Crossing,
    /// The curve touches the surface within the tolerance: a multiple root, a near miss, or
    /// crossings closer together than the tolerance tells apart.
    Tangent,
}

/// One place where a curve meets a surface.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum CurveSurfaceHit {
    /// An isolated point.
    Point {
        /// The curve's parameter (a conic's angle in radians, inside the curve's extent).
        t: f64,
        /// The surface's parameters (D13), inside the surface's extent.
        uv: Point2,
        /// The curve's point at `t`.
        point: Point3,
        /// Crossing or tangent.
        contact: Contact,
    },
    /// The curve lies on the surface, within the tolerance, over this extent: the whole
    /// curve extent (an analytic curve on an analytic surface is on it everywhere or at
    /// isolated points).
    Overlap {
        /// The curve parameters.
        t: Extent1d,
    },
}

impl CurveSurfaceHit {
    /// The curve parameter that orders the hits: the point's `t`, an overlap's start.
    fn order_t(&self) -> f64 {
        match self {
            Self::Point { t, .. } => *t,
            Self::Overlap { t } => t.min(),
        }
    }
}

/// Every place where the curve `c` over `ce` meets the surface `s` inside `se`, within
/// `tol`, sorted by `t`, then `u`, then `v` (63 A14).
///
/// A point lies on the curve at its `t` and within `tol` of the surface (a crossing within
/// the rounding of the root); its uv is the surface's foot of that point. A conic's `t` is
/// its angle in radians; a closed extent (`ce` a turn or more) is the one turn from
/// `ce.min()`. The analytic arms are closed forms: they charge `budget` one unit for each
/// polynomial isolated and check `stop` before each piece of the curve.
///
/// # Errors
///
/// - [`SurfaceError::NurbsIntersection`] (Unsupported) when the curve or the surface is a
///   NURBS one: not built yet, never an empty answer.
/// - [`SurfaceError::Exhausted`] when `budget` runs out, [`SurfaceError::Stopped`] when
///   `stop` is set; neither returns a partial answer.
pub fn intersect_curve(
    c: &Curve,
    ce: &Extent1d,
    s: &Surface,
    se: &Extent2d,
    tol: Length,
    budget: &mut Budget,
    stop: &Stop,
) -> Result<Vec<CurveSurfaceHit>, SurfaceError> {
    let Some(target) = Target::of(s) else {
        return Err(SurfaceError::NurbsIntersection);
    };
    let Some(path) = Path::of(c, ce) else {
        return Err(SurfaceError::NurbsIntersection);
    };
    let tol = tol.mm();
    let mut events = Vec::with_capacity(16);
    for piece in path.pieces(&target) {
        stop.check().map_err(|_| SurfaceError::Stopped)?;
        let g = target.equation(&piece);
        let dg = piece.turns(&g);
        budget.charge(2).map_err(SurfaceError::Exhausted)?;
        let roots = bernstein_roots(g.coefficients(), 0.0, 1.0, RootLimits::DEFAULT);
        let turns = bernstein_roots(dg.coefficients(), 0.0, 1.0, RootLimits::DEFAULT);
        for (places, kind) in [(roots.places, Kind::Root), (turns.places, Kind::Turn)] {
            for u in places {
                let t = piece.map.t(u);
                let t = if kind == Kind::Root { path.polish(&target, t) } else { t };
                if let Some(t) = path.place(t) {
                    events.push(Event::new(c, &target, t, kind, tol));
                }
            }
        }
    }
    let (ends, kind) = if path.closed {
        let lo = ce.min();
        ([lo, lo + PI], Kind::Probe)
    } else {
        ([ce.min(), ce.max()], Kind::End)
    };
    for t in ends {
        events.push(Event::new(c, &target, t, kind, tol));
    }
    events.sort_by(|a, b| a.t.total_cmp(&b.t).then(a.kind.cmp(&b.kind)));
    events.dedup_by(|b, a| a.kind == b.kind && (b.t - a.t).abs() <= 8.0 * f64::EPSILON * a.t.abs().max(1.0));
    if events.iter().all(|e| e.near) {
        return Ok(vec![CurveSurfaceHit::Overlap { t: path.extent }]);
    }
    // A closed curve's events are a cycle: start after a far one, so no run is cut in two.
    if path.closed {
        if let Some(i) = events.iter().position(|e| !e.near) {
            events.rotate_left(i + 1);
        }
    }
    let mut hits = Vec::new();
    for run in events.split(|e| !e.near) {
        if let Some((t, contact)) = classify(run) {
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
    Ok(hits)
}

/// The hit a run of near events stands for: the nearest turning point as a tangent, else
/// the one root as a crossing (the middle one, should rounding leave more), else a lone
/// end as a crossing.
fn classify(run: &[Event]) -> Option<(f64, Contact)> {
    let nearest_turn = run
        .iter()
        .filter(|e| e.kind == Kind::Turn)
        .min_by(|a, b| a.distance.total_cmp(&b.distance));
    if let Some(e) = nearest_turn {
        return Some((e.t, Contact::Tangent));
    }
    let roots: Vec<&Event> = run.iter().filter(|e| e.kind == Kind::Root).collect();
    match roots.len() {
        0 => run
            .iter()
            .find(|e| e.kind == Kind::End)
            .map(|e| (e.t, Contact::Crossing)),
        1 => Some((roots[0].t, Contact::Crossing)),
        n => Some((roots[n / 2].t, Contact::Tangent)),
    }
}

/// What an event is. The order breaks ties in `t`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Kind {
    /// A root of `g`.
    Root,
    /// A turning point of `g` along the curve.
    Turn,
    /// An end of the extent.
    End,
    /// A closed curve's probe: breaks the cycle where it is far, stands for nothing.
    Probe,
}

/// A place on the curve where something may happen.
#[derive(Clone, Copy, Debug)]
struct Event {
    t: f64,
    kind: Kind,
    /// The curve point's distance from the surface.
    distance: f64,
    /// Within `tol` of the surface (a root on the surface's own sheet always is).
    near: bool,
}

impl Event {
    fn new(c: &Curve, target: &Target, t: f64, kind: Kind, tol: f64) -> Self {
        let l = target.axes.local(c.point(t));
        let distance = target.distance(l);
        let near = distance <= tol || (kind == Kind::Root && target.on_sheet(l));
        Self {
            t,
            kind,
            distance,
            near,
        }
    }
}

// ---------------------------------------------------------------- the curve

/// How a piece's `u` gives the curve's `t`.
#[derive(Clone, Copy, Debug)]
enum Map {
    /// `t = (1 - u) lo + u hi`.
    Segment { lo: f64, hi: f64 },
    /// `t = t0 + 2 atan(2u - 1)`.
    Half { t0: f64 },
}

impl Map {
    fn t(self, u: f64) -> f64 {
        match self {
            Self::Segment { lo, hi } => (1.0 - u) * lo + u * hi,
            Self::Half { t0 } => t0 + 2.0 * fmath::atan(2.0 * u - 1.0),
        }
    }
}

/// A piece of the curve in the surface's axes: Bernstein coordinates over `[0, 1]` and
/// their weight.
#[derive(Clone, Copy, Debug)]
struct Piece {
    x: Bern,
    y: Bern,
    z: Bern,
    h: Bern,
    map: Map,
}

impl Piece {
    /// A polynomial whose roots are the turning points of the equation's affine value
    /// `f = g / H^d` along the piece: `g'` for a segment (`H` is one), else the numerator
    /// `H g' - d H' g` of `f'` (`g` is homogeneous of degree `d` in the coordinates, so
    /// `d = deg g / deg H`), one degree up.
    fn turns(&self, g: &Bern) -> Bern {
        match self.map {
            Map::Segment { .. } => g.derivative(),
            Map::Half { .. } => {
                let d = (g.n / self.h.n) as f64;
                self.h
                    .mul(&g.derivative())
                    .sub(&self.h.derivative().mul(g).scale(d))
            }
        }
    }
}

/// The curve over its extent.
#[derive(Clone, Copy, Debug)]
struct Path {
    kind: PathKind,
    extent: Extent1d,
    /// A conic over a turn or more.
    closed: bool,
}

#[derive(Clone, Copy, Debug)]
enum PathKind {
    Line { origin: Point3, direction: Vec3 },
    Conic { centre: Point3, a: Vec3, b: Vec3 },
}

impl Path {
    fn of(c: &Curve, ce: &Extent1d) -> Option<Self> {
        let conic = |f: &Frame, a: f64, b: f64| PathKind::Conic {
            centre: f.origin(),
            a: f.x().as_vec() * a,
            b: f.y().as_vec() * b,
        };
        let kind = match c {
            Curve::Line(l) => PathKind::Line {
                origin: l.origin(),
                direction: l.direction(),
            },
            Curve::Circle(k) => {
                let r = k.radius().mm();
                conic(k.frame(), r, r * k.sense().sign())
            }
            Curve::Ellipse(e) => conic(
                e.frame(),
                e.semi_x().mm(),
                e.semi_y().mm() * e.sense().sign(),
            ),
            _ => return None,
        };
        let closed = matches!(kind, PathKind::Conic { .. }) && ce.length() >= TAU;
        Some(Self {
            kind,
            extent: *ce,
            closed,
        })
    }

    /// The pieces in the target's axes: the segment, or the two half turns.
    fn pieces(&self, target: &Target) -> impl Iterator<Item = Piece> {
        let ax = &target.axes;
        let lin = |p: [f64; 3], q: [f64; 3], map| Piece {
            x: Bern::new(&[p[0], q[0]]),
            y: Bern::new(&[p[1], q[1]]),
            z: Bern::new(&[p[2], q[2]]),
            h: Bern::new(&[1.0, 1.0]),
            map,
        };
        let quad = |p: [f64; 3], m: [f64; 3], q: [f64; 3], map| Piece {
            x: Bern::new(&[p[0], m[0], q[0]]),
            y: Bern::new(&[p[1], m[1], q[1]]),
            z: Bern::new(&[p[2], m[2], q[2]]),
            h: Bern::new(&[1.0, 0.0, 1.0]),
            map,
        };
        let mut out = [None, None];
        match self.kind {
            PathKind::Line { origin, direction } => {
                let (lo, hi) = (self.extent.min(), self.extent.max());
                let p = tuple(ax.local(origin + direction * lo));
                let q = tuple(ax.local(origin + direction * hi));
                out[0] = Some(lin(p, q, Map::Segment { lo, hi }));
            }
            PathKind::Conic { centre, a, b } => {
                let (cx, cy, cz) = ax.local(centre);
                let c = Vec3::new(cx, cy, cz);
                let along = |v: Vec3| Vec3::new(v.dot(ax.x), v.dot(ax.y), v.dot(ax.z));
                let (a, b) = (along(a), along(b));
                for (k, t0) in [(0, 0.0), (1, PI)] {
                    let s = if k == 0 { 1.0 } else { -1.0 };
                    let (a, b) = (a * s, b * s);
                    out[k] = Some(quad(
                        (c - b).to_array(),
                        a.to_array(),
                        (c + b).to_array(),
                        Map::Half { t0 },
                    ));
                }
            }
        }
        out.into_iter().flatten()
    }

    /// The point and velocity at `t` in the target's axes.
    fn local(&self, target: &Target, t: f64) -> ([f64; 3], [f64; 3]) {
        let ax = &target.axes;
        let along = |v: Vec3| [v.dot(ax.x), v.dot(ax.y), v.dot(ax.z)];
        let (p, d) = match self.kind {
            PathKind::Line { origin, direction } => (origin + direction * t, direction),
            PathKind::Conic { centre, a, b } => {
                let (sin, cos) = (fmath::sin(t), fmath::cos(t));
                (centre + a * cos + b * sin, b * cos - a * sin)
            }
        };
        (tuple(ax.local(p)), along(d))
    }

    /// A root `t` of the Bernstein equation refined on the surface's own equation, whose
    /// value the curve's point gives without the coefficients' rounding: Newton steps
    /// kept only while each lowers `|f|` and moves `t` by less than a millionth of the
    /// curve's scale, so a root never moves to another.
    fn polish(&self, target: &Target, mut t: f64) -> f64 {
        let reach = match self.kind {
            PathKind::Line { .. } => 1e-6 * (self.extent.length() + t.abs()),
            PathKind::Conic { .. } => 1e-6,
        };
        let (l, mut d) = self.local(target, t);
        let (mut f, mut grad) = target.implicit(l);
        for _ in 0..3 {
            let slope = grad[0] * d[0] + grad[1] * d[1] + grad[2] * d[2];
            let step = f / slope;
            if !step.is_finite() || step.abs() > reach || step == 0.0 {
                break;
            }
            let next = t - step;
            let (l, d2) = self.local(target, next);
            let (g, grad2) = target.implicit(l);
            if g.abs() >= f.abs() {
                break;
            }
            (t, f, grad, d) = (next, g, grad2, d2);
        }
        t
    }

    /// `t` placed in the extent (a conic's angle wrapped to the turn from `ce.min()`), or
    /// `None` outside it.
    fn place(&self, t: f64) -> Option<f64> {
        let (lo, hi) = (self.extent.min(), self.extent.max());
        let t = match self.kind {
            PathKind::Line { .. } => t,
            PathKind::Conic { .. } => {
                let mut t = t + TAU * ((lo - t) / TAU).ceil();
                if t < lo {
                    t += TAU;
                }
                if t >= lo + TAU {
                    t -= TAU;
                }
                t.max(lo)
            }
        };
        if self.closed {
            (t < lo + TAU).then_some(t)
        } else {
            (lo..=hi).contains(&t).then_some(t)
        }
    }
}

fn tuple((x, y, z): (f64, f64, f64)) -> [f64; 3] {
    [x, y, z]
}

// ---------------------------------------------------------------- the surface

/// A surface's equation in its axes.
#[derive(Clone, Copy, Debug)]
enum Shape {
    Plane,
    Cylinder { r: f64, arcs: Arcs },
    Cone { r: f64, k: f64 },
    Sphere { r: f64 },
    Torus { big: f64, small: f64 },
}

/// The surface's axes, equation and uv.
#[derive(Clone, Copy, Debug)]
struct Target {
    axes: Axes,
    shape: Shape,
}

/// The cone's and sphere's `u`, and both of the torus's: a turn over `[0, 1]`.
const TURN: Arcs = Arcs::turn(1.0);
/// The sphere's `v`.
const HALF: Arcs = Arcs::half();

impl Target {
    fn of(s: &Surface) -> Option<Self> {
        let (axes, shape) = match s {
            Surface::Plane(p) => (Axes::new(p.frame(), Sense::Forward), Shape::Plane),
            Surface::Cylinder(c) => {
                let r = c.radius().mm();
                (
                    Axes::new(c.frame(), c.sense()),
                    Shape::Cylinder {
                        r,
                        arcs: Arcs::turn(2.0 * PI * r),
                    },
                )
            }
            Surface::Cone(c) => (
                Axes::new(c.frame(), c.sense()),
                Shape::Cone {
                    r: c.radius().mm(),
                    k: c.slope(),
                },
            ),
            Surface::Sphere(c) => (
                Axes::new(c.frame(), c.sense()),
                Shape::Sphere {
                    r: c.radius().mm(),
                },
            ),
            Surface::Torus(c) => (
                Axes::new(c.frame(), c.sense()),
                Shape::Torus {
                    big: c.major().mm(),
                    small: c.minor().mm(),
                },
            ),
            Surface::Nurbs(_) => return None,
        };
        Some(Self { axes, shape })
    }

    /// `g(u)`: the equation on the piece.
    fn equation(&self, p: &Piece) -> Bern {
        let (x, y, z, h) = (&p.x, &p.y, &p.z, &p.h);
        let rad2 = x.mul(x).add(&y.mul(y));
        match self.shape {
            Shape::Plane => *z,
            Shape::Cylinder { r, .. } => rad2.sub(&h.mul(h).scale(r * r)),
            Shape::Cone { r, k } => {
                let w = h.scale(r).add(&z.scale(k));
                rad2.sub(&w.mul(&w))
            }
            Shape::Sphere { r } => rad2.add(&z.mul(z)).sub(&h.mul(h).scale(r * r)),
            Shape::Torus { big, small } => {
                let h2 = h.mul(h);
                let q = (big - small) * (big + small);
                let s = rad2.add(&z.mul(z)).add(&h2.scale(q));
                s.mul(&s).sub(&h2.mul(&rad2).scale(4.0 * big * big))
            }
        }
    }

    /// The affine equation at `l` in the axes (the homogeneous one at `H = 1`) and its
    /// gradient.
    fn implicit(&self, [x, y, z]: [f64; 3]) -> (f64, [f64; 3]) {
        let rad2 = x * x + y * y;
        match self.shape {
            Shape::Plane => (z, [0.0, 0.0, 1.0]),
            Shape::Cylinder { r, .. } => (rad2 - r * r, [2.0 * x, 2.0 * y, 0.0]),
            Shape::Cone { r, k } => {
                let w = r + k * z;
                (rad2 - w * w, [2.0 * x, 2.0 * y, -2.0 * k * w])
            }
            Shape::Sphere { r } => (rad2 + z * z - r * r, [2.0 * x, 2.0 * y, 2.0 * z]),
            Shape::Torus { big, small } => {
                let s = rad2 + z * z + (big - small) * (big + small);
                let b = 4.0 * big * big;
                (
                    s * s - b * rad2,
                    [4.0 * s * x - 2.0 * b * x, 4.0 * s * y - 2.0 * b * y, 4.0 * s * z],
                )
            }
        }
    }

    /// The distance from the surface of the point at `l` in the axes.
    fn distance(&self, (x, y, z): (f64, f64, f64)) -> f64 {
        let rho = || fmath::hypot(x, y);
        match self.shape {
            Shape::Plane => z.abs(),
            Shape::Cylinder { r, .. } => (rho() - r).abs(),
            Shape::Sphere { r } => (fmath::hypot(rho(), z) - r).abs(),
            Shape::Torus { big, small } => (fmath::hypot(rho() - big, z) - small).abs(),
            Shape::Cone { r, k } => {
                let rho = rho();
                let n = (k * k + 1.0).sqrt();
                if r + k * cone_foot(rho, z, r, k) > 0.0 {
                    ((rho - r) - k * z).abs() / n
                } else {
                    fmath::hypot(rho, z + r / k)
                }
            }
        }
    }

    /// Whether a root at `l` is on the surface itself: every root but the cone's on its
    /// far nappe.
    fn on_sheet(&self, (_, _, z): (f64, f64, f64)) -> bool {
        match self.shape {
            Shape::Cone { r, k } => r + k * z >= 0.0,
            _ => true,
        }
    }

    /// The foot of `p` in D13's uv, wrapped by the periods into `se`, or `None` outside.
    fn uv_in(&self, p: Point3, se: &Extent2d) -> Option<Point2> {
        let (x, y, z) = self.axes.local(p);
        let rho = || fmath::hypot(x, y);
        let (uv, periods) = match self.shape {
            Shape::Plane => (Point2::new(x, y), (None, None)),
            Shape::Cylinder { r, arcs } => (
                Point2::new(arcs.of_direction(x, y), z),
                (Some(2.0 * PI * r), None),
            ),
            Shape::Cone { r, k } => {
                let v = cone_foot(rho(), z, r, k);
                let v = if r + k * v > 0.0 { v } else { -r / k };
                (Point2::new(TURN.of_direction(x, y), v), (Some(1.0), None))
            }
            Shape::Sphere { .. } => (
                Point2::new(TURN.of_direction(x, y), HALF.of_direction(rho(), z)),
                (Some(1.0), None),
            ),
            Shape::Torus { big, .. } => (
                Point2::new(TURN.of_direction(x, y), TURN.of_direction(rho() - big, z)),
                (Some(1.0), Some(1.0)),
            ),
        };
        let (lo, hi) = (se.min(), se.max());
        let u = wrap(uv.x, periods.0, lo.x, hi.x)?;
        let v = wrap(uv.y, periods.1, lo.y, hi.y)?;
        Some(Point2::new(u, v))
    }
}

/// The height of the foot of `(rho, h)` on the generator `rho = r + k v`.
fn cone_foot(rho: f64, h: f64, r: f64, k: f64) -> f64 {
    ((rho - r) * k + h) / (k * k + 1.0)
}

/// `x` moved by whole periods into `[lo, hi]` (the copy at or above `lo`), or `None`.
fn wrap(x: f64, period: Option<f64>, lo: f64, hi: f64) -> Option<f64> {
    let x = match period {
        Some(p) if x < lo || x > hi => x + p * ((lo - x) / p).ceil(),
        _ => x,
    };
    (lo..=hi).contains(&x).then_some(x)
}

// ---------------------------------------------------------------- Bernstein polynomials

/// The most degree a polynomial here reaches: a conic against the torus is 8, its turning
/// points 9.
const MAX: usize = 9;

/// `C(n, k)` for `n <= MAX`.
const BINOMIAL: [[f64; MAX + 1]; MAX + 1] = {
    let mut b = [[0.0; MAX + 1]; MAX + 1];
    let mut n = 0;
    while n <= MAX {
        b[n][0] = 1.0;
        let mut k = 1;
        while k <= n {
            b[n][k] = b[n - 1][k - 1] + if k < n { b[n - 1][k] } else { 0.0 };
            k += 1;
        }
        n += 1;
    }
    b
};

/// A polynomial of degree `n <= 9` on `[0, 1]` in Bernstein form.
#[derive(Clone, Copy, Debug)]
struct Bern {
    c: [f64; MAX + 1],
    n: usize,
}

impl Bern {
    fn new(c: &[f64]) -> Self {
        let mut out = [0.0; MAX + 1];
        out[..c.len()].copy_from_slice(c);
        Self {
            c: out,
            n: c.len() - 1,
        }
    }

    fn coefficients(&self) -> &[f64] {
        &self.c[..=self.n]
    }

    fn mul(&self, o: &Self) -> Self {
        let n = self.n + o.n;
        let mut c = [0.0; MAX + 1];
        for (i, a) in self.coefficients().iter().enumerate() {
            for (j, b) in o.coefficients().iter().enumerate() {
                c[i + j] += BINOMIAL[self.n][i] * BINOMIAL[o.n][j] * a * b;
            }
        }
        for (k, x) in c.iter_mut().enumerate().take(n + 1) {
            *x /= BINOMIAL[n][k];
        }
        Self { c, n }
    }

    /// The sum of two of the same degree.
    fn add(&self, o: &Self) -> Self {
        debug_assert_eq!(self.n, o.n);
        let mut c = self.c;
        for (x, y) in c.iter_mut().zip(o.c) {
            *x += y;
        }
        Self { c, n: self.n }
    }

    fn sub(&self, o: &Self) -> Self {
        self.add(&o.scale(-1.0))
    }

    fn scale(&self, k: f64) -> Self {
        Self {
            c: self.c.map(|x| x * k),
            n: self.n,
        }
    }

    /// `d/du`, degree `n - 1` (a constant's is the zero constant).
    fn derivative(&self) -> Self {
        if self.n == 0 {
            return Self::new(&[0.0]);
        }
        let mut c = [0.0; MAX + 1];
        for (i, x) in c.iter_mut().enumerate().take(self.n) {
            *x = self.n as f64 * (self.c[i + 1] - self.c[i]);
        }
        Self { c, n: self.n - 1 }
    }
}
