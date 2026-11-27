// Written for solidcorex. Apache-2.0.
//! Repository checks, run as `cargo xtask <command>`:
//!
//! - `layers`: every workspace crate is listed in `layers.txt`, and a library depends
//!   (normal, build or dev) only on libraries listed above it and on the external crates
//!   `layers.txt` `[external]` names (none today). The differential-test crates may
//!   depend on any library and alone on the pinned solidcore (`solidcore` and
//!   `solidcore-record`); tool crates depend on no
//!   workspace crate. The test-kit crates (`[testkit]`) may depend on any library, are
//!   depended on only as a dev-dependency (by a library, difftest or bench crate), and say
//!   `publish = false` in their own manifest. The benchmark crates (`[bench]`) may depend on
//!   any library and on the pinned solidcore, on a test kit as a dev-dependency, and alone on
//!   the external crates `[bench-external]` names (criterion, gungraun), as dev-dependencies only;
//!   nothing depends on them, and they say `publish = false`.
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
//! - `conflicts`: fails when a tracked file holds a merge-conflict marker line (`<<<<<<<`,
//!   `=======` or `>>>>>>>` at the start of a line, then a space or the line's end).
//! - `fast`: the `conflicts` check, then `cargo fmt --check` of every member, clippy with warnings denied (the default
//!   members, then the difftest and bench crates, the benchmarks included), `layers`,
//!   `headers`, the tier-1 tests (`cargo test -p <crate>` for each crate named, default every
//!   library and test kit), the bench crate's own tests and xtask's.
//! - `lint` and `tests [crate ... | --except crate ...]`: the two halves of `fast`, which CI runs
//!   as separate jobs (`tests` shards by crate; `--except` takes the rest, with the bench crate's
//!   tests and xtask's, so named shards and one `--except` shard together run all of `fast`).
//! - `diff [--changed-since REV]`: the differential suite (`cargo test -p scx-difftest`). With
//!   `--changed-since`, the three slow tests (`SLOW_DIFF`, minutes each) run only when a file
//!   changed since `REV` (the working tree against it) is one their subject can change:
//!   `scx-algo`, `scx-nurbs`, `scx-nurbs-surface`, `scx-num`, `scx-testkit` (their exact
//!   references), the difftest crate's files for them and the manifests. The skipped tests are
//!   named in the output; CI runs every test every push.
//! - `bench`: build the benchmarks in the bench profile without running them (`cargo bench
//!   -p scx-bench --no-run`); docs/02-testing.md says how to run them.
//!
//! No dependencies: the manifests are read line by line, which is enough for the plain
//! manifests this repository writes.

use std::fmt::Write as _;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};

/// The solidcore commit every moved file derives from.
const PIN: &str = "03e16c9ee766de5b9b1ced845e5149d62ff493b8";
/// The tree of `solidcore/` at [`PIN`]. A checkout rebuilt from the public mirror (CI) has a
/// different commit hash and this same tree, which is the same content.
const PIN_TREE: &str = "c5c43059c99ed6b4d36e5ddf2c76dcb1992848ab";

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
        Some("conflicts") => conflicts(&root),
        Some("fast") => fast(&root, args.get(1..).unwrap_or(&[])),
        Some("lint") => lint(&root),
        Some("diff") => diff(&root, args.get(1..).unwrap_or(&[])),
        Some("tests") => tests(&root, args.get(1..).unwrap_or(&[]), false),
        Some("bench") => bench(&root),
        _ => Err("usage: cargo xtask <layers | headers | pin | bodydiff | cov [crate ...] | movecheck [crate ...] | conflicts | fast [crate ...] | lint | diff [--changed-since rev] | tests [crate ... | --except crate ...] | bench>".to_string()),
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
    /// Benchmarks against the pin: criterion and gungraun as dev-dependencies, never published.
    Bench,
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
    /// `[bench-external]`: crates outside the workspace a bench crate alone may use, as a
    /// dev-dependency only (the timing harness).
    bench_external: Vec<String>,
}

/// `cargo xtask bench`: every benchmark target built in the bench profile, none run.
fn bench(root: &Path) -> Result<String, String> {
    cargo(root, &["bench", "-p", "scx-bench", "--no-run"])?;
    Ok("bench: built".to_string())
}

/// The crates the differential and bench crates may (alone) depend on: the pinned
/// solidcore, and its record crate (the Part 21 reader `scx-p21` is compared with).
const PINNED: &[&str] = &["solidcore", "solidcore-record"];

fn layer_roles(root: &Path) -> Result<Layers, String> {
    parse_layers(&read(&root.join("layers.txt"))?)
}

fn parse_layers(text: &str) -> Result<Layers, String> {
    let mut section = String::new();
    let mut out: Vec<(String, Role)> = Vec::new();
    let mut external = Vec::new();
    let mut bench_external = Vec::new();
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
            "bench" => Role::Bench,
            "tool" => Role::Tool,
            "external" | "bench-external" => {
                if PINNED.contains(&line) {
                    return Err(format!(
                        "layers.txt: [{section}] may not name {line} \
                         (only the difftest and bench crates use it)"
                    ));
                }
                if section == "external" {
                    external.push(line.to_string());
                } else {
                    bench_external.push(line.to_string());
                }
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
        bench_external,
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
/// difftest crates also on the libraries and on the pinned solidcore; a bench crate on the
/// libraries, the pinned solidcore and (as dev-dependencies) `[bench-external]` crates; a
/// test kit on the libraries; a tool on no workspace crate and only on `[external]` crates.
/// A test kit is a dev-dependency only (of a library, difftest or bench crate); nothing
/// depends on a bench crate. Neither is ever published.
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
        if matches!(role, Role::Testkit | Role::Bench)
            && manifest.publish.as_deref() != Some("false")
        {
            let what = if role == Role::Testkit {
                "a test kit"
            } else {
                "a bench crate"
            };
            let _ = writeln!(
                errors,
                "{name} is {what}: its manifest must say `publish = false`"
            );
        }
        for (dep, kind) in &manifest.deps {
            let ok = if names.contains(&dep.as_str()) {
                match (role, role_of(dep)) {
                    (Role::Library(me), Some(Role::Library(it))) => it < me,
                    (Role::Difftest | Role::Testkit | Role::Bench, Some(Role::Library(_))) => true,
                    (Role::Library(_) | Role::Difftest | Role::Bench, Some(Role::Testkit)) => {
                        *kind == Kind::Dev
                    }
                    _ => false,
                }
            } else if PINNED.contains(&dep.as_str()) {
                matches!(role, Role::Difftest | Role::Bench)
            } else if layers.external.iter().any(|e| e == dep) {
                true
            } else {
                role == Role::Bench
                    && *kind == Kind::Dev
                    && layers.bench_external.iter().any(|e| e == dep)
            };
            if !ok {
                let why = if role_of(dep) == Some(Role::Testkit) {
                    " (a test kit is a dev-dependency of a library, difftest or bench crate only)"
                } else if names.contains(&dep.as_str()) {
                    ""
                } else if PINNED.contains(&dep.as_str()) {
                    " (only the difftest and bench crates may use the pinned solidcore)"
                } else if layers.bench_external.iter().any(|e| e == dep) {
                    " (a [bench-external] crate is a dev-dependency of a bench crate only)"
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

/// The revision that names the pin in the pinned checkout: [`PIN`] itself, or `HEAD` when
/// the checkout is a rebuild from the public mirror whose `solidcore/` tree is [`PIN_TREE`].
fn pin_rev(dir: &Path) -> Result<&'static str, String> {
    let head = git_in(dir, &["rev-parse", "HEAD"])?;
    if head.trim() == PIN {
        return Ok(PIN);
    }
    let tree = git_in(dir, &["rev-parse", "HEAD:solidcore"])?;
    if tree.trim() == PIN_TREE {
        return Ok("HEAD");
    }
    Err(format!(
        "pin: {} is at {}, not {PIN} (nor the same solidcore/ tree {PIN_TREE})",
        dir.display(),
        head.trim()
    ))
}

/// The pinned checkout is at [`PIN`] (or has its tree) and clean (the difftest's build
/// script checks the same before it links solidcore).
fn pin(root: &Path) -> Result<String, String> {
    let dir = pin_dir(root);
    pin_rev(&dir)?;
    let dirty = git_in(&dir, &["status", "--porcelain"])?;
    if !dirty.trim().is_empty() {
        return Err(format!("pin: {} has changes:\n{dirty}", dir.display()));
    }
    Ok(format!("pin: {} at {PIN}, clean", dir.display()))
}

/// `git show PIN:<path>` in the pinned checkout.
fn pin_show(root: &Path, path: &str) -> Result<String, String> {
    let dir = pin_dir(root);
    let rev = pin_rev(&dir).map_err(|e| format!("{path}: {e}"))?;
    git_in(&dir, &["show", &format!("{rev}:{path}")])
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

/// Runs the cargo invocations `jobs` side by side (`XTASK_JOBS` at a time, default 16), each
/// with its output held until it ends, so the lines of two runs never interleave. Every job runs
/// to its end; the error names the ones that failed.
fn cargo_parallel(root: &Path, jobs: Vec<Vec<String>>) -> Result<(), String> {
    use std::sync::Mutex;
    let width = std::env::var("XTASK_JOBS")
        .ok()
        .and_then(|v| v.parse::<usize>().ok())
        .filter(|n| *n > 0)
        .unwrap_or(16);
    let cargo = std::env::var("CARGO").unwrap_or_else(|_| "cargo".to_string());
    let queue = Mutex::new(jobs.into_iter());
    let failed = Mutex::new(Vec::new());
    std::thread::scope(|scope| {
        for _ in 0..width {
            scope.spawn(|| {
                loop {
                    let next = queue.lock().ok().and_then(|mut q| q.next());
                    let Some(args) = next else { return };
                    let out = Command::new(&cargo).args(&args).current_dir(root).output();
                    let line = format!("cargo {}", args.join(" "));
                    let ok = match out {
                        Ok(out) => {
                            // One print per job, so its lines stay together.
                            let text = format!(
                                "xtask: {line}\n{}{}",
                                String::from_utf8_lossy(&out.stderr),
                                String::from_utf8_lossy(&out.stdout)
                            );
                            println!("{}", text.trim_end());
                            out.status.success()
                        }
                        Err(e) => {
                            println!("xtask: {line}: {e}");
                            false
                        }
                    };
                    if !ok && let Ok(mut f) = failed.lock() {
                        f.push(line);
                    }
                }
            });
        }
    });
    let failed = failed.into_inner().map_err(|e| e.to_string())?;
    if failed.is_empty() {
        Ok(())
    } else {
        Err(format!("{} failed", failed.join(", ")))
    }
}

/// The difftest tests that take minutes (their subject: the polynomial solvers, curve and
/// surface degree elevation) and the paths whose change can alter them.
const SLOW_DIFF: &[&str] = &[
    "t_algo::poly_from_roots",
    "t_nurbs::nurbs_elevate",
    "t_nurbs_surface_edit::nurbs_surface_elevate",
];
const SLOW_DIFF_PATHS: &[&str] = &[
    "crates/scx-algo/",
    "crates/scx-nurbs/",
    "crates/scx-nurbs-surface/",
    "crates/scx-num/",
    "crates/scx-testkit/",
    "crates/scx-difftest/tests/diff/t_algo.rs",
    "crates/scx-difftest/tests/diff/t_nurbs.rs",
    "crates/scx-difftest/tests/diff/t_nurbs_surface_edit.rs",
    "crates/scx-difftest/tests/diff/pin_defects.rs",
    "crates/scx-difftest/tests/diff/harness.rs",
    "crates/scx-difftest/tests/diff/facade.rs",
    "crates/scx-difftest/tests/diff/main.rs",
    "Cargo.toml",
    "Cargo.lock",
    "rust-toolchain.toml",
];

/// Whether any of `changed` is under a path of [`SLOW_DIFF_PATHS`].
fn slow_diff_due(changed: &[String]) -> bool {
    changed
        .iter()
        .any(|f| SLOW_DIFF_PATHS.iter().any(|p| f.starts_with(p)))
}

/// `cargo xtask diff [--changed-since REV]`.
fn diff(root: &Path, args: &[String]) -> Result<String, String> {
    let mut test = vec![
        "test".to_string(),
        "-p".to_string(),
        "scx-difftest".to_string(),
    ];
    let skipped = match args {
        [] => false,
        [flag, rev] if flag == "--changed-since" => {
            let out = Command::new("git")
                .args(["diff", "--name-only", rev])
                .current_dir(root)
                .output()
                .map_err(|e| format!("git diff: {e}"))?;
            if !out.status.success() {
                return Err(format!(
                    "git diff {rev}: {}",
                    String::from_utf8_lossy(&out.stderr).trim_end()
                ));
            }
            let changed: Vec<String> = String::from_utf8_lossy(&out.stdout)
                .lines()
                .map(str::to_string)
                .collect();
            !slow_diff_due(&changed)
        }
        _ => return Err("usage: cargo xtask diff [--changed-since <rev>]".to_string()),
    };
    if skipped {
        test.push("--".to_string());
        for t in SLOW_DIFF {
            test.push("--skip".to_string());
            test.push((*t).to_string());
        }
        println!(
            "diff: skipping {} (nothing they test changed)",
            SLOW_DIFF.join(", ")
        );
    }
    cargo(root, &test.iter().map(String::as_str).collect::<Vec<_>>())?;
    Ok(if skipped {
        "diff: passed (slow tests skipped)".to_string()
    } else {
        "diff: passed".to_string()
    })
}

/// The pattern of a conflict-marker line, for `git grep -E`.
const CONFLICT_PATTERN: &str = "^(<<<<<<<|=======|>>>>>>>)( |$)";

fn conflicts(root: &Path) -> Result<String, String> {
    let out = Command::new("git")
        .args(["grep", "-nE", CONFLICT_PATTERN])
        .current_dir(root)
        .output()
        .map_err(|e| format!("git grep: {e}"))?;
    match out.status.code() {
        Some(1) => Ok("conflicts: none".to_string()),
        Some(0) => Err(format!(
            "conflict markers in tracked files:\n{}",
            String::from_utf8_lossy(&out.stdout).trim_end()
        )),
        _ => Err(format!(
            "git grep failed: {}",
            String::from_utf8_lossy(&out.stderr).trim_end()
        )),
    }
}

fn fast(root: &Path, crates: &[String]) -> Result<String, String> {
    println!("{}", lint(root)?);
    tests(root, crates, true)
}

/// The static checks of `fast`: conflict markers, fmt, clippy with warnings denied, layers and
/// headers. CI runs them as a job of their own, beside the test shards.
fn lint(root: &Path) -> Result<String, String> {
    println!("{}", conflicts(root)?);
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
    // The bench crate is not a default member either; `--all-targets` checks the benchmarks.
    cargo(
        root,
        &[
            "clippy",
            "-p",
            "scx-bench",
            "--all-targets",
            "--",
            "-D",
            "warnings",
        ],
    )?;
    println!("{}", layers(root)?);
    println!("{}", headers(root)?);
    Ok("lint: passed".to_string())
}

/// The tier-1 tests of `fast`. `crates` names the crates to test (default every library and test
/// kit); `--except a b` takes every one but those named. `extras` adds the bench crate's own tests
/// and xtask's; `tests` (the CI shards) adds them only when `crates` is `--except ...`, so the
/// shards together run exactly what `fast` runs.
fn tests(root: &Path, crates: &[String], extras: bool) -> Result<String, String> {
    let all = || -> Result<Vec<String>, String> {
        Ok(layer_roles(root)?
            .roles
            .into_iter()
            .filter(|(_, r)| matches!(r, Role::Library(_) | Role::Testkit))
            .map(|(n, _)| n)
            .collect())
    };
    let (libs, extras) = match crates.split_first() {
        None => (all()?, true),
        Some((flag, rest)) if flag == "--except" => {
            if let Some(bad) = rest.iter().find(|c| !all().is_ok_and(|a| a.contains(c))) {
                return Err(format!("tests: --except names no library: {bad}"));
            }
            (
                all()?.into_iter().filter(|c| !rest.contains(c)).collect(),
                true,
            )
        }
        Some(_) => (crates.to_vec(), extras),
    };
    // Each crate keeps its own `-p` selection (its own feature set, so nothing is rebuilt),
    // but its test targets run as separate cargo invocations, side by side: a few test
    // binaries are single-thread-bound for tens of seconds, and one after another they left
    // the machine mostly idle. The targets are cargo's auto-discovered ones (`src/lib.rs`,
    // `src/main.rs`, `tests/*.rs`, `tests/*/main.rs`) and the doctests; no library crate sets
    // a custom target.
    let mut long = Vec::new();
    let mut short = Vec::new();
    for lib in &libs {
        let dir = root.join("crates").join(lib);
        if !dir.join("Cargo.toml").exists() {
            return Err(format!("fast: no crate at {}", dir.display()));
        }
        let job = |rest: &[&str]| {
            let mut v = vec!["test".to_string(), "-p".to_string(), lib.clone()];
            v.extend(rest.iter().map(|a| (*a).to_string()));
            v
        };
        let mut tests: Vec<String> = Vec::new();
        for e in fs::read_dir(dir.join("tests"))
            .into_iter()
            .flatten()
            .flatten()
        {
            let path = e.path();
            let name = e.file_name().to_string_lossy().to_string();
            if path.is_dir() && path.join("main.rs").exists() {
                tests.push(name);
            } else if let Some(stem) = name.strip_suffix(".rs") {
                tests.push(stem.to_string());
            }
        }
        tests.sort();
        for t in &tests {
            long.push(job(&["--test", t]));
        }
        let (has_lib, has_bin) = (
            dir.join("src/lib.rs").exists(),
            dir.join("src/main.rs").exists(),
        );
        if has_lib || has_bin {
            let mut v = job(&[]);
            if has_lib {
                v.push("--lib".to_string());
            }
            if has_bin {
                v.push("--bins".to_string());
            }
            short.push(v);
        }
        if has_lib {
            short.push(job(&["--doc"]));
        }
    }
    let mut jobs = long;
    jobs.extend(short);
    if extras {
        jobs.push(
            ["test", "-p", "scx-bench", "--lib"]
                .map(String::from)
                .to_vec(),
        );
        jobs.push(["test", "-p", "xtask"].map(String::from).to_vec());
    }
    cargo_parallel(root, jobs)?;
    Ok(format!("fast: passed ({})", libs.join(", ")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_slow_difftests_run_only_for_their_subjects() {
        let f = |l: &[&str]| l.iter().map(|s| (*s).to_string()).collect::<Vec<_>>();
        assert!(slow_diff_due(&f(&["crates/scx-nurbs/src/lib.rs"])));
        assert!(slow_diff_due(&f(&[
            "docs/x.md",
            "crates/scx-testkit/src/exact.rs"
        ])));
        assert!(slow_diff_due(&f(&[
            "crates/scx-difftest/tests/diff/t_algo.rs"
        ])));
        assert!(!slow_diff_due(&f(&[
            "crates/scx-boolean/src/lib.rs",
            "docs/04-decisions.md"
        ])));
        assert!(!slow_diff_due(&f(&[
            "crates/scx-difftest/tests/diff/t_slice.rs"
        ])));
        assert!(!slow_diff_due(&[]));
    }

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

    const LAYERS: &str = "a\nb # above a\n[difftest]\nd\n[testkit]\nk\n[bench]\nm\n[tool]\nt\n\
                          [bench-external]\ncriterion\n[external]\n";

    #[test]
    fn the_order_holds() {
        let l = parse_layers(LAYERS).unwrap();
        let ok = crates(&[
            ("a", &[]),
            ("b", &["a"]),
            ("d", &["a", "b", "solidcore", "solidcore-record", "dev:k"]),
            ("k", &["a", "b"]),
            ("m", &["a", "b", "solidcore", "dev:k", "dev:criterion"]),
            ("t", &[]),
        ]);
        assert_eq!(check_layers(&l, &ok), "");
    }

    #[test]
    fn a_bench_crate_alone_uses_criterion_and_nothing_uses_it() {
        let l = parse_layers(LAYERS).unwrap();
        // The breaking cases: criterion as a normal dependency of the bench crate, as any
        // dependency of a library or the difftest crate; a dependency on the bench crate;
        // a bench crate its manifest would let be published.
        let mut bad = crates(&[
            ("a", &["dev:criterion"]),
            ("b", &["dev:m"]),
            ("d", &["dev:criterion", "m"]),
            ("k", &[]),
            ("m", &["criterion"]),
            ("t", &[]),
        ]);
        if let Some(m) = bad.iter_mut().find(|m| m.name.as_deref() == Some("m")) {
            m.publish = None;
        }
        let e = check_layers(&l, &bad);
        assert!(
            e.contains("a may not depend on criterion (a [bench-external]"),
            "{e}"
        );
        assert!(e.contains("b may not depend on m\n"), "{e}");
        assert!(
            e.contains("d may not depend on criterion (a [bench-external]"),
            "{e}"
        );
        assert!(e.contains("d may not depend on m\n"), "{e}");
        assert!(
            e.contains("m may not depend on criterion (a [bench-external]"),
            "{e}"
        );
        assert!(
            e.contains("m is a bench crate: its manifest must say"),
            "{e}"
        );
        assert!(parse_layers(&format!("{LAYERS}[bench-external]\nsolidcore\n")).is_err());
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
            ("m", &[]),
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
            ("m", &[]),
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
                ("m", &[]),
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
