// Derived from solidcore `solidcore/crates/solidcore-record/src/step/units.rs` at commit
// 03e16c9ee766de5b9b1ced845e5149d62ff493b8 (its own origin: SolidCraft's `crates/step`,
// MIT OR Apache-2.0).
// Upstream record (verbatim from that file):
// The units of a representation context (ISO 10303-41 measures and units, ISO 10303-43
// representation contexts): `GLOBAL_UNIT_ASSIGNED_CONTEXT`'s length and plane angle units and
// `GLOBAL_UNCERTAINTY_ASSIGNED_CONTEXT`'s distance uncertainty, read from a parsed
// [`Exchange`]. Missing or malformed units read as millimetres and radians; nothing here fails.
//
// Changed in solidcorex (research 74 §23): a unit the context does not name is `Unit::Missing`
// and two different ones `Unit::Conflicting`, not millimetres and radians; the uncertainty is the
// largest the context states, with a flag when it states several.
//
// A unit is a factor: millimetres per file length unit, radians per file plane angle unit. An
// SI unit is its prefix times its base (`METRE` = 1000 mm, `RADIAN` = 1 rad); a
// `CONVERSION_BASED_UNIT` is the value of its measure times the factor of the measure's own unit
// (an inch is `LENGTH_MEASURE(25.4)` of millimetres, a degree `PLANE_ANGLE_MEASURE(0.01745…)`
// of radians). Only multiplications: the factor of a degree is the number the file writes, not
// a π computed here (the record computes no geometric quantity, docs/layers.md §4).
//
// Origin: SolidCraft's `crates/step/src/units.rs` and the `Units` type of
// `crates/step/src/read.rs` at `a5b71ca2` (MIT OR Apache-2.0), carried over with their
// behaviour unchanged.
//
// Deviations from SolidCraft: [`Units`] lives here (it was in `read.rs`, which is not carried
// over yet). The nesting bound of a `CONVERSION_BASED_UNIT` chain is the named
// `MAX_CONVERSION_DEPTH` (was the literal 4). Otherwise only formatting and doc comments on every
// public item.
// Changes: `context` reads its records through `and_then`; the test helper
// `parse` reads through this crate's `parse`. Paths otherwise. Results unchanged.
//! The units of a representation context (ISO 10303-41 measures and units, ISO 10303-43
//! representation contexts): `GLOBAL_UNIT_ASSIGNED_CONTEXT`'s length and plane angle units and
//! `GLOBAL_UNCERTAINTY_ASSIGNED_CONTEXT`'s distance uncertainty, read from a parsed
//! [`Exchange`]. A unit the context does not name is [`Unit::Missing`], and two different ones are
//! [`Unit::Conflicting`]; nothing is defaulted here, and nothing here fails.
//!
//! A unit is a factor: millimetres per file length unit, radians per file plane angle unit. An
//! SI unit is its prefix times its base (`METRE` = 1000 mm, `RADIAN` = 1 rad); a
//! `CONVERSION_BASED_UNIT` is the value of its measure times the factor of the measure's own unit
//! (an inch is `LENGTH_MEASURE(25.4)` of millimetres, a degree `PLANE_ANGLE_MEASURE(0.01745…)`
//! of radians). Only multiplications: the factor of a degree is the number the file writes, not
//! a π computed here (the record computes no geometric quantity, docs/layers.md §4).
//!
//! Origin: SolidCraft's `crates/step/src/units.rs` and the `Units` type of
//! `crates/step/src/read.rs` at `a5b71ca2` (MIT OR Apache-2.0), carried over with their
//! behaviour unchanged.
//!
//! Deviations from SolidCraft: [`Units`] lives here (it was in `read.rs`, which is not carried
//! over yet). The nesting bound of a `CONVERSION_BASED_UNIT` chain is the named
//! `MAX_CONVERSION_DEPTH` (was the literal 4). Otherwise only formatting and doc comments on every
//! public item.

use crate::exchange::{Exchange, Param};

/// Most `CONVERSION_BASED_UNIT` links followed from one unit (a unit defined from a unit defined
/// from …); a deeper chain, or a cycle, reads as no unit.
const MAX_CONVERSION_DEPTH: usize = 4;

/// The units of a representation context.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Units {
    /// Millimetres per file length unit.
    pub len: f64,
    /// Radians per file plane angle unit.
    pub ang: f64,
}

impl Default for Units {
    /// Millimetres and radians. For a caller that chooses them (an assumption it states, a
    /// test); [`context`] never applies it.
    fn default() -> Self {
        Units { len: 1.0, ang: 1.0 }
    }
}

/// What a representation context says about one kind of unit.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum Unit {
    /// The context names no unit of this kind that reads as a finite, positive factor.
    #[default]
    Missing,
    /// The context names this factor (millimetres per file length unit, radians per file plane
    /// angle unit), once or several times alike.
    Named(f64),
    /// The context names two different factors.
    Conflicting,
}

impl Unit {
    /// The factor, when the context names exactly one.
    #[must_use]
    pub const fn named(self) -> Option<f64> {
        match self {
            Self::Named(f) => Some(f),
            _ => None,
        }
    }

    /// Record the factor `f` a context names.
    fn name(&mut self, f: f64) {
        *self = match *self {
            Self::Missing => Self::Named(f),
            Self::Named(old) if old == f => Self::Named(old),
            _ => Self::Conflicting,
        };
    }
}

/// Which of a context's two units (an error names the ones at fault).
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
    #[must_use]
    pub const fn describe(self) -> &'static str {
        match (self.length, self.angle) {
            (true, true) => "length unit or plane angle unit",
            (false, true) => "plane angle unit",
            _ => "length unit",
        }
    }

    /// The plural of [`MissingUnits::describe`].
    #[must_use]
    pub const fn describe_plural(self) -> &'static str {
        match (self.length, self.angle) {
            (true, true) => "length units and plane angle units",
            (false, true) => "plane angle units",
            _ => "length units",
        }
    }
}

/// Why a context's units cannot be used.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UnitsFault {
    /// The context names no such unit, and none was assumed.
    Missing(MissingUnits),
    /// The context names two different such units.
    Conflicting(MissingUnits),
}

/// A context's distance uncertainty in millimetres.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Uncertainty {
    /// The largest finite, positive one the context states (the file's stated accuracy is the
    /// coarsest it claims); `None` when it states none.
    pub largest: Option<f64>,
    /// The context states several different ones.
    pub several: bool,
}

/// What a representation context says about its numbers. Nothing is assumed: see [`Unit`].
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Context {
    /// The length unit.
    pub length: Unit,
    /// The plane angle unit.
    pub angle: Unit,
    /// Each `UNCERTAINTY_MEASURE_WITH_UNIT` the context lists with a readable value: the value
    /// and the factor of its unit (`None` when it names none that reads).
    pub distances: Vec<(f64, Option<f64>)>,
}

impl Context {
    /// The distance uncertainty in millimetres. A measure that names no readable unit is in
    /// `length`, the length unit the caller uses (the context's own, or one it assumes); a
    /// product that is not finite and positive is left out.
    #[must_use]
    pub fn uncertainty(&self, length: f64) -> Uncertainty {
        let mut mm: Vec<f64> = self
            .distances
            .iter()
            .map(|&(v, f)| v * f.unwrap_or(length))
            .filter(|t| size(*t))
            .collect();
        mm.sort_by(f64::total_cmp);
        mm.dedup();
        Uncertainty {
            largest: mm.last().copied(),
            several: mm.len() > 1,
        }
    }

    /// The units to read the context's numbers in. A unit the context leaves out is taken
    /// from `assume` (which the caller states; the file's own units always win); with `None`
    /// it is [`UnitsFault::Missing`]. A conflict is [`UnitsFault::Conflicting`] either way.
    /// The second value says which units were assumed.
    ///
    /// # Errors
    /// [`UnitsFault`] as above.
    pub fn units(&self, assume: Option<Units>) -> Result<(Units, MissingUnits), UnitsFault> {
        let conflict = MissingUnits {
            length: self.length == Unit::Conflicting,
            angle: self.angle == Unit::Conflicting,
        };
        if conflict.any() {
            return Err(UnitsFault::Conflicting(conflict));
        }
        let missing = MissingUnits {
            length: self.length == Unit::Missing,
            angle: self.angle == Unit::Missing,
        };
        match (
            self.length.named().or_else(|| assume.map(|a| a.len)),
            self.angle.named().or_else(|| assume.map(|a| a.ang)),
        ) {
            (Some(len), Some(ang)) => Ok((Units { len, ang }, missing)),
            _ => Err(UnitsFault::Missing(missing)),
        }
    }
}

/// A unit factor that reads as a size.
fn size(f: f64) -> bool {
    f.is_finite() && f > 0.0
}

/// The factor of an SI prefix (`$` or an unknown prefix: 1).
fn si_prefix(p: Option<&Param>) -> f64 {
    match p.and_then(Param::as_enum) {
        Some("EXA") => 1e18,
        Some("PETA") => 1e15,
        Some("TERA") => 1e12,
        Some("GIGA") => 1e9,
        Some("MEGA") => 1e6,
        Some("KILO") => 1e3,
        Some("HECTO") => 1e2,
        Some("DECA") => 1e1,
        Some("DECI") => 1e-1,
        Some("CENTI") => 1e-2,
        Some("MILLI") => 1e-3,
        Some("MICRO") => 1e-6,
        Some("NANO") => 1e-9,
        Some("PICO") => 1e-12,
        Some("FEMTO") => 1e-15,
        Some("ATTO") => 1e-18,
        _ => 1.0,
    }
}

/// What a unit measures.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UnitKind {
    /// A `LENGTH_UNIT`.
    Length,
    /// A `PLANE_ANGLE_UNIT`.
    Angle,
    /// Any other unit (solid angle, mass, …).
    Other,
}

/// A unit's kind and size: millimetres for lengths, radians for plane angles (1 for anything
/// else, or a unit that cannot be read). `None` when `#id` is not an entity, or is a
/// `CONVERSION_BASED_UNIT` whose measure cannot be read or is not finite and positive.
pub fn unit_factor(ex: &Exchange, id: u64) -> Option<(UnitKind, f64)> {
    factor(ex, id, 0)
}

fn factor(ex: &Exchange, id: u64, depth: usize) -> Option<(UnitKind, f64)> {
    if depth > MAX_CONVERSION_DEPTH {
        return None;
    }
    let e = ex.get(id)?;
    let kind = if e.has("LENGTH_UNIT") {
        UnitKind::Length
    } else if e.has("PLANE_ANGLE_UNIT") {
        UnitKind::Angle
    } else {
        UnitKind::Other
    };
    if let Some(si) = e.record("SI_UNIT") {
        let base = match si.get(1).and_then(Param::as_enum) {
            Some("METRE") => 1000.0,
            Some("RADIAN") => 1.0,
            _ => return Some((kind, 1.0)),
        };
        return Some((kind, si_prefix(si.first()) * base));
    }
    if let Some(cb) = e.record("CONVERSION_BASED_UNIT") {
        let m = ex.get(cb.get(1)?.as_ref_id()?)?;
        let mp = m.params();
        let v = mp.first()?.as_f64()?;
        let (_, f) = factor(ex, mp.get(1)?.as_ref_id()?, depth + 1)?;
        let x = v * f;
        return (x.is_finite() && x > 0.0).then_some((kind, x));
    }
    Some((kind, 1.0))
}

/// The units and uncertainty of the representation context `#id` (nothing named when `#id` is
/// not an entity or lists no readable unit).
///
/// Each unit `GLOBAL_UNIT_ASSIGNED_CONTEXT` lists that reads as a finite, positive length or
/// plane angle factor is named for its kind: [`Unit::Named`] for one factor (listed once or
/// several times alike), [`Unit::Conflicting`] for two different ones; a kind it names no
/// unit of is [`Unit::Missing`]. Each `UNCERTAINTY_MEASURE_WITH_UNIT` that
/// `GLOBAL_UNCERTAINTY_ASSIGNED_CONTEXT` lists with a readable value is kept for
/// [`Context::uncertainty`].
#[must_use]
pub fn context(ex: &Exchange, id: u64) -> Context {
    let mut c = Context::default();
    let Some(e) = ex.get(id) else { return c };
    if let Some(units) = e
        .record("GLOBAL_UNIT_ASSIGNED_CONTEXT")
        .and_then(<[Param]>::first)
        .and_then(Param::as_list)
    {
        for uid in units.iter().filter_map(Param::as_ref_id) {
            match unit_factor(ex, uid) {
                Some((UnitKind::Length, f)) if size(f) => c.length.name(f),
                Some((UnitKind::Angle, f)) if size(f) => c.angle.name(f),
                _ => {}
            }
        }
    }
    if let Some(unc) = e
        .record("GLOBAL_UNCERTAINTY_ASSIGNED_CONTEXT")
        .and_then(<[Param]>::first)
        .and_then(Param::as_list)
    {
        for uid in unc.iter().filter_map(Param::as_ref_id) {
            let Some(m) = ex.get(uid) else { continue };
            if let Some(v) = m.params().first().and_then(Param::as_f64) {
                let f = m
                    .params()
                    .get(1)
                    .and_then(Param::as_ref_id)
                    .and_then(|x| unit_factor(ex, x))
                    .map(|x| x.1)
                    .filter(|f| size(*f));
                c.distances.push((v, f));
            }
        }
    }
    c
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::f64::consts::PI;
    use std::fmt::Write as _;

    /// Millimetres, radians and steradians as SolidCraft's writer and its topology tests
    /// (`crates/step/src/topology_tests.rs`, `file`) write them.
    const MM: &str = "#1=(LENGTH_UNIT()NAMED_UNIT(*)SI_UNIT(.MILLI.,.METRE.));\
         #2=(NAMED_UNIT(*)PLANE_ANGLE_UNIT()SI_UNIT($,.RADIAN.));\
         #3=(NAMED_UNIT(*)SI_UNIT($,.STERADIAN.)SOLID_ANGLE_UNIT());";

    fn parse(data: &str) -> Exchange {
        let t = format!(
            "ISO-10303-21;\nHEADER;FILE_SCHEMA(('AUTOMOTIVE_DESIGN'));ENDSEC;\nDATA;\n{data}\nENDSEC;\nEND-ISO-10303-21;\n"
        );
        match crate::parse(&t) {
            Ok(p) => p.exchange,
            Err(e) => panic!("{}\n{t}", e.one_line()),
        }
    }

    const MM_RAD: Units = Units { len: 1.0, ang: 1.0 };

    /// The units of `c` when it names both, else the fault with no assumption.
    fn units_of(c: &Context) -> Units {
        c.units(None).expect("both named").0
    }

    /// A context `#100` with the units `units` and the uncertainties `unc` (entity ids).
    fn ctx(data: &str, units: &str, unc: &str) -> Context {
        let c = format!(
            "#100=(GEOMETRIC_REPRESENTATION_CONTEXT(3)GLOBAL_UNCERTAINTY_ASSIGNED_CONTEXT(({unc}))\
             GLOBAL_UNIT_ASSIGNED_CONTEXT(({units}))REPRESENTATION_CONTEXT('',''));"
        );
        context(&parse(&format!("{data}{c}")), 100)
    }

    #[test]
    fn solidcraft_file_context() {
        // SolidCraft's topology test file: mm, rad, sr and an uncertainty of 1e-7 mm.
        let c = ctx(
            &format!(
                "{MM}#4=UNCERTAINTY_MEASURE_WITH_UNIT(LENGTH_MEASURE(1.E-07),#1,'distance_accuracy_value','');"
            ),
            "#1,#2,#3",
            "#4",
        );
        assert_eq!(units_of(&c), MM_RAD);
        // 1e-7 times MILLI (1e-3) times METRE (1000 mm).
        assert_eq!(c.uncertainty(1.0).largest, Some(1e-7 * (1e-3 * 1000.0)));
    }

    #[test]
    fn si_prefixes() {
        for (prefix, mm) in [
            (".MILLI.", 1e-3 * 1000.0),
            ("$", 1000.0),
            (".MICRO.", 1e-6 * 1000.0),
            (".CENTI.", 1e-2 * 1000.0),
            (".KILO.", 1e3 * 1000.0),
        ] {
            let c = ctx(
                &format!("#1=(LENGTH_UNIT()NAMED_UNIT(*)SI_UNIT({prefix},.METRE.));"),
                "#1",
                "",
            );
            assert_eq!(c.length, Unit::Named(mm), "{prefix}");
            assert_eq!(c.angle, Unit::Missing, "{prefix}");
            assert_eq!(c.uncertainty(1.0).largest, None, "{prefix}");
        }
        // The products the table defines: a millimetre is exactly 1, a metre 1000.
        assert_eq!(1e-3 * 1000.0, 1.0);
        let ex = parse("#1=(LENGTH_UNIT()NAMED_UNIT(*)SI_UNIT(.MICRO.,.METRE.));");
        assert_eq!(unit_factor(&ex, 1), Some((UnitKind::Length, 1e-6 * 1000.0)));
        // A milliradian.
        let ex = parse("#1=(NAMED_UNIT(*)PLANE_ANGLE_UNIT()SI_UNIT(.MILLI.,.RADIAN.));");
        assert_eq!(unit_factor(&ex, 1), Some((UnitKind::Angle, 1e-3)));
    }

    #[test]
    fn inches_and_degrees() {
        // An inch as 25.4 mm, a degree as π/180 rad (written as the shortest decimal of
        // PI / 180, so it reads as exactly that double).
        let data = format!(
            "{MM}#10=LENGTH_MEASURE_WITH_UNIT(LENGTH_MEASURE(25.4),#1);\
             #11=(CONVERSION_BASED_UNIT('INCH',#10)LENGTH_UNIT()NAMED_UNIT(#12));\
             #12=DIMENSIONAL_EXPONENTS(1.,0.,0.,0.,0.,0.,0.);\
             #13=PLANE_ANGLE_MEASURE_WITH_UNIT(PLANE_ANGLE_MEASURE(0.017453292519943295),#2);\
             #14=(CONVERSION_BASED_UNIT('DEGREE',#13)NAMED_UNIT(#15)PLANE_ANGLE_UNIT());\
             #15=DIMENSIONAL_EXPONENTS(0.,0.,0.,0.,0.,0.,0.);"
        );
        let c = ctx(&data, "#11,#14,#3", "");
        assert_eq!(
            c.units(None).unwrap().0,
            Units {
                len: 25.4,
                ang: PI / 180.0
            }
        );
        // An inch defined from metres: 0.0254 times 1000 mm.
        let c = ctx(
            "#1=(LENGTH_UNIT()NAMED_UNIT(*)SI_UNIT($,.METRE.));\
             #10=LENGTH_MEASURE_WITH_UNIT(LENGTH_MEASURE(0.0254),#1);\
             #11=(CONVERSION_BASED_UNIT('INCH',#10)LENGTH_UNIT()NAMED_UNIT(*));",
            "#11",
            "",
        );
        assert_eq!(c.length, Unit::Named(0.0254 * 1000.0));
        // A foot defined from the inch: a chain of two conversions, 12 times 25.4 mm.
        let c = ctx(
            &format!(
                "{data}#20=LENGTH_MEASURE_WITH_UNIT(LENGTH_MEASURE(12.),#11);\
                 #21=(CONVERSION_BASED_UNIT('FOOT',#20)LENGTH_UNIT()NAMED_UNIT(*));"
            ),
            "#21",
            "",
        );
        assert_eq!(c.length, Unit::Named(12.0 * 25.4));
    }

    #[test]
    fn missing_or_broken_units_are_missing_not_mm_and_rad() {
        // No such context, or a context without units.
        assert_eq!(context(&parse(MM), 100), Context::default());
        assert_eq!(context(&parse(MM), 1), Context::default());
        assert_eq!(Context::default().length, Unit::Missing);
        assert_eq!(Context::default().angle, Unit::Missing);
        for (data, units) in [
            // Dangling references.
            ("", "#7,#8"),
            // A conversion whose measure is missing, unreadable, zero, negative or infinite.
            (
                "#1=(CONVERSION_BASED_UNIT('INCH',#9)LENGTH_UNIT()NAMED_UNIT(*));",
                "#1",
            ),
            (
                "#2=(NAMED_UNIT(*)SI_UNIT(.MILLI.,.METRE.)LENGTH_UNIT());\
                 #3=LENGTH_MEASURE_WITH_UNIT($,#2);\
                 #1=(CONVERSION_BASED_UNIT('INCH',#3)LENGTH_UNIT()NAMED_UNIT(*));",
                "#1",
            ),
            (
                "#2=(NAMED_UNIT(*)SI_UNIT(.MILLI.,.METRE.)LENGTH_UNIT());\
                 #3=LENGTH_MEASURE_WITH_UNIT(LENGTH_MEASURE(0.),#2);\
                 #1=(CONVERSION_BASED_UNIT('INCH',#3)LENGTH_UNIT()NAMED_UNIT(*));",
                "#1",
            ),
            (
                "#2=(NAMED_UNIT(*)PLANE_ANGLE_UNIT()SI_UNIT($,.RADIAN.));\
                 #3=PLANE_ANGLE_MEASURE_WITH_UNIT(PLANE_ANGLE_MEASURE(-1.),#2);\
                 #1=(CONVERSION_BASED_UNIT('DEGREE',#3)PLANE_ANGLE_UNIT()NAMED_UNIT(*));",
                "#1",
            ),
            (
                "#2=(NAMED_UNIT(*)SI_UNIT(.EXA.,.METRE.)LENGTH_UNIT());\
                 #3=LENGTH_MEASURE_WITH_UNIT(LENGTH_MEASURE(1.E300),#2);\
                 #1=(CONVERSION_BASED_UNIT('BIG',#3)LENGTH_UNIT()NAMED_UNIT(*));",
                "#1",
            ),
            // A conversion that refers to itself: the depth bound ends it.
            (
                "#3=LENGTH_MEASURE_WITH_UNIT(LENGTH_MEASURE(2.),#1);\
                 #1=(CONVERSION_BASED_UNIT('LOOP',#3)LENGTH_UNIT()NAMED_UNIT(*));",
                "#1",
            ),
            // Units that are neither lengths nor plane angles.
            (MM, "#3"),
            // An empty list.
            (MM, ""),
        ] {
            let c = ctx(data, units, "");
            assert_eq!(c.length, Unit::Missing, "{data} / {units}");
            assert_eq!(c.angle, Unit::Missing, "{data} / {units}");
            // Nothing is defaulted: only an assumption the caller states is used.
            assert_eq!(
                c.units(None),
                Err(UnitsFault::Missing(MissingUnits {
                    length: true,
                    angle: true
                }))
            );
        }
        // An SI unit with an unknown base is read as factor 1 (`unit_factor`'s own guess, a
        // residue of the port: research 74 §23 lists it), so the unit is named.
        let c = ctx(
            "#1=(LENGTH_UNIT()NAMED_UNIT(*)SI_UNIT(.MILLI.,.FURLONG.));",
            "#1",
            "",
        );
        assert_eq!(c.length, Unit::Named(1.0));
        // A unit list that is not a list.
        let ex = parse(&format!(
            "{MM}#100=(GEOMETRIC_REPRESENTATION_CONTEXT(3)GLOBAL_UNIT_ASSIGNED_CONTEXT(#1)REPRESENTATION_CONTEXT('',''));"
        ));
        assert_eq!(context(&ex, 100), Context::default());
        // A broken length unit leaves the readable angle unit in place.
        let c = ctx(
            "#2=(NAMED_UNIT(*)PLANE_ANGLE_UNIT()SI_UNIT(.MILLI.,.RADIAN.));\
             #1=(CONVERSION_BASED_UNIT('INCH',#9)LENGTH_UNIT()NAMED_UNIT(*));",
            "#1,#2",
            "",
        );
        assert_eq!(c.length, Unit::Missing);
        assert_eq!(c.angle, Unit::Named(1e-3));
        // The caller's assumption fills only what is missing.
        let inch = Units {
            len: 25.4,
            ang: 0.5,
        };
        assert_eq!(
            c.units(Some(inch)),
            Ok((
                Units {
                    len: 25.4,
                    ang: 1e-3
                },
                MissingUnits {
                    length: true,
                    angle: false
                }
            ))
        );
    }

    #[test]
    fn two_different_units_conflict() {
        let c = ctx(
            "#1=(LENGTH_UNIT()NAMED_UNIT(*)SI_UNIT(.MILLI.,.METRE.));\
             #2=(LENGTH_UNIT()NAMED_UNIT(*)SI_UNIT($,.METRE.));\
             #3=(NAMED_UNIT(*)PLANE_ANGLE_UNIT()SI_UNIT($,.RADIAN.));",
            "#1,#2,#3,#3",
            "",
        );
        assert_eq!(c.length, Unit::Conflicting);
        // The same unit listed twice is one unit.
        assert_eq!(c.angle, Unit::Named(1.0));
        let fault = UnitsFault::Conflicting(MissingUnits {
            length: true,
            angle: false,
        });
        assert_eq!(c.units(None), Err(fault));
        assert_eq!(c.units(Some(Units::default())), Err(fault));
    }

    #[test]
    fn conversion_depth_is_bounded() {
        // #1 is millimetres; #2..=#7 each 2 of the one before, so #(1+k) is 2^k mm and
        // MAX_CONVERSION_DEPTH links are followed from the unit asked for.
        let mut data = String::from("#1=(LENGTH_UNIT()NAMED_UNIT(*)SI_UNIT(.MILLI.,.METRE.));");
        for k in 2..=7 {
            write!(
                data,
                "#{m}=LENGTH_MEASURE_WITH_UNIT(LENGTH_MEASURE(2.),#{p});\
                 #{k}=(CONVERSION_BASED_UNIT('U',#{m})LENGTH_UNIT()NAMED_UNIT(*));",
                m = 100 + k,
                p = k - 1
            )
            .unwrap();
        }
        let ex = parse(&data);
        assert_eq!(unit_factor(&ex, 5), Some((UnitKind::Length, 16.0)));
        assert_eq!(unit_factor(&ex, 6), None);
    }

    #[test]
    fn uncertainty_in_file_units() {
        let inch = format!(
            "{MM}#10=LENGTH_MEASURE_WITH_UNIT(LENGTH_MEASURE(25.4),#1);\
             #11=(CONVERSION_BASED_UNIT('INCH',#10)LENGTH_UNIT()NAMED_UNIT(*));"
        );
        // 1e-4 inch is 1e-4 * 25.4 mm.
        let c = ctx(
            &format!("{inch}#20=UNCERTAINTY_MEASURE_WITH_UNIT(LENGTH_MEASURE(1.E-04),#11,'d','');"),
            "#11,#2",
            "#20",
        );
        assert_eq!(c.length, Unit::Named(25.4));
        assert_eq!(c.uncertainty(25.4).largest, Some(1e-4 * 25.4));
        // A measure in metres in an inch context: its own unit, 1e-6 * 1000 mm.
        let c = ctx(
            &format!(
                "{inch}#12=(LENGTH_UNIT()NAMED_UNIT(*)SI_UNIT($,.METRE.));\
                 #20=UNCERTAINTY_MEASURE_WITH_UNIT(LENGTH_MEASURE(1.E-06),#12,'d','');"
            ),
            "#11",
            "#20",
        );
        assert_eq!(c.uncertainty(25.4).largest, Some(1e-6 * 1000.0));
        // A measure whose unit cannot be read is in the context's length unit.
        let c = ctx(
            &format!("{inch}#20=UNCERTAINTY_MEASURE_WITH_UNIT(LENGTH_MEASURE(1.E-04),$,'d','');"),
            "#11",
            "#20",
        );
        assert_eq!(c.uncertainty(25.4).largest, Some(1e-4 * 25.4));
        // And the unit the caller assumes when the context names no length unit.
        let c = ctx(
            "#20=UNCERTAINTY_MEASURE_WITH_UNIT(LENGTH_MEASURE(1.E-04),$,'d','');",
            "",
            "#20",
        );
        assert_eq!(c.length, Unit::Missing);
        assert_eq!(c.uncertainty(25.4).largest, Some(1e-4 * 25.4));
        // The largest one listed is the stated accuracy, and several is said.
        let c = ctx(
            &format!(
                "{MM}#20=UNCERTAINTY_MEASURE_WITH_UNIT(LENGTH_MEASURE(2.E-04),#1,'d','');\
                 #21=UNCERTAINTY_MEASURE_WITH_UNIT(LENGTH_MEASURE(1.E-04),#1,'d','');"
            ),
            "#1",
            "#20,#21",
        );
        let u = c.uncertainty(1.0);
        assert_eq!(u.largest, Some(2e-4 * (1e-3 * 1000.0)));
        assert!(u.several);
        // Equal ones are one.
        let c = ctx(
            &format!(
                "{MM}#20=UNCERTAINTY_MEASURE_WITH_UNIT(LENGTH_MEASURE(1.E-04),#1,'d','');\
                 #21=UNCERTAINTY_MEASURE_WITH_UNIT(LENGTH_MEASURE(1.E-04),#1,'d','');"
            ),
            "#1",
            "#20,#21",
        );
        assert!(!c.uncertainty(1.0).several);
    }

    #[test]
    fn uncertainty_zero_negative_or_infinite_is_none() {
        for v in ["0.", "-1.E-04", "1.E300"] {
            // In exametres (1e21 mm): 0, negative, and 1e300 * 1e21 mm, which is infinite.
            let c = ctx(
                &format!(
                    "{MM}#12=(LENGTH_UNIT()NAMED_UNIT(*)SI_UNIT(.EXA.,.METRE.));\
                     #20=UNCERTAINTY_MEASURE_WITH_UNIT(LENGTH_MEASURE({v}),#12,'d','');"
                ),
                "#1",
                "#20",
            );
            assert_eq!(c.uncertainty(1.0).largest, None, "{v}");
        }
        // A bad one beside a good one is left out.
        let c = ctx(
            &format!(
                "{MM}#20=UNCERTAINTY_MEASURE_WITH_UNIT(LENGTH_MEASURE(1.E-04),#1,'d','');\
                 #21=UNCERTAINTY_MEASURE_WITH_UNIT(LENGTH_MEASURE(0.),#1,'d','');"
            ),
            "#1",
            "#20,#21",
        );
        assert_eq!(c.uncertainty(1.0).largest, Some(1e-4 * (1e-3 * 1000.0)));
        // One without a readable value, or a dangling reference, is skipped.
        let c = ctx(
            &format!(
                "{MM}#20=UNCERTAINTY_MEASURE_WITH_UNIT(LENGTH_MEASURE(1.E-04),#1,'d','');\
                 #21=UNCERTAINTY_MEASURE_WITH_UNIT($,#1,'d','');"
            ),
            "#1",
            "#20,#21,#99",
        );
        assert_eq!(c.uncertainty(1.0).largest, Some(1e-4 * (1e-3 * 1000.0)));
    }
}
