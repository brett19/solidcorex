// Derived from solidcore `solidcore/crates/solidcore-record/src/step/p21.rs` at commit
// 03e16c9ee766de5b9b1ced845e5149d62ff493b8 (its own origin: SolidCraft's `crates/step`,
// MIT OR Apache-2.0).
// Upstream record (verbatim from that file): ISO 10303-21 ("Part 21") exchange structure
// parser: the HEADER and the DATA section's entity instances, simple (`#1=NAME(...)`) and
// complex (`#2=(A(...) B(...))`), with the string escapes decoded (`\X\hh`, `\X2\…\X0\`,
// `\X4\…\X0\`, `\S\c`, `\\`). Written from the public ISO 10303-21 specification.
// Changes: the parser only; the data model is `crate::exchange` and the text tokens
// `crate::text`. The limits and their arithmetic (`MAX_BYTES`, `BYTES_PER_PARAM`,
// `BYTES_PER_RECORD`, `MAX_COUNT`, `MAX_DEPTH`, `MAX_COMPLEX_PARTS`, `PEAK_PER_BYTE`,
// `Limits::for_bytes`, `Limits::at_density`, `Limits::for_text`, `check_memory`) are kept, the
// limits a public options value (`Limits`, `Limits::with_memory`) in place of `parse_with`,
// `parse_with_max_bytes` and `parse_with_budget`; the lexer's rules are kept: keywords read in
// upper case, the lenient numbers (`1.`, `1.E-3`, `e`), a number with no digit or an integer
// beyond i64 refused, line breaks in a string skipped, a malformed HEADER record skipped to the
// next line that starts with a keyword, the first definition of a repeated id kept, a leading
// byte order mark skipped, `Progress` asked every 4096 instances. Every fn is rewritten on the
// new model: `Lexer::line` counts on from the last count, forward or back, in a plain field (was a
// `Cell`, recounting from the start on a step back),
// `Lexer::ws`, `Lexer::eat`, `Lexer::expect`, `Lexer::keyword`, `Lexer::number`,
// `Lexer::string`, `Lexer::param`, `Lexer::list_rest`, `Lexer::record`,
// `Lexer::next_header_line`, `Lexer::fail`, `Lexer::peek`, `parse_limited` (now `parse_with`
// and `Lexer::header`, `Lexer::data`, `Lexer::raw_section`, `Lexer::section_keyword`,
// `Lexer::instance`, `Lexer::enumeration`, `Lexer::binary`, `Lexer::reference`, `Lexer::count`,
// `Lexer::warn_page`, `parse`). What is new: every HEADER record is kept, not only the three
// the pin read; each DATA section is kept with its parameters; a complex instance of one part
// stays complex; ANCHOR, REFERENCE and SIGNATURE sections are kept as text, their end found
// outside strings (the pin skipped to the first `ENDSEC;` in the raw text, even one inside a
// string); a typed parameter holds exactly one parameter (§5.5); a binary is checked (§5.4);
// an enumeration is letters, digits and `_` (the pin read to the next `.`); keywords other than
// the section and end keywords may not hold `-`; a complex instance needs a part (§5.5, the
// pin read `#1=();`); an id defined again in a later DATA section is reported; errors and
// warnings are typed.
// New helpers: `at_most_max_count`, `id_of`, `Lexer::peek_at`; `Limits::default` and
// `Unwatched`'s `read` and `stop` (the `Progress` that never stops) take the place of the pin's
// constant limits and `Option<&dyn Progress>`.
//! Reading an exchange structure (ISO 10303-21:2016 §5 to §11).
//!
//! Input is hostile: the text's size, the instance, record and parameter counts, the
//! nesting depth and the parts of a complex instance are capped ([`Limits`]); nothing is
//! allocated from a count read out of the file; every refusal is a [`P21Error`], never a
//! panic.
//!
//! Where the standard is strict and writers in use are not, the reader follows the writers:
//! keywords and enumerations in lower case read in upper case (§6.3 allows only capitals), a
//! real may omit the digits after its point or before it (`1.`, `.5`) or write its exponent
//! as `e` (§6.4.2), and an integer token beyond `i64` is refused rather than read as a real.

use std::collections::HashSet;

use scx_num::hash::FixedState;

use crate::error::{Counted, Limit, P21Error, Refusal, Syntax, Warning};
use crate::exchange::{DataSection, Exchange, Header, Instance, Param, Record};
use crate::text;

/// The longest text read by default, in bytes.
pub const MAX_BYTES: usize = 512 << 20;

/// The fewest bytes a parameter takes in a file (`1,`): the counts follow the text's size at
/// this density.
const BYTES_PER_PARAM: usize = 4;

/// The fewest bytes an instance or record takes in a file of real data.
const BYTES_PER_RECORD: usize = 16;

/// The largest count on any target: on 32-bit targets the default size limit would allow
/// counts whose storage does not fit in the address space.
const MAX_COUNT: usize = if cfg!(target_pointer_width = "32") {
    20_000_000
} else {
    usize::MAX
};

/// `n`, at most [`MAX_COUNT`] (a no-op on 64-bit targets).
const fn at_most_max_count(n: usize) -> usize {
    if n < MAX_COUNT {
        n
    } else {
        MAX_COUNT
    }
}

/// The deepest parameter nesting read.
const MAX_DEPTH: usize = 64;

/// The most parts of one complex instance.
const MAX_COMPLEX_PARTS: usize = 64;

/// The memory a parse takes at its peak, per byte of text, at most.
pub const PEAK_PER_BYTE: usize = 32;

/// How many instances are read between two questions to the [`Progress`].
const PROGRESS_EVERY: usize = 4096;

/// What a parse refuses beyond.
///
/// The default reads a text of up to [`MAX_BYTES`], with as many instances, records and
/// parameters as a text of that size holds. [`Limits::for_bytes`] raises them together.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct Limits {
    /// The longest text, in bytes.
    pub bytes: usize,
    /// The most instances (a repeated id counts again).
    pub instances: usize,
    /// The most records.
    pub records: usize,
    /// The most parameters, header and data, at every depth.
    pub params: usize,
    /// The memory the caller can spare for the reading, in bytes: a text whose reading would
    /// take more ([`PEAK_PER_BYTE`] times its length) is refused before anything is read.
    pub memory: usize,
}

impl Default for Limits {
    fn default() -> Self {
        Self::at_density(MAX_BYTES)
    }
}

impl Limits {
    /// The limits for a text of up to `max_bytes` (never below the defaults), the counts in
    /// proportion: a file of a real density is read whenever its size is.
    #[must_use]
    pub fn for_bytes(max_bytes: usize) -> Self {
        Self::at_density(max_bytes.max(MAX_BYTES))
    }

    /// These limits, refusing a text whose reading would take more than `memory` bytes.
    #[must_use]
    pub fn with_memory(self, memory: usize) -> Self {
        Self { memory, ..self }
    }

    /// The limits for a text of up to `bytes`, the counts at the densest a file can be.
    fn at_density(bytes: usize) -> Self {
        Self {
            bytes,
            instances: at_most_max_count(bytes / BYTES_PER_RECORD),
            records: at_most_max_count(bytes / BYTES_PER_RECORD),
            params: at_most_max_count(bytes / BYTES_PER_PARAM),
            memory: usize::MAX,
        }
    }

    /// These limits for a text of `len` bytes: the counts no more than a text of that length
    /// holds, so the memory a parse takes follows the text it reads.
    fn for_text(self, len: usize) -> Self {
        let own = Self::at_density(len);
        Self {
            instances: self.instances.min(own.instances),
            records: self.records.min(own.records),
            params: self.params.min(own.params),
            ..self
        }
    }
}

/// Whether reading a text of `len` bytes fits in `memory` bytes; [`Limit::NeedsMemory`],
/// naming [`PEAK_PER_BYTE`] times `len`, when it does not. A caller can ask before it reads
/// the file.
pub fn check_memory(len: usize, memory: usize) -> Result<(), P21Error> {
    let needed = len.saturating_mul(PEAK_PER_BYTE);
    if needed > memory {
        return Err(Limit::NeedsMemory {
            needed,
            available: memory,
        }
        .into());
    }
    Ok(())
}

/// Someone waiting for a long parse: told how far it is, and able to stop it. Asked every
/// 4096 instances; neither answer changes what is read.
pub trait Progress {
    /// Whether to stop at this check.
    fn stop(&self) -> bool;
    /// `done` of `total` bytes are read.
    fn read(&self, done: usize, total: usize);
}

/// A [`Progress`] that never stops and hears nothing.
#[derive(Clone, Copy, Debug, Default)]
pub struct Unwatched;

impl Progress for Unwatched {
    fn stop(&self) -> bool {
        false
    }
    fn read(&self, _done: usize, _total: usize) {}
}

/// A read exchange, with what the read passed over.
#[derive(Clone, Debug, PartialEq)]
pub struct Parsed {
    /// The exchange.
    pub exchange: Exchange,
    /// What was skipped or normalised, in the order met.
    pub warnings: Vec<Warning>,
}

/// Read `text` under the default [`Limits`].
pub fn parse(text: &str) -> Result<Parsed, P21Error> {
    parse_with(text, Limits::default(), &Unwatched)
}

/// Read `text` under `limits`, telling `progress` how far it is.
pub fn parse_with(text: &str, limits: Limits, progress: &dyn Progress) -> Result<Parsed, P21Error> {
    if text.len() > limits.bytes {
        return Err(Limit::TooLarge {
            bytes: text.len(),
            limit: limits.bytes,
        }
        .into());
    }
    check_memory(text.len(), limits.memory)?;
    let limits = limits.for_text(text.len());
    // Some writers start the file with a UTF-8 byte order mark.
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    let mut lx = Lexer {
        s: text.as_bytes(),
        i: 0,
        counted: (0, 0),
        limits,
        params: 0,
        records: 0,
        instances: 0,
        warnings: Vec::new(),
        progress,
    };
    let exchange = lx.exchange()?;
    Ok(Parsed {
        exchange,
        warnings: lx.warnings,
    })
}

struct Lexer<'a> {
    s: &'a [u8],
    i: usize,
    /// The last position a line was counted to and the newlines before it.
    counted: (usize, usize),
    limits: Limits,
    params: usize,
    records: usize,
    instances: usize,
    warnings: Vec<Warning>,
    progress: &'a dyn Progress,
}

/// An instance name's digits as an id.
fn id_of(digits: &[u8]) -> Option<u64> {
    core::str::from_utf8(digits).ok()?.parse().ok()
}

impl Lexer<'_> {
    /// The line of the current position, counted from 1. Counting goes on from the last
    /// count, forward or back, so it costs the distance between the two positions: the lines
    /// of a run of skipped header records, each asked at its error and again at its start,
    /// cost one pass in all.
    fn line(&mut self) -> usize {
        #[allow(clippy::naive_bytecount)] // std only
        fn newlines(b: &[u8]) -> usize {
            b.iter().filter(|c| **c == b'\n').count()
        }
        let (at, before) = self.counted;
        let n = if self.i >= at {
            before + newlines(self.s.get(at..self.i).unwrap_or(&[]))
        } else {
            before.saturating_sub(newlines(self.s.get(self.i..at).unwrap_or(&[])))
        };
        self.counted = (self.i, n);
        n + 1
    }

    fn fail<T>(&mut self, cause: Syntax) -> Result<T, P21Error> {
        Err(P21Error::syntax(self.line(), cause))
    }

    fn peek(&self) -> Option<u8> {
        self.s.get(self.i).copied()
    }

    fn peek_at(&self, ahead: usize) -> Option<u8> {
        self.s.get(self.i + ahead).copied()
    }

    /// Skip white space and comments (§5.6).
    fn ws(&mut self) -> Result<(), P21Error> {
        loop {
            match self.peek() {
                Some(c) if c.is_ascii_whitespace() => self.i += 1,
                Some(b'/') if self.peek_at(1) == Some(b'*') => {
                    let rest = self.s.get(self.i + 2..).unwrap_or(&[]);
                    match rest.windows(2).position(|w| w == b"*/") {
                        Some(p) => self.i += p + 4,
                        None => return self.fail(Syntax::UnterminatedComment),
                    }
                }
                _ => return Ok(()),
            }
        }
    }

    fn eat(&mut self, c: u8) -> Result<bool, P21Error> {
        self.ws()?;
        if self.peek() == Some(c) {
            self.i += 1;
            Ok(true)
        } else {
            Ok(false)
        }
    }

    fn expect(&mut self, c: u8) -> Result<(), P21Error> {
        if self.eat(c)? {
            Ok(())
        } else {
            self.fail(Syntax::Expected(char::from(c)))
        }
    }

    /// A keyword (§6.3), upper case: an entity or type name, `!` first for a user-defined one.
    fn keyword(&mut self) -> Result<String, P21Error> {
        self.ws()?;
        let st = self.i;
        if self.peek() == Some(b'!') {
            self.i += 1;
        }
        let first = self.i;
        if self
            .peek()
            .is_some_and(|c| c.is_ascii_alphabetic() || c == b'_')
        {
            while self
                .peek()
                .is_some_and(|c| c.is_ascii_alphanumeric() || c == b'_')
            {
                self.i += 1;
            }
        }
        if self.i == first {
            self.i = st;
            return self.fail(Syntax::ExpectedKeyword);
        }
        let k = self.s.get(st..self.i).unwrap_or(&[]);
        Ok(String::from_utf8_lossy(k).to_ascii_uppercase())
    }

    /// A section or end keyword: letters, digits, `_` and `-` (`END-ISO-10303-21`).
    fn section_keyword(&mut self) -> Result<String, P21Error> {
        self.ws()?;
        let st = self.i;
        while self
            .peek()
            .is_some_and(|c| c.is_ascii_alphanumeric() || c == b'_' || c == b'-')
        {
            self.i += 1;
        }
        if self.i == st {
            return self.fail(Syntax::ExpectedKeyword);
        }
        let k = self.s.get(st..self.i).unwrap_or(&[]);
        Ok(String::from_utf8_lossy(k).to_ascii_uppercase())
    }

    /// An integer or real (§6.4.1, §6.4.2).
    fn number(&mut self) -> Result<Param, P21Error> {
        let st = self.i;
        while self
            .peek()
            .is_some_and(|c| c.is_ascii_digit() || matches!(c, b'+' | b'-' | b'.' | b'E' | b'e'))
        {
            self.i += 1;
        }
        let t = core::str::from_utf8(self.s.get(st..self.i).unwrap_or(&[])).unwrap_or("");
        // A number has a digit before its exponent; a lone `.` or `+.` would otherwise read
        // as 0 through the `1.` fix-up below.
        let mantissa = t.split(['E', 'e']).next().unwrap_or("");
        if !mantissa.bytes().any(|c| c.is_ascii_digit()) {
            return self.fail(Syntax::BadNumber(t.to_string()));
        }
        if !t.contains(['.', 'E', 'e']) {
            return match t.parse::<i64>() {
                Ok(i) => Ok(Param::Int(i)),
                // An integer is a count or an index: one beyond i64 is refused, not rounded
                // into a real.
                Err(e)
                    if matches!(
                        e.kind(),
                        core::num::IntErrorKind::PosOverflow | core::num::IntErrorKind::NegOverflow
                    ) =>
                {
                    self.fail(Syntax::IntegerOutOfRange(t.to_string()))
                }
                Err(_) => self.fail(Syntax::BadNumber(t.to_string())),
            };
        }
        // `1.` and `1.E-3` are Part 21; Rust's reader wants a digit after the point.
        let fixed;
        let x = if t.ends_with('.') || t.contains(".E") || t.contains(".e") {
            fixed = {
                let f = t.replace(".E", ".0E").replace(".e", ".0e");
                if f.ends_with('.') {
                    f + "0"
                } else {
                    f
                }
            };
            fixed.parse::<f64>()
        } else {
            t.parse::<f64>()
        };
        match x {
            Ok(x) => Ok(Param::Real(x)),
            Err(_) => self.fail(Syntax::BadNumber(t.to_string())),
        }
    }

    /// A string after its opening apostrophe (§6.4.3), decoded.
    fn string(&mut self) -> Result<String, P21Error> {
        let st = self.i;
        let mut raw: Vec<u8> = Vec::new();
        let mut from = self.i;
        loop {
            match self.peek() {
                None => return self.fail(Syntax::UnterminatedString),
                Some(b'\'') => {
                    raw.extend_from_slice(self.s.get(from..=self.i).unwrap_or(&[]));
                    if self.peek_at(1) == Some(b'\'') {
                        self.i += 2;
                        from = self.i;
                    } else {
                        // The closing apostrophe is not part of the text.
                        raw.pop();
                        self.i += 1;
                        break;
                    }
                }
                // Line breaks are not part of the text (§5.6).
                Some(b'\n' | b'\r') => {
                    raw.extend_from_slice(self.s.get(from..self.i).unwrap_or(&[]));
                    self.i += 1;
                    from = self.i;
                }
                Some(_) => self.i += 1,
            }
        }
        let mut out = String::with_capacity(raw.len());
        let found = text::decode(&raw, &mut out);
        if let Some(page) = found.unsupported_page {
            let here = self.i;
            self.i = st;
            let line = self.line();
            self.i = here;
            self.warnings
                .push(Warning::UnsupportedCodePage { line, page });
        }
        Ok(out)
    }

    /// An enumeration after its first `.` (§6.4.5).
    fn enumeration(&mut self) -> Result<Param, P21Error> {
        let st = self.i;
        while self
            .peek()
            .is_some_and(|c| c.is_ascii_alphanumeric() || c == b'_')
        {
            self.i += 1;
        }
        let v = String::from_utf8_lossy(self.s.get(st..self.i).unwrap_or(&[])).to_ascii_uppercase();
        if self.peek() != Some(b'.') || !text::enumeration_ok(&v) {
            return self.fail(Syntax::BadEnumeration);
        }
        self.i += 1;
        Ok(Param::Enum(v))
    }

    /// A binary after its opening `"` (§6.4.6).
    fn binary(&mut self) -> Result<Param, P21Error> {
        let st = self.i;
        while self.peek().is_some_and(|c| c != b'"') {
            self.i += 1;
        }
        let v = String::from_utf8_lossy(self.s.get(st..self.i).unwrap_or(&[])).to_ascii_uppercase();
        if self.peek() != Some(b'"') {
            return self.fail(Syntax::Expected('"'));
        }
        if !text::binary_ok(&v) {
            return self.fail(Syntax::BadBinary(v));
        }
        self.i += 1;
        Ok(Param::Binary(v))
    }

    /// An instance name after its `#` (§6.4.4).
    fn reference(&mut self) -> Result<u64, P21Error> {
        let st = self.i;
        while self.peek().is_some_and(|c| c.is_ascii_digit()) {
            self.i += 1;
        }
        match id_of(self.s.get(st..self.i).unwrap_or(&[])) {
            Some(id) => Ok(id),
            None => self.fail(Syntax::BadReference),
        }
    }

    fn param(&mut self, depth: usize) -> Result<Param, P21Error> {
        if depth > MAX_DEPTH {
            return self.fail(Syntax::NestedTooDeeply { limit: MAX_DEPTH });
        }
        if self.params >= self.limits.params {
            return Err(Limit::TooMany {
                what: Counted::Params,
                limit: self.limits.params,
            }
            .into());
        }
        self.params += 1;
        self.ws()?;
        let Some(c) = self.peek() else {
            return self.fail(Syntax::UnexpectedEnd);
        };
        match c {
            b'$' => {
                self.i += 1;
                Ok(Param::Unset)
            }
            b'*' => {
                self.i += 1;
                Ok(Param::Derived)
            }
            b'\'' => {
                self.i += 1;
                Ok(Param::Str(self.string()?))
            }
            b'"' => {
                self.i += 1;
                self.binary()
            }
            b'#' => {
                self.i += 1;
                Ok(Param::Ref(self.reference()?))
            }
            b'.' if self
                .peek_at(1)
                .is_some_and(|c| c.is_ascii_alphabetic() || c == b'_') =>
            {
                self.i += 1;
                self.enumeration()
            }
            b'(' => {
                self.i += 1;
                Ok(Param::List(self.list_rest(depth + 1)?))
            }
            c if c.is_ascii_digit() || matches!(c, b'+' | b'-' | b'.') => self.number(),
            c if c.is_ascii_alphabetic() || c == b'_' || c == b'!' => {
                let name = self.keyword()?;
                self.expect(b'(')?;
                if self.eat(b')')? {
                    return self.fail(Syntax::TypedArity(name));
                }
                let value = self.param(depth + 1)?;
                if !self.eat(b')')? {
                    return self.fail(Syntax::TypedArity(name));
                }
                Ok(Param::Typed {
                    name,
                    value: Box::new(value),
                })
            }
            c => self.fail(Syntax::Unexpected(char::from(c))),
        }
    }

    /// Parameters after an opening `(` up to and including the `)`.
    fn list_rest(&mut self, depth: usize) -> Result<Vec<Param>, P21Error> {
        let mut v = Vec::new();
        if self.eat(b')')? {
            return Ok(v);
        }
        loop {
            v.push(self.param(depth)?);
            if self.eat(b',')? {
                continue;
            }
            self.expect(b')')?;
            return Ok(v);
        }
    }

    fn record(&mut self) -> Result<Record, P21Error> {
        if self.records >= self.limits.records {
            return Err(Limit::TooMany {
                what: Counted::Records,
                limit: self.limits.records,
            }
            .into());
        }
        self.records += 1;
        let name = text::long_name(self.keyword()?);
        self.expect(b'(')?;
        let params = self.list_rest(1)?;
        Ok(Record { name, params })
    }

    /// Where the next line after `from` that starts with a keyword begins.
    fn next_header_line(&self, from: usize) -> Option<usize> {
        let rest = self.s.get(from..)?;
        let mut at = rest.iter().position(|&c| c == b'\n')? + 1;
        loop {
            let line = at
                + rest
                    .get(at..)?
                    .iter()
                    .take_while(|c| matches!(c, b' ' | b'\t' | b'\r'))
                    .count();
            if rest.get(line)?.is_ascii_uppercase() {
                return Some(from + line);
            }
            at += rest.get(at..)?.iter().position(|&c| c == b'\n')? + 1;
        }
    }

    /// The HEADER section after `HEADER;` (§8).
    fn header(&mut self) -> Result<Header, P21Error> {
        let mut header = Header::default();
        loop {
            self.ws()?;
            let start = self.i;
            if self.peek().is_none() {
                return Err(Refusal::MissingEnd.into());
            }
            // A header without its `ENDSEC;` must not read the DATA section as header
            // records to skip.
            if self.section_keyword().as_deref() == Ok("DATA") {
                self.i = start;
                return self.fail(Syntax::HeaderWithoutEndsec);
            }
            self.i = start;
            let record = (|| {
                if self.section_keyword()? == "ENDSEC" {
                    self.expect(b';')?;
                    return Ok(None);
                }
                self.i = start;
                let name = self.keyword()?;
                self.expect(b'(')?;
                let params = self.list_rest(1)?;
                self.expect(b';')?;
                Ok(Some(Record { name, params }))
            })();
            match record {
                Ok(Some(r)) => header.records.push(r),
                Ok(None) => return Ok(header),
                // The header only describes the file, and other systems read past a record
                // that does not parse: skip to the next line that starts with a keyword. A
                // limit is never skipped.
                Err(P21Error::Refused(Refusal::Syntax { cause, .. })) => {
                    let Some(next) = self.next_header_line(start) else {
                        self.i = start;
                        let line = self.line();
                        return Err(P21Error::syntax(line, cause));
                    };
                    self.i = start;
                    let line = self.line();
                    self.warnings
                        .push(Warning::SkippedHeaderRecord { line, cause });
                    self.i = next;
                }
                Err(e) => return Err(e),
            }
        }
    }

    /// Count an instance against the limit and ask the progress watcher.
    fn count(&mut self) -> Result<(), P21Error> {
        if self.instances % PROGRESS_EVERY == 0 {
            if self.progress.stop() {
                return Err(P21Error::Cancelled);
            }
            self.progress.read(self.i, self.s.len());
        }
        // A repeated id's records were read too, so it counts as an instance.
        if self.instances >= self.limits.instances {
            return Err(Limit::TooMany {
                what: Counted::Instances,
                limit: self.limits.instances,
            }
            .into());
        }
        self.instances += 1;
        Ok(())
    }

    /// One instance after its `#` (§5.5 `ENTITY_INSTANCE`).
    fn instance(&mut self) -> Result<(u64, Instance), P21Error> {
        let id = self.reference()?;
        self.expect(b'=')?;
        let instance = if self.eat(b'(')? {
            let mut parts = Vec::new();
            while !self.eat(b')')? {
                if parts.len() >= MAX_COMPLEX_PARTS {
                    return self.fail(Syntax::TooManyParts {
                        limit: MAX_COMPLEX_PARTS,
                    });
                }
                parts.push(self.record()?);
            }
            // At least one part (§5.5 `SIMPLE_RECORD_LIST`).
            if parts.is_empty() {
                return self.fail(Syntax::ExpectedKeyword);
            }
            Instance::Complex(parts)
        } else {
            Instance::Simple(self.record()?)
        };
        self.expect(b';')?;
        self.count()?;
        Ok((id, instance))
    }

    /// A DATA section after `DATA` (§11.1).
    fn data(&mut self) -> Result<DataSection, P21Error> {
        let params = if self.eat(b'(')? {
            Some(self.list_rest(1)?)
        } else {
            None
        };
        self.expect(b';')?;
        let mut instances = Vec::new();
        loop {
            self.ws()?;
            match self.peek() {
                Some(b'#') => {
                    self.i += 1;
                    instances.push(self.instance()?);
                }
                None => return Err(Refusal::MissingEnd.into()),
                Some(_) => {
                    let k = self.section_keyword()?;
                    if k != "ENDSEC" {
                        return self.fail(Syntax::UnexpectedKeyword(k));
                    }
                    self.expect(b';')?;
                    break;
                }
            }
        }
        let (section, repeated) = DataSection::new(params, instances);
        self.warnings
            .extend(repeated.into_iter().map(|id| Warning::DuplicateId { id }));
        Ok(section)
    }

    /// An ANCHOR, REFERENCE or SIGNATURE section after its keyword: the text between `;` and
    /// the `ENDSEC;` that ends it outside a string, comment or binary.
    fn raw_section(&mut self, name: &str) -> Result<String, P21Error> {
        self.expect(b';')?;
        let st = self.i;
        loop {
            match self.peek() {
                None => return self.fail(Syntax::UnterminatedSection(name.to_string())),
                Some(q @ (b'\'' | b'"')) => {
                    // A doubled apostrophe is two strings back to back here: the same end.
                    self.i += 1;
                    while self.peek().is_some_and(|c| c != q) {
                        self.i += 1;
                    }
                    self.i += 1;
                }
                Some(b'/') if self.peek_at(1) == Some(b'*') => self.ws()?,
                Some(b'E')
                    if self
                        .s
                        .get(self.i..)
                        .is_some_and(|r| r.starts_with(b"ENDSEC"))
                        && !self
                            .i
                            .checked_sub(1)
                            .and_then(|b| self.s.get(b))
                            .is_some_and(|c| c.is_ascii_alphanumeric() || *c == b'_') =>
                {
                    let body = self.s.get(st..self.i).unwrap_or(&[]);
                    let body = String::from_utf8_lossy(body).into_owned();
                    self.i += 6;
                    self.expect(b';')?;
                    return Ok(body);
                }
                Some(_) => self.i += 1,
            }
        }
    }

    fn exchange(&mut self) -> Result<Exchange, P21Error> {
        let Ok(magic) = self.section_keyword() else {
            return Err(Refusal::NotStep.into());
        };
        if magic != "ISO-10303-21" || !self.eat(b';')? {
            return Err(Refusal::NotStep.into());
        }
        let mut ex = Exchange::default();
        let mut header = false;
        loop {
            self.ws()?;
            if self.peek().is_none() {
                return Err(Refusal::MissingEnd.into());
            }
            let kw = self.section_keyword()?;
            match kw.as_str() {
                "HEADER" if !header => {
                    self.expect(b';')?;
                    ex.header = self.header()?;
                    header = true;
                }
                "DATA" => ex.sections.push(self.data()?),
                "ANCHOR" => ex.anchor = Some(self.raw_section("ANCHOR")?),
                "REFERENCE" => ex.reference = Some(self.raw_section("REFERENCE")?),
                "END-ISO-10303-21" => {
                    self.expect(b';')?;
                    break;
                }
                _ => return self.fail(Syntax::UnexpectedKeyword(kw)),
            }
        }
        // Instance names are unique across the exchange (§11.1): a name defined again in a later
        // section is kept, as read, and reported.
        if ex.sections.len() > 1 {
            let mut seen = HashSet::with_hasher(FixedState);
            for (id, _) in ex.instances() {
                if !seen.insert(id) {
                    self.warnings.push(Warning::DuplicateId { id });
                }
            }
        }
        // Signature sections follow the end keyword (§14).
        loop {
            self.ws()?;
            if self.peek().is_none() {
                return Ok(ex);
            }
            let at = self.i;
            if self.section_keyword().as_deref() == Ok("SIGNATURE") {
                ex.signatures.push(self.raw_section("SIGNATURE")?);
                continue;
            }
            self.i = at;
            let line = self.line();
            self.warnings.push(Warning::TextAfterEnd { line });
            return Ok(ex);
        }
    }
}
