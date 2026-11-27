// Written for solidcorex. Apache-2.0.
//! Edits by value (04, 11-27): `restrict`, `reversed` and `transformed` on each kind and on
//! [`Surface`], with the [`UvMap`] from the original's parameters to the edit's (research 63
//! §4.5, SU7). The surface version of `scx_curve`'s edits: the same rules, a reversal that
//! negates the parameter, a refusal where there is no exact answer, and the exact image
//! under an affine map with its parameter map.
//!
//! | solidcore at the pin | here |
//! |---|---|
//! | `Surface::reversed(Direction)` on the NURB: the knots reflected about `U[0] + U[m]` | `reversed(axis)`: `S'(-u, v) = S(u, v)` (or in `v`) for every kind but the sphere's latitude, which runs `1 - v`; exact for the analytic kinds |
//! | `Transform` of an analytic kind's NURB (any affine map; the kind is lost) | `transformed`: a plane under any affine map, a surface of revolution under a similarity, with the [`UvMap`]; any other map of a surface of revolution is [`SurfaceError::NotSimilar`] for now: its NURBS image needs an extent, and the signature for that is open (63 §4.9); a caller can write `to_nurbs(over)?.transformed(a)` |
//! | `Trim` of a NURB | [`Surface::restrict`]: a NURBS surface split at each end inside its domain; an analytic kind carries no extent, so it is itself |

use std::sync::Arc;

use scx_num::domain::ParamDomain;
use scx_num::extent1d::Extent1d;
use scx_num::extent2d::Extent2d;
use scx_num::frame::{Frame, Sense};
use scx_num::math::Affine;
use scx_num::units::Length;
use scx_num::vector2d::Point2;
use scx_num::vector3d::Vec3;
use scx_nurbs_surface::{NurbsSurface, ParamAxis};

use crate::{Cone, Cylinder, Plane, Result, Sphere, Surface, SurfaceError, Torus};

/// How an edited surface's parameters relate to the original's: the point at `uv` on the
/// original is at [`UvMap::apply`]`(uv)` on the edit, an affine map of the parameters.
///
/// It is the identity for a restriction and for a NURBS surface's transform; a reversal's
/// negates one parameter (the sphere's latitude maps to `1 - v`); a transform's scales
/// the directions measured in lengths (the plane's two, the cylinder's arc length and
/// height, the cone's height) by the map's scale, and for a plane under a map that is not a
/// similarity it is triangular (the image's frame takes the image of the x axis). A face's
/// pcurves move with it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct UvMap {
    /// Rows: `u' = m[0][0] u + m[0][1] v + c[0]`, `v' = m[1][0] u + m[1][1] v + c[1]`.
    m: [[f64; 2]; 2],
    c: [f64; 2],
}

impl Default for UvMap {
    fn default() -> Self {
        Self::IDENTITY
    }
}

impl UvMap {
    /// No change.
    pub const IDENTITY: Self = Self::scaling(1.0, 1.0);

    /// `(su u, sv v)`.
    const fn scaling(su: f64, sv: f64) -> Self {
        Self {
            m: [[su, 0.0], [0.0, sv]],
            c: [0.0, 0.0],
        }
    }

    /// The parameter of `axis` negated.
    const fn negating(axis: ParamAxis) -> Self {
        match axis {
            ParamAxis::U => Self::scaling(-1.0, 1.0),
            ParamAxis::V => Self::scaling(1.0, -1.0),
        }
    }

    /// The linear part, by rows.
    #[must_use]
    pub const fn matrix(&self) -> [[f64; 2]; 2] {
        self.m
    }

    /// The constant part.
    #[must_use]
    pub const fn offset(&self) -> [f64; 2] {
        self.c
    }

    /// The edit's parameters of the point at `uv` on the original. A negation or a
    /// diagonal scale rounds at most once per coordinate (a negation not at all).
    #[must_use]
    pub fn apply(&self, uv: Point2) -> Point2 {
        let [[a, b], [c, d]] = self.m;
        let [e, f] = self.c;
        let row = |p: f64, q: f64, k: f64| {
            // Skip the zero terms, so a diagonal map is exact where its scale is and an
            // infinite parameter stays infinite rather than NaN.
            let mut s = 0.0;
            let mut any = false;
            for (w, x) in [(p, uv.x), (q, uv.y)] {
                if w != 0.0 {
                    s = if any { s + w * x } else { w * x };
                    any = true;
                }
            }
            if k == 0.0 { s } else { s + k }
        };
        Point2::new(row(a, b, e), row(c, d, f))
    }
}

// ---------------------------------------------------------------- similarity

/// The roundings within which a map is taken as a similarity, as `scx_curve`'s conics
/// take it (16 roundings; research 63 §3.9).
const SIMILAR: f64 = 16.0 * f64::EPSILON;

/// Whether the vectors are pairwise square and of one length, to [`SIMILAR`].
fn similar(vs: &[Vec3]) -> bool {
    vs.iter().enumerate().all(|(i, a)| {
        vs.iter().skip(i + 1).all(|b| {
            let (p, q, r) = (a.dot(*a), b.dot(*b), a.dot(*b));
            (p - q).abs() <= SIMILAR * (p + q) && r.abs() <= SIMILAR * (p + q)
        })
    })
}

/// How much `a` stretches the unit vector `v`: `|A v| / |v|`, so a stored axis a rounding
/// off unit length still gives an exact stretch under an exact map (a power-of-two scale, a
/// mirror, a quarter turn about a world axis: the lengths are summed in any order).
fn stretch(image: Vec3, v: Vec3) -> f64 {
    image.length_any_order() / v.length_any_order()
}

/// The image frame, its sense and scale under a similarity: the frame through
/// [`Frame::transformed`], the scale the mean stretch of the axes.
fn similarity(frame: &Frame, a: &Affine) -> Result<(Frame, Sense, f64)> {
    if !a.is_finite() {
        return Err(SurfaceError::MapNotFinite);
    }
    let units = [frame.x().as_vec(), frame.y().as_vec(), frame.z().as_vec()];
    let axes = units.map(|v| a.apply_vec(v));
    if !a.apply(frame.origin()).is_finite() || axes.iter().any(|v| !v.is_finite()) {
        return Err(SurfaceError::MapNotFinite);
    }
    let (image, sense) = frame.transformed(a).ok_or(SurfaceError::MapFlattens)?;
    if !similar(&axes) {
        return Err(SurfaceError::NotSimilar);
    }
    let k = axes
        .iter()
        .zip(units)
        .map(|(&w, v)| stretch(w, v))
        .sum::<f64>()
        / 3.0;
    if !k.is_finite() {
        return Err(SurfaceError::MapNotFinite);
    }
    Ok((image, sense, k))
}

/// `sense` composed with the map's.
fn compose(sense: Sense, map: Sense) -> Sense {
    match map {
        Sense::Forward => sense,
        Sense::Reversed => sense.reversed(),
    }
}

fn length(mm: f64) -> Length {
    Length::from_mm(mm)
}

// ---------------------------------------------------------------- the kinds

impl Plane {
    /// The same plane run the other way along `axis`, `S'(-u, v) = S(u, v)` (or in `v`):
    /// the frame turned half a turn about its other axis, so the normal flips. Exact: `S'`
    /// at the negated parameter has `S`'s bits.
    #[must_use]
    pub fn reversed(&self, axis: ParamAxis) -> (Self, UvMap) {
        let frame = match axis {
            ParamAxis::U => self.frame().half_turn_about_y(),
            ParamAxis::V => self.frame().half_turn_about_x(),
        };
        (Self::new(frame), UvMap::negating(axis))
    }

    /// The image under any affine map `a`, and its [`UvMap`]: the plane through the image
    /// of the origin, on the frame whose x is the direction of the image `A x` of the frame's x
    /// and whose z is the direction of `A x × A y` ([`Frame::transformed`], which takes `± A z`
    /// scaled back where that is square to the image plane, so an exact map and its inverse
    /// give the plane back to the bit). Then `A y = (A y · x') x' + (A y · y') y'`, so the map
    /// is triangular, `u' = sx u + (A y · x') v`, `v' = (A y · y') v`, with `sx = |A x| / |x|`
    /// the stretch of x. Where `A x` and `A y` are square and of one length to 16 roundings (a
    /// similarity on the plane) it is the scale by the mean stretch of x and y in both
    /// directions.
    ///
    /// # Errors
    ///
    /// [`SurfaceError::MapNotFinite`] for a map or an image that is not finite,
    /// [`SurfaceError::MapFlattens`] where `A x` and `A y` are parallel.
    pub fn transformed(&self, a: &Affine) -> Result<(Self, UvMap)> {
        let f = self.frame();
        if !a.is_finite() {
            return Err(SurfaceError::MapNotFinite);
        }
        let origin = a.apply(f.origin());
        let ax = a.apply_vec(f.x().as_vec());
        let ay = a.apply_vec(f.y().as_vec());
        if !origin.is_finite() || !ax.is_finite() || !ay.is_finite() {
            return Err(SurfaceError::MapNotFinite);
        }
        // Through `Frame::transformed` where it has a frame (exact under an exact map, SU7's
        // open question in 63 §6.12); a map that takes z into the plane still has an image.
        let frame = match f.transformed(a) {
            Some((frame, _)) => frame,
            None => Frame::new(origin, ax.cross(ay), ax).ok_or(SurfaceError::MapFlattens)?,
        };
        let (sx, sy) = (stretch(ax, f.x().as_vec()), stretch(ay, f.y().as_vec()));
        let map = if similar(&[ax, ay]) {
            let k = 0.5 * (sx + sy);
            UvMap::scaling(k, k)
        } else {
            // `x'` is `A x / sx`, so `u'` gains `sx u`.
            UvMap {
                m: [
                    [sx, ay.dot(frame.x().as_vec())],
                    [0.0, ay.dot(frame.y().as_vec())],
                ],
                c: [0.0, 0.0],
            }
        };
        let [[p, q], [_, s]] = map.m;
        if !(p.is_finite() && q.is_finite() && s.is_finite()) {
            return Err(SurfaceError::MapNotFinite);
        }
        Ok((Self::new(frame), map))
    }
}

/// A turn direction's reversal for a surface of revolution: the frame turned half a turn
/// about its x axis (y and z negated) and the sense reversed, so `Z` (the frame's z times
/// the sense) is kept and `D(-u) = cos phi X + sin phi Y` on the new frame is the old
/// `D(u)` to the bit (the quarter arcs' sine is odd and cosine even, 63 D13).
fn turned(frame: &Frame, sense: Sense) -> (Frame, Sense) {
    (frame.half_turn_about_x(), sense.reversed())
}

impl Cylinder {
    /// The same cylinder run the other way along `axis`, `S'(-u, v) = S(u, v)` (or in `v`):
    /// along u the frame turned half a turn about its x axis with the sense reversed (the
    /// axis kept), along v the sense reversed. Exact: `S'` at the negated parameter has `S`'s
    /// bits.
    #[must_use]
    pub fn reversed(&self, axis: ParamAxis) -> (Self, UvMap) {
        let (frame, sense) = match axis {
            ParamAxis::U => turned(self.frame(), self.sense()),
            ParamAxis::V => (*self.frame(), self.sense().reversed()),
        };
        (self.placed(frame, sense), UvMap::negating(axis))
    }

    /// The image under a similarity `a`, and its [`UvMap`]: the cylinder on the image frame
    /// ([`Frame::transformed`], the sense times the map's) with the radius scaled by the
    /// map's scale `k`; `u` (arc length) and `v` (height) are scaled by `k`.
    ///
    /// # Errors
    ///
    /// [`SurfaceError::NotSimilar`] for a map that is not a similarity to 16 roundings,
    /// [`SurfaceError::MapNotFinite`], [`SurfaceError::MapFlattens`], and the radius's
    /// cause where the scaled radius overflows.
    pub fn transformed(&self, a: &Affine) -> Result<(Self, UvMap)> {
        let (frame, sense, k) = similarity(self.frame(), a)?;
        let c = Self::new(
            frame,
            length(k * self.radius().mm()),
            compose(self.sense(), sense),
        )?;
        Ok((c, UvMap::scaling(k, k)))
    }
}

impl Cone {
    /// The same cone run the other way along `axis`, `S'(-u, v) = S(u, v)` (or in `v`):
    /// along u as [`Cylinder::reversed`], along v the sense and the half-angle negated, so
    /// the radius `r + k v` at `-v` is the radius at `v` and the apex moves to `-v` with it.
    /// Exact.
    #[must_use]
    pub fn reversed(&self, axis: ParamAxis) -> (Self, UvMap) {
        let cone = match axis {
            ParamAxis::U => {
                let (frame, sense) = turned(self.frame(), self.sense());
                self.placed(frame, sense)
            }
            ParamAxis::V => self.axis_reversed(),
        };
        (cone, UvMap::negating(axis))
    }

    /// The image under a similarity `a`, and its [`UvMap`]: the cone on the image frame
    /// with the radius scaled by the map's scale `k` and the half-angle kept; `v` (height)
    /// is scaled by `k`, `u` (a turn over `[0, 1]`) kept.
    ///
    /// # Errors
    ///
    /// As [`Cylinder::transformed`].
    pub fn transformed(&self, a: &Affine) -> Result<(Self, UvMap)> {
        let (frame, sense, k) = similarity(self.frame(), a)?;
        let r = length(k * self.radius().mm());
        let c = Self::new(frame, r, self.half_angle(), compose(self.sense(), sense))?;
        Ok((c, UvMap::scaling(1.0, k)))
    }
}

impl Sphere {
    /// The same sphere run the other way along `axis`. Along u, `S'(-u, v) = S(u, v)` as
    /// [`Cylinder::reversed`], exact. Along v the latitude's domain is `[0, 1]` from the
    /// south pole (63 D13), and `-v` would leave it: the sense is reversed (the poles swap)
    /// and `S'(u, 1 - v) = S(u, v)`, a reflection about the domain's middle, which rounds
    /// `1 - v` once.
    #[must_use]
    pub fn reversed(&self, axis: ParamAxis) -> (Self, UvMap) {
        match axis {
            ParamAxis::U => {
                let (frame, sense) = turned(self.frame(), self.sense());
                (self.placed(frame, sense), UvMap::negating(axis))
            }
            ParamAxis::V => (
                self.placed(*self.frame(), self.sense().reversed()),
                UvMap {
                    m: [[1.0, 0.0], [0.0, -1.0]],
                    c: [0.0, 1.0],
                },
            ),
        }
    }

    /// The image under a similarity `a`, and its [`UvMap`] (the identity: both directions
    /// are angles over `[0, 1]`): the sphere on the image frame, the radius scaled.
    ///
    /// # Errors
    ///
    /// As [`Cylinder::transformed`].
    pub fn transformed(&self, a: &Affine) -> Result<(Self, UvMap)> {
        let (frame, sense, k) = similarity(self.frame(), a)?;
        let s = Self::new(
            frame,
            length(k * self.radius().mm()),
            compose(self.sense(), sense),
        )?;
        Ok((s, UvMap::IDENTITY))
    }
}

impl Torus {
    /// The same torus run the other way along `axis`, `S'(-u, v) = S(u, v)` (or in `v`):
    /// along u as [`Cylinder::reversed`], along v the sense reversed (the tube's sine is
    /// odd). Exact.
    #[must_use]
    pub fn reversed(&self, axis: ParamAxis) -> (Self, UvMap) {
        let (frame, sense) = match axis {
            ParamAxis::U => turned(self.frame(), self.sense()),
            ParamAxis::V => (*self.frame(), self.sense().reversed()),
        };
        (self.placed(frame, sense), UvMap::negating(axis))
    }

    /// The image under a similarity `a`, and its [`UvMap`] (the identity): the torus on
    /// the image frame, both radii scaled.
    ///
    /// # Errors
    ///
    /// As [`Cylinder::transformed`].
    pub fn transformed(&self, a: &Affine) -> Result<(Self, UvMap)> {
        let (frame, sense, k) = similarity(self.frame(), a)?;
        let t = Self::new(
            frame,
            length(k * self.major().mm()),
            length(k * self.minor().mm()),
            compose(self.sense(), sense),
        )?;
        Ok((t, UvMap::IDENTITY))
    }
}

// ---------------------------------------------------------------- the enum

/// Checks `e` along `axis` against a direction of the domain: inside its ends (a periodic
/// or endless direction has none), and not a single value.
fn check_interval(axis: ParamAxis, e: Extent1d, d: ParamDomain) -> Result<()> {
    let (min, max) = (e.min(), e.max());
    if min == max {
        return Err(SurfaceError::EmptyExtent { axis, at: min });
    }
    let inside = match d {
        ParamDomain::Bounded(b) => b.min() <= min && max <= b.max(),
        ParamDomain::AtLeast { from, .. } => from <= min,
        ParamDomain::AtMost { to, .. } => max <= to,
        _ => true,
    };
    if inside {
        Ok(())
    } else {
        Err(SurfaceError::OutsideDomain { axis, min, max })
    }
}

/// The NURBS surface over `e`, already checked inside its domain: SU3's restriction
/// (split at each end inside the domain), the same `Arc` where `e` is the domain.
fn restrict_nurbs(n: &Arc<NurbsSurface<3>>, e: Extent2d) -> Result<Arc<NurbsSurface<3>>> {
    if e.are_same(&n.domain()) {
        return Ok(Arc::clone(n));
    }
    crate::to_nurbs::restrict(n, &e).map(Arc::new)
}

impl Surface {
    /// The surface over `e`, on the same parameters.
    ///
    /// An analytic kind carries no extent (a face holds its loops), so it comes back
    /// unchanged; a NURBS surface is split at each end of `e` inside its domain (`e` equal to
    /// the domain gives the same `Arc`). Either way `e` is checked first, against the kind's
    /// domain where it ends: refused, not clamped.
    ///
    /// # Errors
    ///
    /// [`SurfaceError::EmptyExtent`] for an `e` that is a single value in a direction,
    /// [`SurfaceError::OutsideDomain`] for one that reaches past an end of the domain (a
    /// sphere's pole, a cone's apex, a NURBS surface's knots), and the split's own cause
    /// ([`SurfaceError::Nurbs`]).
    pub fn restrict(&self, e: Extent2d) -> Result<Self> {
        let d = self.domain();
        check_interval(ParamAxis::U, e.x_interval(), d.u)?;
        check_interval(ParamAxis::V, e.y_interval(), d.v)?;
        Ok(match self {
            Self::Nurbs(n) => Self::Nurbs(restrict_nurbs(n, e)?),
            _ => self.clone(),
        })
    }

    /// The same surface run the other way along `axis`, and the [`UvMap`] from the
    /// original's parameters to the result's: the negation of that parameter
    /// (`S'(-u, v) = S(u, v)`) for every kind, as `scx_curve`'s `Curve::reversed` is
    /// `C(-t)`, except the sphere's latitude, which maps to `1 - v`
    /// ([`Sphere::reversed`]). The normal flips. Total; an analytic kind is exact (the
    /// sphere's latitude to one rounding of `1 - v`), and a NURBS surface's knots are
    /// negated exactly ([`NurbsSurface::reversed`]).
    #[must_use]
    pub fn reversed(&self, axis: ParamAxis) -> (Self, UvMap) {
        match self {
            Self::Plane(k) => wrap(k.reversed(axis)),
            Self::Cylinder(k) => wrap(k.reversed(axis)),
            Self::Cone(k) => wrap(k.reversed(axis)),
            Self::Sphere(k) => wrap(k.reversed(axis)),
            Self::Torus(k) => wrap(k.reversed(axis)),
            Self::Nurbs(n) => (
                Self::Nurbs(Arc::new(n.reversed(axis))),
                UvMap::negating(axis),
            ),
        }
    }

    /// The image under `a`, and the [`UvMap`] from the original's parameters to the
    /// image's: with `(s, m)` the result, `s.point(m.apply(uv))` is `a(self.point(uv))` up
    /// to rounding.
    ///
    /// A plane maps to a plane under any affine map ([`Plane::transformed`]); a cylinder,
    /// cone, sphere or torus to the same kind under a similarity; a NURBS surface to a NURBS
    /// surface, its control points mapped ([`NurbsSurface::transformed`]), with the
    /// identity map. These are exact images.
    ///
    /// # Errors
    ///
    /// [`SurfaceError::NotSimilar`] (`Unsupported`) for a surface of revolution under a map
    /// that is not a similarity: its exact image is a NURBS surface over an extent the kind
    /// does not carry (for now `self.to_nurbs(over)?` and then the NURBS surface's own
    /// transform; 63 §4.9). [`SurfaceError::MapNotFinite`],
    /// [`SurfaceError::MapFlattens`], an image size's cause, and a NURBS surface's own cause
    /// ([`SurfaceError::Nurbs`]).
    pub fn transformed(&self, a: &Affine) -> Result<(Self, UvMap)> {
        match self {
            Self::Plane(k) => k.transformed(a).map(wrap),
            Self::Cylinder(k) => k.transformed(a).map(wrap),
            Self::Cone(k) => k.transformed(a).map(wrap),
            Self::Sphere(k) => k.transformed(a).map(wrap),
            Self::Torus(k) => k.transformed(a).map(wrap),
            Self::Nurbs(n) => Ok((n.transformed(a)?.into(), UvMap::IDENTITY)),
        }
    }
}

fn wrap<K: Into<Surface>>((k, m): (K, UvMap)) -> (Surface, UvMap) {
    (k.into(), m)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::surface::tests::{analytic, nurbs};
    use scx_num::domain::Clamp;
    use scx_num::error::ErrorKind;
    use scx_num::hash::Rng;
    use scx_num::vector3d::Point3;
    use scx_nurbs::Approach;

    const AXES: [ParamAxis; 2] = [ParamAxis::U, ParamAxis::V];
    const ABOVE: [Approach; 2] = [Approach::Above, Approach::Above];

    /// Random parameters inside the working range of each direction (none on a break).
    fn params(rng: &mut Rng, s: &Surface) -> Vec<Point2> {
        let w = s.domain().working(Clamp::Sample, Clamp::Sample);
        (0..24)
            .map(|_| {
                let (u, v) = (rng.next_f64(), rng.next_f64());
                Point2::new(
                    w.x_min() + u * (w.x_max() - w.x_min()),
                    w.y_min() + v * (w.y_max() - w.y_min()),
                )
            })
            .collect()
    }

    fn bits(p: Point3) -> [u64; 3] {
        [p.x.to_bits(), p.y.to_bits(), p.z.to_bits()]
    }

    /// The size a point's rounding is measured against.
    fn scale(p: Point3) -> f64 {
        p.x.abs().max(p.y.abs()).max(p.z.abs()).max(1.0)
    }

    fn near(a: Point3, b: Point3, tol: f64) -> bool {
        (a - b).length() <= tol * scale(b)
    }

    fn surfaces() -> Vec<Surface> {
        let mut rng = Rng::new(0x5e7_0001);
        let mut out = analytic(&mut rng);
        out.extend(nurbs(&mut rng));
        out
    }

    #[test]
    fn reversal_negates_the_parameter_bit_for_bit() {
        let mut rng = Rng::new(0x5e7_0002);
        let mut checked = 0;
        for s in surfaces() {
            for axis in AXES {
                let (r, m) = s.reversed(axis);
                let sphere_v = matches!(s, Surface::Sphere(_)) && axis == ParamAxis::V;
                if sphere_v {
                    assert_eq!(m.matrix(), [[1.0, 0.0], [0.0, -1.0]]);
                    assert_eq!(m.offset(), [0.0, 1.0]);
                } else {
                    assert_eq!(m, UvMap::negating(axis), "{s:?}");
                }
                assert_eq!(r.reversed(axis).0, s, "reversed twice");
                for uv in params(&mut rng, &s) {
                    let back = m.apply(uv);
                    let (got, want) = (r.point(back), s.point(uv));
                    if matches!(s, Surface::Nurbs(_)) {
                        // The basis sums run in the other order.
                        assert!(near(got, want, 1e-12), "{s:?} at {uv:?}");
                    } else if sphere_v {
                        assert!(near(got, want, 8.0 * f64::EPSILON), "{s:?} at {uv:?}");
                    } else {
                        assert_eq!(bits(got), bits(want), "{s:?} along {axis} at {uv:?}");
                    }
                    let side = match axis {
                        ParamAxis::U => [Approach::Below, Approach::Above],
                        ParamAxis::V => [Approach::Above, Approach::Below],
                    };
                    let (nr, ns) = (r.normal(back, side), s.normal(uv, ABOVE));
                    if let (Some(nr), Some(ns)) = (nr, ns) {
                        assert!((*nr + *ns).length() <= 1e-9, "{s:?} normal at {uv:?}");
                    }
                    checked += 1;
                }
            }
        }
        assert!(checked > 10_000, "{checked}");
    }

    /// At an arc's end the reversed surface's jet from one side is the original's from the
    /// other, the odd derivatives along the axis negated.
    #[test]
    fn reversal_swaps_the_side_at_a_break() {
        for s in surfaces()
            .into_iter()
            .filter(|s| !matches!(s, Surface::Nurbs(_)))
        {
            for axis in [ParamAxis::U] {
                let (r, m) = s.reversed(axis);
                let uv = Point2::new(0.25, 0.375);
                let uv = match s {
                    Surface::Cylinder(c) => {
                        Point2::new(0.25 * core::f64::consts::TAU * c.radius().mm(), 0.375)
                    }
                    _ => uv,
                };
                let got = r.jet::<2>(m.apply(uv), [Approach::Above, Approach::Above]);
                let want = s.jet::<2>(uv, [Approach::Below, Approach::Above]);
                let tol = 1e-12 * scale(s.point(uv));
                for (i, j, sign) in [
                    (0, 0, 1.0),
                    (1, 0, -1.0),
                    (0, 1, 1.0),
                    (2, 0, 1.0),
                    (1, 1, -1.0),
                ] {
                    let (g, w) = (got.get(i, j).unwrap(), want.get(i, j).unwrap());
                    assert!(
                        (g - w * sign).length() <= tol * 8.0,
                        "{s:?} d{i}{j}: {g:?} {w:?}"
                    );
                }
            }
        }
    }

    #[test]
    fn reversed_cone_keeps_its_slope_to_the_bit() {
        for s in surfaces() {
            if let Surface::Cone(c) = s {
                let (r, _) = c.reversed(ParamAxis::V);
                assert_eq!(r.half_angle(), -c.half_angle());
                assert_eq!(r.radius(), c.radius());
                assert_eq!(r.pole().v, -c.pole().v);
            }
        }
    }

    /// Rotations, uniform scales, mirrors and their products.
    fn similarities(rng: &mut Rng) -> Vec<Affine> {
        let mut out = Vec::new();
        for i in 0..12 {
            let p = Point3::new(rng.next_signed_unit(), rng.next_signed_unit(), 3.0);
            let axis = Vec3::new(rng.next_signed_unit(), rng.next_signed_unit(), 0.5);
            let rot = Affine::rotation(p, axis, 6.0 * rng.next_signed_unit()).unwrap();
            let k = [1.0, 0.5, 7.25, 1e-3][i % 4];
            let scale = Affine::uniform_scaling(Point3::new(1.0, -2.0, 0.5), k).unwrap();
            let mirror = Affine::reflection(p, Vec3::new(0.3, -1.0, 0.2)).unwrap();
            out.push(rot);
            out.push(scale * rot);
            out.push(mirror * scale * rot);
        }
        out
    }

    fn shear() -> Affine {
        Affine::from_axes(
            Vec3::new(1.0, 0.0, 0.0),
            Vec3::new(0.7, 2.0, 0.0),
            Vec3::new(0.1, -0.3, 0.5),
            Point3::new(1.0, 2.0, 3.0),
        )
    }

    #[test]
    fn transformed_maps_every_point() {
        let mut rng = Rng::new(0x5e7_0003);
        let maps = similarities(&mut rng);
        let mut checked = 0;
        for s in surfaces() {
            let mut all = maps.clone();
            if matches!(s, Surface::Plane(_) | Surface::Nurbs(_)) {
                all.push(shear());
            }
            for a in &all {
                let (t, m) = s.transformed(a).unwrap();
                assert_eq!(
                    core::mem::discriminant(&t),
                    core::mem::discriminant(&s),
                    "the kind is kept"
                );
                for uv in params(&mut rng, &s) {
                    let want = a.apply(s.point(uv));
                    let got = t.point(m.apply(uv));
                    assert!(near(got, want, 1e-12), "{s:?} under {a:?} at {uv:?}");
                    checked += 1;
                }
            }
        }
        assert!(checked > 10_000, "{checked}");
    }

    /// Maps whose matrices are exact (a power-of-two scale, mirrors in the coordinate planes,
    /// a quarter turn about a coordinate axis, all through the origin) and their inverses:
    /// there and back gives every analytic surface and every `UvMap` back to the bit, for
    /// frames on no world axis.
    #[test]
    fn exact_maps_return_every_surface_to_the_bit() {
        let o = Point3::new(0.0, 0.0, 0.0);
        let quarter = Affine::from_axes(Vec3::Y, -Vec3::X, Vec3::Z, o);
        let maps = [
            Affine::uniform_scaling(o, 4.0).unwrap(),
            Affine::uniform_scaling(o, 0.125).unwrap(),
            Affine::reflection(o, Vec3::Y).unwrap(),
            Affine::reflection(o, Vec3::Z).unwrap(),
            quarter,
            Affine::from_axes(Vec3::X, -Vec3::Z, Vec3::Y, o),
            Affine::uniform_scaling(o, 2.0).unwrap() * Affine::reflection(o, Vec3::X).unwrap(),
        ];
        let mut rng = Rng::new(0x5e7_0005);
        let mut checked = 0;
        for s in analytic(&mut rng) {
            for a in &maps {
                let back = a.inverse().unwrap();
                let (t, m) = s.transformed(a).unwrap();
                let (r, n) = t.transformed(&back).unwrap();
                assert_eq!(r, s, "{s:?} under {a:?} and back");
                for uv in params(&mut rng, &s) {
                    assert_eq!(n.apply(m.apply(uv)), uv, "{s:?} uv {uv:?}");
                }
                checked += 1;
            }
        }
        assert!(checked > 1_000, "{checked}");
    }

    #[test]
    fn transformed_uv_maps_against_closed_forms() {
        let f = Frame::new(Point3::new(1.0, 2.0, 3.0), Vec3::Z, Vec3::X).unwrap();
        let k = Affine::uniform_scaling(Point3::new(0.0, 0.0, 0.0), 2.0).unwrap();
        let cyl = Cylinder::new(f, length(1.5), Sense::Forward).unwrap();
        let (c, m) = cyl.transformed(&k).unwrap();
        assert_eq!((c.radius(), m), (length(3.0), UvMap::scaling(2.0, 2.0)));
        assert_eq!(c.frame().origin(), Point3::new(2.0, 4.0, 6.0));
        let cone = Cone::new(f, length(1.5), 0.5, Sense::Forward).unwrap();
        let (c, m) = cone.transformed(&k).unwrap();
        assert_eq!((c.half_angle(), m), (0.5, UvMap::scaling(1.0, 2.0)));
        let sphere = Sphere::new(f, length(1.5), Sense::Forward).unwrap();
        assert_eq!(sphere.transformed(&k).unwrap().1, UvMap::IDENTITY);
        // A mirror keeps the frame right-handed and reverses the sense (63 D14).
        let mirror = Affine::reflection(Point3::new(0.0, 0.0, 0.0), Vec3::Z).unwrap();
        let (c, m) = cyl.transformed(&mirror).unwrap();
        assert_eq!((c.sense(), m), (Sense::Reversed, UvMap::IDENTITY));
        // The plane's triangular map: u' = u + 0.7 v, v' = 2 v.
        let (p, m) = Plane::new(f).transformed(&shear()).unwrap();
        assert_eq!(m.matrix(), [[1.0, 0.7], [0.0, 2.0]]);
        assert_eq!(p.frame().x(), f.x());
        assert_eq!(m.apply(Point2::new(1.0, 1.0)), Point2::new(1.7, 2.0));
    }

    #[test]
    fn transforms_without_an_exact_image_are_refused() {
        let f = Frame::new(Point3::new(1.0, 2.0, 3.0), Vec3::Z, Vec3::X).unwrap();
        let kinds: [Surface; 4] = [
            Cylinder::new(f, length(1.5), Sense::Forward)
                .unwrap()
                .into(),
            Cone::new(f, length(1.5), 0.5, Sense::Forward)
                .unwrap()
                .into(),
            Sphere::new(f, length(1.5), Sense::Forward).unwrap().into(),
            Torus::new(f, length(1.5), length(0.5), Sense::Reversed)
                .unwrap()
                .into(),
        ];
        let near_similar = Affine::from_axes(
            Vec3::new(1.0 + 1e-12, 0.0, 0.0),
            Vec3::Y,
            Vec3::Z,
            Point3::new(0.0, 0.0, 0.0),
        );
        let nan = Affine::translation(Vec3::new(f64::NAN, 0.0, 0.0));
        let flat = Affine::from_axes(Vec3::X, Vec3::X, Vec3::Z, Point3::new(0.0, 0.0, 0.0));
        for s in &kinds {
            for a in [shear(), near_similar] {
                let e = s.transformed(&a).unwrap_err();
                assert_eq!(
                    (e, e.kind()),
                    (SurfaceError::NotSimilar, ErrorKind::Unsupported)
                );
            }
            assert_eq!(s.transformed(&nan).unwrap_err(), SurfaceError::MapNotFinite);
            assert_eq!(s.transformed(&flat).unwrap_err(), SurfaceError::MapFlattens);
        }
        let plane = Surface::from(Plane::new(f));
        assert_eq!(
            plane.transformed(&nan).unwrap_err(),
            SurfaceError::MapNotFinite
        );
        assert_eq!(
            plane.transformed(&flat).unwrap_err(),
            SurfaceError::MapFlattens
        );
        let huge = Affine::uniform_scaling(Point3::new(0.0, 0.0, 0.0), 1e308).unwrap();
        let big = Sphere::new(f, length(1e300), Sense::Forward).unwrap();
        assert_eq!(
            big.transformed(&huge).unwrap_err().kind(),
            ErrorKind::Refused
        );
        // Within 16 roundings a map is a similarity, and the scale is the mean.
        let eps = Affine::from_axes(
            Vec3::new(1.0 + 4.0 * f64::EPSILON, 0.0, 0.0),
            Vec3::Y,
            Vec3::Z,
            Point3::new(0.0, 0.0, 0.0),
        );
        assert!(kinds.iter().all(|s| s.transformed(&eps).is_ok()));
    }

    #[test]
    fn restrict_splits_a_nurbs_surface_and_checks_the_extent() {
        let mut rng = Rng::new(0x5e7_0004);
        let mut checked = 0;
        for s in surfaces() {
            let d = s.domain().working(Clamp::Sample, Clamp::Sample);
            let (u0, u1, v0, v1) = (d.x_min(), d.x_max(), d.y_min(), d.y_max());
            let sub = Extent2d::from_bounds(
                u0 + 0.2 * (u1 - u0),
                v0 + 0.1 * (v1 - v0),
                u0 + 0.7 * (u1 - u0),
                v1,
            )
            .unwrap();
            let r = s.restrict(sub).unwrap();
            match (&s, &r) {
                (Surface::Nurbs(a), Surface::Nurbs(b)) => {
                    assert!(b.domain().are_same(&sub));
                    for uv in params(&mut rng, &r) {
                        assert!(near(r.point(uv), s.point(uv), 1e-12));
                        checked += 1;
                    }
                    let whole = s.restrict(a.domain()).unwrap();
                    let Surface::Nurbs(w) = whole else {
                        unreachable!()
                    };
                    assert!(Arc::ptr_eq(&w, a));
                    let past = Extent2d::from_bounds(u0 - 0.5, v0, u1, v1).unwrap();
                    let e = s.restrict(past).unwrap_err();
                    assert_eq!(e.name(), "extent_outside_domain");
                }
                _ => assert_eq!(r, s),
            }
            let flat = Extent2d::from_bounds(u0, 0.5 * (v0 + v1), u1, 0.5 * (v0 + v1)).unwrap();
            let e = s.restrict(flat).unwrap_err();
            assert!(
                matches!(
                    e,
                    SurfaceError::EmptyExtent {
                        axis: ParamAxis::V,
                        ..
                    }
                ),
                "{e:?}"
            );
            assert_eq!(e.kind(), ErrorKind::Refused);
        }
        assert!(checked > 500, "{checked}");
        // Past a sphere's pole, or a cone's apex: refused, not clamped.
        let f = Frame::new(Point3::new(0.0, 0.0, 0.0), Vec3::Z, Vec3::X).unwrap();
        let sphere = Surface::from(Sphere::new(f, length(1.0), Sense::Forward).unwrap());
        let e = sphere.restrict(Extent2d::from_bounds(0.0, -0.1, 1.0, 0.5).unwrap());
        assert_eq!(
            e.unwrap_err(),
            SurfaceError::OutsideDomain {
                axis: ParamAxis::V,
                min: -0.1,
                max: 0.5
            }
        );
        let cone = Surface::from(Cone::new(f, length(1.0), 0.5, Sense::Forward).unwrap());
        assert!(
            cone.restrict(Extent2d::from_bounds(0.0, -5.0, 1.0, 0.0).unwrap())
                .is_err()
        );
        assert!(
            cone.restrict(Extent2d::from_bounds(0.0, -1.0, 1.0, 9.0).unwrap())
                .is_ok()
        );
    }

    #[test]
    fn uv_map_apply_is_exact_where_it_can_be() {
        let n = UvMap::negating(ParamAxis::U);
        assert_eq!(
            n.apply(Point2::new(0.3, f64::INFINITY)),
            Point2::new(-0.3, f64::INFINITY)
        );
        assert_eq!(
            UvMap::IDENTITY.apply(Point2::new(0.1, -7.0)),
            Point2::new(0.1, -7.0)
        );
        assert_eq!(UvMap::default(), UvMap::IDENTITY);
    }
    /// SU3's `to_nurbs` composes with the reversal: the image of a reversed kind over the
    /// mapped extent is the reversal of the kind's image. Exact (knots, control points,
    /// weights) wherever the reversal negates; the sphere's latitude, `1 - v`, is the NURBS
    /// reversal shifted by 1, to rounding.
    #[test]
    fn to_nurbs_of_a_reversal_is_the_reversal_of_to_nurbs() {
        let mut rng = Rng::new(0x5e7_0009);
        let (mut checked, mut exact, mut total) = (0, 0, 0);
        for s in analytic(&mut rng) {
            for axis in AXES {
                let (r, m) = s.reversed(axis);
                let sphere_v = matches!(s, Surface::Sphere(_)) && axis == ParamAxis::V;
                let p = params(&mut rng, &s);
                let (a, b) = (p[0], p[1]);
                let Some(e) =
                    Extent2d::from_bounds(a.x.min(b.x), a.y.min(b.y), a.x.max(b.x), a.y.max(b.y))
                else {
                    continue;
                };
                let (lo, hi) = (m.apply(e.min()), m.apply(e.max()));
                let mapped = Extent2d::from_bounds(
                    lo.x.min(hi.x),
                    lo.y.min(hi.y),
                    lo.x.max(hi.x),
                    lo.y.max(hi.y),
                )
                .unwrap();
                let got = r.to_nurbs(&mapped).unwrap();
                let want = s.to_nurbs(&e).unwrap().reversed(axis);
                for ax in AXES {
                    let (g, w) = (got.knot_vector(ax).knots(), want.knot_vector(ax).knots());
                    assert_eq!(g.len(), w.len(), "{s:?} {axis} knots {ax}");
                    for (g, w) in g.iter().zip(w) {
                        if sphere_v && ax == ParamAxis::V {
                            // The NURBS reversal negates; the sphere's maps to `1 - v`.
                            assert!((g - (w + 1.0)).abs() <= 2.0 * f64::EPSILON, "{s:?} {g} {w}");
                        } else {
                            assert_eq!(g.to_bits(), w.to_bits(), "{s:?} {axis} knots {ax}");
                        }
                    }
                }
                let (gp, wp) = (got.points().points(), want.points().points());
                assert_eq!(gp.len(), wp.len());
                for (g, w) in gp.iter().zip(wp) {
                    let (g, w) = (Point3::from(*g), Point3::from(*w));
                    total += 1;
                    if bits(g) == bits(w) {
                        exact += 1;
                    } else {
                        assert!(near(g, w, 8.0 * f64::EPSILON), "{s:?} {axis}: {g:?} {w:?}");
                        assert!(sphere_v, "{s:?} along {axis}: {g:?} {w:?} not exact");
                    }
                }
                let gw = got.weights().map(|w| w.points().to_vec());
                let ww = want.weights().map(|w| w.points().to_vec());
                if sphere_v {
                    for (g, w) in gw.unwrap().iter().zip(ww.unwrap().iter()) {
                        assert!((g - w).abs() <= 4.0 * f64::EPSILON * w.abs());
                    }
                } else {
                    assert_eq!(gw, ww, "{s:?} along {axis}: weights");
                }
                for _ in 0..8 {
                    let (t, w) = (rng.next_f64(), rng.next_f64());
                    let uv = [
                        mapped.x_min() + t * (mapped.x_max() - mapped.x_min()),
                        mapped.y_min() + w * (mapped.y_max() - mapped.y_min()),
                    ];
                    let at = Point3::from(got.point(uv, ABOVE).unwrap());
                    let shift = if sphere_v { 1.0 } else { 0.0 };
                    let bt = Point3::from(want.point([uv[0], uv[1] - shift], ABOVE).unwrap());
                    assert!(near(at, bt, 8.0 * f64::EPSILON), "{s:?} {axis} at {uv:?}");
                    assert!(near(at, r.point(Point2::new(uv[0], uv[1])), 1e-14), "{s:?}");
                    checked += 1;
                }
            }
        }
        assert!(exact * 2 > total, "{exact} of {total}");
        assert!(total > 1_000, "{total}");
        assert!(checked > 1_000, "{checked}");
    }
}
