//! Running the broken files through the real checker and judging the answers.

use super::{CASES, Case, Expect, RULES, sound};
use crate::readers_agree::{checker_binary, repo_root};
use plinth_cli::check::{self, Checked, Job, Verdict};
use std::fs;
use std::path::PathBuf;

/// Every case: the table in the parent module and the controls of `mesh_verdict`.
pub(crate) fn all_cases() -> impl Iterator<Item = &'static Case> {
    CASES.iter().chain(super::mesh_verdict::CASES)
}

pub(crate) fn scratch(tag: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("plinth-accept-{tag}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&d);
    fs::create_dir_all(&d).expect("make scratch folder");
    d
}

/// Write `bytes` and `stl` into `dir`, run the real checker through the production hand-off.
pub(crate) fn check_bytes(
    dir: &std::path::Path,
    bytes: &[u8],
    stl: &[u8],
) -> Result<Checked, String> {
    let file = dir.join("f.chitubox");
    let stl_path = dir.join("f.stl");
    let report = dir.join("r.json");
    fs::write(&file, bytes).expect("write file");
    fs::write(&stl_path, stl).expect("write stl");
    let limits = repo_root().join("limits.toml");
    check::run(&Job {
        checker: &checker_binary(),
        limits: &limits,
        chitubox: &file,
        stl: &stl_path,
        report: &report,
        written: bytes,
    })
}

fn status<'a>(c: &'a Checked, id: &str) -> &'a str {
    c.rules
        .iter()
        .find(|r| r.id == id)
        .map_or("missing", |r| r.status.as_str())
}

/// Judge the checker's answer for a case: `Err` says what is wrong.
fn judge(rule: &str, expect: Expect, c: &Checked) -> Result<(), String> {
    let got = status(c, rule);
    if got != expect.word() {
        let reason = c
            .rules
            .iter()
            .find(|r| r.id == rule)
            .map_or("", |r| r.reason.as_str());
        return Err(format!(
            "{rule} is \"{got}\"; expected \"{}\" ({reason})",
            expect.word()
        ));
    }
    if status(c, "H8") == "pass" {
        return Err("H8 passed, but no ChituBox record exists".to_owned());
    }
    if !c.structure_notes.is_empty() {
        return Err(format!(
            "the file must be sound apart from its rule; structure notes: {:?}",
            c.structure_notes
        ));
    }
    if expect == Expect::Violation && c.verdict != Verdict::Fail {
        return Err(format!(
            "verdict {}; a violation must fail the file",
            c.verdict.name()
        ));
    }
    if matches!(expect, Expect::Violation | Expect::Unverified) && c.candidate_blockers().is_empty()
    {
        return Err("the file could be offered as a print candidate".to_owned());
    }
    Ok(())
}

fn run_rule(rule: &str) {
    let cases: Vec<&Case> = all_cases().filter(|c| c.rule == rule).collect();
    assert!(!cases.is_empty(), "{rule} has no broken file");
    for case in cases {
        let built = (case.build)();
        let dir = scratch(&format!(
            "{}-{}",
            case.rule,
            case.name.replace([' ', '.', '(', ')'], "-")
        ));
        let c = check_bytes(&dir, &built.chitubox(), &built.stl_bytes())
            .unwrap_or_else(|e| panic!("{} / {}: the checker's answer: {e}", case.rule, case.name));
        if let Err(why) = judge(rule, case.expect, &c) {
            panic!("{} / {}: {why}", case.rule, case.name);
        }
        let _ = fs::remove_dir_all(&dir);
    }
}

macro_rules! rule_tests {
    ($($name:ident => $rule:literal),* $(,)?) => {
        $( #[test] fn $name() { run_rule($rule); } )*
    };
}

rule_tests! {
    broken_h1_patch_area => "H1",
    broken_h2_fused_contacts => "H2",
    broken_h3_continuity => "H3",
    broken_h4_lift => "H4",
    broken_h5_drains => "H5",
    broken_h6_gap => "H6",
    broken_h7_fidelity => "H7",
    broken_h8_chitubox_record => "H8",
    broken_h9_thickness => "H9",
}

#[test]
fn broken_every_rule_has_a_violation_file_where_it_can_be_measured_and_no_file_is_empty() {
    for id in RULES {
        assert!(all_cases().any(|c| c.rule == id), "{id} has no broken file");
        if id != "H8" {
            assert!(
                all_cases().any(|c| c.rule == id && c.expect == Expect::Violation),
                "{id} has no file that must be a violation"
            );
            assert!(
                all_cases()
                    .any(|c| c.rule == id
                        && matches!(c.expect, Expect::Pass | Expect::NotApplicable)),
                "{id} has no control file"
            );
        }
    }
    for case in all_cases() {
        let built = (case.build)();
        assert!(!built.mesh.is_empty(), "{}: empty model", case.name);
        let bytes = built.chitubox();
        let back = plinth_chitubox::parse(&bytes).expect("the production reader reads it");
        assert_eq!(back.models.len(), 1, "{}", case.name);
        assert!(back.deviations().is_empty(), "{}", case.name);
    }
}

#[test]
fn broken_the_sound_control_has_every_geometry_rule_decided() {
    let built = sound();
    let dir = scratch("sound");
    let c = check_bytes(&dir, &built.chitubox(), &built.stl_bytes()).expect("answer");
    for (id, want) in [
        ("H1", "pass"),
        ("H2", "pass"),
        ("H3", "pass"),
        ("H4", "pass"),
        ("H5", "not_applicable"),
        ("H6", "pass"),
        ("H7", "pass"),
        ("H8", "unverified"),
        ("H9", "pass"),
    ] {
        assert_eq!(status(&c, id), want, "{id}: {:?}", c.rules);
    }
    // Only H8 is left: the file is geometry-accepted but never a print candidate.
    assert_eq!(c.verdict, Verdict::Unverified);
    assert_eq!(
        c.candidate_blockers().len(),
        2,
        "{:?}",
        c.candidate_blockers()
    );
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn broken_a_file_beyond_the_build_volume_is_refused_with_the_reason_named() {
    // Round 8: coordinates outside the printer's build volume make the whole file unverified.
    let cases: Vec<&Case> = super::mesh_verdict::CASES
        .iter()
        .filter(|c| c.name.contains("build volume") || c.name.contains("above the plate"))
        .collect();
    assert_eq!(
        cases.len(),
        2,
        "one file beyond the volume in y and one in z"
    );
    for case in cases {
        let built = (case.build)();
        let dir = scratch("volume");
        let c = check_bytes(&dir, &built.chitubox(), &built.stl_bytes()).expect("answer");
        for r in &c.rules {
            assert_eq!(
                r.status, "unverified",
                "{} / {}: {}",
                case.name, r.id, r.reason
            );
            // H8 always names its own reason: a record the checker cannot see.
            assert!(
                r.id == "H8" || r.reason.contains("build volume"),
                "{} / {}: {}",
                case.name,
                r.id,
                r.reason
            );
        }
        assert_ne!(c.verdict, Verdict::Pass, "{}", case.name);
        assert!(!c.candidate_blockers().is_empty(), "{}", case.name);
        let _ = fs::remove_dir_all(&dir);
    }
}

#[test]
fn broken_the_judge_catches_a_pass_and_a_wrong_status() {
    // The judge is what makes a broken-file test fail when a rule wrongly passes. Prove it
    // fires on a synthetic answer in which every rule passes.
    let rule = |id: &str, s: &str| check::Rule {
        id: id.to_owned(),
        status: s.to_owned(),
        reason: "synthetic".to_owned(),
    };
    let answer = |s: &str, verdict| Checked {
        verdict,
        sha256: String::new(),
        rules: RULES.iter().map(|id| rule(id, s)).collect(),
        structure_notes: Vec::new(),
    };
    let passed = judge("H1", Expect::Violation, &answer("pass", Verdict::Pass)).unwrap_err();
    assert!(passed.contains("expected \"violation\""), "{passed}");
    assert!(judge("H1", Expect::Pass, &answer("violation", Verdict::Fail)).is_err());
    assert!(judge("H1", Expect::Violation, &answer("violation", Verdict::Fail)).is_ok());
    // A violation that does not fail the file is refused.
    assert!(
        judge(
            "H1",
            Expect::Violation,
            &answer("violation", Verdict::Unverified)
        )
        .is_err()
    );
    // H8 passing is refused.
    let mut sneaky = answer("unverified", Verdict::Unverified);
    for r in &mut sneaky.rules {
        if r.id == "H8" {
            "pass".clone_into(&mut r.status);
        }
    }
    assert!(
        judge("H1", Expect::Unverified, &sneaky)
            .unwrap_err()
            .contains("H8")
    );
    // A structural problem is not a rule rejection.
    let mut unsound = answer("unverified", Verdict::Unverified);
    unsound
        .structure_notes
        .push("chitubox unreadable: x".to_owned());
    assert!(judge("H1", Expect::Unverified, &unsound).is_err());
}

// ---- structural breaks -----------------------------------------------------------------------

/// A sound supported file to break.
fn sound_bytes() -> Vec<u8> {
    sound().chitubox()
}

fn put32(b: &mut [u8], at: usize, v: u32) {
    b[at..at + 4].copy_from_slice(&v.to_le_bytes());
}

/// (name, a function that breaks the bytes, text the checker's note must contain: the place
/// or region where the file breaks)
type Mutation = (&'static str, fn(Vec<u8>) -> Vec<u8>, &'static str);

const STRUCTURAL: &[Mutation] = &[
    (
        "truncated",
        |mut b| {
            b.truncate(b.len() - 100);
            b
        },
        "support records: bytes [",
    ),
    ("empty file", |_| Vec::new(), "header magic: bytes [0, 4)"),
    (
        "bad magic",
        |mut b| {
            b[0] ^= 0xff;
            b
        },
        "magic is 0xab2312bc",
    ),
    (
        "model count huge",
        |mut b| {
            put32(&mut b, 4, 0xffff_fff0);
            b
        },
        "model table: bytes [",
    ),
    (
        "model table pointer past the end",
        |mut b| {
            let len = u32::try_from(b.len()).expect("small");
            put32(&mut b, 8, len + 1000);
            b
        },
        "lie past the end of the file",
    ),
    (
        "model table pointer into the header",
        |mut b| {
            put32(&mut b, 8, 4);
            b
        },
        "model table offset is 4, expected",
    ),
    (
        "non-zero byte in the zero block",
        |mut b| {
            b[100] = 1;
            b
        },
        "zero block [24, 420) is not all zero",
    ),
    (
        "trailing bytes",
        |mut b| {
            b.extend_from_slice(&[0xab; 64]);
            b
        },
        "64 bytes after the last region",
    ),
];

#[test]
fn broken_structural_breaks_are_rejected_with_a_located_note() {
    // Control: the unbroken file is read cleanly (so the breaks below are what fail).
    let dir = scratch("structural");
    let good = sound_bytes();
    let stl = sound().stl_bytes();
    let c = check_bytes(&dir, &good, &stl).expect("control answer");
    assert!(c.structure_notes.is_empty(), "{:?}", c.structure_notes);
    assert_eq!(c.verdict, Verdict::Unverified);
    for (name, mutate, place) in STRUCTURAL {
        let broken = mutate(good.clone());
        assert_ne!(broken, good, "{name}: the mutation changed nothing");
        let c = check_bytes(&dir, &broken, &stl).unwrap_or_else(|e| panic!("{name}: {e}"));
        assert_eq!(c.verdict, Verdict::Fail, "{name}");
        let notes = c.structure_notes.join("; ");
        let found = notes.contains(place);
        assert!(found, "{name}: expected {place:?} in {notes}");
        assert!(
            c.rules.iter().all(|r| r.status != "pass"),
            "{name}: a rule passed a broken file"
        );
    }
    let _ = fs::remove_dir_all(&dir);
}
