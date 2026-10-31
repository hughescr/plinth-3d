//! The checker's JSON report (versioned). Skeleton stage: the files are read and their
//! structure is checked, and every hard-limit rule (H1 to H9) is reported "unverified" until
//! its measurement is built. No rule can report "pass" yet.

use crate::chitubox::{self, ChituFile};
use crate::json::Json;
use crate::limits::Limits;
use crate::{layout, sha256, status, stl};

pub const REPORT_VERSION: u64 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    Pass,
    Violation,
    Unverified,
    NotApplicable,
}

impl Status {
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::Pass => "pass",
            Self::Violation => "violation",
            Self::Unverified => "unverified",
            Self::NotApplicable => "not_applicable",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuleResult {
    pub id: &'static str,
    pub name: &'static str,
    pub status: Status,
    pub reason: String,
}

/// The overall answer. `Fail` also covers a file the checker could not read.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Verdict {
    Pass,
    Unverified,
    Fail,
}

impl Verdict {
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::Pass => "pass",
            Self::Unverified => "unverified",
            Self::Fail => "fail",
        }
    }

    /// Process exit code: 0 pass, 2 unverified, 1 fail. (4 is for usage and I/O errors.)
    #[must_use]
    pub fn exit_code(self) -> i32 {
        match self {
            Self::Pass => 0,
            Self::Fail => 1,
            Self::Unverified => 2,
        }
    }
}

const RULES: [(&str, &str); 9] = [
    ("H1", "contact patch area"),
    ("H2", "no fused contacts"),
    ("H3", "print-order continuity"),
    ("H4", "model lifted above the plate"),
    ("H5", "cavities drain and vent, nothing in a drain bore"),
    ("H6", "gap outside contact-surrounding regions"),
    ("H7", "model fidelity against the input STL"),
    ("H8", "opens and slices in ChituBox"),
    ("H9", "support part thickness"),
];

/// Every rule as "unverified" with the reason it cannot be decided.
#[must_use]
pub fn rules_unverified(reason_all: &str) -> Vec<RuleResult> {
    RULES
        .iter()
        .map(|(id, name)| {
            let reason = if *id == "H8" {
                "H8 is a record that ChituBox opened and sliced this exact file; the checker cannot measure it".to_owned()
            } else {
                reason_all.to_owned()
            };
            RuleResult { id, name, status: Status::Unverified, reason }
        })
        .collect()
}

/// The overall verdict from the rule results and the structure checks.
#[must_use]
pub fn verdict(rules: &[RuleResult], structure_ok: bool) -> Verdict {
    if !structure_ok || rules.iter().any(|r| r.status == Status::Violation) {
        Verdict::Fail
    } else if rules.iter().any(|r| r.status == Status::Unverified) {
        Verdict::Unverified
    } else {
        Verdict::Pass
    }
}

/// A file named on the command line, read or not.
#[derive(Debug)]
pub struct Input {
    pub path: String,
    pub data: Result<Vec<u8>, String>,
}

/// The finished report and its verdict.
#[derive(Debug)]
pub struct Report {
    pub json: Json,
    pub verdict: Verdict,
}

fn f3(v: [f32; 3]) -> Json {
    Json::Arr(v.iter().map(|x| Json::Num(f64::from(*x))).collect())
}

fn file_head(input: &Input) -> Vec<(&'static str, Json)> {
    let mut v = vec![("path", Json::str(&input.path))];
    match &input.data {
        Ok(d) => {
            v.push(("bytes", Json::Uint(crate::bytes::wide(d.len()))));
            v.push(("sha256", Json::str(sha256::hex(d))));
        }
        Err(e) => v.push(("read_error", Json::str(e))),
    }
    v
}

fn model_json(m: &chitubox::Model) -> Json {
    Json::obj([
        ("name", Json::str(&m.name)),
        ("translate", f3(m.translate)),
        ("triangles", Json::Uint(m.triangles())),
        (
            "support_records",
            m.support
                .as_ref()
                .map_or(Json::Null, |s| Json::Uint(u64::from(s.index.n_records))),
        ),
        (
            "presets",
            m.front.as_ref().map_or(Json::Null, |f| {
                Json::Arr(f.presets.iter().map(|p| Json::str(&p.name)).collect())
            }),
        ),
    ])
}

fn chitubox_pairs(f: &ChituFile, differences: &[String]) -> Vec<(&'static str, Json)> {
    let preview = f.preview.as_ref().map_or(Json::Null, |p| {
        Json::obj([
            ("width", Json::Uint(u64::from(p.width))),
            ("height", Json::Uint(u64::from(p.height))),
            ("data_length", Json::Uint(u64::from(p.data_len))),
            ("pixels_decoded", p.pixels.map_or(Json::Null, Json::Uint)),
        ])
    });
    vec![
        (
            "models",
            Json::Arr(f.models.iter().map(model_json).collect()),
        ),
        ("preview", preview),
        ("regions", Json::Uint(crate::bytes::wide(f.regions.len()))),
        ("regions_tile_the_file", Json::Bool(f.tiles)),
        ("anomalies", Json::strings(&f.anomalies)),
        ("layout_walk_differences", Json::strings(differences)),
    ]
}

/// Read the .chitubox input. Returns its JSON and whether it was read with a clean structure.
fn chitubox_input(input: &Input) -> (Json, bool) {
    let mut head = file_head(input);
    let mut ok = false;
    match input.data.as_ref().map(|d| (chitubox::parse(d), d)) {
        Ok((Ok(f), data)) => {
            let diff = layout::walk(data).map_or_else(
                |e| vec![format!("walk failed: {e}")],
                |w| layout::diff(&f.regions, &w),
            );
            ok = f.anomalies.is_empty() && f.tiles && diff.is_empty();
            head.push(("readable", Json::Bool(true)));
            head.extend(chitubox_pairs(&f, &diff));
        }
        Ok((Err(e), _)) => {
            head.push(("readable", Json::Bool(false)));
            head.push(("parse_error", Json::str(e.0)));
        }
        Err(_) => head.push(("readable", Json::Bool(false))),
    }
    (Json::from_pairs(head), ok)
}

fn stl_input(input: &Input) -> (Json, bool) {
    let mut head = file_head(input);
    let mut ok = false;
    match input.data.as_ref().map(|d| stl::parse(d)) {
        Ok(Ok(m)) => {
            ok = true;
            head.push(("readable", Json::Bool(true)));
            head.push(("format", Json::str(m.format.name())));
            head.push(("triangles", Json::Uint(m.triangles())));
            if let Some((lo, hi)) = m.bounds() {
                head.push(("bounds_min", f3(lo)));
                head.push(("bounds_max", f3(hi)));
            }
            head.push(("signed_volume_mm3", Json::Num(m.signed_volume())));
        }
        Ok(Err(e)) => {
            head.push(("readable", Json::Bool(false)));
            head.push(("parse_error", Json::str(e.0)));
        }
        Err(_) => head.push(("readable", Json::Bool(false))),
    }
    (Json::from_pairs(head), ok)
}

fn rules_json(rules: &[RuleResult]) -> Json {
    Json::Arr(
        rules
            .iter()
            .map(|r| {
                Json::obj([
                    ("id", Json::str(r.id)),
                    ("name", Json::str(r.name)),
                    ("status", Json::str(r.status.name())),
                    ("reason", Json::str(&r.reason)),
                ])
            })
            .collect(),
    )
}

fn summary_json(rules: &[RuleResult]) -> Json {
    let count = |s: Status| {
        Json::Uint(crate::bytes::wide(
            rules.iter().filter(|r| r.status == s).count(),
        ))
    };
    Json::obj([
        ("pass", count(Status::Pass)),
        ("violation", count(Status::Violation)),
        ("unverified", count(Status::Unverified)),
        ("not_applicable", count(Status::NotApplicable)),
    ])
}

/// Build the report for one .chitubox file and, optionally, the STL it was made from.
#[must_use]
pub fn build(limits: &Limits, chitubox: &Input, stl: Option<&Input>) -> Report {
    let (chitubox_part, chitubox_ok) = chitubox_input(chitubox);
    let stl_part = stl.map(stl_input);
    let structure_ok = chitubox_ok && stl_part.as_ref().is_none_or(|(_, ok)| *ok);
    let reason = if structure_ok {
        "measurement not built yet"
    } else {
        "not decided: an input file is unreadable or does not match the format"
    };
    let rules = rules_unverified(reason);
    let verdict = verdict(&rules, structure_ok);
    let mut json = Json::obj([
        ("report_version", Json::Uint(REPORT_VERSION)),
        (
            "tool",
            Json::obj([
                ("name", Json::str("plinth-check")),
                ("version", Json::str(env!("CARGO_PKG_VERSION"))),
            ]),
        ),
        (
            "limits",
            Json::Obj(
                limits
                    .named()
                    .iter()
                    .map(|(k, v)| ((*k).to_owned(), Json::Num(*v)))
                    .collect(),
            ),
        ),
        (
            "inputs",
            Json::obj([
                ("chitubox", chitubox_part),
                ("stl", stl_part.map_or(Json::Null, |(j, _)| j)),
            ]),
        ),
        ("rules", rules_json(&rules)),
        ("summary", summary_json(&rules)),
        ("verdict", Json::str(verdict.name())),
    ]);
    // Geometry acceptance apart from H8, and the rejection list (design section 7.5). The
    // fields are additive: `verdict` and `rules` keep their meaning.
    if let Json::Obj(pairs) = &mut json {
        pairs.extend(
            status::fields(&rules, structure_ok)
                .into_iter()
                .map(|(k, v)| (k.to_owned(), v)),
        );
    }
    Report { json, verdict }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn limits() -> Limits {
        Limits {
            h1_patch_area_mm2: 0.5,
            h4_lift_mm: 5.0,
            h6_gap_mm: 0.3,
            h9_thickness_mm: 0.1,
            region_length_mm: 3.0,
        }
    }

    #[test]
    fn every_rule_is_unverified_and_none_passes() {
        let rules = rules_unverified("later");
        let ids: Vec<&str> = rules.iter().map(|r| r.id).collect();
        assert_eq!(ids, ["H1", "H2", "H3", "H4", "H5", "H6", "H7", "H8", "H9"]);
        assert!(rules.iter().all(|r| r.status == Status::Unverified));
        assert_eq!(verdict(&rules, true), Verdict::Unverified);
        assert_eq!(verdict(&rules, false), Verdict::Fail);
    }

    #[test]
    fn verdict_is_pass_only_when_every_rule_is_decided_and_clean() {
        let mut rules = rules_unverified("x");
        for r in &mut rules {
            r.status = Status::Pass;
        }
        assert_eq!(verdict(&rules, true), Verdict::Pass);
        rules[3].status = Status::NotApplicable;
        assert_eq!(verdict(&rules, true), Verdict::Pass);
        rules[0].status = Status::Violation;
        assert_eq!(verdict(&rules, true), Verdict::Fail);
        assert_eq!(Verdict::Fail.exit_code(), 1);
        assert_eq!(Verdict::Unverified.exit_code(), 2);
    }

    #[test]
    fn an_unreadable_file_fails_and_every_rule_stays_unverified() {
        let bad = Input {
            path: "x.chitubox".into(),
            data: Ok(vec![1, 2, 3]),
        };
        let r = build(&limits(), &bad, None);
        assert_eq!(r.verdict, Verdict::Fail);
        let text = r.json.render();
        assert!(text.contains("\"parse_error\""), "{text}");
        assert!(text.contains("\"unverified\": 9"), "{text}");
        // The geometry / H8 split: an unreadable file is a geometry violation, H8 is
        // pending, and nothing is ready for review or credible to print.
        assert!(text.contains("\"acceptance\": \"violation\""), "{text}");
        assert!(text.contains("\"state\": \"pending\""), "{text}");
        assert!(text.contains("\"ready_for_owner_review\": false"), "{text}");
        assert!(text.contains("\"credible_to_print\": false"), "{text}");
        assert!(text.contains("\"rule\": \"STRUCTURE\""), "{text}");
        let missing = Input {
            path: "y".into(),
            data: Err("no such file".into()),
        };
        assert_eq!(build(&limits(), &missing, None).verdict, Verdict::Fail);
    }
}
