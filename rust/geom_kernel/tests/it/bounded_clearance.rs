//! `segment_clearance_classified_at_least` (speed lane B, 2026-10-13): a row
//! the kernel proves below its bound comes back `None`, and every other row
//! is the unbounded predicate's answer, bit for bit.
//!
//! The bounded entry point is the clearance re-route's and the lip-seat
//! rule's question: "does this moved solid clear what it must?" Most of the
//! rows it reads are refused, and a long slanted segment's box is most of the
//! model, so the full walk tests thousands of faces before it reaches the
//! near one. The bound lets it stop at one face that proves the row below.
//! Whatever it returns must leave the caller's verdict, and the value of
//! every row it admits, exactly as the full reading leaves them.

use geom_kernel::testkit::build_from_tris;
use geom_kernel::{NativeSurface, SegmentClearanceClassified};

/// A rolling heightfield over `[-5, 5]^2` (about 3 200 faces), so that long
/// slanted segments meet it at every distance.
fn hills() -> NativeSurface {
    let (n, side) = (40_usize, 10.0_f64);
    let step = side / f64::from(u32::try_from(n).unwrap_or(1));
    let at = |i: usize, j: usize| -> [f64; 3] {
        let x = -5.0 + step * f64::from(u32::try_from(i).unwrap_or(0));
        let y = -5.0 + step * f64::from(u32::try_from(j).unwrap_or(0));
        [x, y, 0.6 * (0.9 * x).sin() * (0.7 * y).cos()]
    };
    let mut tris: Vec<f64> = Vec::with_capacity(n * n * 18);
    for i in 0..n {
        for j in 0..n {
            for corner in [at(i, j), at(i + 1, j), at(i + 1, j + 1), at(i, j), at(i + 1, j + 1), at(i, j + 1)] {
                tris.extend_from_slice(&corner);
            }
        }
    }
    build_from_tris(tris)
}

/// xorshift64*, so the rows are the same on every run.
struct Rows(u64);

impl Rows {
    fn unit(&mut self) -> f64 {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        let bits = self.0.wrapping_mul(0x2545_F491_4F6C_DD1D) >> 11;
        f64::from(u32::try_from(bits >> 21).unwrap_or(0)) / f64::from(u32::MAX)
    }

    fn point(&mut self) -> [f64; 3] {
        [8.0 * self.unit() - 4.0, 8.0 * self.unit() - 4.0, 3.0 * self.unit() - 0.5]
    }
}

/// The reading a full row gives, measured or bounded; `None` unmeasured.
fn reading(row: SegmentClearanceClassified) -> Option<f64> {
    row.clearance_mm().or_else(|| row.upper_bound_mm())
}

#[test]
fn a_bounded_row_is_the_full_row_or_proven_below_its_bound() {
    let surface = hills();
    let mut rows = Rows(0x2545_F491_4F6C_DD1D);
    let n = 2_000;
    let mut p0s = Vec::with_capacity(n);
    let mut p1s = Vec::with_capacity(n);
    let mut radii = Vec::with_capacity(n);
    for _ in 0..n {
        p0s.push(rows.point());
        p1s.push(rows.point());
        radii.push(0.05 + 0.45 * rows.unit());
    }
    let insets = vec![0.0; n];
    let full = surface
        .segment_clearance_classified(&p0s, &p1s, &radii, &radii, 0.0, &insets, &insets)
        .expect("full rows");
    // Bounds on both sides of each row's own reading, and far below it.
    let bounds: Vec<f64> = full
        .iter()
        .map(|&row| {
            let offset = 0.6 * rows.unit() - 0.3;
            match reading(row) {
                Some(value) if rows.unit() < 0.8 => value + offset,
                _ => offset - 10.0,
            }
        })
        .collect();
    let bounded = surface
        .segment_clearance_classified_at_least(&p0s, &p1s, &radii, &radii, 0.0, &insets, &insets, &bounds)
        .expect("bounded rows");
    let (mut admitted, mut refused) = (0_usize, 0_usize);
    for ((row, full_row), &bound) in bounded.iter().zip(&full).zip(&bounds) {
        match row {
            Some(row) => {
                assert_eq!(*row, *full_row, "an admitted row is the full row, bit for bit");
                admitted += 1;
            }
            None => {
                let value = reading(*full_row).expect("a refused row was measured");
                assert!(value < bound, "refused {value} against a bound of {bound}");
                refused += 1;
            }
        }
        // The caller's verdict, "a MEASURED clearance at or above the bound",
        // is the full reading's.
        let verdict = row.and_then(SegmentClearanceClassified::clearance_mm).is_some_and(|v| v >= bound);
        let full_verdict = full_row.clearance_mm().is_some_and(|v| v >= bound);
        assert_eq!(verdict, full_verdict);
    }
    assert!(admitted > 200 && refused > 200, "admitted {admitted}, refused {refused}");
}

#[test]
fn an_unbounded_row_is_never_refused() {
    let surface = hills();
    let (p0, p1) = ([-4.0, -4.0, -0.4], [4.0, 3.0, 2.0]);
    let full = surface
        .segment_clearance_classified(&[p0], &[p1], &[0.2], &[0.2], 0.0, &[0.0], &[0.0])
        .expect("full row");
    let bounded = surface
        .segment_clearance_classified_at_least(
            &[p0],
            &[p1],
            &[0.2],
            &[0.2],
            0.0,
            &[0.0],
            &[0.0],
            &[f64::NEG_INFINITY],
        )
        .expect("bounded row");
    assert_eq!(bounded, vec![Some(full[0])]);
}
