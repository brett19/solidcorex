// Written for solidcorex. Apache-2.0.
//! Stage 5 on blocks, cylinders and spheres: the pieces each face is cut into worked by
//! hand from the geometry; each operand's part of the draft finished, its volume the
//! operand's, and every pcurve on its edge.

use std::collections::BTreeSet;

use scx_brep::op::Scratch;
use scx_brep::{
    Brep, Document, Draft, EdgeId, EdgeShape, Entity, FaceId, Operand, Origin, RegionId, ShellId,
    UseGeom, VertexId,
};
use scx_mass::RelAccuracy;
use scx_num::budget::{Budget, Stop};
use scx_surface::Surface;

use super::face_face::{X, Z, ball, cyl};
use super::{cuboid, run, unit};
use crate::BooleanError;
use crate::faceface::face_face;
use crate::geom::{Operands, Side};
use crate::imprint::{Imprint, imprint};

fn imp(a: &Brep, b: &Brep) -> Result<(Imprint, u64), BooleanError> {
    let doc = Document::default();
    let ops = Operands { a, b };
    let mut budget = Budget::unlimited();
    let i = run(a, b)?;
    let f = face_face(ops, &i, &mut budget, &Stop::new())?;
    let before = budget.used();
    let m = imprint(ops, &i, &f, doc, &mut budget, &Stop::new())?;
    Ok((m, budget.used() - before))
}

/// The faces of `side` in the draft.
fn faces(m: &Imprint, side: Side) -> BTreeSet<FaceId> {
    m.pieces
        .iter()
        .filter(|((s, _), _)| *s == side)
        .flat_map(|(_, p)| p.iter().copied())
        .collect()
}

/// How many pieces each of `side`'s faces is in, sorted.
fn counts(m: &Imprint, side: Side) -> Vec<usize> {
    let mut v: Vec<usize> = m
        .pieces
        .iter()
        .filter(|((s, _), _)| *s == side)
        .map(|(_, p)| p.len())
        .collect();
    v.sort_unstable();
    v
}

/// `side`'s part of the draft: the other side's faces removed, then whatever bounds
/// nothing left.
fn only(m: &Imprint, side: Side) -> Draft {
    let mut d = m.draft.clone();
    for f in faces(m, side.other()) {
        d.remove_face(f).unwrap();
    }
    let loose: Vec<EdgeId> = d
        .edges()
        .filter(|e| e.fins().len() == 0)
        .map(scx_brep::Edge::id)
        .collect();
    for e in loose {
        d.remove_edge(e).unwrap();
    }
    let ends: BTreeSet<VertexId> = d.edges().flat_map(|e| e.geometry().ends).collect();
    let loose: Vec<VertexId> = d
        .vertices()
        .map(scx_brep::Vertex::id)
        .filter(|v| !ends.contains(v))
        .collect();
    for v in loose {
        d.remove_vertex(v).unwrap();
    }
    let used: BTreeSet<ShellId> = d.faces().flat_map(|f| f.record().sides).collect();
    let empty: Vec<ShellId> = d
        .shells()
        .map(scx_brep::Shell::id)
        .filter(|s| !used.contains(s))
        .collect();
    for s in empty {
        d.remove_shell(s).unwrap();
    }
    let used: BTreeSet<RegionId> = d.shells().map(|s| s.record().region).collect();
    let empty: Vec<RegionId> = d
        .regions()
        .map(scx_brep::Region::id)
        .filter(|r| *r != RegionId::OUTSIDE && !used.contains(r))
        .collect();
    for r in empty {
        d.remove_region(r).unwrap();
    }
    d
}

fn volume(body: &Brep) -> f64 {
    scx_mass::volume(
        body,
        RelAccuracy::new(1e-10).unwrap(),
        &mut Scratch::new(),
        &mut Budget::unlimited(),
        &Stop::new(),
    )
    .unwrap()
    .value
}

/// Every pcurve of `body` on its edge, and every pole point on its vertex, within the
/// edge's or vertex's reach (its accuracy or the zone) plus rounding: the largest miss.
fn pcurves_on_edges(body: &Brep) -> f64 {
    let zone = Document::default().resolution().zone().mm();
    let mut worst: f64 = 0.0;
    for f in body.faces() {
        let s = f.surface();
        for l in &f.record().loops {
            for u in &l.uses {
                let e = body.edge(u.edge).unwrap();
                let reach = e.accuracy().map_or(zone, |r| r.acc.length().mm().max(zone));
                match (&u.geom, e.shape()) {
                    (UseGeom::Pcurve { curve, interval }, EdgeShape::Curve { curve: c, .. }) => {
                        for k in 0..=64 {
                            let t = interval.min() + interval.length() * f64::from(k) / 64.0;
                            let miss = s.point(curve.point(t)).distance(c.point(t));
                            assert!(miss <= reach + 1e-9, "{:?} {miss}", u.edge);
                            worst = worst.max(miss);
                        }
                    }
                    // A pole's use: a point, or the segment along the pole's line in uv.
                    (UseGeom::Point(p), EdgeShape::Collapsed) => {
                        let v = body.vertex(e.ends()[0]).unwrap();
                        let miss = s.point(*p).distance(v.point());
                        assert!(miss <= reach + 1e-9, "{:?} {miss}", u.edge);
                        worst = worst.max(miss);
                    }
                    (UseGeom::Pcurve { curve, interval }, EdgeShape::Collapsed) => {
                        let v = body.vertex(e.ends()[0]).unwrap();
                        for k in 0..=64 {
                            let t = interval.min() + interval.length() * f64::from(k) / 64.0;
                            let miss = s.point(curve.point(t)).distance(v.point());
                            assert!(miss <= reach + 1e-9, "{:?} {miss}", u.edge);
                            worst = worst.max(miss);
                        }
                    }
                    (g, sh) => panic!("{:?}: {g:?} on {sh:?}", u.edge),
                }
            }
        }
    }
    worst
}

/// Both operands' parts of `m` finish; each has the operand's volume, the given face
/// count, and its pcurves on its edges.
fn check(m: &Imprint, a: &Brep, b: &Brep, faces: [usize; 2]) {
    let doc = Document::default();
    for (side, op, n) in [(Side::A, a, faces[0]), (Side::B, b, faces[1])] {
        let (body, _) = only(m, side)
            .finish(&doc)
            .unwrap_or_else(|e| panic!("{side:?}: {e:?}"));
        assert_eq!(body.faces().len(), n, "{side:?}");
        let (v, w) = (volume(&body), volume(op));
        assert!((v - w).abs() <= 1e-8 * w.abs(), "{side:?}: {v} against {w}");
        let worst = pcurves_on_edges(&body);
        assert!(worst < 1e-9, "{side:?}: {worst}");
    }
}

fn sections(m: &Imprint) -> usize {
    m.cuts.len()
}

#[test]
fn overlapping_cubes_cut_three_faces_each_into_a_square_and_an_l() {
    // b = a + (0.5, 0.5, 0.5). Each face of a's corner (1, 1, 1) holds two of the six
    // sections, an L from the edge it shares with b's face to the next: they cut off the
    // quarter [0.5, 1]^2 of it. So 3 of a's faces in 2, and the same of b's.
    let doc = Document::default();
    let (a, b) = (unit(doc), cuboid([0.5; 3], [1.5; 3], doc));
    let (m, _) = imp(&a, &b).unwrap();
    assert_eq!(counts(&m, Side::A), vec![1, 1, 1, 2, 2, 2]);
    assert_eq!(counts(&m, Side::B), vec![1, 1, 1, 2, 2, 2]);
    assert_eq!(sections(&m), 6);
    check(&m, &a, &b, [9, 9]);
    // Each of a's three corner edges is cut once, where it crosses b's face: 12 + 3 of
    // a's, the same of b's, and the six sections.
    assert_eq!(m.draft.edges().len(), 15 + 15 + 6);
}

#[test]
fn a_block_through_a_side_cuts_the_front_top_and_four_of_its_faces() {
    // b = [0.25, 0.75] x [-0.5, 0.5] x [0.5, 1]. a's front y = 0 loses the rectangle the
    // three sections bound; a's top, the rectangle [0.25, 0.75] x [0, 0.5] that b's three
    // top edges (lying pieces) cut off. b's sides x = 0.25 and x = 0.75 and its bottom are
    // each cut in two at y = 0 by a section, and its top by a's front top edge.
    let doc = Document::default();
    let (a, b) = (unit(doc), cuboid([0.25, -0.5, 0.5], [0.75, 0.5, 1.0], doc));
    let (m, _) = imp(&a, &b).unwrap();
    assert_eq!(counts(&m, Side::A), vec![1, 1, 1, 1, 2, 2]);
    assert_eq!(counts(&m, Side::B), vec![1, 1, 2, 2, 2, 2]);
    // 3 sections, a's front top piece on b's top, and b's three top pieces on a's top.
    assert_eq!(sections(&m), 7);
    check(&m, &a, &b, [8, 10]);
}

#[test]
fn blocks_sharing_a_face_share_its_edges_and_cut_nothing() {
    // b = a + (1, 0, 0): b's copy of the shared square's four edges is a's edges, so b
    // adds the other 8 edges and the 4 vertices off the square.
    let doc = Document::default();
    let (a, b) = (unit(doc), cuboid([1.0, 0.0, 0.0], [2.0, 1.0, 1.0], doc));
    let (m, _) = imp(&a, &b).unwrap();
    assert!(m.cuts.is_empty());
    assert_eq!(counts(&m, Side::A), vec![1; 6]);
    assert_eq!(counts(&m, Side::B), vec![1; 6]);
    assert_eq!(m.draft.edges().len(), 12 + 8);
    assert_eq!(m.draft.vertices().len(), 8 + 4);
    check(&m, &a, &b, [6, 6]);
    let (_, changes) = only(&m, Side::B).finish(&doc).unwrap();
    let c = &changes.edges.coincident;
    assert_eq!(c.len(), 4, "{c:?}");
    assert!(c.iter().all(|x| x.operand == Operand::from_raw(1)));
    assert_eq!(changes.vertices.coincident.len(), 4);
}

#[test]
fn a_cylinder_through_a_block_cuts_a_disk_from_the_top_and_bottom_and_the_wall_in_three() {
    // The unit cylinder from z = -1 to 2 through [-2, 2]^2 x [0, 1]: a's top and bottom
    // are each a disk and a square with a hole; the wall is three bands (z < 0, the
    // middle, z > 1), the caps whole.
    let doc = Document::default();
    let a = cuboid([-2.0, -2.0, 0.0], [2.0, 2.0, 1.0], doc);
    let b = cyl([0.0, 0.0, -1.0], Z, X, 1.0, 3.0, doc);
    let (m, _) = imp(&a, &b).unwrap();
    assert_eq!(counts(&m, Side::A), vec![1, 1, 1, 1, 2, 2]);
    assert_eq!(counts(&m, Side::B), vec![1, 1, 3]);
    assert_eq!(sections(&m), 2);
    check(&m, &a, &b, [8, 5]);
    // a's top and bottom: the piece holding the square keeps the id, with the circle as
    // a hole; the disk is new, from it. Each circle is from a's face and the wall.
    let (body, changes) = only(&m, Side::A).finish(&doc).unwrap();
    for ((_, f), p) in m
        .pieces
        .iter()
        .filter(|((s, _), p)| *s == Side::A && p.len() == 2)
    {
        let split = changes.faces.split_of(*f).unwrap();
        assert_eq!(&split.pieces, p);
        assert_eq!(body.face(*f).unwrap().loops().len(), 2);
        let disk = p.iter().find(|x| *x != f).unwrap();
        assert_eq!(body.face(*disk).unwrap().loops().len(), 1);
    }
    let wall = b
        .faces()
        .find(|f| matches!(f.surface(), Surface::Cylinder(_)))
        .unwrap()
        .id();
    for &e in &m.cuts {
        let o = changes.edges.origins(e).unwrap();
        assert!(
            o.contains(Origin::Operand(Operand::from_raw(1), Entity::Face(wall))),
            "{o:?}"
        );
        assert!(o.iter().any(|x| matches!(x, Origin::Face(_))), "{o:?}");
    }
}

#[test]
fn a_cylinder_on_a_blocks_side_cuts_off_half_disks_and_the_wall_at_two_rulings() {
    // The unit cylinder about x = 2, y = 0, z from -1 to 2, against [-2, 2]^2 x [0, 1]:
    // a's top and bottom each lose a half disk; a's side x = 2 is cut by the rulings
    // y = +-1 into three strips. The half circles and the rulings close one loop on the
    // wall: the patch inside the block, and the rest of the wall around it (the patch its
    // hole).
    let doc = Document::default();
    let a = cuboid([-2.0, -2.0, 0.0], [2.0, 2.0, 1.0], doc);
    let b = cyl([2.0, 0.0, -1.0], Z, X, 1.0, 3.0, doc);
    let (m, _) = imp(&a, &b).unwrap();
    assert_eq!(counts(&m, Side::A), vec![1, 1, 1, 2, 2, 3]);
    assert_eq!(counts(&m, Side::B), vec![1, 1, 2]);
    check(&m, &a, &b, [10, 4]);
}

#[test]
fn a_plane_through_a_spheres_centre_cuts_it_into_three_lunes() {
    // The unit sphere against [0, 2] x [-2, 2]^2: the two half great circles at x = 0
    // run pole to pole. With the seam meridian at +x they bound three pieces: the half
    // x < 0, and the quarters either side of the seam (BO3 does not join across a seam:
    // BO4's merge does). b's face x = 0 loses the unit disk.
    let doc = Document::default();
    let a = ball([0.0; 3], 1.0, doc);
    let b = cuboid([0.0, -2.0, -2.0], [2.0, 2.0, 2.0], doc);
    let (m, _) = imp(&a, &b).unwrap();
    assert_eq!(counts(&m, Side::A), vec![3]);
    assert_eq!(counts(&m, Side::B), vec![1, 1, 1, 1, 1, 2]);
    check(&m, &a, &b, [3, 7]);
}

#[test]
fn two_spheres_cut_each_other_along_a_circle_through_the_chart() {
    // The circle x = 1/2 is no line in either sphere's (u, v): its pcurves map it through
    // each sphere's chart inverse (OnChart). It crosses a's seam meridian (at +x) twice, so
    // a's cap is two pieces and the rest one; b's seam is on its far side, so b's cap and
    // rest are one piece each.
    let doc = Document::default();
    let (a, b) = (ball([0.0; 3], 1.0, doc), ball([1.0, 0.0, 0.0], 1.0, doc));
    let (m, _) = imp(&a, &b).unwrap();
    assert_eq!(counts(&m, Side::A), vec![3]);
    assert_eq!(counts(&m, Side::B), vec![2]);
    check(&m, &a, &b, [3, 2]);
}

#[test]
fn a_plane_tangent_to_a_cylinder_cuts_the_wall_and_the_face_along_the_ruling() {
    // The unit cylinder z in [0, 1], seam at -x, against [1, 2] x [-1, 1] x [0, 1]: the
    // tangent ruling at x = 1 cuts the wall (rim to rim) and b's face x = 1 (bottom edge
    // to top edge) in two each; the rims are cut there, the caps not.
    let doc = Document::default();
    let a = cyl([0.0; 3], Z, [-1.0, 0.0, 0.0], 1.0, 1.0, doc);
    let b = cuboid([1.0, -1.0, 0.0], [2.0, 1.0, 1.0], doc);
    let (m, _) = imp(&a, &b).unwrap();
    assert_eq!(counts(&m, Side::A), vec![1, 1, 2]);
    assert_eq!(counts(&m, Side::B), vec![1, 1, 1, 1, 1, 2]);
    check(&m, &a, &b, [4, 7]);
}

#[test]
fn a_plane_tangent_to_a_sphere_cuts_nothing() {
    // The touch at (0, 1, 0) is inside both faces: nothing to cut (BO4 reads it).
    let doc = Document::default();
    let a = ball([0.0; 3], 1.0, doc);
    let b = cuboid([-2.0, 1.0, -2.0], [2.0, 2.0, 2.0], doc);
    let (m, _) = imp(&a, &b).unwrap();
    assert!(m.cuts.is_empty());
    assert_eq!(counts(&m, Side::A), vec![1]);
    check(&m, &a, &b, [1, 6]);
}

#[test]
fn the_imprint_is_deterministic() {
    let doc = Document::default();
    let a = ball([0.0; 3], 1.0, doc);
    let b = cuboid([0.0, -2.0, -2.0], [2.0, 2.0, 2.0], doc);
    let (m1, w1) = imp(&a, &b).unwrap();
    let (m2, w2) = imp(&a, &b).unwrap();
    assert_eq!(w1, w2);
    assert_eq!(m1.pieces, m2.pieces);
    assert_eq!(m1.cuts, m2.cuts);
    assert_eq!(format!("{:?}", m1.draft), format!("{:?}", m2.draft));
    let _ = doc;
}

#[test]
fn swapped_operands_cut_the_same_faces() {
    // The cylinder through the block with the operands swapped: the wall is now a's, in
    // three bands, and the block's top and bottom b's, each in two; b's copies record
    // where they came from.
    let doc = Document::default();
    let a = cyl([0.0, 0.0, -1.0], Z, X, 1.0, 3.0, doc);
    let b = cuboid([-2.0, -2.0, 0.0], [2.0, 2.0, 1.0], doc);
    let (m, _) = imp(&a, &b).unwrap();
    assert_eq!(counts(&m, Side::A), vec![1, 1, 3]);
    assert_eq!(counts(&m, Side::B), vec![1, 1, 1, 1, 2, 2]);
    check(&m, &a, &b, [5, 8]);
    let (_, changes) = only(&m, Side::B).finish(&doc).unwrap();
    for ((_, f), p) in m.pieces.iter().filter(|((s, _), _)| *s == Side::B) {
        for x in p {
            let o = changes.faces.origins(*x).unwrap();
            assert!(
                o.contains(Origin::Operand(Operand::from_raw(1), Entity::Face(*f))),
                "{o:?}"
            );
        }
    }
}
