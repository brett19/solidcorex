// Written for solidcorex. Apache-2.0.
//! [`blend_edges`]: the section blend between an edge of one sheet and an edge of another,
//! as a one-face NURBS sheet.

use std::sync::Arc;

use scx_brep::{Brep, Document, EdgeId, EdgeShape, FaceId};
use scx_curve::Curve;
use scx_num::budget::{Budget, Stop};
use scx_num::extent1d::Extent1d;
use scx_num::units::{Accuracy, Length};
use scx_num::vector2d::Point2;
use scx_num::vector3d::Vec3;
use scx_nurbs::{Approach, KnotVector, NurbsCurve};
use scx_nurbs_surface::NurbsSurface;
use scx_surface::certificate::{Node, SpanBound, span_bound};

use super::definition::SectionBlendDefinition;
use super::error::{SheetBlendError, SheetBlendRefusal, refused};
use super::frames::{Walk, open_section_turns, section_frames_outward};
use super::nurbs_face::square_sheet;
use crate::Made;
use crate::blend::EndOrder;

/// One side of an edge blend: the sheet, its edge, and the order the blend holds there.
#[derive(Debug, Clone, Copy)]
pub struct BlendSide<'a> {
    /// The sheet.
    pub body: &'a Brep,
    /// Its edge the blend starts or ends on.
    pub edge: EdgeId,
    /// The continuity the blend holds with the sheet's face across the edge.
    pub order: EndOrder,
}

/// How far a blend's boundary is from the sheet edge it meets.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SeamGap {
    /// A proved bound of the distance between the blend's boundary and the edge's curve
    /// (`scx_surface::certificate::span_bound` against the edge's exact NURBS form).
    Proved(Length),
    /// No bound is proved: the edge's curve has no exact NURBS form here (an ellipse,
    /// parabola or hyperbola), or the certificate found none.
    Unproved,
}

/// Where a blend meets one sheet.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Seam {
    /// The blend's edge along the sheet's.
    pub edge: EdgeId,
    /// How far it is from the sheet's edge.
    pub gap: SeamGap,
    /// The largest angle (radians) between the blend's and the sheet's tangent planes at the
    /// stations (the blend holds the sheet's tangent there exactly for G1 and above; between
    /// stations it is the skin's): measured, not proved.
    pub angle_at_stations: f64,
}

/// An edge blend.
#[derive(Debug, Clone)]
pub struct EdgeBlend {
    /// The one-face sheet and its face.
    pub made: Made<FaceId>,
    /// What the blend's surface is rebuilt from.
    pub definition: SectionBlendDefinition,
    /// [`crate::Blended::through`]: a certified bound of the surface's miss of its sections.
    pub through: f64,
    /// Where it meets the first and the second sheet.
    pub seams: [Seam; 2],
}

/// The blend from `from`'s edge to `to`'s with `stations` stations along each and the crowns
/// `crown`: section 0 is `from`'s edge leaving its face outward towards `to`, section 1 is
/// `to`'s edge leaving its face outward back towards `from`, each holding its own order
/// ([`section_frames_outward`], [`section_blend`](crate::section_blend)). The second edge's
/// stations run end to nearer end with the first's ([`open_section_turns`]).
///
/// The result is a one-face sheet on the blend's NURBS surface over its whole domain; its
/// edges at `u = 0` and `u = 1` lie along `from`'s and `to`'s edges, each seam's gap proved
/// against the edge's own curve under `budget`.
///
/// # Errors
///
/// [`SheetBlendRefusal::OtherDocument`] for a sheet of another resolution; the frames'
/// refusals; [`SheetBlendError::Blend`] for the blend's; `SheetBlendUnsupported::ClosedSection`
/// for a closed edge; [`SheetBlendError::Make`] when the sheet does not finish;
/// [`SheetBlendError::Surface`] for the certificate's exhaustion or stop.
pub fn blend_edges(
    from: BlendSide<'_>,
    to: BlendSide<'_>,
    stations: usize,
    crown: [f64; 2],
    doc: &Document,
    budget: &mut Budget,
    stop: &Stop,
) -> Result<EdgeBlend, SheetBlendError> {
    let res = doc.resolution();
    for (operand, side) in [from, to].iter().enumerate() {
        if side.body.resolution() != res {
            return Err(refused(SheetBlendRefusal::OtherDocument { operand }));
        }
    }
    let mid = |s: &BlendSide<'_>| -> Result<_, SheetBlendError> {
        let walk = Walk::new(s.body, &[s.edge])?;
        Ok(walk.first_middle().p)
    };
    let (ma, mb) = (mid(&from)?, mid(&to)?);
    let fa = section_frames_outward(from.body, &[from.edge], mb - ma, stations)?;
    let mut fb = section_frames_outward(to.body, &[to.edge], ma - mb, stations)?;
    let ends = |f: &[crate::SectionFrame]| match (f.first(), f.last()) {
        (Some(a), Some(b)) => Ok([a.p, b.p]),
        _ => Err(refused(SheetBlendRefusal::Stations { count: 0 })),
    };
    let turns = open_section_turns(&[ends(&fa)?, ends(&fb)?], doc)?;
    let turned = turns.get(1).copied().unwrap_or(false);
    if turned {
        fb.reverse();
    }
    let frames = [fa.clone(), fb.clone()];
    let (definition, blended) =
        SectionBlendDefinition::new(vec![fa, fb], [from.order, to.order], crown)?;
    let (body, square) = square_sheet(&blended.surface, *doc)?;
    let tol = Accuracy::new(res.zone()).ok_or(SheetBlendError::Defect {
        why: "the resolution as an accuracy",
    })?;
    let mut seam = |i: usize, side: &BlendSide<'_>, reversed: bool, edge| {
        let gap = seam_gap(&blended.surface, side, i, reversed, tol, budget, stop)?;
        let angle = frames.get(i).map_or(0.0, |f| {
            angle_at_stations(&blended.surface, i, f.len(), side, reversed)
        });
        Ok::<_, SheetBlendError>(Seam {
            edge,
            gap,
            angle_at_stations: angle,
        })
    };
    let seams = [
        seam(0, &from, false, square.u0)?,
        seam(1, &to, turned, square.u1)?,
    ];
    Ok(EdgeBlend {
        made: Made {
            body,
            faces: square.face,
        },
        definition,
        through: blended.through,
        seams,
    })
}

/// The proved gap between the blend's section `i` boundary (`u = i`) and `side`'s edge.
fn seam_gap(
    surface: &NurbsSurface<3>,
    side: &BlendSide<'_>,
    i: usize,
    reversed: bool,
    tol: Accuracy,
    budget: &mut Budget,
    stop: &Stop,
) -> Result<SeamGap, SheetBlendError> {
    let edge = side
        .body
        .edge(side.edge)
        .ok_or_else(|| refused(SheetBlendRefusal::NotEdge(side.edge)))?;
    let EdgeShape::Curve { curve, interval } = edge.shape() else {
        return Err(refused(SheetBlendRefusal::NotEdge(side.edge)));
    };
    let Some(rail) = rail(curve, *interval) else {
        return Ok(SeamGap::Unproved);
    };
    #[allow(clippy::cast_precision_loss)] // 0 or 1
    let u = i as f64;
    let (va, vb) = if reversed { (1.0, 0.0) } else { (0.0, 1.0) };
    let a = Node {
        t: rail.domain().min(),
        uv: Point2::new(u, va),
    };
    let b = Node {
        t: rail.domain().max(),
        uv: Point2::new(u, vb),
    };
    Ok(match span_bound(surface, &rail, a, b, tol, budget, stop)? {
        SpanBound::Bounded(l) => SeamGap::Proved(l),
        SpanBound::Unbounded => SeamGap::Unproved,
    })
}

/// The largest angle between the blend's tangent plane at `u = i` and the sheet's at the
/// stations.
fn angle_at_stations(
    surface: &NurbsSurface<3>,
    i: usize,
    stations: usize,
    side: &BlendSide<'_>,
    reversed: bool,
) -> f64 {
    let Ok(walk) = Walk::new(side.body, &[side.edge]) else {
        return f64::NAN;
    };
    let Ok(at) = walk.stations(stations) else {
        return f64::NAN;
    };
    let blend = scx_surface::Surface::Nurbs(Arc::new(surface.clone()));
    #[allow(clippy::cast_precision_loss)] // 0 or 1, and a count of stations
    let (u, last) = (i as f64, (stations - 1) as f64);
    let mut worst: f64 = 0.0;
    for (k, s) in at.iter().enumerate() {
        let face = walk.faces.first().copied();
        let Some(Ok((sheet, uv, _))) = face.map(|f| walk.on_face(side.body, f, s)) else {
            return f64::NAN;
        };
        #[allow(clippy::cast_precision_loss)] // a count of stations
        let v = if reversed {
            1.0 - k as f64 / last
        } else {
            k as f64 / last
        };
        let n1 = blend.normal(Point2::new(u, v), [Approach::Above; 2]);
        let n2 = sheet.normal(uv, [Approach::Above; 2]);
        let (Some(n1), Some(n2)) = (n1, n2) else {
            return f64::NAN;
        };
        let c = Vec3::from(n1).dot(Vec3::from(n2)).abs().min(1.0);
        worst = worst.max(scx_num::vector3d::acos(c));
    }
    worst
}

/// The exact NURBS form of `curve` over `interval`, on the curve's own parameter at the ends:
/// a line's degree-1 form, a circle's rational quadratic arcs (at most a quarter turn each),
/// a NURBS curve restricted. `None` for the other conics.
pub(super) fn rail(curve: &Curve, interval: Extent1d) -> Option<NurbsCurve<3>> {
    let (t0, t1) = (interval.min(), interval.max());
    match curve {
        Curve::Line(_) => {
            let (p, q) = (curve.point(t0), curve.point(t1));
            let k = KnotVector::new(1, vec![t0, t0, t1, t1]).ok()?;
            NurbsCurve::new(k, vec![p.to_array(), q.to_array()], None).ok()
        }
        Curve::Circle(_) => {
            let quarter = core::f64::consts::FRAC_PI_2;
            let span = t1 - t0;
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)] // a few pieces
            let pieces = ((span / quarter).ceil() as usize).max(1);
            #[allow(clippy::cast_precision_loss)] // a few pieces
            let at = |k: usize| t0 + span * (k as f64 / pieces as f64);
            #[allow(clippy::cast_precision_loss)] // a few pieces
            let w = scx_num::fmath::cos(0.5 * span / pieces as f64);
            let mut knots = vec![t0, t0, t0];
            let mut points = vec![curve.point(t0).to_array()];
            let mut weights = vec![1.0];
            for k in 0..pieces {
                let (a, b) = (at(k), at(k + 1));
                let (pa, pb) = (curve.point(a), curve.point(b));
                let ta = curve.jet::<1>(a, Approach::Above).d[0].normalized()?;
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
            let k = KnotVector::new(2, knots).ok()?;
            NurbsCurve::new(k, points, Some(weights)).ok()
        }
        Curve::Nurbs(_) => match curve.restrict(interval).ok()? {
            Curve::Nurbs(n) => Some(Arc::unwrap_or_clone(n)),
            _ => None,
        },
        _ => None,
    }
}
