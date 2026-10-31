//! Provenance check of the read-only evidence (design dev-loop.md section 9, gate step 4):
//! every `MANIFEST.tsv` row for a file under `test-data/` or `reference/` must still name a
//! file of the recorded size and SHA-256. The hashes come from `shasum`, in a few batches.

use std::collections::BTreeMap;
use std::path::Path;
use std::process::Command;

/// One manifest row.
#[derive(Debug, PartialEq, Eq)]
pub struct Row {
    pub dest: String,
    pub sha256: String,
    pub bytes: u64,
}

/// The rows of `MANIFEST.tsv` (destination, source, sha256, bytes, provenance) for the
/// read-only folders. The header row and rows for other folders are left out.
#[must_use]
pub fn parse(text: &str) -> Vec<Row> {
    text.lines()
        .filter_map(|l| {
            let f: Vec<&str> = l.split('\t').collect();
            let dest = f.first()?;
            let evidence = dest.starts_with("test-data/") || dest.starts_with("reference/");
            evidence.then_some(())?;
            Some(Row {
                dest: (*dest).to_owned(),
                sha256: f.get(2)?.to_ascii_lowercase(),
                bytes: f.get(3)?.parse().ok()?,
            })
        })
        .collect()
}

/// What `shasum -a 256` printed: `hex  path` per line.
#[must_use]
pub fn parse_shasum(out: &str) -> BTreeMap<String, String> {
    out.lines()
        .filter_map(|l| {
            let (hex, path) = l.split_once("  ")?;
            Some((path.to_owned(), hex.to_ascii_lowercase()))
        })
        .collect()
}

/// The problems of `rows` against the actual sizes and hashes (`None` size = missing file).
#[must_use]
pub fn problems(
    rows: &[Row],
    sizes: &BTreeMap<String, Option<u64>>,
    hashes: &BTreeMap<String, String>,
) -> Vec<String> {
    let mut out = Vec::new();
    for r in rows {
        match sizes.get(&r.dest).copied().flatten() {
            None => out.push(format!("{}: missing", r.dest)),
            Some(n) if n != r.bytes => {
                out.push(format!("{}: {n} bytes, manifest says {}", r.dest, r.bytes));
            }
            Some(_) => match hashes.get(&r.dest) {
                Some(h) if *h == r.sha256 => {}
                Some(h) => out.push(format!(
                    "{}: sha256 {h}, manifest says {}",
                    r.dest, r.sha256
                )),
                None => out.push(format!("{}: not hashed", r.dest)),
            },
        }
    }
    out
}

/// Check every evidence row of `MANIFEST.tsv` under `root`. Returns the problems.
pub fn check(root: &Path) -> Result<Vec<String>, String> {
    let text = std::fs::read_to_string(root.join("MANIFEST.tsv"))
        .map_err(|e| format!("MANIFEST.tsv: {e}"))?;
    let rows = parse(&text);
    if rows.is_empty() {
        return Err("MANIFEST.tsv lists no test-data or reference files".to_owned());
    }
    let mut sizes = BTreeMap::new();
    for r in &rows {
        let n = std::fs::metadata(root.join(&r.dest)).ok().map(|m| m.len());
        sizes.insert(r.dest.clone(), n);
    }
    let present: Vec<&str> = rows
        .iter()
        .filter(|r| sizes.get(&r.dest).copied().flatten().is_some())
        .map(|r| r.dest.as_str())
        .collect();
    let mut hashes = BTreeMap::new();
    for batch in present.chunks(100) {
        let out = Command::new("shasum")
            .args(["-a", "256", "--"])
            .args(batch)
            .current_dir(root)
            .output()
            .map_err(|e| format!("cannot run shasum: {e}"))?;
        if !out.status.success() {
            return Err(format!(
                "shasum failed: {}",
                String::from_utf8_lossy(&out.stderr).trim()
            ));
        }
        hashes.extend(parse_shasum(&String::from_utf8_lossy(&out.stdout)));
    }
    Ok(problems(&rows, &sizes, &hashes))
}

#[cfg(test)]
mod tests {
    use super::*;

    const TSV: &str = "destination\tsource\tsha256\tbytes\tprovenance\n\
        docs/A.md\t/x\taa\t1\tp\n\
        test-data/a b.stl\t/x\tABCD\t10\tp\n\
        reference/c.md\t/x\tef01\t5\tp\n";

    #[test]
    fn only_evidence_rows_are_read_and_hashes_are_lowercased() {
        let rows = parse(TSV);
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].dest, "test-data/a b.stl");
        assert_eq!(rows[0].sha256, "abcd");
        assert_eq!(rows[1].bytes, 5);
    }

    #[test]
    fn shasum_output_with_spaces_in_names_parses() {
        let m = parse_shasum("abcd  test-data/a b.stl\nEF01  reference/c.md\n");
        assert_eq!(m.get("test-data/a b.stl").map(String::as_str), Some("abcd"));
        assert_eq!(m.get("reference/c.md").map(String::as_str), Some("ef01"));
    }

    #[test]
    fn each_kind_of_drift_is_a_problem() {
        let rows = parse(TSV);
        let ok_sizes: BTreeMap<_, _> = [
            ("test-data/a b.stl".to_owned(), Some(10)),
            ("reference/c.md".to_owned(), Some(5)),
        ]
        .into();
        let ok_hashes: BTreeMap<_, _> = [
            ("test-data/a b.stl".to_owned(), "abcd".to_owned()),
            ("reference/c.md".to_owned(), "ef01".to_owned()),
        ]
        .into();
        assert!(problems(&rows, &ok_sizes, &ok_hashes).is_empty());
        let mut sizes = ok_sizes.clone();
        sizes.insert("reference/c.md".into(), Some(6));
        assert!(problems(&rows, &sizes, &ok_hashes)[0].contains("6 bytes"));
        let mut sizes = ok_sizes.clone();
        sizes.insert("reference/c.md".into(), None);
        assert!(problems(&rows, &sizes, &ok_hashes)[0].contains("missing"));
        let mut hashes = ok_hashes.clone();
        hashes.insert("test-data/a b.stl".into(), "0000".into());
        assert!(problems(&rows, &ok_sizes, &hashes)[0].contains("sha256 0000"));
        hashes.remove("test-data/a b.stl");
        assert!(problems(&rows, &ok_sizes, &hashes)[0].contains("not hashed"));
    }
}
