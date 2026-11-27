// Written for solidcorex. Apache-2.0.
//! The pin's provenance, history and attribute tests (63 §6.13), ported onto [`Changes`] and
//! [`carry`]. The operations are not in scx yet, so each test makes by hand the edit the
//! pin's operation makes, declares the lineage that operation would declare, and checks the
//! outcome the pin checks. Each test names the pin tests it ports (READ, under
//! `solidcore/crates/`); where the pin's answer is wrong by truth, the test says so and
//! checks the truth.
//!
//! The edits are bookkeeping (a boolean or a blend made by hand is not a valid body), so
//! they finish through [`done`], which checks references only. What is checked is never a
//! count the hand edit fixes by construction: it is what `Changes` reports and what `carry`
//! hands on, given the lineage.

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use super::*;

/// The edited body's operand number in a boolean.
const TOOL: Operand = Operand::from_raw(1);

fn op_face(raw: u32) -> Origin {
    Origin::Operand(TOOL, Entity::Face(FaceId::from_raw(raw)))
}

/// Adds a face over square `i` of the strip: a face whose own geometry the port does not
/// need (a copied face, an operand's face, a wall).
fn face_over(d: &mut Draft, s: &Strip, i: usize) -> FaceId {
    let lp = square(d, [s.bottom[i], s.rungs[i + 1], s.top[i], s.rungs[i]]);
    d.add_face(FaceRecord::new(plane(0.0), [s.shell, s.shell], vec![lp]))
        .unwrap()
}

/// Moves `f`'s uses of `from` onto `to`, keeping the fins in step.
fn reroute(d: &mut Draft, f: FaceId, from: EdgeId, to: EdgeId) {
    d.edit_face(f, |rec| {
        for u in rec.loops.iter_mut().flat_map(|l| l.uses.iter_mut()) {
            if u.edge == from {
                u.edge = to;
            }
        }
    })
    .unwrap();
}

/// A boundary-only write: the loop started at another use (a trimmed face, for these
/// bookkeeping edits).
fn trim(d: &mut Draft, f: FaceId) {
    d.edit_face(f, |rec| rec.loops[0].uses.rotate_left(1))
        .unwrap();
}

/// A use of `edge` with a stand-in pcurve at `at`: for an edge whose xy is a point (an
/// upright), in edits that check references only.
fn use_at(edge: EdgeId, sense: Sense, at: Point2) -> EdgeUse {
    let pc = Line2::new(at, Vec2::new(1.0, 0.0)).unwrap();
    let geom = UseGeom::Pcurve {
        curve: Curve2::Line(pc),
        interval: unit(),
    };
    EdgeUse::new(edge, sense, geom)
}

/// Separate rectangles `[x0, y0, x1, y1]` in the plane `z = 0`, each with its own vertices
/// and edges (bottom, right, top, left), finished, and a draft of the result: sheet faces
/// before a stitch.
struct Sheets {
    draft: Draft,
    faces: Vec<FaceId>,
    edges: Vec<[EdgeId; 4]>,
    /// `(x0, y0)`, `(x1, y0)`, `(x1, y1)`, `(x0, y1)`.
    verts: Vec<[VertexId; 4]>,
}

fn sheets(rects: &[[f64; 4]]) -> Sheets {
    let mut d = Draft::new();
    let shell = d.add_shell(ShellRecord::new(RegionId::OUTSIDE)).unwrap();
    let (mut faces, mut edges, mut verts) = (Vec::new(), Vec::new(), Vec::new());
    for &[x0, y0, x1, y1] in rects {
        let v = [(x0, y0), (x1, y0), (x1, y1), (x0, y1)].map(|(x, y)| {
            d.add_vertex(VertexRecord::new(Point3::new(x, y, 0.0)))
                .unwrap()
        });
        let e = [
            segment(&mut d, [v[0], v[1]]),
            segment(&mut d, [v[1], v[2]]),
            segment(&mut d, [v[3], v[2]]),
            segment(&mut d, [v[0], v[3]]),
        ];
        let lp = square(&d, e);
        faces.push(
            d.add_face(FaceRecord::new(plane(0.0), [shell, shell], vec![lp]))
                .unwrap(),
        );
        edges.push(e);
        verts.push(v);
    }
    let (body, _) = done(d);
    Sheets {
        draft: body.edit(),
        faces,
        edges,
        verts,
    }
}

/// SMLib's `SM_AB_TEMP`: kept where nothing happened, never copied (pin
/// `geom/attribute.rs`, `Behavior::Temp`: "not copied, not persistent").
struct Temp;

impl Carry<u32> for Temp {
    fn split(&self, _: &u32) -> Option<u32> {
        None
    }

    fn merge(&self, _: &[&u32]) -> Option<u32> {
        None
    }

    fn coincident(&self, onto: Option<&u32>, _: &u32) -> Option<u32> {
        onto.copied()
    }

    fn offset(&self, _: &u32) -> Option<u32> {
        None
    }
}

/// SMLib's `SM_AB_REFERENCE`: one value shared by every entity holding it.
struct Shared;

impl Carry<Arc<u32>> for Shared {
    fn split(&self, from: &Arc<u32>) -> Option<Arc<u32>> {
        Some(Arc::clone(from))
    }

    fn merge(&self, values: &[&Arc<u32>]) -> Option<Arc<u32>> {
        values.first().map(|v| Arc::clone(v))
    }

    fn coincident(&self, onto: Option<&Arc<u32>>, from: &Arc<u32>) -> Option<Arc<u32>> {
        Some(Arc::clone(onto.unwrap_or(from)))
    }

    fn offset(&self, from: &Arc<u32>) -> Option<Arc<u32>> {
        Some(Arc::clone(from))
    }
}

/// An application handler that counts its calls (pin `issue_1730`'s handlers), joining as
/// [`Join`] does.
#[derive(Default)]
struct Counting {
    splits: AtomicUsize,
    merges: AtomicUsize,
    coincidences: AtomicUsize,
}

impl Carry<Vec<u32>> for Counting {
    fn split(&self, from: &Vec<u32>) -> Option<Vec<u32>> {
        self.splits.fetch_add(1, Ordering::Relaxed);
        Join.split(from)
    }

    fn merge(&self, values: &[&Vec<u32>]) -> Option<Vec<u32>> {
        self.merges.fetch_add(1, Ordering::Relaxed);
        Join.merge(values)
    }

    fn coincident(&self, onto: Option<&Vec<u32>>, from: &Vec<u32>) -> Option<Vec<u32>> {
        self.coincidences.fetch_add(1, Ordering::Relaxed);
        Join.coincident(onto, from)
    }

    fn offset(&self, from: &Vec<u32>) -> Option<Vec<u32>> {
        Join.offset(from)
    }
}

/// A quantity shared out by a split and summed by a merge (an area): a rule for which the
/// order of a merge and a split matters.
struct Halve;

impl Carry<f64> for Halve {
    fn split(&self, from: &f64) -> Option<f64> {
        Some(from / 2.0)
    }

    fn merge(&self, values: &[&f64]) -> Option<f64> {
        Some(values.iter().copied().sum())
    }

    fn coincident(&self, onto: Option<&f64>, from: &f64) -> Option<f64> {
        Some(onto.copied().unwrap_or(0.0) + from)
    }

    fn offset(&self, from: &f64) -> Option<f64> {
        Some(*from)
    }
}

fn sorted<T: Ord + Clone>(values: impl IntoIterator<Item = T>) -> Vec<T> {
    let mut v: Vec<T> = values.into_iter().collect();
    v.sort();
    v
}

/// Ports `solidcore-ops/src/tests.rs::provenance_goes_through_splits_and_merges` (the
/// split half), `issue_1462_split_attributes.rs::slice_by_a_sheet_keeps_the_origins_of_the_
/// trimmed_faces`, `::imprint_of_a_box_by_a_plate_keeps_the_origin_on_both_pieces`,
/// `::merge_curves_on_surface_splits_a_face_onto_both_pieces`, `issue_1462_fillet_
/// attributes.rs::issue_1462_a_surface_fillet_splits_the_base_faces_with_their_origins` and
/// `issue_1462_boolean_family_attributes.rs::plain_edits_split_and_delete_as_upstream`: a
/// face cut in two, and its cut edges, hand their origin to both pieces whichever kept the
/// id; the cutting edge is new and bare. The pin's `issue_1462_boolean_2d_attributes.rs`
/// split-edge tests (`boolean_2d_union_keeps_both_pieces_of_a_split_edge_marked`,
/// `boolean_2d_intersection_keeps_the_split_edges_marks`) are this edge case.
#[test]
fn a_cut_hands_its_origin_to_both_pieces() {
    for keep_left in [true, false] {
        let (s, d, faces, [bs, ts]) = cut(keep_left);
        let (_, changes) = done(d);
        let carried = carry(&changes.faces, &tags(&s.faces), &[], &Join);
        assert_eq!(sorted(carried.keys().copied()), sorted(faces));
        assert!(carried.values().all(|v| v == &[0]));
        let mut edges = s.bottom.clone();
        edges.extend(&s.top);
        edges.extend(&s.rungs);
        let own = tags(&edges);
        let carried = carry(&changes.edges, &own, &[], &Join);
        for half in bs {
            assert_eq!(carried.get(&half), own.get(&s.bottom[0]));
        }
        for half in ts {
            assert_eq!(carried.get(&half), own.get(&s.top[0]));
        }
        // Two halves of each of two edges, and the two rungs: the middle edge is bare.
        assert_eq!(carried.len(), 6, "keep_left {keep_left}");
    }
}

/// Ports `issue_1462_boolean_attributes.rs`'s union, difference and intersection tests and
/// `issue_1462_boolean_family_attributes.rs`'s operand-list tests: every face of the result
/// carries exactly one origin (the edited body's, whole or trimmed, or the tool's), a face
/// inside the other body goes, and the new intersection edges are bare.
#[test]
fn a_boolean_gives_each_face_one_origin() {
    let (s, _) = edit_of(3);
    let mut draft = s.draft.clone();
    let d = &mut draft;
    let [f0, f1, f2] = [s.faces[0], s.faces[1], s.faces[2]];
    // f0 whole, f1 inside the tool (a difference's or a union's lost face), f2 trimmed.
    d.remove_face(f1).unwrap();
    trim(d, f2);
    // The tool's two faces that remain, copied in.
    let t0 = face_over(d, &s, 1);
    d.derive(t0, op_face(0)).unwrap();
    let t1 = face_over(d, &s, 1);
    d.derive(t1, op_face(1)).unwrap();
    // An intersection edge: new, from nothing.
    let x = segment(d, [s.verts[1][0], s.verts[2][1]]);
    let (_, changes) = done(draft);
    assert_eq!(changes.faces.removed, [f1]);
    assert_eq!(changes.faces.touch(f2), Some(Touch::Boundary));
    let tool: BTreeMap<FaceId, Vec<u32>> = [
        (FaceId::from_raw(0), vec![10]),
        (FaceId::from_raw(1), vec![11]),
    ]
    .into();
    let carried = carry(
        &changes.faces,
        &tags(&s.faces),
        &[&BTreeMap::new(), &tool],
        &Join,
    );
    assert_eq!(
        carried,
        [(f0, vec![0]), (f2, vec![2]), (t0, vec![10]), (t1, vec![11])].into()
    );
    let mut edges = s.bottom.clone();
    edges.extend(&s.top);
    edges.extend(&s.rungs);
    let carried = carry(&changes.edges, &tags(&edges), &[], &Join);
    assert_eq!(carried.get(&x), None);
    assert_eq!(carried.len(), edges.len());
}

/// Ports `issue_1462_boolean_attributes.rs::temporary_attributes_are_not_propagated_and_
/// references_stay_shared`. A reference is one value on every face that holds it; a
/// temporary mark is never copied. The pin keeps the temporary mark only on the three faces
/// the union leaves whole, because SMLib trims a face by splitting it and deleting the
/// waste, and the split drops the mark. In scx a trimmed face keeps its id and is
/// `replaced`, so `carry` keeps the mark on it too; an application that wants the pin's
/// "not persistent" drops it where `replaced` says the face was written.
#[test]
fn a_reference_stays_shared_and_a_temporary_mark_stays_put() {
    let (s, _) = edit_of(6);
    let mut draft = s.draft.clone();
    let d = &mut draft;
    let f = s.faces.clone();
    // Three faces whole, three trimmed, and a face cut (the slice the temporary mark must
    // not survive).
    for &t in &f[3..] {
        trim(d, t);
    }
    let piece = face_over(d, &s, 5);
    d.derive(piece, Origin::Face(f[5])).unwrap();
    let tool_face = face_over(d, &s, 0);
    d.derive(tool_face, op_face(0)).unwrap();
    let (_, changes) = done(draft);

    let shared = Arc::new(1);
    let refs: BTreeMap<FaceId, Arc<u32>> = f.iter().map(|&id| (id, Arc::clone(&shared))).collect();
    let carried = carry(&changes.faces, &refs, &[], &Shared);
    // Six faces and the piece: one value, shared.
    assert_eq!(carried.len(), 7);
    assert!(carried.values().all(|v| Arc::ptr_eq(v, &shared)));
    assert_eq!(Arc::strong_count(&shared), 1 + refs.len() + carried.len());

    let temp: BTreeMap<FaceId, u32> = f.iter().map(|&id| (id, 2)).collect();
    let carried = carry(&changes.faces, &temp, &[], &Temp);
    // Not copied: the cut face's pieces and the tool's face have none.
    assert_eq!(sorted(carried.keys().copied()), f[..5].to_vec());
    let whole: Vec<FaceId> = carried
        .keys()
        .copied()
        .filter(|&id| changes.faces.touch(id).is_none())
        .collect();
    assert_eq!(whole, f[..3]);
}

/// Ports `issue_1462_boolean_attributes.rs::coplanar_faces_merged_by_the_cleanup_keep_both_
/// origins`,
/// `solidcore-ops/src/tests.rs::a_union_keeps_both_boxes_provenance_on_what_it_merges` and
/// `::a_face_merge_by_delete_edge_keeps_both_faces_provenance`: a merged face holds both
/// faces' origins, whether the face absorbed was the edited body's or one copied in from the
/// tool in the same edit (the latter was lost before 63 §6.13: see `Merge::taken`). Where
/// the tool's new face survives, the body's faces it took in are `removed` (before 63 §6.13
/// they were in no list, and `carry` kept their values on ids no longer in the body).
#[test]
fn a_coplanar_merge_keeps_both_origins() {
    for tool_survives in [false, true] {
        let (s, _) = edit_of(2);
        let mut draft = s.draft.clone();
        let d = &mut draft;
        let [f0, f1] = [s.faces[0], s.faces[1]];
        // The tool's face lands on f1's plane and the union merges them.
        let t = face_over(d, &s, 1);
        d.derive(t, op_face(0)).unwrap();
        let survivor = if tool_survives {
            d.merge_face(f1, t).unwrap();
            t
        } else {
            d.merge_face(t, f1).unwrap();
            f1
        };
        // And the delete-edge merge: f0 into the survivor.
        d.merge_face(f0, survivor).unwrap();
        let (_, changes) = done(draft);
        let tool: BTreeMap<FaceId, Vec<u32>> = [(FaceId::from_raw(0), vec![10])].into();
        let carried = carry(
            &changes.faces,
            &tags(&s.faces),
            &[&BTreeMap::new(), &tool],
            &Join,
        );
        assert_eq!(
            carried,
            [(survivor, vec![0, 1, 10])].into(),
            "tool_survives {tool_survives}"
        );
        if !tool_survives {
            assert_eq!(
                changes.faces.merged,
                [Merge {
                    survivor: f1,
                    absorbed: vec![f0],
                    taken: vec![op_face(0)],
                }]
            );
        }
    }
}

/// Ports `issue_1462_boolean_2d_attributes.rs::boolean_2d_coincident_edges_keep_both_
/// sources` and `::boolean_2d_coincident_edges_with_the_same_id_keep_one_value`, and
/// `solidcore-ops/src/tests.rs::provenance_goes_through_merges_and_coincidences`: a tool's
/// edge found lying on the body's edge adds its value; the same value twice is held once. The pin accepts one value or two in the second case
/// (it depends on SMLib's handler order); scx's rule decides, so the answer is exact.
#[test]
fn a_coincident_edge_adds_its_source_once() {
    let (s, _) = edit_of(1);
    let mut draft = s.draft.clone();
    let d = &mut draft;
    let [e, same] = [s.bottom[0], s.top[0]];
    d.coincident(TOOL, EdgeId::from_raw(0), e).unwrap();
    d.coincident(TOOL, EdgeId::from_raw(1), same).unwrap();
    let (_, changes) = done(draft);
    let own: BTreeMap<EdgeId, Vec<u32>> = [(e, vec![1]), (same, vec![7])].into();
    let tool: BTreeMap<EdgeId, Vec<u32>> = [
        (EdgeId::from_raw(0), vec![2]),
        (EdgeId::from_raw(1), vec![7]),
    ]
    .into();
    let rule = Counting::default();
    let carried = carry(&changes.edges, &own, &[&BTreeMap::new(), &tool], &rule);
    assert_eq!(carried.get(&e), Some(&vec![1, 2]));
    assert_eq!(carried.get(&same), Some(&vec![7]));
    assert_eq!(rule.coincidences.load(Ordering::Relaxed), 2);
}

/// Ports `issue_1462_boolean_family_attributes.rs`'s `merge_breps` fold and tree tests: a
/// body made of three operands' faces gives each face its own operand's value. The pin's
/// counts ([6, 6, 6] and [6, 6, 3]) are those of its boxes; here two faces each.
#[test]
fn a_body_made_of_operands_keeps_each_operand_apart() {
    let s = strip(6);
    let mut d = s.draft.clone();
    let made: Vec<FaceId> = s.faces.clone();
    for (i, &f) in made.iter().enumerate() {
        let k = Operand::from_raw(i as u32 / 2);
        let raw = i as u32 % 2;
        d.derive(f, Origin::Operand(k, Entity::Face(FaceId::from_raw(raw))))
            .unwrap();
    }
    let (_, changes) = done(d);
    let operand = |k: u32| -> BTreeMap<FaceId, Vec<u32>> {
        (0..2).map(|raw| (FaceId::from_raw(raw), vec![k])).collect()
    };
    let [a, b, c] = [operand(0), operand(1), operand(2)];
    let carried = carry(&changes.faces, &BTreeMap::new(), &[&a, &b, &c], &Join);
    let mut counts = [0; 3];
    for v in carried.values() {
        counts[v[0] as usize] += 1;
    }
    assert_eq!(counts, [2, 2, 2]);
}

/// Ports `issue_1462_split_attributes.rs::stitch_and_orient_unions_the_glued_edges`: two
/// sheet faces glued along their coincident edges; the edge that survives holds both
/// origins, and the faces keep theirs.
#[test]
fn a_stitch_unions_the_glued_edges() {
    let s = sheets(&[[0.0, 0.0, 0.5, 1.0], [0.5, 0.0, 1.0, 1.0]]);
    let mut draft = s.draft.clone();
    let d = &mut draft;
    let (keep, gone) = (s.edges[0][1], s.edges[1][3]);
    reroute(d, s.faces[1], gone, keep);
    d.merge_edge(gone, keep).unwrap();
    let (_, changes) = done(draft);
    let all: Vec<EdgeId> = s.edges.iter().flatten().copied().collect();
    let carried = carry(&changes.edges, &tags(&all), &[], &Join);
    assert_eq!(carried.len(), 7);
    let mut counts: Vec<usize> = carried.values().map(Vec::len).collect();
    counts.sort_unstable();
    assert_eq!(counts, [1, 1, 1, 1, 1, 1, 2]);
    let faces = carry(&changes.faces, &tags(&s.faces), &[], &Join);
    assert_eq!(faces, tags(&s.faces));
}

/// Ports `issue_1462_split_attributes.rs::stitch_and_orient_splits_a_long_edge_onto_both_
/// pieces`: a T-junction. Face 0's right edge spans the left edges of faces 1 and 2, so the
/// stitch cuts it at their common vertex and glues each piece to one short edge. Both glued
/// edges hold the long edge's origin and their own, whichever edge of each pair the stitch
/// keeps. Before `Merge::taken`, keeping the short edge and absorbing the new piece lost the
/// long edge's origin: the answer hung on the operation's choice of survivor.
#[test]
fn a_stitch_cuts_a_long_edge_onto_both_glued_pieces() {
    let mut results = Vec::new();
    for keep_short in [false, true] {
        let s = sheets(&[
            [0.0, 0.0, 0.5, 1.0],
            [0.5, 0.0, 1.0, 0.5],
            [0.5, 0.5, 1.0, 1.0],
        ]);
        let mut draft = s.draft.clone();
        let d = &mut draft;
        let long = s.edges[0][1];
        let [low, high] = [s.edges[1][3], s.edges[2][3]];
        // Cut the long edge at face 1's upper left vertex: it keeps its lower half.
        let mid = s.verts[1][3];
        let top = d.edge(long).unwrap().end().unwrap().id();
        d.edge_mut(long).unwrap().ends[1] = mid;
        let upper = segment(d, [mid, top]);
        d.derive(upper, Origin::Edge(long)).unwrap();
        let back = use_of(d, upper, Sense::Forward);
        d.edit_face(s.faces[0], |rec| {
            let at = rec.loops[0]
                .uses
                .iter()
                .position(|u| u.edge == long)
                .unwrap();
            rec.loops[0].uses.insert(at + 1, back);
        })
        .unwrap();
        // Glue each piece to its short edge.
        let glue = |d: &mut Draft, piece: EdgeId, short: EdgeId, face: FaceId, piece_face| {
            if keep_short {
                reroute(d, piece_face, piece, short);
                d.merge_edge(piece, short).unwrap();
                short
            } else {
                reroute(d, face, short, piece);
                d.merge_edge(short, piece).unwrap();
                piece
            }
        };
        let a = glue(d, long, low, s.faces[1], s.faces[0]);
        let b = glue(d, upper, high, s.faces[2], s.faces[0]);
        let (_, changes) = done(draft);
        let all: Vec<EdgeId> = s.edges.iter().flatten().copied().collect();
        let own = tags(&all);
        let carried = carry(&changes.edges, &own, &[], &Join);
        let index = |e: EdgeId| own[&e][0];
        let mut want = [
            vec![index(long), index(low)],
            vec![index(long), index(high)],
        ];
        for v in &mut want {
            v.sort_unstable();
        }
        assert_eq!([carried[&a].clone(), carried[&b].clone()], want);
        let carriers = carried
            .values()
            .filter(|v| v.contains(&index(long)))
            .count();
        assert_eq!(carriers, 2, "keep_short {keep_short}");
        if keep_short {
            // The long edge went into `low`, and its new piece into `high`: two merges, the
            // second taking in the piece's lineage.
            let mut want = vec![
                Merge {
                    survivor: low,
                    absorbed: vec![long],
                    taken: vec![],
                },
                Merge {
                    survivor: high,
                    absorbed: vec![],
                    taken: vec![Origin::Edge(long)],
                },
            ];
            want.sort_by_key(|m| m.survivor);
            assert_eq!(changes.edges.merged, want);
        } else {
            // The long edge cut into two pieces, each of which took in a short edge.
            let split = changes
                .edges
                .split_of(long)
                .map(|s| sorted(s.pieces.clone()));
            assert_eq!(split, Some(sorted([a, b])));
        }
        results.push(sorted(carried.into_values()));
    }
    assert_eq!(results[0], results[1]);
}

/// Ports `issue_1462_split_attributes.rs::remove_topological_vertices_unions_the_merged_
/// edges`: an edge cut (both pieces keep its origin), its pieces tagged apart by the
/// application, then merged back by a second edit. Two edits, carried one after the
/// other: no composition of `Changes` is needed.
#[test]
fn a_cut_edge_merged_back_holds_every_tag() {
    let (s, _) = edit_of(1);
    let mut draft = s.draft.clone();
    let d = &mut draft;
    let e = s.bottom[0];
    let mid = d
        .add_vertex(VertexRecord::new(Point3::new(0.5, 0.0, 0.0)))
        .unwrap();
    let end = s.verts[1][0];
    d.edge_mut(e).unwrap().ends[1] = mid;
    let n = segment(d, [mid, end]);
    d.derive(n, Origin::Edge(e)).unwrap();
    let back = use_of(d, n, Sense::Forward);
    let f = s.faces[0];
    d.edit_face(f, |rec| rec.loops[0].uses.insert(1, back))
        .unwrap();
    let (body, changes) = done(draft);
    let origin: BTreeMap<EdgeId, Vec<u32>> = [(e, vec![100])].into();
    let mut held = carry(&changes.edges, &origin, &[], &Join);
    assert_eq!(held, [(e, vec![100]), (n, vec![100])].into());
    // The application tags the pieces apart.
    held.get_mut(&e).unwrap().push(0);
    held.get_mut(&n).unwrap().push(1);
    // Remove the vertex between them: n into e.
    let mut d = body.edit();
    reroute(&mut d, f, n, e);
    d.edit_face(f, |rec| rec.loops[0].uses.remove(2)).unwrap();
    d.merge_edge(n, e).unwrap();
    d.edge_mut(e).unwrap().ends[1] = end;
    d.remove_vertex(mid).unwrap();
    let (_, changes) = done(d);
    let held = carry(&changes.edges, &held, &[], &Join);
    assert_eq!(held, [(e, vec![0, 1, 100])].into());
}

/// Ports `issue_1462_fillet_attributes.rs::issue_1462_a_rounded_box_edge_keeps_the_box_
/// origins` and its per-edge, variable, chamfer and circular siblings: a blend of the
/// edge between two faces. The blend face is made along the edge (a cross-kind origin, so
/// it carries nothing), the faces and edges beside it are trimmed and keep theirs, and the
/// blended edge's origin is on no edge. The rails lie on the faces, not on the edge: an
/// operation that declared them `Origin::Edge(e)` would make them pieces of `e` and hand
/// them its origin, which the pin forbids (hence `Origin::Edge`'s doc).
#[test]
fn a_blend_keeps_the_faces_and_drops_the_blended_edge() {
    for rails_on_edge in [false, true] {
        let (s, _) = edit_of(2);
        let mut draft = s.draft.clone();
        let d = &mut draft;
        let [f0, f1] = [s.faces[0], s.faces[1]];
        let e = s.rungs[1];
        let [low, high] = s.verts[1];
        let at = |d: &mut Draft, x: f64, y: f64| {
            d.add_vertex(VertexRecord::new(Point3::new(x, y, 0.0)))
                .unwrap()
        };
        let (a0, a1, b0, b1) = (
            at(d, 0.9, 0.0),
            at(d, 0.9, 1.0),
            at(d, 1.1, 0.0),
            at(d, 1.1, 1.0),
        );
        let rail0 = segment(d, [a0, a1]);
        let rail1 = segment(d, [b0, b1]);
        let arc0 = segment(d, [a0, b0]);
        let arc1 = segment(d, [a1, b1]);
        for (rail, face) in [(rail0, f0), (rail1, f1)] {
            let origin = if rails_on_edge {
                Origin::Edge(e)
            } else {
                Origin::Face(face)
            };
            d.derive(rail, origin).unwrap();
        }
        let blend = {
            let lp = square(d, [arc0, rail1, arc1, rail0]);
            d.add_face(FaceRecord::new(plane(0.0), [s.shell, s.shell], vec![lp]))
                .unwrap()
        };
        d.derive(blend, Origin::Edge(e)).unwrap();
        // Trim: the faces onto the rails, the edges beside onto the rails' ends.
        reroute(d, f0, e, rail0);
        reroute(d, f1, e, rail1);
        d.edge_mut(s.bottom[0]).unwrap().ends[1] = a0;
        d.edge_mut(s.top[0]).unwrap().ends[1] = a1;
        d.edge_mut(s.bottom[1]).unwrap().ends[0] = b0;
        d.edge_mut(s.top[1]).unwrap().ends[0] = b1;
        d.remove_edge(e).unwrap();
        d.remove_vertex(low).unwrap();
        d.remove_vertex(high).unwrap();
        let (_, changes) = done(draft);
        let faces = carry(&changes.faces, &tags(&s.faces), &[], &Join);
        assert_eq!(faces, tags(&s.faces), "the blend face is bare");
        assert_eq!(changes.faces.touch(f0), Some(Touch::Boundary));
        let mut edges = s.bottom.clone();
        edges.extend(&s.top);
        edges.extend(&s.rungs);
        let own = tags(&edges);
        let carried = carry(&changes.edges, &own, &[], &Join);
        let blended = own[&e][0];
        let holding_it = carried.values().filter(|v| v.contains(&blended)).count();
        if rails_on_edge {
            // The wrong lineage: the rails are reported pieces of the blended edge.
            assert_eq!(holding_it, 2);
            assert!(changes.edges.split_of(e).is_some());
        } else {
            assert_eq!(holding_it, 0);
            assert!(changes.edges.split.is_empty());
            // Every other edge keeps its origin, the trimmed ones too; the four new are bare.
            assert_eq!(carried.len(), edges.len() - 1);
            assert_eq!(changes.edges.touch(s.bottom[0]), Some(Touch::Boundary));
        }
    }
}

/// Ports `solidcore-ops/tests/offset_history.rs` (its four tests),
/// `issue_1462_offset_attributes.rs::offset_box_faces_copy_their_source_faces` and
/// `::shell_box_inner_faces_carry_their_sources`: the pin's `Change` and `FaceOrigin` read
/// off `Changes`. `Kept` is a face not listed; `Modified` a `replaced` face with
/// `Touch::Geometry`; `OffsetOf(i)` a new face with `Origin::Offset`; `AlongEdge(i)` a new
/// face with `Origin::Edge`. An offset copies (`Carry::offset`): a copied mark goes to the
/// offset face, a temporary one does not.
#[test]
fn an_offset_reads_as_the_pins_history() {
    let (s, _) = edit_of(3);
    let mut draft = s.draft.clone();
    let d = &mut draft;
    let [kept, moved, cut] = [s.faces[0], s.faces[1], s.faces[2]];
    d.face_mut(moved).unwrap().surface = plane(0.5);
    // A shell's inner wall: an offset of `cut`, which the shell opens.
    let wall = face_over(d, &s, 2);
    d.derive(wall, Origin::Offset(cut.into())).unwrap();
    d.remove_face(cut).unwrap();
    // A rounding face along the rung between `moved` and `cut`.
    let round = face_over(d, &s, 2);
    d.derive(round, Origin::Edge(s.rungs[2])).unwrap();
    let (_, changes) = done(draft);
    let f = &changes.faces;
    assert_eq!(f.touch(kept), None);
    assert!(f.origins(kept).is_none());
    assert_eq!(f.touch(moved), Some(Touch::Geometry));
    assert_eq!(
        f.origins(wall).unwrap().as_slice(),
        [Origin::Offset(cut.into())]
    );
    assert_eq!(
        f.origins(round).unwrap().as_slice(),
        [Origin::Edge(s.rungs[2])]
    );
    assert_eq!(f.removed, [cut]);
    assert!(f.split.is_empty(), "an offset is not a piece");
    let own = tags(&s.faces);
    let copied = carry(f, &own, &[], &Join);
    assert_eq!(
        copied,
        [(kept, vec![0]), (moved, vec![1]), (wall, vec![2])].into()
    );
    let temp: BTreeMap<FaceId, u32> = s.faces.iter().map(|&id| (id, 9)).collect();
    let carried = carry(f, &temp, &[], &Temp);
    assert_eq!(sorted(carried.into_keys()), [kept, moved]);
}

/// Ports `offset_history.rs`'s loops-cross test: where the offset loops cross, every face
/// is made anew, each an offset of its source; nothing is kept.
#[test]
fn an_offset_made_anew_keeps_no_face() {
    let (s, _) = edit_of(2);
    let mut draft = s.draft.clone();
    let d = &mut draft;
    let made: Vec<FaceId> = (0..2)
        .map(|i| {
            let g = face_over(d, &s, i);
            d.derive(g, Origin::Offset(s.faces[i].into())).unwrap();
            g
        })
        .collect();
    for &f in &s.faces {
        d.remove_face(f).unwrap();
    }
    let (_, changes) = done(draft);
    assert_eq!(changes.faces.removed, s.faces);
    assert!(changes.faces.replaced.is_empty());
    for (i, &g) in made.iter().enumerate() {
        assert_eq!(
            changes.faces.origins(g).unwrap().as_slice(),
            [Origin::Offset(s.faces[i].into())]
        );
    }
}

/// Ports `issue_1973_thicken_sheet_edge_attributes.rs` (its three tests) and
/// `issue_1973_thicken_flat_sheet_edge_attributes.rs`: thickening a sheet keeps the sheet's
/// edges and their marks, makes an offset of each (which copies the mark), and makes an
/// upright at each vertex (made at it, another kind: bare). The pin's (12, 8): twelve
/// edges, eight carrying.
#[test]
fn a_thickened_sheet_marks_its_edges_and_their_offsets() {
    let (s, _) = edit_of(1);
    let mut draft = s.draft.clone();
    let d = &mut draft;
    let corners = [s.verts[0][0], s.verts[1][0], s.verts[1][1], s.verts[0][1]];
    let lifted = corners.map(|v| {
        let p = d.vertex(v).unwrap().point();
        let w = d
            .add_vertex(VertexRecord::new(Point3::new(p.x, p.y, 1.0)))
            .unwrap();
        d.derive(w, Origin::Offset(v.into())).unwrap();
        w
    });
    let sheet = [s.bottom[0], s.rungs[1], s.top[0], s.rungs[0]];
    let ends = [[0, 1], [1, 2], [3, 2], [0, 3]];
    let offsets: Vec<EdgeId> = sheet
        .iter()
        .zip(ends)
        .map(|(&e, [a, b])| {
            let o = segment(d, [lifted[a], lifted[b]]);
            d.derive(o, Origin::Offset(e.into())).unwrap();
            o
        })
        .collect();
    let uprights: Vec<EdgeId> = (0..4)
        .map(|i| {
            let u = segment(d, [corners[i], lifted[i]]);
            d.derive(u, Origin::Vertex(corners[i])).unwrap();
            u
        })
        .collect();
    let top = {
        let lp = square(d, [offsets[0], offsets[1], offsets[2], offsets[3]]);
        d.add_face(FaceRecord::new(plane(1.0), [s.shell, s.shell], vec![lp]))
            .unwrap()
    };
    d.derive(top, Origin::Offset(s.faces[0].into())).unwrap();
    for (i, &e) in sheet.iter().enumerate() {
        let [a, b] = ends[i];
        let at = |k: f64| Point2::new(i as f64 * 2.0 + k, 0.0);
        let lp = LoopRecord::new(vec![
            use_of(d, e, Sense::Forward),
            use_at(uprights[b], Sense::Forward, at(0.25)),
            use_of(d, offsets[i], Sense::Reversed),
            use_at(uprights[a], Sense::Reversed, at(0.75)),
        ]);
        let side = d
            .add_face(FaceRecord::new(plane(0.0), [s.shell, s.shell], vec![lp]))
            .unwrap();
        d.derive(side, Origin::Edge(e)).unwrap();
    }
    let (body, changes) = done(draft);
    let carried = carry(&changes.edges, &tags(&sheet), &[], &Join);
    assert_eq!((body.edges().len(), carried.len()), (12, 8));
    for (i, (&e, &o)) in sheet.iter().zip(&offsets).enumerate() {
        assert_eq!(carried.get(&e), Some(&vec![i as u32]));
        assert_eq!(carried.get(&o), Some(&vec![i as u32]));
    }
    assert!(uprights.iter().all(|u| !carried.contains_key(u)));
    let faces = carry(&changes.faces, &tags(&s.faces), &[], &Join);
    assert_eq!(faces, [(s.faces[0], vec![0]), (top, vec![0])].into());
    let vertices = carry(&changes.vertices, &tags(&corners), &[], &Join);
    assert_eq!(vertices.len(), 8);
}

/// Ports `issue_1462_offset_attributes.rs::rounded_offset_edges_keep_their_sources_through_
/// the_stitch`: a rounded offset makes each source edge's offset twice (once on each face
/// beside it, before the stitch glues them, or as the two rails of the rounding face).
/// Each copy carries the source; the rounding face, made along the edge, carries nothing.
#[test]
fn a_rounded_offset_puts_each_edge_on_both_copies() {
    let (s, _) = edit_of(1);
    let mut draft = s.draft.clone();
    let d = &mut draft;
    let e = s.bottom[0];
    let copies = [0.1, 0.2].map(|y| {
        let [a, b] = [0.0, 1.0].map(|x| {
            d.add_vertex(VertexRecord::new(Point3::new(x, -y, 0.0)))
                .unwrap()
        });
        let c = segment(d, [a, b]);
        d.derive(c, Origin::Offset(e.into())).unwrap();
        c
    });
    let round = face_over(d, &s, 0);
    d.derive(round, Origin::Edge(e)).unwrap();
    let (_, changes) = done(draft);
    let carried = carry(&changes.edges, &[(e, vec![5])].into(), &[], &Join);
    assert_eq!(
        carried,
        [(e, vec![5]), (copies[0], vec![5]), (copies[1], vec![5])].into()
    );
    assert!(changes.edges.split.is_empty());
    let faces = carry(&changes.faces, &tags(&s.faces), &[], &Join);
    assert!(!faces.contains_key(&round));
}

/// Ports `solidcore-convert/tests/order_pieces.rs::an_edge_solids_touch_along_is_two_
/// pieces`: an edge with four fins (two solids touching along it) cut into two edges of two
/// fins each, with the same ends and the same interval. Their order is decided by the
/// lineage of the faces each piece bounds, so it does not depend on which piece kept the
/// id (before 63 §6.13 these tied and fell back to id order).
#[test]
fn pieces_with_the_same_ends_are_ordered_by_their_faces() {
    for move_own in [false, true] {
        let (s, _) = edit_of(2);
        let mut draft = s.draft.clone();
        let d = &mut draft;
        let e = s.rungs[1];
        let t0 = face_over(d, &s, 0);
        d.derive(t0, op_face(0)).unwrap();
        let t1 = face_over(d, &s, 1);
        d.derive(t1, op_face(1)).unwrap();
        assert_eq!(d.edge(e).unwrap().fins().count(), 4);
        let ends = [s.verts[1][0], s.verts[1][1]];
        let n = segment(d, ends);
        d.derive(n, Origin::Edge(e)).unwrap();
        let moved = if move_own {
            [s.faces[0], s.faces[1]]
        } else {
            [t0, t1]
        };
        for f in moved {
            reroute(d, f, e, n);
        }
        let (_, changes) = done(draft);
        let split = changes.edges.split_of(e).unwrap();
        // The piece bounding the edited body's own faces first.
        let want = if move_own { [n, e] } else { [e, n] };
        assert_eq!(split.pieces, want, "move_own {move_own}");
    }
}

/// Ports `issue_1730_application_attribute_handlers.rs`: `issue_minimal_test_split_merge_
/// coincident_copy`, `copy_onto_a_tagged_target_keeps_the_targets_tag`, `merge_runs_once_
/// per_attribute_id` and `imprint_splits_run_the_application_split_handler`. The
/// application's rule runs once per event (each piece of a split, each merge), and a value
/// copied onto a tagged survivor joins it. The pin's
/// `imprint_splits_run_the_application_split_handler` hands the piece that keeps the id a
/// different value; `Carry::split` is told nothing of the piece, so an application that
/// wants that reads `Split::from` among the pieces afterwards, as here.
#[test]
fn the_application_rule_runs_once_per_event() {
    let (s, mut d, [left, right], _) = cut(true);
    d.merge_face(right, left).unwrap();
    d.coincident(TOOL, FaceId::from_raw(0), left).unwrap();
    let (_, changes) = done(d);
    // A cut whose new piece merged back into the old: one face, holding what it held.
    assert!(changes.faces.split.is_empty());
    let rule = Counting::default();
    let tool: BTreeMap<FaceId, Vec<u32>> = [(FaceId::from_raw(0), vec![4])].into();
    let carried = carry(
        &changes.faces,
        &tags(&s.faces),
        &[&BTreeMap::new(), &tool],
        &rule,
    );
    assert_eq!(carried, [(left, vec![0, 4])].into());
    assert_eq!(rule.coincidences.load(Ordering::Relaxed), 1);

    // A plain cut: the split handler runs for each piece, once; the id-keeper is found by
    // the application from the split.
    let (s, d, [left, right], _) = cut(false);
    let (_, changes) = done(d);
    let rule = Counting::default();
    let mut carried = carry(&changes.faces, &tags(&s.faces), &[], &rule);
    assert_eq!(rule.splits.load(Ordering::Relaxed), 2);
    assert_eq!(rule.merges.load(Ordering::Relaxed), 0);
    for split in &changes.faces.split {
        if split.pieces.contains(&split.from) {
            carried.get_mut(&split.from).unwrap().push(99);
        }
    }
    assert_eq!(carried, [(left, vec![0]), (right, vec![0, 99])].into());
}

/// Ports `merge_after_copy_faces_keeps_attributes.rs` and `issue_1462_split_attributes.rs::
/// copy_faces_with_a_merge_copies_the_face_attributes`: faces copied into another body,
/// then merged there by a second edit; the survivor holds both. Two edits, carried in turn.
#[test]
fn copied_faces_merged_later_hold_both() {
    let (s, _) = edit_of(2);
    let mut draft = s.draft.clone();
    let d = &mut draft;
    let copies: Vec<FaceId> = (0..2)
        .map(|i| {
            let g = face_over(d, &s, i);
            d.derive(g, op_face(i as u32)).unwrap();
            g
        })
        .collect();
    let (body, changes) = done(draft);
    let source: BTreeMap<FaceId, Vec<u32>> = [
        (FaceId::from_raw(0), vec![1]),
        (FaceId::from_raw(1), vec![2]),
    ]
    .into();
    let held = carry(
        &changes.faces,
        &BTreeMap::new(),
        &[&BTreeMap::new(), &source],
        &Join,
    );
    assert_eq!(held, [(copies[0], vec![1]), (copies[1], vec![2])].into());
    let mut d = body.edit();
    d.merge_face(copies[1], copies[0]).unwrap();
    let (_, changes) = done(d);
    let held = carry(&changes.faces, &held, &[], &Join);
    assert_eq!(held, [(copies[0], vec![1, 2])].into());
}

/// Ports `issue_1875_local_operation_marks.rs`, `issue_1462_split_attributes.rs::local_
/// operation_keeps_every_face_origin`, the cliff overflow of `issue_1980` and
/// `issue_1462_sweep_attributes.rs`'s in-place sweep: a face moved, an edge shortened and a
/// vertex moved keep their ids and their marks; the swept block's new faces are bare.
#[test]
fn a_local_operation_keeps_every_mark() {
    let (s, _) = edit_of(2);
    let mut draft = s.draft.clone();
    let d = &mut draft;
    d.face_mut(s.faces[1]).unwrap().surface = plane(0.25);
    d.vertex_mut(s.verts[2][0]).unwrap().point = Point3::new(1.5, 0.0, 0.0);
    let short = d
        .add_vertex(VertexRecord::new(Point3::new(1.5, 1.0, 0.0)))
        .unwrap();
    d.edge_mut(s.top[1]).unwrap().ends[1] = short;
    let block = face_over(d, &s, 1);
    let (_, changes) = done(draft);
    assert_eq!(changes.faces.touch(s.faces[1]), Some(Touch::Geometry));
    assert_eq!(changes.edges.touch(s.top[1]), Some(Touch::Boundary));
    assert_eq!(
        changes.faces.origins(block).unwrap().as_slice(),
        [Origin::Free]
    );
    assert_eq!(
        carry(&changes.faces, &tags(&s.faces), &[], &Join),
        tags(&s.faces)
    );
    assert_eq!(
        carry(&changes.edges, &tags(&s.top), &[], &Join),
        tags(&s.top)
    );
    let corners: Vec<VertexId> = s.verts.iter().flatten().copied().collect();
    assert_eq!(
        carry(&changes.vertices, &tags(&corners), &[], &Join),
        tags(&corners)
    );
}

/// Ports `issue_1462_boolean_family_attributes.rs::plain_remove_faces_clears_the_deleted_
/// face_attributes`, `issue_1462_boolean_attributes.rs::clone_copies_attributes_and_dead_
/// entities_have_none`, `solidcore-ops/src/tests.rs`'s `a_copy_keeps_the_provenance_on_its_
/// faces_and_edges`, `attached_provenance_reads_back_by_record_index` and `a_copy_maps_
/// record_order_through_renumbered_entities`, and the attribute parts of
/// `copy_keeps_attributes_default_naming.rs`, `copy_faces_merge_key_aliasing.rs` and
/// `issue_2010_plain_brep_copy.rs`: a removed face's value goes; a copy of a body is the
/// same ids, so the table is the copy's as it is; across `compact` the table is renumbered.
#[test]
fn a_removed_face_drops_its_value_and_a_copy_keeps_all() {
    let (s, body) = edit_of(3);
    let copy = body.clone();
    assert!(
        copy.faces()
            .zip(body.faces())
            .all(|(a, b)| a.id() == b.id())
    );
    let mut draft = s.draft.clone();
    let d = &mut draft;
    d.remove_face(s.faces[0]).unwrap();
    let (edited, changes) = done(draft);
    let carried = carry(&changes.faces, &tags(&s.faces), &[], &Join);
    assert_eq!(
        carried,
        [(s.faces[1], vec![1]), (s.faces[2], vec![2])].into()
    );
    let (compact, map) = edited.compact();
    let renumbered = map.renumber(carried);
    assert_eq!(
        sorted(renumbered.keys().copied()),
        sorted(compact.faces().map(Face::id))
    );
}

/// The evidence against `Changes::then` (63 §6.13): one `Changes` cannot say whether a
/// merge came before a split. Merging two faces and then cutting the result, as two edits
/// carried in turn, gives each piece half the merged quantity; the same events in one edit
/// (which is what a composition would report) give each piece the absorbed face's whole
/// value and half the survivor's. For a rule whose split copies (every behaviour of the
/// pin's attributes: `Join` here) the two agree, and the one-edit answer holds both faces'
/// marks on both pieces because a piece of a survivor stands for what it took in.
#[test]
fn a_merge_then_a_cut_is_carried_edit_by_edit() {
    let new_piece = |d: &mut Draft, s: &Strip| {
        let g = face_over(d, s, 1);
        d.derive(g, Origin::Face(s.faces[0])).unwrap();
        g
    };
    // Two edits.
    let (mut s, _) = edit_of(2);
    s.draft.merge_face(s.faces[1], s.faces[0]).unwrap();
    let (body, first) = done(s.draft.clone());
    let mut d = body.edit();
    let g = new_piece(&mut d, &s);
    let (_, second) = done(d);
    let area: BTreeMap<FaceId, f64> = [(s.faces[0], 1.0), (s.faces[1], 1.0)].into();
    let twice = carry(
        &second.faces,
        &carry(&first.faces, &area, &[], &Halve),
        &[],
        &Halve,
    );
    assert_eq!(twice, [(s.faces[0], 1.0), (g, 1.0)].into());
    let marks = tags(&s.faces);
    let joined = carry(
        &second.faces,
        &carry(&first.faces, &marks, &[], &Join),
        &[],
        &Join,
    );

    // One edit.
    let (mut s, _) = edit_of(2);
    s.draft.merge_face(s.faces[1], s.faces[0]).unwrap();
    let mut draft = s.draft.clone();
    let g1 = new_piece(&mut draft, &s);
    assert_eq!(g1, g);
    let (_, once) = done(draft);
    assert_eq!(
        once.faces.origins(g).unwrap().as_slice(),
        [Origin::Face(s.faces[0]), Origin::Face(s.faces[1])]
    );
    let at_once = carry(&once.faces, &area, &[], &Halve);
    assert_eq!(at_once, [(s.faces[0], 1.5), (g, 1.5)].into());
    assert_eq!(carry(&once.faces, &marks, &[], &Join), joined);
    assert_eq!(joined, [(s.faces[0], vec![0, 1]), (g, vec![0, 1])].into());
}
