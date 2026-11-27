// Written for solidcorex. Apache-2.0.
//! The sphere pairs and the coaxial pairs (research 63 SS3; the table in [`super`]).
//!
//! Two surfaces of revolution on one axis meet in circles about it: each surface is its
//! meridian, a curve in the half-plane `(rho, h)` of the distance from the axis and the
//! height along it, and the circles stand on the points where the meridians meet. Every
//! meridian here is a line or a circle: a cylinder's is the line `rho = r`, a cone's the
//! ray `rho = r + k h` from its apex (its one nappe), a sphere's a circle about a point of
//! the axis and a torus's a circle about `(R, h0)`. So every pair is a line or a circle
//! against a line or a circle in the plane, each solved by distances: two circles by their
//! centres' distance (the radical line, the touch where the gaps close), a line and a
//! circle by the centre's distance from the line, two lines by their crossing. Two spheres
//! are always on one axis, the line through their centres.
//!
//! A pair is on one axis when the second's axis, moved onto the first's, moves no point of
//! its extent by more than the tolerance: that move's bound `delta` (the axis's offset and
//! tilt over the extent's reach) is the snap's slack. Projecting onto the meridian plane
//! moves no two points apart, so meridians more than `tol + delta` apart are surfaces more
//! than `tol` apart: that is the only `Empty` here, besides the extents' heights apart.
//! Between `tol` and `tol + delta` the answer is [`Closed::General`].

use scx_num::extent2d::Extent2d;
use scx_num::fmath;
use scx_num::frame::Frame;
use scx_num::units::Length;
use scx_num::vector2d::{Point2, Vec2};
use scx_num::vector3d::{Point3, Vec3};
use scx_surface::{Closed, Surface};

use super::Contact::{self, Tangent, Transversal};
use super::Intersection;
use super::clip::{Axes, Clip, Part, Raw, Shape, step_bounds};
use super::overlap::{Map1, Span1, apart_angle, rectangles, spans};

/// A surface of revolution's form, in its own axes (z times the sense).
#[derive(Clone, Copy, Debug)]
enum Form {
    Cylinder { r: f64 },
    Cone { r: f64, k: f64 },
    Sphere { r: f64 },
    Torus { big: f64, small: f64 },
}

/// A surface of revolution over its extent.
#[derive(Clone, Copy, Debug)]
struct Revolution {
    axes: Axes,
    form: Form,
    /// The extent's heights along its own axis (a sphere's and a torus's whole heights:
    /// wider than the extent's, never narrower).
    heights: [f64; 2],
    /// The extent's angles about its own axis (STEP's `u`, from the frame's x to its y).
    around: [f64; 2],
    /// The extent's STEP `v`: a height, a latitude or a tube angle.
    along: [f64; 2],
    /// The farthest the extent reaches along and off its axis together (`max |h|` plus
    /// `max rho`): a tilt `w` of the axis moves no point by more than `w` times it.
    reach: f64,
}

impl Revolution {
    /// `None` for a plane or a NURBS surface.
    fn of(s: &Surface, e: &Extent2d) -> Option<Self> {
        let far = |[v0, v1]: [f64; 2]| v0.abs().max(v1.abs());
        Some(match s {
            Surface::Cylinder(k) => {
                let (u, v) = step_bounds(|p| k.to_step(p), e);
                let r = k.radius().mm();
                Self {
                    axes: Axes::new(k.frame(), k.sense()),
                    form: Form::Cylinder { r },
                    heights: v,
                    around: u,
                    along: v,
                    reach: far(v) + r,
                }
            }
            Surface::Cone(c) => {
                let (u, v) = step_bounds(|p| c.to_step(p), e);
                let (r, k) = (c.radius().mm(), fmath::tan(c.half_angle()));
                Self {
                    axes: Axes::new(c.frame(), c.sense()),
                    form: Form::Cone { r, k },
                    heights: v,
                    around: u,
                    along: v,
                    reach: far(v) + far(v.map(|h| r + k * h)),
                }
            }
            Surface::Sphere(k) => {
                let (u, v) = step_bounds(|p| k.to_step(p), e);
                let r = k.radius().mm();
                Self {
                    axes: Axes::new(k.frame(), k.sense()),
                    form: Form::Sphere { r },
                    // The axis may not be the sphere's own: its heights there are its whole.
                    heights: [-r, r],
                    around: u,
                    along: v,
                    reach: 2.0 * r,
                }
            }
            Surface::Torus(k) => {
                let (u, v) = step_bounds(|p| k.to_step(p), e);
                let (big, small) = (k.major().mm(), k.minor().mm());
                Self {
                    axes: Axes::new(k.frame(), k.sense()),
                    form: Form::Torus { big, small },
                    heights: [-small, small],
                    around: u,
                    along: v,
                    reach: big + 2.0 * small,
                }
            }
            _ => return None,
        })
    }

    /// Its meridian in an axis's half-plane, its own axis at height `h0` there and running
    /// `dir` (`+-1`) times that axis.
    fn meridian(&self, h0: f64, dir: f64) -> Meridian {
        let [t0, t1] = self.heights.map(|t| h0 + dir * t);
        let heights = [t0.min(t1), t0.max(t1)];
        match self.form {
            Form::Cylinder { r } => Meridian::Line {
                p: Point2::new(r, h0),
                u: Vec2::new(0.0, 1.0),
                apex: None,
                heights,
            },
            Form::Cone { r, k } => {
                // rho = r + k dir (h - h0), run towards rho growing.
                let ks = k * dir;
                let n = fmath::hypot(1.0, ks);
                Meridian::Line {
                    p: Point2::new(r, h0),
                    u: Vec2::new(ks.abs() / n, ks.signum() / n),
                    apex: Some(Point2::new(0.0, h0 - r / ks)),
                    heights,
                }
            }
            Form::Sphere { r } => Meridian::Circle {
                c: Point2::new(0.0, h0),
                r,
            },
            Form::Torus { big, small } => Meridian::Circle {
                c: Point2::new(big, h0),
                r: small,
            },
        }
    }
}

/// A meridian in the half-plane `(rho, h)`, a line with its extent's heights.
#[derive(Clone, Copy, Debug)]
enum Meridian {
    /// The line through `p` along the unit `u` (`rho` growing along it); a cone's is the
    /// ray from its apex.
    Line {
        p: Point2,
        u: Vec2,
        apex: Option<Point2>,
        heights: [f64; 2],
    },
    /// The circle about `c` of radius `r`.
    Circle { c: Point2, r: f64 },
}

/// Where two meridians meet.
enum Meet {
    /// They are one, within the tolerance.
    Coincident,
    /// At these points of the half-plane (none: apart by more than the slack).
    Points(Vec<(Point2, Contact)>),
}

fn cross(a: Vec2, b: Vec2) -> f64 {
    a.x * b.y - a.y * b.x
}

/// The meeting of two meridians within `tol`; `None` for an answer between `tol` and
/// `slack` (module docs).
fn meet(m1: &Meridian, m2: &Meridian, tol: f64, slack: f64) -> Option<Meet> {
    match (*m1, *m2) {
        (
            Meridian::Line {
                p: p1,
                u: u1,
                apex: a1,
                heights: h1,
            },
            Meridian::Line {
                p: p2,
                u: u2,
                apex: a2,
                heights: h2,
            },
        ) => lines([p1, p2], [u1, u2], [a1, a2], [h1, h2], tol, slack),
        (Meridian::Line { p, u, apex, .. }, Meridian::Circle { c, r, .. })
        | (Meridian::Circle { c, r, .. }, Meridian::Line { p, u, apex, .. }) => {
            line_circle(p, u, apex, c, r, tol, slack)
        }
        (Meridian::Circle { c: c1, r: r1, .. }, Meridian::Circle { c: c2, r: r2, .. }) => {
            circles(c1, r1, c2, r2, tol, slack)
        }
    }
}

/// Two lines: one over the heights both extents share, or crossing at a point.
fn lines(
    [p1, p2]: [Point2; 2],
    [u1, u2]: [Vec2; 2],
    [a1, a2]: [Option<Point2>; 2],
    [h1, h2]: [[f64; 2]; 2],
    tol: f64,
    slack: f64,
) -> Option<Meet> {
    // The heights both extents share (a point when they only touch within the tolerance).
    let (lo, hi) = (h1[0].max(h2[0]), h1[1].min(h2[1]));
    if lo - hi > slack {
        return Some(Meet::Points(Vec::new()));
    }
    // Each line's rho at a height (no line here is level: its u has a height).
    let rho = |p: Point2, u: Vec2, h: f64| p.x + u.x * (h - p.y) / u.y;
    let gap = |h: f64| (rho(p1, u1, h) - rho(p2, u2, h)).abs();
    let one = gap(lo.min(hi)) <= tol && gap(lo.max(hi)) <= tol;
    let c = cross(u1, u2);
    let x = (c != 0.0).then(|| p1 + u1 * (cross(p2 - p1, u2) / c));
    // One line over the shared heights; when those are a point, unless the lines cross
    // there (two cones' nappes meeting at their apexes).
    let crossing_there = |x: Point2| x.y >= lo.min(hi) - tol && x.y <= lo.max(hi) + tol;
    if one && (hi - lo > tol || !x.is_some_and(crossing_there)) {
        return Some(Meet::Coincident);
    }
    let Some(x) = x else {
        // Parallel and apart: by the distance across them.
        return (cross(p2 - p1, u1).abs() > slack).then(|| Meet::Points(Vec::new()));
    };
    if x.x >= -tol {
        return Some(Meet::Points(vec![(x, Transversal)]));
    }
    // Crossing behind both apexes (two cones): the rays diverge from there, so they come
    // nearest at an apex.
    let from = |a: Option<Point2>, p: Point2, u: Vec2| a.map(|a| cross(a - p, u).abs());
    let near = from(a1, p2, u2)
        .into_iter()
        .chain(from(a2, p1, u1))
        .fold(f64::INFINITY, f64::min);
    (near > slack).then(|| Meet::Points(Vec::new()))
}

/// A line and a circle: two crossings, a touch, or none.
fn line_circle(
    p: Point2,
    u: Vec2,
    apex: Option<Point2>,
    c: Point2,
    r: f64,
    tol: f64,
    slack: f64,
) -> Option<Meet> {
    let t0 = (c - p).dot(u);
    let foot = p + u * t0;
    let dist = cross(u, c - p).abs();
    let mut found = Vec::new();
    if dist > r {
        let g = dist - r;
        if g > slack {
            return Some(Meet::Points(Vec::new()));
        }
        if g > tol {
            return None;
        }
        found.push((foot, Tangent));
    } else if r - dist <= tol {
        // The line no deeper than the tolerance into the circle: one touch.
        found.push((foot, Tangent));
    } else {
        let half = ((r - dist) * (r + dist)).sqrt();
        found.push((p + u * (t0 - half), Transversal));
        found.push((p + u * (t0 + half), Transversal));
    }
    found.retain(|(q, _)| q.x >= -tol);
    if !found.is_empty() {
        return Some(Meet::Points(found));
    }
    // Every crossing on the cone's far side of its apex: the ray's nearest point to the
    // centre is past the circle, or this is no answer.
    let a = apex?;
    let t = (c - a).dot(u).max(0.0);
    let near = (c - (a + u * t)).length() - r;
    (near > slack).then(|| Meet::Points(Vec::new()))
}

/// Two circles: coincident, apart, a touch, or two crossings on their radical line (the
/// half-chord by Heron's product of the gaps, after the pin's WR-79).
fn circles(c1: Point2, r1: f64, c2: Point2, r2: f64, tol: f64, slack: f64) -> Option<Meet> {
    let v = c2 - c1;
    let d = v.length();
    let dr = (r1 - r2).abs();
    if d + dr <= tol {
        return Some(Meet::Coincident);
    }
    let none = Some(Meet::Points(Vec::new()));
    if d == 0.0 {
        return if dr > slack { none } else { None };
    }
    let u = v / d;
    let at = |s: f64| c1 + u * s;
    let touch = |s: f64| Some(Meet::Points(vec![(at(s), Tangent)]));
    let out = r1 + r2 - d;
    let inn = d - dr;
    if out < 0.0 {
        // Apart: the touch halfway across the gap.
        return if -out > slack {
            none
        } else if -out <= tol {
            touch(0.5 * (d + r1 - r2))
        } else {
            None
        };
    }
    if inn < 0.0 {
        // One inside the other: the touch on the far side of the smaller from the larger's
        // centre.
        return if -inn > slack {
            none
        } else if -inn <= tol {
            touch(if r1 >= r2 {
                0.5 * (r1 + d + r2)
            } else {
                0.5 * (d - r2 - r1)
            })
        } else {
            None
        };
    }
    // Crossing: the lens between them no thicker than the tolerance (`out` across the
    // overlap, `inn` across the gap of one nearly inside the other) is one touch.
    if out <= inn && out <= tol {
        return touch(0.5 * (d + r1 - r2));
    }
    if inn <= tol {
        return touch(if r1 >= r2 {
            0.5 * (r1 + d + r2)
        } else {
            0.5 * (d - r2 - r1)
        });
    }
    let x = 0.5 * (d + (r1 - r2) * (r1 + r2) / d);
    let half = (out * (d + r1 + r2) * (d - r1 + r2) * (d + r1 - r2)).sqrt() / (2.0 * d);
    let w = Vec2::new(-u.y, u.x) * half;
    let mut found = vec![(at(x) - w, Transversal), (at(x) + w, Transversal)];
    // Two spheres' meridians are mirror images across the axis: one circle, not two.
    found.retain(|(q, _)| q.x >= -tol);
    Some(Meet::Points(found))
}

/// How `b`'s angle about `a`'s axis maps: `b`'s `u` at `a`'s `x` is `turn (x - phi)`, `phi`
/// the angle of `b`'s x in `a`'s frame and `turn` the sign of their frames' z against each
/// other (STEP's `u` runs from a frame's x to its y).
fn around(a: &Revolution, b: &Revolution) -> Map1 {
    let (xa, ya, xb, yb) = (a.axes.x, a.axes.y, b.axes.x, b.axes.y);
    Map1 {
        s: xb.cross(yb).dot(xa.cross(ya)).signum(),
        shift: fmath::atan2(xb.dot(ya), xb.dot(xa)),
        turns: true,
    }
}

/// The overlap of two coincident patches from the pieces of their `v` (`vs`, `along` its
/// map) and the distances from the axis those keep (`rho`: least and most), the angle
/// clipped with `slack` (module docs of [`super::overlap`]).
fn overlap(
    pair: [(&Surface, &Revolution); 2],
    vs: Option<Vec<Span1>>,
    rho: impl Fn(&[Span1]) -> (f64, f64),
    tol: f64,
    slack: f64,
) -> Closed<Intersection> {
    let [(a, ra), (b, rb)] = pair;
    let Some(vs) = vs else {
        return Closed::General;
    };
    if vs.is_empty() {
        return Closed::Solved(Intersection::Empty);
    }
    let (rho_min, rho_max) = rho(&vs);
    let touch = if rho_max > 0.0 {
        tol / rho_max
    } else {
        f64::INFINITY
    };
    let Some(us) = spans(
        ra.around,
        rb.around,
        around(ra, rb),
        touch,
        apart_angle(rho_min, slack),
    ) else {
        return Closed::General;
    };
    if us.is_empty() {
        return Closed::Solved(Intersection::Empty);
    }
    match rectangles(a, b, &us, &vs) {
        Some(pieces) => Closed::Solved(Intersection::Coincident(pieces)),
        None => Closed::General,
    }
}

/// Two spheres one within the tolerance (centres `d` apart): their overlap when their axes
/// agree within it, else `General` (the map between their uv is a rotation, and the
/// overlap of two latitude and longitude patches is no rectangle of either's).
fn spheres(
    a: &Surface,
    b: &Surface,
    ra: &Revolution,
    rb: &Revolution,
    d: f64,
    tol: f64,
) -> Closed<Intersection> {
    let Form::Sphere { r } = ra.form else {
        return Closed::General;
    };
    let dir = rb.axes.z.dot(ra.axes.z).signum();
    let delta = d + (rb.axes.z - ra.axes.z * dir).length() * r;
    if delta > tol {
        return Closed::General;
    }
    let slack = tol + delta;
    let lat = Map1 {
        s: dir,
        shift: 0.0,
        turns: false,
    };
    let vs = spans(ra.along, rb.along, lat, tol / r, slack / r);
    let rho = |vs: &[Span1]| {
        let (mut lo, mut hi) = (f64::INFINITY, 0.0_f64);
        for v in vs {
            let [v0, v1] = v.a;
            let far = v0.abs().max(v1.abs());
            let near = if v0 <= 0.0 && v1 >= 0.0 {
                0.0
            } else {
                v0.abs().min(v1.abs())
            };
            lo = lo.min(r * fmath::cos(far));
            hi = hi.max(r * fmath::cos(near));
        }
        (lo.max(0.0), hi)
    };
    overlap([(a, ra), (b, rb)], vs, rho, tol, slack)
}

/// Two surfaces of revolution on `a`'s axis with one meridian (a cylinder or cone pair, or
/// two tori), `b`'s snap `delta`: their overlap.
fn revolved(
    a: &Surface,
    b: &Surface,
    ra: &Revolution,
    rb: &Revolution,
    tol: f64,
    delta: f64,
) -> Closed<Intersection> {
    let slack = tol + delta;
    let dir = rb.axes.z.dot(ra.axes.z).signum();
    match (ra.form, rb.form) {
        (Form::Torus { big, small }, Form::Torus { .. }) => {
            let tube = Map1 {
                s: dir,
                shift: 0.0,
                turns: true,
            };
            let vs = spans(ra.along, rb.along, tube, tol / small, slack / small);
            let rho = |_: &[Span1]| ((big - small).max(0.0), big + small);
            overlap([(a, ra), (b, rb)], vs, rho, tol, slack)
        }
        (Form::Cylinder { .. } | Form::Cone { .. }, Form::Cylinder { .. } | Form::Cone { .. }) => {
            let (h, _) = on_axis(&ra.axes, rb.axes.o);
            let height = Map1 {
                s: dir,
                shift: h,
                turns: false,
            };
            let vs = spans(ra.along, rb.along, height, tol, slack);
            let at = |v: f64| match ra.form {
                Form::Cone { r, k } => (r + k * v).abs(),
                Form::Cylinder { r } | Form::Sphere { r } => r,
                Form::Torus { big, .. } => big,
            };
            let rho = |vs: &[Span1]| {
                let (mut lo, mut hi) = (f64::INFINITY, 0.0_f64);
                for v in vs {
                    for x in v.a {
                        lo = lo.min(at(x));
                        hi = hi.max(at(x));
                    }
                }
                (lo, hi)
            };
            overlap([(a, ra), (b, rb)], vs, rho, tol, slack)
        }
        _ => Closed::General,
    }
}

/// An axis in space: its point, a frame square to it, and its direction.
fn axis_through(o: Point3, z: Vec3) -> Option<Axes> {
    // The world axis least along z, as the frame's x hint: fixed for a given input.
    let hint = if z.x.abs() <= z.y.abs() && z.x.abs() <= z.z.abs() {
        Vec3::new(1.0, 0.0, 0.0)
    } else if z.y.abs() <= z.z.abs() {
        Vec3::new(0.0, 1.0, 0.0)
    } else {
        Vec3::new(0.0, 0.0, 1.0)
    };
    let f = Frame::new(o, z, hint)?;
    Some(Axes {
        o,
        x: *f.x(),
        y: *f.y(),
        z: *f.z(),
    })
}

/// The shared axis and both meridians on it, with the snap's slack; `None` when the pair
/// is not on one axis within the tolerance (or two concentric spheres, `Some` with no
/// axis).
struct OnAxis {
    axis: Axes,
    meridians: [Meridian; 2],
    delta: f64,
}

/// Where a sphere's centre sits on an axis: its height and its distance off.
fn on_axis(axis: &Axes, c: Point3) -> (f64, f64) {
    let d = c - axis.o;
    let h = d.dot(axis.z);
    (h, (d - axis.z * h).length())
}

/// Puts `a` and `b` on one axis (module docs).
fn shared(a: &Revolution, b: &Revolution, tol: f64) -> Option<OnAxis> {
    let sphere = |s: &Revolution| matches!(s.form, Form::Sphere { .. });
    let (axis, ma, mb, delta) = match (sphere(a), sphere(b)) {
        (true, true) => {
            let v = b.axes.o - a.axes.o;
            let d = v.length();
            let axis = axis_through(a.axes.o, *v.normalized()?)?;
            (axis, a.meridian(0.0, 1.0), b.meridian(d, 1.0), 0.0)
        }
        (true, false) => {
            let (h, off) = on_axis(&b.axes, a.axes.o);
            (b.axes, a.meridian(h, 1.0), b.meridian(0.0, 1.0), off)
        }
        (false, true) => {
            let (h, off) = on_axis(&a.axes, b.axes.o);
            (a.axes, a.meridian(0.0, 1.0), b.meridian(h, 1.0), off)
        }
        (false, false) => {
            let along = b.axes.z.dot(a.axes.z);
            if along.abs() < 0.5 {
                return None;
            }
            let dir = along.signum();
            let (h, off) = on_axis(&a.axes, b.axes.o);
            let tilt = (b.axes.z - a.axes.z * dir).length();
            let delta = off + tilt * b.reach;
            (a.axes, a.meridian(0.0, 1.0), b.meridian(h, dir), delta)
        }
    };
    (delta <= tol).then_some(OnAxis {
        axis,
        meridians: [ma, mb],
        delta,
    })
}

/// Two spheres, or two surfaces of revolution on one axis, within `tol` over their
/// extents; curves along `n_a x n_b`. [`Closed::General`] for any other pair of curved
/// kinds and between `tol` and the snap's slack.
pub(super) fn coaxial(
    a: &Surface,
    ae: &Extent2d,
    b: &Surface,
    be: &Extent2d,
    tol: Length,
) -> Closed<Intersection> {
    let tol = tol.mm();
    let (Some(pa), Some(pb), Some(ra), Some(rb)) = (
        Part::of(a, ae),
        Part::of(b, be),
        Revolution::of(a, ae),
        Revolution::of(b, be),
    ) else {
        return Closed::General;
    };
    if let (Form::Sphere { r: r1 }, Form::Sphere { r: r2 }) = (ra.form, rb.form) {
        let d = (rb.axes.o - ra.axes.o).length();
        if d + (r1 - r2).abs() <= tol {
            // One sphere, within the tolerance.
            return spheres(a, b, &ra, &rb, d, tol);
        }
        if d == 0.0 {
            // Concentric, of radii apart by more.
            return Closed::Solved(Intersection::Empty);
        }
    }
    let Some(on) = shared(&ra, &rb, tol) else {
        return Closed::General;
    };
    let [ma, mb] = on.meridians;
    let Some(found) = meet(&ma, &mb, tol, tol + on.delta) else {
        return Closed::General;
    };
    let points = match found {
        Meet::Coincident => return revolved(a, b, &ra, &rb, tol, on.delta),
        Meet::Points(points) => points,
    };
    let axis = on.axis;
    let clip = Clip::new(pa, pb, tol);
    let mut raws = Vec::new();
    let mut touches = Vec::new();
    for (q, contact) in points {
        let c = axis.o + axis.z * q.y;
        if q.x <= tol {
            touches.push(c);
        } else {
            let shape = Shape::Conic {
                c,
                a: axis.x * q.x,
                b: axis.y * q.x,
                circle: true,
            };
            raws.push(Raw::new(shape, contact));
        }
    }
    let (mut curves, mut out) = (Vec::new(), Vec::new());
    if clip.clip(raws, &mut curves, &mut out).is_none() {
        return Closed::General;
    }
    for p in touches {
        clip.point(p, Tangent, &mut out);
    }
    Closed::Solved(Intersection::pieces(curves, out))
}
