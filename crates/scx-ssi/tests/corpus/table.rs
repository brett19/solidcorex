// Written for solidcorex. Apache-2.0.
//! The score table: one row a case, the totals, and the timings apart from the table (a
//! time is not deterministic).

use std::collections::BTreeMap;
use std::fmt::Write as _;

use crate::score::{Row, Verdict};

/// The pin `ssi` crate's score and the SMLib port's, over all 119 cases (research 73 §2).
const PIN_CRATE: usize = 118;
const SMLIB_PORT: usize = 78;

fn sci(x: Option<f64>) -> String {
    match x {
        None => "-".into(),
        Some(v) if v == 0.0 => "0".into(),
        Some(v) if v.is_finite() => format!("{v:.1e}"),
        Some(_) => "inf".into(),
    }
}

fn triple(t: [usize; 3]) -> String {
    format!("{}/{}/{}", t[0], t[1], t[2])
}

pub fn render(rows: &[Row], times: bool) -> String {
    let mut s = String::new();
    let w = rows.iter().map(|r| r.id.len()).max().unwrap_or(4).max(4);
    let _ = write!(
        s,
        "{:w$}  {:<28}  {:>9}  {:>9}  {:>8}  {:>8}  {:>5}  {:>8}",
        "case", "verdict", "truth b/n/t", "out b/n/t", "h_out", "h_truth", "certs", "surf",
    );
    if times {
        let _ = write!(s, "  {:>9}", "ms");
    }
    s.push('\n');
    for r in rows {
        let certs = if r.certs.1 == 0 {
            "-".to_string()
        } else {
            format!("{}/{}", r.certs.0, r.certs.1)
        };
        let _ = write!(
            s,
            "{:w$}  {:<28}  {:>9}  {:>9}  {:>8}  {:>8}  {:>5}  {:>8}",
            r.id,
            r.verdict.label(),
            triple(r.truth),
            r.out.map_or("-".into(), triple),
            sci(r.h_out),
            sci(r.h_tru),
            certs,
            sci(r.surf),
        );
        if times {
            let _ = write!(s, "  {:>9.3}", r.ms);
        }
        s.push('\n');
    }
    let total = rows.len();
    let mut by: BTreeMap<String, usize> = BTreeMap::new();
    let mut refused: BTreeMap<String, usize> = BTreeMap::new();
    for r in rows {
        match &r.verdict {
            Verdict::Refused(n) => {
                *refused.entry(n.clone()).or_default() += 1;
            }
            v => {
                *by.entry(v.label()).or_default() += 1;
            }
        }
    }
    let count = |l: &str| by.get(l).copied().unwrap_or(0);
    let nrefused: usize = refused.values().sum();
    let failed = total - count("PASS") - nrefused;
    let kinds = refused
        .iter()
        .map(|(k, n)| format!("{k} {n}"))
        .collect::<Vec<_>>()
        .join(", ");
    let _ = writeln!(
        s,
        "\ntotals over {total} cases (the six Yang-data cases are not run)"
    );
    let _ = writeln!(
        s,
        "pass {} of {total}; refused {nrefused} ({kinds}); failed {failed} \
         (incomplete {}, wrong topology {}, bad cert {}, inaccurate {}, setup {})",
        count("PASS"),
        count("INCOMPLETE"),
        count("WRONG TOPOLOGY"),
        count("BAD CERT"),
        count("INACCURATE"),
        count("SETUP"),
    );
    let _ = writeln!(
        s,
        "for comparison, over all 119: the pin crate passes {PIN_CRATE}, the SMLib port {SMLIB_PORT}"
    );
    s
}

/// The timings, apart from the table.
pub fn timings(rows: &[Row]) -> String {
    let total: f64 = rows.iter().map(|r| r.ms).sum();
    let worst = rows
        .iter()
        .max_by(|a, b| a.ms.total_cmp(&b.ms))
        .map_or("-".to_string(), |r| format!("{} {:.3} ms", r.id, r.ms));
    format!(
        "timings (not deterministic): total {total:.3} ms over {} cases; slowest {worst}",
        rows.len()
    )
}
