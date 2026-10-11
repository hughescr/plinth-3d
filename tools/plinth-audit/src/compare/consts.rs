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
/// A mesh vertex this close to the model surface is a vertex of the model's cut (the record ends in
/// the model and the emitted mesh stops at the model surface), not a disagreement between the
/// record's mesh and its header (G7). Two voxels: the engine ends a taper 0.016-0.024 mm from the
/// model by design (the seat gap), a distance that one voxel (0.0187) splits, and a micron of noise
/// then moves a vertex across it (Bat_Cat_Owl r45, cjitter 1e-3 seed 3, with one voxel).
pub const T0_MODEL_TRIM_MM: f64 = 2.0 * VOXEL_TIGHT_MM;

// ---- flicker allowance (G6) ---------------------------------------------------
//
// PROVENANCE OF EVERY NUMBER IN THIS SECTION. Calibration set (M): the frozen Bat_Cat_Owl (v22
// file) and Deinonychus (manifest t4 file), each against copies with one-ulp noise and with
// `cjitter` noise of 2e-6 and 1e-4 mm (both files) and 1e-3 mm (Bat_Cat_Owl only) on header ends
// and radii (support only, and model plus support), seeds 0-6, 84 pairs, compared with
// `--no-flicker` (the strict reading). `cjitter` carries each record's mesh with its header, as an
// engine's output does (`mutate::Mutation::JitterConsistent`). Every value below is the LARGEST
// that those pairs produced, read from the `evidence.h4`, `n3_ratio_moved`, `h1_new_floating`,
// `h2_low_attach_changes` and row values of their `verdict.json` (files in
// `wave1b/cal-raw-s*-cons/`; the reader is `wave1b/scripts/calib4.sh`). The values were then held out
// against seeds 7-9 (`G6-REPORT.md`).
//
// Deinonychus at 1e-3 mm is NOT in the set, on purpose. There the noise turns a 0.02 mm long,
// 0.6 mm wide member by a few degrees and moves its rim by two voxels, and the same noise makes
// a contact's area deficit reach 0.97 of its requirement; a band wide enough to take that in
// also takes in a halved contact. It stays REGRESSION on most seeds, and the report says so.
// Seeds 4-6 once failed at the edges when only seeds 0-3 were used, so the maxima were refreshed
// from seeds 0-6 before seeds 7-9 were drawn fresh.
// Nothing here is a design constant; if the engine's noise grows, measure again and change these.
//
// What this costs, stated so it is not forgotten: a real change that stays inside these bands and
// touches no more than FLICKER_FRACTION of an invariant's items is invisible to the gate. The
// cumulative check against the blessed set bounds the drift of a series of such changes.

/// Per code, two bands, each `(absolute, fraction of the item's own requirement)` read as
/// `abs + frac * scale`: the largest severity noise gave to an item that was NOT failing in the
/// reference (`new`), and the largest growth it gave to an item that WAS (`worse`). The two differ
/// a lot (a contact area deficit appeared up to 0.965 of its requirement and never grew), and a
/// matched item is where a real defect shows (a contact whose area is halved), so `worse` gets
/// only what noise was seen to do. `f64::INFINITY` marks a yes/no code with no magnitude: only
/// the count cap limits it.
struct Rule {
    new: (f64, f64),
    worse: (f64, f64),
}

fn flicker_rule(code: &str) -> Option<Rule> {
    const NONE: (f64, f64) = (0.0, 0.0);
    let r = |new: (f64, f64), worse: (f64, f64)| Rule { new, worse };
    Some(match code {
        // I1: declared-depth mismatch new 0.152, growth 0.779 of the declared depth; shallow new
        // 0.323, growth 0.0197 of the required depth; a seat that fell to zero depth is yes/no.
        "SEAT_DEPTH_DECLARED_MISMATCH" => r((0.0, 0.152), (0.0, 0.78)),
        "SEAT_DEPTH_SHALLOW" => r((0.0, 0.324), (0.0, 0.02)),
        "SEAT_DEPTH_ZERO" => r((f64::INFINITY, 0.0), NONE),
        // I2: no contact area deficit and no disconnected patch appeared in these pairs, so none is
        // allowed: a halved contact is a finding. I3 new up to 0.642, growth 0.0836 of the required area.
        "MIN_SECTION_SHORT" => r((0.0, 0.643), (0.0, 0.084)),
        // I6, mm3: outside-seat growth 0.000413; far-side breach growth 0.000386.
        "INTERSECT_OUTSIDE_SEAT" => r(NONE, (0.000_413, 0.0)),
        "INTERSECT_FAR_SIDE_BREACH" => r(NONE, (0.000_386, 0.0)),
        // I7 / I9 unsupported island area, mm2: new up to 0.0261 (I9), growth 0.000798.
        "ISLAND_UNSUPPORTED" => r((0.0261, 0.0), (0.0008, 0.0)),
        // I8 dead member volume, mm3: 48 new ones, the largest 0.01272 mm3; none was seen to grow.
        "GEOM_DEAD_MEMBER" => r((0.0128, 0.0), NONE),
        // I10 collinear contacts with no equilibrium, margin shortfall mm: new 0.8665, growth 0.0526.
        "SPREAD_COLLINEAR_NO_EQUILIBRIUM" => r((0.867, 0.0), (0.0527, 0.0)),
        _ => return None,
    })
}

/// Noise band of one audit item code in the units of its severity, for an item whose own
/// requirement is `scale` (1 when the code has none). `worse` false: the band for a NEW failing
/// candidate item; true: the band for the growth of a matched item. Inside it the change is
/// flicker and not a regression. 0 for a code the calibration never saw move (no allowance).
#[must_use]
pub fn flicker_band(code: &str, scale: f64, worse: bool) -> f64 {
    flicker_rule(code).map_or(0.0, |r| {
        let (abs, frac) = if worse { r.worse } else { r.new };
        if abs.is_infinite() { abs } else { abs + frac * scale }
    })
}

/// The largest share of one invariant's checked items that noise moved across a threshold
/// (new small failures plus lost evaluations): 1 of 21 islands on Bat_Cat_Owl, I7, seed 3 (M).
pub const FLICKER_FRACTION: f64 = 0.048;

/// The most crossings one invariant may absorb as flicker, given how many items it checked
/// (rounded DOWN, so a model with fewer than 21 items absorbs none). Past it, none is absorbed.
#[must_use]
pub fn flicker_cap(checked: usize) -> usize {
    (FLICKER_FRACTION * checked as f64).floor() as usize
}

/// N3: a member section that moved by no more than this, mm, did not change. M: the largest
/// absolute section change in the calibration pairs is 0.0158 mm; it is set to one voxel
/// (the plan's pitch, 0.019 mm), the raster's own resolution, which that maximum sits under.
pub const N3_NOISE_MM: f64 = VOXEL_MM;
/// T0: a record that already disagrees with its header in the reference (a member whose mesh is
/// a bulb, not a frustum) may grow its disagreement by this fraction without failing. A record
/// that agrees (disagreement 0) gets nothing, so a planted shrink of a good record still fails.
/// M: the largest growth beyond the one-voxel allowance in the calibration pairs is 1.97 %.
pub const T0_MESH_REL_NOISE: f64 = 0.02;
/// T0: the share of records whose mesh is past one voxel from its header may rise by this fraction
/// of the reference's share, plus [`T0_RATE_PLUS`] records, before T0 fails. E (G7, first real use):
/// the reference holds 145 of 1430 (Deinonychus), 10 of 186 (Bat_Cat_Owl), 53 of 1172 (Cylinder) from
/// the junction hulls; the Q12 candidate holds 104 of 1219, 10 of 162, 53 of 1164, so the ratios of
/// the shares are 0.84, 1.15 and 1.00. Re-measure when a lane changes the hull drawing.
pub const T0_RATE_SLACK: f64 = 0.25;
/// ... and this many records on top, for a small model (E, with [`T0_RATE_SLACK`]).
pub const T0_RATE_PLUS: usize = 2;

/// H1: a newly floating piece is connectivity flicker when at least this fraction of its pixels
/// lie inside the 1-voxel band of the reference's cured material and the drift is small. M: 1.00
/// on all 228 pieces of 16 px or more in the calibration pairs (the evidence lists at most 60
/// pieces per pair, so the count is a floor); a planted float has 0.
pub const H1_OLD_FRACTION: f64 = 1.0;
/// H2: an island whose bearing in the reference was at most this many pixels is not counted as
/// having lost all its support when its bearing goes to 0, and the count of bearing-less islands
/// may rise by the crossing cap. M: the island that lost all its support in the calibration
/// pairs (Bat_Cat_Owl at 1e-3) had 2 px of bearing.
pub const H2_ATTACH_NOISE_PX: i64 = 2;
/// H6: straight contacts beyond the skim law may rise by this fraction of the reference count,
/// rounded up. M: 1 of 10 on Bat_Cat_Owl at 1e-3 (Deinonychus: none moved).
pub const H6_SKIM_NOISE_FRACTION: f64 = 0.10;
/// N4: thin joins may rise by at least this fraction. M: the calibration pairs moved them by at
/// most +21 of 13914 (0.15 %), far inside the plan's 10 %, so no widening is needed (0).
pub const N4_NOISE_FRACTION: f64 = 0.0;

/// Every noise allowance of the gate, in one value, so that `--no-flicker` can switch them all
/// off at once: the strict reading, which is the sabotage case (the jittered pairs must fail it)
/// and the way to take new calibration data.
#[derive(Clone, Copy, Debug)]
pub struct Noise {
    /// Per-code item bands and the crossing cap (H4).
    pub items: bool,
    pub n3_mm: f64,
    pub t0_rel: f64,
    /// T0: a record the reference does not have is held to the reference's own worst of its kind.
    pub t0_by_ref: bool,
    pub h1_old_fraction: f64,
    pub h2_attach_px: i64,
    pub h6_skim_fraction: f64,
    pub n4_fraction: f64,
    pub cap_fraction: f64,
}

impl Noise {
    /// The measured allowances above.
    #[must_use]
    pub const fn measured() -> Self {
        Self {
            items: true,
            n3_mm: N3_NOISE_MM,
            t0_rel: T0_MESH_REL_NOISE,
            t0_by_ref: true,
            h1_old_fraction: H1_OLD_FRACTION,
            h2_attach_px: H2_ATTACH_NOISE_PX,
            h6_skim_fraction: H6_SKIM_NOISE_FRACTION,
            n4_fraction: N4_NOISE_FRACTION,
            cap_fraction: FLICKER_FRACTION,
        }
    }
    /// No allowance at all.
    #[must_use]
    pub const fn none() -> Self {
        Self { items: false, n3_mm: 0.0, t0_rel: 0.0, t0_by_ref: false, h1_old_fraction: 0.0, h2_attach_px: 0, h6_skim_fraction: 0.0, n4_fraction: 0.0, cap_fraction: 0.0 }
    }
    /// Band of an item code (see [`flicker_band`]); 0 when the allowance is off.
    #[must_use]
    pub fn band(&self, code: &str, scale: f64, worse: bool) -> f64 {
        if self.items { flicker_band(code, scale, worse) } else { 0.0 }
    }
    /// Crossing cap for an invariant that checked `checked` items.
    #[must_use]
    pub fn cap(&self, checked: usize) -> usize {
        (self.cap_fraction * checked as f64).floor() as usize
    }
}

// ---- H1 / H2 / N4 ----------------------------------------------------------
/// Lateral tolerance when grounding a cured component on the layer below, px (M: metric-design H1).
pub const H1_LATERAL_PX: i64 = 1;
/// A new floating component of this many px fails (M: jitter max 5 px, real defect 22 px; E for the limit, G5 pending).
pub const H1_FLOAT_PX: i64 = 16;
/// New floating voxels in total above this fail (S, plan).
pub const H1_FLOAT_TOTAL: i64 = 64;
/// A 3D piece of the candidate's floating material is "floating in both files" when at least this
/// many of its voxels lie within the band of the reference's floating material (E; a piece that
/// shares fewer is new material that happens to touch an old piece).
pub const H1_PIECE_MATCH_MIN_VOX: i64 = 8;
/// A piece that floats in both files may float over this fraction more of itself, or over
/// [`H1_FLOAT_TOTAL`] more voxels, whichever is larger, before the growth counts as new floating
/// material (S: the plan's 10 % band; G7: its break moved by a layer or two).
pub const H1_GROWTH_FRACTION: f64 = 0.10;
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
/// N2, declared contact merge: a merged twin lies within this height of the contact that took its
/// place, mm (E, G7: the Bat_Cat_Owl survivor sits 0.38 mm and 7 layers above the twin it absorbed;
/// the twin rule puts both in one column and raises the earlier arm to the later layer).
pub const N2_MERGE_DZ_MM: f64 = 0.5;
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_model_with_fewer_than_21_items_absorbs_no_flicker() {
        // The calibration saw 1 crossing in 21 items (Bat_Cat_Owl I7): below that, one
        // crossing is more than the measured share, so it is a finding and not flicker.
        assert_eq!(flicker_cap(0), 0);
        assert_eq!(flicker_cap(20), 0);
        assert_eq!(flicker_cap(21), 1);
        assert_eq!(flicker_cap(186), 8);
        assert_eq!(flicker_cap(1430), 68);
        assert_eq!(Noise::none().cap(1430), 0);
    }

    #[test]
    fn bands_are_per_code_and_unlisted_codes_get_none() {
        assert_eq!(flicker_band("GEOM_DEAD_MEMBER", 1.0, false), 0.0128);
        // a deficit band is a share of the item's own requirement
        assert!((flicker_band("MIN_SECTION_SHORT", 2.0, false) - 1.286).abs() < 1e-12);
        // ... and an item that already failed is allowed to grow only as far as noise grew one.
        assert!((flicker_band("MIN_SECTION_SHORT", 2.0, true) - 0.168).abs() < 1e-12);
        // No contact area deficit ever appeared under in-regime noise, so a halved contact is a finding.
        assert_eq!(flicker_band("CONTACT_AREA_SHORT", 2.0, false), 0.0);
        assert_eq!(flicker_band("CONTACT_AREA_SHORT", 2.0, true), 0.0);
        assert_eq!(Noise::none().band("GEOM_DEAD_MEMBER", 1.0, false), 0.0);
        assert!(flicker_band("SEAT_DEPTH_ZERO", 1.0, false).is_infinite(), "a yes/no code is limited by the cap alone");
        for code in ["PATH_FREE_END", "LOAD_BOOKS_DISAGREE", "GEOM_MESH_DISAGREES_WITH_RECORD", "GEOM_DEGENERATE", "NO_SUCH_CODE"] {
            assert_eq!(flicker_band(code, 1.0, false), 0.0, "{code} was never seen to move under noise");
            assert_eq!(flicker_band(code, 1.0, true), 0.0);
        }
    }
}
