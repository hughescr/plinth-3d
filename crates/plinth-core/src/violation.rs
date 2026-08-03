use serde::{Deserialize, Serialize};

/// Every run-time condition that can produce a defect.
///
/// This deliberately has no responder table. Repair policy belongs to the
/// crate that owns repair, while this core enum remains exhaustive.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ViolationKind {
    UncoveredIsland,
    UncoveredIslandBirth,
    UnsupportedMinimum,
    LoadExceeded,
    ArrangementInstability,
    ClearanceTransit,
    BallBreach,
    ScarCapExceeded,
    Disconnected,
    Unprintable,
    MurrayViolation,
    UnroutedContact,
    ContactProvenance,
    CavityNotDrainable,
    EmitBoundaryEdge,
    EmitNegativeShell,
    EmitInconsistentWinding,
    /// The artifact could not be read from the requested path. Its measure is
    /// an artifact failure count: exactly +1.0 for the failing artifact.
    ArtifactUnreadable,
    /// The on-disk artifact failed ChituBox container parsing, its independent
    /// structural audit, or exact clean round-trip. Its measure is an artifact
    /// failure count: exactly +1.0 for the failing artifact. This does not
    /// assert GUI/native ChituBox acceptance.
    ChituboxStructuralFormat,
    /// Reserved solely for actual GUI/native ChituBox acceptance evidence;
    /// structural verification must never emit this kind.
    ChituboxLoadability,
    RepairCapExhausted,
}
