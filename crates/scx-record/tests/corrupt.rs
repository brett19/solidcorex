// Written for solidcorex. Apache-2.0.
//! The corruption half of the record suite (research 63 RE5), deterministic and seeded.
//!
//! - **Probes.** Every corrupted input goes through one probe, which asserts: no panic; an
//!   input that decodes re-encodes to itself (so the changed input was already the record's
//!   own spelling); an error is `Refused`, `Unsupported` or `Exhausted`, never `Defect`,
//!   points inside the input, and prints.
//! - **Targeted mutations.** One field at a time, each with the typed cause it must give:
//!   ids, counts, tags, floats to NaN and infinity (the exact offset), interval ends
//!   swapped, senses flipped, fins reordered, removed and doubled, records deleted and
//!   doubled, runs of gaps, the frame of the record, and the spellings only text has.
//! - **Sweeps.** Every byte of a small body's record set to a handful of values, every
//!   position's `u32` set to ids and counts, every float field set to the awkward doubles,
//!   every token of the text replaced from a dictionary, and every truncation, deletion
//!   and duplication; then a seeded fuzz of stacked changes over every kind. The wide
//!   forms are `#[ignore]`d (the slow tier).

#![allow(
    missing_docs,
    clippy::pedantic,
    clippy::indexing_slicing,
    clippy::panic,
    clippy::unwrap_used
)]

mod common;

use std::collections::{BTreeMap, BTreeSet};
use std::panic::{AssertUnwindSafe, catch_unwind};

use common::{body, dec, dec_text, doc, gapped};
use scx_brep::{
    Brep, EdgeId, FaceId, FinishCause, GeometryCause, RegionId, ShellId, StructureCause,
};
use scx_num::error::ErrorKind;
use scx_num::hash::Rng;
use scx_record::{Limits, ReadCause, ReadError, decode, encode, write_text};
use scx_testkit::bodies::Kind;

// ---- the probe ---------------------------------------------------------------------------

#[derive(Default)]
struct Stats {
    tried: usize,
    decoded: usize,
    /// Refusals by the cause's name.
    refused: BTreeMap<&'static str, usize>,
    /// The enums `BadTag` and `BadToken` named.
    tags: BTreeSet<&'static str>,
}

impl Stats {
    fn note(&mut self, e: &ReadError) {
        *self.refused.entry(e.name()).or_default() += 1;
        match e.cause {
            ReadCause::BadTag { record, .. } => {
                self.tags.insert(record);
            }
            ReadCause::BadToken { expected } => {
                self.tags.insert(expected);
            }
            _ => {}
        }
    }

    fn merge(&mut self, o: Stats) {
        self.tried += o.tried;
        self.decoded += o.decoded;
        for (k, v) in o.refused {
            *self.refused.entry(k).or_default() += v;
        }
        self.tags.extend(o.tags);
    }
}

/// Runs `f`; a panic is re-raised with `ctx`, since the mutation is what matters.
fn guarded<R>(ctx: &dyn Fn() -> String, f: impl FnOnce() -> R) -> R {
    match catch_unwind(AssertUnwindSafe(f)) {
        Ok(r) => r,
        Err(p) => {
            let msg = p
                .downcast_ref::<String>()
                .cloned()
                .or_else(|| p.downcast_ref::<&str>().map(|s| (*s).to_string()))
                .unwrap_or_default();
            panic!("PANIC at {}: {msg}", ctx());
        }
    }
}

fn check_error(e: &ReadError, len: usize, ctx: &dyn Fn() -> String) {
    assert!(e.at <= len, "{}: at {} past {len}: {e:?}", ctx(), e.at);
    assert!(
        matches!(
            e.kind(),
            ErrorKind::Refused | ErrorKind::Unsupported | ErrorKind::Exhausted
        ),
        "{}: kind {:?}: {e:?}",
        ctx(),
        e.kind()
    );
    assert!(!e.name().is_empty() && !format!("{e} {:?} {}", e.cause, e.cause).is_empty());
    assert_eq!(
        matches!(e.cause, ReadCause::Invalid(_)),
        std::error::Error::source(&e.cause).is_some() && e.name() == "invalid",
        "{}",
        ctx()
    );
}

/// A decoded body is a finished one: editing it and finishing again, with nothing changed,
/// succeeds and is the same record.
fn still_finishes(b: &Brep, ctx: &dyn Fn() -> String) {
    guarded(ctx, || {
        let again = b
            .edit()
            .finish(&doc())
            .unwrap_or_else(|u| {
                panic!("{}: decoded, but does not finish again: {}", ctx(), u.cause)
            })
            .0;
        assert!(encode(&again).unwrap() == encode(b).unwrap(), "{}", ctx());
    });
}

/// Decodes `bytes`: no panic; a body re-encodes to `bytes`; an error is sound.
fn probe(bytes: &[u8], st: &mut Stats, ctx: &dyn Fn() -> String) -> Option<ReadError> {
    st.tried += 1;
    match guarded(ctx, || dec(bytes)) {
        Ok(back) => {
            st.decoded += 1;
            let again = guarded(ctx, || encode(&back).unwrap());
            assert!(
                again == bytes,
                "{}: decodes but re-encodes differently",
                ctx()
            );
            still_finishes(&back, ctx);
            None
        }
        Err(e) => {
            check_error(&e, bytes.len(), ctx);
            st.note(&e);
            Some(e)
        }
    }
}

/// As [`probe`], for text.
fn probe_text(text: &str, st: &mut Stats, ctx: &dyn Fn() -> String) -> Option<ReadError> {
    st.tried += 1;
    match guarded(ctx, || dec_text(text)) {
        Ok(back) => {
            st.decoded += 1;
            let again = guarded(ctx, || write_text(&back).unwrap());
            assert!(
                again == text,
                "{}: decodes but re-encodes differently",
                ctx()
            );
            still_finishes(&back, ctx);
            None
        }
        Err(e) => {
            check_error(&e, text.len(), ctx);
            st.note(&e);
            Some(e)
        }
    }
}

// ---- the text, as lines of words ---------------------------------------------------------

#[derive(Clone)]
struct Doc(Vec<Vec<String>>);

impl Doc {
    fn new(text: &str) -> Self {
        Self(
            text.lines()
                .map(|l| l.split(' ').map(String::from).collect())
                .collect(),
        )
    }

    fn text(&self) -> String {
        self.0
            .iter()
            .map(|l| l.join(" "))
            .collect::<Vec<_>>()
            .join("\n")
            + "\n"
    }

    /// The byte offset of word `w` of line `l`.
    fn offset(&self, l: usize, w: usize) -> usize {
        let lines: usize = self.0[..l].iter().map(|x| x.join(" ").len() + 1).sum();
        lines + self.0[l][..w].iter().map(|x| x.len() + 1).sum::<usize>()
    }

    fn set(&self, l: usize, w: usize, new: &str) -> String {
        let mut d = self.clone();
        d.0[l][w] = new.into();
        d.text()
    }

    /// The first line starting with `a b`.
    fn find(&self, a: &str, b: &str) -> usize {
        self.0
            .iter()
            .position(|l| l[0] == a && l.get(1).is_some_and(|w| w == b))
            .unwrap()
    }
}

/// A body of the generators, as text, and where its first edge, use and vertex are. Edges
/// and faces have no recorded accuracy, so the first `false` of an edge's line follows its
/// ends; the edge line is `live curve K .. lo hi e0 e1 false k (face sense)*k`.
struct Located {
    doc: Doc,
    edge: usize,
    face: usize,
    vertex: usize,
    /// The word of the edge line's accuracy flag.
    flag: usize,
}

fn located(kind: Kind, seed: u64) -> Located {
    let text = write_text(&body(kind, seed, 1)).unwrap();
    let doc = Doc::new(&text);
    let edge = doc.find("live", "curve");
    let flag = doc.0[edge].iter().position(|w| w == "false").unwrap();
    let vertex = doc
        .0
        .iter()
        .position(|l| l.len() == 5 && l[0] == "live" && l[4] == "false")
        .unwrap();
    let face = doc.0.iter().position(|l| l[0] == "live" && l[1] == "plane");
    Located {
        edge,
        flag,
        vertex,
        face: face.unwrap_or(0),
        doc,
    }
}

fn invalid(r: Result<Brep, ReadError>) -> FinishCause {
    match r {
        Err(ReadError {
            cause: ReadCause::Invalid(c),
            ..
        }) => *c,
        Err(e) => panic!("not invalid: {e:?}"),
        Ok(_) => panic!("decoded"),
    }
}

fn refuses(text: &str) -> ReadError {
    match dec_text(text) {
        Err(e) => e,
        Ok(_) => panic!("decoded: {text:.60}"),
    }
}

// ---- targeted mutations: what is wrong is what the cause says ----------------------------

#[test]
fn structure_causes_in_the_edges_fins_and_uses() {
    let l = located(Kind::Block, 1);
    let (e0, f) = (l.edge, l.flag);
    let fin0 = f + 2;
    let eid = EdgeId::from_raw(0);
    let fin = |face: u32, reversed: bool| {
        scx_brep::Fin::new(
            FaceId::from_raw(face),
            if reversed {
                scx_num::frame::Sense::Reversed
            } else {
                scx_num::frame::Sense::Forward
            },
        )
    };
    let d = &l.doc;
    // The edge's first fin is (face 0, reversed) and its second (face 5, forward).
    assert_eq!(&d.0[e0][fin0..fin0 + 4], ["0", "reversed", "5", "forward"]);

    // A fin that a face has no use for, and a use that no fin matches.
    assert_eq!(
        invalid(dec_text(&d.set(e0, fin0, "3"))),
        FinishCause::Structure(StructureCause::FinWithoutUse {
            edge: eid,
            fin: fin(3, true)
        })
    );
    assert_eq!(
        invalid(dec_text(&d.set(e0, fin0 + 1, "forward"))),
        FinishCause::Structure(StructureCause::FinWithoutUse {
            edge: eid,
            fin: fin(0, false)
        })
    );
    // A fin naming a face that does not exist.
    assert_eq!(
        invalid(dec_text(&d.set(e0, fin0, "999"))),
        FinishCause::Structure(StructureCause::Dangling {
            from: eid.into(),
            to: FaceId::from_raw(999).into()
        })
    );
    // A fin doubled.
    let mut two = d.clone();
    two.0[e0][f + 1] = "3".into();
    let dup: Vec<String> = two.0[e0][fin0..fin0 + 2].to_vec();
    two.0[e0].extend(dup);
    assert_eq!(
        invalid(dec_text(&two.text())),
        FinishCause::Structure(StructureCause::RepeatedFin {
            edge: eid,
            fin: fin(0, true)
        })
    );
    // No fins at all: the use of face 0 has none.
    let mut none = d.clone();
    none.0[e0].truncate(fin0);
    none.0[e0][f + 1] = "0".into();
    assert_eq!(
        invalid(dec_text(&none.text())),
        FinishCause::Structure(StructureCause::UseWithoutFin {
            face: FaceId::from_raw(0),
            edge: eid,
            sense: scx_num::frame::Sense::Reversed
        })
    );
    // A fin short of its count is not a record: the count runs into the next line.
    let mut short = d.clone();
    short.0[e0].truncate(fin0 + 2);
    assert!(matches!(
        refuses(&short.text()).cause,
        ReadCause::BadToken { .. } | ReadCause::Truncated
    ));

    // The first use (face 0): an edge that does not exist, another edge, the other sense.
    let u = l.face + 2;
    assert_eq!(
        invalid(dec_text(&d.set(u, 0, "999"))),
        FinishCause::Structure(StructureCause::Dangling {
            from: FaceId::from_raw(0).into(),
            to: EdgeId::from_raw(999).into()
        })
    );
    for (word, new) in [(0, "5"), (1, "forward")] {
        assert!(
            matches!(
                invalid(dec_text(&d.set(u, word, new))),
                FinishCause::Structure(StructureCause::FinWithoutUse { .. })
            ),
            "{word}"
        );
    }
}

#[test]
fn ends_sides_and_vertices_keep_their_causes() {
    let l = located(Kind::Block, 1);
    let (e0, f, d) = (l.edge, l.flag, &l.doc);
    // An end moved to another vertex: the loop of the face no longer meets.
    for (word, at) in [(f - 2, 0), (f - 1, 3)] {
        assert_eq!(
            invalid(dec_text(&d.set(e0, word, "5"))),
            FinishCause::Structure(StructureCause::LoopNotConnected {
                face: FaceId::from_raw(0),
                lp: 0,
                at
            }),
            "word {word}"
        );
    }
    // A vertex moved off the ends of its edges.
    assert!(matches!(
        invalid(dec_text(&d.set(l.vertex, 1, "0.25"))),
        FinishCause::Geometry(GeometryCause::EdgeEndOffVertex { .. })
    ));
    // The edge's interval no longer the pcurves'.
    for (word, new) in [(f - 4, "0.25"), (f - 3, "0.5")] {
        assert!(
            matches!(
                invalid(dec_text(&d.set(e0, word, new))),
                FinishCause::Structure(StructureCause::PcurveInterval { .. })
            ),
            "word {word}"
        );
    }
    // Zero length: min = max, which an interval allows and a use does not match.
    let mut zero = d.clone();
    zero.0[e0][f - 4] = "1".into();
    assert!(matches!(
        invalid(dec_text(&zero.text())),
        FinishCause::Structure(StructureCause::PcurveInterval { .. })
    ));
    // The sides: a shell that does not exist; the sides swapped; both the same.
    let face = &d.0[l.face];
    let k = face.len();
    assert_eq!(&face[k - 3..], ["0", "1", "1"]);
    assert_eq!(
        invalid(dec_text(&d.set(l.face, k - 3, "999"))),
        FinishCause::Structure(StructureCause::Dangling {
            from: FaceId::from_raw(0).into(),
            to: ShellId::from_raw(999).into()
        })
    );
    for text in [d.set(l.face, k - 3, "1"), {
        let mut s = d.clone();
        s.0[l.face].swap(k - 3, k - 2);
        s.text()
    }] {
        assert!(matches!(
            invalid(dec_text(&text)),
            FinishCause::Structure(StructureCause::SidesDisagree { .. })
        ));
    }
    // A shell in a region that does not exist.
    let shell =
        d.0.iter()
            .position(|l| l[0] == "live" && l.len() == 4)
            .unwrap();
    assert_eq!(
        invalid(dec_text(&d.set(shell, 1, "999"))),
        FinishCause::Structure(StructureCause::Dangling {
            from: ShellId::from_raw(0).into(),
            to: RegionId::from_raw(999).into()
        })
    );
}

/// A cyclic shift of an edge's fins is the same radial order, so the two-fin edge swapped is
/// a body, and its record is the swapped one.
#[test]
fn two_fins_swapped_are_a_cyclic_shift_and_kept() {
    let l = located(Kind::Block, 1);
    let mut d = l.doc.clone();
    let at = l.flag + 2;
    for k in 0..2 {
        let (a, b) = (d.0[l.edge][at + k].clone(), d.0[l.edge][at + 2 + k].clone());
        d.0[l.edge][at + k] = b;
        d.0[l.edge][at + 2 + k] = a;
    }
    let b = dec_text(&d.text()).unwrap();
    assert_eq!(write_text(&b).unwrap(), d.text());
    assert_ne!(d.text(), l.doc.text());
}

/// Four fins reversed: the geometry orders them, whatever the record says (exists in
/// `body.rs` for text; here the binary of the same, rotated and reversed).
#[test]
fn four_fins_reordered_are_judged_by_the_geometry() {
    let l = located(Kind::Touching, 0);
    let e = l
        .doc
        .0
        .iter()
        .position(|w| w[0] == "live" && w[1] == "curve" && w[w.len() - 9] == "4")
        .unwrap();
    let n = l.doc.0[e].len();
    let pairs: Vec<[String; 2]> = (0..4)
        .map(|i| {
            [
                l.doc.0[e][n - 8 + 2 * i].clone(),
                l.doc.0[e][n - 7 + 2 * i].clone(),
            ]
        })
        .collect();
    let with = |order: [usize; 4]| {
        let mut d = l.doc.clone();
        for (i, &p) in order.iter().enumerate() {
            d.0[e][n - 8 + 2 * i] = pairs[p][0].clone();
            d.0[e][n - 7 + 2 * i] = pairs[p][1].clone();
        }
        d.text()
    };
    // Each cyclic shift is valid, and its text is its own record.
    for order in [[0, 1, 2, 3], [1, 2, 3, 0], [2, 3, 0, 1], [3, 0, 1, 2]] {
        let t = with(order);
        let b = dec_text(&t).unwrap_or_else(|e| panic!("{order:?}: {e:?}"));
        assert_eq!(write_text(&b).unwrap(), t);
        let bytes = encode(&b).unwrap();
        assert_eq!(write_text(&dec(&bytes).unwrap()).unwrap(), t);
    }
    // Reversed, and the swaps that are not a shift: refused, with the edge named.
    let edge0 = l.doc.find("live", "curve");
    for order in [[3, 2, 1, 0], [0, 1, 3, 2], [1, 0, 2, 3], [0, 2, 1, 3]] {
        // The sectors between fins must agree with their faces' sides, and then the
        // geometry must agree with the order: each is a cause that names the edge.
        let want = EdgeId::from_raw((e - edge0) as u32);
        match invalid(dec_text(&with(order))) {
            FinishCause::Geometry(GeometryCause::RadialOrder { edge, .. })
            | FinishCause::Structure(StructureCause::SidesDisagree { edge, .. }) => {
                assert_eq!(edge, want, "{order:?}");
            }
            other => panic!("{order:?}: {other:?}"),
        }
    }
}

#[test]
fn every_float_to_nan_or_infinity_is_refused_at_its_offset() {
    for (kind, seed) in [(Kind::Block, 1), (Kind::Cylinder, 2), (Kind::Sphere, 3)] {
        let l = located(kind, seed);
        let mut fields = 0;
        for (i, line) in l.doc.0.iter().enumerate() {
            for (j, w) in line.iter().enumerate() {
                if !(w.contains('.') || w.contains('e')) || w.parse::<f64>().is_err() {
                    continue;
                }
                for bad in ["NaN", "inf", "-inf"] {
                    let text = l.doc.set(i, j, bad);
                    let e = refuses(&text);
                    assert_eq!(
                        (e.at, &e.cause, e.kind()),
                        (
                            l.doc.offset(i, j),
                            &ReadCause::NonFinite,
                            ErrorKind::Refused
                        ),
                        "{} line {i} word {j} {bad}",
                        kind.name()
                    );
                }
                fields += 1;
            }
        }
        assert!(fields > 20, "{} {fields}", kind.name());
    }
}

/// The same in binary: each float's eight bytes found by value, set to the bits of a NaN,
/// an infinity, and the largest NaN payload; the refusal is at that float.
#[test]
fn every_binary_float_to_nan_or_infinity_is_refused_at_its_offset() {
    for (kind, seed) in [(Kind::Block, 1), (Kind::Torus, 2)] {
        let b = body(kind, seed, 1);
        let bytes = encode(&b).unwrap();
        let text = write_text(&b).unwrap();
        let mut hit = 0;
        let mut seen = BTreeSet::new();
        for w in text.split([' ', '\n']) {
            let Ok(v) = w.parse::<f64>() else { continue };
            if !(w.contains('.') || w.contains('e')) || !seen.insert(w.to_string()) {
                continue;
            }
            let pat = v.to_le_bytes();
            let spots: Vec<usize> = (0..=bytes.len() - 8)
                .filter(|&o| bytes[o..o + 8] == pat)
                .take(3)
                .collect();
            for o in spots {
                for bits in [
                    f64::NAN.to_bits(),
                    f64::INFINITY.to_bits(),
                    f64::NEG_INFINITY.to_bits(),
                    0xFFFF_FFFF_FFFF_FFFF,
                    0x7FF0_0000_0000_0001,
                ] {
                    let mut x = bytes.clone();
                    x[o..o + 8].copy_from_slice(&bits.to_le_bytes());
                    let e = dec(&x).unwrap_err();
                    assert_eq!((e.at, &e.cause), (o, &ReadCause::NonFinite), "{w} at {o}");
                }
                hit += 1;
            }
        }
        assert!(hit > 30, "{hit}");
    }
}

#[test]
fn intervals_swapped_do_not_build() {
    // Every interval of every edge and use: min and max exchanged.
    for kind in [Kind::Block, Kind::Cylinder, Kind::Cone] {
        let text = write_text(&body(kind, 1, 1)).unwrap();
        let d = Doc::new(&text);
        let mut swapped = 0;
        for (i, line) in d.0.iter().enumerate() {
            // Both `false`-terminated lines (uses and edges) end their geometry with
            // `lo hi` before the flag or the ends: find the interval by the line's shape.
            let at = if line[0] == "live" && line.get(1).is_some_and(|w| w == "curve") {
                line.iter().position(|w| w == "false").map(|f| f - 4)
            } else if line.len() > 4
                && line[0].chars().all(|c| c.is_ascii_digit())
                && line[2] == "pcurve"
            {
                line.iter().position(|w| w == "false").map(|f| f - 2)
            } else {
                None
            };
            let Some(at) = at else { continue };
            let (lo, hi) = (line[at].clone(), line[at + 1].clone());
            if lo.parse::<f64>().unwrap() >= hi.parse::<f64>().unwrap() {
                continue;
            }
            let mut x = d.clone();
            x.0[i][at] = hi;
            x.0[i][at + 1] = lo;
            let e = refuses(&x.text());
            assert_eq!(
                (e.at, &e.cause, e.kind()),
                (d.offset(i, at), &ReadCause::BadInterval, ErrorKind::Refused),
                "{} line {i}",
                kind.name()
            );
            swapped += 1;
        }
        assert!(swapped >= 5, "{} {swapped}", kind.name());
    }
}

#[test]
fn the_header_and_the_frame() {
    let b = body(Kind::Cylinder, 1, 1);
    let bytes = encode(&b).unwrap();
    let text = write_text(&b).unwrap();
    // Nothing, and a prefix of the magic.
    let e = dec(&[]).unwrap_err();
    assert_eq!((e.at, &e.cause), (0, &ReadCause::Truncated));
    for n in 1..4 {
        assert_eq!(dec(&bytes[..n]).unwrap_err().cause, ReadCause::Truncated);
    }
    assert_eq!(refuses("").cause, ReadCause::Truncated);
    assert_eq!(refuses("SCXB").cause, ReadCause::Truncated);
    // Versions: 0 is no writer's; 2 and above are a newer writer's.
    for (v, kind) in [
        (0u16, ErrorKind::Refused),
        (2, ErrorKind::Unsupported),
        (255, ErrorKind::Unsupported),
        (256, ErrorKind::Unsupported),
        (u16::MAX, ErrorKind::Unsupported),
    ] {
        let mut x = bytes.clone();
        x[4..6].copy_from_slice(&v.to_le_bytes());
        let e = dec(&x).unwrap_err();
        assert_eq!(
            (e.at, &e.cause, e.kind()),
            (4, &ReadCause::BadVersion { found: v }, kind)
        );
        let e = refuses(&text.replacen("SCXB 1", &format!("SCXB {v}"), 1));
        assert_eq!(
            (e.at, &e.cause, e.kind()),
            (5, &ReadCause::BadVersion { found: v }, kind)
        );
    }
    // Another record's magic, in each case, and the magic in the wrong case.
    for magic in ["SCXC", "scxb", "SCXG", "XXXX"] {
        let mut x = bytes.clone();
        x[..4].copy_from_slice(magic.as_bytes());
        assert_eq!(dec(&x).unwrap_err().cause, ReadCause::BadMagic);
        assert_eq!(
            refuses(&text.replacen("SCXB", magic, 1)).cause,
            ReadCause::BadMagic
        );
    }
    // A plane's x axis that is no unit vector: the frame refuses it, with the frame's cause.
    let d = Doc::new(&text);
    let plane = d.find("live", "cylinder");
    let e = refuses(&d.set(plane, 5, "2"));
    assert!(matches!(e.cause, ReadCause::Frame(_)), "{e:?}");
    assert_eq!(e.kind(), ErrorKind::Refused);
    assert!(std::error::Error::source(&e.cause).is_some());
}

#[test]
fn tags_that_name_nothing() {
    let b = body(Kind::Cylinder, 1, 1);
    let bytes = encode(&b).unwrap();
    // The frame bytes after the magic and version: the region table's count (u32), then
    // slot tag, then the region's bool.
    assert_eq!(&bytes[6..10], 1u32.to_le_bytes());
    for (at, record) in [(10, "slot"), (11, "bool")] {
        for found in [2u8, 3, 7, 0x80, 0xFF] {
            let mut x = bytes.clone();
            x[at] = found;
            let e = dec(&x).unwrap_err();
            assert_eq!(
                (e.at, &e.cause, e.kind()),
                (at, &ReadCause::BadTag { record, found }, ErrorKind::Refused)
            );
        }
    }
    // In text a word that is no tag is the enum's own.
    let text = write_text(&b).unwrap();
    let d = Doc::new(&text);
    let e = refuses(&d.set(1, 0, "alive"));
    assert_eq!(
        (e.at, &e.cause),
        (d.offset(1, 0), &ReadCause::BadToken { expected: "slot" })
    );
    let e = refuses(&d.set(1, 1, "TRUE"));
    assert_eq!(
        (e.at, &e.cause),
        (d.offset(1, 1), &ReadCause::BadToken { expected: "bool" })
    );
}

/// Counts: a list or table claiming what the input cannot hold is refused before anything
/// is allocated, whatever the count.
#[test]
fn counts_past_the_input_are_refused_cheaply() {
    let b = body(Kind::Block, 1, 1);
    let bytes = encode(&b).unwrap();
    // The region table's count, then each as the count of the next table: u32::MAX and
    // one over the limit are `TooMany`, a million is under the limit and runs out.
    for count in [u32::MAX, 1 << 20] {
        let mut x = bytes.clone();
        x[6..10].copy_from_slice(&count.to_le_bytes());
        let e = dec(&x).unwrap_err();
        assert_eq!(
            e.cause,
            ReadCause::TooMany {
                count,
                limit: (1 << 20) - 1
            }
        );
        assert_eq!((e.at, e.kind()), (6, ErrorKind::Exhausted));
        let d = Doc::new(&write_text(&b).unwrap());
        let e = refuses(&d.set(0, 2, &count.to_string()));
        assert_eq!(
            e.cause,
            ReadCause::TooMany {
                count,
                limit: (1 << 20) - 1
            }
        );
    }
    // A count under the limit is read as that many slots, so the input runs out or the next
    // table's bytes are not slots.
    for count in [2u32, 5, 1 << 19] {
        let mut x = bytes.clone();
        x[6..10].copy_from_slice(&count.to_le_bytes());
        let e = dec(&x).unwrap_err();
        assert!(
            matches!(e.cause, ReadCause::Truncated | ReadCause::BadTag { .. }),
            "{count}: {e:?}"
        );
    }
    // Every u32 position of the record at u32::MAX: no panic, no allocation to speak of,
    // and an error that is the count's.
    let mut st = Stats::default();
    for o in 0..bytes.len() - 4 {
        let mut x = bytes.clone();
        x[o..o + 4].copy_from_slice(&u32::MAX.to_le_bytes());
        probe(&x, &mut st, &|| format!("u32::MAX at {o}"));
    }
    for name in ["too_many", "truncated"] {
        assert!(st.refused.get(name).copied().unwrap_or(0) > 10, "{name}");
    }
    // A limit holds the table, not the record: an exact fit decodes.
    assert!(decode(&bytes, &doc(), Limits::new(12)).is_ok());
    assert_eq!(
        decode(&bytes, &doc(), Limits::new(11)).unwrap_err().kind(),
        ErrorKind::Exhausted
    );
}

#[test]
fn runs_of_deleted_slots() {
    let text = write_text(&gapped(0)).unwrap();
    let d = Doc::new(&text);
    let gaps: Vec<usize> = (0..d.0.len()).filter(|&i| d.0[i][0] == "gap").collect();
    assert!(gaps.len() >= 5);
    let first = gaps[0];
    // A run of nothing, a run past the table, two runs in a row, a run of a billion.
    assert_eq!(refuses(&d.set(first, 1, "0")).cause, ReadCause::BadRun);
    assert_eq!(refuses(&d.set(first, 1, "999")).cause, ReadCause::BadRun);
    assert_eq!(
        refuses(&d.set(first, 1, "1000000000")).cause,
        ReadCause::BadRun
    );
    // A gap split in two is two runs in a row, whatever the sum: the other spelling is
    // refused, and the first is the only one.
    let long = *gaps.iter().find(|&&g| d.0[g][1] != "1").unwrap();
    let n: u32 = d.0[long][1].parse().unwrap();
    let mut split = d.clone();
    // The next table's count, if the run ends a table, rides on the run's line.
    let tail = split.0[long].split_off(2);
    split.0[long][1] = "1".into();
    let mut rest = vec!["gap".to_string(), (n - 1).to_string()];
    rest.extend(tail);
    split.0.insert(long + 1, rest);
    assert_eq!(refuses(&split.text()).cause, ReadCause::BadRun);
}

#[test]
fn accuracies_and_certificates() {
    let text = write_text(&gapped(0)).unwrap();
    let d = Doc::new(&text);
    // Every `true <mm> <basis>` is a recorded accuracy.
    let mut found = BTreeMap::new();
    for (i, line) in d.0.iter().enumerate() {
        for j in 0..line.len().saturating_sub(2) {
            if line[j] == "true"
                && line[j + 1].parse::<f64>().is_ok()
                && ["measured", "stated", "certified", "unproven"].contains(&line[j + 2].as_str())
            {
                found.entry(line[j + 2].clone()).or_insert((i, j));
            }
        }
    }
    assert_eq!(found.len(), 4, "{found:?}");
    for (basis, &(i, j)) in &found {
        // A negative accuracy; minus zero is a second spelling of zero.
        let e = refuses(&d.set(i, j + 1, "-0.0000001"));
        assert_eq!(
            (e.at, &e.cause, e.kind()),
            (
                d.offset(i, j + 1),
                &ReadCause::BadAccuracy,
                ErrorKind::Refused
            ),
            "{basis}"
        );
        let e = refuses(&d.set(i, j + 1, "-0"));
        assert_eq!(
            (e.at, &e.cause),
            (
                d.offset(i, j + 1),
                &ReadCause::NonCanonical { record: "accuracy" }
            ),
            "{basis}"
        );
        // Zero is an accuracy, and a plain one.
        probe_text(&d.set(i, j + 1, "0"), &mut Stats::default(), &|| {
            basis.clone()
        });
        // An unknown basis.
        let e = refuses(&d.set(i, j + 2, "proven"));
        assert_eq!(e.cause, ReadCause::BadToken { expected: "basis" });
    }
    // A certificate: its claim, method and its read keys, five of them.
    let (i, j) = found["certified"];
    let line = &d.0[i];
    let claim = j + 3;
    assert!(
        [
            "pcurve_on_surface",
            "curve_to_intersection",
            "outward",
            "ends_at_start",
            "ends_at_end",
            "net_to_definition"
        ]
        .contains(&line[claim].as_str()),
        "{line:?}"
    );
    let e = refuses(&d.set(i, claim, "inward"));
    assert_eq!(e.cause, ReadCause::BadToken { expected: "claim" });
    let e = refuses(&d.set(i, claim + 2, "guess"));
    assert_eq!(e.cause, ReadCause::BadToken { expected: "method" });
    let count = claim + 3;
    for (n, name) in [
        (5u32, "bad_reads"),
        (999, "bad_reads"),
        (u32::MAX, "bad_reads"),
    ] {
        let e = refuses(&d.set(i, count, &n.to_string()));
        assert_eq!(e.name(), name, "{n}: {e:?}");
        assert_eq!(e.cause, ReadCause::BadReads { count: n });
    }
}

/// Spellings text has and bytes do not: each number has one, each line one ending, and
/// the record is the whole input.
#[test]
fn text_has_one_spelling() {
    let b = body(Kind::Block, 1, 1);
    let text = write_text(&b).unwrap();
    let d = Doc::new(&text);
    let v = l_vertex(&d);
    let x = d.0[v][1].clone();
    let n: f64 = x.parse().unwrap();
    for (what, bad) in [
        ("trailing zero", format!("{x}0")),
        ("exponent", format!("{n:e}")),
        ("plus", format!("+{x}")),
        ("leading zero", format!("0{x}")),
        ("hex", "0x1".into()),
        ("empty", String::new()),
        ("comma", x.replace('.', ",")),
        ("dots", format!("{x}.")),
    ] {
        let e = refuses(&d.set(v, 1, &bad));
        assert!(
            matches!(e.cause, ReadCause::BadToken { .. }),
            "{what} {bad}: {e:?}"
        );
    }
    // A count with a leading zero, a sign, and past u32.
    for bad in ["01", "+1", "-1", "4294967296", "1e1", "1.0"] {
        let e = refuses(&d.set(0, 2, bad));
        assert_eq!(e.cause, ReadCause::BadToken { expected: "u32" }, "{bad}");
    }
    // Endings and separators.
    assert_eq!(
        refuses(text.trim_end_matches('\n')).cause,
        ReadCause::Truncated
    );
    assert_eq!(refuses(&format!("{text}\n")).cause, ReadCause::Trailing);
    assert_eq!(refuses(&format!("{text} ")).cause, ReadCause::Trailing);
    assert_eq!(refuses(&format!("{text}x")).cause, ReadCause::Trailing);
    assert!(refuses(&text.replace('\n', "\r\n")).kind() == ErrorKind::Refused);
    assert!(matches!(
        refuses(&text.replacen(' ', "  ", 1)).cause,
        ReadCause::BadToken { .. }
    ));
    assert_eq!(
        refuses(&text.replacen(' ', "\t", 1)).cause,
        ReadCause::BadMagic
    );
    assert!(matches!(
        refuses(&text.replacen(' ', "\n", 1)).cause,
        ReadCause::BadToken { .. }
    ));
    // Binary bytes are not text, and no non-ASCII character is a word.
    assert!(refuses(&text.replacen("live", "l\u{ef}ve", 1)).kind() == ErrorKind::Refused);
    assert!(refuses("\u{feff}SCXB 1 1\n").kind() == ErrorKind::Refused);
    assert!(refuses(&"\u{1F600}".repeat(50)).kind() == ErrorKind::Refused);
}

fn l_vertex(d: &Doc) -> usize {
    d.0.iter()
        .position(|l| l.len() == 5 && l[0] == "live" && l[4] == "false")
        .unwrap()
}

/// Records deleted and doubled, the table's count kept or kept in step.
#[test]
fn records_deleted_and_doubled() {
    let l = located(Kind::Block, 1);
    let d = &l.doc;
    // Where each table's count sits: the last word of the line before its first record.
    let counts: Vec<(usize, usize)> = (0..d.0.len())
        .filter(|&i| d.0[i][0] != "live" || d.0[i].len() > 2)
        .filter(|&i| d.0[i].last().unwrap().chars().all(|c| c.is_ascii_digit()))
        .filter_map(|i| {
            let w = d.0[i].len() - 1;
            // A table count is followed by a `live` line.
            d.0.get(i + 1)
                .is_some_and(|n| n[0] == "live")
                .then_some((i, w))
        })
        .collect();
    // The edge table: the count, the line, and its first line.
    let edges = l.edge;
    let (ci, cw) = *counts.iter().rfind(|(i, _)| *i < edges).unwrap();
    assert_eq!(d.0[ci][cw], "12");
    // Deleted, the count kept: the table runs into the next table's count.
    let mut gone = d.clone();
    gone.0.remove(edges);
    assert!(matches!(
        refuses(&gone.text()).cause,
        ReadCause::BadToken { .. } | ReadCause::Truncated
    ));
    // Deleted, the count in step: the ids move up, and what named the last edge dangles.
    gone.0[ci][cw] = "11".into();
    let e = refuses(&gone.text());
    assert!(matches!(e.cause, ReadCause::Invalid(_)), "{e:?}");
    // Doubled, the count in step: a 13th edge, and the fins of the doubled edge are the
    // first's: the faces have no use for them.
    let mut twice = d.clone();
    twice.0.insert(edges, d.0[edges].clone());
    twice.0[ci][cw] = "13".into();
    assert!(matches!(
        invalid(dec_text(&twice.text())),
        FinishCause::Structure(_)
    ));
    // Vertices: the last deleted, the count in step: an end names a vertex that is gone.
    let vtx = (0..d.0.len()).rfind(|&i| d.0[i][0] == "live").unwrap();
    let mut fewer = d.clone();
    fewer.0.remove(vtx);
    let edge_line = (0..d.0.len())
        .rfind(|&i| d.0[i].get(1).is_some_and(|w| w == "curve"))
        .unwrap();
    let last = fewer.0[edge_line].len() - 1;
    assert_eq!(fewer.0[edge_line][last], "8");
    fewer.0[edge_line][last] = "7".into();
    assert!(matches!(
        invalid(dec_text(&fewer.text())),
        FinishCause::Structure(StructureCause::Dangling { .. })
    ));
    // A vertex doubled: a 9th vertex nothing meets.
    let mut more = d.clone();
    more.0.push(d.0[vtx].clone());
    more.0[edge_line][last] = "9".into();
    match dec_text(&more.text()) {
        Ok(b) => assert_eq!(write_text(&b).unwrap(), more.text()),
        Err(e) => assert!(matches!(e.cause, ReadCause::Invalid(_)), "{e:?}"),
    }
}

// ---- sweeps ------------------------------------------------------------------------------

/// A small body of each shape the record has: planes and lines; circles, angle lines and
/// cylinders; poles; inner loops; all the recorded accuracies, certificates and gaps.
fn small() -> Vec<(String, Brep)> {
    vec![
        ("block".into(), body(Kind::Block, 1, 1)),
        ("sphere".into(), body(Kind::Sphere, 3, 1)),
        ("cylinder".into(), body(Kind::Cylinder, 2, 1)),
        ("gapped".into(), gapped(0)),
    ]
}

const BYTE_VALUES: [u8; 3] = [0x00, 0xFF, 0x80];
const WORD_VALUES: [u32; 5] = [0, 1, 999, u32::MAX, 0x8000_0000];

/// Each byte set to a few values, flipped, deleted, doubled, and every prefix.
fn sweep_bytes(name: &str, bytes: &[u8], values: &[u8], st: &mut Stats) {
    for len in 0..bytes.len() {
        let e = probe(&bytes[..len], st, &|| format!("{name} prefix {len}")).unwrap();
        assert_eq!(e.cause, ReadCause::Truncated, "{name} prefix {len}");
    }
    for o in 0..bytes.len() {
        for &v in values.iter().chain(&[bytes[o] ^ 1]) {
            if v == bytes[o] {
                continue;
            }
            let mut x = bytes.to_vec();
            x[o] = v;
            probe(&x, st, &|| format!("{name} byte {o} = {v:#x}"));
        }
        let mut x = bytes.to_vec();
        x.remove(o);
        probe(&x, st, &|| format!("{name} byte {o} deleted"));
        let mut x = bytes.to_vec();
        x.insert(o, bytes[o]);
        probe(&x, st, &|| format!("{name} byte {o} doubled"));
    }
}

/// Each position's four bytes set to an id or a count.
fn sweep_words(name: &str, bytes: &[u8], values: &[u32], st: &mut Stats) {
    for o in 0..bytes.len().saturating_sub(3) {
        for &v in values {
            let mut x = bytes.to_vec();
            x[o..o + 4].copy_from_slice(&v.to_le_bytes());
            probe(&x, st, &|| format!("{name} word {o} = {v}"));
        }
    }
}

const FLOATS: [f64; 10] = [
    f64::NAN,
    f64::INFINITY,
    -0.0,
    5e-324,
    f64::MIN_POSITIVE,
    1e-300,
    1e300,
    f64::MAX,
    -f64::MAX,
    9.223_372_036_854_776e18,
];

/// The offsets in `bytes` of the floats `text` (its twin) spells with a point: found by value,
/// so a few may be other bytes that happen to match.
fn float_offsets(bytes: &[u8], text: &str) -> Vec<usize> {
    let mut seen = BTreeSet::new();
    let mut out = BTreeSet::new();
    for w in text.split([' ', '\n']) {
        if !(w.contains('.') || w == "-0") || !seen.insert(w.to_string()) {
            continue;
        }
        let Ok(v) = w.parse::<f64>() else { continue };
        let pat = v.to_le_bytes();
        out.extend((0..=bytes.len() - 8).filter(|&o| bytes[o..o + 8] == pat));
    }
    out.into_iter().collect()
}

/// Each float field set to each awkward double, found by value in the binary, and as a
/// word in the text.
fn sweep_floats(name: &str, b: &Brep, floats: &[f64], st: &mut Stats) {
    let bytes = encode(b).unwrap();
    let text = write_text(b).unwrap();
    let mut seen = BTreeSet::new();
    for w in text.split([' ', '\n']) {
        if !(w.contains('.') || w.contains('e') || w == "-0") || !seen.insert(w.to_string()) {
            continue;
        }
        let Ok(v) = w.parse::<f64>() else { continue };
        let pat = v.to_le_bytes();
        for o in (0..=bytes.len() - 8).filter(|&o| bytes[o..o + 8] == pat) {
            for &f in floats {
                let mut x = bytes.clone();
                x[o..o + 8].copy_from_slice(&f.to_le_bytes());
                probe(&x, st, &|| format!("{name} float at {o} = {f:e}"));
            }
        }
    }
    // The text: every token that is a float, each one spelled each way.
    let d = Doc::new(&text);
    for (i, line) in d.0.iter().enumerate() {
        for (j, w) in line.iter().enumerate() {
            if !(w.contains('.') || w.contains('e') || w == "-0") || w.parse::<f64>().is_err() {
                continue;
            }
            for &f in floats {
                let t = d.set(i, j, &format!("{f}"));
                probe_text(&t, st, &|| format!("{name} text {i}:{j} = {f:e}"));
            }
        }
    }
}

const WORDS: [&str; 17] = [
    "",
    "0",
    "1",
    "2",
    "4294967295",
    "-1",
    "NaN",
    "inf",
    "-0",
    "5e-324",
    "true",
    "false",
    "live",
    "gap",
    "forward",
    "pcurve",
    "zz",
];

/// Each token replaced from the dictionary, deleted, swapped with its neighbour; each line
/// deleted and doubled; every prefix.
fn sweep_text(name: &str, text: &str, words: &[&str], st: &mut Stats) {
    let d = Doc::new(text);
    for (i, line) in d.0.iter().enumerate() {
        for j in 0..line.len() {
            for &w in words {
                if w == line[j] {
                    continue;
                }
                let mut x = d.clone();
                x.0[i][j] = w.into();
                probe_text(&x.text(), st, &|| format!("{name} {i}:{j} = {w:?}"));
            }
            let mut x = d.clone();
            x.0[i].remove(j);
            probe_text(&x.text(), st, &|| format!("{name} {i}:{j} deleted"));
            if j + 1 < line.len() {
                let mut x = d.clone();
                x.0[i].swap(j, j + 1);
                probe_text(&x.text(), st, &|| format!("{name} {i}:{j} swapped"));
            }
        }
        let mut x = d.clone();
        x.0.remove(i);
        probe_text(&x.text(), st, &|| format!("{name} line {i} deleted"));
        let mut x = d.clone();
        x.0.insert(i, line.clone());
        probe_text(&x.text(), st, &|| format!("{name} line {i} doubled"));
    }
    for len in (0..text.len()).filter(|&n| text.is_char_boundary(n)) {
        probe_text(&text[..len], st, &|| format!("{name} prefix {len}"));
    }
}

fn the_causes_a_sweep_must_reach(st: &Stats, what: &str) {
    for name in [
        "truncated",
        "bad_tag",
        "non_finite",
        "bad_run",
        "too_many",
        "bad_interval",
        "invalid",
        "trailing",
    ] {
        assert!(
            st.refused.contains_key(name),
            "{what}: no {name}: {:?}",
            st.refused
        );
    }
    assert!(st.decoded > 0, "{what}");
}

#[test]
fn small_bodies_survive_the_sweeps() {
    let mut all = Stats::default();
    for (name, b) in small() {
        let mut st = Stats::default();
        // The normal tier takes the smaller bodies' bytes whole, and the words and floats
        // of every one.
        let bytes = encode(&b).unwrap();
        if bytes.len() < 2000 || name == "gapped" {
            sweep_bytes(&name, &bytes, &BYTE_VALUES[..1], &mut st);
        }
        sweep_words(&name, &bytes, &WORD_VALUES[2..4], &mut st);
        sweep_floats(&name, &b, &FLOATS, &mut st);
        all.merge(st);
    }
    eprintln!(
        "binary sweeps: {} changed inputs, {} decoded",
        all.tried, all.decoded
    );
    the_causes_a_sweep_must_reach(&all, "binary");
    // The enums the bytes reached: every tag the record has.
    for tag in [
        "slot",
        "bool",
        "sense",
        "edge_shape",
        "use_geom",
        "basis",
        "claim",
        "method",
        "surface",
        "curve",
        "curve2",
    ] {
        assert!(all.tags.contains(tag), "{tag}: {:?}", all.tags);
    }
}

#[test]
fn small_bodies_text_survives_the_sweeps() {
    let mut all = Stats::default();
    for (name, b) in small() {
        let text = write_text(&b).unwrap();
        if text.len() < 3000 || name == "gapped" {
            sweep_text(&name, &text, &WORDS[..5], &mut all);
        }
    }
    for name in [
        "bad_token",
        "truncated",
        "bad_run",
        "trailing",
        "invalid",
        "too_many",
    ] {
        assert!(
            all.refused.contains_key(name),
            "no {name}: {:?}",
            all.refused
        );
    }
    for tag in [
        "u32",
        "f64",
        "slot",
        "bool",
        "sense",
        "separator",
        "edge_shape",
        "use_geom",
        "surface",
    ] {
        assert!(all.tags.contains(tag), "{tag}: {:?}", all.tags);
    }
}

/// The slow sweep: every value, every body of the small set and one of each kind.
#[test]
#[ignore = "slow tier: every sweep value over every small body; cargo test -p scx-record --test corrupt -- --ignored"]
fn the_sweeps_wide() {
    let mut all = Stats::default();
    let mut bodies = small();
    for kind in Kind::ALL {
        bodies.push((kind.name().to_string(), body(kind, 7, 1)));
    }
    for (name, b) in bodies {
        let bytes = encode(&b).unwrap();
        let text = write_text(&b).unwrap();
        if bytes.len() < 12_000 {
            sweep_bytes(&name, &bytes, &[0x00, 0x01, 0x7F, 0x80, 0xFF], &mut all);
            sweep_words(&name, &bytes, &WORD_VALUES, &mut all);
            sweep_text(&name, &text, &WORDS, &mut all);
        }
        sweep_floats(&name, &b, &FLOATS, &mut all);
    }
    eprintln!(
        "wide sweeps: {} changed inputs, {} decoded",
        all.tried, all.decoded
    );
    the_causes_a_sweep_must_reach(&all, "wide");
}

// ---- seeded fuzz of stacked changes ------------------------------------------------------

fn fuzz(iterations: usize, seed: u64, kinds: &[Kind]) -> Stats {
    let mut rng = Rng::new(seed);
    let mut st = Stats::default();
    for &kind in kinds {
        let b = body(kind, rng.next_u64() % 8, 1);
        let bytes = encode(&b).unwrap();
        let text = write_text(&b).unwrap();
        let floats = float_offsets(&bytes, &text);
        for it in 0..iterations {
            // One to four stacked changes.
            let n = 1 + (rng.next_u64() % 4) as usize;
            let mut x = bytes.clone();
            let mut log = vec![];
            for _ in 0..n {
                if x.len() < 16 {
                    break;
                }
                let o = (rng.next_u64() % x.len() as u64) as usize;
                match rng.next_u64() % 8 {
                    0 => x[o] = rng.next_u64() as u8,
                    1 => x[o] ^= 1 << (rng.next_u64() % 8),
                    2 => {
                        x.remove(o);
                    }
                    3 => x.insert(o, rng.next_u64() as u8),
                    4 => {
                        let end = (o + 1 + (rng.next_u64() % 24) as usize).min(x.len());
                        let chunk = x[o..end].to_vec();
                        x.splice(o..o, chunk);
                    }
                    5 => x.truncate(o.max(1)),
                    6 => {
                        let o = o.min(x.len().saturating_sub(4));
                        let v = WORD_VALUES[(rng.next_u64() % 5) as usize];
                        x[o..o + 4].copy_from_slice(&v.to_le_bytes());
                    }
                    _ => {
                        // A float field, and often the field after it (an interval's other
                        // end, the next coordinate), as the same body's offsets say.
                        let Some(&o) =
                            floats.get((rng.next_u64() % floats.len().max(1) as u64) as usize)
                        else {
                            continue;
                        };
                        for k in 0..1 + (rng.next_u64() % 3) as usize {
                            let f = FLOATS[(rng.next_u64() % FLOATS.len() as u64) as usize];
                            if let Some(slot) = x.get_mut(o + 8 * k..o + 8 * k + 8) {
                                slot.copy_from_slice(&f.to_le_bytes());
                            }
                        }
                    }
                }
                log.push((o, x.len()));
                if x.is_empty() {
                    break;
                }
            }
            probe(&x, &mut st, &|| {
                format!("{} fuzz {seed} #{it} {log:?}", kind.name())
            });

            let d = Doc::new(&text);
            let mut t = d.clone();
            for _ in 0..n {
                let i = (rng.next_u64() % t.0.len() as u64) as usize;
                if t.0[i].is_empty() {
                    continue;
                }
                let j = (rng.next_u64() % t.0[i].len() as u64) as usize;
                match rng.next_u64() % 5 {
                    0 | 1 => {
                        t.0[i][j] = WORDS[(rng.next_u64() % WORDS.len() as u64) as usize].into()
                    }
                    2 => {
                        t.0[i].remove(j);
                    }
                    3 => {
                        let w = t.0[i][j].clone();
                        t.0[i].insert(j, w);
                    }
                    _ => {
                        let line = t.0[i].clone();
                        t.0.insert(i, line);
                    }
                }
            }
            probe_text(&t.text(), &mut st, &|| {
                format!("{} text fuzz {seed} #{it}", kind.name())
            });
        }
    }
    st
}

#[test]
fn seeded_fuzz_of_stacked_changes() {
    let st = fuzz(120, 0x005C_0004, &Kind::ALL);
    eprintln!("fuzz: {} changed inputs, {} decoded", st.tried, st.decoded);
    assert!(
        st.tried >= 3000 && st.decoded > 0,
        "{} {}",
        st.tried,
        st.decoded
    );
}

#[test]
#[ignore = "slow tier: 20,000 stacked changes per kind; cargo test -p scx-record --test corrupt -- --ignored"]
fn seeded_fuzz_wide() {
    for seed in 0..4 {
        let st = fuzz(2500, 0x005C_1000 + seed, &Kind::ALL);
        assert!(st.decoded > 0);
    }
}
