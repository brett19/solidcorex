// Written for solidcorex. Apache-2.0.
//! [`tight_box`]: a certified box of a finished entity, as tight as its trims allow
//! (research 74 §2.3, §22, BW2).
//!
//! The box of a trimmed face is not the box of its surface over the face's uv hull
//! (`Face::bounding_box`, which is certified but loose where a trim cuts across a bulge). It
//! is found per side, as the largest value of one coordinate over the face. That maximum is
//! at the boundary or at a critical point of the coordinate over the face's interior, so the
//! search is two parts, each outward-rounded and so certified:
//!
//! - **The boundary.** Each edge's curve over its interval is searched in one parameter: a
//!   cell is dropped when its enclosure lies at or below the best end so far, or when the
//!   enclosure of the derivative excludes 0 (a monotone cell's largest value is at an end of
//!   the cell, which a neighbour or the interval end holds); a cell whose range is within the
//!   tolerance is taken whole, else halved. Across a knot of a NURBS curve the derivative's
//!   enclosure holds both sides, so a kink is a candidate until it is pinned down.
//! - **The interior.** The face's uv hull is searched the same way with
//!   [`Surface::along`], which also encloses the two partials of the coordinate. A cell with a
//!   partial that excludes 0 has no critical point; a cell [`Face::side_of_cell`] proves
//!   outside the face holds no point of it. What is left is halved on the partial whose
//!   enclosure moves the range most, until its range is within the tolerance. A trim that is
//!   itself the maximum (a cut of a cylinder at `z = 0.3`) leaves no cell at all: the
//!   boundary term is the answer.
//!
//! The tolerance is a thousandth of the body's coincidence zone (and never below the rounding
//! of the entity's coordinates), so the box exceeds the true one by about that and by the
//! entity's accuracy and the zone it is grown by, as `Face::bounding_box` is. It is met with
//! that box, so it is never looser.
//!
//! **Memo and work** (04 row 104; R8). The answer for an edge and for a face is kept in the
//! caller's [`Scratch`] with the work it cost, in cells enclosed. The entry holds the full
//! input the answer was computed from (curve and interval; surface and every use's pcurve,
//! sense and edge), and a hit needs it equal, so a hash collision only replaces an entry. A
//! hit charges the recorded work at once, so a budget stops at the same place warm or cold.
//! Only a finished answer is kept. The body is `&Brep` and nothing is written to it.

use std::collections::BTreeSet;

use scx_brep::op::{Cx, Scratch};
use scx_brep::{Brep, EdgeId, EdgeShape, Entity, Face, UseGeom, UvSide};
use scx_curve::Curve;
use scx_num::budget::{Budget, Stop};
use scx_num::extent1d::Extent1d;
use scx_num::extent2d::Extent2d;
use scx_num::extent3d::Extent3d;
use scx_num::frame::Sense;
use scx_num::hash::mix;
use scx_num::interval::{Ivl, round_down, round_up};
use scx_num::vector3d::{Point3, Vec3};
use scx_nurbs::Approach;
use scx_surface::Surface;

use crate::error::{BodyqDefect, BodyqError, BodyqRefusal};

/// The most halvings of one cell before its range is taken as it is (a certified overshoot of
/// a cell that small, never a silent cut).
const MAX_DEPTH: u32 = 60;
/// The tolerance as a fraction of the coincidence zone.
const TOL_OF_ZONE: f64 = 1e-3;
/// The tolerance's floor as a fraction of the entity's largest coordinate (above the
/// rounding of its enclosures).
const TOL_FLOOR: f64 = 9.1e-13;

/// A certified box of `e`: the hull of its edges' and faces' tight boxes, each as tight as
/// its trims allow, grown by its accuracy and the body's resolution (so it is within
/// `Face::bounding_box` and `Edge::bounding_box` where those exist). A region's and a shell's
/// box is the hull of its faces' (wire edges and lone vertices of a shell included).
///
/// The answers for edges and faces are kept in `cx.scratch`; a hit charges the work its fill
/// recorded to the call's budget (`cx.budget()`), so the call stops at the same place whether
/// warm or cold.
///
/// # Errors
///
/// [`BodyqRefusal::NoEntity`] for an entity the body does not have;
/// [`BodyqError::Exhausted`] or [`BodyqError::Cancelled`]; [`BodyqDefect::NoFaceBox`],
/// [`BodyqDefect::NoUvBox`], [`BodyqDefect::NoEdgeBox`] or [`BodyqDefect::Surface`] where a
/// face or edge of a finished body cannot be enclosed (a bug in the body or in the query);
/// never a default box.
pub fn tight_box(body: &Brep, e: Entity, cx: &mut Cx<'_>) -> Result<Extent3d, BodyqError> {
    let mut budget = cx.budget();
    tight_box_charged(body, e, cx.scratch, &mut budget, cx.stop)
}

/// [`tight_box`] charging a budget the caller keeps, for a caller that boxes many entities
/// under one budget.
///
/// # Errors
///
/// As [`tight_box`].
pub fn tight_box_charged(
    body: &Brep,
    e: Entity,
    scratch: &mut Scratch,
    budget: &mut Budget,
    stop: &Stop,
) -> Result<Extent3d, BodyqError> {
    let mut run = Run {
        body,
        scratch,
        budget,
        stop,
    };
    run.entity(e)
}

/// A certified box of the whole body: every face, wire edge and lone vertex, each as
/// [`tight_box`] gives it. Charged as [`tight_box`] is.
///
/// # Errors
///
/// [`BodyqRefusal::EmptyBody`] for a body with nothing to box, else as [`tight_box`].
pub fn tight_body_box(body: &Brep, cx: &mut Cx<'_>) -> Result<Extent3d, BodyqError> {
    let mut budget = cx.budget();
    let mut run = Run {
        body,
        scratch: cx.scratch,
        budget: &mut budget,
        stop: cx.stop,
    };
    run.whole()
}

type Res<T> = Result<T, BodyqError>;

/// One call's state.
struct Run<'a> {
    body: &'a Brep,
    scratch: &'a mut Scratch,
    budget: &'a mut Budget,
    stop: &'a Stop,
}

/// What an edge's raw box is computed from.
#[derive(Clone, PartialEq)]
enum EdgeIn {
    Curve(Curve, Extent1d),
    Point(Point3),
}

/// An edge's entry: the input, the tolerance it was searched at, the raw box and the work.
struct EdgeEntry {
    input: EdgeIn,
    tol: f64,
    bx: Extent3d,
    work: u64,
}

/// What a face's raw box is computed from: its surface and, per loop, each use's sense,
/// pcurve and edge input.
#[derive(Clone, PartialEq)]
struct FaceIn {
    /// How far the entity's box is grown (its accuracy and the zone): the search reads the
    /// loose box, which carries it.
    margin: f64,
    surface: Surface,
    loops: Vec<Vec<(Sense, UseGeom, EdgeIn)>>,
}

struct FaceEntry {
    input: FaceIn,
    tol: f64,
    bx: Extent3d,
    work: u64,
}

const TAG_EDGE: u64 = 0x7462_6564;
const TAG_FACE: u64 = 0x7462_6661;

impl Run<'_> {
    /// Charges `n` cells.
    fn charge(&mut self, n: u64) -> Res<()> {
        self.stop.check()?;
        self.budget.charge(n)?;
        Ok(())
    }

    fn entity(&mut self, e: Entity) -> Res<Extent3d> {
        let no = BodyqError::Refused(BodyqRefusal::NoEntity { entity: e });
        match e {
            Entity::Vertex(v) => {
                let v = self.body.vertex(v).ok_or(no)?;
                let acc = v.accuracy().map(|r| r.acc.length().mm());
                grown(Extent3d::from_point(v.point()), self.margin(acc))
                    .ok_or(BodyqError::Defect(BodyqDefect::NoNormal { at: e }))
            }
            Entity::Edge(id) => {
                self.body.edge(id).ok_or(no)?;
                self.edge_box(id)
            }
            Entity::Face(id) => {
                let face = self.body.face(id).ok_or(no)?;
                self.face_box(face)
            }
            Entity::Shell(id) => {
                let shell = self.body.shell(id).ok_or(no)?;
                let mut hull: Option<Extent3d> = None;
                let mut seen = BTreeSet::new();
                for (face, _) in shell.faces() {
                    if seen.insert(face.id()) {
                        let b = self.face_box(face)?;
                        hull = Some(hull.map_or(b, |h| h.union(b)));
                    }
                }
                for edge in shell.wires() {
                    let b = self.edge_box(edge.id())?;
                    hull = Some(hull.map_or(b, |h| h.union(b)));
                }
                for v in shell.lone_vertices() {
                    let b = self.entity(Entity::Vertex(v.id()))?;
                    hull = Some(hull.map_or(b, |h| h.union(b)));
                }
                hull.ok_or(BodyqError::Refused(BodyqRefusal::EmptyBody))
            }
            Entity::Region(id) => {
                let region = self.body.region(id).ok_or(no)?;
                let mut hull: Option<Extent3d> = None;
                for shell in region.shells() {
                    match self.entity(Entity::Shell(shell.id())) {
                        Ok(b) => hull = Some(hull.map_or(b, |h| h.union(b))),
                        Err(BodyqError::Refused(BodyqRefusal::EmptyBody)) => {}
                        Err(e) => return Err(e),
                    }
                }
                hull.ok_or(BodyqError::Refused(BodyqRefusal::EmptyBody))
            }
        }
    }

    fn whole(&mut self) -> Res<Extent3d> {
        let mut hull: Option<Extent3d> = None;
        let add = |b: Extent3d, h: &mut Option<Extent3d>| {
            *h = Some(h.map_or(b, |x| x.union(b)));
        };
        for face in self.body.faces() {
            let b = self.face_box(face)?;
            add(b, &mut hull);
        }
        for edge in self.body.edges() {
            if edge.is_wire() {
                let b = self.edge_box(edge.id())?;
                add(b, &mut hull);
            }
        }
        for v in self.body.vertices() {
            if v.edges().next().is_none() {
                let b = self.entity(Entity::Vertex(v.id()))?;
                add(b, &mut hull);
            }
        }
        hull.ok_or(BodyqError::Refused(BodyqRefusal::EmptyBody))
    }

    /// How far a box is grown for an entity of accuracy `acc` (mm), rounded up.
    fn margin(&self, acc: Option<f64>) -> f64 {
        round_up(acc.unwrap_or(0.0) + self.body.resolution().zone().mm())
    }

    /// The tolerance for an entity whose largest coordinate is `scale`.
    fn tol(&self, scale: f64) -> f64 {
        (self.body.resolution().zone().mm() * TOL_OF_ZONE).max(scale * TOL_FLOOR)
    }

    // ----- edges ---------------------------------------------------------------------------

    /// The edge's tight box, grown by its accuracy and the zone.
    fn edge_box(&mut self, id: EdgeId) -> Res<Extent3d> {
        let edge = self
            .body
            .edge(id)
            .ok_or(BodyqError::Refused(BodyqRefusal::NoEntity {
                entity: Entity::Edge(id),
            }))?;
        let acc = edge.accuracy().map(|r| r.acc.length().mm());
        let raw = self.edge_raw(id)?;
        let m = self.margin(acc);
        let out =
            grown(Some(raw), m).ok_or(BodyqError::Defect(BodyqDefect::NoEdgeBox { edge: id }))?;
        // The loose box holds the same set, so the two meet.
        Ok(edge
            .bounding_box()
            .and_then(|b| out.intersect(b))
            .unwrap_or(out))
    }

    /// The edge's input as the memo keeps it, with the tolerance its box is searched at.
    fn edge_input(&self, id: EdgeId) -> Res<(EdgeIn, f64)> {
        let edge = self
            .body
            .edge(id)
            .ok_or(BodyqError::Refused(BodyqRefusal::NoEntity {
                entity: Entity::Edge(id),
            }))?;
        let input = match edge.shape() {
            EdgeShape::Curve { curve, interval } => EdgeIn::Curve(curve.clone(), *interval),
            EdgeShape::Collapsed => EdgeIn::Point(edge.start().point()),
            #[allow(unreachable_patterns)]
            _ => return Err(BodyqError::Defect(BodyqDefect::NoEdgeBox { edge: id })),
        };
        let tol = self.tol(in_reach(&input));
        Ok((input, tol))
    }

    /// The edge's box with no growth, memoized.
    fn edge_raw(&mut self, id: EdgeId) -> Res<Extent3d> {
        let (input, tol) = self.edge_input(id)?;
        let key = edge_key(&input, tol);
        if let Some(e) = self.scratch.get::<EdgeEntry>(key)
            && e.input == input
            && e.tol.to_bits() == tol.to_bits()
        {
            let (bx, work) = (e.bx, e.work);
            self.charge(work)?;
            return Ok(bx);
        }
        let before = self.budget.used();
        let bx = self.edge_search(id, &input, tol)?;
        let work = self.budget.used().saturating_sub(before);
        self.scratch.insert(
            key,
            EdgeEntry {
                input,
                tol,
                bx,
                work,
            },
        );
        Ok(bx)
    }

    fn edge_search(&mut self, id: EdgeId, input: &EdgeIn, tol: f64) -> Res<Extent3d> {
        let no = BodyqError::Defect(BodyqDefect::NoEdgeBox { edge: id });
        match input {
            EdgeIn::Point(p) => Extent3d::from_point(*p).ok_or(no),
            EdgeIn::Curve(curve, iv) => {
                let mut lo = [0.0; 3];
                let mut hi = [0.0; 3];
                for axis in 0..3 {
                    let up = self.curve_max(curve, *iv, axis, 1.0, tol)?.ok_or(no)?;
                    let down = self.curve_max(curve, *iv, axis, -1.0, tol)?.ok_or(no)?;
                    if let (Some(l), Some(h)) = (lo.get_mut(axis), hi.get_mut(axis)) {
                        *l = -down;
                        *h = up;
                    }
                }
                Extent3d::from_bounds(lo[0], lo[1], lo[2], hi[0], hi[1], hi[2]).ok_or(no)
            }
        }
    }

    /// An upper bound of `sign * coordinate(axis)` over `curve` on `iv`, within about `tol`
    /// of the largest; `None` where the curve cannot be enclosed.
    fn curve_max(
        &mut self,
        curve: &Curve,
        iv: Extent1d,
        axis: usize,
        sign: f64,
        tol: f64,
    ) -> Res<Option<f64>> {
        let value = |p: Point3<Ivl>| signed(pick(p.x, p.y, p.z, axis), sign);
        let mut best = f64::NEG_INFINITY;
        for t in [iv.min(), iv.max()] {
            let v = value(curve.point_over(Ivl::from(t)));
            if !v.is_finite() {
                return Ok(None);
            }
            best = best.max(v.hi);
        }
        let mut stack = vec![(iv.min(), iv.max(), 0u32)];
        while let Some((a, b, depth)) = stack.pop() {
            self.charge(1)?;
            let cell = Ivl::new(a, b);
            let r = value(curve.point_over(cell));
            let mid = 0.5 * (a + b);
            let can_halve = depth < MAX_DEPTH && a < mid && mid < b;
            if !r.is_finite() {
                // A wide cell a rational curve cannot enclose (its weights not proved
                // positive): halve it.
                if !can_halve {
                    return Ok(None);
                }
                stack.push((mid, b, depth + 1));
                stack.push((a, mid, depth + 1));
                continue;
            }
            let r = hull_met(curve, a, b, axis, sign, r);
            if r.hi <= best {
                continue;
            }
            let d = curve.jet_over::<Ivl, 1>(cell, Approach::Above).d[0];
            let slope = signed(pick(d.x, d.y, d.z, axis), sign);
            if slope.lo > 0.0 || slope.hi < 0.0 {
                // Monotone: the largest is at an end of the cell, which the interval's ends
                // or the neighbouring cell hold.
                continue;
            }
            // The mean-value form about the middle, met with the natural range: second order
            // in the cell where the slope is small (a constant coordinate of a rational curve
            // has a first-order natural range and a first-order slope enclosure).
            let r = if slope.is_finite() {
                let m = value(curve.point_over(Ivl::from(mid)));
                let half = round_up((b - mid).max(mid - a));
                let mean = m + slope * Ivl::new(-half, half);
                let (lo, hi) = (r.lo.max(mean.lo), r.hi.min(mean.hi));
                if m.is_finite() && lo <= hi {
                    hull_met(curve, a, b, axis, sign, Ivl::new(lo, hi))
                } else {
                    r
                }
            } else {
                r
            };
            if r.hi <= best {
                continue;
            }
            if r.hi - r.lo <= tol || !can_halve {
                best = best.max(r.hi);
                continue;
            }
            stack.push((mid, b, depth + 1));
            stack.push((a, mid, depth + 1));
        }
        Ok(Some(best))
    }

    // ----- faces ---------------------------------------------------------------------------

    /// The face's tight box, grown by its accuracy and the zone and met with its loose box.
    fn face_box(&mut self, face: Face<'_>) -> Res<Extent3d> {
        // The loose box is a bound to meet, where the enclosures give one (a rational patch
        // of many spans may have none); the search does not need it.
        let loose = face.bounding_box();
        let raw = self.face_raw(face)?;
        let acc = face.accuracy().map(|a| a.length().mm());
        let out = grown(Some(raw), self.margin(acc))
            .ok_or_else(|| BodyqError::Defect(BodyqDefect::NoFaceBox { face: face.id() }))?;
        Ok(loose.and_then(|l| out.intersect(l)).unwrap_or(out))
    }

    fn face_input(&self, face: Face<'_>) -> Res<FaceIn> {
        let mut loops = Vec::with_capacity(face.loops().len());
        for lp in face.loops() {
            let mut uses = Vec::with_capacity(lp.len());
            for u in lp.uses() {
                let (e, _) = self.edge_input(u.record().edge)?;
                uses.push((u.sense(), u.geom().clone(), e));
            }
            loops.push(uses);
        }
        Ok(FaceIn {
            margin: self.margin(face.accuracy().map(|a| a.length().mm())),
            surface: face.surface().clone(),
            loops,
        })
    }

    fn face_raw(&mut self, face: Face<'_>) -> Res<Extent3d> {
        let input = self.face_input(face)?;
        let scale = input
            .loops
            .iter()
            .flatten()
            .fold(0.0, |m: f64, (_, _, e)| m.max(in_reach(e)));
        let tol = self.tol(scale);
        let key = face_key(&input, tol);
        if let Some(e) = self.scratch.get::<FaceEntry>(key)
            && e.input == input
            && e.tol.to_bits() == tol.to_bits()
        {
            let (bx, work) = (e.bx, e.work);
            self.charge(work)?;
            return Ok(bx);
        }
        let before = self.budget.used();
        let bx = self.face_search(face, tol, input.margin)?;
        let work = self.budget.used().saturating_sub(before);
        self.scratch.insert(
            key,
            FaceEntry {
                input,
                tol,
                bx,
                work,
            },
        );
        Ok(bx)
    }

    fn face_search(&mut self, face: Face<'_>, tol: f64, margin: f64) -> Res<Extent3d> {
        let fail = BodyqError::Defect(BodyqDefect::NoFaceBox { face: face.id() });
        // The boundary: each edge once, in id order.
        let mut ids = BTreeSet::new();
        for u in face.uses() {
            ids.insert(u.record().edge);
        }
        let mut edges: Option<Extent3d> = None;
        for id in ids {
            let b = self.edge_raw(id)?;
            edges = Some(edges.map_or(b, |h| h.union(b)));
        }
        let edges = edges.ok_or(fail)?;
        let uv = face
            .uv_box()
            .ok_or_else(|| BodyqError::Defect(BodyqDefect::NoUvBox { face: face.id() }))?;
        let mut lo = [0.0; 3];
        let mut hi = [0.0; 3];
        for axis in 0..3 {
            for sign in [1.0, -1.0] {
                let on_edges = if sign > 0.0 {
                    pick(edges.max().x, edges.max().y, edges.max().z, axis)
                } else {
                    -pick(edges.min().x, edges.min().y, edges.min().z, axis)
                };
                // The loose box (the surface over the uv hull, grown by `margin`) bounds the
                // side too: where the edges already reach it, the interior has nothing to add.
                let cap = face.bounding_box().map(|l| {
                    let side = if sign > 0.0 {
                        pick(l.max().x, l.max().y, l.max().z, axis)
                    } else {
                        -pick(l.min().x, l.min().y, l.min().z, axis)
                    };
                    let raw = round_up(side - margin);
                    raw + 4.0 * f64::EPSILON * raw.abs()
                });
                let best = match cap {
                    Some(c) if c <= on_edges + tol => on_edges.max(c),
                    _ => self.interior_max(face, uv, axis, sign, on_edges, tol)?,
                };
                let slot = if sign > 0.0 {
                    hi.get_mut(axis)
                } else {
                    lo.get_mut(axis)
                };
                if let Some(s) = slot {
                    *s = if sign > 0.0 { best } else { -best };
                }
            }
        }
        Extent3d::from_bounds(lo[0], lo[1], lo[2], hi[0], hi[1], hi[2]).ok_or(fail)
    }

    /// An upper bound of `sign * coordinate(axis)` over the face, given `best` as an upper
    /// bound over its boundary: the larger of `best` and the cells of the interior search.
    fn interior_max(
        &mut self,
        face: Face<'_>,
        uv: Extent2d,
        axis: usize,
        sign: f64,
        mut best: f64,
        tol: f64,
    ) -> Res<f64> {
        let surf = |()| BodyqError::Defect(BodyqDefect::Surface { face: face.id() });
        let dir = match axis {
            0 => Vec3::new(sign, 0.0, 0.0),
            1 => Vec3::new(0.0, sign, 0.0),
            _ => Vec3::new(0.0, 0.0, sign),
        };
        let mut stack = vec![(uv, 0u32)];
        while let Some((cell, depth)) = stack.pop() {
            self.charge(1)?;
            let Some(a) = face.surface().along(dir, cell) else {
                // A cell the surface cannot enclose (a rational patch whose weights are
                // not proved positive): halve it.
                let (lo, hi) = (cell.min(), cell.max());
                let (mx, my) = (0.5 * (lo.x + hi.x), 0.5 * (lo.y + hi.y));
                let halves = if hi.x - lo.x >= hi.y - lo.y {
                    [
                        Extent2d::from_bounds(lo.x, lo.y, mx, hi.y),
                        Extent2d::from_bounds(mx, lo.y, hi.x, hi.y),
                    ]
                } else {
                    [
                        Extent2d::from_bounds(lo.x, lo.y, hi.x, my),
                        Extent2d::from_bounds(lo.x, my, hi.x, hi.y),
                    ]
                };
                let (Some(h0), Some(h1)) = (halves[0], halves[1]) else {
                    return Err(surf(()));
                };
                if depth >= MAX_DEPTH || h0 == cell || h1 == cell {
                    return Err(surf(()));
                }
                stack.push((h1, depth + 1));
                stack.push((h0, depth + 1));
                continue;
            };
            if a.range.hi <= best {
                continue;
            }
            if excludes_zero(a.du) || excludes_zero(a.dv) {
                continue;
            }
            if face.side_of_cell(cell) == Some(UvSide::Outside) {
                continue;
            }
            let (lo, hi) = (cell.min(), cell.max());
            let (w, h) = (hi.x - lo.x, hi.y - lo.y);
            let (su, sv) = (a.du.mag() * w, a.dv.mag() * h);
            // Halve along the side whose slope moves the range most (the longer when both
            // are unbounded).
            let along_u = if su.is_finite() && sv.is_finite() {
                su >= sv
            } else {
                w >= h
            };
            let (c0, c1) = if along_u {
                let m = 0.5 * (lo.x + hi.x);
                (
                    (m > lo.x && m < hi.x)
                        .then(|| Extent2d::from_bounds(lo.x, lo.y, m, hi.y))
                        .flatten(),
                    Extent2d::from_bounds(m, lo.y, hi.x, hi.y),
                )
            } else {
                let m = 0.5 * (lo.y + hi.y);
                (
                    (m > lo.y && m < hi.y)
                        .then(|| Extent2d::from_bounds(lo.x, lo.y, hi.x, m))
                        .flatten(),
                    Extent2d::from_bounds(lo.x, m, hi.x, hi.y),
                )
            };
            let (Some(c0), Some(c1)) = (c0, c1) else {
                best = best.max(a.range.hi);
                continue;
            };
            if a.range.hi - a.range.lo <= tol || depth >= MAX_DEPTH {
                best = best.max(a.range.hi);
                continue;
            }
            stack.push((c1, depth + 1));
            stack.push((c0, depth + 1));
        }
        Ok(best)
    }
}

/// `r` met with the range of the coordinate over the control points a NURBS curve's cell
/// `[a, b]` is a convex combination of (the weights are positive, so a point of each span
/// lies in the hull of its `degree + 1` control points): exact for a coordinate that is
/// constant over the cell's spans, where the interval forms are only first order. Both hold
/// the same values, so they meet; were rounding to part them, `r` stands.
fn hull_met(curve: &Curve, a: f64, b: f64, axis: usize, sign: f64, r: Ivl) -> Ivl {
    let Curve::Nurbs(n) = curve else {
        return r;
    };
    let kv = n.knot_vector();
    let (Ok(sa), Ok(sb)) = (kv.span(a, Approach::Above), kv.span(b, Approach::Below)) else {
        return r;
    };
    let p = n.degree();
    let first = sa.saturating_sub(p);
    let Some(pts) = n.points().get(first..=sb.max(sa)) else {
        return r;
    };
    let (mut lo, mut hi) = (f64::INFINITY, f64::NEG_INFINITY);
    for q in pts {
        let v = sign * q.get(axis).copied().unwrap_or(f64::NAN);
        lo = lo.min(v);
        hi = hi.max(v);
    }
    // The control values are rounded; one ulp each way holds the combination's rounding.
    let (lo, hi) = (round_down(lo), round_up(hi));
    let (m_lo, m_hi) = (r.lo.max(lo), r.hi.min(hi));
    if lo.is_finite() && hi.is_finite() && m_lo <= m_hi {
        Ivl::new(m_lo, m_hi)
    } else {
        r
    }
}

/// `x` of an enclosure triple by axis.
fn pick<T>(x: T, y: T, z: T, axis: usize) -> T {
    match axis {
        0 => x,
        1 => y,
        _ => z,
    }
}

/// `v` or `-v` as `sign` says (exact).
fn signed(v: Ivl, sign: f64) -> Ivl {
    if sign < 0.0 {
        Ivl::new(-v.hi, -v.lo)
    } else {
        v
    }
}

/// Whether the enclosure holds no 0.
fn excludes_zero(v: Ivl) -> bool {
    v.lo > 0.0 || v.hi < 0.0
}

/// The largest coordinate magnitude among an edge's ends and middle: the scale its tolerance
/// floor is taken from (a floor, so a rough value is enough).
fn in_reach(e: &EdgeIn) -> f64 {
    let big = |p: Point3| p.x.abs().max(p.y.abs()).max(p.z.abs());
    match e {
        EdgeIn::Point(p) => big(*p),
        EdgeIn::Curve(c, iv) => [iv.min(), 0.5 * (iv.min() + iv.max()), iv.max()]
            .into_iter()
            .map(|t| big(c.point(t)))
            .filter(|x| x.is_finite())
            .fold(0.0, f64::max),
    }
}

/// `bx` with every side moved out by `d`, rounded outward; `None` for no box.
fn grown(bx: Option<Extent3d>, d: f64) -> Option<Extent3d> {
    let bx = bx?;
    let (lo, hi) = (bx.min(), bx.max());
    let down = |x: f64| round_down(x - d);
    let up = |x: f64| round_up(x + d);
    Extent3d::from_bounds(
        down(lo.x),
        down(lo.y),
        down(lo.z),
        up(hi.x),
        up(hi.y),
        up(hi.z),
    )
}

fn edge_key(input: &EdgeIn, tol: f64) -> u64 {
    let mut w = vec![TAG_EDGE, tol.to_bits()];
    edge_words(input, &mut w);
    mix(&w)
}

fn edge_words(input: &EdgeIn, w: &mut Vec<u64>) {
    match input {
        EdgeIn::Point(p) => w.extend([1, p.x.to_bits(), p.y.to_bits(), p.z.to_bits()]),
        EdgeIn::Curve(c, iv) => {
            w.extend([2, iv.min().to_bits(), iv.max().to_bits()]);
            for t in [iv.min(), iv.max()] {
                let p = c.point(t);
                w.extend([p.x.to_bits(), p.y.to_bits(), p.z.to_bits()]);
            }
        }
    }
}

/// A face's scratch key: its tolerance and each use's edge ends. The entry holds the full
/// input, so this need only spread faces.
fn face_key(input: &FaceIn, tol: f64) -> u64 {
    let mut w = vec![TAG_FACE, tol.to_bits()];
    for lp in &input.loops {
        w.push(lp.len() as u64);
        for (_, _, e) in lp {
            edge_words(e, &mut w);
        }
    }
    mix(&w)
}
