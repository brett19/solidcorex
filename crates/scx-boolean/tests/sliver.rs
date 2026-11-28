// Written for solidcorex. Apache-2.0.
//! Two pie slices whose arc centre misses the apex, the second turned so that the facing
//! walls overlap by a thin wedge (the pin's `curved_sliver_coincidence`, 75 §7 row BO22). The
//! two arc cylinders' axes lie 8.4e-5 apart, yet over the sliver between the two corners they
//! stay within 1e-10 of each other: they meet only there, along each other. Each result is
//! one valid solid whose volume is the closed form within 1e-6 (the wedges are 0.14 and 0.43).
//!
//! Both are ignored for now. This is a near-coincidence over a band, not a tangent point or
//! line, so neither the stated-fact nor the closed-form tier of BO22 answers it: the first
//! case's intersection gives `Unsupported(Pcurve)`, the second's `Defect(Piece)`.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
// The operands' geometry and the closed forms the test checks against, not code under test.
#![allow(clippy::disallowed_methods)]

use core::f64::consts::PI;

use scx_boolean::{BooleanOp, boolean};
use scx_brep::op::{Cx, NoTrace, Scratch, Settings};
use scx_brep::{Brep, Document};
use scx_check::Contacts;
use scx_num::budget::{Budget, Stop};
use scx_num::frame::{Frame, Sense};
use scx_num::units::Length;
use scx_num::vector2d::Point2;
use scx_testkit::bodies::made::{Region, Segment, Span, extrude};

const RHO: f64 = 85.5;
const H: f64 = 119.0;

/// The corners of a 120° pie slice symmetric about +y, `RHO` from the apex.
fn corners() -> ([f64; 2], [f64; 2]) {
    let p0 = [RHO * (PI / 6.0).cos(), RHO * (PI / 6.0).sin()];
    (p0, [-p0[0], p0[1]])
}

/// A pie slice of height `H` whose arc is the circle about (0, `c`) through its corners, turned
/// by `turn` about z.
fn slice(c: f64, turn: f64) -> Brep {
    let (p0, p1) = corners();
    let (ct, st) = (turn.cos(), turn.sin());
    let at = |p: [f64; 2]| Point2::new(ct * p[0] - st * p[1], st * p[0] + ct * p[1]);
    let o = Point2::new(0.0, 0.0);
    let outer = vec![
        Segment::line(o, at(p0)),
        Segment::arc(at([0.0, c]), at(p0), at(p1), Sense::Forward),
        Segment::line(at(p1), o),
    ];
    let doc = Document::default();
    let region = Region::new(outer, vec![], doc.resolution()).unwrap();
    let span = Span::new(Length::from_mm(0.0), Length::from_mm(H));
    extrude(&region, &Frame::WORLD, span, &doc).unwrap().body
}

/// The slice's volume: the triangle apex-corner-corner plus the circular segment between the
/// chord and the arc, times `H`.
fn slice_volume(c: f64) -> f64 {
    let (p0, p1) = corners();
    let r = p0[0].hypot(p0[1] - c);
    let theta = 2.0 * (p0[0] / r).asin();
    let triangle = 0.5 * (p0[0] * p1[1] - p0[1] * p1[0]);
    (triangle + r * r / 2.0 * (theta - theta.sin())) * H
}

/// The overlap wedge of walls turned `overlap` into each other: the triangle apex-corner-corner
/// times `H` (the arc's bulge between the corners adds 6e-16 per unit height).
fn wedge_volume(overlap: f64) -> f64 {
    0.5 * RHO * RHO * overlap.sin() * H
}

/// Slice `c` against the second slice turned 120° − `overlap`: one valid solid whose volume is
/// `want` within 1e-6.
fn slices(c: f64, overlap: f64, op: BooleanOp, want: f64) {
    let a = slice(c, 0.0);
    let b = slice(c, 2.0 * PI / 3.0 - overlap);
    let doc = Document::default();
    let (settings, mut scratch, stop) = (Settings::new(), Scratch::new(), Stop::new());
    let mut cx = Cx::new(&doc, &settings, &mut scratch, &stop, &NoTrace);
    let got =
        boolean(&a, &b, op, &mut cx).unwrap_or_else(|e| panic!("{c} {overlap} {op:?}: {e:?}"));
    let body = &got.body;
    let cx = Cx::new(&doc, &settings, &mut scratch, &stop, &NoTrace);
    scx_check::contract(body, &cx, Contacts::Radial)
        .unwrap_or_else(|e| panic!("{c} {overlap} {op:?}: {e:?}"));
    assert_eq!(
        body.regions().count(),
        2,
        "{c} {overlap} {op:?}: one solid and the outside"
    );
    let v = scx_mass::volume(
        body,
        scx_mass::RelAccuracy::new(1e-12).unwrap(),
        &mut Scratch::new(),
        &mut Budget::unlimited(),
        &Stop::new(),
    )
    .unwrap()
    .value;
    assert!(
        (v - want).abs() <= 1e-6,
        "{c} {overlap} {op:?}: {v}, closed form {want}"
    );
}

/// Turned 120° − 1e-6: the first wall lies 8.55e-5 inside the first slice at the corners,
/// and the two arcs cross halfway between the corners at about 1e-6 rad.
#[test]
#[ignore = "near-coincident arc walls over a band: Unsupported(Pcurve) or Defect(Piece) (75 §13.39, open)"]
fn pie_slices_with_off_centre_arcs_and_overlapping_walls_meet_in_the_wedge() {
    let overlap = 1e-6;
    let wedge = wedge_volume(overlap);
    for c in [-4.85e-5, 4.85e-5] {
        slices(c, overlap, BooleanOp::Intersect, wedge);
        slices(c, overlap, BooleanOp::Subtract, slice_volume(c) - wedge);
    }
}

/// Turned 120° − 3.3e-7: the corners lie 2.82e-5 apart, each on the other slice's arc within
/// 1.4e-11, and the arcs never cross between them.
#[test]
#[ignore = "near-coincident arc walls over a band: Defect(Piece) (75 §13.39, open)"]
fn pie_slices_whose_arcs_run_along_each_other_without_crossing_meet_in_the_wedge() {
    let overlap = 3.3e-7;
    let wedge = wedge_volume(overlap);
    for c in [-4.85e-5, 4.85e-5] {
        slices(c, overlap, BooleanOp::Intersect, wedge);
        slices(c, overlap, BooleanOp::Subtract, slice_volume(c) - wedge);
        slices(c, overlap, BooleanOp::Union, 2.0 * slice_volume(c) - wedge);
    }
}
