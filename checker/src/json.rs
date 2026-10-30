//! A small JSON writer for the report. The report is only written, never parsed, by the
//! checker, so no JSON library is needed. Keys keep the order they were added in, so the
//! output is the same on every run.

use std::fmt::Write;

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

    /// An object from `(key, value)` pairs.
    #[must_use]
    pub fn obj<const N: usize>(pairs: [(&str, Json); N]) -> Self {
        Self::Obj(pairs.into_iter().map(|(k, v)| (k.to_owned(), v)).collect())
    }

    /// An object from a list of `(key, value)` pairs.
    #[must_use]
    pub fn from_pairs(pairs: Vec<(&str, Json)>) -> Self {
        Self::Obj(pairs.into_iter().map(|(k, v)| (k.to_owned(), v)).collect())
    }

    /// An array of strings.
    #[must_use]
    pub fn strings(items: &[String]) -> Self {
        Self::Arr(items.iter().map(Self::str).collect())
    }

    /// Pretty-printed text with two-space indentation and a final newline.
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
            // JSON has no NaN or infinity; they become null.
            Self::Num(x) if x.is_finite() => {
                let _ = write!(out, "{x}");
            }
            Self::Num(_) => out.push_str("null"),
            Self::Str(s) => quote(out, s),
            Self::Arr(items) if items.is_empty() => out.push_str("[]"),
            Self::Arr(items) => {
                // Short arrays of numbers stay on one line.
                let flat = items
                    .iter()
                    .all(|i| matches!(i, Self::Uint(_) | Self::Num(_)));
                out.push('[');
                for (k, item) in items.iter().enumerate() {
                    if k > 0 {
                        out.push_str(if flat { ", " } else { "," });
                    }
                    if !flat {
                        newline(out, depth + 1);
                    }
                    item.write(out, depth + 1);
                }
                if !flat {
                    newline(out, depth);
                }
                out.push(']');
            }
            Self::Obj(pairs) if pairs.is_empty() => out.push_str("{}"),
            Self::Obj(pairs) => {
                out.push('{');
                for (k, (key, val)) in pairs.iter().enumerate() {
                    if k > 0 {
                        out.push(',');
                    }
                    newline(out, depth + 1);
                    quote(out, key);
                    out.push_str(": ");
                    val.write(out, depth + 1);
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
    fn escapes_quotes_backslashes_and_control_characters() {
        let j = Json::str("a\"b\\c\nd\u{1}é");
        assert_eq!(j.render(), "\"a\\\"b\\\\c\\nd\\u0001é\"\n");
    }

    #[test]
    fn objects_keep_key_order_and_nest() {
        let j = Json::obj([
            ("z", Json::Uint(1)),
            ("a", Json::Arr(vec![Json::Num(0.5), Json::Num(2.0)])),
            ("n", Json::Null),
            ("e", Json::Arr(vec![])),
            ("s", Json::strings(&["x".to_owned()])),
        ]);
        let want = "{\n  \"z\": 1,\n  \"a\": [0.5, 2],\n  \"n\": null,\n  \"e\": [],\n  \"s\": [\n    \"x\"\n  ]\n}\n";
        assert_eq!(j.render(), want);
    }

    #[test]
    fn non_finite_numbers_become_null() {
        assert_eq!(Json::Num(f64::NAN).render(), "null\n");
        assert_eq!(Json::Num(f64::INFINITY).render(), "null\n");
    }
}
