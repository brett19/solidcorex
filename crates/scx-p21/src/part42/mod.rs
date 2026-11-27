// Written for solidcorex.
//! ISO 10303-42 ("Part 42") geometry: the curves and surfaces of a STEP file as plain data
//! ([`geom`]), read from an exchange structure ([`read`]) and written into one ([`write`]).
//!
//! Lengths are millimetres and angles radians once read; nothing is evaluated, normalised or
//! approximated, so `read(write(g)) == g` bit for bit for geometry in those units. Building
//! curves and surfaces from this data, with the checks that need a geometric quantity, is the
//! kernel's.

pub mod geom;
pub mod read;
pub mod write;
