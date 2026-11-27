// Written for solidcorex. Apache-2.0.
//! Layer 0 of solidcorex: plain numbers. Constants, 2D and 3D vectors, small and
//! general matrices, the two transforms, 1D/2D/3D and periodic extents, rounding
//! bounds, the scalar tolerance functions, and interval arithmetic with outward
//! rounding (intervals, interval vectors, Taylor models and the certificate's series,
//! [`interval`], [`taylor_model`], [`series`], [`ring`]; an interval vector is a `Vec3<Ivl>`), the
//! certificate types ([`cert`]), the one fixed hash and random generator ([`hash`]), and the
//! error-free transformations ([`eft`]) and the transcendental functions on them
//! ([`fmath`]).
//! Depends on nothing.
//!
//! Every module but [`cert`], [`consts`], [`eft`], [`fmath`], [`hash`], [`units`],
//! [`error`] and the crate's own `expansion` (exact sums and products) is moved from
//! solidcore at commit `03e16c9ee766de5b9b1ced845e5149d62ff493b8` with its results
//! unchanged bit for bit (`docs/research/52-num-move.md`; the interval modules,
//! `53-interval-and-algo.md`), except that the transcendental functions are [`fmath`]'s in
//! place of the C library's, which may differ in the last bit, and that
//! [`math::three_point_circle_2d`] is computed exactly and rounded once, where the pin's
//! is not.

pub mod cert;
pub mod certified;
pub mod consts;
pub mod domain;
pub mod eft;
pub mod error;
pub(crate) mod expansion;
pub mod extent1d;
pub mod extent2d;
pub mod extent3d;
pub mod fmath;
pub mod hash;
pub mod interval;
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

pub use cert::{Basis, Cert, Claim, EdgeEnd, Method, Reads, Recorded};
pub use error::{NumError, NumFailure, Result};
pub use hash::{mix, Rng};
pub use units::{coincident, Accuracy, Length, ParamTol, Resolution};
