// Written for solidcorex. Apache-2.0.
//! Layer 0 of solidcorex: plain numbers. Constants, 2D and 3D vectors, small and
//! general matrices, the two transforms, 1D/2D/3D and periodic extents, rounding
//! bounds, the scalar tolerance functions, and interval arithmetic with outward
//! rounding (intervals, interval vectors, Taylor models and the certificate's series,
//! [`interval`], [`interval_vector`], [`taylor_model`], [`series`], [`ring`]). Depends on
//! nothing.
//!
//! Every module but [`consts`], [`units`] and [`error`] is moved from solidcore at
//! commit `03e16c9ee766de5b9b1ced845e5149d62ff493b8` with its results unchanged bit for
//! bit (`docs/research/52-num-move.md`; the interval modules, `53-interval-and-algo.md`).

pub mod certified;
pub mod consts;
pub mod error;
pub mod extent1d;
pub mod extent2d;
pub mod extent3d;
pub mod interval;
pub mod interval_vector;
pub mod limits;
pub mod math;
pub mod matrix;
pub mod periodic_extent1d;
pub mod ring;
pub mod series;
pub mod small_matrix;
pub mod taylor_model;
pub mod tol;
pub mod transform;
pub mod units;
pub mod vector2d;
pub mod vector3d;

pub use error::{NumError, NumFailure, Result};
pub use tol::Tolerances;
pub use units::{Accuracy, AngleTol, Length, ParamTol, Resolution};
