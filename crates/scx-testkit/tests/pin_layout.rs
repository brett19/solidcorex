// Written for solidcorex. Apache-2.0.
//! The pin's uv layout (research 63 D13, `quadric::Implicit::pin_layout`) against the exact
//! rational quarter arc the pin builds it from, against its own inverse, and at its knots.

#![allow(
    missing_docs,
    clippy::pedantic,
    clippy::indexing_slicing,
    clippy::unwrap_used,
    clippy::float_cmp
)]

use scx_num::hash::Rng;
use scx_testkit::analytic::Frame;
use scx_testkit::bound::rat_to_dd;
use scx_testkit::dd::{self, Dd};
use scx_testkit::nurbs::{Curve, Side};
use scx_testkit::quadric::{Implicit, Layout, Param};

fn close(a: Dd, b: Dd, tol: f64) -> bool {
    (a - b).abs().to_f64() <= tol
}

const FRAME: Frame = Frame {
    origin: [0.0; 3],
    x: [1.0, 0.0, 0.0],
    y: [0.0, 1.0, 0.0],
    z: [0.0, 0.0, 1.0],
};

fn cylinder() -> Layout {
    Implicit::Cylinder {
        frame: FRAME,
        radius: 1.0,
    }
    .pin_layout()
}

fn cone() -> Layout {
    Implicit::Cone {
        frame: FRAME,
        radius: 1.0,
        slope: Dd::new(0.5),
    }
    .pin_layout()
}

fn sphere() -> Layout {
    Implicit::Sphere {
        frame: FRAME,
        radius: 1.0,
    }
    .pin_layout()
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
    let quarter = Param::arcs((0.0, 1.0), Dd::new(0.0), dd::consts().pio2);
    let mut rng = Rng::new(0x7301);
    for i in 0..200 {
        let s = if i == 0 { 0.5 } else { rng.next_f64() };
        let [x, y] = arc.point(s, Side::Above).unwrap().exact;
        let angle = dd::atan2(rat_to_dd(&y), rat_to_dd(&x));
        // The weight's rounding moves the arc by about 1e-17.
        assert!(close(quarter.to_step(Dd::new(s)), angle, 1e-16), "{s}");
        assert!(close(quarter.to_nurb(angle), Dd::new(s), 1e-16), "{s}");
    }
}

#[test]
fn the_maps_invert_each_other() {
    let mut rng = Rng::new(0x7302);
    let two_pi = dd::consts().pi * 2.0;
    let (cyl, cone, sphere) = (cylinder(), cone(), sphere());
    for _ in 0..500 {
        let theta = two_pi * rng.next_f64();
        let h = Dd::new(rng.next_signed_unit() * 4.0);
        let (f, t) = cone.to_nurb((theta, h));
        assert!((0.0..=1.0).contains(&f.to_f64()));
        assert_eq!(t, h);
        let (a, b) = cone.to_step((f, t));
        assert!(close(a, theta, 1e-30) && b == h);
        let (u, _) = cyl.to_nurb((theta, h));
        assert!((0.0..=4.0 * core::f64::consts::PI).contains(&u.to_f64()));
        assert!(close(cyl.to_step((u, h)).0, theta, 1e-30));
        let lat = dd::consts().pi * (rng.next_f64() - 0.5);
        let (_, v) = sphere.to_nurb((theta, lat));
        assert!((0.0..=1.0).contains(&v.to_f64()));
        assert!(close(sphere.to_step((Dd::new(0.5), v)).1, lat, 1e-30));
    }
}

#[test]
fn the_knots_are_the_quarter_turns() {
    let c = dd::consts();
    let quarter = |k: f64| c.pio2 * k;
    let Param::Arcs { breaks, .. } = cylinder().u else {
        panic!("the cylinder's u is arcs")
    };
    assert_eq!(breaks.len(), 5);
    assert_eq!(breaks[3], 3.0 * core::f64::consts::PI);
    let (cyl, cone, sphere) = (cylinder(), cone(), sphere());
    for (k, &brk) in breaks.iter().take(4).enumerate() {
        let kf = k as f64;
        let zero = Dd::new(0.0);
        assert!(close(
            cone.to_nurb((quarter(kf), zero)).0,
            Dd::new(kf / 4.0),
            1e-31
        ));
        assert!(close(
            cyl.to_nurb((quarter(kf), zero)).0,
            Dd::new(brk),
            1e-30
        ));
    }
    let lat = |v: Dd| sphere.to_nurb((Dd::new(0.0), v)).1;
    assert!(close(lat(Dd::new(0.0)), Dd::new(0.5), 1e-31));
    assert!(close(lat(-c.pio2), Dd::new(0.0), 1e-31));
    // Half a quarter arc: s = sqrt2 t / (1 + (sqrt2 - 1) t) at t = tan(pi/8) = sqrt2 - 1
    // is (2 - sqrt2) / (4 - 2 sqrt2) = 1/2.
    let arc = Param::arcs((0.0, 1.0), Dd::new(0.0), c.pio2);
    assert!(close(arc.to_nurb(c.pio2 * 0.5), Dd::new(0.5), 1e-31));
}
