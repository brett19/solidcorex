// Derived from solidcore `solidcore/crates/solidcore-record/src/step/p21.rs` and
// `solidcore/crates/solidcore-step/src/write.rs` at commit
// 03e16c9ee766de5b9b1ced845e5149d62ff493b8 (their own origin: SolidCraft's `crates/step`,
// MIT OR Apache-2.0).
// Upstream record (verbatim from those files): ISO 10303-21 ("Part 21") exchange structure
// parser: the HEADER and the DATA section's entity instances, simple (`#1=NAME(...)`) and
// complex (`#2=(A(...) B(...))`), with the string escapes decoded (`\X\hh`, `\X2\…\X0\`,
// `\X4\…\X0\`, `\S\c`, `\\`). Written from the public ISO 10303-21 specification.
// Writing ISO 10303-21: a DATA section built entity by entity, and [`crate::geom`] curves and
// surfaces as ISO 10303-42 entities. Identical geometric entities are written once and shared,
// as the reader shares them; topological ones are distinct ([`Writer::add_distinct`]).
// [`topology`] writes a schema 4 record's solids.
// Changes: only the text tokens of those files move here: `SHORT_NAMES` and `long_name`,
// the string decoder (`decode_string`, `decode_string_with`), and the writer's `real`,
// `real_into`, `real_text`, `RealText` and `string`. `decode_string` became `decode`, which
// also follows the code page directives `\PA\`..`\PI\` (§6.4.3.2; a `\S\` off page A reads
// as U+FFFD and is reported), returns U+FFFD (was `?`) for a `\X4\` group that is no
// character, and keeps the terminator search of `decode_string_with` (linear time) inline.
// `real` became `real_into` alone, refusing a value that is not finite (was written as `0.`
// and counted). `string` became `encode`: it writes a control character as `\X\hh` (it was
// dropped, so the text did not read back) and a run of non-ASCII characters as one `\X2\` or
// `\X4\` group. `keyword_ok` and `enumeration_ok` are new: the writer checks names against
// §5.4.
// `hex`, `binary_ok` and `RealText::write_str` are new helpers of those. The tests are new:
// `dec`, `enc`, `decodes_each_directive`, `unknown_and_broken_directives_are_kept_as_written`,
// `code_pages_switch_s_and_other_pages_are_reported`, `unterminated_runs_decode_in_linear_time`,
// `encodes_so_that_decoding_gives_it_back`, `reals_are_shortest_with_a_point_and_capital_e`,
// `names_follow_the_grammar`, `short_names_read_long`.
//! Part 21's text tokens: decoding and encoding strings (ISO 10303-21:2016 §6.4.3), writing
//! reals (§6.4.2), and the AP203 short entity names (§12.2.11).

use core::fmt::Write as _;

use crate::error::Unwritable;

/// Part 21 short entity names (the short-name form ISO 10303-21 allows, which some AP203
/// writers use) of the entities a B-rep, its representation, product and units are read
/// from, with their long names, as ISO's short-names file gives them
/// (<http://standards.iso.org/iso/10303/tech/short_names/short-names.txt>).
const SHORT_NAMES: [(&str, &str); 36] = [
    ("A2PL3D", "AXIS2_PLACEMENT_3D"),
    ("ADVFC", "ADVANCED_FACE"),
    ("BSCWK", "B_SPLINE_CURVE_WITH_KNOTS"),
    ("BSSWK", "B_SPLINE_SURFACE_WITH_KNOTS"),
    ("CLSSHL", "CLOSED_SHELL"),
    ("CMCRSG", "COMPOSITE_CURVE_SEGMENT"),
    ("CMPCRV", "COMPOSITE_CURVE"),
    ("CNBSUN", "CONVERSION_BASED_UNIT"),
    ("CRTPNT", "CARTESIAN_POINT"),
    ("DMNEXP", "DIMENSIONAL_EXPONENTS"),
    ("DRCTN", "DIRECTION"),
    ("EDGCRV", "EDGE_CURVE"),
    ("EDGLP", "EDGE_LOOP"),
    ("FBSR", "FACETED_BREP_SHAPE_REPRESENTATION"),
    ("FCBND", "FACE_BOUND"),
    ("FCOTBN", "FACE_OUTER_BOUND"),
    ("FCSRF", "FACE_SURFACE"),
    ("FCTBR", "FACETED_BREP"),
    ("GMRPCN", "GEOMETRIC_REPRESENTATION_CONTEXT"),
    ("GMTST", "GEOMETRIC_SET"),
    ("LMWU", "LENGTH_MEASURE_WITH_UNIT"),
    ("LNGUNT", "LENGTH_UNIT"),
    ("MNSLBR", "MANIFOLD_SOLID_BREP"),
    ("NMDUNT", "NAMED_UNIT"),
    ("ORNEDG", "ORIENTED_EDGE"),
    ("PAMWU", "PLANE_ANGLE_MEASURE_WITH_UNIT"),
    ("PLYLP", "POLY_LOOP"),
    ("PRDCT", "PRODUCT"),
    ("PRDDFN", "PRODUCT_DEFINITION"),
    ("PRDFSH", "PRODUCT_DEFINITION_SHAPE"),
    ("SHDFRP", "SHAPE_DEFINITION_REPRESENTATION"),
    ("SHPRPR", "SHAPE_REPRESENTATION"),
    ("SHRPRL", "SHAPE_REPRESENTATION_RELATIONSHIP"),
    ("TRMCRV", "TRIMMED_CURVE"),
    ("UMWU", "UNCERTAINTY_MEASURE_WITH_UNIT"),
    ("VRTPNT", "VERTEX_POINT"),
];

/// An entity name as its long form where it is one of [`SHORT_NAMES`].
pub(crate) fn long_name(name: String) -> String {
    // Every short name is at most six letters: a longer name is never looked up.
    if name.len() > 6 {
        return name;
    }
    match SHORT_NAMES.iter().find(|(short, _)| *short == name) {
        Some((_, long)) => (*long).to_string(),
        None => name,
    }
}

/// The value of hexadecimal digits.
fn hex(b: &[u8]) -> Option<u32> {
    core::str::from_utf8(b)
        .ok()
        .and_then(|t| u32::from_str_radix(t, 16).ok())
}

/// What decoding a string met that its reader should report.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct Decoded {
    /// The first code page other than `A` that a `\S\` was read under.
    pub(crate) unsupported_page: Option<char>,
}

/// Decode a string's control directives (§6.4.3): `\\`, `\X\hh`, `\X2\…\X0\`, `\X4\…\X0\`,
/// `\S\c` and the code pages `\PA\`..`\PI\`. `raw` is the text between the apostrophes with
/// doubled apostrophes made single and line breaks removed (§5.6). Anything else after a
/// backslash is kept as written.
///
/// The search for a run's `\X0\` is remembered: the first one at or after a position is also
/// the first at or after every later position up to it, and when none follows one position
/// none follows a later one. So the searches cover disjoint stretches of `raw` and decoding is
/// linear however many unterminated runs the string holds.
pub(crate) fn decode(raw: &[u8], out: &mut String) -> Decoded {
    let mut found = Decoded::default();
    let mut page = 'A';
    let mut i = 0;
    // The last search's answer: the position of the first `\X0\` it found (`None`: none to
    // the end); `None` outside: no search yet.
    let mut next_end: Option<Option<usize>> = None;
    while let Some(&c) = raw.get(i) {
        if c != b'\\' {
            // Plain bytes, up to the next backslash: whole UTF-8 characters (the backslash is
            // ASCII), so the slice is valid wherever `raw` is.
            let st = i;
            while raw.get(i).is_some_and(|c| *c != b'\\') {
                i += 1;
            }
            out.push_str(&String::from_utf8_lossy(raw.get(st..i).unwrap_or(&[])));
            continue;
        }
        let rest = raw.get(i..).unwrap_or(&[]);
        if rest.starts_with(b"\\\\") {
            out.push('\\');
            i += 2;
            continue;
        }
        if rest.starts_with(b"\\X\\")
            && let Some(v) = rest.get(3..5).and_then(hex)
        {
            // Two hexadecimal digits: always a character (U+0000 to U+00FF).
            out.push(char::from_u32(v).unwrap_or(char::REPLACEMENT_CHARACTER));
            i += 5;
            continue;
        }
        let width = if rest.starts_with(b"\\X2\\") {
            4
        } else if rest.starts_with(b"\\X4\\") {
            8
        } else {
            0
        };
        if width > 0 {
            let end = match next_end {
                Some(Some(end)) if end >= i => Some(end),
                Some(None) => None,
                _ => {
                    let end = rest
                        .windows(4)
                        .position(|w| w == b"\\X0\\")
                        .map(|end| i + end);
                    next_end = Some(end);
                    end
                }
            };
            if let Some(end) = end.and_then(|end| end.checked_sub(i)) {
                let body = rest.get(4..end).unwrap_or(&[]);
                let units = body.chunks(width).filter_map(hex);
                if width == 4 {
                    let u16s: Vec<u16> = units.map(|u| u as u16).collect();
                    out.push_str(&String::from_utf16_lossy(&u16s));
                } else {
                    out.extend(
                        units.map(|u| char::from_u32(u).unwrap_or(char::REPLACEMENT_CHARACTER)),
                    );
                }
                // A run may be followed straight away by the backslash of the next escape.
                i += end + 4;
                continue;
            }
        }
        // `\PA\`..`\PI\`: the ISO 8859 part `\S\` reads from (§6.4.3.2).
        if let (Some(b'P'), Some(&p @ b'A'..=b'I'), Some(b'\\')) =
            (rest.get(1), rest.get(2), rest.get(3))
        {
            page = char::from(p);
            i += 4;
            continue;
        }
        // `\S\` takes a basic-alphabet character (space to `~`); before any other byte, the
        // start of a UTF-8 character, adding 128 would split it, so the escape is kept as
        // written.
        if rest.starts_with(b"\\S\\")
            && let Some(&ch) = rest.get(3).filter(|ch| (b' '..=b'~').contains(*ch))
        {
            if page == 'A' {
                // ISO 8859-1 is Unicode's first 256 code points.
                out.push(char::from(ch + 128));
            } else {
                found.unsupported_page.get_or_insert(page);
                out.push(char::REPLACEMENT_CHARACTER);
            }
            i += 4;
            continue;
        }
        out.push('\\');
        i += 1;
    }
    found
}

/// Write `s` as a Part 21 string, its apostrophes (§6.4.3.1).
///
/// Apostrophes and backslashes are doubled; the other printable ASCII characters are written
/// as they are; a control character as `\X\hh` (§6.4.3.4: a line break must be encoded);
/// a run of other characters as one `\X2\` group (all in the basic plane) or one `\X4\` group
/// (§6.4.3.3). The text is ASCII, which every edition's reader reads.
pub(crate) fn encode(s: &str, o: &mut String) {
    o.push('\'');
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '\'' => o.push_str("''"),
            '\\' => o.push_str("\\\\"),
            ' '..='~' => o.push(c),
            c if c.is_ascii() => {
                let _ = write!(o, "\\X\\{:02X}", u32::from(c));
            }
            c => {
                // A run of non-ASCII characters, all in the basic plane or all outside it.
                let wide = u32::from(c) > 0xFFFF;
                o.push_str(if wide { "\\X4\\" } else { "\\X2\\" });
                let mut put = |c: char| {
                    if wide {
                        let _ = write!(o, "{:08X}", u32::from(c));
                    } else {
                        let _ = write!(o, "{:04X}", u32::from(c));
                    }
                };
                put(c);
                while let Some(&n) = chars.peek() {
                    if n.is_ascii() || (u32::from(n) > 0xFFFF) != wide {
                        break;
                    }
                    put(n);
                    chars.next();
                }
                o.push_str("\\X0\\");
            }
        }
    }
    o.push('\'');
}

/// Room for the longest text a finite `f64`'s shortest form makes: a sign, 17 digits, `.`,
/// `e`, a sign and three exponent digits.
const REAL_MAX: usize = 32;

/// `{x:?}` taken into a stack buffer, so writing a real allocates nothing.
struct RealText {
    bytes: [u8; REAL_MAX],
    len: usize,
}

impl core::fmt::Write for RealText {
    fn write_str(&mut self, s: &str) -> core::fmt::Result {
        let end = self.len.checked_add(s.len()).ok_or(core::fmt::Error)?;
        let slot = self.bytes.get_mut(self.len..end).ok_or(core::fmt::Error)?;
        slot.copy_from_slice(s.as_bytes());
        self.len = end;
        Ok(())
    }
}

/// Write `x` as a Part 21 real (§6.4.2): the shortest decimal that reads back as the same
/// `f64`, always with a decimal point in the mantissa and the exponent as `E` (`1.`,
/// `-0.`, `2.5E-7`). A value that is not finite has no form and is refused.
pub(crate) fn real_into(x: f64, o: &mut String) -> Result<(), Unwritable> {
    if !x.is_finite() {
        return Err(Unwritable::NonFiniteReal(x));
    }
    let mut t = RealText {
        bytes: [0; REAL_MAX],
        len: 0,
    };
    let owned;
    let s = if write!(t, "{x:?}").is_ok() {
        core::str::from_utf8(t.bytes.get(..t.len).unwrap_or(&[])).unwrap_or("")
    } else {
        // Not reachable for a finite `f64`; keep the bytes exact all the same.
        owned = format!("{x:?}");
        &owned
    };
    let (m, e) = match s.split_once('e') {
        Some((m, e)) => (m, Some(e)),
        None => (s, None),
    };
    // `{:?}` writes `1.0`; Part 21's shortest is `1.`.
    o.push_str(m.strip_suffix(".0").unwrap_or(m));
    if !m.contains('.') || m.ends_with(".0") {
        o.push('.');
    }
    if let Some(e) = e {
        o.push('E');
        o.push_str(e);
    }
    Ok(())
}

/// Whether `k` is a §5.4 keyword: `UPPER {UPPER | DIGIT}`, `!` first for a user-defined one
/// (`UPPER` is `A`..`Z` and `_`).
pub(crate) fn keyword_ok(k: &str) -> bool {
    let b = k.strip_prefix('!').unwrap_or(k).as_bytes();
    matches!(b.first(), Some(c) if c.is_ascii_uppercase() || *c == b'_')
        && b.iter()
            .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || *c == b'_')
}

/// Whether `e` is the value of a §5.4 enumeration: `UPPER {UPPER | DIGIT}`.
pub(crate) fn enumeration_ok(e: &str) -> bool {
    !e.starts_with('!') && keyword_ok(e)
}

/// Whether `b` is the text of a §5.4 binary: `0`..`3` then upper-case hexadecimal digits.
pub(crate) fn binary_ok(b: &str) -> bool {
    let b = b.as_bytes();
    matches!(b.first(), Some(b'0'..=b'3'))
        && b.iter()
            .skip(1)
            .all(|c| c.is_ascii_digit() || (b'A'..=b'F').contains(c))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dec(raw: &str) -> String {
        let mut s = String::new();
        decode(raw.as_bytes(), &mut s);
        s
    }

    fn enc(s: &str) -> String {
        let mut o = String::new();
        encode(s, &mut o);
        o
    }

    fn real(x: f64) -> String {
        let mut o = String::new();
        real_into(x, &mut o).unwrap();
        o
    }

    #[test]
    fn decodes_each_directive() {
        // §6.4.3.4 `\X\hh`, §6.4.3.3 `\X2\` and `\X4\`, §6.4.3.2 `\S\`, §6.4.3.1 `\\`.
        assert_eq!(dec(r"\X\E9t\X\E9"), "été");
        assert_eq!(dec(r"\X2\03B103B2\X0\"), "αβ");
        assert_eq!(dec(r"\X4\0001F600\X0\"), "😀");
        assert_eq!(dec(r"\S\i"), "é");
        assert_eq!(dec(r"a\\b"), "a\\b");
        // A run followed at once by another escape.
        assert_eq!(dec(r"\X2\00E9\X0\\X\E9"), "éé");
        // A surrogate pair inside `\X2\` (UTF-16), and a lone surrogate as U+FFFD.
        assert_eq!(dec(r"\X2\D83DDE00\X0\"), "😀");
        assert_eq!(dec(r"\X2\D83D\X0\"), "\u{FFFD}");
        // A `\X4\` group beyond U+10FFFF is no character.
        assert_eq!(dec(r"\X4\00110000\X0\"), "\u{FFFD}");
    }

    #[test]
    fn unknown_and_broken_directives_are_kept_as_written() {
        assert_eq!(dec(r"\Q\x"), r"\Q\x");
        assert_eq!(dec(r"\X\G1"), r"\X\G1");
        assert_eq!(dec(r"\X2\00E9"), r"\X2\00E9");
        // `\S\` before a non-ASCII character (UTF-8, edition 3) is kept.
        assert_eq!(dec("\\S\\é"), "\\S\\é");
        assert_eq!(dec("é"), "é");
    }

    #[test]
    fn code_pages_switch_s_and_other_pages_are_reported() {
        let mut s = String::new();
        let d = decode(br"\PA\\S\i", &mut s);
        assert_eq!((s.as_str(), d.unsupported_page), ("é", None));
        let mut s = String::new();
        let d = decode(br"\PE\\S\i\PA\\S\i", &mut s);
        assert_eq!((s.as_str(), d.unsupported_page), ("\u{FFFD}é", Some('E')));
    }

    #[test]
    fn unterminated_runs_decode_in_linear_time() {
        // Each `\X2\` without its `\X0\` searches no further than the last search reached.
        // The space keeps a tag's closing backslash from pairing with the next tag's opening
        // one as an escaped `\\`.
        for tag in [r"\X2\ ", r"\X4\ "] {
            let raw = tag.repeat(50_000);
            assert_eq!(dec(&raw), raw);
        }
    }

    #[test]
    fn encodes_so_that_decoding_gives_it_back() {
        for s in [
            "",
            "plain",
            "it's",
            r"back\slash",
            "line\nbreak\ttab\u{7f}",
            "été",
            "αβγ and 😀😀 and é😀é",
            "\u{FFFD}",
            r"\X2\ literal",
        ] {
            let e = enc(s);
            assert!(e.is_ascii(), "{e}");
            let inner = &e[1..e.len() - 1];
            let raw = inner.replace("''", "'");
            assert_eq!(dec(&raw), s, "{e}");
        }
        assert_eq!(enc("a'b"), "'a''b'");
        assert_eq!(enc("\n"), r"'\X\0A'");
        assert_eq!(enc("αβ"), r"'\X2\03B103B2\X0\'");
        assert_eq!(enc("é😀"), r"'\X2\00E9\X0\\X4\0001F600\X0\'");
    }

    #[test]
    fn reals_are_shortest_with_a_point_and_capital_e() {
        // §6.4.2: the decimal point is required, the exponent letter is `E`.
        assert_eq!(real(1.0), "1.");
        assert_eq!(real(-0.0), "-0.");
        assert_eq!(real(0.5), "0.5");
        assert_eq!(real(100.0), "100.");
        assert_eq!(real(1e-7), "1.E-7");
        assert_eq!(real(1.5e300), "1.5E300");
        assert_eq!(real(f64::MAX), "1.7976931348623157E308");
        assert_eq!(real(5e-324), "5.E-324");
        let mut o = String::new();
        assert_eq!(
            real_into(f64::NAN, &mut o).map_err(|e| matches!(e, Unwritable::NonFiniteReal(_))),
            Err(true)
        );
        assert!(real_into(f64::INFINITY, &mut o).is_err());
    }

    #[test]
    fn names_follow_the_grammar() {
        assert!(keyword_ok("CARTESIAN_POINT"));
        assert!(keyword_ok("_X1"));
        assert!(keyword_ok("!USER_DEFINED"));
        assert!(!keyword_ok("1ABC"));
        assert!(!keyword_ok("lower"));
        assert!(!keyword_ok(""));
        assert!(!keyword_ok("!"));
        assert!(!keyword_ok("A-B"));
        assert!(enumeration_ok("T"));
        assert!(!enumeration_ok("!T"));
        assert!(binary_ok("0"));
        assert!(binary_ok("3FA"));
        assert!(!binary_ok("4"));
        assert!(!binary_ok("0fa"));
        assert!(!binary_ok(""));
    }

    #[test]
    fn short_names_read_long() {
        assert_eq!(long_name("CRTPNT".into()), "CARTESIAN_POINT");
        assert_eq!(long_name("CMCRSG".into()), "COMPOSITE_CURVE_SEGMENT");
        assert_eq!(long_name("CARTESIAN_POINT".into()), "CARTESIAN_POINT");
    }
}
