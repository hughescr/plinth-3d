//! Re-read the emitted plate, then measure and report it. No physical-print claim.
use crate::{
    Result, build,
    goo_grid::{self, Boundary, Grid},
    plate::{self, IDS},
    section::{self, SectionMesh},
    spec::PlateSpec,
};
use plinth_chitubox::{Model, Project};
use serde_json::{Value, json};
use std::{fmt::Write as _, path::Path};

pub const PROJECT: &str = "CAL1-plate-calibration-not-print-candidate.chitubox";
fn read(path: &Path) -> Result<Project> {
    Ok(plinth_chitubox::parse(&std::fs::read(path)?)?)
}
fn save(path: &Path, p: &Project) -> Result<Project> {
    std::fs::write(path, p.to_bytes()?)?;
    read(path)
}
fn json_file(dir: &Path, name: &str, value: &Value) -> Result<()> {
    std::fs::write(dir.join(name), serde_json::to_vec_pretty(value)?)?;
    Ok(())
}
fn area(layer: &[crate::goo::Span], grid: Grid) -> f64 {
    layer.iter().map(|s| u64::from(s.x1 - s.x0)).sum::<u64>() as f64 * grid.pitch[0] * grid.pitch[1]
}
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
/// Each native model can move only by its XY translation. Compare serialized bytes,
/// not approximate float equality, after undoing that move and the display name.
fn unchanged(source: &Model, placed: &Model) -> Result<bool> {
    let mut restored = placed.clone();
    restored.name = source.name.clone();
    restored.translate[..2].copy_from_slice(&source.translate[..2]);
    let one = |m: Model| {
        Project {
            models: vec![m],
            ..Project::default()
        }
        .to_bytes()
    };
    Ok(one(source.clone())? == one(restored)?)
}
fn prediction(
    m: &Model,
    id: &str,
    measured: &Value,
    grid: Grid,
    original: Option<&Model>,
) -> Result<Value> {
    let mut contacts = Vec::new();
    let offset = goo_grid::offset(m);
    let mut indices_lo = Vec::new();
    let mut indices_hi = Vec::new();
    if matches!(id, "B-100" | "S-100" | "S-324") {
        let records = &m.supports.as_ref().ok_or("missing native heads")?.records;
        let old: Vec<_> = measured["waists"]
            .as_array()
            .ok_or("missing waist measurements")?
            .iter()
            .filter(|r| r["piece"] == id)
            .collect();
        if old.len() != 4
            || old
                .iter()
                .enumerate()
                .any(|(i, r)| r["contact"].as_u64() != Some(i as u64))
        {
            return Err("waist measurements must contain exactly the four ordered contacts".into());
        }
        let body = if id == "B-100" { 18.254250 } else { 18.051026 };
        for (i, head) in records[..28].chunks_exact(7).enumerate() {
            let waist = &head[1];
            let mesh = SectionMesh::new(&waist.mesh, [0.0; 3], [0.0, 0.0, 1.0]);
            let sample = section::minimum(&[mesh], -3.12, -2.88, 240);
            let inflation_lo = old
                .iter()
                .map(|r| r["lit_range"][0].as_f64().unwrap() / r["mesh"].as_f64().unwrap())
                .fold(f64::INFINITY, f64::min);
            let inflation_hi = old
                .iter()
                .map(|r| r["lit_range"][1].as_f64().unwrap() / r["mesh"].as_f64().unwrap())
                .fold(0.0, f64::max);
            let sliced = [sample.area * inflation_lo, sample.area * inflation_hi];
            let index = [body / (4.0 * sliced[1]), body / (4.0 * sliced[0])];
            let old_model =
                original.ok_or("original coupon is required to calibrate grid prediction")?;
            let old_waist = &old_model
                .supports
                .as_ref()
                .ok_or("original native heads missing")?
                .records[i * 7 + 1];
            let old_layers = goo_grid::slice(
                &old_waist.mesh,
                goo_grid::offset(old_model),
                grid,
                Boundary::Touch,
            )?;
            let old_grid: Vec<_> = (87..92).map(|k| area(&old_layers[k], grid)).collect();
            let old_grid_min = old_grid.iter().copied().fold(f64::INFINITY, f64::min);
            let old_grid_max = old_grid.iter().copied().fold(0.0, f64::max);
            let native_to_grid = [
                old[i]["lit_range"][0].as_f64().unwrap() / old_grid_max,
                old[i]["lit_range"][1].as_f64().unwrap() / old_grid_min,
            ];
            let mut grid_prediction = Value::Null;
            let mut raster = Vec::new();
            for boundary in [Boundary::Centre, Boundary::Touch] {
                let layers = goo_grid::slice(&waist.mesh, offset, grid, boundary)?;
                let areas: Vec<_> = (87..92).map(|k| area(&layers[k], grid)).collect();
                if matches!(boundary, Boundary::Touch) {
                    let predicted_area = [
                        areas.iter().copied().fold(f64::INFINITY, f64::min) * native_to_grid[0],
                        areas.iter().copied().fold(0.0, f64::max) * native_to_grid[1],
                    ];
                    let grid_index = [
                        body / (4.0 * predicted_area[1]),
                        body / (4.0 * predicted_area[0]),
                    ];
                    indices_lo.push(index[0].min(grid_index[0]));
                    indices_hi.push(index[1].max(grid_index[1]));
                    grid_prediction = json!({"old_grid_touch_area_mm2_M":old_grid,"measured_native_to_grid_range_M":native_to_grid,"predicted_slice_area_mm2_U":predicted_area,"predicted_index_range_U":grid_index});
                }
                raster.push(json!({"boundary":format!("{boundary:?}"),"layer_labels_mm":[4.40,4.45,4.50,4.55,4.60],"waist_area_mm2":areas}));
            }
            contacts.push(json!({"contact":i,"record":i*7+1,"diameter_mm":waist.r1*2.0,"written_mesh_section_mm2_M":sample.area,"written_minimum_caliper_width_mm_M":crate::plate_sections::waist_width(&waist.mesh),"sample_step_mm":sample.step,"world_z_sample_mm":sample.at+offset[2],"grid_witness":raster,"grid_calibrated_prediction":grid_prediction,"measured_old_slice_inflation_range_M":[inflation_lo,inflation_hi],"predicted_slice_area_range_mm2_U":sliced,"predicted_as_sliced_index_range_U":index}));
        }
    }
    let range = if indices_lo.is_empty() {
        Value::Null
    } else {
        json!([
            indices_lo.into_iter().fold(f64::INFINITY, f64::min),
            indices_hi.into_iter().fold(0.0, f64::max)
        ])
    };
    Ok(
        json!({"piece":m.name,"id":id,"written_section_diagnostics":crate::plate_sections::measure(m,id),"status":"Unverified: full plinth load-model replay not run","reason":"The available foreign-support adapter assumes a tip-ball/cone/joint chain and equal body meshes. CAL-1 has extra waisted-head junctions and solid K/W load paths. Reusing it without adapting and testing those paths could silently omit the controlling section. The table below is quarter-share coupon arithmetic only, not a replay.","replayed_index":null,"replayed_layer":null,"replayed_contact":null,"compact_law_replay_call":"Unverified","sliced_file_sha256":"Unverified: owner has not sliced the assembled plate","contrast":match id {"A0"=>"C3; C4", "A1"=>"C4", "B-100"|"S-100"=>"C1", "S-324"=>"C2", "W"=>"formation gate for C1/C2", _=>"K replication control"},"coupon_prediction_method":"Old measured body lit-any area / (4 × written new mesh waist area × old measured slice/mesh inflation). Envelope over all five layers and four contacts, not a physical load measurement. Also calibrate old midpoint cell-touching grid areas to the old native lit-any waist areas, then apply those factors to the new written grid areas. Report the envelope of both prediction methods; neither is a measured assembled slice or certified bound.","predicted_as_sliced_index_range_U":range,"coupon_contacts":contacts}),
    )
}

fn prepare_sources(
    spec: &PlateSpec,
    input: &Path,
    measured_path: &Path,
    out: &Path,
) -> Result<(Vec<Model>, Vec<Value>)> {
    let individual = out.join("individual");
    std::fs::create_dir(&individual)?;
    let evidence = out.join("input-measurements");
    std::fs::create_dir(&evidence)?;
    std::fs::copy(measured_path, evidence.join("summary.json"))?;
    for name in ["method.json", "a1b.json"] {
        std::fs::copy(
            measured_path
                .parent()
                .ok_or("measurement parent missing")?
                .join(name),
            evidence.join(name),
        )?;
    }
    let mut sources = Vec::new();
    let mut source_info = Vec::new();
    for id in IDS {
        let original = if matches!(id, "A0" | "A1" | "K-15" | "K-60") {
            let source = if id == "A1" { "A1b" } else { id };
            read(&input.join(format!("{source}-calibration-not-print-candidate.chitubox")))?
        } else {
            build::specimen(
                spec.specimens
                    .iter()
                    .find(|s| s.id == id)
                    .ok_or("missing specimen")?,
                &spec.head,
            )
        };
        if original.models.len() != 1 {
            return Err("input must have exactly one editable model".into());
        }
        let path = individual.join(format!("{id}-calibration-not-print-candidate.chitubox"));
        let written = save(&path, &original)?;
        source_info.push(json!({"id":id,"file":path.file_name().unwrap().to_string_lossy(),"sha256":sha(&path)?,"provenance":if matches!(id,"A0"|"A1"|"K-15"|"K-60") {"existing individual, retained unchanged (A1 is A1b)"} else {"retuned individual, reconstructed with fixed head and raft"}}));
        sources.push(written.models[0].clone());
    }
    Ok((sources, source_info))
}
pub fn assemble(spec_path: &Path, input: &Path, measured_path: &Path, out: &Path) -> Result<()> {
    let spec: PlateSpec = serde_json::from_slice(&std::fs::read(spec_path)?)?;
    spec.validate()?;
    if spec.shuffle_seed != plate::SEED {
        return Err("spec seed differs from assembly seed".into());
    }
    let measured: Value = serde_json::from_slice(&std::fs::read(measured_path)?)?;
    std::fs::create_dir_all(out)?;
    if std::fs::read_dir(out)?.next().is_some() {
        return Err("output directory must be empty; refusing overwrite".into());
    }
    let (sources, source_info) = prepare_sources(&spec, input, measured_path, out)?;
    let owner_goo = crate::goo::Goo::parse(&std::fs::read(input.join("a1b.goo"))?)?;
    let grid = Grid::from_header(&owner_goo.header);
    let project = Project {
        preview: Some(crate::blank_preview()),
        models: plate::place_on_grid(&sources, plate::SEED, grid)?,
        ..Project::default()
    };
    let path = out.join(PROJECT);
    let written = save(&path, &project)?;
    let hash = sha(&path)?;
    let gap = plate::validate(&written.models)?;
    let mut placements = Vec::new();
    let mut predictions = Vec::new();
    let mut world = Vec::new();
    for (i, m) in written.models.iter().enumerate() {
        let id = IDS
            .iter()
            .find(|id| {
                m.name
                    .starts_with(&format!("{}-{id};", if i < 8 { "front" } else { "back" }))
            })
            .ok_or("unknown piece")?;
        let index = IDS.iter().position(|x| x == id).unwrap();
        if !unchanged(&sources[index], m)? {
            return Err(format!("{id}: native source bytes changed").into());
        }
        if !plate::raster_phase_kept(&sources[index], m, grid) {
            return Err(format!("{id}: source raster phase changed").into());
        }
        placements.push(json!({"piece":m.name,"band":if i<8 {"front"} else {"back"},"id":id,"bounds_mm":plate::bounds(m),"translate_mm":m.translate,"source_native_bytes_preserved":true,"support_records":m.supports.as_ref().map_or(0,|s|s.records.len())}));
        let frame = goo_grid::offset(m);
        world.extend(
            plate::triangles(m)
                .into_iter()
                .map(|t| t.map(|p| std::array::from_fn(|k| (f64::from(p[k]) + frame[k]) as f32))),
        );
        let old_coupon = if matches!(*id, "B-100" | "S-100" | "S-324") {
            Some(read(&input.join(format!(
                "{id}-calibration-not-print-candidate.chitubox"
            )))?)
        } else {
            None
        };
        predictions.push(prediction(
            m,
            id,
            &measured,
            grid,
            old_coupon.as_ref().map(|p| &p.models[0]),
        )?);
    }
    let mut raster_checks = Vec::new();
    let mut profiles = Vec::new();
    for boundary in [Boundary::Centre, Boundary::Touch] {
        let layers = goo_grid::slice(&world, [0.0; 3], grid, boundary)?;
        let islands = goo_grid::islands(&layers, grid);
        let total:Vec<_>=layers.iter().enumerate().map(|(k,l)|json!({"layer":k+1,"midpoint_z_mm":(k as f64+0.5)*grid.height,"total_cured_area_mm2":area(l,grid)})).collect();
        println!(
            "plate {boundary:?}: {} layers, {} islands",
            layers.len(),
            islands.len()
        );
        raster_checks.push(json!({"boundary":format!("{boundary:?}"),"layers_checked":layers.len(),"islands":islands,"island_count":islands.len()}));
        profiles.push(json!({"boundary":format!("{boundary:?}"),"layers":total}));
    }
    json_file(
        out,
        "area-profile.json",
        &json!({"project_sha256":hash,"status":"M: written-mesh midpoint-grid witness, not ChituBox cured pixels","layer_height_mm":grid.height,"pixel_pitch_mm":grid.pitch,"profiles":profiles}),
    )?;
    json_file(
        out,
        "predictions.json",
        &json!({"project_sha256":hash,"load_model_replay":"Unverified; see per-piece reasons","project_hash_frozen":true,"sliced_hash_frozen":false,"pieces":predictions}),
    )?;
    let check = json!({"label":crate::LABEL,"project_sha256":hash,"shuffle_seed":plate::SEED,"shuffle_algorithm":"xorshift64 + rejection sampling, Fisher-Yates over six units; random orientation of both pairs before shuffling; front then back from one stream","build_area_mm":plate::BUILD,"grid_pitch_mm_M":grid.pitch,"grid_height_mm_M":grid.height,"placement_construction":"Translate only by whole owner-grid pixel steps, preserving source raster phase; reserve 2 X pixels beyond the 6 mm minimum before rounding. Arbitrary translation is not continuity-preserving.","area_profile_file":"area-profile.json","measurement_summary_sha256":sha(measured_path)?,"owner_a1b_goo_sha256":sha(&input.join("a1b.goo"))?,"gap_requested_mm":plate::GAP,"minimum_conservative_bbox_gap_mm_M":gap,"piece_count":written.models.len(),"all_on_plate":written.models.iter().all(|m|plate::bounds(m).lo[2].abs()<0.00001),"placements":placements,"sources":source_info,"raster_checks":raster_checks,"predicted_rung_indices_U":predictions.iter().filter(|p|!p["coupon_contacts"].as_array().unwrap().is_empty()).map(|p|json!({"piece":p["piece"],"range":p["predicted_as_sliced_index_range_U"]})).collect::<Vec<_>>(),"H9":"New coupon minimum opposing-face waist widths = diameter × cos(pi/16); both >0.10 mm. R1 calibration-only test pieces; R3 applies to unchanged A0/A1b. No new production exception.","engraved_labels":"Skipped: adding native raft-tab cuts without a ChituBox round-trip is not established safe. Use plate-map.png. No physical scale bars added.","ChituBox_open_save_slice_H8":"Unverified: owner must open and slice this assembled project","resin_volume_and_print_time":"Unverified: awaiting ChituBox assembled slice","cross_family_review":"Unverified: no independent reviewer in this assembly step"});
    json_file(out, "plate-check.json", &check)?;
    std::fs::write(out.join("plate-map.svg"), map(&written.models)?)?;
    if raster_checks
        .iter()
        .any(|r| r["island_count"].as_u64() != Some(0))
    {
        return Err(
            "assembled mesh-grid witness has islands; see plate-check.json; do not print".into(),
        );
    }
    std::fs::write(
        out.join("README.txt"),
        format!(
            "CAL-1 calibration plate — NOT a production print candidate\n\nOpen {PROJECT} in ChituBox. There must be 16 separate pieces, eight near the front and eight near the back. Check the layout against plate-map.png and check the native supports are editable. The front edge releases first.\n\nSlice to cal1.goo in this same folder with the existing Anycubic WW ABS-Like Clear profile and 0.05 mm layers. Note ChituBox's resin volume and print time. This is a slice check, NOT a request to print yet. Tell us the resin lot number and tank temperature.\n\nA1 is the corrected A1b. Only neck records 1, 11, 13, 15, 17 and 19 qualify for the x2 comparison. Records 3, 5, 7 and 9 are observations only.\n\nThe 100 waists are now 0.229 mm and the 324 waist is 0.125 mm; W uses these same two sizes. Predictions are estimates from your earlier slices, NOT measured assembled-slice results. See predictions.json. The full plinth load-model replay is Unverified.\n\nNo engraved IDs or physical scale bars were added; use the picture as the key. The 10 mm bar in the picture is a drawing scale, not a printed feature. The mesh-grid check is not ChituBox and does not prove a physical print. We still need the assembled slice measurements, final sliced-file hash, load-model replay and independent review before asking you to print.\n\nSeed: {}. Frozen project SHA-256: {hash}\n",
            plate::SEED
        ),
    )?;
    Ok(())
}
pub fn map(models: &[Model]) -> Result<String> {
    let mut s = String::from(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"2400\" height=\"1660\" viewBox=\"0 0 2400 1660\"><rect width=\"2400\" height=\"1660\" fill=\"white\"/><g font-family=\"Arial, sans-serif\" fill=\"#171717\"><text x=\"120\" y=\"90\" font-size=\"48\" font-weight=\"bold\">CAL-1 • calibration only — not a production print candidate</text><text x=\"120\" y=\"142\" font-size=\"29\">Top view • all 16 pieces • supports preserved • at least 6 mm between pieces • shuffle seed 20261102</text>",
    );
    let x = |v: f64| 1200.0 + v * 10.0;
    let y = |v: f64| 830.0 - v * 10.0;
    writeln!(
        s,
        "<rect x=\"{}\" y=\"{}\" width=\"2116.8\" height=\"1183.7\" fill=\"#fafafa\" stroke=\"#777\" stroke-width=\"2\"/>",
        x(-105.84),
        y(59.185)
    )?;
    for (i, m) in models.iter().enumerate() {
        let b = plate::bounds(m);
        let c = b.centre();
        let offset = goo_grid::offset(m);
        let name = m.name.split(';').next().unwrap();
        let id = name
            .strip_prefix("front-")
            .or_else(|| name.strip_prefix("back-"))
            .unwrap_or(name);
        // Orthographic silhouettes from the actual written body and support triangles.
        let mut path = String::new();
        for mut t in plate::triangles(m) {
            let cross = (t[1][0] - t[0][0]) * (t[2][1] - t[0][1])
                - (t[1][1] - t[0][1]) * (t[2][0] - t[0][0]);
            if cross.abs() < 1e-12 {
                continue;
            }
            if cross < 0.0 {
                t.swap(1, 2);
            }
            writeln!(
                path,
                "M{:.2},{:.2}L{:.2},{:.2}L{:.2},{:.2}Z",
                x(f64::from(t[0][0]) + offset[0]),
                y(f64::from(t[0][1]) + offset[1]),
                x(f64::from(t[1][0]) + offset[0]),
                y(f64::from(t[1][1]) + offset[1]),
                x(f64::from(t[2][0]) + offset[0]),
                y(f64::from(t[2][1]) + offset[1])
            )?;
        }
        writeln!(
            s,
            "<path d=\"{path}\" fill=\"#505050\" fill-rule=\"nonzero\"/>"
        )?;
        let label_y = if i < 8 {
            y(b.lo[1]) + 42.0
        } else {
            y(b.hi[1]) - 25.0
        };
        writeln!(
            s,
            "<text x=\"{}\" y=\"{label_y}\" text-anchor=\"middle\" font-size=\"26\" font-weight=\"bold\">{id}</text>",
            x(c[0])
        )?;
    }
    s.push_str("<text x=\"1200\" y=\"200\" text-anchor=\"middle\" font-size=\"30\" font-weight=\"bold\">BACK EDGE</text><text x=\"1200\" y=\"1470\" text-anchor=\"middle\" font-size=\"36\" font-weight=\"bold\">FRONT EDGE — releases first (−Y in ChituBox)</text><path d=\"M1200 1410 L1200 1440 M1187 1427 L1200 1440 L1213 1427\" fill=\"none\" stroke=\"#171717\" stroke-width=\"4\"/><path d=\"M160 1515v15h100v-15\" fill=\"none\" stroke=\"#171717\" stroke-width=\"4\"/><text x=\"280\" y=\"1535\" font-size=\"25\">10 mm drawing scale — not printed</text><text x=\"120\" y=\"1605\" font-size=\"27\">A0-f2 / A0-b2 replace A1 under the plan rule. Original A0 names unchanged. No engraved labels; do not rearrange.</text></g></svg>");
    Ok(s)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn byte_preservation_guard_detects_a_changed_radius() {
        let spec: PlateSpec =
            serde_json::from_str(include_str!("../../../fixtures/calibration/cal1.json")).unwrap();
        let source = build::specimen(&spec.specimens[0], &spec.head)
            .models
            .remove(0);
        let mut moved = source.clone();
        moved.translate[0] += 10.0;
        assert!(unchanged(&source, &moved).unwrap());
        moved.supports.as_mut().unwrap().records[1].r1 += 0.001;
        assert!(!unchanged(&source, &moved).unwrap());
        let mut lifted = source.clone();
        lifted.translate[2] += 0.1;
        assert!(!unchanged(&source, &lifted).unwrap());
    }
}
