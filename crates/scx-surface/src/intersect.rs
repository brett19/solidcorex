// Written for solidcorex. Apache-2.0.
//! Where a line, circle, ellipse, parabola or hyperbola meets a plane, cylinder, cone,
//! sphere or torus (research 63 §4.2, SU6; §4.13, §4.14 and §3.12 have the decisions).
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
//! - A parabola `V + F s^2 X + 2 F s Y` over `[lo, hi]` is the polynomial quadratic
//!   `[P(lo), V + A lo hi + B (lo + hi) / 2, P(hi)]` over `[1, 1, 1]` (`A = F X`,
//!   `B = 2 F Y`), `s` affine in `u` as a line's `t`.
//! - A hyperbola `C + a cosh t X + b sinh t Y` over `[lo, hi]` is the rational quadratic
//!   whose control points are the ends and the meeting point of their tangents,
//!   `C + (A cosh m + B sinh m) / cosh w`, over `[1, cosh w, 1]` (`m` the middle and `w`
//!   the half width): `2u - 1 = tanh((t - m) / 2) / tanh(w / 2)`, the conic's map with the
//!   hyperbolic functions.
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
//! into `se`; a point whose uv is outside `se` is dropped. An overlap is clipped by `se`
//! into the pieces whose uv is inside it (§4.14): the cuts are where the curve meets the
//! planes, in the surface's axes, that hold `se`'s sides (a side of constant `u` is a
//! plane through the axis; one of constant `v` a plane across it, or the plane's own
//! lines), each piece is kept when its middle's uv is in `se`, and a touch of no length is
//! dropped.

use core::f64::consts::{PI, TAU};

use scx_algo::bernstein::{RootLimits, bernstein_roots};
use scx_curve::Curve;
use scx_num::budget::{Budget, Stop};
use scx_num::extent1d::Extent1d;
use scx_num::extent2d::Extent2d;
use scx_num::fmath;
use scx_num::frame::{Frame, Sense};
use scx_num::units::Resolution;
use scx_num::vector2d::Point2;
use scx_num::vector3d::{Point3, Vec3};

use crate::common::Axes;
use crate::error::SurfaceError;
use crate::surface::Surface;
use scx_curve::arcs::Arcs;

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
    /// The curve lies on the surface, within the tolerance, over this extent: a piece of
    /// the curve extent whose uv lies in the surface's extent (an analytic curve on an
    /// analytic surface is on it everywhere or at isolated points, so the pieces are where
    /// the curve leaves and re-enters `se`). The whole extent when no part leaves `se`;
    /// on a closed curve a piece across the turn's start runs past `ce.min() + 2 pi`
    /// (63 §4.14).
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
/// `res.zone()` (`tol` below), sorted by `t`, then `u`, then `v` (63 A14).
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
    res: Resolution,
    budget: &mut Budget,
    stop: &Stop,
) -> Result<Vec<CurveSurfaceHit>, SurfaceError> {
    let Some(target) = Target::of(s) else {
        return Err(SurfaceError::NurbsIntersection);
    };
    let Some(path) = Path::of(c, ce) else {
        return Err(SurfaceError::NurbsIntersection);
    };
    let tol = res.zone().mm();
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
                let t = if kind == Kind::Root {
                    path.polish(&target, t)
                } else {
                    t
                };
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
    events.dedup_by(|b, a| {
        a.kind == b.kind && (b.t - a.t).abs() <= 8.0 * f64::EPSILON * a.t.abs().max(1.0)
    });
    if events.iter().all(|e| e.near) {
        return Ok(path
            .clip(c, &target, se)
            .into_iter()
            .map(|t| CurveSurfaceHit::Overlap { t })
            .collect());
    }
    // A closed curve's events are a cycle: start after a far one, so no run is cut in two.
    if path.closed
        && let Some(i) = events.iter().position(|e| !e.near)
    {
        events.rotate_left(i + 1);
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
        1 => roots.first().map(|e| (e.t, Contact::Crossing)),
        n => roots.get(n / 2).map(|e| (e.t, Contact::Tangent)),
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
    /// `t = m + 2 atanh(k (2u - 1))`, `k = tanh(w / 2)`, `m` and `w` the middle and half
    /// width of `[lo, hi]`; the ends are `lo` and `hi` to the bit. Kept as `k` and
    /// `c = 1 - k = 2 / (1 + e^w)`, so `1 +- k (2u - 1)` are sums of positive terms,
    /// `c + 2 k u` and `c + 2 k (1 - u)`, with no cancellation however near one `k` is.
    Hyper { lo: f64, hi: f64, k: f64, c: f64 },
}

impl Map {
    fn t(self, u: f64) -> f64 {
        match self {
            Self::Segment { lo, hi } => (1.0 - u) * lo + u * hi,
            Self::Half { t0 } => t0 + 2.0 * fmath::atan(2.0 * u - 1.0),
            // 2 atanh x = ln((1 + x) / (1 - x)), to a few ulps of `t`'s size; the root's
            // polish on the curve's own point refines it.
            Self::Hyper { lo, hi, k, c } => match u {
                0.0 => lo,
                1.0 => hi,
                _ => {
                    let r = (c + 2.0 * k * u) / (c + 2.0 * k * (1.0 - u));
                    (0.5 * (lo + hi) + fmath::ln(r)).clamp(lo, hi)
                }
            },
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
            Map::Half { .. } | Map::Hyper { .. } => {
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
    Line {
        origin: Point3,
        direction: Vec3,
    },
    Conic {
        centre: Point3,
        a: Vec3,
        b: Vec3,
    },
    /// `vertex + a s^2 + b s`.
    Parabola {
        vertex: Point3,
        a: Vec3,
        b: Vec3,
    },
    /// `centre + a cosh t + b sinh t`.
    Hyperbola {
        centre: Point3,
        a: Vec3,
        b: Vec3,
    },
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
            Curve::Parabola(p) => {
                let (f, sign) = (p.focal().mm(), p.sense().sign());
                PathKind::Parabola {
                    vertex: p.frame().origin(),
                    a: p.frame().x().as_vec() * f,
                    b: p.frame().y().as_vec() * (2.0 * f * sign),
                }
            }
            Curve::Hyperbola(h) => PathKind::Hyperbola {
                centre: h.frame().origin(),
                a: h.frame().x().as_vec() * h.semi_x().mm(),
                b: h.frame().y().as_vec() * (h.semi_y().mm() * h.sense().sign()),
            },
            _ => return None,
        };
        let closed = matches!(kind, PathKind::Conic { .. }) && ce.length() >= TAU;
        Some(Self {
            kind,
            extent: *ce,
            closed,
        })
    }

    /// The pieces in the target's axes: the segment, the two half turns, or the open
    /// conic's arc.
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
        let quad3 = |p: [f64; 3], m: [f64; 3], q: [f64; 3], w: f64, map| Piece {
            x: Bern::new(&[p[0], m[0] * w, q[0]]),
            y: Bern::new(&[p[1], m[1] * w, q[1]]),
            z: Bern::new(&[p[2], m[2] * w, q[2]]),
            h: Bern::new(&[1.0, w, 1.0]),
            map,
        };
        let local = |p: Point3| tuple(ax.local(p));
        let (lo, hi) = (self.extent.min(), self.extent.max());
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
                for (slot, (t0, s)) in out.iter_mut().zip([(0.0, 1.0), (PI, -1.0)]) {
                    let (a, b) = (a * s, b * s);
                    *slot = Some(quad(
                        (c - b).to_array(),
                        a.to_array(),
                        (c + b).to_array(),
                        Map::Half { t0 },
                    ));
                }
            }
            PathKind::Parabola { vertex, a, b } => {
                let at = |s: f64| vertex + a * (s * s) + b * s;
                let mid = vertex + a * (lo * hi) + b * (0.5 * (lo + hi));
                let map = Map::Segment { lo, hi };
                out[0] = Some(quad3(local(at(lo)), local(mid), local(at(hi)), 1.0, map));
            }
            PathKind::Hyperbola { centre, a, b } => {
                let at = |t: f64| {
                    let (sh, ch) = fmath::sinh_cosh(t);
                    centre + a * ch + b * sh
                };
                // The middle and half width; the tangents at the ends meet at `mid`.
                let (m, w) = (0.5 * (lo + hi), 0.5 * (hi - lo));
                let (sh, ch) = fmath::sinh_cosh(m);
                let cw = fmath::cosh(w);
                let mid = centre + (a * ch + b * sh) / cw;
                let map = Map::Hyper {
                    lo,
                    hi,
                    k: fmath::tanh(0.5 * w),
                    c: 2.0 / (1.0 + fmath::exp(w)),
                };
                out[0] = Some(quad3(local(at(lo)), local(mid), local(at(hi)), cw, map));
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
            PathKind::Parabola { vertex, a, b } => {
                (vertex + a * (t * t) + b * t, a * (2.0 * t) + b)
            }
            PathKind::Hyperbola { centre, a, b } => {
                let (sh, ch) = fmath::sinh_cosh(t);
                (centre + a * ch + b * sh, a * sh + b * ch)
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
            PathKind::Conic { .. } | PathKind::Hyperbola { .. } => 1e-6,
            PathKind::Parabola { .. } => 1e-6 * (1.0 + t.abs()),
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

    /// The pieces of an overlap whose uv lies in `se`, sorted by `t` (63 §4.14).
    ///
    /// The uv can leave `se` only where it meets one of `se`'s sides. Each side is a set
    /// the curve's point meets on a plane in the surface's axes: a plane's `x` or `y`; a
    /// height `z` for a cylinder's or cone's `v`, a sphere's latitude (`r sin`) or a torus's
    /// tube angle (the minor radius times its sine); and for an angle `u` the plane through
    /// the axis at that azimuth (both its halves). On a plane, the curve's signed distance
    /// is affine in a line's `t`, `alpha cos t + beta sin t + gamma` on a conic, quadratic in
    /// a parabola's `t` and `alpha cosh t + beta sinh t + gamma` on a hyperbola, so the
    /// meeting parameters are closed forms. Between neighbours the uv is wholly in or out
    /// of `se`, which the midpoint decides; neighbouring pieces inside merge, and on a
    /// closed curve the pieces at the turn's two ends join across it. A piece of zero
    /// length (a touch of a side from outside) is dropped.
    fn clip(&self, c: &Curve, target: &Target, se: &Extent2d) -> Vec<Extent1d> {
        let (lo, hi) = (self.extent.min(), self.extent.max());
        let end = if self.closed { lo + TAU } else { hi };
        let mut cuts: Vec<f64> = Vec::with_capacity(18);
        cuts.push(lo);
        cuts.push(end);
        for (normal, offset) in target.sides(se) {
            for t in self.meets(target, normal, offset).into_iter().flatten() {
                if let Some(t) = self.place(t).filter(|t| *t > lo && *t < end) {
                    cuts.push(t);
                }
            }
        }
        cuts.sort_by(f64::total_cmp);
        cuts.dedup();
        let mut pieces: Vec<(f64, f64)> = Vec::new();
        for w in cuts.windows(2) {
            let &[a, b] = w else { continue };
            if target.uv_in(c.point(0.5 * (a + b)), se).is_none() {
                continue;
            }
            match pieces.last_mut() {
                Some(last) if last.1 == a => last.1 = b,
                _ => pieces.push((a, b)),
            }
        }
        if pieces.as_slice() == [(lo, end)] {
            return vec![self.extent];
        }
        if self.closed
            && pieces.len() > 1
            && pieces.first().is_some_and(|p| p.0 == lo)
            && pieces.last().is_some_and(|p| p.1 == end)
            && let Some(first) = pieces.first().copied()
        {
            pieces.remove(0);
            if let Some(last) = pieces.last_mut() {
                last.1 = first.1 + TAU;
            }
        }
        pieces
            .into_iter()
            .filter_map(|(a, b)| Extent1d::new(a, b))
            .collect()
    }

    /// The parameters where the curve meets the plane `normal . l = offset` in the
    /// target's axes, unplaced: a line's one (none when parallel), a conic's two (none
    /// when it misses or lies in a parallel plane), a parabola's up to two (the roots of a
    /// quadratic in `t`) and a hyperbola's up to two (of a quadratic in `e^t`).
    fn meets(&self, target: &Target, normal: [f64; 3], offset: f64) -> [Option<f64>; 2] {
        let ax = &target.axes;
        let dot = |l: [f64; 3]| normal[0] * l[0] + normal[1] * l[1] + normal[2] * l[2];
        let along = |v: Vec3| dot([v.dot(ax.x), v.dot(ax.y), v.dot(ax.z)]);
        match self.kind {
            PathKind::Line { origin, direction } => {
                let (rate, at0) = (along(direction), dot(tuple(ax.local(origin))) - offset);
                let t = -at0 / rate;
                [t.is_finite().then_some(t), None]
            }
            PathKind::Conic { centre, a, b } => {
                let (alpha, beta) = (along(a), along(b));
                let gamma = dot(tuple(ax.local(centre))) - offset;
                let amp = fmath::hypot(alpha, beta);
                let cos = -gamma / amp;
                if !(amp > 0.0 && cos.abs() <= 1.0) {
                    return [None, None];
                }
                let (psi, half) = (fmath::atan2(beta, alpha), fmath::acos(cos));
                [Some(psi - half), Some(psi + half)]
            }
            PathKind::Parabola { vertex, a, b } => {
                let gamma = dot(tuple(ax.local(vertex))) - offset;
                quadratic_roots(along(a), along(b), gamma)
            }
            PathKind::Hyperbola { centre, a, b } => {
                // `alpha cosh t + beta sinh t + gamma` times `2 e^t`, in `u = e^t > 0`.
                let (alpha, beta) = (along(a), along(b));
                let gamma = dot(tuple(ax.local(centre))) - offset;
                quadratic_roots(alpha + beta, 2.0 * gamma, alpha - beta)
                    .map(|u| u.filter(|u| *u > 0.0).map(fmath::ln))
            }
        }
    }

    /// `t` placed in the extent (a conic's angle wrapped to the turn from `ce.min()`), or
    /// `None` outside it.
    fn place(&self, t: f64) -> Option<f64> {
        let (lo, hi) = (self.extent.min(), self.extent.max());
        let t = match self.kind {
            PathKind::Line { .. } | PathKind::Parabola { .. } | PathKind::Hyperbola { .. } => t,
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

/// The real roots of `a x^2 + b x + c` (one when `a` is zero; none when both are), by the
/// stable form that never subtracts nearly equal terms.
fn quadratic_roots(a: f64, b: f64, c: f64) -> [Option<f64>; 2] {
    let fin = |x: f64| x.is_finite().then_some(x);
    if a == 0.0 {
        return [fin(-c / b), None];
    }
    let disc = b * b - 4.0 * a * c;
    if disc.is_nan() || disc < 0.0 {
        return [None, None];
    }
    let q = -0.5 * (b + disc.sqrt().copysign(b));
    if q == 0.0 {
        return [Some(0.0), None];
    }
    [fin(q / a), fin(c / q)]
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
                Shape::Sphere { r: c.radius().mm() },
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
                    [
                        4.0 * s * x - 2.0 * b * x,
                        4.0 * s * y - 2.0 * b * y,
                        4.0 * s * z,
                    ],
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

    /// Planes in the axes, `normal . l = offset`, that hold every point on the surface
    /// where its uv meets a side of `se` (see [`Path::clip`]); a side at an infinite bound,
    /// or an angle range of a whole period or more, has none.
    fn sides(&self, se: &Extent2d) -> Vec<([f64; 3], f64)> {
        const Z: [f64; 3] = [0.0, 0.0, 1.0];
        let (lo, hi) = (se.min(), se.max());
        let mut out = Vec::with_capacity(4);
        // An angle direction's sides: the planes through the axis at their azimuths.
        let angles = |out: &mut Vec<_>, arcs: Arcs, period: f64| {
            if hi.x - lo.x < period {
                for u in [lo.x, hi.x] {
                    let phi = arcs.angle(u);
                    out.push(([-fmath::sin(phi), fmath::cos(phi), 0.0], 0.0));
                }
            }
        };
        match self.shape {
            Shape::Plane => {
                out.extend([([1.0, 0.0, 0.0], lo.x), ([1.0, 0.0, 0.0], hi.x)]);
                out.extend([([0.0, 1.0, 0.0], lo.y), ([0.0, 1.0, 0.0], hi.y)]);
            }
            Shape::Cylinder { r, arcs } => {
                angles(&mut out, arcs, 2.0 * PI * r);
                out.extend([(Z, lo.y), (Z, hi.y)]);
            }
            Shape::Cone { .. } => {
                angles(&mut out, TURN, 1.0);
                out.extend([(Z, lo.y), (Z, hi.y)]);
            }
            Shape::Sphere { r } => {
                angles(&mut out, TURN, 1.0);
                for v in [lo.y, hi.y] {
                    out.push((Z, r * fmath::sin(HALF.angle(v))));
                }
            }
            Shape::Torus { small, .. } => {
                angles(&mut out, TURN, 1.0);
                if hi.y - lo.y < 1.0 {
                    for v in [lo.y, hi.y] {
                        out.push((Z, small * fmath::sin(TURN.angle(v))));
                    }
                }
            }
        }
        out.retain(|(_, offset)| offset.is_finite());
        out
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

/// The most degree a polynomial here reaches: a conic (closed or open) against the torus is
/// 8, its turning points 9.
const MAX: usize = 9;

/// `C(n, k)` for `n <= MAX` (Pascal's triangle, exact).
const BINOMIAL: [[f64; MAX + 1]; MAX + 1] = [
    [1.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0],
    [1.0, 1.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0],
    [1.0, 2.0, 1.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0],
    [1.0, 3.0, 3.0, 1.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0],
    [1.0, 4.0, 6.0, 4.0, 1.0, 0.0, 0.0, 0.0, 0.0, 0.0],
    [1.0, 5.0, 10.0, 10.0, 5.0, 1.0, 0.0, 0.0, 0.0, 0.0],
    [1.0, 6.0, 15.0, 20.0, 15.0, 6.0, 1.0, 0.0, 0.0, 0.0],
    [1.0, 7.0, 21.0, 35.0, 35.0, 21.0, 7.0, 1.0, 0.0, 0.0],
    [1.0, 8.0, 28.0, 56.0, 70.0, 56.0, 28.0, 8.0, 1.0, 0.0],
    [1.0, 9.0, 36.0, 84.0, 126.0, 126.0, 84.0, 36.0, 9.0, 1.0],
];

/// `C(n, k)`, zero past the table.
fn binomial(n: usize, k: usize) -> f64 {
    BINOMIAL
        .get(n)
        .and_then(|row| row.get(k))
        .copied()
        .unwrap_or(0.0)
}

/// A polynomial of degree `n <= 9` on `[0, 1]` in Bernstein form.
#[derive(Clone, Copy, Debug)]
struct Bern {
    c: [f64; MAX + 1],
    n: usize,
}

impl Bern {
    fn new(c: &[f64]) -> Self {
        debug_assert!((1..=MAX + 1).contains(&c.len()));
        let mut out = [0.0; MAX + 1];
        for (o, x) in out.iter_mut().zip(c) {
            *o = *x;
        }
        Self {
            c: out,
            n: c.len().clamp(1, MAX + 1) - 1,
        }
    }

    fn coefficients(&self) -> &[f64] {
        self.c.get(..=self.n).unwrap_or(&self.c)
    }

    fn mul(&self, o: &Self) -> Self {
        let n = self.n + o.n;
        let mut c = [0.0; MAX + 1];
        for (i, a) in self.coefficients().iter().enumerate() {
            for (j, b) in o.coefficients().iter().enumerate() {
                if let Some(x) = c.get_mut(i + j) {
                    *x += binomial(self.n, i) * binomial(o.n, j) * a * b;
                }
            }
        }
        for (k, x) in c.iter_mut().enumerate().take(n + 1) {
            *x /= binomial(n, k);
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
        let k = self.coefficients();
        for (x, (lo, hi)) in c.iter_mut().zip(k.iter().zip(k.iter().skip(1))) {
            *x = self.n as f64 * (hi - lo);
        }
        Self { c, n: self.n - 1 }
    }
}
