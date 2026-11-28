// Written for solidcorex. Apache-2.0.
//! A body's outline on its edges under a view (research 74 §5.3 (C), task HL3): the
//! stretches of each edge between a face that faces the viewer and one that does not, and
//! of an open sheet's border where its face is seen.
//!
//! **What faces the viewer.** A face's normal out of the material, `n`, against `v = -d`
//! (toward the viewer): it faces where `n · v > FACING`. The material is the solid region
//! beside one side of the face (the front side's normal points into the front region, so
//! out of the material is `-n` when the front region is solid and `n` when the back one
//! is); a sheet (neither side solid) is seen from either side, so it faces where
//! `|n · v| > FACING`; a face between two solid regions is never seen. Edge-on is away.
//! Where a face is edge-on at its edge (`|n · v| <= EDGE_ON`: the end of a round seen along
//! the round, or an edge that is itself a silhouette generator) it faces as it does a hair
//! inside it, [`STEP`] of its size from the edge, the side that leans more deciding.
//!
//! **Where the state changes, exactly.** Along an edge a face's `n · v` is zero only where
//! the edge meets the surface's silhouette. On a cylinder, cone or sphere that curve lies
//! in a plane found in closed form (`silhouette`'s splitting planes); on a torus the edge
//! is a meridian or a parallel and `n · v = 0` there is a closed-form angle, cut by a
//! plane through it (`contour::edge_planes`). The edge's crossings with those planes
//! (`scx_surface::intersect_curve`, complete for analytic curves) are every place a face's
//! state can change: between two consecutive ones the state is constant, and one point
//! decides it. A crossing too many splits a stretch in two that are then merged. The pin
//! looks at 32 even samples of the parameter and bisects, so a stretch between two
//! samples (a face that turns toward the viewer and away again within a 32nd of an edge)
//! is missed: not here.
//!
//! This is the whole outline, not the visible lines: a stretch is reported whether or not
//! another part hides it, and an edge between two faces that both face the viewer (a
//! crease seen from outside) is not, however sharp. Visible lines are
//! [`Scene::hidden_lines`](crate::Scene::hidden_lines).
//!
//! **Images.** [`Outline::images`] gives each stretch's exact image on the screen as
//! `scx_planar::Curve2` pieces (a line, or conic arcs: a circle at a slant is an exact
//! rational quadratic, seen edge-on a line), and [`drop_coincident`] draws coincident
//! pieces once: the pin's rational-spline and circle tidying (`conic_spans`, `flat_conic`,
//! `round_conics`) has nothing left to repair because no image is sampled.

use std::collections::btree_map::Entry;
use std::collections::{BTreeMap, BTreeSet};

use scx_brep::op::{Cx, Event, Trace};
use scx_brep::{Brep, EdgeId, EdgeShape, Face, FaceId, FaceSide, UvSide};
use scx_curve::Curve;
use scx_num::budget::{Budget, Stop};
use scx_num::extent1d::Extent1d;
use scx_num::frame::Frame;
use scx_num::invert::Inversion;
use scx_num::units::Resolution;
use scx_num::vector2d::Point2;
use scx_num::vector3d::UnitVec3;
use scx_num::vector3d::{Point3, Vec3};
use scx_nurbs::Approach;
use scx_planar::Curve2;
use scx_surface::CurveSurfaceHit;

use crate::contour;
use crate::error::HlrError;
use crate::project::{Lens, NoImage, pieces};
use crate::silhouette::{
    Quadric, Rim, Run, Splitter, Undecided, Why, plane_hits, rims, ruled_or_round,
};
use crate::tolerance::ROUNDING;
use crate::view::{View, place_point};
use crate::visibility::analytic_normal;

/// Least lean toward the viewer (the cosine of the normal's angle with the view) that faces
/// it.
pub const FACING: f64 = 1e-9;
/// Below this lean a face is taken for edge-on at an edge and asked how it leans a hair
/// inside.
pub const EDGE_ON: f64 = 1e-6;
/// The step inside a face, as a fraction of the face's size.
pub const STEP: f64 = 1e-5;

/// A body's outline stretches, and the edges whose outline is not found.
#[derive(Debug, Clone, PartialEq)]
pub struct Outline {
    /// The stretches, edge by edge in id order and along each edge in parameter order.
    pub stretches: Vec<Stretch>,
    /// The edges left out, each with the reason. Whether they bound the outline is not
    /// known.
    pub undecided: Vec<EdgeUndecided>,
}

/// One stretch of an edge on the outline.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Stretch {
    /// The edge.
    pub edge: EdgeId,
    /// The stretch's interval on the edge's own parameter.
    pub range: Extent1d,
    /// The points at the ends of `range`.
    pub ends: [Point3; 2],
    /// It is the whole edge.
    pub whole: bool,
}

/// An edge whose outline is not found.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct EdgeUndecided {
    /// The edge.
    pub edge: EdgeId,
    /// Why.
    pub why: OutlineUndecided,
}

/// Why an edge's outline is not found.
#[derive(Debug, Clone, Copy, PartialEq)]
#[non_exhaustive]
pub enum OutlineUndecided {
    /// More than two faces use the edge: which of them face the viewer does not say which
    /// stretches are on the outline.
    MoreThanTwoUses,
    /// One of its faces has no closed form here (a freeform or unbounded face).
    Face {
        /// The face.
        face: FaceId,
        /// Why it has none.
        why: Why,
    },
    /// Where a face's facing changes along the edge is not found in closed form (a NURBS
    /// edge, a torus edge that is neither a meridian nor a parallel).
    Cuts(Undecided),
    /// A point on the edge could not be placed on a face (`contains_uv` could not tell, or
    /// the surface has no normal there).
    Unplaced {
        /// The face.
        face: FaceId,
    },
}

/// The outline of `body` under `view`, in the body's coordinates.
///
/// Emits the phase `"hlr.outline"` and one progress event per edge. Charges one unit of the
/// budget per edge and one per span classified, besides what a NURBS face's recognition
/// charges.
///
/// # Errors
///
/// [`HlrError::Exhausted`] and [`HlrError::Stopped`]; [`HlrError::Surface`] when a surface
/// refuses a query; [`HlrError::Curve`] when a curve's coordinates overflow.
pub fn outline(body: &Brep, view: &View, cx: &mut Cx<'_>) -> Result<Outline, HlrError> {
    let mut budget = cx.budget();
    of_body(body, view, &mut budget, cx.stop, cx.trace)
}

/// Which way a face's normal out of the material lies against its surface's natural one.
#[derive(Clone, Copy)]
enum Outward {
    /// `n` times this sign.
    Sign(f64),
    /// Seen from both sides.
    Sheet,
    /// Between two solids: never seen.
    Inside,
}

/// One face's closed forms.
struct FaceState<'a> {
    face: Face<'a>,
    quadric: Quadric,
    rims: Vec<Rim<'a>>,
    outward: Outward,
    res: Resolution,
}

type Placed<T> = Result<T, OutlineUndecided>;

/// [`outline`] on a budget shared with other bodies.
pub(crate) fn of_body(
    body: &Brep,
    view: &View,
    budget: &mut Budget,
    stop: &Stop,
    trace: &dyn Trace,
) -> Result<Outline, HlrError> {
    trace.event(Event::Phase("hlr.outline"));
    let total = body.edges().len() as u64;
    let mut out = Outline {
        stretches: Vec::new(),
        undecided: Vec::new(),
    };
    let mut cache: BTreeMap<FaceId, Result<FaceState<'_>, Why>> = BTreeMap::new();
    for (done, edge) in (1u64..).zip(body.edges()) {
        stop.check()?;
        budget.charge(1)?;
        let fins: Vec<FaceId> = edge.fins().map(|f| f.face).collect();
        let faces: BTreeSet<FaceId> = fins.iter().copied().collect();
        // A wire has none; a seam (one face on both its sides) is not a change of face.
        let seam = faces.len() == 1 && fins.len() > 1;
        if let (false, EdgeShape::Curve { curve, interval }) =
            (faces.is_empty() || seam, edge.shape())
        {
            let id = edge.id();
            let found = if fins.len() > 2 {
                Err(OutlineUndecided::MoreThanTwoUses)
            } else {
                for &f in &faces {
                    if let Entry::Vacant(slot) = cache.entry(f) {
                        let Some(face) = body.face(f) else { continue };
                        let mut run = Run {
                            face,
                            res: body.resolution(),
                            budget,
                            stop,
                        };
                        let made = match run.quadric()? {
                            Ok(quadric) => Ok(FaceState {
                                face,
                                rims: rims(face, &quadric.tol),
                                outward: outward(face),
                                res: body.resolution(),
                                quadric,
                            }),
                            Err(why) => Err(why),
                        };
                        slot.insert(made);
                    }
                }
                let mut states = Vec::with_capacity(2);
                let mut refused = None;
                for &f in &faces {
                    match cache.get(&f) {
                        Some(Ok(s)) => states.push(s),
                        Some(Err(why)) => {
                            refused = Some(OutlineUndecided::Face { face: f, why: *why });
                        }
                        None => {}
                    }
                }
                match refused {
                    Some(why) => Err(why),
                    None => edge_stretches(id, curve, *interval, &states, view, budget, stop)?,
                }
            };
            match found {
                Ok(ranges) => {
                    for range in ranges {
                        out.stretches.push(Stretch {
                            edge: id,
                            range,
                            ends: [curve.point(range.min()), curve.point(range.max())],
                            whole: range.min() == interval.min() && range.max() == interval.max(),
                        });
                    }
                }
                Err(why) => out.undecided.push(EdgeUndecided { edge: id, why }),
            }
        }
        trace.event(Event::Progress { done, total });
    }
    Ok(out)
}

/// The material side of `face`.
fn outward(face: Face<'_>) -> Outward {
    let solid = |side| face.shell(side).region().is_solid();
    match (solid(FaceSide::Front), solid(FaceSide::Back)) {
        (true, false) => Outward::Sign(-1.0),
        (false, true) => Outward::Sign(1.0),
        (false, false) => Outward::Sheet,
        (true, true) => Outward::Inside,
    }
}

/// The ranges of the edge on the outline.
fn edge_stretches(
    edge: EdgeId,
    curve: &Curve,
    interval: Extent1d,
    states: &[&FaceState<'_>],
    view: &View,
    budget: &mut Budget,
    stop: &Stop,
) -> Result<Placed<Vec<Extent1d>>, HlrError> {
    let (lo, hi) = (interval.min(), interval.max());
    let eps = ROUNDING * lo.abs().max(hi.abs()).max(hi - lo);
    let mut cuts: Vec<f64> = Vec::new();
    for s in states {
        match cuts_of(edge, s, view, budget, stop)? {
            Ok(c) => cuts.extend(c),
            Err(u) => return Ok(Err(OutlineUndecided::Cuts(u))),
        }
    }
    cuts.retain(|&c| lo + eps < c && c < hi - eps);
    cuts.sort_by(f64::total_cmp);
    let mut bounds = vec![lo];
    for c in cuts {
        if bounds.last().is_some_and(|&l| c > l + eps) {
            bounds.push(c);
        }
    }
    if bounds.last().is_some_and(|&l| hi > l + eps) {
        bounds.push(hi);
    } else if let Some(l) = bounds.last_mut() {
        // A last cut within rounding of the end is the end.
        *l = hi;
    }
    let mut ranges: Vec<Extent1d> = Vec::new();
    for (&a, &b) in bounds.iter().zip(bounds.iter().skip(1)) {
        budget.charge(1)?;
        let mid = 0.5 * (a + b);
        let mut facing = Vec::with_capacity(2);
        for s in states {
            match s.faces(curve, mid, view, budget, stop)? {
                Some(f) => facing.push(f),
                None => return Ok(Err(OutlineUndecided::Unplaced { face: s.face.id() })),
            }
        }
        let on = match facing.as_slice() {
            [one] => *one,
            [a, b] => a != b,
            _ => false,
        };
        if !on {
            continue;
        }
        match ranges.last_mut() {
            Some(last) if last.max() == a => {
                *last = Extent1d::new(last.min(), b).ok_or(HlrError::Defect {
                    reason: "an edge's stretch is not ordered and finite",
                })?;
            }
            _ => ranges.push(Extent1d::new(a, b).ok_or(HlrError::Defect {
                reason: "an edge's stretch is not ordered and finite",
            })?),
        }
    }
    Ok(Ok(ranges))
}

/// The parameters on `edge` where `s`'s facing may change.
fn cuts_of(
    edge: EdgeId,
    s: &FaceState<'_>,
    view: &View,
    budget: &mut Budget,
    stop: &Stop,
) -> Result<Result<Vec<f64>, Undecided>, HlrError> {
    // An edge the face uses twice is a seam: no change of face there.
    let Some(rim) = s.rims.iter().find(|r| r.edge == edge) else {
        return Ok(Ok(Vec::new()));
    };
    let id = s.face.id();
    let frames: Vec<Frame> = match &s.quadric.surface {
        scx_surface::Surface::Torus(t) => match contour::edge_planes(rim, t, view) {
            Ok(f) => f,
            Err(u) => return Ok(Err(u)),
        },
        surface => match ruled_or_round(id, surface, view, &s.quadric.fbox)? {
            Some(candidates) => candidates
                .into_iter()
                .filter_map(|c| match c.splitter {
                    Splitter::Plane(f) => Some(f),
                    Splitter::Found(_) => None,
                })
                .collect(),
            // A plane has no silhouette: its facing is constant.
            None => Vec::new(),
        },
    };
    let mut out = Vec::new();
    for frame in &frames {
        let hits = match plane_hits(id, rim, frame, &s.quadric.fbox, s.res, budget, stop)? {
            Ok(h) => h,
            Err(u) => return Ok(Err(u)),
        };
        for h in hits {
            match h {
                CurveSurfaceHit::Point { t, .. } => out.push(t),
                CurveSurfaceHit::Overlap { t } => {
                    out.push(t.min());
                    out.push(t.max());
                }
            }
        }
    }
    Ok(Ok(out))
}

impl FaceState<'_> {
    /// The normal out of the material at the body point `p` of the face, as `n · v`.
    fn lean(
        &self,
        p: Point3,
        v: Vec3,
        budget: &mut Budget,
        stop: &Stop,
    ) -> Result<Option<(f64, bool, UnitVec3)>, HlrError> {
        let id = self.face.id();
        let inv = self
            .quadric
            .surface
            .invert(p, None, Some(&self.quadric.uv), budget, stop)
            .map_err(|e| HlrError::surface(id, e))?;
        let foot = inv.foot().at;
        let on_face = match inv {
            Inversion::Outside(_) => false,
            // Proved inside: a point that is only near the boundary may be on the face on
            // the other side of the edge, whose lean has the other sign.
            Inversion::Inside(_) => matches!(self.face.contains_uv(foot), Some(UvSide::Inside)),
        };
        let Some(n) = analytic_normal(&self.quadric.surface, foot) else {
            return Ok(None);
        };
        let s = n.dot(v);
        Ok(Some((
            match self.outward {
                Outward::Sign(k) => k * s,
                Outward::Sheet => s.abs(),
                Outward::Inside => f64::NEG_INFINITY,
            },
            on_face,
            n,
        )))
    }

    /// Whether the face faces the viewer at the point `t` of `curve`, which lies on it;
    /// `None` where it cannot be placed.
    fn faces(
        &self,
        curve: &Curve,
        t: f64,
        view: &View,
        budget: &mut Budget,
        stop: &Stop,
    ) -> Result<Option<bool>, HlrError> {
        let v = -*view.d();
        let jet = curve.jet::<1>(t, Approach::Above);
        let (p, [along]) = (jet.point, jet.d);
        let Some((s, _, n)) = self.lean(p, v, budget, stop)? else {
            return Ok(None);
        };
        if s == f64::NEG_INFINITY {
            return Ok(Some(false));
        }
        if s.abs() > EDGE_ON {
            return Ok(Some(s > FACING));
        }
        // Edge-on at the edge: a step to either side finds how the face leans a hair
        // inside it; a side off the face does not count, and the side that leans more is
        // the face's.
        let Some(across) = n.cross(along).normalized() else {
            return Ok(Some(false));
        };
        let step = self.quadric.tol.size() * STEP;
        let mut best = s;
        for side in [1.0, -1.0] {
            let q = p + *across * (step * side);
            if let Some((lean, true, _)) = self.lean(q, v, budget, stop)?
                && lean.abs() > best.abs()
            {
                best = lean;
            }
        }
        Ok(Some(best > FACING))
    }
}

/// The exact image of one stretch of the outline.
#[derive(Debug, Clone, PartialEq)]
pub struct StretchImage {
    /// The edge.
    pub edge: EdgeId,
    /// The image: a line, or conic arcs in order along the stretch.
    pub curves: Vec<Curve2>,
}

/// The images of an outline's stretches, and the edges that have none yet.
#[derive(Debug, Clone, PartialEq)]
pub struct Images {
    /// The images, in the outline's order.
    pub images: Vec<StretchImage>,
    /// Edges whose curve has no exact image yet (a NURBS curve, a parabola or hyperbola:
    /// step 5).
    pub pending: Vec<EdgeId>,
}

impl Outline {
    /// The stretches' exact images on the screen of `view`, measured from `origin` (a point
    /// near the body keeps the coordinates small). A stretch seen end on has no image and
    /// is left out.
    ///
    /// # Errors
    ///
    /// [`HlrError::BadPlacement`] when the view cannot be carried into the body's frame.
    pub fn images(&self, body: &Brep, view: &View, origin: Point3) -> Result<Images, HlrError> {
        let lens =
            Lens::new(view, Frame::WORLD, origin).ok_or(HlrError::BadPlacement { body: 0 })?;
        let mut out = Images {
            images: Vec::new(),
            pending: Vec::new(),
        };
        for s in &self.stretches {
            let Some(EdgeShape::Curve { curve, .. }) = body.edge(s.edge).map(scx_brep::Edge::shape)
            else {
                continue;
            };
            match pieces(curve, s.range.min(), s.range.max(), &lens) {
                Ok(ps) => out.images.push(StretchImage {
                    edge: s.edge,
                    curves: ps.into_iter().map(|p| p.image).collect(),
                }),
                Err(NoImage::Point) => {}
                Err(NoImage::Pending) => {
                    if !out.pending.contains(&s.edge) {
                        out.pending.push(s.edge);
                    }
                }
            }
        }
        Ok(out)
    }

    /// The same outline on the body placed by `frame`.
    pub(crate) fn placed(mut self, frame: &Frame) -> Self {
        for s in &mut self.stretches {
            s.ends = s.ends.map(|p| place_point(frame, p));
        }
        self
    }
}

/// `curves` with coincident pieces drawn once (within `tol`, in mm on the screen): lines on
/// one line become the stretches they cover together; any other piece given twice (equal,
/// or a conic run the other way) is kept once. Lines come first, then the rest in order.
///
/// An outline seen square to a face meets several edges on one line or circle: a hole's rim
/// seen edge-on along a plate's edge, a boss's rim on its foot.
#[must_use]
pub fn drop_coincident(curves: Vec<Curve2>, tol: f64) -> Vec<Curve2> {
    let near = |a: Point2, b: Point2| (a - b).length() <= tol;
    let duplicate = |a: &Curve2, b: &Curve2| match (a, b) {
        (
            Curve2::Conic {
                start,
                apex,
                end,
                weight,
            },
            Curve2::Conic {
                start: s2,
                apex: a2,
                end: e2,
                weight: w2,
            },
        ) => {
            near(*apex, *a2)
                && (weight - w2).abs() <= 1e-9
                && ((near(*start, *s2) && near(*end, *e2))
                    || (near(*start, *e2) && near(*end, *s2)))
        }
        _ => false,
    };
    let mut lines: Vec<(Point2, Point2)> = Vec::new();
    let mut others: Vec<Curve2> = Vec::new();
    for c in curves {
        match c {
            Curve2::Line { start, end } => lines.push((start, end)),
            c if others.iter().any(|o| duplicate(o, &c) || *o == c) => {}
            c => others.push(c),
        }
    }
    let mut used = vec![false; lines.len()];
    let mut out = Vec::new();
    for (i, &(a, b)) in lines.iter().enumerate() {
        if used.get(i).copied().unwrap_or(true) {
            continue;
        }
        let Some(u) = (b - a).normalized() else {
            continue;
        };
        let on = |p: Point2| {
            let r = p - a;
            (r.x * u.y - r.y * u.x).abs() <= tol
        };
        let mut spans: Vec<(f64, f64)> = Vec::new();
        for (j, (p, q)) in lines.iter().enumerate().skip(i) {
            if !used.get(j).copied().unwrap_or(true) && on(*p) && on(*q) {
                if let Some(slot) = used.get_mut(j) {
                    *slot = true;
                }
                let (s, t) = ((*p - a).dot(*u), (*q - a).dot(*u));
                spans.push((s.min(t), s.max(t)));
            }
        }
        spans.sort_by(|x, y| x.0.total_cmp(&y.0));
        let mut merged: Vec<(f64, f64)> = Vec::new();
        for (s, t) in spans {
            match merged.last_mut() {
                Some(last) if s <= last.1 + tol => last.1 = last.1.max(t),
                _ => merged.push((s, t)),
            }
        }
        out.extend(merged.into_iter().map(|(s, t)| Curve2::Line {
            start: a + *u * s,
            end: a + *u * t,
        }));
    }
    out.extend(others);
    out
}
