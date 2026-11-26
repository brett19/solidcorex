// Written for solidcorex. Apache-2.0.
//! Refuses to build the differential tests against anything but the pin: the solidcore
//! checkout this crate links (`../../../sc-wt/scx-pin`, its path dependency) must be at commit
//! 03e16c9ee766de5b9b1ced845e5149d62ff493b8 with no change to a tracked or untracked file.
//! Reruns when the checkout's HEAD or index changes or a file under solidcore's crate
//! directory does (cargo scans a directory named by `rerun-if-changed` recursively).

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
        let path = if path.is_absolute() { path } else { pin.join(path) };
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

fn main() -> std::process::ExitCode {
    match check() {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("scx-difftest: the pinned solidcore checkout is not the pin: {e}");
            std::process::ExitCode::FAILURE
        }
    }
}
