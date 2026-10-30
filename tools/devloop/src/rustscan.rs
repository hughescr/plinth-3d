//! A small Rust lexer and the include scan built on it (design dev-loop.md section 4.4, item 3).
//!
//! The scan reads tokens, not text, so spacing, line breaks and comments between the tokens of
//! `include_str ! (...)` change nothing. Strings, raw strings, character literals and comments
//! are skipped as units, so a token inside them is not code.
//!
//! Rules, on code tokens only:
//! - `include!`, `include_str!` and `include_bytes!` must be called with exactly one string
//!   literal that names a plain relative path: not empty, not absolute, no `..` component, no
//!   backslash (also how an escape such as `\x2e` is refused) and no drive colon. A computed
//!   argument (`concat!`, `env!`, a macro variable) is refused.
//! - Those names must not appear in a `use` declaration (an alias would hide the call).
//! - `path = <value>` (the `#[path]` attribute, also inside `cfg_attr`, also passed through a
//!   macro) must have a plain relative path string literal as its value.
//! - `#[` followed by `$` (an attribute a macro assembles) is refused.

#[derive(Debug, Clone, PartialEq, Eq)]
enum Tok {
    Ident(String),
    /// A string literal's raw text between the quotes, in any of its spellings.
    Str(String),
    Punct(char),
    Other,
}

#[derive(Debug)]
struct Lexed {
    tok: Tok,
    line: usize,
}

fn is_ident_start(c: char) -> bool {
    c == '_' || c.is_alphabetic()
}

fn is_ident_char(c: char) -> bool {
    c == '_' || c.is_alphanumeric()
}

/// Where a string literal that starts at `i` begins and how it ends: (index of the first
/// content char, number of `#` that close it, raw?). `None` if no string literal starts at `i`.
fn string_start(c: &[char], i: usize) -> Option<(usize, usize, bool)> {
    let mut j = i + usize::from(matches!(c.get(i), Some('b' | 'c')));
    let raw = c.get(j) == Some(&'r');
    let mut hashes = 0;
    if raw {
        j += 1;
        while c.get(j) == Some(&'#') {
            hashes += 1;
            j += 1;
        }
    }
    (c.get(j) == Some(&'"')).then_some((j + 1, hashes, raw))
}

/// A string body from `body`: (text, index after the closing quote and hashes, newlines inside).
/// Escapes are kept as written (backslash and the next char); a raw string has none.
fn read_string(c: &[char], body: usize, hashes: usize, raw: bool) -> (String, usize, usize) {
    let (mut j, mut lines, mut s) = (body, 0, String::new());
    while j < c.len() {
        if !raw && c[j] == '\\' {
            s.push('\\');
            j += 1;
            if let Some(&n) = c.get(j) {
                lines += usize::from(n == '\n');
                s.push(n);
                j += 1;
            }
        } else if c[j] == '"' && (0..hashes).all(|k| c.get(j + 1 + k) == Some(&'#')) {
            return (s, j + 1 + hashes, lines);
        } else {
            lines += usize::from(c[j] == '\n');
            s.push(c[j]);
            j += 1;
        }
    }
    (s, j, lines)
}

/// A nested block comment from `i`: (index after it, newlines inside).
fn skip_block_comment(c: &[char], mut i: usize) -> (usize, usize) {
    let (mut depth, mut lines) = (0usize, 0);
    while i < c.len() {
        if c[i] == '/' && c.get(i + 1) == Some(&'*') {
            depth += 1;
            i += 2;
        } else if c[i] == '*' && c.get(i + 1) == Some(&'/') {
            depth -= 1;
            i += 2;
            if depth == 0 {
                break;
            }
        } else {
            lines += usize::from(c[i] == '\n');
            i += 1;
        }
    }
    (i, lines)
}

/// The identifier (or number) characters from `i`: (text, index after).
fn read_ident(c: &[char], i: usize) -> (String, usize) {
    let end = c[i..]
        .iter()
        .position(|&x| !is_ident_char(x))
        .map_or(c.len(), |n| i + n);
    (c[i..end].iter().collect(), end)
}

/// A character literal (`'x'`, `'\n'`, `'\''`) or a lifetime or label (`'a`) from `i`, which is
/// at the quote: the index after it. Neither is code that matters here.
fn skip_quote(c: &[char], i: usize) -> usize {
    if c.get(i + 1) == Some(&'\\') {
        let close = c[(i + 3).min(c.len())..].iter().position(|&x| x == '\'');
        close.map_or(c.len(), |n| i + 3 + n + 1)
    } else if c.get(i + 2) == Some(&'\'') {
        i + 3
    } else {
        read_ident(c, (i + 1).min(c.len())).1
    }
}

/// Lex `text`. Never fails: unterminated constructs end at the end of the text.
fn lex(text: &str) -> Vec<Lexed> {
    let c: Vec<char> = text.chars().collect();
    let (mut i, mut line) = (0, 1);
    let mut out = Vec::new();
    while i < c.len() {
        let ch = c[i];
        if ch == '\n' {
            line += 1;
            i += 1;
        } else if ch.is_whitespace() {
            i += 1;
        } else if ch == '/' && c.get(i + 1) == Some(&'/') {
            while i < c.len() && c[i] != '\n' {
                i += 1;
            }
        } else if ch == '/' && c.get(i + 1) == Some(&'*') {
            let (end, lines) = skip_block_comment(&c, i);
            i = end;
            line += lines;
        } else if let Some((body, hashes, raw)) = string_start(&c, i) {
            let (s, end, lines) = read_string(&c, body, hashes, raw);
            out.push(Lexed {
                tok: Tok::Str(s),
                line,
            });
            i = end;
            line += lines;
        } else if ch == 'r'
            && c.get(i + 1) == Some(&'#')
            && c.get(i + 2).is_some_and(|&x| is_ident_start(x))
        {
            // A raw identifier: `r#include_str` names the same thing as `include_str`.
            let (s, end) = read_ident(&c, i + 2);
            out.push(Lexed {
                tok: Tok::Ident(s),
                line,
            });
            i = end;
        } else if is_ident_start(ch) {
            let (s, end) = read_ident(&c, i);
            out.push(Lexed {
                tok: Tok::Ident(s),
                line,
            });
            i = end;
        } else if ch.is_ascii_digit() {
            i = read_ident(&c, i).1;
            out.push(Lexed {
                tok: Tok::Other,
                line,
            });
        } else if ch == '\'' {
            i = skip_quote(&c, i);
            out.push(Lexed {
                tok: Tok::Other,
                line,
            });
        } else {
            out.push(Lexed {
                tok: Tok::Punct(ch),
                line,
            });
            i += 1;
        }
    }
    out
}

/// True if `s` is a non-empty, relative path with no `..` component, backslash or colon.
fn plain_path(s: &str) -> bool {
    !s.is_empty()
        && !s.starts_with('/')
        && !s.contains(['\\', ':'])
        && s.split('/').all(|part| part != "..")
}

const INCLUDE_MACROS: [&str; 3] = ["include", "include_str", "include_bytes"];

fn is_punct(t: Option<&Lexed>, c: char) -> bool {
    t.is_some_and(|l| l.tok == Tok::Punct(c))
}

/// The string literal at `t`, if it names a plain relative path.
fn plain_literal(t: Option<&Lexed>) -> bool {
    matches!(t, Some(Lexed { tok: Tok::Str(s), .. }) if plain_path(s))
}

/// True if the tokens after an include macro's `!` are `( "plain path" )` (any bracket pair,
/// an optional trailing comma).
fn include_call_is_plain(rest: &[Lexed]) -> bool {
    let close = match rest.first().map(|l| &l.tok) {
        Some(Tok::Punct('(')) => ')',
        Some(Tok::Punct('[')) => ']',
        Some(Tok::Punct('{')) => '}',
        _ => return false,
    };
    plain_literal(rest.get(1))
        && (is_punct(rest.get(2), close)
            || (is_punct(rest.get(2), ',') && is_punct(rest.get(3), close)))
}

/// For each token, whether it lies inside a `#[...]` or `#![...]` attribute.
fn attribute_marks(toks: &[Lexed]) -> Vec<bool> {
    let mut marks = vec![false; toks.len()];
    let mut i = 0;
    while i < toks.len() {
        let open = i + 1 + usize::from(is_punct(toks.get(i + 1), '!'));
        if is_punct(toks.get(i), '#') && is_punct(toks.get(open), '[') {
            let mut depth = 0usize;
            let mut j = open;
            while j < toks.len() {
                if is_punct(toks.get(j), '[') {
                    depth += 1;
                } else if is_punct(toks.get(j), ']') {
                    depth -= 1;
                }
                marks[j] = true;
                j += 1;
                if depth == 0 {
                    break;
                }
            }
            i = j;
        } else {
            i += 1;
        }
    }
    marks
}

/// Violations in one source file's text, each `file:line: reason`.
#[must_use]
pub fn scan_text(file: &str, text: &str) -> Vec<String> {
    let toks = lex(text);
    let mut bad = Vec::new();
    let in_attr = attribute_marks(&toks);
    let mut in_use = false;
    for (i, t) in toks.iter().enumerate() {
        let mut flag = |why: &str| bad.push(format!("{file}:{}: {why}", t.line));
        match &t.tok {
            Tok::Ident(name) if name == "use" => in_use = true,
            Tok::Punct(';') => in_use = false,
            Tok::Ident(name) if INCLUDE_MACROS.contains(&name.as_str()) => {
                if is_punct(toks.get(i + 1), '!') {
                    if !include_call_is_plain(&toks[i + 2..]) {
                        flag(&format!(
                            "{name}! must be called with one string literal naming a plain relative path inside checker/"
                        ));
                    }
                } else if in_use {
                    flag(&format!(
                        "{name} must not be imported or renamed in a use declaration"
                    ));
                }
            }
            Tok::Ident(name) if name == "path" && is_punct(toks.get(i + 1), '=') => {
                // Any `path = "<not plain>"`, and inside an attribute any `path =` whose value is
                // not a plain string literal. A plain assignment such as `path = p.join(x)`
                // outside an attribute is ordinary code.
                let value = toks.get(i + 2);
                let literal = matches!(
                    value,
                    Some(Lexed {
                        tok: Tok::Str(_),
                        ..
                    })
                );
                if (literal || in_attr[i]) && !plain_literal(value) {
                    flag(
                        "path = must be a string literal naming a plain relative path inside checker/",
                    );
                }
            }
            Tok::Punct('#') => {
                let j = i + 1 + usize::from(is_punct(toks.get(i + 1), '!'));
                if is_punct(toks.get(j), '[') && is_punct(toks.get(j + 1), '$') {
                    flag(
                        "an attribute assembled by a macro hides #[path]; spell the attribute out",
                    );
                }
            }
            _ => {}
        }
    }
    bad
}

#[cfg(test)]
mod tests {
    use super::*;

    fn count(src: &str) -> usize {
        scan_text("a.rs", src).len()
    }

    #[test]
    fn plain_inside_includes_pass() {
        let ok = "#[path = \"sub/mod.rs\"]\nmod sub;\nconst A: &str = include_str!(\"data.txt\");\n\
                  const B: &[u8] = include_bytes!(\"d/e.bin\",);\ninclude!{\"gen.rs\"}\n";
        assert_eq!(scan_text("a.rs", ok), Vec::<String>::new());
    }

    #[test]
    fn direct_outside_includes_fail() {
        for bad in [
            "#[path = \"../../crates/geom/src/lib.rs\"]\nmod g;",
            "const A: &str = include_str!(\"../../limits.toml\");",
            "const A: &[u8] = include_bytes!(\"/etc/hosts\");",
            "include!(concat!(env!(\"CARGO_MANIFEST_DIR\"), \"/x.rs\"));",
            "#[path = some_macro!()]\nmod g;",
            "const A: &str = include_str!(\"a/../../x\");",
        ] {
            assert_eq!(count(bad), 1, "{bad}");
        }
    }

    #[test]
    fn spacing_comments_and_line_breaks_do_not_hide_an_include() {
        // The regression behind the verifier's finding: whitespace between the name and `!`.
        for bad in [
            "pub const P: &str = include_str ! (\"../../limits.toml\");",
            "const P: &str = include_str\n!\n(\n\"../../limits.toml\"\n);",
            "const P: &str = include_str /* c */ ! /* d */ (\"../../limits.toml\");",
            "const P: &str = include_str // c\n ! (\"../../limits.toml\");",
            "const P: &str = r#include_str!(\"../../limits.toml\");",
            "const P: &str = core::include_str ! (\"../../limits.toml\");",
            "const P: &str = std::include_str!  [ \"../../limits.toml\" ];",
            "#  [  path  =  \"../../x.rs\"  ]\nmod g;",
            "#![path=\"../x.rs\"]",
            "#[cfg_attr(unix, path = \"../x.rs\")]\nmod g;",
        ] {
            assert_eq!(count(bad), 1, "{bad}");
        }
    }

    #[test]
    fn escapes_aliases_and_macros_do_not_hide_an_include() {
        for bad in [
            "const P: &str = include_str!(\"\\x2e\\x2e/x\");",
            "const P: &str = include_str!(\"..\\\\x\");",
            "use std::include_str as inc;\nconst P: &str = inc!(\"../x\");",
            "use std::{include_bytes as b};",
            "macro_rules! m { ($p:literal) => { include_str!($p) }; }",
            "macro_rules! m { ($p:literal) => { #[path = $p] mod g; }; }",
            "macro_rules! m { ($($a:tt)*) => { #[$($a)*] mod g; }; }\nm!(path = \"../x.rs\");",
            "m!(path = \"../x.rs\");",
            "const P: &str = include_str!(r\"../x\");",
            "const P: &str = include_str!(r##\"../x\"##);",
            "const P: &str = include_str!(\"x\", \"y\");",
        ] {
            assert!(count(bad) >= 1, "{bad}");
        }
    }

    #[test]
    fn comments_and_strings_are_not_code() {
        let ok = "// include_str!(\"../x\") in a comment\n/* include!(\"../y\") /* nested */ still */\n\
                  /// #[path = \"../z\"]\nconst S: &str = \"include_str!(\\\"../x\\\")\";\n\
                  const R: &str = r#\"#[path = \"../q\"]\"#;\nlet c = '\"'; let l: &'static str = \"\";\n\
                  fn f(include: bool, path: &str) { let _ = (include, path); }\nlet path = 1;\n";
        assert_eq!(scan_text("a.rs", ok), Vec::<String>::new());
    }

    #[test]
    fn odd_literals_do_not_desynchronise_the_lexer() {
        // After each oddity an include must still be seen, exactly once.
        for prefix in [
            "let q = '\\''; let d = '\"'; let n = '\\n';",
            "let b = b'\"'; let s = b\"x\\\"y\"; let r = br#\"a\"b\"#;",
            "fn f<'a>(x: &'a str) -> &'a str { x } 'outer: loop { break 'outer; }",
            "let x = 1_000u32 + 0xFFu32; let y = 1e5;",
            "let z = \"multi\nline\nstring\";",
        ] {
            let src = format!("{prefix}\nconst P: &str = include_str!(\"../x\");\n");
            assert_eq!(count(&src), 1, "{prefix}");
        }
    }

    #[test]
    fn reports_the_line_of_the_macro_name() {
        let bad = scan_text(
            "a.rs",
            "fn a() {}\n\nconst P: &str = include_str\n!(\"../x\");\n",
        );
        assert_eq!(bad.len(), 1);
        assert!(bad[0].starts_with("a.rs:3:"), "{bad:?}");
    }
}
