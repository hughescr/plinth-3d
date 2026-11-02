//! Read-only assembled CAL-1 slice evidence. No project or printer-file writes.
use plinth_calibration::{
    Result,
    goo::{self, Blob, Goo, Span},
    goo_grid, plate,
};
use plinth_chitubox::{Model, Project, RecordType};
use serde_json::{Value, json};
use std::{collections::BTreeMap, path::Path};

fn pixel_xy(xy: [f64; 2], resolution: [u16; 2], pitch: [f64; 2]) -> [f64; 2] {
    std::array::from_fn(|k| f64::from(resolution[k]) / 2.0 + 0.5 - xy[k] / pitch[k])
}

fn owner(xy: [f64; 2], boxes: &[plate::Bounds], pitch: [f64; 2]) -> Result<usize> {
    let hits: Vec<_> = boxes
        .iter()
        .enumerate()
        .filter(|(_, b)| (0..2).all(|k| xy[k] >= b.lo[k] - pitch[k] && xy[k] <= b.hi[k] + pitch[k]))
        .map(|(i, _)| i)
        .collect();
    match hits.as_slice() {
        [i] => Ok(*i),
        _ => Err(format!("blob at {xy:?} has {} footprint owners", hits.len()).into()),
    }
}

fn blob_at<'a>(xy: [f64; 2], spans: &[Span], blobs: &'a [Blob]) -> Option<&'a Blob> {
    blobs.iter().find(|b| {
        b.span_ids.iter().any(|&i| {
            let s = spans[i];
            xy[0] >= f64::from(s.x0)
                && xy[0] < f64::from(s.x1)
                && xy[1] >= f64::from(s.y)
                && xy[1] < f64::from(s.y + 1)
        })
    })
}

/// Register from the plate-contact mask, independently of the waist/tip tests.
/// Bounding-box centres are insensitive to symmetric AA edge inflation. Retain
/// the subpixel residual rather than adjusting shape or fitting each waist.
fn mask_centre(spans: &[Span]) -> Result<[f64; 2]> {
    if spans.is_empty() {
        return Err("cannot register an empty first-layer mask".into());
    }
    let x0 = spans.iter().map(|s| s.x0).min().unwrap_or(0);
    let x1 = spans.iter().map(|s| s.x1).max().unwrap_or(0);
    let y0 = spans.iter().map(|s| s.y).min().unwrap_or(0);
    let y1 = spans.iter().map(|s| s.y + 1).max().unwrap_or(0);
    Ok([
        (f64::from(x0) + f64::from(x1)) / 2.0,
        (f64::from(y0) + f64::from(y1)) / 2.0,
    ])
}

fn registration_shift(native: &[Span], predicted: [f64; 2]) -> Result<([i32; 2], [f64; 2])> {
    let a = mask_centre(native)?;
    let delta: [f64; 2] = std::array::from_fn(|k| a[k] - predicted[k]);
    if delta.iter().any(|d| d.abs() > 2.0) {
        return Err(format!("first-layer registration exceeds two pixels: {delta:?}").into());
    }
    let shift = delta.map(|d| d.round() as i32);
    Ok((
        shift,
        std::array::from_fn(|k| delta[k] - f64::from(shift[k])),
    ))
}

fn shifted(spans: &[Span], shift: [i32; 2]) -> Result<Vec<Span>> {
    spans
        .iter()
        .map(|s| {
            Ok(Span {
                x0: u32::try_from(i64::from(s.x0) + i64::from(shift[0]))?,
                x1: u32::try_from(i64::from(s.x1) + i64::from(shift[0]))?,
                y: u32::try_from(i64::from(s.y) + i64::from(shift[1]))?,
                value: s.value,
            })
        })
        .collect()
}

fn registrations(
    project: &Project,
    goo: &Goo,
    boxes: &[plate::Bounds],
) -> Result<(Vec<[i32; 2]>, Vec<Value>)> {
    let grid = goo_grid::Grid::from_header(&goo.header);
    let spans = goo.spans(0)?;
    let mut native = vec![Vec::new(); project.models.len()];
    for blob in goo::blobs(&spans, &[]) {
        let world = std::array::from_fn(|k| {
            (f64::from(grid.resolution[k]) / 2.0 + 0.5 - blob.centroid[k]) * grid.pitch[k]
        });
        let i = owner(world, boxes, grid.pitch)?;
        native[i].extend(blob.span_ids.iter().map(|&j| spans[j]));
    }
    let mut shifts = Vec::new();
    let mut evidence = Vec::new();
    for (i, m) in project.models.iter().enumerate() {
        let frame = goo_grid::offset(m);
        let z_local = grid.height / 2.0 - frame[2];
        // Only triangles crossing the first midpoint are needed for registration.
        // Slice them in one batch; no waist or tip pixels influence this fit.
        let triangles: Vec<_> = plate::triangles(m)
            .into_iter()
            .filter(|t| {
                let lo = t
                    .iter()
                    .map(|p| f64::from(p[2]))
                    .fold(f64::INFINITY, f64::min);
                let hi = t
                    .iter()
                    .map(|p| f64::from(p[2]))
                    .fold(f64::NEG_INFINITY, f64::max);
                lo <= z_local && hi > z_local
            })
            .collect();
        // Use continuous section bounds, not raster bounds: rasterizing the
        // reference first can add a half-pixel bbox quantization and round the
        // wrong way. Intersect all crossing edges at the first midpoint.
        let mut lo = [f64::INFINITY; 2];
        let mut hi = [f64::NEG_INFINITY; 2];
        for t in &triangles {
            for j in 0..3 {
                let (a, b) = (t[j], t[(j + 1) % 3]);
                let (az, bz) = (f64::from(a[2]), f64::from(b[2]));
                if (az <= z_local && bz > z_local) || (bz <= z_local && az > z_local) {
                    let f = (z_local - az) / (bz - az);
                    for k in 0..2 {
                        let v = f64::from(a[k]) + f * (f64::from(b[k]) - f64::from(a[k]));
                        lo[k] = lo[k].min(v);
                        hi[k] = hi[k].max(v);
                    }
                }
            }
        }
        if lo.iter().chain(&hi).any(|v| !v.is_finite()) {
            return Err("missing first-layer section".into());
        }
        let predicted = pixel_xy(
            std::array::from_fn(|k| (lo[k] + hi[k]) / 2.0 + frame[k]),
            grid.resolution,
            grid.pitch,
        );
        let (shift, residual) = registration_shift(&native[i], predicted)?;
        evidence.push(json!({"model":i,"name":m.name,"registration_layer_1based":1,
            "native_bbox_centre_pixel":mask_centre(&native[i])?,"predicted_bbox_centre_pixel":predicted,
            "native_minus_mesh_shift_pixels":shift,"residual_pixels":residual}));
        shifts.push(shift);
    }
    Ok((shifts, evidence))
}

fn tracks(
    model: &Model,
    goo: &Goo,
    shift: [i32; 2],
    z: f64,
    spans: &[Span],
    blobs: &[Blob],
) -> Vec<Value> {
    let Some(supports) = &model.supports else {
        return vec![];
    };
    let frame = goo_grid::offset(model);
    let local_z = z - frame[2];
    supports.records.iter().enumerate().filter_map(|(i, r)| {
        if ![RecordType::TIP_CONE, RecordType::TIP_BALL, RecordType::ROD].contains(&r.record_type) { return None; }
        let dz = f64::from(r.end[2]) - f64::from(r.start[2]);
        if dz.abs() < 1e-9 { return None; }
        let f = (local_z - f64::from(r.start[2])) / dz;
        if !(-0.1..=1.1).contains(&f) { return None; }
        let world = std::array::from_fn(|k| f64::from(r.start[k]) + f * (f64::from(r.end[k]) - f64::from(r.start[k])) + frame[k]);
        let xy = pixel_xy(world, goo.header.resolution, goo.header.pitch());
        let xy: [f64; 2] = std::array::from_fn(|k| xy[k] + f64::from(shift[k]));
        Some(json!({"record":i,"axis_fraction_at_z_top":f,"predicted_xy_at_z_top":xy,"blob":blob_at(xy, spans, blobs)}))
    }).collect()
}

/// Symmetric one-row binary boundary tolerance: either mask must lie inside
/// the other's one-pixel Chebyshev dilation (14 um X, 19 um Y on this grid).
fn outside_one_row(a: &[Span], b: &[Span]) -> usize {
    use std::collections::BTreeSet;
    let expanded: BTreeSet<_> = b
        .iter()
        .flat_map(|s| {
            (s.x0..s.x1).flat_map(move |x| {
                (-1..=1).flat_map(move |dy| {
                    (-1..=1).map(move |dx| (i64::from(x) + dx, i64::from(s.y) + dy))
                })
            })
        })
        .collect();
    a.iter()
        .flat_map(|s| (s.x0..s.x1).map(move |x| (i64::from(x), i64::from(s.y))))
        .filter(|p| !expanded.contains(p))
        .count()
}

struct WaistReference {
    model: usize,
    contact: usize,
    xy: [f64; 2],
    layers: Vec<Vec<Span>>,
}

/// Batch each small written waist mesh on the same grid. This is geometry
/// evidence, not cured area, and uses one mesh slice for all five waist layers.
fn waist_references(
    project: &Project,
    goo: &Goo,
    shifts: &[[i32; 2]],
) -> Result<Vec<WaistReference>> {
    let grid = goo_grid::Grid::from_header(&goo.header);
    let mut references = Vec::new();
    for (i, m) in project.models.iter().enumerate() {
        let frame = goo_grid::offset(m);
        let heads: Vec<_> = if m.name.contains("-W;") {
            [-6.25, -3.75, -1.25, 1.25, 3.75, 6.25]
                .into_iter()
                .map(|x| {
                    (
                        [f64::from(x), 0.0],
                        plinth_calibration::witness::column(&m.mesh, [x, 0.0]),
                    )
                })
                .collect()
        } else if m.name.contains("-B-100;")
            || m.name.contains("-S-100;")
            || m.name.contains("-S-324;")
        {
            let records = &m.supports.as_ref().ok_or("missing coupon heads")?.records;
            records
                .get(..28)
                .ok_or("incomplete coupon heads")?
                .chunks_exact(7)
                .map(|r| {
                    (
                        [f64::from(r[1].start[0]), f64::from(r[1].start[1])],
                        r[1].mesh.clone(),
                    )
                })
                .collect()
        } else {
            vec![]
        };
        for (contact, (site, mesh)) in heads.into_iter().enumerate() {
            let xy = pixel_xy(
                [site[0] + frame[0], site[1] + frame[1]],
                goo.header.resolution,
                grid.pitch,
            );
            references.push(WaistReference {
                model: i,
                contact,
                xy: std::array::from_fn(|k| xy[k] + f64::from(shifts[i][k])),
                layers: goo_grid::slice(&mesh, frame, grid, goo_grid::Boundary::Centre)?
                    .iter()
                    .map(|s| shifted(s, shifts[i]))
                    .collect::<Result<_>>()?,
            });
        }
    }
    Ok(references)
}

fn witness_comparison(project: &Project) -> Value {
    let witnesses: Vec<_> = project
        .models
        .iter()
        .enumerate()
        .filter(|(_, m)| m.name.contains("-W;"))
        .collect();
    if let [(ai, a), (bi, b)] = witnesses.as_slice() {
        let mut ac = (*a).clone();
        let mut bc = (*b).clone();
        ac.name.clear();
        bc.name.clear();
        ac.translate = [0.0; 3];
        bc.translate = [0.0; 3];
        json!({"models":[ai,bi],"translate_mm":[a.translate,b.translate],"same_z_translation_bits":a.translate[2].to_bits()==b.translate[2].to_bits(),"geometry_and_supports_identical":ac==bc,"placement_fields":"Only translate; rotation and scale are baked into identical meshes. No per-model hidden lift is represented by the strict project reader."})
    } else {
        json!(null)
    }
}

fn run() -> Result<()> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    let [project_path, goo_path, output] = args.as_slice() else {
        return Err(
            "usage: cal1-slice READ_ONLY_PROJECT READ_ONLY_GOO NEW_OUTPUT_DIRECTORY".into(),
        );
    };
    let project = plinth_chitubox::parse(&std::fs::read(project_path)?)?;
    let bytes = std::fs::read(goo_path)?;
    let goo = Goo::parse(&bytes)?;
    if goo.to_bytes()? != bytes {
        return Err("GOO lossless roundtrip differs".into());
    }
    let out = Path::new(output);
    std::fs::create_dir(out)?;
    let boxes: Vec<_> = project.models.iter().map(plate::bounds).collect();
    let pitch = goo.header.pitch();
    let pa = pitch[0] * pitch[1];
    let models: Vec<_> = project.models.iter().map(|m| {
        let b = plate::bounds(m);
        let records = m.supports.as_ref().map(|s| s.records.iter().enumerate().map(|(i, r)| {
            json!({"record":i,"type":r.record_type.0,"start":r.start,"end":r.end,"r1":r.r1,"r2":r.r2})
        }).collect::<Vec<_>>());
        json!({"name":m.name,"translate":m.translate,"offset":goo_grid::offset(m),"bounds":{"lo":b.lo,"hi":b.hi},"records":records})
    }).collect();
    let (shifts, registration) = registrations(&project, &goo, &boxes)?;
    let references = waist_references(&project, &goo, &shifts)?;
    let mut boundary_checks = Vec::new();
    let mut layers = Vec::new();
    let mut islands = Vec::new();
    let mut first: Vec<Option<Value>> = vec![None; project.models.len()];
    let mut histogram = BTreeMap::<u8, u64>::new();
    let mut previous = Vec::new();
    for (li, layer) in goo.layers.iter().enumerate() {
        let spans = goo.spans(li)?;
        for s in &spans {
            *histogram.entry(s.value).or_default() += u64::from(s.x1 - s.x0);
        }
        let blobs = goo::blobs(&spans, &previous);
        let mut grouped = vec![Vec::new(); project.models.len()];
        for b in blobs {
            let world = std::array::from_fn(|k| {
                (f64::from(goo.header.resolution[k]) / 2.0 + 0.5 - b.centroid[k]) * pitch[k]
            });
            let i = owner(world, &boxes, pitch)?;
            if li > 0 && b.overlap_below_pixels == 0 {
                islands.push(json!({"model":i,"name":project.models[i].name,"layer":li+1,"z_top_mm":layer.z,"blob":b}));
            }
            grouped[i].push(b);
        }
        if (4.39999..=4.60001).contains(&f64::from(layer.z)) {
            for reference in &references {
                let i = reference.model;
                let contact = reference.contact;
                let b = blob_at(reference.xy, &spans, &grouped[i]).ok_or("missing waist blob")?;
                let native: Vec<_> = b.span_ids.iter().map(|&j| spans[j]).collect();
                let mesh = reference
                    .layers
                    .get(li)
                    .ok_or("missing reference waist layer")?;
                let a = outside_one_row(&native, mesh);
                let b = outside_one_row(mesh, &native);
                boundary_checks.push(json!({"model":i,"contact":contact,"layer":li+1,"z_top_mm":layer.z,"native_pixels":native.iter().map(|s|u64::from(s.x1-s.x0)).sum::<u64>(),"mesh_centre_pixels":mesh.iter().map(|s|u64::from(s.x1-s.x0)).sum::<u64>(),"native_outside_dilated_mesh_pixels":a,"mesh_outside_dilated_native_pixels":b,"within_one_row":a==0&&b==0}));
            }
        }
        let mut pieces = Vec::new();
        for (i, bs) in grouped.iter().enumerate() {
            let pixels: u64 = bs.iter().map(|b| b.pixels).sum();
            let weighted: f64 = bs.iter().map(|b| b.weighted_pixels).sum();
            if pixels > 0 && first[i].is_none() {
                first[i] = Some(
                    json!({"layer_1based":li+1,"layer_0based":li,"z_top_mm":layer.z,"pixels":pixels,"weighted_pixels":weighted}),
                );
            }
            let ts = if project.models[i].name.contains("-A0;")
                || project.models[i].name.contains("-A1;")
            {
                tracks(
                    &project.models[i],
                    &goo,
                    shifts[i],
                    f64::from(layer.z),
                    &spans,
                    bs,
                )
            } else {
                vec![]
            };
            pieces.push(json!({"model":i,"pixels":pixels,"weighted_pixels":weighted,"lit_any_mm2":pixels as f64*pa,"weighted_mm2":weighted*pa,"blobs":bs,"tracks":ts}));
        }
        let total: u64 = grouped.iter().flatten().map(|b| b.pixels).sum();
        let weighted: f64 = grouped.iter().flatten().map(|b| b.weighted_pixels).sum();
        layers.push(json!({"layer":li+1,"z_top_mm":layer.z,"exposure_s":layer.exposure,"total_pixels":total,"total_weighted_pixels":weighted,"total_lit_any_mm2":total as f64*pa,"total_weighted_mm2":weighted*pa,"pieces":pieces}));
        previous = spans;
    }
    let witness_comparison = witness_comparison(&project);
    std::fs::write(
        out.join("decoded.json"),
        serde_json::to_vec_pretty(&json!({
            "status":"M: owner slice pixels, not cured material or physical loads", "project_path":project_path,"goo_path":goo_path,
            "header":goo.header,"pixel_pitch_mm":pitch,"pixel_area_mm2":pa,"models":models,"first_lit_layer":first,
            "witness_comparison":witness_comparison,"nonzero_grey_histogram":histogram,"islands":islands,"layers":layers,
            "registration":registration,
            "registration_method":"Per-piece first-layer native lit-any bounding-box centre minus continuous written first-midpoint section bbox centre, rounded to nearest integer pixel; fit independent of waist/tip layers. Same shift for every layer of that piece; no shape correction.",
            "waist_one_row_boundary_checks":boundary_checks,
            "one_row_definition":"Symmetric mask inclusion in the other mask's one-pixel Chebyshev dilation, X +/-1 and Y +/-1; midpoint centre-filled written waist mesh versus native lit-any blob. Explicit operational interpretation of the plan, not a percentage tolerance or a cure certificate."
        }))?,
    )?;
    println!(
        "Decoded {} layers, {} models, {} islands into {}",
        goo.layers.len(),
        project.models.len(),
        islands.len(),
        out.display()
    );
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
    #[test]
    fn footprint_mapping_preserves_axis_reversal_and_rejects_ambiguous_or_missing_owners() {
        let xy = pixel_xy([14.0, -19.0], [200, 200], [0.014, 0.019]);
        assert!((xy[0] + 899.5).abs() < 1e-9);
        assert!((xy[1] - 1100.5).abs() < 1e-9);
        let boxes = [plate::Bounds {
            lo: [-1.0, -1.0, 0.0],
            hi: [1.0, 1.0, 1.0],
        }];
        assert_eq!(owner([0.0, 0.0], &boxes, [0.014, 0.019]).expect("owner"), 0);
        assert!(owner([3.0, 0.0], &boxes, [0.014, 0.019]).is_err());
        assert!(owner([0.0, 0.0], &[boxes[0], boxes[0]], [0.014, 0.019]).is_err());
    }
    #[test]
    fn witness_comparison_does_not_hide_z_lift_or_geometry_change() {
        let a = Model {
            name: "front-W; calibration piece".into(),
            translate: [-10.0, -30.0, 0.0],
            mesh: vec![[[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 1.0]]],
            supports: None,
        };
        let mut b = a.clone();
        b.name = "back-W; calibration piece".into();
        b.translate = [10.0, 30.0, 0.0];
        let mut project = Project {
            models: vec![a, b],
            ..Project::default()
        };
        let same = witness_comparison(&project);
        assert_eq!(same["same_z_translation_bits"], true);
        assert_eq!(same["geometry_and_supports_identical"], true);
        project.models[1].translate[2] = 3.0;
        assert_eq!(
            witness_comparison(&project)["same_z_translation_bits"],
            false
        );
        project.models[1].mesh[0][0][2] = 3.0;
        assert_eq!(
            witness_comparison(&project)["geometry_and_supports_identical"],
            false
        );
    }
    #[test]
    fn one_row_boundary_rejects_two_pixel_shift_and_missing_feature() {
        let a = [Span {
            y: 10,
            x0: 10,
            x1: 12,
            value: 255,
        }];
        let one = [Span {
            y: 11,
            x0: 11,
            x1: 13,
            value: 255,
        }];
        let two = [Span {
            y: 12,
            x0: 12,
            x1: 14,
            value: 255,
        }];
        assert_eq!(outside_one_row(&a, &one), 0);
        assert_eq!(outside_one_row(&one, &a), 0);
        assert!(outside_one_row(&two, &a) > 0);
        assert_eq!(outside_one_row(&a, &[]), 2);
    }
    #[test]
    fn registration_removes_only_constant_translation_not_missing_shape() {
        let sample = |y, x0, x1| Span {
            y,
            x0,
            x1,
            value: 255,
        };
        let mesh = [sample(20, 20, 30), sample(21, 20, 30)];
        let native = shifted(&mesh, [-1, -1]).expect("shift");
        let (shift, residual) =
            registration_shift(&native, mask_centre(&mesh).expect("centre")).expect("register");
        assert_eq!(shift, [-1, -1]);
        assert!(residual.iter().all(|r| r.abs() < f64::EPSILON));
        let registered = shifted(&mesh, shift).expect("registered");
        assert_eq!(outside_one_row(&native, &registered), 0);
        assert_eq!(outside_one_row(&registered, &native), 0);
        // A missing centre stripe keeps the bbox/registration unchanged but
        // must still fail: registration cannot fit away a missing feature.
        let broken = [
            sample(19, 19, 21),
            sample(19, 27, 29),
            sample(20, 19, 21),
            sample(20, 27, 29),
        ];
        let (same, _) = registration_shift(&broken, mask_centre(&mesh).expect("centre"))
            .expect("register broken");
        assert_eq!(same, [-1, -1]);
        assert!(outside_one_row(&registered, &broken) > 0);
        assert!(registration_shift(&[], [25.0, 21.0]).is_err());
        assert!(registration_shift(&shifted(&mesh, [3, 0]).expect("shift"), [25.0, 21.0]).is_err());
        assert!(shifted(&[sample(0, 0, 1)], [-1, 0]).is_err());
        // Rasterizing this reference before registration would move its bbox
        // by half a pixel. Register against continuous section coordinates.
        let half = [sample(20, 20, 31)];
        let (shift, _) = registration_shift(&half, [25.5001, 20.5001]).expect("continuous");
        assert_eq!(shift, [0, 0]);
    }
    #[test]
    fn blob_hit_requires_actual_lit_pixel_not_bounding_box() {
        let spans = [
            Span {
                y: 0,
                x0: 0,
                x1: 1,
                value: 255,
            },
            Span {
                y: 1,
                x0: 1,
                x1: 2,
                value: 255,
            },
        ];
        let blobs = goo::blobs(&spans, &[]);
        assert!(blob_at([0.5, 0.5], &spans, &blobs).is_some());
        assert!(blob_at([1.5, 0.5], &spans, &blobs).is_none());
    }
}
