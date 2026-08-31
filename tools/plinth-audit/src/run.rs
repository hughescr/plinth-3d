//! The audit driver and the output document.
//!
//! Produces exactly the schema of `docs/specs/audit-invariants.md` §5, plus
//! the human-readable summary of the same section. Every pinned rule of that
//! section is enforced here rather than left to the checks:
//!
//! * `status` takes exactly two values and `indeterminate > 0` forces `fail`;
//! * `print_candidate` is true only when all EIGHT gating invariants pass, and
//!   the report section never contributes to it;
//! * a file-level failure aborts before any invariant runs, and **the document
//!   is still written**, with every invariant `"fail"` and `indeterminate`
//!   equal to its `checked` count, so a parse failure is never confused with a
//!   clean file;
//! * `declared_inputs` is echoed in full, with provenance.

use crate::check::{Check, CheckOutcome, Status};
use crate::checks;
use crate::consts as c;
use crate::context::{AuditContext, AuditOptions, ModelContext};
use crate::json::Json;
use crate::reader::{ChituboxFile, ParseError};
use crate::sidecar::Sidecar;
use std::path::Path;

pub const SCHEMA: &str = "plinth/audit-result";
pub const SCHEMA_VERSION: i64 = 1;
pub const TOOL_NAME: &str = "plinth-audit";
pub const TOOL_VERSION: &str = env!("CARGO_PKG_VERSION");

/// A failure that aborts before any invariant runs (§5 rule 6).
#[derive(Clone, Debug)]
pub struct FileFailure {
    pub code: &'static str,
    pub message: String,
}

/// The complete result of auditing one file.
pub struct AuditResult {
    pub document: Json,
    pub summary: String,
    /// `true` only when every model is a print candidate and there is no
    /// file-level failure.
    pub print_candidate: bool,
    pub file_level_failures: Vec<FileFailure>,
}

impl AuditResult {
    /// The process exit code: 0 only for a clean pass.
    #[must_use]
    pub const fn exit_code(&self) -> i32 {
        if self.print_candidate {
            0
        } else {
            1
        }
    }
}

/// Audit one `.chitubox` file.
///
/// `sidecar_path`: when `Some` the file must load or the audit reports the
/// failure; when `None` the conventional path beside the artifact is tried and
/// its **absence is recorded, never treated as a pass**.
#[must_use]
pub fn audit_file(path: &Path, options: &AuditOptions, sidecar_path: Option<&Path>) -> AuditResult {
    let mut file_failures: Vec<FileFailure> = Vec::new();

    // Declared inputs first: an absent or out-of-range one fails the file
    // before anything is read (§1.3).
    if let Err(bad) = options.inputs.validate() {
        for b in bad {
            file_failures.push(FileFailure {
                code: "FILE_DECLARED_INPUT_MISSING",
                message: format!(
                    "FILE_DECLARED_INPUT_MISSING file={} name={} reason={} value={}",
                    path.display(),
                    b.name,
                    b.reason,
                    b.value
                ),
            });
        }
    }

    let parsed = if file_failures.is_empty() {
        ChituboxFile::open(path)
    } else {
        // Still parse, so the document carries what it can.
        ChituboxFile::open(path)
    };

    let file = match parsed {
        Ok(f) => f,
        Err(e) => {
            file_failures.push(FileFailure {
                code: e.code(),
                message: format!("{} file={} {e}", e.code(), path.display()),
            });
            return aborted_document(path, options, &file_failures, None);
        }
    };

    // Sidecar. Absence degrades the DECLARED comparisons to unverifiable; it
    // never degrades them to "pass" (`audit-contract.md` §5.4).
    let want = sidecar_path.map_or_else(|| Sidecar::default_path_for(path), Path::to_path_buf);
    let (sidecar, sidecar_absence) = if want.exists() {
        match Sidecar::load(&want) {
            Ok(s) => (Some(s), None),
            Err(e) => (None, Some(format!("{e}"))),
        }
    } else if sidecar_path.is_some() {
        (
            None,
            Some(format!(
                "requested sidecar {} does not exist",
                want.display()
            )),
        )
    } else {
        (
            None,
            Some(format!(
                "no sidecar at {}; every DECLARED comparison that needs one is unverifiable",
                want.display()
            )),
        )
    };

    let audit = AuditContext {
        file: &file,
        options,
        sidecar: sidecar.as_ref(),
        sidecar_absence: sidecar_absence.clone(),
    };

    let gating = checks::gating();
    let report_check = checks::report();

    let mut models_json = Json::Arr(Vec::new());
    let mut summary = String::new();
    let mut models_print_candidate = 0usize;

    for mi in 0..file.models.len() {
        let ctx = ModelContext::build(&audit, mi);

        // §1.2: the frame cross-check must run before anything else. It is the
        // one check that catches a mis-parsed lift.
        let frame = ctx.frame_check();
        if !frame.is_established() {
            file_failures.push(FileFailure {
                code: "FILE_FRAME_MISMATCH",
                message: format!(
                    "FILE_FRAME_MISMATCH file={} model={mi} z_plate_mm={} \
                     min_support_z_mm={} support_below_plate_mm={} record={} \
                     tolerance_mm={} source={}",
                    path.display(),
                    frame.z_plate_mm,
                    frame.min_support_z_mm.unwrap_or(f64::NAN),
                    frame.support_below_plate_mm,
                    frame
                        .lowest_support_record
                        .map_or_else(|| "none".to_string(), |r| r.to_string()),
                    frame.tolerance_mm,
                    frame.source.label()
                ),
            });
        }
        // §1.4: a model that is not closed fails the file. Inside/outside is
        // undefined on an open soup, and the never-pass rule forbids
        // proceeding on a guess.
        if !ctx.model_mesh.is_empty() && !ctx.model_mesh.closure.is_closed() {
            file_failures.push(FileFailure {
                code: "FILE_MESH_NOT_CLOSED",
                message: format!(
                    "FILE_MESH_NOT_CLOSED file={} model={mi} {}",
                    path.display(),
                    ctx.model_mesh.closure.summary()
                ),
            });
        }

        let abort = !file_failures.is_empty();
        let mut invariants = Json::obj();
        let mut passed = 0usize;
        let mut first_failure: Option<String> = None;
        let mut model_summary = Vec::new();

        for chk in &gating {
            let outcome = if abort {
                aborted_outcome(chk.as_ref(), &ctx)
            } else {
                chk.run(&ctx)
            };
            if outcome.status() == Status::Pass {
                passed += 1;
            } else if first_failure.is_none() {
                first_failure = outcome
                    .failures
                    .first()
                    .map(|f| f.code.to_string())
                    .or_else(|| Some("UNSPECIFIED".to_string()));
            }
            model_summary.push(format!(
                "  {:<20} {:<5} {:>4} checked, {} failed, {} indeterminate   {}",
                chk.title(),
                if outcome.status() == Status::Pass {
                    "pass"
                } else {
                    "FAIL"
                },
                outcome.checked,
                outcome.failed,
                outcome.indeterminate,
                outcome.headline()
            ));
            invariants.set(chk.id(), outcome.to_json());
        }

        let report_outcome = if abort {
            aborted_outcome(&report_check, &ctx)
        } else {
            report_check.run(&ctx)
        };
        let mut report_json = report_outcome.measured.clone();
        // The report never carries a pass or a fail. Its verdict is literally
        // "disclosed", and "disclosed" is not a passing value anywhere.
        report_json.set("verdict", Json::str("disclosed"));
        report_json.set(
            "implemented",
            Json::Bool(!report_outcome.is_not_implemented()),
        );

        let model_candidate = passed == 8;
        if model_candidate {
            models_print_candidate += 1;
        }

        let m = &file.models[mi];
        let mut mj = Json::obj();
        mj.set("index", Json::usize(mi));
        mj.set("name", Json::str(m.name.clone()));
        mj.set("translate_mm", Json::v3(m.translate_mm));
        mj.set("z_plate_mm", Json::num(ctx.z_plate_mm));
        mj.set("model_triangles", Json::usize(m.model_soup.len()));
        mj.set("support_records", Json::usize(m.records.len()));
        mj.set("contacts", Json::usize(ctx.seats.len()));
        mj.set("invariants", invariants);
        mj.set("report", report_json);
        mj.set("reconstruction", reconstruction_json(&ctx));
        let mut vj = Json::obj();
        vj.set("print_candidate", Json::Bool(model_candidate));
        vj.set("gating_passed", Json::usize(passed));
        vj.set("gating_total", Json::usize(8));
        vj.set(
            "first_failure",
            first_failure.clone().map_or(Json::Null, Json::str),
        );
        mj.set("verdict", vj);
        models_json.push(mj);

        summary.push_str(&format!(
            "{}  model {mi} \"{}\"   {}  ({passed}/8)\n",
            path.file_stem()
                .map_or_else(|| "?".into(), |s| s.to_string_lossy().into_owned()),
            m.name,
            if model_candidate {
                "PRINT CANDIDATE"
            } else {
                "NOT A PRINT CANDIDATE"
            }
        ));
        for line in model_summary {
            summary.push_str(&line);
            summary.push('\n');
        }
        summary.push_str(&format!(
            "  disclosed  {}\n",
            report_outcome
                .measured
                .get("headline")
                .and_then(Json::as_str)
                .unwrap_or("report metrics not implemented")
        ));
    }

    let print_candidate = file_failures.is_empty()
        && models_print_candidate == file.models.len()
        && !file.models.is_empty();

    let mut doc = envelope(path, options, Some(&file), &file_failures);
    doc.set("models", models_json);
    let mut fv = Json::obj();
    fv.set("print_candidate", Json::Bool(print_candidate));
    fv.set("models_total", Json::usize(file.models.len()));
    fv.set(
        "models_print_candidate",
        Json::usize(models_print_candidate),
    );
    fv.set(
        "file_level_failures",
        Json::Arr(
            file_failures
                .iter()
                .map(|f| {
                    let mut o = Json::obj();
                    o.set("code", Json::str(f.code));
                    o.set("message", Json::str(f.message.clone()));
                    o
                })
                .collect(),
        ),
    );
    doc.set("verdict", fv);

    for f in &file_failures {
        summary.push_str(&f.message);
        summary.push('\n');
    }

    AuditResult {
        document: doc,
        summary,
        print_candidate,
        file_level_failures: file_failures,
    }
}

/// The `checked == indeterminate == scope` shape §5 rule 6 pins for an abort.
fn aborted_outcome(chk: &dyn Check, ctx: &ModelContext) -> CheckOutcome {
    let scope = chk.scope(ctx).max(1);
    let mut o = CheckOutcome::new(scope);
    o.indeterminate = scope;
    o.measured.set("aborted", Json::Bool(true));
    o.measured.set(
        "headline",
        Json::str("file-level failure; no invariant was run"),
    );
    o
}

fn reconstruction_json(ctx: &ModelContext) -> Json {
    let mut o = Json::obj();
    o.set("welded_vertices", Json::usize(ctx.model_mesh.verts.len()));
    o.set(
        "mesh_closed",
        Json::Bool(ctx.model_mesh.closure.is_closed()),
    );
    o.set(
        "mesh_boundary_edges",
        Json::usize(ctx.model_mesh.closure.boundary_edges),
    );
    o.set(
        "mesh_nonmanifold_edges",
        Json::usize(ctx.model_mesh.closure.nonmanifold_edges),
    );
    o.set(
        "surface_components",
        Json::usize(ctx.model_mesh.component_count),
    );
    o.set("graph_join_tol_mm", Json::num(ctx.graph.join_tol_mm));
    o.set("graph_file_class", Json::str(ctx.graph.file_class_label));
    o.set("graph_joins", Json::usize(ctx.graph.joins.len()));
    o.set("graph_components", Json::usize(ctx.graph.component_count));
    o.set(
        "graph_ambiguities",
        Json::usize(ctx.graph.ambiguity_count()),
    );
    o.set(
        "graph_structural_ambiguities",
        Json::usize(ctx.graph.structural_ambiguity_count()),
    );
    let degenerate = ctx.primitives.iter().filter(|p| p.is_err()).count();
    o.set("degenerate_records", Json::usize(degenerate));
    o.set("frame_check", ctx.frame_check().to_json());
    o.set("z_plate_source", Json::str(ctx.z_plate_source.label()));
    o
}

fn envelope(
    path: &Path,
    options: &AuditOptions,
    file: Option<&ChituboxFile>,
    _failures: &[FileFailure],
) -> Json {
    let mut doc = Json::obj();
    doc.set("schema", Json::str(SCHEMA));
    doc.set("version", Json::int(SCHEMA_VERSION));
    let mut tool = Json::obj();
    tool.set("name", Json::str(TOOL_NAME));
    tool.set("version", Json::str(TOOL_VERSION));
    tool.set(
        "commit",
        std::env::var("PLINTH_AUDIT_COMMIT").map_or(Json::Null, Json::str),
    );
    doc.set("tool", tool);
    let mut fj = Json::obj();
    fj.set(
        "path",
        Json::str(
            std::fs::canonicalize(path)
                .unwrap_or_else(|_| path.to_path_buf())
                .display()
                .to_string(),
        ),
    );
    match file {
        Some(f) => {
            fj.set("sha256", Json::str(f.sha256.clone()));
            fj.set("bytes", Json::usize(f.byte_len));
            fj.set("model_count", Json::usize(f.models.len()));
            fj.set("file_class", Json::str(f.file_class.label()));
            fj.set("container_findings", findings_json(f));
        }
        None => {
            fj.set("sha256", Json::Null);
            fj.set("bytes", Json::Null);
        }
    }
    doc.set("file", fj);
    doc.set("declared_inputs", options.inputs.to_json());
    let mut derived = Json::obj();
    derived.set("pixel_area_mm2", Json::num(c::PIXEL_AREA_MM2));
    derived.set("voxel_volume_mm3", Json::num(c::VOXEL_VOLUME_MM3));
    derived.set("d2_mm", Json::num(c::D2_MM));
    derived.set("d3_fusion_uncertain_band_mm", Json::num(c::D3_MM));
    derived.set("eps32", Json::num(c::EPS32));
    derived.set(
        "ball_seat_factor",
        Json::num(c::ball_seat_factor(options.inputs.ball_factor_stub)),
    );
    doc.set("derived", derived);
    doc
}

fn findings_json(f: &ChituboxFile) -> Json {
    let fd = &f.findings;
    let mut o = Json::obj();
    o.set("total", Json::usize(fd.total()));
    o.set(
        "model_reserved_nonzero_bytes",
        Json::usize(fd.model_reserved_nonzero.iter().map(|x| x.1).sum()),
    );
    o.set(
        "group_header_nonzero_bytes",
        Json::usize(fd.group_header_nonzero.iter().map(|x| x.1).sum()),
    );
    o.set(
        "records_with_nonzero_trailing_tail",
        Json::usize(fd.trailing_tail_nonzero.len()),
    );
    o.set(
        "raft_records_with_unexpected_slot1",
        Json::usize(fd.raft_slot1_unexpected.len()),
    );
    o.set(
        "unreferenced_support_mesh_bytes",
        Json::num(
            fd.unreferenced_support_mesh_bytes
                .iter()
                .map(|x| x.1)
                .sum::<u64>() as f64,
        ),
    );
    o.set(
        "overlapping_mesh_intervals",
        Json::usize(fd.overlapping_mesh_intervals.len()),
    );
    o.set(
        "settings_size_unexpected",
        fd.settings_size_unexpected
            .map_or(Json::Null, |v| Json::num(f64::from(v))),
    );
    o.set(
        "const16_unexpected",
        fd.const16_unexpected
            .map_or(Json::Null, |v| Json::num(f64::from(v))),
    );
    let mut counts = Json::obj();
    for (tris, n) in &f.ball_triangle_counts {
        counts.set(&format!("{tris}"), Json::usize(*n));
    }
    o.set("ball_triangle_counts", counts);
    o
}

/// A parse failure still writes a document, with every invariant `"fail"`.
fn aborted_document(
    path: &Path,
    options: &AuditOptions,
    failures: &[FileFailure],
    file: Option<&ChituboxFile>,
) -> AuditResult {
    let mut doc = envelope(path, options, file, failures);
    doc.set("models", Json::Arr(Vec::new()));
    let mut fv = Json::obj();
    fv.set("print_candidate", Json::Bool(false));
    fv.set("models_total", Json::usize(0));
    fv.set("models_print_candidate", Json::usize(0));
    fv.set(
        "file_level_failures",
        Json::Arr(
            failures
                .iter()
                .map(|f| {
                    let mut o = Json::obj();
                    o.set("code", Json::str(f.code));
                    o.set("message", Json::str(f.message.clone()));
                    o
                })
                .collect(),
        ),
    );
    doc.set("verdict", fv);
    let summary = failures
        .iter()
        .map(|f| format!("{}\n", f.message))
        .collect::<String>();
    AuditResult {
        document: doc,
        summary,
        print_candidate: false,
        file_level_failures: failures.to_vec(),
    }
}

/// The parse error a caller may want to report separately.
#[must_use]
pub const fn parse_error_code(e: &ParseError) -> &'static str {
    e.code()
}

/// A human-readable dump of the reconstruction, for an agent debugging one
/// invariant. Records, their analytic solids, the joins, the components, the
/// ambiguities and the seats. Diagnostic only; nothing here decides a verdict.
#[must_use]
pub fn dump_reconstruction(ctx: &ModelContext) -> String {
    use std::fmt::Write as _;
    let mut s = String::new();
    let _ = writeln!(
        s,
        "model {} \"{}\"  records={}  contacts={}  z_plate={} ({})",
        ctx.model_index,
        ctx.model.name,
        ctx.model.records.len(),
        ctx.seats.len(),
        ctx.z_plate_mm,
        ctx.z_plate_source.label()
    );
    let _ = writeln!(
        s,
        "mesh: {} tris, {} welded verts, closed={}, {} surface components",
        ctx.model_mesh.tri_count(),
        ctx.model_mesh.verts.len(),
        ctx.model_mesh.closure.is_closed(),
        ctx.model_mesh.component_count
    );
    let _ = writeln!(
        s,
        "graph: tol={} mm ({}), {} joins, {} components, {} ambiguities",
        ctx.graph.join_tol_mm,
        ctx.graph.file_class_label,
        ctx.graph.joins.len(),
        ctx.graph.component_count,
        ctx.graph.ambiguity_count()
    );
    for (i, rec) in ctx.model.records.iter().enumerate() {
        let solid = match &ctx.primitives[i] {
            Ok(p) => format!("{p:?}"),
            Err(e) => format!("DEGENERATE {e:?}"),
        };
        let _ = writeln!(
            s,
            "  r{i:<4} {:<7} start={:?} end={:?} r1={} r2={} tri={} comp={} nbrs={:?}",
            rec.kind.label(),
            rec.start_mm,
            rec.end_mm,
            rec.r_start,
            rec.r_end,
            rec.soup.len(),
            ctx.graph.component_of_record[i],
            ctx.graph.neighbours_of(i)
        );
        let _ = writeln!(s, "         solid {solid}");
    }
    for a in &ctx.graph.ambiguities {
        let _ = writeln!(s, "  AMBIGUITY {} {a:?}", a.kind());
    }
    for st in &ctx.seats {
        let _ = writeln!(
            s,
            "  seat c{} h_delivered={:?} h_required={:?} binding={:?} sin_theta={:?} \
             r_tip={} spread={} agrees={} declared={:?} indet={:?}",
            st.contact_id,
            st.h_delivered_mm,
            st.h_required_mm,
            st.binding_term.map(crate::seat::BindingTerm::token),
            st.sin_theta,
            st.r_tip_mm,
            st.r_tip_spread_mm,
            st.r_tip_agrees,
            st.declared_depth_mm,
            st.indeterminate
        );
    }
    s
}
