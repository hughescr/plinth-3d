//! One side of a comparison: everything `compare` needs from one `.chitubox`,
//! measured from the emitted bytes. The model AND the supports are rasterised
//! from the EMITTED triangle meshes (never from the header primitives, which is
//! the prototype's known defect), and each record's mesh is checked against its
//! own header (Tier 0), so a mesh that has drifted from its header cannot hide.

use super::band::{self, Comp};
use super::consts as k;
use super::log::{self, LogFacts, PoseSpec};
use crate::check::{CheckOutcome, Failure};
use crate::checks;
use crate::context::{AuditContext, AuditOptions, ModelContext};
use crate::geometry::{self, report as georeport};
use crate::json::Json;
use crate::mesh::Mesh;
use crate::primitives::Primitive;
use crate::raster::{Budget, RasterGrid, RunRaster};
use crate::reader::{ChituboxFile, RecordKind, Soup};
use crate::synth::{SynthFile, SynthModel, SynthRecord};
use crate::vec3::{self as v, V3};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::Instant;

/// How the model raster was obtained.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ModelRaster {
    /// The mesh closes; the raster is the full-containment fill.
    Closed,
    /// The mesh does not close (the winding fill ran on what there is). The
    /// structural checks on it are INDETERMINATE until the consumer match (G5).
    Open,
    /// The raster could not be built (budget, empty). INDETERMINATE.
    Failed(String),
}

/// One support record, as the comparison sees it.
#[derive(Clone, Debug)]
pub struct RecInfo {
    pub kind: RecordKind,
    pub a: V3,
    pub b: V3,
    pub ra: f64,
    pub rb: f64,
    pub mid: V3,
    pub len: f64,
    /// Largest distance of an emitted vertex from the header surface, or the
    /// shortfall of the mesh ends against the header ends, whichever is larger.
    pub dev_mm: f64,
    /// Smallest section radius of the emitted mesh along the record axis
    /// (frusta only).
    pub sec_r: Option<f64>,
}

/// A post: a chain of collinear Member records that no other solid braces at
/// the joints between them. Its span is the unbraced length N5 holds to the
/// slenderness limit; removing a brace merges two posts into one longer span.
#[derive(Clone, Debug)]
pub struct Post {
    pub a: V3,
    pub b: V3,
    pub span: f64,
    pub rmin: f64,
    pub mid: V3,
}

impl Post {
    /// Span over the thinnest diameter.
    #[must_use]
    pub fn slenderness(&self) -> f64 {
        self.span / (2.0 * self.rmin.max(1e-6))
    }
}

/// Group Member records into posts (see [`Post`]).
#[must_use]
pub fn posts(recs: &[RecInfo]) -> Vec<Post> {
    const NODE_MM: f64 = 0.002;
    const COLLINEAR_COS: f64 = 0.984_807_753; // cos 10 degrees
    let frusta: Vec<usize> = (0..recs.len()).filter(|&i| recs[i].kind.is_frustum()).collect();
    let ends: Vec<(usize, V3)> = frusta
        .iter()
        .flat_map(|&i| [(i * 2, recs[i].a), (i * 2 + 1, recs[i].b)])
        .collect();
    let h = super::matching::Hash3::new(ends.iter().copied(), 0.02);
    let pos = |e: usize| if e.is_multiple_of(2) { recs[e / 2].a } else { recs[e / 2].b };
    // Other frustum records that end at the same node as end `e` of record `i`.
    let others = |e: usize| -> Vec<usize> {
        let p = pos(e);
        let mut v: Vec<usize> = h
            .near(p)
            .filter(|&o| o / 2 != e / 2 && crate::vec3::dist(pos(o), p) <= NODE_MM)
            .collect();
        v.sort_unstable();
        v
    };
    let dir = |i: usize| crate::vec3::normalize(crate::vec3::sub(recs[i].b, recs[i].a));
    let mut seen = vec![false; recs.len()];
    let mut out = Vec::new();
    for &i in &frusta {
        if seen[i] || recs[i].kind != RecordKind::Member {
            continue;
        }
        seen[i] = true;
        // Walk both ways: (record, end index leaving it).
        let mut chain: Vec<(usize, V3, V3)> = vec![(i, recs[i].a, recs[i].b)];
        for forward in [true, false] {
            let (mut cur, mut end) = (i, if forward { i * 2 + 1 } else { i * 2 });
            loop {
                let o = others(end);
                if o.len() != 1 {
                    break;
                }
                let n = o[0] / 2;
                if seen[n] || recs[n].kind != RecordKind::Member {
                    break;
                }
                let (Some(d0), Some(d1)) = (dir(cur), dir(n)) else { break };
                if crate::vec3::dot(d0, d1).abs() < COLLINEAR_COS {
                    break;
                }
                seen[n] = true;
                let far = if o[0] % 2 == 0 { n * 2 + 1 } else { n * 2 };
                let (s, e) = if o[0] % 2 == 0 { (recs[n].a, recs[n].b) } else { (recs[n].b, recs[n].a) };
                if forward {
                    chain.push((n, s, e));
                } else {
                    chain.insert(0, (n, e, s));
                }
                cur = n;
                end = far;
            }
        }
        let span: f64 = chain.iter().map(|&(n, _, _)| recs[n].len).sum();
        let rmin = chain.iter().map(|&(n, _, _)| recs[n].ra.min(recs[n].rb)).fold(f64::MAX, f64::min);
        let a = chain.first().map_or(recs[i].a, |c| c.1);
        let b = chain.last().map_or(recs[i].b, |c| c.2);
        out.push(Post { a, b, span, rmin, mid: crate::vec3::midpoint(a, b) });
    }
    out
}

/// One failing or indeterminate audit item.
#[derive(Clone, Debug)]
pub struct Item {
    /// 0..=9 for I1..I10.
    pub inv: usize,
    pub code: String,
    pub indeterminate: bool,
    pub anchor: Option<V3>,
    pub severity: f64,
    pub floor: f64,
    pub contact: Option<usize>,
    pub record: Option<usize>,
}

/// The audit on one model: either it ran, or the reason it could not.
#[derive(Clone, Debug, Default)]
pub struct AuditFacts {
    pub ran: bool,
    pub refused: Vec<String>,
    pub items: Vec<Item>,
    /// (checked, failed, indeterminate) per invariant.
    pub counts: Vec<(usize, usize, usize)>,
    /// Per contact: I3 `a_min / a_required`, when both were measured.
    pub i3_margin: HashMap<usize, f64>,
}

/// H6: the heads and straight contacts of the geometry report.
#[derive(Clone, Debug, Default)]
pub struct HeadFacts {
    pub measured: bool,
    pub heads: usize,
    pub defective: usize,
    pub skim_violations: usize,
    pub intrusion_count: usize,
    pub intrusion_max_mm: f64,
}

/// A model island (a model component born with no model pixel below).
#[derive(Clone, Debug)]
pub struct Island {
    pub c: Comp,
    /// Tips whose ball lies under the island.
    pub contacts: usize,
}

pub struct ModelFacts {
    pub name: String,
    pub translate: V3,
    pub grid: RasterGrid,
    pub frame_ok: bool,
    pub model_raster: ModelRaster,
    pub support_raster_err: Option<String>,
    pub m: RunRaster,
    pub s: RunRaster,
    pub u: RunRaster,
    pub recs: Vec<RecInfo>,
    /// Record indices of the contact balls.
    pub tips: Vec<usize>,
    pub posts: Vec<Post>,
    /// Per tip: smallest mesh section radius along its path to ground.
    pub path_sec_r: HashMap<usize, f64>,
    pub audit: AuditFacts,
    pub heads: HeadFacts,
    pub floating: Vec<Comp>,
    pub thin_joins: usize,
    pub islands: Vec<Island>,
    pub model_bbox: [f64; 6],
    pub model_volume_mm3: Option<f64>,
    pub model_area_mm2: f64,
    pub model_tris: usize,
    pub seconds: Phase,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct Phase {
    pub parse: f64,
    pub build: f64,
    pub raster: f64,
    pub structure: f64,
    pub audit: f64,
    pub geometry: f64,
}

pub struct FileFacts {
    pub path: PathBuf,
    pub sha256: String,
    pub byte_len: usize,
    pub file: ChituboxFile,
    pub models: Vec<ModelFacts>,
    pub log: Option<LogFacts>,
    pub pose: Option<PoseSpec>,
    pub roundtrip: Result<(), String>,
    pub findings: usize,
}

pub struct Settings {
    pub geometry: bool,
    pub threads: usize,
}

fn model_stem(path: &Path) -> String {
    path.file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default()
}

/// Rebuild the file from its parsed parts and read it back; every field the
/// gate reads must survive. The container round trip of Tier 0.
fn round_trip(file: &ChituboxFile) -> Result<(), String> {
    let synth = SynthFile {
        models: file
            .models
            .iter()
            .map(|m| SynthModel {
                name: m.name.clone(),
                translate_mm: m.translate_mm,
                mesh: m.model_soup.tris.clone(),
                records: m
                    .records
                    .iter()
                    .map(|r| SynthRecord {
                        kind: r.kind,
                        start_mm: r.start_mm,
                        end_mm: r.end_mm,
                        r_start: r.r_start,
                        r_end: r.r_end,
                        trailing: r.trailing,
                        soup: r.soup.tris.clone(),
                    })
                    .collect(),
            })
            .collect(),
        settings_size: 0,
        const16: None,
    };
    let bytes = synth.to_bytes();
    let back = ChituboxFile::parse(Path::new("roundtrip"), &bytes).map_err(|e| format!("re-read failed: {e}"))?;
    if back.models.len() != file.models.len() {
        return Err("model count changed".into());
    }
    for (a, b) in file.models.iter().zip(&back.models) {
        if a.model_soup.tris != b.model_soup.tris {
            return Err("model triangles changed".into());
        }
        if a.records.len() != b.records.len() {
            return Err("record count changed".into());
        }
        for (i, (x, y)) in a.records.iter().zip(&b.records).enumerate() {
            let same = x.kind == y.kind
                && x.start_mm == y.start_mm
                && x.end_mm == y.end_mm
                && x.r_start == y.r_start
                && x.r_end == y.r_end
                && x.soup.tris == y.soup.tris;
            if !same {
                return Err(format!("record {i} changed"));
            }
        }
    }
    Ok(())
}

fn mesh_dev(p: &Primitive, rec: &crate::reader::SupportRecord) -> f64 {
    let ag = p.verify_against_soup(rec);
    let mut dev = ag.max_surface_deviation_mm.max(ag.bbox_overflow_mm);
    if rec.soup.is_empty() {
        return f64::INFINITY;
    }
    if rec.kind.is_frustum() {
        let (a, b) = (rec.start_mm, rec.end_mm);
        let len = v::dist(a, b);
        if len > 0.0 {
            let ax = v::mul(v::sub(b, a), 1.0 / len);
            let (mut t0, mut t1) = (f64::MAX, f64::MIN);
            let mut radial = 0.0f64;
            for i in 0..rec.soup.len() {
                for j in 0..3 {
                    let d = v::sub(rec.soup.v(i, j), a);
                    let t = v::dot(d, ax);
                    t0 = t0.min(t);
                    t1 = t1.max(t);
                    // The audit's signed distance cannot see a mesh whose end rings shrank
                    // inside their own cap discs (every vertex is still ON a cap), so measure
                    // the radius directly: ring vertices must sit at the header radius for
                    // their station. A vertex on the axis is a cap-fan centre and has none.
                    let rho = v::norm(v::sub(d, v::mul(ax, t)));
                    let r_t = rec.r_start + (rec.r_end - rec.r_start) * (t / len).clamp(0.0, 1.0);
                    if rho > 0.05 * r_t {
                        radial = radial.max((rho - r_t).abs());
                    }
                }
            }
            dev = dev.max(t0.max(0.0)).max((len - t1).max(0.0)).max(radial);
        }
    }
    dev
}

/// Smallest section radius of a frustum record's mesh: the vertices' distance to the header
/// axis, taken ring by ring. A ring is a run of vertices at one station along the axis (the
/// generator puts every ring vertex at the same station). Fixed-width bins were tried first and
/// were wrong: a ring that falls on a bin edge moves between bins under one-ulp noise, so the
/// narrowest bin changed from 0.0756 to 0.0212 mm on real Morel_base and Deinonychus files.
fn section_min(rec: &crate::reader::SupportRecord) -> Option<f64> {
    /// Vertices closer than this along the axis belong to one ring, mm.
    const RING_GAP_MM: f64 = 0.002;
    if !rec.kind.is_frustum() || rec.soup.is_empty() {
        return None;
    }
    let (a, b) = (rec.start_mm, rec.end_mm);
    let len = v::dist(a, b);
    if len <= 0.0 {
        return None;
    }
    let ax = v::mul(v::sub(b, a), 1.0 / len);
    let mut pts: Vec<(f64, f64)> = Vec::with_capacity(rec.soup.len() * 3);
    for i in 0..rec.soup.len() {
        for j in 0..3 {
            let p = v::sub(rec.soup.v(i, j), a);
            let t = v::dot(p, ax);
            let rho = v::norm(v::sub(p, v::mul(ax, t)));
            // A cap-fan centre sits ON the axis (rho 0): it is no section.
            if rho >= 1.0e-3 {
                pts.push((t, rho));
            }
        }
    }
    pts.sort_by(|x, y| x.0.total_cmp(&y.0));
    let (mut best, mut ring_r, mut n, mut last_t) = (f64::INFINITY, 0.0f64, 0usize, f64::NEG_INFINITY);
    for &(t, rho) in &pts {
        if t - last_t > RING_GAP_MM && n > 0 {
            if n >= 3 {
                best = best.min(ring_r);
            }
            ring_r = 0.0;
            n = 0;
        }
        ring_r = ring_r.max(rho);
        n += 1;
        last_t = t;
    }
    if n >= 3 {
        best = best.min(ring_r);
    }
    best.is_finite().then_some(best)
}

fn num3(j: &Json) -> Option<V3> {
    let a = j.as_array()?;
    (a.len() == 3).then(|| [a[0].as_f64().unwrap_or(f64::NAN), a[1].as_f64().unwrap_or(f64::NAN), a[2].as_f64().unwrap_or(f64::NAN)])
}

fn rec_point(m: &crate::reader::ModelEntry, r: usize) -> Option<V3> {
    let rec = m.records.get(r)?;
    Some(rec.ball_centre().unwrap_or_else(|| v::midpoint(rec.start_mm, rec.end_mm)))
}

/// Severity (bigger = worse) and the noise floor under which a change is ignored.
fn severity(inv: usize, code: &str, vals: &Json) -> (f64, f64) {
    let f = |key: &str| vals.get(key).and_then(Json::as_f64);
    let px_area = crate::consts::PIXEL_AREA_MM2;
    let vox = crate::consts::VOXEL_VOLUME_MM3;
    match (inv, code) {
        (0, "SEAT_DEPTH_SHALLOW") => (f("deficit_mm").unwrap_or(1.0), 0.001),
        (0, "SEAT_DEPTH_DECLARED_MISMATCH") => (
            (f("declared_mm").unwrap_or(0.0) - f("reconstructed_mm").unwrap_or(0.0)).abs(),
            k::VOXEL_TIGHT_MM,
        ),
        (1, "CONTACT_AREA_SHORT") => (f("deficit_mm2").unwrap_or(1.0), px_area),
        (2, "MIN_SECTION_SHORT") => (f("deficit_mm2").unwrap_or(1.0), px_area),
        // I4 books: how far d_total sits outside the interval the contacts imply, as a fraction (0.1 % is noise).
        (3, "LOAD_BOOKS_DISAGREE") => {
            let d = f("d_total_mm2").unwrap_or(0.0);
            let out = (f("implied_lo_mm2").unwrap_or(d) - d).max(d - f("implied_hi_mm2").unwrap_or(d)).max(0.0);
            (out / d.abs().max(1e-12), 1.0e-3)
        }
        (3, "LOAD_UNOWNED") => (f("a_unowned_mm2").unwrap_or(1.0), px_area),
        (3, "LOAD_COMPONENT_UNSUPPORTED") => (f("area_mm2").unwrap_or(1.0), px_area),
        (4, "PATH_FREE_END") => (f("nearest_solid_mm").unwrap_or(1.0), 0.001),
        (5, "INTERSECT_OUTSIDE_SEAT") => (f("volume_mm3").unwrap_or(1.0), vox),
        (5, "INTERSECT_FAR_SIDE_BREACH") => (f("breach_volume_mm3").unwrap_or(1.0), vox),
        (6 | 8, "ISLAND_UNSUPPORTED") => (f("area_mm2").unwrap_or(1.0), px_area),
        (7, "GEOM_DEAD_MEMBER") => (f("volume_mm3").unwrap_or(1.0), vox),
        (7, "GEOM_MESH_DISAGREES_WITH_RECORD") => (f("max_deviation_mm").unwrap_or(1.0), k::VOXEL_TIGHT_MM),
        (9, c) if c.starts_with("SPREAD_") => ((-f("margin_mm").unwrap_or(0.0)).max(0.0), k::VOXEL_TIGHT_MM),
        _ => (1.0, 0.0),
    }
}

fn anchor_of(f: &Failure, ctx: &ModelContext) -> Option<V3> {
    let g = |key: &str| f.values.get(key);
    if let Some(c) = g("centroid_mm").or_else(|| g("breach_centroid_mm")).and_then(|j| {
        num3(j).or_else(|| {
            let a = j.as_array()?;
            (a.len() == 2).then(|| {
                let z = g("z_mm").and_then(Json::as_f64).unwrap_or_else(|| {
                    g("layer")
                        .and_then(Json::as_f64)
                        .map_or(0.0, |l| ctx.grid.layer_z_mm(l as i64))
                });
                [a[0].as_f64().unwrap_or(f64::NAN), a[1].as_f64().unwrap_or(f64::NAN), z]
            })
        })
    }) {
        return Some(c);
    }
    if let Some(p) = g("point").and_then(num3) {
        return Some(p);
    }
    f.record
        .and_then(|r| rec_point(ctx.model, r))
        .or_else(|| f.contact.and_then(|c| rec_point(ctx.model, c)))
}

/// The audit's frame check with a voxel of slack: a flare foot a few microns
/// under the plate is noise, not a mis-parsed lift.
fn frame_established(f: &crate::context::FrameCheck) -> bool {
    f.support_below_plate_mm <= f.tolerance_mm.max(k::T0_FRAME_SLACK_MM)
}

fn run_audit(ctx: &ModelContext) -> AuditFacts {
    let mut out = AuditFacts::default();
    let frame = ctx.frame_check();
    if !frame_established(&frame) {
        out.refused.push(format!(
            "FILE_FRAME_MISMATCH support_below_plate_mm={:.6}",
            frame.support_below_plate_mm
        ));
    }
    if !ctx.model_mesh.is_empty() && !ctx.model_mesh.closure.is_closed() {
        out.refused.push(format!("FILE_MESH_NOT_CLOSED {}", ctx.model_mesh.closure.summary()));
    }
    if !out.refused.is_empty() {
        return out;
    }
    out.ran = true;
    let gating = checks::gating();
    for (inv, chk) in gating.iter().enumerate() {
        let o: CheckOutcome = chk.run(ctx);
        out.counts.push((o.checked, o.failed, o.indeterminate));
        for f in &o.failures {
            let indeterminate = f.code.ends_with("_INDETERMINATE");
            let (severity, floor) = severity(inv, f.code, &f.values);
            out.items.push(Item {
                inv,
                code: f.code.to_string(),
                indeterminate,
                anchor: anchor_of(f, ctx),
                severity,
                floor,
                contact: f.contact,
                record: f.record,
            });
        }
        if inv == 2 {
            if let Some(rows) = o.measured.get("contacts").and_then(Json::as_array) {
                for r in rows {
                    let (c, a, q) = (
                        r.get("contact").and_then(Json::as_f64),
                        r.get("a_min_mm2").and_then(Json::as_f64),
                        r.get("a_required_mm2").and_then(Json::as_f64),
                    );
                    if let (Some(c), Some(a), Some(q)) = (c, a, q) {
                        if q > 0.0 {
                            out.i3_margin.insert(c as usize, a / q);
                        }
                    }
                }
            }
        }
    }
    out
}

fn head_facts(file: &ChituboxFile, mi: usize, threads: usize) -> HeadFacts {
    let params = geometry::Params { threads, ..geometry::Params::default() };
    let g = georeport::analyse(file, params);
    let Some(m) = g.models.get(mi) else {
        return HeadFacts::default();
    };
    let depth: HashMap<usize, f64> = m.intrusion.ranked.iter().map(|r| (r.record, r.depth_mm)).collect();
    let defective = m
        .heads
        .iter()
        .filter(|h| {
            std::iter::once(&h.tip)
                .chain(h.frusta.iter())
                .any(|r| depth.get(r).copied().unwrap_or(0.0) > k::H6_INTRUSION_MM)
        })
        .count();
    HeadFacts {
        measured: true,
        heads: m.heads.len(),
        defective,
        skim_violations: m.contacts.off_normal_beyond_skim,
        intrusion_count: m.intrusion.count_above,
        intrusion_max_mm: m.intrusion.ranked.first().map_or(0.0, |r| r.depth_mm),
    }
}

/// The union of every record's emitted mesh on the printer grid.
fn support_raster(model: &crate::reader::ModelEntry, grid: RasterGrid, budget: &Budget) -> Result<RunRaster, String> {
    let n: usize = model.records.iter().map(|r| r.soup.len()).sum();
    let mut tris = Vec::with_capacity(n);
    for r in &model.records {
        tris.extend_from_slice(&r.soup.tris);
    }
    if tris.is_empty() {
        return Ok(RunRaster::empty(grid));
    }
    let mesh = Mesh::weld_exact(&Soup { tris, source_offset: 0 });
    RunRaster::from_mesh_unclosed(&mesh, grid, budget).map_err(|e| e.to_string())
}

fn analyse_model(
    file: &ChituboxFile,
    mi: usize,
    options: &AuditOptions,
    set: &Settings,
) -> ModelFacts {
    let t0 = Instant::now();
    let audit_ctx = AuditContext { file, options, sidecar: None, sidecar_absence: Some("compare".into()) };
    let ctx = ModelContext::build(&audit_ctx, mi);
    let model = ctx.model;
    let mut secs = Phase { build: t0.elapsed().as_secs_f64(), ..Phase::default() };

    // Records: header numbers, mesh-vs-header, mesh section.
    let recs: Vec<RecInfo> = model
        .records
        .iter()
        .zip(&ctx.primitives)
        .map(|(r, p)| {
            // A record whose header has no solid (zero length) has nothing to compare its mesh
            // with; I8 reports it as GEOM_DEGENERATE and H4 carries that. A record that shipped
            // no triangles at all is infinitely far from its header.
            let dev_mm = p.as_ref().map_or_else(|_| if r.soup.is_empty() { f64::INFINITY } else { 0.0 }, |p| mesh_dev(p, r));
            RecInfo {
                kind: r.kind,
                a: r.start_mm,
                b: r.end_mm,
                ra: r.r_start,
                rb: r.r_end,
                mid: r.ball_centre().unwrap_or_else(|| v::midpoint(r.start_mm, r.end_mm)),
                len: r.axis_length_mm(),
                dev_mm,
                sec_r: section_min(r),
            }
        })
        .collect();
    let tips = model.contact_records();

    // Rasters, from the emitted meshes.
    let t1 = Instant::now();
    let budget = options.raster_budget;
    let grid = ctx.grid;
    let (m_res, s_res) = std::thread::scope(|sc| {
        let hm = sc.spawn(|| {
            if ctx.model_mesh.is_empty() {
                return (RunRaster::empty(grid), ModelRaster::Failed("model mesh is empty".into()));
            }
            if ctx.model_mesh.closure.is_closed() {
                match RunRaster::from_mesh(&ctx.model_mesh, grid, &budget) {
                    Ok(r) => (r, ModelRaster::Closed),
                    Err(e) => (RunRaster::empty(grid), ModelRaster::Failed(e.to_string())),
                }
            } else {
                match RunRaster::from_mesh_unclosed(&ctx.model_mesh, grid, &budget) {
                    Ok(r) => (r, ModelRaster::Open),
                    Err(e) => (RunRaster::empty(grid), ModelRaster::Failed(e.to_string())),
                }
            }
        });
        let s = support_raster(model, grid, &budget);
        (hm.join().unwrap_or_else(|_| (RunRaster::empty(grid), ModelRaster::Failed("raster thread panicked".into()))), s)
    });
    let (m, model_raster) = m_res;
    let (s, support_raster_err) = match s_res {
        Ok(s) => (s, None),
        Err(e) => (RunRaster::empty(grid), Some(e)),
    };
    let u = m.union(&s);
    secs.raster = t1.elapsed().as_secs_f64();

    // Structure.
    let t2 = Instant::now();
    let gr = band::grounding(&u, &grid, k::H1_LATERAL_PX);
    let s_rr = band::layer_rr(&s);
    let isl: Vec<Island> = band::islands(&m, &s_rr, &grid)
        .into_iter()
        .filter(|c| c.px >= 2)
        .map(|c| {
            let z = grid.layer_z_mm(c.k);
            let contacts = tips
                .iter()
                .filter(|&&t| {
                    let p = recs[t].mid;
                    let r = recs[t].ra + 0.3;
                    p[0] >= c.bb[0] - r
                        && p[0] <= c.bb[1] + r
                        && p[1] >= c.bb[2] - r
                        && p[1] <= c.bb[3] + r
                        && p[2] >= z - 0.6
                        && p[2] <= z + 0.2
                })
                .count();
            Island { c, contacts }
        })
        .collect();
    secs.structure = t2.elapsed().as_secs_f64();

    // Path sections per tip.
    let z_plate = ctx.z_plate_mm;
    let layer = grid.layer_height_mm;
    // Where a path ends. The audit's own ground (the solid reaches the plate) is a clause of its
    // load-path rule; on real files the foot flares end about 0.2 mm above the frame's plate and
    // the raft pads sit in components of their own, so that clause alone ends NO path (N3 then
    // evaluated nothing and passed: found on the real BCO file). A path therefore also ends at
    // the lowest non-contact end of its own component.
    let is_contact = |r: usize| matches!(recs[r].kind, RecordKind::ContactSphere | RecordKind::ContactTaper);
    let low_end = |r: usize| recs[r].a[2].min(recs[r].b[2]);
    let mut comp_low: HashMap<usize, f64> = HashMap::new();
    for r in (0..recs.len()).filter(|&r| !is_contact(r)) {
        let e = comp_low.entry(ctx.graph.component_of_record[r]).or_insert(f64::INFINITY);
        *e = e.min(low_end(r));
    }
    let is_ground = |r: usize| {
        !is_contact(r)
            && (ctx.primitive(r).map(|p| p.aabb().min[2] <= z_plate + layer).unwrap_or(false)
                || comp_low.get(&ctx.graph.component_of_record[r]).is_some_and(|&lo| low_end(r) <= lo + k::VOXEL_TIGHT_MM))
    };
    let dist = ctx.graph.distance_to_ground(&is_ground);
    let mut path_sec_r = HashMap::new();
    for &t in &tips {
        let mut cur = t;
        let mut best = recs[t].sec_r.unwrap_or(f64::INFINITY);
        let mut guard = 0;
        while let Some(d) = dist[cur] {
            if d == 0 || guard > 4096 {
                break;
            }
            guard += 1;
            let next = ctx
                .graph
                .neighbours_of(cur)
                .iter()
                .copied()
                .filter(|&n| dist[n] == Some(d - 1))
                .min();
            match next {
                Some(n) => {
                    cur = n;
                    if let Some(r) = recs[cur].sec_r {
                        best = best.min(r);
                    }
                }
                None => break,
            }
        }
        if best.is_finite() {
            path_sec_r.insert(t, best);
        }
    }

    // The audit.
    let t3 = Instant::now();
    let audit = run_audit(&ctx);
    secs.audit = t3.elapsed().as_secs_f64();

    let frame_ok = frame_established(&ctx.frame_check());
    let bbox = ctx.model_mesh.bbox;
    let facts_bbox = if ctx.model_mesh.is_empty() {
        [0.0; 6]
    } else {
        [bbox.min[0], bbox.min[1], bbox.min[2], bbox.max[0], bbox.max[1], bbox.max[2]]
    };
    let model_volume_mm3 = ctx.model_mesh.closed_volume_mm3();
    let model_area_mm2 = ctx.model_mesh.surface_area_mm2();
    let model_tris = model.model_soup.len();
    let name = model.name.clone();
    let translate = model.translate_mm;
    drop(ctx);

    let t4 = Instant::now();
    let heads = if set.geometry { head_facts(file, mi, set.threads) } else { HeadFacts::default() };
    secs.geometry = t4.elapsed().as_secs_f64();

    ModelFacts {
        name,
        translate,
        grid,
        frame_ok,
        model_raster,
        support_raster_err,
        m,
        s,
        u,
        posts: posts(&recs),
        recs,
        tips,
        path_sec_r,
        audit,
        heads,
        floating: gr.floating,
        thin_joins: gr.thin_joins,
        islands: isl,
        model_bbox: facts_bbox,
        model_volume_mm3,
        model_area_mm2,
        model_tris,
        seconds: secs,
    }
}

/// Analyse one file: parse, round trip, then every model.
pub fn analyse(
    path: &Path,
    log_path: Option<&Path>,
    poses_path: Option<&Path>,
    set: &Settings,
) -> Result<FileFacts, String> {
    let t0 = Instant::now();
    let file = ChituboxFile::open(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let parse_s = t0.elapsed().as_secs_f64();
    let roundtrip = round_trip(&file);
    let options = AuditOptions::default();
    let mut models = Vec::new();
    for mi in 0..file.models.len() {
        let mut mf = analyse_model(&file, mi, &options, set);
        mf.seconds.parse = parse_s;
        models.push(mf);
    }
    let (dlog, dposes) = log::discover(path);
    let log_file = log_path.map(Path::to_path_buf).or(dlog);
    let poses_file = poses_path.map(Path::to_path_buf).or(dposes);
    let log = log_file.as_deref().and_then(LogFacts::read);
    let stem = model_stem(path);
    let pose = poses_file
        .as_deref()
        .and_then(|p| log::read_poses(p, &stem))
        .or_else(|| {
            log.as_ref().and_then(|l| l.rotation_text.as_ref()).map(|t| PoseSpec {
                source: "log",
                how: String::new(),
                committed: t.clone(),
                rot: log::Rot::parse(t),
            })
        });
    Ok(FileFacts {
        path: path.to_path_buf(),
        sha256: file.sha256.clone(),
        byte_len: file.byte_len,
        findings: file.findings.total(),
        file,
        models,
        log,
        pose,
        roundtrip,
    })
}
