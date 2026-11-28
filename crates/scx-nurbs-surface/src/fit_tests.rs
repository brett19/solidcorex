// Written for solidcorex. Apache-2.0.
//! Tests of the tensor fits against truth: a bilinear patch, a polynomial surface of the fitted
//! degrees, a rational cylinder, the data itself, and the Bernstein forms against the surface.

use scx_algo::bernstein::Bernstein2;
use scx_num::error::ErrorKind;
use scx_num::extent2d::Extent2d;
use scx_num::fmath;
use scx_num::hash::Rng;
use scx_nurbs::fit::{self, End, FitError, Param};
use scx_nurbs::{Approach, CompatibleCurves, KnotVector, NurbsError};
use scx_testkit::sample::{self, Knots, Spec, Weights};

use crate::fit::{
    Edge, SurfaceFitError, grid_parameters, interpolate, interpolate_on, interpolate_rows,
    interpolate_rows_on, least_squares, least_squares_rows,
};
use crate::{Net, NurbsSurface, NurbsSurfaceError, ParamAxis, RestrictCause};

const ABOVE: [Approach; 2] = [Approach::Above; 2];

fn at<const D: usize>(s: &NurbsSurface<D>, u: f64, v: f64) -> [f64; D] {
    s.point([u, v], ABOVE).unwrap()
}

fn near<const D: usize>(got: &[f64; D], want: &[f64; D], tol: f64) -> bool {
    got.iter()
        .zip(want)
        .all(|(g, w)| (g - w).abs() <= tol * (1.0 + w.abs()))
}

/// Parameters 0 = t0 < ... < tn = 1, unevenly spaced.
fn params(rng: &mut Rng, n: usize) -> Vec<f64> {
    let mut t = vec![0.0];
    let mut run = 0.0;
    let steps: Vec<f64> = (0..n).map(|_| 0.3 + rng.next_f64()).collect();
    let total: f64 = steps.iter().sum();
    for (k, s) in steps.iter().enumerate() {
        run += s;
        t.push(if k + 1 == n { 1.0 } else { run / total });
    }
    t
}

/// `sum a[i][j] u^i v^j`.
fn poly(a: &[Vec<[f64; 3]>], u: f64, v: f64) -> [f64; 3] {
    let mut out = [0.0; 3];
    for (i, row) in a.iter().enumerate() {
        for (j, c) in row.iter().enumerate() {
            let f = fmath::powi(u, i as i32) * fmath::powi(v, j as i32);
            for (o, x) in out.iter_mut().zip(c) {
                *o += x * f;
            }
        }
    }
    out
}

fn random_poly(rng: &mut Rng, p: usize, q: usize) -> Vec<Vec<[f64; 3]>> {
    (0..=p)
        .map(|_| {
            (0..=q)
                .map(|_| core::array::from_fn(|_| rng.next_signed_unit()))
                .collect()
        })
        .collect()
}

fn grid_of(f: impl Fn(f64, f64) -> [f64; 3], u: &[f64], v: &[f64]) -> Net<[f64; 3]> {
    let pts = u
        .iter()
        .flat_map(|&a| v.iter().map(move |&b| (a, b)))
        .map(|(a, b)| f(a, b))
        .collect();
    Net::new(u.len(), v.len(), pts).unwrap()
}

#[test]
fn a_bilinear_patch_is_reproduced_at_and_between_the_parameters() {
    let c = [
        [0.0, 0.0, 0.0],
        [1.0, 0.2, 2.0],
        [0.3, 1.5, -1.0],
        [2.0, 2.0, 0.5],
    ];
    let bil = |u: f64, v: f64| -> [f64; 3] {
        core::array::from_fn(|k| {
            (1.0 - u) * (1.0 - v) * c[0][k]
                + (1.0 - u) * v * c[1][k]
                + u * (1.0 - v) * c[2][k]
                + u * v * c[3][k]
        })
    };
    // The four corners alone, degrees (1, 1): the patch.
    let grid = grid_of(bil, &[0.0, 1.0], &[0.0, 1.0]);
    let s = interpolate(&grid, &[0.0, 1.0], &[0.0, 1.0], (1, 1)).unwrap();
    assert_eq!(s.points().points().len(), 4);
    let mut rng = Rng::new(1);
    for _ in 0..200 {
        let (u, v) = (rng.next_f64(), rng.next_f64());
        assert!(near(&at(&s, u, v), &bil(u, v), 1e-14));
    }
    // A finer grid at uneven parameters, any degree 1..=4: still the patch.
    for (nu, nv) in [(5, 4), (7, 9)] {
        let (u, v) = (params(&mut rng, nu), params(&mut rng, nv));
        let grid = grid_of(bil, &u, &v);
        for p in 1..=4 {
            for q in 1..=3 {
                let s = interpolate(&grid, &u, &v, (p, q)).unwrap();
                for (i, &a) in u.iter().enumerate() {
                    for (j, &b) in v.iter().enumerate() {
                        let want = grid.get(i, j).unwrap();
                        assert!(near(&at(&s, a, b), want, 1e-12), "at the data");
                    }
                }
                for _ in 0..100 {
                    let (a, b) = (rng.next_f64(), rng.next_f64());
                    assert!(near(&at(&s, a, b), &bil(a, b), 1e-11), "between");
                }
            }
        }
    }
}

#[test]
fn a_polynomial_surface_of_the_fitted_degrees_is_reproduced() {
    let mut rng = Rng::new(2);
    for p in 1..=4 {
        for q in 1..=4 {
            let a = random_poly(&mut rng, p, q);
            let f = |u, v| poly(&a, u, v);
            let (u, v) = (params(&mut rng, p + 4), params(&mut rng, q + 3));
            let s = interpolate(&grid_of(f, &u, &v), &u, &v, (p, q)).unwrap();
            for _ in 0..200 {
                let (x, y) = (rng.next_f64(), rng.next_f64());
                assert!(near(&at(&s, x, y), &f(x, y), 1e-9), "p {p} q {q}");
            }
            // The least squares of the same exact data, on net sizes with and without
            // interior knots.
            for (nu, nv) in [(p + 1, q + 1), (p + 2, q + 1), (p + 3, q + 2)] {
                let (u, v) = (params(&mut rng, 3 * nu), params(&mut rng, 3 * nv));
                let grid = grid_of(f, &u, &v);
                let s = least_squares(&grid, &u, &v, (p, q), (nu, nv)).unwrap();
                assert_eq!(s.points().nrows(), nu);
                assert_eq!(s.points().ncols(), nv);
                for _ in 0..200 {
                    let (x, y) = (rng.next_f64(), rng.next_f64());
                    assert!(near(&at(&s, x, y), &f(x, y), 1e-8), "lsq {p} {q} {nu} {nv}");
                }
            }
        }
    }
}

/// The ring of the cylinder's circle: nine rational points.
fn ring(r: f64) -> ([[f64; 3]; 9], [f64; 9]) {
    let s = core::f64::consts::FRAC_1_SQRT_2;
    let xyw = [
        (r, 0.0, 1.0),
        (r, r, s),
        (0.0, r, 1.0),
        (-r, r, s),
        (-r, 0.0, 1.0),
        (-r, -r, s),
        (0.0, -r, 1.0),
        (r, -r, s),
        (r, 0.0, 1.0),
    ];
    (
        core::array::from_fn(|i| [xyw[i].0, xyw[i].1, 0.0]),
        core::array::from_fn(|i| xyw[i].2),
    )
}

/// Circles of radius `r` at the heights `z`, as compatible rational curves.
fn circles(r: f64, z: &[f64]) -> CompatibleCurves<3> {
    let knots = KnotVector::new(
        2,
        vec![
            0.0, 0.0, 0.0, 0.25, 0.25, 0.5, 0.5, 0.75, 0.75, 1.0, 1.0, 1.0,
        ],
    )
    .unwrap();
    let (pts, w) = ring(r);
    let mut points = Vec::new();
    let mut weights = Vec::new();
    for &h in z {
        points.extend(pts.iter().map(|p| [p[0], p[1], h]));
        weights.extend_from_slice(&w);
    }
    CompatibleCurves::new(knots, z.len(), points, Some(weights)).unwrap()
}

/// The point at `t` of the circle, each quarter's rational quadratic written out.
fn circle(r: f64, z: f64, t: f64) -> [f64; 3] {
    let k = ((t * 4.0).floor() as usize).min(3);
    let s = t * 4.0 - k as f64;
    let w = core::f64::consts::FRAC_1_SQRT_2;
    let (b0, b1, b2) = ((1.0 - s) * (1.0 - s), 2.0 * s * (1.0 - s) * w, s * s);
    let x = (b0 * r + b1 * r) / (b0 + b1 + b2);
    let y = (b1 * r + b2 * r) / (b0 + b1 + b2);
    let (x, y) = (0..k).fold((x, y), |(x, y), _| (-y, x));
    [x, y, z]
}

#[test]
fn circles_at_heights_fit_to_the_exact_rational_cylinder() {
    let (r, h) = (2.5, 3.0);
    let mut rng = Rng::new(3);
    for n in [2, 4, 7] {
        let v = params(&mut rng, n);
        let z: Vec<f64> = v.iter().map(|t| t * h).collect();
        let rows = circles(r, &z);
        for q in 1..=3.min(n) {
            let fits = [
                interpolate_rows(&rows, &v, q, &Edge::Point, &Edge::Point),
                if n > q + 1 {
                    least_squares_rows(&rows, &v, q, q + 1)
                } else {
                    interpolate_rows(&rows, &v, q, &Edge::Point, &Edge::Point)
                },
            ];
            for s in fits {
                let s = s.unwrap();
                assert!(s.is_rational());
                assert_eq!(s.degree(ParamAxis::U), 2);
                assert_eq!(s.knots(ParamAxis::U), rows.knot_vector().knots());
                for _ in 0..300 {
                    let (u, w) = (rng.next_f64(), rng.next_f64());
                    let got = at(&s, u, w);
                    let want = circle(r, w * h, u);
                    assert!(near(&got, &want, 1e-12), "n {n} q {q}: {got:?} {want:?}");
                    let radius = fmath::hypot(got[0], got[1]);
                    assert!((radius - r).abs() < 1e-12 * r);
                }
                // At each row: the source curve.
                for (k, &t) in v.iter().enumerate() {
                    for u in [0.0, 0.1, 0.37, 0.5, 0.9, 1.0] {
                        assert!(near(&at(&s, u, t), &circle(r, z[k], u), 1e-12));
                    }
                }
            }
        }
    }
}

#[test]
fn ends_hold_the_boundary_derivatives_of_a_polynomial_surface() {
    // f(u, v) = sum_r u^r h_r(v), h_r cubic in v: the control point i of the row at v along u
    // (degree p, one Bezier piece) is sum_r C(i, r) / C(p, r) h_r(v).
    let p = 3_usize;
    let mut rng = Rng::new(4);
    let h: Vec<[[f64; 3]; 4]> = (0..=p)
        .map(|_| core::array::from_fn(|_| core::array::from_fn(|_| rng.next_signed_unit())))
        .collect();
    let hv = |r: usize, v: f64, d: usize| -> [f64; 3] {
        let mut out = [0.0; 3];
        for (b, c) in h[r].iter().enumerate() {
            if b < d {
                continue;
            }
            let falling: f64 = (0..d).map(|m| (b - m) as f64).product();
            for (o, x) in out.iter_mut().zip(c) {
                *o += x * falling * fmath::powi(v, (b - d) as i32);
            }
        }
        out
    };
    let ratio = |i: usize, r: usize| -> f64 {
        let c = |n: usize, k: usize| -> f64 {
            if k > n {
                return 0.0;
            }
            (1..=k).fold(1.0, |a, m| a * (n + 1 - m) as f64 / m as f64)
        };
        c(i, r) / c(p, r)
    };
    let ctrl = |i: usize, v: f64, d: usize| -> [f64; 3] {
        let mut out = [0.0; 3];
        for r in 0..=p {
            let x = hv(r, v, d);
            for (o, y) in out.iter_mut().zip(x) {
                *o += ratio(i, r) * y;
            }
        }
        out
    };
    let f = |u: f64, v: f64| -> [f64; 3] {
        let mut out = [0.0; 3];
        for r in 0..=p {
            let x = hv(r, v, 0);
            for (o, y) in out.iter_mut().zip(x) {
                *o += fmath::powi(u, r as i32) * y;
            }
        }
        out
    };
    let v = params(&mut rng, 5);
    let ku = KnotVector::new(p, vec![0.0, 0.0, 0.0, 0.0, 1.0, 1.0, 1.0, 1.0]).unwrap();
    let points: Vec<[f64; 3]> = v
        .iter()
        .flat_map(|&t| (0..=p).map(move |i| (i, t)))
        .map(|(i, t)| ctrl(i, t, 0))
        .collect();
    let rows = CompatibleCurves::new(ku, v.len(), points, None).unwrap();
    let edge = |t: f64, order: usize| -> Edge<3> {
        Edge::Held(
            (0..=p)
                .map(|i| match order {
                    1 => End::Tangent { d1: ctrl(i, t, 1) },
                    _ => End::Curvature {
                        d1: ctrl(i, t, 1),
                        d2: ctrl(i, t, 2),
                    },
                })
                .collect(),
        )
    };
    for order in [1, 2] {
        let s = interpolate_rows(&rows, &v, 3, &edge(0.0, order), &edge(1.0, order)).unwrap();
        assert_eq!(s.points().ncols(), v.len() + 2 * order);
        for _ in 0..200 {
            let (x, y) = (rng.next_f64(), rng.next_f64());
            assert!(near(&at(&s, x, y), &f(x, y), 1e-9), "order {order}");
        }
        // The held derivative is the surface's: forward difference at the end.
        let eps = 1e-6;
        let d = |t: f64, sgn: f64| -> [f64; 3] {
            let (a, b) = (at(&s, 0.4, t + sgn * eps), at(&s, 0.4, t));
            core::array::from_fn(|k| sgn * (a[k] - b[k]) / eps)
        };
        let want: [f64; 3] = {
            let mut o = [0.0; 3];
            for r in 0..=p {
                let x = hv(r, 0.0, 1);
                for (a, y) in o.iter_mut().zip(x) {
                    *a += fmath::powi(0.4_f64, r as i32) * y;
                }
            }
            o
        };
        assert!(near(&d(0.0, 1.0), &want, 1e-4));
    }
}

#[test]
fn the_pins_geometry_tests_judged_against_the_data() {
    // `interpolation_hits_points`: a 5 x 4 grid at chord parameters, degrees (3, 2).
    let pts: Vec<[f64; 3]> = (0..5)
        .flat_map(|i| (0..4).map(move |j| (f64::from(i), f64::from(j))))
        .map(|(i, j)| [i, j, fmath::sin(i * 0.7) + j * j * 0.1])
        .collect();
    let grid = Net::new(5, 4, pts).unwrap();
    let (u, v) = grid_parameters(&grid, Param::ChordLength).unwrap();
    let s = interpolate(&grid, &u, &v, (3, 2)).unwrap();
    for (i, &a) in u.iter().enumerate() {
        for (j, &b) in v.iter().enumerate() {
            assert!(near(&at(&s, a, b), grid.get(i, j).unwrap(), 1e-9));
        }
    }
    // Knot vectors of the wrong count are refused, naming the count.
    let kv = fit::averaging(&v, 2, 0, 0).unwrap();
    let wrong = interpolate_on(&grid, &kv, &kv, &u, &v);
    assert!(matches!(
        wrong,
        Err(SurfaceFitError::Fit {
            axis: ParamAxis::U,
            cause: FitError::KnotCount { .. }
        })
    ));

    // `approximation_of_plane_is_exact`: a plane through a 9 x 7 grid, net 5 x 4, degrees (3, 2).
    let plane: Vec<[f64; 3]> = (0..9)
        .flat_map(|i| (0..7).map(move |j| (f64::from(i), f64::from(j))))
        .map(|(i, j)| [i, 2.0 * j, 0.5 * i - j])
        .collect();
    let grid = Net::new(9, 7, plane).unwrap();
    let (u, v) = grid_parameters(&grid, Param::ChordLength).unwrap();
    let s = least_squares(&grid, &u, &v, (3, 2), (5, 4)).unwrap();
    assert_eq!((s.points().nrows(), s.points().ncols()), (5, 4));
    for (i, &a) in u.iter().enumerate() {
        for (j, &b) in v.iter().enumerate() {
            assert!(near(&at(&s, a, b), grid.get(i, j).unwrap(), 1e-9));
        }
    }
    // The ruled and bilinear nets of the pin are net sizes 2 x m, n x 2 with degree 1.
    let s = least_squares(&grid, &u, &v, (1, 1), (2, 2)).unwrap();
    assert!(near(
        s.points().get(1, 1).unwrap(),
        grid.get(8, 6).unwrap(),
        1e-14
    ));
    // More control points than data: refused.
    assert!(matches!(
        least_squares(&grid, &u, &v, (2, 2), (9, 3)),
        Err(SurfaceFitError::Fit {
            axis: ParamAxis::U,
            cause: FitError::TooFewControlPoints { .. }
        })
    ));

    // `function_interpolation`: a scalar grid is a one-dimensional surface.
    let f: Vec<[f64; 1]> = (0..5)
        .flat_map(|i| (0..4).map(move |j| [f64::from(i * i - j)]))
        .collect();
    let (u, v) = ([0.0, 0.2, 0.5, 0.7, 1.0], [0.0, 0.3, 0.6, 1.0]);
    let grid = Net::new(5, 4, f).unwrap();
    let s = interpolate(&grid, &u, &v, (3, 2)).unwrap();
    for (i, &a) in u.iter().enumerate() {
        for (j, &b) in v.iter().enumerate() {
            assert!(near(&at(&s, a, b), grid.get(i, j).unwrap(), 1e-9));
        }
    }
    assert!(interpolate(&grid, &u, &v, (5, 2)).is_err());
    assert!(interpolate(&grid, &u, &v, (0, 2)).is_err());
}

#[test]
fn parameters_of_a_grid() {
    let pts: Vec<[f64; 3]> = (0..3)
        .flat_map(|i| (0..4).map(move |j| (f64::from(i), f64::from(j))))
        .map(|(i, j)| [i * i, j, 0.0])
        .collect();
    let grid = Net::new(3, 4, pts).unwrap();
    let (u, v) = grid_parameters(&grid, Param::Uniform).unwrap();
    assert_eq!(u, vec![0.0, 0.5, 1.0]);
    assert_eq!(v, vec![0.0, 1.0 / 3.0, 2.0 / 3.0, 1.0]);
    let (u, v) = grid_parameters(&grid, Param::ChordLength).unwrap();
    assert_eq!(u, vec![0.0, 0.25, 1.0]);
    assert!((v[1] - 1.0 / 3.0).abs() < 1e-15);
    // A single row has no chord parameters; a grid of one point has none at all.
    let one = Net::new(1, 3, vec![[0.0; 3], [1.0, 0.0, 0.0], [2.0, 0.0, 0.0]]).unwrap();
    assert!(matches!(
        grid_parameters(&one, Param::ChordLength),
        Err(SurfaceFitError::Fit {
            axis: ParamAxis::U,
            cause: FitError::TooFewPoints { .. }
        })
    ));
    let same = Net::new(2, 2, vec![[1.0; 3]; 4]).unwrap();
    assert!(matches!(
        grid_parameters(&same, Param::Centripetal),
        Err(SurfaceFitError::Fit {
            cause: FitError::ZeroLength,
            ..
        })
    ));
}

#[test]
fn refusals_are_typed_and_leave_the_inputs() {
    let z = [0.0, 1.0, 2.5, 3.0];
    let rows = circles(1.0, &z);
    let v = [0.0, 0.3, 0.8, 1.0];
    let copy = rows.clone();
    let held = |n: usize, order: usize| -> Edge<3> {
        Edge::Held(
            (0..n)
                .map(|_| {
                    if order == 1 {
                        End::Tangent { d1: [0.0; 3] }
                    } else {
                        End::Point
                    }
                })
                .collect(),
        )
    };
    assert!(matches!(
        interpolate_rows(&rows, &v, 3, &held(9, 1), &Edge::Point),
        Err(SurfaceFitError::RationalEnds)
    ));
    // Polynomial rows with the wrong number of conditions or mixed orders.
    let poly_rows = CompatibleCurves::new(
        KnotVector::new(1, vec![0.0, 0.0, 1.0, 1.0]).unwrap(),
        4,
        vec![[0.0, 0.0, 0.0]; 8],
        None,
    )
    .unwrap();
    assert!(matches!(
        interpolate_rows(&poly_rows, &v, 3, &held(3, 1), &Edge::Point),
        Err(SurfaceFitError::EdgeCount { count: 3, lines: 2 })
    ));
    let mixed = Edge::Held(vec![End::Point, End::Tangent { d1: [0.0; 3] }]);
    assert!(matches!(
        interpolate_rows(&poly_rows, &v, 3, &mixed, &Edge::Point),
        Err(SurfaceFitError::EdgeOrders)
    ));
    // Parameter count, order, and knots.
    let e = interpolate_rows(&rows, &v[..3], 2, &Edge::Point, &Edge::Point).unwrap_err();
    assert!(matches!(
        e,
        SurfaceFitError::Fit {
            axis: ParamAxis::V,
            cause: FitError::ParameterCount {
                count: 3,
                points: 4
            }
        }
    ));
    let bad = [0.0, 0.5, 0.4, 1.0];
    assert!(matches!(
        interpolate_rows(&rows, &bad, 2, &Edge::Point, &Edge::Point),
        Err(SurfaceFitError::Fit {
            cause: FitError::ParametersNotIncreasing { .. },
            ..
        })
    ));
    // Degree above the rows: a cubic through 3 rows.
    assert!(interpolate_rows(&rows, &v[..2], 3, &Edge::Point, &Edge::Point).is_err());
    let kv = fit::averaging(&v, 2, 0, 0).unwrap();
    assert!(interpolate_rows_on(&rows, &kv, &v, &Edge::Point, &Edge::Point).is_ok());
    assert_eq!(rows, copy);
    // Kinds and stable names.
    let e = SurfaceFitError::EdgeOrders;
    assert_eq!(e.kind(), ErrorKind::Refused);
    let frame = SurfaceFitError::Fit {
        axis: ParamAxis::U,
        cause: FitError::ZeroLength,
    };
    assert_eq!(frame.kind(), FitError::ZeroLength.kind());
    let names = [
        frame.name(),
        SurfaceFitError::Curves(NurbsError::NoCurves).name(),
        SurfaceFitError::Surface(NurbsSurfaceError::NoSurfaces).name(),
        SurfaceFitError::EdgeCount { count: 1, lines: 2 }.name(),
        e.name(),
        SurfaceFitError::RationalEnds.name(),
        SurfaceFitError::Defect { why: "x" }.name(),
    ];
    let mut sorted = names.to_vec();
    sorted.sort_unstable();
    sorted.dedup();
    assert_eq!(sorted.len(), names.len());
    assert!(std::error::Error::source(&frame).is_some());
    assert!(!frame.to_string().contains('\n'));
}

#[test]
fn a_fitted_weight_that_is_not_positive_is_refused_not_clamped() {
    // Weights 1, 0.01, 1, 0.01, 1 along v: the interpolating control weights oscillate below 0.
    let knots = KnotVector::new(1, vec![0.0, 0.0, 1.0, 1.0]).unwrap();
    let w = [1.0, 0.01, 1.0, 0.01, 1.0];
    let mut points = Vec::new();
    let mut weights = Vec::new();
    for (k, &wk) in w.iter().enumerate() {
        points.extend([[0.0, k as f64, 0.0], [1.0, k as f64, 0.0]]);
        weights.extend([wk, wk]);
    }
    let rows = CompatibleCurves::new(knots, 5, points, Some(weights)).unwrap();
    let v = [0.0, 0.25, 0.5, 0.75, 1.0];
    let r = interpolate_rows(&rows, &v, 3, &Edge::Point, &Edge::Point);
    assert!(matches!(
        r,
        Err(SurfaceFitError::Surface(NurbsSurfaceError::BadWeights(_)))
    ));
}

#[test]
fn random_nets_interpolate_their_own_samples() {
    // A random surface of each knot kind, sampled on a grid and fitted back with its own
    // degrees and enough rows: the fit passes through the samples.
    let mut rng = Rng::new(5);
    for (pu, pv) in [(1, 2), (2, 3), (3, 3)] {
        for knots in [Knots::Uniform, Knots::Random] {
            let spec = |degree| Spec {
                degree,
                count: degree + 3,
                knots,
            };
            let d = sample::surface(&mut rng, (spec(pu), spec(pv)), Weights::Unit);
            let src = NurbsSurface::new(
                KnotVector::new(pu, d.knots_u.clone()).unwrap(),
                KnotVector::new(pv, d.knots_v.clone()).unwrap(),
                Net::new(pu + 3, pv + 3, d.points.clone()).unwrap(),
                None,
            );
            let Ok(src) = src else { continue };
            let (u, v) = (params(&mut rng, 8), params(&mut rng, 9));
            let grid = grid_of(|a, b| at(&src, a, b), &u, &v);
            let s = interpolate(&grid, &u, &v, (pu, pv)).unwrap();
            for (i, &a) in u.iter().enumerate() {
                for (j, &b) in v.iter().enumerate() {
                    assert!(near(&at(&s, a, b), grid.get(i, j).unwrap(), 1e-9));
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------- Bernstein

fn forms_agree(s: &NurbsSurface<3>, uv: &Extent2d, rng: &mut Rng) {
    let form = s.bernstein_over(uv).unwrap();
    let (a, b) = (uv.min(), uv.max());
    for _ in 0..50 {
        let (x, y) = (rng.next_f64(), rng.next_f64());
        let got = form.point(x, y);
        let want = at(s, a.x + x * (b.x - a.x), a.y + y * (b.y - a.y));
        assert!(near(&got, &want, 1e-11), "{got:?} {want:?}");
    }
}

#[test]
fn bernstein_forms_equal_the_surface_on_every_span() {
    let mut rng = Rng::new(6);
    let (u, v) = (params(&mut rng, 7), params(&mut rng, 6));
    let a = random_poly(&mut rng, 3, 2);
    let poly_s = interpolate(&grid_of(|x, y| poly(&a, x, y), &u, &v), &u, &v, (3, 2)).unwrap();
    let cyl_rows = circles(2.0, &[0.0, 1.0, 3.0]);
    let cyl = interpolate_rows(&cyl_rows, &[0.0, 0.4, 1.0], 2, &Edge::Point, &Edge::Point).unwrap();
    for s in [&poly_s, &cyl] {
        let mut ku: Vec<f64> = s.knots(ParamAxis::U).to_vec();
        let mut kv: Vec<f64> = s.knots(ParamAxis::V).to_vec();
        ku.dedup();
        kv.dedup();
        for wu in ku.windows(2) {
            for wv in kv.windows(2) {
                // The span, and a box inside it.
                let span = Extent2d::from_bounds(wu[0], wv[0], wu[1], wv[1]).unwrap();
                forms_agree(s, &span, &mut rng);
                let (du, dv) = (wu[1] - wu[0], wv[1] - wv[0]);
                let inner = Extent2d::from_bounds(
                    wu[0] + 0.2 * du,
                    wv[0] + 0.3 * dv,
                    wu[0] + 0.9 * du,
                    wv[0] + 0.6 * dv,
                )
                .unwrap();
                forms_agree(s, &inner, &mut rng);
            }
        }
    }
}

#[test]
fn bernstein_products_are_the_products_of_the_values() {
    // A bicubic patch's normal (the cross product of the partials) as a Bernstein form is the
    // cross product of the partials' values: the form of the product, not the product of
    // rounded values.
    let mut rng = Rng::new(7);
    let a = random_poly(&mut rng, 3, 3);
    let (u, v) = (params(&mut rng, 6), params(&mut rng, 6));
    let s = interpolate(&grid_of(|x, y| poly(&a, x, y), &u, &v), &u, &v, (3, 3)).unwrap();
    // Take a span-sized box in the middle of the first knot spans.
    let ku = s.knots(ParamAxis::U);
    let kv = s.knots(ParamAxis::V);
    let (u0, u1) = (ku[4], ku[5]);
    let (v0, v1) = (kv[4], kv[5]);
    let box_ = Extent2d::from_bounds(u0, v0, u1, v1).unwrap();
    let form = s.bernstein_over(&box_).unwrap();
    let c = form.coordinates();
    let (du, dv) = (u1 - u0, v1 - v0);
    // Partials of the form, in the unit square, scaled back by the box's widths.
    let pu: Vec<_> = c.iter().map(Bernstein2::deriv_u).collect();
    let pv: Vec<_> = c.iter().map(Bernstein2::deriv_v).collect();
    let cross_x = pu[1]
        .mul(&pv[2])
        .unwrap()
        .sub(&pu[2].mul(&pv[1]).unwrap())
        .unwrap();
    for _ in 0..50 {
        let (x, y) = (rng.next_f64(), rng.next_f64());
        // Finite-difference partials of the surface give the same cross product.
        let (uu, vv) = (u0 + x * du, v0 + y * dv);
        let h = 1e-6;
        let d_u: [f64; 3] = core::array::from_fn(|k| {
            (at(&s, uu + h, vv)[k] - at(&s, uu - h, vv)[k]) / (2.0 * h) * du
        });
        let d_v: [f64; 3] = core::array::from_fn(|k| {
            (at(&s, uu, vv + h)[k] - at(&s, uu, vv - h)[k]) / (2.0 * h) * dv
        });
        let want = d_u[1] * d_v[2] - d_u[2] * d_v[1];
        let got = cross_x.eval(x, y);
        assert!(
            (got - want).abs() < 1e-5 * (1.0 + want.abs()),
            "{got} {want}"
        );
    }
    // The form's restriction to a sub-box is the form over that sub-box of the surface.
    let sub = form.restrict(0.25, 0.75, 0.1, 0.6);
    let sub_box =
        Extent2d::from_bounds(u0 + 0.25 * du, v0 + 0.1 * dv, u0 + 0.75 * du, v0 + 0.6 * dv)
            .unwrap();
    let direct = s.bernstein_over(&sub_box).unwrap();
    for _ in 0..50 {
        let (x, y) = (rng.next_f64(), rng.next_f64());
        assert!(near(&sub.point(x, y), &direct.point(x, y), 1e-12));
    }
}

#[test]
fn bernstein_refusals() {
    let rows = circles(1.0, &[0.0, 1.0]);
    let s = interpolate_rows(&rows, &[0.0, 1.0], 1, &Edge::Point, &Edge::Point).unwrap();
    // Across the knot 0.25.
    let across = Extent2d::from_bounds(0.1, 0.0, 0.4, 1.0).unwrap();
    assert!(matches!(
        s.bernstein_over(&across),
        Err(NurbsSurfaceError::Restrict(RestrictCause::Interior {
            axis: ParamAxis::U,
            knot
        })) if knot == 0.25
    ));
    let outside = Extent2d::from_bounds(0.0, 0.0, 0.2, 2.0).unwrap();
    assert!(matches!(
        s.bernstein_over(&outside),
        Err(NurbsSurfaceError::Restrict(RestrictCause::Outside { .. }))
    ));
    let flat = Extent2d::from_bounds(0.0, 0.5, 0.2, 0.5).unwrap();
    assert!(matches!(
        s.bernstein_over(&flat),
        Err(NurbsSurfaceError::Restrict(RestrictCause::Empty { .. }))
    ));
}
