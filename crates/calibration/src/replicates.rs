//! Apply R-C4 without re-laying out or rebuilding any other native record.
use crate::{Result, goo_grid, plate, plate_report};
use plinth_chitubox::{Model, Project};
use serde_json::{Value, json};
use std::{fmt::Write as _, path::Path};

fn bytes(m: &Model) -> Result<Vec<u8>> {
    Ok(Project {
        models: vec![m.clone()],
        ..Default::default()
    }
    .to_bytes()?)
}
fn hash(path: &Path) -> Result<String> {
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
        .ok_or("missing hash")?
        .into())
}
fn read_json(path: &Path) -> Result<Value> {
    Ok(serde_json::from_slice(&std::fs::read(path)?)?)
}
fn write_json(path: &Path, v: &Value) -> Result<()> {
    std::fs::write(path, serde_json::to_vec_pretty(v)?)?;
    Ok(())
}
/// Clone the same band's A0, changing only its name and the replaced slot's XY.
/// Original A0 names, Z, and every other model's serialized bytes stay unchanged.
pub fn replace(old: &Project, grid: goo_grid::Grid) -> Result<Project> {
    let mut new = old.clone();
    if old.models.len() != 16 {
        return Err("expected sixteen models".into());
    }
    for band in ["front", "back"] {
        let a0 = old
            .models
            .iter()
            .find(|m| m.name.starts_with(&format!("{band}-A0;")))
            .ok_or("missing A0")?;
        let slots: Vec<_> = old
            .models
            .iter()
            .enumerate()
            .filter(|(_, m)| m.name.starts_with(&format!("{band}-A1;")))
            .collect();
        let [(i, a1)] = slots.as_slice() else {
            return Err("expected one A1 per band".into());
        };
        let mut m = a0.clone();
        m.translate[..2].copy_from_slice(&a1.translate[..2]);
        m.name = format!(
            "{band}-A0-{}2; {}",
            if band == "front" { "f" } else { "b" },
            crate::LABEL
        );
        if !plate::raster_phase_kept(a0, &m, grid) {
            return Err("A0 replacement changes raster phase".into());
        }
        new.models[*i] = m;
    }
    plate::validate(&new.models)?;
    Ok(new)
}
/// Inputs must already be archived. Never writes into the superseded directory.
pub fn run(out: &Path) -> Result<()> {
    let archived = out.join("superseded-A1b");
    let old_path = archived.join(plate_report::PROJECT);
    let old = plinth_chitubox::parse(&std::fs::read(&old_path)?)?;
    let goo = crate::goo::Goo::parse(&std::fs::read(archived.join("cal1.goo"))?)?;
    let grid = goo_grid::Grid::from_header(&goo.header);
    let old_check = read_json(&archived.join("plate-check.json"))?;
    if old_check["shuffle_seed"] != plate::SEED || old_check["project_sha256"] != hash(&old_path)? {
        return Err("archived plate seed/hash mismatch".into());
    }
    let path = out.join(plate_report::PROJECT);
    if path.exists() || out.join("cal1.goo").exists() {
        return Err("archive project and GOO before writing".into());
    }
    let new = replace(&old, grid)?;
    std::fs::write(&path, new.to_bytes()?)?;
    let written = plinth_chitubox::parse(&std::fs::read(&path)?)?;
    let hash = hash(&path)?;
    let mut proof = Vec::new();
    let mut world = Vec::new();
    let mut check = old_check;
    for (i, (a, b)) in old.models.iter().zip(&written.models).enumerate() {
        let changed = a.name.contains("-A1;");
        let equal = bytes(a)? == bytes(b)?;
        if equal == changed {
            return Err(format!("unexpected model change at {i}").into());
        }
        if changed && a.translate.map(f32::to_bits) != b.translate.map(f32::to_bits) {
            return Err("replacement moved its slot".into());
        }
        proof.push(json!({"model":i,"old_name":a.name,"new_name":b.name,"whole_serialized_model_byte_identical":equal,"allowed_replacement":changed,"same_translation_bits":a.translate.map(f32::to_bits)==b.translate.map(f32::to_bits)}));
        if changed {
            check["placements"][i]["piece"] = json!(b.name);
            check["placements"][i]["id"] = json!("A0");
            check["placements"][i]["bounds_mm"] = serde_json::to_value(plate::bounds(b))?;
            check["placements"][i]["replacement_of"] = json!(a.name);
        }
        let frame = goo_grid::offset(b);
        world.extend(
            plate::triangles(b)
                .into_iter()
                .map(|t| t.map(|p| std::array::from_fn(|k| (f64::from(p[k]) + frame[k]) as f32))),
        );
    }
    let mut checks = Vec::new();
    let mut profiles = Vec::new();
    for boundary in [goo_grid::Boundary::Centre, goo_grid::Boundary::Touch] {
        let layers = goo_grid::slice(&world, [0.; 3], grid, boundary)?;
        let islands = goo_grid::islands(&layers, grid);
        println!(
            "replacement {boundary:?}: {} layers, {} islands",
            layers.len(),
            islands.len()
        );
        if !islands.is_empty() {
            return Err("replacement grid has islands; do not slice/print".into());
        }
        checks.push(json!({"boundary":format!("{boundary:?}"),"layers_checked":layers.len(),"island_count":0,"islands":islands}));
        profiles.push(json!({"boundary":format!("{boundary:?}"),"layers":layers.iter().enumerate().map(|(k,l)|json!({"layer":k+1,"midpoint_z_mm":(k as f64+0.5)*grid.height,"total_cured_area_mm2":l.iter().map(|s|u64::from(s.x1-s.x0)).sum::<u64>() as f64*grid.pitch[0]*grid.pitch[1]})).collect::<Vec<_>>()}));
    }
    check["project_sha256"] = json!(hash);
    check["ruling"] = json!(
        "R-C4 [S]: 3/10 robust necks; replace both A1 slots by second A0, no further thinning. C4 deferred."
    );
    check["replacement_proof"] = json!(proof);
    check["unchanged_whole_model_count"] = json!(14);
    check["raster_checks"] = json!(checks);
    check["minimum_conservative_bbox_gap_mm_M"] = json!(plate::validate(&written.models)?);
    check["cross_family_review"] = json!(
        "Replay: gpt-sol built, Claude Opus reviewed, PASS. Revised replacement/slice measurements not yet independently reviewed."
    );
    check["ChituBox_open_save_slice_H8"] =
        json!("Unverified for replacement plate; owner re-slice required");
    check["resin_volume_and_print_time"] = json!(
        "Prior A1b slice only: 5.858 mL [O], 5857.640625 mm3 / 3685 s [M]. Replacement slice pending."
    );
    write_json(&out.join("plate-check.json"), &check)?;
    write_json(
        &out.join("area-profile.json"),
        &json!({"project_sha256":hash,"status":"M: replacement written-mesh midpoint-grid witness, not ChituBox pixels","profiles":profiles}),
    )?;
    std::fs::write(
        out.join("plate-map.svg"),
        plate_report::map(&written.models)?,
    )?;
    write_predictions(out, &archived, &old, &written, &hash)?;
    println!(
        "new project SHA-256 {hash}; 14 whole model records byte-identical; 2 A0 replacements"
    );
    Ok(())
}

fn write_predictions(
    out: &Path,
    archived: &Path,
    old: &Project,
    written: &Project,
    hash: &str,
) -> Result<()> {
    let mut predictions = read_json(&archived.join("predictions.json"))?;
    let prior_rows = predictions["pieces"]
        .as_array()
        .ok_or("missing predictions")?
        .clone();
    for (i, m) in written.models.iter().enumerate() {
        if old.models[i].name.contains("-A1;") {
            let prefix = if i < 8 { "front-A0;" } else { "back-A0;" };
            predictions["pieces"][i] = prior_rows
                .iter()
                .find(|r| r["piece"].as_str().is_some_and(|s| s.starts_with(prefix)))
                .ok_or("missing A0 prediction")?
                .clone();
        }
        let row = &mut predictions["pieces"][i];
        row["piece"] = json!(m.name);
        row["sliced_file_sha256"] = json!("Unverified: replacement plate not yet sliced");
        row["contrast"] = json!(if row["id"] == "A0" {
            "C3 (A0 replicate); C4 deferred to plan §10"
        } else {
            row["contrast"].as_str().unwrap_or("")
        });
        if let Some(call) = row["compact_law_replay_call"].as_object_mut() {
            call.insert("conditional".into(), json!("Coupon load failures require W formation and a waist break. C4 deferred under R-C4."));
        }
    }
    predictions["project_sha256"] = json!(hash);
    predictions["sliced_hash_frozen"] = json!(false);
    predictions["load_model_replay"]["project_sha256_M"] = json!(hash);
    predictions["load_model_replay"]["sliced_file_sha256"] =
        json!("Unverified: replacement slice pending");
    predictions["review"] = json!(
        "Replay cross-family PASS: gpt-sol built, Claude Opus reviewed. Exact A0 results reused only after serialized geometry equality; new plate/slice review pending."
    );
    predictions["deferred_contrasts"] =
        json!(["C4: 3/10 robust necks under exposed-narrowest-section rule; both A1 slots now A0"]);
    // All four A0 replay results must match bit-for-bit as JSON, not just rounded maxima.
    let a0: Vec<_> = predictions["pieces"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|r| r["id"] == "A0")
        .collect();
    if a0.len() != 4 || a0.iter().any(|r| r["replay"] != a0[0]["replay"]) {
        return Err("A0 replicate replay differs".into());
    }
    write_json(&out.join("predictions.json"), &predictions)?;
    let mut md = format!(
        "# CAL-1 replacement plate predictions\n\nCalibration only. Not a print request. Project SHA-256 [M]: `{hash}`. Replacement GOO: **pending**. Prior slice evidence belongs to `superseded-A1b/`, not this project.\n\nR-C4 [S]: only 3/10 robust necks; both A1 slots become A0. C4 is deferred to plan §10. Original A0 names unchanged; front-A0-f2 and back-A0-b2 are the second A0 in each band (four total).\n\nReplay cross-family review: **gpt-sol built; Claude Opus reviewed; PASS**. Numbers below reuse the reviewed replay only after exact serialized geometry equality, excluding names and XY. This is not review of the new native slice. [U] load-model estimates are not measured forces; sections and layer/contact IDs are written-file diagnostics [M]. The JSON preserves all four models, sections, warnings and assumptions.\n\n| Piece | Contrast | Additive index [U] | Layer / contact [M] | Call |\n|---|---|---:|---|---|\n"
    );
    for row in predictions["pieces"].as_array().unwrap() {
        writeln!(
            md,
            "| {} | {} | {} | {} / {} | {} |",
            row["piece"].as_str().unwrap(),
            row["contrast"].as_str().unwrap(),
            row["replayed_index"],
            row["replayed_layer"],
            row["replayed_contact"],
            row["compact_law_replay_call"]["call"]
                .as_str()
                .unwrap_or("no clear call")
        )?;
    }
    md.push_str("\nAll four A0 rows have exactly identical replay JSON and additive 311.221 / rigid-carrier 2292.948 [U]. Equal-share and tributary maxima remain patch-model-sensitive first-exposure artefacts, not scored calls. Historical additive 312.672 versus current 311.221 reflects a 0.466% larger nominal patch denominator, not a changed law. K replication expects K-15 hold / K-60 fail; W is unscored. Coupon calls require W formation and a waist break. New slice measurements and both-hash stamping, final review and committing remain required before any print request.\n");
    std::fs::write(out.join("predictions.md"), md)?;
    std::fs::write(
        out.join("README.txt"),
        format!(
            "CAL-1 calibration plate — NOT a production print candidate\n\nPlease open {} in ChituBox, check all 16 pieces against plate-map.png, and slice to cal1.goo in this folder with the existing Anycubic WW ABS-Like Clear profile at 0.05 mm layers. Check all pieces rest on the plate and native supports remain editable. Do not rearrange the pieces. This is NOT a print request yet.\n\nPer the plan's preregistered rule, fewer than six qualifying necks means A1 becomes a second A0 in each band: four A0 copies total. Only three necks qualify under the exposed-narrowest-section rule. No further thinning was attempted. Original front-A0 / back-A0 names are unchanged; front-A0-f2 / back-A0-b2 replace the two A1 pieces at exactly the same translations and whole-pixel raster phase. C4 is deferred.\n\nThe original project and slice are in superseded-A1b/ and must not be used for this revised plate. Its 5.858 mL / 1 h 1 min 25 s are prior-slice measurements, not the new slice's values. Please note the new volume/time, resin lot and tank temperature.\n\nReplay WAS cross-family reviewed: gpt-sol built it, Claude Opus reviewed it, PASS. All four A0 copies have equal replay numbers. New native-slice checks and final review are still needed before asking to print. The midpoint-grid witness has zero islands but is not ChituBox or proof of a physical print. No engraved IDs were added; use the map.\n\nSeed: {}. Project SHA-256: {hash}\n",
            plate_report::PROJECT,
            plate::SEED
        ),
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn replacement_preserves_every_other_byte_and_rejects_bad_phase() {
        let spec: crate::spec::PlateSpec =
            serde_json::from_str(include_str!("../../../fixtures/calibration/cal1.json")).unwrap();
        let source = crate::build::specimen(&spec.specimens[0], &spec.head)
            .models
            .remove(0);
        let sources = vec![source; 8];
        let mut old = Project {
            models: plate::place(&sources, plate::SEED).unwrap(),
            ..Default::default()
        };
        let new = replace(&old, goo_grid::Grid::cal1()).unwrap();
        assert_eq!(
            old.models
                .iter()
                .zip(&new.models)
                .filter(|(a, b)| bytes(a).unwrap() == bytes(b).unwrap())
                .count(),
            14
        );
        let i = old
            .models
            .iter()
            .position(|m| m.name.contains("-A1;"))
            .unwrap();
        assert_eq!(
            old.models[i].translate.map(f32::to_bits),
            new.models[i].translate.map(f32::to_bits)
        );
        old.models[i].translate[0] += 0.007;
        assert!(replace(&old, goo_grid::Grid::cal1()).is_err());
    }
}
