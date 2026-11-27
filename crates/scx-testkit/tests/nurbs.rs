// Written for solidcorex. Apache-2.0.
//! The NURBS references checked against closed forms and against each other: the Bezier
//! (Bernstein) form, the exact rational quarter circle, de Boor against the basis sum,
//! the continuity a knot's multiplicity allows, and knot insertion and degree elevation
//! leaving the shape exactly unchanged (Boehm against blossoms, elevation against the
//! Bezier formula).

#![allow(
    missing_docs,
    clippy::pedantic,
    clippy::indexing_slicing,
    clippy::needless_range_loop,
    clippy::unwrap_used,
    clippy::float_cmp,
    clippy::panic
)]

use scx_num::hash::Rng;
use scx_testkit::data::CurveData;
use scx_testkit::gen::{self, Knots, Spec, Weights};
use scx_testkit::nurbs::{Curve, Error, Exact, Side, Surface};
use scx_testkit::Rat;

fn r(x: f64) -> Rat {
    Rat::from_f64(x).unwrap()
}

fn q(n: i64, d: i64) -> Rat {
    Rat::from(n).checked_div(&Rat::from(d)).unwrap()
}

fn binomial(n: usize, k: usize) -> i64 {
    (0..k).fold(1, |b, i| b * (n - i) as i64 / (i as i64 + 1))
}

fn pow(x: &Rat, k: usize) -> Rat {
    (0..k).fold(Rat::one(), |a, _| &a * x)
}

const KNOTS: [Knots; 4] = [
    Knots::Uniform,
    Knots::Random,
    Knots::Clustered,
    Knots::Multiple,
];
const WEIGHTS: [Weights; 3] = [Weights::Unit, Weights::Moderate, Weights::NearZero];
const SIDES: [Side; 2] = [Side::Below, Side::Above];

/// Random curves of every kind, degrees 1..=4.
fn curves(seed: u64, per_kind: usize) -> Vec<CurveData<3>> {
    let mut rng = Rng::new(seed);
    let mut out = Vec::new();
    for knots in KNOTS {
        for weights in WEIGHTS {
            for i in 0..per_kind {
                let degree = 1 + i % 4;
                let count = degree + 1 + (i * 7 + 3) % 5;
                let spec = Spec {
                    degree,
                    count,
                    knots,
                };
                out.push(gen::curve(&mut rng, spec, weights));
            }
        }
    }
    out
}

/// Parameters per span: the ends and `extra` random points, on both sides.
fn samples(rng: &mut Rng, knots: &[f64], domain: (f64, f64), extra: usize) -> Vec<f64> {
    let mut out: Vec<f64> = knots
        .iter()
        .copied()
        .filter(|&k| domain.0 <= k && k <= domain.1)
        .collect();
    out.dedup();
    for _ in 0..extra {
        out.push(gen::parameter(rng, knots, domain));
    }
    out
}

#[test]
fn a_bezier_curve_is_its_bernstein_sum_inside_and_past_its_ends() {
    let mut rng = Rng::new(1);
    for p in 1..=5 {
        let knots: Vec<f64> = [0.0; 6]
            .iter()
            .take(p + 1)
            .chain([1.0; 6].iter().take(p + 1))
            .copied()
            .collect();
        let pts = gen::points::<2>(&mut rng, p + 1, 4.0);
        let w = gen::weights(&mut rng, p + 1, Weights::Moderate).unwrap();
        let c = Curve::new(p, &knots, &pts, Some(&w)).unwrap();
        for _ in 0..12 {
            let u = 3.0 * rng.next_f64() - 1.0;
            let ur = r(u);
            let one_minus = &Rat::one() - &ur;
            let (mut num, mut den) = ([Rat::zero(), Rat::zero()], Rat::zero());
            for (i, (pt, wi)) in pts.iter().zip(&w).enumerate() {
                let b = &(&Rat::from(binomial(p, i)) * &pow(&ur, i)) * &pow(&one_minus, p - i);
                let bw = &b * &r(*wi);
                for (n, x) in num.iter_mut().zip(pt) {
                    *n = &*n + &(&bw * &r(*x));
                }
                den = &den + &bw;
            }
            let want = num.map(|n| n.checked_div(&den).unwrap());
            for side in SIDES {
                assert_eq!(c.point(u, side).unwrap().exact, want, "p {p} u {u}");
            }
        }
    }
}

#[test]
fn the_rational_quarter_circle_is_exactly_a_circle() {
    // Weights (1, 1, 2), points (1, 0), (1, 1), (0, 1): C(u) = ((1 - u^2), 2u) / (1 + u^2).
    let c = Curve::new(
        2,
        &[0.0, 0.0, 0.0, 1.0, 1.0, 1.0],
        &[[1.0, 0.0], [1.0, 1.0], [0.0, 1.0]],
        Some(&[1.0, 1.0, 2.0]),
    )
    .unwrap();
    let mut rng = Rng::new(2);
    for i in 0..40 {
        let u = if i == 0 { 0.5 } else { rng.next_f64() };
        let ur = r(u);
        let u2 = &ur * &ur;
        let den = &Rat::one() + &u2;
        let d = c.derivs(u, Side::Above, 2).unwrap();
        let [x, y] = d[0].exact.clone();
        assert_eq!(&(&x * &x) + &(&y * &y), Rat::one(), "u {u}");
        assert_eq!(x, (&Rat::one() - &u2).checked_div(&den).unwrap());
        assert_eq!(y, (&Rat::from(2) * &ur).checked_div(&den).unwrap());
        // x' = -4u / (1 + u^2)^2, y' = 2 (1 - u^2) / (1 + u^2)^2.
        let den2 = &den * &den;
        assert_eq!(
            d[1].exact[0],
            (&Rat::from(-4) * &ur).checked_div(&den2).unwrap()
        );
        assert_eq!(
            d[1].exact[1],
            (&Rat::from(2) * &(&Rat::one() - &u2))
                .checked_div(&den2)
                .unwrap()
        );
        // The curvature of a unit circle: x' y'' - y' x'' = |C'|^3, here with |C'| =
        // 2 / (1 + u^2), so the cross product is 8 / (1 + u^2)^3.
        let cross = &(&d[1].exact[0] * &d[2].exact[1]) - &(&d[1].exact[1] * &d[2].exact[0]);
        assert_eq!(cross, Rat::from(8).checked_div(&(&den2 * &den)).unwrap());
        assert_eq!(d[0].rounded, d[0].exact.each_ref().map(Rat::to_f64));
    }
    assert_eq!(c.point(0.5, Side::Below).unwrap().exact, [q(3, 5), q(4, 5)]);
}

#[test]
fn de_boor_and_the_basis_sum_agree_exactly() {
    let mut rng = Rng::new(3);
    let mut checked = 0;
    for data in curves(30, 6) {
        let c = data.exact().unwrap();
        for u in samples(&mut rng, &data.knots, c.domain(), 6) {
            for side in SIDES {
                let a = c.derivs(u, side, 3).unwrap();
                let b = c.derivs_by_basis(u, side, 3).unwrap();
                assert_eq!(a, b, "{data:?} u {u} {side:?}");
                checked += 1;
            }
        }
    }
    assert!(checked > 1000, "{checked}");
}

#[test]
fn a_knot_of_multiplicity_m_joins_its_pieces_with_p_minus_m_derivatives() {
    let mut rng = Rng::new(4);
    let mut joins = 0;
    for data in curves(40, 8) {
        let c = data.exact().unwrap();
        let p = c.degree();
        let (a, b) = c.domain();
        let mut distinct = data.knots.clone();
        distinct.dedup();
        for &k in distinct.iter().filter(|&&k| a < k && k < b) {
            let m = data.knots.iter().filter(|&&x| x == k).count();
            let below = c.derivs(k, Side::Below, p).unwrap();
            let above = c.derivs(k, Side::Above, p).unwrap();
            assert_ne!(c.span(k, Side::Below), c.span(k, Side::Above));
            // C^(p - m) at the knot: derivatives 0..=p-m agree (none for m = p + 1).
            for d in 0..(p + 1).saturating_sub(m) {
                assert_eq!(below[d], above[d], "{data:?} knot {k} derivative {d}");
            }
            joins += 1;
        }
        // Away from the knots both sides are one piece.
        let u = gen::parameter(&mut rng, &[], (a, b));
        if !data.knots.contains(&u) {
            assert_eq!(c.derivs(u, Side::Below, 2), c.derivs(u, Side::Above, 2));
        }
    }
    assert!(joins > 100, "{joins}");
    // A generic C1 quadratic join really breaks at the second derivative.
    let c = Curve::new(
        2,
        &[0.0, 0.0, 0.0, 0.5, 1.0, 1.0, 1.0],
        &[[0.0], [1.0], [0.0], [3.0]],
        None,
    )
    .unwrap();
    let (lo, hi) = (
        c.derivs(0.5, Side::Below, 2).unwrap(),
        c.derivs(0.5, Side::Above, 2).unwrap(),
    );
    assert_eq!(lo[..2], hi[..2]);
    assert_ne!(lo[2], hi[2]);
}

/// Every sample of `a` and `b` agrees exactly, derivatives to order 2 on both sides.
fn same_curve(a: &Curve<3>, b: &Curve<3>, rng: &mut Rng, what: &str) {
    assert_eq!(a.domain(), b.domain());
    // b's knots: an inserted knot is sampled on both its sides.
    for u in samples(rng, b.knots(), a.domain(), 8) {
        for side in SIDES {
            // A side at a knot of the new curve that the old one lacks sees the same piece.
            let at = |c: &Curve<3>| {
                c.derivs(u, side, 2)
                    .unwrap_or_else(|e| panic!("{e}: {what} u {u} {side:?}"))
            };
            assert_eq!(at(a), at(b), "{what} u {u} {side:?}");
        }
    }
}

/// Each control point of `c` is the blossom of a piece at its knots `t_{i+1..=i+p}`,
/// evaluated on `of`, which must be the same spline.
fn points_are_blossoms(c: &Curve<3>, of: &Curve<3>) {
    let (p, t) = (c.degree(), c.knots());
    let n = c.points().len();
    for (i, (pt, w)) in c.points().iter().zip(c.weights()).enumerate() {
        let Some(s) = (i.max(p)..=(i + p).min(n - 1)).find(|&s| t[s] < t[s + 1]) else {
            continue; // a point under a break: no nondegenerate span in its support
        };
        let l = of.span(t[s], Side::Above);
        let (b, bw) = of.blossom(l, &t[i + 1..=i + p]).unwrap();
        assert_eq!((&b, &bw), (pt, &w), "point {i}");
    }
}

#[test]
fn knot_insertion_leaves_the_shape_exactly_unchanged() {
    let mut rng = Rng::new(5);
    for data in curves(50, 5) {
        let c = data.exact().unwrap();
        let p = c.degree();
        for _ in 0..3 {
            let u = gen::parameter(&mut rng, &data.knots, c.domain());
            let room = p + 1 - data.knots.iter().filter(|&&k| k == u).count().min(p + 1);
            let times = 1 + gen_below(&mut rng, 3);
            let inserted = c.insert_knot(u, times);
            if times > room {
                assert_eq!(inserted, Err(Error::Multiplicity));
                continue;
            }
            let inserted = inserted.unwrap();
            assert_eq!(inserted.knots().len(), data.knots.len() + times);
            same_curve(
                &c,
                &inserted,
                &mut rng,
                &format!("{data:?} insert {u} x{times}"),
            );
            points_are_blossoms(&inserted, &c);
        }
    }
}

fn gen_below(rng: &mut Rng, n: u64) -> usize {
    (rng.next_u64() % n) as usize
}

#[test]
fn a_bezier_curve_elevates_by_the_bernstein_formula() {
    let mut rng = Rng::new(6);
    for p in 1..=4 {
        let knots: Vec<f64> = core::iter::repeat_n(0.0, p + 1)
            .chain(core::iter::repeat_n(1.0, p + 1))
            .collect();
        let pts = gen::points::<3>(&mut rng, p + 1, 2.0);
        let w = gen::weights(&mut rng, p + 1, Weights::Moderate).unwrap();
        let c = Curve::new(p, &knots, &pts, Some(&w)).unwrap();
        let e = c.elevate(1).unwrap();
        // Homogeneous Q_i = i/(p+1) P_{i-1} + (1 - i/(p+1)) P_i.
        let hom: Vec<[Rat; 4]> = pts
            .iter()
            .zip(&w)
            .map(|(pt, &wi)| {
                let wr = r(wi);
                [&r(pt[0]) * &wr, &r(pt[1]) * &wr, &r(pt[2]) * &wr, wr]
            })
            .collect();
        for (i, (pt, we)) in e.points().iter().zip(e.weights()).enumerate() {
            let a = q(i as i64, p as i64 + 1);
            let b = &Rat::one() - &a;
            let get = |k: usize| hom.get(k).cloned().unwrap_or_default();
            let prev = if i == 0 {
                Default::default()
            } else {
                get(i - 1)
            };
            let want: Vec<Rat> = (0..4)
                .map(|k| &(&a * &prev[k]) + &(&b * &get(i)[k]))
                .collect();
            assert_eq!(we, want[3], "p {p} weight {i}");
            for k in 0..3 {
                assert_eq!(&pt[k] * &we, want[k], "p {p} point {i}");
            }
        }
        same_curve(&c, &e, &mut rng, "bezier");
    }
}

#[test]
fn degree_elevation_leaves_the_shape_exactly_unchanged() {
    let mut rng = Rng::new(7);
    for data in curves(70, 4) {
        let c = data.exact().unwrap();
        let times = 1 + gen_below(&mut rng, 2);
        let e = c.elevate(times).unwrap();
        assert_eq!(e.degree(), c.degree() + times);
        same_curve(&c, &e, &mut rng, &format!("{data:?} elevate x{times}"));
        points_are_blossoms(&e, &e.clone());
    }
}

#[test]
fn bad_data_is_refused_with_its_reason() {
    let pts = [[0.0], [1.0], [2.0]];
    let k = [0.0, 0.0, 0.0, 1.0, 1.0, 1.0];
    assert_eq!(
        Curve::new(2, &k[1..], &pts, None),
        Err(Error::KnotCount {
            expected: 6,
            found: 5
        })
    );
    assert_eq!(Curve::new(3, &k, &pts, None), Err(Error::TooFewPoints));
    assert_eq!(
        Curve::new(2, &[0.0, 0.0, 0.0, 1.0, 0.5, 1.0], &pts, None),
        Err(Error::KnotsDecrease)
    );
    assert_eq!(
        Curve::new(1, &[0.0, 0.0, 0.0, 1.0, 1.0], &[[0.0], [1.0], [2.0]], None).map(|_| ()),
        Err(Error::Multiplicity)
    );
    assert_eq!(
        Curve::new(2, &[0.0, 0.5, 1.0, 1.0, 2.0, 3.0], &pts, None),
        Err(Error::EmptyDomain)
    );
    assert_eq!(
        Curve::new(2, &k, &pts, Some(&[1.0, 0.0, 1.0])),
        Err(Error::ZeroWeight)
    );
    assert_eq!(
        Curve::new(2, &k, &pts, Some(&[1.0, 1.0])),
        Err(Error::WeightCount {
            expected: 3,
            found: 2
        })
    );
    assert_eq!(
        Curve::new(2, &k, &[[f64::NAN], [1.0], [2.0]], None),
        Err(Error::NonFinite)
    );
    let c = Curve::new(2, &k, &pts, None).unwrap();
    assert_eq!(c.insert_knot(1.5, 1), Err(Error::OutsideDomain));
    assert_eq!(c.insert_knot(0.0, 1), Err(Error::Multiplicity));
    let unclamped = Curve::new(2, &[0.0, 1.0, 2.0, 3.0, 4.0, 5.0], &pts, None).unwrap();
    assert_eq!(unclamped.elevate(1), Err(Error::Unclamped));
    assert_eq!(unclamped.domain(), (2.0, 3.0));
    // Weights (1, -1, 1): w(u) = (1 - u)^2 - 2u(1 - u) + u^2 = (1 - 2u)^2, zero at 1/2.
    let zero = Curve::new(2, &k, &pts, Some(&[1.0, -1.0, 1.0])).unwrap();
    assert_eq!(zero.point(0.5, Side::Above), Err(Error::ZeroWeight));
    let e: &dyn std::error::Error = &Error::Unclamped;
    assert!(e.to_string().contains("end knots"));
}

#[test]
fn rounding_gives_back_the_data_when_nothing_was_done() {
    for data in curves(80, 2) {
        assert_eq!(data.exact().unwrap().rounded(), data);
    }
    let mut rng = Rng::new(8);
    let s = gen::surface::<3>(
        &mut rng,
        (
            Spec {
                degree: 2,
                count: 4,
                knots: Knots::Random,
            },
            Spec {
                degree: 3,
                count: 5,
                knots: Knots::Multiple,
            },
        ),
        Weights::Moderate,
    );
    assert_eq!(s.exact().unwrap().rounded(), s);
}

// ---------------------------------------------------------------- surfaces

fn surfaces(seed: u64, per_kind: usize) -> Vec<scx_testkit::data::SurfaceData<3>> {
    let mut rng = Rng::new(seed);
    let mut out = Vec::new();
    for (ki, knots) in KNOTS.into_iter().enumerate() {
        for weights in WEIGHTS {
            for i in 0..per_kind {
                let du = 1 + (i + ki) % 3;
                let dv = 1 + (i * 2 + ki) % 3;
                let su = Spec {
                    degree: du,
                    count: du + 1 + i % 3,
                    knots,
                };
                let sv = Spec {
                    degree: dv,
                    count: dv + 1 + (i + 1) % 3,
                    knots: KNOTS[(ki + i) % 4],
                };
                out.push(gen::surface(&mut rng, (su, sv), weights));
            }
        }
    }
    out
}

fn surface_samples(rng: &mut Rng, s: &Surface<3>, extra: usize) -> Vec<(f64, f64)> {
    let (ku, kv) = s.knots();
    let (du, dv) = s.domain();
    let us = samples(rng, ku, du, extra);
    let vs = samples(rng, kv, dv, extra);
    us.iter()
        .enumerate()
        .map(|(i, &u)| (u, vs[(i * 7 + 1) % vs.len()]))
        .chain(
            vs.iter()
                .enumerate()
                .map(|(i, &v)| (us[(i * 5) % us.len()], v)),
        )
        .collect()
}

const SIDE2: [(Side, Side); 4] = [
    (Side::Below, Side::Below),
    (Side::Below, Side::Above),
    (Side::Above, Side::Below),
    (Side::Above, Side::Above),
];

#[test]
fn surface_de_boor_and_basis_sum_agree_exactly() {
    let mut rng = Rng::new(9);
    let mut checked = 0;
    for data in surfaces(90, 2) {
        let s = data.exact().unwrap();
        for uv in surface_samples(&mut rng, &s, 2) {
            for side in SIDE2 {
                let a = s.derivs(uv, side, 2).unwrap();
                let b = s.derivs_by_basis(uv, side, 2).unwrap();
                assert_eq!(a, b, "{data:?} {uv:?} {side:?}");
                checked += 1;
            }
        }
    }
    assert!(checked > 500, "{checked}");
}

#[test]
fn a_surface_extruded_from_a_curve_is_that_curve() {
    let mut rng = Rng::new(10);
    for data in curves(100, 2) {
        let (nu, nv) = (data.points.len(), 3);
        let points: Vec<[f64; 3]> = data
            .points
            .iter()
            .flat_map(|p| core::iter::repeat_n(*p, nv))
            .collect();
        let weights = data.weights.as_ref().map(|w| {
            w.iter()
                .flat_map(|&x| core::iter::repeat_n(x, nv))
                .collect::<Vec<_>>()
        });
        let s = Surface::new(
            (data.degree, 2),
            &data.knots,
            &[0.0, 0.0, 0.0, 1.0, 1.0, 1.0],
            &points,
            weights.as_deref(),
        )
        .unwrap();
        assert_eq!(s.counts(), (nu, nv));
        let c = data.exact().unwrap();
        for u in samples(&mut rng, &data.knots, c.domain(), 3) {
            let v = rng.next_f64();
            let d = s.derivs((u, v), (Side::Above, Side::Below), 2).unwrap();
            let cd = c.derivs(u, Side::Above, 2).unwrap();
            for k in 0..=2 {
                assert_eq!(d.get(k, 0), Some(&cd[k]));
            }
            let zero = Exact {
                exact: <[Rat; 3]>::default(),
                rounded: [0.0; 3],
            };
            assert_eq!(d.get(0, 1), Some(&zero));
            assert_eq!(d.get(1, 1), Some(&zero));
            assert_eq!(d.get(0, 2), Some(&zero));
        }
    }
}

fn same_surface(a: &Surface<3>, b: &Surface<3>, rng: &mut Rng, what: &str) {
    for uv in surface_samples(rng, b, 2) {
        for side in SIDE2 {
            assert_eq!(
                a.derivs(uv, side, 2).unwrap(),
                b.derivs(uv, side, 2).unwrap(),
                "{what} {uv:?} {side:?}"
            );
        }
    }
}

#[test]
fn surface_insertion_and_elevation_leave_the_shape_exactly_unchanged() {
    let mut rng = Rng::new(11);
    for data in surfaces(110, 1) {
        let s = data.exact().unwrap();
        let (du, dv) = s.domain();
        let u = gen::parameter(&mut rng, &data.knots_u, du);
        let v = gen::parameter(&mut rng, &data.knots_v, dv);
        if let Ok(t) = s.insert_knot_u(u, 1) {
            same_surface(&s, &t, &mut rng, &format!("{data:?} insert u {u}"));
        }
        if let Ok(t) = s.insert_knot_v(v, 1) {
            same_surface(&s, &t, &mut rng, &format!("{data:?} insert v {v}"));
        }
        let e = s.elevate_u(1).unwrap().elevate_v(1).unwrap();
        assert_eq!(e.degrees(), (data.degrees.0 + 1, data.degrees.1 + 1));
        same_surface(&s, &e, &mut rng, &format!("{data:?} elevate"));
    }
}
