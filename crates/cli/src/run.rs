//! The transport run: each input STL is read, lifted, written to a temp file, checked by the
//! separate `plinth-check` program on those exact bytes, and published only if the checker
//! read the file cleanly (design sections 7.3 and 10 step 7). Models run one at a time, in
//! sorted input order; a failed model never stops the others, and finished files are kept
//! (PRD FR-42).

#[cfg(test)]
#[path = "run_status_tests.rs"]
mod status_tests;

use crate::check::{self, Checked};
use crate::mem;
use crate::names::{self, Named};
use crate::publish::{self, Fault, Faults};
use plinth_emit::{Parallel, transport_project};
use plinth_geom::{Mesh, measure::Measure, slice::LayerGrid};
use plinth_support::{construct as c, load, place, report_data as rd};
use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

/// Everything a run needs, with every default already resolved.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Config {
    /// Input STL files, as given.
    pub inputs: Vec<PathBuf>,
    /// Directory for the finished files.
    pub out_dir: PathBuf,
    /// Cap on worker threads for every parallel step.
    pub max_threads: usize,
    /// The `plinth-check` program.
    pub checker: PathBuf,
    /// The limits file handed to the checker.
    pub limits: PathBuf,
    /// Ask for print-candidate files: a file is published only if the checker passed it with
    /// no applicable rule unverified or violated. Otherwise (transport mode) a file the
    /// checker could read is published labelled "not a print candidate".
    pub candidate: bool,
    /// Review mode: supports for any posed STL (resting up +Z), to look at. Never a print
    /// candidate; refused together with `candidate`.
    pub review: Option<Review>,
}

/// The bounds of review mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Review {
    /// Wall-clock budget of the contact planner.
    pub plan_budget: Duration,
    /// Wall-clock limit of one checker run.
    pub checker_timeout: Duration,
    /// Test hook, `None` in every real run (the command line has no flag for it): the
    /// planner's deadline fires at the first check in the planner phase of this name,
    /// whatever the clock says. See `PlanSettings::stop_at_phase`.
    pub stop_plan_at_phase: Option<&'static str>,
    /// Hollow and drill an eligible convex mesh before the unchanged support pipeline.
    pub hollow_fixture: bool,
}

/// The label every review file, report and embedded model name carries.
pub const REVIEW_LABEL: &str = "REVIEW ONLY — NOT A PRINT CANDIDATE";
/// [`REVIEW_LABEL`] as it appears in file names.
pub const REVIEW_FILE_TAG: &str = "REVIEW-ONLY-NOT-A-PRINT-CANDIDATE";

/// What review mode did to one model, beyond the shared record.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ReviewRun {
    /// The planner phase running when the planning budget passed.
    pub stopped_at_budget: Option<&'static str>,
    /// The checker found a rule violated in a well-formed file; the file was kept anyway.
    pub checker_failed: bool,
}

/// The status line of a review model: the label plus what stopped it short.
#[must_use]
pub fn review_status(r: &ReviewRun) -> String {
    let mut s = REVIEW_LABEL.to_owned();
    if let Some(phase) = r.stopped_at_budget {
        s.push_str(&format!(" — stopped at time budget (during {phase})"));
    }
    if r.checker_failed {
        s.push_str(" — checker-failed");
    }
    s
}

/// The model name embedded in a review file: the input's stem (cut to fit the 256-byte
/// name field) and the review status. "checker-failed" is not known until after the file
/// is written, so it appears in the file name and reports only.
#[must_use]
pub fn review_model_name(input: &Path, r: &ReviewRun) -> String {
    let stem = input
        .file_stem()
        .map_or_else(String::new, |s| s.to_string_lossy().replace('\0', "_"));
    let mut end = stem.len().min(80);
    while !stem.is_char_boundary(end) {
        end -= 1;
    }
    let only_known_before_writing = ReviewRun {
        checker_failed: false,
        ..*r
    };
    format!(
        "{} {}",
        &stem[..end],
        review_status(&only_known_before_writing)
    )
}

/// Wall time of one step of one model.
#[derive(Debug, Clone, PartialEq)]
pub struct Step {
    /// `read`, `convert`, `serialize`, `write`, `check` or `publish`.
    pub name: &'static str,
    /// Seconds.
    pub secs: f64,
}

/// What became of one model.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    /// The file is in the output folder under its final name.
    Published {
        /// The final path.
        path: PathBuf,
        /// Its size.
        bytes: u64,
        /// What the checker said.
        checked: Checked,
    },
    /// Nothing was published for this model.
    Failed {
        /// The step that failed.
        stage: &'static str,
        /// A located message.
        message: String,
        /// The checker's answer, when it got that far.
        checked: Option<Checked>,
    },
}

/// One model's record.
#[derive(Debug, Clone, PartialEq)]
pub struct ModelRun {
    /// The input path.
    pub input: PathBuf,
    /// The output name without extension.
    pub stem: String,
    /// Triangles read (0 when the read failed).
    pub triangles: usize,
    /// Time per step, in the order run.
    pub steps: Vec<Step>,
    /// Tasks and threads of the parallel step, when it ran.
    pub parallel: Option<Parallel>,
    /// A supported construction attempt, not a transport conversion.
    pub supported: bool,
    /// Unresolved placement, load or representation measurement.
    pub planning_blockers: bool,
    /// FR-21 emitted-file evidence also embedded in run.json.
    pub fr21: Option<crate::json::Json>,
    /// Convex hollow measurements on the decoded model (review mode only).
    pub hollow: Option<crate::json::Json>,
    /// Review-mode facts (`None` outside review mode).
    pub review: Option<ReviewRun>,
    /// The result.
    pub outcome: Outcome,
}

/// A finished run.
#[derive(Debug, Clone)]
pub struct Run {
    /// One record per input, in sorted input order.
    pub models: Vec<ModelRun>,
    /// Wall seconds for the whole run.
    pub total_secs: f64,
    /// Job peak memory, or why it could not be read.
    pub peak: Result<mem::Peak, String>,
}

impl Run {
    /// Number of models that were published.
    #[must_use]
    pub fn published(&self) -> usize {
        self.models
            .iter()
            .filter(|m| matches!(m.outcome, Outcome::Published { .. }))
            .count()
    }
}

/// Gather the final frozen layout and its modelled loads. The separate checker,
/// not these heuristic results, decides whether emitted geometry passes.
#[must_use]
pub fn collect_report_data(
    plan: &c::SupportPlan,
    layers: load::ModelLayers,
    proposal: place::Proposal,
) -> rd::ReportData {
    let mut layout = proposal.layout;
    place::add_members(plan, &mut layout);
    let ld = rd::LoadData::build(&layers.stack, layout, layers.structure, proposal.needs);
    let mut data = rd::ReportData {
        process: rd::ProcessValues {
            layer_height_mm: 0.05,
            layer_height_source: "owner profile [O]".into(),
            design_ratio: load::DESIGN_RATIO,
            h1_admission_mm2: 0.48,
            coverage_reach_mm: 3.,
            overhang_deg: 45.,
            lift_mm: 5.,
        },
        ..rd::ReportData::default()
    };
    data.identity.approved_sha256 = plan.model().keep_up().approved_sha256.clone();
    data.identity.pose = "Approved keep-up +115 degrees about X (disc 65 degrees from flat); identity rotation at generation, centred in f32; lift 5 mm [O].".into();
    for c in &ld.layout.contacts {
        data.supports.contacts.add(c.site_class);
    }
    data.warnings = ld.detail.report.warnings.clone();
    for &contact in plan.unbraced_columns() {
        data.warnings.push(load::LoadWarning {
            class: load::WarningClass::Intervention,
            kind: "unbraced column".into(),
            layers: vec![ld.layout.contacts[contact as usize].first_layer],
            contacts: vec![contact],
            location_mm: None,
            message: format!("Column at contact {contact} has no admitted 45-degree X-pair to a carrier 4–15 mm away. Add bracing at this picture marker; stability is unassessed."),
        });
    }
    if proposal.termination == place::Termination::TimeBudget {
        let phase = proposal.stopped_at_budget.unwrap_or("planning");
        data.warnings.push(load::LoadWarning { class:load::WarningClass::Intervention,
            kind:"construction plan stopped at time budget".into(), layers:vec![], contacts:vec![], location_mm:None,
            message:format!("The search was stopped at its time budget during {phase}; the contacts chosen so far are kept and needs may remain unresolved. This is not a proof that no legal site exists.") });
    } else if proposal.termination != place::Termination::Satisfied {
        data.warnings.push(load::LoadWarning { class:load::WarningClass::Intervention,
            kind:"construction plan unresolved".into(), layers:vec![], contacts:vec![], location_mm:None,
            message:format!("Search ended {:?}; unresolved needs remain. This is not a proof that no legal site exists.",proposal.termination) });
    }
    let partial: Vec<_> = ld
        .detail
        .sections
        .iter()
        .filter(|s| {
            s.kind == load::PartKind::Patch
                && ld
                    .layout
                    .contacts
                    .get(s.chain as usize)
                    .is_some_and(|c| s.area_mm2 < c.patch_lower_mm2 - 1e-9)
                && ld
                    .layout
                    .patch_history
                    .get(s.chain as usize)
                    .is_none_or(Vec::is_empty)
        })
        .collect();
    if !partial.is_empty() {
        data.warnings.push(load::LoadWarning { class:load::WarningClass::Intervention,
            kind:"early patch growth not certified".into(), location_mm:None,
            layers:partial.iter().map(|s|s.tributary.1).collect(), contacts:partial.iter().map(|s|s.chain).collect(),
            message:"The limiting early patches use a disc approximation [U], not certified per-layer geometry. P5=18 remains a lower bound, not a second planner run.".into() });
    }
    data.load = Some(ld.detail.report.clone());
    data.load_data = Some(ld);
    data.run.rejections = proposal.rejections;
    data
}

/// A single status gate shared by JSON and publication. It also rejects incomplete
/// controlled checker results; production check::run validates the rule identities.
/// Only H5 may be not_applicable: the independent checker proves that exemption.
#[must_use]
pub fn candidate_allowed(requested: bool, checked: &Checked, planning_blockers: bool) -> bool {
    requested
        && !planning_blockers
        && checked.rules.len() == 9
        && checked.candidate_blockers().is_empty()
        && (1..=9).all(|i| {
            checked
                .rules
                .iter()
                .filter(|r| {
                    r.id == format!("H{i}")
                        && (r.status == "pass" || (i == 5 && r.status == "not_applicable"))
                })
                .count()
                == 1
        })
}

type Failure = (&'static str, String);

fn timed<T>(steps: &mut Vec<Step>, name: &'static str, f: impl FnOnce() -> T) -> T {
    let t = Instant::now();
    let v = f();
    steps.push(Step {
        name,
        secs: t.elapsed().as_secs_f64(),
    });
    v
}

struct Built {
    bytes: Vec<u8>,
    report: Option<rd::ReportData>,
    producer_patch_upper: Option<f64>,
}
const APPROVED_ROUND25: &str = "853709e4063de2e3c9dc7a646f13d5ede5a2f26c4a4372ab220cfe153480750a";

fn model_layers(model: &c::PosedModel, threads: usize) -> Result<load::ModelLayers, Failure> {
    let shift = -f64::from(model.plate_z());
    let local = model.mesh();
    let world = Mesh::new(
        local.xs().to_vec(),
        local.ys().to_vec(),
        local.zs().iter().map(|z| z + shift).collect(),
        local.indices().to_vec(),
    )
    .map_err(|e| ("slice", e.to_string()))?;
    let grid = LayerGrid::covering(&world, 0.05).ok_or(("slice", "no layer grid".into()))?;
    match load::build_model_layers(
        &world,
        &grid,
        &load::StackOptions {
            threads,
            ..load::StackOptions::default()
        },
    ) {
        Measure::Known(layers) => Ok(layers),
        Measure::Unverified(why) => Err(("slice", why.into())),
    }
}

fn proposal_diagnostic(
    proposal: &place::Proposal,
    answers: &[Result<c::Quote, c::Reject>],
) -> String {
    let upper = answers
        .iter()
        .filter_map(|q| q.as_ref().ok())
        .map(|q| q.contact().patch().upper())
        .reduce(f64::max);
    let ratio = proposal
        .load
        .sections
        .iter()
        .map(|s| s.ratios.tributary)
        .reduce(f64::max);
    let unprintable = proposal
        .load
        .warnings
        .iter()
        .filter(|w| w.class == load::WarningClass::Unprintable)
        .count();
    format!(
        "{:?}; {} proposed contacts; tip ball radii {:?} mm; rejections {:?}; private-plan patch upper {upper:?} mm² (NOT an emitted-file measurement); largest modelled tributary ratio {ratio:?}; {unprintable} unprintable warnings",
        proposal.termination,
        proposal.candidates.len(),
        proposal
            .candidates
            .iter()
            .map(|c| c.ball_r)
            .collect::<Vec<_>>(),
        proposal.rejections
    )
}

#[allow(clippy::float_cmp)] // Exact equality binds certificates to decoded f32 geometry.
fn checked_decode(
    bytes: &[u8],
    project: &plinth_chitubox::Project,
) -> Result<plinth_chitubox::Project, Failure> {
    let decoded = plinth_chitubox::parse(bytes).map_err(|e| ("replay", e.to_string()))?;
    let same = decoded.models.len() == project.models.len()
        && decoded.models.iter().zip(&project.models).all(|(a, b)| {
            a.mesh == b.mesh
                && a.translate == b.translate
                && a.supports
                    .as_ref()
                    .zip(b.supports.as_ref())
                    .is_some_and(|(a, b)| {
                        a.records.len() == b.records.len()
                            && a.records.iter().zip(&b.records).all(|(a, b)| {
                                a.mesh == b.mesh
                                    && a.record_type == b.record_type
                                    && a.start == b.start
                                    && a.end == b.end
                                    && a.r1 == b.r1
                                    && a.r2 == b.r2
                            })
                    })
        });
    if !same {
        return Err((
            "replay",
            "Decoded geometry differs from the fully certified proposal; no load result is claimed"
                .into(),
        ));
    }
    // ChituBox loads but does not draw supports whose attr names preset 0 or a preset
    // missing from the table; refuse such a file instead of writing it.
    for m in &decoded.models {
        if let Some(s) = &m.supports {
            plinth_emit::shapes::check_support_presets(s).map_err(|e| {
                (
                    "replay",
                    format!("Decoded supports would not draw in ChituBox: {e}"),
                )
            })?;
        }
    }
    Ok(decoded)
}

fn fail(e: c::Reject) -> Failure {
    (
        "construct",
        format!("{e:?}; no partially admitted file was emitted"),
    )
}

/// Everything the planning stage hands to construction.
struct Planned {
    layers: load::ModelLayers,
    builder: c::PlanBuilder<c::Contacts>,
    proposal: place::Proposal,
    answers: Vec<Result<c::Quote, c::Reject>>,
    diagnostic: String,
}

/// Pose, slice and plan. Round 25 runs with the owner's recorded +115 degree keep-up and
/// no time bound. Review mode treats the input as already posed, resting up +Z (typed, see
/// `RestingUp`), records the input's real hash as provenance only, and bounds the planner
/// by its wall-clock budget.
fn plan_stage(
    cfg: &Config,
    mesh: &Mesh,
    sha256: &str,
    m: &mut ModelRun,
) -> Result<Planned, Failure> {
    let limits_text =
        std::fs::read_to_string(&cfg.limits).map_err(|e| ("construct", e.to_string()))?;
    let limits = c::Limits::parse(&limits_text).map_err(fail)?;
    let keep = if cfg.review.is_some() {
        c::KeepUpRecord::resting_up(c::RestingUp::PlusZ, sha256.into())
    } else {
        c::KeepUpRecord {
            rotation_x_deg: 115.,
            approved_sha256: APPROVED_ROUND25.into(),
        }
    };
    let model = c::PosedModel::new(mesh, &keep, &limits).map_err(|e| ("pose", format!("{e:?}")))?;
    let layers = timed(&mut m.steps, "slice", || {
        model_layers(&model, cfg.max_threads)
    })?;
    let builder = c::PlanBuilder::new(
        model,
        limits,
        c::ProcessRecord {
            layer_height_mm: 0.05,
            layer_origin_mm: 0.,
        },
    )
    .with_workers(cfg.max_threads)
    .map_err(fail)?;
    let proposal = timed(&mut m.steps, "plan", || {
        let settings = place::PlanSettings {
            deadline: cfg.review.map(|r| Instant::now() + r.plan_budget),
            stop_at_phase: cfg.review.and_then(|r| r.stop_plan_at_phase),
            ..place::PlanSettings::default()
        };
        place::plan_contacts(&builder, &layers, &settings)
    });
    let mut answers = Vec::new();
    builder.probe(&proposal.candidates, &mut answers);
    let diagnostic = proposal_diagnostic(&proposal, &answers);
    if let Some(r) = m.review.as_mut() {
        r.stopped_at_budget = proposal
            .stopped_at_budget
            .filter(|_| proposal.termination == place::Termination::TimeBudget);
    }
    let stopped = proposal
        .stopped_at_budget
        .map_or_else(String::new, |phase| {
            format!("planning stopped at its time budget during {phase}; ")
        });
    if proposal.stopped_at_budget.is_some() && proposal.candidates.is_empty() {
        return Err((
            "plan",
            format!(
                "{stopped}no contact had been chosen yet, so there is no best-so-far plan to emit; {diagnostic}"
            ),
        ));
    }
    if !proposal.routed {
        return Err((
            "plan",
            format!(
                "{stopped}route preview rejected before contact commit: {:?}; {diagnostic}",
                proposal.routing_blocker
            ),
        ));
    }
    Ok(Planned {
        layers,
        builder,
        proposal,
        answers,
        diagnostic,
    })
}

fn supported(
    cfg: &Config,
    named: &Named,
    (mesh, sha256): (&Mesh, &str),
    m: &mut ModelRun,
) -> Result<Built, Failure> {
    let Planned {
        layers,
        mut builder,
        proposal,
        answers,
        diagnostic,
    } = plan_stage(cfg, mesh, sha256, m)?;
    let contact_loads = place::routing_loads(&load::analyse_full(
        &layers.stack,
        &proposal.layout,
        &load::Extras::default(),
    ));
    let column_radii: Vec<_> = answers
        .iter()
        .zip(&contact_loads)
        .map(|(q, &load)| q.as_ref().map(|q| q.contact().column_radius(load)))
        .collect::<Result<_, _>>()
        .map_err(|e| fail(e.clone()))?;
    let contacts = builder
        .commit(c::QuoteBatch::from_results(answers).map_err(fail)?)
        .map_err(fail)?;
    let mut routed = builder.into_routing();
    let columns: Vec<_> = contacts
        .into_iter()
        .zip(column_radii)
        .map(|(contact, radius)| {
            c::Section::new(radius).map(|section| c::MemberProposal::Column { contact, section })
        })
        .collect::<Result<_, _>>()
        .map_err(fail)?;
    let result = timed(&mut m.steps, "construct", || {
        let mut answers = Vec::new();
        routed.probe_members(&columns, &mut answers);
        routed.commit_members(c::MemberQuoteBatch::from_results(answers)?)?;
        let braces = routed.brace_proposals()?;
        let mut answers = Vec::new();
        routed.probe_members(&braces, &mut answers);
        routed.commit_members(c::MemberQuoteBatch::from_results(answers)?)?;
        routed.raft_connected()?;
        routed.freeze()
    });
    let plan = result.map_err(|e| {
        (
            "construct",
            format!("{e:?}; {diagnostic}; no geometry was relaxed to force an output"),
        )
    })?;
    let project = timed(&mut m.steps, "emit", || match m.review {
        Some(r) => {
            plinth_emit::constructed::project_named(&plan, &review_model_name(&named.input, &r))
        }
        None => plinth_emit::constructed::project(&plan),
    })
    .map_err(|e| ("emit", format!("{e:?}")))?;
    let bytes = timed(&mut m.steps, "serialize", || project.to_bytes())
        .map_err(|e| ("serialize", e.to_string()))?;
    // Bind the complete replay and all producer certificates to the bytes that
    // will be written. No rounded descriptor or mesh may differ after decoding.
    let decoded = timed(&mut m.steps, "decode-replay", || {
        checked_decode(&bytes, &project)
    })?;
    let mut data = timed(&mut m.steps, "load-replay", || {
        collect_report_data(&plan, layers, proposal)
    });
    if cfg.review.is_some() {
        data.identity.original_sha256 = sha256.into();
        data.identity.approved_sha256 = "none: review mode records no approval".into();
        data.identity.pose = "Review mode: pose as drawn in the input STL, resting up +Z; no rotation applied; centred in f32; lift 5 mm [O].".into();
    }
    data.written_metrics = decoded
        .models
        .first()
        .map(crate::load_report::measure_written);
    data.identity.generated_sha256 = crate::sha256::hex(&bytes);
    // These are producer certificates, not checker measurements. Keep checker
    // summary fields unset until independent measurements are available.
    let upper = plan
        .contacts()
        .iter()
        .map(|c| c.patch().upper())
        .fold(0., f64::max);
    Ok(Built {
        bytes,
        report: Some(data),
        producer_patch_upper: Some(upper),
    })
}

/// Original fixture inputs remain transport tests until their stage changes.
/// Only the hash-bound approved Round 25 takes the supported construction path.
fn build_bytes(cfg: &Config, named: &Named, m: &mut ModelRun) -> Result<Built, Failure> {
    // Parse and hash the same read. Re-reading for provenance could bind a
    // replaced input's hash to geometry from the preceding read.
    let (mesh, sha256) = timed(&mut m.steps, "read", || {
        let bytes = std::fs::read(&named.input).map_err(|e| ("read", e.to_string()))?;
        let mesh = plinth_geom::parse_stl(&bytes).map_err(|e| ("read", e.to_string()))?;
        Ok::<_, Failure>((mesh, crate::sha256::hex(&bytes)))
    })?;
    m.triangles = mesh.triangle_count();
    // Approval is the content hash alone, never a file name. Review mode builds supports
    // for every readable input and approves none.
    if cfg.review.is_some() || sha256 == APPROVED_ROUND25 {
        m.supported = true;
        // A failed search is a planning blocker too, even before ReportData exists.
        // Successful construction replaces this provisional blocker from its data.
        m.planning_blockers = true;
        if cfg.review.is_some_and(|r| r.hollow_fixture) {
            let hollowed = timed(&mut m.steps, "hollow", || {
                plinth_geom::hollow::hollow(&mesh)
            })
            .map_err(|e| ("hollow", e))?;
            let built = supported(cfg, named, (&hollowed.mesh, &sha256), m)?;
            m.hollow = Some(
                timed(&mut m.steps, "hollow-measure-written", || {
                    crate::hollow::written(&mesh, &hollowed, &built.bytes)
                })
                .map_err(|e| ("hollow-measure-written", e))?,
            );
            return Ok(built);
        }
        return supported(cfg, named, (&mesh, &sha256), m);
    }
    let (project, parallel) = timed(&mut m.steps, "convert", || {
        transport_project(&named.stem, &mesh, cfg.max_threads)
    })
    .map_err(|e| ("convert", e.to_string()))?;
    m.parallel = Some(parallel);
    drop(mesh);
    timed(&mut m.steps, "serialize", || project.to_bytes())
        .map(|bytes| Built {
            bytes,
            report: None,
            producer_patch_upper: None,
        })
        .map_err(|e| ("serialize", e.to_string()))
}

fn io_message(what: &str, path: &Path, e: &std::io::Error) -> String {
    if e.kind() == ErrorKind::AlreadyExists {
        format!(
            "{what} {} already exists; it is never overwritten",
            path.display()
        )
    } else {
        format!("{what} {}: {e}", path.display())
    }
}

fn checker_refusal(named: &Named, checked: Checked) -> Outcome {
    let violated = checked.ids_with("violation");
    let message = format!(
        "{}: the checker rejected the file (violated rules: {}; structure: {}); nothing was published",
        named.input.display(),
        if violated.is_empty() {
            "none".to_owned()
        } else {
            violated.join(", ")
        },
        if checked.structure_notes.is_empty() {
            "no notes".to_owned()
        } else {
            checked.structure_notes.join("; ")
        }
    );
    Outcome::Failed {
        stage: "check",
        message,
        checked: Some(checked),
    }
}

/// Write the temp file, run the checker on it, publish on success. On any failure the temp
/// file is removed (except for a simulated crash, which leaves it as a dead process would).
fn write_check_publish(
    cfg: &Config,
    named: &Named,
    bytes: &[u8],
    fault: Fault,
    review: Option<(&mut rd::ReportData, Option<f64>)>,
    m: &mut ModelRun,
) -> Result<Outcome, Failure> {
    let planning_blockers = m.planning_blockers;
    let final_name = format!("{}.chitubox", named.stem);
    let dest = cfg.out_dir.join(&final_name);
    let temp = publish::temp_path(&cfg.out_dir, "model");
    timed(&mut m.steps, "write", || {
        publish::write_temp(&temp, bytes, fault)
    })
    .map_err(|e| ("write", io_message("cannot write", &temp, &e)))?;
    let report = publish::temp_path(&cfg.out_dir, "check");
    let job = check::Job {
        checker: &cfg.checker,
        limits: &cfg.limits,
        chitubox: &temp,
        stl: &named.input,
        report: &report,
        written: bytes,
        declared_hollow: m.hollow.is_some(),
    };
    let (checked, raw) = timed(&mut m.steps, "check", || match cfg.review {
        Some(r) => check::run_bounded(&job, r.checker_timeout).map(|(c, raw)| (c, Some(raw))),
        None => check::run(&job).map(|c| (c, None)),
    })
    .map_err(|e| {
        let _ = std::fs::remove_file(&temp);
        ("check", e)
    })?;
    // Review mode keeps a file the checker found a rule violated in, because the owner
    // wants to look at it. A file the checker could not read or whose structure is damaged
    // is malformed and stays a refusal.
    let retained = cfg.review.is_some()
        && !checked.verdict.publishable()
        && checked.structure_notes.is_empty();
    if !checked.verdict.publishable() && !retained {
        let _ = std::fs::remove_file(&temp);
        return Ok(checker_refusal(named, checked));
    }
    let marked;
    let named = if retained {
        if let Some(r) = m.review.as_mut() {
            r.checker_failed = true;
        }
        marked = Named {
            input: named.input.clone(),
            stem: format!("{}.checker-failed", named.stem),
        };
        m.stem.clone_from(&marked.stem);
        &marked
    } else {
        named
    };
    let mut blockers = checked.candidate_blockers();
    if planning_blockers {
        blockers.push("placement, load path or early-patch capacity remains unresolved".into());
    }
    if cfg.candidate && !candidate_allowed(true, &checked, planning_blockers) {
        if blockers.is_empty() {
            blockers.push("incomplete measured rule set".into());
        }
        let _ = std::fs::remove_file(&temp);
        let message = format!(
            "{}: not offered as a print candidate; nothing was published ({})",
            named.input.display(),
            blockers.join("; ")
        );
        return Ok(Outcome::Failed {
            stage: "candidate",
            message,
            checked: Some(checked),
        });
    }
    if fault == Fault::CrashBeforePublish {
        return Err((
            "publish",
            format!(
                "simulated crash before publish; {} was left behind",
                temp.display()
            ),
        ));
    }
    if let Some((data, upper)) = review {
        publish_supported(cfg, named, bytes, &temp, (data, upper, checked, raw), m)
    } else {
        timed(&mut m.steps, "publish", || publish::publish(&temp, &dest))
            .map_err(|e| ("publish", io_message("cannot publish", &dest, &e)))?;
        Ok(Outcome::Published {
            path: dest,
            bytes: bytes.len() as u64,
            checked,
        })
    }
}

fn publish_supported(
    cfg: &Config,
    named: &Named,
    bytes: &[u8],
    temp: &Path,
    review: (&mut rd::ReportData, Option<f64>, Checked, Option<Vec<u8>>),
    m: &mut ModelRun,
) -> Result<Outcome, Failure> {
    let (data, upper, checked, raw_checker_report) = review;
    let staging = publish::temp_path(&cfg.out_dir, "bundle");
    let bundle = cfg.out_dir.join(format!("{}.supported", named.stem));
    let final_name = format!("{}.chitubox", named.stem);
    std::fs::create_dir(&staging).map_err(|e| {
        let _ = std::fs::remove_file(temp);
        ("report", e.to_string())
    })?;
    let mut staged_cfg = cfg.clone();
    staged_cfg.out_dir = staging.clone();
    staged_cfg.candidate = candidate_allowed(cfg.candidate, &checked, m.planning_blockers);
    let outcome = Outcome::Published {
        path: bundle.join(&final_name),
        bytes: bytes.len() as u64,
        checked,
    };
    data.run.steps = m
        .steps
        .iter()
        .map(|s| rd::StepTime {
            name: s.name.into(),
            seconds: s.secs,
        })
        .collect();
    data.run.peak_memory_bytes = mem::peak().map_or(0, mem::Peak::job);
    let result = (|| {
        std::fs::rename(temp, staging.join(&final_name)).map_err(|e| ("write", e.to_string()))?;
        let status = m.review.as_ref().map(review_status);
        timed(&mut m.steps, "report", || {
            write_review(
                &staged_cfg,
                named,
                bytes,
                (data, upper),
                (&outcome, status.as_deref()),
            )?;
            // Review mode keeps the checker's own report beside the file, whatever it said.
            raw_checker_report.as_ref().map_or(Ok(()), |raw| {
                let path = staging.join(format!("{}.checker-report.json", named.stem));
                std::fs::write(&path, raw)
                    .map_err(|e| ("report", io_message("cannot write", &path, &e)))
            })
        })?;
        timed(&mut m.steps, "publish", || {
            publish::publish_bundle(&staging, &bundle)
        })
        .map_err(|e| ("publish", io_message("cannot publish", &bundle, &e)))
    })();
    if result.is_err() {
        let _ = std::fs::remove_dir_all(&staging);
        let _ = std::fs::remove_file(temp);
    }
    result?;
    Ok(outcome)
}

fn review_scene(model: &plinth_chitubox::Model) -> plinth_render::Scene {
    let min_z = model
        .mesh
        .iter()
        .flatten()
        .map(|v| v[2])
        .fold(f32::INFINITY, f32::min);
    let mut scene = crate::report::scene_from_model(model);
    for tri in scene
        .model
        .iter_mut()
        .chain(scene.parts.iter_mut().flat_map(|p| p.tris.iter_mut()))
    {
        for v in tri {
            // Replay uses local XY / plate Z. Transform model and supports
            // together to that frame; translating only their markers is wrong.
            v[0] -= model.translate[0];
            v[1] -= model.translate[1];
            v[2] -= min_z;
        }
    }
    scene
}

fn write_review(
    cfg: &Config,
    named: &Named,
    bytes: &[u8],
    (data, upper): (&rd::ReportData, Option<f64>),
    (outcome, review_status): (&Outcome, Option<&str>),
) -> Result<(), Failure> {
    use crate::json::Json;
    let project = plinth_chitubox::parse(bytes).map_err(|e| ("report", e.to_string()))?;
    let model = project
        .models
        .first()
        .ok_or(("report", "emitted model missing".into()))?;
    let mut scene = review_scene(model);
    for (i, part) in scene.parts.iter_mut().enumerate().take(
        data.supports.contacts.hidden_underside as usize * 3
            + data.supports.contacts.partly_visible_rim as usize * 3,
    ) {
        part.contact = Some((i / 3) as u32);
    }
    let checked = match outcome {
        Outcome::Published { checked, .. } => Some(checked),
        Outcome::Failed { checked, .. } => checked.as_ref(),
    };
    let rules = checked.map_or_else(Vec::new, |c| {
        c.rules
            .iter()
            .map(|r| {
                Json::obj([
                    ("id", Json::str(&r.id)),
                    ("status", Json::str(&r.status)),
                    ("reason", Json::str(&r.reason)),
                ])
            })
            .collect()
    });
    let tips: Vec<_> = model
        .supports
        .as_ref()
        .into_iter()
        .flat_map(|s| s.records.iter())
        .filter(|r| {
            use plinth_chitubox::RecordType as T;
            r.record_type == T::TIP_BALL
        })
        .map(|r| Json::Num(f64::from(r.r1)))
        .collect();
    let candidate = checked.is_some_and(|c| candidate_allowed(cfg.candidate, c, false));
    let status = review_status.unwrap_or(if candidate {
        "print candidate"
    } else {
        "not a print candidate"
    });
    let mut text = format!(
        "Supported output: {status}. Modelled loads are not independent checker acceptance.\n"
    );
    if review_status.is_some() {
        text.push_str("Review mode: supports built to look at in ChituBox, for a pose given as drawn. Nothing here says this file can be printed.\n");
    }
    if let Some(c) = checked {
        for r in &c.rules {
            text.push_str(&format!("{}: {} — {}\n", r.id, r.status, r.reason));
        }
    }
    text.push_str(&format!(
        "Producer H1 upper bound: {upper:?} mm². This is not a checker measurement.\n"
    ));
    text.push_str(&crate::report::fr21_text(data));
    let json = Json::obj([
        ("status", Json::str(status)),
        ("sha256", Json::str(&data.identity.generated_sha256)),
        ("checker_rules", Json::Arr(rules)),
        (
            "producer_patch_upper_mm2",
            upper.map_or(Json::Null, Json::Num),
        ),
        ("tip_ball_radii_mm", Json::Arr(tips)),
        ("fr21", crate::report::fr21_json(data)),
    ]);
    let write = |path: PathBuf, contents: &[u8]| -> Result<(), Failure> {
        use std::io::Write as _;
        let mut f = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
            .map_err(|e| ("report", io_message("cannot write", &path, &e)))?;
        f.write_all(contents).map_err(|e| ("report", e.to_string()))
    };
    write(
        cfg.out_dir.join(format!("{}.fr21.txt", named.stem)),
        text.as_bytes(),
    )?;
    write(
        cfg.out_dir.join(format!("{}.fr21.json", named.stem)),
        json.render().as_bytes(),
    )?;
    let pictures = plinth_render::render_pictures(&scene, data);
    plinth_render::write_all(
        &cfg.out_dir.join(format!("{}.pictures", named.stem)),
        &pictures,
    )
    .map_err(|e| ("report", e.to_string()))?;
    Ok(())
}

fn run_model(cfg: &Config, named: &Named, fault: Fault) -> ModelRun {
    // Review files carry the label in their names, so none can be mistaken for a candidate.
    let labelled;
    let named = if cfg.review.is_some() {
        labelled = Named {
            input: named.input.clone(),
            stem: format!("{}.{REVIEW_FILE_TAG}", named.stem),
        };
        &labelled
    } else {
        named
    };
    let mut m = ModelRun {
        input: named.input.clone(),
        stem: named.stem.clone(),
        triangles: 0,
        steps: Vec::new(),
        parallel: None,
        supported: false,
        planning_blockers: false,
        fr21: None,
        hollow: None,
        review: cfg.review.map(|_| ReviewRun::default()),
        outcome: Outcome::Failed {
            stage: "read",
            message: String::new(),
            checked: None,
        },
    };
    let result = build_bytes(cfg, named, &mut m).and_then(|mut built| {
        let stopped;
        let named = if m.review.is_some_and(|r| r.stopped_at_budget.is_some()) {
            stopped = Named {
                input: named.input.clone(),
                stem: format!("{}.stopped-at-time-budget", named.stem),
            };
            m.stem.clone_from(&stopped.stem);
            &stopped
        } else {
            named
        };
        let blocked = built.report.as_ref().is_some_and(|d| {
            d.warnings
                .iter()
                .any(|w| w.class != load::WarningClass::Information)
        });
        m.planning_blockers = blocked;
        let review = built
            .report
            .as_mut()
            .map(|data| (data, built.producer_patch_upper));
        let result = write_check_publish(cfg, named, &built.bytes, fault, review, &mut m);
        m.fr21 = built.report.as_ref().map(crate::report::fr21_json);
        result
    });
    m.outcome = result.unwrap_or_else(|(stage, message)| Outcome::Failed {
        stage,
        message: format!("{}: {message}", named.input.display()),
        checked: None,
    });
    m
}

/// Check what a run needs before touching any input.
fn preflight(cfg: &Config) -> Result<(), String> {
    if cfg.review.is_some() && cfg.candidate {
        return Err(
            "review mode cannot be combined with candidate mode: review files are never print candidates".into(),
        );
    }
    for (what, path) in [
        ("the checker", &cfg.checker),
        ("the limits file", &cfg.limits),
    ] {
        if !path.is_file() {
            let hint = if what == "the checker" {
                " (build it: cd checker && cargo build --locked)"
            } else {
                ""
            };
            return Err(format!("{what} not found: {}{hint}", path.display()));
        }
    }
    std::fs::create_dir_all(&cfg.out_dir)
        .map_err(|e| format!("cannot create {}: {e}", cfg.out_dir.display()))
}

/// Run every input. `progress` is called once per finished model.
///
/// # Errors
/// A setup message (missing checker, limits file or output folder) before any input is read.
pub fn run(
    cfg: &Config,
    faults: Faults,
    progress: &mut dyn FnMut(&ModelRun),
) -> Result<Run, String> {
    preflight(cfg)?;
    let started = Instant::now();
    let mut models = Vec::new();
    for (i, named) in names::assign(&cfg.inputs).iter().enumerate() {
        let m = run_model(cfg, named, faults.for_model(i));
        progress(&m);
        models.push(m);
    }
    Ok(Run {
        models,
        total_secs: started.elapsed().as_secs_f64(),
        peak: mem::peak(),
    })
}
