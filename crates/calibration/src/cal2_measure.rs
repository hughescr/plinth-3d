//! CAL-2 per-coupon measurement, always from a model re-read from the written file.
//! The label ratio of a layer is plinth's own: `load::analyse_full` on the written body's
//! layer stack and the coupon's load view, so the denominator is exactly what the load
//! model takes as capacity (the certified patch lower bound printed by that layer, capped at
//! that bound, and every smaller section of the path). The physical ratio against sections
//! sliced from the written record meshes (the ball plane, the cone exit, the path) is
//! reported beside it under its own names; no design field is ever read back as a section.
use crate::{
    Result, cal2_body,
    cal2_head::{exit_t, polygon_area},
    cal2_spec::{Cal2Spec, Item, Kind, LAYER, LOW_FRACTION, Variant},
    goo_grid::{self, Boundary, Grid},
    section::{self, SectionMesh},
};
use plinth_chitubox::{Model, RecordType as Ty, SupportRecord};
use plinth_geom::{measure::Measure, slice::LayerGrid};
use plinth_support::load::{
    self, ContactView, LayerStack, LayoutView, ModelLayers, PartChain, PartKind, PartSection,
    SiteClass, StackOptions,
};
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

/// What the load model needs from the build of one plinth-headed coupon, in the plate frame
/// and the body's own xy: the load view of its frozen plan, and the certified patch history.
#[derive(Debug, Clone)]
pub struct HeadLoad {
    /// Contacts and the chain of sections of each, as `place::layout_from_contacts` and
    /// `place::add_members` make them. The first layer of every contact is set at analysis.
    pub layout: LayoutView,
    /// Per contact: (plate z of a layer top in mm, certified patch lower bound printed by
    /// then in mm^2), the planner's `ball_patches_below` history capped at the contact's
    /// lower bound.
    pub history: Vec<Vec<(f64, f64)>>,
}

impl HeadLoad {
    /// Contact `i`'s capacity in the first body layer, as the load model takes it: the
    /// patch printed by the top of that layer (the first history entry) or any smaller
    /// section of the path. The patch only grows, so this is the smallest capacity the
    /// contact has in the loaded layers.
    #[must_use]
    pub fn first_layer_capacity(&self, i: usize) -> f64 {
        let first = self.history[i].first().map_or(0.0, |&(_, a)| a);
        self.layout.chains[i]
            .parts
            .iter()
            .map(|p| {
                if p.kind == PartKind::Patch {
                    first
                } else {
                    p.area_lower_mm2
                }
            })
            .fold(f64::INFINITY, f64::min)
    }
}

/// The load model's input for a plain column coupon, which has no plinth contact: each head's
/// patch and its limiting path section, measured from the written records.
fn column_view(chains: &[Chain], face: f64) -> LayoutView {
    let mut layout = LayoutView::default();
    for (i, c) in chains.iter().enumerate() {
        layout.contacts.push(ContactView {
            centre: [c.site[0], c.site[1], face],
            patch_lower_mm2: c.patch,
            neck_area_mm2: c.minimum(),
            first_layer: 0,
            site_class: SiteClass::HiddenUnderside,
        });
        layout.chains.push(PartChain {
            contact: i as u32,
            parts: vec![
                PartSection {
                    kind: PartKind::Patch,
                    record: None,
                    area_lower_mm2: c.patch,
                    z_range_mm: [face, face],
                },
                PartSection {
                    kind: PartKind::Column,
                    record: None,
                    area_lower_mm2: c.minimum(),
                    z_range_mm: [0.0, face],
                },
            ],
        });
    }
    layout
}

/// The load view with its first layers and certified patch history in the stack's grid.
fn head_view(load: &HeadLoad, stack: &LayerStack, face: f64) -> LayoutView {
    let first = (((face - stack.layer_origin_mm) / stack.layer_height_mm) + 1e-6)
        .floor()
        .max(0.0) as u32;
    let mut layout = load.layout.clone();
    for c in &mut layout.contacts {
        c.first_layer = first;
    }
    layout.patch_history = load
        .history
        .iter()
        .map(|h| {
            h.iter()
                .filter_map(|&(z, a)| {
                    let k = ((z - stack.layer_origin_mm) / stack.layer_height_mm).round() - 1.0;
                    (k >= 0.0).then_some((k as u32, a))
                })
                .collect()
        })
        .collect();
    layout
}

/// The model's per-layer ratio: peel area over the summed capacity of the active contacts.
fn layer_ratios(report: &load::LoadReport) -> Vec<f64> {
    report
        .layers
        .iter()
        .map(|l| {
            if l.capacity_mm2 > 0.0 {
                l.area_mm2 * load::DESIGN_RATIO / l.capacity_mm2
            } else if l.area_mm2 > 0.0 {
                f64::INFINITY
            } else {
                0.0
            }
        })
        .collect()
}

/// plinth's layer stack of the written body, in the plate frame (the body's bottom face
/// at `lift`). The grid starts at the body's lowest point and must hold whole layers.
fn layers_of(m: &Model, shift: f64) -> Result<ModelLayers> {
    let tris: Vec<cal2_body::Tri> = m
        .mesh
        .iter()
        .map(|t| t.map(|p| [f64::from(p[0]), f64::from(p[1]), f64::from(p[2]) + shift]))
        .collect();
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
    match load::build_model_layers(&mesh, &grid, &StackOptions::default()) {
        Measure::Known(v) => Ok(v),
        Measure::Unverified(why) => Err(format!("the body's layer stack: {why:?}").into()),
    }
}

/// The slab envelope area of every body layer, from plinth's own layer stack.
fn envelopes(ml: &ModelLayers) -> Vec<f64> {
    ml.stack.layers.iter().map(|l| l.model_envelope).collect()
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

/// The load model's view of one written coupon.
struct Modelled {
    /// The slab envelope of every body layer, mm^2.
    peel: Vec<f64>,
    /// The model's ratio of every body layer.
    ratio: Vec<f64>,
    /// Per contact: the smallest section on its chain with the whole certified patch, mm^2.
    capacity: Vec<f64>,
    /// Per contact: the capacity in the first loaded layer, mm^2.
    first_layer: Vec<f64>,
    /// Per contact: the certified patch lower bound, mm^2.
    certified: Vec<f64>,
    /// The model's per-contact sizing ratio: its layer (0-based) and value.
    sizing: (u32, f64),
}

/// Run `load::analyse_full` on the written body's layer stack and the coupon's load view.
/// `floor` is the lowest body point and the plate height below it, in the model's frame.
fn model_coupon(
    m: &Model,
    item: &Item,
    load: Option<&HeadLoad>,
    chains: &[Chain],
    floor: (f64, f64),
) -> Result<Modelled> {
    let (zb, plate) = floor;
    let shift = -plate;
    let ml = layers_of(m, shift)?;
    let peel = envelopes(&ml);
    let face = zb + shift;
    let layout = match (item.kind, load) {
        (Kind::Column, _) => column_view(chains, face),
        (_, Some(l)) => head_view(l, &ml.stack, face),
        (_, None) => return Err(format!("{}: a head coupon without its load view", m.name).into()),
    };
    if layout.contacts.len() != item.heads {
        return Err(format!(
            "{}: the load view has {} contacts, {} designed",
            m.name,
            layout.contacts.len(),
            item.heads
        )
        .into());
    }
    let detail = load::analyse_full(&ml.stack, &layout, &load::Extras::default());
    if !detail.report.is_computed() || detail.report.layers.len() != peel.len() {
        return Err(format!("{}: the load model did not compute this coupon", m.name).into());
    }
    // The label ratio of each layer: the load model's own, peel over the active capacity.
    let ratio = layer_ratios(&detail.report);
    if ratio.iter().any(|x| !x.is_finite()) {
        return Err(format!("{}: a body layer has no load capacity", m.name).into());
    }
    let capacity: Vec<f64> = layout
        .chains
        .iter()
        .take(layout.contacts.len())
        .map(|c| {
            c.parts
                .iter()
                .map(|p| p.area_lower_mm2)
                .fold(f64::INFINITY, f64::min)
        })
        .collect();
    let first_layer = match (item.kind, load) {
        (Kind::Column, _) | (_, None) => capacity.clone(),
        (_, Some(l)) => (0..capacity.len())
            .map(|i| l.first_layer_capacity(i))
            .collect(),
    };
    let sizing = detail
        .contacts
        .iter()
        .map(|c| (c.worst_layer, c.ratio))
        .fold((0, 0.0), |a, b| if b.1 > a.1 { b } else { a });
    Ok(Modelled {
        peel,
        ratio,
        capacity,
        first_layer,
        certified: layout.contacts.iter().map(|c| c.patch_lower_mm2).collect(),
        sizing,
    })
}

/// Measure one written coupon. Errors if a head chain cannot be read, a layer is over the
/// target ratio, the achieved peak is off the target by more than the spec tolerance, or a
/// variant's layer profile is not the designed one. The ratio of every layer is the load
/// model's, from `load` (the coupon's load view; none for a plain column, whose view is
/// made from its measured sections).
///
/// # Errors
/// Unreadable chain, ratio outside tolerance, or a profile that breaks its rules.
pub fn coupon(
    spec: &Cal2Spec,
    item: &Item,
    m: &Model,
    grid: Grid,
    load: Option<&HeadLoad>,
) -> Result<Value> {
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
    let Modelled {
        peel,
        ratio,
        capacity,
        first_layer,
        certified,
        sizing: (sizing_layer, sizing_peak),
    } = model_coupon(m, item, load, &chains, (zb, plate))?;
    // The model credits a certified section; the written meshes must carry at least that.
    let credited: f64 = capacity.iter().sum();
    if sum_min < credited * (1.0 - 1e-3) {
        return Err(format!(
            "{}: the written sections ({sum_min:.4} mm2) are under the capacity the load model credits ({credited:.4} mm2)",
            m.name
        )
        .into());
    }
    // The physical ratios, from sections sliced out of the written record meshes.
    let ratio_mesh: Vec<f64> = peel.iter().map(|p| p / sum_min).collect();
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
        "load_model_capacity_mm2": {
            "per_contact": capacity,
            "sum": credited,
            "first_loaded_layer_per_contact": first_layer,
            "first_loaded_layer_sum": first_layer.iter().sum::<f64>(),
            "certified_patch_lower_per_contact": certified,
            "note": "what plinth's load model divides by: the smallest of the certified patch printed by the layer and every section of the path (analyse.rs Ctx::capacity). per_contact is once the whole certified lower bound has printed; the first loaded layers have a patch about 0.2 percent smaller",
        },
        "limiting_sections_mm2": {
            "mesh_minimum_per_contact": chains.iter().map(Chain::minimum).collect::<Vec<_>>(),
            "mesh_minimum_sum": sum_min,
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
        "ratio_definition": "layer peel area (plinth slab envelope) / sum over the active contacts of plinth's load-model capacity (load::analyse_full: the certified patch lower bound printed by that layer, capped at it, or any smaller section of the path)",
        "target_ratio": item.ratio,
        "achieved_ratio": peak,
        "achieved_ratio_model": peak,
        "achieved_ratio_mesh_section": ratio_mesh.iter().copied().fold(0.0, f64::max),
        "achieved_ratio_cone_exit": ratio_neck.iter().copied().fold(0.0, f64::max),
        "model_contact_sizing_ratio_peak": sizing_peak,
        "model_contact_sizing_peak_layer_in_body": sizing_layer + 1,
        "model_contact_sizing_note": "the load model's per-contact sizing ratio (rigid-carrier or additive, with upstream self-peel); information only, not the label",
        "achieved_ratio_error_percent": error * 100.0,
        "within_tolerance": true,
        "ratio_by_layer": {
            "peak_layer_in_body": peak_layer + 1,
            "ratio_at_first_layer": ratio[0],
            "layers_held_at_target": held,
            "no_layer_over_target": true,
            "model_per_body_layer": ratio.iter().map(round).collect::<Vec<_>>(),
            "mesh_section_per_body_layer": ratio_mesh.iter().map(round).collect::<Vec<_>>(),
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
    /// The coupon built, written and re-read, with its load view.
    fn written(item: &Item, number: u32) -> (Model, Option<HeadLoad>) {
        let s = spec();
        let b = cal2_head::build(&s, item, number).unwrap();
        let project = plinth_chitubox::Project {
            models: vec![b.model],
            ..plinth_chitubox::Project::default()
        };
        let m = plinth_chitubox::parse(&project.to_bytes().unwrap())
            .unwrap()
            .models
            .remove(0);
        (m, b.load)
    }
    fn measure(it: &Item, m: &Model, load: Option<&HeadLoad>) -> Result<Value> {
        coupon(&spec(), it, m, Grid::cal1(), load)
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
            let (m, load) = written(&it, 5);
            let j = measure(&it, &m, load.as_ref()).unwrap();
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
                let mesh = j["achieved_ratio_mesh_section"].as_f64().unwrap();
                let model = j["achieved_ratio_model"].as_f64().unwrap();
                // The certified patch is under the written ball's section, which is under
                // the cone exit's.
                assert!(
                    cone < mesh && mesh < model && cone / model > 0.88,
                    "{cone} {mesh} {model}"
                );
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
            let (m, load) = written(&it, 5);
            let j = measure(&it, &m, load.as_ref()).unwrap();
            assert_eq!(j["ratio_by_layer"]["layers_held_at_target"], held);
            assert_eq!(j["body_layers"], body);
            let r: Vec<f64> = j["ratio_by_layer"]["model_per_body_layer"]
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

    fn witness() -> Item {
        Item {
            kind: Kind::Witness,
            ..item(Variant::Standard, 0.55, 1, crate::cal2_spec::WITNESS_RATIO)
        }
    }

    /// The label is plinth's load-model ratio, not a mesh section's. For one coupon of every
    /// variant: no layer is above the label; the peak equals the peak peel area over the sum
    /// of `Contact::patch().lower()` (the certified patch the load model caps at) within 0.5
    /// percent; and every layer once the patch has printed is the physical mesh-section ratio
    /// scaled by the certified-to-mesh section, within 0.5 percent. The mesh-section ratio is
    /// a separate field and is visibly lower for heads (the review finding: about 3 percent).
    #[test]
    fn reported_ratio_is_the_load_models_for_every_variant() {
        let s = spec();
        let cases = [
            item(Variant::Standard, 0.40, 16, 25.0),
            item(Variant::Standard, 0.55, 9, 40.0),
            item(Variant::Standard, 0.80, 4, 70.0),
            item(Variant::Standard, 1.20, 4, 40.0),
            item(Variant::Strip, 0.55, 8, 40.0),
            item(Variant::Ring, 0.55, 12, 50.0),
            item(Variant::ShortHold, 0.55, 9, 40.0),
            item(Variant::LongHold, 0.55, 9, 50.0),
            item(Variant::LateLoad, 0.55, 9, 40.0),
            witness(),
        ];
        for it in cases {
            let b = cal2_head::build(&s, &it, 5).unwrap();
            let lower: f64 = b.patch.iter().map(|p| p[0]).sum();
            let (m, load) = written(&it, 5);
            let j = measure(&it, &m, load.as_ref()).unwrap();
            let name = format!(
                "{:?} {} x{} R{}",
                it.variant, it.neck_mm, it.heads, it.ratio
            );
            let model = j["achieved_ratio_model"].as_f64().unwrap();
            let mesh = j["achieved_ratio_mesh_section"].as_f64().unwrap();
            let sum_mesh = j["limiting_sections_mm2"]["mesh_minimum_sum"]
                .as_f64()
                .unwrap();
            let layers = |key: &str| -> Vec<f64> {
                j["ratio_by_layer"][key]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|x| x.as_f64().unwrap())
                    .collect()
            };
            let (by_model, by_mesh) = (
                layers("model_per_body_layer"),
                layers("mesh_section_per_body_layer"),
            );
            // No layer is above the label, and the headline is the peak of the layers.
            assert!(
                by_model.iter().all(|&x| x <= it.ratio * (1.0 + 1e-4)),
                "{name}"
            );
            let peak = by_model.iter().copied().fold(0.0, f64::max);
            assert!((peak - model).abs() < 1e-3, "{name}: {peak} {model}");
            // The load model's own per-contact sizing ratio (what the planner tests against
            // the design ratio) is the label too, wherever every head carries an equal pad.
            // Late load is the exception: its pads are separate islands of unequal area until
            // the ramp ends, so the centre head's own ratio is above the average label.
            let sizing = j["model_contact_sizing_ratio_peak"].as_f64().unwrap();
            if it.variant == Variant::LateLoad {
                assert!(sizing > model && sizing < model * 1.06, "{name}: {sizing}");
            } else {
                assert!(
                    (sizing / model - 1.0).abs() < 5e-3,
                    "{name}: {sizing} {model}"
                );
            }
            if it.kind == Kind::Column {
                assert!(
                    b.patch.is_empty() && (model / mesh - 1.0).abs() < 5e-3,
                    "{name}"
                );
                continue;
            }
            // Independent denominator: the certified patch lower bound of the contacts.
            let want = j["peak_peel_area_mm2"].as_f64().unwrap() / lower;
            assert!(
                (model / want - 1.0).abs() < 5e-3,
                "{name}: model {model} certified {want}"
            );
            // The mesh section is visibly larger, so the label and it differ by the finding.
            assert!(mesh < model * 0.99, "{name}: mesh {mesh} model {model}");
            // Every layer after the patch has printed (layer 4 on) is the mesh ratio times
            // mesh-section / certified-patch.
            let scale = sum_mesh / lower;
            for (k, (&a, &z)) in by_model.iter().zip(&by_mesh).enumerate().skip(4) {
                if z > 1.0 {
                    assert!(
                        (a / (z * scale) - 1.0).abs() < 5e-3,
                        "{name} layer {k}: {a} {z}"
                    );
                }
            }
        }
    }

    #[test]
    fn strip_and_ring_hold_their_ratio() {
        for variant in [Variant::Strip, Variant::Ring] {
            let heads = if variant == Variant::Strip { 8 } else { 12 };
            let it = item(variant, 0.55, heads, 40.0);
            let (m, load) = written(&it, 5);
            let j = measure(&it, &m, load.as_ref()).unwrap();
            assert_eq!(j["ratio_by_layer"]["layers_held_at_target"], 40);
        }
    }

    #[test]
    fn measurement_reads_the_mesh_not_the_fields() {
        let it = item(Variant::Standard, 0.8, 4, 35.0);
        let (mut m, load) = written(&it, 5);
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
        assert!(measure(&it, &m, load.as_ref()).is_ok());
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
        // The load model credits the certified patch; a written ball under it is refused.
        let err = measure(&it, &m, load.as_ref()).unwrap_err().to_string();
        assert!(err.contains("under the capacity"), "{err}");
    }

    #[test]
    fn a_wider_layer_above_the_loaded_one_is_an_error_not_hidden() {
        let it = item(Variant::Standard, 0.8, 4, 35.0);
        let (mut m, load) = written(&it, 5);
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
        let err = measure(&it, &m, load.as_ref()).unwrap_err().to_string();
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
        let (m, _) = written(&it, 5);
        let env = envelopes(&layers_of(&m, 0.0).unwrap());
        for (l, e) in env.iter().take(80).enumerate() {
            let want = cal2_shape::layer_area(&plan, l);
            assert!((e - want).abs() < 1e-3 * want, "layer {l}: {e} vs {want}");
        }
    }
}
