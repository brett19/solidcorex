// Written for solidcorex. Apache-2.0.
//! The truth references the geometry layers are validated against (docs/01-architecture.md
//! §9.2, docs/02-testing.md §2-5; design note docs/research/59-truth-references.md).
//! A dev-dependency only, never published (layers.txt `[testkit]`).
//!
//! Everything here works on plain data (`f64` arrays, knots and weights as slices) and
//! depends on no geometry type, so a library and its reference never share a bug.
//!
//! - [`exact`], [`rat`]: exact integers and rationals, with a correctly rounded `f64`.
//! - [`nurbs`]: exact B-spline and NURBS curves and surfaces: evaluation with any
//!   derivatives by two independent methods (de Boor and the basis-function sum), knot
//!   insertion, degree elevation and splitting.
//! - [`dd`]: double-double arithmetic and elementary functions.
//! - [`analytic`]: lines, circles, ellipses, planes, cylinders, cones, spheres and tori in
//!   double-double (exact for the rational cases), with closest points.
//! - [`quadric`]: the analytic surfaces as exact implicit equations, an exact on-surface
//!   decision for rational surfaces, and the NURB-to-STEP uv map in closed form.
//! - [`gen`]: seeded generators of well- and ill-conditioned inputs.

pub mod analytic;
pub mod data;
pub mod dd;
pub mod exact;
pub mod gen;
pub mod nurbs;
pub mod quadric;
pub mod rat;

pub use rat::Rat;

/// The partial derivatives `d^(i+j) / du^i dv^j` of a surface at a point, for `i + j` up
/// to an order.
#[derive(Clone, Debug, PartialEq)]
pub struct Partials<T> {
    /// `rows[i][j]`, `j <= order - i`.
    rows: Vec<Vec<T>>,
}

impl<T> Partials<T> {
    /// From `rows[i][j]` for `d^(i+j) / du^i dv^j`.
    pub(crate) fn new(rows: Vec<Vec<T>>) -> Partials<T> {
        Partials { rows }
    }

    /// The highest order held.
    pub fn order(&self) -> usize {
        self.rows.len().saturating_sub(1)
    }

    /// `d^(i+j) / du^i dv^j`; `None` past the order.
    pub fn get(&self, i: usize, j: usize) -> Option<&T> {
        self.rows.get(i)?.get(j)
    }

    /// Each partial with its `(i, j)`, `i` major.
    pub fn iter(&self) -> impl Iterator<Item = ((usize, usize), &T)> {
        self.rows
            .iter()
            .enumerate()
            .flat_map(|(i, r)| r.iter().enumerate().map(move |(j, t)| ((i, j), t)))
    }

    /// Each partial mapped.
    pub fn map<U>(&self, mut f: impl FnMut(&T) -> U) -> Partials<U> {
        Partials {
            rows: self
                .rows
                .iter()
                .map(|r| r.iter().map(&mut f).collect())
                .collect(),
        }
    }
}
