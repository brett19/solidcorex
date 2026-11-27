// Written for solidcorex. Apache-2.0.
//! The cylinder and cone pairs off one axis that still have a closed form (research 73
//! §4.2; the table in [`super`]): two cylinders with parallel axes, and the Miller-Goldman
//! pairs, two cylinders or cones whose axes meet and that share an inscribed sphere.
//!
//! **Parallel cylinders.** Moved onto `a`'s direction, `b`'s axis moves no point of its
//! extent by more than its tilt times its reach, `delta`; within the tolerance the pair is
//! two circles in `a`'s cross-section, solved as two meridians are ([`circles`]), and each
//! meeting point is a line along the axes. Their lens no deeper than the tolerance is one
//! tangent line, as everywhere here.
//!
//! **Miller-Goldman.** With the sphere `S(x) = |x - O|^2 - rho^2` centred where the axes
//! meet, a cylinder or cone tangent to it along a circle is `S = L^2`, `L` linear: a
//! cylinder's `L = (x - O) . z`, a cone's `L = ((x - O) . V - rho^2) / sqrt(|V|^2 - rho^2)`,
//! `V` its apex from `O` (`L = 0` is the plane of the circle of contact). Two of them meet
//! where `L_a^2 = L_b^2`: on the two planes `L_a = +- L_b`, so the answer is `a`'s sections
//! by those planes, conics, each on `b` too. They cross where both circles of contact meet
//! (`L_a = L_b = 0` on the sphere), the surfaces tangent there: those branch points are
//! points of the answer, and the curves are cut at them. Two cylinders of one radius whose
//! axes meet are the plainest case: the planes bisect the axes, the conics are ellipses.
//!
//! The pair is a Miller-Goldman one when moving `b`'s axis onto `a`'s point nearest it and
//! changing its size to the sphere's moves no point by more than the tolerance (the axes'
//! distance plus the spheres' radii apart); past it the pair is [`Closed::General`], so the
//! general path sees the near case. So is a pair whose sections hold a tangent piece or a
//! touch (a plane grazing `a`): there the closed form's depth rule would decide what the
//! general path must.

use scx_num::extent2d::Extent2d;
use scx_num::fmath;
use scx_num::vector2d::Point2;
use scx_num::vector3d::{Point3, Vec3};
use scx_surface::{Closed, Surface};

use super::Contact::Tangent;
use super::Intersection;
use super::clip::{Clip, Raw, Shape};
use super::coaxial::{Form, Meet, Revolution, circles};
use super::section::{self, Found};

/// Two curved surfaces off one axis (module docs), `ra` and `rb` their revolutions and
/// `clip` their extents: `General` for every other pair.
pub(super) fn pair(
    (a, ae): (&Surface, &Extent2d),
    b: &Surface,
    [ra, rb]: [&Revolution; 2],
    clip: &Clip<'_>,
    tol: f64,
) -> Closed<Intersection> {
    if let Some(answer) = parallel(ra, rb, clip, tol) {
        return answer;
    }
    match (a, b) {
        (Surface::Cylinder(_) | Surface::Cone(_), Surface::Cylinder(_) | Surface::Cone(_)) => {
            miller_goldman((a, ae), [ra, rb], clip, tol)
        }
        _ => Closed::General,
    }
}

/// Two cylinders whose axes are parallel within the tolerance: their lines. `None` when the
/// pair is no such pair.
fn parallel(
    ra: &Revolution,
    rb: &Revolution,
    clip: &Clip<'_>,
    tol: f64,
) -> Option<Closed<Intersection>> {
    let (Form::Cylinder { r: r1 }, Form::Cylinder { r: r2 }) = (ra.form, rb.form) else {
        return None;
    };
    let (za, zb) = (ra.axes.z, rb.axes.z);
    let dir = zb.dot(za).signum();
    let delta = (zb - za * dir).length() * rb.reach;
    if delta > tol {
        return None;
    }
    let [x, y, _] = ra.axes.local(rb.axes.o);
    let meet = circles(Point2::ORIGIN, r1, Point2::new(x, y), r2, tol, tol + delta);
    let Some(Meet::Points(points)) = meet else {
        // Between the tolerance and its slack, or one cylinder within the tolerance but off
        // its axis by more once tilted (`coaxial` answers the coincident pair).
        return Some(Closed::General);
    };
    let mut found = Found::default();
    for (q, contact) in points {
        let o = ra.axes.point([q.x, q.y, 0.0]);
        found.raws.push(Raw::new(Shape::Line { o, d: za }, contact));
    }
    Some(section::clipped(clip, found, tol))
}

/// The linear `L(x) = g . (x - O) + c` of a cylinder or cone about a sphere centred at `O`
/// on its axis (module docs).
#[derive(Clone, Copy, Debug)]
struct Polar {
    g: Vec3,
    c: f64,
}

/// The radius of the sphere about `o`, a point of `r`'s axis, tangent to it along a circle,
/// and its [`Polar`] there; `None` for a sphere or torus, or a cone's point on its apex or
/// past it.
fn inscribed(rev: &Revolution, o: Point3) -> Option<(f64, Polar)> {
    let z = rev.axes.z;
    match rev.form {
        Form::Cylinder { r } => Some((r, Polar { g: z, c: 0.0 })),
        Form::Cone { r, k } => {
            let h = (o - rev.axes.o).dot(z);
            let rho_axis = r + k * h;
            if rho_axis <= 0.0 {
                return None;
            }
            let rho = rho_axis / fmath::hypot(k, 1.0);
            // The apex from `o`: at `h = -r / k` on the axis.
            let v = z * (-r / k - h);
            let q = ((v.length() - rho) * (v.length() + rho)).sqrt();
            (q > 0.0).then(|| {
                (
                    rho,
                    Polar {
                        g: v / q,
                        c: -rho * rho / q,
                    },
                )
            })
        }
        Form::Sphere { .. } | Form::Torus { .. } => None,
    }
}

/// Two cylinders or cones sharing an inscribed sphere within the tolerance: `a`'s sections
/// by the planes `L_a = +- L_b`, cut at the branch points.
fn miller_goldman(
    (a, ae): (&Surface, &Extent2d),
    [ra, rb]: [&Revolution; 2],
    clip: &Clip<'_>,
    tol: f64,
) -> Closed<Intersection> {
    let (za, zb) = (ra.axes.z, rb.axes.z);
    let w = za.cross(zb);
    let ww = w.dot(w);
    if ww == 0.0 {
        return Closed::General;
    }
    // The axes' nearest points.
    let between = rb.axes.o - ra.axes.o;
    let o = ra.axes.o + za * (between.dot(zb.cross(w)) / ww);
    let ob = rb.axes.o + zb * (between.dot(za.cross(w)) / ww);
    let (Some((rho, la)), Some((rho_b, _))) = (inscribed(ra, o), inscribed(rb, ob)) else {
        return Closed::General;
    };
    if o.distance(ob) + (rho - rho_b).abs() > tol {
        return Closed::General;
    }
    // `b` moved onto `o` with the sphere's radius: its polar there.
    let lb = match rb.form {
        Form::Cylinder { .. } => Polar { g: zb, c: 0.0 },
        Form::Cone { r, k } => {
            let v = zb * (-r / k) + (rb.axes.o - o);
            let v = zb * v.dot(zb);
            let q = ((v.length() - rho) * (v.length() + rho)).sqrt();
            if q.is_nan() || q <= 0.0 {
                return Closed::General;
            }
            Polar {
                g: v / q,
                c: -rho * rho / q,
            }
        }
        Form::Sphere { .. } | Form::Torus { .. } => return Closed::General,
    };
    // The farthest a point of `a`'s extent lies from `o`.
    let reach = o.distance(ra.axes.o) + ra.reach;
    let mut found = Found::default();
    for sign in [1.0, -1.0] {
        let m = la.g - lb.g * sign;
        let h = la.c - lb.c * sign;
        let ml = m.length();
        if ml * reach <= tol {
            // `L_a -+ L_b` is all but constant over the extent: apart, or no plane.
            if h.abs() > tol + ml * reach {
                continue;
            }
            return Closed::General;
        }
        let n = m / ml;
        let p0 = o - n * (h / ml);
        let solved = match a {
            Surface::Cylinder(k) => section::cylinder(k, ae, n, p0, tol, &mut found),
            Surface::Cone(k) => section::cone(k, ae, n, p0, tol, &mut found),
            _ => None,
        };
        if solved.is_none() {
            return Closed::General;
        }
    }
    if !found.touches.is_empty() || found.raws.iter().any(|r| r.contact == Tangent) {
        return Closed::General;
    }
    // The branch points: where the circles of contact meet, on the line of their planes.
    let cw = la.g.cross(lb.g);
    let cc = cw.dot(cw);
    if cc == 0.0 {
        return Closed::General;
    }
    let y0 = (lb.g.cross(cw) * -la.c + cw.cross(la.g) * -lb.c) / cc;
    let near = y0.length();
    let half = ((rho - near) * (rho + near)).max(0.0).sqrt();
    if near - rho > tol {
        // The circles of contact miss: two conics apart.
    } else if near > rho || 2.0 * half <= tol {
        // They touch within the tolerance: the surfaces touch along a generator there
        // (one plane runs along `a`'s), which the sections' depth rule does not measure.
        return Closed::General;
    } else {
        let along = cw / cc.sqrt();
        for side in [1.0, -1.0] {
            found.branches.push(o + y0 + along * (side * half));
        }
    }
    section::clipped(clip, found, tol)
}
