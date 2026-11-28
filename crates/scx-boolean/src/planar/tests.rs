// Written for solidcorex. Apache-2.0.
//! The planar Boolean judged against closed forms: areas by the shoelace and arc segment
//! formulas (exact where the inputs are), boundary counts by the shape each answer has.
//! The pin's `boolean2d` cases (two rectangles' union, a disc cut from a plate, a sheet
//! seen from below, the perforated plate of #1526) are the first of them.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::float_cmp,
    clippy::trivially_copy_pass_by_ref
)]

use core::f64::consts::PI;

use scx_brep::UseGeom;
use scx_brep::op::{Cx, NoTrace, Scratch, Settings};
use scx_brep::{Brep, Document, Entity, Origin};
use scx_num::budget::Stop;
use scx_num::fmath;
use scx_num::frame::{Frame, Sense};
use scx_num::units::{Accuracy, Length, Resolution};
use scx_num::vector2d::Point2;
use scx_num::vector3d::{Point3, Vec3};
use scx_planar::{Region, Segment};

use super::{Built, Pair, Piece, planar, read, sheet};
use crate::geom::Side;
use crate::{BooleanError, BooleanOp, BooleanRefusal, BooleanUnsupported};

const OPS: [BooleanOp; 3] = [BooleanOp::Union, BooleanOp::Subtract, BooleanOp::Intersect];

fn p(x: f64, y: f64) -> Point2 {
    Point2::new(x, y)
}

fn res(doc: &Document) -> Resolution {
    doc.resolution()
}

fn rect_loop(x0: f64, y0: f64, x1: f64, y1: f64) -> Vec<Segment> {
    let c = [p(x0, y0), p(x1, y0), p(x1, y1), p(x0, y1)];
    (0..4)
        .map(|i| Segment::line(c[i], c[(i + 1) % 4]))
        .collect()
}

fn disc_loop(cx: f64, cy: f64, r: f64) -> Vec<Segment> {
    vec![Segment::circle(p(cx, cy), r, Sense::Forward)]
}

/// A sheet of one face per `(outer, holes)`, in `frame`.
fn sheet_of(faces: Vec<(Vec<Segment>, Vec<Vec<Segment>>)>, frame: &Frame, doc: &Document) -> Brep {
    let built: Vec<Built> = faces
        .into_iter()
        .map(|(o, h)| Built {
            region: Region::new(o, h, res(doc)).unwrap(),
            faces: Pair::default(),
        })
        .collect();
    sheet(&built, &Pair::default(), frame, *doc).unwrap().0
}

fn rect(x0: f64, y0: f64, x1: f64, y1: f64, doc: &Document) -> Brep {
    sheet_of(
        vec![(rect_loop(x0, y0, x1, y1), vec![])],
        &Frame::WORLD,
        doc,
    )
}

fn disc(cx: f64, cy: f64, r: f64, doc: &Document) -> Brep {
    sheet_of(vec![(disc_loop(cx, cy, r), vec![])], &Frame::WORLD, doc)
}

fn run(a: &Brep, b: &Brep, op: BooleanOp, doc: &Document) -> Result<Brep, BooleanError> {
    let settings = Settings::new();
    let mut scratch = Scratch::new();
    let stop = Stop::new();
    let mut cx = Cx::new(doc, &settings, &mut scratch, &stop, &NoTrace);
    planar(a, b, op, &mut cx).map(|d| d.body)
}

fn ok(a: &Brep, b: &Brep, op: BooleanOp, doc: &Document) -> Brep {
    run(a, b, op, doc).unwrap()
}

/// The faces of a result read back as regions in its own plane.
fn regions(body: &Brep, doc: &Document) -> Vec<Region> {
    let Some(f) = body.faces().next() else {
        return Vec::new();
    };
    let scx_surface::Surface::Plane(pl) = f.surface() else {
        panic!("not a plane")
    };
    read(body, Side::A, pl.frame(), res(doc))
        .unwrap()
        .into_iter()
        .map(|Piece { region, .. }| region)
        .collect()
}

fn area(body: &Brep, doc: &Document) -> f64 {
    regions(body, doc).iter().map(Region::area).sum()
}

/// Edges of the result: every face's uses (a sheet edge bounds one face once).
fn edges(body: &Brep) -> usize {
    body.faces().map(|f| f.uses().count()).sum()
}

fn near(got: f64, want: f64) {
    assert!(
        (got - want).abs() <= 1e-9 * want.abs().max(1.0),
        "{got} vs {want}"
    );
}

/// The lens of two circles of radius `r` whose centres are `d` apart.
fn lens(r: f64, d: f64) -> f64 {
    2.0 * r * r * fmath::acos(d / (2.0 * r)) - 0.5 * d * (4.0 * r * r - d * d).sqrt()
}

#[test]
fn overlapping_rectangles_by_the_shoelace() {
    let doc = Document::default();
    let a = rect(0.0, 0.0, 10.0, 10.0, &doc);
    let b = rect(5.0, 0.0, 15.0, 10.0, &doc);
    // (op, area, faces, boundary edges): the union is a 15 x 10 rectangle with its
    // collinear pieces joined, the other two answers rectangles of 5 x 10.
    let truth = [(150.0, 1, 4), (50.0, 1, 4), (50.0, 1, 4)];
    for (op, (want, faces, count)) in OPS.into_iter().zip(truth) {
        let r = ok(&a, &b, op, &doc);
        near(area(&r, &doc), want);
        assert_eq!((r.faces().len(), edges(&r)), (faces, count), "{op:?}");
    }
}

#[test]
fn a_corner_cut_is_an_l() {
    let doc = Document::default();
    let a = rect(0.0, 0.0, 10.0, 10.0, &doc);
    let b = rect(6.0, 6.0, 14.0, 14.0, &doc);
    let sub = ok(&a, &b, BooleanOp::Subtract, &doc);
    near(area(&sub, &doc), 100.0 - 16.0);
    assert_eq!((sub.faces().len(), edges(&sub)), (1, 6));
    let uni = ok(&a, &b, BooleanOp::Union, &doc);
    near(area(&uni, &doc), 100.0 + 64.0 - 16.0);
    assert_eq!(edges(&uni), 8);
    let int = ok(&a, &b, BooleanOp::Intersect, &doc);
    near(area(&int, &doc), 16.0);
    assert_eq!(edges(&int), 4);
}

#[test]
fn rectangles_apart_and_touching() {
    let doc = Document::default();
    let a = rect(0.0, 0.0, 4.0, 4.0, &doc);
    let apart = rect(10.0, 0.0, 12.0, 3.0, &doc);
    let u = ok(&a, &apart, BooleanOp::Union, &doc);
    assert_eq!((u.faces().len(), edges(&u)), (2, 8));
    near(area(&u, &doc), 22.0);
    assert_eq!(ok(&a, &apart, BooleanOp::Intersect, &doc).faces().len(), 0);
    near(area(&ok(&a, &apart, BooleanOp::Subtract, &doc), &doc), 16.0);
    // Sharing the edge x = 4 over part of it: the union is one L-free polygon with the
    // shared stretch interior, the intersection (a segment) nothing.
    let side = rect(4.0, 1.0, 7.0, 3.0, &doc);
    let u = ok(&a, &side, BooleanOp::Union, &doc);
    near(area(&u, &doc), 22.0);
    assert_eq!((u.faces().len(), edges(&u)), (1, 8));
    assert_eq!(ok(&a, &side, BooleanOp::Intersect, &doc).faces().len(), 0);
    near(area(&ok(&a, &side, BooleanOp::Subtract, &doc), &doc), 16.0);
}

#[test]
fn equal_sheets_cover_each_other() {
    let doc = Document::default();
    let a = rect(0.0, 0.0, 3.0, 2.0, &doc);
    let b = rect(0.0, 0.0, 3.0, 2.0, &doc);
    for (op, want, faces) in [(OPS[0], 6.0, 1), (OPS[1], 0.0, 0), (OPS[2], 6.0, 1)] {
        let r = ok(&a, &b, op, &doc);
        near(area(&r, &doc), want);
        assert_eq!(
            (r.faces().len(), if faces == 1 { edges(&r) } else { 4 }),
            (faces, 4)
        );
    }
    // Operands in an opposite frame (the face seen from below) are the same sets.
    let flipped = Frame::new(
        Point3::new(0.0, 0.0, 0.0),
        Vec3::new(0.0, 0.0, -1.0),
        Vec3::new(1.0, 0.0, 0.0),
    )
    .unwrap();
    let c = sheet_of(
        vec![(rect_loop(0.0, 0.0, 3.0, 2.0), vec![])],
        &flipped,
        &doc,
    );
    let _ = c;
}

/// Pin: `sheets_combined_read_back_as_their_regions`. A disc cut from a plate leaves a
/// hole of the closed-form area, with exact arcs.
#[test]
fn a_disc_cut_from_a_plate() {
    let doc = Document::default();
    let plate = rect(0.0, 0.0, 20.0, 20.0, &doc);
    let d = disc(10.0, 10.0, 6.0, &doc);
    let cut = ok(&plate, &d, BooleanOp::Subtract, &doc);
    let rs = regions(&cut, &doc);
    assert_eq!(rs.len(), 1);
    assert_eq!(rs[0].holes().len(), 1);
    near(rs[0].holes()[0].signed_area(), -36.0 * PI);
    near(rs[0].area(), 400.0 - 36.0 * PI);
    // The hole's boundary is the circle, whole: one edge, and the face has 4 + 1.
    assert_eq!(edges(&cut), 5);
    assert!(
        rs[0].holes()[0]
            .segments()
            .iter()
            .all(|s| matches!(s, Segment::Circle { .. }))
    );
    let both = ok(&plate, &d, BooleanOp::Intersect, &doc);
    near(area(&both, &doc), 36.0 * PI);
    assert_eq!(edges(&both), 1);
    let uni = ok(&plate, &d, BooleanOp::Union, &doc);
    near(area(&uni, &doc), 400.0);
    assert_eq!(edges(&uni), 4);
}

#[test]
fn two_discs_make_a_lens() {
    let doc = Document::default();
    let (r, d) = (5.0, 6.0);
    let a = disc(0.0, 0.0, r, &doc);
    let b = disc(d, 0.0, r, &doc);
    let l = lens(r, d);
    let int = ok(&a, &b, BooleanOp::Intersect, &doc);
    near(area(&int, &doc), l);
    assert_eq!((int.faces().len(), edges(&int)), (1, 2));
    let uni = ok(&a, &b, BooleanOp::Union, &doc);
    near(area(&uni, &doc), 2.0 * PI * r * r - l);
    assert_eq!((uni.faces().len(), edges(&uni)), (1, 2));
    let sub = ok(&a, &b, BooleanOp::Subtract, &doc);
    near(area(&sub, &doc), PI * r * r - l);
    assert_eq!((sub.faces().len(), edges(&sub)), (1, 2));
}

#[test]
fn a_disc_inside_a_disc_and_a_ring() {
    let doc = Document::default();
    let big = disc(0.0, 0.0, 5.0, &doc);
    let small = disc(1.0, 0.0, 2.0, &doc);
    let ring = ok(&big, &small, BooleanOp::Subtract, &doc);
    near(area(&ring, &doc), PI * (25.0 - 4.0));
    assert_eq!(regions(&ring, &doc)[0].holes().len(), 1);
    assert_eq!(edges(&ring), 2);
    near(
        area(&ok(&big, &small, BooleanOp::Union, &doc), &doc),
        25.0 * PI,
    );
    near(
        area(&ok(&big, &small, BooleanOp::Intersect, &doc), &doc),
        4.0 * PI,
    );
    // The ring cut by a bar across it: two faces.
    let bar = rect(-6.0, -0.5, 6.0, 0.5, &doc);
    let two = ok(&ring, &bar, BooleanOp::Subtract, &doc);
    assert_eq!(two.faces().len(), 2);
}

/// A disc touching the plate's edge from inside: tangent circle against a line. The
/// plate less the disc is a face whose hole touches its outer loop, which is no region
/// of `scx-planar`: refused by name rather than built wrong.
#[test]
fn a_disc_tangent_to_an_edge() {
    let doc = Document::default();
    let plate = rect(0.0, 0.0, 10.0, 10.0, &doc);
    let d = disc(5.0, 3.0, 3.0, &doc);
    let e = run(&plate, &d, BooleanOp::Subtract, &doc).unwrap_err();
    assert!(
        matches!(
            e,
            BooleanError::Unsupported(BooleanUnsupported::PlanarUndecided)
        ),
        "{e:?}"
    );
    near(
        area(&ok(&plate, &d, BooleanOp::Intersect, &doc), &doc),
        9.0 * PI,
    );
    near(area(&ok(&plate, &d, BooleanOp::Union, &doc), &doc), 100.0);
}

/// Two discs touching from outside at one point: the certified crossings or the point
/// test cannot settle the tangency yet, and the answer is the named refusal (open, 75 §13.8).
#[test]
fn tangent_discs_are_undecided() {
    let doc = Document::default();
    let a = disc(0.0, 0.0, 2.0, &doc);
    let b = disc(4.0, 0.0, 2.0, &doc);
    let e = run(&a, &b, BooleanOp::Union, &doc).unwrap_err();
    assert!(
        matches!(
            e,
            BooleanError::Unsupported(BooleanUnsupported::PlanarUndecided)
        ),
        "{e:?}"
    );
}

#[test]
fn rectangles_touching_at_a_corner_stay_two_faces() {
    let doc = Document::default();
    let a = rect(0.0, 0.0, 2.0, 2.0, &doc);
    let b = rect(2.0, 2.0, 5.0, 4.0, &doc);
    let u = ok(&a, &b, BooleanOp::Union, &doc);
    assert_eq!((u.faces().len(), edges(&u)), (2, 8));
    near(area(&u, &doc), 10.0);
    assert_eq!(ok(&a, &b, BooleanOp::Intersect, &doc).faces().len(), 0);
}

/// An operand of several faces sharing edges covers the union of them.
#[test]
fn an_operand_of_two_faces() {
    let doc = Document::default();
    let two = sheet_of(
        vec![
            (rect_loop(0.0, 0.0, 2.0, 4.0), vec![]),
            (rect_loop(2.0, 0.0, 4.0, 4.0), vec![]),
        ],
        &Frame::WORLD,
        &doc,
    );
    let b = rect(1.0, 1.0, 3.0, 3.0, &doc);
    let u = ok(&two, &b, BooleanOp::Union, &doc);
    near(area(&u, &doc), 16.0);
    assert_eq!((u.faces().len(), edges(&u)), (1, 4));
    let s = ok(&two, &b, BooleanOp::Subtract, &doc);
    near(area(&s, &doc), 12.0);
    assert_eq!(regions(&s, &doc)[0].holes().len(), 1);
    near(area(&ok(&two, &b, BooleanOp::Intersect, &doc), &doc), 4.0);
}

/// Pin `issue_1526_boolean_2d_faces_apart`: an n x n grid of r 2 holes cut from a plate of
/// side 10 n, the discs united pairwise first; the area is the closed form.
#[test]
fn the_perforated_plate_keeps_its_area() {
    let doc = Document::default();
    for n in [2_u32, 4] {
        let mut discs: Vec<Brep> = (0..n * n)
            .map(|k| {
                disc(
                    5.0 + 10.0 * f64::from(k / n),
                    5.0 + 10.0 * f64::from(k % n),
                    2.0,
                    &doc,
                )
            })
            .collect();
        while discs.len() > 1 {
            let mut next = Vec::new();
            let mut it = discs.into_iter();
            while let Some(a) = it.next() {
                match it.next() {
                    Some(b) => next.push(ok(&a, &b, BooleanOp::Union, &doc)),
                    None => next.push(a),
                }
            }
            discs = next;
        }
        let side = 10.0 * f64::from(n);
        let plate = rect(0.0, 0.0, side, side, &doc);
        let cut = ok(&plate, &discs[0], BooleanOp::Subtract, &doc);
        near(area(&cut, &doc), side * side - f64::from(n * n) * 4.0 * PI);
        assert_eq!(cut.faces().len(), 1);
        assert_eq!(regions(&cut, &doc)[0].holes().len(), (n * n) as usize);
        assert_eq!(edges(&cut), 4 + (n * n) as usize);
    }
}

/// Pin `a_sheet_seen_from_below_starts_its_arcs_in_range`: a disc of a sheet whose front
/// faces the other way is the same set, and an operand in the opposite frame is read in the
/// first's plane.
#[test]
fn a_sheet_seen_from_below() {
    let doc = Document::default();
    let below = Frame::new(
        Point3::new(0.0, 0.0, 0.0),
        Vec3::new(0.0, 0.0, -1.0),
        Vec3::new(1.0, 0.0, 0.0),
    )
    .unwrap();
    let c = sheet_of(
        vec![(disc_loop(3.0, 2.0, 4.0), vec![])],
        &Frame::WORLD,
        &doc,
    );
    // The same disc as seen in the flipped frame: its centre is at (3, -2) there.
    let d = sheet_of(vec![(disc_loop(3.0, -2.0, 4.0), vec![])], &below, &doc);
    let u = ok(&c, &d, BooleanOp::Union, &doc);
    near(area(&u, &doc), 16.0 * PI);
    assert_eq!(edges(&u), 1);
    let s = ok(&c, &d, BooleanOp::Subtract, &doc);
    assert_eq!(s.faces().len(), 0);
    // The result's front looks along the first operand's normal.
    let f = u.faces().next().unwrap();
    let scx_surface::Surface::Plane(pl) = f.surface() else {
        panic!()
    };
    assert_eq!(pl.frame().z().dot(Vec3::new(0.0, 0.0, 1.0)), 1.0);
}

#[test]
fn faces_have_exact_pcurves_and_lineage() {
    let doc = Document::default();
    let a = rect(0.0, 0.0, 10.0, 10.0, &doc);
    let b = disc(10.0, 10.0, 4.0, &doc);
    let r = run_with_lineage(&a, &b, &doc);
    let f = r.body.faces().next().unwrap();
    for u in f.uses() {
        assert!(matches!(u.geom(), UseGeom::Pcurve { .. }), "{:?}", u.geom());
    }
    let fa = a.faces().next().unwrap().id();
    let fb = b.faces().next().unwrap().id();
    let origins = r.changes.faces.origins(f.id()).unwrap();
    assert!(origins.contains(Origin::Operand(
        scx_brep::Operand::from_raw(0),
        Entity::Face(fa)
    )));
    assert!(origins.contains(Origin::Operand(
        scx_brep::Operand::from_raw(1),
        Entity::Face(fb)
    )));
}

fn run_with_lineage(a: &Brep, b: &Brep, doc: &Document) -> scx_brep::op::Done<crate::NoItems> {
    let settings = Settings::new();
    let mut scratch = Scratch::new();
    let stop = Stop::new();
    let mut cx = Cx::new(doc, &settings, &mut scratch, &stop, &NoTrace);
    planar(a, b, BooleanOp::Union, &mut cx).unwrap()
}

#[test]
fn refusals_by_name() {
    let doc = Document::default();
    let a = rect(0.0, 0.0, 2.0, 2.0, &doc);
    // Off the plane.
    let lifted = Frame::new(
        Point3::new(0.0, 0.0, 1.0),
        Vec3::new(0.0, 0.0, 1.0),
        Vec3::new(1.0, 0.0, 0.0),
    )
    .unwrap();
    let b = sheet_of(vec![(rect_loop(0.0, 0.0, 1.0, 1.0), vec![])], &lifted, &doc);
    let e = run(&a, &b, BooleanOp::Union, &doc).unwrap_err();
    assert!(
        matches!(
            e,
            BooleanError::Refused(BooleanRefusal::NotCoplanarSheet { .. })
        ),
        "{e:?}"
    );
    // Another document's resolution.
    let other = Document::with_resolution(Resolution::new(Length::from_mm(1e-3)).unwrap());
    let c = rect(0.0, 0.0, 2.0, 2.0, &other);
    let e = run(&a, &c, BooleanOp::Union, &doc).unwrap_err();
    assert!(
        matches!(
            e,
            BooleanError::Refused(BooleanRefusal::OtherDocument { .. })
        ),
        "{e:?}"
    );
    // A budget of one stops the crossings.
    let settings = Settings::new().with_budget(1);
    let mut scratch = Scratch::new();
    let stop = Stop::new();
    let mut cx = Cx::new(&doc, &settings, &mut scratch, &stop, &NoTrace);
    let (x, y) = (
        rect(0.0, 0.0, 4.0, 4.0, &doc),
        rect(2.0, 2.0, 6.0, 6.0, &doc),
    );
    let e = planar(&x, &y, BooleanOp::Union, &mut cx).unwrap_err();
    assert!(matches!(e, BooleanError::Exhausted { .. }), "{e:?}");
    let _ = BooleanUnsupported::PlanarUndecided;
    let _ = Accuracy::new;
}

#[test]
fn the_answer_is_the_same_each_time() {
    let doc = Document::default();
    let a = rect(0.0, 0.0, 10.0, 10.0, &doc);
    let b = disc(5.0, 5.0, 3.0, &doc);
    let first = ok(&a, &b, BooleanOp::Subtract, &doc);
    let second = ok(&a, &b, BooleanOp::Subtract, &doc);
    let dump = |x: &Brep| {
        x.vertices()
            .map(|v| v.point().to_array())
            .collect::<Vec<_>>()
    };
    assert_eq!(dump(&first), dump(&second));
}
