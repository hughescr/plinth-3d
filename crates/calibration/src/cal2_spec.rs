//! CAL-2 specification: strict, bounded JSON, separate from the CAL-1 spec.
//! The matrix is preregistered here. Any change to a row, ratio, count or copy number
//! is refused rather than written as a different experiment under the CAL-2 name.
use serde::{Deserialize, Serialize};

/// Print layer height, mm.
pub const LAYER: f64 = 0.05;
/// Seven ratios of the dense rows: 25 to 50 in steps of 5, then 70.
pub const RATIOS: [f64; 7] = [25.0, 30.0, 35.0, 40.0, 45.0, 50.0, 70.0];
/// Fatigue and cure-age rows run at these two ratios.
pub const PAIR: [f64; 2] = [40.0, 50.0];
/// Body ratio of a formation witness: half the design limit of 12.
pub const WITNESS_RATIO: f64 = 6.0;
/// Layers a short hold keeps at full area before the body steps in to its stem.
pub const SHORT_FULL_LAYERS: usize = 5;
/// Layers a long hold keeps at full area (5 mm).
pub const LONG_FULL_LAYERS: usize = 100;
/// The stem (short hold) and the first ramp layer (late load) are at most this fraction of
/// the full area.
pub const LOW_FRACTION: f64 = 0.40;
/// Layers a late-load body takes to ramp from `LOW_FRACTION` to full area, and then holds.
pub const RAMP_LAYERS: usize = 40;
pub const LATE_HOLD_LAYERS: usize = 40;
/// Strip body: long side over short side, long side along X.
pub const STRIP_ASPECT: f64 = 5.0;
pub const STRIP_HEADS: usize = 8;
/// Ring body: radial width, mm, and heads on its centre line.
pub const RING_WIDTH: f64 = 3.0;
pub const RING_HEADS: usize = 12;

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

/// What the body is and how its area is loaded, layer by layer.
#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Variant {
    /// Square slab at the target ratio for `hold_layers` layers, loaded right above the heads.
    Standard,
    /// 5 layers at full area, then a stem of at most 40 percent of the area.
    ShortHold,
    /// 100 layers at full area.
    LongHold,
    /// Starts at 40 percent of the area right above the heads, ramps to full area over 40
    /// layers (one square pad per head until the pads meet), then holds 40 layers.
    LateLoad,
    /// Body 5:1, long side along X, heads in one row.
    Strip,
    /// Annulus 3 mm wide, heads on its centre line.
    Ring,
}

impl Variant {
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::Standard => "standard",
            Self::ShortHold => "short hold",
            Self::LongHold => "long hold",
            Self::LateLoad => "late load",
            Self::Strip => "strip",
            Self::Ring => "ring",
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Row {
    pub id: String,
    /// Owner's priority (1 first); rows 1 to 5 never go below two copies.
    pub priority: usize,
    pub kind: Kind,
    pub variant: Variant,
    /// Designed neck diameters, mm. One cell group per entry.
    pub necks_mm: Vec<f64>,
    /// Heads on the coupon. Square variants need a perfect square.
    pub heads: usize,
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
    /// Layers a standard body holds its target ratio (2.0 mm).
    pub hold_layers: usize,
    pub gap_mm: f64,
    pub ratio_tolerance: f64,
    pub rows: Vec<Row>,
}

/// One printed coupon before placement.
#[derive(Debug, Clone, PartialEq)]
pub struct Item {
    pub row: usize,
    pub priority: usize,
    pub kind: Kind,
    pub variant: Variant,
    pub neck_mm: f64,
    pub heads: usize,
    pub ratio: f64,
    pub copy: usize,
    pub copies: usize,
}

impl Item {
    /// Heads per side of a square grid, when the coupon has one.
    #[must_use]
    pub fn grid(&self) -> usize {
        (0..=self.heads).find(|n| n * n >= self.heads).unwrap_or(0)
    }
}

/// One preregistered matrix row as data. `matrix()` turns each into a `Row`.
struct RowSpec {
    id: &'static str,
    priority: usize,
    kind: Kind,
    variant: Variant,
    necks: &'static [f64],
    heads: usize,
    ratios: &'static [f64],
    copies: usize,
}

/// The preregistered matrix as data, in priority order (row 1 first).
const MATRIX: [RowSpec; 11] = [
    RowSpec {
        id: "R1",
        priority: 1,
        kind: Kind::Head,
        variant: Variant::Standard,
        necks: &[0.40],
        heads: 16,
        ratios: &RATIOS,
        copies: 2,
    },
    RowSpec {
        id: "R2",
        priority: 2,
        kind: Kind::Head,
        variant: Variant::Standard,
        necks: &[0.55],
        heads: 9,
        ratios: &RATIOS,
        copies: 2,
    },
    RowSpec {
        id: "R3a",
        priority: 3,
        kind: Kind::Head,
        variant: Variant::ShortHold,
        necks: &[0.55],
        heads: 9,
        ratios: &PAIR,
        copies: 2,
    },
    RowSpec {
        id: "R3b",
        priority: 3,
        kind: Kind::Head,
        variant: Variant::LongHold,
        necks: &[0.55],
        heads: 9,
        ratios: &PAIR,
        copies: 2,
    },
    RowSpec {
        id: "R4",
        priority: 4,
        kind: Kind::Head,
        variant: Variant::LateLoad,
        necks: &[0.55],
        heads: 9,
        ratios: &PAIR,
        copies: 2,
    },
    RowSpec {
        id: "R5a",
        priority: 5,
        kind: Kind::Head,
        variant: Variant::Strip,
        necks: &[0.55],
        heads: STRIP_HEADS,
        ratios: &PAIR,
        copies: 2,
    },
    RowSpec {
        id: "R5b",
        priority: 5,
        kind: Kind::Head,
        variant: Variant::Ring,
        necks: &[0.55],
        heads: RING_HEADS,
        ratios: &PAIR,
        copies: 2,
    },
    RowSpec {
        id: "R6",
        priority: 6,
        kind: Kind::Head,
        variant: Variant::Standard,
        necks: &[0.80],
        heads: 4,
        ratios: &[25.0, 35.0, 40.0, 50.0, 70.0],
        copies: 2,
    },
    RowSpec {
        id: "R7",
        priority: 7,
        kind: Kind::Column,
        variant: Variant::Standard,
        necks: &[1.20],
        heads: 4,
        ratios: &[40.0, 70.0],
        copies: 2,
    },
    RowSpec {
        id: "R8",
        priority: 8,
        kind: Kind::Head,
        variant: Variant::Standard,
        necks: &[0.55],
        heads: 4,
        ratios: &[40.0],
        copies: 2,
    },
    RowSpec {
        id: "R9",
        priority: 9,
        kind: Kind::Witness,
        variant: Variant::Standard,
        necks: &[0.40, 0.55, 0.80],
        heads: 1,
        ratios: &[WITNESS_RATIO],
        copies: 1,
    },
];

/// The preregistered matrix, in priority order (row 1 first).
#[must_use]
pub fn matrix() -> Vec<Row> {
    MATRIX
        .iter()
        .map(|r| Row {
            id: r.id.into(),
            priority: r.priority,
            kind: r.kind,
            variant: r.variant,
            necks_mm: r.necks.to_vec(),
            heads: r.heads,
            ratios: r.ratios.to_vec(),
            copies: r.copies,
        })
        .collect()
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
            || self.hold_layers != 40
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
                            priority: r.priority,
                            kind: r.kind,
                            variant: r.variant,
                            neck_mm,
                            heads: r.heads,
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
        // 14 + 14 + 4 + 4 + 4 + 4 + 4 + 10 + 4 + 2 + 3
        assert_eq!(items.len(), 67);
        for (row, n) in [
            (0, 14),
            (1, 14),
            (2, 4),
            (3, 4),
            (4, 4),
            (5, 4),
            (6, 4),
            (7, 10),
            (8, 4),
            (9, 2),
            (10, 3),
        ] {
            assert_eq!(items.iter().filter(|i| i.row == row).count(), n);
        }
        // Rows of priority 1 to 5 carry two copies of every cell.
        assert!(
            s.rows
                .iter()
                .filter(|r| r.priority <= 5)
                .all(|r| r.copies == 2)
        );
    }

    #[test]
    fn square_variants_have_square_head_counts() {
        for r in spec().rows {
            if !matches!(r.variant, Variant::Strip | Variant::Ring) {
                let n = Item {
                    row: 0,
                    priority: r.priority,
                    kind: r.kind,
                    variant: r.variant,
                    neck_mm: r.necks_mm[0],
                    heads: r.heads,
                    ratio: 1.0,
                    copy: 0,
                    copies: 1,
                }
                .grid();
                assert_eq!(n * n, r.heads, "{}", r.id);
            }
        }
    }

    #[test]
    fn every_changed_value_is_refused() {
        type Edit = fn(&mut Cal2Spec);
        let edits: [Edit; 11] = [
            |s| s.rows[0].copies = 3,
            |s| s.rows[1].ratios[0] = 31.0,
            |s| s.rows[2].heads = 16,
            |s| s.rows[8].necks_mm[0] = 1.0,
            |s| s.rows[4].ratios.push(95.0),
            |s| s.rows.swap(0, 1),
            |s| s.rows[2].variant = Variant::Standard,
            |s| s.shuffle_seed = 0,
            |s| s.lift_mm = 4.0,
            |s| s.hold_layers = 41,
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
