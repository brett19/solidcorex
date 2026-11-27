// Written for solidcorex. Apache-2.0.
//! Why bytes or text did not read as a record, and why a value could not be written.
//!
//! [`ReadError`] is a context frame (01 §5.1): it adds the offset where reading stopped and
//! prints only that; [`ReadError::cause`] (and `source()`) is the typed [`ReadCause`]. The
//! kind is computed from the cause, never stored (research 62 §10, 63 X13). There is no
//! state to clear: a failed read returns its error and the reader is not used again.

use core::fmt;

use scx_brep::FinishCause;
use scx_curve::CurveError;
use scx_num::error::ErrorKind;
use scx_num::frame::FrameError;
use scx_surface::SurfaceError;

/// Bytes or text that did not read: where, and why.
#[derive(Debug, Clone, PartialEq)]
pub struct ReadError {
    /// The byte offset into the input where the failing field starts (for text, the offset
    /// of the token or separator that failed; for a truncation, where more was needed).
    pub at: usize,
    /// Why.
    pub cause: ReadCause,
}

impl ReadError {
    /// The error `cause` at byte `at`.
    #[must_use]
    pub const fn new(at: usize, cause: ReadCause) -> Self {
        Self { at, cause }
    }

    /// What the caller does next: the cause's kind.
    #[must_use]
    pub fn kind(&self) -> ErrorKind {
        self.cause.kind()
    }

    /// The cause's stable name.
    #[must_use]
    pub fn name(&self) -> &'static str {
        self.cause.name()
    }
}

impl fmt::Display for ReadError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "record unreadable at byte {}", self.at)
    }
}

impl std::error::Error for ReadError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(&self.cause)
    }
}

/// Why a record did not read. Each cause has its own kind (63 X13).
///
/// A geometry record is rebuilt by its type's own checked constructor, and a value that
/// constructor refuses is the constructor's cause, framed: [`ReadCause::Frame`],
/// [`ReadCause::Curve`], [`ReadCause::Surface`], whose kind is the cause's and whose
/// `source()` is it. The curve and surface causes are boxed, so a [`ReadError`] stays at
/// 32 bytes.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub enum ReadCause {
    /// The input ends inside a field, or a list or table claims more entries than the input
    /// has left. [`ErrorKind::Refused`].
    Truncated,
    /// The input goes on after the record ends. [`ErrorKind::Refused`]: a record is the whole
    /// input, so that `encode(decode(x)) == x` for every `x` that decodes.
    Trailing,
    /// The record does not start with the magic its reader asked for. [`ErrorKind::Refused`]:
    /// the bytes are not this record.
    BadMagic,
    /// The format version is not one this reader knows. A version newer than
    /// [`FORMAT_VERSION`](crate::FORMAT_VERSION) is [`ErrorKind::Unsupported`] (valid data
    /// from a newer writer); version 0, which no writer writes, is [`ErrorKind::Refused`].
    BadVersion {
        /// The version found.
        found: u16,
    },
    /// A tag byte names no variant of its enum. [`ErrorKind::Refused`]. There is no
    /// `Unknown` variant to read it into (research 57 F2).
    BadTag {
        /// The enum the tag picks a variant of ([`Tag::RECORD`](crate::Tag::RECORD)).
        record: &'static str,
        /// The byte found.
        found: u8,
    },
    /// A text token is not the canonical spelling of what was expected: not a number, a
    /// number with a sign, a leading zero or a non-shortest digit string, an unknown name,
    /// or the wrong separator. [`ErrorKind::Refused`].
    BadToken {
        /// What was expected: a type (`"u32"`, `"f64"`), an enum ([`Tag::RECORD`](crate::Tag::RECORD))
        /// or `"separator"`.
        expected: &'static str,
    },
    /// A number is a NaN or an infinity. [`ErrorKind::Refused`]: no record stores one
    /// (63 D1).
    NonFinite,
    /// A run of deleted slots is empty, follows another run, or passes the end of its table.
    /// [`ErrorKind::Refused`]: runs are written whole, so each spelling of a table is the
    /// only one.
    BadRun,
    /// A table claims more slots than its reader allows. [`ErrorKind::Exhausted`]: a run of
    /// deleted slots costs a few bytes on the wire and a slot each in memory, so the reader
    /// takes a cap.
    TooMany {
        /// The slots claimed.
        count: u32,
        /// The cap.
        limit: u32,
    },
    /// A frame's axes as read are not a frame ([`Frame::from_axes`]): its cause.
    ///
    /// [`Frame::from_axes`]: scx_num::frame::Frame::from_axes
    Frame(FrameError),
    /// A curve as read does not build: its constructor's cause (a NURBS curve's as
    /// [`CurveError::Nurbs`]).
    Curve(Box<CurveError>),
    /// A surface as read does not build: its constructor's cause (a NURBS surface's as
    /// [`SurfaceError::Nurbs`]).
    Surface(Box<SurfaceError>),
    /// A value spelled in a form its writer never picks for it: a rational NURBS record in
    /// the homogeneous form whose homogeneous points are also each point times its weight,
    /// rounded, which the writer spells by its points. [`ErrorKind::Refused`]: each value has
    /// one spelling.
    NonCanonical {
        /// The record ([`Tag::RECORD`](crate::Tag::RECORD) of its kind).
        record: &'static str,
    },
    /// An interval's ends are inverted (`min > max`): no [`Extent1d`] holds them.
    /// [`ErrorKind::Refused`].
    ///
    /// [`Extent1d`]: scx_num::extent1d::Extent1d
    BadInterval,
    /// A recorded accuracy is negative: no [`Accuracy`] holds it. [`ErrorKind::Refused`].
    ///
    /// [`Accuracy`]: scx_num::units::Accuracy
    BadAccuracy,
    /// A certificate names more inputs than [`MAX_READS`]. [`ErrorKind::Refused`].
    ///
    /// [`MAX_READS`]: scx_num::cert::MAX_READS
    BadReads {
        /// The inputs named.
        count: u32,
    },
    /// The body as read is not a body: [`Draft::finish`] refused it, and this is its cause.
    /// The kind is the cause's: a structure or geometry fault is [`ErrorKind::Refused`].
    /// The offset is the input's end, since the whole record is judged.
    ///
    /// [`Draft::finish`]: scx_brep::Draft::finish
    Invalid(Box<FinishCause>),
}

impl ReadCause {
    /// What the caller does next.
    #[must_use]
    pub fn kind(&self) -> ErrorKind {
        match self {
            Self::Truncated
            | Self::Trailing
            | Self::BadMagic
            | Self::BadTag { .. }
            | Self::BadToken { .. }
            | Self::NonFinite
            | Self::BadRun
            | Self::NonCanonical { .. }
            | Self::BadInterval
            | Self::BadAccuracy
            | Self::BadReads { .. } => ErrorKind::Refused,
            Self::Invalid(e) => e.kind(),
            Self::Frame(e) => e.kind(),
            Self::Curve(e) => e.kind(),
            Self::Surface(e) => e.kind(),
            Self::BadVersion { found } => {
                if *found > crate::FORMAT_VERSION {
                    ErrorKind::Unsupported
                } else {
                    ErrorKind::Refused
                }
            }
            Self::TooMany { .. } => ErrorKind::Exhausted,
        }
    }

    /// The cause's stable name. A frame, curve or surface cause is named for the frame;
    /// its own name is the source's.
    #[must_use]
    pub const fn name(&self) -> &'static str {
        match self {
            Self::Truncated => "truncated",
            Self::Trailing => "trailing",
            Self::BadMagic => "bad_magic",
            Self::BadVersion { .. } => "bad_version",
            Self::BadTag { .. } => "bad_tag",
            Self::BadToken { .. } => "bad_token",
            Self::NonFinite => "non_finite",
            Self::BadRun => "bad_run",
            Self::TooMany { .. } => "too_many",
            Self::Frame(_) => "bad_frame",
            Self::Curve(_) => "bad_curve",
            Self::Surface(_) => "bad_surface",
            Self::NonCanonical { .. } => "non_canonical",
            Self::BadInterval => "bad_interval",
            Self::BadAccuracy => "bad_accuracy",
            Self::BadReads { .. } => "bad_reads",
            Self::Invalid(_) => "invalid",
        }
    }
}

impl fmt::Display for ReadCause {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Truncated => f.write_str("the input ends inside a record"),
            Self::Trailing => f.write_str("the input goes on after the record"),
            Self::BadMagic => f.write_str("the input is not this record (wrong magic)"),
            Self::BadVersion { found } => write!(
                f,
                "format version {found} is not one this reader knows (1 to {})",
                crate::FORMAT_VERSION
            ),
            Self::BadTag { record, found } => write!(f, "tag {found} is no {record}"),
            Self::BadToken { expected } => write!(f, "expected a canonical {expected}"),
            Self::NonFinite => f.write_str("a number is not finite"),
            Self::BadRun => f.write_str("a run of deleted slots is empty, doubled or too long"),
            Self::TooMany { count, limit } => {
                write!(f, "a table claims {count} slots, over the limit of {limit}")
            }
            Self::Frame(_) => f.write_str("a frame record is no frame"),
            Self::Curve(_) => f.write_str("a curve record does not build"),
            Self::Surface(_) => f.write_str("a surface record does not build"),
            Self::NonCanonical { record } => {
                write!(f, "a {record} record is not in its canonical form")
            }
            Self::BadInterval => f.write_str("an interval's ends are inverted"),
            Self::BadAccuracy => f.write_str("a recorded accuracy is negative"),
            Self::BadReads { count } => write!(
                f,
                "a certificate names {count} inputs, over {}",
                scx_num::cert::MAX_READS
            ),
            Self::Invalid(_) => f.write_str("the body as read does not finish"),
        }
    }
}

impl std::error::Error for ReadCause {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Frame(e) => Some(e),
            Self::Curve(e) => Some(&**e),
            Self::Surface(e) => Some(&**e),
            Self::Invalid(e) => Some(&**e),
            _ => None,
        }
    }
}

/// A value that could not be written: where, and why.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WriteError {
    /// The output's length when the value was refused.
    pub at: usize,
    /// Why.
    pub cause: WriteCause,
}

impl WriteError {
    /// The error `cause` at output length `at`.
    #[must_use]
    pub const fn new(at: usize, cause: WriteCause) -> Self {
        Self { at, cause }
    }

    /// What the caller does next: the cause's kind. A writer of a type whose values are
    /// finite and short by construction (a body) re-judges these as `Defect` (01 §5.1).
    #[must_use]
    pub const fn kind(&self) -> ErrorKind {
        self.cause.kind()
    }

    /// The cause's stable name.
    #[must_use]
    pub const fn name(&self) -> &'static str {
        self.cause.name()
    }
}

impl fmt::Display for WriteError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "record unwritable at byte {}", self.at)
    }
}

impl std::error::Error for WriteError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(&self.cause)
    }
}

/// Why a value could not be written.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum WriteCause {
    /// A number is a NaN or an infinity. [`ErrorKind::Refused`]: no record stores one.
    NonFinite,
    /// A list or table has more than `u32::MAX` entries. [`ErrorKind::Exhausted`]: the
    /// format's fixed cap.
    TooLong {
        /// The entries.
        len: usize,
    },
    /// A rational NURBS value whose homogeneous points are neither each point times its
    /// weight, rounded, nor whose points each homogeneous point over its weight, rounded:
    /// neither record form rebuilds it to the bit.
    /// [`ErrorKind::Defect`]: no constructor or edit builds one.
    MixedRational {
        /// The record ([`Tag::RECORD`](crate::Tag::RECORD) of its kind).
        record: &'static str,
    },
    /// A variant of a `#[non_exhaustive]` geometry enum this crate has no record for (one
    /// added after it). [`ErrorKind::Unsupported`].
    NoRecord {
        /// The enum ([`Tag::RECORD`](crate::Tag::RECORD) of its kinds).
        record: &'static str,
    },
}

impl WriteCause {
    /// What the caller does next.
    #[must_use]
    pub const fn kind(&self) -> ErrorKind {
        match self {
            Self::NonFinite => ErrorKind::Refused,
            Self::TooLong { .. } => ErrorKind::Exhausted,
            Self::MixedRational { .. } => ErrorKind::Defect,
            Self::NoRecord { .. } => ErrorKind::Unsupported,
        }
    }

    /// The cause's stable name.
    #[must_use]
    pub const fn name(&self) -> &'static str {
        match self {
            Self::NonFinite => "non_finite",
            Self::TooLong { .. } => "too_long",
            Self::MixedRational { .. } => "mixed_rational",
            Self::NoRecord { .. } => "no_record",
        }
    }
}

impl fmt::Display for WriteCause {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NonFinite => f.write_str("a number is not finite"),
            Self::TooLong { len } => write!(f, "{len} entries are over the format's u32 count"),
            Self::MixedRational { record } => write!(
                f,
                "a rational {record} keeps neither its points nor its homogeneous points exactly"
            ),
            Self::NoRecord { record } => write!(f, "this {record} kind has no record"),
        }
    }
}

impl std::error::Error for WriteCause {}
