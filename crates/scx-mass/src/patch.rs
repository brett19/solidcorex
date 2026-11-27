// Written for solidcorex. Apache-2.0.
//! A surface and a trim evaluated in [`Ser`]: one smooth patch of the surface (the analytic
//! kinds' arc, a NURBS surface's knot span, picked by a point `near` inside it) at series
//! `u` and `v`, and a curve's Taylor coefficients at a point or over a box.

use scx_curve::{Curve, Curve2};
use scx_num::interval::Ivl;
use scx_num::ring::Ring;
use scx_num::vector2d::Point2;
use scx_nurbs::Approach;
use scx_nurbs_surface::{NurbsSurface, ParamAxis};
use scx_surface::Surface;

use crate::tseries::Ser;

/// A point in series.
pub(crate) type P3 = [Ser; 3];

/// Why a patch cannot be evaluated: a kind this method has no series for, or a NURBS span
/// or point outside the surface's tables.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PatchFail {
    /// The surface kind has no series form.
    Kind,
    /// A NURBS table read failed.
    Table,
}

/// `S(u, v)` on the patch holding `near`.
pub(crate) fn eval(s: &Surface, u: &Ser, v: &Ser, near: Point2) -> Result<P3, PatchFail> {
    let (u, v) = (*u, *v);
    let p = match s {
        Surface::Plane(p) => p.eval(u, v, near),
        Surface::Cylinder(c) => c.eval(u, v, near),
        Surface::Cone(c) => c.eval(u, v, near),
        Surface::Sphere(c) => c.eval(u, v, near),
        Surface::Torus(c) => c.eval(u, v, near),
        Surface::Nurbs(n) => return nurbs(n, &u, &v, near),
        _ => return Err(PatchFail::Kind),
    };
    Ok([p.x, p.y, p.z])
}

/// The `p + 1` basis functions of degree `p` on span `i` at `x` (Cox-de Boor, the knot
/// differences taken as constants, so nothing divides by a series).
fn basis(knots: &[f64], p: usize, i: usize, x: &Ser) -> Result<Vec<Ser>, PatchFail> {
    let k = |j: usize| knots.get(j).copied().ok_or(PatchFail::Table);
    let zero = Ser::lift(0.0);
    let mut n = vec![zero; p + 1];
    let mut left = vec![zero; p + 1];
    let mut right = vec![zero; p + 1];
    if let Some(f) = n.first_mut() {
        *f = Ser::lift(1.0);
    }
    for j in 1..=p {
        let lo = k((i + 1).checked_sub(j).ok_or(PatchFail::Table)?)?;
        let hi = k(i + j)?;
        if let (Some(l), Some(r)) = (left.get_mut(j), right.get_mut(j)) {
            *l = *x - Ser::lift(lo);
            *r = Ser::lift(hi) - *x;
        }
        let mut saved = zero;
        for r in 0..j {
            // right[r + 1] + left[j - r] = U[i + r + 1] - U[i + 1 - j + r], free of x.
            let a = k(i + r + 1)?;
            let b = k((i + 1 + r).checked_sub(j).ok_or(PatchFail::Table)?)?;
            let width = Ivl::from(a) - Ivl::from(b);
            let inv = Ser::constant(Ivl::from(1.0) / width, crate::tseries::MAX);
            let nr = n.get(r).copied().ok_or(PatchFail::Table)?;
            let temp = nr * inv;
            let rr = right.get(r + 1).copied().ok_or(PatchFail::Table)?;
            let lr = left.get(j - r).copied().ok_or(PatchFail::Table)?;
            if let Some(slot) = n.get_mut(r) {
                *slot = saved + rr * temp;
            }
            saved = lr * temp;
        }
        if let Some(slot) = n.get_mut(j) {
            *slot = saved;
        }
    }
    Ok(n)
}

/// A NURBS surface's point on the knot span holding `near` (`Above` at a knot).
fn nurbs(n: &NurbsSurface<3>, u: &Ser, v: &Ser, near: Point2) -> Result<P3, PatchFail> {
    let (ku, kv) = (n.knot_vector(ParamAxis::U), n.knot_vector(ParamAxis::V));
    let (p, q) = (ku.degree(), kv.degree());
    let i = ku
        .span(near.x, Approach::Above)
        .map_err(|_| PatchFail::Table)?;
    let j = kv
        .span(near.y, Approach::Above)
        .map_err(|_| PatchFail::Table)?;
    let nu = basis(ku.knots(), p, i, u)?;
    let nv = basis(kv.knots(), q, j, v)?;
    let (i0, j0) = (
        i.checked_sub(p).ok_or(PatchFail::Table)?,
        j.checked_sub(q).ok_or(PatchFail::Table)?,
    );
    let zero = Ser::lift(0.0);
    let mut sum = [zero; 3];
    let mut w = zero;
    let points = n.weighted_points().unwrap_or_else(|| n.points());
    for (a, bu) in nu.iter().enumerate() {
        for (b, bv) in nv.iter().enumerate() {
            let basis = *bu * *bv;
            let pt = points.get(i0 + a, j0 + b).ok_or(PatchFail::Table)?;
            for (s, &c) in sum.iter_mut().zip(pt) {
                *s = *s + basis * Ser::lift(c);
            }
            if let Some(ws) = n.weights() {
                let wt = ws.get(i0 + a, j0 + b).ok_or(PatchFail::Table)?;
                w = w + basis * Ser::lift(*wt);
            }
        }
    }
    if n.is_rational() {
        let r = Ser::lift(1.0) / w;
        Ok(sum.map(|s| s * r))
    } else {
        Ok(sum)
    }
}

/// A 2-D curve's Taylor coefficients at `t` (a point, or a box for the remainder's), up to
/// degree `n <= 9`: `[u, v]` per degree.
pub(crate) fn coeffs2(c: &Curve2, t: Ivl, n: usize) -> Vec<[Ivl; 2]> {
    fn take<const D: usize>(c: &Curve2, t: Ivl, n: usize) -> Vec<[Ivl; 2]> {
        let j = c.jet_over::<Ivl, D>(t, Approach::Above);
        let mut out = vec![[j.point.x, j.point.y]];
        let mut fact = 1.0;
        for (k, d) in j.d.iter().enumerate().take(n) {
            fact *= (k + 1) as f64;
            let f = Ivl::from(fact);
            out.push([d.x / f, d.y / f]);
        }
        out
    }
    if n <= 4 {
        take::<4>(c, t, n)
    } else {
        take::<9>(c, t, n)
    }
}

/// A 3-D curve's Taylor coefficients at `t`, as [`coeffs2`].
pub(crate) fn coeffs3(c: &Curve, t: Ivl, n: usize) -> Vec<[Ivl; 3]> {
    fn take<const D: usize>(c: &Curve, t: Ivl, n: usize) -> Vec<[Ivl; 3]> {
        let j = c.jet_over::<Ivl, D>(t, Approach::Above);
        let mut out = vec![[j.point.x, j.point.y, j.point.z]];
        let mut fact = 1.0;
        for (k, d) in j.d.iter().enumerate().take(n) {
            fact *= (k + 1) as f64;
            let f = Ivl::from(fact);
            out.push([d.x / f, d.y / f, d.z / f]);
        }
        out
    }
    if n <= 4 {
        take::<4>(c, t, n)
    } else {
        take::<9>(c, t, n)
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use std::sync::Arc;

    use scx_num::frame::{Frame, Sense};
    use scx_num::units::Length;
    use scx_num::vector3d::{Point3, Vec3};
    use scx_nurbs::KnotVector;
    use scx_nurbs_surface::Net;
    use scx_surface::{Cone, Cylinder, Plane, Sphere, Torus};

    use super::*;

    fn holds(x: Ivl, v: f64, slack: f64) -> bool {
        x.lo - slack <= v && v <= x.hi + slack
    }

    fn surfaces() -> Vec<Surface> {
        let f = Frame::new(
            Point3::new(0.5, -1.0, 2.0),
            Vec3::new(0.0, 0.6, 0.8),
            Vec3::new(1.0, 0.0, 0.0),
        )
        .unwrap();
        let mm = Length::from_mm;
        let ku = KnotVector::new(2, vec![0.0, 0.0, 0.0, 0.4, 1.0, 1.0, 1.0]).unwrap();
        let kv = KnotVector::new(1, vec![0.0, 0.0, 0.5, 1.0, 1.0]).unwrap();
        let mut pts = Vec::new();
        let mut ws = Vec::new();
        for i in 0..4 {
            for j in 0..3 {
                let (x, y) = (f64::from(i), f64::from(j));
                pts.push([x, y, 0.3 * x * y - 0.1 * x * x]);
                ws.push(1.0 + 0.25 * f64::from((i + j) % 2));
            }
        }
        let net = Net::new(4, 3, pts).unwrap();
        let wnet = Net::new(4, 3, ws).unwrap();
        vec![
            Surface::Plane(Plane::new(f)),
            Surface::Cylinder(Cylinder::new(f, mm(1.5), Sense::Forward).unwrap()),
            Surface::Cylinder(Cylinder::new(f, mm(1.5), Sense::Reversed).unwrap()),
            Surface::Cone(Cone::new(f, mm(1.0), 0.3, Sense::Forward).unwrap()),
            Surface::Sphere(Sphere::new(f, mm(2.0), Sense::Forward).unwrap()),
            Surface::Torus(Torus::new(f, mm(3.0), mm(1.0), Sense::Forward).unwrap()),
            Surface::Nurbs(Arc::new(
                NurbsSurface::new(ku.clone(), kv.clone(), net.clone(), None).unwrap(),
            )),
            Surface::Nurbs(Arc::new(
                NurbsSurface::new(ku, kv, net, Some(wnet)).unwrap(),
            )),
        ]
    }

    /// Each kind's series point and first partials, at a point, hold the surface's own jet.
    #[test]
    fn a_patch_series_holds_the_surface_jet() {
        for s in surfaces() {
            for uv in [Point2::new(0.3, 0.2), Point2::new(0.7, 0.8)] {
                let u = Ser::var_t(Ivl::from(uv.x), 3);
                let v = Ser::var_s(Ivl::from(uv.y), 3);
                let p = eval(&s, &u, &v, uv).unwrap();
                let j = s.jet::<1>(uv, [Approach::Above; 2]);
                let (q, su, sv) = (j.point(), j.get(1, 0).unwrap(), j.get(0, 1).unwrap());
                let want = [[q.x, su.x, sv.x], [q.y, su.y, sv.y], [q.z, su.z, sv.z]];
                for (c, w) in p.iter().zip(want) {
                    let slack = 1e-12 * (1.0 + w[0].abs() + w[1].abs() + w[2].abs());
                    assert!(holds(c.at(0, 0), w[0], slack), "{s:?} {uv:?} point");
                    assert!(holds(c.at(1, 0), w[1], slack), "{s:?} {uv:?} su");
                    assert!(holds(c.at(0, 1), w[2], slack), "{s:?} {uv:?} sv");
                }
            }
        }
    }
}
