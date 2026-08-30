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
//! **Why these seven and not a support/contact count.** Support count is the
//! textbook bad ratchet: it improves by deleting supports until the model
//! falls off the plate, and it is seeded from per-vertex local z minima, so
//! it moves under retessellation with no change in plan quality. Every metric
//! here is instead a physical quantity with a direction that means something
//! on the plate: a tip that snaps, resin that has to print and get cleaned
//! off, a scar the operator sands by hand, a strip that sags because nothing
//! is under it.
//!
//! ## The seven metrics
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
/// measurement failure naming the budget, and THIS call stops waiting on it.
///
/// Say precisely what that does and does not promise, because an earlier
/// version of this sentence said "never waited on" and that was false. The
/// breaching model's thread keeps running — Rust has no safe preemption — and
/// the NEXT model's [`run_with_budget`] call joins it, blocking until it
/// finishes, before starting its own measurement. That is deliberate: an
/// overrun stealing CPU from the next model's window would show up as a false
/// budget breach in an innocent model. What is guaranteed is that no two
/// measurement windows overlap; what is NOT guaranteed is a bounded run, if a
/// breaching model is ever followed by another one.
///
/// This is deliberately NOT special-cased to `Weeping Treant` by name — the
/// guard is for whichever model regresses into this shape next.
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

/// Runs `work` on its own thread and gives up after `budget` rather than
/// blocking the caller on a search that may never finish.
///
/// **Not preemptive cancellation.** Rust has no safe way to stop a running
/// thread, so a `work` closure that outlives its budget keeps its CPU and
/// memory live in the background after this function returns its timeout
/// `Err`. What this function DOES guarantee, and what it did not guarantee
/// before this fix: `previous_worker`, if it is still holding the
/// [`thread::JoinHandle`] of an earlier call's straggler, is joined --
/// unconditionally blocked on until that straggler actually finishes --
/// before this call's own worker thread is spawned. That is what stops two
/// workers from ever running at once: not by killing the slow one, but by
/// never starting the next one until the slow one is provably done. This
/// call's own handle is stashed back into `previous_worker` on a timeout (for
/// the NEXT call to join) or joined immediately once it reports (there is
/// nothing left to wait for).
///
/// **Residual risk this does not remove.** A `work` closure that never
/// returns at all blocks every later call sharing the same `previous_worker`
/// slot forever, at its own join point. That trades an unattributable
/// resource-contention flake (the old defect: two heavy searches overlapping,
/// possibly starving a THIRD, unrelated model into a false budget breach) for
/// a loud, attributable hang on the very next call -- the best containment
/// available without unsafe preemption, not a guarantee that the harness
/// always finishes in bounded time.
fn run_with_budget<T, F>(
    name: &str,
    budget: Duration,
    previous_worker: &mut Option<thread::JoinHandle<()>>,
    work: F,
) -> Result<T, String>
where
    F: FnOnce() -> T + Send + 'static,
    T: Send + 'static,
{
    if let Some(handle) = previous_worker.take() {
        // Block until the prior timeout's worker is actually finished before
        // this call's clock -- or its CPU and memory footprint -- starts.
        let _ = handle.join();
    }

    let (sender, receiver) = mpsc::channel();
    let handle = thread::Builder::new()
        .name(name.to_owned())
        .spawn(move || {
            // The receiver may already be gone (this call timed out and
            // returned) -- `send` failing in that case is expected, not an
            // error to report.
            let _ = sender.send(work());
        })
        .unwrap_or_else(|error| panic!("spawn worker thread '{name}': {error}"));
    match receiver.recv_timeout(budget) {
        Ok(result) => {
            // The thread already sent its result, so it is finished or about
            // to be -- reclaim it now rather than carrying it forward.
            let _ = handle.join();
            Ok(result)
        }
        Err(mpsc::RecvTimeoutError::Timeout) => {
            *previous_worker = Some(handle);
            Err(format!(
                "exceeded the {budget:?} budget (still running when the caller gave up). Its \
                 thread is retained and will be joined -- blocked on until it actually finishes \
                 -- before the next worker sharing this budget slot starts, so it cannot overlap \
                 that worker's measurement window."
            ))
        }
        Err(mpsc::RecvTimeoutError::Disconnected) => {
            let _ = handle.join();
            Err(format!(
                "worker thread '{name}' ended without reporting a result (panicked?)"
            ))
        }
    }
}

/// Runs [`run_model`] under [`run_with_budget`], naming
/// [`PER_MODEL_MEASUREMENT_BUDGET`] in the timeout message the way the prior
/// version of this function did.
fn run_model_with_budget(
    root: &Path,
    name: &'static str,
    budget: Duration,
    previous_worker: &mut Option<thread::JoinHandle<()>>,
) -> Result<RunEvidence, String> {
    let root = root.to_path_buf();
    run_with_budget(
        &format!("ratchet-{name}"),
        budget,
        previous_worker,
        move || run_model(&root, name),
    )
    .unwrap_or_else(|timeout_or_panic_message| {
        Err(format!(
            "{timeout_or_panic_message} (see PER_MODEL_MEASUREMENT_BUDGET's doc comment)"
        ))
    })
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
    // Carries a timed-out model's worker handle forward so the NEXT model's
    // call joins it -- blocks until it is actually finished -- before
    // spawning its own worker. See `run_with_budget`'s doc comment for why
    // this is what actually prevents two models' searches from overlapping.
    let mut previous_worker: Option<thread::JoinHandle<()>> = None;

    for &name in &CORPUS_MODELS {
        let stl_path = root.join("corpus/models").join(format!("{name}.stl"));
        let sha256 = corpus_sha256(&stl_path);
        match run_model_with_budget(
            &root,
            name,
            PER_MODEL_MEASUREMENT_BUDGET,
            &mut previous_worker,
        ) {
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
    // DELIBERATELY NOT JOINED, and the reason is the whole point of the join.
    // `previous_worker` may still hold the straggler left behind by the LAST
    // model. Joining it here protects nothing: the join exists so that one
    // model's overrun cannot steal CPU from the NEXT model's measurement
    // window, and after the last model there is no next one.
    //
    // Joining anyway is what turned the mandated `cargo test --workspace`
    // gate into a non-terminating run. `Weeping Treant v1.2` breaches
    // `PER_MODEL_MEASUREMENT_BUDGET`, then opens an 8,906-candidate repair
    // search whose FIRST trial alone measured 161 s -- so this join waited on
    // work that needs hours, at the tail of a suite that had already passed
    // everything it was going to pass. The straggler is left to be reaped
    // when the test process exits, which is what the budget promised: a model
    // over budget is "reported as a measurement failure naming the budget,
    // never waited on".
    //
    // RESIDUAL, stated because it is real and load-bearing: termination here
    // holds only because the budget-breaching model is LAST in
    // `CORPUS_MODELS`. The between-models join inside `run_with_budget` is
    // unbounded by construction -- std has no join-with-timeout -- so placing
    // any model AFTER one that breaches its budget moves the same hang to
    // that model's join point. If that happens the fix is a bounded wait that
    // discloses the overlap and proceeds, NOT a longer budget: overlapping
    // measurement is a flake, and a hung gate is a stopped project.
    drop(previous_worker.take());

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

/// One metric's write-time resolution against its previously recorded value.
/// See [`update_recorded_file`]'s doc comment for the three cases this
/// implements. `accept_regression` only ever matters for case 3 (a genuine,
/// beyond-tolerance regression); by the time this runs, `update_recorded_file`
/// has already panicked if such a case exists and `accept_regression` is
/// false, so reaching that branch with `accept_regression` false would be a
/// logic error in the caller, not a case this function silently mishandles.
fn resolve_metric_value(old_value: f64, new_value: f64, accept_regression: bool) -> f64 {
    if new_value < old_value {
        // Case 1: genuine improvement, however small. Always take it.
        new_value
    } else if is_worse_beyond_tolerance(old_value, new_value) {
        // Case 3: a genuine regression. Only reachable here with
        // `accept_regression` true -- `update_recorded_file`'s `blocking`
        // check already refused the write otherwise.
        debug_assert!(
            accept_regression,
            "resolve_metric_value reached a beyond-tolerance regression without \
             accept_regression=true; update_recorded_file's blocking check should have panicked \
             before this ran"
        );
        new_value
    } else {
        // Case 2: unchanged within tolerance -- floating-point noise, not a
        // fact about the plan. Keep the recorded value so it can never be
        // walked backward by noise alone.
        old_value
    }
}

/// Applies [`resolve_metric_value`] to every field, field by field. Written
/// out by name (mirroring [`RatchetMetrics::fields`]'s own listing) rather
/// than generically, because [`RatchetMetrics`] is a concrete struct with no
/// reflective field-by-name setter.
fn resolve_metrics(
    previous: &RatchetMetrics,
    measured: &RatchetMetrics,
    accept_regression: bool,
) -> RatchetMetrics {
    RatchetMetrics {
        worst_tip_area_ratio: resolve_metric_value(
            previous.worst_tip_area_ratio,
            measured.worst_tip_area_ratio,
            accept_regression,
        ),
        total_scar_area_mm2: resolve_metric_value(
            previous.total_scar_area_mm2,
            measured.total_scar_area_mm2,
            accept_regression,
        ),
        support_resin_volume_ml: resolve_metric_value(
            previous.support_resin_volume_ml,
            measured.support_resin_volume_ml,
            accept_regression,
        ),
        residual_uncovered_area_mm2: resolve_metric_value(
            previous.residual_uncovered_area_mm2,
            measured.residual_uncovered_area_mm2,
            accept_regression,
        ),
        worst_uncovered_gap_mm: resolve_metric_value(
            previous.worst_uncovered_gap_mm,
            measured.worst_uncovered_gap_mm,
            accept_regression,
        ),
        ownerless_model_area_mm2: resolve_metric_value(
            previous.ownerless_model_area_mm2,
            measured.ownerless_model_area_mm2,
            accept_regression,
        ),
        abandoned_load_mm2: resolve_metric_value(
            previous.abandoned_load_mm2,
            measured.abandoned_load_mm2,
            accept_regression,
        ),
    }
}

/// **What `tolerance_for` is FOR, and the one rule this function enforces
/// because of it.** The tolerance exists so that floating-point summation
/// noise -- a metric that is unchanged in every physical sense but lands a
/// few ULPs off from run to run -- does not fail a run that did not really
/// change. It is arithmetic hygiene, not a licence to worsen. So for a single
/// metric on a model with a comparable recorded baseline (same corpus
/// `sha256`), exactly one of three things happens:
///
/// 1. **Genuine improvement** (`new < old`, by any amount, even far below
///    tolerance): recorded freely and silently. A tighter number can never
///    make the ratchet unsound, so nothing gates this case.
/// 2. **Unchanged within tolerance** (`new >= old` but not worse by more than
///    `tolerance_for(old)`): the RECORDED value is kept, not overwritten.
///    This is the fix for the defect this function used to carry -- it used
///    to write `measured` unconditionally once no metric was a `blocking`
///    violation, so a metric nudging worse by less than tolerance was stored
///    as the new baseline, and repeated `PLINTH_RATCHET_UPDATE=1` runs could
///    walk it backward in tolerated increments forever with no
///    `PLINTH_RATCHET_ACCEPT_REGRESSION=1` ever required. Keeping the old
///    value instead means noise can never accumulate into a regression no
///    matter how many times the harness is re-run.
/// 3. **Genuine regression** (worse by more than tolerance): refused unless
///    `PLINTH_RATCHET_ACCEPT_REGRESSION=1` is also set (checked by the
///    `blocking` panic below, before any of this rewriting happens); when
///    accepted, the new, worse value is recorded, exactly as before.
///
/// A brand-new model, or one whose corpus STL changed (a re-baseline,
/// already reported separately), has no comparable recorded value at all --
/// its fresh measurement is taken whole, same as always.
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
        let existing = recorded.models.get(name);
        let resolved_row = match existing {
            Some(previous) if previous.corpus_sha256 == measured.corpus_sha256 => {
                RecordedModelRow {
                    corpus_sha256: measured.corpus_sha256.clone(),
                    metrics: resolve_metrics(
                        &previous.metrics,
                        &measured.metrics,
                        accept_regression,
                    ),
                }
            }
            // No recorded baseline yet, or the corpus STL changed underneath
            // it (a re-baseline, already reported by `rebaseline_needed` in
            // the comparison path): nothing recorded is comparable, so the
            // fresh measurement is taken whole.
            _ => measured.clone(),
        };
        if existing != Some(&resolved_row) {
            changed_models.push(name.clone());
        }
        updated.models.insert(name.clone(), resolved_row);
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

// ---------------------------------------------------------------------------
// Defect 3 proof: the ratchet update path must be one-way.
// ---------------------------------------------------------------------------

/// Builds a `RatchetMetrics` with every field set to the same value, so a
/// test can nudge exactly one field without having to name the other six.
fn uniform_metrics(value: f64) -> RatchetMetrics {
    RatchetMetrics {
        worst_tip_area_ratio: value,
        total_scar_area_mm2: value,
        support_resin_volume_ml: value,
        residual_uncovered_area_mm2: value,
        worst_uncovered_gap_mm: value,
        ownerless_model_area_mm2: value,
        abandoned_load_mm2: value,
    }
}

const TEST_SHA: &str = "sha256:0000000000000000000000000000000000000000000000000000000000aa";

fn single_model_file(name: &str, sha256: &str, metrics: RatchetMetrics) -> RecordedRatchetFile {
    let mut models = BTreeMap::new();
    models.insert(
        name.to_owned(),
        RecordedModelRow {
            corpus_sha256: sha256.to_owned(),
            metrics,
        },
    );
    RecordedRatchetFile {
        schema: SCHEMA.to_owned(),
        models,
    }
}

/// A private, per-test scratch path under the crate's own `target/` dir --
/// never `tests/fixtures/support_ratchets.json`, the real committed baseline
/// these tests must never touch.
fn scratch_ratchet_path(test_name: &str) -> PathBuf {
    let dir = workspace_root()
        .join("target")
        .join("ratchet_harness_test_scratch");
    fs::create_dir_all(&dir).expect("create scratch dir");
    dir.join(format!(
        "{test_name}-{}-{:?}.json",
        std::process::id(),
        thread::current().id()
    ))
}

/// **Proof for defect 3, unit level.** [`resolve_metric_value`] directly:
/// an improvement is always taken, a worsening within tolerance keeps the
/// OLD value (this is the fix -- the pre-fix code did not have this
/// function at all and simply took `measured` unconditionally), and an
/// accepted beyond-tolerance regression takes the new value.
// `float_cmp` is the workspace's real-defects lint for exact equality on
// INDEPENDENTLY COMPUTED geometry (see `Cargo.toml`'s lint table). Every
// comparison below instead checks that `resolve_metric_value` returns one of
// its own inputs UNCHANGED, bit for bit -- there is no independent
// computation on either side to have drifted, so exact equality is exactly
// the property under test, not a code smell.
#[allow(clippy::float_cmp)]
#[test]
fn resolve_metric_value_implements_the_three_cases() {
    let old = 10.0_f64;
    let tolerance = tolerance_for(old);

    // Case 1: improvement, however small -- always taken.
    assert_eq!(resolve_metric_value(old, old - 1e-15, false), old - 1e-15);
    assert_eq!(resolve_metric_value(old, 0.0, false), 0.0);

    // Case 2: worse, but within tolerance -- THE FIX. The old value survives.
    let worse_within_tolerance = old + tolerance * 0.5;
    assert_eq!(
        resolve_metric_value(old, worse_within_tolerance, false),
        old,
        "a metric worsened by half the tolerance must not move the recorded baseline"
    );
    // Exactly unchanged is the same case and must also keep `old` (trivially
    // true either way, asserted for completeness).
    assert_eq!(resolve_metric_value(old, old, false), old);

    // Case 3: a genuine, beyond-tolerance regression, accepted explicitly.
    let worse_beyond_tolerance = old + tolerance * 2.0;
    assert_eq!(
        resolve_metric_value(old, worse_beyond_tolerance, true),
        worse_beyond_tolerance
    );
}

/// **Proof for defect 3, end-to-end.** This is the scenario named in the
/// brief: a metric worsened by half the tolerance, the update path taken
/// with `accept_regression=false` (exactly what running with only
/// `PLINTH_RATCHET_UPDATE=1` selects -- see `support_ratchets_do_not_regress`
/// for how that env var maps to this flag), and the recorded baseline must
/// NOT move.
///
/// Confirmed red against the pre-fix code before this commit: the old
/// `update_recorded_file` copied `measured.clone()` into the file
/// unconditionally whenever `blocking` was empty, and a worsening within
/// tolerance is never `blocking` (that is exactly what "within tolerance"
/// means) -- so it wrote `worse_within_tolerance`, not `old`, and this
/// assertion failed. It is green now that the write path resolves
/// metric-by-metric through [`resolve_metric_value`].
// See `resolve_metric_value_implements_the_three_cases`'s `float_cmp` note --
// the values compared here round-trip through JSON unchanged, they are not
// independently computed.
#[allow(clippy::float_cmp)]
#[test]
fn update_with_plain_update_flag_does_not_move_baseline_within_tolerance() {
    let old = 10.0_f64;
    let tolerance = tolerance_for(old);
    let worse_within_tolerance = old + tolerance * 0.5;
    assert!(
        !is_worse_beyond_tolerance(old, worse_within_tolerance),
        "test setup: this nudge must be inside the tolerance band"
    );

    let recorded = single_model_file("TestModel", TEST_SHA, uniform_metrics(old));
    let mut measured_rows = BTreeMap::new();
    let mut measured_metrics = uniform_metrics(old);
    measured_metrics.total_scar_area_mm2 = worse_within_tolerance;
    measured_rows.insert(
        "TestModel".to_owned(),
        RecordedModelRow {
            corpus_sha256: TEST_SHA.to_owned(),
            metrics: measured_metrics,
        },
    );

    let path = scratch_ratchet_path("baseline-does-not-move");
    // `accept_regression: false` -- this is the plain `PLINTH_RATCHET_UPDATE=1`
    // case named in the brief, with no `PLINTH_RATCHET_ACCEPT_REGRESSION=1`.
    update_recorded_file(&path, &recorded, &measured_rows, false);

    let written = load_recorded(&path);
    let row = written.models.get("TestModel").expect("row written");
    assert_eq!(
        row.metrics.total_scar_area_mm2, old,
        "a metric worsened by less than tolerance must not move the recorded baseline, even \
         after PLINTH_RATCHET_UPDATE=1 alone"
    );
    // Every other metric was untouched, so they must round-trip unchanged
    // too -- this is not a coincidental pass on the one field being watched.
    assert_eq!(row.metrics.worst_tip_area_ratio, old);
    assert_eq!(row.metrics.support_resin_volume_ml, old);

    fs::remove_file(&path).ok();
}

/// A genuine improvement is still recorded freely and silently by
/// `PLINTH_RATCHET_UPDATE=1` alone -- the doctrine's easy case must stay easy.
// See `resolve_metric_value_implements_the_three_cases`'s `float_cmp` note.
#[allow(clippy::float_cmp)]
#[test]
fn update_with_plain_update_flag_records_genuine_improvement() {
    let old = 10.0_f64;
    let improved = 7.5_f64;
    let recorded = single_model_file("TestModel", TEST_SHA, uniform_metrics(old));
    let mut measured_rows = BTreeMap::new();
    let mut measured_metrics = uniform_metrics(old);
    measured_metrics.total_scar_area_mm2 = improved;
    measured_rows.insert(
        "TestModel".to_owned(),
        RecordedModelRow {
            corpus_sha256: TEST_SHA.to_owned(),
            metrics: measured_metrics,
        },
    );

    let path = scratch_ratchet_path("records-improvement");
    update_recorded_file(&path, &recorded, &measured_rows, false);

    let written = load_recorded(&path);
    let row = written.models.get("TestModel").expect("row written");
    assert_eq!(
        row.metrics.total_scar_area_mm2, improved,
        "a genuine improvement must be recorded by PLINTH_RATCHET_UPDATE=1 alone"
    );

    fs::remove_file(&path).ok();
}

/// A genuine, beyond-tolerance regression is refused by
/// `PLINTH_RATCHET_UPDATE=1` alone -- `update_recorded_file` panics and, per
/// its own contract, writes nothing at all.
#[test]
fn update_with_plain_update_flag_refuses_genuine_regression() {
    let old = 10.0_f64;
    let tolerance = tolerance_for(old);
    let worse_beyond_tolerance = old + tolerance * 2.0;
    let recorded = single_model_file("TestModel", TEST_SHA, uniform_metrics(old));
    let mut measured_rows = BTreeMap::new();
    let mut measured_metrics = uniform_metrics(old);
    measured_metrics.total_scar_area_mm2 = worse_beyond_tolerance;
    measured_rows.insert(
        "TestModel".to_owned(),
        RecordedModelRow {
            corpus_sha256: TEST_SHA.to_owned(),
            metrics: measured_metrics,
        },
    );

    let path = scratch_ratchet_path("refuses-regression");
    fs::remove_file(&path).ok();
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        update_recorded_file(&path, &recorded, &measured_rows, false);
    }));
    assert!(
        result.is_err(),
        "a beyond-tolerance regression must panic without PLINTH_RATCHET_ACCEPT_REGRESSION=1"
    );
    assert!(
        !path.exists(),
        "a refused update must write nothing at all, not a partial file"
    );
}

/// The same regression IS recorded once
/// `PLINTH_RATCHET_ACCEPT_REGRESSION=1` (`accept_regression: true`) is also
/// set, exactly as the doctrine requires for a deliberate, argued change.
// See `resolve_metric_value_implements_the_three_cases`'s `float_cmp` note.
#[allow(clippy::float_cmp)]
#[test]
fn update_with_accept_regression_flag_records_genuine_regression() {
    let old = 10.0_f64;
    let tolerance = tolerance_for(old);
    let worse_beyond_tolerance = old + tolerance * 2.0;
    let recorded = single_model_file("TestModel", TEST_SHA, uniform_metrics(old));
    let mut measured_rows = BTreeMap::new();
    let mut measured_metrics = uniform_metrics(old);
    measured_metrics.total_scar_area_mm2 = worse_beyond_tolerance;
    measured_rows.insert(
        "TestModel".to_owned(),
        RecordedModelRow {
            corpus_sha256: TEST_SHA.to_owned(),
            metrics: measured_metrics,
        },
    );

    let path = scratch_ratchet_path("accepts-regression");
    update_recorded_file(&path, &recorded, &measured_rows, true);

    let written = load_recorded(&path);
    let row = written.models.get("TestModel").expect("row written");
    assert_eq!(
        row.metrics.total_scar_area_mm2, worse_beyond_tolerance,
        "an explicitly accepted regression must be recorded"
    );

    fs::remove_file(&path).ok();
}

// ---------------------------------------------------------------------------
// Defect 4 proof: a timed-out worker must not overlap the next one.
// ---------------------------------------------------------------------------

/// **Proof for defect 4.** Two synthetic "models" run through
/// [`run_with_budget`] sharing one `previous_worker` slot, the same way
/// `support_ratchets_do_not_regress` shares one slot across the whole corpus
/// loop. The first closure blows its budget and, once released, records the
/// wall-clock instant it finished into a shared `Mutex`. The second closure
/// records the instant it STARTED. If the fix works, the join at the top of
/// `run_with_budget` forces "first finished" to happen-before "second
/// started" -- proving no overlap, not merely asserting it.
///
/// Confirmed red against the pre-fix code: the old function discarded the
/// spawn's `JoinHandle` and never joined it, so the second closure's start
/// time could and did land before the first closure's (artificially delayed)
/// finish time.
#[test]
fn timed_out_worker_does_not_overlap_the_next_one() {
    use std::sync::{Arc, Mutex};

    let first_finished_at: Arc<Mutex<Option<std::time::Instant>>> = Arc::new(Mutex::new(None));
    let second_started_at: Arc<Mutex<Option<std::time::Instant>>> = Arc::new(Mutex::new(None));

    let mut previous_worker: Option<thread::JoinHandle<()>> = None;

    // First "model": deliberately outlasts its tiny budget, then -- well
    // after the caller has given up and moved on -- records when it actually
    // finished.
    let first_finished_at_clone = Arc::clone(&first_finished_at);
    let first_result = run_with_budget(
        "test-first",
        Duration::from_millis(20),
        &mut previous_worker,
        move || {
            thread::sleep(Duration::from_millis(200));
            *first_finished_at_clone.lock().expect("lock") = Some(std::time::Instant::now());
        },
    );
    assert!(
        first_result.is_err(),
        "test setup: the first closure must exceed its budget and time out"
    );
    assert!(
        previous_worker.is_some(),
        "a timed-out worker's handle must be retained for the next call to join"
    );

    // Second "model": starts immediately after. Its very first action is to
    // record its own start time, before doing any work of its own.
    let second_started_at_clone = Arc::clone(&second_started_at);
    let second_result = run_with_budget(
        "test-second",
        Duration::from_secs(5),
        &mut previous_worker,
        move || {
            *second_started_at_clone.lock().expect("lock") = Some(std::time::Instant::now());
        },
    );
    assert!(
        second_result.is_ok(),
        "the second closure should complete well within its budget"
    );
    assert!(
        previous_worker.is_none(),
        "the second call's own handle must have been reclaimed immediately since it reported \
         within budget"
    );

    let finished = first_finished_at
        .lock()
        .expect("lock")
        .expect("first closure ran to completion");
    let started = second_started_at
        .lock()
        .expect("lock")
        .expect("second closure ran");
    assert!(
        started >= finished,
        "the second worker started at {started:?}, before the first (timed-out) worker actually \
         finished at {finished:?} -- their work overlapped"
    );
}

/// Sanity check that two calls through [`run_with_budget`] that both
/// complete within budget behave exactly as before: no artificial join
/// delay is introduced when there is no straggler to wait for.
#[test]
fn run_with_budget_has_no_overlap_penalty_when_nothing_times_out() {
    let mut previous_worker: Option<thread::JoinHandle<()>> = None;
    let first = run_with_budget(
        "fast-first",
        Duration::from_secs(5),
        &mut previous_worker,
        || 41,
    );
    assert_eq!(first, Ok(41));
    assert!(previous_worker.is_none());
    let second = run_with_budget(
        "fast-second",
        Duration::from_secs(5),
        &mut previous_worker,
        || 1,
    );
    assert_eq!(second, Ok(1));
    assert!(previous_worker.is_none());
}
