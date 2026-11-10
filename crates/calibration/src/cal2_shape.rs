//! CAL-2 body shapes and load profiles. The caller gives the full body area (the target
//! ratio times the section sum, less a small guard); this module returns the parts, the
//! head sites and the top face for each variant. Dimensions are rounded down to 0.01 mm, so
//! no layer is ever over the target.
//!
//! Every step in height is a whole number of 0.05 mm layers, moved by `NUDGE` so that no
//! flat face lies exactly on a layer boundary. Plinth's slab envelope counts a face on a
//! boundary in both layers it touches, which would put the footprint below a step into the
//! layer above it. A step that grows is moved up, so the smaller footprint below that
//! intrudes into the layer above and is lost in the union; a step that shrinks, and every
//! top, is moved down for the same reason. The 0.5 micrometre move does not change which
//! footprint a layer's mid-plane cuts.
use crate::{
    Result,
    cal2_body::{self, Pads, Part, RING_FACETS, Shape},
    cal2_spec::{
        Item, LATE_HOLD_LAYERS, LAYER, LONG_FULL_LAYERS, LOW_FRACTION, RAMP_LAYERS, RING_WIDTH,
        SHORT_FULL_LAYERS, STRIP_ASPECT, Variant,
    },
};
use serde_json::{Value, json};

/// The top face that carries the tab and number: lower and upper corner, and its height.
pub type Top = ([f64; 2], [f64; 2], f64);

/// Move of a step off its layer boundary, mm.
pub const NUDGE: f64 = 0.0005;
/// Plinth's raft flares (1.1 mm) cannot be joined at a head pitch between these two values,
/// mm (found by building the 16-head coupon at every ratio, 28 to 47).
const FLARE_FAIL: (f64, f64) = (1.90, 2.25);
/// Pitches used instead: the narrow side of a rectangular grid, and the wide side's floor.
const NARROW_PITCH: f64 = 1.85;
const WIDE_PITCH: f64 = 2.30;

#[derive(Debug, Clone)]
pub struct Plan {
    pub parts: Vec<Part>,
    pub sites: Vec<[f64; 2]>,
    pub top: Option<Top>,
    /// Designed dimensions, for the report.
    pub dims: Value,
}

fn floor2(x: f64) -> f64 {
    (x * 100.0).floor() / 100.0
}

/// Height of `n` layers, mm.
#[must_use]
pub fn z(n: usize) -> f64 {
    n as f64 * LAYER
}

fn rect(c: [f64; 2], w: f64, h: f64) -> Shape {
    Shape::Rect {
        lo: [c[0] - w / 2.0, c[1] - h / 2.0],
        hi: [c[0] + w / 2.0, c[1] + h / 2.0],
    }
}

fn grid_sites(w: f64, h: f64, n: usize) -> Vec<[f64; 2]> {
    let (px, py) = (w / n as f64, h / n as f64);
    (0..n * n)
        .map(|i| {
            [
                -w / 2.0 + (i % n) as f64 * px + px / 2.0,
                -h / 2.0 + (i / n) as f64 * py + py / 2.0,
            ]
        })
        .collect()
}

/// A part from layer `a` to layer `b`, each end moved by its nudge.
fn part(shape: Shape, a: (usize, f64), b: (usize, f64)) -> Part {
    Part {
        shape,
        z0: z(a.0) + a.1,
        z1: z(b.0) + b.1,
    }
}

fn corners(shape: &Shape) -> ([f64; 2], [f64; 2]) {
    match shape {
        Shape::Rect { lo, hi } => (*lo, *hi),
        Shape::Ring { outer, .. } => ([-outer; 2], [*outer; 2]),
        Shape::Pads(p) => (
            p.origin,
            [
                p.origin[0] + p.cell[0] * p.n as f64,
                p.origin[1] + p.cell[1] * p.n as f64,
            ],
        ),
    }
}

/// Width and height of an `n` x `n` grid body of `area` mm². A square, unless the head
/// pitch would fall where plinth's flares cannot be joined; then a rectangle whose
/// pitches are on the two good sides of that window and whose area is the same.
fn extent(area: f64, n: usize) -> (f64, f64) {
    let pitch = area.sqrt() / n as f64;
    if n == 1 || pitch <= FLARE_FAIL.0 || pitch >= FLARE_FAIL.1 {
        let s = floor2(area.sqrt());
        return (s, s);
    }
    let cells = (n * n) as f64;
    let px = NARROW_PITCH.min(area / (cells * WIDE_PITCH));
    let py = area / (cells * px);
    (floor2(n as f64 * px), floor2(n as f64 * py))
}

fn square(item: &Item, hold: usize, area: f64) -> Result<Plan> {
    let n = item.grid();
    if n * n != item.heads {
        return Err(format!("{} heads is not a square grid", item.heads).into());
    }
    let (w, h) = extent(area, n);
    let full = rect([0.0; 2], w, h);
    let mut dims = json!({
        "shape": if (w - h).abs() < 1e-9 { "square" } else { "rectangle" },
        "width_mm": w, "height_mm": h,
        "head_pitch_mm": [w / n as f64, h / n as f64],
    });
    let mut parts = Vec::new();
    let (top_shape, top_layers) = match item.variant {
        Variant::ShortHold => {
            let scale = LOW_FRACTION.sqrt();
            let (sw, sh) = (floor2(w * scale), floor2(h * scale));
            let stem = rect([0.0; 2], sw, sh);
            parts.push(part(full.clone(), (0, 0.0), (SHORT_FULL_LAYERS, -NUDGE)));
            parts.push(part(
                stem.clone(),
                (SHORT_FULL_LAYERS, -NUDGE),
                (hold, -NUDGE),
            ));
            dims["stem_mm"] = json!([sw, sh]);
            dims["stem_fraction"] = json!(sw * sh / (w * h));
            dims["full_layers"] = json!(SHORT_FULL_LAYERS);
            (stem, hold)
        }
        Variant::LongHold => {
            parts.push(part(full.clone(), (0, 0.0), (LONG_FULL_LAYERS, -NUDGE)));
            dims["full_layers"] = json!(LONG_FULL_LAYERS);
            (full, LONG_FULL_LAYERS)
        }
        Variant::LateLoad => {
            // One pad per head, LOW_FRACTION..1 of its cell, growing each layer; one slab
            // over all of them once the ramp is done.
            let steps = (0..RAMP_LAYERS)
                .map(|j| {
                    let f = LOW_FRACTION + (1.0 - LOW_FRACTION) * j as f64 / RAMP_LAYERS as f64;
                    [w / n as f64 * f.sqrt(), h / n as f64 * f.sqrt()]
                })
                .collect();
            let levels = (0..=RAMP_LAYERS)
                .map(|j| z(j) + if j == 0 { 0.0 } else { NUDGE })
                .collect();
            let end = RAMP_LAYERS + LATE_HOLD_LAYERS;
            parts.push(part(
                Shape::Pads(Pads {
                    n,
                    origin: [-w / 2.0, -h / 2.0],
                    cell: [w / n as f64, h / n as f64],
                    steps,
                    z: levels,
                }),
                (0, 0.0),
                (end, -NUDGE),
            ));
            dims["ramp_layers"] = json!(RAMP_LAYERS);
            dims["hold_layers"] = json!(LATE_HOLD_LAYERS);
            dims["first_layer_fraction"] = json!(LOW_FRACTION);
            (full, end)
        }
        _ => {
            parts.push(part(full.clone(), (0, 0.0), (hold, -NUDGE)));
            dims["hold_layers"] = json!(hold);
            (full, hold)
        }
    };
    let (lo, hi) = corners(&top_shape);
    Ok(Plan {
        parts,
        sites: grid_sites(w, h, n),
        top: Some((lo, hi, z(top_layers) - NUDGE)),
        dims,
    })
}

fn strip(item: &Item, hold: usize, area: f64) -> Plan {
    let w = floor2((area / STRIP_ASPECT).sqrt());
    let l = STRIP_ASPECT * w;
    let shape = rect([0.0; 2], l, w);
    let (lo, hi) = corners(&shape);
    let heads = item.heads as f64;
    Plan {
        parts: vec![part(shape, (0, 0.0), (hold, -NUDGE))],
        sites: (0..item.heads)
            .map(|i| [-l / 2.0 + (i as f64 + 0.5) * l / heads, 0.0])
            .collect(),
        top: Some((lo, hi, z(hold) - NUDGE)),
        dims: json!({"shape": "strip", "long_mm": l, "short_mm": w, "aspect": STRIP_ASPECT,
                     "rows": 1, "head_pitch_mm": l / heads, "hold_layers": hold}),
    }
}

fn ring(item: &Item, hold: usize, area: f64) -> Plan {
    let n = RING_FACETS as f64;
    let k = 0.5 * n * (std::f64::consts::TAU / n).sin();
    let outer = floor2((area / k + RING_WIDTH * RING_WIDTH) / (2.0 * RING_WIDTH));
    let inner = outer - RING_WIDTH;
    let mid = outer - RING_WIDTH / 2.0;
    Plan {
        parts: vec![part(Shape::Ring { outer, inner }, (0, 0.0), (hold, -NUDGE))],
        sites: (0..item.heads)
            .map(|i| {
                let a = std::f64::consts::TAU * (i as f64 + 0.5) / item.heads as f64;
                [mid * a.cos(), mid * a.sin()]
            })
            .collect(),
        top: None,
        dims: json!({"shape": "ring", "outer_radius_mm": outer, "inner_radius_mm": inner,
                     "width_mm": RING_WIDTH, "centre_line_radius_mm": mid,
                     "hold_layers": hold}),
    }
}

/// Parts, sites and top face for `item`, with `area` mm² at full load.
///
/// # Errors
/// A head count that does not fit the variant.
pub fn plan(item: &Item, hold_layers: usize, area: f64) -> Result<Plan> {
    match item.variant {
        Variant::Strip => Ok(strip(item, hold_layers, area)),
        Variant::Ring => Ok(ring(item, hold_layers, area)),
        _ => square(item, hold_layers, area),
    }
}

/// Footprint area of one part, mm².
fn area_of(shape: &Shape, mid: f64) -> f64 {
    match shape {
        Shape::Rect { lo, hi } => (hi[0] - lo[0]) * (hi[1] - lo[1]),
        Shape::Ring { outer, inner } => {
            cal2_body::polygon_area(*outer) - cal2_body::polygon_area(*inner)
        }
        Shape::Pads(p) => {
            let cells = (p.n * p.n) as f64;
            // The slab covers everything from the top of the last pad.
            match p.z.iter().rposition(|&q| q < mid) {
                Some(j) if j < p.steps.len() => cells * p.steps[j][0] * p.steps[j][1],
                _ => cells * p.cell[0] * p.cell[1],
            }
        }
    }
}

/// Area of the footprint of one layer of the plan, mm², from the designed dimensions.
#[must_use]
pub fn layer_area(p: &Plan, layer: usize) -> f64 {
    let mid = (layer as f64 + 0.5) * LAYER;
    p.parts
        .iter()
        .filter(|q| q.z0 < mid && mid < q.z1)
        .map(|q| area_of(&q.shape, mid))
        .sum()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cal2_spec::Kind;

    fn same(a: f64, b: f64) -> bool {
        (a - b).abs() < 1e-9
    }

    fn item(variant: Variant, heads: usize) -> Item {
        Item {
            row: 0,
            priority: 1,
            kind: Kind::Head,
            variant,
            neck_mm: 0.55,
            heads,
            ratio: 40.0,
            copy: 0,
            copies: 1,
        }
    }

    #[test]
    fn standard_holds_the_full_area_for_every_layer_and_never_exceeds_it() {
        let p = plan(&item(Variant::Standard, 9), 40, 77.0).unwrap();
        for l in 0..40 {
            let a = layer_area(&p, l);
            assert!(a <= 77.0 && a > 77.0 * 0.995, "{a}");
        }
        assert!(same(layer_area(&p, 40), 0.0));
        assert_eq!(p.sites.len(), 9);
    }

    #[test]
    fn short_hold_steps_to_a_stem_of_at_most_forty_percent() {
        let p = plan(&item(Variant::ShortHold, 9), 40, 77.0).unwrap();
        let full = layer_area(&p, 0);
        for l in 0..5 {
            assert!(same(layer_area(&p, l), full));
        }
        for l in 5..40 {
            assert!(layer_area(&p, l) <= 0.4 * full + 1e-9);
            assert!(layer_area(&p, l) > 0.35 * full);
        }
        // Every head site is under the full part, not the stem.
        let side = full.sqrt();
        assert!(p.sites.iter().all(|s| s[0].abs() < side / 2.0));
    }

    #[test]
    fn long_hold_is_a_hundred_full_layers() {
        let p = plan(&item(Variant::LongHold, 9), 40, 77.0).unwrap();
        let full = layer_area(&p, 0);
        assert!((0..100).all(|l| same(layer_area(&p, l), full)));
        assert!(same(layer_area(&p, 100), 0.0));
    }

    #[test]
    fn late_load_starts_low_ramps_linearly_then_holds() {
        let p = plan(&item(Variant::LateLoad, 9), 40, 77.0).unwrap();
        let full = layer_area(&p, 79);
        assert!(layer_area(&p, 0) <= 0.4 * full + 1e-6);
        assert!((layer_area(&p, 0) / full - 0.4).abs() < 1e-6);
        for l in 1..40 {
            assert!(layer_area(&p, l) > layer_area(&p, l - 1));
            assert!(layer_area(&p, l) < full);
            let want = 0.4 + 0.6 * l as f64 / 40.0;
            assert!((layer_area(&p, l) / full - want).abs() < 1e-6);
        }
        assert!((40..80).all(|l| same(layer_area(&p, l), full)));
        assert!(same(layer_area(&p, 80), 0.0));
        // One stepped pad per head in a single part, then the slab on top of them.
        assert_eq!(p.parts.len(), 1);
    }

    #[test]
    fn strip_is_five_to_one_and_ring_is_three_wide() {
        let s = plan(&item(Variant::Strip, 8), 40, 70.0).unwrap();
        let Shape::Rect { lo, hi } = &s.parts[0].shape else {
            panic!()
        };
        assert!(((hi[0] - lo[0]) / (hi[1] - lo[1]) - 5.0).abs() < 1e-9);
        assert!(layer_area(&s, 0) <= 70.0);
        assert!(s.sites.iter().all(|p| p[1] == 0.0));
        let r = plan(&item(Variant::Ring, 12), 40, 100.0).unwrap();
        let Shape::Ring { outer, inner } = &r.parts[0].shape else {
            panic!()
        };
        assert!((outer - inner - 3.0).abs() < 1e-9);
        assert!(layer_area(&r, 0) <= 100.0 && layer_area(&r, 0) > 99.0 * 0.95);
        let mid = outer - 1.5;
        assert!(
            r.sites
                .iter()
                .all(|p| (p[0].hypot(p[1]) - mid).abs() < 1e-9)
        );
        assert!(r.top.is_none());
    }

    #[test]
    fn non_square_head_count_is_refused_for_square_variants() {
        assert!(plan(&item(Variant::Standard, 8), 40, 50.0).is_err());
    }

    #[test]
    fn sixteen_heads_avoid_the_flare_window_with_the_same_area() {
        for area in [60.0, 64.0, 70.0, 73.8, 75.0] {
            let (w, h) = extent(area, 4);
            assert!(w * h <= area && w * h > area * 0.985, "{area}: {w} x {h}");
            for p in [w / 4.0, h / 4.0] {
                assert!(
                    p <= FLARE_FAIL.0 || p >= FLARE_FAIL.1,
                    "{area}: pitch {p} is in the window"
                );
            }
        }
        // Outside the window it stays square.
        let (w, h) = extent(46.0, 4);
        assert!(same(w, h));
        let (w, h) = extent(100.0, 4);
        assert!(same(w, h));
    }
}
