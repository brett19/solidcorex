// Written for solidcorex. Apache-2.0.
//! solidcorex layer 1: NURBS as plain data over `scx-num`.
//!
//! - [`KnotVector`]: a validated, non-decreasing knot vector of a degree, with multiplicity,
//!   span finding ([`Approach`]), knot insertion and removal at the knot level;
//! - [`Basis`], [`BasisDerivs`], [`BasisJet`]: the non-vanishing B-spline basis functions at a parameter and
//!   their derivatives;
//! - [`bezier`]: binomials, Bernstein polynomials, Bezier degree elevation and subdivision;
//! - [`NurbsCurve`]: a validated curve in `D` dimensions, polynomial or rational, with
//!   evaluation, derivatives, knot insertion, splitting and degree elevation.
//!
//! A value that exists is valid: every check runs once, where it is built, so evaluation
//! fails only for a parameter or arithmetic that cannot go on ([`NurbsError`]).
//!
//! The algorithms are those of solidcore at the pin (Piegl and Tiller's A2.1-A2.3, A3.2,
//! A5.1, A5.9, de Casteljau's, and the pin's rational basis derivatives), with the pin's
//! arithmetic order, so results agree bit for bit where the inputs are the same. What differs is exactness at the edges: spans are found
//! without the pin's absolute `1e-12` knot tolerance, a knot difference fails only when it is
//! exactly zero (not below `1e-24`), and the ends of the domain evaluate.
//!
//! Fitting, approximation, deviation, enclosures and IGES 126 build on these types later (01
//! §10.2 step 5); nothing here is a long-lived object with setters.

mod basis;
pub mod bezier;
mod curve;
mod error;
mod knots;
mod point;

pub use basis::{Basis, BasisDerivs, BasisJet};
pub use curve::NurbsCurve;
pub use error::NurbsError;
pub use knots::{Approach, KnotVector};

/// The highest degree supported, of a basis, a curve or a Bezier piece.
///
/// It bounds the scratch arrays the evaluation keeps on the stack, and keeps every binomial
/// coefficient that degree elevation and rational derivatives use exact in `f64`.
pub const MAX_DEGREE: usize = 31;

/// The highest derivative order supported. Derivatives above the degree of a polynomial
/// curve vanish; the bound keeps the binomial coefficients of rational derivatives exact.
pub const MAX_ORDER: usize = MAX_DEGREE;

/// `Result` with [`NurbsError`].
pub type Result<T> = core::result::Result<T, NurbsError>;

// Every public type is shareable across threads (57 §5.2).
const _: () = {
    const fn ok<T: Send + Sync>() {}
    ok::<KnotVector>();
    ok::<Approach>();
    ok::<Basis>();
    ok::<BasisDerivs<3>>();
    ok::<BasisJet<2>>();
    ok::<NurbsCurve<3>>();
    ok::<NurbsError>();
};
