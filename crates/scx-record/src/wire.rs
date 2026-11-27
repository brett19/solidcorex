// Written for solidcorex. Apache-2.0.
//! The two traits every record is written and read through, once for both encodings.
//!
//! A record type writes itself to any [`Sink`] and reads itself from any [`Source`], so its
//! field order is stated once and the binary and text encodings cannot drift apart. The
//! primitives are the methods; tags, flags, lists, tables and the header are provided on
//! top of them, the same for both encodings.
//!
//! Every list entry and table entry takes at least one byte on the wire (a record is never
//! empty), so a count larger than the input left is [`Truncated`](ReadCause::Truncated)
//! before anything is allocated.

use crate::error::{ReadCause, ReadError, WriteCause, WriteError};
use crate::tag::{SlotKind, Tag};

/// The format version this crate writes, and the newest it reads (63 §7.2). Adding a record
/// kind or a field is a new version (01 §7.4). Decoding that depends on the version is a
/// `match` on [`Source::header`]'s answer at the record boundary, never threaded through
/// field reads.
pub const FORMAT_VERSION: u16 = 1;

/// The four bytes that open a top-level record and say which record it is: ASCII letters
/// and digits, so the text encoding writes them as a word.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Magic(pub(crate) [u8; 4]);

impl Magic {
    /// The magic `bytes`, if all four are ASCII letters or digits.
    #[must_use]
    pub const fn new(bytes: [u8; 4]) -> Option<Self> {
        let [a, b, c, d] = bytes;
        if a.is_ascii_alphanumeric()
            && b.is_ascii_alphanumeric()
            && c.is_ascii_alphanumeric()
            && d.is_ascii_alphanumeric()
        {
            Some(Self(bytes))
        } else {
            None
        }
    }

    /// Its four bytes.
    #[must_use]
    pub const fn bytes(self) -> [u8; 4] {
        self.0
    }

    /// Its word: the four bytes as text.
    #[must_use]
    pub fn word(&self) -> &str {
        // Four ASCII letters or digits, checked by `new`.
        core::str::from_utf8(&self.0).unwrap_or_default()
    }
}

/// Where a record writes itself: the binary [`Writer`](crate::Writer) or the
/// [`TextWriter`](crate::TextWriter).
pub trait Sink {
    /// The output's length so far, in bytes.
    fn at(&self) -> usize;
    /// Write a magic.
    fn magic(&mut self, magic: Magic);
    /// Write a byte.
    fn u8(&mut self, v: u8);
    /// Write a `u16`.
    fn u16(&mut self, v: u16);
    /// Write a `u32`.
    fn u32(&mut self, v: u32);
    /// Write a `u64`.
    fn u64(&mut self, v: u64);
    /// Write a finite `f64`, exactly. A NaN or an infinity is refused
    /// ([`WriteCause::NonFinite`]) and nothing is written.
    fn f64(&mut self, v: f64) -> Result<(), WriteError>;
    /// Write an enum's tag.
    fn tag<T: Tag>(&mut self, t: T);
    /// Start the next field on a new line. Layout only: binary writes nothing, and the
    /// reader states the same breaks with [`Source::line`].
    fn line(&mut self);

    /// Write a top-level record's header: `magic` and [`FORMAT_VERSION`].
    fn header(&mut self, magic: Magic) {
        self.magic(magic);
        self.u16(FORMAT_VERSION);
    }

    /// Write a flag.
    fn bool(&mut self, v: bool) {
        self.tag(v);
    }

    /// Write a count: a `u32`, refused over `u32::MAX` ([`WriteCause::TooLong`]).
    fn count(&mut self, len: usize) -> Result<(), WriteError> {
        let n = u32::try_from(len)
            .map_err(|_| WriteError::new(self.at(), WriteCause::TooLong { len }))?;
        self.u32(n);
        Ok(())
    }

    /// Write a list: its length, then each item through `put`.
    fn list<T>(
        &mut self,
        items: &[T],
        mut put: impl FnMut(&mut Self, &T) -> Result<(), WriteError>,
    ) -> Result<(), WriteError> {
        self.count(items.len())?;
        for item in items {
            put(self, item)?;
        }
        Ok(())
    }

    /// Write a table by id (01 §2.2): its length (the next id), then each live record
    /// through `put` and each run of deleted slots as one count. Each entry starts a line.
    fn slots<'r, R: 'r, I>(
        &mut self,
        slots: I,
        mut put: impl FnMut(&mut Self, &R) -> Result<(), WriteError>,
    ) -> Result<(), WriteError>
    where
        I: IntoIterator<Item = Option<&'r R>>,
        I::IntoIter: ExactSizeIterator,
    {
        let slots = slots.into_iter();
        let len = slots.len();
        let total = u32::try_from(len)
            .map_err(|_| WriteError::new(self.at(), WriteCause::TooLong { len }))?;
        self.u32(total);
        let mut run = 0u32;
        for slot in slots {
            match slot {
                None => run += 1,
                Some(record) => {
                    if run > 0 {
                        gap(self, run);
                        run = 0;
                    }
                    self.line();
                    self.tag(SlotKind::Live);
                    put(self, record)?;
                }
            }
        }
        if run > 0 {
            gap(self, run);
        }
        Ok(())
    }
}

/// A run of `run` deleted slots.
fn gap<S: Sink + ?Sized>(sink: &mut S, run: u32) {
    sink.line();
    sink.tag(SlotKind::Gap);
    sink.u32(run);
}

/// Where a record reads itself from: the binary [`Reader`](crate::Reader) or the
/// [`TextReader`](crate::TextReader).
///
/// Every method either reads its field and moves past it, or fails with the offset of the
/// field. Nothing is sticky: after a failure the error is the answer and the source is
/// dropped.
pub trait Source {
    /// The offset of the next field, in bytes.
    fn at(&self) -> usize;
    /// The bytes left.
    fn remaining(&self) -> usize;
    /// Read `magic`, refusing anything else ([`ReadCause::BadMagic`]).
    fn magic(&mut self, magic: Magic) -> Result<(), ReadError>;
    /// Read a byte.
    fn u8(&mut self) -> Result<u8, ReadError>;
    /// Read a `u16`.
    fn u16(&mut self) -> Result<u16, ReadError>;
    /// Read a `u32`.
    fn u32(&mut self) -> Result<u32, ReadError>;
    /// Read a `u64`.
    fn u64(&mut self) -> Result<u64, ReadError>;
    /// Read an `f64`, exactly, refusing a NaN or an infinity ([`ReadCause::NonFinite`]).
    fn f64(&mut self) -> Result<f64, ReadError>;
    /// Read an enum's tag, refusing one that names no variant.
    fn tag<T: Tag>(&mut self) -> Result<T, ReadError>;
    /// The next field starts on a new line, as the writer's [`Sink::line`].
    fn line(&mut self);
    /// The record is over: refuse anything after it ([`ReadCause::Trailing`]).
    fn finish(self) -> Result<(), ReadError>;

    /// Read a top-level record's header: `magic`, then the version, refused when it is 0
    /// or newer than [`FORMAT_VERSION`]. Returns the version, for the record's `match`.
    fn header(&mut self, magic: Magic) -> Result<u16, ReadError> {
        self.magic(magic)?;
        let at = self.at();
        match self.u16()? {
            found @ 1..=FORMAT_VERSION => Ok(found),
            found => Err(ReadError::new(at, ReadCause::BadVersion { found })),
        }
    }

    /// Read a flag.
    fn bool(&mut self) -> Result<bool, ReadError> {
        self.tag()
    }

    /// Read a count of entries that each take at least one byte: refused as
    /// [`ReadCause::Truncated`] when the input has fewer bytes left.
    fn count(&mut self) -> Result<usize, ReadError> {
        let at = self.at();
        let n = self.u32()? as usize;
        if n > self.remaining() {
            return Err(ReadError::new(at, ReadCause::Truncated));
        }
        Ok(n)
    }

    /// Read a list: its length, then each item through `take`.
    fn list<T>(
        &mut self,
        mut take: impl FnMut(&mut Self) -> Result<T, ReadError>,
    ) -> Result<Vec<T>, ReadError> {
        let n = self.count()?;
        let mut out = Vec::with_capacity(n);
        for _ in 0..n {
            out.push(take(self)?);
        }
        Ok(out)
    }

    /// Read a table by id (01 §2.2): its length, then live records through `take` and
    /// runs of deleted slots, each run at least 1, never two in a row, and none past the
    /// table's end ([`ReadCause::BadRun`]). A table longer than `limit` slots is refused
    /// before anything is allocated ([`ReadCause::TooMany`]): a run costs a few bytes here
    /// and a slot each in memory.
    fn slots<R>(
        &mut self,
        limit: u32,
        mut take: impl FnMut(&mut Self) -> Result<R, ReadError>,
    ) -> Result<Vec<Option<R>>, ReadError> {
        let at = self.at();
        let count = self.u32()?;
        if count > limit {
            return Err(ReadError::new(at, ReadCause::TooMany { count, limit }));
        }
        let total = count as usize;
        let mut out = Vec::with_capacity(total.min(self.remaining()));
        let mut after_gap = false;
        while out.len() < total {
            self.line();
            let at = self.at();
            match self.tag()? {
                SlotKind::Live => {
                    out.push(Some(take(self)?));
                    after_gap = false;
                }
                SlotKind::Gap => {
                    let run = self.u32()? as usize;
                    if run == 0 || after_gap || run > total - out.len() {
                        return Err(ReadError::new(at, ReadCause::BadRun));
                    }
                    out.resize_with(out.len() + run, || None);
                    after_gap = true;
                }
            }
        }
        Ok(out)
    }
}
