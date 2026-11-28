// Written for solidcorex. Apache-2.0.
//! The volume maps of the bend, twist and unbend deformations (research 77 §5.1, §5.2).
//!
//! Each map is a closed form in a frame's local coordinates `(u, v, w)`: the frame's origin
//! sits on the axis, its x on the mid direction (where the bend leaves points in place up to
//! the turn) and its z along the axis. A point in space is taken into the frame, mapped, and
//! taken back out (the pin's `orient ∘ simple ∘ inv_orient`):
//!
//! | map | `(u, v, w)` goes to | inverse |
//! |---|---|---|
//! | [`VolumeMap::Bend`] | `(u cos(v/d), u sin(v/d), w)` | the unbend, refused on the axis |
//! | [`VolumeMap::Unbend`] | `(r, d atan2(y, x), w)`, `r = sqrt(x^2 + y^2)` | the bend, refused outside `u > 0`, `\|v\| <= pi d` |
//! | [`VolumeMap::Twist`] | `(u, v)` turned by `2 pi w / p` about the axis | the twist of `-p` |
//! | [`VolumeMap::Chain`] | each map in order | each inverse in reverse order |
//!
//! `d` is the bend's neutral distance (positive) and `p` the twist's pitch, one turn per `p`
//! along the axis (signed: negative turns left-handed).
//!
//! [`VolumeMap::map`] runs in any [`Polar`] ring: in `f64` for a point and in [`Ivl`] for an
//! enclosure of a box's image ([`VolumeMap::bound`]). [`VolumeMap::inverse`] is the closed
//! form, never a point solve, and says why when the point has no preimage.
//!
//! [`VolumeMap::image`] gives the image of an analytic surface when a rule on its kind and
//! frame names one (77 §5.2), and otherwise [`Image::NotExact`]: the image is decided by the
//! rule, never by sampling.
//!
//! | map | surface (in the map's frame) | image |
//! |---|---|---|
//! | bend | plane square to the axis | the same plane |
//! | bend | plane `u = u0 > 0` | cylinder of radius `u0` about the axis |
//! | bend | plane `v = v0`, `\|v0\| <= pi d` | the plane through the axis at `v0 / d` (same uv) |
//! | bend | plane with normal `(a, 0, c)`, `a, c != 0` | cone about the axis, apex on it |
//! | bend | cylinder along `y`, centre `u_c > rho` | torus, major `u_c`, minor `rho` |
//! | bend | cylinder along `y`, centre on the axis | sphere of radius `rho` on the axis |
//! | unbend | each inverse of the above, the half-plane or nappe picked by `near` | |
//! | twist | plane square to the axis | the plane turned (same uv) |
//! | twist | cylinder, cone or torus about the axis, sphere centred on it | the same surface |

use std::f64::consts::PI;
use std::fmt;
use std::sync::Arc;

use scx_num::extent3d::Extent3d;
use scx_num::fmath;
use scx_num::frame::{Frame, Sense};
use scx_num::interval::Ivl;
use scx_num::ring::{Ring, wide};
use scx_num::trig::Trig;
use scx_num::units::Length;
use scx_num::vector3d::{Point3, Vec3};

use crate::{Cone, Cylinder, Plane, Sphere, Surface, Torus};

/// How far a direction may lean off an axis, or a position off a line or plane relative to
/// its size, and still be taken as on it: a few roundings of a unit vector (as
/// [`UvMap`](crate::UvMap)'s `SIMILAR`). The image rules decide kind by this alone.
const ALIGNED: f64 = 16.0 * f64::EPSILON;

/// How close to the axis, relative to the point's size, a point may be before the bend's
/// inverse refuses it: below this the angle is rounding.
const ON_AXIS: f64 = 4.0 * f64::EPSILON;

/// The rounding allowed past the bend's angular range `|v| <= pi d` before the unbend's
/// inverse refuses a point as outside the image.
const CUT_MARGIN: f64 = 4.0 * f64::EPSILON;

mod sealed {
    pub trait Sealed {}
    impl Sealed for f64 {}
    impl Sealed for scx_num::interval::Ivl {}
}

/// A ring with the angle of a point of the plane: `f64` and [`Ivl`].
pub trait Polar: Trig + sealed::Sealed {
    /// The angle of `(x, y)` in `[-pi, pi]`: [`fmath::atan2`] for an `f64`; for an interval
    /// box an enclosure of the angles of its points, `[-pi, pi]` when the box meets the cut
    /// (the negative x axis), holds the origin or holds a NaN.
    #[must_use]
    fn atan2(y: Self, x: Self) -> Self;
}

impl Polar for f64 {
    #[inline]
    fn atan2(y: Self, x: Self) -> Self {
        fmath::atan2(y, x)
    }
}

impl Polar for Ivl {
    fn atan2(y: Self, x: Self) -> Self {
        let whole = wide(-PI, PI);
        let ends = [y.lo, y.hi, x.lo, x.hi];
        if ends.iter().any(|e| e.is_nan()) {
            return whole;
        }
        let holds_zero_y = y.lo <= 0.0 && 0.0 <= y.hi;
        // The cut (or the origin, when `x` holds zero too) is in the box.
        if holds_zero_y && x.lo <= 0.0 {
            return whole;
        }
        // A box off the origin and the cut: its angles run between those of two corners.
        let mut lo = f64::INFINITY;
        let mut hi = f64::NEG_INFINITY;
        for (cy, cx) in [(y.lo, x.lo), (y.lo, x.hi), (y.hi, x.lo), (y.hi, x.hi)] {
            let a = fmath::atan2(cy, cx);
            lo = lo.min(a);
            hi = hi.max(a);
        }
        wide(lo, hi)
    }
}

/// Why a volume map cannot be built, or a point has no preimage.
#[derive(Debug, Clone, Copy, PartialEq)]
#[non_exhaustive]
pub enum MapError {
    /// The bend's neutral distance is not positive and finite.
    BadNeutral {
        /// The value given, mm.
        value: f64,
    },
    /// The twist's pitch is zero or not finite.
    BadPitch {
        /// The value given, mm.
        value: f64,
    },
    /// The point is not finite.
    NotFinite,
    /// The point is on the bend's axis, where every angle maps to it: the bend has no single
    /// preimage there.
    OnAxis,
    /// The point is outside the bend's domain `u > 0`, `|v| <= pi d`: no point bends to it.
    OutsideImage,
}

impl fmt::Display for MapError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::BadNeutral { value } => {
                write!(
                    f,
                    "bend neutral distance {value} is not positive and finite"
                )
            }
            Self::BadPitch { value } => write!(f, "twist pitch {value} is zero or not finite"),
            Self::NotFinite => f.write_str("point is not finite"),
            Self::OnAxis => f.write_str("point is on the bend axis"),
            Self::OutsideImage => f.write_str("point is outside the bend's image"),
        }
    }
}

impl std::error::Error for MapError {}

/// A bend about a frame's z axis, its neutral layer at distance `d` from the axis.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Bend {
    frame: Frame,
    neutral: f64,
}

impl Bend {
    /// The bend about `frame`'s z axis whose neutral layer, at `neutral` from the axis along
    /// `frame`'s x, keeps its length.
    ///
    /// # Errors
    ///
    /// [`MapError::BadNeutral`] when `neutral` is not positive and finite.
    pub fn new(frame: Frame, neutral: Length) -> Result<Self, MapError> {
        let d = neutral.mm();
        if !(d.is_finite() && d > 0.0) {
            return Err(MapError::BadNeutral { value: d });
        }
        Ok(Self { frame, neutral: d })
    }

    /// The frame.
    #[must_use]
    pub const fn frame(&self) -> &Frame {
        &self.frame
    }

    /// The neutral distance.
    #[must_use]
    pub const fn neutral(&self) -> Length {
        Length::from_mm(self.neutral)
    }
}

/// A twist about a frame's z axis: one turn per pitch along it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Twist {
    frame: Frame,
    pitch: f64,
}

impl Twist {
    /// The twist about `frame`'s z axis turning once per `pitch` along it, right-handed for a
    /// positive pitch and left-handed for a negative one.
    ///
    /// # Errors
    ///
    /// [`MapError::BadPitch`] when `pitch` is zero or not finite.
    pub fn new(frame: Frame, pitch: Length) -> Result<Self, MapError> {
        let p = pitch.mm();
        if !(p.is_finite() && p != 0.0) {
            return Err(MapError::BadPitch { value: p });
        }
        Ok(Self { frame, pitch: p })
    }

    /// The frame.
    #[must_use]
    pub const fn frame(&self) -> &Frame {
        &self.frame
    }

    /// The pitch, signed.
    #[must_use]
    pub const fn pitch(&self) -> Length {
        Length::from_mm(self.pitch)
    }

    /// The turn per unit length, `2 pi / p`.
    fn rate(&self) -> f64 {
        2.0 * PI / self.pitch
    }
}

/// A volume map: a bend, a twist, an unbend (the bend's inverse) or several in order.
#[derive(Clone, Debug, PartialEq)]
pub enum VolumeMap {
    /// The bend.
    Bend(Bend),
    /// The twist.
    Twist(Twist),
    /// The inverse of the bend.
    Unbend(Bend),
    /// The maps in order: the first applies first.
    Chain(Arc<[VolumeMap]>),
}

/// Whether an exact image keeps the source's parametrisation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Uv {
    /// `image.point(uv)` is the map of `source.point(uv)`, to rounding: pcurves carry over.
    Same,
    /// The image is exact as a set, in its own parametrisation: pcurves are derived again.
    Moved,
}

/// Why [`VolumeMap::image`] gives no exact surface.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum NotExact {
    /// No rule names the image of this kind in this frame: its image is the in-volume kind
    /// or a certified fit (77 §5.2).
    NoRule,
    /// The surface reaches the bend's axis (or the unbend's), where the image is not of the
    /// rule's kind.
    MeetsAxis,
    /// The face lies on the unbend's cut at `pi`, where its image splits in two.
    OnCut,
    /// The surface lies outside the bend's domain `|v| <= pi d`.
    OutsideDomain,
}

/// The image of a surface under a volume map.
#[derive(Clone, Debug, PartialEq)]
pub enum Image {
    /// The image is this surface.
    Exact {
        /// The surface.
        surface: Surface,
        /// Whether it keeps the source's uv.
        uv: Uv,
    },
    /// No exact image by rule.
    NotExact(NotExact),
}

/// `p` in `frame`'s coordinates, in any ring.
fn to_local<T: Ring>(frame: &Frame, p: Point3<T>) -> [T; 3] {
    let o = frame.origin();
    let l = T::lift;
    let d = [p.x - l(o.x), p.y - l(o.y), p.z - l(o.z)];
    let along = |a: Vec3| {
        let [dx, dy, dz] = d;
        dx * l(a.x) + dy * l(a.y) + dz * l(a.z)
    };
    [along(*frame.x()), along(*frame.y()), along(*frame.z())]
}

/// The point at `frame`'s coordinates `c`, in any ring.
fn to_world<T: Ring>(frame: &Frame, c: [T; 3]) -> Point3<T> {
    let o = frame.origin();
    let (x, y, z) = (*frame.x(), *frame.y(), *frame.z());
    let l = T::lift;
    let [a, b, w] = c;
    Point3::new(
        l(o.x) + a * l(x.x) + b * l(y.x) + w * l(z.x),
        l(o.y) + a * l(x.y) + b * l(y.y) + w * l(z.y),
        l(o.z) + a * l(x.z) + b * l(y.z) + w * l(z.z),
    )
}

/// `v` in `frame`'s axes.
fn dir_local(frame: &Frame, v: Vec3) -> [f64; 3] {
    [v.dot(*frame.x()), v.dot(*frame.y()), v.dot(*frame.z())]
}

/// The vector of `frame`'s components `c`.
fn dir_world(frame: &Frame, c: [f64; 3]) -> Vec3 {
    let [a, b, w] = c;
    *frame.x() * a + *frame.y() * b + *frame.z() * w
}

/// `c` turned by the angle of `(sin, cos)` about the local z axis.
fn turned<T: Ring>(c: [T; 3], sin: T, cos: T) -> [T; 3] {
    let [u, v, w] = c;
    [cos * u - sin * v, sin * u + cos * v, w]
}

fn bend_local<T: Polar>(c: [T; 3], d: f64) -> [T; 3] {
    let [u, v, w] = c;
    let (s, co) = (v / T::lift(d)).sin_cos();
    [u * co, u * s, w]
}

fn unbend_local<T: Polar>(c: [T; 3], d: f64) -> [T; 3] {
    let [x, y, w] = c;
    let r = (x.sqr() + y.sqr()).sqrt();
    [r, T::atan2(y, x) * T::lift(d), w]
}

fn twist_local<T: Polar>(c: [T; 3], rate: f64) -> [T; 3] {
    let [u, v, w] = c;
    let (s, co) = (w * T::lift(rate)).sin_cos();
    let [x, y, w] = turned([u, v, w], s, co);
    if u.range_ends().is_none() {
        return [x, y, w];
    }
    // The turn keeps the distance to the axis: no coordinate passes the box's widest.
    let reach = (u.sqr() + v.sqr()).sqrt().ends().1;
    [x.clip(-reach, reach), y.clip(-reach, reach), w]
}

/// The derivative of the local bend at `c` along the local direction `t`.
fn bend_push(c: [f64; 3], t: [f64; 3], d: f64) -> [f64; 3] {
    let [u, v, _] = c;
    let [tu, tv, tw] = t;
    let (s, co) = fmath::sin_cos(v / d);
    let k = u / d;
    [co * tu - k * s * tv, s * tu + k * co * tv, tw]
}

fn unbend_push(c: [f64; 3], t: [f64; 3], d: f64) -> [f64; 3] {
    let [x, y, _] = c;
    let [tx, ty, tw] = t;
    let r2 = x * x + y * y;
    let r = r2.sqrt();
    [(x * tx + y * ty) / r, d * (x * ty - y * tx) / r2, tw]
}

fn twist_push(c: [f64; 3], t: [f64; 3], rate: f64) -> [f64; 3] {
    let [u, v, w] = c;
    let [tu, tv, tw] = t;
    let (s, co) = fmath::sin_cos(w * rate);
    [
        co * tu - s * tv + rate * (-s * u - co * v) * tw,
        s * tu + co * tv + rate * (co * u - s * v) * tw,
        tw,
    ]
}

impl VolumeMap {
    /// The map of `p`, in any [`Polar`] ring: a point in `f64`, an enclosure of a box's image
    /// in [`Ivl`]. Total: the bend maps points outside its domain by the same formula, and
    /// the unbend maps a point on its axis to `u = 0`.
    #[must_use]
    pub fn map<T: Polar>(&self, p: Point3<T>) -> Point3<T> {
        match self {
            Self::Bend(b) => to_world(&b.frame, bend_local(to_local(&b.frame, p), b.neutral)),
            Self::Unbend(b) => to_world(&b.frame, unbend_local(to_local(&b.frame, p), b.neutral)),
            Self::Twist(t) => to_world(&t.frame, twist_local(to_local(&t.frame, p), t.rate())),
            Self::Chain(maps) => maps.iter().fold(p, |q, m| m.map(q)),
        }
    }

    /// A box holding the image of `b`; `None` when the enclosure is not finite.
    #[must_use]
    pub fn bound(&self, b: &Extent3d) -> Option<Extent3d> {
        let p = Point3::new(
            Ivl::new(b.x_min(), b.x_max()),
            Ivl::new(b.y_min(), b.y_max()),
            Ivl::new(b.z_min(), b.z_max()),
        );
        let q = self.map(p);
        Extent3d::from_bounds(q.x.lo, q.y.lo, q.z.lo, q.x.hi, q.y.hi, q.z.hi)
    }

    /// The map that undoes this one: the unbend of a bend and the bend of an unbend, the
    /// twist of the opposite pitch, and a chain's inverses in reverse order.
    #[must_use]
    pub fn inverted(&self) -> Self {
        match self {
            Self::Bend(b) => Self::Unbend(*b),
            Self::Unbend(b) => Self::Bend(*b),
            Self::Twist(t) => Self::Twist(Twist {
                pitch: -t.pitch,
                ..*t
            }),
            Self::Chain(maps) => Self::Chain(maps.iter().rev().map(Self::inverted).collect()),
        }
    }

    /// The point `p` maps from, in closed form.
    ///
    /// # Errors
    ///
    /// - [`MapError::NotFinite`] for a point not finite;
    /// - [`MapError::OnAxis`] under a bend, for a point on its axis;
    /// - [`MapError::OutsideImage`] under an unbend, for a point outside the bend's domain
    ///   (`u <= 0` or `|v| > pi d`), which no point unbends to.
    pub fn inverse(&self, q: Point3) -> Result<Point3, MapError> {
        if !q.is_finite() {
            return Err(MapError::NotFinite);
        }
        match self {
            Self::Bend(b) => {
                let c = to_local(&b.frame, q);
                let [x, y, w] = c;
                let size = x.abs().max(y.abs()).max(w.abs());
                if fmath::hypot(x, y) <= ON_AXIS * size {
                    return Err(MapError::OnAxis);
                }
                Ok(to_world(&b.frame, unbend_local(c, b.neutral)))
            }
            Self::Unbend(b) => {
                let c = to_local(&b.frame, q);
                let [u, v, _] = c;
                let reach = PI * b.neutral;
                if u <= 0.0 || v.abs() > reach + CUT_MARGIN * reach {
                    return Err(MapError::OutsideImage);
                }
                Ok(to_world(&b.frame, bend_local(c, b.neutral)))
            }
            Self::Twist(t) => Ok(to_world(
                &t.frame,
                twist_local(to_local(&t.frame, q), -t.rate()),
            )),
            Self::Chain(maps) => maps.iter().rev().try_fold(q, |p, m| m.inverse(p)),
        }
    }

    /// The derivative of the map at `p` along `t`: the Jacobian applied to `t`, in closed
    /// form (chained through a [`VolumeMap::Chain`]). NaN under an unbend on its axis.
    #[must_use]
    pub fn push(&self, p: Point3, t: Vec3) -> Vec3 {
        let local = |frame: &Frame, f: &dyn Fn([f64; 3], [f64; 3]) -> [f64; 3]| {
            dir_world(frame, f(to_local(frame, p), dir_local(frame, t)))
        };
        match self {
            Self::Bend(b) => local(&b.frame, &|c, d| bend_push(c, d, b.neutral)),
            Self::Unbend(b) => local(&b.frame, &|c, d| unbend_push(c, d, b.neutral)),
            Self::Twist(w) => local(&w.frame, &|c, d| twist_push(c, d, w.rate())),
            Self::Chain(maps) => {
                let mut at = p;
                let mut along = t;
                for m in maps.iter() {
                    along = m.push(at, along);
                    at = m.map(at);
                }
                along
            }
        }
    }

    /// The Jacobian at `p`: its columns are the derivatives along the world x, y and z.
    #[must_use]
    pub fn jacobian(&self, p: Point3) -> [Vec3; 3] {
        [
            self.push(p, Vec3::new(1.0, 0.0, 0.0)),
            self.push(p, Vec3::new(0.0, 1.0, 0.0)),
            self.push(p, Vec3::new(0.0, 0.0, 1.0)),
        ]
    }

    /// The image of `s` when a rule on its kind and frame names one (the module's table);
    /// [`Image::NotExact`] otherwise. `near` is a point of the face on `s`: the unbend takes
    /// the half-plane or cone nappe it lies on, since the other maps elsewhere, and a chain
    /// carries it through each step.
    #[must_use]
    pub fn image(&self, s: &Surface, near: Point3) -> Image {
        match self {
            Self::Bend(b) => bend_image(b, s),
            Self::Unbend(b) => unbend_image(b, s, near),
            Self::Twist(t) => twist_image(t, s),
            Self::Chain(maps) => {
                let mut surface = s.clone();
                let mut at = near;
                let mut uv = Uv::Same;
                for m in maps.iter() {
                    match m.image(&surface, at) {
                        Image::Exact {
                            surface: next,
                            uv: step,
                        } => {
                            surface = next;
                            if step == Uv::Moved {
                                uv = Uv::Moved;
                            }
                        }
                        not @ Image::NotExact(_) => return not,
                    }
                    at = m.map(at);
                }
                Image::Exact { surface, uv }
            }
        }
    }
}

/// `|x|` is rounding beside `scale`.
fn small(x: f64, scale: f64) -> bool {
    x.abs() <= ALIGNED * scale
}

/// The unit direction `n` (local) lies along the local axis `k` (0, 1 or 2), either way.
fn along_axis(n: [f64; 3], k: usize) -> bool {
    let [a, b, c] = n;
    match k {
        0 => small(b, 1.0) && small(c, 1.0),
        1 => small(a, 1.0) && small(c, 1.0),
        _ => small(a, 1.0) && small(b, 1.0),
    }
}

/// `+1` or `-1` by the sign of `x`.
fn sign(x: f64) -> f64 {
    1.0_f64.copysign(x)
}

fn norm(c: [f64; 3]) -> f64 {
    let [a, b, w] = c;
    fmath::hypot(fmath::hypot(a, b), w)
}

const fn sense_of(outward: bool) -> Sense {
    if outward {
        Sense::Forward
    } else {
        Sense::Reversed
    }
}

fn exact(surface: Option<Surface>, uv: Uv) -> Image {
    // `None` is a frame or size the rule built out of rounding: no exact image.
    surface.map_or(Image::NotExact(NotExact::NoRule), |surface| Image::Exact {
        surface,
        uv,
    })
}

/// The frame of `axes` (local origin, local z, local x hint) in space.
fn frame_at(map: &Frame, origin: [f64; 3], z: [f64; 3], x: [f64; 3]) -> Option<Frame> {
    Frame::new(to_world(map, origin), dir_world(map, z), dir_world(map, x))
}

/// The plane through `plane`'s points carried by `f`, an affine map of local coordinates
/// that is a rotation plus a shift: same uv.
fn moved_plane(map: &Frame, plane: &Plane, f: impl Fn([f64; 3], bool) -> [f64; 3]) -> Image {
    let pf = plane.frame();
    let o = f(to_local(map, pf.origin()), true);
    let x = f(dir_local(map, *pf.x()), false);
    let z = f(dir_local(map, *pf.z()), false);
    exact(
        Frame::new(to_world(map, o), dir_world(map, z), dir_world(map, x))
            .map(|fr| Surface::Plane(Plane::new(fr))),
        Uv::Same,
    )
}

/// A kind's local origin and z axis, and the scale its positions are judged at.
fn placed(map: &Frame, frame: &Frame, size: f64) -> ([f64; 3], [f64; 3], f64) {
    let o = to_local(map, frame.origin());
    let z = dir_local(map, *frame.z());
    (o, z, 1.0 + norm(o) + size)
}

/// The kind is about the map's axis: its axis along it and its origin on it.
fn coaxial(map: &Frame, frame: &Frame, size: f64) -> bool {
    let (o, z, scale) = placed(map, frame, size);
    let [ox, oy, _] = o;
    along_axis(z, 2) && small(ox, scale) && small(oy, scale)
}

fn centred_on_axis(map: &Frame, sphere: &Sphere) -> bool {
    let (o, _, scale) = placed(map, sphere.frame(), sphere.radius().mm());
    let [ox, oy, _] = o;
    small(ox, scale) && small(oy, scale)
}

fn bend_image(b: &Bend, s: &Surface) -> Image {
    let (m, d) = (&b.frame, b.neutral);
    match s {
        Surface::Plane(p) => {
            let (o, n, scale) = placed(m, p.frame(), 0.0);
            let [n0, n1, n2] = n;
            let [o0, o1, o2] = o;
            if along_axis(n, 2) {
                return exact(Some(s.clone()), Uv::Moved);
            }
            if along_axis(n, 0) {
                if o0 <= ALIGNED * scale {
                    return Image::NotExact(NotExact::MeetsAxis);
                }
                let frame = frame_at(m, [0.0; 3], [0.0, 0.0, 1.0], [1.0, 0.0, 0.0]);
                let sense = sense_of(n0 > 0.0);
                return exact(
                    frame.and_then(|f| {
                        Cylinder::new(f, Length::from_mm(o0), sense)
                            .ok()
                            .map(Surface::Cylinder)
                    }),
                    Uv::Moved,
                );
            }
            if along_axis(n, 1) {
                if o1.abs() > PI * d {
                    return Image::NotExact(NotExact::OutsideDomain);
                }
                let (sin, cos) = fmath::sin_cos(o1 / d);
                // A point (a, v0, c) bends to the turn of (a, 0, c).
                return moved_plane(m, p, |c, is_point| {
                    let [a, bb, w] = c;
                    let c = if is_point { [a, bb - o1, w] } else { c };
                    turned(c, sin, cos)
                });
            }
            if small(n1, 1.0) {
                // n0 u + n2 w = k, a line in each half-plane: a cone with its apex on the
                // axis, opening along the half where u grows.
                let k = n0 * o0 + n1 * o1 + n2 * o2;
                let apex = [0.0, 0.0, k / n2];
                let rises = -n0 / n2 > 0.0;
                let half_angle = fmath::atan((n2 / n0).abs());
                let outward = n0 > 0.0;
                let up = if rises { 1.0 } else { -1.0 };
                let fz = up * sense_of(outward).sign();
                let frame = frame_at(m, apex, [0.0, 0.0, fz], [1.0, 0.0, 0.0]);
                return exact(
                    frame.and_then(|f| {
                        Cone::new(f, Length::ZERO, half_angle, sense_of(outward))
                            .ok()
                            .map(Surface::Cone)
                    }),
                    Uv::Moved,
                );
            }
            Image::NotExact(NotExact::NoRule)
        }
        Surface::Cylinder(c) => {
            let rho = c.radius().mm();
            let (o, z, scale) = placed(m, c.frame(), rho);
            let [o0, _, o2] = o;
            if !along_axis(z, 1) {
                return Image::NotExact(NotExact::NoRule);
            }
            let frame = frame_at(m, [0.0, 0.0, o2], [0.0, 0.0, 1.0], [1.0, 0.0, 0.0]);
            if small(o0, scale) {
                return exact(
                    frame.and_then(|f| {
                        Sphere::new(f, c.radius(), c.sense())
                            .ok()
                            .map(Surface::Sphere)
                    }),
                    Uv::Moved,
                );
            }
            if o0 <= rho + ALIGNED * scale {
                return Image::NotExact(NotExact::MeetsAxis);
            }
            exact(
                frame.and_then(|f| {
                    Torus::new(f, Length::from_mm(o0), c.radius(), c.sense())
                        .ok()
                        .map(Surface::Torus)
                }),
                Uv::Moved,
            )
        }
        _ => Image::NotExact(NotExact::NoRule),
    }
}

fn unbend_image(b: &Bend, s: &Surface, near: Point3) -> Image {
    let (m, d) = (&b.frame, b.neutral);
    let y_hint = [0.0, 1.0, 0.0];
    let x_hint = [1.0, 0.0, 0.0];
    let at = to_local(m, near);
    let [nx, ny, nz] = at;
    match s {
        Surface::Plane(p) => {
            let (o, n, scale) = placed(m, p.frame(), 0.0);
            let [n0, n1, n2] = n;
            if along_axis(n, 2) {
                return exact(Some(s.clone()), Uv::Moved);
            }
            let k = n0 * o[0] + n1 * o[1] + n2 * o[2];
            if !(small(n2, 1.0) && small(k, scale)) {
                return Image::NotExact(NotExact::NoRule);
            }
            // A plane holding the axis: the half the face is on unbends to v = d theta0.
            let (e0, e1) = (-n1, n0);
            let side = e0 * nx + e1 * ny;
            if small(side, 1.0 + norm(at)) {
                return Image::NotExact(NotExact::MeetsAxis);
            }
            let theta = fmath::atan2(sign(side) * e1, sign(side) * e0);
            if PI - theta.abs() <= ALIGNED * PI {
                return Image::NotExact(NotExact::OnCut);
            }
            let (sin, cos) = fmath::sin_cos(-theta);
            let shift = d * theta;
            moved_plane(m, p, |c, is_point| {
                let [a, bb, w] = turned(c, sin, cos);
                if is_point {
                    [a, bb + shift, w]
                } else {
                    [a, bb, w]
                }
            })
        }
        Surface::Cylinder(c) if coaxial(m, c.frame(), c.radius().mm()) => {
            let rho = c.radius().mm();
            let out = c.sense().sign();
            exact(
                frame_at(m, [rho, 0.0, 0.0], [out, 0.0, 0.0], y_hint)
                    .map(|f| Surface::Plane(Plane::new(f))),
                Uv::Moved,
            )
        }
        Surface::Cone(c) if coaxial(m, c.frame(), c.radius().mm()) => {
            let (o, z, _) = placed(m, c.frame(), 0.0);
            let z_o = o[2];
            let axis = sign(z[2]) * c.sense().sign();
            let (r0, k) = (c.radius().mm(), c.slope());
            let radius = r0 + k * (nz - z_o) * axis;
            if small(radius, 1.0 + norm(at)) {
                return Image::NotExact(NotExact::MeetsAxis);
            }
            let nappe = sign(radius);
            // A cone's normal is scaled by the radius's sign, so it faces away from the axis
            // on both nappes when Forward and toward it when Reversed.
            let out = c.sense().sign();
            // On the nappe, u = nappe (r0 + k axis (w - z_o)).
            let normal = [out, 0.0, -out * nappe * k * axis];
            exact(
                frame_at(m, [nappe * r0, 0.0, z_o], normal, y_hint)
                    .map(|f| Surface::Plane(Plane::new(f))),
                Uv::Moved,
            )
        }
        Surface::Torus(t) if coaxial(m, t.frame(), t.major().mm()) => {
            let z_t = to_local(m, t.frame().origin())[2];
            exact(
                frame_at(m, [t.major().mm(), 0.0, z_t], y_hint, x_hint).and_then(|f| {
                    Cylinder::new(f, t.minor(), t.sense())
                        .ok()
                        .map(Surface::Cylinder)
                }),
                Uv::Moved,
            )
        }
        Surface::Sphere(sp) if centred_on_axis(m, sp) => {
            let z_s = to_local(m, sp.frame().origin())[2];
            exact(
                frame_at(m, [0.0, 0.0, z_s], y_hint, x_hint).and_then(|f| {
                    Cylinder::new(f, sp.radius(), sp.sense())
                        .ok()
                        .map(Surface::Cylinder)
                }),
                Uv::Moved,
            )
        }
        _ => Image::NotExact(NotExact::NoRule),
    }
}

fn twist_image(t: &Twist, s: &Surface) -> Image {
    let m = &t.frame;
    match s {
        Surface::Plane(p) => {
            let (o, n, _) = placed(m, p.frame(), 0.0);
            if !along_axis(n, 2) {
                return Image::NotExact(NotExact::NoRule);
            }
            let (sin, cos) = fmath::sin_cos(o[2] * t.rate());
            moved_plane(m, p, |c, _| turned(c, sin, cos))
        }
        Surface::Cylinder(c) if coaxial(m, c.frame(), c.radius().mm()) => {
            exact(Some(s.clone()), Uv::Moved)
        }
        Surface::Cone(c) if coaxial(m, c.frame(), c.radius().mm()) => {
            exact(Some(s.clone()), Uv::Moved)
        }
        Surface::Torus(c) if coaxial(m, c.frame(), c.major().mm()) => {
            exact(Some(s.clone()), Uv::Moved)
        }
        Surface::Sphere(sp) if centred_on_axis(m, sp) => exact(Some(s.clone()), Uv::Moved),
        _ => Image::NotExact(NotExact::NoRule),
    }
}

#[cfg(test)]
mod tests;
