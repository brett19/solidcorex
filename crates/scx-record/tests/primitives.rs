// Written for solidcorex. Apache-2.0.
//! The primitives of both encodings (research 63 RE1, RE4): every primitive round-trips its
//! bits in binary and in text; binary → text → binary gives equal bytes; magic, version,
//! tag, run and non-finite refusals; and every truncation and single-byte change of an
//! encoded stream gives a value or a `ReadError`, never a panic, and a value only when the
//! changed input is that value's own spelling (each encoding is canonical).

#![allow(
    missing_docs,
    clippy::pedantic,
    clippy::indexing_slicing,
    clippy::unwrap_used
)]

use scx_num::error::ErrorKind;
use scx_num::hash::Rng;
use scx_record::{
    FORMAT_VERSION, Magic, ReadCause, ReadError, Reader, Sink, Source, Tag, TextReader, TextWriter,
    WriteCause, Writer,
};

const MAGIC: Magic = match Magic::new(*b"TEST") {
    Some(m) => m,
    None => Magic::new(*b"FAIL").unwrap(),
};
const LIMIT: u32 = 1 << 20;

/// A tag written by hand, as a caller outside the crate would.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Shape {
    Line,
    Circle,
    Spline,
}

impl Tag for Shape {
    const RECORD: &'static str = "shape";
    fn code(self) -> u8 {
        match self {
            Self::Line => 0,
            Self::Circle => 1,
            Self::Spline => 7,
        }
    }
    fn from_code(code: u8) -> Option<Self> {
        match code {
            0 => Some(Self::Line),
            1 => Some(Self::Circle),
            7 => Some(Self::Spline),
            _ => None,
        }
    }
    fn name(self) -> &'static str {
        match self {
            Self::Line => "line",
            Self::Circle => "circle",
            Self::Spline => "spline",
        }
    }
    fn from_name(name: &str) -> Option<Self> {
        match name {
            "line" => Some(Self::Line),
            "circle" => Some(Self::Circle),
            "spline" => Some(Self::Spline),
            _ => None,
        }
    }
}

/// One of every primitive.
#[derive(Debug, Clone, PartialEq)]
struct Sample {
    a: u8,
    b: u16,
    c: u32,
    d: u64,
    flag: bool,
    shape: Shape,
    xs: Vec<f64>,
    nested: Vec<Vec<u32>>,
    table: Vec<Option<(Shape, f64)>>,
}

fn put(s: &Sample, w: &mut impl Sink) -> Result<(), scx_record::WriteError> {
    w.header(MAGIC);
    w.line();
    w.u8(s.a);
    w.u16(s.b);
    w.u32(s.c);
    w.u64(s.d);
    w.bool(s.flag);
    w.tag(s.shape);
    w.line();
    w.list(&s.xs, |w, &x| w.f64(x))?;
    w.line();
    w.list(&s.nested, |w, v| {
        w.list(v, |w, &n| {
            w.u32(n);
            Ok(())
        })
    })?;
    w.line();
    w.slots(s.table.iter().map(Option::as_ref), |w, &(shape, x)| {
        w.tag(shape);
        w.f64(x)
    })
}

fn take(r: &mut impl Source) -> Result<Sample, ReadError> {
    let version = r.header(MAGIC)?;
    assert_eq!(version, FORMAT_VERSION);
    r.line();
    let a = r.u8()?;
    let b = r.u16()?;
    let c = r.u32()?;
    let d = r.u64()?;
    let flag = r.bool()?;
    let shape = r.tag()?;
    r.line();
    let xs = r.list(|r| r.f64())?;
    r.line();
    let nested = r.list(|r| r.list(|r| r.u32()))?;
    r.line();
    let table = r.slots(LIMIT, |r| Ok((r.tag()?, r.f64()?)))?;
    Ok(Sample {
        a,
        b,
        c,
        d,
        flag,
        shape,
        xs,
        nested,
        table,
    })
}

fn bin(s: &Sample) -> Vec<u8> {
    let mut w = Writer::new();
    put(s, &mut w).unwrap();
    w.finish()
}

fn txt(s: &Sample) -> String {
    let mut w = TextWriter::new();
    put(s, &mut w).unwrap();
    w.finish()
}

fn from_bin(bytes: &[u8]) -> Result<Sample, ReadError> {
    let mut r = Reader::new(bytes);
    let s = take(&mut r)?;
    r.finish()?;
    Ok(s)
}

fn from_txt(text: &str) -> Result<Sample, ReadError> {
    let mut r = TextReader::new(text);
    let s = take(&mut r)?;
    r.finish()?;
    Ok(s)
}

/// A finite `f64` from random bits: every exponent, subnormals and `-0` included.
fn any_finite(rng: &mut Rng) -> f64 {
    loop {
        let x = f64::from_bits(rng.next_u64());
        if x.is_finite() {
            return x;
        }
    }
}

const EDGE: [f64; 12] = [
    0.0,
    -0.0,
    1.0,
    -1.5,
    0.1,
    1.0 / 3.0,
    5e-324,
    -5e-324,
    f64::MIN_POSITIVE,
    f64::MAX,
    f64::MIN,
    1e21,
];

fn sample(rng: &mut Rng) -> Sample {
    let below = |rng: &mut Rng, n: u64| rng.next_u64() % n;
    let shapes = [Shape::Line, Shape::Circle, Shape::Spline];
    let mut xs: Vec<f64> = (0..below(rng, 6)).map(|_| any_finite(rng)).collect();
    xs.push(EDGE[below(rng, EDGE.len() as u64) as usize]);
    let nested = (0..below(rng, 4))
        .map(|_| (0..below(rng, 4)).map(|_| rng.next_u64() as u32).collect())
        .collect();
    let table = (0..below(rng, 9))
        .map(|_| {
            (below(rng, 3) > 0).then(|| {
                (
                    shapes[below(rng, 3) as usize],
                    rng.next_signed_unit() * 100.0,
                )
            })
        })
        .collect();
    Sample {
        a: rng.next_u64() as u8,
        b: rng.next_u64() as u16,
        c: rng.next_u64() as u32,
        d: rng.next_u64(),
        flag: rng.next_u64() & 1 == 1,
        shape: shapes[below(rng, 3) as usize],
        xs,
        nested,
        table,
    }
}

/// Equal to the bit: `-0` is not `0`.
fn same_bits(x: &Sample, y: &Sample) -> bool {
    let bits = |s: &Sample| {
        (
            s.xs.iter().map(|x| x.to_bits()).collect::<Vec<_>>(),
            s.table
                .iter()
                .map(|e| e.map(|(k, x)| (k, x.to_bits())))
                .collect::<Vec<_>>(),
        )
    };
    x == y && bits(x) == bits(y)
}

fn header_bytes() -> Vec<u8> {
    let mut w = Writer::new();
    w.header(MAGIC);
    w.finish()
}

// ---- round trips -------------------------------------------------------------------------

#[test]
fn every_primitive_round_trips_in_both_encodings() {
    let mut rng = Rng::new(0x5ec0_4d01);
    for _ in 0..2000 {
        let s = sample(&mut rng);
        let b = bin(&s);
        let t = txt(&s);
        let from_b = from_bin(&b).unwrap();
        let from_t = from_txt(&t).unwrap();
        assert!(same_bits(&s, &from_b), "binary: {s:?}");
        assert!(same_bits(&s, &from_t), "text: {s:?}\n{t}");
        // Binary -> text -> binary, and text -> binary -> text: equal bytes.
        assert_eq!(bin(&from_t), b);
        assert_eq!(txt(&from_b), t);
    }
}

#[test]
fn f64_text_keeps_the_bits() {
    let mut rng = Rng::new(0xf64);
    let check = |x: f64| {
        let mut w = TextWriter::new();
        w.f64(x).unwrap();
        let t = w.finish();
        let mut r = TextReader::new(&t);
        let y = r.f64().unwrap();
        r.finish().unwrap();
        assert_eq!(x.to_bits(), y.to_bits(), "{x:e} as {t:?}");
        let mut w = Writer::new();
        w.f64(x).unwrap();
        let b = w.finish();
        let mut r = Reader::new(&b);
        assert_eq!(r.f64().unwrap().to_bits(), x.to_bits());
        r.finish().unwrap();
    };
    EDGE.into_iter().for_each(check);
    for _ in 0..50_000 {
        check(any_finite(&mut rng));
    }
    // Near the decimal-binary boundaries: neighbours of powers of ten.
    for e in -320..=308 {
        let p: f64 = format!("1e{e}").parse().unwrap();
        for x in [
            p,
            f64::from_bits(p.to_bits() + 1),
            f64::from_bits(p.to_bits() - 1),
        ] {
            check(x);
        }
    }
}

#[test]
fn the_text_reads_as_written() {
    let s = Sample {
        a: 7,
        b: 300,
        c: 0,
        d: u64::MAX,
        flag: true,
        shape: Shape::Circle,
        xs: vec![-0.0, 0.5, 1e21],
        nested: vec![vec![], vec![1, 2]],
        table: vec![None, None, Some((Shape::Line, 2.0)), None],
    };
    assert_eq!(
        txt(&s),
        "TEST 1\n7 300 0 18446744073709551615 true circle\n3 -0 0.5 1000000000000000000000\n\
         2 0 2 1 2\n4\ngap 2\nlive line 2\ngap 1\n"
    );
    let b = bin(&s);
    assert_eq!(&b[..6], &[b'T', b'E', b'S', b'T', 1, 0]);
}

#[test]
fn tables_keep_their_ids() {
    let mut s = sample(&mut Rng::new(3));
    for table in [
        vec![],
        vec![None],
        vec![None, None, None],
        vec![Some((Shape::Line, 1.0))],
        vec![None, Some((Shape::Line, 1.0)), None],
        vec![
            Some((Shape::Line, 1.0)),
            None,
            None,
            Some((Shape::Spline, -1.0)),
        ],
    ] {
        s.table = table;
        assert!(same_bits(&from_bin(&bin(&s)).unwrap(), &s));
        assert!(same_bits(&from_txt(&txt(&s)).unwrap(), &s));
    }
}

// ---- refusals ----------------------------------------------------------------------------

fn cause_bin(bytes: &[u8]) -> ReadCause {
    from_bin(bytes).unwrap_err().cause
}

fn cause_txt(text: &str) -> ReadCause {
    from_txt(text).unwrap_err().cause
}

#[test]
fn magic_and_version() {
    let s = sample(&mut Rng::new(1));
    let good = bin(&s);

    let mut b = good.clone();
    b[0] = b'X';
    let e = from_bin(&b).unwrap_err();
    assert_eq!(
        (e.at, e.cause.clone(), e.kind()),
        (0, ReadCause::BadMagic, ErrorKind::Refused)
    );

    for (version, kind) in [
        (0u16, ErrorKind::Refused),
        (FORMAT_VERSION + 1, ErrorKind::Unsupported),
        (u16::MAX, ErrorKind::Unsupported),
    ] {
        let mut b = good.clone();
        b[4..6].copy_from_slice(&version.to_le_bytes());
        let e = from_bin(&b).unwrap_err();
        assert_eq!(e.at, 4);
        assert_eq!(e.cause, ReadCause::BadVersion { found: version });
        assert_eq!(e.kind(), kind);
        assert_eq!(e.name(), "bad_version");

        let t = txt(&s).replacen(" 1\n", &format!(" {version}\n"), 1);
        let e = from_txt(&t).unwrap_err();
        assert_eq!(
            (e.at, e.cause.clone(), e.kind()),
            (5, ReadCause::BadVersion { found: version }, kind)
        );
    }

    assert_eq!(
        cause_txt(&txt(&s).replacen("TEST", "TESX", 1)),
        ReadCause::BadMagic
    );
    assert_eq!(
        cause_txt(&txt(&s).replacen("TEST", "TES", 1)),
        ReadCause::BadMagic
    );
    assert_eq!(cause_bin(b""), ReadCause::Truncated);
    assert_eq!(cause_txt(""), ReadCause::Truncated);

    assert!(Magic::new(*b"AB C").is_none());
    assert!(Magic::new(*b"ab\n9").is_none());
    assert_eq!(Magic::new(*b"Ab09").unwrap().word(), "Ab09");
}

#[test]
fn non_finite_values_are_refused() {
    for x in [
        f64::NAN,
        -f64::NAN,
        f64::INFINITY,
        f64::NEG_INFINITY,
        f64::from_bits(0x7ff0_0000_0000_0001),
    ] {
        let mut w = Writer::new();
        w.u8(1);
        let e = w.f64(x).unwrap_err();
        assert_eq!(
            (e.at, e.cause.clone(), e.kind()),
            (1, WriteCause::NonFinite, ErrorKind::Refused)
        );
        assert_eq!(w.finish(), [1], "nothing written");
        let mut w = TextWriter::new();
        assert_eq!(w.f64(x).unwrap_err().cause, WriteCause::NonFinite);

        let bits = x.to_bits().to_le_bytes();
        let mut r = Reader::new(&bits);
        assert_eq!(
            r.f64().unwrap_err(),
            ReadError::new(0, ReadCause::NonFinite)
        );
    }
    for word in ["NaN", "nan", "inf", "-inf", "+inf", "infinity", "-Infinity"] {
        let t = format!("{word}\n");
        let mut r = TextReader::new(&t);
        assert_eq!(
            r.f64().unwrap_err(),
            ReadError::new(0, ReadCause::NonFinite),
            "{word}"
        );
    }
    // Inside a record: the offset of the field.
    let mut s = sample(&mut Rng::new(2));
    s.xs = vec![1.0, 2.0];
    let mut b = bin(&s);
    let at = header_bytes().len() + 1 + 2 + 4 + 8 + 1 + 1 + 4 + 8;
    b[at..at + 8].copy_from_slice(&f64::NAN.to_bits().to_le_bytes());
    assert_eq!(
        from_bin(&b).unwrap_err(),
        ReadError::new(at, ReadCause::NonFinite)
    );
}

#[test]
fn text_takes_only_the_canonical_spelling() {
    let one = |t: &str, f: fn(&mut TextReader<'_>) -> Result<(), ReadError>| {
        let mut r = TextReader::new(t);
        f(&mut r).and_then(|()| r.finish())
    };
    let f = |r: &mut TextReader<'_>| r.f64().map(drop);
    let u = |r: &mut TextReader<'_>| r.u32().map(drop);
    for good in [
        "0\n",
        "-0\n",
        "1\n",
        "0.1\n",
        "123.5\n",
        "1000000000000000000000\n",
    ] {
        one(good, f).unwrap();
    }
    for bad in [
        "1.0\n", "+1\n", "01\n", "1e0\n", "-0.0\n", ".5\n", "0.10\n", "1e21\n", "x\n",
    ] {
        assert_eq!(
            one(bad, f).unwrap_err().cause,
            ReadCause::BadToken { expected: "f64" },
            "{bad:?}"
        );
    }
    // 0.1 + one ulp has its own shortest spelling; a longer one for the same bits is refused.
    assert_eq!(
        one("0.1000000000000000055511151231257827\n", f)
            .unwrap_err()
            .cause,
        ReadCause::BadToken { expected: "f64" }
    );
    for good in ["0\n", "7\n", "4294967295\n"] {
        one(good, u).unwrap();
    }
    for bad in [
        "00\n",
        "07\n",
        "+7\n",
        "-0\n",
        "4294967296\n",
        "7a\n",
        "\t7\n",
    ] {
        assert_eq!(
            one(bad, u).unwrap_err().cause,
            ReadCause::BadToken { expected: "u32" },
            "{bad:?}"
        );
    }
    // Separators: exactly one, of the kind the record states; one final newline.
    let s = sample(&mut Rng::new(5));
    let t = txt(&s);
    assert_eq!(
        cause_txt(&t.replacen(' ', "  ", 1)),
        ReadCause::BadToken { expected: "u16" }
    );
    assert_eq!(
        cause_txt(&t.replacen('\n', " ", 1)),
        ReadCause::BadToken {
            expected: "separator"
        }
    );
    assert_eq!(cause_txt(t.trim_end()), ReadCause::Truncated);
    assert_eq!(cause_txt(&format!("{t}\n")), ReadCause::Trailing);
    assert_eq!(cause_txt(&format!("{t}x")), ReadCause::Trailing);
    // An unknown word for a tag.
    assert_eq!(
        cause_txt(&t.replacen(&format!(" {}\n", s.shape.name()), " ellipse\n", 1)),
        ReadCause::BadToken { expected: "shape" }
    );
}

#[test]
fn tags_are_refused_unless_known() {
    for code in 0..=u8::MAX {
        let mut r = Reader::new(core::slice::from_ref(&code));
        match (code, r.bool()) {
            (0, Ok(false)) | (1, Ok(true)) => {}
            (_, Err(e)) => assert_eq!(
                e,
                ReadError::new(
                    0,
                    ReadCause::BadTag {
                        record: "bool",
                        found: code
                    }
                )
            ),
            (_, other) => panic!("{code}: {other:?}"),
        }
        let mut r = Reader::new(core::slice::from_ref(&code));
        match r.tag::<Shape>() {
            Ok(s) => assert_eq!(s.code(), code),
            Err(e) => {
                assert!(![0, 1, 7].contains(&code));
                assert_eq!(
                    e.cause,
                    ReadCause::BadTag {
                        record: "shape",
                        found: code
                    }
                );
                assert_eq!(e.kind(), ErrorKind::Refused);
            }
        }
    }
}

/// A stream of just a table, by hand.
fn table_bytes(entries: &[(u8, u32)], total: u32) -> Vec<u8> {
    let mut w = Writer::new();
    w.u32(total);
    for &(tag, n) in entries {
        w.u8(tag);
        if tag == 1 {
            w.u32(n);
        } else {
            w.u8(n as u8); // a `Shape` tag
            w.f64(0.5).unwrap();
        }
    }
    w.finish()
}

fn read_table(bytes: &[u8], limit: u32) -> Result<Vec<Option<(Shape, f64)>>, ReadError> {
    let mut r = Reader::new(bytes);
    let t = r.slots(limit, |r| Ok((r.tag()?, r.f64()?)))?;
    r.finish()?;
    Ok(t)
}

#[test]
fn runs_are_whole_and_tables_capped() {
    // Live (0) and gap (1) entries.
    assert_eq!(
        read_table(&table_bytes(&[(1, 2), (0, 0), (1, 1)], 4), 10)
            .unwrap()
            .len(),
        4
    );
    for (entries, total) in [
        (&[(1u8, 0u32)][..], 1),    // an empty run
        (&[(1, 1), (1, 1)][..], 2), // two runs in a row
        (&[(0, 0), (1, 3)][..], 3), // a run past the end
        (&[(1, u32::MAX)][..], 2),  // a run far past the end
    ] {
        let e = read_table(&table_bytes(entries, total), 10).unwrap_err();
        assert_eq!(
            (e.cause.clone(), e.kind()),
            (ReadCause::BadRun, ErrorKind::Refused),
            "{entries:?}"
        );
    }
    // A table of four billion deleted slots is nine bytes: refused before allocation.
    let e = read_table(&table_bytes(&[(1, u32::MAX)], u32::MAX), LIMIT).unwrap_err();
    assert_eq!(
        (e.at, e.cause.clone(), e.kind()),
        (
            0,
            ReadCause::TooMany {
                count: u32::MAX,
                limit: LIMIT
            },
            ErrorKind::Exhausted
        )
    );
    // A list claiming more entries than the input has bytes.
    let mut w = Writer::new();
    w.u32(u32::MAX);
    let b = w.finish();
    let mut r = Reader::new(&b);
    assert_eq!(
        r.list(|r| r.u8()).unwrap_err(),
        ReadError::new(0, ReadCause::Truncated)
    );
}

#[test]
fn errors_frame_their_cause() {
    let e = ReadError::new(
        12,
        ReadCause::BadTag {
            record: "shape",
            found: 9,
        },
    );
    assert_eq!(e.to_string(), "record unreadable at byte 12");
    let source = std::error::Error::source(&e).unwrap();
    assert_eq!(source.to_string(), "tag 9 is no shape");
    assert_eq!(e.name(), "bad_tag");
    assert!(size_of::<ReadError>() <= 32, "{}", size_of::<ReadError>());
}

// ---- corruption --------------------------------------------------------------------------

/// Every prefix, and every change of one byte to each of `values`, reads as a value or a
/// `ReadError` with an offset inside the input; a value only when re-encoding it gives the
/// changed input back.
fn corrupt_bin(good: &[u8]) -> (usize, usize) {
    let mut decoded = 0;
    for len in 0..good.len() {
        let e = from_bin(&good[..len]).unwrap_err();
        assert!(e.at <= len);
    }
    let mut b = good.to_vec();
    for i in 0..b.len() {
        let orig = b[i];
        for v in 0..=u8::MAX {
            if v == orig {
                continue;
            }
            b[i] = v;
            match from_bin(&b) {
                Ok(s) => {
                    assert_eq!(
                        bin(&s),
                        b,
                        "byte {i} = {v}: decoded a value with another spelling"
                    );
                    decoded += 1;
                }
                Err(e) => assert!(e.at <= b.len()),
            }
        }
        b[i] = orig;
    }
    (decoded, good.len() * 255)
}

fn corrupt_txt(good: &str) -> (usize, usize) {
    let mut decoded = 0;
    let mut tried = 0;
    for len in 0..good.len() {
        let e = from_txt(&good[..len]).unwrap_err();
        assert!(e.at <= len);
    }
    let mut b = good.as_bytes().to_vec();
    for i in 0..b.len() {
        let orig = b[i];
        for v in 0..=u8::MAX {
            if v == orig {
                continue;
            }
            b[i] = v;
            let Ok(t) = core::str::from_utf8(&b) else {
                continue;
            };
            tried += 1;
            match from_txt(t) {
                Ok(s) => {
                    assert_eq!(
                        txt(&s),
                        t,
                        "byte {i} = {v}: decoded a value with another spelling"
                    );
                    decoded += 1;
                }
                Err(e) => assert!(e.at <= t.len()),
            }
        }
        b[i] = orig;
    }
    (decoded, tried)
}

#[test]
fn every_truncation_and_byte_change_reads_or_refuses() {
    let mut rng = Rng::new(0xc0_44_07);
    let (mut bin_ok, mut bin_all, mut txt_ok, mut txt_all) = (0, 0, 0, 0);
    for _ in 0..5 {
        let s = sample(&mut rng);
        let (ok, all) = corrupt_bin(&bin(&s));
        bin_ok += ok;
        bin_all += all;
        let (ok, all) = corrupt_txt(&txt(&s));
        txt_ok += ok;
        txt_all += all;
    }
    println!("binary: {bin_ok} of {bin_all} changed inputs decode; text: {txt_ok} of {txt_all}");
    // Changes inside numbers decode to other values; most others are refused.
    assert!(bin_ok > 0 && bin_ok < bin_all, "{bin_ok} of {bin_all}");
    assert!(txt_ok > 0 && txt_ok < txt_all, "{txt_ok} of {txt_all}");
}
