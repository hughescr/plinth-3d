//! Readers agree (design dev-loop.md section 4.4 item 6, section 10 step 5).
//!
//! The checker is a separate program with its own reader. This test runs `plinth-check dump`
//! on every native `.chitubox` file and compares the numbers it prints with what the production
//! reader (`plinth-chitubox`) holds: model count and names, translate, mesh triangle count,
//! support record count, and for every record its type, end points and radii, bit for bit.
//! Only text crosses between the two readers; no type and no code is shared.
//!
//! Agreement shows both readers decode the same bytes the same way. It does not show that
//! either is right about geometry.

use plinth_chitubox::{Project, parse};
use std::path::{Path, PathBuf};
use std::process::Command;

/// The one native file that is not a clean ChituBox save (format reference section 14): the
/// checker reports it as not clean (exit 1) but still prints its numbers.
const MUDSKIPPER: &str = "morel_mudskipper_supports.chitubox";

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// The built checker program. The runner (`scripts/t`) builds it once before nextest starts
/// and passes its absolute path in `PLINTH_CHECK_BIN`. The test never builds it and never
/// skips: with no path, or a path to nothing, it fails.
#[allow(
    clippy::disallowed_methods,
    reason = "test-only read of the checker path set by scripts/t"
)]
fn checker_binary() -> PathBuf {
    let path = std::env::var_os("PLINTH_CHECK_BIN")
        .map(PathBuf::from)
        .expect("run via scripts/t; PLINTH_CHECK_BIN not set");
    assert!(
        path.is_file(),
        "PLINTH_CHECK_BIN names a file that is not there: {} (run via scripts/t)",
        path.display()
    );
    path
}

/// Every `.chitubox` file under `test-data/chitubox-native`, sorted.
fn native_files() -> Vec<PathBuf> {
    fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
        for entry in std::fs::read_dir(dir).expect("read native dir").flatten() {
            let p = entry.path();
            if p.is_dir() {
                walk(&p, out);
            } else if p.extension().is_some_and(|e| e == "chitubox") {
                out.push(p);
            }
        }
    }
    let mut out = Vec::new();
    walk(&repo_root().join("test-data/chitubox-native"), &mut out);
    out.sort();
    out
}

fn file_name(p: &Path) -> String {
    p.file_name()
        .map_or_else(String::new, |n| n.to_string_lossy().into_owned())
}

/// Run `plinth-check dump` on `path`: (exit code, standard output).
fn run_dump(path: &Path) -> (i32, String) {
    let out = Command::new(checker_binary())
        .arg("dump")
        .arg(path)
        .output()
        .expect("run plinth-check dump");
    (
        out.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&out.stdout).into_owned(),
    )
}

/// `f32` values written in shortest decimal form read back to the same bits.
fn bits(tokens: &[&str]) -> Result<Vec<u32>, String> {
    tokens
        .iter()
        .map(|t| {
            t.parse::<f32>()
                .map(f32::to_bits)
                .map_err(|e| format!("bad number {t:?}: {e}"))
        })
        .collect()
}

/// What `dump` printed for one model line: name, translate bits, triangles.
struct DumpedModel {
    name: String,
    translate: Vec<u32>,
    triangles: usize,
}

/// `model <i> "<name>": translate=(x, y, z) mesh=[a, b) triangles=<n>`
fn parse_model_line(line: &str, index: usize) -> Result<DumpedModel, String> {
    let head = format!("model {index} \"");
    let rest = line
        .strip_prefix(&head)
        .ok_or_else(|| format!("expected a line starting {head:?}, got {line:?}"))?;
    let (name, rest) = rest
        .split_once("\": translate=(")
        .ok_or_else(|| format!("no translate in {line:?}"))?;
    let (vec3, rest) = rest
        .split_once(')')
        .ok_or_else(|| format!("no closing bracket in {line:?}"))?;
    let parts: Vec<&str> = vec3.split(", ").collect();
    let triangles = rest
        .rsplit_once("triangles=")
        .and_then(|(_, n)| n.trim().parse::<usize>().ok())
        .ok_or_else(|| format!("no triangle count in {line:?}"))?;
    Ok(DumpedModel {
        name: name.to_owned(),
        translate: bits(&parts)?,
        triangles,
    })
}

/// `record <model> <index> <type> <start x y z> <end x y z> <r1> <r2>`
fn parse_record_line(line: &str) -> Result<(usize, usize, u32, Vec<u32>), String> {
    let t: Vec<&str> = line.split(' ').collect();
    let [_, model, index, kind, nums @ ..] = t.as_slice() else {
        return Err(format!("short record line {line:?}"));
    };
    if nums.len() != 8 {
        return Err(format!("record line needs 8 numbers: {line:?}"));
    }
    let int = |s: &str| s.parse::<usize>().map_err(|e| format!("{line:?}: {e}"));
    let kind = kind.parse::<u32>().map_err(|e| format!("{line:?}: {e}"))?;
    Ok((int(model)?, int(index)?, kind, bits(nums)?))
}

/// The numbers the production reader holds for a record, in dump order.
fn record_bits(r: &plinth_chitubox::SupportRecord) -> Vec<u32> {
    let [sx, sy, sz] = r.start;
    let [ex, ey, ez] = r.end;
    [sx, sy, sz, ex, ey, ez, r.r1, r.r2]
        .map(f32::to_bits)
        .to_vec()
}

/// Differences between the checker's dump text and the production reader's project.
/// Empty means the two readers agree.
fn disagreements(dump: &str, project: &Project) -> Vec<String> {
    let mut bad = Vec::new();
    let model_lines: Vec<&str> = dump.lines().filter(|l| l.starts_with("model ")).collect();
    if model_lines.len() != project.models.len() {
        bad.push(format!(
            "model count: checker {} production {}",
            model_lines.len(),
            project.models.len()
        ));
        return bad;
    }
    let mut records = Vec::new();
    for l in dump.lines().filter(|l| l.starts_with("record ")) {
        match parse_record_line(l) {
            Ok(r) => records.push(r),
            Err(e) => bad.push(e),
        }
    }
    for (i, (line, m)) in model_lines.iter().zip(&project.models).enumerate() {
        match parse_model_line(line, i) {
            Ok(d) => model_differences(i, &d, m, &mut bad),
            Err(e) => bad.push(e),
        }
        let mine: Vec<_> = records.iter().filter(|r| r.0 == i).collect();
        let theirs = m.supports.as_ref().map_or(&[][..], |s| &s.records[..]);
        record_differences(i, &mine, theirs, &mut bad);
    }
    let known: usize = project
        .models
        .iter()
        .map(|m| m.supports.as_ref().map_or(0, |s| s.records.len()))
        .sum();
    if records.len() != known {
        bad.push(format!(
            "total records: checker {} production {known}",
            records.len()
        ));
    }
    bad
}

fn model_differences(i: usize, d: &DumpedModel, m: &plinth_chitubox::Model, bad: &mut Vec<String>) {
    if d.name != m.name {
        bad.push(format!("model {i} name: {:?} vs {:?}", d.name, m.name));
    }
    if d.translate != m.translate.map(f32::to_bits) {
        bad.push(format!(
            "model {i} translate bits: {:?} vs {:?}",
            d.translate, m.translate
        ));
    }
    if d.triangles != m.mesh.len() {
        bad.push(format!(
            "model {i} triangles: checker {} production {}",
            d.triangles,
            m.mesh.len()
        ));
    }
}

fn record_differences(
    model: usize,
    mine: &[&(usize, usize, u32, Vec<u32>)],
    theirs: &[plinth_chitubox::SupportRecord],
    bad: &mut Vec<String>,
) {
    if mine.len() != theirs.len() {
        bad.push(format!(
            "model {model} record count: checker {} production {}",
            mine.len(),
            theirs.len()
        ));
        return;
    }
    for (k, (c, p)) in mine.iter().zip(theirs).enumerate() {
        let (_, index, kind, nums) = c;
        if *index != k {
            bad.push(format!("model {model} record {k}: checker index {index}"));
        }
        if *kind != p.record_type.0 {
            bad.push(format!(
                "model {model} record {k} type: checker {kind} production {}",
                p.record_type.0
            ));
        }
        if *nums != record_bits(p) {
            bad.push(format!(
                "model {model} record {k} end points or radii differ"
            ));
        }
    }
}

fn load(path: &Path) -> Project {
    let bytes = std::fs::read(path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    parse(&bytes).unwrap_or_else(|e| panic!("{}: production reader: {e}", path.display()))
}

#[test]
fn native_checker_and_production_readers_agree_on_every_file() {
    let files = native_files();
    assert!(files.len() >= 15, "found only {} native files", files.len());
    let (mut models, mut records, mut triangles) = (0, 0, 0);
    for p in &files {
        let who = file_name(p);
        let project = load(p);
        let (code, dump) = run_dump(p);
        let want = i32::from(who == MUDSKIPPER);
        assert_eq!(code, want, "{who}: checker dump exit code\n{dump}");
        let bad = disagreements(&dump, &project);
        assert!(bad.is_empty(), "{who}: readers disagree: {bad:#?}");
        models += project.models.len();
        for m in &project.models {
            triangles += m.mesh.len();
            records += m.supports.as_ref().map_or(0, |s| s.records.len());
        }
    }
    // The comparison must have looked at real content, not an empty set.
    assert!(
        models >= 15 && records >= 500 && triangles >= 20_000,
        "compared only {models} models, {records} records, {triangles} triangles"
    );
}

/// Sabotage of the readers-agree test: the comparison must notice each kind of difference.
#[test]
fn broken_readers_agree_notices_each_kind_of_difference() {
    let path = repo_root().join("test-data/chitubox-native/samples/03-round25-supports.chitubox");
    let project = load(&path);
    let (_, dump) = run_dump(&path);
    assert!(
        disagreements(&dump, &project).is_empty(),
        "unchanged dump must agree"
    );
    let first_record = dump
        .lines()
        .find(|l| l.starts_with("record 0 5 "))
        .expect("record 5 of model 0")
        .to_owned();
    let tokens: Vec<&str> = first_record.split(' ').collect();
    let mut other_kind = tokens.clone();
    other_kind[3] = "99";
    let mut other_radius = tokens.clone();
    other_radius[11] = "0.123456";
    let mut other_end = tokens.clone();
    other_end[7] = "1234.5";
    let model_line = dump
        .lines()
        .find(|l| l.starts_with("model 0 "))
        .expect("model line")
        .to_owned();
    let cases: [(&str, String, String); 8] = [
        ("type", first_record.clone(), other_kind.join(" ")),
        ("radius", first_record.clone(), other_radius.join(" ")),
        ("end point", first_record.clone(), other_end.join(" ")),
        ("record removed", format!("{first_record}\n"), String::new()),
        (
            "triangle count",
            model_line.clone(),
            model_line.replace("triangles=2996", "triangles=2995"),
        ),
        (
            "translate",
            model_line.clone(),
            model_line.replace("translate=(", "translate=(1"),
        ),
        (
            "name",
            model_line.clone(),
            model_line.replace("Round 25.stl", "Round 26.stl"),
        ),
        ("model removed", format!("{model_line}\n"), String::new()),
    ];
    for (what, from, to) in cases {
        assert!(
            dump.contains(&from),
            "{what}: the text to change is missing"
        );
        let changed = dump.replacen(&from, &to, 1);
        assert!(
            !disagreements(&changed, &project).is_empty(),
            "{what}: a changed dump was not noticed"
        );
    }
}
