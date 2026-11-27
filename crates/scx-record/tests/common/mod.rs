// Written for solidcorex. Apache-2.0.
//! What the record test files share: the bodies, the two decoders, a body's slots as text,
//! and the round trip every encoding must keep.

#![allow(
    dead_code,
    missing_docs,
    clippy::pedantic,
    clippy::indexing_slicing,
    clippy::panic,
    clippy::unwrap_used
)]

use std::sync::Arc;

use scx_brep::{
    Brep, Document, EdgeId, EntityKind, FaceId, RegionId, ShellId, VertexId, VertexRecord,
};
use scx_num::cert::{Basis, Cert, Claim, EdgeEnd, Method, Reads, Recorded};
use scx_num::units::{Accuracy, Length};
use scx_num::vector3d::Point3;
use scx_record::{Limits, ReadError, decode, encode, read_text, write_text};
use scx_testkit::bodies::{Kind, generate};

pub fn doc() -> Document {
    Document::default()
}

pub fn body(kind: Kind, seed: u64, size: usize) -> Brep {
    let g = generate(kind, seed, size).unwrap();
    g.draft.finish(&doc()).unwrap().0
}

pub fn dec(bytes: &[u8]) -> Result<Brep, ReadError> {
    decode(bytes, &doc(), Limits::default())
}

pub fn dec_text(text: &str) -> Result<Brep, ReadError> {
    read_text(text, &doc(), Limits::default())
}

/// Every table slot for slot, each record field by its `Debug` text, which prints each
/// `f64` as its shortest round-trip spelling: equal text is equal bits, `-0` and `0` apart.
/// Cells are left out: they are caches, rebuilt on demand.
pub fn slots(b: &Brep) -> Vec<String> {
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
pub fn round_trip(b: &Brep, what: &str) {
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

pub fn recorded(acc: f64, basis: Basis) -> Option<Arc<Recorded>> {
    Some(Arc::new(Recorded {
        acc: Accuracy::new(Length::from_mm(acc)).unwrap(),
        basis,
    }))
}

pub fn cert(claim: Claim, method: Method, keys: &[u64]) -> Cert {
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
pub fn gapped(seed: u64) -> Brep {
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
