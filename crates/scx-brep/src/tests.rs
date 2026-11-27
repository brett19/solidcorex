// Written for solidcorex. Apache-2.0.
//! Unit tests of the draft's rules: sharing, the cell resets, pending curves, the checks
//! `finish` runs, removal and compaction. The bodies here are structural (a plane face
//! with a lone vertex); the integration tests walk a block and a cylinder.

use scx_curve::{Curve, Line};
use scx_num::error::ErrorKind;
use scx_num::extent1d::Extent1d;
use scx_num::frame::{Frame, Sense};
use scx_num::vector2d::Point2;
use scx_num::vector3d::{Point3, Vec3};
use scx_surface::{Plane, Surface};

use crate::*;

fn plane() -> Surface {
    Surface::Plane(Plane::new(Frame::WORLD))
}

/// A sheet: one plane face in the outside region's shell, its one loop a lone vertex on a
/// collapsed edge.
struct Sheet {
    draft: Draft,
    shell: ShellId,
    face: FaceId,
    edge: EdgeId,
    vertex: VertexId,
}

fn sheet() -> Sheet {
    let mut draft = Draft::new();
    let shell = draft
        .add_shell(ShellRecord::new(RegionId::OUTSIDE))
        .unwrap();
    let vertex = draft
        .add_vertex(VertexRecord::new(Point3::new(0.0, 0.0, 0.0)))
        .unwrap();
    let edge = draft
        .add_edge(EdgeGeom::new(EdgeShape::Collapsed, [vertex, vertex]))
        .unwrap();
    let lone = EdgeUse::new(edge, Sense::Forward, UseGeom::Point(Point2::new(0.0, 0.0)));
    let face = draft
        .add_face(FaceRecord::new(
            plane(),
            [shell, shell],
            vec![LoopRecord::new(vec![lone])],
        ))
        .unwrap();
    Sheet {
        draft,
        shell,
        face,
        edge,
        vertex,
    }
}

fn line(len: f64) -> (Curve, Extent1d) {
    let l = Line::new(Point3::new(0.0, 0.0, 0.0), Vec3::new(1.0, 0.0, 0.0)).unwrap();
    (Curve::Line(l), Extent1d::new(0.0, len).unwrap())
}

#[test]
fn an_empty_draft_finishes_to_the_outside_alone() {
    let (body, changes) = Draft::new().finish(&Document::default()).unwrap();
    assert_eq!(changes, Changes::default());
    assert_eq!(body.regions().len(), 1);
    assert!(body.outside().is_outside());
    assert!(!body.outside().is_solid());
    assert_eq!(
        (body.shells().len(), body.faces().len(), body.edges().len()),
        (0, 0, 0)
    );
    assert_eq!(body.resolution(), Document::default().resolution());
}

#[test]
fn add_face_appends_its_fins() {
    let s = sheet();
    let adj = s.draft.edge(s.edge).unwrap().adjacency();
    assert_eq!(adj.fins, [Fin::new(s.face, Sense::Forward)]);
    let (body, _) = s.draft.finish(&Document::default()).unwrap();
    let edge = body.edge(s.edge).unwrap();
    assert_eq!(edge.shape(), &EdgeShape::Collapsed);
    let u = edge.uses().next().unwrap();
    assert_eq!(
        (u.face().id(), u.position(), u.lp().index()),
        (s.face, 0, 0)
    );
    assert_eq!(u.start().id(), s.vertex);
    // A one-use loop steps to itself.
    assert_eq!(u.next().position(), 0);
    assert_eq!(u.prev().position(), 0);
    // A sheet face has both sides in one shell: the shell lists it twice.
    let sides: Vec<_> = body
        .shell(s.shell)
        .unwrap()
        .faces()
        .map(|(f, side)| (f.id(), side))
        .collect();
    assert_eq!(sides, [(s.face, FaceSide::Front), (s.face, FaceSide::Back)]);
}

#[test]
fn an_edit_shares_until_written_and_writes_one_face() {
    let s = sheet();
    let (body, _) = s.draft.finish(&Document::default()).unwrap();
    let mut d = body.edit();
    assert!(d.tables().faces.shares(&body.tables().faces));
    assert!(d.tables().vertices.shares(&body.tables().vertices));
    let f = d.face_mut(s.face).unwrap();
    f.sides = [s.shell, s.shell];
    // The face table is copied (its slots are `Arc`s), and the one face record with it;
    // the other tables stay shared.
    assert!(!d.tables().faces.shares(&body.tables().faces));
    assert!(d.tables().edges.shares(&body.tables().edges));
    let before = body.tables().faces.get(s.face).unwrap();
    let after = d.tables().faces.get(s.face).unwrap();
    assert!(!std::sync::Arc::ptr_eq(before, after));
}

#[test]
fn into_draft_writes_in_place_when_unshared() {
    let s = sheet();
    let (body, _) = s.draft.finish(&Document::default()).unwrap();
    let before: *const EdgeGeom = body.tables().edges.get(s.edge).unwrap();
    let mut d = body.into_draft();
    let after: *const EdgeGeom = d.edge_mut(s.edge).unwrap();
    assert_eq!(before, after);
    // A face the draft started with is copied once on its first write, since `Changes`
    // keeps it as it was (BR3); a face the draft made is written in place.
    let f = d
        .add_face(d.face(s.face).unwrap().record().clone())
        .unwrap();
    let made: *const FaceRecord = &raw const **d.tables().faces.get(f).unwrap();
    let written: *const FaceRecord = &raw const *d.face_mut(f).unwrap();
    assert_eq!(made, written);
}

#[test]
fn a_vertex_or_edge_write_copies_no_face_with_empty_cells() {
    let s = sheet();
    let (body, _) = s.draft.finish(&Document::default()).unwrap();
    let mut d = body.edit();
    d.vertex_mut(s.vertex).unwrap().point = Point3::new(1.0, 0.0, 0.0);
    d.edge_mut(s.edge).unwrap().accuracy = None;
    // No cell is filled in BR1, so no face record is replaced.
    assert!(d.tables().faces.shares(&body.tables().faces));
    // The adjacency write copies only the adjacency table.
    d.adjacency_mut(s.edge).unwrap().fins.reverse();
    assert!(d.tables().faces.shares(&body.tables().faces));
    assert!(!d.tables().adjacency.shares(&body.tables().adjacency));
}

#[test]
fn a_pending_edge_has_no_shape_until_attached() {
    let mut s = sheet();
    let v = s.vertex;
    // The wire runs from the lone vertex to a second one, where its line will end.
    let w = s
        .draft
        .add_vertex(VertexRecord::new(Point3::new(1.0, 0.0, 0.0)))
        .unwrap();
    let e = s.draft.add_edge_pending([v, w]).unwrap();
    assert_eq!(s.draft.edge(e).unwrap().shape(), None);
    assert_eq!(
        s.draft.edge(s.edge).unwrap().shape(),
        Some(&EdgeShape::Collapsed)
    );
    let shell = s.draft.shell_mut(s.shell).unwrap();
    shell.wires.push(e);
    let refused = s.draft.finish(&Document::default()).unwrap_err();
    assert_eq!(refused.cause, FinishCause::MissingCurve { edge: e });
    assert_eq!(
        (refused.kind(), refused.name()),
        (ErrorKind::Defect, "missing_curve")
    );
    let mut d = refused.draft;
    let (curve, interval) = line(1.0);
    d.attach_curve(e, curve, interval).unwrap();
    assert!(matches!(
        d.edge(e).unwrap().shape(),
        Some(EdgeShape::Curve { .. })
    ));
    let absent = d
        .attach_curve(EdgeId::from_raw(99), line(1.0).0, line(1.0).1)
        .unwrap_err();
    assert_eq!(absent.entity, Entity::Edge(EdgeId::from_raw(99)));
    let (body, _) = d.finish(&Document::default()).unwrap();
    let wire = body.shell(s.shell).unwrap().wires().next().unwrap();
    assert!(wire.is_wire());
    assert_eq!(wire.id(), e);
}

#[test]
fn finish_refuses_dangling_references_lowest_first() {
    let mut s = sheet();
    s.draft.remove_vertex(s.vertex).unwrap();
    let refused = s.draft.finish(&Document::default()).unwrap_err();
    assert_eq!(
        refused.cause,
        FinishCause::Structure(StructureCause::Dangling {
            from: Entity::Edge(s.edge),
            to: Entity::Vertex(s.vertex),
        })
    );
    assert_eq!(refused.kind(), ErrorKind::Refused);
    assert_eq!(refused.name(), "dangling_reference");
    assert_eq!(
        refused.to_string(),
        "the draft is not a body: e0 names v0, which is not there"
    );
}

#[test]
fn finish_refuses_a_fin_without_its_use() {
    let mut s = sheet();
    s.draft.adjacency_mut(s.edge).unwrap().fins[0].sense = Sense::Reversed;
    let refused = s.draft.finish(&Document::default()).unwrap_err();
    assert_eq!(refused.name(), "fin_without_use");
}

#[test]
fn finish_refuses_a_solid_outside() {
    let mut d = Draft::new();
    d.region_mut(RegionId::OUTSIDE).unwrap().solid = true;
    let refused = d.finish(&Document::default()).unwrap_err();
    assert_eq!(
        refused.cause,
        FinishCause::Structure(StructureCause::Outside)
    );
    let mut d = refused.draft;
    assert_eq!(d.remove_region(RegionId::OUTSIDE), None);
}

#[test]
fn draft_views_answer_options_for_what_is_missing() {
    let mut s = sheet();
    s.draft.remove_shell(s.shell).unwrap();
    let face = s.draft.face(s.face).unwrap();
    assert!(face.shell(FaceSide::Front).is_none());
    s.draft.remove_edge(s.edge).unwrap();
    let face = s.draft.face(s.face).unwrap();
    assert!(face.uses().next().unwrap().edge().is_none());
    assert!(face.uses().next().unwrap().start().is_none());
}

#[test]
fn remove_face_drops_its_fins() {
    let mut s = sheet();
    let face = s.draft.remove_face(s.face).unwrap();
    assert_eq!(face.loops.len(), 1);
    assert!(s.draft.edge(s.edge).unwrap().is_wire());
    assert!(s.draft.face(s.face).is_none());
    assert_eq!(s.draft.faces().len(), 0);
}

#[test]
fn compact_renumbers_densely_in_order() {
    let mut s = sheet();
    // Leave empty slots: a dead vertex, edge, face and shell before the live ones.
    let dead_v = s
        .draft
        .add_vertex(VertexRecord::new(Point3::new(5.0, 0.0, 0.0)))
        .unwrap();
    s.draft.remove_vertex(dead_v).unwrap();
    let (body, _) = s.draft.finish(&Document::default()).unwrap();
    let (same, map) = body.compact();
    assert!(map.is_identity() || same.vertex_slots() == 1);

    let mut d = Draft::new();
    let dead_shell = d.add_shell(ShellRecord::new(RegionId::OUTSIDE)).unwrap();
    let v0 = d
        .add_vertex(VertexRecord::new(Point3::new(0.0, 0.0, 0.0)))
        .unwrap();
    let v1 = d
        .add_vertex(VertexRecord::new(Point3::new(1.0, 0.0, 0.0)))
        .unwrap();
    let dead_edge = d
        .add_edge(EdgeGeom::new(EdgeShape::Collapsed, [v0, v0]))
        .unwrap();
    let shell = d.add_shell(ShellRecord::new(RegionId::OUTSIDE)).unwrap();
    let e = d
        .add_edge(EdgeGeom::new(EdgeShape::Collapsed, [v1, v1]))
        .unwrap();
    let lone = EdgeUse::new(e, Sense::Forward, UseGeom::Point(Point2::new(1.0, 0.0)));
    let f = d
        .add_face(FaceRecord::new(
            plane(),
            [shell, shell],
            vec![LoopRecord::new(vec![lone])],
        ))
        .unwrap();
    d.remove_shell(dead_shell).unwrap();
    d.remove_edge(dead_edge).unwrap();
    d.remove_vertex(v0).unwrap();
    let (body, _) = d.finish(&Document::default()).unwrap();
    let (compact, map) = body.compact();
    assert!(!map.is_identity());
    assert_eq!(map.shell(shell), Some(ShellId::from_raw(0)));
    assert_eq!(map.shell(dead_shell), None);
    assert_eq!(map.edge(e), Some(EdgeId::from_raw(0)));
    assert_eq!(map.vertex(v1), Some(VertexId::from_raw(0)));
    assert_eq!(map.face(f), Some(f));
    assert_eq!((compact.edge_slots(), compact.vertex_slots()), (1, 1));
    let face = compact.face(f).unwrap();
    assert_eq!(face.shell(FaceSide::Front).id(), ShellId::from_raw(0));
    let u = face.uses().next().unwrap();
    assert_eq!(u.edge().id(), EdgeId::from_raw(0));
    assert_eq!(u.start().point(), Point3::new(1.0, 0.0, 0.0));
    assert_eq!(
        compact.edge(EdgeId::from_raw(0)).unwrap().fins().next(),
        Some(Fin::new(f, Sense::Forward))
    );
}

#[test]
fn record_sizes_stay_small() {
    use core::mem::size_of;
    // A face record is its surface by value plus sides and loops; a use its pcurve plus
    // 16 bytes; a vertex its point plus 8.
    assert!(size_of::<FaceRecord>() <= size_of::<Surface>() + 32);
    assert!(size_of::<EdgeUse>() <= size_of::<UseGeom>() + 24);
    assert!(size_of::<VertexRecord>() <= 32);
    assert!(size_of::<EdgeGeom>() <= size_of::<EdgeShape>() + 24);
    assert_eq!(size_of::<Fin>(), 8);
    assert_eq!(size_of::<EdgeAdj>(), 24);
}
