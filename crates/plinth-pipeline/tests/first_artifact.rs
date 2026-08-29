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

#[path = "../examples/shared/first_artifact_chain.rs"]
mod chain;

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
fn a_coverage_refusal_removes_a_stale_artifact_already_at_the_target() {
    let scratch = Scratch::new("raking-blade-stale");
    std::fs::write(scratch.path(), b"an artifact from an earlier run")
        .expect("plant a stale artifact");

    let mesh = plinth_mesh::make_raking_blade().expect("procedural raking blade");
    let refusal = chain::run_procedural_model("make_raking_blade", mesh, scratch.path())
        .expect_err("make_raking_blade's residual islands have no closing repair-pool candidate");
    // FU-56: the coverage verdict is spoken by the closure controller, one
    // stage before the pre-write gate. The claim under test — a stale artifact
    // does not survive a refusal at this target — is about the path, not about
    // which stage spoke.
    assert!(refusal.is_coverage_closure_refusal(), "{refusal}");
    assert!(
        !scratch.path().exists(),
        "a coverage refusal left the previous run artifact at {}",
        scratch.path().display()
    );
}

/// The same refusal on a model that needs no provisioned corpus:
/// `make_raking_blade`, whose islands 3 and 5 have `NoRoutedContact` on every
/// one of the 8 closure attempts.
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
fn procedural_raking_blade_is_refused_before_any_file_exists() {
    let scratch = Scratch::new("raking-blade");
    let mesh = plinth_mesh::make_raking_blade().expect("procedural raking blade");

    let refusal = chain::run_procedural_model("make_raking_blade", mesh, scratch.path())
        .expect_err(
            "make_raking_blade's residual islands have no closing repair-pool \
             candidate at the registered radius",
        );
    assert!(
        refusal.is_coverage_closure_refusal(),
        "make_raking_blade must be refused by coverage closure: {refusal}"
    );
    assert!(
        !refusal.is_gates(),
        "the sealed run refuses before the pre-write gate is ever reached: {refusal}"
    );
    assert!(
        !refusal.is_support_refusal(),
        "a coverage-closure refusal must not be answered with a substitute geometry: {refusal}"
    );
    assert!(
        !scratch.path().exists(),
        "a coverage-closure-refused model wrote {}",
        scratch.path().display()
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
