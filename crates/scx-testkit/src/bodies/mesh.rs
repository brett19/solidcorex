// Written for solidcorex. Apache-2.0.
//! The builder every generator writes through: vertices, edges and faces added to a
//! [`Draft`], each use's pcurve made exact by `pcurve_of` and moved into the period window
//! its loop needs, and a collapsed edge's use given the pole's uv segment (63 §6.1, Face 24).

use std::collections::BTreeMap;

use scx_brep::{
    Draft, EdgeGeom, EdgeId, EdgeShape, EdgeUse, FaceId, FaceRecord, LoopRecord, RegionId, ShellId,
    ShellRecord, UseGeom, VertexId, VertexRecord,
};
use scx_curve::{Curve, Curve2, Line, Line2};
use scx_num::extent1d::Extent1d;
use scx_num::fmath;
use scx_num::frame::{Frame, Sense};
use scx_num::vector2d::{Point2, Vec2};
use scx_num::vector3d::{Point3, Vec3};
use scx_surface::{Plane, Surface, pcurve_of};

use super::{GenError, Result, aligned, build_resolution, use_ends};

/// An edge as the builder knows it: its id, and its curve and interval unless collapsed.
#[derive(Debug, Clone)]
pub(crate) struct Ed {
    pub(crate) id: EdgeId,
    pub(crate) curve: Option<(Curve, Extent1d)>,
}

/// One step of a loop.
#[derive(Debug, Clone)]
pub(crate) enum Step {
    /// A use of a curved edge in a sense.
    Use(Ed, Sense),
    /// A use of a collapsed edge at a pole: the uv segment along `u` from the previous use's
    /// end to the next use's start, in the direction of `sign` (at most a period).
    Pole(Ed, f64),
}

/// The pole segment's own parameter interval: a collapsed edge has none (open question in
/// 63 §6.10), so its use runs over `[0, 1]`.
pub(crate) fn unit() -> Result<Extent1d> {
    Extent1d::new(0.0, 1.0).ok_or(GenError::Degenerate("unit interval"))
}

/// A draft being built.
pub(crate) struct Builder {
    pub(crate) d: Draft,
    /// Each planar face's front normal, for radial ordering.
    pub(crate) normals: BTreeMap<FaceId, Vec3>,
}

impl Builder {
    pub(crate) fn new() -> Self {
        Self {
            d: Draft::new(),
            normals: BTreeMap::new(),
        }
    }

    /// The outside's shell and one solid region's: what a one-solid body has.
    pub(crate) fn solid_shells(&mut self) -> Result<(ShellId, ShellId)> {
        let outer = self.d.add_shell(ShellRecord::new(RegionId::OUTSIDE))?;
        let inner = self.solid()?;
        Ok((outer, inner))
    }

    /// A new region, solid or void, and its shell.
    pub(crate) fn region_shell(&mut self, solid: bool) -> Result<ShellId> {
        let r = self.d.add_region(solid)?;
        Ok(self.d.add_shell(ShellRecord::new(r))?)
    }

    fn solid(&mut self) -> Result<ShellId> {
        self.region_shell(true)
    }

    pub(crate) fn vertex(&mut self, p: Point3) -> Result<VertexId> {
        Ok(self.d.add_vertex(VertexRecord::new(p))?)
    }

    pub(crate) fn point(&self, v: VertexId) -> Result<Point3> {
        self.d
            .vertex(v)
            .map(scx_brep::Vertex::point)
            .ok_or(GenError::Degenerate("vertex"))
    }

    /// An edge along `curve` over `[lo, hi]` from `ends[0]` to `ends[1]`.
    pub(crate) fn edge(
        &mut self,
        curve: Curve,
        lo: f64,
        hi: f64,
        ends: [VertexId; 2],
    ) -> Result<Ed> {
        let interval = Extent1d::new(lo, hi).ok_or(GenError::Degenerate("edge interval"))?;
        let shape = EdgeShape::Curve {
            curve: curve.clone(),
            interval,
        };
        let id = self.d.add_edge(EdgeGeom::new(shape, ends))?;
        Ok(Ed {
            id,
            curve: Some((curve, interval)),
        })
    }

    /// The straight edge from `a` to `b`, over `[0, 1]`.
    pub(crate) fn line(&mut self, a: VertexId, b: VertexId) -> Result<Ed> {
        let (p, q) = (self.point(a)?, self.point(b)?);
        let line = Line::new(p, q - p)?;
        self.edge(Curve::Line(line), 0.0, 1.0, [a, b])
    }

    /// An edge collapsed to `v`.
    pub(crate) fn collapsed(&mut self, v: VertexId) -> Result<Ed> {
        let id = self
            .d
            .add_edge(EdgeGeom::new(EdgeShape::Collapsed, [v, v]))?;
        Ok(Ed { id, curve: None })
    }

    /// Adds the face on `surface` with `sides` (front, back) and `loops`, each use's pcurve
    /// in the window that chains it to the use before (the first use's as `pcurve_of`
    /// gives it).
    pub(crate) fn face(
        &mut self,
        surface: Surface,
        sides: [ShellId; 2],
        loops: &[Vec<Step>],
    ) -> Result<FaceId> {
        let records = loops
            .iter()
            .map(|steps| loop_record(&surface, steps))
            .collect::<Result<Vec<_>>>()?;
        Ok(self.d.add_face(FaceRecord::new(surface, sides, records))?)
    }

    /// Adds the planar face bounded by straight edges through `rings` of vertices, the
    /// first ring the outer one; its front is the side the outer ring runs counter-clockwise
    /// about. `edge` gives the edge between two vertices and whether it runs from the first.
    pub(crate) fn polygon<F>(
        &mut self,
        sides: [ShellId; 2],
        rings: &[Vec<VertexId>],
        mut edge: F,
    ) -> Result<FaceId>
    where
        F: FnMut(&mut Self, VertexId, VertexId) -> Result<(Ed, Sense)>,
    {
        let outer = rings.first().ok_or(GenError::Degenerate("polygon"))?;
        let pts = outer
            .iter()
            .map(|&v| self.point(v))
            .collect::<Result<Vec<_>>>()?;
        let normal = newell(&pts);
        let (Some(&p0), Some(&p1)) = (pts.first(), pts.get(1)) else {
            return Err(GenError::Degenerate("polygon"));
        };
        let frame = Frame::new(p0, normal, p1 - p0).ok_or(GenError::Degenerate("plane frame"))?;
        let mut loops = Vec::with_capacity(rings.len());
        for ring in rings {
            let mut steps = Vec::with_capacity(ring.len());
            for (k, &a) in ring.iter().enumerate() {
                let b = *ring
                    .get((k + 1) % ring.len())
                    .ok_or(GenError::Degenerate("ring"))?;
                let (e, sense) = edge(self, a, b)?;
                steps.push(Step::Use(e, sense));
            }
            loops.push(steps);
        }
        let id = self.face(Surface::Plane(Plane::new(frame)), sides, &loops)?;
        self.normals.insert(id, *frame.z());
        Ok(id)
    }

    /// Puts the fins of every edge with three or more in radial order: counter-clockwise
    /// about the edge's direction, by the direction each face leaves the edge into
    /// (`n x t` for its front normal `n` and its use's direction `t`). Planar faces only.
    pub(crate) fn order_fins(&mut self) -> Result<()> {
        let mut work = Vec::new();
        for e in self.d.edges() {
            if e.fins().len() < 3 {
                continue;
            }
            let Some(EdgeShape::Curve {
                curve: Curve::Line(l),
                ..
            }) = e.shape()
            else {
                return Err(GenError::Degenerate("radial edge"));
            };
            work.push((e.id(), l.direction()));
        }
        for (id, dir) in work {
            let d: Vec3 = dir;
            let normals = &self.normals;
            let Some(adj) = self.d.adjacency_mut(id) else {
                continue;
            };
            let mut keyed = Vec::with_capacity(adj.fins.len());
            for &fin in &adj.fins {
                let n = *normals
                    .get(&fin.face)
                    .ok_or(GenError::Degenerate("fin normal"))?;
                keyed.push((radial_angle(d, n, fin.sense)?, fin));
            }
            keyed.sort_by(|a, b| a.0.total_cmp(&b.0));
            adj.fins = keyed.into_iter().map(|(_, f)| f).collect();
        }
        Ok(())
    }
}

/// The angle about the edge direction `d` at which a face of front normal `n` whose use
/// runs in `sense` leaves the edge.
pub(crate) fn radial_angle(d: Vec3, n: Vec3, sense: Sense) -> Result<f64> {
    let t = d * sense.sign();
    let w = n.cross(t);
    let reference =
        Frame::new(Point3::ORIGIN, d, any_normal(d)).ok_or(GenError::Degenerate("edge frame"))?;
    Ok(fmath::atan2(w.dot(*reference.y()), w.dot(*reference.x())))
}

/// A vector not parallel to `d`.
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

/// Newell's normal of a planar polygon: counter-clockwise about it, the length twice the
/// area.
pub(crate) fn newell(pts: &[Point3]) -> Vec3 {
    let mut n = Vec3::ZERO;
    for (k, p) in pts.iter().enumerate() {
        let Some(q) = pts.get((k + 1) % pts.len()) else {
            continue;
        };
        n += Vec3::new(
            (p.y - q.y) * (p.z + q.z),
            (p.z - q.z) * (p.x + q.x),
            (p.x - q.x) * (p.y + q.y),
        );
    }
    n
}

/// The loop record of `steps` on `surface`: each curved use's pcurve from `pcurve_of`,
/// moved by whole periods to start where the use before ends; each pole use the segment
/// along `u` to where the next use starts.
fn loop_record(surface: &Surface, steps: &[Step]) -> Result<LoopRecord> {
    let domain = surface.domain();
    // The pcurves as `pcurve_of` gives them, before any move.
    let raw = steps
        .iter()
        .map(|s| match s {
            Step::Use(e, _) => {
                let (curve, interval) = e.curve.as_ref().ok_or(GenError::Degenerate("use"))?;
                Ok(Some(pcurve_of(
                    curve,
                    surface,
                    *interval,
                    build_resolution(),
                )?))
            }
            Step::Pole(..) => Ok(None),
        })
        .collect::<Result<Vec<_>>>()?;
    let mut placed: Vec<Option<(Curve2, [Point2; 2])>> = vec![None; steps.len()];
    let mut prev_end: Option<Point2> = None;
    let mut uses = Vec::with_capacity(steps.len());
    for (k, step) in steps.iter().enumerate() {
        match step {
            Step::Use(e, sense) => {
                let (curve, interval) = e.curve.as_ref().ok_or(GenError::Degenerate("use"))?;
                let _ = curve;
                let (lo, hi) = (interval.min(), interval.max());
                let mut c2 = raw
                    .get(k)
                    .cloned()
                    .flatten()
                    .ok_or(GenError::Degenerate("pcurve"))?;
                if let Some(at) = prev_end {
                    c2 = aligned(c2, lo, hi, *sense, at, &domain)?;
                }
                let ends = use_ends(&c2, lo, hi, *sense);
                prev_end = Some(ends[1]);
                if let Some(slot) = placed.get_mut(k) {
                    *slot = Some((c2.clone(), ends));
                }
                uses.push(EdgeUse::new(
                    e.id,
                    *sense,
                    UseGeom::Pcurve {
                        curve: c2,
                        interval: *interval,
                    },
                ));
            }
            Step::Pole(e, sign) => {
                let start = prev_end.ok_or(GenError::Degenerate("pole first in loop"))?;
                // Where the next use starts: placed already when it is the loop's first.
                let next = (k + 1) % steps.len();
                let target = match placed.get(next).cloned().flatten() {
                    Some((_, [s, _])) => s,
                    None => match (steps.get(next), raw.get(next).cloned().flatten()) {
                        (Some(Step::Use(n, sense)), Some(c)) => {
                            let (_, iv) = n.curve.as_ref().ok_or(GenError::Degenerate("use"))?;
                            use_ends(&c, iv.min(), iv.max(), *sense)[0]
                        }
                        _ => return Err(GenError::Degenerate("pole not before a use")),
                    },
                };
                let period = domain
                    .u
                    .period()
                    .ok_or(GenError::Degenerate("pole period"))?;
                let du = toward(target.x - start.x, period, *sign);
                let line = Line2::new(start, Vec2::new(du, 0.0))?;
                prev_end = Some(start + Vec2::new(du, 0.0));
                uses.push(EdgeUse::new(
                    e.id,
                    Sense::Forward,
                    UseGeom::Pcurve {
                        curve: Curve2::Line(line),
                        interval: unit()?,
                    },
                ));
            }
        }
    }
    Ok(LoopRecord::new(uses))
}

/// `gap` moved by whole `period`s to the sign of `sign`, in `(0, period]` in size: the
/// length of a pole segment.
fn toward(gap: f64, period: f64, sign: f64) -> f64 {
    let g = gap * sign.signum();
    let mut m = g - period * (g / period).floor();
    // A gap a rounding short of a whole period is a whole period; one a rounding past zero
    // is zero, and a segment of zero is the whole turn.
    if m > period * (1.0 - 1e-12) || m < period * 1e-12 {
        m = period;
    }
    m * sign.signum()
}
