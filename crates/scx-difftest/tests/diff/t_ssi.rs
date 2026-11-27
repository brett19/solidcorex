// Written for solidcorex. Apache-2.0.
//! scx-ssi's plane sections (`analytic::section_by_plane`, research 63 SS5) against the
//! pin's `Surface::intersect_with_plane`, on the same surface, plane and extents.
//!
//! The surfaces are `t_surface`'s (the pin's canonical kinds with their NURB made, the
//! cylinders and cones bounded over a full turn and their heights), each over its whole
//! domain; the plane is the pin's `Plane::new` over a square of its uv about its origin,
//! scx's plane over the same square. Both sides take the pin's default tolerance,
//! `EFF_ZERO_SQRT`. Only the forward sense is compared: the pin's kinds have none.
//!
//! The answers are compared as fingerprints, not curve by curve: whether the pair is left
//! to the general intersector (`general`: scx's `Closed::General`, the pin's
//! `needs_general`), empty, two coincident planes (`coincident`), or its pieces, each a
//! class and contact with its numbers:
//! - a line: its two ends, in a fixed order;
//! - a whole circle or ellipse (`conic`): its centre and its two radii, smaller first;
//! - a part of one (`arc`): those and its two ends;
//! - a point (scx's isolated point, the pin's `Touching` curve): the point.
//!
//! The pieces are in a fixed order (by a weighted sum of their numbers), so the order of
//! construction is not compared. The pin's `Crossing` is scx's `Transversal`.

use crate::harness::{Gen, SplitMix64, Tol, run_defect_tol};
use crate::t_surface::Kind;

use scx_num::frame::Frame;
use scx_num::hash::Rng;
use scx_num::vector3d::{Point3, Vec3};

/// Floats agree to `1e-9` relative or absolute: the answers are closed forms of the same
/// data, of size 1 to 30, but each side rounds them its own way (scx from the apex for a
/// cone, the pin from its bottom circle).
pub const TOL: Tol = Tol {
    rel: 1e-9,
    abs: 1e-9,
};

/// The bounded cylinders' and cones' height (`t_surface`'s).
const HEIGHT: f64 = 6.0;
/// The tolerance both sides take: the pin's default, `EFF_ZERO_SQRT`.
pub const SNAP: f64 = scx_num::math::EFF_ZERO_SQRT;

/// One input: a kind in a frame (origin, z, x), a plane (origin, z, x) over the square
/// `[-half, half]^2` of its uv, and what the pool aimed at (for the report only).
#[derive(Clone, Debug)]
pub struct Case {
    pub kind: Kind,
    pub frame: [[f64; 3]; 3],
    pub plane: [[f64; 3]; 3],
    pub half: f64,
    #[expect(dead_code, reason = "read through `Debug`, in the report")]
    pub aim: &'static str,
}

fn frame_of(f: [[f64; 3]; 3]) -> Frame {
    let [o, z, x] = f;
    Frame::new(
        Point3::new(o[0], o[1], o[2]),
        Vec3::new(z[0], z[1], z[2]),
        Vec3::new(x[0], x[1], x[2]),
    )
    .unwrap()
}

impl Case {
    pub fn surface_frame(&self) -> Frame {
        frame_of(self.frame)
    }

    pub fn plane_frame(&self) -> Frame {
        frame_of(self.plane)
    }
}

/// A unit vector from the generator.
fn unit(rng: &mut SplitMix64) -> [f64; 3] {
    loop {
        let v = [
            rng.uniform(-1.0, 1.0),
            rng.uniform(-1.0, 1.0),
            rng.uniform(-1.0, 1.0),
        ];
        let l = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
        if l > 0.1 && l <= 1.0 {
            return [v[0] / l, v[1] / l, v[2] / l];
        }
    }
}

/// Any direction square to `n`.
fn across(n: [f64; 3]) -> [f64; 3] {
    let [a, b, c] = n.map(f64::abs);
    let e = if a <= b && a <= c {
        [1.0, 0.0, 0.0]
    } else if b <= c {
        [0.0, 1.0, 0.0]
    } else {
        [0.0, 0.0, 1.0]
    };
    [
        n[1] * e[2] - n[2] * e[1],
        n[2] * e[0] - n[0] * e[2],
        n[0] * e[1] - n[1] * e[0],
    ]
}

/// The plane through the local point `at` square to the local direction `n`, in the world.
fn plane(f: &Frame, at: [f64; 3], n: [f64; 3]) -> [[f64; 3]; 3] {
    let (o, x, y, z) = (f.origin(), *f.x(), *f.y(), *f.z());
    let world = |v: [f64; 3]| x * v[0] + y * v[1] + z * v[2];
    let p = o + world(at);
    let nz = world(n);
    let nx = world(across(n));
    [[p.x, p.y, p.z], [nz.x, nz.y, nz.z], [nx.x, nx.y, nx.z]]
}

/// A direction at `angle` from the axis, about it at `turn`.
fn tilted(angle: f64, turn: f64) -> [f64; 3] {
    let s = scx_num::fmath::sin(angle);
    [
        s * scx_num::fmath::cos(turn),
        s * scx_num::fmath::sin(turn),
        scx_num::fmath::cos(angle),
    ]
}

fn kind(rng: &mut SplitMix64, i: usize) -> Kind {
    match i % 5 {
        0 => Kind::Plane,
        1 => Kind::Cylinder {
            r: rng.uniform(0.5, 5.0),
        },
        2 => {
            let r = rng.uniform(0.5, 5.0);
            let a = rng.uniform(0.1, 1.3) * if rng.below(2) == 0 { 1.0 } else { -1.0 };
            let k = scx_num::fmath::tan(a);
            let h = if r + k * HEIGHT > 0.2 {
                HEIGHT
            } else {
                0.8 * r / -k
            };
            Kind::Cone { r, a, h }
        }
        3 => Kind::Sphere {
            r: rng.uniform(0.5, 5.0),
        },
        _ => {
            let big = rng.uniform(2.0, 5.0);
            Kind::Torus {
                big,
                small: big * rng.uniform(0.1, 0.9),
            }
        }
    }
}

/// A plane aimed at one of the kind's cases, in local coordinates (a point, a normal).
fn aimed(rng: &mut SplitMix64, k: Kind) -> (&'static str, [f64; 3], [f64; 3]) {
    let turn = rng.uniform(0.0, core::f64::consts::TAU);
    let pick = rng.below(8);
    let pi2 = core::f64::consts::FRAC_PI_2;
    match (k, pick) {
        (Kind::Plane, 0) => (
            "parallel",
            [0.0, 0.0, rng.uniform(-1.0, 1.0)],
            [0.0, 0.0, 1.0],
        ),
        (Kind::Plane, 1) => (
            "coincident",
            [rng.uniform(-3.0, 3.0), 0.0, 0.0],
            [0.0, 0.0, 1.0],
        ),
        (Kind::Plane, 2) => (
            "square",
            [rng.uniform(-3.0, 3.0), 0.0, 0.0],
            tilted(pi2, turn),
        ),
        (Kind::Cylinder { .. }, 0 | 1) => (
            "across",
            [0.0, 0.0, rng.uniform(-1.0, HEIGHT + 1.0)],
            [0.0, 0.0, 1.0],
        ),
        (Kind::Cylinder { r }, 2 | 3) => {
            let d = r * rng.uniform(-1.3, 1.3);
            let (c, s) = (scx_num::fmath::cos(turn), scx_num::fmath::sin(turn));
            ("along", [d * c, d * s, 1.0], [c, s, 0.0])
        }
        (Kind::Cylinder { r }, 4) => {
            let (c, s) = (scx_num::fmath::cos(turn), scx_num::fmath::sin(turn));
            ("tangent", [r * c, r * s, 1.0], [c, s, 0.0])
        }
        (Kind::Cylinder { .. }, 5) => (
            "steep",
            [0.0, 0.0, rng.uniform(1.0, 5.0)],
            tilted(rng.uniform(1.4, 1.57), turn),
        ),
        (Kind::Cone { h, .. }, 0 | 1) => (
            "across",
            [0.0, 0.0, rng.uniform(-1.0, h + 1.0)],
            [0.0, 0.0, 1.0],
        ),
        (Kind::Cone { r, a, .. }, 2) => {
            // The apex, at `z = -r / tan a`.
            let apex = -r / scx_num::fmath::tan(a);
            (
                "apex",
                [0.0, 0.0, apex],
                tilted(rng.uniform(0.0, pi2), turn),
            )
        }
        (Kind::Cone { r, a, h }, 3) => {
            // Square to a generator's direction rotated into the plane (parallel to it):
            // a parabola.
            let off = rng.uniform(0.0, h);
            ("generator", [r, 0.0, off], tilted(pi2 - a.abs(), turn))
        }
        (Kind::Cone { a, h, .. }, 4) => (
            "steep",
            [0.0, 0.0, rng.uniform(0.0, h)],
            tilted(rng.uniform(0.0, pi2 - a.abs() - 0.05), turn),
        ),
        (Kind::Sphere { r }, 0..=2) => {
            let n = tilted(rng.uniform(0.0, core::f64::consts::PI), turn);
            let d = r * rng.uniform(-1.2, 1.2);
            ("offset", [n[0] * d, n[1] * d, n[2] * d], n)
        }
        (Kind::Sphere { r }, 3) => ("touch", [0.0, 0.0, r], [0.0, 0.0, 1.0]),
        (Kind::Torus { small, .. }, 0 | 1) => (
            "across",
            [0.0, 0.0, small * rng.uniform(-1.2, 1.2)],
            [0.0, 0.0, 1.0],
        ),
        (Kind::Torus { small, .. }, 2) => ("top", [0.0, 0.0, small], [0.0, 0.0, 1.0]),
        (Kind::Torus { .. }, 3) => {
            let (c, s) = (scx_num::fmath::cos(turn), scx_num::fmath::sin(turn));
            ("meridian", [0.0, 0.0, 0.0], [c, s, 0.0])
        }
        (Kind::Torus { big, small }, 4) => {
            // Bitangent (Villarceau): through the centre at asin(small / big) from the
            // equator plane.
            let tilt = scx_num::fmath::asin(small / big);
            ("bitangent", [0.0, 0.0, 0.0], tilted(tilt, turn))
        }
        _ => {
            let size = match k {
                Kind::Plane => 3.0,
                Kind::Cylinder { r } | Kind::Sphere { r } => r,
                Kind::Cone { r, .. } => r,
                Kind::Torus { big, small } => big + small,
            };
            let at = [
                size * rng.uniform(-1.2, 1.2),
                size * rng.uniform(-1.2, 1.2),
                rng.uniform(-1.0, HEIGHT + 1.0),
            ];
            ("random", at, unit(rng))
        }
    }
}

impl Gen for Case {
    fn pool(rng: &mut SplitMix64) -> Vec<Self> {
        let mut out = Vec::new();
        let mut frames = Rng::new(rng.next_u64());
        for i in 0..1000 {
            let f = scx_testkit::sample::frame(&mut frames, 10.0);
            let kind = kind(rng, i);
            let frame = [f.origin, f.z, f.x];
            let (aim, at, n) = aimed(rng, kind);
            let half = if rng.below(3) == 0 {
                rng.uniform(0.5, 4.0)
            } else {
                10.0
            };
            let plane = plane(&frame_of(frame), at, n);
            out.push(Case {
                kind,
                frame,
                plane,
                half,
                aim,
            });
        }
        out
    }
}

/// One piece of a fingerprint: its class, its contact and its numbers.
pub type Piece = (&'static str, &'static str, Vec<f64>);
/// A fingerprint: the outcome and the pieces.
pub type Print = (&'static str, Vec<Piece>);

/// The ends of a line or arc, in a fixed order.
fn ends(a: [f64; 3], b: [f64; 3]) -> Vec<f64> {
    let key = |p: [f64; 3]| p[0] + 0.618 * p[1] + 0.414 * p[2];
    let (a, b) = if key(a) <= key(b) { (a, b) } else { (b, a) };
    a.iter().chain(&b).copied().collect()
}

/// A conic's centre and radii, smaller first.
fn conic(c: [f64; 3], r1: f64, r2: f64) -> Vec<f64> {
    let (lo, hi) = if r1 <= r2 { (r1, r2) } else { (r2, r1) };
    vec![c[0], c[1], c[2], lo, hi]
}

fn sorted(outcome: &'static str, mut pieces: Vec<Piece>) -> Print {
    let key = |p: &Piece| {
        p.2.iter()
            .enumerate()
            .map(|(i, x)| x * (1.0 + 0.37 * i as f64))
            .sum::<f64>()
    };
    pieces.sort_by(|a, b| (a.0, key(a)).partial_cmp(&(b.0, key(b))).unwrap());
    (outcome, pieces)
}

/// scx's side.
pub mod x {
    use super::{Case, HEIGHT, Kind, Piece, Print, SNAP, conic, ends, sorted};
    use core::f64::consts::{FRAC_PI_2, PI, TAU};
    use scx_curve::Curve;
    use scx_num::extent2d::Extent2d;
    use scx_num::frame::Sense;
    use scx_num::units::Length;
    use scx_num::vector3d::Point3;
    use scx_ssi::analytic::{Contact, Intersection, section_by_plane};
    use scx_surface::{Closed, Cone, Cylinder, Plane, Sphere, Surface, Torus};

    /// The surface and its whole extent (research 63 D13).
    pub fn surface(c: &Case) -> (Surface, Extent2d) {
        let (f, s, mm) = (c.surface_frame(), Sense::Forward, Length::from_mm);
        let e = |u: f64, v: f64| Extent2d::from_bounds(0.0, 0.0, u, v).unwrap();
        match c.kind {
            Kind::Plane => (
                Surface::Plane(Plane::new(f)),
                Extent2d::from_bounds(-10.0, -10.0, 10.0, 10.0).unwrap(),
            ),
            Kind::Cylinder { r } => (
                Surface::Cylinder(Cylinder::new(f, mm(r), s).unwrap()),
                e(TAU * r, HEIGHT),
            ),
            Kind::Cone { r, a, h } => {
                (Surface::Cone(Cone::new(f, mm(r), a, s).unwrap()), e(1.0, h))
            }
            Kind::Sphere { r } => (
                Surface::Sphere(Sphere::new(f, mm(r), s).unwrap()),
                e(1.0, 1.0),
            ),
            Kind::Torus { big, small } => (
                Surface::Torus(Torus::new(f, mm(big), mm(small), s).unwrap()),
                e(1.0, 1.0),
            ),
        }
    }

    pub fn plane(c: &Case) -> (Plane, Extent2d) {
        let h = c.half;
        (
            Plane::new(c.plane_frame()),
            Extent2d::from_bounds(-h, -h, h, h).unwrap(),
        )
    }

    pub fn answer(c: &Case) -> Closed<Intersection> {
        let (s, se) = surface(c);
        let (p, pe) = plane(c);
        section_by_plane(&s, &se, &p, &pe, Length::from_mm(SNAP))
    }

    fn arr(p: Point3) -> [f64; 3] {
        [p.x, p.y, p.z]
    }

    fn contact(c: Contact) -> &'static str {
        match c {
            Contact::Transversal => "transversal",
            Contact::Tangent => "tangent",
        }
    }

    pub fn print(r: &Closed<Intersection>) -> Print {
        let (curves, points) = match r {
            Closed::General => return ("general", vec![]),
            Closed::Solved(Intersection::Empty) => return ("empty", vec![]),
            Closed::Solved(Intersection::Coincident { .. }) => return ("coincident", vec![]),
            Closed::Solved(Intersection::Pieces { curves, points }) => (curves, points),
            _ => return ("other", vec![]),
        };
        let mut out: Vec<Piece> = Vec::new();
        for c in curves {
            let (t0, t1) = (c.span.min(), c.span.max());
            let (a, b) = (arr(c.curve.point(t0)), arr(c.curve.point(t1)));
            let k = contact(c.contact);
            match &c.curve {
                Curve::Line(_) => out.push(("line", k, ends(a, b))),
                Curve::Circle(_) | Curve::Ellipse(_) => {
                    let p0 = c.curve.point(0.0);
                    let pq = c.curve.point(FRAC_PI_2);
                    let pp = c.curve.point(PI);
                    let m = Point3::new(
                        0.5 * (p0.x + pp.x),
                        0.5 * (p0.y + pp.y),
                        0.5 * (p0.z + pp.z),
                    );
                    let mut v = conic(arr(m), p0.distance(m), pq.distance(m));
                    if t1 - t0 >= TAU - 1e-9 {
                        out.push(("conic", k, v));
                    } else {
                        v.extend(ends(a, b));
                        out.push(("arc", k, v));
                    }
                }
                _ => out.push(("nurbs", k, vec![])),
            }
        }
        for p in points {
            out.push(("point", contact(p.contact), arr(p.point).to_vec()));
        }
        sorted("pieces", out)
    }
}

/// The pin's side.
pub mod s {
    use super::{Case, HEIGHT, Kind, Piece, Print, conic, ends, sorted};
    use solidcore::curve::curve::{Curve, CurveKind};
    use solidcore::geom::axis2_placement::Axis2Placement;
    use solidcore::geom::extent2d::Extent2d;
    use solidcore::surface::cone::p1::Cone;
    use solidcore::surface::plane::Plane;
    use solidcore::surface::sphere::Sphere;
    use solidcore::surface::surface::p2::{Surface, SurfaceIntersection, TsectCurveType};
    use solidcore::surface::torus::{Torus, TorusOptions};
    use solidcore::{Kernel, Point3, Vec2, Vec3};

    fn pin(f: &scx_num::frame::Frame) -> (Point3, Vec3, Vec3) {
        let o = f.origin();
        let (x, y) = (*f.x(), *f.y());
        (
            Point3::new(o.x, o.y, o.z),
            Vec3::new(x.x, x.y, x.z),
            Vec3::new(y.x, y.y, y.z),
        )
    }

    fn square(h: f64) -> Extent2d {
        Extent2d::from_bounds(-h, -h, h, h)
    }

    pub fn surface(c: &Case, k: &Kernel) -> Surface {
        let (o, x, y) = pin(&c.surface_frame());
        match c.kind {
            Kind::Plane => {
                let mut p = Plane::new(o, x, y, Vec2::new(1.0, 1.0), &square(10.0));
                p.make_nurb().unwrap();
                Surface::from(p)
            }
            Kind::Cylinder { r } => {
                let mut s =
                    Cone::new_cylinder_bounded(o, x, y, r, 0.0, 360.0, HEIGHT, false, false, true);
                s.make_nurb(k).unwrap();
                Surface::from(s)
            }
            Kind::Cone { r, a, h } => {
                let top = r + scx_num::fmath::tan(a) * h;
                let mut s = Cone::new_bounded(o, x, y, r, top, 0.0, 360.0, h, false, false, true);
                s.make_nurb(k).unwrap();
                Surface::from(s)
            }
            Kind::Sphere { r } => {
                let place = Axis2Placement::new(o, x, y);
                Surface::from(Sphere::create_canonical(k, &place, r).unwrap())
            }
            Kind::Torus { big, small } => {
                let place = Axis2Placement::new(o, x, y);
                let opts = TorusOptions {
                    swap_uv: false,
                    inside_out: false,
                    circle_nurb: None,
                };
                Surface::from(Torus::create_canonical(k, &place, big, small, opts, None).unwrap())
            }
        }
    }

    pub fn plane(c: &Case) -> (Plane, Extent2d) {
        let (o, x, y) = pin(&c.plane_frame());
        let e = square(c.half);
        let mut p = Plane::new(o, x, y, Vec2::new(1.0, 1.0), &e);
        p.make_nurb().unwrap();
        (p, e)
    }

    pub fn answer(c: &Case) -> Option<(bool, SurfaceIntersection)> {
        let k = Kernel::default();
        let s = surface(c, &k);
        let domain = s.natural_uv_domain_with_nurb(&k).ok()?;
        let (p, pe) = plane(c);
        s.intersect_with_plane(&k, &domain, &p, &pe, [false; 2], None, None)
            .ok()
            .map(|(more, x, _)| (more, x))
    }

    fn arr(p: Point3) -> [f64; 3] {
        [p.x, p.y, p.z]
    }

    fn piece(k: &Kernel, c: &Curve, t: TsectCurveType) -> Piece {
        let i = c.natural_interval();
        let at = |t: f64| c.evaluate_point(k, t).map(arr).unwrap_or([f64::NAN; 3]);
        let (a, b) = (at(i.min()), at(i.max()));
        let contact = match t {
            TsectCurveType::Crossing => "transversal",
            TsectCurveType::Tangent => "tangent",
            TsectCurveType::Touching => return ("point", "tangent", a.to_vec()),
            TsectCurveType::Unknown => "unknown",
            TsectCurveType::Coincident => "coincident",
            TsectCurveType::NearTangent => "near-tangent",
            TsectCurveType::RegionBoundary => "region-boundary",
        };
        match c.kind() {
            CurveKind::Line(_) => ("line", contact, ends(a, b)),
            CurveKind::Ellipse(e) => {
                let m = arr(e.position().origin());
                let mut v = conic(m, e.x_radius(), e.y_radius());
                if e.end_angle_deg() - e.start_angle_deg() >= 360.0 - 1e-9 {
                    ("conic", contact, v)
                } else {
                    v.extend(ends(a, b));
                    ("arc", contact, v)
                }
            }
            CurveKind::BSpline(_) => ("nurbs", contact, vec![]),
            _ => ("other", contact, vec![]),
        }
    }

    pub fn print(r: &Option<(bool, SurfaceIntersection)>) -> Print {
        let Some((more, x)) = r else {
            return ("error", vec![]);
        };
        if *more {
            return ("general", vec![]);
        }
        if x.curves_3d.is_empty() {
            return ("empty", vec![]);
        }
        let k = Kernel::default();
        let pieces = x
            .curves_3d
            .iter()
            .zip(&x.curve_types)
            .map(|(c, &t)| piece(&k, c, t))
            .collect();
        sorted("pieces", pieces)
    }
}

/// The check against the exact sections (testkit T5) for the inputs where scx and the pin
/// disagree (`pin_defects::ssi_section_by_plane`): scx's answer is recomputed with its
/// curves, and judged on its own.
pub mod truth {
    use super::{Case, HEIGHT, Kind, SNAP, x};
    use core::f64::consts::TAU;
    use scx_curve::Curve;
    use scx_num::frame::Frame;
    use scx_num::vector3d::{Point3, Vec3};
    use scx_ssi::analytic::Intersection;
    use scx_surface::Closed;
    use scx_testkit::analytic as ta;
    use scx_testkit::curve_surface::{self as cs, SurfaceKind};
    use scx_testkit::dd::Dd;
    use scx_testkit::ssi::{self, Section};

    /// How far an answer's point may lie from a surface, relative to `1 + |p|`.
    const ON: f64 = 1e-9;
    /// How far inside both extents a truth point must be to be owed.
    const MARGIN: f64 = 1e-6;
    /// How near an answer's piece an owed truth point must be, relative to `1 + |p|`.
    const NEAR: f64 = 1e-7;

    fn arr(p: Point3) -> [f64; 3] {
        [p.x, p.y, p.z]
    }

    fn varr(v: Vec3) -> [f64; 3] {
        [v.x, v.y, v.z]
    }

    fn tframe(f: &Frame) -> ta::Frame {
        ta::Frame {
            origin: arr(f.origin()),
            x: varr(*f.x()),
            y: varr(*f.y()),
            z: varr(*f.z()),
        }
    }

    fn kinds(c: &Case) -> (SurfaceKind, SurfaceKind) {
        let s = kind_of(c.kind, &c.surface_frame());
        let p = SurfaceKind::Plane(ta::Plane {
            frame: tframe(&c.plane_frame()),
        });
        (s, p)
    }

    /// The testkit's form of a kind in a frame (forward sense).
    pub fn kind_of(kind: Kind, f: &Frame) -> SurfaceKind {
        let frame = tframe(f);
        match kind {
            Kind::Plane => SurfaceKind::Plane(ta::Plane { frame }),
            Kind::Cylinder { r } => SurfaceKind::Cylinder(ta::Cylinder { frame, radius: r }),
            // The testkit's cone widens along its z: a narrowing one is the same cone with
            // z and y turned over.
            Kind::Cone { r, a, .. } => SurfaceKind::Cone(ta::Cone {
                frame: if a < 0.0 {
                    ta::Frame {
                        y: frame.y.map(|t| -t),
                        z: frame.z.map(|t| -t),
                        ..frame
                    }
                } else {
                    frame
                },
                radius: r,
                semi_angle: a.abs(),
            }),
            Kind::Sphere { r } => SurfaceKind::Sphere(ta::Sphere { frame, radius: r }),
            Kind::Torus { big, small } => SurfaceKind::Torus(ta::Torus {
                frame,
                major: big,
                minor: small,
            }),
        }
    }

    fn local(f: &Frame, q: Point3) -> [f64; 3] {
        let d = q - f.origin();
        [d.dot(*f.x()), d.dot(*f.y()), d.dot(*f.z())]
    }

    /// Whether `q` (on both surfaces) is inside both whole extents, by `tol` (negative: by
    /// that much inside). Only heights and the plane's square bound them: every kind's turn
    /// is whole, and a sphere's and torus's `v` too.
    fn inside(c: &Case, q: Point3, tol: f64) -> bool {
        let within = |x: f64, lo: f64, hi: f64| x >= lo - tol && x <= hi + tol;
        let [u, w, _] = local(&c.plane_frame(), q);
        let [lx, ly, z] = local(&c.surface_frame(), q);
        let on_surface = match c.kind {
            Kind::Plane => within(lx, -10.0, 10.0) && within(ly, -10.0, 10.0),
            Kind::Cylinder { .. } => within(z, 0.0, HEIGHT),
            Kind::Cone { r, a, h } => within(z, 0.0, h) && r + scx_num::fmath::tan(a) * z >= -tol,
            Kind::Sphere { .. } | Kind::Torus { .. } => true,
        };
        on_surface && within(u, -c.half, c.half) && within(w, -c.half, c.half)
    }

    fn tpoint(q: [Dd; 3]) -> Point3 {
        Point3::new(q[0].hi, q[1].hi, q[2].hi)
    }

    /// The truth's sample points inside both extents.
    fn owed(c: &Case, pieces: &[ssi::Piece]) -> Vec<Point3> {
        let mut out = Vec::new();
        for piece in pieces {
            let (n, t0, t1) = match piece.curve {
                ssi::Curve::Point(_) => (1, 0.0, 0.0),
                ssi::Curve::Line { .. } => (4000, -100.0, 100.0),
                ssi::Curve::Hyperbola { .. } => (4000, -10.0, 10.0),
                _ => (720, 0.0, TAU),
            };
            for i in 0..n {
                let t = t0 + (t1 - t0) * f64::from(i) / f64::from(n);
                let q = tpoint(piece.curve.point(Dd::new(t)));
                if inside(c, q, -MARGIN) {
                    out.push(q);
                }
            }
        }
        out
    }

    /// The distance from `q` to the curve over `[t0, t1]`: the nearest of a sampling, then
    /// a golden-section search beside it (across the ends on a whole turn).
    pub fn distance_to(c: &Curve, t0: f64, t1: f64, q: Point3) -> f64 {
        const N: usize = 256;
        let f = |t: f64| c.point(t).distance(q);
        let h = (t1 - t0) / N as f64;
        let mut best = (f(t0), t0);
        for i in 1..=N {
            let t = t0 + h * i as f64;
            let d = f(t);
            if d < best.0 {
                best = (d, t);
            }
        }
        let turn = t1 - t0 >= TAU - 1e-9;
        let (mut a, mut b) = if turn {
            (best.1 - h, best.1 + h)
        } else {
            ((best.1 - h).max(t0), (best.1 + h).min(t1))
        };
        let g = 0.5 * (5f64.sqrt() - 1.0);
        for _ in 0..80 {
            let (c1, c2) = (b - g * (b - a), a + g * (b - a));
            if f(c1) < f(c2) {
                b = c2;
            } else {
                a = c1;
            }
        }
        best.0.min(f(0.5 * (a + b)))
    }

    /// Whether the surface over its whole extent lies off the plane by more than the
    /// tolerance on one side, on a grid of its points (for an empty answer where the truth
    /// has no closed form to sample).
    fn off_the_plane(c: &Case) -> bool {
        let (s, se) = x::surface(c);
        let f = c.plane_frame();
        let (mut lo, mut hi) = (f64::INFINITY, f64::NEG_INFINITY);
        for i in 0..=64 {
            for j in 0..=64 {
                let u = se.min().x + (se.max().x - se.min().x) * f64::from(i) / 64.0;
                let v = se.min().y + (se.max().y - se.min().y) * f64::from(j) / 64.0;
                let p = s.point(scx_num::vector2d::Point2::new(u, v));
                let d = local(&f, p)[2];
                lo = lo.min(d);
                hi = hi.max(d);
            }
        }
        lo > SNAP || hi < -SNAP
    }

    /// Whether two planes are one within the tolerance where they overlap: every corner of
    /// each square lies within it of the other plane, and one square's centre lies in the
    /// other square. The exact truth sees a line or nothing where the doubles leave the
    /// planes an ulp apart; within the tolerance they are coincident.
    fn coincident(c: &Case) -> bool {
        let (a, b) = (c.surface_frame(), c.plane_frame());
        let corners = |f: &Frame, h: f64| {
            [(-h, -h), (h, -h), (h, h), (-h, h)].map(|(u, v)| f.origin() + *f.x() * u + *f.y() * v)
        };
        let near = |f: &Frame, q: Point3| local(f, q)[2].abs() <= SNAP;
        let within = |f: &Frame, h: f64, q: Point3| {
            let [u, v, _] = local(f, q);
            u.abs() <= h && v.abs() <= h
        };
        corners(&a, 10.0).into_iter().all(|q| near(&b, q))
            && corners(&b, c.half).into_iter().all(|q| near(&a, q))
            && (within(&a, 10.0, b.origin()) || within(&b, c.half, a.origin()))
    }

    /// `Err` with what is wrong when scx's answer is not the exact section's.
    pub fn check(c: &Case) -> Result<(), String> {
        let (sk, pk) = kinds(c);
        let truth = ssi::intersect(&sk, &pk);
        let got = x::answer(c);
        let (curves, points) = match &got {
            Closed::General => {
                return match &truth {
                    Err(ssi::Error::NoClosedForm | ssi::Error::Undecided) => Ok(()),
                    Ok(Section::Curves(t))
                        if t.iter()
                            .any(|q| matches!(q.curve, ssi::Curve::Hyperbola { .. })) =>
                    {
                        Ok(())
                    }
                    t => Err(format!("General where the truth is {t:?}")),
                };
            }
            Closed::Solved(Intersection::Coincident { .. }) => {
                return if truth == Ok(Section::Coincident) || coincident(c) {
                    Ok(())
                } else {
                    Err(format!("coincident; the truth is {truth:?}"))
                };
            }
            Closed::Solved(Intersection::Empty) => (&[][..], &[][..]),
            Closed::Solved(Intersection::Pieces { curves, points }) => (&curves[..], &points[..]),
            _ => return Err("an unknown answer".to_owned()),
        };
        let tpieces = match &truth {
            Ok(Section::Curves(t)) => t.clone(),
            Ok(Section::Empty) => Vec::new(),
            Err(ssi::Error::NoClosedForm)
                if curves.is_empty() && points.is_empty() && off_the_plane(c) =>
            {
                Vec::new()
            }
            t => return Err(format!("the truth is {t:?}")),
        };
        let on = |q: Point3| -> Result<(), String> {
            let a = cs::distance(&sk, arr(q)).map_err(|e| format!("{e:?}"))?;
            let b = cs::distance(&pk, arr(q)).map_err(|e| format!("{e:?}"))?;
            let scale = 1.0 + q.to_vec().length();
            if a > ON * scale || b > ON * scale {
                return Err(format!("{q:?} off the surfaces by {a:e}, {b:e}"));
            }
            if !inside(c, q, 1e-7) {
                return Err(format!("{q:?} outside an extent"));
            }
            Ok(())
        };
        let mut ends = Vec::new();
        for k in curves {
            let (t0, t1) = (k.span.min(), k.span.max());
            for i in 0..=32 {
                on(k.curve.point(t0 + (t1 - t0) * f64::from(i) / 32.0))?;
            }
            if t1 - t0 < TAU - 1e-9 {
                ends.push(k.curve.point(t0));
                ends.push(k.curve.point(t1));
            }
        }
        for q in points {
            on(q.point)?;
        }
        // Each piece is a whole connected piece: no two curves meet at their ends.
        for (i, a) in ends.iter().enumerate() {
            for (j, b) in ends.iter().enumerate().skip(i + 1) {
                if i / 2 != j / 2 && a.distance(*b) <= NEAR * (1.0 + a.to_vec().length()) {
                    return Err(format!("two pieces meet at {a:?}"));
                }
            }
        }
        for q in owed(c, &tpieces) {
            let near = curves
                .iter()
                .map(|k| distance_to(&k.curve, k.span.min(), k.span.max(), q))
                .chain(points.iter().map(|p| p.point.distance(q)))
                .fold(f64::INFINITY, f64::min);
            if near > NEAR * (1.0 + q.to_vec().length()) {
                return Err(format!("truth point {q:?} missed by {near:e}"));
            }
        }
        Ok(())
    }
}

#[test]
fn ssi_section_by_plane() {
    run_defect_tol(
        "ssi_section_by_plane",
        TOL,
        &crate::pin_defects::ssi_section_by_plane(),
        |(c,): (Case,)| x::print(&x::answer(&c)),
        |(c,): (Case,)| s::print(&s::answer(&c)),
    );
}
