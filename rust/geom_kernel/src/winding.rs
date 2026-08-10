//! Batched outward-winding decision for unwelded triangle soups.
//!
//! This is the kernel's sole signed-volume-by-tetrahedron-sum implementation
//! (O-5: one implementation per capability).  It exists because the synthesized
//! support geometry handed to `.chitubox` is an *unwelded* soup — a frustum,
//! sphere, pad, or bridge emitted as loose triangles with no shared vertex
//! table — so no half-edge orientation propagation is available.  The only
//! orientation signal such a soup carries is the sign of
//! `Σ v0 · (v1 × v2) / 6`, the divergence-theorem volume of the tetrahedra
//! spanned from the origin to each triangle.  A closed soup wound outward sums
//! to a positive volume; the same soup wound inward sums to its negation.
//!
//! Determinism (sec 4.2, R6): parallelism is index-parallel map-collect across
//! *soups* only.  Each soup's own sum stays serial in triangle order, so no
//! float reduction ever crosses a work item and results are bit-identical for
//! any thread count.  The first failing soup in index order is the reported
//! failure, never whichever worker happened to finish first.

use rayon::prelude::*;

use crate::par;
use crate::vec3::{cross, dot, V3};

/// One unwelded triangle: three corners in emission order.
pub type SoupTriangle = [V3; 3];

/// Failure raised while summing tetrahedron volumes.  Both variants mean the
/// caller handed in geometry a signed volume cannot be stated for; neither is
/// recoverable by retrying with different limits.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SignedVolumeError {
    /// One triangle's own tetrahedron term is not finite.
    NonFiniteTetrahedron { soup: usize, triangle: usize },
    /// The running sum left the finite range at this triangle, even though
    /// every term up to and including it was finite.
    NonFiniteAccumulation { soup: usize, triangle: usize },
}

impl std::fmt::Display for SignedVolumeError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NonFiniteTetrahedron { soup, triangle } => write!(
                formatter,
                "soup {soup} triangle {triangle} has a non-finite tetrahedron volume"
            ),
            Self::NonFiniteAccumulation { soup, triangle } => write!(
                formatter,
                "soup {soup} signed volume left the finite range at triangle {triangle}"
            ),
        }
    }
}

impl std::error::Error for SignedVolumeError {}

/// Signed volumes for one batch of soups, plus the configured pool width the
/// batch ran under.  The width reports pool configuration, not a claim that
/// every worker received work.
#[derive(Clone, Debug, PartialEq)]
pub struct SignedSoupVolumeBatch {
    volumes_mm3: Vec<f64>,
    effective_threads: usize,
}

impl SignedSoupVolumeBatch {
    /// One signed volume per input soup, in input order.
    #[must_use]
    pub fn volumes_mm3(&self) -> &[f64] {
        &self.volumes_mm3
    }

    #[must_use]
    pub const fn effective_threads(&self) -> usize {
        self.effective_threads
    }
}

/// Sum `Σ v0 · (v1 × v2) / 6` over every triangle of every soup, in one batch.
///
/// An empty soup is `0.0`, which is neither outward nor inward — callers that
/// need a decision must reject empty geometry themselves.  Vertex finiteness is
/// not pre-validated: a non-finite corner cannot produce a finite tetrahedron
/// term, so the per-term check below already covers it and does so without
/// walking the soup twice.
pub fn signed_soup_volumes(
    soups: &[&[SoupTriangle]],
) -> Result<SignedSoupVolumeBatch, SignedVolumeError> {
    let effective_threads = par::configured_pool_threads();
    let evaluated: Vec<Result<f64, SignedVolumeError>> = par::run(|| {
        soups
            .par_iter()
            .enumerate()
            .map(|(soup, triangles)| soup_volume(soup, triangles))
            .collect()
    });
    let mut volumes_mm3 = Vec::with_capacity(evaluated.len());
    for volume in evaluated {
        volumes_mm3.push(volume?);
    }
    Ok(SignedSoupVolumeBatch {
        volumes_mm3,
        effective_threads,
    })
}

/// Wind every soup in the batch outward, reversing exactly those whose signed
/// volume is negative, and return the signed volumes measured *before* any
/// reversal.
///
/// Reversal swaps corners 1 and 2, which negates every tetrahedron term and so
/// negates the sum, leaving the corner-0 vertex — and therefore the emitted
/// leading vertex of each triangle — where it was.
pub fn wind_soups_outward(
    soups: &mut [Vec<SoupTriangle>],
) -> Result<SignedSoupVolumeBatch, SignedVolumeError> {
    let borrowed: Vec<&[SoupTriangle]> = soups.iter().map(Vec::as_slice).collect();
    let batch = signed_soup_volumes(&borrowed)?;
    drop(borrowed);
    par::run(|| {
        soups
            .par_iter_mut()
            .zip(batch.volumes_mm3.par_iter())
            .for_each(|(triangles, volume)| {
                if *volume < 0.0 {
                    for triangle in triangles.iter_mut() {
                        triangle.swap(1, 2);
                    }
                }
            });
    });
    Ok(batch)
}

/// Serial, fixed-order sum for one soup.  Kept serial deliberately: a parallel
/// float reduction here would make the result thread-count dependent (sec 4.2).
fn soup_volume(soup: usize, triangles: &[SoupTriangle]) -> Result<f64, SignedVolumeError> {
    let mut sum = 0.0;
    for (triangle, corners) in triangles.iter().enumerate() {
        let term = dot(corners[0], cross(corners[1], corners[2])) / 6.0;
        if !term.is_finite() {
            return Err(SignedVolumeError::NonFiniteTetrahedron { soup, triangle });
        }
        sum += term;
        if !sum.is_finite() {
            return Err(SignedVolumeError::NonFiniteAccumulation { soup, triangle });
        }
    }
    Ok(sum)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::Command;

    const THREAD_CHILD: &str = "GEOM_KERNEL_WINDING_THREAD_CHILD";
    const THREAD_SIGNATURE: &str = "WINDING_SIGNATURE ";

    /// Axis-aligned box as an unwelded 12-triangle soup, wound outward.
    fn box_soup(min: V3, max: V3) -> Vec<SoupTriangle> {
        let corner = |x: usize, y: usize, z: usize| {
            [
                if x == 0 { min[0] } else { max[0] },
                if y == 0 { min[1] } else { max[1] },
                if z == 0 { min[2] } else { max[2] },
            ]
        };
        let v = [
            corner(0, 0, 0),
            corner(1, 0, 0),
            corner(1, 1, 0),
            corner(0, 1, 0),
            corner(0, 0, 1),
            corner(1, 0, 1),
            corner(1, 1, 1),
            corner(0, 1, 1),
        ];
        const FACES: [[usize; 3]; 12] = [
            [0, 2, 1],
            [0, 3, 2],
            [4, 5, 6],
            [4, 6, 7],
            [0, 1, 5],
            [0, 5, 4],
            [1, 2, 6],
            [1, 6, 5],
            [2, 3, 7],
            [2, 7, 6],
            [3, 0, 4],
            [3, 4, 7],
        ];
        FACES
            .iter()
            .map(|face| [v[face[0]], v[face[1]], v[face[2]]])
            .collect()
    }

    fn reversed(soup: &[SoupTriangle]) -> Vec<SoupTriangle> {
        soup.iter()
            .map(|triangle| [triangle[0], triangle[2], triangle[1]])
            .collect()
    }

    #[test]
    fn analytic_box_volume_is_exact_and_sign_follows_winding() {
        let outward = box_soup([0.0; 3], [2.0, 3.0, 5.0]);
        let inward = reversed(&outward);
        let batch =
            signed_soup_volumes(&[outward.as_slice(), inward.as_slice(), [].as_slice()]).unwrap();
        assert_eq!(batch.volumes_mm3(), [30.0, -30.0, 0.0]);
    }

    #[test]
    fn translating_a_closed_soup_leaves_the_sign_alone() {
        // The tetrahedron sum is only translation-invariant for a CLOSED soup;
        // this pins that the unwelded box really is closed, far from the origin.
        let near = box_soup([0.0; 3], [1.0; 3]);
        let far = box_soup([100.0, -250.0, 37.5], [101.0, -249.0, 38.5]);
        let batch = signed_soup_volumes(&[near.as_slice(), far.as_slice()]).unwrap();
        // Cancellation grows with the offset, so this is a tolerance test on
        // the magnitude and an exact test on the sign, which is all the
        // winding decision consumes.
        assert!(batch.volumes_mm3().iter().all(|volume| *volume > 0.0));
        assert!((batch.volumes_mm3()[0] - 1.0).abs() < 1e-12);
        assert!((batch.volumes_mm3()[1] - 1.0).abs() < 1e-9);
    }

    #[test]
    fn winding_reverses_only_the_negative_soups_and_is_idempotent() {
        let outward = box_soup([0.0; 3], [2.0, 3.0, 5.0]);
        let mut soups = vec![outward.clone(), reversed(&outward)];
        let before = wind_soups_outward(&mut soups).unwrap();
        assert_eq!(before.volumes_mm3(), [30.0, -30.0]);
        assert_eq!(soups[0], outward);
        assert_eq!(soups[1], outward);

        let again = wind_soups_outward(&mut soups).unwrap();
        assert_eq!(again.volumes_mm3(), [30.0, 30.0]);
        assert_eq!(soups[0], outward);
        assert_eq!(soups[1], outward);
    }

    #[test]
    fn reversal_keeps_the_leading_corner_of_every_triangle() {
        let outward = box_soup([0.0; 3], [2.0, 3.0, 5.0]);
        let mut soups = vec![reversed(&outward)];
        let leading: Vec<V3> = soups[0].iter().map(|triangle| triangle[0]).collect();
        wind_soups_outward(&mut soups).unwrap();
        assert_eq!(
            soups[0]
                .iter()
                .map(|triangle| triangle[0])
                .collect::<Vec<_>>(),
            leading
        );
    }

    #[test]
    fn non_finite_geometry_names_its_soup_and_triangle_in_index_order() {
        let good = box_soup([0.0; 3], [1.0; 3]);
        let nan = [[[f64::NAN, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]]];
        let huge = [[
            [f64::MAX, 0.0, 0.0],
            [0.0, f64::MAX, 0.0],
            [0.0, 0.0, f64::MAX],
        ]];
        assert_eq!(
            signed_soup_volumes(&[good.as_slice(), nan.as_slice()]),
            Err(SignedVolumeError::NonFiniteTetrahedron {
                soup: 1,
                triangle: 0
            })
        );
        assert_eq!(
            signed_soup_volumes(&[huge.as_slice(), nan.as_slice()]),
            Err(SignedVolumeError::NonFiniteTetrahedron {
                soup: 0,
                triangle: 0
            })
        );
    }

    #[test]
    fn a_finite_term_that_overflows_the_running_sum_is_an_accumulation_failure() {
        // cross([0,1,0], [0,0,1]) is [1,0,0], so each term is f64::MAX / 6 —
        // individually finite, and seven of them cannot be summed.
        const LARGE: SoupTriangle = [[f64::MAX, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]];
        let single = [LARGE];
        assert!(signed_soup_volumes(&[single.as_slice()])
            .unwrap()
            .volumes_mm3()[0]
            .is_finite());

        let soup = [LARGE; 8];
        assert!(matches!(
            signed_soup_volumes(&[soup.as_slice()]),
            Err(SignedVolumeError::NonFiniteAccumulation { soup: 0, .. })
        ));
    }

    #[test]
    fn volume_bits_are_identical_under_one_four_and_sixteen_configured_threads() {
        let test_name = "winding::tests::volume_bits_are_identical_under_one_four_and_sixteen_configured_threads";
        if std::env::var_os(THREAD_CHILD).is_none() {
            let signatures = ["1", "4", "16"].map(|threads| {
                let output = Command::new(std::env::current_exe().unwrap())
                    .args(["--exact", test_name, "--nocapture"])
                    .env(THREAD_CHILD, "1")
                    .env("PLINTH_THREADS", threads)
                    .output()
                    .unwrap();
                assert!(
                    output.status.success(),
                    "{}",
                    String::from_utf8_lossy(&output.stderr)
                );
                String::from_utf8(output.stdout)
                    .unwrap()
                    .lines()
                    .find(|line| line.starts_with(THREAD_SIGNATURE))
                    .unwrap()
                    .to_owned()
            });
            assert!(signatures.windows(2).all(|pair| pair[0] == pair[1]));
            return;
        }

        let expected_threads = std::env::var("PLINTH_THREADS")
            .unwrap()
            .parse::<usize>()
            .unwrap();
        let mut soups: Vec<Vec<SoupTriangle>> = (0..64)
            .map(|index| {
                let offset = index as f64 * 0.375;
                let soup = box_soup(
                    [offset, offset * 0.5, -offset],
                    [offset + 1.25, offset * 0.5 + 2.5, -offset + 0.75],
                );
                if index % 2 == 0 {
                    reversed(&soup)
                } else {
                    soup
                }
            })
            .collect();
        let batch = wind_soups_outward(&mut soups).unwrap();
        assert_eq!(batch.effective_threads(), expected_threads);
        let bits: Vec<u64> = batch
            .volumes_mm3()
            .iter()
            .map(|volume| volume.to_bits())
            .chain(
                soups
                    .iter()
                    .flat_map(|soup| soup.iter().flatten().flatten())
                    .map(|coordinate| coordinate.to_bits()),
            )
            .collect();
        let digest = bits.iter().fold(1469598103934665603_u64, |hash, value| {
            (hash ^ value).wrapping_mul(1099511628211)
        });
        println!("{THREAD_SIGNATURE}{digest:016x}");
    }
}
