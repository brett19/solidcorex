//! FT2's tests (77 §8): the truth is the closed form (a polynomial, a curve's own control
//! points and derivatives), never the pin's output.

use super::*;
use crate::{Approach, KnotVector, NurbsCurve};
use scx_num::fmath;
use scx_num::hash::Rng;

/// A uniform random value in `[lo, hi)`.
fn between(rng: &mut Rng, lo: f64, hi: f64) -> f64 {
    lo + (hi - lo) * rng.next_f64()
}

/// Parameters `0 = t[0] < ... < t[n] = 1`, irregularly spaced (gaps within a factor 4).
fn spaced(rng: &mut Rng, count: usize) -> Vec<f64> {
    let gaps: Vec<f64> = (1..count).map(|_| between(rng, 1.0, 4.0)).collect();
    let total: f64 = gaps.iter().sum();
    let mut t = vec![0.0];
    let mut run = 0.0;
    for g in &gaps[..gaps.len() - 1] {
        run += g;
        t.push(run / total);
    }
    t.push(1.0);
    t
}

/// A vector polynomial `sum c[k] t^k` and its derivatives, by the closed form.
struct Poly<const N: usize> {
    c: Vec<[f64; N]>,
}

impl<const N: usize> Poly<N> {
    fn random(rng: &mut Rng, degree: usize) -> Self {
        Self {
            c: (0..=degree)
                .map(|_| core::array::from_fn(|_| rng.next_signed_unit()))
                .collect(),
        }
    }

    /// The `order`-th derivative at `t`.
    fn at(&self, t: f64, order: usize) -> [f64; N] {
        let mut out = [0.0; N];
        for (k, c) in self.c.iter().enumerate().skip(order) {
            let mut factor = 1.0;
            for j in 0..order {
                factor *= (k - j) as f64;
            }
            let power = (0..k - order).fold(1.0, |a, _| a * t);
            for (o, x) in out.iter_mut().zip(c) {
                *o += factor * power * x;
            }
        }
        out
    }
}

fn close<const N: usize>(a: [f64; N], b: [f64; N], tol: f64) -> bool {
    a.iter()
        .zip(&b)
        .all(|(x, y)| (x - y).abs() <= tol * (1.0 + x.abs().max(y.abs())))
}

#[test]
fn polynomials_of_degree_at_most_p_come_back() {
    let mut rng = Rng::new(0x05ee_df12);
    for p in 1..=6 {
        for d in 0..=p {
            for count in [p + 1, p + 2, 2 * p + 3, 40] {
                let poly = Poly::<3>::random(&mut rng, d);
                let t = spaced(&mut rng, count);
                let data: Vec<[f64; 3]> = t.iter().map(|&u| poly.at(u, 0)).collect();
                let curve = interpolate(&data, &t, p, &End::Point, &End::Point)
                    .unwrap()
                    .into_curve()
                    .unwrap();
                for i in 0..=200 {
                    let u = f64::from(i) / 200.0;
                    let got = curve.point(u, Approach::Above).unwrap();
                    assert!(
                        close(got, poly.at(u, 0), 2e-11),
                        "p {p} d {d} count {count} u {u}: {got:?} vs {:?}",
                        poly.at(u, 0)
                    );
                }
            }
        }
    }
}

#[test]
fn polynomials_with_their_own_end_derivatives_come_back() {
    let mut rng = Rng::new(77);
    for p in 2..=6usize {
        for orders in [(1, 0), (0, 1), (1, 1), (2, 1), (1, 2), (2, 2)] {
            if orders.0.max(orders.1) >= p {
                continue;
            }
            let poly = Poly::<2>::random(&mut rng, p);
            let t = spaced(&mut rng, 3 * p);
            let data: Vec<[f64; 2]> = t.iter().map(|&u| poly.at(u, 0)).collect();
            let cond = |order: usize, at: f64| match order {
                0 => End::Point,
                1 => End::Tangent { d1: poly.at(at, 1) },
                _ => End::Curvature {
                    d1: poly.at(at, 1),
                    d2: poly.at(at, 2),
                },
            };
            let curve = interpolate(&data, &t, p, &cond(orders.0, 0.0), &cond(orders.1, 1.0))
                .unwrap()
                .into_curve()
                .unwrap();
            for i in 0..=100 {
                let u = f64::from(i) / 100.0;
                let got = curve.point(u, Approach::Above).unwrap();
                assert!(close(got, poly.at(u, 0), 5e-10), "p {p} {orders:?} u {u}");
            }
        }
    }
}

/// The derivatives of `curve` at both ends.
fn end_derivs(curve: &NurbsCurve<3>) -> ([[f64; 3]; 4], [[f64; 3]; 4]) {
    let (lo, hi) = curve.knot_vector().ends();
    (
        curve.derivs::<4>(lo, Approach::Above).unwrap(),
        curve.derivs::<4>(hi, Approach::Below).unwrap(),
    )
}

#[test]
fn end_tangents_curvatures_and_hermite_are_met() {
    let mut rng = Rng::new(1234);
    let ends = |rng: &mut Rng, order: usize| -> End<3> {
        let v = |rng: &mut Rng, s: f64| -> [f64; 3] {
            core::array::from_fn(|_| s * rng.next_signed_unit())
        };
        match order {
            0 => End::Point,
            1 => End::Tangent { d1: v(rng, 3.0) },
            2 => End::Curvature {
                d1: v(rng, 3.0),
                d2: v(rng, 20.0),
            },
            _ => End::Hermite {
                d1: v(rng, 3.0),
                d2: v(rng, 20.0),
                d3: v(rng, 100.0),
            },
        }
    };
    for p in [4usize, 5, 7] {
        for (a, b) in [(1, 1), (2, 2), (3, 3), (3, 0), (0, 2), (1, 3)] {
            for count in [6, 9, 25] {
                let t = spaced(&mut rng, count);
                let data = points(&mut rng, count);
                let (s, e) = (ends(&mut rng, a), ends(&mut rng, b));
                let curve = interpolate(&data, &t, p, &s, &e)
                    .unwrap()
                    .into_curve()
                    .unwrap();
                let (d0, dn) = end_derivs(&curve);
                for (k, want) in s.derivatives().iter().take(a).enumerate() {
                    assert!(
                        close(d0[k + 1], *want, 1e-8),
                        "p {p} start order {} {:?}",
                        k + 1,
                        (a, b)
                    );
                }
                for (k, want) in e.derivatives().iter().take(b).enumerate() {
                    assert!(
                        close(dn[k + 1], *want, 1e-8),
                        "p {p} end order {} {:?}",
                        k + 1,
                        (a, b)
                    );
                }
                let (lo, hi) = curve.knot_vector().ends();
                assert!(close(
                    curve.point(lo, Approach::Above).unwrap(),
                    data[0],
                    1e-12
                ));
                for (u, q) in t.iter().zip(&data) {
                    let got = curve.point(*u, Approach::Above).unwrap();
                    assert!(close(got, *q, 1e-10), "p {p} {:?} at {u}", (a, b));
                }
                assert!(close(
                    curve.point(hi, Approach::Below).unwrap(),
                    data[count - 1],
                    1e-12
                ));
            }
        }
    }
}

fn points(rng: &mut Rng, count: usize) -> Vec<[f64; 3]> {
    (0..count)
        .map(|_| core::array::from_fn(|_| rng.next_signed_unit()))
        .collect()
}

/// A curve of degree `p` with `count` control points on random clamped knots, and samples of it
/// at `3 * count` irregular parameters.
fn random_curve(rng: &mut Rng, p: usize, count: usize) -> NurbsCurve<3> {
    let spans = count - p;
    let mut knots = vec![0.0; p + 1];
    let interior = spaced(rng, spans + 1);
    knots.extend_from_slice(&interior[1..spans]);
    knots.extend(vec![1.0; p + 1]);
    NurbsCurve::new(KnotVector::new(p, knots).unwrap(), points(rng, count), None).unwrap()
}

/// `m + 1` parameters on `[0, 1]`: the ends and `m - 1` random interior ones, sorted.
fn dense(rng: &mut Rng, m: usize) -> Vec<f64> {
    spaced(rng, m + 1)
}

fn max_diff(a: &[[f64; 3]], b: &[[f64; 3]]) -> f64 {
    a.iter()
        .zip(b)
        .flat_map(|(x, y)| x.iter().zip(y).map(|(u, v)| (u - v).abs()))
        .fold(0.0, f64::max)
}

#[test]
fn least_squares_of_exact_data_on_its_own_knots_gives_the_curve_back() {
    let mut rng = Rng::new(99);
    for p in 1..=6 {
        for count in [p + 1, p + 3, 12, 30] {
            let curve = random_curve(&mut rng, p, count);
            let t = dense(&mut rng, 4 * count);
            let data: Vec<[f64; 3]> = t
                .iter()
                .map(|&u| curve.point(u, Approach::Above).unwrap())
                .collect();
            let got =
                least_squares(curve.knot_vector(), &data, &t, &End::Point, &End::Point).unwrap();
            let err = max_diff(got.points(), curve.points());
            assert!(err < 1e-10, "p {p} count {count}: {err}");
            // Weighted: any positive weights give the same answer for exact data.
            let w: Vec<f64> = t.iter().map(|_| between(&mut rng, 0.01, 100.0)).collect();
            let got = weighted_least_squares(
                curve.knot_vector(),
                &data,
                &t,
                &w,
                &End::Point,
                &End::Point,
            )
            .unwrap();
            let err = max_diff(got.points(), curve.points());
            assert!(err < 1e-9, "weighted p {p} count {count}: {err}");
        }
    }
}

#[test]
fn least_squares_holds_end_derivatives() {
    let mut rng = Rng::new(5150);
    for p in [3usize, 4, 6] {
        let curve = random_curve(&mut rng, p, 14);
        let t = dense(&mut rng, 60);
        let data: Vec<[f64; 3]> = t
            .iter()
            .map(|&u| curve.point(u, Approach::Above).unwrap())
            .collect();
        let (d0, dn) = end_derivs(&curve);
        let s = End::Curvature {
            d1: d0[1],
            d2: d0[2],
        };
        let e = if p >= 4 {
            End::Hermite {
                d1: dn[1],
                d2: dn[2],
                d3: dn[3],
            }
        } else {
            End::Tangent { d1: dn[1] }
        };
        let got = least_squares(curve.knot_vector(), &data, &t, &s, &e).unwrap();
        assert!(max_diff(got.points(), curve.points()) < 1e-8, "p {p}");
        // Perturbed data: the held conditions still hold exactly.
        let noisy: Vec<[f64; 3]> = data
            .iter()
            .enumerate()
            .map(|(i, q)| {
                if i == 0 || i == data.len() - 1 {
                    *q
                } else {
                    q.map(|x| x + 1e-2 * rng.next_signed_unit())
                }
            })
            .collect();
        let fit = least_squares(curve.knot_vector(), &noisy, &t, &s, &e)
            .unwrap()
            .into_curve()
            .unwrap();
        let (f0, fn_) = end_derivs(&fit);
        assert!(
            close(f0[1], d0[1], 1e-9) && close(f0[2], d0[2], 1e-8),
            "p {p} start"
        );
        assert!(close(fn_[1], dn[1], 1e-9), "p {p} end");
    }
}

#[test]
fn least_squares_is_least() {
    // The residual of the fit is no larger than that of every perturbed control polygon (the
    // fit is a minimum of a convex quadratic), and its normal equations vanish.
    let mut rng = Rng::new(31);
    let (p, count) = (3usize, 9usize);
    let t = dense(&mut rng, 50);
    let data: Vec<[f64; 3]> = t.iter().map(|&u| [u, fmath::sin(7.0 * u), u * u]).collect();
    let knots = approximation(&t, p, count).unwrap();
    let fit = least_squares(&knots, &data, &t, &End::Point, &End::Point).unwrap();
    let residual = |points: &[[f64; 3]]| -> f64 {
        let c = NurbsCurve::new(knots.clone(), points.to_vec(), None).unwrap();
        t.iter()
            .zip(&data)
            .map(|(&u, q)| {
                let x = c.point(u, Approach::Above).unwrap();
                (0..3)
                    .map(|i| {
                        let d = x[i] - q[i];
                        d * d
                    })
                    .sum::<f64>()
            })
            .sum()
    };
    let best = residual(fit.points());
    for _ in 0..200 {
        let mut pts = fit.points().to_vec();
        let i = 1 + (rng.next_u64() % (count as u64 - 2)) as usize;
        for x in &mut pts[i] {
            *x += 1e-3 * rng.next_signed_unit();
        }
        assert!(residual(&pts) >= best * (1.0 - 1e-12));
    }
    // A polynomial of degree <= p is in the space of the approximation knots.
    let poly = Poly::<3>::random(&mut rng, p);
    let samples: Vec<[f64; 3]> = t.iter().map(|&u| poly.at(u, 0)).collect();
    let curve = least_squares(&knots, &samples, &t, &End::Point, &End::Point)
        .unwrap()
        .into_curve()
        .unwrap();
    for i in 0..=100 {
        let u = f64::from(i) / 100.0;
        assert!(close(
            curve.point(u, Approach::Above).unwrap(),
            poly.at(u, 0),
            1e-10
        ));
    }
}

#[test]
fn parameters_are_as_defined() {
    let pts = [[0.0, 0.0], [3.0, 4.0], [3.0, 14.0], [3.0, 14.0]];
    assert_eq!(
        parameters(&pts, Param::Uniform).unwrap(),
        vec![0.0, 1.0 / 3.0, 2.0 / 3.0, 1.0]
    );
    let chord = parameters(&pts, Param::ChordLength).unwrap();
    assert_eq!(chord, vec![0.0, 5.0 / 15.0, 1.0, 1.0]);
    let cent = parameters(&pts, Param::Centripetal).unwrap();
    let total = 5f64.sqrt() + 10f64.sqrt();
    assert!((cent[1] - 5f64.sqrt() / total).abs() < 1e-16);
    let same = [[1.0, 2.0]; 3];
    assert_eq!(
        parameters(&same, Param::ChordLength),
        Err(FitError::ZeroLength)
    );
    assert!(parameters(&same, Param::Uniform).is_ok());
    assert!(matches!(
        parameters(&[[0.0]], Param::Uniform),
        Err(FitError::TooFewPoints { .. })
    ));
    assert_eq!(
        parameters(&[[0.0], [f64::NAN]], Param::Uniform),
        Err(FitError::NotFinite { index: 1 })
    );
}

#[test]
fn averaging_follows_the_book() {
    let t = [0.0, 0.25, 0.5, 0.75, 1.0];
    let k = averaging(&t, 3, 0, 0).unwrap();
    assert_eq!(k.knots(), [0.0, 0.0, 0.0, 0.0, 0.5, 1.0, 1.0, 1.0, 1.0]);
    // One derivative at each end (§9.2.2): three interior knots.
    let k = averaging(&t, 3, 1, 1).unwrap();
    assert_eq!(
        k.knots(),
        [0.0, 0.0, 0.0, 0.0, 0.25, 0.5, 0.75, 1.0, 1.0, 1.0, 1.0]
    );
    assert_eq!(k.point_count(), 7);
    // Approximation: 5 points of 9 parameters -> interior knots at the interpolated spacing.
    let u: Vec<f64> = (0..9).map(|i| f64::from(i) / 8.0).collect();
    let k = approximation(&u, 3, 6).unwrap();
    assert_eq!(k.point_count(), 6);
    for (got, want) in k
        .knots()
        .iter()
        .zip([0.0, 0.0, 0.0, 0.0, 0.25, 0.625, 1.0, 1.0, 1.0, 1.0])
    {
        assert!((got - want).abs() < 1e-15, "{got} {want}");
    }
    assert!(k.is_clamped());
}

#[test]
fn refusals_are_typed() {
    let t = [0.0, 0.5, 1.0];
    let d = [[0.0], [1.0], [0.0]];
    assert_eq!(
        interpolate(&d, &[0.0, 0.5, 0.5], 2, &End::Point, &End::Point),
        Err(FitError::ParametersNotIncreasing { index: 2 })
    );
    assert!(matches!(
        interpolate(&d, &t, 4, &End::Point, &End::Point),
        Err(FitError::TooFewControlPoints { .. })
    ));
    let tangent = End::Tangent { d1: [1.0] };
    assert!(matches!(
        averaging(&t, 1, 1, 0),
        Err(FitError::DegreeTooLow { .. })
    ));
    assert!(interpolate(&d, &t, 2, &tangent, &End::Point).is_ok());
    assert!(matches!(
        interpolate(&d, &[0.0, 1.0], 2, &End::Point, &End::Point),
        Err(FitError::ParameterCount { .. })
    ));
    let knots = averaging(&t, 2, 0, 0).unwrap();
    assert_eq!(
        interpolate_on(&knots, &d, &[0.1, 0.5, 1.0], &End::Point, &End::Point),
        Err(FitError::EndsNotOnDomain)
    );
    assert!(matches!(
        interpolate_on(&knots, &d, &t, &tangent, &End::Point),
        Err(FitError::KnotCount { .. })
    ));
    // Fewer data than unknowns: no unique fit.
    let many = KnotVector::new(2, vec![0., 0., 0., 0.2, 0.4, 0.6, 0.8, 1., 1., 1.]).unwrap();
    assert!(matches!(
        least_squares(&many, &d, &t, &End::Point, &End::Point),
        Err(FitError::Singular { .. })
    ));
    let open = KnotVector::new(2, vec![0., 1., 2., 3., 4., 5.]).unwrap();
    assert_eq!(
        least_squares(&open, &d, &[2.0, 2.5, 3.0], &End::Point, &End::Point),
        Err(FitError::Unclamped)
    );
    assert!(matches!(
        weighted_least_squares(&knots, &d, &t, &[1.0, 0.0, 1.0], &End::Point, &End::Point),
        Err(FitError::Weight { index: 1, .. })
    ));
    assert!(matches!(
        interpolate(&d, &t, 2, &End::Tangent { d1: [f64::NAN] }, &End::Point),
        Err(FitError::NotFinite { .. })
    ));
}

#[test]
fn a_homogeneous_fit_makes_a_rational_curve() {
    // A quarter circle as a rational quadratic, its homogeneous points (w x, w y, w) sampled at
    // its own parameters and fitted, comes back.
    let w = core::f64::consts::FRAC_1_SQRT_2;
    let knots = KnotVector::new(2, vec![0., 0., 0., 1., 1., 1.]).unwrap();
    let homogeneous = NurbsCurve::new(
        knots.clone(),
        vec![[1.0, 0.0, 1.0], [w, w, w], [0.0, 1.0, 1.0]],
        None,
    )
    .unwrap();
    let t: Vec<f64> = (0..=8).map(|i| f64::from(i) / 8.0).collect();
    let data: Vec<[f64; 3]> = t
        .iter()
        .map(|&u| homogeneous.point(u, Approach::Above).unwrap())
        .collect();
    let fit = least_squares(&knots, &data, &t, &End::Point, &End::Point).unwrap();
    for (got, want) in fit
        .points()
        .iter()
        .zip([[1.0, 0.0, 1.0], [w, w, w], [0.0, 1.0, 1.0]])
    {
        assert!(close(*got, want, 1e-13), "{got:?} {want:?}");
    }
}

#[test]
fn a_twenty_thousand_point_system_is_banded() {
    // A dense collocation matrix would be 20 001^2 doubles (3.2 GB); the band is 4 per row.
    let n = 20_000;
    let poly = Poly::<3>::random(&mut Rng::new(8), 3);
    let t: Vec<f64> = (0..=n).map(|i| i as f64 / n as f64).collect();
    let data: Vec<[f64; 3]> = t.iter().map(|&u| poly.at(u, 0)).collect();
    let tangent = End::Tangent {
        d1: poly.at(0.0, 1),
    };
    let curve = interpolate(&data, &t, 3, &tangent, &End::Point)
        .unwrap()
        .into_curve()
        .unwrap();
    for i in (0..=n).step_by(997) {
        let got = curve.point(t[i], Approach::Above).unwrap();
        assert!(close(got, data[i], 1e-9), "at {i}");
    }
}
