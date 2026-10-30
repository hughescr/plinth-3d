//! The hard-limit values, read from `limits.toml` by the checker's own small parser.
//! The file is data: `key = number  # comment` lines under a `[limits]` heading.

use crate::bytes::{Res, fail};

#[derive(Debug, Clone, PartialEq)]
pub struct Limits {
    pub h1_patch_area_mm2: f64,
    pub h4_lift_mm: f64,
    pub h6_gap_mm: f64,
    pub h9_thickness_mm: f64,
    pub region_length_mm: f64,
}

impl Limits {
    /// The values with their names, in file order (for the report).
    #[must_use]
    pub fn named(&self) -> [(&'static str, f64); 5] {
        [
            ("h1_patch_area_mm2", self.h1_patch_area_mm2),
            ("h4_lift_mm", self.h4_lift_mm),
            ("h6_gap_mm", self.h6_gap_mm),
            ("h9_thickness_mm", self.h9_thickness_mm),
            ("region_length_mm", self.region_length_mm),
        ]
    }
}

/// Parse the text of `limits.toml`. Unknown keys, duplicate keys, missing keys and bad numbers
/// are errors, so a typo cannot silently leave a limit at a default.
pub fn parse(text: &str) -> Res<Limits> {
    let mut values: Vec<(String, f64)> = Vec::new();
    let mut in_limits = false;
    for (n, raw) in text.lines().enumerate() {
        let line = raw.split('#').next().unwrap_or("").trim();
        if line.is_empty() {
            continue;
        }
        if line.starts_with('[') {
            in_limits = line == "[limits]";
            continue;
        }
        if !in_limits {
            continue;
        }
        let Some((key, val)) = line.split_once('=') else {
            return fail(format!("limits line {}: expected key = value", n + 1));
        };
        let (key, val) = (key.trim(), val.trim());
        let Ok(v) = val.parse::<f64>() else {
            return fail(format!("limits line {}: '{val}' is not a number", n + 1));
        };
        if !v.is_finite() || v <= 0.0 {
            return fail(format!(
                "limits line {}: {key} must be a positive number",
                n + 1
            ));
        }
        if values.iter().any(|(k, _)| k == key) {
            return fail(format!("limits line {}: {key} appears twice", n + 1));
        }
        values.push((key.to_owned(), v));
    }
    let mut take = |key: &str| -> Res<f64> {
        let Some(i) = values.iter().position(|(k, _)| k == key) else {
            return fail(format!("limits: {key} is missing"));
        };
        Ok(values.remove(i).1)
    };
    let limits = Limits {
        h1_patch_area_mm2: take("h1_patch_area_mm2")?,
        h4_lift_mm: take("h4_lift_mm")?,
        h6_gap_mm: take("h6_gap_mm")?,
        h9_thickness_mm: take("h9_thickness_mm")?,
        region_length_mm: take("region_length_mm")?,
    };
    match values.first() {
        Some((k, _)) => fail(format!("limits: unknown key {k}")),
        None => Ok(limits),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const GOOD: &str = "[limits]\nh1_patch_area_mm2 = 0.50 # c\nh4_lift_mm = 5.0\nh6_gap_mm = 0.3\nh9_thickness_mm = 0.1\nregion_length_mm = 3.0\n";

    #[test]
    fn parses_the_values_and_ignores_comments() {
        let l = parse(GOOD).expect("parses");
        assert_eq!(l.named()[0], ("h1_patch_area_mm2", 0.5));
        assert_eq!(l.named()[4], ("region_length_mm", 3.0));
    }

    #[test]
    fn rejects_missing_unknown_duplicate_and_bad_values() {
        assert!(parse(&GOOD.replace("h4_lift_mm = 5.0\n", "")).is_err());
        assert!(parse(&format!("{GOOD}h2_extra = 1\n")).is_err());
        assert!(parse(&format!("{GOOD}h6_gap_mm = 0.4\n")).is_err());
        assert!(parse(&GOOD.replace("0.3", "abc")).is_err());
        assert!(parse(&GOOD.replace("0.50", "-1")).is_err());
    }

    #[test]
    fn the_committed_limits_file_has_the_prd_values() {
        // PRD section 6: H1 0.50 mm2, H4 5.0 mm, H6 0.3 mm, H9 0.1 mm, region length 3.0 mm.
        let text = std::fs::read_to_string("../limits.toml").expect("../limits.toml exists");
        let l = parse(&text).expect("parses");
        let want = [
            ("h1_patch_area_mm2", 0.50),
            ("h4_lift_mm", 5.0),
            ("h6_gap_mm", 0.3),
            ("h9_thickness_mm", 0.1),
            ("region_length_mm", 3.0),
        ];
        assert_eq!(l.named(), want);
    }
}
