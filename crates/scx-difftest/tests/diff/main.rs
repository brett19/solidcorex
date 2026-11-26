// Written for solidcorex. Apache-2.0.
//! Differential tests: every public scx-num function against its counterpart in the
//! pinned solidcore, on the same inputs, compared bit for bit (`f64::to_bits`).
//! Each test prints `difftest <name>: <n> inputs, <m> mismatches`.

#![allow(missing_docs, clippy::pedantic)]

mod facade;
mod harness;
mod t_extent;
mod t_math;
mod t_rest;
mod t_tol;
mod t_vector;
