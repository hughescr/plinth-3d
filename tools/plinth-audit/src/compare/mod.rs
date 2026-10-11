//! `plinth-audit compare <ref.chitubox> <cand.chitubox>`: the similarity gate.
//!
//! Specification: `ROUND-PLAN-v2.md` section 1 (and `OWNER-DECISIONS.md`,
//! 2026-10-10: O1 near-tied pose flips, O2 blessing at wave milestones, O3
//! decimation, O4 the BCO target). This module reads only the emitted bytes,
//! the engine `.log` and `poses.tsv`; it shares no code with the engine and
//! has no dependency, like the rest of this crate.
//!
//! **Class first.** The pose is judged from the committed transform and the
//! mesh separately, never from a voxel difference. **Tier 0** the file is
//! valid; **Tier 1** hard invariants H1-H6, any failure is REGRESSION whatever
//! the class; **Tier 2** bands N1-N5. EQUIVALENT is computed from the numbers.
//!
//! **What it does not settle** (so it says INDETERMINATE and never passes):
//! open model meshes and the BCO v22 junction near r83 until a ChituBox slice
//! sample has been matched to this raster (G5, `--assume-consumer-matched`
//! lifts it); an item the reference could evaluate and the candidate cannot.
//!
//! Exit codes: 1 REGRESSION, 3 INDETERMINATE, 2 POSE-CHANGED, 4 MESH-CHANGED,
//! 10 PLAN-CHANGED, 0 IDENTICAL or EQUIVALENT (the most severe wins; the
//! combined classes are in `verdict.json`). 64 is a usage or I/O error.

pub mod band;
pub mod classify;
pub mod consts;
pub mod log;
pub mod matching;
pub mod mutate;
pub mod side;
pub mod verdict;

use crate::json::Json;
use classify::{Options, Outcome};
use side::{FileFacts, Settings};
use std::path::{Path, PathBuf};
use std::time::Instant;
use verdict::{Status, Verdict};

pub const USAGE: &str = "\
plinth-audit compare -- the similarity gate (ROUND-PLAN-v2.md section 1).

USAGE:
  plinth-audit compare <ref.chitubox> <cand.chitubox> [OPTIONS]

OPTIONS:
  --ref-log F --cand-log F       engine logs (found by the run-recipe layout when omitted)
  --ref-poses F --cand-poses F   poses.tsv files (same)
  --out-dir D                    write D/verdict.json and D/layers.tsv
  --json                         print verdict.json on stdout instead of the summary
  --threads N                    threads for the geometry report
  --band N                       tolerance band in voxels (default 1; 0 is the sabotage case)
  --no-geometry                  skip the head and intrusion report (H6 is then not assessed)
  --require-logs                 a missing log makes H5 INDETERMINATE (the bench sets this)
  --assume-consumer-matched      lift the INDETERMINATE rule for open meshes and the BCO
                                 junction (only after G5 has matched a ChituBox slice sample)
  --no-flicker                   switch off every noise allowance (consts.rs, \"flicker allowance\"):
                                 the strict reading, for calibration and as the sabotage case
  --low-memory                   analyse the two files one after the other (half the peak memory)
  --full-listing                 list EVERY item behind every row in verdict.json and the summary
                                 (the default keeps the first few); for piece-by-piece triage
  --declared CLASS               what the lane promises, as bench.sh --declared: IDENTICAL, EQUIVALENT,
                                 PLAN-CHANGED or MESH-CHANGED. Recorded in verdict.json and listed in a
                                 C.declared row (within the declaration or not); the exit code stays the
                                 computed verdict's, and bench.sh makes the pass/fail decision
  --declare-change WORD[,WORD]   an expected change the lane declares (needs --declared PLAN-CHANGED or
                                 MESH-CHANGED). contact-merge: twin seats are merged, so a reference
                                 contact with no match whose ball is overlapped by a candidate contact
                                 is a merge, not a loss (N2); a loss with no survivor beside it is not
  --probe x,y,z,r               print the support records of both files whose axis passes within
                                 r mm of the point, with their match in the other file (repeatable)
  -h, --help                     this text

EXIT: 1 REGRESSION, 3 INDETERMINATE, 2 POSE-CHANGED, 4 MESH-CHANGED, 10 PLAN-CHANGED,
      0 IDENTICAL or EQUIVALENT, 64 usage or I/O error. The most severe verdict wins.
";

/// Parsed command line.
#[derive(Clone, Debug, Default)]
pub struct Args {
    pub reference: PathBuf,
    pub candidate: PathBuf,
    pub ref_log: Option<PathBuf>,
    pub cand_log: Option<PathBuf>,
    pub ref_poses: Option<PathBuf>,
    pub cand_poses: Option<PathBuf>,
    pub out_dir: Option<PathBuf>,
    pub json: bool,
    pub threads: usize,
    pub band: i64,
    pub no_geometry: bool,
    pub require_logs: bool,
    pub assume_consumer_matched: bool,
    /// Switch every noise allowance off (the strict reading; calibration and sabotage).
    pub no_flicker: bool,
    /// Analyse the two files one after the other (half the peak memory, more wall time).
    pub low_memory: bool,
    /// List every item behind every row (verdict.json and the summary), not the first few.
    pub full_listing: bool,
    /// `--probe x,y,z,r`: print the support records of both files near each point (triage).
    pub probes: Vec<(crate::vec3::V3, f64)>,
    /// `--declared CLASS`: what the lane promises (the same word as `bench.sh --declared`).
    pub declared: Option<classify::DeclaredClass>,
    /// `--declare-change WORD`: expected changes (a plan change each); see [`classify::DeclaredChanges`].
    pub declared_changes: classify::DeclaredChanges,
}

/// Parse the arguments after `compare`.
pub fn parse_args(argv: &[String]) -> Result<Args, String> {
    let mut a = Args { band: 1, threads: geometry_threads(), ..Args::default() };
    let mut files: Vec<PathBuf> = Vec::new();
    let mut i = 0;
    while i < argv.len() {
        let opt = argv[i].clone();
        let mut take = |name: &str| -> Result<String, String> {
            i += 1;
            argv.get(i).cloned().ok_or_else(|| format!("{name} needs a value"))
        };
        match opt.as_str() {
            "--ref-log" => a.ref_log = Some(PathBuf::from(take("--ref-log")?)),
            "--cand-log" => a.cand_log = Some(PathBuf::from(take("--cand-log")?)),
            "--ref-poses" => a.ref_poses = Some(PathBuf::from(take("--ref-poses")?)),
            "--cand-poses" => a.cand_poses = Some(PathBuf::from(take("--cand-poses")?)),
            "--out-dir" => a.out_dir = Some(PathBuf::from(take("--out-dir")?)),
            "--json" => a.json = true,
            "--no-geometry" => a.no_geometry = true,
            "--require-logs" => a.require_logs = true,
            "--assume-consumer-matched" => a.assume_consumer_matched = true,
            "--no-flicker" => a.no_flicker = true,
            "--low-memory" => a.low_memory = true,
            "--full-listing" => a.full_listing = true,
            "--declared" => {
                let s = take("--declared")?;
                a.declared = Some(classify::DeclaredClass::parse(&s).ok_or_else(|| format!("--declared wants IDENTICAL, EQUIVALENT, PLAN-CHANGED or MESH-CHANGED, got {s:?}"))?);
            }
            "--declare-change" => {
                let s = take("--declare-change")?;
                for w in s.split(',').filter(|w| !w.is_empty()) {
                    a.declared_changes.add(w)?;
                }
            }
            "--probe" => {
                let s = take("--probe")?;
                let v: Vec<f64> = s.split(',').filter_map(|x| if x.trim() == "any" { Some(f64::NAN) } else { x.trim().parse().ok() }).collect();
                if v.len() != 4 {
                    return Err("--probe wants x,y,z,r (mm)".into());
                }
                a.probes.push(([v[0], v[1], v[2]], v[3]));
            }
            "--threads" => {
                a.threads = take("--threads")?.parse().ok().filter(|&n| n > 0).ok_or("--threads wants a positive integer")?;
            }
            "--band" => {
                a.band = take("--band")?.parse().ok().filter(|&n| n >= 0).ok_or("--band wants a non-negative integer")?;
            }
            o if o.starts_with("--") => return Err(format!("unknown option {o:?}")),
            f => files.push(PathBuf::from(f)),
        }
        i += 1;
    }
    if files.len() != 2 {
        return Err(format!("expected <ref> and <cand>, got {} file(s)", files.len()));
    }
    if a.declared_changes.any() && !matches!(a.declared, Some(classify::DeclaredClass::PlanChanged | classify::DeclaredClass::MeshChanged)) {
        return Err("--declare-change needs --declared PLAN-CHANGED or MESH-CHANGED: a declared change is a plan change".into());
    }
    a.candidate = files.pop().unwrap_or_default();
    a.reference = files.pop().unwrap_or_default();
    Ok(a)
}

fn geometry_threads() -> usize {
    crate::geometry::default_threads()
}

/// Everything one comparison produced.
pub struct Run {
    pub outcome: Outcome,
    pub ref_facts: Result<FileFacts, String>,
    pub cand_facts: Result<FileFacts, String>,
    pub wall_s: f64,
    pub analyse_s: (f64, f64),
}

/// Compare two files on disk.
#[must_use]
pub fn compare_paths(a: &Args) -> Run {
    let t0 = Instant::now();
    let set = Settings { geometry: !a.no_geometry, threads: a.threads };
    let sha = |p: &Path| std::fs::read(p).ok().map(|b| crate::sha256::hex(&b));
    // Same bytes: IDENTICAL, with no analysis at all.
    if let (Some(x), Some(y)) = (sha(&a.reference), sha(&a.candidate)) {
        if x == y {
            let classes = verdict::Classes { identical: true, ..verdict::Classes::default() };
            return Run {
                outcome: Outcome {
                    verdict: Verdict::Identical,
                    classes,
                    rows: vec![verdict::Row::new("T0.sha256", "same sha256", Status::Pass, x, "equal")],
                    evidence: Json::obj(),
                    layers_tsv: String::new(),
                    compare_seconds: 0.0,
                },
                ref_facts: Err("not analysed: IDENTICAL by sha256".into()),
                cand_facts: Err("not analysed: IDENTICAL by sha256".into()),
                wall_s: t0.elapsed().as_secs_f64(),
                analyse_s: (0.0, 0.0),
            };
        }
    }
    let (rf, cf) = if a.low_memory {
        // One file at a time: the peak is one analysis, not two (laid_tube: 5.4 GB to about half).
        let t = Instant::now();
        let r = side::analyse(&a.reference, a.ref_log.as_deref(), a.ref_poses.as_deref(), &set);
        let rt = t.elapsed().as_secs_f64();
        let t = Instant::now();
        let c = side::analyse(&a.candidate, a.cand_log.as_deref(), a.cand_poses.as_deref(), &set);
        ((r, rt), (c, t.elapsed().as_secs_f64()))
    } else {
        std::thread::scope(|sc| {
            let h = sc.spawn(|| {
                let t = Instant::now();
                (side::analyse(&a.reference, a.ref_log.as_deref(), a.ref_poses.as_deref(), &set), t.elapsed().as_secs_f64())
            });
            let t = Instant::now();
            let c = side::analyse(&a.candidate, a.cand_log.as_deref(), a.cand_poses.as_deref(), &set);
            let ct = t.elapsed().as_secs_f64();
            let (r, rt) = h.join().unwrap_or_else(|_| (Err("reference analysis panicked".into()), 0.0));
            ((r, rt), (c, ct))
        })
    };
    let analyse_s = (rf.1, cf.1);
    let opts = Options {
        band_voxels: a.band,
        assume_consumer_matched: a.assume_consumer_matched,
        require_logs: a.require_logs,
        noise: if a.no_flicker { consts::Noise::none() } else { consts::Noise::measured() },
        full_listing: a.full_listing,
        declared: a.declared,
        declared_changes: a.declared_changes,
    };
    let outcome = match (&rf.0, &cf.0) {
        (Ok(r), Ok(c)) => classify::compare(r, c, &opts),
        (Err(e), _) => failed_outcome(Verdict::Indeterminate, "T0.parse", "the reference does not parse", e),
        (_, Err(e)) => failed_outcome(Verdict::Regression, "T0.parse", "the candidate does not parse", e),
    };
    Run { outcome, ref_facts: rf.0, cand_facts: cf.0, wall_s: t0.elapsed().as_secs_f64(), analyse_s }
}

fn failed_outcome(v: Verdict, id: &str, name: &'static str, why: &str) -> Outcome {
    let classes = verdict::Classes {
        regression: v == Verdict::Regression,
        indeterminate: v == Verdict::Indeterminate,
        ..verdict::Classes::default()
    };
    Outcome {
        verdict: v,
        classes,
        rows: vec![verdict::Row::new(id, name, if v == Verdict::Regression { Status::Fail } else { Status::Indeterminate }, why.to_string(), "both files parse")],
        evidence: Json::obj(),
        layers_tsv: String::new(),
        compare_seconds: 0.0,
    }
}

fn side_json(label: &str, path: &Path, f: &Result<FileFacts, String>) -> Json {
    let mut o = Json::obj();
    o.set("label", Json::str(label));
    o.set("path", Json::str(path.display().to_string()));
    match f {
        Ok(f) => {
            o.set("sha256", Json::str(f.sha256.clone()));
            o.set("bytes", Json::usize(f.byte_len));
            o.set("log", f.log.as_ref().and_then(|l| l.path.as_ref()).map_or(Json::Null, |p| Json::str(p.display().to_string())));
            o.set(
                "pose",
                f.pose.as_ref().map_or(Json::Null, |p| {
                    let mut j = Json::obj();
                    j.set("source", Json::str(p.source));
                    j.set("committed", Json::str(p.committed.clone()));
                    j
                }),
            );
            let mut models = Json::Arr(Vec::new());
            for m in &f.models {
                let mut j = Json::obj();
                j.set("name", Json::str(m.name.clone()));
                j.set("translate_mm", Json::v3(m.translate));
                j.set("records", Json::usize(m.recs.len()));
                j.set("contacts", Json::usize(m.tips.len()));
                j.set("model_raster", Json::str(format!("{:?}", m.model_raster)));
                j.set("model_voxels", Json::int(m.m.pixel_count as i64));
                j.set("support_voxels", Json::int(m.s.pixel_count as i64));
                let mut ph = Json::obj();
                let s = m.seconds;
                for (k, v) in [("parse", s.parse), ("build", s.build), ("raster", s.raster), ("structure", s.structure), ("audit", s.audit), ("geometry", s.geometry)] {
                    ph.set(k, Json::num(v));
                }
                j.set("seconds", ph);
                j.set("audit_check_seconds", Json::nums(m.audit.check_s.iter().copied()));
                models.push(j);
            }
            o.set("models", models);
        }
        Err(e) => {
            o.set("error", Json::str(e.clone()));
        }
    }
    o
}

/// The `verdict.json` document.
#[must_use]
pub fn verdict_json(a: &Args, run: &Run) -> Json {
    let out = &run.outcome;
    let mut d = Json::obj();
    d.set("schema", Json::str("plinth/compare-result"));
    d.set("schema_version", Json::int(1));
    d.set("tool", Json::str(format!("plinth-audit compare {}", env!("CARGO_PKG_VERSION"))));
    d.set("verdict", Json::str(out.verdict.name()));
    d.set("exit_code", Json::int(out.verdict.exit_code()));
    d.set("classes", out.classes.to_json());
    let mut opt = Json::obj();
    opt.set("band_voxels", Json::int(a.band));
    opt.set("assume_consumer_matched", Json::Bool(a.assume_consumer_matched));
    opt.set("require_logs", Json::Bool(a.require_logs));
    opt.set("flicker_allowance", Json::Bool(!a.no_flicker));
    opt.set("geometry", Json::Bool(!a.no_geometry));
    opt.set("declared", a.declared.map_or(Json::Null, |c| Json::str(c.name())));
    opt.set("declared_changes", Json::Arr(a.declared_changes.names().into_iter().map(Json::str).collect()));
    d.set("options", opt);
    let mut cm = Json::obj();
    cm.set("status", Json::str(if a.assume_consumer_matched { "assumed matched" } else { "pending G5" }));
    cm.set("effect", Json::str("raster structural checks (H1, H2, N4) on open meshes and at the BCO v22 junction near r83 report INDETERMINATE"));
    d.set("consumer_match", cm);
    d.set("reference", side_json("ref", &a.reference, &run.ref_facts));
    d.set("candidate", side_json("cand", &a.candidate, &run.cand_facts));
    d.set("rows", Json::Arr(out.rows.iter().map(verdict::Row::to_json).collect()));
    d.set("evidence", out.evidence.clone());
    let mut cost = Json::obj();
    cost.set("wall_s", Json::num(run.wall_s));
    cost.set("analyse_ref_s", Json::num(run.analyse_s.0));
    cost.set("analyse_cand_s", Json::num(run.analyse_s.1));
    cost.set("compare_s", Json::num(out.compare_seconds));
    cost.set("note", Json::str("CPU time is not read in-process (zero dependencies); measure it with /usr/bin/time -l"));
    d.set("cost", cost);
    d
}

/// The human summary.
#[must_use]
pub fn summary(a: &Args, run: &Run) -> String {
    let out = &run.outcome;
    let mut s = String::new();
    s.push_str(&format!("ref  {}\ncand {}\n", a.reference.display(), a.candidate.display()));
    s.push_str(&format!("VERDICT {}  (exit {})\n", out.verdict.name(), out.verdict.exit_code()));
    let c = out.classes;
    s.push_str(&format!(
        "classes: identical={} equivalent={} plan_changed={} mesh_changed={} pose_changed={} regression={} indeterminate={}\n",
        c.identical, c.equivalent, c.plan_changed, c.mesh_changed, c.pose_changed, c.regression, c.indeterminate
    ));
    for r in &out.rows {
        s.push_str(&format!("  {:<14} {:<13} {}  [{}]\n", r.id, r.status.as_str().to_uppercase(), r.value, r.name));
        if r.status != Status::Pass && r.status != Status::Info {
            for d in r.detail.iter().take(if a.full_listing { usize::MAX } else { 5 }) {
                s.push_str(&format!("                   - {d}\n"));
            }
        }
    }
    s.push_str(&format!("time: wall {:.2} s (analyse ref {:.2} s, cand {:.2} s, compare {:.2} s)\n", run.wall_s, run.analyse_s.0, run.analyse_s.1, out.compare_seconds));
    s
}

/// `plinth-audit compare ...`; returns the process exit code.
pub fn main(argv: &[String]) -> i32 {
    if argv.iter().any(|x| x == "-h" || x == "--help") {
        print!("{USAGE}");
        return 0;
    }
    let args = match parse_args(argv) {
        Ok(a) => a,
        Err(e) => {
            eprintln!("plinth-audit compare: {e}\n\n{USAGE}");
            return 64;
        }
    };
    for p in [&args.reference, &args.candidate] {
        if !p.is_file() {
            eprintln!("plinth-audit compare: {} is not a file", p.display());
            return 64;
        }
    }
    let run = compare_paths(&args);
    let doc = verdict_json(&args, &run);
    if let Some(dir) = &args.out_dir {
        if let Err(e) = std::fs::create_dir_all(dir)
            .and_then(|()| std::fs::write(dir.join("verdict.json"), doc.to_pretty()))
            .and_then(|()| std::fs::write(dir.join("layers.tsv"), &run.outcome.layers_tsv))
        {
            eprintln!("plinth-audit compare: cannot write {}: {e}", dir.display());
            return 64;
        }
    }
    if args.json {
        print!("{}", doc.to_pretty());
    } else {
        print!("{}", summary(&args, &run));
    }
    if let (Ok(r), Ok(c)) = (&run.ref_facts, &run.cand_facts) {
        for (p, rad) in &args.probes {
            println!("probe ({:.3},{:.3},{:.3}) within {rad} mm:", p[0], p[1], p[2]);
            for l in classify::probe(r, c, *p, *rad) {
                println!("  {l}");
            }
        }
    }
    run.outcome.verdict.exit_code()
}
