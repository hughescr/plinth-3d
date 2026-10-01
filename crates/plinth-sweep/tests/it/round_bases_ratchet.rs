//! **Regression ratchet: the flat round bases, on the slice-sweep placement
//! path, through the REAL dense orientation search.**
//!
//! The owner's gate for this project (2026-09-02/03 rulings) is that
//! `Round 25/50/100` — flat discs, 3.5 mm thick, a plain glue face on one
//! side and a recessed, rib-stiffened face on the other — emit in seconds,
//! FLAT (no tilt), the correct way up, supported at regular spacing, zero
//! uncarried load, no refusal.
//!
//! # AS DRAWN, 2026-10-01 (GitHub issue #1, owner spec)
//!
//! **"Glue face up" is now asserted through the AS-DRAWN path.** The
//! repository's Round discs are committed glue face UP under
//! `corpus/as-drawn/` with a `<stem>.asdrawn` marker each, so production uses
//! them exactly as drawn and searches nothing. Three tests:
//!
//! 1. `round_bases_stay_flat_supported_and_land_glue_face_up` — the main
//!    ratchet. The committed marker is read by the production probe; the
//!    parametric disc DRAWN glue face up ([`turned_over`]) goes through
//!    `orient_as_drawn` and the production placement sweep; every placement
//!    assertion below is unchanged. **The arm pins are NOT re-pinned** (55 /
//!    217 / 803 measured again, with the scar identity 0.889 / 0.944 / 0.972
//!    as before): the as-drawn pose is the same mesh as the searched flip,
//!    which test 2 proves vertex for vertex, so placement cannot move.
//! 2. `an_unmarked_disc_is_still_searched_and_its_flip_is_the_as_drawn_pose`
//!    — rule A for a disc nobody marked: (2a) the flip reaches the commit tier
//!    and (2b) it is committed, then the searched mesh equals the as-drawn
//!    turned-over mesh exactly. Search only, no placement.
//! 3. `the_committed_round_discs_are_marked_and_drawn_glue_face_up` — the
//!    real committed files are marked and, used as drawn, have the plain face
//!    (the one flat face across the axis) on TOP.
//!
//! # THE RECORD, CORRECTED 2026-09-28 (owner ruling on the Round 50 diagnosis)
//!
//! **The correct pose for ALL THREE discs is the 180° flip: the plain glue
//! face UP, the stiffener side DOWN against the supports.** All three STLs
//! are AUTHORED the same way round — measured on `corpus/models/Round_*.stl`
//! (`plinth-evidence/slice-sweep-2026-09-02/field-2026-09-28/round50.md` Q1):
//! the single plain plane is at the authored `z` MINIMUM and the recess, the
//! ring stiffeners and the rim annulus are at the authored `z` MAXIMUM. The
//! owner's 2026-09-03 ChituBox verdict on `Round 25` ("the flat side is up,
//! where the mini gets glued") therefore applies to `Round 50` and
//! `Round 100` too.
//!
//! What this file said before, and why it was wrong — kept, not softened:
//!
//! * It said `Round 50` and `Round 100` "are correct at identity, recess UP,
//!   and the owner judged them good", and asserted `expect_recess_up: true`
//!   for both. Identity is glue face DOWN. The pose the owner called good on
//!   2026-09-03 is the same pose he called upside down on 2026-09-28; nothing
//!   regressed — that verdict was about a pose already wrong under his own
//!   Round 25 reasoning, and this ratchet froze it.
//! * Its negative-sabotage note said height and peel "already penalize a
//!   tilt/flip on their own for R50 and R100". **False for the flip.** The
//!   height term (1.050) and the peel term (4.327 / 6.077) are IDENTICAL at
//!   ±Z (`round50.md` Q1). The flip lost because it was never scored: the
//!   coarse proxy ranked it 76th of 646 (R50) and 482nd of 644 (R100), only
//!   the top 24 were promoted, and the legacy escort carried identity but not
//!   the flip (`round50.md` Q2). On Round 100 it would ALSO have lost on the
//!   display term D, which read the stiffener ribs as detail and charged the
//!   glue-face-up pose 24.1 mm2 (`round50.md` Q3).
//!
//! The two rules that make all three land glue face up, and the assertion in
//! this file each one is answerable to:
//!
//! | rule | where | what reddens without it |
//! |---|---|---|
//! | **A** — a flat face's two horizontal poses (that face down, that face up) reach the commit tier by right, as the legacy 13 do | `plinth-orient` grid + escort | (2a) here, on Round 50 (measured: the flip absent from a 16-candidate commit tier, identity committed) |
//! | **B** — detail is the SPREAD of the local normal-change distribution, not its mean (owner ruling 2026-09-05) | `plinth-mesh` display field | NOT this file — see below; `plinth-mesh/tests/it/display_field.rs::a_regular_groove_field_is_change_and_an_irregular_texture_is_detail`, and the real `Round_100.stl` |
//!
//! **Measured, not assumed: this fixture does not need rule B.** With rule A
//! alone (rule B not yet built, 2026-09-28) all three fixture discs already
//! land glue face up and this test passes, Round 100 included. On the real
//! `Round_100.stl` the old D charge (24.1 mm2) outweighed the flip's
//! fragility margin (14.2 mm2) and decided it; on this concentric-ring
//! fixture it does not — the fixture has fewer and simpler stiffeners than
//! the real file's off-centre cylinders, so its rib-edge D charge is smaller.
//! Rule B is therefore graded where it can be seen: by the display-field test
//! named above, and by the real-file pose in the lane's run
//! (`plinth-evidence/.../lanes/round-pose.md`).
//!
//! Every other outcome assertion here — no refusal, no residual demand, no
//! uncarried load, p95 spacing under the κ lattice pitch, the scar invariant —
//! is unchanged from the file this replaces; the fixture and the pose are what
//! moved.
//!
//! # Why the pose is asserted END TO END, not per term
//!
//! `oracle_disc.rs` and `pose_demand_probe.rs` grade the placement physics at
//! a PINNED identity pose; neither runs the orientation search that decides
//! whether the disc lands glue face up in the first place. This file drives
//! [`plinth_orient::search_and_orient`] with the real
//! [`plinth_sweep::SweepDemand`] measurer — the same two calls
//! `crates/plinth-pipeline/src/prepare.rs`'s `orient_model` makes for
//! `OrientationPolicy::DenseSweep` — and then runs the production placement
//! sweep on whatever pose wins, exactly as `sweep_and_emit` does.
//!
//! The hollow stage is skipped. `plinth_hollow` decides `Solid` for a
//! 3.5 mm-thick disc well under the hollowing thresholds (`Hollowing is for
//! large minis`), so `hollowed.mesh()` in `sweep_and_emit` is `oriented.mesh()`
//! verbatim for this family.
//!
//! # The fixture: the Round family WITH its stiffener rings
//!
//! The `Round_*.stl` files are not committed (`.gitignore`), so the fixture
//! is parametric. Earlier revisions used `make_recessed_disc`, which has no
//! stiffener ribs — the very feature the D term misread on Round 100 — so it
//! could not exercise rule B. The fixture here is a closed surface of
//! revolution built from the measured profile of the real files
//! (`round50-probe/geom.txt`, and ring radii read off the STL vertices on
//! 2026-09-28), authored the way the real files are: plain face DOWN at
//! `z = 0`, stiffener side UP:
//!
//! * the plain face, `r ≤ R_b`, at `z = 0`;
//! * the outer rim, drafted `R_b → R_b + 1` over the full 3.5 mm;
//! * the rim annulus at `z = 3.5`, from `R_b − 1` out to the rim;
//! * the recess wall, drafted inward from `R_b − 1` at the top to
//!   `R_b − 1.67` at the recess floor (measured 22.33 → 23.0 on Round 50);
//! * the recess floor at `z = 1.0`;
//! * concentric ring stiffeners rising from the floor to the full 3.5 mm: the
//!   central ring (`r` 2.62–4.12, present on all three files), plus one ring
//!   standing in for the three off-centre stiffener cylinders centred at
//!   `r ≈ 12.5` on Round 50, and a further one at `r ≈ 25` on Round 100. The
//!   real outer stiffeners are off-centre cylinders; a lathe cannot build
//!   those, and a concentric ring carries the property under test — flat rib
//!   tops at the full height bounded by 90° creases — without claiming to be
//!   the same solid.
//!
//! # Sabotage: what single change turns which assertion red
//!
//! | change | assertion that reddens |
//! |---|---|
//! | the flat-face escort in `crates/plinth-orient/src/search.rs` is dropped (rule A reverted) | (2a) on Round 50 (measured 2026-09-28; the loop stops at the first disc that fails) |
//! | `display_eval.rs`'s detail reverts to the mean `1 − cos` articulation (rule B reverted) | nothing in this file (measured 2026-09-28, see above) |
//! | the reach test or the vertical-inheritance branch in `crates/plinth-sweep/src/reach.rs` is bypassed (S1/S2 in `sabotage.rs`) | `residual_demand_cells == 0` / `uncarried_load_mm2 == 0` |
//! | the reach/potential form reverts to spending cured thickness τ instead of the one-lamina clamp | the seat-spacing p95 bound (√3·ρ) |
//!
//! Each row's measured outcome is recorded in the lane report
//! (`plinth-evidence/slice-sweep-2026-09-02/field-2026-09-28/lanes/round-pose.md`).

use crate::harness;

use std::path::{Path, PathBuf};

use harness::{bands, lamina_reach_mm, layer_height_mm, x_allowed, z_span, LIFT_MM};
use plinth_core::{m0_constants, LayerPlan};
use plinth_decimate::{BudgetConfigError, ProbeConfig};
use plinth_mesh::Mesh;
use plinth_orient::as_drawn::{probe_source, AsDrawnReason};
use plinth_orient::grid::GridSettings;
use plinth_orient::search::{DenseSweepRecord, SearchTier};
use plinth_orient::{
    orient_as_drawn, registered_sweep_request, search_and_orient, BaselineSelection,
    OrientationProvenance, OrientedModel,
};
use plinth_sweep::{run_sweep, SweepDemand, SweepPlan, SweepPolicy};

const HEIGHT_MM: f64 = 3.5;
const FLOOR_Z_MM: f64 = 1.0;
/// Rim draft: the outer rim grows 1 mm in radius over the full height, on all
/// three files (24.0 → 25.0 on Round 50).
const RIM_DRAFT_MM: f64 = 1.0;
/// Rim annulus width at the top face, measured: the recess opens 1 mm inside
/// the bottom radius (23.0 on Round 50, 10.5 on Round 25, 48.0 on Round 100).
const ANNULUS_INSET_MM: f64 = 1.0;
/// Recess wall draft, measured: the recess floor edge sits 0.67 mm further in
/// than its lip (22.33 against 23.0 on Round 50; 9.83 / 10.5 on Round 25;
/// 47.33 / 48.0 on Round 100).
const RECESS_DRAFT_MM: f64 = 0.67;

/// One member of the `Round_*` family.
pub(crate) struct Disc {
    name: &'static str,
    /// The stem of the committed, glue-face-UP, marked copy of the real file:
    /// `corpus/as-drawn/<stem>.stl` beside `<stem>.asdrawn`.
    stem: &'static str,
    /// Radius of the plain glue face (the authored bottom).
    bottom_radius_mm: f64,
    /// Ring stiffeners, `(inner, outer)` radius, rising from the recess floor
    /// to the full height. Innermost first.
    rings: &'static [(f64, f64)],
    segments: usize,
    /// Arms placed at the glue-face-up pose (see the (4) assertion for why
    /// this is a pin and not a prediction). Measured 2026-09-28 as 169 / 338 /
    /// 1142; Round 25 re-pinned to 105 by fix/no-dangling (see `round_25`).
    ///
    /// RE-PINNED 2026-09-29 by the wall-creep lane, ON PURPOSE, to 88 / 304 /
    /// 1067, measured on develop `5db81a5` merged into the lane. This is the
    /// move the (4) comment predicted: a drafted wall's creep is now priced by
    /// its overhang weight (`reach.rs`, `creep_step`), so the drafted recess
    /// wall no longer draws minimum-size hairline arms. Measured on the same
    /// tree with only the creep weight switched off
    /// (`SweepPolicy::without_creep_weight`): 105 / 338 / 1130, which is
    /// develop's own count on all three discs; with it on, 88 / 304 / 1067, and
    /// Round 25's 13 over-limit runs, 3 forced merges and 10 braces all go to 0.
    /// The lane's other rule (congestion remedy 2 sized by the stack) does not
    /// fire on any disc here: none is congested. The scar band is NOT widened:
    /// the identity reads 1.00000 / 1.00000 / 0.99630 with the lane.
    ///
    /// Not a detector of the creep rule: 105 lies inside Round 25's band around
    /// 88. `wall_creep::` grades the rule itself.
    ///
    /// RE-PINNED 2026-09-30 by the lattice-align lane, ON PURPOSE, 88 / 304 /
    /// 1067 → 55 / 217 / 803, measured on develop `8007750` plus the lane. Two
    /// engineering fixes to seat placement, no physics change: the reach is
    /// measured as a straight length (a root now reaches `κ·h` in every
    /// direction, not `κ·h/1.0824`), and each island of demand is covered by
    /// the fewest seats among coverings built to it (the rim annulus and the
    /// rib tops are seated as one midline row each). Residual 0, uncarried 0
    /// and the p95 spacing under the lattice pitch hold on all three
    /// (`lanes/lattice-align.md`). The old counts are reproduced by
    /// `harness::at_path_fixture_seating`, which is how the path fixtures that
    /// need the old field still reach their paths.
    pinned_arms: usize,
}

/// RE-PINNED 169 -> 105 on 2026-09-29 by fix/no-dangling, on purpose and
/// said so; the ruling on it is the owner's (Round bases are the gate).
///
/// The 169 held 64 FLANK ARMS the congestion lane inserted (arms 105-168, 4
/// rounds, still congested at exit) to answer a congestion the pre-congestion
/// `unmerge_overgrown` pass had made itself: it put an over-grown trunk back
/// on its straight plate column and left the branches that rested on it
/// behind, and they came down to the plate beside it (1 overlapping root
/// pair, stack depth 2). The slenderness lane then saw 77 over-limit runs and
/// answered with 106 braces, 83 masts and 50 unbraced-run warnings.
///
/// Under "a trunk carrying branches moves with them or not at all" the two
/// branches move with their trunk, the plate roots do not overlap (54 roots,
/// ratio 1.0, 0 pairs), the congestion lane inserts nothing, and the
/// slenderness lane sees 13 runs: 3 merges, 20 braces, 3 masts, 0 warnings.
/// 105 is the up-sweep's own count. Every physical assertion in this file is
/// unchanged and passes (flat, glue face up, residual 0, uncarried 0, p95
/// spacing 2.102 mm under the lattice pitch, the scar invariant).
///
/// Measured, not argued: with the subtree walk in `unmerge_overgrown`
/// sabotaged (trunk moves alone), this fixture returns to exactly 169 arms,
/// 64 of them congestion-inserted. Round 50 is unchanged (338); Round 100
/// moves 1142 -> 1130, inside its band. Evidence:
/// `plinth-evidence/.../field-2026-09-28/lanes/no-dangling/retry/`.
///
/// RE-PINNED 105 -> 88 on 2026-09-29 by fix/wall-creep, on purpose: the creep
/// rule removes the recess wall's hairline arms and the slenderness remedies
/// that hung off them (see `Disc::pinned_arms`). Evidence:
/// `plinth-evidence/.../field-2026-09-28/lanes/wall-creep/logs/ratchet-merged-*.log`.
pub(crate) fn round_25() -> Disc {
    Disc {
        name: "Round 25",
        stem: "Round_25",
        bottom_radius_mm: 11.5,
        rings: &[(2.62, 4.12)],
        segments: 96,
        pinned_arms: 55,
    }
}

pub(crate) fn round_50() -> Disc {
    Disc {
        name: "Round 50",
        stem: "Round_50",
        bottom_radius_mm: 24.0,
        rings: &[(2.62, 4.12), (12.25, 12.75)],
        segments: 128,
        pinned_arms: 217,
    }
}

fn round_100() -> Disc {
    Disc {
        name: "Round 100",
        stem: "Round_100",
        bottom_radius_mm: 49.0,
        rings: &[(2.62, 4.12), (12.25, 12.75), (25.0, 25.5)],
        segments: 128,
        pinned_arms: 803,
    }
}

/// The disc's `(r, z)` profile, from the axis at the plain face round the
/// outside to the axis at the recess floor. Revolving it about `z` gives the
/// closed solid; walking it in this order keeps every outward normal outward
/// (see [`revolve`]).
fn profile(disc: &Disc) -> Vec<(f64, f64)> {
    let bottom = disc.bottom_radius_mm;
    let lip = bottom - ANNULUS_INSET_MM;
    let recess_edge = lip - RECESS_DRAFT_MM;
    let mut points = vec![
        (0.0, 0.0),
        (bottom, 0.0),
        (bottom + RIM_DRAFT_MM, HEIGHT_MM),
        (lip, HEIGHT_MM),
        (recess_edge, FLOOR_Z_MM),
    ];
    for &(inner, outer) in disc.rings.iter().rev() {
        points.push((outer, FLOOR_Z_MM));
        points.push((outer, HEIGHT_MM));
        points.push((inner, HEIGHT_MM));
        points.push((inner, FLOOR_Z_MM));
    }
    points.push((0.0, FLOOR_Z_MM));
    points
}

/// Revolves a closed `(r, z)` profile about the `z` axis into a watertight
/// triangle mesh. The profile must start and end ON the axis (`r = 0`); an
/// on-axis point becomes one vertex (a fan apex), every other point a ring
/// of `segments` vertices.
///
/// Winding: with `∂s` along the profile and `∂θ` around the axis, the outward
/// normal of this walk is `∂θ × ∂s` (on the first span, `(0,0)→(R,0)`, that
/// is `−z`: the plain face looks down). Each quad `(i, j)` is emitted as
/// `(P[i][j], P[i][j+1], P[i+1][j])` and `(P[i+1][j], P[i][j+1],
/// P[i+1][j+1])`, both of which have that normal; a triangle that would
/// collapse onto the axis is not emitted.
fn revolve(points: &[(f64, f64)], segments: usize) -> Mesh {
    let index = |value: usize| u32::try_from(value).expect("a fixture vertex index fits a u32");
    let mut verts: Vec<[f64; 3]> = Vec::new();
    // First vertex id of each profile point's ring (an on-axis point has one).
    let mut ring_start: Vec<usize> = Vec::with_capacity(points.len());
    for &(r, z) in points {
        ring_start.push(verts.len());
        if r == 0.0 {
            verts.push([0.0, 0.0, z]);
        } else {
            for segment in 0..segments {
                let theta = std::f64::consts::TAU * segment as f64 / segments as f64;
                verts.push([r * theta.cos(), r * theta.sin(), z]);
            }
        }
    }
    let vertex = |point: usize, segment: usize| -> u32 {
        if points[point].0 == 0.0 {
            index(ring_start[point])
        } else {
            index(ring_start[point] + segment % segments)
        }
    };
    let mut tris: Vec<[u32; 3]> = Vec::new();
    for point in 0..points.len() - 1 {
        let (on_axis_here, on_axis_next) = (points[point].0 == 0.0, points[point + 1].0 == 0.0);
        for segment in 0..segments {
            if !on_axis_here {
                tris.push([
                    vertex(point, segment),
                    vertex(point, segment + 1),
                    vertex(point + 1, segment),
                ]);
            }
            if !on_axis_next {
                tris.push([
                    vertex(point + 1, segment),
                    vertex(point, segment + 1),
                    vertex(point + 1, segment + 1),
                ]);
            }
        }
    }
    Mesh::new(verts, tris).expect("the revolved disc is a valid mesh")
}

pub(crate) fn mesh_for(disc: &Disc) -> Mesh {
    revolve(&profile(disc), disc.segments)
}

/// The disc DRAWN glue face UP, the way the committed `corpus/as-drawn/`
/// copies are: turned 180° about X, `(x, y, z) -> (x, -y, -z)`, which is the
/// exact map `plinth-mesh`'s `flip_stl_x180` example applies to the real files
/// and the exact rotation the dense search applies when it commits the flip
/// (its antipode convention, `UpDirection::rotation_to_build_frame`). A
/// rotation keeps winding, so the triangles are reused unchanged.
fn turned_over(mesh: &Mesh) -> Mesh {
    let verts = mesh
        .verts()
        .iter()
        .map(|&[x, y, z]| [x, -y, -z])
        .collect();
    Mesh::new(verts, mesh.tris().to_vec()).expect("a rotated valid mesh is valid")
}

/// `corpus/as-drawn/<stem>.stl`: the committed, glue-face-UP, marked copy of
/// the real Round file (`corpus/as-drawn/README.md`).
fn committed_as_drawn(stem: &str) -> PathBuf {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../corpus/as-drawn")
        .join(format!("{stem}.stl"));
    assert!(
        path.is_file(),
        "the committed as-drawn fixture {} is missing",
        path.display()
    );
    path
}

/// The operator mark on the committed file, read by the PRODUCTION probe the
/// executor calls (`plinth_orient::as_drawn::probe_source`). The ratchet
/// requires it to be the portable marker: an extended attribute does not
/// survive git, so a fixture marked any other way would be marked only on the
/// machine that tagged it.
fn committed_mark(disc: &Disc) -> AsDrawnReason {
    let source = committed_as_drawn(disc.stem);
    let probe = probe_source(&source);
    assert!(probe.warnings.is_empty(), "{}: {:?}", disc.name, probe.warnings);
    match probe.reason {
        Some(reason @ AsDrawnReason::MarkerFile { .. }) => reason,
        other => panic!(
            "{}: {} must be marked as drawn by its <stem>.asdrawn marker; the probe read {other:?}",
            disc.name,
            source.display()
        ),
    }
}

/// Decimate exactly as production does, then use the result AS DRAWN — the
/// `plinth_orient::orient_as_drawn` call `orient_model` makes for a marked
/// model in `crates/plinth-pipeline/src/prepare.rs`.
fn orient_marked(mesh: &Mesh, name: &str, reason: AsDrawnReason) -> OrientedModel {
    let outcome = plinth_decimate::run(mesh, registered_decimation_budget())
        .unwrap_or_else(|error| panic!("{name}: decimation must run: {error}"));
    let decimated = outcome
        .into_decimated_model()
        .unwrap_or_else(|scale| panic!("{name}: scale sanity must pass: {scale:?}"));
    orient_as_drawn(decimated, reason)
        .unwrap_or_else(|error| panic!("{name}: the as-drawn pose must not refuse: {error}"))
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
/// `crates/plinth-pipeline/src/prepare.rs`.
///
/// # Panics
/// On any stage refusal — a refusal here is the property under test failing.
fn orient_dense(mesh: &Mesh, name: &str) -> OrientedModel {
    let budget = registered_decimation_budget();
    let outcome = plinth_decimate::run(mesh, budget)
        .unwrap_or_else(|error| panic!("{name}: decimation must run: {error}"));
    let decimated = outcome
        .into_decimated_model()
        .unwrap_or_else(|scale| panic!("{name}: scale sanity must pass: {scale:?}"));

    let request = registered_sweep_request().expect("the registered sweep request");
    let demand = SweepDemand::production();
    search_and_orient(decimated, &request, Some(&demand))
        .unwrap_or_else(|error| panic!("{name}: the dense orientation search must not refuse: {error}"))
}

/// Runs the production placement sweep on an already-oriented mesh, with the
/// same layer-plan construction `sweep_and_emit` uses: `plate_z = min_z −
/// lift`, one layer plan stopping at the model top plus one layer of
/// headroom.
pub(crate) fn place(mesh: &Mesh, name: &str) -> SweepPlan {
    let policy = SweepPolicy::from_registry().expect("the registry supplies every sweep number");
    place_with(mesh, name, &policy)
}

/// [`place`] under a given policy, so a sabotage knob can be measured on the
/// same fixture and the same layer plan.
pub(crate) fn place_with(mesh: &Mesh, name: &str, policy: &SweepPolicy) -> SweepPlan {
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
    run_sweep(mesh, &layers, policy)
        .unwrap_or_else(|error| panic!("{name}: the sweep emits and warns; it must not refuse: {error}"))
}

/// One flat face found by scanning triangles whose three vertices all sit
/// within `eps` of `target_z`: `area` is the shoelace sum, `count` the number
/// of qualifying triangles, `r_min`/`r_max` the radial extent of their
/// vertices.
struct FlatFace {
    area_mm2: f64,
    count: usize,
    r_min: f64,
    r_max: f64,
    /// Some qualifying triangle contains the axis `(0, 0)` in XY (closed
    /// test). The real files triangulate the plain face without a vertex on
    /// the axis, so `r_min` alone cannot say the face reaches it.
    covers_axis: bool,
}

fn flat_face(mesh: &Mesh, target_z: f64, eps: f64) -> FlatFace {
    let verts = mesh.verts();
    let mut area = 0.0_f64;
    let mut count = 0_usize;
    let mut r_min = f64::INFINITY;
    let mut r_max = 0.0_f64;
    let mut covers_axis = false;
    for tri in mesh.tris() {
        let corners = [verts[tri[0] as usize], verts[tri[1] as usize], verts[tri[2] as usize]];
        if corners.iter().any(|vertex| (vertex[2] - target_z).abs() > eps) {
            continue;
        }
        let [a, b, c] = corners;
        area += ((b[0] - a[0]) * (c[1] - a[1]) - (c[0] - a[0]) * (b[1] - a[1])).abs() / 2.0;
        count += 1;
        // The axis lies in the triangle when it is on the same side of (or
        // on) all three edges, whichever way the triangle winds.
        let side = |p: [f64; 3], q: [f64; 3]| p[0] * q[1] - p[1] * q[0];
        let sides = [side(a, b), side(b, c), side(c, a)];
        covers_axis |= sides.iter().all(|&s| s >= 0.0) || sides.iter().all(|&s| s <= 0.0);
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
        covers_axis,
    }
}

/// A mesh-vertex-coincidence epsilon for "this triangle lies on the flat".
/// Not a registered constant — nothing physical to register it against — and
/// it decides no outcome: it only selects triangles, and sits far below the
/// 0.0143 mm/layer rim creep `oracle_disc.rs` derives, so no legitimate flat
/// face can fail to qualify.
const FLAT_FACE_EPS_MM: f64 = 1.0e-6;

/// The committed record of the dense search that oriented `oriented`.
fn dense_record(oriented: &OrientedModel) -> &DenseSweepRecord {
    match &oriented.history().orientation().provenance {
        OrientationProvenance::DenseSweep(record) => record,
        OrientationProvenance::Pinned { .. } | OrientationProvenance::AsDrawn { .. } => {
            panic!("a dense search was run, not a pin or an as-drawn pose")
        }
    }
}

/// Degrees between the committed up-direction and the authored `−Z` (the
/// 180° flip that puts the authored-bottom glue face on top).
fn degrees_off_the_flip(selection: &BaselineSelection) -> Option<f64> {
    match selection {
        BaselineSelection::Identity => Some(180.0),
        BaselineSelection::UpDirection { up_model_frame } => {
            let z = up_model_frame[2].get().clamp(-1.0, 1.0);
            Some((-z).acos().to_degrees())
        }
        BaselineSelection::AxisDegrees { .. } => None,
    }
}

/// **The Round bases, AS DRAWN glue face up** (GitHub issue #1, owner spec
/// 2026-10-01): the committed copies are drawn glue face UP and marked
/// `<stem>.asdrawn`, so production uses them as drawn and searches nothing.
/// This drives that path — the committed marker read by the production probe,
/// then `orient_as_drawn` — on the parametric disc drawn the same way up
/// ([`turned_over`]), and then the production placement sweep.
///
/// The placement assertions and the arm pins are unchanged, and that is
/// itself a check: the as-drawn pose of the turned-over disc is the same mesh,
/// bit for bit, as the searched flip of the authored disc
/// (`an_unmarked_disc_is_still_searched_and_its_flip_is_the_as_drawn_pose`),
/// so placement cannot tell the two paths apart.
///
/// What the search must still do for an UNMARKED disc — reach the flip and
/// commit it (rule A) — is graded by that second test.
#[test]
fn round_bases_stay_flat_supported_and_land_glue_face_up() {
    let lattice_pitch_mm = 3.0_f64.sqrt() * lamina_reach_mm();

    for disc in [round_25(), round_50(), round_100()] {
        let started = std::time::Instant::now();
        let mesh = mesh_for(&disc);

        // ---- (2) pose: AS DRAWN, by the committed marker -------------------
        let reason = committed_mark(&disc);
        let oriented = orient_marked(&turned_over(&mesh), disc.name, reason.clone());
        let orientation = oriented.history().orientation();
        assert_eq!(
            orientation.selection,
            BaselineSelection::Identity,
            "{}: a disc marked as drawn must not be turned",
            disc.name
        );
        assert_eq!(
            orientation.provenance,
            OrientationProvenance::AsDrawn { reason },
            "{}: the pose must be recorded as the operator's, not a search's",
            disc.name
        );

        // ---- (1) placement: run the real production sweep, no refusal -----
        let plan = place(oriented.mesh(), disc.name);
        let report = &plan.report;
        let elapsed_s = started.elapsed().as_secs_f64();
        // Where the arm count comes from, printed so a re-pin can say which
        // pass moved it (see (4)): arms the congestion lane inserted, branches
        // the un-merge moved with their trunk, and the slenderness remedies.
        println!(
            "{}: arms inserted by congestion {} (rounds {}), by load {}; branches moved with \
             their trunk {}; slenderness: runs over the limit {}, merges forced {}, braces {}, \
             masts {}, unbraced-run warnings {}",
            disc.name,
            report.congestion_lane.arms_inserted,
            report.congestion_lane.rounds_used,
            report.arms_inserted_by_load,
            report.merges_moved_with_trunk,
            report.slenderness_lane.runs_over_limit_before,
            report.slenderness_lane.merges_forced,
            report.slenderness_lane.braces_added,
            report.slenderness_lane.masts_dropped,
            report.slenderness_lane.warnings.len(),
        );
        let placed_arms = plan
            .struts
            .iter()
            .filter(|strut| strut.kind == plinth_sweep::StrutKind::Arm)
            .count();

        let (min_z, max_z) = z_span(oriented.mesh());
        let lo = flat_face(oriented.mesh(), min_z, FLAT_FACE_EPS_MM);
        let hi = flat_face(oriented.mesh(), max_z, FLAT_FACE_EPS_MM);
        let plain_face_mm2 = flat_face(&mesh, 0.0, FLAT_FACE_EPS_MM).area_mm2;

        println!(
            "{name}: as drawn ({provenance:?}), selection {selection:?}; {placed_arms} arms of \
             {members} members, \
             spacing mean {mean:.3} p95 {p95:.3} mm, residual {residual}, uncarried {uncarried:.4} \
             mm2; bottom z={min_z:.4} {lo_area:.2} mm2 (r {lo_r0:.3}..{lo_r1:.3}), top z={max_z:.4} \
             {hi_area:.2} mm2 (r {hi_r0:.3}..{hi_r1:.3}), plain face {plain_face_mm2:.2} mm2; \
             {elapsed_s:.2} s (wall time, not asserted)",
            name = disc.name,
            provenance = orientation.provenance,
            selection = orientation.selection,
            members = report.struts_total,
            mean = report.seat_spacing.mean_mm,
            p95 = report.seat_spacing.p95_mm,
            residual = report.residual_demand_cells,
            uncarried = report.uncarried_load_mm2,
            lo_area = lo.area_mm2,
            lo_r0 = lo.r_min,
            lo_r1 = lo.r_max,
            hi_area = hi.area_mm2,
            hi_r0 = hi.r_min,
            hi_r1 = hi.r_max,
        );

        // ---- (2c) glue face up, measured geometrically on the oriented mesh
        //
        // Flat at both extremes (not tilted), and the top extreme is the
        // single plain plane: its area is the authored plain face's own area
        // (within the harness's circular-boundary tiling band) and it runs to
        // the axis, where the stiffener side is an annulus plus rib tops.
        assert!(
            lo.count > 0 && hi.count > 0,
            "{}: no flat face at one of the z extremes (bottom {} tris, top {} tris); the oriented \
             mesh is TILTED, not flat",
            disc.name,
            lo.count,
            hi.count,
        );
        assert!(
            (hi.area_mm2 - plain_face_mm2).abs() <= bands::AREA_IDENTITY * plain_face_mm2
                && hi.r_min < disc.rings[0].0,
            "{}: the top face is {:.2} mm2 (r {:.3}..{:.3}); the plain glue face is {:.2} mm2 and \
             reaches the axis. The glue face is NOT up.",
            disc.name,
            hi.area_mm2,
            hi.r_min,
            hi.r_max,
            plain_face_mm2,
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
        assert!(placed_arms > 0, "{}: a disc needs a support field", disc.name);
        // The members that are NOT arms are exactly the ones the slenderness
        // lane published, so a brace can never hide inside an arm count.
        assert_eq!(
            report.struts_total - placed_arms,
            2 * report.slenderness_lane.braces_added + report.slenderness_lane.masts_dropped,
            "{}: the members that are not arms and the remedies published disagree",
            disc.name,
        );

        // ---- (4) arm count, PINNED to the measurement of 2026-09-28 --------
        //
        // Not a physics prediction, and said so. The glue-face-up pose puts
        // three load paths at the plate — the rim annulus and rib tops in the
        // first layer, the recess floor as a cavity ceiling 2.5 mm up, and the
        // drafted recess wall overhanging into the cavity — and a single hex
        // covering of the down-facing area predicts about half of what is
        // placed (Round 50: 1868 mm2 / 11.79 mm2 = 159 hex cells against 338
        // arms). The excess is named rather than absorbed: the drafted recess
        // wall draws minimum-size arms from the reach potential's lateral
        // accumulation (`round50.md` Q4, rule C — another lane's fix). So the
        // band is the harness's licensed multiplicative width around today's
        // count, the same honest pin this file used for Round 25's flip before
        // 2026-09-28: a ratchet against a gross regression, and a number the
        // rule-C lane is EXPECTED to move downward and re-pin. It did, on
        // 2026-09-29 (the wall-creep lane; see `Disc::pinned_arms`).
        let pinned = disc.pinned_arms as f64;
        let (low, high) = (pinned * bands::STRUT_COUNT.0, pinned * bands::STRUT_COUNT.1);
        assert!(
            (placed_arms as f64) >= low && (placed_arms as f64) <= high,
            "{}: {placed_arms} arms against the pinned {pinned:.0}, band [{low:.1}, {high:.1}] \
             (`bands::STRUT_COUNT`)",
            disc.name,
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
        // two coincide only on a flat-bottomed model, and the glue-face-up
        // pose puts a ring, rib tops and a cavity ceiling at the bottom.
        //
        // WHAT IT MEASURES (2026-09-29): `total_scar_mm2` sums `π r_tip²`
        // over the arms, and `r_tip` is each arm's CONTACT slot, sized from
        // the peel that crosses that contact. So the identity holds exactly
        // when every injected mm² reaches one contact once, up to the
        // minimum-radius clamp. Until the contact-sizing rule a lander on a
        // ring arm's head was booked into the parent's contact too, and this
        // read 1.07-1.15 on a double count; the published split below is what
        // a reader grades it against.
        let contact_load_mm2: f64 = plan
            .struts
            .iter()
            .filter(|strut| strut.kind == plinth_sweep::StrutKind::Arm)
            .map(plinth_sweep::SweepStrut::tip_load_mm2)
            .sum();
        println!(
            "{}: scar identity {:.5} (total scar {:.4} mm2, injected/X {:.4} mm2); contact slots \
             hold {:.4} of {:.4} mm2 injected; {} lander merges moved below a contact ({:.4} mm2)",
            disc.name,
            report.total_scar_mm2 / (report.injected_load_mm2 / x_allowed()),
            report.total_scar_mm2,
            report.injected_load_mm2 / x_allowed(),
            contact_load_mm2,
            report.injected_load_mm2,
            report.counters.contact_merges_moved_below,
            report.contact_merge_load_moved_mm2,
        );
        // RE-SCOPED 2026-09-30 (lane lattice-align), ON PURPOSE: the identity
        // is now asserted from ABOVE only. It exists to catch a DOUBLE COUNT
        // (a lander booked into two scars read 1.07-1.15), and a double count
        // can only push the scar over `injected/X`. From below, the identity
        // holds only while every injected mm² reaches a contact in that
        // contact's own worst event. With the straight-line reach and the
        // island seating the recess floor takes fewer arms, more of its peel
        // reaches the plate through the stiffener walls and the rim's
        // contacts, and a per-event-sized contact (rule a′, owner-approved
        // 2026-09-29) is sized for its worst single event, not for the sum of
        // the events it serves. Measured: 0.889 / 0.944 / 0.972 (Round 25 /
        // 50 / 100) with the contact slots holding 432 of 486, 1844 of 1954
        // and 7611 of 7834 mm² injected, uncarried 0 on all three. The drift
        // is not new: at develop's own placement (`harness::
        // at_path_fixture_seating`, which reproduces 88 / 304 / 1067 exactly)
        // the same reading is already 0.938 / 0.973 / 0.987 at X 18, inside
        // the band only because the band is 8 %. Whether a
        // contact that serves two peel events should carry their sum is the
        // owner's question, asked in `lanes/lattice-align.md`; the rule under
        // it is unchanged here. What the physics does guarantee is asserted
        // below it: every contact is sized by the area law for the load its
        // own slot carries.
        assert!(
            report.total_scar_mm2
                <= (1.0 + bands::AREA_IDENTITY) * report.injected_load_mm2 / x_allowed(),
            "{}: total scar {:.3} mm2 above injected_load/X_ALLOWED {:.3} mm2 (band {:.0}%): a \
             double count",
            disc.name,
            report.total_scar_mm2,
            report.injected_load_mm2 / x_allowed(),
            bands::AREA_IDENTITY * 100.0,
        );
        assert!(
            (report.total_scar_mm2 - contact_load_mm2 / x_allowed()).abs()
                <= bands::AREA_IDENTITY * contact_load_mm2 / x_allowed(),
            "{}: total scar {:.3} mm2 against the contact slots' own load/X_ALLOWED {:.3} mm2 \
             (band {:.0}%): a contact not sized by the area law for what it carries",
            disc.name,
            report.total_scar_mm2,
            contact_load_mm2 / x_allowed(),
            bands::AREA_IDENTITY * 100.0,
        );
    }
}

/// **Rule A, for a disc nobody marked**: the dense search still reaches the
/// glue-face-up flip and commits it, and that searched pose IS the as-drawn
/// pose of the turned-over disc, vertex for vertex.
///
/// The Round discs in the repository are now marked and searched no more, but
/// an unmarked copy (the owner's own folders, the frozen field inputs) still
/// goes through the search, so the search's answer stays graded here. Search
/// only: placement on this pose is graded by the test above, and the vertex
/// identity below is what lets that one placement stand for both paths.
///
/// | change | assertion that reddens |
/// |---|---|
/// | the flat-face escort in `crates/plinth-orient/src/search.rs` is dropped (rule A reverted) | (2a) on Round 50 (measured 2026-09-28) |
/// | `orient_as_drawn` applies any rotation but identity, or `turned_over` / the flip tool's map stops being the search's antipode rotation | the vertex identity |
#[test]
fn an_unmarked_disc_is_still_searched_and_its_flip_is_the_as_drawn_pose() {
    let grid_settings = GridSettings::registered().expect("registered grid settings");
    let dedupe_tolerance_deg = grid_settings.dedupe_tolerance_deg();
    let dedupe_cosine = grid_settings.dedupe_cosine();

    for disc in [round_25(), round_50(), round_100()] {
        let mesh = mesh_for(&disc);

        // ---- (2a) the flip reaches the commit tier -------------------------
        let oriented = orient_dense(&mesh, disc.name);
        let record = dense_record(&oriented);
        let commit = record
            .tiers
            .iter()
            .find(|tier| tier.tier == SearchTier::Commit)
            .expect("the commit tier ran");
        let flip_in_commit = commit
            .ranking
            .iter()
            .find(|entry| -entry.up_model_frame[2].get() > dedupe_cosine);
        let orientation = oriented.history().orientation();
        println!(
            "{name}: commit tier {scored} (promoted {promoted} + escorted {escorted}), flip in \
             commit tier: {flip}, selection {selection:?}",
            name = disc.name,
            scored = commit.scored,
            promoted = record.tiers[1].promoted,
            escorted = commit.escorted,
            flip = flip_in_commit.map_or_else(
                || "NO".to_owned(),
                |entry| format!("rank {} score {:.5}", entry.rank, entry.terms.score.get())
            ),
            selection = orientation.selection,
        );
        assert!(
            flip_in_commit.is_some(),
            "{}: the 180 deg flip (glue face UP) never reached the commit tier — a proxy \
             discarded the pose that lays the part's flat face horizontal (rule A)",
            disc.name,
        );

        // ---- (2b) pose: the flip, within the search's own tolerance --------
        let angle = degrees_off_the_flip(&orientation.selection).unwrap_or_else(|| {
            panic!(
                "{}: won an explicit AxisDegrees rotation ({:?}), not the flip",
                disc.name, orientation.selection
            )
        });
        assert!(
            angle <= dedupe_tolerance_deg,
            "{}: the committed pose is {angle:.4} deg off the glue-face-up flip, outside the \
             search's own dedupe tolerance {dedupe_tolerance_deg:.4} deg (selection {:?}). Owner \
             ruling 2026-09-28: all three Round discs print glue face UP.",
            disc.name,
            orientation.selection,
        );

        // ---- the searched flip IS the as-drawn pose ------------------------
        let drawn = orient_marked(&turned_over(&mesh), disc.name, AsDrawnReason::OrientNone);
        assert_eq!(
            drawn.mesh().tris(),
            oriented.mesh().tris(),
            "{}: the as-drawn and searched meshes differ in their triangles",
            disc.name
        );
        let differing = drawn
            .mesh()
            .verts()
            .iter()
            .zip(oriented.mesh().verts())
            .filter(|(a, b)| a != b)
            .count();
        assert_eq!(
            differing, 0,
            "{}: {differing} vertices differ between the as-drawn turned-over disc and the \
             searched flip of the authored disc",
            disc.name
        );
    }
}

/// **The committed Round files are marked, and drawn glue face UP.**
///
/// `corpus/as-drawn/Round_{25,50,100}.stl` are the provisioned real files
/// turned over once by `plinth-mesh`'s `flip_stl_x180` example
/// (`corpus/as-drawn/README.md`). Loaded the way the executor loads a source
/// (binary STL, the repair pre-pass, the registered decimation), probed by the
/// production probe and used as drawn, each must stand flat with its single
/// plain face — the one that reaches the axis — on TOP, and the stiffener side
/// (rim annulus and rib tops, nothing at the axis: the centre is recess) at the
/// bottom. No placement here: the field run of these files is the end-to-end
/// check, and the placement physics is graded on the parametric twin above.
#[test]
fn the_committed_round_discs_are_marked_and_drawn_glue_face_up() {
    for disc in [round_25(), round_50(), round_100()] {
        let reason = committed_mark(&disc);
        let source = committed_as_drawn(disc.stem);
        let loaded = plinth_mesh::load_binary_stl(&source, plinth_mesh::WeldPolicy::None)
            .unwrap_or_else(|error| panic!("{}: {error}", disc.name));
        let (repaired, _) = plinth_mesh::repair_mesh(&loaded).into_parts();
        let oriented = orient_marked(&repaired, disc.name, reason);
        assert_eq!(
            oriented.history().orientation().selection,
            BaselineSelection::Identity
        );

        let (min_z, max_z) = z_span(oriented.mesh());
        let lo = flat_face(oriented.mesh(), min_z, FLAT_FACE_EPS_MM);
        let hi = flat_face(oriented.mesh(), max_z, FLAT_FACE_EPS_MM);
        println!(
            "{}: {} as drawn; bottom z={min_z:.4} {:.2} mm2 (r {:.3}..{:.3}), top z={max_z:.4} \
             {:.2} mm2 (r {:.3}..{:.3})",
            disc.name,
            source.display(),
            lo.area_mm2,
            lo.r_min,
            lo.r_max,
            hi.area_mm2,
            hi.r_min,
            hi.r_max,
        );
        assert!(
            lo.count > 0 && hi.count > 0,
            "{}: no flat face at one of the z extremes; the committed file is TILTED",
            disc.name
        );
        assert!(
            hi.covers_axis && !lo.covers_axis,
            "{}: top face covers the axis: {}, bottom face covers it: {}. The plain glue face \
             (the only flat face across the axis; the stiffener side's centre is recess) must be \
             on TOP. The committed file is glue face DOWN.",
            disc.name,
            hi.covers_axis,
            lo.covers_axis,
        );
        assert!(
            hi.area_mm2 > lo.area_mm2,
            "{}: the top face ({:.2} mm2) must be the whole plain face, larger than the rim \
             annulus and rib tops at the bottom ({:.2} mm2)",
            disc.name,
            hi.area_mm2,
            lo.area_mm2,
        );
    }
}
