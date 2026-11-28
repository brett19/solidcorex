// Written for solidcorex. Apache-2.0.
//! How near is near, for the plane, cylinder and sheet helpers (research 74 §34).
//!
//! The pin tested "parallel" against a kernel angle (half a degree) and "the same place"
//! against its zone tolerance. There is no angle tolerance here (01 §4.2): two directions
//! are one line when the lines they make stay within coincidence over the stretch they are
//! compared across. [`Near`] holds that stretch (the `reach`: a face's or an edge's size)
//! and the coincidence distance, so the angle is coincidence divided by size.

use scx_num::units::{Accuracy, Length, Resolution, coincident};
use scx_num::vector3d::{Point3, Vec3};

use crate::error::{BodyqError, BodyqRefusal};

/// A coincidence distance and the reach it is held over.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Near {
    coincidence: f64,
    reach: f64,
}

impl Near {
    /// `coincidence` held over `reach`.
    ///
    /// # Errors
    ///
    /// Refused ([`NotFinite`](BodyqRefusal::NotFinite)) for a length that is not finite,
    /// ([`NotPositive`](BodyqRefusal::NotPositive)) for a negative coincidence or a reach
    /// not above zero.
    pub fn new(coincidence: Length, reach: Length) -> Result<Self, BodyqError> {
        let (c, r) = (coincidence.mm(), reach.mm());
        if !(c.is_finite() && r.is_finite()) {
            return Err(BodyqError::Refused(BodyqRefusal::NotFinite));
        }
        if c < 0.0 || r <= 0.0 {
            return Err(BodyqError::Refused(BodyqRefusal::NotPositive));
        }
        Ok(Self {
            coincidence: c,
            reach: r,
        })
    }

    /// The coincidence of two entities of accuracies `a` and `b` under `res`
    /// ([`coincident`]), held over `reach`.
    ///
    /// # Errors
    ///
    /// As [`Near::new`].
    pub fn of(
        res: Resolution,
        a: Accuracy,
        b: Accuracy,
        reach: Length,
    ) -> Result<Self, BodyqError> {
        Self::new(coincident(res, a, b), reach)
    }

    /// The distance within which two places are one.
    #[must_use]
    pub const fn coincidence(&self) -> Length {
        Length::from_mm(self.coincidence)
    }

    /// The stretch the directions are compared over.
    #[must_use]
    pub const fn reach(&self) -> Length {
        Length::from_mm(self.reach)
    }

    /// The same, held over another reach.
    ///
    /// # Errors
    ///
    /// As [`Near::new`].
    pub fn over(&self, reach: Length) -> Result<Self, BodyqError> {
        Self::new(self.coincidence(), reach)
    }

    /// Whether two unit directions make one line over the reach: the lines through a point
    /// of each, parallel or anti-parallel, stay within coincidence of each other there.
    pub(crate) fn one_line(&self, a: Vec3, b: Vec3) -> bool {
        a.cross(b).length() * self.reach <= self.coincidence
    }

    /// Whether `d` is within coincidence.
    pub(crate) fn within(&self, d: f64) -> bool {
        d.abs() <= self.coincidence
    }
}

/// `v` as a unit vector.
///
/// # Errors
///
/// [`NotFinite`](BodyqRefusal::NotFinite), or [`NoDirection`](BodyqRefusal::NoDirection)
/// for the zero vector.
pub(crate) fn unit3(v: Vec3) -> Result<Vec3, BodyqError> {
    if !v.is_finite() {
        return Err(BodyqError::Refused(BodyqRefusal::NotFinite));
    }
    v.length_and_unit()
        .map(|(_, u)| u.as_vec())
        .ok_or(BodyqError::Refused(BodyqRefusal::NoDirection))
}

/// `p` as it is, if finite.
pub(crate) fn finite_point(p: Point3) -> Result<Point3, BodyqError> {
    if p.is_finite() {
        Ok(p)
    } else {
        Err(BodyqError::Refused(BodyqRefusal::NotFinite))
    }
}

/// `x` as it is, if finite.
pub(crate) fn finite(x: f64) -> Result<f64, BodyqError> {
    if x.is_finite() {
        Ok(x)
    } else {
        Err(BodyqError::Refused(BodyqRefusal::NotFinite))
    }
}

/// A length above zero and finite, in mm.
pub(crate) fn positive(l: Length) -> Result<f64, BodyqError> {
    let x = finite(l.mm())?;
    if x > 0.0 {
        Ok(x)
    } else {
        Err(BodyqError::Refused(BodyqRefusal::NotPositive))
    }
}
