// Written for solidcorex. Apache-2.0.
//! Gap closure (research 72 §14.17, MA2): a body whose trims carry a recorded accuracy
//! ([`scx_testkit::bodies::loose`]) is measured with each such use closed onto its edge's
//! curve, so it holds its closed form where the same body unclosed leaks to first order in
//! the gap; a use with none costs nothing.

#![allow(
    missing_docs,
    clippy::pedantic,
    clippy::unwrap_used,
    clippy::indexing_slicing,
    clippy::panic
)]

use scx_brep::op::Scratch;
use scx_brep::{Brep, Document, Draft, FaceId};
use scx_mass::{Mass, MassSpec, RelAccuracy, mass, volume};
use scx_num::budget::{Budget, Stop};
use scx_num::units::{Length, Resolution};
use scx_num::vector3d::Point3;
use scx_testkit::bodies::{Invariant, Kind, broken, loose};
use scx_testkit::solid;

/// `draft` finished.
fn finish(d: Draft) -> Brep {
    d.finish(&Document::default())
        .map_err(|u| u.cause)
        .unwrap()
        .0
}

/// `draft` finished under a resolution that covers its gaps, with every recorded accuracy
/// forgotten: the same pcurves, read as exact (what a measure with no closure would see).
fn open(d: Draft) -> Brep {
    let res = Resolution::new(Length::from_mm(0.1)).unwrap();
    unclosed(d)
        .finish(&Document::with_resolution(res))
        .map_err(|u| u.cause)
        .unwrap()
        .0
}

fn unclosed(mut d: Draft) -> Draft {
    let faces: Vec<FaceId> = d.faces().map(scx_brep::Face::id).collect();
    for f in faces {
        for lp in &mut d.face_mut(f).unwrap().loops {
            for u in &mut lp.uses {
                u.bound = None;
            }
        }
    }
    d
}

fn measure(b: &Brep, acc: f64, about: Point3) -> Mass {
    let spec = MassSpec::new(RelAccuracy::new(acc).unwrap()).with_about(about);
    mass(
        b,
        &spec,
        &mut Scratch::new(),
        &mut Budget::unlimited(),
        &Stop::new(),
    )
    .unwrap()
}

fn vol(b: &Brep, acc: f64) -> (f64, u64) {
    let mut budget = Budget::unlimited();
    let v = volume(
        b,
        RelAccuracy::new(acc).unwrap(),
        &mut Scratch::new(),
        &mut budget,
        &Stop::new(),
    )
    .unwrap();
    (v.value, budget.used())
}

const ORIGIN: Point3 = Point3::new(0.0, 0.0, 0.0);

/// A block of unit sides with every pcurve 0.02 off its edge: closed it is the unit block
/// to rounding (the strips and fans lie in each face's plane, so close the faces to the
/// edges exactly), unclosed it is 1.5 percent out, and its first moments differ with
/// the reference.
#[test]
fn a_loose_block_closes_to_its_closed_form() {
    let want = solid::block(1.0, 1.0, 1.0).unwrap();
    for offset in [1e-2, 2e-2, 1e-4] {
        let d = loose(Kind::Block, 0, 1, offset).unwrap();
        let open = open(d.clone());
        let closed = finish(d);
        let m = measure(&closed, 1e-9, ORIGIN);
        let miss = (m.volume - want.moments.volume).abs();
        assert!(
            miss <= 1e-12,
            "offset {offset}: volume {} miss {miss:e}",
            m.volume
        );
        for i in 0..3 {
            let f = [m.first.x, m.first.y, m.first.z][i];
            assert!((f - want.moments.first[i]).abs() <= 1e-12, "first {i}: {f}");
        }
        let s = m.second;
        let got = [[s.xx, s.xy, s.zx], [s.xy, s.yy, s.yz], [s.zx, s.yz, s.zz]];
        for i in 0..3 {
            for j in 0..3 {
                let miss = (got[i][j] - want.moments.second[i][j]).abs();
                assert!(miss <= 1e-12, "second {i}{j}: miss {miss:e}");
            }
        }
        // The same body unclosed leaks to first order in the gap.
        let leak = (measure(&open, 1e-9, ORIGIN).volume - 1.0).abs();
        assert!(leak >= 0.5 * offset, "offset {offset}: leak {leak:e}");
        // The origin does not matter once closed.
        let far = measure(&closed, 1e-9, Point3::new(5.0, -7.0, 3.0));
        assert!((far.volume - 1.0).abs() <= 1e-12);
    }
}

/// The pin's `a_cubes_strips_close_in_one_piece_per_edge_use`: on a cube each strip is
/// polynomial of degree at most 4 along its use, which the 7-point Kronrod rule integrates
/// in one piece, so the closure adds 7 evaluations per use and nothing else.
#[test]
fn a_cubes_strips_close_in_one_piece_per_edge_use() {
    let d = loose(Kind::Block, 0, 1, 1e-2).unwrap();
    let closed = finish(d.clone());
    let open = open(d);
    let (_, with) = vol(&closed, 1e-9);
    let (_, without) = vol(&open, 1e-9);
    // 6 faces x 4 uses, 7 evaluations each.
    assert_eq!(with - without, 24 * 7);
}

/// A use with no recorded accuracy costs nothing: a valid body's work is the same with the
/// closure in (the pin's gap closure had no such case: it closed every face).
#[test]
fn an_exact_body_pays_nothing() {
    let d = scx_testkit::bodies::generate(Kind::Block, 0, 1)
        .unwrap()
        .draft;
    let b = finish(d);
    // 6 faces x 4 uses' Green integrals only; the figure is MA1's.
    assert_eq!(vol(&b, 1e-9).1, 798);
}

/// A cylinder and a cone (with its apex pole) whose lines and circles are off their edges
/// (the walls' lines by an arc, the caps' circles by their radius): the closed volume is
/// within its stated error of the closed form and far nearer than the unclosed; the error
/// includes the tube the closure cannot count. (A cone's trims are held to the resolution
/// by `finish` at its poles, so its offset is smaller.)
#[test]
fn a_loose_curved_body_holds_within_its_stated_error() {
    for (kind, want, offset) in [
        (Kind::Cylinder, solid::cylinder(1.0, 2.0).unwrap(), 1e-3),
        (Kind::Cone, solid::cone(1.0, 0.0, 1.0).unwrap(), 1e-5),
    ] {
        let d = loose(kind, 0, 2, offset).unwrap();
        let open = open(d.clone());
        let closed = finish(d);
        let m = measure(&closed, 1e-9, ORIGIN);
        let miss = (m.volume - want.moments.volume).abs();
        let leak = (measure(&open, 1e-9, ORIGIN).volume - want.moments.volume).abs();
        let name = kind.name();
        assert!(
            miss <= m.error.volume,
            "{name}: miss {miss:e} not covered by {:e}",
            m.error.volume
        );
        assert!(miss <= 1e-12, "{name}: miss {miss:e}");
        assert!(leak >= 0.1 * offset, "{name}: unclosed leak {leak:e}");
        // The closure's tube is in the stated error: more than the exact body's.
        let exact = measure(&finish(loose(kind, 0, 2, 0.0).unwrap()), 1e-9, ORIGIN);
        assert!(m.error.volume > 10.0 * exact.error.volume, "{name}");
    }
}

/// A8 of research 72 §10: a pcurve recorded with no accuracy that is off its edge is not a
/// gap to close but a defect, and `finish` refuses it before a measure sees it.
#[test]
fn an_exact_pcurve_off_its_edge_is_refused_by_finish() {
    let d = broken(Invariant::PcurveOffEdge, 0, 1).unwrap().unwrap();
    assert!(d.finish(&Document::default()).is_err());
}
