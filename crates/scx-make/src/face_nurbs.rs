// Written for solidcorex. Apache-2.0.
//! A face on a NURBS surface, bounded by a uv box or by a region of iso-lines (research 75
//! `MK15b`).
//!
//! Every edge is an iso-curve of the surface, taken exactly by
//! [`NurbsSurface::iso`] (knot insertion to full multiplicity, no parameter moved), over the
//! piece of it the boundary segment spans; every pcurve is the segment itself, a line in uv
//! whose parameter is the iso-curve's own. So the edge and its pcurve agree by construction
//! and the exact pcurve writer, which has no closed form on a NURBS surface, is not asked.
//!
//! What is not made stays typed [`Unsupported`]: a segment that is not along `u` or `v`
//! waits on a curve-on-surface kind (step 5), and a boundary that meets itself (a seam of a
//! closed surface, a side collapsed to a point) is [`Unsupported::NurbsFace`], since its
//! edges would need merging or collapsing.

use std::sync::Arc;

use scx_brep::{
    Document, Draft, EdgeGeom, EdgeShape, EdgeUse, FaceId, FaceRecord, LoopRecord, RegionId,
    ShellRecord, UseGeom, VertexRecord,
};
use scx_curve::{Curve, Curve2, Line2};
use scx_num::extent1d::Extent1d;
use scx_num::extent2d::Extent2d;
use scx_num::frame::Sense;
use scx_num::vector2d::{Point2, Vec2};
use scx_num::vector3d::Point3;
use scx_nurbs_surface::{NurbsSurface, ParamAxis};
use scx_planar::Segment;
use scx_surface::Surface;

use crate::Made;
use crate::error::{Defect, MakeError, Refusal, Unsupported};
use crate::extrude::{given, loops_of};
use crate::face::Bound;
use crate::prism::check;

/// The loops of a boundary: each a ring of segments in uv, start and end.
type Loops = Vec<Vec<[Point2; 2]>>;

const fn meets_itself() -> MakeError {
    MakeError::Unsupported(Unsupported::NurbsFace)
}

/// The face of the NURBS `surface` inside `bound`, as a sheet.
pub(crate) fn nurbs_face(
    surface: &Surface,
    nurbs: &NurbsSurface<3>,
    bound: &Bound,
    doc: Document,
) -> Result<Made<FaceId>, MakeError> {
    let loops = match bound {
        Bound::Box(b) => box_loops(b)?,
        Bound::Region(r) => {
            check(core::slice::from_ref(r), doc.resolution())?;
            let mut loops: Loops = Vec::new();
            for (l, turned, in_loop) in loops_of(r) {
                let n = l.segments().len();
                let mut segs = Vec::with_capacity(n);
                for (i, s) in l.segments().iter().enumerate() {
                    let at = given(in_loop, turned, n, i);
                    let Segment::Line { start, end } = *s else {
                        return Err(MakeError::Unsupported(Unsupported::Segment(at)));
                    };
                    if start.x != end.x && start.y != end.y {
                        return Err(MakeError::Unsupported(Unsupported::Segment(at)));
                    }
                    segs.push([start, end]);
                }
                loops.push(segs);
            }
            loops
        }
    };
    let loops = within_domain(nurbs, loops)?;
    build(surface, &loops, doc)
}

fn box_loops(b: &Extent2d) -> Result<Loops, MakeError> {
    let (ui, vi) = (b.x_interval(), b.y_interval());
    let (du, dv) = (ui.length(), vi.length());
    if !(du > 0.0 && dv > 0.0 && du.is_finite() && dv.is_finite()) {
        return Err(MakeError::Refused(Refusal::UvBox {
            u: [ui.min(), ui.max()],
            v: [vi.min(), vi.max()],
        }));
    }
    let c = [
        Point2::new(ui.min(), vi.min()),
        Point2::new(ui.max(), vi.min()),
        Point2::new(ui.max(), vi.max()),
        Point2::new(ui.min(), vi.max()),
    ];
    Ok(vec![vec![
        [c[0], c[1]],
        [c[1], c[2]],
        [c[2], c[3]],
        [c[3], c[0]],
    ]])
}

/// Relative slack for a parameter at an end of the domain.
const SNAP: f64 = 1e-12;

/// The loops with every coordinate inside the surface's domain: one within rounding of an
/// end is moved onto it, past that is [`Refusal::UvBox`].
fn within_domain(n: &NurbsSurface<3>, loops: Loops) -> Result<Loops, MakeError> {
    let d = n.domain();
    let (ui, vi) = (d.x_interval(), d.y_interval());
    let all = loops.iter().flatten().flatten().copied();
    let hull = Extent2d::hull(all).ok_or(MakeError::Defect(Defect::Interval))?;
    let (hu, hv) = (hull.x_interval(), hull.y_interval());
    let slack = |t: f64| SNAP * (1.0 + t.abs());
    let inside =
        |lo: f64, hi: f64, dlo: f64, dhi: f64| lo >= dlo - slack(dlo) && hi <= dhi + slack(dhi);
    if !(inside(hu.min(), hu.max(), ui.min(), ui.max())
        && inside(hv.min(), hv.max(), vi.min(), vi.max()))
    {
        return Err(MakeError::Refused(Refusal::UvBox {
            u: [hu.min(), hu.max()],
            v: [hv.min(), hv.max()],
        }));
    }
    let snap =
        |p: Point2| Point2::new(p.x.clamp(ui.min(), ui.max()), p.y.clamp(vi.min(), vi.max()));
    Ok(loops
        .into_iter()
        .map(|l| l.into_iter().map(|[a, b]| [snap(a), snap(b)]).collect())
        .collect())
}

fn build(surface: &Surface, loops: &Loops, doc: Document) -> Result<Made<FaceId>, MakeError> {
    let Surface::Nurbs(n) = surface else {
        return Err(meets_itself());
    };
    let zone = doc.resolution().zone().mm();
    let mut draft = Draft::new();
    let shell = draft.add_shell(ShellRecord::new(RegionId::OUTSIDE))?;
    // The vertices, one per segment start; a boundary that touches itself is refused.
    let mut points: Vec<Point3> = Vec::new();
    let mut records = Vec::with_capacity(loops.len());
    for segs in loops {
        let mut ids = Vec::with_capacity(segs.len());
        for [start, _] in segs {
            let p = surface.point(*start);
            if points.iter().any(|q| q.distance(p) <= zone) {
                return Err(meets_itself());
            }
            points.push(p);
            ids.push(draft.add_vertex(VertexRecord::new(p))?);
        }
        let mut uses = Vec::with_capacity(segs.len());
        for (i, ([start, end], &from)) in segs.iter().zip(&ids).enumerate() {
            let to = ids
                .get((i + 1) % segs.len())
                .copied()
                .ok_or(MakeError::Defect(Defect::Interval))?;
            // The edge runs up the parameter; the use runs the segment's way.
            let (sense, lo, hi, ends) = if start.x < end.x || start.y < end.y {
                (Sense::Forward, *start, *end, [from, to])
            } else {
                (Sense::Reversed, *end, *start, [to, from])
            };
            let (axis, t, range, origin, dir) = if start.y == end.y {
                (
                    ParamAxis::V,
                    start.y,
                    [lo.x, hi.x],
                    Point2::new(0.0, start.y),
                    Vec2::new(1.0, 0.0),
                )
            } else {
                (
                    ParamAxis::U,
                    start.x,
                    [lo.y, hi.y],
                    Point2::new(start.x, 0.0),
                    Vec2::new(0.0, 1.0),
                )
            };
            let iso = n.iso(axis, t).map_err(|_| meets_itself())?;
            let interval =
                Extent1d::new(range[0], range[1]).ok_or(MakeError::Defect(Defect::Interval))?;
            let edge = draft.add_edge(EdgeGeom::new(
                EdgeShape::Curve {
                    curve: Curve::Nurbs(Arc::new(iso)),
                    interval,
                },
                ends,
            ))?;
            let line = Line2::new(origin, dir).map_err(|e| MakeError::Defect(Defect::Curve(e)))?;
            uses.push(EdgeUse::new(
                edge,
                sense,
                UseGeom::Pcurve {
                    curve: Curve2::Line(line),
                    interval,
                },
            ));
        }
        records.push(LoopRecord::new(uses));
    }
    let face = draft.add_face(FaceRecord::new(surface.clone(), [shell, shell], records))?;
    let body = match draft.finish(&doc) {
        Ok((body, _)) => body,
        Err(un) => return Err(MakeError::Defect(Defect::Finish(un.cause))),
    };
    Ok(Made { body, faces: face })
}
