// Written for solidcorex. Apache-2.0.
//! The structure oracle (research 63 §6.4): which invariants a draft breaks, found from its
//! records alone, by code that shares none with `finish`. Every valid generator's body
//! breaks none; each of [`broken`](super::broken)'s breaks exactly the one it is named for.
//!
//! The geometric predicates read a use's pcurve over its edge's interval (`[0, 1]` for a
//! collapsed edge's pole segment), so a wrong stored interval is
//! [`Invariant::IntervalMismatch`] alone. Records a dead reference names are skipped by
//! every predicate but [`Invariant::LiveReferences`], and uv joins are judged only where
//! the loop is chained.

use std::collections::{BTreeMap, BTreeSet};

use scx_brep::{Draft, EdgeId, EdgeShape, FaceId, Fin, RegionId, ShellId, UseGeom, VertexId};
use scx_curve::{Curve, Curve2};
use scx_num::extent1d::Extent1d;
use scx_num::fmath;
use scx_num::frame::{Frame, Sense};
use scx_num::vector2d::Point2;
use scx_num::vector3d::{Point3, Vec3};
use scx_nurbs::Approach;
use scx_surface::Surface;

/// A structure invariant (63 §6.4; the pin's `docs/validity-and-tolerances.md` numbers
/// where it has one).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Invariant {
    /// The outside region is there and void.
    OutsideVoid,
    /// Every id a record names is live.
    LiveReferences,
    /// Every fin has its use: the face uses the edge in the fin's sense.
    FinWithoutUse,
    /// Every use has its fin.
    UseWithoutFin,
    /// An edge lists a face's pass no more times than the face makes it.
    RepeatedFin,
    /// A face uses an edge at most once in each sense (a seam once each way).
    SameSenseTwice,
    /// Each use ends at the vertex the next starts at.
    LoopChained,
    /// Successive pcurves meet in uv, not a whole period apart (Edgeuse 7).
    PeriodJump,
    /// Successive pcurves meet at a pole, joined by the pole's uv segment (Edgeuse 20,
    /// Face 24).
    PoleGap,
    /// Successive pcurves meet in uv where there is no pole or period to explain a gap.
    UvGap,
    /// A pcurve's interval is its edge's.
    IntervalMismatch,
    /// A pcurve lifts onto its edge: the surface at the pcurve is the edge's curve (for a
    /// collapsed edge, its vertex).
    PcurveOffEdge,
    /// An edge's curve ends at its vertices.
    VertexOffEdge,
    /// Every edge a solid's shell uses, it uses an even number of times: the shell is
    /// closed.
    ShellOpen,
    /// Every edge a solid's shell uses, it uses as often each way, seen from the solid.
    Misoriented,
    /// An edge's fins run counter-clockwise about its direction.
    RadialOrder,
    /// Every vertex ends an edge or is a shell's lone vertex.
    OrphanVertex,
}

impl Invariant {
    /// Every invariant.
    pub const ALL: [Self; 17] = [
        Self::OutsideVoid,
        Self::LiveReferences,
        Self::FinWithoutUse,
        Self::UseWithoutFin,
        Self::RepeatedFin,
        Self::SameSenseTwice,
        Self::LoopChained,
        Self::PeriodJump,
        Self::PoleGap,
        Self::UvGap,
        Self::IntervalMismatch,
        Self::PcurveOffEdge,
        Self::VertexOffEdge,
        Self::ShellOpen,
        Self::Misoriented,
        Self::RadialOrder,
        Self::OrphanVertex,
    ];
}

/// The oracle's tolerances.
#[derive(Debug, Clone, Copy)]
pub struct Tolerance {
    /// In model space, mm.
    pub space: f64,
    /// In uv.
    pub uv: f64,
}

impl Default for Tolerance {
    fn default() -> Self {
        Self {
            space: 1e-7,
            uv: 1e-9,
        }
    }
}

/// The invariants `draft` breaks.
pub fn violations(draft: &Draft, tol: Tolerance) -> BTreeSet<Invariant> {
    let mut out = BTreeSet::new();
    let mut flag = |i: Invariant, broken: bool| {
        if broken {
            out.insert(i);
        }
    };
    flag(Invariant::OutsideVoid, !outside_void(draft));
    flag(Invariant::LiveReferences, !live(draft));
    let (without_use, without_fin, repeated) = fins(draft);
    flag(Invariant::FinWithoutUse, without_use);
    flag(Invariant::UseWithoutFin, without_fin);
    flag(Invariant::RepeatedFin, repeated);
    flag(Invariant::SameSenseTwice, same_sense_twice(draft));
    flag(Invariant::VertexOffEdge, vertex_off_edge(draft, tol));
    let (open, misoriented) = shells(draft);
    flag(Invariant::ShellOpen, open);
    flag(Invariant::Misoriented, misoriented);
    flag(Invariant::RadialOrder, radial(draft));
    flag(Invariant::OrphanVertex, orphans(draft));
    out.extend(loops(draft, tol));
    out
}

fn outside_void(d: &Draft) -> bool {
    d.region(RegionId::OUTSIDE).is_some_and(|r| !r.is_solid())
}

fn live(d: &Draft) -> bool {
    let shells_ok = d.shells().all(|s| {
        let r = s.record();
        d.region(r.region).is_some()
            && r.wires.iter().all(|&e| d.edge(e).is_some())
            && r.lone_vertices.iter().all(|&v| d.vertex(v).is_some())
    });
    let faces_ok = d.faces().all(|f| {
        f.record().sides.iter().all(|&s| d.shell(s).is_some())
            && f.uses().all(|u| d.edge(u.record().edge).is_some())
    });
    let edges_ok = d.edges().all(|e| {
        e.ends().iter().all(|&v| d.vertex(v).is_some())
            && e.fins().all(|f| d.face(f.face).is_some())
    });
    shells_ok && faces_ok && edges_ok
}

/// Per (edge, face, sense): the uses the face makes and the fins the edge lists.
fn fins(d: &Draft) -> (bool, bool, bool) {
    let mut count: BTreeMap<(EdgeId, FaceId, bool), (usize, usize)> = BTreeMap::new();
    let fwd = |s: Sense| s == Sense::Forward;
    for f in d.faces() {
        for u in f.uses() {
            let e = u.record().edge;
            if d.edge(e).is_some() {
                count.entry((e, f.id(), fwd(u.sense()))).or_default().0 += 1;
            }
        }
    }
    for e in d.edges() {
        for Fin { face, sense } in e.fins() {
            if d.face(face).is_some() {
                count.entry((e.id(), face, fwd(sense))).or_default().1 += 1;
            }
        }
    }
    let mut out = (false, false, false);
    for &(uses, fins) in count.values() {
        out.0 |= uses == 0 && fins > 0;
        out.1 |= fins < uses;
        out.2 |= uses > 0 && fins > uses;
    }
    out
}

fn same_sense_twice(d: &Draft) -> bool {
    d.faces().any(|f| {
        let mut seen = BTreeSet::new();
        f.uses()
            .any(|u| !seen.insert((u.record().edge, u.sense() == Sense::Forward)))
    })
}

/// The interval a use's pcurve is read over: its edge's, or `[0, 1]` when collapsed.
fn edge_interval(d: &Draft, e: EdgeId) -> Option<Extent1d> {
    match d.edge(e)?.shape()? {
        EdgeShape::Curve { interval, .. } => Some(*interval),
        EdgeShape::Collapsed => Extent1d::new(0.0, 1.0),
    }
}

/// A use's uv start and end, read over its edge's interval.
fn uv_ends(geom: &UseGeom, over: Extent1d, sense: Sense) -> [Point2; 2] {
    let [a, b] = match geom {
        UseGeom::Pcurve { curve, .. } => [curve.point(over.min()), curve.point(over.max())],
        UseGeom::Point(p) => [*p, *p],
    };
    match sense {
        Sense::Forward => [a, b],
        Sense::Reversed => [b, a],
    }
}

/// The `v` of each pole of a surface.
fn pole_vs(s: &Surface) -> Vec<f64> {
    match s {
        Surface::Sphere(s) => s.poles().iter().map(|p| p.v).collect(),
        Surface::Cone(c) => vec![c.pole().v],
        _ => Vec::new(),
    }
}

/// What the loop walk finds: the chain, the uv joins, the intervals and the lifts.
fn loops(d: &Draft, tol: Tolerance) -> BTreeSet<Invariant> {
    let mut out = BTreeSet::new();
    for f in d.faces() {
        let surface = f.surface();
        let domain = surface.domain();
        let periods = [domain.u.period(), domain.v.period()];
        let poles = pole_vs(surface);
        for lp in f.loops() {
            let uses: Vec<_> = lp.uses().collect();
            for (k, u) in uses.iter().enumerate() {
                let rec = u.record();
                let Some(over) = edge_interval(d, rec.edge) else {
                    continue;
                };
                // Interval and lift.
                if let UseGeom::Pcurve { curve, interval } = &rec.geom {
                    if (interval.min() - over.min()).abs() > tol.uv
                        || (interval.max() - over.max()).abs() > tol.uv
                    {
                        out.insert(Invariant::IntervalMismatch);
                    }
                    if !lifts(d, surface, curve, rec.edge, over, tol) {
                        out.insert(Invariant::PcurveOffEdge);
                    }
                }
                // The join to the next use.
                let Some(next) = uses.get((k + 1) % uses.len()) else {
                    continue;
                };
                let nrec = next.record();
                let (Some(end), Some(start)) = (u.end(), next.start()) else {
                    continue;
                };
                if end.id() != start.id() {
                    out.insert(Invariant::LoopChained);
                    continue;
                }
                let Some(nover) = edge_interval(d, nrec.edge) else {
                    continue;
                };
                let [_, a] = uv_ends(&rec.geom, over, rec.sense);
                let [b, _] = uv_ends(&nrec.geom, nover, nrec.sense);
                let gap = [b.x - a.x, b.y - a.y];
                let mut whole = false;
                let mut residual = [0.0; 2];
                for (i, (g, p)) in gap.iter().zip(periods).enumerate() {
                    let m = p.map_or(0.0, |p| (g / p).round());
                    whole |= m != 0.0;
                    if let Some(slot) = residual.get_mut(i) {
                        *slot = g - m * p.unwrap_or(0.0);
                    }
                }
                let [ru, rv] = residual;
                if ru.abs() <= tol.uv && rv.abs() <= tol.uv {
                    if whole {
                        out.insert(Invariant::PeriodJump);
                    }
                } else if gap[1].abs() <= tol.uv && poles.iter().any(|&v| (a.y - v).abs() <= tol.uv)
                {
                    out.insert(Invariant::PoleGap);
                } else {
                    out.insert(Invariant::UvGap);
                }
            }
        }
    }
    out
}

/// Whether the surface at `curve` is edge `e`'s curve at nine parameters over `over` (its
/// vertex when collapsed).
fn lifts(
    d: &Draft,
    surface: &Surface,
    curve: &Curve2,
    e: EdgeId,
    over: Extent1d,
    tol: Tolerance,
) -> bool {
    let Some(edge) = d.edge(e) else {
        return true;
    };
    let Some(shape) = edge.shape() else {
        return true;
    };
    let target = |t: f64| -> Option<Point3> {
        match shape {
            EdgeShape::Curve { curve, .. } => Some(curve.point(t)),
            EdgeShape::Collapsed => edge.start().map(scx_brep::Vertex::point),
        }
    };
    (0..=8).all(|k| {
        let t = over.min() + (over.max() - over.min()) * f64::from(k) / 8.0;
        target(t).is_none_or(|p| (surface.point(curve.point(t)) - p).length() <= tol.space)
    })
}

fn vertex_off_edge(d: &Draft, tol: Tolerance) -> bool {
    d.edges().any(|e| {
        let Some(EdgeShape::Curve { curve, interval }) = e.shape() else {
            return false;
        };
        let ends = [interval.min(), interval.max()];
        e.ends().iter().zip(ends).any(|(&v, t)| {
            d.vertex(v)
                .is_some_and(|v| (curve.point(t) - v.point()).length() > tol.space)
        })
    })
}

/// Per solid shell and edge: the uses in each sense seen from the solid.
fn shells(d: &Draft) -> (bool, bool) {
    let solid = |s: ShellId| {
        d.shell(s)
            .and_then(|s| d.region(s.record().region))
            .is_some_and(scx_brep::Region::is_solid)
    };
    // (shell, edge) -> (with, against) seen from the solid.
    let mut count: BTreeMap<(ShellId, EdgeId), (usize, usize)> = BTreeMap::new();
    for f in d.faces() {
        for (side, &s) in f.record().sides.iter().enumerate() {
            if !solid(s) {
                continue;
            }
            // The solid behind the face sees its loops as they run; in front, reversed.
            let flip = side == 0;
            for u in f.uses() {
                let e = u.record().edge;
                let collapsed = d
                    .edge(e)
                    .is_none_or(|e| matches!(e.shape(), Some(EdgeShape::Collapsed) | None));
                if collapsed {
                    continue;
                }
                let with = (u.sense() == Sense::Forward) != flip;
                let c = count.entry((s, e)).or_default();
                if with {
                    c.0 += 1;
                } else {
                    c.1 += 1;
                }
            }
        }
    }
    let mut out = (false, false);
    for &(a, b) in count.values() {
        if (a + b) % 2 == 1 {
            out.0 = true;
        } else if a != b {
            out.1 = true;
        }
    }
    out
}

/// Edges of three or more fins, straight, each fin's use found: the fins' angles about the
/// edge's direction rise round the cycle once.
fn radial(d: &Draft) -> bool {
    d.edges().any(|e| {
        if e.fins().len() < 3 {
            return false;
        }
        let Some(EdgeShape::Curve {
            curve: Curve::Line(line),
            interval,
        }) = e.shape()
        else {
            return false;
        };
        let dir = line.point(interval.max()) - line.point(interval.min());
        let Some(frame) = Frame::new(Point3::ORIGIN, dir, any_normal(dir)) else {
            return false;
        };
        let mid = interval.mid();
        let mut angles = Vec::new();
        for fin in e.fins() {
            let Some(face) = d.face(fin.face) else {
                return false;
            };
            let Some(u) = face
                .uses()
                .find(|u| u.record().edge == e.id() && u.sense() == fin.sense)
            else {
                return false;
            };
            let UseGeom::Pcurve { curve, .. } = u.geom() else {
                return false;
            };
            let Some(n) = face
                .surface()
                .normal(curve.point(mid), [Approach::Above, Approach::Above])
            else {
                return false;
            };
            let w = (*n).cross(dir * fin.sense.sign());
            angles.push(fmath::atan2(w.dot(*frame.y()), w.dot(*frame.x())));
        }
        let descents = (0..angles.len())
            .filter(|&k| {
                let (a, b) = (angles.get(k), angles.get((k + 1) % angles.len()));
                matches!((a, b), (Some(a), Some(b)) if b < a)
            })
            .count();
        descents > 1
    })
}

fn any_normal(d: Vec3) -> Vec3 {
    let a = [d.x.abs(), d.y.abs(), d.z.abs()];
    if a[0] <= a[1] && a[0] <= a[2] {
        Vec3::X
    } else if a[1] <= a[2] {
        Vec3::new(0.0, 1.0, 0.0)
    } else {
        Vec3::Z
    }
}

fn orphans(d: &Draft) -> bool {
    let mut used: BTreeSet<VertexId> = BTreeSet::new();
    for e in d.edges() {
        used.extend(e.ends());
    }
    for s in d.shells() {
        used.extend(s.record().lone_vertices.iter().copied());
    }
    d.vertices().any(|v| !used.contains(&v.id()))
}
