//! Identity of the source and build configuration, and the loop-bench record (design
//! dev-loop.md section 9, gate step 5). The bench slot writes a record when `loop-bench` passes;
//! the gate checks that a passing record matches the tree under review, whenever the build
//! configuration changed. The record lives in the git common directory, so every slot sees it.

use crate::proc;
use std::path::{Path, PathBuf};

/// Everything a bench record must match.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Identity {
    /// `git rev-parse HEAD^{tree}` (the bench requires a clean tree).
    pub tree: String,
    /// Fingerprint of the build configuration files.
    pub config: String,
    /// `rustc -V`, as the pinned toolchain prints it.
    pub toolchain: String,
    /// Fingerprint of the committed stage manifest.
    pub manifest: String,
    /// Fingerprint of the tracked fixture files (names and blob hashes).
    pub fixtures: String,
}

/// True for a path that is build configuration: a manifest, a lockfile, a toolchain pin or a
/// cargo config of the main or checker workspace. The loop runner's own workspace is not part
/// of the products' build configuration.
#[must_use]
pub fn is_config_path(path: &str) -> bool {
    if path.starts_with("tools/devloop/") {
        return false;
    }
    let name = path.rsplit('/').next().unwrap_or(path);
    matches!(name, "Cargo.toml" | "Cargo.lock" | "rust-toolchain.toml")
        || path.ends_with(".cargo/config.toml")
}

/// The configuration paths among `paths`.
#[must_use]
pub fn config_paths<'a>(paths: impl IntoIterator<Item = &'a str>) -> Vec<&'a str> {
    paths.into_iter().filter(|p| is_config_path(p)).collect()
}

/// The git common directory (shared by every worktree).
pub fn common_dir(root: &Path) -> Result<PathBuf, String> {
    proc::git(
        root,
        &["rev-parse", "--path-format=absolute", "--git-common-dir"],
    )
    .map(PathBuf::from)
}

/// Where the bench record lives.
pub fn record_path(root: &Path) -> Result<PathBuf, String> {
    Ok(common_dir(root)?.join("plinth-bench-record.tsv"))
}

/// The identity of the checkout at `root` as committed (HEAD) plus its working configuration.
pub fn current(root: &Path) -> Result<Identity, String> {
    let tree = proc::git(root, &["rev-parse", "HEAD^{tree}"])?;
    let listed = proc::git(root, &["ls-files"])?;
    let mut cfg = String::new();
    for p in config_paths(listed.lines()) {
        let text = std::fs::read_to_string(root.join(p)).map_err(|e| format!("{p}: {e}"))?;
        cfg.push_str(&format!("{p}\n{text}\n"));
    }
    let manifest = std::fs::read(root.join("fixtures/round/stage-manifest.tsv"))
        .map_err(|e| format!("fixtures/round/stage-manifest.tsv: {e}"))?;
    let fixtures = proc::git(root, &["ls-files", "-s", "fixtures"])?;
    Ok(Identity {
        tree,
        config: proc::fingerprint(root, cfg.as_bytes()),
        toolchain: proc::out("rustc", &["-V"], root)?,
        manifest: proc::fingerprint(root, &manifest),
        fixtures: proc::fingerprint(root, fixtures.as_bytes()),
    })
}

impl Identity {
    /// One tab-separated record row for a bench run that ended `verdict`.
    #[must_use]
    pub fn row(&self, stamp: &str, verdict: &str) -> String {
        format!(
            "{stamp}\t{}\t{}\t{}\t{}\t{}\t{verdict}\n",
            self.tree, self.config, self.toolchain, self.manifest, self.fixtures
        )
    }

    /// True if some row of `record` is a passing bench run of exactly this identity.
    #[must_use]
    pub fn has_passing_row(&self, record: &str) -> bool {
        record.lines().any(|l| {
            let f: Vec<&str> = l.split('\t').collect();
            f.len() == 7
                && f[1] == self.tree
                && f[2] == self.config
                && f[3] == self.toolchain
                && f[4] == self.manifest
                && f[5] == self.fixtures
                && f[6] == "pass"
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn id() -> Identity {
        Identity {
            tree: "t1".into(),
            config: "c1".into(),
            toolchain: "rustc 1.97.1 (x 2026-01-01)".into(),
            manifest: "m1".into(),
            fixtures: "f1".into(),
        }
    }

    #[test]
    fn configuration_paths_are_manifests_locks_pins_and_cargo_config() {
        let all = [
            "Cargo.toml",
            "Cargo.lock",
            "rust-toolchain.toml",
            ".cargo/config.toml",
            "checker/Cargo.lock",
            "checker/.cargo/config.toml",
            "crates/geom/Cargo.toml",
            "crates/geom/src/lib.rs",
            "tools/devloop/Cargo.toml",
            "loop-budgets.tsv",
        ];
        assert_eq!(
            config_paths(all),
            [
                "Cargo.toml",
                "Cargo.lock",
                "rust-toolchain.toml",
                ".cargo/config.toml",
                "checker/Cargo.lock",
                "checker/.cargo/config.toml",
                "crates/geom/Cargo.toml"
            ]
        );
    }

    #[test]
    fn only_a_passing_row_of_exactly_this_identity_counts() {
        let me = id();
        let good = me.row("2026-10-30T10:00:00", "pass");
        assert!(me.has_passing_row(&good));
        assert!(!me.has_passing_row(&me.row("s", "fail")));
        for change in 0..5 {
            let mut other = id();
            match change {
                0 => other.tree = "t2".into(),
                1 => other.config = "c2".into(),
                2 => other.toolchain = "rustc 1.98.0".into(),
                3 => other.manifest = "m2".into(),
                _ => other.fixtures = "f2".into(),
            }
            assert!(!other.has_passing_row(&good), "change {change}");
        }
        assert!(!me.has_passing_row(""));
        assert!(!me.has_passing_row("garbage\tline\n"));
    }
}
