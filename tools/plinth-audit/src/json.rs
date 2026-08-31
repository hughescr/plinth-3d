//! A hand-rolled JSON value, writer and reader.
//!
//! Zero dependencies is the crate's rule, so `serde_json` is not available.
//! The writer is what produces the output document of
//! `docs/specs/audit-invariants.md` §5; the reader is what parses an optional
//! `.coverage.repair.json` sidecar.
//!
//! **Number policy, and it is load-bearing.** Schema rule 5 says every numeric
//! field is a JSON *number*, never a string. [`Json::num`] therefore rejects
//! non-finite values at construction: `NaN` and the infinities have no JSON
//! spelling, and silently writing `null` for one would convert a broken
//! measurement into an absent one. Use [`Json::num_or_null`] where an absent
//! measurement is the honest answer, and never to launder a `NaN`.

use std::collections::BTreeMap;
use std::fmt::Write as _;

#[derive(Clone, Debug, PartialEq)]
pub enum Json {
    Null,
    Bool(bool),
    Num(f64),
    Str(String),
    Arr(Vec<Json>),
    /// `BTreeMap` so key order is stable across runs: two audits of the same
    /// bytes must produce byte-identical documents.
    Obj(BTreeMap<String, Json>),
}

impl Json {
    /// A JSON number. Panics on a non-finite value -- see the module note.
    #[must_use]
    pub fn num(v: f64) -> Self {
        assert!(
            v.is_finite(),
            "non-finite value reached the JSON writer; report it as a finding, never as null"
        );
        Self::Num(v)
    }
    /// A JSON number, or `null` when the quantity genuinely could not be
    /// measured. Never use this to hide a `NaN` produced by arithmetic.
    #[must_use]
    pub fn num_or_null(v: Option<f64>) -> Self {
        match v {
            Some(x) if x.is_finite() => Self::Num(x),
            _ => Self::Null,
        }
    }
    #[must_use]
    pub fn int(v: impl Into<i64>) -> Self {
        Self::Num(v.into() as f64)
    }
    #[must_use]
    pub fn usize(v: usize) -> Self {
        Self::Num(v as f64)
    }
    #[must_use]
    pub fn str(v: impl Into<String>) -> Self {
        Self::Str(v.into())
    }
    #[must_use]
    pub fn obj() -> Self {
        Self::Obj(BTreeMap::new())
    }
    #[must_use]
    pub fn v3(p: [f64; 3]) -> Self {
        Self::Arr(vec![Self::num(p[0]), Self::num(p[1]), Self::num(p[2])])
    }
    #[must_use]
    pub fn nums(vs: impl IntoIterator<Item = f64>) -> Self {
        Self::Arr(vs.into_iter().map(Self::num).collect())
    }

    /// Insert into an object. Panics if `self` is not an object.
    pub fn set(&mut self, key: &str, value: Self) -> &mut Self {
        match self {
            Self::Obj(m) => {
                m.insert(key.to_string(), value);
            }
            _ => panic!("Json::set on a non-object"),
        }
        self
    }
    pub fn push(&mut self, value: Self) {
        match self {
            Self::Arr(a) => a.push(value),
            _ => panic!("Json::push on a non-array"),
        }
    }

    #[must_use]
    pub fn get(&self, key: &str) -> Option<&Self> {
        match self {
            Self::Obj(m) => m.get(key),
            _ => None,
        }
    }
    #[must_use]
    pub fn as_f64(&self) -> Option<f64> {
        match self {
            Self::Num(v) => Some(*v),
            _ => None,
        }
    }
    #[must_use]
    pub fn as_str(&self) -> Option<&str> {
        match self {
            Self::Str(s) => Some(s),
            _ => None,
        }
    }
    #[must_use]
    pub fn as_array(&self) -> Option<&[Self]> {
        match self {
            Self::Arr(a) => Some(a),
            _ => None,
        }
    }
    #[must_use]
    pub const fn is_null(&self) -> bool {
        matches!(self, Self::Null)
    }

    /// Pretty-print with two-space indent, deterministic key order.
    #[must_use]
    pub fn to_pretty(&self) -> String {
        let mut s = String::new();
        self.write(&mut s, 0, true);
        s.push('\n');
        s
    }
    /// Compact single-line form.
    #[must_use]
    pub fn to_compact(&self) -> String {
        let mut s = String::new();
        self.write(&mut s, 0, false);
        s
    }

    fn write(&self, out: &mut String, indent: usize, pretty: bool) {
        let pad = |o: &mut String, n: usize| {
            if pretty {
                o.push('\n');
                for _ in 0..n {
                    o.push_str("  ");
                }
            }
        };
        match self {
            Self::Null => out.push_str("null"),
            Self::Bool(b) => out.push_str(if *b { "true" } else { "false" }),
            Self::Num(v) => write_number(out, *v),
            Self::Str(s) => write_string(out, s),
            Self::Arr(a) => {
                if a.is_empty() {
                    out.push_str("[]");
                    return;
                }
                out.push('[');
                for (i, item) in a.iter().enumerate() {
                    if i > 0 {
                        out.push(',');
                    }
                    pad(out, indent + 1);
                    item.write(out, indent + 1, pretty);
                }
                pad(out, indent);
                out.push(']');
            }
            Self::Obj(m) => {
                if m.is_empty() {
                    out.push_str("{}");
                    return;
                }
                out.push('{');
                for (i, (k, v)) in m.iter().enumerate() {
                    if i > 0 {
                        out.push(',');
                    }
                    pad(out, indent + 1);
                    write_string(out, k);
                    out.push(':');
                    if pretty {
                        out.push(' ');
                    }
                    v.write(out, indent + 1, pretty);
                }
                pad(out, indent);
                out.push('}');
            }
        }
    }
}

fn write_number(out: &mut String, v: f64) {
    if !v.is_finite() {
        // Unreachable through `Json::num`; belt and braces for a hand-built
        // `Json::Num(..)`. Writing `null` is wrong, so make it loud instead.
        out.push_str("\"NON_FINITE\"");
        return;
    }
    // Rust's `Display` for `f64` is the SHORTEST decimal that round-trips
    // exactly, and it never emits exponent notation, so the output is both
    // valid JSON and readable. `0.014` stays `0.014` rather than becoming
    // `1.40000000000000003e-2`.
    let _ = write!(out, "{v}");
}

fn write_string(out: &mut String, s: &str) {
    out.push('"');
    for ch in s.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => {
                let _ = write!(out, "\\u{:04x}", c as u32);
            }
            c => out.push(c),
        }
    }
    out.push('"');
}

// ---------------------------------------------------------------------------
// Reader
// ---------------------------------------------------------------------------

#[derive(Debug)]
pub struct JsonError {
    pub offset: usize,
    pub message: String,
}

impl std::fmt::Display for JsonError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "JSON parse error at byte {}: {}",
            self.offset, self.message
        )
    }
}
impl std::error::Error for JsonError {}

/// Parse a complete JSON document. Trailing non-whitespace is an error --
/// a truncated or doubled sidecar must be refused loudly, not half-read.
pub fn parse(text: &str) -> Result<Json, JsonError> {
    let b = text.as_bytes();
    let mut p = Parser { b, i: 0 };
    p.ws();
    let v = p.value()?;
    p.ws();
    if p.i != b.len() {
        return Err(p.err("trailing content after the top-level value"));
    }
    Ok(v)
}

struct Parser<'a> {
    b: &'a [u8],
    i: usize,
}

impl Parser<'_> {
    fn err(&self, m: &str) -> JsonError {
        JsonError {
            offset: self.i,
            message: m.to_string(),
        }
    }
    fn ws(&mut self) {
        while self.i < self.b.len() && matches!(self.b[self.i], b' ' | b'\t' | b'\n' | b'\r') {
            self.i += 1;
        }
    }
    fn eat(&mut self, lit: &[u8]) -> bool {
        if self.b[self.i..].starts_with(lit) {
            self.i += lit.len();
            true
        } else {
            false
        }
    }
    fn value(&mut self) -> Result<Json, JsonError> {
        if self.i >= self.b.len() {
            return Err(self.err("unexpected end of input"));
        }
        match self.b[self.i] {
            b'n' => {
                if self.eat(b"null") {
                    return Ok(Json::Null);
                }
                Err(self.err("expected null"))
            }
            b't' => {
                if self.eat(b"true") {
                    return Ok(Json::Bool(true));
                }
                Err(self.err("expected true"))
            }
            b'f' => {
                if self.eat(b"false") {
                    return Ok(Json::Bool(false));
                }
                Err(self.err("expected false"))
            }
            b'"' => self.string().map(Json::Str),
            b'[' => self.array(),
            b'{' => self.object(),
            b'-' | b'0'..=b'9' => self.number(),
            _ => Err(self.err("unexpected character at the start of a value")),
        }
    }
    fn string(&mut self) -> Result<String, JsonError> {
        self.i += 1; // opening quote
        let mut s = String::new();
        loop {
            if self.i >= self.b.len() {
                return Err(self.err("unterminated string"));
            }
            match self.b[self.i] {
                b'"' => {
                    self.i += 1;
                    return Ok(s);
                }
                b'\\' => {
                    self.i += 1;
                    if self.i >= self.b.len() {
                        return Err(self.err("unterminated escape"));
                    }
                    let c = self.b[self.i];
                    self.i += 1;
                    match c {
                        b'"' => s.push('"'),
                        b'\\' => s.push('\\'),
                        b'/' => s.push('/'),
                        b'b' => s.push('\u{8}'),
                        b'f' => s.push('\u{c}'),
                        b'n' => s.push('\n'),
                        b'r' => s.push('\r'),
                        b't' => s.push('\t'),
                        b'u' => {
                            let cp = self.hex4()?;
                            if (0xD800..0xDC00).contains(&cp) {
                                if !self.eat(b"\\u") {
                                    return Err(self.err("lone high surrogate"));
                                }
                                let lo = self.hex4()?;
                                if !(0xDC00..0xE000).contains(&lo) {
                                    return Err(self.err("invalid low surrogate"));
                                }
                                let combined = 0x1_0000 + ((cp - 0xD800) << 10) + (lo - 0xDC00);
                                s.push(
                                    char::from_u32(combined)
                                        .ok_or_else(|| self.err("invalid code point"))?,
                                );
                            } else {
                                s.push(
                                    char::from_u32(cp)
                                        .ok_or_else(|| self.err("invalid code point"))?,
                                );
                            }
                        }
                        _ => return Err(self.err("unknown escape")),
                    }
                }
                _ => {
                    let start = self.i;
                    while self.i < self.b.len() && self.b[self.i] != b'"' && self.b[self.i] != b'\\'
                    {
                        self.i += 1;
                    }
                    match std::str::from_utf8(&self.b[start..self.i]) {
                        Ok(chunk) => s.push_str(chunk),
                        Err(_) => return Err(self.err("invalid UTF-8 in string")),
                    }
                }
            }
        }
    }
    fn hex4(&mut self) -> Result<u32, JsonError> {
        if self.i + 4 > self.b.len() {
            return Err(self.err("truncated \\u escape"));
        }
        let s = std::str::from_utf8(&self.b[self.i..self.i + 4])
            .map_err(|_| self.err("bad \\u escape"))?;
        let v = u32::from_str_radix(s, 16).map_err(|_| self.err("bad \\u escape"))?;
        self.i += 4;
        Ok(v)
    }
    fn number(&mut self) -> Result<Json, JsonError> {
        let start = self.i;
        if self.i < self.b.len() && self.b[self.i] == b'-' {
            self.i += 1;
        }
        while self.i < self.b.len()
            && matches!(
                self.b[self.i],
                b'0'..=b'9' | b'.' | b'e' | b'E' | b'+' | b'-'
            )
        {
            self.i += 1;
        }
        let s = std::str::from_utf8(&self.b[start..self.i]).map_err(|_| self.err("bad number"))?;
        let v: f64 = s.parse().map_err(|_| self.err("bad number"))?;
        if !v.is_finite() {
            return Err(self.err("non-finite number"));
        }
        Ok(Json::Num(v))
    }
    fn array(&mut self) -> Result<Json, JsonError> {
        self.i += 1;
        let mut a = Vec::new();
        self.ws();
        if self.i < self.b.len() && self.b[self.i] == b']' {
            self.i += 1;
            return Ok(Json::Arr(a));
        }
        loop {
            self.ws();
            a.push(self.value()?);
            self.ws();
            if self.i >= self.b.len() {
                return Err(self.err("unterminated array"));
            }
            match self.b[self.i] {
                b',' => self.i += 1,
                b']' => {
                    self.i += 1;
                    return Ok(Json::Arr(a));
                }
                _ => return Err(self.err("expected ',' or ']'")),
            }
        }
    }
    fn object(&mut self) -> Result<Json, JsonError> {
        self.i += 1;
        let mut m = BTreeMap::new();
        self.ws();
        if self.i < self.b.len() && self.b[self.i] == b'}' {
            self.i += 1;
            return Ok(Json::Obj(m));
        }
        loop {
            self.ws();
            if self.i >= self.b.len() || self.b[self.i] != b'"' {
                return Err(self.err("expected an object key"));
            }
            let k = self.string()?;
            self.ws();
            if self.i >= self.b.len() || self.b[self.i] != b':' {
                return Err(self.err("expected ':'"));
            }
            self.i += 1;
            self.ws();
            let v = self.value()?;
            m.insert(k, v);
            self.ws();
            if self.i >= self.b.len() {
                return Err(self.err("unterminated object"));
            }
            match self.b[self.i] {
                b',' => self.i += 1,
                b'}' => {
                    self.i += 1;
                    return Ok(Json::Obj(m));
                }
                _ => return Err(self.err("expected ',' or '}'")),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_a_nested_document() {
        let src = r#"{"a":[1,2.5,-3e2],"b":{"c":null,"d":true},"e":"x\ny"}"#;
        let v = parse(src).expect("parse");
        let again = parse(&v.to_compact()).expect("reparse");
        assert_eq!(v, again);
        assert_eq!(
            v.get("a").and_then(Json::as_array).map(<[Json]>::len),
            Some(3)
        );
    }

    #[test]
    fn null_survives_and_is_not_zero() {
        let v = parse(r#"{"carried_area_mm2":null}"#).expect("parse");
        assert!(v.get("carried_area_mm2").expect("key").is_null());
        assert_eq!(v.get("carried_area_mm2").and_then(Json::as_f64), None);
    }

    #[test]
    fn trailing_content_is_refused() {
        assert!(parse("{} {}").is_err());
        assert!(parse("[1,2,]").is_err());
    }

    #[test]
    fn keys_are_written_in_a_stable_order() {
        let mut o = Json::obj();
        o.set("z", Json::num(1.0));
        o.set("a", Json::num(2.0));
        assert_eq!(o.to_compact(), r#"{"a":2,"z":1}"#);
    }
}
