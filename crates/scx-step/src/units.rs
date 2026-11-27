// Written for solidcorex.
//! The units and uncertainty of a representation context, with nothing defaulted.
//!
//! [`scx_p21::units::context`] reads a context's units and reads a missing one as millimetres
//! and radians. That is right for a reader that only needs numbers; an importer must not guess,
//! so this module reads the same entities (through [`scx_p21::units::unit_factor`]) and keeps
//! what the file did not say: [`MissingUnits`], and [`Uncertainty::Absent`].

use scx_p21::units::{UnitKind, Units, unit_factor};
use scx_p21::{Exchange, Param};

/// Which of a context's two units a file leaves out (or, in an error, which is at fault).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct MissingUnits {
    /// The length unit.
    pub length: bool,
    /// The plane angle unit.
    pub angle: bool,
}

impl MissingUnits {
    /// Whether anything is named.
    #[must_use]
    pub const fn any(self) -> bool {
        self.length || self.angle
    }

    /// "length unit", "plane angle unit" or "length unit or plane angle unit".
    pub(crate) const fn describe(self) -> &'static str {
        match (self.length, self.angle) {
            (true, true) => "length unit or plane angle unit",
            (false, true) => "plane angle unit",
            _ => "length unit",
        }
    }

    /// The plural of [`MissingUnits::describe`].
    pub(crate) const fn describe_plural(self) -> &'static str {
        match (self.length, self.angle) {
            (true, true) => "length units and plane angle units",
            (false, true) => "plane angle units",
            _ => "length units",
        }
    }
}

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

/// Why a context's units cannot be resolved.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UnitsFault {
    /// The context names no such unit, and none was assumed.
    Missing(MissingUnits),
    /// The context names two different such units.
    Conflicting(MissingUnits),
}

/// A unit factor that reads as a size.
fn size(f: f64) -> bool {
    f.is_finite() && f > 0.0
}

/// The list the record `name` of entity `id` gives as its first attribute.
fn listed<'a>(ex: &'a Exchange, id: u64, name: &str) -> &'a [Param] {
    ex.get(id)
        .and_then(|e| e.record(name))
        .and_then(<[Param]>::first)
        .and_then(Param::as_list)
        .unwrap_or(&[])
}

/// Resolve the context `context` (`None`: the representation names none). `assume` supplies
/// the units the file leaves out; with `None`, a missing unit is [`UnitsFault::Missing`].
/// A unit a file names that cannot be read (a conversion to an unreadable unit) counts as
/// not named.
pub fn resolve(
    ex: &Exchange,
    context: Option<u64>,
    assume: Option<Units>,
) -> Result<Resolved, UnitsFault> {
    let mut length: Option<f64> = None;
    let mut angle: Option<f64> = None;
    let mut conflict = MissingUnits::default();
    let mut raw: Vec<(f64, Option<f64>)> = Vec::new();
    if let Some(id) = context {
        for uid in listed(ex, id, "GLOBAL_UNIT_ASSIGNED_CONTEXT")
            .iter()
            .filter_map(Param::as_ref_id)
        {
            let Some((kind, f)) = unit_factor(ex, uid).filter(|x| size(x.1)) else {
                continue;
            };
            match kind {
                UnitKind::Length => name(&mut length, f, &mut conflict.length),
                UnitKind::Angle => name(&mut angle, f, &mut conflict.angle),
                UnitKind::Other => {}
            }
        }
        for uid in listed(ex, id, "GLOBAL_UNCERTAINTY_ASSIGNED_CONTEXT")
            .iter()
            .filter_map(Param::as_ref_id)
        {
            let Some(m) = ex.get(uid) else { continue };
            let Some(v) = m.params().first().and_then(Param::as_f64) else {
                continue;
            };
            let f = m
                .params()
                .get(1)
                .and_then(Param::as_ref_id)
                .and_then(|x| unit_factor(ex, x))
                .map(|x| x.1)
                .filter(|f| size(*f));
            raw.push((v, f));
        }
    }
    if conflict.any() {
        return Err(UnitsFault::Conflicting(conflict));
    }
    let missing = MissingUnits {
        length: length.is_none(),
        angle: angle.is_none(),
    };
    let (length, angle) = if missing.any() {
        let Some(a) = assume else {
            return Err(UnitsFault::Missing(missing));
        };
        (Some(length.unwrap_or(a.len)), Some(angle.unwrap_or(a.ang)))
    } else {
        (length, angle)
    };
    Ok(finish(length, angle, missing, &raw))
}

/// Record the unit `f` a context names in `slot`, or flag a conflict with one already named.
fn name(slot: &mut Option<f64>, f: f64, conflict: &mut bool) {
    match *slot {
        Some(old) if old != f => *conflict = true,
        _ => *slot = Some(f),
    }
}

/// The resolved context: both units known (the caller checked), the uncertainties converted.
fn finish(
    length: Option<f64>,
    angle: Option<f64>,
    assumed: MissingUnits,
    raw: &[(f64, Option<f64>)],
) -> Resolved {
    let units = Units {
        len: length.unwrap_or(1.0),
        ang: angle.unwrap_or(1.0),
    };
    let mut mm: Vec<f64> = raw
        .iter()
        .map(|&(v, f)| v * f.unwrap_or(units.len))
        .filter(|t| size(*t))
        .collect();
    mm.sort_by(f64::total_cmp);
    mm.dedup();
    Resolved {
        units,
        assumed,
        uncertainty: mm
            .last()
            .map_or(Uncertainty::Absent, |&t| Uncertainty::Stated(t)),
        several_uncertainties: mm.len() > 1,
    }
}

/// Whether `units` can be assumed: both factors finite and positive.
pub(crate) fn assumable(units: Units) -> bool {
    size(units.len) && size(units.ang)
}
