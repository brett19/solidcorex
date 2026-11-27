// Written for solidcorex (docs/research/58-p21.md).
//! solidcorex layer 1: STEP's exchange structure (ISO 10303-21, "Part 21") read and
//! written, the geometry entities of ISO 10303-42 as plain data, units (ISO 10303-41) and
//! product structure.
//!
//! Nothing here builds a body: mapping STEP to solids and sheets is `scx-step`'s. This crate
//! holds no state between calls; every read takes its [`Limits`] and [`Progress`] as
//! arguments.
//!
//! - [`parse`] reads text into an [`Exchange`]; [`write`] writes one back, so that
//!   `parse(write(x)) == x` for every exchange `write` accepts.
//! - [`Writer`] builds a DATA section instance by instance, sharing identical ones.
//! - [`units::context`] reads a representation context's units.
//!
//! Clause numbers (§) are those of ISO 10303-21:2016 (edition 3) unless another part is named.

pub mod error;
pub mod exchange;
pub mod parse;
pub mod protocol;
mod text;
pub mod units;
pub mod write;

pub use error::{P21Error, Refusal, Syntax, Warning};
pub use exchange::{
    DataSection, Exchange, FileDescription, FileName, Header, Instance, Param, Record,
};
pub use parse::{check_memory, parse, parse_with, Limits, Parsed, Progress, Unwatched};
pub use write::{write, write_into, Writer};
