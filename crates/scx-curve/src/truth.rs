// Written for solidcorex. Apache-2.0.
//! Test support: comparisons against the testkit's double-double references.

use scx_num::interval::Ivl;
use scx_testkit::dd::Dd;

/// Unit roundoff, `2^-53`.
pub const U: f64 = f64::EPSILON / 2.0;

/// `|got - want| <= ulps u scale` (plus the subnormal floor), compared in double-double.
pub fn near(got: f64, want: Dd, scale: f64, ulps: f64) -> bool {
    let err = (Dd::new(got) - want).abs().hi;
    err <= ulps * U * scale + f64::MIN_POSITIVE
}

/// `want` lies in `i`, compared in double-double.
pub fn holds(i: Ivl, want: Dd) -> bool {
    (want - Dd::new(i.lo)).hi >= 0.0 && (Dd::new(i.hi) - want).hi >= 0.0
}
