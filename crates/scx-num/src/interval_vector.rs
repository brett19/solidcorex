// Moved from solidcore `solidcore/crates/solidcore/src/geom/interval_vector.rs` at commit
// 03e16c9ee766de5b9b1ced845e5149d62ff493b8.
// Upstream record (verbatim from that file): Upstream: none. Shared by geometric certificates
// and analytic support queries. Deviations: no upstream counterpart. No recursion.
// Changes: the functions are public (callers are in other crates); `Ivl` from
// `crate::interval`. Results unchanged bit for bit.
//! Outward-rounded dot products, cross products and norms of three-dimensional vectors.
//!
//! Upstream: none. Shared by geometric certificates and analytic support queries.
//!
//! | Rust | Upstream |
//! |------|----------|
//! | [`cross`], [`dot`], [`square`], [`norm`] | none |
//!
//! Deviations: no upstream counterpart. No recursion.

use crate::interval::Ivl;

/// `a × b`, enclosed.
#[must_use]
pub fn cross(a: [Ivl; 3], b: [Ivl; 3]) -> [Ivl; 3] {
    let [ax, ay, az] = a;
    let [bx, by, bz] = b;
    [ay * bz - az * by, az * bx - ax * bz, ax * by - ay * bx]
}

/// `a · b`, enclosed.
#[must_use]
pub fn dot(a: [Ivl; 3], b: [Ivl; 3]) -> Ivl {
    let [ax, ay, az] = a;
    let [bx, by, bz] = b;
    ax * bx + ay * by + az * bz
}

/// `x^2`, enclosed with a nonnegative lower bound.
#[must_use]
pub fn square(x: Ivl) -> Ivl {
    let s = x * x;
    Ivl::new(s.lo.max(0.0), s.hi)
}

/// `|a|`, enclosed with nonnegative squared components.
#[must_use]
pub fn norm(a: [Ivl; 3]) -> Ivl {
    let [x, y, z] = a;
    (square(x) + square(y) + square(z)).sqrt()
}

/// A unit direction, enclosed, when its norm is bounded away from zero.
#[must_use]
pub fn unit(a: [Ivl; 3]) -> Option<[Ivl; 3]> {
    let length = norm(a);
    if !length.is_finite() || length.lo <= 0.0 {
        return None;
    }
    Some(a.map(|v| v / length))
}
