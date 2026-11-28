// Written for solidcorex. Apache-2.0.
//! RP0: what the pin's SMLib healer (`heal_made`, as `solidcore-convert`'s `to_kernel` runs it
//! on an untrusted record) changes over the corpus's records (75 section 7, Q7).
//!
//! Every record is built twice: as a trusted record (no healer) and as an untrusted one (the
//! healer runs). Both bodies go through the pin's full check (`check_brep`, level 2: pointers,
//! then `AssertValid`). A record is *altered* when the healed body differs from the unhealed
//! one (topology counts, vertex and edge tolerances, or the record the body gives back).
//! Failed checks are classed by the failing report's name. Records are the rig corpus's
//! (`solidcore/tests/kernel_cases`, every body, trusted or not) and the pin's STEP fixtures;
//! `SCX_HEAL_STEP_DIR` adds every STEP file under a directory (the NIST files).
//!
//! Run: `cargo run --release -p scx-difftest --example heal_count`. Deterministic.

use std::collections::{BTreeMap, BTreeSet};
use std::io::Read as _;
use std::path::{Path, PathBuf};

use solidcore::Brep;
use solidcore::kernel::{Kernel, KernelOptions, UpstreamFixes};
use solidcore::topology::brep::validation::{CheckLevel, PointerCheck, check_brep};
use solidcore::topology::{edge, vertex};
use solidcore_convert::brep::{to_kernel_body, to_record};
use solidcore_record::brep::Brep as Record;

fn pin() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../sc-wt/scx-pin/solidcore")
}

fn kernel() -> Kernel {
    let mut k = Kernel::new(&KernelOptions::default());
    k.fixes = UpstreamFixes::all();
    k
}

/// The names of the failed reports (pointer and AssertValid), sorted and distinct; `None`
/// when the check itself errored.
fn failures(k: &mut Kernel, b: &mut Brep) -> Option<BTreeSet<String>> {
    let c = check_brep(k, b, CheckLevel::Level2).ok()?;
    let mut s: BTreeSet<String> = c.failures().map(|r| r.name.to_owned()).collect();
    if c.pointers != PointerCheck::Passed && s.is_empty() {
        s.insert("pointers".to_owned());
    }
    if c.assert_valid == Some(false) && s.is_empty() {
        s.insert("assert_valid".to_owned());
    }
    Some(s)
}

#[derive(PartialEq)]
struct Fp {
    counts: [usize; 3],
    vtol: Vec<u64>,
    etol: Vec<u64>,
    record: Option<String>,
}

fn fp(k: &mut Kernel, b: &mut Brep) -> Fp {
    let vtol = b
        .vertex_list()
        .to_vec()
        .into_iter()
        .map(|v| vertex::tolerance(b, v).map_or(0, f64::to_bits))
        .collect();
    let etol = b
        .edge_list()
        .to_vec()
        .into_iter()
        .map(|e| edge::tolerance(b, e).map_or(0, f64::to_bits))
        .collect();
    let counts = [
        b.face_list().len(),
        b.edge_list().len(),
        b.vertex_list().len(),
    ];
    let record = to_record(k, b)
        .ok()
        .and_then(|(r, _)| serde_json::to_string(&r).ok());
    Fp {
        counts,
        vtol,
        etol,
        record,
    }
}

#[derive(Default)]
struct Tally {
    records: usize,
    refused_unhealed: usize,
    refused_healed: Vec<String>,
    /// Failing before: class -> (records, of which the healed body passes, still fails).
    before: BTreeMap<String, (usize, usize, usize)>,
    failing_before: usize,
    fixed: usize,
    altered: usize,
    /// How the altered bodies differ: topology, tolerance, geometry.
    how: BTreeMap<&'static str, usize>,
    altered_valid_before: usize,
    broke: Vec<String>,
    check_error: usize,
    /// After healing, failing: class -> records.
    after_fail: BTreeMap<String, usize>,
    examples: BTreeMap<String, Vec<String>>,
}

fn one(name: &str, r: &Record, t: &mut Tally) {
    t.records += 1;
    let mut k = kernel();
    let Ok(mut a) = to_kernel_body(&mut k, r, true) else {
        t.refused_unhealed += 1;
        return;
    };
    let mut kb = kernel();
    let mut b = match to_kernel_body(&mut kb, r, false) {
        Ok(b) => b,
        Err(e) => {
            t.refused_healed.push(format!("{name}: {e}"));
            return;
        }
    };
    let (Some(fa), Some(fb)) = (failures(&mut k, &mut a), failures(&mut kb, &mut b)) else {
        t.check_error += 1;
        return;
    };
    let (pa, pb) = (fp(&mut k, &mut a), fp(&mut kb, &mut b));
    let altered = pa != pb;
    if altered {
        t.altered += 1;
        let how = if pa.counts != pb.counts {
            "topology (entity counts)"
        } else if pa.vtol != pb.vtol || pa.etol != pb.etol {
            "tolerances only"
        } else {
            "geometry or trims (record differs)"
        };
        *t.how.entry(how).or_default() += 1;
        t.examples
            .entry(how.to_owned())
            .or_default()
            .push(name.to_owned());
        if fa.is_empty() {
            t.altered_valid_before += 1;
        }
    }
    if !fa.is_empty() {
        t.failing_before += 1;
        let now_pass = fb.is_empty();
        if now_pass {
            t.fixed += 1;
        }
        for c in &fa {
            let e = t.before.entry(c.clone()).or_default();
            e.0 += 1;
            if now_pass {
                e.1 += 1;
            } else {
                e.2 += 1;
            }
        }
    }
    if fa.is_empty() && !fb.is_empty() {
        t.broke.push(format!("{name}: {fb:?}"));
    }
    for c in &fb {
        *t.after_fail.entry(c.clone()).or_default() += 1;
    }
}

fn corpus_records() -> Vec<(String, Record)> {
    let dir = pin().join("crates/solidcore/tests/kernel_cases");
    let mut files = Vec::new();
    walk(&dir, &mut files, &|p| {
        p.extension().is_some_and(|x| x == "json") && !p.to_string_lossy().ends_with(".bodies.json")
    });
    files.sort();
    let mut out = Vec::new();
    for f in files {
        if f.parent() == Some(dir.as_path()) {
            continue;
        }
        let Ok(text) = std::fs::read_to_string(&f) else {
            continue;
        };
        let Ok(e) = serde_json::from_str::<serde_json::Value>(&text) else {
            continue;
        };
        let Some(bodies) = e["bodies"].as_object() else {
            continue;
        };
        if bodies.is_empty() {
            continue;
        }
        let mut gz = String::new();
        let Ok(file) = std::fs::File::open(f.with_extension("bodies.json.gz")) else {
            continue;
        };
        if flate2::read::GzDecoder::new(file)
            .read_to_string(&mut gz)
            .is_err()
        {
            continue;
        }
        let Ok(records) = serde_json::from_str::<serde_json::Value>(&gz) else {
            continue;
        };
        for id in bodies.keys() {
            if let Ok(r) = serde_json::from_value::<Record>(records[id].clone()) {
                let rel = f.strip_prefix(&dir).unwrap_or(&f).display().to_string();
                out.push((format!("{rel}:{id}"), r));
            }
        }
    }
    out
}

fn walk(dir: &Path, out: &mut Vec<PathBuf>, keep: &dyn Fn(&Path) -> bool) {
    let Ok(rd) = std::fs::read_dir(dir) else {
        return;
    };
    for e in rd.flatten() {
        let p = e.path();
        if p.is_dir() {
            walk(&p, out, keep);
        } else if keep(&p) {
            out.push(p);
        }
    }
}

fn step_records(dir: &Path, label: &str) -> Vec<(String, Record)> {
    let mut files = Vec::new();
    walk(dir, &mut files, &|p| {
        matches!(
            p.extension().and_then(|e| e.to_str()),
            Some("stp" | "step" | "STP" | "STEP")
        )
    });
    files.sort();
    let mut out = Vec::new();
    for f in files {
        let Ok(bytes) = std::fs::read(&f) else {
            continue;
        };
        let text = String::from_utf8_lossy(&bytes);
        let Ok(ex) = solidcore_record::step::p21::parse(&text) else {
            continue;
        };
        let solids = solidcore_step::topology::read_solids(&ex);
        for s in solids.solids {
            let rel = f.strip_prefix(dir).unwrap_or(&f).display().to_string();
            out.push((format!("{label}:{rel}#{}", s.item), s.brep));
        }
    }
    out
}

fn report(title: &str, recs: &[(String, Record)]) {
    let mut t = Tally::default();
    let mut seen = BTreeSet::new();
    let mut dup = 0usize;
    for (n, r) in recs {
        // distinct records only: the corpus repeats bodies across cases
        let key = serde_json::to_string(r).unwrap_or_default();
        if !seen.insert(key) {
            dup += 1;
            continue;
        }
        one(n, r, &mut t);
    }
    println!(
        "== {title}: {} records ({} repeats skipped) ==",
        t.records, dup
    );
    println!(
        "  refused building without the healer: {}",
        t.refused_unhealed
    );
    println!(
        "  refused by the healed build only: {}",
        t.refused_healed.len()
    );
    for x in t.refused_healed.iter().take(5) {
        println!("    {x}");
    }
    println!("  check errored: {}", t.check_error);
    println!(
        "  failing the check before the healer: {}",
        t.failing_before
    );
    println!("  of those, passing after the healer: {}", t.fixed);
    println!(
        "  altered by the healer: {} (of which valid before: {})",
        t.altered, t.altered_valid_before
    );
    for (h, n) in &t.how {
        println!("    altered, {h}: {n}");
    }
    println!("  passing before, failing after: {}", t.broke.len());
    for x in t.broke.iter().take(5) {
        println!("    {x}");
    }
    println!("  failing class before: records / now pass / still fail");
    for (c, (n, p, f)) in &t.before {
        println!("    {c}: {n} / {p} / {f}");
    }
    println!("  failing class after: records");
    for (c, n) in &t.after_fail {
        println!("    {c}: {n}");
    }
    for (h, v) in &t.examples {
        println!(
            "  e.g. {h}: {}",
            v.iter().take(3).cloned().collect::<Vec<_>>().join(", ")
        );
    }
}

fn main() {
    let corpus = corpus_records();
    report(
        "rig corpus (kernel_cases), every body healed as if untrusted",
        &corpus,
    );
    let mut steps = step_records(
        &pin().join("crates/solidcore-step/tests/fixtures"),
        "fixture",
    );
    steps.extend(step_records(
        &pin().join("crates/step-export/src"),
        "export",
    ));
    report("pin STEP fixtures", &steps);
    if let Some(d) = std::env::var_os("SCX_HEAL_STEP_DIR") {
        report("SCX_HEAL_STEP_DIR", &step_records(Path::new(&d), "dir"));
    }
}
