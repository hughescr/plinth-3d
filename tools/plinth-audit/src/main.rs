//! `plinth-audit <file.chitubox> [--sidecar <path>] [--json] [--out-dir <dir>]`
//!
//! Exits non-zero when any invariant fails, when a file-level failure aborts
//! the run, or when the arguments are wrong. It exits zero only when every
//! model of the file passed all ten gating invariants: I1-I8, plus I9
//! unsupported span and I10 arrangement spread.
//!
//! **stdout carries the human summary and nothing else**, so the tool
//! composes. `--json` puts the document on stdout instead. Diagnostics go to
//! stderr either way.

use plinth_audit::context::{AuditOptions, DeclaredInputs};
use plinth_audit::run;
use std::path::{Path, PathBuf};

const USAGE: &str = "\
plinth-audit -- audit the emitted bytes of a .chitubox file against the ten
gating invariants of docs/specs/audit-invariants.md, plus I9 unsupported span
(audit-span-criterion.md) and I10 arrangement spread (audit-spread-criterion.md).

USAGE:
  plinth-audit <file.chitubox> [OPTIONS]

OPTIONS:
  --sidecar <path>        read this .coverage.repair.json instead of the
                          conventional path beside the artifact. Its absence is
                          recorded and fails the DECLARED comparisons; it never
                          passes them.
  --json                  write the result document to stdout instead of the
                          human summary.
  --out-dir <dir>         also write <dir>/<stem>.audit.json.
  --contact-depth <mm>    override default_contact_depth_mm. Ruled 0.100 mm on
                          2026-08-30; values above 0.300 or below the layer
                          height are refused as out_of_ruled_range.
  --sample-budget <n>     ownership sampling cap; exceeding it FAILS I4 with
                          sample_budget_exceeded rather than coarsening.
  --dump-graph            print the reconstruction (records, solids, joins,
                          components, ambiguities, seats) to stderr. Diagnostic
                          only; it decides nothing.
  --join-tol <mm>         override the endpoint-coincidence tolerance. Leave it
                          alone: a hand-picked tolerance is how a wrong support
                          graph gets built quietly.
  -h, --help              this text.

EXIT CODE: 0 only when every model passes all ten gating invariants. That is
           NOT a statement that the print is safe -- see audit-span-criterion.md
           section 8 and audit-spread-criterion.md section 6 for what a full
           pass still cannot see.
";

fn main() {
    let argv: Vec<String> = std::env::args().skip(1).collect();
    if argv.is_empty() || argv.iter().any(|a| a == "-h" || a == "--help") {
        print!("{USAGE}");
        std::process::exit(if argv.is_empty() { 2 } else { 0 });
    }

    let mut target: Option<PathBuf> = None;
    let mut sidecar: Option<PathBuf> = None;
    let mut out_dir: Option<PathBuf> = None;
    let mut want_json = false;
    let mut dump_graph = false;
    let mut options = AuditOptions {
        inputs: DeclaredInputs::default(),
        ..AuditOptions::default()
    };

    let mut i = 0usize;
    while i < argv.len() {
        let a = &argv[i];
        let mut take = |name: &str| -> String {
            i += 1;
            argv.get(i).cloned().unwrap_or_else(|| {
                eprintln!("plinth-audit: {name} needs a value");
                std::process::exit(2);
            })
        };
        match a.as_str() {
            "--sidecar" => sidecar = Some(PathBuf::from(take("--sidecar"))),
            "--out-dir" => out_dir = Some(PathBuf::from(take("--out-dir"))),
            "--json" => want_json = true,
            "--dump-graph" => dump_graph = true,
            "--contact-depth" => {
                options.inputs.default_contact_depth_mm = parse_f64(&take("--contact-depth"));
            }
            "--sample-budget" => {
                let v = take("--sample-budget");
                options.sample_budget.max_samples = v.parse().unwrap_or_else(|_| {
                    eprintln!("plinth-audit: --sample-budget wants an integer, got {v:?}");
                    std::process::exit(2);
                });
            }
            "--join-tol" => options.join_tol_override_mm = Some(parse_f64(&take("--join-tol"))),
            other if other.starts_with("--") => {
                eprintln!("plinth-audit: unknown option {other:?}\n\n{USAGE}");
                std::process::exit(2);
            }
            other => {
                if target.is_some() {
                    eprintln!("plinth-audit: more than one input file given");
                    std::process::exit(2);
                }
                target = Some(PathBuf::from(other));
            }
        }
        i += 1;
    }

    let Some(target) = target else {
        eprintln!("plinth-audit: no input file\n\n{USAGE}");
        std::process::exit(2);
    };
    if !target.exists() {
        eprintln!("plinth-audit: {} does not exist", target.display());
        std::process::exit(2);
    }

    if dump_graph {
        match plinth_audit::reader::ChituboxFile::open(&target) {
            Ok(file) => {
                let audit = plinth_audit::context::AuditContext {
                    file: &file,
                    options: &options,
                    sidecar: None,
                    sidecar_absence: None,
                };
                for mi in 0..file.models.len() {
                    let ctx = plinth_audit::context::ModelContext::build(&audit, mi);
                    eprint!("{}", run::dump_reconstruction(&ctx));
                }
            }
            Err(e) => eprintln!("plinth-audit: {e}"),
        }
    }

    let result = run::audit_file(&target, &options, sidecar.as_deref());

    if let Some(dir) = out_dir {
        if let Err(e) = std::fs::create_dir_all(&dir) {
            eprintln!("plinth-audit: cannot create {}: {e}", dir.display());
            std::process::exit(2);
        }
        let stem = target
            .file_stem()
            .map_or_else(|| "audit".to_string(), |s| s.to_string_lossy().into_owned());
        let path: PathBuf = Path::new(&dir).join(format!("{stem}.audit.json"));
        if let Err(e) = std::fs::write(&path, result.document.to_pretty()) {
            eprintln!("plinth-audit: cannot write {}: {e}", path.display());
            std::process::exit(2);
        }
        eprintln!("plinth-audit: wrote {}", path.display());
    }

    if want_json {
        print!("{}", result.document.to_pretty());
    } else {
        print!("{}", result.summary);
    }
    std::process::exit(result.exit_code());
}

fn parse_f64(s: &str) -> f64 {
    s.parse().unwrap_or_else(|_| {
        eprintln!("plinth-audit: expected a number, got {s:?}");
        std::process::exit(2);
    })
}
