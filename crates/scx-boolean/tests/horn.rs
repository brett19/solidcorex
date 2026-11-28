// Written for solidcorex. Apache-2.0.
//! A horn torus (MK4b: major = minor = R, the tube meets the axis at a pole) against a
//! block and against a cylinder, judged by closed-form volumes.
//!
//! Below the plane `z = h` the horn solid (the points with `(rho - R)^2 + z^2 <= R^2`) is the
//! solid of revolution between `rho = R -+ sqrt(R^2 - z^2)`, whose ring at height `z` has area
//! `4 pi R sqrt(R^2 - z^2)`, so `V(h) = 2 pi R (h sqrt(R^2 - h^2) + R^2 asin(h / R) + pi R^2 / 2)`
//! and the whole is `2 pi^2 R^3`. Inside the cylinder `rho <= R` about the axis the ring at `z`
//! has area `pi (R^2 - (R - s)^2)` with `s = sqrt(R^2 - z^2)`: `pi R^3 (pi - 4/3)`.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

use core::f64::consts::PI;

use scx_boolean::{BooleanError, BooleanOp, boolean};
use scx_brep::op::{Cx, NoTrace, Scratch, Settings};
use scx_brep::{Brep, Document};
use scx_num::budget::{Budget, Stop};
use scx_num::frame::Frame;
use scx_num::units::Length;
use scx_num::vector3d::{Point3, Vec3};
use scx_testkit::bodies::made::{block, cylinder, torus};

const R: f64 = 2.0;

fn mm(v: f64) -> Length {
    Length::from_mm(v)
}

fn at(o: [f64; 3]) -> Frame {
    Frame::new(
        Point3::new(o[0], o[1], o[2]),
        Vec3::new(0.0, 0.0, 1.0),
        Vec3::new(1.0, 0.0, 0.0),
    )
    .unwrap()
}

/// The torus of major `big` and minor `small` about the z axis: a horn torus when equal.
fn tube(big: f64, small: f64) -> Brep {
    torus(&at([0.0; 3]), mm(big), mm(small), &Document::default())
        .unwrap()
        .body
}

fn volume(b: &Brep) -> f64 {
    scx_mass::volume(
        b,
        scx_mass::RelAccuracy::new(1e-9).unwrap(),
        &mut Scratch::new(),
        &mut Budget::unlimited(),
        &Stop::new(),
    )
    .unwrap()
    .value
}

fn op(a: &Brep, b: &Brep, op: BooleanOp) -> Result<Brep, BooleanError> {
    let doc = Document::default();
    let (settings, mut scratch, stop) = (Settings::new(), Scratch::new(), Stop::new());
    let mut cx = Cx::new(&doc, &settings, &mut scratch, &stop, &NoTrace);
    boolean(a, b, op, &mut cx).map(|d| d.body)
}

/// The torus solid's volume below `z = h`: the ring at height `z` has area
/// `4 pi big sqrt(small^2 - z^2)`.
fn below(big: f64, small: f64, h: f64) -> f64 {
    let s = (small * small - h * h).sqrt();
    2.0 * PI * big * (h * s + small * small * (h / small).asin() + PI * small * small / 2.0)
}

fn whole(big: f64, small: f64) -> f64 {
    2.0 * PI * PI * big * small * small
}

fn near(got: f64, want: f64, what: &str) {
    assert!(
        (got - want).abs() <= 1e-7 * want,
        "{what}: {got} against {want}"
    );
}

/// A block over `z` in `[-3, h]` and `x, y` in `[-5, 5]`: its top is a plane square to the
/// axis, cutting both rings of the torus at `z = h`; its sides are clear of it.
fn slab(h: f64) -> Brep {
    block(
        &at([-5.0, -5.0, -3.0]),
        [mm(10.0), mm(10.0), mm(3.0 + h)],
        &Document::default(),
    )
    .unwrap()
    .body
}

/// The cut by a plane square to the axis: intersection, difference and union volumes.
fn cut_by_slab(big: f64, small: f64, h: f64) -> Result<(), BooleanError> {
    let (t, b) = (tube(big, small), slab(h));
    let kept = below(big, small, h);
    let what = format!("{big}/{small} h {h}");
    near(
        volume(&op(&t, &b, BooleanOp::Intersect)?),
        kept,
        &format!("intersect {what}"),
    );
    near(
        volume(&op(&t, &b, BooleanOp::Subtract)?),
        whole(big, small) - kept,
        &format!("subtract {what}"),
    );
    near(
        volume(&op(&t, &b, BooleanOp::Union)?),
        volume(&b) + whole(big, small) - kept,
        &format!("union {what}"),
    );
    Ok(())
}

/// The cut by the cylinder `rho <= big` about the axis (through the pole of a horn torus):
/// the ring at `z` lies between `big - s` and `big + s`, `s = sqrt(small^2 - z^2)`, so
/// `pi (2 big s - s^2)` is inside: `pi (pi big small^2 - 4 small^3 / 3)`.
fn cut_by_cylinder(big: f64, small: f64) -> Result<(), BooleanError> {
    let t = tube(big, small);
    let c = cylinder(
        &at([0.0, 0.0, -3.0]),
        mm(big),
        mm(6.0),
        &Document::default(),
    )
    .unwrap()
    .body;
    let inside = PI * (PI * big * small * small - 4.0 * small.powi(3) / 3.0);
    near(
        volume(&op(&t, &c, BooleanOp::Intersect)?),
        inside,
        "cylinder intersect",
    );
    near(
        volume(&op(&t, &c, BooleanOp::Subtract)?),
        whole(big, small) - inside,
        "cylinder subtract",
    );
    Ok(())
}

#[test]
fn the_ring_torus_controls() {
    println!("ring block: {:?}", cut_by_slab(3.0, 1.0, 0.5));
    println!("ring cylinder: {:?}", cut_by_cylinder(3.0, 1.0));
}

#[test]
fn a_block_cuts_a_horn_torus_by_a_plane_square_to_its_axis() {
    for h in [0.5, -0.75] {
        println!("horn block {h}: {:?}", cut_by_slab(R, R, h));
    }
}

/// The plane through the pole: its section is the outer circle and the pole itself, a touch.
#[test]
fn a_block_whose_face_passes_through_the_pole() {
    println!("through the pole: {:?}", cut_by_slab(R, R, 0.0));
}

#[test]
fn a_cylinder_about_the_axis_cuts_a_horn_torus() {
    println!("horn cylinder: {:?}", cut_by_cylinder(R, R));
}
