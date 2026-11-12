//! The hard-limit values, read from `limits.toml` by the checker's own small parser.
//! The file is data: `key = number  # comment` lines under a `[limits]` heading.

use crate::bytes::{Res, fail};
use crate::v3::V;

#[derive(Debug, Clone, PartialEq)]
pub struct Limits {
    pub h1_patch_area_mm2: f64,
    pub h4_lift_mm: f64,
    pub h6_gap_mm: f64,
    pub h9_thickness_mm: f64,
    pub region_length_mm: f64,
    pub drain_min_mm: f64,
    pub mouth_clearance_mm: f64,
    /// Compatibility defaults because the file omitted the entire optional section.
    pub hollow_defaults_used: bool,
}

impl Limits {
    /// The values with their names, in file order (for the report).
    #[must_use]
    pub fn named(&self) -> [(&'static str, f64); 7] {
        [
            ("h1_patch_area_mm2", self.h1_patch_area_mm2),
            ("h4_lift_mm", self.h4_lift_mm),
            ("h6_gap_mm", self.h6_gap_mm),
            ("h9_thickness_mm", self.h9_thickness_mm),
            ("region_length_mm", self.region_length_mm),
            ("drain_min_mm", self.drain_min_mm),
            ("mouth_clearance_mm", self.mouth_clearance_mm),
        ]
    }
}

/// The build volume the checker measures files for, in plate coordinates (mm): the Saturn 4
/// Ultra 16K plate is 211.68 x 118.37 mm with 220 mm of height; the box is that, rounded up
/// (`|x| <= 120`, `|y| <= 70`, `-1 <= z <= 230`). A coordinate outside it is not a file for
/// this printer, and the numbers there are too large for the rounding allowances this checker
/// states (they grow with the coordinates, but are only certified for a file this size), so
/// the file is refused, never measured.
pub const BUILD_X_MM: f64 = 120.0;
pub const BUILD_Y_MM: f64 = 70.0;
pub const BUILD_Z_MM: (f64, f64) = (-1.0, 230.0);

/// Is the plate-coordinate point `p` inside the build volume (and finite)?
#[must_use]
pub fn in_build_volume(p: V) -> bool {
    p[0].abs() <= BUILD_X_MM
        && p[1].abs() <= BUILD_Y_MM
        && p[2] >= BUILD_Z_MM.0
        && p[2] <= BUILD_Z_MM.1
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
    let hollow = hollow_values(text)?;
    let limits = Limits {
        h1_patch_area_mm2: take("h1_patch_area_mm2")?,
        h4_lift_mm: take("h4_lift_mm")?,
        h6_gap_mm: take("h6_gap_mm")?,
        h9_thickness_mm: take("h9_thickness_mm")?,
        region_length_mm: take("region_length_mm")?,
        drain_min_mm: hollow.0[0],
        mouth_clearance_mm: hollow.0[1],
        hollow_defaults_used: hollow.1,
    };
    match values.first() {
        Some((k, _)) => fail(format!("limits: unknown key {k}")),
        None => Ok(limits),
    }
}

fn hollow_values(text: &str) -> Res<([f64; 2], bool)> {
    let names = ["drain_min_mm", "mouth_clearance_mm"];
    let mut values = [3.0, 5.0];
    let (mut active, mut section_seen, mut seen) = (false, false, [false; 2]);
    for raw in text.lines() {
        let line = raw.split('#').next().unwrap_or("").trim();
        if line.starts_with('[') || line.ends_with(']') {
            match line {
                "[limits]" => active = false,
                "[hollow]" if !section_seen => {
                    active = true;
                    section_seen = true;
                }
                _ => {
                    return fail(format!(
                        "limits: unknown, duplicate or malformed section {line}"
                    ));
                }
            }
            continue;
        }
        if !active || line.is_empty() {
            continue;
        }
        let (key, value) = line.split_once('=').ok_or_else(|| {
            crate::bytes::ReadError("hollow limits: expected key = number".to_owned())
        })?;
        let Some(i) = names.iter().position(|k| *k == key.trim()) else {
            return fail(format!("unknown hollow limit {key}"));
        };
        let Ok(v) = value.trim().parse::<f64>() else {
            return fail("invalid hollow limit");
        };
        if seen[i] || !v.is_finite() || v <= 0.0 {
            return fail("duplicate or nonpositive hollow limit");
        }
        seen[i] = true;
        values[i] = v;
    }
    if section_seen && seen.iter().any(|v| !v) {
        return fail("hollow limits: both drain_min_mm and mouth_clearance_mm are required");
    }
    // Old producer limits omit this whole section. Report that provenance for hollow runs.
    Ok((values, !section_seen))
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
    fn the_build_volume_is_the_plate_box_and_refuses_far_or_non_finite_points() {
        assert!(in_build_volume([0.0, 0.0, 0.0]));
        assert!(in_build_volume([120.0, -70.0, 230.0]));
        assert!(in_build_volume([-120.0, 70.0, -1.0]));
        for p in [
            [120.1, 0.0, 10.0],
            [0.0, 70.1, 10.0],
            [0.0, 0.0, 230.1],
            [0.0, 0.0, -1.1],
            [0.0, 3000.0, 10.0],
            [0.0, 0.0, 3000.0],
            [f64::NAN, 0.0, 0.0],
            [0.0, f64::INFINITY, 0.0],
        ] {
            assert!(!in_build_volume(p), "{p:?}");
        }
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
    fn hollow_defaults_and_explicit_overrides_are_independent_and_strict() {
        let defaults = parse(GOOD).expect("legacy limits remain supported");
        assert_eq!(
            [
                defaults.drain_min_mm.to_bits(),
                defaults.mouth_clearance_mm.to_bits()
            ],
            [3.0_f64.to_bits(), 5.0_f64.to_bits()]
        );
        let explicit = parse(&format!(
            "{GOOD}[hollow]\ndrain_min_mm = 3.2\nmouth_clearance_mm = 10\n"
        ))
        .expect("override");
        assert_eq!(
            [
                explicit.drain_min_mm.to_bits(),
                explicit.mouth_clearance_mm.to_bits()
            ],
            [3.2_f64.to_bits(), 10.0_f64.to_bits()]
        );
        for bad in [
            "unknown = 3",
            "drain_min_mm = NaN",
            "mouth_clearance_mm = 0",
            "drain_min_mm = 3\ndrain_min_mm = 4",
            "broken",
        ] {
            assert!(parse(&format!("{GOOD}[hollow]\n{bad}")).is_err(), "{bad}");
        }
    }

    #[test]
    fn hollow_headers_cannot_silently_default_and_missing_section_is_reported() {
        for header in [
            "[holow]",
            "[hollow",
            "hollow]",
            "[[hollow]]",
            "[hollow] trailing",
        ] {
            assert!(
                parse(&format!(
                    "{GOOD}{header}\ndrain_min_mm = 2\nmouth_clearance_mm = 5\n"
                ))
                .is_err(),
                "{header}"
            );
        }
        assert!(parse(&format!("{GOOD}[hollow]\n")).is_err());
        let limits = parse(GOOD).expect("absent section has documented defaults");
        assert!(limits.hollow_defaults_used);
        let explicit = parse(&format!(
            "{GOOD}[hollow]\ndrain_min_mm = 3\nmouth_clearance_mm = 5\n"
        ))
        .unwrap();
        assert!(!explicit.hollow_defaults_used);
        let input = crate::report::Input {
            path: "unreadable".to_owned(),
            data: Err("test".to_owned()),
        };
        for declared in [false, true] {
            let report = crate::report::build_posed_declared(
                &limits,
                &input,
                None,
                1,
                &crate::h7::Pose::default(),
                declared,
            );
            assert_eq!(
                report
                    .json
                    .get("settings")
                    .and_then(|s| s.get("hollow_limits_defaults_used")),
                Some(&crate::json::Json::Bool(declared))
            );
        }
    }

    #[test]
    fn the_committed_limits_file_has_the_prd_values() {
        // PRD section 6: H1 0.50 mm2, H4 5.0 mm, H6 0.3 mm, H9 0.1 mm, region length 3.0 mm.
        let text = std::fs::read_to_string("limits.toml").expect("checker limits.toml exists");
        let l = parse(&text).expect("parses");
        let want = [
            ("h1_patch_area_mm2", 0.50),
            ("h4_lift_mm", 5.0),
            ("h6_gap_mm", 0.3),
            ("h9_thickness_mm", 0.1),
            ("region_length_mm", 3.0),
            ("drain_min_mm", 3.0),
            ("mouth_clearance_mm", 5.0),
        ];
        assert_eq!(l.named(), want);
    }
}
