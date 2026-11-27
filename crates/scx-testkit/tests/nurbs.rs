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
use scx_testkit::Rat;
use scx_testkit::data::CurveData;
use scx_testkit::nurbs::{Curve, Error, Exact, Side, Surface};
use scx_testkit::sample::{self, Knots, Spec, Weights};

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
    curves_from(seed, per_kind, false)
}

/// `curves`, or with `spread` the degrees shifted per kind, so a few curves per kind still
/// reach every degree across the kinds.
fn curves_if(seed: u64, per_kind: usize, spread: bool) -> Vec<CurveData<3>> {
    curves_from(seed, per_kind, spread)
}

fn curves_from(seed: u64, per_kind: usize, spread: bool) -> Vec<CurveData<3>> {
    let mut rng = Rng::new(seed);
    let mut out = Vec::new();
    let mut kind = 0;
    for knots in KNOTS {
        for weights in WEIGHTS {
            kind += 1;
            for i in 0..per_kind {
                let degree = 1 + (i + if spread { kind } else { 0 }) % 4;
                let count = degree + 1 + (i * 7 + 3) % 5;
                let spec = Spec {
                    degree,
                    count,
                    knots,
                };
                out.push(sample::curve(&mut rng, spec, weights));
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
        out.push(sample::parameter(rng, knots, domain));
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
        let pts = sample::points::<2>(&mut rng, p + 1, 4.0);
        let w = sample::weights(&mut rng, p + 1, Weights::Moderate).unwrap();
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

fn de_boor_sweep(per_kind: usize, spread: bool, extra: usize, min_checked: usize) {
    let mut rng = Rng::new(3);
    let mut checked = 0;
    for data in curves_if(30, per_kind, spread) {
        let c = data.exact().unwrap();
        for u in samples(&mut rng, &data.knots, c.domain(), extra) {
            for side in SIDES {
                let a = c.derivs(u, side, 3).unwrap();
                let b = c.derivs_by_basis(u, side, 3).unwrap();
                assert_eq!(a, b, "{data:?} u {u} {side:?}");
                checked += 1;
            }
        }
    }
    assert!(checked > min_checked, "{checked}");
}

#[test]
fn de_boor_and_the_basis_sum_agree_exactly() {
    de_boor_sweep(2, true, 1, 50);
}

/// Full sweep (6 curves per kind, 6 extra samples per curve); run with
/// `cargo test -p scx-testkit --test nurbs -- --ignored`.
#[test]
#[ignore = "slow exhaustive sweep"]
fn de_boor_and_the_basis_sum_agree_exactly_full() {
    de_boor_sweep(6, false, 6, 1000);
}

fn multiplicity_sweep(per_kind: usize, min_joins: usize) {
    let mut rng = Rng::new(4);
    let mut joins = 0;
    for data in curves(40, per_kind) {
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
        let u = sample::parameter(&mut rng, &[], (a, b));
        if !data.knots.contains(&u) {
            assert_eq!(c.derivs(u, Side::Below, 2), c.derivs(u, Side::Above, 2));
        }
    }
    assert!(joins > min_joins, "{joins}");
}

#[test]
fn a_knot_of_multiplicity_m_joins_its_pieces_with_p_minus_m_derivatives() {
    multiplicity_sweep(4, 40);
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

/// Full sweep (8 curves per kind); run with `cargo test -p scx-testkit --test nurbs -- --ignored`.
#[test]
#[ignore = "slow exhaustive sweep"]
fn a_knot_of_multiplicity_m_joins_its_pieces_with_p_minus_m_derivatives_full() {
    multiplicity_sweep(8, 100);
}

/// Every sample of `a` and `b` agrees exactly, derivatives to order 2 on both sides.
fn same_curve(a: &Curve<3>, b: &Curve<3>, rng: &mut Rng, extra: usize, what: &str) {
    assert_eq!(a.domain(), b.domain());
    // b's knots: an inserted knot is sampled on both its sides.
    for u in samples(rng, b.knots(), a.domain(), extra) {
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

fn insertion_sweep(per_kind: usize, spread: bool, rounds: usize, extra: usize) {
    let mut rng = Rng::new(5);
    for data in curves_if(50, per_kind, spread) {
        let c = data.exact().unwrap();
        let p = c.degree();
        for _ in 0..rounds {
            let u = sample::parameter(&mut rng, &data.knots, c.domain());
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
                extra,
                &format!("{data:?} insert {u} x{times}"),
            );
            points_are_blossoms(&inserted, &c);
        }
    }
}

#[test]
fn knot_insertion_leaves_the_shape_exactly_unchanged() {
    insertion_sweep(2, true, 1, 1);
}

/// Full sweep (5 curves per kind, 3 insertions each); run with
/// `cargo test -p scx-testkit --test nurbs -- --ignored`.
#[test]
#[ignore = "slow exhaustive sweep"]
fn knot_insertion_leaves_the_shape_exactly_unchanged_full() {
    insertion_sweep(5, false, 3, 8);
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
        let pts = sample::points::<3>(&mut rng, p + 1, 2.0);
        let w = sample::weights(&mut rng, p + 1, Weights::Moderate).unwrap();
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
        same_curve(&c, &e, &mut rng, 8, "bezier");
    }
}

fn elevation_sweep(per_kind: usize, spread: bool, max_times: u64, extra: usize) {
    let mut rng = Rng::new(7);
    for data in curves_if(70, per_kind, spread) {
        let c = data.exact().unwrap();
        let times = 1 + gen_below(&mut rng, max_times);
        let e = c.elevate(times).unwrap();
        assert_eq!(e.degree(), c.degree() + times);
        same_curve(
            &c,
            &e,
            &mut rng,
            extra,
            &format!("{data:?} elevate x{times}"),
        );
        points_are_blossoms(&e, &e.clone());
    }
}

#[test]
fn degree_elevation_leaves_the_shape_exactly_unchanged() {
    elevation_sweep(2, true, 1, 1);
}

/// Full sweep (4 curves per kind, elevated by 1 or 2 degrees, 8 extra samples); run with
/// `cargo test -p scx-testkit --test nurbs -- --ignored`.
#[test]
#[ignore = "slow exhaustive sweep"]
fn degree_elevation_leaves_the_shape_exactly_unchanged_full() {
    elevation_sweep(4, false, 2, 8);
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
    let s = sample::surface::<3>(
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
                out.push(sample::surface(&mut rng, (su, sv), weights));
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

fn surface_de_boor_sweep(per_kind: usize, extra: usize, min_checked: usize) {
    let mut rng = Rng::new(9);
    let mut checked = 0;
    for data in surfaces(90, per_kind) {
        let s = data.exact().unwrap();
        for uv in surface_samples(&mut rng, &s, extra) {
            for side in SIDE2 {
                let a = s.derivs(uv, side, 2).unwrap();
                let b = s.derivs_by_basis(uv, side, 2).unwrap();
                assert_eq!(a, b, "{data:?} {uv:?} {side:?}");
                checked += 1;
            }
        }
    }
    assert!(checked > min_checked, "{checked}");
}

#[test]
fn surface_de_boor_and_basis_sum_agree_exactly() {
    surface_de_boor_sweep(1, 0, 30);
}

/// Full sweep (2 surfaces per kind, 2 extra samples per direction); run with
/// `cargo test -p scx-testkit --test nurbs -- --ignored`.
#[test]
#[ignore = "slow exhaustive sweep"]
fn surface_de_boor_and_basis_sum_agree_exactly_full() {
    surface_de_boor_sweep(2, 2, 500);
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

fn same_surface(a: &Surface<3>, b: &Surface<3>, rng: &mut Rng, extra: usize, what: &str) {
    for uv in surface_samples(rng, b, extra) {
        for side in SIDE2 {
            assert_eq!(
                a.derivs(uv, side, 2).unwrap(),
                b.derivs(uv, side, 2).unwrap(),
                "{what} {uv:?} {side:?}"
            );
        }
    }
}

fn surface_edit_sweep(per_kind: usize, extra: usize) {
    let mut rng = Rng::new(11);
    for data in surfaces(110, per_kind) {
        let s = data.exact().unwrap();
        let (du, dv) = s.domain();
        let u = sample::parameter(&mut rng, &data.knots_u, du);
        let v = sample::parameter(&mut rng, &data.knots_v, dv);
        if let Ok(t) = s.insert_knot_u(u, 1) {
            same_surface(&s, &t, &mut rng, extra, &format!("{data:?} insert u {u}"));
        }
        if let Ok(t) = s.insert_knot_v(v, 1) {
            same_surface(&s, &t, &mut rng, extra, &format!("{data:?} insert v {v}"));
        }
        let e = s.elevate_u(1).unwrap().elevate_v(1).unwrap();
        assert_eq!(e.degrees(), (data.degrees.0 + 1, data.degrees.1 + 1));
        same_surface(&s, &e, &mut rng, extra, &format!("{data:?} elevate"));
    }
}

#[test]
fn surface_insertion_and_elevation_leave_the_shape_exactly_unchanged() {
    surface_edit_sweep(1, 0);
}

/// Full sweep (12 surfaces, 2 extra samples per direction); run with
/// `cargo test -p scx-testkit --test nurbs -- --ignored`.
#[test]
#[ignore = "slow exhaustive sweep"]
fn surface_insertion_and_elevation_leave_the_shape_exactly_unchanged_full() {
    surface_edit_sweep(1, 2);
}

// ---------------------------------------------------------------- order 3 and higher; splitting

/// Random surfaces of degrees up to 4 in each direction.
fn high_surfaces(seed: u64, count: usize) -> Vec<scx_testkit::data::SurfaceData<3>> {
    let mut rng = Rng::new(seed);
    (0..count)
        .map(|i| {
            let (du, dv) = (1 + i % 4, 1 + (i * 3 + 2) % 4);
            let spec = |degree: usize, k: usize| Spec {
                degree,
                count: degree + 1 + k % 3,
                knots: KNOTS[k % 4],
            };
            sample::surface(&mut rng, (spec(du, i), spec(dv, i + 1)), WEIGHTS[i % 3])
        })
        .collect()
}

fn order_five_sweep(per_surface: usize, min_checked: usize) {
    let mut rng = Rng::new(12);
    let mut checked = 0;
    for data in high_surfaces(120, 12) {
        let s = data.exact().unwrap();
        for uv in surface_samples(&mut rng, &s, 0)
            .into_iter()
            .take(per_surface)
        {
            let side = SIDE2[checked % 4];
            let a = s.derivs(uv, side, 5).unwrap();
            let b = s.derivs_by_basis(uv, side, 5).unwrap();
            assert_eq!(a.order(), 5);
            assert_eq!(a, b, "{data:?} {uv:?} {side:?}");
            checked += 1;
        }
    }
    assert!(checked > min_checked, "{checked}");
}

#[test]
fn surface_de_boor_and_basis_sum_agree_exactly_to_order_five() {
    order_five_sweep(2, 20);
}

/// Full sweep (5 samples per surface); run with `cargo test -p scx-testkit --test nurbs -- --ignored`.
#[test]
#[ignore = "slow exhaustive sweep"]
fn surface_de_boor_and_basis_sum_agree_exactly_to_order_five_full() {
    order_five_sweep(5, 50);
}

/// The power-basis coefficients of the Bernstein polynomials of degree `p`:
/// `B_i(t) = sum_k c[i][k] t^k`.
fn bernstein_to_power(p: usize) -> Vec<Vec<Rat>> {
    (0..=p)
        .map(|i| {
            (0..=p)
                .map(|k| {
                    if k < i {
                        Rat::zero()
                    } else {
                        let c = binomial(p, i) * binomial(p - i, k - i);
                        Rat::from(if (k - i) % 2 == 0 { c } else { -c })
                    }
                })
                .collect()
        })
        .collect()
}

/// `d^k/dt^k t^a` at `t`.
fn power_deriv(a: usize, k: usize, t: &Rat) -> Rat {
    if k > a {
        return Rat::zero();
    }
    let falling = (a - k + 1..=a).fold(1i64, |f, x| f * x as i64);
    &Rat::from(falling) * &pow(t, a - k)
}

/// The partials up to `order` of the Bezier patch of degrees `(p, q)` on
/// `[u0, u1] x [v0, v1]` with homogeneous points `h[i][j]` (`w P`, `w`), by expanding to the
/// power basis and differentiating each monomial, then the quotient rule (Leibniz):
/// `S^(k,l) = (A^(k,l) - sum_{(i,j) != 0} C(k,i) C(l,j) w^(i,j) S^(k-i,l-j)) / w`.
fn power_truth(
    (p, q): (usize, usize),
    (du, dv): ((f64, f64), (f64, f64)),
    h: &[Vec<[Rat; 4]>],
    (u, v): (f64, f64),
    order: usize,
) -> Vec<Vec<[Rat; 3]>> {
    let (cu, cv) = (bernstein_to_power(p), bernstein_to_power(q));
    // the monomial coefficients of the homogeneous polynomial in (s, t)
    let mut mono = vec![vec![[Rat::zero(), Rat::zero(), Rat::zero(), Rat::zero()]; q + 1]; p + 1];
    for (i, row) in h.iter().enumerate() {
        for (j, hij) in row.iter().enumerate() {
            for a in 0..=p {
                for b in 0..=q {
                    let c = &cu[i][a] * &cv[j][b];
                    for (m, x) in mono[a][b].iter_mut().zip(hij) {
                        *m = &*m + &(&c * x);
                    }
                }
            }
        }
    }
    let local = |x: f64, (a, b): (f64, f64)| (&r(x) - &r(a)).checked_div(&(&r(b) - &r(a))).unwrap();
    let (s, t) = (local(u, du), local(v, dv));
    let (ku, kv) = (
        (&r(du.1) - &r(du.0)).recip().unwrap(),
        (&r(dv.1) - &r(dv.0)).recip().unwrap(),
    );
    let homo = |k: usize, l: usize| -> [Rat; 4] {
        let mut out = [Rat::zero(), Rat::zero(), Rat::zero(), Rat::zero()];
        for a in 0..=p {
            for b in 0..=q {
                let f = &power_deriv(a, k, &s) * &power_deriv(b, l, &t);
                for (o, m) in out.iter_mut().zip(&mono[a][b]) {
                    *o = &*o + &(&f * m);
                }
            }
        }
        let scale = &pow(&ku, k) * &pow(&kv, l);
        out.map(|x| &x * &scale)
    };
    let a: Vec<Vec<[Rat; 4]>> = (0..=order)
        .map(|k| (0..=order - k).map(|l| homo(k, l)).collect())
        .collect();
    let w0 = a[0][0][3].recip().unwrap();
    let mut out: Vec<Vec<[Rat; 3]>> = Vec::new();
    for k in 0..=order {
        let mut row = Vec::new();
        for l in 0..=order - k {
            let mut num: [Rat; 3] = core::array::from_fn(|c| a[k][l][c].clone());
            for i in 0..=k {
                for j in 0..=l {
                    if i == 0 && j == 0 {
                        continue;
                    }
                    let c = Rat::from(binomial(k, i) * binomial(l, j));
                    let wij = &c * &a[i][j][3];
                    let prev: &[Rat; 3] = if i == 0 {
                        &row[l - j]
                    } else {
                        &out[k - i][l - j]
                    };
                    for (n, x) in num.iter_mut().zip(prev) {
                        *n = &*n - &(&wij * x);
                    }
                }
            }
            row.push(num.map(|n| &n * &w0));
        }
        out.push(row);
    }
    out
}

#[test]
fn high_order_partials_are_the_power_basis_truth_before_and_after_insertion() {
    let mut rng = Rng::new(13);
    let mut checked = 0;
    for n in 0..8 {
        let (p, q) = (1 + n % 4, 1 + (n * 3 + 1) % 4);
        // dyadic ends keep the exact values small
        let du = (-1.0 - (n % 3) as f64 * 0.25, 2.5);
        let dv = (0.5 * (n % 2) as f64 - 1.0, 3.0);
        let ends = |d: (f64, f64), p: usize| {
            let mut k = vec![d.0; p + 1];
            k.extend(vec![d.1; p + 1]);
            k
        };
        let pts = sample::points::<3>(&mut rng, (p + 1) * (q + 1), 2.0);
        let w = if n % 2 == 0 {
            vec![1.0; pts.len()]
        } else {
            sample::weights(&mut rng, pts.len(), Weights::Moderate).unwrap()
        };
        let s = Surface::new((p, q), &ends(du, p), &ends(dv, q), &pts, Some(&w)).unwrap();
        let h: Vec<Vec<[Rat; 4]>> = (0..=p)
            .map(|i| {
                (0..=q)
                    .map(|j| {
                        let k = i * (q + 1) + j;
                        let wk = r(w[k]);
                        let x = pts[k].map(|c| &r(c) * &wk);
                        [x[0].clone(), x[1].clone(), x[2].clone(), wk]
                    })
                    .collect()
            })
            .collect();
        // past a degree, so vanishing partials are checked too
        let order = (p + q + 1).min(6);
        // parameters of 16 bits keep the exact values small
        let mut short = || (rng.next_u64() % (1 << 16)) as f64 / 65536.0;
        let iu = du.0 + (du.1 - du.0) * short();
        let iv = dv.0 + (dv.1 - dv.0) * short();
        let inserted = s
            .insert_knot_u(iu, 1 + n % p.max(1))
            .unwrap()
            .insert_knot_v(iv, 1)
            .unwrap();
        for _ in 0..2 {
            let uv = (
                du.0 + (du.1 - du.0) * short(),
                dv.0 + (dv.1 - dv.0) * short(),
            );
            let want = power_truth((p, q), (du, dv), &h, uv, order);
            for t in [&s, &inserted] {
                let got = t.derivs(uv, SIDE2[checked % 4], order).unwrap();
                for (k, row) in want.iter().enumerate() {
                    for (l, x) in row.iter().enumerate() {
                        assert_eq!(&got.get(k, l).unwrap().exact, x, "n {n} {uv:?} ({k}, {l})");
                    }
                }
                checked += 1;
            }
        }
    }
    assert_eq!(checked, 32);
}

/// The `u`-lines (fixed `j`) or `v`-lines (fixed `i`) of a surface as curves of its data.
fn lines(data: &scx_testkit::data::SurfaceData<3>, along_u: bool) -> Vec<Curve<3>> {
    let (nu, nv) = (
        data.knots_u.len() - data.degrees.0 - 1,
        data.knots_v.len() - data.degrees.1 - 1,
    );
    let (outer, inner) = if along_u { (nv, nu) } else { (nu, nv) };
    (0..outer)
        .map(|o| {
            let at = |i: usize| if along_u { i * nv + o } else { o * nv + i };
            let pts: Vec<[f64; 3]> = (0..inner).map(|i| data.points[at(i)]).collect();
            let w: Option<Vec<f64>> = data
                .weights
                .as_ref()
                .map(|w| (0..inner).map(|i| w[at(i)]).collect());
            let (p, k) = if along_u {
                (data.degrees.0, &data.knots_u)
            } else {
                (data.degrees.1, &data.knots_v)
            };
            Curve::new(p, k, &pts, w.as_deref()).unwrap()
        })
        .collect()
}

/// The homogeneous control points of a surface's `u`- or `v`-lines.
fn surface_lines(s: &Surface<3>, along_u: bool) -> Vec<(Vec<[Rat; 3]>, Vec<Rat>)> {
    let (nu, nv) = s.counts();
    let (pts, w) = (s.points(), s.weights());
    let (outer, inner) = if along_u { (nv, nu) } else { (nu, nv) };
    (0..outer)
        .map(|o| {
            let at = |i: usize| if along_u { i * nv + o } else { o * nv + i };
            (
                (0..inner).map(|i| pts[at(i)].clone()).collect(),
                (0..inner).map(|i| w[at(i)].clone()).collect(),
            )
        })
        .collect()
}

fn same_lines(s: &Surface<3>, curves: &[Curve<3>], along_u: bool, what: &str) {
    let got = surface_lines(s, along_u);
    assert_eq!(got.len(), curves.len(), "{what}");
    for (g, c) in got.iter().zip(curves) {
        assert_eq!(g.0, c.points(), "{what}");
        assert_eq!(g.1, c.weights(), "{what}");
    }
}

#[test]
fn tensor_product_insertion_elevation_and_split_are_the_curve_operations_per_line() {
    let mut rng = Rng::new(14);
    for data in high_surfaces(140, 8) {
        let s = data.exact().unwrap();
        let (du, dv) = s.domain();
        let u = sample::parameter(&mut rng, &data.knots_u, du);
        let v = sample::parameter(&mut rng, &data.knots_v, dv);
        for along_u in [true, false] {
            let (t, k) = if along_u {
                (u, &data.knots_u)
            } else {
                (v, &data.knots_v)
            };
            let curves = lines(&data, along_u);
            let what = format!("{data:?} along_u {along_u} at {t}");
            let ins = if along_u {
                s.insert_knot_u(t, 2)
            } else {
                s.insert_knot_v(t, 2)
            };
            let each: Result<Vec<_>, _> = curves.iter().map(|c| c.insert_knot(t, 2)).collect();
            match (ins, each) {
                (Ok(a), Ok(b)) => {
                    same_lines(&a, &b, along_u, &what);
                    let knots = if along_u { a.knots().0 } else { a.knots().1 };
                    assert_eq!(knots, b[0].knots(), "{what}");
                }
                (a, b) => assert_eq!(a.err(), b.err(), "{what}"),
            }
            let el = if along_u {
                s.elevate_u(2)
            } else {
                s.elevate_v(2)
            }
            .unwrap();
            let each: Vec<_> = curves.iter().map(|c| c.elevate(2).unwrap()).collect();
            same_lines(&el, &each, along_u, &what);
            let mid = {
                let (a, b) = if along_u { du } else { dv };
                let x = sample::parameter(&mut rng, k, (a, b));
                if a < x && x < b { x } else { a + (b - a) * 0.5 }
            };
            let (lo, hi) = if along_u {
                s.split_u(mid)
            } else {
                s.split_v(mid)
            }
            .unwrap();
            let halves: Vec<_> = curves.iter().map(|c| c.split(mid).unwrap()).collect();
            let (l, h): (Vec<_>, Vec<_>) = halves.into_iter().unzip();
            same_lines(&lo, &l, along_u, &what);
            same_lines(&hi, &h, along_u, &what);
        }
    }
}

#[test]
fn tensor_product_operations_commute_across_directions() {
    let mut rng = Rng::new(15);
    for data in high_surfaces(150, 8) {
        let s = data.exact().unwrap();
        let (du, dv) = s.domain();
        let u = sample::parameter(&mut rng, &data.knots_u, du);
        let v = sample::parameter(&mut rng, &data.knots_v, dv);
        if let (Ok(a), Ok(b)) = (
            s.insert_knot_u(u, 1).and_then(|t| t.insert_knot_v(v, 2)),
            s.insert_knot_v(v, 2).and_then(|t| t.insert_knot_u(u, 1)),
        ) {
            assert_eq!(a, b, "{data:?} insert {u} {v}");
        }
        let a = s.elevate_u(1).unwrap().elevate_v(2).unwrap();
        let b = s.elevate_v(2).unwrap().elevate_u(1).unwrap();
        assert_eq!(a, b, "{data:?} elevate");
    }
}

#[test]
fn split_halves_are_the_surface_on_each_side() {
    let mut rng = Rng::new(16);
    for data in high_surfaces(160, 6) {
        let s = data.exact().unwrap();
        let ((u0, u1), (v0, v1)) = s.domain();
        let u = u0 + (u1 - u0) * (0.1 + 0.8 * rng.next_f64());
        let v = data.knots_v[data.knots_v.len() / 2].clamp(v0, v1);
        let v = if v0 < v && v < v1 {
            v
        } else {
            v0 + (v1 - v0) * 0.5
        };
        for (along_u, t) in [(true, u), (false, v)] {
            let (lo, hi) = if along_u { s.split_u(t) } else { s.split_v(t) }.unwrap();
            let what = format!("{data:?} along_u {along_u} at {t}");
            let (ld, hd) = (lo.domain(), hi.domain());
            let (a, b) = if along_u { (ld.0, hd.0) } else { (ld.1, hd.1) };
            assert_eq!((a.1, b.0), (t, t), "{what}");
            for uv in surface_samples(&mut rng, &s, 0) {
                let x = if along_u { uv.0 } else { uv.1 };
                let side = SIDE2[(x.to_bits() % 4) as usize];
                let want = s.derivs(uv, side, 3).unwrap();
                let below = if along_u { side.0 } else { side.1 } == Side::Below;
                // a half holds the side of `t` it lies on, with that side's limits at `t`
                if x < t || (x == t && below) {
                    assert_eq!(lo.derivs(uv, side, 3).unwrap(), want, "{what} {uv:?} lo");
                }
                if x > t || (x == t && !below) {
                    assert_eq!(hi.derivs(uv, side, 3).unwrap(), want, "{what} {uv:?} hi");
                }
            }
        }
        assert_eq!(s.split_u(u0).err(), Some(Error::OutsideDomain));
        assert_eq!(s.split_v(v1).err(), Some(Error::OutsideDomain));
    }
}

#[test]
fn curve_split_halves_are_the_curve_on_each_side() {
    let mut rng = Rng::new(17);
    for data in curves(170, 2) {
        let c = data.exact().unwrap();
        let (a, b) = c.domain();
        let u = a + (b - a) * (0.1 + 0.8 * rng.next_f64());
        let (lo, hi) = c.split(u).unwrap();
        assert_eq!((lo.domain(), hi.domain()), ((a, u), (u, b)), "{data:?}");
        for x in samples(&mut rng, &data.knots, (a, b), 3)
            .into_iter()
            .chain([u])
        {
            for side in SIDES {
                let want = c.derivs(x, side, 3).unwrap();
                if x < u || (x == u && side == Side::Below) {
                    assert_eq!(lo.derivs(x, side, 3).unwrap(), want, "{data:?} {u} {x}");
                }
                if x > u || (x == u && side == Side::Above) {
                    assert_eq!(hi.derivs(x, side, 3).unwrap(), want, "{data:?} {u} {x}");
                }
            }
        }
        assert_eq!(c.split(a).err(), Some(Error::OutsideDomain));
        assert_eq!(c.split(f64::NAN).err(), Some(Error::NonFinite));
    }
}
