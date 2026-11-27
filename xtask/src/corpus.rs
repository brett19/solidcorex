// Written for solidcorex. Apache-2.0.
//! `cargo xtask corpus fetch [prefix ...]` and `cargo xtask corpus verify [prefix ...]`: the
//! STEP corpus of research 74 §7, which is fetched at test time and never committed.
//!
//! `corpus/manifest.tsv` names every file (`name`, `url`, `sha256`, `size`, `licence`, `part`).
//! The files go under the directory `SCX_CORPUS_CACHE` names, at their manifest names, and
//! every one is checked against its SHA-256 and size before it takes its place. A file already
//! there with another hash is an error (remove it and run again); nothing is overwritten and
//! nothing is deleted. Downloads are data: they are written, hashed and left alone, and the
//! one archive is read by a Python `zipfile` call that extracts the exact member the manifest
//! names (`python3 -I`, so nothing is imported from beside the archive).
//!
//! A prefix selects the rows whose name starts with it (`nist-pmi`, `dan335`, `solidcore`,
//! `mfcad`); none selects every row. Selecting a member also selects its archive.
//!
//! The reader of this manifest in `scx-testkit` is a second copy of the parse: a tool crate
//! depends on no workspace crate (`layers.txt`).

use std::fmt::Write as _;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

/// The environment variable that names the cache directory.
const CACHE_VAR: &str = "SCX_CORPUS_CACHE";

#[derive(Clone, Debug, PartialEq, Eq)]
enum Part {
    File,
    Archive,
    Member(String),
}

#[derive(Clone, Debug)]
struct Row {
    name: String,
    url: String,
    sha256: String,
    size: u64,
    part: Part,
}

fn parse_manifest(text: &str) -> Result<Vec<Row>, String> {
    let mut rows = Vec::new();
    for (i, line) in text.lines().enumerate().skip(1) {
        if line.trim().is_empty() || line.starts_with('#') {
            continue;
        }
        let at = |what: &str| format!("manifest line {}: {what}", i + 1);
        let f: Vec<&str> = line.split('\t').collect();
        let &[name, url, sha256, size, _licence, part] = f.as_slice() else {
            return Err(at("not six tab-separated fields"));
        };
        if name.is_empty() || name.starts_with('/') || name.split('/').any(|c| c == "..") {
            return Err(at("a name must be a relative path without .."));
        }
        if !url.starts_with("https://") {
            return Err(at("a url must be https"));
        }
        if sha256.len() != 64
            || !sha256
                .bytes()
                .all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
        {
            return Err(at("sha256 is not 64 lower-case hex digits"));
        }
        let part = match part {
            "file" => Part::File,
            "archive" => Part::Archive,
            p => match p.strip_prefix("member:") {
                Some(a) if !a.is_empty() => Part::Member(a.to_string()),
                _ => return Err(at("part is not file, archive or member:<archive>")),
            },
        };
        rows.push(Row {
            name: name.to_string(),
            url: url.to_string(),
            sha256: sha256.to_string(),
            size: size.parse().map_err(|_| at("size is not a number"))?,
            part,
        });
    }
    Ok(rows)
}

/// The rows a prefix list selects, with each selected member's archive, in manifest order.
fn select<'a>(rows: &'a [Row], prefixes: &[String]) -> Vec<&'a Row> {
    let chosen = |r: &Row| prefixes.is_empty() || prefixes.iter().any(|p| r.name.starts_with(p));
    let wanted_archives: Vec<&str> = rows
        .iter()
        .filter(|r| chosen(r))
        .filter_map(|r| match &r.part {
            Part::Member(a) => Some(a.as_str()),
            _ => None,
        })
        .collect();
    rows.iter()
        .filter(|r| chosen(r) || wanted_archives.contains(&r.name.as_str()))
        .collect()
}

// ---- SHA-256 (FIPS 180-4) ------------------------------------------------------------

const K: [u32; 64] = [
    0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4, 0xab1c5ed5,
    0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe, 0x9bdc06a7, 0xc19bf174,
    0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f, 0x4a7484aa, 0x5cb0a9dc, 0x76f988da,
    0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7, 0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967,
    0x27b70a85, 0x2e1b2138, 0x4d2c6dfc, 0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85,
    0xa2bfe8a1, 0xa81a664b, 0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070,
    0x19a4c116, 0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
    0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7, 0xc67178f2,
];

struct Sha256 {
    h: [u32; 8],
    buf: Vec<u8>,
    len: u64,
}

impl Sha256 {
    fn new() -> Self {
        Self {
            h: [
                0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab,
                0x5be0cd19,
            ],
            buf: Vec::with_capacity(64),
            len: 0,
        }
    }

    fn block(h: &mut [u32; 8], b: &[u8]) {
        let mut w = [0u32; 64];
        for (wi, c) in w.iter_mut().zip(b.as_chunks::<4>().0) {
            *wi = u32::from_be_bytes(*c);
        }
        for i in 16..64 {
            let g = |k: usize| w.get(i - k).copied().unwrap_or(0);
            let s0 = g(15).rotate_right(7) ^ g(15).rotate_right(18) ^ (g(15) >> 3);
            let s1 = g(2).rotate_right(17) ^ g(2).rotate_right(19) ^ (g(2) >> 10);
            let v = g(16).wrapping_add(s0).wrapping_add(g(7)).wrapping_add(s1);
            if let Some(slot) = w.get_mut(i) {
                *slot = v;
            }
        }
        let mut v = *h;
        for (k, wi) in K.iter().zip(w.iter()) {
            let [a, b, c, d, e, f, g, hh] = v;
            let s1 = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
            let ch = (e & f) ^ (!e & g);
            let t1 = hh
                .wrapping_add(s1)
                .wrapping_add(ch)
                .wrapping_add(*k)
                .wrapping_add(*wi);
            let s0 = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
            let maj = (a & b) ^ (a & c) ^ (b & c);
            let t2 = s0.wrapping_add(maj);
            v = [t1.wrapping_add(t2), a, b, c, d.wrapping_add(t1), e, f, g];
        }
        for (x, y) in h.iter_mut().zip(v) {
            *x = x.wrapping_add(y);
        }
    }

    fn update(&mut self, mut data: &[u8]) {
        self.len += data.len() as u64;
        while !data.is_empty() {
            let take = (64 - self.buf.len()).min(data.len());
            let (a, rest) = data.split_at(take);
            self.buf.extend_from_slice(a);
            data = rest;
            if self.buf.len() == 64 {
                Self::block(&mut self.h, &self.buf);
                self.buf.clear();
            }
        }
    }

    fn finish(mut self) -> String {
        let bits = self.len * 8;
        self.update(&[0x80]);
        while self.buf.len() != 56 {
            self.update(&[0]);
        }
        self.update(&bits.to_be_bytes());
        self.h.iter().fold(String::new(), |mut s, x| {
            let _ = write!(s, "{x:08x}");
            s
        })
    }
}

/// The SHA-256 of a file and its size, read in blocks.
fn hash_file(path: &Path) -> Result<(String, u64), String> {
    use std::io::Read;
    let mut f = fs::File::open(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let mut h = Sha256::new();
    let mut buf = vec![0u8; 1 << 16];
    let mut n = 0u64;
    loop {
        let k = f
            .read(&mut buf)
            .map_err(|e| format!("{}: {e}", path.display()))?;
        if k == 0 {
            break;
        }
        h.update(buf.get(..k).unwrap_or_default());
        n += k as u64;
    }
    Ok((h.finish(), n))
}

// ---- fetch and verify ----------------------------------------------------------------

fn cache_dir() -> Result<PathBuf, String> {
    std::env::var_os(CACHE_VAR)
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
        .ok_or_else(|| format!("{CACHE_VAR} must name the directory the corpus is stored in"))
}

fn load(root: &Path) -> Result<Vec<Row>, String> {
    let p = root.join("corpus/manifest.tsv");
    parse_manifest(&fs::read_to_string(&p).map_err(|e| format!("{}: {e}", p.display()))?)
}

/// What a file in the cache is: absent, as the manifest says, or something else.
enum State {
    Absent,
    Good,
    Wrong(String),
}

fn state(cache: &Path, r: &Row) -> Result<State, String> {
    let p = cache.join(&r.name);
    if !p.exists() {
        return Ok(State::Absent);
    }
    let (sha, n) = hash_file(&p)?;
    Ok(if sha == r.sha256 && n == r.size {
        State::Good
    } else {
        State::Wrong(format!(
            "sha256 {sha}, {n} bytes; the manifest has {}, {}",
            r.sha256, r.size
        ))
    })
}

fn check_part(part: &Path, r: &Row) -> Result<(), String> {
    let (sha, n) = hash_file(part)?;
    if sha != r.sha256 || n != r.size {
        return Err(format!(
            "{}: got sha256 {sha}, {n} bytes; the manifest has {}, {} (left as {})",
            r.name,
            r.sha256,
            r.size,
            part.display()
        ));
    }
    Ok(())
}

fn download(r: &Row, part: &Path) -> Result<(), String> {
    let status = Command::new("curl")
        .args([
            "--fail",
            "--silent",
            "--show-error",
            "--location",
            "--proto",
            "=https",
        ])
        .args(["--max-time", "900", "--output"])
        .arg(part)
        .arg(&r.url)
        .status()
        .map_err(|e| format!("curl: {e}"))?;
    if status.success() {
        Ok(())
    } else {
        Err(format!(
            "{}: download of {} failed ({status})",
            r.name, r.url
        ))
    }
}

/// Writes the exact member of the archive to `part`, at most `size + 1` bytes of it.
fn extract(archive: &Path, member: &str, size: u64, part: &Path) -> Result<(), String> {
    const SCRIPT: &str = "import sys, zipfile\n\
        a, m, o, n = sys.argv[1:5]\n\
        with zipfile.ZipFile(a) as z, z.open(m) as f:\n    d = f.read(int(n) + 1)\n\
        open(o, 'wb').write(d)\n";
    let status = Command::new("python3")
        .args(["-I", "-c", SCRIPT])
        .arg(archive)
        .arg(member)
        .arg(part)
        .arg(size.to_string())
        .status()
        .map_err(|e| format!("python3: {e}"))?;
    if status.success() {
        Ok(())
    } else {
        Err(format!(
            "{member}: not extracted from {} ({status})",
            archive.display()
        ))
    }
}

fn fetch(root: &Path, prefixes: &[String]) -> Result<String, String> {
    let cache = cache_dir()?;
    let rows = load(root)?;
    let (mut got, mut had) = (0usize, 0usize);
    for r in select(&rows, prefixes) {
        match state(&cache, r)? {
            State::Good => {
                had += 1;
                continue;
            }
            State::Wrong(w) => {
                return Err(format!(
                    "{}: exists with another content ({w}); remove it and run again",
                    r.name
                ));
            }
            State::Absent => {}
        }
        let out = cache.join(&r.name);
        if let Some(dir) = out.parent() {
            fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
        }
        let part = out.with_file_name(format!(
            "{}.part",
            out.file_name().and_then(|n| n.to_str()).unwrap_or("file")
        ));
        match &r.part {
            Part::File | Part::Archive => download(r, &part)?,
            Part::Member(a) => {
                // The member path is the name below the archive's first component.
                let top = a.split('/').next().unwrap_or_default();
                let member = r
                    .name
                    .strip_prefix(top)
                    .and_then(|m| m.strip_prefix('/'))
                    .filter(|m| !top.is_empty() && !m.is_empty())
                    .ok_or_else(|| format!("{}: not under its archive's directory {top}", r.name))?
                    .to_string();
                extract(&cache.join(a), &member, r.size, &part)?;
            }
        }
        check_part(&part, r)?;
        fs::rename(&part, &out).map_err(|e| format!("{}: {e}", out.display()))?;
        got += 1;
    }
    Ok(format!(
        "corpus in {}: {got} fetched, {had} already present",
        cache.display()
    ))
}

fn verify(root: &Path, prefixes: &[String]) -> Result<String, String> {
    let cache = cache_dir()?;
    let rows = load(root)?;
    let (mut good, mut missing, mut bad) = (0usize, Vec::new(), Vec::new());
    for r in select(&rows, prefixes) {
        match state(&cache, r)? {
            State::Good => good += 1,
            State::Absent => missing.push(r.name.clone()),
            State::Wrong(w) => bad.push(format!("{}: {w}", r.name)),
        }
    }
    if bad.is_empty() && missing.is_empty() {
        return Ok(format!("corpus verified: {good} files match the manifest"));
    }
    Err(format!(
        "{good} files match; {} missing {:?}; {} differ {:?}",
        missing.len(),
        missing,
        bad.len(),
        bad
    ))
}

/// `cargo xtask corpus <fetch | verify> [prefix ...]`.
pub fn run(root: &Path, args: &[String]) -> Result<String, String> {
    match args.first().map(String::as_str) {
        Some("fetch") => fetch(root, args.get(1..).unwrap_or_default()),
        Some("verify") => verify(root, args.get(1..).unwrap_or_default()),
        _ => Err("usage: cargo xtask corpus <fetch | verify> [name-prefix ...] (SCX_CORPUS_CACHE names the directory)".to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sha(data: &[u8]) -> String {
        let mut h = Sha256::new();
        // Odd chunk sizes: the block buffer must not care where the splits fall.
        for c in data.chunks(7) {
            h.update(c);
        }
        h.finish()
    }

    #[test]
    fn sha256_vectors() {
        assert_eq!(
            sha(b""),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
        assert_eq!(
            sha(b"abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
        assert_eq!(
            sha(b"abcdbcdecdefdefgefghfghighijhijkijkljklmklmnlmnomnopnopq"),
            "248d6a61d20638b8e5c026930c3e6039a33ce45964ff2167f6ecedd419db06c1"
        );
        assert_eq!(
            sha(&vec![b'a'; 1_000_000]),
            "cdc76e5c9914fb9281a1c7e284d73e67f1809a48a497200e046d39ccc7112cd0"
        );
    }

    #[test]
    fn committed_manifest_parses_and_selects() {
        let rows = parse_manifest(include_str!("../../corpus/manifest.tsv")).unwrap();
        assert!(rows.len() > 60);
        let nist = select(
            &rows,
            &["nist-pmi/NIST-PMI-STEP-Files/nist_ctc_01".to_string()],
        );
        assert_eq!(nist.len(), 2, "the member and its archive");
        assert_eq!(select(&rows, &[]).len(), rows.len());
        assert!(rows.iter().all(|r| r.size > 0));
    }

    #[test]
    fn manifest_refusals() {
        let h = "name\turl\tsha256\tsize\tlicence\tpart\n";
        let s = "0".repeat(64);
        assert!(parse_manifest(&format!("{h}a/b\thttps://x\t{s}\t1\tMIT\tfile\n")).is_ok());
        for bad in [
            format!("{h}../b\thttps://x\t{s}\t1\tMIT\tfile\n"),
            format!("{h}a\thttp://x\t{s}\t1\tMIT\tfile\n"),
            format!("{h}a\thttps://x\t{s}\t1\tMIT\tmember:\n"),
            format!("{h}a\thttps://x\tAB\t1\tMIT\tfile\n"),
        ] {
            assert!(parse_manifest(&bad).is_err(), "{bad}");
        }
    }
}
