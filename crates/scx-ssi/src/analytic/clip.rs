// Written for solidcorex. Apache-2.0.
//! Clipping a section curve to both extents (module docs of [`super`]): the extents as
//! regions of space with a tolerant membership test and linear sides, the crossings of a
//! curve with the sides, and the spans kept.

use core::f64::consts::{FRAC_PI_2, TAU};

use scx_curve::{Circle, Curve, Ellipse, Hyperbola, Line, Parabola};
use scx_num::extent1d::Extent1d;
use scx_num::extent2d::Extent2d;
use scx_num::fmath;
use scx_num::frame::{Frame, Sense};
use scx_num::units::Length;
use scx_num::vector2d::Point2;
use scx_num::vector3d::{Point3, Vec3};
use scx_surface::{Plane, Surface};

use super::{Contact, IntersectionCurve, IntersectionPoint};

/// A frame's origin and axes, z times the kind's sense (research 63 D14), as the kinds
/// place themselves.
#[derive(Clone, Copy, Debug)]
pub(super) struct Axes {
    pub(super) o: Point3,
    pub(super) x: Vec3,
    pub(super) y: Vec3,
    pub(super) z: Vec3,
}

impl Axes {
    pub(super) fn new(frame: &Frame, sense: Sense) -> Self {
        Self {
            o: frame.origin(),
            x: *frame.x(),
            y: *frame.y(),
            z: *frame.z() * sense.sign(),
        }
    }

    /// The coordinates of `p` in the axes.
    pub(super) fn local(&self, p: Point3) -> [f64; 3] {
        let d = p - self.o;
        [d.dot(self.x), d.dot(self.y), d.dot(self.z)]
    }

    /// The coordinates of the vector `v` in the axes.
    pub(super) fn local_vec(&self, v: Vec3) -> [f64; 3] {
        [v.dot(self.x), v.dot(self.y), v.dot(self.z)]
    }

    /// The vector of coordinates `l`.
    pub(super) fn vec(&self, [a, b, c]: [f64; 3]) -> Vec3 {
        self.x * a + self.y * b + self.z * c
    }

    /// The point of coordinates `l`.
    pub(super) fn point(&self, l: [f64; 3]) -> Point3 {
        self.o + self.vec(l)
    }
}

/// A side of an extent: the linear function `w . p + c` of the point, zero on the side.
#[derive(Clone, Copy, Debug)]
pub(super) struct Side {
    w: Vec3,
    c: f64,
}

impl Side {
    /// The side through `at` square to `w`.
    pub(super) fn through(w: Vec3, at: Point3) -> Self {
        Self {
            w,
            c: -w.dot(at.to_vec()),
        }
    }
}

/// A section curve before clipping, in its construction parameter.
#[derive(Clone, Copy, Debug)]
pub(super) enum Shape {
    /// `o + t d`, `d` unit.
    Line { o: Point3, d: Vec3 },
    /// `c + cos t a + sin t b`, `a` and `b` square; a circle when they are the same length.
    Conic {
        c: Point3,
        a: Vec3,
        b: Vec3,
        circle: bool,
    },
    /// `v + t^2 x + t y`, `x` square to `y` and `|y| = 2 |x|`: [`Parabola`]'s parameter.
    Parabola { v: Point3, x: Vec3, y: Vec3 },
    /// The branch `c + cosh t a + sinh t b`, `a` square to `b`.
    Hyperbola { c: Point3, a: Vec3, b: Vec3 },
}

impl Shape {
    fn at(&self, t: f64) -> Point3 {
        match *self {
            Self::Line { o, d } => o + d * t,
            Self::Conic { c, a, b, .. } => {
                let (s, co) = fmath::sin_cos(t);
                c + a * co + b * s
            }
            Self::Parabola { v, x, y } => v + x * (t * t) + y * t,
            Self::Hyperbola { c, a, b } => {
                let (sh, ch) = fmath::sinh_cosh(t);
                c + a * ch + b * sh
            }
        }
    }

    fn tangent(&self, t: f64) -> Vec3 {
        match *self {
            Self::Line { d, .. } => d,
            Self::Conic { a, b, .. } => {
                let (s, co) = fmath::sin_cos(t);
                b * co - a * s
            }
            Self::Parabola { x, y, .. } => x * (2.0 * t) + y,
            Self::Hyperbola { a, b, .. } => {
                let (sh, ch) = fmath::sinh_cosh(t);
                a * sh + b * ch
            }
        }
    }

    /// The crossings of the side with the curve, pushed onto `out`: on a conic in
    /// `[0, 2 pi)`. A side touching an open conic may be missed: the spans either side of
    /// a touch are both in or both out, and merge.
    fn crossings(&self, side: &Side, out: &mut Vec<f64>) {
        match *self {
            Self::Line { o, d } => {
                let slope = side.w.dot(d);
                if slope != 0.0 {
                    let t = -(side.w.dot(o.to_vec()) + side.c) / slope;
                    if t.is_finite() {
                        out.push(t);
                    }
                }
            }
            Self::Conic { c, a, b, .. } => {
                // alpha cos t + beta sin t + gamma = 0, i.e. cos(t - phi) = -gamma / r.
                let (alpha, beta) = (side.w.dot(a), side.w.dot(b));
                let gamma = side.w.dot(c.to_vec()) + side.c;
                let r = fmath::hypot(alpha, beta);
                if r == 0.0 {
                    return;
                }
                let q = -gamma / r;
                if q.is_nan() || q.abs() > 1.0 {
                    return;
                }
                let phi = fmath::atan2(beta, alpha);
                let off = fmath::acos(q);
                for t in [phi - off, phi + off] {
                    out.push(t.rem_euclid(TAU));
                }
            }
            Self::Parabola { v, x, y } => {
                let gamma = side.w.dot(v.to_vec()) + side.c;
                quadratic(side.w.dot(x), side.w.dot(y), gamma, out);
            }
            Self::Hyperbola { c, a, b } => {
                // alpha cosh t + beta sinh t + gamma = 0 is, times 2 e^t, a quadratic in
                // e^t; only its positive roots are crossings.
                let (alpha, beta) = (side.w.dot(a), side.w.dot(b));
                let gamma = side.w.dot(c.to_vec()) + side.c;
                let mut roots = Vec::with_capacity(2);
                quadratic(alpha + beta, 2.0 * gamma, alpha - beta, &mut roots);
                for x in roots {
                    if x > 0.0 {
                        let t = fmath::ln(x);
                        if t.is_finite() {
                            out.push(t);
                        }
                    }
                }
            }
        }
    }

    /// The same curve run the other way, `t -> -t`.
    fn reversed(self) -> Self {
        match self {
            Self::Line { o, d } => Self::Line { o, d: -d },
            Self::Conic { c, a, b, circle } => Self::Conic {
                c,
                a,
                b: -b,
                circle,
            },
            Self::Parabola { v, x, y } => Self::Parabola { v, x, y: -y },
            Self::Hyperbola { c, a, b } => Self::Hyperbola { c, a, b: -b },
        }
    }

    /// The curve kind, in the same parameter. `None` for degenerate data (not reached for
    /// the shapes the sections build).
    fn curve(&self) -> Option<Curve> {
        match *self {
            Self::Line { o, d } => Line::new(o, d).ok().map(Curve::Line),
            Self::Conic { c, a, b, circle } => {
                let frame = Frame::new(c, a.cross(b), a)?;
                let (la, lb) = (Length::from_mm(a.length()), Length::from_mm(b.length()));
                if circle {
                    Circle::new(frame, la, Sense::Forward)
                        .ok()
                        .map(Curve::Circle)
                } else {
                    Ellipse::new(frame, la, lb, Sense::Forward)
                        .ok()
                        .map(Curve::Ellipse)
                }
            }
            Self::Parabola { v, x, y } => {
                let frame = Frame::new(v, x.cross(y), x)?;
                Parabola::new(frame, Length::from_mm(x.length()), Sense::Forward)
                    .ok()
                    .map(Curve::Parabola)
            }
            Self::Hyperbola { c, a, b } => {
                let frame = Frame::new(c, a.cross(b), a)?;
                let (la, lb) = (Length::from_mm(a.length()), Length::from_mm(b.length()));
                Hyperbola::new(frame, la, lb, Sense::Forward)
                    .ok()
                    .map(Curve::Hyperbola)
            }
        }
    }
}

/// The finite real roots of `a t^2 + b t + c = 0` onto `out`, without cancellation
/// (Numerical Recipes 5.6); a double root once, none when the discriminant is negative.
fn quadratic(a: f64, b: f64, c: f64, out: &mut Vec<f64>) {
    let mut push = |t: f64| {
        if t.is_finite() {
            out.push(t);
        }
    };
    if a == 0.0 {
        if b != 0.0 {
            push(-c / b);
        }
        return;
    }
    let disc = b * b - 4.0 * a * c;
    if disc.is_nan() || disc < 0.0 {
        return;
    }
    let q = -0.5 * (b + disc.sqrt().copysign(b));
    if q == 0.0 {
        push(0.0);
    } else {
        push(q / a);
        push(c / q);
    }
}

/// A section curve with how the surfaces meet along it and any sides of its own (a torus
/// meridian's tube angle).
#[derive(Clone, Debug)]
pub(super) struct Raw {
    pub(super) shape: Shape,
    pub(super) contact: Contact,
    pub(super) sides: Vec<Side>,
}

impl Raw {
    pub(super) fn new(shape: Shape, contact: Contact) -> Self {
        Self {
            shape,
            contact,
            sides: Vec::new(),
        }
    }
}

/// A plane's extent: `[u0, u1] x [v0, v1]` in its frame's x and y (a plane's uv is STEP's).
#[derive(Clone, Copy, Debug)]
struct Rect {
    o: Point3,
    x: Vec3,
    y: Vec3,
    u: [f64; 2],
    v: [f64; 2],
}

impl Rect {
    fn new(frame: &Frame, e: &Extent2d) -> Self {
        Self {
            o: frame.origin(),
            x: *frame.x(),
            y: *frame.y(),
            u: [e.x_min(), e.x_max()],
            v: [e.y_min(), e.y_max()],
        }
    }

    fn uv(&self, p: Point3) -> (f64, f64) {
        let d = p - self.o;
        (d.dot(self.x), d.dot(self.y))
    }

    fn contains(&self, p: Point3, tol: f64) -> bool {
        let (u, v) = self.uv(p);
        within(u, self.u, tol) && within(v, self.v, tol)
    }

    fn sides(&self, out: &mut Vec<Side>) {
        let [u0, u1] = self.u;
        let [v0, v1] = self.v;
        out.extend([
            Side::through(self.x, self.o + self.x * u0),
            Side::through(self.x, self.o + self.x * u1),
            Side::through(self.y, self.o + self.y * v0),
            Side::through(self.y, self.o + self.y * v1),
        ]);
    }
}

/// An angle range at least this wide is a whole turn.
const FULL_TURN: f64 = TAU - 1e-12;

fn within(x: f64, [lo, hi]: [f64; 2], tol: f64) -> bool {
    x >= lo - tol && x <= hi + tol
}

/// Whether the angle `theta` is in `[a0, a1]` within `atol`, periodically.
fn angle_within(theta: f64, [a0, a1]: [f64; 2], atol: f64) -> bool {
    if a1 - a0 >= FULL_TURN {
        return true;
    }
    let t = a0 + (theta - a0).rem_euclid(TAU);
    t <= a1 + atol || t >= a0 + TAU - atol
}

/// A revolution kind's sizes, for the membership test.
#[derive(Clone, Copy, Debug)]
enum Revolved {
    Cylinder,
    Cone { r: f64, k: f64 },
    Sphere { r: f64 },
    Torus { big: f64, small: f64 },
}

/// The surface's extent as a region of space.
#[derive(Clone, Copy, Debug)]
enum Region {
    Plane(Rect),
    /// A revolution kind over STEP's `u` (the angle) in `u` and STEP's `v` in `v`.
    Revolved {
        axes: Axes,
        kind: Revolved,
        u: [f64; 2],
        v: [f64; 2],
    },
}

/// The STEP extent of a revolution kind over `e`, from its uv map (monotone in each
/// direction).
pub(super) fn step_bounds(
    to_step: impl Fn(Point2) -> Point2,
    e: &Extent2d,
) -> ([f64; 2], [f64; 2]) {
    let lo = to_step(e.min());
    let hi = to_step(e.max());
    ([lo.x, hi.x], [lo.y, hi.y])
}

impl Region {
    /// `None` for a NURBS surface or a kind this crate does not know.
    fn new(s: &Surface, e: &Extent2d) -> Option<Self> {
        Some(match s {
            Surface::Plane(p) => Self::Plane(Rect::new(p.frame(), e)),
            Surface::Cylinder(c) => {
                let (u, v) = step_bounds(|p| c.to_step(p), e);
                Self::Revolved {
                    axes: Axes::new(c.frame(), c.sense()),
                    kind: Revolved::Cylinder,
                    u,
                    v,
                }
            }
            Surface::Cone(c) => {
                let (u, v) = step_bounds(|p| c.to_step(p), e);
                Self::Revolved {
                    axes: Axes::new(c.frame(), c.sense()),
                    kind: Revolved::Cone {
                        r: c.radius().mm(),
                        k: fmath::tan(c.half_angle()),
                    },
                    u,
                    v,
                }
            }
            Surface::Sphere(c) => {
                let (u, v) = step_bounds(|p| c.to_step(p), e);
                Self::Revolved {
                    axes: Axes::new(c.frame(), c.sense()),
                    kind: Revolved::Sphere { r: c.radius().mm() },
                    u,
                    v,
                }
            }
            Surface::Torus(c) => {
                let (u, v) = step_bounds(|p| c.to_step(p), e);
                Self::Revolved {
                    axes: Axes::new(c.frame(), c.sense()),
                    kind: Revolved::Torus {
                        big: c.major().mm(),
                        small: c.minor().mm(),
                    },
                    u,
                    v,
                }
            }
            _ => return None,
        })
    }

    /// STEP's `(u, v)` of a point on the surface (the angles in `[0, 2 pi)`; 0 on the axis).
    fn step_of(&self, p: Point3) -> Point2 {
        match self {
            Self::Plane(r) => {
                let (u, v) = r.uv(p);
                Point2::new(u, v)
            }
            Self::Revolved { axes, kind, .. } => {
                let [x, y, z] = axes.local(p);
                let rho = fmath::hypot(x, y);
                let theta = fmath::atan2(y, x).rem_euclid(TAU);
                let v = match *kind {
                    Revolved::Cylinder | Revolved::Cone { .. } => z,
                    Revolved::Sphere { .. } => fmath::atan2(z, rho),
                    Revolved::Torus { big, .. } => fmath::atan2(z, rho - big).rem_euclid(TAU),
                };
                Point2::new(theta, v)
            }
        }
    }

    fn contains(&self, p: Point3, tol: f64) -> bool {
        match self {
            Self::Plane(r) => r.contains(p, tol),
            Self::Revolved { axes, kind, u, v } => {
                let [x, y, z] = axes.local(p);
                let rho = fmath::hypot(x, y);
                let around = rho <= tol || angle_within(fmath::atan2(y, x), *u, tol / rho);
                around
                    && match *kind {
                        Revolved::Cylinder => within(z, *v, tol),
                        Revolved::Cone { r, k } => within(z, *v, tol) && r + k * z >= -tol,
                        Revolved::Sphere { r } => within(fmath::atan2(z, rho), *v, tol / r),
                        Revolved::Torus { big, small } => {
                            angle_within(fmath::atan2(z, rho - big), *v, tol / small)
                        }
                    }
            }
        }
    }

    /// The sides of the extent every curve is cut by (a torus's tube-angle sides are the
    /// meridian's own, [`Raw::sides`]).
    fn sides(&self, out: &mut Vec<Side>) {
        match self {
            Self::Plane(r) => r.sides(out),
            Self::Revolved { axes, kind, u, v } => {
                let [u0, u1] = *u;
                if u1 - u0 < FULL_TURN {
                    for a in [u0, u1] {
                        let (s, c) = fmath::sin_cos(a);
                        out.push(Side::through(axes.y * c - axes.x * s, axes.o));
                    }
                }
                let height = |h: f64| Side::through(axes.z, axes.o + axes.z * h);
                match *kind {
                    Revolved::Cylinder => out.extend(v.map(height)),
                    Revolved::Cone { r, k } => {
                        out.extend(v.map(height));
                        out.push(height(-r / k));
                    }
                    Revolved::Sphere { r } => {
                        for lat in *v {
                            if lat.abs() < FRAC_PI_2 {
                                out.push(height(r * fmath::sin(lat)));
                            }
                        }
                    }
                    Revolved::Torus { .. } => {}
                }
            }
        }
    }

    /// Whether the extent covers the whole tube angle (a torus) or has no tube angle.
    pub(super) fn tube(&self) -> Option<[f64; 2]> {
        match self {
            Self::Revolved {
                kind: Revolved::Torus { .. },
                v,
                ..
            } if v[1] - v[0] < FULL_TURN => Some(*v),
            _ => None,
        }
    }
}

/// Both extents, the tolerance and the orientation wanted.
pub(super) struct Clip<'a> {
    surface: &'a Surface,
    region: Region,
    plane: Rect,
    n_plane: Vec3,
    tol: f64,
    sign: f64,
}

impl<'a> Clip<'a> {
    /// `None` for a NURBS surface.
    pub(super) fn new(
        surface: &'a Surface,
        se: &Extent2d,
        plane: &Plane,
        pe: &Extent2d,
        tol: f64,
        sign: f64,
    ) -> Option<Self> {
        Some(Self {
            surface,
            region: Region::new(surface, se)?,
            plane: Rect::new(plane.frame(), pe),
            n_plane: *plane.frame().z(),
            tol,
            sign,
        })
    }

    /// The torus extent's tube angle, when it is not a whole turn.
    pub(super) fn tube(&self) -> Option<[f64; 2]> {
        self.region.tube()
    }

    fn contains(&self, p: Point3) -> bool {
        self.plane.contains(p, self.tol) && self.region.contains(p, self.tol)
    }

    /// The surface's normal at its point `p`, `None` where it has none (a cone's apex).
    fn normal(&self, p: Point3) -> Option<Vec3> {
        let step = self.region.step_of(p);
        Some(Vec3::from(match self.surface {
            Surface::Plane(k) => k.normal(k.from_step(step)),
            Surface::Cylinder(k) => k.normal(k.from_step(step)),
            Surface::Cone(k) => k.normal(k.from_step(step))?,
            Surface::Sphere(k) => k.normal(k.from_step(step)),
            Surface::Torus(k) => k.normal(k.from_step(step)),
            _ => return None,
        }))
    }

    /// The pieces of `raws` inside both extents, onto `curves` and `points`. `None` when a
    /// curve kind refuses its data (not reached for the shapes the sections build).
    pub(super) fn clip(
        &self,
        raws: Vec<Raw>,
        curves: &mut Vec<IntersectionCurve>,
        points: &mut Vec<IntersectionPoint>,
    ) -> Option<()> {
        let mut shared = Vec::new();
        self.plane.sides(&mut shared);
        self.region.sides(&mut shared);
        for raw in raws {
            let mut cuts = Vec::new();
            for side in shared.iter().chain(&raw.sides) {
                raw.shape.crossings(side, &mut cuts);
            }
            let spans = self.spans(&raw.shape, cuts);
            let Some(first) = spans.first() else {
                continue;
            };
            let mut shape = raw.shape;
            let mut spans_in = spans.clone();
            if raw.contact == Contact::Transversal {
                let t = 0.5 * (first[0] + first[1]);
                if let Some(n) = self.normal(shape.at(t)) {
                    let want = n.cross(self.n_plane) * self.sign;
                    if shape.tangent(t).dot(want) < 0.0 {
                        shape = shape.reversed();
                        spans_in = spans.iter().map(|&s| flipped(&shape, s)).collect();
                    }
                }
            }
            let curve = shape.curve()?;
            for [t0, t1] in spans_in {
                let mid = shape.at(0.5 * (t0 + t1));
                let (p0, p1) = (shape.at(t0), shape.at(t1));
                let tol = self.tol;
                if p0.distance(mid) <= tol && p1.distance(mid) <= tol && p0.distance(p1) <= tol {
                    points.push(IntersectionPoint {
                        point: mid,
                        contact: raw.contact,
                    });
                } else {
                    curves.push(IntersectionCurve {
                        curve: curve.clone(),
                        span: Extent1d::new(t0, t1)?,
                        contact: raw.contact,
                    });
                }
            }
        }
        Some(())
    }

    /// Keeps `p` when it lies in both extents.
    pub(super) fn point(&self, p: Point3, contact: Contact, points: &mut Vec<IntersectionPoint>) {
        if self.contains(p) {
            points.push(IntersectionPoint { point: p, contact });
        }
    }

    /// The spans of the curve's parameter inside both extents, between the crossings
    /// `cuts`, neighbours merged.
    fn spans(&self, shape: &Shape, mut cuts: Vec<f64>) -> Vec<[f64; 2]> {
        cuts.sort_by(f64::total_cmp);
        cuts.dedup();
        let inside = |t0: f64, t1: f64| self.contains(shape.at(0.5 * (t0 + t1)));
        let mut out: Vec<[f64; 2]> = Vec::new();
        match shape {
            // An open curve is bounded by the plane's rectangle, so its pieces lie between
            // crossings.
            Shape::Line { .. } | Shape::Parabola { .. } | Shape::Hyperbola { .. } => {
                for w in cuts.windows(2) {
                    if let [t0, t1] = *w
                        && inside(t0, t1)
                    {
                        match out.last_mut() {
                            Some(last) if last[1] == t0 => last[1] = t1,
                            _ => out.push([t0, t1]),
                        }
                    }
                }
            }
            Shape::Conic { .. } => {
                let m = cuts.len();
                if m == 0 {
                    if inside(0.0, TAU) {
                        out.push([0.0, TAU]);
                    }
                    return out;
                }
                // T(q): the q-th crossing, unwrapped across turns.
                let at = |q: usize| cuts.get(q % m).copied().unwrap_or(0.0) + TAU * (q / m) as f64;
                let flags: Vec<bool> = (0..m).map(|i| inside(at(i), at(i + 1))).collect();
                let Some(start) = flags.iter().position(|&f| !f) else {
                    out.push([0.0, TAU]);
                    return out;
                };
                let mut run: Option<usize> = None;
                for q in start + 1..=start + m {
                    let f = flags.get(q % m).copied().unwrap_or(false);
                    match (f, run) {
                        (true, None) => run = Some(q),
                        (false, Some(r)) => {
                            out.push(normalised(at(r), at(q)));
                            run = None;
                        }
                        _ => {}
                    }
                }
                if let Some(r) = run {
                    out.push(normalised(at(r), at(start + m + 1)));
                }
            }
        }
        out
    }
}

/// The span `[t0, t1]` of a conic moved by whole turns to start in `[0, 2 pi)`.
fn normalised(t0: f64, t1: f64) -> [f64; 2] {
    let k = (t0 / TAU).floor();
    [t0 - TAU * k, t1 - TAU * k]
}

/// The span `[t0, t1]` in the reversed curve's parameter `-t` (on a conic moved to start in
/// `[0, 2 pi)`).
fn flipped(shape: &Shape, [t0, t1]: [f64; 2]) -> [f64; 2] {
    match shape {
        Shape::Line { .. } | Shape::Parabola { .. } | Shape::Hyperbola { .. } => [-t1, -t0],
        Shape::Conic { .. } => normalised(-t1, -t0),
    }
}
