// Written for solidcorex. Apache-2.0.
//! The ssi-corpus scorer (research 73 §16, row SM10): runs `scx_ssi::intersect` on the
//! corpus's cases and scores each against its stored truth.
//!
//! The corpus (jyviko/ssi-corpus, MIT OR Apache-2.0) is not in this repository. Point
//! `SCX_SSI_CORPUS` at a checkout's folder (the one holding `cases/`, each case a `.json` or
//! `.json.gz`; `gzip` must be on the path), then:
//!
//! ```text
//! SCX_SSI_CORPUS=<dir> cargo test -p scx-ssi --test corpus -- --ignored --nocapture score_corpus
//! ```
//!
//! prints one table (deterministic: no timings), the totals, and the timings after it.
//! `SCX_SSI_TIMES=1` adds a time column. `SCX_SSI_CASES=a,b` limits the run to those ids.
//! The tests that run by default use no corpus file: they check the scorer on inline cases
//! and, when `SCX_SSI_CORPUS` is set, a small subset of the corpus.

#![allow(
    dead_code,
    missing_docs,
    clippy::pedantic,
    clippy::indexing_slicing,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::float_cmp,
    clippy::collapsible_if,
    clippy::or_fun_call,
    clippy::redundant_guards,
    // `Instant`: the time column (the clippy.toml reason reads "tests only").
    clippy::disallowed_types
)]

mod case;
mod json;
mod score;
mod table;

use case::{Case, YANG_DATA};
use score::{Row, Verdict, judge, run_case};

/// The cases the default (smoke) run takes from a corpus: closed-form pairs and one refusal.
const SMOKE: [&str; 4] = [
    "c1_plane_sphere_circle",
    "c1_cone_plane_ellipse",
    "c1n_cylN_plane_ellipse",
    "c4_steinmetz",
];

fn scored_files(dir: &std::path::Path) -> Vec<(String, std::path::PathBuf)> {
    let only: Option<Vec<String>> = std::env::var("SCX_SSI_CASES")
        .ok()
        .map(|s| s.split(',').map(str::to_string).collect());
    case::case_files(dir)
        .unwrap()
        .into_iter()
        .filter(|(id, _)| !YANG_DATA.contains(&id.as_str()))
        .filter(|(id, _)| only.as_ref().is_none_or(|o| o.contains(id)))
        .collect()
}

fn score_files(files: &[(String, std::path::PathBuf)]) -> Vec<Row> {
    files
        .iter()
        .map(|(id, path)| match case::load(path) {
            Ok(c) => run_case(&c),
            Err(e) => {
                eprintln!("{id}: {e}");
                Row {
                    id: id.clone(),
                    verdict: Verdict::Setup,
                    truth: [0; 3],
                    out: None,
                    h_out: None,
                    h_tru: None,
                    certs: (0, 0),
                    surf: None,
                    ms: 0.0,
                }
            }
        })
        .collect()
}

#[test]
#[ignore = "the full corpus run; needs SCX_SSI_CORPUS (see the module docs)"]
fn score_corpus() {
    let Some(dir) = case::corpus_dir() else {
        panic!("set SCX_SSI_CORPUS to the ssi-corpus folder");
    };
    let rows = score_files(&scored_files(&dir));
    let times = std::env::var_os("SCX_SSI_TIMES").is_some();
    let text = table::render(&rows, times);
    println!("{text}");
    println!("{}", table::timings(&rows));
    if let Some(out) = std::env::var_os("SCX_SSI_OUT") {
        std::fs::write(out, text).unwrap();
    }
    // Every case must be set up: a surface the loader cannot build is a scorer bug.
    assert!(
        rows.iter().all(|r| r.verdict != Verdict::Setup),
        "a case could not be set up"
    );
}

#[test]
fn smoke_corpus_subset() {
    let Some(dir) = case::corpus_dir() else {
        eprintln!("SCX_SSI_CORPUS is not set: the corpus smoke test is skipped");
        return;
    };
    let files: Vec<_> = case::case_files(&dir)
        .unwrap()
        .into_iter()
        .filter(|(id, _)| SMOKE.contains(&id.as_str()))
        .collect();
    assert_eq!(
        files.len(),
        SMOKE.len(),
        "the smoke cases are in the corpus"
    );
    for r in score_files(&files) {
        // The built surfaces hold the truth's points (the loader's conventions are right).
        let fit = r.surf.unwrap();
        assert!(fit < 1e-6, "{}: truth is {fit:e} from a surface", r.id);
        match r.verdict {
            Verdict::Pass | Verdict::Refused(_) => {}
            ref v => panic!("{}: {}", r.id, v.label()),
        }
    }
}

#[test]
fn excluded_cases_are_the_six_yang_data_cases() {
    assert_eq!(YANG_DATA.len(), 6);
    let Some(dir) = case::corpus_dir() else {
        return;
    };
    let files = case::case_files(&dir).unwrap();
    for id in YANG_DATA {
        assert!(files.iter().any(|(f, _)| f == id), "{id} is in the corpus");
    }
    assert_eq!(scored_files(&dir).len(), files.len() - 6);
}

// ---------------------------------------------------------------- the scorer on inline cases

/// Two planes, z = 0 (u, v = x, y over [-2, 2]) and x = 0 (u, v = y, z): they meet along the
/// y axis from -1 to 1 inside both extents' overlap in y.
const TWO_PLANES: &str = r#"{
 "id": "inline_planes", "cov_tol": 0.001,
 "surfaces": [
  {"type":"plane","origin":[0,0,0],"xaxis":[1,0,0],"yaxis":[0,1,0],"u":[-2,2],"v":[-1,1]},
  {"type":"plane","origin":[0,0,0],"xaxis":[0,1,0],"yaxis":[0,0,1],"u":[-1,1],"v":[-2,2]}],
 "truth": [{"kind":"plane"},{"kind":"plane"}],
 "expected": {"branches":[{"closed":false,"kind":"crossing","points":[[0,-1,0],[0,-0.5,0],[0,0,0],[0,0.5,0],[0,1,0]]}],
  "isolated_points":[],"singular_points":[],"coincident_regions":[]}
}"#;

fn planes() -> Case {
    Case::from_text(TWO_PLANES).unwrap()
}

fn line(a: [f64; 3], b: [f64; 3], n: usize) -> Vec<[f64; 3]> {
    (0..=n)
        .map(|i| {
            let t = i as f64 / n as f64;
            [0, 1, 2].map(|k| a[k] + t * (b[k] - a[k]))
        })
        .collect()
}

fn out_of(curves: Vec<Vec<[f64; 3]>>, points: Vec<[f64; 3]>) -> score::Output {
    score::Output {
        counts: [curves.len(), 0, points.len()],
        curves,
        points,
        ..score::Output::default()
    }
}

#[test]
fn closed_forms_answer_two_planes_and_pass() {
    let row = run_case(&planes());
    assert_eq!(row.verdict, Verdict::Pass, "{row:?}");
    assert!(row.surf.unwrap() < 1e-9);
}

impl std::fmt::Debug for Row {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{} {} {:?} {:?}",
            self.id,
            self.verdict.label(),
            self.h_out,
            self.h_tru
        )
    }
}

#[test]
fn verdicts_follow_the_truth() {
    let c = planes();
    let exact = line([0.0, -1.0, 0.0], [0.0, 1.0, 0.0], 40);
    let v = |o| judge(&c, &o).verdict;
    assert_eq!(v(out_of(vec![exact.clone()], vec![])), Verdict::Pass);
    // Two arcs meeting at an interior point are one curve.
    let halves = vec![
        line([0.0, -1.0, 0.0], [0.0, 0.2, 0.0], 20),
        line([0.0, 0.2, 0.0], [0.0, 1.0, 0.0], 20),
    ];
    assert_eq!(v(out_of(halves, vec![])), Verdict::Pass);
    // Nothing for a curve owed.
    assert_eq!(v(out_of(vec![], vec![])), Verdict::Incomplete);
    // Short of the end.
    let short = line([0.0, -1.0, 0.0], [0.0, 0.5, 0.0], 20);
    assert_eq!(v(out_of(vec![short], vec![])), Verdict::Incomplete);
    // Off the true curve: the truth is uncovered (the more severe verdict).
    let off = line([0.0, -1.0, 0.1], [0.0, 1.0, 0.1], 20);
    assert_eq!(v(out_of(vec![off], vec![])), Verdict::Incomplete);
    // A gap in the middle: two components, one owed.
    let gap = vec![
        line([0.0, -1.0, 0.0], [0.0, -0.2, 0.0], 20),
        line([0.0, 0.2, 0.0], [0.0, 1.0, 0.0], 20),
    ];
    assert_eq!(v(out_of(gap, vec![])), Verdict::Incomplete);
    // An extra curve far from the truth.
    let extra = vec![exact.clone(), line([5.0, 5.0, 0.0], [6.0, 5.0, 0.0], 5)];
    assert_eq!(v(out_of(extra, vec![])), Verdict::WrongTopology);
    // Off by more than the accuracy but within the coverage: INACCURATE.
    let near = line([0.0, -1.0, 2e-4], [0.0, 1.0, 2e-4], 20);
    assert_eq!(v(out_of(vec![near], vec![])), Verdict::Inaccurate);
    // Nothing owed and nothing returned.
    let mut none = planes();
    none.branches.clear();
    assert_eq!(judge(&none, &out_of(vec![], vec![])).verdict, Verdict::Pass);
    // A coincident answer where the truth has no region: its pieces are held to the truth,
    // so one along the curve (a piece of no width) is right and one elsewhere is wrong.
    let mut o = out_of(vec![], vec![]);
    o.coincident = true;
    o.regions = vec![exact.clone()];
    assert_eq!(v(o), Verdict::Pass);
    let mut o = out_of(vec![exact.clone()], vec![]);
    o.coincident = true;
    o.regions = vec![line([3.0, 0.0, 0.0], [3.0, 1.0, 0.0], 4)];
    assert_eq!(v(o), Verdict::WrongTopology);
}

#[test]
fn a_certificate_that_claims_too_much_is_bad() {
    let c = planes();
    let exact = line([0.0, -1.0, 0.0], [0.0, 1.0, 0.0], 40);
    let shifted = line([0.0, -1.0, 2e-6], [0.0, 1.0, 2e-6], 40);
    let cert = |samples: Vec<[f64; 3]>, claim: f64| score::BranchCert {
        samples,
        to_intersection: claim,
        residual: [(0.0, None), (0.0, None)],
    };
    let with = |curve: Vec<[f64; 3]>, bc| {
        let mut o = out_of(vec![curve], vec![]);
        o.certs.push(bc);
        judge(&c, &o)
    };
    // The curve is 2e-6 off and says 1e-5: sound.
    let r = with(shifted.clone(), cert(shifted.clone(), 1e-5));
    assert_eq!((r.verdict, r.certs), (Verdict::Pass, (1, 1)));
    // It says 1e-9: a lie.
    let r = with(shifted.clone(), cert(shifted.clone(), 1e-9));
    assert_eq!((r.verdict, r.certs), (Verdict::BadCert, (0, 1)));
    // A pcurve residual above its claim is a lie too.
    let mut bad = cert(exact.clone(), 1e-5);
    bad.residual[1] = (1e-6, Some(1e-8));
    assert_eq!(with(exact, bad).verdict, Verdict::BadCert);
}

#[test]
fn a_refusal_is_scored_by_kind_and_is_no_failure() {
    // A general pair: the NURBS sphere and a plane through it.
    let text = r#"{
     "id": "inline_nurbs_sphere", "cov_tol": 0.001,
     "surfaces": [
      {"type":"plane","origin":[0,0,0],"xaxis":[1,0,0],"yaxis":[0,1,0],"u":[-2,2],"v":[-2,2]},
      {"type":"nurbs","degree":[1,1],"knots_u":[0,0,1,1],"knots_v":[0,0,1,1],"nu":2,"nv":2,
       "points":[[-1,-1,-1],[-1,1,1],[1,-1,1],[1,1,-1]],"weights":[1,1,1,1]}],
     "truth": [{"kind":"plane"},{"kind":"param"}],
     "expected": {"branches":[{"closed":false,"kind":"crossing","points":[[-1,-1,0],[1,1,0]]}],
      "isolated_points":[],"singular_points":[],"coincident_regions":[]}
    }"#;
    let c = Case::from_text(text).unwrap();
    let row = run_case(&c);
    assert!(matches!(row.verdict, Verdict::Refused(_)), "{row:?}");
    let t = table::render(&[row], false);
    assert!(t.contains("REFUSED ssi_"), "{t}");
}

#[test]
fn the_table_is_deterministic_and_totals_exclude_refusals_from_failures() {
    let rows = vec![
        run_case(&planes()),
        Row {
            id: "x_refused".into(),
            verdict: Verdict::Refused("ssi_unresolved".into()),
            truth: [1, 0, 0],
            out: None,
            h_out: None,
            h_tru: None,
            certs: (0, 0),
            surf: None,
            ms: 3.0,
        },
    ];
    let a = table::render(&rows, false);
    assert_eq!(a, table::render(&rows, false));
    assert!(a.contains("pass 1 of 2"), "{a}");
    assert!(a.contains("refused 1 (ssi_unresolved 1)"), "{a}");
    assert!(a.contains("failed 0"), "{a}");
}
