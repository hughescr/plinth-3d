//! Every pinned literal the audit needs, restated here.
//!
//! Nothing in this module is imported from `chitubox_io::consts`, from
//! `plinth-core`'s constants registry, or from any other engine crate. Each
//! value carries the citation it was copied from so a reader can check it
//! against the source of record without the audit ever linking to it
//! (`docs/specs/audit-contract.md` §1.1, §4).

// ---------------------------------------------------------------------------
// Container magics and header layout  (audit-contract.md §1.1)
// ---------------------------------------------------------------------------

/// First `u32` of every `.chitubox` file. `chitubox_io/src/consts.rs:8`.
pub const FILE_MAGIC: u32 = 0xAB23_1243;
/// First `u32` of every 72-byte support segment record. `consts.rs:10`.
pub const SEG_MAGIC: u32 = 0xEA34_2389;

pub const HEADER_LEN: usize = 24;
pub const H_MAGIC: usize = 0;
pub const H_MODEL_COUNT: usize = 4;
/// Model-table offset, called `T` in the contract.
pub const H_OFF8: usize = 8;
/// Settings-blob size: 0 when empty, 420 when populated. Two-valued, NOT a pin
/// (`audit-contract.md` §1.1 corrects the "constant" claim in `consts.rs:23`).
pub const H_SETTINGS_SIZE: usize = 12;
/// Pinned constant 412. `consts.rs:25`.
pub const H_CONST16: usize = 16;
/// Support-section anchor; must equal `off8 + 680 * model_count`, or 0.
pub const H_OFF20: usize = 20;

pub const SETTINGS_SIZE_POPULATED: u32 = 420;
pub const CONST16: u32 = 412;

// ---------------------------------------------------------------------------
// Model record  (audit-contract.md §1.1)
// ---------------------------------------------------------------------------

pub const MODEL_RECORD_LEN: usize = 680;
/// NUL-terminated name begins at +0 and is capped at 63 bytes on write.
pub const M_NAME: usize = 0;
pub const M_NAME_FIELD_LEN: usize = 256;
/// Largest name accepted on write; capacity unproven, conservative cap.
pub const MAX_NAME_LEN: usize = 63;
pub const M_TRANSLATE_X: usize = 256;
pub const M_TRANSLATE_Y: usize = 260;
/// Translate Z is the plate lift; the plate sits at `z = -translate_z` in the
/// model-centred frame (`audit-invariants.md` §1.2).
pub const M_TRANSLATE_Z: usize = 264;
/// ABSOLUTE offset of this model's support index; 0 means no supports.
pub const M_SUPPORT_INDEX_PTR: usize = 268;
/// ABSOLUTE offset of this model's mesh blob.
pub const M_MESH_OFF: usize = 272;
/// Mesh blob length in bytes, `36 * n_triangles`.
pub const M_MESH_SIZE: usize = 276;
/// Start of the unmapped tail measured all-zero in every specimen.
pub const M_UNMAPPED_TAIL: usize = 280;

// ---------------------------------------------------------------------------
// Support index and group header  (audit-contract.md §1.1)
// ---------------------------------------------------------------------------

pub const SUPPORT_INDEX_LEN: usize = 16;
pub const SI_N_RECORDS: usize = 0;
pub const SI_RECORDS_OFF: usize = 4;
pub const SI_MESH_OFF: usize = 8;
pub const SI_MESH_SIZE: usize = 12;

/// 400 bytes between the index end and `records_off`. Measured all-zero in
/// every specimen; a nonzero byte here is a finding, not ordinary data.
pub const GROUP_HEADER_LEN: usize = 400;

// ---------------------------------------------------------------------------
// Support segment record  (audit-contract.md §1.2)
// ---------------------------------------------------------------------------

pub const SEG_RECORD_LEN: usize = 72;
pub const S_MAGIC: usize = 0;
pub const S_TYPE: usize = 4;
pub const S_START: usize = 8;
pub const S_END: usize = 20;
/// A RADIUS in mm for types 1/3/4/7/9. For type 6 it is a DIAMETER (pad) or a
/// WIDTH (bridge) -- see [`crate::primitives`] and the note on
/// `RAFT_FIELDS_ARE_NOT_RADII` below.
pub const S_R1: usize = 32;
pub const S_R2: usize = 36;
pub const S_MESH_OFF: usize = 40;
pub const S_MESH_LEN: usize = 44;
pub const S_TRAILING: usize = 48;
pub const S_TRAILING_LEN: usize = 24;
pub const S_TRAILING_SLOTS: usize = 6;

/// **Units trap, and it is not in the contract document's field table.**
///
/// `audit-contract.md` §1.2 says +32 and +36 are radii, which is true for
/// types 1, 3, 4, 7 and 9. For type 6 the emitter passes different quantities
/// through those same slots (`crates/plinth-emit/src/lib.rs:636-637`,
/// `raft_pad_triangles(start, diameter, height, sides)` and
/// `raft_bridge_triangles(start, end, width, height)`):
///
/// * pad    -- `r1` is the pad DIAMETER, `r2` is the pad HEIGHT;
/// * bridge -- `r1` is the bridge WIDTH,  `r2` is the bridge HEIGHT.
///
/// Reading either as a radius lands a factor of two wrong. The audit decodes
/// them by role in [`crate::primitives::Primitive::from_record`] and reports
/// the discrepancy against the contract in the output's `notes`.
pub const RAFT_FIELDS_ARE_NOT_RADII: bool = true;

/// Reverse-engineered raft pad contract: 2.2 mm diameter, 0.2 mm height,
/// 80 triangles (`docs/specs/chitubox-format.md` §10 provenance note).
pub const RAFT_PAD_DIAMETER_MM: f64 = 2.2;
pub const RAFT_PAD_HEIGHT_MM: f64 = 0.2;
/// Reverse-engineered raft bridge contract: 12 triangles (a box).
pub const RAFT_BRIDGE_TRIANGLES: usize = 12;

// ---------------------------------------------------------------------------
// Mesh encoding  (audit-contract.md §1.5)
// ---------------------------------------------------------------------------

/// 9 `f32`, `v0.xyz v1.xyz v2.xyz`. No normal, no attribute, no header.
pub const MESH_BYTES_PER_TRI: usize = 36;
pub const FLOATS_PER_TRI: usize = 9;

/// Emitted UV-sphere tessellation: `2 * 16 * (8 - 1) = 224` triangles
/// (`plinth-emit/src/lib.rs:23-24`, `858-884`). Used only to CLASSIFY a file
/// as emitted rather than native; never to validate geometry.
pub const EMITTED_SPHERE_TRIANGLES: usize = 224;
/// Native ChituBox writes 480-triangle balls (`audit-contract.md` §2.1).
pub const NATIVE_SPHERE_TRIANGLES: usize = 480;

// ---------------------------------------------------------------------------
// Tolerances, all derived  (audit-invariants.md §1.5, audit-contract.md §1.7)
// ---------------------------------------------------------------------------

/// sqrt(3), spelled out because `std::f64::consts::SQRT_3` is unstable.
pub const SQRT_3: f64 = 1.732_050_807_568_877_2;

/// `2^-24`, the binary32 relative precision. Every coordinate and radius in
/// the file is `f32`, so this is the resolution of the evidence.
pub const EPS32: f64 = 5.960_464_477_539_063e-8;
/// `f64` machine epsilon.
pub const EPS64: f64 = 2.220_446_049_250_313e-16;
/// The project's derived summation headroom: relative bound `SUM_K * EPS64`.
/// `docs/development.md` tolerance rule; 4096 is headroom over the largest
/// corpus fold, on the order of 10^3 items.
pub const SUM_K: f64 = 4096.0;
/// Absolute leg of the summation bound, in the metric's own unit.
pub const SUM_ABS: f64 = 1.0e-9;

/// Coordinate half-ULP bound anywhere in the build volume
/// (`audit-contract.md` §1.7). Prefer [`tol_geom`]; this constant exists for
/// the few places that genuinely need a magnitude-free bound.
pub const COORD_TOL_MM: f64 = 1.0e-5;

/// Endpoint-coincidence tolerance for OUR OWN emitted artifacts
/// (`audit-contract.md` §3.2): 1000x above the worst observed rounding
/// (1e-6 mm) and 100x below the smallest observed real separation (1e-1 mm).
pub const JOIN_TOL_EMITTED_MM: f64 = 1.0e-3;
/// Endpoint-coincidence tolerance for NATIVE, UI-authored files. Four
/// coincidences on `two-supports.chitubox` land in (1e-4, 1e-2] mm.
pub const JOIN_TOL_NATIVE_MM: f64 = 2.0e-2;

// ---------------------------------------------------------------------------
// Printer raster  (audit-contract.md §4, audit-invariants.md §1.3, §1.5)
// ---------------------------------------------------------------------------

/// 14 um. Measured, vendor. `plinth-core/src/constants/evidence.rs:261-266`.
pub const PIXEL_PITCH_X_MM: f64 = 0.014;
/// 19 um. Measured, vendor. `evidence.rs:154-159`.
pub const PIXEL_PITCH_Y_MM: f64 = 0.019;
/// 0.050 mm. Measured, PROJECT RECORD -- not a vendor default.
/// `evidence.rs:210-220`, `plate-06-results.md` §1.
pub const LAYER_HEIGHT_DEFAULT_MM: f64 = 0.050;

pub const BUILD_VOLUME_X_MM: f64 = 211.68;
pub const BUILD_VOLUME_Y_MM: f64 = 118.37;
pub const BUILD_VOLUME_Z_MM: f64 = 220.0;
pub const LCD_PIXELS_X: u32 = 15_120;
pub const LCD_PIXELS_Y: u32 = 6_230;

/// The audit's own area censor: the smallest area the printer can render.
/// Deliberately far stricter than the engine's 0.16 mm^2 A3 coverage censor,
/// which is a sampling artefact of a 0.4 mm analysis pitch and has no standing
/// here (`audit-invariants.md` §1.5).
pub const PIXEL_AREA_MM2: f64 = PIXEL_PITCH_X_MM * PIXEL_PITCH_Y_MM;
/// The audit's own volume censor.
pub const VOXEL_VOLUME_MM3: f64 = PIXEL_AREA_MM2 * LAYER_HEIGHT_DEFAULT_MM;

/// In-layer pixel diagonal, 2.360085e-2 mm. Also the demand-sampling edge cap
/// (`audit-invariants.md` §2.1).
pub const D2_MM: f64 = 2.360_084_744_241_189_3e-2;
/// Voxel diagonal, 5.529014e-2 mm, and the width of the FUSION-UNCERTAIN band:
/// two solids separated by a gap in (0, d3] may or may not cure into one piece
/// and the audit refuses to decide (`audit-invariants.md` §1.5).
pub const D3_MM: f64 = 5.529_014_378_711_635e-2;

// ---------------------------------------------------------------------------
// Declared inputs  (audit-invariants.md §1.3)
// ---------------------------------------------------------------------------

/// **Owner ruling 2026-08-30.** Down from the retired 0.300 mm. Burial depth
/// is not an adhesion requirement; the only real failure is a rasterisation
/// gap, bounded by one pixel and one layer. Bound to the condition that
/// contact area still governs and depth never overrides it -- I2 enforces it.
pub const DEFAULT_CONTACT_DEPTH_MM: f64 = 0.100;
/// Upper bound the caller may not exceed: above the retired value the caller
/// is loosening a ruling rather than configuring an instrument.
pub const DEFAULT_CONTACT_DEPTH_MAX_MM: f64 = 0.300;

/// `g`: the registered FINAL clearance gap, 0.300 mm of true surface-to-surface
/// clearance between a support and the model (Craig ruling 2026-08-22,
/// `FINAL_CLEARANCE_GAP_MM`, `plinth-core/src/constants/evidence.rs`). The
/// `geometry` subcommand reads it for the skim law `(g + r_t)(sec x - 1) >
/// layer + pixel`. The file carries no gap field, so the registry is the only
/// source.
pub const FINAL_CLEARANCE_GAP_MM: f64 = 0.300;

/// Ball factor `f`. **Placeholder**, `BALL_FACTOR_STUB`.
pub const BALL_FACTOR_STUB: f64 = 1.5;
/// `X_ALLOWED = floor(25.00 / 2) = 12`. Derived: the arm snaps at
/// `supported_area / arm_section ~ 25`, and the design point is half of it.
pub const X_ALLOWED: f64 = 12.0;
/// Smallest admitted tip RADIUS. Derived, `evidence.rs:354-358`.
pub const TIP_RADIUS_MIN_ALLOWED_MM: f64 = 0.050;
/// Largest tip RADIUS. **Placeholder**, `TIP_RADIUS_MAX_LEGACY_MM`.
pub const TIP_RADIUS_MAX_MM: f64 = 1.000;

/// The unmeasured arm-radius placeholder the report counts against.
/// `MIN_SHOOT_R_MM`; 36-54 % of emitted arms sit at exactly this value.
pub const MIN_SHOOT_R_MM: f64 = 0.35;
/// Reference line only, never a threshold. `SPACING_FLOOR_MM`, Placeholder.
pub const SPACING_MINIMUM_REF_MM: f64 = 2.5;
/// Reference line only, never a threshold. `TARGET_SPACING_MM`, Placeholder.
pub const TARGET_SPACING_REF_MM: f64 = 4.0;

// ---------------------------------------------------------------------------
// Derived formulae
// ---------------------------------------------------------------------------

/// `ball_seat_factor(f) = f - sqrt(f*f - 1)`; `0.3819660112501051` at f = 1.5.
/// `docs/physics.md` §9, `plinth-support/src/contact.rs`.
#[must_use]
pub fn ball_seat_factor(f: f64) -> f64 {
    f - (f * f - 1.0).sqrt()
}

/// `tol_geom(p, q) = 4 * sqrt(3) * eps32 * max(|p|_inf, |q|_inf, 1.0)` mm.
///
/// Four rounded operations (two stored endpoints, one differencing, one norm),
/// three coordinates in quadrature. **Used as a function of magnitude, never
/// flattened to a constant** (`audit-invariants.md` §1.5). At a 100 mm
/// coordinate this is 4.130e-5 mm.
#[must_use]
pub fn tol_geom(p: [f64; 3], q: [f64; 3]) -> f64 {
    let m = inf_norm(p).max(inf_norm(q)).max(1.0);
    4.0 * SQRT_3 * EPS32 * m
}

/// [`tol_geom`] for a single point.
#[must_use]
pub fn tol_geom_at(p: [f64; 3]) -> f64 {
    tol_geom(p, p)
}

/// [`tol_geom`] for a bare magnitude in mm.
#[must_use]
pub fn tol_geom_mag(m: f64) -> f64 {
    4.0 * SQRT_3 * EPS32 * m.abs().max(1.0)
}

/// A stored radius against a reconstructed one: `EPS32 * r`, floored at 1e-9
/// (`audit-contract.md` §1.7).
#[must_use]
pub fn radius_tol_mm(r: f64) -> f64 {
    (EPS32 * r.abs()).max(1.0e-9)
}

/// The project's derived summation bound applied to one summed input.
/// `tol_sum(A) = max(SUM_K * EPS64 * |A|, SUM_ABS)` in the metric's own unit.
#[must_use]
pub fn tol_sum(a: f64) -> f64 {
    (SUM_K * EPS64 * a.abs()).max(SUM_ABS)
}

/// `tol_area(A) = max(SUM_K * eps64 * A / x_allowed, 1e-9)` mm^2
/// (`audit-invariants.md` §3.2). The area law's own tolerance, applied to the
/// one summed input `A_owned`.
#[must_use]
pub fn tol_area(a_owned_mm2: f64, x_allowed: f64) -> f64 {
    (SUM_K * EPS64 * a_owned_mm2.abs() / x_allowed).max(SUM_ABS)
}

fn inf_norm(p: [f64; 3]) -> f64 {
    p[0].abs().max(p[1].abs()).max(p[2].abs())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn derived_raster_quantities_match_the_contract() {
        assert!((PIXEL_AREA_MM2 - 2.66e-4).abs() < 1e-18);
        assert!((VOXEL_VOLUME_MM3 - 1.33e-5).abs() < 1e-19);
        let d2 = (PIXEL_PITCH_X_MM.powi(2) + PIXEL_PITCH_Y_MM.powi(2)).sqrt();
        assert!((d2 - D2_MM).abs() <= f64::EPSILON * d2, "d2 {d2}");
        let d3 =
            (PIXEL_PITCH_X_MM.powi(2) + PIXEL_PITCH_Y_MM.powi(2) + LAYER_HEIGHT_DEFAULT_MM.powi(2))
                .sqrt();
        assert!((d3 - D3_MM).abs() <= f64::EPSILON * d3, "d3 {d3}");
    }

    #[test]
    fn ball_seat_factor_at_the_stub_matches_the_pinned_value() {
        let v = ball_seat_factor(BALL_FACTOR_STUB);
        assert!((v - 0.381_966_011_250_105_1).abs() < 1e-15, "{v}");
    }

    #[test]
    fn the_axis_mapping_arithmetic_closes() {
        assert!((BUILD_VOLUME_X_MM / f64::from(LCD_PIXELS_X) - PIXEL_PITCH_X_MM).abs() < 1e-15);
        assert!((BUILD_VOLUME_Y_MM / f64::from(LCD_PIXELS_Y) - PIXEL_PITCH_Y_MM).abs() < 1e-15);
    }
}
