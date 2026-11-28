// Written for solidcorex. Apache-2.0.
//! Tests of skinning against closed forms: the cylinder, the cone, a surface of revolution of
//! a parabolic radius law, rings closed across a seam, held and homogeneous boundary
//! derivatives on rational rows, the pin's skin cases, and the typed refusals.

use scx_num::error::ErrorKind;
use scx_num::hash::Rng;
use scx_nurbs::fit::End;
use scx_nurbs::{Approach, CompatibleCurves, KnotVector, NurbsCurve};

use crate::fit::{Edge, SurfaceFitError};
use crate::fit_tests::{circle, params, ring};
use crate::skin::{MAX_CLOSED_DEGREE, Skin, SkinError, Skinned, skin};
use crate::{NurbsSurface, ParamAxis};

const ABOVE: [Approach; 2] = [Approach::Above; 2];

const OPEN: Skin<3> = Skin::Open {
    start: Edge::Point,
    end: Edge::Point,
};

fn at<const D: usize>(s: &NurbsSurface<D>, u: f64, v: f64) -> [f64; D] {
    s.point([u, v], ABOVE).unwrap()
}

fn dist<const D: usize>(a: &[f64; D], b: &[f64; D]) -> f64 {
    a.iter()
        .zip(b)
        .map(|(x, y)| (x - y) * (x - y))
        .sum::<f64>()
        .sqrt()
}

fn circle_knots() -> KnotVector {
    KnotVector::new(
        2,
        vec![
            0.0, 0.0, 0.0, 0.25, 0.25, 0.5, 0.5, 0.75, 0.75, 1.0, 1.0, 1.0,
        ],
    )
    .unwrap()
}

/// Circles about the z axis of radius `r[k]` at height `z[k]`, as compatible rational curves.
fn circles(r: &[f64], z: &[f64]) -> CompatibleCurves<3> {
    let (unit, w) = ring(1.0);
    let mut points = Vec::new();
    let mut weights = Vec::new();
    for (&rk, &h) in r.iter().zip(z) {
        points.extend(unit.iter().map(|p| [rk * p[0], rk * p[1], h]));
        weights.extend_from_slice(&w);
    }
    CompatibleCurves::new(circle_knots(), z.len(), points, Some(weights)).unwrap()
}

/// The largest sampled `|S(u, v_k) - section_k(u)|`.
fn sampled_miss(s: &Skinned<3>, rows: &CompatibleCurves<3>, v: &[f64]) -> f64 {
    let mut most = 0.0_f64;
    for (k, &t) in v.iter().enumerate() {
        let c = rows.curve(k).unwrap();
        for j in 0..=64 {
            let u = f64::from(j) / 64.0;
            let want = c.point(u, Approach::Above).unwrap();
            most = most.max(dist(&at(&s.surface, u, t), &want));
        }
    }
    most
}

/// Checks `S(u, v) = f(u, v)` at random points to `tol`, and that the certified `through`
/// covers the sampled miss and is small.
fn matches(
    s: &Skinned<3>,
    rows: &CompatibleCurves<3>,
    v: &[f64],
    f: impl Fn(f64, f64) -> [f64; 3],
    tol: f64,
) {
    let mut rng = Rng::new(7);
    for _ in 0..400 {
        let (u, t) = (rng.next_f64(), rng.next_f64());
        let got = at(&s.surface, u, t);
        let want = f(u, t);
        assert!(dist(&got, &want) <= tol, "at ({u}, {t}): {got:?} {want:?}");
    }
    let miss = sampled_miss(s, rows, v);
    assert!(
        s.through >= miss,
        "through {} below the miss {miss}",
        s.through
    );
    assert!(s.through <= 1e-11, "through {}", s.through);
}

#[test]
fn equal_circles_on_degree_one_give_the_cylinder() {
    let (r, h) = (2.5, 3.0);
    let mut rng = Rng::new(1);
    for n in [1, 3, 6] {
        let v = params(&mut rng, n);
        let z: Vec<f64> = v.iter().map(|t| t * h).collect();
        let rows = circles(&vec![r; z.len()], &z);
        let s = skin(&rows, &v, 1, &OPEN).unwrap();
        assert!(s.surface.is_rational());
        matches(&s, &rows, &v, |u, t| circle(r, t * h, u), 1e-12);
    }
}

#[test]
fn a_radius_linear_in_height_gives_the_cone() {
    let (r0, c, h) = (1.0, 0.75, 2.0);
    let radius = |z: f64| r0 + c * z;
    let mut rng = Rng::new(2);
    for (n, q) in [(1, 1), (4, 1), (4, 2), (6, 3)] {
        let v = params(&mut rng, n);
        let z: Vec<f64> = v.iter().map(|t| t * h).collect();
        let r: Vec<f64> = z.iter().map(|&x| radius(x)).collect();
        let rows = circles(&r, &z);
        let s = skin(&rows, &v, q, &OPEN).unwrap();
        matches(&s, &rows, &v, |u, t| circle(radius(t * h), t * h, u), 1e-12);
    }
}

#[test]
fn radii_on_a_parabola_give_the_surface_of_revolution() {
    // r(z) = 1 + z/2 - z^2/5: three circles at z = 0, 1, 3 at degree 2 are the quadratic
    // law exactly (z linear in v, r quadratic); five circles at degree 2 or 3 too, since
    // the spline space holds the quadratics.
    let radius = |z: f64| 1.0 + 0.5 * z - 0.2 * z * z;
    for (z, q) in [
        (vec![0.0, 1.0, 3.0], 2),
        (vec![0.0, 0.4, 1.0, 2.2, 3.0], 2),
        (vec![0.0, 0.4, 1.0, 2.2, 3.0], 3),
    ] {
        let v: Vec<f64> = z.iter().map(|x| x / 3.0).collect();
        let r: Vec<f64> = z.iter().map(|&x| radius(x)).collect();
        let rows = circles(&r, &z);
        let s = skin(&rows, &v, q, &OPEN).unwrap();
        matches(
            &s,
            &rows,
            &v,
            |u, t| circle(radius(3.0 * t), 3.0 * t, u),
            1e-12,
        );
    }
}

/// Circles of radius `r` in the planes through the z axis at `theta[k]`, centred `big` from
/// the axis: sections of the torus, as compatible rational curves.
fn torus_sections(big: f64, r: f64, theta: &[f64]) -> CompatibleCurves<3> {
    let (unit, w) = ring(1.0);
    let mut points = Vec::new();
    let mut weights = Vec::new();
    for &a in theta {
        let (s, c) = a.sin_cos();
        points.extend(unit.iter().map(|p| {
            let rho = big + r * p[0];
            [rho * c, rho * s, r * p[1]]
        }));
        weights.extend_from_slice(&w);
    }
    CompatibleCurves::new(circle_knots(), theta.len(), points, Some(weights)).unwrap()
}

/// Open polynomial sections in rotating planes, the last the first again.
fn polygon_ring(m: usize, lift: f64) -> (CompatibleCurves<3>, Vec<f64>) {
    let knots = KnotVector::new(2, vec![0.0, 0.0, 0.0, 0.5, 1.0, 1.0, 1.0]).unwrap();
    let profile = [[1.0, 0.0], [1.5, 0.4], [2.0, -0.2], [2.2, 0.5]];
    let mut points = Vec::new();
    let mut v = Vec::new();
    for k in 0..=m {
        let t = k as f64 / m as f64;
        // Uneven parameters, so the seam's conditions are not symmetric.
        v.push(t + 0.05 * (core::f64::consts::TAU * t).sin());
        // The seam's angle is 0 exactly, so only `lift` parts the ends.
        let (s, c) = (core::f64::consts::TAU * (k % m) as f64 / m as f64).sin_cos();
        let scale = 1.0 + 0.3 * (2.0 * core::f64::consts::TAU * (k % m) as f64 / m as f64).cos();
        let z = if k == m { lift } else { 0.0 };
        points.extend(
            profile
                .iter()
                .map(|p| [scale * p[0] * c, scale * p[0] * s, p[1] + z]),
        );
    }
    (
        CompatibleCurves::new(knots, m + 1, points, None).unwrap(),
        v,
    )
}

/// The control curve across `v` of u index `i`: homogeneous coordinates and weight.
fn column_curve(s: &NurbsSurface<3>, i: usize) -> NurbsCurve<4> {
    let kv = s.knot_vector(ParamAxis::V).clone();
    let pts: Vec<[f64; 4]> = match (s.weighted_points(), s.weights()) {
        (Some(wp), Some(w)) => wp
            .row(i)
            .unwrap()
            .iter()
            .zip(w.row(i).unwrap())
            .map(|(p, &x)| [p[0], p[1], p[2], x])
            .collect(),
        _ => s
            .points()
            .row(i)
            .unwrap()
            .iter()
            .map(|p| [p[0], p[1], p[2], 1.0])
            .collect(),
    };
    NurbsCurve::new(kv, pts, None).unwrap()
}

/// The seam is bitwise one row, and every column is `C^(q-1)` across it.
fn check_seam(s: &NurbsSurface<3>, q: usize) {
    let kv = s.knot_vector(ParamAxis::V);
    let (v0, vm) = (kv.knots()[0], *kv.knots().last().unwrap());
    let n = s.knot_vector(ParamAxis::U).point_count();
    for i in 0..n {
        let row = s.points().row(i).unwrap();
        assert_eq!(row.first(), row.last(), "seam row {i}");
        let c = column_curve(s, i);
        let a = c.derivs::<8>(v0, Approach::Above).unwrap();
        let b = c.derivs::<8>(vm, Approach::Below).unwrap();
        let scale = a[0].iter().fold(1.0_f64, |m, x| m.max(x.abs()));
        for k in 1..q {
            let size = scale.max(dist(&a[k], &[0.0; 4]));
            assert!(
                dist(&a[k], &b[k]) <= 1e-9 * size,
                "q {q} column {i} derivative {k}: {:?} {:?}",
                a[k],
                b[k]
            );
        }
        if q >= 2 {
            // Not trivially smooth: the first derivative is not zero.
            assert!(dist(&a[1], &[0.0; 4]) > 1e-3);
        }
    }
    if let Some(w) = s.weights() {
        for i in 0..n {
            let row = w.row(i).unwrap();
            assert_eq!(row.first(), row.last());
        }
    }
}

#[test]
fn a_closed_ring_is_smooth_and_exact_across_the_seam() {
    for q in 1..=5 {
        let (rows, v) = polygon_ring(8, 0.0);
        let s = skin(&rows, &v, q, &Skin::Closed { coincidence: 0.0 }).unwrap();
        check_seam(&s.surface, q);
        let miss = sampled_miss(&s, &rows, &v);
        assert!(
            s.through >= miss && s.through <= 1e-12,
            "q {q}: {} {miss}",
            s.through
        );
    }
    // The torus: rational sections, the last one the first rotated by a full turn (its
    // points off by the rounding of sin 2 pi).
    let theta: Vec<f64> = (0..=12)
        .map(|k| core::f64::consts::TAU * k as f64 / 12.0)
        .collect();
    let v: Vec<f64> = (0..=12).map(|k| k as f64 / 12.0).collect();
    let rows = torus_sections(3.0, 1.0, &theta);
    for q in 1..=MAX_CLOSED_DEGREE {
        let s = skin(&rows, &v, q, &Skin::Closed { coincidence: 1e-12 }).unwrap();
        assert!(s.surface.is_rational());
        check_seam(&s.surface, q);
        let miss = sampled_miss(&s, &rows, &v);
        assert!(
            s.through >= miss && s.through <= 1e-11,
            "q {q}: {} {miss}",
            s.through
        );
        // A torus point stays within the skin's reach of the tube: |dist to the core - 1|.
        let mut rng = Rng::new(q as u64);
        for _ in 0..50 {
            let p = at(&s.surface, rng.next_f64(), rng.next_f64());
            let rho = (p[0] * p[0] + p[1] * p[1]).sqrt();
            let tube = ((rho - 3.0).powi(2) + p[2] * p[2]).sqrt();
            assert!((tube - 1.0).abs() < 0.2, "q {q}: tube {tube}");
        }
    }
}

#[test]
fn a_closed_skin_within_the_coincidence_counts_the_gap() {
    let lift = 1e-10;
    let (rows, v) = polygon_ring(6, lift);
    let s = skin(&rows, &v, 3, &Skin::Closed { coincidence: 1e-9 }).unwrap();
    check_seam(&s.surface, 3);
    // The last section is missed by its gap, and through says so.
    let miss = sampled_miss(&s, &rows, &v);
    assert!(miss >= lift * 0.99);
    assert!(
        s.through >= miss && s.through <= 2.0 * lift,
        "{} {miss}",
        s.through
    );
}

#[test]
fn a_closed_skin_whose_ends_are_apart_is_refused_with_the_gap() {
    for h in [1e-6, 0.5] {
        let (rows, v) = polygon_ring(5, h);
        let e = skin(&rows, &v, 3, &Skin::Closed { coincidence: 1e-7 }).unwrap_err();
        let SkinError::NotClosed { gap, coincidence } = e else {
            panic!("{e:?}");
        };
        assert_eq!(coincidence, 1e-7);
        assert!(gap >= h && gap <= h + 1e-13, "h {h}: gap {gap}");
        assert_eq!(e.kind(), ErrorKind::Refused);
    }
}

#[test]
fn held_ends_on_rational_rows_are_derivatives_on_the_rows_weights() {
    // Two circles, cubic across, the tangent at both ends (c x_i, c y_i, h) on the unit
    // circle's control points: the radius' cubic Hermite with slope c is linear, the cone.
    let (r0, c, h) = (1.5, 0.6, 2.0);
    let (unit, _) = ring(1.0);
    for c in [0.0, c] {
        let d: Vec<End<3>> = unit
            .iter()
            .map(|p| End::Tangent {
                d1: [c * p[0], c * p[1], h],
            })
            .collect();
        let rows = circles(&[r0, r0 + c], &[0.0, h]);
        let v = [0.0, 1.0];
        let how = Skin::Open {
            start: Edge::Held(d.clone()),
            end: Edge::Held(d),
        };
        let s = skin(&rows, &v, 3, &how).unwrap();
        assert_eq!(s.surface.knot_vector(ParamAxis::V).point_count(), 4);
        matches(&s, &rows, &v, |u, t| circle(r0 + c * t, h * t, u), 1e-12);
    }
}

#[test]
fn homogeneous_ends_carry_the_weights_derivative() {
    // Two vertical segments at (1, 0) and (0, 1), quadratic across with the quarter circle's
    // homogeneous derivative at the start: the quarter cylinder, rational from polynomial
    // rows. The numerator is W (x, y, z_i), so its derivative is (W'x + W x', ..., W' z_i).
    let knots = KnotVector::new(1, vec![0.0, 0.0, 1.0, 1.0]).unwrap();
    let rows = CompatibleCurves::new(
        knots,
        2,
        vec![
            [1.0, 0.0, 0.0],
            [1.0, 0.0, 1.0],
            [0.0, 1.0, 0.0],
            [0.0, 1.0, 1.0],
        ],
        None,
    )
    .unwrap();
    let s2 = core::f64::consts::FRAC_1_SQRT_2;
    let dw = 2.0 * s2 - 2.0;
    let points: Vec<End<3>> = [0.0, 1.0]
        .iter()
        .map(|&z| End::Tangent {
            d1: [dw, 2.0 * s2, dw * z],
        })
        .collect();
    let weights = vec![End::Tangent { d1: [dw] }; 2];
    let how = Skin::Open {
        start: Edge::Homogeneous { points, weights },
        end: Edge::Point,
    };
    let v = [0.0, 1.0];
    let s = skin(&rows, &v, 2, &how).unwrap();
    assert!(s.surface.is_rational());
    let mut rng = Rng::new(5);
    for _ in 0..300 {
        let (u, t) = (rng.next_f64(), rng.next_f64());
        let p = at(&s.surface, u, t);
        assert!(
            ((p[0] * p[0] + p[1] * p[1]).sqrt() - 1.0).abs() <= 1e-14,
            "{p:?}"
        );
        assert!((p[2] - u).abs() <= 1e-14);
        assert!(p[0] >= -1e-15 && p[1] >= -1e-15);
    }
    let miss = sampled_miss(&s, &rows, &v);
    assert!(s.through >= miss && s.through <= 1e-13, "{}", s.through);
}

#[test]
fn homogeneous_with_still_weights_is_held_on_polynomial_rows() {
    let (rows, v) = pin_sections();
    let d: Vec<End<3>> = (0..4)
        .map(|j| End::Curvature {
            d1: [0.1 * j as f64, -0.2, 3.0],
            d2: [0.0, 0.5, -0.3 * j as f64],
        })
        .collect();
    let w = vec![
        End::Curvature {
            d1: [0.0],
            d2: [0.0]
        };
        4
    ];
    let held = skin(
        &rows,
        &v,
        3,
        &Skin::Open {
            start: Edge::Held(d.clone()),
            end: Edge::Point,
        },
    )
    .unwrap();
    let homog = skin(
        &rows,
        &v,
        3,
        &Skin::Open {
            start: Edge::Homogeneous {
                points: d,
                weights: w,
            },
            end: Edge::Point,
        },
    )
    .unwrap();
    assert!(!held.surface.is_rational() && homog.surface.is_rational());
    let mut rng = Rng::new(6);
    for _ in 0..300 {
        let (u, t) = (rng.next_f64(), rng.next_f64());
        assert!(dist(&at(&held.surface, u, t), &at(&homog.surface, u, t)) <= 1e-12);
    }
}

#[test]
fn pin_skins_lines_into_a_bilinear_patch() {
    // srf_adv `skins`: lines (0, 0, z) to (1, 0.1 z, z), z = 0..3, degree 2.
    let knots = KnotVector::new(1, vec![0.0, 0.0, 1.0, 1.0]).unwrap();
    let points = (0..4)
        .flat_map(|z| {
            let z = f64::from(z);
            [[0.0, 0.0, z], [1.0, 0.1 * z, z]]
        })
        .collect();
    let rows = CompatibleCurves::new(knots, 4, points, None).unwrap();
    let v = [0.0, 1.0 / 3.0, 2.0 / 3.0, 1.0];
    let s = skin(&rows, &v, 2, &OPEN).unwrap();
    assert!(dist(&at(&s.surface, 1.0, 1.0), &[1.0, 0.3, 3.0]) <= 1e-14);
    matches(&s, &rows, &v, |u, t| [u, 0.3 * t * u, 3.0 * t], 1e-13);
    // One curve does not close on itself.
    let one = CompatibleCurves::new(
        KnotVector::new(1, vec![0.0, 0.0, 1.0, 1.0]).unwrap(),
        1,
        vec![[0.0; 3], [1.0, 0.0, 0.0]],
        None,
    )
    .unwrap();
    let e = skin(&one, &[0.0], 1, &Skin::Closed { coincidence: 1.0 }).unwrap_err();
    assert!(
        matches!(
            e,
            SkinError::TooFewSections {
                count: 1,
                needed: 3
            }
        ),
        "{e:?}"
    );
}

/// srf_adv `interpolates_cross_sections`: four sections at z = 2i of radius 0.5 + 0.1i.
fn pin_sections() -> (CompatibleCurves<3>, Vec<f64>) {
    let knots = KnotVector::new(2, vec![0.0, 0.0, 0.0, 0.5, 1.0, 1.0, 1.0]).unwrap();
    let points = (0..4)
        .flat_map(|i| {
            let (z, r) = (2.0 * f64::from(i), 0.5 + 0.1 * f64::from(i));
            (0..4).map(move |j| [f64::from(j), r * f64::from(j % 2), z])
        })
        .collect();
    let rows = CompatibleCurves::new(knots, 4, points, None).unwrap();
    (rows, vec![0.0, 1.0 / 3.0, 2.0 / 3.0, 1.0])
}

#[test]
fn pin_interpolates_cross_sections() {
    let (rows, v) = pin_sections();
    let s = skin(&rows, &v, 3, &OPEN).unwrap();
    let want = rows.curve(3).unwrap().point(0.25, Approach::Above).unwrap();
    assert!(dist(&at(&s.surface, 0.25, 1.0), &want) <= 1e-13);
    assert!(s.through >= sampled_miss(&s, &rows, &v) && s.through <= 1e-12);
    // Every section is linear in i, so the skin is too (z = 6 v, r = 0.5 + 0.3 v).
    let mut rng = Rng::new(8);
    for _ in 0..100 {
        let (u, t) = (rng.next_f64(), rng.next_f64());
        let mut c = rows.curve(0).unwrap().point(u, Approach::Above).unwrap();
        let odd = c[1] / 0.5;
        c[1] = (0.5 + 0.3 * t) * odd;
        c[2] = 6.0 * t;
        assert!(dist(&at(&s.surface, u, t), &c) <= 1e-12);
    }
}

#[test]
fn pin_derivative_strips_add_rows() {
    let (rows, v) = pin_sections();
    let up = vec![
        End::Tangent {
            d1: [0.0, 0.0, 1.0]
        };
        4
    ];
    let how = Skin::Open {
        start: Edge::Held(up.clone()),
        end: Edge::Held(up),
    };
    let s = skin(&rows, &v, 3, &how).unwrap();
    assert_eq!(s.surface.knot_vector(ParamAxis::V).point_count(), 6);
    assert!(s.through >= sampled_miss(&s, &rows, &v) && s.through <= 1e-12);
    let d = column_curve(&s.surface, 2)
        .derivs::<2>(0.0, Approach::Above)
        .unwrap();
    assert!(dist(&d[1], &[0.0, 0.0, 1.0, 0.0]) <= 1e-12);
}

#[test]
fn refusals_are_typed() {
    let (rows, v) = pin_sections();
    // srf_adv `rejects_bad_input`: degree 5 on four sections.
    let e = skin(&rows, &v, 5, &OPEN).unwrap_err();
    assert!(matches!(e, SkinError::Fit(_)), "{e:?}");
    assert_eq!(e.kind(), ErrorKind::Refused);
    let e = skin(&rows, &v[..3], 2, &OPEN).unwrap_err();
    assert!(matches!(e, SkinError::Fit(_)), "{e:?}");
    let (ring_rows, rv) = polygon_ring(8, 0.0);
    let e = skin(
        &ring_rows,
        &rv,
        MAX_CLOSED_DEGREE + 1,
        &Skin::Closed { coincidence: 0.0 },
    )
    .unwrap_err();
    assert_eq!(
        e,
        SkinError::ClosedDegree {
            degree: MAX_CLOSED_DEGREE + 1,
            most: MAX_CLOSED_DEGREE
        }
    );
    assert_eq!(e.kind(), ErrorKind::Unsupported);
    for bad in [-1.0, f64::NAN, f64::INFINITY] {
        let e = skin(&ring_rows, &rv, 2, &Skin::Closed { coincidence: bad }).unwrap_err();
        assert!(matches!(e, SkinError::Coincidence { .. }), "{e:?}");
    }
    let names = [
        SkinError::Fit(SurfaceFitError::EdgeOrders).name(),
        SkinError::NotClosed {
            gap: 1.0,
            coincidence: 0.0,
        }
        .name(),
        SkinError::Coincidence { value: -1.0 }.name(),
        SkinError::TooFewSections {
            count: 1,
            needed: 3,
        }
        .name(),
        SkinError::ClosedDegree { degree: 9, most: 7 }.name(),
        SkinError::Seam.name(),
        SkinError::Gap(scx_nurbs::deviation::DeviationError::Empty).name(),
        SkinError::Through(scx_nurbs::deviation::DeviationError::Empty).name(),
        SkinError::WeightsLost.name(),
        SkinError::Defect { why: "x" }.name(),
    ];
    for (i, a) in names.iter().enumerate() {
        assert!(names.iter().skip(i + 1).all(|b| a != b), "{a}");
    }
}
