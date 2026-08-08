//! FU-10 phase B -- per-layer polygon union with fill-rule normalization.
//!
//! Design: `docs/rust-migration/slicer-union-design-2026-08-07.md` sec 3
//! (phase table row B; the OQ-1 resolution and phase-B deviations are the
//! sec 7 addendum). The stitched slice loops of one layer -- wound contours
//! when the phase-A orientation derivation succeeded, unoriented loops when it
//! degraded -- are resolved by ONE boolean union per layer into the canonical
//! normalized form: pairwise-disjoint outer boundaries, counter-clockwise
//! outers (positive signed area), clockwise holes (negative), each hole
//! attached to its unique owner, no self-intersections, no coincident edges.
//! On that form the even-odd and nonzero fill rules agree, which is the
//! definition of "normalized": every downstream consumer is correct without
//! modification, whatever rule it evaluates.
//!
//! # Numeric contract
//!
//! One explicit quantization at this boundary, exact integer arithmetic
//! inside, one dequantization out (design sec 3.2). The quantum is
//! [`SLICE_UNION_QUANTUM_MM`] = 1e-8 mm: the design's OQ-1 i64 branch, taken
//! because the pinned `i_overlay` 8.0.0 exposes an i64 integer API with i128
//! wide arithmetic. 1e-8 mm coincides with the perception stitch key
//! `LOOP_MERGE_MM`, so stitched vertices -- already snapped to that grid --
//! quantize losslessly and the pass-through comparison below is exact integer
//! equality. Coordinates must lie within [`SLICE_UNION_MAX_ABS_MM`], which
//! bounds quantized magnitudes to ~2^40 and keeps every wide product far from
//! i128 overflow. Float-input overlay APIs that pick an adaptive scale from
//! the data's bounding box are forbidden here: the scale would change when
//! the model moves, silently moving digests. Only the integer API is called.
//!
//! # Pass-through-if-already-normal
//!
//! Because this normalizer is upstream of every frozen OD-8 digest, a layer
//! whose union output equals its (area-filtered) input under canonical
//! integer form is emitted as the ORIGINAL f64 loops verbatim -- not the
//! requantized ones (design sec 3.5). Clean layers are therefore
//! byte-identical by construction, and the equality check doubles as the
//! idempotence property running in production.
//!
//! # Batch shape
//!
//! Batch-first (standing rule: per-item calls in hot paths are defects): the
//! API takes every layer at once, preflights all resource caps from exact
//! vertex counts before any engine allocation, and rayon-parallelizes one
//! union call per layer over ALL of that layer's loops -- the engine is
//! multi-polygon native, so there are no per-loop or per-pair calls. Thread
//! telemetry mirrors `geom_kernel::slicing` so the thread-sweep verification
//! idiom applies unchanged.

use std::collections::BTreeMap;
use std::sync::atomic::{AtomicU64, Ordering};

use i_overlay::core::fill_rule::FillRule;
use i_overlay::core::overlay::{IntOverlayOptions, Overlay, ShapeType};
use i_overlay::core::overlay_rule::OverlayRule;
use i_overlay::core::solver::Solver;
use i_overlay::i_float::int::point::IntPoint;
use rayon::prelude::*;
use thiserror::Error;

/// Quantization step of the layer-union boundary, in mm.
///
/// A determinism contract, not a tunable: changing it moves every downstream
/// slice digest. Registered as `SLICE_UNION_QUANTUM_MM` in the M0 registry
/// with the honest OQ-1 provenance; equal to the perception stitch key
/// `LOOP_MERGE_MM = 1e-8` so stitched vertices quantize losslessly.
pub const SLICE_UNION_QUANTUM_MM: f64 = 1.0e-8;

/// Half-width of the admissible coordinate domain, in mm.
///
/// Coordinates beyond this magnitude are refused (typed error, no clamping):
/// 1e4 mm keeps quantized magnitudes at or below 1e12 (~2^40), so per-term
/// i128 wide products (~2^80) summed over any feasible contour stay far below
/// the i128 range inside the pinned integer engine. Registered as
/// `SLICE_UNION_MAX_ABS_MM`; a refusal boundary, not printable reach.
pub const SLICE_UNION_MAX_ABS_MM: f64 = 1.0e4;

/// Modeled bytes per quantized input point (the engine's `IntPoint<i64>`).
const QUANTIZED_POINT_BYTES: usize = std::mem::size_of::<IntPoint<i64>>();

/// Modeled engine working-storage bytes per input edge (segment record,
/// sort keys, and graph links). An accounting model in the
/// `PlaneSliceLimits` spirit: it describes modeled storage charged against
/// [`LayerUnionLimits::max_scratch_bytes`], not allocator or temporary-heap
/// behavior inside the engine.
const ENGINE_EDGE_BYTES_MODEL: usize = 64;

/// Which fill rule the union applies to one layer's input loops.
///
/// `NonZero` is the physically correct rule for loops carrying the phase-A
/// derived winding (design sec 3.3): sub-solids each contribute +1, overlap
/// regions wind to >= 2 and stay solid. `EvenOdd` is the degraded path for a
/// layer whose winding derivation failed -- direction is meaningless there,
/// so parity is the only defensible fill.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LayerFillRule {
    NonZero,
    EvenOdd,
}

impl LayerFillRule {
    const fn engine(self) -> FillRule {
        match self {
            Self::NonZero => FillRule::NonZero,
            Self::EvenOdd => FillRule::EvenOdd,
        }
    }
}

/// One layer's stitched input: closed loops in walk order plus the fill rule
/// their directions warrant. Loops are neither filtered nor reordered by the
/// caller -- the union must see everything, including large self-cancelling
/// loops (design sec 3.2).
#[derive(Clone, Debug, PartialEq)]
pub struct LayerLoops {
    /// Closed loops; each vertex is `[x_mm, y_mm]` and the closing edge from
    /// last back to first is implicit.
    pub loops: Vec<Vec<[f64; 2]>>,
    pub fill_rule: LayerFillRule,
}

/// Explicit resource limits for [`normalize_layers_batch`].
///
/// Every cap is checked from exact input vertex counts before any engine
/// buffer is allocated, except `max_output_vertices`, which can only be
/// enforced on the union results themselves (output size is not knowable in
/// advance) and is checked before the batch returns.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LayerUnionLimits {
    /// Cap on total input vertices summed over every layer of the batch.
    pub max_input_vertices: usize,
    /// Cap on total emitted vertices summed over every layer of the batch.
    pub max_output_vertices: usize,
    /// Cap on the modeled worst-case union work summed over layers: per
    /// layer `(n + k) * ceil(log2(n))` with the intersection count `k`
    /// charged at its explicit `n^2` worst case (design sec 3.4).
    pub max_union_operations: usize,
    /// Cap on modeled scratch bytes: quantized input points plus
    /// [`ENGINE_EDGE_BYTES_MODEL`] per input edge plus the modeled output
    /// allowance of `max_output_vertices` points.
    pub max_scratch_bytes: usize,
}

/// One emitted loop: canonical rotation (minimum quantized key first), with
/// its shoelace area over exactly the emitted points.
#[derive(Clone, Debug, PartialEq)]
pub struct NormalizedLoop {
    pub points_xy_mm: Vec<[f64; 2]>,
    /// Positive for outer boundaries, negative for holes.
    pub signed_area_mm2: f64,
}

/// One normalized solid: a counter-clockwise outer boundary and its
/// clockwise holes. Hole ownership comes from the union engine's shape
/// structure and is unique by construction -- there is no fallback chain and
/// no droppable "unowned hole" in normalized output.
#[derive(Clone, Debug, PartialEq)]
pub struct NormalizedSolid {
    pub outer: NormalizedLoop,
    pub holes: Vec<NormalizedLoop>,
}

/// One layer of normalized output.
#[derive(Clone, Debug, PartialEq)]
pub struct NormalizedLayer {
    /// Solids ordered largest outer area first, ties by first-vertex
    /// quantized key (the legacy stitcher's solid order).
    pub solids: Vec<NormalizedSolid>,
    /// True when the union output equaled the area-filtered input under
    /// canonical integer form and the ORIGINAL f64 loops were emitted
    /// verbatim (design sec 3.5). False whenever the union changed anything,
    /// including representation (deduplicated vertices).
    pub passed_through: bool,
}

/// All layers of one batch plus result-inert execution telemetry, mirroring
/// `geom_kernel::slicing::PlaneSliceBatch` for thread-sweep verification.
#[derive(Clone, Debug, PartialEq)]
pub struct NormalizedLayers {
    pub layers: Vec<NormalizedLayer>,
    pub effective_threads: usize,
    pub participating_threads: usize,
}

#[derive(Clone, Debug, PartialEq, Error)]
pub enum LayerUnionError {
    #[error("layers[{layer}].loops[{loop_index}][{vertex}] is not finite")]
    NonFiniteCoordinate {
        layer: usize,
        loop_index: usize,
        vertex: usize,
    },
    #[error(
        "layers[{layer}].loops[{loop_index}][{vertex}] magnitude {value_mm} mm \
         exceeds the {SLICE_UNION_MAX_ABS_MM} mm quantization domain"
    )]
    CoordinateOutOfDomain {
        layer: usize,
        loop_index: usize,
        vertex: usize,
        value_mm: f64,
    },
    #[error("minimum loop area {value_mm2} mm^2 must be finite and non-negative")]
    InvalidMinArea { value_mm2: f64 },
    #[error("input vertices {requested} exceed cap {cap}")]
    InputVertexCap { requested: usize, cap: usize },
    #[error("worst-case union operations {requested} exceed cap {cap}")]
    UnionOperationCap { requested: usize, cap: usize },
    #[error("modeled scratch bytes {requested} exceed cap {cap}")]
    ScratchBytesCap { requested: usize, cap: usize },
    #[error("output vertices {requested} exceed cap {cap}")]
    OutputVertexCap { requested: usize, cap: usize },
    #[error("layer-union resource arithmetic overflow in {section}")]
    ArithmeticOverflow { section: &'static str },
    #[error("layer-union thread pool: {0}")]
    Pool(String),
    #[error("union engine contract violated on layer {layer}: {detail}")]
    EngineContract { layer: usize, detail: &'static str },
}

#[inline]
fn quantize(value: f64) -> i64 {
    // Identical rounding to perception's `point_key`, which is what makes the
    // 1e-8 quantum lossless with respect to stitched vertices.
    (value / SLICE_UNION_QUANTUM_MM).round() as i64
}

#[inline]
fn dequantize(value: i64) -> f64 {
    value as f64 * SLICE_UNION_QUANTUM_MM
}

/// Shoelace area over emitted f64 points, identical to the perception
/// stitcher's `signed_area`.
fn signed_area(points: &[[f64; 2]]) -> f64 {
    if points.len() < 3 {
        return 0.0;
    }
    (0..points.len())
        .map(|i| {
            let next = (i + 1) % points.len();
            points[i][0] * points[next][1] - points[next][0] * points[i][1]
        })
        .sum::<f64>()
        * 0.5
}

/// Twice the signed area of a quantized contour, exactly, in quantum^2 units.
fn int_shoelace2(points: &[IntPoint<i64>]) -> i128 {
    if points.len() < 3 {
        return 0;
    }
    (0..points.len())
        .map(|i| {
            let next = (i + 1) % points.len();
            let a = &points[i];
            let b = &points[next];
            i128::from(a.x) * i128::from(b.y) - i128::from(b.x) * i128::from(a.y)
        })
        .sum()
}

/// Index of the lexicographically minimum `(x, y)` key; first occurrence on
/// ties. The same ordering as perception's `PointKey`, so canonical rotations
/// agree across the boundary.
fn min_key_index(points: &[IntPoint<i64>]) -> usize {
    points
        .iter()
        .enumerate()
        .min_by_key(|(_, point)| (point.x, point.y))
        .map(|(index, _)| index)
        .unwrap_or(0)
}

/// Canonical comparison key of a quantized contour: the point sequence in its
/// own direction, rotated so the minimum key comes first.
fn canonical_key(points: &[IntPoint<i64>]) -> Vec<(i64, i64)> {
    let start = min_key_index(points);
    (0..points.len())
        .map(|offset| {
            let point = &points[(start + offset) % points.len()];
            (point.x, point.y)
        })
        .collect()
}

/// One input loop after validation and quantization.
struct QuantizedInput {
    /// Quantized points, consecutive duplicates and the closing duplicate
    /// removed.
    points: Vec<IntPoint<i64>>,
    /// True when quantization was a pure per-vertex map (no vertex was
    /// dropped), so the original f64 loop and the quantized loop correspond
    /// index for index -- the precondition for emitting originals verbatim.
    exact: bool,
}

fn quantize_layer(
    layer_index: usize,
    input: &LayerLoops,
) -> Result<Vec<QuantizedInput>, LayerUnionError> {
    let mut quantized = Vec::with_capacity(input.loops.len());
    for (loop_index, loop_points) in input.loops.iter().enumerate() {
        let mut points: Vec<IntPoint<i64>> = Vec::with_capacity(loop_points.len());
        for (vertex, point) in loop_points.iter().enumerate() {
            for &value in point {
                if !value.is_finite() {
                    return Err(LayerUnionError::NonFiniteCoordinate {
                        layer: layer_index,
                        loop_index,
                        vertex,
                    });
                }
                if value.abs() > SLICE_UNION_MAX_ABS_MM {
                    return Err(LayerUnionError::CoordinateOutOfDomain {
                        layer: layer_index,
                        loop_index,
                        vertex,
                        value_mm: value,
                    });
                }
            }
            let quantized_point = IntPoint {
                x: quantize(point[0]),
                y: quantize(point[1]),
            };
            if points.last() == Some(&quantized_point) {
                continue;
            }
            points.push(quantized_point);
        }
        while points.len() > 1 && points.first() == points.last() {
            points.pop();
        }
        let exact = points.len() == loop_points.len();
        quantized.push(QuantizedInput { points, exact });
    }
    Ok(quantized)
}

/// Normalize one layer: one union call over all of its loops, canonical
/// output, pass-through when the layer was already normal.
fn normalize_one_layer(
    layer_index: usize,
    input: &LayerLoops,
    min_area_mm2: f64,
) -> Result<NormalizedLayer, LayerUnionError> {
    let quantized = quantize_layer(layer_index, input)?;

    // Pass-through eligibility: the loop survives the area rule AND its
    // original representation maps one-to-one onto its quantized form.
    // The area rule mirrors the legacy stitcher: keep when |area| >= min.
    let eligible: Vec<usize> = (0..quantized.len())
        .filter(|&index| {
            quantized[index].exact
                && quantized[index].points.len() >= 3
                && signed_area(&input.loops[index]).abs() >= min_area_mm2
        })
        .collect();

    // Everything with enough points feeds the union -- sub-threshold and
    // self-cancelling loops included; degenerate output is filtered AFTER
    // the union so the engine resolves them instead of us guessing.
    let fed: Vec<&QuantizedInput> = quantized
        .iter()
        .filter(|input_loop| input_loop.points.len() >= 3)
        .collect();
    if fed.is_empty() {
        return Ok(NormalizedLayer {
            solids: Vec::new(),
            passed_through: eligible.is_empty(),
        });
    }

    let capacity = fed.iter().map(|input_loop| input_loop.points.len()).sum();
    let mut overlay = Overlay::<i64>::new_custom(
        capacity,
        IntOverlayOptions::keep_all_points(),
        Solver::default(),
    );
    for input_loop in &fed {
        overlay.add_contour(&input_loop.points, ShapeType::Subject);
    }
    let shapes = overlay.overlay(OverlayRule::Subject, input.fill_rule.engine());

    // Engine contract: first contour of each shape is the CCW outer, the
    // rest are CW holes. Verified exactly, in integers.
    struct SurvivingShape {
        /// (contour, twice-signed-area) pairs; index 0 is the outer.
        contours: Vec<(Vec<IntPoint<i64>>, i128)>,
    }
    let mut survivors: Vec<SurvivingShape> = Vec::with_capacity(shapes.len());
    for shape in shapes {
        let mut contours = Vec::with_capacity(shape.len());
        for (position, contour) in shape.into_iter().enumerate() {
            let doubled = int_shoelace2(&contour);
            if position == 0 {
                if doubled <= 0 {
                    return Err(LayerUnionError::EngineContract {
                        layer: layer_index,
                        detail: "shape outer is not counter-clockwise",
                    });
                }
            } else if doubled >= 0 {
                return Err(LayerUnionError::EngineContract {
                    layer: layer_index,
                    detail: "shape hole is not clockwise",
                });
            }
            contours.push((contour, doubled));
        }
        let outer_doubled = match contours.first() {
            Some((_, doubled)) => *doubled,
            None => {
                return Err(LayerUnionError::EngineContract {
                    layer: layer_index,
                    detail: "shape carries no outer contour",
                });
            }
        };
        // Degenerate-output filter (design sec 3.2): a solid whose outer
        // fails the area rule drops whole; a failing hole drops alone.
        if int_area_mm2_from_doubled(outer_doubled) < min_area_mm2 {
            continue;
        }
        contours.retain(|(_, doubled)| int_area_mm2_from_doubled(*doubled).abs() >= min_area_mm2);
        survivors.push(SurvivingShape { contours });
    }

    // Pass-through comparison (design sec 3.5): every surviving contour must
    // match a distinct eligible input, in either direction, and the counts
    // must agree. Matching is exact integer equality of canonical keys.
    let mut by_key: BTreeMap<Vec<(i64, i64)>, Vec<usize>> = BTreeMap::new();
    for &index in &eligible {
        by_key
            .entry(canonical_key(&quantized[index].points))
            .or_default()
            .push(index);
    }
    let surviving_contours: usize = survivors.iter().map(|shape| shape.contours.len()).sum();
    let mut matches: Vec<Vec<(usize, bool)>> = Vec::with_capacity(survivors.len());
    let mut matched = surviving_contours == eligible.len();
    'shapes: for shape in &survivors {
        let mut shape_matches = Vec::with_capacity(shape.contours.len());
        for (contour, _) in &shape.contours {
            if !matched {
                break 'shapes;
            }
            let direct = by_key.get_mut(&canonical_key(contour)).and_then(Vec::pop);
            let found = match direct {
                Some(index) => Some((index, false)),
                None => {
                    let mut reversed = contour.clone();
                    reversed.reverse();
                    by_key
                        .get_mut(&canonical_key(&reversed))
                        .and_then(Vec::pop)
                        .map(|index| (index, true))
                }
            };
            match found {
                Some(hit) => shape_matches.push(hit),
                None => {
                    matched = false;
                    break 'shapes;
                }
            }
        }
        matches.push(shape_matches);
    }
    let passed_through = matched;

    // Emission. Pass-through: the ORIGINAL f64 loop verbatim, reversed
    // exactly when it matched the engine contour reversed, rotated so the
    // minimum quantized key comes first (a permutation of the original
    // values -- no arithmetic touches them). Union path: the engine contour
    // rotated the same way, then dequantized.
    let mut solids = Vec::with_capacity(survivors.len());
    for (shape_index, shape) in survivors.iter().enumerate() {
        let mut loops = Vec::with_capacity(shape.contours.len());
        for (contour_index, (contour, _)) in shape.contours.iter().enumerate() {
            let points_xy_mm = if passed_through {
                let (input_index, reversed) = matches[shape_index][contour_index];
                let mut original = input.loops[input_index].clone();
                if reversed {
                    original.reverse();
                }
                let keys: Vec<IntPoint<i64>> = original
                    .iter()
                    .map(|point| IntPoint {
                        x: quantize(point[0]),
                        y: quantize(point[1]),
                    })
                    .collect();
                original.rotate_left(min_key_index(&keys));
                original
            } else {
                let start = min_key_index(contour);
                (0..contour.len())
                    .map(|offset| {
                        let point = &contour[(start + offset) % contour.len()];
                        [dequantize(point.x), dequantize(point.y)]
                    })
                    .collect()
            };
            loops.push(NormalizedLoop {
                signed_area_mm2: signed_area(&points_xy_mm),
                points_xy_mm,
            });
        }
        let mut iterator = loops.into_iter();
        let Some(outer) = iterator.next() else {
            continue;
        };
        let mut holes: Vec<NormalizedLoop> = iterator.collect();
        holes.sort_by(|left, right| {
            right
                .signed_area_mm2
                .abs()
                .total_cmp(&left.signed_area_mm2.abs())
                .then_with(|| {
                    first_point_key(&left.points_xy_mm).cmp(&first_point_key(&right.points_xy_mm))
                })
        });
        solids.push(NormalizedSolid { outer, holes });
    }
    solids.sort_by(|left, right| {
        right
            .outer
            .signed_area_mm2
            .abs()
            .total_cmp(&left.outer.signed_area_mm2.abs())
            .then_with(|| {
                first_point_key(&left.outer.points_xy_mm)
                    .cmp(&first_point_key(&right.outer.points_xy_mm))
            })
    });
    Ok(NormalizedLayer {
        solids,
        passed_through,
    })
}

#[inline]
fn int_area_mm2_from_doubled(doubled: i128) -> f64 {
    (doubled as f64) * 0.5 * SLICE_UNION_QUANTUM_MM * SLICE_UNION_QUANTUM_MM
}

fn first_point_key(points: &[[f64; 2]]) -> (i64, i64) {
    points
        .first()
        .map(|point| (quantize(point[0]), quantize(point[1])))
        .unwrap_or((i64::MAX, i64::MAX))
}

/// Preflight the batch-level caps from exact input counts (no allocation has
/// happened yet when this refuses).
fn preflight(layers: &[LayerLoops], limits: &LayerUnionLimits) -> Result<(), LayerUnionError> {
    let mut input_vertices = 0usize;
    let mut operations = 0usize;
    for layer in layers {
        let mut layer_vertices = 0usize;
        for loop_points in &layer.loops {
            layer_vertices = layer_vertices.checked_add(loop_points.len()).ok_or(
                LayerUnionError::ArithmeticOverflow {
                    section: "input vertices",
                },
            )?;
        }
        input_vertices = input_vertices.checked_add(layer_vertices).ok_or(
            LayerUnionError::ArithmeticOverflow {
                section: "input vertices",
            },
        )?;
        // Worst-case union work for this layer: (n + k) * ceil(log2(n)) with
        // k charged at its explicit n^2 worst case (design sec 3.4).
        let n = layer_vertices;
        let k = n
            .checked_mul(n)
            .ok_or(LayerUnionError::ArithmeticOverflow {
                section: "intersection worst case",
            })?;
        let log2 = (usize::BITS - n.max(1).leading_zeros()) as usize;
        let layer_operations = n
            .checked_add(k)
            .and_then(|work| work.checked_mul(log2))
            .ok_or(LayerUnionError::ArithmeticOverflow {
                section: "union operations",
            })?;
        operations = operations.checked_add(layer_operations).ok_or(
            LayerUnionError::ArithmeticOverflow {
                section: "union operations",
            },
        )?;
    }
    if input_vertices > limits.max_input_vertices {
        return Err(LayerUnionError::InputVertexCap {
            requested: input_vertices,
            cap: limits.max_input_vertices,
        });
    }
    if operations > limits.max_union_operations {
        return Err(LayerUnionError::UnionOperationCap {
            requested: operations,
            cap: limits.max_union_operations,
        });
    }
    let scratch = input_vertices
        .checked_mul(QUANTIZED_POINT_BYTES + ENGINE_EDGE_BYTES_MODEL)
        .and_then(|input_bytes| {
            limits
                .max_output_vertices
                .checked_mul(QUANTIZED_POINT_BYTES)
                .and_then(|output_bytes| input_bytes.checked_add(output_bytes))
        })
        .ok_or(LayerUnionError::ArithmeticOverflow {
            section: "scratch bytes",
        })?;
    if scratch > limits.max_scratch_bytes {
        return Err(LayerUnionError::ScratchBytesCap {
            requested: scratch,
            cap: limits.max_scratch_bytes,
        });
    }
    Ok(())
}

/// Normalize every layer of a batch in one call.
///
/// * `layers`: all stitched layers, each with the fill rule its phase-A
///   winding derivation warrants.
/// * `min_area_mm2`: the degenerate-output filter (the legacy stitcher's
///   `BIRTH_MIN_AREA_MM2` at the perception call sites), applied AFTER the
///   union on union output. Must be finite and non-negative.
/// * `limits`: explicit resource caps; see [`LayerUnionLimits`].
///
/// One rayon `par_iter` over layers (layers are independent) under the same
/// `PLINTH_THREADS` pool authority as the kernel's own batch calls,
/// deterministic assembly by layer index. Output is bit-reproducible across
/// thread counts and platforms: all geometric decisions happen in exact
/// integer arithmetic on explicitly quantized coordinates.
pub fn normalize_layers_batch(
    layers: &[LayerLoops],
    min_area_mm2: f64,
    limits: &LayerUnionLimits,
) -> Result<NormalizedLayers, LayerUnionError> {
    if !min_area_mm2.is_finite() || min_area_mm2 < 0.0 {
        return Err(LayerUnionError::InvalidMinArea {
            value_mm2: min_area_mm2,
        });
    }
    preflight(layers, limits)?;

    let effective_threads = geom_kernel::current_threads();
    let pool = rayon::ThreadPoolBuilder::new()
        .num_threads(effective_threads)
        .build()
        .map_err(|error| LayerUnionError::Pool(error.to_string()))?;
    let worker_mask = AtomicU64::new(0);
    let results: Vec<Result<NormalizedLayer, LayerUnionError>> = pool.install(|| {
        layers
            .par_iter()
            .enumerate()
            .map(|(layer_index, layer)| {
                if let Some(index) = rayon::current_thread_index() {
                    if index < 64 {
                        worker_mask.fetch_or(1u64 << index, Ordering::Relaxed);
                    }
                }
                normalize_one_layer(layer_index, layer, min_area_mm2)
            })
            .collect()
    });
    let layers_out = results.into_iter().collect::<Result<Vec<_>, _>>()?;

    let mut output_vertices = 0usize;
    for layer in &layers_out {
        for solid in &layer.solids {
            output_vertices = output_vertices
                .checked_add(solid.outer.points_xy_mm.len())
                .and_then(|count| {
                    solid.holes.iter().try_fold(count, |count, hole| {
                        count.checked_add(hole.points_xy_mm.len())
                    })
                })
                .ok_or(LayerUnionError::ArithmeticOverflow {
                    section: "output vertices",
                })?;
        }
    }
    if output_vertices > limits.max_output_vertices {
        return Err(LayerUnionError::OutputVertexCap {
            requested: output_vertices,
            cap: limits.max_output_vertices,
        });
    }

    Ok(NormalizedLayers {
        layers: layers_out,
        effective_threads,
        participating_threads: worker_mask.load(Ordering::Relaxed).count_ones() as usize,
    })
}
