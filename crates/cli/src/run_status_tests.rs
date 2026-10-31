use super::*;
use crate::check::{Rule, Verdict};

fn measured_pass() -> Checked {
    Checked {
        verdict: Verdict::Pass,
        sha256: "a".repeat(64),
        structure_notes: vec![],
        rules: (1..=9)
            .map(|i| Rule {
                id: format!("H{i}"),
                status: if i == 5 { "not_applicable" } else { "pass" }.into(),
                reason: "controlled measured result".into(),
                not_applicable_proved: i == 5,
            })
            .collect(),
    }
}
#[test]
fn candidate_opens_only_for_a_complete_measured_pass_without_planning_blockers() {
    let pass = measured_pass();
    assert!(candidate_allowed(true, &pass, false));
    assert!(!candidate_allowed(false, &pass, false));
    assert!(!candidate_allowed(true, &pass, true));
    for i in 0..9 {
        for status in ["unverified", "violation", "not_applicable"] {
            if i == 4 && status == "not_applicable" {
                continue;
            }
            let mut result = pass.clone();
            result.rules[i].status = status.into();
            assert!(
                !candidate_allowed(true, &result, false),
                "H{} {status}",
                i + 1
            );
        }
        let mut result = pass.clone();
        result.rules.remove(i);
        assert!(!candidate_allowed(true, &result, false));
    }
    let mut result = pass.clone();
    result.rules[4].not_applicable_proved = false;
    assert!(!candidate_allowed(true, &result, false));
    let mut result = pass.clone();
    result.verdict = Verdict::Unverified;
    assert!(!candidate_allowed(true, &result, false));
    let mut result = pass.clone();
    result.structure_notes.push("damaged output".into());
    assert!(!candidate_allowed(true, &result, false));
}

#[test]
fn json_status_uses_the_same_measured_gate_not_publication_alone() {
    let cfg = Config {
        inputs: vec!["input.stl".into()],
        out_dir: "out".into(),
        max_threads: 3,
        checker: "checker".into(),
        limits: "limits".into(),
        candidate: true,
    };
    for (checked, blocked, expected) in [
        (measured_pass(), false, true),
        (measured_pass(), true, false),
        (
            {
                let mut c = measured_pass();
                c.rules[0].status = "unverified".into();
                c
            },
            false,
            false,
        ),
    ] {
        let run = Run {
            models: vec![ModelRun {
                input: "input.stl".into(),
                stem: "input".into(),
                triangles: 12,
                steps: vec![],
                parallel: None,
                supported: true,
                planning_blockers: blocked,
                outcome: Outcome::Published {
                    path: "out/file.chitubox".into(),
                    bytes: 1,
                    checked,
                },
            }],
            total_secs: 0.0,
            peak: Err("test".into()),
        };
        let json = crate::report::to_json(&cfg, &run);
        assert_eq!(
            json.get("print_candidate"),
            Some(&crate::json::Json::Bool(expected))
        );
        let model = &json
            .get("models")
            .and_then(crate::json::Json::as_arr)
            .unwrap()[0];
        assert_eq!(
            model.get("print_candidate"),
            Some(&crate::json::Json::Bool(expected))
        );
        assert_eq!(
            model.get("kind").and_then(crate::json::Json::as_str),
            Some("supported")
        );
    }
}

#[test]
fn complete_bundle_appears_together_and_destination_is_never_replaced() {
    let root = std::env::temp_dir().join(format!("plinth-bundle-status-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    let staged = publish::temp_path(&root, "bundle");
    std::fs::create_dir(&staged).unwrap();
    let dest = root.join("result.supported");
    for name in ["model.chitubox", "report.json", "01.png"] {
        publish::write_temp(&staged.join(name), b"complete", Fault::None).unwrap();
        assert!(!dest.exists());
    }
    publish::publish_bundle(&staged, &dest).unwrap();
    assert_eq!(std::fs::read_dir(&dest).unwrap().count(), 3);
    assert!(publish::publish_bundle(&staged, &dest).is_err());
    assert_eq!(
        std::fs::read(dest.join("report.json")).unwrap(),
        b"complete"
    );
    std::fs::remove_dir_all(&root).unwrap();
}
