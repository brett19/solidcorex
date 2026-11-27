// Written for solidcorex (docs/01-architecture.md §5). The refusals are the ones solidcore's
// Part 21 parser at 03e16c9ee766de5b9b1ced845e5149d62ff493b8 made (`step/p21.rs` `Error`),
// typed where it carried a message string (docs/research/58-p21.md §4).
//! The crate's one error type, [`P21Error`], its typed causes, and the warnings of a read
//! that succeeded.
//!
//! Display strings are for people; the variants and causes are the interface. Each
//! `Display` prints its own frame; [`scx_num::error::chain`] joins a chain of them.

use core::fmt;

use scx_num::error::{chain, ErrorKind};

/// Why a text could not be read or an exchange could not be written.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum P21Error {
    /// The input is not valid: the text breaks the exchange structure, or holds a value
    /// Part 21 cannot write.
    Refused(Refusal),
    /// The input is beyond a limit the caller or the reader sets (research 62 M1).
    Exhausted(Limit),
    /// Valid input this crate does not read: an entity kind the reader has no mapping for.
    Unsupported(Unsupported),
    /// The caller's [`Progress`](crate::Progress) asked to stop.
    Cancelled,
    /// Reading instance `#id` needed another instance, and reading that one failed: the
    /// error `source` names the inner instance, and a chain of these names the whole path
    /// from the instance read to the one at fault.
    Within {
        /// The instance being read.
        id: u64,
        /// Why reading what it uses failed.
        source: Box<P21Error>,
    },
}

impl P21Error {
    /// The class of the error (docs/01-architecture.md §5.1).
    #[must_use]
    pub fn kind(&self) -> ErrorKind {
        match self.innermost() {
            Self::Refused(_) | Self::Within { .. } => ErrorKind::Refused,
            Self::Unsupported(_) => ErrorKind::Unsupported,
            Self::Exhausted(_) => ErrorKind::Exhausted,
            Self::Cancelled => ErrorKind::Cancelled,
        }
    }

    /// The stable name of the cause at the end of the chain, unique in the crate
    /// (research 62 D6): with the kind and the call, the error's signature.
    #[must_use]
    pub fn name(&self) -> &'static str {
        match self.innermost() {
            Self::Refused(r) => r.name(),
            Self::Exhausted(l) => l.name(),
            Self::Unsupported(u) => u.name(),
            Self::Cancelled => "cancelled",
            // `innermost` passes every `Within`
            Self::Within { source, .. } => source.name(),
        }
    }

    /// The error at the end of a chain of [`P21Error::Within`]: itself, when it is not one.
    #[must_use]
    pub fn innermost(&self) -> &Self {
        let mut e = self;
        while let Self::Within { source, .. } = e {
            e = source;
        }
        e
    }

    /// The instance the error names first, if it names one.
    #[must_use]
    pub const fn id(&self) -> Option<u64> {
        match self {
            Self::Refused(
                Refusal::Entity { id, .. } | Refusal::Unwritable { id: Some(id), .. },
            )
            | Self::Unsupported(Unsupported::Entity { id, .. })
            | Self::Exhausted(Limit::TooDeep { id, .. } | Limit::TooManyInstances { id, .. })
            | Self::Within { id, .. } => Some(*id),
            _ => None,
        }
    }

    /// The error as met while reading instance `#id`: wrapped in [`P21Error::Within`], unless
    /// it already names `#id` first, or is a cancellation, which is the caller's and belongs
    /// to no instance.
    #[must_use]
    pub fn within(self, id: u64) -> Self {
        if matches!(self, Self::Cancelled) || self.id() == Some(id) {
            return self;
        }
        Self::Within {
            id,
            source: Box::new(self),
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
    /// The innermost error's class, the path of instances to it, and its cause: "STEP input
    /// refused: in #90: in #20: #4: no such instance".
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let leaf = self.innermost();
        f.write_str(match leaf {
            Self::Refused(_) | Self::Within { .. } => "STEP input refused: ",
            Self::Unsupported(_) => "STEP input not supported: ",
            Self::Exhausted(_) => "STEP input beyond a limit: ",
            Self::Cancelled => "STEP reading cancelled",
        })?;
        let mut e = self;
        while let Self::Within { id, source } = e {
            write!(f, "in #{id}: ")?;
            e = source;
        }
        match leaf {
            Self::Refused(r) => chain(r).fmt(f),
            Self::Unsupported(u) => u.fmt(f),
            Self::Exhausted(l) => l.fmt(f),
            Self::Cancelled | Self::Within { .. } => Ok(()),
        }
    }
}

impl std::error::Error for P21Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Refused(r) => Some(r),
            Self::Unsupported(u) => Some(u),
            Self::Exhausted(l) => Some(l),
            Self::Within { source, .. } => Some(source.as_ref()),
            Self::Cancelled => None,
        }
    }
}

impl From<Refusal> for P21Error {
    fn from(r: Refusal) -> Self {
        Self::Refused(r)
    }
}

impl From<Limit> for P21Error {
    fn from(l: Limit) -> Self {
        Self::Exhausted(l)
    }
}

/// A limit the input is beyond: the fixed caps on input size and the caller's
/// [`Limits`](crate::Limits) (research 62 M1).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum Limit {
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
    /// References from instance `#id` nested deeper than the reader follows (or a cycle).
    TooDeep {
        /// The instance read at the limit.
        id: u64,
        /// The deepest nesting followed.
        limit: usize,
    },
    /// Expanding the structure from instance `#id` gives more nodes than the reader makes.
    TooManyInstances {
        /// The instance expanded.
        id: u64,
        /// The most nodes made.
        limit: usize,
    },
}

impl Limit {
    /// The cause's stable name, unique in the crate (research 62 D6).
    #[must_use]
    pub fn name(&self) -> &'static str {
        match self {
            Self::TooLarge { .. } => "too_large",
            Self::TooMany { .. } => "too_many",
            Self::NeedsMemory { .. } => "needs_memory",
            Self::TooDeep { .. } => "too_deep",
            Self::TooManyInstances { .. } => "too_many_instances",
        }
    }
}

impl fmt::Display for Limit {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
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
            Self::TooDeep { id, limit } => {
                write!(f, "#{id}: references nested more than {limit} deep")
            }
            Self::TooManyInstances { id, limit } => {
                write!(f, "#{id}: expands to more than {limit} instances")
            }
        }
    }
}

impl std::error::Error for Limit {}

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

impl Refusal {
    /// The stable name of the refusal's cause, unique in the crate (research 62 D6).
    #[must_use]
    pub fn name(&self) -> &'static str {
        match self {
            Self::NotStep => "not_step",
            Self::MissingEnd => "missing_end",
            Self::Syntax { cause, .. } => cause.name(),
            Self::Unwritable { cause, .. } => cause.name(),
            Self::Entity { cause, .. } => cause.name(),
        }
    }
}

impl fmt::Display for Refusal {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotStep => f.write_str("not a STEP file (no ISO-10303-21 header)"),
            Self::MissingEnd => f.write_str("the file ends without END-ISO-10303-21"),
            Self::Syntax { line, .. } => write!(f, "syntax error at line {line}"),
            Self::Unwritable { id: Some(id), .. } | Self::Entity { id, .. } => write!(f, "#{id}"),
            // Transparent: the cause's own frame.
            Self::Unwritable { id: None, cause } => cause.fmt(f),
        }
    }
}

/// A syntax error, an instance's refusal or a written instance's unwritable value is a
/// context frame (research 62 D12): it prints where ("syntax error at line 8", "#10") and
/// its `source()` is the cause. An unwritable value outside an instance is transparent.
impl std::error::Error for Refusal {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Syntax { cause, .. } => Some(cause),
            Self::Unwritable { id: Some(_), cause } => Some(cause),
            Self::Unwritable { id: None, cause } => cause.source(),
            Self::Entity { cause, .. } => Some(cause),
            Self::NotStep | Self::MissingEnd => None,
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

impl Syntax {
    /// The cause's stable name, unique in the crate (research 62 D6).
    #[must_use]
    pub fn name(&self) -> &'static str {
        match self {
            Self::Expected(..) => "expected_char",
            Self::ExpectedKeyword => "expected_keyword",
            Self::UnexpectedEnd => "unexpected_end",
            Self::Unexpected(..) => "unexpected_char",
            Self::UnexpectedKeyword(..) => "unexpected_keyword",
            Self::UnterminatedComment => "unterminated_comment",
            Self::UnterminatedString => "unterminated_string",
            Self::UnterminatedSection(..) => "unterminated_section",
            Self::HeaderWithoutEndsec => "header_without_endsec",
            Self::BadNumber(..) => "bad_number",
            Self::IntegerOutOfRange(..) => "integer_out_of_range",
            Self::BadReference => "bad_reference",
            Self::BadBinary(..) => "bad_binary",
            Self::BadEnumeration => "bad_enumeration",
            Self::TypedArity(..) => "typed_arity",
            Self::NestedTooDeeply { .. } => "nested_too_deeply",
            Self::TooManyParts { .. } => "too_many_parts",
        }
    }
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

impl Unwritable {
    /// The cause's stable name, unique in the crate (research 62 D6).
    #[must_use]
    pub fn name(&self) -> &'static str {
        match self {
            Self::NonFiniteReal(..) => "non_finite_real",
            Self::BadKeyword(..) => "unwritable_keyword",
            Self::BadEnumeration(..) => "unwritable_enumeration",
            Self::BadBinary(..) => "unwritable_binary",
            Self::EmptyComplex => "empty_complex",
            Self::Geometry(g) => g.name(),
        }
    }
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

impl NoPart42Form {
    /// The cause's stable name, unique in the crate (research 62 D6).
    #[must_use]
    pub fn name(&self) -> &'static str {
        match self {
            Self::OffParameterPlane => "off_parameter_plane",
            Self::MixedUnitVector => "mixed_unit_vector",
            Self::AxisInParameterSpace => "axis_in_parameter_space",
            Self::SpaceCurveInParameterSpace => "space_curve_in_parameter_space",
        }
    }
}

impl fmt::Display for NoPart42Form {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::OffParameterPlane => {
                "a point or direction off a parameter space's plane has no ISO 10303-42 form"
            }
            Self::MixedUnitVector => {
                "a vector whose components have different unit factors has no ISO 10303-42 form"
            }
            Self::AxisInParameterSpace => {
                "a placement with an axis in a parameter space has no ISO 10303-42 form"
            }
            Self::SpaceCurveInParameterSpace => {
                "a surface curve or 3D offset curve in a parameter space has no ISO 10303-42 form"
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
            // Transparent (research 62 D12): the geometry's own frame.
            Self::Geometry(g) => g.fmt(f),
        }
    }
}

impl std::error::Error for Unwritable {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Geometry(g) => g.source(),
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
    Invalid(InvalidValue),
    /// The instance is part of a cycle where the file's structure must be a tree or a
    /// directed acyclic graph (an assembly using itself, a mapped item mapping itself).
    Cycle,
}

impl EntityCause {
    /// The cause's stable name, unique in the crate (research 62 D6).
    #[must_use]
    pub fn name(&self) -> &'static str {
        match self {
            Self::Missing => "missing",
            Self::WrongType { .. } => "wrong_type",
            Self::BadParam { .. } => "bad_param",
            Self::Invalid(v) => v.name(),
            Self::Cycle => "cycle",
        }
    }
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
            Self::Invalid(v) => v.fmt(f),
            Self::Cycle => f.write_str("part of a cycle"),
        }
    }
}

impl std::error::Error for EntityCause {}

/// A value of an instance that is out of its range or inconsistent with another
/// ([`EntityCause::Invalid`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum InvalidValue {
    /// Knots that decrease, or not one multiplicity per knot.
    KnotsDecrease,
    /// A knot vector without two distinct knots, or a knot above the degree + 1.
    KnotVector,
    /// Knot multiplicities that do not match the control points.
    KnotCount,
    /// No more control points than the degree.
    TooFewControlPoints,
    /// A Bezier spline whose control points are not the degree times the segments, plus 1.
    BezierSegments,
    /// A weight that is not positive.
    WeightNotPositive,
    /// Weights that do not match the control points.
    WeightCount,
    /// Weights that do not match the control net.
    NetWeightCount,
    /// A direction whose ratios are all zero.
    ZeroDirection,
    /// A vector magnitude that is negative.
    NegativeMagnitude,
    /// A vector of magnitude zero.
    ZeroMagnitude,
    /// A circle radius that is not positive.
    CircleRadius,
    /// An ellipse semi axis that is not positive.
    EllipseSemiAxis,
    /// A hyperbola semi axis that is not positive.
    HyperbolaSemiAxis,
    /// A parabola focal distance of zero.
    ParabolaFocalDistance,
    /// A surface curve with more than two associated items.
    SurfaceCurveItems,
    /// A pcurve representation without exactly one item.
    PcurveItems,
    /// A B-spline with more than 1 000 000 control points.
    CurveControlPoints,
    /// A cylinder radius that is not positive.
    CylinderRadius,
    /// A cone with a negative radius or a semi angle outside (0, π/2) in size.
    ConeShape,
    /// A sphere radius that is not positive.
    SphereRadius,
    /// A torus minor radius that is not positive.
    TorusMinorRadius,
    /// A torus major radius that is not finite.
    TorusMajorRadius,
    /// A B-spline surface with more than 1 000 000 control points.
    SurfaceControlPoints,
    /// A B-spline control net that is empty or ragged.
    ControlNet,
}

impl InvalidValue {
    /// The cause's stable name, unique in the crate (research 62 D6).
    #[must_use]
    pub fn name(&self) -> &'static str {
        match self {
            Self::KnotsDecrease => "knots_decrease",
            Self::KnotVector => "knot_vector",
            Self::KnotCount => "knot_count",
            Self::TooFewControlPoints => "too_few_control_points",
            Self::BezierSegments => "bezier_segments",
            Self::WeightNotPositive => "weight_not_positive",
            Self::WeightCount => "weight_count",
            Self::NetWeightCount => "net_weight_count",
            Self::ZeroDirection => "zero_direction",
            Self::NegativeMagnitude => "negative_magnitude",
            Self::ZeroMagnitude => "zero_magnitude",
            Self::CircleRadius => "circle_radius",
            Self::EllipseSemiAxis => "ellipse_semi_axis",
            Self::HyperbolaSemiAxis => "hyperbola_semi_axis",
            Self::ParabolaFocalDistance => "parabola_focal_distance",
            Self::SurfaceCurveItems => "surface_curve_items",
            Self::PcurveItems => "pcurve_items",
            Self::CurveControlPoints => "curve_control_points",
            Self::CylinderRadius => "cylinder_radius",
            Self::ConeShape => "cone_shape",
            Self::SphereRadius => "sphere_radius",
            Self::TorusMinorRadius => "torus_minor_radius",
            Self::TorusMajorRadius => "torus_major_radius",
            Self::SurfaceControlPoints => "surface_control_points",
            Self::ControlNet => "control_net",
        }
    }
}

impl fmt::Display for InvalidValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::KnotsDecrease => "knots that decrease, or not one multiplicity per knot",
            Self::KnotVector => {
                "a knot vector without two distinct knots, or a knot above the degree + 1"
            }
            Self::KnotCount => "knot multiplicities that do not match the control points",
            Self::TooFewControlPoints => "no more control points than the degree",
            Self::BezierSegments => {
                "a Bezier spline whose control points are not the degree times the segments, plus 1"
            }
            Self::WeightNotPositive => "a weight that is not positive",
            Self::WeightCount => "weights that do not match the control points",
            Self::NetWeightCount => "weights that do not match the control net",
            Self::ZeroDirection => "a direction whose ratios are all zero",
            Self::NegativeMagnitude => "a vector magnitude that is negative",
            Self::ZeroMagnitude => "a vector of magnitude zero",
            Self::CircleRadius => "a circle radius that is not positive",
            Self::EllipseSemiAxis => "an ellipse semi axis that is not positive",
            Self::HyperbolaSemiAxis => "a hyperbola semi axis that is not positive",
            Self::ParabolaFocalDistance => "a parabola focal distance of zero",
            Self::SurfaceCurveItems => "a surface curve with more than two associated items",
            Self::PcurveItems => "a pcurve representation without exactly one item",
            Self::CurveControlPoints => "a B-spline with more than 1 000 000 control points",
            Self::CylinderRadius => "a cylinder radius that is not positive",
            Self::ConeShape => {
                "a cone with a negative radius or a semi angle outside (0, π/2) in size"
            }
            Self::SphereRadius => "a sphere radius that is not positive",
            Self::TorusMinorRadius => "a torus minor radius that is not positive",
            Self::TorusMajorRadius => "a torus major radius that is not finite",
            Self::SurfaceControlPoints => {
                "a B-spline surface with more than 1 000 000 control points"
            }
            Self::ControlNet => "a B-spline control net that is empty or ragged",
        })
    }
}

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

impl Unsupported {
    /// The cause's stable name, unique in the crate (research 62 D6).
    #[must_use]
    pub fn name(&self) -> &'static str {
        match self {
            Self::Entity { .. } => "unsupported_entity",
        }
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    /// Every cause's name is unique in the crate (research 62 D6), and an error's name is
    /// its last cause's.
    #[test]
    fn cause_names_are_unique() {
        let limits = [
            Limit::TooLarge { bytes: 0, limit: 0 },
            Limit::TooMany {
                what: Counted::Instances,
                limit: 0,
            },
            Limit::NeedsMemory {
                needed: 0,
                available: 0,
            },
            Limit::TooDeep { id: 0, limit: 0 },
            Limit::TooManyInstances { id: 0, limit: 0 },
        ];
        let syntax = [
            Syntax::Expected('x'),
            Syntax::ExpectedKeyword,
            Syntax::UnexpectedEnd,
            Syntax::Unexpected('x'),
            Syntax::UnexpectedKeyword(String::new()),
            Syntax::UnterminatedComment,
            Syntax::UnterminatedString,
            Syntax::UnterminatedSection(String::new()),
            Syntax::HeaderWithoutEndsec,
            Syntax::BadNumber(String::new()),
            Syntax::IntegerOutOfRange(String::new()),
            Syntax::BadReference,
            Syntax::BadBinary(String::new()),
            Syntax::BadEnumeration,
            Syntax::TypedArity(String::new()),
            Syntax::NestedTooDeeply { limit: 0 },
            Syntax::TooManyParts { limit: 0 },
        ];
        let unwritable = [
            Unwritable::NonFiniteReal(0.0),
            Unwritable::BadKeyword(String::new()),
            Unwritable::BadEnumeration(String::new()),
            Unwritable::BadBinary(String::new()),
            Unwritable::EmptyComplex,
            Unwritable::Geometry(NoPart42Form::OffParameterPlane),
            Unwritable::Geometry(NoPart42Form::MixedUnitVector),
            Unwritable::Geometry(NoPart42Form::AxisInParameterSpace),
            Unwritable::Geometry(NoPart42Form::SpaceCurveInParameterSpace),
        ];
        let entity = [
            EntityCause::Missing,
            EntityCause::WrongType {
                expected: "",
                found: String::new(),
            },
            EntityCause::BadParam {
                index: 0,
                expected: "",
            },
            EntityCause::Cycle,
            EntityCause::Invalid(InvalidValue::KnotsDecrease),
            EntityCause::Invalid(InvalidValue::KnotVector),
            EntityCause::Invalid(InvalidValue::KnotCount),
            EntityCause::Invalid(InvalidValue::TooFewControlPoints),
            EntityCause::Invalid(InvalidValue::BezierSegments),
            EntityCause::Invalid(InvalidValue::WeightNotPositive),
            EntityCause::Invalid(InvalidValue::WeightCount),
            EntityCause::Invalid(InvalidValue::NetWeightCount),
            EntityCause::Invalid(InvalidValue::ZeroDirection),
            EntityCause::Invalid(InvalidValue::NegativeMagnitude),
            EntityCause::Invalid(InvalidValue::ZeroMagnitude),
            EntityCause::Invalid(InvalidValue::CircleRadius),
            EntityCause::Invalid(InvalidValue::EllipseSemiAxis),
            EntityCause::Invalid(InvalidValue::HyperbolaSemiAxis),
            EntityCause::Invalid(InvalidValue::ParabolaFocalDistance),
            EntityCause::Invalid(InvalidValue::SurfaceCurveItems),
            EntityCause::Invalid(InvalidValue::PcurveItems),
            EntityCause::Invalid(InvalidValue::CurveControlPoints),
            EntityCause::Invalid(InvalidValue::CylinderRadius),
            EntityCause::Invalid(InvalidValue::ConeShape),
            EntityCause::Invalid(InvalidValue::SphereRadius),
            EntityCause::Invalid(InvalidValue::TorusMinorRadius),
            EntityCause::Invalid(InvalidValue::TorusMajorRadius),
            EntityCause::Invalid(InvalidValue::SurfaceControlPoints),
            EntityCause::Invalid(InvalidValue::ControlNet),
        ];
        let errors: Vec<P21Error> = limits
            .into_iter()
            .map(P21Error::Exhausted)
            .chain([Refusal::NotStep, Refusal::MissingEnd].map(P21Error::Refused))
            .chain(
                syntax
                    .into_iter()
                    .map(|cause| P21Error::Refused(Refusal::Syntax { line: 1, cause })),
            )
            .chain(
                unwritable
                    .into_iter()
                    .map(|cause| P21Error::Refused(Refusal::Unwritable { id: None, cause })),
            )
            .chain(entity.into_iter().map(|cause| P21Error::entity(1, cause)))
            .chain([
                P21Error::Unsupported(Unsupported::Entity {
                    id: 1,
                    name: String::new(),
                }),
                P21Error::Cancelled,
            ])
            .collect();
        let names: Vec<&str> = errors.iter().map(P21Error::name).collect();
        let mut sorted = names.clone();
        sorted.sort_unstable();
        sorted.dedup();
        assert_eq!(sorted.len(), names.len(), "{names:?}");
        assert!(names.iter().all(|n| n
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_')));
        let within = P21Error::Within {
            id: 2,
            source: Box::new(P21Error::entity(1, EntityCause::Cycle)),
        };
        assert_eq!(within.name(), "cycle");
    }
}
