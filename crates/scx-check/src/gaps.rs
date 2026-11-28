// Written for solidcorex. Apache-2.0.
//! The certified gaps of a body (research 74 §28, `CF2a`): each edge against its faces, its
//! uses and its vertices, its own extent, and (under [`Scope::Full`]) the ratio limit.
//!
//! **The allowances are finish's** (09 §3: a vertex is a ball, an edge a tube, distances
//! add), with the reach of a recorded accuracy `max(acc, zone)`:
//!
//! | question | measured | allowed |
//! |---|---|---|
//! | the edge lies on the face | the curve's largest distance from the kind's whole surface | `re` |
//! | the use follows the edge | the pcurve's lift against the curve at the same parameter | `rb` |
//! | a collapsed edge's use | the lift against the vertex | `rb + re + rv` |
//! | the edge ends at its vertex | the curve's end against the vertex | `re + rv` |
//! | the edge is not degenerate | the curve's diameter | above `re` |
//!
//! **Edge against face, without a trimmed gap.** The face's own gap (from the trimmed face)
//! is bounded below by the whole surface's ([`scx_surface::curve_surface_gap`]) and above by
//! the pcurve's ([`scx_surface::pcurve_edge_gap`]: the lift lies in the face). The edge is
//! taken to lie on its face when one of the two upper bounds is within `re`; a finding needs
//! the surface's lower bound past `re`. What neither decides is [`Undecided::EdgeOffFace`]:
//! no trim-restricted gap is built (74 §28 argues why the check does not need one).
//!
//! **A recorded certificate** (04 SP2, 74 §35): under [`Scope::Import`] and
//! [`Scope::Stitched`], a gap the body records with a [`Cert`](scx_num::cert::Cert) of the question's claim whose
//! bound is within the question's allowance is taken as decided, not proved again: a use's
//! [`Claim::PcurveOnSurface`] answers the use (and the face, within `re`), an edge's
//! [`Claim::CurveToIntersection`] answers its faces. Any other basis, another claim or a
//! bound past the allowance is proved as before. [`Scope::Full`] proves everything.
//!
//! **Budget** (74 R6): each edge in id order gets an equal share of what is left; what it
//! does not spend passes on. An edge whose share runs out lists what it reached.

use core::cmp::Ordering;
use std::collections::BinaryHeap;

use scx_brep::{Brep, Edge, EdgeShape, Entity, FaceId, Fin, Use, UseGeom};
use scx_curve::{Curve, Curve2};
use scx_num::budget::{Budget, Stop};
use scx_num::cert::{Basis, Claim, Recorded};
use scx_num::consts::MAX_ACCURACY_RATIO;
use scx_num::extent1d::Extent1d;
use scx_num::extent2d::Extent2d;
use scx_num::interval::Ivl;
use scx_num::units::{Length, Resolution};
use scx_num::vector2d::Point2;
use scx_num::vector3d::Point3;
use scx_surface::{Aim, Bracket, GapError, Pending, Short, Surface};

use crate::error::CheckError;
use crate::findings::{Finding, Scope, Undecided, Why};

/// A bracket that straddles its allowance ends as undecided ([`Why::AtAllowance`]) once it
/// is at most this times the allowance wide.
pub(crate) const NEAR: f64 = 1e-3;

/// Pieces a degenerate edge's diameter is first bounded over; doubled twice at most.
const DIAMETER_PIECES: usize = 16;

/// Points a curve's diameter is bounded below by.
const DIAMETER_SAMPLES: usize = 9;

/// The gaps' findings and what they left undecided.
pub(crate) struct Found {
    pub(crate) list: Vec<Finding>,
    pub(crate) undecided: Vec<Undecided>,
}

/// The reach of a recorded accuracy: finish's rule.
pub(crate) fn reach(recorded: Option<&Recorded>, res: Resolution) -> f64 {
    let zone = res.zone().mm();
    recorded.map_or(zone, |r| r.acc.length().mm().max(zone))
}

/// Every gap of `body`, the edges sharing what is left of `budget`.
pub(crate) fn run(
    body: &Brep,
    scope: Scope,
    budget: &mut Budget,
    stop: &Stop,
) -> Result<Found, CheckError> {
    let res = body.resolution();
    let mut out = Found {
        list: Vec::new(),
        undecided: Vec::new(),
    };
    if scope == Scope::Full {
        wide(body, res, &mut out);
    }
    let n = body.edges().len();
    for (i, edge) in body.edges().enumerate() {
        stop.check().map_err(|_| CheckError::Cancelled)?;
        let share = budget.remaining() / (n - i) as u64;
        let mut sub = Budget::new(share);
        edge_gaps(edge, res, scope, &mut sub, stop, &mut out)?;
        // A share is at most what is left, so this charge fits.
        let _ = budget.charge(sub.used().min(share));
    }
    Ok(out)
}

/// The ratio limit (09 §6), free: a recorded accuracy at or past 1000 zones.
fn wide(body: &Brep, res: Resolution, out: &mut Found) {
    let limit = res.zone().mm() * MAX_ACCURACY_RATIO;
    let over = |r: Option<&Recorded>| {
        r.map(|r| r.acc.length())
            .filter(|acc| acc.mm().partial_cmp(&limit) != Some(Ordering::Less))
    };
    for v in body.vertices() {
        if let Some(accuracy) = over(v.accuracy()) {
            out.list.push(Finding::WideVertex {
                vertex: v.id(),
                accuracy,
                limit: Length::from_mm(limit),
            });
        }
    }
    for e in body.edges() {
        if let Some(accuracy) = over(e.accuracy()) {
            out.list.push(Finding::WideEdge {
                edge: e.id(),
                accuracy,
                limit: Length::from_mm(limit),
            });
        }
    }
}

/// How a bracket stands against its allowance.
#[derive(Clone, Copy)]
enum Verdict {
    Clean(Bracket),
    Over(Bracket),
    Open(Bracket, Why),
    Pending(Pending),
    NotFinite,
}

impl Verdict {
    /// The best upper bound known, where there is one.
    fn upper(self) -> Option<f64> {
        match self {
            Self::Clean(b) | Self::Over(b) | Self::Open(b, _) => Some(b.upper()),
            Self::Pending(_) | Self::NotFinite => None,
        }
    }
}

/// `b` against `allowed`, `why` when it straddles.
fn against(b: Bracket, allowed: f64, why: Why) -> Verdict {
    match b.exceeds(allowed) {
        Some(true) => Verdict::Over(b),
        Some(false) => Verdict::Clean(b),
        None => Verdict::Open(b, why),
    }
}

/// A gap search's result against `allowed`; a reached bracket still counts.
fn verdict(r: Result<Bracket, GapError>, allowed: f64) -> Result<Verdict, CheckError> {
    match r {
        Ok(b) => Ok(against(b, allowed, Why::AtAllowance)),
        Err(GapError::Undecided {
            why: Short::Stopped,
            ..
        }) => Err(CheckError::Cancelled),
        Err(GapError::Undecided { reached, why }) => Ok(against(
            reached,
            allowed,
            match why {
                Short::Exhausted(_) => Why::Exhausted,
                _ => Why::Precision,
            },
        )),
        Err(GapError::Pending(p)) => Ok(Verdict::Pending(p)),
        Err(_) => Ok(Verdict::NotFinite),
    }
}

/// The bracket `[0, bound]` of a recorded certificate of `claim` whose bound is within
/// `allowed`, where `scope` takes one as decided; `None` when the question must be proved.
fn certified(
    recorded: Option<&Recorded>,
    claim: Claim,
    allowed: f64,
    scope: Scope,
) -> Option<Bracket> {
    if !matches!(scope, Scope::Import | Scope::Stitched) {
        return None;
    }
    let Basis::Certified(cert) = recorded?.basis else {
        return None;
    };
    let bound = cert.bound.mm();
    (cert.claim == claim && bound <= allowed)
        .then(|| Bracket::new(0.0, bound))
        .flatten()
}

const fn decide(allowance: f64) -> Aim {
    Aim::Decide {
        allowance,
        width: NEAR * allowance,
    }
}

fn edge_gaps(
    edge: Edge<'_>,
    res: Resolution,
    scope: Scope,
    budget: &mut Budget,
    stop: &Stop,
    out: &mut Found,
) -> Result<(), CheckError> {
    let id = edge.id();
    let re = reach(edge.accuracy(), res);
    let EdgeShape::Curve { curve, interval } = edge.shape() else {
        let v = edge.start();
        let (p, rv) = (v.point(), reach(v.accuracy(), res));
        for u in edge.uses() {
            let rb = reach(u.bound(), res);
            let allowed = rb + re + rv;
            let lift = lift_of(u);
            let r = lift_gap(
                u.face().surface(),
                lift,
                Target::Point(p),
                allowed,
                budget,
                stop,
            );
            use_verdict(id, u.fin(), allowed, verdict(r, allowed)?, out);
        }
        return Ok(());
    };
    // The ends against their vertices.
    for (v, at) in [(edge.start(), interval.min()), (edge.end(), interval.max())] {
        let allowed = re + reach(v.accuracy(), res);
        let _ = budget.charge(1);
        match scx_surface::vertex_curve_gap(v.point(), curve, at) {
            Ok(gap) => match gap.exceeds(allowed) {
                Some(true) => out.list.push(Finding::VertexOffEdge {
                    vertex: v.id(),
                    edge: id,
                    at,
                    gap,
                    allowed: Length::from_mm(allowed),
                }),
                Some(false) => {}
                None => out.undecided.push(Undecided::VertexOffEdge {
                    vertex: v.id(),
                    edge: id,
                    at,
                    gap,
                    allowed: Length::from_mm(allowed),
                    why: Why::Precision,
                }),
            },
            Err(_) => out.undecided.push(Undecided::NotFinite {
                edge: id,
                face: None,
            }),
        }
    }
    // The uses, and through them the faces; a seam's two uses share their face's answer.
    let mut faces: Vec<(FaceId, Option<f64>)> = Vec::new();
    for u in edge.uses() {
        let surface = u.face().surface();
        let rb = reach(u.bound(), res);
        let p = match certified(u.bound(), Claim::PcurveOnSurface, rb, scope) {
            Some(b) => Verdict::Clean(b),
            None => use_gap(surface, u, curve, interval, rb, re, budget, stop)?,
        };
        use_verdict(id, u.fin(), rb, p, out);
        let face = u.face().id();
        let best = p.upper();
        match faces.iter_mut().find(|(f, _)| *f == face) {
            Some((_, b)) => *b = min_opt(*b, best),
            None => faces.push((face, best)),
        }
    }
    let by_record = certified(edge.accuracy(), Claim::CurveToIntersection, re, scope).is_some();
    for (face, through_use) in faces {
        if by_record || through_use.is_some_and(|b| b <= re) {
            continue;
        }
        let Some(surface) = edge
            .uses()
            .find(|u| u.face().id() == face)
            .map(|u| u.face().surface())
        else {
            continue;
        };
        let r = scx_surface::curve_surface_gap(curve, interval, surface, decide(re), budget, stop);
        let allowed = Length::from_mm(re);
        match verdict(r, re)? {
            Verdict::Clean(_) => {}
            Verdict::Over(gap) => out.list.push(Finding::EdgeOffFace {
                edge: id,
                face,
                gap,
                allowed,
            }),
            Verdict::Open(gap, why) => out.undecided.push(Undecided::EdgeOffFace {
                edge: id,
                face,
                gap,
                allowed,
                why,
            }),
            Verdict::Pending(what) => out.undecided.push(Undecided::Pending {
                edge: id,
                face,
                what,
            }),
            Verdict::NotFinite => out.undecided.push(Undecided::NotFinite {
                edge: id,
                face: Some(face),
            }),
        }
    }
    degenerate(id, curve, interval, re, budget, out);
    Ok(())
}

fn min_opt(a: Option<f64>, b: Option<f64>) -> Option<f64> {
    match (a, b) {
        (Some(a), Some(b)) => Some(a.min(b)),
        (a, b) => a.or(b),
    }
}

/// A curve edge's use against the edge, to `rb`. Where the use's bound is wider than the
/// edge's, the search aims at the edge's reach first: a lift within it settles the face too.
#[allow(clippy::too_many_arguments)]
fn use_gap(
    surface: &Surface,
    u: Use<'_>,
    curve: &Curve,
    interval: &Extent1d,
    rb: f64,
    re: f64,
    budget: &mut Budget,
    stop: &Stop,
) -> Result<Verdict, CheckError> {
    let run = |a: f64, budget: &mut Budget| -> Result<Bracket, GapError> {
        match u.geom() {
            UseGeom::Pcurve { curve: pc, .. } => {
                scx_surface::pcurve_edge_gap(surface, pc, curve, interval, decide(a), budget, stop)
            }
            UseGeom::Point(p) => lift_gap(
                surface,
                Lift::Point(*p),
                Target::Curve(curve, interval),
                a,
                budget,
                stop,
            ),
        }
    };
    let first = rb.min(re);
    let v = verdict(run(first, budget), rb)?;
    if first < rb
        && let Verdict::Open(..) = v
    {
        return verdict(run(rb, budget), rb);
    }
    Ok(v)
}

fn use_verdict(edge: scx_brep::EdgeId, fin: Fin, allowed: f64, v: Verdict, out: &mut Found) {
    let allowed = Length::from_mm(allowed);
    match v {
        Verdict::Clean(_) => {}
        Verdict::Over(gap) => out.list.push(Finding::PcurveOffEdge {
            edge,
            fin,
            gap,
            allowed,
        }),
        Verdict::Open(gap, why) => out.undecided.push(Undecided::PcurveOffEdge {
            edge,
            fin,
            gap,
            allowed,
            why,
        }),
        Verdict::Pending(what) => out.undecided.push(Undecided::Pending {
            edge,
            face: fin.face,
            what,
        }),
        Verdict::NotFinite => out.undecided.push(Undecided::NotFinite {
            edge,
            face: Some(fin.face),
        }),
    }
}

/// What a use lifts from its face.
#[derive(Clone, Copy)]
enum Lift<'a> {
    Pcurve(&'a Curve2, Extent1d),
    Point(Point2),
}

fn lift_of(u: Use<'_>) -> Lift<'_> {
    match u.geom() {
        UseGeom::Pcurve { curve, interval } => Lift::Pcurve(curve, *interval),
        UseGeom::Point(p) => Lift::Point(*p),
    }
}

/// What the lift is measured against.
#[derive(Clone, Copy)]
enum Target<'a> {
    Point(Point3),
    Curve(&'a Curve, &'a Extent1d),
}

/// A piece of a lift search: its range and the upper bound over it.
struct Piece {
    a: f64,
    b: f64,
    upper: f64,
    /// The distance's lower bound at the piece's middle.
    low: f64,
}

impl PartialEq for Piece {
    fn eq(&self, o: &Self) -> bool {
        self.cmp(o) == Ordering::Equal
    }
}
impl Eq for Piece {}
impl PartialOrd for Piece {
    fn partial_cmp(&self, o: &Self) -> Option<Ordering> {
        Some(self.cmp(o))
    }
}
impl Ord for Piece {
    fn cmp(&self, o: &Self) -> Ordering {
        self.upper
            .total_cmp(&o.upper)
            .then_with(|| o.a.total_cmp(&self.a))
    }
}

fn ivl_box(e: Option<scx_num::extent3d::Extent3d>) -> Option<[Ivl; 3]> {
    e.map(|e| {
        let (lo, hi) = (e.min(), e.max());
        [
            Ivl::new(lo.x, hi.x),
            Ivl::new(lo.y, hi.y),
            Ivl::new(lo.z, hi.z),
        ]
    })
}

fn norm(v: [Ivl; 3]) -> Ivl {
    (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt()
}

/// The enclosed distance between the lift and the target over `t`.
fn lift_at(surface: &Surface, lift: Lift<'_>, target: Target<'_>, t: Ivl) -> Option<Ivl> {
    let uv = match lift {
        Lift::Pcurve(pc, _) => pc.point_over(t),
        Lift::Point(p) => Point2::new(Ivl::from(p.x), Ivl::from(p.y)),
    };
    let s = ivl_box(
        Extent2d::from_corners(Point2::new(uv.x.lo, uv.y.lo), Point2::new(uv.x.hi, uv.y.hi))
            .and_then(|e| surface.box_over(e)),
    )?;
    let c = match target {
        Target::Point(p) => [Ivl::from(p.x), Ivl::from(p.y), Ivl::from(p.z)],
        Target::Curve(c, _) => {
            let p = c.point_over(t);
            [p.x, p.y, p.z]
        }
    };
    let d = norm([s[0] - c[0], s[1] - c[1], s[2] - c[2]]);
    (d.lo.is_finite() && d.hi.is_finite()).then_some(d)
}

/// The largest distance of a use's lift from `target`, by branch and bound on the direct
/// enclosure (first order: these are short, or constant, lifts), to
/// [`decide`]`(allowed)`, one budget unit per piece.
fn lift_gap(
    surface: &Surface,
    lift: Lift<'_>,
    target: Target<'_>,
    allowed: f64,
    budget: &mut Budget,
    stop: &Stop,
) -> Result<Bracket, GapError> {
    let range = match (lift, target) {
        (Lift::Pcurve(_, i), _) | (Lift::Point(_), Target::Curve(_, &i)) => (i.min(), i.max()),
        (Lift::Point(_), Target::Point(_)) => (0.0, 0.0),
    };
    let width = NEAR * allowed;
    let mut lower = 0.0_f64;
    let mut heap = BinaryHeap::new();
    // A piece's evaluation fails short (`Some`) or not finite (`None`).
    let eval = |a: f64, b: f64, budget: &mut Budget| -> Result<Piece, Option<Short>> {
        budget.charge(1).map_err(|e| Some(Short::Exhausted(e)))?;
        let at = lift_at(surface, lift, target, Ivl::from(0.5 * (a + b))).ok_or(None)?;
        let over = lift_at(surface, lift, target, Ivl::new(a, b)).ok_or(None)?;
        Ok(Piece {
            a,
            b,
            upper: over.hi,
            low: at.lo.max(0.0),
        })
    };
    let short = |reached: Option<Bracket>, why: Option<Short>| match (reached, why) {
        (Some(reached), Some(why)) => GapError::Undecided { reached, why },
        _ => GapError::NotFinite,
    };
    let first = eval(range.0, range.1, budget)
        .map_err(|why| short(Bracket::new(0.0, f64::INFINITY), why))?;
    lower = lower.max(first.low);
    heap.push(first);
    loop {
        let upper = heap.peek().map_or(0.0, |p| p.upper).max(lower);
        let reached = Bracket::new(lower, upper).ok_or(GapError::NotFinite)?;
        if reached.exceeds(allowed).is_some() || reached.width() <= width {
            return Ok(reached);
        }
        if stop.is_set() {
            return Err(short(Some(reached), Some(Short::Stopped)));
        }
        let Some(top) = heap.pop() else {
            return Ok(reached);
        };
        let m = 0.5 * (top.a + top.b);
        if !(top.a < m && m < top.b) {
            return Err(short(Some(reached), Some(Short::Precision)));
        }
        for (a, b) in [(top.a, m), (m, top.b)] {
            let p = eval(a, b, budget).map_err(|why| short(Some(reached), why))?;
            lower = lower.max(p.low);
            heap.push(p);
        }
    }
}

/// A curve edge no wider than its reach (74 Q5). Its diameter is bounded below by points and
/// above by the farthest pair of piece boxes; a lower bound past the reach (every real edge)
/// ends it at once.
fn degenerate(
    edge: scx_brep::EdgeId,
    curve: &Curve,
    interval: &Extent1d,
    re: f64,
    budget: &mut Budget,
    out: &mut Found,
) {
    let (lo, hi) = (interval.min(), interval.max());
    let at = |k: usize, n: usize| lo + (hi - lo) * (k as f64 / n as f64);
    let point = |t: Ivl| {
        let p = curve.point_over(t);
        [p.x, p.y, p.z]
    };
    let samples: Vec<[Ivl; 3]> = (0..DIAMETER_SAMPLES)
        .map(|k| point(Ivl::from(at(k, DIAMETER_SAMPLES - 1))))
        .collect();
    let _ = budget.charge(DIAMETER_SAMPLES as u64);
    let mut lower = 0.0_f64;
    for p in [samples.first(), samples.get(DIAMETER_SAMPLES / 2)]
        .into_iter()
        .flatten()
    {
        for q in &samples {
            let d = norm([p[0] - q[0], p[1] - q[1], p[2] - q[2]]);
            if d.lo.is_finite() {
                lower = lower.max(d.lo);
            }
        }
    }
    if lower > re {
        return;
    }
    let allowed = Length::from_mm(re);
    let mut upper = f64::INFINITY;
    let mut n = DIAMETER_PIECES;
    for _ in 0..3 {
        if budget.charge(n as u64).is_err() {
            break;
        }
        let boxes: Vec<[Ivl; 3]> = (0..n)
            .map(|k| point(Ivl::new(at(k, n), at(k + 1, n))))
            .collect();
        let mut far = 0.0_f64;
        for (i, a) in boxes.iter().enumerate() {
            for b in boxes.iter().skip(i) {
                // The farthest two points of the boxes: per axis the larger end difference.
                let axis = |a: Ivl, b: Ivl| {
                    let x = (b - Ivl::from(a.lo)).hi.max((a - Ivl::from(b.lo)).hi);
                    Ivl::from(x.max(0.0))
                };
                far = far.max(norm([axis(a[0], b[0]), axis(a[1], b[1]), axis(a[2], b[2])]).hi);
            }
        }
        upper = upper.min(if far.is_nan() { f64::INFINITY } else { far });
        if upper <= re {
            break;
        }
        n *= 2;
    }
    match Bracket::new(lower, upper.max(lower)) {
        Some(extent) if upper <= re => out.list.push(Finding::DegenerateEdge {
            edge,
            extent,
            allowed,
        }),
        Some(extent) => out.undecided.push(Undecided::DegenerateEdge {
            edge,
            extent,
            allowed,
            why: if upper.is_finite() {
                Why::Enclosure
            } else {
                Why::Exhausted
            },
        }),
        None => out
            .undecided
            .push(Undecided::NotFinite { edge, face: None }),
    }
}

/// The sort key of a finding: kind, then the entities it names, then the parameter.
pub(crate) fn finding_key(f: &Finding) -> (u8, Entity, Option<Entity>, u8, u64) {
    let fin = |fin: &Fin| (Some(Entity::Face(fin.face)), fin.sense as u8);
    match f {
        Finding::VertexNotManifold { vertex, .. } => (0, Entity::Vertex(*vertex), None, 0, 0),
        Finding::NotSolid { shell } => (1, Entity::Shell(*shell), None, 0, 0),
        Finding::EdgeOffFace { edge, face, .. } => {
            (2, Entity::Edge(*edge), Some(Entity::Face(*face)), 0, 0)
        }
        Finding::PcurveOffEdge { edge, fin: f, .. } => {
            let (face, sense) = fin(f);
            (3, Entity::Edge(*edge), face, sense, 0)
        }
        Finding::VertexOffEdge {
            vertex, edge, at, ..
        } => (
            4,
            Entity::Edge(*edge),
            Some(Entity::Vertex(*vertex)),
            0,
            at.to_bits(),
        ),
        Finding::DegenerateEdge { edge, .. } => (5, Entity::Edge(*edge), None, 0, 0),
        Finding::WideVertex { vertex, .. } => (6, Entity::Vertex(*vertex), None, 0, 0),
        Finding::WideEdge { edge, .. } => (7, Entity::Edge(*edge), None, 0, 0),
        Finding::VerticesWithinCoincidence { a, b, .. } => {
            (10, Entity::Vertex(*a), Some(Entity::Vertex(*b)), 0, 0)
        }
    }
}

/// The sort key of an undecided question, as [`finding_key`].
pub(crate) fn undecided_key(u: &Undecided) -> (u8, Entity, Option<Entity>, u8, u64) {
    match u {
        Undecided::EdgeOffFace { edge, face, .. } => {
            (2, Entity::Edge(*edge), Some(Entity::Face(*face)), 0, 0)
        }
        Undecided::PcurveOffEdge { edge, fin, .. } => (
            3,
            Entity::Edge(*edge),
            Some(Entity::Face(fin.face)),
            fin.sense as u8,
            0,
        ),
        Undecided::VertexOffEdge {
            vertex, edge, at, ..
        } => (
            4,
            Entity::Edge(*edge),
            Some(Entity::Vertex(*vertex)),
            0,
            at.to_bits(),
        ),
        Undecided::DegenerateEdge { edge, .. } => (5, Entity::Edge(*edge), None, 0, 0),
        Undecided::Pending { edge, face, .. } => {
            (8, Entity::Edge(*edge), Some(Entity::Face(*face)), 0, 0)
        }
        Undecided::NotFinite { edge, face } => {
            (9, Entity::Edge(*edge), face.map(Entity::Face), 0, 0)
        }
        Undecided::VerticesWithinCoincidence { a, b, .. } => {
            (10, Entity::Vertex(*a), Some(Entity::Vertex(*b)), 0, 0)
        }
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::panic, clippy::indexing_slicing)]

    use scx_curve::Circle;
    use scx_num::frame::{Frame, Sense};
    use scx_num::vector3d::Vec3;
    use scx_surface::Sphere;

    use super::*;

    fn circle(r: f64) -> Curve {
        Curve::Circle(Circle::new(Frame::WORLD, Length::from_mm(r), Sense::Forward).unwrap())
    }

    fn degenerate_of(r: f64, re: f64, budget: &mut Budget) -> Found {
        let mut out = Found {
            list: Vec::new(),
            undecided: Vec::new(),
        };
        let turn = Extent1d::new(0.0, core::f64::consts::TAU).unwrap();
        degenerate(EdgeId::from_raw(0), &circle(r), &turn, re, budget, &mut out);
        out
    }

    use scx_brep::EdgeId;

    /// A whole circle's diameter is `2 r`: one of radius `r` in `(re / 6.1, re / 2]` passes
    /// finish's 8-segment length test (`6.12 r > re`) and is degenerate; one of `0.6 re` is
    /// not, and is ended by its samples alone.
    #[test]
    fn a_circle_within_its_reach_is_degenerate() {
        let re = 1e-5;
        for r in [0.2 * re, 0.4 * re, 0.45 * re] {
            let out = degenerate_of(r, re, &mut Budget::unlimited());
            let [
                Finding::DegenerateEdge {
                    extent, allowed, ..
                },
            ] = out.list[..]
            else {
                panic!("{r}: {:?} {:?}", out.list, out.undecided);
            };
            assert!(extent.lower() <= 2.0 * r && 2.0 * r <= extent.upper());
            assert_eq!(allowed.mm(), re);
        }
        for r in [0.6 * re, 1.0, 100.0] {
            let mut b = Budget::unlimited();
            let out = degenerate_of(r, re, &mut b);
            assert!(out.list.is_empty() && out.undecided.is_empty(), "{r}");
            assert_eq!(b.used(), DIAMETER_SAMPLES as u64);
        }
        // At the allowance the boxes cannot decide: undecided, never a finding.
        let out = degenerate_of(0.5 * re, re, &mut Budget::unlimited());
        assert!(out.list.is_empty(), "{:?}", out.list);
        // Out of budget: undecided with why.
        let out = degenerate_of(0.2 * re, re, &mut Budget::new(10));
        assert!(matches!(
            out.undecided[..],
            [Undecided::DegenerateEdge {
                why: Why::Exhausted,
                ..
            }]
        ));
    }

    fn sphere() -> Surface {
        Surface::Sphere(Sphere::new(Frame::WORLD, Length::from_mm(2.0), Sense::Forward).unwrap())
    }

    /// A pole's lift against its vertex: a point use and a pcurve along the pole's line are
    /// the pole, to rounding; a vertex moved `d` off is `d` off.
    #[test]
    fn a_pole_lift_is_bracketed_against_its_vertex() {
        let s = sphere();
        let pole = Point3::new(0.0, 0.0, 2.0);
        let line = Curve2::Line(
            scx_curve::Line2::new(
                Point2::new(0.0, 1.0),
                scx_num::vector2d::Vec2::new(1.0, 0.0),
            )
            .unwrap(),
        );
        let lifts = [
            Lift::Point(Point2::new(0.3, 1.0)),
            Lift::Pcurve(&line, Extent1d::new(0.0, 0.9).unwrap()),
        ];
        for lift in lifts {
            for (d, allowed, over) in [(0.0, 1e-9, false), (1e-4, 1e-5, true), (1e-6, 1e-5, false)]
            {
                let target = pole + Vec3::new(0.0, d, 0.0);
                let mut budget = Budget::unlimited();
                let b = lift_gap(
                    &s,
                    lift,
                    Target::Point(target),
                    allowed,
                    &mut budget,
                    &Stop::new(),
                )
                .unwrap();
                assert!(b.lower() <= d && d <= b.upper() + 1e-15, "{d}: {b:?}");
                assert_eq!(b.exceeds(allowed), Some(over), "{d}: {b:?}");
                assert!(budget.used() < 1000, "{}", budget.used());
            }
        }
    }
}
