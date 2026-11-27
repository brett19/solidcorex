// Written for solidcorex. Apache-2.0.
//! The nearest point of a curve or surface to a query, as a value: a [`Foot`], and an
//! [`Inversion`] that says whether its parameter lies in the domain asked about (57 F6,
//! without `Failed`; 63 D7). Shared by `scx-curve` and `scx-surface`.

use crate::units::Length;
use crate::vector3d::Point3;

/// A point of a curve nearest a query: its parameter `at`, the point there, and the
/// distance from the query.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Foot<A = f64, Q = Point3> {
    /// The parameter.
    pub at: A,
    /// The curve's point at `at`.
    pub point: Q,
    /// The distance from the query to `point`.
    pub distance: f64,
}

impl<A, Q> Foot<A, Q> {
    /// The query lies on the curve within `tol`: `distance <= tol`. A zero tolerance is
    /// zero; a NaN distance is never on.
    #[must_use]
    pub fn is_on(&self, tol: Length) -> bool {
        self.distance <= tol.mm()
    }
}

/// A curve's inversion of a point: the nearest foot, inside or outside the domain asked
/// about (57 F6, without `Failed`; 63 D7).
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Inversion<A = f64, Q = Point3> {
    /// The foot's parameter lies in the domain.
    Inside(Foot<A, Q>),
    /// It does not; the foot is still the kind's nearest point.
    Outside(Foot<A, Q>),
}

impl<A, Q> Inversion<A, Q> {
    /// The foot, inside or not.
    #[must_use]
    pub const fn foot(&self) -> &Foot<A, Q> {
        match self {
            Self::Inside(f) | Self::Outside(f) => f,
        }
    }

    /// The foot, by value.
    #[must_use]
    pub fn into_foot(self) -> Foot<A, Q> {
        match self {
            Self::Inside(f) | Self::Outside(f) => f,
        }
    }

    /// The foot lies in the domain.
    #[must_use]
    pub const fn is_inside(&self) -> bool {
        matches!(self, Self::Inside(_))
    }
}
