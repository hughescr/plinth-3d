//! Orchestrator rulings R-S324/R-C4: narrowest sampled section, never a station window.
use crate::Result;
use serde_json::{Value, json};

#[derive(Clone, Debug)]
pub struct Sample {
    pub layer: u64,
    pub lit: f64,
    pub weighted: f64,
    pub merged: bool,
}
/// Retain the minimum separately for each area measure. A descending neck that
/// disappears into the body has no measurable minimum: the last isolated layer
/// is not a new admissible station. Areas are sampled, not a continuous bound.
pub fn narrowest(samples: &[Sample]) -> Option<(usize, usize, bool)> {
    let isolated: Vec<_> = samples
        .iter()
        .enumerate()
        .filter(|(_, s)| !s.merged && s.lit > 0.0 && s.weighted > 0.0)
        .collect();
    let lit = isolated
        .iter()
        .min_by(|a, b| a.1.lit.total_cmp(&b.1.lit))?
        .0;
    let weighted = isolated
        .iter()
        .min_by(|a, b| a.1.weighted.total_cmp(&b.1.weighted))?
        .0;
    let descending_into_merge = |index: usize, weighted: bool| {
        let area = |s: &Sample| if weighted { s.weighted } else { s.lit };
        samples.get(index + 1).is_some_and(|next| next.merged)
            && index > 0
            && !samples[index - 1].merged
            && area(&samples[index - 1]) > area(&samples[index])
    };
    Some((
        lit,
        weighted,
        !descending_into_merge(lit, false) && !descending_into_merge(weighted, true),
    ))
}
fn sample(row: &Value) -> Option<Sample> {
    let b = &row["blob"];
    let bounds = b["bounds"].as_array()?;
    Some(Sample {
        layer: row["layer"].as_u64()?,
        lit: b["pixels"].as_f64()?,
        weighted: b["weighted_pixels"].as_f64()?,
        merged: bounds[2].as_u64()? - bounds[0].as_u64()? > 200
            || bounds[3].as_u64()? - bounds[1].as_u64()? > 200,
    })
}
fn minimum_json(samples: &[Sample], pa: f64) -> Value {
    narrowest(samples).map_or(Value::Null,|(a,b,valid)|json!({"lit_any_layer_1based":samples[a].layer,"weighted_layer_1based":samples[b].layer,"lit_any_mm2":samples[a].lit*pa,"weighted_mm2":samples[b].weighted*pa,"minimum_measurable_before_body_merge":valid,"sample_count":samples.len()}))
}
/// The decoder batches native layer blobs. This reducer never re-reads a GOO
/// layer or changes registration to make a waist or neck measurement agree.
pub fn measure(models: &[Value], layers: &[Value], waist: &[Value], pa: f64) -> Result<Value> {
    let mut coupons = Vec::new();
    for (i, m) in models.iter().enumerate().filter(|(_, m)| {
        ["-B-100;", "-S-100;", "-S-324;"]
            .iter()
            .any(|id| m["name"].as_str().unwrap_or("").contains(id))
    }) {
        let body = layers
            .iter()
            .filter(|l| l["z_top_mm"].as_f64().unwrap_or(0.) >= 5.04999)
            .map(|l| &l["pieces"][i])
            .collect::<Vec<_>>();
        let lit = body
            .iter()
            .map(|b| b["lit_any_mm2"].as_f64().unwrap_or(0.))
            .fold(0., f64::max);
        let weighted = body
            .iter()
            .map(|b| b["weighted_mm2"].as_f64().unwrap_or(0.))
            .fold(0., f64::max);
        let mut contacts = Vec::new();
        for contact in 0..4 {
            let rows: Vec<_> = waist
                .iter()
                .filter(|r| r["model"] == i && r["contact"] == contact)
                .collect();
            if rows.is_empty()
                || !rows
                    .iter()
                    .any(|r| (r["z_top_mm"].as_f64().unwrap_or(0.) - 4.65).abs() < 0.0001)
            {
                return Err("waist scan must include all sampled layers through 4.65 mm".into());
            }
            let samples: Vec<_> = rows
                .iter()
                .map(|r| Sample {
                    layer: r["layer"].as_u64().unwrap(),
                    lit: r["lit_any_mm2"].as_f64().unwrap() / pa,
                    weighted: r["weighted_mm2"].as_f64().unwrap() / pa,
                    merged: false,
                })
                .collect();
            let mut min = minimum_json(&samples, pa);
            min["contact"] = json!(contact);
            min["lit_any_index"] =
                json!(lit / 4. / min["lit_any_mm2"].as_f64().ok_or("missing waist")?);
            min["weighted_index"] =
                json!(weighted / 4. / min["weighted_mm2"].as_f64().ok_or("missing waist")?);
            contacts.push(min);
        }
        coupons.push(json!({"model":i,"piece":m["name"],"body_maximum_mm2":{"lit_any":lit,"weighted":weighted},"narrowest_waist_per_contact":contacts,"rule":"R-S324 [S]: body maximum / 4 / narrowest sampled waist per contact; all six waist layers 4.40–4.65 mm included; same rule for both 100 rungs"}));
    }
    let mut tips = Vec::new();
    for band in ["front", "back"] {
        let find = |id: &str| {
            models.iter().position(|m| {
                m["name"]
                    .as_str()
                    .is_some_and(|s| s.starts_with(&format!("{band}-{id};")))
            })
        };
        let (Some(a0), Some(a1)) = (find("A0"), find("A1")) else {
            continue;
        };
        let mut necks = Vec::new();
        for record in (1..20).step_by(2) {
            let collect = |i: usize| -> Vec<Sample> {
                layers
                    .iter()
                    .filter_map(|l| {
                        let t = l["pieces"][i]["tracks"]
                            .as_array()?
                            .iter()
                            .find(|t| t["record"] == record)?;
                        let mut row = t.clone();
                        row["layer"] = l["layer"].clone();
                        sample(&row)
                    })
                    .collect()
            };
            let s0 = collect(a0);
            let s1 = collect(a1);
            let m0 = minimum_json(&s0, pa);
            let m1 = minimum_json(&s1, pa);
            let ratio = |key: &str| m0[key].as_f64().zip(m1[key].as_f64()).map(|(a, b)| a / b);
            let lit = ratio("lit_any_mm2");
            let weighted = ratio("weighted_mm2");
            let qualifies = m0["minimum_measurable_before_body_merge"] == true
                && m1["minimum_measurable_before_body_merge"] == true
                && lit.is_some_and(|v| v >= 2.)
                && weighted.is_some_and(|v| v >= 2.);
            necks.push(json!({"record":record,"a0":m0,"a1":m1,"lit_any_ratio":lit,"weighted_ratio":weighted,"qualifies":qualifies}));
        }
        let qualified: Vec<_> = necks
            .iter()
            .filter(|r| r["qualifies"] == true)
            .map(|r| r["record"].clone())
            .collect();
        tips.push(json!({"band":band,"keep_A1":qualified.len()>=6,"qualifying_records":qualified,"necks":necks}));
    }
    Ok(
        json!({"rulings":"Orchestrator R-S324 and R-C4 [S], 2026-11-02; not owner rulings","coupons":coupons,"C4":if tips.is_empty(){"deferred: A1 slots replaced by A0"}else{"diagnostic only; keep A1 requires >=6 measurable x2 necks"},"tips":tips,"neck_isolation":"Tracked cone axis fraction -0.1..1.1 at every native labelled layer. Blob extent >200 px in X or Y marks body merge (same operational exclusion as slice-check.json). A shrinking last isolated section before merge is not a measurable minimum. Not a continuous-area certificate."}),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    fn s(layer: u64, lit: f64, merged: bool) -> Sample {
        Sample {
            layer,
            lit,
            weighted: lit,
            merged,
        }
    }
    #[test]
    fn window_sabotage_reverses_x2_and_late_waist_changes_index() {
        // Window picks 925/387 >=2; full neck scan must find 761/387 <2.
        let samples = [
            s(57, 1001., false),
            s(58, 925., false),
            s(59, 820., false),
            s(60, 761., false),
            s(61, 900., false),
        ];
        let (i, _, valid) = narrowest(&samples).unwrap();
        assert!(valid);
        assert_eq!(samples[i].layer, 60);
        let (window_min, _, _) = narrowest(&samples[..2]).unwrap();
        assert!(samples[window_min].lit / 387. >= 2.);
        assert!(samples[i].lit / 387. < 2.);
        // 4.65 is narrower than every sample in the obsolete 4.40–4.60 scan.
        let waist = [
            s(88, 60., false),
            s(89, 59., false),
            s(90, 58., false),
            s(91, 57., false),
            s(92, 56., false),
            s(93, 50., false),
        ];
        assert_eq!(narrowest(&waist).unwrap().0, 5);
        assert_ne!(narrowest(&waist[..5]).unwrap().0, 5);
    }
    #[test]
    fn decoder_summary_uses_all_six_waist_layers_and_rejects_missing_last_layer() {
        let models = [json!({"name":"front-B-100; calibration"})];
        let layers = [json!({"z_top_mm":5.05,"pieces":[{"lit_any_mm2":20.,"weighted_mm2":18.}]} )];
        let mut waist = Vec::new();
        for layer in 88..=93 {
            for contact in 0..4 {
                waist.push(json!({"model":0,"contact":contact,"layer":layer,"z_top_mm":layer as f64*0.05,"lit_any_mm2":if layer==93 {0.01}else{0.02},"weighted_mm2":if layer==93 {0.009}else{0.018}}));
            }
        }
        let result = measure(&models, &layers, &waist, 0.001).unwrap();
        for contact in result["coupons"][0]["narrowest_waist_per_contact"]
            .as_array()
            .unwrap()
        {
            assert_eq!(contact["lit_any_layer_1based"], 93);
            assert!((contact["lit_any_index"].as_f64().unwrap() - 500.).abs() < 1e-9);
            assert!((contact["weighted_index"].as_f64().unwrap() - 500.).abs() < 1e-9);
        }
        waist.truncate(20);
        assert!(measure(&models, &layers, &waist, 0.001).is_err());
    }
    #[test]
    fn c4_measure_requires_six_necks_and_measurable_minima_on_both_pieces() {
        let models = [
            json!({"name":"front-A0; calibration"}),
            json!({"name":"front-A1; calibration"}),
        ];
        // Exactly six records have both independent area ratios at the x2 boundary.
        let tracks = |pixels: f64| -> Vec<Value> {
            (1..20).step_by(2).map(|record| {
                let area = if record <= 11 { pixels } else { 15.0 };
                json!({"record":record,"blob":{"pixels":area,"weighted_pixels":area,"bounds":[0,0,10,10]}})
            }).collect()
        };
        let mut layers =
            vec![json!({"layer":1,"pieces":[{"tracks":tracks(20.)},{"tracks":tracks(10.)}]})];
        let verdict =
            |layers: &[Value]| measure(&models, layers, &[], 0.001).unwrap()["tips"][0].clone();
        let pass = verdict(&layers);
        assert_eq!(pass["qualifying_records"], json!([1, 3, 5, 7, 9, 11]));
        assert_eq!(pass["keep_A1"], true);
        // Sabotage only the weighted ratio: five qualifying records cannot keep A1.
        layers[0]["pieces"][1]["tracks"][5]["blob"]["weighted_pixels"] = json!(10.01);
        assert_eq!(
            verdict(&layers)["qualifying_records"]
                .as_array()
                .unwrap()
                .len(),
            5
        );
        assert_eq!(verdict(&layers)["keep_A1"], false);
        layers[0]["pieces"][1]["tracks"][5]["blob"]["weighted_pixels"] = json!(10.);
        // Both A0 and A1 must have a measurable exposed minimum. A last isolated
        // shrinking sample before body merge has a numeric ratio, but cannot count.
        for unmeasurable in 0..2 {
            let mut sabotaged = layers.clone();
            if unmeasurable == 0 {
                sabotaged[0]["pieces"][0]["tracks"][5]["blob"]["pixels"] = json!(22.);
                sabotaged[0]["pieces"][0]["tracks"][5]["blob"]["weighted_pixels"] = json!(22.);
            }
            let mut next = sabotaged[0].clone();
            next["layer"] = json!(2);
            let blob = &mut next["pieces"][unmeasurable]["tracks"][5]["blob"];
            blob["pixels"] = json!(if unmeasurable == 0 { 21. } else { 9. });
            blob["weighted_pixels"] = blob["pixels"].clone();
            let mut merged = next.clone();
            merged["layer"] = json!(3);
            merged["pieces"][unmeasurable]["tracks"][5]["blob"]["bounds"] = json!([0, 0, 201, 10]);
            sabotaged.extend([next, merged]);
            let failed = verdict(&sabotaged);
            assert_eq!(
                failed["keep_A1"], false,
                "piece {unmeasurable} minimum is not measurable"
            );
            assert_eq!(failed["necks"][5]["qualifies"], false);
            assert!(failed["necks"][5]["lit_any_ratio"].as_f64().unwrap() >= 2.);
            assert!(failed["necks"][5]["weighted_ratio"].as_f64().unwrap() >= 2.);
            let side = if unmeasurable == 0 { "a0" } else { "a1" };
            assert_eq!(
                failed["necks"][5][side]["minimum_measurable_before_body_merge"],
                false
            );
        }
        // The lit-any ratio is independently binding as well.
        layers[0]["pieces"][1]["tracks"][5]["blob"]["pixels"] = json!(10.01);
        assert_eq!(verdict(&layers)["keep_A1"], false);
    }
    #[test]
    fn descending_into_body_merge_does_not_qualify_as_a_minimum() {
        let samples = [
            s(401, 1500., false),
            s(402, 1339., false),
            s(403, 9000., true),
        ];
        let (i, _, valid) = narrowest(&samples).unwrap();
        assert_eq!(i, 1);
        assert!(!valid);
        assert!(samples[i].lit / 578. > 2.); // Would falsely qualify records 13/15.
        assert!(narrowest(&[]).is_none());
    }
}
