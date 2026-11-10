//! CAL-2 per-coupon measurement, always from a model re-read from the written file.
//! The body area is the written body mesh sliced at the loaded layer. Head sections are
//! sliced from the written record meshes, never taken from the design fields.
use crate::{
    Result,
    cal2_head::{exit_t, polygon_area},
    cal2_spec::{Cal2Spec, Item, Kind},
    goo_grid::{self, Boundary, Grid},
    section::{self, SectionMesh},
};
use plinth_chitubox::{Model, RecordType as Ty, SupportRecord};
use serde_json::{Value, json};

type V = [f64; 3];
fn v(p: [f32; 3]) -> V {
    p.map(f64::from)
}
fn sub(a: V, b: V) -> V {
    std::array::from_fn(|i| a[i] - b[i])
}
fn len(a: V) -> f64 {
    a.iter().map(|x| x * x).sum::<f64>().sqrt()
}
fn ball_centre(r: &SupportRecord) -> V {
    let (a, b) = (v(r.start), v(r.end));
    std::array::from_fn(|i| (a[i] + b[i]) / 2.0)
}
fn at(mesh: &[plinth_chitubox::Triangle], origin: V, axis: V, t: f64) -> f64 {
    section::union_area(&[SectionMesh::new(mesh, origin, axis)], t)
}
const UP: V = [0.0, 0.0, 1.0];

/// One head chain measured from the written records.
#[derive(Debug, Clone)]
pub struct Chain {
    pub site: [f64; 2],
    /// Section where the cone leaves the ball (plinth heads) or the waist (columns).
    pub neck: f64,
    /// Written ball (or column top) section in the plane of the body face.
    pub patch: f64,
    pub column: f64,
    pub flare: f64,
    /// Smallest horizontal union section from above the raft to the ball centre.
    pub scan_min: f64,
    pub scan_min_z: f64,
}

fn group(m: &Model, site: [f64; 2], keep: impl Fn(&SupportRecord) -> bool) -> Vec<&SupportRecord> {
    m.supports
        .iter()
        .flat_map(|s| s.records.iter())
        .filter(|r| keep(r))
        .filter(|r| (f64::from(r.start[0]) - site[0]).hypot(f64::from(r.start[1]) - site[1]) < 1e-3)
        .collect()
}

fn scan(parts: &[&SupportRecord], lo: f64, hi: f64) -> (f64, f64) {
    let meshes: Vec<_> = parts
        .iter()
        .map(|r| SectionMesh::new(&r.mesh, [0.0; 3], UP))
        .collect();
    let m = section::minimum(&meshes, lo, hi, 300);
    (m.area, m.at)
}

fn measure_head(m: &Model, cone: &SupportRecord, face: f64, plate: f64) -> Result<Chain> {
    let ball = m
        .supports
        .iter()
        .flat_map(|s| s.records.iter())
        .find(|r| r.record_type == Ty::TIP_BALL && len(sub(ball_centre(r), v(cone.start))) < 1e-4)
        .ok_or("cone without its ball")?;
    let (start, end) = (v(cone.start), v(cone.end));
    let axis = sub(end, start);
    let t = exit_t(
        f64::from(cone.r1),
        f64::from(cone.r2),
        len(axis),
        f64::from(ball.r1),
    );
    let site = [start[0], start[1]];
    let parts = group(m, site, |r| r.record_type != Ty::RAFT);
    let rod = parts
        .iter()
        .find(|r| r.record_type == Ty::ROD)
        .ok_or("head without column")?;
    let flare = parts
        .iter()
        .find(|r| r.record_type == Ty::BASE_FLARE)
        .ok_or("head without flare")?;
    let (scan_min, scan_min_z) = scan(&parts, plate + 1.3, start[2]);
    let rod_mid = (v(rod.start)[2] + v(rod.end)[2]) / 2.0;
    Ok(Chain {
        site,
        neck: at(&cone.mesh, start, axis, t),
        patch: at(&ball.mesh, [0.0; 3], UP, face),
        column: at(&rod.mesh, [0.0; 3], UP, rod_mid),
        flare: at(&flare.mesh, [0.0; 3], UP, v(flare.start)[2] - 1e-3),
        scan_min,
        scan_min_z,
    })
}

fn measure_column(m: &Model, site: [f64; 2], face: f64, plate: f64) -> Result<Chain> {
    let parts = group(m, site, |r| r.record_type != Ty::RAFT);
    let (scan_min, scan_min_z) = scan(&parts, plate + 1.3, face - 1e-3);
    let rods: Vec<_> = parts.iter().filter(|r| r.record_type == Ty::ROD).collect();
    let pillar = rods
        .iter()
        .min_by(|a, b| v(a.start)[2].total_cmp(&v(b.start)[2]))
        .ok_or("no pillar")?;
    let top = rods
        .iter()
        .max_by(|a, b| v(a.start)[2].total_cmp(&v(b.start)[2]))
        .ok_or("no top")?;
    let flare = parts
        .iter()
        .find(|r| r.record_type == Ty::BASE_FLARE)
        .ok_or("no flare")?;
    Ok(Chain {
        site,
        neck: scan_min,
        patch: at(&top.mesh, [0.0; 3], UP, face - 1e-3),
        column: at(
            &pillar.mesh,
            [0.0; 3],
            UP,
            (v(pillar.start)[2] + v(pillar.end)[2]) / 2.0,
        ),
        flare: at(&flare.mesh, [0.0; 3], UP, v(flare.start)[2] - 1e-3),
        scan_min,
        scan_min_z,
    })
}

/// Head sites in the written model, in a stable order.
pub(crate) fn sites(m: &Model, kind: Kind) -> Vec<[f64; 2]> {
    let mut s: Vec<[f64; 2]> = m
        .supports
        .iter()
        .flat_map(|s| s.records.iter())
        .filter(|r| {
            r.record_type
                == if kind == Kind::Column {
                    Ty::BASE_FLARE
                } else {
                    Ty::TIP_BALL
                }
        })
        .map(|r| [f64::from(r.start[0]), f64::from(r.start[1])])
        .collect();
    s.sort_by(|a, b| a[1].total_cmp(&b[1]).then(a[0].total_cmp(&b[0])));
    s
}

fn layer_areas(m: &Model, grid: Grid) -> Result<Vec<f64>> {
    let layers = goo_grid::slice(&m.mesh, goo_grid::offset(m), grid, Boundary::Centre)?;
    Ok(layers
        .iter()
        .map(|l| {
            l.iter().map(|s| f64::from(s.x1 - s.x0)).sum::<f64>() * grid.pitch[0] * grid.pitch[1]
        })
        .collect())
}

fn stats(a: &[f64]) -> Value {
    json!({"min": a.iter().copied().fold(f64::INFINITY, f64::min),
           "max": a.iter().copied().fold(f64::NEG_INFINITY, f64::max)})
}

/// Measure one written coupon. Errors if a head chain cannot be read or the achieved ratio
/// is off the target by more than the spec tolerance.
///
/// # Errors
/// Unreadable chain, or ratio outside tolerance.
pub fn coupon(spec: &Cal2Spec, item: &Item, m: &Model, grid: Grid) -> Result<Value> {
    let lift = f64::from(m.translate[2]);
    let zb = m
        .mesh
        .iter()
        .flatten()
        .map(|p| f64::from(p[2]))
        .fold(f64::INFINITY, f64::min);
    let plate = zb - lift;
    let loaded = (lift / grid.height).round() as usize + 1;
    let mid = (loaded as f64 - 0.5) * grid.height - lift + zb;
    let body_area = at(&m.mesh, [0.0; 3], UP, mid);
    let before = at(&m.mesh, [0.0; 3], UP, mid - grid.height);
    let (lo, hi) = m.mesh.iter().flatten().fold(
        ([f64::INFINITY; 2], [f64::NEG_INFINITY; 2]),
        |(lo, hi), p| {
            (
                std::array::from_fn(|k| lo[k].min(f64::from(p[k]))),
                std::array::from_fn(|k| hi[k].max(f64::from(p[k]))),
            )
        },
    );
    let chains = sites(m, item.kind)
        .into_iter()
        .map(|s| {
            if item.kind == Kind::Column {
                measure_column(m, s, zb, plate)
            } else {
                let cone = group(m, s, |r| r.record_type == Ty::TIP_CONE)
                    .first()
                    .copied()
                    .ok_or("site without cone")?
                    .clone();
                measure_head(m, &cone, zb, plate)
            }
        })
        .collect::<Result<Vec<_>>>()?;
    let heads = item.grid * item.grid;
    if chains.len() != heads {
        return Err(format!(
            "{}: {} heads written, {heads} designed",
            m.name,
            chains.len()
        )
        .into());
    }
    let sum: f64 = chains.iter().map(|c| c.neck).sum();
    let achieved = body_area / sum;
    let error = achieved / item.ratio - 1.0;
    if error.abs() > spec.ratio_tolerance {
        return Err(format!(
            "{}: achieved ratio {achieved:.2} is {:.2}% off {}",
            m.name,
            error * 100.0,
            item.ratio
        )
        .into());
    }
    let areas = layer_areas(m, grid)?;
    let peel: Vec<f64> = (0..areas.len())
        .map(|k| (areas[k] - if k == 0 { 0.0 } else { areas[k - 1] }).max(0.0))
        .collect();
    let ratios: Vec<f64> = peel.iter().map(|p| p / sum).collect();
    let earlier = ratios[..loaded - 1].iter().copied().fold(0.0, f64::max);
    let later = ratios[loaded..].iter().copied().fold(0.0, f64::max);
    let loaded_ratio = ratios[loaded - 1];
    let col = |f: fn(&Chain) -> f64| chains.iter().map(f).collect::<Vec<_>>();
    let patch_over_neck: Vec<f64> = chains.iter().map(|c| c.patch / c.neck).collect();
    let weakest: f64 = chains.iter().map(|c| c.neck.min(c.patch)).sum();
    let strictly_larger = chains.iter().all(|c| c.column > c.neck && c.flare > c.neck);
    Ok(json!({
        "loaded_layer": loaded,
        "loaded_layer_mid_z_mm": (loaded as f64 - 0.5) * grid.height,
        "body_area_loaded_layer_mm2": body_area,
        "body_area_loaded_layer_raster_mm2": areas[loaded - 1],
        "body_side_mm": [hi[0] - lo[0], hi[1] - lo[1]],
        "model_area_one_layer_before_mm2": before,
        "limiting_sections_mm2": {
            "neck_at_cone_exit": col(|c| c.neck),
            "neck_sum": sum,
            "patch_plane_at_body_face": col(|c| c.patch),
            "column": stats(&col(|c| c.column)),
            "flare_top": stats(&col(|c| c.flare)),
            "scan_min_above_raft": col(|c| c.scan_min),
            "scan_min_z_local_mm": col(|c| c.scan_min_z),
            "neck_design_each": polygon_area(item.neck_mm / 2.0),
        },
        "patch_over_neck": stats(&patch_over_neck),
        "columns_and_flares_strictly_larger_than_neck": strictly_larger,
        "target_ratio": item.ratio,
        "achieved_ratio": achieved,
        "achieved_ratio_error_percent": error * 100.0,
        "within_tolerance": true,
        "achieved_ratio_using_smaller_of_neck_and_patch": body_area / weakest,
        "ratio_by_layer": {
            "layer_peel_area_loaded_mm2": peel[loaded - 1],
            "ratio_at_loaded_layer": loaded_ratio,
            "max_ratio_earlier_layers": earlier,
            "max_ratio_later_layers": later,
            "no_earlier_layer_higher": earlier <= loaded_ratio,
            "no_layer_higher": earlier <= loaded_ratio && later <= loaded_ratio,
        },
        "raster_note": "layer areas: midpoint raster of the written body mesh on the owner grid; peel = positive change from the layer below",
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cal2_head;

    fn spec() -> Cal2Spec {
        serde_json::from_str(include_str!("../../../fixtures/calibration/cal2.json")).unwrap()
    }
    fn written(item: &Item, number: u32) -> Model {
        let s = spec();
        let b = cal2_head::build(&s, item, number).unwrap();
        let project = plinth_chitubox::Project {
            models: vec![b.model],
            ..plinth_chitubox::Project::default()
        };
        plinth_chitubox::parse(&project.to_bytes().unwrap())
            .unwrap()
            .models
            .remove(0)
    }
    fn item(row: usize, neck: f64, grid: usize, ratio: f64) -> Item {
        let kind = if row == 3 { Kind::Column } else { Kind::Head };
        Item {
            row,
            kind,
            neck_mm: neck,
            grid,
            ratio,
            copy: 0,
            copies: 1,
        }
    }

    #[test]
    fn re_read_coupons_hit_target_and_no_other_layer_is_higher() {
        for it in [
            item(2, 0.8, 2, 55.0),
            item(1, 0.55, 3, 30.0),
            item(3, 1.2, 2, 40.0),
        ] {
            let m = written(&it, 5);
            let j = coupon(&spec(), &it, &m, Grid::cal1()).unwrap();
            assert!(
                j["achieved_ratio_error_percent"].as_f64().unwrap().abs() < 1.0,
                "{j}"
            );
            assert_eq!(j["loaded_layer"], 101);
            assert_eq!(j["ratio_by_layer"]["no_layer_higher"], true);
            assert!(j["model_area_one_layer_before_mm2"].as_f64().unwrap() < 1e-9);
            assert_eq!(j["columns_and_flares_strictly_larger_than_neck"], true);
            let pn = j["patch_over_neck"]["max"].as_f64().unwrap();
            if it.kind == Kind::Head {
                // Recorded finding: plinth's own face patch is below the exit neck.
                assert!(pn < 1.0 && pn > 0.85, "{pn}");
            }
        }
    }

    #[test]
    fn measurement_reads_the_mesh_not_the_fields() {
        let it = item(2, 0.8, 2, 55.0);
        let mut m = written(&it, 5);
        // Fatten one written cone by 10% in plan view; leave every field alone.
        let cone = m
            .supports
            .as_mut()
            .unwrap()
            .records
            .iter_mut()
            .find(|r| r.record_type == Ty::TIP_CONE)
            .unwrap();
        let (cx, cy) = (cone.start[0], cone.start[1]);
        for p in cone.mesh.iter_mut().flatten() {
            p[0] = cx + (p[0] - cx) * 1.2;
            p[1] = cy + (p[1] - cy) * 1.2;
        }
        let err = coupon(&spec(), &it, &m, Grid::cal1())
            .unwrap_err()
            .to_string();
        assert!(err.contains("off"), "{err}");
    }

    #[test]
    fn a_wider_layer_above_the_loaded_one_is_reported_not_hidden() {
        let it = item(2, 0.8, 2, 55.0);
        let mut m = written(&it, 5);
        // Add a 20 mm slab on top of the body (raised on top, so a larger layer
        // follows the loaded one only if it overhangs: it does, being wider).
        let top = m
            .mesh
            .iter()
            .flatten()
            .map(|p| p[2])
            .fold(f32::NEG_INFINITY, f32::max);
        let big = crate::cal2_body::body(20.0, 0.5, None);
        for t in big {
            m.mesh
                .push(t.map(|p| [p[0] as f32, p[1] as f32, p[2] as f32 + top]));
        }
        let j = coupon(&spec(), &it, &m, Grid::cal1()).unwrap();
        let later = j["ratio_by_layer"]["max_ratio_later_layers"]
            .as_f64()
            .unwrap();
        let loaded = j["ratio_by_layer"]["ratio_at_loaded_layer"]
            .as_f64()
            .unwrap();
        assert!(later > 0.0, "the overhanging slab must show as later peel");
        assert_eq!(j["ratio_by_layer"]["no_layer_higher"], later <= loaded);
    }
}
