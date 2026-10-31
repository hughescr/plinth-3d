//! The JSON run report, written by hand (no serde). Keys keep a fixed order.
//!
//! Two parts, so a run can be compared without its timings: `models` and `job` hold the
//! results and are the same at any thread count; `run` holds the settings, time per step and
//! peak memory, which differ from run to run (design section 8: timings never mix with the
//! measured part).

use crate::check::Checked;
use crate::json::Json;
use crate::mem::Peak;
use crate::run::{Config, ModelRun, Outcome, Run};
use plinth_emit::TRANSPORT_LABEL;

/// Report format version.
pub const REPORT_VERSION: u64 = 1;

fn path(p: &std::path::Path) -> Json {
    Json::str(p.display().to_string())
}

fn count(n: usize) -> Json {
    Json::Uint(n as u64)
}

fn checker_json(c: Option<&Checked>) -> Json {
    c.map_or(Json::Null, Checked::to_json)
}

/// What a file is called in the report: a transport file, or (candidate mode, published only
/// after the checker passed it with no applicable rule unverified or violated) a print
/// candidate.
fn kind_of(candidate: bool, supported: bool) -> &'static str {
    if supported {
        "supported"
    } else if candidate {
        "candidate"
    } else {
        "transport"
    }
}

fn label_of(candidate: bool) -> &'static str {
    if candidate {
        "print candidate: the checker passed every applicable rule"
    } else {
        TRANSPORT_LABEL
    }
}

fn model_json(candidate: bool, m: &ModelRun) -> Json {
    let (status, output, bytes, stage, error, checked) = match &m.outcome {
        Outcome::Published {
            path: p,
            bytes,
            checked,
        } => (
            "published",
            path(p),
            Json::Uint(*bytes),
            Json::Null,
            Json::Null,
            Some(checked),
        ),
        Outcome::Failed {
            stage,
            message,
            checked,
        } => (
            "failed",
            Json::Null,
            Json::Null,
            Json::str(*stage),
            Json::str(message),
            checked.as_ref(),
        ),
    };
    let mut blockers = checked.map_or_else(Vec::new, Checked::candidate_blockers);
    if m.planning_blockers {
        blockers.push("placement or load measurements unresolved".into());
    }
    let is_candidate = status == "published"
        && checked
            .is_some_and(|c| crate::run::candidate_allowed(candidate, c, m.planning_blockers));
    Json::obj([
        ("input", path(&m.input)),
        ("output_name", Json::str(format!("{}.chitubox", m.stem))),
        ("status", Json::str(status)),
        ("kind", Json::str(kind_of(candidate, m.supported))),
        ("print_candidate", Json::Bool(is_candidate)),
        (
            "label",
            Json::str(if m.supported && !is_candidate {
                "supported, not a print candidate"
            } else {
                label_of(is_candidate)
            }),
        ),
        (
            "candidate_blockers",
            Json::Arr(blockers.iter().map(Json::str).collect()),
        ),
        ("triangles", count(m.triangles)),
        ("output", output),
        ("bytes", bytes),
        ("failed_stage", stage),
        ("error", error),
        ("checker", checker_json(checked)),
    ])
}

fn peak_json(peak: &Result<Peak, String>) -> Json {
    match peak {
        Ok(p) => Json::obj([
            ("job_bytes", Json::Uint(p.job())),
            ("plinth_process_bytes", Json::Uint(p.own)),
            ("largest_child_bytes", Json::Uint(p.largest_child)),
            (
                "basis",
                Json::str(
                    "lifetime high-water marks (getrusage) of the plinth process and of the \
                     largest finished child (the checker), added as if they coexisted; a job \
                     figure, not one model's peak; it reports, it does not prevent",
                ),
            ),
        ]),
        Err(e) => Json::obj([("error", Json::str(e))]),
    }
}

fn steps_json(m: &ModelRun) -> Json {
    let parallel = m.parallel.map_or(Json::Null, |p| {
        Json::obj([
            ("tasks", count(p.tasks)),
            ("threads_ran", count(p.threads_ran)),
        ])
    });
    let steps = m
        .steps
        .iter()
        .map(|s| Json::obj([("step", Json::str(s.name)), ("seconds", Json::Num(s.secs))]))
        .collect();
    Json::obj([
        ("input", path(&m.input)),
        ("parallel_conversion", parallel),
        ("steps", Json::Arr(steps)),
    ])
}

/// The whole report for a finished run.
#[must_use]
pub fn to_json(cfg: &Config, run: &Run) -> Json {
    let failed = run.models.len() - run.published();
    let supported = run.models.iter().any(|m| m.supported);
    let candidate=cfg.candidate && !run.models.is_empty() && run.models.iter().all(|m|matches!(&m.outcome,
        Outcome::Published { checked,.. } if crate::run::candidate_allowed(true,checked,m.planning_blockers)));
    Json::obj([
        ("report_version", Json::Uint(REPORT_VERSION)),
        (
            "tool",
            Json::obj([
                ("name", Json::str("plinth")),
                ("version", Json::str(env!("CARGO_PKG_VERSION"))),
            ]),
        ),
        ("kind", Json::str(kind_of(cfg.candidate, supported))),
        ("print_candidate", Json::Bool(candidate)),
        (
            "label",
            Json::str(if supported && !candidate {
                "supported, not a print candidate"
            } else {
                label_of(candidate)
            }),
        ),
        (
            "models",
            Json::Arr(
                run.models
                    .iter()
                    .map(|m| model_json(cfg.candidate, m))
                    .collect(),
            ),
        ),
        (
            "job",
            Json::obj([
                ("models", count(run.models.len())),
                ("published", count(run.published())),
                ("failed", count(failed)),
            ]),
        ),
        (
            "run",
            Json::obj([
                ("max_threads", count(cfg.max_threads)),
                ("checker", path(&cfg.checker)),
                ("limits", path(&cfg.limits)),
                ("total_seconds", Json::Num(run.total_secs)),
                ("peak_memory", peak_json(&run.peak)),
                (
                    "models",
                    Json::Arr(run.models.iter().map(steps_json).collect()),
                ),
            ]),
        ),
    ])
}

/// The section of a neck of the owner's Light tip, mm² (design section 3: neck radius 0.10).
pub const LIGHT_NECK_MM2: f64 = 0.031;

/// Why the tributary ratio is not a load bound (design sections 4.3 and 4.7).
pub const TRIBUTARY_LABEL: &str = "Tributary ratio: a sizing heuristic under one sharing \
    assumption, not a bound on the force in any section.";

/// The uncertainty statement (design section 3).
pub const UNCERTAINTY: [&str; 4] = [
    "The snapping ratio of about 25 was measured on flat horizontal caps 2 to 12.5 mm across \
     with vertical arms 4 to 6 mm long and one resin, and fails at the narrowest section above \
     the contact. It was not measured on thin tilted strips, or on angled, bent or merged \
     members, or off-axis loads.",
    "The design ratio of 12 is the owner's starting policy (P5), not a claim that 12 is safe or \
     needed. Heads were lost at 9 on one plate; another way of measuring gives 11 to 14.",
    "Load sharing among up to 6 arms within about 14 mm under a stiff part was measured (P6). \
     More arms, wider spans and off-centre loads were not.",
    "The load is taken as proportional to the full cured layer area, carried over from caps to \
     strips. A Light-sized tip at 6 mm spacing would score a ratio of about 100 to 300 at the \
     lowest layers; if such layouts print reliably on tilted bases, this convention overstates \
     the load on strips by up to ten times. That is a hypothesis. The side-by-side print \
     (PRD section 8.4) settles it.",
];

/// The assumptions table (design section 4.5): number, assumption, check.
pub const ASSUMPTIONS: [(&str, &str, &str); 12] = [
    (
        "A1",
        "Peel is the only load (P3).",
        "Owner experience; the first print, per region.",
    ),
    (
        "A2",
        "Load is proportional to the full cured layer area, carried over from caps to strips.",
        "Not testable from the file. The side-by-side print. The report shows the ratio a Light-sized tip would have at each site.",
    ),
    (
        "A3",
        "One layer peels at a time.",
        "Matches how the plates were designed.",
    ),
    (
        "A4",
        "The peel area is the slab envelope.",
        "The mid-plane result is reported beside it.",
    ),
    (
        "A5",
        "Failure is at the narrowest perpendicular section; the patch is a section (P7).",
        "The owner reads the breaks on the first print.",
    ),
    (
        "A6",
        "Tributary sharing sizes contacts; it is not a reaction bound.",
        "All four sharing results and the moment-balance test are classified. The owner reports which contacts failed.",
    ),
    (
        "A7",
        "A contact carries only the patch printed so far.",
        "Geometric; tested on synthetic shapes. The printed part of the patch is a disc in the plane the patch's height range implies.",
    ),
    (
        "A8",
        "Loads add at joints; braces take no model load.",
        "Not a bound for individual columns. Brace self-peel is counted; brace bending and redistribution are an intervention.",
    ),
    (
        "A9",
        "Angled heads behave like vertical arms.",
        "Unmeasured. Each head's angle, length and offset is reported.",
    ),
    (
        "A10",
        "Faces at 65 degrees hold themselves.",
        "ChituBox's overhang check on the output; the print.",
    ),
    (
        "A11",
        "No print-order cups on Round 25.",
        "Computed on the layer rasters.",
    ),
    (
        "A12",
        "Contacts within 3.0 mm cover a sub-45 degree region.",
        "The print; ChituBox's overhang check.",
    ),
];

fn num(v: f64) -> Json {
    Json::Num(v)
}

fn class_name(c: plinth_support::load::WarningClass) -> &'static str {
    use plinth_support::load::WarningClass as W;
    match c {
        W::Unprintable => "unprintable as is",
        W::Intervention => "intervention",
        W::Information => "information",
    }
}

fn warning_json(w: &plinth_support::load::LoadWarning) -> Json {
    Json::obj([
        ("class", Json::str(class_name(w.class))),
        ("kind", Json::str(&w.kind)),
        (
            "layers",
            Json::Arr(w.layers.iter().map(|&l| Json::Uint(u64::from(l))).collect()),
        ),
        (
            "contacts",
            Json::Arr(
                w.contacts
                    .iter()
                    .map(|&c| Json::Uint(u64::from(c)))
                    .collect(),
            ),
        ),
        ("message", Json::str(&w.message)),
    ])
}

fn part_name(k: plinth_support::load::PartKind) -> &'static str {
    use plinth_support::load::PartKind as P;
    match k {
        P::Patch => "contact patch",
        P::Neck => "neck",
        P::TipCone => "tip cone",
        P::JointThroat => "joint throat",
        P::Column => "column",
        P::Flare => "base flare",
        P::Brace => "brace",
    }
}

/// The plate height of the bottom of model layer `k`, mm.
fn plate_z(
    data: &plinth_support::report_data::ReportData,
    ld: &plinth_support::report_data::LoadData,
    k: u32,
) -> f64 {
    let _ = ld;
    data.process.lift_mm + f64::from(k) * data.process.layer_height_mm
}

fn ranked_sections(
    data: &plinth_support::report_data::ReportData,
    ld: &plinth_support::report_data::LoadData,
) -> Vec<Json> {
    let mut s: Vec<_> = ld.detail.sections.iter().collect();
    s.sort_by(|a, b| {
        b.tributary
            .0
            .total_cmp(&a.tributary.0)
            .then(a.chain.cmp(&b.chain))
            .then(a.part.cmp(&b.part))
    });
    s.iter()
        .take(10)
        .enumerate()
        .map(|(i, d)| {
            let load = d.tributary.0 * d.area_mm2;
            Json::obj([
                ("rank", count(i + 1)),
                ("contact", Json::Uint(u64::from(d.chain))),
                ("part", Json::str(part_name(d.kind))),
                (
                    "native_record",
                    d.record.map_or(Json::Null, |r| Json::Uint(u64::from(r))),
                ),
                ("worst_layer", Json::Uint(u64::from(d.tributary.1))),
                ("plate_z_mm", num(plate_z(data, ld, d.tributary.1))),
                ("section_area_mm2", num(d.area_mm2)),
                ("load_area_mm2", num(load)),
                ("tributary_ratio", num(d.tributary.0)),
                ("tributary_label", Json::str(TRIBUTARY_LABEL)),
                ("additive_ratio", num(d.additive.0)),
                ("equal_share_ratio", num(d.equal_share.0)),
                ("rigid_lever_ratio", num(d.rigid_lever.0)),
                ("light_tip_ratio_at_this_load", num(load / LIGHT_NECK_MM2)),
                ("picture", Json::str(format!("7c, contact {}", d.chain))),
            ])
        })
        .collect()
}

fn load_json(
    data: &plinth_support::report_data::ReportData,
    ld: &plinth_support::report_data::LoadData,
) -> Json {
    let peak = ld.detail.peak.map_or(Json::Null, |p| {
        Json::obj([
            ("layer", Json::Uint(u64::from(p.layer))),
            ("plate_z_mm", num(plate_z(data, ld, p.layer))),
            ("peel_area_mm2", num(p.area_mm2)),
            ("capacity_mm2", num(p.capacity_mm2)),
            ("active_contacts", Json::Uint(u64::from(p.active_contacts))),
            ("note", Json::str("capacity is the design ratio times the sum of the active limiting sections, over all components")),
        ])
    });
    let s = &ld.sensitivity;
    Json::obj([
        ("design_ratio_p5", num(ld.detail.report.design_ratio)),
        ("peak_layer", peak),
        ("largest_ratios", Json::Arr(ranked_sections(data, ld))),
        (
            "sensitivity",
            Json::obj([
                ("largest_tributary_ratio_envelope", num(s.envelope_largest)),
                ("largest_tributary_ratio_midplane", num(s.midplane_largest)),
                (
                    "contacts_lower_bound_at_p5_18",
                    s.contacts_lower_bound_p5_18
                        .map_or(Json::Null, |n| Json::Uint(u64::from(n))),
                ),
                (
                    "contacts_lower_bound_note",
                    Json::str(
                        "peak layer area over 18 times the largest limiting section in this layout; not the planner's count, which needs the planner",
                    ),
                ),
            ]),
        ),
        (
            "sharing_models",
            Json::str(
                "tributary (sizes contacts), additive group ratio, equal share, rigid lever; the last three are reported beside it and any of them above the design ratio is an intervention (above 25 for the additive ratio, unprintable)",
            ),
        ),
    ])
}

fn structure_json(ld: &plinth_support::report_data::LoadData) -> Json {
    let islands: Vec<Json> = ld
        .structure
        .islands
        .iter()
        .map(|i| {
            Json::obj([
                ("component", Json::Uint(u64::from(i.component))),
                ("first_layer", Json::Uint(u64::from(i.first_layer))),
                (
                    "first_xy_mm",
                    Json::Arr(vec![num(i.first_xy[0]), num(i.first_xy[1])]),
                ),
                ("first_area_mm2", num(i.first_area_mm2)),
                ("peak_area_mm2_before_joining", num(i.peak_area_mm2)),
                ("width_mm", num(i.width_mm)),
                ("height_mm", num(i.height_mm)),
                ("width_px", Json::Uint(u64::from(i.width_px))),
                ("height_px", Json::Uint(u64::from(i.height_px))),
                ("layers_alone", Json::Uint(u64::from(i.layers_alone))),
                (
                    "joins",
                    i.merged.map_or(Json::Null, |m| {
                        Json::obj([
                            ("into", Json::Uint(u64::from(m.0))),
                            ("layer", Json::Uint(u64::from(m.1))),
                        ])
                    }),
                ),
                ("p18_exempt", Json::Bool(i.exempt)),
            ])
        })
        .collect();
    let cups: Vec<Json> = ld
        .structure
        .cups
        .iter()
        .map(|c| {
            Json::obj([
                ("layer", Json::Uint(u64::from(c.layer))),
                ("area_mm2_pixel_count", num(c.area_mm2)),
                (
                    "xy_mm",
                    Json::Arr(vec![num(c.centre_xy[0]), num(c.centre_xy[1])]),
                ),
            ])
        })
        .collect();
    let n = &ld.needs_summary;
    Json::obj([
        ("islands", Json::Arr(islands)),
        ("cups", Json::Arr(cups)),
        ("reach_beyond_p8", count(ld.structure.reach.len())),
        (
            "needs_support",
            Json::obj([
                ("samples", count(n.samples)),
                ("sub_45_surface_area_mm2", num(n.surface_mm2)),
                ("sub_45_plan_view_area_mm2", num(n.projected_mm2)),
                ("plan_view_below_30_mm2", num(n.below_30_projected_mm2)),
                ("plan_view_30_to_45_mm2", num(n.from_30_projected_mm2)),
                ("surface_area_hidden_mm2", num(n.class_surface_mm2[0])),
                (
                    "surface_area_partly_visible_mm2",
                    num(n.class_surface_mm2[1]),
                ),
                ("surface_area_top_mm2", num(n.class_surface_mm2[2])),
                (
                    "surface_area_class_undecided_mm2",
                    num(n.class_surface_mm2[3]),
                ),
                (
                    "cover_within_3_mm",
                    n.coverage.map_or(Json::Null, |c| {
                        Json::obj([
                            ("samples_needing_cover", count(c.needing)),
                            ("covered", count(c.covered)),
                            ("uncovered_area_mm2", num(c.uncovered_mm2)),
                        ])
                    }),
                ),
                (
                    "self_supporting",
                    Json::str(
                        "faces at or above 45 degrees, and new material within 2.13 mm of rooted material (P8)",
                    ),
                ),
            ]),
        ),
    ])
}

/// The FR-21 block of the report for one supported model (design `step11-construction.md`
/// sections 4.7 and 9). Without computed load data it says so and carries no ratio.
#[must_use]
pub fn fr21_json(data: &plinth_support::report_data::ReportData) -> Json {
    let p = &data.process;
    let process = Json::obj([
        ("design_ratio_p5", num(p.design_ratio)),
        ("h1_admission_mm2", num(p.h1_admission_mm2)),
        ("coverage_reach_mm", num(p.coverage_reach_mm)),
        ("overhang_deg", num(p.overhang_deg)),
        ("layer_height_mm", num(p.layer_height_mm)),
        ("layer_height_source", Json::str(&p.layer_height_source)),
        ("pixel_pitch_mm", Json::Arr(vec![num(0.014), num(0.019)])),
        ("lift_mm", num(p.lift_mm)),
    ]);
    let (load, structure) = match &data.load_data {
        Some(ld) => (load_json(data, ld), structure_json(ld)),
        None => (
            Json::obj([(
                "unverified",
                Json::str("the load model was not run on this file"),
            )]),
            Json::Null,
        ),
    };
    let warnings: Vec<Json> = data.warnings.iter().map(warning_json).collect();
    Json::obj([
        ("computed", Json::Bool(data.is_computed())),
        (
            "unverified",
            data.unverified.as_ref().map_or(Json::Null, Json::str),
        ),
        ("process", process),
        ("load", load),
        ("structure", structure),
        ("warnings", Json::Arr(warnings)),
        (
            "uncertainty",
            Json::Arr(UNCERTAINTY.iter().map(|s| Json::str(*s)).collect()),
        ),
        (
            "assumptions",
            Json::Arr(
                ASSUMPTIONS
                    .iter()
                    .map(|(n, a, c)| {
                        Json::obj([
                            ("id", Json::str(*n)),
                            ("assumption", Json::str(*a)),
                            ("check", Json::str(*c)),
                        ])
                    })
                    .collect(),
            ),
        ),
        (
            "contacts_by_site_class",
            Json::obj([
                (
                    "hidden_underside",
                    Json::Uint(u64::from(data.supports.contacts.hidden_underside)),
                ),
                (
                    "partly_visible_rim",
                    Json::Uint(u64::from(data.supports.contacts.partly_visible_rim)),
                ),
            ]),
        ),
    ])
}

/// The FR-21 report as plain text for the owner: short sentences, every number with its unit
/// and what it counts. Ratios are labelled as a sizing heuristic.
#[must_use]
pub fn fr21_text(data: &plinth_support::report_data::ReportData) -> String {
    use std::fmt::Write as _;
    let mut s = String::new();
    let p = &data.process;
    if let Some(why) = &data.unverified {
        let _ = writeln!(s, "UNVERIFIED: {why}. No number below is a result.");
    }
    let _ = writeln!(s, "PROCESS VALUES");
    let _ = writeln!(
        s,
        "  Design ratio (P5, owner): {} mm² of peel area per mm² of section.\n  H1 admission bound: {} mm² per contact patch.\n  Coverage reach: {} mm. Overhang rule: faces flatter than {} degrees.\n  Layer height: {} mm ({}). Pixel pitch: 0.014 x 0.019 mm. Lift: {} mm.",
        p.design_ratio,
        p.h1_admission_mm2,
        p.coverage_reach_mm,
        p.overhang_deg,
        p.layer_height_mm,
        p.layer_height_source,
        p.lift_mm
    );
    let Some(ld) = &data.load_data else {
        let _ = writeln!(s, "LOAD: the load model was not run on this file.");
        return s;
    };
    let _ = writeln!(
        s,
        "LOAD (one layer peels at a time; layers are never summed)"
    );
    if let Some(pk) = ld.detail.peak {
        let _ = writeln!(
            s,
            "  Largest peeling layer: layer {} (plate height {:.2} mm), {:.2} mm² of peel area. The active sections hold {:.2} mm² at the design ratio ({} contacts).",
            pk.layer,
            plate_z(data, ld, pk.layer),
            pk.area_mm2,
            pk.capacity_mm2,
            pk.active_contacts
        );
    }
    let _ = writeln!(s, "  {TRIBUTARY_LABEL}");
    let _ = writeln!(
        s,
        "  The 10 largest ratios (tributary, then additive / equal share / rigid lever; Light tip = same load on a Light-sized tip, 0.031 mm²):"
    );
    for r in ranked_sections(data, ld) {
        let g = |k: &str| {
            r.get(k)
                .and_then(|v| if let Json::Num(n) = v { Some(*n) } else { None })
                .unwrap_or(f64::NAN)
        };
        let _ = writeln!(
            s,
            "    #{} contact {} {} layer {}: {:.1}  ({:.1} / {:.1} / {:.1}; Light tip {:.0})",
            r.get("rank").and_then(Json::as_u64).unwrap_or(0),
            r.get("contact").and_then(Json::as_u64).unwrap_or(0),
            r.get("part").and_then(Json::as_str).unwrap_or("?"),
            r.get("worst_layer").and_then(Json::as_u64).unwrap_or(0),
            g("tributary_ratio"),
            g("additive_ratio"),
            g("equal_share_ratio"),
            g("rigid_lever_ratio"),
            g("light_tip_ratio_at_this_load"),
        );
    }
    let v = &ld.sensitivity;
    let _ = writeln!(
        s,
        "  Sensitivity: largest tributary ratio {:.1} with the slab envelope, {:.1} with the mid-plane slice. At P5 = 18 the peak layer needs at least {} contacts (a lower bound; the planner's count needs the planner).",
        v.envelope_largest,
        v.midplane_largest,
        v.contacts_lower_bound_p5_18
            .map_or_else(|| "(no contact)".to_owned(), |n| n.to_string())
    );
    structure_text(&mut s, ld);
    let _ = writeln!(s, "WARNINGS");
    for w in &data.warnings {
        let _ = writeln!(s, "  [{}] {}: {}", class_name(w.class), w.kind, w.message);
    }
    let _ = writeln!(s, "UNCERTAINTY");
    for u in UNCERTAINTY {
        let _ = writeln!(s, "  - {u}");
    }
    let _ = writeln!(s, "ASSUMPTIONS");
    for (n, a, c) in ASSUMPTIONS {
        let _ = writeln!(s, "  {n}: {a} Check: {c}");
    }
    s
}

fn structure_text(s: &mut String, ld: &plinth_support::report_data::LoadData) {
    use std::fmt::Write as _;
    let n = &ld.needs_summary;
    let _ = writeln!(s, "WHAT NEEDS SUPPORT");
    for i in &ld.structure.islands {
        let _ = writeln!(
            s,
            "  Island {}: first layer {}, {:.3} mm² at ({:.2}, {:.2}) mm, grows to {:.2} mm² over {} layers, at most {:.2} x {:.2} mm ({} x {} px), {}.",
            i.component,
            i.first_layer,
            i.first_area_mm2,
            i.first_xy[0],
            i.first_xy[1],
            i.peak_area_mm2,
            i.layers_alone,
            i.width_mm,
            i.height_mm,
            i.width_px,
            i.height_px,
            if i.exempt {
                "left unsupported (P18)"
            } else {
                "needs an anchor"
            }
        );
    }
    let _ = writeln!(
        s,
        "  Faces flatter than 45 degrees: {:.1} mm² of surface ({:.1} mm² seen from above, the plan-view area). Hidden {:.1} mm², partly visible {:.1} mm², top {:.1} mm², class not decided {:.1} mm² (surface areas).",
        n.surface_mm2,
        n.projected_mm2,
        n.class_surface_mm2[0],
        n.class_surface_mm2[1],
        n.class_surface_mm2[2],
        n.class_surface_mm2[3]
    );
    if let Some(c) = n.coverage {
        let _ = writeln!(
            s,
            "  Cover within 3.0 mm: {} of {} samples needing cover are covered ({:.2} mm² not covered).",
            c.covered, c.needing, c.uncovered_mm2
        );
    }
    let _ = writeln!(
        s,
        "  Self-supporting: faces at or above 45 degrees, and new material within 2.13 mm of rooted material (P8). Closed pockets: {}. Reach beyond P8: {} pieces.",
        ld.structure.cups.len(),
        ld.structure.reach.len()
    );
}

/// The scene of a written model for the review pictures: the model and each support record
/// as triangles in the plate frame. The model's translation (its lift above the plate) is
/// applied to the model; support records are in the same local frame and get the same
/// translation. Rods become columns when they are steeper than 60 degrees and braces
/// otherwise; the contact each part belongs to is not in the file, so it is `None`.
#[must_use]
pub fn scene_from_model(m: &plinth_chitubox::Model) -> plinth_render::Scene {
    use plinth_chitubox::RecordType as T;
    use plinth_render::{Part, Role, Scene};
    let shift = |t: &plinth_chitubox::Triangle| -> plinth_render::Tri {
        [0, 1, 2].map(|k| {
            [
                t[k][0] + m.translate[0],
                t[k][1] + m.translate[1],
                t[k][2] + m.translate[2],
            ]
        })
    };
    let model = m.mesh.iter().map(shift).collect();
    let parts = m.supports.as_ref().map_or_else(Vec::new, |s| {
        s.records
            .iter()
            .map(|r| {
                let dz = (r.end[2] - r.start[2]).abs();
                let len = (0..3)
                    .map(|k| (r.end[k] - r.start[k]).powi(2))
                    .sum::<f32>()
                    .sqrt();
                let role = match r.record_type {
                    T::TIP_BALL | T::FOOT_BALL => Role::TipBall,
                    T::TIP_CONE | T::FOOT_CONE => Role::TipCone,
                    T::JOINT_BALL => Role::Joint,
                    T::BASE_FLARE => Role::Flare,
                    T::BASE_PAD | T::RAFT => Role::Raft,
                    _ if len > 0.0 && dz / len > 0.866 => Role::Column,
                    _ => Role::Brace,
                };
                Part {
                    tris: r.mesh.iter().map(shift).collect(),
                    role,
                    contact: None,
                }
            })
            .collect()
    });
    Scene { model, parts }
}

/// One progress line for a finished model, for standard error.
#[must_use]
pub fn progress_line(candidate: bool, m: &ModelRun) -> String {
    let secs: f64 = m.steps.iter().map(|s| s.secs).sum();
    match &m.outcome {
        Outcome::Published { path, checked, .. } => format!(
            "plinth: {} -> {} ({}; checker: {}) {secs:.2} s",
            m.input.display(),
            path.display(),
            if m.supported
                && !crate::run::candidate_allowed(candidate, checked, m.planning_blockers)
            {
                "supported, not a print candidate"
            } else {
                label_of(crate::run::candidate_allowed(
                    candidate,
                    checked,
                    m.planning_blockers,
                ))
            },
            checked.verdict.name()
        ),
        Outcome::Failed { stage, message, .. } => {
            format!("plinth: FAILED at {stage}: {message}")
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::check::{Rule, Verdict};
    use std::path::PathBuf;

    fn run_with(outcome: Outcome) -> (Config, Run) {
        let cfg = Config {
            inputs: vec![PathBuf::from("a.stl")],
            out_dir: PathBuf::from("o"),
            max_threads: 2,
            checker: PathBuf::from("c"),
            limits: PathBuf::from("l"),
            candidate: false,
        };
        let m = ModelRun {
            input: PathBuf::from("a.stl"),
            stem: "a".into(),
            triangles: 12,
            steps: vec![],
            parallel: None,
            supported: false,
            planning_blockers: false,
            outcome,
        };
        let run = Run {
            models: vec![m],
            total_secs: 0.5,
            peak: Err("no rusage".into()),
        };
        (cfg, run)
    }

    #[test]
    fn a_report_is_labelled_and_splits_results_from_run_data() {
        let checked = Checked {
            verdict: Verdict::Unverified,
            sha256: "ab".into(),
            rules: vec![Rule {
                id: "H1".into(),
                status: "unverified".into(),
                reason: "later".into(),
                not_applicable_proved: false,
            }],
            structure_notes: vec![],
        };
        let (cfg, run) = run_with(Outcome::Published {
            path: PathBuf::from("o/a.chitubox"),
            bytes: 5,
            checked,
        });
        let text = to_json(&cfg, &run).render();
        let j = crate::jsonin::parse(text.as_bytes()).unwrap();
        assert_eq!(j.get("print_candidate"), Some(&Json::Bool(false)));
        assert_eq!(j.get("label").and_then(Json::as_str), Some(TRANSPORT_LABEL));
        let m = &j.get("models").and_then(Json::as_arr).unwrap()[0];
        assert_eq!(m.get("status").and_then(Json::as_str), Some("published"));
        assert_eq!(m.get("print_candidate"), Some(&Json::Bool(false)));
        assert!(m.get("checker").and_then(|c| c.get("rules")).is_some());
        // Timings and memory are only under "run".
        assert!(!text[..text.find("\"run\"").unwrap()].contains("seconds"));
        assert!(j.get("run").and_then(|r| r.get("peak_memory")).is_some());
    }

    fn small_load_data() -> plinth_support::report_data::LoadData {
        use plinth_support::load::{
            ComponentInfo, ComponentSlice, ContactView, LayerArea, LayerStack, LayoutView,
            PartChain, PartKind, PartSection, SiteClass, SourceElement, Structure,
        };
        let layers = (0..4)
            .map(|k| {
                let src: Vec<SourceElement> = (0..16)
                    .map(|i| SourceElement {
                        xyz: [
                            0.25 * f64::from(i % 4),
                            0.25 * f64::from(i / 4),
                            0.05 * f64::from(k),
                        ],
                        area: 0.25,
                    })
                    .collect();
                LayerArea {
                    model_envelope: 4.0,
                    model_midplane: 3.0,
                    components: vec![ComponentSlice {
                        component: 0,
                        area: 4.0,
                        sources: src,
                    }],
                    ..LayerArea::default()
                }
            })
            .collect();
        let stack = LayerStack {
            layer_height_mm: 0.05,
            layer_origin_mm: 0.0,
            pixel_pitch_mm: [0.014, 0.019],
            layers,
            components: vec![ComponentInfo {
                first_layer: 0,
                merged_into: None,
                is_island: true,
                exempt: false,
            }],
        };
        let part = |kind, area| PartSection {
            kind,
            record: Some(3),
            area_lower_mm2: area,
            z_range_mm: [-1.0, 0.0],
        };
        let layout = LayoutView {
            contacts: vec![ContactView {
                centre: [0.4, 0.4, 0.0],
                patch_lower_mm2: 0.5,
                neck_area_mm2: 0.5,
                first_layer: 0,
                site_class: SiteClass::HiddenUnderside,
            }],
            chains: vec![PartChain {
                contact: 0,
                parts: vec![part(PartKind::Patch, 0.5), part(PartKind::Neck, 0.5)],
            }],
        };
        plinth_support::report_data::LoadData::build(
            &stack,
            layout,
            Structure::default(),
            plinth_support::load::needs::NeedsMap::default(),
        )
    }

    #[test]
    fn the_fr21_block_names_ratios_as_a_heuristic_and_states_its_sources() {
        use plinth_support::report_data::{ProcessValues, ReportData};
        let ld = small_load_data();
        let data = ReportData {
            process: ProcessValues {
                layer_height_mm: 0.05,
                layer_height_source: "owner default profile".into(),
                design_ratio: 12.0,
                h1_admission_mm2: 0.48,
                coverage_reach_mm: 3.0,
                overhang_deg: 45.0,
                lift_mm: 5.0,
            },
            warnings: ld.detail.report.warnings.clone(),
            load: Some(ld.detail.report.clone()),
            load_data: Some(ld),
            ..ReportData::default()
        };
        let j = fr21_json(&data);
        assert_eq!(j.get("computed"), Some(&Json::Bool(true)));
        let load = j.get("load").expect("load");
        let top = load
            .get("largest_ratios")
            .and_then(Json::as_arr)
            .expect("ratios");
        assert!(!top.is_empty() && top.len() <= 10);
        // 4 mm² on a section of 0.5 mm² is 8; the worst layer is a layer index, the height is on the plate.
        let first = &top[0];
        assert_eq!(
            first.get("tributary_label").and_then(Json::as_str),
            Some(TRIBUTARY_LABEL)
        );
        assert!(
            matches!(first.get("tributary_ratio"), Some(Json::Num(r)) if (*r - 8.0).abs() < 1e-6)
        );
        assert!(matches!(first.get("plate_z_mm"), Some(Json::Num(z)) if *z >= 5.0));
        // The mid-plane sensitivity is 3/4 of the envelope one.
        let s = load.get("sensitivity").expect("sensitivity");
        assert!(matches!(
            (s.get("largest_tributary_ratio_envelope"), s.get("largest_tributary_ratio_midplane")),
            (Some(Json::Num(e)), Some(Json::Num(m))) if (m / e - 0.75).abs() < 1e-6
        ));
        assert_eq!(
            j.get("assumptions")
                .and_then(Json::as_arr)
                .map(<[Json]>::len),
            Some(12)
        );
        let text = fr21_text(&data);
        assert!(text.contains("PROCESS VALUES") && text.contains("sizing heuristic"));
        assert!(text.contains("Design ratio (P5, owner): 12"));
    }

    #[test]
    fn a_native_supported_file_becomes_a_scene_whose_supports_touch_the_model() {
        let path = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../test-data/chitubox-native/samples/04-round25-supports-raft-connected.chitubox"
        );
        let bytes = std::fs::read(path).expect("native sample");
        let project = plinth_chitubox::parse(&bytes).expect("parse");
        let scene = scene_from_model(&project.models[0]);
        assert_eq!(scene.model.len(), 2996);
        assert!(scene.parts.len() > 50, "{}", scene.parts.len());
        let balls = |r: plinth_render::Role| scene.parts.iter().filter(|p| p.role == r).count();
        assert!(balls(plinth_render::Role::TipBall) >= 3);
        assert!(balls(plinth_render::Role::Raft) >= 1);
        // Same frame: the lowest support part sits on the plate and the model's lowest point is
        // above it by the lift, so the tip balls are within a millimetre of the model surface.
        let (lo, _) = scene.bounds();
        assert!(lo[2] < 1.0, "supports start near the plate: {}", lo[2]);
        let near = scene
            .parts
            .iter()
            .filter(|p| p.role == plinth_render::Role::TipBall)
            .filter(|p| {
                let c = p.tris[0][0];
                scene.model.iter().flatten().any(|v| {
                    ((v[0] - c[0]).powi(2) + (v[1] - c[1]).powi(2) + (v[2] - c[2]).powi(2)).sqrt()
                        < 1.5
                })
            })
            .count();
        assert!(
            near >= 3,
            "tip balls within 1.5 mm of a model vertex: {near}"
        );
        // The pictures render from it, with no load data: the load pictures say unverified.
        let pics = plinth_render::render_pictures(
            &scene,
            &plinth_support::report_data::ReportData::default(),
        );
        assert_eq!(pics.len(), 12);
        let dir = std::env::temp_dir().join("plinth-native-pictures");
        plinth_render::write_all(&dir, &pics[..3]).expect("write pictures");
    }

    #[test]
    fn without_load_data_the_block_carries_no_ratio() {
        let data = plinth_support::report_data::ReportData::unverified("not built");
        let j = fr21_json(&data);
        assert_eq!(j.get("computed"), Some(&Json::Bool(false)));
        assert!(
            j.get("load")
                .and_then(|l| l.get("largest_ratios"))
                .is_none()
        );
        assert!(fr21_text(&data).starts_with("UNVERIFIED: not built"));
    }

    #[test]
    fn a_failure_names_its_stage_and_has_no_output() {
        let (cfg, run) = run_with(Outcome::Failed {
            stage: "read",
            message: "bad".into(),
            checked: None,
        });
        let j = crate::jsonin::parse(to_json(&cfg, &run).render().as_bytes()).unwrap();
        let m = &j.get("models").and_then(Json::as_arr).unwrap()[0];
        assert_eq!(m.get("failed_stage").and_then(Json::as_str), Some("read"));
        assert_eq!(m.get("output"), Some(&Json::Null));
        assert_eq!(
            j.get("job")
                .and_then(|x| x.get("failed"))
                .and_then(Json::as_u64),
            Some(1)
        );
    }
}
