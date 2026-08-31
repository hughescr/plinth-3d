//! The optional `<target-stem>.coverage.repair.json` sidecar.
//!
//! **DECLARED data. Never truth** (`audit-contract.md` §5.3). The audit reads
//! it to compare against reconstructions, never to substitute for one.
//!
//! Three rules this module enforces at the type level.
//!
//! 1. **`null` is load-bearing and is never read as zero.** The disclosure
//!    type's own documentation is explicit: `0.0 mm^2` is a legal area to READ
//!    and an illegal one to INVENT; `null` means "could not be described". An
//!    audit that coalesces `null` to `0.0` converts an undescribable contact
//!    into a compliant one. Every optional field is [`Option`] and there is no
//!    `unwrap_or(0.0)` anywhere in this crate.
//! 2. **Absence is not a pass.** A missing sidecar degrades the audit to
//!    "these facts are unverifiable, and this model therefore FAILS the
//!    DECLARED-comparison checks" (`audit-contract.md` §5.4). Measured
//!    2026-08-30: no `.coverage.repair.json` exists anywhere in the checkout,
//!    so this is the normal case, not the exception.
//! 3. **Row count is checkable and is a hard failure.** The number of
//!    `contacts[]` rows should equal the number of type-7 records in the
//!    artifact. A mismatch is a failure, not a warning.

use crate::json::{self, Json};
use std::path::{Path, PathBuf};

/// The schema tag `persist_sealed_repair_ledger` writes.
pub const SCHEMA_TAG: &str = "plinth.coverage-repair.v1";

/// One `contacts[]` row: `plinth_support::EmittedContactDisclosure`.
#[derive(Clone, Debug)]
pub struct ContactDisclosure {
    /// The physical contact id, which is also its terminal id.
    pub contact_id: u64,
    /// Where the contact sits on the model, mm. Reconstructible as the tip
    /// ball centre, so rule 2 of the contract applies: reconstruct, compare,
    /// fail on disagreement.
    pub position_mm: Option<[f64; 3]>,
    /// Model area this contact is charged to carry, after every removal and
    /// repartition. **DECLARED-ONLY**: not in the file at all. The audit can
    /// bound it with its own re-attribution; it cannot verify it.
    pub carried_area_mm2: Option<f64>,
    /// **RADIUS** of the member end that touches the model, mm.
    /// Reconstructible from the tip soup, so rule 2 applies.
    pub tip_radius_mm: Option<f64>,
    /// Exactly `2 * tip_radius_mm` when both are present.
    pub tip_diameter_mm: Option<f64>,
}

#[derive(Clone, Debug)]
pub struct Sidecar {
    pub path: PathBuf,
    pub schema: String,
    pub model: String,
    pub commits: Option<f64>,
    pub trials: Option<f64>,
    pub contacts: Vec<ContactDisclosure>,
    /// `true` when `report` and `steps` were present. Their contents are not
    /// decoded: nothing in this document may decide a verdict.
    pub has_report: bool,
    pub has_steps: bool,
}

#[derive(Debug)]
pub enum SidecarError {
    Io(String),
    Json(String),
    /// The document parsed but is not the schema this reader understands.
    /// Refused loudly rather than read optimistically.
    WrongSchema {
        found: String,
    },
    /// A structural field is missing or the wrong type.
    Malformed(String),
}

impl std::fmt::Display for SidecarError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(m) => write!(f, "sidecar io error: {m}"),
            Self::Json(m) => write!(f, "sidecar {m}"),
            Self::WrongSchema { found } => {
                write!(f, "sidecar schema is {found:?}, expected {SCHEMA_TAG:?}")
            }
            Self::Malformed(m) => write!(f, "sidecar malformed: {m}"),
        }
    }
}

impl Sidecar {
    /// The conventional sidecar path beside an artifact.
    #[must_use]
    pub fn default_path_for(artifact: &Path) -> PathBuf {
        let stem = artifact.file_stem().map_or_else(
            || "unknown".to_string(),
            |s| s.to_string_lossy().into_owned(),
        );
        artifact.with_file_name(format!("{stem}.coverage.repair.json"))
    }

    pub fn load(path: &Path) -> Result<Self, SidecarError> {
        let text = std::fs::read_to_string(path)
            .map_err(|e| SidecarError::Io(format!("{}: {e}", path.display())))?;
        let v = json::parse(&text).map_err(|e| SidecarError::Json(e.to_string()))?;
        Self::from_json(path, &v)
    }

    pub fn from_json(path: &Path, v: &Json) -> Result<Self, SidecarError> {
        let schema = v
            .get("schema")
            .and_then(Json::as_str)
            .ok_or_else(|| SidecarError::Malformed("no `schema` string".into()))?
            .to_string();
        if schema != SCHEMA_TAG {
            return Err(SidecarError::WrongSchema { found: schema });
        }
        let model = v
            .get("model")
            .and_then(Json::as_str)
            .unwrap_or_default()
            .to_string();
        let rows = v
            .get("contacts")
            .and_then(Json::as_array)
            .ok_or_else(|| SidecarError::Malformed("no `contacts` array".into()))?;
        let mut contacts = Vec::with_capacity(rows.len());
        for (i, row) in rows.iter().enumerate() {
            let contact_id = row
                .get("contact_id")
                .and_then(Json::as_f64)
                .ok_or_else(|| {
                    SidecarError::Malformed(format!("contacts[{i}] has no numeric contact_id"))
                })? as u64;
            let position_mm = row.get("position_mm").and_then(|p| {
                let a = p.as_array()?;
                (a.len() == 3).then(|| Some([a[0].as_f64()?, a[1].as_f64()?, a[2].as_f64()?]))?
            });
            contacts.push(ContactDisclosure {
                contact_id,
                position_mm,
                // `and_then(as_f64)` yields None for JSON null, which is the
                // whole point: null must never become 0.0.
                carried_area_mm2: row.get("carried_area_mm2").and_then(Json::as_f64),
                tip_radius_mm: row.get("tip_radius_mm").and_then(Json::as_f64),
                tip_diameter_mm: row.get("tip_diameter_mm").and_then(Json::as_f64),
            });
        }
        Ok(Self {
            path: path.to_path_buf(),
            schema,
            model,
            commits: v.get("commits").and_then(Json::as_f64),
            trials: v.get("trials").and_then(Json::as_f64),
            contacts,
            has_report: v.get("report").is_some(),
            has_steps: v.get("steps").is_some(),
        })
    }

    /// The row for a contact id, if the sidecar has one.
    #[must_use]
    pub fn contact(&self, contact_id: u64) -> Option<&ContactDisclosure> {
        self.contacts.iter().find(|c| c.contact_id == contact_id)
    }

    /// Rows carrying a `null` in any field. Each one is a contact the engine
    /// could not describe, and `audit-contract.md` §5.2 says to FAIL on them
    /// rather than default them.
    #[must_use]
    pub fn undescribable_rows(&self) -> Vec<u64> {
        self.contacts
            .iter()
            .filter(|c| {
                c.position_mm.is_none()
                    || c.carried_area_mm2.is_none()
                    || c.tip_radius_mm.is_none()
                    || c.tip_diameter_mm.is_none()
            })
            .map(|c| c.contact_id)
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const DOC: &str = r#"{
      "schema": "plinth.coverage-repair.v1",
      "model": "Sphere",
      "commits": 3, "trials": 7,
      "report": {}, "steps": [],
      "contacts": [
        {"contact_id": 4, "position_mm": [1.0, 2.0, -3.5],
         "carried_area_mm2": 0.0, "tip_radius_mm": 0.197, "tip_diameter_mm": 0.394},
        {"contact_id": 9, "position_mm": null,
         "carried_area_mm2": null, "tip_radius_mm": null, "tip_diameter_mm": null}
      ]}"#;

    #[test]
    fn a_zero_area_reads_as_zero_and_a_null_reads_as_absent() {
        let v = crate::json::parse(DOC).expect("json");
        let s = Sidecar::from_json(Path::new("x.json"), &v).expect("sidecar");
        assert_eq!(s.contacts.len(), 2);
        let a = s.contact(4).expect("row");
        assert_eq!(a.carried_area_mm2, Some(0.0), "0.0 is a legal area to READ");
        let b = s.contact(9).expect("row");
        assert_eq!(
            b.carried_area_mm2, None,
            "null must never be coalesced to zero"
        );
        assert_eq!(s.undescribable_rows(), vec![9]);
    }

    #[test]
    fn a_wrong_schema_is_refused_not_read_optimistically() {
        let v = crate::json::parse(r#"{"schema":"plinth.coverage-refusal.v2","contacts":[]}"#)
            .expect("json");
        assert!(matches!(
            Sidecar::from_json(Path::new("x"), &v),
            Err(SidecarError::WrongSchema { .. })
        ));
    }

    #[test]
    fn the_default_sidecar_path_follows_the_stem() {
        let p = Sidecar::default_path_for(Path::new("/a/b/sphere-first-artifact.chitubox"));
        assert!(
            p.ends_with("sphere-first-artifact.coverage.repair.json"),
            "{p:?}"
        );
    }
}
