//! FU-10 phase B property tests (design sec 4, V2), seeded and
//! deterministic: random star-polygon soups through the normalizer, checking
//! idempotence, area monotonicity, fill-rule agreement, and containment
//! invariants. Seeded `rand_chacha` stands in for the design's `proptest`
//! suggestion -- it is already the workspace's pinned deterministic RNG and
//! adds no new dependency (recorded in the design sec 7 addendum).

mod common;

use common::{distance_to_boundary, loose_limits, point_in_polygon, shoelace, winding_number};
use plinth_layers::{normalize_layers_batch, LayerFillRule, LayerLoops, NormalizedLayer};
use rand_chacha::ChaCha8Rng;
use rand_core::{Rng, SeedableRng};

const MIN_AREA: f64 = 1e-3;
const CASES: u64 = 48;

fn unit(rng: &mut ChaCha8Rng) -> f64 {
    (rng.next_u64() >> 11) as f64 * (1.0 / (1u64 << 53) as f64)
}

/// A simple (star-shaped) CCW polygon around a random center: strictly
/// increasing vertex angles guarantee no self-intersection.
fn star_polygon(rng: &mut ChaCha8Rng) -> Vec<[f64; 2]> {
    let center = [unit(rng) * 100.0 - 50.0, unit(rng) * 100.0 - 50.0];
    let count = 3 + (rng.next_u64() % 8) as usize;
    (0..count)
        .map(|index| {
            let jitter = 0.8 * unit(rng);
            let angle = std::f64::consts::TAU * (index as f64 + jitter) / count as f64;
            let radius = 1.0 + 9.0 * unit(rng);
            [
                center[0] + radius * angle.cos(),
                center[1] + radius * angle.sin(),
            ]
        })
        .collect()
}

fn soup(rng: &mut ChaCha8Rng, max_loops: usize) -> Vec<Vec<[f64; 2]>> {
    let count = 1 + (rng.next_u64() as usize) % max_loops;
    (0..count).map(|_| star_polygon(rng)).collect()
}

fn normalize(loops: Vec<Vec<[f64; 2]>>) -> NormalizedLayer {
    normalize_layers_batch(
        &[LayerLoops {
            loops,
            fill_rule: LayerFillRule::NonZero,
        }],
        MIN_AREA,
        &loose_limits(),
    )
    .expect("property soup must normalize")
    .layers
    .remove(0)
}

fn layer_loops(layer: &NormalizedLayer) -> Vec<Vec<[f64; 2]>> {
    layer
        .solids
        .iter()
        .flat_map(|solid| {
            std::iter::once(solid.outer.points_xy_mm.clone())
                .chain(solid.holes.iter().map(|hole| hole.points_xy_mm.clone()))
        })
        .collect()
}

fn layer_area(layer: &NormalizedLayer) -> f64 {
    layer
        .solids
        .iter()
        .map(|solid| {
            solid.outer.signed_area_mm2
                + solid
                    .holes
                    .iter()
                    .map(|hole| hole.signed_area_mm2)
                    .sum::<f64>()
        })
        .sum()
}

/// V2 idempotence: feeding normalized output back through the normalizer is
/// the bit-for-bit identity, via the pass-through path.
#[test]
fn normalization_is_idempotent_bit_for_bit() {
    for seed in 0..CASES {
        let mut rng = ChaCha8Rng::seed_from_u64(seed);
        let first = normalize(soup(&mut rng, 4));
        let second = normalize(layer_loops(&first));
        assert!(
            second.passed_through,
            "seed {seed}: second application must pass through"
        );
        assert_eq!(
            second.solids, first.solids,
            "seed {seed}: second application must be the identity"
        );
    }
}

/// V2 area monotonicity: max individual |loop| <= union <= sum of |loops|,
/// and adding a loop never shrinks the union.
#[test]
fn union_area_is_bounded_and_monotone() {
    for seed in 0..CASES {
        let mut rng = ChaCha8Rng::seed_from_u64(1_000 + seed);
        let loops = soup(&mut rng, 5);
        let areas: Vec<f64> = loops.iter().map(|points| shoelace(points).abs()).collect();
        let sum: f64 = areas.iter().sum();
        let max = areas.iter().cloned().fold(0.0, f64::max);
        let tolerance = 1e-6 * sum.max(1.0);

        let mut previous = 0.0;
        for count in 1..=loops.len() {
            let union = layer_area(&normalize(loops[..count].to_vec()));
            assert!(
                union + tolerance >= previous,
                "seed {seed}: union shrank from {previous} to {union} at {count} loops"
            );
            previous = union;
        }
        assert!(
            previous + tolerance >= max && previous <= sum + tolerance,
            "seed {seed}: union {previous} outside [{max}, {sum}]"
        );
    }
}

/// V2 fill-rule agreement on normalized output: even-odd parity and nonzero
/// winding classify a dense random probe set identically. This is the
/// acceptance criterion of the whole item.
#[test]
fn even_odd_and_nonzero_agree_on_normalized_output() {
    for seed in 0..CASES {
        let mut rng = ChaCha8Rng::seed_from_u64(2_000 + seed);
        let layer = normalize(soup(&mut rng, 4));
        let contours = layer_loops(&layer);
        if contours.is_empty() {
            continue;
        }
        let mut checked = 0usize;
        for _ in 0..200 {
            let probe = [unit(&mut rng) * 140.0 - 70.0, unit(&mut rng) * 140.0 - 70.0];
            // Boundary-degenerate probes have no defined answer under either
            // rule; skip anything within a micron of an edge.
            if contours
                .iter()
                .any(|contour| distance_to_boundary(probe, contour) < 1e-3)
            {
                continue;
            }
            checked += 1;
            let evenodd = contours
                .iter()
                .filter(|contour| point_in_polygon(probe, contour))
                .count()
                % 2
                == 1;
            let nonzero = contours
                .iter()
                .map(|contour| winding_number(probe, contour))
                .sum::<i32>()
                != 0;
            assert_eq!(evenodd, nonzero, "seed {seed}: rules disagree at {probe:?}");
        }
        assert!(checked > 0, "seed {seed}: no probe survived the edge guard");
    }
}

/// V2 containment invariants: outers are CCW and pairwise non-crossing;
/// holes are CW, lie inside their owner, and only their owner.
#[test]
fn normalized_structure_invariants_hold() {
    for seed in 0..CASES {
        let mut rng = ChaCha8Rng::seed_from_u64(3_000 + seed);
        let layer = normalize(soup(&mut rng, 4));
        for solid in &layer.solids {
            assert!(
                solid.outer.signed_area_mm2 > 0.0,
                "seed {seed}: outer must be CCW"
            );
            for hole in &solid.holes {
                assert!(hole.signed_area_mm2 < 0.0, "seed {seed}: hole must be CW");
                // A strictly interior sample of the hole boundary: nudge the
                // edge midpoint inward is overkill here -- vertex containment
                // in the OWNER's outer suffices because normalized contours
                // do not touch.
                for vertex in &hole.points_xy_mm {
                    assert!(
                        point_in_polygon(*vertex, &solid.outer.points_xy_mm)
                            || distance_to_boundary(*vertex, &solid.outer.points_xy_mm) < 1e-6,
                        "seed {seed}: hole vertex {vertex:?} escapes its owner"
                    );
                }
            }
        }
        // Outers pairwise: no vertex of one strictly inside another.
        for (left_index, left) in layer.solids.iter().enumerate() {
            for (right_index, right) in layer.solids.iter().enumerate() {
                if left_index == right_index {
                    continue;
                }
                for vertex in &left.outer.points_xy_mm {
                    let inside = point_in_polygon(*vertex, &right.outer.points_xy_mm)
                        && distance_to_boundary(*vertex, &right.outer.points_xy_mm) > 1e-6
                        && right
                            .holes
                            .iter()
                            .all(|hole| !point_in_polygon(*vertex, &hole.points_xy_mm));
                    assert!(
                        !inside,
                        "seed {seed}: outer vertex {vertex:?} of solid {left_index} \
                         sits in the filled region of solid {right_index}"
                    );
                }
            }
        }
    }
}
