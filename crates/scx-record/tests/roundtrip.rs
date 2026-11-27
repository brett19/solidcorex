// Written for solidcorex. Apache-2.0.
//! The round-trip half of the record suite (research 63 RE5): every testkit body kind at
//! more seeds and sizes, and bodies produced by edits, in both encodings, to the bit, with
//! each record its own fixed point and binary → text → binary the same bytes. The wide
//! sweep is `#[ignore]`d (the slow tier); the rest runs in the normal tests.

#![allow(
    missing_docs,
    clippy::pedantic,
    clippy::indexing_slicing,
    clippy::panic,
    clippy::unwrap_used
)]

mod common;

use common::{body, dec, dec_text, doc, gapped, round_trip, slots};
use scx_brep::Brep;
use scx_num::math::Affine;
use scx_num::vector3d::{Point3, Vec3};
use scx_record::{encode, write_text};
use scx_testkit::bodies::Kind;

#[test]
fn every_kind_at_more_seeds_and_sizes() {
    for kind in Kind::ALL {
        for seed in 3..7 {
            for size in 1..=3 {
                // The big ones stay at two in the normal tier.
                if kind.faces(size) > 400 && seed > 4 {
                    continue;
                }
                round_trip(
                    &body(kind, seed, size),
                    &format!("{} {seed} {size}", kind.name()),
                );
            }
        }
    }
}

/// The slow sweep: seeds 0 to 24, sizes 1 to 5.
#[test]
#[ignore = "slow tier: every kind at 24 seeds and sizes 1 to 5; cargo test -p scx-record --test roundtrip -- --ignored"]
fn every_kind_wide() {
    for kind in Kind::ALL {
        for seed in 0..24 {
            for size in 1..=5 {
                if kind.faces(size) > 1500 && seed > 2 {
                    continue;
                }
                round_trip(
                    &body(kind, seed, size),
                    &format!("{} {seed} {size}", kind.name()),
                );
            }
        }
    }
}

fn rigid() -> Affine {
    let turn =
        Affine::rotation(Point3::new(0.3, -1.2, 0.7), Vec3::new(1.0, 2.0, -0.5), 0.9).unwrap();
    Affine::translation(Vec3::new(4.0, -3.5, 12.25)) * turn
}

fn maps() -> Vec<(&'static str, Affine)> {
    vec![
        ("rigid", rigid()),
        (
            "mirror",
            Affine::reflection(Point3::new(0.5, 1.0, -2.0), Vec3::new(0.3, -1.0, 0.8)).unwrap(),
        ),
        (
            "grow",
            Affine::uniform_scaling(Point3::new(1.5, -0.25, 2.0), 1e3).unwrap(),
        ),
        (
            "shrink",
            Affine::uniform_scaling(Point3::new(1.5, -0.25, 2.0), 1e-3).unwrap(),
        ),
        ("far", Affine::translation(Vec3::new(1e6, -2.5e6, 3e5))),
        // A quarter turn: coordinates land on zeros of either sign.
        (
            "quarter",
            Affine::rotation(
                Point3::new(0.0, 0.0, 0.0),
                Vec3::new(0.0, 0.0, 1.0),
                core::f64::consts::FRAC_PI_2,
            )
            .unwrap(),
        ),
    ]
}

/// A body moved by a map is a new body; its records round trip like any other.
#[test]
fn transformed_bodies_round_trip() {
    for kind in Kind::ALL {
        for (name, a) in maps() {
            let b = body(kind, 1, 2);
            let mut d = b.edit();
            d.transform(&a, &doc())
                .unwrap_or_else(|e| panic!("{} {name}: {e:?}", kind.name()));
            let moved = d
                .finish(&doc())
                .unwrap_or_else(|u| panic!("{} {name}: {}", kind.name(), u.cause))
                .0;
            round_trip(&moved, &format!("{} moved by {name}", kind.name()));
        }
    }
}

/// A body moved twice is moved once by the product, and each step is a record of its own:
/// the second record is not the first's.
#[test]
fn a_moved_body_is_another_record() {
    let b = body(Kind::Cylinder, 2, 3);
    let mut d = b.edit();
    d.transform(&rigid(), &doc()).unwrap();
    let moved = d.finish(&doc()).unwrap().0;
    assert_ne!(encode(&b).unwrap(), encode(&moved).unwrap());
    assert_ne!(write_text(&b).unwrap(), write_text(&moved).unwrap());
}

/// `compact` renumbers a gapped body densely; both the gapped body and its compaction are
/// records, and the compaction's record has no gap.
#[test]
fn compacted_bodies_round_trip() {
    for seed in 0..3 {
        let b = gapped(seed);
        let (dense, _) = b.compact();
        round_trip(&dense, &format!("compacted {seed}"));
        assert_eq!(dense.edge_slots(), dense.edges().len());
        assert_eq!(dense.vertex_slots(), dense.vertices().len());
        assert!(!write_text(&dense).unwrap().contains("\ngap "));
        round_trip(&b, &format!("gapped {seed}"));
    }
}

/// A body decoded from a record is a body like any other: edited, it gives the record the
/// original's edit gives.
#[test]
fn a_decoded_body_edits_and_round_trips() {
    for kind in [Kind::Block, Kind::Holed, Kind::Touching, Kind::Sphere] {
        let b = body(kind, 5, 2);
        let back: Brep = dec(&encode(&b).unwrap()).unwrap();
        let mut d = back.edit();
        d.transform(&rigid(), &doc()).unwrap();
        let moved = d.finish(&doc()).unwrap().0;
        round_trip(&moved, &format!("{} decoded then moved", kind.name()));
        // The same edit of the original and of the decoded copy gives the same record.
        let mut d = b.edit();
        d.transform(&rigid(), &doc()).unwrap();
        let direct = d.finish(&doc()).unwrap().0;
        assert_eq!(slots(&direct), slots(&moved), "{}", kind.name());
        assert_eq!(encode(&direct).unwrap(), encode(&moved).unwrap());
        assert_eq!(
            write_text(&direct).unwrap(),
            write_text(&dec_text(&write_text(&moved).unwrap()).unwrap()).unwrap()
        );
    }
}

/// The text is plain: ASCII, one record on its lines, ending in one newline; and the binary
/// starts with the magic and version.
#[test]
fn the_spellings_are_plain() {
    for kind in Kind::ALL {
        let b = body(kind, 0, 1);
        let text = write_text(&b).unwrap();
        assert!(text.is_ascii() && text.ends_with('\n') && !text.ends_with("\n\n"));
        assert!(text.starts_with("SCXB 1 "), "{text:.40}");
        assert!(!text.contains("  ") && !text.contains(" \n"));
        let bytes = encode(&b).unwrap();
        assert_eq!(&bytes[..6], b"SCXB\x01\x00");
    }
}
