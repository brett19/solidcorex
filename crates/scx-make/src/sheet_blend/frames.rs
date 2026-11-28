// Written for solidcorex. Apache-2.0.
//! The frames a section blend takes: from a polyline, or from a chain of a body's edges with
//! the support face's shape read along the leave direction, and the turns that pair open
//! sections end to nearer end.
//!
//! **Stations** are at even arc length along the chain or polyline, its two ends included. An
//! edge's arc length is a five-point Gauss rule over 16 equal pieces of its interval (exact for
//! a line and a circle, whose speeds are constant here, and to rounding for a smooth curve's
//! placement, which only places the stations); a station's parameter is found by bisection on
//! it. The placement is the sampling of the section, not a truth: the blend passes through
//! whatever stations it is given.
//!
//! **A frame** is the station's point, the leave direction in the support face's tangent plane
//! (`w`), and the curvature vector and its arc-length derivative of the face's curve through
//! the point along `w`. The uv is the support face's use's pcurve at the edge's parameter (the
//! pin projected the point back onto the surface); `S_u du + S_v dv = w` is solved by its
//! normal equations, and the derivatives up to third order are read on the side of each
//! parameter `w` leaves into, so a knot that changes the parameter's speed is read on the
//! outgoing span.

use scx_brep::{Brep, Document, EdgeId, EdgeShape, FaceId, UseGeom};
use scx_curve::Curve;
use scx_num::extent1d::Extent1d;
use scx_num::frame::Sense;
use scx_num::vector2d::Point2;
use scx_num::vector3d::{Point3, Vec3};
use scx_nurbs::Approach;
use scx_surface::Surface;

use super::error::{SheetBlendError, SheetBlendRefusal, SheetBlendUnsupported, refused};
use crate::blend::{MIN_STATIONS, SectionFrame};

/// Pieces of an edge's interval its arc length is summed over.
const PIECES: usize = 16;
/// The five-point Gauss-Legendre nodes on `[-1, 1]` and their weights.
const GAUSS: [(f64, f64); 5] = [
    (0.0, 0.568_888_888_888_888_9),
    (-0.538_469_310_105_683_1, 0.478_628_670_499_366_5),
    (0.538_469_310_105_683_1, 0.478_628_670_499_366_5),
    (-0.906_179_845_938_664, 0.236_926_885_056_189_1),
    (0.906_179_845_938_664, 0.236_926_885_056_189_1),
];
/// Bisection steps placing a station within a piece: the piece's interval halved to below
/// one rounding of it.
const BISECTIONS: usize = 60;

fn finite(v: Vec3) -> bool {
    v.x.is_finite() && v.y.is_finite() && v.z.is_finite()
}

/// The frames of a section given by `points` in order, the polyline through them: at
/// `stations` stations of even arc length along it, its first and last points included, each
/// the station's point with `leave` as given and no bend (nothing lies behind such a section
/// for the blend to continue). A point repeating the one before adds no length.
///
/// # Errors
///
/// [`SheetBlendRefusal::Stations`] for fewer than two stations or points;
/// [`SheetBlendRefusal::NotFinite`] for a point or `leave` that is not finite, or a polyline
/// whose length is not positive and finite.
pub fn point_frames(
    points: &[Point3],
    leave: Vec3,
    stations: usize,
) -> Result<Vec<SectionFrame>, SheetBlendError> {
    if stations < MIN_STATIONS {
        return Err(refused(SheetBlendRefusal::Stations { count: stations }));
    }
    if points.len() < 2 {
        return Err(refused(SheetBlendRefusal::Stations {
            count: points.len(),
        }));
    }
    if !(finite(leave) && points.iter().all(|p| p.is_finite())) {
        return Err(refused(SheetBlendRefusal::NotFinite));
    }
    let lengths: Vec<f64> = points.windows(2).map(dist).collect();
    let total: f64 = lengths.iter().sum();
    if !(total.is_finite() && total > 0.0) {
        return Err(refused(SheetBlendRefusal::NotFinite));
    }
    let frame = |p| SectionFrame {
        leave,
        ..SectionFrame::at(p)
    };
    let (mut walked, mut seg) = (0.0, 0);
    let mut out = Vec::with_capacity(stations);
    #[allow(clippy::cast_precision_loss)] // a count of stations
    let last = (stations - 1) as f64;
    for i in 0..stations {
        #[allow(clippy::cast_precision_loss)] // a count of stations
        let want = total * i as f64 / last;
        while let Some(&l) = lengths.get(seg) {
            if walked + l < want && seg + 2 < points.len() {
                walked += l;
                seg += 1;
            } else {
                break;
            }
        }
        let p = match (i, points.get(seg), points.get(seg + 1), lengths.get(seg)) {
            (0, ..) => points.first().copied(),
            (i, ..) if i + 1 == stations => points.last().copied(),
            (_, Some(&a), Some(&b), Some(&l)) if l > 0.0 => {
                Some(a.lerp(b, ((want - walked) / l).clamp(0.0, 1.0)))
            }
            (_, Some(&a), ..) => Some(a),
            _ => None,
        };
        out.push(frame(p.ok_or(SheetBlendError::Defect {
            why: "a station past the polyline",
        })?));
    }
    Ok(out)
}

fn dist(w: &[Point3]) -> f64 {
    match w {
        [a, b] => a.distance(*b),
        _ => 0.0,
    }
}

/// Which open sections to turn so each runs the way the one before it does (after its own
/// turn): section `i` with ends `s, e` is turned when `|s - s'| + |e - e'| > |s - e'| + |e - s'|`
/// for the one before's ends `s', e'`. The first is never turned.
///
/// # Errors
///
/// [`SheetBlendRefusal::NotFinite`] for an end that is not finite;
/// [`SheetBlendUnsupported::ClosedSection`] for a section whose ends are within the
/// document's resolution of each other.
pub fn open_section_turns(
    ends: &[[Point3; 2]],
    doc: &Document,
) -> Result<Vec<bool>, SheetBlendError> {
    let zone = doc.resolution().zone().mm();
    let mut before: Option<[Point3; 2]> = None;
    let mut out = Vec::with_capacity(ends.len());
    for (section, &[s, e]) in ends.iter().enumerate() {
        if !(s.is_finite() && e.is_finite()) {
            return Err(refused(SheetBlendRefusal::NotFinite));
        }
        if s.distance(e) <= zone {
            return Err(SheetBlendError::Unsupported(
                SheetBlendUnsupported::ClosedSection { section },
            ));
        }
        let turn = before.is_some_and(|[ps, pe]| {
            s.distance(ps) + e.distance(pe) > s.distance(pe) + e.distance(ps)
        });
        before = Some(if turn { [e, s] } else { [s, e] });
        out.push(turn);
    }
    Ok(out)
}

/// The frames of a section that is the edge chain `chain` of `body`, leaving along `leave`:
/// at `stations` stations of even arc length, the chain's point and the shape of the support
/// face's curve along `leave` projected into its tangent plane.
///
/// The support face is the face bounding every chain edge whose tangent plane at the first
/// edge's middle keeps most of `leave` (the length of its projection, `leave` taken unit); it
/// must keep at least `min_in_plane` (in `[0, 1]`).
///
/// # Errors
///
/// [`SheetBlendRefusal::Stations`], [`SheetBlendRefusal::NotFinite`] for a `leave` or
/// `min_in_plane` that is not finite, of no length or outside `[0, 1]`; the chain's refusals
/// ([`SheetBlendRefusal::NoChain`], [`SheetBlendRefusal::NotEdge`],
/// [`SheetBlendRefusal::NotChained`], [`SheetBlendRefusal::NoFace`]);
/// [`SheetBlendRefusal::NoSupport`] when no face keeps `min_in_plane` of `leave`, or `leave`
/// is normal to the support face at a station.
pub fn section_frames(
    body: &Brep,
    chain: &[EdgeId],
    leave: Vec3,
    stations: usize,
    min_in_plane: f64,
) -> Result<Vec<SectionFrame>, SheetBlendError> {
    if stations < MIN_STATIONS {
        return Err(refused(SheetBlendRefusal::Stations { count: stations }));
    }
    if !(finite(leave) && min_in_plane.is_finite() && (0.0..=1.0).contains(&min_in_plane)) {
        return Err(refused(SheetBlendRefusal::NotFinite));
    }
    let leave = leave
        .normalized()
        .map(Vec3::from)
        .ok_or(refused(SheetBlendRefusal::NotFinite))?;
    let walk = Walk::new(body, chain)?;
    let mut best: Option<(f64, FaceId)> = None;
    for &f in &walk.faces {
        let Some(n) = walk.normal_at_middle(body, f) else {
            continue;
        };
        let keeps = (leave - n * leave.dot(n)).length();
        if best.is_none_or(|(s, _)| keeps > s) {
            best = Some((keeps, f));
        }
    }
    let face = match best {
        Some((keeps, f)) if keeps >= min_in_plane && keeps > 0.0 => f,
        _ => return Err(refused(SheetBlendRefusal::NoSupport)),
    };
    walk.stations(stations)?
        .iter()
        .map(|s| {
            let (surface, uv, _) = walk.on_face(body, face, s)?;
            frame_at(surface, uv, s.p, leave)
        })
        .collect()
}

/// The frames of a section that is the edge chain `chain` of `body`, leaving the support face
/// outward across the chain: as [`section_frames`], with the leave direction at each station
/// the face's own direction out across the edge (square to it in the face's tangent plane,
/// away from the face), so a blend through the frames continues the face past its edge.
///
/// The support face is the face bounding every chain edge; where several do, the one whose
/// outward direction at the first edge's middle runs furthest along `toward` (the way the
/// blend goes). A face using an edge twice (a seam) is left across its first use.
///
/// # Errors
///
/// As [`section_frames`], with [`SheetBlendRefusal::NotFinite`] for a `toward` that is not
/// finite or of no length.
pub fn section_frames_outward(
    body: &Brep,
    chain: &[EdgeId],
    toward: Vec3,
    stations: usize,
) -> Result<Vec<SectionFrame>, SheetBlendError> {
    if stations < MIN_STATIONS {
        return Err(refused(SheetBlendRefusal::Stations { count: stations }));
    }
    let toward = Some(toward)
        .filter(|&t| finite(t))
        .and_then(Vec3::normalized)
        .map(Vec3::from)
        .ok_or(refused(SheetBlendRefusal::NotFinite))?;
    let walk = Walk::new(body, chain)?;
    let middle = walk.first_middle();
    let mut best: Option<(f64, FaceId)> = None;
    for &f in &walk.faces {
        let Ok((surface, uv, tangent)) = walk.on_face(body, f, &middle) else {
            continue;
        };
        let Some(out) = outward(surface, uv, tangent) else {
            continue;
        };
        let along = out.dot(toward);
        if best.is_none_or(|(s, _)| along > s) {
            best = Some((along, f));
        }
    }
    let (_, face) = best.ok_or(refused(SheetBlendRefusal::NoFace))?;
    walk.stations(stations)?
        .iter()
        .map(|s| {
            let (surface, uv, tangent) = walk.on_face(body, face, s)?;
            let w = outward(surface, uv, tangent).ok_or(refused(SheetBlendRefusal::NoSupport))?;
            frame_at(surface, uv, s.p, w)
        })
        .collect()
}

/// The direction out of a face across an edge whose use runs along `tangent` (in the use's
/// sense) at `uv`: the loops run counter-clockwise about the surface's natural normal `n`,
/// so the face is on the left, `n x tangent`, and outward is `tangent x n`.
pub(super) fn outward(surface: &Surface, uv: Point2, tangent: Vec3) -> Option<Vec3> {
    let n = Vec3::from(surface.normal(uv, [Approach::Above; 2])?);
    let out = tangent.cross(n);
    out.normalized().map(Vec3::from)
}

/// One edge of a walked chain.
#[derive(Debug, Clone)]
struct Run {
    edge: EdgeId,
    curve: Curve,
    interval: Extent1d,
    /// Whether the chain runs against the edge's direction.
    backwards: bool,
    /// The arc length at each piece's end, from the run's start in the chain's direction.
    lengths: [f64; PIECES],
}

/// A station of a chain: its run, the edge's parameter and the point.
#[derive(Debug, Clone, Copy)]
pub(super) struct Station {
    run: usize,
    t: f64,
    pub(super) p: Point3,
}

/// An edge chain walked for its stations.
pub(super) struct Walk {
    runs: Vec<Run>,
    /// The faces bounding every edge, in id order.
    pub(super) faces: Vec<FaceId>,
}

impl Walk {
    /// Walks `chain` of `body`: each edge turned to run on from the one before (the first to
    /// run into the second), by its vertices.
    pub(super) fn new(body: &Brep, chain: &[EdgeId]) -> Result<Self, SheetBlendError> {
        let first = *chain.first().ok_or(refused(SheetBlendRefusal::NoChain))?;
        let edge_of = |e: EdgeId| {
            body.edge(e)
                .ok_or_else(|| refused(SheetBlendRefusal::NotEdge(e)))
        };
        let mut runs: Vec<Run> = Vec::with_capacity(chain.len());
        let mut at = None;
        let mut faces: Option<Vec<FaceId>> = None;
        for (i, &e) in chain.iter().enumerate() {
            let edge = edge_of(e)?;
            let [s, t] = edge.ends();
            let backwards = match at {
                Some(v) if v == s => false,
                Some(v) if v == t => true,
                Some(_) => return Err(refused(SheetBlendRefusal::NotChained { at: i })),
                None => match chain.get(1) {
                    Some(&next) => {
                        let [ns, nt] = edge_of(next)?.ends();
                        if t == ns || t == nt {
                            false
                        } else if s == ns || s == nt {
                            true
                        } else {
                            return Err(refused(SheetBlendRefusal::NotChained { at: 1 }));
                        }
                    }
                    None => false,
                },
            };
            at = Some(if backwards { s } else { t });
            let EdgeShape::Curve { curve, interval } = edge.shape() else {
                return Err(refused(SheetBlendRefusal::NotEdge(e)));
            };
            let mut own: Vec<FaceId> = edge.fins().map(|f| f.face).collect();
            own.sort_unstable();
            own.dedup();
            faces = Some(match faces {
                None => own,
                Some(f) => f.into_iter().filter(|x| own.contains(x)).collect(),
            });
            runs.push(Run {
                edge: e,
                curve: curve.clone(),
                interval: *interval,
                backwards,
                lengths: piece_lengths(curve, *interval, backwards),
            });
        }
        let faces = faces.unwrap_or_default();
        if faces.is_empty() {
            return Err(refused(SheetBlendRefusal::NoFace));
        }
        let _ = first;
        Ok(Self { runs, faces })
    }

    /// The first edge's middle (by parameter), as a station.
    pub(super) fn first_middle(&self) -> Station {
        let (t, p) = self
            .runs
            .first()
            .map_or((0.0, Point3::new(0.0, 0.0, 0.0)), |r| {
                let t = 0.5 * (r.interval.min() + r.interval.max());
                (t, r.curve.point(t))
            });
        Station { run: 0, t, p }
    }

    /// The total arc length.
    pub(super) fn length(&self) -> f64 {
        self.runs
            .iter()
            .map(|r| r.lengths.last().copied().unwrap_or(0.0))
            .sum()
    }

    /// `count` stations at even arc length, the chain's ends included.
    pub(super) fn stations(&self, count: usize) -> Result<Vec<Station>, SheetBlendError> {
        let total = self.length();
        if !(total.is_finite() && total > 0.0) {
            return Err(refused(SheetBlendRefusal::NotFinite));
        }
        let mut out = Vec::with_capacity(count);
        #[allow(clippy::cast_precision_loss)] // a count of stations
        let last = (count - 1) as f64;
        let (mut run, mut walked) = (0, 0.0);
        for i in 0..count {
            #[allow(clippy::cast_precision_loss)] // a count of stations
            let want = total * i as f64 / last;
            while let Some(r) = self.runs.get(run) {
                let l = r.lengths.last().copied().unwrap_or(0.0);
                if walked + l < want && run + 1 < self.runs.len() {
                    walked += l;
                    run += 1;
                } else {
                    break;
                }
            }
            let r = self.runs.get(run).ok_or(SheetBlendError::Defect {
                why: "a station past the chain",
            })?;
            let t = if i == 0 {
                r.chain_start()
            } else if i + 1 == count {
                r.chain_end()
            } else {
                r.parameter_at(want - walked)
            };
            out.push(Station {
                run,
                t,
                p: r.curve.point(t),
            });
        }
        Ok(out)
    }

    /// The support face's surface, the station's uv on it (its use's pcurve at the edge's
    /// parameter) and the edge's tangent there in the use's sense.
    pub(super) fn on_face<'b>(
        &self,
        body: &'b Brep,
        face: FaceId,
        s: &Station,
    ) -> Result<(&'b Surface, Point2, Vec3), SheetBlendError> {
        let run = self.runs.get(s.run).ok_or(SheetBlendError::Defect {
            why: "a station's run",
        })?;
        let f = body.face(face).ok_or(refused(SheetBlendRefusal::NoFace))?;
        let found = f.uses().find(|u| u.record().edge == run.edge);
        let u = found.ok_or(refused(SheetBlendRefusal::NoFace))?;
        let uv = match u.geom() {
            UseGeom::Pcurve { curve, .. } => curve.point(s.t),
            UseGeom::Point(p) => *p,
        };
        let d = run.curve.jet::<1>(s.t, Approach::Above).d[0];
        let tangent = match u.sense() {
            Sense::Forward => d,
            Sense::Reversed => -d,
        };
        Ok((f.surface(), uv, tangent))
    }

    /// The support face's natural normal at the first edge's middle.
    fn normal_at_middle(&self, body: &Brep, face: FaceId) -> Option<Vec3> {
        let (surface, uv, _) = self.on_face(body, face, &self.first_middle()).ok()?;
        surface.normal(uv, [Approach::Above; 2]).map(Vec3::from)
    }
}

impl Run {
    const fn chain_start(&self) -> f64 {
        if self.backwards {
            self.interval.max()
        } else {
            self.interval.min()
        }
    }

    const fn chain_end(&self) -> f64 {
        if self.backwards {
            self.interval.min()
        } else {
            self.interval.max()
        }
    }

    /// The parameter `length` along the run from its chain start.
    fn parameter_at(&self, length: f64) -> f64 {
        let piece = self
            .lengths
            .iter()
            .position(|&l| l >= length)
            .unwrap_or(PIECES - 1);
        let before = match piece {
            0 => 0.0,
            k => self.lengths.get(k - 1).copied().unwrap_or(0.0),
        };
        let (a, b) = piece_ends(self.interval, self.backwards, piece);
        let (mut lo, mut hi) = (a, b);
        for _ in 0..BISECTIONS {
            let mid = 0.5 * (lo + hi);
            if before + arc(&self.curve, a, mid) < length {
                lo = mid;
            } else {
                hi = mid;
            }
        }
        0.5 * (lo + hi)
    }
}

/// Piece `k`'s parameters, in the chain's direction.
fn piece_ends(interval: Extent1d, backwards: bool, k: usize) -> (f64, f64) {
    let (t0, t1) = if backwards {
        (interval.max(), interval.min())
    } else {
        (interval.min(), interval.max())
    };
    #[allow(clippy::cast_precision_loss)] // small counts
    let at = |i: usize| t0 + (t1 - t0) * (i as f64 / PIECES as f64);
    (at(k), at(k + 1))
}

fn piece_lengths(curve: &Curve, interval: Extent1d, backwards: bool) -> [f64; PIECES] {
    let mut out = [0.0; PIECES];
    let mut sum = 0.0;
    for (k, o) in out.iter_mut().enumerate() {
        let (a, b) = piece_ends(interval, backwards, k);
        sum += arc(curve, a, b);
        *o = sum;
    }
    out
}

/// The arc length from `a` to `b` (either order), by the five-point Gauss rule.
fn arc(curve: &Curve, a: f64, b: f64) -> f64 {
    let (m, h) = (0.5 * (a + b), 0.5 * (b - a));
    let sum: f64 = GAUSS
        .iter()
        .map(|&(x, w)| w * curve.jet::<1>(m + h * x, Approach::Above).d[0].length())
        .sum();
    sum * h.abs()
}

/// The frame at `p` (on `surface` at `uv`) leaving along `leave` projected into the tangent
/// plane.
pub(super) fn frame_at(
    surface: &Surface,
    uv: Point2,
    p: Point3,
    leave: Vec3,
) -> Result<SectionFrame, SheetBlendError> {
    let mut sides = [Approach::Above; 2];
    for _ in 0..4 {
        let j = surface.jet::<3>(uv, sides);
        let at = |i: usize, k: usize| {
            j.get(i, k).ok_or(SheetBlendError::Defect {
                why: "a surface jet's partial",
            })
        };
        let (su, sv) = (at(1, 0)?, at(0, 1)?);
        let n = su
            .cross(sv)
            .normalized()
            .map(Vec3::from)
            .ok_or(refused(SheetBlendRefusal::NoSupport))?;
        let w = (leave - n * leave.dot(n))
            .normalized()
            .map(Vec3::from)
            .ok_or(refused(SheetBlendRefusal::NoSupport))?;
        let (a, b, c) = (su.dot(su), su.dot(sv), sv.dot(sv));
        let det = a * c - b * b;
        if det.is_nan() || det <= 0.0 {
            return Err(refused(SheetBlendRefusal::NoSupport));
        }
        let du = (c * w.dot(su) - b * w.dot(sv)) / det;
        let dv = (a * w.dot(sv) - b * w.dot(su)) / det;
        let side = |d: f64, current| {
            if d < 0.0 {
                Approach::Below
            } else if d > 0.0 {
                Approach::Above
            } else {
                current
            }
        };
        let outgoing = [side(du, sides[0]), side(dv, sides[1])];
        if outgoing != sides {
            sides = outgoing;
            continue;
        }
        let c1 = su * du + sv * dv;
        let c2 = at(2, 0)? * (du * du) + at(1, 1)? * (2.0 * du * dv) + at(0, 2)? * (dv * dv);
        let c3 = at(3, 0)? * (du * du * du)
            + at(2, 1)? * (3.0 * du * du * dv)
            + at(1, 2)? * (3.0 * du * dv * dv)
            + at(0, 3)? * (dv * dv * dv);
        let (bend, bend_change) = shape(c1, c2, c3).ok_or(refused(SheetBlendRefusal::NoSupport))?;
        if !(finite(bend) && finite(bend_change)) {
            return Err(refused(SheetBlendRefusal::NotFinite));
        }
        return Ok(SectionFrame {
            p,
            leave: w,
            bend,
            bend_change,
        });
    }
    Err(SheetBlendError::Defect {
        why: "no outgoing span is consistent",
    })
}

/// The curvature vector of a curve with derivatives `c1`, `c2`, `c3` and its arc-length
/// derivative: with `sigma = |c1|` and `t = c1 / sigma`, `kappa = (c2 - t (t . c2)) / sigma^2`
/// and `d kappa / ds = (d kappa / d tau) / sigma`, each factor differentiated
/// (`sigma' = t . c2`, `t' = (c2 - t (t . c2)) / sigma`). `None` for a tangent of no length.
fn shape(c1: Vec3, c2: Vec3, c3: Vec3) -> Option<(Vec3, Vec3)> {
    let sigma = c1.length();
    if sigma.is_nan() || sigma <= 0.0 {
        return None;
    }
    let t = c1 * (1.0 / sigma);
    let a2 = t.dot(c2);
    let across = c2 - t * a2;
    let kappa = across * (1.0 / (sigma * sigma));
    let dt = across * (1.0 / sigma);
    let da2 = dt.dot(c2) + t.dot(c3);
    let dacross = c3 - dt * a2 - t * da2;
    let dkappa = dacross * (1.0 / (sigma * sigma)) - across * (2.0 * a2 / (sigma * sigma * sigma));
    Some((kappa, dkappa * (1.0 / sigma)))
}
