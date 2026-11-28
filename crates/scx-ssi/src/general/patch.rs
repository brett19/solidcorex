// Written for solidcorex. Apache-2.0.
//! A piece's own formula and certified enclosures of its point and partials to order 2 over
//! any uv box (research 73 §4.4, row SM5).
//!
//! A piece lies on one Bézier cell of its surface: one arc of an analytic kind (the arc that
//! holds its lower corner, as `Surface::box_over` picks it) or one knot span of a NURBS
//! surface. Its **home** is that cell's formula, continued past the cell where a box pokes
//! out of it (an arc's rational quadratic runs on round its circle; a span's polynomial
//! continues as `NurbsSurface::partials_over_piece` continues it). A proof over a box that
//! leaves the piece is then a proof about one smooth function, and a root on the piece's
//! edge is inside the box it is proved in.
//!
//! The analytic kinds are evaluated in [`Jet2`], a ring of values with their partials to
//! order 2 over intervals (forward-mode differentiation, rounded outward), through each kind's
//! own `eval` in any [`Ring`]; NURBS surfaces through `partials_over_piece::<2>`. A system
//! that needs only the first partials (an edge's or a ray's seed solve) takes [`Jet1`], the
//! same through [`Grad`] or `partials_over_piece::<1>`, at about half the work.

use core::ops::{Add, Div, Mul, Sub};

use scx_num::extent2d::Extent2d;
use scx_num::interval::Ivl;
use scx_num::ring::Ring;
use scx_num::vector2d::Point2;
use scx_num::vector3d::{Point3, Vec3};
use scx_nurbs::Approach;
use scx_nurbs_surface::{About, NurbsSurface, ParamAxis};
use scx_surface::Surface;

/// A surface's point and partials to order 2 over a uv box, each an enclosure.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Jet {
    pub p: Vec3<Ivl>,
    pub su: Vec3<Ivl>,
    pub sv: Vec3<Ivl>,
    pub suu: Vec3<Ivl>,
    pub suv: Vec3<Ivl>,
    pub svv: Vec3<Ivl>,
}

impl Jet {
    /// `S_u x S_v` over the box.
    pub fn normal(&self) -> Vec3<Ivl> {
        self.su.cross(self.sv)
    }

    /// The longest side of the box holding the point.
    pub fn size(&self) -> f64 {
        let w = |x: Ivl| x.hi - x.lo;
        w(self.p.x).max(w(self.p.y)).max(w(self.p.z))
    }

    /// The middle of the box holding the point.
    pub fn mid(&self) -> Point3 {
        Point3::new(self.p.x.mid(), self.p.y.mid(), self.p.z.mid())
    }

    fn is_finite(&self) -> bool {
        [self.p, self.su, self.sv, self.suu, self.suv, self.svv]
            .iter()
            .all(|v| v.x.is_finite() && v.y.is_finite() && v.z.is_finite())
    }
}

/// A surface's point and first partials over a uv box, each an enclosure: what a system
/// that needs no curvature (an edge or a ray against a piece) evaluates, at about a third
/// of a [`Jet`]'s cost.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Jet1 {
    pub p: Vec3<Ivl>,
    pub su: Vec3<Ivl>,
    pub sv: Vec3<Ivl>,
}

impl Jet1 {
    /// The longest side of the box holding the point.
    pub fn size(&self) -> f64 {
        let w = |x: Ivl| x.hi - x.lo;
        w(self.p.x).max(w(self.p.y)).max(w(self.p.z))
    }

    /// The middle of the box holding the point.
    pub fn mid(&self) -> Point3 {
        Point3::new(self.p.x.mid(), self.p.y.mid(), self.p.z.mid())
    }

    fn is_finite(&self) -> bool {
        [self.p, self.su, self.sv]
            .iter()
            .all(|v| v.x.is_finite() && v.y.is_finite() && v.z.is_finite())
    }
}

/// Which formula a piece is evaluated with.
#[derive(Clone, Copy, Debug)]
enum Home {
    /// An analytic kind: the arc that holds this point (the piece's lower corner).
    Arc(Point2),
    /// A NURBS surface: the knot span that holds the piece.
    Span([usize; 2]),
}

/// A piece of a surface with its home formula (module docs).
#[derive(Clone, Copy, Debug)]
pub(crate) struct Patch<'s> {
    surface: &'s Surface,
    home: Home,
}

impl<'s> Patch<'s> {
    /// The patch of `surface` over the piece `uv`; `None` when a NURBS span cannot be found
    /// (a parameter not finite).
    pub fn new(surface: &'s Surface, uv: &Extent2d) -> Option<Self> {
        let home = match surface {
            Surface::Nurbs(n) => {
                let m = uv.mid();
                let su = n
                    .knot_vector(ParamAxis::U)
                    .span(m.x, Approach::Above)
                    .ok()?;
                let sv = n
                    .knot_vector(ParamAxis::V)
                    .span(m.y, Approach::Above)
                    .ok()?;
                Home::Span([su, sv])
            }
            _ => Home::Arc(uv.mid()),
        };
        Some(Self { surface, home })
    }

    /// The jet over `u x v`, `None` where it cannot be enclosed (a value not finite, or a
    /// NURBS box whose weights the form cannot prove positive).
    pub fn jet(&self, u: Ivl, v: Ivl) -> Option<Jet> {
        let j = match (self.surface, self.home) {
            (Surface::Nurbs(n), Home::Span(span)) => nurbs_jet(n, span, u, v)?,
            (Surface::Nurbs(_), Home::Arc(_)) | (_, Home::Span(_)) => return None,
            (s, Home::Arc(near)) => analytic_jet(s, u, v, near)?,
        };
        j.is_finite().then_some(j)
    }

    /// The point and first partials over `u x v`, as [`Patch::jet`] encloses them.
    pub fn jet1(&self, u: Ivl, v: Ivl) -> Option<Jet1> {
        let j = match (self.surface, self.home) {
            (Surface::Nurbs(n), Home::Span(span)) => nurbs_jet1(n, span, u, v)?,
            (Surface::Nurbs(_), Home::Arc(_)) | (_, Home::Span(_)) => return None,
            (s, Home::Arc(near)) => analytic_jet1(s, u, v, near)?,
        };
        j.is_finite().then_some(j)
    }

    /// The jet at a point (each part a narrow enclosure).
    #[cfg(test)]
    pub fn jet_at(&self, u: f64, v: f64) -> Option<Jet> {
        self.jet(Ivl::from(u), Ivl::from(v))
    }
}

fn nurbs_jet(n: &NurbsSurface<3>, span: [usize; 2], u: Ivl, v: Ivl) -> Option<Jet> {
    let j = n
        .partials_over_piece::<2>(span, [u, v], About::Middle)
        .ok()?;
    let g = |i, k| j.get(i, k).map(|[x, y, z]| Vec3::new(x, y, z));
    Some(Jet {
        p: g(0, 0)?,
        su: g(1, 0)?,
        sv: g(0, 1)?,
        suu: g(2, 0)?,
        suv: g(1, 1)?,
        svv: g(0, 2)?,
    })
}

fn nurbs_jet1(n: &NurbsSurface<3>, span: [usize; 2], u: Ivl, v: Ivl) -> Option<Jet1> {
    let j = n
        .partials_over_piece::<1>(span, [u, v], About::Middle)
        .ok()?;
    let g = |i, k| j.get(i, k).map(|[x, y, z]| Vec3::new(x, y, z));
    Some(Jet1 {
        p: g(0, 0)?,
        su: g(1, 0)?,
        sv: g(0, 1)?,
    })
}

fn analytic_jet1(s: &Surface, u: Ivl, v: Ivl, near: Point2) -> Option<Jet1> {
    let (ju, jv) = (Grad::var(u, 0), Grad::var(v, 1));
    let p = match s {
        Surface::Plane(k) => k.eval(ju, jv, near),
        Surface::Cylinder(k) => k.eval(ju, jv, near),
        Surface::Cone(k) => k.eval(ju, jv, near),
        Surface::Sphere(k) => k.eval(ju, jv, near),
        Surface::Torus(k) => k.eval(ju, jv, near),
        _ => return None,
    };
    let pick = |f: fn(Grad) -> Ivl| Vec3::new(f(p.x), f(p.y), f(p.z));
    Some(Jet1 {
        p: pick(|j| j.f),
        su: pick(|j| j.u),
        sv: pick(|j| j.v),
    })
}

fn analytic_jet(s: &Surface, u: Ivl, v: Ivl, near: Point2) -> Option<Jet> {
    let (ju, jv) = (Jet2::var(u, 0), Jet2::var(v, 1));
    let p = match s {
        Surface::Plane(k) => k.eval(ju, jv, near),
        Surface::Cylinder(k) => k.eval(ju, jv, near),
        Surface::Cone(k) => k.eval(ju, jv, near),
        Surface::Sphere(k) => k.eval(ju, jv, near),
        Surface::Torus(k) => k.eval(ju, jv, near),
        _ => return None,
    };
    let pick = |f: fn(Jet2) -> Ivl| Vec3::new(f(p.x), f(p.y), f(p.z));
    Some(Jet {
        p: pick(|j| j.f),
        su: pick(|j| j.u),
        sv: pick(|j| j.v),
        suu: pick(|j| j.uu),
        suv: pick(|j| j.uv),
        svv: pick(|j| j.vv),
    })
}

/// A value with its partials in `u` and `v` to order 2, each an interval enclosure over the
/// same box: the ring the analytic kinds' formulas are evaluated in. A quotient by a value
/// that may be 0 is the whole line (as [`Ivl`]'s), so the jet is then not finite.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Jet2 {
    f: Ivl,
    u: Ivl,
    v: Ivl,
    uu: Ivl,
    uv: Ivl,
    vv: Ivl,
}

const Z: Ivl = Ivl::new(0.0, 0.0);

impl Jet2 {
    /// The variable `x` (`which` 0 for `u`, 1 for `v`) over its box.
    fn var(x: Ivl, which: usize) -> Self {
        let (du, dv) = if which == 0 {
            (Ivl::from(1.0), Z)
        } else {
            (Z, Ivl::from(1.0))
        };
        Self {
            f: x,
            u: du,
            v: dv,
            uu: Z,
            uv: Z,
            vv: Z,
        }
    }

    /// `1 / self`, by the chain rule on `1/x`: `r' = -x' r^2`, `r'' = 2 x'x' r^3 - x'' r^2`.
    fn recip(self) -> Self {
        let r = Ivl::from(1.0) / self.f;
        let r2 = r * r;
        let r3 = r2 * r;
        let two = Ivl::from(2.0);
        Self {
            f: r,
            u: Z - self.u * r2,
            v: Z - self.v * r2,
            uu: two * self.u * self.u * r3 - self.uu * r2,
            uv: two * self.u * self.v * r3 - self.uv * r2,
            vv: two * self.v * self.v * r3 - self.vv * r2,
        }
    }
}

impl Add for Jet2 {
    type Output = Self;
    fn add(self, b: Self) -> Self {
        Self {
            f: self.f + b.f,
            u: self.u + b.u,
            v: self.v + b.v,
            uu: self.uu + b.uu,
            uv: self.uv + b.uv,
            vv: self.vv + b.vv,
        }
    }
}

impl Sub for Jet2 {
    type Output = Self;
    fn sub(self, b: Self) -> Self {
        Self {
            f: self.f - b.f,
            u: self.u - b.u,
            v: self.v - b.v,
            uu: self.uu - b.uu,
            uv: self.uv - b.uv,
            vv: self.vv - b.vv,
        }
    }
}

impl Mul for Jet2 {
    type Output = Self;
    fn mul(self, b: Self) -> Self {
        let (a, c) = (self, b);
        // Twice a product by a sum, not a second product: one rounding, not four.
        let (uu, vv) = (a.u * c.u, a.v * c.v);
        Self {
            f: a.f * c.f,
            u: a.u * c.f + a.f * c.u,
            v: a.v * c.f + a.f * c.v,
            uu: a.uu * c.f + (uu + uu) + a.f * c.uu,
            uv: a.uv * c.f + a.u * c.v + a.v * c.u + a.f * c.uv,
            vv: a.vv * c.f + (vv + vv) + a.f * c.vv,
        }
    }
}

impl Div for Jet2 {
    type Output = Self;
    #[allow(
        clippy::suspicious_arithmetic_impl,
        reason = "a quotient of jets is the product with the reciprocal's jet"
    )]
    fn div(self, b: Self) -> Self {
        self * b.recip()
    }
}

impl Ring for Jet2 {
    fn lift(d: f64) -> Self {
        Self {
            f: Ivl::from(d),
            u: Z,
            v: Z,
            uu: Z,
            uv: Z,
            vv: Z,
        }
    }
}

/// A value with its first partials in `u` and `v`, each an interval enclosure over the same
/// box: [`Jet2`] without the second order.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Grad {
    f: Ivl,
    u: Ivl,
    v: Ivl,
}

impl Grad {
    /// The variable `x` (`which` 0 for `u`, 1 for `v`) over its box.
    fn var(x: Ivl, which: usize) -> Self {
        let (u, v) = if which == 0 {
            (Ivl::from(1.0), Z)
        } else {
            (Z, Ivl::from(1.0))
        };
        Self { f: x, u, v }
    }
}

impl Add for Grad {
    type Output = Self;
    fn add(self, b: Self) -> Self {
        Self {
            f: self.f + b.f,
            u: self.u + b.u,
            v: self.v + b.v,
        }
    }
}

impl Sub for Grad {
    type Output = Self;
    fn sub(self, b: Self) -> Self {
        Self {
            f: self.f - b.f,
            u: self.u - b.u,
            v: self.v - b.v,
        }
    }
}

impl Mul for Grad {
    type Output = Self;
    fn mul(self, b: Self) -> Self {
        Self {
            f: self.f * b.f,
            u: self.u * b.f + self.f * b.u,
            v: self.v * b.f + self.f * b.v,
        }
    }
}

impl Div for Grad {
    type Output = Self;
    fn div(self, b: Self) -> Self {
        // (a / b)' = (a' - q b') / b, with q = a / b.
        let q = self.f / b.f;
        Self {
            f: q,
            u: (self.u - q * b.u) / b.f,
            v: (self.v - q * b.v) / b.f,
        }
    }
}

impl Ring for Grad {
    fn lift(d: f64) -> Self {
        Self {
            f: Ivl::from(d),
            u: Z,
            v: Z,
        }
    }
}

/// `a . b` over intervals.
pub(crate) fn dot(a: Vec3<Ivl>, b: Vec3<Ivl>) -> Ivl {
    a.dot(b)
}

/// The vector of intervals holding `v` exactly.
pub(crate) fn lift(v: Vec3) -> Vec3<Ivl> {
    Vec3::new(Ivl::from(v.x), Ivl::from(v.y), Ivl::from(v.z))
}

/// The middle of each component.
pub(crate) fn mid(v: Vec3<Ivl>) -> Vec3 {
    Vec3::new(v.x.mid(), v.y.mid(), v.z.mid())
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;
    use scx_num::frame::{Frame, Sense};
    use scx_num::units::Length;
    use scx_surface::{Sphere, Torus};

    fn frame() -> Frame {
        Frame::new(
            Point3::new(0.5, -0.25, 1.0),
            Vec3::new(0.0, 0.6, 0.8),
            Vec3::new(1.0, 0.0, 0.0),
        )
        .unwrap()
    }

    /// The jet over a box holds the surface's own jet at points in it, and the point jet
    /// is narrow; the first-order jet too; for a sphere, a torus and their NURBS forms.
    #[test]
    fn jets_hold_the_surface_partials() {
        let r = Length::from_mm(2.0);
        let sphere = Surface::Sphere(Sphere::new(frame(), r, Sense::Forward).unwrap());
        let torus = Surface::Torus(
            Torus::new(
                frame(),
                Length::from_mm(3.0),
                Length::from_mm(1.0),
                Sense::Forward,
            )
            .unwrap(),
        );
        let ex = Extent2d::from_bounds(0.3, 0.2, 0.9, 0.7).unwrap();
        let nurbs = |s: &Surface| Surface::Nurbs(std::sync::Arc::new(s.to_nurbs(&ex).unwrap()));
        for s in [sphere.clone(), nurbs(&sphere), torus.clone(), nurbs(&torus)] {
            let piece = Extent2d::from_bounds(0.4, 0.3, 0.5, 0.4).unwrap();
            let patch = Patch::new(&s, &piece).unwrap();
            let over = patch.jet(Ivl::new(0.4, 0.5), Ivl::new(0.3, 0.4)).unwrap();
            let first = patch.jet1(Ivl::new(0.4, 0.5), Ivl::new(0.3, 0.4)).unwrap();
            for (u, v) in [(0.4, 0.3), (0.45, 0.37), (0.5, 0.4)] {
                // The piece's own arcs: from below at its upper ends (the sphere's and torus's
                // quarter arcs join at 0.5, where the second partials jump).
                let side = |x: f64, hi: f64| {
                    if x == hi {
                        Approach::Below
                    } else {
                        Approach::Above
                    }
                };
                let truth = s.jet::<2>(Point2::new(u, v), [side(u, 0.5), side(v, 0.4)]);
                let at = patch.jet_at(u, v).unwrap();
                let holds = |b: Vec3<Ivl>, t: Vec3, slack: f64| {
                    let h = |i: Ivl, x: f64| i.lo - slack <= x && x <= i.hi + slack;
                    h(b.x, t.x) && h(b.y, t.y) && h(b.z, t.z)
                };
                let get = |i, j| truth.get(i, j).unwrap();
                let pairs = [
                    (over.p, at.p, get(0, 0)),
                    (over.su, at.su, get(1, 0)),
                    (over.sv, at.sv, get(0, 1)),
                    (over.suu, at.suu, get(2, 0)),
                    (over.suv, at.suv, get(1, 1)),
                    (over.svv, at.svv, get(0, 2)),
                    (first.p, at.p, get(0, 0)),
                    (first.su, at.su, get(1, 0)),
                    (first.sv, at.sv, get(0, 1)),
                ];
                for (k, (b, a, t)) in pairs.into_iter().enumerate() {
                    assert!(holds(b, t, 1e-9), "{k} {s:?} {b:?} {t:?}");
                    assert!(holds(a, t, 1e-9), "{k} {a:?} {t:?}");
                }
            }
        }
    }
}
