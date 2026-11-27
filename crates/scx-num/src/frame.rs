// Written for solidcorex. Apache-2.0.
//! A placement: an origin and a right-handed orthonormal basis.
//!
//! Every placed analytic kind (plane, cylinder, cone, sphere, torus, circle, ellipse) is a
//! [`Frame`] plus its sizes. The frame stores its origin, its x axis and its z axis (the kind's
//! axis); y is derived as `z × x`, so the basis is right-handed by construction and no
//! left-handed frame can be stored.
//!
//! | Rust                 | Upstream                                                    |
//! |----------------------|-------------------------------------------------------------|
//! | [`Frame`]            | `SmAxis2Placement3d` (location, axis, ref direction)         |
//! | [`Frame::new`]       | its constructor from a location, axis and ref direction     |
//! | [`Frame::from_axes`] | none: the stored axes taken back as they are (a record)     |
//! | [`Frame::to_affine`] | `GetTransform` (local to world)                              |
//! | [`Frame::transformed`] | `Transform`, with the handedness returned as a [`Sense`] |
//! | [`Frame2`]           | `SmAxis2Placement2d` (location, ref direction)              |
//!
//! Deviations:
//! - The axes are unit and orthogonal by construction: [`Frame::new`] refuses an axis or a
//!   reference direction with no direction, or a reference direction along the axis, where
//!   upstream stores what it is given.
//! - A map that reverses orientation (a mirror) gives a right-handed frame and
//!   [`Sense::Reversed`] (research 63 D14): the images of x and y are kept, so a kind's
//!   `(u, v)` map is unchanged, and the image of z is the reverse of the stored z. The kind
//!   carries the sense and evaluates with the axis `sense · z`.

use core::fmt;

use crate::error::ErrorKind;
use crate::math::Affine;
use crate::vector2d::{Point2, UnitVec2, Vec2};
use crate::vector3d::{Point3, UnitVec3, Vec3};

/// Whether a placed kind runs with its frame or against it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Sense {
    /// Along the frame's z axis.
    #[default]
    Forward,
    /// Against the frame's z axis: the image of a forward kind under a mirror.
    Reversed,
}

impl Sense {
    /// The other sense.
    #[must_use]
    pub const fn reversed(self) -> Self {
        match self {
            Self::Forward => Self::Reversed,
            Self::Reversed => Self::Forward,
        }
    }

    /// `1.0` for [`Sense::Forward`], `-1.0` for [`Sense::Reversed`].
    #[must_use]
    pub const fn sign(self) -> f64 {
        match self {
            Self::Forward => 1.0,
            Self::Reversed => -1.0,
        }
    }
}

/// How far from unit length, and from square to each other, [`Frame::from_axes`] and
/// [`Frame2::from_axis`] take an axis to be: `|a · a - 1|` and `|x · z|` at most this.
///
/// Eight ulps of 1. The axes [`Frame::new`] and [`Frame2::new`] build are within 2.5 ulps of
/// unit and 1 ulp of square (measured over 200 000 random frames at scales `1e-10` to `1e9`,
/// half of them mapped by random affine maps); [`Frame::transformed`] keeps a scaled image as
/// it is only within this same tolerance (`IMAGE_SQUARE`). So every frame they build passes;
/// an axis off by more than rounding does not.
pub const AXIS_TOLERANCE: f64 = 8.0 * f64::EPSILON;

/// Why axes given as they are make no frame ([`Frame::from_axes`], [`Frame2::from_axis`]).
/// Each is [`ErrorKind::Refused`]: the axes are the caller's.
#[derive(Debug, Clone, Copy, PartialEq)]
#[non_exhaustive]
pub enum FrameError {
    /// A coordinate of the origin or an axis is infinite or NaN.
    NotFinite,
    /// An axis is not unit: `|a · a - 1|` is over [`AXIS_TOLERANCE`].
    NotUnit {
        /// Which axis.
        axis: FrameAxis,
        /// Its squared length, `a · a`.
        length_squared: f64,
    },
    /// The axes are not square: `|x · z|` is over [`AXIS_TOLERANCE`].
    NotSquare {
        /// `x · z`.
        dot: f64,
    },
}

/// A stored axis of a frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FrameAxis {
    /// The x axis.
    X,
    /// The z axis.
    Z,
}

impl FrameError {
    /// What the caller does next: [`ErrorKind::Refused`].
    #[must_use]
    pub const fn kind(&self) -> ErrorKind {
        ErrorKind::Refused
    }

    /// The cause's stable name.
    #[must_use]
    pub const fn name(&self) -> &'static str {
        match self {
            Self::NotFinite => "frame_not_finite",
            Self::NotUnit { .. } => "frame_not_unit",
            Self::NotSquare { .. } => "frame_not_square",
        }
    }
}

impl fmt::Display for FrameError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotFinite => f.write_str("a frame's origin or axis is not finite"),
            Self::NotUnit {
                axis,
                length_squared,
            } => {
                let axis = match axis {
                    FrameAxis::X => "x",
                    FrameAxis::Z => "z",
                };
                write!(
                    f,
                    "a frame's {axis} axis is not unit (squared length {length_squared})"
                )
            }
            Self::NotSquare { dot } => write!(f, "a frame's axes are not square (x · z = {dot})"),
        }
    }
}

impl std::error::Error for FrameError {}

/// `a · a` within [`AXIS_TOLERANCE`] of 1.
fn unit(axis: FrameAxis, length_squared: f64) -> Result<(), FrameError> {
    if (length_squared - 1.0).abs() <= AXIS_TOLERANCE {
        Ok(())
    } else {
        Err(FrameError::NotUnit {
            axis,
            length_squared,
        })
    }
}

/// An origin and a right-handed orthonormal basis: x and z stored, `y = z × x`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Frame {
    origin: Point3,
    x: UnitVec3,
    z: UnitVec3,
}

impl Frame {
    /// The world frame: the origin and the X, Y and Z axes.
    pub const WORLD: Self = Self {
        origin: Point3::new(0.0, 0.0, 0.0),
        x: UnitVec3::X,
        z: UnitVec3::Z,
    };

    /// The frame at `origin` with axis `z`, its x axis the part of `x_hint` square to `z`.
    /// `None` when `origin` is not finite, when `z` or `x_hint` has no direction
    /// ([`Vec3::normalized`]), or when `x_hint` lies along `z` (its part square to `z` has no
    /// direction).
    #[must_use]
    pub fn new(origin: Point3, z: Vec3, x_hint: Vec3) -> Option<Self> {
        if !(origin.x.is_finite() && origin.y.is_finite() && origin.z.is_finite()) {
            return None;
        }
        // `MakeUnitOrthoVectors(z, x_hint)` without its arbitrary pick: y = z × x_hint made
        // unit, then x = y × z.
        let [z, x, _] = z.unit_ortho_toward(x_hint)?;
        Some(Self { origin, x, z })
    }

    /// The frame at `origin` with the axes `x` and `z` exactly as given, bit for bit: the
    /// inverse of [`Frame::origin`], [`Frame::x`] and [`Frame::z`], for a record. Nothing is
    /// normalized ([`Frame::new`] rounds its axes again, so it does not give a frame back to
    /// the bit); the axes are checked instead.
    ///
    /// # Errors
    ///
    /// [`FrameError::NotFinite`] for a coordinate that is not finite;
    /// [`FrameError::NotUnit`] for an axis not unit and [`FrameError::NotSquare`] for axes
    /// not square, each within [`AXIS_TOLERANCE`].
    pub fn from_axes(origin: Point3, x: Vec3, z: Vec3) -> Result<Self, FrameError> {
        if !(origin.is_finite() && x.is_finite() && z.is_finite()) {
            return Err(FrameError::NotFinite);
        }
        // The same sums as `transformed`'s exact path, so every frame it keeps passes.
        unit(FrameAxis::X, x.length_squared())?;
        unit(FrameAxis::Z, z.length_squared())?;
        let dot = x.dot(z);
        if dot.abs() > AXIS_TOLERANCE {
            return Err(FrameError::NotSquare { dot });
        }
        Ok(Self {
            origin,
            x: UnitVec3::kept(x),
            z: UnitVec3::kept(z),
        })
    }

    /// The origin.
    #[must_use]
    pub const fn origin(&self) -> Point3 {
        self.origin
    }

    /// The x axis: where a kind's angle parameter starts.
    #[must_use]
    pub const fn x(&self) -> UnitVec3 {
        self.x
    }

    /// The y axis, `z × x`.
    #[must_use]
    pub fn y(&self) -> UnitVec3 {
        self.z.cross_square(self.x)
    }

    /// The z axis: a kind's axis or normal.
    #[must_use]
    pub const fn z(&self) -> UnitVec3 {
        self.z
    }

    /// The frame turned half a turn about its x axis: the origin and x kept, y and z
    /// negated. Exact: no axis is rounded, and [`Frame::y`] of the result is the negated y
    /// to the bit.
    #[must_use]
    pub fn half_turn_about_x(&self) -> Self {
        Self {
            origin: self.origin,
            x: self.x,
            z: -self.z,
        }
    }

    /// The frame turned half a turn about its y axis: the origin and y kept, x and z
    /// negated. Exact, as [`Frame::half_turn_about_x`].
    #[must_use]
    pub fn half_turn_about_y(&self) -> Self {
        Self {
            origin: self.origin,
            x: -self.x,
            z: -self.z,
        }
    }

    /// The map from frame coordinates to world coordinates: the X, Y and Z axes to x, y and
    /// z, the origin to [`Frame::origin`].
    #[must_use]
    pub fn to_affine(&self) -> Affine {
        Affine::from_axes(*self.x, *self.y(), *self.z, self.origin)
    }

    /// The frame `a` maps this one to, and whether `a` reverses orientation.
    ///
    /// The image's origin is the image of the origin, its x axis the direction of the image
    /// of x, and its xy plane the image of the xy plane; its z is that plane's right-handed
    /// normal. Where `a` reverses orientation (its linear part has a negative determinant,
    /// as a mirror's has) the image of z points against that normal, and the sense is
    /// [`Sense::Reversed`]. For a rigid map the image frame's axes are the images of the
    /// axes, z reversed under a mirror; a map that is not a similarity changes the angles in
    /// the plane, which the kind accounts for (research 63, 3.2).
    ///
    /// `None` when the image is not finite or `a` flattens the frame (its linear part is
    /// singular on the frame's axes).
    ///
    /// Where the map keeps the axes square (a similarity, or any map whose image of z is
    /// square to the image plane), the axes are the images scaled back by their own stretch,
    /// `x' = A x · |x| / |A x|` and `z' = ±A z · |z| / |A z|`, kept as they are when they are
    /// unit and square to 8 roundings. Under a map whose matrix is exact (a
    /// scale by a power of two, a mirror in a coordinate plane, a quarter turn about a
    /// coordinate axis) these are exact, so the map and its inverse give the frame back to the
    /// bit. Otherwise z is made again from `A x × A y` and normalised.
    #[must_use]
    pub fn transformed(&self, a: &Affine) -> Option<(Self, Sense)> {
        let ax = a.apply_vec(*self.x);
        let ay = a.apply_vec(*self.y());
        let az = a.apply_vec(*self.z);
        let normal = ax.cross(ay);
        let origin = a.apply(self.origin);
        let side = az.dot(normal);
        let sense = if side > 0.0 {
            Sense::Forward
        } else if side < 0.0 {
            Sense::Reversed
        } else {
            // z maps into the image plane (or the image is not finite).
            return None;
        };
        if let Some(frame) = self.square_image(origin, ax, ay, az, sense) {
            return Some((frame, sense));
        }
        Some((Self::new(origin, normal, ax)?, sense))
    }

    /// [`Frame::transformed`]'s exact path: the images of x and z scaled back by their own
    /// stretch, z's sign the sense's. `None` where the image is not finite, or where the
    /// result is not unit and square to [`IMAGE_SQUARE`] roundings or `A y` leaves its plane
    /// by more (the map is not square on the axes), and the caller normalises instead.
    fn square_image(
        &self,
        origin: Point3,
        ax: Vec3,
        ay: Vec3,
        az: Vec3,
        sense: Sense,
    ) -> Option<Self> {
        // Lengths summed in any order, so a signed permutation's stretch is exact too.
        let x = ax * (self.x.length_any_order() / ax.length_any_order());
        let z = az * (sense.sign() * self.z.length_any_order() / az.length_any_order());
        let tol = IMAGE_SQUARE;
        let fits = origin.is_finite()
            && x.is_finite()
            && z.is_finite()
            && (x.length_squared() - 1.0).abs() <= tol
            && (z.length_squared() - 1.0).abs() <= tol
            && x.dot(z).abs() <= tol
            && ay.dot(z).abs() <= tol * ay.length();
        fits.then_some(Self {
            origin,
            x: UnitVec3::kept(x),
            z: UnitVec3::kept(z),
        })
    }
}

/// The roundings within which [`Frame::transformed`] keeps the scaled images of the axes as
/// they are: each unit, and square to the other and to the image of y.
const IMAGE_SQUARE: f64 = AXIS_TOLERANCE;

/// A placement in the plane: an origin and a unit x axis, `y` its quarter turn
/// counter-clockwise, so the basis is right-handed (counter-clockwise) by construction. A
/// placed 2-D kind (circle, ellipse) runs clockwise by carrying [`Sense::Reversed`].
///
/// Upstream: `SmAxis2Placement2d` (location, ref direction).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Frame2 {
    origin: Point2,
    x: UnitVec2,
}

impl Frame2 {
    /// The world frame of the plane: the origin and the X and Y axes.
    pub const WORLD: Self = Self {
        origin: Point2::ORIGIN,
        x: UnitVec2::X,
    };

    /// The frame at `origin` with x axis the direction of `x`. `None` when `origin` is not
    /// finite or `x` has no direction ([`Vec2::normalized`]).
    #[must_use]
    pub fn new(origin: Point2, x: Vec2) -> Option<Self> {
        if !(origin.x.is_finite() && origin.y.is_finite()) {
            return None;
        }
        Some(Self {
            origin,
            x: x.normalized()?,
        })
    }

    /// The frame at `origin` with the axis `x` exactly as given, bit for bit, as
    /// [`Frame::from_axes`].
    ///
    /// # Errors
    ///
    /// [`FrameError::NotFinite`] for a coordinate that is not finite;
    /// [`FrameError::NotUnit`] for an axis not unit within [`AXIS_TOLERANCE`].
    pub fn from_axis(origin: Point2, x: Vec2) -> Result<Self, FrameError> {
        if ![origin.x, origin.y, x.x, x.y].iter().all(|c| c.is_finite()) {
            return Err(FrameError::NotFinite);
        }
        unit(FrameAxis::X, x.dot(x))?;
        Ok(Self {
            origin,
            x: UnitVec2::kept(x),
        })
    }

    /// The same axes at `origin`, x kept to the bit. `None` when `origin` is not finite.
    #[must_use]
    pub fn moved_to(&self, origin: Point2) -> Option<Self> {
        (origin.x.is_finite() && origin.y.is_finite()).then_some(Self { origin, x: self.x })
    }

    /// The origin.
    #[must_use]
    pub const fn origin(&self) -> Point2 {
        self.origin
    }

    /// The x axis: where a kind's angle parameter starts.
    #[must_use]
    pub const fn x(&self) -> UnitVec2 {
        self.x
    }

    /// The y axis, x turned a quarter counter-clockwise.
    #[must_use]
    pub const fn y(&self) -> UnitVec2 {
        self.x.perp()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hash::Rng;

    const TIGHT: f64 = 8.0 * f64::EPSILON;

    fn v(r: &mut Rng, s: f64) -> Vec3 {
        Vec3::new(
            r.next_signed_unit() * s,
            r.next_signed_unit() * s,
            r.next_signed_unit() * s,
        )
    }
    fn p(r: &mut Rng, s: f64) -> Point3 {
        let a = v(r, s);
        Point3::new(a.x, a.y, a.z)
    }
    fn near(a: Vec3, b: Vec3, tol: f64) -> bool {
        (a - b).length() <= tol
    }
    fn near_p(a: Point3, b: Point3, tol: f64) -> bool {
        near(Vec3::new(a.x - b.x, a.y - b.y, a.z - b.z), Vec3::ZERO, tol)
    }
    /// Unit, square and right-handed axes.
    fn orthonormal(f: &Frame) -> bool {
        let (x, y, z) = (*f.x(), *f.y(), *f.z());
        [x, y, z].iter().all(|a| (a.length() - 1.0).abs() <= TIGHT)
            && x.dot(y).abs() <= TIGHT
            && y.dot(z).abs() <= TIGHT
            && z.dot(x).abs() <= TIGHT
            && near(x.cross(y), z, 4.0 * TIGHT)
    }
    fn random_frame(r: &mut Rng) -> Frame {
        loop {
            if let Some(f) = Frame::new(p(r, 100.0), v(r, 1.0), v(r, 1.0)) {
                return f;
            }
        }
    }

    #[test]
    fn world_frame_is_the_identity_map() {
        let f = Frame::WORLD;
        assert_eq!(*f.y(), Vec3::Y);
        assert_eq!(f.to_affine(), Affine::IDENTITY);
        assert_eq!(
            Frame::new(Point3::ORIGIN, Vec3::Z * 3.0, Vec3::X * 0.5),
            Some(f)
        );
    }

    #[test]
    fn new_takes_the_hint_square_to_the_axis() {
        let f = Frame::new(
            Point3::new(1.0, 2.0, 3.0),
            Vec3::Z,
            Vec3::new(1.0, 1.0, 5.0),
        )
        .unwrap();
        assert!(near(
            *f.x(),
            Vec3::new(1.0, 1.0, 0.0) / 2.0_f64.sqrt(),
            TIGHT
        ));
        assert!(near(
            *f.y(),
            Vec3::new(-1.0, 1.0, 0.0) / 2.0_f64.sqrt(),
            TIGHT
        ));
        assert_eq!(*f.z(), Vec3::Z);
        assert_eq!(f.origin(), Point3::new(1.0, 2.0, 3.0));
    }

    #[test]
    fn new_refuses_what_has_no_frame() {
        let o = Point3::ORIGIN;
        assert_eq!(Frame::new(o, Vec3::ZERO, Vec3::X), None, "no axis");
        assert_eq!(Frame::new(o, Vec3::Z, Vec3::ZERO), None, "no hint");
        assert_eq!(
            Frame::new(o, Vec3::Z, Vec3::Z * -2.0),
            None,
            "hint along the axis"
        );
        assert_eq!(Frame::new(o, Vec3::new(f64::NAN, 0.0, 1.0), Vec3::X), None);
        assert_eq!(
            Frame::new(Point3::new(f64::INFINITY, 0.0, 0.0), Vec3::Z, Vec3::X),
            None
        );
        // A hint shorter than `Vec3::normalized` takes for a direction gives no frame; a
        // short or huge one above that is still a direction.
        assert_eq!(Frame::new(o, Vec3::Z, Vec3::X * 1e-300), None);
        assert!(Frame::new(o, Vec3::Z, Vec3::X * 1e-9).is_some());
        assert!(Frame::new(o, Vec3::Z * 1e300, Vec3::X * 1e300).is_some());
    }

    /// Property: every frame `new` builds is orthonormal and right-handed, its x lies in the
    /// plane of z and the hint on the hint's side, and `to_affine` maps the unit axes to it.
    #[test]
    fn new_frames_are_orthonormal_and_right_handed() {
        let mut r = Rng::new(0xf4a3_e001);
        let mut built = 0;
        for i in 0..20_000 {
            // Scales 1e-10 .. 1e190: above the length `normalized` takes for a direction.
            let s = crate::fmath::powf(10.0, r.next_f64() * 200.0 - 10.0);
            let (o, z, h) = (
                p(&mut r, 1e3),
                v(&mut r, s),
                v(&mut r, if i % 2 == 0 { s } else { 1.0 }),
            );
            let Some(f) = Frame::new(o, z, h) else {
                continue;
            };
            built += 1;
            assert!(orthonormal(&f), "{f:?}");
            let hn = *h.normalized().unwrap();
            assert!(f.x().dot(hn) >= -TIGHT, "x on the hint's side");
            assert!(f.y().dot(hn).abs() <= 4.0 * TIGHT, "hint in the xz plane");
            let a = f.to_affine();
            assert_eq!(a.apply(Point3::ORIGIN), o);
            assert_eq!(a.apply_vec(Vec3::X), *f.x());
            assert_eq!(a.apply_vec(Vec3::Z), *f.z());
        }
        assert!(built > 19_000, "{built}");
    }

    /// Property: a rotation and translation carry a frame to the frame of the images, with
    /// the sense kept: mapping the frame's local point equals mapping its image.
    #[test]
    fn rigid_maps_carry_the_frame() {
        let mut r = Rng::new(0xf4a3_e002);
        for _ in 0..2_000 {
            let f = random_frame(&mut r);
            let rot = Affine::rotation(p(&mut r, 50.0), v(&mut r, 1.0), r.next_signed_unit() * 7.0);
            let Ok(rot) = rot else { continue };
            let a = Affine::translation(v(&mut r, 20.0)) * rot;
            let (g, sense) = f.transformed(&a).unwrap();
            assert_eq!(sense, Sense::Forward);
            assert!(orthonormal(&g));
            let (lhs, rhs) = (a * f.to_affine(), g.to_affine());
            for q in [Vec3::X, Vec3::Y, Vec3::Z, Vec3::new(3.0, -2.0, 5.0)] {
                assert!(near(lhs.apply_vec(q), rhs.apply_vec(q), 1e-13), "{q:?}");
            }
            assert!(near_p(
                lhs.apply(Point3::ORIGIN),
                rhs.apply(Point3::ORIGIN),
                1e-11
            ));
        }
    }

    /// Property (D14): a mirror gives a right-handed frame and the reversed sense. The images
    /// of x and y are kept, so the `(u, v)` map is unchanged, and the stored z is the reverse
    /// of the image of z: `mirror ∘ F` is `G ∘ (u, v, w) ↦ (u, v, -w)`.
    #[test]
    fn mirror_then_frame_is_right_handed_and_reversed() {
        let mut r = Rng::new(0xf4a3_e003);
        let flip = Affine::from_axes(Vec3::X, Vec3::Y, -Vec3::Z, Point3::ORIGIN);
        for _ in 0..2_000 {
            let f = random_frame(&mut r);
            let Ok(m) = Affine::reflection(p(&mut r, 50.0), v(&mut r, 1.0)) else {
                continue;
            };
            let (g, sense) = f.transformed(&m).unwrap();
            assert_eq!(sense, Sense::Reversed);
            assert!(orthonormal(&g), "a stored frame is right-handed");
            assert!(near(*g.x(), m.apply_vec(*f.x()), 1e-14));
            assert!(near(*g.y(), m.apply_vec(*f.y()), 1e-14));
            assert!(near(*g.z(), -m.apply_vec(*f.z()), 1e-14));
            let (lhs, rhs) = (m * f.to_affine(), g.to_affine() * flip);
            for q in [Vec3::X, Vec3::Y, Vec3::Z, Vec3::new(3.0, -2.0, 5.0)] {
                assert!(near(lhs.apply_vec(q), rhs.apply_vec(q), 1e-13), "{q:?}");
            }
            assert!(near_p(
                lhs.apply(Point3::ORIGIN),
                rhs.apply(Point3::ORIGIN),
                1e-11
            ));
            // Mirroring back restores the frame and the sense.
            let (h, back) = g.transformed(&m).unwrap();
            assert_eq!(back, Sense::Reversed, "each mirror reverses once more");
            assert!(near(*h.x(), *f.x(), 1e-13) && near(*h.z(), *f.z(), 1e-13));
        }
    }

    /// Property (FU1): under a map whose matrix is exact (a power-of-two scale, a mirror in a
    /// coordinate plane, a quarter turn about a coordinate axis, through the origin) and its
    /// inverse, a frame on no world axis comes back to the bit, and each image is orthonormal.
    #[test]
    fn exact_maps_and_their_inverses_return_the_frame_to_the_bit() {
        let mut r = Rng::new(0xf4a3_e008);
        let o = Point3::ORIGIN;
        let maps = [
            Affine::uniform_scaling(o, 4.0).unwrap(),
            Affine::uniform_scaling(o, 0.125).unwrap(),
            Affine::reflection(o, Vec3::Y).unwrap(),
            Affine::from_axes(Vec3::Y, -Vec3::X, Vec3::Z, o),
            Affine::from_axes(Vec3::X, -Vec3::Z, Vec3::Y, o),
            Affine::uniform_scaling(o, 2.0).unwrap() * Affine::reflection(o, Vec3::X).unwrap(),
        ];
        for _ in 0..2_000 {
            let f = random_frame(&mut r);
            for a in &maps {
                let (g, sense) = f.transformed(a).unwrap();
                assert!(orthonormal(&g));
                let (h, back) = g.transformed(&a.inverse().unwrap()).unwrap();
                assert_eq!(h, f, "{a:?}");
                assert_eq!(back, sense);
            }
        }
    }

    /// A uniform scale keeps the axes; a non-uniform one keeps x's direction and the image
    /// plane of x and y; a map that flattens that plane gives no frame.
    #[test]
    fn scales_and_flattening() {
        let f = Frame::new(
            Point3::new(1.0, 0.0, 0.0),
            Vec3::new(0.0, 1.0, 1.0),
            Vec3::X,
        )
        .unwrap();
        let s = Affine::uniform_scaling(Point3::ORIGIN, 3.0).unwrap();
        let (g, sense) = f.transformed(&s).unwrap();
        assert_eq!(sense, Sense::Forward);
        assert!(near(*g.x(), *f.x(), TIGHT) && near(*g.z(), *f.z(), TIGHT));
        assert_eq!(g.origin(), Point3::new(3.0, 0.0, 0.0));
        let squash = Affine::from_axes(Vec3::X, Vec3::Y * 2.0, Vec3::Z * 0.5, Point3::ORIGIN);
        let (g, sense) = f.transformed(&squash).unwrap();
        assert_eq!(sense, Sense::Forward);
        assert!(orthonormal(&g));
        assert!(near(*g.x(), Vec3::X, TIGHT));
        let flat = Affine::from_axes(Vec3::X, Vec3::ZERO, Vec3::ZERO, Point3::ORIGIN);
        assert_eq!(f.transformed(&flat), None);
        let nan = Affine::translation(Vec3::new(f64::NAN, 0.0, 0.0));
        assert_eq!(f.transformed(&nan), None);
    }

    #[test]
    fn half_turns_negate_two_axes_exactly() {
        let mut r = Rng::new(0xf4a3_e007);
        for _ in 0..200 {
            let f = Frame::new(p(&mut r, 10.0), v(&mut r, 1.0), v(&mut r, 1.0)).unwrap();
            let h = f.half_turn_about_x();
            assert_eq!((h.origin(), h.x(), h.z()), (f.origin(), f.x(), -f.z()));
            assert_eq!(h.y(), -f.y());
            let h = f.half_turn_about_y();
            assert_eq!((h.origin(), h.x(), h.z()), (f.origin(), -f.x(), -f.z()));
            assert_eq!(h.y(), f.y());
        }
    }

    #[test]
    fn sense_signs() {
        assert_eq!(Sense::default(), Sense::Forward);
        assert_eq!(Sense::Forward.sign(), 1.0);
        assert_eq!(Sense::Reversed.sign(), -1.0);
        assert_eq!(Sense::Forward.reversed(), Sense::Reversed);
    }

    /// Property: every frame `new` and `transformed` build comes back from its own axes to
    /// the bit, which `new` does not give (it rounds the axes again).
    #[test]
    fn from_axes_gives_built_frames_back_to_the_bit() {
        let mut r = Rng::new(0xf4a3_e008);
        let mut renormalized = 0;
        for i in 0..20_000 {
            let s = [1e-6, 1e-3, 1.0, 1e3, 1e6][i % 5];
            let f = Frame::new(p(&mut r, 100.0), v(&mut r, s), v(&mut r, s));
            let Some(mut f) = f else { continue };
            // Random maps take `transformed`'s normalizing path; similarities (a rotation
            // and a scale) its exact path, which keeps axes within the same tolerance.
            let a = match i % 4 {
                1 => Some(Affine::from_axes(
                    v(&mut r, 1.0),
                    v(&mut r, 1.0),
                    v(&mut r, 1.0),
                    f.origin(),
                )),
                3 => Affine::rotation(f.origin(), v(&mut r, 1.0), 0.7)
                    .ok()
                    .and_then(|rot| Some(Affine::uniform_scaling(f.origin(), 3.0).ok()? * rot)),
                _ => None,
            };
            if let Some((g, _)) = a.and_then(|a| f.transformed(&a)) {
                f = g;
            }
            assert_eq!(Frame::from_axes(f.origin(), *f.x(), *f.z()), Ok(f));
            renormalized += usize::from(Frame::new(f.origin(), *f.z(), *f.x()) != Some(f));
            let x2 = Vec2::new(f.x().x, f.x().y);
            if let Some(g) = Frame2::new(Point2::new(f.origin().x, 1.0), x2) {
                assert_eq!(Frame2::from_axis(g.origin(), *g.x()), Ok(g));
            }
        }
        assert!(
            renormalized > 0,
            "new kept every frame: from_axes is not needed"
        );
    }

    #[test]
    fn from_axes_refuses_axes_off_by_more_than_rounding() {
        let o = Point3::ORIGIN;
        let tol = AXIS_TOLERANCE;
        assert_eq!(Frame::from_axes(o, Vec3::X, Vec3::Z), Ok(Frame::WORLD));
        let long = Vec3::X * (1.0 + 2.0 * tol);
        assert!(matches!(
            Frame::from_axes(o, long, Vec3::Z),
            Err(FrameError::NotUnit {
                axis: FrameAxis::X,
                ..
            })
        ));
        assert!(matches!(
            Frame::from_axes(o, Vec3::X, Vec3::Z * 0.5),
            Err(FrameError::NotUnit { axis: FrameAxis::Z, length_squared }) if length_squared == 0.25
        ));
        let tilted = Vec3::new(0.0, 1e-3, 1.0);
        let tilted = tilted / tilted.length();
        assert!(matches!(
            Frame::from_axes(o, Vec3::X + Vec3::Z * 1e-3, Vec3::Z),
            Err(FrameError::NotUnit { .. })
        ));
        assert!(matches!(
            Frame::from_axes(o, *Frame::WORLD.y(), tilted),
            Err(FrameError::NotSquare { .. })
        ));
        assert_eq!(
            Frame::from_axes(Point3::new(f64::NAN, 0.0, 0.0), Vec3::X, Vec3::Z),
            Err(FrameError::NotFinite)
        );
        assert_eq!(
            Frame::from_axes(o, Vec3::X, Vec3::new(0.0, 0.0, f64::INFINITY)),
            Err(FrameError::NotFinite)
        );
        let e = Frame::from_axes(o, Vec3::X, Vec3::X).unwrap_err();
        assert_eq!(
            (e.kind(), e.name()),
            (ErrorKind::Refused, "frame_not_square")
        );
        assert_eq!(
            Frame2::from_axis(Point2::ORIGIN, Vec2::new(0.0, 2.0)).map_err(|e| e.name()),
            Err("frame_not_unit")
        );
        assert_eq!(
            Frame2::from_axis(Point2::ORIGIN, Vec2::new(0.0, 1.0)).map(|f| *f.x()),
            Ok(Vec2::new(0.0, 1.0))
        );
    }
}
