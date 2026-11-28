// Written for solidcorex. Apache-2.0.
//! [`edge_comb`] and [`iso_comb`]: the curvature vectors a comb is drawn from, along an edge's
//! curve and along a parameter line of a face (research 74 §52, the pin's `edge_comb` and
//! `iso_comb`).
//!
//! A tooth is a point and a vector of the curvature there, per mm. Along a curve the vector is
//! `kappa N`, towards the centre of curvature, from the first two derivatives:
//! `(C' x C'') x C' / |C'|^4` (zero where the curve is straight). Along a parameter line of a
//! surface it is the normal curvature times the surface's unit normal, `(n . S_tt / |S_t|^2) n`:
//! the part of the line's curvature the surface itself causes. The vector does not depend on the
//! sign of `n`.
//!
//! **No tooth is made up.** A station where the curve has no tangent (a cusp) or the surface no
//! normal or tangent along the line (a pole, where the partials do not span a plane to the
//! surface's own rounding) has no tooth: it is left out, and each tooth names
//! its parameter, so a caller sees where the comb has a gap. The stations are `teeth` even
//! steps of the parameter, the ends included; a closed edge's end is its start, so it takes
//! `teeth` steps and the start once.
//!
//! One budget unit is charged for each station, and `stop` is polled before each.

use scx_brep::{Brep, EdgeId, EdgeShape, Entity, FaceId};
use scx_num::budget::{Budget, Stop};
use scx_num::vector2d::Point2;
use scx_num::vector3d::{Point3, Vec3};
use scx_nurbs::Approach;

use crate::curvature::pole;
use crate::error::{BodyqError, BodyqRefusal};
use crate::geom::ABOVE;
use crate::reach::finite;

/// A tooth of a curvature comb.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Tooth {
    /// The running parameter the tooth stands at: the edge curve's, or the iso line's.
    pub at: f64,
    /// Where the tooth stands.
    pub point: Point3,
    /// The curvature vector, per mm. Its length is the curvature.
    pub curvature: Vec3,
}

/// Which parameter line of a surface runs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Running {
    /// `u` runs and `v` is fixed.
    U,
    /// `v` runs and `u` is fixed.
    V,
}

/// The parameter at station `i` of `steps` between `lo` and `hi`, the end exactly.
fn station(lo: f64, hi: f64, i: usize, steps: usize) -> f64 {
    if i == steps {
        hi
    } else {
        lo + (hi - lo) * (i as f64 / steps as f64)
    }
}

fn count(teeth: usize) -> Result<(), BodyqError> {
    if teeth < 2 {
        return Err(BodyqError::Refused(BodyqRefusal::Count));
    }
    Ok(())
}

/// The curvature comb of `edge`: [`Tooth`]s at `teeth` even steps of its curve's interval.
///
/// # Errors
///
/// Refused for an edge the body does not have, a collapsed edge, or fewer than two teeth
/// ([`Count`](BodyqRefusal::Count)). Exhausted or cancelled by `budget` and `stop`.
pub fn edge_comb(
    body: &Brep,
    edge: EdgeId,
    teeth: usize,
    budget: &mut Budget,
    stop: &Stop,
) -> Result<Vec<Tooth>, BodyqError> {
    count(teeth)?;
    let e = body
        .edge(edge)
        .ok_or(BodyqError::Refused(BodyqRefusal::NoEntity {
            entity: Entity::Edge(edge),
        }))?;
    let EdgeShape::Curve { curve, interval } = e.shape() else {
        return Err(BodyqError::Refused(BodyqRefusal::Collapsed { edge }));
    };
    let [v0, v1] = e.ends();
    let steps = if v0 == v1 { teeth } else { teeth - 1 };
    let mut out = Vec::with_capacity(teeth);
    for i in 0..teeth {
        stop.check()?;
        budget.charge(1)?;
        let t = station(interval.min(), interval.max(), i, steps);
        let j = curve.jet::<2>(t, Approach::Above);
        let [d1, d2] = j.d;
        let s2 = d1.dot(d1);
        if !s2.is_finite() || s2 <= 0.0 {
            continue;
        }
        let curvature = d1.cross(d2).cross(d1) * (1.0 / (s2 * s2));
        if j.point.is_finite() && curvature.is_finite() {
            out.push(Tooth {
                at: t,
                point: j.point,
                curvature,
            });
        }
    }
    Ok(out)
}

/// The comb along a parameter line of `face`'s surface: the `running` parameter over `span`
/// at the other held at `fixed`, [`Tooth`]s at `teeth` even steps of the span.
///
/// # Errors
///
/// Refused for a face the body does not have, a parameter that is not finite, or fewer than
/// two teeth. Exhausted or cancelled by `budget` and `stop`.
pub fn iso_comb(
    body: &Brep,
    face: FaceId,
    running: Running,
    fixed: f64,
    span: [f64; 2],
    teeth: usize,
    budget: &mut Budget,
    stop: &Stop,
) -> Result<Vec<Tooth>, BodyqError> {
    count(teeth)?;
    let [s0, s1] = span;
    finite(fixed)?;
    finite(s0)?;
    finite(s1)?;
    let f = body
        .face(face)
        .ok_or(BodyqError::Refused(BodyqRefusal::NoEntity {
            entity: Entity::Face(face),
        }))?;
    let surface = f.surface();
    let mut out = Vec::with_capacity(teeth);
    for i in 0..teeth {
        stop.check()?;
        budget.charge(1)?;
        let s = station(s0, s1, i, teeth - 1);
        let uv = match running {
            Running::U => Point2::new(s, fixed),
            Running::V => Point2::new(fixed, s),
        };
        let j = surface.jet::<2>(uv, ABOVE);
        let (Some(su), Some(sv)) = (j.get(1, 0), j.get(0, 1)) else {
            continue;
        };
        let (along, bend) = match running {
            Running::U => (su, j.get(2, 0)),
            Running::V => (sv, j.get(0, 2)),
        };
        let Some(bend) = bend else { continue };
        let speed2 = along.dot(along);
        let Some(n) = su.cross(sv).normalized() else {
            continue;
        };
        let n = n.as_vec();
        // a pole: the partials do not span a tangent plane (to the surface's rounding)
        if pole(su, sv) {
            continue;
        }
        let curvature = n * (n.dot(bend) / speed2);
        let point = j.point().to_point();
        if point.is_finite() && curvature.is_finite() {
            out.push(Tooth {
                at: s,
                point,
                curvature,
            });
        }
    }
    Ok(out)
}
