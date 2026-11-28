// Written for solidcorex. Apache-2.0.
//! `stitch` on sheets of known placement (74 §4.3, ST1 and ST2): vertices identified within
//! coincidence and not beyond, lineage per operand, the accuracy raise, the refusals, edges
//! joined within coincidence and not beyond, cut at another sheet's vertex, left open when
//! ambiguous or undecided, and determinism under re-run and permutation.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

use std::sync::Arc;

use scx_brep::op::{Cx, NoTrace, Scratch, Settings, WarningKind};
use scx_brep::{
    Brep, Document, Draft, EdgeGeom, EdgeShape, EdgeUse, Entity, FaceRecord, LoopRecord, Operand,
    Origin, RegionId, ShellRecord, UseGeom, VertexId, VertexRecord,
};
use scx_curve::{Circle, Curve, Line};
use scx_num::budget::{Budget, Stop};
use scx_num::cert::{Basis, Recorded};
use scx_num::extent1d::Extent1d;
use scx_num::frame::{Frame, Sense};
use scx_num::math::Affine;
use scx_num::units::{Accuracy, Length, Resolution};
use scx_num::vector2d::Point2;
use scx_num::vector3d::{Point3, Vec3};
use scx_stitch::{Left, StitchError, StitchRefusal, StitchUnsupported, Stitched, Want, stitch};
use scx_surface::{Aim, Plane, Surface, curve_curve_gap, point_curve_gap};
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

fn run_limited(sheets: &[&Brep], limit: u64) -> Result<Stitched, StitchError> {
    let doc = Document::default();
    let mut scratch = Scratch::new();
    let settings = Settings::new().with_budget(limit);
    let stop = Stop::new();
    let mut cx = Cx::new(&doc, &settings, &mut scratch, &stop, &NoTrace);
    stitch(sheets, Want::Sheets, &mut cx)
}

/// A planar sheet in `z = 0` of one face: `points`, `edges` as (curve, interval, ends),
/// and the loop as (edge, sense), its pcurves written exact.
fn face_sheet(
    points: &[Point3],
    edges: Vec<(Curve, [f64; 2], [usize; 2])>,
    lp: &[(usize, Sense)],
) -> Brep {
    let doc = Document::default();
    let mut d = Draft::new();
    let shell = d.add_shell(ShellRecord::new(RegionId::OUTSIDE)).unwrap();
    let vs: Vec<VertexId> = points
        .iter()
        .map(|&p| d.add_vertex(VertexRecord::new(p)).unwrap())
        .collect();
    let es: Vec<_> = edges
        .into_iter()
        .map(|(curve, [lo, hi], [a, b])| {
            let interval = Extent1d::new(lo, hi).unwrap();
            d.add_edge(EdgeGeom::new(
                EdgeShape::Curve { curve, interval },
                [vs[a], vs[b]],
            ))
            .unwrap()
        })
        .collect();
    let uses = lp
        .iter()
        .map(|&(e, sense)| EdgeUse::new(es[e], sense, UseGeom::Point(Point2::ORIGIN)))
        .collect();
    let plane = Surface::Plane(Plane::new(
        Frame::new(
            Point3::ORIGIN,
            Vec3::new(0.0, 0.0, 1.0),
            Vec3::new(1.0, 0.0, 0.0),
        )
        .unwrap(),
    ));
    let f = d
        .add_face(FaceRecord::new(
            plane,
            [shell, shell],
            vec![LoopRecord::new(uses)],
        ))
        .unwrap();
    d.write_pcurves(f, &doc).unwrap();
    finish(d)
}

fn line(p: Point3, q: Point3) -> Curve {
    Curve::Line(Line::new(p, q - p).unwrap())
}

/// The upper half of the unit disc, and the region over the arc of radius `1 + delta` up
/// to `y = 2` between `x = -(1 + delta)` and `1 + delta`: their arcs run opposite ways.
fn discs(delta: f64) -> (Brep, Brep) {
    let z = Vec3::new(0.0, 0.0, 1.0);
    let up = Circle::new(
        Frame::new(Point3::ORIGIN, z, Vec3::new(1.0, 0.0, 0.0)).unwrap(),
        Length::from_mm(1.0),
        Sense::Forward,
    )
    .unwrap();
    let (pa, pb) = (Point3::new(1.0, 0.0, 0.0), Point3::new(-1.0, 0.0, 0.0));
    let half = face_sheet(
        &[pa, pb],
        vec![
            (Curve::Circle(up), [0.0, std::f64::consts::PI], [0, 1]),
            (line(pb, pa), [0.0, 1.0], [1, 0]),
        ],
        &[(0, Sense::Forward), (1, Sense::Forward)],
    );
    let r = 1.0 + delta;
    // About -z from -x: from (-r, 0) over the top to (r, 0).
    let down = Circle::new(
        Frame::new(Point3::ORIGIN, -z, Vec3::new(-1.0, 0.0, 0.0)).unwrap(),
        Length::from_mm(r),
        Sense::Forward,
    )
    .unwrap();
    let (qa, qb) = (Point3::new(r, 0.0, 0.0), Point3::new(-r, 0.0, 0.0));
    let (qc, qd) = (Point3::new(-r, 2.0, 0.0), Point3::new(r, 2.0, 0.0));
    let over = face_sheet(
        &[qa, qd, qc, qb],
        vec![
            (line(qa, qd), [0.0, 1.0], [0, 1]),
            (line(qd, qc), [0.0, 1.0], [1, 2]),
            (line(qc, qb), [0.0, 1.0], [2, 3]),
            (Curve::Circle(down), [0.0, std::f64::consts::PI], [3, 0]),
        ],
        &[
            (0, Sense::Forward),
            (1, Sense::Forward),
            (2, Sense::Forward),
            (3, Sense::Forward),
        ],
    );
    (half, over)
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
fn a_shared_edge_is_joined() {
    let a = sheet(Vec3::new(0.0, 0.0, 0.0));
    let b = sheet(Vec3::new(1.0, 0.0, 0.0));
    let out = run(&[&a, &b]).unwrap();
    let body = &out.done.body;
    assert_eq!(body.vertices().count(), 6);
    assert_eq!(body.edges().count(), 7);
    assert_eq!(body.faces().count(), 2);
    assert_eq!(out.report.joined.len(), 1);
    assert!(out.report.left.is_empty() && out.report.split.is_empty());
    let j = &out.report.joined[0];
    assert!(j.gap.mm() <= 1e-12, "{:?}", j.gap);
    // The kept edge is sheet 0's (equal reach, equal midpoint, the lower sheet).
    let seam = a
        .edges()
        .find(|e| {
            e.ends()
                .iter()
                .all(|&v| a.vertex(v).unwrap().point().x == 1.0)
        })
        .unwrap()
        .id();
    assert_eq!(j.edge, seam);
    assert_eq!(body.edge(seam).unwrap().fins().len(), 2);
    let merged = &out.done.changes.edges.merged;
    assert_eq!(merged.len(), 1);
    assert_eq!(merged[0].survivor, seam);
    assert!(
        out.done
            .report
            .warnings
            .iter()
            .any(|w| w.kind == WarningKind::Coincident && w.entity == Entity::Edge(seam))
    );
}

#[test]
fn edges_are_joined_within_coincidence_and_not_beyond() {
    let a = sheet(Vec3::new(0.0, 0.0, 0.0));
    let near = sheet(Vec3::new(1.0 + 0.5 * ZONE, 0.0, 0.0));
    let far = sheet(Vec3::new(1.0 + 2.0 * ZONE, 0.0, 0.0));
    let joined = run(&[&a, &near]).unwrap();
    assert_eq!(joined.done.body.edges().count(), 7);
    let gap = joined.report.joined[0].gap.mm();
    assert!(
        (0.5 * ZONE..=0.5 * ZONE * (1.0 + 1e-9)).contains(&gap),
        "{gap}"
    );
    let apart = run(&[&a, &far]).unwrap();
    assert_eq!(apart.done.body.edges().count(), 8);
    assert!(apart.report.joined.is_empty() && apart.report.left.is_empty());
}

#[test]
fn a_vertex_on_another_sheets_edge_cuts_it_and_the_overlap_is_joined() {
    // b's corner (0.5, 1) is inside a's top edge, a's corner (1, 1) inside b's bottom one:
    // each is cut there, and the two pieces over [0.5, 1] are one edge.
    let a = sheet(Vec3::new(0.0, 0.0, 0.0));
    let b = sheet(Vec3::new(0.5, 1.0, 0.0));
    let out = run(&[&a, &b]).unwrap();
    let body = &out.done.body;
    assert_eq!(body.vertices().count(), 8);
    assert_eq!(body.edges().count(), 9);
    assert_eq!(out.report.split.len(), 2);
    assert_eq!(out.report.joined.len(), 1);
    let j = body.edge(out.report.joined[0].edge).unwrap();
    let mut xs = j.ends().map(|v| body.vertex(v).unwrap().point().x);
    xs.sort_by(f64::total_cmp);
    assert_eq!(xs, [0.5, 1.0]);
    assert_eq!(j.fins().len(), 2);
    // Both joined pieces are made by the cuts, so the change log holds no merge of an
    // input edge: the absorbed piece is simply never created.
    // Each cut's new piece is derived from the edge it cuts (an operand edge's origin, for
    // a copied one): sheet 0's four edges and five created, each with an origin.
    let created = &out.done.changes.edges.created;
    assert_eq!(created.len(), 5, "{created:?}");
    assert!(created.iter().all(|(_, o)| !o.as_slice().is_empty()));
    assert!(out.done.report.raised.is_empty(), "the cuts are exact");
}

#[test]
fn an_edge_with_two_possible_partners_is_left_open() {
    // b and c lie on each other, both against a: a's right edge and b's and c's left
    // edges are each within coincidence of two others, so none is joined.
    let a = sheet(Vec3::new(0.0, 0.0, 0.0));
    let b = sheet(Vec3::new(1.0, 0.0, 0.0));
    let c = sheet(Vec3::new(1.0, 0.0, 0.0));
    let out = run(&[&a, &b, &c]).unwrap();
    let open: Vec<_> = out
        .report
        .left
        .iter()
        .filter_map(|l| match l {
            Left::Ambiguous { edge, partners } => Some((*edge, partners.len())),
            _ => None,
        })
        .collect();
    assert_eq!(open.len(), 3, "{:?}", out.report.left);
    assert!(open.iter().all(|&(_, n)| n == 2));
    for (e, _) in &open {
        assert!(out.report.joined.iter().all(|j| j.edge != *e));
    }
    // b's other three edges have only c's to join.
    assert_eq!(out.report.joined.len(), 3);
}

#[test]
fn a_join_no_share_decides_is_left_undecided() {
    // An arc against a coaxial arc 0.9 coincidence out is a plateau of equal distances: a
    // small budget leaves it open, a large one joins it, and no budget guesses.
    let (a, b) = discs(0.9 * ZONE);
    let mut open = 0;
    let mut decided = 0;
    for limit in [40, 60, 80, 120, 200, 400, 1_000, 4_000, 20_000, 200_000] {
        match run_limited(&[&a, &b], limit) {
            Ok(out) => {
                if out.report.undecided() > 0 {
                    assert!(out.report.joined.is_empty());
                    assert!(matches!(
                        out.report.left[0],
                        Left::Undecided {
                            with: Entity::Edge(_),
                            ..
                        }
                    ));
                    open += 1;
                } else {
                    assert_eq!(out.report.joined.len(), 1, "{limit}");
                    decided += 1;
                }
            }
            Err(StitchError::Exhausted { .. }) => {}
            Err(e) => panic!("{e:?}"),
        }
    }
    assert!(open > 0 && decided > 0, "{open} {decided}");
}

#[test]
fn arcs_run_opposite_ways_are_joined_within_coincidence() {
    let (a, b) = discs(0.5 * ZONE);
    let out = run(&[&a, &b]).unwrap();
    assert_eq!(out.report.joined.len(), 1);
    // The recorded gap is the certified upper bound, which need only fall within
    // coincidence: at least the true 0.5 coincidence, at most coincidence.
    let gap = out.report.joined[0].gap.mm();
    assert!((0.5 * ZONE..=ZONE).contains(&gap), "{gap}");
    assert_eq!(out.done.body.edges().count(), 5);
    let (a, b) = discs(2.0 * ZONE);
    let out = run(&[&a, &b]).unwrap();
    assert!(out.report.joined.is_empty());
    assert_eq!(out.done.body.edges().count(), 6);
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

fn midpoints(b: &Brep) -> Vec<[u64; 3]> {
    let mut v: Vec<[u64; 3]> = b
        .edges()
        .filter_map(|e| match &e.geometry().shape {
            EdgeShape::Curve { curve, interval } => {
                let p = curve.point(interval.mid());
                Some([p.x.to_bits(), p.y.to_bits(), p.z.to_bits()])
            }
            EdgeShape::Collapsed => None,
        })
        .collect();
    v.sort_unstable();
    v
}

#[test]
fn joins_and_cuts_do_not_follow_the_order() {
    let a = sheet(Vec3::new(0.0, 0.0, 0.0));
    let b = sheet(Vec3::new(1.0 + 0.3 * ZONE, 0.0, 0.0));
    let c = sheet(Vec3::new(0.5, 1.0 - 0.2 * ZONE, 0.0));
    let d = sheet(Vec3::new(1.0, -1.0, 0.0));
    let one = run(&[&a, &b, &c, &d]).unwrap();
    let again = run(&[&a, &b, &c, &d]).unwrap();
    assert_eq!(one.report, again.report);
    assert_eq!(one.done.changes, again.done.changes);
    let (p, m) = (points(&one.done.body), midpoints(&one.done.body));
    for order in [[&d, &c, &b, &a], [&b, &a, &d, &c], [&c, &d, &a, &b]] {
        let other = run(&order).unwrap();
        assert_eq!(points(&other.done.body), p);
        assert_eq!(midpoints(&other.done.body), m);
        assert_eq!(other.report.joined.len(), one.report.joined.len());
        assert_eq!(other.report.split.len(), one.report.split.len());
    }
}

/// The pin's stitch bounds (`heal/stitch/bounds.rs`) against their closed forms, through
/// the gaps the stitch measures with: a degree-one hat of height 1 between samples, a
/// rational arc of height 2/3, and a reversed sub-range.
#[test]
fn the_gaps_hold_the_pins_bound_truths() {
    use scx_nurbs::{KnotVector, NurbsCurve};
    use std::sync::Arc as A;
    let nurbs = |deg: usize, knots: Vec<f64>, pts: &[(f64, f64)], w: Option<Vec<f64>>| {
        Curve::Nurbs(A::new(
            NurbsCurve::new(
                KnotVector::new(deg, knots).unwrap(),
                pts.iter().map(|&(x, y)| [x, y, 0.0]).collect(),
                w,
            )
            .unwrap(),
        ))
    };
    let unit = Extent1d::new(0.0, 1.0).unwrap();
    let gap = |a: &Curve, ao: &Extent1d, b: &Curve, bo: &Extent1d| {
        let mut budget = Budget::new(1_000_000);
        let stop = Stop::new();
        // A bracket the precision limit stops is still a bracket.
        let held = |r: Result<scx_surface::Bracket, scx_surface::GapError>| match r {
            Ok(b) => b,
            Err(scx_surface::GapError::Undecided { reached, .. }) => reached,
            Err(e) => panic!("{e:?}"),
        };
        let x = held(curve_curve_gap(
            a,
            ao,
            b,
            bo,
            Aim::Width(1e-12),
            &mut budget,
            &stop,
        ));
        let y = held(curve_curve_gap(
            b,
            bo,
            a,
            ao,
            Aim::Width(1e-12),
            &mut budget,
            &stop,
        ));
        (x.lower().max(y.lower()), x.upper().max(y.upper()))
    };
    let knots = vec![0.0, 0.0, 0.0625, 0.125, 0.1875, 1.0, 1.0];
    let flat = nurbs(
        1,
        knots.clone(),
        &[
            (0.0, 0.0),
            (0.0625, 0.0),
            (0.125, 0.0),
            (0.1875, 0.0),
            (1.0, 0.0),
        ],
        None,
    );
    let bump = nurbs(
        1,
        knots,
        &[
            (0.0, 0.0),
            (0.0625, 0.0),
            (0.125, 1.0),
            (0.1875, 0.0),
            (1.0, 0.0),
        ],
        None,
    );
    // The pin bounds the correspondence t -> t, which is 1 here; the symmetric Hausdorff
    // distance is the hat's peak to the flat curve, also 1.
    let (lo, hi) = gap(&flat, &unit, &bump, &unit);
    assert!(lo <= 1.0 && (1.0..1.0 + 1e-9).contains(&hi), "{lo} {hi}");
    let mut budget = Budget::new(1_000_000);
    let to_origin = point_curve_gap(
        Point3::ORIGIN,
        &bump,
        &unit,
        Aim::Width(1e-12),
        &mut budget,
        &Stop::new(),
    )
    .unwrap();
    assert!(to_origin.upper() <= 1e-12);
    // Y(t) = 4t(1-t)/(1+2t(1-t)), maximum 2/3 at 1/2, against the x axis from 0 to 1.
    let rational = nurbs(
        2,
        vec![0.0, 0.0, 0.0, 1.0, 1.0, 1.0],
        &[(0.0, 0.0), (0.5, 1.0), (1.0, 0.0)],
        Some(vec![1.0, 2.0, 1.0]),
    );
    let axis = line(Point3::ORIGIN, Point3::new(1.0, 0.0, 0.0));
    let (lo, hi) = gap(&rational, &unit, &axis, &unit);
    assert!(
        lo <= 2.0 / 3.0 && 2.0 / 3.0 <= hi && hi - lo <= 1e-9,
        "{lo} {hi}"
    );
    // A forward segment's middle half against the reversed segment's: no gap.
    let fwd = line(Point3::ORIGIN, Point3::new(1.0, 0.0, 0.0));
    let rev = line(Point3::new(1.0, 0.0, 0.0), Point3::ORIGIN);
    let half = Extent1d::new(0.25, 0.75).unwrap();
    assert!(gap(&fwd, &half, &rev, &half).1 <= 1e-12);
    // Two skew segments: the symmetric gap is the largest of the four end-to-segment
    // gaps (a point's distance is convex along a segment), the stitch's shortcut.
    let skew = line(Point3::new(0.0, 0.1, 0.2), Point3::new(1.0, 0.3, 0.0));
    let (lo, hi) = gap(&fwd, &unit, &skew, &unit);
    let mut ends: f64 = 0.0;
    for (from, c) in [(&fwd, &skew), (&skew, &fwd)] {
        for t in [0.0, 1.0] {
            let mut budget = Budget::new(1_000_000);
            let b = point_curve_gap(
                from.point(t),
                c,
                &unit,
                Aim::Width(1e-12),
                &mut budget,
                &Stop::new(),
            );
            ends = ends.max(b.unwrap().upper());
        }
    }
    assert!(lo <= ends + 1e-9 && ends <= hi + 1e-9, "{lo} {hi} {ends}");
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
