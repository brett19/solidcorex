// Written for solidcorex. Apache-2.0.
//! `scx-curve`'s enum and queries against the pin's `curve::curve::Curve::evaluate` and the
//! kinds' inversions (63 §3.5, CU6).
//!
//! The facade's two mappings (63 X5):
//! - the pin's `from_left` is an [`Approach`]: `true` is `Side::Left`, the span that starts
//!   at the knot, so [`Approach::Above`]; `false` is [`Approach::Below`];
//! - the pin's `non_zero_tangents = true` output is compared as a direction: its first
//!   derivative scaled to unit length against [`Curve::tangent_dir`]. With the flag off the
//!   derivatives are the true ones, scx's [`Curve::jet`].
//!
//! The NURBS arm is compared at parameters inside the knots' domain: outside it the pin
//! clamps the parameter and scx continues the end pieces (CU2). A conic is not compared
//! through the pin's `Curve`: its `evaluate` runs on the carried NURB's parameter, not the
//! angle; `t_curve` compares the conics' `evaluate_step` instead.
//!
//! The inversions: a line's foot parameter against `Line::step_inversion`; a circle's foot
//! distance against `Ellipse::drop_point_fast` (which returns a NURB parameter, so the
//! distance is what both share); an ellipse's foot angle against `Ellipse::step_inversion`
//! for points on the curve only, the one place its `atan2` is the foot. An ellipse's foot off
//! the curve is held to the certified truth in scx-curve's own tests.

use std::sync::Arc;

use crate::diff;
use crate::harness::{Gen, SplitMix64, Tol, run_tol};
use crate::pin_defects;
use crate::t_curve::{DEG, GoodConic, GoodLine, axes3, frame3, sense};
use crate::t_nurbs;

use scx_curve::{Circle, Curve, Ellipse, Line};
use scx_num::units::Length;
use scx_num::vector3d::{Point3, Vec3};
use scx_nurbs::{Approach, KnotVector, NurbsCurve};
use scx_testkit::data::CurveData;

/// A NURBS curve and a parameter in its domain: the seeded curves of `t_nurbs` that scx
/// builds (a knot of multiplicity above the degree is refused, CU3) and curves
/// whose first derivative vanishes (doubled and tripled control points at an end and at a
/// double knot inside, rational and not, and a curve collapsed to a point).
#[derive(Clone, Debug)]
pub struct NurbsAt(pub t_nurbs::At);

/// A query in a conic's frame, in units of its first radius: `(u, v, w)` is
/// `o + a (u x + v y + w z)`.
#[derive(Clone, Debug)]
pub struct Local(pub [f64; 3]);

fn cusps() -> Vec<(CurveData<3>, Vec<f64>)> {
    let clamped3 = vec![0.0, 0.0, 0.0, 0.0, 1.0, 1.0, 1.0, 1.0];
    let o = [0.0; 3];
    let c = |degree, knots: &[f64], points: &[[f64; 3]], weights: Option<Vec<f64>>| CurveData {
        degree,
        knots: knots.to_vec(),
        points: points.to_vec(),
        weights,
    };
    let ends = vec![0.0, 1e-9, 0.5, 1.0 - 1e-9, 1.0];
    let mut v = vec![
        // Doubled start, doubled end, tripled start.
        (
            c(
                3,
                &clamped3,
                &[o, o, [2.0, 0.0, 0.0], [3.0, 1.0, 0.0]],
                None,
            ),
            ends.clone(),
        ),
        (
            c(
                3,
                &clamped3,
                &[[0.0, 1.0, 0.0], [1.0, 0.0, 0.0], o, o],
                None,
            ),
            ends.clone(),
        ),
        (
            c(3, &clamped3, &[o, o, o, [3.0, -1.0, 2.0]], None),
            ends.clone(),
        ),
        (
            c(
                3,
                &clamped3,
                &[o, o, [2.0, 0.0, 1.0], [3.0, 1.0, 0.0]],
                Some(vec![1.0, 2.0, 0.5, 1.0]),
            ),
            ends.clone(),
        ),
        // Collapsed to a point.
        (
            c(1, &[0.0, 0.0, 1.0, 1.0], &[[1.0, 2.0, 3.0]; 2], None),
            ends,
        ),
    ];
    // A cusp at a double knot inside, below and at the domain's middle.
    for k in [0.4, 0.5, 0.7] {
        let p = [1.0, 0.0, 0.0];
        v.push((
            c(
                2,
                &[0.0, 0.0, 0.0, k, k, 1.0, 1.0, 1.0],
                &[o, p, p, p, [1.0, 1.0, 0.0]],
                None,
            ),
            vec![0.0, 0.5 * k, k, k.next_down(), k.next_up(), 1.0],
        ));
    }
    v
}

impl Gen for NurbsAt {
    fn pool(rng: &mut SplitMix64) -> Vec<Self> {
        let mut v: Vec<Self> = t_nurbs::At::pool(rng)
            .into_iter()
            .filter(|a| (0.0..=1.0).contains(&a.u) && t_nurbs::x::curve(a).is_some())
            .map(NurbsAt)
            .collect();
        for (curve, us) in cusps() {
            v.extend(us.into_iter().map(|u| {
                NurbsAt(t_nurbs::At {
                    curve: curve.clone(),
                    u,
                })
            }));
        }
        v
    }
}

impl Gen for Local {
    fn pool(rng: &mut SplitMix64) -> Vec<Self> {
        let mut v = vec![
            [0.0; 3],
            [0.0, 0.0, 1.0],
            [1e-20, 0.0, 1.0],
            [1e-9, -1e-9, 0.5],
            [0.5, 0.0, 0.0],
            [2.0, 0.0, 0.0],
            [0.0, 0.3, 0.0],
            [0.0, -4.0, 0.0],
            [1.0, 0.0, 0.0],
            [-0.3, 0.7, 2.0],
            [1e6, 1e6, 0.0],
        ];
        v.extend((0..12).map(|_| core::array::from_fn(|_| rng.uniform(-3.0, 3.0))));
        v.into_iter().map(Local).collect()
    }
}

/// A unit direction, as both sides scale one: by the largest coordinate first, so any
/// finite non-zero vector has one.
fn unit(v: [f64; 3]) -> Option<[f64; 3]> {
    let v = Vec3::from(v);
    (v / v.max_dimension())
        .normalized()
        .map(|u| u.as_vec().to_array())
}

/// scx's side.
pub mod x {
    use super::*;

    pub fn side(from_left: bool) -> Approach {
        if from_left {
            Approach::Above
        } else {
            Approach::Below
        }
    }

    fn nurbs(a: &t_nurbs::At) -> Curve {
        let d = &a.curve;
        let k = KnotVector::new(d.degree, d.knots.clone()).unwrap();
        Curve::Nurbs(Arc::new(
            NurbsCurve::new(k, d.points.clone(), d.weights.clone()).unwrap(),
        ))
    }

    pub fn nurbs_jet(a: &NurbsAt, from_left: bool) -> Vec<[f64; 3]> {
        let j = nurbs(&a.0).jet::<3>(a.0.u, side(from_left));
        let mut v = vec![j.point.to_array()];
        v.extend(j.d.iter().map(|d| d.to_array()));
        v
    }

    pub fn nurbs_tangent(a: &NurbsAt, from_left: bool) -> Option<[f64; 3]> {
        nurbs(&a.0)
            .tangent_dir(a.0.u, side(from_left))
            .map(|u| u.as_vec().to_array())
    }

    fn line(l: &GoodLine) -> Line {
        Line::new(Point3::from(l.0.o), Vec3::from(l.0.d)).unwrap()
    }

    pub fn line_tangent(l: &GoodLine, t: f64, from_left: bool) -> Option<[f64; 3]> {
        Curve::Line(line(l))
            .tangent_dir(t, side(from_left))
            .map(|u| u.as_vec().to_array())
    }

    pub fn line_invert(l: &GoodLine, p: [f64; 3]) -> f64 {
        line(l).invert(Point3::from(p), None).foot().at
    }

    /// The query `o + a (u x + v y + w z)` in world coordinates.
    pub fn query(c: &GoodConic, q: &Local) -> [f64; 3] {
        let f = frame3(&c.0).unwrap();
        let a = c.0.a;
        let [u, v, w] = q.0;
        (f.origin()
            + f.x().as_vec() * (a * u)
            + f.y().as_vec() * (a * v)
            + f.z().as_vec() * (a * w))
            .to_array()
    }

    pub fn circle_distance(c: &GoodConic, q: &Local) -> f64 {
        let k = Circle::new(frame3(&c.0).unwrap(), Length::from_mm(c.0.a), sense(&c.0)).unwrap();
        k.invert(Point3::from(query(c, q)), None).foot().distance
    }

    pub fn ellipse(c: &GoodConic) -> Ellipse {
        Ellipse::new(
            frame3(&c.0).unwrap(),
            Length::from_mm(c.0.a),
            Length::from_mm(c.0.b),
            sense(&c.0),
        )
        .unwrap()
    }

    /// The ellipse's point at `t`, the query both sides invert.
    pub fn on_ellipse(c: &GoodConic, t: f64) -> [f64; 3] {
        ellipse(c).point(t).to_array()
    }

    pub fn ellipse_invert_on(c: &GoodConic, t: f64) -> f64 {
        ellipse(c)
            .invert(Point3::from(on_ellipse(c, t)), None)
            .foot()
            .at
    }
}

/// The pin's side.
pub mod s {
    use super::*;
    use solidcore::Kernel;
    use solidcore::curve::curve::Curve as PinCurve;
    use solidcore::curve::ellipse::{Ellipse as PinEllipse, EllipseOptions};
    use solidcore::curve::line::Line as PinLine;
    use solidcore::geom::axis2_placement::Axis2Placement;
    use solidcore::geom::extent1d::Extent1d;
    use solidcore::geom::global_solver::{SolutionRequested, SolverOperation};
    use solidcore::nurbs::b_spline_curve::BSplineCurve;

    fn p3(a: [f64; 3]) -> solidcore::Point3 {
        solidcore::Point3::new(a[0], a[1], a[2])
    }

    fn v3(a: [f64; 3]) -> solidcore::Vec3 {
        solidcore::Vec3::new(a[0], a[1], a[2])
    }

    fn arr(v: solidcore::Vec3) -> [f64; 3] {
        [v.x, v.y, v.z]
    }

    fn nurbs(a: &t_nurbs::At) -> PinCurve {
        let n = t_nurbs::s::curve(a).unwrap();
        PinCurve::from(BSplineCurve::from_nurb(n, 3, true))
    }

    fn evaluate(c: &PinCurve, t: f64, n: usize, from_left: bool, nzt: bool) -> Vec<[f64; 3]> {
        let k = Kernel::default();
        c.evaluate(&k, t, n, from_left, nzt)
            .map(|v| v.into_iter().map(arr).collect())
            .unwrap_or_else(|_| vec![[f64::NAN; 3]; n + 1])
    }

    pub fn nurbs_jet(a: &NurbsAt, from_left: bool) -> Vec<[f64; 3]> {
        evaluate(&nurbs(&a.0), a.0.u, 3, from_left, false)
    }

    /// The flag-on first derivative, scaled to unit length.
    pub fn nurbs_tangent(a: &NurbsAt, from_left: bool) -> Option<[f64; 3]> {
        unit(evaluate(&nurbs(&a.0), a.0.u, 2, from_left, true)[1])
    }

    fn line(l: &GoodLine) -> PinLine {
        PinLine::infinite(p3(l.0.o), v3(l.0.d), 3, false)
    }

    pub fn line_tangent(l: &GoodLine, t: f64, from_left: bool) -> Option<[f64; 3]> {
        unit(evaluate(&PinCurve::from(line(l)), t, 1, from_left, true)[1])
    }

    pub fn line_invert(l: &GoodLine, p: [f64; 3]) -> f64 {
        line(l).step_inversion(p3(p)).0
    }

    pub fn circle_distance(c: &GoodConic, q: &Local) -> f64 {
        let k = Kernel::default();
        let (o, x, y) = axes3(&c.0);
        let place = Axis2Placement::new(p3(o), v3(x), v3(y));
        let e = PinEllipse::create_canonical_circle(&k, &place, c.0.a, None, Some(false)).unwrap();
        let domain = e.base().natural_interval();
        e.drop_point_fast::<u8>(
            &k,
            &domain,
            SolverOperation::Minimize,
            p3(x::query(c, q)),
            None,
            0.0,
            None,
            SolutionRequested::Single,
            None,
        )
        .ok()
        .and_then(|s| s.first().map(|s| s.start.solution_value))
        .unwrap_or(f64::NAN)
    }

    pub fn ellipse_invert_on(c: &GoodConic, t: f64) -> f64 {
        let k = Kernel::default();
        let (o, x, y) = axes3(&c.0);
        let e = PinEllipse::new(
            &k,
            p3(o),
            v3(x),
            v3(y),
            Extent1d::from_min_max(0.0, 360.0),
            c.0.a,
            c.0.b,
            &EllipseOptions {
                dim: 3,
                nurb: None,
                inside_out: false,
            },
        )
        .unwrap();
        e.step_inversion(&k, p3(x::on_ellipse(c, t)), false, 0.0).0 / DEG
    }
}

/// A parameter on a line.
#[derive(Clone, Debug)]
pub struct OnLine(pub f64);

impl Gen for OnLine {
    fn pool(rng: &mut SplitMix64) -> Vec<Self> {
        let mut v = vec![0.0, 1.0, -2.5, 1e6];
        v.extend((0..4).map(|_| rng.uniform(-10.0, 10.0)));
        v.into_iter().map(OnLine).collect()
    }
}

/// An angle on the curve, finite.
#[derive(Clone, Debug)]
pub struct OnConic(pub f64);

impl Gen for OnConic {
    fn pool(rng: &mut SplitMix64) -> Vec<Self> {
        use scx_num::math::{HALF_PI, PI};
        let mut v = vec![0.0, HALF_PI, PI, 1.5 * PI, 1e-7, 2.0 * PI - 1e-7, 1.0];
        v.extend((0..10).map(|_| rng.uniform(0.0, 2.0 * PI)));
        v.into_iter().map(OnConic).collect()
    }
}

diff!(
    curve_nurbs_jet,
    |a: NurbsAt, from_left: bool| x::nurbs_jet(&a, from_left),
    s::nurbs_jet(&a, from_left)
);
diff!(
    curve_nurbs_tangent,
    pin_defect = pin_defects::curve_nurbs_tangent,
    |a: NurbsAt, from_left: bool| x::nurbs_tangent(&a, from_left),
    s::nurbs_tangent(&a, from_left)
);
diff!(
    curve_line_tangent,
    |l: GoodLine, t: OnLine, from_left: bool| x::line_tangent(&l, t.0, from_left),
    s::line_tangent(&l, t.0, from_left)
);
diff!(
    curve_line_invert,
    |l: GoodLine, p: [f64; 3]| x::line_invert(&l, p),
    s::line_invert(&l, p)
);
/// A distance near zero carries the rounding of the query's coordinates, not of the
/// distance: `1e-17` for a point on a unit circle, `4e-14` a thousand units out.
const DISTANCE: Tol = Tol {
    rel: 1e-12,
    abs: 1e-12,
};

#[test]
fn curve_circle_invert() {
    run_tol::<(GoodConic, Local), _, _>(
        concat!(module_path!(), "::curve_circle_invert"),
        DISTANCE,
        |(c, q)| x::circle_distance(&c, &q),
        |(c, q)| s::circle_distance(&c, &q),
    );
}
diff!(
    curve_ellipse_invert_on,
    pin_defect = pin_defects::curve_ellipse_invert_on,
    |c: GoodConic, t: OnConic| x::ellipse_invert_on(&c, t.0),
    s::ellipse_invert_on(&c, t.0)
);
