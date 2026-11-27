// Written for solidcorex.
//! The units and uncertainty of a representation context, with nothing defaulted.
//!
//! [`scx_p21::units::context`] reads a context's units and says which it leaves out
//! ([`scx_p21::units::Unit::Missing`]) and which it contradicts; an importer must not guess, so
//! this module asks it for the units, with an assumption only where the caller states one
//! ([`MissingUnits`], [`UnitsFault`]), and keeps [`Uncertainty::Absent`] for a context that
//! states none.

pub use scx_p21::units::{MissingUnits, UnitsFault};
use scx_p21::units::{Units, context};
use scx_p21::Exchange;

/// A context's distance uncertainty.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Uncertainty {
    /// The context names none that reads as finite and positive. SP2 takes the document's
    /// resolution.
    Absent,
    /// The context's `UNCERTAINTY_MEASURE_WITH_UNIT`, in millimetres: its value times the factor
    /// of its unit (the context's length unit when it names none). Where a context lists
    /// several, the largest: the file's stated accuracy is the coarsest it claims.
    Stated(f64),
}

/// A representation context's units and uncertainty, with what was assumed.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Resolved {
    /// Millimetres per file length unit, radians per file plane angle unit. A part that
    /// [`Resolved::assumed`] marks is the caller's assumption, not the file's.
    pub units: Units,
    /// Which of the two units the caller assumed because the file names none.
    pub assumed: MissingUnits,
    /// The context's uncertainty.
    pub uncertainty: Uncertainty,
    /// The context lists several different uncertainties, so [`Uncertainty::Stated`] is the
    /// largest.
    pub several_uncertainties: bool,
}

/// Resolve the context `context` (`None`: the representation names none). `assume` supplies
/// the units the file leaves out; with `None`, a missing unit is [`UnitsFault::Missing`].
/// A unit a file names that cannot be read (a conversion to an unreadable unit) counts as
/// not named.
///
/// # Errors
/// [`UnitsFault`]: a unit missing and not assumed, or two different units of one kind.
pub fn resolve(
    ex: &Exchange,
    context_id: Option<u64>,
    assume: Option<Units>,
) -> Result<Resolved, UnitsFault> {
    let c = context_id.map(|id| context(ex, id)).unwrap_or_default();
    let (units, assumed) = c.units(assume)?;
    let u = c.uncertainty(units.len);
    Ok(Resolved {
        units,
        assumed,
        uncertainty: u.largest.map_or(Uncertainty::Absent, Uncertainty::Stated),
        several_uncertainties: u.several,
    })
}

/// Whether `units` can be assumed: both factors finite and positive.
pub(crate) fn assumable(units: Units) -> bool {
    let size = |f: f64| f.is_finite() && f > 0.0;
    size(units.len) && size(units.ang)
}
