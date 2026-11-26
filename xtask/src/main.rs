// Written for solidcorex. Apache-2.0.
//! Repository checks, run as `cargo xtask <command>`:
//!
//! - `layers`: every workspace crate is listed in `layers.txt`, and a library depends
//!   (normal, build or dev) only on libraries listed above it. The differential-test
//!   crates may depend on any library; tool crates depend on no workspace crate.
//! - `headers`: every `.rs` file under `crates/` and `xtask/` opens with a derivation
//!   header (`Moved from solidcore ... at commit <pin>`, its `Upstream record` and its
//!   `Changes:`) or says it was written for solidcorex.
//! - `fast`: `cargo fmt --check` of every member, clippy with warnings denied, `layers`, `headers` and
//!   the tier-1 tests (`cargo test -p <crate>` for each crate named, default every
//!   library).
//!
//! No dependencies: the manifests are read line by line, which is enough for the plain
//! manifests this repository writes.

use std::fmt::Write as _;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};

/// The solidcore commit every moved file derives from.
const PIN: &str = "03e16c9ee766de5b9b1ced845e5149d62ff493b8";

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let root = match workspace_root() {
        Ok(r) => r,
        Err(e) => return fail(&e),
    };
    let result = match args.first().map(String::as_str) {
        Some("layers") => layers(&root),
        Some("headers") => headers(&root),
        Some("fast") => fast(&root, args.get(1..).unwrap_or(&[])),
        _ => Err("usage: cargo xtask <layers | headers | fast [crate ...]>".to_string()),
    };
    match result {
        Ok(msg) => {
            println!("{msg}");
            ExitCode::SUCCESS
        }
        Err(e) => fail(&e),
    }
}

fn fail(e: &str) -> ExitCode {
    eprintln!("xtask: {e}");
    ExitCode::FAILURE
}

/// The directory above `xtask/`.
fn workspace_root() -> Result<PathBuf, String> {
    let here = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    here.parent()
        .map(Path::to_path_buf)
        .ok_or_else(|| "no workspace root above xtask".to_string())
}

fn read(path: &Path) -> Result<String, String> {
    fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))
}

// ---- layers ------------------------------------------------------------------------

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Role {
    /// A library at this position, 0 the lowest.
    Library(usize),
    Difftest,
    Tool,
}

fn layer_roles(root: &Path) -> Result<Vec<(String, Role)>, String> {
    let text = read(&root.join("layers.txt"))?;
    let mut section = String::new();
    let mut out: Vec<(String, Role)> = Vec::new();
    let mut libs = 0;
    for line in text.lines() {
        let line = line.split('#').next().unwrap_or("").trim();
        if line.is_empty() {
            continue;
        }
        if let Some(s) = line.strip_prefix('[').and_then(|l| l.strip_suffix(']')) {
            s.clone_into(&mut section);
            continue;
        }
        let role = match section.as_str() {
            "" => {
                libs += 1;
                Role::Library(libs - 1)
            }
            "difftest" => Role::Difftest,
            "tool" => Role::Tool,
            other => return Err(format!("layers.txt: unknown section [{other}]")),
        };
        if out.iter().any(|(n, _)| n == line) {
            return Err(format!("layers.txt lists {line} twice"));
        }
        out.push((line.to_string(), role));
    }
    Ok(out)
}

/// The `members = [...]` of the workspace manifest.
fn workspace_members(root: &Path) -> Result<Vec<String>, String> {
    let text = read(&root.join("Cargo.toml"))?;
    let start = text
        .find("\nmembers")
        .ok_or("Cargo.toml has no workspace members")?;
    let rest = text.get(start..).unwrap_or("");
    let open = rest.find('[').ok_or("members has no list")?;
    let close = rest.find(']').ok_or("members list is not closed")?;
    let list = rest.get(open + 1..close).unwrap_or("");
    Ok(list
        .split(',')
        .map(|s| s.trim().trim_matches('"').to_string())
        .filter(|s| !s.is_empty())
        .collect())
}

/// A member's package name and the crate names it depends on (every dependency table).
fn manifest_deps(manifest: &str) -> (Option<String>, Vec<String>) {
    let mut section = String::new();
    let mut name = None;
    let mut deps = Vec::new();
    for raw in manifest.lines() {
        let line = raw.split('#').next().unwrap_or("").trim();
        if line.is_empty() {
            continue;
        }
        if line.starts_with('[') {
            section = line.trim_matches(|c| c == '[' || c == ']').to_string();
            // `[dependencies.foo]`
            for table in ["dependencies.", "dev-dependencies.", "build-dependencies."] {
                if let Some(pos) = section.find(table) {
                    if let Some(dep) = section.get(pos + table.len()..) {
                        deps.push(dep.trim_matches('"').to_string());
                    }
                }
            }
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        let key = key.trim();
        if section == "package" && key == "name" {
            name = Some(value.trim().trim_matches('"').to_string());
        }
        if section.ends_with("dependencies") {
            let dep = key.split('.').next().unwrap_or(key).trim_matches('"');
            let renamed = value
                .split_once("package")
                .and_then(|(_, p)| p.split('"').nth(1))
                .map(str::to_string);
            deps.push(renamed.unwrap_or_else(|| dep.to_string()));
        }
    }
    (name, deps)
}

fn layers(root: &Path) -> Result<String, String> {
    let roles = layer_roles(root)?;
    let role_of = |n: &str| roles.iter().find(|(m, _)| m == n).map(|(_, r)| *r);
    let mut crates = Vec::new();
    for member in workspace_members(root)? {
        let text = read(&root.join(&member).join("Cargo.toml"))?;
        let (name, deps) = manifest_deps(&text);
        let name = name.ok_or_else(|| format!("{member}/Cargo.toml has no package name"))?;
        crates.push((name, deps));
    }
    let names: Vec<&str> = crates.iter().map(|(n, _)| n.as_str()).collect();
    let mut errors = String::new();
    for (name, deps) in &crates {
        let Some(role) = role_of(name) else {
            let _ = writeln!(errors, "{name} is not listed in layers.txt");
            continue;
        };
        for dep in deps.iter().filter(|d| names.contains(&d.as_str())) {
            let ok = match (role, role_of(dep)) {
                (Role::Library(me), Some(Role::Library(it))) => it < me,
                (Role::Difftest, Some(Role::Library(_))) => true,
                _ => false,
            };
            if !ok {
                let _ = writeln!(errors, "{name} may not depend on {dep}");
            }
        }
    }
    for (listed, _) in &roles {
        if !names.contains(&listed.as_str()) {
            let _ = writeln!(errors, "layers.txt lists {listed}, not a workspace crate");
        }
    }
    if errors.is_empty() {
        Ok(format!("layers: {} crates in order", crates.len()))
    } else {
        Err(format!("layers:\n{errors}"))
    }
}

// ---- headers -----------------------------------------------------------------------

fn rust_files(dir: &Path, out: &mut Vec<PathBuf>) -> Result<(), String> {
    let entries = fs::read_dir(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    for entry in entries {
        let path = entry.map_err(|e| e.to_string())?.path();
        let skip = path
            .file_name()
            .is_some_and(|n| n == "target" || n == "bin" || n.to_string_lossy().starts_with('.'));
        if skip {
            continue;
        }
        if path.is_dir() {
            rust_files(&path, out)?;
        } else if path.extension().is_some_and(|e| e == "rs") {
            out.push(path);
        }
    }
    Ok(())
}

/// The plain `//` comment lines a file opens with (not `//!` or `///`).
fn opening_comment(text: &str) -> String {
    text.lines()
        .take_while(|l| l.starts_with("//") && !l.starts_with("//!") && !l.starts_with("///"))
        .collect::<Vec<_>>()
        .join("\n")
}

fn header_ok(text: &str) -> bool {
    let head = opening_comment(text);
    let written = head.to_lowercase().contains("written for solidcorex");
    let moved = head.contains("Moved from solidcore")
        && head.contains(PIN)
        && head.contains("Upstream record")
        && head.contains("Changes:");
    written || moved
}

fn headers(root: &Path) -> Result<String, String> {
    let mut files = Vec::new();
    for dir in ["crates", "xtask"] {
        let d = root.join(dir);
        if d.is_dir() {
            rust_files(&d, &mut files)?;
        }
    }
    let mut bad = String::new();
    for f in &files {
        if !header_ok(&read(f)?) {
            let shown = f.strip_prefix(root).unwrap_or(f);
            let _ = writeln!(
                bad,
                "{} has no derivation or solidcorex header",
                shown.display()
            );
        }
    }
    if bad.is_empty() {
        Ok(format!("headers: {} files", files.len()))
    } else {
        Err(format!("headers:\n{bad}"))
    }
}

// ---- fast --------------------------------------------------------------------------

fn cargo(root: &Path, args: &[&str]) -> Result<(), String> {
    let cargo = std::env::var("CARGO").unwrap_or_else(|_| "cargo".to_string());
    println!("xtask: cargo {}", args.join(" "));
    let status = Command::new(cargo)
        .args(args)
        .current_dir(root)
        .status()
        .map_err(|e| e.to_string())?;
    if status.success() {
        Ok(())
    } else {
        Err(format!("cargo {} failed", args.join(" ")))
    }
}

fn fast(root: &Path, crates: &[String]) -> Result<String, String> {
    // `--all` would take in solidcore too, a path dependency of the difftest crate.
    let mut fmt = vec!["fmt".to_string(), "--check".to_string()];
    for member in workspace_members(root)? {
        let (name, _) = manifest_deps(&read(&root.join(&member).join("Cargo.toml"))?);
        if let Some(n) = name {
            fmt.push("-p".to_string());
            fmt.push(n);
        }
    }
    cargo(root, &fmt.iter().map(String::as_str).collect::<Vec<_>>())?;
    cargo(root, &["clippy", "--all-targets", "--", "-D", "warnings"])?;
    println!("{}", layers(root)?);
    println!("{}", headers(root)?);
    let libs: Vec<String> = if crates.is_empty() {
        layer_roles(root)?
            .into_iter()
            .filter(|(_, r)| matches!(r, Role::Library(_)))
            .map(|(n, _)| n)
            .collect()
    } else {
        crates.to_vec()
    };
    for lib in &libs {
        cargo(root, &["test", "-p", lib])?;
    }
    Ok(format!("fast: passed ({})", libs.join(", ")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deps_of_every_table_are_read() {
        let m = "[package]\nname = \"a\"\n[dependencies]\nb = { path = \"../b\" }\n\
                 [dev-dependencies]\nc.workspace = true\nx = { package = \"d\", path = \"p\" }\n\
                 [dependencies.e]\npath = \"../e\"\n";
        let (name, deps) = manifest_deps(m);
        assert_eq!(name.as_deref(), Some("a"));
        assert_eq!(deps, ["b", "c", "d", "e"]);
    }

    #[test]
    fn header_forms() {
        assert!(header_ok("// Written for solidcorex.\n//! doc\n"));
        let moved = format!(
            "// Moved from solidcore `x.rs` at commit {PIN}.\n// Upstream record: y.\n// Changes: z.\n"
        );
        assert!(header_ok(&moved));
        assert!(!header_ok("//! doc only\n"));
        assert!(!header_ok(
            "// Moved from solidcore `x.rs` at commit deadbeef.\n// Changes: z.\n"
        ));
    }
}
