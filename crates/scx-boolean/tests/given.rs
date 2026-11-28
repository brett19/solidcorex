// Written for solidcorex. Apache-2.0.
//! [`boolean_given`] (BO21): a true fact gives [`boolean`]'s body and changes to the bit in
//! less work; a false one is `Refused(FalseFact)`, never a different result (75 §8.2).
//!
//! The bodies are compared by their record encoding (`scx_record::encode`), so equality is
//! of every table and every coordinate's bits. Each case's result is also judged against
//! its closed-form volume, so the shared answer is the true one, not only the same one.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

use scx_boolean::NoItems;
use scx_boolean::{
    BooleanError, BooleanOp, BooleanRefusal, Fact, Incidences, boolean, boolean_given,
};
use scx_brep::op::{Cx, Done, NoTrace, Scratch, Settings};
use scx_brep::{Brep, Document, EdgeId, FaceId, Operand};
use scx_num::budget::{Budget, Stop};
use scx_num::frame::Frame;
use scx_num::units::Length;
use scx_num::vector3d::{Point3, Vec3};
use scx_testkit::bodies::made::{self, BlockFaces, CylinderFaces};

const A: Operand = Operand::from_raw(0);
const B: Operand = Operand::from_raw(1);

fn mm(v: f64) -> Length {
    Length::from_mm(v)
}

fn at(x: f64, y: f64, z: f64) -> Frame {
    Frame::new(
        Point3::new(x, y, z),
        Vec3::new(0.0, 0.0, 1.0),
        Vec3::new(1.0, 0.0, 0.0),
    )
    .unwrap()
}

fn block(o: [f64; 3], size: [f64; 3]) -> (Brep, BlockFaces) {
    let m = made::block(&at(o[0], o[1], o[2]), size.map(mm), &Document::default()).unwrap();
    (m.body, m.faces)
}

fn cylinder(o: [f64; 3], r: f64, h: f64) -> (Brep, CylinderFaces) {
    let m = made::cylinder(&at(o[0], o[1], o[2]), mm(r), mm(h), &Document::default()).unwrap();
    (m.body, m.faces)
}

/// The edges of `face`, each once.
fn edges(body: &Brep, face: FaceId) -> Vec<EdgeId> {
    let mut e: Vec<EdgeId> = body
        .face(face)
        .unwrap()
        .uses()
        .map(|u| u.edge().id())
        .collect();
    e.sort_unstable();
    e.dedup();
    e
}

fn on(operand: Operand, edge: EdgeId, face: FaceId) -> Fact {
    Fact::EdgeOnFace {
        operand,
        edge,
        face,
    }
}

fn plain(a: &Brep, b: &Brep, op: BooleanOp) -> Result<Done<NoItems>, BooleanError> {
    let doc = Document::default();
    let (settings, mut scratch, stop) = (Settings::new(), Scratch::new(), Stop::new());
    let mut cx = Cx::new(&doc, &settings, &mut scratch, &stop, &NoTrace);
    boolean(a, b, op, &mut cx)
}

fn given(
    a: &Brep,
    b: &Brep,
    op: BooleanOp,
    facts: &Incidences,
) -> Result<Done<NoItems>, BooleanError> {
    let doc = Document::default();
    let (settings, mut scratch, stop) = (Settings::new(), Scratch::new(), Stop::new());
    let mut cx = Cx::new(&doc, &settings, &mut scratch, &stop, &NoTrace);
    boolean_given(a, b, op, facts, &mut cx)
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

/// `facts` are true for `a op b`: the same body (to the bit) and changes as without them,
/// in no more work, with the closed-form `truth` volume. Returns the work without and with.
fn same(a: &Brep, b: &Brep, op: BooleanOp, facts: &Incidences, truth: f64) -> (u64, u64) {
    let want = plain(a, b, op).unwrap();
    let got = given(a, b, op, facts).unwrap();
    assert_eq!(
        scx_record::encode(&got.body).unwrap(),
        scx_record::encode(&want.body).unwrap(),
        "the body differs"
    );
    assert_eq!(got.changes, want.changes, "the changes differ");
    let v = volume(&got.body);
    assert!(
        (v - truth).abs() <= 1e-9 * truth,
        "volume {v}, truth {truth}"
    );
    let (w0, w1) = (want.report.work, got.report.work);
    assert!(w1 <= w0, "work {w1} with the facts, {w0} without");
    (w0, w1)
}

/// Without and with the facts: the same body (to the bit) and changes, or the same error.
fn alike(want: Result<Done<NoItems>, BooleanError>, got: Result<Done<NoItems>, BooleanError>) {
    match (want, got) {
        (Ok(w), Ok(g)) => {
            assert_eq!(
                scx_record::encode(&g.body).unwrap(),
                scx_record::encode(&w.body).unwrap(),
                "the body differs"
            );
            assert_eq!(g.changes, w.changes, "the changes differ");
        }
        (Err(w), Err(g)) => assert_eq!(w, g),
        (w, g) => panic!("without the facts {w:?}, with them {g:?}"),
    }
}

fn refused(r: Result<Done<NoItems>, BooleanError>) -> Fact {
    match r {
        Err(BooleanError::Refused(BooleanRefusal::FalseFact { fact })) => fact,
        Err(e) => panic!("refused otherwise: {e}"),
        Ok(_) => panic!("a false fact was taken"),
    }
}

/// A 10 mm cube and a 10 x 6 x 4 block on its top, flush on three sides: the top and
/// bottom faces coincide, and four of the upper block's bottom edges lie on the cube's top
/// (one inside it, three on its rim) while all four of the cube's top edges lie on the
/// upper block's bottom plane.
fn stacked() -> (Brep, BlockFaces, Brep, BlockFaces) {
    let (a, fa) = block([0.0; 3], [10.0; 3]);
    let (b, fb) = block([0.0, 0.0, 10.0], [10.0, 6.0, 4.0]);
    (a, fa, b, fb)
}

#[test]
fn a_stated_face_pair_gives_the_same_union_in_less_work() {
    let (a, fa, b, fb) = stacked();
    let facts: Incidences = [Fact::FaceOnFace {
        a: fa.top,
        b: fb.bottom,
    }]
    .into_iter()
    .collect();
    let (w0, w1) = same(&a, &b, BooleanOp::Union, &facts, 1000.0 + 240.0);
    assert!(w1 < w0, "work {w1} with the face pair, {w0} without");
}

#[test]
fn stated_edges_give_the_same_union_subtraction_and_intersection() {
    let (a, fa, b, fb) = stacked();
    let facts: Incidences = edges(&b, fb.bottom)
        .into_iter()
        .map(|e| on(B, e, fa.top))
        .chain(edges(&a, fa.top).into_iter().map(|e| on(A, e, fb.bottom)))
        .collect();
    for (op, truth) in [
        (BooleanOp::Union, 1240.0),
        (BooleanOp::Subtract, 1000.0),
        (BooleanOp::Intersect, 0.0),
    ] {
        match op {
            // Faces that only touch meet in no volume: both refuse it alike or both
            // build the same nothing.
            BooleanOp::Intersect => {
                alike(plain(&a, &b, op), given(&a, &b, op, &facts));
            }
            _ => {
                let (w0, w1) = same(&a, &b, op, &facts, truth);
                assert!(w1 < w0, "{op:?}: work {w1} with the edges, {w0} without");
            }
        }
    }
}

#[test]
fn a_boss_on_a_plate_gives_the_same_union() {
    // A rim circle on a plane: the bound proves it, and the cylinder's bottom disc lies in
    // the plate's top.
    let (a, fa) = block([0.0; 3], [20.0, 20.0, 5.0]);
    let (b, fb) = cylinder([10.0, 10.0, 5.0], 4.0, 6.0);
    let pi = core::f64::consts::PI;
    let truth = 2000.0 + pi * 16.0 * 6.0;
    let rim: Incidences = edges(&b, fb.bottom)
        .into_iter()
        .map(|e| on(B, e, fa.top))
        .collect();
    let (w0, w1) = same(&a, &b, BooleanOp::Union, &rim, truth);
    assert!(w1 < w0, "work {w1} with the rim, {w0} without");
    let disc: Incidences = [Fact::FaceOnFace {
        a: fa.top,
        b: fb.bottom,
    }]
    .into_iter()
    .collect();
    same(&a, &b, BooleanOp::Union, &disc, truth);
    // A hole through the plate: the rim on the bottom as well.
    let (t, ft) = cylinder([10.0, 10.0, 0.0], 4.0, 5.0);
    let through: Incidences = edges(&t, ft.bottom)
        .into_iter()
        .map(|e| on(B, e, fa.bottom))
        .chain(edges(&t, ft.top).into_iter().map(|e| on(B, e, fa.top)))
        .collect();
    same(
        &a,
        &t,
        BooleanOp::Subtract,
        &through,
        2000.0 - pi * 16.0 * 5.0,
    );
}

#[test]
fn coaxial_walls_give_the_same_answer() {
    // Two cylinders on one axis and radius: the walls coincide, each rim lies on the other's
    // wall. Built or not, the answer with the facts is the answer without.
    let (a, fa) = cylinder([0.0; 3], 5.0, 10.0);
    let (b, fb) = cylinder([0.0, 0.0, 4.0], 5.0, 10.0);
    let facts: Incidences = [Fact::FaceOnFace {
        a: fa.wall,
        b: fb.wall,
    }]
    .into_iter()
    .chain(edges(&b, fb.bottom).into_iter().map(|e| on(B, e, fa.wall)))
    .chain(edges(&a, fa.top).into_iter().map(|e| on(A, e, fb.wall)))
    .collect();
    let truth = core::f64::consts::PI * 25.0 * 14.0;
    match plain(&a, &b, BooleanOp::Union) {
        Ok(_) => {
            same(&a, &b, BooleanOp::Union, &facts, truth);
        }
        Err(e) => assert_eq!(given(&a, &b, BooleanOp::Union, &facts).unwrap_err(), e),
    }
}

#[test]
fn a_false_edge_fact_is_refused() {
    let (a, fa, b, fb) = stacked();
    // The upper block's top edges are 4 mm above the cube's top: their boxes are apart, so
    // stage 2 never pairs them and each is checked on its own.
    let top = edges(&b, fb.top)[0];
    let lie = on(B, top, fa.top);
    let facts: Incidences = [lie].into_iter().collect();
    assert_eq!(refused(given(&a, &b, BooleanOp::Union, &facts)), lie);
    // A side edge of the upper block crosses the cube's top plane at one end: tested in
    // stage 2, where the roots find it leaving.
    let side = edges(&b, fb.sides[0])
        .into_iter()
        .find(|e| !edges(&b, fb.bottom).contains(e) && !edges(&b, fb.top).contains(e))
        .unwrap();
    let lie = on(B, side, fa.top);
    let facts: Incidences = [lie].into_iter().collect();
    assert_eq!(refused(given(&a, &b, BooleanOp::Union, &facts)), lie);
    // True facts beside it do not hide it.
    let facts: Incidences = edges(&b, fb.bottom)
        .into_iter()
        .map(|e| on(B, e, fa.top))
        .chain([lie])
        .collect();
    assert_eq!(refused(given(&a, &b, BooleanOp::Union, &facts)), lie);
}

#[test]
fn a_false_face_fact_is_refused() {
    let (a, fa, b, fb) = stacked();
    for lie in [
        // Parallel planes 10 mm apart.
        Fact::FaceOnFace {
            a: fa.bottom,
            b: fb.bottom,
        },
        // Perpendicular planes that meet along a line.
        Fact::FaceOnFace {
            a: fa.sides[0],
            b: fb.bottom,
        },
    ] {
        let facts: Incidences = [lie].into_iter().collect();
        assert_eq!(refused(given(&a, &b, BooleanOp::Union, &facts)), lie);
    }
}

#[test]
fn a_fact_naming_what_is_not_there_is_refused() {
    let (a, fa, b, fb) = stacked();
    let e = edges(&b, fb.bottom)[0];
    for lie in [
        on(Operand::from_raw(2), e, fa.top),
        on(B, EdgeId::from_raw(999), fa.top),
        on(B, e, FaceId::from_raw(999)),
        Fact::FaceOnFace {
            a: FaceId::from_raw(999),
            b: fb.bottom,
        },
    ] {
        let facts: Incidences = [lie].into_iter().collect();
        assert_eq!(refused(given(&a, &b, BooleanOp::Union, &facts)), lie);
    }
}

#[test]
fn near_the_coincidence_a_fact_is_judged_as_the_test_judges_it() {
    // The upper block raised by a fraction of the coincidence distance: still on the cube's
    // top as the Boolean reads it, so the fact holds and the answer is the same; raised by
    // many, the fact is false.
    let zone = Document::default().resolution().zone().mm();
    let (a, fa) = block([0.0; 3], [10.0; 3]);
    for (lift, holds) in [(0.25, true), (0.9, true), (50.0, false)] {
        let (b, fb) = block([0.0, 0.0, 10.0 + lift * zone], [10.0, 6.0, 4.0]);
        let facts: Incidences = edges(&b, fb.bottom)
            .into_iter()
            .map(|e| on(B, e, fa.top))
            .collect();
        let r = given(&a, &b, BooleanOp::Union, &facts);
        if holds {
            alike(plain(&a, &b, BooleanOp::Union), r);
        } else {
            let fact = refused(r);
            assert!(facts.facts().contains(&fact));
        }
    }
}

#[test]
fn no_facts_is_the_boolean() {
    let (a, _, b, _) = stacked();
    let want = plain(&a, &b, BooleanOp::Union).unwrap();
    let got = given(&a, &b, BooleanOp::Union, &Incidences::NONE).unwrap();
    assert_eq!(
        scx_record::encode(&got.body).unwrap(),
        scx_record::encode(&want.body).unwrap()
    );
    assert_eq!(got.report.work, want.report.work);
}

#[test]
fn incidences_are_sorted_and_each_once() {
    let (_, fa, b, fb) = stacked();
    let e = edges(&b, fb.bottom);
    let f: Incidences = [
        on(B, e[1], fa.top),
        on(B, e[0], fa.top),
        on(B, e[1], fa.top),
    ]
    .into_iter()
    .collect();
    assert_eq!(f.facts(), &[on(B, e[0], fa.top), on(B, e[1], fa.top)]);
}
