// Written for solidcorex (docs/09-tolerances.md §7.1, docs/07-conventions.md §6): the one
// definition of each tolerance constant the layers share. Values are solidcore's at
// commit 03e16c9ee766de5b9b1ced845e5149d62ff493b8, unchanged.
//! Shared numeric constants, one definition each.
//!
//! Each constant names its unit and who owns the decision it makes. The values are
//! solidcore's; the older names ([`crate::math::EFF_ZERO_PARAM`],
//! [`crate::units::Resolution::DEFAULT`]) are defined from these, so changing one here
//! changes every reader.
//! [`crate::tol::LARGE_SMALL_SIZE_RATIO`] is not one of them: it has the same value as
//! [`MAX_ACCURACY_RATIO`] but a different owner.

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
/// of the validity spec). Unitless. Owner: the validity checks. The value of solidcore's
/// `SM_LARGE_SMALL_SIZE_RATIO`, 1000, but not that constant: the tolerance default
/// [`crate::tol::LARGE_SMALL_SIZE_RATIO`] is its own literal, and changing one does not
/// change the other (a test holds them equal for now).
pub const MAX_ACCURACY_RATIO: f64 = 1000.0;

/// The largest angle between two directions that are parallel (or, from a right angle,
/// perpendicular), the default of [`crate::tol::are_parallel`]'s callers. Degrees.
/// Owner: the tolerance policy. solidcore's `SM_ANG_TOL_DEG`, 0.5.
pub const PARALLEL_ANGLE_DEG: f64 = 0.5;

/// [`PARALLEL_ANGLE_DEG`] in radians, `0.5 * PI / 180` as solidcore's `SM_ANG_TOL_RAD`
/// computes it.
pub const PARALLEL_ANGLE_RAD: f64 = PARALLEL_ANGLE_DEG * crate::math::PI / 180.0;

/// The largest error bound a NURBS degree elevation answers with: past it the elevation
/// is refused as unstable. Relative, to the largest homogeneous coordinate of the control
/// points. Owner: `scx-nurbs` degree elevation (`docs/05-status.md` §6); not solidcore's,
/// whose A5.9 has no bound. Between what the seeded truths need (a bound of `9e-9` at
/// degree 23, with the result `4e-11` off) and what it refuses (uniform knots raised to
/// degree 28 or more, where the result would be `3e-8` off or more).
pub const ELEVATION_ERROR: f64 = 1.0e-7;

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
        assert_eq!(PARALLEL_ANGLE_DEG, 0.5);
        assert_eq!(
            PARALLEL_ANGLE_RAD.to_bits(),
            (0.5 * crate::math::PI / 180.0).to_bits()
        );
    }

    #[test]
    fn the_validity_ratio_and_the_size_ratio_default_agree_for_now() {
        // Two decisions with two owners that happen to share a value at the pin; when one
        // moves, this test is where that is decided.
        assert_eq!(MAX_ACCURACY_RATIO, crate::tol::LARGE_SMALL_SIZE_RATIO);
    }
}
