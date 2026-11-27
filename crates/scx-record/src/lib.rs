// Written for solidcorex. Apache-2.0.
//! solidcorex layer 6: the record format (research 63 §7), bytes and text, no files.
//!
//! This is solidcorex's own format, versioned from 1 ([`FORMAT_VERSION`]). The pin's
//! records are read by `scx-import-sc`, not here (04, 11-27).
//!
//! # The primitives (63 RE1, RE4)
//!
//! A record writes itself once to a [`Sink`] and reads itself once from a [`Source`]. Each
//! has two encodings:
//!
//! - binary, [`Writer`] and [`Reader`]: little-endian fixed-width integers, `f64` as its
//!   IEEE bits, a tag as one byte;
//! - text, [`TextWriter`] and [`TextReader`]: one word per field, `f64` as Rust's shortest
//!   round-trip `Display`, a tag as its variant's word.
//!
//! Both keep every bit, and both are canonical: each value has one spelling in each, and any
//! other input is refused. So `encode(decode(x)) == x` for every `x` that decodes, and
//! binary → text → binary gives the same bytes.
//!
//! - A top-level record opens with a [`Magic`] and the version ([`Sink::header`]); a
//!   newer version is [`ReadCause::BadVersion`], kind `Unsupported`.
//! - An enum is a [`Tag`], mapped by exhaustive matches; an unknown tag is refused.
//! - A list is its length, then its items ([`Sink::list`]).
//! - A table by id keeps its ids (01 §2.2): live records, and runs of deleted slots as
//!   counts ([`Sink::slots`]).
//! - A NaN or an infinity is refused on both sides (63 D1).
//!
//! # Geometry (63 RE2)
//!
//! [`Record`] writes and reads every curve and surface kind a body holds ([`scx_curve::Curve`],
//! [`scx_curve::Curve2`], [`scx_surface::Surface`], their NURBS kinds and the frames),
//! nested in a larger record. A read value is rebuilt by its type's checked constructor,
//! whose refusal is the cause ([`ReadCause::Frame`], [`ReadCause::Curve`],
//! [`ReadCause::Surface`]); every bit is kept, frames' axes and rational NURBS points
//! included.
//!
//! # Errors
//!
//! A read fails with [`ReadError`], the byte offset and a typed, kinded [`ReadCause`]
//! (63 X13). There is no sticky state: each read returns `Result`, chained with `?`.
//! Refusing heals nothing (01 §1.1).

mod binary;
mod error;
mod geom;
mod tag;
mod text;
mod wire;

pub use binary::{Reader, Writer};
pub use error::{ReadCause, ReadError, WriteCause, WriteError};
pub use geom::Record;
pub use tag::Tag;
pub use text::{TextReader, TextWriter};
pub use wire::{FORMAT_VERSION, Magic, Sink, Source};
