// Written for solidcorex. Apache-2.0.
//! `scx_surface::pcurve_of` of an oblique cylinder section (an ellipse on a cylinder)
//! against the pin's `SmApiDropCurveToSrf` (`primitive::api_surface_ops::drop_curve_to_srf`),
//! whose exact rational uv curve (`surface_drop_lib::cylinder_section`, WR-680) is in the
//! curve's own parameter, as scx's `AngleWave2` is (63 §4.14).
//!
//! The two parameters differ: scx's ellipse runs on its angle, the pin's on its NURB's
//! parameter. Both sides are therefore fingerprinted at the same points of the ellipse,
//! the angles `t_k` evenly over the arc: scx evaluates its pcurve at `t_k`; the pin finds
//! the NURB parameter whose point has the angle `t_k` (bisection on its own curve) and
//! evaluates its uv curve there. Both uv are the same: the pin's bounded cylinder's `u`
//! runs `[0, 2 pi r]`, as scx's does (D13).
//!
//! Where the pin's answer is not the section's (it drops at a 3-D tolerance of 10, so it
//! takes a gently tilted section for an iso line), scx's is held to the exact uv
//! ([`crate::pin_defects::oblique_pcurve`]).
//!
//! The arcs keep off the cylinder's seam (the pin's construction refuses one across it)
//! and inside its heights `[0, 6]`.

use crate::harness::{Gen, SplitMix64, Tol, run_defect_tol};

/// The cylinders' height.
pub const HEIGHT: f64 = 6.0;
/// The stations along each arc.
pub const STATIONS: usize = 16;

/// The uv agree to `1e-9`: both are exact closed forms, rounded.
pub const AGREE: Tol = Tol {
    rel: 1e-9,
    abs: 1e-9,
};

/// One input: a cylinder (frame origin, z, x and radius) and an arc of a plane section.
#[derive(Clone, Debug)]
pub struct Case {
    pub frame: [[f64; 3]; 3],
    pub r: f64,
    /// The ellipse's centre, `x` and `y` (orthonormal), semi-axes and angles.
    pub c: [f64; 3],
    pub x: [f64; 3],
    pub y: [f64; 3],
    pub a: f64,
    pub b: f64,
    pub t: [f64; 2],
}

fn cross(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

fn dot(a: [f64; 3], b: [f64; 3]) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

fn comb(p: [f64; 3], s: f64, q: [f64; 3]) -> [f64; 3] {
    [p[0] + s * q[0], p[1] + s * q[1], p[2] + s * q[2]]
}

fn unit(rng: &mut SplitMix64) -> [f64; 3] {
    loop {
        let v = [
            rng.uniform(-1.0, 1.0),
            rng.uniform(-1.0, 1.0),
            rng.uniform(-1.0, 1.0),
        ];
        let l = dot(v, v).sqrt();
        if l > 0.2 && l <= 1.0 {
            return v.map(|x| x / l);
        }
    }
}

impl Case {
    fn frame(&self) -> scx_num::frame::Frame {
        use scx_num::vector3d::{Point3, Vec3};
        let [o, z, x] = self.frame;
        scx_num::frame::Frame::new(Point3::from(o), Vec3::from(z), Vec3::from(x)).unwrap()
    }

    /// The stations' angles.
    fn stations(&self) -> impl Iterator<Item = f64> + '_ {
        let [t0, t1] = self.t;
        (0..=STATIONS).map(move |k| t0 + (t1 - t0) * (k as f64 / STATIONS as f64))
    }
}

impl Gen for Case {
    /// 120 cylinders, each with two arcs of one section: 240 inputs.
    fn pool(rng: &mut SplitMix64) -> Vec<Self> {
        let mut out = Vec::new();
        while out.len() < 240 {
            let o = [
                rng.uniform(-10.0, 10.0),
                rng.uniform(-10.0, 10.0),
                rng.uniform(-10.0, 10.0),
            ];
            let (z, x) = (unit(rng), unit(rng));
            let yl = cross(z, x);
            if dot(yl, yl) < 0.05 {
                continue;
            }
            let base = Case {
                frame: [o, z, x],
                r: rng.uniform(0.5, 10.0),
                c: o,
                x,
                y: x,
                a: 1.0,
                b: 1.0,
                t: [0.0, 1.0],
            };
            let f = base.frame();
            let (fz, fx, fy) = (
                f.z().as_vec().to_array(),
                f.x().as_vec().to_array(),
                f.y().as_vec().to_array(),
            );
            let r = base.r;
            // The plane's tilt off square, kept within the heights.
            let tilt = rng.uniform(0.05, scx_num::fmath::atan(1.8 / r).min(1.2));
            let turn = rng.uniform(0.0, core::f64::consts::TAU);
            let (ct, st) = (scx_num::fmath::cos(turn), scx_num::fmath::sin(turn));
            // Across the axis (horizontal) and up the plane.
            let across = comb([0.0; 3], ct, fx);
            let across = comb(across, st, fy);
            let side = cross(fz, across);
            let up = comb(
                side.map(|c| c * scx_num::fmath::cos(tilt)),
                scx_num::fmath::sin(tilt),
                fz,
            );
            let c = comb(f.origin().to_array(), rng.uniform(2.0, 4.0), fz);
            let (ra, rb) = (r, r / scx_num::fmath::cos(tilt));
            // Either axis first, either sense.
            let flip = if rng.below(2) == 0 { 1.0 } else { -1.0 };
            let (ex, ey, a, b) = if rng.below(2) == 0 {
                (across, up.map(|c| c * flip), ra, rb)
            } else {
                (up, across.map(|c| c * flip), rb, ra)
            };
            // The angle where the section crosses the seam: its offset across the axis,
            // `a cos t ex + b sin t ey`, along `fx`.
            let (ay, by) = (a * dot(ex, fy), b * dot(ey, fy));
            let mut seam = scx_num::fmath::atan2(-ay, by);
            let along = a * scx_num::fmath::cos(seam) * dot(ex, fx)
                + b * scx_num::fmath::sin(seam) * dot(ey, fx);
            if along < 0.0 {
                seam += core::f64::consts::PI;
            }
            let seam = seam.rem_euclid(core::f64::consts::TAU);
            for _ in 0..2 {
                let room = core::f64::consts::TAU - 0.1;
                let t0 = seam + 0.05 + rng.uniform(0.0, room - 0.5);
                let t1 = t0 + rng.uniform(0.5, seam + 0.05 + room - t0);
                out.push(Case {
                    c,
                    x: ex,
                    y: ey,
                    a,
                    b,
                    t: [t0, t1],
                    ..base.clone()
                });
            }
        }
        out
    }
}

/// scx's side.
pub mod x {
    use super::{Case, HEIGHT};
    use scx_curve::{Curve, Curve2, Ellipse};
    use scx_num::extent1d::Extent1d;
    use scx_num::frame::{Frame, Sense};
    use scx_num::units::Length;
    use scx_num::vector3d::{Point3, Vec3};
    use scx_surface::{Cylinder, Surface, pcurve_of};

    /// The pcurve at the stations; `None` when it is not an
    /// `AngleWave2`.
    pub fn answer(c: &Case) -> Option<Vec<[f64; 2]>> {
        let s: Surface = Cylinder::new(c.frame(), Length::from_mm(c.r), Sense::Forward)
            .unwrap()
            .into();
        let (x, y) = (Vec3::from(c.x), Vec3::from(c.y));
        let f = Frame::new(Point3::from(c.c), x.cross(y), x).unwrap();
        let k: Curve = Ellipse::new(
            f,
            Length::from_mm(c.a),
            Length::from_mm(c.b),
            Sense::Forward,
        )
        .unwrap()
        .into();
        let over = Extent1d::new(c.t[0], c.t[1]).unwrap();
        let size = 10.0 + c.a.max(c.b) + HEIGHT;
        let pc = pcurve_of(&k, &s, over, Length::from_mm(1e-9 * size)).ok()?;
        if !matches!(pc, Curve2::AngleWave(_)) {
            return None;
        }
        Some(
            c.stations()
                .map(|t| {
                    let p = pc.point(t);
                    [p.x, p.y]
                })
                .collect(),
        )
    }
}

/// The pin's side.
pub mod s {
    use super::{Case, HEIGHT, dot};
    use solidcore::curve::curve::Curve as PinCurve;
    use solidcore::curve::ellipse::{Ellipse as PinEllipse, EllipseOptions};
    use solidcore::primitive::api_surface_ops::drop_curve_to_srf;
    use solidcore::surface::cone::p1::Cone;
    use solidcore::surface::surface::p2::Surface;
    use solidcore::{Kernel, Point3, Vec3};

    fn p3(a: [f64; 3]) -> Point3 {
        Point3::new(a[0], a[1], a[2])
    }

    fn v3(a: [f64; 3]) -> Vec3 {
        Vec3::new(a[0], a[1], a[2])
    }

    /// The ellipse's angle of the point at the NURB parameter `s`, in `[t0, t0 + 2 pi)`
    /// less a little.
    fn angle(c: &Case, k: &Kernel, e: &PinCurve, s: f64) -> f64 {
        let p = e.evaluate_point(k, s).unwrap();
        let d = [p.x - c.c[0], p.y - c.c[1], p.z - c.c[2]];
        let t = scx_num::fmath::atan2(dot(d, c.y) / c.b, dot(d, c.x) / c.a);
        let tau = core::f64::consts::TAU;
        t + ((c.t[0] - 1e-9 - t) / tau).ceil() * tau
    }

    pub fn answer(c: &Case) -> Option<Vec<[f64; 2]>> {
        let k = Kernel::default();
        let f = c.frame();
        let (o, x, y) = (
            p3(f.origin().to_array()),
            v3(f.x().as_vec().to_array()),
            v3(f.y().as_vec().to_array()),
        );
        let mut cyl =
            Cone::new_cylinder_bounded(o, x, y, c.r, 0.0, 360.0, HEIGHT, false, false, true);
        cyl.make_nurb(&k).unwrap();
        let surface = Surface::from(cyl);
        let opts = EllipseOptions {
            dim: 3,
            nurb: None,
            inside_out: false,
        };
        let deg = 180.0 / core::f64::consts::PI;
        let e = PinEllipse::new(
            &k,
            p3(c.c),
            v3(c.x),
            v3(c.y),
            solidcore::geom::extent1d::Extent1d::from_min_max(c.t[0] * deg, c.t[1] * deg),
            c.a,
            c.b,
            &opts,
        )
        .ok()?;
        let e = PinCurve::from(e);
        let dropped = drop_curve_to_srf(&k, &e, &surface, false).ok()?;
        let pieces = &dropped.uv_curves;
        let ivl = e.natural_interval();
        Some(
            c.stations()
                .map(|t| {
                    let (mut lo, mut hi) = (ivl.min(), ivl.max());
                    for _ in 0..200 {
                        let mid = 0.5 * (lo + hi);
                        if angle(c, &k, &e, mid) < t {
                            lo = mid;
                        } else {
                            hi = mid;
                        }
                    }
                    let s = 0.5 * (lo + hi);
                    let Some(uv) = pieces.iter().find(|p| {
                        let i = p.natural_interval();
                        i.min() <= s && s <= i.max()
                    }) else {
                        return [f64::NAN; 2];
                    };
                    let p = uv.evaluate_point(&k, s).unwrap();
                    [p.x, p.y]
                })
                .collect(),
        )
    }
}

#[test]
fn oblique_cylinder_pcurve() {
    run_defect_tol(
        "oblique_cylinder_pcurve",
        AGREE,
        &crate::pin_defects::oblique_pcurve(),
        |(c,): (Case,)| x::answer(&c),
        |(c,): (Case,)| s::answer(&c),
    );
}
