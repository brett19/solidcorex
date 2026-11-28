// Written for solidcorex. Apache-2.0.
//! Clean-up after an operation (research 75 §6, BR11): faces on one surface that meet along
//! an edge become one face, and two edges on one curve that meet at a vertex nothing else
//! uses become one edge. SMLib does both (`Brep::merge_faces`,
//! `RemoveTopologicalEdgesAndVertices`) at the end of its Booleans; here they are [`Draft`]
//! methods, called by the Boolean's build stage, by repair and by the local operations.
//!
//! **What is one surface, one curve.** The surfaces' and curves' own `==` (the test the
//! cells and `Changes` already use): the same kind, the same frame, the same numbers. The
//! faces of one operand face cut apart, and the pieces of one cut edge, are equal in this
//! sense; two coplanar faces of different operands are not, and the Boolean's own merge
//! (which tests them with the surface intersection) keeps those.
//!
//! **Protected edges.** A caller's set of edges that mark a split made on purpose. A
//! protected edge is never removed by a face merge, and two faces that share one are never
//! merged. Two edges are joined only when both are protected or neither is; the survivor is
//! the lower id, and the report names the absorbed one so the caller can drop it from its
//! set. An id in the set that is not a live edge is [`Absent`], before anything is written.
//!
//! **Face merge** ([`Draft::merge_faces_across`], [`Draft::merge_faces`]). The faces
//! `a < b` on both sides of an edge with exactly two fins are merged when their surfaces
//! are equal, their sides are the same shells, and every edge only they bound is used in
//! opposite senses. Every such edge goes (the pair's *twins*), and the loops are walked
//! across them; `b` is merged into `a`. A twin whose two uses lie a period apart in uv (the
//! faces meet across a seam of the surface) stays as a seam when another twin is in step.
//! A pair is merged whole or not at all: it is left (and listed, [`Why`]) when a twin is
//! protected, when the walk leaves no loop or two outer ones, or when exact pcurves cannot
//! be written for a pair that meets across a seam only.
//!
//! **Edge join** ([`Draft::join_edges_at`], [`Draft::join_edges`]). Two edges that meet at a
//! vertex nothing else ends at, with equal curves whose parameters run on from one to the
//! other, bound by the same faces the same way, become one: the lower id keeps the curve
//! over the joined interval and the vertex goes. Each face's two uses become one with the
//! joined pcurve, when the two uses' pcurves are equal and run on likewise; where they do
//! not the join is left, so no pcurve is ever written at a guess. The joined edge's accuracy
//! is the larger of the two, kept as a bound no longer proved.
//!
//! **Atomic.** Each merge or join is planned from the draft as it stands, then written; a
//! refusal writes nothing. The batch methods report each merge made and each near miss, in
//! id order, and are deterministic.

use core::fmt;
use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::sync::Arc;

use scx_curve::{Curve, Curve2};
use scx_num::cert::{Basis, Recorded};
use scx_num::extent1d::Extent1d;
use scx_num::frame::Sense;
use scx_num::vector2d::{Point2, Vec2};

use super::Draft;
use crate::document::Document;
use crate::error::Absent;
use crate::id::{EdgeId, Entity, FaceId, VertexId};
use crate::pcurve::{ExactLoop, PcurveWriteError, placed};
use crate::record::{EdgeShape, EdgeUse, Fin, LoopRecord, UseGeom};
use crate::tables::Tables;

/// Two uses of one edge whose middles are this close in uv lie in one window of the
/// surface; further apart (a whole period) they meet across its seam.
const SAME_UV: f64 = 1e-7;

/// How far the end of one edge's interval may be from the start of the next's, relative,
/// and still run on: a few units in the last place of the parameter.
const RUN_ON: f64 = 1e-12;

/// A face merge that was made.
#[derive(Debug, Clone, PartialEq)]
pub struct FaceMerge {
    /// The face merged into `survivor`: gone.
    pub absorbed: FaceId,
    /// The lower of the two ids: holds the loops of both.
    pub survivor: FaceId,
    /// The edges removed, in id order: the edges only the two faces bounded.
    pub removed: Vec<EdgeId>,
}

/// An edge join that was made.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EdgeJoin {
    /// The edge joined onto `survivor`: gone.
    pub absorbed: EdgeId,
    /// The lower of the two ids: runs over both.
    pub survivor: EdgeId,
    /// The vertex between them: gone.
    pub vertex: VertexId,
}

/// Why two faces on one surface, or two edges on one curve, were left as they are.
#[derive(Debug, Clone, Copy, PartialEq)]
#[non_exhaustive]
pub enum Why {
    /// An edge the faces share is protected, or the edges are protected one and not the
    /// other.
    Protected,
    /// The faces' sides are not the same shells.
    Sides,
    /// A shared edge is used in one sense by both faces: their loops run against each other.
    Orientation,
    /// Walking the loops across the shared edges gave no loop, or two outer loops.
    Loops,
    /// The faces meet across a seam only, and exact pcurves could not be written for the
    /// merged loops.
    Pcurves(PcurveWriteError),
    /// The edges' fins differ: not the same faces in the same senses.
    Fins,
    /// The edges' parameters do not run on from one to the other.
    Interval,
    /// A face's loops do not pass the vertex from one edge to the other, or two uses'
    /// pcurves are not equal and running on.
    Uses,
}

impl fmt::Display for Why {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Protected => f.write_str("a protected edge"),
            Self::Sides => f.write_str("the faces' sides are different shells"),
            Self::Orientation => f.write_str("a shared edge is used in one sense by both faces"),
            Self::Loops => f.write_str("the merged loops are not one outer loop and its holes"),
            Self::Pcurves(e) => write!(f, "exact pcurves for the merged loops: {e}"),
            Self::Fins => f.write_str("the edges' fins differ"),
            Self::Interval => f.write_str("the edges' parameters do not run on"),
            Self::Uses => f.write_str("the uses at the vertex are not one pcurve running on"),
        }
    }
}

/// Why a merge or join asked for by name was not made. Nothing is written.
#[derive(Debug, Clone, Copy, PartialEq)]
#[non_exhaustive]
pub enum Refusal {
    /// The draft holds no such edge or vertex, or a protected id is not a live edge.
    Absent(Absent),
    /// The entity is not a candidate: not an edge of two faces on one surface, or not a
    /// vertex of two edges on one curve.
    NotACandidate,
    /// The candidate was left, for this reason.
    Left(Why),
}

impl fmt::Display for Refusal {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Absent(a) => a.fmt(f),
            Self::NotACandidate => {
                f.write_str("not two faces on one surface or two edges on one curve")
            }
            Self::Left(w) => write!(f, "left: {w}"),
        }
    }
}

impl core::error::Error for Refusal {}

impl From<Absent> for Refusal {
    fn from(a: Absent) -> Self {
        Self::Absent(a)
    }
}

impl From<Skip> for Refusal {
    fn from(s: Skip) -> Self {
        match s {
            Skip::Silent => Self::NotACandidate,
            Skip::Left(w) => Self::Left(w),
        }
    }
}

/// A near miss: faces on one surface or edges on one curve that were left, and why.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Left {
    /// The edge between the faces, or the vertex between the edges.
    pub at: Entity,
    /// Why they were left.
    pub why: Why,
}

/// What a batch merge or join, or [`Draft::heal`], did.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Healed {
    /// The face merges made, in the order made.
    pub faces: Vec<FaceMerge>,
    /// The edge joins made, in the order made.
    pub edges: Vec<EdgeJoin>,
    /// The near misses at the end: candidates that stay as they are.
    pub left: Vec<Left>,
}

/// A candidate that is not merged: silently not a candidate, or left for a reason.
enum Skip {
    Silent,
    Left(Why),
}

/// The edges at each vertex, an edge twice at a vertex it starts and ends at.
struct Stars {
    at: Vec<Vec<EdgeId>>,
}

impl Stars {
    fn of(t: &Tables) -> Self {
        let mut at: Vec<Vec<EdgeId>> = vec![Vec::new(); t.vertices.slots()];
        for (id, geom) in t.edges.iter() {
            for v in geom.ends {
                if v.index() >= at.len() {
                    at.resize(v.index() + 1, Vec::new());
                }
                if let Some(star) = at.get_mut(v.index()) {
                    star.push(id);
                }
            }
        }
        Self { at }
    }

    fn star(&self, v: VertexId) -> &[EdgeId] {
        self.at.get(v.index()).map_or(&[], Vec::as_slice)
    }

    /// `edge`, which ran between `ends`, is gone.
    fn drop_edge(&mut self, edge: EdgeId, ends: [VertexId; 2]) {
        for v in ends {
            if let Some(star) = self.at.get_mut(v.index())
                && let Some(i) = star.iter().position(|&e| e == edge)
            {
                star.remove(i);
            }
        }
    }

    /// `gone` is `keep` at the vertices `ends`.
    fn rename(&mut self, gone: EdgeId, keep: EdgeId, ends: [VertexId; 2]) {
        for v in ends {
            if let Some(star) = self.at.get_mut(v.index()) {
                for e in star.iter_mut().filter(|e| **e == gone) {
                    *e = keep;
                }
            }
        }
    }
}

/// The root of `f`'s set in a union-find over faces, compressing the path.
fn find(parent: &mut BTreeMap<FaceId, FaceId>, f: FaceId) -> FaceId {
    let mut r = f;
    while let Some(&p) = parent.get(&r) {
        if p == r {
            break;
        }
        r = p;
    }
    parent.insert(f, r);
    r
}

/// A use, by face, loop and position.
type At = (FaceId, usize, usize);

/// A planned face merge.
struct FacePlan {
    survivor: FaceId,
    absorbed: FaceId,
    twins: Vec<EdgeId>,
    loops: Vec<LoopRecord>,
    /// The fins of each edge the absorbed face keeps, with it replaced by the survivor in
    /// place, so the radial order is as it was.
    fixes: Vec<(EdgeId, Vec<Fin>)>,
}

/// A planned merge of a group of faces into the lowest.
struct GroupPlan {
    survivor: FaceId,
    absorbed: Vec<FaceId>,
    twins: Vec<EdgeId>,
    loops: Vec<LoopRecord>,
    fixes: Vec<(EdgeId, Vec<Fin>)>,
}

/// A planned edge join.
struct JoinPlan {
    keep: EdgeId,
    gone: EdgeId,
    vertex: VertexId,
    curve: Curve,
    interval: Extent1d,
    ends: [VertexId; 2],
    gone_ends: [VertexId; 2],
    accuracy: Option<Arc<Recorded>>,
    faces: Vec<(FaceId, Vec<LoopRecord>)>,
}

/// What two edges meeting at a free vertex need to be one.
struct Link<'a> {
    first: EdgeId,
    second: EdgeId,
    curve: &'a Curve,
    interval: Extent1d,
    ends: [VertexId; 2],
    accuracy: Option<Arc<Recorded>>,
    faces: Vec<FaceId>,
}

/// A maximal run of edges on one curve through free vertices: `verts[j]` is between
/// `edges[j]` and `edges[j + 1]`.
struct Chain {
    edges: Vec<EdgeId>,
    verts: Vec<VertexId>,
}

/// A planned join of a whole chain into its lowest edge.
struct ChainPlan {
    keep: EdgeId,
    gone: Vec<(EdgeId, [VertexId; 2])>,
    verts: Vec<VertexId>,
    curve: Curve,
    interval: Extent1d,
    ends: [VertexId; 2],
    accuracy: Option<Arc<Recorded>>,
    faces: Vec<(FaceId, Vec<LoopRecord>)>,
}

fn use_mid(g: &UseGeom) -> Point2 {
    match g {
        UseGeom::Pcurve { curve, interval } => curve.point(interval.mid()),
        UseGeom::Point(p) => *p,
    }
}

/// Twice the signed area of the loop's uv polygon, by samples along each use in the loop's
/// direction: the sign says outer or inner.
fn area2(uses: &[EdgeUse]) -> f64 {
    const SAMPLES: usize = 8;
    let mut ring: Vec<Point2> = Vec::with_capacity(uses.len() * SAMPLES);
    for u in uses {
        match &u.geom {
            UseGeom::Pcurve { curve, interval } => {
                let (lo, hi) = (interval.min(), interval.max());
                for k in 0..SAMPLES {
                    let s = k as f64 / SAMPLES as f64;
                    let t = match u.sense {
                        Sense::Forward => lo + (hi - lo) * s,
                        Sense::Reversed => hi - (hi - lo) * s,
                    };
                    ring.push(curve.point(t));
                }
            }
            UseGeom::Point(p) => ring.push(*p),
        }
    }
    let n = ring.len();
    (0..n)
        .filter_map(|i| Some((ring.get(i)?, ring.get((i + 1) % n)?)))
        .map(|(a, b)| a.x * b.y - b.x * a.y)
        .sum()
}

/// A closed curve's period.
const fn period(c: &Curve) -> Option<f64> {
    match c {
        Curve::Circle(_) | Curve::Ellipse(_) => Some(core::f64::consts::TAU),
        _ => None,
    }
}

/// The interval of two edges on equal curves, `i0` then `i1`, when `i1` runs on from `i0`
/// (up to a whole period of a closed curve) and the joined edge covers at most one period.
fn run_on(curve: &Curve, i0: Extent1d, i1: Extent1d) -> Option<Extent1d> {
    let shift = i0.max() - i1.min();
    let p = period(curve);
    let scale = 1.0 + i0.max().abs().max(i1.min().abs());
    let fits = shift.abs() <= RUN_ON * scale
        || p.is_some_and(|p| {
            let k = (shift / p).round();
            k != 0.0 && (shift - k * p).abs() <= RUN_ON * (scale + p)
        });
    if !fits {
        return None;
    }
    let end = i1.max() + shift;
    if p.is_some_and(|p| end - i0.min() > p * (1.0 + RUN_ON)) {
        return None;
    }
    Extent1d::new(i0.min(), end)
}

/// The larger of two recorded accuracies, as a bound no longer proved. Equal ones are
/// kept as they are.
fn larger(a: Option<&Arc<Recorded>>, b: Option<&Arc<Recorded>>) -> Option<Arc<Recorded>> {
    match (a, b) {
        (None, None) => None,
        _ if a == b => a.cloned(),
        (Some(r), None) | (None, Some(r)) => Some(Arc::new(unproved(r))),
        (Some(r), Some(s)) => {
            let big = if r.acc >= s.acc { r } else { s };
            Some(Arc::new(unproved(big)))
        }
    }
}

fn unproved(r: &Recorded) -> Recorded {
    let basis = match r.basis {
        Basis::Certified(c) => Basis::Unproven(c),
        other => other,
    };
    Recorded { acc: r.acc, basis }
}

impl Draft {
    /// Merges the faces on both sides of `edge` with the pair's other twins, as the
    /// [module](self) says.
    ///
    /// # Errors
    ///
    /// [`Refusal`]: `edge` or a protected id not live, `edge` not between two faces on one
    /// surface, or the pair left for a [`Why`]. The draft is unchanged.
    pub fn merge_faces_across(
        &mut self,
        edge: EdgeId,
        protected: &BTreeSet<EdgeId>,
        doc: &Document,
    ) -> Result<FaceMerge, Refusal> {
        self.live_protected(protected)?;
        if self.t.edges.get(edge).is_none() {
            return Err(Absent {
                entity: edge.into(),
            }
            .into());
        }
        let plan = self.plan_faces(edge, protected, *doc)?;
        let mut stars = Stars::of(&self.t);
        Ok(self.apply_faces(plan, &mut stars))
    }

    /// Merges every pair of faces on one surface that [`Draft::merge_faces_across`] would,
    /// in the order of the edges between them, once through.
    ///
    /// # Errors
    ///
    /// [`Absent`] for a protected id that is not a live edge; nothing is written.
    pub fn merge_faces(
        &mut self,
        protected: &BTreeSet<EdgeId>,
        doc: &Document,
    ) -> Result<Healed, Absent> {
        self.live_protected(protected)?;
        let mut stars = Stars::of(&self.t);
        Ok(self.merge_faces_with(protected, *doc, &mut stars))
    }

    /// Every connected group of faces that can merge is merged whole, one loop walk a group
    /// (linear in the group, where pair by pair re-walks a growing loop). A group is the
    /// faces joined by edges that are removable twins; it is merged at once when every twin
    /// lies in one uv window and no protected, other-shell or one-sense edge lies inside it.
    /// Any other group goes pair by pair (`plan_faces`), which keeps seams and refuses
    /// what a group cannot.
    fn merge_faces_with(
        &mut self,
        protected: &BTreeSet<EdgeId>,
        doc: Document,
        stars: &mut Stars,
    ) -> Healed {
        let mut out = Healed::default();
        let mut cands: Vec<(EdgeId, FaceId, FaceId)> = Vec::new();
        let mut blocked: BTreeMap<(FaceId, FaceId), Why> = BTreeMap::new();
        let mut blocked_edges: Vec<(EdgeId, (FaceId, FaceId), Why)> = Vec::new();
        for (e, geom) in self.t.edges.iter() {
            if !matches!(geom.shape, EdgeShape::Curve { .. }) {
                continue;
            }
            let Some([f0, f1]) = self
                .t
                .adjacency
                .get(e)
                .map(|a| a.fins.as_slice())
                .and_then(|f| match f {
                    [a, b] => Some([*a, *b]),
                    _ => None,
                })
            else {
                continue;
            };
            if f0.face == f1.face {
                continue;
            }
            let pair = (f0.face.min(f1.face), f0.face.max(f1.face));
            let (Some(fa), Some(fb)) = (self.t.faces.get(pair.0), self.t.faces.get(pair.1)) else {
                continue;
            };
            if fa.surface != fb.surface {
                continue;
            }
            let why = if protected.contains(&e) {
                Some(Why::Protected)
            } else if fa.sides != fb.sides {
                Some(Why::Sides)
            } else if f0.sense == f1.sense {
                Some(Why::Orientation)
            } else {
                None
            };
            match why {
                Some(w) => {
                    blocked.entry(pair).or_insert(w);
                    blocked_edges.push((e, pair, w));
                }
                None => cands.push((e, pair.0, pair.1)),
            }
        }
        cands.retain(|c| !blocked.contains_key(&(c.1, c.2)));
        // Union-find over the faces the candidates join.
        let mut parent: BTreeMap<FaceId, FaceId> = BTreeMap::new();
        for &(_, a, b) in &cands {
            parent.entry(a).or_insert(a);
            parent.entry(b).or_insert(b);
            let (ra, rb) = (find(&mut parent, a), find(&mut parent, b));
            if ra != rb {
                parent.insert(ra.max(rb), ra.min(rb));
            }
        }
        let faces_of: Vec<FaceId> = parent.keys().copied().collect();
        let mut groups: BTreeMap<FaceId, Vec<FaceId>> = BTreeMap::new();
        for f in faces_of {
            let r = find(&mut parent, f);
            groups.entry(r).or_default().push(f);
        }
        let mut twins_of: BTreeMap<FaceId, BTreeSet<EdgeId>> = BTreeMap::new();
        for &(e, a, _) in &cands {
            let r = find(&mut parent, a);
            twins_of.entry(r).or_default().insert(e);
        }
        let mut dirty: BTreeSet<FaceId> = BTreeSet::new();
        for pair in blocked.keys() {
            if parent.contains_key(&pair.0) && parent.contains_key(&pair.1) {
                let (ra, rb) = (find(&mut parent, pair.0), find(&mut parent, pair.1));
                if ra == rb {
                    dirty.insert(ra);
                }
            }
        }
        for (root, faces) in &groups {
            if dirty.contains(root) {
                continue;
            }
            let twins = twins_of.get(root).cloned().unwrap_or_default();
            match self.plan_group(faces, &twins) {
                // Written before the next group is planned: groups share edges.
                Ok(p) => out.faces.extend(self.apply_group(p, stars)),
                Err(Skip::Left(why)) => {
                    if let Some(&e) = twins.first() {
                        out.left.push(Left { at: e.into(), why });
                    }
                }
                Err(Skip::Silent) => {
                    dirty.insert(*root);
                }
            }
        }
        // What a group cannot take goes pair by pair.
        let mut fallback: Vec<EdgeId> = Vec::new();
        for &(e, a, _) in &cands {
            if dirty.contains(&find(&mut parent, a)) {
                fallback.push(e);
            }
        }
        for &(e, pair, _) in &blocked_edges {
            let inside = parent.contains_key(&pair.0)
                && parent.contains_key(&pair.1)
                && dirty.contains(&find(&mut parent, pair.0))
                && find(&mut parent, pair.0) == find(&mut parent, pair.1);
            if inside {
                fallback.push(e);
            }
        }
        fallback.sort_unstable();
        for e in fallback {
            if self.t.edges.get(e).is_none() {
                continue;
            }
            match self.plan_faces(e, protected, doc) {
                Ok(plan) => out.faces.push(self.apply_faces(plan, stars)),
                Err(Skip::Left(why)) => out.left.push(Left { at: e.into(), why }),
                Err(Skip::Silent) => {}
            }
        }
        // Pairs a protected, other-shell or one-sense edge holds apart, and not in a group
        // that went pair by pair (which listed them itself).
        for &(e, pair, why) in &blocked_edges {
            let held = parent.contains_key(&pair.0)
                && parent.contains_key(&pair.1)
                && dirty.contains(&find(&mut parent, pair.0))
                && find(&mut parent, pair.0) == find(&mut parent, pair.1);
            if !held && self.t.edges.get(e).is_some() {
                out.left.push(Left { at: e.into(), why });
            }
        }
        out.left.dedup();
        out
    }

    /// The merge of the faces of one group across its twins, or why not. `Silent` when a
    /// twin's two uses are in different uv windows: the group needs the pair walk's seams.
    fn plan_group(&self, faces: &[FaceId], twins: &BTreeSet<EdgeId>) -> Result<GroupPlan, Skip> {
        let t = &self.t;
        let mut recs = BTreeMap::new();
        for &f in faces {
            recs.insert(f, t.faces.get(f).ok_or(Skip::Silent)?);
        }
        let mut at: BTreeMap<EdgeId, Vec<At>> = BTreeMap::new();
        let mut lens: BTreeMap<(FaceId, usize), usize> = BTreeMap::new();
        for (&f, rec) in &recs {
            for (l, lp) in rec.loops.iter().enumerate() {
                lens.insert((f, l), lp.uses.len());
                for (p, u) in lp.uses.iter().enumerate() {
                    if twins.contains(&u.edge) {
                        at.entry(u.edge).or_default().push((f, l, p));
                    }
                }
            }
        }
        let use_at = |(f, l, p): At| {
            recs.get(&f)
                .and_then(|r| r.loops.get(l))
                .and_then(|lp| lp.uses.get(p))
        };
        let mut twin: BTreeMap<At, At> = BTreeMap::new();
        for tw in twins {
            let Some([x, y]) = at.get(tw).map(Vec::as_slice).and_then(|v| match v {
                [x, y] => Some([*x, *y]),
                _ => None,
            }) else {
                return Err(Skip::Silent);
            };
            let (Some(ux), Some(uy)) = (use_at(x), use_at(y)) else {
                return Err(Skip::Silent);
            };
            if (use_mid(&ux.geom) - use_mid(&uy.geom)).length() > SAME_UV {
                return Err(Skip::Silent);
            }
            twin.insert(x, y);
            twin.insert(y, x);
        }
        let next = |(f, l, p): At| -> At {
            let n = lens.get(&(f, l)).copied().unwrap_or(1).max(1);
            (f, l, (p + 1) % n)
        };
        let total: usize = lens.values().sum();
        let mut visited: BTreeSet<At> = BTreeSet::new();
        let mut cycles: Vec<Vec<At>> = Vec::new();
        for (&(f, l), &n) in &lens {
            for p in 0..n {
                let start = (f, l, p);
                if twin.contains_key(&start) || visited.contains(&start) {
                    continue;
                }
                let mut cycle = Vec::new();
                let mut cur = start;
                loop {
                    if !visited.insert(cur) {
                        return Err(Skip::Left(Why::Loops));
                    }
                    cycle.push(cur);
                    let mut nx = next(cur);
                    let mut steps = 0;
                    while let Some(&partner) = twin.get(&nx) {
                        nx = next(partner);
                        steps += 1;
                        if steps > total {
                            return Err(Skip::Left(Why::Loops));
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
        if cycles.is_empty() {
            return Err(Skip::Left(Why::Loops));
        }
        let mut loops: Vec<LoopRecord> = cycles
            .iter()
            .map(|c| LoopRecord::new(c.iter().filter_map(|&x| use_at(x)).cloned().collect()))
            .collect();
        loops.sort_by(|p, q| area2(&q.uses).total_cmp(&area2(&p.uses)));
        if loops.iter().filter(|l| area2(&l.uses) > 0.0).count() > 1 {
            return Err(Skip::Left(Why::Loops));
        }
        let survivor = *faces.first().ok_or(Skip::Silent)?;
        let absorbed: Vec<FaceId> = faces.iter().copied().skip(1).collect();
        let mut fixes: Vec<(EdgeId, Vec<Fin>)> = Vec::new();
        let mut done: BTreeSet<EdgeId> = BTreeSet::new();
        for f in &absorbed {
            let Some(rec) = recs.get(f) else { continue };
            for u in rec.loops.iter().flat_map(|l| &l.uses) {
                if twins.contains(&u.edge) || !done.insert(u.edge) {
                    continue;
                }
                let Some(adj) = t.adjacency.get(u.edge) else {
                    continue;
                };
                let fins = adj
                    .fins
                    .iter()
                    .map(|x| {
                        if absorbed.contains(&x.face) {
                            Fin::new(survivor, x.sense)
                        } else {
                            *x
                        }
                    })
                    .collect();
                fixes.push((u.edge, fins));
            }
        }
        Ok(GroupPlan {
            survivor,
            absorbed,
            twins: twins.iter().copied().collect(),
            loops,
            fixes,
        })
    }

    fn apply_group(&mut self, plan: GroupPlan, stars: &mut Stars) -> Vec<FaceMerge> {
        let GroupPlan {
            survivor,
            absorbed,
            twins,
            loops,
            fixes,
        } = plan;
        // The twins each absorbed face's merge reports: those it bounds.
        let bounded: Vec<Vec<EdgeId>> = absorbed
            .iter()
            .map(|&f| {
                twins
                    .iter()
                    .copied()
                    .filter(|e| {
                        self.t
                            .adjacency
                            .get(*e)
                            .is_some_and(|a| a.fins.iter().any(|x| x.face == f))
                    })
                    .collect()
            })
            .collect();
        for &f in &absorbed {
            self.merge_face(f, survivor);
        }
        self.edit_face(survivor, |r| r.loops = loops);
        for (edge, fins) in fixes {
            if let Some(adj) = self.adjacency_mut(edge) {
                adj.fins = fins;
            }
        }
        let mut ends: BTreeSet<VertexId> = BTreeSet::new();
        for &tw in &twins {
            if let Some((geom, _)) = self.remove_edge(tw) {
                stars.drop_edge(tw, geom.ends);
                ends.extend(geom.ends);
            }
        }
        for v in ends {
            if stars.star(v).is_empty() {
                self.remove_vertex(v);
            }
        }
        absorbed
            .into_iter()
            .zip(bounded)
            .map(|(absorbed, removed)| FaceMerge {
                absorbed,
                survivor,
                removed,
            })
            .collect()
    }

    /// Joins the two edges at `vertex`, as the [module](self) says.
    ///
    /// # Errors
    ///
    /// [`Refusal`]: `vertex` or a protected id not live, `vertex` not the end of exactly
    /// two edges on one curve, or the pair left for a [`Why`]. The draft is unchanged.
    pub fn join_edges_at(
        &mut self,
        vertex: VertexId,
        protected: &BTreeSet<EdgeId>,
    ) -> Result<EdgeJoin, Refusal> {
        self.live_protected(protected)?;
        if self.t.vertices.get(vertex).is_none() {
            return Err(Absent {
                entity: vertex.into(),
            }
            .into());
        }
        let mut stars = Stars::of(&self.t);
        let plan = self.plan_join(vertex, protected, &stars)?;
        Ok(self.apply_join(plan, &mut stars))
    }

    /// Joins every pair of edges that [`Draft::join_edges_at`] would, in the order of the
    /// vertices between them, once through.
    ///
    /// # Errors
    ///
    /// [`Absent`] for a protected id that is not a live edge; nothing is written.
    pub fn join_edges(&mut self, protected: &BTreeSet<EdgeId>) -> Result<Healed, Absent> {
        self.live_protected(protected)?;
        let mut stars = Stars::of(&self.t);
        Ok(self.join_edges_with(protected, &mut stars))
    }

    fn join_edges_with(&mut self, protected: &BTreeSet<EdgeId>, stars: &mut Stars) -> Healed {
        let mut out = Healed::default();
        let ids: Vec<VertexId> = self.t.vertices.iter().map(|(id, _)| id).collect();
        let mut done: BTreeSet<VertexId> = BTreeSet::new();
        for v in ids {
            if self.t.vertices.get(v).is_none() || done.contains(&v) {
                continue;
            }
            // A chain is joined at once; one that is left for a reason, or a ring, goes pair
            // by pair, in the order of the vertices, which says why.
            let Some(chain) = self.chain_at(v, protected, stars) else {
                self.join_one(v, protected, stars, &mut out);
                continue;
            };
            done.extend(chain.verts.iter().copied());
            if let Some(plan) = self.plan_chain(&chain) {
                let joins = self.apply_chain(plan, &chain, stars);
                out.edges.extend(joins);
            } else {
                let mut verts = chain.verts.clone();
                verts.sort_unstable();
                for w in verts {
                    if self.t.vertices.get(w).is_some() {
                        self.join_one(w, protected, stars, &mut out);
                    }
                }
            }
        }
        out
    }

    fn join_one(
        &mut self,
        v: VertexId,
        protected: &BTreeSet<EdgeId>,
        stars: &mut Stars,
        out: &mut Healed,
    ) {
        match self.plan_join(v, protected, stars) {
            Ok(plan) => out.edges.push(self.apply_join(plan, stars)),
            Err(Skip::Left(why)) => out.left.push(Left { at: v.into(), why }),
            Err(Skip::Silent) => {}
        }
    }

    /// Merges faces and joins edges, again and again until neither finds more: a face
    /// merge frees the vertices its edges ended at, and an edge join can make two faces'
    /// shared edge one. Terminates: each round that does something removes an entity.
    ///
    /// A protected edge joined onto another protected one leaves the set naming the
    /// survivor only; the report names each absorbed edge.
    ///
    /// # Errors
    ///
    /// [`Absent`] for a protected id that is not a live edge; nothing is written.
    pub fn heal(&mut self, protected: &BTreeSet<EdgeId>, doc: &Document) -> Result<Healed, Absent> {
        self.live_protected(protected)?;
        let mut protected = protected.clone();
        let mut stars = Stars::of(&self.t);
        let mut all = Healed::default();
        loop {
            let faces = self.merge_faces_with(&protected, *doc, &mut stars);
            let edges = self.join_edges_with(&protected, &mut stars);
            for j in &edges.edges {
                protected.remove(&j.absorbed);
            }
            let done = faces.faces.is_empty() && edges.edges.is_empty();
            all.faces.extend(faces.faces);
            all.edges.extend(edges.edges);
            if done {
                all.left = faces.left;
                all.left.extend(edges.left);
                return Ok(all);
            }
        }
    }

    fn live_protected(&self, protected: &BTreeSet<EdgeId>) -> Result<(), Absent> {
        match protected.iter().find(|e| self.t.edges.get(**e).is_none()) {
            Some(&e) => Err(Absent { entity: e.into() }),
            None => Ok(()),
        }
    }

    // Faces.

    fn plan_faces(
        &self,
        e: EdgeId,
        protected: &BTreeSet<EdgeId>,
        doc: Document,
    ) -> Result<FacePlan, Skip> {
        let t = &self.t;
        let geom = t.edges.get(e).ok_or(Skip::Silent)?;
        if !matches!(geom.shape, EdgeShape::Curve { .. }) {
            return Err(Skip::Silent);
        }
        let adj = t.adjacency.get(e).ok_or(Skip::Silent)?;
        let [f0, f1] = adj.fins.as_slice() else {
            return Err(Skip::Silent);
        };
        if f0.face == f1.face {
            return Err(Skip::Silent);
        }
        let (a, b) = (f0.face.min(f1.face), f0.face.max(f1.face));
        let (Some(fa), Some(fb)) = (t.faces.get(a), t.faces.get(b)) else {
            return Err(Skip::Silent);
        };
        if fa.surface != fb.surface {
            return Err(Skip::Silent);
        }
        if protected.contains(&e) {
            return Err(Skip::Left(Why::Protected));
        }
        if fa.sides != fb.sides {
            return Err(Skip::Left(Why::Sides));
        }
        // The twins: the edges only these two faces bound.
        let mut seen: BTreeSet<EdgeId> = BTreeSet::new();
        let mut twins: Vec<EdgeId> = Vec::new();
        for u in fa.loops.iter().flat_map(|l| &l.uses) {
            if !seen.insert(u.edge) {
                continue;
            }
            let (Some(g), Some(adj)) = (t.edges.get(u.edge), t.adjacency.get(u.edge)) else {
                continue;
            };
            let [p, q] = adj.fins.as_slice() else {
                continue;
            };
            let pair = (p.face == a && q.face == b) || (p.face == b && q.face == a);
            if !pair || !matches!(g.shape, EdgeShape::Curve { .. }) {
                continue;
            }
            if protected.contains(&u.edge) {
                return Err(Skip::Left(Why::Protected));
            }
            if p.sense == q.sense {
                return Err(Skip::Left(Why::Orientation));
            }
            twins.push(u.edge);
        }
        twins.sort_unstable();
        // Where each twin is used, in each face.
        let mut at: BTreeMap<EdgeId, [Option<At>; 2]> = BTreeMap::new();
        for (slot, (id, rec)) in [(a, fa), (b, fb)].into_iter().enumerate() {
            for (l, lp) in rec.loops.iter().enumerate() {
                for (p, u) in lp.uses.iter().enumerate() {
                    if twins.binary_search(&u.edge).is_ok()
                        && let Some(cell) = at.entry(u.edge).or_default().get_mut(slot)
                    {
                        *cell = Some((id, l, p));
                    }
                }
            }
        }
        let rec = |f: FaceId| if f == a { fa } else { fb };
        let use_at = |(f, l, p): At| rec(f).loops.get(l).and_then(|lp| lp.uses.get(p));
        // Each twin's two uses in uv: `b`'s window against `a`'s. A twin whose uses lie in
        // one window is in step; one a period apart meets across a seam. The twins in step
        // with the first one that is in one window (else with the lowest) go; the others
        // stay as seams.
        let offset = |tw: EdgeId| -> Option<Vec2> {
            let Some([Some(x), Some(y)]) = at.get(&tw) else {
                return None;
            };
            let (ux, uy) = (use_at(*x)?, use_at(*y)?);
            Some(use_mid(&ux.geom) - use_mid(&uy.geom))
        };
        let offsets: Vec<(EdgeId, Vec2)> = twins
            .iter()
            .filter_map(|&tw| Some((tw, offset(tw)?)))
            .collect();
        let reference = offsets
            .iter()
            .find(|(_, o)| o.length() <= SAME_UV)
            .or_else(|| offsets.first())
            .map(|&(_, o)| o);
        let Some(reference) = reference else {
            return Err(Skip::Silent);
        };
        let exact = reference.length() > SAME_UV;
        let removed: Vec<EdgeId> = offsets
            .iter()
            .filter(|(_, o)| (*o - reference).length() <= SAME_UV)
            .map(|&(tw, _)| tw)
            .collect();
        let mut twin: BTreeMap<At, At> = BTreeMap::new();
        for tw in &removed {
            let Some([Some(x), Some(y)]) = at.get(tw) else {
                return Err(Skip::Silent);
            };
            twin.insert(*x, *y);
            twin.insert(*y, *x);
        }
        // Walk the loops across the twins.
        let mut lens: BTreeMap<(FaceId, usize), usize> = BTreeMap::new();
        for (id, r) in [(a, fa), (b, fb)] {
            for (l, lp) in r.loops.iter().enumerate() {
                lens.insert((id, l), lp.uses.len());
            }
        }
        let next = |(f, l, p): At| -> At {
            let n = lens.get(&(f, l)).copied().unwrap_or(1).max(1);
            (f, l, (p + 1) % n)
        };
        let total: usize = lens.values().sum();
        let mut visited: BTreeSet<At> = BTreeSet::new();
        let mut cycles: Vec<Vec<At>> = Vec::new();
        for (&(f, l), &n) in &lens {
            for p in 0..n {
                let start = (f, l, p);
                if twin.contains_key(&start) || visited.contains(&start) {
                    continue;
                }
                let mut cycle = Vec::new();
                let mut cur = start;
                loop {
                    if !visited.insert(cur) {
                        return Err(Skip::Left(Why::Loops));
                    }
                    cycle.push(cur);
                    let mut nx = next(cur);
                    let mut steps = 0;
                    while let Some(&partner) = twin.get(&nx) {
                        nx = next(partner);
                        steps += 1;
                        if steps > total {
                            return Err(Skip::Left(Why::Loops));
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
        if cycles.is_empty() {
            return Err(Skip::Left(Why::Loops));
        }
        let loops = if exact {
            let mut written: Vec<ExactLoop> = Vec::with_capacity(cycles.len());
            for c in &cycles {
                let seq: Vec<(EdgeId, Sense)> = c
                    .iter()
                    .filter_map(|&x| use_at(x))
                    .map(|u| (u.edge, u.sense))
                    .collect();
                let one = self
                    .exact_loop(&fa.surface, &seq, &doc)
                    .map_err(|e| Skip::Left(Why::Pcurves(e)))?;
                written.push(one);
            }
            written.sort_by(|p, q| area2(&q.record().uses).total_cmp(&area2(&p.record().uses)));
            let outer = written
                .iter()
                .filter(|w| area2(&w.record().uses) > 0.0)
                .count();
            if outer > 1 {
                return Err(Skip::Left(Why::Loops));
            }
            placed(&fa.surface, written).map_err(|e| Skip::Left(Why::Pcurves(e)))?
        } else {
            let mut loops: Vec<LoopRecord> = cycles
                .iter()
                .map(|c| LoopRecord::new(c.iter().filter_map(|&x| use_at(x)).cloned().collect()))
                .collect();
            loops.sort_by(|p, q| area2(&q.uses).total_cmp(&area2(&p.uses)));
            if loops.iter().filter(|l| area2(&l.uses) > 0.0).count() > 1 {
                return Err(Skip::Left(Why::Loops));
            }
            loops
        };
        // The fins the absorbed face leaves on its edges, with the survivor in its place.
        let gone: BTreeSet<EdgeId> = removed.iter().copied().collect();
        let mut fixes: Vec<(EdgeId, Vec<Fin>)> = Vec::new();
        let mut done: BTreeSet<EdgeId> = BTreeSet::new();
        for u in fb.loops.iter().flat_map(|l| &l.uses) {
            if gone.contains(&u.edge) || !done.insert(u.edge) {
                continue;
            }
            let Some(adj) = t.adjacency.get(u.edge) else {
                continue;
            };
            let fins = adj
                .fins
                .iter()
                .map(|f| {
                    if f.face == b {
                        Fin::new(a, f.sense)
                    } else {
                        *f
                    }
                })
                .collect();
            fixes.push((u.edge, fins));
        }
        Ok(FacePlan {
            survivor: a,
            absorbed: b,
            twins: removed,
            loops,
            fixes,
        })
    }

    fn apply_faces(&mut self, plan: FacePlan, stars: &mut Stars) -> FaceMerge {
        let FacePlan {
            survivor,
            absorbed,
            twins,
            loops,
            fixes,
        } = plan;
        // The plan read both faces live, so neither write can miss.
        self.merge_face(absorbed, survivor);
        self.edit_face(survivor, |r| r.loops = loops);
        for (edge, fins) in fixes {
            if let Some(adj) = self.adjacency_mut(edge) {
                adj.fins = fins;
            }
        }
        let mut ends: BTreeSet<VertexId> = BTreeSet::new();
        for &tw in &twins {
            if let Some((geom, _)) = self.remove_edge(tw) {
                stars.drop_edge(tw, geom.ends);
                ends.extend(geom.ends);
            }
        }
        for v in ends {
            if stars.star(v).is_empty() {
                self.remove_vertex(v);
            }
        }
        FaceMerge {
            absorbed,
            survivor,
            removed: twins,
        }
    }

    // Edges.

    /// What joining the two edges at `v` needs of the edges alone: the pair on one curve,
    /// running on, with the same faces on its sides.
    fn link(
        &self,
        v: VertexId,
        protected: &BTreeSet<EdgeId>,
        stars: &Stars,
    ) -> Result<Link<'_>, Skip> {
        let t = &self.t;
        let &[e0, e1] = stars.star(v) else {
            return Err(Skip::Silent);
        };
        if e0 == e1 {
            return Err(Skip::Silent);
        }
        let (Some(g0), Some(g1)) = (t.edges.get(e0), t.edges.get(e1)) else {
            return Err(Skip::Silent);
        };
        // `first` runs into `v`, `second` out of it.
        let (first, second, g_first, g_second) = if g0.ends[1] == v && g1.ends[0] == v {
            (e0, e1, g0, g1)
        } else if g1.ends[1] == v && g0.ends[0] == v {
            (e1, e0, g1, g0)
        } else {
            return Err(Skip::Silent);
        };
        if g_first.ends[0] == v || g_second.ends[1] == v {
            return Err(Skip::Silent);
        }
        let (
            EdgeShape::Curve {
                curve: c0,
                interval: i0,
            },
            EdgeShape::Curve {
                curve: c1,
                interval: i1,
            },
        ) = (&g_first.shape, &g_second.shape)
        else {
            return Err(Skip::Silent);
        };
        if c0 != c1 {
            return Err(Skip::Silent);
        }
        let (Some(a0), Some(a1)) = (t.adjacency.get(first), t.adjacency.get(second)) else {
            return Err(Skip::Silent);
        };
        if a0.fins.is_empty() || a1.fins.is_empty() {
            // Wire edges are the shell's: left alone.
            return Err(Skip::Silent);
        }
        if protected.contains(&first) != protected.contains(&second) {
            return Err(Skip::Left(Why::Protected));
        }
        let key = |f: &Fin| (f.face, f.sense == Sense::Forward);
        let mut fa: Vec<_> = a0.fins.iter().map(key).collect();
        let mut fb: Vec<_> = a1.fins.iter().map(key).collect();
        fa.sort_unstable();
        fb.sort_unstable();
        if fa != fb {
            return Err(Skip::Left(Why::Fins));
        }
        let interval = run_on(c0, *i0, *i1).ok_or(Skip::Left(Why::Interval))?;
        Ok(Link {
            first,
            second,
            curve: c0,
            interval,
            ends: [g_first.ends[0], g_second.ends[1]],
            accuracy: larger(g_first.accuracy.as_ref(), g_second.accuracy.as_ref()),
            faces: a0.fins.iter().map(|f| f.face).collect(),
        })
    }

    fn plan_join(
        &self,
        v: VertexId,
        protected: &BTreeSet<EdgeId>,
        stars: &Stars,
    ) -> Result<JoinPlan, Skip> {
        let t = &self.t;
        let link = self.link(v, protected, stars)?;
        let (first, second) = (link.first, link.second);
        let keep = first.min(second);
        let gone = first.max(second);
        let mut faces: Vec<(FaceId, Vec<LoopRecord>)> = Vec::new();
        let mut done: BTreeSet<FaceId> = BTreeSet::new();
        for &f in &link.faces {
            if !done.insert(f) {
                continue;
            }
            let rec = t.faces.get(f).ok_or(Skip::Left(Why::Uses))?;
            let mut loops = Vec::with_capacity(rec.loops.len());
            for lp in &rec.loops {
                let uses = join_uses(&lp.uses, first, second, keep, link.interval)
                    .ok_or(Skip::Left(Why::Uses))?;
                loops.push(LoopRecord::new(uses));
            }
            faces.push((f, loops));
        }
        let gone_ends = t.edges.get(gone).map_or([v, v], |g| g.ends);
        Ok(JoinPlan {
            keep,
            gone,
            vertex: v,
            curve: link.curve.clone(),
            interval: link.interval,
            ends: link.ends,
            gone_ends,
            accuracy: link.accuracy,
            faces,
        })
    }

    /// The maximal chain of joinable pairs through `v`, `None` when `v` is not between
    /// joinable edges, the chain is a closed ring or it is one join (the single join does
    /// that).
    fn chain_at(&self, v: VertexId, protected: &BTreeSet<EdgeId>, stars: &Stars) -> Option<Chain> {
        let start = self.link(v, protected, stars).ok()?;
        let mut edges: VecDeque<EdgeId> = VecDeque::from([start.first, start.second]);
        let mut verts: VecDeque<VertexId> = VecDeque::from([v]);
        let mut seen: BTreeSet<VertexId> = BTreeSet::from([v]);
        let mut at = start.ends[1];
        let mut last = start.second;
        while let Ok(l) = self.link(at, protected, stars)
            && l.first == last
        {
            if !seen.insert(at) {
                return None;
            }
            verts.push_back(at);
            edges.push_back(l.second);
            at = l.ends[1];
            last = l.second;
        }
        let mut at = start.ends[0];
        let mut head = start.first;
        while let Ok(l) = self.link(at, protected, stars)
            && l.second == head
        {
            if !seen.insert(at) {
                return None;
            }
            verts.push_front(at);
            edges.push_front(l.first);
            at = l.ends[0];
            head = l.first;
        }
        (verts.len() >= 2).then(|| Chain {
            edges: edges.into(),
            verts: verts.into(),
        })
    }

    /// Plans the join of a whole chain at once: every loop of every face on it is rebuilt
    /// once. `None` when anything is left for a [`Why`]; the pairs are then joined one by
    /// one, which reports it.
    fn plan_chain(&self, chain: &Chain) -> Option<ChainPlan> {
        let t = &self.t;
        let first_g = t.edges.get(*chain.edges.first()?)?;
        let EdgeShape::Curve {
            curve,
            interval: first_interval,
        } = &first_g.shape
        else {
            return None;
        };
        // The interval and accuracy after each edge.
        let mut prefix: Vec<Extent1d> = vec![*first_interval];
        let mut accuracy = first_g.accuracy.clone();
        let mut end = first_g.ends[1];
        for &e in chain.edges.iter().skip(1) {
            let g = t.edges.get(e)?;
            let EdgeShape::Curve { interval, .. } = &g.shape else {
                return None;
            };
            prefix.push(run_on(curve, *prefix.last()?, *interval)?);
            accuracy = larger(accuracy.as_ref(), g.accuracy.as_ref());
            end = g.ends[1];
        }
        let interval = *prefix.last()?;
        let keep = *chain.edges.iter().min()?;
        let pos: BTreeMap<EdgeId, usize> = chain
            .edges
            .iter()
            .enumerate()
            .map(|(i, &e)| (e, i))
            .collect();
        let a = t.adjacency.get(*chain.edges.first()?)?;
        let mut faces: Vec<(FaceId, Vec<LoopRecord>)> = Vec::new();
        let mut done: BTreeSet<FaceId> = BTreeSet::new();
        for f in a.fins.iter().map(|f| f.face) {
            if !done.insert(f) {
                continue;
            }
            let rec = t.faces.get(f)?;
            let mut loops = Vec::with_capacity(rec.loops.len());
            for lp in &rec.loops {
                let uses = join_chain_uses(&lp.uses, &pos, chain, &prefix, keep)?;
                loops.push(LoopRecord::new(uses));
            }
            faces.push((f, loops));
        }
        let gone: Vec<(EdgeId, [VertexId; 2])> = chain
            .edges
            .iter()
            .filter(|&&e| e != keep)
            .map(|&e| t.edges.get(e).map(|g| (e, g.ends)))
            .collect::<Option<_>>()?;
        Some(ChainPlan {
            keep,
            gone,
            verts: chain.verts.clone(),
            curve: curve.clone(),
            interval,
            ends: [first_g.ends[0], end],
            accuracy,
            faces,
        })
    }

    fn apply_chain(&mut self, plan: ChainPlan, chain: &Chain, stars: &mut Stars) -> Vec<EdgeJoin> {
        let ChainPlan {
            keep,
            gone,
            verts,
            curve,
            interval,
            ends,
            accuracy,
            faces,
        } = plan;
        for &(g, _) in &gone {
            self.merge_edge(g, keep);
        }
        if let Some(g) = self.edge_mut(keep) {
            g.shape = EdgeShape::Curve { curve, interval };
            g.ends = ends;
            g.accuracy = accuracy;
        }
        for (f, loops) in faces {
            self.edit_face(f, |r| r.loops = loops);
        }
        for &v in &verts {
            self.remove_vertex(v);
        }
        for &(g, g_ends) in &gone {
            stars.rename(g, keep, g_ends);
            stars.rename(g, keep, ends);
        }
        for &v in &verts {
            if let Some(star) = stars.at.get_mut(v.index()) {
                star.clear();
            }
        }
        // Pair `j` of the chain is at `verts[j]`: the edge on the far side of it from the
        // survivor is the one that went.
        let at = chain.edges.iter().position(|&e| e == keep).unwrap_or(0);
        verts
            .iter()
            .enumerate()
            .filter_map(|(j, &vertex)| {
                let absorbed = *chain.edges.get(if j < at { j } else { j + 1 })?;
                Some(EdgeJoin {
                    absorbed,
                    survivor: keep,
                    vertex,
                })
            })
            .collect()
    }

    fn apply_join(&mut self, plan: JoinPlan, stars: &mut Stars) -> EdgeJoin {
        let JoinPlan {
            keep,
            gone,
            vertex,
            curve,
            interval,
            ends,
            gone_ends,
            accuracy,
            faces,
        } = plan;
        // The plan read both edges live, so none of these writes can miss.
        self.merge_edge(gone, keep);
        if let Some(g) = self.edge_mut(keep) {
            g.shape = EdgeShape::Curve { curve, interval };
            g.ends = ends;
            g.accuracy = accuracy;
        }
        for (f, loops) in faces {
            self.edit_face(f, |r| r.loops = loops);
        }
        self.remove_vertex(vertex);
        stars.rename(gone, keep, gone_ends);
        stars.rename(gone, keep, ends);
        if let Some(star) = stars.at.get_mut(vertex.index()) {
            star.clear();
        }
        EdgeJoin {
            absorbed: gone,
            survivor: keep,
            vertex,
        }
    }
}

/// A loop's uses with each pass of `first` then `second` (forward) or `second` then `first`
/// (reversed) made one use of `keep` over `interval`. `None` when a use of either is not in
/// such a pair, or a pair's pcurves are not equal and running on.
fn join_uses(
    uses: &[EdgeUse],
    first: EdgeId,
    second: EdgeId,
    keep: EdgeId,
    interval: Extent1d,
) -> Option<Vec<EdgeUse>> {
    let n = uses.len();
    let mut consumed = vec![false; n];
    let mut replace: Vec<Option<EdgeUse>> = vec![None; n];
    for (i, u) in uses.iter().enumerate() {
        let j = (i + 1) % n;
        let (a, b) = match u.sense {
            Sense::Forward if u.edge == first => (u, uses.get(j)?),
            Sense::Reversed if u.edge == second => (uses.get(j)?, u),
            _ => continue,
        };
        if j == i
            || a.edge != first
            || b.edge != second
            || a.sense != b.sense
            || consumed.get(j) == Some(&true)
        {
            return None;
        }
        let geom = joined_geom(&a.geom, &b.geom, interval)?;
        let mut merged = EdgeUse::new(keep, u.sense, geom);
        merged.bound = larger(a.bound.as_ref(), b.bound.as_ref());
        *replace.get_mut(i)? = Some(merged);
        *consumed.get_mut(j)? = true;
    }
    let mut out = Vec::with_capacity(n);
    for (i, u) in uses.iter().enumerate() {
        if consumed.get(i) == Some(&true) {
            continue;
        }
        match replace.get_mut(i).and_then(Option::take) {
            Some(m) => out.push(m),
            None if u.edge == first || u.edge == second => return None,
            None => out.push(u.clone()),
        }
    }
    Some(out)
}

/// A loop's uses with each run of the whole chain (forward, in order, or reversed, in the
/// other) made one use of `keep` over the chain's joined interval. `None` when a use of a
/// chain edge is in no whole run, or a pair's pcurves are not equal and running on.
fn join_chain_uses(
    uses: &[EdgeUse],
    pos: &BTreeMap<EdgeId, usize>,
    chain: &Chain,
    prefix: &[Extent1d],
    keep: EdgeId,
) -> Option<Vec<EdgeUse>> {
    let n = uses.len();
    let k = chain.edges.len();
    let at = |i: usize| -> Option<(usize, &EdgeUse)> {
        let u = uses.get(i % n)?;
        Some((*pos.get(&u.edge)?, u))
    };
    // The use after `(j, u)` in the run, if the next one continues it.
    let continues = |i: usize| -> Option<bool> {
        let (j, u) = at(i)?;
        let (p, w) = at(i + n - 1)?;
        Some(
            u.sense == w.sense
                && match u.sense {
                    Sense::Forward => j == p + 1,
                    Sense::Reversed => j + 1 == p,
                },
        )
    };
    let mut consumed = vec![false; n];
    let mut replace: Vec<Option<EdgeUse>> = vec![None; n];
    let mut members = 0;
    for i in 0..n {
        if !pos.contains_key(&uses.get(i)?.edge) || continues(i) == Some(true) && n > 1 {
            continue;
        }
        // A run starts here: at the chain's first edge forward, its last reversed.
        let (j, u) = at(i)?;
        let sense = u.sense;
        if j != if sense == Sense::Forward { 0 } else { k - 1 } || k > n {
            return None;
        }
        let mut run: Vec<&EdgeUse> = Vec::with_capacity(k);
        for m in 0..k {
            let (jm, um) = at(i + m)?;
            let want = if sense == Sense::Forward {
                m
            } else {
                k - 1 - m
            };
            if jm != want || um.sense != sense {
                return None;
            }
            run.push(um);
            if m > 0 {
                *consumed.get_mut((i + m) % n)? = true;
            }
        }
        members += k;
        if sense == Sense::Reversed {
            run.reverse();
        }
        let mut geom = run.first()?.geom.clone();
        let mut bound = run.first()?.bound.clone();
        for (m, w) in run.iter().enumerate().skip(1) {
            geom = joined_geom(&geom, &w.geom, *prefix.get(m)?)?;
            bound = larger(bound.as_ref(), w.bound.as_ref());
        }
        let mut merged = EdgeUse::new(keep, sense, geom);
        merged.bound = bound;
        *replace.get_mut(i)? = Some(merged);
    }
    let total = uses.iter().filter(|u| pos.contains_key(&u.edge)).count();
    if members != total {
        return None;
    }
    let mut out = Vec::with_capacity(n);
    for (i, u) in uses.iter().enumerate() {
        if consumed.get(i) == Some(&true) {
            continue;
        }
        match replace.get_mut(i).and_then(Option::take) {
            Some(m) => out.push(m),
            None if pos.contains_key(&u.edge) => return None,
            None => out.push(u.clone()),
        }
    }
    Some(out)
}

/// The pcurve of two uses, `a` then `b` along the edge, when they are one curve and `b`
/// starts where `a` ends in parameter (no period between): over the joined interval.
fn joined_geom(a: &UseGeom, b: &UseGeom, interval: Extent1d) -> Option<UseGeom> {
    let (
        UseGeom::Pcurve {
            curve: ca,
            interval: ia,
        },
        UseGeom::Pcurve {
            curve: cb,
            interval: ib,
        },
    ) = (a, b)
    else {
        return None;
    };
    if ca != cb {
        return None;
    }
    let scale = 1.0 + ia.max().abs().max(ib.min().abs());
    let shift = ia.max() - ib.min();
    let step = shift.abs() <= RUN_ON * scale
        // A circle or ellipse in uv is itself periodic in its parameter: the second use
        // taken a whole turn on is the same uv curve.
        || (matches!(ca, Curve2::Circle(_) | Curve2::Ellipse(_))
            && (shift - (shift / core::f64::consts::TAU).round() * core::f64::consts::TAU).abs()
                <= RUN_ON * (scale + core::f64::consts::TAU));
    let runs = step
        && (ia.min() - interval.min()).abs() <= RUN_ON * scale
        && (ib.max() + shift - interval.max()).abs() <= RUN_ON * (scale + shift.abs());
    runs.then(|| UseGeom::Pcurve {
        curve: ca.clone(),
        interval,
    })
}
