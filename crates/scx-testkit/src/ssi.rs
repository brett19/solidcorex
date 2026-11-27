// Written for solidcorex. Apache-2.0.
//! Where two analytic surfaces meet, in closed form (research 63 T5): plane-plane, a plane
//! against a cylinder, cone or sphere, sphere-sphere and coaxial pairs (a plane across a
//! torus's axis among them), with the tangent and degenerate cases. Each answer is the
//! exact curve kind with its parameters and certified sample points. WR-680's oblique
//! cylinder section also has its exact rational form on the pin's NURBS cylinder, with the
//! uv curve in the pin's uv ([`cylinder_section`]). Design note
//! docs/research/64-intersection-truths.md §4.
//!
//! **The model.** As in [`crate::curve_surface`], a surface is the image of its canonical
//! form under its frame as given: `P = C + M L`, `M = [X Y Z]`, so it is exactly the zero
//! set of a polynomial in the frame coordinates `L = M^-1 (P - C)`. A plane is linear in
//! any frame's `L`, so a plane section is a conic in the plane, decided exactly in
//! rationals in the plane's own coordinates and mapped back by `M`, which keeps the kind
//! (an ellipse is an ellipse); every parameter is exact up to one square root, taken last
//! in double-double. A kind is the kind in the frame coordinates: a circle there is a
//! world circle when the frame is orthonormal.
//!
//! Two curved surfaces are a closed-form pair here when they share their axes (`X`, `Y`
//! and `Z` equal bit for bit) so they share `L` up to a shift: spheres anywhere (their
//! radical plane against one of them), and coaxial pairs, the shift along `Z` only, whose
//! meridians meet at circle heights from a polynomial in the axial coordinate. A plane
//! meets a torus here only across its axis. Any other pair is [`Error::NoClosedForm`].
//!
//! **The cone's slope.** `k = tan a` of a double semi-angle `a > 0` is transcendental, so no
//! rational configuration is exactly tangent to the true cone unless it is for every
//! slope. The truth holds `k` as the rational value of its double-double, within `2^-96`
//! relative of `tan a` (the double-double rounding with room), and decides a plane
//! section's predicates as polynomials in `k`: zero when the zero polynomial, else the sign
//! at `k` when no root lies within that band, else [`Error::Undecided`]. A coaxial pair
//! with a cone is solved at `k` and proved for every slope in the band: its section
//! polynomial's degree, discriminant and value at each apex keep their signs over the band
//! (a bound on the polynomials in the slopes' errors; for a cone against a torus, the
//! quartic's resultant with its derivative and its apex value as polynomials in `k`, by
//! the band's root test), else it is [`Error::Undecided`] (research 64 §4.4).

use crate::analytic::{Cylinder, Frame, Plane, V3};
use crate::bound::{dd_to_rat, rat_to_dd};
use crate::curve_surface::{self as cs, R3, SurfaceKind, rcross, rdot};
use crate::dd::{self, Dd};
use crate::poly::Poly;
use crate::quadric::{Exactly, Layout, Param, Shape};
use crate::rat::Rat;

/// Why a section has no answer here.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error {
    /// A surface's data are bad.
    Surface(cs::Error),
    /// A predicate on the cone's slope has a root within the slope's rounding band: the
    /// doubles do not decide the kind.
    Undecided,
    /// The pair is not one of the closed forms (module docs).
    NoClosedForm,
}

impl From<cs::Error> for Error {
    fn from(e: cs::Error) -> Error {
        Error::Surface(e)
    }
}

impl core::fmt::Display for Error {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Error::Surface(e) => write!(f, "a surface's data are bad: {e}"),
            Error::Undecided => f.write_str("the cone's slope rounding does not decide the kind"),
            Error::NoClosedForm => f.write_str("the pair has no closed form here"),
        }
    }
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Error::Surface(e) => Some(e),
            Error::Undecided | Error::NoClosedForm => None,
        }
    }
}

/// How the surfaces meet along a piece.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Contact {
    /// They cross.
    Transversal,
    /// They touch: a tangent plane's line or point, a double circle, or an isolated point
    /// such as a plane meeting a cone only at its apex.
    Tangent,
}

/// A section curve, in world coordinates. `a` and `b` are conjugate semi-diameters: for a
/// circle in an orthonormal frame, perpendicular and of the radius.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Curve {
    /// One point.
    Point(V3),
    /// `origin + t direction`.
    Line {
        /// A point.
        origin: V3,
        /// The direction (not normalised).
        direction: V3,
    },
    /// `centre + cos t a + sin t b`, a circle in the frame coordinates.
    Circle {
        /// The centre.
        centre: V3,
        /// The semi-diameter at `t = 0`.
        a: V3,
        /// The semi-diameter at `t = pi/2`.
        b: V3,
    },
    /// `centre + cos t a + sin t b`.
    Ellipse {
        /// The centre.
        centre: V3,
        /// The semi-diameter at `t = 0`.
        a: V3,
        /// The semi-diameter at `t = pi/2`.
        b: V3,
    },
    /// One branch, `centre + cosh t a + sinh t b`.
    Hyperbola {
        /// The centre.
        centre: V3,
        /// The transverse semi-diameter, to this branch's vertex.
        a: V3,
        /// The conjugate semi-diameter.
        b: V3,
    },
}

fn vadd(a: V3, b: V3) -> V3 {
    let ([a0, a1, a2], [b0, b1, b2]) = (a, b);
    [a0 + b0, a1 + b1, a2 + b2]
}

fn vscale(a: V3, s: Dd) -> V3 {
    a.map(|x| x * s)
}

fn vdd(a: &R3) -> V3 {
    a.each_ref().map(rat_to_dd)
}

/// `e^z` as one double-double.
fn exp(z: Dd) -> Dd {
    let (d, n) = dd::exp(z);
    d.scale(n)
}

impl Curve {
    /// The point at `t`.
    pub fn point(&self, t: Dd) -> V3 {
        match *self {
            Curve::Point(p) => p,
            Curve::Line { origin, direction } => vadd(origin, vscale(direction, t)),
            Curve::Circle { centre, a, b } | Curve::Ellipse { centre, a, b } => {
                let (s, c) = dd::sin_cos(t);
                vadd(centre, vadd(vscale(a, c), vscale(b, s)))
            }
            Curve::Hyperbola { centre, a, b } => {
                let (ep, em) = (exp(t), exp(-t));
                let (ch, sh) = ((ep + em) * 0.5, (ep - em) * 0.5);
                vadd(centre, vadd(vscale(a, ch), vscale(b, sh)))
            }
        }
    }

    /// The parameters the samples are taken at.
    fn sample_params(&self) -> Vec<Dd> {
        match self {
            Curve::Point(_) => vec![Dd::new(0.0)],
            Curve::Circle { .. } | Curve::Ellipse { .. } => {
                let step = dd::consts().pi * 0.25;
                (0..8).map(|j| step * f64::from(j)).collect()
            }
            Curve::Line { .. } | Curve::Hyperbola { .. } => {
                [-1.0, -0.5, 0.0, 0.5, 1.0].map(Dd::new).to_vec()
            }
        }
    }
}

/// A sample point of a piece, certified on both surfaces.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Sample {
    /// The curve parameter.
    pub t: Dd,
    /// The point.
    pub point: V3,
    /// Its STEP uv on each surface.
    pub uv: [(Dd, Dd); 2],
    /// Its pin uv (D13) on each surface; `None` on a cone's far nappe.
    pub pin_uv: [Option<(Dd, Dd)>; 2],
    /// The larger of its first-order distances from the two surfaces: each surface's exact
    /// implicit value at the point over its gradient's length there.
    pub gap: f64,
}

/// One connected piece of a section.
#[derive(Clone, Debug, PartialEq)]
pub struct Piece {
    /// The curve.
    pub curve: Curve,
    /// How the surfaces meet along it.
    pub contact: Contact,
    /// Sample points.
    pub samples: Vec<Sample>,
}

/// Where two surfaces meet.
#[derive(Clone, Debug, PartialEq)]
pub enum Section {
    /// Nowhere.
    Empty,
    /// They are the same surface.
    Coincident,
    /// Along these pieces.
    Curves(Vec<Piece>),
}

// ---------------------------------------------------------------- the frames

fn frame(s: &SurfaceKind) -> &Frame {
    match s {
        SurfaceKind::Plane(p) => &p.frame,
        SurfaceKind::Cylinder(c) => &c.frame,
        SurfaceKind::Cone(c) => &c.frame,
        SurfaceKind::Sphere(c) => &c.frame,
        SurfaceKind::Torus(c) => &c.frame,
    }
}

/// A surface's frame coordinates, its axes and its implicit form.
struct Space<'s> {
    surface: &'s SurfaceKind,
    exact: Exactly,
    cols: [R3; 3],
    layout: Layout,
}

impl<'s> Space<'s> {
    fn of(surface: &'s SurfaceKind) -> Result<Space<'s>, Error> {
        let exact = cs::exact(surface)?;
        let layout = cs::implicit(surface)?.pin_layout();
        let f = frame(surface);
        let (x, y) = (cs::r3(f.x)?, cs::r3(f.y)?);
        let z = match surface {
            SurfaceKind::Plane(_) => rcross(&x, &y),
            _ => cs::r3(f.z)?,
        };
        Ok(Space {
            surface,
            exact,
            cols: [x, y, z],
            layout,
        })
    }

    /// `M l`.
    fn vector(&self, l: &R3) -> R3 {
        core::array::from_fn(|i| {
            self.cols
                .iter()
                .zip(l)
                .fold(Rat::zero(), |s, (c, li)| s + &(&at(c, i) * li))
        })
    }

    /// `C + M l`.
    fn point(&self, l: &R3) -> R3 {
        radd(&self.exact.origin, &self.vector(l))
    }

    /// `M l` for frame coordinates in double-double.
    fn vector_dd(&self, l: V3) -> V3 {
        self.cols
            .iter()
            .zip(l)
            .fold([Dd::new(0.0); 3], |s, (c, li)| vadd(s, vscale(vdd(c), li)))
    }

    /// `C + M l` for frame coordinates in double-double.
    fn point_dd(&self, l: V3) -> V3 {
        vadd(vdd(&self.exact.origin), self.vector_dd(l))
    }

    /// A plane, as `m . L = d` in these coordinates.
    fn plane(&self, p: &Plane) -> Result<(R3, Rat), Error> {
        let n = rcross(&cs::r3(p.frame.x)?, &cs::r3(p.frame.y)?);
        let m = self.cols.each_ref().map(|c| rdot(c, &n));
        let o = cs::r3(p.frame.origin)?;
        let d = rdot(&n, &cs::rsub(&o, &self.exact.origin));
        Ok((m, d))
    }

    /// A point on this surface: its STEP and pin uv, and its first-order distance.
    fn on(&self, p: V3) -> ((Dd, Dd), Option<(Dd, Dd)>, f64) {
        let uv = cs::step_uv(self.surface, self.exact.local_dd(p));
        let exact = p.map(|x| dd_to_rat(x).unwrap_or_default());
        let gap = self.exact.distance(&exact);
        (uv, cs::pin_uv_of(self.surface, &self.layout, uv), gap)
    }
}

/// The pieces with their samples.
fn sampled(s: [&Space; 2], pieces: Vec<(Curve, Contact)>) -> Section {
    if pieces.is_empty() {
        return Section::Empty;
    }
    let [s0, s1] = s;
    Section::Curves(
        pieces
            .into_iter()
            .map(|(curve, contact)| {
                let samples = curve
                    .sample_params()
                    .into_iter()
                    .map(|t| {
                        let point = curve.point(t);
                        let (uv0, pin0, g0) = s0.on(point);
                        let (uv1, pin1, g1) = s1.on(point);
                        Sample {
                            t,
                            point,
                            uv: [uv0, uv1],
                            pin_uv: [pin0, pin1],
                            gap: g0.max(g1),
                        }
                    })
                    .collect();
                Piece {
                    curve,
                    contact,
                    samples,
                }
            })
            .collect(),
    )
}

fn same_axes(a: &Frame, b: &Frame) -> bool {
    a.x == b.x && a.y == b.y && a.z == b.z
}

// ---------------------------------------------------------------- the cone's slope

/// The slope's value and rounding band; with no slope, `k = 0` and no band.
struct Slope {
    k: Rat,
    band: Option<(Rat, Rat)>,
}

impl Slope {
    fn of(form: &Shape) -> Slope {
        match form {
            Shape::Cone { k, .. } => {
                let w = k.abs().scale(-96);
                Slope {
                    k: k.clone(),
                    band: Some((k - &w, k + &w)),
                }
            }
            _ => Slope {
                k: Rat::zero(),
                band: None,
            },
        }
    }

    /// The sign of `p` at the true slope (module docs).
    fn sign(&self, p: &Poly) -> Result<i32, Error> {
        if p.is_zero() {
            return Ok(0);
        }
        if let Some((lo, hi)) = &self.band {
            if p.degree().unwrap_or(0) > 0
                && p.real_roots(&cs::root_width())
                    .iter()
                    .any(|r| r.at.lo() <= hi && lo <= r.at.hi())
            {
                return Err(Error::Undecided);
            }
        }
        Ok(p.eval(&self.k).signum())
    }
}

// ---------------------------------------------------------------- a plane section

fn rscale(v: &R3, s: &Rat) -> R3 {
    v.each_ref().map(|x| x * s)
}

fn radd(a: &R3, b: &R3) -> R3 {
    let ([a0, a1, a2], [b0, b1, b2]) = (a, b);
    [a0 + b0, a1 + b1, a2 + b2]
}

fn at(v: &R3, j: usize) -> Rat {
    v.get(j).cloned().unwrap_or_default()
}

/// The quadric `sum_j Q_j L_j^2 + 2 q L_z + c0` with its coefficients as polynomials in
/// the cone's slope; `None` for the plane and the torus.
fn quadric(form: &Shape) -> Option<([Poly; 3], Poly, Poly)> {
    let one = || Poly::constant(Rat::one());
    Some(match form {
        Shape::Cylinder { r2 } => (
            [one(), one(), Poly::default()],
            Poly::default(),
            Poly::constant(-r2),
        ),
        Shape::Cone { r, .. } => (
            [
                one(),
                one(),
                Poly::new(vec![Rat::zero(), Rat::zero(), Rat::from(-1)]),
            ],
            Poly::new(vec![Rat::zero(), -r]),
            Poly::constant(-(r * r)),
        ),
        Shape::Sphere { r2 } => ([one(), one(), one()], Poly::default(), Poly::constant(-r2)),
        Shape::Plane | Shape::Torus { .. } => return None,
    })
}

/// The plane `m . L = d` across a quadric: `L = L0 + x e1 + y e2`, where the quadric is
/// `A x^2 + 2 B xy + C y^2 + 2 D x + 2 E y + F`, its coefficients polynomials in the slope.
struct Conic {
    l0: R3,
    e: [R3; 2],
    c: [Poly; 6],
}

impl Conic {
    fn of(form: &Shape, m: &R3, d: &Rat) -> Option<Conic> {
        let (q, ql, c0) = quadric(form)?;
        let l0 = rscale(m, &d.checked_div(&rdot(m, m))?);
        // e1 = m x (the axis of m's least component), e2 = m x e1: exact and independent.
        let i = (0..3).min_by_key(|&i| at(m, i).abs()).unwrap_or(0);
        let axis: R3 = core::array::from_fn(|j| if j == i { Rat::one() } else { Rat::zero() });
        let e1 = rcross(m, &axis);
        let e2 = rcross(m, &e1);
        let form2 = |u: &R3, v: &R3| {
            q.iter().enumerate().fold(Poly::default(), |s, (j, qj)| {
                s.add(&qj.scale(&(&at(u, j) * &at(v, j))))
            })
        };
        let lin = |u: &R3| form2(&l0, u).add(&ql.scale(&at(u, 2)));
        let f = form2(&l0, &l0)
            .add(&ql.scale(&at(&l0, 2).scale(1)))
            .add(&c0);
        let c = [
            form2(&e1, &e1),
            form2(&e1, &e2),
            form2(&e2, &e2),
            lin(&e1),
            lin(&e2),
            f,
        ];
        Some(Conic { l0, e: [e1, e2], c })
    }

    /// Frame coordinates of the plane vector `(x, y)`.
    fn direction(&self, [x, y]: &[Rat; 2]) -> R3 {
        let [e1, e2] = &self.e;
        radd(&rscale(e1, x), &rscale(e2, y))
    }

    /// Frame coordinates of the plane point `(x, y)`.
    fn local(&self, p: &[Rat; 2]) -> R3 {
        radd(&self.l0, &self.direction(p))
    }
}

fn v2(x: &Rat, y: &Rat) -> [Rat; 2] {
    [x.clone(), y.clone()]
}

fn sqrt(x: &Rat) -> Dd {
    rat_to_dd(x).sqrt()
}

/// The section of a quadric by a plane, decided exactly for the true slope (research 64
/// §4.2), as `(curve, contact)` in world coordinates.
fn plane_quadric(
    space: &Space,
    cn: &Conic,
    slope: &Slope,
    circular: bool,
) -> Result<Vec<(Curve, Contact)>, Error> {
    let [pa, pb, pc, pd, pe, pf] = &cn.c;
    let pdelta = pa.mul(pc).sub(&pb.mul(pb));
    let pbig = pa
        .mul(&pc.mul(pf).sub(&pe.mul(pe)))
        .sub(&pb.mul(&pb.mul(pf).sub(&pd.mul(pe))))
        .add(&pd.mul(&pb.mul(pe).sub(&pc.mul(pd))));
    let (sd, sbig) = (slope.sign(&pdelta)?, slope.sign(&pbig)?);
    let (sa, sc) = (slope.sign(pa)?, slope.sign(pc)?);
    let k = &slope.k;
    let [a, b, c, d, e, f] = [pa, pb, pc, pd, pe, pf].map(|p| p.eval(k));
    let (delta, big) = (pdelta.eval(k), pbig.eval(k));
    let quo = |x: &Rat, y: &Rat| x.checked_div(y).unwrap_or_default();
    let (zero, one) = (Rat::zero(), Rat::one());
    // A world point of plane coordinates, and a world vector times a square root.
    let point = |p: &[Rat; 2]| vdd(&space.point(&cn.local(p)));
    let vector = |u: &[Rat; 2], r: &Rat| vscale(vdd(&space.vector(&cn.direction(u))), sqrt(r));
    let flip = |v: V3| vscale(v, Dd::new(-1.0));
    let centre = || {
        v2(
            &quo(&(&(&b * &e) - &(&c * &d)), &delta),
            &quo(&(&(&b * &d) - &(&a * &e)), &delta),
        )
    };
    Ok(match sd {
        // An ellipse, a point or nothing; a circle on the sphere or across the axis.
        1 => {
            if sbig == 0 {
                return Ok(vec![(Curve::Point(point(&centre())), Contact::Tangent)]);
            }
            // Real when F' = big / delta, the value at the centre, is opposite to A.
            if sbig * sa > 0 {
                return Ok(Vec::new());
            }
            let fp = quo(&big, &delta);
            // Conjugate directions (1, 0) and (-B, A), with u S u = A and A delta.
            let (u, w) = (v2(&one, &zero), v2(&-&b, &a));
            let (ra, rb) = (quo(&-&fp, &a), quo(&-&fp, &(&a * &delta)));
            let (centre, a, b) = (point(&centre()), vector(&u, &ra), vector(&w, &rb));
            let curve = if circular {
                Curve::Circle { centre, a, b }
            } else {
                Curve::Ellipse { centre, a, b }
            };
            vec![(curve, Contact::Transversal)]
        }
        // A hyperbola, or two lines through the centre.
        -1 => {
            if sbig == 0 {
                // u S u = 0: (-B +- sqrt(-delta), A), or (1, 0) and (C, -2B) when A = 0.
                let origin = point(&centre());
                let dirs = if sa == 0 {
                    [
                        vector(&v2(&one, &zero), &one),
                        vector(&v2(&c, &-b.scale(1)), &one),
                    ]
                } else {
                    let base = vector(&v2(&-&b, &a), &one);
                    let root = vector(&v2(&one, &zero), &-&delta);
                    [vadd(base, root), vadd(base, flip(root))]
                };
                return Ok(dirs
                    .into_iter()
                    .map(|direction| (Curve::Line { origin, direction }, Contact::Transversal))
                    .collect());
            }
            let fp = quo(&big, &delta);
            // Conjugate pairs (u, u S u): (1,0), A with (-B,A), A delta; (0,1), C with
            // (C,-B), C delta; or (1,1), 2B with (1,-1), -2B.
            let (u1, s1, u2, s2) = if sa != 0 {
                (v2(&one, &zero), a.clone(), v2(&-&b, &a), &a * &delta)
            } else if sc != 0 {
                (v2(&zero, &one), c.clone(), v2(&c, &-&b), &c * &delta)
            } else {
                (v2(&one, &one), b.scale(1), v2(&one, &-&one), -b.scale(1))
            };
            // The transverse direction's u S u is opposite to F'.
            let ((ut, st), (uc, sc)) = if (&s1 * &fp).signum() < 0 {
                ((u1, s1), (u2, s2))
            } else {
                ((u2, s2), (u1, s1))
            };
            let centre = point(&centre());
            let ta = vector(&ut, &quo(&-&fp, &st));
            let cb = vector(&uc, &quo(&fp, &sc));
            [ta, flip(ta)]
                .into_iter()
                .map(|a| (Curve::Hyperbola { centre, a, b: cb }, Contact::Transversal))
                .collect()
        }
        // S has rank one: a cylinder cut parallel to its axis (the cone's delta is never
        // the zero polynomial, research 64 §4.2), lines along the null direction u0.
        _ => {
            if sbig != 0 {
                // A parabola: no plane section of these quadrics reaches it exactly.
                return Err(Error::NoClosedForm);
            }
            // u1 S u1 = s1 and g1 = g . u1: s1 x^2 + 2 g1 x + F = 0 along u1.
            let (u0, u1, s1, g1, pdisc) = if sa == 0 {
                (
                    v2(&one, &zero),
                    v2(&zero, &one),
                    c.clone(),
                    e.clone(),
                    pe.mul(pe).sub(&pc.mul(pf)),
                )
            } else {
                (
                    v2(&-&b, &a),
                    v2(&one, &zero),
                    a.clone(),
                    d.clone(),
                    pd.mul(pd).sub(&pa.mul(pf)),
                )
            };
            let direction = vector(&u0, &one);
            let x0 = quo(&-&g1, &s1);
            let origin = point(&[&u1[0] * &x0, &u1[1] * &x0]);
            match slope.sign(&pdisc)? {
                0 => vec![(Curve::Line { origin, direction }, Contact::Tangent)],
                1 => {
                    let disc = &(&g1 * &g1) - &(&s1 * &f);
                    let half = vector(&u1, &quo(&disc, &(&s1 * &s1)));
                    [half, flip(half)]
                        .into_iter()
                        .map(|h| {
                            let line = Curve::Line {
                                origin: vadd(origin, h),
                                direction,
                            };
                            (line, Contact::Transversal)
                        })
                        .collect()
                }
                _ => Vec::new(),
            }
        }
    })
}

// ---------------------------------------------------------------- coaxial pairs

/// A surface of revolution's meridian against the shared axial coordinate `h`.
enum Meridian {
    /// A plane across the axis at a height.
    Height(Rat),
    /// `rho^2 = P(h)`: a cylinder, cone or sphere.
    RhoSq(Poly),
    /// `rho^2 - 2 R rho + (h - h0)^2 + R^2 - r^2 = 0`.
    Torus { big2: Rat, small2: Rat, h0: Rat },
}

/// The meridian of a form whose own axial coordinate is `h - h0`, at slope `k`.
fn meridian(form: &Shape, h0: &Rat, k: &Rat) -> Option<Meridian> {
    let z = Poly::linear(-h0, Rat::one());
    Some(match form {
        Shape::Plane => return None,
        Shape::Cylinder { r2 } => Meridian::RhoSq(Poly::constant(r2.clone())),
        Shape::Cone { r, .. } => {
            let g = z.scale(k).add(&Poly::constant(r.clone()));
            Meridian::RhoSq(g.mul(&g))
        }
        Shape::Sphere { r2 } => Meridian::RhoSq(Poly::constant(r2.clone()).sub(&z.mul(&z))),
        Shape::Torus { big2, small2 } => Meridian::Torus {
            big2: big2.clone(),
            small2: small2.clone(),
            h0: h0.clone(),
        },
    })
}

/// A circle (or a point, radius zero) at a height, with its contact.
struct Ring {
    h: Dd,
    rho: Dd,
    contact: Contact,
}

fn of_multiplicity(m: u32) -> Contact {
    if m > 1 {
        Contact::Tangent
    } else {
        Contact::Transversal
    }
}

/// The rings of `rho = R +- sqrt(disc)` at height `h`: two, one tangent, or none.
fn split(h: &Rat, big: Dd, disc: &Rat) -> Vec<Ring> {
    let ring = |rho, contact| Ring {
        h: rat_to_dd(h),
        rho,
        contact,
    };
    match disc.signum() {
        1 => {
            let w = sqrt(disc);
            vec![
                ring(big - w, Contact::Transversal),
                ring(big + w, Contact::Transversal),
            ]
        }
        0 => vec![ring(big, Contact::Tangent)],
        _ => Vec::new(),
    }
}

/// The rings at the roots of `g` with `rho^2 = p(h) >= 0`; `None` when `g` is zero (the
/// surfaces coincide). A common root of `g` and `p` is a point on the axis, decided
/// exactly by their gcd; elsewhere `rho^2` is `p` at the root refined to `2^-110`.
fn rings_on(g: &Poly, p: &Poly) -> Option<Vec<Ring>> {
    if g.is_zero() {
        return None;
    }
    let points = g.gcd(p).real_roots(&cs::root_width());
    Some(
        g.real_roots(&cs::root_width())
            .into_iter()
            .filter_map(|root| {
                let h = root.at.to_dd();
                if points.iter().any(|q| q.at.meets(&root.at)) {
                    return Some(Ring {
                        h,
                        rho: Dd::new(0.0),
                        contact: Contact::Tangent,
                    });
                }
                let rho2 = p.eval_dd(h);
                (rho2.hi > 0.0).then(|| Ring {
                    h,
                    rho: rho2.sqrt(),
                    contact: of_multiplicity(root.multiplicity),
                })
            })
            .collect(),
    )
}

/// A torus's meridian `rho^2 - 2 R rho + (h - h0)^2 + R^2 - r^2 = 0` with `rho^2 = p(h)`,
/// squared out: `(p + (h - h0)^2 + R^2 - r^2)^2 - 4 R^2 p`.
fn torus_of(p: &Poly, big2: &Rat, small2: &Rat, h0: &Rat) -> Poly {
    let z = Poly::linear(-h0, Rat::one());
    let s = p.add(&z.mul(&z)).add(&Poly::constant(big2 - small2));
    s.mul(&s).sub(&p.scale(&big2.scale(2)))
}

/// The coaxial section; `None` when the surfaces coincide.
fn coaxial(m1: &Meridian, m2: &Meridian) -> Option<Vec<Ring>> {
    match (m1, m2) {
        (Meridian::Height(a), Meridian::Height(b)) => (a != b).then(Vec::new),
        (Meridian::Height(h), Meridian::RhoSq(p)) | (Meridian::RhoSq(p), Meridian::Height(h)) => {
            rings_on(&Poly::linear(-h, Rat::one()), p)
        }
        (Meridian::RhoSq(p1), Meridian::RhoSq(p2)) => rings_on(&p1.sub(p2), p1),
        (Meridian::RhoSq(p), Meridian::Torus { big2, small2, h0 })
        | (Meridian::Torus { big2, small2, h0 }, Meridian::RhoSq(p)) => {
            rings_on(&torus_of(p, big2, small2, h0), p)
        }
        (Meridian::Height(h), Meridian::Torus { big2, small2, h0 })
        | (Meridian::Torus { big2, small2, h0 }, Meridian::Height(h)) => {
            let z = h - h0;
            Some(split(h, sqrt(big2), &(small2 - &(&z * &z))))
        }
        (
            Meridian::Torus {
                big2: b1,
                small2: s1,
                h0: a1,
            },
            Meridian::Torus {
                big2: b2,
                small2: s2,
                h0: a2,
            },
        ) => tori([b1, s1, a1], [b2, s2, a2]),
    }
}

/// Two coaxial tori: `rho^2 - 2 R_i rho + (h - a_i)^2 + c_i = 0`, `c_i = R_i^2 - r_i^2`
/// (the positive root of the meridian's square, as `rho^2 + ... >= c_i > 0`).
fn tori([b1, s1, a1]: [&Rat; 3], [b2, s2, a2]: [&Rat; 3]) -> Option<Vec<Ring>> {
    // R_i^2 is the square of a double, so R_i is that double exactly.
    let big = |b2: &Rat| dd_to_rat(sqrt(b2)).unwrap_or_default();
    let (r1, r2) = (big(b1), big(b2));
    let (c1, c2) = (b1 - s1, b2 - s2);
    let z = |a: &Rat| Poly::linear(-a, Rat::one());
    let (z1, z2) = (z(a1), z(a2));
    // Their difference: -2 (R1 - R2) rho + (h - a1)^2 - (h - a2)^2 + c1 - c2 = 0.
    let rest = z1
        .mul(&z1)
        .sub(&z2.mul(&z2))
        .add(&Poly::constant(&c1 - &c2));
    if r1 == r2 {
        // Linear in h (or zero: the same torus, or none).
        if rest.is_zero() {
            return None;
        }
        // Its one root, exactly: `-c_0 / c_1`.
        return Some(match rest.coeffs() {
            [c0, c1] => c0.checked_div(c1).map_or_else(Vec::new, |h| {
                let h = -&h;
                let zz = &h - a1;
                split(&h, rat_to_dd(&r1), &(s1 - &(&zz * &zz)))
            }),
            _ => Vec::new(),
        });
    }
    // rho = rest / 2 (R1 - R2), linear in h, into the first.
    let rho = rest.scale(&(&r1 - &r2).scale(1).recip().unwrap_or_default());
    let g = rho
        .mul(&rho)
        .sub(&rho.scale(&r1.scale(1)))
        .add(&z1.mul(&z1))
        .add(&Poly::constant(c1));
    Some(
        g.real_roots(&cs::root_width())
            .into_iter()
            .filter_map(|root| {
                let h = root.at.to_dd();
                let rho = rho.eval_dd(h);
                (rho.hi > 0.0).then(|| Ring {
                    h,
                    rho,
                    contact: of_multiplicity(root.multiplicity),
                })
            })
            .collect(),
    )
}

// ---------------------------------------------------------------- the truth

/// Where two surfaces meet (module docs). Each sample's `uv[i]` is on surface `i + 1`.
///
/// # Errors
///
/// [`Error::Surface`] for bad data, [`Error::NoClosedForm`] for a pair outside the closed
/// forms, [`Error::Undecided`] when the cone's slope rounding does not decide the answer.
pub fn intersect(s1: &SurfaceKind, s2: &SurfaceKind) -> Result<Section, Error> {
    let (a, b) = (Space::of(s1)?, Space::of(s2)?);
    match (s1, s2) {
        (SurfaceKind::Plane(_), SurfaceKind::Plane(p)) => planes(&a, &b, p),
        (SurfaceKind::Plane(p), _) => plane_surface(&b, [&a, &b], p),
        (_, SurfaceKind::Plane(p)) => plane_surface(&a, [&a, &b], p),
        (SurfaceKind::Sphere(_), SurfaceKind::Sphere(_)) if same_axes(frame(s1), frame(s2)) => {
            spheres(&a, &b)
        }
        _ if same_axes(frame(s1), frame(s2)) => revolved(&a, &b),
        _ => Err(Error::NoClosedForm),
    }
}

/// The first plane is `L_z = 0` in its own coordinates, the second `m . L = d`.
fn planes(a: &Space, b: &Space, p2: &Plane) -> Result<Section, Error> {
    let (m, d) = a.plane(p2)?;
    let [mx, my, _] = &m;
    let mm = &(mx * mx) + &(my * my);
    if mm.is_zero() {
        return Ok(if d.is_zero() {
            Section::Coincident
        } else {
            Section::Empty
        });
    }
    let s = d.checked_div(&mm).unwrap_or_default();
    let origin = vdd(&a.point(&[mx * &s, my * &s, Rat::zero()]));
    let direction = vdd(&a.vector(&[-my, mx.clone(), Rat::zero()]));
    Ok(sampled(
        [a, b],
        vec![(Curve::Line { origin, direction }, Contact::Transversal)],
    ))
}

/// The plane `p` across the curved surface `q`; `both` orders the samples' surfaces.
fn plane_surface(q: &Space, both: [&Space; 2], p: &Plane) -> Result<Section, Error> {
    let (m, d) = q.plane(p)?;
    let [mx, my, mz] = &m;
    let across = mx.is_zero() && my.is_zero();
    if let Shape::Torus { .. } = q.exact.shape {
        if !across {
            return Err(Error::NoClosedForm);
        }
        let h = d.checked_div(mz).unwrap_or_default();
        let torus =
            meridian(&q.exact.shape, &Rat::zero(), &Rat::zero()).ok_or(Error::NoClosedForm)?;
        return Ok(rings(both, q, coaxial(&Meridian::Height(h), &torus)));
    }
    let cn = Conic::of(&q.exact.shape, &m, &d).ok_or(Error::NoClosedForm)?;
    let circular = across || matches!(q.exact.shape, Shape::Sphere { .. });
    let pieces = plane_quadric(q, &cn, &Slope::of(&q.exact.shape), circular)?;
    Ok(sampled(both, pieces))
}

/// `|L|^2 = r1^2` and `|L - o|^2 = r2^2`: the radical plane `2 o . L = r1^2 - r2^2 + |o|^2`
/// across the first.
fn spheres(a: &Space, b: &Space) -> Result<Section, Error> {
    let o = a.exact.local(&b.exact.origin);
    let (Shape::Sphere { r2: q1 }, Shape::Sphere { r2: q2 }) = (&a.exact.shape, &b.exact.shape)
    else {
        return Err(Error::NoClosedForm);
    };
    let oo = rdot(&o, &o);
    if oo.is_zero() {
        return Ok(if q1 == q2 {
            Section::Coincident
        } else {
            Section::Empty
        });
    }
    let m = o.each_ref().map(|x| x.scale(1));
    let d = &(q1 - q2) + &oo;
    let cn = Conic::of(&a.exact.shape, &m, &d).ok_or(Error::NoClosedForm)?;
    let pieces = plane_quadric(a, &cn, &Slope::of(&a.exact.shape), true)?;
    Ok(sampled([a, b], pieces))
}

/// Two surfaces of revolution on one axis, in the first's axial coordinate.
fn revolved(a: &Space, b: &Space) -> Result<Section, Error> {
    let o = a.exact.local(&b.exact.origin);
    let [ox, oy, oz] = &o;
    if !(ox.is_zero() && oy.is_zero()) {
        return Err(Error::NoClosedForm);
    }
    let (sa, sb) = (&a.exact.shape, &b.exact.shape);
    let (ka, kb) = (Slope::of(sa), Slope::of(sb));
    let m1 = meridian(sa, &Rat::zero(), &ka.k).ok_or(Error::NoClosedForm)?;
    let m2 = meridian(sb, oz, &kb.k).ok_or(Error::NoClosedForm)?;
    banded::prove(sa, sb, oz)?;
    Ok(rings([a, b], a, coaxial(&m1, &m2)))
}

/// The proof that a coaxial answer with a cone holds for the true slopes: each slope is
/// `k_i + e_i` with `|e_i|` within its band, and the answer's shape is fixed by the signs
/// of a few polynomials in `e` (the section polynomial's coefficients, its discriminant,
/// and its value at each cone's apex). A sign is proved when the constant term outweighs
/// the sum of the other terms at the band's corner, or when the polynomial is zero; any
/// sign not proved is [`Error::Undecided`]. Two cones with the same slope share one `e`.
mod banded {
    use std::collections::BTreeMap;

    use super::{Error, Meridian, Poly, Rat, Shape};

    /// A polynomial in `(e_0, e_1)`, by exponent pair.
    #[derive(Clone, Debug, Default)]
    pub(super) struct E(BTreeMap<(u32, u32), Rat>);

    impl E {
        pub(super) fn constant(c: Rat) -> E {
            let mut m = BTreeMap::new();
            if !c.is_zero() {
                m.insert((0, 0), c);
            }
            E(m)
        }

        /// `k + e_i`.
        pub(super) fn slope(k: &Rat, i: usize) -> E {
            let mut e = E::constant(k.clone());
            e.0.insert(if i == 0 { (1, 0) } else { (0, 1) }, Rat::one());
            e
        }

        fn add(&self, o: &E) -> E {
            let mut m = self.0.clone();
            for (p, c) in &o.0 {
                let v = m.get(p).map_or_else(|| c.clone(), |x| x + c);
                if v.is_zero() {
                    m.remove(p);
                } else {
                    m.insert(*p, v);
                }
            }
            E(m)
        }

        fn neg(&self) -> E {
            E(self.0.iter().map(|(p, c)| (*p, -c)).collect())
        }

        pub(super) fn sub(&self, o: &E) -> E {
            self.add(&o.neg())
        }

        pub(super) fn mul(&self, o: &E) -> E {
            self.0.iter().fold(E::default(), |acc, ((i, j), a)| {
                let t = E(o
                    .0
                    .iter()
                    .map(|((k, l), b)| ((i + k, j + l), a * b))
                    .collect());
                acc.add(&t)
            })
        }

        /// The sign over the band `|e_i| <= w_i`, when proved.
        pub(super) fn sign(&self, w: &[Rat; 2]) -> Option<i32> {
            let c = self.0.get(&(0, 0)).cloned().unwrap_or_default();
            let pow = |x: &Rat, n: u32| (0..n).fold(Rat::one(), |p, _| &p * x);
            let [w0, w1] = w;
            let rest = self
                .0
                .iter()
                .filter(|(p, _)| **p != (0, 0))
                .fold(Rat::zero(), |s, ((i, j), a)| {
                    &s + &(&a.abs() * &(&pow(w0, *i) * &pow(w1, *j)))
                });
            if c.is_zero() && rest.is_zero() {
                return Some(0);
            }
            (c.abs() > rest).then(|| c.signum())
        }
    }

    /// A polynomial in the axial coordinate `h` with coefficients in `e`.
    type H = Vec<E>;

    fn hadd(a: &H, b: &H) -> H {
        let n = a.len().max(b.len());
        (0..n)
            .map(|i| match (a.get(i), b.get(i)) {
                (Some(x), Some(y)) => x.add(y),
                (Some(x), None) | (None, Some(x)) => x.clone(),
                (None, None) => E::default(),
            })
            .collect()
    }

    fn hmul(a: &H, b: &H) -> H {
        let mut out = vec![E::default(); (a.len() + b.len()).saturating_sub(1)];
        for (i, x) in a.iter().enumerate() {
            for (j, y) in b.iter().enumerate() {
                if let Some(o) = out.get_mut(i + j) {
                    *o = o.add(&x.mul(y));
                }
            }
        }
        out
    }

    fn hneg(a: &H) -> H {
        a.iter().map(E::neg).collect()
    }

    /// A meridian `rho^2 = p(h)` and, for a cone, its `rho = r + k (h - h0)`; `None` for a
    /// form with another meridian (the torus).
    fn meridian(form: &Shape, h0: &Rat, k: &E) -> Option<(H, Option<H>)> {
        let c = |x: Rat| E::constant(x);
        let z = vec![c(-h0), c(Rat::one())];
        Some(match form {
            Shape::Cylinder { r2 } => (vec![c(r2.clone())], None),
            Shape::Cone { r, .. } => {
                let l = hadd(&vec![c(r.clone())], &z.iter().map(|t| t.mul(k)).collect());
                (hmul(&l, &l), Some(l))
            }
            Shape::Sphere { r2 } => (hadd(&vec![c(r2.clone())], &hneg(&hmul(&z, &z))), None),
            Shape::Plane | Shape::Torus { .. } => return None,
        })
    }

    /// `k^d g(h0 - r / k)` for `rho = r + k (h - h0)` written `l = [r - k h0, k]`: zero
    /// exactly when `g` has a root at the cone's apex.
    fn at_apex(g: &H, l: &H) -> E {
        let (Some(l0), Some(l1)) = (l.first(), l.get(1)) else {
            return E::default();
        };
        // the apex is h = -l0 / l1
        let d = g.len().saturating_sub(1);
        g.iter().enumerate().fold(E::default(), |s, (i, ci)| {
            let mut t = ci.clone();
            for _ in 0..i {
                t = t.mul(&l0.neg());
            }
            for _ in i..d {
                t = t.mul(l1);
            }
            s.add(&t)
        })
    }

    /// The nodes the polynomials in `k` of a cone against a torus are interpolated at: more
    /// than their degree (at most `7 * 4` for the quartic's resultant with its derivative,
    /// its coefficients being of degree at most 4 in `k`).
    const NODES: i64 = 30;

    /// The polynomial of degree below [`NODES`] through `f` at `k = 1, ..., NODES` (Newton's
    /// divided differences, exactly).
    fn in_k(f: impl Fn(&Rat) -> Rat) -> Poly {
        let xs: Vec<Rat> = (1..=NODES).map(Rat::from).collect();
        let mut c: Vec<Rat> = xs.iter().map(&f).collect();
        for j in 1..c.len() {
            for i in (j..c.len()).rev() {
                let (Some(xi), Some(xj)) = (xs.get(i), xs.get(i - j)) else {
                    continue;
                };
                let d = c.get(i).cloned().unwrap_or_default()
                    - c.get(i - 1).cloned().unwrap_or_default();
                if let Some(ci) = c.get_mut(i) {
                    *ci = d.checked_div(&(xi - xj)).unwrap_or_default();
                }
            }
        }
        c.iter()
            .zip(&xs)
            .rev()
            .fold(Poly::default(), |p, (ci, xi)| {
                p.mul(&Poly::linear(-xi, Rat::one()))
                    .add(&Poly::constant(ci.clone()))
            })
    }

    /// The determinant, by exact elimination.
    fn det(mut m: Vec<Vec<Rat>>) -> Rat {
        let n = m.len();
        let mut d = Rat::one();
        for c in 0..n {
            let Some(p) = (c..n).find(|&r| {
                m.get(r)
                    .and_then(|row| row.get(c))
                    .is_some_and(|x| !x.is_zero())
            }) else {
                return Rat::zero();
            };
            if p != c {
                m.swap(p, c);
                d = -&d;
            }
            let pivot = m.get(c).cloned().unwrap_or_default();
            let pc = pivot.get(c).cloned().unwrap_or_default();
            d = &d * &pc;
            for row in m.iter_mut().skip(c + 1) {
                let f = row
                    .get(c)
                    .cloned()
                    .unwrap_or_default()
                    .checked_div(&pc)
                    .unwrap_or_default();
                if f.is_zero() {
                    continue;
                }
                for (x, y) in row.iter_mut().zip(&pivot).skip(c) {
                    *x = &*x - &(&f * y);
                }
            }
        }
        d
    }

    /// The Sylvester resultant of `f` and `g` of formal degrees `m` and `n`.
    fn resultant(f: &[Rat], m: usize, g: &[Rat], n: usize) -> Rat {
        let size = m + n;
        let row = |c: &[Rat], deg: usize, shift: usize| -> Vec<Rat> {
            (0..size)
                .map(|j| {
                    // highest power first
                    j.checked_sub(shift)
                        .filter(|&i| i <= deg)
                        .and_then(|i| c.get(deg - i))
                        .cloned()
                        .unwrap_or_default()
                })
                .collect()
        };
        det((0..n)
            .map(|s| row(f, m, s))
            .chain((0..m).map(|s| row(g, n, s)))
            .collect())
    }

    /// A cone against a torus coaxially: the quartic's resultant with its derivative (its
    /// degree and square-freeness) and its value at the apex, as polynomials in the one
    /// slope `k`, keep their signs over the band (the slope's [`super::Slope::sign`]).
    fn torus(a: &Shape, b: &Shape, oz: &Rat, slope: &super::Slope) -> Result<(), Error> {
        const D: usize = 4;
        let quartic = |k: &Rat| -> Vec<Rat> {
            let m1 = super::meridian(a, &Rat::zero(), k);
            let m2 = super::meridian(b, oz, k);
            let g = match (m1, m2) {
                (Some(Meridian::RhoSq(p)), Some(Meridian::Torus { big2, small2, h0 }))
                | (Some(Meridian::Torus { big2, small2, h0 }), Some(Meridian::RhoSq(p))) => {
                    super::torus_of(&p, &big2, &small2, &h0)
                }
                _ => Poly::default(),
            };
            (0..=D)
                .map(|i| g.coeffs().get(i).cloned().unwrap_or_default())
                .collect()
        };
        let cone = |k: &Rat| -> (Rat, Rat) {
            // rho = r + k (h - h0): (r - k h0, k)
            match (a, b) {
                (Shape::Cone { r, .. }, _) => (r.clone(), k.clone()),
                (_, Shape::Cone { r, .. }) => (r - &(k * oz), k.clone()),
                _ => (Rat::zero(), Rat::zero()),
            }
        };
        let squarefree = in_k(|k| {
            let g = quartic(k);
            let dg: Vec<Rat> = g
                .iter()
                .enumerate()
                .skip(1)
                .map(|(i, c)| c * &Rat::from(i as i64))
                .collect();
            resultant(&g, D, &dg, D - 1)
        });
        let apex = in_k(|k| {
            let (l0, l1) = cone(k);
            let g = quartic(k);
            g.iter().enumerate().fold(Rat::zero(), |s, (i, ci)| {
                let mut t = ci.clone();
                for _ in 0..i {
                    t = &t * &-&l0;
                }
                for _ in i..D {
                    t = &t * &l1;
                }
                &s + &t
            })
        });
        if slope.sign(&squarefree)? == 0 {
            return Err(Error::Undecided);
        }
        slope.sign(&apex)?;
        Ok(())
    }

    /// Proves the coaxial answer of `a` and `b` (at axial offset `oz`) at the slopes'
    /// values holds over their bands (module docs); `Ok` too when neither is a cone.
    pub(super) fn prove(a: &Shape, b: &Shape, oz: &Rat) -> Result<(), Error> {
        let (ka, kb) = (super::Slope::of(a), super::Slope::of(b));
        if ka.band.is_none() && kb.band.is_none() {
            return Ok(());
        }
        let width = |s: &super::Slope| s.band.as_ref().map(|(lo, hi)| (hi - lo).scale(-1));
        let shared = ka.band.is_some() && kb.band.is_some() && ka.k == kb.k;
        let w = [
            width(&ka).or_else(|| width(&kb)).unwrap_or_default(),
            if shared || ka.band.is_none() {
                Rat::zero()
            } else {
                width(&kb).unwrap_or_default()
            },
        ];
        let ea = E::slope(&ka.k, 0);
        let eb = E::slope(&kb.k, usize::from(ka.band.is_some() && !shared));
        let (Some((p1, l1)), Some((p2, l2))) =
            (meridian(a, &Rat::zero(), &ea), meridian(b, oz, &eb))
        else {
            return torus(a, b, oz, if ka.band.is_some() { &ka } else { &kb });
        };
        let proved = |e: &E| e.sign(&w).ok_or(Error::Undecided);
        let mut g = hadd(&p1, &hneg(&p2));
        // the degree: the highest coefficient not zero is nonzero over the band
        while let Some(top) = g.last() {
            if proved(top)? != 0 {
                break;
            }
            g.pop();
        }
        // a quadratic's roots keep their count and contact when its discriminant's sign does
        if let [c0, c1, c2] = g.as_slice() {
            let four = E::constant(Rat::from(4));
            proved(&c1.mul(c1).sub(&four.mul(c2).mul(c0)))?;
        }
        // a root at a cone's apex (a point on the axis) stays one, or never is
        for l in [l1, l2].iter().flatten() {
            proved(&at_apex(&g, l))?;
        }
        Ok(())
    }
}

/// Coaxial rings as section pieces, in the frame of `axis`.
fn rings(both: [&Space; 2], axis: &Space, rings: Option<Vec<Ring>>) -> Section {
    let Some(rings) = rings else {
        return Section::Coincident;
    };
    let zero = Dd::new(0.0);
    let pieces = rings
        .into_iter()
        .map(|r| {
            let centre = axis.point_dd([zero, zero, r.h]);
            let curve = if r.rho.hi == 0.0 {
                Curve::Point(centre)
            } else {
                Curve::Circle {
                    centre,
                    a: axis.vector_dd([r.rho, zero, zero]),
                    b: axis.vector_dd([zero, r.rho, zero]),
                }
            };
            (curve, r.contact)
        })
        .collect();
    sampled(both, pieces)
}

// ---------------------------------------------------------------- WR-680

/// One span of a plane's section of the pin's NURBS cylinder: a rational quadratic Bezier
/// over the span's local parameter `s` in `[0, 1]`, exact in rationals.
#[derive(Clone, Debug, PartialEq)]
pub struct Span {
    /// The pin's u at the span's ends (its knots, `k pi` rounded).
    pub knots: (Rat, Rat),
    /// The 3D control points.
    pub points: [[Rat; 3]; 3],
    /// Their weights: the pin's quarter arc's, `(1, cos(pi/4) rounded, 1)`.
    pub weights: [Rat; 3],
    /// The pin's u along the span, `knots.0 + s (knots.1 - knots.0)`.
    pub u: Poly,
    /// The pin's v (the height) is `v_num(s) / weight(s)`, both quadratic.
    pub v_num: Poly,
    /// `sum_i w_i B_i(s)`.
    pub weight: Poly,
}

/// The pin's quarter arcs, span by span: their control points in the frame's `(x, y)`
/// over the radius.
pub const ARCS: [[[i64; 2]; 3]; 4] = [
    [[1, 0], [1, 1], [0, 1]],
    [[0, 1], [-1, 1], [-1, 0]],
    [[-1, 0], [-1, -1], [0, -1]],
    [[0, -1], [1, -1], [1, 0]],
];

/// The Bernstein quadratics `(1 - s)^2, 2 s (1 - s), s^2`.
pub fn bernstein() -> [Poly; 3] {
    let p = |v: [i64; 3]| Poly::new(v.map(Rat::from).to_vec());
    [p([1, -2, 1]), p([0, 2, -2]), p([0, 0, 1])]
}

/// WR-680: a plane's section of the pin's NURBS cylinder (its four quarter-arc spans, the
/// height linear), exactly. On span `k` the cylinder is `C + N(s) / W(s) + v Z` with
/// `N = sum w_i B_i r (a_i X + b_i Y)`, so the plane `n . (P - P0) = 0` holds at
/// `v = H(s) / W(s)`, `H = sum w_i B_i c_i`, `c_i = (n . (P0 - C) - r n . (a_i X +
/// b_i Y)) / n . Z`: the section is the rational quadratic with points
/// `C + r (a_i X + b_i Y) + c_i Z` and the arc's weights, on the pin's surface exactly, and
/// its uv curve is `(u(s), H(s) / W(s))`.
///
/// # Errors
///
/// [`Error::Surface`] for bad data, [`Error::NoClosedForm`] for a plane parallel to the
/// axis (no ellipse).
pub fn cylinder_section(cyl: &Cylinder, plane: &Plane) -> Result<[Span; 4], Error> {
    let f = &cyl.frame;
    let (c, x, y, z) = (cs::r3(f.origin)?, cs::r3(f.x)?, cs::r3(f.y)?, cs::r3(f.z)?);
    let r = cs::rf(cyl.radius)?;
    if r.signum() <= 0 {
        return Err(Error::Surface(cs::Error::DegenerateSurface));
    }
    let n = rcross(&cs::r3(plane.frame.x)?, &cs::r3(plane.frame.y)?);
    let nz = rdot(&n, &z);
    if nz.is_zero() {
        return Err(Error::NoClosedForm);
    }
    let d = rdot(&n, &cs::rsub(&cs::r3(plane.frame.origin)?, &c));
    let weights = [
        Rat::one(),
        cs::rf(core::f64::consts::FRAC_1_SQRT_2)?,
        Rat::one(),
    ];
    let b = bernstein();
    let weight = b
        .iter()
        .zip(&weights)
        .fold(Poly::default(), |s, (bi, wi)| s.add(&bi.scale(wi)));
    let breaks = match cs::implicit(&SurfaceKind::Cylinder(cyl.clone()))?
        .pin_layout()
        .u
    {
        Param::Arcs { breaks, .. } => breaks,
        Param::Linear { .. } => Vec::new(),
    };
    let knot = |k: usize| cs::rf(breaks.get(k).copied().unwrap_or(0.0));
    let span = |k: usize, arc: &[[i64; 2]; 3]| -> Result<Span, Error> {
        let (lo, hi) = (knot(k)?, knot(k + 1)?);
        let mut points: [R3; 3] = Default::default();
        let mut cz: [Rat; 3] = Default::default();
        for ((p, ci), [ai, bi]) in points.iter_mut().zip(&mut cz).zip(arc) {
            let rim = rscale(
                &radd(&rscale(&x, &Rat::from(*ai)), &rscale(&y, &Rat::from(*bi))),
                &r,
            );
            *ci = (&d - &rdot(&n, &rim)).checked_div(&nz).unwrap_or_default();
            *p = radd(&radd(&c, &rim), &rscale(&z, ci));
        }
        let v_num = b
            .iter()
            .zip(weights.iter().zip(&cz))
            .fold(Poly::default(), |s, (bi, (wi, ci))| {
                s.add(&bi.scale(&(wi * ci)))
            });
        Ok(Span {
            u: Poly::linear(lo.clone(), &hi - &lo),
            knots: (lo, hi),
            points,
            weights: weights.clone(),
            v_num,
            weight: weight.clone(),
        })
    };
    let [a0, a1, a2, a3] = &ARCS;
    Ok([span(0, a0)?, span(1, a1)?, span(2, a2)?, span(3, a3)?])
}

#[cfg(test)]
mod tests {
    use super::{Error, Poly, Rat, Shape, Slope};

    /// A predicate with a root inside the slope's band is undecided; one just outside it,
    /// or the zero polynomial, is decided.
    #[test]
    fn the_slope_band_decides_or_refuses() {
        let k = Rat::from(3).scale(-2);
        let slope = Slope::of(&Shape::Cone {
            r: Rat::one(),
            k: k.clone(),
        });
        let near = &k + &k.scale(-100);
        let far = &k + &k.scale(-90);
        let at = |x: &Rat| Poly::linear(-x, Rat::one());
        assert_eq!(slope.sign(&at(&near)), Err(Error::Undecided));
        assert_eq!(slope.sign(&at(&far)), Ok(-1));
        assert_eq!(slope.sign(&at(&k).mul(&at(&k))), Err(Error::Undecided));
        assert_eq!(slope.sign(&Poly::default()), Ok(0));
        assert_eq!(slope.sign(&Poly::constant(Rat::from(-2))), Ok(-1));
    }

    /// A polynomial in the slopes' errors is signed over the band only when its constant
    /// term outweighs the rest at the band's corner; the zero polynomial is zero.
    #[test]
    fn the_band_bound_decides_or_refuses() {
        let k = Rat::from(3).scale(-2);
        let w = [k.scale(-96), Rat::zero()];
        let e =
            |c: &Rat| super::banded::E::slope(&k, 0).sub(&super::banded::E::constant(c.clone()));
        assert_eq!(e(&(&k + &k.scale(-100))).sign(&w), None);
        assert_eq!(e(&(&k + &k.scale(-90))).sign(&w), Some(-1));
        assert_eq!(e(&k).mul(&e(&k)).sign(&w), None);
        assert_eq!(e(&k).sub(&e(&k)).sign(&w), Some(0));
    }
}
