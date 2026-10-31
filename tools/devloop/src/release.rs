//! `devloop release-check`: release-path correctness (design dev-loop.md section 10, step 9).
//! Run it in the background; a cold release build takes a while.
//!
//! 1. Guards on the committed settings: release LTO off, the system allocator (no allocator
//!    crate in any lockfile, no `global_allocator` in any source).
//! 2. Build the transport path in the dev profile and in release mode, timing the release build
//!    of the `plinth` binary and of the checker. A build folder that did not exist before the
//!    run is reported as cold.
//! 3. Run both builds on the three original Round STLs and compare every output file byte for
//!    byte: release output must equal dev-profile output.
//!
//! It appends one row to `target/release-log.tsv`. Exit: 0 pass; 1 a guard, build or run failed
//! or an output differs; 4 setup error.

use crate::gate::shell_in;
use crate::proc::{self, Exit};
use crate::t;
use std::fs;
use std::path::{Path, PathBuf};

const ROUNDS: [&str; 3] = ["Round 25", "Round 50", "Round 100"];
/// Packages that replace the system allocator.
const ALLOCATOR_CRATES: [&str; 6] = [
    "mimalloc",
    "libmimalloc-sys",
    "tikv-jemallocator",
    "tikv-jemalloc-sys",
    "jemallocator",
    "snmalloc-rs",
];

/// What is wrong with the `[profile.release]` table of a `Cargo.toml` text for "LTO off".
#[must_use]
pub fn lto_problem(cargo_toml: &str) -> Option<String> {
    let mut in_release = false;
    let mut lto: Option<String> = None;
    for line in cargo_toml.lines() {
        let l = line.split('#').next().unwrap_or("").trim();
        if l.starts_with('[') {
            in_release = l == "[profile.release]";
        } else if let Some((_, v)) = l
            .split_once('=')
            .filter(|(k, _)| in_release && k.trim() == "lto")
        {
            lto = Some(v.trim().trim_matches('"').to_owned());
        }
    }
    match lto.as_deref() {
        Some("false" | "off") => None,
        Some(other) => Some(format!("[profile.release] lto = {other}; it must be off")),
        None => {
            Some("[profile.release] has no lto key; write lto = false so it is explicit".to_owned())
        }
    }
}

/// Allocator crates named in a `Cargo.lock` text.
#[must_use]
pub fn allocator_crates(lock: &str) -> Vec<String> {
    lock.lines()
        .filter_map(|l| l.strip_prefix("name = \""))
        .map(|n| n.trim_end_matches('"'))
        .filter(|n| ALLOCATOR_CRATES.contains(n))
        .map(str::to_owned)
        .collect()
}

/// True if a source line installs a global allocator.
#[must_use]
pub fn sets_allocator(line: &str) -> bool {
    let t = line.trim_start();
    !t.starts_with("//") && t.contains(concat!("global_", "allocator"))
}

fn rust_sources(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for e in entries.flatten() {
        let p = e.path();
        let name = e.file_name();
        if p.is_dir() {
            if name != "target" {
                rust_sources(&p, out);
            }
        } else if p.extension().is_some_and(|x| x == "rs") {
            out.push(p);
        }
    }
}

/// Every guard problem under `root`.
fn guards(root: &Path) -> Vec<String> {
    let mut out = Vec::new();
    for ws in [".", "checker"] {
        let dir = root.join(ws);
        match fs::read_to_string(dir.join("Cargo.toml")) {
            Ok(t) => out.extend(lto_problem(&t).map(|p| format!("{ws}/Cargo.toml: {p}"))),
            Err(e) => out.push(format!("{ws}/Cargo.toml: {e}")),
        }
        let lock = fs::read_to_string(dir.join("Cargo.lock")).unwrap_or_default();
        out.extend(
            allocator_crates(&lock)
                .into_iter()
                .map(|n| format!("{ws}/Cargo.lock names the allocator crate {n}")),
        );
    }
    let mut files = Vec::new();
    rust_sources(&root.join("crates"), &mut files);
    rust_sources(&root.join("checker"), &mut files);
    for f in files {
        let text = fs::read_to_string(&f).unwrap_or_default();
        if text.lines().any(sets_allocator) {
            out.push(format!("{} installs a global allocator", f.display()));
        }
    }
    out
}

fn setup(msg: impl std::fmt::Display) -> Exit {
    Exit::setup(format!("release-check: {msg}"))
}

fn fail(msg: impl Into<String>) -> Exit {
    Exit {
        code: 1,
        msg: msg.into(),
    }
}

/// Run `plinth` of one profile on the three originals into `out`; the seconds it took.
fn run_rounds(root: &Path, profile: &str, threads: u32, out: &Path) -> Result<f64, Exit> {
    fs::create_dir_all(out).map_err(setup)?;
    let inputs: Vec<String> = ROUNDS
        .iter()
        .map(|r| format!("'fixtures/round/original/{r}.stl'"))
        .collect();
    let script = format!(
        "target/{profile}/plinth --max-threads {threads} --out-dir '{o}' --report '{o}/report.json' {i}",
        o = out.display(),
        i = inputs.join(" ")
    );
    let name = format!("release-check-run-{profile}");
    shell_in(root, threads, &name, &script, 120)
}

/// The files of `dir` by name, `.chitubox` only.
fn outputs(dir: &Path) -> Result<Vec<(String, Vec<u8>)>, Exit> {
    let mut v = Vec::new();
    let listing = fs::read_dir(dir).map_err(|e| setup(format!("{}: {e}", dir.display())))?;
    for e in listing.flatten() {
        let name = e.file_name().to_string_lossy().into_owned();
        if name.ends_with(".chitubox") {
            v.push((name, fs::read(e.path()).map_err(setup)?));
        }
    }
    v.sort();
    Ok(v)
}

/// Differences between two sets of output files (empty when byte for byte equal).
#[must_use]
pub fn differences(dev: &[(String, Vec<u8>)], rel: &[(String, Vec<u8>)]) -> Vec<String> {
    let names = |v: &[(String, Vec<u8>)]| v.iter().map(|f| f.0.clone()).collect::<Vec<_>>();
    if names(dev) != names(rel) {
        return vec![format!(
            "file sets differ: dev {:?}, release {:?}",
            names(dev),
            names(rel)
        )];
    }
    dev.iter()
        .zip(rel)
        .filter(|(a, b)| a.1 != b.1)
        .map(|(a, b)| {
            format!(
                "{}: dev {} bytes, release {} bytes, contents differ",
                a.0,
                a.1.len(),
                b.1.len()
            )
        })
        .collect()
}

fn append_row(root: &Path, row: &str) {
    use std::io::Write;
    let tsv = root.join("target/release-log.tsv");
    let header = "time\trev\tfolder\trelease_build_plinth_s\trelease_build_checker_s\tdev_run_s\trelease_run_s\tbyte_identical\n";
    let fresh = !tsv.exists();
    let file = fs::OpenOptions::new().create(true).append(true).open(&tsv);
    if let Ok(mut f) = file {
        let _ = f.write_all(format!("{}{row}\n", if fresh { header } else { "" }).as_bytes());
    }
}

/// Build the dev profile and release builds; the release build seconds of `plinth` and of the
/// checker, and whether the release build folders were cold.
fn build_both(root: &Path, th: u32) -> Result<(f64, f64, bool), Exit> {
    let cold =
        !root.join("target/release").exists() || !root.join("checker/target/release").exists();
    let dev = "cargo build --locked -p plinth-cli && (cd checker && cargo build --locked)";
    shell_in(root, th, "release-check-dev", dev, 500)?;
    let main_cmd = "cargo build --release --locked -p plinth-cli";
    let plinth = shell_in(root, th, "release-check-build", main_cmd, 560)?;
    let checker_cmd = "cd checker && cargo build --release --locked";
    let checker = shell_in(root, th, "release-check-build-checker", checker_cmd, 560)?;
    Ok((plinth, checker, cold))
}

fn check(root: &Path) -> Result<(), Exit> {
    let alloc = t::allocation(root).map_err(setup)?;
    let problems = guards(root);
    if !problems.is_empty() {
        return Err(fail(format!(
            "release settings:\n  {}",
            problems.join("\n  ")
        )));
    }
    println!("release-check: LTO off, system allocator (no allocator crate, no global allocator)");
    let th = alloc.threads;
    let (build_plinth, build_checker, cold) = build_both(root, th)?;
    let folder = if cold { "cold" } else { "warm" };
    println!(
        "release-check: release build ({folder} build folder): plinth {build_plinth:.1} s, checker {build_checker:.1} s, threads {th}"
    );
    let tmp = std::env::temp_dir().join(format!("plinth-release-{}", std::process::id()));
    let _ = fs::remove_dir_all(&tmp);
    let dev_s = run_rounds(root, "debug", th, &tmp.join("dev"))?;
    let rel_s = run_rounds(root, "release", th, &tmp.join("release"))?;
    let (dev, rel) = (outputs(&tmp.join("dev"))?, outputs(&tmp.join("release"))?);
    let diff = differences(&dev, &rel);
    println!(
        "release-check: ran the three Round originals: dev profile {dev_s:.2} s, release {rel_s:.2} s ({} files each)",
        dev.len()
    );
    let rev = proc::git(root, &["rev-parse", "--short", "HEAD"]).unwrap_or_default();
    let (stamp, _) = proc::now_stamp();
    let same = diff.is_empty() && dev.len() == ROUNDS.len();
    let row = format!(
        "{stamp}\t{rev}\t{folder}\t{build_plinth:.1}\t{build_checker:.1}\t{dev_s:.2}\t{rel_s:.2}\t{same}"
    );
    append_row(root, &row);
    if same {
        println!("release-check: PASS: release outputs are byte-identical to dev-profile outputs");
        let _ = fs::remove_dir_all(&tmp);
        return Ok(());
    }
    let why = if diff.is_empty() {
        let (want, found) = (ROUNDS.len(), dev.len());
        format!("expected {want} output files, found {found}")
    } else {
        diff.join("\n  ")
    };
    Err(fail(format!(
        "release-check: FAIL (outputs kept in {}):\n  {why}",
        tmp.display()
    )))
}

pub fn main(root: &Path) -> i32 {
    match check(root) {
        Ok(()) => 0,
        Err(e) => {
            eprintln!("{}", e.msg);
            e.code
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lto_must_be_explicitly_off() {
        let ok = "[profile.release]\nopt-level = 3\nlto = false # not measured yet\n[profile.dev]\nlto = true\n";
        assert_eq!(lto_problem(ok), None);
        assert_eq!(lto_problem("[profile.release]\nlto = \"off\"\n"), None);
        for bad in ["thin", "\"fat\"", "true"] {
            let t = format!("[profile.release]\nlto = {bad}\n");
            assert!(lto_problem(&t).is_some(), "{bad}");
        }
        assert!(lto_problem("[profile.release]\nopt-level = 3\n").is_some());
        assert!(lto_problem("[profile.dev]\nlto = false\n").is_some());
    }

    #[test]
    fn allocator_crates_in_a_lockfile_are_found() {
        let lock = "[[package]]\nname = \"lexopt\"\nversion = \"0.3.1\"\n\n[[package]]\nname = \"mimalloc\"\nversion = \"0.1\"\n";
        assert_eq!(allocator_crates(lock), ["mimalloc"]);
        assert!(allocator_crates("[[package]]\nname = \"nix\"\n").is_empty());
    }

    #[test]
    fn a_global_allocator_line_is_found_outside_comments() {
        let attr = format!("#[{}{}]", "global_", "allocator");
        assert!(sets_allocator(&format!("{attr} static A: X = X;")));
        assert!(!sets_allocator(&format!("// {attr}")));
        assert!(!sets_allocator("let a = 1;"));
    }

    #[test]
    fn outputs_must_match_name_for_name_and_byte_for_byte() {
        let a = vec![("x.chitubox".to_owned(), vec![1, 2, 3])];
        assert!(differences(&a, &a.clone()).is_empty());
        let b = vec![("x.chitubox".to_owned(), vec![1, 2, 4])];
        assert_eq!(differences(&a, &b).len(), 1);
        let c = vec![("y.chitubox".to_owned(), vec![1, 2, 3])];
        assert!(differences(&a, &c)[0].contains("file sets differ"));
        assert_eq!(differences(&a, &[]).len(), 1);
    }
}
