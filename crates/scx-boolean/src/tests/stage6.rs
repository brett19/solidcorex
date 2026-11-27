// Written for solidcorex. Apache-2.0.
//! Stage 6's check and stage 7's merge across turned faces: a seed's one-point check
//! answers a wrong seed with [`BooleanDefect::Seeds`], and a subtract whose turned piece
//! lies flush beside a face of `a` merges the two.

use std::collections::BTreeSet;

use scx_brep::{Brep, Document};
use scx_num::budget::{Budget, Stop};

use super::face_face::{X, Z, cyl};
use super::whole::{area, checked, face_from, near, volume};
use super::{cuboid, run};
use crate::classify::{Class, check_seed, classify};
use crate::faceface::face_face;
use crate::geom::Operands;
use crate::imprint::imprint;
use crate::{BooleanDefect, BooleanError, BooleanOp};

#[test]
fn a_wrong_seed_is_a_seeds_defect() {
    // A plate less a cylinder through it: every piece checked by one point, with its own
    // class and with the other.
    let doc = Document::default();
    let a = cuboid([-2.0, -2.0, 0.0], [2.0, 2.0, 1.0], doc);
    let b = cyl([0.0, 0.0, -1.0], Z, X, 1.0, 3.0, doc);
    let ops = Operands { a: &a, b: &b };
    let mut budget = Budget::unlimited();
    let stop = Stop::new();
    let i = run(&a, &b).unwrap();
    let f = face_face(ops, &i, &mut budget, &stop).unwrap();
    let m = imprint(ops, &i, &f, doc, &mut budget, &stop).unwrap();
    let c = classify(ops, &f, &m, &mut budget, &stop).unwrap();
    // No edge counts as met, so a cut's middle falls on the other operand and checks
    // nothing: the plate's two disks in the hole, bounded by the cut alone, go unchecked.
    let met = BTreeSet::new();
    let (mut caught, mut unchecked) = (0, Vec::new());
    for (&p, &class) in &c.class {
        let flipped = match class {
            Class::In => Class::Out,
            Class::Out => Class::In,
            Class::On { .. } => continue,
        };
        let side = c.owner[&p].0;
        check_seed(ops, &m, side, &[p], &met, class, &mut budget, &stop)
            .unwrap_or_else(|e| panic!("{p:?} as {class:?}: {e:?}"));
        match check_seed(ops, &m, side, &[p], &met, flipped, &mut budget, &stop) {
            Err(BooleanError::Defect(BooleanDefect::Seeds { face, .. })) if face == p => {
                caught += 1;
            }
            Ok(()) => unchecked.push(p),
            e => panic!("{p:?} as {flipped:?}: {e:?}"),
        }
    }
    assert!(caught >= 9, "{caught} pieces caught");
    assert_eq!(unchecked.len(), 2, "{unchecked:?}");
    for p in unchecked {
        assert_eq!(m.draft.face(p).unwrap().uses().count(), 1, "{p:?}");
    }
}

/// An L: [0, 2] x [0, 1] x [0, 1] with [0, 1] x [0, 1] x [1, 2] on it, made two ways. As
/// a union its step's top is the lower block's, natural normal up; as the block
/// [0, 2] x [0, 1] x [0, 2] less its corner it is the corner's bottom turned over, natural
/// normal down.
fn ell(union: bool, doc: Document) -> Brep {
    if union {
        let low = cuboid([0.0; 3], [2.0, 1.0, 1.0], doc);
        let high = cuboid([0.0, 0.0, 1.0], [1.0, 1.0, 2.0], doc);
        checked(&low, &high, BooleanOp::Union, doc).body
    } else {
        let block = cuboid([0.0; 3], [2.0, 1.0, 2.0], doc);
        let notch = cuboid([1.0, -1.0, 1.0], [3.0, 2.0, 3.0], doc);
        checked(&block, &notch, BooleanOp::Subtract, doc).body
    }
}

#[test]
fn a_turned_piece_flush_with_a_face_merges_with_it() {
    // The L less its upper arm: b's bottom, turned, lies on z = 1 beside the step's top
    // with the material below both, one face of area 2 from both operands. Made by the
    // union, the two natural normals are opposite (§14.35); made by the subtract, alike.
    for union in [true, false] {
        let doc = Document::default();
        let l = ell(union, doc);
        near(volume(&l), 3.0, 1e-9);
        let arm = cuboid([0.0, -1.0, 1.0], [1.0, 2.0, 3.0], doc);
        let d = checked(&l, &arm, BooleanOp::Subtract, doc);
        near(volume(&d.body), 2.0, 1e-9);
        assert_eq!(d.body.faces().count(), 6, "{union}");
        let tops: Vec<_> = d
            .body
            .faces()
            .filter(|f| (area(&d.body, f.id()) - 2.0).abs() < 1e-9)
            .map(|f| face_from(&d.changes, f.id()))
            .collect();
        // Top, bottom, front and back; only the top from both operands.
        assert_eq!(tops.len(), 4, "{union}: {tops:?}");
        let both = tops
            .iter()
            .filter(|from| from.iter().map(|o| o.0).collect::<BTreeSet<_>>().len() == 2)
            .count();
        assert_eq!(both, 1, "{union}: {tops:?}");
    }
}
