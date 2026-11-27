// Written for solidcorex.
//! Hand-written exchange structures with known contents, one per rule of ISO 10303-21:2016
//! (§ numbers are that edition's), and the hostile texts a reader must refuse.

#![allow(
    clippy::pedantic,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

use std::error::Error as _;

use scx_num::error::ErrorKind;
use scx_p21::error::{Counted, Unwritable};
use scx_p21::parse::{MAX_BYTES, PEAK_PER_BYTE};
use scx_p21::{
    parse, parse_with, Instance, Limits, P21Error, Param, Progress, Refusal, Syntax, Warning,
};

/// A file of the minimal header and one DATA section holding `data`.
fn file(data: &str) -> String {
    format!(
        "ISO-10303-21;\nHEADER;\nFILE_DESCRIPTION((''),'2;1');\n\
         FILE_NAME('','',(''),(''),'','','');\nFILE_SCHEMA(('S'));\nENDSEC;\n\
         DATA;\n{data}\nENDSEC;\nEND-ISO-10303-21;\n"
    )
}

/// The parameters of `#1` in `file(data)`.
fn params_of(data: &str) -> Vec<Param> {
    let p = parse(&file(data)).unwrap();
    p.exchange.get(1).unwrap().params().to_vec()
}

/// The refusal `file(data)` gives.
fn refusal(text: &str) -> Refusal {
    match parse(text) {
        Err(P21Error::Refused(r)) => r,
        other => panic!("{text:?} gave {other:?}"),
    }
}

/// The syntax error `file(data)` gives.
fn syntax(data: &str) -> Syntax {
    match refusal(&file(data)) {
        Refusal::Syntax { cause, .. } => cause,
        other => panic!("{data:?} gave {other:?}"),
    }
}

#[test]
fn a_known_file_reads_as_written() {
    let t = "ISO-10303-21;\nHEADER;\nFILE_DESCRIPTION(('a part'),'2;1');\n\
             FILE_NAME('p.stp','2026-11-27T00:00:00',('me'),('us'),'pre','sys','auth');\n\
             FILE_SCHEMA(('AUTOMOTIVE_DESIGN { 1 0 10303 214 1 1 1 1 }'));\nENDSEC;\nDATA;\n\
             #1=CARTESIAN_POINT('p',(0.,1.5E1,-2.));\n\
             /* c */ #2=(LENGTH_UNIT()NAMED_UNIT(*)SI_UNIT(.MILLI.,.METRE.));\n\
             #3=X('it''s','\\X2\\00E9\\X0\\',$,#1,LENGTH_MEASURE(1.E-3),\"0F\",.T.,-7);\n\
             ENDSEC;\nEND-ISO-10303-21;\n";
    let p = parse(t).unwrap();
    assert!(p.warnings.is_empty(), "{:?}", p.warnings);
    let ex = &p.exchange;
    let d = ex.header.file_description().unwrap();
    assert_eq!(d.description, vec!["a part"]);
    assert_eq!(d.implementation_level, "2;1");
    let n = ex.header.file_name().unwrap();
    assert_eq!(
        (
            n.name.as_str(),
            n.time_stamp.as_str(),
            n.originating_system.as_str()
        ),
        ("p.stp", "2026-11-27T00:00:00", "sys")
    );
    assert_eq!(
        (n.author, n.organization),
        (vec!["me".to_string()], vec!["us".to_string()])
    );
    assert_eq!(
        ex.header.schemas(),
        vec!["AUTOMOTIVE_DESIGN { 1 0 10303 214 1 1 1 1 }"]
    );
    assert_eq!(ex.len(), 3);
    let point = ex.get(1).unwrap();
    assert_eq!(point.name(), "CARTESIAN_POINT");
    assert_eq!(
        point.params(),
        [
            Param::Str("p".into()),
            Param::List(vec![Param::Real(0.0), Param::Real(15.0), Param::Real(-2.0)])
        ]
    );
    let unit = ex.get(2).unwrap();
    assert!(unit.is_complex() && unit.has("SI_UNIT") && unit.has("LENGTH_UNIT"));
    assert_eq!(unit.record("NAMED_UNIT").unwrap(), [Param::Derived]);
    assert_eq!(unit.record("SI_UNIT").unwrap()[0].as_enum(), Some("MILLI"));
    assert_eq!(
        ex.get(3).unwrap().params(),
        [
            Param::Str("it's".into()),
            Param::Str("\u{e9}".into()),
            Param::Unset,
            Param::Ref(1),
            Param::typed("LENGTH_MEASURE", Param::Real(1e-3)),
            Param::Binary("0F".into()),
            Param::Enum("T".into()),
            Param::Int(-7),
        ]
    );
    assert_eq!(ex.get(3).unwrap().params()[6].as_bool(), Some(true));
    assert_eq!(ex.get(4), None);
}

/// §6.4.2: a real has a point, digits before it, and an optional `E` exponent; §6.4.1: an
/// integer has none. The reader also takes what writers in use write (`.5`, `1.e3`).
#[test]
fn reals_and_integers() {
    assert_eq!(
        params_of("#1=A(1.,-2.5,+3.E2,4.5E-1,0.,.5,1.e3,7,-0,+12);"),
        [
            Param::Real(1.0),
            Param::Real(-2.5),
            Param::Real(300.0),
            Param::Real(0.45),
            Param::Real(0.0),
            Param::Real(0.5),
            Param::Real(1000.0),
            Param::Int(7),
            Param::Int(0),
            Param::Int(12),
        ]
    );
    // Every f64 written by the shortest round-trip form reads back bit for bit.
    let p = params_of("#1=A(2.2250738585072014E-308,5.E-324,1.7976931348623157E308,-0.);");
    let bits: Vec<u64> = p.iter().map(|x| x.as_f64().unwrap().to_bits()).collect();
    assert_eq!(
        bits,
        [
            f64::MIN_POSITIVE.to_bits(),
            5e-324f64.to_bits(),
            f64::MAX.to_bits(),
            (-0.0f64).to_bits()
        ]
    );
    assert_eq!(
        params_of("#1=A(9223372036854775807,-9223372036854775808);"),
        [Param::Int(i64::MAX), Param::Int(i64::MIN)]
    );
    assert!(matches!(
        syntax("#1=A(9223372036854775808);"),
        Syntax::IntegerOutOfRange(_)
    ));
    for bad in ["#1=A(.);", "#1=A(+.);", "#1=A(-.E3);", "#1=A(1e999999.);"] {
        assert!(matches!(syntax(bad), Syntax::BadNumber(_)), "{bad}");
    }
    // A real beyond f64 reads as infinity, which `as_f64` refuses.
    let big = params_of("#1=A(1.E999999);");
    assert_eq!(big, [Param::Real(f64::INFINITY)]);
    assert_eq!(big[0].as_f64(), None);
}

/// §6.4.3: an apostrophe is doubled, a backslash is `\\`, and the `\X\`, `\X2\`, `\X4\`
/// directives give characters beyond the basic alphabet; §5.6: a line break inside a string
/// is not part of it.
#[test]
fn strings_and_their_directives() {
    assert_eq!(
        params_of(concat!(
            "#1=A('','it''s','a\\\\b','\\X\\E9','\\X2\\00E900E8\\X0\\',",
            "'\\X4\\0001F600\\X0\\','\\PA\\\\S\\i','a\nb','\\X2\\5706\\X0\\\\X2\\89D2\\X0\\20');"
        )),
        [
            Param::Str(String::new()),
            Param::Str("it's".into()),
            Param::Str("a\\b".into()),
            Param::Str("\u{e9}".into()),
            Param::Str("\u{e9}\u{e8}".into()),
            Param::Str("\u{1f600}".into()),
            Param::Str("\u{e9}".into()),
            Param::Str("ab".into()),
            Param::Str("\u{5706}\u{89d2}20".into()),
        ]
    );
    // A UTF-8 string (edition 3 allows any character) reads as written.
    assert_eq!(
        params_of("#1=A('h\u{e9}llo \u{4e16}');"),
        [Param::Str("h\u{e9}llo \u{4e16}".into())]
    );
    assert_eq!(syntax("#1=A('open);\n"), Syntax::UnterminatedString);
}

/// §6.4.3.2: `\S\` under a code page other than `\PA\` (ISO 8859-1) reads as U+FFFD, and is
/// reported once with its line.
#[test]
fn an_unsupported_code_page_is_a_warning() {
    let p = parse(&file("#1=A('\\PE\\\\S\\i');")).unwrap();
    assert_eq!(
        p.exchange.get(1).unwrap().params(),
        [Param::Str("\u{FFFD}".into())]
    );
    assert_eq!(
        p.warnings,
        vec![Warning::UnsupportedCodePage { line: 8, page: 'E' }]
    );
}

/// §6.4.4: an enumeration is `.NAME.`, upper case (lower case read upper, as writers emit it).
#[test]
fn enumerations_and_logicals() {
    let p = params_of("#1=A(.T.,.F.,.U.,.UNSPECIFIED.,.milli.);");
    assert_eq!(
        p.iter().map(Param::as_bool).collect::<Vec<_>>(),
        [Some(true), Some(false), None, None, None]
    );
    assert_eq!(p[4].as_enum(), Some("MILLI"));
    assert_eq!(p[2].as_enum(), Some("U"));
    assert_eq!(syntax("#1=A(.T);"), Syntax::BadEnumeration);
    assert_eq!(syntax("#1=A(.A-B.);"), Syntax::BadEnumeration);
    // §5.4: an instance name is one token, `#` and its digits with nothing between.
    assert_eq!(syntax("# 1=A();"), Syntax::BadReference);
}

/// §6.4.5: a binary is a digit 0 to 3 (the unused bits of the first hex digit) and upper-case
/// hexadecimal digits.
#[test]
fn binaries() {
    assert_eq!(
        params_of("#1=A(\"0\",\"3F\",\"0FFFF\");"),
        [
            Param::Binary("0".into()),
            Param::Binary("3F".into()),
            Param::Binary("0FFFF".into())
        ]
    );
    for bad in ["\"4F\"", "\"\"", "\"0G\""] {
        assert!(
            matches!(syntax(&format!("#1=A({bad});")), Syntax::BadBinary(_)),
            "{bad}"
        );
    }
}

/// §12.2.2 `$` (unset), §12.2.6 `*` (derived) and §5.5 typed parameters, which hold exactly
/// one parameter (a list is one).
#[test]
fn unset_derived_and_typed() {
    assert_eq!(
        params_of("#1=A($,*,LENGTH_MEASURE(2.),T((1,2)),T($),U(V(#3)));"),
        [
            Param::Unset,
            Param::Derived,
            Param::typed("LENGTH_MEASURE", Param::Real(2.0)),
            Param::typed("T", Param::List(vec![Param::Int(1), Param::Int(2)])),
            Param::typed("T", Param::Unset),
            Param::typed("U", Param::typed("V", Param::Ref(3))),
        ]
    );
    assert_eq!(syntax("#1=A(T(1.,2.));"), Syntax::TypedArity("T".into()));
    assert_eq!(syntax("#1=A(T());"), Syntax::TypedArity("T".into()));
}

/// §12.2.5.3: a complex instance lists its parts; they are kept in the file's order, and
/// `Instance::complex` sorts them as the standard asks of a writer.
#[test]
fn complex_instances() {
    let p = parse(&file("#1=(B(1)A()C(#1,$));")).unwrap();
    let e = p.exchange.get(1).unwrap();
    assert_eq!(
        e.records()
            .iter()
            .map(|r| r.name.as_str())
            .collect::<Vec<_>>(),
        ["B", "A", "C"]
    );
    assert_eq!(e.record("C").unwrap(), [Param::Ref(1), Param::Unset]);
    assert_eq!(e.record("D"), None);
    let sorted = Instance::complex(e.records().to_vec());
    assert_eq!(
        sorted
            .records()
            .iter()
            .map(|r| r.name.as_str())
            .collect::<Vec<_>>(),
        ["A", "B", "C"]
    );
    // An empty complex instance has nothing to name.
    assert!(matches!(syntax("#1=();"), Syntax::ExpectedKeyword));
    // Padded so that the density limit (a record per 16 bytes) is not what refuses it.
    let pad = format!("/*{}*/", " ".repeat(2000));
    assert_eq!(
        syntax(&format!("{pad}#1=({});", "A()".repeat(65))),
        Syntax::TooManyParts { limit: 64 }
    );
    assert!(parse(&file(&format!("{pad}#1=({});", "A()".repeat(64)))).is_ok());
}

/// §5.6: a comment may stand wherever white space may, and white space between tokens is not
/// significant.
#[test]
fn comments_and_white_space() {
    let t = "/* lead */ISO-10303-21 ;\n HEADER ; /* x */ FILE_SCHEMA ( ( 'S' ) ) ;ENDSEC;\nDATA;\n\
             #1 = A ( /* in */ 1 , ( 2 /**/ , 3 ) ) ;\n#2=B(/* ) ; */ 'x/*y*/');\n\
             ENDSEC;END-ISO-10303-21;";
    let ex = parse(t).unwrap().exchange;
    assert_eq!(
        ex.get(1).unwrap().params(),
        [
            Param::Int(1),
            Param::List(vec![Param::Int(2), Param::Int(3)])
        ]
    );
    // A comment's text inside a string is the string's.
    assert_eq!(ex.get(2).unwrap().params(), [Param::Str("x/*y*/".into())]);
    assert_eq!(ex.header.schemas(), vec!["S"]);
    assert_eq!(syntax("#1=A(/* never closed"), Syntax::UnterminatedComment);
}

/// §11.1: an exchange may hold several DATA sections, each with its name and schema.
#[test]
fn several_data_sections() {
    let t = "ISO-10303-21;HEADER;ENDSEC;\nDATA('one',('S1'));#1=A();#3=C();ENDSEC;\n\
             DATA('two',('S2'));#2=B(#1);ENDSEC;\nDATA;ENDSEC;\nEND-ISO-10303-21;";
    let ex = parse(t).unwrap().exchange;
    assert_eq!(ex.sections.len(), 3);
    assert_eq!(
        ex.sections[0].params.as_deref(),
        Some(
            &[
                Param::Str("one".into()),
                Param::List(vec![Param::Str("S1".into())])
            ][..]
        )
    );
    assert_eq!(ex.sections[2].params, None);
    assert!(ex.sections[2].is_empty());
    assert_eq!(ex.get(2).unwrap().params(), [Param::Ref(1)]);
    assert_eq!(
        ex.instances().map(|(id, _)| id).collect::<Vec<_>>(),
        [1, 3, 2]
    );
    // No DATA section at all is a valid exchange (§5.5 `{ DATA_SECTION }`).
    let empty = parse("ISO-10303-21;HEADER;ENDSEC;END-ISO-10303-21;")
        .unwrap()
        .exchange;
    assert!(empty.is_empty() && empty.sections.is_empty());
}

/// §11.1: an instance name is defined once in the exchange. A repeat in one section is
/// dropped; one in a later section is kept; both are reported.
#[test]
fn repeated_ids_are_warned_of() {
    let p = parse(&file("#1=A(1);#1=B();#2=C();")).unwrap();
    assert_eq!(p.exchange.get(1).unwrap().name(), "A");
    assert_eq!(p.exchange.len(), 2);
    assert_eq!(p.warnings, vec![Warning::DuplicateId { id: 1 }]);
    let t = "ISO-10303-21;HEADER;ENDSEC;DATA;#1=A();ENDSEC;DATA;#1=B();ENDSEC;END-ISO-10303-21;";
    let p = parse(t).unwrap();
    assert_eq!(p.exchange.len(), 2);
    assert_eq!(p.exchange.get(1).unwrap().name(), "A");
    assert_eq!(p.warnings, vec![Warning::DuplicateId { id: 1 }]);
}

/// §9, §10 and §14: ANCHOR and REFERENCE sections before the data and SIGNATURE sections after
/// the end keyword are kept as their text; an `ENDSEC` in a string, comment or longer name does
/// not end one.
#[test]
fn anchor_reference_and_signature_sections_are_kept() {
    let t = "ISO-10303-21;HEADER;ENDSEC;\nANCHOR;<a>=#1; /* ENDSEC; */ <b>='ENDSEC;';ENDSEC;\n\
             REFERENCE;#9=<x.stp#y>;NOT_ENDSEC;ENDSEC;\nDATA;#1=A();ENDSEC;\nEND-ISO-10303-21;\n\
             SIGNATURE;abc\"0F\"ENDSEC;\nSIGNATURE;ENDSEC;";
    let p = parse(t).unwrap();
    assert!(p.warnings.is_empty(), "{:?}", p.warnings);
    let ex = p.exchange;
    assert_eq!(
        ex.anchor.as_deref(),
        Some("<a>=#1; /* ENDSEC; */ <b>='ENDSEC;';")
    );
    assert_eq!(ex.reference.as_deref(), Some("#9=<x.stp#y>;NOT_ENDSEC;"));
    assert_eq!(ex.signatures, vec!["abc\"0F\"".to_string(), String::new()]);
    assert_eq!(
        refusal("ISO-10303-21;HEADER;ENDSEC;ANCHOR;<a>=#1;'ENDSEC;"),
        Refusal::Syntax {
            line: 1,
            cause: Syntax::UnterminatedSection("ANCHOR".into())
        }
    );
}

#[test]
fn text_after_the_end_is_a_warning() {
    let p = parse(&format!("{}\ntrailing junk", file(""))).unwrap();
    assert_eq!(p.warnings, vec![Warning::TextAfterEnd { line: 12 }]);
}

/// A UTF-8 byte order mark before the first keyword is skipped.
#[test]
fn a_byte_order_mark_is_skipped() {
    assert!(parse(&format!("\u{feff}{}", file("#1=A();"))).is_ok());
}

/// §6.3 and §12.2.11: short names (a schema's six-letter abbreviations) read as their long
/// names; a name outside the table is kept as written.
#[test]
fn short_names_read_as_long_names() {
    let ex = parse(&file("#1=CRTPNT('',(0.,1.,2.));#2=PLYLP('',(#1));#3=SOME_SHRT('');#4=VRTPNT('',#1);#5=FCBND('',#2,.T.);"))
        .unwrap()
        .exchange;
    let name = |id| ex.get(id).unwrap().name().to_string();
    assert_eq!(
        [name(1), name(2), name(3), name(4), name(5)],
        [
            "CARTESIAN_POINT",
            "POLY_LOOP",
            "SOME_SHRT",
            "VERTEX_POINT",
            "FACE_BOUND"
        ]
    );
}

/// The header is metadata: a malformed record in it is skipped, with a warning, to the next
/// line that starts with a keyword (solidcore #11650).
#[test]
fn a_malformed_header_record_is_skipped() {
    let t = "ISO-10303-21;\nHEADER;\nFILE_DESCRIPTION((''), '2;1');1');\n\n\
             FILE_NAME('','',(''),(''),'','sys','');\n\nFILE_SCHEMA (('AUTOMOTIVE_DESIGN'));\n\
             ENDSEC;\nDATA;\n#1=CARTESIAN_POINT('p',(0.,1.,2.));\nENDSEC;\nEND-ISO-10303-21;\n";
    let p = parse(t).unwrap();
    assert_eq!(
        p.exchange.header.file_name().unwrap().originating_system,
        "sys"
    );
    assert_eq!(p.exchange.header.schemas(), vec!["AUTOMOTIVE_DESIGN"]);
    assert_eq!(p.exchange.get(1).unwrap().name(), "CARTESIAN_POINT");
    assert!(
        matches!(
            p.warnings.as_slice(),
            [Warning::SkippedHeaderRecord { line: 3, .. }]
        ),
        "{:?}",
        p.warnings
    );
    // With nothing after the junk to resume at, the file is an error.
    assert!(parse(
        "ISO-10303-21;\nHEADER;\nFILE_NAME('a');junk').;ENDSEC;DATA;ENDSEC;END-ISO-10303-21;"
    )
    .is_err());
}

/// Skipping stops at DATA when the header's `ENDSEC;` is missing, so the instances are not
/// skipped as header records.
#[test]
fn a_header_without_endsec_does_not_swallow_the_data() {
    for data in ["DATA;", "DATA('d', ('AUTOMOTIVE_DESIGN'));"] {
        let t =
            format!("ISO-10303-21;\nHEADER;\nBAD;\n{data}\n#1=A();\nENDSEC;\nEND-ISO-10303-21;\n");
        assert_eq!(
            refusal(&t),
            Refusal::Syntax {
                line: 4,
                cause: Syntax::HeaderWithoutEndsec
            }
        );
    }
}

/// Many bad header records in a row are skipped in time that grows with the file, and the
/// error after them still names the right line.
#[test]
fn many_bad_header_records_are_skipped_in_one_pass() {
    let n = 100_000;
    let bad = "BAD;\n".repeat(n);
    let ok = format!(
        "ISO-10303-21;\nHEADER;\n{bad}ENDSEC;\nDATA;\n#1=A();\nENDSEC;\nEND-ISO-10303-21;\n"
    );
    let p = parse(&ok).unwrap();
    assert_eq!(p.exchange.get(1).unwrap().name(), "A");
    let broken =
        format!("ISO-10303-21;\nHEADER;\n{bad}DATA;\n#1=A();\nENDSEC;\nEND-ISO-10303-21;\n");
    assert!(matches!(refusal(&broken), Refusal::Syntax { line, .. } if line == n + 3));
}

#[test]
fn hostile_text_is_refused() {
    assert_eq!(refusal(""), Refusal::NotStep);
    assert_eq!(refusal("garbage"), Refusal::NotStep);
    assert_eq!(refusal("ISO-10303-21;"), Refusal::MissingEnd);
    for t in [
        "ISO-10303-21;DATA;#1=A(".to_string(),
        "ISO-10303-21;DATA;#1=A('x".into(),
        "ISO-10303-21;DATA;#1=A(/*".into(),
        file("#=A();"),
        file("#99999999999999999999999=A();"),
        file("#1=A(1,,2);"),
        file("#1=A(1"),
        file("#1=A()"),
        file("#1=A();junk;"),
        format!(
            "ISO-10303-21;DATA;#1=A({}{});ENDSEC;END-ISO-10303-21;",
            "(".repeat(10_000),
            ")".repeat(10_000)
        ),
    ] {
        let e = parse(&t).unwrap_err();
        assert_eq!(e.kind(), ErrorKind::InvalidInput, "{t:.60}: {e}");
    }
    // Padded so that the density limit (a parameter per four bytes) is not what refuses it.
    let pad = format!("/*{}*/", " ".repeat(1000));
    assert_eq!(
        syntax(&format!("{pad}#1=A({}{});", "(".repeat(65), ")".repeat(65))),
        Syntax::NestedTooDeeply { limit: 64 }
    );
    assert!(parse(&file(&format!(
        "{pad}#1=A({}{});",
        "(".repeat(64),
        ")".repeat(64)
    )))
    .is_ok());
}

/// Errors name their cause through `source()`, down to the grammar rule broken.
#[test]
fn errors_chain_to_their_cause() {
    let e = parse(&file("#1=A(T(1.,2.));")).unwrap_err();
    assert_eq!(e.kind(), ErrorKind::InvalidInput);
    assert_eq!(e.to_string(), "STEP input refused: syntax error at line 8: typed parameter `T` does not hold exactly one parameter");
    let refusal = e.source().unwrap();
    assert_eq!(
        refusal.to_string(),
        "syntax error at line 8: typed parameter `T` does not hold exactly one parameter"
    );
    let cause = refusal.source().unwrap();
    assert_eq!(
        cause.to_string(),
        "typed parameter `T` does not hold exactly one parameter"
    );
    assert!(cause.source().is_none());
    // A refused write chains the same way.
    let mut ex = parse(&file("#1=A(1.);")).unwrap().exchange;
    ex.sections[0] = scx_p21::DataSection::new(
        None,
        vec![(1, Instance::simple("A", vec![Param::Real(f64::NAN)]))],
    )
    .0;
    let e = scx_p21::write(&ex).unwrap_err();
    assert!(
        matches!(
            &e,
            P21Error::Refused(Refusal::Unwritable {
                id: Some(1),
                cause: Unwritable::NonFiniteReal(_)
            })
        ),
        "{e:?}"
    );
    assert!(e.source().unwrap().source().is_some());
}

/// The limits count across the whole file; a raised size raises the counts with it.
#[test]
fn limits() {
    let at = |t: &str, f: &dyn Fn(&mut Limits)| {
        let mut l = Limits::default();
        f(&mut l);
        parse_with(t, l, &scx_p21::Unwatched)
    };
    let too_many = |r: Result<_, P21Error>| match r {
        Err(P21Error::Refused(Refusal::TooMany { what, limit })) => Some((what, limit)),
        _ => None,
    };
    // With no header, so that only these count. `$`, the list, its `$`, the typed value and
    // its `1.`: five parameters.
    let data = |d: &str| format!("ISO-10303-21;DATA;{d}ENDSEC;END-ISO-10303-21;");
    let t = data("#1=A($,($),T(1.));");
    assert!(at(&t, &|l| l.params = 5).is_ok());
    assert_eq!(
        too_many(at(&t, &|l| l.params = 4)),
        Some((Counted::Params, 4))
    );
    let two = data("#1=A($,$);#2=A($,$);");
    assert_eq!(
        too_many(at(&two, &|l| l.params = 3)),
        Some((Counted::Params, 3))
    );
    // Empty parts of complex instances hold no parameter but each is a record.
    let t = data("#1=(A()B()C());");
    let records = |n| at(&t, &|l| l.records = n);
    assert!(records(3).is_ok());
    assert_eq!(too_many(records(2)), Some((Counted::Records, 2)));
    // A repeated id counts again.
    let t = data("#1=A();#1=A();#1=A();");
    assert_eq!(
        too_many(at(&t, &|l| l.instances = 2)),
        Some((Counted::Instances, 2))
    );
    assert!(at(&data("#1=A();#2=A();"), &|l| l.instances = 2).is_ok());
    // The header's records are not counted as records (their parameters are).
    assert!(at(&file(""), &|l| l.records = 0).is_ok());
    // The size.
    let t = file("#1=A($);");
    assert_eq!(
        at(&t, &|l| l.bytes = t.len() - 1).unwrap_err(),
        P21Error::Refused(Refusal::TooLarge {
            bytes: t.len(),
            limit: t.len() - 1
        })
    );
    assert!(at(&t, &|l| l.bytes = t.len()).is_ok());
    assert_eq!(Limits::for_bytes(0), Limits::default());
    let four = Limits::for_bytes(4 * MAX_BYTES);
    assert_eq!(four.params, 4 * Limits::default().params);
    // A dense text's counts follow its own length: `$,` is two bytes a parameter.
    let dense = file(&format!("#1=A({}$);", "$,".repeat(400)));
    assert_eq!(
        too_many(parse(&dense)),
        Some((Counted::Params, dense.len() / 4))
    );
    assert!(parse(&file(&format!("#1=A({}$);", "1.5,".repeat(400)))).is_ok());
}

/// A text whose reading would take more memory than the caller can spare is refused before
/// anything is read.
#[test]
fn memory_is_checked_first() {
    let t = file("#1=A();");
    let needed = t.len() * PEAK_PER_BYTE;
    assert!(parse_with(
        &t,
        Limits::default().with_memory(needed),
        &scx_p21::Unwatched
    )
    .is_ok());
    let refused = P21Error::Refused(Refusal::NeedsMemory {
        needed,
        available: needed - 1,
    });
    assert_eq!(
        parse_with(
            &t,
            Limits::default().with_memory(needed - 1),
            &scx_p21::Unwatched
        )
        .unwrap_err(),
        refused
    );
    assert_eq!(
        scx_p21::check_memory(t.len(), needed - 1).unwrap_err(),
        refused
    );
    // Not even the magic is read first.
    let e = parse_with(
        "not a step file",
        Limits::default().with_memory(1),
        &scx_p21::Unwatched,
    )
    .unwrap_err();
    assert!(matches!(e, P21Error::Refused(Refusal::NeedsMemory { .. })));
    assert_eq!(
        Refusal::NeedsMemory {
            needed: 11 << 30,
            available: 8 << 30
        }
        .to_string(),
        "reading the file needs about 11264 MB of memory, more than the 8192 MB available"
    );
}

/// Counts the checks and stops at the `stop_at`th.
struct Watch {
    stop_at: usize,
    checks: std::sync::atomic::AtomicUsize,
    last: std::sync::atomic::AtomicUsize,
}

impl Progress for Watch {
    fn stop(&self) -> bool {
        self.checks
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed)
            + 1
            >= self.stop_at
    }
    fn read(&self, done: usize, total: usize) {
        assert!(done <= total);
        let before = self.last.swap(done, std::sync::atomic::Ordering::Relaxed);
        assert!(before <= done, "progress went back from {before} to {done}");
    }
}

#[test]
fn progress_is_told_and_can_stop() {
    let body: String = (1..=20_000)
        .map(|i| format!("#{i}=CARTESIAN_POINT('',({i}.,0.,0.));\n"))
        .collect();
    let t = file(&body);
    let watch = |stop_at| Watch {
        stop_at,
        checks: 0.into(),
        last: 0.into(),
    };
    let all = watch(usize::MAX);
    let p = parse_with(&t, Limits::default(), &all).unwrap();
    assert_eq!(p.exchange.len(), 20_000);
    // Asked at the first instance and every 4096 after; never told less than before.
    assert_eq!(all.checks.into_inner(), 1 + 20_000 / 4096);
    let stopped = parse_with(&t, Limits::default(), &watch(2)).unwrap_err();
    assert_eq!(stopped, P21Error::Cancelled);
    assert_eq!(stopped.kind(), ErrorKind::Cancelled);
}
