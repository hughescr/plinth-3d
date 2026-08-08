//! Shared helpers for the plinth-layers test suites.
// Each test binary uses a different subset of these helpers.
#![allow(dead_code)]

use plinth_layers::{LayerFillRule, LayerLoops, LayerUnionLimits, NormalizedLayers};

/// Loose limits that no fixture in these suites can trip.
pub fn loose_limits() -> LayerUnionLimits {
    LayerUnionLimits {
        max_input_vertices: 1 << 20,
        max_output_vertices: 1 << 20,
        max_union_operations: usize::MAX / 4,
        max_scratch_bytes: 1 << 30,
    }
}

/// One-layer convenience wrapper.
pub fn normalize_layer(
    loops: Vec<Vec<[f64; 2]>>,
    fill_rule: LayerFillRule,
    min_area_mm2: f64,
) -> NormalizedLayers {
    plinth_layers::normalize_layers_batch(
        &[LayerLoops { loops, fill_rule }],
        min_area_mm2,
        &loose_limits(),
    )
    .expect("fixture layers must normalize")
}

/// Counter-clockwise rectangle starting at its minimum-key corner.
pub fn rect_ccw(x0: f64, y0: f64, x1: f64, y1: f64) -> Vec<[f64; 2]> {
    vec![[x0, y0], [x1, y0], [x1, y1], [x0, y1]]
}

/// Clockwise rectangle starting at its minimum-key corner.
pub fn rect_cw(x0: f64, y0: f64, x1: f64, y1: f64) -> Vec<[f64; 2]> {
    vec![[x0, y0], [x0, y1], [x1, y1], [x1, y0]]
}

/// Shoelace signed area, the same formula the crate emits.
pub fn shoelace(points: &[[f64; 2]]) -> f64 {
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

/// Even-odd point containment, the perception stitcher's exact formula.
pub fn point_in_polygon(point: [f64; 2], polygon: &[[f64; 2]]) -> bool {
    let mut inside = false;
    for i in 0..polygon.len() {
        let a = polygon[i];
        let b = polygon[(i + 1) % polygon.len()];
        if (a[1] > point[1]) != (b[1] > point[1])
            && point[0] < (b[0] - a[0]) * (point[1] - a[1]) / (b[1] - a[1]) + a[0]
        {
            inside = !inside;
        }
    }
    inside
}

/// Signed winding number of `point` with respect to `polygon`.
pub fn winding_number(point: [f64; 2], polygon: &[[f64; 2]]) -> i32 {
    let mut winding = 0i32;
    for i in 0..polygon.len() {
        let a = polygon[i];
        let b = polygon[(i + 1) % polygon.len()];
        let cross = (b[0] - a[0]) * (point[1] - a[1]) - (point[0] - a[0]) * (b[1] - a[1]);
        if a[1] <= point[1] {
            if b[1] > point[1] && cross > 0.0 {
                winding += 1;
            }
        } else if b[1] <= point[1] && cross < 0.0 {
            winding -= 1;
        }
    }
    winding
}

/// Distance from `point` to the closed polyline over `polygon`.
pub fn distance_to_boundary(point: [f64; 2], polygon: &[[f64; 2]]) -> f64 {
    let mut best = f64::INFINITY;
    for i in 0..polygon.len() {
        let a = polygon[i];
        let b = polygon[(i + 1) % polygon.len()];
        let d = [b[0] - a[0], b[1] - a[1]];
        let length2 = d[0] * d[0] + d[1] * d[1];
        let t = if length2 > 0.0 {
            (((point[0] - a[0]) * d[0] + (point[1] - a[1]) * d[1]) / length2).clamp(0.0, 1.0)
        } else {
            0.0
        };
        let nearest = [a[0] + t * d[0], a[1] + t * d[1]];
        best = best.min((point[0] - nearest[0]).hypot(point[1] - nearest[1]));
    }
    best
}
