//! Review-only hollow evidence bound to the serialized production model.
use crate::json::Json;
use plinth_geom::{
    Mesh,
    hollow::{self, Evidence, Hollowed, RimPhase},
};

fn numbers(v: impl IntoIterator<Item = f64>) -> Json {
    Json::Arr(v.into_iter().map(Json::Num).collect())
}
fn soup(triangles: &[plinth_chitubox::Triangle]) -> Result<Mesh, String> {
    let axes: [Vec<f64>; 3] = std::array::from_fn(|k| {
        triangles
            .iter()
            .flatten()
            .map(|v| f64::from(v[k]))
            .collect()
    });
    Mesh::from_soup(&axes[0], &axes[1], &axes[2]).map_err(|e| e.to_string())
}
fn centred_source(original: &Mesh, hollowed: &Mesh) -> Result<Mesh, String> {
    let b = hollowed.bounds();
    let axes = [original.xs(), original.ys(), original.zs()];
    let [x, y, z] = std::array::from_fn(|k| {
        let centre = (b.min[k] + b.max[k]) / 2.;
        axes[k]
            .iter()
            .map(|v| f64::from((v - centre) as f32))
            .collect::<Vec<_>>()
    });
    Mesh::new(x, y, z, original.indices().to_vec()).map_err(|e| e.to_string())
}
fn phase(p: &RimPhase, lift: f64) -> Json {
    Json::obj([
        ("plate_slab_bottom_mm", Json::Num(p.slab_bottom_mm + lift)),
        ("plate_slab_top_mm", Json::Num(p.slab_top_mm + lift)),
        ("model_local_slab_top_mm", Json::Num(p.slab_top_mm)),
        ("area_mm2", Json::Num(p.area_mm2)),
        ("outer_radius_mm", Json::Num(p.outer_radius_mm)),
        ("pieces", Json::Uint(p.pieces as u64)),
    ])
}
fn dimensions(h: &Hollowed, e: &Evidence, original: &Mesh, plate_z: f64) -> Json {
    let holes = e
        .holes
        .iter()
        .enumerate()
        .map(|(i, h)| {
            Json::obj([
                (
                    "mouth",
                    Json::str(if i == 0 {
                        "nearest plate / prints first"
                    } else {
                        "downhill drain when hanging"
                    }),
                ),
                ("model_local_centre_xy_mm", numbers(h.centre_xy_mm)),
                (
                    "plate_z_range_mm",
                    numbers(h.z_range_mm.map(|z| z - plate_z)),
                ),
                ("clear_diameter_mm", Json::Num(h.clear_diameter_mm)),
                (
                    "diameter_roundoff_reserve_mm",
                    Json::Num(h.clear_diameter_mm - hollow::DIAMETER),
                ),
                ("measured_wall_triangles", Json::Uint(h.triangles as u64)),
            ])
        })
        .collect();
    Json::obj([
        (
            "max_convexity_violation_mm",
            Json::Num(h.convexity.maximum_mm),
        ),
        ("convexity_tolerance_mm", Json::Num(hollow::CONVEX_EPS)),
        (
            "convexity_violation_upper_bound_mm",
            Json::Num(h.convexity.upper_bound_mm),
        ),
        (
            "convexity_witness_triangle",
            Json::Uint(h.convexity.triangle as u64),
        ),
        (
            "convexity_witness_vertex",
            Json::Uint(h.convexity.vertex as u64),
        ),
        ("minimum_wall_lower_bound_mm", Json::Num(e.minimum_wall_mm)),
        (
            "exterior_inward_rounding_bound_mm",
            Json::Num(e.exterior_inward_deviation_mm),
        ),
        (
            "minimum_wall_excess_mm",
            Json::Num(e.minimum_wall_mm - hollow::WALL),
        ),
        (
            "wall_certificate_triangles",
            Json::Uint(e.inner_triangles as u64),
        ),
        ("holes", Json::Arr(holes)),
        (
            "original_dimensions_mm",
            numbers(original.bounds().extent()),
        ),
        ("emitted_model_triangles", Json::Uint(e.triangles as u64)),
        (
            "original_model_triangles",
            Json::Uint(original.triangle_count() as u64),
        ),
        (
            "preserved_uncut_exterior_triangles",
            Json::Uint(e.preserved_uncut_triangles as u64),
        ),
    ])
}
fn volumes(h: &Hollowed, e: &Evidence, support_sum: f64) -> Json {
    Json::obj([
        ("original_volume_mm3", Json::Num(e.original_mm3)),
        ("cavity_including_bores_mm3", Json::Num(e.cavity_mm3)),
        (
            "offset_void_before_drilling_mm3_construction_only",
            Json::Num(h.offset_void_mm3),
        ),
        ("model_material_mm3", Json::Num(e.material_mm3)),
        ("model_resin_saved_ml", Json::Num(e.cavity_mm3 / 1000.)),
        ("support_record_volume_sum_mm3", Json::Num(support_sum)),
        (
            "net_resin_saved_ml_lower_bound",
            Json::Num((e.cavity_mm3 - support_sum) / 1000.),
        ),
        (
            "volume_note",
            Json::str(
                "Support record volumes are summed without subtracting designed overlaps; net saving is conservative, not an assembly Boolean-union measurement.",
            ),
        ),
    ])
}
fn rims(e: &Evidence, source: &Mesh, plate_z: f64) -> Json {
    Json::obj([
        (
            "first_material_plate_z_mm",
            Json::Num(e.first_material_z_mm - plate_z),
        ),
        (
            "first_material_above_uncut_pole_mm",
            Json::Num(e.first_material_z_mm - source.bounds().min[2]),
        ),
        (
            "first_bore_rim_plate_z_mm",
            Json::Num(e.first_bore_rim_z_mm - plate_z),
        ),
        (
            "first_inner_surface_plate_z_mm",
            Json::Num(e.first_inner_z_mm - plate_z),
        ),
        ("first_edge_005mm_slab", phase(&e.first_edge_slab, -plate_z)),
        ("first_cavity_layer", phase(&e.first_cavity_layer, -plate_z)),
        (
            "rim_birth_phase_tests",
            Json::Arr(e.phases.iter().map(|p| phase(p, -plate_z)).collect()),
        ),
        (
            "ideal_sphere_reference",
            Json::str(
                "§4 derivation R=10 b=1.5: birth 0.11314 mm above uncut pole; 0.05 mm edge-start slab area 3.098 mm² and envelope outer radius 1.79894 mm. Actual f32 faceting and phase pieces above, not this ideal ring, govern placement.",
            ),
        ),
    ])
}
fn fields(value: Json) -> Vec<(String, Json)> {
    match value {
        Json::Obj(pairs) => pairs,
        _ => unreachable!("all hollow evidence sections are objects"),
    }
}

pub fn written(original: &Mesh, h: &Hollowed, bytes: &[u8]) -> Result<Json, String> {
    let decoded = plinth_chitubox::parse(bytes).map_err(|e| e.to_string())?;
    let model = decoded.models.first().ok_or("missing emitted model")?;
    let mesh = soup(&model.mesh)?;
    let source = centred_source(original, &h.mesh)?;
    let e = hollow::measure(&source, &mesh)?;
    e.require_dimensions()?;
    let parts = model.supports.as_ref().map_or_else(Vec::new, |s| {
        s.records
            .iter()
            .map(|r| r.mesh.iter().map(|t| t.map(|v| v.map(f64::from))).collect())
            .collect()
    });
    let plate_z = mesh.bounds().min[2] - f64::from(model.translate[2]);
    let intrusions =
        hollow::bore_intrusions(&parts, &e, hollow::BoreExtent::WithOutlets { plate_z });
    let bore_intrusions = hollow::bore_intrusions(&parts, &e, hollow::BoreExtent::ThroughWall);
    let support_sum: f64 = parts.iter().map(|tris| signed_volume(tris).abs()).sum();
    let mut pairs = fields(Json::obj([
        (
            "basis",
            Json::str(
                "Decoded emitted f32 model; same bytes passed to the existing checker. Producer dimension certificate, not independent H5/FR-17a acceptance.",
            ),
        ),
        (
            "input_stl_centre_xy_mm",
            numbers((0..2).map(|k| (original.bounds().min[k] + original.bounds().max[k]) / 2.)),
        ),
        (
            "support_bore_or_outlet_surface_intrusion_records",
            Json::Arr(
                intrusions
                    .into_iter()
                    .map(|i| Json::Uint(i as u64))
                    .collect(),
            ),
        ),
        (
            "support_through_wall_bore_surface_intrusion_records",
            Json::Arr(
                bore_intrusions
                    .into_iter()
                    .map(|i| Json::Uint(i as u64))
                    .collect(),
            ),
        ),
        ("support_records_tested", Json::Uint(parts.len() as u64)),
        (
            "support_status",
            Json::str(
                "Today's plate-rooted columns, no bore reservations or model-rooted inside supports until steps 2–3. Reported intrusions are expected defects, not a print-ready layout. Surface/prism intersection hits do not certify flow or exclude complete solid containment. Independent H5 and FR-17a remain unverified.",
            ),
        ),
    ]));
    pairs.extend(fields(dimensions(h, &e, original, plate_z)));
    pairs.extend(fields(volumes(h, &e, support_sum)));
    pairs.extend(fields(rims(&e, &source, plate_z)));
    Ok(Json::Obj(pairs))
}
fn signed_volume(tris: &[[[f64; 3]; 3]]) -> f64 {
    tris.iter()
        .map(|[a, b, c]| {
            (a[0] * (b[1] * c[2] - b[2] * c[1])
                + a[1] * (b[2] * c[0] - b[0] * c[2])
                + a[2] * (b[0] * c[1] - b[1] * c[0]))
                / 6.
        })
        .sum()
}

#[cfg(test)]
#[path = "hollow_tests.rs"]
mod tests;
