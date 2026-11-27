// Written for solidcorex. Apache-2.0.
//! Plain B-spline and NURBS data: what the generators produce and a library under test
//! takes. [`CurveData::exact`] and [`SurfaceData::exact`] give the exact reference.

use crate::nurbs::{Curve, Error, Surface};

/// A B-spline or NURBS curve as plain doubles.
#[derive(Clone, Debug, PartialEq)]
pub struct CurveData<const D: usize> {
    /// The degree `p`.
    pub degree: usize,
    /// `n + p + 1` nondecreasing knots.
    pub knots: Vec<f64>,
    /// The `n` control points.
    pub points: Vec<[f64; D]>,
    /// The weights (`None`: all 1).
    pub weights: Option<Vec<f64>>,
}

impl<const D: usize> CurveData<D> {
    /// The exact curve these doubles describe.
    pub fn exact(&self) -> Result<Curve<D>, Error> {
        Curve::new(
            self.degree,
            &self.knots,
            &self.points,
            self.weights.as_deref(),
        )
    }
}

/// A B-spline or NURBS surface as plain doubles, control points row-major (`i * nv + j`).
#[derive(Clone, Debug, PartialEq)]
pub struct SurfaceData<const D: usize> {
    /// `(pu, pv)`.
    pub degrees: (usize, usize),
    /// The knots along `u`.
    pub knots_u: Vec<f64>,
    /// The knots along `v`.
    pub knots_v: Vec<f64>,
    /// The `nu * nv` control points.
    pub points: Vec<[f64; D]>,
    /// The weights (`None`: all 1).
    pub weights: Option<Vec<f64>>,
}

impl<const D: usize> SurfaceData<D> {
    /// The exact surface these doubles describe.
    pub fn exact(&self) -> Result<Surface<D>, Error> {
        Surface::new(
            self.degrees,
            &self.knots_u,
            &self.knots_v,
            &self.points,
            self.weights.as_deref(),
        )
    }
}
