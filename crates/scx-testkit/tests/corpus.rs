// Written for solidcorex. Apache-2.0.
//! The corpus manifest and the closed-form volumes of the step-conformance truth files.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

use std::collections::HashSet;
use std::f64::consts::PI;

use scx_testkit::corpus::{CorpusError, Part, parse_manifest, references_tsv, truth_volume};

const MANIFEST: &str = include_str!("../../../corpus/manifest.tsv");
const REFERENCES: &str = include_str!("../../../corpus/references.tsv");

fn rel(a: f64, b: f64) -> f64 {
    (a - b).abs() / b.abs()
}

fn solid(profile: &str) -> String {
    format!(r#"{{"solids":[{{"origin":[0,0,0],"axis":[0,0,1],"profile":[{profile}]}}]}}"#)
}

fn seg(a: [f64; 2], b: [f64; 2]) -> String {
    format!(
        r#"{{"segment":{{"from":[{},{}],"to":[{},{}]}}}}"#,
        a[0], a[1], b[0], b[1]
    )
}

#[test]
fn cylinder_is_pi_r_squared_h() {
    // Axis 0..30, radius 10, as step-conformance writes it.
    let p = [
        seg([0.0, 0.0], [0.0, 10.0]),
        seg([0.0, 10.0], [30.0, 10.0]),
        seg([30.0, 10.0], [30.0, 0.0]),
        seg([30.0, 0.0], [0.0, 0.0]),
    ]
    .join(",");
    let v = truth_volume(&solid(&p)).unwrap().unwrap();
    assert!(rel(v, PI * 100.0 * 30.0) < 1e-15, "{v}");
}

#[test]
fn hemisphere_and_half_torus_closed_forms() {
    let arc = r#"{"arc":{"center":[0,0],"radius":10,"start":1.5707963267948966,"sweep":-1.5707963267948966}}"#;
    let p = [
        seg([0.0, 0.0], [0.0, 10.0]),
        arc.to_string(),
        seg([10.0, 0.0], [0.0, 0.0]),
    ]
    .join(",");
    let v = truth_volume(&solid(&p)).unwrap().unwrap();
    assert!(rel(v, 2.0 / 3.0 * PI * 1000.0) < 1e-14, "{v}");

    // Half a torus, R = 20, r = 5: pi^2 R r^2.
    let half = r#"{"arc":{"center":[0,20],"radius":5,"start":1.5707963267948966,"sweep":-3.141592653589793}}"#;
    let p = [seg([0.0, 15.0], [0.0, 25.0]), half.to_string()].join(",");
    let v = truth_volume(&solid(&p)).unwrap().unwrap();
    assert!(rel(v, PI * PI * 20.0 * 25.0) < 1e-13, "{v}");
}

#[test]
fn prism_with_a_hole_and_two_solids_add() {
    let sq = |s: f64, o: f64| {
        [
            seg([o, o], [o + s, o]),
            seg([o + s, o], [o + s, o + s]),
            seg([o + s, o + s], [o, o + s]),
            seg([o, o + s], [o, o]),
        ]
        .join(",")
    };
    // A 10 x 10 plate 2 high with a 4 x 4 hole (the hole listed first, the other way round).
    let j = format!(
        r#"{{"prisms":[{{"origin":[0,0,0],"axis":[0,0,1],"x":[1,0,0],"height":2,"loops":[[{}],[{}]]}}]}}"#,
        sq(4.0, 3.0),
        sq(10.0, 0.0)
    );
    let v = truth_volume(&j).unwrap().unwrap();
    assert!(rel(v, 2.0 * (100.0 - 16.0)) < 1e-15, "{v}");

    let cyl = [
        seg([0.0, 0.0], [0.0, 1.0]),
        seg([0.0, 1.0], [1.0, 1.0]),
        seg([1.0, 1.0], [1.0, 0.0]),
        seg([1.0, 0.0], [0.0, 0.0]),
    ]
    .join(",");
    let two = format!(r#"{{"solids":[{{"profile":[{cyl}]}},{{"profile":[{cyl}]}}]}}"#);
    assert!(rel(truth_volume(&two).unwrap().unwrap(), 2.0 * PI) < 1e-15);
}

#[test]
fn refusals_are_typed() {
    // A profile open by 0.5.
    let open = solid(&[seg([0.0, 0.0], [0.0, 10.0]), seg([0.0, 10.0], [30.0, 10.0])].join(","));
    assert!(matches!(truth_volume(&open), Err(CorpusError::Open { gap }) if gap > 1.0));
    // Not JSON; a piece that is neither; nesting past the limit; text after the value.
    assert!(matches!(truth_volume("{"), Err(CorpusError::Json { .. })));
    assert!(matches!(
        truth_volume("[1] x"),
        Err(CorpusError::Json { .. })
    ));
    let deep = format!("{}{}", "[".repeat(100_000), "]".repeat(100_000));
    assert!(matches!(truth_volume(&deep), Err(CorpusError::Json { .. })));
    assert!(matches!(
        truth_volume(&solid(r#"{"curve":{}}"#)),
        Err(CorpusError::Truth(_))
    ));
    // A file with neither solids nor prisms (the freeform panel's) has no volume.
    assert_eq!(
        truth_volume(r#"{"solids":[],"freeform":{}}"#).unwrap(),
        None
    );
}

#[test]
fn manifest_refusals() {
    let head = "name\turl\tsha256\tsize\tlicence\tpart\n";
    let sha = "a".repeat(64);
    let ok = format!("{head}x/y\thttps://e/x\t{sha}\t3\tMIT\tfile\n");
    assert_eq!(parse_manifest(&ok).unwrap().len(), 1);
    for bad in [
        format!("{head}x/y\tu\t{sha}\t3\tMIT\n"),
        format!("{head}../y\tu\t{sha}\t3\tMIT\tfile\n"),
        format!("{head}/y\tu\t{sha}\t3\tMIT\tfile\n"),
        format!("{head}y\tu\t{}\t3\tMIT\tfile\n", "A".repeat(64)),
        format!("{head}y\tu\t{sha}\tx\tMIT\tfile\n"),
        format!("{head}y\tu\t{sha}\t3\tMIT\tzip\n"),
    ] {
        assert!(
            matches!(parse_manifest(&bad), Err(CorpusError::Manifest { .. })),
            "{bad}"
        );
    }
}

#[test]
fn committed_manifest_is_well_formed() {
    let rows = parse_manifest(MANIFEST).unwrap();
    assert!(rows.len() > 60);
    let names: HashSet<&str> = rows.iter().map(|r| r.name.as_str()).collect();
    assert_eq!(names.len(), rows.len(), "a name twice");
    // Truth files of equal parts may be equal; a step file twice under two names may not be.
    let steps: Vec<&str> = rows
        .iter()
        .filter(|r| !r.name.ends_with(".truth.json"))
        .map(|r| r.sha256.as_str())
        .collect();
    assert_eq!(
        steps.iter().collect::<HashSet<_>>().len(),
        steps.len(),
        "a file twice"
    );
    for r in &rows {
        assert!(
            !r.licence.is_empty() && r.url.starts_with("https://"),
            "{}",
            r.name
        );
        assert!(r.size > 0);
        if let Part::Member { archive } = &r.part {
            assert!(
                rows.iter()
                    .any(|a| &a.name == archive && a.part == Part::Archive),
                "{}",
                r.name
            );
        }
    }
    // The bar (docs/04-decisions.md): permissive licences only.
    for r in &rows {
        let ok = ["CC0-1.0", "MIT", "Apache-2.0", "US Government work"];
        assert!(
            ok.iter().any(|p| r.licence.starts_with(p)),
            "{}: {}",
            r.name,
            r.licence
        );
    }
}

/// Step-conformance's step files and the pin's table (`bench/step/references.tsv`, 3 decimals).
#[test]
fn references_agree_with_the_pins_table() {
    let pin: &[(&str, f64)] = &[
        ("cone_point", 3141.592653589793),
        ("cone_point_vertex", 3141.592653589793),
        ("cylinder", 9424.778),
        ("cylinder_inch", 9424.778),
        ("cylinder_nurbs", 9424.778),
        ("cylinder_placed", 9424.778),
        ("cylinder_seam", 9424.778),
        ("cylinder_twice", 18849.556),
        ("dome", 16755.161),
        ("dome_nurbs", 16755.161),
        ("far_cylinder", 9424.778),
        ("grooved_shaft", 153532.251),
        ("plate_100_holes", 71588.321),
        ("sheet_disc", 193396443.755),
        ("tiny_pin", std::f64::consts::FRAC_PI_8),
        ("torus_ring_nurbs", 4934.802),
        ("washer", 3534.292),
    ];
    for (name, v) in pin {
        let tail = format!("dan335-step-conformance/generated/{name}.step (closed form");
        let line = REFERENCES
            .lines()
            .find(|l| l.contains(&tail))
            .unwrap_or_else(|| panic!("no reference for {name}"));
        let ours: f64 = line.split('\t').nth(1).unwrap().parse().unwrap();
        assert!(rel(ours, *v) < 1e-6, "{name}: {ours} against the pin's {v}");
    }
}

/// With the cache fetched (`SCX_CORPUS_CACHE`), the committed `references.tsv` is what the
/// generator writes from the manifest and the files.
#[test]
fn committed_references_are_current() {
    let Some(dir) = std::env::var_os("SCX_CORPUS_CACHE") else {
        eprintln!("SCX_CORPUS_CACHE not set: references not regenerated");
        return;
    };
    let rows = parse_manifest(MANIFEST).unwrap();
    let got = references_tsv(&rows, |r| {
        let p = std::path::Path::new(&dir).join(&r.name);
        std::fs::read_to_string(&p).map_err(|e| CorpusError::Truth(format!("{}: {e}", p.display())))
    })
    .unwrap();
    assert_eq!(got, REFERENCES);
}
