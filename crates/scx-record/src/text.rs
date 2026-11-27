// Written for solidcorex. Apache-2.0.
//! The text encoding (63 RE4): the same fields as the binary, one word each.
//!
//! - Words are separated by one space, or by one newline where the record asks for a line
//!   ([`Sink::line`]); the text ends with one newline.
//! - Integers are decimal, with no sign and no leading zero.
//! - An `f64` is Rust's shortest round-trip `Display` (no exponent), read back by
//!   `str::parse`. Both are correctly rounded, so the text keeps the bits; `-0` stays
//!   negative.
//! - A tag is its variant's word ([`Tag::name`]); a magic is its four letters.
//!
//! The reader takes only that spelling: a sign, a leading zero, a longer digit string for
//! the same `f64`, a second space or a missing newline is
//! [`BadToken`](ReadCause::BadToken). So each value has one text, `write(read(t)) == t` for
//! every `t` that reads, and binary → text → binary gives the same bytes.

use core::fmt::{self, Write as _};

use crate::error::{ReadCause, ReadError, WriteCause, WriteError};
use crate::tag::Tag;
use crate::wire::{Magic, Sink, Source};

/// Writes a record's text.
#[derive(Debug, Clone)]
pub struct TextWriter {
    out: String,
    sep: char,
}

impl Default for TextWriter {
    fn default() -> Self {
        Self::new()
    }
}

impl TextWriter {
    /// An empty writer.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            out: String::new(),
            sep: ' ',
        }
    }

    /// The text written, ended by its newline.
    #[must_use]
    pub fn finish(mut self) -> String {
        self.out.push('\n');
        self.out
    }

    /// Start a word: its separator, unless it is the first.
    #[inline]
    fn start(&mut self) {
        if !self.out.is_empty() {
            self.out.push(self.sep);
        }
        self.sep = ' ';
    }

    /// Write a word formatted by `Display`.
    #[inline]
    fn display(&mut self, v: impl fmt::Display) {
        self.start();
        // Writing to a `String` never fails.
        let _ = write!(self.out, "{v}");
    }
}

impl Sink for TextWriter {
    fn at(&self) -> usize {
        self.out.len()
    }

    fn magic(&mut self, magic: Magic) {
        self.start();
        self.out.push_str(magic.word());
    }

    fn u8(&mut self, v: u8) {
        self.display(v);
    }

    fn u16(&mut self, v: u16) {
        self.display(v);
    }

    fn u32(&mut self, v: u32) {
        self.display(v);
    }

    fn u64(&mut self, v: u64) {
        self.display(v);
    }

    fn f64(&mut self, v: f64) -> Result<(), WriteError> {
        if !v.is_finite() {
            return Err(WriteError::new(self.at(), WriteCause::NonFinite));
        }
        self.display(v);
        Ok(())
    }

    fn tag<T: Tag>(&mut self, t: T) {
        self.start();
        self.out.push_str(t.name());
    }

    fn line(&mut self) {
        self.sep = '\n';
    }
}

/// Reads a record from its text.
#[derive(Debug, Clone)]
pub struct TextReader<'a> {
    text: &'a str,
    pos: usize,
    sep: u8,
}

impl<'a> TextReader<'a> {
    /// A reader at the start of `text`.
    #[must_use]
    pub const fn new(text: &'a str) -> Self {
        Self {
            text,
            pos: 0,
            sep: b' ',
        }
    }

    /// The next word and its offset: past its separator (unless it is the first), up to the
    /// next space or newline. An empty word is [`ReadCause::BadToken`] for `expected`.
    fn word(&mut self, expected: &'static str) -> Result<(usize, &'a str), ReadError> {
        let bytes = self.text.as_bytes();
        let sep = core::mem::replace(&mut self.sep, b' ');
        if self.pos > 0 {
            match bytes.get(self.pos) {
                None => return Err(ReadError::new(self.pos, ReadCause::Truncated)),
                Some(&b) if b == sep => self.pos += 1,
                Some(_) => {
                    return Err(ReadError::new(
                        self.pos,
                        ReadCause::BadToken {
                            expected: "separator",
                        },
                    ));
                }
            }
        }
        let start = self.pos;
        let rest = bytes.get(start..).unwrap_or_default();
        let len = rest
            .iter()
            .position(|&b| b == b' ' || b == b'\n')
            .unwrap_or(rest.len());
        if len == 0 {
            let cause = if rest.is_empty() {
                ReadCause::Truncated
            } else {
                ReadCause::BadToken { expected }
            };
            return Err(ReadError::new(start, cause));
        }
        self.pos = start + len;
        // The ends are at the text's ends or next to an ASCII byte, so on char boundaries.
        let word = self.text.get(start..self.pos).unwrap_or_default();
        Ok((start, word))
    }

    /// The next word as an unsigned integer: decimal digits, no leading zero.
    fn unsigned<T: core::str::FromStr>(&mut self, expected: &'static str) -> Result<T, ReadError> {
        let (at, word) = self.word(expected)?;
        let canonical =
            word.bytes().all(|b| b.is_ascii_digit()) && (word.len() == 1 || !word.starts_with('0'));
        let bad = ReadError::new(at, ReadCause::BadToken { expected });
        if !canonical {
            return Err(bad);
        }
        word.parse().map_err(|_| bad)
    }
}

/// Compares what is written to it with a text, without storing it.
struct Same<'a>(&'a [u8]);

impl fmt::Write for Same<'_> {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        let rest = self.0.strip_prefix(s.as_bytes()).ok_or(fmt::Error)?;
        self.0 = rest;
        Ok(())
    }
}

impl Source for TextReader<'_> {
    /// The offset of the next word: past its separator.
    fn at(&self) -> usize {
        if self.pos > 0 { self.pos + 1 } else { 0 }
    }

    fn remaining(&self) -> usize {
        self.text.len().saturating_sub(self.pos)
    }

    fn magic(&mut self, magic: Magic) -> Result<(), ReadError> {
        let (at, word) = self.word("magic")?;
        if word == magic.word() {
            Ok(())
        } else {
            Err(ReadError::new(at, ReadCause::BadMagic))
        }
    }

    fn u8(&mut self) -> Result<u8, ReadError> {
        self.unsigned("u8")
    }

    fn u16(&mut self) -> Result<u16, ReadError> {
        self.unsigned("u16")
    }

    fn u32(&mut self) -> Result<u32, ReadError> {
        self.unsigned("u32")
    }

    fn u64(&mut self) -> Result<u64, ReadError> {
        self.unsigned("u64")
    }

    fn f64(&mut self) -> Result<f64, ReadError> {
        let (at, word) = self.word("f64")?;
        let bad = ReadError::new(at, ReadCause::BadToken { expected: "f64" });
        let v: f64 = word.parse().map_err(|_| bad.clone())?;
        if !v.is_finite() {
            return Err(ReadError::new(at, ReadCause::NonFinite));
        }
        // Only the shortest spelling: `1.0`, `+1`, `1e0` and `01` all parse as 1.
        let mut same = Same(word.as_bytes());
        if write!(same, "{v}").is_err() || !same.0.is_empty() {
            return Err(bad);
        }
        Ok(v)
    }

    fn tag<T: Tag>(&mut self) -> Result<T, ReadError> {
        let (at, word) = self.word(T::RECORD)?;
        T::from_name(word).ok_or_else(|| {
            ReadError::new(
                at,
                ReadCause::BadToken {
                    expected: T::RECORD,
                },
            )
        })
    }

    fn line(&mut self) {
        self.sep = b'\n';
    }

    fn finish(self) -> Result<(), ReadError> {
        match self.text.as_bytes().get(self.pos..) {
            Some(b"\n") => Ok(()),
            Some([]) | None => Err(ReadError::new(self.pos, ReadCause::Truncated)),
            Some([b'\n', ..]) => Err(ReadError::new(self.pos + 1, ReadCause::Trailing)),
            Some(_) => Err(ReadError::new(self.pos, ReadCause::Trailing)),
        }
    }
}
