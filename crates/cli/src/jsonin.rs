//! A small JSON reader for the checker's report (the two programs share only this text, never
//! Rust types). It accepts standard JSON, limits nesting, and never panics on any input.

use crate::json::Json;

/// Deepest nesting accepted.
const MAX_DEPTH: usize = 64;

struct Reader<'a> {
    b: &'a [u8],
    at: usize,
}

type Res<T> = Result<T, String>;

impl Reader<'_> {
    fn err<T>(&self, what: &str) -> Res<T> {
        Err(format!("{what} at byte {}", self.at))
    }

    fn ws(&mut self) {
        while self.b.get(self.at).is_some_and(u8::is_ascii_whitespace) {
            self.at += 1;
        }
    }

    fn eat(&mut self, c: u8) -> bool {
        if self.b.get(self.at) == Some(&c) {
            self.at += 1;
            true
        } else {
            false
        }
    }

    fn word(&mut self, w: &str, v: Json) -> Res<Json> {
        if self.b[self.at..].starts_with(w.as_bytes()) {
            self.at += w.len();
            Ok(v)
        } else {
            self.err("bad literal")
        }
    }

    fn value(&mut self, depth: usize) -> Res<Json> {
        if depth > MAX_DEPTH {
            return self.err("nesting too deep");
        }
        self.ws();
        match self.b.get(self.at) {
            None => self.err("unexpected end"),
            Some(b'n') => self.word("null", Json::Null),
            Some(b't') => self.word("true", Json::Bool(true)),
            Some(b'f') => self.word("false", Json::Bool(false)),
            Some(b'"') => Ok(Json::Str(self.string()?)),
            Some(b'[') => self.array(depth),
            Some(b'{') => self.object(depth),
            Some(b'-' | b'0'..=b'9') => self.number(),
            Some(_) => self.err("unexpected character"),
        }
    }

    fn number(&mut self) -> Res<Json> {
        let start = self.at;
        while self
            .b
            .get(self.at)
            .is_some_and(|c| matches!(c, b'-' | b'+' | b'.' | b'e' | b'E' | b'0'..=b'9'))
        {
            self.at += 1;
        }
        let text = std::str::from_utf8(&self.b[start..self.at]).map_err(|e| e.to_string())?;
        if let Ok(n) = text.parse::<u64>() {
            return Ok(Json::Uint(n));
        }
        match text.parse::<f64>() {
            Ok(x) if x.is_finite() => Ok(Json::Num(x)),
            _ => self.err("bad number"),
        }
    }

    fn hex4(&mut self) -> Res<u32> {
        let digits = self.b.get(self.at..self.at + 4);
        let text = digits.and_then(|d| std::str::from_utf8(d).ok());
        match text.and_then(|t| u32::from_str_radix(t, 16).ok()) {
            Some(n) => {
                self.at += 4;
                Ok(n)
            }
            None => self.err("bad \\u escape"),
        }
    }

    fn unicode(&mut self) -> Res<char> {
        let hi = self.hex4()?;
        if (0xD800..0xDC00).contains(&hi) && self.b[self.at..].starts_with(b"\\u") {
            self.at += 2;
            let lo = self.hex4()?;
            if (0xDC00..0xE000).contains(&lo) {
                let c = 0x10000 + ((hi - 0xD800) << 10) + (lo - 0xDC00);
                return Ok(char::from_u32(c).unwrap_or('\u{fffd}'));
            }
            return Ok('\u{fffd}');
        }
        Ok(char::from_u32(hi).unwrap_or('\u{fffd}'))
    }

    fn string(&mut self) -> Res<String> {
        self.at += 1; // the opening quote
        let mut out: Vec<u8> = Vec::new();
        loop {
            let Some(&c) = self.b.get(self.at) else {
                return self.err("unterminated string");
            };
            self.at += 1;
            match c {
                b'"' => break,
                b'\\' => {
                    let Some(&e) = self.b.get(self.at) else {
                        return self.err("unterminated escape");
                    };
                    self.at += 1;
                    let ch = match e {
                        b'"' => '"',
                        b'\\' => '\\',
                        b'/' => '/',
                        b'b' => '\u{8}',
                        b'f' => '\u{c}',
                        b'n' => '\n',
                        b'r' => '\r',
                        b't' => '\t',
                        b'u' => self.unicode()?,
                        _ => return self.err("bad escape"),
                    };
                    let mut buf = [0_u8; 4];
                    out.extend_from_slice(ch.encode_utf8(&mut buf).as_bytes());
                }
                c if c < 0x20 => return self.err("control character in string"),
                c => out.push(c),
            }
        }
        String::from_utf8(out).map_err(|e| e.to_string())
    }

    fn array(&mut self, depth: usize) -> Res<Json> {
        self.at += 1;
        let mut items = Vec::new();
        self.ws();
        if self.eat(b']') {
            return Ok(Json::Arr(items));
        }
        loop {
            items.push(self.value(depth + 1)?);
            self.ws();
            if self.eat(b']') {
                return Ok(Json::Arr(items));
            }
            if !self.eat(b',') {
                return self.err("expected , or ]");
            }
        }
    }

    fn object(&mut self, depth: usize) -> Res<Json> {
        self.at += 1;
        let mut pairs = Vec::new();
        self.ws();
        if self.eat(b'}') {
            return Ok(Json::Obj(pairs));
        }
        loop {
            self.ws();
            if self.b.get(self.at) != Some(&b'"') {
                return self.err("expected a key");
            }
            let key = self.string()?;
            self.ws();
            if !self.eat(b':') {
                return self.err("expected :");
            }
            pairs.push((key, self.value(depth + 1)?));
            self.ws();
            if self.eat(b'}') {
                return Ok(Json::Obj(pairs));
            }
            if !self.eat(b',') {
                return self.err("expected , or }");
            }
        }
    }
}

/// Parse one JSON document.
///
/// # Errors
/// A one-line message with the byte position for malformed text, trailing text or nesting
/// deeper than 64.
pub fn parse(text: &[u8]) -> Result<Json, String> {
    let mut r = Reader { b: text, at: 0 };
    let v = r.value(0)?;
    r.ws();
    if r.at == text.len() {
        Ok(v)
    } else {
        r.err("trailing text")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_what_the_writer_writes() {
        let j = Json::obj([
            ("a", Json::str("q\"\\\n\u{1}é")),
            (
                "b",
                Json::Arr(vec![Json::Uint(1), Json::Num(0.5), Json::Null]),
            ),
            ("c", Json::Arr(vec![])),
            ("d", Json::obj([("t", Json::Bool(true))])),
        ]);
        assert_eq!(parse(j.render().as_bytes()).unwrap(), j);
    }

    #[test]
    fn reads_escapes_and_numbers() {
        let j =
            parse(br#"{"s":"\u00e9\ud83d\ude00\/","n":-2.5e1,"u":18446744073709551615}"#).unwrap();
        assert_eq!(j.get("s").and_then(Json::as_str), Some("é😀/"));
        assert_eq!(j.get("n"), Some(&Json::Num(-25.0)));
        assert_eq!(j.get("u").and_then(Json::as_u64), Some(u64::MAX));
    }

    #[test]
    fn rejects_bad_text_without_panicking() {
        for bad in [
            &b""[..],
            b"{",
            b"[1,]",
            b"{\"a\" 1}",
            b"\"abc",
            b"\"\\u12\"",
            b"nul",
            b"1 2",
            b"\"\x01\"",
            b"-",
            b"\xff",
        ] {
            assert!(parse(bad).is_err(), "{bad:?}");
        }
        let deep = "[".repeat(100_000);
        assert!(parse(deep.as_bytes()).unwrap_err().contains("too deep"));
    }
}
