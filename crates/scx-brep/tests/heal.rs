// Written for solidcorex. Apache-2.0.
//! `Draft::heal` and its parts (research 75 BR11): faces on one surface merged across an
//! edge, and edges on one curve joined at a free vertex, with a protected-edge set. Ported
//! from the pin's merge tests (`merge_tests.rs`: a split cube merges to six faces, two
//! unioned boxes to one block, a box is left alone, a picked edge keeps the faces apart,
//! more kept edges than a point list takes, an unknown kept edge refused), judged against
//! closed forms: the face, edge and vertex counts of a box and of a cylinder, a box's
//! area and volume unchanged and equal to `2(ab+bc+ca)` and `abc`, and the check passing
//! (`finish` and the oracle).
//!
//! The bodies are built the way a cut leaves them: one plane value for every quad of a
//! box face, one line value for every piece of a grid line, one circle value for every arc.

#![allow(clippy::unwrap_used, clippy::indexing_slicing, clippy::panic)]

use std::collections::{BTreeMap, BTreeSet};
use std::f64::consts::TAU;
use std::fmt::Write as _;

use scx_brep::{
    Document, Draft, EdgeGeom, EdgeId, EdgeShape, EdgeUse, FaceRecord, LoopRecord, Refusal,
    RegionId, ShellId, ShellRecord, UseGeom, VertexId, VertexRecord, Why,
};
use scx_curve::{Circle, Curve, Line};
use scx_num::extent1d::Extent1d;
use scx_num::frame::{Frame, Sense};
use scx_num::units::{Length, Resolution};
use scx_num::vector3d::{Point3, Vec3};
use scx_surface::{Plane, Surface, pcurve_of};
use scx_testkit::bodies::check::{Tolerance, violations};
use scx_testkit::bodies::{Kind, generate};

fn doc() -> Document {
    Document::default()
}

fn none() -> BTreeSet<EdgeId> {
    BTreeSet::new()
}

fn res() -> Resolution {
    Resolution::new(Length::from_mm(2e-9)).unwrap()
}

fn shells(d: &mut Draft) -> (ShellId, ShellId) {
    let outer = d.add_shell(ShellRecord::new(RegionId::OUTSIDE)).unwrap();
    let solid = d.add_region(true).unwrap();
    let inner = d.add_shell(ShellRecord::new(solid)).unwrap();
    (outer, inner)
}

fn pcurve(edge: &Curve, interval: Extent1d, surface: &Surface) -> UseGeom {
    UseGeom::Pcurve {
        curve: pcurve_of(edge, surface, interval, res()).unwrap(),
        interval,
    }
}

/// A box of side lengths `len`, cut by grid planes into `n[axis]` cells along each axis: a
/// quad for each cell of each side, one plane per side, one line per grid line.
struct Lattice {
    draft: Draft,
    /// The edge along `axis` from lattice point `p`.
    edges: BTreeMap<(usize, [usize; 3]), EdgeId>,
}

fn lattice(len: [f64; 3], n: [usize; 3]) -> Lattice {
    let mut d = Draft::new();
    let (outer, inner) = shells(&mut d);
    let at = |axis: usize, i: usize| len[axis] * i as f64 / n[axis] as f64;
    let point = |p: [usize; 3]| Point3::new(at(0, p[0]), at(1, p[1]), at(2, p[2]));
    let boundary = |p: [usize; 3]| (0..3).any(|a| p[a] == 0 || p[a] == n[a]);
    let mut verts: BTreeMap<[usize; 3], VertexId> = BTreeMap::new();
    for i in 0..=n[0] {
        for j in 0..=n[1] {
            for k in 0..=n[2] {
                if boundary([i, j, k]) {
                    let v = d.add_vertex(VertexRecord::new(point([i, j, k]))).unwrap();
                    verts.insert([i, j, k], v);
                }
            }
        }
    }
    let mut edges: BTreeMap<(usize, [usize; 3]), EdgeId> = BTreeMap::new();
    let mut curves: BTreeMap<(usize, [usize; 3]), (Curve, Extent1d)> = BTreeMap::new();
    for axis in 0..3 {
        for i in 0..=n[0] {
            for j in 0..=n[1] {
                for k in 0..=n[2] {
                    let p = [i, j, k];
                    if p[axis] == n[axis] {
                        continue;
                    }
                    let mut q = p;
                    q[axis] += 1;
                    // On the surface when another coordinate is on a side.
                    if !(0..3).any(|b| b != axis && (p[b] == 0 || p[b] == n[b])) {
                        continue;
                    }
                    let mut o = p;
                    o[axis] = 0;
                    let mut dir = [0.0; 3];
                    dir[axis] = 1.0;
                    let line = Line::new(point(o), Vec3::new(dir[0], dir[1], dir[2])).unwrap();
                    let curve = Curve::Line(line);
                    let interval = Extent1d::new(at(axis, p[axis]), at(axis, q[axis])).unwrap();
                    let shape = EdgeShape::Curve {
                        curve: curve.clone(),
                        interval,
                    };
                    let e = d
                        .add_edge(EdgeGeom::new(shape, [verts[&p], verts[&q]]))
                        .unwrap();
                    edges.insert((axis, p), e);
                    curves.insert((axis, p), (curve, interval));
                }
            }
        }
    }
    for axis in 0..3 {
        let (u, v) = ((axis + 1) % 3, (axis + 2) % 3);
        for side in 0..2 {
            let mut o = [0.0; 3];
            o[axis] = side as f64 * len[axis];
            let mut z = [0.0; 3];
            z[axis] = if side == 1 { 1.0 } else { -1.0 };
            let mut x = [0.0; 3];
            x[u] = 1.0;
            let frame = Frame::new(
                Point3::new(o[0], o[1], o[2]),
                Vec3::new(z[0], z[1], z[2]),
                Vec3::new(x[0], x[1], x[2]),
            )
            .unwrap();
            let surface = Surface::Plane(Plane::new(frame));
            for p in 0..n[u] {
                for q in 0..n[v] {
                    let mut ring: Vec<[usize; 3]> = [(0, 0), (1, 0), (1, 1), (0, 1)]
                        .iter()
                        .map(|&(du, dv)| {
                            let mut c = [0; 3];
                            c[axis] = side * n[axis];
                            c[u] = p + du;
                            c[v] = q + dv;
                            c
                        })
                        .collect();
                    if side == 0 {
                        ring.reverse();
                    }
                    let uses: Vec<EdgeUse> = (0..4)
                        .map(|m| {
                            let (a, b) = (ring[m], ring[(m + 1) % 4]);
                            let along = (0..3).find(|&c| a[c] != b[c]).unwrap();
                            let (lo, sense) = if a[along] < b[along] {
                                (a, Sense::Forward)
                            } else {
                                (b, Sense::Reversed)
                            };
                            let key = (along, lo);
                            let (curve, interval) = &curves[&key];
                            EdgeUse::new(edges[&key], sense, pcurve(curve, *interval, &surface))
                        })
                        .collect();
                    d.add_face(FaceRecord::new(
                        surface.clone(),
                        [outer, inner],
                        vec![LoopRecord::new(uses)],
                    ))
                    .unwrap();
                }
            }
        }
    }
    Lattice { draft: d, edges }
}

/// What a planar body holds, found from its vertices and loops alone.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Totals {
    faces: usize,
    edges: usize,
    vertices: usize,
    area: f64,
    volume: f64,
}

fn point(d: &Draft, v: VertexId) -> Point3 {
    d.vertex(v).unwrap().point()
}

/// The area of each face from the area vector of its loops' vertices (half the sum of the
/// cross products of the points), and from them, by the divergence theorem with the front
/// outward, the volume. Exact for planar faces with straight edges.
fn totals(d: &Draft) -> Totals {
    let (mut area, mut volume) = (0.0, 0.0);
    for f in d.faces() {
        let mut sum = Vec3::new(0.0, 0.0, 0.0);
        let mut any: Option<Point3> = None;
        for lp in f.loops() {
            for u in lp.uses() {
                let [a, b] = d.edge(u.record().edge).unwrap().ends();
                let (s, t) = match u.sense() {
                    Sense::Forward => (a, b),
                    Sense::Reversed => (b, a),
                };
                let (p, q) = (point(d, s), point(d, t));
                any.get_or_insert(p);
                let (p, q) = (Vec3::new(p.x, p.y, p.z), Vec3::new(q.x, q.y, q.z));
                sum += p.cross(q);
            }
        }
        let a = sum * 0.5;
        area += a.length();
        if let Some(p0) = any {
            volume += Vec3::new(p0.x, p0.y, p0.z).dot(a) / 3.0;
        }
    }
    Totals {
        faces: d.faces().count(),
        edges: d.edges().count(),
        vertices: d.vertices().count(),
        area,
        volume,
    }
}

fn counts(d: &Draft) -> (usize, usize, usize) {
    (d.faces().count(), d.edges().count(), d.vertices().count())
}

fn near(a: f64, b: f64) {
    assert!((a - b).abs() <= 1e-9 * (1.0 + b.abs()), "{a} against {b}");
}

/// The body passes the oracle and `finish`, by the touched check and by the whole one.
fn checked(d: &Draft) {
    let bad = violations(d, Tolerance::default());
    assert!(bad.is_empty(), "the oracle finds {bad:?}");
    if let Err(u) = d.clone().finish(&doc()) {
        panic!("finish refuses: {}", u.cause);
    }
    if let Err(u) = d.clone().finish_checking_all(&doc()) {
        panic!("finish (all) refuses: {}", u.cause);
    }
}

fn cells(n: [usize; 3]) -> usize {
    2 * (n[0] * n[1] + n[1] * n[2] + n[2] * n[0])
}

#[test]
fn a_lattice_box_heals_to_the_box() {
    // A split cube (the pin's `merge_a_split_cube_into_one`) and two unioned boxes
    // (`merge_two_boxes_into_one_block`, here a 2 x 1 x 1 lattice), and bigger cuts: six
    // faces, twelve edges, eight vertices, and the area and volume of the box.
    for (len, n) in [
        ([10.0, 10.0, 10.0], [1, 1, 1]),
        ([10.0, 10.0, 10.0], [2, 1, 1]),
        ([20.0, 10.0, 10.0], [2, 1, 1]),
        ([1.5, 2.0, 3.25], [3, 2, 4]),
        ([4.0, 4.5, 5.0], [5, 5, 5]),
        ([2.0, 3.0, 1.0], [8, 1, 3]),
    ] {
        let mut d = lattice(len, n).draft;
        let before = totals(&d);
        assert_eq!(before.faces, cells(n), "{n:?}");
        let (a, b, c) = (len[0], len[1], len[2]);
        near(before.volume, a * b * c);
        near(before.area, 2.0 * (a * b + b * c + c * a));
        let report = d.heal(&none(), &doc()).unwrap();
        let after = totals(&d);
        assert_eq!(
            (after.faces, after.edges, after.vertices),
            (6, 12, 8),
            "{n:?}"
        );
        near(after.volume, a * b * c);
        near(after.area, 2.0 * (a * b + b * c + c * a));
        assert_eq!(report.faces.len(), cells(n) - 6, "{n:?}");
        assert!(report.left.is_empty(), "{n:?}: {:?}", report.left);
        checked(&d);
        // Healed once, there is nothing left to do.
        let again = d.heal(&none(), &doc()).unwrap();
        assert!(again.faces.is_empty() && again.edges.is_empty() && again.left.is_empty());
    }
}

#[test]
fn a_body_with_nothing_to_merge_is_left_as_it_is() {
    // The pin's `merge_a_box_changes_nothing`: a box of one face a side, and every
    // generated planar kind, whose faces and edges are on surfaces and curves of their own
    // (equal in shape, not in value).
    let mut d = lattice([1.0, 2.0, 3.0], [1, 1, 1]).draft;
    let before = totals(&d);
    let report = d.heal(&none(), &doc()).unwrap();
    assert!(report.faces.is_empty() && report.edges.is_empty() && report.left.is_empty());
    assert_eq!(before, totals(&d));
    for kind in [
        Kind::Block,
        Kind::Prism,
        Kind::QuadSheet,
        Kind::TriSheet,
        Kind::Holed,
        Kind::Cavity,
        Kind::Stack,
        Kind::Checker,
        Kind::Touching,
    ] {
        let mut d = generate(kind, 5, 3).unwrap().draft;
        let before = totals(&d);
        let report = d.heal(&none(), &doc()).unwrap();
        assert!(
            report.faces.is_empty() && report.edges.is_empty(),
            "{kind:?}"
        );
        assert_eq!(before, totals(&d), "{kind:?}");
        checked(&d);
    }
}

#[test]
fn generated_bodies_with_equal_surfaces_stay_valid() {
    // The curved kinds cut their walls into lunes of one surface, the seam lune across a
    // period: whatever merges, the result is a body that passes the check.
    for kind in [Kind::Cylinder, Kind::Cone, Kind::Sphere, Kind::Torus] {
        for n in [1, 2, 3, 5] {
            let mut d = generate(kind, 1, n).unwrap().draft;
            let before = d.faces().count();
            let report = d.heal(&none(), &doc()).unwrap();
            assert!(d.faces().count() <= before, "{kind:?} {n}");
            assert_eq!(
                d.faces().count() + report.faces.len(),
                before,
                "{kind:?} {n}"
            );
            checked(&d);
        }
    }
}

#[test]
fn a_cylinder_cut_into_lunes_merges_its_wall() {
    // The generated cylinder's wall is `n` lunes (one across the seam) and each circle `n`
    // arcs. Healed, the wall is one face with its seam (the other lunes' shared edges go,
    // the seam stays), and the arcs join where their uses' pcurves are equal. The arcs
    // either side of the seam lune's window differ by a whole period in the wall's uv
    // (`start` a turn apart), which is left and listed (`Why::Uses`), not guessed: open.
    for n in [1, 2, 3, 5, 12] {
        let mut d = generate(Kind::Cylinder, 3, n).unwrap().draft;
        let before = d.faces().count();
        let report = d.heal(&none(), &doc()).unwrap();
        assert_eq!(d.faces().count(), 3, "n {n}: {report:?}");
        assert_eq!(report.faces.len(), before - 3, "n {n}");
        assert!(
            report.left.iter().all(|l| l.why == Why::Uses),
            "n {n}: {:?}",
            report.left
        );
        assert!(d.edges().count() <= 5 && d.vertices().count() <= 4, "n {n}");
        checked(&d);
    }
}

#[test]
fn a_bar_of_a_thousand_pieces_with_one_kept_cut() {
    // The pin's 4,100-edge bar (`heal_and_offset_face_handle_more_than_max_points`): a
    // 1,025-cell bar, one cut across its top kept: seven faces, 15 edges, ten vertices.
    let n = [1025, 1, 1];
    let l = lattice([1025.0, 10.0, 10.0], n);
    let kept: BTreeSet<EdgeId> = [l.edges[&(1, [500, 0, 1])]].into();
    let mut d = l.draft;
    assert!(d.edges().count() > 4096);
    let report = d.heal(&kept, &doc()).unwrap();
    assert_eq!(counts(&d), (7, 15, 10));
    assert_eq!(report.faces.len(), cells(n) - 7);
    near(totals(&d).volume, 1025.0 * 100.0);
    checked(&d);
}

/// The pieces of the line across the top at `x = x_1`, in a lattice.
fn top_line(l: &Lattice, n: [usize; 3]) -> BTreeSet<EdgeId> {
    (0..n[1]).map(|j| l.edges[&(1, [1, j, n[2]])]).collect()
}

#[test]
fn a_protected_line_keeps_the_faces_apart() {
    // The pin's `merge_keeps_a_picked_edge` (a cube with one face's cut kept: seven faces,
    // fifteen edges, ten vertices): the line crosses the top, which stays two faces; the
    // two cube edges it ends on are split in two; its own pieces are joined, all being
    // protected.
    for (len, n) in [
        ([10.0, 10.0, 10.0], [2, 1, 1]),
        ([10.0, 10.0, 10.0], [2, 2, 2]),
        ([6.0, 5.0, 4.0], [3, 4, 2]),
    ] {
        let l = lattice(len, n);
        let kept = top_line(&l, n);
        let mut d = l.draft;
        let before = totals(&d);
        let report = d.heal(&kept, &doc()).unwrap();
        let after = totals(&d);
        assert_eq!(
            (after.faces, after.edges, after.vertices),
            (7, 15, 10),
            "{n:?}"
        );
        near(after.area, before.area);
        near(after.volume, before.volume);
        // The protected edges survive: one is left, the others absorbed into it.
        let left: Vec<EdgeId> = kept
            .iter()
            .copied()
            .filter(|e| d.edge(*e).is_some())
            .collect();
        assert_eq!(left.len(), 1, "{n:?}");
        let absorbed: BTreeSet<EdgeId> = report
            .edges
            .iter()
            .filter(|j| kept.contains(&j.absorbed))
            .map(|j| j.absorbed)
            .collect();
        assert_eq!(absorbed.len(), kept.len() - 1, "{n:?}");
        for j in &report.edges {
            assert_eq!(
                kept.contains(&j.absorbed),
                kept.contains(&j.survivor),
                "{n:?}"
            );
        }
        assert!(report.left.iter().any(|l| l.why == Why::Protected), "{n:?}");
        checked(&d);
    }
}

#[test]
fn a_protected_id_that_is_not_an_edge_is_refused_before_any_write() {
    // The pin refuses an out-of-range kept index (`Error::Argument`); a typed id says which.
    let mut d = lattice([1.0; 3], [2, 2, 2]).draft;
    let before = totals(&d);
    let ghost = EdgeId::from_raw(99_999);
    let kept: BTreeSet<EdgeId> = [ghost].into();
    let e = d.heal(&kept, &doc()).unwrap_err();
    assert_eq!(e.entity, ghost.into());
    assert!(d.merge_faces(&kept, &doc()).is_err());
    assert!(d.join_edges(&kept).is_err());
    assert_eq!(totals(&d), before);
    // A removed edge is as absent as one never made.
    let first = d.edges().next().unwrap().id();
    d.remove_edge(first).unwrap();
    assert!(d.heal(&[first].into(), &doc()).is_err());
}

#[test]
fn a_great_many_protected_edges_are_no_limit() {
    // The pin's point list takes 4,096 kept edges at most; here every edge of a 12-cell and
    // of a 24-cell lattice (1,728 and 6,912 edges) is kept, and nothing merges or joins.
    for k in [12, 24] {
        let mut d = lattice([1.0; 3], [k, k, k]).draft;
        let all: BTreeSet<EdgeId> = d.edges().map(scx_brep::Edge::id).collect();
        let before = totals(&d);
        let report = d.heal(&all, &doc()).unwrap();
        assert!(report.faces.is_empty() && report.edges.is_empty(), "k {k}");
        assert_eq!(totals(&d), before);
    }
}

#[test]
fn a_big_lattice_heals_and_the_parts_agree_with_the_whole() {
    // Faces first and then edges is `heal`'s first round; a second `heal` finds nothing.
    let n = [20, 20, 20];
    let mut d = lattice([1.0, 2.0, 3.0], n).draft;
    let mut parts = d.clone();
    let whole = d.heal(&none(), &doc()).unwrap();
    let faces = parts.merge_faces(&none(), &doc()).unwrap();
    assert_eq!(faces.faces.len(), cells(n) - 6);
    let joined = parts.join_edges(&none()).unwrap();
    // Each cube edge of 20 pieces takes 19 joins.
    assert_eq!(joined.edges.len(), 12 * 19);
    assert_eq!(counts(&parts), counts(&d));
    assert_eq!(counts(&d), (6, 12, 8));
    assert_eq!(whole.faces.len(), faces.faces.len());
    near(totals(&d).volume, 6.0);
    near(totals(&d).area, 2.0 * (2.0 + 6.0 + 3.0));
    let again = d.heal(&none(), &doc()).unwrap();
    assert!(again.faces.is_empty() && again.edges.is_empty());
    checked(&d);
}

#[test]
fn named_entities_that_are_not_candidates_are_refused() {
    let l = lattice([1.0; 3], [2, 2, 2]);
    let mut d = l.draft;
    let before = totals(&d);
    // An edge along a cube edge: its two faces are on different planes.
    let corner = l.edges[&(0, [0, 0, 0])];
    assert_eq!(
        d.merge_faces_across(corner, &none(), &doc()),
        Err(Refusal::NotACandidate)
    );
    assert!(matches!(
        d.merge_faces_across(EdgeId::from_raw(99_999), &none(), &doc()),
        Err(Refusal::Absent(_))
    ));
    assert!(matches!(
        d.join_edges_at(VertexId::from_raw(99_999), &none()),
        Err(Refusal::Absent(_))
    ));
    // A corner vertex has three edges, not the free vertex of two.
    let v = d.vertices().find(|v| v.edges().count() == 3).unwrap().id();
    assert_eq!(d.join_edges_at(v, &none()), Err(Refusal::NotACandidate));
    // Faces first: a protected edge between two faces on one plane is named and left.
    let mid = l.edges[&(1, [1, 0, 2])];
    let kept: BTreeSet<EdgeId> = [mid].into();
    assert_eq!(
        d.merge_faces_across(mid, &kept, &doc()),
        Err(Refusal::Left(Why::Protected))
    );
    assert_eq!(totals(&d), before);
}

#[test]
fn one_merge_by_name_then_the_rest() {
    let l = lattice([1.0, 2.0, 3.0], [2, 2, 2]);
    let mut d = l.draft;
    let before = totals(&d);
    let e = l.edges[&(1, [1, 0, 2])];
    let m = d.merge_faces_across(e, &none(), &doc()).unwrap();
    assert!(m.survivor < m.absorbed);
    assert_eq!(m.removed, vec![e]);
    assert!(d.face(m.absorbed).is_none() && d.edge(e).is_none());
    let mid = totals(&d);
    assert_eq!(mid.faces, before.faces - 1);
    assert_eq!(mid.edges, before.edges - 1);
    near(mid.area, before.area);
    near(mid.volume, before.volume);
    checked(&d);
    // The same edge now is gone.
    assert!(matches!(
        d.merge_faces_across(e, &none(), &doc()),
        Err(Refusal::Absent(_))
    ));
    d.heal(&none(), &doc()).unwrap();
    assert_eq!(counts(&d), (6, 12, 8));
    checked(&d);
}

#[test]
fn one_join_by_name() {
    // The middle vertex of a cube edge cut in two, once the faces round it are merged.
    let l = lattice([1.0; 3], [2, 1, 1]);
    let mut d = l.draft;
    d.merge_faces(&none(), &doc()).unwrap();
    let a = l.edges[&(0, [0, 0, 0])];
    let b = l.edges[&(0, [1, 0, 0])];
    let v = d.edge(a).unwrap().ends()[1];
    assert_eq!(d.edge(b).unwrap().ends()[0], v);
    let j = d.join_edges_at(v, &none()).unwrap();
    assert_eq!((j.survivor, j.absorbed, j.vertex), (a, b, v));
    assert!(d.edge(b).is_none() && d.vertex(v).is_none());
    let EdgeShape::Curve { interval, .. } = d.edge(a).unwrap().shape().unwrap().clone() else {
        panic!("a curve");
    };
    assert_eq!((interval.min(), interval.max()), (0.0, 1.0));
    checked(&d);
}

#[test]
fn faces_on_other_shells_are_left_and_listed() {
    // Two faces on one surface whose sides are not the same shells are not one face.
    let mut d = lattice([2.0, 1.0, 1.0], [2, 1, 1]).draft;
    let shell = d.add_shell(ShellRecord::new(RegionId::OUTSIDE)).unwrap();
    // One quad of a side cut in two, its back side on a shell of its own.
    let faces: Vec<_> = d.faces().map(|f| (f.id(), f.surface().clone())).collect();
    let f = faces
        .iter()
        .find(|(id, s)| faces.iter().any(|(o, t)| o != id && t == s))
        .unwrap()
        .0;
    let [front, _] = d.face(f).unwrap().record().sides;
    d.face_mut(f).unwrap().sides = [front, shell];
    let report = d.merge_faces(&none(), &doc()).unwrap();
    assert!(
        report.left.iter().any(|l| l.why == Why::Sides),
        "{report:?}"
    );
}

#[test]
fn a_disc_in_a_plate_merges_across_its_circle() {
    // A plate cut by a circle: the disc and the annulus share one closed edge, and merging
    // them leaves the plate with no edge inside (the pin's circular split lines). The
    // annulus's inner loop and the disc's only loop are twins and both go.
    let mut d = Draft::new();
    let outer = d.add_shell(ShellRecord::new(RegionId::OUTSIDE)).unwrap();
    let plate = Surface::Plane(Plane::new(Frame::WORLD));
    let corners = [(-2.0, -2.0), (2.0, -2.0), (2.0, 2.0), (-2.0, 2.0)];
    let vs: Vec<VertexId> = corners
        .iter()
        .map(|&(x, y)| {
            d.add_vertex(VertexRecord::new(Point3::new(x, y, 0.0)))
                .unwrap()
        })
        .collect();
    let rim = d
        .add_vertex(VertexRecord::new(Point3::new(1.0, 0.0, 0.0)))
        .unwrap();
    let circle =
        Curve::Circle(Circle::new(Frame::WORLD, Length::from_mm(1.0), Sense::Forward).unwrap());
    let round = Extent1d::new(0.0, TAU).unwrap();
    let ring = d
        .add_edge(EdgeGeom::new(
            EdgeShape::Curve {
                curve: circle.clone(),
                interval: round,
            },
            [rim, rim],
        ))
        .unwrap();
    let mut sides = Vec::new();
    let unit = Extent1d::new(0.0, 1.0).unwrap();
    for i in 0..4 {
        let (p, q) = (corners[i], corners[(i + 1) % 4]);
        let line = Line::new(
            Point3::new(p.0, p.1, 0.0),
            Vec3::new(q.0 - p.0, q.1 - p.1, 0.0),
        )
        .unwrap();
        let e = d
            .add_edge(EdgeGeom::new(
                EdgeShape::Curve {
                    curve: Curve::Line(line),
                    interval: unit,
                },
                [vs[i], vs[(i + 1) % 4]],
            ))
            .unwrap();
        sides.push((e, Curve::Line(line)));
    }
    let outer_loop: Vec<EdgeUse> = sides
        .iter()
        .map(|(e, c)| EdgeUse::new(*e, Sense::Forward, pcurve(c, unit, &plate)))
        .collect();
    // The hole runs clockwise in the annulus, the disc counter-clockwise.
    let hole = vec![EdgeUse::new(
        ring,
        Sense::Reversed,
        pcurve(&circle, round, &plate),
    )];
    let disc = vec![EdgeUse::new(
        ring,
        Sense::Forward,
        pcurve(&circle, round, &plate),
    )];
    d.add_face(FaceRecord::new(
        plate.clone(),
        [outer, outer],
        vec![LoopRecord::new(outer_loop), LoopRecord::new(hole)],
    ))
    .unwrap();
    d.add_face(FaceRecord::new(
        plate,
        [outer, outer],
        vec![LoopRecord::new(disc)],
    ))
    .unwrap();
    assert_eq!(counts(&d), (2, 5, 5));
    checked(&d);
    let report = d.heal(&none(), &doc()).unwrap();
    assert_eq!(report.faces.len(), 1);
    assert_eq!(counts(&d), (1, 4, 4));
    assert!(d.vertex(rim).is_none() && d.edge(ring).is_none());
    checked(&d);
}

/// Every record of the draft, in id order, as text: two drafts with equal dumps are equal.
fn dump(d: &Draft) -> String {
    let mut s = String::new();
    for f in d.faces() {
        writeln!(s, "{:?} {:?}", f.id(), f.record()).unwrap();
    }
    for e in d.edges() {
        writeln!(s, "{:?} {:?} {:?}", e.id(), e.geometry(), e.adjacency()).unwrap();
    }
    for v in d.vertices() {
        writeln!(s, "{:?}", v.id()).unwrap();
    }
    s
}

/// The vertex ids of a draft, in order.
fn vertex_ids(d: &Draft) -> Vec<VertexId> {
    d.vertices().map(scx_brep::Vertex::id).collect()
}

#[test]
fn joining_a_chain_at_once_equals_the_single_joins() {
    // `join_edges` takes a chain of edges on one curve through free vertices as one join;
    // `join_edges_at` is the single join. Every pair joined by hand, in the order of the
    // vertices, leaves the same body (same records, same ids).
    let cases = [
        ([10.0, 10.0, 10.0], [2, 1, 1]),
        ([6.0, 5.0, 4.0], [3, 4, 2]),
        ([30.0, 5.0, 4.0], [30, 1, 2]),
        ([6.0, 5.0, 4.0], [6, 5, 4]),
    ];
    for (len, n) in cases {
        let l = lattice(len, n);
        let kept = top_line(&l, n);
        for protected in [none(), kept] {
            let mut a = l.draft.clone();
            a.merge_faces(&protected, &doc()).unwrap();
            let mut b = a.clone();
            let joined = a.join_edges(&protected).unwrap();
            let mut count = 0;
            let mut still = protected.clone();
            for v in vertex_ids(&b) {
                if let Ok(j) = b.join_edges_at(v, &still) {
                    still.remove(&j.absorbed);
                    count += 1;
                }
            }
            assert_eq!(joined.edges.len(), count, "{n:?}");
            assert_eq!(dump(&a), dump(&b), "{n:?} {}", protected.len());
            checked(&a);
        }
    }
    for k in [1, 2, 3, 5, 12] {
        let mut a = generate(Kind::Cylinder, 3, k).unwrap().draft;
        a.merge_faces(&none(), &doc()).unwrap();
        let mut b = a.clone();
        a.join_edges(&none()).unwrap();
        for v in vertex_ids(&b) {
            let _ = b.join_edges_at(v, &none());
        }
        assert_eq!(dump(&a), dump(&b), "cylinder {k}");
    }
}
