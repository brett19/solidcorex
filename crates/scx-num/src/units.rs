// Written for solidcorex (docs/09-tolerances.md §7.1): the tolerance newtypes, defined
// now so the accuracy/resolution model can be introduced later without reopening this
// layer. Nothing moved here reads them yet.
//! Typed lengths and tolerances.
//!
//! [`Length`] is a length in millimetres. [`Accuracy`] (per entity: a proved bound on
//! how far the stored entity lies from the one it stands for) and [`Resolution`] (per
//! operation: how close two things must be to count as one) both wrap a length and are
//! distinct types, with no arithmetic between them. [`AngleTol`] is an angle tolerance
//! in radians and [`ParamTol`] a tolerance in a curve's or surface's parameter space.
//! The glossary (`docs/08-glossary.md`) gives each word its one meaning.

/// A length, in millimetres.
#[derive(Debug, Clone, Copy, PartialEq, PartialOrd, Default)]
pub struct Length(pub f64);

/// Per entity: a proved upper bound on how far the stored entity lies from the exact
/// entity it stands for. A length, at least 0.
#[derive(Debug, Clone, Copy, PartialEq, PartialOrd, Default)]
pub struct Accuracy(pub Length);

/// Per operation: how close two things must be for the operation to treat them as one.
/// A length, above 0.
#[derive(Debug, Clone, Copy, PartialEq, PartialOrd)]
pub struct Resolution(pub Length);

/// An angle tolerance, in radians.
#[derive(Debug, Clone, Copy, PartialEq, PartialOrd, Default)]
pub struct AngleTol(pub f64);

/// A tolerance in parameter space (a curve's `t`, a surface's `u` or `v`).
#[derive(Debug, Clone, Copy, PartialEq, PartialOrd, Default)]
pub struct ParamTol(pub f64);

impl Length {
    /// The length in millimetres.
    #[must_use]
    pub const fn mm(self) -> f64 {
        self.0
    }
}

impl Accuracy {
    /// The bound, as a length.
    #[must_use]
    pub const fn length(self) -> Length {
        self.0
    }
}

impl Resolution {
    /// The resolution, as a length.
    #[must_use]
    pub const fn length(self) -> Length {
        self.0
    }
}

impl AngleTol {
    /// The tolerance in radians.
    #[must_use]
    pub const fn rad(self) -> f64 {
        self.0
    }
}

impl ParamTol {
    /// The tolerance in parameter units.
    #[must_use]
    pub const fn value(self) -> f64 {
        self.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn values_pass_through_unchanged() {
        let x = 1.0e-5;
        assert_eq!(Accuracy(Length(x)).length().mm().to_bits(), x.to_bits());
        assert_eq!(Resolution(Length(x)).length().mm().to_bits(), x.to_bits());
        assert_eq!(AngleTol(x).rad().to_bits(), x.to_bits());
        assert_eq!(ParamTol(x).value().to_bits(), x.to_bits());
    }
}
