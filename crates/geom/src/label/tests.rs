use super::*;
use crate::raster::PIXEL_PITCH_MM;

/// A stack from ASCII art: `#` is a filled pixel; every layer has the same size.
fn art(layers: &[&[&str]]) -> RasterStack {
    let h = layers[0].len() as u32;
    let w = layers
        .iter()
        .flat_map(|l| l.iter())
        .map(|r| r.len())
        .max()
        .unwrap_or(0) as u32;
    let rasters = layers
        .iter()
        .enumerate()
        .map(|(k, rows)| {
            let mut runs = Vec::new();
            for (y, row) in rows.iter().enumerate() {
                let b = row.as_bytes();
                let mut x = 0;
                while x < b.len() {
                    if b[x] == b'#' {
                        let a = x;
                        while x < b.len() && b[x] == b'#' {
                            x += 1;
                        }
                        runs.push(Run {
                            y: y as u32,
                            x0: a as u32,
                            x1: x as u32,
                        });
                    } else {
                        x += 1;
                    }
                }
            }
            LayerRaster {
                layer: k as u32,
                runs,
            }
        })
        .collect();
    RasterStack {
        pitch: PIXEL_PITCH_MM,
        origin: [0.0, 0.0],
        size: [w, h],
        layers: rasters,
    }
}

fn labels(st: &RasterStack) -> Labels {
    match label_components(st) {
        Measure::Known(l) => l,
        Measure::Unverified(w) => panic!("{w}"),
    }
}

const BLOCKS: [&str; 5] = [
    "..............",
    ".####....####.",
    ".####....####.",
    ".####....####.",
    "..............",
];
const BRIDGED: [&str; 5] = [
    "..............",
    ".############.",
    ".############.",
    ".############.",
    "..............",
];

#[test]
fn two_blocks_that_join_in_a_later_layer_merge_there() {
    let st = art(&[&BLOCKS, &BLOCKS, &BRIDGED, &BRIDGED]);
    let l = labels(&st);
    assert_eq!(l.components.len(), 2);
    assert_eq!(l.components[0].first_layer, 0);
    assert_eq!(l.components[1].merged_into, Some((0, 2)));
    assert_eq!(l.components[0].merged_into, None);
    // Layer 1: two components; layer 2 and 3: one.
    let ids = |k: usize| {
        let mut v = l.run_component[k].clone();
        v.sort_unstable();
        v.dedup();
        v
    };
    assert_eq!(ids(1), vec![0, 1]);
    assert_eq!(ids(2), vec![0]);
    assert_eq!(ids(3), vec![0]);
    // Extents of the loser stop at its last own layer: 4 wide, 3 tall.
    assert_eq!(l.components[1].first_layer_width_px, 4);
    assert_eq!(l.components[1].first_layer_height_px, 3);
}

#[test]
fn a_small_island_that_joins_a_big_body_is_the_one_marked_merged() {
    let big = [
        "................",
        ".##############.",
        ".##############.",
        ".##############.",
        ".##############.",
        "................",
    ];
    let with_dot = [
        "................",
        ".##############.",
        ".##############.",
        ".##############.",
        ".##############.",
        "................",
    ];
    let floating = [
        "................",
        "................",
        "....##..........",
        "....##..........",
        "................",
        "................",
    ];
    // Layer 0 big body, layer 1 body only, layer 2 a floating dot above empty space is
    // impossible without a body: use the dot beside the body, and join it a layer later.
    let beside = [
        "..................",
        ".#####.....##.....",
        ".#####.....##.....",
        ".#####............",
        "..................",
    ];
    let joined = [
        "..................",
        ".#############....",
        ".#############....",
        ".#####............",
        "..................",
    ];
    let _ = (big, with_dot, floating);
    let st = art(&[&beside, &beside, &joined]);
    let l = labels(&st);
    assert_eq!(l.components.len(), 2);
    let dot = l.components[1];
    assert_eq!(dot.first_layer, 0);
    assert_eq!(dot.merged_into, Some((0, 2)));
    assert_eq!(
        (dot.first_layer_width_px, dot.first_layer_height_px),
        (2, 2)
    );
}

#[test]
fn the_run_labels_equal_the_dense_flood_fill_reference() {
    let a = [
        "....................",
        ".###..###...#.......",
        ".#.#..#.#..###...##.",
        ".###..###...#....##.",
        "....................",
    ];
    let b = [
        "....................",
        ".##########..#......",
        ".#.#..#.#..####..##.",
        ".##########.#....##.",
        "....................",
    ];
    let c = [
        "....................",
        ".##.....##..........",
        ".##.....##..####....",
        "............####....",
        "....................",
    ];
    let st = art(&[&a, &b, &c, &a, &c]);
    let l = labels(&st);
    assert_eq!(reference::image_of(&st, &l), reference::label_dense(&st));
}

#[test]
fn a_batch_call_counts_one_call_with_one_item_per_layer() {
    counters::reset();
    let st = art(&[&BLOCKS, &BLOCKS, &BRIDGED]);
    let _ = labels(&st);
    let c = counters::snapshot(Op::LabelComponents);
    assert_eq!((c.calls, c.items), (1, 3));
}

const DISC: [&str; 9] = [
    "...........",
    ".#########.",
    ".#########.",
    ".#########.",
    ".#########.",
    ".#########.",
    ".#########.",
    ".#########.",
    "...........",
];
const RING: [&str; 9] = [
    "...........",
    ".#########.",
    ".#########.",
    ".##.....##.",
    ".##.....##.",
    ".##.....##.",
    ".#########.",
    ".#########.",
    "...........",
];
const RING_WITH_SLOT: [&str; 9] = [
    "...........",
    ".#########.",
    ".#########.",
    ".##.....##.",
    ".##........",
    ".##.....##.",
    ".#########.",
    ".#########.",
    "...........",
];

#[test]
fn a_hole_over_filled_pixels_with_a_closed_wall_is_a_cup() {
    let st = art(&[&DISC, &RING, &RING]);
    let cups = find_cups(&st).known().cloned().unwrap();
    assert_eq!(cups.len(), 2, "{cups:?}");
    assert_eq!((cups[0].layer, cups[0].pixels), (1, 15));
    assert_eq!(cups[1].layer, 2);
    assert!((cups[0].centre_px[0] - 5.5).abs() < 1e-9);
}

#[test]
fn a_hole_with_an_opening_is_not_a_cup_and_neither_is_the_first_layer() {
    let st = art(&[&RING, &DISC, &RING_WITH_SLOT]);
    let cups = find_cups(&st).known().cloned().unwrap();
    assert!(cups.is_empty(), "{cups:?}");
}

#[test]
fn a_hole_that_is_open_below_stays_open_above() {
    // Layer 1 hole is open (slot); layer 2 hole is closed in plane but sits over layer 1's
    // open gap, so resin can drain downward and it is not a cup.
    let st = art(&[&DISC, &RING_WITH_SLOT, &RING]);
    let cups = find_cups(&st).known().cloned().unwrap();
    assert!(cups.is_empty(), "{cups:?}");
}

#[test]
fn the_distance_transform_matches_brute_force() {
    let (w, h) = (9, 7);
    let mut f = vec![f64::INFINITY; w * h];
    f[3 * w + 2] = 0.0;
    f[w + 7] = 0.0;
    f[6 * w] = 0.0;
    let want: Vec<f64> = (0..w * h)
        .map(|i| {
            let (x, y) = ((i % w) as f64, (i / w) as f64);
            [(2.0, 3.0), (7.0, 1.0), (0.0, 6.0)]
                .iter()
                .map(|&(sx, sy)| ((x - sx) * 0.014).powi(2) + ((y - sy) * 0.019).powi(2))
                .fold(f64::INFINITY, f64::min)
        })
        .collect();
    let got = squared_distance(&mut f, w, h, 0.014, 0.019);
    for (g, e) in got.iter().zip(&want) {
        assert!((g - e).abs() < 1e-12, "{g} {e}");
    }
}

#[test]
fn a_wide_shelf_reaches_far_and_a_narrow_lip_does_not() {
    let base = [
        "......................................",
        ".######..............................",
        ".######..............................",
        ".######..............................",
        "......................................",
    ];
    let shelf = [
        "......................................",
        ".###################..................",
        ".###################..................",
        ".###################..................",
        "......................................",
    ];
    // New pixels are columns 7..19: 13 wide from the supported edge at column 6.
    let st = art(&[&base, &shelf]);
    let wide = find_reach(&st, 0.1).known().cloned().unwrap();
    assert_eq!(wide.len(), 1, "{wide:?}");
    // The farthest new pixel is column 19, 13 pixels from the supported column 6. The reach is
    // measured on cells of 4 x 4 pixels, so it is within one cell of the pixel value.
    assert!(
        (wide[0].reach_mm - 13.0 * 0.014).abs() < 0.06,
        "{}",
        wide[0].reach_mm
    );
    assert_eq!(wide[0].pixels, 3 * 13);
    let none = find_reach(&st, 0.5).known().cloned().unwrap();
    assert!(none.is_empty(), "{none:?}");
}

#[test]
fn material_with_nothing_supported_nearby_has_infinite_reach() {
    let base = [
        "..........................",
        ".###.....................",
        ".###.....................",
        "..........................",
    ];
    let island = [
        "..........................",
        ".###..............##......",
        ".###..............##......",
        "..........................",
    ];
    let st = art(&[&base, &island]);
    let r = find_reach(&st, 0.05).known().cloned().unwrap();
    assert_eq!(r.len(), 1);
    assert!(r[0].reach_mm.is_infinite());
}
