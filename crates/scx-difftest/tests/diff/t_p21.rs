// Written for solidcorex. Apache-2.0.
//! scx-p21 against the pin's `solidcore-record` STEP reader: the Part 21 grammar on edge
//! cases, then every file of a corpus through the reader, the units, the protocol, the
//! Part 42 reader and the product structure.
//!
//! The edge cases go through the harness: one pool the two readers agree on, and one pool
//! per pin defect (`pin_defects.rs`), each with controls beside the defective inputs. An
//! exchange is compared by a canonical form: the header's fields, the schemas, every
//! instance in id order (each record's name and parameters, a real by its bits) and the
//! duplicated ids. A refusal is compared only as a refusal: the two readers' messages
//! differ by design.
//!
//! The corpus is the pin's STEP fixtures (beside this repository, as the pin is), plus,
//! when `SCX_P21_CORPUS` names a directory, every `.stp` and `.step` file under it (the NIST
//! PMI models are the one used so far). Each category prints a harness line; a mismatch in
//! any fails the test, and scx is run twice on every file to check it is deterministic.

#![allow(
    clippy::pedantic,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use crate::harness::{Bits, Gen, Out, SplitMix64, run, run_defect};
use crate::pin_defects;

use scx_p21 as x;
use solidcore_record::step::p21 as s;

// ---- the canonical form ------------------------------------------------------------------

/// A parameter, in a form both readers' parameters map to.
enum Cp {
    Int(i64),
    Real(f64),
    Str(String),
    Enum(String),
    Ref(u64),
    List(Vec<Cp>),
    Typed(String, Vec<Cp>),
    Binary(String),
    Unset,
    Derived,
}

impl Bits for Cp {
    fn bits(&self, o: &mut Out) {
        match self {
            Cp::Int(i) => {
                o.tag("Int");
                i.bits(o);
            }
            Cp::Real(r) => {
                o.tag("Real");
                r.bits(o);
            }
            Cp::Str(t) => {
                o.tag("Str");
                o.tag(t);
            }
            Cp::Enum(t) => {
                o.tag("Enum");
                o.tag(t);
            }
            Cp::Ref(id) => {
                o.tag("Ref");
                id.bits(o);
            }
            Cp::List(v) => {
                o.tag("List");
                v.bits(o);
            }
            Cp::Typed(n, v) => {
                o.tag("Typed");
                o.tag(n);
                v.bits(o);
            }
            Cp::Binary(t) => {
                o.tag("Binary");
                o.tag(t);
            }
            Cp::Unset => o.tag("Unset"),
            Cp::Derived => o.tag("Derived"),
        }
    }
}

fn cx(p: &x::Param) -> Cp {
    use x::Param as P;
    match p {
        P::Int(i) => Cp::Int(*i),
        P::Real(r) => Cp::Real(*r),
        P::Str(t) => Cp::Str(t.clone()),
        P::Enum(t) => Cp::Enum(t.clone()),
        P::Ref(id) => Cp::Ref(*id),
        P::List(v) => Cp::List(v.iter().map(cx).collect()),
        // ours holds exactly one value; the pin's form is a list of them
        P::Typed { name, value } => Cp::Typed(name.clone(), vec![cx(value)]),
        P::Binary(t) => Cp::Binary(t.clone()),
        P::Unset => Cp::Unset,
        P::Derived => Cp::Derived,
    }
}

fn cs(p: &s::Param) -> Cp {
    use s::Param as P;
    match p {
        P::Int(i) => Cp::Int(*i),
        P::Real(r) => Cp::Real(*r),
        P::Str(t) => Cp::Str(t.clone()),
        P::Enum(t) => Cp::Enum(t.clone()),
        P::Ref(id) => Cp::Ref(*id),
        P::List(v) => Cp::List(v.iter().map(cs).collect()),
        P::Typed(n, v) => Cp::Typed(n.clone(), v.iter().map(cs).collect()),
        P::Binary(t) => Cp::Binary(t.clone()),
        P::Null => Cp::Unset,
        P::Derived => Cp::Derived,
    }
}

/// One instance's records: each its name and parameters.
type Records = Vec<(String, Vec<Cp>)>;

/// An exchange's canonical form.
struct Canon {
    header: Vec<Vec<String>>,
    /// Each instance: its id and its records (name, parameters).
    instances: Vec<(u64, Records)>,
    duplicates: Vec<u64>,
}

impl Bits for Canon {
    fn bits(&self, o: &mut Out) {
        for field in &self.header {
            field.iter().map(String::as_str).collect::<Vec<_>>().bits(o);
        }
        o.bits
            .push(crate::harness::Item::I(self.instances.len() as u64));
        for (id, records) in &self.instances {
            id.bits(o);
            records.len().bits(o);
            for (name, params) in records {
                o.tag(name);
                params.bits(o);
            }
        }
        self.duplicates.bits(o);
    }
}

fn canon_x(p: &x::Parsed) -> Canon {
    let ex = &p.exchange;
    let fd = ex.header.file_description().unwrap_or_default();
    let f = ex.header.file_name().unwrap_or_default();
    let one = |s: &String| vec![s.clone()];
    let ids: BTreeSet<u64> = ex.instances().map(|(id, _)| id).collect();
    let mut duplicates: Vec<u64> = p
        .warnings
        .iter()
        .filter_map(|w| match w {
            x::Warning::DuplicateId { id, .. } => Some(*id),
            _ => None,
        })
        .collect();
    duplicates.sort_unstable();
    Canon {
        header: vec![
            fd.description.clone(),
            one(&fd.implementation_level),
            one(&f.name),
            one(&f.time_stamp),
            f.author.clone(),
            f.organization.clone(),
            one(&f.preprocessor_version),
            one(&f.originating_system),
            one(&f.authorization),
            ex.header.schemas(),
        ],
        instances: ids
            .iter()
            .map(|id| {
                let e = ex.get(*id).unwrap();
                let records = e
                    .records()
                    .iter()
                    .map(|r| (r.name.clone(), r.params.iter().map(cx).collect()))
                    .collect();
                (*id, records)
            })
            .collect(),
        duplicates,
    }
}

fn canon_s(p: &s::Exchange) -> Canon {
    let h = &p.header;
    let one = |s: &String| vec![s.clone()];
    let mut duplicates = p.duplicates.clone();
    duplicates.sort_unstable();
    Canon {
        header: vec![
            h.descriptions.clone(),
            one(&h.implementation_level),
            one(&h.file_name),
            one(&h.time_stamp),
            h.authors.clone(),
            h.organizations.clone(),
            one(&h.preprocessor_version),
            one(&h.originating_system),
            one(&h.authorization),
            p.schemas.clone(),
        ],
        instances: p
            .entities
            .iter()
            .map(|(id, e)| {
                let records = e
                    .records
                    .iter()
                    .map(|r| (r.name.clone(), r.params.iter().map(cs).collect()))
                    .collect();
                (*id, records)
            })
            .collect(),
        duplicates,
    }
}

/// scx's read, fingerprinted by its canonical form (a refusal as `Err` alone).
pub struct XRead(pub Result<x::Parsed, x::P21Error>);

impl Bits for XRead {
    fn bits(&self, o: &mut Out) {
        self.0.as_ref().map(canon_x).map_err(|_| ()).bits(o);
    }
}

fn fp_x(r: &Result<x::Parsed, x::P21Error>) -> Out {
    let mut o = Out::default();
    r.as_ref().map(canon_x).map_err(|_| ()).bits(&mut o);
    o
}

/// The pin's read, fingerprinted the same way.
struct SRead(Result<s::Exchange, s::Error>);

impl Bits for SRead {
    fn bits(&self, o: &mut Out) {
        self.0.as_ref().map(canon_s).map_err(|_| ()).bits(o);
    }
}

// ---- the edge cases ------------------------------------------------------------------------

/// A DATA section's body from pool `P`, read inside a minimal valid header.
#[derive(Clone, Debug)]
pub struct Text<const P: usize>(pub &'static str);

impl<const P: usize> Text<P> {
    pub fn file(&self) -> String {
        format!(
            "ISO-10303-21;\nHEADER;\nFILE_DESCRIPTION((''),'2;1');\n\
             FILE_NAME('','',(''),(''),'','','');\nFILE_SCHEMA(('AUTOMOTIVE_DESIGN'));\n\
             ENDSEC;\nDATA;\n{}\nENDSEC;\nEND-ISO-10303-21;\n",
            self.0
        )
    }
}

impl<const P: usize> Gen for Text<P> {
    fn pool(_: &mut SplitMix64) -> Vec<Self> {
        POOLS[P].iter().map(|t| Text(t)).collect()
    }
}

pub const AGREE: usize = 0;
pub const EMPTY_COMPLEX: usize = 1;
pub const TYPED_MANY: usize = 2;
pub const TYPED_NONE: usize = 3;
pub const ENUM_DASH: usize = 4;
pub const PAGE_LATIN: usize = 5;
pub const LOWER_HEX: usize = 6;

const POOLS: [&[&str]; 7] = [
    // AGREE: the grammar's edges on which the two readers agree
    &[
        "#1=(A(1));",
        r"#1=A(1.);",
        r"#1=A(1.e3);",
        r"#1=A(.5);",
        r"#1=A(1E3);",
        r"#1=A(+1);",
        r"#1=A(# 1);",
        r"#1=A('it''s');",
        r"#1=A('a\\b');",
        r"#1=A('\X2\00E9\X0\');",
        r"#1=A('\X4\0001F642\X0\');",
        r"#1=A('\X\E9');",
        r"#1=A('\S\a');",
        r#"#1=A("0F");"#,
        r"#1=A(1/*x*/,2);",
        r"#1=A(1);/* x",
        r"#1=a(1);",
        r"#1=!A(1);",
        r"#0=A(1);",
        r"#99999999999999999999=A(1);",
        r"#1=A(99999999999999999999);",
        r"#1=A(1.E400);",
        r"#1=A(1);#1=B(2);",
        r"#1=A(1)",
        r"#1=A(1,);",
        r"#1=A();",
        r"#1=A(((1,2),(3)));",
        r"#1=A($,*);",
        r"#1=CRTPNT('',(0.,0.,0.));",
    ],
    // EMPTY_COMPLEX
    &["#1=();", "#1=A(1);#2=();", "#1=(A(1));", "#1=(A(1)B(2));"],
    // TYPED_MANY
    &[
        "#1=A(B(1,2));",
        "#1=A(B(1,2,3));",
        "#1=A(B(1));",
        "#1=A(B((1,2)));",
        "#1=A((1,2));",
    ],
    // TYPED_NONE
    &[
        "#1=A(B());",
        "#1=A(1,B());",
        "#1=A(B(1));",
        "#1=A(());",
        "#1=A();",
    ],
    // ENUM_DASH
    &[
        "#1=A(.A-B.);",
        "#1=A(.X-1.);",
        "#1=A(.A_B.);",
        "#1=A(.T.,1.E-3);",
    ],
    // PAGE_LATIN
    &[
        r"#1=A('\PA\\S\a');",
        r"#1=A('x\PA\\S\ay');",
        r"#1=A('\S\a');",
        r"#1=A('\PA\abc');",
    ],
    // LOWER_HEX
    &[
        r#"#1=A("0f");"#,
        r#"#1=A("3ab");"#,
        r#"#1=A("0Ff");"#,
        r#"#1=A("0F");"#,
        r#"#1=A("0");"#,
        r#"#1=A("3AB");"#,
    ],
];

fn xread<const P: usize>((t,): (Text<P>,)) -> XRead {
    XRead(x::parse(&t.file()))
}

fn sread<const P: usize>((t,): (Text<P>,)) -> SRead {
    SRead(s::parse(&t.file()))
}

#[test]
fn p21_grammar() {
    run("t_p21::p21_grammar", xread::<AGREE>, sread::<AGREE>);
}

macro_rules! defect_test {
    ($name:ident, $pool:ident) => {
        #[test]
        fn $name() {
            run_defect(
                concat!("t_p21::", stringify!($name)),
                &pin_defects::$name(),
                xread::<$pool>,
                sread::<$pool>,
            );
        }
    };
}

defect_test!(p21_empty_complex, EMPTY_COMPLEX);
defect_test!(p21_typed_many, TYPED_MANY);
defect_test!(p21_typed_none, TYPED_NONE);
defect_test!(p21_enum_dash, ENUM_DASH);
defect_test!(p21_page_latin, PAGE_LATIN);
defect_test!(p21_lower_hex, LOWER_HEX);

// ---- the corpus ----------------------------------------------------------------------------

/// The pin's STEP fixtures, beside this repository as the pin is (the Discord-origin
/// fixtures are left out).
fn fixtures() -> Vec<PathBuf> {
    let pin = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../sc-wt/scx-pin/solidcore");
    let mut v: Vec<PathBuf> = std::fs::read_dir(pin.join("crates/solidcore-step/tests/fixtures"))
        .expect("the pin's solidcore-step fixtures")
        .map(|e| e.unwrap().path())
        .filter(|p| is_step(p) && !name_of(p).starts_with("discord-"))
        .collect();
    v.push(pin.join("crates/step-export/src/tetra_disk.stp"));
    v.push(pin.join("crates/solidcore-ops/tests/data/assembly_extract.stp"));
    v.sort();
    v
}

fn is_step(p: &Path) -> bool {
    matches!(
        p.extension().and_then(|e| e.to_str()),
        Some("stp" | "step" | "STP" | "STEP")
    )
}

fn name_of(p: &Path) -> String {
    p.file_name().unwrap().to_string_lossy().into_owned()
}

/// One category's counts, printed as a harness line.
#[derive(Default)]
struct Tally {
    exact: usize,
    within: usize,
    bad: Vec<String>,
    unstable: Vec<String>,
}

impl Tally {
    fn take(&mut self, same: bool, close: bool, what: impl FnOnce() -> String) {
        if same {
            self.exact += 1;
        } else if close {
            self.within += 1;
        } else {
            self.bad.push(what());
        }
    }
    fn report(&self, name: &str) {
        println!(
            "difftest t_p21::corpus_{name}: {} inputs, {} exact, {} within tolerance, 0 pin defects, {} mismatches, {} nondeterministic",
            self.exact + self.within + self.bad.len(),
            self.exact,
            self.within,
            self.bad.len(),
            self.unstable.len()
        );
    }
}

#[derive(Default)]
struct Corpus {
    files: Tally,
    entities: Tally,
    units: Tally,
    contexts: Tally,
    protocols: Tally,
    part42: Tally,
    part42_refused: usize,
    products: Tally,
    /// The pin's unit defects (research 74 §23): where the file leaves a unit out, or lists two,
    /// or states several uncertainties, scx says so and the pin reads mm / rad / the last one.
    pin_units: PinUnits,
}

/// How often the pin's reading of a context is a defect rather than a disagreement.
#[derive(Default)]
struct PinUnits {
    /// A unit the context names none of (scx `Unit::Missing`): the pin reads mm or radians.
    defaulted: usize,
    /// Two different units of one kind (scx `Unit::Conflicting`): the pin keeps the last.
    conflicting: usize,
    /// Several uncertainties (scx keeps the largest): the pin keeps the last.
    last_wins: usize,
    /// A product read scx refuses for want of a length unit, which the pin reads in mm.
    refused: usize,
}

/// The unit the pin reads where scx reads `u`: the file's, or the pin's default (millimetres,
/// radians: `default`, written out here at the comparison) where the file names none, counted
/// as the pin's defect. `None` for two different units, which has no pin value to compare (the
/// pin keeps the last), also counted.
fn pin_unit(u: x::units::Unit, default: f64, n: &mut PinUnits) -> Option<f64> {
    match u {
        x::units::Unit::Named(f) => Some(f),
        x::units::Unit::Missing => {
            n.defaulted += 1;
            Some(default)
        }
        x::units::Unit::Conflicting => {
            n.conflicting += 1;
            None
        }
    }
}

/// The pin's reading of the context scx reads as `c` (its units, its uncertainty), with
/// `None` for a part that is the pin's defect and cannot be compared.
struct PinView {
    len: Option<f64>,
    ang: Option<f64>,
    /// Outer `None`: not compared (several uncertainties, or no length unit to read them in).
    uncertainty: Option<Option<f64>>,
}

fn pin_view(c: &x::units::Context, n: &mut PinUnits) -> PinView {
    let d = x::units::Units::default();
    let len = pin_unit(c.length, d.len, n);
    let ang = pin_unit(c.angle, d.ang, n);
    let uncertainty = len.and_then(|l| {
        let u = c.uncertainty(l);
        if u.several {
            n.last_wins += 1;
            None
        } else {
            Some(u.largest)
        }
    });
    PinView { len, ang, uncertainty }
}

fn same_f(a: f64, b: f64) -> bool {
    a.to_bits() == b.to_bits() || (a.is_nan() && b.is_nan())
}

fn close(a: f64, b: f64) -> bool {
    crate::harness::Tol::DEFAULT.floats_agree(a, b)
}

fn fingerprint(b: &impl Bits) -> Out {
    let mut o = Out::default();
    b.bits(&mut o);
    o
}

/// Every Part 42 read of the exchange, by kind of entity, as text (a refusal as `Err`).
fn part42_x(ex: &x::Exchange, ids: &BTreeSet<u64>) -> Vec<(u64, String, Result<String, ()>)> {
    let ctx = ids
        .iter()
        .find(|id| ex.get(**id).unwrap().has("GLOBAL_UNIT_ASSIGNED_CONTEXT"));
    // The pin reads a missing unit as its default; the same is written out here, so both
    // readers read the geometry in the same units (a conflict names no unit: the default).
    let d = x::units::Units::default();
    let units = ctx.map_or(d, |id| {
        let c = x::units::context(ex, *id);
        x::units::Units {
            len: c.length.named().unwrap_or(d.len),
            ang: c.angle.named().unwrap_or(d.ang),
        }
    });
    let mut r = x::part42::read::Reader::new(ex, units);
    ids.iter()
        .filter_map(|id| {
            let n = ex.get(*id).unwrap().name().to_owned();
            let got = match kind(&n)? {
                Kind::Point => r.point(*id).map(|v| format!("{v:?}")),
                Kind::Curve => r.curve(*id).map(|v| format!("{v:?}")),
                Kind::Surface => r.surface(*id).map(|v| format!("{v:?}")),
                Kind::Pcurve => r.pcurve(*id).map(|v| format!("{v:?}")),
            };
            Some((*id, n, got.map_err(|_| ())))
        })
        .collect()
}

fn part42_s(p: &s::Exchange, ids: &BTreeSet<u64>) -> Vec<Result<String, ()>> {
    use solidcore_record::step::{read::Reader, units};
    let ctx = ids.iter().find(|id| {
        p.get(**id)
            .unwrap()
            .records
            .iter()
            .any(|r| r.name == "GLOBAL_UNIT_ASSIGNED_CONTEXT")
    });
    let u = ctx.map_or_else(Default::default, |id| units::context(p, *id).units);
    let mut r = Reader::new(p, u);
    ids.iter()
        .filter_map(|id| {
            let e = p.get(*id).unwrap();
            let got = match kind(&e.records[0].name)? {
                Kind::Point => r.point(*id).map(|v| format!("{v:?}")),
                Kind::Curve => r.curve(*id).map(|v| format!("{v:?}")),
                Kind::Surface => r.surface(*id).map(|v| format!("{v:?}")),
                Kind::Pcurve => r.pcurve(*id).map(|v| format!("{v:?}")),
            };
            Some(got.map_err(|_| ()))
        })
        .collect()
}

enum Kind {
    Point,
    Curve,
    Surface,
    Pcurve,
}

fn kind(n: &str) -> Option<Kind> {
    if n == "CARTESIAN_POINT" {
        Some(Kind::Point)
    } else if n == "PCURVE" {
        Some(Kind::Pcurve)
    } else if n.contains("SURFACE") && !n.contains("CURVE") || n == "PLANE" {
        Some(Kind::Surface)
    } else if [
        "LINE",
        "CIRCLE",
        "ELLIPSE",
        "PARABOLA",
        "HYPERBOLA",
        "POLYLINE",
    ]
    .contains(&n)
        || n.contains("CURVE")
    {
        Some(Kind::Curve)
    } else {
        None
    }
}

/// scx's product structure as text in the pin's shape, with each representation's context
/// written as the pin reads it (a missing unit as the pin's default). `Err(true)`:
/// refused for want of a length unit, which the pin reads in millimetres; `Err(false)`: any
/// other refusal. `Ok((text, comparable))`: `comparable` is false where the pin's reading of a
/// context is its defect with no value to compare (conflicting units, several uncertainties).
fn product_x(ex: &x::Exchange, n: &mut PinUnits) -> Result<(String, bool), bool> {
    let ps = x::product::read(ex).map_err(|e| format!("{e:?}").contains("Units("))?;
    let mut text = format!("{ps:?}");
    let mut comparable = true;
    for r in &ps.representations {
        let v = pin_view(&r.units, n);
        let (Some(len), Some(ang), Some(unc)) = (v.len, v.ang, v.uncertainty) else {
            comparable = false;
            continue;
        };
        let pin_form = format!(
            "Context {{ units: Units {{ len: {len:?}, ang: {ang:?} }}, uncertainty: {unc:?} }}"
        );
        text = text.replace(&format!("{:?}", r.units), &pin_form);
    }
    Ok((text, comparable))
}

fn compare_file(name: &str, text: &str, c: &mut Corpus) {
    let ox = x::parse(text);
    let ox2 = x::parse(text);
    let os = s::parse(text);
    if fp_x(&ox) != fp_x(&ox2) {
        c.files.unstable.push(name.to_owned());
    }
    let (o, p) = match (ox, os) {
        (Ok(o), Ok(p)) => (o, p),
        (Err(_), Err(_)) => return c.files.take(true, true, String::new),
        (o, p) => {
            return c.files.take(false, false, || {
                format!(
                    "{name}: scx {:?}, pin {:?}",
                    o.err().map(|e| e.one_line().to_string()),
                    p.err().map(|e| e.to_string())
                )
            });
        }
    };
    let (cx_, cs_) = (canon_x(&o), canon_s(&p));
    let (fx, fs) = (fingerprint(&cx_), fingerprint(&cs_));
    c.files.take(fx == fs, false, || {
        format!("{name}: the header, schemas or duplicates differ")
    });
    // each instance on its own, so one difference names its id
    let ids: BTreeSet<u64> = cx_.instances.iter().map(|(id, _)| *id).collect();
    let sids: BTreeSet<u64> = cs_.instances.iter().map(|(id, _)| *id).collect();
    for id in ids.symmetric_difference(&sids) {
        c.entities
            .bad
            .push(format!("{name}: #{id} is read by one reader only"));
    }
    let pin: std::collections::BTreeMap<u64, &[(String, Vec<Cp>)]> = cs_
        .instances
        .iter()
        .map(|(id, r)| (*id, r.as_slice()))
        .collect();
    for (id, a) in &cx_.instances {
        let Some(b) = pin.get(id) else { continue };
        let (a, b) = (fingerprint(&Rec(a)), fingerprint(&Rec(b)));
        c.entities.take(a == b, false, || {
            format!("{name}: #{id}: scx {a:?}\n  pin {b:?}")
        });
    }
    let ex = &o.exchange;
    for id in &ids {
        let e = ex.get(*id).unwrap();
        if e.records().iter().any(|r| r.name.ends_with("_UNIT")) {
            let a = x::units::unit_factor(ex, *id);
            let b = solidcore_record::step::units::unit_factor(&p, *id);
            let ka = a.map(|(k, f)| (format!("{k:?}"), f));
            let kb = b.map(|(k, f)| (format!("{k:?}"), f));
            let (same, near) = match (&ka, &kb) {
                (Some((k1, f1)), Some((k2, f2))) => {
                    (k1 == k2 && same_f(*f1, *f2), k1 == k2 && close(*f1, *f2))
                }
                (None, None) => (true, true),
                _ => (false, false),
            };
            c.units.take(same, near, || {
                format!("{name}: unit #{id}: scx {ka:?} pin {kb:?}")
            });
        }
        if e.has("GLOBAL_UNIT_ASSIGNED_CONTEXT") || e.has("GLOBAL_UNCERTAINTY_ASSIGNED_CONTEXT") {
            let a = x::units::context(ex, *id);
            let b = solidcore_record::step::units::context(&p, *id);
            let v = pin_view(&a, &mut c.pin_units);
            let eq = |f: fn(f64, f64) -> bool| {
                // A part that is the pin's defect is not compared; a missing unit is compared
                // with the pin's default.
                v.len.is_none_or(|l| f(l, b.units.len))
                    && v.ang.is_none_or(|g| f(g, b.units.ang))
                    && v.uncertainty.is_none_or(|u| match (u, b.uncertainty) {
                        (Some(u), Some(w)) => f(u, w),
                        (None, None) => true,
                        _ => false,
                    })
            };
            c.contexts.take(eq(same_f), eq(close), || {
                format!("{name}: context #{id}: scx {a:?} pin {b:?}")
            });
        }
    }
    let a = x::protocol::of_exchange(ex);
    let b = solidcore_record::step::protocol::declared(&p.schemas);
    c.protocols.take(
        a.protocol.to_string() == b.protocol.to_string()
            && a.doubt.as_ref().map(ToString::to_string) == b.warning,
        false,
        || format!("{name}: protocol: scx {a:?} pin {b:?}"),
    );
    let gx = part42_x(ex, &ids);
    if gx != part42_x(ex, &ids) {
        c.part42.unstable.push(name.to_owned());
    }
    let gs = part42_s(&p, &ids);
    for ((id, n, a), b) in gx.iter().zip(&gs) {
        match (a, b) {
            (Err(()), Err(())) => c.part42_refused += 1,
            _ => c.part42.take(a == b, false, || {
                let cut =
                    |r: &Result<String, ()>| format!("{r:?}").chars().take(400).collect::<String>();
                format!("{name}: #{id} {n}: scx {}\n  pin {}", cut(a), cut(b))
            }),
        }
    }
    // The contexts' own tally counts the pin's unit defects; these reads are not counted again.
    let a = product_x(ex, &mut PinUnits::default());
    if a != product_x(ex, &mut PinUnits::default()) {
        c.products.unstable.push(name.to_owned());
    }
    let b = solidcore_record::step::product::read(&p)
        // the pin names its node `Instance`; the fields are the same
        .map(|v| format!("{v:?}").replace("Instance {", "Node {"))
        .map_err(|_| ());
    match &a {
        // Refused for want of a length unit where the pin reads in millimetres: its defect.
        Err(true) if b.is_ok() => c.pin_units.refused += 1,
        Ok((_, false)) => {}
        _ => {
            let a = a.map(|x| x.0).map_err(|_| ());
            c.products.take(a == b, false, || {
                let cut = |r: &Result<String, ()>| {
                    format!("{r:?}").chars().take(400).collect::<String>()
                };
                format!("{name}: product: scx {}\n  pin {}", cut(&a), cut(&b))
            });
        }
    }
}

/// One instance's records, fingerprinted.
struct Rec<'a>(&'a [(String, Vec<Cp>)]);

impl Bits for Rec<'_> {
    fn bits(&self, o: &mut Out) {
        self.0.len().bits(o);
        for (name, params) in self.0 {
            o.tag(name);
            params.bits(o);
        }
    }
}

/// Every STEP file under `dir`, at any depth.
fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
    let entries =
        std::fs::read_dir(dir).unwrap_or_else(|e| panic!("SCX_P21_CORPUS: {}: {e}", dir.display()));
    for e in entries {
        let p = e.unwrap().path();
        if p.is_dir() {
            walk(&p, out);
        } else if is_step(&p) {
            out.push(p);
        }
    }
}

#[test]
fn p21_corpus() {
    let mut files = fixtures();
    match std::env::var_os("SCX_P21_CORPUS") {
        Some(dir) => {
            let mut more = Vec::new();
            walk(Path::new(&dir), &mut more);
            more.sort();
            files.extend(more);
        }
        None => {
            println!("difftest t_p21::p21_corpus: SCX_P21_CORPUS is unset; the pin's fixtures only")
        }
    }
    let mut c = Corpus::default();
    for path in &files {
        let bytes = std::fs::read(path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
        compare_file(&name_of(path), &String::from_utf8_lossy(&bytes), &mut c);
    }
    let all = [
        ("files", &c.files),
        ("entities", &c.entities),
        ("units", &c.units),
        ("contexts", &c.contexts),
        ("protocols", &c.protocols),
        ("part42", &c.part42),
        ("products", &c.products),
    ];
    for (name, t) in &all {
        t.report(name);
    }
    println!(
        "difftest t_p21::corpus_pin_units: pin defects: {} missing units read as mm/rad, {} conflicting units read as the last, {} several uncertainties read as the last, {} products read where scx refuses for want of a length unit",
        c.pin_units.defaulted,
        c.pin_units.conflicting,
        c.pin_units.last_wins,
        c.pin_units.refused
    );
    println!(
        "difftest t_p21::corpus_part42: {} reads refused by both readers",
        c.part42_refused
    );
    for (name, t) in &all {
        assert!(
            t.unstable.is_empty(),
            "{name}: not deterministic on {:?}",
            t.unstable
        );
        assert!(
            t.bad.is_empty(),
            "{name}: {} mismatches; first:\n{}",
            t.bad.len(),
            t.bad.iter().take(3).cloned().collect::<Vec<_>>().join("\n")
        );
    }
}
