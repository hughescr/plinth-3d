//! A minimal JSON writer for the run report (no serde). Keys keep insertion order, so the
//! text is the same on every run.

use std::fmt::Write;

/// A JSON value.
#[derive(Debug, Clone, PartialEq)]
pub enum Json {
    Null,
    Bool(bool),
    Uint(u64),
    Num(f64),
    Str(String),
    Arr(Vec<Json>),
    Obj(Vec<(String, Json)>),
}

impl Json {
    #[must_use]
    pub fn str(s: impl Into<String>) -> Self {
        Self::Str(s.into())
    }

    #[must_use]
    pub fn obj<const N: usize>(pairs: [(&str, Json); N]) -> Self {
        Self::Obj(pairs.into_iter().map(|(k, v)| (k.to_owned(), v)).collect())
    }

    /// The value of `key` when this is an object that has it.
    #[must_use]
    pub fn get(&self, key: &str) -> Option<&Json> {
        match self {
            Self::Obj(pairs) => pairs.iter().find(|(k, _)| k == key).map(|(_, v)| v),
            _ => None,
        }
    }

    /// The text when this is a string.
    #[must_use]
    pub fn as_str(&self) -> Option<&str> {
        match self {
            Self::Str(s) => Some(s),
            _ => None,
        }
    }

    /// The number when this is a non-negative whole number.
    #[must_use]
    pub fn as_u64(&self) -> Option<u64> {
        match self {
            Self::Uint(n) => Some(*n),
            _ => None,
        }
    }

    /// The items when this is an array.
    #[must_use]
    pub fn as_arr(&self) -> Option<&[Json]> {
        match self {
            Self::Arr(v) => Some(v),
            _ => None,
        }
    }

    /// Pretty text, two-space indent, ending in a newline. Non-finite numbers become null.
    #[must_use]
    pub fn render(&self) -> String {
        let mut out = String::new();
        self.write(&mut out, 0);
        out.push('\n');
        out
    }

    fn write(&self, out: &mut String, depth: usize) {
        match self {
            Self::Null => out.push_str("null"),
            Self::Bool(b) => out.push_str(if *b { "true" } else { "false" }),
            Self::Uint(n) => {
                let _ = write!(out, "{n}");
            }
            Self::Num(x) if x.is_finite() => {
                let _ = write!(out, "{x}");
            }
            Self::Num(_) => out.push_str("null"),
            Self::Str(s) => quote(out, s),
            Self::Arr(items) if items.is_empty() => out.push_str("[]"),
            Self::Arr(items) => {
                out.push('[');
                for (i, v) in items.iter().enumerate() {
                    if i > 0 {
                        out.push(',');
                    }
                    newline(out, depth + 1);
                    v.write(out, depth + 1);
                }
                newline(out, depth);
                out.push(']');
            }
            Self::Obj(pairs) if pairs.is_empty() => out.push_str("{}"),
            Self::Obj(pairs) => {
                out.push('{');
                for (i, (k, v)) in pairs.iter().enumerate() {
                    if i > 0 {
                        out.push(',');
                    }
                    newline(out, depth + 1);
                    quote(out, k);
                    out.push_str(": ");
                    v.write(out, depth + 1);
                }
                newline(out, depth);
                out.push('}');
            }
        }
    }
}

fn newline(out: &mut String, depth: usize) {
    out.push('\n');
    for _ in 0..depth {
        out.push_str("  ");
    }
}

fn quote(out: &mut String, s: &str) {
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if u32::from(c) < 0x20 => {
                let _ = write!(out, "\\u{:04x}", u32::from(c));
            }
            c => out.push(c),
        }
    }
    out.push('"');
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn escapes_and_nests() {
        let j = Json::obj([
            ("a", Json::str("q\"\\\n\u{1}")),
            (
                "b",
                Json::Arr(vec![Json::Uint(1), Json::Num(0.5), Json::Null]),
            ),
            ("c", Json::Arr(vec![])),
            ("n", Json::Num(f64::NAN)),
        ]);
        let want = "{\n  \"a\": \"q\\\"\\\\\\n\\u0001\",\n  \"b\": [\n    1,\n    0.5,\n    null\n  ],\n  \"c\": [],\n  \"n\": null\n}\n";
        assert_eq!(j.render(), want);
    }
}
