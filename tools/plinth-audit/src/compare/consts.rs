//! Every threshold of the similarity gate, with its provenance.
//!
//! Tags: **M** = measured (the evidence file is named), **E** = estimated and
//! to be calibrated by G3 or by the first real output-changing lane, **S** =
//! stated by the plan or the owner. A threshold with no tag does not belong
//! here. Source documents: `ROUND-PLAN-v2.md` section 1 (plan), `metric-design.md`
//! (prototype calibration), `OWNER-DECISIONS.md` 2026-10-10 (O1-O4).

use crate::consts as c;

/// One voxel in plan, the larger pitch (S: plan "1 voxel").
pub const VOXEL_MM: f64 = c::PIXEL_PITCH_Y_MM;
/// The tight pitch, used where a smaller number is the safe side (S).
pub const VOXEL_TIGHT_MM: f64 = c::PIXEL_PITCH_X_MM;

// ---- class ---------------------------------------------------------------
/// Pose differs when the committed rotation differs by more than this (S, plan).
pub const POSE_ROT_RAD: f64 = 1.0e-6;
/// ... or the committed translation differs by more than one voxel (S, plan).
pub const POSE_TRANSLATE_MM: f64 = VOXEL_MM;
/// Bytes-only pose proxy (used only when neither file has a pose text): the
/// model bounding box moved by more than two voxels (E; a 1e-3 rad turn of a
/// 40 mm model moves a corner 0.04 mm).
pub const POSE_BYTES_BBOX_MM: f64 = 2.0 * VOXEL_MM;
/// Mesh differs when the model drift beyond the band exceeds this many voxels (S, plan).
pub const MESH_DRIFT_VOXELS: u64 = 500;
/// ... or one drift blob has this many voxels or more (S, plan).
pub const MESH_DRIFT_BLOB_VOXELS: i64 = 64;
/// ... or the vertex Hausdorff distance exceeds one voxel (S, plan).
pub const MESH_HAUSDORFF_MM: f64 = VOXEL_TIGHT_MM;
/// Rotation-invariant mesh signature: relative volume and area change under
/// which two meshes at different poses count as the same mesh (E).
pub const MESH_SIGNATURE_REL: f64 = 1.0e-5;

// ---- Tier 0 ----------------------------------------------------------------
/// Matching radius for support items between the two files, mm (M: metric-design N2, 0.1 mm).
pub const MATCH_MM: f64 = 0.10;
/// A record's emitted mesh may deviate from its header by this much more than
/// the reference's matched record does before Tier 0 fails (S: "within 1 voxel").
pub const T0_MESH_HEADER_MM: f64 = VOXEL_TIGHT_MM;
/// Record kinds whose mesh is trimmed at the model surface (tips, tapers) may
/// legitimately differ from their header; they are compared against the
/// reference, never against zero (M: real BCO, I8 shows 0.02-0.07 mm on tips).
pub const T0_TRIMMED_SLACK_MM: f64 = 0.0;

/// A support solid may sit this far below the plate before the frame is not
/// established and the audit refuses to run (E; the audit's own tolerance is
/// 1e-6 mm, which a sub-voxel change in a flare foot would trip).
pub const T0_FRAME_SLACK_MM: f64 = VOXEL_TIGHT_MM;

// ---- H1 / H2 / N4 ----------------------------------------------------------
/// Lateral tolerance when grounding a cured component on the layer below, px (M: metric-design H1).
pub const H1_LATERAL_PX: i64 = 1;
/// A new floating component of this many px fails (M: jitter max 5 px, real defect 22 px; E for the limit, G5 pending).
pub const H1_FLOAT_PX: i64 = 16;
/// New floating voxels in total above this fail (S, plan).
pub const H1_FLOAT_TOTAL: i64 = 64;
/// A model island of this many px or more must keep support (S, plan; 0.01 mm2).
pub const H2_ISLAND_PX: i64 = 38;
/// Islands are matched by layer and centroid within this distance, mm (E).
pub const H2_MATCH_MM: f64 = 0.05;
/// Matched-failure severity may grow by this fraction before H4 fails (S, plan: 10 %).
pub const H4_WORSE_FRACTION: f64 = 0.10;
/// Failure items are matched within this distance, mm (E).
pub const H4_MATCH_MM: f64 = 0.15;

// ---- H3 / EQUIVALENT ---------------------------------------------------------
/// Model voxels beyond the 1-voxel band allowed in a same-pose, same-mesh run (S, plan).
pub const H3_VOXELS: u64 = 500;
/// A model blob this large fails H3 (S, plan).
pub const H3_BLOB_VOXELS: i64 = 64;
/// EQUIVALENT: every contact moves at most this far, mm (S, plan).
pub const EQ_CONTACT_MM: f64 = 0.02;
/// EQUIVALENT: every contact radius changes at most this fraction (S, plan).
pub const EQ_CONTACT_DR: f64 = 0.02;
/// EQUIVALENT: member sections within this fraction (S, plan).
pub const EQ_SECTION: f64 = 0.05;
/// EQUIVALENT: voxels beyond the band, support included (S, plan).
pub const EQ_DRIFT_VOXELS: u64 = 500;
pub const EQ_DRIFT_BLOB_VOXELS: i64 = 64;

// ---- Tier 2 --------------------------------------------------------------------
/// N1 support volume band (S, plan; E until calibrated).
pub const N1_FRACTION: f64 = 0.05;
/// N2 contact count band (S, plan).
pub const N2_COUNT_FRACTION: f64 = 0.05;
/// N2 share of the reference contacts matched within [`MATCH_MM`] (S, plan).
pub const N2_MATCHED_FRACTION: f64 = 0.95;
/// N3 no member may get thinner than this fraction of itself unless I3 holds (S, plan).
pub const N3_THINNER: f64 = 0.15;
/// N4 a join this small, px, counts as thin (M: metric-design N4, 30 px).
pub const N4_THIN_PX: i64 = 30;
/// N4 thin joins may rise by max(this fraction, [`N4_MIN_RISE`]) (S, plan).
pub const N4_FRACTION: f64 = 0.10;
pub const N4_MIN_RISE: i64 = 20;
/// N5 worst tied-post sway may rise by this fraction (S, plan, critique M4).
pub const N5_SWAY_FRACTION: f64 = 0.10;
/// N5 slenderness limit used when the log does not print `S = ...` (M: log "S = 40.0 in force").
pub const N5_S_DEFAULT: f64 = 40.0;

// ---- H5 / H6 -----------------------------------------------------------------------
/// The load gap may not fall below this, mm2 (M baseline, plan H5).
pub const H5_LOAD_GAP_MM2: f64 = -0.08;
/// H6 intrusion depth counted as defective, mm (S: geometry `COUNT_ABOVE_MM`).
pub const H6_INTRUSION_MM: f64 = crate::geometry::intrusion::COUNT_ABOVE_MM;

// ---- consumer match (G5 pending) ---------------------------------------------------
/// The BCO v22 junction near member r83: layer 138, xy (5.26, 2.03) mm (M: metric-design "Gaps found").
pub const BCO_JUNCTION_XY: [f64; 2] = [5.26, 2.03];
pub const BCO_JUNCTION_LAYER: i64 = 138;
/// Search radius around the junction, mm, and layers either side (E).
pub const BCO_JUNCTION_RADIUS_MM: f64 = 0.30;
pub const BCO_JUNCTION_LAYERS: i64 = 4;
