// Written for solidcorex. Apache-2.0.
//! scx-ssi's sphere and coaxial pairs (`analytic::intersect`, research 63 SS3) against the
//! pin's `Sphere::intersect_with_sphere`, `Sphere::intersect_with_cone` and
//! `Cone::intersect_with_cone`, on the same surfaces and extents.
//!
//! The surfaces are `t_ssi`'s (the pin's canonical kinds, the cylinders and cones bounded
//! over a full turn and their heights), each over its whole domain. The pin has no torus
//! pair in closed form, so the tori are left to the truth tests (`scx-ssi`'s
//! `tests/coaxial.rs`). Every frame's axes are signed world axes and every origin is on a
//! grid of eighths, the second's on the first's axis (two spheres anywhere), so the exact
//! sections (testkit T5) answer each pair for the check of the disagreements. Both sides
//! take the pin's default tolerance, `EFF_ZERO_SQRT`. The answers are compared as
//! `t_ssi`'s fingerprints.

use crate::harness::{Gen, SplitMix64, run_defect_tol};
use crate::t_ssi::{self, TOL};
use crate::t_surface::Kind;

/// The bounded cylinders' height (`t_surface`'s).
const HEIGHT: f64 = 6.0;

/// One input: two kinds, each in a frame (origin, z, x), and what the pool aimed at (for
/// the report only).
#[derive(Clone, Debug)]
pub struct Case {
    pub a: (Kind, [[f64; 3]; 3]),
    pub b: (Kind, [[f64; 3]; 3]),
    #[expect(dead_code, reason = "read through `Debug`, in the report")]
    pub aim: &'static str,
}

/// `t_ssi`'s input for one side (its plane unused).
fn side((kind, frame): (Kind, [[f64; 3]; 3])) -> t_ssi::Case {
    t_ssi::Case {
        kind,
        frame,
        plane: frame,
        half: 1.0,
        aim: "",
    }
}

const AXES: [[f64; 3]; 3] = [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]];

fn signed(rng: &mut SplitMix64, v: [f64; 3]) -> [f64; 3] {
    if rng.below(2) == 0 { v } else { v.map(|t| -t) }
}

/// A signed world axis for z and another for x.
fn axes(rng: &mut SplitMix64) -> ([f64; 3], [f64; 3]) {
    let i = rng.below(3);
    let j = (i + 1 + rng.below(2)) % 3;
    (signed(rng, AXES[i]), signed(rng, AXES[j]))
}

/// A signed world axis square to `z` (itself a signed world axis).
fn square_to(rng: &mut SplitMix64, z: [f64; 3]) -> [f64; 3] {
    let i = (0..3).find(|&i| z[i] != 0.0).unwrap_or(2);
    let j = (i + 1 + rng.below(2)) % 3;
    signed(rng, AXES[j])
}

/// A multiple of an eighth in `[lo, hi]`.
fn grid(rng: &mut SplitMix64, lo: f64, hi: f64) -> f64 {
    (rng.uniform(lo, hi) * 8.0).round() / 8.0
}

fn add(o: [f64; 3], z: [f64; 3], t: f64) -> [f64; 3] {
    [o[0] + z[0] * t, o[1] + z[1] * t, o[2] + z[2] * t]
}

/// A radius on a grid of quarters, so pairs share radii often.
fn radius(rng: &mut SplitMix64) -> f64 {
    (rng.uniform(0.5, 4.0) * 4.0).round() / 4.0
}

fn cone(rng: &mut SplitMix64, r: f64) -> Kind {
    let a = [0.25, 0.5, 0.75][rng.below(3)] * if rng.below(4) == 0 { -1.0 } else { 1.0 };
    let k = scx_num::fmath::tan(a);
    let h = if r + k * HEIGHT > 0.2 {
        HEIGHT
    } else {
        0.8 * r / -k
    };
    Kind::Cone { r, a, h }
}

impl Gen for Case {
    fn pool(rng: &mut SplitMix64) -> Vec<Self> {
        let mut out = Vec::new();
        for i in 0..900 {
            let o = [0; 3].map(|_| grid(rng, -4.0, 4.0));
            let (z, x) = axes(rng);
            let r = radius(rng);
            // A shared radius a third of the time: tangent and coincident pairs.
            let rb = if rng.below(3) == 0 { r } else { radius(rng) };
            let (aim, ka, kb) = match i % 6 {
                0 => ("spheres", Kind::Sphere { r }, Kind::Sphere { r: rb }),
                1 => (
                    "sphere-cylinder",
                    Kind::Sphere { r },
                    Kind::Cylinder { r: rb },
                ),
                2 => ("sphere-cone", Kind::Sphere { r }, cone(rng, rb)),
                3 => ("cylinders", Kind::Cylinder { r }, Kind::Cylinder { r: rb }),
                4 => ("cylinder-cone", Kind::Cylinder { r }, cone(rng, rb)),
                _ => ("cones", cone(rng, r), cone(rng, rb)),
            };
            let (ob, zb, xb) = if i % 6 == 0 {
                let ob = [0; 3].map(|_| grid(rng, -2.0 * r, 2.0 * r));
                let (zb, xb) = axes(rng);
                (add(o, ob, 1.0), zb, xb)
            } else {
                // On the first's axis, at its origin a quarter of the time.
                let t = if rng.below(4) == 0 {
                    0.0
                } else {
                    grid(rng, -HEIGHT, HEIGHT)
                };
                let zb = signed(rng, z);
                (add(o, z, t), zb, square_to(rng, zb))
            };
            out.push(Case {
                a: (ka, [o, z, x]),
                b: (kb, [ob, zb, xb]),
                aim,
            });
        }
        out
    }
}

/// scx's side.
pub mod x {
    use super::{Case, side};
    use crate::t_ssi::{self, Print, SNAP};
    use scx_num::units::{Accuracy, Length, Resolution};
    use scx_ssi::analytic::{Intersection, intersect};
    use scx_surface::Closed;

    pub fn answer(c: &Case) -> Closed<Intersection> {
        let (a, ae) = t_ssi::x::surface(&side(c.a));
        let (b, be) = t_ssi::x::surface(&side(c.b));
        let acc = Accuracy::new(Length::from_mm(SNAP)).unwrap();
        let res = Resolution::new(Length::from_mm(SNAP)).unwrap();
        intersect(&a, &ae, &b, &be, [acc, acc], res)
    }

    pub fn print(c: &Case) -> Print {
        t_ssi::x::print(&answer(c))
    }
}

/// The pin's side.
pub mod s {
    use super::{Case, HEIGHT, side};
    use crate::t_ssi::{self, Print, SNAP};
    use crate::t_surface::Kind;
    use solidcore::Kernel;
    use solidcore::geom::extent2d::Extent2d;
    use solidcore::surface::cone::Cone;
    use solidcore::surface::sphere::Sphere;
    use solidcore::surface::surface::p2::{Surface, SurfaceIntersection};
    use solidcore::{Point3, Vec3};

    enum Pin {
        Sphere(Sphere, Extent2d),
        Cone(Cone, Extent2d),
    }

    fn pin(k: &Kernel, (kind, f): (Kind, [[f64; 3]; 3])) -> Option<Pin> {
        let frame = side((kind, f)).surface_frame();
        let o = frame.origin();
        let (x, y) = (*frame.x(), *frame.y());
        let (o, x, y) = (
            Point3::new(o.x, o.y, o.z),
            Vec3::new(x.x, x.y, x.z),
            Vec3::new(y.x, y.y, y.z),
        );
        Some(match kind {
            Kind::Sphere { r } => {
                let place = solidcore::geom::axis2_placement::Axis2Placement::new(o, x, y);
                let s = Sphere::create_canonical(k, &place, r).ok()?;
                let d = Surface::from(s.clone())
                    .natural_uv_domain_with_nurb(k)
                    .ok()?;
                Pin::Sphere(s, d)
            }
            Kind::Cylinder { r } => {
                let mut s =
                    Cone::new_cylinder_bounded(o, x, y, r, 0.0, 360.0, HEIGHT, false, false, true);
                s.make_nurb(k).ok()?;
                let d = Surface::from(s.clone())
                    .natural_uv_domain_with_nurb(k)
                    .ok()?;
                Pin::Cone(s, d)
            }
            Kind::Cone { r, a, h } => {
                let top = r + scx_num::fmath::tan(a) * h;
                let mut s = Cone::new_bounded(o, x, y, r, top, 0.0, 360.0, h, false, false, true);
                s.make_nurb(k).ok()?;
                let d = Surface::from(s.clone())
                    .natural_uv_domain_with_nurb(k)
                    .ok()?;
                Pin::Cone(s, d)
            }
            _ => return None,
        })
    }

    pub fn answer(c: &Case) -> Option<(bool, SurfaceIntersection)> {
        let k = Kernel::default();
        let (a, b) = (pin(&k, c.a)?, pin(&k, c.b)?);
        let tol = Some(SNAP);
        match (&a, &b) {
            (Pin::Sphere(s, sd), Pin::Sphere(t, td)) => s.intersect_with_sphere(&k, sd, t, td, tol),
            (Pin::Sphere(s, sd), Pin::Cone(t, td)) => s.intersect_with_cone(&k, sd, t, td, tol),
            (Pin::Cone(s, sd), Pin::Sphere(t, td)) => t.intersect_with_cone(&k, td, s, sd, tol),
            (Pin::Cone(s, sd), Pin::Cone(t, td)) => s.intersect_with_cone(&k, sd, t, td, tol),
        }
        .ok()
    }

    pub fn print(c: &Case) -> Print {
        t_ssi::s::print(&answer(c))
    }
}

/// The check against the exact sections (testkit T5) for the inputs where scx and the pin
/// disagree (`pin_defects::ssi_coaxial`): scx's answer is judged on its own.
pub mod truth {
    use super::{Case, HEIGHT, side};
    use crate::t_ssi::truth::{absorbed, depth, distance_to, kind_of, on_tol, overlap};
    use crate::t_surface::Kind;
    use core::f64::consts::TAU;
    use scx_num::frame::Frame;
    use scx_num::vector3d::Point3;
    use scx_ssi::analytic::{Contact, Intersection};
    use scx_surface::Closed;
    use scx_testkit::curve_surface as cs;
    use scx_testkit::dd::Dd;
    use scx_testkit::ssi::{self, Section};

    /// How far inside both extents a truth point must be to be owed.
    const MARGIN: f64 = 1e-6;
    /// How near an answer's piece an owed truth point must be, relative to `1 + |p|`.
    const NEAR: f64 = 1e-7;

    fn local(f: &Frame, q: Point3) -> [f64; 3] {
        let d = q - f.origin();
        [d.dot(*f.x()), d.dot(*f.y()), d.dot(*f.z())]
    }

    /// Whether `q` (on the surface) is inside one side's whole extent, by `tol` (negative:
    /// by that much inside): only heights bound them, and a cone's nappe.
    fn inside_one((kind, f): (Kind, [[f64; 3]; 3]), q: Point3, tol: f64) -> bool {
        let frame = side((kind, f)).surface_frame();
        let within = |x: f64, lo: f64, hi: f64| x >= lo - tol && x <= hi + tol;
        let z = local(&frame, q)[2];
        match kind {
            Kind::Cylinder { .. } => within(z, 0.0, HEIGHT),
            Kind::Cone { r, a, h } => within(z, 0.0, h) && r + scx_num::fmath::tan(a) * z >= -tol,
            _ => true,
        }
    }

    fn inside(c: &Case, q: Point3, tol: f64) -> bool {
        inside_one(c.a, q, tol) && inside_one(c.b, q, tol)
    }

    fn tpoint(q: [Dd; 3]) -> Point3 {
        Point3::new(q[0].hi, q[1].hi, q[2].hi)
    }

    /// The distance from `q` to a truth circle.
    fn near_circle(c: &ssi::Curve, q: Point3) -> f64 {
        let ssi::Curve::Circle { centre, a, b } = *c else {
            return f64::INFINITY;
        };
        let v = |w: [Dd; 3]| tpoint(w).to_vec();
        let (o, a, b) = (tpoint(centre), v(a), v(b));
        let n = *a.cross(b).normalized().unwrap();
        let d = q - o;
        let up = d.dot(n);
        scx_num::fmath::hypot(up, (d - n * up).length() - a.length())
    }

    /// `Err` with what is wrong when scx's answer is not the exact section's.
    pub fn check(c: &Case) -> Result<(), String> {
        let (fa, fb) = (side(c.a).surface_frame(), side(c.b).surface_frame());
        let (ka, kb) = (kind_of(c.a.0, &fa), kind_of(c.b.0, &fb));
        let truth = ssi::intersect(&ka, &kb);
        let got = super::x::answer(c);
        let (curves, points) = match &got {
            Closed::General => {
                return match &truth {
                    Err(ssi::Error::NoClosedForm | ssi::Error::Undecided) => Ok(()),
                    t => Err(format!("General where the truth is {t:?}")),
                };
            }
            Closed::Solved(Intersection::Coincident(pieces)) => {
                if truth != Ok(Section::Coincident) {
                    return Err(format!("coincident; the truth is {truth:?}"));
                }
                let a = crate::t_ssi::x::surface(&side(c.a));
                let (b, _) = crate::t_ssi::x::surface(&side(c.b));
                return overlap((&a.0, &a.1), &b, |q, tol| inside_one(c.b, q, tol), pieces);
            }
            Closed::Solved(Intersection::Empty) => (&[][..], &[][..]),
            Closed::Solved(Intersection::Pieces { curves, points }) => (&curves[..], &points[..]),
            _ => return Err("an unknown answer".to_owned()),
        };
        let tpieces = match &truth {
            Ok(Section::Curves(t)) => t.clone(),
            Ok(Section::Empty) => Vec::new(),
            // The truth's cone has both nappes: one double cone, the extents' nappes
            // meeting in the pieces (checked on the surfaces below) or nowhere.
            Ok(Section::Coincident) if matches!(c.a.0, Kind::Cone { .. }) => Vec::new(),
            t => return Err(format!("the truth is {t:?}")),
        };
        // On both surfaces: a crossing to rounding, a touch within the snap (63 SS4).
        let on = |q: Point3, contact: Contact| -> Result<(), String> {
            let da = cs::distance(&ka, [q.x, q.y, q.z]).map_err(|e| format!("{e:?}"))?;
            let db = cs::distance(&kb, [q.x, q.y, q.z]).map_err(|e| format!("{e:?}"))?;
            let tol = on_tol(contact, q);
            if da > tol || db > tol {
                return Err(format!("{q:?} off the surfaces by {da:e}, {db:e}"));
            }
            if !inside(c, q, 1e-7) {
                return Err(format!("{q:?} outside an extent"));
            }
            Ok(())
        };
        for k in curves {
            let (t0, t1) = (k.span.min(), k.span.max());
            for i in 0..=32 {
                on(
                    k.curve.point(t0 + (t1 - t0) * f64::from(i) / 32.0),
                    k.contact,
                )?;
            }
            let q = k.curve.point(t0);
            let near =
                |t: &&ssi::Piece| near_circle(&t.curve, q) <= NEAR * (1.0 + q.to_vec().length());
            let contact = |t: &ssi::Piece| match t.contact {
                ssi::Contact::Transversal => Contact::Transversal,
                _ => Contact::Tangent,
            };
            // The contact the truth's, or a touch standing for crossings within the snap
            // (the owed points judge it, 63 SS4).
            let matched = tpieces.is_empty()
                || k.contact == Contact::Tangent
                || tpieces.iter().filter(near).any(|t| contact(t) == k.contact);
            if !matched {
                return Err(format!("{k:?} is no truth circle of its contact"));
            }
        }
        for q in points {
            on(q.point, q.contact)?;
        }
        for piece in &tpieces {
            let (n, t0, t1) = match piece.curve {
                ssi::Curve::Point(_) => (1, 0.0, 0.0),
                _ => (720, 0.0, TAU),
            };
            for i in 0..n {
                let t = t0 + (t1 - t0) * f64::from(i) / f64::from(n);
                let q = tpoint(piece.curve.point(Dd::new(t)));
                if !inside(c, q, -MARGIN) {
                    continue;
                }
                let near = curves
                    .iter()
                    .map(|k| distance_to(&k.curve, k.span.min(), k.span.max(), q))
                    .chain(points.iter().map(|p| p.point.distance(q)))
                    .fold(f64::INFINITY, f64::min);
                if near > NEAR * (1.0 + q.to_vec().length())
                    && !absorbed(&ka, &kb, curves, points, q)
                {
                    return Err(format!("truth point {q:?} missed by {near:e}"));
                }
            }
        }
        depth(&ka, &kb, curves)
    }
}

#[test]
fn ssi_coaxial() {
    run_defect_tol(
        "ssi_coaxial",
        TOL,
        &crate::pin_defects::ssi_coaxial(),
        |(c,): (Case,)| x::print(&c),
        |(c,): (Case,)| s::print(&c),
    );
}
