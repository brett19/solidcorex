// Written for solidcorex. Apache-2.0.
//! `scx-algo`: `bernstein.rs` (the pin's `offset/construction/bernstein.rs`, with
//! `query/tangent_edge.rs`'s `bernstein_roots` made public by the build script).
//!
//! The `bern_*` diffs compare every operation over the recipes of [`crate::berns`] (degrees
//! up to (5, 2) and (4, 4), coefficients ordinary, over five decades, zero, one, short
//! dyadics, and with one edge value among them): the coefficients, the degrees, the error
//! bounds and each refusal's operation and reason.
//!
//! The `truth_*` tests hold scx alone to exact arithmetic (`scx_testkit::Rat`): each
//! result's exact value (exact de Casteljau of its coefficients) against the exact value of
//! the operation on its operands, each bounded form's coefficients within their bound of the
//! exact ones, and the roots of polynomials with known dyadic roots.

use crate::berns::{BdR, BernR, V3R};
use crate::diff;
use crate::harness::List;

diff!(bern_new, |du: usize, dv: usize, c: List<f64>| {
    (
        m::bern::new(&(du, dv, c.0.clone())),
        m::bern::extremes(&(du, dv, c.0.clone())),
        c.0.first().map(|&x| m::bern::constant(x)),
    )
});

diff!(bern_new_extreme, || {
    let mx = scx_algo::bernstein::MAX_DEGREE;
    (
        (
            m::bern::new(&(usize::MAX, usize::MAX, Vec::new())),
            m::bern::new(&(0, mx + 1, vec![0.0; mx + 2])),
            m::bern::new(&(mx + 1, 0, vec![0.0; mx + 2])),
            m::bern::new(&(mx, 0, vec![1.0; mx + 1])),
            m::bern::new(&(1, 1, vec![0.0; 3])),
        ),
        m::bern::mul(&(mx, 0, vec![1.0; mx + 1]), &(1, 0, vec![1.0; 2])),
        m::bern::elevate(&(mx, 0, vec![1.0; mx + 1]), mx, 1),
        m::bern::elevate(&(1, 1, vec![1.0; 4]), mx + 1, 1),
    )
});

diff!(bern_binomial, |n: usize, k: usize| m::bern::binomial(n, k));

diff!(bern_binomial_all, || {
    let mut v = Vec::new();
    for n in 0..=2 * scx_algo::bernstein::MAX_DEGREE {
        for k in 0..=n + 1 {
            v.push(m::bern::binomial(n, k));
        }
    }
    v
});

diff!(bern_ops, |p: BernR, q: BernR, x: f64| {
    let (p, q) = (&p.0, &q.0);
    (
        m::bern::mul(p, q),
        m::bern::add_scaled(p, q, x),
        m::bern::add(p, q),
        m::bern::sub(p, q),
        m::bern::scale(p, x),
        m::bern::deriv_u(p),
        m::bern::deriv_v(p),
    )
});

diff!(bern_elevate, |p: BernR, a: usize, b: usize| {
    let p = &p.0;
    (
        m::bern::elevate(p, p.0 + a, p.1 + b),
        m::bern::elevate(p, a, b),
        m::bern::extremes(p),
    )
});

diff!(bern_split, |p: BernR, t: f64, u: f64, v: f64| {
    let p = &p.0;
    // the raw arguments, and fractions made of them
    let f = |x: f64| x.abs().fract();
    (
        m::bern::split_u(p, t),
        m::bern::split_v(p, t),
        m::bern::restrict(p, t, u, v, t),
        m::bern::restrict(p, f(t), f(u), f(v), 1.0 - f(t)),
        m::bern::eval(p, u, v),
        m::bern::eval(p, f(u), f(v)),
    )
});

diff!(bern_bounded, |p: BdR, q: BdR, x: f64| {
    let (p, q) = (&p.0, &q.0);
    (
        (
            m::bern::bmul(p, q),
            m::bern::badd_scaled(p, q, x),
            m::bern::badd(p, q),
            m::bern::bsub(p, q),
            m::bern::bscale(p, x),
        ),
        (
            m::bern::bderiv_u(p),
            m::bern::bderiv_v(p),
            m::bern::bfinite(p),
            m::bern::bquarter(p, false, false),
            m::bern::bquarter(p, true, false),
            m::bern::bquarter(p, false, true),
            m::bern::bquarter(p, true, true),
        ),
    )
});

diff!(bern_vec3, |a: V3R, c: V3R, s: BernR| {
    let (a, c, s) = (&a.0, &c.0, &s.0);
    (
        (
            m::bern::dot(a, c),
            m::bern::cross(a, c),
            m::bern::sub3(a, c),
            m::bern::mul3(a, s),
            m::bern::deriv_u3(a),
            m::bern::deriv_v3(a),
            m::bern::restrict3(a, 0.25, 0.7, 0.1, 0.9),
        ),
        (
            m::bern::eval3(a, 0.3, 0.6),
            m::bern::eval3(a, 1.0, 0.0),
            m::bern::bdot(a, c, 1e-15),
            m::bern::bcross(a, c, 0.0),
            m::bern::bsub3(a, c, 3e-12),
            m::bern::bmul3(a, s, 1e-15),
        ),
        (
            m::bern::bderiv_u3(a, 1e-15),
            m::bern::bderiv_v3(a, 0.5),
            m::bern::unbounded3(a, 1e-15),
        ),
    )
});

diff!(bern_product_rounding, |du: usize, dv: usize| {
    m::bern::product_rounding(du, dv)
});

diff!(bern_product_rounding_extreme, || {
    (
        m::bern::product_rounding(usize::MAX, 0),
        m::bern::product_rounding(usize::MAX, usize::MAX),
        m::bern::product_rounding(0, usize::MAX - 1),
    )
});

diff!(bern_roots, |c: List<f64>, lo: f64, hi: f64| {
    let c = &c.0;
    (
        m::bern::roots(c, lo, hi, 48, 4096),
        m::bern::roots(c, lo, hi, 3, 2),
        m::bern::roots(c, lo, hi, 0, 4096),
        m::bern::roots(c, lo, hi, 48, 0),
    )
});

diff!(bern_roots_shapes, |p: BernR, d: usize| {
    // each recipe's coefficients as one polynomial, on [0, 1] and on a shifted interval
    let c = &p.0.2;
    (
        m::bern::roots(c, 0.0, 1.0, 48, 4096),
        m::bern::roots(c, -2.0, 0.5, 48, 4096),
        m::bern::roots(c, 0.0, 1.0, u32::try_from(d).unwrap_or(u32::MAX), d),
    )
});

diff!(bern_roots_known, |k: usize| {
    // products of (s - r) with dyadic roots r, close pairs and double roots among them
    let cs = truth::known_cases();
    let (c, _) = &cs[k % cs.len()];
    (
        m::bern::roots(c, 0.0, 1.0, 48, 4096),
        m::bern::roots(c, 0.0, 1.0, 12, 64),
        m::bern::roots(c, 1.0, 3.0, 48, 4096),
    )
});

/// Exact arithmetic over the scx side alone.
mod truth {
    use scx_algo::bernstein::{
        Bernstein2, Bounded, Half, MAX_DEGREE, RootLimits, bernstein_roots, binomial,
    };
    use scx_num::fmath;
    use scx_testkit::Rat;

    fn q(x: f64) -> Rat {
        Rat::from_f64(x).unwrap()
    }

    fn qn(n: usize) -> Rat {
        Rat::from(i64::try_from(n).unwrap())
    }

    fn poly(du: usize, dv: usize, f: impl Fn(usize, usize) -> f64) -> Bernstein2 {
        let mut c = Vec::new();
        for i in 0..=du {
            for j in 0..=dv {
                c.push(f(i, j));
            }
        }
        Bernstein2::new(du, dv, c).unwrap()
    }

    /// Coefficients of both signs over five decades, none a short binary fraction.
    fn mixed(du: usize, dv: usize, seed: f64) -> Bernstein2 {
        poly(du, dv, |i, j| {
            let k = (i * (dv + 1) + j) as f64;
            fmath::sin((k + seed) * 0.7)
                * fmath::powi(10.0, i32::try_from((i + 2 * j) % 5).unwrap_or(0) - 2)
                + 1.0 / 3.0
        })
    }

    /// The exact coefficients, row by row.
    fn exact(p: &Bernstein2) -> Vec<Rat> {
        p.coefficients().iter().map(|&c| q(c)).collect()
    }

    /// De Casteljau of `c` at `t`, exactly: the value, and the two coefficients of the last
    /// level but one (empty for a constant).
    fn casteljau(c: &[Rat], t: &Rat) -> (Rat, Option<(Rat, Rat)>) {
        let s = &Rat::one() - t;
        let mut b = c.to_vec();
        let mut before = None;
        while b.len() > 1 {
            if b.len() == 2 {
                before = Some((b[0].clone(), b[1].clone()));
            }
            b = b.windows(2).map(|w| &(&s * &w[0]) + &(t * &w[1])).collect();
        }
        (b.first().cloned().unwrap_or_else(Rat::zero), before)
    }

    /// The exact value at `(u, v)` of degrees `(du, dv)` and coefficients `c`, row by row.
    fn value2(du: usize, dv: usize, c: &[Rat], u: &Rat, v: &Rat) -> Rat {
        let col: Vec<Rat> = c.chunks(dv + 1).map(|row| casteljau(row, v).0).collect();
        debug_assert_eq!(col.len(), du + 1);
        casteljau(&col, u).0
    }

    fn value(p: &Bernstein2, u: &Rat, v: &Rat) -> Rat {
        let (du, dv) = p.degrees();
        value2(du, dv, &exact(p), u, v)
    }

    /// The exact partial derivative in `u` at `(u, v)`, from de Casteljau's last level but
    /// one: `du (b1 - b0)`.
    fn deriv_u_value(p: &Bernstein2, u: &Rat, v: &Rat) -> Rat {
        let (du, dv) = p.degrees();
        let col: Vec<Rat> = exact(p)
            .chunks(dv + 1)
            .map(|row| casteljau(row, v).0)
            .collect();
        match casteljau(&col, u).1 {
            Some((b0, b1)) => &qn(du) * &(&b1 - &b0),
            None => Rat::zero(),
        }
    }

    /// `|got - want| <= tol`, exactly.
    fn near(got: &Rat, want: &Rat, tol: f64) -> bool {
        (got - want).abs() <= q(tol)
    }

    const SAMPLES: [f64; 6] = [0.0, 0.2, 0.5, 0.7, 0.9375, 1.0];
    const DYADIC: [f64; 5] = [0.0, 0.25, 0.5, 0.8125, 1.0];

    /// The product, elevation, derivatives, splits and restriction of polynomials of
    /// several degrees take the exact values of the operation on the operands (to the
    /// rounding of their coefficients, a few units in the last place of their scale).
    #[test]
    fn truth_values_of_the_operations() {
        let shapes = [
            ((2, 1), (1, 3)),
            ((0, 0), (3, 3)),
            ((4, 2), (2, 0)),
            ((1, 5), (0, 2)),
        ];
        for (k, (da, db)) in shapes.into_iter().enumerate() {
            let p = mixed(da.0, da.1, 0.1 + k as f64);
            let r = mixed(db.0, db.1, 0.9 + k as f64);
            let scale = p.max_abs_coefficient() * r.max_abs_coefficient();
            let tol = 64.0 * f64::EPSILON * scale.max(1.0);
            let pr = p.mul(&r).unwrap();
            let e = p.elevate(da.0 + 3, da.1 + 2).unwrap();
            let sum = p.add_scaled(&r, -0.7).unwrap();
            let du = pr.deriv_u();
            let dv = p.deriv_v();
            let (l, rt) = pr.split_u(0.3);
            let (lv, rv) = p.split_v(0.6);
            let s = pr.restrict(0.1, 0.6, 0.2, 0.9);
            for &a in &SAMPLES {
                for &b in &SAMPLES {
                    let (u, v) = (q(a), q(b));
                    let pv = value(&p, &u, &v);
                    let rv0 = value(&r, &u, &v);
                    let want = &pv * &rv0;
                    assert!(near(&value(&pr, &u, &v), &want, tol), "product {k} {a} {b}");
                    assert!(near(&value(&e, &u, &v), &pv, tol), "elevation {k} {a} {b}");
                    let want = &pv - &(&q(0.7) * &rv0);
                    assert!(near(&value(&sum, &u, &v), &want, tol), "sum {k} {a} {b}");
                    let want = deriv_u_value(&pr, &u, &v);
                    assert!(
                        near(&value(&du, &u, &v), &want, 64.0 * tol),
                        "du {k} {a} {b}"
                    );
                    // the v derivative, by the transpose's u derivative
                    let (pu, pvd) = p.degrees();
                    let t = poly(pvd, pu, |i, j| p.coefficients()[j * (pvd + 1) + i]);
                    let want = deriv_u_value(&t, &v, &u);
                    assert!(
                        near(&value(&dv, &u, &v), &want, 64.0 * tol),
                        "dv {k} {a} {b}"
                    );
                    let at = |x: f64, y: f64, w: f64| &q(x) + &(&(&q(y) - &q(x)) * &q(w));
                    let want = value(&pr, &(&q(0.3) * &u), &v);
                    assert!(near(&value(&l, &u, &v), &want, tol), "left {k} {a} {b}");
                    let want = value(&pr, &at(0.3, 1.0, a), &v);
                    assert!(near(&value(&rt, &u, &v), &want, tol), "right {k} {a} {b}");
                    let want = value(&p, &u, &(&q(0.6) * &v));
                    assert!(near(&value(&lv, &u, &v), &want, tol), "lower v {k} {a} {b}");
                    let want = value(&p, &u, &at(0.6, 1.0, b));
                    assert!(near(&value(&rv, &u, &v), &want, tol), "upper v {k} {a} {b}");
                    let want = value(&pr, &at(0.1, 0.6, a), &at(0.2, 0.9, b));
                    assert!(near(&value(&s, &u, &v), &want, tol), "restrict {k} {a} {b}");
                    let ev = q(pr.eval(a, b));
                    assert!(near(&ev, &value(&pr, &u, &v), tol), "eval {k} {a} {b}");
                    // the convex hull: every value within the coefficients' extremes
                    let pv = value(&p, &u, &v);
                    assert!(q(p.min_coefficient()) <= pv && pv <= q(p.max_coefficient()));
                }
            }
        }
    }

    /// A product's degrees add, a refusal names the bound, and a polynomial at the bound
    /// splits and restricts at its degrees with its values.
    #[test]
    fn truth_extreme_degrees() {
        let p = poly(MAX_DEGREE, 0, |_, _| 1.0);
        assert!(p.mul(&poly(1, 0, |_, _| 1.0)).is_err());
        assert!(Bernstein2::new(1, 1, vec![0.0; 3]).is_err());
        assert!(Bernstein2::new(usize::MAX, usize::MAX, Vec::new()).is_err());
        let k = Bernstein2::constant(2.5);
        // dyadic coefficients and samples keep the exact arithmetic short at degree 128
        let wide = poly(0, MAX_DEGREE, |_, j| 1.0 + (j as f64) / 128.0);
        let tall = poly(MAX_DEGREE, 0, |i, _| 1.0 - (i as f64) / 128.0);
        for p in [&k, &wide, &tall] {
            let (lu, ru) = p.split_u(0.5);
            let (lv, rv) = p.split_v(0.5);
            let s = p.restrict(0.25, 0.75, 0.5, 1.0);
            for &a in &DYADIC {
                for &b in &DYADIC {
                    let (u, v) = (q(a), q(b));
                    let h = |x: &Rat| x.scale(-1);
                    let up = |x: &Rat| &h(x) + &Rat::one().scale(-1);
                    // the halves are exact in binary but for the averages' rounding
                    let t = 1e-12;
                    assert!(near(&value(&lu, &u, &v), &value(p, &h(&u), &v), t));
                    assert!(near(&value(&ru, &u, &v), &value(p, &up(&u), &v), t));
                    assert!(near(&value(&lv, &u, &v), &value(p, &u, &h(&v)), t));
                    assert!(near(&value(&rv, &u, &v), &value(p, &u, &up(&v)), t));
                    let su = &Rat::one().scale(-2) + &h(&u);
                    assert!(near(&value(&s, &u, &v), &value(p, &su, &up(&v)), t));
                }
            }
            assert_eq!(lu.degrees(), p.degrees());
            assert_eq!(rv.degrees(), p.degrees());
        }
    }

    /// Pascal's rule and the row sums `2^n`, exactly, up to the largest product's degree.
    #[test]
    fn truth_binomials() {
        for n in 1..=2 * MAX_DEGREE {
            let mut row = Rat::zero();
            for k in 0..=n {
                let (a, b, c) = (
                    binomial(n, k),
                    binomial(n - 1, k),
                    binomial(n - 1, k.wrapping_sub(1)),
                );
                let b = if k == n { 0.0 } else { b };
                let c = if k == 0 { 0.0 } else { c };
                // exact while the row stays below 2^53 (n <= 56), within rounding after
                let sum = &q(b) + &q(c);
                if n <= 56 {
                    assert_eq!(q(a), sum, "C({n}, {k})");
                } else {
                    assert!(
                        near(&q(a), &sum, a * 8.0 * f64::EPSILON * n as f64),
                        "C({n}, {k})"
                    );
                }
                row = &row + &q(a);
            }
            if n <= 56 {
                assert_eq!(
                    row,
                    Rat::one().scale(i64::try_from(n).unwrap_or(0)),
                    "row {n}"
                );
            }
            assert_eq!(binomial(n, n + 1), 0.0);
        }
    }

    /// The exact product's coefficients, row by row.
    fn exact_product(fa: &Bernstein2, fb: &Bernstein2) -> Vec<Rat> {
        let ((au, av), (bu, bv)) = (fa.degrees(), fb.degrees());
        let (du, dv) = (au + bu, av + bv);
        let (ca, cb) = (exact(fa), exact(fb));
        let bq = |n: usize, k: usize| q(binomial(n, k));
        let mut out = Vec::new();
        for i in 0..=du {
            for j in 0..=dv {
                let mut sum = Rat::zero();
                for ia in 0..=au {
                    for ja in 0..=av {
                        let (Some(ib), Some(jb)) = (i.checked_sub(ia), j.checked_sub(ja)) else {
                            continue;
                        };
                        if ib > bu || jb > bv {
                            continue;
                        }
                        let w = &(&bq(au, ia) * &bq(av, ja)) * &(&bq(bu, ib) * &bq(bv, jb));
                        let xy = &ca[ia * (av + 1) + ja] * &cb[ib * (bv + 1) + jb];
                        sum = &sum + &(&w * &xy);
                    }
                }
                let den = &bq(du, i) * &bq(dv, j);
                out.push(sum.checked_div(&den).unwrap());
            }
        }
        out
    }

    /// The exact half `[0, 1/2]` (`Half::Lower`) or `[1/2, 1]` of `c` along one row.
    fn exact_half(c: &[Rat], h: Half) -> Vec<Rat> {
        let mut left = Vec::new();
        let mut right = Vec::new();
        let mut b = c.to_vec();
        while let (Some(f), Some(l)) = (b.first().cloned(), b.last().cloned()) {
            left.push(f);
            right.push(l);
            b = b.windows(2).map(|w| (&w[0] + &w[1]).scale(-1)).collect();
        }
        right.reverse();
        match h {
            Half::Lower => left,
            Half::Upper => right,
        }
    }

    /// The exact quarter of degrees `(du, dv)` and coefficients `c`.
    fn exact_quarter(du: usize, dv: usize, c: &[Rat], hu: Half, hv: Half) -> Vec<Rat> {
        let rows: Vec<Vec<Rat>> = c.chunks(dv + 1).map(|r| exact_half(r, hv)).collect();
        let mut out = vec![Rat::zero(); (du + 1) * (dv + 1)];
        for j in 0..=dv {
            let col: Vec<Rat> = rows.iter().map(|r| r[j].clone()).collect();
            for (i, x) in exact_half(&col, hu).into_iter().enumerate() {
                out[i * (dv + 1) + j] = x;
            }
        }
        out
    }

    /// Whether every coefficient of `b` is within its bound of `want`, and whether any is
    /// off it at all.
    fn held(b: &Bounded, want: &[Rat]) -> (bool, bool) {
        let got = exact(&b.f);
        assert_eq!(got.len(), want.len());
        let all = got.iter().zip(want).all(|(g, w)| near(g, w, b.err));
        let off = got.iter().zip(want).any(|(g, w)| g != w);
        (all, off)
    }

    /// `Bounded::mul` and `Bounded::quarter` hold each coefficient within their bound of
    /// the exact one, and the rounding happens (some coefficient is off it) and is counted.
    #[test]
    fn truth_the_counted_rounding_holds_the_exact_coefficients() {
        for (da, db) in [
            ((3, 2), (4, 3)),
            ((1, 1), (1, 1)),
            ((0, 5), (2, 0)),
            ((5, 2), (4, 4)),
        ] {
            let (fa, fb) = (mixed(da.0, da.1, 0.1), mixed(db.0, db.1, 0.9));
            let p = Bounded::exact(fa.clone())
                .mul(&Bounded::exact(fb.clone()))
                .unwrap();
            let (all, off) = held(&p, &exact_product(&fa, &fb));
            assert!(all && off, "product {da:?} {db:?}");
            let (du, dv) = p.f.degrees();
            for hu in [Half::Lower, Half::Upper] {
                for hv in [Half::Lower, Half::Upper] {
                    let qb = Bounded::exact(p.f.clone()).quarter(hu, hv);
                    let want = exact_quarter(du, dv, &exact(&p.f), hu, hv);
                    let (all, off) = held(&qb, &want);
                    assert!(all && off, "quarter {da:?} {db:?} {hu:?} {hv:?}");
                    // a bounded operand's bound is carried: the quarter of the rounded
                    // product within its bound of the quarter of the exact one
                    let qp = p.quarter(hu, hv);
                    let ex = exact_quarter(du, dv, &exact_product(&fa, &fb), hu, hv);
                    assert!(
                        held(&qp, &ex).0,
                        "carried quarter {da:?} {db:?} {hu:?} {hv:?}"
                    );
                }
            }
        }
    }

    /// Below the normal range a rounding is absolute: half the least subnormal rounds to
    /// 0, and the bound covers it.
    #[test]
    fn truth_a_rounding_below_the_normal_range_is_counted() {
        let t = f64::from_bits(1);
        let f = Bounded::exact(Bernstein2::new(1, 0, vec![t, t]).unwrap());
        let qb = f.quarter(Half::Lower, Half::Lower);
        assert_eq!(qb.f.coefficients(), &[t, 0.0]);
        assert!(held(&qb, &[q(t), q(t)]).0, "{}", qb.err);
        // `3 t` times `(1 + 2^-52) 2^1023` is `3 2^-51 + 3 2^-103` exactly, rounded by
        // `2^-103`, while `gamma * 3 t` alone underflows to 0
        let (a, b) = (3.0 * t, (1.0 + f64::EPSILON) * fmath::powi(2.0, 1023));
        let p = Bounded::exact(Bernstein2::constant(a))
            .mul(&Bounded::exact(Bernstein2::constant(b)))
            .unwrap();
        let want = &q(a) * &q(b);
        let (all, off) = held(&p, &[want]);
        assert!(all && off, "{}", p.err);
    }

    /// Sums, scalings, derivatives and products of an operand itself off the exact
    /// polynomial `g` by up to its bound: each result is within its bound of the same
    /// operation on `g`, exactly.
    #[test]
    fn truth_the_counted_rounding_carries_through_sums_scalings_and_derivatives() {
        let (g, h) = (mixed(3, 2, 0.1), mixed(3, 2, 0.9));
        let e = 8.0 * f64::EPSILON * g.max_abs_coefficient();
        let off = poly(3, 2, |i, j| {
            let c = g.coefficients()[i * 3 + j];
            if (i + j) % 2 == 0 {
                c + 0.5 * e
            } else {
                c - 0.25 * e
            }
        });
        let a = Bounded { f: off, err: e };
        let x = -0.7;
        let (gc, hc) = (exact(&g), exact(&h));
        let s = a.add_scaled(&Bounded::exact(h.clone()), x).unwrap();
        let want: Vec<Rat> = gc.iter().zip(&hc).map(|(p, r)| p + &(&q(x) * r)).collect();
        assert!(held(&s, &want).0, "sum");
        let t = a.scale(x);
        let want: Vec<Rat> = gc.iter().map(|p| &q(x) * p).collect();
        assert!(held(&t, &want).0, "scale");
        let du = a.deriv_u();
        let mut want = Vec::new();
        for i in 0..3 {
            for j in 0..3 {
                want.push(&qn(3) * &(&gc[(i + 1) * 3 + j] - &gc[i * 3 + j]));
            }
        }
        assert!(held(&du, &want).0, "u");
        let dv = a.deriv_v();
        let mut want = Vec::new();
        for i in 0..4 {
            for j in 0..2 {
                want.push(&qn(2) * &(&gc[i * 3 + j + 1] - &gc[i * 3 + j]));
            }
        }
        assert!(held(&dv, &want).0, "v");
        let p = a.mul(&Bounded::exact(h.clone())).unwrap();
        assert!(held(&p, &exact_product(&g, &h)).0, "product");
        let p = a.mul(&a).unwrap();
        assert!(held(&p, &exact_product(&g, &g)).0, "square");
    }

    /// Polynomials with known dyadic roots in `[0, 1]`: the Bernstein coefficients of
    /// `n! prod (s - r)`, exact in binary, and the roots (a double root listed twice).
    pub fn known_cases() -> Vec<(Vec<f64>, Vec<f64>)> {
        let sets: [&[f64]; 12] = [
            &[0.25, 0.75],
            &[0.5],
            &[0.125, 0.375, 0.625, 0.875],
            &[0.0, 0.5, 1.0],
            &[0.3125, 0.6875, 0.9375],
            &[0.5, 0.5],
            &[0.5, 0.5 + 1.0 / 1024.0],
            &[0.25, 0.25, 0.75],
            &[1.5, 0.25],
            &[-0.5, 2.0],
            &[0.1875, 0.4375, 0.5625, 0.8125, 0.96875],
            &[0.5, 0.5 + 1.0 / 1_048_576.0],
        ];
        sets.iter().map(|r| (known(r), r.to_vec())).collect()
    }

    /// The Bernstein coefficients on `[0, 1]` of `n! prod (s - r)`.
    fn known(roots: &[f64]) -> Vec<f64> {
        // monomial coefficients, lowest first
        let mut a = vec![Rat::one()];
        for &r in roots {
            let mut b = vec![Rat::zero(); a.len() + 1];
            for (k, x) in a.iter().enumerate() {
                b[k + 1] = &b[k + 1] + x;
                b[k] = &b[k] - &(x * &q(r));
            }
            a = b;
        }
        let n = roots.len();
        let fact = (1..=n).fold(Rat::one(), |f, k| &f * &qn(k));
        // b_i = sum_k a_k C(i, k) / C(n, k)
        (0..=n)
            .map(|i| {
                let mut s = Rat::zero();
                for (k, x) in a.iter().enumerate().take(i + 1) {
                    let w = q(binomial(i, k)).checked_div(&q(binomial(n, k))).unwrap();
                    s = &s + &(x * &w);
                }
                let c = &s * &fact;
                let f = c.to_f64();
                assert_eq!(q(f), c, "{roots:?} {i}: not exact in binary");
                f
            })
            .collect()
    }

    /// The exact value of `c` at `t` in `[0, 1]`.
    fn value1(c: &[f64], t: f64) -> Rat {
        let c: Vec<Rat> = c.iter().map(|&x| q(x)).collect();
        casteljau(&c, &q(t)).0
    }

    /// The simple roots are each found once, to rounding (each brackets an exact sign
    /// change of the polynomial or is an exact zero), and nothing else is found; a double
    /// root is either found where the coefficients reach exact zeros or left undecided with
    /// `isolated` false and the root inside a piece reported.
    #[test]
    fn truth_roots_of_known_polynomials() {
        for (c, roots) in known_cases() {
            let inside: Vec<f64> = roots
                .iter()
                .copied()
                .filter(|r| (0.0..=1.0).contains(r))
                .collect();
            let r = bernstein_roots(&c, 0.0, 1.0, RootLimits::DEFAULT);
            let double = inside.windows(2).any(|w| w[0] == w[1]);
            let close = inside.windows(2).any(|w| w[1] - w[0] < 1e-5);
            if !double && !close {
                assert!(r.isolated, "{roots:?}: {r:?}");
                let mut got = r.places.clone();
                got.sort_by(f64::total_cmp);
                got.dedup();
                assert_eq!(got.len(), inside.len(), "{roots:?}: {got:?}");
                for (g, w) in got.iter().zip(&inside) {
                    assert!((g - w).abs() <= 1e-12, "{roots:?}: {g} vs {w}");
                    if value1(&c, *g).is_zero() {
                        continue;
                    }
                    // a sign change within a few units of the reported root
                    let ulp = 4.0 * f64::EPSILON;
                    let (a, b) = (
                        value1(&c, (g - ulp).max(0.0)),
                        value1(&c, (g + ulp).min(1.0)),
                    );
                    assert!(a.signum() * b.signum() <= 0, "{roots:?}: {g}");
                }
            } else {
                // every reported place is within reach of a root
                for g in &r.places {
                    assert!(
                        inside.iter().any(|w| (g - w).abs() <= 1e-5),
                        "{roots:?}: {g} in {r:?}"
                    );
                }
                for w in &inside {
                    assert!(
                        r.places.iter().any(|g| (g - w).abs() <= 1e-5),
                        "{roots:?}: {w} missing from {r:?}"
                    );
                }
            }
        }
        // the double root at 1/2 halves to exact zeros there
        let r = bernstein_roots(&[0.25, -0.25, 0.25], 0.0, 1.0, RootLimits::DEFAULT);
        assert!(
            !r.places.is_empty() && r.places.iter().all(|&x| x == 0.5),
            "{r:?}"
        );
        // one sign throughout: none
        let r = bernstein_roots(&[1.0, 0.5, 2.0], 0.0, 1.0, RootLimits::DEFAULT);
        assert!(r.isolated && r.places.is_empty());
        // roots closer than the depth resolves exhaust it: undecided, both ends reported
        let c = known(&[0.5, 0.5 + 1.0 / 1_048_576.0]);
        let r = bernstein_roots(
            &c,
            0.0,
            1.0,
            RootLimits {
                depth: 8,
                subdivisions: 4096,
            },
        );
        assert!(!r.isolated, "{r:?}");
        let r = bernstein_roots(
            &c,
            0.0,
            1.0,
            RootLimits {
                depth: 48,
                subdivisions: 1,
            },
        );
        assert!(!r.isolated, "{r:?}");
        // on another interval the places map to it
        let c = known(&[0.25, 0.75]);
        let r = bernstein_roots(&c, 2.0, 6.0, RootLimits::DEFAULT);
        let mut got = r.places;
        got.sort_by(f64::total_cmp);
        assert!(got.len() == 2 && (got[0] - 3.0).abs() < 1e-12 && (got[1] - 5.0).abs() < 1e-12);
    }
}
