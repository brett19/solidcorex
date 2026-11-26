// Written for solidcorex. Apache-2.0.
//! `tol.rs` and `kernel::Tolerances`: the scalar tolerance functions. The functions that
//! took `&Kernel` take `&Tolerances` here; `m::tolk` builds each side's argument from
//! the same seven numbers.

use crate::diff;
use crate::facade::TolR;

type V = [f64; 3];
type P = [f64; 2];
type I = [f64; 2];
type B = [f64; 6];
type R = [f64; 4];

fn small(code: i32) -> scx_num::tol::SmallTopoType {
    scx_num::tol::SmallTopoType::from_code(code.rem_euclid(5)).unwrap()
}
fn small_s(code: i32) -> solidcore::geom::tol::SmallTopoType {
    solidcore::geom::tol::SmallTopoType::from_code(code.rem_euclid(5)).unwrap()
}

diff!(
    tolerances_default,
    || {
        let t = scx_num::Tolerances::default();
        (t, t.ang_tol_rad())
    },
    {
        let t = solidcore::kernel::Tolerances::default();
        (t, t.ang_tol_rad())
    }
);
diff!(tolerances_ang_tol_rad, |r: TolR| m::tol_set(&r)
    .ang_tol_rad());
diff!(consts, || [
    m::tol::USE_DEFAULT,
    m::tol::UNINIT_TOL,
    m::tol::MODEL_SIZE_ESTIMATE,
    m::tol::LARGE_SMALL_SIZE_RATIO,
    m::tol::ZONE_TOL_3D,
    m::tol::TOL_GAP_GAIN,
    m::tol::ANG_TOL_DEG,
    m::tol::ANG_TOL_RAD,
]);
diff!(tol3d, |d: f64| m::tol::tol3d(d));
diff!(zone_xsect_approx_tol3d, |r: TolR, d: f64| {
    let t = m::tolk(&r);
    (
        m::tol::zone_tol3d(&t, d),
        m::tol::xsect_tol3d(&t, d),
        m::tol::approx_tol3d(&t, d),
    )
});
diff!(tol_mul, |a: f64, b: f64| m::tol::tol_mul(a, b));
diff!(small_topo_codes, |c: i32| m::tol::SmallTopoType::from_code(
    c
)
.map(m::tol::SmallTopoType::code));
diff!(
    zone_for_size,
    |s: f64| scx_num::tol::zone_for_size(s),
    solidcore::geom::tol::size_estimate_to_zone_tol_3d(s)
);
diff!(kernel_reads, |r: TolR| {
    let t = m::tolk(&r);
    (
        m::tol::model_size_estimate(&t),
        m::tol::gap_gain(&t),
        m::tol::contact_tol_3d(&t),
    )
});
diff!(
    small_topo_tols,
    |r: TolR, c: i32| {
        let t = m::tolk(&r);
        let s = small(c);
        (
            m::tol::zone_tol_3d(&t, s),
            m::tol::approx_tol_3d(&t, s),
            m::tol::xsect_tol_3d(&t, s),
        )
    },
    {
        let t = m::tolk(&r);
        let s = small_s(c);
        (
            m::tol::zone_tol_3d(&t, s),
            m::tol::approx_tol_3d(&t, s),
            m::tol::xsect_tol_3d(&t, s),
        )
    }
);
diff!(zones, |r: TolR, a: f64, b: f64| {
    let t = m::tolk(&r);
    (
        m::tol::xsect_tol_3d_from_zones(&t, a, b),
        m::tol::coincidence_tol_3d(&t, a, b),
    )
});
diff!(coincidence_reach_3d, |r: TolR, a: f64, b: f64, s: f64| {
    m::tol::coincidence_reach_3d(&m::tolk(&r), a, b, s)
});
diff!(
    within_coincidence_tol,
    |r: TolR, d: f64, a: f64, b: f64, s: f64| {
        m::tol::within_coincidence_tol(&m::tolk(&r), d, a, b, s)
    }
);
diff!(within_contact_tol, |d: f64, c: f64, s: f64| {
    m::tol::within_contact_tol(d, c, s)
});
diff!(zone_reach_3d, |z: f64, s: f64| m::tol::zone_reach_3d(z, s));
diff!(fixed_zeros, || [
    m::tol::ang_tol_rad(),
    m::tol::eff_zero(),
    m::tol::eff_zero_sq(),
    m::tol::eff_zero_param(),
    m::tol::eff_zero_ang_rad(),
    m::tol::scaled_zero(),
    m::tol::scaled_zero_sq(),
    m::tol::scaled_zero_sqrt(),
]);
diff!(in_tol, |a: f64, t: f64| (
    m::tol::in_tol(a, t),
    m::tol::is_zero_to_tol(a, t),
    m::tol::are_same(a, t)
));
diff!(is_zero, |a: f64| (
    m::tol::is_zero(a),
    m::tol::is_zero_squared(a)
));
diff!(are_same_to_tol, |a: f64, b: f64, t: f64| {
    m::tol::are_same_to_tol(a, b, t)
});
diff!(is_zero_speed, |a: f64, d: f64, o: Option<f64>| {
    m::tol::is_zero_speed(a, d, o)
});
diff!(is_degenerate_vec3, |r: TolR, v: V, o: Option<f64>| {
    m::tol::is_degenerate_vec3(&m::tolk(&r), m::v3(v), o)
});
diff!(are_parallel_perpendicular, |a: V, b: V, o: Option<f64>| (
    m::tol::are_parallel(m::v3(a), m::v3(b), o),
    m::tol::are_perpendicular(m::v3(a), m::v3(b), o)
));
diff!(map_to_1d, |t: f64, d: V, i: I| m::tol::map_to_1d(
    t,
    m::v3(d),
    &m::e1(i)
));
diff!(scaled_zero_of_each, |a: f64, b: P, c: V, d: I| {
    use m::tol::ScaledZero;
    let (b, c, d) = (m::v2(b), m::v3(c), m::e1(d));
    (
        [
            a.scaled_zero(),
            b.scaled_zero(),
            c.scaled_zero(),
            d.scaled_zero(),
        ],
        [
            m::tol::scaled_zero_sq_of(&a),
            m::tol::scaled_zero_sqrt_of(&c),
        ],
        [
            m::tol::scaled_zero_sq_of(&b),
            m::tol::scaled_zero_sqrt_of(&d),
        ],
    )
});
diff!(scaled_zero_of_boxes, |e: B, f: R| {
    use m::tol::ScaledZero;
    let (e, f) = (m::e3(e), m::e2(f));
    [
        e.scaled_zero(),
        f.scaled_zero(),
        m::tol::scaled_zero_sq_of(&e),
        m::tol::scaled_zero_sqrt_of(&f),
    ]
});
diff!(scaled_zero_n, |a: f64, b: V, c: I, d: B| {
    let (b, c, d) = (m::v3(b), m::e1(c), m::e3(d));
    (
        [
            m::tol::scaled_zero2(&a, &b),
            m::tol::scaled_zero_sq2(&b, &c),
            m::tol::scaled_zero_sqrt2(&c, &d),
        ],
        [
            m::tol::scaled_zero3(&a, &b, &c),
            m::tol::scaled_zero_sq3(&b, &c, &d),
            m::tol::scaled_zero_sqrt3(&a, &c, &d),
        ],
        [
            m::tol::scaled_zero4(&a, &b, &c, &d),
            m::tol::scaled_zero_sq4(&a, &b, &c, &d),
            m::tol::scaled_zero_sqrt4(&a, &b, &c, &d),
        ],
    )
});
