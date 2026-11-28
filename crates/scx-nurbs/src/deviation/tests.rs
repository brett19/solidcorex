// Written for solidcorex. Apache-2.0.
//! The pin's `crv_deviation` tests that assert truth, the rational ones, and the circle's
//! closed form (77 §8).

use super::*;
use crate::{Approach, KnotVector};

type Curve = NurbsCurve<3>;

fn clamped_uniform(n: usize, p: usize) -> Vec<f64> {
    let inner = n - p;
    let mut k = vec![0.0; p + 1];
    k.extend((1..inner).map(|j| j as f64 / inner as f64));
    k.extend(vec![1.0; p + 1]);
    k
}

fn spline(points: &[[f64; 3]], p: usize) -> Curve {
    let k = KnotVector::new(p, clamped_uniform(points.len(), p)).unwrap();
    NurbsCurve::new(k, points.to_vec(), None).unwrap()
}

const WAVE: [[f64; 3]; 6] = [
    [0.0, 0.0, 0.0],
    [1.0, 2.0, 0.0],
    [2.0, -1.0, 0.0],
    [3.0, 3.0, 0.0],
    [4.0, 0.0, 1.0],
    [5.0, 1.0, 0.0],
];

fn at(c: &Curve, t: f64) -> [f64; 3] {
    c.point(t, Approach::Above).unwrap()
}

fn dist(a: [f64; 3], b: [f64; 3]) -> f64 {
    a.iter()
        .zip(&b)
        .map(|(x, y)| (x - y) * (x - y))
        .sum::<f64>()
        .sqrt()
}

/// The largest sampled same-parameter distance.
fn sampled(a: &Curve, b: &Curve, n: usize) -> f64 {
    (0..=n)
        .map(|i| {
            let t = i as f64 / n as f64;
            dist(at(a, t), at(b, t))
        })
        .fold(0.0, f64::max)
}

#[test]
fn a_curve_is_no_distance_from_itself_its_refinements_or_elevations() {
    let c = spline(&WAVE, 3);
    let floor = distance_floor(&c).unwrap();
    let d = distance(&c, &c).unwrap();
    assert!(d >= floor && d < 1e-12 && d <= 2.0 * floor, "{d} {floor}");
    let more = c.insert_knot(0.3, 2).unwrap();
    let higher = c.elevate(2).unwrap();
    for other in [&more, &higher] {
        for d in [distance(&c, other).unwrap(), distance(other, &c).unwrap()] {
            assert!(d < 1e-12, "{d}");
            assert!(
                d >= distance_floor(&c)
                    .unwrap()
                    .min(distance_floor(other).unwrap())
            );
        }
    }
    // the bound is at most a few floors: its excess is rounding in the gap alone
    let d = distance(&c, &more).unwrap();
    assert!(d <= 2.0 * floor, "{d} against the floor {floor}");
}

#[test]
fn a_moved_control_point_is_bounded_by_its_move_and_holds_every_sampled_gap() {
    let c = spline(&WAVE, 3);
    let mut moved = WAVE;
    moved[2][2] += 1.0;
    let m = spline(&moved, 3);
    let d = distance(&c, &m).unwrap();
    assert!(d <= 1.0 + 1e-12, "{d}");
    let truth = sampled(&c, &m, 2000);
    assert!(d >= truth && d <= 1.0 + 1e-12, "{d} against {truth}");
}

#[test]
fn curves_of_other_degrees_and_knots_are_compared_on_common_pieces() {
    let polyline = spline(&[0.0, 1.0, 2.0, 3.0, 4.0, 5.0].map(|x| [x, 0.0, 0.0]), 1);
    let line = spline(&[[0.0; 3], [5.0, 0.0, 0.0]], 1);
    assert!(distance(&polyline, &line).unwrap() < 1e-12);
    let cubic = line.elevate(2).unwrap();
    assert!(distance(&polyline, &cubic).unwrap() < 1e-12);
    let bent = spline(
        &[[0.0; 3], [2.5, 4.0, 0.0], [5.0; 3].map(|x| x * 0.0 + x)],
        2,
    );
    let _ = bent;
    let bent = spline(&[[0.0; 3], [2.5, 4.0, 0.0], [5.0, 0.0, 0.0]], 2);
    let d = distance(&polyline, &bent).unwrap();
    // the true gap at the middle is 2 (the quadratic reaches y = 2 there)
    assert!(d >= 2.0, "{d}");
}

#[test]
fn a_periodic_curves_ends_are_read_through_the_general_basis() {
    let square = [
        [0.0, 0.0, 0.0],
        [10.0, 0.0, 0.0],
        [10.0, 10.0, 0.0],
        [0.0, 10.0, 0.0],
    ];
    let pts: Vec<[f64; 3]> = square.iter().chain(&square[..3]).copied().collect();
    let knots: Vec<f64> = (0..11).map(|i| f64::from(i - 3) / 4.0).collect();
    let closed = NurbsCurve::new(KnotVector::new(3, knots).unwrap(), pts, None).unwrap();
    let start = [50.0 / 6.0, 10.0 / 6.0, 0.0];
    let d = interpolation_bound(&closed, &[0.0, 1.0], &[start, start]).unwrap();
    assert!(d < 1e-12, "{d}");
}

#[test]
fn an_interpolants_bound_holds_its_misses_and_counts_them() {
    let c = spline(&WAVE, 3);
    let params = [0.0, 0.25, 0.5, 1.0];
    let on: Vec<[f64; 3]> = params.iter().map(|&u| at(&c, u)).collect();
    assert!(interpolation_bound(&c, &params, &on).unwrap() < 1e-12);
    let mut off = on.clone();
    off[2][2] += 0.5;
    let d = interpolation_bound(&c, &params, &off).unwrap();
    assert!((0.5..0.5 + 1e-12).contains(&d), "{d}");
    assert_eq!(
        interpolation_bound(&c, &params[..3], &on),
        Err(DeviationError::Count {
            params: 3,
            points: 4
        })
    );
    let outside = interpolation_bound(&c, &[1.5], &[[0.0; 3]]).unwrap_err();
    assert!(matches!(
        outside,
        DeviationError::Curve(NurbsError::ParameterOutside { .. })
    ));
}

#[test]
fn numbers_not_finite_are_refused_not_dropped() {
    let c = spline(&WAVE, 3);
    let on = [at(&c, 0.0), at(&c, 0.5), at(&c, 1.0)];
    assert_eq!(
        interpolation_bound(&c, &[0.0, f64::NAN, 1.0], &on),
        Err(DeviationError::NotFinite)
    );
    let mut bad = on;
    bad[1][1] = f64::NAN;
    assert_eq!(
        interpolation_bound(&c, &[0.0, 0.5, 1.0], &bad),
        Err(DeviationError::NotFinite)
    );
    // a curve with a NaN point cannot be built
    let mut w = WAVE;
    w[2][0] = f64::NAN;
    let k = KnotVector::new(3, clamped_uniform(6, 3)).unwrap();
    assert!(NurbsCurve::new(k, w.to_vec(), None).is_err());
}

#[test]
fn curves_too_far_apart_for_the_bound_overflow_by_name() {
    let a = spline(&[[8e307, 8e307, 0.0], [8e307, 8e307, 1.0]], 1);
    let b = spline(&[[-8e307, -8e307, 0.0], [-8e307, -8e307, 1.0]], 1);
    let e = distance(&a, &b).unwrap_err();
    assert_eq!(e, DeviationError::BoundOverflows);
    assert_eq!(e.kind(), ErrorKind::Unsupported);
}

#[test]
fn a_gap_near_the_bottom_of_the_floating_range_is_not_lost_to_underflow() {
    let line = spline(&[[0.0; 3], [1e-200, 0.0, 0.0]], 1);
    let d = interpolation_bound(&line, &[0.0], &[[0.0, 1e-200, 0.0]]).unwrap();
    assert!(d >= 1e-200, "{d}");
    let beside = spline(&[[0.0, 1e-200, 0.0], [1e-200, 1e-200, 0.0]], 1);
    let d = distance(&line, &beside).unwrap();
    assert!((1e-200..=1.0001e-200).contains(&d), "{d}");
}

#[test]
fn the_bound_follows_the_curves_size_not_their_place() {
    let far: Vec<[f64; 3]> = WAVE.iter().map(|&[x, y, z]| [x + 1e9, y, z]).collect();
    let c = spline(&far, 3);
    assert!(distance(&c, &c).unwrap() < 1e-9);
    let higher = c.elevate(1).unwrap();
    // the elevation itself rounds at 1e9 (u 1e9 is about 1e-7)
    assert!(distance(&c, &higher).unwrap() < 1e-5);
}

#[test]
fn other_domains_are_refused_and_rational_ones_are_not() {
    let c = spline(&WAVE, 3);
    let k = KnotVector::new(3, clamped_uniform(6, 3).iter().map(|x| 2.0 * x).collect()).unwrap();
    let other = NurbsCurve::new(k, WAVE.to_vec(), None).unwrap();
    assert!(matches!(
        distance(&c, &other),
        Err(DeviationError::Domains { .. })
    ));
    let unit = NurbsCurve::new(
        KnotVector::new(3, clamped_uniform(6, 3)).unwrap(),
        WAVE.to_vec(),
        Some(vec![1.0; 6]),
    )
    .unwrap();
    // the rational bound counts the products of the numerators: a size-squared term
    assert!(distance(&c, &unit).unwrap() < 1e-10);
    assert_eq!(distance_floor(&unit), Err(DeviationError::RationalFloor));
}

fn pairs() -> Vec<(Curve, Curve)> {
    let c = spline(&WAVE, 3);
    let mut moved = WAVE;
    moved[2][2] += 1.0;
    let m = spline(&moved, 3);
    let far: Vec<[f64; 3]> = WAVE.iter().map(|&[x, y, z]| [x + 1e9, y, z]).collect();
    let quad = spline(&WAVE, 2);
    let quartic = spline(
        &[
            WAVE[0], WAVE[3], WAVE[1], WAVE[5], WAVE[2], WAVE[4], WAVE[0],
        ],
        4,
    );
    vec![
        (c.clone(), m.clone()),
        (c.clone(), c.insert_knot(0.3, 2).unwrap()),
        (spline(&far, 3), spline(&far, 3)),
        (c.clone(), quad),
        (quartic, c.clone()),
        (m, c.elevate(1).unwrap()),
    ]
}

#[test]
fn the_largest_span_bound_is_the_distance_bit_for_bit() {
    for (a, b) in pairs() {
        let spans = distance_by_span(&a, &b).unwrap();
        let largest = spans.iter().map(|s| s.bound).fold(0.0, f64::max);
        assert_eq!(distance(&a, &b).unwrap().to_bits(), largest.to_bits());
    }
}

#[test]
fn the_spans_are_the_union_of_the_breakpoints_in_order() {
    let a = spline(&WAVE, 3); // breakpoints 1/3, 2/3
    let b = a
        .insert_knot(0.5, 1)
        .unwrap()
        .insert_knot(1.0 / 3.0, 1)
        .unwrap();
    let spans = distance_by_span(&a, &b).unwrap();
    let ends: Vec<(f64, f64)> = spans.iter().map(|s| (s.from, s.to)).collect();
    assert_eq!(
        ends,
        vec![
            (0.0, 1.0 / 3.0),
            (1.0 / 3.0, 0.5),
            (0.5, 2.0 / 3.0),
            (2.0 / 3.0, 1.0)
        ]
    );
}

#[test]
fn each_span_bound_holds_the_gaps_sampled_in_its_span() {
    for (a, b) in pairs() {
        for s in distance_by_span(&a, &b).unwrap() {
            for i in 0..=100 {
                let t = s.from + (s.to - s.from) * f64::from(i) / 100.0;
                let g = dist(at(&a, t), at(&b, t));
                assert!(g <= s.bound, "at {t}: {g} past {}", s.bound);
            }
        }
    }
}

#[test]
fn no_bound_is_below_the_floor_of_the_first_curve() {
    for (a, b) in pairs() {
        let floor = distance_floor(&a).unwrap();
        assert!(floor >= 0.0 && floor.is_finite());
        for s in distance_by_span(&a, &b).unwrap() {
            assert!(s.bound >= floor, "{} below {floor}", s.bound);
        }
    }
    // the floor follows the curve's size, not its place
    let near = distance_floor(&spline(&WAVE, 3)).unwrap();
    let far: Vec<[f64; 3]> = WAVE.iter().map(|&[x, y, z]| [x + 1e9, y, z]).collect();
    assert!(distance_floor(&spline(&far, 3)).unwrap() > 0.0);
    let shifted: Vec<[f64; 3]> = WAVE.iter().map(|&[x, y, z]| [x + 4.0, y, z]).collect();
    assert_eq!(
        distance_floor(&spline(&shifted, 3)).unwrap().to_bits(),
        near.to_bits()
    );
}

#[test]
fn spans_outside_a_moved_control_points_support_are_at_rounding_level() {
    let c = spline(&WAVE, 3);
    let mut moved = WAVE;
    moved[5][1] += 1.0; // the last point: its support is the last span only
    let m = spline(&moved, 3);
    let spans = distance_by_span(&c, &m).unwrap();
    assert!(spans.first().is_some_and(|s| s.bound < 1e-12));
    assert!(spans.last().is_some_and(|s| s.bound > 0.9));
}

/// The quarter circle of radius 1 as a quadratic rational Bezier.
fn quarter() -> Curve {
    NurbsCurve::new(
        KnotVector::new(2, vec![0.0, 0.0, 0.0, 1.0, 1.0, 1.0]).unwrap(),
        vec![[1.0, 0.0, 0.0], [1.0, 1.0, 0.0], [0.0, 1.0, 0.0]],
        Some(vec![1.0, 0.5_f64.sqrt(), 1.0]),
    )
    .unwrap()
}

/// The cubic Bezier through `c` at `t = 0, 1/3, 2/3, 1` (a non-rational fit).
fn cubic_through(c: &Curve) -> Curve {
    let ts = [0.0, 1.0 / 3.0, 2.0 / 3.0, 1.0];
    let b = |i: usize, t: f64| {
        let binom = [1.0, 3.0, 3.0, 1.0][i];
        binom * scx_num::fmath::powi(t, i as i32) * scx_num::fmath::powi(1.0 - t, 3 - i as i32)
    };
    // solve the 4 x 4 system by elimination, one right-hand side per coordinate
    let mut m: Vec<Vec<f64>> = ts
        .iter()
        .map(|&t| {
            let mut row: Vec<f64> = (0..4).map(|i| b(i, t)).collect();
            row.extend(at(c, t));
            row
        })
        .collect();
    for col in 0..4 {
        let piv = (col..4)
            .max_by(|&x, &y| m[x][col].abs().total_cmp(&m[y][col].abs()))
            .unwrap();
        m.swap(col, piv);
        for r in 0..4 {
            if r != col {
                let f = m[r][col] / m[col][col];
                for k in col..7 {
                    let v = m[col][k];
                    m[r][k] -= f * v;
                }
            }
        }
    }
    let pts: Vec<[f64; 3]> = (0..4)
        .map(|i| [m[i][4] / m[i][i], m[i][5] / m[i][i], m[i][6] / m[i][i]])
        .collect();
    NurbsCurve::new(
        KnotVector::new(3, vec![0.0, 0.0, 0.0, 0.0, 1.0, 1.0, 1.0, 1.0]).unwrap(),
        pts,
        None,
    )
    .unwrap()
}

#[test]
fn a_circles_rational_form_against_a_cubic_fit_is_bounded_by_the_radial_error() {
    let arc = quarter();
    let fit = cubic_through(&arc);
    // closed form: |C(t)| = 1 on the arc, so the radial error is max ||fit(t)| - 1|
    let radial = (0..=4000)
        .map(|i| {
            let p = at(&fit, f64::from(i) / 4000.0);
            ((p[0] * p[0] + p[1] * p[1]).sqrt() - 1.0).abs()
        })
        .fold(0.0, f64::max);
    assert!(radial > 1e-5 && radial < 1e-2, "{radial}");
    let truth = sampled(&arc, &fit, 4000);
    for (a, b) in [(&arc, &fit), (&fit, &arc)] {
        let d = distance(a, b).unwrap();
        assert!(d >= truth, "{d} below the sampled distance {truth}");
        assert!(d >= radial, "{d} below the radial error {radial}");
        assert!(d <= 8.0 * radial, "{d} against the radial error {radial}");
    }
}

#[test]
fn a_rational_curve_is_no_distance_from_its_refinements_and_elevations() {
    let arc = quarter();
    assert!(distance(&arc, &arc).unwrap() < 1e-12);
    for other in [
        arc.insert_knot(0.4, 1).unwrap(),
        arc.elevate(1).unwrap(),
        arc.elevate(2).unwrap(),
    ] {
        for d in [
            distance(&arc, &other).unwrap(),
            distance(&other, &arc).unwrap(),
        ] {
            assert!(d < 1e-12, "{d}");
        }
    }
}

#[test]
fn a_moved_rational_control_point_is_bounded_and_holds_every_sampled_gap() {
    let arc = quarter();
    let mut pts = arc.points().to_vec();
    pts[1][1] += 0.25;
    let moved = NurbsCurve::new(
        arc.knot_vector().clone(),
        pts,
        arc.weights().map(<[f64]>::to_vec),
    )
    .unwrap();
    let truth = sampled(&arc, &moved, 4000);
    let d = distance(&arc, &moved).unwrap();
    assert!(d >= truth, "{d} below {truth}");
    // the move is 0.25 of the point; its basis function is at most the weight share
    assert!(
        d <= 0.25 / 0.5_f64.sqrt() + 1e-9 && d <= 6.0 * truth,
        "{d} against {truth}"
    );
    for s in distance_by_span(&arc, &moved).unwrap() {
        assert!(s.bound >= 0.0);
    }
}

#[test]
fn weights_within_the_rounding_of_zero_are_refused() {
    let k = KnotVector::new(2, vec![0.0, 0.0, 0.0, 1.0, 1.0, 1.0]).unwrap();
    let pts = vec![[1.0, 0.0, 0.0], [1.0, 1.0, 0.0], [0.0, 1.0, 0.0]];
    let tiny = NurbsCurve::new(k, pts, Some(vec![1.0, 1e-300, 1.0])).unwrap();
    assert_eq!(distance(&tiny, &tiny), Err(DeviationError::WeightsLost));
}

#[test]
fn the_work_is_linear_in_the_spans() {
    // a closed-form cubic on n spans against its refinement: the spans are the union
    for n in [4_usize, 40, 400] {
        let pts: Vec<[f64; 3]> = (0..n + 3)
            .map(|i| [i as f64, scx_num::fmath::sin(i as f64 * 0.7), 0.0])
            .collect();
        let a = spline(&pts, 3);
        let b = a.insert_knot(0.5 / n as f64, 1).unwrap();
        assert_eq!(distance_by_span(&a, &b).unwrap().len(), n + 1);
    }
}

#[test]
fn error_names_and_kinds() {
    assert_eq!(DeviationError::NotFinite.name(), "not_finite");
    assert_eq!(DeviationError::Defect { why: "" }.kind(), ErrorKind::Defect);
    assert_eq!(
        DeviationError::from(NurbsError::Unstable).kind(),
        ErrorKind::Unsupported
    );
}
