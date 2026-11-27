// Written for solidcorex. Apache-2.0.
//! solidcorex layer 1: generic numerical algorithms over `scx-num` (Bernstein polynomials and
//! their roots, closed-form polynomial roots, the local 1-D and N-D solvers, the adaptive ODE
//! integrator and quadrature, the box tree, the executor). Nothing here names a curve,
//! surface, body or the kernel: the solvers are generic over their evaluator's error
//! ([`error::EvalError`]).

pub mod bernstein;
pub mod box_tree;
pub mod error;
pub mod executor;
pub mod geom;
pub mod global_solve;
pub mod hash_table;
pub mod krawczyk;
pub mod local_solve1d;
pub mod local_solve_nd;
pub mod ode;
pub mod point_set;
pub mod polynomial;
pub mod pseudo_box;
pub mod solution;
pub mod sort;
pub mod split_tree;
