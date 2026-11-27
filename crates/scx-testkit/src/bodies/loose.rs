// Written for solidcorex. Apache-2.0.
//! A valid body whose trims carry a recorded accuracy (research 72 §14.17, MA2): the body of
//! a kind with the pcurves of its straight and constant-`v` uses moved off their edges, each
//! use stating the bound it keeps, so that a measure of it must close the gaps it leaves.
//!
//! The body is valid in the sense that matters to a measure: its topology is the kind's, its
//! edges and vertices are exact, and each moved use's recorded bound holds (it is measured
//! by sampling and rounded up). It is not one a checker that holds pcurves to the
//! resolution would pass ([`Invariant::PcurveOffEdge`](super::Invariant)): the recorded bound
//! is what says the pcurve may be off.

use std::sync::Arc;

use scx_brep::{Draft, EdgeId, EdgeShape, FaceId, UseGeom};
use scx_curve::{AngleAxis, AngleLine2, Circle2, Curve2, Line2};
use scx_num::cert::{Basis, Recorded};
use scx_num::units::Accuracy;
use scx_num::units::Length;
use scx_num::vector2d::Vec2;

use super::{GenError, Kind, Result, generate};

/// Samples a use's distance from its edge is measured at.
const SAMPLES: u32 = 24;

/// `kind`'s body for `seed` at `size` with every line and constant-`v` pcurve moved by
/// `offset` in uv: along the loop's left normal (into the face), and, on every second use
/// of a loop, `offset / 2` along the pcurve (a parametric slip, so the end of a trim is off
/// its vertex too). Each moved use carries the bound it measured. A circle's radius changes by the
/// offset instead; the other pcurve kinds stay exact, with no record.
///
/// # Errors
///
/// [`GenError`] only on a bug.
pub fn loose(kind: Kind, seed: u64, size: usize, offset: f64) -> Result<Draft> {
    let mut d = generate(kind, seed, size)?.draft;
    let faces: Vec<FaceId> = d.faces().map(scx_brep::Face::id).collect();
    for f in faces {
        // The edges' curves, read before the face is borrowed for writing.
        let curves: Vec<(EdgeId, Option<scx_curve::Curve>)> = {
            let face = d.face(f).ok_or(GenError::Degenerate("face"))?;
            face.loops()
                .flat_map(scx_brep::Loop::uses)
                .map(|u| {
                    let edge = u.record().edge;
                    let curve = d.edge(edge).and_then(|e| match e.shape() {
                        Some(EdgeShape::Curve { curve, .. }) => Some(curve.clone()),
                        _ => None,
                    });
                    (edge, curve)
                })
                .collect()
        };
        let rec = d.face_mut(f).ok_or(GenError::Degenerate("face"))?;
        let mut at = 0;
        for lp in &mut rec.loops {
            for (k, u) in lp.uses.iter_mut().enumerate() {
                let curve = curves.get(at).and_then(|c| c.1.as_ref());
                at += 1;
                let (
                    UseGeom::Pcurve {
                        curve: pc,
                        interval,
                    },
                    Some(edge),
                ) = (&mut u.geom, curve)
                else {
                    continue;
                };
                // The pcurve's direction along the loop, and its left normal.
                let (lo, hi) = (interval.min(), interval.max());
                let mid = 0.5 * (lo + hi);
                let h = 1e-3 * (hi - lo);
                let mut tangent = pc.point(mid + h) - pc.point(mid - h);
                if matches!(u.sense, scx_num::frame::Sense::Reversed) {
                    tangent = -tangent;
                }
                let len = tangent.length();
                if len.is_nan() || len <= 0.0 {
                    continue;
                }
                let tangent = tangent * (1.0 / len);
                let normal = Vec2::new(-tangent.y, tangent.x);
                let slip = if k % 2 == 1 { 0.5 } else { 0.0 };
                let shift = (normal + tangent * slip) * offset;
                let Some(moved) = moved(pc, shift, normal, mid)? else {
                    continue;
                };
                // The bound it keeps: the largest distance sampled, rounded up.
                let surface = &rec.surface;
                let mut far = 0.0_f64;
                for i in 0..=SAMPLES {
                    let t = interval.min()
                        + (interval.max() - interval.min()) * f64::from(i) / f64::from(SAMPLES);
                    far = far.max((surface.point(moved.point(t)) - edge.point(t)).length());
                }
                let bound = Accuracy::new(Length::from_mm(far * 1.001 + 1e-15))
                    .ok_or(GenError::Degenerate("bound"))?;
                *pc = moved;
                u.bound = Some(Arc::new(Recorded {
                    acc: bound,
                    basis: Basis::Measured,
                }));
            }
        }
    }
    Ok(d)
}

/// `curve` moved by `shift` in uv; `None` for a kind that is not moved. An angle line moves
/// along its other coordinate only (`shift.y`), its angle being the surface's, not a length;
/// a circle keeps its centre and its angle and has its radius changed by the shift's length,
/// less where `normal` (the loop's left) points at the centre, so each moves into the face.
fn moved(curve: &Curve2, shift: Vec2, normal: Vec2, at: f64) -> Result<Option<Curve2>> {
    Ok(match curve {
        Curve2::Circle(c) => {
            let toward = normal.dot(c.point(at) - c.frame().origin()) < 0.0;
            let r = c.radius().mm() + if toward { -1.0 } else { 1.0 } * shift.length();
            Some(Curve2::Circle(Circle2::new(
                *c.frame(),
                Length::from_mm(r),
                c.sense(),
            )?))
        }
        Curve2::Line(l) => Some(Curve2::Line(Line2::new(l.origin() + shift, l.direction())?)),
        Curve2::AngleLine(a) if matches!(a.axis(), AngleAxis::U) => {
            Some(Curve2::AngleLine(AngleLine2::new(
                a.arcs(),
                a.axis(),
                a.start(),
                a.sense(),
                a.other() + shift.y,
                a.rate(),
            )?))
        }
        _ => None,
    })
}
