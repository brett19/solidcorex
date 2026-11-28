// Written for solidcorex. Apache-2.0.
//! Curved faces (research 72 TS3): a cylinder, cone, sphere or torus face triangulated in
//! its own `uv`, every triangle then checked against its proved chord bound.
//!
//! **Boundary.** Each use's points come from the wireframe's polyline of its edge: the mesh
//! indices are the polyline's, and the `uv` is the use's pcurve at the edge's own sample
//! parameters, never a projection (a seam's two uses are its two pcurves, a period apart).
//! A ring position, not a mesh index, is a triangulation point, so a seam vertex is two
//! points. A pole's (or an apex's) collapsed edge is its pcurve, the segment along `u` at the
//! pole's `v`, cut into steps of the face's pole step, every point the pole's vertex; a
//! triangle with two corners on one pole is the pole itself and is dropped, which leaves the
//! fan of triangles round the pole closed.
//!
//! **Interior points.** A cylinder needs none for the chord: a triangle's sag is bounded by
//! its angle span alone, `2 rho sin²(span/4)`, so the edge samples hold it already. A cone
//! face whose rims' radii differ by more than [`RATIO`] gets rows at radii in even ratio
//! between them, each spaced at the step of the next wider row; a cone to an apex is the
//! fan of its rulings and gets none. Either gets rows for an edge-length bound. A sphere or torus
//! face gets rows of points in latitude (or tube angle), each row's points evenly spaced in
//! longitude at its own circumference, about equilateral in space at a side of `0.8 sqrt(3)`
//! times the enclosing radius the chord allows, and clear of the boundary by `0.4` of a
//! step. A row's step is also at most [`WIDE`] row spacings in the scaled `uv` the face is
//! triangulated in, so near a pole (or a torus's inner equator), where the circumference is
//! small against the scale's, the Delaunay triangles still join adjacent rows rather than
//! spanning several and failing.
//!
//! **The check.** Every triangle is tested on its corners: its normal within 90° of the
//! surface normal at its centroid, its sides within the edge-length bound, and its sag
//! within the chord: for a cylinder or cone the largest `2 rho_i sin²((phi_i - mid)/2)` over
//! its corners, `mid` the middle of its angle span (the radial deficit `sum w_i rho_i -
//! |sum w_i b_i|` of a point at weights `w` is at most `max_i (rho_i - d . b_i)` for any unit
//! `d`, here the one at `mid`), for a sphere or torus `rho - sqrt(rho² - R²)` with `R` its
//! smallest enclosing circle and `rho` the least radius of curvature (exact for a sphere: the
//! triangle lies in the convex hull of the cap the circle cuts). A triangle that fails gets
//! its centroid as a new point and the face is triangulated again, at most [`ROUNDS`] times.

use core::f64::consts::{FRAC_PI_2, FRAC_PI_4, PI, TAU};

use scx_brep::{EdgeShape, Face, FaceId, FaceSide, UseGeom};
use scx_curve::arcs::Arcs;
use scx_num::budget::{Budget, Stop};
use scx_num::fmath::{acos, cos, ln, powf, sin, tan};
use scx_num::frame::Sense;
use scx_num::units::Length;
use scx_num::vector2d::Point2;
use scx_num::vector3d::{Point3, Vec3, sqrt};
use scx_nurbs::Approach;
use scx_surface::Surface;

use crate::cdt::{Fault, triangulate};
use crate::error::{TessDefect, TessError, TessRefusal};
use crate::mesh::Mesh;
use crate::spec::TessSpec;

/// How many times a face is triangulated again with the centroids of its failing triangles
/// before the failure is a defect.
const ROUNDS: usize = 32;

/// The relative room a triangle's bound has over the chord: the rounding of the bound's own
/// arithmetic, so a triangle on an edge sampled exactly at the chord passes.
const SLACK: f64 = 1e-12;

/// A lattice side as a share of the largest the chord allows for an equilateral triangle.
pub(crate) const SIDE: f64 = 0.8;

/// A lattice point's least distance from the boundary, as a share of its row's step.
const CLEAR: f64 = 0.4;

/// The largest ratio of radii between a cone face's rows (or its rims) with none between.
const RATIO: f64 = 1.25;

/// The widest a row's step may be in the triangulation's scaled `uv`, per row spacing:
/// under 2, the Delaunay triangles of staggered rows join adjacent rows.
const WIDE: f64 = 1.5;

/// `sqrt(3) / 2`: an equilateral lattice's row spacing per side.
const ROW: f64 = 0.866_025_403_784_438_6;

/// How a face's sag is bounded and its rows laid.
#[derive(Clone, Copy, Debug)]
enum Kind {
    /// A cylinder (`slope` 0) or cone: radius `r0 + slope v`, `slant` the length per unit
    /// of `v`.
    Ruled { r0: f64, slope: f64, slant: f64 },
    /// A sphere (`centre` 0) or torus: circumference radius `centre + tube cos(psi)` at the
    /// `v` angle `psi`, `rho` the least radius of curvature, `v` the arcs of `v`. A horn
    /// torus (`pinch`) has no least radius: its parallels' radius of curvature, `(centre +
    /// tube cos psi) / -cos psi` inside the tube, falls to 0 at the pole, so `rho` is the
    /// tube's and a triangle's own bound reads its corners' `psi` (see `Rules::bound`).
    Round {
        rho: f64,
        centre: f64,
        tube: f64,
        v: Arcs,
        pinch: bool,
    },
}

/// One curved face's rules: its kind, `u`'s arcs, and the spec's bounds.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Rules {
    kind: Kind,
    u: Arcs,
    chord: f64,
    angle: Option<f64>,
    edge: Option<f64>,
}

impl Rules {
    /// The rules of `surface` under `spec`, or `None` for a kind that is not curved here.
    pub(crate) fn of(surface: &Surface, spec: &TessSpec) -> Option<Self> {
        let (kind, u) = match surface {
            Surface::Cylinder(c) => {
                let r = c.radius().mm();
                (
                    Kind::Ruled {
                        r0: r,
                        slope: 0.0,
                        slant: 1.0,
                    },
                    Arcs::turn(2.0 * PI * r),
                )
            }
            Surface::Cone(c) => {
                let slope = tan(c.half_angle());
                (
                    Kind::Ruled {
                        r0: c.radius().mm(),
                        slope,
                        slant: sqrt(1.0 + slope * slope),
                    },
                    Arcs::turn(1.0),
                )
            }
            Surface::Sphere(s) => {
                let r = s.radius().mm();
                (
                    Kind::Round {
                        rho: r,
                        centre: 0.0,
                        tube: r,
                        v: Arcs::half(),
                        pinch: false,
                    },
                    Arcs::turn(1.0),
                )
            }
            Surface::Torus(t) => {
                let (big, r) = (t.major().mm(), t.minor().mm());
                (
                    Kind::Round {
                        rho: if t.pole().is_some() {
                            r
                        } else {
                            r.min(big - r)
                        },
                        centre: big,
                        tube: r,
                        v: Arcs::turn(1.0),
                        pinch: t.pole().is_some(),
                    },
                    Arcs::turn(1.0),
                )
            }
            _ => return None,
        };
        Some(Self {
            kind,
            u,
            chord: spec.chord().mm(),
            angle: spec.angle(),
            edge: spec.edge_length().map(Length::mm),
        })
    }

    /// The largest angle step a ruled surface's sag allows at radius `rho`: a quarter turn
    /// at most, as an edge's.
    fn ruled_step(&self, rho: f64) -> f64 {
        let c = self.chord / rho;
        let step = if c < 1.0 {
            2.0 * acos(1.0 - c)
        } else {
            FRAC_PI_2
        };
        let step = step.min(FRAC_PI_2);
        self.angle.map_or(step, |a| step.min(a))
    }

    /// A round surface's lattice side: [`SIDE`] of the equilateral side whose enclosing
    /// radius the chord allows, within the angle and edge-length bounds.
    fn round_side(&self, rho: f64) -> f64 {
        let c = self.chord.min(rho);
        let enclosing = sqrt(c * (2.0 * rho - c));
        let mut s = SIDE * 3f64.sqrt() * enclosing;
        s = s.min(rho * FRAC_PI_4);
        if let Some(a) = self.angle {
            s = s.min(SIDE * a * rho);
        }
        if let Some(e) = self.edge {
            s = s.min(SIDE * e);
        }
        s
    }

    /// The angle step along a pole's line: a row's step at the face's widest circumference.
    fn pole_step(&self, rho_max: f64) -> f64 {
        match self.kind {
            Kind::Ruled { .. } => self.ruled_step(rho_max),
            Kind::Round { rho, .. } => (self.round_side(rho) / rho_max).min(FRAC_PI_4),
        }
    }

    /// The bound on triangle `t`'s distance from the surface, or `None` when it fails a
    /// test and needs a point inside.
    fn bound(&self, surface: &Surface, t: &Corners) -> Option<f64> {
        let [a, b, c] = t.at;
        let n = (b - a).cross(c - a);
        let g = middle(t.uv, t.pole);
        let ns = surface.normal(g, [Approach::Above; 2])?.as_vec();
        if n.dot(ns) <= 0.0 {
            return None;
        }
        let sides = [
            (b - a).length_squared(),
            (c - b).length_squared(),
            (a - c).length_squared(),
        ];
        let longest = sides.iter().copied().fold(0.0, f64::max);
        if let Some(e) = self.edge
            && longest > e * e * (1.0 + SLACK)
        {
            return None;
        }
        let sag = match self.kind {
            Kind::Ruled { r0, slope, .. } => {
                // Each corner off the apex: its angle and radius. An apex has no angle,
                // and its radius is 0, so it adds nothing to the bound below.
                let mut at = [(0.0, 0.0); 3];
                let mut lo = f64::INFINITY;
                let mut hi = f64::NEG_INFINITY;
                for ((uv, pole), slot) in t.uv.iter().zip(t.pole).zip(at.iter_mut()) {
                    if pole {
                        continue;
                    }
                    let phi = self.u.angle(uv.x);
                    lo = lo.min(phi);
                    hi = hi.max(phi);
                    *slot = (phi, (r0 + slope * uv.y).abs());
                }
                let span = hi - lo;
                if span.is_nan() || span >= PI {
                    return None;
                }
                if let Some(a) = self.angle
                    && span > a * (1.0 + SLACK)
                {
                    return None;
                }
                // A point of the triangle at weights `w` lies `sum w_i rho_i - |sum w_i b_i|`
                // inside the surface along its radius (`b_i` a corner's offset from the
                // axis): concave in `w`, and at most `max_i (rho_i - d . b_i)` for every unit
                // `d`. With `d` at the middle of the span that is the largest
                // `2 rho_i sin²((phi_i - mid) / 2)`.
                let mid = 0.5 * (lo + hi);
                at.iter()
                    .map(|&(phi, rho)| {
                        let s = sin(0.5 * (phi - mid));
                        2.0 * rho * s * s
                    })
                    .fold(0.0, f64::max)
            }
            Kind::Round {
                rho,
                centre,
                tube,
                v,
                pinch,
            } => {
                if let Some(a) = self.angle
                    && longest > (a * rho) * (a * rho) * (1.0 + SLACK)
                {
                    return None;
                }
                if pinch {
                    // A horn torus: no one radius of curvature bounds it.
                    let r2 = enclosing_squared(sides, n.length_squared());
                    let sag = pinched_sag(self, t, [centre, tube, rho], v, r2)?;
                    return (sag <= self.chord * (1.0 + SLACK)).then_some(sag);
                }
                let r2 = enclosing_squared(sides, n.length_squared());
                if r2.is_nan() || r2 >= rho * rho {
                    return None;
                }
                r2 / (rho + sqrt(rho * rho - r2))
            }
        };
        (sag <= self.chord * (1.0 + SLACK)).then_some(sag)
    }
}

/// A horn torus triangle's bound on its distance from the surface, or `None` when it has none.
///
/// Off the pole, the parallels' radius of curvature at tube angle `psi` inside the tube is
/// `(centre + tube cos psi) / -cos psi`, least at the corner nearest the pole (it grows with
/// the distance from it), so the sphere bound's `rho` is the least of the tube's and each
/// corner's. A triangle with a corner on the pole has the surface's cusp under it, where that
/// radius is 0, so it is bounded in its two directions apart: the sphere bound at the tube's
/// own radius for the meridian (the triangle's side from the pole is a chord of the meridian),
/// and, for the parallels, what a chord across the angle its other corners span lies inside a
/// circle of their largest circumference radius `d`: `2 d sin²(span / 4)`.
fn pinched_sag(
    rules: &Rules,
    t: &Corners,
    [centre, tube, rho]: [f64; 3],
    v: Arcs,
    r2: f64,
) -> Option<f64> {
    let mut least = rho;
    let mut ring = 0.0_f64;
    let mut pole = false;
    let (mut lo, mut hi) = (f64::INFINITY, f64::NEG_INFINITY);
    for (uv, on) in t.uv.iter().zip(t.pole) {
        if on {
            pole = true;
            continue;
        }
        let c = cos(v.angle(uv.y));
        let circ = centre + tube * c;
        ring = ring.max(circ.abs());
        if c < 0.0 {
            least = least.min(circ / -c);
        }
        let phi = rules.u.angle(uv.x);
        lo = lo.min(phi);
        hi = hi.max(phi);
    }
    if !pole {
        return (r2 < least * least).then(|| r2 / (least + sqrt(least * least - r2)));
    }
    let span = hi - lo;
    if span.is_nan() || span >= PI || r2 >= rho * rho {
        return None;
    }
    let s = sin(0.25 * span);
    Some(r2 / (rho + sqrt(rho * rho - r2)) + 2.0 * ring * s * s)
}

/// The squared radius of the smallest circle enclosing a triangle of squared sides `sides`
/// and squared doubled area `area2`: half the longest side for a right or obtuse triangle,
/// the circumradius `abc / (2 |n|)` otherwise.
fn enclosing_squared(sides: [f64; 3], area2: f64) -> f64 {
    let [a, b, c] = sides;
    let longest = a.max(b).max(c);
    if 2.0 * longest >= a + b + c || area2 <= 0.0 {
        return longest / 4.0;
    }
    a * b * c / (4.0 * area2)
}

/// Where a triangle's surface normal is read: its centroid, but in `u` the mean of its
/// corners off a pole, since a pole corner's `u` says nothing of where it is.
fn middle(uv: [Point2; 3], pole: [bool; 3]) -> Point2 {
    let (mut u, mut n) = (0.0, 0.0);
    for (p, on) in uv.iter().zip(pole) {
        if !on {
            u += p.x;
            n += 1.0;
        }
    }
    let g = centroid(uv);
    if n > 0.0 { Point2::new(u / n, g.y) } else { g }
}

fn centroid(uv: [Point2; 3]) -> Point2 {
    let [a, b, c] = uv;
    Point2::new((a.x + b.x + c.x) / 3.0, (a.y + b.y + c.y) / 3.0)
}

/// One triangle's corners.
struct Corners {
    uv: [Point2; 3],
    at: [Point3; 3],
    pole: [bool; 3],
}

/// One face's points, kept from face to face.
#[derive(Default)]
pub(crate) struct Points {
    /// Each point's `uv`.
    uv: Vec<Point2>,
    /// Its place in space: the mesh's own for a boundary point.
    at: Vec<Point3>,
    /// Its mesh index: a boundary point's from the start, an inner one's once a triangle
    /// uses it.
    mesh: Vec<Option<u32>>,
    /// On a pole's line.
    pole: Vec<bool>,
    segments: Vec<[u32; 2]>,
    /// The points in the scaled `uv` the triangulation runs in.
    xy: Vec<Point2>,
    /// Ring boundaries in `uv`, `[from, to)` per loop.
    rings: Vec<(usize, usize)>,
    /// One row's crossings.
    cross: [Vec<f64>; 3],
}

impl Points {
    fn clear(&mut self) {
        self.uv.clear();
        self.at.clear();
        self.mesh.clear();
        self.pole.clear();
        self.segments.clear();
        self.rings.clear();
    }

    fn push(&mut self, uv: Point2, at: Point3, mesh: Option<u32>, pole: bool) {
        self.uv.push(uv);
        self.at.push(at);
        self.mesh.push(mesh);
        self.pole.push(pole);
    }

    fn pop(&mut self) {
        self.uv.pop();
        self.at.pop();
        self.mesh.pop();
        self.pole.pop();
    }

    /// Whether point `i` repeats point `j`: the same mesh index at the same `uv`.
    fn same(&self, i: usize, j: usize) -> bool {
        let m = (self.mesh.get(i), self.mesh.get(j));
        let p = (self.uv.get(i), self.uv.get(j));
        matches!((m, p), ((Some(a), Some(b)), (Some(p), Some(q))) if a == b && p == q)
    }
}

/// The edge polylines and their parameters, as the wireframe made them.
pub(crate) struct Lines<'m> {
    /// `line_of[edge index]` is the edge's place in `mesh.edges`.
    pub(crate) line_of: &'m [usize],
    pub(crate) params: &'m [Vec<f64>],
}

/// Triangulates the curved `face` onto `mesh` under `rules`.
#[allow(clippy::too_many_lines)]
pub(crate) fn curved_face(
    mesh: &mut Mesh,
    lines: &Lines<'_>,
    face: Face<'_>,
    rules: &Rules,
    s: &mut Points,
    budget: &mut Budget,
    stop: &Stop,
) -> Result<(), TessError> {
    let id = face.id();
    let defect = |d: TessDefect| TessError::Defect(d);
    let lost = || defect(TessDefect::Triangulation { face: id });
    let surface = face.surface();
    s.clear();
    let mut chord = Length::ZERO;

    // The face's widest circumference radius, from its pcurves' `v` range, for the pole
    // step: read from the loops' sample points before the poles are cut.
    let mut v_range = (f64::INFINITY, f64::NEG_INFINITY);
    for u in face.uses() {
        if let UseGeom::Pcurve { curve, interval } = u.geom() {
            for t in [interval.min(), interval.max()] {
                let y = curve.point(t).y;
                v_range = (v_range.0.min(y), v_range.1.max(y));
            }
        }
    }
    let rho_max = widest(rules, v_range);
    let pole_step = rules.pole_step(rho_max);

    for lp in face.loops() {
        let from = s.uv.len();
        for u in lp.uses() {
            let edge = u.edge();
            let eid = edge.id();
            let li = lines
                .line_of
                .get(eid.index())
                .copied()
                .ok_or_else(|| defect(TessDefect::Unindexed { edge: eid }))?;
            let (line, inner) = mesh
                .edges
                .get(li)
                .zip(lines.params.get(li))
                .ok_or_else(|| defect(TessDefect::Unindexed { edge: eid }))?;
            chord = chord.max(line.chord);
            let first = *line.points.first().ok_or_else(lost)?;
            let at_of = |i: u32| mesh.positions.get(i as usize).copied();
            match u.geom() {
                UseGeom::Point(p) => {
                    s.push(*p, at_of(first).ok_or_else(lost)?, Some(first), false);
                }
                UseGeom::Pcurve { curve, interval } => {
                    let (lo, hi) = (interval.min(), interval.max());
                    if matches!(edge.shape(), EdgeShape::Collapsed) {
                        // A pole's line: every point the pole's vertex.
                        let (a, b) = (curve.point(lo), curve.point(hi));
                        let span = (rules.u.angle(b.x) - rules.u.angle(a.x)).abs();
                        let n = steps(span / pole_step, id)?;
                        let at = at_of(first).ok_or_else(lost)?;
                        for k in 0..n {
                            let f = f64::from(k) / f64::from(n);
                            let t = match u.sense() {
                                Sense::Forward => lo + (hi - lo) * f,
                                Sense::Reversed => hi - (hi - lo) * f,
                            };
                            s.push(curve.point(t), at, Some(first), true);
                        }
                    } else {
                        if line.points.len() != inner.len() + 2 {
                            return Err(lost());
                        }
                        let ts = core::iter::once(lo)
                            .chain(inner.iter().copied())
                            .chain(core::iter::once(hi));
                        let pts = ts.zip(line.points.iter().copied());
                        // Each use's points but its last, which is the next use's first.
                        let n = line.points.len() - 1;
                        let mut push = |(t, i): (f64, u32)| -> Result<(), TessError> {
                            s.push(curve.point(t), at_of(i).ok_or_else(lost)?, Some(i), false);
                            Ok(())
                        };
                        match u.sense() {
                            Sense::Forward => pts.take(n).try_for_each(&mut push)?,
                            Sense::Reversed => {
                                let all: Vec<(f64, u32)> = pts.collect();
                                all.into_iter().rev().take(n).try_for_each(&mut push)?;
                            }
                        }
                    }
                }
            }
        }
        // A point repeated in place (a collapsed use beside its neighbour) is one point.
        let mut k = from + 1;
        while k < s.uv.len() {
            if s.same(k, k - 1) {
                s.uv.remove(k);
                s.at.remove(k);
                s.mesh.remove(k);
                s.pole.remove(k);
            } else {
                k += 1;
            }
        }
        if s.uv.len() > from + 1 && s.same(s.uv.len() - 1, from) {
            s.pop();
        }
        let to = s.uv.len();
        match to - from {
            0 | 1 => {}
            2 => return Err(lost()),
            _ => {
                for k in from..to {
                    let next = if k + 1 == to { from } else { k + 1 };
                    s.segments.push([k as u32, next as u32]);
                }
                s.rings.push((from, to));
            }
        }
    }

    // A pole's vertex is a pole wherever it is: the end of a seam at it too.
    let poles: Vec<u32> = s
        .mesh
        .iter()
        .zip(&s.pole)
        .filter_map(|(m, &p)| m.filter(|_| p))
        .collect();
    for (m, p) in s.mesh.iter().zip(s.pole.iter_mut()) {
        if m.is_some_and(|m| poles.contains(&m)) {
            *p = true;
        }
    }

    // The box, and the scale that makes `uv` about isometric at its centre.
    let mut lo = Point2::new(f64::INFINITY, f64::INFINITY);
    let mut hi = Point2::new(f64::NEG_INFINITY, f64::NEG_INFINITY);
    for p in &s.uv {
        lo = Point2::new(lo.x.min(p.x), lo.y.min(p.y));
        hi = Point2::new(hi.x.max(p.x), hi.y.max(p.y));
    }
    let scale = scale(rules, lo, hi).ok_or_else(lost)?;

    lattice(rules, surface, lo, hi, scale, s);
    let mut tris;
    let mut worst: f64;
    let mut round = 0;
    loop {
        stop.check()?;
        s.xy.clear();
        s.xy.extend(
            s.uv.iter()
                .map(|p| Point2::new(p.x * scale.0, p.y * scale.1)),
        );
        if u32::try_from(s.xy.len()).is_err() {
            return Err(TessError::Refused(TessRefusal::FaceTooFine {
                face: id,
                needed: s.xy.len() as f64,
            }));
        }
        tris = triangulate(&s.xy, &s.segments, budget).map_err(|f| match f {
            Fault::Duplicate => defect(TessDefect::Duplicate { face: id }),
            Fault::Crossing => defect(TessDefect::Crossing { face: id }),
            Fault::Lost | Fault::Parity => lost(),
            Fault::Exhausted(e) => e.into(),
        })?;
        let mut added = Vec::new();
        worst = 0.0;
        tris.retain(|t| !pole_pair(s, *t));
        for t in &tris {
            let c = corners(s, *t).ok_or_else(lost)?;
            match rules.bound(surface, &c) {
                Some(b) => worst = worst.max(b),
                None => added.push(centroid(c.uv)),
            }
        }
        if added.is_empty() {
            break;
        }
        round += 1;
        if round > ROUNDS {
            return Err(defect(TessDefect::Refinement { face: id }));
        }
        for g in added {
            s.push(g, surface.point(g), None, false);
        }
    }

    let solid = |side| face.shell(side).region().is_solid();
    let flip = solid(FaceSide::Front) && !solid(FaceSide::Back);
    mesh.triangles.reserve(tris.len());
    for t in tris {
        let mut ids = [0u32; 3];
        let mut normals = [Vec3::ZERO; 3];
        let g = corners(s, t)
            .map(|c| middle(c.uv, c.pole))
            .ok_or_else(lost)?;
        for ((m, n), l) in ids.iter_mut().zip(normals.iter_mut()).zip(t) {
            let l = l as usize;
            let slot = s.mesh.get_mut(l).ok_or_else(lost)?;
            *m = if let Some(i) = *slot {
                i
            } else {
                let i = u32::try_from(mesh.positions.len()).map_err(|_| {
                    TessError::Refused(TessRefusal::FaceTooFine {
                        face: id,
                        needed: mesh.positions.len() as f64,
                    })
                })?;
                mesh.positions.push(*s.at.get(l).ok_or_else(lost)?);
                *slot = Some(i);
                i
            };
            let uv = *s.uv.get(l).ok_or_else(lost)?;
            // A pole's normal depends on the way it is approached (an apex has none, and a
            // `v` a rounding past it is the other nappe's): there it is the surface's at the
            // triangle's middle.
            let on_pole = s.pole.get(l).copied().unwrap_or(false);
            let normal = if on_pole {
                None
            } else {
                surface.normal(uv, [Approach::Above; 2])
            }
            .or_else(|| surface.normal(g, [Approach::Above; 2]))
            .ok_or_else(lost)?
            .as_vec();
            *n = if flip { -normal } else { normal };
        }
        let [a, b, c] = ids;
        let [na, nb, nc] = normals;
        if flip {
            mesh.triangles.push([a, c, b]);
            mesh.normals.push([na, nc, nb]);
        } else {
            mesh.triangles.push([a, b, c]);
            mesh.normals.push([na, nb, nc]);
        }
        mesh.face_of.push(id);
    }
    mesh.chord_bound
        .push((id, chord.max(Length::from_mm(worst))));
    Ok(())
}

/// Whether two of `t`'s corners are on one pole's line: the pole itself, no triangle.
fn pole_pair(s: &Points, t: [u32; 3]) -> bool {
    let at = |k: u32| {
        let k = k as usize;
        match (s.pole.get(k), s.mesh.get(k)) {
            (Some(true), Some(Some(m))) => Some(*m),
            _ => None,
        }
    };
    let [a, b, c] = t.map(at);
    let same = |x: Option<u32>, y: Option<u32>| x.is_some() && x == y;
    same(a, b) || same(b, c) || same(c, a)
}

fn corners(s: &Points, t: [u32; 3]) -> Option<Corners> {
    let [a, b, c] = t.map(|k| k as usize);
    Some(Corners {
        uv: [*s.uv.get(a)?, *s.uv.get(b)?, *s.uv.get(c)?],
        at: [*s.at.get(a)?, *s.at.get(b)?, *s.at.get(c)?],
        pole: [*s.pole.get(a)?, *s.pole.get(b)?, *s.pole.get(c)?],
    })
}

/// `x` steps rounded up, at least one, refused past what a mesh indexes.
fn steps(x: f64, face: FaceId) -> Result<u32, TessError> {
    if x.is_finite() && x < f64::from(u32::MAX / 4) {
        Ok(x.ceil().max(1.0) as u32)
    } else {
        Err(TessError::Refused(TessRefusal::FaceTooFine {
            face,
            needed: x,
        }))
    }
}

/// The largest circumference radius over the `v` range.
fn widest(rules: &Rules, (v0, v1): (f64, f64)) -> f64 {
    match rules.kind {
        Kind::Ruled { r0, slope, .. } => (r0 + slope * v0).abs().max((r0 + slope * v1).abs()),
        Kind::Round {
            centre, tube, v, ..
        } => {
            let (a, b) = (v.angle(v0), v.angle(v1));
            // `cos` is largest at the angle nearest a whole turn.
            let near = (a / TAU).ceil() * TAU;
            let c = if near <= b { 1.0 } else { cos(a).max(cos(b)) };
            centre + tube * c
        }
    }
}

/// The per-axis scale of `uv` that makes it about isometric at the box's centre.
fn scale(rules: &Rules, lo: Point2, hi: Point2) -> Option<(f64, f64)> {
    let du = hi.x - lo.x;
    let dv = hi.y - lo.y;
    let dphi = rules.u.angle(hi.x) - rules.u.angle(lo.x);
    let mid = 0.5 * (lo.y + hi.y);
    let (rho, sv) = match rules.kind {
        Kind::Ruled { r0, slope, slant } => {
            let r = (r0 + slope * mid)
                .abs()
                .max((r0 + slope * lo.y).abs())
                .max((r0 + slope * hi.y).abs());
            (r, slant)
        }
        Kind::Round {
            centre, tube, v, ..
        } => {
            let dpsi = v.angle(hi.y) - v.angle(lo.y);
            (centre + tube * cos(v.angle(mid)), tube * dpsi / dv)
        }
    };
    let su = rho * dphi / du;
    (su.is_finite() && sv.is_finite() && su > 0.0 && sv > 0.0).then_some((su, sv))
}

/// The face's inner points: rows over the box, each kept clear of the boundary.
fn lattice(
    rules: &Rules,
    surface: &Surface,
    lo: Point2,
    hi: Point2,
    scale: (f64, f64),
    s: &mut Points,
) {
    let (phi0, phi1) = (rules.u.angle(lo.x), rules.u.angle(hi.x));
    let span_u = phi1 - phi0;
    if span_u.is_nan() || span_u <= 0.0 {
        return;
    }
    let du_per_rad = (hi.x - lo.x) / span_u;
    // A step in `u` at most `WIDE` row spacings, both in the triangulation's `uv`.
    let wide = |dphi: f64, dy: f64| dphi.min(WIDE * dy.abs() * scale.1 / (du_per_rad * scale.0));
    // Each row's `v`, its parameter step to the next, and its angle step in `u`.
    let mut rows: Vec<(f64, f64, f64)> = Vec::new();
    match rules.kind {
        Kind::Ruled { r0, slope, slant } => {
            let Some(e) = rules.edge else {
                cone_rows(rules, r0, slope, lo.y, hi.y, &mut rows);
                for row in &mut rows {
                    row.2 = wide(row.2, row.1);
                }
                return place(rules, surface, &rows, phi0, phi1, du_per_rad, s);
            };
            let dv = SIDE * ROW * e / slant;
            let n = ((hi.y - lo.y) / dv).ceil();
            if !(n.is_finite() && n >= 2.0) {
                return;
            }
            let step = (hi.y - lo.y) / n;
            let rho_max = (r0 + slope * lo.y).abs().max((r0 + slope * hi.y).abs());
            let mut k = 1.0;
            while k < n {
                let v = lo.y + k * step;
                let rho = (r0 + slope * v).abs();
                let dphi = rules
                    .ruled_step(rho_max)
                    .min(SIDE * e / rho.max(f64::MIN_POSITIVE));
                rows.push((v, step, wide(dphi, step)));
                k += 1.0;
            }
        }
        Kind::Round {
            rho,
            centre,
            tube,
            v,
            ..
        } => {
            let side = rules.round_side(rho);
            let (psi0, psi1) = (v.angle(lo.y), v.angle(hi.y));
            let n = ((psi1 - psi0) * tube / (ROW * side)).ceil();
            if !(n.is_finite() && n >= 2.0) {
                return;
            }
            let step = (psi1 - psi0) / n;
            let mut k = 1.0;
            while k < n {
                let psi = psi0 + k * step;
                let y = v.parameter(psi);
                let dy = v.parameter(psi + step) - y;
                let circ = centre + tube * cos(psi);
                let dphi = if circ > 0.0 {
                    (side / circ).min(FRAC_PI_4)
                } else {
                    FRAC_PI_4
                };
                rows.push((y, dy, wide(dphi, dy)));
                k += 1.0;
            }
        }
    }
    place(rules, surface, &rows, phi0, phi1, du_per_rad, s);
}

/// A cone face's rows when no edge length is asked: between rims whose radii differ by more
/// than [`RATIO`], rows at radii in even ratio between them, each spaced at the step its
/// next wider row allows, so a triangle between two rows passes at its larger radius. A
/// cylinder, or a cone to an apex (whose fan of rulings is within the chord already), has
/// none.
fn cone_rows(
    rules: &Rules,
    r0: f64,
    slope: f64,
    y0: f64,
    y1: f64,
    rows: &mut Vec<(f64, f64, f64)>,
) {
    let (a, b) = ((r0 + slope * y0).abs(), (r0 + slope * y1).abs());
    let (small, wide) = (a.min(b), a.max(b));
    if !(small > rules.chord && wide / small > RATIO) {
        return;
    }
    let n = (ln(wide / small) / ln(RATIO)).ceil();
    if !(n.is_finite() && n >= 2.0) {
        return;
    }
    let q = powf(wide / small, 1.0 / n);
    let mut k = 1.0;
    let mut rho = small;
    while k < n {
        rho *= q;
        let next = rho * q;
        // `v` where the radius is `rho`, linear between the rims.
        let at = |r: f64| y0 + (r - a) / (b - a) * (y1 - y0);
        let y = at(rho);
        rows.push((y, (at(next) - y).abs(), SIDE * rules.ruled_step(next)));
        k += 1.0;
    }
}

/// The rows' points, each row's spaced evenly over the face's angle span (every other row
/// half a step on), kept where they are clear of the boundary.
fn place(
    rules: &Rules,
    surface: &Surface,
    rows: &[(f64, f64, f64)],
    phi0: f64,
    phi1: f64,
    du_per_rad: f64,
    s: &mut Points,
) {
    let span_u = phi1 - phi0;
    for (r, &(y, dy, dphi)) in rows.iter().enumerate() {
        let n = (span_u / dphi).ceil();
        if !n.is_finite() || n < 1.0 {
            continue;
        }
        let dphi = span_u / n;
        let cu = CLEAR * dphi * du_per_rad;
        let cv = CLEAR * dy.abs();
        for (c, yy) in [y - cv, y, y + cv].into_iter().enumerate() {
            if let Some(buf) = s.cross.get_mut(c) {
                buf.clear();
            }
            crossings(s, yy, c);
        }
        let shift = if r % 2 == 0 { 0.5 } else { 0.0 };
        let mut j = 0.0;
        while j <= n {
            let phi = phi0 + (j + shift) * dphi;
            j += 1.0;
            if phi >= phi1 {
                break;
            }
            let x = rules.u.parameter(phi);
            if s.cross.iter().all(|xs| inside(xs, x, cu)) {
                let uv = Point2::new(x, y);
                s.push(uv, surface.point(uv), None, false);
            }
        }
    }
}

/// The `x` where the rings cross the line at `y`, ascending, into `s.cross[c]`.
fn crossings(s: &mut Points, y: f64, c: usize) {
    let Some(buf) = s.cross.get_mut(c) else {
        return;
    };
    for &[a, b] in &s.segments {
        let (Some(p), Some(q)) = (s.uv.get(a as usize), s.uv.get(b as usize)) else {
            continue;
        };
        if (p.y > y) != (q.y > y) {
            buf.push(p.x + (y - p.y) * (q.x - p.x) / (q.y - p.y));
        }
    }
    buf.sort_unstable_by(f64::total_cmp);
}

/// Whether `x` lies inside the crossings' intervals by at least `clear`.
fn inside(xs: &[f64], x: f64, clear: f64) -> bool {
    xs.as_chunks::<2>()
        .0
        .iter()
        .any(|&[a, b]| a + clear <= x && x <= b - clear)
}
