//! CAL-2 assembly: build every coupon with plinth's head path, lay them out, write the
//! ChituBox project, re-read it, then measure and report from the re-read file only.
use crate::{
    Result, cal2_body, cal2_head,
    cal2_layout::{self, Footprint},
    cal2_map::{self, MapCoupon},
    cal2_measure,
    cal2_spec::{Cal2Spec, Item, Kind, LAYER, SHORT_FULL_LAYERS, Variant},
    goo_grid::Grid,
    plate,
};
use plinth_chitubox::{Model, Project};
use serde_json::{Value, json};
use std::path::Path;

pub const PROJECT: &str = "CAL2-plate-calibration-not-print-candidate.chitubox";

fn sha(path: &Path) -> Result<String> {
    let out = std::process::Command::new("shasum")
        .args(["-a", "256"])
        .arg(path)
        .output()?;
    if !out.status.success() {
        return Err("shasum failed".into());
    }
    Ok(String::from_utf8(out.stdout)?
        .split_whitespace()
        .next()
        .ok_or("no hash")?
        .into())
}

/// The coupon number written in a model name, `C07 ...`.
fn number(m: &Model) -> Result<u32> {
    let digits: String = m
        .name
        .strip_prefix('C')
        .ok_or("model name lacks its coupon number")?
        .chars()
        .take_while(char::is_ascii_digit)
        .collect();
    Ok(digits.parse()?)
}

fn label(spec: &Cal2Spec, k: usize, i: &Item) -> String {
    // A witness's "R6" would read as row R6.
    let ratio = if i.kind == Kind::Witness {
        format!("ratio {:.0}", i.ratio)
    } else {
        format!("R{:.0}", i.ratio)
    };
    format!(
        "C{:02} {} {:.2} mm x{} {ratio} {} copy {}",
        k + 1,
        spec.rows[i.row].id,
        i.neck_mm,
        i.heads,
        i.variant.name(),
        i.copy + 1
    )
}

struct Written {
    project: Project,
    gap: f64,
    /// Least distance from any part's footprint (raft included) to an edge of the build area.
    margin: f64,
    identical: bool,
    hash: String,
}

/// Least distance from any model's footprint, supports included, to each edge of the build
/// area, mm.
fn edge_margin(models: &[Model]) -> f64 {
    models
        .iter()
        .map(plate::bounds)
        .flat_map(|b| {
            (0..2).flat_map(move |k| {
                [
                    b.lo[k] + plate::BUILD[k] / 2.0,
                    plate::BUILD[k] / 2.0 - b.hi[k],
                ]
            })
        })
        .fold(f64::INFINITY, f64::min)
}

fn write(path: &Path, models: Vec<Model>) -> Result<Written> {
    let project = Project {
        preview: Some(crate::blank_preview()),
        models,
        ..Project::default()
    };
    let bytes = project.to_bytes()?;
    std::fs::write(path, &bytes)?;
    let read = plinth_chitubox::parse(&std::fs::read(path)?)?;
    // The repo's CAL-1 round trip: the file parses and writes back byte for byte.
    let identical = read.to_bytes()? == bytes;
    let gap = plate::validate(&read.models)?;
    let margin = edge_margin(&read.models);
    if margin < plate::EDGE_MARGIN {
        return Err(format!(
            "a part is {margin:.3} mm from the plate edge, under the {} mm margin",
            plate::EDGE_MARGIN
        )
        .into());
    }
    Ok(Written {
        project: read,
        gap,
        margin,
        identical,
        hash: sha(path)?,
    })
}

fn map_coupon(spec: &Cal2Spec, item: &Item, n: u32, m: &Model, sites: Vec<[f64; 2]>) -> MapCoupon {
    let b = plate::bounds(m);
    let t = [f64::from(m.translate[0]), f64::from(m.translate[1])];
    let (mut lo, mut hi) = ([f64::INFINITY; 2], [f64::NEG_INFINITY; 2]);
    for p in m.mesh.iter().flatten() {
        for k in 0..2 {
            lo[k] = lo[k].min(f64::from(p[k]) + t[k]);
            hi[k] = hi[k].max(f64::from(p[k]) + t[k]);
        }
    }
    let zmin = m
        .mesh
        .iter()
        .flatten()
        .map(|p| f64::from(p[2]))
        .fold(f64::INFINITY, f64::min);
    let centre = [(lo[0] + hi[0]) / 2.0, (lo[1] + hi[1]) / 2.0];
    // A ring's inner radius is the nearest bottom-face corner to the centre.
    let ring_inner = (item.variant == Variant::Ring).then(|| {
        m.mesh
            .iter()
            .flatten()
            .filter(|p| f64::from(p[2]) < zmin + 1e-6)
            .map(|p| (f64::from(p[0]) + t[0] - centre[0]).hypot(f64::from(p[1]) + t[1] - centre[1]))
            .fold(f64::INFINITY, f64::min)
    });
    // A stem is the footprint of every vertex above the full layers.
    let stem = (item.variant == Variant::ShortHold).then(|| {
        let cut = zmin + (SHORT_FULL_LAYERS as f64 + 0.5) * LAYER;
        let (mut slo, mut shi) = ([f64::INFINITY; 2], [f64::NEG_INFINITY; 2]);
        for p in m.mesh.iter().flatten().filter(|p| f64::from(p[2]) > cut) {
            for k in 0..2 {
                slo[k] = slo[k].min(f64::from(p[k]) + t[k]);
                shi[k] = shi[k].max(f64::from(p[k]) + t[k]);
            }
        }
        [slo, shi]
    });
    MapCoupon {
        number: n,
        row: spec.rows[item.row].id.clone(),
        kind: item.kind,
        variant: item.variant,
        neck_mm: item.neck_mm,
        heads: item.heads,
        ratio: item.ratio,
        foot: [[b.lo[0], b.lo[1]], [b.hi[0], b.hi[1]]],
        body: [lo, hi],
        ring_inner,
        stem,
        sites: sites.iter().map(|s| [s[0] + t[0], s[1] + t[1]]).collect(),
        control: spec.rows[item.row].id == "R8",
    }
}

/// Identity, design and placement fields of one coupon's report entry.
fn annotate(
    j: &mut Value,
    spec: &Cal2Spec,
    (k, item): (usize, &Item),
    built: &cal2_head::Built,
    (slot, foot): (&cal2_layout::Slot, Footprint),
) {
    let patch = &built.patch;
    j["id"] = json!(label(spec, k, item));
    j["number"] = json!(k + 1);
    j["row"] = json!(spec.rows[item.row].id);
    j["kind"] = json!(item.kind);
    j["neck_mm_designed"] = json!(item.neck_mm);
    j["variant"] = json!(item.variant);
    j["heads"] = json!(item.heads);
    j["designed_body"] = built.dims.clone();
    j["sized_to_section_mm2"] = json!({
        "load_model_capacity_per_head": built.sections.minimum,
        "certified_patch_per_head": built.sections.certified_patch,
        "mesh_face_patch_per_head": built.sections.mesh_patch,
        "mesh_minimum_per_head": built.sections.mesh_minimum,
        "cone_exit_per_head": built.sections.cone_exit,
        "full_area_mm2": built.full_area,
    });
    j["copy"] = json!(item.copy + 1);
    j["zone"] = json!(["front", "middle", "back"][slot.zone]);
    j["centre_mm"] = json!(slot.centre);
    j["footprint_mm"] = json!([foot.width, foot.height]);
    j["head_path"] = json!(match item.kind {
        Kind::Column => "plain waisted column (plinth's head cannot make 1.2 mm)",
        _ => "plinth head: head_candidate + PlanBuilder + freeze + emit",
    });
    j["plinth_certified_patch_mm2"] = json!({
        "lower_min": patch.iter().map(|p| p[0]).fold(f64::INFINITY, f64::min),
        "upper_max": patch.iter().map(|p| p[1]).fold(f64::NEG_INFINITY, f64::max),
    });
}

/// The coupon number raised on the top face, checked on the re-read model: the written mesh
/// must carry exactly the raised boxes of the designed number (and tab). Only a formation
/// witness, whose top face is too small for the smallest legible number, may carry none.
fn number_label(m: &Model, item: &Item, built: &cal2_head::Built) -> Result<Value> {
    let Some(f) = &built.features else {
        if item.kind != Kind::Witness {
            return Err(format!("{}: no number on the top face", m.name).into());
        }
        return Ok(json!({
            "raised_number": null,
            "reason": "the top face is smaller than the smallest legible number (7 x 5 pixels of 0.3 mm plus a 0.2 mm border, 2.5 x 1.9 mm); read it by its place on the plate map",
        }));
    };
    let boxes = cal2_body::raised_boxes(&m.mesh);
    if boxes != f.boxes() {
        return Err(format!(
            "{}: {boxes} raised boxes in the written file, {} designed for the number",
            m.name,
            f.boxes()
        )
        .into());
    }
    let (w, h) = {
        let b = cal2_body::number_box(f.number, f.pixel);
        (b[0], b[1])
    };
    Ok(json!({
        "raised_number": f.number,
        "on": if built.dims["shape"] == "ring" { "top surface of the ring, at the front" } else { "top face" },
        "pixel_mm": f.pixel,
        "digit_height_mm": h,
        "number_width_mm": w,
        "raised_mm": cal2_body::RAISE,
        "orientation_tab": f.tab.is_some(),
        "raised_boxes_in_written_file": boxes,
    }))
}

/// Build, lay out, write, re-read, measure and report into the empty `out` directory.
///
/// # Errors
/// Anything plinth refuses, a ratio off target, a clearance failure, or a non-empty `out`.
pub fn assemble(spec_path: &Path, out: &Path) -> Result<()> {
    let spec: Cal2Spec = serde_json::from_slice(&std::fs::read(spec_path)?)?;
    spec.validate()?;
    std::fs::create_dir_all(out)?;
    if std::fs::read_dir(out)?.next().is_some() {
        return Err("output directory must be empty; refusing overwrite".into());
    }
    let items = spec.items();
    let built = items
        .iter()
        .enumerate()
        .map(|(k, i)| cal2_head::build(&spec, i, k as u32 + 1))
        .collect::<Result<Vec<_>>>()?;
    let foot: Vec<Footprint> = built
        .iter()
        .map(|b| cal2_layout::footprint(&b.model))
        .collect();
    let plan = cal2_layout::plan(&items, &foot, spec.shuffle_seed)?;
    let grid = Grid::cal1();
    let models: Vec<Model> = plan
        .slots
        .iter()
        .map(|s| {
            let mut m = built[s.item].model.clone();
            cal2_layout::move_to(&mut m, s.centre, grid);
            m
        })
        .collect();
    let written = write(&out.join(PROJECT), models)?;
    if !written.identical {
        return Err("written project does not round-trip byte for byte".into());
    }
    let mut coupons = Vec::new();
    let mut map = Vec::new();
    for (slot, m) in plan.slots.iter().zip(&written.project.models) {
        let k = number(m)? as usize - 1;
        if k != slot.item {
            return Err("written model order differs from the layout".into());
        }
        let item = &items[k];
        let mut j = cal2_measure::coupon(&spec, item, m, grid, built[k].load.as_ref())?;
        annotate(&mut j, &spec, (k, item), &built[k], (slot, foot[k]));
        j["number_label"] = number_label(m, item, &built[k])?;
        map.push(map_coupon(
            &spec,
            item,
            k as u32 + 1,
            m,
            cal2_measure::sites(m, item.kind),
        ));
        coupons.push(j);
    }
    let cut: Vec<Value> = plan
        .cut
        .iter()
        .map(|&k| json!(label(&spec, k, &items[k])))
        .collect();
    let cut_text = if cut.is_empty() {
        "Nothing cut: every coupon in the matrix is on the plate.".to_string()
    } else {
        let mut ids: Vec<String> = plan
            .cut
            .iter()
            .map(|&k| format!("C{:02} ({})", k + 1, spec.rows[items[k].row].id))
            .collect();
        ids.sort();
        format!(
            "Cut to fit ({}), lowest-priority rows first: {}. Not on the plate.",
            cut.len(),
            ids.join(", ")
        )
    };
    std::fs::write(
        out.join("plate-map.svg"),
        cal2_map::svg(&map, &cut_text, spec.shuffle_seed)?,
    )?;
    // Tallest part, from the written file: top of the highest triangle above the plate.
    let tallest = written
        .project
        .models
        .iter()
        .map(|m| plate::bounds(m).hi[2])
        .fold(0.0, f64::max);
    let layers = (tallest / LAYER - 1e-6).ceil();
    std::fs::write(
        out.join("cal2-report.json"),
        serde_json::to_vec_pretty(&json!({
            "label": crate::LABEL,
            "project": PROJECT,
            "project_sha256": written.hash,
            "round_trip_byte_identical": written.identical,
            "shuffle_seed": spec.shuffle_seed,
            "build_area_mm": plate::BUILD,
            "minimum_bbox_gap_mm": written.gap,
            "edge_margin_required_mm": plate::EDGE_MARGIN,
            "minimum_edge_margin_mm": written.margin,
            "edge_margin_note": "least distance from any part's footprint, raft and supports included, to an edge of the build area, measured on the re-read file",
            "coupons_without_number": coupons
                .iter()
                .filter(|j| j["number_label"]["raised_number"].is_null())
                .map(|j| j["id"].clone())
                .collect::<Vec<_>>(),
            "coupon_count": coupons.len(),
            "cut_for_fit": cut,
            "tallest_part_mm": tallest,
            "estimated_layers": layers,
            "ratio_tolerance": spec.ratio_tolerance,
            "cells": cells(&coupons),
            "coupons": coupons,
        }))?,
    )?;
    println!(
        "CAL-2: {} coupons, {} cut, minimum bounding-box gap {:.3} mm, minimum edge margin {:.3} mm, tallest {:.2} mm ({} layers), sha256 {}",
        written.project.models.len(),
        plan.cut.len(),
        written.gap,
        written.margin,
        tallest,
        layers,
        written.hash
    );
    Ok(())
}

/// One line per cell (row, neck, ratio): copies on the plate, both achieved ratios and the
/// designed body, so the report reads without the per-coupon detail.
fn cells(coupons: &[Value]) -> Vec<Value> {
    let mut keys: Vec<(String, String, String)> = Vec::new();
    for j in coupons {
        let key = (
            j["row"].to_string(),
            j["neck_mm_designed"].to_string(),
            j["target_ratio"].to_string(),
        );
        if !keys.contains(&key) {
            keys.push(key);
        }
    }
    keys.iter()
        .map(|key| {
            let group: Vec<&Value> = coupons
                .iter()
                .filter(|j| {
                    (
                        j["row"].to_string(),
                        j["neck_mm_designed"].to_string(),
                        j["target_ratio"].to_string(),
                    ) == *key
                })
                .collect();
            let f = |p: &str| {
                group
                    .iter()
                    .filter_map(|j| j[p].as_f64())
                    .collect::<Vec<_>>()
            };
            let (model, mesh, cone) = (
                f("achieved_ratio_model"),
                f("achieved_ratio_mesh_section"),
                f("achieved_ratio_cone_exit"),
            );
            let range = |a: &[f64]| {
                json!([
                    a.iter().copied().fold(f64::INFINITY, f64::min),
                    a.iter().copied().fold(f64::NEG_INFINITY, f64::max)
                ])
            };
            json!({
                "row": group[0]["row"],
                "variant": group[0]["variant"],
                "neck_mm": group[0]["neck_mm_designed"],
                "heads": group[0]["heads"],
                "target_ratio": group[0]["target_ratio"],
                "copies_on_plate": group.len(),
                "achieved_ratio_model": range(&model),
                "achieved_ratio_mesh_section": range(&mesh),
                "achieved_ratio_cone_exit": range(&cone),
                "designed_body": group[0]["designed_body"],
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const SPEC: &str = include_str!("../../../fixtures/calibration/cal2.json");

    fn run(tag: &str, text: &str) -> (Value, std::path::PathBuf) {
        let dir = std::env::temp_dir().join(format!("cal2-test-{}-{tag}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let spec = dir.join("spec.json");
        std::fs::write(&spec, text).unwrap();
        let out = dir.join("out");
        assemble(&spec, &out).unwrap();
        let report =
            serde_json::from_slice(&std::fs::read(out.join("cal2-report.json")).unwrap()).unwrap();
        (report, dir)
    }

    #[test]
    fn layout_of_the_real_matrix_is_reproducible_and_seed_dependent() {
        let spec: Cal2Spec = serde_json::from_str(SPEC).unwrap();
        let items = spec.items();
        let foot: Vec<_> = items
            .iter()
            .enumerate()
            .map(|(k, i)| {
                cal2_layout::footprint(
                    &cal2_head::build_for_layout(&spec, i, k as u32 + 1)
                        .unwrap()
                        .model,
                )
            })
            .collect();
        let a = cal2_layout::plan(&items, &foot, spec.shuffle_seed).unwrap();
        let b = cal2_layout::plan(&items, &foot, spec.shuffle_seed).unwrap();
        let c = cal2_layout::plan(&items, &foot, spec.shuffle_seed + 1).unwrap();
        assert_eq!(a.slots, b.slots);
        assert_ne!(a.slots, c.slots);
    }

    #[test]
    // An `e2e_` name: the whole matrix (67 coupons built, written, re-read, load-modelled)
    // takes longer than the 6 s unit cap in .config/nextest.toml.
    fn e2e_cal2_whole_plate_is_complete_in_tolerance_clear_and_round_trips() {
        let (a, dir_a) = run("a", SPEC);
        assert_eq!(a["round_trip_byte_identical"], true);
        assert!(a["minimum_bbox_gap_mm"].as_f64().unwrap() >= plate::GAP - 1e-9);
        // Every footprint, raft included, is 8 mm or more from each plate edge.
        assert!(a["minimum_edge_margin_mm"].as_f64().unwrap() >= 8.0);
        let coupons = a["coupons"].as_array().unwrap();
        // Every coupon carries its raised number in the written file, except a formation
        // witness, whose face is too small for the smallest legible digits.
        for j in coupons {
            let l = &j["number_label"];
            if j["kind"] == "witness" {
                assert!(l["raised_number"].is_null(), "{}", j["id"]);
                continue;
            }
            assert_eq!(l["raised_number"], j["number"], "{}", j["id"]);
            assert!(l["pixel_mm"].as_f64().unwrap() >= 0.3 - 1e-9);
            assert!(l["raised_boxes_in_written_file"].as_u64().unwrap() >= 7);
            if j["variant"] == "ring" {
                assert_eq!(l["on"], "top surface of the ring, at the front");
            }
        }
        assert!(
            a["coupons_without_number"]
                .as_array()
                .unwrap()
                .iter()
                .all(|id| id.as_str().unwrap().contains("ratio 6"))
        );
        assert_eq!(
            coupons.len() + a["cut_for_fit"].as_array().unwrap().len(),
            67
        );
        for j in coupons {
            // Never over the target; at most the 3 percent tolerance under it.
            let e = j["achieved_ratio_error_percent"].as_f64().unwrap();
            assert!((-3.0..=0.01).contains(&e), "{e}");
            assert_eq!(j["ratio_by_layer"]["no_layer_over_target"], true);
            // Nor is any contact's own ratio in any layer (late load included).
            let target = j["target_ratio"].as_f64().unwrap();
            assert!(
                j["ratio_by_layer"]["model_largest_contact_per_body_layer"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .all(|x| x.as_f64().unwrap() <= target * (1.0 + 1e-4)),
                "{}",
                j["id"]
            );
            assert_eq!(j["columns_and_flares_strictly_larger_than_neck"], true);
            // The physical ratios are below the model's: the certified patch is smaller
            // than the written ball's mesh section, which is smaller than the cone exit.
            if j["kind"] == "head" {
                assert!(j["achieved_ratio_mesh_section"].as_f64() < j["achieved_ratio"].as_f64());
                assert!(j["achieved_ratio_cone_exit"].as_f64() < j["achieved_ratio"].as_f64());
            }
        }
        // Copies of one cell never share a front-to-back zone.
        let mut seen = std::collections::BTreeSet::new();
        for j in coupons.iter().filter(|j| j["row"] != "R9") {
            let key = (
                j["row"].to_string(),
                j["neck_mm_designed"].to_string(),
                j["target_ratio"].to_string(),
                j["zone"].to_string(),
            );
            assert!(
                seen.insert(key),
                "two copies of a cell in one zone: {}",
                j["id"]
            );
        }
        // Rows of priority 1 to 5 keep two copies of every cell, whatever was cut.
        for row in ["R1", "R2"] {
            for ratio in crate::cal2_spec::RATIOS {
                let n = coupons
                    .iter()
                    .filter(|j| j["row"] == row && j["target_ratio"] == ratio)
                    .count();
                assert!(n >= 2, "{row} R{ratio}: {n} copies");
            }
        }
        for row in ["R3a", "R3b", "R4", "R5a", "R5b"] {
            for ratio in [40.0, 50.0] {
                let n = coupons
                    .iter()
                    .filter(|j| j["row"] == row && j["target_ratio"] == ratio)
                    .count();
                assert!(n >= 2, "{row} R{ratio}: {n} copies");
            }
        }
        // The tallest part is the long hold: 5 mm lift + 5 mm hold + raised features + raft.
        assert!(a["tallest_part_mm"].as_f64().unwrap() > 10.0);
        let _ = std::fs::remove_dir_all(dir_a);
    }

    #[test]
    fn refuses_a_changed_matrix_and_a_non_empty_output_directory() {
        let dir = std::env::temp_dir().join(format!("cal2-test-{}-refuse", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("out")).unwrap();
        let spec = dir.join("spec.json");
        std::fs::write(&spec, SPEC.replace("\"copies\": 1", "\"copies\": 2")).unwrap();
        assert!(assemble(&spec, &dir.join("out")).is_err());
        std::fs::write(&spec, SPEC).unwrap();
        std::fs::write(dir.join("out").join("keep"), b"x").unwrap();
        assert!(assemble(&spec, &dir.join("out")).is_err());
        let _ = std::fs::remove_dir_all(dir);
    }
}
