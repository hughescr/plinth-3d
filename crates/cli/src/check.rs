//! Running the independent checker (`plinth-check`) on the exact bytes of a written file.
//!
//! The two programs share only the command line and the versioned JSON report, never Rust
//! types (design section 4.4). The answer fails closed: a checker that cannot start, dies,
//! gives an exit code that disagrees with its own report, reports another file than the one
//! written (another size, or another SHA-256 than this program computes itself), lists
//! anything but rules H1 to H9, or gives a verdict that its own rule and structure entries
//! contradict, is a failure. Nothing is published unless the checker read the file cleanly.

use crate::json::Json;
use crate::{jsonin, sha256};
use std::path::Path;
use std::process::{Command, Stdio};

/// The report version this program understands.
pub const REPORT_VERSION: u64 = 1;

/// The checker's overall answer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Verdict {
    /// Every rule is decided and clean.
    Pass,
    /// The file is readable and no rule is violated, but some rule is not decided.
    Unverified,
    /// A violation, or a file the checker cannot read.
    Fail,
}

impl Verdict {
    /// The word the checker's report uses.
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::Pass => "pass",
            Self::Unverified => "unverified",
            Self::Fail => "fail",
        }
    }

    fn of_exit(code: i32) -> Option<Self> {
        match code {
            0 => Some(Self::Pass),
            2 => Some(Self::Unverified),
            1 => Some(Self::Fail),
            _ => None,
        }
    }

    fn of_name(name: &str) -> Option<Self> {
        [Self::Pass, Self::Unverified, Self::Fail]
            .into_iter()
            .find(|v| v.name() == name)
    }

    /// True when the file may be published (a failed file never is).
    #[must_use]
    pub fn publishable(self) -> bool {
        self != Self::Fail
    }
}

/// One rule's status as the checker reported it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Rule {
    /// `H1` to `H9`.
    pub id: String,
    /// `pass`, `violation`, `unverified` or `not_applicable`.
    pub status: String,
    /// The checker's reason.
    pub reason: String,
}

/// What the checker said about one file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Checked {
    /// The overall verdict.
    pub verdict: Verdict,
    /// The SHA-256 of the written bytes (the checker's answer matched it).
    pub sha256: String,
    /// Every rule, in the checker's order.
    pub rules: Vec<Rule>,
    /// What the checker found wrong with the file's structure (empty for a clean file).
    pub structure_notes: Vec<String>,
}

/// Why an input file of the report does not count as cleanly read, as text. Empty when the
/// report says the file was read and its structure is sound. A missing field is a problem,
/// never a pass.
fn structure_notes(label: &str, file: &Json, walk_checks: bool) -> Vec<String> {
    let mut notes = Vec::new();
    let readable = file.get("readable") == Some(&Json::Bool(true));
    if let Some(e) = file.get("parse_error").and_then(Json::as_str) {
        notes.push(format!("{label} unreadable: {e}"));
    } else if !readable {
        notes.push(format!("{label} unreadable"));
    }
    if walk_checks && readable {
        for key in ["anomalies", "layout_walk_differences"] {
            match file.get(key).and_then(Json::as_arr) {
                Some(items) => notes.extend(
                    items
                        .iter()
                        .map(|i| format!("{label} {key}: {}", i.as_str().unwrap_or("?"))),
                ),
                None => notes.push(format!("{label} report lacks {key}")),
            }
        }
        if file.get("regions_tile_the_file") != Some(&Json::Bool(true)) {
            notes.push(format!("{label}: the regions do not tile the file"));
        }
    }
    notes
}

/// The nine rules a report must list, once each.
const RULE_IDS: [&str; 9] = ["H1", "H2", "H3", "H4", "H5", "H6", "H7", "H8", "H9"];

/// The statuses a rule may have.
const STATUSES: [&str; 4] = ["pass", "violation", "unverified", "not_applicable"];

impl Checked {
    /// Ids of the rules the checker reported as `status`, in report order.
    #[must_use]
    pub fn ids_with(&self, status: &str) -> Vec<&str> {
        self.rules
            .iter()
            .filter(|r| r.status == status)
            .map(|r| r.id.as_str())
            .collect()
    }

    /// Everything that forbids offering this file as a print candidate, one line each; empty
    /// only when the checker passed the file, no applicable rule is unverified or violated,
    /// and the structure is sound. An unverified rule is a blocker, never a pass (design
    /// dev-loop.md sections 6 and 10 step 8).
    #[must_use]
    pub fn candidate_blockers(&self) -> Vec<String> {
        let mut out = Vec::new();
        if self.verdict != Verdict::Pass {
            out.push(format!("the checker's verdict is {}", self.verdict.name()));
        }
        for r in &self.rules {
            if r.status == "unverified" || r.status == "violation" {
                out.push(format!("{} is {}: {}", r.id, r.status, r.reason));
            }
        }
        for n in &self.structure_notes {
            out.push(format!("structure: {n}"));
        }
        out
    }

    /// The JSON form for the run report.
    #[must_use]
    pub fn to_json(&self) -> Json {
        let rules = self
            .rules
            .iter()
            .map(|r| {
                Json::obj([
                    ("id", Json::str(&r.id)),
                    ("status", Json::str(&r.status)),
                    ("reason", Json::str(&r.reason)),
                ])
            })
            .collect();
        Json::obj([
            ("verdict", Json::str(self.verdict.name())),
            ("file_sha256", Json::str(&self.sha256)),
            (
                "structure_notes",
                Json::Arr(self.structure_notes.iter().map(Json::str).collect()),
            ),
            ("rules", Json::Arr(rules)),
        ])
    }
}

/// The files one checker run uses.
#[derive(Debug, Clone, Copy)]
pub struct Job<'a> {
    /// The `plinth-check` program.
    pub checker: &'a Path,
    /// The limits file.
    pub limits: &'a Path,
    /// The written file, under its temp name.
    pub chitubox: &'a Path,
    /// The input STL the file was made from.
    pub stl: &'a Path,
    /// Where the checker writes its report (removed afterwards).
    pub report: &'a Path,
    /// The bytes that were written: the checker's answer must be about exactly these.
    pub written: &'a [u8],
}

fn rule_of(j: &Json) -> Option<Rule> {
    Some(Rule {
        id: j.get("id")?.as_str()?.to_owned(),
        status: j.get("status")?.as_str()?.to_owned(),
        reason: j.get("reason")?.as_str()?.to_owned(),
    })
}

/// The rules of a report: exactly H1 to H9, each once, each with an allowed status.
fn rules_of(j: &Json) -> Result<Vec<Rule>, String> {
    let rules: Vec<Rule> = j
        .get("rules")
        .and_then(Json::as_arr)
        .ok_or("checker report has no rules")?
        .iter()
        .map(rule_of)
        .collect::<Option<_>>()
        .ok_or("a checker rule entry is malformed")?;
    let mut ids: Vec<&str> = rules.iter().map(|r| r.id.as_str()).collect();
    ids.sort_unstable();
    if ids != RULE_IDS {
        let listed: Vec<&str> = rules.iter().map(|r| r.id.as_str()).collect();
        return Err(format!(
            "checker report must list H1 to H9 once each, but lists: {}",
            listed.join(", ")
        ));
    }
    if let Some(r) = rules
        .iter()
        .find(|r| !STATUSES.contains(&r.status.as_str()))
    {
        return Err(format!(
            "checker rule {} has the unknown status \"{}\"",
            r.id, r.status
        ));
    }
    Ok(rules)
}

/// The verdict the rules and the structure notes call for, by the checker's own definition:
/// fail on a structural problem or a violation, unverified while any rule is undecided,
/// pass otherwise.
fn verdict_called_for(rules: &[Rule], notes: &[String]) -> Verdict {
    let any = |status: &str| rules.iter().any(|r| r.status == status);
    if !notes.is_empty() || any("violation") {
        Verdict::Fail
    } else if any("unverified") {
        Verdict::Unverified
    } else {
        Verdict::Pass
    }
}

/// Read the checker's report text and cross-check it against its exit code, its own rule
/// and structure entries, and the exact bytes that were written: same size and same
/// SHA-256, computed here, not copied from the report.
///
/// # Errors
/// A message naming what does not add up.
pub fn interpret(exit: i32, report: &[u8], written: &[u8]) -> Result<Checked, String> {
    let by_exit = Verdict::of_exit(exit)
        .ok_or_else(|| format!("checker exit code {exit} (usage or I/O error, not a verdict)"))?;
    let j = jsonin::parse(report).map_err(|e| format!("checker report is not JSON: {e}"))?;
    if j.get("report_version").and_then(Json::as_u64) != Some(REPORT_VERSION) {
        return Err(format!("checker report is not version {REPORT_VERSION}"));
    }
    let by_report = j
        .get("verdict")
        .and_then(Json::as_str)
        .and_then(Verdict::of_name)
        .ok_or("checker report has no verdict")?;
    if by_report != by_exit {
        return Err(format!(
            "checker exit code says {} but its report says {}",
            by_exit.name(),
            by_report.name()
        ));
    }
    let inputs = j.get("inputs").ok_or("checker report has no inputs")?;
    let file = inputs
        .get("chitubox")
        .ok_or("checker report has no chitubox input")?;
    let stl = inputs.get("stl").ok_or("checker report has no stl input")?;
    if file.get("bytes").and_then(Json::as_u64) != Some(written.len() as u64) {
        return Err(format!(
            "checker read a file of another size than the {} bytes written",
            written.len()
        ));
    }
    let expected = sha256::hex(written);
    let reported = file
        .get("sha256")
        .and_then(Json::as_str)
        .ok_or("checker report has no file hash")?;
    if !reported.eq_ignore_ascii_case(&expected) {
        return Err(format!(
            "checker read another file than the one written (its SHA-256 is {reported}, the written bytes hash to {expected})"
        ));
    }
    let rules = rules_of(&j)?;
    let mut notes = structure_notes("chitubox", file, true);
    notes.extend(structure_notes("stl", stl, false));
    let called_for = verdict_called_for(&rules, &notes);
    if called_for != by_exit {
        return Err(format!(
            "checker says {} but its own rule and structure entries call for {}",
            by_exit.name(),
            called_for.name()
        ));
    }
    Ok(Checked {
        verdict: by_exit,
        sha256: expected,
        rules,
        structure_notes: notes,
    })
}

/// Run the checker on the written file and interpret its answer. The temp report file is
/// removed in every case.
///
/// # Errors
/// A message when the checker cannot start, is killed, or gives an answer that
/// [`interpret`] refuses.
pub fn run(job: &Job) -> Result<Checked, String> {
    let out = command(job)
        .stdin(Stdio::null())
        .output()
        .map_err(|e| format!("cannot run the checker {}: {e}", job.checker.display()))?;
    conclude(job, out.status.code(), &out.stderr).map(|(checked, _)| checked)
}

fn command(job: &Job) -> Command {
    let mut c = Command::new(job.checker);
    c.arg("check")
        .arg("--chitubox")
        .arg(job.chitubox)
        .arg("--limits")
        .arg(job.limits)
        .arg("--stl")
        .arg(job.stl)
        .arg("--out")
        .arg(job.report);
    c
}

/// Read the report the checker left, remove it, and interpret it with the exit code.
/// Returns the raw report bytes with the answer.
fn conclude(job: &Job, code: Option<i32>, stderr: &[u8]) -> Result<(Checked, Vec<u8>), String> {
    let report = std::fs::read(job.report);
    let _ = std::fs::remove_file(job.report);
    let Some(code) = code else {
        return Err("the checker was killed by a signal".into());
    };
    let report = report.map_err(|e| {
        let said = String::from_utf8_lossy(stderr);
        format!(
            "checker exit {code} and no report ({e}): {}",
            said.lines().next().unwrap_or("")
        )
    })?;
    interpret(code, &report, job.written).map(|checked| (checked, report))
}

/// [`run`] for review mode: the checker is stopped if it has not finished within `timeout`
/// (a refusal naming the time, never a pass), and the raw report is returned beside the
/// answer so a retained file can carry it.
///
/// # Errors
/// As [`run`], and a message when the timeout passes.
pub fn run_bounded(job: &Job, timeout: std::time::Duration) -> Result<(Checked, Vec<u8>), String> {
    use std::io::Read as _;
    let mut child = command(job)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("cannot run the checker {}: {e}", job.checker.display()))?;
    let mut pipe = child.stderr.take();
    let reader = std::thread::spawn(move || {
        let mut said = Vec::new();
        if let Some(p) = pipe.as_mut() {
            let _ = p.read_to_end(&mut said);
        }
        said
    });
    let started = std::time::Instant::now();
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) if started.elapsed() < timeout => {
                std::thread::sleep(std::time::Duration::from_millis(10));
            }
            Ok(None) => {
                let _ = child.kill();
                let _ = child.wait();
                let _ = std::fs::remove_file(job.report);
                return Err(format!(
                    "the checker did not finish within {:.1} s and was stopped",
                    timeout.as_secs_f64()
                ));
            }
            Err(e) => {
                let _ = child.kill();
                let _ = std::fs::remove_file(job.report);
                return Err(format!("cannot wait for the checker: {e}"));
            }
        }
    };
    let said = reader.join().unwrap_or_default();
    conclude(job, status.code(), &said)
}

#[cfg(test)]
mod tests {
    use super::*;

    const WRITTEN: &[u8] = b"pretend chitubox bytes";

    /// The rules H1 to H9, all with `status` except the ones in `except`.
    fn rules(status: &str, except: &[(&str, &str)]) -> String {
        RULE_IDS
            .iter()
            .map(|id| {
                let s = except.iter().find(|e| e.0 == *id).map_or(status, |e| e.1);
                format!(r#"{{"id":"{id}","name":"x","status":"{s}","reason":"later"}}"#)
            })
            .collect::<Vec<_>>()
            .join(",")
    }

    /// A chitubox input entry that the checker would write for a clean file.
    const CLEAN: &str = r#""readable":true,"regions_tile_the_file":true,"anomalies":[],"layout_walk_differences":[]"#;

    /// A report in the checker's shape. `file` is the inside of the chitubox input entry
    /// after bytes and hash.
    fn report(verdict: &str, hash_of: &[u8], file: &str, rules: &str) -> String {
        let (bytes, sha) = (hash_of.len(), sha256::hex(hash_of));
        format!(
            r#"{{"report_version":1,"verdict":"{verdict}","inputs":{{"chitubox":{{"bytes":{bytes},"sha256":"{sha}",{file}}},"stl":{{"readable":true}}}},"rules":[{rules}]}}"#
        )
    }

    fn clean_unverified() -> String {
        report("unverified", WRITTEN, CLEAN, &rules("unverified", &[]))
    }

    fn refused(code: i32, text: &str) -> String {
        interpret(code, text.as_bytes(), WRITTEN).unwrap_err()
    }

    #[test]
    fn a_clean_unverified_answer_is_read() {
        let c = interpret(2, clean_unverified().as_bytes(), WRITTEN).unwrap();
        assert_eq!(c.verdict, Verdict::Unverified);
        assert!(c.verdict.publishable());
        assert_eq!(c.ids_with("unverified").len(), 9);
        assert_eq!(c.sha256, sha256::hex(WRITTEN));
        assert!(c.structure_notes.is_empty());
    }

    #[test]
    fn a_clean_pass_with_every_rule_decided_is_read() {
        let text = report(
            "pass",
            WRITTEN,
            CLEAN,
            &rules("pass", &[("H8", "not_applicable")]),
        );
        let c = interpret(0, text.as_bytes(), WRITTEN).unwrap();
        assert_eq!(c.verdict, Verdict::Pass);
    }

    #[test]
    fn every_disagreement_fails_closed() {
        let ok = clean_unverified();
        let other = b"pretend chitubox bytes!";
        let cases = [
            (0, ok.clone()), // exit code and report disagree
            (4, ok.clone()), // usage error is not a verdict
            (
                2,
                report("unverified", other, CLEAN, &rules("unverified", &[])),
            ), // another file
            (2, report("unverified", WRITTEN, CLEAN, "")), // no rules
            (
                2,
                ok.replace("\"report_version\":1", "\"report_version\":2"),
            ),
            (2, "not json".to_owned()),
            (0, report("pass", WRITTEN, CLEAN, &rules("unverified", &[]))), // pass, unverified rules
            (2, report("unverified", WRITTEN, CLEAN, r#"{"id":"H1"}"#)),
        ];
        for (code, text) in cases {
            assert!(interpret(code, text.as_bytes(), WRITTEN).is_err(), "{text}");
        }
    }

    #[test]
    fn the_hash_must_match_the_written_bytes_not_only_the_size() {
        // Same length, different content: the case a size check cannot see.
        let same_size = b"PRETEND CHITUBOX BYTES";
        assert_eq!(same_size.len(), WRITTEN.len());
        let text = report("unverified", same_size, CLEAN, &rules("unverified", &[]));
        let e = refused(2, &text);
        assert!(e.contains("another file"), "{e}");
    }

    #[test]
    fn a_verdict_the_rules_contradict_is_refused() {
        // Unverified verdict, but a rule is violated.
        let t = report(
            "unverified",
            WRITTEN,
            CLEAN,
            &rules("unverified", &[("H1", "violation")]),
        );
        assert!(refused(2, &t).contains("call for fail"));
        // Unverified verdict, but the file is unreadable.
        let t = report(
            "unverified",
            WRITTEN,
            r#""readable":false"#,
            &rules("unverified", &[]),
        );
        assert!(refused(2, &t).contains("call for fail"));
        // Unverified verdict, but the regions do not tile the file.
        let t = report(
            "unverified",
            WRITTEN,
            r#""readable":true,"regions_tile_the_file":false,"anomalies":[],"layout_walk_differences":[]"#,
            &rules("unverified", &[]),
        );
        assert!(refused(2, &t).contains("call for fail"));
        // Unverified verdict, but an anomaly or a walk difference is listed.
        for field in ["anomalies", "layout_walk_differences"] {
            let file = CLEAN.replace(
                &format!(r#""{field}":[]"#),
                &format!(r#""{field}":["odd"]"#),
            );
            let t = report("unverified", WRITTEN, &file, &rules("unverified", &[]));
            assert!(refused(2, &t).contains("call for fail"), "{field}");
        }
        // Pass verdict, but a rule is violated.
        let t = report(
            "pass",
            WRITTEN,
            CLEAN,
            &rules("pass", &[("H4", "violation")]),
        );
        assert!(refused(0, &t).contains("call for fail"));
        // Pass verdict, but a rule is unverified.
        let t = report(
            "pass",
            WRITTEN,
            CLEAN,
            &rules("pass", &[("H4", "unverified")]),
        );
        assert!(refused(0, &t).contains("call for unverified"));
        // A fail verdict with nothing wrong anywhere.
        let t = report("fail", WRITTEN, CLEAN, &rules("unverified", &[]));
        assert!(refused(1, &t).contains("call for unverified"));
    }

    #[test]
    fn fields_the_verdict_depends_on_cannot_simply_be_missing() {
        // A readable file with no anomalies, walk or tiling entries is not a clean file.
        let t = report(
            "unverified",
            WRITTEN,
            r#""readable":true"#,
            &rules("unverified", &[]),
        );
        assert!(refused(2, &t).contains("call for fail"));
        let t = report("unverified", WRITTEN, "\"x\":1", &rules("unverified", &[]));
        assert!(refused(2, &t).contains("call for fail"));
    }

    #[test]
    fn the_rule_set_must_be_exactly_h1_to_h9() {
        let nine = rules("unverified", &[]);
        let only_h1 = report(
            "pass",
            WRITTEN,
            CLEAN,
            r#"{"id":"H1","status":"pass","reason":"r"}"#,
        );
        assert!(refused(0, &only_h1).contains("H1 to H9"));
        // A duplicate in place of H9.
        let dup = nine.replace(r#""id":"H9""#, r#""id":"H8""#);
        let t = report("unverified", WRITTEN, CLEAN, &dup);
        assert!(refused(2, &t).contains("H1 to H9"));
        // An extra tenth rule.
        let ten = format!(r#"{nine},{{"id":"H10","status":"unverified","reason":"r"}}"#);
        let t = report("unverified", WRITTEN, CLEAN, &ten);
        assert!(refused(2, &t).contains("H1 to H9"));
        // An unknown status.
        let t = report("unverified", WRITTEN, CLEAN, &rules("maybe", &[]));
        assert!(refused(2, &t).contains("unknown status"));
    }

    #[test]
    fn only_a_clean_pass_has_no_candidate_blockers() {
        let pass = report(
            "pass",
            WRITTEN,
            CLEAN,
            &rules("pass", &[("H8", "not_applicable")]),
        );
        let c = interpret(0, pass.as_bytes(), WRITTEN).unwrap();
        assert!(c.candidate_blockers().is_empty());
        // Every rule unverified: nine rule blockers and the verdict.
        let c = interpret(2, clean_unverified().as_bytes(), WRITTEN).unwrap();
        assert_eq!(c.candidate_blockers().len(), 10);
        // One unverified rule among passes blocks.
        let one = report(
            "unverified",
            WRITTEN,
            CLEAN,
            &rules("pass", &[("H3", "unverified")]),
        );
        let c = interpret(2, one.as_bytes(), WRITTEN).unwrap();
        assert!(
            c.candidate_blockers()
                .iter()
                .any(|b| b.starts_with("H3 is unverified"))
        );
        // One violated rule blocks and is named.
        let bad = report(
            "fail",
            WRITTEN,
            CLEAN,
            &rules("pass", &[("H1", "violation")]),
        );
        let c = interpret(1, bad.as_bytes(), WRITTEN).unwrap();
        assert!(
            c.candidate_blockers()
                .iter()
                .any(|b| b.starts_with("H1 is violation"))
        );
    }

    #[test]
    fn a_failed_file_is_not_publishable() {
        let t = report(
            "fail",
            WRITTEN,
            CLEAN,
            &rules("unverified", &[("H2", "violation")]),
        );
        let c = interpret(1, t.as_bytes(), WRITTEN).unwrap();
        assert_eq!(c.verdict, Verdict::Fail);
        assert!(!c.verdict.publishable());
        assert_eq!(c.ids_with("violation"), ["H2"]);
    }

    #[test]
    fn an_unreadable_file_is_a_failure_with_a_note() {
        let t = report(
            "fail",
            WRITTEN,
            r#""readable":false,"parse_error":"bad magic""#,
            &rules("unverified", &[]),
        );
        let c = interpret(1, t.as_bytes(), WRITTEN).unwrap();
        assert!(!c.verdict.publishable());
        assert_eq!(c.structure_notes, ["chitubox unreadable: bad magic"]);
    }

    #[test]
    fn a_missing_checker_is_an_error_not_a_pass() {
        let job = Job {
            checker: Path::new("/nonexistent/plinth-check"),
            limits: Path::new("l"),
            chitubox: Path::new("c"),
            stl: Path::new("s"),
            report: Path::new("/nonexistent/r.json"),
            written: b"x",
        };
        assert!(run(&job).unwrap_err().contains("cannot run the checker"));
    }
}
