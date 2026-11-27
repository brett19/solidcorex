// Written for solidcorex. Apache-2.0.
//! The plane `S(u, v) = O + u X + v Y` of a frame.
//!
//! Its uv is STEP's (research 64 §2). A plane carries no sense: the image of a plane's frame
//! under a mirror is again a right-handed placement of the image (`X`, `Y` and their cross
//! product), so the mirror needs nothing more.

use crate::common::check_lengths;
use crate::error::SurfaceError;
use crate::to_nurbs::{Dir, Reach, build};
use scx_num::domain::{ParamDomain, UvDomain};
use scx_num::extent2d::Extent2d;
use scx_num::frame::Frame;
use scx_num::ring::Ring;
use scx_num::vector2d::Point2;
use scx_num::vector3d::{Point3, UnitVec3, Vec3};
use scx_nurbs::Approach;
use scx_nurbs_surface::{NurbsSurface, ParamAxis};

use scx_num::jet::SurfaceJet;

/// A plane: the frame's origin and its x and y axes. The normal is the frame's z.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Plane {
    frame: Frame,
}

impl Plane {
    /// The plane through `frame`'s origin spanned by its x and y axes. Every frame makes one.
    #[must_use]
    pub const fn new(frame: Frame) -> Self {
        Self { frame }
    }

    /// The frame.
    #[must_use]
    pub const fn frame(&self) -> &Frame {
        &self.frame
    }

    /// `S(u, v)`.
    #[must_use]
    pub fn point(&self, uv: Point2) -> Point3 {
        self.frame.origin() + *self.frame.x() * uv.x + *self.frame.y() * uv.y
    }

    /// `S(u, v)` in any ring (63 §3.2, 01 C10). `near` is unused: the plane has no breaks.
    #[must_use]
    pub fn eval<T: Ring>(&self, u: T, v: T, near: Point2) -> Point3<T> {
        let _ = near;
        let (o, x, y) = (self.frame.origin(), *self.frame.x(), *self.frame.y());
        let l = T::lift;
        Point3::new(
            l(o.x) + u * l(x.x) + v * l(y.x),
            l(o.y) + u * l(x.y) + v * l(y.y),
            l(o.z) + u * l(x.z) + v * l(y.z),
        )
    }

    /// The point at each of `uv`, into `out`: `out[i]` is `point(uv[i])`, bit for bit (a NaN
    /// is a NaN; Rust leaves its sign and payload open).
    ///
    /// # Panics
    ///
    /// If `uv` and `out` differ in length.
    pub fn eval_many(&self, uv: &[Point2], out: &mut [Point3]) {
        check_lengths(uv, out);
        for (o, &p) in out.iter_mut().zip(uv) {
            *o = self.point(p);
        }
    }

    /// The partials up to total order `D`. `side` changes nothing: the plane is smooth.
    #[must_use]
    pub fn jet<const D: usize>(&self, uv: Point2, side: [Approach; 2]) -> SurfaceJet<Vec3, D> {
        let _ = side;
        let p = self.point(uv).to_vec();
        SurfaceJet::from_fn(|i, j| match (i, j) {
            (0, 0) => p,
            (1, 0) => *self.frame.x(),
            (0, 1) => *self.frame.y(),
            _ => Vec3::default(),
        })
    }

    /// The unit normal `S_u x S_v`, the frame's z.
    #[must_use]
    pub fn normal(&self, uv: Point2) -> UnitVec3 {
        let _ = uv;
        self.frame.z()
    }

    /// The domain: no end in either direction.
    #[must_use]
    pub const fn domain(&self) -> UvDomain {
        UvDomain::new(ParamDomain::Unbounded, ParamDomain::Unbounded)
    }

    /// The exact rational image over `over`, in the same uv (63 §4.2, §4.8; D4, D13): the
    /// NURBS surface at `uv` is `point(uv)` within the rounding of its control data. Polynomial of degree 1 in each direction, its four corners `point` at the extent's.
    ///
    /// # Errors
    ///
    /// `Refused`: [`SurfaceError::EmptyExtent`] for an extent that is a single value along a
    /// direction, and [`SurfaceError::OutsideDomain`] for one that leaves the domain.
    /// [`SurfaceError::Nurbs`] only on overflow, for sizes near the largest double.
    pub fn to_nurbs(&self, over: &Extent2d) -> Result<NurbsSurface<3>, SurfaceError> {
        let u = Dir::new(ParamAxis::U, over.x_interval(), Reach::Any)?;
        let v = Dir::new(ParamAxis::V, over.y_interval(), Reach::Any)?;
        build(u, v, false, |(a, _), (b, _)| self.point(Point2::new(a, b)))
    }

    /// STEP's `(u, v)` at `uv`: the same.
    #[must_use]
    pub const fn to_step(&self, uv: Point2) -> Point2 {
        uv
    }

    /// `uv` at STEP's `(u, v)`: the same.
    #[must_use]
    pub const fn from_step(&self, step: Point2) -> Point2 {
        step
    }
}
