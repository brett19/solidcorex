// Written for solidcorex. Apache-2.0.
//! The binary encoding: fixed-width little-endian integers, `f64` as its IEEE bits, a tag
//! as one byte, a magic as its four bytes (63 §7.2).
//!
//! Every byte string either reads as exactly one value or is refused, so a value's bytes
//! are its only spelling: `encode(decode(x)) == x` for every `x` that decodes.
//!
//! The pin's reader (`topology/database_io.rs`, READ for its lessons only) emulates an
//! iostream, with sticky state bits to test and `clear()`, and `Option` per extracted field;
//! here each read returns `Result` and is chained with `?`.

use crate::error::{ReadCause, ReadError, WriteCause, WriteError};
use crate::tag::Tag;
use crate::wire::{Magic, Sink, Source};

/// Writes a record's bytes.
#[derive(Debug, Clone, Default)]
pub struct Writer {
    out: Vec<u8>,
}

impl Writer {
    /// An empty writer.
    #[must_use]
    pub const fn new() -> Self {
        Self { out: Vec::new() }
    }

    /// The bytes written.
    #[must_use]
    pub fn finish(self) -> Vec<u8> {
        self.out
    }
}

impl Sink for Writer {
    fn at(&self) -> usize {
        self.out.len()
    }

    fn magic(&mut self, magic: Magic) {
        self.out.extend_from_slice(&magic.bytes());
    }

    fn u8(&mut self, v: u8) {
        self.out.push(v);
    }

    fn u16(&mut self, v: u16) {
        self.out.extend_from_slice(&v.to_le_bytes());
    }

    fn u32(&mut self, v: u32) {
        self.out.extend_from_slice(&v.to_le_bytes());
    }

    fn u64(&mut self, v: u64) {
        self.out.extend_from_slice(&v.to_le_bytes());
    }

    fn f64(&mut self, v: f64) -> Result<(), WriteError> {
        if !v.is_finite() {
            return Err(WriteError::new(self.at(), WriteCause::NonFinite));
        }
        self.out.extend_from_slice(&v.to_bits().to_le_bytes());
        Ok(())
    }

    fn tag<T: Tag>(&mut self, t: T) {
        self.out.push(t.code());
    }

    fn line(&mut self) {}
}

/// Reads a record from its bytes.
#[derive(Debug, Clone)]
pub struct Reader<'a> {
    bytes: &'a [u8],
    pos: usize,
}

impl<'a> Reader<'a> {
    /// A reader at the start of `bytes`.
    #[must_use]
    pub const fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, pos: 0 }
    }

    /// The next `N` bytes, or [`ReadCause::Truncated`] at the field's start.
    #[inline]
    fn take<const N: usize>(&mut self) -> Result<[u8; N], ReadError> {
        let chunk = self
            .bytes
            .get(self.pos..)
            .and_then(<[u8]>::first_chunk::<N>)
            .ok_or_else(|| ReadError::new(self.pos, ReadCause::Truncated))?;
        self.pos += N;
        Ok(*chunk)
    }
}

impl Source for Reader<'_> {
    fn at(&self) -> usize {
        self.pos
    }

    fn remaining(&self) -> usize {
        self.bytes.len().saturating_sub(self.pos)
    }

    fn magic(&mut self, magic: Magic) -> Result<(), ReadError> {
        let at = self.pos;
        if self.take::<4>()? == magic.bytes() {
            Ok(())
        } else {
            Err(ReadError::new(at, ReadCause::BadMagic))
        }
    }

    fn u8(&mut self) -> Result<u8, ReadError> {
        let [b] = self.take::<1>()?;
        Ok(b)
    }

    fn u16(&mut self) -> Result<u16, ReadError> {
        self.take().map(u16::from_le_bytes)
    }

    fn u32(&mut self) -> Result<u32, ReadError> {
        self.take().map(u32::from_le_bytes)
    }

    fn u64(&mut self) -> Result<u64, ReadError> {
        self.take().map(u64::from_le_bytes)
    }

    fn f64(&mut self) -> Result<f64, ReadError> {
        let at = self.pos;
        let v = f64::from_bits(u64::from_le_bytes(self.take()?));
        if v.is_finite() {
            Ok(v)
        } else {
            Err(ReadError::new(at, ReadCause::NonFinite))
        }
    }

    fn tag<T: Tag>(&mut self) -> Result<T, ReadError> {
        let at = self.pos;
        let [found] = self.take::<1>()?;
        T::from_code(found).ok_or_else(|| {
            ReadError::new(
                at,
                ReadCause::BadTag {
                    record: T::RECORD,
                    found,
                },
            )
        })
    }

    fn line(&mut self) {}

    fn finish(self) -> Result<(), ReadError> {
        if self.pos == self.bytes.len() {
            Ok(())
        } else {
            Err(ReadError::new(self.pos, ReadCause::Trailing))
        }
    }
}
