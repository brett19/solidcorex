// Written for solidcorex. Apache-2.0.
//! Repository checks, run as `cargo xtask <command>`:
//!
//! - `layers`: every workspace crate is listed in `layers.txt`, and a library depends
//!   (normal, build or dev) only on libraries listed above it and on the external crates
//!   `layers.txt` `[external]` names (none today). The differential-test crates may
//!   depend on any library and alone on the pinned solidcore (`solidcore` and
//!   `solidcore-record`); tool crates depend on no
//!   workspace crate. The test-kit crates (`[testkit]`) may depend on any library, are
//!   depended on only as a dev-dependency (by a library or a difftest crate), and say
//!   `publish = false` in their own manifest.
//! - `headers`: every `.rs` file under `crates/` and `xtask/` opens with a derivation
//!   header (`Moved from solidcore ...` or `Derived from solidcore ...` `at commit <pin>`,
//!   its `Upstream record` and its `Changes:`) or says it was written for solidcorex. For
//!   a derivation header, each named pin file must exist at the pin (`git show PIN:path`
//!   in the pinned checkout `../sc-wt/scx-pin`) and its own upstream record
//!   must appear word for word in the quoted record.
//! - `pin`: the pinned checkout is at the pin commit with no change (the difftest
//!   crate's build script refuses to build otherwise).
//! - `bodydiff`: the normalised body diff (`xtask/bodydiff.py`) of every derived file
//!   against its pin files; each fn that differs, is only here, or (unless the move was
//!   partial) is only at the pin must be named in the header's `Changes:`; a file written
//!   for solidcorex may hold no fn body of the pin.
//! - `cov`: region coverage of the moved libraries (crate names, default every library) while
//!   the differential suite runs (`xtask/cov.sh`, written under `target/cov/`).
//! - `movecheck`: the standing steps of a move, in order: `pin`, `layers`, `headers`,
//!   `bodydiff`, `cov`.
//! - `fast`: `cargo fmt --check` of every member, clippy with warnings denied (the default
//!   members, then the difftest crate), `layers`, `headers`, the tier-1 tests (`cargo test
//!   -p <crate>` for each crate named, default every library and test kit) and xtask's own tests.
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
        Some("pin") => pin(&root),
        Some("bodydiff") => bodydiff(&root),
        Some("cov") => cov(&root, args.get(1..).unwrap_or(&[])),
        Some("movecheck") => movecheck(&root, args.get(1..).unwrap_or(&[])),
        Some("fast") => fast(&root, args.get(1..).unwrap_or(&[])),
        _ => Err("usage: cargo xtask <layers | headers | pin | bodydiff | cov [crate ...] | movecheck [crate ...] | fast [crate ...]>".to_string()),
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
    /// Test support: references and generators, never published, a dev-dependency only.
    Testkit,
    Tool,
}

/// The table a dependency is listed in.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Kind {
    Normal,
    Dev,
    Build,
}

/// What `layers` reads from a member's manifest.
#[derive(Debug, Default)]
struct Manifest {
    name: Option<String>,
    /// Every dependency, by crate name (a rename's `package`), with its table.
    deps: Vec<(String, Kind)>,
    /// `[package] publish`, as written (`None` when absent).
    publish: Option<String>,
}

/// `layers.txt`: the crates in order and the external crates libraries may use.
struct Layers {
    roles: Vec<(String, Role)>,
    /// `[external]`: crates outside the workspace a library or tool may depend on
    /// (empty today).
    external: Vec<String>,
}

/// The crates the differential crate may (alone) depend on: the pinned solidcore, and its
/// record crate (the Part 21 reader `scx-p21` is compared with).
const PINNED: &[&str] = &["solidcore", "solidcore-record"];

fn layer_roles(root: &Path) -> Result<Layers, String> {
    parse_layers(&read(&root.join("layers.txt"))?)
}

fn parse_layers(text: &str) -> Result<Layers, String> {
    let mut section = String::new();
    let mut out: Vec<(String, Role)> = Vec::new();
    let mut external = Vec::new();
    let mut libs = 0;
    for line in text.lines() {
        let line = strip_comment(line).trim();
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
            "testkit" => Role::Testkit,
            "tool" => Role::Tool,
            "external" => {
                if PINNED.contains(&line) {
                    return Err(format!(
                        "layers.txt: [external] may not name {line} (only the difftest crate uses it)"
                    ));
                }
                external.push(line.to_string());
                continue;
            }
            other => return Err(format!("layers.txt: unknown section [{other}]")),
        };
        if out.iter().any(|(n, _)| n == line) {
            return Err(format!("layers.txt lists {line} twice"));
        }
        out.push((line.to_string(), role));
    }
    Ok(Layers {
        roles: out,
        external,
    })
}

/// A line without its `#` comment; a `#` inside a quoted string is kept.
fn strip_comment(line: &str) -> &str {
    let mut quoted = false;
    let mut escaped = false;
    for (i, c) in line.char_indices() {
        match c {
            '\\' if quoted && !escaped => {
                escaped = true;
                continue;
            }
            '"' if !escaped => quoted = !quoted,
            '#' if !quoted => return line.get(..i).unwrap_or(line),
            _ => {}
        }
        escaped = false;
    }
    line
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
    let members: Vec<String> = list
        .lines()
        .map(strip_comment)
        .collect::<Vec<_>>()
        .join(",")
        .split(',')
        .map(|s| s.trim().trim_matches('"').to_string())
        .filter(|s| !s.is_empty())
        .collect();
    if let Some(glob) = members.iter().find(|m| m.contains(['*', '?', '['])) {
        return Err(format!(
            "Cargo.toml: workspace member `{glob}` is a glob; xtask reads members literally, so list each crate"
        ));
    }
    Ok(members)
}

/// A member's package name, the crate names it depends on (every dependency table) and
/// its `publish` setting.
fn manifest_deps(manifest: &str) -> Manifest {
    let mut section = String::new();
    let mut out = Manifest::default();
    let kind_of = |section: &str| {
        if section.contains("dev-dependencies") {
            Kind::Dev
        } else if section.contains("build-dependencies") {
            Kind::Build
        } else {
            Kind::Normal
        }
    };
    for raw in manifest.lines() {
        let line = strip_comment(raw).trim();
        if line.is_empty() {
            continue;
        }
        if line.starts_with('[') {
            section = line.trim_matches(|c| c == '[' || c == ']').to_string();
            // `[dependencies.foo]`: the first table that matches, as `dependencies.` is
            // inside the other two.
            let table = ["dev-dependencies.", "build-dependencies.", "dependencies."]
                .into_iter()
                .find_map(|t| section.find(t).map(|pos| pos + t.len()));
            if let Some(dep) = table.and_then(|start| section.get(start..)) {
                out.deps
                    .push((dep.trim_matches('"').to_string(), kind_of(&section)));
            }
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        let key = key.trim();
        if section == "package" && key == "name" {
            out.name = Some(value.trim().trim_matches('"').to_string());
        }
        if section == "package" && key == "publish" {
            out.publish = Some(value.trim().to_string());
        }
        if section.ends_with("dependencies") {
            let dep = key.split('.').next().unwrap_or(key).trim_matches('"');
            let renamed = value
                .split_once("package")
                .and_then(|(_, p)| p.split('"').nth(1))
                .map(str::to_string);
            out.deps.push((
                renamed.unwrap_or_else(|| dep.to_string()),
                kind_of(&section),
            ));
        }
    }
    out
}

fn layers(root: &Path) -> Result<String, String> {
    let layers = layer_roles(root)?;
    let mut crates = Vec::new();
    for member in workspace_members(root)? {
        let text = read(&root.join(&member).join("Cargo.toml"))?;
        let manifest = manifest_deps(&text);
        if manifest.name.is_none() {
            return Err(format!("{member}/Cargo.toml has no package name"));
        }
        crates.push(manifest);
    }
    let errors = check_layers(&layers, &crates);
    if errors.is_empty() {
        Ok(format!("layers: {} crates in order", crates.len()))
    } else {
        Err(format!("layers:\n{errors}"))
    }
}

/// Every breach of the order, one per line (empty when there is none). A library depends
/// only on libraries above it in `layers.txt` and on crates `[external]` names; the
/// difftest crates also on the libraries and on the pinned solidcore; a test kit on the
/// libraries; a tool on no workspace crate and only on `[external]` crates. A test kit is
/// a dev-dependency only (of a library or a difftest crate) and is never published.
fn check_layers(layers: &Layers, crates: &[Manifest]) -> String {
    let role_of = |n: &str| layers.roles.iter().find(|(m, _)| m == n).map(|(_, r)| *r);
    let names: Vec<&str> = crates.iter().filter_map(|m| m.name.as_deref()).collect();
    let mut errors = String::new();
    for manifest in crates {
        let Some(name) = manifest.name.as_deref() else {
            continue;
        };
        let Some(role) = role_of(name) else {
            let _ = writeln!(errors, "{name} is not listed in layers.txt");
            continue;
        };
        if role == Role::Testkit && manifest.publish.as_deref() != Some("false") {
            let _ = writeln!(
                errors,
                "{name} is a test kit: its manifest must say `publish = false`"
            );
        }
        for (dep, kind) in &manifest.deps {
            let ok = if names.contains(&dep.as_str()) {
                match (role, role_of(dep)) {
                    (Role::Library(me), Some(Role::Library(it))) => it < me,
                    (Role::Difftest | Role::Testkit, Some(Role::Library(_))) => true,
                    (Role::Library(_) | Role::Difftest, Some(Role::Testkit)) => *kind == Kind::Dev,
                    _ => false,
                }
            } else if PINNED.contains(&dep.as_str()) {
                role == Role::Difftest
            } else {
                layers.external.iter().any(|e| e == dep)
            };
            if !ok {
                let why = if role_of(dep) == Some(Role::Testkit) {
                    " (a test kit is a dev-dependency of a library or difftest crate only)"
                } else if names.contains(&dep.as_str()) {
                    ""
                } else if PINNED.contains(&dep.as_str()) {
                    " (only the difftest crate may use the pinned solidcore)"
                } else {
                    " (an external crate not in layers.txt [external])"
                };
                let _ = writeln!(errors, "{name} may not depend on {dep}{why}");
            }
        }
    }
    for (listed, _) in &layers.roles {
        if !names.contains(&listed.as_str()) {
            let _ = writeln!(errors, "layers.txt lists {listed}, not a workspace crate");
        }
    }
    errors
}

// ---- headers -----------------------------------------------------------------------

fn rust_files(dir: &Path, out: &mut Vec<PathBuf>) -> Result<(), String> {
    let entries = fs::read_dir(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    for entry in entries {
        let path = entry.map_err(|e| e.to_string())?.path();
        let skip = path
            .file_name()
            .is_some_and(|n| n == "target" || n.to_string_lossy().starts_with('.'));
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

/// Whitespace runs as one space.
fn squash(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// A derivation header names its pin files and quotes their upstream records.
fn is_derived(head: &str) -> bool {
    head.contains("Moved from solidcore") || head.contains("Derived from solidcore")
}

fn header_ok(text: &str) -> bool {
    let head = opening_comment(text);
    let written = head.to_lowercase().contains("written for solidcorex");
    let moved = is_derived(&head)
        && head.contains(PIN)
        && head.contains("Upstream record")
        && head.contains("Changes:");
    written || moved
}

/// The pin files a derivation header names: the backquoted `solidcore/...` paths before
/// its `Upstream record`.
fn pin_sources(head: &str) -> Vec<String> {
    let before = head.split("Upstream record").next().unwrap_or("");
    before
        .split('`')
        .skip(1)
        .step_by(2)
        .filter(|p| {
            p.starts_with("solidcore/") && Path::new(p).extension().is_some_and(|e| e == "rs")
        })
        .map(str::to_string)
        .collect()
}

/// The header's quoted record: from `Upstream record (...):` to `Changes:`, squashed,
/// without the `//` comment markers.
fn quoted_record(head: &str) -> String {
    let plain: String = head
        .lines()
        .map(|l| l.strip_prefix("//").unwrap_or(l))
        .collect::<Vec<_>>()
        .join(" ");
    let after = plain.split_once("Upstream record").map_or("", |(_, a)| a);
    let after = after.split_once("):").map_or(after, |(_, a)| a);
    squash(after.split("Changes:").next().unwrap_or(""))
}

/// A pin file's upstream record: the paragraph of its module doc that opens with
/// `Upstream`, `Fork` or `Not upstream`, squashed.
fn pin_record(pin_file: &str) -> Option<String> {
    let mut rec: Vec<&str> = Vec::new();
    for line in pin_file.lines() {
        let Some(doc) = line.strip_prefix("//!") else {
            if rec.is_empty() {
                continue;
            }
            break;
        };
        let doc = doc.trim();
        if rec.is_empty() {
            if doc.starts_with("Upstream")
                || doc.starts_with("Fork")
                || doc.starts_with("Not upstream")
            {
                rec.push(doc);
            }
        } else if doc.is_empty() {
            break;
        } else {
            rec.push(doc);
        }
    }
    (!rec.is_empty()).then(|| squash(&rec.join(" ")))
}

/// A pin file's first module-doc paragraph, squashed: the record of a file that has no
/// `Upstream`/`Fork` paragraph (one written for solidcore, saying so there).
fn pin_first_paragraph(pin_file: &str) -> Option<String> {
    let mut rec: Vec<&str> = Vec::new();
    for line in pin_file.lines() {
        let Some(doc) = line.strip_prefix("//!") else {
            if rec.is_empty() {
                continue;
            }
            break;
        };
        let doc = doc.trim();
        if doc.is_empty() {
            if rec.is_empty() {
                continue;
            }
            break;
        }
        rec.push(doc);
    }
    (!rec.is_empty()).then(|| squash(&rec.join(" ")))
}

/// The pinned checkout, `../sc-wt/scx-pin` beside the repository (the difftest crate's
/// path dependency).
fn pin_dir(root: &Path) -> PathBuf {
    root.parent().unwrap_or(root).join("sc-wt").join("scx-pin")
}

fn git_in(dir: &Path, args: &[&str]) -> Result<String, String> {
    let out = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(args)
        .output()
        .map_err(|e| format!("git {}: {e}", args.join(" ")))?;
    if out.status.success() {
        String::from_utf8(out.stdout).map_err(|e| e.to_string())
    } else {
        Err(format!(
            "git {} in {}: {}",
            args.join(" "),
            dir.display(),
            String::from_utf8_lossy(&out.stderr).trim()
        ))
    }
}

/// The pinned checkout is at [`PIN`] and clean (the difftest's build script checks the
/// same before it links solidcore).
fn pin(root: &Path) -> Result<String, String> {
    let dir = pin_dir(root);
    let head = git_in(&dir, &["rev-parse", "HEAD"])?;
    if head.trim() != PIN {
        return Err(format!(
            "pin: {} is at {}, not {PIN}",
            dir.display(),
            head.trim()
        ));
    }
    let dirty = git_in(&dir, &["status", "--porcelain"])?;
    if !dirty.trim().is_empty() {
        return Err(format!("pin: {} has changes:\n{dirty}", dir.display()));
    }
    Ok(format!("pin: {} at {PIN}, clean", dir.display()))
}

/// `git show PIN:<path>` in the pinned checkout.
fn pin_show(root: &Path, path: &str) -> Result<String, String> {
    git_in(&pin_dir(root), &["show", &format!("{PIN}:{path}")])
        .map_err(|_| format!("{path} does not exist at the pin"))
}

/// What is wrong with a derivation header against the pin: each named file exists at
/// [`PIN`] and its upstream record (or, where it has none, its first module-doc paragraph)
/// appears word for word in the quoted record.
fn derivation_against_pin(root: &Path, head: &str) -> Vec<String> {
    let sources = pin_sources(head);
    if sources.is_empty() {
        return vec!["names no `solidcore/...` pin file".to_string()];
    }
    let quoted = quoted_record(head);
    let mut out = Vec::new();
    for src in &sources {
        match pin_show(root, src) {
            Err(e) => out.push(e),
            Ok(text) => match pin_record(&text).or_else(|| pin_first_paragraph(&text)) {
                None => out.push(format!("{src} has no upstream record at the pin")),
                Some(rec) if !quoted.contains(&rec) => out.push(format!(
                    "the quoted record lacks {src}'s, word for word: \"{rec}\""
                )),
                Some(_) => {}
            },
        }
    }
    out
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
    let mut derived = 0;
    for f in &files {
        let text = read(f)?;
        let shown = f.strip_prefix(root).unwrap_or(f).display().to_string();
        if !header_ok(&text) {
            let _ = writeln!(bad, "{shown} has no derivation or solidcorex header");
            continue;
        }
        let head = opening_comment(&text);
        if is_derived(&head) {
            derived += 1;
            for e in derivation_against_pin(root, &head) {
                let _ = writeln!(bad, "{shown}: {e}");
            }
        }
    }
    if bad.is_empty() {
        Ok(format!(
            "headers: {} files ({derived} derived, records match the pin)",
            files.len()
        ))
    } else {
        Err(format!("headers:\n{bad}"))
    }
}

// ---- standing steps of a move ------------------------------------------------------

fn run(root: &Path, program: &str, args: &[&str]) -> Result<(), String> {
    println!("xtask: {program} {}", args.join(" "));
    let status = Command::new(program)
        .args(args)
        .current_dir(root)
        .status()
        .map_err(|e| format!("{program}: {e}"))?;
    if status.success() {
        Ok(())
    } else {
        Err(format!("{program} {} failed", args.join(" ")))
    }
}

fn bodydiff(root: &Path) -> Result<String, String> {
    println!("{}", pin(root)?);
    let script = root.join("xtask").join("bodydiff.py");
    let pin = pin_dir(root);
    run(
        root,
        "python3",
        &[
            "-I",
            &script.to_string_lossy(),
            &root.to_string_lossy(),
            &pin.to_string_lossy(),
        ],
    )?;
    Ok("bodydiff: passed".to_string())
}

fn cov(root: &Path, crates: &[String]) -> Result<String, String> {
    println!("{}", pin(root)?);
    let script = root.join("xtask").join("cov.sh");
    let mut args = vec![
        script.to_string_lossy().to_string(),
        root.to_string_lossy().to_string(),
    ];
    args.extend(crates.iter().cloned());
    run(
        root,
        "bash",
        &args.iter().map(String::as_str).collect::<Vec<_>>(),
    )?;
    Ok("cov: written to target/cov/".to_string())
}

fn movecheck(root: &Path, crates: &[String]) -> Result<String, String> {
    println!("{}", pin(root)?);
    println!("{}", layers(root)?);
    println!("{}", headers(root)?);
    println!("{}", bodydiff(root)?);
    println!("{}", cov(root, crates)?);
    Ok("movecheck: passed".to_string())
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
        if let Some(n) = manifest_deps(&read(&root.join(&member).join("Cargo.toml"))?).name {
            fmt.push("-p".to_string());
            fmt.push(n);
        }
    }
    cargo(root, &fmt.iter().map(String::as_str).collect::<Vec<_>>())?;
    cargo(root, &["clippy", "--all-targets", "--", "-D", "warnings"])?;
    // The difftest crate is not a default member (it links solidcore): lint it by name.
    cargo(
        root,
        &[
            "clippy",
            "-p",
            "scx-difftest",
            "--all-targets",
            "--",
            "-D",
            "warnings",
        ],
    )?;
    println!("{}", layers(root)?);
    println!("{}", headers(root)?);
    let libs: Vec<String> = if crates.is_empty() {
        layer_roles(root)?
            .roles
            .into_iter()
            .filter(|(_, r)| matches!(r, Role::Library(_) | Role::Testkit))
            .map(|(n, _)| n)
            .collect()
    } else {
        crates.to_vec()
    };
    for lib in &libs {
        cargo(root, &["test", "-p", lib])?;
    }
    cargo(root, &["test", "-p", "xtask"])?;
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
        let m = manifest_deps(m);
        assert_eq!(m.name.as_deref(), Some("a"));
        let names: Vec<&str> = m.deps.iter().map(|(d, _)| d.as_str()).collect();
        assert_eq!(names, ["b", "c", "d", "e"]);
        let kinds: Vec<Kind> = m.deps.iter().map(|(_, k)| *k).collect();
        assert_eq!(kinds, [Kind::Normal, Kind::Dev, Kind::Dev, Kind::Normal]);
        let m = manifest_deps(
            "[package]\nname = \"k\"\npublish = false\n[target.'cfg(unix)'.dev-dependencies]\n\
             a = { path = \"../a\" }\n[build-dependencies.b]\npath = \"../b\"\n",
        );
        assert_eq!(m.publish.as_deref(), Some("false"));
        assert_eq!(
            m.deps,
            [("a".to_string(), Kind::Dev), ("b".to_string(), Kind::Build)]
        );
    }

    /// Manifests: each dependency `"x"` is normal, `"dev:x"` a dev-dependency; every crate
    /// says `publish = false`.
    fn crates(list: &[(&str, &[&str])]) -> Vec<Manifest> {
        list.iter()
            .map(|(n, d)| Manifest {
                name: Some((*n).to_string()),
                deps: d
                    .iter()
                    .map(|s| match s.strip_prefix("dev:") {
                        Some(dev) => (dev.to_string(), Kind::Dev),
                        None => ((*s).to_string(), Kind::Normal),
                    })
                    .collect(),
                publish: Some("false".to_string()),
            })
            .collect()
    }

    const LAYERS: &str = "a\nb # above a\n[difftest]\nd\n[testkit]\nk\n[tool]\nt\n[external]\n";

    #[test]
    fn the_order_holds() {
        let l = parse_layers(LAYERS).unwrap();
        let ok = crates(&[
            ("a", &[]),
            ("b", &["a"]),
            ("d", &["a", "b", "solidcore", "solidcore-record", "dev:k"]),
            ("k", &["a", "b"]),
            ("t", &[]),
        ]);
        assert_eq!(check_layers(&l, &ok), "");
    }

    #[test]
    fn a_test_kit_is_a_dev_dependency_never_published() {
        let l = parse_layers(LAYERS).unwrap();
        // The breaking cases: a normal dependency on the kit, a tool on it, the kit on
        // the pin, and a kit its manifest would let be published.
        let mut bad = crates(&[
            ("a", &["dev:k"]),
            ("b", &["k"]),
            ("d", &[]),
            ("k", &["solidcore"]),
            ("t", &["dev:k"]),
        ]);
        if let Some(k) = bad.iter_mut().find(|m| m.name.as_deref() == Some("k")) {
            k.publish = None;
        }
        let e = check_layers(&l, &bad);
        assert!(!e.contains("a may not"), "{e}");
        assert!(e.contains("b may not depend on k (a test kit"), "{e}");
        assert!(e.contains("t may not depend on k (a test kit"), "{e}");
        assert!(e.contains("k may not depend on solidcore"), "{e}");
        assert!(e.contains("k is a test kit: its manifest must say"), "{e}");
    }

    #[test]
    fn a_library_may_not_depend_outside_the_workspace() {
        let l = parse_layers(LAYERS).unwrap();
        // The breaking case: a library on the pinned solidcore, or on any crate the
        // (empty) allow-list does not name; a library below on one above.
        let bad = crates(&[
            ("a", &["solidcore", "b", "solidcore-record"]),
            ("b", &["rand"]),
            ("d", &[]),
            ("k", &[]),
            ("t", &["a"]),
        ]);
        let e = check_layers(&l, &bad);
        assert!(
            e.contains("a may not depend on solidcore (only the difftest"),
            "{e}"
        );
        assert!(e.contains("a may not depend on b\n"), "{e}");
        assert!(
            e.contains("a may not depend on solidcore-record (only the difftest"),
            "{e}"
        );
        assert!(
            e.contains("b may not depend on rand (an external crate"),
            "{e}"
        );
        assert!(e.contains("t may not depend on a\n"), "{e}");
        let allowed = parse_layers(&format!("{LAYERS}rand\n")).unwrap();
        let e = check_layers(
            &allowed,
            &crates(&[
                ("a", &[]),
                ("b", &["rand"]),
                ("d", &[]),
                ("k", &[]),
                ("t", &[]),
            ]),
        );
        assert_eq!(e, "");
        assert!(parse_layers(&format!("{LAYERS}solidcore\n")).is_err());
        assert!(parse_layers(&format!("{LAYERS}solidcore-record\n")).is_err());
    }

    #[test]
    fn comments_and_quotes() {
        assert_eq!(strip_comment("a = \"x#y\" # c"), "a = \"x#y\" ");
        assert_eq!(strip_comment("a = \"x\\\"#\" # c"), "a = \"x\\\"#\" ");
        let m = manifest_deps(
            "[package]\nname = \"a\"\n[dependencies]\nb = { path = \"../b#1\" } # c\n",
        );
        assert_eq!(m.deps, [("b".to_string(), Kind::Normal)]);
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

    #[test]
    fn records_are_read_from_both_sides() {
        let head = "// Moved from solidcore `solidcore/a.rs` and `solidcore/b.rs` at commit x.\n\
                    // Upstream record (verbatim from those files): Upstream: `A.cpp`,\n\
                    // `A.h`. Fork: b.\n// Changes: none.";
        assert_eq!(pin_sources(head), ["solidcore/a.rs", "solidcore/b.rs"]);
        assert_eq!(quoted_record(head), "Upstream: `A.cpp`, `A.h`. Fork: b.");
        let pin =
            "//! Title.\n//!\n//! Upstream: `A.cpp`,\n//! `A.h`.\n//!\n//! | table |\nuse x;\n";
        assert_eq!(
            pin_record(pin).as_deref(),
            Some("Upstream: `A.cpp`, `A.h`.")
        );
        assert_eq!(pin_record("//! No record.\n"), None);
    }
}
