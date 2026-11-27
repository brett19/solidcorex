// Written for solidcorex. Apache-2.0.
//! Writes `corpus/references.tsv` to standard output: `cargo run -p scx-testkit --example
//! corpus_references -- corpus/manifest.tsv "$SCX_CORPUS_CACHE" > corpus/references.tsv`.
//! The cache must hold the truth files (`cargo xtask corpus fetch dan335`). Nothing read
//! here is executed.

use std::path::PathBuf;
use std::process::ExitCode;

use scx_testkit::corpus::{CorpusError, parse_manifest, references_tsv};

fn run() -> Result<String, String> {
    let mut args = std::env::args().skip(1);
    let (Some(manifest), Some(cache)) = (args.next(), args.next()) else {
        return Err("usage: corpus_references MANIFEST CACHE_DIR".to_string());
    };
    let text = std::fs::read_to_string(&manifest).map_err(|e| format!("{manifest}: {e}"))?;
    let rows = parse_manifest(&text).map_err(|e| e.to_string())?;
    let cache = PathBuf::from(cache);
    references_tsv(&rows, |r| {
        let p = cache.join(&r.name);
        std::fs::read_to_string(&p).map_err(|e| CorpusError::Truth(format!("{}: {e}", p.display())))
    })
    .map_err(|e| e.to_string())
}

fn main() -> ExitCode {
    match run() {
        Ok(t) => {
            print!("{t}");
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("corpus_references: {e}");
            ExitCode::FAILURE
        }
    }
}
