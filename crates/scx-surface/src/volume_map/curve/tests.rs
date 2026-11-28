// Written for solidcorex. Apache-2.0.
//! Curve images against closed forms: each rule's image holds the map of the source pointwise
//! and the fit's bound holds at dense samples against the closed-form image.

use std::f64::consts::PI;

use scx_num::vector3d::{Point3, Vec3};

use super::*;
use crate::volume_map::{Bend, Twist};

fn mm(x: f64) -> Length {
    Length::from_mm(x)
}

fn p(x: f64, y: f64, z: f64) -> Point3 {
    Point3::new(x, y, z)
}

fn world() -> Frame {
    Frame::new(
        p(0.0, 0.0, 0.0),
        Vec3::new(0.0, 0.0, 1.0),
        Vec3::new(1.0, 0.0, 0.0),
    )
    .unwrap()
}

/// A frame off every world axis.
fn skew() -> Frame {
    Frame::new(
        p(1.0, -2.0, 0.5),
        Vec3::new(1.0, 1.0, 2.0),
        Vec3::new(1.0, -1.0, 0.0),
    )
    .unwrap()
}

fn bend(f: Frame, d: f64) -> VolumeMap {
    VolumeMap::Bend(Bend::new(f, mm(d)).unwrap())
}
fn unbend(f: Frame, d: f64) -> VolumeMap {
    VolumeMap::Unbend(Bend::new(f, mm(d)).unwrap())
}
fn twist(f: Frame, pitch: f64) -> VolumeMap {
    VolumeMap::Twist(Twist::new(f, mm(pitch)).unwrap())
}

fn line(o: Point3, d: Vec3) -> Curve {
    Curve::Line(Line::new(o, d).unwrap())
}

fn span(a: f64, b: f64) -> Extent1d {
    Extent1d::new(a, b).unwrap()
}

fn dist(a: Point3, b: Point3) -> f64 {
    (a - b).length()
}

fn image(
    m: &VolumeMap,
    c: &Curve,
    over: Extent1d,
    aim: f64,
) -> (Result<CurveImage, CurveNot>, Budget) {
    let mut budget = Budget::unlimited();
    let r = m.image_curve(c, over, aim, &mut budget, &Stop::new());
    (r, budget)
}

/// The image's point for the source's parameter `t`.
fn at(img: &CurveImage, t: f64) -> Point3 {
    match img {
        CurveImage::Exact { curve, param, .. } => {
            let (k, c) = param.parts();
            curve.point(k * t + c)
        }
        CurveImage::Fitted(f) => f.curve.point(t),
    }
}

fn samples(e: Extent1d, n: usize) -> impl Iterator<Item = f64> {
    (0..=n).map(move |k| e.min() + e.length() * k as f64 / n as f64)
}

/// An exact image holds the map of the source at every sample, to rounding.
fn assert_exact(m: &VolumeMap, c: &Curve, over: Extent1d) -> CurveImage {
    let (r, _) = image(m, c, over, 1e-6);
    let img = r.unwrap();
    assert!(matches!(img, CurveImage::Exact { .. }), "{img:?}");
    for t in samples(over, 64) {
        let want = m.map(c.point(t));
        let got = at(&img, t);
        assert!(
            dist(want, got) <= 1e-12 * (1.0 + want.to_vec().length()),
            "t {t}: {want:?} {got:?}"
        );
    }
    if let CurveImage::Exact { over: o, param, .. } = &img {
        // The image's span is the parameter map of the source's.
        assert_eq!(*o, param.over(over).unwrap());
    }
    img
}

/// A fit holds the map of the source within its bound, the bound within the aim.
fn assert_fitted(m: &VolumeMap, c: &Curve, over: Extent1d, aim: f64) -> Fit {
    let (r, _) = image(m, c, over, aim);
    let CurveImage::Fitted(f) = r.unwrap() else {
        panic!("expected a fit")
    };
    assert!(f.bound <= aim, "{} > {aim}", f.bound);
    for t in samples(over, 800) {
        let e = dist(m.map(c.point(t)), f.curve.point(t));
        assert!(e <= f.bound, "t {t}: {e} > {}", f.bound);
    }
    f
}

#[test]
fn a_line_under_a_twist_is_a_helix_within_the_bound() {
    let pitch = 4.0;
    let m = twist(world(), pitch);
    let c = line(p(1.5, 0.5, -1.0), Vec3::new(0.0, 0.0, 1.0));
    let over = span(0.0, 6.0);
    let mut last = 0;
    for aim in [1e-3, 1e-6] {
        let mut budget = Budget::unlimited();
        let r = m
            .image_curve(&c, over, aim, &mut budget, &Stop::new())
            .unwrap();
        let CurveImage::Fitted(f) = r else {
            panic!("{r:?}")
        };
        assert!(f.bound <= aim);
        // The helix of 77 §5.1: (u cos a - v sin a, u sin a + v cos a, w), a = 2 pi w / p.
        for t in samples(over, 600) {
            let w = -1.0 + t;
            let (sa, ca) = fmath::sin_cos(2.0 * PI * w / pitch);
            let want = p(1.5 * ca - 0.5 * sa, 1.5 * sa + 0.5 * ca, w);
            let e = dist(want, f.curve.point(t));
            assert!(e <= f.bound, "aim {aim} t {t}: {e} > {}", f.bound);
        }
        assert!(f.spans.len() >= last);
        last = f.spans.len();
        eprintln!(
            "helix aim {aim}: spans {} fits {} evals {} bound {:e}",
            f.spans.len(),
            f.fits,
            budget.used(),
            f.bound
        );
    }
}

#[test]
fn a_twisted_line_on_the_axis_and_in_a_square_plane_are_exact() {
    let m = twist(skew(), 3.0);
    let f = skew();
    // On the axis: itself.
    let on = line(f.origin(), *f.z() * 2.0);
    assert_exact(&m, &on, span(-1.0, 2.0));
    // Square to the axis: turned.
    let origin = f.origin() + *f.x() * 1.5 + *f.z() * 0.7;
    let sq = line(origin, *f.x() * 0.5 + *f.y() * 2.0);
    assert_exact(&m, &sq, span(-2.0, 3.0));
    // A circle square to the axis, off it.
    let cf = Frame::new(origin, *f.z(), *f.y()).unwrap();
    let c = Curve::Circle(Circle::new(cf, mm(1.2), Sense::Reversed).unwrap());
    assert_exact(&m, &c, span(0.0, 5.0));
}

#[test]
fn bend_lines_by_rule() {
    let d = 3.0;
    for f in [world(), skew()] {
        let m = bend(f, d);
        let at_ = |u: f64, v: f64, w: f64| f.origin() + *f.x() * u + *f.y() * v + *f.z() * w;
        // Along the axis, along u, and along v (a circle).
        assert_exact(&m, &line(at_(1.0, 2.0, 0.0), *f.z() * 1.5), span(-1.0, 1.0));
        assert_exact(&m, &line(at_(1.0, 2.0, 0.5), *f.x() * 2.0), span(0.0, 2.0));
        let img = assert_exact(
            &m,
            &line(at_(2.0, -1.0, 0.5), *f.y() * 1.5),
            span(-1.0, 3.0),
        );
        let CurveImage::Exact {
            curve: Curve::Circle(c),
            param,
            ..
        } = img
        else {
            panic!("a circle")
        };
        assert!((c.radius().mm() - 2.0).abs() < 1e-14);
        let CurveParam::Affine { scale, shift } = param else {
            panic!()
        };
        assert!((scale - 0.5).abs() < 1e-14 && (shift + 1.0 / d).abs() < 1e-14);
    }
}

#[test]
fn a_circle_in_a_plane_of_constant_v_is_an_arc_under_the_bend() {
    let d = 2.5;
    let f = skew();
    let m = bend(f, d);
    let o = f.origin() + *f.x() * 4.0 + *f.y() * 1.2 + *f.z() * 0.3;
    // Axis along y: the plane v = 1.2.
    let cf = Frame::new(o, *f.y(), Vec3::new(0.6, 0.0, 0.8).x_in(&f)).unwrap();
    let c = Curve::Circle(Circle::new(cf, mm(1.1), Sense::Forward).unwrap());
    let img = assert_exact(&m, &c, span(0.5, 2.5));
    assert!(matches!(
        img,
        CurveImage::Exact {
            curve: Curve::Circle(_),
            param: CurveParam::Same,
            ..
        }
    ));
    // Out of the domain.
    let far = f.origin() + *f.y() * (PI * d + 0.5);
    let cf = Frame::new(far, *f.y(), *f.x()).unwrap();
    let c = Curve::Circle(Circle::new(cf, mm(1.0), Sense::Forward).unwrap());
    assert!(matches!(
        image(&m, &c, span(0.0, 1.0), 1e-6).0,
        Err(CurveNot::OutsideDomain)
    ));
}

trait InFrame {
    fn x_in(self, f: &Frame) -> Vec3;
}
impl InFrame for Vec3 {
    /// The local components `(a, b, c)` as a world vector.
    fn x_in(self, f: &Frame) -> Vec3 {
        *f.x() * self.x + *f.y() * self.y + *f.z() * self.z
    }
}

#[test]
fn an_arc_off_the_rules_is_fitted_within_its_bound() {
    // A circle square to the axis, off it, under a bend: no rule.
    let f = world();
    let m = bend(f, 4.0);
    let cf = Frame::new(
        p(3.0, 0.0, 1.0),
        Vec3::new(0.0, 0.0, 1.0),
        Vec3::new(1.0, 0.0, 0.0),
    )
    .unwrap();
    let c = Curve::Circle(Circle::new(cf, mm(1.0), Sense::Forward).unwrap());
    for aim in [1e-3, 1e-6] {
        let fit = assert_fitted(&m, &c, span(0.0, 2.0 * PI), aim);
        eprintln!("arc under bend {aim}: spans {}", fit.spans.len());
    }
}

#[test]
fn unbend_rules() {
    let d = 2.0;
    let f = skew();
    let m = unbend(f, d);
    let at_ = |x: f64, y: f64, w: f64| f.origin() + *f.x() * x + *f.y() * y + *f.z() * w;
    // A line along the axis.
    assert_exact(&m, &line(at_(2.0, 1.0, 0.0), *f.z() * 2.0), span(0.0, 1.0));
    // A radial line, one side of the axis, either side.
    let dir = *f.x() * 1.0 + *f.y() * 1.0;
    assert_exact(&m, &line(at_(1.0, 1.0, 0.3), dir), span(0.2, 2.0));
    assert_exact(&m, &line(at_(-1.0, -1.0, 0.3), dir), span(-2.0, -0.2));
    assert!(matches!(
        image(&m, &line(at_(1.0, 1.0, 0.3), dir), span(-2.0, 2.0), 1e-6).0,
        Err(CurveNot::MeetsAxis)
    ));
    assert!(matches!(
        image(&m, &line(at_(0.0, 0.0, 0.3), *f.z()), span(0.0, 1.0), 1e-6).0,
        Err(CurveNot::MeetsAxis)
    ));
    // A coaxial circle: a line along v. In both senses; an arc across the cut is refused.
    for sense in [Sense::Forward, Sense::Reversed] {
        for (zdir, over) in [(*f.z(), span(0.5, 2.0)), (-*f.z(), span(-2.0, 1.0))] {
            let cf = Frame::new(at_(0.0, 0.0, 0.7), zdir, *f.x() + *f.y() * 0.5).unwrap();
            let c = Curve::Circle(Circle::new(cf, mm(1.5), sense).unwrap());
            let img = assert_exact(&m, &c, over);
            assert!(matches!(
                img,
                CurveImage::Exact {
                    curve: Curve::Line(_),
                    param: CurveParam::Same,
                    ..
                }
            ));
        }
    }
    let cf = Frame::new(at_(0.0, 0.0, 0.7), *f.z(), *f.x()).unwrap();
    let c = Curve::Circle(Circle::new(cf, mm(1.5), Sense::Forward).unwrap());
    assert!(matches!(
        image(&m, &c, span(0.0, 4.0), 1e-6).0,
        Err(CurveNot::OnCut)
    ));
    assert!(matches!(
        image(&m, &c, span(0.0, 2.0 * PI), 1e-6).0,
        Err(CurveNot::OnCut)
    ));
}

#[test]
fn a_general_line_under_an_unbend_is_fitted() {
    let m = unbend(world(), 2.0);
    // Off the axis, in the x > 0 half, with a z rise.
    let c = line(p(1.0, 0.5, 0.0), Vec3::new(1.0, 1.0, 0.5));
    for aim in [1e-4, 1e-8] {
        let fit = assert_fitted(&m, &c, span(0.0, 2.0), aim);
        eprintln!("unbend line {aim}: spans {}", fit.spans.len());
    }
}

#[test]
fn a_fit_across_the_unbend_cut_is_refused_not_proved() {
    let m = unbend(world(), 2.0);
    // Crosses y = 0 at x < 0: a jump of 2 pi d in v.
    let c = line(p(-1.0, -0.5, 0.0), Vec3::new(0.2, 1.0, 0.5));
    let mut budget = Budget::new(200_000);
    let r = m.image_curve(&c, span(0.0, 1.0), 1e-4, &mut budget, &Stop::new());
    assert!(
        matches!(r, Err(CurveNot::Unproven { .. } | CurveNot::Fit(_))),
        "{r:?}"
    );
}

#[test]
fn a_chain_is_exact_when_every_step_is_and_fitted_whole_otherwise() {
    let f = world();
    // A line along u at v0, w0: the bend (a radial line at an angle), then a twist (square
    // to the axis): exact, the parameter kept.
    let chain = VolumeMap::Chain(vec![bend(f, 3.0), twist(f, 5.0)].into());
    let c = line(p(1.0, 1.5, 0.8), Vec3::new(1.0, 0.0, 0.0));
    let img = assert_exact(&chain, &c, span(0.0, 2.0));
    assert!(matches!(
        img,
        CurveImage::Exact {
            param: CurveParam::Same,
            ..
        }
    ));
    // A line along v: the bend's circle (Affine), then a twist turning it: affine kept.
    let c = line(p(2.0, -1.0, 0.8), Vec3::new(0.0, 1.0, 0.0));
    let img = assert_exact(&chain, &c, span(0.0, 3.0));
    assert!(matches!(
        img,
        CurveImage::Exact {
            param: CurveParam::Affine { .. },
            ..
        }
    ));
    // A twist then a bend of a line along the axis: a helix, bent: fitted whole.
    let chain = VolumeMap::Chain(vec![twist(f, 6.0), bend(f, 4.0)].into());
    let c = line(p(1.0, 0.0, 0.0), Vec3::new(0.0, 0.0, 1.0));
    let fit = assert_fitted(&chain, &c, span(0.0, 3.0), 1e-6);
    eprintln!("chain {}: spans {}", 1e-6, fit.spans.len());
    // A round trip: the unbend of the bend's image of a general line is the line.
    let rt = VolumeMap::Chain(vec![bend(f, 3.0), unbend(f, 3.0)].into());
    let c = line(p(2.0, 0.3, 0.0), Vec3::new(0.5, 0.5, 1.0));
    let fit = assert_fitted(&rt, &c, span(0.0, 1.0), 1e-8);
    for t in samples(span(0.0, 1.0), 50) {
        assert!(dist(fit.curve.point(t), c.point(t)) <= 2e-8);
    }
}

#[test]
fn a_nurbs_curve_is_fitted_breaking_at_its_knots() {
    use scx_nurbs::{KnotVector, NurbsCurve};
    let m = twist(world(), 5.0);
    let knots = vec![0.0, 0.0, 0.0, 0.0, 1.0, 2.0, 3.0, 3.0, 3.0, 3.0];
    let pts = vec![
        [0.0, 0.0, 0.0],
        [1.0, 0.5, 0.5],
        [2.0, -0.5, 1.0],
        [3.0, 0.0, 1.5],
        [4.0, 1.0, 2.0],
        [5.0, 1.0, 2.5],
    ];
    let n = NurbsCurve::new(KnotVector::new(3, knots).unwrap(), pts, None).unwrap();
    let c = Curve::Nurbs(Arc::new(n));
    let fit = assert_fitted(&m, &c, span(0.0, 3.0), 1e-6);
    eprintln!("nurbs under twist: spans {}", fit.spans.len());
}
