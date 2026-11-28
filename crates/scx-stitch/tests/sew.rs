// Written for solidcorex. Apache-2.0.
//! ST5: the single-body operations, judged against closed forms.
//!
//! - `open_edges`, `gather` and `sew`: bodies exploded to sheets, gathered and sewn are the
//!   stitched solid (counts, closed-form volume and area, `contract`), whatever the order.
//! - `merge_faces`: the pin's two boxes merged into one box, a subdivided block merged back
//!   to six faces, the faces of a partition left alone, only the faces asked for merged.
//! - The pin's `edges_of_three_faces_the_body_came_with_do_not_refuse_the_stitch`, restated
//!   for a body that comes with a partition: its edges of three faces stay as they are.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

use std::f64::consts::PI;

use scx_brep::op::{Cx, NoTrace, Scratch, Settings};
use scx_brep::{
    Brep, Document, EdgeGeom, EdgeId, EdgeShape, Entity, FaceId, FaceRecord, FaceSide, Operand,
    Origin, ShellRecord, VertexRecord,
};
use scx_check::{Contacts, contract};
use scx_curve::{Curve, Line};
use scx_mass::{RelAccuracy, area, outward};
use scx_num::budget::{Budget, Stop};
use scx_num::extent1d::Extent1d;
use scx_num::frame::Frame;
use scx_num::frame::Sense;
use scx_num::units::{Length, Resolution};
use scx_num::vector3d::{Point3, Vec3};
use scx_stitch::{
    StitchError, StitchRefusal, StitchUnsupported, Want, gather, merge_faces, open_edges, sew,
    stitch,
};
use scx_surface::Plane;
use scx_surface::Surface;
use scx_testkit::bodies::Kind;

mod common;
use common::*;

fn counts(b: &Brep) -> [usize; 4] {
    [
        b.shells().count(),
        b.faces().count(),
        b.edges().count(),
        b.vertices().count(),
    ]
}

fn surface_area(b: &Brep) -> f64 {
    area(
        b,
        RelAccuracy::new(1e-12).unwrap(),
        &mut Scratch::new(),
        &mut Budget::unlimited(),
        &Stop::new(),
    )
    .unwrap()
    .value
}

fn close(got: f64, want: f64, what: &str) {
    assert!(
        (got - want).abs() <= 1e-9 * want.abs().max(1.0),
        "{what}: {got}, want {want}"
    );
}

/// Runs `f` with a context over the default document.
fn with_cx<T>(f: impl FnOnce(&mut Cx<'_>) -> T) -> T {
    let doc = Document::default();
    let mut scratch = Scratch::new();
    let settings = Settings::new();
    let stop = Stop::new();
    let mut cx = Cx::new(&doc, &settings, &mut scratch, &stop, &NoTrace);
    f(&mut cx)
}

fn assert_contract(b: &Brep, what: &str) {
    with_cx(|cx| {
        contract(b, &*cx, Contacts::Refused).unwrap_or_else(|e| panic!("{what}: {e:?}"));
    });
}

/// The unit block moved `x` along x.
fn block_at(x: f64) -> Brep {
    let mut d = scx_testkit::bodies::generate(Kind::Block, 0, 1)
        .unwrap()
        .draft;
    d.transform(
        &scx_num::math::Affine::translation(Vec3::new(x, 0.0, 0.0)),
        &Document::default(),
    )
    .unwrap();
    finish(d)
}

/// The plane of face `i` of `b` (in face order): its normal and origin.
fn plane(b: &Brep, i: usize) -> (Vec3, Point3) {
    match &b.faces().nth(i).unwrap().record().surface {
        Surface::Plane(p) => (Vec3::from(p.frame().z()), p.frame().origin()),
        s => panic!("not a plane: {s:?}"),
    }
}

/// The index of the face of `b` on the plane `axis = at` with its normal along `+axis`
/// when `plus` (else `-axis`).
fn wall(b: &Brep, axis: usize, at: f64, plus: bool) -> usize {
    (0..b.faces().count())
        .find(|&i| {
            let (n, o) = plane(b, i);
            let (n, o) = ([n.x, n.y, n.z][axis], [o.x, o.y, o.z][axis]);
            (n - if plus { 1.0 } else { -1.0 }).abs() < 1e-12 && (o - at).abs() < 1e-12
        })
        .unwrap()
}

/// Two unit blocks side by side as one closed body of ten faces: the pin's
/// `two_boxes_side_by_side_merge_into_one_box`, built by stitching the five outer faces of
/// each block (the walls at x = 1 left out) so that each pair of walls is split in two.
fn two_boxes() -> Brep {
    let (a, b) = (block_at(0.0), block_at(1.0));
    let (sa, sb) = (explode(&a, |_| false), explode(&b, |_| false));
    let (ia, ib) = (wall(&a, 0, 1.0, true), wall(&b, 0, 1.0, false));
    let sheets: Vec<&Brep> = sa
        .iter()
        .enumerate()
        .filter(|&(i, _)| i != ia)
        .map(|(_, s)| s)
        .chain(
            sb.iter()
                .enumerate()
                .filter(|&(i, _)| i != ib)
                .map(|(_, s)| s),
        )
        .collect();
    let out = run(&sheets).unwrap();
    assert!(out.report.left.is_empty());
    out.done.body
}

fn all_faces(b: &Brep) -> Vec<FaceId> {
    b.faces().map(scx_brep::Face::id).collect()
}

fn merged(b: &Brep, faces: &[FaceId]) -> scx_stitch::Merged {
    with_cx(|cx| merge_faces(b, faces, cx)).unwrap()
}

type Key = Vec<String>;
/// A closed-form volume and area, where one is known.
type Truth = Option<(f64, f64)>;

/// The body's geometry as sorted text (every float to its bits; no id).
fn geometry(b: &Brep) -> (Key, Key, Key) {
    let pt = |v: scx_brep::VertexId| format!("{:?}", b.vertex(v).unwrap().record());
    let mut vs: Key = b.vertices().map(|v| format!("{:?}", v.record())).collect();
    let edge = |e: EdgeId| {
        let g = b.edge(e).unwrap().geometry().clone();
        format!(
            "{:?} {:?} {}|{}",
            g.shape,
            g.accuracy,
            pt(g.ends[0]),
            pt(g.ends[1])
        )
    };
    let mut es: Key = b.edges().map(|e| edge(e.id())).collect();
    let mut fs: Key = b
        .faces()
        .map(|f| {
            let mut uses: Vec<String> = f
                .uses()
                .map(|u| format!("{} {:?}", edge(u.edge().id()), u.sense()))
                .collect();
            uses.sort();
            let side = |s| f.shell(s).region().is_solid();
            format!(
                "{:?} {} {}{} {uses:?}",
                f.record().surface,
                f.record().loops.len(),
                u8::from(side(FaceSide::Back)),
                u8::from(side(FaceSide::Front)),
            )
        })
        .collect();
    vs.sort();
    es.sort();
    fs.sort();
    (vs, es, fs)
}

/// A deterministic shuffle of `0..n`.
fn shuffled(n: usize, mut seed: u64) -> Vec<usize> {
    let mut v: Vec<usize> = (0..n).collect();
    for i in (1..n).rev() {
        seed = seed
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        let j = usize::try_from(seed >> 33).unwrap() % (i + 1);
        v.swap(i, j);
    }
    v
}

/// `body`'s faces as one gathered body of loose sheets, in the order `order` gives.
fn loose(body: &Brep, order: &[usize]) -> Brep {
    let sheets = explode(body, |_| false);
    let refs: Vec<&Brep> = order.iter().map(|&i| &sheets[i]).collect();
    with_cx(|cx| gather(&refs, cx)).unwrap().body
}

#[test]
fn open_edges_are_the_edges_of_one_face_use() {
    // A closed solid has none, nor has a cylinder, whose wall uses its seam twice; each
    // face alone has its whole loop; a body of its faces but one has the rim.
    let block = solid(Kind::Block, 0, 1);
    assert!(open_edges(&block).is_empty());
    assert!(open_edges(&solid(Kind::Cylinder, 0, 3)).is_empty());
    let sheets = explode(&block, |_| false);
    for s in &sheets {
        assert_eq!(open_edges(s).len(), 4);
    }
    let top = wall(&block, 2, 1.0, true);
    let rest: Vec<&Brep> = sheets
        .iter()
        .enumerate()
        .filter(|&(i, _)| i != top)
        .map(|(_, s)| s)
        .collect();
    let open = with_cx(|cx| stitch(&rest, Want::Sheets, cx))
        .unwrap()
        .done
        .body;
    let rim = open_edges(&open);
    assert_eq!(rim.len(), 4);
    assert!(rim.windows(2).all(|w| w[0] < w[1]), "id order");
    for e in rim {
        for v in open.edge(e).unwrap().geometry().ends {
            assert!((open.vertex(v).unwrap().point().z - 1.0).abs() < 1e-12);
        }
    }
}

#[test]
fn gather_puts_bodies_side_by_side_unsewn() {
    let (a, b) = (block_at(0.0), block_at(1.0));
    let got = with_cx(|cx| gather(&[&a, &b], cx)).unwrap();
    // Two solids that touch along a wall: both bodies' entities, none identified.
    assert_eq!(counts(&got.body), [4, 12, 24, 16]);
    assert_eq!(got.body.regions().filter(|r| r.is_solid()).count(), 2);
    close(vol(&got.body), 2.0, "gathered volume");
    // Body 0 keeps its ids and its entities have no origin; each face of body 1 is
    // derived from it.
    for f in a.faces() {
        assert!(got.body.face(f.id()).is_some());
        assert!(got.changes.faces.origins(f.id()).is_none());
    }
    let from_b = got
        .body
        .faces()
        .filter_map(|f| got.changes.faces.origins(f.id()))
        .filter(|o| {
            o.iter()
                .all(|o| matches!(o, Origin::Operand(op, Entity::Face(_)) if *op == Operand::from_raw(1)))
        })
        .count();
    assert_eq!(from_b, 6);
    assert!(open_edges(&got.body).is_empty());
    // No bodies is the empty body, one body is itself.
    let none = with_cx(|cx| gather(&[], cx)).unwrap();
    assert_eq!(none.body.faces().count() + none.body.vertices().count(), 0);
    let one = with_cx(|cx| gather(&[&a], cx)).unwrap();
    assert_eq!(geometry(&one.body), geometry(&a));
    assert!(one.changes.is_empty());
}

#[test]
fn gather_refuses_another_documents_body() {
    let a = block_at(0.0);
    let doc = Document::with_resolution(Resolution::new(Length::from_mm(1e-4)).unwrap());
    let mut scratch = Scratch::new();
    let settings = Settings::new();
    let stop = Stop::new();
    let mut cx = Cx::new(&doc, &settings, &mut scratch, &stop, &NoTrace);
    assert_eq!(
        gather(&[&a], &mut cx).unwrap_err(),
        StitchError::Refused(StitchRefusal::OtherDocument {
            operand: Operand::from_raw(0)
        })
    );
    assert!(matches!(
        sew(&a, Want::Sheets, &mut cx),
        Err(StitchError::Refused(StitchRefusal::OtherDocument { .. }))
    ));
    assert!(matches!(
        merge_faces(&a, &[], &mut cx),
        Err(StitchError::Refused(StitchRefusal::OtherDocument { .. }))
    ));
}

/// A body of loose sheets, sewn, is the stitch of the sheets: the same solid by closed form
/// (volume, area, `contract`) and the same bits, whatever the order the sheets were
/// gathered in.
#[test]
fn sewn_loose_sheets_are_the_stitched_solid() {
    let cases: [(Kind, usize, Truth); 9] = [
        (Kind::Block, 1, Some((1.0, 6.0))),
        (Kind::Block, 3, Some((1.0, 6.0))),
        (Kind::Prism, 2, None),
        (Kind::Cylinder, 3, Some((2.0 * PI, 6.0 * PI))),
        (Kind::Cone, 4, Some((PI / 3.0, PI * (1.0 + 2f64.sqrt())))),
        (Kind::Sphere, 4, Some((4.0 / 3.0 * PI, 4.0 * PI))),
        (Kind::Torus, 3, Some((4.0 * PI * PI, 8.0 * PI * PI))),
        (Kind::Holed, 2, None),
        (Kind::Cavity, 1, Some((26.0, 60.0))),
    ];
    for (kind, n, truth) in cases {
        let what = format!("{kind:?} {n}");
        let body = solid(kind, 0, n);
        let faces = body.faces().count();
        let order: Vec<usize> = (0..faces).collect();
        let g = loose(&body, &order);
        assert!(!open_edges(&g).is_empty(), "{what}");
        let sewn =
            with_cx(|cx| sew(&g, Want::Solid, cx)).unwrap_or_else(|e| panic!("{what}: {e:?}"));
        let b = &sewn.done.body;
        assert_eq!(counts(b), counts(&body), "{what}: counts");
        assert_eq!(
            b.regions().count(),
            body.regions().count(),
            "{what}: regions"
        );
        assert!(sewn.open.is_empty(), "{what}: {:?}", sewn.open);
        assert!(sewn.report.left.is_empty(), "{what}");
        assert_eq!(sewn.pieces.len(), faces, "{what}: one piece a face");
        close(vol(b), vol(&body), &what);
        close(surface_area(b), surface_area(&body), &what);
        if let Some((v, a)) = truth {
            close(vol(b), v, &what);
            close(surface_area(b), a, &what);
        }
        outward(b, &mut Budget::unlimited(), &Stop::new()).unwrap();
        assert_contract(b, &what);
        // The same bits as the stitch of the sheets, and of any order of them.
        let sheets = explode(&body, |_| false);
        let stitched = run(&refs(&sheets)).unwrap().done.body;
        assert_eq!(geometry(b), geometry(&stitched), "{what}: stitch");
        for seed in 1..4 {
            let g = loose(&body, &shuffled(faces, seed));
            let again = with_cx(|cx| sew(&g, Want::Solid, cx)).unwrap();
            assert_eq!(
                geometry(&again.done.body),
                geometry(b),
                "{what}: seed {seed}"
            );
        }
    }
}

#[test]
fn a_body_with_no_open_edge_is_returned_as_it_is() {
    let block = solid(Kind::Block, 0, 2);
    for want in [Want::Sheets, Want::Solid] {
        let s = with_cx(|cx| sew(&block, want, cx)).unwrap();
        assert_eq!(geometry(&s.done.body), geometry(&block));
        assert!(s.done.changes.is_empty());
        assert!(s.open.is_empty());
        assert_eq!(s.pieces, vec![all_faces(&block)]);
    }
    // A closed sheet with no material is oriented by `Want::Solid`, not left.
    let sheets = explode(&block, |_| false);
    let closed = with_cx(|cx| stitch(&refs(&sheets), Want::Sheets, cx))
        .unwrap()
        .done
        .body;
    assert!(closed.regions().all(|r| !r.is_solid()));
    let s = with_cx(|cx| sew(&closed, Want::Solid, cx)).unwrap();
    close(vol(&s.done.body), vol(&block), "oriented");
    // `Want::Sheets` leaves it a sheet.
    let s = with_cx(|cx| sew(&closed, Want::Sheets, cx)).unwrap();
    assert!(s.done.body.regions().all(|r| !r.is_solid()));
}

/// An open edge left open is reported, and `Want::Solid` refuses it.
#[test]
fn what_does_not_close_stays_open() {
    let block = cube(0.0, 1.0);
    let sheets = explode(&block, |_| false);
    let top = wall(&block, 2, 1.0, true);
    let rest: Vec<usize> = (0..sheets.len()).filter(|&i| i != top).collect();
    let g = loose(&block, &rest);
    let s = with_cx(|cx| sew(&g, Want::Sheets, cx)).unwrap();
    assert_eq!(s.open.len(), 4);
    assert_eq!(s.done.body.faces().count(), 5);
    assert_eq!(s.report.joined.len(), 8);
    assert!(matches!(
        with_cx(|cx| sew(&g, Want::Solid, cx)),
        Err(StitchError::Refused(StitchRefusal::NotSolid { .. }))
    ));
}

#[test]
fn a_wire_or_a_lone_vertex_is_not_lost() {
    let block = solid(Kind::Block, 0, 1);
    let sheets = explode(&block, |_| false);
    let g = with_cx(|cx| gather(&refs(&sheets), cx)).unwrap().body;
    let mut d = g.edit();
    let a = d
        .add_vertex(VertexRecord::new(Point3::new(5.0, 0.0, 0.0)))
        .unwrap();
    let b = d
        .add_vertex(VertexRecord::new(Point3::new(6.0, 0.0, 0.0)))
        .unwrap();
    let line = Line::through(Point3::new(5.0, 0.0, 0.0), Point3::new(6.0, 0.0, 0.0)).unwrap();
    let wire = d
        .add_edge(EdgeGeom::new(
            EdgeShape::Curve {
                curve: Curve::Line(line),
                interval: Extent1d::new(0.0, 1.0).unwrap(),
            },
            [a, b],
        ))
        .unwrap();
    let mut rec = ShellRecord::new(scx_brep::RegionId::OUTSIDE);
    rec.wires.push(wire);
    d.add_shell(rec).unwrap();
    let with_wire = finish(d);
    assert_eq!(
        with_cx(|cx| sew(&with_wire, Want::Sheets, cx)).unwrap_err(),
        StitchError::Unsupported(StitchUnsupported::Loose {
            at: Entity::Edge(wire)
        })
    );
}

/// Two unit blocks side by side as one closed body with their wall a partition: two solid
/// regions, the wall one face between them, and each of the wall's four edges used by
/// three faces (the wall and the two faces on each side that meet there). Built from
/// `two_boxes`: the faces are given to the block they lie in, and the wall is added.
fn partitioned() -> Brep {
    let two = two_boxes();
    let doc = Document::default();
    let mut d = two.edit();
    let inner = two
        .regions()
        .find(|r| r.is_solid())
        .map(scx_brep::Region::id)
        .unwrap();
    let a_shell = two
        .shells()
        .find(|s| s.region().id() == inner)
        .map(scx_brep::Shell::id)
        .unwrap();
    let r_b = d.add_region(true).unwrap();
    let b_shell = d.add_shell(ShellRecord::new(r_b)).unwrap();
    for f in two.faces() {
        if f.bounding_box().unwrap().mid().x > 1.0 {
            let id = f.id();
            d.edit_face(id, |rec| {
                for side in &mut rec.sides {
                    if *side == a_shell {
                        *side = b_shell;
                    }
                }
            })
            .unwrap();
        }
    }
    // The wall: the plane x = 1 facing +x, whose front is block B's.
    let frame = Frame::new(
        Point3::new(1.0, 0.0, 0.0),
        Vec3::new(1.0, 0.0, 0.0),
        Vec3::new(0.0, 1.0, 0.0),
    )
    .unwrap();
    let surface = Surface::Plane(Plane::new(frame));
    let at_wall = |v: scx_brep::VertexId| (two.vertex(v).unwrap().point().x - 1.0).abs() < 1e-12;
    let rim: Vec<EdgeId> = two
        .edges()
        .filter(|e| e.geometry().ends.iter().all(|&v| at_wall(v)))
        .map(scx_brep::Edge::id)
        .collect();
    assert_eq!(rim.len(), 4);
    // Walk (y, z) = (0,0) (1,0) (1,1) (0,1): counter-clockwise about +x.
    let corner = |y: f64, z: f64| {
        two.vertices()
            .find(|v| {
                at_wall(v.id())
                    && (v.point().y - y).abs() < 1e-12
                    && (v.point().z - z).abs() < 1e-12
            })
            .unwrap()
            .id()
    };
    let ring = [
        corner(0.0, 0.0),
        corner(1.0, 0.0),
        corner(1.0, 1.0),
        corner(0.0, 1.0),
    ];
    let uses: Vec<(EdgeId, Sense)> = (0..4)
        .map(|i| {
            let (p, q) = (ring[i], ring[(i + 1) % 4]);
            let e = rim
                .iter()
                .copied()
                .find(|&e| {
                    let ends = two.edge(e).unwrap().geometry().ends;
                    ends == [p, q] || ends == [q, p]
                })
                .unwrap();
            let sense = if two.edge(e).unwrap().geometry().ends[0] == p {
                Sense::Forward
            } else {
                Sense::Reversed
            };
            (e, sense)
        })
        .collect();
    let loops = d.exact_loops(&surface, &[uses], &doc).unwrap();
    d.add_face(FaceRecord::new(surface, [b_shell, a_shell], loops))
        .unwrap();
    // The partition is the only face between two material regions; its edges have three
    // fins. About each, the wall goes between the faces of the two blocks, in the order
    // that is counter-clockwise about the edge: found by trying the two on each edge.
    let wall_face = d.faces().last().unwrap().id();
    for mask in 0..(1_u32 << rim.len()) {
        let mut d = d.clone();
        for (i, &e) in rim.iter().enumerate() {
            let fins = d.edge(e).unwrap().fins().collect::<Vec<_>>();
            assert_eq!(fins.len(), 3);
            let (mid, others): (Vec<_>, Vec<_>) =
                fins.into_iter().partition(|f| f.face == wall_face);
            let ordered = if mask >> i & 1 == 0 {
                vec![others[0], mid[0], others[1]]
            } else {
                vec![others[1], mid[0], others[0]]
            };
            d.adjacency_mut(e).unwrap().fins = ordered;
        }
        if let Ok((b, _)) = d.finish(&doc) {
            return b;
        }
    }
    panic!("no radial order of the partition's edges finishes");
}

#[test]
fn a_partition_gives_edges_of_three_faces() {
    let p = partitioned();
    assert_eq!(counts(&p), [3, 11, 20, 12]);
}

#[test]
fn a_partitions_three_face_edges_are_not_merged_across() {
    let p = partitioned();
    for e in p.edges() {
        let wall = e
            .geometry()
            .ends
            .iter()
            .all(|&v| (p.vertex(v).unwrap().point().x - 1.0).abs() < 1e-12);
        if wall {
            assert_eq!(e.fins().len(), 3, "{:?}", e.id());
        }
    }
    // Nothing to sew: the body is returned as it is, its edges of three faces intact.
    let s = with_cx(|cx| sew(&p, Want::Sheets, cx)).unwrap();
    assert_eq!(geometry(&s.done.body), geometry(&p));
    assert!(s.done.changes.is_empty() && s.open.is_empty());
    // Nothing to merge across them either, whatever faces are asked for: the faces on each
    // side lie on one plane but across an edge of three faces, bound by two shells.
    let m = merged(&p, &all_faces(&p));
    assert_eq!(counts(&m.done.body), counts(&p));
    assert!(m.report.faces.is_empty() && m.report.edges.is_empty());
    assert!(m.done.changes.is_empty());
}

/// The pin's `edges_of_three_faces_the_body_came_with_do_not_refuse_the_stitch`, restated:
/// loose sheets of a cube gathered beside a body that comes with a partition are sewn
/// into a cube, and the partition's edges of three faces are neither refused nor changed.
/// (The pin lays a cap onto the partitioned body's own edges, which would give those edges a
/// fourth fin: that join is `Unsupported` here, and a solid operand with it.)
#[test]
fn a_body_with_a_partition_sews_with_loose_sheets_and_keeps_it() {
    let p = partitioned();
    let cube_sheets = explode(&cube(5.0, 1.0), |_| false);
    let mut bodies: Vec<&Brep> = vec![&p];
    bodies.extend(cube_sheets.iter());
    let g = with_cx(|cx| gather(&bodies, cx)).unwrap().body;
    let s = with_cx(|cx| sew(&g, Want::Sheets, cx)).unwrap();
    let b = &s.done.body;
    assert_eq!(
        s.pieces.len(),
        1 + 6,
        "the partitioned body is one piece, each sheet its own"
    );
    assert_eq!(s.pieces[0].len(), 11);
    assert_eq!(s.report.joined.len(), 12);
    assert!(s.open.is_empty());
    // 11 faces and 6, edges 20 + 12, vertices 12 + 8.
    assert_eq!(counts(b)[1], 17);
    assert_eq!(counts(b)[2], p.edges().count() + 12);
    assert_eq!(counts(b)[3], p.vertices().count() + 8);
    assert_eq!(b.edges().filter(|e| e.fins().len() == 3).count(), 4);
    assert_eq!(b.regions().filter(|r| r.is_solid()).count(), 2);
    // A solid is asked of a sheet to orient: the partitioned body is not one.
    assert!(matches!(
        with_cx(|cx| sew(&g, Want::Solid, cx)),
        Err(StitchError::Unsupported(
            StitchUnsupported::SolidOperand { .. }
        ))
    ));
}

/// The pin's `two_boxes_side_by_side_merge_into_one_box`: ten faces, two on each of the
/// four walls that run along both boxes, merge into one box of six faces, volume 2.
#[test]
fn two_boxes_merge_into_one_box() {
    let two = two_boxes();
    assert_eq!(counts(&two), [2, 10, 20, 12]);
    close(vol(&two), 2.0, "before");
    let m = merged(&two, &all_faces(&two));
    let b = &m.done.body;
    assert_eq!(counts(b), [2, 6, 12, 8]);
    close(vol(b), 2.0, "volume");
    close(surface_area(b), 10.0, "area");
    outward(b, &mut Budget::unlimited(), &Stop::new()).unwrap();
    assert_contract(b, "merged");
    assert_eq!(m.report.faces.len(), 4);
    assert_eq!(m.report.edges.len(), 4);
    assert!(m.report.left.is_empty(), "{:?}", m.report.left);
    // Merging again changes nothing.
    let again = merged(b, &all_faces(b));
    assert_eq!(geometry(&again.done.body), geometry(b));
    assert!(again.done.changes.is_empty());
    // Only the faces asked for: all but the top of the second box leaves the top split.
    let top_b = two
        .faces()
        .find(|f| {
            f.bounding_box().unwrap().mid().x > 1.0
                && (plane(&two, f.id().index()).0.z - 1.0).abs() < 1e-12
        })
        .map(scx_brep::Face::id)
        .unwrap();
    let rest: Vec<FaceId> = all_faces(&two)
        .into_iter()
        .filter(|&f| f != top_b)
        .collect();
    let part = merged(&two, &rest);
    assert_eq!(counts(&part.done.body), [2, 7, 15, 10]);
    close(vol(&part.done.body), 2.0, "partial");
    assert_contract(&part.done.body, "partial");
    // Nothing asked for, nothing merged.
    let none = merged(&two, &[]);
    assert_eq!(geometry(&none.done.body), geometry(&two));
    assert!(none.done.changes.is_empty());
}

/// A block of n steps is a body of 6 n^2 faces on six planes, each face on its own frame:
/// merged, it is the six-faced block, whatever the sizes.
#[test]
fn a_subdivided_block_merges_back_to_six_faces() {
    for (seed, n) in [(0, 2), (0, 3), (0, 5), (1, 3), (2, 3), (3, 2)] {
        let b = solid(Kind::Block, seed, n);
        let m = merged(&b, &all_faces(&b));
        let out = &m.done.body;
        let what = format!("block seed {seed} steps {n}");
        assert_eq!(counts(out), [2, 6, 12, 8], "{what} {:?}", m.report);
        close(vol(out), vol(&b), &what);
        close(surface_area(out), surface_area(&b), &what);
        outward(out, &mut Budget::unlimited(), &Stop::new()).unwrap();
        assert_contract(out, &what);
        if seed == 0 {
            close(vol(out), 1.0, &what);
            close(surface_area(out), 6.0, &what);
        }
    }
}

/// Merging keeps every recipe the same solid: the volume and area of the source, the
/// outward certificate, the contract, and no more faces than it had.
#[test]
fn merging_keeps_every_recipe_the_same_solid() {
    for (kind, n) in [
        (Kind::Prism, 2),
        (Kind::Cylinder, 3),
        (Kind::Cylinder, 7),
        (Kind::Cone, 4),
        (Kind::Sphere, 4),
        (Kind::Torus, 3),
        (Kind::Holed, 2),
        (Kind::Cavity, 1),
        (Kind::Cavity, 3),
    ] {
        let b = solid(kind, 0, n);
        let what = format!("{kind:?} {n}");
        let m = merged(&b, &all_faces(&b));
        let out = &m.done.body;
        assert!(out.faces().count() <= b.faces().count(), "{what}");
        assert_eq!(out.regions().count(), b.regions().count(), "{what}");
        close(vol(out), vol(&b), &what);
        close(surface_area(out), surface_area(&b), &what);
        outward(out, &mut Budget::unlimited(), &Stop::new()).unwrap();
        assert_contract(out, &what);
    }
}

/// The two boxes built from their sheets in any order merge to the same bits.
#[test]
fn merging_is_the_same_whatever_the_order() {
    let (a, b) = (block_at(0.0), block_at(1.0));
    let (ia, ib) = (wall(&a, 0, 1.0, true), wall(&b, 0, 1.0, false));
    let mut sheets = explode(&a, |_| false);
    sheets.remove(ia);
    let mut sb = explode(&b, |_| false);
    sb.remove(ib);
    sheets.extend(sb);
    let build = |order: &[usize]| {
        let refs: Vec<&Brep> = order.iter().map(|&i| &sheets[i]).collect();
        let two = run(&refs).unwrap().done.body;
        merged(&two, &all_faces(&two)).done.body
    };
    let n = sheets.len();
    let first = geometry(&build(&(0..n).collect::<Vec<_>>()));
    for seed in 1..6 {
        assert_eq!(geometry(&build(&shuffled(n, seed))), first, "seed {seed}");
    }
}

#[test]
fn a_face_the_body_does_not_hold_is_refused() {
    let b = block_at(0.0);
    let f = FaceId::from_raw(99);
    assert_eq!(
        with_cx(|cx| merge_faces(&b, &[f], cx)).unwrap_err(),
        StitchError::Refused(StitchRefusal::NoSuchFace { face: f })
    );
}
