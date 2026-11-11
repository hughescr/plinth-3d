//! Read-only CAL-2 sliced-file checker: measures the ChituBox GOO against the written project
//! and `cal2-report.json`. Writes one JSON file and a few small label crops; changes nothing else.
use plinth_calibration::{
    Result,
    goo::{self, Goo, Span},
    goo_grid::{self, Boundary, Grid},
};
use plinth_chitubox::{Model, Triangle};
use serde_json::{Value, json};
use std::path::Path;

/// Pad around each coupon's written footprint when assigning lit pixels (mm).
const PAD_MM: f64 = 0.3;
const EDGE_MM: f64 = 5.0;
const MAX_LIST: usize = 40;

struct Coupon {
    id: String,
    number: String,
    kind: String,
    rect: [u32; 4],
    /// Written-footprint centre in plate mm.
    centre_mm: [f64; 2],
    report_centre_mm: Option<[f64; 2]>,
    top_mm: f64,
    report_top_mm: Option<f64>,
    predicted: Vec<Vec<Span>>,
    pred_last: usize,
}

#[derive(Default, Clone)]
struct Acc {
    actual_px: Vec<u64>,
    pred_px: Vec<u64>,
    overlap_px: Vec<u64>,
    blobs: Vec<u32>,
    pred_blobs: Vec<u32>,
    islands: Vec<Value>,
    dead_ends: Vec<Value>,
    pred_dead_ends: u32,
    l1_shift_mm: Option<[f64; 2]>,
}

fn bounds(mesh: &[Triangle], b: &mut [[f64; 3]; 2]) {
    for p in mesh.iter().flatten() {
        for i in 0..3 {
            b[0][i] = b[0][i].min(f64::from(p[i]));
            b[1][i] = b[1][i].max(f64::from(p[i]));
        }
    }
}
fn px(mm: f64, axis: usize, grid: Grid) -> f64 {
    f64::from(grid.resolution[axis]) / 2.0 + 0.5 - mm / grid.pitch[axis]
}
fn id_key(name: &str) -> String {
    name.split_whitespace().next().unwrap_or("").to_owned()
}
fn coupon(m: &Model, report: &Value, grid: Grid) -> Coupon {
    let o = goo_grid::offset(m);
    let mut all = [[f64::INFINITY; 3], [f64::NEG_INFINITY; 3]];
    bounds(&m.mesh, &mut all);
    let mut mesh = m.mesh.clone();
    if let Some(s) = &m.supports {
        for r in &s.records {
            bounds(&r.mesh, &mut all);
            mesh.extend_from_slice(&r.mesh);
        }
    }
    let key = id_key(&m.name);
    let rep = report["coupons"].as_array().and_then(|a| {
        a.iter()
            .find(|c| id_key(c["id"].as_str().unwrap_or("")) == key)
    });
    let num = |v: &Value| v.as_f64();
    let report_centre_mm =
        rep.and_then(|c| Some([num(&c["centre_mm"][0])?, num(&c["centre_mm"][1])?]));
    let report_top_mm =
        rep.and_then(|c| num(&c["body_height_mm"]).map(|h| m.translate[2] as f64 + h));
    let pad = PAD_MM;
    let rect = [
        (px(all[1][0] + o[0] + pad, 0, grid).floor().max(0.0)) as u32,
        (px(all[1][1] + o[1] + pad, 1, grid).floor().max(0.0)) as u32,
        (px(all[0][0] + o[0] - pad, 0, grid).ceil()) as u32,
        (px(all[0][1] + o[1] - pad, 1, grid).ceil()) as u32,
    ];
    let predicted = goo_grid::slice(&mesh, o, grid, Boundary::Centre).unwrap_or_default();
    let pred_last = predicted
        .iter()
        .rposition(|l| !l.is_empty())
        .map_or(0, |i| i + 1);
    Coupon {
        id: rep
            .and_then(|c| c["id"].as_str())
            .unwrap_or(&m.name)
            .to_owned(),
        number: rep.map_or(Value::Null, |c| c["number"].clone()).to_string(),
        kind: rep
            .and_then(|c| c["kind"].as_str())
            .unwrap_or("?")
            .to_owned(),
        rect,
        centre_mm: [
            f64::midpoint(all[0][0], all[1][0]) + o[0],
            f64::midpoint(all[0][1], all[1][1]) + o[1],
        ],
        report_centre_mm,
        top_mm: all[1][2] + o[2],
        report_top_mm,
        predicted,
        pred_last,
    }
}
fn count(s: &[Span]) -> u64 {
    s.iter().map(|s| u64::from(s.x1 - s.x0)).sum()
}
fn intersect(a: &[Span], b: &[Span]) -> u64 {
    let (mut i, mut j, mut n) = (0, 0, 0u64);
    while i < a.len() && j < b.len() {
        let (p, q) = (a[i], b[j]);
        if p.y < q.y || (p.y == q.y && p.x1 <= q.x0) {
            i += 1;
        } else if q.y < p.y || (q.y == p.y && q.x1 <= p.x0) {
            j += 1;
        } else {
            n += u64::from(p.x1.min(q.x1) - p.x0.max(q.x0));
            if p.x1 < q.x1 {
                i += 1;
            } else {
                j += 1;
            }
        }
    }
    n
}
/// Pixel-weighted centroid (x, y) in pixels.
fn centroid(s: &[Span]) -> Option<[f64; 2]> {
    let n = count(s);
    if n == 0 {
        return None;
    }
    let (mut cx, mut cy) = (0.0, 0.0);
    for p in s {
        let w = f64::from(p.x1 - p.x0);
        cx += f64::from(p.x0 + p.x1) / 2.0 * w;
        cy += (f64::from(p.y) + 0.5) * w;
    }
    Some([cx / n as f64, cy / n as f64])
}
fn owner(coupons: &[Coupon], c: [f64; 2]) -> Option<usize> {
    coupons.iter().position(|k| {
        c[0] >= f64::from(k.rect[0])
            && c[0] < f64::from(k.rect[2])
            && c[1] >= f64::from(k.rect[1])
            && c[1] < f64::from(k.rect[3])
    })
}
fn mm_of(p: [f64; 2], grid: Grid) -> [f64; 2] {
    [
        (f64::from(grid.resolution[0]) / 2.0 + 0.5 - p[0]) * grid.pitch[0],
        (f64::from(grid.resolution[1]) / 2.0 + 0.5 - p[1]) * grid.pitch[1],
    ]
}

struct Edge {
    min_x: u32,
    max_x: u32,
    min_y: u32,
    max_y: u32,
    band_px: u64,
    band_layers: Vec<usize>,
    outside_px: u64,
    outside_layers: Vec<usize>,
}
struct Scan<'a> {
    goo: &'a Goo,
    grid: Grid,
    coupons: &'a [Coupon],
    rows: Vec<Vec<u8>>,
    band: [u32; 2],
}
impl Scan<'_> {
    /// Split a layer into per-coupon span lists; also count pixels in the edge band and outside
    /// every padded footprint.
    fn split(&self, layer: usize, spans: &[Span], edge: &mut Edge) -> Vec<Vec<Span>> {
        let w = u32::from(self.goo.header.resolution[0]);
        let h = u32::from(self.goo.header.resolution[1]);
        let mut per = vec![Vec::new(); self.coupons.len()];
        let (mut band, mut outside) = (0u64, 0u64);
        for s in spans {
            edge.min_x = edge.min_x.min(s.x0);
            edge.max_x = edge.max_x.max(s.x1);
            edge.min_y = edge.min_y.min(s.y);
            edge.max_y = edge.max_y.max(s.y + 1);
            let n = u64::from(s.x1 - s.x0);
            if s.y < self.band[1] || s.y >= h - self.band[1] {
                band += n;
            } else {
                band += u64::from(self.band[0].saturating_sub(s.x0).min(s.x1 - s.x0))
                    + u64::from(s.x1.saturating_sub((w - self.band[0]).max(s.x0)));
            }
            let mut inside = 0u64;
            for &k in &self.rows[s.y as usize] {
                let r = self.coupons[k as usize].rect;
                let (a, b) = (s.x0.max(r[0]), s.x1.min(r[2]));
                if b > a {
                    inside += u64::from(b - a);
                    per[k as usize].push(Span {
                        y: s.y,
                        x0: a,
                        x1: b,
                        value: s.value,
                    });
                }
            }
            outside += n - inside;
        }
        edge.band_px += band;
        edge.outside_px += outside;
        if band > 0 {
            edge.band_layers.push(layer + 1);
        }
        if outside > 0 {
            edge.outside_layers.push(layer + 1);
        }
        per
    }
}
fn blob_json(b: &goo::Blob, layer: usize, grid: Grid, who: Option<&str>) -> Value {
    let p = grid.pitch;
    json!({"layer":layer,"coupon":who,"pixels":b.pixels,"area_mm2":b.pixels as f64*p[0]*p[1],
        "centre_mm":mm_of(b.centroid,grid),"bounds_px":b.bounds})
}
fn pred_blob_stats(c: &Coupon, i: usize) -> (u32, u32) {
    let empty: Vec<Span> = Vec::new();
    let cur = c.predicted.get(i).unwrap_or(&empty);
    let n = goo::blobs(cur, &empty).len() as u32;
    let next = c.predicted.get(i + 1).unwrap_or(&empty);
    let dead = if i + 1 < c.pred_last {
        goo::blobs(cur, next)
            .iter()
            .filter(|b| b.overlap_below_pixels == 0)
            .count() as u32
    } else {
        0
    };
    (n, dead)
}
fn tally(
    scan: &Scan<'_>,
    i: usize,
    layers: (&[Span], &[Span]),
    per: &[Vec<Span>],
    acc: &mut [Acc],
    unexpected: &mut Vec<Value>,
) {
    let (cur, prev) = layers;
    let grid = scan.grid;
    let coupons = scan.coupons;
    for (k, c) in coupons.iter().enumerate() {
        let empty: Vec<Span> = Vec::new();
        let pred = c.predicted.get(i).unwrap_or(&empty);
        acc[k].actual_px.push(count(&per[k]));
        acc[k].pred_px.push(count(pred));
        acc[k].overlap_px.push(intersect(&per[k], pred));
        let (pb, de) = pred_blob_stats(c, i);
        acc[k].pred_blobs.push(pb);
        acc[k].pred_dead_ends += de;
        acc[k].blobs.push(0);
    }
    // Islands: blobs in this layer with no lit overlap in the layer below (layer 1 rests on the plate).
    for b in goo::blobs(cur, prev) {
        let who = owner(coupons, b.centroid);
        if let Some(k) = who {
            *acc[k].blobs.last_mut().unwrap_or(&mut 0) += 1;
        }
        let name = who.map(|k| coupons[k].id.as_str());
        if who.is_none() && unexpected.len() < MAX_LIST {
            unexpected.push(blob_json(&b, i + 1, grid, None));
        }
        if i > 0
            && b.overlap_below_pixels == 0
            && let Some(k) = who
        {
            acc[k].islands.push(blob_json(&b, i + 1, grid, name));
        } else if i > 0 && b.overlap_below_pixels == 0 && unexpected.len() < MAX_LIST {
            unexpected.push(blob_json(&b, i + 1, grid, None));
        }
    }
    // Dead ends: blobs of the layer below with no lit overlap in this layer, before the coupon's top.
    if i > 0 {
        for b in goo::blobs(prev, cur) {
            if b.overlap_below_pixels > 0 {
                continue;
            }
            if let Some(k) = owner(coupons, b.centroid) {
                let last = last_lit(&acc[k].actual_px).max(coupons[k].pred_last);
                if i < last {
                    acc[k]
                        .dead_ends
                        .push(blob_json(&b, i, grid, Some(&coupons[k].id)));
                }
            }
        }
    }
}
fn last_lit(px: &[u64]) -> usize {
    px.iter().rposition(|&n| n > 0).map_or(0, |i| i + 1)
}
fn first_lit(px: &[u64]) -> usize {
    px.iter().position(|&n| n > 0).map_or(0, |i| i + 1)
}

/// JSON equality that treats 0 and 0.0 (integer vs float spelling) as the same number.
fn same(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::Number(x), Value::Number(y)) => x.as_f64() == y.as_f64(),
        (Value::Array(x), Value::Array(y)) => {
            x.len() == y.len() && x.iter().zip(y).all(|(p, q)| same(p, q))
        }
        _ => a == b,
    }
}
fn header_check(goo: &Goo, cal1: &Value) -> Result<Value> {
    let new = serde_json::to_value(&goo.header)?;
    let old = &cal1["header"];
    let mut diffs = Vec::new();
    let skip = [
        "layers",
        "print_seconds",
        "volume_mm3",
        "weight_g",
        "price",
        "layer_offset",
        "file_time",
    ];
    let mut keys: Vec<&String> = new.as_object().map_or(vec![], |o| o.keys().collect());
    if let Some(o) = old.as_object() {
        keys.extend(o.keys().filter(|k| !new.get(k.as_str()).is_some()));
    }
    for k in keys {
        if skip.contains(&k.as_str()) {
            continue;
        }
        if k == "settings" {
            let (a, b) = (&new["settings"], &old["settings"]);
            let names: std::collections::BTreeSet<_> = a
                .as_object()
                .into_iter()
                .chain(b.as_object())
                .flat_map(|o| o.keys().cloned())
                .collect();
            for n in names {
                if !same(&a[&n], &b[&n]) {
                    diffs.push(json!({"field":format!("settings.{n}"),"cal1":b[&n],"cal2":a[&n]}));
                }
            }
        } else if !same(&new[k], &old[k]) {
            diffs.push(json!({"field":k,"cal1":old[k],"cal2":new[k]}));
        }
    }
    let n = goo.layers.len();
    let bottom = goo.header.bottom_layers as usize;
    let z_err = goo
        .layers
        .iter()
        .enumerate()
        .map(|(i, l)| (f64::from(l.z) - (i + 1) as f64 * f64::from(goo.header.layer_mm)).abs())
        .fold(0.0, f64::max);
    let exposures = |r: std::ops::Range<usize>| -> Vec<f64> {
        let mut v: Vec<f64> = goo.layers[r]
            .iter()
            .map(|l| f64::from(l.exposure))
            .collect();
        v.dedup();
        v
    };
    Ok(json!({
        "pitch_mm":goo.header.pitch(),
        "cal2_header":new,
        "cal1_header_source":"plinth-review/2026-11-02-cal1-plate/slice-check.json",
        "differences_vs_cal1":diffs,
        "layer_count":n,
        "max_layer_z_error_mm":z_err,
        "per_layer_exposure_bottom":exposures(0..bottom.min(n)),
        "per_layer_exposure_normal":exposures(bottom.min(n)..n),
    }))
}
fn crops(goo: &Goo, coupons: &[Coupon], picks: &[usize], dir: &Path) -> Result<Vec<Value>> {
    let mut out = Vec::new();
    for &k in picks {
        let c = &coupons[k];
        let top = c.pred_last;
        let spans = goo.spans(top - 1)?;
        let [x0, y0, x1, y1] = c.rect;
        let (w, h) = ((x1 - x0) as usize, (y1 - y0) as usize);
        let mut img = vec![0u8; w * h];
        for s in spans.iter().filter(|s| s.y >= y0 && s.y < y1) {
            for x in s.x0.max(x0)..s.x1.min(x1) {
                // flip x so plate +X is to the right
                img[(s.y - y0) as usize * w + (w - 1 - (x - x0) as usize)] = s.value.max(1);
            }
        }
        let name = format!("label-{}-layer{top}.pgm", id_key(&c.id));
        let mut file = format!("P5\n{w} {h}\n255\n").into_bytes();
        file.extend_from_slice(&img);
        std::fs::write(dir.join(&name), file)?;
        out.push(json!({"coupon":c.id,"layer":top,"file":name}));
    }
    Ok(out)
}
fn ratio(a: f64, b: f64) -> Value {
    if b > 0.0 { json!(a / b) } else { Value::Null }
}
fn f(v: &Value) -> f64 {
    v.as_f64().unwrap_or(f64::NAN)
}
/// Per-layer area agreement between the sliced file and the raster of the written meshes.
fn area_json(a: &Acc, last: usize, grid: Grid, lift_layer: usize, report: Option<&Value>) -> Value {
    let pix = grid.pitch[0] * grid.pitch[1];
    let n = last.max(1).min(a.actual_px.len());
    let (mut max_rel, mut sum_rel, mut worst) = (0.0f64, 0.0f64, 0usize);
    let (mut min_recall, mut min_precision) = (1.0f64, 1.0f64);
    for i in 0..n {
        let (p, q, o) = (
            a.pred_px[i] as f64,
            a.actual_px[i] as f64,
            a.overlap_px[i] as f64,
        );
        if p > 0.0 {
            let rel = (q - p).abs() / p;
            sum_rel += rel;
            if rel > max_rel {
                (max_rel, worst) = (rel, i + 1);
            }
            min_recall = min_recall.min(o / p);
        }
        if q > 0.0 {
            min_precision = min_precision.min(o / q);
        }
    }
    let peak = |v: &[u64], from: usize| {
        v.iter()
            .skip(from)
            .take(n.saturating_sub(from))
            .copied()
            .max()
            .unwrap_or(0) as f64
            * pix
    };
    json!({
        "max_abs_rel_area_deviation":max_rel,"worst_layer":worst,"mean_abs_rel_area_deviation":sum_rel/n as f64,
        "min_layer_recall":min_recall,"min_layer_precision":min_precision,
        "actual_peak_body_area_mm2":peak(&a.actual_px, lift_layer),"raster_of_written_mesh_peak_body_area_mm2":peak(&a.pred_px, lift_layer),
        "report_raster_peak_area_mm2":report.map(|r|r["raster_peak_area_mm2"].clone()),
        "report_peak_peel_area_mm2":report.map(|r|r["peak_peel_area_mm2"].clone()),
        "actual_vs_report_peak":report.map(|r|ratio(peak(&a.actual_px, lift_layer), f(&r["raster_peak_area_mm2"]))),
    })
}
fn label_json(a: &Acc, last: usize, layers_raised: usize) -> Value {
    let from = last.saturating_sub(layers_raised);
    let (mut min_recall, mut act, mut pred) = (1.0f64, 0u64, 0u64);
    for i in from..last.min(a.actual_px.len()) {
        if a.pred_px[i] > 0 {
            min_recall = min_recall.min(a.overlap_px[i] as f64 / a.pred_px[i] as f64);
        }
        act += a.actual_px[i];
        pred += a.pred_px[i];
    }
    let t = last.saturating_sub(1);
    json!({"raised_layers":format!("{}..{}",from+1,last),"actual_px_sum":act,"predicted_px_sum":pred,
        "min_layer_recall":min_recall,
        "top_layer_blobs_actual":a.blobs.get(t),"top_layer_blobs_predicted":a.pred_blobs.get(t)})
}
struct Ctx<'a> {
    grid: Grid,
    lift_layer: usize,
    raised_layers: usize,
    report: &'a Value,
}
fn coupon_json(c: &Coupon, a: &Acc, ctx: &Ctx<'_>) -> Value {
    let h = ctx.grid.pitch;
    let (first, last) = (first_lit(&a.actual_px), last_lit(&a.actual_px));
    let layer_h = f64::from(ctx.grid.height as f32);
    let top_layer = (c.top_mm / layer_h).round() as i64;
    let report_layer = c.report_top_mm.map(|t| (t / layer_h).round() as i64);
    let empty_between = (first.saturating_sub(1)..last)
        .filter(|&i| a.actual_px[i] == 0)
        .count();
    let blob_mismatch: Vec<usize> = (0..last.min(a.blobs.len()))
        .filter(|&i| a.blobs[i] != a.pred_blobs[i])
        .map(|i| i + 1)
        .collect();
    let rep = ctx.report["coupons"].as_array().and_then(|v| {
        v.iter()
            .find(|r| id_key(r["id"].as_str().unwrap_or("")) == id_key(&c.id))
    });
    let l1 = (a.actual_px[0], a.pred_px[0], a.overlap_px[0]);
    json!({
        "id":c.id,"kind":c.kind,"number":c.number,"rect_px":c.rect,"centre_mm":c.centre_mm,"report_centre_mm":c.report_centre_mm,
        "layer1":{"actual_px":l1.0,"actual_area_mm2":l1.0 as f64*h[0]*h[1],"predicted_px":l1.1,"overlap_px":l1.2,
            "recall":ratio(l1.2 as f64,l1.1 as f64),"precision":ratio(l1.2 as f64,l1.0 as f64),
            "blobs_actual":a.blobs[0],"blobs_predicted":a.pred_blobs[0],"centroid_shift_mm_actual_minus_predicted":a.l1_shift_mm},
        "first_lit_layer":first,"last_lit_layer":last,"expected_last_layer_from_raster":c.pred_last,
        "expected_last_layer_from_project_top":top_layer,"expected_last_layer_from_report":report_layer,
        "last_layer_matches_within_1":(last as i64-top_layer).abs()<=1 && report_layer.is_none_or(|r|(last as i64-r).abs()<=1),
        "empty_layers_between_first_and_last":empty_between,
        "island_count":a.islands.len(),"islands":a.islands,
        "dead_end_blobs_before_top":a.dead_ends.len(),"dead_ends":a.dead_ends.iter().take(8).collect::<Vec<_>>(),
        "predicted_dead_ends_before_top":a.pred_dead_ends,
        "blob_count_mismatch_layers":blob_mismatch.len(),"blob_count_mismatch_first":blob_mismatch.iter().take(8).collect::<Vec<_>>(),
        "blobs_per_layer_actual_min_max":[a.blobs.iter().take(last).min(),a.blobs.iter().take(last).max()],
        "area":area_json(a,last,ctx.grid,ctx.lift_layer,rep),
        "label":(c.number!="null").then(||label_json(a,last,ctx.raised_layers)),
        "blobs_per_layer_actual":a.blobs.iter().take(last).collect::<Vec<_>>(),
        "blobs_per_layer_predicted":a.pred_blobs.iter().take(last).collect::<Vec<_>>(),
        "actual_px_per_layer":a.actual_px.iter().take(last).collect::<Vec<_>>(),
        "predicted_px_per_layer":a.pred_px.iter().take(last).collect::<Vec<_>>(),
    })
}
fn run() -> Result<()> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    let [goo_p, proj_p, rep_p, cal1_p, out_dir, cal1_goo_p] = args.as_slice() else {
        return Err(
            "usage: cal2-goo GOO PROJECT CAL2-REPORT CAL1-SLICE-CHECK OUT_DIR CAL1-GOO".into(),
        );
    };
    let cal1_goo = Goo::parse(&std::fs::read(cal1_goo_p)?)?;
    let first = |g: &Goo| {
        g.layers
            .iter()
            .take(12)
            .map(|l| f64::from(l.exposure))
            .collect::<Vec<_>>()
    };
    let goo = Goo::parse(&std::fs::read(goo_p)?)?;
    let project = plinth_chitubox::parse(&std::fs::read(proj_p)?)?;
    let report: Value = serde_json::from_slice(&std::fs::read(rep_p)?)?;
    let cal1: Value = serde_json::from_slice(&std::fs::read(cal1_p)?)?;
    let out = Path::new(out_dir);
    let grid = Grid::from_header(&goo.header);
    let coupons: Vec<Coupon> = project
        .models
        .iter()
        .map(|m| coupon(m, &report, grid))
        .collect();
    let (acc, edge, unexpected) = scan_all(&goo, &coupons, grid)?;
    let ctx = Ctx {
        grid,
        lift_layer: 100,
        raised_layers: 8,
        report: &report,
    };
    let list: Vec<Value> = coupons
        .iter()
        .zip(&acc)
        .map(|(c, a)| coupon_json(c, a, &ctx))
        .collect();
    let picks = [0usize, 10, 40, 60];
    let crop_dir = out.join("label-crops");
    std::fs::create_dir_all(&crop_dir)?;
    let crop_files = crops(&goo, &coupons, &picks, &crop_dir)?;
    let result = json!({
        "goo":goo_p,"header":header_check(&goo, &cal1)?,"coupon_count":coupons.len(),
        "edge":{"edge_band_mm":EDGE_MM,"lit_px_within_band_all_layers":edge.band_px,"layers_with_band_px":edge.band_layers,
            "lit_bounds_px":[edge.min_x,edge.min_y,edge.max_x,edge.max_y],
            "min_distance_to_edge_mm":{"left_x":f64::from(edge.min_x)*grid.pitch[0],"right_x":(f64::from(goo.header.resolution[0])-f64::from(edge.max_x))*grid.pitch[0],
                "top_y":f64::from(edge.min_y)*grid.pitch[1],"bottom_y":(f64::from(goo.header.resolution[1])-f64::from(edge.max_y))*grid.pitch[1]},
            "image_size_px":goo.header.resolution},
        "lit_px_outside_every_padded_footprint":edge.outside_px,"layers_with_outside_px":edge.outside_layers,
        "unexpected_blobs_first":unexpected,"pad_mm":PAD_MM,
        "exposure_first_12_layers":{"cal1_goo":first(&cal1_goo),"cal2_goo":first(&goo),"equal":first(&cal1_goo)==first(&goo)},
        "label_crops":crop_files,"coupons":list,
    });
    std::fs::write(
        out.join("goo-check.json"),
        serde_json::to_vec_pretty(&result)?,
    )?;
    println!("wrote {}", out.join("goo-check.json").display());
    Ok(())
}
fn scan_all(goo: &Goo, coupons: &[Coupon], grid: Grid) -> Result<(Vec<Acc>, Edge, Vec<Value>)> {
    let h = usize::from(goo.header.resolution[1]);
    let mut rows = vec![Vec::new(); h];
    for (k, c) in coupons.iter().enumerate() {
        for row in rows
            .iter_mut()
            .take(c.rect[3] as usize)
            .skip(c.rect[1] as usize)
        {
            row.push(k as u8);
        }
    }
    let band = [
        (EDGE_MM / grid.pitch[0]).ceil() as u32,
        (EDGE_MM / grid.pitch[1]).ceil() as u32,
    ];
    let scan = Scan {
        goo,
        grid,
        coupons,
        rows,
        band,
    };
    let mut edge = Edge {
        min_x: u32::MAX,
        max_x: 0,
        min_y: u32::MAX,
        max_y: 0,
        band_px: 0,
        band_layers: vec![],
        outside_px: 0,
        outside_layers: vec![],
    };
    let mut acc = vec![Acc::default(); coupons.len()];
    let mut unexpected = Vec::new();
    let mut prev: Vec<Span> = Vec::new();
    for i in 0..goo.layers.len() {
        let cur = goo.spans(i)?;
        let per = scan.split(i, &cur, &mut edge);
        tally(&scan, i, (&cur, &prev), &per, &mut acc, &mut unexpected);
        if i == 0 {
            for (k, c) in coupons.iter().enumerate() {
                if let (Some(a), Some(p)) = (
                    centroid(&per[k]),
                    centroid(c.predicted.first().map_or(&[], |v| v)),
                ) {
                    let (a, p) = (mm_of(a, grid), mm_of(p, grid));
                    acc[k].l1_shift_mm = Some([a[0] - p[0], a[1] - p[1]]);
                }
            }
        }
        prev = cur;
    }
    Ok((acc, edge, unexpected))
}
fn main() {
    if let Err(e) = run() {
        eprintln!("{e}");
        std::process::exit(1);
    }
}
