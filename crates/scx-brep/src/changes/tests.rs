// Written for solidcorex. Apache-2.0.
//! The events each draft write records, the lineage the operation declares, the order of a
//! split's pieces, and `carry` across edits and compaction (63 §6.4's `Changes` oracles).
//!
//! The provenance tests of the pin (`solidcore-ops/src/tests.rs`, READ) are written there
//! against operations; here the same events are made by hand on a strip of square faces.

use std::collections::BTreeMap;

use scx_curve::{Curve, Curve2, Line, Line2};
use scx_num::cert::{Basis, Recorded};
use scx_num::extent1d::Extent1d;
use scx_num::frame::{Frame, Sense};
use scx_num::units::{Accuracy, Length};
use scx_num::vector2d::{Point2, Vec2};
use scx_num::vector3d::{Point3, Vec3};
use scx_surface::{Plane, Surface};

use crate::*;

fn unit() -> Extent1d {
    Extent1d::new(0.0, 1.0).unwrap()
}

fn plane(z: f64) -> Surface {
    let frame = Frame::new(Point3::new(0.0, 0.0, z), Vec3::Z, Vec3::X).unwrap();
    Surface::Plane(Plane::new(frame))
}

/// The straight edge from `p` to `q` over `[0, 1]`.
fn segment(d: &mut Draft, ends: [VertexId; 2]) -> EdgeId {
    let [p, q] = ends.map(|v| d.vertex(v).unwrap().point());
    let line = Line::new(p, q - p).unwrap();
    let shape = EdgeShape::Curve {
        curve: Curve::Line(line),
        interval: unit(),
    };
    d.add_edge(EdgeGeom::new(shape, ends)).unwrap()
}

/// A use of `edge` in the plane `z = 0`, whose uv is its xy.
fn use_of(d: &Draft, edge: EdgeId, sense: Sense) -> EdgeUse {
    let e = d.edge(edge).unwrap();
    let [p, q] = [e.start(), e.end()].map(|v| v.unwrap().point());
    let pc = Line2::new(Point2::new(p.x, p.y), Vec2::new(q.x - p.x, q.y - p.y)).unwrap();
    let geom = UseGeom::Pcurve {
        curve: Curve2::Line(pc),
        interval: unit(),
    };
    EdgeUse::new(edge, sense, geom)
}

/// The square loop over `bottom`, `right`, `top` and `left`, counter-clockwise.
fn square(d: &Draft, [bottom, right, top, left]: [EdgeId; 4]) -> LoopRecord {
    LoopRecord::new(vec![
        use_of(d, bottom, Sense::Forward),
        use_of(d, right, Sense::Forward),
        use_of(d, top, Sense::Reversed),
        use_of(d, left, Sense::Reversed),
    ])
}

/// `n` unit squares in a row along x, in the outside region's shell.
struct Strip {
    draft: Draft,
    shell: ShellId,
    faces: Vec<FaceId>,
    /// `(i, 0)` and `(i, 1)`.
    verts: Vec<[VertexId; 2]>,
    bottom: Vec<EdgeId>,
    top: Vec<EdgeId>,
    /// At `x = i`.
    rungs: Vec<EdgeId>,
}

fn strip(n: usize) -> Strip {
    let mut d = Draft::new();
    let shell = d.add_shell(ShellRecord::new(RegionId::OUTSIDE)).unwrap();
    let verts: Vec<[VertexId; 2]> = (0..=n)
        .map(|i| {
            [0.0, 1.0].map(|y| {
                d.add_vertex(VertexRecord::new(Point3::new(i as f64, y, 0.0)))
                    .unwrap()
            })
        })
        .collect();
    let rungs: Vec<EdgeId> = verts.iter().map(|&v| segment(&mut d, v)).collect();
    let bottom: Vec<EdgeId> = (0..n)
        .map(|i| segment(&mut d, [verts[i][0], verts[i + 1][0]]))
        .collect();
    let top: Vec<EdgeId> = (0..n)
        .map(|i| segment(&mut d, [verts[i][1], verts[i + 1][1]]))
        .collect();
    let faces = (0..n)
        .map(|i| {
            let lp = square(&d, [bottom[i], rungs[i + 1], top[i], rungs[i]]);
            d.add_face(FaceRecord::new(plane(0.0), [shell, shell], vec![lp]))
                .unwrap()
        })
        .collect();
    Strip {
        draft: d,
        shell,
        faces,
        verts,
        bottom,
        top,
        rungs,
    }
}

/// The strip, finished, and a draft of it.
fn edit_of(n: usize) -> (Strip, Brep) {
    let s = strip(n);
    let (body, _) = s.draft.clone().finish(&Document::default()).unwrap();
    let draft = body.edit();
    (Strip { draft, ..s }, body)
}

fn done(d: Draft) -> (Brep, Changes) {
    d.finish(&Document::default()).unwrap()
}

#[test]
fn a_built_body_is_all_created_and_free() {
    let s = strip(2);
    let (_, changes) = done(s.draft);
    assert_eq!(changes.faces.created.len(), 2);
    assert_eq!(changes.edges.created.len(), 7);
    assert_eq!(changes.vertices.created.len(), 6);
    assert!(
        changes
            .faces
            .created
            .iter()
            .all(|(_, o)| o.as_slice() == [Origin::Free])
    );
    assert!(changes.faces.replaced.is_empty() && changes.faces.removed.is_empty());
}

#[test]
fn an_edit_that_touches_nothing_changes_nothing() {
    let (mut s, _) = edit_of(2);
    let d = &mut s.draft;
    let f = s.faces[0];
    // Writes that change nothing, and radial order, which is not a record of its own.
    let _ = d.face_mut(f).unwrap();
    let ends = d.edge_mut(s.bottom[0]).unwrap().ends;
    d.edge_mut(s.bottom[0]).unwrap().ends = ends;
    let p = d.vertex_mut(s.verts[0][0]).unwrap().point;
    d.vertex_mut(s.verts[0][0]).unwrap().point = p;
    d.adjacency_mut(s.rungs[1]).unwrap().fins.reverse();
    let _ = d.region_mut(RegionId::OUTSIDE).unwrap();
    d.edit_face(f, |_| ()).unwrap();
    let (_, changes) = done(s.draft);
    assert!(changes.is_empty(), "{changes:?}");
    assert_eq!(changes, Changes::default());
}

#[test]
fn each_write_reports_its_touch() {
    let (mut s, _) = edit_of(3);
    let d = &mut s.draft;
    let [f0, f1, f2] = [s.faces[0], s.faces[1], s.faces[2]];
    d.face_mut(f2).unwrap().surface = plane(0.5);
    d.face_mut(f1).unwrap().loops[0].uses.rotate_left(1);
    // A boundary write, then a geometry write: geometry wins.
    d.face_mut(f0).unwrap().loops[0].uses.rotate_left(1);
    d.face_mut(f0).unwrap().surface = plane(0.25);
    let (e0, e1, e2) = (s.top[0], s.top[1], s.top[2]);
    let (curve, interval) = line_x();
    d.attach_curve(e0, curve, interval).unwrap();
    // Attaching the curve it has already: no change.
    let same = d.edge(s.top[1]).unwrap().shape().cloned();
    if let Some(EdgeShape::Curve { curve, interval }) = same {
        d.attach_curve(s.top[1], curve, interval).unwrap();
    }
    d.edge_mut(e1).unwrap().ends.swap(0, 1);
    d.edge_mut(e2).unwrap().accuracy = Some(std::sync::Arc::new(Recorded {
        acc: Accuracy::new(Length::from_mm(1e-6)).unwrap(),
        basis: Basis::Measured,
    }));
    d.vertex_mut(s.verts[3][1]).unwrap().point = Point3::new(3.0, 1.5, 0.0);
    let (_, changes) = done(s.draft);
    assert_eq!(
        changes.faces.replaced,
        [
            (f0, Touch::Geometry),
            (f1, Touch::Boundary),
            (f2, Touch::Geometry)
        ]
    );
    assert_eq!(
        changes.edges.replaced,
        [
            (e0, Touch::Geometry),
            (e1, Touch::Boundary),
            (e2, Touch::Geometry)
        ]
    );
    assert_eq!(
        changes.vertices.replaced,
        [(s.verts[3][1], Touch::Geometry)]
    );
    // Only the records written: the faces round the moved vertex are not listed.
    assert_eq!(changes.faces.touch(f1), Some(Touch::Boundary));
    assert!(changes.faces.created.is_empty() && changes.edges.removed.is_empty());
}

fn line_x() -> (Curve, Extent1d) {
    // The top edge's own line at twice the speed: the same points, a new curve.
    let l = Line::new(Point3::new(0.0, 1.0, 0.0), Vec3::new(2.0, 0.0, 0.0)).unwrap();
    (Curve::Line(l), Extent1d::new(0.0, 0.5).unwrap())
}

#[test]
fn adds_and_removes() {
    let (mut s, _) = edit_of(2);
    let d = &mut s.draft;
    let gone = d.remove_face(s.faces[1]).unwrap();
    assert_eq!(gone.loops.len(), 1);
    d.remove_edge(s.bottom[1]).unwrap();
    // Written, then removed: removed only.
    d.vertex_mut(s.verts[2][0]).unwrap().point = Point3::new(9.0, 0.0, 0.0);
    d.remove_vertex(s.verts[2][0]).unwrap();
    d.remove_vertex(s.verts[2][1]).unwrap();
    d.remove_edge(s.rungs[2]).unwrap();
    d.remove_edge(s.top[1]).unwrap();
    // Made and removed in the one edit: not there at all.
    let brief = d
        .add_vertex(VertexRecord::new(Point3::new(5.0, 5.0, 0.0)))
        .unwrap();
    d.remove_vertex(brief).unwrap();
    let v = d
        .add_vertex(VertexRecord::new(Point3::new(0.5, 0.5, 0.0)))
        .unwrap();
    d.shell_mut(s.shell).unwrap().lone_vertices.push(v);
    let (_, changes) = done(s.draft);
    assert_eq!(changes.faces.removed, [s.faces[1]]);
    let mut edges = vec![s.bottom[1], s.rungs[2], s.top[1]];
    edges.sort_unstable();
    assert_eq!(changes.edges.removed, edges);
    assert_eq!(changes.vertices.removed, s.verts[2]);
    assert!(changes.vertices.replaced.is_empty());
    assert_eq!(changes.vertices.created.len(), 1);
    assert_eq!(changes.vertices.created[0].0, v);
    assert_eq!(
        changes.vertices.origins(v).unwrap().as_slice(),
        [Origin::Free]
    );
}

/// One square cut down the middle at `x = 0.5`. `keep_left` says which half keeps the
/// face's id (and each cut edge's). Returns the draft and the halves, left then right.
fn cut(keep_left: bool) -> (Strip, Draft, [FaceId; 2], [[EdgeId; 2]; 2]) {
    let (s, _) = edit_of(1);
    let mut d = s.draft.clone();
    let (f, b, t) = (s.faces[0], s.bottom[0], s.top[0]);
    let mid = [0.0, 1.0].map(|y| {
        d.add_vertex(VertexRecord::new(Point3::new(0.5, y, 0.0)))
            .unwrap()
    });
    for &v in &mid {
        d.derive(v, Origin::Free).unwrap();
    }
    let m = segment(&mut d, mid);
    // Each cut edge: the half that keeps the id is rewritten, the other is new.
    let halves = |d: &mut Draft, e: EdgeId, y: usize| {
        let [a, z] = [s.verts[0][y], s.verts[1][y]];
        let (kept_ends, new_ends) = if keep_left {
            ([a, mid[y]], [mid[y], z])
        } else {
            ([mid[y], z], [a, mid[y]])
        };
        let kept_line = {
            let [p, q] = kept_ends.map(|v| d.vertex(v).unwrap().point());
            Line::new(p, q - p).unwrap()
        };
        let g = d.edge_mut(e).unwrap();
        g.ends = kept_ends;
        g.shape = EdgeShape::Curve {
            curve: Curve::Line(kept_line),
            interval: unit(),
        };
        let n = segment(d, new_ends);
        d.derive(n, Origin::Edge(e)).unwrap();
        if keep_left { [e, n] } else { [n, e] }
    };
    let bs = halves(&mut d, b, 0);
    let ts = halves(&mut d, t, 1);
    let (l, r) = (s.rungs[0], s.rungs[1]);
    let left = [bs[0], m, ts[0], l];
    let right = [bs[1], r, ts[1], m];
    let (kept, new) = if keep_left {
        (left, right)
    } else {
        (right, left)
    };
    let lp = square(&d, kept);
    d.edit_face(f, |rec| rec.loops = vec![lp]).unwrap();
    let lp = square(&d, new);
    let g = d
        .add_face(FaceRecord::new(plane(0.0), [s.shell, s.shell], vec![lp]))
        .unwrap();
    d.derive(g, Origin::Face(f)).unwrap();
    let faces = if keep_left { [f, g] } else { [g, f] };
    (s, d, faces, [bs, ts])
}

#[test]
fn a_split_lists_its_pieces_by_what_bounds_them() {
    for keep_left in [true, false] {
        let (s, d, [left, right], [bs, ts]) = cut(keep_left);
        let (_, changes) = done(d);
        let f = s.faces[0];
        // The halves are listed left, then right, whichever kept the id: the left is
        // bounded by the rung at x = 0, which is numbered before the one at x = 1.
        assert_eq!(
            changes.faces.split,
            [Split {
                from: f,
                pieces: vec![left, right],
            }],
            "keep_left {keep_left}"
        );
        assert_eq!(changes.faces.touch(f), Some(Touch::Boundary));
        let new = if keep_left { right } else { left };
        assert_eq!(
            changes.faces.origins(new).unwrap().as_slice(),
            [Origin::Face(f)]
        );
        // Each cut edge: its halves, by their ends (the old vertex's side first).
        for (e, halves) in [(s.bottom[0], bs), (s.top[0], ts)] {
            let split = changes.edges.split_of(e).unwrap();
            assert_eq!(split.pieces, halves, "keep_left {keep_left}");
            assert_eq!(changes.edges.touch(e), Some(Touch::Geometry));
        }
        // The cut edge and its vertices are free.
        assert_eq!(changes.edges.created.len(), 3);
        assert_eq!(changes.vertices.created.len(), 2);
    }
}

#[test]
fn pieces_bounded_alike_are_ordered_by_where_they_lie() {
    // Two new lone-vertex faces cut from one, bounded by new free edges alone: the same
    // key, so their uv decides.
    let (s, _) = edit_of(1);
    let mut d = s.draft;
    let f = s.faces[0];
    let lone = |d: &mut Draft, x: f64| {
        let v = d
            .add_vertex(VertexRecord::new(Point3::new(x, 0.5, 0.0)))
            .unwrap();
        let e = d
            .add_edge(EdgeGeom::new(EdgeShape::Collapsed, [v, v]))
            .unwrap();
        let u = EdgeUse::new(e, Sense::Forward, UseGeom::Point(Point2::new(x, 0.5)));
        let g = d
            .add_face(FaceRecord::new(
                plane(0.0),
                [s.shell, s.shell],
                vec![LoopRecord::new(vec![u])],
            ))
            .unwrap();
        d.derive(g, Origin::Face(f)).unwrap();
        g
    };
    let right = lone(&mut d, 0.75);
    let left = lone(&mut d, 0.25);
    d.remove_face(f).unwrap();
    let (_, changes) = done(d);
    assert_eq!(changes.faces.split_of(f).unwrap().pieces, [left, right]);
    // The face cut is gone, and listed as removed.
    assert_eq!(changes.faces.removed, [f]);
}

#[test]
fn merges_name_the_survivor_through_chains() {
    let (mut s, _) = edit_of(4);
    let d = &mut s.draft;
    let [f0, f1, f2, f3] = [s.faces[0], s.faces[1], s.faces[2], s.faces[3]];
    // f0 into f1, then f1 into f2: f2 took in both.
    assert!(d.merge_face(f0, f1).is_some());
    assert!(d.merge_face(f1, f2).is_some());
    assert!(d.merge_face(f1, f2).is_none(), "f1 is gone");
    assert!(d.merge_face(f2, f2).is_none(), "one face");
    // A merge into a new face makes the absorbed one an origin of it.
    let lp = square(d, [s.bottom[3], s.rungs[4], s.top[3], s.rungs[3]]);
    let rec = FaceRecord::new(plane(0.0), [s.shell, s.shell], vec![lp]);
    d.remove_face(f3).unwrap();
    let n = d.add_face(rec).unwrap();
    // The rung between f0 and f1 merged into f1's right rung: an edge merge, with no edge
    // removed.
    assert!(d.merge_edge(s.rungs[1], s.rungs[2]).is_some());
    // A vertex merged into one later removed: removed, with the edges at x = 0.
    for e in [s.rungs[0], s.bottom[0], s.top[0]] {
        d.remove_edge(e).unwrap();
    }
    assert!(d.merge_vertex(s.verts[0][0], s.verts[0][1]).is_some());
    d.remove_vertex(s.verts[0][1]).unwrap();
    let mut d2 = s.draft.clone();
    let (_, changes) = done(s.draft);
    assert_eq!(
        changes.faces.merged,
        [Merge {
            survivor: f2,
            absorbed: vec![f0, f1],
        }]
    );
    assert_eq!(changes.faces.removed, [f3]);
    assert_eq!(
        changes.edges.merged,
        [Merge {
            survivor: s.rungs[2],
            absorbed: vec![s.rungs[1]],
        }]
    );
    let mut es = vec![s.rungs[0], s.bottom[0], s.top[0]];
    es.sort_unstable();
    assert_eq!(changes.edges.removed, es);
    let mut vs = vec![s.verts[0][0], s.verts[0][1]];
    vs.sort_unstable();
    assert_eq!(changes.vertices.removed, vs);
    // Into a new face: an origin, not a merge.
    assert!(d2.merge_face(f2, n).is_some());
    let (_, changes) = done(d2);
    assert!(changes.faces.merged.is_empty());
    assert_eq!(
        changes.faces.origins(n).unwrap().as_slice(),
        [Origin::Face(f0), Origin::Face(f1), Origin::Face(f2)]
    );
}

#[test]
fn derive_refuses_what_it_cannot_name() {
    let (mut s, _) = edit_of(1);
    let d = &mut s.draft;
    let f = s.faces[0];
    let refused = d.derive(f, Origin::Free).unwrap_err();
    assert_eq!(refused, LineageError::NotCreated { entity: f.into() });
    assert_eq!(
        (refused.kind(), refused.name()),
        (scx_num::error::ErrorKind::Refused, "not_created")
    );
    let v = d
        .add_vertex(VertexRecord::new(Point3::new(2.0, 0.0, 0.0)))
        .unwrap();
    let dead = d
        .add_vertex(VertexRecord::new(Point3::new(3.0, 0.0, 0.0)))
        .unwrap();
    d.remove_vertex(dead).unwrap();
    let refused = d.derive(v, Origin::Vertex(dead)).unwrap_err();
    assert_eq!(refused.name(), "unknown_origin");
    let refused = d
        .derive(v, Origin::Offset(Entity::Shell(s.shell)))
        .unwrap_err();
    assert_eq!(refused.name(), "unknown_origin");
    assert_eq!(
        d.derive(dead, Origin::Free).unwrap_err().name(),
        "not_created"
    );
    // An origin naming a new entity stands for its origins; an offset of it, for their
    // offsets. Operands are another body's ids, not checked.
    let w = d
        .add_vertex(VertexRecord::new(Point3::new(4.0, 0.0, 0.0)))
        .unwrap();
    d.derive(v, Origin::Vertex(s.verts[0][0])).unwrap();
    d.derive(
        v,
        Origin::Operand(Operand::from_raw(1), Entity::Vertex(VertexId::from_raw(9))),
    )
    .unwrap();
    d.derive(w, Origin::Offset(Entity::Vertex(v))).unwrap();
    let (_, changes) = done(s.draft);
    assert_eq!(
        changes.vertices.origins(w).unwrap().as_slice(),
        [
            Origin::Offset(Entity::Vertex(s.verts[0][0])),
            Origin::Operand(Operand::from_raw(1), Entity::Vertex(VertexId::from_raw(9))),
        ]
    );
}

#[test]
fn coincidences_follow_the_survivor() {
    let (mut s, _) = edit_of(2);
    let d = &mut s.draft;
    let op = Operand::from_raw(1);
    let theirs = FaceId::from_raw(7);
    d.coincident(op, theirs, s.faces[0]).unwrap();
    d.merge_face(s.faces[0], s.faces[1]).unwrap();
    let absent = d.coincident(op, theirs, s.faces[0]).unwrap_err();
    assert_eq!(absent.entity, Entity::Face(s.faces[0]));
    let (_, changes) = done(s.draft);
    assert_eq!(
        changes.faces.coincident,
        [Coincidence {
            operand: op,
            entity: theirs,
            onto: s.faces[1],
        }]
    );
}

#[test]
fn the_same_edit_twice_gives_equal_changes() {
    let a = done(cut(true).1).1;
    let b = done(cut(true).1).1;
    assert_eq!(a, b);
    assert!(a.faces.split.windows(2).all(|w| w[0].from < w[1].from));
    assert!(a.edges.split.windows(2).all(|w| w[0].from < w[1].from));
    assert!(a.edges.created.windows(2).all(|w| w[0].0 < w[1].0));
}

/// Tags as sets of numbers, joined at every merge and coincidence: the pin's provenance
/// test rule.
struct Join;

impl Carry<Vec<u32>> for Join {
    fn split(&self, from: &Vec<u32>) -> Option<Vec<u32>> {
        Some(from.clone())
    }

    fn merge(&self, values: &[&Vec<u32>]) -> Option<Vec<u32>> {
        let mut all: Vec<u32> = values.iter().flat_map(|v| v.iter().copied()).collect();
        all.sort_unstable();
        all.dedup();
        Some(all)
    }

    fn coincident(&self, onto: Option<&Vec<u32>>, from: &Vec<u32>) -> Option<Vec<u32>> {
        let mut all = onto.cloned().unwrap_or_default();
        all.extend(from);
        all.sort_unstable();
        all.dedup();
        Some(all)
    }
}

fn tags<I: Tracked>(ids: &[I]) -> BTreeMap<I, Vec<u32>> {
    ids.iter()
        .enumerate()
        .map(|(k, &id)| (id, vec![k as u32]))
        .collect()
}

#[test]
fn carry_hands_a_split_face_to_both_pieces() {
    let (s, d, [left, right], [bs, _]) = cut(false);
    let (_, changes) = done(d);
    let own = tags(&s.faces);
    let carried = carry(&changes.faces, &own, &[], &Join);
    assert_eq!(carried.get(&left), Some(&vec![0]));
    assert_eq!(carried.get(&right), Some(&vec![0]));
    // The edges: both halves of the cut bottom carry, and the new middle edge nothing.
    let own = tags(&s.bottom);
    let carried = carry(&changes.edges, &own, &[], &Join);
    assert_eq!(carried.len(), 2);
    assert!(bs.iter().all(|e| carried.get(e) == Some(&vec![0])));
}

#[test]
fn carry_joins_merges_and_coincidences() {
    let (mut s, _) = edit_of(4);
    let d = &mut s.draft;
    let f = &s.faces;
    d.merge_face(f[0], f[1]).unwrap();
    d.coincident(Operand::from_raw(1), FaceId::from_raw(0), f[3])
        .unwrap();
    d.remove_face(f[2]).unwrap();
    // A face from two operands' faces (a union's coincident faces): merged.
    let lp = square(d, [s.bottom[2], s.rungs[3], s.top[2], s.rungs[2]]);
    let both = d
        .add_face(FaceRecord::new(plane(0.0), [s.shell, s.shell], vec![lp]))
        .unwrap();
    d.derive(both, Origin::Face(f[2])).unwrap();
    d.derive(
        both,
        Origin::Operand(Operand::from_raw(1), Entity::Face(FaceId::from_raw(1))),
    )
    .unwrap();
    let (_, changes) = done(s.draft);
    let own = tags(f);
    let tool: BTreeMap<FaceId, Vec<u32>> = [
        (FaceId::from_raw(0), vec![10]),
        (FaceId::from_raw(1), vec![11]),
    ]
    .into();
    let carried = carry(&changes.faces, &own, &[&BTreeMap::new(), &tool], &Join);
    assert_eq!(carried.get(&f[0]), None);
    assert_eq!(carried.get(&f[1]), Some(&vec![0, 1]));
    assert_eq!(carried.get(&f[2]), None);
    assert_eq!(carried.get(&f[3]), Some(&vec![3, 10]));
    assert_eq!(carried.get(&both), Some(&vec![2, 11]));
}

#[test]
fn carry_takes_an_offset_and_leaves_a_free_entity_bare() {
    let (mut s, _) = edit_of(1);
    let d = &mut s.draft;
    let f = s.faces[0];
    let lp = square(d, [s.bottom[0], s.rungs[1], s.top[0], s.rungs[0]]);
    let wall = d
        .add_face(FaceRecord::new(
            plane(0.0),
            [s.shell, s.shell],
            vec![lp.clone()],
        ))
        .unwrap();
    d.derive(wall, Origin::Offset(f.into())).unwrap();
    let free = d
        .add_face(FaceRecord::new(plane(0.0), [s.shell, s.shell], vec![lp]))
        .unwrap();
    // A face made along an edge: a cross-kind origin carries nothing between kinds.
    d.derive(free, Origin::Edge(s.bottom[0])).unwrap();
    let (_, changes) = done(s.draft);
    // An offset is not a piece: no split.
    assert!(changes.faces.split.is_empty());
    let carried = carry(&changes.faces, &tags(&s.faces), &[], &Join);
    assert_eq!(carried.get(&wall), Some(&vec![0]));
    assert_eq!(carried.get(&f), Some(&vec![0]));
    assert_eq!(carried.get(&free), None);
}

#[test]
fn carry_across_an_edit_then_compact() {
    let (mut s, _) = edit_of(3);
    let d = &mut s.draft;
    d.remove_face(s.faces[0]).unwrap();
    d.remove_edge(s.bottom[0]).unwrap();
    d.remove_edge(s.top[0]).unwrap();
    d.remove_edge(s.rungs[0]).unwrap();
    for v in s.verts[0] {
        d.remove_vertex(v).unwrap();
    }
    let (body, changes) = done(s.draft);
    let own = tags(&s.faces);
    let carried = carry(&changes.faces, &own, &[], &Join);
    let (compact, map) = body.compact();
    assert!(!map.is_identity());
    let renumbered = map.renumber(carried);
    // Faces were not renumbered (face 0's slot... is empty, so the others move down).
    assert_eq!(renumbered.len(), 2);
    for (id, tag) in &renumbered {
        assert!(compact.face(*id).is_some());
        let old = s.faces[tag[0] as usize];
        assert_eq!(map.face(old), Some(*id));
    }
    let edges = map.renumber(carry(&changes.edges, &tags(&s.rungs), &[], &Join));
    assert_eq!(
        edges.values().cloned().collect::<Vec<_>>(),
        [vec![1], vec![2], vec![3]]
    );
}

#[test]
fn edit_face_keeps_the_fins_in_step() {
    let (mut s, _) = edit_of(2);
    let d = &mut s.draft;
    let f = s.faces[0];
    // Swap the face onto the other square's edges: its fins leave the old ones.
    let lp = square(d, [s.bottom[1], s.rungs[2], s.top[1], s.rungs[1]]);
    d.edit_face(f, |rec| rec.loops = vec![lp]).unwrap();
    d.remove_face(s.faces[1]).unwrap();
    for e in [s.bottom[0], s.top[0], s.rungs[0]] {
        assert_eq!(d.edge(e).unwrap().fins().count(), 0);
    }
    let fins: Vec<Fin> = d.edge(s.rungs[1]).unwrap().fins().collect();
    assert_eq!(fins, [Fin::new(f, Sense::Reversed)]);
    let (_, changes) = done(s.draft);
    assert_eq!(changes.faces.touch(f), Some(Touch::Boundary));

    // A seam: one edge used twice by the face, in both senses. Dropping one use drops
    // the fin of that sense only.
    let (mut s, _) = edit_of(1);
    let d = &mut s.draft;
    let f = s.faces[0];
    let r = s.rungs[0];
    let back = use_of(d, r, Sense::Forward);
    d.edit_face(f, |rec| rec.loops[0].uses.push(back)).unwrap();
    let senses = |d: &Draft| -> Vec<bool> {
        d.edge(r)
            .unwrap()
            .fins()
            .map(|fin| fin.sense == Sense::Forward)
            .collect()
    };
    assert_eq!(senses(d), [false, true]);
    d.edit_face(f, |rec| {
        rec.loops[0]
            .uses
            .retain(|u| !(u.edge == r && u.sense == Sense::Reversed));
    })
    .unwrap();
    assert_eq!(senses(d), [true]);
    assert_eq!(d.edit_face(FaceId::from_raw(40), |_| ()), None);
}
