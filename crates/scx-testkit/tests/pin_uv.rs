// Written for solidcorex. Apache-2.0.
//! The pin's uv layout (research 63 D13) against the exact rational quarter arc the pin
//! builds it from, against its own inverse, and at its knots.

#![allow(
    missing_docs,
    clippy::pedantic,
    clippy::indexing_slicing,
    clippy::unwrap_used,
    clippy::float_cmp
)]

use scx_num::hash::Rng;
use scx_testkit::dd::{self, Dd};
use scx_testkit::nurbs::{Curve, Side};
use scx_testkit::pin_uv;

fn close(a: Dd, b: Dd, tol: f64) -> bool {
    (a - b).abs().to_f64() <= tol
}

/// The pin's quarter arc, as a NURBS with its double weight `cos(pi/4)` rounded: its angle
/// at `s` (exact, then the angle in double-double) is the closed form's, to that rounding.
#[test]
fn the_quarter_arc_map_is_the_pins_rational_arc() {
    let w = core::f64::consts::FRAC_1_SQRT_2;
    let arc = Curve::new(
        2,
        &[0.0, 0.0, 0.0, 1.0, 1.0, 1.0],
        &[[1.0, 0.0], [1.0, 1.0], [0.0, 1.0]],
        Some(&[1.0, w, 1.0]),
    )
    .unwrap();
    let mut rng = Rng::new(0x7301);
    for i in 0..200 {
        let s = if i == 0 { 0.5 } else { rng.next_f64() };
        let [x, y] = arc.point(s, Side::Above).unwrap().exact;
        let angle = dd::atan2(scx_testkit::poly::to_dd(&y), scx_testkit::poly::to_dd(&x));
        // The weight's rounding moves the arc by about 1e-17.
        assert!(
            close(pin_uv::quarter_angle(Dd::new(s)), angle, 1e-16),
            "{s}"
        );
        assert!(
            close(pin_uv::quarter_param(angle), Dd::new(s), 1e-16),
            "{s}"
        );
    }
}

#[test]
fn the_maps_invert_each_other() {
    let mut rng = Rng::new(0x7302);
    let two_pi = dd::consts().pi * 2.0;
    for _ in 0..500 {
        let theta = two_pi * rng.next_f64();
        let f = pin_uv::turn_fraction(theta);
        assert!((0.0..1.0).contains(&f.to_f64()));
        assert!(close(pin_uv::fraction_turn(f), theta, 1e-30));
        let u = pin_uv::cylinder_u(theta);
        assert!(close(pin_uv::cylinder_angle(u), theta, 1e-30));
        let lat = dd::consts().pi * (rng.next_f64() - 0.5);
        let v = pin_uv::latitude_fraction(lat);
        assert!(close(pin_uv::fraction_latitude(v), lat, 1e-30));
    }
}

#[test]
fn the_knots_are_the_quarter_turns() {
    let c = dd::consts();
    let quarter = |k: f64| c.pio2 * k;
    for k in 0..4 {
        let kf = k as f64;
        assert!(close(
            pin_uv::turn_fraction(quarter(kf)),
            Dd::new(kf / 4.0),
            1e-31
        ));
        assert!(close(
            pin_uv::cylinder_u(quarter(kf)),
            Dd::new(pin_uv::CYLINDER_U_KNOTS[k]),
            1e-30
        ));
    }
    assert_eq!(pin_uv::CYLINDER_U_KNOTS[3], 3.0 * core::f64::consts::PI);
    assert!(close(
        pin_uv::latitude_fraction(Dd::new(0.0)),
        Dd::new(0.5),
        1e-31
    ));
    assert!(close(
        pin_uv::latitude_fraction(-c.pio2),
        Dd::new(0.0),
        1e-31
    ));
    // Half a quarter arc: s = sqrt2 t / (1 + (sqrt2 - 1) t) at t = tan(pi/8) = sqrt2 - 1
    // is (2 - sqrt2) / (4 - 2 sqrt2) = 1/2.
    assert!(close(
        pin_uv::quarter_param(c.pio2 * 0.5),
        Dd::new(0.5),
        1e-31
    ));
    // A turn and a negative angle reduce.
    assert!(close(pin_uv::turn(c.pi * 5.0), c.pi, 1e-30));
    assert!(close(pin_uv::turn(-c.pio2), c.pi * 1.5, 1e-30));
}
