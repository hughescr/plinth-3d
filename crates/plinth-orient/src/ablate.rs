//! Pose-stability ablation switches (design Lane A, `PLINTH_ORIENT_ABLATE`).
//!
//! An ablation changes WHAT the search does, so it is not a diagnostic. It
//! exists to measure one change at a time on the same binary, and it is
//! deleted when the change is promoted or dropped (design Lane P). With every
//! word off, the search, its record, its `search_digest` and the emitted
//! bytes are the recorded ones.
//!
//! | word | effect |
//! |---|---|
//! | `exact-grid` | build the candidate grid (G1-G4) from the EXACT table instead of the coarse proxy's, and enumerate the rest poses in ascending reference-bin order instead of heaviest-first. The coarse tier still scores on its proxy. CHANGES the search. |
//! | `exact-shadow` | out of band: after the record is final, score the SAME C1 and C2 input key sets on the exact table and repeat each cut. Written to the diagnostics only; needs `--perf`. Changes nothing. |
//!
//! Comma-separated. Unknown words warn and are ignored. A run with
//! `exact-grid` on is not a production run: its cache key does not carry the
//! switch, so use `--no-cache` with it.

/// The parsed `PLINTH_ORIENT_ABLATE`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Ablation {
    /// Build the grid from the exact table, rest poses in bin order.
    pub exact_grid: bool,
    /// Out of band: score the proxy cuts' inputs on the exact table.
    pub exact_shadow: bool,
    /// Words that did not parse; the caller prints them.
    pub warnings: Vec<String>,
}

impl Ablation {
    /// Reads `PLINTH_ORIENT_ABLATE`; unset reads as everything off.
    #[must_use]
    pub fn from_env() -> Self {
        std::env::var("PLINTH_ORIENT_ABLATE")
            .map(|text| Self::parse(&text))
            .unwrap_or_default()
    }

    /// Parses the switch text (module doc).
    #[must_use]
    pub fn parse(text: &str) -> Self {
        let mut ablation = Self::default();
        for word in text
            .split(|c: char| c == ',' || c.is_whitespace())
            .filter(|word| !word.is_empty())
        {
            match word {
                "exact-grid" => ablation.exact_grid = true,
                "exact-shadow" => ablation.exact_shadow = true,
                "off" => {}
                other => ablation
                    .warnings
                    .push(format!("unknown word {other:?}, ignored")),
            }
        }
        ablation
    }

    /// Whether any word is on.
    #[must_use]
    pub const fn any(&self) -> bool {
        self.exact_grid || self.exact_shadow
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_switch_text_parses_and_warns_on_what_it_cannot_read() {
        assert_eq!(Ablation::parse(""), Ablation::default());
        assert!(!Ablation::parse("").any());
        let grid = Ablation::parse("exact-grid");
        assert!(grid.exact_grid && !grid.exact_shadow && grid.warnings.is_empty());
        let both = Ablation::parse("exact-grid, exact-shadow");
        assert!(both.exact_grid && both.exact_shadow && both.any());
        let bad = Ablation::parse("exact-shadow,exact_grid");
        assert!(bad.exact_shadow && !bad.exact_grid);
        assert_eq!(bad.warnings.len(), 1);
        assert!(!Ablation::parse("off").any());
    }
}
