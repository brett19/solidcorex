// Written for solidcorex. Apache-2.0.
//! A one-face sheet on a whole NURBS surface: its four boundary curves as edges (the
//! surface's own iso curves, exact) and their pcurves as the lines of the parameter square's
//! sides (exact), written by hand because `Draft::exact_loops` has no pcurve on a NURBS
//! surface.

use std::sync::Arc;

use scx_brep::{
    Brep, Document, Draft, EdgeGeom, EdgeId, EdgeShape, EdgeUse, FaceId, FaceRecord, LoopRecord,
    RegionId, ShellRecord, UseGeom, VertexRecord,
};
use scx_curve::{Curve, Curve2, Line2};
use scx_num::frame::Sense;
use scx_num::vector2d::{Point2, Vec2};
use scx_num::vector3d::Point3;
use scx_nurbs::Approach;
use scx_nurbs_surface::{NurbsSurface, ParamAxis};
use scx_surface::Surface;

use super::error::SheetBlendError;
use crate::error::{Defect, MakeError};

/// The sheet's face and its boundary edges by side of the parameter square.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct Square {
    pub(super) face: FaceId,
    /// `v = v0`, along `u`.
    pub(super) v0: EdgeId,
    /// `u = u1`, along `v`.
    pub(super) u1: EdgeId,
    /// `v = v1`, along `u`.
    pub(super) v1: EdgeId,
    /// `u = u0`, along `v`.
    pub(super) u0: EdgeId,
}

fn make(e: impl Into<MakeError>) -> SheetBlendError {
    SheetBlendError::Make(e.into())
}

/// The sheet of the whole of `surface`.
pub(super) fn square_sheet(
    surface: &NurbsSurface<3>,
    doc: Document,
) -> Result<(Brep, Square), SheetBlendError> {
    let d = surface.domain();
    let (u0, u1) = (d.x_interval().min(), d.x_interval().max());
    let (v0, v1) = (d.y_interval().min(), d.y_interval().max());
    let iso = |axis, t| {
        surface
            .iso(axis, t)
            .map(|c| Curve::Nurbs(Arc::new(c)))
            .map_err(|_| SheetBlendError::Defect {
                why: "a boundary iso curve of the blend",
            })
    };
    let mut draft = Draft::new();
    let shell = draft
        .add_shell(ShellRecord::new(RegionId::OUTSIDE))
        .map_err(make)?;
    let corner = |u, v| {
        surface
            .point([u, v], [Approach::Above; 2])
            .map(Point3::from_array)
            .map_err(|_| SheetBlendError::Defect {
                why: "a corner of the blend",
            })
    };
    let mut vertex = |p| draft.add_vertex(VertexRecord::new(p)).map_err(make);
    let c00 = vertex(corner(u0, v0)?)?;
    let c10 = vertex(corner(u1, v0)?)?;
    let c11 = vertex(corner(u1, v1)?)?;
    let c01 = vertex(corner(u0, v1)?)?;
    let sides = [
        // (axis held, its value, ends, the pcurve's origin and direction)
        (
            ParamAxis::V,
            v0,
            [c00, c10],
            Point2::new(0.0, v0),
            Vec2::new(1.0, 0.0),
        ),
        (
            ParamAxis::U,
            u1,
            [c10, c11],
            Point2::new(u1, 0.0),
            Vec2::new(0.0, 1.0),
        ),
        (
            ParamAxis::V,
            v1,
            [c01, c11],
            Point2::new(0.0, v1),
            Vec2::new(1.0, 0.0),
        ),
        (
            ParamAxis::U,
            u0,
            [c00, c01],
            Point2::new(u0, 0.0),
            Vec2::new(0.0, 1.0),
        ),
    ];
    let senses = [
        Sense::Forward,
        Sense::Forward,
        Sense::Reversed,
        Sense::Reversed,
    ];
    let mut edges: Vec<EdgeId> = Vec::with_capacity(4);
    let mut uses = Vec::with_capacity(4);
    for ((axis, t, ends, origin, dir), sense) in sides.into_iter().zip(senses) {
        let curve = iso(axis, t)?;
        let interval = match &curve {
            Curve::Nurbs(c) => c.domain(),
            _ => {
                return Err(SheetBlendError::Defect {
                    why: "an iso curve",
                });
            }
        };
        let e = draft
            .add_edge(EdgeGeom::new(EdgeShape::Curve { curve, interval }, ends))
            .map_err(make)?;
        edges.push(e);
        let line = Line2::new(origin, dir)
            .map_err(|e| SheetBlendError::Make(MakeError::Defect(Defect::Curve(e))))?;
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
        Err(un) => {
            return Err(SheetBlendError::Make(MakeError::Defect(Defect::Finish(
                un.cause,
            ))));
        }
    };
    let Ok([v0, u1, v1, u0]) = <[EdgeId; 4]>::try_from(edges) else {
        return Err(SheetBlendError::Defect {
            why: "the four sides",
        });
    };
    Ok((
        body,
        Square {
            face,
            v0,
            u1,
            v1,
            u0,
        },
    ))
}
