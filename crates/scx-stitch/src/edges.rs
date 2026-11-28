// Written for solidcorex. Apache-2.0.
//! The edge stage (74 §4.3 steps 2 and 3): each sheet edge cut at the other sheets' vertices
//! that lie within coincidence of its inside, then each pair of edges of two sheets with the
//! same ends joined where the certified symmetric Hausdorff distance of their curves is
//! within coincidence. Every distance is a [`Bracket`] from `scx_surface`'s gaps (SU6), so
//! a pair is proved within, proved beyond, or left [`Left::Undecided`]: never sampled.
//!
//! | solidcore at the pin (`heal/stitch`) | here |
//! |---|---|
//! | `bounds.rs`: `curves`, both directed gaps under an endpoint correspondence (64 cells of a centred mean-value form) | two [`curve_curve_gap`] calls, each the one-sided Hausdorff distance with no assumed correspondence, aimed [`Aim::Against`] the coincidence |
//! | `bounds.rs`: `curve_to_point`, `point_at_curve` | [`point_curve_gap`] for a vertex against an edge's inside |
//! | p2 split pass: a box tree of edges, each vertex's box grown by the tolerance, up to 10 loops | one [`BoxTree`] of edge boxes, one pass: a cut point is an existing vertex, so a cut makes no new point to cut at |
//! | p2 `glue_edges_at_vertex`: greedy per vertex, a 5-sample distance (0 for two lines) | candidates from identified ends and box overlap, accepted from a heap in (bound, ids) order; an edge with two possible partners is left open ([`Left::Ambiguous`]) |
//! | `MAX_PIECES`-style caps | a budget share per edge; a pair or touch left at it is [`Left::Undecided`] |

use core::cmp::{Ordering, Reverse};
use std::collections::{BTreeMap, BinaryHeap};

use scx_algo::box_tree::BoxTree;
use scx_brep::op::{Raise, Warning, WarningKind};
use scx_brep::{
    Document, EdgeGeom, EdgeId, EdgeShape, Entity, FaceId, Operand, Origin, PcurveWriteError,
    RegionId, ShellId, UseGeom, VertexId,
};
use scx_curve::Curve;
use scx_num::budget::{Budget, Stop};
use scx_num::extent1d::Extent1d;
use scx_num::extent3d::Extent3d;
use scx_num::invert::Inversion;
use scx_num::units::{Accuracy, Length};
use scx_num::vector3d::Point3;
use scx_nurbs::Approach;
use scx_surface::{Aim, Bracket, GapError, Short, curve_curve_gap, point_curve_gap};

use crate::error::{EdgePending, StitchDefect, StitchError, StitchUnsupported, too_large};
use crate::report::{Joined, Left, Split};
use crate::stitch::{Build, measured};

/// Work charged per box candidate.
const UNIT: u64 = 1;

/// A sheet edge with a curve.
struct SheetEdge<'a> {
    sheet: usize,
    /// The sheet's id.
    id: EdgeId,
    /// The draft's id.
    at: EdgeId,
    curve: &'a Curve,
    interval: Extent1d,
    reach: f64,
    /// The draft's vertices, start then end.
    ends: [VertexId; 2],
    fins: usize,
    bx: Extent3d,
}

impl SheetEdge<'_> {
    fn operand(&self) -> Operand {
        Operand::from_raw(self.sheet as u32)
    }

    fn pending(&self, why: EdgePending) -> StitchError {
        StitchError::Unsupported(StitchUnsupported::Edges {
            operand: self.operand(),
            edge: self.id,
            why,
        })
    }
}

/// A vertex of the draft, as a cut point.
struct Node {
    point: Point3,
    reach: f64,
    /// The sheets it stands for a vertex of, ascending.
    sheets: Vec<usize>,
}

/// A cut of an edge at a vertex.
struct Cut {
    at: f64,
    vertex: VertexId,
    gap: f64,
    /// How far the cut's curve and pcurve ends lie from the vertex.
    far: f64,
}

/// A piece of a sheet edge: the whole edge, or the part between two cuts.
struct Piece {
    edge: usize,
    interval: Extent1d,
    ends: [VertexId; 2],
    /// The draft's edge, once the cuts are made.
    at: EdgeId,
}

/// What a gap test proved.
#[derive(Clone, Copy)]
enum Test {
    /// Within the allowance, the gap at most this.
    Within(f64),
    Beyond,
    /// Straddling the allowance where the search ended: (lower, upper).
    Open(f64, f64),
}

/// The test a gap search gives against `allowance`.
fn test(
    r: Result<Bracket, GapError>,
    allowance: f64,
    operand: Operand,
    at: Entity,
) -> Result<Test, StitchError> {
    match r {
        Ok(b) => Ok(match b.exceeds(allowance) {
            Some(false) => Test::Within(b.upper()),
            Some(true) => Test::Beyond,
            None => Test::Open(b.lower(), b.upper()),
        }),
        Err(GapError::Undecided {
            why: Short::Stopped,
            ..
        }) => Err(StitchError::Cancelled),
        Err(GapError::Undecided { reached, .. }) => Ok(match reached.exceeds(allowance) {
            Some(true) => Test::Beyond,
            Some(false) => Test::Within(reached.upper()),
            None => Test::Open(reached.lower(), reached.upper()),
        }),
        Err(_) => Err(StitchError::Defect(StitchDefect::Query { operand, at })),
    }
}

/// Runs `f` on the edge's share and charges the main budget what it used.
/// The test of the larger of two gaps: beyond if either is, else within if both are, else
/// open between the larger of each bound.
fn larger(x: Test, y: Test) -> Test {
    match (x, y) {
        (_, Test::Beyond) | (Test::Beyond, _) => Test::Beyond,
        (Test::Within(a), Test::Within(b)) => Test::Within(a.max(b)),
        (Test::Open(a, b), Test::Open(c, d)) => Test::Open(a.max(c), b.max(d)),
        (Test::Open(a, b), Test::Within(c)) | (Test::Within(c), Test::Open(a, b)) => {
            Test::Open(a, b.max(c))
        }
    }
}

fn on_share<T>(
    share: &mut Budget,
    budget: &mut Budget,
    f: impl FnOnce(&mut Budget) -> T,
) -> Result<T, StitchError> {
    let before = share.used();
    let out = f(share);
    budget.charge(share.used().saturating_sub(before))?;
    Ok(out)
}

/// The share each of `n` edges gets of what is left of `budget`.
fn share(budget: &Budget, n: usize) -> u64 {
    budget.remaining() / (n.max(1) as u64)
}

fn mm(x: f64) -> Length {
    Length::from_mm(x)
}

/// What the edge stage did: the report's parts.
pub(crate) struct Edges {
    pub(crate) split: Vec<Split>,
    pub(crate) joined: Vec<Joined>,
    pub(crate) left: Vec<Left>,
}

impl Build<'_> {
    /// The edge stage over the built draft: cuts, then joins. Raises a cut vertex's accuracy
    /// to the cut's farthest end where that is beyond its reach, into `warnings` and
    /// `raised`.
    pub(crate) fn edges(
        &mut self,
        zone: f64,
        doc: Document,
        budget: &mut Budget,
        stop: &Stop,
        warnings: &mut Vec<Warning>,
        raised: &mut Vec<Raise>,
    ) -> Result<Edges, StitchError> {
        let sheets = self.sheets;
        let reach =
            |a: Option<&scx_num::cert::Recorded>| a.map_or(zone, |r| r.acc.length().mm().max(zone));
        let mut edges = Vec::new();
        for (i, s) in sheets.iter().enumerate() {
            let operand = Operand::from_raw(i as u32);
            for e in s.edges() {
                budget.charge(UNIT)?;
                let EdgeShape::Curve { curve, interval } = &e.geometry().shape else {
                    continue;
                };
                let at = Entity::Edge(e.id());
                let bx = e
                    .bounding_box()
                    .ok_or(StitchError::Defect(StitchDefect::Query { operand, at }))?;
                let [a, b] = e.ends().map(|v| self.vertex_of(i, v));
                edges.push(SheetEdge {
                    sheet: i,
                    id: e.id(),
                    at: self.edge_of(i, e.id())?,
                    curve,
                    interval: *interval,
                    reach: reach(e.accuracy()),
                    ends: [a?, b?],
                    fins: e.fins().len(),
                    bx,
                });
            }
        }
        let nodes = self.nodes(zone)?;
        let (cuts, mut left) = self.cuts(&edges, &nodes, budget, stop)?;
        let (pieces, split) = self.cut(&edges, &cuts, zone, warnings, raised)?;
        let (joined, more) = self.join(&edges, &pieces, doc, budget, stop, warnings)?;
        left.extend(more);
        left.sort_by(|a, b| {
            a.edge().cmp(&b.edge()).then_with(|| match (a, b) {
                (Left::Undecided { with: x, .. }, Left::Undecided { with: y, .. }) => x.cmp(y),
                _ => Ordering::Equal,
            })
        });
        Ok(Edges {
            split,
            joined,
            left,
        })
    }

    /// Each draft vertex as a cut point: its point, reach and sheets.
    fn nodes(&self, zone: f64) -> Result<BTreeMap<VertexId, Node>, StitchError> {
        let mut nodes: BTreeMap<VertexId, Node> = BTreeMap::new();
        for (k, it) in self.ix.items.iter().enumerate() {
            let v = self
                .at
                .get(k)
                .copied()
                .flatten()
                .ok_or(StitchError::Defect(StitchDefect::Draft))?;
            let sheet = it.operand.index();
            if let Some(n) = nodes.get_mut(&v) {
                if !n.sheets.contains(&sheet) {
                    n.sheets.push(sheet);
                    n.sheets.sort_unstable();
                }
                continue;
            }
            let rec = self
                .draft
                .vertex(v)
                .ok_or(StitchError::Defect(StitchDefect::Draft))?;
            nodes.insert(
                v,
                Node {
                    point: rec.point(),
                    reach: rec
                        .accuracy()
                        .map_or(zone, |r| r.acc.length().mm().max(zone)),
                    sheets: vec![sheet],
                },
            );
        }
        Ok(nodes)
    }

    /// The cuts of each edge: a vertex of a sheet the edge's is not, within coincidence of
    /// the edge's inside, found through one box tree of the edges and proved by
    /// [`point_curve_gap`]. A vertex the gap leaves open is [`Left::Undecided`].
    fn cuts(
        &self,
        edges: &[SheetEdge<'_>],
        nodes: &BTreeMap<VertexId, Node>,
        budget: &mut Budget,
        stop: &Stop,
    ) -> Result<(Vec<Vec<Cut>>, Vec<Left>), StitchError> {
        let boxes: Vec<Extent3d> = edges.iter().map(|e| e.bx).collect();
        let tree = BoxTree::new(&boxes);
        let mut near: Vec<Vec<VertexId>> = edges.iter().map(|_| Vec::new()).collect();
        let mut tested = 0usize;
        for (&v, node) in nodes {
            stop.check()?;
            let q = Extent3d::from_point(node.point).ok_or_else(|| {
                StitchError::Defect(StitchDefect::Query {
                    operand: Operand::from_raw(node.sheets.first().copied().unwrap_or(0) as u32),
                    at: Entity::Vertex(v),
                })
            })?;
            // An edge's box holds every point within its reach; a vertex reaching farther
            // grows the query by its own.
            for k in tree.overlapping(&q, node.reach) {
                budget.charge(UNIT)?;
                let (Some(e), Some(slot)) = (edges.get(k), near.get_mut(k)) else {
                    continue;
                };
                if e.ends.contains(&v) || node.sheets.contains(&e.sheet) {
                    continue;
                }
                if slot.is_empty() {
                    tested += 1;
                }
                slot.push(v);
            }
        }
        // Half of what is left goes to the cuts, so the joins after them keep a share.
        let each = share(budget, tested.saturating_mul(2));
        let mut cuts: Vec<Vec<Cut>> = edges.iter().map(|_| Vec::new()).collect();
        let mut left = Vec::new();
        for ((e, vs), out) in edges.iter().zip(&near).zip(&mut cuts) {
            if vs.is_empty() {
                continue;
            }
            stop.check()?;
            let mut mine = Budget::new(each);
            for v in vs {
                let node = nodes
                    .get(v)
                    .ok_or(StitchError::Defect(StitchDefect::Draft))?;
                let allowance = node.reach.max(e.reach);
                let r = on_share(&mut mine, budget, |b| {
                    point_curve_gap(
                        node.point,
                        e.curve,
                        &e.interval,
                        Aim::Against(allowance),
                        b,
                        stop,
                    )
                })?;
                match test(r, allowance, e.operand(), Entity::Edge(e.id))? {
                    Test::Beyond => {}
                    Test::Open(lower, upper) => left.push(Left::Undecided {
                        edge: e.at,
                        with: Entity::Vertex(*v),
                        lower: mm(lower),
                        upper: mm(upper),
                        allowance: mm(allowance),
                    }),
                    Test::Within(gap) => {
                        if e.fins == 0 {
                            return Err(e.pending(EdgePending::Wire));
                        }
                        let foot = match e.curve.invert(node.point, Some(&e.interval)) {
                            Ok(Inversion::Inside(f)) => f,
                            Ok(_) => return Err(e.pending(EdgePending::NearEnd)),
                            Err(_) => return Err(e.pending(EdgePending::NurbsFoot)),
                        };
                        let (lo, hi) = (e.interval.min(), e.interval.max());
                        if !(foot.at > lo && foot.at < hi) {
                            return Err(e.pending(EdgePending::NearEnd));
                        }
                        out.push(Cut {
                            at: foot.at,
                            vertex: *v,
                            gap,
                            far: self.far(e, foot.at, node.point)?,
                        });
                    }
                }
            }
        }
        Ok((cuts, left))
    }

    /// How far from `p` a cut at `t` puts the edge's curve end and each use's surface point.
    fn far(&self, e: &SheetEdge<'_>, t: f64, p: Point3) -> Result<f64, StitchError> {
        let edge = self
            .sheet(e.sheet)?
            .edge(e.id)
            .ok_or(StitchError::Defect(StitchDefect::Draft))?;
        let mut far = e.curve.point(t).distance(p);
        for u in edge.uses() {
            let at = match u.geom() {
                UseGeom::Pcurve { curve, .. } => u.face().surface().point(curve.point(t)),
                UseGeom::Point(uv) => u.face().surface().point(*uv),
            };
            far = far.max(at.distance(p));
        }
        Ok(far)
    }

    /// Makes the cuts in the draft: each cut edge keeps its id for its first piece and
    /// gains a new edge, derived from it, for each other; each face's use of it becomes
    /// the pieces' uses in its sense, the pcurve cut at the same parameters. Gives every
    /// piece of every edge, and the report's splits.
    fn cut(
        &mut self,
        edges: &[SheetEdge<'_>],
        cuts: &[Vec<Cut>],
        zone: f64,
        warnings: &mut Vec<Warning>,
        raised: &mut Vec<Raise>,
    ) -> Result<(Vec<Piece>, Vec<Split>), StitchError> {
        let mut pieces = Vec::new();
        let mut split = Vec::new();
        let mut raise: BTreeMap<VertexId, f64> = BTreeMap::new();
        for (k, (e, cs)) in edges.iter().zip(cuts).enumerate() {
            if cs.is_empty() {
                pieces.push(Piece {
                    edge: k,
                    interval: e.interval,
                    ends: e.ends,
                    at: e.at,
                });
                continue;
            }
            let mut cs: Vec<&Cut> = cs.iter().collect();
            cs.sort_by(|a, b| a.at.total_cmp(&b.at).then(a.vertex.cmp(&b.vertex)));
            let mut ts = vec![e.interval.min()];
            ts.extend(cs.iter().map(|c| c.at));
            ts.push(e.interval.max());
            let mut ends = vec![e.ends[0]];
            ends.extend(cs.iter().map(|c| c.vertex));
            ends.push(e.ends[1]);
            let mut ids = Vec::with_capacity(ts.len() - 1);
            for (j, w) in ts.windows(2).enumerate() {
                let (&[a, b], (Some(&v0), Some(&v1))) = (w, (ends.get(j), ends.get(j + 1))) else {
                    return Err(StitchError::Defect(StitchDefect::Draft));
                };
                let interval = Extent1d::new(a, b)
                    .filter(|_| a < b)
                    .ok_or_else(|| e.pending(EdgePending::SameFoot))?;
                let id = if j == 0 {
                    let g = self
                        .draft
                        .edge_mut(e.at)
                        .ok_or(StitchError::Defect(StitchDefect::Draft))?;
                    g.shape = EdgeShape::Curve {
                        curve: e.curve.clone(),
                        interval,
                    };
                    g.ends[1] = v1;
                    e.at
                } else {
                    let before = self
                        .draft
                        .edge(e.at)
                        .ok_or(StitchError::Defect(StitchDefect::Draft))?
                        .accuracy()
                        .copied();
                    let mut g = EdgeGeom::new(
                        EdgeShape::Curve {
                            curve: e.curve.clone(),
                            interval,
                        },
                        [v0, v1],
                    );
                    g.accuracy = before.map(std::sync::Arc::new);
                    let id = self.draft.add_edge(g).map_err(too_large)?;
                    self.draft
                        .derive(id, Origin::Edge(e.at))
                        .map_err(crate::error::draft_defect)?;
                    id
                };
                ids.push(id);
                pieces.push(Piece {
                    edge: k,
                    interval,
                    ends: [v0, v1],
                    at: id,
                });
            }
            self.cut_uses(e.at, &ids, &ts)?;
            for (c, id) in cs.iter().zip(&ids) {
                split.push(Split {
                    edge: *id,
                    vertex: c.vertex,
                    from: (e.operand(), e.id),
                    gap: mm(c.gap),
                });
                let f = raise.entry(c.vertex).or_insert(0.0);
                *f = f.max(c.far);
            }
        }
        for (v, far) in raise {
            self.raise_vertex(v, far, zone, warnings, raised)?;
        }
        split.sort_by_key(|s| (s.edge, s.vertex));
        Ok((pieces, split))
    }

    /// Each face's use of `edge` replaced by the uses of its pieces `ids` (over `ts`), in
    /// the use's sense; the pieces take the edge's radial order.
    fn cut_uses(&mut self, edge: EdgeId, ids: &[EdgeId], ts: &[f64]) -> Result<(), StitchError> {
        let fins: Vec<_> = self
            .draft
            .edge(edge)
            .ok_or(StitchError::Defect(StitchDefect::Draft))?
            .fins()
            .collect();
        let mut faces: Vec<FaceId> = fins.iter().map(|f| f.face).collect();
        faces.sort_unstable();
        faces.dedup();
        let spans: Vec<Extent1d> = ts
            .windows(2)
            .filter_map(|w| match w {
                &[a, b] => Extent1d::new(a, b),
                _ => None,
            })
            .collect();
        for f in faces {
            self.draft
                .edit_face(f, |rec| {
                    for lp in &mut rec.loops {
                        let mut uses = Vec::with_capacity(lp.uses.len() + ids.len());
                        for u in lp.uses.drain(..) {
                            if u.edge != edge {
                                uses.push(u);
                                continue;
                            }
                            let mut parts: Vec<_> = ids
                                .iter()
                                .zip(&spans)
                                .map(|(&id, &span)| {
                                    let mut w = u.clone();
                                    w.edge = id;
                                    if let UseGeom::Pcurve { interval, .. } = &mut w.geom {
                                        *interval = span;
                                    }
                                    w
                                })
                                .collect();
                            if u.sense == scx_num::frame::Sense::Reversed {
                                parts.reverse();
                            }
                            uses.extend(parts);
                        }
                        lp.uses = uses;
                    }
                })
                .ok_or(StitchError::Defect(StitchDefect::Draft))?;
        }
        for &id in ids.get(1..).unwrap_or_default() {
            self.draft
                .adjacency_mut(id)
                .ok_or(StitchError::Defect(StitchDefect::Draft))?
                .fins
                .clone_from(&fins);
        }
        Ok(())
    }

    /// Raises the draft vertex `v` to `far`, `Measured`, where that is beyond its reach;
    /// a raise the identification made is grown in place.
    fn raise_vertex(
        &mut self,
        v: VertexId,
        far: f64,
        zone: f64,
        warnings: &mut Vec<Warning>,
        raised: &mut Vec<Raise>,
    ) -> Result<(), StitchError> {
        let rec = self
            .draft
            .vertex_mut(v)
            .ok_or(StitchError::Defect(StitchDefect::Draft))?;
        let before = rec.accuracy.as_ref().map(|r| r.acc);
        let reach = before.map_or(zone, |a| a.length().mm().max(zone));
        if far <= reach {
            return Ok(());
        }
        let after = measured(far).ok_or(StitchError::Defect(StitchDefect::Draft))?;
        let acc = after.acc;
        rec.accuracy = Some(after);
        let entity = Entity::Vertex(v);
        if let Some(r) = raised.iter_mut().find(|r| r.entity == entity) {
            r.after = acc;
        } else {
            raised.push(Raise {
                entity,
                before: before.unwrap_or(Accuracy::ZERO),
                after: acc,
            });
            warnings.push(Warning {
                entity,
                kind: WarningKind::AccuracyRaised,
            });
        }
        Ok(())
    }

    /// Joins pieces of two sheets with the same ends: candidates from the ends and box
    /// overlap, each proved by both directed [`curve_curve_gap`]s on the lead piece's
    /// budget share, accepted from a heap in (bound, ids) order where neither piece has
    /// another possible partner.
    fn join(
        &mut self,
        edges: &[SheetEdge<'_>],
        pieces: &[Piece],
        doc: Document,
        budget: &mut Budget,
        stop: &Stop,
        warnings: &mut Vec<Warning>,
    ) -> Result<(Vec<Joined>, Vec<Left>), StitchError> {
        let edge = |p: &Piece| {
            edges
                .get(p.edge)
                .ok_or(StitchError::Defect(StitchDefect::Draft))
        };
        let mut by_ends: BTreeMap<[VertexId; 2], Vec<usize>> = BTreeMap::new();
        for (k, p) in pieces.iter().enumerate() {
            let [a, b] = p.ends;
            by_ends.entry([a.min(b), a.max(b)]).or_default().push(k);
        }
        // Candidate pairs, by lead piece.
        let mut lead: BTreeMap<usize, Vec<usize>> = BTreeMap::new();
        for ks in by_ends.values() {
            for (x, &i) in ks.iter().enumerate() {
                for &j in ks.get(x + 1..).unwrap_or_default() {
                    budget.charge(UNIT)?;
                    let (Some(pi), Some(pj)) = (pieces.get(i), pieces.get(j)) else {
                        continue;
                    };
                    let (ei, ej) = (edge(pi)?, edge(pj)?);
                    if ei.sheet == ej.sheet || ei.bx.are_disjoint(&ej.bx, ei.reach.max(ej.reach)) {
                        continue;
                    }
                    lead.entry(i).or_default().push(j);
                }
            }
        }
        let each = share(budget, lead.len());
        let mut tests: Vec<(usize, usize, Test, f64)> = Vec::new();
        for (&i, js) in &lead {
            stop.check()?;
            let mut mine = Budget::new(each);
            for &j in js {
                let (Some(pi), Some(pj)) = (pieces.get(i), pieces.get(j)) else {
                    continue;
                };
                let (ei, ej) = (edge(pi)?, edge(pj)?);
                let allowance = ei.reach.max(ej.reach);
                let aim = Aim::Against(allowance);
                // Each directed gap in turn, the symmetric one the larger: past the first
                // proved beyond, the rest cannot change it.
                let mut both = Test::Within(0.0);
                if let (Curve::Line(_), Curve::Line(_)) = (ei.curve, ej.curve) {
                    // A point's distance from a segment is convex along another segment,
                    // so each directed gap of two segments is at one of the ends: four
                    // point gaps are the whole symmetric gap.
                    let ends = |e: &SheetEdge<'_>, iv: &Extent1d| {
                        [e.curve.point(iv.min()), e.curve.point(iv.max())]
                    };
                    let probes = ends(ei, &pi.interval)
                        .map(|p| (p, ej, pj))
                        .into_iter()
                        .chain(ends(ej, &pj.interval).map(|p| (p, ei, pi)));
                    for (p, e, piece) in probes {
                        if matches!(both, Test::Beyond) {
                            break;
                        }
                        let r = on_share(&mut mine, budget, |m| {
                            point_curve_gap(p, e.curve, &piece.interval, aim, m, stop)
                        })?;
                        both = larger(both, test(r, allowance, e.operand(), Entity::Edge(e.id))?);
                    }
                } else {
                    for (a, ia, b, ib) in [(ei, pi, ej, pj), (ej, pj, ei, pi)] {
                        if matches!(both, Test::Beyond) {
                            break;
                        }
                        let r = on_share(&mut mine, budget, |m| {
                            curve_curve_gap(
                                a.curve,
                                &ia.interval,
                                b.curve,
                                &ib.interval,
                                aim,
                                m,
                                stop,
                            )
                        })?;
                        both = larger(both, test(r, allowance, a.operand(), Entity::Edge(a.id))?);
                    }
                }
                tests.push((i, j, both, allowance));
            }
        }
        // Each piece's possible partners: proved within, or open.
        let mut maybe: BTreeMap<usize, Vec<usize>> = BTreeMap::new();
        for &(i, j, t, _) in &tests {
            if !matches!(t, Test::Beyond) {
                maybe.entry(i).or_default().push(j);
                maybe.entry(j).or_default().push(i);
            }
        }
        let alone = |k: usize| maybe.get(&k).is_some_and(|v| v.len() == 1);
        let mut left = Vec::new();
        let mut heap = BinaryHeap::new();
        for &(i, j, t, allowance) in &tests {
            match t {
                Test::Within(gap) if alone(i) && alone(j) => {
                    heap.push(Reverse((order_key(gap), i, j)));
                }
                Test::Beyond | Test::Within(_) => {}
                Test::Open(lower, upper) => {
                    let (Some(pi), Some(pj)) = (pieces.get(i), pieces.get(j)) else {
                        continue;
                    };
                    left.push(Left::Undecided {
                        edge: pi.at.min(pj.at),
                        with: Entity::Edge(pi.at.max(pj.at)),
                        lower: mm(lower),
                        upper: mm(upper),
                        allowance: mm(allowance),
                    });
                }
            }
        }
        for (&k, partners) in &maybe {
            if partners.len() > 1 {
                let at = |k: &usize| pieces.get(*k).map(|p| p.at);
                let mut partners: Vec<EdgeId> = partners.iter().filter_map(at).collect();
                partners.sort_unstable();
                left.push(Left::Ambiguous {
                    edge: at(&k).ok_or(StitchError::Defect(StitchDefect::Draft))?,
                    partners,
                });
            }
        }
        let mut joined = Vec::new();
        let mut shells = Shells::default();
        while let Some(Reverse((key, i, j))) = heap.pop() {
            stop.check()?;
            let (Some(pi), Some(pj)) = (pieces.get(i), pieces.get(j)) else {
                return Err(StitchError::Defect(StitchDefect::Draft));
            };
            let (ei, ej) = (edge(pi)?, edge(pj)?);
            let (keep, gone) = if keep_order((ei, pi), (ej, pj)) == Ordering::Greater {
                ((ej, pj), (ei, pi))
            } else {
                ((ei, pi), (ej, pj))
            };
            let (s, sides) = self.glue(keep, gone, doc)?;
            shells.join(sides);
            warnings.push(Warning {
                entity: Entity::Edge(s),
                kind: WarningKind::Coincident,
            });
            joined.push(Joined {
                edge: s,
                gap: mm(f64::from_bits(key)),
            });
        }
        self.merge_shells(&shells)?;
        joined.sort_by_key(|j| j.edge);
        Ok((joined, left))
    }

    /// Every face's sides and every wire and lone vertex moved to its shell's class's
    /// least shell, and the others removed: the joined sheets are one shell each.
    fn merge_shells(&mut self, shells: &Shells) -> Result<(), StitchError> {
        if shells.up.is_empty() {
            return Ok(());
        }
        let faces: Vec<FaceId> = self.draft.faces().map(scx_brep::Face::id).collect();
        for f in faces {
            let sides = self
                .draft
                .face(f)
                .ok_or(StitchError::Defect(StitchDefect::Draft))?
                .record()
                .sides;
            let to = sides.map(|s| shells.root(s));
            if to != sides {
                self.draft
                    .face_mut(f)
                    .ok_or(StitchError::Defect(StitchDefect::Draft))?
                    .sides = to;
            }
        }
        for &gone in shells.up.keys() {
            let root = shells.root(gone);
            if root == gone {
                continue;
            }
            let rec = self
                .draft
                .remove_shell(gone)
                .ok_or(StitchError::Defect(StitchDefect::Draft))?;
            let keep = self
                .draft
                .shell_mut(root)
                .ok_or(StitchError::Defect(StitchDefect::Draft))?;
            keep.wires.extend(rec.wires);
            keep.lone_vertices.extend(rec.lone_vertices);
        }
        Ok(())
    }

    /// Joins the piece `gone` onto `keep`: the faces of `gone` take `keep` in its sense,
    /// with pcurves written from its curve, and `gone` is merged into it.
    fn glue(
        &mut self,
        (ek, pk): (&SheetEdge<'_>, &Piece),
        (eg, pg): (&SheetEdge<'_>, &Piece),
        doc: Document,
    ) -> Result<(EdgeId, [ShellId; 2]), StitchError> {
        let (s, a) = (pk.at, pg.at);
        let fins = |d: &scx_brep::Draft, e: EdgeId| {
            d.edge(e)
                .map(|e| e.fins().collect::<Vec<_>>())
                .ok_or(StitchError::Defect(StitchDefect::Draft))
        };
        let (fs, fa) = (fins(&self.draft, s)?, fins(&self.draft, a)?);
        if fs.is_empty() {
            return Err(ek.pending(EdgePending::Wire));
        }
        if fa.is_empty() {
            return Err(eg.pending(EdgePending::Wire));
        }
        if fs.len() + fa.len() > 2 {
            return Err(ek.pending(EdgePending::Radial));
        }
        // A face of one fin has one shell on both sides (`finish`'s sides rule).
        let shell = |d: &scx_brep::Draft, f: FaceId| -> Result<(ShellId, RegionId), StitchError> {
            let s = d
                .face(f)
                .ok_or(StitchError::Defect(StitchDefect::Draft))?
                .record()
                .sides[0];
            let r = d
                .shell(s)
                .ok_or(StitchError::Defect(StitchDefect::Draft))?
                .record()
                .region;
            Ok((s, r))
        };
        let (Some(f0), Some(f1)) = (fs.first(), fa.first()) else {
            return Err(StitchError::Defect(StitchDefect::Draft));
        };
        let ((s0, r0), (s1, r1)) = (shell(&self.draft, f0.face)?, shell(&self.draft, f1.face)?);
        if r0 != r1 {
            return Err(ek.pending(EdgePending::Region));
        }
        let reversed = if pk.ends[0] == pk.ends[1] {
            let dir = |e: &SheetEdge<'_>, p: &Piece| {
                e.curve.tangent_dir(p.interval.min(), Approach::Above)
            };
            match (dir(ek, pk), dir(eg, pg)) {
                (Some(x), Some(y)) if x.dot(*y) > 0.5 => false,
                (Some(x), Some(y)) if x.dot(*y) < -0.5 => true,
                _ => return Err(ek.pending(EdgePending::Direction)),
            }
        } else {
            pk.ends[0] != pg.ends[0]
        };
        let mut faces: Vec<FaceId> = fa.iter().map(|f| f.face).collect();
        faces.sort_unstable();
        faces.dedup();
        for &f in &faces {
            self.draft
                .edit_face(f, |rec| {
                    for u in rec.loops.iter_mut().flat_map(|l| l.uses.iter_mut()) {
                        if u.edge == a {
                            u.edge = s;
                            if reversed {
                                u.sense = u.sense.reversed();
                            }
                        }
                    }
                })
                .ok_or(StitchError::Defect(StitchDefect::Draft))?;
        }
        self.draft
            .merge_edge(a, s)
            .ok_or(StitchError::Defect(StitchDefect::Draft))?;
        for f in faces {
            self.draft
                .write_pcurves(f, &doc)
                .map_err(|cause| match cause {
                    PcurveWriteError::NeedsFit { .. } | PcurveWriteError::OffSurface { .. } => {
                        ek.pending(EdgePending::Fit)
                    }
                    cause => StitchError::Defect(StitchDefect::Pcurve { face: f, cause }),
                })?;
        }
        Ok((s, [s0, s1]))
    }
}

/// The shells joins put together: a union-find, each class led by its least shell.
#[derive(Default)]
struct Shells {
    up: BTreeMap<ShellId, ShellId>,
}

impl Shells {
    fn root(&self, mut s: ShellId) -> ShellId {
        while let Some(&p) = self.up.get(&s) {
            if p == s {
                break;
            }
            s = p;
        }
        s
    }

    fn join(&mut self, [a, b]: [ShellId; 2]) {
        let (ra, rb) = (self.root(a), self.root(b));
        let (lo, hi) = (ra.min(rb), ra.max(rb));
        self.up.entry(lo).or_insert(lo);
        self.up.insert(hi, lo);
        self.up.entry(a).or_insert(lo);
        self.up.entry(b).or_insert(lo);
    }
}

/// A gap's heap key: a bound not below zero orders as its bits.
fn order_key(gap: f64) -> u64 {
    if gap > 0.0 { gap.to_bits() } else { 0 }
}

/// The order a joined pair's kept piece is chosen by: the smallest reach, then the lowest
/// curve midpoint, then sheet, id and place along the edge (as the vertices' rule, so
/// permuted sheets keep the same curve).
fn keep_order((ea, pa): (&SheetEdge<'_>, &Piece), (eb, pb): (&SheetEdge<'_>, &Piece)) -> Ordering {
    let (ma, mb) = (
        ea.curve.point(pa.interval.mid()),
        eb.curve.point(pb.interval.mid()),
    );
    ea.reach
        .total_cmp(&eb.reach)
        .then(ma.x.total_cmp(&mb.x))
        .then(ma.y.total_cmp(&mb.y))
        .then(ma.z.total_cmp(&mb.z))
        .then(ea.sheet.cmp(&eb.sheet))
        .then(ea.id.cmp(&eb.id))
        .then(pa.interval.min().total_cmp(&pb.interval.min()))
}
