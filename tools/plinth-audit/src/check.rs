//! The check trait, its result type, and the failure record.
//!
//! **Two statuses, and only two** (`audit-invariants.md` §5 rule 1): `"pass"`
//! and `"fail"`. There is no `"skip"`, no `"warn"`, no `"n/a"`, and
//! `indeterminate > 0` forces `"fail"`. That rule is enforced here in
//! [`CheckOutcome::status`] rather than left to each implementation, so no
//! check can accidentally invent a third outcome.

use crate::json::Json;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Status {
    Pass,
    Fail,
}

impl Status {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Pass => "pass",
            Self::Fail => "fail",
        }
    }
    #[must_use]
    pub const fn is_pass(self) -> bool {
        matches!(self, Self::Pass)
    }
}

/// One failure, exactly as `audit-invariants.md` §5 rule 4 pins it.
///
/// `message` must be **byte-identical** to the human-readable failure line of
/// §3, so grepping the log and parsing the JSON find the same thing. Build it
/// with [`Failure::line`], which is the only place the format lives.
#[derive(Clone, Debug)]
pub struct Failure {
    pub code: &'static str,
    pub model: usize,
    pub contact: Option<usize>,
    pub record: Option<usize>,
    /// Every quantity the failure line names, as JSON numbers in the unit
    /// named by the key suffix.
    pub values: Json,
    pub message: String,
}

impl Failure {
    /// Compose a failure line and its JSON row together.
    ///
    /// `fields` are the `key=value` pairs of the line, in the exact order the
    /// specification writes them. The `file=`, `model=`, `contact=` and
    /// `record=` prefixes are added here so every line agrees on them.
    #[must_use]
    pub fn line(
        code: &'static str,
        file: &str,
        model: usize,
        contact: Option<usize>,
        record: Option<usize>,
        fields: &[(&str, String)],
        values: Json,
    ) -> Self {
        let mut msg = format!("{code} file={file} model={model}");
        if let Some(cc) = contact {
            msg.push_str(&format!(" contact={cc}"));
        }
        if let Some(r) = record {
            msg.push_str(&format!(" record={r}"));
        }
        for (k, v) in fields {
            msg.push_str(&format!(" {k}={v}"));
        }
        Self {
            code,
            model,
            contact,
            record,
            values,
            message: msg,
        }
    }

    #[must_use]
    pub fn to_json(&self) -> Json {
        let mut o = Json::obj();
        o.set("code", Json::str(self.code));
        o.set("model", Json::usize(self.model));
        if let Some(cc) = self.contact {
            o.set("contact", Json::usize(cc));
        }
        if let Some(r) = self.record {
            o.set("record", Json::usize(r));
        }
        o.set("values", self.values.clone());
        o.set("message", Json::str(self.message.clone()));
        o
    }
}

/// What one invariant found on one model.
#[derive(Clone, Debug)]
pub struct CheckOutcome {
    /// Items examined. **A passing invariant that reports no numbers is not
    /// evidence** (§5 rule 3), so this is the count the `measured` block must
    /// carry rows for.
    pub checked: usize,
    pub failed: usize,
    /// Items the check could not establish. Any nonzero value forces `fail`.
    pub indeterminate: usize,
    /// Free-form per-invariant measurements. Must carry the quantities its
    /// failure lines name, **for every item checked and not only for
    /// failures**.
    pub measured: Json,
    pub failures: Vec<Failure>,
}

impl CheckOutcome {
    #[must_use]
    pub fn new(checked: usize) -> Self {
        Self {
            checked,
            failed: 0,
            indeterminate: 0,
            measured: Json::obj(),
            failures: Vec::new(),
        }
    }

    /// Record a failure and count it.
    pub fn fail(&mut self, f: Failure) {
        self.failed += 1;
        self.failures.push(f);
    }
    /// Record an item the check could not establish. It is BOTH counted as
    /// indeterminate and reported as a failure line, because "not established"
    /// is a failure with a reason, never a silence.
    pub fn indeterminate(&mut self, f: Failure) {
        self.indeterminate += 1;
        self.failures.push(f);
    }

    /// The one place the two-value rule lives.
    #[must_use]
    pub const fn status(&self) -> Status {
        if self.failed > 0 || self.indeterminate > 0 {
            Status::Fail
        } else {
            Status::Pass
        }
    }

    /// The stub every unimplemented check returns.
    ///
    /// It is a FAILURE, never a pass and never a skip. `scope` is how many
    /// items the check would have examined, so the counts stay honest while
    /// the check is unwritten: `checked == indeterminate == scope`, exactly the
    /// shape §5 rule 6 pins for an aborted file.
    #[must_use]
    pub fn not_implemented(id: &'static str, file: &str, model: usize, scope: usize) -> Self {
        let scope = scope.max(1);
        let f = Failure::line(
            NOT_IMPLEMENTED_CODE,
            file,
            model,
            None,
            None,
            &[("invariant", id.to_string()), ("scope", scope.to_string())],
            {
                let mut o = Json::obj();
                o.set("invariant", Json::str(id));
                o.set("scope", Json::usize(scope));
                o
            },
        );
        let mut measured = Json::obj();
        measured.set("implemented", Json::Bool(false));
        measured.set(
            "headline",
            Json::str("NOT IMPLEMENTED -- a stub never passes"),
        );
        measured.set(
            "note",
            Json::str("this invariant is not implemented; \"not implemented\" never maps to pass"),
        );
        Self {
            checked: scope,
            failed: 0,
            indeterminate: scope,
            measured,
            failures: vec![f],
        }
    }

    /// Is this outcome the untouched stub?
    #[must_use]
    pub fn is_not_implemented(&self) -> bool {
        self.measured
            .get("implemented")
            .is_some_and(|v| *v == Json::Bool(false))
    }

    #[must_use]
    pub fn to_json(&self) -> Json {
        let mut o = Json::obj();
        o.set("status", Json::str(self.status().as_str()));
        o.set("checked", Json::usize(self.checked));
        o.set("failed", Json::usize(self.failed));
        o.set("indeterminate", Json::usize(self.indeterminate));
        o.set("measured", self.measured.clone());
        o.set(
            "failures",
            Json::Arr(self.failures.iter().map(Failure::to_json).collect()),
        );
        o
    }

    /// The single line the human summary prints for this invariant.
    #[must_use]
    pub fn headline(&self) -> String {
        self.measured
            .get("headline")
            .and_then(Json::as_str)
            .unwrap_or("")
            .to_string()
    }
}

/// The code an unimplemented check reports. It is deliberately NOT one of the
/// specification's own codes: a stub must be distinguishable from a real
/// finding at a glance, in the log and in the JSON.
pub const NOT_IMPLEMENTED_CODE: &str = "AUDIT_CHECK_NOT_IMPLEMENTED";

/// One gating invariant.
///
/// Implementations live one per file in `src/checks/`, so ten agents each
/// edit exactly one file and nobody touches a shared module list.
pub trait Check {
    /// The schema key, e.g. `"i1_seat_depth"`. It is the JSON field name and
    /// must not change.
    fn id(&self) -> &'static str;
    /// The label for the human summary, e.g. `"I1 seat depth"`.
    fn title(&self) -> &'static str;
    /// How many items this check examines on this model. Used for the stub's
    /// honest counts and for nothing else.
    fn scope(&self, ctx: &crate::context::ModelContext) -> usize;
    /// Run the invariant.
    fn run(&self, ctx: &crate::context::ModelContext) -> CheckOutcome;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn indeterminate_forces_fail_and_there_is_no_third_status() {
        let mut o = CheckOutcome::new(10);
        assert_eq!(o.status(), Status::Pass);
        o.indeterminate(Failure::line(
            "X_INDETERMINATE",
            "f",
            0,
            Some(1),
            Some(2),
            &[("reason", "ray_parity_exhausted".into())],
            Json::obj(),
        ));
        assert_eq!(o.status(), Status::Fail);
        assert_eq!(o.failed, 0, "indeterminate is counted separately");
        assert_eq!(o.indeterminate, 1);
        assert_eq!(o.failures.len(), 1, "and still reported as a line");
    }

    #[test]
    fn the_stub_is_a_failure_with_honest_counts() {
        let o = CheckOutcome::not_implemented("i1_seat_depth", "f.chitubox", 0, 24);
        assert_eq!(o.status(), Status::Fail);
        assert_eq!(o.checked, 24);
        assert_eq!(o.indeterminate, 24);
        assert!(o.is_not_implemented());
        assert_eq!(o.failures[0].code, NOT_IMPLEMENTED_CODE);
    }

    #[test]
    fn the_failure_line_and_the_json_row_carry_the_same_text() {
        let f = Failure::line(
            "SEAT_DEPTH_ZERO",
            "/tmp/a.chitubox",
            0,
            Some(12),
            Some(34),
            &[
                ("h_delivered_mm", "0".to_string()),
                ("p0", "1,2,3".to_string()),
            ],
            Json::obj(),
        );
        assert_eq!(
            f.message,
            "SEAT_DEPTH_ZERO file=/tmp/a.chitubox model=0 contact=12 record=34 \
             h_delivered_mm=0 p0=1,2,3"
                .replace("             ", " ")
                .replace("  ", " ")
        );
        let j = f.to_json();
        assert_eq!(
            j.get("message").and_then(Json::as_str),
            Some(&f.message[..])
        );
    }
}
