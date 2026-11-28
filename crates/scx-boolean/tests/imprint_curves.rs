// Written for solidcorex. Apache-2.0.
//! `imprint_curves` on a block's top face and a cylinder's cap, judged against closed forms
//! (75 §13, BO13b): the regions' count and areas, the body's counts and volume, determinism,
//! and the typed refusals. The pin's `split_face_by_curves` cases (a closed curve cuts a
//! sheet in two, a line across it in two, a slit inside it is refused) are the first of them.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

use core::f64::consts::PI;

use scx_boolean::{
    BooleanError, BooleanRefusal, BooleanUnsupported, Cut, CutRefusal, CutUnsupported, Imprinted,
    imprint_curves,
};
use scx_brep::op::{Cx, NoTrace, Scratch, Settings};
use scx_brep::{Brep, Document, FaceId};
use scx_curve::{Circle, Curve, Ellipse, Line};
use scx_mass::RelAccuracy;
use scx_num::budget::{Budget, Stop};
use scx_num::extent1d::Extent1d;
use scx_num::fmath;
use scx_num::frame::{Frame, Sense};
use scx_num::units::Length;
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

/// The unit cube and its top face (z = 1).
fn cube() -> (Brep, FaceId) {
    let doc = Document::default();
    let made = block(&frame([0.0; 3]), [1.0; 3].map(Length::from_mm), &doc).unwrap();
    (made.body, made.faces.top)
}

/// The cylinder of radius 1, height 1, and its top cap.
fn can() -> (Brep, FaceId) {
    let doc = Document::default();
    let made = cylinder(
        &frame([0.0; 3]),
        Length::from_mm(1.0),
        Length::from_mm(1.0),
        &doc,
    )
    .unwrap();
    (made.body, made.faces.top)
}

fn line(a: [f64; 3], b: [f64; 3]) -> Cut {
    Cut::whole(Curve::Line(
        Line::through(Point3::new(a[0], a[1], a[2]), Point3::new(b[0], b[1], b[2])).unwrap(),
    ))
}

fn circle(c: [f64; 3], r: f64) -> Cut {
    Cut::whole(Curve::Circle(
        Circle::new(frame(c), Length::from_mm(r), Sense::Forward).unwrap(),
    ))
}

fn run(body: &Brep, face: FaceId, cuts: &[Cut]) -> Result<Imprinted, BooleanError> {
    let doc = Document::default();
    let settings = Settings::new();
    let mut scratch = Scratch::new();
    let stop = Stop::new();
    let mut cx = Cx::new(&doc, &settings, &mut scratch, &stop, &NoTrace);
    imprint_curves(body, face, cuts, &mut cx)
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

fn area(body: &Brep, face: FaceId) -> f64 {
    scx_mass::face_area(
        body.face(face).unwrap(),
        RelAccuracy::new(1e-10).unwrap(),
        &mut Scratch::new(),
        &mut Budget::unlimited(),
        &Stop::new(),
    )
    .unwrap()
    .value
}

/// The regions' areas, ascending.
fn areas(r: &Imprinted) -> Vec<f64> {
    let mut v: Vec<f64> = r
        .regions
        .iter()
        .map(|g| area(&r.done.body, g.face))
        .collect();
    v.sort_by(f64::total_cmp);
    v
}

fn assert_areas(r: &Imprinted, want: &[f64]) {
    let got = areas(r);
    assert_eq!(got.len(), want.len(), "{got:?} against {want:?}");
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

/// The regions' points lie in the plane and apart, and the smallest region's is within
/// `near` of `at` (the point of a disc cut from a face).
fn assert_inside(r: &Imprinted, z: f64, at: Option<([f64; 2], f64)>) {
    for (i, g) in r.regions.iter().enumerate() {
        assert!((g.inside.z - z).abs() < 1e-9, "{g:?}");
        for h in &r.regions[i + 1..] {
            assert!(g.inside.distance(h.inside) > 1e-6, "{g:?} {h:?}");
        }
    }
    if let Some((c, near)) = at {
        let small = r
            .regions
            .iter()
            .min_by(|p, q| area(&r.done.body, p.face).total_cmp(&area(&r.done.body, q.face)))
            .unwrap();
        let d = fmath::hypot(small.inside.x - c[0], small.inside.y - c[1]);
        assert!(d < near, "{small:?}");
    }
}

#[test]
fn a_circle_inside_a_face_cuts_a_disc_from_it() {
    let (body, top) = cube();
    let r = run(&body, top, &[circle([0.5, 0.5, 1.0], 0.25)]).unwrap();
    assert_areas(&r, &[PI / 16.0, 1.0 - PI / 16.0]);
    assert_inside(&r, 1.0, Some(([0.5, 0.5], 0.25)));
    // 6 faces + 1; 12 edges + the circle; 8 vertices + the circle's start.
    assert_eq!(counts(&r.done.body), (9, 13, 7));
    assert!((volume(&r.done.body) - 1.0).abs() < 1e-9);
    assert!(!r.done.changes.is_empty());
}

#[test]
fn a_line_across_a_face_cuts_it_in_two_and_the_edges_it_meets_in_two() {
    let (body, top) = cube();
    let r = run(&body, top, &[line([0.5, -3.0, 1.0], [0.5, 4.0, 1.0])]).unwrap();
    assert_areas(&r, &[0.5, 0.5]);
    assert_inside(&r, 1.0, None);
    // The line's piece is one edge; the two edges it meets are cut, and so are the side
    // faces' uses of them: 8 + 2 vertices, 12 + 2 + 1 edges, 6 + 1 faces.
    assert_eq!(counts(&r.done.body), (10, 15, 7));
    assert!((volume(&r.done.body) - 1.0).abs() < 1e-9);
}

#[test]
fn two_lines_crossing_in_a_face_cut_it_in_four() {
    let (body, top) = cube();
    let r = run(
        &body,
        top,
        &[
            line([0.5, -1.0, 1.0], [0.5, 2.0, 1.0]),
            line([-1.0, 0.5, 1.0], [2.0, 0.5, 1.0]),
        ],
    )
    .unwrap();
    assert_areas(&r, &[0.25; 4]);
    assert_inside(&r, 1.0, None);
    // 8 + 4 edge cuts + the crossing; 12 + 4 + 4 pieces; 5 + 4 faces: Euler 13 - 20 + 9 = 2.
    assert_eq!(counts(&r.done.body), (13, 20, 9));
}

#[test]
fn a_circle_crossing_the_boundary_cuts_a_disc_and_four_corners() {
    let (body, top) = cube();
    let (rad, d) = (0.6_f64, 0.5_f64);
    let r = run(&body, top, &[circle([0.5, 0.5, 1.0], rad)]).unwrap();
    let segment = rad * rad * fmath::acos(d / rad) - d * (rad * rad - d * d).sqrt();
    let disc = PI * rad * rad - 4.0 * segment;
    let corner = (1.0 - disc) / 4.0;
    assert_areas(&r, &[corner, corner, corner, corner, disc]);
    assert_inside(&r, 1.0, None);
    // 8 + 8 vertices where the circle crosses the four edges twice each; 12 + 8 cuts + 4
    // arcs edges; 6 - 1 + 5 faces: Euler 16 - 24 + 10 = 2.
    assert_eq!(counts(&r.done.body), (16, 24, 10));
    assert!((volume(&r.done.body) - 1.0).abs() < 1e-9);
}

#[test]
fn nested_circles_cut_a_disc_a_ring_and_the_rest() {
    let (body, top) = cube();
    let r = run(
        &body,
        top,
        &[circle([0.5, 0.5, 1.0], 0.1), circle([0.5, 0.5, 1.0], 0.3)],
    )
    .unwrap();
    assert_areas(&r, &[PI * 0.01, PI * (0.09 - 0.01), 1.0 - PI * 0.09]);
    assert_inside(&r, 1.0, None);
}

#[test]
fn a_chord_cuts_a_disc_cap_into_a_segment_and_the_rest() {
    let (body, top) = can();
    let r = run(&body, top, &[line([0.5, -3.0, 1.0], [0.5, 3.0, 1.0])]).unwrap();
    let segment = fmath::acos(0.5) - 0.5 * 0.75_f64.sqrt();
    assert_areas(&r, &[segment, PI - segment]);
    assert_inside(&r, 1.0, None);
    assert!((volume(&r.done.body) - PI).abs() < 1e-9);
}

#[test]
fn a_bounded_curve_ending_on_the_boundary_cuts_like_the_whole_line() {
    let (body, top) = cube();
    let whole = run(&body, top, &[line([0.5, -3.0, 1.0], [0.5, 4.0, 1.0])]).unwrap();
    let l = Line::through(Point3::new(0.5, 0.0, 1.0), Point3::new(0.5, 1.0, 1.0)).unwrap();
    let cut = Cut::over(Curve::Line(l), Extent1d::new(0.0, 1.0).unwrap());
    let part = run(&body, top, &[cut]).unwrap();
    assert_areas(&part, &[0.5, 0.5]);
    assert_eq!(counts(&part.done.body), counts(&whole.done.body));
}

#[test]
fn the_same_curves_give_the_same_answer() {
    let (body, top) = cube();
    let cuts = [
        circle([0.5, 0.5, 1.0], 0.6),
        line([0.5, -1.0, 1.0], [0.5, 2.0, 1.0]),
    ];
    let one = run(&body, top, &cuts).unwrap();
    let two = run(&body, top, &cuts).unwrap();
    assert_eq!(one.regions, two.regions);
    assert_eq!(one.done.report.work, two.done.report.work);
    assert_eq!(counts(&one.done.body), counts(&two.done.body));
    assert_eq!(areas(&one), areas(&two));
}

#[test]
fn refusals_are_typed() {
    let (body, top) = cube();
    let cut = |c: &[Cut]| run(&body, top, c).err().unwrap();
    assert_eq!(
        cut(&[]),
        BooleanError::Refused(BooleanRefusal::Cut(CutRefusal::NoCurves))
    );
    // Off the face's plane.
    assert_eq!(
        cut(&[line([0.5, -1.0, 0.5], [0.5, 2.0, 0.5])]),
        BooleanError::Refused(BooleanRefusal::Cut(CutRefusal::OffFace(0)))
    );
    // On the plane, far from the face.
    assert_eq!(
        cut(&[line([5.0, -1.0, 1.0], [5.0, 2.0, 1.0])]),
        BooleanError::Refused(BooleanRefusal::Cut(CutRefusal::Misses(0)))
    );
    // A slit: an end inside the face that meets nothing.
    let l = Line::through(Point3::new(0.2, 0.5, 1.0), Point3::new(0.8, 0.5, 1.0)).unwrap();
    assert_eq!(
        cut(&[Cut::over(Curve::Line(l), Extent1d::new(0.0, 1.0).unwrap())]),
        BooleanError::Refused(BooleanRefusal::Cut(CutRefusal::Dangles(0)))
    );
    // The same line twice.
    assert_eq!(
        cut(&[
            line([0.5, -1.0, 1.0], [0.5, 2.0, 1.0]),
            line([0.5, -2.0, 1.0], [0.5, 3.0, 1.0]),
        ]),
        BooleanError::Refused(BooleanRefusal::Cut(CutRefusal::Overlap([0, 1])))
    );
    // A line along the boundary cuts nothing.
    assert_eq!(
        cut(&[line([0.0, -1.0, 1.0], [0.0, 2.0, 1.0])]),
        BooleanError::Refused(BooleanRefusal::Cut(CutRefusal::Misses(0)))
    );
    // A face of another body.
    assert_eq!(
        run(
            &body,
            FaceId::from_raw(999),
            &[circle([0.5, 0.5, 1.0], 0.2)]
        )
        .err()
        .unwrap(),
        BooleanError::Refused(BooleanRefusal::Cut(CutRefusal::NoFace(FaceId::from_raw(
            999
        ))))
    );
}

#[test]
fn what_has_no_exact_pcurve_here_is_unsupported() {
    let (body, top) = cube();
    let ellipse = Cut::whole(Curve::Ellipse(
        Ellipse::new(
            frame([0.5, 0.5, 1.0]),
            Length::from_mm(0.3),
            Length::from_mm(0.2),
            Sense::Forward,
        )
        .unwrap(),
    ));
    assert_eq!(
        run(&body, top, &[ellipse]).err().unwrap(),
        BooleanError::Unsupported(BooleanUnsupported::Cut(CutUnsupported::Curve(0)))
    );
    let (can, _) = can();
    let wall = can
        .faces()
        .find(|f| matches!(f.surface(), scx_surface::Surface::Cylinder(_)))
        .unwrap()
        .id();
    assert_eq!(
        run(&can, wall, &[circle([0.0, 0.0, 0.5], 1.0)])
            .err()
            .unwrap(),
        BooleanError::Unsupported(BooleanUnsupported::Cut(CutUnsupported::Surface(wall)))
    );
}
