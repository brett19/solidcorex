// Written for solidcorex.
//! `parse(write(x)) == x`: for hand-built exchanges, for generated ones, and (when
//! `SCX_P21_CORPUS` names a directory of STEP files) for every file in a corpus, where also
//! `write(parse(write(parse(f)))) == write(parse(f))` byte for byte.

#![allow(
    clippy::pedantic,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

use std::path::{Path, PathBuf};

use scx_p21::error::Unwritable;
use scx_p21::{
    DataSection, Exchange, FileDescription, FileName, Header, Instance, P21Error, Param, Record,
    Refusal, Writer, parse, write,
};

/// `ex` written and read back, which must be `ex` again with no warning.
fn round_trip(ex: &Exchange) -> String {
    let text = write(ex).unwrap();
    let back = parse(&text).unwrap_or_else(|e| panic!("{e}\n{text}"));
    assert!(back.warnings.is_empty(), "{:?}", back.warnings);
    assert_eq!(&back.exchange, ex, "\n{text}");
    // Bit for bit: `==` on reals takes 0. for -0.
    let bits = |ex: &Exchange| {
        let mut v = Vec::new();
        for (_, e) in ex.instances() {
            for r in e.records() {
                reals(&r.params, &mut v);
            }
        }
        v
    };
    assert_eq!(bits(&back.exchange), bits(ex));
    // Writing again gives the same text.
    assert_eq!(write(&back.exchange).unwrap(), text);
    text
}

fn reals(ps: &[Param], v: &mut Vec<u64>) {
    for p in ps {
        match p {
            Param::Real(x) => v.push(x.to_bits()),
            Param::List(l) => reals(l, v),
            Param::Typed { value, .. } => reals(core::slice::from_ref(value.as_ref()), v),
            _ => {}
        }
    }
}

fn header() -> Header {
    Header::new(
        &FileDescription {
            description: vec!["round trip".into()],
            implementation_level: "2;1".into(),
        },
        &FileName {
            name: "x.stp".into(),
            time_stamp: "2026-11-27T00:00:00".into(),
            author: vec!["a".into(), "b".into()],
            organization: vec![String::new()],
            preprocessor_version: "scx-p21".into(),
            originating_system: "it's \u{e9}".into(),
            authorization: String::new(),
        },
        &["AUTOMOTIVE_DESIGN".into(), "CONFIG_CONTROL_DESIGN".into()],
    )
}

#[test]
fn the_header_reads_back_as_built() {
    let h = header();
    assert_eq!(
        h.file_description().unwrap().description,
        vec!["round trip"]
    );
    assert_eq!(h.file_name().unwrap().originating_system, "it's \u{e9}");
    assert_eq!(
        h.schemas(),
        vec!["AUTOMOTIVE_DESIGN", "CONFIG_CONTROL_DESIGN"]
    );
    round_trip(&Exchange {
        header: h,
        ..Exchange::default()
    });
}

#[test]
fn every_parameter_form_round_trips() {
    let strings = [
        "",
        "'",
        "''",
        "\\",
        "\\X\\",
        "\\X2\\00E9\\X0\\",
        "a\nb\tc\u{7f}\u{0}",
        "\u{e9}\u{4e16}\u{1f600}x\u{ffff}\u{10ffff}",
        "/* not a comment */",
        "ENDSEC;",
        "\\PA\\\\S\\i",
    ];
    let reals = [
        0.0,
        -0.0,
        1.0,
        -2.5,
        0.1,
        1e-300,
        5e-324,
        f64::MIN_POSITIVE,
        f64::MAX,
        f64::MIN,
        1e21,
        1e22,
        123_456_789.012_345_68,
        1.0 / 3.0,
    ];
    let mut params: Vec<Param> = strings.iter().map(|s| Param::Str((*s).into())).collect();
    params.extend(reals.iter().map(|x| Param::Real(*x)));
    params.extend([
        Param::Int(0),
        Param::Int(i64::MIN),
        Param::Int(i64::MAX),
        Param::Enum("T".into()),
        Param::Enum("_X1".into()),
        Param::Ref(1),
        Param::Ref(u64::from(u32::MAX) * 7),
        Param::Unset,
        Param::Derived,
        Param::Binary("0".into()),
        Param::Binary("3ABCDEF0123456789".into()),
        Param::List(vec![]),
        Param::List(vec![
            Param::List(vec![Param::List(vec![Param::Unset])]),
            Param::Int(1),
        ]),
        Param::typed("LENGTH_MEASURE", Param::Real(2.0)),
        Param::typed(
            "T",
            Param::List(vec![Param::Int(1), Param::Str("x".into())]),
        ),
        Param::typed("U", Param::typed("V", Param::Derived)),
    ]);
    let instances = vec![
        (1, Instance::simple("A", params.clone())),
        (2, Instance::simple("EMPTY", vec![])),
        // Kept in the order given, sorted or not.
        (
            3,
            Instance::Complex(vec![
                Record::new("Z", vec![Param::Ref(1)]),
                Record::new("A", vec![]),
            ]),
        ),
        (
            4,
            Instance::complex(vec![
                Record::new("Z", params),
                Record::new("A", vec![Param::Unset]),
            ]),
        ),
        (
            u64::from(u32::MAX) * 7,
            Instance::Complex(vec![Record::new("ONLY", vec![])]),
        ),
    ];
    let data = DataSection::new(
        Some(vec![
            Param::Str("s".into()),
            Param::List(vec![Param::Str("S".into())]),
        ]),
        instances,
    )
    .0;
    let mut ex = Exchange::new(header(), data);
    round_trip(&ex);
    // Every optional section, several DATA sections, signatures.
    ex.anchor = Some("<a>=#1;".into());
    ex.reference = Some("#9=<x.stp#y>;".into());
    ex.sections
        .push(DataSection::new(None, vec![(5, Instance::simple("B", vec![Param::Ref(1)]))]).0);
    ex.sections.push(DataSection::default());
    ex.signatures = vec!["\"0F\"".into(), String::new()];
    round_trip(&ex);
}

#[test]
fn a_value_with_no_part21_form_is_refused() {
    let refused = |p: Param| {
        let data = DataSection::new(None, vec![(7, Instance::simple("A", vec![p]))]).0;
        match write(&Exchange::new(Header::default(), data)) {
            Err(P21Error::Refused(Refusal::Unwritable { id: Some(7), cause })) => cause,
            other => panic!("{other:?}"),
        }
    };
    assert!(matches!(
        refused(Param::Real(f64::INFINITY)),
        Unwritable::NonFiniteReal(_)
    ));
    assert!(matches!(
        refused(Param::Real(f64::NAN)),
        Unwritable::NonFiniteReal(_)
    ));
    assert!(matches!(
        refused(Param::List(vec![Param::Real(f64::NEG_INFINITY)])),
        Unwritable::NonFiniteReal(_)
    ));
    assert_eq!(
        refused(Param::Enum("t".into())),
        Unwritable::BadEnumeration("t".into())
    );
    assert_eq!(
        refused(Param::Enum(String::new())),
        Unwritable::BadEnumeration(String::new())
    );
    assert_eq!(
        refused(Param::Enum("1A".into())),
        Unwritable::BadEnumeration("1A".into())
    );
    assert_eq!(
        refused(Param::Binary("4F".into())),
        Unwritable::BadBinary("4F".into())
    );
    assert_eq!(
        refused(Param::Binary("0f".into())),
        Unwritable::BadBinary("0f".into())
    );
    assert_eq!(
        refused(Param::typed("a b", Param::Unset)),
        Unwritable::BadKeyword("a b".into())
    );
    let bad_name = DataSection::new(None, vec![(1, Instance::simple("lower", vec![]))]).0;
    assert!(write(&Exchange::new(Header::default(), bad_name)).is_err());
    let empty = DataSection::new(None, vec![(1, Instance::Complex(vec![]))]).0;
    assert!(matches!(
        write(&Exchange::new(Header::default(), empty)),
        Err(P21Error::Refused(Refusal::Unwritable {
            id: Some(1),
            cause: Unwritable::EmptyComplex
        }))
    ));
    // A header record and a section's parameters are checked too.
    let mut h = Header::default();
    h.records
        .push(Record::new("FILE_SCHEMA", vec![Param::Real(f64::NAN)]));
    assert!(matches!(
        write(&Exchange {
            header: h,
            ..Exchange::default()
        }),
        Err(P21Error::Refused(Refusal::Unwritable { id: None, .. }))
    ));
}

#[test]
fn the_writer_shares_identical_instances_and_numbers_from_one() {
    let mut w = Writer::new();
    let p = |x: f64| {
        Instance::simple(
            "CARTESIAN_POINT",
            vec![Param::Str(String::new()), Param::List(vec![Param::Real(x)])],
        )
    };
    assert_eq!(w.add(p(1.0)).unwrap(), 1);
    assert_eq!(w.add(p(2.0)).unwrap(), 2);
    assert_eq!(w.add(p(1.0)).unwrap(), 1);
    assert_eq!(w.add_distinct(p(1.0)).unwrap(), 3);
    // A distinct instance is still shared by a later `add` of the same text.
    assert_eq!(w.add(p(2.0)).unwrap(), 2);
    assert_eq!(w.len(), 3);
    assert_eq!(w.get(3), Some(&p(1.0)));
    assert_eq!(w.get(0), None);
    assert!(w.add(p(f64::NAN)).is_err());
    assert_eq!(w.len(), 3, "a refused instance is not added");
    let data = w.finish(None);
    assert_eq!(
        data.instances()
            .iter()
            .map(|(id, _)| *id)
            .collect::<Vec<_>>(),
        [1, 2, 3]
    );
    round_trip(&Exchange::new(header(), data));
}

/// SplitMix64 (Steele, Lea, Flood 2014): a fixed stream for generated cases.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9e37_79b9_7f4a_7c15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        z ^ (z >> 31)
    }
    fn below(&mut self, n: u64) -> u64 {
        self.next() % n
    }
    fn name(&mut self) -> String {
        const FIRST: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZ_";
        const REST: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZ_0123456789";
        let mut s = String::new();
        s.push(char::from(FIRST[self.below(FIRST.len() as u64) as usize]));
        for _ in 0..self.below(8) {
            s.push(char::from(REST[self.below(REST.len() as u64) as usize]));
        }
        s
    }
    fn string(&mut self) -> String {
        (0..self.below(6))
            .map(|_| match self.below(6) {
                0 => '\'',
                1 => '\\',
                2 => char::from_u32(self.below(0x80) as u32).unwrap(),
                3 => char::from_u32(0x80 + self.below(0xd800 - 0x80) as u32).unwrap(),
                4 => char::from_u32(0x1_0000 + self.below(0x10_0000) as u32).unwrap(),
                _ => 'a',
            })
            .collect()
    }
    fn param(&mut self, depth: u32) -> Param {
        match self.below(if depth > 3 { 8 } else { 10 }) {
            0 => Param::Int(self.next() as i64),
            1 => {
                let x = f64::from_bits(self.next());
                Param::Real(if x.is_finite() { x } else { 0.5 })
            }
            2 => Param::Str(self.string()),
            3 => Param::Enum(self.name()),
            4 => Param::Ref(1 + self.below(1 << 40)),
            5 => Param::Unset,
            6 => Param::Derived,
            7 => Param::Binary(format!("{}{:X}", self.below(4), self.next())),
            8 => Param::List((0..self.below(4)).map(|_| self.param(depth + 1)).collect()),
            _ => Param::typed(self.name(), self.param(depth + 1)),
        }
    }
    fn record(&mut self) -> Record {
        let n = self.below(5);
        Record::new(self.name(), (0..n).map(|_| self.param(0)).collect())
    }
}

#[test]
fn generated_exchanges_round_trip() {
    let mut rng = Rng(0x5c_7021);
    for _ in 0..300 {
        let mut sections = Vec::new();
        for _ in 0..1 + rng.below(3) {
            let instances = (0..rng.below(20))
                .map(|_| {
                    let id = 1 + rng.below(1000);
                    let e = if rng.below(4) == 0 {
                        Instance::Complex((0..1 + rng.below(3)).map(|_| rng.record()).collect())
                    } else {
                        Instance::Simple(rng.record())
                    };
                    (id, e)
                })
                .collect();
            let params = (rng.below(2) == 0).then(|| vec![rng.param(0)]);
            sections.push(DataSection::new(params, instances).0);
        }
        let mut ex = Exchange {
            header: header(),
            sections,
            ..Exchange::default()
        };
        // Unique ids across sections, so the read gives no warning.
        let mut seen = std::collections::BTreeSet::new();
        for s in &mut ex.sections {
            let kept: Vec<_> = s
                .instances()
                .iter()
                .filter(|(id, _)| seen.insert(*id))
                .cloned()
                .collect();
            *s = DataSection::new(s.params.clone(), kept).0;
        }
        round_trip(&ex);
    }
}

/// Every `.stp`/`.step` file under `dir`.
fn step_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(rd) = std::fs::read_dir(dir) else {
        return;
    };
    for e in rd.flatten() {
        let p = e.path();
        if p.is_dir() {
            step_files(&p, out);
        } else if p
            .extension()
            .and_then(|x| x.to_str())
            .is_some_and(|x| x.eq_ignore_ascii_case("stp") || x.eq_ignore_ascii_case("step"))
        {
            out.push(p);
        }
    }
}

/// Every file of the corpus `SCX_P21_CORPUS` names (for instance the NIST MBE PMI files, US
/// Government work, public domain) reads, writes, and reads back as it was read.
#[test]
fn corpus_files_round_trip() {
    let Some(dir) = std::env::var_os("SCX_P21_CORPUS") else {
        eprintln!("SCX_P21_CORPUS not set: no corpus read");
        return;
    };
    let mut files = Vec::new();
    step_files(Path::new(&dir), &mut files);
    files.sort();
    assert!(!files.is_empty(), "no STEP file under {dir:?}");
    for f in &files {
        let bytes = std::fs::read(f).unwrap();
        let text = String::from_utf8_lossy(&bytes);
        let p = parse(&text).unwrap_or_else(|e| panic!("{}: {e}", f.display()));
        let once = round_trip(&p.exchange);
        eprintln!(
            "{}: {} instances, {} warnings, {} -> {} bytes",
            f.display(),
            p.exchange.len(),
            p.warnings.len(),
            bytes.len(),
            once.len()
        );
    }
}
