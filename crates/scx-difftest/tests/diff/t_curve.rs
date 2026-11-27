// Written for solidcorex. Apache-2.0.
//! `scx-curve` against the pin's `curve::line::Line` and `curve::ellipse::Ellipse`: each
//! kind's construction, point and jet, in 3-D and (the pin's dimension-2 curves at z = 0)
//! 2-D.
//!
//! The pin's side:
//! - a line `Line::new(o, d)` is `Line::infinite(o, d, ..)`, `Line::through(a, b)` is
//!   `Line::from_points(a, b, ..)`, read back through `canonical` (`P`, `scale V`);
//! - a circle is `Ellipse::create_canonical_circle` and an ellipse `Ellipse::new` over
//!   `[0, 360]`, both on scx's frame axes (`y` negated for `Sense::Reversed`, the pin's
//!   mirror), read back through `canonical`;
//! - a point or jet is `evaluate_step` (three derivatives, the pin's most) at the angle in
//!   degrees, `t 180 / pi`, its k-th derivative times `(180 / pi)^k` back to radians.
//!
//! Where the two disagree past the tolerance, the pin defects (`pin_defects.rs`, `curve_*`)
//! judge scx against the definition (construction) or the double-double truth
//! (`scx_testkit::analytic`; conic points and jets). A line's point and jet stay within the
//! tolerance of the pin's and need none; scx-curve's own tests hold them to the truth.

use crate::diff;
use crate::harness::{Gen, SplitMix64};
use crate::pin_defects;

use scx_curve::{Circle, Circle2, Ellipse, Ellipse2, Line, Line2};
use scx_num::frame::{Frame, Frame2, Sense};
use scx_num::math::{EFF_ZERO, HALF_PI, PI};
use scx_num::units::Length;
use scx_num::vector2d::{Point2, Vec2};
use scx_num::vector3d::{Point3, Vec3};
use scx_nurbs::Approach;

/// Degrees per radian, as the pin's angles go.
pub const DEG: f64 = 180.0 / PI;

/// Line data: an origin (or a start) and a direction (or an end).
#[derive(Clone, Debug)]
pub struct LineIn {
    pub o: [f64; 3],
    pub d: [f64; 3],
}

/// A line that builds on both sides (finite, direction well above `EFF_ZERO`).
#[derive(Clone, Debug)]
pub struct GoodLine(pub LineIn);

/// A line parameter.
#[derive(Clone, Debug)]
pub struct LineT(pub f64);

/// Conic data: the frame's origin, z and x hint (a frame scx builds), the radii and the
/// sense. In 2-D the hint's x and y are the frame's x, and the origin's z is dropped.
#[derive(Clone, Debug)]
pub struct ConicIn {
    pub o: [f64; 3],
    pub z: [f64; 3],
    pub xh: [f64; 3],
    pub a: f64,
    pub b: f64,
    pub rev: bool,
}

/// A conic that builds on both sides.
#[derive(Clone, Debug)]
pub struct GoodConic(pub ConicIn);

/// An angle in radians.
#[derive(Clone, Debug)]
pub struct Angle(pub f64);

fn signed(rng: &mut SplitMix64, s: f64) -> [f64; 3] {
    core::array::from_fn(|_| rng.uniform(-s, s))
}

fn good_lines(rng: &mut SplitMix64) -> Vec<LineIn> {
    let mut v = Vec::new();
    for s in [1.0, 1e-3, 1e4] {
        for _ in 0..8 {
            v.push(LineIn {
                o: signed(rng, s),
                d: signed(rng, s),
            });
        }
    }
    v.push(LineIn {
        o: [0.0; 3],
        d: [1.0, 0.0, 0.0],
    });
    v.push(LineIn {
        o: [1e8, -1e8, 3.0],
        d: [1e-6, 0.0, -2e-6],
    });
    v
}

impl Gen for LineIn {
    fn pool(rng: &mut SplitMix64) -> Vec<Self> {
        let mut v = good_lines(rng);
        let o = [1.0, 2.0, 3.0];
        for d in [
            [0.0; 3],
            [0.0, 9e-13, 0.0],
            [0.0, 1e-12, 0.0],
            [1e-13, 1e-13, 1e-13],
            [1e300, 1e300, 0.0],
            [f64::INFINITY, 0.0, 0.0],
            [f64::NAN, 1.0, 0.0],
            [-f64::MAX, 0.0, 0.0],
        ] {
            v.push(LineIn { o, d });
        }
        v.push(LineIn {
            o: [f64::NAN, 0.0, 0.0],
            d: [1.0, 0.0, 0.0],
        });
        v.push(LineIn {
            o: [f64::MAX, 0.0, 0.0],
            d: [-f64::MAX, 0.0, 0.0],
        });
        // An end equal to the start, and one an ulp away.
        v.push(LineIn { o, d: o });
        v.push(LineIn {
            o,
            d: [1.0, 2.0, 3.0_f64.next_up()],
        });
        v
    }
}

impl Gen for GoodLine {
    fn pool(rng: &mut SplitMix64) -> Vec<Self> {
        good_lines(rng).into_iter().map(GoodLine).collect()
    }
}

impl Gen for LineT {
    fn pool(rng: &mut SplitMix64) -> Vec<Self> {
        let mut v = vec![
            0.0,
            -0.0,
            1.0,
            -1.0,
            0.5,
            1e-300,
            1e6,
            -3.75e5,
            f64::NAN,
            f64::INFINITY,
        ];
        v.extend((0..10).map(|_| rng.uniform(-10.0, 10.0)));
        v.into_iter().map(LineT).collect()
    }
}

fn good_conics(rng: &mut SplitMix64) -> Vec<ConicIn> {
    let mut v = Vec::new();
    for (s, r) in [(1.0, 1.0), (1e3, 1e-3), (1e-3, 1e3)] {
        for i in 0..6 {
            let a = r * rng.uniform(0.25, 1.25);
            let b = if i % 3 == 0 {
                a
            } else {
                r * rng.uniform(0.25, 1.25)
            };
            v.push(ConicIn {
                o: signed(rng, s),
                z: signed(rng, 1.0),
                xh: signed(rng, 1.0),
                a,
                b,
                rev: i % 2 == 1,
            });
        }
    }
    v.push(ConicIn {
        o: [0.0; 3],
        z: [0.0, 0.0, 1.0],
        xh: [1.0, 0.0, 0.0],
        a: 1.0,
        b: 1.0,
        rev: false,
    });
    v.retain(|c| frame3(c).is_some() && frame2(c).is_some());
    v
}

impl Gen for ConicIn {
    fn pool(rng: &mut SplitMix64) -> Vec<Self> {
        let mut v = good_conics(rng);
        let base = ConicIn {
            o: [1.0, -2.0, 0.5],
            z: [0.0, 0.0, 1.0],
            xh: [1.0, 1.0, 0.0],
            a: 1.0,
            b: 2.0,
            rev: false,
        };
        for r in [
            0.0,
            -0.0,
            -1.0,
            EFF_ZERO,
            EFF_ZERO.next_up(),
            1e-9,
            1e9,
            f64::NAN,
            f64::INFINITY,
            -f64::INFINITY,
        ] {
            v.push(ConicIn {
                a: r,
                ..base.clone()
            });
            v.push(ConicIn {
                b: r,
                rev: true,
                ..base.clone()
            });
        }
        v
    }
}

impl Gen for GoodConic {
    fn pool(rng: &mut SplitMix64) -> Vec<Self> {
        good_conics(rng).into_iter().map(GoodConic).collect()
    }
}

impl Gen for Angle {
    fn pool(rng: &mut SplitMix64) -> Vec<Self> {
        let mut v = vec![
            0.0,
            -0.0,
            HALF_PI,
            PI,
            -PI,
            2.0 * PI,
            HALF_PI.next_up(),
            PI.next_down(),
            1e3,
            -1e5,
            1e-300,
            f64::NAN,
            f64::INFINITY,
        ];
        v.extend((0..10).map(|_| rng.uniform(-4.0 * PI, 4.0 * PI)));
        v.into_iter().map(Angle).collect()
    }
}

// ---------------------------------------------------------------- shared readings

pub fn sense(c: &ConicIn) -> Sense {
    if c.rev {
        Sense::Reversed
    } else {
        Sense::Forward
    }
}

pub fn frame3(c: &ConicIn) -> Option<Frame> {
    Frame::new(Point3::from(c.o), Vec3::from(c.z), Vec3::from(c.xh))
}

pub fn frame2(c: &ConicIn) -> Option<Frame2> {
    Frame2::new(Point2::new(c.o[0], c.o[1]), Vec2::new(c.xh[0], c.xh[1]))
}

/// scx's frame as the curve runs: origin, x and `y` times the sense's sign.
pub fn axes3(c: &ConicIn) -> ([f64; 3], [f64; 3], [f64; 3]) {
    let f = frame3(c).unwrap();
    let s = sense(c).sign();
    (
        f.origin().to_array(),
        f.x().as_vec().to_array(),
        (f.y().as_vec() * s).to_array(),
    )
}

/// The 2-D frame embedded at z = 0, likewise.
pub fn axes2(c: &ConicIn) -> ([f64; 3], [f64; 3], [f64; 3]) {
    let f = frame2(c).unwrap();
    let s = sense(c).sign();
    let (x, y) = (f.x().as_vec(), f.y().as_vec() * s);
    (
        [f.origin().x, f.origin().y, 0.0],
        [x.x, x.y, 0.0],
        [y.x, y.y, 0.0],
    )
}

fn flat(parts: &[[f64; 3]]) -> Vec<f64> {
    parts.iter().flatten().copied().collect()
}

fn xy(v: Vec<[f64; 3]>) -> Vec<[f64; 2]> {
    v.into_iter().map(|p| [p[0], p[1]]).collect()
}

/// scx's side.
pub mod x {
    use super::*;

    pub fn line_new(l: &LineIn) -> Option<Vec<f64>> {
        let l = Line::new(Point3::from(l.o), Vec3::from(l.d)).ok()?;
        Some(flat(&[l.origin().to_array(), l.direction().to_array()]))
    }

    pub fn line_through(l: &LineIn) -> Option<Vec<f64>> {
        let l = Line::through(Point3::from(l.o), Point3::from(l.d)).ok()?;
        Some(flat(&[l.origin().to_array(), l.direction().to_array()]))
    }

    pub fn line2_new(l: &LineIn) -> Option<Vec<f64>> {
        let l = Line2::new(Point2::new(l.o[0], l.o[1]), Vec2::new(l.d[0], l.d[1])).ok()?;
        let (o, d) = (l.origin(), l.direction());
        Some(vec![o.x, o.y, d.x, d.y])
    }

    fn line(l: &LineIn) -> Line {
        Line::new(Point3::from(l.o), Vec3::from(l.d)).unwrap()
    }

    fn line2(l: &LineIn) -> Line2 {
        Line2::new(Point2::new(l.o[0], l.o[1]), Vec2::new(l.d[0], l.d[1])).unwrap()
    }

    pub fn line_point(l: &LineIn, t: f64) -> [f64; 3] {
        line(l).point(t).to_array()
    }

    pub fn line_jet(l: &LineIn, t: f64) -> Vec<[f64; 3]> {
        let j = line(l).jet::<3>(t, Approach::Above);
        let mut v = vec![j.point.to_array()];
        v.extend(j.d.iter().map(|d| d.to_array()));
        v
    }

    pub fn line2_point(l: &LineIn, t: f64) -> [f64; 2] {
        line2(l).point(t).to_array()
    }

    pub fn line2_jet(l: &LineIn, t: f64) -> Vec<[f64; 2]> {
        let j = line2(l).jet::<3>(t, Approach::Below);
        let mut v = vec![j.point.to_array()];
        v.extend(j.d.iter().map(|d| d.to_array()));
        v
    }

    fn len(x: f64) -> Length {
        Length::from_mm(x)
    }

    /// The read-back of a built conic: origin, x, signed y, then the radii.
    fn read(c: &ConicIn, axes: ([f64; 3], [f64; 3], [f64; 3]), a: f64, b: f64) -> Vec<f64> {
        let _ = c;
        let mut v = flat(&[axes.0, axes.1, axes.2]);
        v.extend([a, b]);
        v
    }

    pub fn circle_new(c: &ConicIn) -> Option<Vec<f64>> {
        let e = Circle::new(frame3(c)?, len(c.a), sense(c)).ok()?;
        Some(read(c, axes3(c), e.radius().mm(), e.radius().mm()))
    }

    pub fn ellipse_new(c: &ConicIn) -> Option<Vec<f64>> {
        let e = Ellipse::new(frame3(c)?, len(c.a), len(c.b), sense(c)).ok()?;
        Some(read(c, axes3(c), e.semi_x().mm(), e.semi_y().mm()))
    }

    pub fn circle2_new(c: &ConicIn) -> Option<Vec<f64>> {
        let e = Circle2::new(frame2(c)?, len(c.a), sense(c)).ok()?;
        Some(read(c, axes2(c), e.radius().mm(), e.radius().mm()))
    }

    pub fn ellipse2_new(c: &ConicIn) -> Option<Vec<f64>> {
        let e = Ellipse2::new(frame2(c)?, len(c.a), len(c.b), sense(c)).ok()?;
        Some(read(c, axes2(c), e.semi_x().mm(), e.semi_y().mm()))
    }

    /// Which conic of the input: a circle (radius `a`) or the ellipse.
    #[derive(Clone, Copy)]
    pub enum Kind {
        Circle,
        Ellipse,
    }

    pub fn conic_jet(c: &ConicIn, kind: Kind, t: f64) -> Vec<[f64; 3]> {
        let f = frame3(c).unwrap();
        let j = match kind {
            Kind::Circle => Circle::new(f, len(c.a), sense(c))
                .unwrap()
                .jet::<3>(t, Approach::Above),
            Kind::Ellipse => Ellipse::new(f, len(c.a), len(c.b), sense(c))
                .unwrap()
                .jet::<3>(t, Approach::Above),
        };
        let mut v = vec![j.point.to_array()];
        v.extend(j.d.iter().map(|d| d.to_array()));
        v
    }

    pub fn conic_point(c: &ConicIn, kind: Kind, t: f64) -> [f64; 3] {
        let f = frame3(c).unwrap();
        match kind {
            Kind::Circle => Circle::new(f, len(c.a), sense(c)).unwrap().point(t),
            Kind::Ellipse => Ellipse::new(f, len(c.a), len(c.b), sense(c))
                .unwrap()
                .point(t),
        }
        .to_array()
    }

    pub fn conic2_jet(c: &ConicIn, kind: Kind, t: f64) -> Vec<[f64; 2]> {
        let f = frame2(c).unwrap();
        let j = match kind {
            Kind::Circle => Circle2::new(f, len(c.a), sense(c))
                .unwrap()
                .jet::<3>(t, Approach::Below),
            Kind::Ellipse => Ellipse2::new(f, len(c.a), len(c.b), sense(c))
                .unwrap()
                .jet::<3>(t, Approach::Below),
        };
        let mut v = vec![j.point.to_array()];
        v.extend(j.d.iter().map(|d| d.to_array()));
        v
    }

    pub fn conic2_point(c: &ConicIn, kind: Kind, t: f64) -> [f64; 2] {
        let f = frame2(c).unwrap();
        match kind {
            Kind::Circle => Circle2::new(f, len(c.a), sense(c)).unwrap().point(t),
            Kind::Ellipse => Ellipse2::new(f, len(c.a), len(c.b), sense(c))
                .unwrap()
                .point(t),
        }
        .to_array()
    }
}

/// The pin's side.
pub mod s {
    use super::x::Kind;
    use super::*;
    use solidcore::Kernel;
    use solidcore::curve::ellipse::{Ellipse as PinEllipse, EllipseOptions};
    use solidcore::curve::line::Line as PinLine;
    use solidcore::geom::axis2_placement::Axis2Placement;
    use solidcore::geom::extent1d::Extent1d;

    fn p3(a: [f64; 3]) -> solidcore::Point3 {
        solidcore::Point3::new(a[0], a[1], a[2])
    }

    fn v3(a: [f64; 3]) -> solidcore::Vec3 {
        solidcore::Vec3::new(a[0], a[1], a[2])
    }

    fn arr(v: solidcore::Vec3) -> [f64; 3] {
        [v.x, v.y, v.z]
    }

    fn read_line(l: &PinLine) -> Vec<f64> {
        let c = l.canonical();
        flat(&[arr(c.point), arr(c.vector)])
    }

    pub fn line_new(l: &LineIn) -> Option<Vec<f64>> {
        Some(read_line(&PinLine::infinite(p3(l.o), v3(l.d), 3, false)))
    }

    pub fn line_through(l: &LineIn) -> Option<Vec<f64>> {
        Some(read_line(&PinLine::from_points(p3(l.o), p3(l.d), 3)))
    }

    pub fn line2_new(l: &LineIn) -> Option<Vec<f64>> {
        let o = [l.o[0], l.o[1], 0.0];
        let d = [l.d[0], l.d[1], 0.0];
        let v = read_line(&PinLine::infinite(p3(o), v3(d), 2, false));
        Some(vec![v[0], v[1], v[3], v[4]])
    }

    pub fn line_jet(l: &LineIn, t: f64) -> Vec<[f64; 3]> {
        PinLine::infinite(p3(l.o), v3(l.d), 3, false)
            .evaluate_step(t, 3)
            .into_iter()
            .map(arr)
            .collect()
    }

    pub fn line_point(l: &LineIn, t: f64) -> [f64; 3] {
        line_jet(l, t)[0]
    }

    fn flat_line(l: &LineIn) -> LineIn {
        LineIn {
            o: [l.o[0], l.o[1], 0.0],
            d: [l.d[0], l.d[1], 0.0],
        }
    }

    pub fn line2_jet(l: &LineIn, t: f64) -> Vec<[f64; 2]> {
        let l = flat_line(l);
        xy(PinLine::infinite(p3(l.o), v3(l.d), 2, false)
            .evaluate_step(t, 3)
            .into_iter()
            .map(arr)
            .collect())
    }

    pub fn line2_point(l: &LineIn, t: f64) -> [f64; 2] {
        line2_jet(l, t)[0]
    }

    /// The pin's conic on the given axes, as `canonical` reads it back.
    fn build(
        axes: ([f64; 3], [f64; 3], [f64; 3]),
        kind: Kind,
        a: f64,
        b: f64,
        dim: u64,
    ) -> Option<PinEllipse> {
        let k = Kernel::default();
        let (o, x, y) = axes;
        match kind {
            Kind::Circle => {
                let place = Axis2Placement::new(p3(o), v3(x), v3(y));
                if dim == 3 {
                    PinEllipse::create_canonical_circle(&k, &place, a, None, Some(false)).ok()
                } else {
                    // `create_canonical_circle` is 3-D only: its radius rule, then the
                    // dimension-2 circle.
                    if a.is_nan() || a <= EFF_ZERO {
                        return None;
                    }
                    PinEllipse::new_circle(
                        &k,
                        p3(o),
                        v3(x),
                        v3(y),
                        Extent1d::from_min_max(0.0, 360.0),
                        a,
                        &EllipseOptions {
                            dim,
                            nurb: None,
                            inside_out: false,
                        },
                    )
                    .ok()
                }
            }
            Kind::Ellipse => PinEllipse::new(
                &k,
                p3(o),
                v3(x),
                v3(y),
                Extent1d::from_min_max(0.0, 360.0),
                a,
                b,
                &EllipseOptions {
                    dim,
                    nurb: None,
                    inside_out: false,
                },
            )
            .ok(),
        }
    }

    fn read(e: &PinEllipse) -> Vec<f64> {
        let c = e.canonical();
        let p = &c.position;
        let mut v = flat(&[arr(p.origin()), arr(p.x_axis()), arr(p.y_axis())]);
        v.extend([c.radius_x, c.radius_y]);
        v
    }

    pub fn circle_new(c: &ConicIn) -> Option<Vec<f64>> {
        build(axes3(c), Kind::Circle, c.a, c.a, 3).map(|e| read(&e))
    }

    pub fn ellipse_new(c: &ConicIn) -> Option<Vec<f64>> {
        build(axes3(c), Kind::Ellipse, c.a, c.b, 3).map(|e| read(&e))
    }

    pub fn circle2_new(c: &ConicIn) -> Option<Vec<f64>> {
        build(axes2(c), Kind::Circle, c.a, c.a, 2).map(|e| read(&e))
    }

    pub fn ellipse2_new(c: &ConicIn) -> Option<Vec<f64>> {
        build(axes2(c), Kind::Ellipse, c.a, c.b, 2).map(|e| read(&e))
    }

    fn step(e: &PinEllipse, t: f64) -> Vec<[f64; 3]> {
        let d = e.evaluate_step(t * DEG, 3).unwrap();
        let mut scale = 1.0;
        d.into_iter()
            .map(|v| {
                let r = arr(v * scale);
                scale *= DEG;
                r
            })
            .collect()
    }

    pub fn conic_jet(c: &ConicIn, kind: Kind, t: f64) -> Vec<[f64; 3]> {
        step(&build(axes3(c), kind, c.a, c.b, 3).unwrap(), t)
    }

    pub fn conic_point(c: &ConicIn, kind: Kind, t: f64) -> [f64; 3] {
        conic_jet(c, kind, t)[0]
    }

    pub fn conic2_jet(c: &ConicIn, kind: Kind, t: f64) -> Vec<[f64; 2]> {
        xy(step(&build(axes2(c), kind, c.a, c.b, 2).unwrap(), t))
    }

    pub fn conic2_point(c: &ConicIn, kind: Kind, t: f64) -> [f64; 2] {
        conic2_jet(c, kind, t)[0]
    }
}

use x::Kind;

diff!(
    curve_line_new,
    pin_defect = pin_defects::curve_line_new,
    |l: LineIn| x::line_new(&l),
    s::line_new(&l)
);
diff!(
    curve_line_through,
    pin_defect = pin_defects::curve_line_through,
    |l: LineIn| x::line_through(&l),
    s::line_through(&l)
);
diff!(
    curve_line2_new,
    pin_defect = pin_defects::curve_line2_new,
    |l: LineIn| x::line2_new(&l),
    s::line2_new(&l)
);
diff!(
    curve_line_point,
    |l: GoodLine, t: LineT| x::line_point(&l.0, t.0),
    s::line_point(&l.0, t.0)
);
diff!(
    curve_line_jet,
    |l: GoodLine, t: LineT| x::line_jet(&l.0, t.0),
    s::line_jet(&l.0, t.0)
);
diff!(
    curve_line2_point,
    |l: GoodLine, t: LineT| x::line2_point(&l.0, t.0),
    s::line2_point(&l.0, t.0)
);
diff!(
    curve_line2_jet,
    |l: GoodLine, t: LineT| x::line2_jet(&l.0, t.0),
    s::line2_jet(&l.0, t.0)
);

diff!(
    curve_circle_new,
    pin_defect = pin_defects::curve_circle_new,
    |c: ConicIn| x::circle_new(&c),
    s::circle_new(&c)
);
diff!(
    curve_ellipse_new,
    pin_defect = pin_defects::curve_ellipse_new,
    |c: ConicIn| x::ellipse_new(&c),
    s::ellipse_new(&c)
);
diff!(
    curve_circle2_new,
    pin_defect = pin_defects::curve_circle2_new,
    |c: ConicIn| x::circle2_new(&c),
    s::circle2_new(&c)
);
diff!(
    curve_ellipse2_new,
    pin_defect = pin_defects::curve_ellipse2_new,
    |c: ConicIn| x::ellipse2_new(&c),
    s::ellipse2_new(&c)
);

diff!(
    curve_circle_point,
    pin_defect = pin_defects::curve_circle_point,
    |c: GoodConic, t: Angle| x::conic_point(&c.0, Kind::Circle, t.0),
    s::conic_point(&c.0, Kind::Circle, t.0)
);
diff!(
    curve_circle_jet,
    pin_defect = pin_defects::curve_circle_jet,
    |c: GoodConic, t: Angle| x::conic_jet(&c.0, Kind::Circle, t.0),
    s::conic_jet(&c.0, Kind::Circle, t.0)
);
diff!(
    curve_ellipse_point,
    pin_defect = pin_defects::curve_ellipse_point,
    |c: GoodConic, t: Angle| x::conic_point(&c.0, Kind::Ellipse, t.0),
    s::conic_point(&c.0, Kind::Ellipse, t.0)
);
diff!(
    curve_ellipse_jet,
    pin_defect = pin_defects::curve_ellipse_jet,
    |c: GoodConic, t: Angle| x::conic_jet(&c.0, Kind::Ellipse, t.0),
    s::conic_jet(&c.0, Kind::Ellipse, t.0)
);
diff!(
    curve_circle2_point,
    pin_defect = pin_defects::curve_circle2_point,
    |c: GoodConic, t: Angle| x::conic2_point(&c.0, Kind::Circle, t.0),
    s::conic2_point(&c.0, Kind::Circle, t.0)
);
diff!(
    curve_circle2_jet,
    pin_defect = pin_defects::curve_circle2_jet,
    |c: GoodConic, t: Angle| x::conic2_jet(&c.0, Kind::Circle, t.0),
    s::conic2_jet(&c.0, Kind::Circle, t.0)
);
diff!(
    curve_ellipse2_point,
    pin_defect = pin_defects::curve_ellipse2_point,
    |c: GoodConic, t: Angle| x::conic2_point(&c.0, Kind::Ellipse, t.0),
    s::conic2_point(&c.0, Kind::Ellipse, t.0)
);
diff!(
    curve_ellipse2_jet,
    pin_defect = pin_defects::curve_ellipse2_jet,
    |c: GoodConic, t: Angle| x::conic2_jet(&c.0, Kind::Ellipse, t.0),
    s::conic2_jet(&c.0, Kind::Ellipse, t.0)
);
