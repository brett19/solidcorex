// Written for solidcorex. Apache-2.0.
//! Gap closure for a use with a recorded accuracy (research 72 §14.17; the pin's #16735).
//!
//! A pcurve recorded with an accuracy runs within that bound of its edge's curve, not on
//! it, so two faces of an edge leave a gap between them and the divergence theorem's fields
//! leak through it, to first order in the bound and differently for each origin. The face is
//! closed onto the edges instead, so the faces share the edges' curves and the vertices
//! exactly and bound one closed surface:
//!
//! - along each such use, a **strip**: the ruled surface from the pcurve's image `P(t)` to
//!   the edge's curve `E(t)` at the same `t`. A use's pcurve runs over the edge's own
//!   interval, so `t` is the correspondence the record states and no nearest-point search
//!   is made (the pin marches a grid of nearest points because its trims have no such
//!   promise);
//! - at each corner of the loop next to such a use, a **fan** over the polygon `P_end`,
//!   `E_end`, the vertex, `E_start`, `P_start`, from its centroid.
//!
//! Both are the flux of the volume integrands through a surface the face's boundary does not
//! reach, so only the volume terms ([`Set::volume_start`]) take them; the area is the face's
//! own. A use with no recorded accuracy has an exact pcurve and gets nothing.
//!
//! The strip is integrated along `t` by the face's own step control (a tenth of the
//! accuracy, its estimate in the face's error); both are polynomial of degree 4 across the
//! ruling and so taken there by the 3-point Gauss rule, exactly. What the closure cannot
//! count is the tube between the recorded surface and the closed one, `pi tol^2 L` for a
//! use of length `L` and bound `tol`, and a ball `4/3 pi tol^3` at a corner: each is added
//! to the error by the divergence of each integrand over that volume ([`model_error`]).

use scx_curve::Curve;
use scx_num::fmath;
use scx_num::vector2d::{Point2, Vec2};
use scx_num::vector3d::{Point3, Vec3};
use scx_nurbs::Approach;
use scx_surface::Surface;

use crate::integrand::{FIRST_MONOMIAL, MONOMIALS, Set};
use crate::kronrod::{Fail, MAX_COMPONENTS};

/// A use's edge, for the strip along it.
#[derive(Clone, Copy)]
pub(crate) struct Strip<'a> {
    /// The edge's curve.
    pub(crate) edge: &'a Curve,
    /// The use's recorded bound, in millimetres.
    pub(crate) tol: f64,
    /// The vertex the use starts at, and the one it ends at, in the loop's direction.
    pub(crate) start: Point3,
    pub(crate) end: Point3,
}

/// The 3-point Gauss rule's nodes and weights on `[-1, 1]`.
const GAUSS3: [(f64, f64); 3] = [
    (-0.774_596_669_241_483_377_035_853_079_956_480, 5.0 / 9.0),
    (0.0, 8.0 / 9.0),
    (0.774_596_669_241_483_377_035_853_079_956_480, 5.0 / 9.0),
];

/// `weight` times the volume terms of `set` at `x` (about the reference) with area vector
/// `n`, added to `out`.
fn add(set: Set, x: Vec3, n: Vec3, weight: f64, out: &mut [f64]) {
    let len = set.len();
    let mut at = [0.0; MAX_COMPONENTS];
    set.eval(x, n, at.get_mut(..len).unwrap_or_default());
    for (o, a) in out.iter_mut().zip(&at).take(len).skip(set.volume_start()) {
        *o += weight * a;
    }
}

/// The ruled patch `X = (1 - w) a + w c` of rails `a(t)`, `c(t)` with derivatives `da`, `dc`,
/// at one `t`, its area vector `X_t x X_w`, over `w` in `[0, 1]`, times `weight`.
fn ruled(set: Set, a: Vec3, da: Vec3, c: Vec3, dc: Vec3, weight: f64, out: &mut [f64]) {
    let xw = c - a;
    for (node, wg) in GAUSS3 {
        let w = 0.5 + 0.5 * node;
        let x = a * (1.0 - w) + c * w;
        let xt = da * (1.0 - w) + dc * w;
        add(set, x, xt.cross(xw), 0.5 * wg * weight, out);
    }
}

/// The strip's integrand at `t`, into `out`: minus the flux through the ruled patch from the
/// face's image at `uv` to the edge at `t`, taken along the loop (`duv` the pcurve's
/// derivative and `sign` the use's, in the loop's direction). Minus, because the patch lies
/// beyond the face's boundary, on the side its normal does not point to.
pub(crate) fn strip_at(
    set: Set,
    surface: &Surface,
    strip: &Strip<'_>,
    sign: f64,
    reference: Vec3,
    (uv, duv): (Point2, Vec2),
    t: f64,
    out: &mut [f64],
) -> Result<(), Fail> {
    let j = surface.jet::<1>(uv, [Approach::Above; 2]);
    let (Some(su), Some(sv)) = (j.get(1, 0), j.get(0, 1)) else {
        return Err(Fail::NotFinite);
    };
    let e = strip.edge.jet::<1>(t, Approach::Above);
    let [de] = e.d;
    let (p, dp) = (j.point() - reference, su * duv.x + sv * duv.y);
    let origin = Point3::new(0.0, 0.0, 0.0);
    ruled(
        set,
        p,
        dp,
        (e.point - origin) - reference,
        de * sign,
        -1.0,
        out,
    );
    Ok(())
}

/// The corner fan over `points` (about the reference): the polygon's triangles from its
/// centroid, each a ruled patch, closing the surface between two uses' strips. Added into
/// `out`.
pub(crate) fn fan(set: Set, points: &[Vec3], out: &mut [f64]) {
    if points.is_empty() {
        return;
    }
    let sum = points.iter().fold(Vec3::new(0.0, 0.0, 0.0), |a, &p| a + p);
    let centre = sum * (1.0 / points.len() as f64);
    let zero = Vec3::new(0.0, 0.0, 0.0);
    for (i, &p) in points.iter().enumerate() {
        let q = points
            .get(i + 1)
            .or_else(|| points.first())
            .copied()
            .unwrap_or(p);
        let side = q - p;
        for (node, wg) in GAUSS3 {
            let t = 0.5 + 0.5 * node;
            // The ruled patch's area vector is minus the polygon's right-hand one.
            ruled(set, centre, zero, p + side * t, side, -0.5 * wg, out);
        }
    }
}

/// The error the closure adds for a volume `volume` at distance at most `reach` from the
/// reference: the divergence theorem over that volume, per component, of `x^a y^b z^c n_z`'s
/// field `(0, 0, x^a y^b z^c)`, whose divergence is `c x^a y^b z^(c-1)`.
pub(crate) fn model_error(set: Set, volume: f64, reach: f64, error: &mut [f64]) {
    let len = set.len();
    for (i, e) in error
        .iter_mut()
        .enumerate()
        .take(len)
        .skip(set.volume_start())
    {
        let m = match set {
            Set::Volume => [0, 0, i as u8],
            _ => MONOMIALS
                .get(i.saturating_sub(FIRST_MONOMIAL))
                .copied()
                .unwrap_or([0, 0, 0]),
        };
        let c = f64::from(m[2]);
        if c > 0.0 {
            let degree = i32::from(m[0] + m[1] + m[2]) - 1;
            *e += c * fmath::powi(reach, degree) * volume;
        }
    }
}
