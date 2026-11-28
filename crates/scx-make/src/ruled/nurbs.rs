// Written for solidcorex. Apache-2.0.
//! The general case: a NURBS surface of degree 1 in `t` (module docs of [`super`]).

use std::sync::Arc;

use scx_brep::{
    Document, Draft, EdgeGeom, EdgeId, EdgeShape, EdgeUse, FaceId, FaceRecord, LoopRecord,
    RegionId, ShellRecord, UseGeom, VertexRecord,
};
use scx_curve::{Curve, Curve2, Line2};
use scx_num::extent1d::Extent1d;
use scx_num::frame::Sense;
use scx_num::vector2d::{Point2, Vec2};
use scx_num::vector3d::{Point3, Vec3};
use scx_nurbs::{Approach, CompatibleCurves, KnotVector, NurbsCurve};
use scx_nurbs_surface::{Net, NurbsSurface, ParamAxis};
use scx_surface::Surface;

use super::{Join, RuledError, RuledRefusal, RuledSide, RuledUnsupported, Which};
use crate::Made;
use crate::error::{Defect, MakeError};

fn make(e: impl Into<MakeError>) -> RuledError {
    RuledError::Make(e.into())
}

const fn defect(why: &'static str) -> RuledError {
    RuledError::Defect { why }
}

/// The exact NURBS form of `side`, on the curve's own parameter at the ends: a line's degree-1
/// form, a circle's rational quadratic arcs (at most a quarter turn each), a NURBS curve
/// restricted. `None` for the other conics.
fn form(side: &RuledSide, which: Which) -> Result<NurbsCurve<3>, RuledError> {
    let (curve, interval) = (&side.curve, side.interval);
    let (t0, t1) = (interval.min(), interval.max());
    let unsupported = || RuledError::Unsupported(RuledUnsupported::Form(which));
    match curve {
        Curve::Line(_) => {
            let (p, q) = (curve.point(t0), curve.point(t1));
            let k = KnotVector::new(1, vec![t0, t0, t1, t1]).map_err(RuledError::Compatible)?;
            NurbsCurve::new(k, vec![p.to_array(), q.to_array()], None)
                .map_err(RuledError::Compatible)
        }
        Curve::Circle(_) => {
            let quarter = core::f64::consts::FRAC_PI_2;
            let span = t1 - t0;
            let pieces = ((span / quarter).ceil() as usize).max(1);
            let at = |k: usize| t0 + span * (k as f64 / pieces as f64);
            let w = scx_num::fmath::cos(0.5 * span / pieces as f64);
            let mut knots = vec![t0, t0, t0];
            let mut points = vec![curve.point(t0).to_array()];
            let mut weights = vec![1.0];
            for k in 0..pieces {
                let (a, b) = (at(k), at(k + 1));
                let (pa, pb) = (curve.point(a), curve.point(b));
                let ta = curve
                    .jet::<1>(a, Approach::Above)
                    .d
                    .first()
                    .and_then(|d| Vec3::from(d.to_array()).normalized())
                    .ok_or(defect("an arc's tangent"))?;
                let m = pa + Vec3::from(ta) * (pa.distance(pb) / (2.0 * w));
                points.push(m.to_array());
                weights.push(w);
                points.push(pb.to_array());
                weights.push(1.0);
                if k + 1 < pieces {
                    knots.extend([b, b]);
                }
            }
            knots.extend([t1, t1, t1]);
            let k = KnotVector::new(2, knots).map_err(RuledError::Compatible)?;
            NurbsCurve::new(k, points, Some(weights)).map_err(RuledError::Compatible)
        }
        Curve::Nurbs(_) => match curve.restrict(interval) {
            Ok(Curve::Nurbs(n)) => Ok(Arc::unwrap_or_clone(n)),
            _ => Err(RuledError::Refused(RuledRefusal::Interval(which))),
        },
        _ => Err(unsupported()),
    }
}

pub(super) fn ruled(
    a: &RuledSide,
    b: &RuledSide,
    join: Join,
    doc: Document,
) -> Result<Made<FaceId>, RuledError> {
    let fa = form(a, Which::First)?;
    let mut fb = form(b, Which::Second)?;
    if join == Join::Reversed {
        fb = fb.reversed();
    }
    let both = CompatibleCurves::from_curves(&[fa, fb]).map_err(RuledError::Compatible)?;
    let (Some(ca), Some(cb)) = (both.curve(0), both.curve(1)) else {
        return Err(defect("two compatible sections"));
    };
    let mut points = Vec::with_capacity(2 * ca.points().len());
    for (p, q) in ca.points().iter().zip(cb.points()) {
        points.push(*p);
        points.push(*q);
    }
    let weights = match (ca.weights(), cb.weights()) {
        (Some(wa), Some(wb)) => {
            let mut w = Vec::with_capacity(2 * wa.len());
            for (p, q) in wa.iter().zip(wb) {
                w.push(*p);
                w.push(*q);
            }
            Some(w)
        }
        _ => None,
    };
    let rows = ca.points().len();
    let net_err = RuledError::Surface;
    let net = Net::new(rows, 2, points).map_err(net_err)?;
    let wnet = match weights {
        Some(w) => Some(Net::new(rows, 2, w).map_err(RuledError::Surface)?),
        None => None,
    };
    let kv = KnotVector::new(1, vec![0.0, 0.0, 1.0, 1.0]).map_err(RuledError::Compatible)?;
    let surface =
        NurbsSurface::new(ca.knot_vector().clone(), kv, net, wnet).map_err(RuledError::Surface)?;
    square_sheet(&surface, doc)
}

/// The sheet of the whole of `surface`: its four boundary curves as edges (the surface's own
/// iso curves, exact) with the lines of the parameter square's sides as pcurves.
fn square_sheet(surface: &NurbsSurface<3>, doc: Document) -> Result<Made<FaceId>, RuledError> {
    let d = surface.domain();
    let (u0, u1) = (d.x_interval().min(), d.x_interval().max());
    let (v0, v1) = (d.y_interval().min(), d.y_interval().max());
    let corner = |u, v| {
        surface
            .point([u, v], [Approach::Above; 2])
            .map(Point3::from_array)
            .map_err(|_| defect("a corner of the surface"))
    };
    let (c00, c10, c11, c01) = (
        corner(u0, v0)?,
        corner(u1, v0)?,
        corner(u1, v1)?,
        corner(u0, v1)?,
    );
    let zone = doc.resolution().zone().mm();
    if c00.distance(c01) <= zone || c10.distance(c11) <= zone {
        return Err(RuledError::Unsupported(RuledUnsupported::MeetingSides));
    }
    let mut draft = Draft::new();
    let shell = draft
        .add_shell(ShellRecord::new(RegionId::OUTSIDE))
        .map_err(make)?;
    let mut vertex = |p| draft.add_vertex(VertexRecord::new(p)).map_err(make);
    let ids = [vertex(c00)?, vertex(c10)?, vertex(c11)?, vertex(c01)?];
    let [i00, i10, i11, i01] = ids;
    let sides = [
        // (axis held, its value, ends, the pcurve's origin and direction, the use's sense)
        (
            ParamAxis::V,
            v0,
            [i00, i10],
            Point2::new(0.0, v0),
            Vec2::new(1.0, 0.0),
            Sense::Forward,
        ),
        (
            ParamAxis::U,
            u1,
            [i10, i11],
            Point2::new(u1, 0.0),
            Vec2::new(0.0, 1.0),
            Sense::Forward,
        ),
        (
            ParamAxis::V,
            v1,
            [i01, i11],
            Point2::new(0.0, v1),
            Vec2::new(1.0, 0.0),
            Sense::Reversed,
        ),
        (
            ParamAxis::U,
            u0,
            [i00, i01],
            Point2::new(u0, 0.0),
            Vec2::new(0.0, 1.0),
            Sense::Reversed,
        ),
    ];
    let mut uses: Vec<EdgeUse> = Vec::with_capacity(4);
    let mut edges: Vec<EdgeId> = Vec::with_capacity(4);
    for (axis, t, ends, origin, dir, sense) in sides {
        let iso = surface
            .iso(axis, t)
            .map_err(|_| defect("a boundary iso curve"))?;
        let interval: Extent1d = iso.domain();
        let e = draft
            .add_edge(EdgeGeom::new(
                EdgeShape::Curve {
                    curve: Curve::Nurbs(Arc::new(iso)),
                    interval,
                },
                ends,
            ))
            .map_err(make)?;
        edges.push(e);
        let line =
            Line2::new(origin, dir).map_err(|e| make(MakeError::Defect(Defect::Curve(e))))?;
        uses.push(EdgeUse::new(
            e,
            sense,
            UseGeom::Pcurve {
                curve: Curve2::Line(line),
                interval,
            },
        ));
    }
    let face = draft
        .add_face(FaceRecord::new(
            Surface::Nurbs(Arc::new(surface.clone())),
            [shell, shell],
            vec![LoopRecord::new(uses)],
        ))
        .map_err(make)?;
    let body = match draft.finish(&doc) {
        Ok((body, _)) => body,
        Err(un) => return Err(make(MakeError::Defect(Defect::Finish(un.cause)))),
    };
    Ok(Made { body, faces: face })
}
