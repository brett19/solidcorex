// Written for solidcorex. Apache-2.0.
//! A draft of a finished body checks only what it touched (research 63 §6.14), and must
//! refuse exactly when checking the whole body would, with the same cause.
//!
//! Every generated body (BR8) is finished, then edited a few rounds at random: valid edits
//! that keep it finishing (a vertex nudged within the resolution, a face replaced, a wire
//! added and removed, records written unchanged) and edits that break one rule or several
//! (records removed under their references, loose vertices and edges, fins dropped or
//! doubled, uses re-sensed, sides swapped, empty shells and regions, ends swapped, a curve
//! left pending). Each round `finish` and `finish_checking_all` must agree; where both
//! finish, the bodies' boxes must agree too, with each other and with boxes built afresh.
//! A round that finishes is the base of the next, so drafts of drafts, base wires and
//! seeded boxes are covered.

#![allow(clippy::unwrap_used, clippy::indexing_slicing, clippy::panic)]

use std::collections::BTreeMap;
use std::sync::Arc;

use scx_brep::{
    Brep, Document, Draft, EdgeId, FaceId, FaceSide, FinishCause, RegionId, ShellId, ShellRecord,
    VertexId, VertexRecord,
};
use scx_num::cert::{Basis, Recorded};
use scx_num::frame::Sense;
use scx_num::hash::Rng;
use scx_num::math::Affine;
use scx_num::units::{Accuracy, Length};
use scx_num::vector3d::{Point3, Vec3};
use scx_testkit::bodies::{Kind, generate};

struct Dice(Rng);

impl Dice {
    fn below(&mut self, n: usize) -> usize {
        (self.0.next_u64() % n.max(1) as u64) as usize
    }

    fn chance(&mut self, num: usize, den: usize) -> bool {
        self.below(den) < num
    }

    fn pick<T: Copy>(&mut self, items: &[T]) -> Option<T> {
        (!items.is_empty()).then(|| items[self.below(items.len())])
    }
}

fn faces(d: &Draft) -> Vec<FaceId> {
    d.faces().map(scx_brep::Face::id).collect()
}
fn edges(d: &Draft) -> Vec<EdgeId> {
    d.edges().map(scx_brep::Edge::id).collect()
}
fn vertices(d: &Draft) -> Vec<VertexId> {
    d.vertices().map(scx_brep::Vertex::id).collect()
}
fn shells(d: &Draft) -> Vec<ShellId> {
    d.shells().map(scx_brep::Shell::id).collect()
}

/// The ops, by name; the first `VALID` keep a valid body valid.
const OPS: [&str; 37] = [
    "nudge vertex",
    "loosen vertex",
    "loosen edge",
    "replace face",
    "add wire",
    "remove wire",
    "touch edge",
    "touch face",
    "touch shell",
    "vertex into new",
    "face into new",
    "move rigidly",
    "mirror",
    "move vertex",
    "remove face",
    "remove edge",
    "remove vertex",
    "add loose vertex",
    "add loose edge",
    "re-sense use",
    "swap sides",
    "reverse fins",
    "drop fin",
    "double fin",
    "wire with fins",
    "lone vertex",
    "empty shell",
    "bare region",
    "remove shell",
    "remove region",
    "swap ends",
    "merge vertices",
    "clear loops",
    "solid outside",
    "move shell",
    "pending edge",
    "move wire",
];
const VALID: usize = 13;

/// A recorded accuracy far inside any check's reach, but enough to grow boxes.
fn loose(dice: &mut Dice) -> Arc<Recorded> {
    let mm = [1e-7, 1e-6, 1e-5][dice.below(3)];
    Arc::new(Recorded {
        acc: Accuracy::new(Length::from_mm(mm)).unwrap(),
        basis: Basis::Measured,
    })
}

/// Applies op `op` to `d`; whether it found something to act on.
fn apply(d: &mut Draft, op: usize, dice: &mut Dice) -> Option<()> {
    match OPS[op] {
        "nudge vertex" => {
            let v = dice.pick(&vertices(d))?;
            let p = d.vertex_mut(v)?;
            p.point += Vec3::new(1e-12, 0.0, 0.0);
        }
        "loosen vertex" => {
            let v = dice.pick(&vertices(d))?;
            d.vertex_mut(v)?.accuracy = Some(loose(dice));
        }
        "loosen edge" => {
            let e = dice.pick(&edges(d))?;
            d.edge_mut(e)?.accuracy = Some(loose(dice));
        }
        "replace face" => {
            let f = dice.pick(&faces(d))?;
            let rec = d.remove_face(f)?;
            d.add_face(rec).ok()?;
        }
        "add wire" => {
            // A copy of a face's edge, as a wire of the shell on one of its sides.
            let e = dice.pick(&edges(d))?;
            let fin = d.edge(e)?.fins().next()?;
            let side = if dice.chance(1, 2) {
                FaceSide::Front
            } else {
                FaceSide::Back
            };
            let shell = d.face(fin.face)?.record().side(side);
            let geom = d.edge(e)?.geometry().clone();
            let w = d.add_edge(geom).ok()?;
            d.shell_mut(shell)?.wires.push(w);
        }
        "remove wire" => {
            let (s, w) = d
                .shells()
                .find_map(|s| Some((s.id(), *s.record().wires.last()?)))?;
            d.shell_mut(s)?.wires.pop();
            d.remove_edge(w)?;
        }
        "vertex into new" => {
            // A copy of a vertex takes its edges, and the vertex merges into the copy: a kept
            // entity gone into a new one.
            let v = dice.pick(&vertices(d))?;
            let copy = d.vertex(v)?.record().clone();
            let at: Vec<EdgeId> = d
                .edges()
                .filter(|e| e.ends().contains(&v))
                .map(scx_brep::Edge::id)
                .collect();
            let n = d.add_vertex(copy).ok()?;
            for e in at {
                for end in &mut d.edge_mut(e)?.ends {
                    if *end == v {
                        *end = n;
                    }
                }
            }
            for s in shells(d) {
                if d.shell(s)?.record().lone_vertices.contains(&v) {
                    for lone in &mut d.shell_mut(s)?.lone_vertices {
                        if *lone == v {
                            *lone = n;
                        }
                    }
                }
            }
            d.merge_vertex(v, n)?;
        }
        "face into new" => {
            let f = dice.pick(&faces(d))?;
            let copy = d.face(f)?.record().clone();
            let n = d.add_face(copy).ok()?;
            d.merge_face(f, n)?;
        }
        "move rigidly" => {
            // Every record written at once, through the transform's own writes.
            let turn =
                Affine::rotation(Point3::new(0.3, -1.2, 0.7), Vec3::new(1.0, 2.0, -0.5), 0.9)
                    .ok()?;
            let a = Affine::translation(Vec3::new(4.0, -3.5, 12.25)) * turn;
            d.transform(&a, &Document::default()).ok()?;
        }
        "mirror" => {
            // As a move, and every edge's fins reversed.
            let a =
                Affine::reflection(Point3::new(0.5, 1.0, -2.0), Vec3::new(0.3, -1.0, 0.8)).ok()?;
            d.transform(&a, &Document::default()).ok()?;
        }
        "touch edge" => {
            let e = dice.pick(&edges(d))?;
            d.edge_mut(e)?;
        }
        "touch face" => {
            let f = dice.pick(&faces(d))?;
            d.face_mut(f)?;
        }
        "touch shell" => {
            let s = dice.pick(&shells(d))?;
            d.shell_mut(s)?;
        }
        "move vertex" => {
            let v = dice.pick(&vertices(d))?;
            let p = d.vertex_mut(v)?;
            p.point += Vec3::new(0.25, -0.125, 0.0625);
        }
        "remove face" => {
            let f = dice.pick(&faces(d))?;
            d.remove_face(f)?;
        }
        "remove edge" => {
            let e = dice.pick(&edges(d))?;
            d.remove_edge(e)?;
        }
        "remove vertex" => {
            let v = dice.pick(&vertices(d))?;
            d.remove_vertex(v)?;
        }
        "add loose vertex" => {
            let v = dice.pick(&vertices(d))?;
            let p = d.vertex(v)?.point();
            d.add_vertex(VertexRecord::new(p)).ok()?;
        }
        "add loose edge" => {
            let e = dice.pick(&edges(d))?;
            let geom = d.edge(e)?.geometry().clone();
            d.add_edge(geom).ok()?;
        }
        "re-sense use" => {
            let f = dice.pick(&faces(d))?;
            let n = d.face(f)?.record().loops.first()?.uses.len();
            let i = dice.below(n);
            d.edit_face(f, |r| {
                if let Some(u) = r.loops.first_mut().and_then(|l| l.uses.get_mut(i)) {
                    u.sense = match u.sense {
                        Sense::Forward => Sense::Reversed,
                        Sense::Reversed => Sense::Forward,
                    };
                }
            })?;
        }
        "swap sides" => {
            let f = dice.pick(&faces(d))?;
            d.face_mut(f)?.sides.reverse();
        }
        "reverse fins" => {
            let e = dice.pick(&edges(d))?;
            d.adjacency_mut(e)?.fins.reverse();
        }
        "drop fin" => {
            let e = dice.pick(&edges(d))?;
            d.adjacency_mut(e)?.fins.pop()?;
        }
        "double fin" => {
            let e = dice.pick(&edges(d))?;
            let a = d.adjacency_mut(e)?;
            let fin = *a.fins.first()?;
            a.fins.push(fin);
        }
        "wire with fins" => {
            let e = dice.pick(&edges(d))?;
            let s = dice.pick(&shells(d))?;
            d.shell_mut(s)?.wires.push(e);
        }
        "lone vertex" => {
            let v = dice.pick(&vertices(d))?;
            let s = dice.pick(&shells(d))?;
            d.shell_mut(s)?.lone_vertices.push(v);
        }
        "empty shell" => {
            d.add_shell(ShellRecord::new(RegionId::OUTSIDE)).ok()?;
        }
        "bare region" => {
            d.add_region(dice.chance(1, 2)).ok()?;
        }
        "remove shell" => {
            let s = dice.pick(&shells(d))?;
            d.remove_shell(s)?;
        }
        "remove region" => {
            let regions: Vec<RegionId> = d.regions().map(scx_brep::Region::id).collect();
            let r = dice.pick(&regions[1..])?;
            d.remove_region(r)?;
        }
        "swap ends" => {
            let e = dice.pick(&edges(d))?;
            d.edge_mut(e)?.ends.reverse();
        }
        "merge vertices" => {
            let vs = vertices(d);
            let (a, b) = (dice.pick(&vs)?, dice.pick(&vs)?);
            d.merge_vertex(a, b)?;
        }
        "clear loops" => {
            let f = dice.pick(&faces(d))?;
            d.face_mut(f)?.loops.clear();
        }
        "solid outside" => {
            d.region_mut(RegionId::OUTSIDE)?.solid = true;
        }
        "move shell" => {
            let s = dice.pick(&shells(d))?;
            let regions: Vec<RegionId> = d.regions().map(scx_brep::Region::id).collect();
            let r = dice.pick(&regions)?;
            d.shell_mut(s)?.region = r;
        }
        "pending edge" => {
            let e = dice.pick(&edges(d))?;
            let ends = d.edge(e)?.ends();
            d.add_edge_pending(ends).ok()?;
        }
        "move wire" => {
            // Both ends to a new vertex: the wire comes apart from its shell.
            let w = d.shells().find_map(|s| s.record().wires.first().copied())?;
            let p = d.vertex(d.edge(w)?.ends()[0])?.point();
            let v = d.add_vertex(VertexRecord::new(p)).ok()?;
            d.edge_mut(w)?.ends = [v, v];
        }
        _ => unreachable!(),
    }
    Some(())
}

/// The finished bodies' boxes agree: the body's box, and the faces meeting each face's box
/// and a few boxes around vertices.
fn same_boxes(a: &Brep, b: &Brep, label: &str) {
    assert_eq!(a.bounding_box(), b.bounding_box(), "{label}: body box");
    let ids = |br: &Brep, q| {
        br.faces_meeting(q)
            .map(scx_brep::Face::id)
            .collect::<Vec<_>>()
    };
    for f in a.faces().step_by(3) {
        if let Some(q) = f.bounding_box() {
            assert_eq!(ids(a, q), ids(b, q), "{label}: faces meeting {:?}", f.id());
        }
    }
    for v in a.vertices().step_by(5) {
        if let Some(q) = scx_num::extent3d::Extent3d::from_point(v.point()) {
            assert_eq!(ids(a, q), ids(b, q), "{label}: faces meeting {:?}", v.id());
        }
    }
}

fn outcome(
    r: Result<(Brep, scx_brep::Changes), Box<scx_brep::Unfinished>>,
) -> Result<Brep, FinishCause> {
    r.map(|(b, _)| b).map_err(|u| u.cause)
}

#[test]
fn finishing_the_touched_records_agrees_with_checking_all() {
    let doc = Document::default();
    let mut tally: BTreeMap<String, usize> = BTreeMap::new();
    let mut ran = [0usize; OPS.len()];
    for kind in Kind::ALL {
        for seed in 1..=32u64 {
            let mut dice = Dice(Rng::new(seed * 7919 + kind as u64));
            let size = 1 + dice.below(3);
            let g = generate(kind, seed, size).unwrap();
            let mut body = g.draft.finish(&doc).map_err(|u| u.cause).unwrap().0;
            for round in 0..4 {
                // The base's cells filled or not: the stars and boxes paths both run.
                if dice.chance(1, 2) {
                    let _ = body.bounding_box();
                }
                if dice.chance(1, 2)
                    && let Some(v) = body.vertices().next()
                {
                    let _ = v.edges().count();
                }
                let mut d = if dice.chance(1, 3) {
                    body.clone().into_draft()
                } else {
                    body.edit()
                };
                let n = 1 + dice.below(3);
                for _ in 0..n {
                    // Valid ops half the time, so rounds go on.
                    let op = if dice.chance(1, 2) {
                        dice.below(VALID)
                    } else {
                        dice.below(OPS.len())
                    };
                    if apply(&mut d, op, &mut dice).is_some() {
                        ran[op] += 1;
                    }
                }
                let label = format!("{} seed {seed} size {size} round {round}", kind.name());
                let touched = outcome(d.clone().finish(&doc));
                let all = outcome(d.finish_checking_all(&doc));
                match (touched, all) {
                    (Ok(a), Ok(b)) => {
                        *tally.entry("finished".into()).or_default() += 1;
                        same_boxes(&a, &b, &label);
                        let mut fresh = a.clone();
                        fresh.shed_cells();
                        same_boxes(&a, &fresh, &label);
                        body = a;
                    }
                    (Err(a), Err(b)) => {
                        assert_eq!(a, b, "{label}");
                        let name = format!("{a:?}");
                        let name = name
                            .split([' ', '(', '{'])
                            .take(2)
                            .collect::<Vec<_>>()
                            .join(" ");
                        *tally.entry(name).or_default() += 1;
                        break;
                    }
                    (a, b) => panic!("{label}: touched {:?}, all {:?}", a.err(), b.err()),
                }
            }
        }
    }
    eprintln!("outcomes: {tally:#?}");
    for (op, n) in OPS.iter().zip(ran) {
        assert!(n > 0, "op {op} never ran");
    }
    // The run reaches many causes, not one.
    assert!(tally.len() >= 12, "{tally:?}");
}
