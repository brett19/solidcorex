// Written for solidcorex. Apache-2.0.
//! The view: a direction of sight and the screen's axes, and how a view and the curves found
//! under it move between a body's coordinates and the world's.

use scx_num::frame::Frame;
use scx_num::vector2d::Point2;
use scx_num::vector3d::{Point3, UnitVec3, Vec3};

use crate::error::{BadView, HlrError};

/// A parallel view: the direction of sight `d` and the screen's axes `x = unit(d × up)` and
/// `y = x × d`, a right-handed frame with `d` going into the screen (the pin's convention).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct View {
    x: UnitVec3,
    y: UnitVec3,
    d: UnitVec3,
}

impl View {
    /// The view looking along `direction` with `up` up the screen.
    ///
    /// # Errors
    ///
    /// [`HlrError::BadView`] for a direction or an up that is zero, infinite or NaN, or an up
    /// parallel to the direction (their cross product has no direction).
    pub fn new(direction: Vec3, up: Vec3) -> Result<Self, HlrError> {
        let d = unit(direction).ok_or(HlrError::BadView(BadView::Direction))?;
        unit(up).ok_or(HlrError::BadView(BadView::Up))?;
        let x = d
            .cross(up)
            .normalized()
            .ok_or(HlrError::BadView(BadView::Parallel))?;
        let y = x
            .cross(*d)
            .normalized()
            .ok_or(HlrError::BadView(BadView::Parallel))?;
        Ok(Self { x, y, d })
    }

    /// The screen's x axis.
    #[must_use]
    pub const fn x(&self) -> UnitVec3 {
        self.x
    }

    /// The screen's y axis.
    #[must_use]
    pub const fn y(&self) -> UnitVec3 {
        self.y
    }

    /// The direction of sight, into the screen.
    #[must_use]
    pub const fn d(&self) -> UnitVec3 {
        self.d
    }

    /// `p` on the screen, measured from `origin` (a scene's centre keeps the coordinates
    /// small, so the screen's rounding is the scene's size, not its distance from zero).
    #[must_use]
    pub fn screen(&self, p: Point3, origin: Point3) -> Point2 {
        let r = p - origin;
        Point2::new(r.dot(*self.x), r.dot(*self.y))
    }

    /// The same view in the coordinates of a body placed by `frame` (body coordinates
    /// `(a, b, c)` are the world point `o + a x + b y + c z`): each axis's components along
    /// the frame's axes. A rigid frame keeps the axes unit and orthogonal up to rounding.
    pub(crate) fn in_frame(&self, frame: &Frame) -> Option<Self> {
        let to = |v: UnitVec3| {
            Vec3::new(v.dot(*frame.x()), v.dot(*frame.y()), v.dot(*frame.z())).normalized()
        };
        Some(Self {
            x: to(self.x)?,
            y: to(self.y)?,
            d: to(self.d)?,
        })
    }
}

/// `v` as a unit vector, `None` for zero, infinite or NaN.
fn unit(v: Vec3) -> Option<UnitVec3> {
    if v.is_finite() { v.normalized() } else { None }
}

/// The world point of the body point `p` placed by `frame`.
pub(crate) fn place_point(frame: &Frame, p: Point3) -> Point3 {
    frame.origin() + place_vec(frame, p.to_vec())
}

/// The world direction of the body direction `v` placed by `frame`.
pub(crate) fn place_vec(frame: &Frame, v: Vec3) -> Vec3 {
    *frame.x() * v.x + *frame.y() * v.y + *frame.z() * v.z
}
