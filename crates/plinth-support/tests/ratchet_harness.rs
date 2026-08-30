//! **The quality-ratchet harness.**
//!
//! A fingerprint (a digest, a pinned member count) tells you the plan
//! changed. It says nothing about whether it got BETTER, so a deliberate
//! improvement and a silent regression look identical in that kind of check.
//! This harness answers the question a fingerprint cannot: six scalars, one
//! per corpus model, each with a physical meaning and a direction, recorded
//! in [`RECORDED_RATCHETS_PATH`] and compared against a fresh run every time
//! this test executes.
//!
//! **The three-kind doctrine this implements one third of.**
//! `docs/development.md`'s "Testing doctrine: three kinds of check" section
//! describes three kinds of check with three different lifetimes:
//!
//! 1. **Invariants** — permanent, placement-independent, never refreshed. Not
//!    this file's job.
//! 2. **Quality ratchets** — THIS file. A recorded number that may only
//!    improve; worsening it beyond tolerance is a test failure, and a
//!    deliberate, argued regression is recorded through [`UPDATE_ENV_VAR`],
//!    never by hand-editing the recorded JSON.
//! 3. **Equivalence checks** — a before/after comparison run at the time of a
//!    specific change. Not this file's job either.
//!
//! **Why these six and not a support/contact count.** Support count is the
//! textbook bad ratchet: it improves by deleting supports until the model
//! falls off the plate, and it is seeded from per-vertex local z minima, so
//! it moves under retessellation with no change in plan quality. Every metric
//! here is instead a physical quantity with a direction that means something
//! on the plate: a tip that snaps, resin that has to print and get cleaned
//! off, a scar the operator sands by hand, a strip that sags because nothing
//! is under it.
//!
//! ## The six metrics
//!
//! | # | metric | unit | source |
//! |---|---|---|---|
//! | 1 | worst tip area ratio | dimensionless | [`worst_tip_area_ratio`] |
//! | 2 | total model-facing scar area | mm² | [`total_scar_area_mm2`] |
//! | 3 | support resin volume | mL | [`support_resin_volume_ml`] |
//! | 4 | residual uncovered area at seal | mm² | [`residual_uncovered_area_mm2`] |
//! | 5 | worst uncovered gap | mm | [`worst_uncovered_gap_mm`] |
//! | 6 | ownerless model area | mm² | [`ownerless_model_area_mm2`] |
//! | 7 | abandoned routed load | mm² | [`abandoned_load_mm2`] |
//!
//! **Metric 7 arrived 2026-08-29 with the placement-pin retirement.**
//! `support_runner_integration.rs` asserted `abandoned_mm2 == 0.0` on the
//! toed slab as a bare literal, sitting after that test's own snapshot-digest
//! equality — so it stopped running whenever the digest moved, which is every
//! time placement changes. The quantity itself is worth keeping: a refused
//! contact is quarantined, nothing is grown to replace it, and the coverage
//! measurement's censor radius cannot see the hole one claw tip leaves, so
//! the area really is abandoned. That makes it a lower-is-better physical
//! number about plan quality, which is a ratchet, not a fingerprint. Recorded
//! per model here, it now tracks EVERY corpus model rather than one fixture,
//! and it can no longer be silenced by an unrelated re-pin lag.
//!
//! All seven are lower-is-better, computed entirely from evidence the public
//! `plinth_support` API already discloses (`EmittedContactDisclosure`,
//! `FinalLiveMemberEvidence`, `CoverageMeasurement`,
//! `OwnerlessIslandDisclosure`) — no new field was added to any disclosure
//! type to build this harness.
//!
//! ## Running it
//!
//! ```sh
//! cargo test -p plinth-support --test ratchet_harness --profile gate --offline
//! ```
//!
//! Compares a fresh run of every corpus model against
//! [`RECORDED_RATCHETS_PATH`] and fails, naming the model, the metric, the
//! old value, the new value and the direction, on any unrecorded worsening.
//!
//! ## Recording an improvement, or a deliberate regression
//!
//! ```sh
//! PLINTH_RATCHET_UPDATE=1 cargo test -p plinth-support --test ratchet_harness \
//!     --profile gate --offline -- --nocapture
//! ```
//!
//! Silently tightens every metric that improved. If any metric would get
//! WORSE, the update refuses and lists exactly which model/metric pairs would
//! regress and by how much — a silent worsening is never written. A
//! deliberate, argued regression (a physics-model change the owner ruled on,
//! per the adopted doctrine's table) is recorded by additionally setting
//! `PLINTH_RATCHET_ACCEPT_REGRESSION=1`; the commit that does so states the
//! argument, the same way a golden re-pin states `from -> to` and why.
//! Hand-editing the recorded JSON is never the fix — it bypasses the exact
//! measurement this file exists to force.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

use plinth_core::{Finite, LayerPlan, PositiveFinite};
use plinth_decimate::{run as run_decimation, DecimationBudget, ProbeConfig};
use plinth_hollow::{HollowedModel, SolidDecisionReason};
use plinth_mesh::{
    evaluate_delicacy, load_binary_stl, DelicacyBuildConfig, DelicacySettings, Mesh, WeldPolicy,
};
use plinth_orient::{run as run_orientation, OrientRequest, PinnedBaseline};
use plinth_support::forest::{RootwardEndRadius, TerminalEndRadius};
use plinth_support::member_sizing::{frustum_volume_ml, MemberSpan};
use plinth_support::{
    registered_coverage_request, run_support, AbandonedContactDisclosure, CoverageGap,
    CoverageMeasurement, EmittedContactDisclosure, OwnerlessIslandDisclosure,
    SupportPreparationLimits, SupportRequest,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// Env var: set to `1` to rewrite [`RECORDED_RATCHETS_PATH`] from a fresh
/// measurement instead of only comparing against it.
const UPDATE_ENV_VAR: &str = "PLINTH_RATCHET_UPDATE";
/// Env var: set to `1` alongside [`UPDATE_ENV_VAR`] to permit the rewrite to
/// record a metric that got WORSE. Absent, an update that would worsen any
/// metric refuses to write anything and lists what it would have worsened.
const ACCEPT_REGRESSION_ENV_VAR: &str = "PLINTH_RATCHET_ACCEPT_REGRESSION";

/// The recorded ratchet file. Read every run; rewritten only under
/// [`UPDATE_ENV_VAR`]. Kept beside this test, not under `target/`, because it
/// is a committed record, not a build output.
const RECORDED_RATCHETS_PATH: &str = "tests/fixtures/support_ratchets.json";

/// **Per-model measurement budget.** `crates/plinth-support/tests/support_runner_integration.rs`
/// already carries a documented trap: the coverage-repair controller runs an
/// unbounded search on at least one fixture and never prints a `test result`
/// line, so a caller that waits for one loses its whole run. Measured live
/// while building this harness (2026-08-29, this worktree): `Weeping Treant
/// v1.2` reaches `CoverageMeasurement` with 8,906 repair candidates
/// outstanding, and its FIRST repair trial alone took 161 s — a rate that
/// would need many hours to exhaust the candidate pool, not the minutes a
/// test suite can spend. Every model here therefore runs on its own thread
/// with this wall-clock budget; a model that exceeds it is reported as a
/// measurement failure naming the budget, never waited on. This is
/// deliberately NOT special-cased to `Weeping Treant` by name — the guard is
/// for whichever model regresses into this shape next.
const PER_MODEL_MEASUREMENT_BUDGET: Duration = Duration::from_secs(120);

/// **The exact eight-model corpus**, in the order named by
/// `docs/engineering-journal/06-conventions.md` §6. `Weeping Treant v1.2` is
/// the one model whose contact and member sets are large enough to matter for
/// the summation-error headroom this file's tolerance rule is built on (see
/// the module doc on [`tolerance_for`]) -- when it can be measured within
/// [`PER_MODEL_MEASUREMENT_BUDGET`] at all; see that constant's doc for the
/// measured state of that as of 2026-08-29.
const CORPUS_MODELS: [&str; 8] = [
    "Sphere",
    "Cylinder",
    "make_box",
    "make_overhang_table",
    "make_toed_slab",
    "make_raking_blade",
    "make_thin_fin",
    "Weeping Treant v1.2",
];

fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("workspace root")
}

/// One model's measured ratchet metrics, plus the corpus-file digest they are
/// keyed to. **A model's ratchets are only comparable to a prior recording
/// taken against the identical STL bytes** — contact candidates are seeded
/// from per-vertex local z minima, so a retessellated mesh yields a different
/// candidate set and a plan of equal quality reads as a different plan.
/// Comparing two different meshes compares nothing, which is why this digest
/// travels with every recorded row and gates every comparison.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
struct RatchetMetrics {
    worst_tip_area_ratio: f64,
    total_scar_area_mm2: f64,
    support_resin_volume_ml: f64,
    residual_uncovered_area_mm2: f64,
    worst_uncovered_gap_mm: f64,
    ownerless_model_area_mm2: f64,
    /// Metric 7. `#[serde(default)]` so a recorded file written before this
    /// metric existed still loads, reading 0.0 — the tightest possible
    /// baseline, so an unrecorded model cannot silently acquire abandoned
    /// load. Every row in the committed file carries a measured value.
    #[serde(default)]
    abandoned_load_mm2: f64,
}

impl RatchetMetrics {
    /// Every `(metric name, unit, lower-is-better value)` triple, in the
    /// table order from the module doc. One place this list is written, so a
    /// metric added here automatically joins recording, comparison and the
    /// failure report.
    fn fields(&self) -> [(&'static str, &'static str, f64); 7] {
        [
            (
                "worst_tip_area_ratio",
                "dimensionless",
                self.worst_tip_area_ratio,
            ),
            ("total_scar_area_mm2", "mm^2", self.total_scar_area_mm2),
            (
                "support_resin_volume_ml",
                "mL",
                self.support_resin_volume_ml,
            ),
            (
                "residual_uncovered_area_mm2",
                "mm^2",
                self.residual_uncovered_area_mm2,
            ),
            ("worst_uncovered_gap_mm", "mm", self.worst_uncovered_gap_mm),
            (
                "ownerless_model_area_mm2",
                "mm^2",
                self.ownerless_model_area_mm2,
            ),
            ("abandoned_load_mm2", "mm^2", self.abandoned_load_mm2),
        ]
    }

    fn get(&self, key: &str) -> f64 {
        self.fields()
            .into_iter()
            .find(|(name, _, _)| *name == key)
            .map(|(_, _, value)| value)
            .unwrap_or_else(|| panic!("unknown ratchet metric key {key}"))
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
struct RecordedModelRow {
    /// `sha256:<hex>` of the corpus STL this row's metrics were measured
    /// against. Compared byte-for-byte before any metric comparison runs.
    corpus_sha256: String,
    #[serde(flatten)]
    metrics: RatchetMetrics,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
struct RecordedRatchetFile {
    schema: String,
    /// Ordered so the file diffs one model at a time regardless of a
    /// `BTreeMap`'s own (already-sorted) iteration order — `BTreeMap`
    /// serializes its keys in sorted order via `serde_json`, so alphabetic
    /// model order is what a human sees in the diff, not a struct or
    /// insertion order.
    models: BTreeMap<String, RecordedModelRow>,
}

const SCHEMA: &str = "plinth/support-ratchet-metrics/v1";

fn load_recorded(path: &Path) -> RecordedRatchetFile {
    match fs::read_to_string(path) {
        Ok(text) => serde_json::from_str(&text)
            .unwrap_or_else(|error| panic!("{} is not valid JSON: {error}", path.display())),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => RecordedRatchetFile {
            schema: SCHEMA.to_owned(),
            models: BTreeMap::new(),
        },
        Err(error) => panic!("could not read {}: {error}", path.display()),
    }
}

fn write_recorded(path: &Path, file: &RecordedRatchetFile) {
    let mut text = serde_json::to_string_pretty(file).expect("serialize recorded ratchets");
    text.push('\n');
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).expect("create ratchet fixture directory");
    }
    fs::write(path, text).unwrap_or_else(|error| panic!("write {}: {error}", path.display()));
}

fn corpus_sha256(stl_path: &Path) -> String {
    let bytes = fs::read(stl_path)
        .unwrap_or_else(|error| panic!("read corpus STL {}: {error}", stl_path.display()));
    let digest = Sha256::digest(&bytes);
    format!("sha256:{digest:x}")
}

/// Mirrors `plinth_pipeline::prepare::prepared_delicacy_field` exactly (same
/// LOD name, same registered `DelicacySettings`, same scaled
/// `DelicacyBuildConfig`) — inlined because `plinth-support`'s dev-dependency
/// graph cannot depend on `plinth-pipeline` (the dependency runs the other
/// way). The same duplication already exists in `support_runner_integration.rs`
/// and the `embed_incidence_census` example; this is a third, independent
/// site computing the identical production field, not a fourth definition of
/// it.
fn real_delicacy(hollowed: &HollowedModel) -> Vec<f64> {
    let field = evaluate_delicacy(
        hollowed.mesh(),
        "prepared",
        DelicacySettings::production().expect("production delicacy settings"),
        DelicacyBuildConfig::production(hollowed.mesh().tris().len()),
    )
    .expect("delicacy field");
    field
        .face_values()
        .iter()
        .map(|value| value.weight())
        .collect()
}

/// What one model's run contributes toward its ratchet metrics: the sealed
/// authority's disclosures plus the coverage measurement taken against the
/// registered production request, both computed once and folded by the
/// metric functions below.
struct RunEvidence {
    emitted_contacts: Vec<EmittedContactDisclosure>,
    ownerless_islands: Vec<OwnerlessIslandDisclosure>,
    abandoned_contacts: Vec<AbandonedContactDisclosure>,
    member_volumes_ml: Vec<f64>,
    coverage: CoverageMeasurement,
}

/// Runs the exact public support-preparation path over one corpus model,
/// the same path `embed_incidence_census` and `support_runner_integration`
/// exercise, and gathers everything the metric functions need. A model that
/// cannot be prepared or measured returns `Err`, naming the stage that
/// refused, rather than a struct with invented zeros — the same
/// refuse-don't-degrade rule the engine itself is held to.
fn run_model(root: &Path, name: &str) -> Result<RunEvidence, String> {
    let stl = root.join("corpus/models").join(format!("{name}.stl"));
    let mesh = load_binary_stl(&stl, WeldPolicy::None).map_err(|error| format!("load: {error}"))?;
    let budget =
        DecimationBudget::ratified_default(ProbeConfig::new(1.0, 64 * 1024 * 1024, 10_000.0))
            .map_err(|error| format!("budget: {error}"))?;
    let decimated = run_decimation(&mesh, budget)
        .map_err(|error| format!("decimate: {error}"))?
        .into_decimated_model()
        .map_err(|scale| format!("decimate-scale-skip: {scale:?}"))?;
    let oriented = run_orientation(
        decimated,
        &OrientRequest {
            pinned: PinnedBaseline {
                pipeline_report: root
                    .join("corpus/baseline")
                    .join(format!("{name}.pipeline.json")),
            },
        },
    )
    .map_err(|error| format!("orient: {error}"))?;
    let (mesh, orientation): (Mesh, _) = oriented.into_parts();
    let hollowed =
        HollowedModel::solid_decision(mesh, orientation, SolidDecisionReason::BelowThreshold)
            .map_err(|error| format!("hollow: {error}"))?;

    let max_z = hollowed
        .mesh()
        .verts()
        .iter()
        .map(|vertex| vertex[2])
        .fold(f64::NEG_INFINITY, f64::max);
    let min_z = hollowed
        .mesh()
        .verts()
        .iter()
        .map(|vertex| vertex[2])
        .fold(f64::INFINITY, f64::min);
    if !max_z.is_finite() || !min_z.is_finite() {
        return Err("degenerate z extent".to_owned());
    }
    let pitch_mm = 0.05;
    let span_mm = (max_z - min_z) + 5.0 + 1.0;
    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "bounded corpus-fixture arithmetic, identical to the census harness's own \
                  layer-count derivation"
    )]
    let layer_count = ((span_mm / pitch_mm).ceil() as usize).max(1_000);
    let layers = LayerPlan::new(0.0, vec![pitch_mm; layer_count])
        .map_err(|error| format!("layers: {error}"))?;
    let delicacy = real_delicacy(&hollowed);

    let input = SupportPreparationLimits::production()
        .map_err(|error| format!("limits: {error}"))?
        .input(&layers, &delicacy, &[]);
    let authority = run_support(
        hollowed,
        SupportRequest {
            lift_mm: PositiveFinite::new(5.0).map_err(|_| "lift".to_owned())?,
        },
        input,
    )
    .map_err(|error| format!("support: {error}"))?;

    let emitted_contacts = authority.emitted_contacts().to_vec();
    let ownerless_islands = authority.ownerless_islands().to_vec();
    let abandoned_contacts = authority.abandoned_contacts().to_vec();

    let snapshot = authority
        .verification_snapshot()
        .map_err(|error| format!("snapshot: {error}"))?;

    let mut member_volumes_ml = Vec::with_capacity(snapshot.final_live_members().len());
    for member in snapshot.final_live_members() {
        let p0 = to_finite3(member.p0_mm()).map_err(|error| format!("member p0: {error}"))?;
        let p1 = to_finite3(member.p1_mm()).map_err(|error| format!("member p1: {error}"))?;
        let span = MemberSpan::new(p0, p1).map_err(|error| format!("member span: {error:?}"))?;
        let terminal_radius = PositiveFinite::new(member.terminal_radius_mm())
            .map_err(|error| format!("terminal radius: {error}"))?;
        let rootward_radius = PositiveFinite::new(member.rootward_radius_mm())
            .map_err(|error| format!("rootward radius: {error}"))?;
        let volume = frustum_volume_ml(
            span,
            TerminalEndRadius::new(terminal_radius),
            RootwardEndRadius::new(rootward_radius),
        )
        .map_err(|error| format!("frustum volume: {error:?}"))?;
        member_volumes_ml.push(volume.get().get());
    }

    let coverage_request =
        registered_coverage_request().map_err(|error| format!("coverage request: {error}"))?;
    let coverage = snapshot
        .measure_coverage(coverage_request)
        .map_err(|error| format!("measure_coverage: {error}"))?;

    Ok(RunEvidence {
        emitted_contacts,
        ownerless_islands,
        abandoned_contacts,
        member_volumes_ml,
        coverage,
    })
}

/// Runs [`run_model`] on its own thread and gives up after
/// [`PER_MODEL_MEASUREMENT_BUDGET`] rather than blocking the whole harness on
/// a search that may never finish. **The worker thread is not cancelled on
/// timeout** — Rust has no safe preemptive thread cancellation, and this
/// binary's own process lifetime (it exits once every `#[test]` in it has
/// returned) is what eventually reclaims it, the same way `gtimeout` reclaims
/// a killed subprocess elsewhere in this project's test conventions. A
/// timeout is reported exactly like any other measurement failure: named,
/// with the budget stated, never silently retried or silently waited past.
fn run_model_with_budget(
    root: &Path,
    name: &'static str,
    budget: Duration,
) -> Result<RunEvidence, String> {
    let root = root.to_path_buf();
    let (sender, receiver) = mpsc::channel();
    thread::Builder::new()
        .name(format!("ratchet-{name}"))
        .spawn(move || {
            // The receiver may already be gone (this function timed out and
            // returned) -- `send` failing in that case is expected, not an
            // error to report.
            let _ = sender.send(run_model(&root, name));
        })
        .unwrap_or_else(|error| panic!("spawn ratchet worker thread for '{name}': {error}"));
    match receiver.recv_timeout(budget) {
        Ok(result) => result,
        Err(mpsc::RecvTimeoutError::Timeout) => Err(format!(
            "exceeded the {budget:?} per-model measurement budget (still running when the \
             harness gave up -- see PER_MODEL_MEASUREMENT_BUDGET's doc comment)"
        )),
        Err(mpsc::RecvTimeoutError::Disconnected) => {
            Err("worker thread ended without reporting a result (panicked?)".to_owned())
        }
    }
}

fn to_finite3(point_mm: [f64; 3]) -> Result<[Finite; 3], plinth_core::FiniteError> {
    Ok([
        Finite::new(point_mm[0])?,
        Finite::new(point_mm[1])?,
        Finite::new(point_mm[2])?,
    ])
}

/// Metric 1 — **max**, not mean: the failure mode is the narrowest section,
/// so an average would hide the one tip that snaps. `None` fields (a
/// transcript row whose contact could not be resolved in the accepted plan)
/// are skipped rather than treated as zero, for the same reason the
/// disclosure itself refuses to invent a zero: an unresolved row is a
/// different fact from a resolved row that carries nothing.
fn worst_tip_area_ratio(contacts: &[EmittedContactDisclosure]) -> f64 {
    contacts
        .iter()
        .filter_map(
            |contact| match (contact.carried_area_mm2, contact.tip_radius_mm) {
                (Some(area_mm2), Some(radius_mm)) => {
                    Some(area_mm2 / (std::f64::consts::PI * radius_mm * radius_mm))
                }
                _ => None,
            },
        )
        .fold(0.0_f64, f64::max)
}

/// Metric 2 — **sum** of every emitted tip's cross-sectional footprint, the
/// mark each one leaves on the model surface.
fn total_scar_area_mm2(contacts: &[EmittedContactDisclosure]) -> f64 {
    contacts
        .iter()
        .filter_map(|contact| contact.tip_radius_mm)
        .map(|radius_mm| std::f64::consts::PI * radius_mm * radius_mm)
        .sum()
}

/// Metric 3 — **sum** of every final live member's realized frustum volume.
fn support_resin_volume_ml(member_volumes_ml: &[f64]) -> f64 {
    member_volumes_ml.iter().sum()
}

/// Metric 4 — **sum** of every patch's uncovered area, uncensored, at the
/// sealed run's own registered coverage measurement. Blocking patches are
/// already zero at seal by invariant; what remains is the sub-censor dust the
/// seal is allowed to leave behind.
fn residual_uncovered_area_mm2(coverage: &CoverageMeasurement) -> f64 {
    coverage
        .evidence()
        .patches()
        .iter()
        .map(|patch| patch.uncovered_area_mm2().get())
        .sum()
}

/// Metric 5 — **max** finite gap over every patch. A patch reporting
/// [`CoverageGap::NoRoutedContact`] is a hard failure, not a large number —
/// folding it into the maximum would let an unroutable patch pose as merely a
/// bad ratchet reading instead of the invariant violation it actually is, so
/// this panics instead of returning a value for it. That panic is
/// deliberately NOT the same failure shape as a ratchet regression: the
/// caller must be able to tell "the plan is unsound" from "the plan is worse
/// than last time".
fn worst_uncovered_gap_mm(model: &str, coverage: &CoverageMeasurement) -> f64 {
    let mut worst = 0.0_f64;
    for patch in coverage.evidence().patches() {
        match patch.worst_gap() {
            CoverageGap::Finite(gap_mm) => worst = worst.max(gap_mm.get()),
            CoverageGap::NoRoutedContact => panic!(
                "INVARIANT VIOLATION, not a ratchet reading: model '{model}' has an uncovered \
                 patch (island {}, component {}) with NO ROUTED CONTACT anywhere near it. This \
                 is a hard failure of the closed-load-path invariant, not a large gap value —\
                 fix the support plan, not this harness.",
                patch.island_id(),
                patch.component_id(),
            ),
        }
    }
    worst
}

/// Metric 6 — **sum** of every ownerless island's face area.
fn ownerless_model_area_mm2(disclosures: &[OwnerlessIslandDisclosure]) -> f64 {
    disclosures
        .iter()
        .map(OwnerlessIslandDisclosure::ownerless_face_area_mm2)
        .sum()
}

/// Metric 7 — **sum** of the routed load every refused contact abandons.
///
/// **Sum, not max, and not a count.** The count of refusals is the textbook
/// bad ratchet this file's module doc warns about: it improves by refusing
/// one contact that carried 400 mm² instead of two that carried nothing. The
/// AREA is the physical quantity — load the plan gave up on and told no
/// neighbour about — and summing it says how much of the model is being held
/// by nothing at all.
///
/// Zero is the good answer and every corpus model that seals reaches it
/// today. A model whose refusals start costing real area regresses this
/// metric and has to argue for it, which is exactly what the bare
/// `abandoned_mm2 == 0.0` literal in `support_runner_integration.rs` was
/// trying to say before it was stranded behind a digest equality.
fn abandoned_load_mm2(disclosures: &[AbandonedContactDisclosure]) -> f64 {
    disclosures
        .iter()
        .map(AbandonedContactDisclosure::abandoned_area_mm2)
        .sum()
}

/// `-0.0` is numerically identical to `0.0` for every comparison this file
/// makes, but it is an ugly, confusing thing to see in a file a human reads
/// to decide whether a change is a regression. A sum over zero or
/// exactly-cancelling terms can legitimately land on negative zero; this
/// normalizes it away at the point every metric is finalized, once, rather
/// than asking every fold and every reader to know the rule.
fn normalize_zero(value: f64) -> f64 {
    if value == 0.0 {
        0.0
    } else {
        value
    }
}

fn metrics_from_evidence(model: &str, evidence: &RunEvidence) -> RatchetMetrics {
    RatchetMetrics {
        worst_tip_area_ratio: normalize_zero(worst_tip_area_ratio(&evidence.emitted_contacts)),
        total_scar_area_mm2: normalize_zero(total_scar_area_mm2(&evidence.emitted_contacts)),
        support_resin_volume_ml: normalize_zero(support_resin_volume_ml(
            &evidence.member_volumes_ml,
        )),
        residual_uncovered_area_mm2: normalize_zero(residual_uncovered_area_mm2(
            &evidence.coverage,
        )),
        worst_uncovered_gap_mm: normalize_zero(worst_uncovered_gap_mm(model, &evidence.coverage)),
        ownerless_model_area_mm2: normalize_zero(ownerless_model_area_mm2(
            &evidence.ownerless_islands,
        )),
        abandoned_load_mm2: normalize_zero(abandoned_load_mm2(&evidence.abandoned_contacts)),
    }
}

/// **The tolerance rule**, stated once. Every metric here is a floating-point
/// physical quantity — none of the six registered metrics is an integer
/// count (a support/contact count was rejected by design precisely because
/// counts make bad ratchets), so only the floating-point branch of the
/// adopted rule is implemented: a metric that gets worse by more than
/// `max(1e-12 * |recorded|, 1e-9)` **in the metric's own unit** fails.
///
/// **Where the two constants come from, not asserted, derived:**
/// - `1e-12` relative: naive summation of `n` terms in a fixed order carries
///   worst-case relative error about `n * eps` with `eps = f64::EPSILON =
///   2.220446e-16`. The largest fold in this harness is over the Weeping
///   Treant's contact and member sets, order 10^3 items; `n <= 4096` is
///   generous headroom, giving `4096 * 2.220446e-16 = 9.09e-13`, which rounds
///   up to `1e-12`. Metrics 1 and 5 are maxima rather than sums, so this same
///   bound is generous for them by roughly three orders of magnitude.
/// - `1e-9` absolute: a relative tolerance around zero is meaningless, and
///   metrics 4 and 6 are legitimately zero on a good plan. `1e-9 mm²` is six
///   orders below the printer's own resolvable area (one 34.4 µm pixel is
///   about `1.2e-3 mm²`), so a movement that small cannot correspond to
///   anything that happens on the plate. The same `1e-9` is reused for mL and
///   mm and for the dimensionless ratio, for the identical reason: nothing
///   this small is a fact about the plate.
///
/// Summation order must stay fixed for this bound to hold, which the run
/// already guarantees: `docs/development.md`'s thread-sweep determinism check
/// proves byte-identical output at 1, 4 and 16 threads and must keep running
/// independently of this harness.
fn tolerance_for(recorded: f64) -> f64 {
    (recorded.abs() * 1e-12).max(1e-9)
}

/// One model/metric pair whose fresh measurement is worse than the recorded
/// value by more than [`tolerance_for`] allows.
struct RatchetViolation {
    model: String,
    metric: &'static str,
    unit: &'static str,
    old: f64,
    new: f64,
}

impl std::fmt::Display for RatchetViolation {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let worse_by = self.new - self.old;
        let tolerance = tolerance_for(self.old);
        write!(
            formatter,
            "model '{model}', metric '{metric}' (lower-is-better, {unit}): {old:.12} -> \
             {new:.12}, WORSE by {worse_by:.12} (tolerance {tolerance:.3e})",
            model = self.model,
            metric = self.metric,
            unit = self.unit,
            old = self.old,
            new = self.new,
        )
    }
}

/// All six lower-is-better metrics are worsened by getting LARGER, so
/// `new - old > tolerance` is worse for every metric this harness records.
/// If a higher-is-better metric is ever added, this is the one place the
/// direction must be threaded through instead of assumed.
fn is_worse_beyond_tolerance(old: f64, new: f64) -> bool {
    (new - old) > tolerance_for(old)
}

fn is_improvement(old: f64, new: f64) -> bool {
    new < old
}

#[test]
fn support_ratchets_do_not_regress() {
    let root = workspace_root();
    let recorded_path = root
        .join("crates/plinth-support")
        .join(RECORDED_RATCHETS_PATH);
    let recorded = load_recorded(&recorded_path);

    let update_requested = std::env::var(UPDATE_ENV_VAR).ok().as_deref() == Some("1");
    let accept_regression = std::env::var(ACCEPT_REGRESSION_ENV_VAR).ok().as_deref() == Some("1");

    let mut measured_rows: BTreeMap<String, RecordedModelRow> = BTreeMap::new();
    // A model that has NEVER been measurable (no recorded baseline exists)
    // gets a loud warning but does not fail the gate -- there is no ratchet
    // to violate for a model that has never had one, and failing the whole
    // suite over it would block every OTHER model's ratchets indefinitely on
    // a limitation this harness did not create. A model that USED TO seal
    // (it has a recorded baseline) and now does not is a different claim
    // entirely -- reachability itself regressed -- and that is a hard
    // failure.
    let mut never_reachable: Vec<String> = Vec::new();
    let mut reachability_regressions: Vec<String> = Vec::new();

    for &name in &CORPUS_MODELS {
        let stl_path = root.join("corpus/models").join(format!("{name}.stl"));
        let sha256 = corpus_sha256(&stl_path);
        match run_model_with_budget(&root, name, PER_MODEL_MEASUREMENT_BUDGET) {
            Ok(evidence) => {
                let metrics = metrics_from_evidence(name, &evidence);
                let field_values = metrics.fields();
                println!("RATCHET MEASURED\t{name}\t{sha256}\t{field_values:?}");
                measured_rows.insert(
                    name.to_owned(),
                    RecordedModelRow {
                        corpus_sha256: sha256,
                        metrics,
                    },
                );
            }
            Err(reason) => {
                let message = format!("model '{name}' did not seal: {reason}");
                if recorded.models.contains_key(name) {
                    reachability_regressions.push(message);
                } else {
                    println!("RATCHET SKIPPED (no baseline exists to regress)\t{message}");
                    never_reachable.push(name.to_owned());
                }
            }
        }
    }

    assert!(
        reachability_regressions.is_empty(),
        "REACHABILITY REGRESSION, not an ordinary ratchet worsening -- these corpus model(s) \
         have a recorded ratchet baseline (so they sealed before) and now do not seal at all, \
         within budget or otherwise:\n{}",
        reachability_regressions.join("\n")
    );

    if update_requested {
        update_recorded_file(&recorded_path, &recorded, &measured_rows, accept_regression);
        return;
    }

    let mut violations: Vec<RatchetViolation> = Vec::new();
    let mut rebaseline_needed: Vec<String> = Vec::new();
    let mut missing_baseline: Vec<String> = Vec::new();

    for (name, measured) in &measured_rows {
        let Some(previous) = recorded.models.get(name) else {
            missing_baseline.push(name.clone());
            continue;
        };
        if previous.corpus_sha256 != measured.corpus_sha256 {
            rebaseline_needed.push(format!(
                "model '{name}': corpus STL changed ({} -> {}). Its recorded metrics are not \
                 comparable to this mesh -- contact candidates are seeded from per-vertex local \
                 z minima, so a retessellated mesh yields a different candidate set and a plan \
                 of equal quality reads as a different plan. Re-baseline it with \
                 `{UPDATE_ENV_VAR}=1`.",
                previous.corpus_sha256, measured.corpus_sha256,
            ));
            continue;
        }
        for (metric, unit, new_value) in measured.metrics.fields() {
            let old_value = previous.metrics.get(metric);
            if is_worse_beyond_tolerance(old_value, new_value) {
                violations.push(RatchetViolation {
                    model: name.clone(),
                    metric,
                    unit,
                    old: old_value,
                    new: new_value,
                });
            }
        }
    }

    assert!(
        missing_baseline.is_empty(),
        "NO RECORDED BASELINE for corpus model(s) {missing_baseline:?} in {}. Record one with \
         `{UPDATE_ENV_VAR}=1 cargo test -p plinth-support --test ratchet_harness --profile gate \
         --offline` -- never by hand-editing the JSON.",
        recorded_path.display(),
    );

    assert!(
        rebaseline_needed.is_empty(),
        "CORPUS RE-BASELINE NEEDED:\n{}",
        rebaseline_needed.join("\n")
    );

    assert!(
        violations.is_empty(),
        "RATCHET REGRESSION -- {count} metric(s) got worse than the recorded baseline in {path}:\n{list}\n\n\
         If every one of these is a deliberate, argued change (per the adopted doctrine's table \
         -- a physics-model change the owner ruled on), record it with \
         `{UPDATE_ENV_VAR}=1 {ACCEPT_REGRESSION_ENV_VAR}=1 cargo test -p plinth-support --test \
         ratchet_harness --profile gate --offline -- --nocapture`, and state the argument in the \
         commit that does so. Never hand-edit the JSON.",
        count = violations.len(),
        path = recorded_path.display(),
        list = violations
            .iter()
            .map(RatchetViolation::to_string)
            .collect::<Vec<_>>()
            .join("\n"),
    );

    if !never_reachable.is_empty() {
        println!(
            "RATCHET GATE PASSED with {} model(s) skipped (no baseline exists, so nothing could \
             regress): {}. This is a disclosed limitation, not a silent pass -- see \
             PER_MODEL_MEASUREMENT_BUDGET's doc comment for why.",
            never_reachable.len(),
            never_reachable.join(", "),
        );
    }
}

/// Rewrites the recorded file from a fresh measurement. Every metric that
/// improved (or a brand-new model) is written unconditionally -- an
/// unratcheted improvement would leave the check looser than the code
/// deserves. Any metric that would get WORSE is refused unless
/// `PLINTH_RATCHET_ACCEPT_REGRESSION=1` is also set, and refusal reports
/// every model/metric pair that blocked the write without touching the file
/// at all, so a partial, silently-mixed rewrite can never happen.
fn update_recorded_file(
    path: &Path,
    recorded: &RecordedRatchetFile,
    measured_rows: &BTreeMap<String, RecordedModelRow>,
    accept_regression: bool,
) {
    let mut blocking: Vec<RatchetViolation> = Vec::new();
    for (name, measured) in measured_rows {
        let Some(previous) = recorded.models.get(name) else {
            continue;
        };
        if previous.corpus_sha256 != measured.corpus_sha256 {
            continue;
        }
        for (metric, unit, new_value) in measured.metrics.fields() {
            let old_value = previous.metrics.get(metric);
            if is_worse_beyond_tolerance(old_value, new_value) {
                blocking.push(RatchetViolation {
                    model: name.clone(),
                    metric,
                    unit,
                    old: old_value,
                    new: new_value,
                });
            }
        }
    }

    if !blocking.is_empty() && !accept_regression {
        panic!(
            "REFUSING TO UPDATE {path} -- {count} metric(s) would get WORSE than recorded, and \
             `{ACCEPT_REGRESSION_ENV_VAR}=1` was not set:\n{list}\n\nIf this is a deliberate, \
             argued regression, re-run with `{UPDATE_ENV_VAR}=1 {ACCEPT_REGRESSION_ENV_VAR}=1` \
             and state the argument in the commit. Nothing was written.",
            path = path.display(),
            count = blocking.len(),
            list = blocking
                .iter()
                .map(RatchetViolation::to_string)
                .collect::<Vec<_>>()
                .join("\n"),
        );
    }

    let mut updated = recorded.clone();
    updated.schema = SCHEMA.to_owned();
    let mut changed_models: Vec<String> = Vec::new();
    for (name, measured) in measured_rows {
        let previously_recorded = updated.models.get(name);
        if previously_recorded != Some(measured) {
            changed_models.push(name.clone());
        }
        updated.models.insert(name.clone(), measured.clone());
    }
    write_recorded(path, &updated);
    if changed_models.is_empty() {
        println!("RATCHET UPDATE: no change -- every recorded value already matches this run.");
    } else {
        println!(
            "RATCHET UPDATE: wrote {} for model(s): {}{}",
            path.display(),
            changed_models.join(", "),
            if blocking.is_empty() {
                String::new()
            } else {
                format!(
                    " (INCLUDING {} ACCEPTED REGRESSION(S) -- state the argument in the commit)",
                    blocking.len()
                )
            }
        );
    }
}

/// **Proof the tolerance rule and the improvement rule agree with each
/// other**: an improvement is never reported as a violation, and a worsening
/// just inside the tolerance band is not (yet) a violation while one just
/// outside it is. This deliberately tests just INSIDE and just OUTSIDE the
/// boundary rather than exactly ON it (`old + tolerance`): `new - old` is not
/// guaranteed to round-trip to bit-exact `tolerance` for an arbitrary `old`,
/// so an exact-boundary assertion would be testing `f64` rounding, not this
/// rule.
#[test]
fn tolerance_boundary_is_exact() {
    let old = 10.0_f64;
    let tolerance = tolerance_for(old);
    assert!(!is_worse_beyond_tolerance(old, old)); // unchanged
    assert!(!is_worse_beyond_tolerance(old, old - 5.0)); // improved
    assert!(!is_worse_beyond_tolerance(old, old + tolerance * 0.5)); // inside the band
    assert!(is_worse_beyond_tolerance(old, old + tolerance * 2.0)); // past the band
    assert!(is_improvement(old, old - 1e-6));
    assert!(!is_improvement(old, old));
    assert!(!is_improvement(old, old + 1e-6));

    // The absolute floor applies at zero, where a relative tolerance would be
    // meaningless.
    assert!(!is_worse_beyond_tolerance(0.0, 1e-10));
    assert!(is_worse_beyond_tolerance(0.0, 1e-8));
}
