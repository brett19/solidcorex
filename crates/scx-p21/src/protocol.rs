// Derived from solidcore `solidcore/crates/solidcore-record/src/step/protocol.rs` at commit
// 03e16c9ee766de5b9b1ced845e5149d62ff493b8 (new in solidcore, no SolidCraft or SMLib
// counterpart).
// Upstream record (verbatim from that file): Which application protocol a STEP file declares
// in its `FILE_SCHEMA`: AP203, AP214, or AP242 and its edition. A file is read the same
// whatever it declares; an AP242 edition that isn't given or isn't known, and a schema that
// isn't one of these, read with a warning, never a refusal.
// Changes: the warning is a typed `Doubt` (was a `String` built in `declared`), its text
// that of the pin, from `Doubt`'s `Display` (`Doubt::fmt`, new); `Declared::doubt` in place of
// `Declared::warning`; `known` and `warned` folded into `declared`; `Ap242Edition::number` is
// `const`; `of_exchange` is new (the schemas of a parsed exchange). `split`, `edition` and
// `shown` are kept as the pin has them; the tests' let-chain is unrolled
// Tests: `ap242_without_identifier_is_read_with_a_warning`,
// `unknown_identifier_is_read_with_a_warning` and `unknown_or_missing_schema_is_read_with_a_warning`
// are merged into `doubts_are_typed_and_read_as_the_pin_words_them` (the doubts typed, their
// words checked); `ap242`, `ap242_editions_by_identifier`, `Lcg::below` and
// `hostile_names_never_panic` changed (the `AP242` constant in the name, `of` over the
// test's `&str` schemas, `usize::try_from` for an `as` cast, the let-chain unrolled);
// `a_parsed_header_declares_its_protocol` is new.
//! Which application protocol a STEP file declares in its `FILE_SCHEMA`: AP203, AP214, or
//! AP242 and its edition. A file is read the same whatever it declares; an AP242 edition that
//! is not given or not known, and a schema that is not one of these, read with a [`Doubt`],
//! never a refusal.
//!
//! Every AP242 edition has the same schema name; only the object identifier after it, in
//! braces, tells the editions apart. Written from public descriptions of the schema names and
//! identifiers (STEP Tools' notes on each protocol, MBx-IF's table of EXPRESS schemas). They
//! differ on Edition 4's identifier (`442 5 1 4` and `442 7 1 4`), so both are read as
//! Edition 4.
//!
//! Input is hostile: at most [`MAX_SCHEMAS`] names are looked at, at most `MAX_ARCS` arcs of
//! one identifier are read, and a doubt repeats at most `MAX_SHOWN` characters of the file's
//! text.

use core::fmt;

use crate::exchange::Exchange;

/// Most `FILE_SCHEMA` names looked at for a protocol.
pub const MAX_SCHEMAS: usize = 16;
/// Most arcs read from one object identifier: the AP242 identifiers have seven.
const MAX_ARCS: usize = 16;
/// Most characters of the file's own text one doubt repeats.
const MAX_SHOWN: usize = 80;

/// The AP242 schema name, shared by every edition.
const AP242: &str = "AP242_MANAGED_MODEL_BASED_3D_ENGINEERING_MIM_LF";
/// AP203's names: Edition 1, Edition 2, and the interim name met in older files.
const AP203: [&str; 3] = [
    "CONFIG_CONTROL_DESIGN",
    "AP203_CONFIGURATION_CONTROLLED_3D_DESIGN_OF_MECHANICAL_PARTS_AND_ASSEMBLIES_MIM_LF",
    "CONFIGURATION_CONTROL_3D_DESIGN_MIM_LF",
];
/// AP214's names: every edition's, and the older conformance-class name.
const AP214: [&str; 2] = ["AUTOMOTIVE_DESIGN", "AUTOMOTIVE_DESIGN_CC2"];

/// An edition of AP242.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Ap242Edition {
    /// Edition 1: `{ 1 0 10303 442 1 1 4 }`.
    First,
    /// Edition 2: `{ 1 0 10303 442 3 1 4 }`.
    Second,
    /// Edition 3: `{ 1 0 10303 442 4 1 4 }`.
    Third,
    /// Edition 4: `{ 1 0 10303 442 7 1 4 }`, or `{ 1 0 10303 442 5 1 4 }` as some sources
    /// give it.
    Fourth,
}

impl Ap242Edition {
    /// The edition's number, 1 to 4.
    #[must_use]
    pub const fn number(self) -> u8 {
        match self {
            Self::First => 1,
            Self::Second => 2,
            Self::Third => 3,
            Self::Fourth => 4,
        }
    }
}

/// The application protocol a file declares.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Protocol {
    /// AP203, configuration controlled design.
    Ap203,
    /// AP214, automotive design.
    Ap214,
    /// AP242, managed model-based 3D engineering: the edition, when the file gives one that
    /// is known.
    Ap242(Option<Ap242Edition>),
    /// No schema, or none of the above.
    Unknown,
}

impl fmt::Display for Protocol {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Ap203 => f.write_str("AP203"),
            Self::Ap214 => f.write_str("AP214"),
            Self::Ap242(Some(edition)) => write!(f, "AP242 edition {}", edition.number()),
            Self::Ap242(None) => f.write_str("AP242"),
            Self::Unknown => f.write_str("an unknown protocol"),
        }
    }
}

/// Why a declared protocol is not fully known. The file is read anyway.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum Doubt {
    /// AP242 with no identifier after its name.
    NoEdition,
    /// AP242 with an identifier that names no edition known here (shown, cut to
    /// `MAX_SHOWN` characters).
    UnknownEdition(String),
    /// A first schema that is none of AP203, AP214 and AP242 (shown, cut).
    UnknownSchema(String),
    /// No schema at all, or an empty one.
    NoSchema,
}

impl fmt::Display for Doubt {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NoEdition => {
                f.write_str("the file declares AP242 without its edition; it is read as AP242")
            }
            Self::UnknownEdition(id) => write!(
                f,
                "the file declares AP242 with the identifier {{ {id} }}, which names no edition known here; it is read as AP242"
            ),
            Self::UnknownSchema(s) => write!(
                f,
                "the file's schema {s} is not AP203, AP214 or AP242; it is read as far as it goes"
            ),
            Self::NoSchema => {
                f.write_str("the file declares no schema; it is read as far as it goes")
            }
        }
    }
}

/// What a file's `FILE_SCHEMA` declares.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Declared {
    /// The protocol of the first name that is a known one.
    pub protocol: Protocol,
    /// Why the protocol is not fully known, when it is not.
    pub doubt: Option<Doubt>,
}

/// The protocol the `FILE_SCHEMA` names `schemas` declare: the first of the first
/// [`MAX_SCHEMAS`] names that is AP203, AP214 or AP242, compared without case. An AP242
/// edition comes from the identifier after the name.
#[must_use]
pub fn declared(schemas: &[String]) -> Declared {
    let known = |protocol| Declared {
        protocol,
        doubt: None,
    };
    let found = schemas.iter().take(MAX_SCHEMAS).find_map(|schema| {
        let (name, identifier) = split(schema);
        let named = |names: &[&str]| names.iter().any(|n| n.eq_ignore_ascii_case(name));
        if name.eq_ignore_ascii_case(AP242) {
            let edition = identifier.and_then(|(id, closed)| edition(id).filter(|_| closed));
            Some(match (edition, identifier) {
                (Some(e), _) => known(Protocol::Ap242(Some(e))),
                (None, None) => Declared {
                    protocol: Protocol::Ap242(None),
                    doubt: Some(Doubt::NoEdition),
                },
                (None, Some((id, _))) => Declared {
                    protocol: Protocol::Ap242(None),
                    doubt: Some(Doubt::UnknownEdition(shown(id))),
                },
            })
        } else if named(&AP203) {
            Some(known(Protocol::Ap203))
        } else if named(&AP214) {
            Some(known(Protocol::Ap214))
        } else {
            None
        }
    });
    found.unwrap_or_else(|| {
        let doubt = match schemas.first().map(|s| s.trim()).filter(|s| !s.is_empty()) {
            Some(first) => Doubt::UnknownSchema(shown(first)),
            None => Doubt::NoSchema,
        };
        Declared {
            protocol: Protocol::Unknown,
            doubt: Some(doubt),
        }
    })
}

/// The protocol a parsed exchange's `FILE_SCHEMA` declares.
#[must_use]
pub fn of_exchange(ex: &Exchange) -> Declared {
    declared(&ex.header.schemas())
}

/// A schema's name and its identifier, both trimmed: the text inside the braces, and whether
/// its `}` closes it. An identifier that is not closed names no edition, but is shown in the
/// doubt.
fn split(schema: &str) -> (&str, Option<(&str, bool)>) {
    match schema.split_once('{') {
        Some((name, rest)) => {
            let identifier = match rest.split_once('}') {
                Some((inside, _)) => (inside.trim(), true),
                None => (rest.trim(), false),
            };
            (name.trim(), Some(identifier))
        }
        None => (schema.trim(), None),
    }
}

/// The AP242 edition an identifier's arcs name, if it names one.
fn edition(identifier: &str) -> Option<Ap242Edition> {
    let arcs: Option<Vec<u32>> = identifier
        .split_ascii_whitespace()
        .take(MAX_ARCS + 1)
        .map(|arc| arc.parse().ok())
        .collect();
    match arcs?.as_slice() {
        [1, 0, 10303, 442, 1, 1, 4] => Some(Ap242Edition::First),
        [1, 0, 10303, 442, 3, 1, 4] => Some(Ap242Edition::Second),
        [1, 0, 10303, 442, 4, 1, 4] => Some(Ap242Edition::Third),
        [1, 0, 10303, 442, 7 | 5, 1, 4] => Some(Ap242Edition::Fourth),
        _ => None,
    }
}

/// `text` as a doubt repeats it: at most [`MAX_SHOWN`] characters, then `…`.
fn shown(text: &str) -> String {
    let mut chars = text.chars();
    let mut out: String = chars.by_ref().take(MAX_SHOWN).collect();
    if chars.next().is_some() {
        out.push('…');
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn of(schemas: &[&str]) -> Declared {
        declared(&schemas.iter().map(ToString::to_string).collect::<Vec<_>>())
    }

    fn known(protocol: Protocol) -> Declared {
        Declared {
            protocol,
            doubt: None,
        }
    }

    fn ap242(id: &str) -> String {
        format!("{AP242} {{ {id} }}")
    }

    #[test]
    fn ap242_editions_by_identifier() {
        for (id, edition) in [
            ("1 0 10303 442 1 1 4", Ap242Edition::First),
            ("1 0 10303 442 3 1 4", Ap242Edition::Second),
            ("1 0 10303 442 4 1 4", Ap242Edition::Third),
            ("1 0 10303 442 7 1 4", Ap242Edition::Fourth),
            ("1 0 10303 442 5 1 4", Ap242Edition::Fourth),
        ] {
            assert_eq!(
                of(&[&ap242(id)]),
                known(Protocol::Ap242(Some(edition))),
                "{id}"
            );
        }
        assert_eq!(
            Protocol::Ap242(Some(Ap242Edition::Fourth)).to_string(),
            "AP242 edition 4"
        );
    }

    #[test]
    fn ap203_and_ap214_by_name() {
        for name in AP203 {
            assert_eq!(of(&[name]), known(Protocol::Ap203), "{name}");
            let with_id = format!("{name} {{ 1 0 10303 403 2 1 2 }}");
            assert_eq!(of(&[&with_id]), known(Protocol::Ap203), "{with_id}");
        }
        for name in AP214 {
            assert_eq!(of(&[name]), known(Protocol::Ap214), "{name}");
            let with_id = format!("{name} {{ 1 0 10303 214 1 1 1 1 }}");
            assert_eq!(of(&[&with_id]), known(Protocol::Ap214), "{with_id}");
        }
    }

    #[test]
    fn names_ignore_case_and_spaces() {
        let third = known(Protocol::Ap242(Some(Ap242Edition::Third)));
        for schema in [
            "ap242_managed_model_based_3d_engineering_mim_lf { 1 0 10303 442 4 1 4 }",
            "Ap242_Managed_Model_Based_3D_Engineering_Mim_Lf{1 0 10303 442 4 1 4}",
            "  AP242_MANAGED_MODEL_BASED_3D_ENGINEERING_MIM_LF   {   1  0 10303\t442 4 1 4   }  ",
        ] {
            assert_eq!(of(&[schema]), third, "{schema:?}");
        }
        assert_eq!(of(&["  automotive_design  "]), known(Protocol::Ap214));
        assert_eq!(of(&["Config_Control_Design"]), known(Protocol::Ap203));
    }

    #[test]
    fn doubts_are_typed_and_read_as_the_pin_words_them() {
        let d = of(&[AP242]);
        assert_eq!(d.protocol, Protocol::Ap242(None));
        assert_eq!(d.doubt, Some(Doubt::NoEdition));
        assert_eq!(
            Doubt::NoEdition.to_string(),
            "the file declares AP242 without its edition; it is read as AP242"
        );
        for (schema, id) in [
            (ap242("1 0 10303 442 9 1 4"), "1 0 10303 442 9 1 4"),
            (ap242("1 0 10303 442 1 1"), "1 0 10303 442 1 1"),
            (ap242("1 0 10303 442 x 1 4"), "1 0 10303 442 x 1 4"),
            (
                ap242("1 0 10303 442 99999999999 1 4"),
                "1 0 10303 442 99999999999 1 4",
            ),
            (ap242(""), ""),
            (
                format!("{AP242} {{ 1 0 10303 442 1 1 4"),
                "1 0 10303 442 1 1 4",
            ),
        ] {
            let d = of(&[&schema]);
            assert_eq!(d.protocol, Protocol::Ap242(None), "{schema}");
            assert_eq!(d.doubt, Some(Doubt::UnknownEdition(id.into())), "{schema}");
        }
        assert_eq!(
            Doubt::UnknownEdition("1 2".into()).to_string(),
            "the file declares AP242 with the identifier { 1 2 }, which names no edition known here; it is read as AP242"
        );
        for (schemas, doubt) in [
            (&[][..], Doubt::NoSchema),
            (&[""][..], Doubt::NoSchema),
            (&["  "][..], Doubt::NoSchema),
            (&["X_DESIGN"][..], Doubt::UnknownSchema("X_DESIGN".into())),
        ] {
            let d = of(schemas);
            assert_eq!(d.protocol, Protocol::Unknown, "{schemas:?}");
            assert_eq!(d.doubt.as_ref(), Some(&doubt), "{schemas:?}");
        }
        assert!(
            Doubt::UnknownSchema("X_DESIGN".into())
                .to_string()
                .contains("X_DESIGN is not AP203")
        );
    }

    #[test]
    fn the_first_known_schema_decides() {
        assert_eq!(of(&["X", "AUTOMOTIVE_DESIGN"]), known(Protocol::Ap214));
        let both = [
            "AUTOMOTIVE_DESIGN".to_string(),
            ap242("1 0 10303 442 4 1 4"),
        ];
        assert_eq!(declared(&both), known(Protocol::Ap214));
        // A name past the first MAX_SCHEMAS is not looked at.
        let mut late = vec!["X".to_string(); MAX_SCHEMAS];
        late.push("AUTOMOTIVE_DESIGN".into());
        assert_eq!(declared(&late).protocol, Protocol::Unknown);
    }

    #[test]
    fn a_parsed_header_declares_its_protocol() {
        let t = format!(
            "ISO-10303-21;HEADER;FILE_SCHEMA(('{}'));ENDSEC;END-ISO-10303-21;",
            ap242("1 0 10303 442 4 1 4")
        );
        let ex = crate::parse(&t).unwrap().exchange;
        assert_eq!(
            of_exchange(&ex),
            known(Protocol::Ap242(Some(Ap242Edition::Third)))
        );
    }

    /// A small generator for the hostile shapes.
    struct Lcg(u64);

    impl Lcg {
        fn below(&mut self, n: usize) -> usize {
            self.0 = self
                .0
                .wrapping_mul(6_364_136_223_846_793_005)
                .wrapping_add(1_442_695_040_888_963_407);
            usize::try_from(self.0 >> 33).unwrap() % n.max(1)
        }
    }

    #[test]
    fn hostile_names_never_panic() {
        let names = [
            AP242,
            "ap242_managed_model_based_3d_engineering_mim_lf",
            "AUTOMOTIVE_DESIGN",
            "",
            " ",
            "{",
            "}",
            "}{",
            "{{}}",
            "é漢字🙂",
        ];
        let many_arcs = "1 ".repeat(10_000);
        let ids = [
            "1 0 10303 442 4 1 4",
            "1 0 10303 442 7 1 4",
            "",
            "{",
            "}",
            "999999999999999999999999999999",
            many_arcs.as_str(),
            "1 0 10303 442 4 1 4 }{ 1",
            "é漢字🙂 1 0",
            "-1 0 10303 442 4 1 4",
        ];
        let long_name = "é".repeat(10_000);
        let mut rng = Lcg(13736);
        let longest = MAX_SHOWN + 200;
        for _ in 0..10_000 {
            let count = rng.below(MAX_SCHEMAS + 4);
            let mut schemas = Vec::new();
            for _ in 0..count {
                let name = match rng.below(12) {
                    10 => long_name.as_str(),
                    11 => many_arcs.as_str(),
                    i => names.get(i).copied().unwrap_or(""),
                };
                let schema = match rng.below(3) {
                    0 => name.to_string(),
                    1 => format!("{name} {{ {} }}", ids[rng.below(ids.len())]),
                    _ => format!("{name}{{{}", ids[rng.below(ids.len())]),
                };
                schemas.push(schema);
            }
            let d = declared(&schemas);
            if let Some(w) = &d.doubt {
                let n = w.to_string().chars().count();
                assert!(n <= longest, "{n} characters");
            }
            // The first schema decides when it is AP242 with a known identifier.
            if let Some(first) = schemas.first()
                && let (name, Some((id, true))) = split(first)
                && let Some(edition) = edition(id).filter(|_| name.eq_ignore_ascii_case(AP242))
            {
                assert_eq!(d, known(Protocol::Ap242(Some(edition))), "{first:?}");
            }
        }
    }
}
