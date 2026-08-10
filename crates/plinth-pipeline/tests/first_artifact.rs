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

/// The geometry `make_overhang_table` reaches the pre-write gate on.
///
/// Pinned rather than left implicit: the gate refusal below is only the
/// refusal this test is about if it was spoken over the geometry the hollow
/// stage actually decided on, and not over a fallback the chain substituted.
const EXPECTED_OVERHANG_TABLE_GEOMETRY: chain::SupportedGeometry =
    chain::SupportedGeometry::HollowStageDecidedSolid;

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

/// The corpus STLs are provisioned inputs, not source, so they are gitignored
/// and fetched by the M0 corpus step. Follow the precedent already set by
/// `plinth-decimate`'s `corpus_budgets` gate: name the missing path in the
/// failure rather than skipping silently.
fn require_corpus_stl(name: &str) -> PathBuf {
    let path = chain::corpus_stl(name);
    assert!(
        path.is_file(),
        "RequiredCorpusModelMissing: required corpus model {} is missing; \
         provision corpus/models per the M0 corpus step",
        path.display()
    );
    path
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

/// **The corpus Sphere no longer reaches disk, and that is the fix.**
///
/// The Sphere hollows, drains, drills and seals — every one of those arrows
/// still works, and this test still drives all of them. What changed is the
/// last arrow: the pre-write coverage gate now judges the sealed plan, and the
/// drilled shell's plan leaves uncovered islands. Before the gate was wired,
/// that plan was written to a `.chitubox` and handed to the owner anyway,
/// because *nothing* consulted a physics verdict before the write.
///
/// The refusal is terminal by design: `ChainRefusal::Gates` does not fall back
/// to the oriented-solid reserve. The reserve answers "this geometry could not
/// be sealed"; substituting a different geometry for a model the engine just
/// declared under-supported would hand the owner a plausible artifact for a
/// question it failed.
///
/// The uncovered islands themselves are a support-planning finding, not this
/// seam's to fix. What this seam owes is that they stop the file.
#[test]
fn corpus_sphere_drilled_plan_is_refused_before_any_file_exists() {
    require_corpus_stl("Sphere");
    let scratch = Scratch::new("sphere");

    let refusal = chain::run_corpus_model("Sphere", scratch.path())
        .expect_err("the Sphere's drilled plan does not cover its islands");
    assert!(
        refusal.is_gates(),
        "Sphere must be refused by the gates, not by an earlier stage: {refusal}"
    );
    // The gate as a value, not a rendered substring: a renamed variant must be
    // a compile error, not an assertion that silently becomes permanently
    // false.
    assert_eq!(
        refusal.gate(),
        Some(plinth_pipeline::PreWriteGate::UncoveredIslands),
        "the refusal must name the gate that spoke: {refusal}"
    );
    // **What was refused**, which `is_gates()` alone cannot say. This is the
    // old OQ-1 pin restored to the place it now belongs: the drilled shell
    // *sealed*, and the gate refused that sealed plan. Without it this test
    // passes just as happily on "support refused the drilled shell, the chain
    // fell back to the oriented solid, and the gate refused that instead" --
    // the exact substitution the terminal-Gates design exists to prevent.
    assert_eq!(
        refusal.refused_geometry(),
        Some(&chain::SupportedGeometry::DrilledShellSealed),
        "the Sphere's drilled shell must reach the gate sealed: {refusal}"
    );

    // The whole point: refused *before* the write, so there is nothing on disk
    // to hand anyone — not a truncated file, not an empty one.
    assert!(
        !scratch.path().exists(),
        "a gate-refused model wrote {}",
        scratch.path().display()
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
#[test]
fn a_gate_refusal_removes_a_stale_artifact_already_at_the_target() {
    require_corpus_stl("Sphere");
    let scratch = Scratch::new("sphere-stale");
    std::fs::write(scratch.path(), b"an artifact from an earlier run")
        .expect("plant a stale artifact");

    let refusal = chain::run_corpus_model("Sphere", scratch.path())
        .expect_err("the Sphere's drilled plan does not cover its islands");
    assert!(refusal.is_gates(), "{refusal}");
    assert!(
        !scratch.path().exists(),
        "a gate refusal left the previous run's artifact at {}",
        scratch.path().display()
    );
}

/// The same refusal on a model that needs no provisioned corpus, and the one
/// `plinth-verify`'s own frozen coverage golden is built from: nine uncovered
/// islands measured on `make_overhang_table`.
#[test]
fn procedural_overhang_table_is_refused_before_any_file_exists() {
    let scratch = Scratch::new("overhang-table");
    let mesh = make_overhang_table().expect("procedural overhang table");

    let refusal = chain::run_procedural_model("make_overhang_table", mesh, scratch.path())
        .expect_err("make_overhang_table's plan leaves uncovered islands");
    assert!(
        refusal.is_gates(),
        "make_overhang_table must be refused by the gates: {refusal}"
    );
    assert_eq!(
        refusal.gate(),
        Some(plinth_pipeline::PreWriteGate::UncoveredIslands),
        "the refusal must name the gate that spoke: {refusal}"
    );
    assert_eq!(
        refusal.refused_geometry(),
        Some(&EXPECTED_OVERHANG_TABLE_GEOMETRY),
        "make_overhang_table reached the gate on a different geometry: {refusal}"
    );
    assert!(
        !scratch.path().exists(),
        "a gate-refused model wrote {}",
        scratch.path().display()
    );
}

/// The accepted path, unchanged by the gate: a plan whose coverage the verifier
/// agrees with still lowers, still writes, and still survives the disk audit.
///
/// This is the assertion that the gate is a *gate* and not a wall.
#[test]
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

    // Eleven of the twelve gates in `00-migration-guide.md` have no ported
    // implementation yet. That is carried out to the caller rather than
    // described in a comment, and pinned here so it cannot silently grow.
    assert_eq!(
        outcome.pending_gates.len(),
        11,
        "the ported-gate count moved: {:?}",
        outcome.pending_gates
    );
    assert!(
        !outcome
            .pending_gates
            .contains(&plinth_pipeline::PreWriteGate::UncoveredIslands),
        "the coverage gate is ported and must not be listed as pending"
    );
}

/// The hollow-error fallback, gated on a model that actually takes it.
///
/// `make_box` errors out of `plinth_hollow::run` with `InvalidShellTopology`
/// (FU-34) — it neither hollows nor decides solid — and before the fallback
/// existed that error ended the chain, so `make_box` reached no artifact at
/// all. This pins the path that carries it to one now, because a new branch in
/// the shared composition with no gate on it is exactly what drifts.
///
/// **Why `make_box` and not `Cylinder`, which refuses identically.** `Cylinder`
/// cannot be gated here at all today: on the sealed run it trips FU-8f phase
/// G6c's `debug_assert` in `specialized.rs` — a retained round executes 23,203
/// load operations against a charge of 4,146 — so any debug-profile test that
/// carries it into `run_support` panics before reaching an artifact. **That is
/// not this fallback's doing and it is not new:** `embed_incidence_census`,
/// which shares none of this code and takes the same solid handoff, panics
/// identically on `Cylinder` at this tip. It is filed as **FU-35**, and this
/// test deliberately does not paper over it by pinning a model that hides it.
///
/// `make_box` and not the Weeping Treant, in turn, because the two take the
/// identical path and this one costs milliseconds where the treant costs
/// minutes, which is not a test's budget.
#[test]
fn hollow_stage_refusal_falls_back_to_the_oriented_solid_and_still_emits() {
    let _ = require_corpus_stl("make_box");
    let scratch = Scratch::new("make-box-hollow-refusal");

    let outcome = chain::run_corpus_model("make_box", scratch.path())
        .expect("make_box must reach a written file through the hollow-error fallback");
    let audit = chain::audit_disk_artifact(scratch.path(), "make_box")
        .expect("reopen and audit the artifact");

    // Pinned as an equality on the whole variant, refusal string included, so
    // that the day `plinth-hollow` stops refusing this shell — or starts
    // refusing it for a different reason — this fails and says so rather than
    // quietly changing the artifact's provenance.
    assert_eq!(
        outcome.supported_geometry,
        chain::SupportedGeometry::HollowStageRefused {
            refusal: "native shell did not contain one negative cavity enclosed by \
                      positive outer material"
                .to_owned()
        },
        "make_box's hollow-stage verdict moved; re-derive what this artifact is made of"
    );

    // The provenance caveat this fallback knowingly inherits, asserted rather
    // than described: the model carries a `BelowThreshold` solid stamp that no
    // stage actually reached, because `SolidDecisionReason` cannot express
    // "the hollow stage errored". The `SupportedGeometry` above is the only
    // place the truth is recorded, which is why it is pinned first.
    assert_eq!(
        outcome.hollow_disposition,
        HollowDisposition::Solid {
            reason: SolidDecisionReason::BelowThreshold
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
