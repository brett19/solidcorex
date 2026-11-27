// Written for solidcorex. Apache-2.0.
//! Just enough JSON for the corpus cases: parse a document and read numbers, strings,
//! arrays and objects out of it. (The standard library has no parser, and the libraries use
//! no external crate, layers.txt.)

pub enum Json {
    Null,
    Bool(bool),
    Num(f64),
    Str(String),
    Arr(Vec<Json>),
    Obj(Vec<(String, Json)>),
}

impl Json {
    pub fn get(&self, key: &str) -> Option<&Json> {
        match self {
            Json::Obj(f) => f.iter().find(|(k, _)| k == key).map(|(_, v)| v),
            _ => None,
        }
    }
    pub fn num(&self) -> Option<f64> {
        match self {
            Json::Num(x) => Some(*x),
            _ => None,
        }
    }
    pub fn str(&self) -> Option<&str> {
        match self {
            Json::Str(s) => Some(s),
            _ => None,
        }
    }
    pub fn arr(&self) -> Option<&[Json]> {
        match self {
            Json::Arr(a) => Some(a),
            _ => None,
        }
    }
    pub fn nums(&self) -> Option<Vec<f64>> {
        self.arr()?.iter().map(Json::num).collect()
    }
    pub fn is_truthy_bool(&self) -> bool {
        matches!(self, Json::Bool(true))
    }
    pub fn is_null(&self) -> bool {
        matches!(self, Json::Null)
    }
}

pub fn parse(text: &str) -> Result<Json, String> {
    let mut p = Parser {
        s: text.as_bytes(),
        i: 0,
    };
    let v = p.value()?;
    p.ws();
    if p.i != p.s.len() {
        return Err(format!("trailing text at byte {}", p.i));
    }
    Ok(v)
}

struct Parser<'a> {
    s: &'a [u8],
    i: usize,
}

impl Parser<'_> {
    fn ws(&mut self) {
        while self.s.get(self.i).is_some_and(u8::is_ascii_whitespace) {
            self.i += 1;
        }
    }
    fn peek(&self) -> Option<u8> {
        self.s.get(self.i).copied()
    }
    fn eat(&mut self, lit: &str) -> bool {
        if self
            .s
            .get(self.i..)
            .is_some_and(|r| r.starts_with(lit.as_bytes()))
        {
            self.i += lit.len();
            true
        } else {
            false
        }
    }
    fn value(&mut self) -> Result<Json, String> {
        self.ws();
        match self.peek() {
            Some(b'{') => self.object(),
            Some(b'[') => self.array(),
            Some(b'"') => Ok(Json::Str(self.string()?)),
            Some(_) => {
                for (lit, v) in [
                    ("null", Json::Null),
                    ("true", Json::Bool(true)),
                    ("false", Json::Bool(false)),
                    ("NaN", Json::Num(f64::NAN)),
                    ("Infinity", Json::Num(f64::INFINITY)),
                    ("-Infinity", Json::Num(f64::NEG_INFINITY)),
                ] {
                    if self.eat(lit) {
                        return Ok(v);
                    }
                }
                self.number()
            }
            None => Err("unexpected end".to_string()),
        }
    }
    fn number(&mut self) -> Result<Json, String> {
        let start = self.i;
        while self
            .peek()
            .is_some_and(|c| c.is_ascii_digit() || matches!(c, b'-' | b'+' | b'.' | b'e' | b'E'))
        {
            self.i += 1;
        }
        let t = self
            .s
            .get(start..self.i)
            .and_then(|b| core::str::from_utf8(b).ok())
            .unwrap_or("");
        t.parse::<f64>()
            .map(Json::Num)
            .map_err(|_| format!("bad number at byte {start}"))
    }
    fn string(&mut self) -> Result<String, String> {
        self.i += 1;
        let mut out = Vec::new();
        loop {
            let c = self.peek().ok_or("unterminated string")?;
            self.i += 1;
            match c {
                b'"' => break,
                b'\\' => {
                    let e = self.peek().ok_or("unterminated escape")?;
                    self.i += 1;
                    match e {
                        b'n' => out.push(b'\n'),
                        b't' => out.push(b'\t'),
                        b'r' => out.push(b'\r'),
                        b'b' | b'f' => out.push(b' '),
                        b'u' => {
                            let hex = self
                                .s
                                .get(self.i..self.i + 4)
                                .and_then(|b| core::str::from_utf8(b).ok())
                                .ok_or("bad \\u escape")?;
                            let code =
                                u32::from_str_radix(hex, 16).map_err(|_| "bad \\u escape")?;
                            self.i += 4;
                            let ch = char::from_u32(code).unwrap_or('?');
                            let mut buf = [0u8; 4];
                            out.extend_from_slice(ch.encode_utf8(&mut buf).as_bytes());
                        }
                        other => out.push(other),
                    }
                }
                other => out.push(other),
            }
        }
        String::from_utf8(out).map_err(|_| "string is not UTF-8".to_string())
    }
    fn array(&mut self) -> Result<Json, String> {
        self.i += 1;
        let mut items = Vec::new();
        self.ws();
        if self.peek() == Some(b']') {
            self.i += 1;
            return Ok(Json::Arr(items));
        }
        loop {
            items.push(self.value()?);
            self.ws();
            match self.peek() {
                Some(b',') => self.i += 1,
                Some(b']') => {
                    self.i += 1;
                    return Ok(Json::Arr(items));
                }
                _ => return Err(format!("bad array at byte {}", self.i)),
            }
        }
    }
    fn object(&mut self) -> Result<Json, String> {
        self.i += 1;
        let mut fields = Vec::new();
        self.ws();
        if self.peek() == Some(b'}') {
            self.i += 1;
            return Ok(Json::Obj(fields));
        }
        loop {
            self.ws();
            if self.peek() != Some(b'"') {
                return Err(format!("expected a key at byte {}", self.i));
            }
            let k = self.string()?;
            self.ws();
            if self.peek() != Some(b':') {
                return Err(format!("expected ':' at byte {}", self.i));
            }
            self.i += 1;
            fields.push((k, self.value()?));
            self.ws();
            match self.peek() {
                Some(b',') => self.i += 1,
                Some(b'}') => {
                    self.i += 1;
                    return Ok(Json::Obj(fields));
                }
                _ => return Err(format!("bad object at byte {}", self.i)),
            }
        }
    }
}
