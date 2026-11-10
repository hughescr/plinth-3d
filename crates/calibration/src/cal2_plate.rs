//! CAL-2 assembly: build every coupon with plinth's head path, lay them out, write the
//! ChituBox project, re-read it, then measure and report from the re-read file only.
use crate::{
    Result, cal2_head,
    cal2_layout::{self, Footprint},
    cal2_map::{self, MapCoupon},
    cal2_measure,
    cal2_spec::{Cal2Spec, Item, Kind},
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
    format!(
        "C{:02} {} {:.2} mm x{} R{:.0} copy {}",
        k + 1,
        spec.rows[i.row].id,
        i.neck_mm,
        i.grid * i.grid,
        i.ratio,
        i.copy + 1
    )
}

struct Written {
    project: Project,
    gap: f64,
    identical: bool,
    hash: String,
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
    Ok(Written {
        project: read,
        gap,
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
    MapCoupon {
        number: n,
        row: spec.rows[item.row].id.clone(),
        kind: item.kind,
        neck_mm: item.neck_mm,
        heads: item.grid * item.grid,
        ratio: item.ratio,
        foot: [[b.lo[0], b.lo[1]], [b.hi[0], b.hi[1]]],
        body: [lo, hi],
        sites: sites.iter().map(|s| [s[0] + t[0], s[1] + t[1]]).collect(),
        control: item.row == 4,
    }
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
        let mut j = cal2_measure::coupon(&spec, item, m, grid)?;
        let patch = &built[k].patch;
        j["id"] = json!(label(&spec, k, item));
        j["number"] = json!(k + 1);
        j["row"] = json!(spec.rows[item.row].id);
        j["kind"] = json!(item.kind);
        j["neck_mm_designed"] = json!(item.neck_mm);
        j["heads"] = json!(item.grid * item.grid);
        j["copy"] = json!(item.copy + 1);
        j["zone"] = json!(["front", "middle", "back"][slot.zone]);
        j["centre_mm"] = json!(slot.centre);
        j["footprint_mm"] = json!([foot[k].width, foot[k].height]);
        j["head_path"] = json!(match item.kind {
            Kind::Column => "plain waisted column (plinth's head cannot make 1.2 mm)",
            _ => "plinth head: head_candidate + PlanBuilder + freeze + emit",
        });
        j["plinth_certified_patch_mm2"] = json!({
            "lower_min": patch.iter().map(|p| p[0]).fold(f64::INFINITY, f64::min),
            "upper_max": patch.iter().map(|p| p[1]).fold(f64::NEG_INFINITY, f64::max),
        });
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
        format!("Cut to fit ({}): lowest-priority rows first.", cut.len())
    };
    std::fs::write(
        out.join("plate-map.svg"),
        cal2_map::svg(&map, &cut_text, spec.shuffle_seed)?,
    )?;
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
            "coupon_count": coupons.len(),
            "cut_for_fit": cut,
            "ratio_tolerance": spec.ratio_tolerance,
            "coupons": coupons,
        }))?,
    )?;
    println!(
        "CAL-2: {} coupons, {} cut, minimum bounding-box gap {:.3} mm, sha256 {}",
        written.project.models.len(),
        plan.cut.len(),
        written.gap,
        written.hash
    );
    Ok(())
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
                cal2_layout::footprint(&cal2_head::build(&spec, i, k as u32 + 1).unwrap().model)
            })
            .collect();
        let a = cal2_layout::plan(&items, &foot, spec.shuffle_seed).unwrap();
        let b = cal2_layout::plan(&items, &foot, spec.shuffle_seed).unwrap();
        let c = cal2_layout::plan(&items, &foot, spec.shuffle_seed + 1).unwrap();
        assert_eq!(a.slots, b.slots);
        assert_ne!(a.slots, c.slots);
    }

    #[test]
    fn whole_plate_is_complete_in_tolerance_clear_and_round_trips() {
        let (a, dir_a) = run("a", SPEC);
        assert_eq!(a["round_trip_byte_identical"], true);
        assert!(a["minimum_bbox_gap_mm"].as_f64().unwrap() >= plate::GAP - 1e-9);
        let coupons = a["coupons"].as_array().unwrap();
        assert_eq!(
            coupons.len() + a["cut_for_fit"].as_array().unwrap().len(),
            53
        );
        for j in coupons {
            assert!(j["achieved_ratio_error_percent"].as_f64().unwrap().abs() <= 3.0);
            assert_eq!(j["ratio_by_layer"]["no_earlier_layer_higher"], true);
            assert_eq!(j["columns_and_flares_strictly_larger_than_neck"], true);
        }
        // Copies of one cell never share a front-to-back zone.
        let mut seen = std::collections::BTreeSet::new();
        for j in coupons.iter().filter(|j| j["row"] != "R6") {
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
        // Rows 1 to 3 keep at least two copies of every cell, whatever was cut.
        for row in ["R1", "R2", "R3"] {
            for ratio in [30.0, 40.0, 55.0, 70.0, 95.0] {
                let n = coupons
                    .iter()
                    .filter(|j| j["row"] == row && j["target_ratio"] == ratio)
                    .count();
                assert!(n >= 2, "{row} R{ratio}: {n} copies");
            }
        }
        let _ = std::fs::remove_dir_all(dir_a);
    }

    #[test]
    fn refuses_a_changed_matrix_and_a_non_empty_output_directory() {
        let dir = std::env::temp_dir().join(format!("cal2-test-{}-refuse", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("out")).unwrap();
        let spec = dir.join("spec.json");
        std::fs::write(&spec, SPEC.replace("\"copies\": 3", "\"copies\": 2")).unwrap();
        assert!(assemble(&spec, &dir.join("out")).is_err());
        std::fs::write(&spec, SPEC).unwrap();
        std::fs::write(dir.join("out").join("keep"), b"x").unwrap();
        assert!(assemble(&spec, &dir.join("out")).is_err());
        let _ = std::fs::remove_dir_all(dir);
    }
}
