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
    ChituboxLoadability,
    RepairCapExhausted,
}
