//! `devloop check-independence`: the checker shares no code with the generator
//! (design dev-loop.md section 4.4, items 2 and 3).
//!
//! 1. Resolve the checker's complete dependency graph (`cargo metadata --locked --offline`, in
//!    `checker/`, so normal, build and development dependencies all appear) and read it with jq.
//!    Every package must be a path package whose canonical location lies inside `checker/`, or a
//!    registry package listed in `checker/approved-registry.tsv` (name, version, source; an
//!    absent file means none). Git sources and outside path packages are rejected. The checker
//!    must also be its own workspace.
//!    The source file of every target (library, binaries, tests, build script) of a path package
//!    must lie inside `checker/` too.
//! 2. Lex every regular file under `checker/` whatever its extension (so a `.inc` file pulled in
//!    by `#[path]` is scanned too), except the top-level `target/` and `Cargo.lock`
//!    (`rustscan`, token based, so spacing and comments cannot hide anything). Reject every
//!    occurrence of the names `include`, `include_str` and `include_bytes` outside comments and
//!    string literals, in any context, and every `#[path = ...]` attribute. A symbolic link that
//!    leaves `checker/` is rejected.
//!
//! Threat model: the check guards against code sharing by accident, not against a deliberate
//! adversary; text-scanner bypasses beyond that are out of scope.
//!
//! Exit codes: 0 pass; 1 violation (or a dependency graph that cannot be resolved as committed);
//! 4 setup error (no cargo or jq).

use crate::proc;
use crate::rustscan;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

const JQ_FILTER: &str = r#". as $m
| ([$m.resolve.nodes[].id]) as $ids
| (["ROOT", $m.workspace_root] | @tsv),
  ($m.packages[]
   | select(.id as $i | $ids | index($i))
   | . as $p
   | ([$p.name, $p.version, ($p.source // ""), $p.manifest_path] | @tsv),
     (select($p.source == null) | $p.targets[]
      | ["TARGET", ($p.name + " " + $p.version), .src_path] | @tsv))"#;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Pkg {
    pub name: String,
    pub version: String,
    /// Empty for a path package.
    pub source: String,
    pub manifest: PathBuf,
    /// Source files of the targets of a path package (library, binaries, tests, build script).
    pub targets: Vec<PathBuf>,
}

/// What jq printed: the workspace root, and one `Pkg` per resolved package.
pub fn parse_graph(tsv: &str) -> Result<(Option<PathBuf>, Vec<Pkg>), String> {
    let (mut root, mut pkgs): (_, Vec<Pkg>) = (None, Vec::new());
    for line in tsv.lines().filter(|l| !l.is_empty()) {
        let f: Vec<&str> = line.split('\t').collect();
        match f.as_slice() {
            ["ROOT", dir] => root = Some(PathBuf::from(dir)),
            ["TARGET", id, src] => {
                let owner = pkgs
                    .iter_mut()
                    .rfind(|p| format!("{} {}", p.name, p.version) == *id)
                    .ok_or_else(|| format!("target line before its package: {line}"))?;
                owner.targets.push(PathBuf::from(src));
            }
            [name, version, source, manifest] => pkgs.push(Pkg {
                name: (*name).to_owned(),
                version: (*version).to_owned(),
                source: (*source).to_owned(),
                manifest: PathBuf::from(manifest),
                targets: Vec::new(),
            }),
            _ => return Err(format!("unexpected line from jq: {line}")),
        }
    }
    Ok((root, pkgs))
}

/// `name version source` lines; `#` starts a comment.
#[must_use]
pub fn parse_approved(text: &str) -> Vec<(String, String, String)> {
    text.lines()
        .map(|l| l.split('#').next().unwrap_or("").trim())
        .filter(|l| !l.is_empty())
        .filter_map(|l| {
            let mut it = l.split_whitespace();
            Some((
                it.next()?.to_owned(),
                it.next()?.to_owned(),
                it.next()?.to_owned(),
            ))
        })
        .collect()
}

fn canon(p: &Path) -> PathBuf {
    std::fs::canonicalize(p).unwrap_or_else(|_| p.to_path_buf())
}

/// Violations in the resolved graph. `checker` is the checker directory.
#[must_use]
pub fn judge(
    root: Option<&Path>,
    pkgs: &[Pkg],
    checker: &Path,
    approved: &[(String, String, String)],
) -> Vec<String> {
    let checker = canon(checker);
    let mut bad = Vec::new();
    match root {
        Some(r) if canon(r) == checker => {}
        Some(r) => bad.push(format!(
            "the checker is not its own workspace: its workspace root is {}",
            r.display()
        )),
        None => bad.push("cargo metadata did not report a workspace root".to_owned()),
    }
    for p in pkgs {
        let id = format!("{} {}", p.name, p.version);
        if p.source.is_empty() {
            if !canon(&p.manifest).starts_with(&checker) {
                bad.push(format!(
                    "{id} is a path package outside checker/ ({})",
                    p.manifest.display()
                ));
            }
            for t in p.targets.iter().filter(|t| !canon(t).starts_with(&checker)) {
                bad.push(format!(
                    "{id} has a target source outside checker/ ({})",
                    t.display()
                ));
            }
        } else if p.source.starts_with("registry+") {
            let listed = approved
                .iter()
                .any(|(n, v, s)| *n == p.name && *v == p.version && *s == p.source);
            if !listed {
                bad.push(format!(
                    "{id} ({}) is a registry package that is not in checker/approved-registry.tsv",
                    p.source
                ));
            }
        } else {
            bad.push(format!("{id} has a forbidden source: {}", p.source));
        }
    }
    bad
}

/// Every regular file under the checker (any extension), except its top-level `target/` and
/// `Cargo.lock`, into `files`. A symbolic link whose real location is outside the checker is a
/// violation (it would put outside code into the checker without any text that names it).
fn walk_checker(checker: &Path, dir: &Path, files: &mut Vec<PathBuf>, bad: &mut Vec<String>) {
    let Ok(rd) = std::fs::read_dir(dir) else {
        return;
    };
    let mut entries: Vec<PathBuf> = rd.flatten().map(|e| e.path()).collect();
    entries.sort();
    for p in entries {
        let is_link = std::fs::symlink_metadata(&p).is_ok_and(|m| m.file_type().is_symlink());
        if is_link {
            if !canon(&p).starts_with(checker) {
                bad.push(format!(
                    "{} is a symbolic link to a place outside checker/",
                    p.display()
                ));
            }
        } else if p.is_dir() {
            if !(dir == checker && p.file_name().is_some_and(|n| n == "target")) {
                walk_checker(checker, &p, files, bad);
            }
        } else if p.is_file()
            && !(dir == checker && p.file_name().is_some_and(|n| n == "Cargo.lock"))
        {
            files.push(p);
        }
    }
}

/// Run jq over `input` with `JQ_FILTER`.
fn jq(input: &[u8]) -> Result<String, String> {
    let mut child = Command::new("jq")
        .args(["-r", JQ_FILTER])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("cannot run jq: {e}"))?;
    let mut stdin = child.stdin.take().ok_or("jq has no stdin")?;
    let data = input.to_vec();
    let writer = std::thread::spawn(move || stdin.write_all(&data));
    let out = child
        .wait_with_output()
        .map_err(|e| format!("jq failed: {e}"))?;
    let _ = writer.join();
    if out.status.success() {
        Ok(String::from_utf8_lossy(&out.stdout).into_owned())
    } else {
        Err(format!(
            "jq: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        ))
    }
}

/// The checker's resolved graph as committed (`--locked`: a dependency change that Cargo.lock
/// does not record is itself a failure).
fn resolve(checker: &Path) -> Result<(Option<PathBuf>, Vec<Pkg>), (i32, String)> {
    let meta = Command::new("cargo")
        .args(["metadata", "--locked", "--offline", "--format-version", "1"])
        .current_dir(checker)
        .stdin(Stdio::null())
        .output()
        .map_err(|e| (4, format!("cannot run cargo: {e}")))?;
    if !meta.status.success() {
        let err = String::from_utf8_lossy(&meta.stderr);
        let tail: Vec<&str> = err.lines().rev().take(6).collect();
        let tail: Vec<&str> = tail.into_iter().rev().collect();
        return Err((
            1,
            format!(
                "cannot resolve the checker's dependency graph as committed: {}",
                tail.join(" | ")
            ),
        ));
    }
    let tsv = jq(&meta.stdout).map_err(|e| (4, e))?;
    parse_graph(&tsv).map_err(|e| (1, e))
}

pub fn main(root: &Path) -> i32 {
    let checker = root.join("checker");
    if !checker.join("Cargo.toml").is_file() {
        eprintln!("SETUP ERROR: {} has no Cargo.toml", checker.display());
        return 4;
    }
    if !proc::has_tool("jq") {
        eprintln!("SETUP ERROR: jq is missing (brew install jq)");
        return 4;
    }
    let (ws, pkgs) = match resolve(&checker) {
        Ok(v) => v,
        Err((4, msg)) => {
            eprintln!("SETUP ERROR: {msg}");
            return 4;
        }
        Err((_, msg)) => {
            eprintln!("FAIL: {msg}\ncheck-independence: FAIL");
            return 1;
        }
    };
    let approved = parse_approved(
        &std::fs::read_to_string(checker.join("approved-registry.tsv")).unwrap_or_default(),
    );
    let mut bad = judge(ws.as_deref(), &pkgs, &checker, &approved);
    let mut files = Vec::new();
    let real = canon(&checker);
    walk_checker(&real, &real, &mut files, &mut bad);
    for f in &files {
        let rel = f
            .strip_prefix(canon(root))
            .unwrap_or(f)
            .display()
            .to_string();
        // Lossy: a binary file (a fixture) is scanned as text rather than rejected.
        match std::fs::read(f) {
            Ok(bytes) => bad.extend(rustscan::scan_text(&rel, &String::from_utf8_lossy(&bytes))),
            Err(e) => bad.push(format!("{rel}: cannot read the source file: {e}")),
        }
    }
    if bad.is_empty() {
        println!(
            "check-independence: PASS ({} resolved packages, {} source files scanned)",
            pkgs.len(),
            files.len()
        );
        0
    } else {
        for b in &bad {
            eprintln!("FAIL: {b}");
        }
        eprintln!("check-independence: FAIL ({} violations)", bad.len());
        1
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pkg(name: &str, source: &str, manifest: &str) -> Pkg {
        Pkg {
            name: name.into(),
            version: "1.0.0".into(),
            source: source.into(),
            manifest: manifest.into(),
            targets: Vec::new(),
        }
    }

    const REG: &str = "registry+https://github.com/rust-lang/crates.io-index";

    #[test]
    fn the_checker_alone_passes() {
        let pkgs = [pkg("plinth-check", "", "/r/checker/Cargo.toml")];
        assert!(
            judge(
                Some(Path::new("/r/checker")),
                &pkgs,
                Path::new("/r/checker"),
                &[]
            )
            .is_empty()
        );
    }

    #[test]
    fn a_path_package_outside_the_checker_fails_even_when_indirect() {
        let pkgs = [
            pkg("plinth-check", "", "/r/checker/Cargo.toml"),
            pkg("helper", "", "/r/checker/helper/Cargo.toml"),
            pkg("plinth-geom", "", "/r/crates/geom/Cargo.toml"),
        ];
        let bad = judge(
            Some(Path::new("/r/checker")),
            &pkgs,
            Path::new("/r/checker"),
            &[],
        );
        assert_eq!(bad.len(), 1, "{bad:?}");
        assert!(bad[0].contains("plinth-geom") && bad[0].contains("outside checker/"));
    }

    #[test]
    fn registry_packages_need_an_exact_approved_entry() {
        let pkgs = [
            pkg("plinth-check", "", "/r/checker/Cargo.toml"),
            pkg("tiny", REG, "/h/tiny/Cargo.toml"),
        ];
        let ws = Some(Path::new("/r/checker"));
        let none = judge(ws, &pkgs, Path::new("/r/checker"), &[]);
        assert_eq!(none.len(), 1, "{none:?}");
        let ok = [("tiny".to_owned(), "1.0.0".to_owned(), REG.to_owned())];
        assert!(judge(ws, &pkgs, Path::new("/r/checker"), &ok).is_empty());
        let wrong_version = [("tiny".to_owned(), "1.0.1".to_owned(), REG.to_owned())];
        assert_eq!(
            judge(ws, &pkgs, Path::new("/r/checker"), &wrong_version).len(),
            1
        );
    }

    #[test]
    fn git_sources_and_a_shared_workspace_fail() {
        let pkgs = [pkg("dep", "git+https://example.com/x", "/g/Cargo.toml")];
        let bad = judge(Some(Path::new("/r")), &pkgs, Path::new("/r/checker"), &[]);
        assert_eq!(bad.len(), 2, "{bad:?}");
        assert!(bad.iter().any(|b| b.contains("forbidden source")));
        assert!(bad.iter().any(|b| b.contains("not its own workspace")));
    }

    #[test]
    fn graph_and_approved_lists_parse() {
        let tsv = format!(
            "ROOT\t/r/checker\nplinth-check\t0.0.0\t\t/r/checker/Cargo.toml\ntiny\t1.0.0\t{REG}\t/h/Cargo.toml\n"
        );
        let (root, pkgs) = parse_graph(&tsv).expect("parses");
        assert_eq!(root, Some(PathBuf::from("/r/checker")));
        assert_eq!(pkgs.len(), 2);
        assert!(pkgs[0].source.is_empty() && pkgs[1].source == REG);
        assert!(parse_graph("garbage line\n").is_err());
        let a = parse_approved(&format!("# comment\ntiny 1.0.0 {REG} # why\n\n"));
        assert_eq!(
            a,
            vec![("tiny".to_owned(), "1.0.0".to_owned(), REG.to_owned())]
        );
    }

    #[test]
    fn a_target_source_outside_the_checker_fails() {
        let mut lib = pkg("plinth-check", "", "/r/checker/Cargo.toml");
        lib.targets = vec![
            "/r/checker/src/lib.rs".into(),
            "/r/crates/geom/src/lib.rs".into(),
        ];
        let bad = judge(
            Some(Path::new("/r/checker")),
            &[lib],
            Path::new("/r/checker"),
            &[],
        );
        assert_eq!(bad.len(), 1, "{bad:?}");
        assert!(bad[0].contains("crates/geom/src/lib.rs"), "{bad:?}");
    }

    #[test]
    fn a_symbolic_link_out_of_the_checker_fails_and_one_inside_passes() {
        let tmp = std::env::temp_dir().join(format!("devloop-indep-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        let checker = tmp.join("checker");
        std::fs::create_dir_all(checker.join("src")).expect("mkdir");
        std::fs::create_dir_all(checker.join("target")).expect("mkdir");
        std::fs::write(tmp.join("outside.rs"), "pub fn f() {}\n").expect("write");
        std::fs::write(checker.join("src/a.rs"), "pub fn a() {}\n").expect("write");
        std::fs::write(checker.join("target/skipped.rs"), "\n").expect("write");
        std::fs::write(checker.join("src/data.inc"), "\n").expect("write");
        std::fs::write(checker.join("Cargo.lock"), "\n").expect("write");
        let real = canon(&checker);
        let (mut files, mut bad) = (Vec::new(), Vec::new());
        std::os::unix::fs::symlink(checker.join("src/a.rs"), checker.join("src/inside.rs"))
            .expect("link");
        walk_checker(&real, &real, &mut files, &mut bad);
        assert!(bad.is_empty(), "{bad:?}");
        assert_eq!(
            files.len(),
            2,
            "a.rs and data.inc only: target/ and Cargo.lock are skipped, links are not followed: {files:?}"
        );
        assert!(
            files.iter().any(|f| f.ends_with("src/data.inc")),
            "{files:?}"
        );
        std::os::unix::fs::symlink(tmp.join("outside.rs"), checker.join("src/out.rs"))
            .expect("link");
        walk_checker(&real, &real, &mut Vec::new(), &mut bad);
        assert_eq!(bad.len(), 1, "{bad:?}");
        assert!(bad[0].contains("out.rs"), "{bad:?}");
        let _ = std::fs::remove_dir_all(&tmp);
    }

    /// A throwaway repo root with a dependency-free `checker/` whose lib is `lib_rs`, plus
    /// `extra` (path under `checker/`, text) files.
    fn temp_root(tag: &str, lib_rs: &str, extra: &[(&str, &str)]) -> PathBuf {
        let root = std::env::temp_dir().join(format!("devloop-e2e-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let checker = root.join("checker");
        std::fs::create_dir_all(checker.join("src")).expect("mkdir");
        std::fs::write(
            checker.join("Cargo.toml"),
            "[workspace]\nmembers = [\".\"]\n\n[package]\nname = \"probe\"\nversion = \"0.0.0\"\n\
             edition = \"2021\"\n\n[lib]\npath = \"src/lib.rs\"\n",
        )
        .expect("write");
        std::fs::write(
            checker.join("Cargo.lock"),
            "version = 4\n\n[[package]]\nname = \"probe\"\nversion = \"0.0.0\"\n",
        )
        .expect("write");
        std::fs::write(checker.join("src/lib.rs"), lib_rs).expect("write");
        for (rel, text) in extra {
            let p = checker.join(rel);
            std::fs::create_dir_all(p.parent().expect("parent")).expect("mkdir");
            std::fs::write(p, text).expect("write");
        }
        root
    }

    #[test]
    fn end_to_end_a_clean_checker_passes() {
        if !proc::has_tool("jq") || !proc::has_tool("cargo") {
            return;
        }
        let root = temp_root("clean", "pub fn f() {}\n", &[("src/notes.txt", "hello\n")]);
        assert_eq!(main(&root), 0);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn end_to_end_a_path_attribute_to_an_inc_probe_fails() {
        if !proc::has_tool("jq") || !proc::has_tool("cargo") {
            return;
        }
        // lib.rs pulls in an `.inc` file by `#[path]`; the `.inc` file includes an outside file.
        let root = temp_root(
            "path-inc",
            "#[path = \"probe.inc\"]\nmod probe;\n",
            &[("src/probe.inc", "pub const X: &str = \"outside\";\n")],
        );
        assert_ne!(main(&root), 0, "a #[path] attribute must fail the check");
        // The same tree without the attribute but with the name in the `.inc` file alone.
        let root2 = temp_root(
            "inc-only",
            "pub fn f() {}\n",
            &[(
                "src/probe.inc",
                "pub const X: &str = include_str!(\"../../limits.toml\");\n",
            )],
        );
        assert_ne!(
            main(&root2),
            0,
            "an include name in a non-.rs file must fail"
        );
        let _ = std::fs::remove_dir_all(&root);
        let _ = std::fs::remove_dir_all(&root2);
    }

    #[test]
    fn jq_filter_reads_real_cargo_metadata_shape() {
        if !proc::on_path("jq") {
            return;
        }
        let meta = r#"{"workspace_root":"/r/checker","resolve":{"nodes":[{"id":"a 0.0.0"}]},
            "packages":[{"id":"a 0.0.0","name":"a","version":"0.0.0","source":null,"manifest_path":"/r/checker/Cargo.toml",
                         "targets":[{"name":"a","src_path":"/r/checker/src/lib.rs"},{"name":"b","src_path":"/r/checker/src/main.rs"}]},
                        {"id":"unused 1.0.0","name":"unused","version":"1.0.0","source":null,"manifest_path":"/x/Cargo.toml",
                         "targets":[{"name":"u","src_path":"/x/src/lib.rs"}]}]}"#;
        let tsv = jq(meta.as_bytes()).expect("jq runs");
        let (root, pkgs) = parse_graph(&tsv).expect("parses");
        assert_eq!(root, Some(PathBuf::from("/r/checker")));
        assert_eq!(
            pkgs.len(),
            1,
            "packages outside the resolved graph are ignored: {pkgs:?}"
        );
        assert_eq!(pkgs[0].targets.len(), 2, "{pkgs:?}");
    }
}
