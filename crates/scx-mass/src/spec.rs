// Written for solidcorex. Apache-2.0.
//! What a measurement is asked for and what it gives back.

use scx_num::vector3d::{Point3, Vec3};

use crate::error::{MassError, MassRefusal};

/// A relative accuracy: finite, in `(0, 1]`. Each piece of each integral is held to a tenth
/// of it against the larger of its own size and its share of the entity's size, so a
/// result's error is of order `accuracy` times the entity's size; every result states the
/// estimate it reached.
#[derive(Debug, Clone, Copy, PartialEq, PartialOrd)]
pub struct RelAccuracy(f64);

impl RelAccuracy {
    /// `value`, checked.
    ///
    /// # Errors
    ///
    /// [`MassRefusal::BadAccuracy`] for a value not finite or not in `(0, 1]`.
    pub fn new(value: f64) -> Result<Self, MassError> {
        if value.is_finite() && value > 0.0 && value <= 1.0 {
            Ok(Self(value))
        } else {
            Err(MassError::Refused(MassRefusal::BadAccuracy { value }))
        }
    }

    /// The value.
    #[must_use]
    pub const fn get(self) -> f64 {
        self.0
    }
}

/// A value and the integrator's estimate of its error: an estimate, not a proved bound
/// (the Kronrod and Gauss values' difference, summed over the pieces, with the inner
/// integrals' estimates carried through).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Estimate {
    /// The value.
    pub value: f64,
    /// The estimated absolute error, at least zero.
    pub error: f64,
}

/// A symmetric 3 x 3 matrix by its six entries.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Sym3 {
    /// `(0, 0)`.
    pub xx: f64,
    /// `(1, 1)`.
    pub yy: f64,
    /// `(2, 2)`.
    pub zz: f64,
    /// `(1, 2)` and `(2, 1)`.
    pub yz: f64,
    /// `(2, 0)` and `(0, 2)`.
    pub zx: f64,
    /// `(0, 1)` and `(1, 0)`.
    pub xy: f64,
}

/// What [`mass`](crate::mass) measures.
#[derive(Debug, Clone, Copy, PartialEq)]
#[non_exhaustive]
pub struct MassSpec {
    /// The relative accuracy.
    pub accuracy: RelAccuracy,
    /// The density, uniform: finite and above zero, 1 unless given.
    pub density: f64,
    /// Where the moments are taken about; the body box's middle, rounded to a grid, when
    /// `None`.
    pub about: Option<Point3>,
}

impl MassSpec {
    /// At `accuracy`, unit density, about the body's own middle.
    #[must_use]
    pub const fn new(accuracy: RelAccuracy) -> Self {
        Self {
            accuracy,
            density: 1.0,
            about: None,
        }
    }

    /// The same at `density`, checked when measured.
    #[must_use]
    pub const fn with_density(self, density: f64) -> Self {
        Self { density, ..self }
    }

    /// The same about `about`, checked when measured.
    #[must_use]
    pub const fn with_about(self, about: Point3) -> Self {
        Self {
            about: Some(about),
            ..self
        }
    }
}

/// A body's mass properties about [`Mass::about`]: the volume integrals at unit density,
/// the boundary's area integrals, and each one's error estimate in [`Mass::error`].
///
/// The volume integrals count each face between a solid and a void region once, signed so
/// that the solid is inside; the area integrals count each face that is not between two
/// solid regions (sheets and the walls of voids included).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Mass {
    /// The reference point.
    pub about: Point3,
    /// The density.
    pub density: f64,
    /// `∫ 1 dV`.
    pub volume: f64,
    /// `∫ p dV`, `p` from [`Mass::about`].
    pub first: Vec3,
    /// `∫ p p^T dV`.
    pub second: Sym3,
    /// `∫ 1 dA` over the boundary.
    pub area: f64,
    /// `∫ p dA` over the boundary.
    pub area_first: Vec3,
    /// The error estimate of each.
    pub error: MassErrors,
}

/// The error estimates of a [`Mass`], field for field.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct MassErrors {
    /// Of [`Mass::volume`].
    pub volume: f64,
    /// Of [`Mass::first`], per component.
    pub first: Vec3,
    /// Of [`Mass::second`], per entry.
    pub second: Sym3,
    /// Of [`Mass::area`].
    pub area: f64,
    /// Of [`Mass::area_first`], per component.
    pub area_first: Vec3,
}

impl Mass {
    /// The mass: the volume times the density.
    #[must_use]
    pub fn mass(&self) -> f64 {
        self.volume * self.density
    }

    /// The centre of volume, or `None` for no volume.
    #[must_use]
    pub fn centroid(&self) -> Option<Point3> {
        (self.volume != 0.0).then(|| self.about + self.first / self.volume)
    }

    /// The centre of area, or `None` for no area.
    #[must_use]
    pub fn area_centroid(&self) -> Option<Point3> {
        (self.area != 0.0).then(|| self.about + self.area_first / self.area)
    }

    /// The inertia tensor about the centroid, times the density: `I = tr(C) 1 - C` with
    /// `C = S - F F^T / V`. `None` for no volume.
    #[must_use]
    pub fn inertia(&self) -> Option<Sym3> {
        if self.volume == 0.0 {
            return None;
        }
        let (f, s, v) = (self.first, self.second, self.volume);
        let c = Sym3 {
            xx: s.xx - f.x * f.x / v,
            yy: s.yy - f.y * f.y / v,
            zz: s.zz - f.z * f.z / v,
            yz: s.yz - f.y * f.z / v,
            zx: s.zx - f.z * f.x / v,
            xy: s.xy - f.x * f.y / v,
        };
        let tr = c.xx + c.yy + c.zz;
        let d = self.density;
        Some(Sym3 {
            xx: d * (tr - c.xx),
            yy: d * (tr - c.yy),
            zz: d * (tr - c.zz),
            yz: -d * c.yz,
            zx: -d * c.zx,
            xy: -d * c.xy,
        })
    }
}
