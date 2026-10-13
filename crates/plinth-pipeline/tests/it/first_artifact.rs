//! Export design phase E2: the first end-to-end composition.
//!
//! Until this file existed, no code path anywhere in the workspace ran
//! `run_support` -> `FinalSupportAuthority::into_supported_model` ->
//! `plinth_emit::write` on a real model. Every stage was reachable and every
//! stage was tested; the arrows between the last three had never been joined,
//! so a `.chitubox` from the native engine's own sealed plan had never been
//! produced. This test joins them and then judges the file it wrote by
//! re-reading it, which is `chitubox_io`'s standing discipline: a write is not
//! done until a fresh parse of the bytes on disk agrees.
//!
//! Deliberately absent: any new frozen digest. Support-gate work runs
//! concurrently with the export track and legitimately moves member counts;
//! digest authority stays with `plinth-support`'s own integration goldens
//! (design record OQ-3). Everything asserted here is a completion, structural
//! or self-consistency property that a changed member count cannot break.
//!
//! The composition itself is shared verbatim with
//! `examples/first_artifact.rs`, which writes the owner's GUI-acceptance
//! artifact; the test and the artifact therefore cannot drift apart.

use crate::chain;

use std::path::PathBuf;

use chitubox_io::SegmentType;
use plinth_hollow::{HollowDisposition, SolidDecisionReason};
use plinth_mesh::{make_overhang_table, make_toed_slab};

/// A per-test scratch path that is removed on the way in and on the way out.
struct Scratch(PathBuf);

impl Scratch {
    fn new(label: &str) -> Self {
        let path = std::env::temp_dir().join(format!(
            "plinth-first-artifact-{label}-{}.chitubox",
            std::process::id()
        ));
        let _ = std::fs::remove_file(&path);
        Self(path)
    }

    fn path(&self) -> &std::path::Path {
        &self.0
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

/// Assert every property of the written artifact that does not depend on how
/// many members the support gate happened to keep.
fn assert_artifact_is_sound(outcome: &chain::ChainOutcome, audit: &chain::DiskAudit) {
    let model = &outcome.model;

    // The composition reached the end: a sealed run produced primitives, and
    // every primitive became one ChituBox record.
    assert!(
        outcome.primitive_count > 0,
        "{model}: the sealed run lowered zero primitives, so nothing was emitted"
    );
    assert_eq!(
        outcome.emit.support_record_count, outcome.primitive_count,
        "{model}: emit dropped or invented support records"
    );
    assert_eq!(
        audit.support_record_count, outcome.primitive_count,
        "{model}: the reopened file disagrees with the emitted record count"
    );

    // The reopened file is the one this crate claims to write.
    assert_eq!(audit.model_name, chain::EMITTED_MODEL_NAME);
    assert_eq!(
        audit.model_triangle_count, outcome.prepared_faces,
        "{model}: the reopened model mesh lost triangles"
    );
    assert_eq!(
        audit.file_bytes, outcome.emit.artifact_bytes,
        "{model}: the emit report's byte count disagrees with the file on disk"
    );

    // Winding, re-derived from the binary32 vertices on disk rather than
    // trusted from the producer. `plinth-emit` winds every synthesised mesh
    // outward by signed volume; a closed outward-wound soup has strictly
    // positive signed volume, and that is translation invariant, so the
    // bbox-centring emit applies cannot flatter the result.
    assert_eq!(
        audit.outward_wound_records,
        audit.support_record_count,
        "{model}: {} of {} support meshes on disk are inward-wound (min signed volume {} mm^3)",
        audit.support_record_count - audit.outward_wound_records,
        audit.support_record_count,
        audit.min_record_signed_volume_mm3,
    );
    assert!(
        audit.min_record_signed_volume_mm3 > 0.0,
        "{model}: a support mesh on disk encloses no volume"
    );
    assert!(
        audit.model_signed_volume_mm3 > 0.0,
        "{model}: the model mesh on disk is inward-wound"
    );

    // Every record carries one of the six frozen segment types.
    for segment_type in audit.records_by_segment_type.keys() {
        assert!(
            [
                SegmentType::TIP.0,
                SegmentType::TAPER.0,
                SegmentType::SHAFT.0,
                SegmentType::JOINER.0,
                SegmentType::FLARE.0,
                SegmentType::PAD_OR_BRIDGE.0,
            ]
            .contains(segment_type),
            "{model}: unknown wire segment type {segment_type} reached disk"
        );
    }

    // E1's native preview fallback is present. ChituBox refused a from-scratch
    // project without it (SPEC-CHITUBOX-RS R1), so an artifact intended for the
    // GUI gate that lacks it is not worth handing over.
    assert_eq!(
        audit.preview_bytes, 26_996,
        "{model}: preview span is {} bytes, not the native fallback's 26,996",
        audit.preview_bytes
    );
    assert!(
        audit.preview_is_native_fallback,
        "{model}: the embedded preview is not the pinned native asset"
    );

    // The independent format gate: fresh read, parse, reserialize, byte-equal.
    assert!(
        audit.format_gate_passed,
        "{model}: the plinth-verify format gate refused the artifact"
    );
}

/// The refusal removes a *stale* artifact at the target too.
///
/// "A refused model leaves no path on disk" is a claim about the path, not
/// about one call. A model that emitted on an earlier run and is refused on
/// this one would otherwise leave the earlier run's file sitting at the target,
/// where the next thing to read it — the owner, or `run_one` in
/// `examples/first_artifact.rs`, which unlike `Scratch` does not unlink first —
/// would take it for this run's answer.
///
/// **Re-derived 2026-09-01 (owner ruling): the claim survives, the mechanism
/// that used to deliver it does not.** A coverage shortfall no longer refuses,
/// so this fixture EMITS, and the old assertion — "the target does not exist" —
/// was being satisfied by the refusal rather than by any stale-artifact
/// discipline. Asserting absence after an emission would be simply false, and
/// flipping it to "the target exists" would assert nothing at all: a surviving
/// stale file also exists.
///
/// So the claim is now made positively — the bytes AT the target are this run's
/// artifact — which is a strictly stronger statement than the one it replaces.
/// **Two independent mechanisms deliver it, and neither was the refusal.**
/// `prepare_one` unlinks the target on the way IN, before any stage can fail,
/// so every disposition it can produce clears the path; and `plinth_emit::write`
/// builds a staging sibling and `rename`s it onto the target, which replaces
/// whatever was there. This test exercises the second, because the chain helper
/// enters at `run_from_oriented` and never reaches the first.
///
/// **Re-pinned off the corpus Sphere onto `make_raking_blade` (2026-08-26).**
/// The claim is unchanged and so is every assertion; what changed is which
/// model still produces a coverage refusal to make it with. `3a6526f`
/// converged the clearance mask and seated the model anchor, the Sphere's
/// islands became coverable, and it now seals and emits — so a test that
/// demanded a refusal from it was asserting something that had stopped being
/// true. `make_raking_blade` is this file's standing refusal fixture (see
/// `procedural_raking_blade_is_refused_before_any_file_exists` below), it is
/// procedural, and it needs no provisioned `corpus/models/`, which is task
/// #16's known hole closed here as a side effect.
#[test]
fn a_stale_artifact_does_not_survive_a_model_that_emits_with_a_coverage_gap() {
    const STALE: &[u8] = b"an artifact from an earlier run";

    let scratch = Scratch::new("raking-blade-stale");
    std::fs::write(scratch.path(), STALE).expect("plant a stale artifact");

    let mesh = plinth_mesh::make_raking_blade().expect("procedural raking blade");
    let outcome = chain::run_procedural_model("make_raking_blade", mesh, scratch.path())
        .expect("a model with an unclosable island now emits and names the gap");

    // The disposition really is the one this test is about. Without this the
    // assertions below would also pass on a model that simply covered.
    assert!(
        outcome.coverage_warning.is_some(),
        "this fixture must reach the emit-and-warn path, or it is not testing it"
    );

    // **The claim, stated positively.** The old test asserted the target was
    // ABSENT, which a refusal gave for free. There is no refusal now, so the
    // claim has to be made the harder way: what stands at the target is THIS
    // run's artifact and not the previous run's file.
    let on_disk = std::fs::read(scratch.path()).expect("this run's artifact must be at the target");
    assert_ne!(
        on_disk.as_slice(),
        STALE,
        "the earlier run's file survived at {}",
        scratch.path().display()
    );
    assert_eq!(
        on_disk.len() as u64,
        outcome.emit.artifact_bytes,
        "the bytes at the target must be the ones this run reports writing"
    );
    // And they are a real artifact, re-read from disk by the independent
    // auditor rather than trusted because they are not the sentinel.
    let audit = chain::audit_disk_artifact(scratch.path(), "make_raking_blade")
        .expect("the emitted artifact must re-open and pass the format gate");
    assert!(audit.format_gate_passed, "{audit:?}");
}

/// The same island on a model that needs no provisioned corpus:
/// `make_raking_blade`, whose islands 3 and 5 have `NoRoutedContact` on every
/// one of the closure attempts.
///
/// **Re-derived 2026-09-01 (owner ruling).** This was
/// `procedural_raking_blade_is_refused_before_any_file_exists` and it asserted
/// there was no file. There is a file now, and the island is still exactly as
/// unclosable as it was: every measurement assertion is kept and re-made
/// against the artifact's own coverage warning instead of against the error.
/// The one claim that could not be carried over verbatim is "it never reaches
/// the pre-write gate" — it now does reach it, and the gate warns rather than
/// refusing, which is the ruling.
///
/// **This is now the only coverage-refusal fixture in this file.** The corpus
/// Sphere was the other one until 2026-08-26: `corpus_sphere_drilled_plan_is_
/// refused_before_any_file_exists` asserted that the Sphere's drilled plan
/// does not cover its islands, and at `3a6526f` that stopped being true — the
/// model-seated anchor family can route, the Sphere seals at attempt 0 and
/// emits a complete artifact. That test was DELETED rather than re-pinned,
/// because every claim it made is made here on a fixture that still refuses:
/// the refusal is a coverage-closure refusal, it is not answered with a
/// substitute geometry, and it leaves nothing on disk. This test additionally
/// pins that it never reaches the pre-write gate, so nothing was lost.
///
/// **Re-pinned off `make_overhang_table` (2026-08-12).** This test used to run
/// the overhang table, the model `plinth-verify`'s frozen coverage golden is
/// built from. With nomination scored against the measurement's uncovered
/// samples that model now CLOSES from its own repair pool and emits
/// (`plinth-support/tests/support_runner_integration.rs`,
/// `public_runner_closes_the_uncovered_rim_table_only_after_appending_repair_contacts`),
/// so it is no longer a refusal fixture. `make_raking_blade` is: the CCO
/// transcript never admits any repair-pool candidate for either of its two
/// residual islands, at the registered radius, from this fixture's pool. That
/// is measured, not forced — nothing here touches the radius, the floor or
/// the tolerance.
///
/// `refused_geometry()` does not apply: that accessor is `None` for every
/// non-gate stop, by design (`ChainStop::refused_geometry`), and a
/// coverage-closure stop is deliberately excluded from `is_support_refusal`
/// too — re-planning the oriented solid after it would substitute a different
/// artifact for a refusal that is about the plan, not the geometry.
#[test]
fn procedural_raking_blade_emits_with_its_unclosable_island_named() {
    let scratch = Scratch::new("raking-blade");
    let mesh = plinth_mesh::make_raking_blade().expect("procedural raking blade");

    let outcome = chain::run_procedural_model("make_raking_blade", mesh, scratch.path()).expect(
        "an island with no closing repair-pool candidate is a warning, not a refusal",
    );
    assert!(
        scratch.path().exists(),
        "the ruling is that the file gets written; nothing is at {}",
        scratch.path().display()
    );

    // **The measurement, unweakened.** The island still has no closing
    // candidate at the registered radius, it is still measured, and it is still
    // genuinely uncovered rather than censored dust. Nothing here touches the
    // radius, the censor or any tolerance — the only thing that moved is what
    // happens to the file.
    let warning = outcome
        .coverage_warning
        .as_ref()
        .expect("the unclosable island must be named on the artifact that carries it");
    let censor = plinth_support::registered_coverage_censor_area_mm2()
        .expect("registered A3 coverage censor");
    let worst = warning
        .islands
        .iter()
        .map(|island| island.uncovered_area_mm2)
        .fold(0.0_f64, f64::max);
    assert!(
        worst > censor,
        "the named island must be genuinely uncovered, not censored dust: {worst} mm² against a \
         {censor} mm² censor"
    );
    assert!(
        warning
            .islands
            .iter()
            .any(|island| island.face_count() > 0),
        "a named island with no faces names nothing: {warning:?}"
    );
    // The stage that gave up, carried onto the artifact. This is the half that
    // points at code rather than at geometry, and it is the half a refusal used
    // to deliver through the error type.
    assert!(
        warning
            .closure_reason
            .as_deref()
            .is_some_and(|reason| reason.contains("NoImprovingRepair")),
        "the warning must name the controller's own stop reason: {warning:?}"
    );

    // **The geometry was not substituted.** A coverage gap is about the PLAN,
    // not the geometry, so the run must not answer it by re-planning the
    // oriented solid — the exact substitution the old refusal's
    // `!is_support_refusal` assertion protected against. The drilled shell this
    // fixture seals is what must be in the file.
    assert!(
        !matches!(
            outcome.supported_geometry,
            plinth_pipeline::SupportedGeometry::DrilledShellRefusedBySupport { .. }
        ),
        "a coverage gap must not be answered with a substitute geometry: {:?}",
        outcome.supported_geometry
    );
}

/// **FU-56's outcome, on the fixture that used to prove the opposite.**
///
/// `make_overhang_table` is the model `plinth-verify`'s frozen coverage golden
/// is built from, and until 2026-08-12 it was this suite's refusal fixture:
/// its island 0 was reported as having no closing repair-pool candidate. It
/// did have one — nomination just could not see it, because candidates were
/// scored against every sample on the blocking patch's faces rather than
/// against the samples still uncovered, and face 51 is a large, mostly
/// covered face. Scored against the uncovered rows, the closing candidate
/// ranks first, the island shuts, and the model emits. Nothing was loosened
/// to get here: same radius, same floor, same zero tolerance.
///
/// **Since `3a6526f` this model does not repair at all: it seals at attempt 0
/// with 0 trials and 0 commits.** The paragraph above is kept as the history
/// of the fixture, not as a description of this run. The claim this test still
/// holds is the one in its name — the table's coverage closes and the model
/// reaches a written file — and it holds it whether the closing happened on
/// the baseline layout or through repair. The committed-repair path moved to
/// `make_uncovered_rim_table`; see `a4_production_door.rs`.
#[test]
fn procedural_overhang_table_closes_its_coverage_and_emits() {
    let scratch = Scratch::new("overhang-table");
    let mesh = make_overhang_table().expect("procedural overhang table");

    let outcome = chain::run_procedural_model("make_overhang_table", mesh, scratch.path())
        .expect("make_overhang_table's coverage closes and the model emits");
    assert!(
        scratch.path().exists(),
        "the closed model must reach a written file at {}",
        scratch.path().display()
    );
    chain::audit_disk_artifact(scratch.path(), "make_overhang_table")
        .expect("reopen and audit the artifact");
    let _ = outcome;
}

/// The accepted path, unchanged by the gate: a plan whose coverage the verifier
/// agrees with still lowers, still writes, and still survives the disk audit.
///
/// This is the assertion that the gate is a *gate* and not a wall.
#[test]
#[expect(
    clippy::float_cmp,
    reason = "test-local arithmetic and exact pins over hand-written, bounded fixture values. See \
          docs/evidence/remediation-2026-08-26/census/cast-audit.md"
)]
fn procedural_toed_slab_passes_the_gates_and_still_emits() {
    let scratch = Scratch::new("toed-slab");
    let mesh = make_toed_slab().expect("procedural toed slab");

    let outcome = chain::run_procedural_model("make_toed_slab", mesh, scratch.path())
        .expect("make_toed_slab's plan covers its islands and must reach a written file");
    let audit = chain::audit_disk_artifact(scratch.path(), "make_toed_slab")
        .expect("reopen and audit the artifact");

    assert_artifact_is_sound(&outcome, &audit);
    assert_eq!(outcome.lift_mm, chain::LIFT_MM);

    // *What* was emitted, not merely that something was. An accepted artifact
    // whose provenance quietly moved from the hollow stage's own verdict to a
    // fallback is a different artifact for the same file name, and nothing
    // else in this suite would notice.
    assert_eq!(
        outcome.supported_geometry,
        chain::SupportedGeometry::HollowStageDecidedSolid,
        "make_toed_slab's artifact is made of something else now"
    );

    // Most of the twelve gates in `docs/engineering-journal/04-migration-plan.md`
    // §"The defect class this milestone exists to make impossible" have no ported
    // implementation yet. That is carried out to the caller rather than
    // described in a comment, and pinned here so it cannot silently grow.
    //
    // The pin is by NAME rather than by count. A count says nothing about which
    // gate moved, so porting one and losing another would read as no change at
    // all; naming them makes either direction a failure that says what happened.
    assert_eq!(
        outcome.pending_gates,
        vec![
            plinth_pipeline::PreWriteGate::Clearance,
            plinth_pipeline::PreWriteGate::UnsupportedMinima,
            plinth_pipeline::PreWriteGate::IslandBirths,
            plinth_pipeline::PreWriteGate::RegularContactInvariants,
            plinth_pipeline::PreWriteGate::UnroutedSpecializedContacts,
            plinth_pipeline::PreWriteGate::Connectivity,
            plinth_pipeline::PreWriteGate::Printability,
            plinth_pipeline::PreWriteGate::Murray,
            plinth_pipeline::PreWriteGate::BallBreach,
            plinth_pipeline::PreWriteGate::PhysicalContactInvariant,
        ],
        "the set of gates that reach a verdict moved"
    );
    assert!(
        !outcome
            .pending_gates
            .contains(&plinth_pipeline::PreWriteGate::UncoveredIslands),
        "the coverage gate is ported and must not be listed as pending"
    );
    assert!(
        !outcome
            .pending_gates
            .contains(&plinth_pipeline::PreWriteGate::Load),
        "the area-law gate is ported and must not be listed as pending"
    );
}

/// A pre-hollowed solid: two nested closed shells, both wound outward, which is
/// what an already-hollowed STL looks like to this engine.
fn make_prehollowed_box() -> plinth_mesh::Mesh {
    let outer = plinth_mesh::make_box([30.0, 30.0, 30.0], [0.0, 0.0, 0.0]).expect("outer shell");
    let inner = plinth_mesh::make_box([12.0, 12.0, 12.0], [0.0, 0.0, 15.0]).expect("inner shell");
    let offset = u32::try_from(outer.verts().len()).expect("fixture vertex count fits u32");
    let mut vertices = outer.verts().to_vec();
    vertices.extend_from_slice(inner.verts());
    let mut triangles = outer.tris().to_vec();
    triangles.extend(inner.tris().iter().map(|triangle| {
        triangle.map(|index| {
            index
                .checked_add(offset)
                .expect("fixture index cannot overflow")
        })
    }));
    plinth_mesh::Mesh::new(vertices, triangles).expect("pre-hollowed fixture is structurally valid")
}

/// The hollow-error fallback, gated on a model that actually takes it.
///
/// `plinth_hollow::run` converts only `IneligibleNoCavity`,
/// `IneligibleCavityBelowDrainThroat` (FU-47), `IneligibleBelowBulk`,
/// `IneligibleSavings`, the well-composedness repair's own
/// `CavityRepairRemovedCavity` / `IneligibleSavingsAfterCavityRepair`, and
/// (FU-55) `RealizedWallBelowSpec` into a typed `HollowStageOutput::Solid`;
/// **every other** `HollowError` propagates, and
/// before this fallback existed that error ended the chain, so such a model
/// reached no artifact at all. This pins the path that carries it to one now,
/// because a new branch in the shared composition with no gate on it is exactly
/// what drifts.
///
/// **Why a procedural pre-hollowed box, and not `make_box`.** This test was
/// written against `make_box`, which used to error with `InvalidShellTopology`
/// (FU-34). It no longer does: the 2026-08-10 cavity-field exterior margin
/// fixed the defect that made every bounding-box-filling solid refuse its own
/// shell, and `make_box` now reaches a *typed* `IneligibleSavings` — its true
/// 166 mm³ cavity is not worth cutting — which is `HollowStageDecidedSolid`,
/// not this branch. The `Cylinder` likewise hollows now. The last corpus model
/// on this branch was the Weeping Treant, and as of 2026-08-11 it is not on it
/// either: the well-composedness repair closed FU-49 and the treant hollows.
/// **No corpus model reaches this branch today**, which is precisely why the
/// gate below cannot be a corpus model.
///
/// A pre-hollowed input is the right permanent gate for it: `TriageError::
/// PreHollowed` is a **designed, stable** refusal — the engine declines to
/// hollow what is already hollow — so unlike `InvalidShellTopology` it will not
/// evaporate when a geometry defect is fixed, and the branch keeps its gate.
#[test]
#[expect(
    clippy::float_cmp,
    reason = "test-local arithmetic and exact pins over hand-written, bounded fixture values. See \
          docs/evidence/remediation-2026-08-26/census/cast-audit.md"
)]
fn hollow_stage_refusal_falls_back_to_the_oriented_solid_and_still_emits() {
    let scratch = Scratch::new("prehollowed-hollow-refusal");

    let outcome = chain::run_procedural_model("make_box", make_prehollowed_box(), scratch.path())
        .expect("a pre-hollowed solid must reach a written file through the hollow-error fallback");
    let audit = chain::audit_disk_artifact(scratch.path(), "make_box")
        .expect("reopen and audit the artifact");

    // Pinned as an equality on the whole variant, refusal string included, so
    // that the day `plinth-hollow` stops refusing this input — or starts
    // refusing it for a different reason — this fails and says so rather than
    // quietly changing the artifact's provenance.
    assert_eq!(
        outcome.supported_geometry,
        chain::SupportedGeometry::HollowStageRefused {
            refusal: "pre-hollowed input: PreHollowed { cavity_components: \
                      [ComponentDescription { component_id: 1, face_count: 12, \
                      dims_mm: [12.0, 12.0, 12.0] }] }"
                .to_owned()
        },
        "the pre-hollowed fixture's hollow-stage verdict moved; re-derive what \
         this artifact is made of"
    );

    // FU-34-mirrors-#33 (task #34): this used to pin `BelowThreshold` -- "the
    // savings gate measured this cavity and found it under the registered
    // minimum" -- next to a `SupportedGeometry` line that had just said the
    // hollow stage itself errored and never reached a threshold test. Moved
    // to `HollowStageRefused`, the reason that says what actually happened:
    // no shell was ever built, so there was nothing to measure a saving
    // against.
    assert_eq!(
        outcome.hollow_disposition,
        HollowDisposition::Solid {
            reason: SolidDecisionReason::HollowStageRefused
        },
        "the fallback's solid handoff changed shape; the caveat documented on \
         SupportedGeometry::HollowStageRefused needs re-deriving with it"
    );

    // EMIT AND WARN (2026-10-13): a model the run set out to hollow and
    // emitted solid carries a not-hollowed record — the reason and the resin
    // it costs — out to the report the CLI warns from. The triage refused
    // before any shell was planned, so the cost is the whole solid's volume.
    // The solid's volume is the fixture's own: the 30 mm box plus the 12 mm
    // box, both wound outward by `make_box` and so both counted positive
    // (30^3 + 12^3 = 28728 mm3) — measured here from the mesh the file holds.
    //
    // SABOTAGE (recorded by hand, 2026-10-13): with the HollowStageRefused
    // branch's `warn_not_hollowed` wrapper removed in `prepare.rs`, this
    // assertion fails with `None`.
    let warning = outcome
        .not_hollowed
        .as_ref()
        .expect("a model emitted solid after a refused hollow must carry the warning");
    assert!(
        warning.reason.contains("pre-hollowed input"),
        "the warning states the refusal: {}",
        warning.reason
    );
    assert_eq!(warning.planned_hollow_mm3, None, "no shell was planned");
    assert_eq!(
        warning.resin_cost_mm3, warning.solid_mm3,
        "with no planned hollow, the cost stated is the whole solid's volume"
    );
    assert!(
        (warning.solid_mm3 - 28_728.0).abs() < 1.0e-6,
        "the solid's volume, from its mesh: {}",
        warning.solid_mm3
    );

    // The oriented solid reached support planning intact -- the pre-clone is a
    // reserve, not a decimation step.
    assert_eq!(
        outcome.prepared_faces, outcome.source_faces,
        "the hollow-error reserve is the oriented solid and must not lose faces"
    );

    assert_artifact_is_sound(&outcome, &audit);
    assert_eq!(outcome.lift_mm, chain::LIFT_MM);
}

/// FU-55, downstream half: a shell discarded for measuring under the
/// requested wall must still reach a written artifact, through exactly the
/// call `run_from_oriented` makes for every `HollowStageOutput::Solid` --
/// this is not a new route, it is the standing solid path carrying a new
/// reason.
///
/// **Why this is built by hand rather than driven through a wall the real
/// engine cannot honour.** FU-53's own exact-distance selection makes a
/// genuinely under-walled shell close to unreachable through the full
/// pipeline on purpose (that is the fix FU-55's check is the safety net
/// under); `HollowRequest` has no seam for the internal margin a synthetic
/// reproduction would need to defeat. `plinth-hollow`'s own test suite
/// exercises the check firing on real geometry (a locally-built,
/// zero-margin `HollowSettings` on the committed Sphere) and the exact
/// `run` conversion this test's `HollowedModel` is the *output* of; this
/// test's job is only the arrow neither of those covers: does the pipeline
/// still write a file for it.
#[test]
fn shell_under_walled_solid_decision_still_reaches_a_written_artifact() {
    let scratch = Scratch::new("shell-under-walled");
    let mesh =
        plinth_mesh::make_box([10.0, 10.0, 10.0], [0.0, 0.0, 0.0]).expect("closed fixture box");
    let (_, oriented) = chain::prepare_from_mesh(mesh.clone(), "make_box")
        .expect("fixture box orients against the committed make_box baseline");
    let (_, orientation) = oriented.into_parts();
    let solid = plinth_hollow::HollowedModel::solid_decision(
        mesh,
        orientation,
        SolidDecisionReason::ShellUnderWalled,
    )
    .expect("closed fixture box is a valid oriented-solid reserve");

    let outcome = plinth_pipeline::prepare::seal_and_emit(
        &plinth_core::ModelId::new("fu55-shell-under-walled"),
        12,
        solid,
        chain::SupportedGeometry::HollowStageDecidedSolid,
        &chain::stage_policy(),
        scratch.path(),
        None,
    )
    .expect("a ShellUnderWalled solid decision must reach a written artifact, not an error")
    .expect("no stop_after was given, so the chain always reaches an outcome");

    let audit = chain::audit_disk_artifact(scratch.path(), "fu55-shell-under-walled")
        .expect("reopen and audit the artifact");

    assert_eq!(
        outcome.hollow_disposition,
        HollowDisposition::Solid {
            reason: SolidDecisionReason::ShellUnderWalled
        },
        "the disposition must name FU-55's own reason, not a different solid path"
    );
    assert_eq!(
        outcome.prepared_faces, outcome.source_faces,
        "the under-walled fallback is the oriented solid and must not lose faces"
    );
    assert_artifact_is_sound(&outcome, &audit);
}
