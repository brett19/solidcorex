// Written for solidcorex. Apache-2.0.
//! `boolean_all`: closed-form volumes, operand order, work summed step by step, the budget
//! and the refusals.
//!
//! Ported (judged by outcome, against closed forms): solidcore's `union_all` tests of two
//! disjoint boxes, two overlapping boxes, a five-box chain and a single body.

use std::f64::consts::PI;

use scx_brep::op::{Cx, Done, NoTrace, Scratch, Settings};
use scx_brep::{Brep, Document, Operand};
use scx_num::budget::Stop;
use scx_num::units::{Length, Resolution};

use super::cuboid;
use super::face_face::{X, Z, cyl};
use super::whole::{INTERSECT, SUBTRACT, UNION, checked, near, op, volume};
use crate::{BooleanError, BooleanOp, BooleanRefusal, NoItems, boolean_all};

fn all(
    first: &Brep,
    rest: &[&Brep],
    which: BooleanOp,
    doc: Document,
    settings: &Settings,
) -> Result<Done<NoItems>, BooleanError> {
    let mut scratch = Scratch::new();
    let stop = Stop::new();
    let mut cx = Cx::new(&doc, settings, &mut scratch, &stop, &NoTrace);
    boolean_all(first, rest, which, &mut cx)
}

/// `boolean_all` checked by the contract.
fn all_checked(first: &Brep, rest: &[&Brep], which: BooleanOp, doc: Document) -> Done<NoItems> {
    let done = all(first, rest, which, doc, &Settings::new())
        .unwrap_or_else(|e| panic!("{which:?}: {e:?}"));
    let mut scratch = Scratch::new();
    let stop = Stop::new();
    let settings = Settings::new();
    let cx = Cx::new(&doc, &settings, &mut scratch, &stop, &NoTrace);
    if let Err(e) = scx_check::contract(&done.body, &cx, scx_check::Contacts::Refused) {
        panic!("{which:?}: contract {e:?}");
    }
    done
}

/// `n` unit blocks, each `step` along x from the last.
fn chain(n: u32, step: f64, doc: Document) -> Vec<Brep> {
    (0..n)
        .map(|i| {
            let x = f64::from(i) * step;
            cuboid([x, 0.0, 0.0], [x + 1.0, 1.0, 1.0], doc)
        })
        .collect()
}

fn refs(v: &[Brep]) -> Vec<&Brep> {
    v.iter().collect()
}

#[test]
fn two_disjoint_blocks_unite_to_the_sum() {
    let doc = Document::default();
    let a = cuboid([0.0; 3], [1.0; 3], doc);
    let b = cuboid([3.0, 0.0, 0.0], [4.0, 1.0, 1.0], doc);
    let done = all_checked(&a, &[&b], UNION, doc);
    near(volume(&done.body), 2.0, 1e-9);
}

#[test]
fn two_overlapping_blocks_unite_without_double_counting() {
    let doc = Document::default();
    let a = cuboid([0.0; 3], [2.0, 1.0, 1.0], doc);
    let b = cuboid([1.0, 0.0, 0.0], [3.0, 1.0, 1.0], doc);
    near(volume(&all_checked(&a, &[&b], UNION, doc).body), 3.0, 1e-9);
}

#[test]
fn a_five_block_chain_unites_to_its_staircase() {
    // Three levels, one operand carried across a level unpaired; each overlaps the next by
    // half: five unit blocks span 3.
    let doc = Document::default();
    let v = chain(5, 0.5, doc);
    let r = refs(&v);
    let done = all_checked(r[0], &r[1..], UNION, doc);
    near(volume(&done.body), 3.0, 1e-9);
}

#[test]
fn one_operand_is_no_boolean() {
    let doc = Document::default();
    let a = cuboid([0.0; 3], [2.0, 1.0, 1.0], doc);
    for which in [UNION, SUBTRACT, INTERSECT] {
        let done = all(&a, &[], which, doc, &Settings::new()).unwrap();
        near(volume(&done.body), 2.0, 1e-9);
        assert_eq!(done.report.work, 0);
        assert!(done.changes.is_empty());
        assert_eq!(done.body.faces().count(), 6);
    }
}

#[test]
fn two_operands_are_the_boolean_of_two() {
    let doc = Document::default();
    let a = cuboid([0.0; 3], [10.0; 3], doc);
    let b = cuboid([5.0; 3], [15.0; 3], doc);
    for which in [UNION, SUBTRACT, INTERSECT] {
        let one = checked(&a, &b, which, doc);
        let n = all_checked(&a, &[&b], which, doc);
        assert_eq!(n.report.work, one.report.work, "{which:?}");
        assert_eq!(n.body.faces().count(), one.body.faces().count());
        near(volume(&n.body), volume(&one.body), 1e-12);
    }
}

#[test]
fn disjoint_cylinders_unite_to_k_times_one() {
    let doc = Document::default();
    let v: Vec<Brep> = (0..3)
        .map(|i| cyl([4.0 * f64::from(i), 0.0, 0.0], Z, X, 1.0, 2.0, doc))
        .collect();
    let r = refs(&v);
    let done = all_checked(r[0], &r[1..], UNION, doc);
    near(volume(&done.body), 6.0 * PI, 1e-9);
    assert_eq!(done.body.faces().count(), 9);
}

#[test]
fn a_plate_less_two_bores_is_the_plate_less_both() {
    let doc = Document::default();
    let plate = cuboid([-2.0, -2.0, 0.0], [6.0, 2.0, 1.0], doc);
    let b0 = cyl([0.0, 0.0, -1.0], Z, X, 1.0, 3.0, doc);
    let b1 = cyl([4.0, 0.0, -1.0], Z, X, 1.0, 3.0, doc);
    let done = all_checked(&plate, &[&b0, &b1], SUBTRACT, doc);
    near(volume(&done.body), 32.0 - 2.0 * PI, 1e-9);
    // The plate's six faces and each bore's wall.
    assert_eq!(done.body.faces().count(), 8);
}

#[test]
fn three_overlapping_cubes_subtract_and_intersect() {
    let doc = Document::default();
    let a = cuboid([0.0; 3], [10.0; 3], doc);
    let b = cuboid([5.0; 3], [15.0; 3], doc);
    let c = cuboid([7.0; 3], [17.0; 3], doc);
    // a less b or c: c's share of a lies in b's, so 1000 - 125.
    near(
        volume(&all_checked(&a, &[&b, &c], SUBTRACT, doc).body),
        875.0,
        1e-9,
    );
    near(
        volume(&all_checked(&a, &[&b, &c], INTERSECT, doc).body),
        27.0,
        1e-9,
    );
    near(
        volume(&all_checked(&a, &[&b, &c], UNION, doc).body),
        3.0 * 1000.0 - 125.0 - 27.0 - 512.0 + 27.0,
        1e-9,
    );
}

/// Every order of four blocks gives one volume, one face count and the closed form.
#[test]
fn union_of_four_blocks_is_the_same_in_every_order() {
    let doc = Document::default();
    let v = chain(4, 0.4, doc);
    let mut order = [0usize, 1, 2, 3];
    let mut seen = 0;
    permute(&mut order, 0, &mut |p| {
        let r: Vec<&Brep> = p.iter().map(|&i| &v[i]).collect();
        let done = all_checked(r[0], &r[1..], UNION, doc);
        near(volume(&done.body), 2.2, 1e-9);
        assert_eq!(done.body.faces().count(), 6, "{p:?}");
        seen += 1;
    });
    assert_eq!(seen, 24);
}

fn permute(a: &mut [usize], k: usize, f: &mut impl FnMut(&[usize])) {
    if k == a.len() {
        f(a);
        return;
    }
    for i in k..a.len() {
        a.swap(k, i);
        permute(a, k + 1, f);
        a.swap(k, i);
    }
}

#[test]
fn the_same_operands_give_the_same_work() {
    let doc = Document::default();
    let v = chain(5, 0.4, doc);
    let r = refs(&v);
    let a = all(r[0], &r[1..], UNION, doc, &Settings::new()).unwrap();
    let b = all(r[0], &r[1..], UNION, doc, &Settings::new()).unwrap();
    assert_eq!(a.report.work, b.report.work);
    assert_eq!(a.body.faces().count(), b.body.faces().count());
}

#[test]
fn work_is_the_steps_summed_in_the_folds_order() {
    // Four operands fold as (0 u 1) u (2 u 3).
    let doc = Document::default();
    let v = chain(4, 0.4, doc);
    let r = refs(&v);
    let left = op(r[0], r[1], UNION, doc).unwrap();
    let right = op(r[2], r[3], UNION, doc).unwrap();
    let top = op(&left.body, &right.body, UNION, doc).unwrap();
    let by_hand = left.report.work + right.report.work + top.report.work;
    let done = all(r[0], &r[1..], UNION, doc, &Settings::new()).unwrap();
    assert_eq!(done.report.work, by_hand);
    // Subtraction: the tools unioned first, then subtracted.
    let plate = cuboid([-2.0, -2.0, 0.0], [6.0, 2.0, 1.0], doc);
    let b0 = cyl([0.0, 0.0, -1.0], Z, X, 1.0, 3.0, doc);
    let b1 = cyl([4.0, 0.0, -1.0], Z, X, 1.0, 3.0, doc);
    let tools = op(&b0, &b1, UNION, doc).unwrap();
    let cut = op(&plate, &tools.body, SUBTRACT, doc).unwrap();
    let done = all(&plate, &[&b0, &b1], SUBTRACT, doc, &Settings::new()).unwrap();
    assert_eq!(done.report.work, tools.report.work + cut.report.work);
}

#[test]
fn the_budget_is_the_whole_folds() {
    let doc = Document::default();
    let v = chain(4, 0.4, doc);
    let r = refs(&v);
    let total = all(r[0], &r[1..], UNION, doc, &Settings::new())
        .unwrap()
        .report
        .work;
    let fits = all(
        r[0],
        &r[1..],
        UNION,
        doc,
        &Settings::new().with_budget(total),
    );
    assert_eq!(fits.unwrap().report.work, total);
    // Short of the sum by one, though every step alone fits: the fold runs out.
    let short = all(
        r[0],
        &r[1..],
        UNION,
        doc,
        &Settings::new().with_budget(total - 1),
    )
    .unwrap_err();
    assert_eq!(short, BooleanError::Exhausted { limit: total - 1 });
}

#[test]
fn an_operand_of_another_document_is_refused_by_its_place_in_the_call() {
    let doc = Document::default();
    let other = Document::with_resolution(Resolution::new(Length::from_mm(1e-4)).unwrap());
    let a = cuboid([0.0; 3], [1.0; 3], doc);
    let b = cuboid([3.0, 0.0, 0.0], [4.0, 1.0, 1.0], doc);
    let c = cuboid([6.0, 0.0, 0.0], [7.0, 1.0, 1.0], other);
    let e = all(&a, &[&b, &c], UNION, doc, &Settings::new()).unwrap_err();
    assert_eq!(
        e,
        BooleanError::Refused(BooleanRefusal::OtherDocument {
            operand: Operand::from_raw(2)
        })
    );
}
