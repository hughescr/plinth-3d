//! Read-only CAL-1 replay against the slot's unchanged support load API.
//! Every body is sliced once as a batch. Front/back reuse is allowed only after
//! serialized geometry equality is checked with placement/name removed.
use crate::{
    Result, goo_grid, replay_geom as g, replay_native,
    section::{self, SectionMesh},
};
use plinth_chitubox::{Model, Project, Triangle};
use plinth_geom::{Mesh, measure::Measure, slice::LayerGrid};
use plinth_support::load::{
    self, ContactView, LayoutView, PartChain, PartKind, PartSection, SiteClass, StackOptions,
};
use serde_json::{Value, json};
use std::{collections::BTreeMap, fmt::Write as _, path::Path};
const HASH: &str = "7d507a3dfa3128278d4717ce132de3128341d9e1aa044d2ca8b7df3d4df358cc";
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
        .ok_or("missing sha")?
        .into())
}
fn mesh(ts: &[Triangle], shift: f64) -> Result<Mesh> {
    let mut xs = Vec::new();
    let mut ys = Vec::new();
    let mut zs = Vec::new();
    for p in ts.iter().flatten() {
        xs.push(f64::from(p[0]));
        ys.push(f64::from(p[1]));
        zs.push(f64::from(p[2]) + shift);
    }
    Ok(Mesh::from_soup(&xs, &ys, &zs)?)
}
fn grid(mesh: &Mesh) -> Result<LayerGrid> {
    let mut grid = LayerGrid::covering(mesh, 0.05).ok_or("invalid grid")?;
    grid.origin = 0.0;
    grid.layers = (mesh.bounds().max[2] / grid.height).ceil() as u32;
    Ok(grid)
}
fn layers(ts: &[Triangle], shift: f64) -> Result<load::ModelLayers> {
    let mesh = mesh(ts, shift)?;
    let grid = grid(&mesh)?;
    match load::build_model_layers(
        &mesh,
        &grid,
        &StackOptions {
            threads: 8,
            ..Default::default()
        },
    ) {
        Measure::Known(v) => Ok(v),
        Measure::Unverified(why) => Err(format!("load stack Unverified: {why:?}").into()),
    }
}
fn value(x: f64) -> Value {
    if x.is_finite() {
        json!(x)
    } else {
        json!("infinite")
    }
}
type Mode = (&'static str, fn(&load::SectionDetail) -> (f64, u32));
/// Contacts that were in at least one group whose rigid-carrier equilibrium the load model
/// assessed. For every other contact the model stores `rigid_lever = 0.0` as a marker for
/// "not assessed" (a lone contact's group, for example, is never assessed); that zero is never
/// a ratio.
fn assessed_contacts(d: &load::LoadDetail) -> std::collections::BTreeSet<u32> {
    d.groups
        .iter()
        .filter(|g| g.equilibrium_assessed)
        .flat_map(|g| g.active.iter().copied())
        .collect()
}
/// The rigid-carrier entry when no section of the piece was assessed.
fn not_assessed() -> Value {
    json!({
        "assessed_U": false,
        "ratio_U": null,
        "reason_U": "no load group containing this path's contacts had an assessed rigid-carrier equilibrium; its stored 0.0 is a marker, not a ratio"
    })
}
/// A ratio as text: `not assessed` where the model did not assess it.
fn shown(v: &Value, precision: usize) -> String {
    v["ratio_U"]
        .as_f64()
        .map_or_else(|| "not assessed".into(), |x| format!("{x:.precision$}"))
}
fn effective(p: &PartSection, zt: f64) -> f64 {
    if p.kind != PartKind::Patch {
        return p.area_lower_mm2;
    }
    let [lo, hi] = p.z_range_mm;
    let t = if hi - lo <= 1e-9 {
        f64::from(u8::from(zt >= lo))
    } else {
        ((zt - lo) / (hi - lo)).clamp(0.0, 1.0)
    };
    let u = 1.0 - 2.0 * t;
    p.area_lower_mm2 * (u.acos() - u * (1.0 - u * u).max(0.0).sqrt()) / std::f64::consts::PI
}
fn maxima(d: &load::LoadDetail, layout: &LayoutView, offset: usize, grid: LayerGrid) -> Value {
    let modes: [Mode; 4] = [
        ("rigid_carrier", |s| s.rigid_lever),
        ("additive", |s| s.additive),
        ("equal_share", |s| s.equal_share),
        ("tributary", |s| s.tributary),
    ];
    let mut out = serde_json::Map::new();
    let assessed = assessed_contacts(d);
    for (name, f) in modes {
        let rigid = name == "rigid_carrier";
        let Some(s) = d
            .sections
            .iter()
            .filter(|s| !rigid || assessed.contains(&layout.chains[s.chain as usize].contact))
            .max_by(|a, b| {
                f(a).0
                    .total_cmp(&f(b).0)
                    .then_with(|| b.chain.cmp(&a.chain))
                    .then_with(|| b.part.cmp(&a.part))
            })
        else {
            if rigid {
                out.insert(name.into(), not_assessed());
            }
            continue;
        };
        let (ratio, layer) = f(s);
        let chain = &layout.chains[s.chain as usize];
        let mut p = &chain.parts[s.part as usize];
        if name == "additive" {
            p = chain
                .parts
                .iter()
                .min_by(|a, b| {
                    effective(a, grid.z_top(layer)).total_cmp(&effective(b, grid.z_top(layer)))
                })
                .unwrap();
        }
        let group = d
            .groups
            .iter()
            .find(|g| g.layer == layer && g.active.contains(&chain.contact));
        // Additive is a GROUP maximum, repeated at every section by the API.
        // Locate its actual limiting denominator, not the arbitrary repeated row.
        let limiting = if name == "additive" {
            group.map(|g|g.active.iter().map(|&ci| {
            let ch=&layout.chains[ci as usize];let sec=ch.parts.iter().min_by(|a,b|effective(a,grid.z_top(layer)).total_cmp(&effective(b,grid.z_top(layer)))).unwrap();
            json!({"contact_M":ci as usize+offset,"kind":format!("{:?}",sec.kind),"record_M":sec.record,"whole_section_mm2_M":sec.area_lower_mm2,"effective_section_mm2_U":effective(sec,grid.z_top(layer))})
        }).collect::<Vec<_>>()).unwrap_or_default()
        } else {
            vec![]
        };
        out.insert(name.into(),json!({"ratio_U":value(ratio),"layer_M":layer,"z_top_mm_M":grid.z_top(layer),"contact_M":chain.contact as usize+offset,"kind":format!("{:?}",p.kind),"record_M":p.record,"section_whole_area_mm2_M":p.area_lower_mm2,"effective_section_area_mm2_U":effective(p,grid.z_top(layer)),"section_z_range_mm_M":p.z_range_mm,"group_limiting_sections":limiting,"group_section_sum_mm2_U":group.map(|g|g.section_sum_mm2),"group_peel_area_mm2_M":group.map(|g|g.area_mm2),"group_rigid_residual_U":group.map(|g|g.lever_residual),"group_centroid_outside_hull_mm_U":group.map(|g|g.outside_hull_mm)}));
        if rigid {
            out[name]["assessed_U"] = json!(true);
        }
    }
    Value::Object(out)
}
fn analyse(ts: &[Triangle], shift: f64, layout: LayoutView, offset: usize) -> Result<Value> {
    let l = layers(ts, shift)?;
    let d = load::analyse_full(
        &l.stack,
        &layout,
        &load::Extras {
            structure: Some(&l.structure),
            needs: None,
        },
    );
    if !d.report.is_computed() || d.sections.is_empty() {
        return Err("load replay did not load any section".into());
    }
    let max = maxima(&d, &layout, offset, l.grid);
    let (ci, minimum) = layout
        .chains
        .iter()
        .flat_map(|ch| {
            ch.parts
                .iter()
                .map(move |p| (ch.contact as usize + offset, p))
        })
        .min_by(|a, b| a.1.area_lower_mm2.total_cmp(&b.1.area_lower_mm2))
        .ok_or("no load-path sections")?;
    let narrowest = json!({"contact_M":ci,"kind":format!("{:?}",minimum.kind),"record_M":minimum.record,"area_mm2_M":minimum.area_lower_mm2,"z_range_mm_M":minimum.z_range_mm,"scope":"Smallest sampled whole section; a partly exposed patch may have a smaller effective area U."});
    let assessed = assessed_contacts(&d);
    let sections:Vec<_>=d.sections.iter().map(|s|json!({"contact_M":layout.chains[s.chain as usize].contact as usize+offset,"kind":format!("{:?}",s.kind),"record_M":s.record,"tributary_U":[value(s.tributary.0),json!(s.tributary.1)],"rigid_carrier_U":if assessed.contains(&layout.chains[s.chain as usize].contact) {json!([value(s.rigid_lever.0),s.rigid_lever.1])} else {Value::Null},"additive_U":[value(s.additive.0),json!(s.additive.1)],"equal_share_U":[value(s.equal_share.0),json!(s.equal_share.1)]})).collect();
    let warnings:Vec<_>=d.report.warnings.iter().map(|w|json!({"kind":w.kind,"message":w.message,"layers_M":w.layers,"contacts_M":w.contacts.iter().map(|&c|c as usize+offset).collect::<Vec<_>>(),"message_contact_ids":"path-local; use contacts_M for piece-global IDs"})).collect();
    Ok(
        json!({"maxima":max,"narrowest_sampled_whole_section_M":narrowest,"sections":sections,"warnings":warnings,"grid_M":{"origin_mm":l.grid.origin,"height_mm":l.grid.height,"layers":l.grid.layers},"contact_count_M":layout.contacts.len()}),
    )
}
fn native(m: &Model) -> Result<Value> {
    let shift = goo_grid::offset(m)[2];
    let mesh = mesh(&m.mesh, shift)?;
    let grid = grid(&mesh)?;
    let (layout, diag) = replay_native::layout(m, grid.origin, grid.height)?;
    let mut result = analyse(&m.mesh, shift, layout, 0)?;
    result["geometry"] = diag;
    Ok(result)
}
fn solid_path(
    ts: &[Triangle],
    shift: f64,
    cut: f64,
    index: usize,
    kind: PartKind,
) -> Result<Value> {
    let upper = g::above(ts, cut);
    let s = SectionMesh::new(ts, [0.0; 3], [0., 0., 1.]);
    let minimum = section::minimum(&[s], 0.201 - shift, cut, 2048);
    if minimum.area <= 0.0 {
        return Err("solid load path has an empty sampled section".into());
    }
    let top = SectionMesh::new(ts, [0.; 3], [0., 0., 1.]);
    let patch = section::union_area(&[top], cut);
    let pts: Vec<_> = upper
        .iter()
        .flatten()
        .filter(|p| (f64::from(p[2]) - cut).abs() < 1e-5)
        .collect();
    let c: [f64; 3] = std::array::from_fn(|k| {
        pts.iter().map(|p| f64::from(p[k])).sum::<f64>() / pts.len() as f64
    });
    let mesh = mesh(&upper, shift)?;
    let grid = grid(&mesh)?;
    let contact = ContactView {
        centre: [c[0], c[1], cut + shift],
        patch_lower_mm2: patch,
        neck_area_mm2: minimum.area,
        first_layer: ((cut + shift - grid.origin) / grid.height).floor().max(0.) as u32,
        site_class: SiteClass::HiddenUnderside,
    };
    let layout = LayoutView {
        contacts: vec![contact],
        chains: vec![PartChain {
            contact: 0,
            parts: vec![
                PartSection {
                    kind: PartKind::Patch,
                    record: None,
                    area_lower_mm2: patch,
                    z_range_mm: [cut + shift; 2],
                },
                PartSection {
                    kind,
                    record: None,
                    area_lower_mm2: minimum.area,
                    z_range_mm: [0.2, cut + shift],
                },
            ],
        }],
        // The replay has no printed patch history: it states the cut patch itself, so the
        // load model uses its documented disc approximation (the pre-merge behaviour).
        patch_history: Vec::new(),
    };
    let mut result = analyse(&upper, shift, layout, index)?;
    result["geometry"] = json!({"path_M":index,"cut_plate_z_mm_M":cut+shift,"minimum_area_mm2_M":minimum.area,"minimum_plate_z_mm_M":minimum.at+shift,"sample_step_mm_M":minimum.step,"scope":"Loaded path from raft top through waist/arm to cut; upper ball crown is load, not a neck. Cap fan closes the written cut; no fields substituted."});
    Ok(result)
}
fn solid(m: &Model, id: &str) -> Result<Value> {
    let shift = goo_grid::offset(m)[2];
    let paths = if id == "W" {
        (0..6)
            .map(|i| {
                solid_path(
                    &crate::witness::column(&m.mesh, [i as f32 * 2.5 - 6.25, 0.]),
                    shift,
                    4.60 - shift,
                    i,
                    PartKind::Neck,
                )
            })
            .collect::<Result<Vec<_>>>()?
    } else {
        vec![solid_path(
            &m.mesh,
            shift,
            8.50 - shift,
            0,
            PartKind::Column,
        )?]
    };
    let mut max = paths[0]["maxima"].clone();
    for p in &paths[1..] {
        for key in ["rigid_carrier", "additive", "equal_share", "tributary"] {
            let better = match (assessed_ratio(&p["maxima"][key]), assessed_ratio(&max[key])) {
                (Some(a), Some(b)) => a > b,
                (Some(_), None) => true,
                (None, _) => false,
            };
            if better {
                max[key] = p["maxima"][key].clone();
            }
        }
    }
    Ok(
        json!({"maxima":max,"paths":paths,"note_U":if id=="W" {"Six independent ball-formation paths, no coupling between free-standing heads. The cut is in the exposed waist below each ball; all upper written material is sliced by the support load API. A Patch result denotes the artificial waist cut, not an adhesion patch."} else {"Cap plus flare above the arm is the load; the written arm/base below the cut is the carrier. This is a solid-path adapter, not a tip-contact claim. A Patch result denotes the arm cut, not an adhesion patch."}}),
    )
}
fn ratio(v: &Value) -> f64 {
    v["ratio_U"].as_f64().unwrap_or(f64::INFINITY)
}
/// The ratio, or `None` where the load model did not assess it.
fn assessed_ratio(v: &Value) -> Option<f64> {
    (v["assessed_U"] != json!(false)).then(|| ratio(v))
}
fn key(m: &Model) -> Result<Vec<u8>> {
    let mut m = m.clone();
    m.name.clear();
    m.translate[0] = 0.;
    m.translate[1] = 0.;
    Ok(Project {
        models: vec![m],
        ..Default::default()
    }
    .to_bytes()?)
}
fn call(id: &str, index: f64) -> Value {
    let call = if index >= 92.0 {
        "fail"
    } else if index <= 6.6 {
        "hold"
    } else {
        "no clear call"
    };
    if id == "W" {
        json!({"call":"no clear call","rule":"W is an unscored formation witness, not a compact P4 body."})
    } else if id.starts_with('K') {
        json!({"call":"no clear call","rule":"K is replication, not P4 scoring. Preregistered expectation from plate 6: K-15 holds; K-60 fails.","replication_expectation":if id=="K-15" {"hold"} else {"fail"}})
    } else {
        json!({"call":call,"rule":"P4 compact full-area law: additive group index >=92 fail; <=6.6 hold; otherwise no clear call. No alternative sharing scenario gets a clear call. Written-mesh model estimate; assembled-slice confirmation still required.","index_U":index,"conditional":"Coupon load failures require W formation and a waist break. C4 is scored only at frozen qualifying records 1,11,13,15,17,19; records 3,5,7,9 remain observations."})
    }
}
/// Keep one complete piece per line, for bounded output and jq-based inspection.
fn encode(output: &Value) -> Result<String> {
    let mut text = String::from("{\n");
    let fields = output.as_object().ok_or("predictions must be an object")?;
    for (i, (key, v)) in fields.iter().enumerate() {
        write!(text, "  {}: ", serde_json::to_string(key)?)?;
        if key == "pieces" {
            text.push_str("[\n");
            let pieces = v.as_array().ok_or("missing piece array")?;
            for (j, row) in pieces.iter().enumerate() {
                writeln!(
                    text,
                    "    {}{}",
                    serde_json::to_string(row)?,
                    if j + 1 == pieces.len() { "" } else { "," }
                )?;
            }
            text.push_str("  ]");
        } else {
            text.push_str(&serde_json::to_string(v)?);
        }
        writeln!(text, "{}", if i + 1 == fields.len() { "" } else { "," })?;
    }
    text.push_str("}\n");
    Ok(text)
}
fn validate_outputs(project: &Path, predictions: &Path, markdown: &Path) -> Result<()> {
    let parent = project
        .parent()
        .ok_or("project has no parent")?
        .canonicalize()?;
    for (path, name) in [
        (predictions, "predictions.json"),
        (markdown, "predictions.md"),
    ] {
        if path.file_name().and_then(|n| n.to_str()) != Some(name)
            || path
                .parent()
                .ok_or("output has no parent")?
                .canonicalize()?
                != parent
        {
            return Err(
                "outputs must be predictions.json and predictions.md beside the project".into(),
            );
        }
        if let Ok(meta) = std::fs::symlink_metadata(path)
            && (!meta.is_file() || meta.file_type().is_symlink())
        {
            return Err("prediction outputs must be ordinary files, not links".into());
        }
    }
    Ok(())
}
pub fn run(project: &Path, predictions: &Path, markdown: &Path) -> Result<()> {
    validate_outputs(project, predictions, markdown)?;
    if hash(project)? != HASH {
        return Err("project hash is not the frozen CAL-1 file".into());
    }
    let p = plinth_chitubox::parse(&std::fs::read(project)?)?;
    let mut output: Value = serde_json::from_slice(&std::fs::read(predictions)?)?;
    let rows = output["pieces"]
        .as_array_mut()
        .ok_or("missing prediction rows")?;
    if rows.len() != 16 || p.models.len() != 16 {
        return Err("CAL-1 requires exactly 16 pieces".into());
    }
    let mut cache: BTreeMap<String, (Vec<u8>, Value)> = BTreeMap::new();
    let mut md = String::from(
        "# CAL-1 load predictions\n\nCalibration only, not a print candidate. [M] means measured or derived from the frozen written file; [U] means a model estimate, not a physical strength or an assembled ChituBox slice measurement. Layers and Z are [M], zero-based from the plate; Z is the layer top. Contact and record IDs are [M], zero-based within each piece. Each model cell gives ratio [U]; section, record (r), contact (c), layer (L) and Z [M]. The narrowest section is a sampled whole mesh section, not the partly exposed patch capacity. For solid W/K, Patch means the waist/arm cut, not adhesion. A dagger marks an unscored first-exposure artefact sensitive to the patch model [U], not a robust strength prediction. The JSON's replayed_index/contact/layer fields use the additive model behind the P4 call, not tributary.\n\n| Piece | Contrast | Narrowest section mm² [M] | Rigid-carrier [U] | Additive [U] | Equal-share [U] | Tributary [U] | P4 call / replication expectation |\n|---|---|---:|---:|---:|---:|---:|---|\n",
    );
    for (i, (m, row)) in p.models.iter().zip(rows.iter_mut()).enumerate() {
        let id = row["id"].as_str().ok_or("missing piece id")?.to_owned();
        if row["piece"] != m.name
            || !crate::plate::IDS.contains(&id.as_str())
            || !m
                .name
                .split(';')
                .next()
                .unwrap_or("")
                .ends_with(&format!("-{id}"))
        {
            return Err(format!("model/row identity mismatch at {i}").into());
        }
        let bytes = key(m)?;
        let result = if let Some((previous, v)) = cache.get(&id) {
            if *previous != bytes {
                return Err(format!("front/back geometry differs: {id}").into());
            }
            v.clone()
        } else {
            eprintln!("replay {id}");
            let mut v = if m.supports.is_some() {
                native(m)?
            } else {
                solid(m, &id)?
            };
            if matches!(id.as_str(), "A0" | "A1") {
                for mode in ["equal_share", "tributary"] {
                    v["maxima"][mode]["interpretation_U"] = json!(
                        "First-exposure artefact sensitive to the patch model: the tilted-disc approximation differs from explicit clipped patch history. Unscored; not the additive index behind the P4 call."
                    );
                }
            }
            cache.insert(id.clone(), (bytes, v.clone()));
            v
        };
        let max = &result["maxima"];
        let additive = ratio(&max["additive"]);
        let prediction = call(&id, additive);
        write_piece(&mut md, row, &result, &prediction)?;
        row["replay"] = result;
        row["status"] = json!(
            "computed from written mesh [M]; load ratios are estimates [U]; assembled slice Unverified"
        );
        row["reason"] = json!(
            "unchanged support load API, calibration adapter; see replay geometry, maxima and warnings"
        );
        set_replayed_fields(row);
        row["compact_law_replay_call"] = prediction;
    }
    write_c4_necks(&mut md, &cache["A0"].1, &cache["A1"].1)?;
    let a0 = &cache["A0"].1["maxima"];
    writeln!(
        md,
        "\n## A0 sanity comparison\n\nEarlier approximate ratios were rigid-carrier 2,293 [U] and additive 313 [U]. Current replay: rigid-carrier {} [U], additive {} [U]. Rigid-carrier reproduces the earlier rounded value. Additive is 0.57% below the earlier rounded 313. A direct rerun of the existing historical binary on the identical A0 individual file returned rigid-carrier 2292.950919 [U] and additive 312.671859 [U], which rounds to 313. Both additive peaks have the same peel area, 37.678898974 mm² [M], at plate Z 5.85 mm [M]. Its denominator was 0.120506204 mm² [M, checker-conservative], versus the current nominal written-mesh 0.121068006 mm² [M]. That 0.466% larger denominator explains the current 0.464% lower additive estimate. The earlier adapter shrinks each convex half-space by the checker's face slack, canonical deviation and 0.000002 mm allowance; this adapter measures the nominal written face-plane intersection, without claiming a certified lower bound. The current unchanged load API uses the tilted-disc patch-exposure approximation [U], not explicit clipped patch history. Equal-share differs from historical 9669.53 [U] to current {} [U], and tributary from 42945.54 [U] to {} [U] (historical about 3.05× current): both peaks are first-exposure artefacts at contact 7, ball record 14 [M], where historical clipped patch area 0.000317 mm² [U] versus the API's tilted-disc 0.000966 mm² [U] explains the discrepancy; historical grid layer 341 maps to current layer 401 [M] through a 3.0 mm origin offset [M], and neither unscored maximum changes the additive call. The actual peak group has five active contacts [M], including the three lip patches and two smaller patches, not only the three lip patches described in the earlier summary. No preregistered call changes.\n",
        shown(&a0["rigid_carrier"], 3),
        shown(&a0["additive"], 3),
        shown(&a0["equal_share"], 2),
        shown(&a0["tributary"], 2)
    )?;
    md.push_str("## Interpretation and remaining checks\n\nFront and back copies have identical serialized geometry after removing only names and XY placement [M]; all replay numbers therefore match exactly. Additive maxima belong to a group, not one physical section: the JSON lists every group's limiting section. The table's contact is a representative row. At a partly printed patch the API reduces patch capacity using its disc approximation [U]. The global maxima for A0 and A1 remain at unchanged branch necks or unchanged patches, so equal headline maxima do not mean the edited main necks are equal. Coupon written-mesh indices (about 112 for the 100 rungs and 376 for S-324 [U]) are higher than the existing predicted native-slice indices because native slicing inflates the tiny waist areas. Both sets remain labelled estimates; only the assembled native slice can replace those predictions. Raw API warnings use production thresholds; they do not replace the preregistered clear-call rule below.\n\nC1 compares B-100 with S-100 per band; C2 tests S-324; C3 tests A0; C4 compares A0/A1 at qualifying edited necks only. Only the P4 additive compact-law rule (fail at >=92, hold at <=6.6) gets a clear call. The assumed strip-tolerance scenario makes no clear call. K expects K-15 hold and K-60 fail from plate 6, as replication rather than P4 scoring. W has no scored strength call. P5's design ratio remains 12 [U]; it is not the calibrated hold/fail bracket.\n\nUnverified: assembled native slicing, formation loads on the actual pixels, one-row boundary tolerance, GOO hash, open/save field preservation for A1b, print time/volume, physical outcomes and cross-family review. The prior same-family replay review found labelling and disclosure defects; this table applies those corrections without changing any model maximum or preregistered call. No physical strength, sharing law, or new design ratio is established. Mesh sections are sampled diagnostics, not certified continuous lower bounds. Support self-peel follows the unchanged API's section-area approximation; W and K upper load geometry is sliced explicitly. The API does not assess brace redistribution, neck bending or slenderness, and its warnings remain in the JSON.\n\n");
    writeln!(
        md,
        "Project SHA-256 [M]: `{HASH}`. Sliced-file SHA-256: Unverified. Nothing was written to the project or individual files."
    )?;
    output["load_model_replay"] = json!({"status":"computed","ratios":"U: model estimates from written geometry M, not measured physical loads or native GOO pixels","api":"plinth-support at slot revision; unchanged","design_ratio_U":load::DESIGN_RATIO,"front_back_identical_M":true,"sanity_comparison":{"historical_binary_sha256_M":"076c5a48250dacd2a92567b22321146ebc913de85366b817861c990fde7b4f8d","historical_binary_rerun_on_A0_U":{"rigid_carrier":2292.9509185520346,"additive":312.67185948571256,"equal_share":9669.53,"tributary":42945.54},"current_A0_U":a0,"first_exposure_comparison":{"explanation_U":"Equal-share 9669.53 historical versus 3171.32 current, and tributary 42945.54 versus 14084.87 (historical about 3.05x current): first-exposure artefacts sensitive to the patch model, not robust strength predictions. Explicit clipped history gives 0.000317 mm2 versus the unchanged API tilted-disc approximation's 0.000966 mm2; neither unscored maximum sets a preregistered call.","contact_M":7,"ball_record_M":14,"historical_layer_M":341,"current_layer_M":401,"grid_origin_offset_mm_M":3.0,"historical_clipped_patch_mm2_U":0.000317,"current_tilted_disc_patch_mm2_U":0.000966},"historical_additive_peak_M":{"area_mm2":37.67889897398439,"conservative_section_sum_mm2":0.12050620428700944,"plate_z_top_mm":5.85,"active_contacts":[0,1,2,3,4]},"explanation":"Current nominal half-space patch areas omit the old checker's shrink allowance, making the group denominator 0.466% larger and the additive ratio 0.464% smaller. Rigid reproduces to 0.00013%. Both preserve all preregistered calls."},"project_sha256_M":HASH,"sliced_file_sha256":"Unverified"});
    if hash(project)? != HASH {
        return Err("read-only project changed during replay".into());
    }
    std::fs::write(predictions, encode(&output)?)?;
    std::fs::write(markdown, md)?;
    Ok(())
}
fn set_replayed_fields(row: &mut Value) {
    let additive = row["replay"]["maxima"]["additive"].clone();
    row["replayed_model"] = json!("additive");
    row["replayed_index"] = additive["ratio_U"].clone();
    row["replayed_contact"] = additive["contact_M"].clone();
    row["replayed_layer"] = additive["layer_M"].clone();
}
fn narrowest(result: &Value) -> Result<&Value> {
    if result["narrowest_sampled_whole_section_M"].is_object() {
        return Ok(&result["narrowest_sampled_whole_section_M"]);
    }
    result["paths"]
        .as_array()
        .ok_or("missing solid paths")?
        .iter()
        .map(|p| &p["narrowest_sampled_whole_section_M"])
        .min_by(|a, b| {
            a["area_mm2_M"]
                .as_f64()
                .unwrap_or(f64::INFINITY)
                .total_cmp(&b["area_mm2_M"].as_f64().unwrap_or(f64::INFINITY))
        })
        .ok_or_else(|| "missing narrowest section".into())
}
fn write_piece(md: &mut String, row: &Value, result: &Value, prediction: &Value) -> Result<()> {
    let piece = row["piece"].as_str().unwrap().split(';').next().unwrap();
    let minimum = narrowest(result)?;
    write!(
        md,
        "| {piece} | {} | {:.7}; {} r{} c{} ",
        row["contrast"].as_str().ok_or("missing contrast")?,
        minimum["area_mm2_M"]
            .as_f64()
            .ok_or("missing section area")?,
        minimum["kind"].as_str().unwrap_or("?"),
        minimum["record_M"],
        minimum["contact_M"]
    )?;
    for name in ["rigid_carrier", "additive", "equal_share", "tributary"] {
        let r = &result["maxima"][name];
        if assessed_ratio(r).is_none() {
            write!(md, "| not assessed ")?;
            continue;
        }
        write!(
            md,
            "| {:.2}; {} r{} c{}, L{} / {:.2} mm ",
            ratio(r),
            r["kind"].as_str().unwrap_or("?"),
            r["record_M"],
            r["contact_M"],
            r["layer_M"],
            r["z_top_mm_M"].as_f64().unwrap()
        )?;
        if r["interpretation_U"].is_string() {
            write!(md, "† ")?;
        }
    }
    let expectation = prediction["replication_expectation"]
        .as_str()
        .map(|s| format!("; plate-6 replication: {s} [U]"))
        .unwrap_or_default();
    writeln!(
        md,
        "| {}{expectation} |",
        prediction["call"].as_str().unwrap()
    )?;
    Ok(())
}
fn write_c4_necks(md: &mut String, a0: &Value, a1: &Value) -> Result<()> {
    md.push_str("\n## C4 qualifying-neck diagnostics\n\nBoth bands share these values. These are axis-normal sampled mesh sections [M], NOT the horizontal sliced areas used to qualify C4. For example, record 1's A1/A0 equal-share index change is about ×3.88 [U], following the axis-normal area change (about ×3.88 [M]); its frozen horizontal sliced-area ratio is ×2.396 [M], not ×3.88. The frozen qualifying records are 1, 11, 13, 15, 17, 19 [M]; records 3, 5, 7, 9 [M] remain observations only (§5). Model cells give ratio [U] / peak layer [M]. Additive is the active group's maximum while that neck carries load, not an isolated per-neck prediction.\n\n| Piece | Neck record / contact [M] | Axis-normal area mm² [M] | Rigid-carrier [U] / L [M] | Additive [U] / L [M] | Equal-share [U] / L [M] | Tributary [U] / L [M] |\n|---|---|---:|---:|---:|---:|---:|\n");
    for (id, result) in [("A0", a0), ("A1", a1)] {
        let contacts = result["geometry"]["contacts_M"]
            .as_array()
            .ok_or("missing contacts")?;
        let sections = result["sections"].as_array().ok_or("missing sections")?;
        for record in [1, 11, 13, 15, 17, 19] {
            let c = contacts
                .iter()
                .find(|c| c["tip_record_M"] == record)
                .ok_or("missing qualifying contact")?;
            let s = sections
                .iter()
                .find(|s| s["record_M"] == record && s["kind"] == "Neck")
                .ok_or("missing qualifying neck")?;
            write!(
                md,
                "| {id} | {record} / {} | {:.7} ",
                c["contact_M"],
                c["neck_area_mm2_M"].as_f64().ok_or("missing neck area")?
            )?;
            for mode in [
                "rigid_carrier_U",
                "additive_U",
                "equal_share_U",
                "tributary_U",
            ] {
                if s[mode].is_null() {
                    write!(md, "| not assessed ")?;
                    continue;
                }
                write!(
                    md,
                    "| {:.2} / {} ",
                    s[mode][0].as_f64().ok_or("missing neck ratio")?,
                    s[mode][1]
                )?;
            }
            md.push_str("|\n");
        }
    }
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    fn coupon() -> Model {
        let spec: crate::spec::PlateSpec =
            serde_json::from_str(include_str!("../../../fixtures/calibration/cal1.json")).unwrap();
        let s = spec.specimens.iter().find(|s| s.id == "S-324").unwrap();
        let bytes = crate::build::specimen(s, &spec.head).to_bytes().unwrap();
        plinth_chitubox::parse(&bytes).unwrap().models.remove(0)
    }
    #[test]
    fn replay_waist_limits_all_models_and_missing_waist_changes_answer() {
        let m = coupon();
        let shift = goo_grid::offset(&m)[2];
        let l = layers(&m.mesh, shift).unwrap();
        let (layout, _) = replay_native::layout(&m, l.grid.origin, l.grid.height).unwrap();
        let good = load::analyse_full(&l.stack, &layout, &load::Extras::default());
        let good = maxima(&good, &layout, 0, l.grid);
        // This path's rigid-carrier equilibrium is not assessed: the model stores 0.0 for it,
        // which is reported as "not assessed", never as a ratio of 0 or a Patch section.
        assert_eq!(good["rigid_carrier"]["assessed_U"], json!(false));
        assert!(good["rigid_carrier"]["ratio_U"].is_null());
        assert!(good["rigid_carrier"].get("kind").is_none());
        assert_eq!(assessed_ratio(&good["rigid_carrier"]), None);
        for name in ["additive", "equal_share", "tributary"] {
            assert_eq!(good[name]["kind"], "Neck");
            assert!(ratio(&good[name]) > 300.0);
        }
        let mut broken = layout.clone();
        for ch in &mut broken.chains {
            ch.parts.retain(|p| p.kind != PartKind::Neck);
        }
        let bad = load::analyse_full(&l.stack, &broken, &load::Extras::default());
        let bad = maxima(&bad, &broken, 0, l.grid);
        // A skipped waist looks falsely safe: this canary must reject that adapter.
        assert!(ratio(&bad["additive"]) < 92.0);
        assert!(ratio(&good["additive"]) > 3.0 * ratio(&bad["additive"]));
    }
    #[test]
    fn solid_cap_replay_finds_written_arm_and_finite_load() {
        let spec: crate::spec::PlateSpec =
            serde_json::from_str(include_str!("../../../fixtures/calibration/cal1.json")).unwrap();
        for (id, lo, hi) in [("K-15", 15.0, 15.1), ("K-60", 60.0, 60.3)] {
            let s = spec.specimens.iter().find(|s| s.id == id).unwrap();
            let bytes = crate::build::specimen(s, &spec.head).to_bytes().unwrap();
            let p = plinth_chitubox::parse(&bytes).unwrap();
            let r = solid(&p.models[0], id).unwrap();
            let x = ratio(&r["maxima"]["additive"]);
            assert!(x > lo && x < hi, "{id}: {x}");
        }
    }
    #[test]
    fn frozen_cal1_table_matches_measurement_identity_and_records_release_ruling() {
        let frozen: Value = serde_json::from_str(include_str!(
            "../../../fixtures/calibration/cal1-frozen/FROZEN.json"
        ))
        .unwrap();
        let table: Value = serde_json::from_str(include_str!(
            "../../../fixtures/calibration/cal1-frozen/predictions.json"
        ))
        .unwrap();
        let measured: Value = serde_json::from_str(include_str!(
            "../../../fixtures/calibration/cal1-frozen/measurements.json"
        ))
        .unwrap();
        assert_eq!(table["freeze"], frozen);
        assert_eq!(measured["freeze"], frozen);
        assert_eq!(
            frozen["project_sha256"],
            "91611c85a661f5caf77704898ff8b04ff5ef69c05d5d6b1b024b65a72ceb28b1"
        );
        assert_eq!(frozen["seed"], 20261102);
        assert_eq!(table["pieces"].as_array().unwrap().len(), 16);
        assert_eq!(measured["first_lit_layer"].as_array().unwrap().len(), 16);
        assert_eq!(measured["ready_to_print"], true);
        assert_eq!(measured["status"], "released");
        assert!(measured["rulings"].as_str().unwrap().contains("R-REP"));
        assert_eq!(measured["island_count"], 0);
        assert_eq!(measured["waist_boundary_pass_count"], 216);
        assert_eq!(
            measured["pixel_diff"]["outside_replaced_slots_differing_pixels"],
            0
        );
        for row in table["pieces"].as_array().unwrap() {
            assert_ne!(row["id"], "A1");
            let index = row["replayed_index"].as_f64().unwrap();
            let additive = &row["sharing_maxima"]["additive"];
            assert_eq!(
                index.to_bits(),
                additive["ratio_U"].as_f64().unwrap().to_bits()
            );
            assert_eq!(row["replayed_layer"], additive["layer_M"]);
            assert_eq!(row["replayed_contact"], additive["contact_M"]);
        }
    }
    #[test]
    fn reviewed_prediction_numbers_survive_json_reencoding_bit_exactly() {
        for value in [
            311.22094393328246_f64,
            15.037470363267387,
            112.50639479508857,
            0.04601799947552978,
        ] {
            let encoded = serde_json::to_string(&value).unwrap();
            let decoded: f64 = serde_json::from_str(&encoded).unwrap();
            assert_eq!(value.to_bits(), decoded.to_bits(), "{encoded}");
        }
    }
    fn replay_reference() -> Value {
        serde_json::from_str(include_str!(
            "../../../fixtures/calibration/cal1-frozen/replay-maxima.json"
        ))
        .unwrap()
    }
    // Compare numeric leaves as IEEE-754 bits, not rounded display strings or
    // tolerance checks. Expected objects may project fields from the full replay.
    fn assert_replay_bits(actual: &Value, expected: &Value) {
        match expected {
            Value::Number(n) => assert_eq!(
                actual.as_f64().unwrap().to_bits(),
                n.as_f64().unwrap().to_bits(),
                "replay numeric drift: {actual} != {expected}"
            ),
            Value::Object(fields) => {
                for (key, value) in fields {
                    assert_replay_bits(&actual[key], value);
                }
            }
            Value::Array(values) => {
                assert_eq!(actual.as_array().unwrap().len(), values.len());
                for (a, b) in actual.as_array().unwrap().iter().zip(values) {
                    assert_replay_bits(a, b);
                }
            }
            _ => assert_eq!(actual, expected),
        }
    }
    #[test]
    fn frozen_rows_are_bit_exact_to_rebuilt_replay_output() {
        // This is a snapshot of OUTPUT from the rebuilt executable, not values
        // read back from the frozen table. A0's owner mesh is not a repo fixture;
        // its full replay was rerun on the frozen 7d507a3d input before snapshotting.
        let reference = replay_reference();
        assert_eq!(reference["replay_project_sha256"], HASH);
        assert_eq!(reference["pieces"].as_array().unwrap().len(), 7);
        let table: Value = serde_json::from_str(include_str!(
            "../../../fixtures/calibration/cal1-frozen/predictions.json"
        ))
        .unwrap();
        let rows = table["pieces"].as_array().unwrap();
        assert_eq!(rows.len(), 16);
        for row in rows {
            let replay = reference["pieces"]
                .as_array()
                .unwrap()
                .iter()
                .find(|r| r["id"] == row["id"])
                .unwrap();
            assert_replay_bits(&row["sharing_maxima"], &replay["maxima"]);
            let additive = &replay["maxima"]["additive"];
            for (field, source) in [
                ("replayed_index", "ratio_U"),
                ("replayed_layer", "layer_M"),
                ("replayed_contact", "contact_M"),
                (
                    "additive_limiting_whole_section_mm2_M",
                    "section_whole_area_mm2_M",
                ),
            ] {
                assert_replay_bits(&row[field], &additive[source]);
            }
            let prediction = call(row["id"].as_str().unwrap(), ratio(additive));
            for field in ["call", "index_U", "replication_expectation"] {
                assert_replay_bits(&row["compact_law_replay_call"][field], &prediction[field]);
            }
        }
    }
    #[test]
    fn frozen_record_files_are_intact() {
        // CAL-1 is printed and closed. `cal1-frozen/` is the record of the predictions made
        // with the load model at release (1a7600c), not a target for the current model.
        // FROZEN.json records only the project and GOO hashes, so each file is pinned here.
        let dir =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/calibration/cal1-frozen");
        for (name, sha) in [
            (
                "FROZEN.json",
                "f49d3cd66edfb2865514ca868b748a6612338baa38aa491d99901b6817a37214",
            ),
            (
                "measurements.json",
                "dbdf022e58db4fbb316eaab291e09527007ffa84eff3ef87619229f2aa91ba4e",
            ),
            (
                "predictions.json",
                "e91d62f5e0e4902c29eedd47f3d1b0eeb95d93d1a620a96f80a2de41ee283921",
            ),
            (
                "replay-maxima.json",
                "a0ecc3db697c413830e99918de704343e9ae455049820206e83f2418c997d132",
            ),
        ] {
            assert_eq!(hash(&dir.join(name)).unwrap(), sha, "{name} was edited");
        }
    }
    #[test]
    fn replay_runs_on_the_current_model_for_every_repo_specimen() {
        // The numbers belong to the current load model and are not compared with the frozen
        // record. Every specimen the repository can build must replay to finite outputs, and
        // rigid-carrier must be either a finite ratio or explicitly not assessed.
        let spec: crate::spec::PlateSpec =
            serde_json::from_str(include_str!("../../../fixtures/calibration/cal1.json")).unwrap();
        assert!(!spec.specimens.is_empty());
        for specimen in &spec.specimens {
            let bytes = crate::build::specimen(specimen, &spec.head)
                .to_bytes()
                .unwrap();
            let model = plinth_chitubox::parse(&bytes).unwrap().models.remove(0);
            let output = if model.supports.is_some() {
                native(&model).unwrap()
            } else {
                solid(&model, &specimen.id).unwrap()
            };
            for mode in ["rigid_carrier", "additive", "equal_share", "tributary"] {
                let r = &output["maxima"][mode];
                if assessed_ratio(r).is_none() {
                    assert_eq!(mode, "rigid_carrier", "{}: {mode}", specimen.id);
                    assert!(r["ratio_U"].is_null(), "{}", specimen.id);
                } else {
                    let x = ratio(r);
                    assert!(x.is_finite() && x > 0.0, "{} {mode}: {x}", specimen.id);
                }
            }
        }
    }
    #[test]
    fn displayed_replay_fields_use_additive_not_larger_tributary() {
        let mut row = json!({"replay":{"maxima":{
            "additive":{"ratio_U":311.22,"contact_M":0,"layer_M":116},
            "tributary":{"ratio_U":14084.87,"contact_M":7,"layer_M":401}
        }}});
        set_replayed_fields(&mut row);
        assert_eq!(row["replayed_model"], "additive");
        assert_eq!(row["replayed_index"], 311.22);
        assert_eq!(row["replayed_contact"], 0);
        assert_eq!(row["replayed_layer"], 116);
        assert_eq!(
            call("A0", row["replayed_index"].as_f64().unwrap())["index_U"],
            row["replayed_index"]
        );
    }
    #[test]
    fn narrowest_solid_section_and_qualifying_neck_table() {
        let r = json!({"paths":[
            {"narrowest_sampled_whole_section_M":{"area_mm2_M":0.04,"contact_M":0}},
            {"narrowest_sampled_whole_section_M":{"area_mm2_M":0.01,"contact_M":1}}
        ]});
        assert_eq!(narrowest(&r).unwrap()["contact_M"], 1);
        let mut contacts = vec![];
        let mut sections = vec![];
        for record in [1, 11, 13, 15, 17, 19] {
            contacts.push(json!({"tip_record_M":record,"contact_M":record,"neck_area_mm2_M":0.04}));
            sections.push(json!({"record_M":record,"kind":"Neck","rigid_carrier_U":[100.0,1],"additive_U":[200.0,2],"equal_share_U":[300.0,3],"tributary_U":[400.0,4]}));
        }
        let r = json!({"geometry":{"contacts_M":contacts},"sections":sections});
        let mut md = String::new();
        write_c4_necks(&mut md, &r, &r).unwrap();
        assert_eq!(md.lines().filter(|l| l.starts_with("| A")).count(), 12);
        assert!(md.contains("NOT the horizontal sliced areas"));
        assert!(md.contains("200.00 / 2"));
    }
    #[test]
    fn patch_exposure_and_compact_law_boundaries() {
        let p = PartSection {
            kind: PartKind::Patch,
            record: None,
            area_lower_mm2: 2.0,
            z_range_mm: [3., 5.],
        };
        assert!(effective(&p, 3.0).abs() < 1e-12);
        assert!((effective(&p, 4.0) - 1.0).abs() < 1e-12);
        assert_eq!(call("S-100", 92.0)["call"], "fail");
        assert_eq!(call("S-100", 91.99)["call"], "no clear call");
        assert_eq!(call("S-100", 6.6)["call"], "hold");
        assert_eq!(call("W", 300.0)["call"], "no clear call");
        assert_eq!(call("K-15", 15.0)["call"], "no clear call");
    }
}
