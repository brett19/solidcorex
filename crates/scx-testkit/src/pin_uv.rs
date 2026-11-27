// Written for solidcorex. Apache-2.0.
//! The pin's NURBS-compatible uv of the analytic surfaces (research 63 D13), in closed form
//! and double-double, for the canonical surfaces the intersection truths report uv on
//! ([`crate::curve_surface`], [`crate::ssi`]).
//!
//! Read from the pin (its canonical constructors, `swap_uv` and `inside_out` off, a full
//! turn; design note docs/research/64-intersection-truths.md §2):
//!
//! - every angle direction is four standard-form quarter arcs from the frame's `x` (end
//!   weights 1, middle weight `cos(pi/4)`), one knot span each; the sphere's latitude is
//!   two such arcs from the south pole (`-pi/2`);
//! - on one quarter arc at the span's local parameter `s` in `[0, 1]`, the angle `phi`
//!   from the arc's start has `tan(phi / 2) = s / (s + sqrt2 (1 - s))`, and back
//!   `s = sqrt2 t / (1 + (sqrt2 - 1) t)` with `t = tan(phi / 2)`;
//! - the cylinder's u knots are `k pi` (`[0, 4 pi]`, as the pin rounds them); the cone's,
//!   sphere's and torus's angle knots are `k / 4` (`[0, 1]`), the sphere's latitude knots
//!   `k / 2`;
//! - the cylinder's and cone's v is the height along the axis, the STEP v itself; the
//!   plane's uv is the STEP uv.
//!
//! A swapped, inside-out or re-parameterised surface maps its uv further (the pin's flags);
//! the truths here state the canonical layout only.

use crate::dd::{self, Dd};

/// The cylinder's u knots at the pin: `k pi` for `k = 0..=4`, each rounded once.
pub const CYLINDER_U_KNOTS: [f64; 5] = [
    0.0,
    core::f64::consts::PI,
    2.0 * core::f64::consts::PI,
    3.0 * core::f64::consts::PI,
    4.0 * core::f64::consts::PI,
];

fn sqrt2() -> Dd {
    Dd::new(2.0).sqrt()
}

/// The local parameter `s` in `[0, 1]` of the angle `phi` in `[0, pi/2]` on a quarter arc.
pub fn quarter_param(phi: Dd) -> Dd {
    let (sn, cs) = dd::sin_cos(phi);
    let t = sn / (Dd::new(1.0) + cs);
    let r2 = sqrt2();
    r2 * t / (Dd::new(1.0) + (r2 - Dd::new(1.0)) * t)
}

/// The angle in `[0, pi/2]` of the local parameter `s` in `[0, 1]` on a quarter arc.
pub fn quarter_angle(s: Dd) -> Dd {
    let t = s / (s + sqrt2() * (Dd::new(1.0) - s));
    dd::atan(t) * 2.0
}

/// `theta` reduced into `[0, 2 pi)`.
pub fn turn(theta: Dd) -> Dd {
    let two_pi = dd::consts().pi * 2.0;
    let mut t = theta;
    while t.hi < 0.0 {
        t = t + two_pi;
    }
    while t.hi >= two_pi.hi {
        t = t - two_pi;
    }
    t
}

/// The span (`0..arcs`) and local parameter of an angle `a` from the arcs' start, `a` in
/// `[0, arcs pi/2]`.
fn span(a: Dd, arcs: usize) -> (usize, Dd) {
    let q = dd::consts().pio2;
    let mut k = ((a.hi / q.hi).floor().max(0.0) as usize).min(arcs - 1);
    let mut phi = a - q * k as f64;
    if phi.hi < 0.0 && k > 0 {
        k -= 1;
        phi = phi + q;
    }
    (k, quarter_param(phi))
}

/// The angle from the start of the local parameter `s` on span `k`.
fn unspan(k: usize, s: Dd) -> Dd {
    dd::consts().pio2 * k as f64 + quarter_angle(s)
}

/// An angle (any turn) as the fraction in `[0, 1)` over four quarter arcs from angle zero:
/// the cone's, sphere's and torus's u, the torus's v.
pub fn turn_fraction(theta: Dd) -> Dd {
    let (k, s) = span(turn(theta), 4);
    (s + Dd::new(k as f64)) * 0.25
}

/// The angle in `[0, 2 pi]` of a fraction in `[0, 1]` over four quarter arcs.
pub fn fraction_turn(f: Dd) -> Dd {
    let x = f * 4.0;
    let k = (x.hi.floor().max(0.0) as usize).min(3);
    unspan(k, x - Dd::new(k as f64))
}

/// The cylinder's u of an angle (any turn): `knot_k + s (knot_(k+1) - knot_k)`.
pub fn cylinder_u(theta: Dd) -> Dd {
    let (k, s) = span(turn(theta), 4);
    let lo = CYLINDER_U_KNOTS.get(k).copied().unwrap_or(0.0);
    let hi = CYLINDER_U_KNOTS.get(k + 1).copied().unwrap_or(lo);
    Dd::new(lo) + s * Dd::diff(hi, lo)
}

/// The angle in `[0, 2 pi]` of the cylinder's u in `[0, 4 pi]`.
pub fn cylinder_angle(u: Dd) -> Dd {
    let k = CYLINDER_U_KNOTS
        .windows(2)
        .position(|w| w.get(1).is_some_and(|&h| u.hi < h))
        .unwrap_or(3);
    let lo = CYLINDER_U_KNOTS.get(k).copied().unwrap_or(0.0);
    let hi = CYLINDER_U_KNOTS.get(k + 1).copied().unwrap_or(lo);
    unspan(k, (u - Dd::new(lo)) / Dd::diff(hi, lo))
}

/// A latitude in `[-pi/2, pi/2]` as the sphere's v in `[0, 1]`: two quarter arcs from the
/// south pole.
pub fn latitude_fraction(lat: Dd) -> Dd {
    let (k, s) = span(lat + dd::consts().pio2, 2);
    (s + Dd::new(k as f64)) * 0.5
}

/// The latitude of the sphere's v in `[0, 1]`.
pub fn fraction_latitude(f: Dd) -> Dd {
    let x = f * 2.0;
    let k = (x.hi.floor().max(0.0) as usize).min(1);
    unspan(k, x - Dd::new(k as f64)) - dd::consts().pio2
}
