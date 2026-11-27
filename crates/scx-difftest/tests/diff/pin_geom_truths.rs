// Written for solidcorex. Apache-2.0.
//! The pin's curve-surface and surface-surface intersectors against the analytic truths
//! (research 63 T2 and T5: `scx_testkit::curve_surface`, `scx_testkit::ssi`; research 64).
//!
//! Each pin surface is the truth's, bounded: a plane to a square about its origin, a
//! cylinder or cone to a length of its axis, spheres and tori whole. The truth's points
//! inside both windows (off their edges by a margin) must be on the pin's answer, and the
//! pin's answer must be on the truth's:
//!
//! - **curve-surface** (`intersect_curve_surface`): lines (as segments), circles and
//!   ellipses through points of each surface, crossing at random and tangent. Each truth
//!   point is matched to the nearest pin point, and each pin point to the nearest truth
//!   point.
//! - **surface-surface** (`intersect_surfaces_within` to [`APPROX`]): plane-plane, planes
//!   against cylinders, cones and spheres, sphere pairs, coaxial pairs and planes across a
//!   torus's axis. Each truth curve is sampled densely and each sample's distance to the
//!   pin's curves measured; each pin curve is sampled and its points' first-order
//!   distances to both truth surfaces measured.
//!
//! An input is exact when every distance is within [`EXACT`], within tolerance when
//! within its bound ([`HIT_TOL`] or [`TANGENT_TOL`] for a point, [`CURVE_TOL`] for a
//! curve). Past it, a pin defect is the pin being wrong: a truth point it missed, or a pin
//! point off the truth. A mismatch is a pin point on both truth objects that the truth
//! does not have, which would be the truth's error; it fails the test. Each test prints
//! `pin geom <name>: <n> inputs, <e> exact, <w> within tolerance, <p> pin defects, <m>
//! mismatches`, then each defect: the input, the pin's answer, the truth and the error.

use scx_num::hash::Rng;
use scx_testkit::analytic::{Cone, Cylinder, Frame, Line, Plane, Sphere, Torus};
use scx_testkit::curve_surface::{self as cs, CurveKind, Meeting, SurfaceKind};
use scx_testkit::dd::{self, Dd};
use scx_testkit::sample;
use scx_testkit::ssi::{self, Section};
use solidcore::curve::curve::Curve as PinCurve;
use solidcore::curve::ellipse::Ellipse as PinEllipse;
use solidcore::curve::line::Line as PinLine;
use solidcore::geom::axis2_placement::Axis2Placement;
use solidcore::geom::extent2d::Extent2d;
use solidcore::intersect::api_intersectors::{intersect_curve_surface, intersect_surfaces_within};
use solidcore::surface::cone::p1::Cone as PinCone;
use solidcore::surface::plane::Plane as PinPlane;
use solidcore::surface::sphere::{Sphere as PinSphere, SphereOptions};
use solidcore::surface::surface::p2::Surface;
use solidcore::surface::torus::{Torus as PinTorus, TorusOptions};
use solidcore::{Kernel, Point3, Vec2, Vec3};

type P = [f64; 3];

/// Distances (the inputs are of unit size) an input is exact within.
const EXACT: f64 = 1e-14;
/// A crossing point's bound.
const HIT_TOL: f64 = 1e-9;
/// A tangent point's bound: a double root moves by the square root of the rounding.
const TANGENT_TOL: f64 = 1e-6;
/// The pin's surface-surface approximation tolerance, and the bound on its curves.
const APPROX: f64 = 1e-7;
const CURVE_TOL: f64 = 1e-6;
/// Points this near a window's edge are left out on both sides.
const MARGIN: f64 = 1e-4;
/// A plane's half-width; a cylinder's axial half-length; a cone's length from its
/// reference circle.
const HALF: f64 = 3.0;
const CONE_LENGTH: f64 = 2.0;
/// A line's half-length: the segment the pin takes is `t` in `[-LINE_HALF, LINE_HALF]`.
const LINE_HALF: f64 = 2.0;

fn p3(a: P) -> Point3 {
    Point3::new(a[0], a[1], a[2])
}

fn v3(a: P) -> Vec3 {
    Vec3::new(a[0], a[1], a[2])
}

fn of(p: Point3) -> P {
    [p.x, p.y, p.z]
}

fn hi(v: [Dd; 3]) -> P {
    v.map(|x| x.to_f64())
}

fn sub(a: P, b: P) -> P {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

fn dot(a: P, b: P) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

fn dist(a: P, b: P) -> f64 {
    let d = sub(a, b);
    dot(d, d).sqrt()
}

fn axes(f: &Frame) -> Axis2Placement {
    let mut a = Axis2Placement::default();
    a.set_canonical(p3(f.origin), v3(f.x), v3(f.y)).unwrap();
    a
}

/// The degrees of an angle in radians, in double-double.
fn degrees(x: f64) -> f64 {
    (Dd::new(x) * 180.0 / dd::consts().pi).to_f64()
}

/// Where a bounded pin surface reaches, in the truth surface's frame.
#[derive(Clone, Copy, Debug)]
enum Window {
    Square(f64),
    Axial(f64, f64),
    Whole,
}

struct Bounded {
    truth: SurfaceKind,
    pin: Surface,
    window: Window,
}

fn frame_of(s: &SurfaceKind) -> Frame {
    match s {
        SurfaceKind::Plane(p) => p.frame,
        SurfaceKind::Cylinder(c) => c.frame,
        SurfaceKind::Cone(c) => c.frame,
        SurfaceKind::Sphere(c) => c.frame,
        SurfaceKind::Torus(c) => c.frame,
    }
}

impl Bounded {
    fn new(k: &Kernel, truth: SurfaceKind) -> Bounded {
        let f = frame_of(&truth);
        let (o, x, y) = (p3(f.origin), v3(f.x), v3(f.y));
        let (pin, window) = match truth {
            SurfaceKind::Plane(_) => {
                let ext = Extent2d::from_bounds(-HALF, -HALF, HALF, HALF);
                let mut s = PinPlane::new(o, x, y, Vec2::new(1.0, 1.0), &ext);
                s.make_nurb().unwrap();
                (Surface::from(s), Window::Square(HALF))
            }
            SurfaceKind::Cylinder(c) => {
                let base = p3(core::array::from_fn(|i| f.origin[i] - HALF * f.z[i]));
                let mut s = PinCone::new_cylinder_bounded(
                    base,
                    x,
                    y,
                    c.radius,
                    0.0,
                    360.0,
                    2.0 * HALF,
                    false,
                    false,
                    true,
                );
                s.make_nurb(k).unwrap();
                (Surface::from(s), Window::Axial(-HALF, HALF))
            }
            SurfaceKind::Cone(c) => {
                let s = PinCone::create_canonical_with_semi_angle(
                    k,
                    &axes(&f),
                    c.radius,
                    degrees(c.semi_angle),
                    Some(CONE_LENGTH),
                    false,
                    false,
                )
                .unwrap();
                (Surface::from(s), Window::Axial(0.0, CONE_LENGTH))
            }
            SurfaceKind::Sphere(c) => {
                let opts = SphereOptions {
                    swap_uv: false,
                    inside_out: false,
                    circle_nurb: None,
                };
                let ext = Extent2d::from_bounds(0.0, -90.0, 360.0, 90.0);
                let mut s = PinSphere::new(k, o, x, y, ext, c.radius, opts).unwrap();
                s.make_nurb(k).unwrap();
                (Surface::from(s), Window::Whole)
            }
            SurfaceKind::Torus(c) => {
                let opts = TorusOptions {
                    swap_uv: false,
                    inside_out: false,
                    circle_nurb: None,
                };
                let ext = Extent2d::from_bounds(0.0, 0.0, 360.0, 360.0);
                let mut s = PinTorus::new(k, o, x, y, ext, c.major, c.minor, opts).unwrap();
                s.make_nurb(k).unwrap();
                (Surface::from(s), Window::Whole)
            }
        };
        Bounded { truth, pin, window }
    }

    /// `p` in the truth surface's frame.
    fn coords(&self, p: P) -> P {
        let f = frame_of(&self.truth);
        let l = sub(p, f.origin);
        [dot(l, f.x), dot(l, f.y), dot(l, f.z)]
    }

    /// Whether `p` is inside the window by at least `margin`.
    fn inside(&self, p: P, margin: f64) -> bool {
        let f = frame_of(&self.truth);
        let l = sub(p, f.origin);
        match self.window {
            Window::Square(h) => dot(l, f.x).abs() <= h - margin && dot(l, f.y).abs() <= h - margin,
            Window::Axial(lo, hi) => (lo + margin..=hi - margin).contains(&dot(l, f.z)),
            Window::Whole => true,
        }
    }
}

/// What a test counts, and the defects it prints.
#[derive(Default)]
struct Tally {
    inputs: usize,
    exact: usize,
    within: usize,
    defects: Vec<(usize, String)>,
    mismatches: Vec<String>,
    notes: Vec<String>,
}

impl Tally {
    /// A pin defect on the current input.
    fn defect(&mut self, d: String) {
        self.defects.push((self.inputs.saturating_sub(1), d));
    }

    /// Prints the counts and each defect, and fails on a mismatch or when the pin's
    /// defective inputs are not `known`.
    fn report(&self, name: &str, known: &[usize]) {
        println!(
            "pin geom {name}: {} inputs, {} exact, {} within tolerance, {} pin defects, {} \
             mismatches",
            self.inputs,
            self.exact,
            self.within,
            self.defects.len(),
            self.mismatches.len()
        );
        for (i, d) in &self.defects {
            println!("  pin defect #{i}: {d}");
        }
        for m in &self.mismatches {
            println!("  MISMATCH: {m}");
        }
        for n in &self.notes {
            println!("  note: {n}");
        }
        assert!(self.mismatches.is_empty(), "{name}: truth mismatches");
        let mut got: Vec<usize> = self.defects.iter().map(|(i, _)| *i).collect();
        got.dedup();
        assert_eq!(got, known, "{name}: the pin's defective inputs changed");
    }
}

/// A pin curve, sampled once: its evaluation, interval and `SAMPLES + 1` points.
struct Track<'c> {
    f: Box<dyn Fn(f64) -> P + 'c>,
    lo: f64,
    hi: f64,
    points: Vec<P>,
}

const SAMPLES: usize = 1024;

impl<'c> Track<'c> {
    fn new(f: Box<dyn Fn(f64) -> P + 'c>, lo: f64, hi: f64) -> Track<'c> {
        let points = (0..=SAMPLES)
            .map(|i| f(lo + (hi - lo) * i as f64 / SAMPLES as f64))
            .collect();
        Track { f, lo, hi, points }
    }

    fn at(&self, i: usize) -> f64 {
        self.lo + (self.hi - self.lo) * i as f64 / SAMPLES as f64
    }

    /// The smallest distance from `p`: golden-section search between the neighbours of
    /// each of the three nearest samples.
    fn nearest(&self, p: P) -> f64 {
        let mut order: Vec<usize> = (0..self.points.len()).collect();
        order.sort_by(|&a, &b| dist(self.points[a], p).total_cmp(&dist(self.points[b], p)));
        let g = 0.5 * (5f64.sqrt() - 1.0);
        let d = |s: f64| dist((self.f)(s), p);
        order
            .iter()
            .take(3)
            .map(|&best| {
                let (mut a, mut b) = (
                    self.at(best.saturating_sub(1)),
                    self.at((best + 1).min(SAMPLES)),
                );
                for _ in 0..60 {
                    let (c, e) = (b - g * (b - a), a + g * (b - a));
                    if d(c) < d(e) {
                        b = e;
                    } else {
                        a = c;
                    }
                }
                d(0.5 * (a + b)).min(dist(self.points[best], p))
            })
            .fold(f64::INFINITY, f64::min)
    }
}

// ---------------------------------------------------------------- curve-surface

/// A truth curve, its pin counterpart, and the truth parameter window (a line's segment).
struct Probe {
    truth: CurveKind,
    pin: PinCurve,
    window: Option<f64>,
}

fn probe(k: &Kernel, truth: CurveKind) -> Probe {
    match truth {
        CurveKind::Line(l) => {
            let len = LINE_HALF;
            let end = |s: f64| {
                p3(core::array::from_fn(|i| {
                    l.origin[i] + s * len * l.direction[i]
                }))
            };
            let line = PinLine::create_line_segment(3, end(-1.0), end(1.0), None).unwrap();
            Probe {
                truth,
                pin: PinCurve::from(line),
                window: Some(len),
            }
        }
        CurveKind::Circle(c) => Probe {
            truth,
            pin: PinCurve::from(
                PinEllipse::create_canonical_circle(k, &axes(&c.frame), c.radius, None, None)
                    .unwrap(),
            ),
            window: None,
        },
        CurveKind::Ellipse(e) => Probe {
            truth,
            pin: PinCurve::from(
                PinEllipse::create_canonical(k, &axes(&e.frame), e.semi_x, e.semi_y, None, None)
                    .unwrap(),
            ),
            window: None,
        },
        CurveKind::Parabola(_) | CurveKind::Hyperbola(_) => {
            unreachable!("the probes are lines and closed conics")
        }
    }
}

/// A point's distance from the truth curve, to first order.
fn curve_distance(c: &CurveKind, p: P) -> f64 {
    let conic = |f: &Frame, a: f64, b: f64| {
        let l = sub(p, f.origin);
        let (x, y, z) = (dot(l, f.x), dot(l, f.y), dot(l, f.z));
        let g = x * x / (a * a) + y * y / (b * b) - 1.0;
        let (a2, b2) = (a * a, b * b);
        let grad = (4.0 * x * x / (a2 * a2) + 4.0 * y * y / (b2 * b2)).sqrt();
        let off = g / grad;
        (off * off + z * z).sqrt()
    };
    match c {
        CurveKind::Line(l) => {
            let d = l.direction;
            let q = sub(p, l.origin);
            let t = dot(q, d) / dot(d, d);
            dist(q, d.map(|x| x * t))
        }
        CurveKind::Circle(k) => conic(&k.frame, k.radius, k.radius),
        CurveKind::Ellipse(e) => conic(&e.frame, e.semi_x, e.semi_y),
        CurveKind::Parabola(_) | CurveKind::Hyperbola(_) => {
            unreachable!("the probes are lines and closed conics")
        }
    }
}

fn line_through(rng: &mut Rng, p: P) -> CurveKind {
    CurveKind::Line(Line {
        origin: p,
        direction: sample::direction(rng),
    })
}

/// The surface's normal at a point on it (unnormalised): the implicit form's gradient, in
/// the frame.
fn normal(s: &SurfaceKind, p: P) -> P {
    let f = frame_of(s);
    let l = sub(p, f.origin);
    let (x, y, z) = (dot(l, f.x), dot(l, f.y), dot(l, f.z));
    let world =
        |a: f64, b: f64, c: f64| core::array::from_fn(|i| a * f.x[i] + b * f.y[i] + c * f.z[i]);
    match s {
        SurfaceKind::Plane(_) => world(0.0, 0.0, 1.0),
        SurfaceKind::Cylinder(_) => world(x, y, 0.0),
        SurfaceKind::Cone(c) => {
            let (sin, cos) = dd::sin_cos(Dd::new(c.semi_angle));
            let k = (sin / cos).to_f64();
            world(x, y, -k * (c.radius + k * z))
        }
        SurfaceKind::Sphere(_) => world(x, y, z),
        SurfaceKind::Torus(c) => {
            let rho = (x * x + y * y).sqrt();
            let s = 1.0 - c.major / rho;
            world(x * s, y * s, z)
        }
    }
}

/// The curve's tangent at a point near it (unnormalised).
fn tangent(c: &CurveKind, p: P) -> P {
    let conic = |f: &Frame, a: f64, b: f64| {
        let l = sub(p, f.origin);
        let (x, y) = (dot(l, f.x), dot(l, f.y));
        let (tx, ty) = (-a * y / b, b * x / a);
        core::array::from_fn(|i| tx * f.x[i] + ty * f.y[i])
    };
    match c {
        CurveKind::Line(l) => l.direction,
        CurveKind::Circle(k) => conic(&k.frame, k.radius, k.radius),
        CurveKind::Ellipse(e) => conic(&e.frame, e.semi_x, e.semi_y),
        CurveKind::Parabola(_) | CurveKind::Hyperbola(_) => {
            unreachable!("the probes are lines and closed conics")
        }
    }
}

fn cross(a: P, b: P) -> P {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

/// The sine of the angle the curve crosses the surface's tangent plane at, near `p`.
fn crossing_sine(c: &CurveKind, s: &SurfaceKind, p: P) -> f64 {
    let (t, n) = (tangent(c, p), normal(s, p));
    dot(t, n).abs() / (dot(t, t) * dot(n, n)).sqrt()
}

/// How far from a true crossing a point found to `HIT_TOL` may be: `HIT_TOL / sin`,
/// between `HIT_TOL` and `TANGENT_TOL` (a near-tangent pair's points merge, a near miss
/// is a touch).
fn crossing_tol(sine: f64) -> f64 {
    (HIT_TOL / sine).clamp(HIT_TOL, TANGENT_TOL)
}

/// A line tangent to `s` at `p`: along a direction perpendicular to the normal there.
fn tangent_line(rng: &mut Rng, s: &SurfaceKind, p: P) -> CurveKind {
    let d = cross(normal(s, p), sample::direction(rng));
    CurveKind::Line(Line {
        origin: p,
        direction: d.map(|v| v / dot(d, d).sqrt()),
    })
}

fn random_surface(rng: &mut Rng, i: usize) -> SurfaceKind {
    let f = sample::frame(rng, 1.0);
    match i % 5 {
        0 => SurfaceKind::Plane(Plane { frame: f }),
        1 => SurfaceKind::Cylinder(Cylinder {
            frame: f,
            radius: 0.25 + rng.next_f64(),
        }),
        2 => SurfaceKind::Cone(Cone {
            frame: f,
            radius: 0.25 + rng.next_f64(),
            semi_angle: 0.1 + 1.0 * rng.next_f64(),
        }),
        3 => SurfaceKind::Sphere(Sphere {
            frame: f,
            radius: 0.25 + rng.next_f64(),
        }),
        _ => {
            let minor = 0.25 + 0.5 * rng.next_f64();
            SurfaceKind::Torus(Torus {
                frame: f,
                major: minor + 0.25 + rng.next_f64(),
                minor,
            })
        }
    }
}

/// A point on `s` inside its pin window.
fn point_in(rng: &mut Rng, b: &Bounded) -> P {
    loop {
        let p = sample::point_on(rng, &b.truth);
        if b.inside(p, 0.25) {
            return p;
        }
    }
}

fn check_curve_surface(k: &Kernel, what: &str, probe: &Probe, b: &Bounded, t: &mut Tally) {
    t.inputs += 1;
    let truth = match cs::intersect(&probe.truth, &b.truth).unwrap() {
        Meeting::Points(h) => h,
        Meeting::OnSurface => {
            t.notes
                .push(format!("{what}: the curve lies on the surface"));
            t.inputs -= 1;
            return;
        }
    };
    let in_window = |p: P, tt: f64, margin: f64| {
        b.inside(p, margin) && probe.window.is_none_or(|w| tt.abs() <= w - margin)
    };
    let pin =
        match intersect_curve_surface(k, &probe.pin, &b.pin) {
            Ok(h) => {
                if !h.curves.is_empty() {
                    // An overlap is right when the curve is on the surface along it.
                    let mut off = 0.0f64;
                    for c in &h.curves {
                        let ivl = c.natural_interval();
                        for j in 0..=32 {
                            let s = ivl.min() + (ivl.max() - ivl.min()) * f64::from(j) / 32.0;
                            let q = c.evaluate_point(k, s).map_or([f64::NAN; 3], of);
                            off = off.max(cs::distance(&b.truth, q).unwrap_or(f64::NAN));
                        }
                    }
                    if off <= CURVE_TOL {
                        t.within += 1;
                        t.notes.push(format!(
                            "{what}: pin reports {} overlaps, on the surface to {off:e}; truth {} \
                         points",
                            h.curves.len(),
                            truth.len()
                        ));
                    } else {
                        t.defect(format!(
                        "{what}: pin reports {} overlaps, off the surface by {off:e}; truth {:?}",
                        h.curves.len(),
                        truth.iter().map(|h| (hi(h.point), h.contact)).collect::<Vec<_>>()
                    ));
                    }
                    return;
                }
                h.points.into_iter().map(of).collect::<Vec<P>>()
            }
            Err(e) => {
                if truth
                    .iter()
                    .any(|h| in_window(hi(h.point), h.t.to_f64(), MARGIN))
                {
                    t.defect(format!(
                        "{what}: pin error {e}; truth {:?}",
                        truth
                            .iter()
                            .map(|h| (hi(h.point), h.contact))
                            .collect::<Vec<_>>()
                    ));
                } else {
                    t.exact += 1;
                }
                return;
            }
        };
    let mut worst = 0.0f64;
    let mut bad = Vec::new();
    for h in &truth {
        let p = hi(h.point);
        if !in_window(p, h.t.to_f64(), MARGIN) {
            continue;
        }
        let d = pin
            .iter()
            .map(|&q| dist(p, q))
            .fold(f64::INFINITY, f64::min);
        let tol = match h.contact {
            cs::Contact::Crossing => crossing_tol(crossing_sine(&probe.truth, &b.truth, p)),
            cs::Contact::Tangent => TANGENT_TOL,
        };
        worst = worst.max(d);
        if d > tol {
            bad.push(format!(
                "missed {:?} truth point {p:?} (nearest pin at {d:e})",
                h.contact
            ));
        }
    }
    for &q in &pin {
        if !in_window(q, 0.0, -MARGIN) && probe.window.is_none() {
            continue;
        }
        let d = truth
            .iter()
            .map(|h| dist(hi(h.point), q))
            .fold(f64::INFINITY, f64::min);
        let sine = crossing_sine(&probe.truth, &b.truth, q);
        let tol = truth
            .iter()
            .min_by(|a, c| dist(hi(a.point), q).total_cmp(&dist(hi(c.point), q)))
            .map_or(HIT_TOL, |h| match h.contact {
                cs::Contact::Crossing => crossing_tol(sine),
                cs::Contact::Tangent => TANGENT_TOL,
            });
        if d > tol {
            let on_surface = cs::distance(&b.truth, q).unwrap();
            let on_curve = curve_distance(&probe.truth, q);
            if on_surface <= HIT_TOL && on_curve <= HIT_TOL {
                // On both to first order: near a true crossing by `(gaps) / sin`, when the
                // crossing is transversal. Past the truth's points by more, the truth missed
                // it; at a near-tangency it is a touch, within tolerance.
                let bound = (on_surface + on_curve) / sine;
                if sine >= 1e-3 && 10.0 * bound < d {
                    t.mismatches.push(format!(
                        "{what}: pin point {q:?} on both (surface {on_surface:e}, curve \
                         {on_curve:e}, crossing sine {sine:e}), nearest truth point at {d:e}"
                    ));
                    return;
                }
                t.notes.push(format!(
                    "{what}: pin point {q:?} a near-tangent touch (sine {sine:e}, gaps \
                     {on_surface:e} {on_curve:e}); nearest truth point at {d:e}"
                ));
                worst = worst.max(on_surface.max(on_curve).max(HIT_TOL));
                continue;
            }
            worst = worst.max(d);
            bad.push(format!(
                "pin point {q:?} off the truth (nearest truth point {d:e}, surface \
                 {on_surface:e}, curve {on_curve:e})"
            ));
        } else {
            worst = worst.max(d);
        }
    }
    if !bad.is_empty() {
        t.defect(format!("{what}: {}", bad.join("; ")));
    } else if worst <= EXACT {
        t.exact += 1;
    } else {
        t.within += 1;
    }
}

/// The pin's intersector defects (research 64 §5.3), by input, until scx ports these
/// intersectors and they become `pin_defects.rs` entries. scx keeps no fixed-tolerance
/// overlap (ruling 11-27): an on-surface answer is exact or certified.
///
/// None in the current inputs. Defects 1 and 2 were found in this test's first draft (60
/// inputs, the same seed): a line across a thin cylinder (r `0.30200`) and a line tangent
/// to a sphere (r `1.02166`), each reported as an overlap off the surface by `2.57e-4` and
/// `6.62e-4`, within the pin's fixed `0.001` curve-on-surface tolerance, where the truth
/// has two crossings and no points.
pub const KNOWN_CURVE_SURFACE: &[usize] = &[];

/// Defect 3, input #2: a line near-tangent to a cone (semi-angle `0.75578`) that the
/// truth crosses twice, `1.1e-8` apart, which the pin reports as no intersection.
pub const KNOWN_TANGENT_LINES: &[usize] = &[2];

/// The pin's `intersect_curve_surface` against T2: a line, a circle and an ellipse through
/// a point of each surface kind, crossing at random.
#[test]
fn pin_curve_surface_against_t2() {
    let mut rng = Rng::new(0x7264);
    let k = Kernel::default();
    let mut t = Tally::default();
    for i in 0..15 {
        let b = Bounded::new(&k, random_surface(&mut rng, i));
        let p = point_in(&mut rng, &b);
        let (what, curve) = match i / 5 {
            0 => ("line", line_through(&mut rng, p)),
            1 => ("circle", sample::circle_through(&mut rng, p)),
            _ => ("ellipse", sample::ellipse_through(&mut rng, p)),
        };
        let what = format!("{what} through {p:?} and {:?}", b.truth);
        check_curve_surface(&k, &what, &probe(&k, curve), &b, &mut t);
    }
    t.report("curve-surface", KNOWN_CURVE_SURFACE);
}

/// The pin's `intersect_curve_surface` against T2 on a line tangent to each surface kind:
/// the f64 line is only near-tangent, so the truth may cross twice, touch or miss.
#[test]
fn pin_tangent_lines_against_t2() {
    let mut rng = Rng::new(0x7265);
    let k = Kernel::default();
    let mut t = Tally::default();
    for i in 0..5 {
        let b = Bounded::new(&k, random_surface(&mut rng, i));
        let p = point_in(&mut rng, &b);
        let what = format!("tangent line through {p:?} and {:?}", b.truth);
        let curve = tangent_line(&mut rng, &b.truth, p);
        check_curve_surface(&k, &what, &probe(&k, curve), &b, &mut t);
    }
    t.report("tangent lines", KNOWN_TANGENT_LINES);
}

// ---------------------------------------------------------------- surface-surface

/// The parameter range a truth curve is sampled over, and how densely.
fn truth_range(c: &ssi::Curve) -> (f64, f64) {
    match c {
        ssi::Curve::Point(_) => (0.0, 0.0),
        ssi::Curve::Circle { .. } | ssi::Curve::Ellipse { .. } => {
            (0.0, 2.0 * dd::consts().pi.to_f64())
        }
        ssi::Curve::Line { direction, .. } => {
            let l = hi(*direction);
            let s = 4.0 * HALF / dot(l, l).sqrt();
            (-s, s)
        }
        ssi::Curve::Hyperbola { .. } => (-4.0, 4.0),
    }
}

fn kind_name(c: &ssi::Curve) -> &'static str {
    match c {
        ssi::Curve::Point(_) => "point",
        ssi::Curve::Line { .. } => "line",
        ssi::Curve::Circle { .. } => "circle",
        ssi::Curve::Ellipse { .. } => "ellipse",
        ssi::Curve::Hyperbola { .. } => "hyperbola",
    }
}

fn check_surfaces(k: &Kernel, what: &str, a: &Bounded, b: &Bounded, t: &mut Tally) {
    let section = match ssi::intersect(&a.truth, &b.truth) {
        Ok(s) => s,
        Err(e) => {
            t.notes.push(format!("{what}: truth {e}"));
            return;
        }
    };
    t.inputs += 1;
    let pieces = match &section {
        Section::Empty => Vec::new(),
        Section::Coincident => {
            t.notes.push(format!("{what}: coincident"));
            t.inputs -= 1;
            return;
        }
        Section::Curves(p) => p.clone(),
    };
    let (curves, status) = intersect_surfaces_within(k, &a.pin, &b.pin, Some(APPROX));
    let pin_kinds: Vec<&str> = curves.iter().map(|c| c.kind().name()).collect();
    let tracks: Vec<Track<'_>> = curves
        .iter()
        .map(|c| {
            let ivl = c.natural_interval();
            let f: Box<dyn Fn(f64) -> P> =
                Box::new(move |s: f64| c.evaluate_point(k, s).map_or([f64::NAN; 3], of));
            Track::new(f, ivl.min(), ivl.max())
        })
        .collect();
    let mut worst = 0.0f64;
    let mut bad = Vec::new();
    let mut truth_inside = 0;
    for piece in &pieces {
        let (lo, hi_t) = truth_range(&piece.curve);
        let n = if matches!(piece.curve, ssi::Curve::Point(_)) {
            0
        } else {
            64
        };
        let mut missed = 0;
        let mut far = 0.0f64;
        let mut far_at = [0.0; 3];
        for j in 0..=n {
            let s = lo + (hi_t - lo) * j as f64 / n.max(1) as f64;
            let p = hi(piece.curve.point(Dd::new(s)));
            if !(a.inside(p, MARGIN) && b.inside(p, MARGIN)) {
                continue;
            }
            truth_inside += 1;
            let d = tracks
                .iter()
                .map(|track| track.nearest(p))
                .fold(f64::INFINITY, f64::min);
            worst = worst.max(d);
            if d > CURVE_TOL {
                missed += 1;
                if d > far {
                    (far, far_at) = (d, p);
                }
            }
        }
        if missed > 0 {
            bad.push(format!(
                "missed {missed} points of the truth's {} {:?} {} (farthest {far:e} from the \
                 pin's curves, at {far_at:?}, in the frames {:?} and {:?})",
                kind_name(&piece.curve),
                piece.contact,
                fmt_curve(&piece.curve),
                a.coords(far_at),
                b.coords(far_at)
            ));
        }
    }
    for (ci, track) in tracks.iter().enumerate() {
        let mut off = 0.0f64;
        for &q in track.points.iter().step_by(4) {
            let g = cs::distance(&a.truth, q)
                .unwrap()
                .max(cs::distance(&b.truth, q).unwrap());
            off = off.max(g);
        }
        worst = worst.max(off);
        if off > CURVE_TOL {
            bad.push(format!(
                "pin curve {ci} ({}) is off the surfaces by {off:e}",
                pin_kinds[ci]
            ));
        } else if pieces.is_empty() {
            t.mismatches.push(format!(
                "{what}: pin curve {ci} ({}) on both surfaces, truth empty",
                pin_kinds[ci]
            ));
            return;
        }
    }
    if let Err(e) = &status
        && (truth_inside > 0 || !curves.is_empty())
    {
        bad.push(format!("pin status {e}"));
    }
    let truth_kinds: Vec<String> = pieces
        .iter()
        .map(|p| format!("{} {:?}", kind_name(&p.curve), p.contact))
        .collect();
    if !bad.is_empty() {
        let ends: Vec<String> = tracks
            .iter()
            .map(|t| {
                let ends = dist(t.points[0], t.points[SAMPLES]);
                format!("[{}, {}] ends {ends:e} apart", t.lo, t.hi)
            })
            .collect();
        bad.push(format!("pin curves {}", ends.join(", ")));
        t.defect(format!(
            "{what}: truth [{}], pin {pin_kinds:?} ({status:?}): {}",
            truth_kinds.join(", "),
            bad.join("; ")
        ));
    } else if worst <= EXACT {
        t.exact += 1;
    } else {
        t.within += 1;
    }
    if truth_inside > 0 {
        t.notes.push(format!(
            "{what}: truth [{}], pin {pin_kinds:?}",
            truth_kinds.join(", ")
        ));
    }
}

fn fmt_curve(c: &ssi::Curve) -> String {
    match c {
        ssi::Curve::Point(p) => format!("at {:?}", hi(*p)),
        ssi::Curve::Line { origin, direction } => {
            format!("through {:?} along {:?}", hi(*origin), hi(*direction))
        }
        ssi::Curve::Circle { centre, a, b }
        | ssi::Curve::Ellipse { centre, a, b }
        | ssi::Curve::Hyperbola { centre, a, b } => {
            format!("centre {:?} a {:?} b {:?}", hi(*centre), hi(*a), hi(*b))
        }
    }
}

/// The pin's `intersect_surfaces` against T5 on planes: plane-plane, and planes through a
/// point of a cylinder, cone or sphere (WR-680's oblique cylinder sections among them).
#[test]
fn pin_plane_sections_against_t5() {
    let mut rng = Rng::new(0x7564);
    let k = Kernel::default();
    let mut t = Tally::default();
    for i in 0..12 {
        let q = match i % 4 {
            0 => SurfaceKind::Plane(Plane {
                frame: sample::frame(&mut rng, 1.0),
            }),
            j => random_surface(&mut rng, j),
        };
        let qb = Bounded::new(&k, q);
        let p = point_in(&mut rng, &qb);
        let plane = Bounded::new(&k, SurfaceKind::Plane(sample::plane_through(&mut rng, p)));
        let what = format!("{:?} and {:?}", q, plane.truth);
        check_surfaces(&k, &what, &qb, &plane, &mut t);
    }
    t.report("plane sections", &[]);
}

/// The pin's `intersect_surfaces` against T5 on sphere pairs, coaxial pairs (frames on the
/// world axes, shifted by dyadic steps) and planes across a torus's axis.
#[test]
fn pin_revolved_pairs_against_t5() {
    let mut rng = Rng::new(0x7565);
    let k = Kernel::default();
    let mut t = Tally::default();
    for i in 0..12 {
        let f = sample::axis_frame(&mut rng);
        let shift = (rng.next_signed_unit() * 16.0).round() / 16.0;
        let (a, b) = match i % 3 {
            0 => {
                let g = sample::frame(&mut rng, 1.0);
                let o = core::array::from_fn(|j| g.origin[j] + 0.75 * rng.next_signed_unit());
                (
                    SurfaceKind::Sphere(Sphere {
                        frame: g,
                        radius: 0.5 + rng.next_f64(),
                    }),
                    SurfaceKind::Sphere(Sphere {
                        frame: Frame { origin: o, ..g },
                        radius: 0.5 + rng.next_f64(),
                    }),
                )
            }
            1 => (
                sample::revolved(&mut rng, f),
                sample::revolved(&mut rng, sample::along(f, shift)),
            ),
            _ => {
                let minor = 0.25 + 0.5 * rng.next_f64();
                (
                    SurfaceKind::Torus(Torus {
                        frame: f,
                        major: minor + 0.25 + rng.next_f64(),
                        minor,
                    }),
                    SurfaceKind::Plane(Plane {
                        frame: sample::along(f, shift * minor),
                    }),
                )
            }
        };
        let (ab, bb) = (Bounded::new(&k, a), Bounded::new(&k, b));
        let what = format!("{a:?} and {b:?}");
        check_surfaces(&k, &what, &ab, &bb, &mut t);
    }
    t.report("revolved pairs", &[]);
}

/// Tangent and degenerate sections: planes tangent to a sphere, along a cylinder and
/// through a cone's apex; spheres touching outside and inside.
#[test]
fn pin_tangent_sections_against_t5() {
    let k = Kernel::default();
    let mut t = Tally::default();
    let frame = |o: P, x: P, y: P| Frame {
        origin: o,
        x,
        y,
        z: [
            x[1] * y[2] - x[2] * y[1],
            x[2] * y[0] - x[0] * y[2],
            x[0] * y[1] - x[1] * y[0],
        ],
    };
    let world = frame([0.0; 3], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]);
    let sphere = |o: P, r: f64| {
        SurfaceKind::Sphere(Sphere {
            frame: Frame { origin: o, ..world },
            radius: r,
        })
    };
    let horizontal = |z: f64| {
        SurfaceKind::Plane(Plane {
            frame: Frame {
                origin: [0.0, 0.0, z],
                ..world
            },
        })
    };
    let cases = [
        (
            "plane on a sphere's pole",
            sphere([0.0; 3], 1.0),
            horizontal(1.0),
        ),
        (
            "plane along a cylinder",
            SurfaceKind::Cylinder(Cylinder {
                frame: world,
                radius: 1.0,
            }),
            SurfaceKind::Plane(Plane {
                frame: frame([1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]),
            }),
        ),
        (
            "plane through a cylinder's axis",
            SurfaceKind::Cylinder(Cylinder {
                frame: world,
                radius: 1.0,
            }),
            SurfaceKind::Plane(Plane {
                frame: frame([0.0; 3], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]),
            }),
        ),
        (
            "spheres touching outside",
            sphere([0.0; 3], 1.0),
            sphere([2.0, 0.0, 0.0], 1.0),
        ),
        (
            "spheres touching inside",
            sphere([0.0; 3], 2.0),
            sphere([1.0, 0.0, 0.0], 1.0),
        ),
        (
            "concentric spheres",
            sphere([0.0; 3], 1.0),
            sphere([0.0; 3], 2.0),
        ),
    ];
    for (what, a, b) in cases {
        let (ab, bb) = (Bounded::new(&k, a), Bounded::new(&k, b));
        check_surfaces(&k, what, &ab, &bb, &mut t);
    }
    t.report("tangent sections", &[]);
}
