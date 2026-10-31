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
use std::time::Instant;

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
            design_ratio: 12.,
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
            message: format!("Column at contact {contact} has no admitted 45-degree X-pair to a carrier 4–15 mm away. Add bracing at this picture marker; stability is unassessed."),
        });
    }
    if proposal.termination != place::Termination::Satisfied {
        data.warnings.push(load::LoadWarning { class:load::WarningClass::Intervention,
            kind:"construction plan unresolved".into(), layers:vec![], contacts:vec![],
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
        })
        .collect();
    if !partial.is_empty() {
        data.warnings.push(load::LoadWarning { class:load::WarningClass::Intervention,
            kind:"early patch growth not certified".into(),
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

fn supported(cfg: &Config, mesh: &Mesh, m: &mut ModelRun) -> Result<Built, Failure> {
    let fail = |e: c::Reject| {
        (
            "construct",
            format!("{e:?}; no partially admitted file was emitted"),
        )
    };
    let limits_text =
        std::fs::read_to_string(&cfg.limits).map_err(|e| ("construct", e.to_string()))?;
    let limits = c::Limits::parse(&limits_text).map_err(fail)?;
    let keep = c::KeepUpRecord {
        rotation_x_deg: 115.,
        approved_sha256: APPROVED_ROUND25.into(),
    };
    let model = c::PosedModel::new(mesh, &keep, &limits).map_err(|e| ("pose", format!("{e:?}")))?;
    let layers = timed(&mut m.steps, "slice", || {
        model_layers(&model, cfg.max_threads)
    })?;
    let mut builder = c::PlanBuilder::new(
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
        place::plan_contacts(&builder, &layers, &place::PlanSettings::default())
    });
    let mut answers = Vec::new();
    builder.probe(&proposal.candidates, &mut answers);
    let diagnostic = proposal_diagnostic(&proposal, &answers);
    if !proposal.routed {
        return Err((
            "plan",
            format!(
                "route preview rejected before contact commit: {:?}; {diagnostic}",
                proposal.routing_blocker
            ),
        ));
    }
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
    let project = timed(&mut m.steps, "emit", || {
        plinth_emit::constructed::project(&plan)
    })
    .map_err(|e| ("emit", format!("{e:?}")))?;
    let bytes = timed(&mut m.steps, "serialize", || project.to_bytes())
        .map_err(|e| ("serialize", e.to_string()))?;
    let mut data = collect_report_data(&plan, layers, proposal);
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
    let (mesh, approved) = timed(&mut m.steps, "read", || {
        let bytes = std::fs::read(&named.input).map_err(|e| ("read", e.to_string()))?;
        let mesh = plinth_geom::parse_stl(&bytes).map_err(|e| ("read", e.to_string()))?;
        Ok::<_, Failure>((mesh, crate::sha256::hex(&bytes) == APPROVED_ROUND25))
    })?;
    m.triangles = mesh.triangle_count();
    if approved {
        m.supported = true;
        // A failed search is a planning blocker too, even before ReportData exists.
        // Successful construction replaces this provisional blocker from its data.
        m.planning_blockers = true;
        return supported(cfg, &mesh, m);
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
    };
    let checked = timed(&mut m.steps, "check", || check::run(&job)).map_err(|e| {
        let _ = std::fs::remove_file(&temp);
        ("check", e)
    })?;
    if !checked.verdict.publishable() {
        let _ = std::fs::remove_file(&temp);
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
        return Ok(Outcome::Failed {
            stage: "check",
            message,
            checked: Some(checked),
        });
    }
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
        publish_supported(cfg, named, bytes, &temp, (data, upper, checked), m)
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
    review: (&mut rd::ReportData, Option<f64>, Checked),
    m: &mut ModelRun,
) -> Result<Outcome, Failure> {
    let (data, upper, checked) = review;
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
        timed(&mut m.steps, "report", || {
            write_review(&staged_cfg, named, bytes, data, upper, &outcome)
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

fn write_review(
    cfg: &Config,
    named: &Named,
    bytes: &[u8],
    data: &rd::ReportData,
    upper: Option<f64>,
    outcome: &Outcome,
) -> Result<(), Failure> {
    use crate::json::Json;
    let project = plinth_chitubox::parse(bytes).map_err(|e| ("report", e.to_string()))?;
    let model = project
        .models
        .first()
        .ok_or(("report", "emitted model missing".into()))?;
    // scene_from_model accepts a plate-relative mesh. Native Z is instead relative
    // to the model's minimum. Correct that frame once on the decoded scene.
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
            v[2] -= min_z;
        }
    }
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
        .filter(|r| r.record_type == plinth_chitubox::RecordType::TIP_BALL)
        .map(|r| Json::Num(f64::from(r.r1)))
        .collect();
    let candidate = checked.is_some_and(|c| candidate_allowed(cfg.candidate, c, false));
    let status = if candidate {
        "print candidate"
    } else {
        "not a print candidate"
    };
    let mut text = format!(
        "Supported output: {status}. Modelled loads are not independent checker acceptance.\n"
    );
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
    let mut m = ModelRun {
        input: named.input.clone(),
        stem: named.stem.clone(),
        triangles: 0,
        steps: Vec::new(),
        parallel: None,
        supported: false,
        planning_blockers: false,
        outcome: Outcome::Failed {
            stage: "read",
            message: String::new(),
            checked: None,
        },
    };
    let result = build_bytes(cfg, named, &mut m).and_then(|mut built| {
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
        write_check_publish(cfg, named, &built.bytes, fault, review, &mut m)
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
