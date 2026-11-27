// Written for solidcorex. Apache-2.0.
//! The body record (research 63 RE3): every testkit body kind round-trips to the bit in
//! binary and in text, ids and deleted slots, recorded accuracies and the radial order
//! kept; binary → text → binary gives the same bytes; a body `finish` refuses is refused
//! with its finish cause; and truncations and single-byte changes never panic, and decode
//! only to a body whose record is the changed input. The full suite is RE5's.

#![allow(
    missing_docs,
    clippy::pedantic,
    clippy::indexing_slicing,
    clippy::panic,
    clippy::unwrap_used
)]

use std::sync::Arc;

use scx_brep::{
    Brep, Document, EdgeId, EntityKind, FaceId, FinishCause, GeometryCause, RegionId, ShellId,
    StructureCause, VertexId, VertexRecord,
};
use scx_num::cert::{Basis, Cert, Claim, EdgeEnd, Method, Reads, Recorded};
use scx_num::error::ErrorKind;
use scx_num::hash::Rng;
use scx_num::units::{Accuracy, Length};
use scx_num::vector3d::Point3;
use scx_record::{Limits, ReadCause, ReadError, decode, encode, read_text, write_text};
use scx_testkit::bodies::{Kind, generate};

fn doc() -> Document {
    Document::default()
}

fn body(kind: Kind, seed: u64, size: usize) -> Brep {
    let g = generate(kind, seed, size).unwrap();
    g.draft.finish(&doc()).unwrap().0
}

fn dec(bytes: &[u8]) -> Result<Brep, ReadError> {
    decode(bytes, &doc(), Limits::default())
}

fn dec_text(text: &str) -> Result<Brep, ReadError> {
    read_text(text, &doc(), Limits::default())
}

/// Every table slot for slot, each record field by its `Debug` text, which prints each
/// `f64` as its shortest round-trip spelling: equal text is equal bits, `-0` and `0` apart.
/// Cells are left out: they are caches, rebuilt on demand.
fn slots(b: &Brep) -> Vec<String> {
    let mut out = vec![format!(
        "{} {} {} {} {} {:?}",
        b.region_slots(),
        b.shell_slots(),
        b.face_slots(),
        b.edge_slots(),
        b.vertex_slots(),
        b.resolution()
    )];
    for i in 0..b.region_slots() as u32 {
        out.push(format!(
            "{:?}",
            b.region(RegionId::from_raw(i)).map(|r| r.record())
        ));
    }
    for i in 0..b.shell_slots() as u32 {
        out.push(format!(
            "{:?}",
            b.shell(ShellId::from_raw(i)).map(|s| s.record())
        ));
    }
    for i in 0..b.face_slots() as u32 {
        out.push(format!(
            "{:?}",
            b.face(FaceId::from_raw(i)).map(|f| (
                &f.record().surface,
                f.record().sides,
                &f.record().loops
            ))
        ));
    }
    for i in 0..b.edge_slots() as u32 {
        out.push(format!(
            "{:?}",
            b.edge(EdgeId::from_raw(i)).map(|e| {
                let g = e.geometry();
                (&g.shape, g.ends, &g.accuracy, &e.adjacency().fins)
            })
        ));
    }
    for i in 0..b.vertex_slots() as u32 {
        out.push(format!(
            "{:?}",
            b.vertex(VertexId::from_raw(i)).map(|v| v.record())
        ));
    }
    out
}

/// The body back to the bit in both encodings, each record its own fixed point, and
/// binary → text → binary the same bytes.
fn round_trip(b: &Brep, what: &str) {
    let bytes = encode(b).unwrap();
    let back = dec(&bytes).unwrap_or_else(|e| panic!("{what}: binary {e:?}"));
    assert_eq!(slots(&back), slots(b), "{what}: binary");
    assert_eq!(encode(&back).unwrap(), bytes, "{what}: binary re-encode");
    let text = write_text(b).unwrap();
    let back = dec_text(&text).unwrap_or_else(|e| panic!("{what}: text {e:?}"));
    assert_eq!(slots(&back), slots(b), "{what}: text");
    assert_eq!(write_text(&back).unwrap(), text, "{what}: text re-encode");
    let via = dec_text(&write_text(&dec(&bytes).unwrap()).unwrap()).unwrap();
    assert_eq!(
        encode(&via).unwrap(),
        bytes,
        "{what}: binary -> text -> binary"
    );
}

#[test]
fn every_kind_round_trips_to_the_bit() {
    for kind in Kind::ALL {
        for seed in 0..3 {
            for size in 1..=2 {
                round_trip(
                    &body(kind, seed, size),
                    &format!("{} {seed} {size}", kind.name()),
                );
            }
        }
    }
}

fn recorded(acc: f64, basis: Basis) -> Option<Arc<Recorded>> {
    Some(Arc::new(Recorded {
        acc: Accuracy::new(Length::from_mm(acc)).unwrap(),
        basis,
    }))
}

fn cert(claim: Claim, method: Method, keys: &[u64]) -> Cert {
    Cert {
        claim,
        bound: Length::from_mm(3.5e-8),
        method,
        reads: Reads::new(keys).unwrap(),
    }
}

/// A cylinder edited to hold deleted slots in every table (inside and at the end), a lone
/// vertex past a gap, and a recorded accuracy of every basis, claim and method on its
/// vertices, edges and uses.
fn gapped(seed: u64) -> Brep {
    let base = body(Kind::Cylinder, seed, 3);
    let mut d = base.edit();
    let r = d.add_region(true).unwrap();
    d.remove_region(r).unwrap();
    for kind in [
        EntityKind::Region,
        EntityKind::Shell,
        EntityKind::Face,
        EntityKind::Edge,
        EntityKind::Vertex,
    ] {
        d.skip(kind, 2).unwrap();
    }
    let lone = d
        .add_vertex(VertexRecord::new(Point3::new(1e3, -0.0, 2.5)))
        .unwrap();
    let mut alone = scx_brep::ShellRecord::new(RegionId::OUTSIDE);
    alone.lone_vertices.push(lone);
    d.add_shell(alone).unwrap();
    d.skip(EntityKind::Vertex, 3).unwrap();
    let s = d.add_shell(scx_brep::ShellRecord::new(RegionId::OUTSIDE));
    d.remove_shell(s.unwrap()).unwrap();

    let bases = [
        Basis::Measured,
        Basis::Stated,
        Basis::Certified(cert(Claim::PcurveOnSurface, Method::Interval, &[])),
        Basis::Certified(cert(Claim::CurveToIntersection, Method::Bernstein, &[1])),
        Basis::Unproven(cert(Claim::NetToDefinition, Method::Taylor, &[1, 2])),
        Basis::Unproven(cert(
            Claim::EndsAtVertex(EdgeEnd::Start),
            Method::Krawczyk,
            &[7, 8, 9],
        )),
        Basis::Certified(cert(
            Claim::EndsAtVertex(EdgeEnd::End),
            Method::Interval,
            &[u64::MAX, 0, 3, 4],
        )),
        Basis::Certified(cert(Claim::Outward, Method::Taylor, &[5])),
    ];
    let edges: Vec<EdgeId> = d.edges().map(|e| e.id()).collect();
    let vertices: Vec<VertexId> = d.vertices().map(|v| v.id()).collect();
    let faces: Vec<FaceId> = d.faces().map(|f| f.id()).collect();
    for (i, &e) in edges.iter().enumerate() {
        d.edge_mut(e).unwrap().accuracy = recorded(1e-7 * (i + 1) as f64, bases[i % bases.len()]);
    }
    for (i, &v) in vertices.iter().enumerate() {
        d.vertex_mut(v).unwrap().accuracy = recorded(2e-7, bases[(i + 3) % bases.len()]);
    }
    let mut k = 0;
    for &f in &faces {
        let face = d.face_mut(f).unwrap();
        for u in face.loops.iter_mut().flat_map(|l| &mut l.uses) {
            u.bound = recorded(5e-8, bases[k % bases.len()]);
            k += 1;
        }
    }
    d.finish(&doc())
        .unwrap_or_else(|u| panic!("{:?}", u.cause))
        .0
}

#[test]
fn deleted_slots_and_recorded_accuracy_round_trip() {
    for seed in 0..3 {
        let b = gapped(seed);
        assert!(b.vertex_slots() > b.vertices().len() + 4);
        round_trip(&b, &format!("gapped {seed}"));
        // A gap in the middle of the vertex table and two at its end, as words.
        let text = write_text(&b).unwrap();
        let lone = text.find("\ngap 2\nlive 1000 -0 2.5 ").expect(&text);
        assert!(text[lone..].ends_with("\ngap 3\n"), "{text}");
    }
}

#[test]
fn the_stored_radial_order_is_the_order_read_back() {
    // A rotation of a four-fin edge's fins is the same cyclic order, and valid; the faces
    // alone would give the fins in face order, so only the stored list brings it back.
    for seed in 0..3 {
        let base = body(Kind::Touching, seed, 2);
        let mut d = base.edit();
        let fours: Vec<EdgeId> = d
            .edges()
            .filter(|e| e.fins().len() == 4)
            .map(|e| e.id())
            .collect();
        assert!(!fours.is_empty());
        for (i, &e) in fours.iter().enumerate() {
            d.adjacency_mut(e).unwrap().fins.rotate_left(1 + i % 3);
        }
        let b = d.finish(&doc()).unwrap().0;
        round_trip(&b, &format!("rotated {seed}"));
        let back = dec(&encode(&b).unwrap()).unwrap();
        for &e in &fours {
            let want: Vec<_> = b.edge(e).unwrap().fins().collect();
            let got: Vec<_> = back.edge(e).unwrap().fins().collect();
            assert_eq!(got, want);
        }
    }
}

// ---- refusals ----------------------------------------------------------------------------

/// The text with line `line`'s word `word` replaced.
fn with_word(text: &str, line: usize, word: usize, new: &str) -> String {
    let mut lines: Vec<String> = text.lines().map(String::from).collect();
    let mut words: Vec<&str> = lines[line].split(' ').collect();
    words[word] = new;
    lines[line] = words.join(" ");
    lines.join("\n") + "\n"
}

/// The index of the first line starting with `prefix`.
fn line_of(text: &str, prefix: &str) -> usize {
    text.lines().position(|l| l.starts_with(prefix)).unwrap()
}

fn invalid(r: Result<Brep, ReadError>) -> FinishCause {
    match r {
        Err(ReadError {
            cause: ReadCause::Invalid(c),
            ..
        }) => *c,
        other => panic!("not invalid: {:?}", other.err()),
    }
}

#[test]
fn a_body_finish_refuses_keeps_its_cause() {
    let b = body(Kind::Block, 0, 1);
    let text = write_text(&b).unwrap();
    // An edge: `live curve line` 6 numbers, its interval, then its two ends.
    let edge = line_of(&text, "live curve line");
    let e = EdgeId::from_raw((edge - line_of(&text, "live curve")) as u32);
    let dangling = with_word(&text, edge, 11, "999");
    let err = dec_text(&dangling).unwrap_err();
    assert_eq!(err.at, dangling.len(), "{err:?}");
    assert_eq!((err.kind(), err.name()), (ErrorKind::Refused, "invalid"));
    assert!(std::error::Error::source(&err.cause).is_some());
    assert_eq!(
        invalid(Err(err)),
        FinishCause::Structure(StructureCause::Dangling {
            from: e.into(),
            to: VertexId::from_raw(999).into()
        })
    );

    // A vertex moved off its edges' ends: a geometry cause.
    let last = text.lines().count() - 1;
    let moved = with_word(&text, last, 1, "0.25");
    assert!(matches!(
        invalid(dec_text(&moved)),
        FinishCause::Geometry(GeometryCause::EdgeEndOffVertex { .. })
    ));

    // An inverted interval does not build: refused before `finish`.
    let inverted = with_word(&text, edge, 9, "1000");
    assert_eq!(
        dec_text(&inverted).unwrap_err().cause,
        ReadCause::BadInterval
    );
}

#[test]
fn fins_out_of_radial_order_are_refused_by_the_geometry() {
    let b = body(Kind::Touching, 0, 1);
    let text = write_text(&b).unwrap();
    let first = line_of(&text, "live curve");
    let (at, line) = text
        .lines()
        .enumerate()
        .find(|(_, l)| {
            let w: Vec<&str> = l.split(' ').collect();
            l.starts_with("live curve") && w[w.len() - 9] == "4"
        })
        .unwrap();
    let mut words: Vec<&str> = line.split(' ').collect();
    let n = words.len();
    // Reverse the four (face, sense) pairs.
    let fins: Vec<[&str; 2]> = (0..4)
        .map(|i| [words[n - 8 + 2 * i], words[n - 7 + 2 * i]])
        .collect();
    for (i, f) in fins.iter().rev().enumerate() {
        words[n - 8 + 2 * i] = f[0];
        words[n - 7 + 2 * i] = f[1];
    }
    let mut lines: Vec<&str> = text.lines().collect();
    let joined = words.join(" ");
    lines[at] = &joined;
    let reversed = lines.join("\n") + "\n";
    assert_eq!(
        invalid(dec_text(&reversed)),
        FinishCause::Geometry(GeometryCause::RadialOrder {
            edge: EdgeId::from_raw((at - first) as u32),
            at: 0.5
        })
    );
}

#[test]
fn the_record_frame_refuses_what_is_not_this_record() {
    let b = body(Kind::Cylinder, 1, 2);
    let bytes = encode(&b).unwrap();
    let mut other = bytes.clone();
    other[3] = b'X';
    assert_eq!(dec(&other).unwrap_err().cause, ReadCause::BadMagic);
    let mut newer = bytes.clone();
    newer[4] = 2;
    let e = dec(&newer).unwrap_err();
    assert_eq!((e.at, e.kind()), (4, ErrorKind::Unsupported));
    let mut longer = bytes.clone();
    longer.push(0);
    assert_eq!(dec(&longer).unwrap_err().cause, ReadCause::Trailing);
    // Each table capped: the faces' (4) is the first over 3 slots; 6 holds the edges.
    let e = decode(&bytes, &doc(), Limits::new(3)).unwrap_err();
    assert_eq!(e.kind(), ErrorKind::Exhausted);
    assert!(
        matches!(e.cause, ReadCause::TooMany { limit: 3, .. }),
        "{e:?}"
    );
    assert!(decode(&bytes, &doc(), Limits::new(6)).is_ok());
}

// ---- corruption --------------------------------------------------------------------------

/// Every truncation is refused inside the input; seeded single-byte changes give a body or
/// a `ReadError`, never a panic, and a body only when the changed input is its own record.
#[test]
fn corruption_never_panics_and_never_decodes_to_another_spelling() {
    let mut rng = Rng::new(0x005E_C04D);
    let (mut tried, mut decoded, mut invalid) = (0usize, 0usize, 0usize);
    for (kind, seed) in [
        (Kind::Block, 1),
        (Kind::Cylinder, 2),
        (Kind::Sphere, 3),
        (Kind::Holed, 4),
    ] {
        let b = body(kind, seed, 1);
        let bytes = encode(&b).unwrap();
        for len in 0..bytes.len() {
            let e = dec(&bytes[..len]).unwrap_err();
            assert!(e.at <= len, "{} truncated at {len}: {e:?}", kind.name());
        }
        for _ in 0..3000 {
            let mut x = bytes.clone();
            let at = (rng.next_u64() % x.len() as u64) as usize;
            let flip = 1 + (rng.next_u64() % 255) as u8;
            x[at] ^= flip;
            tried += 1;
            match dec(&x) {
                Ok(back) => {
                    decoded += 1;
                    assert_eq!(encode(&back).unwrap(), x, "{} byte {at}", kind.name());
                }
                Err(e) => {
                    assert!(e.at <= x.len());
                    invalid += usize::from(matches!(e.cause, ReadCause::Invalid(_)));
                }
            }
        }
        let text = write_text(&b).unwrap();
        for _ in 0..1000 {
            let mut x = text.clone().into_bytes();
            let at = (rng.next_u64() % x.len() as u64) as usize;
            x[at] = b" \n0123456789.-abcz"[(rng.next_u64() % 18) as usize];
            let Ok(x) = String::from_utf8(x) else {
                continue;
            };
            tried += 1;
            if let Ok(back) = dec_text(&x) {
                decoded += 1;
                assert_eq!(write_text(&back).unwrap(), x, "{} text {at}", kind.name());
            }
        }
    }
    // Most changes land in numbers: many still read, and `finish` refuses most of those.
    assert!(invalid > 0 && decoded > 0, "{tried} {decoded} {invalid}");
}
