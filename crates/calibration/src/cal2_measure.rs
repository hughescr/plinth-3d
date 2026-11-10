//! CAL-2 per-coupon measurement, always from a model re-read from the written file.
//! Head sections are sliced from the written record meshes, never taken from the design
//! fields. The peel area of a body layer is plinth's own: the slab envelope of that layer
//! (`plinth_geom::slice`, the "peel area convention" of the load model), and the section of
//! a contact is its minimum along the path (the face patch, the cone exit or the column),
//! as the load model's capacity takes it.
use crate::{
    Result, cal2_body,
    cal2_head::{exit_t, polygon_area},
    cal2_spec::{Cal2Spec, Item, Kind, LAYER, LOW_FRACTION, Variant},
    goo_grid::{self, Boundary, Grid},
    section::{self, SectionMesh},
};
use plinth_chitubox::{Model, RecordType as Ty, SupportRecord};
use plinth_geom::slice::{LayerGrid, slice_layers_par};
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

/// Slack for float noise when a ratio is compared with its target.
const SLACK: f64 = 1e-4;
/// A layer counts as held at the target when its ratio is at least this fraction of it.
const HELD: f64 = 0.98;

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

impl Chain {
    /// The contact's limiting section as plinth's load model takes it: the smallest of the
    /// printed patch and every section of the path.
    #[must_use]
    pub fn minimum(&self) -> f64 {
        self.patch.min(self.neck).min(self.scan_min)
    }
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

/// Lowest body point and the plate height below it.
fn body_floor(m: &Model) -> (f64, f64) {
    let lift = f64::from(m.translate[2]);
    let zb = m
        .mesh
        .iter()
        .flatten()
        .map(|p| f64::from(p[2]))
        .fold(f64::INFINITY, f64::min);
    (zb, zb - lift)
}

fn chain_at(m: &Model, kind: Kind, site: [f64; 2], zb: f64, plate: f64) -> Result<Chain> {
    if kind == Kind::Column {
        measure_column(m, site, zb, plate)
    } else {
        let cone = group(m, site, |r| r.record_type == Ty::TIP_CONE)
            .first()
            .copied()
            .ok_or("site without cone")?
            .clone();
        measure_head(m, &cone, zb, plate)
    }
}

/// The first head chain of a model, for sizing a body before it exists.
///
/// # Errors
/// A head chain that cannot be read.
pub fn first_chain(m: &Model, kind: Kind) -> Result<Chain> {
    let (zb, plate) = body_floor(m);
    let site = *sites(m, kind).first().ok_or("model without a head")?;
    chain_at(m, kind, site, zb, plate)
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

/// The slab envelope area of every body layer, from plinth's own slicer on the written body.
fn envelopes(m: &Model) -> Result<Vec<f64>> {
    let tris: Vec<cal2_body::Tri> = m.mesh.iter().map(|t| t.map(|p| p.map(f64::from))).collect();
    let mesh = cal2_body::mesh(&tris)?;
    let b = mesh.bounds();
    let layers = ((b.max[2] - b.min[2]) / LAYER).round();
    if ((b.max[2] - b.min[2]) / LAYER - layers).abs() > 0.02 || layers < 1.0 {
        return Err("body height is not a whole number of layers".into());
    }
    let grid = LayerGrid {
        height: LAYER,
        origin: b.min[2],
        layers: layers as u32,
    };
    let slabs = slice_layers_par(&mesh, &grid, 1)
        .known()
        .ok_or("the body could not be sliced")?
        .iter()
        .map(|s| s.envelope.area)
        .collect();
    Ok(slabs)
}

fn stats(a: &[f64]) -> Value {
    json!({"min": a.iter().copied().fold(f64::INFINITY, f64::min),
           "max": a.iter().copied().fold(f64::NEG_INFINITY, f64::max)})
}

/// Layers that must sit at the target for each variant (the hold).
fn expected_held(spec: &Cal2Spec, item: &Item) -> usize {
    use crate::cal2_spec::{LATE_HOLD_LAYERS, LONG_FULL_LAYERS, SHORT_FULL_LAYERS};
    match item.variant {
        Variant::Standard | Variant::Strip | Variant::Ring => spec.hold_layers,
        Variant::ShortHold => SHORT_FULL_LAYERS,
        Variant::LongHold => LONG_FULL_LAYERS,
        // The last ramp layer (98.5 percent) and the hold.
        Variant::LateLoad => LATE_HOLD_LAYERS + 1,
    }
}

/// Rules each variant's ratio profile must obey, as errors.
fn check_profile(spec: &Cal2Spec, item: &Item, name: &str, r: &[f64]) -> Result<usize> {
    let target = item.ratio;
    let held = r.iter().filter(|&&x| x >= HELD * target).count();
    let bad = |what: String| -> Result<usize> { Err(format!("{name}: {what}").into()) };
    if held != expected_held(spec, item) {
        return bad(format!(
            "{held} layers at the target, {} designed",
            expected_held(spec, item)
        ));
    }
    let low = LOW_FRACTION * target * (1.0 + SLACK);
    match item.variant {
        Variant::ShortHold => {
            let full = crate::cal2_spec::SHORT_FULL_LAYERS;
            if r[..full].iter().any(|&x| x < HELD * target) {
                return bad("a full layer is below the target".into());
            }
            // Layers above the stem's own: the stem, then the raised tab and number.
            if r[full..].iter().any(|&x| x > low) {
                return bad("a layer above the hold is over 40 percent of the target".into());
            }
        }
        Variant::LateLoad => {
            let ramp = crate::cal2_spec::RAMP_LAYERS;
            if r[0] > low {
                return bad("the first layer is over 40 percent of the target".into());
            }
            if r[..ramp].windows(2).any(|w| w[1] <= w[0]) {
                return bad("the ramp is not rising".into());
            }
            if r[0] > r[ramp] * 0.5 {
                return bad("the load did not start low".into());
            }
        }
        // The loaded layer, right above the heads, is already at the target.
        _ if r[0] < HELD * target => return bad("the first layer is under the target".into()),
        _ => {}
    }
    Ok(held)
}

/// Measure one written coupon. Errors if a head chain cannot be read, a layer is over the
/// target ratio, the achieved peak is off the target by more than the spec tolerance, or a
/// variant's layer profile is not the designed one.
///
/// # Errors
/// Unreadable chain, ratio outside tolerance, or a profile that breaks its rules.
pub fn coupon(spec: &Cal2Spec, item: &Item, m: &Model, grid: Grid) -> Result<Value> {
    let lift = f64::from(m.translate[2]);
    let (zb, plate) = body_floor(m);
    let loaded = (lift / grid.height).round() as usize + 1;
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
        .map(|s| chain_at(m, item.kind, s, zb, plate))
        .collect::<Result<Vec<_>>>()?;
    if chains.len() != item.heads {
        return Err(format!(
            "{}: {} heads written, {} designed",
            m.name,
            chains.len(),
            item.heads
        )
        .into());
    }
    let sum_min: f64 = chains.iter().map(Chain::minimum).sum();
    let sum_neck: f64 = chains.iter().map(|c| c.neck).sum();
    let peel = envelopes(m)?;
    let ratio: Vec<f64> = peel.iter().map(|p| p / sum_min).collect();
    let ratio_neck: Vec<f64> = peel.iter().map(|p| p / sum_neck).collect();
    let (peak_layer, peak) = ratio
        .iter()
        .copied()
        .enumerate()
        .fold((0, 0.0), |a, (i, x)| if x > a.1 { (i, x) } else { a });
    let error = peak / item.ratio - 1.0;
    if error > SLACK || error < -spec.ratio_tolerance {
        return Err(format!(
            "{}: peak layer ratio {peak:.2} is {:.2}% off {} (over the target or past the tolerance)",
            m.name,
            error * 100.0,
            item.ratio
        )
        .into());
    }
    let held = check_profile(spec, item, &m.name, &ratio)?;
    let raster = layer_areas(m, grid)?;
    let raster_peak = raster[loaded - 1..].iter().copied().fold(0.0, f64::max);
    let col = |f: fn(&Chain) -> f64| chains.iter().map(f).collect::<Vec<_>>();
    let patch_over_neck: Vec<f64> = chains.iter().map(|c| c.patch / c.neck).collect();
    let strictly_larger = chains.iter().all(|c| c.column > c.neck && c.flare > c.neck);
    let round = |x: &f64| (x * 1e4).round() / 1e4;
    let body_layers = peel.len();
    Ok(json!({
        "loaded_layer": loaded,
        "loaded_layer_mid_z_mm": (loaded as f64 - 0.5) * grid.height,
        "body_layers": body_layers,
        "body_height_mm": body_layers as f64 * LAYER,
        "body_bbox_mm": [hi[0] - lo[0], hi[1] - lo[1]],
        "peak_peel_area_mm2": peel[peak_layer],
        "limiting_sections_mm2": {
            "minimum_per_contact": chains.iter().map(Chain::minimum).collect::<Vec<_>>(),
            "minimum_sum": sum_min,
            "neck_at_cone_exit": col(|c| c.neck),
            "neck_sum": sum_neck,
            "patch_plane_at_body_face": col(|c| c.patch),
            "column": stats(&col(|c| c.column)),
            "flare_top": stats(&col(|c| c.flare)),
            "scan_min_above_raft": col(|c| c.scan_min),
            "scan_min_z_local_mm": col(|c| c.scan_min_z),
            "neck_design_each": polygon_area(item.neck_mm / 2.0),
        },
        "patch_over_neck": stats(&patch_over_neck),
        "columns_and_flares_strictly_larger_than_neck": strictly_larger,
        "ratio_definition": "layer peel area (plinth slab envelope) / sum over contacts of the minimum section on the path",
        "target_ratio": item.ratio,
        "achieved_ratio": peak,
        "achieved_ratio_min_section": peak,
        "achieved_ratio_cone_exit": ratio_neck.iter().copied().fold(0.0, f64::max),
        "achieved_ratio_error_percent": error * 100.0,
        "within_tolerance": true,
        "ratio_by_layer": {
            "peak_layer_in_body": peak_layer + 1,
            "ratio_at_first_layer": ratio[0],
            "layers_held_at_target": held,
            "no_layer_over_target": true,
            "min_section_per_body_layer": ratio.iter().map(round).collect::<Vec<_>>(),
            "cone_exit_per_body_layer": ratio_neck.iter().map(round).collect::<Vec<_>>(),
        },
        "raster_peak_area_mm2": raster_peak,
        "raster_note": "raster: midpoint raster of the written body mesh on the owner grid, information only; the ratio uses plinth's vector slab envelope",
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{cal2_head, cal2_shape};

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
    fn item(variant: Variant, neck: f64, heads: usize, ratio: f64) -> Item {
        let kind = if neck > 1.0 { Kind::Column } else { Kind::Head };
        Item {
            row: 0,
            priority: 1,
            kind,
            variant,
            neck_mm: neck,
            heads,
            ratio,
            copy: 0,
            copies: 1,
        }
    }

    #[test]
    fn re_read_coupons_hit_target_and_no_layer_is_over_it() {
        for it in [
            item(Variant::Standard, 0.8, 4, 35.0),
            item(Variant::Standard, 0.55, 9, 25.0),
            item(Variant::Standard, 1.2, 4, 40.0),
        ] {
            let m = written(&it, 5);
            let j = coupon(&spec(), &it, &m, Grid::cal1()).unwrap();
            let e = j["achieved_ratio_error_percent"].as_f64().unwrap();
            assert!((-1.0..=0.0).contains(&e), "{j}");
            assert_eq!(j["loaded_layer"], 101);
            assert_eq!(j["ratio_by_layer"]["layers_held_at_target"], 40);
            assert_eq!(j["body_layers"], 48);
            assert_eq!(j["columns_and_flares_strictly_larger_than_neck"], true);
            let pn = j["patch_over_neck"]["max"].as_f64().unwrap();
            if it.kind == Kind::Head {
                // Recorded finding: plinth's own face patch is below the exit neck.
                assert!(pn < 1.0 && pn > 0.85, "{pn}");
                let cone = j["achieved_ratio_cone_exit"].as_f64().unwrap();
                let min = j["achieved_ratio_min_section"].as_f64().unwrap();
                assert!(cone < min && cone / min > 0.90, "{cone} {min}");
            }
        }
    }

    #[test]
    fn fatigue_and_cure_age_profiles_are_what_they_say() {
        for (variant, held, body) in [
            (Variant::ShortHold, 5, 48),
            (Variant::LongHold, 100, 108),
            (Variant::LateLoad, 41, 88),
        ] {
            let it = item(variant, 0.55, 9, 50.0);
            let m = written(&it, 5);
            let j = coupon(&spec(), &it, &m, Grid::cal1()).unwrap();
            assert_eq!(j["ratio_by_layer"]["layers_held_at_target"], held);
            assert_eq!(j["body_layers"], body);
            let r: Vec<f64> = j["ratio_by_layer"]["min_section_per_body_layer"]
                .as_array()
                .unwrap()
                .iter()
                .map(|x| x.as_f64().unwrap())
                .collect();
            assert!(r.iter().all(|&x| x <= 50.0));
            match variant {
                Variant::ShortHold => assert!(r[5..].iter().all(|&x| x <= 20.0 + 1e-3)),
                Variant::LateLoad => {
                    assert!(r[0] <= 20.0 + 1e-3);
                    assert!(r[..40].windows(2).all(|w| w[1] > w[0]));
                }
                _ => {}
            }
        }
    }

    #[test]
    fn strip_and_ring_hold_their_ratio() {
        for variant in [Variant::Strip, Variant::Ring] {
            let heads = if variant == Variant::Strip { 8 } else { 12 };
            let it = item(variant, 0.55, heads, 40.0);
            let m = written(&it, 5);
            let j = coupon(&spec(), &it, &m, Grid::cal1()).unwrap();
            assert_eq!(j["ratio_by_layer"]["layers_held_at_target"], 40);
        }
    }

    #[test]
    fn measurement_reads_the_mesh_not_the_fields() {
        let it = item(Variant::Standard, 0.8, 4, 35.0);
        let mut m = written(&it, 5);
        // Fatten one written cone by 20% in plan view; leave every field alone.
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
        // The patch (ball face) is still the minimum, so the ratio does not move: the
        // fattened cone cannot hide a thin face. Thin the ball instead.
        assert!(coupon(&spec(), &it, &m, Grid::cal1()).is_ok());
        let ball = m
            .supports
            .as_mut()
            .unwrap()
            .records
            .iter_mut()
            .find(|r| r.record_type == Ty::TIP_BALL)
            .unwrap();
        let (bx, by) = (ball.start[0], ball.start[1]);
        for p in ball.mesh.iter_mut().flatten() {
            p[0] = bx + (p[0] - bx) * 0.8;
            p[1] = by + (p[1] - by) * 0.8;
        }
        let err = coupon(&spec(), &it, &m, Grid::cal1())
            .unwrap_err()
            .to_string();
        assert!(err.contains("off"), "{err}");
    }

    #[test]
    fn a_wider_layer_above_the_loaded_one_is_an_error_not_hidden() {
        let it = item(Variant::Standard, 0.8, 4, 35.0);
        let mut m = written(&it, 5);
        // Add a 20 mm slab on top of the body: a layer far over the target.
        let top = m
            .mesh
            .iter()
            .flatten()
            .map(|p| p[2])
            .fold(f32::NEG_INFINITY, f32::max);
        let part = cal2_body::Part {
            shape: cal2_body::Shape::Rect {
                lo: [-10.0; 2],
                hi: [10.0; 2],
            },
            z0: 0.0,
            z1: 0.5,
        };
        for t in cal2_body::body(&[part], None) {
            m.mesh
                .push(t.map(|p| [p[0] as f32, p[1] as f32, p[2] as f32 + top]));
        }
        let err = coupon(&spec(), &it, &m, Grid::cal1())
            .unwrap_err()
            .to_string();
        assert!(err.contains("off"), "{err}");
    }

    #[test]
    fn designed_layer_areas_match_the_measured_envelopes() {
        // The shape module's own area per layer equals plinth's slab envelope, so the
        // construction and the measurement agree layer by layer.
        let it = item(Variant::LateLoad, 0.55, 9, 40.0);
        let s = spec();
        let b = cal2_head::build(&s, &it, 5).unwrap();
        let plan = cal2_shape::plan(&it, s.hold_layers, b.full_area).unwrap();
        let m = written(&it, 5);
        let env = envelopes(&m).unwrap();
        for (l, e) in env.iter().take(80).enumerate() {
            let want = cal2_shape::layer_area(&plan, l);
            assert!((e - want).abs() < 1e-3 * want, "layer {l}: {e} vs {want}");
        }
    }
}
