// Written for solidcorex. Apache-2.0.
//! Curve fitting at given parameters: parameterisations, knot vectors, interpolation with end
//! conditions, and least squares, on banded solves (FT2; 77 §2).
//!
//! | solidcore at the pin (`nurbs::crv_fit`) | here |
//! |---|---|
//! | `ParamMethod`, `param_values` | [`Param`], [`parameters`] |
//! | `interp_knot_vector`, `knot_vector_end_derivs`, `knot_vector_deriv`, `knots_end_derivs`, `knots_derivs`, `knots_high_end_derivs` | [`averaging`] with the two ends' orders |
//! | `knot_vector_approx`, `approx_knots`' knots | [`approximation`] |
//! | `interp_given_params`, `interp`, `interp_knots_and_derivs`, `interp_knots_and_deriv`, `interp_end_derivs`, `interp_high_derivs`, `cubic_spline_interp` | [`interpolate`], [`interpolate_on`] |
//! | `approx_knots`, `approx_lst_sq`, `approx_end_derivs`, `weighted_lst_sq_knots` | [`least_squares`], [`weighted_least_squares`] |
//! | `NL_START`/`NL_END`, `DerivEnd::flag`, the `i32` option flags | [`End`] (a variant per condition), no flags |
//! | `RealMatrix` LU (dense), normal equations | one banded rotation solver (`band`, private) |
//! | `FitPoint` over `Vec3` and `CPoint` | `[f64; N]` for any `N`, homogeneous points included |
//!
//! Not ported here: first derivatives at *every* point (`interp_first_derivs*`,
//! `interp_knots_and_tangents`: the surface fit's, CN3), the closed-curve conditions
//! (`approx_closed_conditions`), the `LU`-reuse solves (`cpts_with_matrix`,
//! `cpts_from_srf_data`; a banded factor is not kept between solves, a surface fit pushes its
//! rows again), `square_to_plane` (MK12's), and the end-direction fit of `end_directions`
//! (direction held, reach free: FT3's, which needs the fitter and the certificate together).
//! Nothing here certifies: a fit's distance from its source is FT1's and FT3's.

mod band;
mod ends;
mod error;
mod interp;
mod knots;
mod lsq;
mod param;

pub use ends::{End, MAX_END_ORDER, Spline};
pub use error::FitError;
pub use interp::{interpolate, interpolate_on};
pub use knots::{approximation, averaging};
pub use lsq::{least_squares, weighted_least_squares};
pub use param::{Param, parameters};

#[cfg(test)]
mod tests;
