// Written for solidcorex. Apache-2.0.
//! The certified heal (research 75 §6, RP1): each item a defect the check reports, done only
//! when every edit is certified within the accuracies the body records. No accuracy grows.
//!
//! **Short-edge clusters** ([`Cluster`]). Vertices joined by an edge and within coincidence
//! of each other (the check's `Finding::VerticesWithinCoincidence` for a pair an edge joins) are one
//! point the body records twice. Union-find over such edges gives the clusters; each is
//! merged into its survivor (the vertex of the largest reach, the lowest id on a tie) at the
//! target, the centre of the cluster's box, as the pin does. The edges with both ends in the
//! cluster are squeezed (removed with their uses); the others at the cluster are re-ended at
//! the target.
//!
//! **Certified, lines and planes exactly.** An item is done only when:
//!
//! - every vertex of the cluster is within the survivor's reach of the target ([`Spread`]),
//!   and every squeezed edge's curve is, by interval enclosures over 16 pieces ([`TooLong`]),
//!   so the merged point stands for each point it replaces;
//! - each re-ended edge is a line, rebuilt between its new ends, longer than its reach
//!   ([`NeighbourShort`]) and ending at its vertices within `re + rv` ([`EndOffVertex`]);
//! - each of its faces is a plane, and the use's new pcurve (the projection of the line, an
//!   affine map of the same parameter) lifts within the smaller of the edge's and the use's
//!   reach of the line ([`OffFace`]). The lift's gap is affine in the parameter, so its
//!   largest value is at an end; both ends are enclosed in intervals. That one bound answers
//!   the check's two questions for the use: the edge lies on the face within `re`, the use
//!   follows the edge within `rb`;
//! - the target is clear of every other vertex, by more than the larger reach ([`NotClear`]);
//! - no loop is left with all its uses squeezed, two or fewer lines, or the survivor twice.
//!
//! **Items in order.** Each item is certified against the items before it that were done:
//! their targets, their squeezed uses. A re-ended edge between two clusters is certified by
//! the later one with both new ends. So whether an item fails depends only on the items
//! below it that did not, and one pass is the fixpoint of 01 §5.2's `Skip` mode: every
//! skipped item is round 1.
//!
//! [`Spread`]: HealCause::Spread
//! [`TooLong`]: HealCause::TooLong
//! [`NeighbourShort`]: HealCause::NeighbourShort
//! [`EndOffVertex`]: HealCause::EndOffVertex
//! [`OffFace`]: HealCause::OffFace
//! [`NotClear`]: HealCause::NotClear

use std::collections::{BTreeMap, BTreeSet};

use scx_brep::op::{Cx, Done, Report, Skipped};
use scx_brep::{Brep, Changes, EdgeId, EdgeShape, FaceId, UseGeom, VertexId};
use scx_curve::{Curve, Curve2, Line, Line2};
use scx_num::budget::Budget;
use scx_num::cert::Recorded;
use scx_num::extent1d::Extent1d;
use scx_num::interval::Ivl;
use scx_num::units::{Length, Resolution};
use scx_num::vector2d::Point2;
use scx_num::vector3d::{Point3, Vec3};
use scx_surface::{Bracket, Plane, Surface};

use crate::error::{HealCause, RepairCause, RepairDefect, RepairError};

/// What a heal does with an item that fails (01 §5.2).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
#[non_exhaustive]
pub enum OnItemFailure {
    /// No body: the lowest failing item's cause ([`RepairError::Cause`]).
    #[default]
    Stop,
    /// No body: every failing item's cause ([`RepairError::Items`]).
    Collect,
    /// The body with the failing items left as they were, each in
    /// [`Report::skipped`](scx_brep::op::Report).
    Skip,
}

/// One defect the heal treats as a unit.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub enum Item {
    /// Vertices within coincidence joined by edges.
    Cluster(Cluster),
}

/// Vertices joined by edges and within coincidence of each other, to be one vertex.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub struct Cluster {
    /// The vertices, in id order; at least two.
    pub vertices: Vec<VertexId>,
    /// The edges with both ends among them, in id order: squeezed by the heal.
    pub edges: Vec<EdgeId>,
    /// The vertex kept: of the largest reach, the lowest id on a tie.
    pub survivor: VertexId,
    /// Its reach: the allowance every point of the cluster is held to.
    pub reach: Length,
    /// Where the survivor goes: the centre of the cluster's box.
    pub target: Point3,
}

/// The heal's items for `body`, in order: each cluster by its lowest vertex id.
#[must_use]
pub fn items(body: &Brep) -> Vec<Item> {
    clusters(body).into_iter().map(Item::Cluster).collect()
}

/// Heals `body`: each [`Item`] done when every edit it makes is certified, in item order.
/// The body's accuracies are kept; an item that would need a larger one fails.
///
/// # Errors
///
/// - [`RepairCause::OtherDocument`] when `body` was finished under another resolution;
/// - by `on_failure`: [`RepairError::Cause`] with the lowest failing item's [`HealCause`],
///   or [`RepairError::Items`] with every one; under [`OnItemFailure::Skip`] none;
/// - [`RepairError::Exhausted`] when the budget runs out, [`RepairError::Cancelled`] when
///   `cx.stop` is set;
/// - [`RepairError::Defect`] when the certified edits do not finish: a bug.
pub fn heal(
    body: &Brep,
    on_failure: OnItemFailure,
    cx: &mut Cx<'_>,
) -> Result<Done<HealCause>, RepairError> {
    if body.resolution() != cx.doc.resolution() {
        return Err(RepairError::Cause(RepairCause::OtherDocument));
    }
    cx.stop.check().map_err(|_| RepairError::Cancelled)?;
    let mut budget = cx.budget();
    let found = body.vertices().len() + body.edges().len();
    budget
        .charge(found as u64)
        .map_err(|_| RepairError::Exhausted { item: 0, work: 0 })?;
    let clusters = clusters(body);
    let mut state = State::new(body);
    let mut failed: Vec<Skipped<HealCause>> = Vec::new();
    for (item, cluster) in clusters.iter().enumerate() {
        cx.stop.check().map_err(|_| RepairError::Cancelled)?;
        let before = budget.used();
        match state.certify(cluster, &mut budget) {
            Ok(Some(done)) => state.accept(cluster, done),
            Ok(None) => {
                return Err(RepairError::Exhausted { item, work: before });
            }
            Err(cause) => {
                if on_failure == OnItemFailure::Stop {
                    return Err(RepairError::Cause(RepairCause::Heal(cause)));
                }
                failed.push(Skipped {
                    item,
                    round: 1,
                    cause,
                });
            }
        }
    }
    if on_failure == OnItemFailure::Collect && !failed.is_empty() {
        return Err(RepairError::Items(
            failed
                .into_iter()
                .map(|s| Skipped {
                    item: s.item,
                    round: s.round,
                    cause: RepairCause::Heal(s.cause),
                })
                .collect(),
        ));
    }
    let work = budget.used();
    if state.targets.is_empty() {
        let report = Report::new(Vec::new(), failed, Vec::new(), work);
        return Ok(Done::new(body.clone(), Changes::default(), report));
    }
    let (healed, changes) = state.apply(*cx.doc)?;
    Ok(Done::new(
        healed,
        changes,
        Report::new(Vec::new(), failed, Vec::new(), work),
    ))
}

/// The reach of a recorded accuracy: finish's rule, at least the zone.
fn reach(recorded: Option<&Recorded>, res: Resolution) -> f64 {
    let zone = res.zone().mm();
    recorded.map_or(zone, |r| r.acc.length().mm().max(zone))
}

fn ivl3(p: Point3) -> Point3<Ivl> {
    p.map(Ivl::from)
}

/// The enclosed length of `v`; `None` when it overflows.
fn norm(v: Vec3<Ivl>) -> Option<Bracket> {
    let n = (v.x * v.x + v.y * v.y + v.z * v.z).sqrt();
    if !(n.lo.is_finite() && n.hi.is_finite()) {
        return None;
    }
    Bracket::new(n.lo.max(0.0), n.hi)
}

/// The enclosed distance between two stored points; `None` when it overflows.
fn distance(p: Point3, q: Point3) -> Option<Bracket> {
    norm(ivl3(p) - ivl3(q))
}

/// The clusters of `body`, by lowest vertex id.
fn clusters(body: &Brep) -> Vec<Cluster> {
    let res = body.resolution();
    let ids: Vec<VertexId> = body.vertices().map(scx_brep::Vertex::id).collect();
    let index: BTreeMap<VertexId, usize> = ids.iter().enumerate().map(|(i, &v)| (v, i)).collect();
    let mut parent: Vec<usize> = (0..ids.len()).collect();
    for e in body.edges() {
        if !matches!(e.shape(), EdgeShape::Curve { .. }) {
            continue;
        }
        let [a, b] = e.ends();
        if a == b {
            continue;
        }
        let (Some(va), Some(vb), Some(&ia), Some(&ib)) =
            (body.vertex(a), body.vertex(b), index.get(&a), index.get(&b))
        else {
            continue;
        };
        let allowed = reach(va.accuracy(), res).max(reach(vb.accuracy(), res));
        let within = distance(va.point(), vb.point()).and_then(|d| d.exceeds(allowed));
        if within != Some(false) {
            continue;
        }
        let (ra, rb) = (root(&mut parent, ia), root(&mut parent, ib));
        // The lower index becomes the root, so a group's root is its lowest vertex.
        if let Some(slot) = parent.get_mut(ra.max(rb)) {
            *slot = ra.min(rb);
        }
    }
    let mut groups: BTreeMap<usize, Vec<VertexId>> = BTreeMap::new();
    for (i, &v) in ids.iter().enumerate() {
        groups.entry(root(&mut parent, i)).or_default().push(v);
    }
    groups
        .into_values()
        .filter(|g| g.len() >= 2)
        .filter_map(|g| cluster(body, g, res))
        .collect()
}

/// The root of `i`'s set, halving the path on the way.
fn root(parent: &mut [usize], mut i: usize) -> usize {
    loop {
        let Some(&p) = parent.get(i) else { return i };
        if p == i {
            return i;
        }
        let up = parent.get(p).copied().unwrap_or(p);
        if let Some(slot) = parent.get_mut(i) {
            *slot = up;
        }
        i = up;
    }
}

/// The cluster of `vertices` (in id order).
fn cluster(body: &Brep, vertices: Vec<VertexId>, res: Resolution) -> Option<Cluster> {
    let members: BTreeSet<VertexId> = vertices.iter().copied().collect();
    let mut lo = [f64::INFINITY; 3];
    let mut hi = [f64::NEG_INFINITY; 3];
    let mut survivor: Option<(VertexId, f64)> = None;
    let mut edges = BTreeSet::new();
    for &id in &vertices {
        let v = body.vertex(id)?;
        let p = v.point();
        for ((l, h), c) in lo.iter_mut().zip(hi.iter_mut()).zip([p.x, p.y, p.z]) {
            *l = l.min(c);
            *h = h.max(c);
        }
        let r = reach(v.accuracy(), res);
        if survivor.is_none_or(|(_, best)| r > best) {
            survivor = Some((id, r));
        }
        for e in v.edges() {
            let [a, b] = e.ends();
            if members.contains(&a) && members.contains(&b) {
                edges.insert(e.id());
            }
        }
    }
    let (survivor, r) = survivor?;
    let [x, y, z] = [0, 1, 2].map(|k| match (lo.get(k), hi.get(k)) {
        (Some(&l), Some(&h)) => l + 0.5 * (h - l),
        _ => f64::NAN,
    });
    Some(Cluster {
        vertices,
        edges: edges.into_iter().collect(),
        survivor,
        reach: Length::from_mm(r),
        target: Point3::new(x, y, z),
    })
}

/// The largest distance from `p` of `curve` over `interval`, enclosed over [`PIECES`]
/// pieces; `None` when it overflows.
fn enclosed_distance(curve: &Curve, interval: Extent1d, p: Point3) -> Option<Bracket> {
    let (a, b) = (interval.min(), interval.max());
    let n = f64::from(PIECES);
    let mut worst: Option<Bracket> = None;
    for k in 0..PIECES {
        let (s, t) = (f64::from(k) / n, f64::from(k + 1) / n);
        let lo = a + (b - a) * s;
        let hi = if k + 1 == PIECES { b } else { a + (b - a) * t };
        let piece = Ivl::new(lo.min(hi), lo.max(hi));
        let d = norm(curve.point_over(piece) - ivl3(p))?;
        if worst.is_none_or(|w| d.upper() > w.upper()) {
            worst = Some(d);
        }
    }
    worst
}

/// The pcurve of the line from `e0` along `dir` on `plane` (its projection over `[0, 1]`)
/// and the larger of its lift's gaps from the line at its two ends; `None` when it
/// overflows. The gap is affine in the parameter, so the larger end bounds it everywhere.
fn lift(plane: &Plane, e0: Point3, dir: Vec3) -> Option<(Line2, Bracket)> {
    let f = plane.frame();
    let (o, x, y) = (f.origin(), *f.x(), *f.y());
    let uv = |e: Point3| {
        let d = e - o;
        Point2::new(d.dot(x), d.dot(y))
    };
    let e1 = e0 + dir;
    let (uv0, uv1) = (uv(e0), uv(e1));
    let duv = uv1 - uv0;
    let pcurve = Line2::new(uv0, duv).ok()?;
    let near = Point2::new(0.0, 0.0);
    let lift_at = |t: Ivl| {
        let u = Ivl::from(uv0.x) + t * Ivl::from(duv.x);
        let v = Ivl::from(uv0.y) + t * Ivl::from(duv.y);
        let s = plane.eval(u, v, near);
        let c = ivl3(e0) + dir.map(|d| t * Ivl::from(d));
        norm(s - c)
    };
    let (g0, g1) = (lift_at(Ivl::from(0.0))?, lift_at(Ivl::from(1.0))?);
    Some((pcurve, if g1.upper() > g0.upper() { g1 } else { g0 }))
}

/// The target `p` (reach `rc`) against vertex `v` at `q` (reach `rv`): clear past the
/// larger reach.
fn clear(v: VertexId, q: Point3, rv: f64, p: Point3, rc: f64) -> Result<(), HealCause> {
    let allowed = rv.max(rc);
    let distance = distance(p, q).ok_or_else(|| HealCause::NotFinite { at: v.into() })?;
    if distance.lower() > allowed {
        return Ok(());
    }
    Err(HealCause::NotClear {
        vertex: v,
        distance,
        allowed: Length::from_mm(allowed),
    })
}

/// Points binned in cubes of side twice a reach, by index (as `scx-check`'s clearance
/// sweep bins them).
struct Grid {
    side: f64,
    /// `None`: one cube holds every point.
    cells: Option<BTreeMap<[i64; 3], Vec<usize>>>,
    all: Vec<usize>,
}

/// Past this many sides a coordinate's quotient is no longer an exact integer's neighbour.
const EXACT: f64 = 4_503_599_627_370_496.0;

impl Grid {
    fn new(points: impl Iterator<Item = Point3>, reach: f64) -> Self {
        let side = 2.0 * reach;
        let points: Vec<Point3> = points.collect();
        let all: Vec<usize> = (0..points.len()).collect();
        let mut cells: BTreeMap<[i64; 3], Vec<usize>> = BTreeMap::new();
        for (i, &p) in points.iter().enumerate() {
            let Some(key) = cell(p, side) else {
                return Self {
                    side,
                    cells: None,
                    all,
                };
            };
            cells.entry(key).or_default().push(i);
        }
        Self {
            side,
            cells: Some(cells),
            all,
        }
    }

    /// The indices in the cube of `p` and its 26 neighbours: every point within the reach.
    fn near(&self, p: Point3) -> impl Iterator<Item = usize> + '_ {
        let (keys, all): (Vec<[i64; 3]>, &[usize]) = match (&self.cells, cell(p, self.side)) {
            (Some(_), Some([x, y, z])) => {
                let mut keys = Vec::with_capacity(27);
                for dx in -1..=1 {
                    for dy in -1..=1 {
                        for dz in -1..=1 {
                            keys.push([x + dx, y + dy, z + dz]);
                        }
                    }
                }
                (keys, &[])
            }
            _ => (Vec::new(), &self.all),
        };
        let cells = self.cells.as_ref();
        all.iter().copied().chain(
            keys.into_iter()
                .filter_map(move |k| cells.and_then(|c| c.get(&k)))
                .flatten()
                .copied(),
        )
    }
}

/// The cube of `p`; `None` past [`EXACT`] sides or for a zero side.
fn cell(p: Point3, side: f64) -> Option<[i64; 3]> {
    if side <= 0.0 || !side.is_finite() {
        return None;
    }
    let k = |c: f64| {
        let q = (c / side).floor();
        #[expect(
            clippy::cast_possible_truncation,
            reason = "within 2^52, checked just before"
        )]
        (q.is_finite() && q.abs() < EXACT).then(|| q as i64)
    };
    Some([k(p.x)?, k(p.y)?, k(p.z)?])
}

/// A re-ended edge as certified: its new ends, its line and each face's pcurve.
#[derive(Debug, Clone)]
struct Rebuilt {
    ends: [VertexId; 2],
    line: Line,
    pcurves: BTreeMap<FaceId, Line2>,
}

/// What an item's certification found to do.
#[derive(Debug)]
struct Plan {
    rebuilt: BTreeMap<EdgeId, Rebuilt>,
    /// The item's squeezed uses, by face and loop index.
    squeezed_uses: BTreeMap<(FaceId, usize), usize>,
}

/// Squeezed uses counted by face and loop index.
type UseCounts = BTreeMap<(FaceId, usize), usize>;

/// The pieces a squeezed curve is enclosed over.
const PIECES: u32 = 16;

/// The items done so far.
struct State<'a> {
    body: &'a Brep,
    res: Resolution,
    /// Each vertex of a done cluster, to its survivor.
    merged: BTreeMap<VertexId, VertexId>,
    /// Each done cluster's survivor, to its target.
    targets: BTreeMap<VertexId, Point3>,
    squeezed: BTreeSet<EdgeId>,
    squeezed_uses: BTreeMap<(FaceId, usize), usize>,
    rebuilt: BTreeMap<EdgeId, Rebuilt>,
    /// Every vertex as `(id, point, reach)`, binned for the clearance test.
    points: Vec<(VertexId, Point3, f64)>,
    grid: Grid,
}

/// Charges `n` units, or returns `Ok(None)` from the enclosing certification.
macro_rules! charge {
    ($budget:expr, $n:expr) => {
        if $budget
            .charge(u64::try_from($n).unwrap_or(u64::MAX))
            .is_err()
        {
            return Ok(None);
        }
    };
}

impl<'a> State<'a> {
    fn new(body: &'a Brep) -> Self {
        let res = body.resolution();
        let points: Vec<(VertexId, Point3, f64)> = body
            .vertices()
            .map(|v| (v.id(), v.point(), reach(v.accuracy(), res)))
            .collect();
        let widest = points.iter().map(|v| v.2).fold(0.0, f64::max);
        let grid = Grid::new(points.iter().map(|v| v.1), widest);
        Self {
            body,
            res,
            merged: BTreeMap::new(),
            targets: BTreeMap::new(),
            squeezed: BTreeSet::new(),
            squeezed_uses: BTreeMap::new(),
            rebuilt: BTreeMap::new(),
            points,
            grid,
        }
    }

    /// The vertex `v` is now (its survivor, once merged) and where it is.
    fn now(&self, v: VertexId) -> Option<(VertexId, Point3)> {
        match self.merged.get(&v) {
            Some(&s) => Some((s, *self.targets.get(&s)?)),
            None => Some((v, self.body.vertex(v)?.point())),
        }
    }

    /// The reach of vertex `v`; a survivor keeps its own.
    fn vertex_reach(&self, v: VertexId) -> f64 {
        self.body
            .vertex(v)
            .map_or_else(|| self.res.zone().mm(), |x| reach(x.accuracy(), self.res))
    }

    /// Certifies `c` against the items done; `Ok(None)` when the budget runs out.
    fn certify(&self, c: &Cluster, budget: &mut Budget) -> Result<Option<Plan>, HealCause> {
        let members: BTreeSet<VertexId> = c.vertices.iter().copied().collect();
        let rc = c.reach.mm();
        // Every vertex within the survivor's reach of the target.
        charge!(budget, c.vertices.len());
        for &v in &c.vertices {
            let Some(x) = self.body.vertex(v) else {
                continue;
            };
            let d = distance(x.point(), c.target)
                .ok_or_else(|| HealCause::NotFinite { at: v.into() })?;
            if d.upper() > rc {
                return Err(HealCause::Spread {
                    vertex: v,
                    distance: d,
                    allowed: c.reach,
                });
            }
        }
        let Some(squeezed_uses) = self.squeezed(c, budget)? else {
            return Ok(None);
        };
        let Some((rebuilt, faces)) = self.re_end(c, &members, budget)? else {
            return Ok(None);
        };
        let mut faces = faces;
        faces.extend(squeezed_uses.keys().copied());
        if self
            .loops(c, &members, &faces, &squeezed_uses, budget)?
            .is_none()
        {
            return Ok(None);
        }
        if self.clearance(c, &members, budget)?.is_none() {
            return Ok(None);
        }
        Ok(Some(Plan {
            rebuilt,
            squeezed_uses,
        }))
    }

    /// The squeezed edges' uses by face and loop, each edge's curve within the survivor's
    /// reach of the target.
    fn squeezed(&self, c: &Cluster, budget: &mut Budget) -> Result<Option<UseCounts>, HealCause> {
        let mut uses: BTreeMap<(FaceId, usize), usize> = BTreeMap::new();
        for &e in &c.edges {
            charge!(budget, PIECES);
            let Some(edge) = self.body.edge(e) else {
                continue;
            };
            if let EdgeShape::Curve { curve, interval } = edge.shape() {
                let bound = enclosed_distance(curve, *interval, c.target)
                    .ok_or_else(|| HealCause::NotFinite { at: e.into() })?;
                if bound.upper() > c.reach.mm() {
                    return Err(HealCause::TooLong {
                        edge: e,
                        bound,
                        allowed: c.reach,
                    });
                }
            }
            for u in edge.uses() {
                *uses.entry((u.face().id(), u.lp().index())).or_default() += 1;
            }
        }
        Ok(Some(uses))
    }

    /// Each edge at the cluster and not squeezed, rebuilt between its new ends and
    /// certified; with the loops it is used in.
    #[expect(
        clippy::type_complexity,
        reason = "a private step's result: the rebuilt edges and their loops"
    )]
    fn re_end(
        &self,
        c: &Cluster,
        members: &BTreeSet<VertexId>,
        budget: &mut Budget,
    ) -> Result<Option<(BTreeMap<EdgeId, Rebuilt>, BTreeSet<(FaceId, usize)>)>, HealCause> {
        let squeezed: BTreeSet<EdgeId> = c.edges.iter().copied().collect();
        let mut rebuilt = BTreeMap::new();
        let mut faces = BTreeSet::new();
        for &v in &c.vertices {
            let Some(x) = self.body.vertex(v) else {
                continue;
            };
            for e in x.edges() {
                let id = e.id();
                if squeezed.contains(&id) || rebuilt.contains_key(&id) {
                    continue;
                }
                charge!(budget, 1 + e.uses().count());
                let EdgeShape::Curve {
                    curve: Curve::Line(_),
                    ..
                } = e.shape()
                else {
                    return Err(HealCause::NeighbourKind { edge: id });
                };
                let re = reach(e.accuracy(), self.res);
                let [s, t] = e.ends().map(|end| {
                    if members.contains(&end) {
                        Some((c.survivor, c.target))
                    } else {
                        self.now(end)
                    }
                });
                let (Some((v0, e0)), Some((v1, e1))) = (s, t) else {
                    return Err(HealCause::NeighbourKind { edge: id });
                };
                let dir = e1 - e0;
                let idir = dir.map(Ivl::from);
                let length = norm(idir).ok_or_else(|| HealCause::NotFinite { at: id.into() })?;
                if length.lower() <= re {
                    return Err(HealCause::NeighbourShort {
                        edge: id,
                        length,
                        allowed: Length::from_mm(re),
                    });
                }
                let line =
                    Line::new(e0, dir).map_err(|_| HealCause::NotFinite { at: id.into() })?;
                // The line's end at 1 against its vertex; at 0 it is the vertex's own point.
                let gap = norm(ivl3(e0) + idir - ivl3(e1))
                    .ok_or_else(|| HealCause::NotFinite { at: id.into() })?;
                let allowed = re + self.vertex_reach(v1);
                if gap.upper() > allowed {
                    return Err(HealCause::EndOffVertex {
                        edge: id,
                        gap,
                        allowed: Length::from_mm(allowed),
                    });
                }
                let mut pcurves = BTreeMap::new();
                for u in e.uses() {
                    let face = u.face();
                    let Surface::Plane(plane) = face.surface() else {
                        return Err(HealCause::FaceKind { face: face.id() });
                    };
                    if !matches!(u.geom(), UseGeom::Pcurve { .. }) {
                        return Err(HealCause::FaceKind { face: face.id() });
                    }
                    faces.insert((face.id(), u.lp().index()));
                    if pcurves.contains_key(&face.id()) {
                        continue;
                    }
                    let allowed = re.min(reach(u.bound(), self.res));
                    let (pcurve, gap) = lift(plane, e0, dir)
                        .ok_or_else(|| HealCause::NotFinite { at: id.into() })?;
                    if gap.upper() > allowed {
                        return Err(HealCause::OffFace {
                            edge: id,
                            face: face.id(),
                            gap,
                            allowed: Length::from_mm(allowed),
                        });
                    }
                    pcurves.insert(face.id(), pcurve);
                }
                rebuilt.insert(
                    id,
                    Rebuilt {
                        ends: [v0, v1],
                        line,
                        pcurves,
                    },
                );
            }
        }
        Ok(Some((rebuilt, faces)))
    }

    /// Each loop at the cluster keeps a use, three or more if they are all lines, and
    /// passes the survivor once.
    fn loops(
        &self,
        c: &Cluster,
        members: &BTreeSet<VertexId>,
        faces: &BTreeSet<(FaceId, usize)>,
        squeezed_uses: &BTreeMap<(FaceId, usize), usize>,
        budget: &mut Budget,
    ) -> Result<Option<()>, HealCause> {
        let squeezed: BTreeSet<EdgeId> = c.edges.iter().copied().collect();
        for &(f, l) in faces {
            let Some(lp) = self.body.face(f).and_then(|face| face.loops().nth(l)) else {
                continue;
            };
            charge!(budget, lp.len());
            let gone = squeezed_uses.get(&(f, l)).copied().unwrap_or(0)
                + self.squeezed_uses.get(&(f, l)).copied().unwrap_or(0);
            let kept: Vec<_> = lp
                .uses()
                .filter(|u| {
                    let e = u.edge().id();
                    !squeezed.contains(&e) && !self.squeezed.contains(&e)
                })
                .collect();
            if kept.is_empty() || gone >= lp.len() {
                return Err(HealCause::LoopVanishes { face: f });
            }
            let lines = kept.iter().all(|u| {
                matches!(
                    u.edge().shape(),
                    EdgeShape::Curve {
                        curve: Curve::Line(_),
                        ..
                    }
                )
            });
            if kept.len() < 3 && lines {
                return Err(HealCause::Sliver { face: f });
            }
            let at = kept
                .iter()
                .filter(|u| members.contains(&u.start().id()))
                .count();
            if at > 1 {
                return Err(HealCause::Pinch { face: f });
            }
        }
        Ok(Some(()))
    }

    /// The target clear of every vertex outside the cluster, as it now stands.
    fn clearance(
        &self,
        c: &Cluster,
        members: &BTreeSet<VertexId>,
        budget: &mut Budget,
    ) -> Result<Option<()>, HealCause> {
        let (p, rc) = (c.target, c.reach.mm());
        // The survivor's reach is a vertex's, so within the grid's: its cubes hold every
        // vertex the target could be within reach of.
        for j in self.grid.near(p) {
            let Some(&(v, q, rv)) = self.points.get(j) else {
                continue;
            };
            if members.contains(&v) || self.merged.contains_key(&v) {
                continue;
            }
            charge!(budget, 1);
            clear(v, q, rv, p, rc)?;
        }
        for (&v, &q) in &self.targets {
            charge!(budget, 1);
            clear(v, q, self.vertex_reach(v), p, rc)?;
        }
        Ok(Some(()))
    }

    fn accept(&mut self, c: &Cluster, plan: Plan) {
        for &v in &c.vertices {
            self.merged.insert(v, c.survivor);
        }
        self.targets.insert(c.survivor, c.target);
        self.squeezed.extend(c.edges.iter().copied());
        for (k, n) in plan.squeezed_uses {
            *self.squeezed_uses.entry(k).or_default() += n;
        }
        self.rebuilt.extend(plan.rebuilt);
    }

    /// The body with the done items' edits, finished.
    fn apply(self, doc: scx_brep::Document) -> Result<(Brep, Changes), RepairError> {
        let defect = || RepairError::Defect(RepairDefect::Draft);
        let interval = unit().ok_or_else(defect)?;
        let mut d = self.body.edit();
        for (&v, &p) in &self.targets {
            d.vertex_mut(v).ok_or_else(defect)?.point = p;
        }
        let mut faces = BTreeSet::new();
        for &e in self.squeezed.iter().chain(self.rebuilt.keys()) {
            let edge = self.body.edge(e).ok_or_else(defect)?;
            faces.extend(edge.uses().map(|u| u.face().id()));
        }
        for &f in &faces {
            let (squeezed, rebuilt) = (&self.squeezed, &self.rebuilt);
            d.edit_face(f, |rec| {
                for lp in &mut rec.loops {
                    lp.uses.retain(|u| !squeezed.contains(&u.edge));
                    for u in &mut lp.uses {
                        if let Some(pc) = rebuilt.get(&u.edge).and_then(|r| r.pcurves.get(&f)) {
                            u.geom = UseGeom::Pcurve {
                                curve: Curve2::Line(*pc),
                                interval,
                            };
                        }
                    }
                }
            })
            .ok_or_else(defect)?;
        }
        for (&e, r) in &self.rebuilt {
            let g = d.edge_mut(e).ok_or_else(defect)?;
            g.shape = EdgeShape::Curve {
                curve: Curve::Line(r.line),
                interval,
            };
            g.ends = r.ends;
        }
        for &e in &self.squeezed {
            d.remove_edge(e).ok_or_else(defect)?;
        }
        for (&v, &s) in &self.merged {
            if v != s {
                d.merge_vertex(v, s).ok_or_else(defect)?;
            }
        }
        d.finish(&doc)
            .map_err(|u| RepairError::Defect(RepairDefect::Finish(Box::new(u.cause))))
    }
}

/// The unit interval every rebuilt edge and pcurve runs over.
fn unit() -> Option<Extent1d> {
    Extent1d::new(0.0, 1.0)
}
