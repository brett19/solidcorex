// Written for solidcorex. Apache-2.0.
//! solidcorex layer 1: generic numerical algorithms over `scx-num` (closed-form polynomial
//! roots, the local 1-D and N-D solvers, the box tree, the executor). Nothing here names a
//! curve, surface, body or the kernel: the solvers are generic over their evaluator's error
//! ([`error::EvalError`]).

pub mod box_tree;
pub mod error;
pub mod executor;
pub mod hash_table;
pub mod krawczyk;
pub mod local_solve1d;
pub mod local_solve_nd;
pub mod polynomial;
pub mod solution;
pub mod split_tree;
