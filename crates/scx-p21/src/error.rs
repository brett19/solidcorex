// Written for solidcorex (docs/01-architecture.md §5). The refusals are the ones solidcore's
// Part 21 parser at 03e16c9ee766de5b9b1ced845e5149d62ff493b8 made (`step/p21.rs` `Error`),
// typed where it carried a message string (docs/research/58-p21.md §4).
//! The crate's one error type, [`P21Error`], its typed causes, and the warnings of a read
//! that succeeded.
//!
//! Display strings are for people; the variants and causes are the interface.

use core::fmt;

use scx_num::error::ErrorKind;

/// Why a text could not be read or an exchange could not be written.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum P21Error {
    /// The input is not valid: the text breaks the exchange structure, is beyond a limit,
    /// or holds a value Part 21 cannot write.
    Refused(Refusal),
    /// Valid input this crate does not read: an entity kind the reader has no mapping for.
    Unsupported(Unsupported),
    /// The caller's [`Progress`](crate::Progress) asked to stop.
    Cancelled,
}

impl P21Error {
    /// The class of the error (docs/01-architecture.md §5.1).
    ///
    /// `scx-num`'s [`ErrorKind`] has no `Unsupported` or `Cancelled` class yet; until it
    /// gains them both read as [`ErrorKind::Failed`].
    #[must_use]
    pub const fn kind(&self) -> ErrorKind {
        match self {
            Self::Refused(_) => ErrorKind::InvalidInput,
            Self::Unsupported(_) | Self::Cancelled => ErrorKind::Failed,
        }
    }

    /// Instance `#id` is not what its reader needs.
    #[must_use]
    pub const fn entity(id: u64, cause: EntityCause) -> Self {
        Self::Refused(Refusal::Entity { id, cause })
    }

    /// A syntax error at `line`.
    pub(crate) const fn syntax(line: usize, cause: Syntax) -> Self {
        Self::Refused(Refusal::Syntax { line, cause })
    }
}

impl fmt::Display for P21Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Refused(r) => write!(f, "STEP input refused: {r}"),
            Self::Unsupported(u) => write!(f, "STEP input not supported: {u}"),
            Self::Cancelled => f.write_str("STEP reading cancelled"),
        }
    }
}

impl std::error::Error for P21Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Refused(r) => Some(r),
            Self::Unsupported(u) => Some(u),
            Self::Cancelled => None,
        }
    }
}

impl From<Refusal> for P21Error {
    fn from(r: Refusal) -> Self {
        Self::Refused(r)
    }
}

/// What a limit counts.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum Counted {
    /// Entity instances (a repeated id counts again).
    Instances,
    /// Records: one per simple instance, one per part of a complex one.
    Records,
    /// Parameters, at every depth, header and data.
    Params,
}

impl fmt::Display for Counted {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Instances => "entity instances",
            Self::Records => "records",
            Self::Params => "parameters",
        })
    }
}

/// Why input is not valid.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum Refusal {
    /// The text does not start with `ISO-10303-21;` (ISO 10303-21:2016 §5.5, `EXCHANGE_FILE`).
    NotStep,
    /// The text ends before `END-ISO-10303-21;`.
    MissingEnd,
    /// The text breaks the exchange structure's grammar.
    Syntax {
        /// The line, counted from 1.
        line: usize,
        /// What was wrong.
        cause: Syntax,
    },
    /// The text is longer than the limit.
    TooLarge {
        /// The text's length in bytes.
        bytes: usize,
        /// The limit in bytes.
        limit: usize,
    },
    /// More of something than the limit.
    TooMany {
        /// What was counted.
        what: Counted,
        /// The limit.
        limit: usize,
    },
    /// Reading the text would take more memory than the caller can spare.
    NeedsMemory {
        /// The memory reading it would take, in bytes.
        needed: usize,
        /// The memory the caller can spare, in bytes.
        available: usize,
    },
    /// A value has no Part 21 form.
    Unwritable {
        /// The instance it is in (`None`: the header or a section's parameters).
        id: Option<u64>,
        /// What cannot be written.
        cause: Unwritable,
    },
    /// An entity instance is not what its reader needs.
    Entity {
        /// The instance.
        id: u64,
        /// What was wrong.
        cause: EntityCause,
    },
}

impl fmt::Display for Refusal {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotStep => f.write_str("not a STEP file (no ISO-10303-21 header)"),
            Self::MissingEnd => f.write_str("the file ends without END-ISO-10303-21"),
            Self::Syntax { line, cause } => write!(f, "syntax error at line {line}: {cause}"),
            Self::TooLarge { bytes, limit } => write!(
                f,
                "the file is too large ({} MB, limit {} MB)",
                bytes >> 20,
                limit >> 20
            ),
            Self::TooMany { what, limit } => write!(f, "the file has more than {limit} {what}"),
            Self::NeedsMemory { needed, available } => write!(
                f,
                "reading the file needs about {} MB of memory, more than the {} MB available",
                needed >> 20,
                available >> 20
            ),
            Self::Unwritable {
                id: Some(id),
                cause,
            } => write!(f, "#{id}: {cause}"),
            Self::Unwritable { id: None, cause } => cause.fmt(f),
            Self::Entity { id, cause } => write!(f, "#{id}: {cause}"),
        }
    }
}

impl std::error::Error for Refusal {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Syntax { cause, .. } => Some(cause),
            Self::Unwritable { cause, .. } => Some(cause),
            Self::Entity { cause, .. } => Some(cause),
            _ => None,
        }
    }
}

/// How a text breaks the grammar of ISO 10303-21:2016 §5.4 and §5.5.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum Syntax {
    /// A token was expected and another came.
    Expected(char),
    /// A keyword was expected.
    ExpectedKeyword,
    /// The text ended inside an instance or record.
    UnexpectedEnd,
    /// A character no token starts with.
    Unexpected(char),
    /// A keyword where none of that name may stand.
    UnexpectedKeyword(String),
    /// A comment with no `*/`.
    UnterminatedComment,
    /// A string with no closing apostrophe.
    UnterminatedString,
    /// An ANCHOR, REFERENCE or SIGNATURE section with no `ENDSEC;`.
    UnterminatedSection(String),
    /// The HEADER section ran into `DATA` without its `ENDSEC;`.
    HeaderWithoutEndsec,
    /// A number with no digit in its mantissa, or otherwise not a number (§5.4 `REAL`,
    /// `INTEGER`).
    BadNumber(String),
    /// An integer beyond the signed 64-bit range.
    IntegerOutOfRange(String),
    /// `#` not followed by an instance number (§5.4 `ENTITY_INSTANCE_NAME`).
    BadReference,
    /// A binary that is not a digit 0 to 3 followed by hexadecimal digits (§5.4 `BINARY`).
    BadBinary(String),
    /// An enumeration that is not `.` letters and digits `.` (§5.4 `ENUMERATION`).
    BadEnumeration,
    /// A typed parameter that does not hold exactly one parameter (§5.5 `TYPED_PARAMETER`).
    TypedArity(String),
    /// Parameters nested deeper than the limit.
    NestedTooDeeply {
        /// The deepest nesting read.
        limit: usize,
    },
    /// A complex instance with more parts than the limit.
    TooManyParts {
        /// The most parts read.
        limit: usize,
    },
}

impl fmt::Display for Syntax {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Expected(c) => write!(f, "expected `{c}`"),
            Self::ExpectedKeyword => f.write_str("expected a keyword"),
            Self::UnexpectedEnd => f.write_str("unexpected end of file"),
            Self::Unexpected(c) => write!(f, "unexpected `{}`", c.escape_debug()),
            Self::UnexpectedKeyword(k) => write!(f, "unexpected `{k}`"),
            Self::UnterminatedComment => f.write_str("unterminated comment"),
            Self::UnterminatedString => f.write_str("unterminated string"),
            Self::UnterminatedSection(s) => write!(f, "unterminated {s} section"),
            Self::HeaderWithoutEndsec => f.write_str("expected `ENDSEC;` before `DATA`"),
            Self::BadNumber(t) => write!(f, "bad number `{t}`"),
            Self::IntegerOutOfRange(t) => write!(f, "integer `{t}` out of range"),
            Self::BadReference => f.write_str("bad entity reference"),
            Self::BadBinary(t) => write!(f, "bad binary `{t}`"),
            Self::BadEnumeration => f.write_str("bad enumeration"),
            Self::TypedArity(k) => {
                write!(
                    f,
                    "typed parameter `{k}` does not hold exactly one parameter"
                )
            }
            Self::NestedTooDeeply { limit } => {
                write!(f, "parameters nested more than {limit} deep")
            }
            Self::TooManyParts { limit } => {
                write!(f, "complex entity with more than {limit} parts")
            }
        }
    }
}

impl std::error::Error for Syntax {}

/// A value with no Part 21 form.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum Unwritable {
    /// A real that is infinite or NaN (§5.4 `REAL` has no form for one).
    NonFiniteReal(f64),
    /// A keyword that is not upper-case letters, digits and `_` starting with a letter or
    /// `_`, `!` first for a user-defined one (§5.4 `KEYWORD`).
    BadKeyword(String),
    /// An enumeration value that is not upper-case letters, digits and `_` starting with a
    /// letter or `_` (§5.4 `ENUMERATION`).
    BadEnumeration(String),
    /// A binary that is not a digit 0 to 3 followed by upper-case hexadecimal digits.
    BadBinary(String),
    /// A complex instance with no part.
    EmptyComplex,
    /// Geometry with no ISO 10303-42 form.
    Geometry(NoPart42Form),
}

/// Geometry that ISO 10303-42 cannot state, as [`crate::part42::write`] meets it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum NoPart42Form {
    /// A point or direction of a 2D parameter space whose z is not 0.
    OffParameterPlane,
    /// A vector whose components have different unit factors, as one read in a parameter
    /// space with an angle on one axis and a length on the other.
    MixedUnitVector,
    /// A placement with an axis, in a 2D parameter space.
    AxisInParameterSpace,
    /// A surface curve or a 3D offset curve, in a 2D parameter space.
    SpaceCurveInParameterSpace,
}

impl fmt::Display for NoPart42Form {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::OffParameterPlane => "a point or direction off a parameter space's plane",
            Self::MixedUnitVector => "a vector whose components have different unit factors",
            Self::AxisInParameterSpace => "a placement with an axis in a parameter space",
            Self::SpaceCurveInParameterSpace => {
                "a surface curve or 3D offset curve in a parameter space"
            }
        })
    }
}

impl std::error::Error for NoPart42Form {}

impl fmt::Display for Unwritable {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NonFiniteReal(x) => write!(f, "the real {x} has no Part 21 form"),
            Self::BadKeyword(k) => write!(f, "`{k}` is not a Part 21 keyword"),
            Self::BadEnumeration(e) => write!(f, "`{e}` is not a Part 21 enumeration value"),
            Self::BadBinary(b) => write!(f, "`{b}` is not a Part 21 binary"),
            Self::EmptyComplex => f.write_str("a complex instance needs at least one part"),
            Self::Geometry(g) => write!(f, "{g} has no ISO 10303-42 form"),
        }
    }
}

impl std::error::Error for Unwritable {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Geometry(g) => Some(g),
            _ => None,
        }
    }
}

/// How an entity instance is not what its reader needs.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum EntityCause {
    /// The instance does not exist.
    Missing,
    /// The instance is not of a type the reader accepts here.
    WrongType {
        /// What was needed.
        expected: &'static str,
        /// The instance's (first record's) entity name.
        found: String,
    },
    /// A parameter is missing or of the wrong kind.
    BadParam {
        /// The parameter's position in its record, from 0.
        index: usize,
        /// What was needed.
        expected: &'static str,
    },
    /// A value is out of its range or inconsistent with another.
    Invalid {
        /// What was wrong.
        what: &'static str,
    },
    /// References nested deeper than the reader follows (or a cycle).
    TooDeep {
        /// The deepest nesting followed.
        limit: usize,
    },
    /// The instance is part of a cycle where the file's structure must be a tree or a
    /// directed acyclic graph (an assembly using itself, a mapped item mapping itself).
    Cycle,
    /// Expanding the structure from the instance gives more nodes than the reader makes.
    TooManyInstances {
        /// The most nodes made.
        limit: usize,
    },
}

impl fmt::Display for EntityCause {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Missing => f.write_str("no such instance"),
            Self::WrongType { expected, found } => {
                write!(f, "expected {expected}, found `{found}`")
            }
            Self::BadParam { index, expected } => {
                write!(f, "parameter {index}: expected {expected}")
            }
            Self::Invalid { what } => f.write_str(what),
            Self::TooDeep { limit } => write!(f, "references nested more than {limit} deep"),
            Self::Cycle => f.write_str("part of a cycle"),
            Self::TooManyInstances { limit } => {
                write!(f, "expands to more than {limit} instances")
            }
        }
    }
}

impl std::error::Error for EntityCause {}

/// Valid input this crate does not read.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum Unsupported {
    /// An entity kind the reader has no mapping for.
    Entity {
        /// The instance.
        id: u64,
        /// Its (first record's) entity name.
        name: String,
    },
}

impl fmt::Display for Unsupported {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Entity { id, name } => write!(f, "#{id}: `{name}` is not read here"),
        }
    }
}

impl std::error::Error for Unsupported {}

/// Something a successful read passed over or normalised, for the caller to report.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum Warning {
    /// An instance id defined more than once. In one DATA section the first definition is
    /// kept and the repeat dropped; in two sections both are kept, and [`crate::Exchange::get`]
    /// finds the first (§11.1 makes the repeat invalid, and some writers emit them for geometry
    /// nobody reads).
    DuplicateId {
        /// The id.
        id: u64,
    },
    /// A HEADER record that does not parse was skipped to the next line that starts with a
    /// keyword (other CAD systems read past one too).
    SkippedHeaderRecord {
        /// Where the record started.
        line: usize,
        /// Why it did not parse.
        cause: Syntax,
    },
    /// A `\S\` string escape under a code page other than ISO 8859-1 (`\PA\`), whose
    /// character tables this crate does not carry: the character reads as U+FFFD.
    UnsupportedCodePage {
        /// The line of the string.
        line: usize,
        /// The page letter, `B` to `I` (ISO 8859-2 to -9).
        page: char,
    },
    /// Text after `END-ISO-10303-21;` that is not a SIGNATURE section was ignored.
    TextAfterEnd {
        /// Where it starts.
        line: usize,
    },
}

impl fmt::Display for Warning {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::DuplicateId { id } => {
                write!(
                    f,
                    "instance #{id} is defined more than once; the first is kept"
                )
            }
            Self::SkippedHeaderRecord { line, cause } => {
                write!(f, "line {line}: skipped a header record ({cause})")
            }
            Self::UnsupportedCodePage { line, page } => write!(
                f,
                "line {line}: a character on ISO 8859 code page {page} read as U+FFFD"
            ),
            Self::TextAfterEnd { line } => {
                write!(f, "line {line}: text after END-ISO-10303-21 ignored")
            }
        }
    }
}
