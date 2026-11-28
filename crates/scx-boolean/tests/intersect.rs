// Written for solidcorex. Apache-2.0.
//! [`intersect`] (BO18) judged against closed forms: the lengths of the circles, ellipses
//! and lines where two bodies meet, and the counts of curves and points, for box, cylinder,
//! sphere and torus pairs. No answer is read from the pin.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

use core::f64::consts::{PI, TAU};

use scx_boolean::{BooleanError, intersect};
use scx_brep::op::{Cx, Done, NoTrace, Scratch, Settings};
use scx_brep::{Brep, Document};
use scx_mass::RelAccuracy;
use scx_num::budget::{Budget, Stop};
use scx_num::fmath;
use scx_num::frame::Frame;
use scx_num::units::Length;
use scx_num::vector3d::{Point3, Vec3};
use scx_testkit::bodies::made::{block, cylinder, sphere, torus};

fn frame(o: [f64; 3], z: [f64; 3]) -> Frame {
    let x = if z[2].abs() > 0.5 {
        [1.0, 0.0, 0.0]
    } else {
        [0.0, 0.0, 1.0]
    };
    Frame::new(
        Point3::new(o[0], o[1], o[2]),
        Vec3::new(z[0], z[1], z[2]),
        Vec3::new(x[0], x[1], x[2]),
    )
    .unwrap()
}

fn cuboid(lo: [f64; 3], hi: [f64; 3], doc: Document) -> Brep {
    let size = [0, 1, 2].map(|i| Length::from_mm(hi[i] - lo[i]));
    block(&frame(lo, [0.0, 0.0, 1.0]), size, &doc).unwrap().body
}

fn cyl(o: [f64; 3], z: [f64; 3], r: f64, h: f64, doc: Document) -> Brep {
    cylinder(&frame(o, z), Length::from_mm(r), Length::from_mm(h), &doc)
        .unwrap()
        .body
}

fn ball(o: [f64; 3], r: f64, doc: Document) -> Brep {
    sphere(&frame(o, [0.0, 0.0, 1.0]), Length::from_mm(r), &doc)
        .unwrap()
        .body
}

fn run(a: &Brep, b: &Brep, doc: Document) -> Result<Done<scx_boolean::NoItems>, BooleanError> {
    let (mut scratch, stop, settings) = (Scratch::new(), Stop::new(), Settings::new());
    let mut cx = Cx::new(&doc, &settings, &mut scratch, &stop, &NoTrace);
    intersect(a, b, &mut cx)
}

/// The answer: finished by the draft's own check (a body of no faces, wires and lone
/// vertices only; the solid contract does not apply to it).
fn meet(a: &Brep, b: &Brep, doc: Document) -> Brep {
    let body = run(a, b, doc).unwrap().body;
    assert_eq!(body.faces().count(), 0);
    body
}

/// Each edge's length, ascending.
fn lengths(body: &Brep) -> Vec<f64> {
    let mut v: Vec<f64> = body
        .edges()
        .map(|e| {
            scx_mass::edge_length(
                e,
                RelAccuracy::new(1e-10).unwrap(),
                &mut Scratch::new(),
                &mut Budget::unlimited(),
                &Stop::new(),
            )
            .unwrap()
            .value
        })
        .collect();
    v.sort_by(f64::total_cmp);
    v
}

#[track_caller]
fn same(got: &[f64], want: &[f64]) {
    let mut want = want.to_vec();
    want.sort_by(f64::total_cmp);
    assert_eq!(got.len(), want.len(), "{got:?} against {want:?}");
    for (g, w) in got.iter().zip(&want) {
        assert!(
            (g - w).abs() <= 1e-7 * w.max(1.0),
            "{got:?} against {want:?}"
        );
    }
}

#[test]
fn bodies_apart_meet_nowhere() {
    let doc = Document::default();
    let body = meet(
        &cuboid([0.0; 3], [1.0; 3], doc),
        &cuboid([3.0; 3], [4.0; 3], doc),
        doc,
    );
    assert_eq!(body.shells().count(), 0);
    assert_eq!(body.edges().count(), 0);
    assert_eq!(body.vertices().count(), 0);
}

#[test]
fn overlapping_cubes_meet_in_six_half_unit_lines() {
    let doc = Document::default();
    let a = cuboid([0.0; 3], [1.0; 3], doc);
    let b = cuboid([0.5; 3], [1.5; 3], doc);
    let body = meet(&a, &b, doc);
    same(&lengths(&body), &[0.5; 6]);
    // The two corners' 3 + 3 lines meet at each corner of the other: (1, 0.5, 0.5) and
    // the like are 6 vertices, in one connected shell.
    assert_eq!(body.vertices().count(), 6);
    assert_eq!(body.shells().count(), 1);
}

#[test]
fn a_cylinder_through_a_plate_meets_it_in_two_unit_circles() {
    let doc = Document::default();
    let a = cuboid([-2.0, -2.0, 0.0], [2.0, 2.0, 1.0], doc);
    let b = cyl([0.0, 0.0, -1.0], [0.0, 0.0, 1.0], 1.0, 3.0, doc);
    let body = meet(&a, &b, doc);
    same(&lengths(&body), &[TAU, TAU]);
    // Two circles apart: two shells.
    assert_eq!(body.shells().count(), 2);
}

#[test]
fn a_cylinder_on_a_blocks_side_meets_it_in_two_arcs_and_two_rulings() {
    let doc = Document::default();
    let a = cuboid([-2.0, -2.0, 0.0], [2.0, 2.0, 1.0], doc);
    let b = cyl([2.0, 0.0, -1.0], [0.0, 0.0, 1.0], 1.0, 3.0, doc);
    let body = meet(&a, &b, doc);
    same(&lengths(&body), &[PI, PI, 1.0, 1.0]);
    // The arcs and rulings close one loop through four corners.
    assert_eq!(body.vertices().count(), 4);
    assert_eq!(body.shells().count(), 1);
}

#[test]
fn a_plane_square_to_a_cylinders_axis_meets_it_in_one_unit_circle() {
    let doc = Document::default();
    let a = cyl([-1.0, 0.0, 0.0], [1.0, 0.0, 0.0], 1.0, 2.5, doc);
    let b = cuboid([0.5, -2.0, -2.0], [2.0, 2.0, 2.0], doc);
    let body = meet(&a, &b, doc);
    same(&lengths(&body), &[TAU]);
}

#[test]
fn a_plate_tilted_to_a_cylinders_axis_meets_it_in_two_ellipses() {
    // The unit cylinder along z, and a plate of thickness 1 whose faces are planes with
    // normal (0, -1, 1) / sqrt 2: each face cuts the wall in an ellipse of semi-axes
    // 1 and sqrt 2, whose perimeter is the integral of sqrt(a^2 sin^2 + b^2 cos^2).
    let doc = Document::default();
    let s = 0.5_f64.sqrt();
    // x' = x, z' = (0, -s, s), y' = z' x x' = (0, s, s).
    let f = Frame::new(
        Point3::new(-5.0, -5.0 * s, -5.0 * s),
        Vec3::new(0.0, -s, s),
        Vec3::new(1.0, 0.0, 0.0),
    )
    .unwrap();
    let plate = block(&f, [10.0, 10.0, 1.0].map(Length::from_mm), &doc)
        .unwrap()
        .body;
    let a = cyl([0.0, 0.0, -3.0], [0.0, 0.0, 1.0], 1.0, 6.0, doc);
    let body = meet(&a, &plate, doc);
    let (p, n) = (2.0_f64.sqrt(), 20_000_u32);
    let h = TAU / f64::from(n);
    let perimeter: f64 = (0..n)
        .map(|k| {
            let t = (f64::from(k) + 0.5) * h;
            (p * p * fmath::sin(t) * fmath::sin(t) + fmath::cos(t) * fmath::cos(t)).sqrt() * h
        })
        .sum();
    let l = lengths(&body);
    assert_eq!(l.len() % 2, 0, "{l:?}");
    assert!(
        (l.iter().sum::<f64>() - 2.0 * perimeter).abs() < 1e-6,
        "{l:?}"
    );
}

#[test]
fn two_unit_spheres_meet_in_one_circle_of_radius_root_three_over_two() {
    let doc = Document::default();
    let a = ball([0.0; 3], 1.0, doc);
    let b = ball([1.0, 0.0, 0.0], 1.0, doc);
    let body = meet(&a, &b, doc);
    let circle = 3.0_f64.sqrt() * PI;
    // The circle's two arcs, cut at the seams.
    let l = lengths(&body);
    assert!((l.iter().sum::<f64>() - circle).abs() < 1e-7, "{l:?}");
}

#[test]
fn a_plane_through_a_spheres_centre_meets_it_in_a_great_circle() {
    let doc = Document::default();
    let a = ball([0.0; 3], 1.0, doc);
    let b = cuboid([0.0, -2.0, -2.0], [2.0, 2.0, 2.0], doc);
    let body = meet(&a, &b, doc);
    let l = lengths(&body);
    assert!((l.iter().sum::<f64>() - TAU).abs() < 1e-7, "{l:?}");
}

#[test]
fn a_plane_tangent_to_a_sphere_meets_it_in_one_point() {
    let doc = Document::default();
    let a = ball([0.0; 3], 1.0, doc);
    let b = cuboid([-2.0, 1.0, -2.0], [2.0, 2.0, 2.0], doc);
    let body = meet(&a, &b, doc);
    assert_eq!(body.edges().count(), 0);
    assert_eq!(body.vertices().count(), 1);
    assert_eq!(body.shells().count(), 1);
    let v = body.vertices().next().unwrap();
    assert!(v.record().point.distance(Point3::new(0.0, 1.0, 0.0)) < 1e-9);
}

#[test]
fn the_mid_plane_of_a_torus_meets_it_in_its_inner_and_outer_circles() {
    // R = 5, r = 1.5: the circles of radius 3.5 and 6.5 in z = 0.
    let doc = Document::default();
    let a = torus(
        &frame([0.0; 3], [0.0, 0.0, 1.0]),
        Length::from_mm(5.0),
        Length::from_mm(1.5),
        &doc,
    )
    .unwrap()
    .body;
    let b = cuboid([-8.0, -8.0, 0.0], [8.0, 8.0, 3.0], doc);
    let body = meet(&a, &b, doc);
    let l = lengths(&body);
    assert!((l.iter().sum::<f64>() - TAU * 10.0).abs() < 1e-6, "{l:?}");
}

#[test]
fn the_answer_does_not_depend_on_the_order_of_the_operands_and_repeats() {
    let doc = Document::default();
    let a = cuboid([-2.0, -2.0, 0.0], [2.0, 2.0, 1.0], doc);
    let b = cyl([2.0, 0.0, -1.0], [0.0, 0.0, 1.0], 1.0, 3.0, doc);
    let ab = meet(&a, &b, doc);
    let ba = meet(&b, &a, doc);
    same(&lengths(&ab), &lengths(&ba));
    assert_eq!(ab.vertices().count(), ba.vertices().count());
    let again = meet(&a, &b, doc);
    assert_eq!(lengths(&ab), lengths(&again));
}

#[test]
fn an_operand_edge_lying_on_the_other_body_is_one_edge() {
    // Two unit cubes sharing a face: the square they share is a coincident region; its
    // four boundary edges are the answer, each once.
    let doc = Document::default();
    let a = cuboid([0.0; 3], [1.0; 3], doc);
    let b = cuboid([1.0, 0.0, 0.0], [2.0, 1.0, 1.0], doc);
    let body = meet(&a, &b, doc);
    same(&lengths(&body), &[1.0; 4]);
    assert_eq!(body.vertices().count(), 4);
}
