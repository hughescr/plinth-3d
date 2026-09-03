//! **Regression ratchet: the flat round bases, on the slice-sweep placement
//! path, through the REAL dense orientation search.**
//!
//! The owner's gate for this project (2026-09-02/03 rulings) is that
//! `Round 25/50/100` — flat discs, 3.5 mm thick, with a 2.5 mm recess on
//! top — emit in seconds, FLAT (no tilt), recess UP, underside supported at
//! regular spacing, zero uncarried load, no refusal. `oracle_disc.rs` and
//! `pose_demand_probe.rs` already grade the PLACEMENT physics at a pinned
//! identity pose; neither runs the orientation search that decides whether
//! the disc lands identity-up or flipped in the first place, and the
//! existing legacy-engine ratchet (`crates/plinth-support/tests/
//! ratchet_harness.rs` + `support_ratchets.json`) measures the LEGACY engine
//! only. This file is the missing piece: it drives
//! [`plinth_orient::search_and_orient`] with the real
//! [`plinth_sweep::SweepDemand`] measurer — the same two calls
//! `crates/plinth-pipeline/src/prepare.rs`'s `orient_model` makes for
//! `OrientationPolicy::DenseSweep` — and then runs the production placement
//! sweep on whatever pose wins, exactly as `sweep_and_emit` does. A pinned
//! pose could not regress on this axis; this can.
//!
//! The hollow stage is skipped. `plinth_hollow` decides `Solid` for a
//! 3.5 mm-thick disc well under the hollowing size/wall thresholds
//! (`Hollowing is for large minis` — 30 mm medium minis are correctly
//! refused, and every disc here is thinner and, for Round 25, smaller than
//! that) and hands the oriented mesh back unchanged, so `hollowed.mesh()` in
//! `sweep_and_emit` is `oriented.mesh()` verbatim for this family. Skipping it
//! here is the same simplification `pose_demand_probe.rs` and `orient_demand.
//! rs` already make for these discs, not a new one.
//!
//! # The fixture
//!
//! The `Round_*.stl` files are run outputs and are not committed
//! (`.gitignore`:54); `make_recessed_disc` is the parametric family member
//! `oracle_disc.rs` and `pose_demand_probe.rs` already grade against, and
//! `round_25()`/`round_50()` below are copied from `pose_demand_probe.rs`'s
//! own fixtures verbatim so the three discs share one derivation. `round_100`
//! is new, at the same 1 mm rim draft and 2.5 mm/2.5 mm recess proportions,
//! checked against the real measured STL below.
//!
//! Measured against the real emitted/extracted STLs (`plinth-evidence/
//! slice-sweep-2026-09-02/orient-term/measure/{recess-up.txt,extract/
//! Round_*.log}`, `node flat-face-measure.mjs`), the parametric family's
//! predicted face areas land within 1.1% of the measured ones for all three
//! discs and both faces (worst case: Round 50's recessed face, 396.99
//! measured against 373.06 predicted, 6.4% — the STL's real recess carries a
//! small center bore this parametric family does not model; every other face
//! is within 1.1%), which is why the harness's own `AREA_IDENTITY` band
//! (8%) is reused below rather than a new one derived for this file.
//!
//! # Sabotage: what single change turns which assertion red
//!
//! | change | assertion that reddens |
//! |---|---|
//! | the reach test or the vertical-inheritance branch in `crates/plinth-sweep/src/reach.rs` is bypassed (S1/S2 in `sabotage.rs`) | `residual_demand_cells == 0` / `uncarried_load_mm2 == 0` on all three discs |
//! | the lattice is repitched at the FULL reach radius `ρ` instead of the octagon inradius `ρ/1.0824` (restoring the un-widened §7.4 reading) | the strut-count band (would fall below the 0.9× floor on the interior-dominated Round 100) |
//! | the reach/potential form reverts to spending cured thickness τ (the ~70× kappa extrapolation named in the 2026-09-02 commit) instead of the one-lamina clamp | the seat-spacing p95 bound (√3·ρ) and the strut-count band together, since spacing is what τ-spending was shown to distort |
//! | Round 25's pinned-defect assertion (`recess_up == false`) | reddens the day the scar-term fix lands and Round 25 orients correctly — **this one going red is success, not regression**, which is why its assertion carries the doc comment below rather than a bare `assert!` |
//!
//! # A NEGATIVE sabotage result, measured rather than assumed (2026-09-03)
//!
//! The obvious first candidate — `crates/plinth-sweep/src/orient_demand.rs`'s
//! `measure_one` reverting `support_area_cm2` from `burden_cm2(struts, ..)`
//! back to `plan.report.total_scar_mm2 / 100.0`, restoring the retired
//! flip-blind scar term — was run for real (`ratchet/sabotage.log`) and
//! **does NOT redden anything in this file.** `pose_demand_probe.rs` proves
//! the scar term ranks a 180° flip backwards in an isolated TWO-POSE
//! comparison, but the full dense search also scores height and peel, and
//! those terms already penalize a tilt/flip on their own for R50 and R100 —
//! their identity selection and 243/878 arm counts came back bit-identical
//! under the mutation. Round 25 was likewise unmoved (it was already
//! recess-down before the mutation, for the same pre-existing reason this
//! file pins it). **What this means:** this ratchet does not, by itself,
//! guard the support-term unit bridge for R50/R100 — it guards the observable
//! OUTCOME (pose, arm count, spacing) those terms currently produce, and a
//! defect that changes the support term's FORM without moving that outcome
//! is invisible here. `pose_demand_probe.rs`'s own assertions are what catch
//! that class of defect; this file and that one are complementary, not
//! redundant. Reverted in place, confirmed green again
//! (`ratchet/run3.log` before, identical result after — struts, spacing and
//! selection bit-for-bit unchanged).

mod harness;

use harness::{bands, hex_cell_area_mm2, lamina_reach_mm, layer_height_mm, x_allowed, z_span, LIFT_MM};
use plinth_core::{m0_constants, LayerPlan};
use plinth_decimate::{BudgetConfigError, ProbeConfig};
use plinth_mesh::{make_recessed_disc, Mesh};
use plinth_orient::grid::GridSettings;
use plinth_orient::{registered_sweep_request, search_and_orient, BaselineSelection, OrientedModel};
use plinth_sweep::{run_sweep, SweepDemand, SweepPlan, SweepPolicy};

/// One member of the `Round_*` family: the parametric dimensions
/// `make_recessed_disc` needs, plus the pose this file asserts.
struct Disc {
    name: &'static str,
    bottom_radius_mm: f64,
    top_radius_mm: f64,
    recess_radius_mm: f64,
    segments: usize,
    /// The pose the DENSE SEARCH is expected to commit to, MEASURED, not
    /// wished for. `true` for Round 50/100 (identity, recess up, the
    /// design's gate). `false` for Round 25 — the open scar-term defect
    /// named in the 2026-09-02/03 commits, pinned at its current truthful
    /// state so a silent regression back to it (from a future fixed state)
    /// is caught, and a fix is visible as this field flipping deliberately.
    expect_recess_up: bool,
    /// `None` for a correctly recess-up disc: its arm-count band is DERIVED
    /// from the hex covering of its demand face (see the assertion site).
    /// `Some(measured)` for Round 25: under the pinned 180° flip its demand
    /// surface at the plate is not a simple disk OR a simple ring — it is a
    /// 2.5 mm annular ring, a 2.5 mm-tall interior wall rising off that ring,
    /// AND the recess floor's downward-facing cap, which is a CAVITY CEILING
    /// that "must be supported from the cavity's lower interior" (owner
    /// ruling, `supports-may-seat-on-model`) — three load paths a single hex-
    /// or strip-covering formula does not capture cleanly. Rather than invent
    /// a multi-term formula for a geometry that is itself the open defect
    /// (and due to be deleted by the ruling this file's header names), the
    /// band is anchored to TODAY'S MEASUREMENT, at the same harness-licensed
    /// multiplicative width `bands::STRUT_COUNT` uses — a real ratchet
    /// against a gross regression, honestly not claiming a physics
    /// derivation this pose does not have.
    pinned_arms: Option<usize>,
}

const HEIGHT_MM: f64 = 3.5;
const RECESS_DEPTH_MM: f64 = 2.5;

fn round_25() -> Disc {
    Disc {
        name: "Round 25",
        bottom_radius_mm: 11.5,
        top_radius_mm: 12.5,
        recess_radius_mm: 10.0,
        segments: 96,
        expect_recess_up: false,
        // Measured 2026-09-03 (see ratchet/run1.log): 88 arms, spacing mean
        // 2.215 / p95 3.400 mm, residual 0, uncarried 0.
        pinned_arms: Some(88),
    }
}

fn round_50() -> Disc {
    Disc {
        name: "Round 50",
        bottom_radius_mm: 24.0,
        top_radius_mm: 25.0,
        recess_radius_mm: 22.5,
        segments: 128,
        expect_recess_up: true,
        pinned_arms: None,
    }
}

fn round_100() -> Disc {
    Disc {
        name: "Round 100",
        bottom_radius_mm: 49.0,
        top_radius_mm: 50.0,
        recess_radius_mm: 47.5,
        segments: 128,
        expect_recess_up: true,
        pinned_arms: None,
    }
}

fn mesh_for(disc: &Disc) -> Mesh {
    make_recessed_disc(
        disc.bottom_radius_mm,
        disc.top_radius_mm,
        HEIGHT_MM,
        RECESS_DEPTH_MM,
        disc.recess_radius_mm,
        disc.segments,
    )
    .unwrap_or_else(|error| panic!("{}: a valid disc profile: {error}", disc.name))
}

/// The production decimation probe configuration, read from the registry.
/// Mirrors `crates/plinth-pipeline/src/prepare.rs`'s `registered_decimation_
/// budget` verbatim; duplicated rather than depended on because `plinth-sweep`
/// depending on `plinth-pipeline` (which depends on `plinth-sweep`) would be a
/// cycle, dev-dependency or not.
fn registered_decimation_budget() -> plinth_decimate::DecimationBudget {
    let placeholders = m0_constants().expect("registry").placeholders();
    let probe = ProbeConfig::new(
        placeholders.decimation_probe_grid_pitch_mm().diagnostic().get(),
        usize::try_from(placeholders.decimation_probe_max_grid_bytes().diagnostic().get())
            .map_err(|_| BudgetConfigError::InvalidGridBytes { value: 0 })
            .expect("registered grid byte count fits a usize"),
        placeholders.decimation_probe_nearest_upper_bound_mm().diagnostic().get(),
    );
    let budget = plinth_decimate::DecimationBudget::ratified_default(probe)
        .expect("the ratified probe config builds a budget");
    budget.validate_for_execution().expect("the registered budget is executable");
    budget
}

/// Runs the REAL dense orientation search — the same `plinth_decimate::run`
/// then `plinth_orient::search_and_orient(.., Some(&SweepDemand))` pair
/// `orient_model` makes for `OrientationPolicy::DenseSweep` in
/// `crates/plinth-pipeline/src/prepare.rs` — so this ratchet exercises the
/// exact orientation stage production runs, not a stand-in.
///
/// # Panics
/// On any stage refusal — a refusal here is the property under test failing,
/// and this file's job is to fail loudly, not to hide it behind an `Err`.
fn orient_dense(mesh: &Mesh, name: &str) -> OrientedModel {
    let budget = registered_decimation_budget();
    let outcome = plinth_decimate::run(mesh, budget)
        .unwrap_or_else(|error| panic!("{name}: decimation must run: {error}"));
    let decimated = outcome
        .into_decimated_model()
        .unwrap_or_else(|scale| panic!("{name}: scale sanity must pass: {scale:?}"));

    let request = registered_sweep_request().expect("the registered sweep request");
    let demand = SweepDemand;
    search_and_orient(decimated, &request, Some(&demand))
        .unwrap_or_else(|error| panic!("{name}: the dense orientation search must not refuse: {error}"))
}

/// Runs the production placement sweep on an already-oriented mesh, with the
/// same layer-plan construction `sweep_and_emit`
/// (`crates/plinth-pipeline/src/prepare.rs`:2262-2280) uses: `plate_z =
/// min_z - lift`, one layer plan stopping at the model top plus one layer of
/// headroom. This is NOT `harness::sweep_with`'s own simplified plan (no
/// headroom layer) — this file exercises production's own construction, not
/// the oracle harness's.
fn place(mesh: &Mesh, name: &str) -> SweepPlan {
    let (min_z, max_z) = z_span(mesh);
    let plate_z = min_z - LIFT_MM;
    let height = layer_height_mm();
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "a layer count is a finite positive quotient of two model-scale lengths, ceil-ed and \
              clamped to at least one, exactly as prepare.rs's sweep_and_emit computes its own plan"
    )]
    let layer_count = (((max_z - plate_z) / height).ceil() as usize).saturating_add(1).max(1);
    let layers = LayerPlan::new(plate_z, vec![height; layer_count])
        .unwrap_or_else(|error| panic!("{name}: a uniform layer plan: {error}"));
    let policy = SweepPolicy::from_registry().expect("the registry supplies every sweep number");
    run_sweep(mesh, &layers, &policy)
        .unwrap_or_else(|error| panic!("{name}: the sweep emits and warns; it must not refuse: {error}"))
}

/// One flat face found by scanning triangles whose three vertices all sit
/// within `eps` of `target_z` — the Rust port of `plinth-evidence/
/// slice-sweep-2026-09-02/orient-term/measure/flat-face-measure.mjs`, same
/// arithmetic, same convention (`area` is the shoelace sum, `count` the
/// number of qualifying triangles, `r_min`/`r_max` the radial extent of their
/// vertices).
struct FlatFace {
    area_mm2: f64,
    count: usize,
    r_min: f64,
    r_max: f64,
}

fn flat_face(mesh: &Mesh, target_z: f64, eps: f64) -> FlatFace {
    let verts = mesh.verts();
    let mut area = 0.0_f64;
    let mut count = 0_usize;
    let mut r_min = f64::INFINITY;
    let mut r_max = 0.0_f64;
    for tri in mesh.tris() {
        let corners = [verts[tri[0] as usize], verts[tri[1] as usize], verts[tri[2] as usize]];
        if corners.iter().any(|vertex| (vertex[2] - target_z).abs() > eps) {
            continue;
        }
        let [a, b, c] = corners;
        area += ((b[0] - a[0]) * (c[1] - a[1]) - (c[0] - a[0]) * (b[1] - a[1])).abs() / 2.0;
        count += 1;
        for vertex in corners {
            let r = vertex[0].hypot(vertex[1]);
            r_min = r_min.min(r);
            r_max = r_max.max(r);
        }
    }
    FlatFace {
        area_mm2: area,
        count,
        r_min: if count > 0 { r_min } else { 0.0 },
        r_max,
    }
}

/// A geometric flatness/handedness tolerance. Not a registered constant —
/// there is nothing physical to register a mesh-vertex-coincidence epsilon
/// against — but `docs/evidence`'s own rule against typed constants is about
/// values that DECIDE an outcome; this one only decides whether a triangle
/// counts as "on the flat", a strictly conservative floor far below the
/// 0.0143 mm/layer rim creep `oracle_disc.rs` derives, so no legitimate flat
/// face could ever fail to qualify.
const FLAT_FACE_EPS_MM: f64 = 1.0e-6;

/// `true` when `selection` is identity, or a searched direction within the
/// dense search's own `ORIENT_DEDUPE_TOLERANCE_DEG` of vertical — the
/// tolerance the search itself uses to call two candidates "the same
/// direction", so a winner inside it is indistinguishable from identity by
/// the search's own yardstick, not by a tolerance invented for this test.
fn is_identity_within_search_tolerance(selection: &BaselineSelection) -> Option<f64> {
    match selection {
        BaselineSelection::Identity => Some(0.0),
        BaselineSelection::UpDirection { up_model_frame } => {
            let z = up_model_frame[2].get().clamp(-1.0, 1.0);
            Some(z.acos().to_degrees())
        }
        BaselineSelection::AxisDegrees { .. } => None,
    }
}

#[test]
fn round_bases_stay_flat_supported_and_round_25s_inversion_stays_pinned() {
    let dedupe_tolerance_deg = GridSettings::registered()
        .expect("registered grid settings")
        .dedupe_tolerance_deg();
    let lattice_pitch_mm = 3.0_f64.sqrt() * lamina_reach_mm();

    for disc in [round_25(), round_50(), round_100()] {
        let started = std::time::Instant::now();
        let mesh = mesh_for(&disc);

        // ---- (2a) orientation: run the REAL dense search ------------------
        let oriented = orient_dense(&mesh, disc.name);
        let orientation = oriented.history().orientation();
        let angle_off_vertical_deg = is_identity_within_search_tolerance(&orientation.selection);

        // ---- (1) placement: run the real production sweep, no refusal -----
        let plan = place(oriented.mesh(), disc.name);
        let report = &plan.report;
        let elapsed_s = started.elapsed().as_secs_f64();

        println!(
            "{name}: selection {selection:?}, {arms} arms, spacing mean {mean:.3} p95 {p95:.3} mm, \
             residual {residual}, uncarried {uncarried:.4} mm2, {elapsed_s:.2} s (wall time, not \
             asserted)",
            name = disc.name,
            selection = orientation.selection,
            arms = report.struts_total,
            mean = report.seat_spacing.mean_mm,
            p95 = report.seat_spacing.p95_mm,
            residual = report.residual_demand_cells,
            uncarried = report.uncarried_load_mm2,
        );

        // ---- (2b) pose: identity within the search's own tolerance, R50/R100 only
        if disc.expect_recess_up {
            let angle = angle_off_vertical_deg.unwrap_or_else(|| {
                panic!(
                    "{}: won an explicit AxisDegrees rotation ({:?}), not near-identity",
                    disc.name, orientation.selection
                )
            });
            assert!(
                angle <= dedupe_tolerance_deg,
                "{}: winning pose is {angle:.4} deg off vertical, outside the search's own dedupe \
                 tolerance {dedupe_tolerance_deg:.4} deg (selection {:?})",
                disc.name,
                orientation.selection,
            );
        }

        // ---- (2c) recess handedness, measured geometrically on the oriented mesh
        let (min_z, max_z) = z_span(oriented.mesh());
        let lo = flat_face(oriented.mesh(), min_z, FLAT_FACE_EPS_MM);
        let hi = flat_face(oriented.mesh(), max_z, FLAT_FACE_EPS_MM);
        assert!(
            lo.count > 0 && hi.count > 0,
            "{}: no flat face found at one of the z extremes (bottom {} tris, top {} tris); the \
             oriented mesh is TILTED, not flat",
            disc.name,
            lo.count,
            hi.count,
        );
        let recess_up = lo.area_mm2 > hi.area_mm2;
        println!(
            "{}: bottom z={min_z:.4} area {:.2} mm2 (r {:.3}..{:.3}), top z={max_z:.4} area {:.2} \
             mm2 (r {:.3}..{:.3}), recess up = {recess_up}",
            disc.name, lo.area_mm2, lo.r_min, lo.r_max, hi.area_mm2, hi.r_min, hi.r_max,
        );
        assert_eq!(
            recess_up, disc.expect_recess_up,
            "{}: recess handedness moved. {} — if this is Round 25 turning `true`, the scar-term \
             defect named in this file's header has been fixed: update `expect_recess_up` \
             deliberately, do not silence this assertion.",
            disc.name,
            if disc.expect_recess_up {
                "R50/R100 must stay recess UP; this is the owner's gate, not the open defect"
            } else {
                "Round 25 is PINNED recess DOWN, the open orientation-scar-term defect awaiting an \
                 owner ruling (see this file's header)"
            },
        );

        // ---- (3) zero uncarried load, no residual demand -------------------
        assert_eq!(
            report.residual_demand_cells, 0,
            "{}: left uncovered demand; a flat disc must not",
            disc.name
        );
        assert!(
            report.uncarried_load_mm2 <= 0.0,
            "{}: {} mm2 of load went uncarried",
            disc.name,
            report.uncarried_load_mm2
        );
        assert!(report.struts_total > 0, "{}: a disc needs a support field", disc.name);

        // ---- (4) arm count against a physics-derived band ------------------
        //
        // R50/R100 (recess up, correctly oriented): the demand face is the
        // drafted solid bottom, r = bottom_radius_mm (`oracle_disc.rs`'s own
        // derivation), hex-covered at `hex_cell_area_mm2()` and banded at the
        // harness's own already-licensed `bands::STRUT_COUNT` multiplicative
        // width.
        //
        // R25 (pinned 180 deg flip): no single hex- or strip-covering formula
        // applies cleanly — see `Disc::pinned_arms`'s doc comment — so the
        // same multiplicative band is applied around today's measurement
        // instead of an independent prediction.
        let (predicted, low, high) = if let Some(pinned) = disc.pinned_arms {
            let pinned = pinned as f64;
            (pinned, pinned * bands::STRUT_COUNT.0, pinned * bands::STRUT_COUNT.1)
        } else {
            let demand_face_mm2 = core::f64::consts::PI * disc.bottom_radius_mm * disc.bottom_radius_mm;
            let hex_count = (demand_face_mm2 / hex_cell_area_mm2()).ceil();
            (hex_count, hex_count * bands::STRUT_COUNT.0, hex_count * bands::STRUT_COUNT.1)
        };
        assert!(
            (report.struts_total as f64) >= low && (report.struts_total as f64) <= high,
            "{}: {} arms against a prediction of {predicted:.1} ({}), band [{low:.1}, {high:.1}] — \
             same licensed boundary factor as `crates/plinth-sweep/tests/harness/mod.rs`'s \
             `bands::STRUT_COUNT`",
            disc.name,
            report.struts_total,
            if disc.pinned_arms.is_some() {
                "pinned to today's measurement, see Disc::pinned_arms"
            } else {
                "hex covering of the drafted bottom face"
            },
        );

        // ---- (5) seat spacing p95 against the kappa-derived lattice pitch --
        assert!(
            report.seat_spacing.p95_mm <= lattice_pitch_mm,
            "{}: p95 seat spacing {:.3} mm exceeds the kappa-derived lattice pitch sqrt(3)*{:.4} \
             = {lattice_pitch_mm:.4} mm",
            disc.name,
            report.seat_spacing.p95_mm,
            lamina_reach_mm(),
        );

        // ---- the scar invariant, cross-checking the area law ----------------
        //
        // Graded against `injected_load_mm2`, not `peak_layer_area_mm2`: the
        // two coincide only on a flat-bottomed model (`report.rs`'s own
        // doc), which R50/R100 are and R25's pinned 180 deg flip — a ring,
        // a rising wall and a cavity ceiling — is not.
        assert!(
            (report.total_scar_mm2 - report.injected_load_mm2 / x_allowed()).abs()
                <= bands::AREA_IDENTITY * report.injected_load_mm2 / x_allowed(),
            "{}: total scar {:.3} mm2 against injected_load/X_ALLOWED {:.3} mm2 (band {:.0}%)",
            disc.name,
            report.total_scar_mm2,
            report.injected_load_mm2 / x_allowed(),
            bands::AREA_IDENTITY * 100.0,
        );
    }
}
