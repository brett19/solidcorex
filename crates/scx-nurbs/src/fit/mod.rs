// Written for solidcorex. Apache-2.0.
//! Curve fitting at given parameters: parameterisations, knot vectors, interpolation with end
//! conditions, and least squares, on banded solves (FT2; 77 §2); and the certified refit of a
//! curve to a distance (FT3, [`fit_within`]).
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
//! | `approx_end_directions`, `EndDirectionFit` (`end_directions`) | [`least_squares_along`], [`Directed`] |
//! | `refit_within`, `WithinFit` (`within`) | [`fit_within`] on any [`CurveFn`], [`Fitted`] with a [`SpanBound`] per span |
//!
//! Not ported here: first derivatives at *every* point (`interp_first_derivs*`,
//! `interp_knots_and_tangents`: the surface fit's, CN3), the closed-curve conditions
//! (`approx_closed_conditions`), the `LU`-reuse solves (`cpts_with_matrix`,
//! `cpts_from_srf_data`; a banded factor is not kept between solves, a surface fit pushes its
//! rows again), `square_to_plane` (MK12's) and `interp_end_directions`. The fits at given
//! parameters do not certify; [`fit_within`] certifies every span of its fit (FT1's bound, or
//! SU6's branch and bound).

mod along;
mod band;
mod ends;
mod error;
mod interp;
mod knots;
mod lsq;
mod param;
mod within;

pub use along::{Directed, least_squares_along};
pub use ends::{End, MAX_END_ORDER, Spline};
pub use error::FitError;
pub use interp::{interpolate, interpolate_on};
pub use knots::{approximation, averaging};
pub use lsq::{least_squares, weighted_least_squares};
pub use param::{Param, parameters};
pub use within::{
    CLOSE, CurveFn, Ended, FLOOR_UNITS, FitAim, Fitted, Held, MAX_CONTROL_POINTS, MAX_PIECES,
    SpanBound, fit_within,
};

#[cfg(test)]
mod tests;
#[cfg(test)]
mod within_tests;
