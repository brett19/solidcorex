// Written for solidcorex. Apache-2.0.
//! The exact rational image of an analytic kind over a bounded extent (research 63 §4.2,
//! §4.8; D4, D13).
//!
//! Every kind is a tensor product of its two directions: a length is degree 1 (the parameter
//! itself), an angle the rational quadratic arcs of [`Arcs::bezier`]. The control point at
//! `(i, j)` is the kind's own formula `O + f D + g Z` at the directions' control values,
//! which is exact because the formula is affine in each direction's homogeneous point, and
//! the weight is the product of the directions' weights. So the image keeps the kind's uv:
//! at the same `uv` both give the same point, within the rounding of the control data.
//!
//! The image is exact in the reals and, in doubles, off by the rounding of its control
//! data: a quarter arc in the pin's parameter has middle weight `cos(pi/4)` relative to its
//! ends, which no double holds (63 §4.8).

use scx_num::extent1d::Extent1d;
use scx_num::extent2d::Extent2d;
use scx_num::vector3d::Point3;
use scx_nurbs::KnotVector;
use scx_nurbs_surface::{Net, NurbsSurface, NurbsSurfaceError, ParamAxis};

use crate::arcs::Arcs;
use crate::error::SurfaceError;

/// What one direction's extent may be.
#[derive(Clone, Copy, Debug)]
pub(crate) enum Reach {
    /// Any finite extent.
    Any,
    /// At least this value (the cone's apex, opening up).
    AtLeast(f64),
    /// At most this value (the cone's apex, opening down).
    AtMost(f64),
    /// The arcs: at most one period of a turn, or inside the half turn's `[0, 1]`.
    Arcs(Arcs),
}

/// One direction of the image: its degree, knots and control values `(c, s, w)`. A length
/// has `(t, 0, 1)`; an angle the unit circle's control point and weight.
pub(crate) struct Dir {
    degree: usize,
    knots: Vec<f64>,
    ctrl: Vec<(f64, f64, f64)>,
}

impl Dir {
    /// The direction `axis` over `ext`, refused where it leaves `reach`.
    pub(crate) fn new(axis: ParamAxis, ext: Extent1d, reach: Reach) -> Result<Self, SurfaceError> {
        let (lo, hi) = (ext.min(), ext.max());
        if lo == hi {
            return Err(SurfaceError::EmptyExtent { axis, at: lo });
        }
        let outside = || SurfaceError::OutsideDomain {
            axis,
            min: lo,
            max: hi,
        };
        match reach {
            Reach::AtLeast(a) if lo < a => return Err(outside()),
            Reach::AtMost(a) if hi > a => return Err(outside()),
            Reach::Any | Reach::AtLeast(_) | Reach::AtMost(_) => {}
            Reach::Arcs(arcs) => {
                let fits = if arcs.is_turn() {
                    hi - lo <= arcs.length()
                } else {
                    0.0 <= lo && hi <= arcs.length()
                };
                if !fits {
                    return Err(outside());
                }
                let (knots, ctrl) = arcs.bezier(lo, hi);
                return Ok(Self {
                    degree: 2,
                    knots,
                    ctrl,
                });
            }
        }
        Ok(Self {
            degree: 1,
            knots: vec![lo, lo, hi, hi],
            ctrl: vec![(lo, 0.0, 1.0), (hi, 0.0, 1.0)],
        })
    }
}

/// The surface over `u` (rows) and `v` (columns), with control point `point(cu, cv)` at the
/// directions' control values and weight `wu wv`; polynomial when `rational` is false (both
/// directions lengths, every weight 1).
pub(crate) fn build(
    u: Dir,
    v: Dir,
    rational: bool,
    point: impl Fn((f64, f64), (f64, f64)) -> Point3,
) -> Result<NurbsSurface<3>, SurfaceError> {
    let conv = SurfaceError::Conversion;
    let (rows, cols) = (u.ctrl.len(), v.ctrl.len());
    let mut points = Vec::with_capacity(rows * cols);
    let mut weights = Vec::with_capacity(if rational { rows * cols } else { 0 });
    for &(cu, su, wu) in &u.ctrl {
        for &(cv, sv, wv) in &v.ctrl {
            points.push(point((cu, su), (cv, sv)).to_array());
            if rational {
                weights.push(wu * wv);
            }
        }
    }
    let knots = |axis, d: Dir| {
        KnotVector::new(d.degree, d.knots)
            .map_err(|cause| conv(NurbsSurfaceError::BadKnots { axis, cause }))
    };
    let ku = knots(ParamAxis::U, u)?;
    let kv = knots(ParamAxis::V, v)?;
    let net = Net::new(rows, cols, points).map_err(conv)?;
    let weights = if rational {
        Some(Net::new(rows, cols, weights).map_err(conv)?)
    } else {
        None
    };
    NurbsSurface::new(ku, kv, net, weights).map_err(conv)
}

/// The restriction of a NURBS surface to `over`, which must lie in its domain: split at
/// each end of `over` strictly inside the domain (`NurbsSurface::split`), the surface
/// itself where `over` is its domain.
pub(crate) fn restrict(
    n: &NurbsSurface<3>,
    over: &Extent2d,
) -> Result<NurbsSurface<3>, SurfaceError> {
    let have = n.domain();
    let mut s = n.clone();
    for (axis, want, have) in [
        (ParamAxis::U, over.x_interval(), have.x_interval()),
        (ParamAxis::V, over.y_interval(), have.y_interval()),
    ] {
        let (lo, hi) = (want.min(), want.max());
        if lo == hi {
            return Err(SurfaceError::EmptyExtent { axis, at: lo });
        }
        if lo < have.min() || hi > have.max() {
            return Err(SurfaceError::OutsideDomain {
                axis,
                min: lo,
                max: hi,
            });
        }
        if lo > have.min() {
            s = s.split(axis, lo).map_err(SurfaceError::Conversion)?.1;
        }
        if hi < have.max() {
            s = s.split(axis, hi).map_err(SurfaceError::Conversion)?.0;
        }
    }
    Ok(s)
}
