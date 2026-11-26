// Written for solidcorex. Apache-2.0.
//! Refuses to build the differential tests against anything but the pin: the solidcore
//! checkout this crate links (`../../../sc-wt/scx-pin`, its path dependency) must be at commit
//! 03e16c9ee766de5b9b1ced845e5149d62ff493b8 with no change to a tracked or untracked file.
//! Reruns when the checkout's HEAD or index changes or a file under solidcore's crate
//! directory does (cargo scans a directory named by `rerun-if-changed` recursively).
//! Also writes the pin's `geom/krawczyk.rs` items, made public, for the difftest
//! ([`pin_krawczyk`]).

use std::path::{Path, PathBuf};
use std::process::Command;

const PIN: &str = "03e16c9ee766de5b9b1ced845e5149d62ff493b8";

fn git(dir: &Path, args: &[&str]) -> Result<String, String> {
    let out = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(args)
        .output()
        .map_err(|e| format!("git {}: {e}", args.join(" ")))?;
    if !out.status.success() {
        return Err(format!(
            "git {} in {}: {}",
            args.join(" "),
            dir.display(),
            String::from_utf8_lossy(&out.stderr).trim()
        ));
    }
    String::from_utf8(out.stdout).map_err(|e| e.to_string())
}

fn check() -> Result<(), String> {
    let manifest = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").map_err(|e| e.to_string())?);
    let pin = manifest.join("../../../sc-wt/scx-pin");
    // Rerun when the checkout moves or its index changes.
    for what in ["HEAD", "index"] {
        let path = git(&pin, &["rev-parse", "--git-path", what])?;
        let path = PathBuf::from(path.trim());
        let path = if path.is_absolute() {
            path
        } else {
            pin.join(path)
        };
        println!("cargo:rerun-if-changed={}", path.display());
    }
    println!(
        "cargo:rerun-if-changed={}",
        pin.join("solidcore/crates/solidcore").display()
    );
    let head = git(&pin, &["rev-parse", "HEAD"])?;
    if head.trim() != PIN {
        return Err(format!(
            "{} is at {}, not the pin {PIN}",
            pin.display(),
            head.trim()
        ));
    }
    let dirty = git(&pin, &["status", "--porcelain"])?;
    if !dirty.trim().is_empty() {
        return Err(format!(
            "{} has changes; the pin must be clean:\n{dirty}",
            pin.display()
        ));
    }
    Ok(())
}

/// The pin's `geom/krawczyk.rs` with its items made public, for the difftest to call (they
/// are `pub(crate)` at the pin): written to `OUT_DIR/pin_krawczyk.rs` without its module
/// docs, its `use` lines (the facade supplies the same names from the pin) and its tests.
/// Every other line is the pin's, unchanged.
fn pin_krawczyk() -> Result<(), String> {
    let manifest = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").map_err(|e| e.to_string())?);
    let src = manifest.join("../../../sc-wt/scx-pin/solidcore/crates/solidcore/src/geom/krawczyk.rs");
    let text = std::fs::read_to_string(&src).map_err(|e| format!("{}: {e}", src.display()))?;
    let body = text.split("#[cfg(test)]").next().unwrap_or("");
    let out: Vec<String> = body
        .lines()
        .filter(|l| !l.starts_with("//!") && !l.starts_with("use "))
        .map(|l| l.replace("pub(crate) ", "pub "))
        .collect();
    let dir = PathBuf::from(std::env::var("OUT_DIR").map_err(|e| e.to_string())?);
    std::fs::write(dir.join("pin_krawczyk.rs"), out.join("\n")).map_err(|e| e.to_string())
}

fn main() -> std::process::ExitCode {
    match check().and_then(|()| pin_krawczyk()) {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("scx-difftest build: {e}");
            std::process::ExitCode::FAILURE
        }
    }
}
