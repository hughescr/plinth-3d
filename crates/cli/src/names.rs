//! Output names (PRD FR-37). One `.chitubox` per input, named after the input's file stem
//! (`.keepup` stays in it). Names are unique within a run, compared without regard to case
//! (macOS volumes ignore case). Tiebreak, documented and deterministic: inputs are taken in
//! sorted path order; the first keeps the plain stem, each later clash gets `-2`, `-3`, ...
//! (the first suffix not yet used). So no output can overwrite another input's output.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

/// One input and the output stem it was given.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Named {
    pub input: PathBuf,
    pub stem: String,
}

fn stem_of(p: &Path) -> String {
    let s = p
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default();
    if s.is_empty() { "model".to_owned() } else { s }
}

/// Assign unique output stems. The result is in sorted input order. The same path given
/// twice counts as two inputs and gets two names.
#[must_use]
pub fn assign(inputs: &[PathBuf]) -> Vec<Named> {
    let mut sorted: Vec<&PathBuf> = inputs.iter().collect();
    sorted.sort();
    let mut taken: BTreeSet<String> = BTreeSet::new();
    let mut out = Vec::with_capacity(sorted.len());
    for p in sorted {
        let base = stem_of(p);
        let mut stem = base.clone();
        let mut n = 2_u64;
        while taken.contains(&stem.to_lowercase()) {
            stem = format!("{base}-{n}");
            n += 1;
        }
        taken.insert(stem.to_lowercase());
        out.push(Named {
            input: p.clone(),
            stem,
        });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn stems(v: &[&str]) -> Vec<String> {
        let inputs: Vec<PathBuf> = v.iter().map(PathBuf::from).collect();
        assign(&inputs).into_iter().map(|n| n.stem).collect()
    }

    #[test]
    fn plain_stems_and_keepup_are_kept() {
        assert_eq!(
            stems(&["b/Round 25.stl", "b/Round 25.keepup.stl"]),
            // Sorted path order: "...keepup.stl" sorts before "....stl".
            ["Round 25.keepup", "Round 25"]
        );
    }

    #[test]
    fn clashes_get_suffixes_in_sorted_path_order() {
        assert_eq!(
            stems(&["z/a.stl", "a/a.stl", "m/a.stl"]),
            ["a", "a-2", "a-3"]
        );
        // Order given does not matter.
        assert_eq!(
            stems(&["m/a.stl", "z/a.stl", "a/a.stl"]),
            ["a", "a-2", "a-3"]
        );
    }

    #[test]
    fn case_differences_clash_and_suffixes_never_collide() {
        assert_eq!(stems(&["a/X.stl", "b/x.stl"]), ["X", "x-2"]);
        assert_eq!(
            stems(&["a/x-2.stl", "b/x.stl", "c/x.stl"]),
            ["x-2", "x", "x-3"]
        );
    }

    #[test]
    fn the_same_path_twice_gets_two_names() {
        assert_eq!(stems(&["a/x.stl", "a/x.stl"]), ["x", "x-2"]);
    }

    #[test]
    fn empty_stem_gets_a_name() {
        assert_eq!(stems(&[".stl"]).len(), 1);
    }
}
