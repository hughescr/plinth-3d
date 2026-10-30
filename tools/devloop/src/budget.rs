//! `loop-budgets.tsv`: level, command, budget_s, target_s, source.

use std::path::Path;

#[derive(Debug, PartialEq, Eq)]
pub struct Row {
    pub budget_s: u64,
    pub target: String,
}

/// The row for `level` in the TSV text, if there is one with a numeric budget.
#[must_use]
pub fn find(tsv: &str, level: &str) -> Option<Row> {
    tsv.lines().find_map(|line| {
        let mut f = line.split('\t');
        if f.next()? != level {
            return None;
        }
        let budget_s = f.nth(1)?.parse().ok()?;
        Some(Row {
            budget_s,
            target: f.next().unwrap_or("-").to_owned(),
        })
    })
}

/// Read the file under `root` and find the row.
pub fn load(root: &Path, level: &str) -> Result<Option<Row>, String> {
    let path = root.join("loop-budgets.tsv");
    let text = std::fs::read_to_string(&path)
        .map_err(|e| format!("cannot read {}: {e}", path.display()))?;
    Ok(find(&text, level))
}

#[cfg(test)]
mod tests {
    use super::*;

    const TSV: &str = "level\tcommand\tbudget_s\ttarget_s\tsource\nL0\tscripts/t L0 <crate>\t5\t2\tx\nLnoop\tscripts/t Lnoop\t2\t-\tx\n";

    #[test]
    fn finds_budget_and_target() {
        assert_eq!(
            find(TSV, "L0"),
            Some(Row {
                budget_s: 5,
                target: "2".into()
            })
        );
        assert_eq!(
            find(TSV, "Lnoop"),
            Some(Row {
                budget_s: 2,
                target: "-".into()
            })
        );
    }

    #[test]
    fn unknown_level_and_header_row_are_not_found() {
        assert_eq!(find(TSV, "L9"), None);
        assert_eq!(find(TSV, "level"), None, "the header has no numeric budget");
    }
}
