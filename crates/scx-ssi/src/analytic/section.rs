// Written for solidcorex. Apache-2.0.
//! The plane sections of the analytic kinds (research 63 SS5; the table in [`super`]).
//!
//! Each kind works in its own axes ([`Axes`]: z along the axis times the sense), with the
//! plane's normal `n = (nx, ny, nz)` there and `d = n . (p0 - o)`, the plane's offset from
//! the kind's origin. Where the plane leans off the axis, `s = |(nx, ny)|`, `e_r` is the
//! horizontal unit vector along `(nx, ny)` and `e_t = z x e_r`: `e_t` lies in the plane.

use scx_num::extent2d::Extent2d;
use scx_num::fmath;
use scx_num::units::Length;
use scx_num::vector3d::{Point3, Vec3};
use scx_surface::{Closed, Cone, Cylinder, Plane, Sphere, Surface, Torus};

use super::Contact::{Tangent, Transversal};
use super::Intersection;
use super::clip::{Axes, Clip, Raw, Shape, Side, step_bounds};

/// A conic's semi-axis longer than this many units of its last place past the tolerance is
/// beyond the precision of the closed form: [`Closed::General`]. `2^-50`, a few units of
/// the last place.
const PRECISION: f64 = 8.881_784_197_001_252e-16;

/// What a kind's section found before clipping.
#[derive(Default)]
struct Found {
    raws: Vec<Raw>,
    touches: Vec<Point3>,
}

impl Found {
    fn conic(&mut self, c: Point3, a: Vec3, b: Vec3, circle: bool, contact: super::Contact) {
        self.raws
            .push(Raw::new(Shape::Conic { c, a, b, circle }, contact));
    }

    fn line(&mut self, o: Point3, d: Vec3, contact: super::Contact) -> Option<()> {
        let d = *d.normalized()?;
        self.raws.push(Raw::new(Shape::Line { o, d }, contact));
        Some(())
    }
}

/// The section of `s` over `se` by `plane` over `pe` within `tol`, its curves along
/// `sign (n_s x n_plane)`.
pub(super) fn section(
    s: &Surface,
    se: &Extent2d,
    plane: &Plane,
    pe: &Extent2d,
    tol: Length,
    sign: f64,
) -> Closed<Intersection> {
    let tol = tol.mm();
    let Some(clip) = Clip::new(s, se, plane, pe, tol, sign) else {
        return Closed::General;
    };
    let n = *plane.frame().z();
    let p0 = plane.frame().origin();
    let mut found = Found::default();
    let solved = match s {
        Surface::Plane(k) => match plane_plane(k, se, plane, pe, tol, &mut found) {
            Some(PlanePlane::Coincident) => {
                return Closed::Solved(Intersection::Coincident {
                    on_a: *se,
                    on_b: *pe,
                });
            }
            Some(PlanePlane::Other) => Some(()),
            None => None,
        },
        Surface::Cylinder(k) => cylinder(k, se, n, p0, tol, &mut found),
        Surface::Cone(k) => cone(k, se, n, p0, tol, &mut found),
        Surface::Sphere(k) => {
            sphere(k, plane, p0, tol, &mut found);
            Some(())
        }
        Surface::Torus(k) => torus(k, &clip, n, p0, tol, &mut found),
        _ => None,
    };
    if solved.is_none() {
        return Closed::General;
    }
    let (mut curves, mut points) = (Vec::new(), Vec::new());
    if clip.clip(found.raws, &mut curves, &mut points).is_none() {
        return Closed::General;
    }
    for p in found.touches {
        clip.point(p, Tangent, &mut points);
    }
    Closed::Solved(Intersection::pieces(curves, points))
}

/// The local normal, `s`, `e_r` and `e_t` (both zero when `s` is).
fn lean(axes: &Axes, n: Vec3) -> ([f64; 3], f64, [f64; 3], [f64; 3]) {
    let l = axes.local_vec(n);
    let [nx, ny, _] = l;
    let s = fmath::hypot(nx, ny);
    if s == 0.0 {
        return (l, s, [0.0; 3], [0.0; 3]);
    }
    (l, s, [nx / s, ny / s, 0.0], [-ny / s, nx / s, 0.0])
}

fn add(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}

fn scale(a: [f64; 3], k: f64) -> [f64; 3] {
    a.map(|x| x * k)
}

const Z: [f64; 3] = [0.0, 0.0, 1.0];

enum PlanePlane {
    Coincident,
    Other,
}

/// Two planes: a line, coincident over an overlap, or apart.
fn plane_plane(
    k: &Plane,
    se: &Extent2d,
    plane: &Plane,
    pe: &Extent2d,
    tol: f64,
    found: &mut Found,
) -> Option<PlanePlane> {
    let (fs, fp) = (k.frame(), plane.frame());
    let n = *fp.z();
    let p0 = fp.origin();
    let a = n.dot(*fs.x());
    let b = n.dot(*fs.y());
    let c = n.dot(fs.origin() - p0);
    let f = |u: f64, v: f64| a * u + b * v + c;
    let fc = corners_of(se).map(|(u, v)| f(u, v));
    if fc.iter().all(|x| x.abs() <= tol) {
        let world = |frame: &scx_num::frame::Frame, e: &Extent2d| {
            corners_of(e).map(|(u, v)| frame.origin() + *frame.x() * u + *frame.y() * v)
        };
        let (qs, qp) = (world(fs, se), world(fp, pe));
        let apart = [*fs.x(), *fs.y(), *fp.x(), *fp.y()].iter().any(|&w| {
            let span = |q: &[Point3; 4]| {
                q.iter()
                    .fold((f64::INFINITY, f64::NEG_INFINITY), |(lo, hi), p| {
                        let x = w.dot(p.to_vec());
                        (lo.min(x), hi.max(x))
                    })
            };
            let ((l0, h0), (l1, h1)) = (span(&qs), span(&qp));
            l0 > h1 + tol || l1 > h0 + tol
        });
        return Some(if apart {
            PlanePlane::Other
        } else {
            PlanePlane::Coincident
        });
    }
    if fc.iter().all(|&x| x > tol) || fc.iter().all(|&x| x < -tol) {
        return Some(PlanePlane::Other);
    }
    let m = se.mid();
    let fm = f(m.x, m.y);
    let g = a * a + b * b;
    let at = k.point(scx_num::vector2d::Point2::new(
        m.x - fm * a / g,
        m.y - fm * b / g,
    ));
    found.line(at, fs.z().cross(n), Transversal)?;
    Some(PlanePlane::Other)
}

fn corners_of(e: &Extent2d) -> [(f64, f64); 4] {
    let (u0, u1, v0, v1) = (e.x_min(), e.x_max(), e.y_min(), e.y_max());
    [(u0, v0), (u1, v0), (u0, v1), (u1, v1)]
}

/// A cylinder: an ellipse, a circle, two lines or a tangent line.
fn cylinder(
    k: &Cylinder,
    se: &Extent2d,
    n: Vec3,
    p0: Point3,
    tol: f64,
    found: &mut Found,
) -> Option<()> {
    let axes = Axes::new(k.frame(), k.sense());
    let r = k.radius().mm();
    let (nl, s, e_r, e_t) = lean(&axes, n);
    let nz = nl[2];
    let d = n.dot(p0 - axes.o);
    let (_, [v0, v1]) = step_bounds(|p| k.to_step(p), se);
    if nz != 0.0 && r * s <= tol * nz.abs() {
        // Square to the axis within the tolerance: a circle.
        let c = axes.point([0.0, 0.0, d / nz]);
        found.conic(c, axes.x * r, axes.y * r, true, Transversal);
        return Some(());
    }
    let (vm, hh) = (0.5 * (v0 + v1), 0.5 * (v1 - v0));
    if nz.abs() * hh <= tol {
        // Along the axis within the tolerance over the height: lines, if the true ellipse
        // stays within the tolerance of them.
        let delta = |v: f64| (d - nz * v) / s;
        let half = |dl: f64| ((r - dl.abs()) * (r + dl.abs())).max(0.0).sqrt();
        let dm = delta(vm);
        if dm.abs() - r > tol {
            return Some(());
        }
        let hm = half(dm);
        if (half(delta(v0)) - hm)
            .abs()
            .max((half(delta(v1)) - hm).abs())
            <= tol
        {
            if hm <= tol {
                let at = add(scale(e_r, r * dm.signum()), scale(Z, vm));
                found.line(axes.point(at), axes.z, Tangent)?;
            } else {
                for side in [1.0, -1.0] {
                    let at = add(add(scale(e_r, dm), scale(e_t, side * hm)), scale(Z, vm));
                    found.line(axes.point(at), axes.z, Transversal)?;
                }
            }
            return Some(());
        }
    }
    if nz == 0.0 || s == 0.0 {
        return None;
    }
    let semi = r / nz.abs();
    if semi * PRECISION > tol {
        return None;
    }
    let w = axes.vec(e_t);
    let a = n.cross(w) * semi;
    found.conic(axes.point([0.0, 0.0, d / nz]), a, w * r, false, Transversal);
    Some(())
}

/// A cone: an ellipse, a circle, generators, the apex, or `General` for a parabola or
/// hyperbola.
///
/// Worked from the apex, where the cone is `rho = k z'` and the plane is `n . p' = delta`:
/// every size is then `delta` times a factor of the plane's lean, with no cancellation (a
/// plane through the apex gives the apex, not a rounding-sized ellipse).
fn cone(k: &Cone, se: &Extent2d, n: Vec3, p0: Point3, tol: f64, found: &mut Found) -> Option<()> {
    let axes = Axes::new(k.frame(), k.sense());
    let r = k.radius().mm();
    let kk = fmath::tan(k.half_angle());
    if kk == 0.0 || !kk.is_finite() {
        return None;
    }
    let (nl, s, e_r, e_t) = lean(&axes, n);
    let nz = nl[2];
    let za = -r / kk;
    let apex = axes.point([0.0, 0.0, za]);
    let delta = n.dot(p0 - apex);
    let (_, [v0, v1]) = step_bounds(|p| k.to_step(p), se);
    let den = nz.abs() - s * kk.abs();
    if nz != 0.0 && den > 0.0 {
        let hc = delta / nz;
        let rc = kk * hc;
        if s * rc.abs() * (kk * kk + 1.0).sqrt() <= tol * den {
            // Square to the axis within the tolerance: a circle, the apex or nothing.
            if rc.abs() <= tol {
                found.touches.push(apex);
            } else if rc > 0.0 {
                let c = apex + axes.z * hc;
                found.conic(c, axes.x * rc, axes.y * rc, true, Transversal);
            }
            return Some(());
        }
    }
    let e = nz * nz - kk * kk * s * s;
    if e > 0.0 {
        // An ellipse about apex + delta n + alpha_c e1, e1 = -nz e_r + s z in the plane:
        // semi-axes |delta k| / e along e1 and |delta k| / sqrt(e) along e_t, on the nappe
        // where k delta nz > 0.
        let sa = (delta * kk).abs() / e;
        if sa <= tol {
            found.touches.push(apex);
            return Some(());
        }
        if kk * delta * nz < 0.0 {
            return Some(());
        }
        if sa * PRECISION > tol || s == 0.0 {
            return None;
        }
        let sb = (delta * kk).abs() / e.sqrt();
        let ac = delta * s * nz * (1.0 + kk * kk) / e;
        let e1 = add(scale(e_r, -nz), scale(Z, s));
        let c = apex + axes.vec(add(scale(nl, delta), scale(e1, ac)));
        found.conic(c, axes.vec(e1) * sa, axes.vec(e_t) * sb, false, Transversal);
        return Some(());
    }
    if delta.abs() > tol || s == 0.0 {
        return None;
    }
    // Through the apex, at least as steep as a generator: the generators in the plane.
    let cos = (-nz / (s * kk)).clamp(-1.0, 1.0);
    let sin = (1.0 - cos * cos).max(0.0).sqrt();
    let rho = (kk * v0 + r).abs().max((kk * v1 + r).abs());
    let dir = |side: f64| {
        axes.vec(add(
            scale(add(scale(e_r, cos), scale(e_t, side * sin)), kk),
            Z,
        ))
    };
    if sin * rho <= tol {
        found.line(apex, dir(0.0), Tangent)?;
    } else {
        found.line(apex, dir(1.0), Transversal)?;
        found.line(apex, dir(-1.0), Transversal)?;
    }
    Some(())
}

/// A sphere: a circle, a touch point or nothing.
fn sphere(k: &Sphere, plane: &Plane, p0: Point3, tol: f64, found: &mut Found) {
    let o = k.frame().origin();
    let big = k.radius().mm();
    let fp = plane.frame();
    let n = *fp.z();
    let d = n.dot(p0 - o);
    let touch = o + n * (d.signum() * big);
    if d.abs() > big {
        if d.abs() - big <= tol {
            found.touches.push(touch);
        }
        return;
    }
    let rho = ((big - d.abs()) * (big + d.abs())).sqrt();
    if rho <= tol {
        found.touches.push(touch);
        return;
    }
    found.conic(o + n * d, *fp.x() * rho, *fp.y() * rho, true, Transversal);
}

/// A torus: circles across the axis, meridians along it, else `General`.
fn torus(
    k: &Torus,
    clip: &Clip<'_>,
    n: Vec3,
    p0: Point3,
    tol: f64,
    found: &mut Found,
) -> Option<()> {
    let axes = Axes::new(k.frame(), k.sense());
    let (big, small) = (k.major().mm(), k.minor().mm());
    let (nl, s, _, e_t) = lean(&axes, n);
    let nz = nl[2];
    let d = n.dot(p0 - axes.o);
    if d.abs() - (big + small) > tol {
        // Past the bounding sphere.
        return Some(());
    }
    if nz != 0.0 && s * (big + small) <= tol * nz.abs() {
        let h = d / nz;
        let c = axes.point([0.0, 0.0, h]);
        let c2 = (small - h.abs()) * (small + h.abs());
        if c2 < 0.0 && h.abs() - small > tol {
            return Some(());
        }
        let half = c2.max(0.0).sqrt();
        if half <= tol {
            found.conic(c, axes.x * big, axes.y * big, true, Tangent);
        } else {
            for rho in [big + half, big - half] {
                found.conic(c, axes.x * rho, axes.y * rho, true, Transversal);
            }
        }
        return Some(());
    }
    if s > 0.0 && d.abs() + nz.abs() * small <= tol {
        // Through the axis: two meridians, the tube angle their parameter.
        let tube = clip.tube();
        let z = axes.z;
        for side in [1.0, -1.0] {
            let out = axes.vec(e_t) * side;
            let c = axes.o + out * big;
            let mut raw = Raw::new(
                Shape::Conic {
                    c,
                    a: out * small,
                    b: z * small,
                    circle: true,
                },
                Transversal,
            );
            if let Some(v) = tube {
                for psi in v {
                    let (sn, cs) = fmath::sin_cos(psi);
                    raw.sides.push(Side::through(z * cs - out * sn, c));
                }
            }
            found.raws.push(raw);
        }
        return Some(());
    }
    None
}
