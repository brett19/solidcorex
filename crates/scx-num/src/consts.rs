// Written for solidcorex (docs/09-tolerances.md §7.1, docs/07-conventions.md §6): the one
// definition of each tolerance constant the layers share. Values are solidcore's at
// commit 03e16c9ee766de5b9b1ced845e5149d62ff493b8, unchanged.
//! Shared numeric constants, one definition each.
//!
//! Each constant names its unit and who owns the decision it makes. The values are
//! solidcore's; the older names ([`crate::tol::ZONE_TOL_3D`],
//! [`crate::tol::LARGE_SMALL_SIZE_RATIO`], [`crate::math::EFF_ZERO_PARAM`],
//! [`crate::tol::Tolerances::default`]) are defined from these, so changing one here
//! changes every reader.

/// The default zone (resolution): two points closer than this are one point.
/// Millimetres. Owner: the tolerance policy (`docs/09-tolerances.md`); solidcore's
/// `SM_ZONE_TOL_3D`, 1e-5.
pub const ZONE_DEFAULT: f64 = 1.0e-5;

/// The absolute numeric zero, a length. Millimetres. solidcore's `SM_EFF_ZERO` read as
/// an absolute value. Its sites still use [`crate::math::EFF_ZERO`]; each is to be
/// classified as absolute ([`EPS_LEN`]) or relative ([`EPS_REL`]), not done yet.
pub const EPS_LEN: f64 = crate::math::EFF_ZERO;

/// The relative numeric zero, applied as `EPS_REL * (1 + |x|)` (the scaled zeros of
/// [`crate::tol`]). Unitless. solidcore's `SM_EFF_ZERO` read as a relative factor; see
/// [`EPS_LEN`].
pub const EPS_REL: f64 = crate::math::EFF_ZERO;

/// The numeric zero of a curve or surface parameter (knots, parameter values). A
/// parameter. solidcore's `SM_EFF_ZERO_PARAM`, 1.2e-8.
pub const EPS_PARAM: f64 = 1.2e-8;

/// The largest ratio of an entity's tolerance to the body's, a validity limit (Test 82
/// of the validity spec). Unitless. solidcore's `SM_LARGE_SMALL_SIZE_RATIO`, 1000.
pub const MAX_ACCURACY_RATIO: f64 = 1000.0;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn values_are_solidcores() {
        assert_eq!(ZONE_DEFAULT.to_bits(), 1.0e-5f64.to_bits());
        assert_eq!(EPS_LEN.to_bits(), 1.0e-12f64.to_bits());
        assert_eq!(EPS_REL.to_bits(), 1.0e-12f64.to_bits());
        assert_eq!(EPS_PARAM.to_bits(), 1.2e-8f64.to_bits());
        assert_eq!(MAX_ACCURACY_RATIO, 1000.0);
    }
}
