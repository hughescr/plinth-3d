//! CAL-2 specification: strict, bounded JSON, separate from the CAL-1 spec.
//! The matrix is preregistered here. Any change to a row, ratio, count or copy number
//! is refused rather than written as a different experiment under the CAL-2 name.
use serde::{Deserialize, Serialize};

pub const RATIOS: [f64; 5] = [30.0, 40.0, 55.0, 70.0, 95.0];
/// Body ratio of a formation witness: half the design limit of 12.
pub const WITNESS_RATIO: f64 = 6.0;

#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    /// Plinth's own head: ball, standoff, cone, joint, column, raft.
    Head,
    /// A plain waisted column, used where plinth's head cannot reach the size.
    Column,
    /// One plinth head under a small body, ratio well below the design limit.
    Witness,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Row {
    pub id: String,
    pub kind: Kind,
    /// Designed neck diameters, mm. One cell group per entry.
    pub necks_mm: Vec<f64>,
    /// Heads per side; the coupon holds grid x grid heads.
    pub grid: usize,
    pub ratios: Vec<f64>,
    pub copies: usize,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Cal2Spec {
    pub label: String,
    pub plate: String,
    pub shuffle_seed: u64,
    pub lift_mm: f64,
    pub body_thickness_mm: f64,
    pub gap_mm: f64,
    pub ratio_tolerance: f64,
    pub rows: Vec<Row>,
}

/// One printed coupon before placement.
#[derive(Debug, Clone, PartialEq)]
pub struct Item {
    pub row: usize,
    pub kind: Kind,
    pub neck_mm: f64,
    pub grid: usize,
    pub ratio: f64,
    pub copy: usize,
    pub copies: usize,
}

fn row(id: &str, kind: Kind, necks: &[f64], grid: usize, ratios: &[f64], copies: usize) -> Row {
    Row {
        id: id.into(),
        kind,
        necks_mm: necks.to_vec(),
        grid,
        ratios: ratios.to_vec(),
        copies,
    }
}

/// The preregistered matrix, in priority order (row 1 first).
#[must_use]
pub fn matrix() -> Vec<Row> {
    use Kind::{Column, Head, Witness};
    vec![
        row("R1", Head, &[0.40], 4, &RATIOS, 3),
        row("R2", Head, &[0.55], 3, &RATIOS, 3),
        row("R3", Head, &[0.80], 2, &RATIOS, 2),
        row("R4", Column, &[1.20], 2, &[40.0, 70.0, 95.0], 2),
        row("R5", Head, &[0.55], 2, &[40.0, 70.0], 2),
        row("R6", Witness, &[0.40, 0.55, 0.80], 1, &[WITNESS_RATIO], 1),
    ]
}

impl Cal2Spec {
    /// Reject drift from the preregistered matrix and process values.
    ///
    /// # Errors
    /// Any value that differs from the CAL-2 definition.
    pub fn validate(&self) -> crate::Result<()> {
        let same = |a: f64, b: f64| a.is_finite() && (a - b).abs() < 1e-12;
        if self.label != crate::LABEL
            || self.plate != "CAL-2"
            || self.shuffle_seed == 0
            || !same(self.lift_mm, 5.0)
            || !same(self.body_thickness_mm, 3.0)
            || !same(self.gap_mm, 6.0)
            || !same(self.ratio_tolerance, 0.03)
            || self.rows != matrix()
        {
            return Err(
                "spec differs from the preregistered CAL-2 matrix or process values".into(),
            );
        }
        Ok(())
    }

    /// Every coupon, row by row in priority order, then neck, ratio, copy.
    #[must_use]
    pub fn items(&self) -> Vec<Item> {
        let mut out = Vec::new();
        for (row, r) in self.rows.iter().enumerate() {
            for &neck_mm in &r.necks_mm {
                for &ratio in &r.ratios {
                    for copy in 0..r.copies {
                        out.push(Item {
                            row,
                            kind: r.kind,
                            neck_mm,
                            grid: r.grid,
                            ratio,
                            copy,
                            copies: r.copies,
                        });
                    }
                }
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spec() -> Cal2Spec {
        serde_json::from_str(include_str!("../../../fixtures/calibration/cal2.json")).unwrap()
    }

    #[test]
    fn fixture_is_the_preregistered_matrix_and_counts_are_right() {
        let s = spec();
        s.validate().unwrap();
        let items = s.items();
        // 15 + 15 + 10 + 6 + 4 + 3
        assert_eq!(items.len(), 53);
        for (row, n) in [(0, 15), (1, 15), (2, 10), (3, 6), (4, 4), (5, 3)] {
            assert_eq!(items.iter().filter(|i| i.row == row).count(), n);
        }
    }

    #[test]
    fn every_changed_value_is_refused() {
        type Edit = fn(&mut Cal2Spec);
        let edits: [Edit; 9] = [
            |s| s.rows[0].copies = 2,
            |s| s.rows[1].ratios[0] = 31.0,
            |s| s.rows[2].grid = 3,
            |s| s.rows[3].necks_mm[0] = 1.0,
            |s| s.rows[4].ratios.push(95.0),
            |s| s.rows.swap(0, 1),
            |s| s.shuffle_seed = 0,
            |s| s.lift_mm = 4.0,
            |s| s.ratio_tolerance = 0.05,
        ];
        for (i, edit) in edits.iter().enumerate() {
            let mut s = spec();
            edit(&mut s);
            assert!(s.validate().is_err(), "edit {i} was accepted");
        }
        let mut s = spec();
        s.label = "x".into();
        assert!(s.validate().is_err());
    }

    #[test]
    fn unknown_fields_are_refused() {
        let text = include_str!("../../../fixtures/calibration/cal2.json").replacen(
            "\"plate\"",
            "\"extra\": 1, \"plate\"",
            1,
        );
        assert!(serde_json::from_str::<Cal2Spec>(&text).is_err());
    }
}
