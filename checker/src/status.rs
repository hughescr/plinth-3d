//! Geometry acceptance, kept apart from H8, and the rejection list (design
//! `step11-construction.md` sections 7.5 and 10, step 0).
//!
//! * **Geometry accepted:** structure sound, and every geometry rule (H1 to H7 and H9) is
//!   decided as `pass` or `not_applicable`: zero violations and zero unverified.
//! * **H8** is a record that ChituBox opened and sliced this exact file. The checker cannot
//!   measure it, so its state here is always `pending` until an external record is given.
//! * `ready_for_owner_review` is geometry accepted. `credible_to_print` is false from the
//!   checker alone: it also needs resolved interventions, H8 on the exact print file and the
//!   owner's judgement (PRD section 8.3).
//!
//! The rejection list names each rule that blocks geometry acceptance, with a location when
//! the measurement has one. Until a rule's measurement exists, the entry says it is
//! unverified and has no location. This module shares no code with the generator.

use crate::json::Json;
use crate::report::{RuleResult, Status};

/// The rules that decide geometry acceptance. H8 is not among them.
pub const GEOMETRY_RULES: [&str; 8] = ["H1", "H2", "H3", "H4", "H5", "H6", "H7", "H9"];

/// What still stands between a geometry-accepted file and a print candidate, by name.
pub const PRINT_REQUIRES: [&str; 3] = [
    "interventions resolved",
    "H8 recorded on the exact print file",
    "the owner's judgement",
];

/// Geometry acceptance.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Acceptance {
    /// Structure sound; every geometry rule `pass` or `not_applicable`.
    Accepted,
    /// A geometry rule is violated, or the file is unreadable.
    Violation,
    /// No violation, but a geometry rule or the structure is not decided.
    Unverified,
}

impl Acceptance {
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::Accepted => "accepted",
            Self::Violation => "violation",
            Self::Unverified => "unverified",
        }
    }
}

/// Why a rule is in the rejection list.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RejectionKind {
    /// The rule is violated.
    Violation,
    /// The rule could not be decided.
    Unverified,
}

impl RejectionKind {
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::Violation => "violation",
            Self::Unverified => "unverified",
        }
    }
}

/// Where in the file a rejection happened. Every field is optional: a rejection with no
/// location says the measurement has none (yet).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Location {
    /// Native support record index.
    pub record: Option<u32>,
    /// A point, mm, in the file's world frame.
    pub xyz: Option<[f64; 3]>,
    /// Print layer at the file's layer height.
    pub layer: Option<u32>,
}

/// One entry of the rejection list.
#[derive(Debug, Clone, PartialEq)]
pub struct Rejection {
    /// `H1` to `H9`, or `STRUCTURE` for an unreadable file.
    pub rule: String,
    /// Violation or unverified.
    pub kind: RejectionKind,
    /// Where, when the measurement knows.
    pub location: Option<Location>,
    /// Why, in plain words.
    pub reason: String,
}

fn is_geometry(id: &str) -> bool {
    GEOMETRY_RULES.contains(&id)
}

/// The geometry acceptance of a set of rule results.
#[must_use]
pub fn acceptance(rules: &[RuleResult], structure_ok: bool) -> Acceptance {
    let geometry = || rules.iter().filter(|r| is_geometry(r.id));
    if !structure_ok || geometry().any(|r| r.status == Status::Violation) {
        Acceptance::Violation
    } else if geometry().all(|r| matches!(r.status, Status::Pass | Status::NotApplicable))
        && geometry().count() == GEOMETRY_RULES.len()
    {
        Acceptance::Accepted
    } else {
        Acceptance::Unverified
    }
}

/// The rejection list implied by the rule results: one entry per geometry rule that is not
/// `pass` or `not_applicable`, plus one for an unreadable file. A rule's measurement may add
/// located entries of its own when it is built.
#[must_use]
pub fn rejections(rules: &[RuleResult], structure_ok: bool) -> Vec<Rejection> {
    let mut out = Vec::new();
    if !structure_ok {
        out.push(Rejection {
            rule: "STRUCTURE".to_owned(),
            kind: RejectionKind::Violation,
            location: None,
            reason: "an input file is unreadable or does not match the format".to_owned(),
        });
    }
    for r in rules.iter().filter(|r| is_geometry(r.id)) {
        let kind = match r.status {
            Status::Violation => RejectionKind::Violation,
            Status::Unverified => RejectionKind::Unverified,
            Status::Pass | Status::NotApplicable => continue,
        };
        out.push(Rejection {
            rule: r.id.to_owned(),
            kind,
            location: None,
            reason: r.reason.clone(),
        });
    }
    out
}

fn location_json(l: Option<&Location>) -> Json {
    let Some(l) = l else { return Json::Null };
    let xyz = l.xyz.map_or(Json::Null, |p| {
        Json::Arr(p.iter().map(|x| Json::Num(*x)).collect())
    });
    Json::obj([
        (
            "record",
            l.record.map_or(Json::Null, |r| Json::Uint(u64::from(r))),
        ),
        ("xyz", xyz),
        (
            "layer",
            l.layer.map_or(Json::Null, |k| Json::Uint(u64::from(k))),
        ),
    ])
}

fn rejection_json(r: &Rejection) -> Json {
    Json::obj([
        ("rule", Json::str(&r.rule)),
        ("kind", Json::str(r.kind.name())),
        ("location", location_json(r.location.as_ref())),
        ("reason", Json::str(&r.reason)),
    ])
}

/// The report fields of the geometry / H8 split, in report order: `geometry`, `h8`,
/// `ready_for_owner_review`, `credible_to_print`, `print_requires` and `rejections`.
#[must_use]
pub fn fields(rules: &[RuleResult], structure_ok: bool) -> Vec<(&'static str, Json)> {
    let accept = acceptance(rules, structure_ok);
    let list = rejections(rules, structure_ok);
    let count = |k: RejectionKind| {
        Json::Uint(crate::bytes::wide(
            list.iter().filter(|r| r.kind == k).count(),
        ))
    };
    let geometry = Json::obj([
        (
            "rules",
            Json::Arr(GEOMETRY_RULES.iter().map(|id| Json::str(*id)).collect()),
        ),
        ("acceptance", Json::str(accept.name())),
        ("violations", count(RejectionKind::Violation)),
        ("unverified", count(RejectionKind::Unverified)),
    ]);
    // Always pending here: H8 is an external record, not a measurement.
    let h8 = Json::obj([("state", Json::str("pending")), ("record", Json::Null)]);
    vec![
        ("geometry", geometry),
        ("h8", h8),
        (
            "ready_for_owner_review",
            Json::Bool(accept == Acceptance::Accepted),
        ),
        ("credible_to_print", Json::Bool(false)),
        (
            "print_requires",
            Json::Arr(PRINT_REQUIRES.iter().map(|s| Json::str(*s)).collect()),
        ),
        (
            "rejections",
            Json::Arr(list.iter().map(rejection_json).collect()),
        ),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::report::rules_unverified;

    fn all(status: Status) -> Vec<RuleResult> {
        let mut rules = rules_unverified("x");
        for r in &mut rules {
            r.status = status;
        }
        rules
    }

    #[test]
    fn h8_does_not_block_geometry_acceptance() {
        let mut rules = all(Status::Pass);
        rules[7].status = Status::Unverified; // H8
        assert_eq!(acceptance(&rules, true), Acceptance::Accepted);
        assert!(rejections(&rules, true).is_empty());
    }

    #[test]
    fn an_unverified_geometry_rule_blocks_and_is_listed() {
        let mut rules = all(Status::Pass);
        rules[0].status = Status::Unverified; // H1
        rules[4].status = Status::NotApplicable; // H5, proved
        assert_eq!(acceptance(&rules, true), Acceptance::Unverified);
        let list = rejections(&rules, true);
        assert_eq!(list.len(), 1);
        assert_eq!(
            (list[0].rule.as_str(), list[0].kind),
            ("H1", RejectionKind::Unverified)
        );
    }

    #[test]
    fn a_violation_or_a_bad_file_is_a_violation() {
        let mut rules = all(Status::Pass);
        rules[5].status = Status::Violation; // H6
        assert_eq!(acceptance(&rules, true), Acceptance::Violation);
        assert_eq!(acceptance(&all(Status::Pass), false), Acceptance::Violation);
        let list = rejections(&all(Status::Pass), false);
        assert_eq!(list[0].rule, "STRUCTURE");
    }

    #[test]
    fn a_missing_geometry_rule_is_never_accepted() {
        let rules: Vec<RuleResult> = all(Status::Pass)
            .into_iter()
            .filter(|r| r.id != "H3")
            .collect();
        assert_eq!(acceptance(&rules, true), Acceptance::Unverified);
    }

    #[test]
    fn the_fields_never_call_a_file_credible_to_print() {
        let text = Json::Obj(
            fields(&all(Status::Pass), true)
                .into_iter()
                .map(|(k, v)| (k.to_owned(), v))
                .collect(),
        )
        .render();
        assert!(text.contains("\"ready_for_owner_review\": true"), "{text}");
        assert!(text.contains("\"credible_to_print\": false"), "{text}");
        assert!(text.contains("\"state\": \"pending\""), "{text}");
        assert!(!text.contains("\"status\""), "{text}");
    }
}
