// Written for solidcorex. Apache-2.0.
//! `interval.rs`, the ring part of `tay.rs` (`ring`), `taylor_model.rs`, `series.rs` and
//! `interval_vector.rs` (the last against the pin's own bodies over its `Ivl`, the module
//! being `pub(crate)` there). Intervals are drawn as `[lo, hi]` pairs from the edge pool,
//! so reversed, NaN, infinite, subnormal and overflowing ends all occur.

use crate::diff;
use crate::harness::List;

type I = [f64; 2];

/// Three intervals from six numbers.
fn six(a: [f64; 6]) -> [I; 3] {
    [[a[0], a[1]], [a[2], a[3]], [a[4], a[5]]]
}

// --- interval
diff!(i_rounding, |x: f64| (
    m::interval::round_down(x),
    m::interval::round_up(x)
));
diff!(i_build, |a: f64, b: f64| (
    m::interval::Ivl::new(a, b),
    m::interval::Ivl::whole(),
    m::interval::Ivl::checked(a, b),
    m::interval::Ivl::from(a),
    m::interval::Ivl::outward_ulp(a, b)
));
diff!(i_queries, |a: I| {
    let x = m::iv(a);
    (
        x.is_positive(),
        x.sqrt(),
        x.mag(),
        x.mig(),
        x.mid(),
        x.is_finite(),
    )
});
diff!(i_hull, |a: I, b: I| m::iv(a).hull(m::iv(b)));
diff!(i_operators, |a: I, b: I| {
    let (x, y) = (m::iv(a), m::iv(b));
    (x + y, x - y, x * y, x / y)
});
diff!(i_ulp, |a: I, b: I| {
    let (x, y) = (m::iv(a), m::iv(b));
    (x.add_ulp(y), x.sub_ulp(y), x.mul_ulp(y), x.div_ulp(y))
});

// --- ring (solidcore's `tay`)
diff!(r_f64, |x: f64| {
    use m::ring::{Ring, TayScalar};
    (
        <f64 as Ring>::lift(x),
        x.sqrt_s(),
        x.acos_s(),
        x.sin_cos_s(),
        m::ring::acos_clamped(x),
    )
});
diff!(r_ivl, |a: I| {
    use m::ring::{Ring, TayScalar};
    let x = m::iv(a);
    (
        <m::interval::Ivl as Ring>::lift(a[0]),
        x.sqrt_s(),
        x.acos_s(),
        x.sin_cos_s(),
    )
});
// arc cosine and sine/cosine where they are defined, off the edge pool
diff!(r_ivl_inside, |a: f64, b: f64| {
    use m::ring::TayScalar;
    let (p, q) = ((a.sin() + 1.0) / 2.0, (b.sin() + 1.0) / 2.0);
    let y = m::interval::Ivl::new(2.0 * p.min(q) - 1.0, 2.0 * p.max(q) - 1.0);
    let t = m::interval::Ivl::new(
        p.min(q) * core::f64::consts::FRAC_PI_2,
        p.max(q) * core::f64::consts::FRAC_PI_2,
    );
    (y.acos_s(), t.sin_cos_s())
});
diff!(r_wide, |a: f64, b: f64| m::ring::wide(a, b));

// --- taylor_model
diff!(t_consts, || (
    m::taylor_model::TM_DEG,
    m::taylor_model::TM_MAX
));
diff!(t_build, |c: List<I>, r: I, p: List<f64>| {
    use m::taylor_model::Tm;
    let t = m::tm(&c, r);
    (
        (
            Tm::constant(m::iv(r)),
            Tm::whole(),
            t,
            Tm::from_points(&p.0),
        ),
        t.mid_poly(),
        t.with_rem(m::iv(r)),
        t.scale(m::iv(r)),
        t.add_ivl(m::iv(r)),
    )
});
diff!(t_ranges, |c: List<I>, r: I, x: f64| {
    let t = m::tm(&c, r);
    (t.poly_range(), t.range(), t.at(x), t.at(x.sin()))
});
diff!(t_arith, |c: List<I>, d: List<I>, r: I| {
    let (a, b) = (m::tm(&c, r), m::tm(&d, [0.0, 0.0]));
    (a + b, a - b, a * b, a / b, a.recip(), a.sqrt_tm())
});
// models of functions bounded away from 0, where the reciprocal and the root are made
diff!(t_positive, |p: List<f64>, q: List<f64>, r: I| {
    let lift = |v: &List<f64>| {
        let mut c: Vec<f64> = v.0.iter().map(|x| x.sin()).collect();
        let rest: f64 = c.iter().skip(1).map(|x| x.abs()).sum();
        if let Some(c0) = c.first_mut() {
            *c0 = rest + 1.0 + c0.abs();
        }
        m::taylor_model::Tm::from_points(&c)
    };
    let (a, b) = (
        lift(&p),
        lift(&q).with_rem(m::iv([r[0].sin() * 0.1, r[0].sin().abs() * 0.1])),
    );
    (
        a.recip(),
        a.sqrt_tm(),
        b.recip(),
        b.sqrt_tm(),
        a / b,
        (a * b).sqrt_tm(),
    )
});
diff!(t_scalar_traits, |c: List<I>, r: I, x: f64| {
    use m::ring::{Ring, TayScalar};
    use m::series::GnScalar;
    use m::taylor_model::Tm;
    let t = m::tm(&c, r);
    (
        (<Tm as Ring>::lift(x), t.sqrt_s(), t.acos_s(), t.sin_cos_s()),
        <Tm as GnScalar>::of(x),
        GnScalar::is_positive(t),
        GnScalar::sqrt(t),
        <Tm as GnScalar>::JOINT_OFFSET,
    )
});

// --- series
diff!(s_consts, || (
    m::series::MAX_S,
    m::series::MAX_E,
    m::series::TAYLOR,
    m::series::MAX_K,
    m::series::STORE_E,
    m::series::VZ
));
diff!(s_trunc, |s: usize, e: usize, d: usize, p: usize| {
    use m::series::Trunc;
    let tr = Trunc::new(s, e);
    let ok = tr.as_ref().ok().map(|&t| {
        let c = t.computing(d, p);
        let k = c.skipping(Trunc::bit(d % 8, p % 6) | Trunc::bit(0, 0));
        (
            (c, k, k.computes(d % 8, p % 6), k.computes(1, 1)),
            t.degree(),
            t.s(),
            t.e(),
            t.computed_mask(),
        )
    });
    (tr.map(|_| ()), ok)
});
diff!(s_gn_scalars, |x: f64, a: I| {
    use m::series::GnScalar;
    (
        (
            <f64 as GnScalar>::of(x),
            GnScalar::is_positive(x),
            GnScalar::sqrt(x),
            <f64 as GnScalar>::JOINT_OFFSET,
        ),
        <m::interval::Ivl as GnScalar>::of(x),
        GnScalar::is_positive(m::iv(a)),
        GnScalar::sqrt(m::iv(a)),
        <m::interval::Ivl as GnScalar>::JOINT_OFFSET,
    )
});
diff!(s_f64_ops, |u: List<f64>,
                  v: List<f64>,
                  s: usize,
                  e: usize| {
    let tr = m::trunc(s, e);
    let (a, b) = (m::ser(&u.0, tr), m::ser(&v.0, tr));
    let f = v.0.first().copied().unwrap_or(0.5);
    let mut acc = a;
    acc.add_scaled(&b, f, tr);
    let mut from = a;
    from.add_scaled_from(&b, f, (s + e) % 4, tr);
    let mut merged = b;
    merged.merge_computed(&a, tr.computing(s % 3, e % 2));
    (
        (a.add(&b, tr), a.sub(&b, tr), acc, from, merged),
        a.masked(tr.computing(2, 1)),
        a.scale(f, tr),
        a.mul(&b, tr),
        a.mul_valued(&b, s % 3, e % 2, tr),
        (a.at(s, e), a.at(9, 9)),
    )
});
diff!(s_ivl_ops, |u: List<I>, v: List<I>, s: usize, e: usize| {
    let tr = m::trunc(s, e);
    let (a, b) = (m::ser(&m::ivs(&u), tr), m::ser(&m::ivs(&v), tr));
    let f = m::ivs(&v)
        .first()
        .copied()
        .unwrap_or(m::interval::Ivl::from(0.5));
    let mut acc = a.clone();
    acc.add_scaled(&b, f, tr);
    let mut from = a.clone();
    from.add_scaled_from(&b, f, (s + e) % 4, tr);
    let mut merged = b.clone();
    merged.merge_computed(&a, tr.computing(s % 3, e % 2));
    (
        (a.add(&b, tr), a.sub(&b, tr), acc, from, merged),
        a.masked(tr.computing(2, 1)),
        a.scale(f, tr),
        a.mul(&b, tr),
        a.mul_valued(&b, s % 3, e % 2, tr),
    )
});
diff!(s_inv_sqrt_f64, |u: List<f64>, s: usize, e: usize| {
    let tr = m::trunc(s, e);
    let a = m::ser(&u.0, tr);
    let mut inc = m::series::InvSqrtSeries::<f64>::default();
    let first = inc.update(&a, tr.computing(1, 1)).cloned();
    let all = inc.update(&a, tr).cloned();
    (a.inv_sqrt(tr), first, all)
});
diff!(s_inv_sqrt_ivl, |u: List<I>, s: usize, e: usize| {
    let tr = m::trunc(s, e);
    let a = m::ser(&m::ivs(&u), tr);
    let mut inc = m::series::InvSqrtSeries::<m::interval::Ivl>::default();
    let first = inc.update(&a, tr.computing(1, 1)).cloned();
    let all = inc.update(&a, tr).cloned();
    (a.inv_sqrt(tr), first, all)
});
// positive constants, where the series is made rather than refused
diff!(s_inv_sqrt_positive, |u: List<f64>, s: usize, e: usize| {
    let tr = m::trunc(s, e);
    let mut v: Vec<f64> = u.0.iter().map(|x| x.sin()).collect();
    if let Some(v0) = v.first_mut() {
        *v0 = 1.0 + v0.abs();
    }
    let a = m::ser(&v, tr);
    let ai = m::ser(
        &v.iter()
            .map(|&x| m::interval::Ivl::from(x))
            .collect::<Vec<_>>(),
        tr,
    );
    (a.inv_sqrt(tr), ai.inv_sqrt(tr))
});
diff!(s_vectors, |a: [f64; 3], p: [f64; 6], q: [f64; 6], x: I| {
    use m::series::{v_add, v_hull, v_scale, v_sub, V3};
    let (p, q) = (m::v3i(six(p)), m::v3i(six(q)));
    (
        V3::<f64>::from_vec3(m::v3(a)),
        V3::<m::interval::Ivl>::from_vec3(m::v3(a)),
        v_add(p, q),
        v_sub(p, q),
        v_scale(p, m::iv(x)),
        v_hull(p, q),
    )
});
diff!(s_vector_series, |u: List<f64>,
                        v: List<f64>,
                        s: usize,
                        e: usize| {
    use m::series::{VSer, V3};
    let tr = m::trunc(s, e);
    let w = |k: usize| u.0.get(k).copied().unwrap_or(1.0);
    let a = VSer::<f64>::constant(V3 {
        x: w(0),
        y: w(1),
        z: w(2),
    });
    let mut b = VSer::<f64>::constant(V3 {
        x: w(2),
        y: w(0),
        z: w(1),
    });
    b.a[0] = m::ser(&v.0, tr);
    let k = m::ser(&u.0, tr);
    (
        a.add(&b, tr),
        a.sub(&b, tr),
        b.times(&k, tr),
        a.dot(&b, tr),
        a.cross(&b, tr),
    )
});
diff!(s_vector_series_ivl, |p: [f64; 6],
                            u: List<I>,
                            s: usize,
                            e: usize| {
    use m::series::VSer;
    let tr = m::trunc(s, e);
    let a = VSer::<m::interval::Ivl>::constant(m::v3i(six(p)));
    let mut b = a.clone();
    b.a[1] = m::ser(&m::ivs(&u), tr);
    let k = m::ser(&m::ivs(&u), tr);
    (
        a.add(&b, tr),
        a.sub(&b, tr),
        b.times(&k, tr),
        a.dot(&b, tr),
        a.cross(&b, tr),
    )
});

// --- interval_vector (scx-num's public functions against the pin's bodies)
diff!(v_ops, |p: [f64; 6], q: [f64; 6]| {
    let (a, b) = (six(p).map(m::iv), six(q).map(m::iv));
    (
        m::interval_vector::cross(a, b),
        m::interval_vector::dot(a, b),
        m::interval_vector::square(a[0]),
        m::interval_vector::norm(a),
        m::interval_vector::unit(a),
    )
});
