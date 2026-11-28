// Written for solidcorex. Apache-2.0.
//! Recognition of what a body is: a uniform prism and its section, and the bores of a body and
//! the stacks they make across bodies (research 74 §2.3, §33, BW6).
//!
//! Every answer here is read from the body's own faces and edges. A failure is an error with a
//! cause, never "not a prism" or "no bores"; a length is the body's coincidence, never a fixed
//! number of millimetres.

mod bores;
mod piece;
mod prism;
mod stacks;

pub use bores::{Bore, Slot, bores};
pub use piece::Piece;
pub use prism::{
    Equivalence, NotPrism, PrismSection, Recognition, Why, prism_equivalent, prism_section,
    prism_section_charged,
};
pub use stacks::{Layer, Lone, Options, Stack, Stacks, hole_stacks, stack_bores};
