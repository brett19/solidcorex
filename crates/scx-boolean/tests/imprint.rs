// Written for solidcorex. Apache-2.0.
//! The public [`imprint`] on blocks and a cylinder, judged against closed forms (75 §8.2,
//! BO13): the face, edge and vertex counts and the face areas of the imprinted target, its
//! volume and outward sense unchanged, and `Done.changes` empty exactly when nothing was cut.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

use core::f64::consts::PI;

use scx_boolean::NoItems;
use scx_boolean::{BooleanError, BooleanRefusal, Imprint, imprint};
use scx_brep::op::{Cx, Done, NoTrace, Scratch, Settings};
use scx_brep::{Brep, Document, Entity, Operand, Origin};
use scx_mass::RelAccuracy;
use scx_num::budget::{Budget, Stop};
use scx_num::frame::Frame;
use scx_num::units::{Length, Resolution};
use scx_num::vector3d::{Point3, Vec3};
use scx_testkit::bodies::made::{block, cylinder};

fn frame(o: [f64; 3]) -> Frame {
    Frame::new(
        Point3::new(o[0], o[1], o[2]),
        Vec3::new(0.0, 0.0, 1.0),
        Vec3::new(1.0, 0.0, 0.0),
    )
    .unwrap()
}

fn cuboid(lo: [f64; 3], hi: [f64; 3], doc: Document) -> Brep {
    let size = [0, 1, 2].map(|i| Length::from_mm(hi[i] - lo[i]));
    block(&frame(lo), size, &doc).unwrap().body
}

fn run(target: &Brep, tool: &Brep, what: Imprint) -> Result<Done<NoItems>, BooleanError> {
    let doc = Document::default();
    let settings = Settings::new();
    let mut scratch = Scratch::new();
    let stop = Stop::new();
    let mut cx = Cx::new(&doc, &settings, &mut scratch, &stop, &NoTrace);
    imprint(target, tool, what, &mut cx)
}

fn volume(body: &Brep) -> f64 {
    scx_mass::volume(
        body,
        RelAccuracy::new(1e-10).unwrap(),
        &mut Scratch::new(),
        &mut Budget::unlimited(),
        &Stop::new(),
    )
    .unwrap()
    .value
}

/// The areas of the body's faces, ascending.
fn areas(body: &Brep) -> Vec<f64> {
    let mut v: Vec<f64> = body
        .faces()
        .map(|f| {
            scx_mass::face_area(
                f,
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

fn assert_areas(body: &Brep, want: &[f64]) {
    let got = areas(body);
    assert_eq!(got.len(), want.len(), "{got:?}");
    for (g, w) in got.iter().zip(want) {
        assert!(
            (g - w).abs() <= 1e-7 * w.max(1.0),
            "{got:?} against {want:?}"
        );
    }
}

/// `(vertices, edges, faces)`.
fn counts(body: &Brep) -> (usize, usize, usize) {
    (
        body.vertices().len(),
        body.edges().len(),
        body.faces().len(),
    )
}

#[test]
fn overlapping_cubes_cut_three_faces_into_a_square_and_an_l() {
    // b = a + (0.5, 0.5, 0.5): each of a's faces x = 1, y = 1, z = 1 is cut by two sections
    // into the quarter [0.5, 1]^2 (area 1/4) and an L (3/4). Edges: 12 + 3 cut where they
    // cross b's face + 6 sections; vertices 8 + 3 + 3 where b's edges pierce a's faces.
    let doc = Document::default();
    let a = cuboid([0.0; 3], [1.0; 3], doc);
    let b = cuboid([0.5; 3], [1.5; 3], doc);
    for what in [Imprint::Edges, Imprint::EdgesAndFaces] {
        let done = run(&a, &b, what).unwrap();
        assert_eq!(counts(&done.body), (14, 21, 9), "{what:?}");
        assert_areas(
            &done.body,
            &[0.25, 0.25, 0.25, 0.75, 0.75, 0.75, 1.0, 1.0, 1.0],
        );
        let (v, w) = (volume(&done.body), volume(&a));
        assert!((v - 1.0).abs() < 1e-9 && (w - 1.0).abs() < 1e-9, "{v} {w}");
        assert!(!done.changes.is_empty());
        // Euler: V - E + F = 2 for a sphere-like shell.
        let (v, e, f) = counts(&done.body);
        assert_eq!(v + f, e + 2);
    }
}

#[test]
fn sections_are_new_edges_from_both_operands() {
    let doc = Document::default();
    let a = cuboid([0.0; 3], [1.0; 3], doc);
    let b = cuboid([0.5; 3], [1.5; 3], doc);
    let done = run(&a, &b, Imprint::Edges).unwrap();
    // The six sections lie on a face of a and a face of b (the tool's, operand 1).
    let from_both = done
        .body
        .edges()
        .filter(|e| {
            let Some(o) = done.changes.edges.origins(e.id()) else {
                return false;
            };
            o.iter().any(|x| matches!(x, Origin::Face(_)))
                && o.iter().any(|x| {
                    matches!(x, Origin::Operand(op, Entity::Face(_)) if *op == Operand::from_raw(1))
                })
        })
        .count();
    assert_eq!(from_both, 6);
}

#[test]
fn a_cylinder_through_a_block_cuts_a_disk_from_the_top_and_bottom() {
    // Radius 1/4 along z through the middle of the unit cube, from z = -1 to 2: top and
    // bottom are each a disk (pi/16) and the square with a hole (1 - pi/16); the four
    // sides are whole. V = 8 + 2 (one on each circle), E = 12 + 2, F = 8; the two holes
    // are loops, so V - E + F - 2 = 2.
    let doc = Document::default();
    let a = cuboid([0.0; 3], [1.0; 3], doc);
    let b = cylinder(
        &Frame::new(
            Point3::new(0.5, 0.5, -1.0),
            Vec3::new(0.0, 0.0, 1.0),
            Vec3::new(1.0, 0.0, 0.0),
        )
        .unwrap(),
        Length::from_mm(0.25),
        Length::from_mm(3.0),
        &doc,
    )
    .unwrap()
    .body;
    for what in [Imprint::Edges, Imprint::EdgesAndFaces] {
        let done = run(&a, &b, what).unwrap();
        assert_eq!(counts(&done.body), (10, 14, 8), "{what:?}");
        let disk = PI / 16.0;
        assert_areas(
            &done.body,
            &[disk, disk, 1.0 - disk, 1.0 - disk, 1.0, 1.0, 1.0, 1.0],
        );
        let (v, w) = (volume(&done.body), volume(&a));
        assert!((v - w).abs() <= 1e-9, "{v} {w}");
    }
}

#[test]
fn a_block_sitting_on_a_face_is_imprinted_only_with_faces() {
    // b = [0.25, 0.75]^2 x [1, 1.5] stands on a's top. Its bottom lies on the top, so its four
    // bottom edges lie in it: with faces they cut the top into the square under b (1/4) and
    // the frame around it (3/4): 7 faces, 12 + 4 edges, 8 + 4 vertices, one hole loop. With
    // edges only nothing is cut: no changes, the same faces.
    let doc = Document::default();
    let a = cuboid([0.0; 3], [1.0; 3], doc);
    let b = cuboid([0.25, 0.25, 1.0], [0.75, 0.75, 1.5], doc);
    let done = run(&a, &b, Imprint::EdgesAndFaces).unwrap();
    assert_eq!(counts(&done.body), (12, 16, 7));
    assert_areas(&done.body, &[0.25, 0.75, 1.0, 1.0, 1.0, 1.0, 1.0]);
    assert!((volume(&done.body) - 1.0).abs() < 1e-9);
    assert!(!done.changes.is_empty());

    let done = run(&a, &b, Imprint::Edges).unwrap();
    assert_eq!(counts(&done.body), (8, 12, 6));
    assert_areas(&done.body, &[1.0; 6]);
    assert!(done.changes.is_empty());
}

#[test]
fn a_block_apart_changes_nothing() {
    let doc = Document::default();
    let a = cuboid([0.0; 3], [1.0; 3], doc);
    let b = cuboid([3.0; 3], [4.0; 3], doc);
    let done = run(&a, &b, Imprint::EdgesAndFaces).unwrap();
    assert!(done.changes.is_empty());
    assert_eq!(counts(&done.body), (8, 12, 6));
    assert_areas(&done.body, &[1.0; 6]);
    assert!((volume(&done.body) - 1.0).abs() < 1e-9);
}

#[test]
fn imprint_is_deterministic_and_swapped_operands_cut_alike() {
    let doc = Document::default();
    let a = cuboid([0.0; 3], [1.0; 3], doc);
    let b = cuboid([0.5; 3], [1.5; 3], doc);
    let (x, y) = (
        run(&a, &b, Imprint::Edges).unwrap(),
        run(&a, &b, Imprint::Edges).unwrap(),
    );
    assert_eq!(x.changes, y.changes);
    assert_eq!(x.report.work, y.report.work);
    // The tool cut by the target: the same counts, by symmetry of the two cubes.
    let swapped = run(&b, &a, Imprint::Edges).unwrap();
    assert_eq!(counts(&swapped.body), counts(&x.body));
    assert_areas(&swapped.body, &areas(&x.body));
}

#[test]
fn an_operand_of_another_resolution_is_refused() {
    let doc = Document::default();
    let a = cuboid([0.0; 3], [1.0; 3], doc);
    let other = Document::with_resolution(Resolution::new(Length::from_mm(1e-4)).unwrap());
    let b = cuboid([0.5; 3], [1.5; 3], other);
    let err = run(&a, &b, Imprint::Edges).unwrap_err();
    assert_eq!(
        err,
        BooleanError::Refused(BooleanRefusal::OtherDocument {
            operand: Operand::from_raw(1)
        })
    );
}
