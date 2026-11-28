// Written for solidcorex. Apache-2.0.
//! B-spline ends (research 75 §6, RP2): a NURBS edge re-ended at a cluster's target by
//! moving its end control point, its empty interior pieces cut, each certified on the
//! gaps the check asks.
//!
//! **Re-ended by its control point.** A NURBS edge at a cluster is trimmed to its interval
//! (so each end is a clamped end control point) and the control point at the cluster moved
//! to the target, its weight kept. A vertex merged by an earlier item already moved the
//! other end; a vertex not merged keeps the curve's own end. Only the end piece changes.
//!
//! **An empty piece** ([`EmptyPieces`]) is an interior Bezier piece of a NURBS edge between
//! two C0 joints (knots of multiplicity `p`) whose control points all lie within the
//! edge's reach of its first: the curve over it is a point at the edge's accuracy. It is
//! cut as the pin cuts it: the first control point is kept, the piece's others (the joint
//! after it among them) go, and the knots after it move down by its length. The piece
//! after it then starts at the kept point; on a rational curve every weight after it is
//! scaled by the two joints' ratio, which leaves each later piece's shape alone (a rational
//! piece is unchanged when all its weights scale alike), so unequal weights are cut too.
//! Moving one control point by `d` with its weight kept moves the curve by at most `d` (the
//! rational basis is a partition of unity), so a cut moves the curve after it by at most the
//! piece's spread. The spreads, added, are held to the edge's reach ([`PiecesSpread`]).
//!
//! **Certified on the check's gaps** (the SU6 gaps, not the pin's `GAP_SHARE`). A reshaped
//! edge is done only when, for the curve as it will be stored:
//!
//! - each end is within `re + rv` of its vertex ([`EndOffVertex`], `vertex_curve_gap`);
//! - its diameter is above `re` ([`NeighbourShort`]: the check's sample lower bound);
//! - each use is on a plane ([`FaceKind`]) and its new pcurve (the exact projection of the
//!   curve: control points projected, knots and weights kept) lifts within the smaller of
//!   the edge's and the use's reach of the curve ([`OffFace`], `pcurve_edge_gap`). That
//!   one bound answers the check's two questions for the use, as for a line (RP1).
//!
//! The pin shares the vertex tolerance between the two gaps (`GAP_SHARE = 0.5`); the check
//! allows each gap its own sum of reaches, so the certificate is the check's own question
//! and no share is needed (RP1 ruled the same for lines).
//!
//! [`PiecesSpread`]: HealCause::PiecesSpread
//! [`EndOffVertex`]: HealCause::EndOffVertex
//! [`NeighbourShort`]: HealCause::NeighbourShort
//! [`FaceKind`]: HealCause::FaceKind
//! [`OffFace`]: HealCause::OffFace

use std::collections::BTreeSet;
use std::sync::Arc;

use scx_brep::{Brep, Edge, EdgeId, EdgeShape, FaceId, UseGeom, VertexId};
use scx_curve::{Curve, Curve2};
use scx_num::budget::Budget;
use scx_num::extent1d::Extent1d;
use scx_num::frame::Sense;
use scx_num::interval::Ivl;
use scx_num::units::Length;
use scx_num::vector3d::Point3;
use scx_nurbs::{KnotVector, NurbsCurve};
use scx_surface::{Aim, Bracket, GapError, Plane, Short, Surface};

use super::{Cluster, State, distance, norm, reach};
use crate::error::HealCause;

/// An edge's empty interior pieces, to be cut.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub struct EmptyPieces {
    /// The edge.
    pub edge: EdgeId,
    /// The pieces' knot spans, in order.
    pub pieces: Vec<Extent1d>,
}

/// A NURBS edge as an item leaves it: its ends, its curve over its domain, and each use's
/// pcurve by face and sense.
#[derive(Debug, Clone)]
pub(super) struct Reshaped {
    pub(super) ends: [VertexId; 2],
    pub(super) curve: Arc<NurbsCurve<3>>,
    pub(super) pcurves: Vec<(FaceId, Sense, Curve2)>,
}

impl Reshaped {
    /// Its interval: the curve's domain.
    pub(super) fn interval(&self) -> Extent1d {
        self.curve.domain()
    }

    /// The pcurve of the use of `face` in `sense`.
    pub(super) fn pcurve(&self, face: FaceId, sense: Sense) -> Option<&Curve2> {
        self.pcurves
            .iter()
            .find(|(f, s, _)| *f == face && *s == sense)
            .map(|(_, _, c)| c)
    }
}

/// A bracket that straddles its allowance ends the search once it is at most this times the
/// allowance wide (the check's own `NEAR`).
const NEAR: f64 = 1e-3;

/// Points a curve's diameter is bounded below by (the check's count).
const DIAMETER_SAMPLES: usize = 9;

/// An empty piece found: the index of its first control point, its span, and the spread
/// of its control points from the first.
struct Piece {
    first: usize,
    span: Extent1d,
    spread: Bracket,
}

/// The empty interior pieces of `curve` strictly inside `interval`, for reach `re`.
fn pieces(curve: &NurbsCurve<3>, interval: Extent1d, re: f64) -> Vec<Piece> {
    let p = curve.degree();
    let knots = curve.knots();
    let joints: Vec<f64> = curve
        .knot_vector()
        .breakpoints()
        .filter(|&(k, m)| m == p && interval.min() < k && k < interval.max())
        .map(|(k, _)| k)
        .collect();
    let points = curve.points();
    let mut found = Vec::new();
    for (&k0, &k1) in joints.iter().zip(joints.iter().skip(1)) {
        // The joint's control point: its last knot's index less the degree.
        let last = knots.partition_point(|&k| k <= k0);
        let Some(first) = last.checked_sub(1 + p) else {
            continue;
        };
        let (Some(piece), Some(span)) = (points.get(first..=first + p), Extent1d::new(k0, k1))
        else {
            continue;
        };
        let Some(&[x, y, z]) = piece.first() else {
            continue;
        };
        let origin = Point3::new(x, y, z);
        let mut spread = distance(origin, origin);
        for &[x, y, z] in piece.iter().skip(1) {
            spread = match (spread, distance(Point3::new(x, y, z), origin)) {
                (Some(s), Some(d)) => {
                    Bracket::new(s.lower().max(d.lower()), s.upper().max(d.upper()))
                }
                _ => None,
            };
        }
        if let Some(spread) = spread
            && spread.upper() <= re
        {
            found.push(Piece {
                first,
                span,
                spread,
            });
        }
    }
    found
}

/// The edges' empty pieces, by edge id.
pub(super) fn empty_pieces(body: &Brep) -> Vec<EmptyPieces> {
    let res = body.resolution();
    body.edges()
        .filter_map(|e| {
            let EdgeShape::Curve {
                curve: Curve::Nurbs(n),
                interval,
            } = e.shape()
            else {
                return None;
            };
            let found = pieces(n, *interval, reach(e.accuracy(), res));
            (!found.is_empty()).then(|| EmptyPieces {
                edge: e.id(),
                pieces: found.iter().map(|x| x.span).collect(),
            })
        })
        .collect()
}

/// `curve` over `interval` alone: split where the interval ends inside the domain.
fn trim(curve: &NurbsCurve<3>, interval: Extent1d) -> Option<NurbsCurve<3>> {
    let domain = curve.domain();
    let mut out = curve.clone();
    if interval.min() > domain.min() {
        out = out.split(interval.min()).ok()?.1;
    }
    if interval.max() < out.domain().max() {
        out = out.split(interval.max()).ok()?.0;
    }
    Some(out)
}

/// `curve` with its first and last control points moved where `to` says, weights kept;
/// `None` unless it is clamped there (its ends are those points).
fn moved(curve: &NurbsCurve<3>, to: [Option<Point3>; 2]) -> Option<NurbsCurve<3>> {
    if !curve.knot_vector().is_clamped() {
        return None;
    }
    let mut points = curve.points().to_vec();
    let n = points.len().checked_sub(1)?;
    for (at, p) in [0, n].into_iter().zip(to) {
        if let (Some(slot), Some(p)) = (points.get_mut(at), p) {
            *slot = [p.x, p.y, p.z];
        }
    }
    NurbsCurve::new(
        curve.knot_vector().clone(),
        points,
        curve.weights().map(<[f64]>::to_vec),
    )
    .ok()
}

/// `curve` with `found` (in order) cut; `None` when the cut knots do not make a curve.
fn cut(curve: &NurbsCurve<3>, found: &[Piece]) -> Option<NurbsCurve<3>> {
    let p = curve.degree();
    let mut knots = curve.knots().to_vec();
    let mut points = curve.points().to_vec();
    let mut weights = curve.weights().map(<[f64]>::to_vec);
    // From the last piece back, so each earlier piece's indices stand.
    for piece in found.iter().rev() {
        let (k0, k1) = (piece.span.min(), piece.span.max());
        let joint = piece.first + p;
        if let Some(w) = weights.as_mut() {
            let ratio = w.get(piece.first)? / w.get(joint)?;
            for x in w.iter_mut().skip(joint + 1) {
                *x *= ratio;
            }
            w.drain(piece.first + 1..=joint);
        }
        points.drain(piece.first + 1..=joint);
        let from = knots.partition_point(|&k| k < k1);
        knots.drain(from..from + p);
        let length = k1 - k0;
        for k in knots.iter_mut().skip(from) {
            *k = (*k - length).max(k0);
        }
    }
    NurbsCurve::new(KnotVector::new(p, knots).ok()?, points, weights).ok()
}

/// The exact image of `curve` in `plane`'s uv: the control points projected, the knots and
/// weights kept (the projection is affine, so it commutes with the rational combination).
fn on_plane(plane: &Plane, curve: &NurbsCurve<3>) -> Option<NurbsCurve<2>> {
    let f = plane.frame();
    let (o, x, y) = (f.origin(), *f.x(), *f.y());
    let points = curve
        .points()
        .iter()
        .map(|&[px, py, pz]| {
            let d = Point3::new(px, py, pz) - o;
            [d.dot(x), d.dot(y)]
        })
        .collect();
    NurbsCurve::new(
        curve.knot_vector().clone(),
        points,
        curve.weights().map(<[f64]>::to_vec),
    )
    .ok()
}

/// An upper bound on the distance of `curve` from `p` anywhere on its domain: the farthest
/// control point (the curve lies in their convex hull, its weights being positive).
pub(super) fn hull_bound(curve: &NurbsCurve<3>, p: Point3) -> Option<f64> {
    curve.points().iter().try_fold(0.0_f64, |far, &[x, y, z]| {
        Some(far.max(distance(Point3::new(x, y, z), p)?.upper()))
    })
}

/// The diameter of `curve` over its domain: below by the check's samples (each against the
/// first and the middle one), above by the control points' box.
fn diameter(curve: &Curve, n: &NurbsCurve<3>) -> Option<Bracket> {
    let domain = n.domain();
    let (lo, hi) = (domain.min(), domain.max());
    let last = DIAMETER_SAMPLES - 1;
    let samples: Vec<Point3<Ivl>> = (0..DIAMETER_SAMPLES)
        .map(|k| {
            #[expect(clippy::cast_precision_loss, reason = "sample indices are below ten")]
            let s = k as f64 / last as f64;
            curve.point_over(Ivl::from(lo + (hi - lo) * s))
        })
        .collect();
    let mut lower = 0.0_f64;
    for p in [samples.first(), samples.get(DIAMETER_SAMPLES / 2)]
        .into_iter()
        .flatten()
    {
        for q in &samples {
            if let Some(d) = norm(*q - *p) {
                lower = lower.max(d.lower());
            }
        }
    }
    let mut lo3 = [f64::INFINITY; 3];
    let mut hi3 = [f64::NEG_INFINITY; 3];
    for c in n.points() {
        for ((l, h), &x) in lo3.iter_mut().zip(hi3.iter_mut()).zip(c) {
            *l = l.min(x);
            *h = h.max(x);
        }
    }
    let [a, b] = [lo3, hi3].map(|[x, y, z]| Point3::new(x, y, z));
    let upper = distance(a, b)?.upper();
    Bracket::new(lower, upper.max(lower))
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

impl State<'_> {
    /// The NURBS curve and interval of edge `e` as the items done left it.
    fn nurbs_now(&self, e: Edge<'_>) -> Option<(Arc<NurbsCurve<3>>, Extent1d)> {
        if let Some(r) = self.reshaped.get(&e.id()) {
            return Some((Arc::clone(&r.curve), r.interval()));
        }
        match e.shape() {
            EdgeShape::Curve {
                curve: Curve::Nurbs(n),
                interval,
            } => Some((Arc::clone(n), *interval)),
            _ => None,
        }
    }

    /// The curve and interval of edge `e` as the items done left it.
    pub(super) fn shape_now(&self, e: Edge<'_>) -> Option<(Curve, Extent1d)> {
        if let Some((n, interval)) = self.nurbs_now(e) {
            return Some((Curve::Nurbs(n), interval));
        }
        match e.shape() {
            EdgeShape::Curve { curve, interval } => Some((curve.clone(), *interval)),
            EdgeShape::Collapsed => None,
        }
    }

    /// The NURBS edge `e` at cluster `c` re-ended at the target, certified; `Ok(None)` when
    /// the budget runs out or the caller stops.
    pub(super) fn re_end_nurbs(
        &self,
        c: &Cluster,
        members: &BTreeSet<VertexId>,
        e: Edge<'_>,
        budget: &mut Budget,
    ) -> Result<Option<Reshaped>, HealCause> {
        let id = e.id();
        let kind = HealCause::NeighbourKind { edge: id };
        let (curve, interval) = self.nurbs_now(e).ok_or(kind)?;
        let trimmed = trim(&curve, interval).ok_or(kind)?;
        let mut ends = [(c.survivor, c.target); 2];
        let mut to = [None; 2];
        for ((slot, t), v) in ends.iter_mut().zip(to.iter_mut()).zip(e.ends()) {
            if members.contains(&v) {
                *t = Some(c.target);
            } else {
                *slot = self.now(v).ok_or(kind)?;
            }
        }
        let curve = moved(&trimmed, to).ok_or(kind)?;
        self.fit(e, curve, ends, budget)
    }

    /// The empty pieces of `item` cut, certified; `Ok(None)` when the budget runs out or
    /// the caller stops.
    pub(super) fn cut_pieces(
        &self,
        item: &EmptyPieces,
        budget: &mut Budget,
    ) -> Result<Option<Reshaped>, HealCause> {
        let id = item.edge;
        let kind = HealCause::NeighbourKind { edge: id };
        let e = self.body.edge(id).ok_or(kind)?;
        let (curve, interval) = self.nurbs_now(e).ok_or(kind)?;
        let trimmed = trim(&curve, interval).ok_or(kind)?;
        let re = reach(e.accuracy(), self.res);
        charge!(budget, trimmed.points().len());
        let found = pieces(&trimmed, trimmed.domain(), re);
        let (lower, upper) = found.iter().fold((0.0, 0.0), |(l, u), x| {
            (l + x.spread.lower(), u + x.spread.upper())
        });
        let not_finite = || HealCause::NotFinite { at: id.into() };
        let bound = Bracket::new(lower, upper.max(lower)).ok_or_else(not_finite)?;
        if found.is_empty() || bound.upper() > re {
            return Err(HealCause::PiecesSpread {
                edge: id,
                bound,
                allowed: Length::from_mm(re),
            });
        }
        let curve = cut(&trimmed, &found).ok_or(HealCause::Cut { edge: id })?;
        let [a, b] = e.ends();
        let ends = [
            self.now(a).ok_or_else(not_finite)?,
            self.now(b).ok_or_else(not_finite)?,
        ];
        self.fit(e, curve, ends, budget)
    }

    /// `curve` for edge `e` between `ends`, with each use's pcurve projected, certified on
    /// the check's gaps.
    fn fit(
        &self,
        e: Edge<'_>,
        curve: NurbsCurve<3>,
        ends: [(VertexId, Point3); 2],
        budget: &mut Budget,
    ) -> Result<Option<Reshaped>, HealCause> {
        let id = e.id();
        let not_finite = || HealCause::NotFinite { at: id.into() };
        let re = reach(e.accuracy(), self.res);
        let curve = Arc::new(curve);
        let interval = curve.domain();
        let whole = Curve::Nurbs(Arc::clone(&curve));
        charge!(budget, 2);
        for (&(v, p), t) in ends.iter().zip([interval.min(), interval.max()]) {
            let allowed = re + self.vertex_reach(v);
            let gap = scx_surface::vertex_curve_gap(p, &whole, t).map_err(|_| not_finite())?;
            if gap.exceeds(allowed) != Some(false) {
                return Err(HealCause::EndOffVertex {
                    edge: id,
                    gap,
                    allowed: Length::from_mm(allowed),
                });
            }
        }
        charge!(budget, DIAMETER_SAMPLES);
        let length = diameter(&whole, &curve).ok_or_else(not_finite)?;
        if length.lower() <= re {
            return Err(HealCause::NeighbourShort {
                edge: id,
                length,
                allowed: Length::from_mm(re),
            });
        }
        let mut pcurves = Vec::new();
        for u in e.uses() {
            let face = u.face();
            let surface = face.surface();
            let Surface::Plane(plane) = surface else {
                return Err(HealCause::FaceKind { face: face.id() });
            };
            if !matches!(u.geom(), UseGeom::Pcurve { .. }) {
                return Err(HealCause::FaceKind { face: face.id() });
            }
            let pcurve = Curve2::Nurbs(Arc::new(on_plane(plane, &curve).ok_or_else(not_finite)?));
            let allowed = re.min(reach(u.bound(), self.res));
            let aim = Aim::Decide {
                allowance: allowed,
                width: NEAR * allowed,
            };
            let off = |gap| HealCause::OffFace {
                edge: id,
                face: face.id(),
                gap,
                allowed: Length::from_mm(allowed),
            };
            match scx_surface::pcurve_edge_gap(
                surface, &pcurve, &whole, &interval, aim, budget, self.stop,
            ) {
                Ok(gap) if gap.exceeds(allowed) == Some(false) => {}
                Ok(gap) => return Err(off(gap)),
                Err(GapError::Undecided {
                    why: Short::Exhausted(_) | Short::Stopped,
                    ..
                }) => return Ok(None),
                Err(GapError::Undecided { reached, .. }) => return Err(off(reached)),
                Err(_) => return Err(not_finite()),
            }
            pcurves.push((face.id(), u.sense(), pcurve));
        }
        Ok(Some(Reshaped {
            ends: ends.map(|(v, _)| v),
            curve,
            pcurves,
        }))
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::indexing_slicing)]

    use super::*;
    use scx_nurbs::Approach;

    fn at(c: &NurbsCurve<3>, u: f64) -> [f64; 3] {
        c.point(u, Approach::Above).unwrap()
    }

    /// A degree-2 curve with C0 joints at 1 and 1.5 and the piece between them within
    /// `1e-6` of a point: cut, the curve keeps its first piece exactly and its last moved
    /// by at most the spread, over a domain `0.5` shorter.
    fn joined(weights: Option<Vec<f64>>) -> NurbsCurve<3> {
        let knots = vec![0.0, 0.0, 0.0, 1.0, 1.0, 1.5, 1.5, 3.0, 3.0, 3.0];
        let points = vec![
            [0.0, 0.0, 0.0],
            [0.5, 1.0, 0.0],
            [1.0, 1.0, 0.0],
            [1.0, 1.0 + 1e-7, 0.0],
            [1.0 + 1e-7, 1.0, 0.0],
            [2.0, 0.0, 0.0],
            [3.0, 0.0, 0.0],
        ];
        NurbsCurve::new(KnotVector::new(2, knots).unwrap(), points, weights).unwrap()
    }

    fn check_cut(c: &NurbsCurve<3>) {
        let domain = c.domain();
        let found = pieces(c, domain, 1e-6);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].span, Extent1d::new(1.0, 1.5).unwrap());
        let out = cut(c, &found).unwrap();
        assert_eq!(out.domain(), Extent1d::new(0.0, 2.5).unwrap());
        assert_eq!(out.points().len(), c.points().len() - 2);
        for k in 0..=10 {
            let u = f64::from(k) / 10.0;
            if k < 10 {
                assert_eq!(at(&out, u), at(c, u));
            }
            let (p, q) = (at(&out, 1.0 + 1.5 * u), at(c, 1.5 + 1.5 * u));
            let d = (0..3)
                .map(|i| (p[i] - q[i]) * (p[i] - q[i]))
                .sum::<f64>()
                .sqrt();
            assert!(d <= found[0].spread.upper() + 1e-15, "{u} {d}");
        }
    }

    #[test]
    fn an_empty_piece_is_cut() {
        check_cut(&joined(None));
    }

    #[test]
    fn an_empty_piece_of_unequal_weights_is_cut() {
        check_cut(&joined(Some(vec![1.0, 0.7, 1.3, 2.0, 0.6, 1.1, 0.9])));
    }

    #[test]
    fn a_piece_past_the_reach_is_kept() {
        assert!(pieces(&joined(None), Extent1d::new(0.0, 3.0).unwrap(), 1e-8).is_empty());
    }
}
