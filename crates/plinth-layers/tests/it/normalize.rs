//! FU-10 phase B golden fixtures (design sec 4, V1): deterministic
//! hand-built layers exercising each diagnosed defect class D1-D4, the
//! pass-through contract, and every typed resource refusal.

use crate::common;

use common::{loose_limits, normalize_layer, point_in_polygon, rect_ccw, rect_cw, shoelace};
use plinth_layers::{normalize_layers_batch, LayerFillRule, LayerLoops, LayerUnionError};

/// Legacy stitcher threshold at the perception call sites.
const MIN_AREA: f64 = 1e-3;

/// Scale that puts fixture coordinates OFF the 1e-8 quantization grid, so
/// verbatim emission of originals is distinguishable from requantization.
const THIRD: f64 = 1.0 / 3.0;

fn scaled(loop_points: Vec<[f64; 2]>) -> Vec<[f64; 2]> {
    loop_points
        .into_iter()
        .map(|point| [point[0] * THIRD, point[1] * THIRD])
        .collect()
}

/// V1: nested holes three deep (solid-hole-solid-hole), already normal.
/// The layer must pass through and emit the ORIGINAL f64 loops verbatim.
#[test]
fn nested_three_deep_passes_through_and_emits_originals() {
    let outer = scaled(rect_ccw(0.0, 0.0, 12.0, 12.0));
    let outer_hole = scaled(rect_cw(1.0, 1.0, 11.0, 11.0));
    let inner = scaled(rect_ccw(2.0, 2.0, 10.0, 10.0));
    let inner_hole = scaled(rect_cw(3.0, 3.0, 9.0, 9.0));
    let result = normalize_layer(
        vec![
            outer.clone(),
            outer_hole.clone(),
            inner.clone(),
            inner_hole.clone(),
        ],
        LayerFillRule::NonZero,
        MIN_AREA,
    );
    let layer = &result.layers[0];
    assert!(layer.passed_through);
    assert_eq!(layer.solids.len(), 2);
    // Largest outer first.
    assert_eq!(layer.solids[0].outer.points_xy_mm, outer);
    assert_eq!(layer.solids[0].holes.len(), 1);
    assert_eq!(layer.solids[0].holes[0].points_xy_mm, outer_hole);
    assert_eq!(layer.solids[1].outer.points_xy_mm, inner);
    assert_eq!(layer.solids[1].holes.len(), 1);
    assert_eq!(layer.solids[1].holes[0].points_xy_mm, inner_hole);
    assert!(layer.solids[0].outer.signed_area_mm2 > 0.0);
    assert!(layer.solids[0].holes[0].signed_area_mm2 < 0.0);
}

/// V1 / D1: two overlapping CCW solids union into ONE solid with no hole --
/// the treant case in miniature. Even-odd would have called the overlap a
/// hole; nonzero winding 2 keeps it solid.
#[test]
fn overlapping_solids_union_into_one_solid_without_phantom_hole() {
    let a = rect_ccw(0.0, 0.0, 10.0, 10.0);
    let b = rect_ccw(5.0, 5.0, 15.0, 15.0);
    let result = normalize_layer(vec![a, b], LayerFillRule::NonZero, MIN_AREA);
    let layer = &result.layers[0];
    assert!(!layer.passed_through);
    assert_eq!(layer.solids.len(), 1);
    assert!(layer.solids[0].holes.is_empty());
    let area = layer.solids[0].outer.signed_area_mm2;
    assert!((area - 175.0).abs() < 1e-6, "union area was {area}");
}

/// V1 / D2: a figure-eight contour. The crossing splits it into a +1-wound
/// lobe and a -1-wound lobe; both are nonzero, and each lobe is covered once,
/// so NONZERO and EVEN-ODD agree here: two disjoint solids, every emitted
/// outer CCW. The self-intersection is resolved either way -- no abort, no
/// undefined inside, no self-cancelled area (the raw shoelace over the whole
/// bowtie is 0, which is exactly why there is no pre-union area filter).
#[test]
fn figure_eight_resolves_under_both_rules() {
    // Crossing at (2,2): lobe (2,2)-(0,4)-(0,0) winds +1, lobe
    // (2,2)-(4,4)-(4,0) winds -1. Each lobe has area 4.
    let bowtie = vec![[0.0, 0.0], [4.0, 4.0], [4.0, 0.0], [0.0, 4.0]];
    for rule in [LayerFillRule::NonZero, LayerFillRule::EvenOdd] {
        let result = normalize_layer(vec![bowtie.clone()], rule, MIN_AREA);
        let layer = &result.layers[0];
        assert_eq!(layer.solids.len(), 2, "{rule:?} lobe count");
        let area: f64 = layer
            .solids
            .iter()
            .map(|solid| solid.outer.signed_area_mm2)
            .sum();
        assert!((area - 8.0).abs() < 1e-6, "{rule:?} kept {area}");
        assert!(layer
            .solids
            .iter()
            .all(|solid| solid.outer.signed_area_mm2 > 0.0 && solid.holes.is_empty()));
        assert!(!layer.passed_through);
    }
}

/// V1 / D2: a doubly wound contour (a large self-cancelling loop's cousin).
/// This is where the two rules genuinely part ways: nonzero fills the
/// winding-2 region once (multiplicity does not double-report area), while
/// even-odd's parity 2 voids it entirely. Neither loses it to a pre-union
/// filter, because there is no pre-union filter.
#[test]
fn doubly_wound_square_fills_once_under_nonzero_and_voids_under_even_odd() {
    let mut doubled = rect_ccw(0.0, 0.0, 10.0, 10.0);
    doubled.extend(rect_ccw(0.0, 0.0, 10.0, 10.0));
    let result = normalize_layer(vec![doubled.clone()], LayerFillRule::NonZero, MIN_AREA);
    let layer = &result.layers[0];
    assert_eq!(layer.solids.len(), 1);
    assert!(layer.solids[0].holes.is_empty());
    let area = layer.solids[0].outer.signed_area_mm2;
    assert!((area - 100.0).abs() < 1e-6, "doubly wound area was {area}");
    assert!(!layer.passed_through);

    let evenodd = normalize_layer(vec![doubled], LayerFillRule::EvenOdd, MIN_AREA);
    assert!(
        evenodd.layers[0].solids.is_empty(),
        "even-odd parity 2 must void the doubly wound square"
    );
}

/// V1 / D4: coincident collinear edges. Two rectangles sharing a full edge
/// merge into one solid; the duplicate positive-edge-owner case (the same
/// loop fed twice) unions to one copy instead of raising a seam error or
/// minting slivers.
#[test]
fn coincident_edges_merge_and_duplicates_collapse() {
    let left = rect_ccw(0.0, 0.0, 5.0, 10.0);
    let right = rect_ccw(5.0, 0.0, 10.0, 10.0);
    let merged = normalize_layer(vec![left, right], LayerFillRule::NonZero, MIN_AREA);
    assert_eq!(merged.layers[0].solids.len(), 1);
    let area = merged.layers[0].solids[0].outer.signed_area_mm2;
    assert!((area - 100.0).abs() < 1e-6, "merged area was {area}");

    let square = rect_ccw(0.0, 0.0, 7.0, 7.0);
    let duplicated = normalize_layer(
        vec![square.clone(), square],
        LayerFillRule::NonZero,
        MIN_AREA,
    );
    assert_eq!(duplicated.layers[0].solids.len(), 1);
    assert!(!duplicated.layers[0].passed_through);
    let area = duplicated.layers[0].solids[0].outer.signed_area_mm2;
    assert!((area - 49.0).abs() < 1e-6, "deduplicated area was {area}");
}

/// V1: a sub-threshold sliver in an otherwise valid layer is filtered after
/// the union and never aborts; the surviving solid still passes through with
/// its original coordinates.
#[test]
fn sub_threshold_sliver_is_filtered_without_aborting() {
    let solid = scaled(rect_ccw(0.0, 0.0, 9.0, 9.0));
    // 0.01 x 0.01 mm = 1e-4 mm^2, below the 1e-3 threshold.
    let sliver = rect_ccw(20.0, 20.0, 20.01, 20.01);
    let result = normalize_layer(
        vec![solid.clone(), sliver],
        LayerFillRule::NonZero,
        MIN_AREA,
    );
    let layer = &result.layers[0];
    assert_eq!(layer.solids.len(), 1);
    assert_eq!(layer.solids[0].outer.points_xy_mm, solid);
    assert!(layer.passed_through);
}

/// Design sec 3.3 degraded path: an unoriented (both-CCW) layer under
/// even-odd still classifies the inner loop as a hole, the emitted hole is
/// the ORIGINAL loop reversed (exact f64 values, no requantization), and the
/// layer still passes through.
#[test]
fn degraded_even_odd_layer_matches_reversed_original() {
    let outer = scaled(rect_ccw(0.0, 0.0, 6.0, 6.0));
    let inner_ccw = scaled(rect_ccw(1.0, 1.0, 5.0, 5.0));
    let result = normalize_layer(
        vec![outer.clone(), inner_ccw],
        LayerFillRule::EvenOdd,
        MIN_AREA,
    );
    let layer = &result.layers[0];
    assert!(layer.passed_through);
    assert_eq!(layer.solids.len(), 1);
    assert_eq!(layer.solids[0].outer.points_xy_mm, outer);
    assert_eq!(layer.solids[0].holes.len(), 1);
    // Reversed and canonically rotated, the original CCW square becomes the
    // min-key-first CW square with the SAME f64 coordinate values.
    assert_eq!(
        layer.solids[0].holes[0].points_xy_mm,
        scaled(rect_cw(1.0, 1.0, 5.0, 5.0))
    );
    assert!(layer.solids[0].holes[0].signed_area_mm2 < 0.0);
}

/// After normalization, even-odd and nonzero agree pointwise on the output
/// (the acceptance criterion of the whole item) -- exercised here on the D1
/// overlap fixture whose INPUT the two rules disagree about.
#[test]
fn fill_rules_agree_on_normalized_output_of_the_overlap_fixture() {
    let a = rect_ccw(0.0, 0.0, 10.0, 10.0);
    let b = rect_ccw(5.0, 5.0, 15.0, 15.0);
    let result = normalize_layer(vec![a, b], LayerFillRule::NonZero, MIN_AREA);
    let contours: Vec<&[[f64; 2]]> = result.layers[0]
        .solids
        .iter()
        .flat_map(|solid| {
            std::iter::once(solid.outer.points_xy_mm.as_slice())
                .chain(solid.holes.iter().map(|hole| hole.points_xy_mm.as_slice()))
        })
        .collect();
    // Probe on and off the overlap; all comfortably interior or exterior.
    for probe in [
        [7.5, 7.5],
        [2.0, 2.0],
        [12.0, 12.0],
        [2.0, 12.0],
        [-1.0, -1.0],
        [16.0, 7.5],
    ] {
        let evenodd = contours
            .iter()
            .filter(|contour| point_in_polygon(probe, contour))
            .count()
            % 2
            == 1;
        let nonzero = contours
            .iter()
            .map(|contour| common::winding_number(probe, contour))
            .sum::<i32>()
            != 0;
        assert_eq!(evenodd, nonzero, "rules disagree at {probe:?}");
    }
}

/// An empty layer and a layer whose only loop quantizes away both normalize
/// to empty output without error.
#[test]
fn empty_and_degenerate_layers_normalize_to_empty() {
    let empty = normalize_layer(Vec::new(), LayerFillRule::NonZero, MIN_AREA);
    assert!(empty.layers[0].solids.is_empty());
    assert!(empty.layers[0].passed_through);

    // Two vertices one quantum apart collapse to a single point.
    let degenerate = normalize_layer(
        vec![vec![[0.0, 0.0], [4e-9, 0.0], [0.0, 4e-9]]],
        LayerFillRule::NonZero,
        MIN_AREA,
    );
    assert!(degenerate.layers[0].solids.is_empty());
}

/// Batches preserve layer order and independence.
#[test]
fn batch_preserves_layer_order() {
    let layers = vec![
        LayerLoops {
            loops: vec![rect_ccw(0.0, 0.0, 2.0, 2.0)],
            fill_rule: LayerFillRule::NonZero,
        },
        LayerLoops {
            loops: Vec::new(),
            fill_rule: LayerFillRule::NonZero,
        },
        LayerLoops {
            loops: vec![rect_ccw(0.0, 0.0, 4.0, 4.0)],
            fill_rule: LayerFillRule::NonZero,
        },
    ];
    let result = normalize_layers_batch(&layers, MIN_AREA, &loose_limits()).unwrap();
    assert_eq!(result.layers.len(), 3);
    assert!((result.layers[0].solids[0].outer.signed_area_mm2 - 4.0).abs() < 1e-6);
    assert!(result.layers[1].solids.is_empty());
    assert!((result.layers[2].solids[0].outer.signed_area_mm2 - 16.0).abs() < 1e-6);
    assert!(result.effective_threads >= 1);
    assert!(result.participating_threads >= 1);
}

/// Solid ordering: largest outer area first, mirroring the legacy stitcher.
#[test]
fn solids_are_ordered_largest_first() {
    let small = scaled(rect_ccw(20.0, 20.0, 22.0, 22.0));
    let large = scaled(rect_ccw(0.0, 0.0, 10.0, 10.0));
    let result = normalize_layer(
        vec![small.clone(), large.clone()],
        LayerFillRule::NonZero,
        MIN_AREA,
    );
    let layer = &result.layers[0];
    assert!(layer.passed_through);
    assert_eq!(layer.solids.len(), 2);
    assert_eq!(layer.solids[0].outer.points_xy_mm, large);
    assert_eq!(layer.solids[1].outer.points_xy_mm, small);
    assert!(
        layer.solids[0].outer.signed_area_mm2 > layer.solids[1].outer.signed_area_mm2,
        "largest solid must come first"
    );
    assert!((layer.solids[0].outer.signed_area_mm2 - shoelace(&large)).abs() < 1e-12);
}

// ---- typed refusals ----

fn tiny_layer() -> Vec<LayerLoops> {
    vec![LayerLoops {
        loops: vec![rect_ccw(0.0, 0.0, 1.0, 1.0), rect_ccw(2.0, 0.0, 3.0, 1.0)],
        fill_rule: LayerFillRule::NonZero,
    }]
}

#[test]
fn input_vertex_cap_refuses_before_running() {
    let mut limits = loose_limits();
    limits.max_input_vertices = 7;
    let error = normalize_layers_batch(&tiny_layer(), MIN_AREA, &limits).unwrap_err();
    assert_eq!(
        error,
        LayerUnionError::InputVertexCap {
            requested: 8,
            cap: 7
        }
    );
}

#[test]
fn union_operation_cap_refuses_the_worst_case() {
    let mut limits = loose_limits();
    // n = 8 vertices: (8 + 64) * 4 = 288 modeled operations.
    limits.max_union_operations = 287;
    let error = normalize_layers_batch(&tiny_layer(), MIN_AREA, &limits).unwrap_err();
    assert_eq!(
        error,
        LayerUnionError::UnionOperationCap {
            requested: 288,
            cap: 287
        }
    );
}

#[test]
fn scratch_byte_cap_refuses_the_model() {
    let mut limits = loose_limits();
    limits.max_scratch_bytes = 64;
    let error = normalize_layers_batch(&tiny_layer(), MIN_AREA, &limits).unwrap_err();
    assert!(matches!(
        error,
        LayerUnionError::ScratchBytesCap { cap: 64, .. }
    ));
}

#[test]
fn output_vertex_cap_refuses_after_union() {
    let mut limits = loose_limits();
    limits.max_output_vertices = 7;
    let error = normalize_layers_batch(&tiny_layer(), MIN_AREA, &limits).unwrap_err();
    assert_eq!(
        error,
        LayerUnionError::OutputVertexCap {
            requested: 8,
            cap: 7
        }
    );
}

#[test]
fn non_finite_and_out_of_domain_coordinates_are_typed_refusals() {
    let bad = vec![LayerLoops {
        loops: vec![vec![[0.0, 0.0], [1.0, f64::NAN], [1.0, 1.0]]],
        fill_rule: LayerFillRule::NonZero,
    }];
    assert_eq!(
        normalize_layers_batch(&bad, MIN_AREA, &loose_limits()).unwrap_err(),
        LayerUnionError::NonFiniteCoordinate {
            layer: 0,
            loop_index: 0,
            vertex: 1
        }
    );

    let far = vec![LayerLoops {
        loops: vec![vec![[0.0, 0.0], [1.0e5, 0.0], [1.0, 1.0]]],
        fill_rule: LayerFillRule::NonZero,
    }];
    assert_eq!(
        normalize_layers_batch(&far, MIN_AREA, &loose_limits()).unwrap_err(),
        LayerUnionError::CoordinateOutOfDomain {
            layer: 0,
            loop_index: 0,
            vertex: 1,
            value_mm: 1.0e5
        }
    );
}

#[test]
fn invalid_min_area_is_a_typed_refusal() {
    let error = normalize_layers_batch(&tiny_layer(), -1.0, &loose_limits()).unwrap_err();
    assert_eq!(error, LayerUnionError::InvalidMinArea { value_mm2: -1.0 });
}
