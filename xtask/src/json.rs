// Written for solidcorex. Apache-2.0.
//! The least JSON `tests` needs: read the binary lists `cargo nextest list --list-type
//! binaries-only --message-format json` prints, merge them and write the result back, so one
//! `cargo nextest run --binaries-metadata` runs the test binaries of several cargo builds in one
//! pool. Numbers are kept as their text; objects keep their order.

use std::fmt::Write as _;

#[derive(Clone, Debug, PartialEq)]
pub enum Value {
    Null,
    Bool(bool),
    /// A number, as written.
    Num(String),
    Str(String),
    Arr(Vec<Value>),
    Obj(Vec<(String, Value)>),
}

impl Value {
    /// The member `key` of an object.
    pub fn get(&self, key: &str) -> Option<&Value> {
        match self {
            Value::Obj(m) => m.iter().find(|(k, _)| k == key).map(|(_, v)| v),
            _ => None,
        }
    }

    /// The compact text of the value.
    pub fn write(&self) -> String {
        let mut s = String::new();
        self.write_to(&mut s);
        s
    }

    fn write_to(&self, s: &mut String) {
        match self {
            Value::Null => s.push_str("null"),
            Value::Bool(b) => s.push_str(if *b { "true" } else { "false" }),
            Value::Num(n) => s.push_str(n),
            Value::Str(t) => write_str(s, t),
            Value::Arr(a) => {
                s.push('[');
                for (i, v) in a.iter().enumerate() {
                    if i > 0 {
                        s.push(',');
                    }
                    v.write_to(s);
                }
                s.push(']');
            }
            Value::Obj(m) => {
                s.push('{');
                for (i, (k, v)) in m.iter().enumerate() {
                    if i > 0 {
                        s.push(',');
                    }
                    write_str(s, k);
                    s.push(':');
                    v.write_to(s);
                }
                s.push('}');
            }
        }
    }
}

fn write_str(s: &mut String, t: &str) {
    s.push('"');
    for c in t.chars() {
        match c {
            '"' => s.push_str("\\\""),
            '\\' => s.push_str("\\\\"),
            '\n' => s.push_str("\\n"),
            '\r' => s.push_str("\\r"),
            '\t' => s.push_str("\\t"),
            c if (c as u32) < 0x20 => {
                let _ = write!(s, "\\u{:04x}", c as u32);
            }
            c => s.push(c),
        }
    }
    s.push('"');
}

/// Parses one JSON value (surrounding white space allowed).
pub fn parse(text: &str) -> Result<Value, String> {
    let mut p = Parser {
        s: text.as_bytes(),
        i: 0,
    };
    let v = p.value()?;
    p.ws();
    if p.i == p.s.len() {
        Ok(v)
    } else {
        Err(format!("json: trailing text at byte {}", p.i))
    }
}

struct Parser<'a> {
    s: &'a [u8],
    i: usize,
}

impl Parser<'_> {
    fn peek(&self) -> Option<u8> {
        self.s.get(self.i).copied()
    }

    fn ws(&mut self) {
        while matches!(self.peek(), Some(b' ' | b'\n' | b'\r' | b'\t')) {
            self.i += 1;
        }
    }

    fn err<T>(&self, what: &str) -> Result<T, String> {
        Err(format!("json: {what} at byte {}", self.i))
    }

    fn eat(&mut self, word: &str) -> bool {
        if self.s.get(self.i..self.i + word.len()) == Some(word.as_bytes()) {
            self.i += word.len();
            true
        } else {
            false
        }
    }

    fn value(&mut self) -> Result<Value, String> {
        self.ws();
        match self.peek() {
            Some(b'{') => {
                self.i += 1;
                let mut m = Vec::new();
                self.ws();
                if self.peek() == Some(b'}') {
                    self.i += 1;
                    return Ok(Value::Obj(m));
                }
                loop {
                    self.ws();
                    let k = self.string()?;
                    self.ws();
                    if !self.eat(":") {
                        return self.err("expected `:`");
                    }
                    let v = self.value()?;
                    m.push((k, v));
                    self.ws();
                    if self.eat(",") {
                        continue;
                    }
                    if self.eat("}") {
                        return Ok(Value::Obj(m));
                    }
                    return self.err("expected `,` or `}`");
                }
            }
            Some(b'[') => {
                self.i += 1;
                let mut a = Vec::new();
                self.ws();
                if self.peek() == Some(b']') {
                    self.i += 1;
                    return Ok(Value::Arr(a));
                }
                loop {
                    a.push(self.value()?);
                    self.ws();
                    if self.eat(",") {
                        continue;
                    }
                    if self.eat("]") {
                        return Ok(Value::Arr(a));
                    }
                    return self.err("expected `,` or `]`");
                }
            }
            Some(b'"') => Ok(Value::Str(self.string()?)),
            Some(b't') if self.eat("true") => Ok(Value::Bool(true)),
            Some(b'f') if self.eat("false") => Ok(Value::Bool(false)),
            Some(b'n') if self.eat("null") => Ok(Value::Null),
            Some(c) if c == b'-' || c.is_ascii_digit() => {
                let start = self.i;
                while matches!(
                    self.peek(),
                    Some(b'-' | b'+' | b'.' | b'e' | b'E' | b'0'..=b'9')
                ) {
                    self.i += 1;
                }
                let t = std::str::from_utf8(self.s.get(start..self.i).unwrap_or(&[]))
                    .map_err(|e| e.to_string())?;
                Ok(Value::Num(t.to_string()))
            }
            _ => self.err("expected a value"),
        }
    }

    fn string(&mut self) -> Result<String, String> {
        if !self.eat("\"") {
            return self.err("expected a string");
        }
        let mut out: Vec<u8> = Vec::new();
        loop {
            let Some(c) = self.peek() else {
                return self.err("unterminated string");
            };
            self.i += 1;
            match c {
                b'"' => return String::from_utf8(out).map_err(|e| e.to_string()),
                b'\\' => {
                    let Some(e) = self.peek() else {
                        return self.err("unterminated escape");
                    };
                    self.i += 1;
                    match e {
                        b'"' | b'\\' | b'/' => out.push(e),
                        b'n' => out.push(b'\n'),
                        b'r' => out.push(b'\r'),
                        b't' => out.push(b'\t'),
                        b'b' => out.push(8),
                        b'f' => out.push(12),
                        b'u' => {
                            let hex = |p: &mut Self| -> Result<u32, String> {
                                let h =
                                    p.s.get(p.i..p.i + 4)
                                        .and_then(|h| std::str::from_utf8(h).ok())
                                        .and_then(|h| u32::from_str_radix(h, 16).ok());
                                p.i += 4;
                                h.ok_or_else(|| format!("json: bad \\u escape at byte {}", p.i))
                            };
                            let mut cp = hex(self)?;
                            if (0xD800..0xDC00).contains(&cp) && self.eat("\\u") {
                                let lo = hex(self)?;
                                cp = 0x10000 + ((cp - 0xD800) << 10) + (lo.wrapping_sub(0xDC00));
                            }
                            let ch = char::from_u32(cp).unwrap_or('\u{fffd}');
                            let mut buf = [0u8; 4];
                            out.extend_from_slice(ch.encode_utf8(&mut buf).as_bytes());
                        }
                        _ => return self.err("bad escape"),
                    }
                }
                c => out.push(c),
            }
        }
    }
}

/// Merges `b` into `a`: objects member by member (a member only in `b` is appended; one in both
/// is merged), arrays by appending the elements `a` lacks, and any other value is kept from `a`
/// when both hold one. The binary lists of two builds over one target directory merge this way
/// into the list of both: their binaries, non-test binaries, build-script output directories and
/// linked paths, with the shared build facts (target directory, platforms) taken once.
pub fn merge(a: &mut Value, b: Value) {
    match (a, b) {
        (Value::Obj(am), Value::Obj(bm)) => {
            for (k, bv) in bm {
                if let Some((_, av)) = am.iter_mut().find(|(ak, _)| *ak == k) {
                    merge(av, bv);
                } else {
                    am.push((k, bv));
                }
            }
        }
        (Value::Arr(aa), Value::Arr(ba)) => {
            for v in ba {
                if !aa.contains(&v) {
                    aa.push(v);
                }
            }
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_and_merge() {
        let mut a = parse(r#"{"m": {"x": [1, "a\"b"], "k": true}, "n": null, "u": "é"}"#)
            .unwrap_or(Value::Null);
        assert_eq!(
            a.write(),
            r#"{"m":{"x":[1,"a\"b"],"k":true},"n":null,"u":"é"}"#
        );
        let b = parse(r#"{"m": {"x": [1, 2], "y": {}}, "n": 3}"#).unwrap_or(Value::Null);
        merge(&mut a, b);
        assert_eq!(
            a.write(),
            r#"{"m":{"x":[1,"a\"b",2],"k":true,"y":{}},"n":null,"u":"é"}"#
        );
        assert!(parse("[1,").is_err());
        assert!(parse("{} x").is_err());
    }
}
