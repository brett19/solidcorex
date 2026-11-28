// Written for solidcorex. Apache-2.0.
//! Stage 6, second half: select and build (BO4, 72 §5.4 steps 6 and 7, §14.29).
//!
//! - **Select.** Each piece is kept or dropped by its class and the operation; `b`'s pieces
//!   kept by a subtraction are turned over (their sides swapped).
//! - **Merge.** Two kept faces meeting at an edge only they bound are one face where they
//!   are pieces of one operand face (a cut the selection left with one face each side, or
//!   a seam the trace crossed), or lie on one surface with their material on one side
//!   (coplanar faces of the two operands). The edges inside a merged face go: its loops
//!   are walked across them. A merged face of one operand face keeps its pcurves; one of
//!   two surfaces has its loops written exact on the survivor's.
//! - **Vertices.** An edge no kept face bounds goes, and a vertex no edge ends at; a
//!   vertex left between two edges on one curve, bounding the same faces, goes too: the
//!   edges are one.
//! - **Shells.** Each set of kept faces connected across edges two faces bound bounds one
//!   solid region, with a shell on each side: the shells of its lowest face of `a` where it
//!   has one, else new ones. An edge more than two bound (the result touching itself or
//!   another lump along a line) has its fins put in radial order, and the shells meeting
//!   across each of its sectors are one ([`pinch`]). With more than one set, each shell
//!   then goes to its region by its volume's sign and the shells holding it: a cavity's
//!   wall to the lump round it, a lump inside a cavity to that cavity's void.

use std::collections::{BTreeMap, BTreeSet};

use scx_brep::{
    Brep, Changes, Document, EdgeId, EdgeShape, Entity, FaceId, FaceSide, LoopRecord, RegionId,
    ShellId, ShellRecord, UseGeom, VertexId,
};
use scx_curve::Curve;
use scx_mass::Sign;
use scx_num::budget::{Budget, Stop};
use scx_num::extent1d::Extent1d;
use scx_num::frame::Sense;
use scx_num::units::Accuracy;
use scx_num::vector2d::Point2;
use scx_nurbs::Approach;
use scx_ssi::analytic::{self, Intersection};
use scx_surface::Closed;

use crate::boolean::BooleanOp;
use crate::classify::{Class, Classes, Joins, material_back};
use crate::error::{BooleanDefect, BooleanError, BooleanUnsupported};
use crate::faceface::Touch;
use crate::geom::{Operands, Side, recorded};
use crate::imprint::{Imprint, loops_error, ring, shoelace};
use crate::pinch;

/// Two uses' middles closer than this in uv are one place: a cut, not a seam.
const SAME_UV: f64 = 1e-7;

/// Faces whose normals at a shared edge are further apart than 60 degrees (or nearer than
/// 120) are not on one surface, which the merge then need not test.
const PARALLEL: f64 = 0.5;

/// A kept face: its operand face, and whether its material is behind it.
#[derive(Debug, Clone, Copy)]
struct Kept {
    side: Side,
    of: FaceId,
    back: bool,
}

/// Whether the operation keeps a piece of `side` of class `c`, and if so whether it turns
/// it over.
const fn select(op: BooleanOp, side: Side, c: Class) -> Option<bool> {
    match (op, side, c) {
        (BooleanOp::Union, Side::A, Class::Out | Class::On { same: true })
        | (BooleanOp::Union, Side::B, Class::Out)
        | (BooleanOp::Intersect, Side::A, Class::In | Class::On { same: true })
        | (BooleanOp::Intersect, Side::B, Class::In)
        | (BooleanOp::Subtract, Side::A, Class::Out | Class::On { same: false }) => Some(false),
        (BooleanOp::Subtract, Side::B, Class::In) => Some(true),
        _ => None,
    }
}

/// Selects, merges and finishes `m`'s draft for `op`.
pub(crate) fn build(
    ops: Operands<'_>,
    mut m: Imprint,
    touches: &[Touch],
    cls: &Classes,
    op: BooleanOp,
    doc: Document,
    budget: &mut Budget,
    stop: &Stop,
) -> Result<(Brep, Changes), BooleanError> {
    let mut kept: BTreeMap<FaceId, Kept> = BTreeMap::new();
    for (&f, &(side, of)) in &cls.owner {
        let c = cls.class.get(&f).copied().ok_or_else(defect)?;
        match select(op, side, c) {
            Some(flip) => {
                let back = material_back(ops.of(side), of)? != flip;
                kept.insert(f, Kept { side, of, back });
            }
            None => {
                m.draft.remove_face(f);
            }
        }
    }
    // A touch inside a face (stage 4 records no other) where both faces keep a piece:
    // the result meets itself at that point.
    for t in touches {
        let both = [Side::A, Side::B]
            .into_iter()
            .zip(t.faces)
            .all(|(side, of)| kept.values().any(|k| (k.side, k.of) == (side, of)));
        if both {
            return Err(BooleanError::Unsupported(BooleanUnsupported::NonManifold {
                at: Entity::Face(t.faces[0]),
            }));
        }
    }
    stop.check()?;
    merge(ops, &mut m, &mut kept, doc, budget, stop)?;
    edges(&mut m)?;
    join_edges(ops, &mut m, doc)?;
    vertices(&mut m);
    stop.check()?;
    shells(ops, &mut m, &kept, doc, budget, stop)?;
    let (body, changes) = m.draft.finish(&doc).map_err(|_| defect())?;
    // A result that cannot be enclosed (a face or an edge with no box) is one its own
    // queries (classification, mass, the contract) cannot answer for: not a result.
    if body.bounding_box().is_none() && body.faces().next().is_some() {
        return Err(defect());
    }
    Ok((body, changes))
}

const fn defect() -> BooleanError {
    BooleanError::Defect(BooleanDefect::Build)
}

/// Merges the kept faces that are one face.
fn merge(
    ops: Operands<'_>,
    m: &mut Imprint,
    kept: &mut BTreeMap<FaceId, Kept>,
    doc: Document,
    budget: &mut Budget,
    stop: &Stop,
) -> Result<(), BooleanError> {
    let mut set = Joins::new(kept.keys().copied());
    let mut same_surface: BTreeMap<(Side, FaceId, Side, FaceId), bool> = BTreeMap::new();
    let edges: Vec<EdgeId> = m.draft.edges().map(scx_brep::Edge::id).collect();
    for e in edges {
        let Some(view) = m.draft.edge(e) else {
            continue;
        };
        let fins: Vec<_> = view.fins().collect();
        let (Some(f0), Some(f1), 2) = (fins.first(), fins.get(1), fins.len()) else {
            continue;
        };
        if f0.face == f1.face {
            continue;
        }
        let (Some(&k0), Some(&k1)) = (kept.get(&f0.face), kept.get(&f1.face)) else {
            continue;
        };
        // Faces turned against each other (one material behind, the other's in front)
        // run their loops opposite ways about one normal, so they use the edge in one
        // sense; faces turned alike use it in opposite senses.
        let turned = k0.back != k1.back;
        if (f0.sense == f1.sense) != turned {
            continue;
        }
        if (k0.side, k0.of) == (k1.side, k1.of) {
            set.join(f0.face, f1.face);
            continue;
        }
        let (Some(v0), Some(v1)) = (m.draft.face(f0.face), m.draft.face(f1.face)) else {
            continue;
        };
        let (s0, s1) = (v0.surface(), v1.surface());
        if core::mem::discriminant(s0) != core::mem::discriminant(s1) {
            continue;
        }
        if s0 == s1 {
            // One surface, so one natural normal: turned faces have material either side.
            if turned {
                continue;
            }
        } else {
            // On one surface, the material lies on one side where the natural normals
            // agree and the faces are turned alike, or they are opposite and the faces
            // turned against each other (a subtraction's turned piece beside a face of
            // `a`, §14.35). Coincident surfaces have parallel normals where they meet, so
            // the normals, read first, also spare the surface test for faces at an angle.
            let n = |f: FaceId| -> Option<scx_num::vector3d::UnitVec3> {
                let face = m.draft.face(f)?;
                let u = face.uses().find(|u| u.record().edge == e)?;
                let uv = use_mid(u.geom())?;
                face.surface()
                    .normal(uv, [Approach::Above, Approach::Above])
            };
            let (Some(n0), Some(n1)) = (n(f0.face), n(f1.face)) else {
                continue;
            };
            let dot = n0.as_vec().dot(n1.as_vec());
            if dot.abs() < PARALLEL || (dot < 0.0) != turned {
                continue;
            }
            let key = (k0.side, k0.of, k1.side, k1.of);
            let on = if let Some(&on) = same_surface.get(&key) {
                on
            } else {
                stop.check()?;
                budget.charge(1)?;
                let on = coincide(ops, k0, k1);
                same_surface.insert(key, on);
                on
            };
            if !on {
                continue;
            }
        }
        set.join(f0.face, f1.face);
    }
    let mut groups: BTreeMap<FaceId, Vec<FaceId>> = BTreeMap::new();
    for &f in kept.keys() {
        groups.entry(set.root(f)).or_default().push(f);
    }
    for faces in groups.into_values().filter(|g| g.len() > 1) {
        stop.check()?;
        let survivor = faces
            .iter()
            .copied()
            .find(|f| kept.get(f).is_some_and(|k| k.side == Side::A))
            .or_else(|| faces.first().copied())
            .ok_or_else(defect)?;
        let loops = splice(m, &faces, survivor, kept, doc)?;
        for &f in &faces {
            if f != survivor {
                m.draft.merge_face(f, survivor).ok_or_else(defect)?;
                kept.remove(&f);
            }
        }
        m.draft
            .edit_face(survivor, |r| r.loops = loops)
            .ok_or_else(defect)?;
    }
    Ok(())
}

/// Whether the surfaces of two kept faces' operand faces coincide over their uv boxes.
fn coincide(ops: Operands<'_>, k0: Kept, k1: Kept) -> bool {
    let (Some(f0), Some(f1)) = (ops.of(k0.side).face(k0.of), ops.of(k1.side).face(k1.of)) else {
        return false;
    };
    let (Some(u0), Some(u1)) = (f0.uv_box(), f1.uv_box()) else {
        return false;
    };
    let acc = [
        f0.accuracy().unwrap_or(Accuracy::ZERO),
        f1.accuracy().unwrap_or(Accuracy::ZERO),
    ];
    matches!(
        analytic::intersect(
            f0.surface(),
            &u0,
            f1.surface(),
            &u1,
            acc,
            ops.a.resolution()
        ),
        Closed::Solved(Intersection::Coincident(_))
    )
}

/// The middle of a use's pcurve.
fn use_mid(g: &UseGeom) -> Option<Point2> {
    match g {
        UseGeom::Pcurve { curve, interval } => Some(curve.point(interval.mid())),
        UseGeom::Point(_) => None,
    }
}

/// A use, by (face, loop, position).
type At = (FaceId, usize, usize);

/// The loops of the face the group `faces` makes, walked across the edges inside it.
fn splice(
    m: &Imprint,
    faces: &[FaceId],
    survivor: FaceId,
    kept: &BTreeMap<FaceId, Kept>,
    doc: Document,
) -> Result<Vec<LoopRecord>, BooleanError> {
    let members: BTreeSet<FaceId> = faces.iter().copied().collect();
    let one_face = {
        let mut owners = faces
            .iter()
            .filter_map(|f| kept.get(f).map(|k| (k.side, k.of)));
        let first = owners.next();
        owners.all(|o| Some(o) == first)
    };
    let mut lens: BTreeMap<(FaceId, usize), usize> = BTreeMap::new();
    let mut by_edge: BTreeMap<EdgeId, Vec<At>> = BTreeMap::new();
    for &f in faces {
        let view = m.draft.face(f).ok_or_else(defect)?;
        for (l, lp) in view.record().loops.iter().enumerate() {
            lens.insert((f, l), lp.uses.len());
            for (p, u) in lp.uses.iter().enumerate() {
                by_edge.entry(u.edge).or_default().push((f, l, p));
            }
        }
    }
    let record = |at: At| {
        m.draft
            .face(at.0)
            .and_then(|v| v.record().loops.get(at.1)?.uses.get(at.2).cloned())
    };
    // The edges inside: both uses in the group, on two faces, at one place.
    let mut twin: BTreeMap<At, At> = BTreeMap::new();
    // A twin a period apart: the pieces meet across the face's seam, so the
    // records cannot be kept and the loops are made again from the edges.
    let mut shifted = false;
    // The uses of collapsed edges among the twins.
    let mut poles: BTreeSet<At> = BTreeSet::new();
    for (&e, at) in &by_edge {
        let (Some(&p), Some(&q), 2) = (at.first(), at.get(1), at.len()) else {
            continue;
        };
        if p.0 == q.0 {
            continue;
        }
        let fins_inside = m
            .draft
            .edge(e)
            .is_some_and(|v| v.fins().all(|f| members.contains(&f.face)));
        if !fins_inside {
            continue;
        }
        // A collapsed edge (a sphere's pole) inside: the two pieces' uses are parts of
        // the pole's line in uv, which the merged face keeps as one use, so its loops are
        // made again with the edge once where the walk crosses it.
        if m.draft
            .edge(e)
            .is_some_and(|v| matches!(v.shape(), Some(EdgeShape::Collapsed)))
        {
            poles.insert(p);
            poles.insert(q);
            shifted = true;
        } else if one_face {
            let (Some(rp), Some(rq)) = (record(p), record(q)) else {
                return Err(defect());
            };
            let (Some(mp), Some(mq)) = (use_mid(&rp.geom), use_mid(&rq.geom)) else {
                continue;
            };
            if mp.distance(mq) > SAME_UV {
                shifted = true;
            }
        }
        twin.insert(p, q);
        twin.insert(q, p);
    }
    // A face turned against the survivor (§14.35) runs its loops the other way about the
    // survivor's normal: it is walked backwards, each use against its own sense.
    let back = |f: FaceId| kept.get(&f).map(|k| k.back);
    let turned: BTreeSet<FaceId> = faces
        .iter()
        .copied()
        .filter(|&f| back(f) != back(survivor))
        .collect();
    let next = |at: At| -> At {
        let n = lens.get(&(at.0, at.1)).copied().unwrap_or(1);
        let step = if turned.contains(&at.0) { n - 1 } else { 1 };
        (at.0, at.1, (at.2 + step) % n)
    };
    let total: usize = lens.values().sum();
    let mut seen: BTreeSet<At> = BTreeSet::new();
    let mut cycles: Vec<Vec<At>> = Vec::new();
    for (&(f, l), &n) in &lens {
        for p in 0..n {
            let start = (f, l, p);
            if twin.contains_key(&start) || seen.contains(&start) {
                continue;
            }
            let mut cycle = Vec::new();
            let mut cur = start;
            loop {
                if !seen.insert(cur) {
                    return Err(defect());
                }
                cycle.push(cur);
                let mut nx = next(cur);
                let mut steps = 0;
                while let Some(&t) = twin.get(&nx) {
                    if poles.contains(&nx) {
                        cycle.push(nx);
                    }
                    nx = next(t);
                    steps += 1;
                    if steps > total {
                        return Err(defect());
                    }
                }
                if nx == start {
                    break;
                }
                cur = nx;
            }
            cycles.push(cycle);
        }
    }
    if one_face && !shifted {
        let mut loops = cycles
            .into_iter()
            .map(|c| {
                c.into_iter()
                    .map(|at| record(at).ok_or_else(defect))
                    .collect::<Result<Vec<_>, _>>()
                    .map(LoopRecord::new)
            })
            .collect::<Result<Vec<_>, _>>()?;
        let area = |lp: &LoopRecord| shoelace(&ring(lp));
        loops.sort_by(|p, q| area(q).total_cmp(&area(p)));
        return Ok(loops);
    }
    let seqs: Vec<Vec<(EdgeId, Sense)>> = cycles
        .into_iter()
        .map(|c| {
            c.into_iter()
                .map(|at| {
                    let u = record(at).ok_or_else(defect)?;
                    let sense = if turned.contains(&at.0) {
                        u.sense.reversed()
                    } else {
                        u.sense
                    };
                    Ok::<_, BooleanError>((u.edge, sense))
                })
                .collect::<Result<Vec<_>, _>>()
        })
        .collect::<Result<_, _>>()?;
    let view = m.draft.face(survivor).ok_or_else(defect)?;
    let side = kept.get(&survivor).map_or(Side::A, |k| k.side);
    m.draft
        .exact_loops(view.surface(), &seqs, &doc)
        .map_err(|e| loops_error(e.kind(), side.operand(), survivor))
}

/// Removes the edges no face bounds and refuses an edge one face bounds; puts the fins of
/// an edge more than two bound in radial order ([`pinch::order_fins`]).
fn edges(m: &mut Imprint) -> Result<(), BooleanError> {
    let mut gone = Vec::new();
    let mut pinched = Vec::new();
    for e in m.draft.edges() {
        let fins: Vec<_> = e.fins().collect();
        let collapsed = matches!(e.shape(), Some(EdgeShape::Collapsed));
        match fins.as_slice() {
            [] => gone.push(e.id()),
            [_] if collapsed => {}
            [_, _] => {}
            [_] => return Err(defect()),
            _ => pinched.push(e.id()),
        }
    }
    for e in gone {
        m.draft.remove_edge(e);
    }
    for e in pinched {
        pinch::order_fins(&mut m.draft, e)?;
    }
    Ok(())
}

/// Removes the vertices no edge ends at, and no shell holds.
fn vertices(m: &mut Imprint) {
    let mut used: BTreeSet<VertexId> = BTreeSet::new();
    for e in m.draft.edges() {
        used.extend(e.ends());
    }
    for s in m.draft.shells() {
        used.extend(s.record().lone_vertices.iter().copied());
    }
    let gone: Vec<VertexId> = m
        .draft
        .vertices()
        .map(scx_brep::Vertex::id)
        .filter(|v| !used.contains(v))
        .collect();
    for v in gone {
        m.draft.remove_vertex(v);
    }
}

/// Joins two edges that meet at a vertex no other edge ends at, run on along one curve and
/// bound the same faces the same way: the vertex goes.
fn join_edges(ops: Operands<'_>, m: &mut Imprint, doc: Document) -> Result<(), BooleanError> {
    loop {
        let mut ends: BTreeMap<VertexId, Vec<EdgeId>> = BTreeMap::new();
        for e in m.draft.edges() {
            for v in e.ends() {
                ends.entry(v).or_default().push(e.id());
            }
        }
        let mut joined = false;
        for (v, es) in ends {
            let (Some(&e0), Some(&e1), 2) = (es.first(), es.get(1), es.len()) else {
                continue;
            };
            if e0 == e1 {
                continue;
            }
            if let Some(plan) = join_plan(ops, m, v, e0, e1) {
                apply_join(m, v, &plan, doc)?;
                joined = true;
                break;
            }
        }
        if !joined {
            return Ok(());
        }
    }
}

/// A closed curve's period.
const fn period(c: &Curve) -> Option<f64> {
    match c {
        Curve::Circle(_) | Curve::Ellipse(_) => Some(core::f64::consts::TAU),
        _ => None,
    }
}

/// How two edges become one: `keep` absorbs `gone`, over `interval` from `ends[0]` to
/// `ends[1]`.
struct Join {
    keep: EdgeId,
    gone: EdgeId,
    curve: Curve,
    interval: Extent1d,
    ends: [VertexId; 2],
    faces: Vec<FaceId>,
}

fn join_plan(ops: Operands<'_>, m: &Imprint, v: VertexId, e0: EdgeId, e1: EdgeId) -> Option<Join> {
    let (g0, g1) = (m.draft.edge(e0)?, m.draft.edge(e1)?);
    // `first` runs into `v`, `second` out of it.
    let (first, second) = if g0.ends()[1] == v && g1.ends()[0] == v {
        (g0, g1)
    } else if g1.ends()[1] == v && g0.ends()[0] == v {
        (g1, g0)
    } else {
        return None;
    };
    if first.ends()[0] == v || second.ends()[1] == v {
        return None;
    }
    let key = |f: scx_brep::Fin| (f.face, f.sense == Sense::Forward);
    let mut fa: Vec<_> = first.fins().map(key).collect();
    let mut fb: Vec<_> = second.fins().map(key).collect();
    fa.sort();
    fb.sort();
    if fa != fb || fa.is_empty() {
        return None;
    }
    let (
        Some(EdgeShape::Curve {
            curve: c0,
            interval: i0,
        }),
        Some(EdgeShape::Curve {
            curve: c1,
            interval: i1,
        }),
    ) = (first.shape(), second.shape())
    else {
        return None;
    };
    let keep = first.id().min(second.id());
    let gone = first.id().max(second.id());
    let (curve, interval) = if c0 == c1 {
        // Run on: the second starts where the first ends, up to a period.
        let shift = i0.max() - i1.min();
        let period = period(c0);
        let fits = shift.abs() <= 1e-12 * (1.0 + i0.max().abs())
            || period.is_some_and(|p| {
                let k = (shift / p).round();
                k != 0.0 && (shift - k * p).abs() <= 1e-12 * (1.0 + p)
            });
        if !fits {
            return None;
        }
        let end = i1.max() + shift;
        if period.is_some_and(|p| end - i0.min() > p * (1.0 + 1e-12)) {
            return None;
        }
        (c0.clone(), Extent1d::new(i0.min(), end)?)
    } else if let (Curve::Line(l0), Curve::Line(l1)) = (c0, c1) {
        // One line within the edges' coincidence: every end of both pieces lies that near
        // the kept line, so the absorbed piece runs along it nowhere farther (a near-coplanar
        // front merged across its jog, §14.42). Both edges must record the same accuracy,
        // so the joined edge claims what each did.
        if first.accuracy() != second.accuracy() {
            return None;
        }
        let d0 = l0.direction();
        let d1 = l1.direction();
        if d0.dot(d1) <= 0.0 {
            return None;
        }
        let line = if keep == first.id() { l0 } else { l1 };
        let reach = ops.coincidence(recorded(first.accuracy()), recorded(second.accuracy()));
        let ends = [
            l0.point(i0.min()),
            l0.point(i0.max()),
            l1.point(i1.min()),
            l1.point(i1.max()),
        ];
        let off = |p: scx_num::vector3d::Point3| {
            (p - line.origin()).cross(line.direction()).length() / line.direction().length()
        };
        if ends.into_iter().any(|p| off(p) > reach) {
            return None;
        }
        // On the kept line: its parameter at the far end of the other.
        let t = |p: scx_num::vector3d::Point3| {
            (p - line.origin()).dot(line.direction()) / line.direction().length_squared()
        };
        let (s, e) = (t(l0.point(i0.min())), t(l1.point(i1.max())));
        (Curve::Line(*line), Extent1d::new(s, e)?)
    } else {
        return None;
    };
    Some(Join {
        keep,
        gone,
        curve,
        interval,
        ends: [first.ends()[0], second.ends()[1]],
        faces: fa.into_iter().map(|f| f.0).collect(),
    })
}

fn apply_join(m: &mut Imprint, v: VertexId, j: &Join, doc: Document) -> Result<(), BooleanError> {
    m.draft.merge_edge(j.gone, j.keep).ok_or_else(defect)?;
    {
        let g = m.draft.edge_mut(j.keep).ok_or_else(defect)?;
        g.shape = EdgeShape::Curve {
            curve: j.curve.clone(),
            interval: j.interval,
        };
        g.ends = j.ends;
    }
    let mut faces = j.faces.clone();
    faces.dedup();
    for f in faces {
        m.draft
            .edit_face(f, |r| {
                for lp in &mut r.loops {
                    lp.uses.retain(|u| u.edge != j.gone);
                }
            })
            .ok_or_else(defect)?;
        m.draft
            .write_pcurves(f, &doc)
            .map_err(|e| loops_error(e.kind(), Side::A.operand(), f))?;
    }
    m.draft.remove_vertex(v);
    Ok(())
}

/// Gives each connected set of kept faces its region and shells.
fn shells(
    ops: Operands<'_>,
    m: &mut Imprint,
    kept: &BTreeMap<FaceId, Kept>,
    doc: Document,
    budget: &mut Budget,
    stop: &Stop,
) -> Result<(), BooleanError> {
    let live: Vec<FaceId> = kept
        .keys()
        .copied()
        .filter(|f| m.draft.face(*f).is_some())
        .collect();
    let mut set = Joins::new(live.iter().copied());
    for e in m.draft.edges() {
        let fins: Vec<_> = e.fins().collect();
        if let [f, g] = fins.as_slice() {
            set.join(f.face, g.face);
        }
    }
    let mut comps: BTreeMap<FaceId, Vec<FaceId>> = BTreeMap::new();
    for &f in &live {
        comps.entry(set.root(f)).or_default().push(f);
    }
    let mut claimed: BTreeSet<ShellId> = BTreeSet::new();
    // Each set's material shell and void shell.
    let mut pairs: Vec<(ShellId, ShellId)> = Vec::new();
    for faces in comps.values() {
        let reuse = faces
            .iter()
            .find(|f| kept.get(f).is_some_and(|k| k.side == Side::A))
            .and_then(|&f| {
                let k = kept.get(&f)?;
                let sides = m.draft.face(f)?.record().sides;
                let (mat, void) = if k.back {
                    (sides[1], sides[0])
                } else {
                    (sides[0], sides[1])
                };
                let solid = |s: ShellId| {
                    m.draft
                        .shell(s)
                        .and_then(scx_brep::Shell::region)
                        .map(scx_brep::Region::is_solid)
                };
                let ok = mat != void
                    && !claimed.contains(&mat)
                    && !claimed.contains(&void)
                    && solid(mat) == Some(true)
                    && solid(void) == Some(false);
                ok.then_some((mat, void))
            });
        let (mat, void) = if let Some(p) = reuse {
            p
        } else {
            let r = m.draft.add_region(true).map_err(|_| defect())?;
            let mat = m
                .draft
                .add_shell(ShellRecord::new(r))
                .map_err(|_| defect())?;
            let void = m
                .draft
                .add_shell(ShellRecord::new(RegionId::OUTSIDE))
                .map_err(|_| defect())?;
            (mat, void)
        };
        claimed.insert(mat);
        claimed.insert(void);
        pairs.push((mat, void));
        for &f in faces {
            let back = kept.get(&f).is_some_and(|k| k.back);
            let want = if back { [void, mat] } else { [mat, void] };
            if m.draft.face(f).is_some_and(|v| v.record().sides != want) {
                m.draft.face_mut(f).ok_or_else(defect)?.sides = want;
            }
        }
    }
    let moved = pinch::link(&mut m.draft, &mut claimed)?;
    unused(m, &claimed);
    if pairs.len() > 1 {
        let to = |s: ShellId| moved.get(&s).copied().unwrap_or(s);
        let mirrored: Vec<(ShellId, ShellId)> = pairs
            .iter()
            .copied()
            .filter(|(mat, void)| !moved.contains_key(mat) && !moved.contains_key(void))
            .filter(|(mat, void)| !moved.values().any(|r| r == mat || r == void))
            .collect();
        let mut all: Vec<ShellId> = pairs.iter().flat_map(|&(a, b)| [to(a), to(b)]).collect();
        all.sort_unstable();
        all.dedup();
        // `pairs` is in `comps`' order: each set beside its shells.
        let whole: Vec<Option<Sign>> = pairs
            .iter()
            .zip(comps.values())
            .map(|(_, faces)| whole_shell(ops, m, kept, faces))
            .collect();
        let mut known: BTreeMap<ShellId, Sign> = BTreeMap::new();
        // A shell is made of the sets of the pairs it holds (one when no link joined it).
        // Signed volume adds over closed sets, so where every set is an operand shell whole
        // and all are of one sign, the shell has it (a void shell the opposite). Anything
        // else is measured.
        let mut joined: BTreeMap<ShellId, Vec<Option<Sign>>> = BTreeMap::new();
        let mut voids: BTreeMap<ShellId, Vec<Option<Sign>>> = BTreeMap::new();
        for (&(mat, void), &s) in pairs.iter().zip(&whole) {
            joined.entry(to(mat)).or_default().push(s);
            voids.entry(to(void)).or_default().push(s);
        }
        let all_of = |v: &[Option<Sign>]| match v.first().copied().flatten() {
            Some(first) if v.iter().all(|&s| s == Some(first)) => Some(first),
            _ => None,
        };
        for (root, v) in &joined {
            if let Some(s) = all_of(v) {
                known.insert(*root, s);
            }
        }
        for (root, v) in &voids {
            if let Some(s) = all_of(v) {
                known.insert(*root, flip(s));
            }
        }
        pinch::nest(&mut m.draft, &all, &mirrored, &known, doc, budget, stop)?;
        unused(m, &claimed);
    }
    Ok(())
}

/// The sign of the volume a set of kept faces bounds from its material side, where the set
/// is one operand shell whole: each face with all the pieces of its operand face, all of one
/// operand, all the shell's faces, each once (no sheet), its material on one side of that
/// shell. Its surface is the operand shell's, so its sign is the operand's: a shell of
/// the outside is negative, the one shell of a region positive (a valid operand's, as
/// classification already trusts). `None` where that does not settle it (a region with a
/// cavity), and nesting measures the sign (§14.42).
fn whole_shell(
    ops: Operands<'_>,
    m: &Imprint,
    kept: &BTreeMap<FaceId, Kept>,
    faces: &[FaceId],
) -> Option<Sign> {
    let first = kept.get(faces.first()?)?;
    let body = ops.of(first.side);
    // The operand side the draft's material is on: the back where the face is kept with
    // its material behind it.
    let at = |k: &Kept| {
        if k.back {
            FaceSide::Back
        } else {
            FaceSide::Front
        }
    };
    let shell = body.face(first.of)?.shell(at(first));
    let set: BTreeSet<FaceId> = faces.iter().copied().collect();
    let mut ofs = BTreeSet::new();
    let mut pieces = 0;
    for f in faces {
        let k = kept.get(f)?;
        // Every piece of its operand face is in the set (a face cut by a contact edge still
        // covers its surface).
        let all = m.pieces.get(&(k.side, k.of))?;
        if k.side != first.side
            || !all.iter().all(|p| set.contains(p))
            || body.face(k.of)?.shell(at(k)).id() != shell.id()
        {
            return None;
        }
        if ofs.insert(k.of) {
            pieces += all.len();
        }
    }
    let mut sides = 0;
    for (f, _) in shell.faces() {
        sides += 1;
        if !ofs.contains(&f.id()) {
            return None;
        }
    }
    if sides != ofs.len() || pieces != faces.len() {
        return None;
    }
    let region = shell.region();
    if region.is_outside() {
        Some(Sign::Negative)
    } else if region.shells().count() == 1 {
        Some(Sign::Positive)
    } else {
        None
    }
}

/// The opposite sign.
const fn flip(s: Sign) -> Sign {
    match s {
        Sign::Positive => Sign::Negative,
        Sign::Negative => Sign::Positive,
        Sign::Zero => Sign::Zero,
    }
}

/// Removes the shells no face bounds, and the regions no shell bounds.
fn unused(m: &mut Imprint, claimed: &BTreeSet<ShellId>) {
    let gone: Vec<ShellId> = m
        .draft
        .shells()
        .filter(|s| {
            !claimed.contains(&s.id())
                && s.record().wires.is_empty()
                && s.record().lone_vertices.is_empty()
        })
        .map(scx_brep::Shell::id)
        .collect();
    for s in gone {
        m.draft.remove_shell(s);
    }
    let used: BTreeSet<RegionId> = m.draft.shells().map(|s| s.record().region).collect();
    let gone: Vec<RegionId> = m
        .draft
        .regions()
        .map(scx_brep::Region::id)
        .filter(|r| *r != RegionId::OUTSIDE && !used.contains(r))
        .collect();
    for r in gone {
        m.draft.remove_region(r);
    }
}
