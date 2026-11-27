// Written for solidcorex. Apache-2.0.
//! `tol.rs` and `kernel::Tolerances`: the scalar tolerance functions. The pin's functions
//! take a `&Kernel` holding a tolerance set (`m::tolk`, built from the seven numbers of a
//! [`TolR`]); scx's take the set's zone as a [`Resolution`], and the other six are
//! constants or retired. So a set compares only where scx can say it: its zone is a
//! resolution (above 0 and finite) and every other field the function reads holds its
//! default ([`expressible`]); elsewhere both sides give `None`. Likewise an accuracy is
//! finite and at least 0, and the pin's `SM_USE_DEFAULT` argument is scx's `None`.

use crate::diff;
use crate::facade::TolR;
use scx_num::consts::{PARALLEL_ANGLE_DEG, PARALLEL_ANGLE_RAD, ZONE_DEFAULT};
use scx_num::{Accuracy, Length, Resolution};

type V = [f64; 3];
type P = [f64; 2];
type I = [f64; 2];
type B = [f64; 6];
type R = [f64; 4];

/// The pin's `SM_USE_DEFAULT` ("use the system value"): scx's `None`.
const UD: f64 = solidcore::geom::tol::USE_DEFAULT;
/// The pin's `SM_UNINIT_TOL`, which its small-topology zone reads as "uninitialised".
const UNINIT: f64 = solidcore::geom::tol::UNINIT_TOL;
/// The pin's default tolerance set, field by field.
const DEFAULTS: [f64; 7] = [1.0e-5, 2.0e-5, 0.5e-5, 0.5, 1.1, 50.0, 1000.0];

fn small(code: i32) -> scx_num::tol::SmallTopoType {
    scx_num::tol::SmallTopoType::from_code(code.rem_euclid(5)).unwrap()
}
fn small_s(code: i32) -> solidcore::geom::tol::SmallTopoType {
    solidcore::geom::tol::SmallTopoType::from_code(code.rem_euclid(5)).unwrap()
}
/// `SM_ST_SMALL_SIZE` or `SM_ST_FORCE_SMALL_SIZE`.
fn is_small(code: i32) -> bool {
    matches!(code.rem_euclid(5), 1 | 3)
}

/// Whether scx can say the pin's set `r`: its zone is above 0 and finite, and each other
/// field in `reads` (indices into [`TolR`]) holds its default.
fn expressible(r: &TolR, reads: &[usize]) -> bool {
    let z = r.0[0];
    z.is_finite()
        && z > 0.0
        && reads
            .iter()
            .all(|&i| r.0[i].to_bits() == DEFAULTS[i].to_bits())
}
/// scx's resolution for the pin's set `r`, where [`expressible`].
fn res(r: &TolR, reads: &[usize]) -> Option<Resolution> {
    if expressible(r, reads) {
        Resolution::new(Length::from_mm(r.0[0]))
    } else {
        None
    }
}
/// scx's accuracies `a` and `b`, where both are ones.
fn accs(a: f64, b: f64) -> Option<(Accuracy, Accuracy)> {
    Accuracy::new(Length::from_mm(a)).zip(Accuracy::new(Length::from_mm(b)))
}
/// Whether `a` is an accuracy (finite, at least 0), as the pin reads a zone tolerance.
fn acc_ok(a: f64) -> bool {
    a.is_finite() && a >= 0.0
}

diff!(
    tolerances_default,
    || {
        use m::tol::{LARGE_SMALL_SIZE_RATIO, MODEL_SIZE_ESTIMATE};
        let k = Resolution::DEFAULT;
        (
            [k.zone().mm(), k.xsect().mm(), k.approx().mm()],
            [
                PARALLEL_ANGLE_DEG,
                MODEL_SIZE_ESTIMATE,
                LARGE_SMALL_SIZE_RATIO,
            ],
            PARALLEL_ANGLE_RAD,
        )
    },
    {
        let t = solidcore::kernel::Tolerances::default();
        (
            [t.zone_tol_3d, t.xsect_tol_3d, t.approx_tol_3d],
            [
                t.ang_tol_deg,
                t.model_size_estimate,
                t.large_small_size_ratio,
            ],
            t.ang_tol_rad(),
        )
    }
);
diff!(
    tolerances_ang_tol_rad,
    |r: TolR| (r.0[3] == DEFAULTS[3]).then_some(PARALLEL_ANGLE_RAD),
    (r.0[3] == DEFAULTS[3]).then(|| m::tol_set(&r).ang_tol_rad())
);
diff!(
    consts,
    || [
        m::tol::MODEL_SIZE_ESTIMATE,
        m::tol::LARGE_SMALL_SIZE_RATIO,
        ZONE_DEFAULT,
        PARALLEL_ANGLE_DEG,
        PARALLEL_ANGLE_RAD,
    ],
    [
        m::tol::MODEL_SIZE_ESTIMATE,
        m::tol::LARGE_SMALL_SIZE_RATIO,
        m::tol::ZONE_TOL_3D,
        m::tol::ANG_TOL_DEG,
        m::tol::ANG_TOL_RAD,
    ]
);
diff!(tol3d, |d: f64| m::tol::tol3d(d));
diff!(
    zone_xsect_approx_tol3d,
    |r: TolR, d: f64| {
        // `d` given, the zone is not read: any resolution
        let k = if d == UD {
            res(&r, &[])
        } else {
            Some(res(&r, &[]).unwrap_or(Resolution::DEFAULT))
        };
        let d = (d != UD).then_some(d);
        k.map(|k| {
            (
                m::tol::zone_tol3d(k, d),
                m::tol::xsect_tol3d(k, d),
                m::tol::approx_tol3d(k, d),
            )
        })
    },
    (d != UD || expressible(&r, &[])).then(|| {
        let t = m::tolk(&r);
        (
            m::tol::zone_tol3d(&t, d),
            m::tol::xsect_tol3d(&t, d),
            m::tol::approx_tol3d(&t, d),
        )
    })
);
diff!(tol_mul, |a: f64, b: f64| m::tol::tol_mul(a, b));
diff!(small_topo_codes, |c: i32| m::tol::SmallTopoType::from_code(
    c
)
.map(m::tol::SmallTopoType::code));
diff!(
    zone_for_size,
    |s: f64| scx_num::tol::zone_for_size(if s == UD {
        scx_num::tol::MODEL_SIZE_ESTIMATE
    } else {
        s
    })
    .mm(),
    solidcore::geom::tol::size_estimate_to_zone_tol_3d(s)
);
diff!(
    kernel_reads,
    |r: TolR| (
        (r.0[5] == DEFAULTS[5]).then_some(m::tol::MODEL_SIZE_ESTIMATE),
        res(&r, &[]).map(m::tol::contact_tol_3d),
    ),
    {
        let t = m::tolk(&r);
        (
            (r.0[5] == DEFAULTS[5]).then(|| m::tol::model_size_estimate(&t)),
            expressible(&r, &[]).then(|| m::tol::contact_tol_3d(&t)),
        )
    }
);
diff!(
    small_topo_tols,
    |r: TolR, c: i32| {
        // the pin's uninitialised zone skips the small-topology factor
        let k = res(&r, &[6]).filter(|_| !(is_small(c) && r.0[0] == UNINIT));
        let s = small(c);
        k.map(|k| {
            (
                m::tol::zone_tol_3d(k, s),
                m::tol::approx_tol_3d(k, s),
                m::tol::xsect_tol_3d(k, s),
            )
        })
    },
    (expressible(&r, &[6]) && !(is_small(c) && r.0[0] == UNINIT)).then(|| {
        let t = m::tolk(&r);
        let s = small_s(c);
        (
            m::tol::zone_tol_3d(&t, s),
            m::tol::approx_tol_3d(&t, s),
            m::tol::xsect_tol_3d(&t, s),
        )
    })
);
diff!(
    zones,
    |r: TolR, a: f64, b: f64| {
        let ab = accs(a, b);
        (
            ab.filter(|_| a + b != UD)
                .map(|(a, b)| m::tol::xsect_tol_3d_from_zones(a, b)),
            res(&r, &[])
                .zip(ab)
                .map(|(k, (a, b))| scx_num::coincident(k, a, b).mm()),
        )
    },
    {
        let t = m::tolk(&r);
        let ok = acc_ok(a) && acc_ok(b);
        (
            (ok && a + b != UD).then(|| m::tol::xsect_tol_3d_from_zones(&t, a, b)),
            (ok && expressible(&r, &[])).then(|| m::tol::coincidence_tol_3d(&t, a, b)),
        )
    }
);
diff!(
    coincidence_reach_3d,
    |r: TolR, a: f64, b: f64, s: f64| {
        let ab = accs(a, b);
        res(&r, &[])
            .zip(ab)
            .map(|(k, (a, b))| m::tol::coincidence_reach_3d(k, a, b, s))
    },
    (acc_ok(a) && acc_ok(b) && expressible(&r, &[])).then(|| m::tol::coincidence_reach_3d(
        &m::tolk(&r),
        a,
        b,
        s
    ))
);
diff!(
    within_coincidence_tol,
    |r: TolR, d: f64, a: f64, b: f64, s: f64| {
        let ab = accs(a, b);
        res(&r, &[])
            .zip(ab)
            .map(|(k, (a, b))| m::tol::within_coincidence_tol(k, d, a, b, s))
    },
    (acc_ok(a) && acc_ok(b) && expressible(&r, &[])).then(|| m::tol::within_coincidence_tol(
        &m::tolk(&r),
        d,
        a,
        b,
        s
    ))
);
diff!(within_contact_tol, |d: f64, c: f64, s: f64| {
    m::tol::within_contact_tol(d, c, s)
});
diff!(zone_reach_3d, |z: f64, s: f64| m::tol::zone_reach_3d(z, s));
diff!(
    fixed_zeros,
    || (
        PARALLEL_ANGLE_RAD,
        [
            m::tol::eff_zero(),
            m::tol::eff_zero_sq(),
            m::tol::eff_zero_param(),
            m::tol::eff_zero_ang_rad(),
            m::tol::scaled_zero(),
            m::tol::scaled_zero_sq(),
            m::tol::scaled_zero_sqrt(),
        ]
    ),
    (
        m::tol::ang_tol_rad(),
        [
            m::tol::eff_zero(),
            m::tol::eff_zero_sq(),
            m::tol::eff_zero_param(),
            m::tol::eff_zero_ang_rad(),
            m::tol::scaled_zero(),
            m::tol::scaled_zero_sq(),
            m::tol::scaled_zero_sqrt(),
        ]
    )
);
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
diff!(
    is_degenerate_vec3,
    |r: TolR, v: V, o: Option<f64>| {
        // an override given, the zone is not read: any resolution
        let k = if o.is_none() {
            res(&r, &[])
        } else {
            Some(res(&r, &[]).unwrap_or(Resolution::DEFAULT))
        };
        k.map(|k| m::tol::is_degenerate_vec3(k, m::v3(v), o))
    },
    (o.is_some() || expressible(&r, &[])).then(|| m::tol::is_degenerate_vec3(
        &m::tolk(&r),
        m::v3(v),
        o
    ))
);
diff!(
    are_parallel_perpendicular,
    |a: V, b: V, o: Option<f64>| {
        let ang = o.unwrap_or(PARALLEL_ANGLE_RAD);
        (
            m::tol::are_parallel(m::v3(a), m::v3(b), ang),
            m::tol::are_perpendicular(m::v3(a), m::v3(b), ang),
        )
    },
    (
        m::tol::are_parallel(m::v3(a), m::v3(b), o),
        m::tol::are_perpendicular(m::v3(a), m::v3(b), o)
    )
);
diff!(map_to_1d, |t: f64, d: V, i: I| m::e1(i)
    .map(|e| m::tol::map_to_1d(t, m::v3(d), &e)));
diff!(
    scaled_zero_of_each,
    truth_defect = crate::pin_truths::scaled_zero_of_each,
    |a: f64, b: P, c: V, d: I| {
        use m::tol::ScaledZero;
        let (b, c) = (m::v2(b), m::v3(c));
        m::e1(d).map(|d| {
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
        })
    }
);
diff!(scaled_zero_of_boxes, |e: B, f: R| {
    use m::tol::ScaledZero;
    m::e3(e).zip(m::e2(f)).map(|(e, f)| {
        [
            e.scaled_zero(),
            f.scaled_zero(),
            m::tol::scaled_zero_sq_of(&e),
            m::tol::scaled_zero_sqrt_of(&f),
        ]
    })
});
diff!(
    scaled_zero_n,
    truth_defect = crate::pin_truths::scaled_zero_n,
    |a: f64, b: V, c: I, d: B| {
        let b = m::v3(b);
        m::e1(c).zip(m::e3(d)).map(|(c, d)| {
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
        })
    }
);
