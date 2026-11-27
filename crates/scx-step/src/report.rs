// Written for solidcorex.
//! What a read says besides the bodies: the protocol, the units it was told to assume, the
//! bodies it skipped, and (from SP2) the accuracy raises it made to enclosed gaps.

use scx_p21::protocol::{Declared, Doubt};
use scx_p21::units::Units;

use crate::units::MissingUnits;

/// The gap an accuracy was raised to enclose (research 74 §6.3).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum GapKind {
    /// An edge's curve to its face's surface.
    EdgeToFace,
    /// A vertex to the edge that ends there.
    VertexToEdge,
    /// A pcurve to its edge's curve.
    PcurveToEdge,
}

/// One accuracy raised to an enclosed gap: imported data whose certified gap exceeds its stated
/// uncertainty is valid input, so the entity's accuracy becomes the gap (09 §6, 04 rows 41 and
/// 42; research 74 R3). A raise is made once, to a proved value, and is recorded here; it is
/// not healing, and `scx-step` makes no other change to the file's geometry. SP2 fills
/// [`ImportReport::raised`]; SP1 only defines it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Raise {
    /// The STEP entity (edge, vertex or face) whose accuracy was raised.
    pub entity: u64,
    /// The gap it encloses.
    pub gap: GapKind,
    /// Its accuracy before, in millimetres.
    pub before: f64,
    /// Its accuracy after, in millimetres: the certified gap.
    pub after: f64,
}

/// Units the caller assumed for one representation because its context names none.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AssumedUnits {
    /// The representation (entity id).
    pub representation: u64,
    /// The units its bodies were read in.
    pub units: Units,
    /// Which of the two the file did not name, so the caller's were used.
    pub which: MissingUnits,
}

/// Why a body is not in [`Imported::bodies`](crate::Imported::bodies).
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum SkipReason {
    /// A kind of body this crate does not read: faceted breps (tessellated models are not B-rep).
    BodyKind(String),
    /// The body's entities are not what its kind needs (a solid whose shell is open, a face with no
    /// surface, a missing entity): the first fault, naming its instance.
    Malformed(Box<scx_p21::P21Error>),
}

/// A body the read dropped, and why.
#[derive(Clone, Debug, PartialEq)]
pub struct Skipped {
    /// The body (entity id).
    pub entity: u64,
    /// Why.
    pub why: SkipReason,
}

/// Something the caller should know that is not a failure.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum ImportWarning {
    /// The file's schema is not fully known (an unknown AP242 edition, an unknown or missing
    /// schema). It is read as far as it goes.
    Protocol(Doubt),
    /// The file lists no body that this crate reads or skips.
    NoBodies,
    /// A context lists several different uncertainties; the largest was taken.
    SeveralUncertainties {
        /// The representation (entity id) whose context it is.
        representation: u64,
    },
}

/// The report of a read.
#[derive(Clone, Debug, PartialEq)]
pub struct ImportReport {
    /// The protocol the file declares, and any doubt about it.
    pub declared: Declared,
    /// Warnings, in the order of representation ids.
    pub warnings: Vec<ImportWarning>,
    /// Units assumed where a context names none, by representation id.
    pub assumed: Vec<AssumedUnits>,
    /// Accuracy raises to enclosed gaps, in entity order. Empty until SP2 measures gaps; a
    /// round trip of our own writer's file must leave it empty (research 74 §10).
    pub raised: Vec<Raise>,
}
