// Written for solidcorex. Apache-2.0.
//! The STEP corpus's truth (research 74 §7, §14): the manifest of files fetched at test time,
//! and the generator of `corpus/references.tsv`, the volumes known from formulas.
//!
//! Nothing here reads a geometry library. The volumes come from the closed form of each part's
//! `.truth.json` (the step-conformance generator's description of its own parts: solids of
//! revolution as a closed profile of segments and arcs, and prisms as loops with holes), by
//! Green's theorem on the profile, so a reader under test never judges itself.
//!
//! The truth files are untrusted data: the JSON reader below has a depth limit, takes only
//! what it needs, and a file that does not describe a closed profile is an error, not a
//! guess.

use std::fmt::{self, Write as _};

use scx_num::fmath::{cos, hypot, sin};

/// Why a manifest or a truth file was refused.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum CorpusError {
    /// A manifest line that is not six tab-separated fields, or has a field that cannot be read.
    Manifest {
        /// The 1-based line.
        line: usize,
        /// What is wrong.
        what: String,
    },
    /// Not JSON, or nested past the reader's limit.
    Json {
        /// Byte offset.
        at: usize,
        /// What is wrong.
        what: &'static str,
    },
    /// JSON that is not a truth file.
    Truth(String),
    /// A profile or loop that does not close within the stated tolerance.
    Open {
        /// The distance between the end and the start.
        gap: f64,
    },
}

impl fmt::Display for CorpusError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Manifest { line, what } => write!(f, "manifest line {line}: {what}"),
            Self::Json { at, what } => write!(f, "json at byte {at}: {what}"),
            Self::Truth(what) => write!(f, "truth file: {what}"),
            Self::Open { gap } => write!(f, "profile open by {gap}"),
        }
    }
}

impl std::error::Error for CorpusError {}

// ---- the manifest --------------------------------------------------------------------

/// How a manifest row's file is obtained.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Part {
    /// Downloaded from its URL.
    File,
    /// Downloaded from its URL and kept as an archive; not itself a corpus file.
    Archive,
    /// Extracted from the named archive row; the member path is the row's name below the
    /// archive's first path component.
    Member {
        /// The archive row's name.
        archive: String,
    },
}

/// One row of `corpus/manifest.tsv`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Row {
    /// The path under the cache directory.
    pub name: String,
    /// Where the bytes come from.
    pub url: String,
    /// Lower-case hex SHA-256 of the file.
    pub sha256: String,
    /// The file's size in bytes.
    pub size: u64,
    /// The licence, with its evidence named in research 74 §14.
    pub licence: String,
    /// How it is obtained.
    pub part: Part,
}

/// The rows of a manifest: a header line, then one line per file with the fields `name`, `url`,
/// `sha256`, `size`, `licence`, `part` (`file`, `archive`, or `member:<archive row name>`).
pub fn parse_manifest(text: &str) -> Result<Vec<Row>, CorpusError> {
    let mut rows = Vec::new();
    for (i, line) in text.lines().enumerate().skip(1) {
        let bad = |what: &str| CorpusError::Manifest {
            line: i + 1,
            what: what.to_string(),
        };
        if line.trim().is_empty() || line.starts_with('#') {
            continue;
        }
        let f: Vec<&str> = line.split('\t').collect();
        let &[name, url, sha256, size, licence, part] = f.as_slice() else {
            return Err(bad("not six tab-separated fields"));
        };
        if name.is_empty() || name.starts_with('/') || name.split('/').any(|c| c == "..") {
            return Err(bad("a name must be a relative path without .."));
        }
        if sha256.len() != 64
            || !sha256
                .bytes()
                .all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
        {
            return Err(bad("sha256 is not 64 lower-case hex digits"));
        }
        let size = size
            .parse::<u64>()
            .map_err(|_| bad("size is not a number"))?;
        let part = match part {
            "file" => Part::File,
            "archive" => Part::Archive,
            p => match p.strip_prefix("member:") {
                Some(a) if !a.is_empty() => Part::Member {
                    archive: a.to_string(),
                },
                _ => return Err(bad("part is not file, archive or member:<archive>")),
            },
        };
        rows.push(Row {
            name: name.to_string(),
            url: url.to_string(),
            sha256: sha256.to_string(),
            size,
            licence: licence.to_string(),
            part,
        });
    }
    Ok(rows)
}

// ---- a small JSON reader -------------------------------------------------------------

#[derive(Clone, Debug, PartialEq)]
enum Json {
    Null,
    Bool(bool),
    Num(f64),
    Str(String),
    Arr(Vec<Json>),
    Obj(Vec<(String, Json)>),
}

impl Json {
    fn get(&self, key: &str) -> Option<&Json> {
        match self {
            Self::Obj(m) => m.iter().find(|(k, _)| k == key).map(|(_, v)| v),
            _ => None,
        }
    }

    fn num(&self) -> Option<f64> {
        match self {
            Self::Num(x) => Some(*x),
            _ => None,
        }
    }

    fn arr(&self) -> Option<&[Json]> {
        match self {
            Self::Arr(a) => Some(a),
            _ => None,
        }
    }
}

/// Deeper than any truth file; a bound on the recursion for hostile input.
const MAX_DEPTH: usize = 32;

struct Reader<'a> {
    s: &'a [u8],
    at: usize,
}

impl Reader<'_> {
    fn err<T>(&self, what: &'static str) -> Result<T, CorpusError> {
        Err(CorpusError::Json { at: self.at, what })
    }

    fn ws(&mut self) {
        while self.s.get(self.at).is_some_and(u8::is_ascii_whitespace) {
            self.at += 1;
        }
    }

    fn eat(&mut self, b: u8) -> bool {
        if self.s.get(self.at) == Some(&b) {
            self.at += 1;
            true
        } else {
            false
        }
    }

    fn word(&mut self, w: &'static str, v: Json) -> Result<Json, CorpusError> {
        if self.s.get(self.at..self.at + w.len()) == Some(w.as_bytes()) {
            self.at += w.len();
            Ok(v)
        } else {
            self.err("unknown word")
        }
    }

    fn string(&mut self) -> Result<String, CorpusError> {
        if !self.eat(b'"') {
            return self.err("a string was expected");
        }
        let start = self.at;
        while let Some(&b) = self.s.get(self.at) {
            match b {
                b'"' => {
                    let raw = self.s.get(start..self.at).unwrap_or_default();
                    self.at += 1;
                    return Ok(String::from_utf8_lossy(raw).into_owned());
                }
                // Truth files hold no escapes; refusing them keeps the reader exact.
                b'\\' => return self.err("a string escape is not supported"),
                _ => self.at += 1,
            }
        }
        self.err("an unterminated string")
    }

    fn value(&mut self, depth: usize) -> Result<Json, CorpusError> {
        if depth > MAX_DEPTH {
            return self.err("nested too deeply");
        }
        self.ws();
        match self.s.get(self.at) {
            Some(b'{') => {
                self.at += 1;
                let mut m = Vec::new();
                self.ws();
                if self.eat(b'}') {
                    return Ok(Json::Obj(m));
                }
                loop {
                    self.ws();
                    let k = self.string()?;
                    self.ws();
                    if !self.eat(b':') {
                        return self.err("a colon was expected");
                    }
                    m.push((k, self.value(depth + 1)?));
                    self.ws();
                    if self.eat(b',') {
                        continue;
                    }
                    if self.eat(b'}') {
                        return Ok(Json::Obj(m));
                    }
                    return self.err("a comma or } was expected");
                }
            }
            Some(b'[') => {
                self.at += 1;
                let mut a = Vec::new();
                self.ws();
                if self.eat(b']') {
                    return Ok(Json::Arr(a));
                }
                loop {
                    a.push(self.value(depth + 1)?);
                    self.ws();
                    if self.eat(b',') {
                        continue;
                    }
                    if self.eat(b']') {
                        return Ok(Json::Arr(a));
                    }
                    return self.err("a comma or ] was expected");
                }
            }
            Some(b'"') => Ok(Json::Str(self.string()?)),
            Some(b't') => self.word("true", Json::Bool(true)),
            Some(b'f') => self.word("false", Json::Bool(false)),
            Some(b'n') => self.word("null", Json::Null),
            Some(b'-' | b'0'..=b'9') => {
                let start = self.at;
                while self
                    .s
                    .get(self.at)
                    .is_some_and(|b| matches!(b, b'-' | b'+' | b'.' | b'e' | b'E' | b'0'..=b'9'))
                {
                    self.at += 1;
                }
                let text = self
                    .s
                    .get(start..self.at)
                    .and_then(|b| std::str::from_utf8(b).ok())
                    .unwrap_or_default();
                match text.parse::<f64>() {
                    Ok(x) if x.is_finite() => Ok(Json::Num(x)),
                    _ => self.err("a number was expected"),
                }
            }
            _ => self.err("a value was expected"),
        }
    }
}

fn parse_json(text: &str) -> Result<Json, CorpusError> {
    let mut r = Reader {
        s: text.as_bytes(),
        at: 0,
    };
    let v = r.value(0)?;
    r.ws();
    if r.at != r.s.len() {
        return r.err("text after the value");
    }
    Ok(v)
}

// ---- the closed forms ----------------------------------------------------------------

/// One piece of a profile or a loop, in the plane's own two coordinates.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Piece {
    Segment {
        from: [f64; 2],
        to: [f64; 2],
    },
    Arc {
        center: [f64; 2],
        radius: f64,
        start: f64,
        sweep: f64,
    },
}

impl Piece {
    fn begin(&self) -> [f64; 2] {
        match *self {
            Self::Segment { from, .. } => from,
            Self::Arc {
                center,
                radius,
                start,
                ..
            } => [
                center[0] + radius * cos(start),
                center[1] + radius * sin(start),
            ],
        }
    }

    fn end(&self) -> [f64; 2] {
        match *self {
            Self::Segment { to, .. } => to,
            Self::Arc {
                center,
                radius,
                start,
                sweep,
            } => {
                let t = start + sweep;
                [center[0] + radius * cos(t), center[1] + radius * sin(t)]
            }
        }
    }
}

fn point(j: &Json, what: &str) -> Result<[f64; 2], CorpusError> {
    match j.arr().map(|a| a.iter().map(Json::num).collect::<Vec<_>>()) {
        Some(v) => match v.as_slice() {
            &[Some(x), Some(y)] => Ok([x, y]),
            _ => Err(CorpusError::Truth(format!("{what}: not a 2-D point"))),
        },
        None => Err(CorpusError::Truth(format!("{what}: not a 2-D point"))),
    }
}

fn number(j: Option<&Json>, what: &str) -> Result<f64, CorpusError> {
    j.and_then(Json::num)
        .ok_or_else(|| CorpusError::Truth(format!("{what}: not a number")))
}

fn piece(j: &Json) -> Result<Piece, CorpusError> {
    if let Some(s) = j.get("segment") {
        let from = point(s.get("from").unwrap_or(&Json::Null), "segment.from")?;
        let to = point(s.get("to").unwrap_or(&Json::Null), "segment.to")?;
        return Ok(Piece::Segment { from, to });
    }
    if let Some(a) = j.get("arc") {
        return Ok(Piece::Arc {
            center: point(a.get("center").unwrap_or(&Json::Null), "arc.center")?,
            radius: number(a.get("radius"), "arc.radius")?,
            start: number(a.get("start"), "arc.start")?,
            sweep: number(a.get("sweep"), "arc.sweep")?,
        });
    }
    Err(CorpusError::Truth(
        "a piece is neither a segment nor an arc".to_string(),
    ))
}

fn pieces(j: Option<&Json>, what: &str) -> Result<Vec<Piece>, CorpusError> {
    let a = j
        .and_then(Json::arr)
        .ok_or_else(|| CorpusError::Truth(format!("{what}: not a list")))?;
    a.iter().map(piece).collect()
}

/// The pieces form one closed chain, each starting where the one before ended, to within
/// `1e-9` of the loop's largest coordinate.
fn check_closed(p: &[Piece]) -> Result<(), CorpusError> {
    let scale = p
        .iter()
        .flat_map(|q| [q.begin(), q.end()])
        .flatten()
        .fold(1.0_f64, |m, x| m.max(x.abs()));
    let tol = 1e-9 * scale;
    let mut prev = p.last().map(Piece::end);
    for q in p {
        if let Some(e) = prev {
            let b = q.begin();
            let gap = hypot(e[0] - b[0], e[1] - b[1]);
            if gap > tol {
                return Err(CorpusError::Open { gap });
            }
        }
        prev = Some(q.end());
    }
    Ok(())
}

/// `∮ r² da` along the pieces of a profile in (axis, radius) coordinates.
fn revolve_integral(p: &[Piece]) -> f64 {
    let mut s = 0.0;
    for q in p {
        match *q {
            Piece::Segment { from, to } => {
                let (a0, r0, a1, r1) = (from[0], from[1], to[0], to[1]);
                s += (a1 - a0) * (r0 * r0 + r0 * r1 + r1 * r1) / 3.0;
            }
            Piece::Arc {
                center,
                radius,
                start,
                sweep,
            } => {
                // a = ca + R cos t, r = cr + R sin t, da = -R sin t dt.
                let cr = center[1];
                let big = |t: f64| {
                    let cs = cos(t);
                    -radius
                        * (cr * cr * -cs
                            + 2.0 * cr * radius * (t / 2.0 - sin(2.0 * t) / 4.0)
                            + radius * radius * (-cs + cs * cs * cs / 3.0))
                };
                s += big(start + sweep) - big(start);
            }
        }
    }
    s
}

/// The signed area `½ ∮ (x dy - y dx)` of a closed loop of pieces.
fn loop_area(p: &[Piece]) -> f64 {
    let mut s = 0.0;
    for q in p {
        match *q {
            Piece::Segment { from, to } => s += (from[0] * to[1] - to[0] * from[1]) / 2.0,
            Piece::Arc {
                center,
                radius,
                start,
                sweep,
            } => {
                let t1 = start + sweep;
                s += 0.5
                    * (radius * radius * sweep
                        + radius
                            * (center[0] * (sin(t1) - sin(start))
                                - center[1] * (cos(t1) - cos(start))));
            }
        }
    }
    s
}

/// The volume in cubic millimetres a step-conformance `.truth.json` describes, the sum of its
/// solids of revolution (`pi |∮ r² da|` of the closed profile, in (axis, radius) coordinates)
/// and its prisms (the height times the area of the largest loop less the areas of the others,
/// which are its holes). `None` when the file describes neither (the freeform panel's truth
/// is surfaces, not a volume).
///
/// Accuracy: each piece is a closed form, so the result is within about 1e-13 of the exact
/// value relative to the part's bounding volume; the tests check it against the cylinder,
/// sphere and torus closed forms.
pub fn truth_volume(json: &str) -> Result<Option<f64>, CorpusError> {
    let root = parse_json(json)?;
    let mut total = 0.0;
    let mut any = false;
    for s in root.get("solids").and_then(Json::arr).unwrap_or_default() {
        let p = pieces(s.get("profile"), "solid.profile")?;
        check_closed(&p)?;
        total += (std::f64::consts::PI * revolve_integral(&p)).abs();
        any = true;
    }
    for pr in root.get("prisms").and_then(Json::arr).unwrap_or_default() {
        let height = number(pr.get("height"), "prism.height")?;
        let loops = pr
            .get("loops")
            .and_then(Json::arr)
            .ok_or_else(|| CorpusError::Truth("prism.loops: not a list".to_string()))?;
        let mut areas = Vec::new();
        for l in loops {
            let p = pieces(Some(l), "prism.loop")?;
            check_closed(&p)?;
            areas.push(loop_area(&p).abs());
        }
        let outer_at = areas
            .iter()
            .enumerate()
            .fold((0, 0.0_f64), |m, (i, &a)| if a > m.1 { (i, a) } else { m });
        let outer = outer_at.1;
        let holes: f64 = areas
            .iter()
            .enumerate()
            .filter(|&(i, _)| i != outer_at.0)
            .map(|(_, a)| a)
            .sum();
        total += height.abs() * (outer - holes);
        any = true;
    }
    Ok(any.then_some(total))
}

// ---- references.tsv ------------------------------------------------------------------

/// The header line of `corpus/references.tsv`: the shape of the pin's `bench/step/references.tsv`
/// (SHA-256, volume in cubic millimetres, where it comes from).
pub const REFERENCES_HEADER: &str = "# sha256\tvolume_mm3\twhere the volume comes from";

/// The text of `corpus/references.tsv` for a manifest: one line per `X.step` row that has a
/// sibling `X.truth.json` row, in manifest order, the volume written to the shortest text that
/// reads back to the same `f64`. `read` gives a row's file text (the caller's cache).
pub fn references_tsv(
    rows: &[Row],
    mut read: impl FnMut(&Row) -> Result<String, CorpusError>,
) -> Result<String, CorpusError> {
    let mut out = format!("{REFERENCES_HEADER}\n");
    for r in rows {
        let Some(stem) = r.name.strip_suffix(".step") else {
            continue;
        };
        let truth_name = format!("{stem}.truth.json");
        let Some(t) = rows.iter().find(|t| t.name == truth_name) else {
            continue;
        };
        if let Some(v) = truth_volume(&read(t)?)? {
            let _ = writeln!(
                out,
                "{}\t{v}\t{} (closed form from its .truth.json)",
                r.sha256, r.name
            );
        }
    }
    Ok(out)
}
