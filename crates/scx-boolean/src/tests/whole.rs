// Written for solidcorex. Apache-2.0.
//! Stages 6 and 7: whole Booleans of blocks, cylinders, spheres and tori. Every result
//! passes `scx_check::contract`, and its volume is the closed form's.
//!
//! Ported (judged by outcome, against closed forms): solidcore's `boolean_api` cases of a
//! box with a cylinder through it, two overlapping boxes, a sphere and a box, two boxes
//! sharing a face, and a torus less a box; `boolean_coplanar_merge`;
//! `issue_1462_boolean_attributes` (as lineage on `Changes`); and
//! `boolean_no_dangling_vertex_refs`.

use std::f64::consts::PI;

use scx_brep::op::{Cx, Done, NoTrace, Scratch, Settings};
use scx_brep::{Brep, Changes, Document, EdgeId, Entity, FaceId, Origin};
use scx_mass::RelAccuracy;
use scx_num::budget::{Budget, Stop};
use scx_num::frame::Frame;
use scx_num::units::Length;
use scx_num::vector3d::Vec3;
use scx_testkit::bodies::made::torus;

use super::face_face::{X, Z, ball, cyl};
use super::{cuboid, pt};
use crate::{BooleanError, BooleanOp, BooleanUnsupported, NoItems, boolean};

pub(super) fn op(
    a: &Brep,
    b: &Brep,
    which: BooleanOp,
    doc: Document,
) -> Result<Done<NoItems>, BooleanError> {
    let mut scratch = Scratch::new();
    let stop = Stop::new();
    let settings = Settings::new();
    let mut cx = Cx::new(&doc, &settings, &mut scratch, &stop, &NoTrace);
    boolean(a, b, which, &mut cx)
}

/// The Boolean, checked by the contract.
pub(super) fn checked(a: &Brep, b: &Brep, which: BooleanOp, doc: Document) -> Done<NoItems> {
    let done = op(a, b, which, doc).unwrap_or_else(|e| panic!("{which:?}: {e:?}"));
    let mut scratch = Scratch::new();
    let stop = Stop::new();
    let settings = Settings::new();
    let cx = Cx::new(&doc, &settings, &mut scratch, &stop, &NoTrace);
    if let Err(e) = scx_check::contract(&done.body, &cx, scx_check::Contacts::Refused) {
        panic!("{which:?}: contract {e:?}");
    }
    done
}

pub(super) fn volume(body: &Brep) -> f64 {
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

pub(super) fn area(body: &Brep, f: FaceId) -> f64 {
    scx_mass::face_area(
        body.face(f).unwrap(),
        RelAccuracy::new(1e-10).unwrap(),
        &mut Scratch::new(),
        &mut Budget::unlimited(),
        &Stop::new(),
    )
    .unwrap()
    .value
}

fn length(body: &Brep, e: EdgeId) -> f64 {
    scx_mass::edge_length(
        body.edge(e).unwrap(),
        RelAccuracy::new(1e-10).unwrap(),
        &mut Scratch::new(),
        &mut Budget::unlimited(),
        &Stop::new(),
    )
    .unwrap()
    .value
}

#[track_caller]
pub(super) fn near(got: f64, want: f64, rel: f64) {
    assert!(
        (got - want).abs() <= rel * want.abs().max(1.0),
        "{got} against {want}"
    );
}

/// Faces, edges, vertices.
fn counts(body: &Brep) -> [usize; 3] {
    [
        body.faces().count(),
        body.edges().count(),
        body.vertices().count(),
    ]
}

pub(super) const UNION: BooleanOp = BooleanOp::Union;
pub(super) const SUBTRACT: BooleanOp = BooleanOp::Subtract;
pub(super) const INTERSECT: BooleanOp = BooleanOp::Intersect;

#[test]
fn overlapping_cubes_unite_subtract_and_intersect() {
    let doc = Document::default();
    let a = cuboid([0.0; 3], [10.0; 3], doc);
    let b = cuboid([5.0; 3], [15.0; 3], doc);
    for (which, v, faces) in [
        (UNION, 1875.0, 12),
        (SUBTRACT, 875.0, 9),
        (INTERSECT, 125.0, 6),
    ] {
        let done = checked(&a, &b, which, doc);
        near(volume(&done.body), v, 1e-9);
        assert_eq!(done.body.faces().count(), faces, "{which:?}");
    }
    // b less a, by swapping.
    near(volume(&checked(&b, &a, SUBTRACT, doc).body), 875.0, 1e-9);
}

#[test]
fn a_cylinder_through_a_plate_unites_bores_and_intersects() {
    let doc = Document::default();
    let a = cuboid([-2.0, -2.0, 0.0], [2.0, 2.0, 1.0], doc);
    let b = cyl([0.0, 0.0, -1.0], Z, X, 1.0, 3.0, doc);
    let hole = checked(&a, &b, SUBTRACT, doc);
    near(volume(&hole.body), 16.0 - PI, 1e-9);
    // The plate's six faces, the bore's wall.
    assert_eq!(hole.body.faces().count(), 7);
    near(
        volume(&checked(&a, &b, UNION, doc).body),
        16.0 + 2.0 * PI,
        1e-9,
    );
    near(volume(&checked(&a, &b, INTERSECT, doc).body), PI, 1e-9);
    // The cylinder less the plate: two lumps.
    let two = checked(&b, &a, SUBTRACT, doc);
    near(volume(&two.body), 2.0 * PI, 1e-9);
}

#[test]
fn two_unit_spheres_a_radius_apart_unite_subtract_and_intersect() {
    // They meet in the circle x = 1/2, whose pcurves go through each sphere's chart. The
    // lens between them is two caps of height 1/2: pi h^2 (3 r - h) / 3 each, 5 pi / 12 in
    // all.
    let doc = Document::default();
    let a = ball([0.0; 3], 1.0, doc);
    let b = ball([1.0, 0.0, 0.0], 1.0, doc);
    let ball_v = 4.0 * PI / 3.0;
    let lens = 5.0 * PI / 12.0;
    for (which, v) in [
        (UNION, 2.0 * ball_v - lens),
        (SUBTRACT, ball_v - lens),
        (INTERSECT, lens),
    ] {
        let done = checked(&a, &b, which, doc);
        near(volume(&done.body), v, 1e-9);
    }
}

#[test]
fn a_sphere_less_a_box_over_its_cap() {
    // The box's floor z = 0.5 cuts the unit sphere's cap of height 0.5.
    let doc = Document::default();
    let a = ball([0.0; 3], 1.0, doc);
    let b = cuboid([-2.0, -2.0, 0.5], [2.0, 2.0, 2.0], doc);
    let h: f64 = 0.5;
    let cap = PI * h * h * (3.0 - h) / 3.0;
    let ball_v = 4.0 * PI / 3.0;
    let less = checked(&a, &b, SUBTRACT, doc);
    near(volume(&less.body), ball_v - cap, 1e-9);
    near(volume(&checked(&a, &b, INTERSECT, doc).body), cap, 1e-9);
    near(
        volume(&checked(&a, &b, UNION, doc).body),
        24.0 + ball_v - cap,
        1e-9,
    );
}

#[test]
fn boxes_sharing_a_face_unite_into_one_box() {
    let doc = Document::default();
    let a = cuboid([0.0; 3], [1.0; 3], doc);
    let b = cuboid([1.0, 0.0, 0.0], [2.0, 1.0, 1.0], doc);
    let done = checked(&a, &b, UNION, doc);
    near(volume(&done.body), 2.0, 1e-9);
    assert_eq!(counts(&done.body), [6, 12, 8]);
}

#[test]
fn a_torus_less_the_box_over_its_half() {
    // The plane x = 0 through the axis cuts the torus in two meridian circles.
    let doc = Document::default();
    let frame = Frame::new(
        pt([0.0; 3]),
        Vec3::new(0.0, 0.0, 1.0),
        Vec3::new(1.0, 0.0, 0.0),
    )
    .unwrap();
    let (major, minor) = (2.0, 0.5);
    let a = torus(&frame, Length::from_mm(major), Length::from_mm(minor), &doc)
        .unwrap()
        .body;
    let b = cuboid([0.0, -3.0, -1.0], [3.0, 3.0, 1.0], doc);
    let half = PI * PI * major * minor * minor;
    near(volume(&checked(&a, &b, SUBTRACT, doc).body), half, 1e-9);
    let kept = checked(&a, &b, INTERSECT, doc).body;
    near(volume(&kept), half, 1e-9);
    // One torus face across its old seam, two disks; the seam meridian inside
    // is gone and the joined latitude arc is the face's only seam.
    assert_eq!(counts(&kept), [3, 3, 2]);
}

#[test]
fn a_sphere_inside_a_box_is_a_cavity() {
    let doc = Document::default();
    let a = cuboid([-2.0; 3], [2.0; 3], doc);
    let b = ball([0.0; 3], 1.0, doc);
    let done = checked(&a, &b, SUBTRACT, doc);
    near(volume(&done.body), 64.0 - 4.0 * PI / 3.0, 1e-9);
    assert_eq!(done.body.regions().filter(|r| r.is_solid()).count(), 1);
    // Apart: two lumps.
    let c = cuboid([5.0; 3], [6.0; 3], doc);
    near(volume(&checked(&a, &c, UNION, doc).body), 65.0, 1e-9);
}

// boolean_coplanar_merge.

/// The 10 cube with the 10 box at (5, `dy`, 0): the front faces on y = 0 and y = `dy`.
fn union_offset(dy: f64) -> Done<NoItems> {
    let doc = Document::default();
    let a = cuboid([0.0; 3], [10.0; 3], doc);
    let b = cuboid([5.0, dy, 0.0], [15.0, 10.0 + dy, 10.0], doc);
    let done = checked(&a, &b, UNION, doc);
    // Within the slab the merged front may move across: its area times `dy`.
    let v = volume(&done.body);
    assert!(
        (v - 1500.0 - 50.0 * dy).abs() <= 1e-9 * 1500.0 + 100.0 * dy,
        "dy {dy}: {v}"
    );
    done
}

#[test]
fn near_coplanar_fronts_merge_within_coincidence() {
    // The pin merges below its plane cover tolerance (5e-9) only; here the faces are one
    // within coincidence of the zone (1e-5): 1e-8 and 1e-6 merge too.
    for dy in [0.0, 2e-10, 1e-9, 4.5e-9, 1e-8, 1e-6] {
        let done = union_offset(dy);
        assert_eq!(done.body.faces().count(), 6, "dy {dy}");
    }
}

#[test]
fn near_coplanar_fronts_stay_apart_past_coincidence() {
    for dy in [1e-4, 1e-3] {
        let done = union_offset(dy);
        assert!(done.body.faces().count() > 6, "dy {dy}");
    }
}

/// The strips of an `n` x `n` grid whose index has parity `parity`, `w` wide and `2 w`
/// apart, on z = 0: those along x 1 high, those along y 2 high, united one by one.
fn grid(n: u32, w: f64, parity: u32, doc: Document) -> Brep {
    let length = 2.0 * w * f64::from(n);
    let mut all: Option<Brep> = None;
    for i in (0..n).filter(|i| i % 2 == parity) {
        let at = 2.0 * w * f64::from(i);
        for strip in [
            cuboid([0.0, at, 0.0], [length, at + w, 1.0], doc),
            cuboid([at, 0.0, 0.0], [at + w, length, 2.0], doc),
        ] {
            all = Some(match all {
                Some(a) => checked(&a, &strip, UNION, doc).body,
                None => strip,
            });
        }
    }
    all.unwrap()
}

#[test]
fn interleaved_grids_on_one_plane_unite() {
    let doc = Document::default();
    let (n, w) = (4, 1.0);
    let a = grid(n, w, 0, doc);
    let b = grid(n, w, 1, doc);
    let done = checked(&a, &b, UNION, doc);
    let length = 2.0 * w * f64::from(n);
    let exact = f64::from(n) * length * w * 3.0 - f64::from(n * n) * w * w;
    near(volume(&done.body), exact, 1e-9);
    // The pin's union of the same grids, all coplanar faces merged.
    assert_eq!(counts(&done.body), [66, 219, 146]);
}

// issue_1462_boolean_attributes, as lineage.

/// Where a result face came from: the operand faces (operand, face) whose lineage it
/// carries, sorted.
pub(super) fn face_from(c: &Changes, f: FaceId) -> Vec<(u32, FaceId)> {
    let mut out = Vec::new();
    let mut add = |o: &Origin| match *o {
        Origin::Face(g) => out.push((0, g)),
        Origin::Operand(op, Entity::Face(g)) => out.push((op.raw(), g)),
        _ => {}
    };
    match c.faces.origins(f) {
        Some(os) => os.iter().for_each(&mut add),
        None => add(&Origin::Face(f)),
    }
    for m in c.faces.merged.iter().filter(|m| m.survivor == f) {
        for &g in &m.absorbed {
            add(&Origin::Face(g));
        }
        m.taken.iter().for_each(&mut add);
    }
    out.sort_unstable();
    out.dedup();
    out
}

fn edge_from(c: &Changes, e: EdgeId) -> Vec<(u32, EdgeId)> {
    let mut out = Vec::new();
    let mut add = |o: &Origin| match *o {
        Origin::Edge(g) => out.push((0, g)),
        Origin::Operand(op, Entity::Edge(g)) => out.push((op.raw(), g)),
        _ => {}
    };
    match c.edges.origins(e) {
        Some(os) => os.iter().for_each(&mut add),
        None => add(&Origin::Edge(e)),
    }
    for m in c.edges.merged.iter().filter(|m| m.survivor == e) {
        for &g in &m.absorbed {
            add(&Origin::Edge(g));
        }
        m.taken.iter().for_each(&mut add);
    }
    out.sort_unstable();
    out.dedup();
    out
}

fn tens(which: BooleanOp) -> Done<NoItems> {
    let doc = Document::default();
    let a = cuboid([0.0; 3], [10.0; 3], doc);
    let b = cuboid([5.0; 3], [15.0; 3], doc);
    checked(&a, &b, which, doc)
}

/// Each face's (area, operand), sorted, with one operand face each.
fn faces_by_area(d: &Done<NoItems>) -> Vec<(i64, u32)> {
    let mut out: Vec<_> = d
        .body
        .faces()
        .map(|f| {
            let from = face_from(&d.changes, f.id());
            assert_eq!(from.len(), 1, "face {:?} from {from:?}", f.id());
            ((area(&d.body, f.id()) * 1e6).round() as i64, from[0].0)
        })
        .collect();
    out.sort_unstable();
    out
}

fn want(list: &[(f64, u32, usize)]) -> Vec<(i64, u32)> {
    let mut out: Vec<_> = list
        .iter()
        .flat_map(|&(a, o, n)| std::iter::repeat_n(((a * 1e6).round() as i64, o), n))
        .collect();
    out.sort_unstable();
    out
}

/// The edges with no operand edge in their lineage.
fn new_edges(d: &Done<NoItems>) -> usize {
    d.body
        .edges()
        .filter(|e| edge_from(&d.changes, e.id()).is_empty())
        .count()
}

#[test]
fn union_of_overlapping_boxes_keeps_one_origin_per_face() {
    let d = tens(UNION);
    assert_eq!(d.body.faces().count(), 12);
    assert_eq!(
        faces_by_area(&d),
        want(&[(100.0, 0, 3), (75.0, 0, 3), (100.0, 1, 3), (75.0, 1, 3)])
    );
    let mut all: Vec<_> = d
        .body
        .faces()
        .map(|f| face_from(&d.changes, f.id())[0])
        .collect();
    all.sort_unstable();
    all.dedup();
    assert_eq!(all.len(), 12, "every operand face once");
    assert_eq!(new_edges(&d), 6, "the six new edges where the faces cross");
    assert!(
        d.body
            .edges()
            .all(|e| edge_from(&d.changes, e.id()).len() <= 1)
    );
}

#[test]
fn difference_of_overlapping_boxes() {
    let d = tens(SUBTRACT);
    assert_eq!(d.body.faces().count(), 9);
    assert_eq!(
        faces_by_area(&d),
        want(&[(100.0, 0, 3), (75.0, 0, 3), (25.0, 1, 3)])
    );
    assert_eq!(new_edges(&d), 6);
}

#[test]
fn intersection_of_overlapping_boxes() {
    let d = tens(INTERSECT);
    assert_eq!(d.body.faces().count(), 6);
    assert_eq!(faces_by_area(&d), want(&[(25.0, 0, 3), (25.0, 1, 3)]));
    assert_eq!(new_edges(&d), 6);
}

#[test]
fn edges_crossed_by_the_other_operand_keep_their_origin_on_the_surviving_piece() {
    let d = tens(UNION);
    assert_eq!(d.body.edges().count(), 30);
    let mut seen = Vec::new();
    let mut halves = [0usize; 2];
    for e in d.body.edges() {
        let from = edge_from(&d.changes, e.id());
        let l = length(&d.body, e.id());
        match from.as_slice() {
            [] => near(l, 5.0, 1e-9),
            [(o, _)] => {
                if (l - 5.0).abs() < 1e-9 {
                    halves[*o as usize] += 1;
                } else {
                    near(l, 10.0, 1e-9);
                }
                seen.push(from[0]);
            }
            _ => panic!("edge from {from:?}"),
        }
    }
    assert_eq!(halves, [3, 3], "the crossed edges of each box");
    seen.sort_unstable();
    seen.dedup();
    assert_eq!(seen.len(), 24, "every operand edge once");
}

#[test]
fn faces_the_union_leaves_whole_are_untouched() {
    // The pin's temporary attributes stay only on a's three faces the union leaves whole:
    // here, those faces are neither replaced nor created.
    let d = tens(UNION);
    let untouched = d
        .body
        .faces()
        .filter(|f| {
            d.changes.faces.origins(f.id()).is_none() && d.changes.faces.touch(f.id()).is_none()
        })
        .count();
    assert_eq!(untouched, 3);
}

#[test]
fn coplanar_faces_merged_keep_both_origins() {
    let doc = Document::default();
    let a = cuboid([0.0; 3], [10.0; 3], doc);
    let b = cuboid([5.0, 5.0, 0.0], [15.0, 15.0, 10.0], doc);
    let d = checked(&a, &b, UNION, doc);
    assert_eq!(d.body.faces().count(), 10);
    near(volume(&d.body), 1750.0, 1e-9);
    let mut caps = Vec::new();
    for f in d.body.faces() {
        let from = face_from(&d.changes, f.id());
        if (area(&d.body, f.id()) - 175.0).abs() < 1e-6 {
            assert_eq!(from.len(), 2, "cap from {from:?}");
            assert_eq!([from[0].0, from[1].0], [0, 1]);
            caps.push(from);
        } else {
            assert_eq!(from.len(), 1, "side from {from:?}");
        }
    }
    assert_eq!(caps.len(), 2);
}

#[test]
fn the_operands_are_read_only() {
    // The pin's clone test: here the operands are `&Brep`, and the result is a's edit.
    let doc = Document::default();
    let a = cuboid([0.0; 3], [10.0; 3], doc);
    let b = cuboid([5.0; 3], [15.0; 3], doc);
    let before = (counts(&a), counts(&b));
    let _ = checked(&a, &b, UNION, doc);
    assert_eq!((counts(&a), counts(&b)), before);
    near(volume(&a), 1000.0, 1e-9);
}

// boolean_no_dangling_vertex_refs.

/// A 10 x 10 x 20 box turned 45 degrees about z, then about x, its edge crossing the cube's
/// edge y = z = 10 at depth `d`.
fn crease(d: f64, doc: Document) -> Brep {
    let (c, s) = (0.5_f64.sqrt(), 0.5_f64.sqrt());
    // Rx(45) * Rz(45).
    let rz = |v: [f64; 3]| [c * v[0] - s * v[1], s * v[0] + c * v[1], v[2]];
    let rx = |v: [f64; 3]| [v[0], c * v[1] - s * v[2], s * v[1] + c * v[2]];
    let r = |v: [f64; 3]| rx(rz(v));
    let k = d / 2.0_f64.sqrt();
    let shift = [5.0, 10.0 - k, 10.0 - k];
    let o = r([0.0, 0.0, -10.0]);
    let o = [o[0] + shift[0], o[1] + shift[1], o[2] + shift[2]];
    let (z, x) = (r([0.0, 0.0, 1.0]), r([1.0, 0.0, 0.0]));
    let frame = Frame::new(
        pt(o),
        Vec3::new(z[0], z[1], z[2]),
        Vec3::new(x[0], x[1], x[2]),
    )
    .unwrap();
    let size = [10.0, 10.0, 20.0].map(Length::from_mm);
    scx_testkit::bodies::made::block(&frame, size, &doc)
        .unwrap()
        .body
}

#[test]
fn a_union_of_crossing_creases_names_only_live_vertices() {
    let doc = Document::default();
    for d in [3e-6, 1e-5, 3e-5, 1e-4] {
        let a = cuboid([0.0; 3], [10.0; 3], doc);
        let b = crease(d, doc);
        let done = checked(&a, &b, UNION, doc);
        assert_eq!(done.body.faces().count(), 12, "d {d}");
        for e in done.body.edges() {
            for v in e.ends() {
                assert!(done.body.vertex(v).is_some(), "d {d}: {v:?}");
            }
        }
    }
}

#[test]
fn a_boolean_is_deterministic() {
    let doc = Document::default();
    let a = cuboid([-2.0, -2.0, 0.0], [2.0, 2.0, 1.0], doc);
    let b = cyl([0.0, 0.0, -1.0], Z, X, 1.0, 3.0, doc);
    let one = checked(&a, &b, SUBTRACT, doc);
    let two = checked(&a, &b, SUBTRACT, doc);
    assert_eq!(one.changes, two.changes);
    assert_eq!(one.report.work, two.report.work);
    assert_eq!(counts(&one.body), counts(&two.body));
}

#[test]
fn non_manifold_results_are_unsupported() {
    // A ball on a cube's top face: the union meets itself at one point inside the face
    // (D5). Two cubes sharing an edge are built (BO7, `contacts`).
    let doc = Document::default();
    let a = cuboid([0.0; 3], [1.0; 3], doc);
    {
        let b = ball([0.5, 0.5, 1.5], 0.5, doc);
        let e = op(&a, &b, UNION, doc).unwrap_err();
        assert!(
            matches!(
                e,
                BooleanError::Unsupported(BooleanUnsupported::NonManifold { .. })
            ),
            "{e:?}"
        );
    }
}
