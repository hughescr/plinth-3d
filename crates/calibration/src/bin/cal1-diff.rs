//! Read-only, sparse, exact native CAL-1 raster comparison. Counts grey changes,
//! not just changes in lit-any area. Every differing interval is retained.
use plinth_calibration::{
    Result,
    goo::{Goo, Span},
    goo_grid, plate,
};
use serde_json::{Value, json};
use std::{collections::BTreeMap, path::Path};

#[derive(Clone, Copy)]
struct Rect {
    lo: [i64; 2],
    hi: [i64; 2],
}
fn rectangle(model: &plinth_chitubox::Model, goo: &Goo) -> Rect {
    let b = plate::bounds(model);
    let pitch = goo.header.pitch();
    let centre = goo.header.resolution.map(|v| f64::from(v) / 2.0 + 0.5);
    Rect {
        lo: std::array::from_fn(|k| (centre[k] - b.hi[k] / pitch[k]).floor() as i64 - 2),
        hi: std::array::from_fn(|k| (centre[k] - b.lo[k] / pitch[k]).ceil() as i64 + 2),
    }
}
fn crop(spans: &[Span], rect: Rect, shift: [i64; 2]) -> Result<Vec<Span>> {
    spans
        .iter()
        .filter(|s| i64::from(s.y) >= rect.lo[1] && i64::from(s.y) < rect.hi[1])
        .filter_map(|s| {
            let x0 = i64::from(s.x0).max(rect.lo[0]);
            let x1 = i64::from(s.x1).min(rect.hi[0]);
            (x0 < x1).then_some((s, x0, x1))
        })
        .map(|(s, x0, x1)| {
            Ok(Span {
                x0: u32::try_from(x0 + shift[0])?,
                x1: u32::try_from(x1 + shift[0])?,
                y: u32::try_from(i64::from(s.y) + shift[1])?,
                value: s.value,
            })
        })
        .collect()
}
/// Sweep span endpoints; no dense image or per-pixel calls. Each output run is
/// [y, x_start_inclusive, x_end_exclusive, old_grey, new_grey].
fn differences(a: &[Span], b: &[Span]) -> Vec<[i64; 5]> {
    let mut events = Vec::with_capacity(2 * (a.len() + b.len()));
    for (side, spans) in [a, b].into_iter().enumerate() {
        for s in spans {
            events.push((s.y, s.x0, side, i64::from(s.value)));
            events.push((s.y, s.x1, side, -i64::from(s.value)));
        }
    }
    events.sort_unstable_by_key(|e| (e.0, e.1));
    let mut result = Vec::new();
    let mut values = [0_i64; 2];
    let mut i = 0;
    while i < events.len() {
        let (y, x, _, _) = events[i];
        while i < events.len() && (events[i].0, events[i].1) == (y, x) {
            values[events[i].2] += events[i].3;
            i += 1;
        }
        if let Some(&(next_y, next_x, _, _)) = events.get(i)
            && next_y == y
            && next_x > x
            && values[0] != values[1]
        {
            result.push([
                i64::from(y),
                i64::from(x),
                i64::from(next_x),
                values[0],
                values[1],
            ]);
        }
    }
    result
}
/// Lit-any inclusion in the other mask's one-row Chebyshev dilation.
fn outside_one_row(a: &[Span], b: &[Span]) -> u64 {
    let mut rows = BTreeMap::<i64, Vec<(i64, i64)>>::new();
    for s in b {
        for dy in -1..=1 {
            rows.entry(i64::from(s.y) + dy)
                .or_default()
                .push((i64::from(s.x0) - 1, i64::from(s.x1) + 1));
        }
    }
    for row in rows.values_mut() {
        row.sort_unstable();
        let mut merged: Vec<(i64, i64)> = Vec::new();
        for &(lo, hi) in row.iter() {
            if let Some(last) = merged.last_mut()
                && lo <= last.1
            {
                last.1 = last.1.max(hi);
            } else {
                merged.push((lo, hi));
            }
        }
        *row = merged;
    }
    a.iter()
        .map(|s| {
            let covered: i64 = rows
                .get(&i64::from(s.y))
                .into_iter()
                .flatten()
                .map(|&(lo, hi)| (hi.min(i64::from(s.x1)) - lo.max(i64::from(s.x0))).max(0))
                .sum();
            u64::from(s.x1 - s.x0) - covered as u64
        })
        .sum()
}
fn run() -> Result<()> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    let [project, old, new, output] = args.as_slice() else {
        return Err("usage: cal1-diff PROJECT OLD_GOO NEW_GOO NEW_OUTPUT_DIRECTORY".into());
    };
    let project = plinth_chitubox::parse(&std::fs::read(project)?)?;
    let old = Goo::parse(&std::fs::read(old)?)?;
    let new = Goo::parse(&std::fs::read(new)?)?;
    if old.header.resolution != new.header.resolution
        || old.header.pitch().map(f64::to_bits) != new.header.pitch().map(f64::to_bits)
        || old.layers.len() != new.layers.len()
    {
        return Err("native grids differ".into());
    }
    let rects: Vec<_> = project.models.iter().map(|m| rectangle(m, &new)).collect();
    let pairs = [(3_usize, 4_usize), (8, 9)];
    let pitch = new.header.pitch();
    let shifts: Vec<[i64; 2]> = pairs
        .iter()
        .map(|&(dest, src)| {
            let a = goo_grid::offset(&project.models[dest]);
            let b = goo_grid::offset(&project.models[src]);
            std::array::from_fn(|k| ((b[k] - a[k]) / pitch[k]).round() as i64)
        })
        .collect();
    for (p, &(dest, src)) in pairs.iter().enumerate() {
        for (k, axis_pitch) in pitch.iter().enumerate() {
            let delta = (goo_grid::offset(&project.models[src])[k]
                - goo_grid::offset(&project.models[dest])[k])
                / axis_pitch;
            if (delta - shifts[p][k] as f64).abs() > 0.001 {
                return Err("replicate offset is not whole-pixel".into());
            }
        }
        if shifts[p] != [2217, 0] {
            return Err("unexpected CAL-1 replicate shift".into());
        }
    }
    let mut locations = Vec::new();
    let mut counts = BTreeMap::<String, u64>::new();
    let mut replica_counts = [0_u64; 2];
    let mut replica_lit_only = [0_u64; 2];
    let mut replica_outside_one_row = [[0_u64; 2]; 2];
    let mut replica_pixels = [[0_u64; 2]; 2];
    let mut replica_locations = Vec::new();
    let mut definition_changes = Vec::new();
    for li in 0..new.layers.len() {
        if old.layers[li].definition != new.layers[li].definition {
            definition_changes.push(li);
        }
        let a = old.spans(li)?;
        let b = new.spans(li)?;
        let diffs = differences(&a, &b);
        for r in &diffs {
            let matches: Vec<_> = rects
                .iter()
                .enumerate()
                .filter(|(_, rect)| {
                    r[1] >= rect.lo[0]
                        && r[2] <= rect.hi[0]
                        && r[0] >= rect.lo[1]
                        && r[0] < rect.hi[1]
                })
                .collect();
            let name = if let [(i, _)] = matches.as_slice() {
                project.models[*i].name.clone()
            } else {
                "OUTSIDE_OR_AMBIGUOUS".into()
            };
            *counts.entry(name).or_default() += (r[2] - r[1]) as u64;
        }
        if !diffs.is_empty() {
            locations.push(json!({"layer_0based":li,"runs_y_x0_x1_old_new":diffs}));
        }
        for (p, &(dest, src)) in pairs.iter().enumerate() {
            let a = crop(&b, rects[src], shifts[p])?;
            let dest_spans = crop(&b, rects[dest], [0, 0])?;
            let diff = differences(&a, &dest_spans);
            replica_counts[p] += diff.iter().map(|r| (r[2] - r[1]) as u64).sum::<u64>();
            replica_lit_only[p] += diff
                .iter()
                .filter(|r| (r[3] == 0) != (r[4] == 0))
                .map(|r| (r[2] - r[1]) as u64)
                .sum::<u64>();
            replica_outside_one_row[p][0] += outside_one_row(&a, &dest_spans);
            replica_outside_one_row[p][1] += outside_one_row(&dest_spans, &a);
            replica_pixels[p][0] += a.iter().map(|s| u64::from(s.x1 - s.x0)).sum::<u64>();
            replica_pixels[p][1] += dest_spans
                .iter()
                .map(|s| u64::from(s.x1 - s.x0))
                .sum::<u64>();
            if !diff.is_empty() {
                replica_locations.push(json!({"pair":p,"layer_0based":li,"runs_y_x0_x1_source_shifted_destination":diff}));
            }
        }
    }
    let old_header = serde_json::to_value(&old.header)?;
    let new_header = serde_json::to_value(&new.header)?;
    let header_changes: Vec<Value> = new_header
        .as_object()
        .unwrap()
        .iter()
        .filter(|(k, v)| old_header[*k] != **v)
        .map(|(k, v)| json!({"field":k,"old":old_header[k],"new":v}))
        .collect();
    let summary = json!({"layers_compared":new.layers.len(),"comparison":"Exact greyscale pixel equality; each changed XY pixel counted once per layer. Locations use native mirrored raster coordinates, not world mm.","header_changes":header_changes,"layer_definition_changes_0based":definition_changes,"changed_pixels_by_piece":counts,"changed_layer_count":locations.len(),"replicates":pairs.iter().enumerate().map(|(p,&(d,s))|json!({"source":project.models[s].name,"destination":project.models[d].name,"source_to_destination_shift_pixels":shifts[p],"differing_pixels":replica_counts[p],"lit_any_differing_pixels":replica_lit_only[p],"lit_any_pixels_source_destination":replica_pixels[p],"lit_any_outside_one_row_source_destination":replica_outside_one_row[p],"one_row_diagnostic":"Diagnostic only, not a waiver of the exact replicate equality check"})).collect::<Vec<_>>()});
    std::fs::create_dir(output)?;
    std::fs::write(
        Path::new(output).join("summary.json"),
        serde_json::to_vec_pretty(&summary)?,
    )?;
    std::fs::write(
        Path::new(output).join("locations.json"),
        serde_json::to_vec(&json!({"old_vs_new":locations,"replicates":replica_locations}))?,
    )?;
    println!("{}", serde_json::to_string_pretty(&summary)?);
    Ok(())
}
fn main() {
    if let Err(e) = run() {
        eprintln!("{e}");
        std::process::exit(1);
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn s(x0: u32, x1: u32, value: u8) -> Span {
        Span {
            y: 2,
            x0,
            x1,
            value,
        }
    }
    #[test]
    fn sparse_diff_counts_grey_not_only_occupancy_and_ignores_span_splits() {
        assert!(differences(&[s(1, 4, 255)], &[s(1, 2, 255), s(2, 4, 255)]).is_empty());
        assert_eq!(
            differences(&[s(1, 4, 255)], &[s(2, 3, 128)]),
            vec![[2, 1, 2, 255, 0], [2, 2, 3, 255, 128], [2, 3, 4, 255, 0]]
        );
        assert!(differences(&[], &[]).is_empty());
    }
    #[test]
    fn one_row_diagnostic_rejects_a_missing_interior_strip() {
        let a = [s(1, 6, 255)];
        assert_eq!(outside_one_row(&a, &[s(2, 5, 128)]), 0);
        assert_eq!(outside_one_row(&a, &[s(1, 2, 255), s(5, 6, 255)]), 1);
        assert_eq!(outside_one_row(&a, &[]), 5);
    }
    #[test]
    fn cropped_translation_preserves_grey_and_detects_missing_pixel() {
        let rect = Rect {
            lo: [1, 2],
            hi: [4, 3],
        };
        let shifted = crop(&[s(0, 5, 128)], rect, [10, 20]).unwrap();
        assert!(
            differences(
                &shifted,
                &[Span {
                    y: 22,
                    x0: 11,
                    x1: 14,
                    value: 128
                }]
            )
            .is_empty()
        );
        assert_eq!(
            differences(
                &shifted,
                &[Span {
                    y: 22,
                    x0: 11,
                    x1: 13,
                    value: 128
                }]
            ),
            vec![[22, 13, 14, 128, 0]]
        );
    }
}
