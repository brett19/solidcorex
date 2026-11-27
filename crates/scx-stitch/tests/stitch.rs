// Written for solidcorex. Apache-2.0.
//! `stitch` on quad sheets of known placement (74 §4.3, ST1): vertices identified within
//! coincidence and not beyond, lineage per operand, the accuracy raise, the refusals, the
//! pending edge stage, and determinism under re-run and permutation.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

use std::sync::Arc;

use scx_brep::op::{Cx, NoTrace, Scratch, Settings, WarningKind};
use scx_brep::{Brep, Document, Draft, Entity, Operand, Origin, VertexId};
use scx_num::budget::Stop;
use scx_num::cert::{Basis, Recorded};
use scx_num::math::Affine;
use scx_num::units::{Accuracy, Length, Resolution};
use scx_num::vector3d::{Point3, Vec3};
use scx_stitch::{StitchError, StitchRefusal, StitchUnsupported, Stitched, Want, stitch};
use scx_testkit::bodies::{Kind, generate};

const ZONE: f64 = 1.0e-5;

/// The unit quad sheet `[0,1]² × {0}`, moved by `d`.
fn sheet_draft(d: Vec3) -> Draft {
    let mut draft = generate(Kind::QuadSheet, 0, 1).unwrap().draft;
    draft
        .transform(&Affine::translation(d), &Document::default())
        .unwrap();
    draft
}

fn finish(d: Draft) -> Brep {
    d.finish(&Document::default())
        .map_err(|u| u.cause)
        .unwrap()
        .0
}

fn sheet(d: Vec3) -> Brep {
    finish(sheet_draft(d))
}

/// `sheet(d)` with the vertex at `p` given accuracy `acc`.
fn sheet_with(d: Vec3, p: Point3, acc: f64) -> Brep {
    let mut draft = sheet_draft(d);
    let v = draft
        .vertices()
        .find(|v| v.point().distance(p) < 1e-12)
        .map(scx_brep::Vertex::id)
        .unwrap();
    draft.vertex_mut(v).unwrap().accuracy = Some(Arc::new(Recorded {
        acc: Accuracy::new(Length::from_mm(acc)).unwrap(),
        basis: Basis::Measured,
    }));
    finish(draft)
}

fn run(sheets: &[&Brep]) -> Result<Stitched, StitchError> {
    run_in(Document::default(), sheets)
}

fn run_in(doc: Document, sheets: &[&Brep]) -> Result<Stitched, StitchError> {
    let mut scratch = Scratch::new();
    let settings = Settings::new();
    let stop = Stop::new();
    let mut cx = Cx::new(&doc, &settings, &mut scratch, &stop, &NoTrace);
    stitch(sheets, Want::Sheets, &mut cx)
}

fn points(b: &Brep) -> Vec<[u64; 3]> {
    let mut v: Vec<[u64; 3]> = b
        .vertices()
        .map(|v| {
            let p = v.point();
            [p.x.to_bits(), p.y.to_bits(), p.z.to_bits()]
        })
        .collect();
    v.sort_unstable();
    v
}

fn vertex_at(b: &Brep, p: Point3) -> VertexId {
    b.vertices()
        .find(|v| v.point().distance(p) < 1e-9)
        .map(scx_brep::Vertex::id)
        .unwrap()
}

#[test]
fn sheets_touching_at_a_corner_share_one_vertex() {
    let a = sheet(Vec3::new(0.0, 0.0, 0.0));
    let b = sheet(Vec3::new(1.0, 1.0, 0.0));
    let out = run(&[&a, &b]).unwrap();
    let body = &out.done.body;
    assert_eq!(body.vertices().count(), 7);
    assert_eq!(body.edges().count(), 8);
    assert_eq!(body.faces().count(), 2);
    let corner = vertex_at(&a, Point3::new(1.0, 1.0, 0.0));
    assert_eq!(out.report.identified.len(), 1);
    let id = &out.report.identified[0];
    assert_eq!(id.vertex, corner, "sheet 0's vertex keeps its id");
    assert_eq!(id.members.len(), 2);
    assert_eq!(id.members[0], (Operand::from_raw(0), corner));
    assert_eq!(id.members[1].0, Operand::from_raw(1));
    assert_eq!(id.spread.mm(), 0.0);
    let c = &out.done.changes.vertices.coincident;
    assert_eq!(c.len(), 1);
    assert_eq!(
        (c[0].operand, c[0].entity, c[0].onto),
        (Operand::from_raw(1), id.members[1].1, corner)
    );
    let w = &out.done.report.warnings;
    assert!(
        w.iter()
            .any(|w| w.kind == WarningKind::Coincident && w.entity == Entity::Vertex(corner))
    );
    assert!(out.done.report.raised.is_empty());
    // Every other vertex of sheet 1 is new, derived from its operand vertex.
    let created = &out.done.changes.vertices.created;
    assert_eq!(created.len(), 3);
    for (_, o) in created {
        assert!(
            matches!(o.as_slice(), [Origin::Operand(op, Entity::Vertex(_))] if *op == Operand::from_raw(1))
        );
    }
    assert_eq!(out.done.changes.edges.created.len(), 4);
    assert_eq!(out.done.changes.faces.created.len(), 1);
}

#[test]
fn identification_stops_at_coincidence() {
    let a = sheet(Vec3::new(0.0, 0.0, 0.0));
    let near = sheet(Vec3::new(1.0 + 0.5 * ZONE, 1.0, 0.0));
    let far = sheet(Vec3::new(1.0 + 2.0 * ZONE, 1.0, 0.0));
    let joined = run(&[&a, &near]).unwrap();
    assert_eq!(joined.done.body.vertices().count(), 7);
    // The merged point is the lower of the two (equal reach): sheet 0's.
    assert_eq!(joined.report.identified[0].kept.0, Operand::from_raw(0));
    let apart = run(&[&a, &far]).unwrap();
    assert_eq!(apart.done.body.vertices().count(), 8);
    assert!(apart.report.identified.is_empty());
    assert!(apart.done.changes.vertices.coincident.is_empty());
}

#[test]
fn a_far_merge_raises_the_kept_accuracy_measured() {
    let a = sheet(Vec3::new(0.0, 0.0, 0.0));
    let d = Vec3::new(1.005, 1.0, 0.0);
    let b = sheet_with(d, Point3::new(1.005, 1.0, 0.0), 0.01);
    let out = run(&[&a, &b]).unwrap();
    let corner = vertex_at(&a, Point3::new(1.0, 1.0, 0.0));
    let id = &out.report.identified[0];
    assert_eq!(id.vertex, corner);
    assert_eq!(id.kept.0, Operand::from_raw(0), "the smaller reach is kept");
    assert!((id.spread.mm() - 0.005).abs() < 1e-12);
    let raise = &out.done.report.raised;
    assert_eq!(raise.len(), 1);
    assert_eq!(raise[0].entity, Entity::Vertex(corner));
    assert!((raise[0].after.length().mm() - 0.005).abs() < 1e-12);
    let v = out.done.body.vertex(corner).unwrap();
    let rec = v.accuracy().unwrap();
    assert_eq!(rec.basis, Basis::Measured);
    assert!(
        out.done
            .report
            .warnings
            .iter()
            .any(|w| w.kind == WarningKind::AccuracyRaised)
    );
}

#[test]
fn a_shared_edge_waits_for_the_edge_stage() {
    let a = sheet(Vec3::new(0.0, 0.0, 0.0));
    let b = sheet(Vec3::new(1.0, 0.0, 0.0));
    let e = run(&[&a, &b]).unwrap_err();
    assert!(
        matches!(e, StitchError::Unsupported(StitchUnsupported::Edges { .. })),
        "{e:?}"
    );
    assert_eq!(e.name(), "edges_pending");
}

#[test]
fn a_vertex_on_another_sheets_edge_waits_for_the_edge_stage() {
    let a = sheet(Vec3::new(0.0, 0.0, 0.0));
    let b = sheet(Vec3::new(0.5, 1.0, 0.0));
    let e = run(&[&a, &b]).unwrap_err();
    assert!(
        matches!(e, StitchError::Unsupported(StitchUnsupported::Edges { .. })),
        "{e:?}"
    );
}

#[test]
fn one_sheets_vertices_within_coincidence_are_refused() {
    let a = sheet_with(Vec3::new(0.0, 0.0, 0.0), Point3::new(0.0, 0.0, 0.0), 1.0);
    let b = sheet(Vec3::new(5.0, 0.0, 0.0));
    let e = run(&[&b, &a]).unwrap_err();
    match e {
        StitchError::Refused(StitchRefusal::VerticesWithinCoincidence { operand, vertices }) => {
            assert_eq!(operand, Operand::from_raw(1));
            assert!(vertices[0] < vertices[1]);
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn another_documents_sheet_is_refused() {
    let a = sheet(Vec3::new(0.0, 0.0, 0.0));
    let doc = Document::with_resolution(Resolution::new(Length::from_mm(1e-4)).unwrap());
    let e = run_in(doc, &[&a]).unwrap_err();
    assert_eq!(
        e,
        StitchError::Refused(StitchRefusal::OtherDocument {
            operand: Operand::from_raw(0)
        })
    );
}

#[test]
fn a_solid_is_pending() {
    let a = sheet(Vec3::new(0.0, 0.0, 0.0));
    let mut scratch = Scratch::new();
    let settings = Settings::new();
    let stop = Stop::new();
    let doc = Document::default();
    let mut cx = Cx::new(&doc, &settings, &mut scratch, &stop, &NoTrace);
    let e = stitch(&[&a], Want::Solid, &mut cx).unwrap_err();
    assert_eq!(e, StitchError::Unsupported(StitchUnsupported::Solid));
}

#[test]
fn no_sheets_give_an_empty_body_and_one_gives_itself() {
    let out = run(&[]).unwrap();
    assert_eq!(out.done.body.vertices().count(), 0);
    assert!(out.done.changes.is_empty());
    let a = sheet(Vec3::new(0.0, 0.0, 0.0));
    let one = run(&[&a]).unwrap();
    assert_eq!(points(&one.done.body), points(&a));
    assert!(one.done.changes.is_empty());
    assert!(one.done.report.is_clean());
}

#[test]
fn a_stitch_is_deterministic_and_its_points_do_not_follow_the_order() {
    let a = sheet(Vec3::new(0.0, 0.0, 0.0));
    let b = sheet(Vec3::new(1.0 + 0.5 * ZONE, 1.0, 0.0));
    let c = sheet(Vec3::new(-1.0, -1.0 - 0.3 * ZONE, 0.0));
    let one = run(&[&a, &b, &c]).unwrap();
    let again = run(&[&a, &b, &c]).unwrap();
    assert_eq!(one.report, again.report);
    assert_eq!(one.done.changes, again.done.changes);
    assert_eq!(points(&one.done.body), points(&again.done.body));
    for order in [[&b, &a, &c], [&c, &b, &a], [&a, &c, &b]] {
        let other = run(&order).unwrap();
        assert_eq!(points(&other.done.body), points(&one.done.body));
        assert_eq!(other.report.identified.len(), 2);
    }
}

#[test]
fn a_stopped_stitch_is_cancelled() {
    let a = sheet(Vec3::new(0.0, 0.0, 0.0));
    let b = sheet(Vec3::new(1.0, 1.0, 0.0));
    let mut scratch = Scratch::new();
    let settings = Settings::new();
    let stop = Stop::new();
    stop.set();
    let doc = Document::default();
    let mut cx = Cx::new(&doc, &settings, &mut scratch, &stop, &NoTrace);
    assert_eq!(
        stitch(&[&a, &b], Want::Sheets, &mut cx).unwrap_err(),
        StitchError::Cancelled
    );
}
