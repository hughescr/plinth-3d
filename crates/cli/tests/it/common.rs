//! Shared helpers: scratch folders, synthetic STL files, a fake checker.

use plinth_cli::run::Config;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

/// The checkout root.
pub fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// A fresh, empty scratch folder under the system temp folder.
pub fn scratch(tag: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("plinth-cli-it-{tag}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&d);
    fs::create_dir_all(&d).expect("make scratch folder");
    d
}

/// Sorted names of the entries of `dir`, hidden ones included.
pub fn names(dir: &Path) -> Vec<String> {
    let mut v: Vec<String> = fs::read_dir(dir)
        .expect("read dir")
        .map(|e| e.expect("entry").file_name().to_string_lossy().into_owned())
        .collect();
    v.sort();
    v
}

/// A binary STL of an axis-aligned box with its low corner at `at` and side `s`.
pub fn box_stl(at: [f32; 3], s: f32) -> Vec<u8> {
    let p = |i: usize| {
        [
            at[0] + if i & 1 != 0 { s } else { 0.0 },
            at[1] + if i & 2 != 0 { s } else { 0.0 },
            at[2] + if i & 4 != 0 { s } else { 0.0 },
        ]
    };
    let quads = [
        [0, 2, 3, 1],
        [4, 5, 7, 6],
        [0, 1, 5, 4],
        [2, 6, 7, 3],
        [0, 4, 6, 2],
        [1, 3, 7, 5],
    ];
    let mut tris: Vec<[[f32; 3]; 3]> = Vec::new();
    for q in quads {
        tris.push([p(q[0]), p(q[1]), p(q[2])]);
        tris.push([p(q[0]), p(q[2]), p(q[3])]);
    }
    let mut out = vec![0_u8; 80];
    out.extend_from_slice(&u32::try_from(tris.len()).expect("count").to_le_bytes());
    for t in &tris {
        out.extend_from_slice(&[0; 12]);
        for v in t.iter().flatten() {
            out.extend_from_slice(&v.to_le_bytes());
        }
        out.extend_from_slice(&[0; 2]);
    }
    out
}

/// Write a box STL at `dir/name` and return its path.
pub fn write_box(dir: &Path, name: &str, side: f32) -> PathBuf {
    fs::create_dir_all(dir).expect("make input folder");
    let p = dir.join(name);
    fs::write(&p, box_stl([1.0, 2.0, 3.0], side)).expect("write stl");
    p
}

/// A stand-in for `plinth-check` that reads the real size and SHA-256 of the file it is
/// asked about and answers in the checker's report shape, with these departures.
#[derive(Debug, Clone)]
pub struct Fake {
    /// The overall verdict word in the report.
    pub verdict: &'static str,
    /// The process exit code.
    pub code: i32,
    /// Added to the size it reports.
    pub size_error: u64,
    /// Report the SHA-256 of that many zero bytes of the same length instead of the file's:
    /// another file of the same size.
    pub hash_other_same_size: bool,
    /// `(rule id, status)` for every rule it lists, in order.
    pub rules: Vec<(&'static str, &'static str)>,
    /// The chitubox input's fields after bytes and hash.
    pub file_fields: &'static str,
}

/// The fields of a cleanly read chitubox input.
pub const CLEAN_FILE: &str =
    r#""readable":true,"regions_tile_the_file":true,"anomalies":[],"layout_walk_differences":[]"#;

impl Fake {
    /// What the real checker says today about a clean transport file: every rule unverified.
    pub fn honest() -> Self {
        Self {
            verdict: "unverified",
            code: 2,
            size_error: 0,
            hash_other_same_size: false,
            rules: ["H1", "H2", "H3", "H4", "H5", "H6", "H7", "H8", "H9"]
                .map(|id| (id, "unverified"))
                .to_vec(),
            file_fields: CLEAN_FILE,
        }
    }

    /// Replace the status of one rule.
    pub fn with_rule(mut self, id: &str, status: &'static str) -> Self {
        for r in &mut self.rules {
            if r.0 == id {
                r.1 = status;
            }
        }
        self
    }

    /// Write the script into `dir` under `name` and return its path.
    pub fn install(&self, dir: &Path, name: &str) -> PathBuf {
        let rules: Vec<String> = self
            .rules
            .iter()
            .map(|(id, status)| {
                format!(
                    "{{\"id\":\"{id}\",\"name\":\"x\",\"status\":\"{status}\",\"reason\":\"fake\"}}"
                )
            })
            .collect();
        let hash = if self.hash_other_same_size {
            "head -c \"$n\" /dev/zero | shasum -a 256"
        } else {
            "shasum -a 256 < \"$3\""
        };
        let script = format!(
            "#!/bin/sh\n\
             # args: check --chitubox F --limits L --stl S --out O\n\
             n=$(wc -c < \"$3\" | tr -d ' ')\n\
             h=$({hash} | cut -d' ' -f1)\n\
             n=$((n + {size_error}))\n\
             cat > \"$9\" <<EOF\n\
             {{\"report_version\":1,\"verdict\":\"{verdict}\",\"inputs\":{{\"chitubox\":{{\"bytes\":$n,\"sha256\":\"$h\",{file}}},\"stl\":{{\"readable\":true}}}},\"rules\":[{rules}]}}\n\
             EOF\n\
             exit {code}\n",
            size_error = self.size_error,
            verdict = self.verdict,
            file = self.file_fields,
            rules = rules.join(","),
            code = self.code,
        );
        let p = dir.join(name);
        fs::write(&p, script).expect("write fake checker");
        fs::set_permissions(&p, fs::Permissions::from_mode(0o755)).expect("chmod");
        p
    }
}

/// An honest-shaped fake answering `verdict` with exit `code` (see [`Fake`]).
pub fn fake_checker(dir: &Path, verdict: &'static str, code: i32, size_error: u64) -> PathBuf {
    let fake = Fake {
        verdict,
        code,
        size_error,
        ..Fake::honest()
    };
    fake.install(dir, &format!("fake-check-{verdict}-{code}-{size_error}"))
}

/// A run configuration with a fake checker answering "unverified".
pub fn config(work: &Path, inputs: Vec<PathBuf>) -> Config {
    let limits = work.join("limits.toml");
    fs::write(&limits, "[limits]\n").expect("write limits");
    Config {
        inputs,
        out_dir: work.join("out"),
        max_threads: 2,
        checker: fake_checker(work, "unverified", 2, 0),
        limits,
    }
}
