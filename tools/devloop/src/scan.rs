//! Text rules for the pre-commit hook and the gate (design dev-loop.md sections 7.2 and 9): a
//! scan of the lines a commit adds, a line-count cap, the read-only evidence folders, and the
//! raw-record grep. Pure functions over text; the callers read git.
//!
//! Patterns are built from pieces (`pat`) so this file does not match its own rules.

/// A broken rule at a place.
#[derive(Debug, PartialEq, Eq)]
pub struct Finding {
    pub file: String,
    /// 1-based line in the new file; 0 for a file-level finding.
    pub line: usize,
    pub rule: &'static str,
    pub text: String,
}

/// Join pieces into one pattern.
fn pat(parts: &[&str]) -> String {
    parts.concat()
}

/// Longest `.rs` file allowed, in lines.
pub const MAX_RS_LINES: usize = 1500;
/// How many earlier lines of the new file a rule may look back at (an `allow` attribute with a
/// stated reason sits above the test function that holds the line it excuses: the attribute
/// itself, the signature, and a few lines of body).
const LOOKBACK: usize = 8;

/// Folders that hold read-only evidence.
const PROTECTED: [&str; 2] = ["test-data/", "reference/"];

/// The one file that may read the process environment.
const ENV_MODULE: &str = "crates/cli/src/args.rs";

/// Top-level folder of upstream third-party code, kept as published. Its files are exempt from
/// the house style and structure rules (the line cap and the lint-allow ban) and from nothing
/// else: the safety and integrity rules apply to them too.
const VENDOR_DIR: &str = "vendor/";

fn is_vendored(path: &str) -> bool {
    path.starts_with(VENDOR_DIR)
}

fn is_rust(path: &str) -> bool {
    path.ends_with(".rs")
}

fn is_test_path(path: &str) -> bool {
    path.contains("/tests/") || path.ends_with("testutil.rs")
}

fn is_comment(line: &str) -> bool {
    line.trim_start().starts_with("//")
}

/// The rule a single new line breaks, if any. `window` holds the earlier lines of the new
/// file, nearest last.
#[must_use]
pub fn line_rule(path: &str, line: &str, window: &[&str]) -> Option<&'static str> {
    if !is_rust(path) || is_comment(line) {
        return None;
    }
    if line.contains(&pat(&["#[", "ignore"])) {
        return Some("an ignored test (no hidden skips)");
    }
    if !is_vendored(path)
        && line.contains(&pat(&["too_many", "_arguments"]))
        && line.contains("allow(")
    {
        return Some("an allow for too many arguments (split the function)");
    }
    let guarded = path.starts_with("crates/") || path.starts_with("checker/");
    if guarded && path != ENV_MODULE && line.contains(&pat(&["env", "::var"])) {
        let excused = window
            .iter()
            .rev()
            .take(LOOKBACK)
            .any(|w| w.contains("disallowed_methods"));
        if !excused {
            return Some("reading the environment outside the CLI's argument module");
        }
    }
    let unfinished =
        line.contains(&pat(&["todo", "!("])) || line.contains(&pat(&["unimplemented", "!("]));
    if unfinished && !is_test_path(path) {
        return Some("an unfinished-code macro outside tests");
    }
    None
}

/// Scan whole new-file text (every line is "added").
#[must_use]
pub fn scan_text(path: &str, text: &str) -> Vec<Finding> {
    let lines: Vec<&str> = text.lines().collect();
    let mut out = Vec::new();
    for (i, line) in lines.iter().enumerate() {
        let from = i.saturating_sub(LOOKBACK);
        if let Some(rule) = line_rule(path, line, &lines[from..i]) {
            out.push(Finding {
                file: path.to_owned(),
                line: i + 1,
                rule,
                text: line.trim().to_owned(),
            });
        }
    }
    out
}

/// Scan the added lines of a unified diff (`git diff -U4`). Context lines feed the lookback
/// window; removed lines are ignored.
#[must_use]
pub fn scan_diff(diff: &str) -> Vec<Finding> {
    let mut out = Vec::new();
    let mut file = String::new();
    let mut new_line = 0_usize;
    let mut window: Vec<String> = Vec::new();
    // Between a "diff --git" line and the first hunk, "---" and "+++" lines are file headers;
    // inside a hunk they are ordinary lines (an added line may begin with "++").
    let mut in_header = false;
    for raw in diff.lines() {
        if raw.starts_with("diff --git") {
            in_header = true;
        } else if in_header && raw.starts_with("+++ ") {
            file = raw[4..].strip_prefix("b/").unwrap_or("").to_owned();
            window.clear();
        } else if in_header && raw.starts_with("--- ") {
            // Old-file header: nothing to scan.
        } else if let Some(h) = raw.strip_prefix("@@ ") {
            in_header = false;
            new_line = hunk_start(h);
            window.clear();
        } else if let Some(added) = raw.strip_prefix('+') {
            let recent: Vec<&str> = window.iter().map(String::as_str).collect();
            if let Some(rule) = line_rule(&file, added, &recent) {
                out.push(Finding {
                    file: file.clone(),
                    line: new_line,
                    rule,
                    text: added.trim().to_owned(),
                });
            }
            push_window(&mut window, added);
            new_line += 1;
        } else if let Some(ctx) = raw.strip_prefix(' ') {
            push_window(&mut window, ctx);
            new_line += 1;
        }
    }
    out
}

fn push_window(window: &mut Vec<String>, line: &str) {
    window.push(line.to_owned());
    if window.len() > LOOKBACK {
        window.remove(0);
    }
}

/// The first new-file line of a hunk header body `-a,b +c,d @@ ...`.
fn hunk_start(header: &str) -> usize {
    header
        .split_whitespace()
        .find_map(|t| t.strip_prefix('+'))
        .and_then(|t| t.split(',').next())
        .and_then(|n| n.parse().ok())
        .unwrap_or(1)
}

/// A `.rs` file over the line cap. Vendored sources are exempt (see `VENDOR_DIR`).
#[must_use]
pub fn too_long(path: &str, lines: usize) -> Option<Finding> {
    (is_rust(path) && !is_vendored(path) && lines > MAX_RS_LINES).then(|| Finding {
        file: path.to_owned(),
        line: 0,
        rule: "a Rust file over 1,500 lines (split it)",
        text: format!("{lines} lines"),
    })
}

/// Paths in `name-status` text (`git diff --name-status`) that touch the read-only folders.
/// A rename names two paths; either one counts.
#[must_use]
pub fn protected_changes(name_status: &str) -> Vec<Finding> {
    let mut out = Vec::new();
    for line in name_status.lines() {
        let mut fields = line.split('\t');
        let status = fields.next().unwrap_or("");
        for path in fields {
            if PROTECTED.iter().any(|p| path.starts_with(p)) {
                out.push(Finding {
                    file: path.to_owned(),
                    line: 0,
                    rule: "a change under test-data/ or reference/ (read-only evidence)",
                    text: status.to_owned(),
                });
            }
        }
    }
    out
}

/// The crates whose sources must never build raw support records (design section 7.2).
const NO_RAW_RECORDS: [&str; 3] = ["crates/support/src/", "crates/cli/src/", "crates/geom/src/"];

/// Raw support records built outside `plinth-emit` and the format crate.
#[must_use]
pub fn raw_records(path: &str, text: &str) -> Vec<Finding> {
    if !is_rust(path) || !NO_RAW_RECORDS.iter().any(|p| path.starts_with(p)) {
        return Vec::new();
    }
    let needles = [pat(&["Support", "Record"]), pat(&["Record", "Type::"])];
    text.lines()
        .enumerate()
        .filter(|(_, l)| !is_comment(l) && needles.iter().any(|n| l.contains(n.as_str())))
        .map(|(i, l)| Finding {
            file: path.to_owned(),
            line: i + 1,
            rule: "raw support records outside plinth-emit",
            text: l.trim().to_owned(),
        })
        .collect()
}

/// Print findings, one per line.
pub fn report(findings: &[Finding]) {
    for f in findings {
        if f.line == 0 {
            eprintln!("  {}: {} [{}]", f.file, f.rule, f.text);
        } else {
            eprintln!("  {}:{}: {}: {}", f.file, f.line, f.rule, f.text);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ignore_line() -> String {
        format!("    {}\n    fn slow() {{}}", pat(&["#[", "ignore]"]))
    }

    #[test]
    fn each_rule_fires_on_its_line_and_only_there() {
        let todo = pat(&["    todo", "!();"]);
        let env = pat(&["    let v = std::env", "::var(\"X\");"]);
        let allow = pat(&["#[allow(clippy::too_many", "_arguments)]"]);
        let cases = [
            (ignore_line(), "crates/geom/src/a.rs"),
            (todo.clone(), "crates/geom/src/a.rs"),
            (env.clone(), "crates/geom/src/a.rs"),
            (allow, "crates/geom/src/a.rs"),
        ];
        for (text, path) in cases {
            assert_eq!(scan_text(path, &text).len(), 1, "{text}");
        }
        // The same lines are fine where the rule does not apply.
        assert!(scan_text("crates/geom/tests/it/a.rs", &todo).is_empty());
        assert!(scan_text("crates/cli/src/args.rs", &env).is_empty());
        assert!(scan_text("tools/devloop/src/a.rs", &env).is_empty());
        assert!(scan_text("README.md", &todo).is_empty());
        assert!(scan_text("crates/geom/src/a.rs", "    let ok = 1;").is_empty());
    }

    #[test]
    fn comments_do_not_count() {
        let c = format!("// {}", pat(&["#[", "ignore]"]));
        assert!(scan_text("crates/geom/src/a.rs", &c).is_empty());
    }

    #[test]
    fn an_environment_read_with_a_stated_allow_just_above_passes() {
        let env = pat(&["std::env", "::var(\"NEXTEST_PROFILE\")"]);
        let allowed = format!(
            "#[allow(\n    clippy::disallowed_methods,\n    reason = \"test-only\"\n)]\nfn f() -> bool {{\n    {env}.is_ok()\n}}"
        );
        assert!(scan_text("crates/geom/src/testutil.rs", &allowed).is_empty());
        let bare = format!("fn g() {{\n    let a = 1;\n    {env};\n}}");
        assert_eq!(scan_text("crates/geom/src/b.rs", &bare).len(), 1);
        // An allow far above (more than the lookback) does not excuse the line.
        let filler = "    let x = 0;\n".repeat(LOOKBACK + 1);
        let far = format!(
            "#[allow(clippy::disallowed_methods, reason = \"r\")]\nfn h() {{\n{filler}    {env};\n}}"
        );
        assert_eq!(scan_text("crates/geom/src/b.rs", &far).len(), 1);
    }

    #[test]
    fn a_diff_reports_added_lines_with_their_new_line_numbers() {
        let bad = pat(&["#[", "ignore]"]);
        let diff = format!(
            "diff --git a/crates/geom/src/a.rs b/crates/geom/src/a.rs\n--- a/crates/geom/src/a.rs\n+++ b/crates/geom/src/a.rs\n@@ -3,2 +10,4 @@ fn x\n context\n+fine\n+{bad}\n-removed {bad}\n context\n"
        );
        let found = scan_diff(&diff);
        assert_eq!(found.len(), 1);
        assert_eq!(
            (found[0].file.as_str(), found[0].line),
            ("crates/geom/src/a.rs", 12)
        );
    }

    #[test]
    fn a_removed_line_and_a_context_line_never_count() {
        let bad = pat(&["#[", "ignore]"]);
        let diff = format!(
            "diff --git a/crates/geom/src/a.rs b/crates/geom/src/a.rs\n--- a/crates/geom/src/a.rs\n+++ b/crates/geom/src/a.rs\n@@ -1,2 +1,2 @@\n {bad}\n-{bad}\n+ok\n"
        );
        assert!(scan_diff(&diff).is_empty());
        // An added line that begins with "++" inside a hunk is a line, not a file header.
        let diff = format!(
            "diff --git a/crates/geom/src/a.rs b/crates/geom/src/a.rs\n--- a/crates/geom/src/a.rs\n+++ b/crates/geom/src/a.rs\n@@ -1,1 +1,2 @@\n ok\n+++ {bad}\n"
        );
        assert_eq!(scan_diff(&diff).len(), 1);
    }

    #[test]
    fn the_line_cap_is_exact() {
        assert!(too_long("a.rs", MAX_RS_LINES).is_none());
        assert!(too_long("a.rs", MAX_RS_LINES + 1).is_some());
        assert!(too_long("a.md", 99_999).is_none());
    }

    #[test]
    fn vendored_sources_skip_the_style_rules_and_keep_the_safety_rules() {
        // A 1,501-line file passes under the top-level vendor/ folder and fails everywhere else.
        let long = "fn f() {}\n".repeat(MAX_RS_LINES + 1);
        let lines = long.lines().count();
        let vendored = "vendor/manifold-csg-sys/src/lib.rs";
        assert!(too_long(vendored, lines).is_none());
        assert!(too_long("crates/geom/src/lib.rs", lines).is_some());
        assert!(too_long("crates/geom/vendor/lib.rs", lines).is_some());
        assert!(too_long("vendored/lib.rs", lines).is_some());
        // The lint-allow ban is a style rule, so vendored code may carry the allow.
        let allow = pat(&["#[allow(clippy::too_many", "_arguments)]"]);
        assert!(scan_text(vendored, &allow).is_empty());
        assert_eq!(scan_text("crates/geom/src/a.rs", &allow).len(), 1);
        // The unfinished-code and ignored-test rules are not style rules and still apply.
        let todo = pat(&["    todo", "!();"]);
        assert_eq!(scan_text(vendored, &todo).len(), 1);
        assert_eq!(scan_text(vendored, &ignore_line()).len(), 1);
    }

    #[test]
    fn evidence_folders_are_read_only_even_for_renames_and_deletes() {
        let ns = "M\tcrates/a.rs\nD\ttest-data/x.bin\nR100\tdocs/a.md\treference/b.md\nA\ttest-data-notes.md\n";
        let found = protected_changes(ns);
        let files: Vec<&str> = found.iter().map(|f| f.file.as_str()).collect();
        assert_eq!(files, ["test-data/x.bin", "reference/b.md"]);
    }

    #[test]
    fn raw_records_are_refused_in_the_generator_crates_only() {
        let text = format!(
            "use plinth_chitubox::{};\n// {} in a comment\n",
            pat(&["Support", "Record"]),
            pat(&["Support", "Record"])
        );
        assert_eq!(raw_records("crates/support/src/lib.rs", &text).len(), 1);
        assert_eq!(raw_records("crates/cli/src/run.rs", &text).len(), 1);
        assert!(raw_records("crates/emit/src/lib.rs", &text).is_empty());
        assert!(raw_records("crates/chitubox/src/supports.rs", &text).is_empty());
        assert!(raw_records("crates/cli/tests/it/a.rs", &text).is_empty());
    }
}
