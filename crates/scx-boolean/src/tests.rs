// Written for solidcorex. Apache-2.0.
//! The crate's own tests: stages 1 to 3 on blocks, each answer worked by hand, and the
//! public signature's errors; stage 4 in [`face_face`].
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

use std::sync::Arc;

use scx_brep::op::{Cx, NoTrace, Scratch, Settings};
use scx_brep::{Brep, Document, Operand};
use scx_num::budget::{Budget, Stop};
use scx_num::cert::{Basis, Recorded};
use scx_num::frame::Frame;
use scx_num::units::{Accuracy, Length, Resolution};
use scx_num::vector3d::{Point3, Vec3};
use scx_testkit::bodies::made::block;

use crate::boolean::{Interference, interfere};
use crate::contact::On;
use crate::geom::{Operands, Side};
use crate::{BooleanError, BooleanOp, BooleanRefusal, BooleanUnsupported, boolean};

mod apart;
mod cleanup;
mod contacts;
mod face_face;
mod imprint;
mod nonmanifold;
mod stage6;
mod tangent;
mod unsupported;
mod whole;

fn pt(p: [f64; 3]) -> Point3 {
    Point3::new(p[0], p[1], p[2])
}

/// The block from `lo` to `hi`, axis-aligned.
fn cuboid(lo: [f64; 3], hi: [f64; 3], doc: Document) -> Brep {
    let frame = Frame::new(pt(lo), Vec3::new(0.0, 0.0, 1.0), Vec3::new(1.0, 0.0, 0.0)).unwrap();
    let size = [0, 1, 2].map(|i| Length::from_mm(hi[i] - lo[i]));
    block(&frame, size, &doc).unwrap().body
}

fn unit(doc: Document) -> Brep {
    cuboid([0.0; 3], [1.0; 3], doc)
}

fn run(a: &Brep, b: &Brep) -> Result<Interference, BooleanError> {
    interfere(Operands { a, b }, &mut Budget::unlimited(), &Stop::new())
}

/// A point as the tests compare it: where, which side's vertex anchors it, and the kinds
/// of each side's members (`V`, `E`, `F`).
#[derive(Debug, PartialEq)]
struct Seen {
    at: [f64; 3],
    anchor: Option<Side>,
    kinds: [String; 2],
}

fn kinds(on: &[On]) -> String {
    on.iter()
        .map(|o| match o {
            On::Vertex(_) => 'V',
            On::Edge { .. } => 'E',
            On::Face { .. } => 'F',
        })
        .collect()
}

/// The points, sorted by position, rounded to 1e-9.
fn seen(i: &Interference) -> Vec<Seen> {
    let r = |x: f64| (x * 1e9).round() / 1e9;
    let mut out: Vec<Seen> = i
        .points
        .iter()
        .map(|p| Seen {
            at: [r(p.at.x), r(p.at.y), r(p.at.z)],
            anchor: p.anchor.map(|(s, _)| s),
            kinds: [kinds(&p.on[0]), kinds(&p.on[1])],
        })
        .collect();
    out.sort_by(|p, q| p.at.partial_cmp(&q.at).unwrap());
    out
}

fn s(at: [f64; 3], anchor: Option<Side>, ka: &str, kb: &str) -> Seen {
    Seen {
        at,
        anchor,
        kinds: [ka.to_owned(), kb.to_owned()],
    }
}

fn sorted(mut v: Vec<Seen>) -> Vec<Seen> {
    v.sort_by(|p, q| p.at.partial_cmp(&q.at).unwrap());
    v
}

#[test]
fn disjoint_blocks_meet_nowhere() {
    let doc = Document::default();
    let i = run(&unit(doc), &cuboid([2.0; 3], [3.0; 3], doc)).unwrap();
    assert!(i.pairs.is_empty());
    assert!(i.points.is_empty());
    assert!(i.spans.is_empty());
}

#[test]
fn overlapping_cubes_cross_at_six_edge_face_points() {
    // b = a + (0.5, 0.5, 0.5): each of the three edges of a's corner (1, 1, 1) crosses a
    // face of b's corner (0.5, 0.5, 0.5), and each of the three edges of b's corner
    // crosses a face of a's.
    let doc = Document::default();
    let i = run(&unit(doc), &cuboid([0.5; 3], [1.5; 3], doc)).unwrap();
    let want = sorted(vec![
        s([0.5, 1.0, 1.0], None, "E", "F"),
        s([1.0, 0.5, 1.0], None, "E", "F"),
        s([1.0, 1.0, 0.5], None, "E", "F"),
        s([1.0, 0.5, 0.5], None, "F", "E"),
        s([0.5, 1.0, 0.5], None, "F", "E"),
        s([0.5, 0.5, 1.0], None, "F", "E"),
    ]);
    assert_eq!(seen(&i), want);
    assert!(i.points.iter().all(|p| p.gap < 1e-12 && !p.tangent));
    assert!(i.spans.is_empty());
    // Each pair is a face of a's corner (x, y, z = 1) with a face of b's corner (x, y,
    // z = 0.5) across it: the three parallel pairs are half apart, so 9 - 3.
    assert_eq!(i.pairs.len(), 6);
}

#[test]
fn blocks_sharing_a_face_meet_at_its_four_corners() {
    let doc = Document::default();
    let a = unit(doc);
    let i = run(&a, &cuboid([1.0, 0.0, 0.0], [2.0, 1.0, 1.0], doc)).unwrap();
    let want = sorted(
        [
            [1.0, 0.0, 0.0],
            [1.0, 0.0, 1.0],
            [1.0, 1.0, 0.0],
            [1.0, 1.0, 1.0],
        ]
        .into_iter()
        .map(|p| s(p, Some(Side::A), "V", "V"))
        .collect(),
    );
    assert_eq!(seen(&i), want);
    // Each point is a's vertex there, and b's vertex coincides exactly.
    for p in &i.points {
        let Some((Side::A, v)) = p.anchor else {
            panic!("anchor");
        };
        assert_eq!(a.vertex(v).unwrap().point(), p.at);
        assert!(p.gap < 1e-12);
    }
    // The four shared edges lie on b's faces, and b's on a's.
    assert!(i.spans.iter().any(|x| x.side == Side::A));
    assert!(i.spans.iter().any(|x| x.side == Side::B));
}

#[test]
fn blocks_touching_at_a_corner_meet_at_one_vertex_pair() {
    let doc = Document::default();
    let i = run(&unit(doc), &cuboid([1.0; 3], [2.0; 3], doc)).unwrap();
    assert_eq!(seen(&i), vec![s([1.0; 3], Some(Side::A), "V", "V")]);
}

#[test]
fn collinear_edges_meet_at_each_others_ends() {
    // b's vertical edge at x = y = 1 runs over z in [-0.5, 0.5], a's over [0, 1]: b's top
    // end lies inside a's edge, a's bottom end inside b's.
    let doc = Document::default();
    let i = run(&unit(doc), &cuboid([1.0, 1.0, -0.5], [2.0, 2.0, 0.5], doc)).unwrap();
    let want = vec![
        s([1.0, 1.0, 0.0], Some(Side::A), "V", "E"),
        s([1.0, 1.0, 0.5], Some(Side::B), "E", "V"),
    ];
    assert_eq!(seen(&i), want);
}

#[test]
fn a_block_through_a_side_meets_in_edge_edge_vertex_face_and_edge_face_points() {
    // b = [0.25, 0.75] x [-0.5, 0.5] x [0.5, 1]: flush with a's top, through a's front.
    let doc = Document::default();
    let i = run(
        &unit(doc),
        &cuboid([0.25, -0.5, 0.5], [0.75, 0.5, 1.0], doc),
    )
    .unwrap();
    let want = sorted(vec![
        s([0.25, 0.0, 1.0], None, "E", "E"),
        s([0.75, 0.0, 1.0], None, "E", "E"),
        s([0.25, 0.5, 1.0], Some(Side::B), "F", "V"),
        s([0.75, 0.5, 1.0], Some(Side::B), "F", "V"),
        s([0.25, 0.0, 0.5], None, "F", "E"),
        s([0.75, 0.0, 0.5], None, "F", "E"),
    ]);
    assert_eq!(seen(&i), want);
    // a's front top edge lies on b's top face, and b's edges at the top on a's top face.
    assert!(!i.spans.is_empty());
}

#[test]
fn swapping_the_operands_mirrors_the_answer() {
    let doc = Document::default();
    let a = unit(doc);
    let b = cuboid([0.25, -0.5, 0.5], [0.75, 0.5, 1.0], doc);
    let ab = run(&a, &b).unwrap();
    let ba = run(&b, &a).unwrap();
    let mirror = |x: &Interference| {
        let mut v = seen(x);
        for p in &mut v {
            p.kinds.swap(0, 1);
            p.anchor = p.anchor.map(Side::other);
        }
        v
    };
    assert_eq!(seen(&ab), mirror(&ba));
    // And the same call twice is the same, to the bit, work included.
    let mut b1 = Budget::unlimited();
    let mut b2 = Budget::unlimited();
    let one = interfere(Operands { a: &a, b: &b }, &mut b1, &Stop::new()).unwrap();
    let two = interfere(Operands { a: &a, b: &b }, &mut b2, &Stop::new()).unwrap();
    assert_eq!(one, two);
    assert_eq!(b1.used(), b2.used());
}

/// `body` with vertex `at`'s accuracy raised to `acc` mm.
fn loosened(body: &Brep, at: [f64; 3], acc: f64, doc: Document) -> Brep {
    let v = body.vertices().find(|v| v.point() == pt(at)).unwrap().id();
    let mut d = body.edit();
    d.vertex_mut(v).unwrap().accuracy = Some(Arc::new(Recorded {
        acc: Accuracy::new(Length::from_mm(acc)).unwrap(),
        basis: Basis::Measured,
    }));
    d.finish(&doc).unwrap().0
}

#[test]
fn two_vertices_of_one_operand_within_coincidence_are_refused() {
    // a's vertex (1, 1, 1) is loose by 1.5, so (1, 1, 0) is within its coincidence; b
    // shares the edge between them, so both meet b's vertex (1, 1, 0) at one place.
    let doc = Document::default();
    let a = loosened(&unit(doc), [1.0; 3], 1.5, doc);
    let b = cuboid([1.0, 1.0, 0.0], [2.0, 2.0, 1.0], doc);
    let e = run(&a, &b).unwrap_err();
    let BooleanError::Refused(BooleanRefusal::VerticesWithinCoincidence {
        operand,
        vertices: [v, w],
    }) = e
    else {
        panic!("{e:?}");
    };
    assert_eq!(operand, Operand::from_raw(0));
    assert!(v < w);
    let ps = [a.vertex(v).unwrap().point(), a.vertex(w).unwrap().point()];
    assert!(ps.contains(&pt([1.0; 3])));
}

#[test]
fn budget_and_stop_reach_the_caller() {
    let doc = Document::default();
    let (a, b) = (unit(doc), cuboid([0.5; 3], [1.5; 3], doc));
    let ops = Operands { a: &a, b: &b };
    let e = interfere(ops, &mut Budget::new(1), &Stop::new()).unwrap_err();
    assert_eq!(e, BooleanError::Exhausted { limit: 1 });
    let stop = Stop::new();
    stop.set();
    let e = interfere(ops, &mut Budget::unlimited(), &stop).unwrap_err();
    assert_eq!(e, BooleanError::Cancelled);
}

fn call(a: &Brep, b: &Brep, doc: Document, settings: &Settings) -> BooleanError {
    let mut scratch = Scratch::new();
    let stop = Stop::new();
    let mut cx = Cx::new(&doc, settings, &mut scratch, &stop, &NoTrace);
    boolean(a, b, BooleanOp::Union, &mut cx).unwrap_err()
}

#[test]
fn the_public_call_runs_every_stage_within_the_settings_budget() {
    let doc = Document::default();
    let (a, b) = (unit(doc), cuboid([0.5; 3], [1.5; 3], doc));
    let mut scratch = Scratch::new();
    let stop = Stop::new();
    let settings = Settings::new();
    let mut cx = Cx::new(&doc, &settings, &mut scratch, &stop, &NoTrace);
    let done = boolean(&a, &b, BooleanOp::Union, &mut cx).unwrap();
    assert_eq!(done.body.faces().count(), 12);
    assert!(done.report.work > 0);
    // The budget is the settings'.
    let e = call(&a, &b, doc, &Settings::new().with_budget(1));
    assert_eq!(e, BooleanError::Exhausted { limit: 1 });
}

#[test]
fn an_operand_of_another_document_is_refused() {
    let doc = Document::default();
    let other = Document::with_resolution(Resolution::new(Length::from_mm(1e-4)).unwrap());
    let (a, b) = (unit(doc), unit(other));
    let e = call(&a, &b, doc, &Settings::new());
    assert_eq!(
        e,
        BooleanError::Refused(BooleanRefusal::OtherDocument {
            operand: Operand::from_raw(1)
        })
    );
}

#[test]
fn two_vertices_joined_only_through_a_chain_are_unsupported() {
    // a's vertical edge at x = y = 1 is 1.5 resolutions long; b's corner sits halfway up,
    // within coincidence of both ends, which are not within coincidence of each other.
    let doc = Document::default();
    let r = doc.resolution().zone().mm();
    let a = cuboid([0.0; 3], [1.0, 1.0, 1.5 * r], doc);
    let b = cuboid([1.0, 1.0, 0.75 * r], [2.0, 2.0, 1.0], doc);
    let e = run(&a, &b).unwrap_err();
    let BooleanError::Unsupported(BooleanUnsupported::Chain {
        operand,
        vertices: [v, w],
    }) = e
    else {
        panic!("{e:?}");
    };
    assert_eq!(operand, Operand::from_raw(0));
    assert!(v < w);
    let d = a
        .vertex(v)
        .unwrap()
        .point()
        .distance(a.vertex(w).unwrap().point());
    assert!(d > r && d < 2.0 * r);
}
