// Written for solidcorex. Apache-2.0.
//! `scx_surface::intersect_curve` against the pin's `SmApiIntersectCurveSurface`
//! (`intersect::api_intersectors::intersect_curve_surface`, `GlobalCurveIntersect` within
//! its fixed `0.001`) on the analytic pairs: a line segment, circle or ellipse against a
//! plane, cylinder, cone, sphere or torus (SU6, 63 §4.2).
//!
//! The pin's answer is points (and copies of the curve where it lies on the surface), not
//! parameters, so both sides are fingerprinted as whether there is an overlap and the
//! points, sorted by `x`; the pin's "no intersection" error is the empty answer. The pin's
//! surfaces are forward and bounded, and scx's extents are the same:
//! - plane: `Plane::new` over `[-10, 10]^2` (uv the plane's coordinates);
//! - cylinder and cone: `Cone::new_cylinder_bounded` and `Cone::new_bounded` over a turn
//!   and heights `[0, 6]` (scx: `[0, 2 pi r]` or `[0, 1]` by `[0, 6]`);
//! - sphere and torus: `create_canonical`, the whole surface;
//! - the line: `Line::from_points` over `[0, 1]` (scx: through the same points, the same
//!   extent); the circle and ellipse a whole turn.
//!
//! Where the two disagree past the tolerance the pin is not trusted: scx's answer is held
//! to testkit T2's exact roots ([`crate::pin_defects::curve_surface`]).

use crate::harness::{Gen, SplitMix64, Tol, run_defect_tol};

/// The pin's tolerance, which scx is called with.
pub const TOL: f64 = 0.001;
/// The bounded cylinders' and cones' height, and the plane's half side.
pub const HEIGHT: f64 = 6.0;
pub const PLANE: f64 = 10.0;

/// Points agree to `1e-9`: both sides refine a crossing to its rounding, and a tangent's
/// point is wherever within `TOL` each side put it, which the defect's truth judges.
pub const AGREE: Tol = Tol {
    rel: 1e-9,
    abs: 1e-9,
};

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Kind {
    Plane,
    Cylinder {
        r: f64,
    },
    /// Radius at `v = 0`, half-angle in radians (either sign; the top radius stays above
    /// zero).
    Cone {
        r: f64,
        a: f64,
    },
    Sphere {
        r: f64,
    },
    Torus {
        big: f64,
        small: f64,
    },
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Shape {
    /// From `p` to `q`.
    Segment { p: [f64; 3], q: [f64; 3] },
    /// Centre, `x`, `y` (orthonormal) and the semi-axes (equal for a circle).
    Conic {
        c: [f64; 3],
        x: [f64; 3],
        y: [f64; 3],
        a: f64,
        b: f64,
        circle: bool,
    },
}

/// One input: a surface in a frame (origin, z, x) and a curve.
#[derive(Clone, Debug)]
pub struct Case {
    pub kind: Kind,
    pub frame: [[f64; 3]; 3],
    pub curve: Shape,
}

/// The fingerprint: an overlap, and the points sorted by `x`.
pub type Answer = (bool, Vec<[f64; 3]>);

fn sorted(overlap: bool, mut pts: Vec<[f64; 3]>) -> Answer {
    pts.sort_by(|a, b| a[0].total_cmp(&b[0]).then(a[1].total_cmp(&b[1])));
    (overlap, pts)
}

fn cross(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

fn unit(rng: &mut SplitMix64) -> [f64; 3] {
    loop {
        let v = [
            rng.uniform(-1.0, 1.0),
            rng.uniform(-1.0, 1.0),
            rng.uniform(-1.0, 1.0),
        ];
        let l = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
        if l > 0.2 && l <= 1.0 {
            return v.map(|x| x / l);
        }
    }
}

fn at(o: [f64; 3], d: [f64; 3], s: f64) -> [f64; 3] {
    [o[0] + d[0] * s, o[1] + d[1] * s, o[2] + d[2] * s]
}

impl Case {
    /// The frame as the kinds take it.
    pub fn frame(&self) -> scx_num::frame::Frame {
        use scx_num::vector3d::{Point3, Vec3};
        let [o, z, x] = self.frame;
        scx_num::frame::Frame::new(Point3::from(o), Vec3::from(z), Vec3::from(x)).unwrap()
    }

    /// A point about the surface: the frame's origin, out to the kind's size.
    fn about(&self, rng: &mut SplitMix64) -> [f64; 3] {
        let reach = match self.kind {
            Kind::Plane => 8.0,
            Kind::Cylinder { r } | Kind::Sphere { r } => r * 1.3,
            Kind::Cone { r, .. } => r + 2.0,
            Kind::Torus { big, small } => big + small,
        };
        let f = self.frame();
        let up = match self.kind {
            Kind::Cylinder { .. } | Kind::Cone { .. } => rng.uniform(0.5, HEIGHT - 0.5),
            _ => 0.0,
        };
        let o = f.origin() + *f.z() * up;
        at(o.to_array(), unit(rng), rng.uniform(0.0, reach))
    }
}

impl Gen for Case {
    /// 30 surfaces of each kind, each with three segments, a circle and an ellipse about it:
    /// 750 inputs.
    fn pool(rng: &mut SplitMix64) -> Vec<Self> {
        let mut out = Vec::new();
        for i in 0..150 {
            let o = [
                rng.uniform(-10.0, 10.0),
                rng.uniform(-10.0, 10.0),
                rng.uniform(-10.0, 10.0),
            ];
            let (z, x) = (unit(rng), unit(rng));
            if cross(z, x).iter().map(|c| c * c).sum::<f64>() < 0.05 {
                continue;
            }
            let kind = match i % 5 {
                0 => Kind::Plane,
                1 => Kind::Cylinder {
                    r: rng.uniform(0.5, 10.0),
                },
                2 => {
                    let r = rng.uniform(0.5, 8.0);
                    let lim = scx_num::fmath::atan(r / HEIGHT).min(1.2);
                    let a = if rng.below(2) == 0 {
                        rng.uniform(0.1, 1.2)
                    } else {
                        -rng.uniform(0.05, 0.9 * lim)
                    };
                    Kind::Cone { r, a }
                }
                3 => Kind::Sphere {
                    r: rng.uniform(0.5, 10.0),
                },
                _ => {
                    let big = rng.uniform(2.0, 10.0);
                    Kind::Torus {
                        big,
                        small: big * rng.uniform(0.2, 0.8),
                    }
                }
            };
            let base = Case {
                kind,
                frame: [o, z, x],
                curve: Shape::Segment { p: o, q: o },
            };
            for j in 0..5 {
                let p = base.about(rng);
                let curve = if j < 3 {
                    let d = unit(rng);
                    let h = rng.uniform(5.0, 25.0);
                    Shape::Segment {
                        p: at(p, d, -h),
                        q: at(p, d, h),
                    }
                } else {
                    let x = unit(rng);
                    let n = cross(x, unit(rng));
                    let l = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt();
                    if l < 0.2 {
                        continue;
                    }
                    let y = cross(n.map(|c| c / l), x);
                    let a = rng.uniform(0.5, 12.0);
                    let circle = j == 3;
                    let b = if circle {
                        a
                    } else {
                        a * rng.uniform(0.2, 0.95)
                    };
                    Shape::Conic {
                        c: p,
                        x,
                        y,
                        a,
                        b,
                        circle,
                    }
                };
                out.push(Case {
                    curve,
                    ..base.clone()
                });
            }
        }
        out
    }
}

/// scx's side.
pub mod x {
    use super::{Answer, Case, HEIGHT, Kind, PLANE, Shape, TOL, sorted};
    use core::f64::consts::{PI, TAU};
    use scx_curve::{Circle, Curve, Ellipse, Line};
    use scx_num::budget::{Budget, Stop};
    use scx_num::extent1d::Extent1d;
    use scx_num::extent2d::Extent2d;
    use scx_num::frame::{Frame, Sense};
    use scx_num::units::Length;
    use scx_num::vector3d::{Point3, Vec3};
    use scx_surface::{Cone, CurveSurfaceHit, Cylinder, Plane, Sphere, Surface, Torus};

    /// The surface and its extent.
    pub fn surface(c: &Case) -> (Surface, Extent2d) {
        let f = c.frame();
        let (len, fw) = (Length::from_mm, Sense::Forward);
        let b = |u0, v0, u1, v1| Extent2d::from_bounds(u0, v0, u1, v1).unwrap();
        match c.kind {
            Kind::Plane => (Plane::new(f).into(), b(-PLANE, -PLANE, PLANE, PLANE)),
            Kind::Cylinder { r } => (
                Cylinder::new(f, len(r), fw).unwrap().into(),
                b(0.0, 0.0, 2.0 * PI * r, HEIGHT),
            ),
            Kind::Cone { r, a } => (
                Cone::new(f, len(r), a, fw).unwrap().into(),
                b(0.0, 0.0, 1.0, HEIGHT),
            ),
            Kind::Sphere { r } => (
                Sphere::new(f, len(r), fw).unwrap().into(),
                b(0.0, 0.0, 1.0, 1.0),
            ),
            Kind::Torus { big, small } => (
                Torus::new(f, len(big), len(small), fw).unwrap().into(),
                b(0.0, 0.0, 1.0, 1.0),
            ),
        }
    }

    /// The curve and its extent.
    pub fn curve(c: &Case) -> (Curve, Extent1d) {
        match c.curve {
            Shape::Segment { p, q } => (
                Line::through(Point3::from(p), Point3::from(q))
                    .unwrap()
                    .into(),
                Extent1d::new(0.0, 1.0).unwrap(),
            ),
            Shape::Conic {
                c: o,
                x,
                y,
                a,
                b,
                circle,
            } => {
                let (x, y) = (Vec3::from(x), Vec3::from(y));
                let f = Frame::new(Point3::from(o), x.cross(y), x).unwrap();
                let turn = Extent1d::new(0.0, TAU).unwrap();
                let len = Length::from_mm;
                let k: Curve = if circle {
                    Circle::new(f, len(a), Sense::Forward).unwrap().into()
                } else {
                    Ellipse::new(f, len(a), len(b), Sense::Forward)
                        .unwrap()
                        .into()
                };
                (k, turn)
            }
        }
    }

    pub fn hits(c: &Case) -> Vec<CurveSurfaceHit> {
        let (s, se) = surface(c);
        let (k, ce) = curve(c);
        scx_surface::intersect_curve(
            &k,
            &ce,
            &s,
            &se,
            scx_num::units::Resolution::new(Length::from_mm(TOL)).unwrap(),
            &mut Budget::unlimited(),
            &Stop::new(),
        )
        .unwrap()
    }

    pub fn answer(c: &Case) -> Answer {
        let h = hits(c);
        let overlap = h
            .iter()
            .any(|h| matches!(h, CurveSurfaceHit::Overlap { .. }));
        let pts = h
            .iter()
            .filter_map(|h| match h {
                CurveSurfaceHit::Point { point, .. } => Some(point.to_array()),
                CurveSurfaceHit::Overlap { .. } => None,
            })
            .collect();
        sorted(overlap, pts)
    }
}

/// The pin's side.
pub mod s {
    use super::{Answer, Case, HEIGHT, Kind, PLANE, Shape, sorted};
    use solidcore::curve::curve::Curve as PinCurve;
    use solidcore::curve::ellipse::{Ellipse as PinEllipse, EllipseOptions};
    use solidcore::curve::line::Line as PinLine;
    use solidcore::geom::axis2_placement::Axis2Placement;
    use solidcore::geom::extent1d::Extent1d;
    use solidcore::geom::extent2d::Extent2d;
    use solidcore::intersect::api_intersectors::intersect_curve_surface;
    use solidcore::surface::cone::p1::Cone;
    use solidcore::surface::plane::Plane;
    use solidcore::surface::sphere::Sphere;
    use solidcore::surface::surface::p2::Surface;
    use solidcore::surface::torus::{Torus, TorusOptions};
    use solidcore::{Kernel, Point3, Vec2, Vec3};

    fn p3(a: [f64; 3]) -> Point3 {
        Point3::new(a[0], a[1], a[2])
    }

    fn v3(a: [f64; 3]) -> Vec3 {
        Vec3::new(a[0], a[1], a[2])
    }

    fn surface(c: &Case, k: &Kernel) -> Surface {
        let f = c.frame();
        let (o, x, y) = (
            p3(f.origin().to_array()),
            v3(f.x().as_vec().to_array()),
            v3(f.y().as_vec().to_array()),
        );
        let place = Axis2Placement::new(o, x, y);
        match c.kind {
            Kind::Plane => {
                let ext = Extent2d::from_bounds(-PLANE, -PLANE, PLANE, PLANE);
                let mut p = Plane::new(o, x, y, Vec2::new(1.0, 1.0), &ext);
                p.make_nurb().unwrap();
                Surface::from(p)
            }
            Kind::Cylinder { r } => {
                let mut s =
                    Cone::new_cylinder_bounded(o, x, y, r, 0.0, 360.0, HEIGHT, false, false, true);
                s.make_nurb(k).unwrap();
                Surface::from(s)
            }
            Kind::Cone { r, a } => {
                let top = r + scx_num::fmath::tan(a) * HEIGHT;
                let mut s =
                    Cone::new_bounded(o, x, y, r, top, 0.0, 360.0, HEIGHT, false, false, true);
                s.make_nurb(k).unwrap();
                Surface::from(s)
            }
            Kind::Sphere { r } => Surface::from(Sphere::create_canonical(k, &place, r).unwrap()),
            Kind::Torus { big, small } => {
                let opts = TorusOptions {
                    swap_uv: false,
                    inside_out: false,
                    circle_nurb: None,
                };
                Surface::from(Torus::create_canonical(k, &place, big, small, opts, None).unwrap())
            }
        }
    }

    fn curve(c: &Case, k: &Kernel) -> PinCurve {
        match c.curve {
            Shape::Segment { p, q } => PinCurve::from(PinLine::from_points(p3(p), p3(q), 3)),
            Shape::Conic {
                c: o,
                x,
                y,
                a,
                b,
                circle,
            } => {
                if circle {
                    let place = Axis2Placement::new(p3(o), v3(x), v3(y));
                    PinCurve::from(
                        PinEllipse::create_canonical_circle(k, &place, a, None, Some(false))
                            .unwrap(),
                    )
                } else {
                    let opts = EllipseOptions {
                        dim: 3,
                        nurb: None,
                        inside_out: false,
                    };
                    let e = PinEllipse::new(
                        k,
                        p3(o),
                        v3(x),
                        v3(y),
                        Extent1d::from_min_max(0.0, 360.0),
                        a,
                        b,
                        &opts,
                    )
                    .unwrap();
                    PinCurve::from(e)
                }
            }
        }
    }

    pub fn answer(c: &Case) -> Answer {
        let k = Kernel::default();
        let (s, cv) = (surface(c, &k), curve(c, &k));
        match intersect_curve_surface(&k, &cv, &s) {
            Ok(h) => sorted(
                !h.curves.is_empty(),
                h.points.iter().map(|p| [p.x, p.y, p.z]).collect(),
            ),
            Err(_) => sorted(false, Vec::new()),
        }
    }
}

#[test]
fn curve_surface_intersect() {
    run_defect_tol(
        "curve_surface_intersect",
        AGREE,
        &crate::pin_defects::curve_surface(),
        |(c,): (Case,)| x::answer(&c),
        |(c,): (Case,)| s::answer(&c),
    );
}
