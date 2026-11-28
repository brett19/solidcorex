// Written for solidcorex. Apache-2.0.
//! FT3's tests (77 §8): every bound is judged against the closed form (the distance from the
//! analytic source, or from a B-spline source's own exact evaluation), never against the pin's
//! output. The pin's `within.rs` tests are ported; the analytic sources are new.

use super::within::search;
use super::*;
use crate::{Approach, KnotVector, NurbsCurve};
use scx_num::budget::{Budget, Stop};
use scx_num::extent1d::Extent1d;
use scx_num::interval::Ivl;
use scx_num::jet::Jet2;
use scx_num::trig::Trig;

/// A closed-form curve: `f` over any scalar with sine and cosine.
struct Analytic<F> {
    lo: f64,
    hi: f64,
    f: F,
}

/// The closed form's own evaluator: `f64`, [`Ivl`] and [`Jet2`] run the same code.
trait Form {
    fn at<T: Trig>(&self, t: T) -> [T; 3];
}

impl<F: Form> CurveFn<3> for Analytic<F> {
    fn domain(&self) -> Extent1d {
        Extent1d::new(self.lo, self.hi).expect("a finite domain")
    }
    fn breaks(&self) -> &[f64] {
        &[]
    }
    fn eval<T: Trig>(&self, t: T) -> [T; 3] {
        self.f.at(t)
    }
}

fn k<T: Trig>(x: f64) -> T {
    T::lift(x)
}

/// A circle of radius 10 in the plane z = 0.
struct Circle;
impl Form for Circle {
    fn at<T: Trig>(&self, t: T) -> [T; 3] {
        let (s, c) = t.sin_cos();
        [k::<T>(10.0) * c, k::<T>(10.0) * s, k(0.0)]
    }
}

/// An ellipse of semi-axes 10 and 4.
struct Ellipse;
impl Form for Ellipse {
    fn at<T: Trig>(&self, t: T) -> [T; 3] {
        let (s, c) = t.sin_cos();
        [k::<T>(10.0) * c, k::<T>(4.0) * s, k(0.0)]
    }
}

/// A helix of radius 5 rising 2 a radian.
struct Helix;
impl Form for Helix {
    fn at<T: Trig>(&self, t: T) -> [T; 3] {
        let (s, c) = t.sin_cos();
        [k::<T>(5.0) * c, k::<T>(5.0) * s, k::<T>(2.0) * t]
    }
}

/// The involute of the circle of radius 10.
struct Involute;
impl Form for Involute {
    fn at<T: Trig>(&self, t: T) -> [T; 3] {
        let (s, c) = t.sin_cos();
        let r = k::<T>(10.0);
        [r * (c + t * s), r * (s - t * c), k(0.0)]
    }
}

/// The parabola `(t, t^2 / 4)`.
struct Parabola;
impl Form for Parabola {
    fn at<T: Trig>(&self, t: T) -> [T; 3] {
        [t, k::<T>(0.25) * t.sqr(), k(0.0)]
    }
}

fn analytic<F: Form>(f: F, lo: f64, hi: f64) -> Analytic<F> {
    Analytic { lo, hi, f }
}

fn fit<const D: usize>(
    source: &impl CurveFn<D>,
    within: f64,
    degree: usize,
    start: Option<[f64; D]>,
    end: Option<[f64; D]>,
) -> Result<Fitted<D>, FitError> {
    let aim = FitAim {
        within,
        degree,
        start,
        end,
    };
    fit_within(source, &aim, &mut Budget::unlimited(), &Stop::new())
}

fn sub<const D: usize>(a: [f64; D], b: [f64; D]) -> [f64; D] {
    core::array::from_fn(|i| a[i] - b[i])
}

fn length<const D: usize>(a: [f64; D]) -> f64 {
    a.iter().map(|x| x * x).sum::<f64>().sqrt()
}

/// The largest absolute coordinate of `c`'s control points.
fn size<const D: usize>(c: &NurbsCurve<D>) -> f64 {
    c.points()
        .iter()
        .flatten()
        .fold(0.0_f64, |m, x| m.max(x.abs()))
}

/// The legs `P1 - P0` and `Pn - P(n-1)` of `c`.
fn end_legs<const D: usize>(c: &NurbsCurve<D>) -> ([f64; D], [f64; D]) {
    let p = c.points();
    let n = p.len();
    (sub(p[1], p[0]), sub(p[n - 1], p[n - 2]))
}

/// Checks the fit's structure (clamped on the source's domain, simple interior knots, the
/// degree, one span bound per span on its breaks) and every span's bound against the
/// closed-form distance `|source(t) - fit(t)|` at 64 points of the span and its ends. The
/// largest distance found is returned.
fn holds<const D: usize>(source: &impl CurveFn<D>, fitted: &Fitted<D>, p: usize) -> f64 {
    let c = &fitted.curve;
    let knots = c.knots();
    assert_eq!(c.degree(), p);
    assert!(!c.is_rational());
    let (lo, hi) = (source.domain().min(), source.domain().max());
    let tail = knots.len() - p - 1;
    assert!(knots[..=p].iter().all(|&k| k == lo), "{knots:?}");
    assert!(knots[tail..].iter().all(|&k| k == hi), "{knots:?}");
    let mut breaks = knots.to_vec();
    breaks.dedup();
    assert!(breaks.windows(2).all(|w| w[0] < w[1]), "{knots:?}");
    assert_eq!(fitted.spans.len(), breaks.len() - 1);
    let mut worst = 0.0_f64;
    for (s, w) in fitted.spans.iter().zip(breaks.windows(2)) {
        assert_eq!((s.span.min(), s.span.max()), (w[0], w[1]));
        assert!(s.bound >= 0.0 && s.bound <= fitted.bound);
        for i in 0..=64 {
            let t = w[0] + (w[1] - w[0]) * f64::from(i) / 64.0;
            let a = source.eval(t);
            let b = c.point(t, Approach::Above).unwrap();
            let gap = length(sub(a, b));
            worst = worst.max(gap);
            assert!(gap <= s.bound, "at {t}: {gap} past {}", s.bound);
        }
    }
    assert_eq!(c.point(lo, Approach::Above).unwrap(), source.eval(lo));
    assert_eq!(c.point(hi, Approach::Below).unwrap(), source.eval(hi));
    worst
}

fn curve3(points: &[[f64; 3]], p: usize, knots: Vec<f64>) -> NurbsCurve<3> {
    NurbsCurve::new(KnotVector::new(p, knots).unwrap(), points.to_vec(), None).unwrap()
}

/// The chord-length cubic through `count + 1` points of `f` at even steps over `[t0, t1]`.
fn through(f: &impl CurveFn<3>, count: u32) -> NurbsCurve<3> {
    let (t0, t1) = (f.domain().min(), f.domain().max());
    let points: Vec<[f64; 3]> = (0..=count)
        .map(|i| f.eval(t0 + (t1 - t0) * f64::from(i) / f64::from(count)))
        .collect();
    let t = parameters(&points, Param::ChordLength).unwrap();
    interpolate(&points, &t, 3, &End::Point, &End::Point)
        .unwrap()
        .into_curve()
        .unwrap()
}

fn pin_sources() -> Vec<NurbsCurve<3>> {
    vec![
        through(&analytic(Involute, 0.0, 1.5), 30),
        through(&analytic(Circle, 0.0, 2.0), 30),
        through(&analytic(Parabola, -3.0, 3.0), 30),
    ]
}

/// Checks a ladder of aims on `source`: each fit within its aim, its reaches positive, its
/// bounds holding the closed-form distance, the counts of control points not falling as the
/// aim tightens, and every knot of a looser fit a knot of the tighter one, to the bit.
fn ladder<const D: usize>(
    source: &impl CurveFn<D>,
    aims: &[f64],
    start: Option<[f64; D]>,
    end: Option<[f64; D]>,
) {
    let mut counts = Vec::new();
    let mut knots: Vec<Vec<f64>> = Vec::new();
    for &within in aims {
        let f = fit(source, within, 3, start, end).unwrap();
        assert_eq!(f.ended, Ended::Within, "{within}");
        assert!(f.bound <= within, "{} past {within}", f.bound);
        assert!(f.spans.iter().all(|s| s.bound <= within));
        assert_eq!(f.start.is_some(), start.is_some());
        assert_eq!(f.end.is_some(), end.is_some());
        for h in [f.start, f.end].into_iter().flatten() {
            assert!(h.reach > 0.0 && h.turn < 1e-12, "{h:?}");
        }
        holds(source, &f, 3);
        counts.push(f.curve.points().len());
        knots.push(f.curve.knots().to_vec());
    }
    assert!(counts.windows(2).all(|w| w[0] <= w[1]), "{counts:?}");
    for w in knots.windows(2) {
        assert!(w[0].iter().all(|k| w[1].contains(k)), "{w:?}");
    }
}

/// A Bezier cubic on `[2, 5]` as a closed form, no B-spline for FT1.
struct Cubic([[f64; 3]; 4]);
impl CurveFn<3> for Cubic {
    fn domain(&self) -> Extent1d {
        Extent1d::new(2.0, 5.0).expect("a finite domain")
    }
    fn breaks(&self) -> &[f64] {
        &[]
    }
    fn eval<T: Trig>(&self, t: T) -> [T; 3] {
        let u = (t - T::lift(2.0)) / T::lift(3.0);
        let v = T::lift(1.0) - u;
        let b = [
            v * v * v,
            T::lift(3.0) * u * v * v,
            T::lift(3.0) * u * u * v,
            u * u * u,
        ];
        core::array::from_fn(|i| {
            b.iter()
                .zip(&self.0)
                .fold(T::lift(0.0), |a, (&w, p)| a + w * T::lift(p[i]))
        })
    }
}

/// Pin :554. A source of one cubic span (on [2, 5]) is in the fit's own space: the first fit
/// gives it back, within a million roundings of its size, free, with its own legs as the
/// directions (their reaches the legs' lengths), and with either alone.
#[test]
fn a_source_of_one_cubic_span_is_given_back_in_one_fit() {
    let bezier = [
        [0.0, 0.0, 0.0],
        [1.0, 3.0, 0.0],
        [4.0, 3.0, 1.0],
        [5.0, 0.0, 0.0],
    ];
    let knots = vec![2.0, 2.0, 2.0, 2.0, 5.0, 5.0, 5.0, 5.0];
    let source = curve3(&bezier, 3, knots.clone());
    let rounding = 1e6 * f64::EPSILON * size(&source);
    let free = fit(&source, rounding, 3, None, None).unwrap();
    assert_eq!((free.fits, free.ended), (1, Ended::Within));
    assert!(free.bound <= rounding, "{}", free.bound);
    assert_eq!((free.start, free.end), (None, None));
    assert_eq!(free.curve.knots(), &knots[..]);
    holds(&source, &free, 3);
    let (s, e) = (sub(bezier[1], bezier[0]), sub(bezier[3], bezier[2]));
    let held = fit(&source, rounding, 3, Some(s), Some(e)).unwrap();
    assert_eq!(held.fits, 1);
    assert!(held.bound <= rounding, "{}", held.bound);
    let (hs, he) = (held.start.unwrap(), held.end.unwrap());
    assert!((hs.reach - length(s)).abs() <= rounding, "{hs:?}");
    assert!((he.reach - length(e)).abs() <= rounding, "{he:?}");
    holds(&source, &held, 3);
    let start = fit(&source, rounding, 3, Some(s), None).unwrap();
    assert_eq!((start.fits, start.end), (1, None));
    assert!(start.bound <= rounding, "{}", start.bound);
    let end = fit(&source, rounding, 3, None, Some(e)).unwrap();
    assert_eq!((end.fits, end.start), (1, None));
    assert!(end.bound <= rounding, "{}", end.bound);
    // The same span as a closed form, with no exact certificate: the branch and bound proves a
    // few 1e-7 on the one span of width 3 (its floor falls as the cube of the width), so an aim
    // of 1e-6 is met in one fit.
    let closed = fit(&Cubic(bezier), 1e-6, 3, None, None).unwrap();
    assert_eq!(
        (closed.fits, closed.ended),
        (1, Ended::Within),
        "{closed:?}"
    );
    holds(&Cubic(bezier), &closed, 3);
}

/// Pin :595. The pin's three interpolated sources, refitted for a ladder of aims, free and
/// with their own end legs as directions.
#[test]
fn pin_sources_are_refitted_within_a_ladder_of_aims() {
    for source in pin_sources() {
        let (s, e) = end_legs(&source);
        ladder(&source, &[1e-2, 1e-4, 1e-6], None, None);
        ladder(&source, &[1e-2, 1e-4, 1e-6], Some(s), Some(e));
    }
}

/// 77's test row on the closed forms themselves: the circle, the ellipse and the helix (and the
/// pin's involute and parabola), free and with their own tangents held, each bound holding the
/// closed-form distance and within its aim, the looser aim's knots among the tighter's.
#[test]
fn closed_forms_are_refitted_within_a_ladder_of_aims() {
    let aims = [1e-2, 1e-4, 1e-6];
    macro_rules! both {
        ($form:expr, $lo:expr, $hi:expr) => {{
            let src = analytic($form, $lo, $hi);
            ladder(&src, &aims, None, None);
            let (s, e) = (tangent(&src, $lo), tangent(&src, $hi));
            ladder(&src, &aims, Some(s), Some(e));
        }};
    }
    both!(Circle, 0.0, 2.0);
    both!(Ellipse, -1.0, 2.5);
    both!(Helix, 0.0, 6.0);
    // Off the involute's cusp at 0, where its tangent vanishes.
    both!(Involute, 0.5, 2.0);
    both!(Parabola, -3.0, 3.0);
}

/// The source's tangent at `t`, from its jet at the point.
fn tangent(src: &impl CurveFn<3>, t: f64) -> [f64; 3] {
    src.eval(Jet2::variable(Ivl::from(t))).map(|j| j.d1.mid())
}

/// Pin :639. One arc cut in two at 45 degrees (the join the same bits in both halves: one
/// closed form). Refitted with the arc's tangent at the join held by both, the fits' legs at
/// the join are parallel within rounding: `4 eps (size + reach) / reach` of sine per leg, and a
/// few `eps` more for the cross product and the norms (16 allowed). Refitted free, the same
/// measure is larger.
#[test]
fn two_halves_of_an_arc_refitted_with_the_shared_tangent_meet_g1() {
    let join = core::f64::consts::FRAC_PI_4;
    let first = analytic(Circle, 0.0, join);
    let second = analytic(Circle, join, 2.0 * join);
    assert_eq!(first.eval(join), second.eval(join));
    let (sin, cos) = scx_num::fmath::sin_cos(join);
    let tangent = [-sin, cos, 0.0];
    let within = 1e-4;
    let a = fit(&first, within, 3, None, Some(tangent)).unwrap();
    let b = fit(&second, within, 3, Some(tangent), None).unwrap();
    assert!(a.bound <= within && b.bound <= within, "{a:?} {b:?}");
    holds(&first, &a, 3);
    holds(&second, &b, 3);
    let (_, la) = end_legs(&a.curve);
    let (lb, _) = end_legs(&b.curve);
    let (ea, sb) = (a.end.unwrap().reach, b.start.unwrap().reach);
    assert!(ea > 0.0 && sb > 0.0, "{ea} {sb}");
    let at_join = size(&a.curve).max(size(&b.curve));
    let allowed =
        4.0 * f64::EPSILON * ((at_join + ea) / ea + (at_join + sb) / sb) + 16.0 * f64::EPSILON;
    let held = sine(la, lb);
    assert!(held <= allowed, "{held} past {allowed}");
    let a = fit(&first, within, 3, None, None).unwrap();
    let b = fit(&second, within, 3, None, None).unwrap();
    let (_, la) = end_legs(&a.curve);
    let (lb, _) = end_legs(&b.curve);
    let free = sine(la, lb);
    assert!(free > held, "{free} not above {held}");
}

fn sine(a: [f64; 3], b: [f64; 3]) -> f64 {
    let c = [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ];
    length(c) / (length(a) * length(b))
}

/// Pin :674. Too few control points allowed for the aim: the search stops at the last fit the
/// cap holds, past the aim. Four control points of a cubic make one span and a second needs
/// five, so a cap of 4 makes one fit; the involute is no cubic.
#[test]
fn the_cap_gives_the_last_fit_past_the_aim() {
    let aim = FitAim {
        within: 1e-9,
        degree: 3,
        start: None,
        end: None,
    };
    let bspline = through(&analytic(Involute, 0.0, 1.5), 30);
    let closed = analytic(Involute, 0.0, 1.5);
    let on_bspline =
        |most: usize| search(&bspline, &aim, most, &mut Budget::unlimited(), &Stop::new()).unwrap();
    let on_closed =
        |most: usize| search(&closed, &aim, most, &mut Budget::unlimited(), &Stop::new()).unwrap();
    for one in [on_bspline(4), on_closed(4)] {
        assert_eq!((one.fits, one.ended), (1, Ended::Cap));
        assert_eq!(one.curve.points().len(), 4);
        assert!(one.bound > 1e-9, "{}", one.bound);
    }
    let few = on_bspline(9);
    assert!(few.curve.points().len() <= 9 && few.ended == Ended::Cap);
    assert!(few.bound > 1e-9, "{}", few.bound);
    holds(&bspline, &few, 3);
    let few = on_closed(9);
    assert!(few.curve.points().len() <= 9 && few.ended == Ended::Cap);
    holds(&closed, &few, 3);
}

/// Pin :689. Degree 2 with both directions starts on two spans: it is refitted, its reaches
/// positive; a cap of three control points cannot hold the first fit.
#[test]
fn a_quadratic_with_both_directions_starts_on_two_spans() {
    let source = through(&analytic(Circle, 0.0, 2.0), 30);
    let (s, e) = end_legs(&source);
    let f = fit(&source, 1e-3, 2, Some(s), Some(e)).unwrap();
    assert!(f.bound <= 1e-3, "{}", f.bound);
    assert!(f.curve.points().len() >= 4);
    assert!(f.start.unwrap().reach > 0.0 && f.end.unwrap().reach > 0.0);
    holds(&source, &f, 2);
    let aim = FitAim {
        within: 1e-3,
        degree: 2,
        start: Some(s),
        end: Some(e),
    };
    assert_eq!(
        search(&source, &aim, 3, &mut Budget::unlimited(), &Stop::new()),
        Err(FitError::TooFewControlPoints {
            count: 3,
            needed: 4
        })
    );
}

/// Pin :711. A direction of any finite length is read as its unit. The source leaves its start
/// along (1, -2, 0) and reaches its end along (2, 1, 0); the largest directions against either
/// are refused, and along them directions of that size give the fit of their units, to the bit.
#[test]
fn a_direction_of_any_finite_length_is_read_as_its_unit() {
    let source = curve3(
        &[
            [0.0, 0.0, 0.0],
            [1.0, -2.0, 0.0],
            [3.0, -3.0, 0.0],
            [5.0, -2.0, 0.0],
        ],
        3,
        vec![0.0, 0.0, 0.0, 0.0, 1.0, 1.0, 1.0, 1.0],
    );
    let big = f64::MAX;
    assert_eq!(
        fit(&source, 1e-3, 3, Some([big, big, 0.0]), None),
        Err(FitError::StartAgainst)
    );
    assert_eq!(
        fit(&source, 1e-3, 3, None, Some([-big, big, 0.0])),
        Err(FitError::EndAgainst)
    );
    let small = fit(
        &source,
        1e-3,
        3,
        Some([1.0, -2.0, 0.0]),
        Some([2.0, 1.0, 0.0]),
    )
    .unwrap();
    let large = fit(
        &source,
        1e-3,
        3,
        Some([0.5 * big, -big, 0.0]),
        Some([big, 0.5 * big, 0.0]),
    )
    .unwrap();
    assert_eq!(small, large);
    assert!(
        small.bound <= 1e-3 && small.ended == Ended::Within,
        "{small:?}"
    );
    for h in [small.start, small.end].into_iter().flatten() {
        assert!(h.reach > 0.0 && h.turn < 1e-12, "{h:?}");
    }
}

/// Pin's floor test: an aim under what FT1's rounding comes to for a polynomial source stops
/// after the first fit, its bound not below the floor.
#[test]
fn an_aim_below_the_floor_stops_after_one_fit() {
    for source in pin_sources() {
        let floor = crate::deviation::distance_floor(&source).unwrap();
        assert!(floor > 0.0);
        for within in [f64::MIN_POSITIVE, 1e-300, 0.5 * floor] {
            let f = fit(&source, within, 3, None, None).unwrap();
            assert_eq!((f.fits, f.ended), (1, Ended::Floor), "{within}");
            assert!(f.bound >= floor && f.bound > within);
            holds(&source, &f, 3);
        }
    }
    // A closed form: the search runs until the rounding of the evaluation is reached.
    let circle = analytic(Circle, 0.0, 2.0);
    let f = fit(&circle, f64::MIN_POSITIVE, 3, None, None).unwrap();
    assert!(
        matches!(f.ended, Ended::Floor | Ended::Cap),
        "{:?}",
        f.ended
    );
    holds(&circle, &f, 3);
}

/// A rational source: the quarter circles of a full circle of radius 10 as a rational quadratic.
/// FT1's rational bound, tightened by the branch and bound, holds the distance from the true
/// circle (whose closed form the rational curve is) and comes within each aim.
#[test]
fn a_rational_circle_is_refitted_within_its_aims() {
    let h = core::f64::consts::FRAC_1_SQRT_2;
    let r = 10.0;
    let pts = [
        [r, 0.0, 0.0],
        [r, r, 0.0],
        [0.0, r, 0.0],
        [-r, r, 0.0],
        [-r, 0.0, 0.0],
    ];
    let weights = vec![1.0, h, 1.0, h, 1.0];
    let knots = vec![0.0, 0.0, 0.0, 1.0, 1.0, 2.0, 2.0, 2.0];
    let half = NurbsCurve::new(
        KnotVector::new(2, knots).unwrap(),
        pts.to_vec(),
        Some(weights),
    )
    .unwrap();
    assert!(half.is_rational());
    for within in [1e-2, 1e-4, 1e-6] {
        let f = fit(&half, within, 3, None, None).unwrap();
        assert_eq!(f.ended, Ended::Within, "{within}");
        holds(&half, &f, 3);
        // The truth: the rational curve's points lie on the circle of radius 10.
        for i in 0..=200 {
            let t = 2.0 * f64::from(i) / 200.0;
            let q = f.curve.point(t, Approach::Above).unwrap();
            let c = half.point(t, Approach::Above).unwrap();
            assert!((length(c) - r).abs() <= 1e-12);
            assert!(length(sub(q, c)) <= f.bound);
        }
    }
}

/// The jets of a B-spline over an interval ([`Jet2`] through `point_over`) enclose its first
/// two derivatives at points of the interval, rational or not, across a knot.
#[test]
fn jets_of_a_bspline_over_an_interval_enclose_its_derivatives() {
    let h = core::f64::consts::FRAC_1_SQRT_2;
    let pts = [[1.0, 0.0], [1.0, 1.0], [0.0, 1.0], [-1.0, 1.0], [-1.0, 0.0]];
    let knots = vec![0.0, 0.0, 0.0, 1.0, 1.0, 2.0, 2.0, 2.0];
    for weights in [None, Some(vec![1.0, h, 1.0, h, 1.0])] {
        let c = NurbsCurve::new(
            KnotVector::new(2, knots.clone()).unwrap(),
            pts.to_vec(),
            weights,
        )
        .unwrap();
        for (lo, hi) in [(0.1, 0.7), (0.0, 1.0), (0.5, 1.5)] {
            let j = c.point_over(Jet2::variable(Ivl::new(lo, hi)), Approach::Above);
            for i in 0..=20 {
                let t = lo + (hi - lo) * f64::from(i) / 20.0;
                for side in [Approach::Below, Approach::Above] {
                    let d = c.derivs::<3>(t, side).unwrap();
                    for (x, jx) in j.iter().enumerate() {
                        let inside = |v: f64, i: Ivl| i.lo <= v && v <= i.hi;
                        assert!(inside(d[0][x], jx.f), "{t}");
                        assert!(inside(d[1][x], jx.d1), "{t}");
                        assert!(inside(d[2][x], jx.d2), "{t}");
                    }
                }
            }
        }
    }
}

/// [`least_squares_along`] on exact data of a cubic with its own end legs' directions gives its
/// control points back, the reaches the legs' lengths; with no direction it is
/// [`least_squares`] with the end points held.
#[test]
fn least_squares_along_gives_a_cubic_back() {
    let pts = [
        [0.0, 0.0],
        [1.0, 2.0],
        [2.5, 2.5],
        [4.0, 1.0],
        [5.0, -1.0],
        [6.0, 0.0],
    ];
    let kv = KnotVector::new(3, vec![0.0, 0.0, 0.0, 0.0, 0.3, 0.7, 1.0, 1.0, 1.0, 1.0]).unwrap();
    let c = NurbsCurve::new(kv.clone(), pts.to_vec(), None).unwrap();
    let t: Vec<f64> = (0..=24).map(|i| f64::from(i) / 24.0).collect();
    let data: Vec<[f64; 2]> = t
        .iter()
        .map(|&u| c.point(u, Approach::Above).unwrap())
        .collect();
    let s = sub(pts[1], pts[0]);
    let e = sub(pts[5], pts[4]);
    for (start, end) in [
        (Some(s), Some(e)),
        (Some(s), None),
        (None, Some(e)),
        (None, None),
    ] {
        let d = least_squares_along(&kv, &data, &t, start, end).unwrap();
        for (a, b) in d.spline.points().iter().zip(&pts) {
            assert!(length(sub(*a, *b)) <= 1e-13, "{a:?} {b:?}");
        }
        assert_eq!(d.start_reach.is_some(), start.is_some());
        if let Some(r) = d.start_reach {
            assert!((r - length(s)).abs() <= 1e-13, "{r}");
        }
        if let Some(r) = d.end_reach {
            assert!((r - length(e)).abs() <= 1e-13, "{r}");
        }
    }
    let plain = least_squares(&kv, &data, &t, &End::Point, &End::Point).unwrap();
    let along = least_squares_along(&kv, &data, &t, None, None).unwrap();
    for (a, b) in plain.points().iter().zip(along.spline.points()) {
        assert!(length(sub(*a, *b)) <= 1e-13);
    }
    // Directions on degree 1, of no length, and too few control points for both.
    let line = KnotVector::new(1, vec![0.0, 0.0, 1.0, 1.0]).unwrap();
    let two = [[0.0, 0.0], [1.0, 1.0]];
    assert_eq!(
        least_squares_along(&line, &two, &[0.0, 1.0], Some([1.0, 1.0]), None),
        Err(FitError::Directions { degree: 1 })
    );
    assert_eq!(
        least_squares_along(&kv, &data, &t, Some([0.0, 0.0]), None),
        Err(FitError::Direction)
    );
    let one = KnotVector::new(2, vec![0.0, 0.0, 0.0, 1.0, 1.0, 1.0]).unwrap();
    let quad: Vec<[f64; 2]> = t.iter().map(|&u| [u, u * u]).collect();
    assert_eq!(
        least_squares_along(&one, &quad, &t, Some([1.0, 0.0]), Some([1.0, 2.0])),
        Err(FitError::TooFewControlPoints {
            count: 3,
            needed: 4
        })
    );
}

/// A source that is not finite.
struct Wild;
impl Form for Wild {
    fn at<T: Trig>(&self, t: T) -> [T; 3] {
        [t, T::lift(f64::NAN), T::lift(0.0)]
    }
}

/// Hostile aims and sources are refused with a typed cause, never a panic; the budget and the
/// stop flag end the search with their own errors.
#[test]
fn hostile_input_is_refused_never_a_panic() {
    let source = through(&analytic(Circle, 0.0, 2.0), 30);
    for within in [0.0, -1.0, f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        let r = fit(&source, within, 3, None, None);
        assert!(matches!(r, Err(FitError::Aim { .. })), "{within}: {r:?}");
    }
    for degree in [0, crate::MAX_DEGREE + 1, usize::MAX] {
        assert_eq!(
            fit(&source, 1e-3, degree, None, None),
            Err(FitError::Degree { degree })
        );
    }
    let d = [0.0, 1.0, 0.0];
    assert_eq!(
        fit(&source, 1e-3, 1, Some(d), None),
        Err(FitError::Directions { degree: 1 })
    );
    for bad in [
        [0.0; 3],
        [f64::NAN, 0.0, 0.0],
        [0.0, f64::INFINITY, 0.0],
        [0.0, 0.0, f64::NEG_INFINITY],
    ] {
        assert_eq!(
            fit(&source, 1e-3, 3, Some(bad), None),
            Err(FitError::Direction)
        );
        assert_eq!(
            fit(&source, 1e-3, 3, None, Some(bad)),
            Err(FitError::Direction)
        );
    }
    let (s, e) = end_legs(&source);
    assert_eq!(
        fit(&source, 1e-3, 3, Some(s.map(|x| -x)), None),
        Err(FitError::StartAgainst)
    );
    assert_eq!(
        fit(&source, 1e-3, 3, None, Some(e.map(|x| -x))),
        Err(FitError::EndAgainst)
    );
    // A domain of no length, or too short to sample.
    let next = f64::from_bits(1.0_f64.to_bits() + 1);
    for (lo, hi) in [(1.0, 1.0), (1.0, next)] {
        let r = fit(&analytic(Circle, lo, hi), 1e-3, 3, None, None);
        assert!(matches!(r, Err(FitError::Domain { .. })), "{r:?}");
    }
    // A source not finite.
    assert!(matches!(
        fit(&analytic(Wild, 0.0, 1.0), 1e-3, 3, None, None),
        Err(FitError::SourceNotFinite { .. })
    ));
    // The budget and the stop flag.
    let aim = FitAim {
        within: 1e-9,
        degree: 3,
        start: None,
        end: None,
    };
    let circle = analytic(Circle, 0.0, 2.0);
    let r = fit_within(&circle, &aim, &mut Budget::new(100), &Stop::new());
    assert_eq!(
        r,
        Err(FitError::Exhausted(scx_num::budget::Exhausted {
            limit: 100
        }))
    );
    let stop = Stop::new();
    stop.set();
    let r = fit_within(&circle, &aim, &mut Budget::unlimited(), &stop);
    assert!(matches!(r, Err(FitError::Stopped(_))), "{r:?}");
    // The ends of the range: any bound is within the largest aim, so one fit.
    let loose = fit(&source, f64::MAX, 3, None, None).unwrap();
    assert_eq!((loose.fits, loose.ended), (1, Ended::Within));
    for e in [
        FitError::Aim { within: 0.0 },
        FitError::Reaches,
        FitError::Direction,
    ] {
        assert_eq!(e.kind(), scx_num::error::ErrorKind::Refused);
        assert!(!e.name().is_empty() && !e.to_string().is_empty());
    }
}
