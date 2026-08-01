//! Data boundary for the single final ChituBox write.
//!
//! This checkpoint intentionally contains no geometry synthesis, boolean union, file write, or
//! audit behavior. Behavior lands on the parallel `plinth-emit` implementation branch with this
//! signature:
//!
//! `plinth_emit::write(&SupportedModel, &EmitRequest) -> Result<EmitReport, EmitError>`
//!
//! That implementation writes once through `chitubox_io`, then reopens the target from disk for
//! audit. This crate has no input type other than `SupportedModel`.
#![forbid(unsafe_code)]

use plinth_core::PrinterProfile;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use thiserror::Error;

#[derive(Clone, Debug, PartialEq)]
pub struct EmitRequest {
    pub target_path: PathBuf,
    pub printer: PrinterProfile,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EmitReport {
    pub artifact_path: PathBuf,
    pub support_record_count: usize,
    pub artifact_bytes: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Error)]
pub enum EmitError {
    #[error("cannot create ChituBox target {path}")]
    TargetUnavailable { path: PathBuf },
    #[error("ChituBox record encoding rejected the supported scene")]
    RecordEncodingRejected,
    #[error("the freshly reopened ChituBox artifact failed its disk audit")]
    DiskAuditRejected,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn successful_emit_report_names_the_single_written_artifact() {
        let report = EmitReport {
            artifact_path: PathBuf::from("out/mini.chitubox"),
            support_record_count: 12,
            artifact_bytes: 4096,
        };
        let wire = serde_json::to_value(&report).expect("emit report serializes");
        assert_eq!(wire["artifact_path"], "out/mini.chitubox");
        assert_eq!(wire["support_record_count"], 12);
        assert_eq!(
            serde_json::from_value::<EmitReport>(wire).expect("emit report deserializes"),
            report
        );
    }
}
