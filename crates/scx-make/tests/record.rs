// Written for solidcorex. Apache-2.0.
//! A made body is a record (research 63 RE5): `block` at assorted frames and sizes, and a
//! made block after an edit, round trip to the bit through both encodings of `scx-record`,
//! each record its own fixed point, binary → text → binary the same bytes. A decoded block
//! is the made one: the same vertices and the same counts.

#![allow(
    clippy::unwrap_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::float_cmp
)]

use scx_brep::{Brep, Document};
use scx_make::block;
use scx_num::frame::Frame;
use scx_num::math::Affine;
use scx_num::units::Length;
use scx_num::vector3d::{Point3, Vec3};
use scx_record::{Limits, decode, encode, read_text, write_text};

fn doc() -> Document {
    Document::default()
}

fn frame(o: [f64; 3], x: [f64; 3], y: [f64; 3]) -> Frame {
    Frame::new(
        Point3::new(o[0], o[1], o[2]),
        Vec3::new(x[0], x[1], x[2]),
        Vec3::new(y[0], y[1], y[2]),
    )
    .unwrap()
}

fn made(f: &Frame, s: [f64; 3]) -> Brep {
    block(f, s.map(Length::from_mm), &doc()).unwrap().body
}

/// Both encodings keep the body: the same points, the same counts, each record its own
/// fixed point, and the two encodings the same record.
fn check(b: &Brep, what: &str) {
    let bytes = encode(b).unwrap();
    let back = decode(&bytes, &doc(), Limits::default()).unwrap();
    assert_eq!(encode(&back).unwrap(), bytes, "{what}: binary");
    let text = write_text(b).unwrap();
    let back_text = read_text(&text, &doc(), Limits::default()).unwrap();
    assert_eq!(write_text(&back_text).unwrap(), text, "{what}: text");
    assert_eq!(encode(&back_text).unwrap(), bytes, "{what}: text -> binary");
    assert_eq!(write_text(&back).unwrap(), text, "{what}: binary -> text");
    for other in [&back, &back_text] {
        assert_eq!(
            (
                other.faces().len(),
                other.edges().len(),
                other.vertices().len()
            ),
            (b.faces().len(), b.edges().len(), b.vertices().len()),
            "{what}"
        );
        for (p, q) in b.vertices().zip(other.vertices()) {
            let (p, q) = (p.point(), q.point());
            assert_eq!(
                [p.x.to_bits(), p.y.to_bits(), p.z.to_bits()],
                [q.x.to_bits(), q.y.to_bits(), q.z.to_bits()],
                "{what}"
            );
        }
    }
}

#[test]
fn blocks_round_trip() {
    let frames = [
        frame([0.0; 3], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]),
        frame([3.0, -2.0, 7.5], [1.0, 2.0, 2.0], [2.0, -1.0, 0.0]),
        frame([1e4, 1e4, 1e4], [0.0, 0.0, 1.0], [1.0, 0.0, 0.0]),
        frame([-0.0, 0.0, 0.0], [1.0, 1.0, 0.0], [-1.0, 1.0, 0.0]),
    ];
    let sizes = [
        [1.0, 1.0, 1.0],
        [3.0, 5.0, 0.75],
        [1e-3, 2e-3, 3e-3],
        [1e3, 2e3, 5e2],
        [0.1, 0.2, 0.3],
    ];
    for (i, f) in frames.iter().enumerate() {
        for (j, s) in sizes.iter().enumerate() {
            check(&made(f, *s), &format!("frame {i} size {j}"));
        }
    }
}

#[test]
fn a_made_block_edited_round_trips() {
    let f = frame([3.0, -2.0, 7.5], [1.0, 2.0, 2.0], [2.0, -1.0, 0.0]);
    let b = made(&f, [3.0, 5.0, 0.75]);
    let maps = [
        Affine::rotation(Point3::new(0.3, -1.2, 0.7), Vec3::new(1.0, 2.0, -0.5), 0.9).unwrap(),
        Affine::reflection(Point3::new(0.5, 1.0, -2.0), Vec3::new(0.3, -1.0, 0.8)).unwrap(),
        Affine::uniform_scaling(Point3::new(1.5, -0.25, 2.0), 7.0).unwrap(),
        Affine::translation(Vec3::new(1e6, 0.0, -1e6)),
    ];
    for (i, a) in maps.iter().enumerate() {
        let mut d = b.edit();
        d.transform(a, &doc()).unwrap();
        let moved = d.finish(&doc()).unwrap().0;
        check(&moved, &format!("map {i}"));
    }
}
