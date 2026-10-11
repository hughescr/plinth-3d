//! The classifier: class first (pose, then mesh), then Tier 0, Tier 1 (H1-H6),
//! Tier 2 (N1-N5), then EQUIVALENT, computed from the numbers and never
//! assigned by hand. `ROUND-PLAN-v2.md` section 1 is the specification; the
//! constants live in [`super::consts`] with their provenance.

use super::band::{self, Blob};
use super::consts as k;
use super::matching::{self, diff_items_with, match_records, match_tips};
use super::side::{FileFacts, Item, ModelFacts, ModelRaster, RecInfo};
use super::verdict::{Classes, Row, Status, Verdict};
use crate::json::Json;
use crate::vec3::{self as v, V3};

/// How the comparison is run.
#[derive(Clone, Copy, Debug)]
pub struct Options {
    /// Tolerance band in voxels. 1 is the gate; 0 is the sabotage.
    pub band_voxels: i64,
    /// Lift the INDETERMINATE rule for open meshes and the BCO junction once G5 has matched a ChituBox slice.
    pub assume_consumer_matched: bool,
    /// A missing log makes H5 INDETERMINATE (the bench sets this); otherwise H5 is reported as not assessed.
    pub require_logs: bool,
    /// The noise allowances (`consts::Noise::measured()` is the gate; `none()` is the strict reading).
    pub noise: k::Noise,
    /// List every item behind a row, not the first few (the piece-by-piece triage of a verdict).
    pub full_listing: bool,
    /// What the lane promises (`--declared`); `bench.sh --declared` is the same word.
    pub declared: Option<DeclaredClass>,
    /// Expected changes the lane declares (`--declare-change`), each one a plan change.
    pub declared_changes: DeclaredChanges,
}

/// The class a lane promises, in the words of `bench.sh --declared`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DeclaredClass {
    Identical,
    Equivalent,
    PlanChanged,
    MeshChanged,
}

impl DeclaredClass {
    /// `IDENTICAL`, `EQUIVALENT`, `PLAN-CHANGED` or `MESH-CHANGED`.
    #[must_use]
    pub fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "IDENTICAL" => Self::Identical,
            "EQUIVALENT" => Self::Equivalent,
            "PLAN-CHANGED" => Self::PlanChanged,
            "MESH-CHANGED" => Self::MeshChanged,
            _ => return None,
        })
    }
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Identical => "IDENTICAL",
            Self::Equivalent => "EQUIVALENT",
            Self::PlanChanged => "PLAN-CHANGED",
            Self::MeshChanged => "MESH-CHANGED",
        }
    }
    /// Is a computed verdict within what the lane promised? (`bench.sh` makes the real decision,
    /// with the exact-byte rule and the classes; this is the same matrix for a direct reader.)
    #[must_use]
    pub fn allows(self, v: Verdict) -> bool {
        match self {
            Self::Identical => v == Verdict::Identical,
            Self::Equivalent => matches!(v, Verdict::Identical | Verdict::Equivalent),
            Self::PlanChanged => matches!(v, Verdict::Identical | Verdict::Equivalent | Verdict::PlanChanged),
            Self::MeshChanged => v == Verdict::MeshChanged,
        }
    }
}

/// Expected changes a lane declares. Each one widens exactly one row, by the amount the change
/// explains, and nothing else; an undeclared change of the same kind stays a finding.
#[derive(Clone, Copy, Debug, Default)]
pub struct DeclaredChanges {
    /// N2: twin seats are merged (Q2). A reference contact with no match whose ball is overlapped
    /// by a candidate contact is a merge, not a loss.
    pub contact_merge: bool,
}

impl DeclaredChanges {
    /// Parse one `--declare-change` word.
    pub fn add(&mut self, word: &str) -> Result<(), String> {
        match word {
            "contact-merge" => self.contact_merge = true,
            other => return Err(format!("unknown declared change {other:?} (known: contact-merge)")),
        }
        Ok(())
    }
    #[must_use]
    pub const fn any(self) -> bool {
        self.contact_merge
    }
    #[must_use]
    pub fn names(self) -> Vec<&'static str> {
        let mut v = Vec::new();
        if self.contact_merge {
            v.push("contact-merge");
        }
        v
    }
}

impl Default for Options {
    fn default() -> Self {
        Self {
            band_voxels: 1,
            assume_consumer_matched: false,
            require_logs: false,
            noise: k::Noise::measured(),
            full_listing: false,
            declared: None,
            declared_changes: DeclaredChanges::default(),
        }
    }
}

impl Options {
    /// How many items of a listing to keep: `n` normally, all of them under `--full-listing`.
    fn lim(&self, n: usize) -> usize {
        if self.full_listing { usize::MAX } else { n }
    }
}

pub struct Outcome {
    pub verdict: Verdict,
    pub classes: Classes,
    pub rows: Vec<Row>,
    pub evidence: Json,
    /// Per-layer XOR table (report only), tab separated.
    pub layers_tsv: String,
    pub compare_seconds: f64,
}

fn p3(p: V3) -> String {
    format!("({:.3},{:.3},{:.3})", p[0], p[1], p[2])
}

fn nearest_rec(recs: &[RecInfo], p: V3) -> String {
    let mut best = (f64::MAX, 0usize);
    for (i, r) in recs.iter().enumerate() {
        let d = if r.kind.is_ball() { v::dist(p, r.mid) } else { seg_dist(p, r.a, r.b) };
        if d < best.0 {
            best = (d, i);
        }
    }
    if best.0.is_finite() && best.0 < f64::MAX {
        format!("r{} {} @{:.3}mm", best.1, recs[best.1].kind.label(), best.0)
    } else {
        "none".into()
    }
}

/// The records among `idx` whose axis passes within `margin` mm (plus the record's own radius) of the box.
fn recs_near_box(recs: &[RecInfo], idx: &[usize], lo: V3, hi: V3, margin: f64) -> Vec<usize> {
    idx.iter()
        .copied()
        .filter(|&i| {
            let r = &recs[i];
            let m = margin + r.ra.max(r.rb);
            let (a, b) = if r.kind.is_ball() { (r.mid, r.mid) } else { (r.a, r.b) };
            (0..=16).any(|s| {
                let p = v::add(a, v::mul(v::sub(b, a), f64::from(s) / 16.0));
                (0..3).all(|d| p[d] >= lo[d] - m && p[d] <= hi[d] + m)
            })
        })
        .collect()
}

/// `--probe x,y,z,r`: every support record of either file whose axis passes within `r` mm of the
/// point, with whether the other file has the matching record. For piece-by-piece triage.
#[must_use]
pub fn probe(r: &FileFacts, c: &FileFacts, p: V3, radius: f64) -> Vec<String> {
    let mut out = Vec::new();
    for mi in 0..r.models.len().min(c.models.len()) {
        let (a, b) = (&r.models[mi], &c.models[mi]);
        let m = match_records(&a.recs, &b.recs);
        let mut cand_hit = vec![None; b.recs.len()];
        for (i, j) in m.iter().enumerate() {
            if let Some(j) = j {
                cand_hit[*j] = Some(i);
            }
        }
        let desc = |x: &RecInfo| {
            let (s, e) = if x.kind.is_ball() { (x.mid, x.mid) } else { (x.a, x.b) };
            format!("{} ({:.3},{:.3},{:.3})->({:.3},{:.3},{:.3}) len {:.3} radii {:.3}/{:.3} mesh-dev {:.4}", x.kind.label(), s[0], s[1], s[2], e[0], e[1], e[2], x.len, x.ra, x.rb, x.dev_mm)
        };
        // A NaN z (`--probe x,y,any,r`) ignores height: every record whose plan position is near.
        let flat = p[2].is_nan();
        let near = |x: &RecInfo| {
            let (mut s, mut e) = if x.kind.is_ball() { (x.mid, x.mid) } else { (x.a, x.b) };
            let mut q = p;
            if flat {
                (s[2], e[2], q[2]) = (0.0, 0.0, 0.0);
            }
            seg_dist(q, s, e) <= radius + x.ra.max(x.rb)
        };
        for (i, x) in a.recs.iter().enumerate().filter(|(_, x)| near(x)) {
            out.push(format!("ref  r{i} {} => {}", desc(x), m[i].map_or("NOT IN CANDIDATE".into(), |j| format!("cand r{j}"))));
        }
        for (j, x) in b.recs.iter().enumerate().filter(|(_, x)| near(x)) {
            out.push(format!("cand r{j} {} => {}", desc(x), cand_hit[j].map_or("NOT IN REFERENCE".into(), |i| format!("ref r{i}"))));
        }
    }
    out
}

fn seg_dist(p: V3, a: V3, b: V3) -> f64 {
    let ab = v::sub(b, a);
    let l2 = v::dot(ab, ab);
    let t = if l2 > 0.0 { (v::dot(v::sub(p, a), ab) / l2).clamp(0.0, 1.0) } else { 0.0 };
    v::dist(p, v::add(a, v::mul(ab, t)))
}

fn status_of(fail: bool, ind: bool) -> Status {
    if fail {
        Status::Fail
    } else if ind {
        Status::Indeterminate
    } else {
        Status::Pass
    }
}

/// Everything one model's comparison decided.
struct ModelResult {
    rows: Vec<Row>,
    pose_changed: bool,
    mesh_changed: bool,
    /// Candidates for EQUIVALENT: true when every EQUIVALENT clause held.
    equivalent_clauses: bool,
    evidence: Json,
    layers_tsv: String,
}

// ---------------------------------------------------------------------------
// Class: pose, then mesh
// ---------------------------------------------------------------------------

struct PoseClass {
    changed: bool,
    source: &'static str,
    detail: String,
}

fn pose_class(r: &FileFacts, c: &FileFacts, mi: usize) -> PoseClass {
    let (a, b) = (&r.models[mi], &c.models[mi]);
    let dt = (0..3).map(|i| (a.translate[i] - b.translate[i]).abs()).fold(0.0, f64::max);
    match (&r.pose, &c.pose) {
        (Some(x), Some(y)) => {
            let rot = x.rot.distance_rad(&y.rot);
            PoseClass {
                changed: rot > k::POSE_ROT_RAD || dt > k::POSE_TRANSLATE_MM,
                source: "committed transform (poses.tsv / log)",
                detail: format!(
                    "rotation {:?} vs {:?}: {:.3e} rad; translate delta {:.4} mm",
                    x.committed, y.committed, rot, dt
                ),
            }
        }
        _ => {
            let db = (0..6).map(|i| (a.model_bbox[i] - b.model_bbox[i]).abs()).fold(0.0, f64::max);
            let one = r.pose.is_some() != c.pose.is_some();
            PoseClass {
                changed: dt > k::POSE_TRANSLATE_MM || db > k::POSE_BYTES_BBOX_MM,
                source: if one {
                    "emitted frame (only one file has a transform text)"
                } else {
                    "emitted frame (no transform text on either side)"
                },
                detail: format!("translate delta {dt:.4} mm; model bounding box moved {db:.4} mm"),
            }
        }
    }
}

struct MeshClass {
    changed: bool,
    soup_identical: bool,
    hausdorff_mm: Option<f64>,
    reason: String,
    /// Model drift beyond the band when it was measured.
    drift: Option<(u64, i64)>,
}

fn hausdorff(a: &crate::mesh::Mesh, b_verts: &[V3]) -> f64 {
    // One-sided: farthest vertex of `b` from the surface of `a`. Chunked over threads.
    let n = b_verts.len();
    if n == 0 {
        return 0.0;
    }
    let t = std::thread::available_parallelism().map_or(4, std::num::NonZeroUsize::get).min(16);
    let chunk = n.div_ceil(t);
    std::thread::scope(|sc| {
        let hs: Vec<_> = b_verts
            .chunks(chunk)
            .map(|part| {
                sc.spawn(move || {
                    part.iter()
                        .map(|p| a.nearest_surface(*p).map_or(f64::INFINITY, |h| h.distance_mm))
                        .fold(0.0, f64::max)
                })
            })
            .collect();
        hs.into_iter().map(|h| h.join().unwrap_or(f64::INFINITY)).fold(0.0, f64::max)
    })
}

fn mesh_class(r: &FileFacts, c: &FileFacts, mi: usize, pose_changed: bool, o: &Options) -> MeshClass {
    let (a, b) = (&r.models[mi], &c.models[mi]);
    let (sa, sb) = (&r.file.models[mi].model_soup.tris, &c.file.models[mi].model_soup.tris);
    if sa == sb {
        return MeshClass {
            changed: false,
            soup_identical: true,
            hausdorff_mm: Some(0.0),
            reason: "model triangles bit-identical".into(),
            drift: Some((0, 0)),
        };
    }
    if pose_changed {
        // Rotation-invariant signature: count, volume, area.
        let rel = |x: f64, y: f64| (x - y).abs() / x.abs().max(y.abs()).max(1e-12);
        let same = a.model_tris == b.model_tris
            && match (a.model_volume_mm3, b.model_volume_mm3) {
                (Some(x), Some(y)) => rel(x, y) < k::MESH_SIGNATURE_REL,
                _ => true,
            }
            && rel(a.model_area_mm2, b.model_area_mm2) < k::MESH_SIGNATURE_REL;
        return MeshClass {
            changed: !same,
            soup_identical: false,
            hausdorff_mm: None,
            reason: format!(
                "pose changed, so judged by signature: triangles {}/{}, volume {:?}/{:?}, area {:.6}/{:.6}",
                a.model_tris, b.model_tris, a.model_volume_mm3, b.model_volume_mm3, a.model_area_mm2, b.model_area_mm2
            ),
            drift: None,
        };
    }
    let mut reasons = Vec::new();
    let mut changed = false;
    if a.model_tris != b.model_tris {
        changed = true;
        reasons.push(format!("triangle count {} -> {}", a.model_tris, b.model_tris));
    }
    let (ma, mb) = (crate::mesh::Mesh::weld_exact(&r.file.models[mi].model_soup), crate::mesh::Mesh::weld_exact(&c.file.models[mi].model_soup));
    let h = hausdorff(&ma, &mb.verts).max(hausdorff(&mb, &ma.verts));
    if h > k::MESH_HAUSDORFF_MM {
        changed = true;
        reasons.push(format!("vertex Hausdorff {h:.5} mm over {:.3}", k::MESH_HAUSDORFF_MM));
    }
    let drift = if matches!(a.model_raster, ModelRaster::Failed(_)) || matches!(b.model_raster, ModelRaster::Failed(_)) {
        None
    } else {
        let d = band::drift(&a.m, &b.m, &a.grid, o.band_voxels);
        let big = d.blobs.first().map_or(0, |x| x.vox);
        if d.voxels > k::MESH_DRIFT_VOXELS || big >= k::MESH_DRIFT_BLOB_VOXELS {
            changed = true;
            reasons.push(format!("model drift beyond the band {} voxels, largest blob {big}", d.voxels));
        }
        Some((d.voxels, big))
    };
    MeshClass {
        changed,
        soup_identical: false,
        hausdorff_mm: Some(h),
        reason: if reasons.is_empty() {
            format!("model differs within the band (vertex Hausdorff {h:.2e} mm)")
        } else {
            reasons.join("; ")
        },
        drift,
    }
}

// ---------------------------------------------------------------------------
// Tier 0
// ---------------------------------------------------------------------------

fn tier0_mesh_header(a: &ModelFacts, b: &ModelFacts, matched: Option<&[Option<usize>]>, sfx: &str, noise: &k::Noise, lim: usize) -> Row {
    // The reference's own worst disagreement of each record kind. A record the reference does not
    // have (a new junction, a merged member) is held to what the reference itself draws for its
    // kind (G7: the reference has 145 of 1430 records past one voxel, worst 0.616 mm, from the
    // junction hulls that the owner's v18 ruling draws instead of frusta), not to zero.
    let mut worst_ref: std::collections::HashMap<&'static str, f64> = std::collections::HashMap::new();
    for r in &a.recs {
        if r.dev_mm.is_finite() {
            let e = worst_ref.entry(r.kind.label()).or_insert(0.0);
            *e = e.max(r.dev_mm);
        }
    }
    let mut ref_of_cand: Vec<Option<usize>> = vec![None; b.recs.len()];
    if let Some(m) = matched {
        for (i, j) in m.iter().enumerate() {
            if let Some(j) = j {
                ref_of_cand[*j] = Some(i);
            }
        }
    }
    let mut bad: Vec<(f64, usize, f64)> = Vec::new();
    // Calibration record of `T0_MESH_REL_NOISE`: the reference's own deviation of each flagged record.
    let ref_dev = |j: usize| ref_of_cand[j].map(|i| a.recs[i].dev_mm);
    for (j, rc) in b.recs.iter().enumerate() {
        // What the reference itself draws for this kind of record: the junction hulls are not on their
        // headers by the owner's ruling, and a hull changes with the members that meet there.
        let kind_worst = worst_ref.get(rc.kind.label()).copied().unwrap_or(0.0).max(k::T0_MESH_HEADER_MM) * (1.0 + k::T0_RATE_SLACK) + k::T0_TRIMMED_SLACK_MM;
        let allowed = match ref_of_cand[j] {
            Some(i) => {
                let rd = a.recs[i].dev_mm;
                let own = if rd.is_finite() { rd * (1.0 + noise.t0_rel) + k::T0_MESH_HEADER_MM } else { f64::INFINITY };
                // A record that already disagrees with its header in the reference is a hull: it
                // may change as far as the reference's hulls go. A record that agreed (a frustum)
                // gets only its own allowance, so a planted shrink of a good record still fails.
                if noise.t0_by_ref && rd.is_finite() && rd > k::T0_MESH_HEADER_MM { own.max(kind_worst) } else { own }
            }
            None if noise.t0_by_ref => kind_worst,
            None => k::T0_MESH_HEADER_MM,
        };
        if rc.dev_mm > allowed {
            bad.push((rc.dev_mm - allowed, j, rc.dev_mm));
        }
    }
    bad.sort_by(|x, y| y.0.total_cmp(&x.0));
    let detail: Vec<String> = bad
        .iter()
        .take(lim)
        .map(|(_, j, d)| {
            let rc = &b.recs[*j];
            format!("r{j} {} (length {:.3} mm, radii {:.3}/{:.3}) mesh is {d:.4} mm from its header with the model's cut left out ({:.4} mm with it; short of the header by {:.4}, past it by {:.4}; header rings outside the mesh by {}) at {} (reference record: {})", rc.kind.label(), rc.len, rc.ra, rc.rb, rc.dev_raw_mm, rc.dev_in_mm, rc.dev_out_mm, rc.cover_gap_mm.map_or("n/a (open mesh)".into(), |c| format!("{c:.4}")), p3(rc.mid), ref_dev(*j).map_or("unmatched".into(), |r| format!("{r:.4} mm")))
        })
        .collect();
    let beyond = |m: &ModelFacts| {
        let v: Vec<f64> = m.recs.iter().map(|r| r.dev_mm).filter(|d| *d > k::T0_MESH_HEADER_MM).collect();
        (v.len(), v.iter().copied().fold(0.0, f64::max))
    };
    let (rb, cb) = (beyond(a), beyond(b));
    // The share of records past one voxel may not rise by more than the noise slack: a shift of
    // the engine's drawing shows here even when each new record is within the reference's worst.
    let rate_limit = if noise.t0_by_ref && !a.recs.is_empty() {
        (rb.0 as f64 / a.recs.len() as f64 * b.recs.len() as f64 * (1.0 + k::T0_RATE_SLACK)).ceil() as usize + k::T0_RATE_PLUS
    } else {
        usize::MAX
    };
    let rate_fail = noise.t0_by_ref && cb.0 > rate_limit;
    let mut detail = detail;
    if rate_fail {
        detail.insert(0, format!("the share of records past one voxel rose: reference {} of {}, candidate {} of {}, limit {rate_limit}", rb.0, a.recs.len(), cb.0, b.recs.len()));
    }
    Row::new(
        &format!("T0.mesh_header{sfx}"),
        "each record's emitted mesh agrees with its header",
        status_of(!bad.is_empty() || rate_fail, false),
        format!("{} of {} records beyond the allowance; worst excess {:.4} mm (records beyond one voxel at all: reference {} of {}, worst {:.4} mm; candidate {} of {}, worst {:.4} mm)", bad.len(), b.recs.len(), bad.first().map_or(0.0, |x| x.0), rb.0, a.recs.len(), rb.1, cb.0, b.recs.len(), cb.1),
        &format!("mesh within {:.3} mm of the header, with the model's cut left out; or of the reference's matched record; a new record: the reference's own worst for its kind; the share past one voxel rises by at most {:.0} % plus {} records", k::T0_MESH_HEADER_MM, 100.0 * k::T0_RATE_SLACK, k::T0_RATE_PLUS),
    )
    .with_detail(detail)
}

// ---------------------------------------------------------------------------
// Per-model comparison
// ---------------------------------------------------------------------------

/// What one invariant's item diff held, item by item: the calibration record of the flicker
/// bands (`consts::flicker_band` is read off the largest values of these in jittered pairs).
fn h4_evidence(inv: usize, checked: usize, ri: &[Item], ci: &[Item], d: &matching::ItemDiff) -> Json {
    let item = |it: &Item, extra: f64| {
        let mut o = Json::obj();
        o.set("code", Json::str(it.code.clone()));
        o.set("severity", Json::num(it.severity));
        o.set("floor", Json::num(it.floor));
        o.set("scale", Json::num(it.scale));
        o.set("delta", Json::num(extra));
        o
    };
    let mut o = Json::obj();
    o.set("invariant", Json::str(crate::checks::GATING_IDS[inv]));
    o.set("checked", Json::usize(checked));
    o.set("new_failing", Json::Arr(d.new_failing.iter().map(|&j| item(&ci[j], ci[j].severity)).collect()));
    o.set("worse", Json::Arr(d.worse.iter().map(|&(i, j)| item(&ci[j], ci[j].severity - ri[i].severity)).collect()));
    o.set("lost_evaluation", Json::Arr(d.lost_evaluation.iter().map(|&j| item(&ci[j], 0.0)).collect()));
    o.set("absorbed", Json::Arr(d.absorbed.iter().map(|&(j, kind)| {
        let mut x = item(&ci[j], ci[j].severity);
        x.set("kind", Json::str(kind));
        x
    }).collect()));
    o.set("resolved", Json::usize(d.resolved));
    o
}

fn in_bco_junction(name: &str, c: &band::Comp, grid: &crate::raster::RasterGrid) -> bool {
    let n = name.to_lowercase().replace(['_', ' '], "");
    if !n.contains("batcatowl") {
        return false;
    }
    let _ = grid;
    (c.k - k::BCO_JUNCTION_LAYER).abs() <= k::BCO_JUNCTION_LAYERS
        && (c.cx - k::BCO_JUNCTION_XY[0]).hypot(c.cy - k::BCO_JUNCTION_XY[1]) <= k::BCO_JUNCTION_RADIUS_MM
}

#[allow(clippy::too_many_lines)]
fn compare_model(r: &FileFacts, c: &FileFacts, mi: usize, o: &Options, sfx: &str) -> ModelResult {
    let (a, b) = (&r.models[mi], &c.models[mi]);
    let mut rows: Vec<Row> = Vec::new();
    let mut ev = Json::obj();

    // ---- class ---------------------------------------------------------
    let pose = pose_class(r, c, mi);
    let mesh = mesh_class(r, c, mi, pose.changed, o);
    rows.push(Row::new(
        &format!("C.pose{sfx}"),
        "pose class (from the committed transform)",
        Status::Info,
        format!("{}; {}", if pose.changed { "CHANGED" } else { "same" }, pose.detail),
        &format!("rotation over {:.0e} rad or translation over {:.3} mm", k::POSE_ROT_RAD, k::POSE_TRANSLATE_MM),
    ));
    rows[0].detail.push(format!("source: {}", pose.source));
    rows.push(Row::new(
        &format!("C.mesh{sfx}"),
        "mesh class (model mesh in model coordinates)",
        Status::Info,
        format!("{}; {}", if mesh.changed { "CHANGED" } else { "same" }, mesh.reason),
        &format!("more than {} voxels or a {}-voxel blob beyond the band, or Hausdorff over {:.3} mm, or a different triangle count", k::MESH_DRIFT_VOXELS, k::MESH_DRIFT_BLOB_VOXELS, k::MESH_HAUSDORFF_MM),
    ));
    let same_frame = !pose.changed;
    {
        let mut cj = Json::obj();
        cj.set("pose_changed", Json::Bool(pose.changed));
        cj.set("pose_source", Json::str(pose.source));
        cj.set("pose_detail", Json::str(pose.detail.clone()));
        cj.set("mesh_changed", Json::Bool(mesh.changed));
        cj.set("mesh_soup_identical", Json::Bool(mesh.soup_identical));
        cj.set("mesh_vertex_hausdorff_mm", Json::num_or_null(mesh.hausdorff_mm));
        cj.set("mesh_reason", Json::str(mesh.reason.clone()));
        ev.set("class", cj);
    }

    // ---- Tier 0 ----------------------------------------------------------
    let rec_match = same_frame.then(|| match_records(&a.recs, &b.recs));
    rows.push(Row::new(
        &format!("T0.frame{sfx}"),
        "plate and frame established",
        if !b.frame_ok {
            Status::Fail
        } else if !a.frame_ok {
            Status::Indeterminate
        } else {
            Status::Pass
        },
        format!("ref {}, cand {}", a.frame_ok, b.frame_ok),
        "no support solid below the plate",
    ));
    rows.push(tier0_mesh_header(a, b, rec_match.as_deref(), sfx, &o.noise, o.lim(40)));
    let ras_bad: Vec<String> = [("ref", a), ("cand", b)]
        .iter()
        .flat_map(|(n, m)| {
            let mut v = Vec::new();
            if let ModelRaster::Failed(e) = &m.model_raster {
                v.push(format!("{n} model raster: {e}"));
            }
            if let Some(e) = &m.support_raster_err {
                v.push(format!("{n} support raster: {e}"));
            }
            v
        })
        .collect();
    rows.push(
        Row::new(
            &format!("T0.raster{sfx}"),
            "model and support rasterise from the emitted meshes",
            status_of(false, !ras_bad.is_empty()),
            if ras_bad.is_empty() { "both rasters built".into() } else { ras_bad.join("; ") },
            "a raster failure is INDETERMINATE, never a pass",
        )
        .with_detail(ras_bad.clone()),
    );
    let raster_ok = ras_bad.is_empty();
    let open = a.model_raster == ModelRaster::Open || b.model_raster == ModelRaster::Open;
    let consumer_unverified = !o.assume_consumer_matched;

    // ---- drift (report, EQUIVALENT, blobs) ------------------------------
    let mut drift_summary = None;
    let mut tol_a: Option<crate::raster::RunRaster> = None;
    let mut layers_tsv = String::new();
    if raster_ok && same_frame {
        let d = band::drift(&a.u, &b.u, &a.grid, o.band_voxels);
        let mut bl = Json::Arr(Vec::new());
        for x in d.blobs.iter().take(12) {
            let mut j = Json::obj();
            j.set("voxels", Json::int(x.vox));
            j.set("added", Json::int(x.added));
            j.set("layers", Json::nums([x.k0 as f64, x.k1 as f64]));
            j.set("xy_box_mm", Json::nums(x.bb));
            j.set("near_ref", Json::str(nearest_rec(&a.recs, x.centroid)));
            j.set("near_cand", Json::str(nearest_rec(&b.recs, x.centroid)));
            bl.push(j);
        }
        let x = a.u.difference(&b.u).union(&b.u.difference(&a.u));
        ev.set("cured_xor_voxels", Json::int(x.pixel_count as i64));
        ev.set("cured_xor_percent_of_ref", Json::num(100.0 * x.pixel_count as f64 / (a.u.pixel_count.max(1)) as f64));
        for l in &x.layers {
            let ca = a.u.layer(l.k).map_or(0, crate::raster::Layer::pixel_count);
            let cb = b.u.layer(l.k).map_or(0, crate::raster::Layer::pixel_count);
            let n = l.pixel_count();
            layers_tsv.push_str(&format!("{}\t{:.3}\t{ca}\t{cb}\t{n}\t{:.3}\n", l.k, a.grid.layer_z_mm(l.k), 100.0 * n as f64 / ca.max(cb).max(1) as f64));
        }
        ev.set("drift_beyond_band_voxels", Json::int(d.voxels as i64));
        ev.set("drift_blobs", bl);
        ev.set("drift_blob_count", Json::usize(d.blobs.len()));
        let big: Vec<Blob> = d.blobs.iter().filter(|x| x.vox >= 8).cloned().collect();
        ev.set("drift_blobs_8plus", Json::usize(big.len()));
        drift_summary = Some((d.voxels, d.blobs.first().map_or(0, |x| x.vox)));
        tol_a = Some(d.tol_a);
        rows.push(Row::new(
            &format!("R.drift{sfx}"),
            "voxels beyond the 1-voxel band (report)",
            Status::Info,
            format!("{} voxels, {} blobs, largest {}", d.voxels, d.blobs.len(), drift_summary.map_or(0, |x| x.1)),
            "report only; EQUIVALENT needs at most 500 and no blob of 64",
        ));
    }

    // Records present in one file only (a removed member, a new brace): the places where the design
    // changed. Noise allowances do not reach an item beside one (matching::Allowance::veto).
    let (cand_new_recs, ref_lost_recs): (Vec<usize>, Vec<usize>) = rec_match.as_ref().map_or_else(Default::default, |m| {
        let mut hit = vec![false; b.recs.len()];
        for j in m.iter().flatten() {
            hit[*j] = true;
        }
        let lost = m.iter().enumerate().filter(|(_, j)| j.is_none()).map(|(i, _)| i).collect();
        let added = hit.iter().enumerate().filter(|(_, h)| !**h).map(|(j, _)| j).collect();
        (added, lost)
    });
    let disturbed: Vec<(V3, V3)> = {
        let seg = |r: &RecInfo| if r.kind.is_ball() { (r.mid, r.mid) } else { (r.a, r.b) };
        ref_lost_recs.iter().map(|&i| seg(&a.recs[i])).chain(cand_new_recs.iter().map(|&j| seg(&b.recs[j]))).collect()
    };

    // ---- H1 floating material ----------------------------------------------
    {
        let name = &a.name;
        let region_a = a.floating.iter().any(|f| in_bco_junction(name, f, &a.grid));
        let region_b = b.floating.iter().any(|f| in_bco_junction(&b.name, f, &b.grid));
        let region = (region_a || region_b) && consumer_unverified;
        let open_ind = open && consumer_unverified;
        let (fail, value, detail) = if !raster_ok {
            (false, "rasters unavailable".to_string(), vec![])
        } else if same_frame {
            // Floating material of the candidate that is not floating in the reference, voxel by
            // voxel (3D, within the band): a piece that floats in both files is not new.
            let (nf, pieces) = band::new_floating(&a.float_r, &b.float_r, &b.m, tol_a.as_ref(), &b.grid, o.band_voxels);
            let new: Vec<&band::Comp> = nf.iter().map(|x| &x.comp).collect();
            // Flicker: when no material moved beyond the band (the drift is inside the EQUIVALENT
            // limits), a piece that is "newly floating" and made of material the reference already
            // had within the band is connectivity flicker of unchanged material, not new material:
            // a sub-voxel motion can cut a one-voxel neck of a thin tip in the raster, but it cannot
            // make material. A planted float is new material (none of it inside the band); a cut that
            // matters removes enough material to show as drift.
            let drift_small = drift_summary.is_some_and(|(vx, big)| vx <= k::EQ_DRIFT_VOXELS && big < k::EQ_DRIFT_BLOB_VOXELS);
            let inside: Vec<i64> = if drift_small { nf.iter().map(|x| x.inside).collect() } else { vec![0; new.len()] };
            {
                let mut pj = Json::Arr(Vec::new());
                for p in pieces.iter().filter(|p| p.added > 0).take(200) {
                    let mut j = Json::obj();
                    j.set("voxels", Json::int(p.vox));
                    j.set("new_voxels", Json::int(p.added));
                    j.set("layers", Json::nums([p.k0 as f64, p.k1 as f64]));
                    j.set("xy_box_mm", Json::nums(p.bb));
                    pj.push(j);
                }
                ev.set("h1_floating_pieces_with_new_voxels", pj);
                ev.set("h1_floating_pieces", Json::usize(pieces.len()));
            }
            // A piece that floats in both files is not new. A 3D piece of the candidate's floating
            // material is the reference's piece when at least `H1_PIECE_MATCH_MIN_VOX` of its voxels
            // lie within the band of the reference's floating material; the voxels beyond the band
            // are then the piece's growth (its break moved), and they are excused while the growth
            // stays within max(64 voxels, 10 % of the part it shares with the reference). A piece
            // that grew more is floating material the reference did not have: it is judged as new.
            let piece_same: Vec<bool> = pieces
                .iter()
                .map(|p| {
                    let shared = p.vox - p.added;
                    shared >= k::H1_PIECE_MATCH_MIN_VOX && p.added as f64 <= (k::H1_FLOAT_TOTAL as f64).max(k::H1_GROWTH_FRACTION * shared as f64)
                })
                .collect();
            let in_same_piece = |x: &band::NewFloat| piece_same.get(x.piece).copied().unwrap_or(false);
            let excused: Vec<bool> = nf
                .iter()
                .zip(&inside)
                .map(|(x, &i)| in_same_piece(x) || (i as f64 >= o.noise.h1_old_fraction * x.comp.px as f64 && drift_small && o.noise.h1_old_fraction > 0.0))
                .collect();
            let piece_excused_n = nf.iter().filter(|x| in_same_piece(x)).count();
            ev.set("h1_pieces_floating_in_both", Json::usize(piece_same.iter().filter(|&&s| s).count()));
            {
                let mut j = Json::Arr(Vec::new());
                for ((x, &i), &e) in new.iter().zip(&inside).zip(&excused).filter(|((x, _), _)| x.px >= 8).take(60) {
                    let mut o = Json::obj();
                    o.set("layer", Json::int(x.k));
                    o.set("px", Json::int(x.px));
                    o.set("inside_band_px", Json::int(i));
                    o.set("excused", Json::Bool(e));
                    j.push(o);
                }
                ev.set("h1_new_floating", j);
                ev.set("h1_drift_small", Json::Bool(drift_small));
            }
            let flicker_n = excused.iter().filter(|&&e| e).count();
            let kept: Vec<&band::Comp> = new.iter().zip(&excused).filter(|(_, &e)| !e).map(|(x, _)| *x).collect();
            let new_all = new.len();
            let all_new: Vec<(&band::Comp, i64, bool)> = new.iter().zip(&inside).zip(&excused).map(|((x, &i), &e)| (*x, i, e)).collect();
            let new = kept;
            let big = new.iter().filter(|x| x.px >= k::H1_FLOAT_PX).count();
            let tot: i64 = new.iter().map(|x| x.px).sum();
            let mut det: Vec<String> = if o.full_listing {
                // Every candidate piece, excused or not, with the nearest reference floating
                // piece in plan (any layer), so a break that moved can be told from new material.
                all_new
                    .iter()
                    .map(|&(x, i, e)| {
                        let near_ref = a
                            .floating
                            .iter()
                            .map(|fa| ((fa.cx - x.cx).hypot(fa.cy - x.cy), fa.k - x.k, fa.px))
                            .min_by(|p, q| p.0.total_cmp(&q.0).then(p.1.abs().cmp(&q.1.abs())))
                            .map_or("none".to_string(), |(d, dk, px)| format!("{d:.3} mm away, {dk:+} layers, {px} px"));
                        format!(
                            "new floating k={} {} px at ({:.3},{:.3}) inside_band_px={i} excused={e} near {} ; nearest reference floating piece: {near_ref}",
                            x.k, x.px, x.cx, x.cy, nearest_rec(&b.recs, [x.cx, x.cy, b.grid.layer_z_mm(x.k)])
                        )
                    })
                    .collect()
            } else {
                new.iter()
                    .filter(|x| x.px >= 4)
                    .take(8)
                    .map(|x| format!("new floating k={} {} px at ({:.3},{:.3}) near {}", x.k, x.px, x.cx, x.cy, nearest_rec(&b.recs, [x.cx, x.cy, b.grid.layer_z_mm(x.k)])))
                    .collect()
            };
            if o.full_listing {
                for (pi, p) in pieces.iter().enumerate().filter(|(_, p)| p.added > 0) {
                    let lo = [p.bb[0], p.bb[2], b.grid.layer_z_mm(p.k0)];
                    let hi = [p.bb[1], p.bb[3], b.grid.layer_z_mm(p.k1)];
                    let cn = recs_near_box(&b.recs, &cand_new_recs, lo, hi, 0.15);
                    let rl = recs_near_box(&a.recs, &ref_lost_recs, lo, hi, 0.15);
                    let nm = |recs: &[RecInfo], v: &[usize]| v.iter().take(6).map(|&i| format!("r{i}:{}", recs[i].kind.label())).collect::<Vec<_>>().join(",");
                    det.push(format!(
                        "PIECE layers {}-{} {} voxels ({} model), {} of them new ({} model), {} xy box ({:.3}..{:.3}, {:.3}..{:.3}) near {}; records here that exist in one file only: candidate {} [{}], reference {} [{}]",
                        p.k0, p.k1, p.vox, p.tagged, p.added, p.added_tagged,
                        if piece_same[pi] { "SAME-AS-REFERENCE (floats in both, growth small)" } else if p.vox > p.added { "FLOATS-IN-BOTH-BUT-GREW-MORE" } else { "NEW-PIECE" },
                        p.bb[0], p.bb[1], p.bb[2], p.bb[3], nearest_rec(&b.recs, [p.centroid[0], p.centroid[1], p.centroid[2]]),
                        cn.len(), nm(&b.recs, &cn), rl.len(), nm(&a.recs, &rl)
                    ));
                }
            }
            (big > 0 || tot > k::H1_FLOAT_TOTAL, format!("{} new floating components ({} of {} px or more), {tot} new floating voxels; {flicker_n} of {new_all} candidate components excused ({piece_excused_n} of a piece that floats in the reference too and grew little; the rest as connectivity flicker of material inside the band); ref {} / cand {} floating components overall", new.len(), big, k::H1_FLOAT_PX, a.floating.len(), b.floating.len()), det)
        } else {
            let cb = b.floating.iter().filter(|x| x.px >= k::H1_FLOAT_PX).count();
            let rb = a.floating.iter().filter(|x| x.px >= k::H1_FLOAT_PX).count();
            let ct: i64 = b.floating.iter().map(|x| x.px).sum();
            let rt: i64 = a.floating.iter().map(|x| x.px).sum();
            (cb > rb || ct > rt + k::H1_FLOAT_TOTAL, format!("absolute (pose changed): {cb} components of {} px or more against {rb}; floating voxels {ct} against {rt}", k::H1_FLOAT_PX), vec![])
        };
        let mut det = detail;
        if region {
            det.push("a cured piece lies at the BCO v22 junction near r83 (layer 138, xy 5.26/2.03): whether ChituBox cures it is open until G5".into());
        }
        if open_ind {
            det.push("a model mesh does not close: its raster is not evidence of what ChituBox slices until G5".into());
        }
        rows.push(
            Row::new(&format!("H1{sfx}"), "no new floating material", status_of(fail, raster_ok && (region || open_ind)), value, &format!("no new component of {} px or more, at most {} new floating voxels", k::H1_FLOAT_PX, k::H1_FLOAT_TOTAL)).with_detail(det),
        );
    }

    // ---- H2 island support ------------------------------------------------------
    let mut i9_bad: Vec<V3> = Vec::new();
    // (filled after H4; H2 reads it, so H4 runs first and H2 is pushed afterwards)

    // ---- H4 audit invariants, per matched item ------------------------------------
    let mut h4_rows: Vec<Row> = Vec::new();
    let mut h4_ev: Vec<Json> = Vec::new();
    {
        let (aa, ab) = (&a.audit, &b.audit);
        if !aa.ran || !ab.ran {
            let why: Vec<String> = aa.refused.iter().map(|s| format!("ref: {s}")).chain(ab.refused.iter().map(|s| format!("cand: {s}"))).collect();
            let cand_only = aa.ran && !ab.ran;
            h4_rows.push(
                Row::new(
                    &format!("H4{sfx}"),
                    "audit invariants I1-I10 per matched item",
                    if cand_only { Status::Fail } else { Status::Indeterminate },
                    format!("the audit did not run ({})", if cand_only { "candidate only" } else { "reference" }),
                    "the audit refuses a file whose frame or model mesh is not established; that is never a pass",
                )
                .with_detail(why),
            );
        } else {
            for inv in 0..crate::checks::GATING_IDS.len() {
                let ri: Vec<Item> = aa.items.iter().filter(|x| x.inv == inv).cloned().collect();
                let ci: Vec<Item> = ab.items.iter().filter(|x| x.inv == inv).cloned().collect();
                let id = format!("H4.i{}{sfx}", inv + 1);
                let title = crate::checks::GATING_TITLES[inv];
                if same_frame {
                    let checked = aa.counts.get(inv).map_or(0, |c| c.0);
                    let band = |it: &Item, worse: bool| o.noise.band(&it.code, it.scale, worse);
                    let veto = |it: &Item| it.anchor.is_some_and(|p| disturbed.iter().any(|&(x, y)| seg_dist(p, x, y) <= k::H4_MATCH_MM));
                    let allow = matching::Allowance { band: &band, veto: &veto, cap: if o.noise.items { o.noise.cap(checked) } else { 0 } };
                    let d = diff_items_with(&ri, &ci, &allow);
                    h4_ev.push(h4_evidence(inv, checked, &ri, &ci, &d));
                    if inv == 8 {
                        i9_bad.extend(d.new_failing.iter().filter_map(|&j| ci[j].anchor));
                        i9_bad.extend(d.worse.iter().filter_map(|&(_, j)| ci[j].anchor));
                    }
                    let mut det: Vec<String> = Vec::new();
                    // Under --full-listing every item says whether the design changed beside it (a
                    // record in one file only within H4_MATCH_MM): an item where it did not is noise.
                    let tag = |it: &Item| -> String {
                        if o.full_listing {
                            let kind = it.record.and_then(|r| b.recs.get(r)).map_or("-", |r| r.kind.label());
                            format!(" [record kind {kind}; design changed beside it: {}]", if veto(it) { "yes" } else { "no" })
                        } else {
                            String::new()
                        }
                    };
                    for &j in d.new_failing.iter().take(o.lim(6)) {
                        let it = &ci[j];
                        det.push(format!("NEW {} contact={:?} record={:?} at {}{}", it.code, it.contact, it.record, it.anchor.map_or("-".into(), p3), tag(it)));
                    }
                    for &(i, j) in d.worse.iter().take(o.lim(4)) {
                        det.push(format!("WORSE {} {:.5} -> {:.5} at {}{}", ci[j].code, ri[i].severity, ci[j].severity, ci[j].anchor.map_or("-".into(), p3), tag(&ci[j])));
                    }
                    for &j in d.lost_evaluation.iter().take(o.lim(4)) {
                        det.push(format!("CANNOT EVALUATE {} at {}{}", ci[j].code, ci[j].anchor.map_or("-".into(), p3), tag(&ci[j])));
                    }
                    for &(j, kind) in d.absorbed.iter().take(o.lim(4)) {
                        det.push(format!("FLICKER ({kind}, inside the noise band) {} severity {:.5} at {}{}", ci[j].code, ci[j].severity, ci[j].anchor.map_or("-".into(), p3), tag(&ci[j])));
                    }
                    let fail = !d.new_failing.is_empty() || !d.worse.is_empty();
                    h4_rows.push(
                        Row::new(
                            &id,
                            title,
                            status_of(fail, !d.lost_evaluation.is_empty()),
                            format!(
                                "new failing {}, worse {}, cannot evaluate {}, resolved {}, flicker absorbed {} of {} allowed (ref {} items, cand {})",
                                d.new_failing.len(), d.worse.len(), d.lost_evaluation.len(), d.resolved, d.absorbed.len(), allow.cap, ri.len(), ci.len()
                            ),
                            "no new failing item, no matched item over 10 % worse, no item the reference evaluated left unevaluated",
                        )
                        .with_detail(det),
                    );
                } else {
                    // Same significance rule as the matched case: under the noise floor is not an item.
                    let real = |x: &&Item| !x.indeterminate && (x.floor <= 0.0 || x.severity > x.floor);
                    let fails = |v: &[Item]| v.iter().filter(real).count();
                    let worst = |v: &[Item]| v.iter().filter(real).map(|x| x.severity).fold(0.0, f64::max);
                    let floor = ri.iter().chain(ci.iter()).map(|x| x.floor).fold(0.0, f64::max);
                    let (fr, fc) = (fails(&ri), fails(&ci));
                    let (wr, wc) = (worst(&ri), worst(&ci));
                    let ind_r = ri.iter().filter(|x| x.indeterminate).count();
                    let ind_c = ci.iter().filter(|x| x.indeterminate).count();
                    let fail = fc > fr || wc > wr * (1.0 + k::H4_WORSE_FRACTION) + floor;
                    h4_rows.push(Row::new(
                        &id,
                        title,
                        status_of(fail, ind_c > ind_r),
                        format!("absolute (pose changed): failing items {fr} -> {fc}; worst severity {wr:.5} -> {wc:.5}; indeterminate {ind_r} -> {ind_c}"),
                        "no more failing items and no worse worst item than the reference",
                    ));
                }
            }
        }
    }

    ev.set("h4", Json::Arr(h4_ev));

    // ---- H2 ---------------------------------------------------------------------------
    {
        let open_ind = open && consumer_unverified;
        let (fail, value, det) = if !raster_ok {
            (false, "rasters unavailable".to_string(), vec![])
        } else if same_frame {
            let mut lost = 0usize;
            let mut contact_lost = 0usize;
            let mut det = Vec::new();
            let mut h2_ev: Vec<Json> = Vec::new();
            for ia in a.islands.iter().filter(|x| x.c.px >= k::H2_ISLAND_PX) {
                let Some(ib) = b.islands.iter().find(|x| x.c.k == ia.c.k && (x.c.cx - ia.c.cx).hypot(x.c.cy - ia.c.cy) < k::H2_MATCH_MM) else {
                    continue;
                };
                if ia.c.attach != ib.c.attach && ia.c.attach.min(ib.c.attach) <= 16 {
                    let mut o = Json::obj();
                    o.set("island_px", Json::int(ia.c.px));
                    o.set("attach_ref", Json::int(ia.c.attach));
                    o.set("attach_cand", Json::int(ib.c.attach));
                    h2_ev.push(o);
                }
                if ia.c.attach > o.noise.h2_attach_px && ib.c.attach == 0 {
                    lost += 1;
                    det.push(format!("island k={} {} px at ({:.3},{:.3}) lost ALL support below it ({} px -> 0)", ia.c.k, ia.c.px, ia.c.cx, ia.c.cy, ia.c.attach));
                }
                if ia.contacts > 0 && ib.contacts == 0 {
                    contact_lost += 1;
                    det.push(format!("island k={} at ({:.3},{:.3}) contact count {} -> 0", ia.c.k, ia.c.cx, ia.c.cy, ia.contacts));
                } else if ib.contacts < ia.contacts {
                    let z = b.grid.layer_z_mm(ib.c.k);
                    let near = i9_bad.iter().any(|p| p[0] >= ib.c.bb[0] - 0.3 && p[0] <= ib.c.bb[1] + 0.3 && p[1] >= ib.c.bb[2] - 0.3 && p[1] <= ib.c.bb[3] + 0.3 && (p[2] - z).abs() < 3.0);
                    if near {
                        contact_lost += 1;
                        det.push(format!("island k={} at ({:.3},{:.3}) contacts {} -> {} and its I9 reach got worse", ib.c.k, ib.c.cx, ib.c.cy, ia.contacts, ib.contacts));
                    }
                }
            }
            ev.set("h2_low_attach_changes", Json::Arr(h2_ev));
            // Islands with no bearing at all, counted; a count may rise by the share the noise moved
            // (an island of a pixel or two of bearing flips between 0 and 1 under sub-voxel motion).
            let un = |m: &ModelFacts| m.islands.iter().filter(|x| x.c.px >= k::H2_ISLAND_PX && x.c.attach == 0).count();
            // A candidate island with no bearing whose reference island bore at most the noise
            // (a pixel or two) is the same island at the edge of its threshold: not a new one.
            let uc_new = b
                .islands
                .iter()
                .filter(|x| x.c.px >= k::H2_ISLAND_PX && x.c.attach == 0)
                .filter(|ib| {
                    !a.islands.iter().any(|ia| ia.c.k == ib.c.k && (ia.c.cx - ib.c.cx).hypot(ia.c.cy - ib.c.cy) < k::H2_MATCH_MM && ia.c.attach > 0 && ia.c.attach <= o.noise.h2_attach_px)
                })
                .count();
            let (ur, uc) = (un(a), uc_new);
            let n_isl = a.islands.iter().filter(|x| x.c.px >= k::H2_ISLAND_PX).count();
            let slack = if o.noise.h2_attach_px > 0 { o.noise.cap(n_isl) } else { 0 };
            (lost > 0 || contact_lost > 0 || uc > ur + slack, format!("islands losing all support {lost}, losing contacts {contact_lost}; unsupported islands of {} px or more {ur} -> {uc} (count slack {slack})", k::H2_ISLAND_PX), det)
        } else {
            let un = |m: &ModelFacts| m.islands.iter().filter(|x| x.c.px >= k::H2_ISLAND_PX && x.c.attach == 0).count();
            let (ur, uc) = (un(a), un(b));
            (uc > ur, format!("absolute (pose changed): unsupported islands {ur} -> {uc}"), vec![])
        };
        let mut det = det;
        if open_ind {
            det.push("a model mesh does not close: island bearing on its raster is open until G5".into());
        }
        rows.push(
            Row::new(&format!("H2{sfx}"), "no model island loses its support", status_of(fail, raster_ok && open_ind), value, &format!("no matched island of {} px or more loses support or contacts", k::H2_ISLAND_PX)).with_detail(det),
        );
    }

    // ---- H3 model drift ----------------------------------------------------------------------
    if same_frame && !mesh.changed {
        let (vox, big) = mesh.drift.unwrap_or((0, 0));
        rows.push(Row::new(
            &format!("H3{sfx}"),
            "model drift beyond the band (same pose, same mesh)",
            status_of(vox > k::H3_VOXELS || big >= k::H3_BLOB_VOXELS, false),
            format!("{vox} voxels, largest blob {big}"),
            &format!("at most {} voxels and no blob of {}", k::H3_VOXELS, k::H3_BLOB_VOXELS),
        ));
    } else {
        rows.push(Row::new(&format!("H3{sfx}"), "model drift beyond the band", Status::Skipped, "pose or mesh changed".into(), "runs only when neither changed"));
    }
    rows.extend(h4_rows);

    // ---- H6 heads ------------------------------------------------------------------------------
    {
        let (x, y) = (&a.heads, &b.heads);
        if !x.measured || !y.measured {
            rows.push(Row::new(&format!("H6{sfx}"), "defective heads do not rise", Status::Skipped, "geometry report not run".into(), ""));
        } else {
            let skim_allow = (o.noise.h6_skim_fraction * x.skim_violations as f64).ceil() as usize;
            let fail = y.defective > x.defective || y.skim_violations > x.skim_violations + skim_allow;
            rows.push(Row::new(
                &format!("H6{sfx}"),
                "defective heads do not rise",
                status_of(fail, false),
                format!("defective heads {} -> {} (heads {} -> {}); straight contacts beyond the skim law {} -> {}; deepest intrusion {:.4} -> {:.4} mm", x.defective, y.defective, x.heads, y.heads, x.skim_violations, y.skim_violations, x.intrusion_max_mm, y.intrusion_max_mm),
                &format!("no more defective heads (intrusion over {} mm) and no more skim violations; the head count may rise", k::H6_INTRUSION_MM),
            ));
        }
    }

    // ---- Tier 2 and EQUIVALENT ------------------------------------------------------------------------
    let mut eq_clauses = same_frame && !mesh.changed && raster_ok;
    if same_frame {
        // N1 support volume.
        let (sv_a, sv_b) = (a.s.pixel_count as f64, b.s.pixel_count as f64);
        let n1 = if sv_a > 0.0 { sv_b / sv_a - 1.0 } else { 0.0 };
        rows.push(Row::new(&format!("N1{sfx}"), "support volume", status_of(n1.abs() > k::N1_FRACTION, false), format!("{:+.2} % ({:.3} -> {:.3} mm3)", 100.0 * n1, sv_a * a.grid.voxel_volume_mm3(), sv_b * b.grid.voxel_volume_mm3()), "within +-5 %"));
        // N2 contacts.
        let matched = match_tips(&a.recs, &a.tips, &b.recs, &b.tips);
        let mut frac = if a.tips.is_empty() { 1.0 } else { matched.len() as f64 / a.tips.len() as f64 };
        let mut cnt = if a.tips.is_empty() { 0.0 } else { b.tips.len() as f64 / a.tips.len() as f64 - 1.0 };
        let mut det = Vec::new();
        let mut matched_r = std::collections::HashSet::new();
        let mut matched_c = std::collections::HashSet::new();
        let (mut max_d, mut max_dr) = (0.0f64, 0.0f64);
        for &(i, j, d) in &matched {
            matched_r.insert(i);
            matched_c.insert(j);
            max_d = max_d.max(d);
            max_dr = max_dr.max((b.recs[j].ra / a.recs[i].ra - 1.0).abs());
        }
        // Nearest contact of the other file, for the reader: a lost contact with a survivor close by
        // is a merge; one with none is a loss.
        let nearest_tip = |recs: &[RecInfo], tips: &[usize], p: V3| -> String {
            tips.iter()
                .map(|&t| (v::dist(recs[t].mid, p), t))
                .min_by(|x, y| x.0.total_cmp(&y.0))
                .map_or("none".into(), |(d, t)| format!("nearest contact of the other file r{t} {d:.3} mm away, r={:.3}", recs[t].ra))
        };
        for &i in a.tips.iter().filter(|i| !matched_r.contains(i)).take(o.lim(6)) {
            det.push(format!("LOST contact r{i} at {} r={:.3} layer {}; {}", p3(a.recs[i].mid), a.recs[i].ra, a.grid.layer_at_or_below(a.recs[i].mid[2]), nearest_tip(&b.recs, &b.tips, a.recs[i].mid)));
        }
        for &j in b.tips.iter().filter(|j| !matched_c.contains(j)).take(o.lim(6)) {
            det.push(format!("NEW contact r{j} at {} r={:.3} layer {}; {}", p3(b.recs[j].mid), b.recs[j].ra, b.grid.layer_at_or_below(b.recs[j].mid[2]), nearest_tip(&a.recs, &a.tips, b.recs[j].mid)));
        }
        let lost = a.tips.len() - matched.len();
        let newc = b.tips.len() - matched.len();
        // DECLARED CONTACT MERGE (`--declare-change contact-merge`, a lane that merges twin seats).
        // A reference contact with no match at 0.1 mm is a merge when a candidate contact's ball
        // overlaps its ball: that is the twin rule itself (a seat whose tip ball overlaps the ball
        // of an arm already seated in its column is raised into that arm). A candidate contact with
        // no match is the survivor of a merge when a reference contact's ball overlaps it. Only the
        // contacts that are NOT explained this way count against N2: the count may not move by more
        // than 5 % of the reference's, and not more than 5 % of the reference contacts may be lost
        // with no survivor. A contact lost with no survivor beside it stays a finding.
        let mut merge_note = String::new();
        if o.declared_changes.contact_merge && !a.tips.is_empty() {
            let ha = matching::Hash3::new(a.tips.iter().map(|&i| (i, a.recs[i].mid)), 1.0);
            let hb = matching::Hash3::new(b.tips.iter().map(|&j| (j, b.recs[j].mid)), 1.0);
            // The twin lies in the same COLUMN: the two balls overlap in plan, and the later seat is
            // up to a few layers higher (the raised arm keeps its column and takes the new layer).
            let overlaps = |x: &RecInfo, y: &RecInfo| {
                (x.mid[0] - y.mid[0]).hypot(x.mid[1] - y.mid[1]) <= x.ra + y.ra && (x.mid[2] - y.mid[2]).abs() <= k::N2_MERGE_DZ_MM
            };
            let merged: Vec<usize> = a.tips.iter().copied().filter(|i| !matched_r.contains(i)).filter(|&i| hb.near(a.recs[i].mid).any(|j| overlaps(&a.recs[i], &b.recs[j]))).collect();
            let survivors: Vec<usize> = b.tips.iter().copied().filter(|j| !matched_c.contains(j)).filter(|&j| ha.near(b.recs[j].mid).any(|i| overlaps(&a.recs[i], &b.recs[j]))).collect();
            let (un_lost, un_new) = (lost - merged.len(), newc - survivors.len());
            let n = a.tips.len() as f64;
            frac = 1.0 - un_lost as f64 / n;
            cnt = (un_new as f64 - un_lost as f64) / n;
            merge_note = format!("; declared contact-merge: {} of {lost} lost contacts merged into a contact whose ball overlaps theirs, {} of {newc} new contacts are such survivors, so {un_lost} lost and {un_new} new are unexplained", merged.len(), survivors.len());
            ev.set("contacts_declared_merge", {
                let mut m = Json::obj();
                m.set("merged", Json::usize(merged.len()));
                m.set("survivors_new", Json::usize(survivors.len()));
                m.set("lost_unexplained", Json::usize(un_lost));
                m.set("new_unexplained", Json::usize(un_new));
                m
            });
            if o.full_listing {
                for &i in a.tips.iter().filter(|i| !matched_r.contains(i)) {
                    det.push(format!("{} contact r{i}: {}", if merged.contains(&i) { "MERGED" } else { "LOST, NO SURVIVOR" }, p3(a.recs[i].mid)));
                }
                for &j in b.tips.iter().filter(|j| !matched_c.contains(j)) {
                    det.push(format!("{} contact r{j}: {}", if survivors.contains(&j) { "SURVIVOR" } else { "NEW, NO SOURCE" }, p3(b.recs[j].mid)));
                }
            }
        }
        ev.set("contacts_ref", Json::usize(a.tips.len()));
        ev.set("contacts_cand", Json::usize(b.tips.len()));
        ev.set("contacts_matched", Json::usize(matched.len()));
        ev.set("contacts_max_offset_mm", Json::num(max_d));
        ev.set("contacts_max_dr_over_r", Json::num(max_dr));
        rows.push(
            Row::new(&format!("N2{sfx}"), "contacts", status_of(cnt.abs() > k::N2_COUNT_FRACTION || frac < k::N2_MATCHED_FRACTION, false), format!("count {} -> {} ({:+.2} % net of declared merges); {:.1} % of reference contacts matched within {} mm (or merged, when declared); lost {lost}, new {newc}; max offset {max_d:.4} mm, max |dr/r| {max_dr:.4}{merge_note}", a.tips.len(), b.tips.len(), 100.0 * cnt, 100.0 * frac, k::MATCH_MM), "count within +-5 % and 95 % matched")
                .with_detail(det),
        );
        // N3 member sections along each matched contact's path.
        let (mut thin, mut waived, mut worst_ratio, mut sec_eq) = (Vec::new(), 0usize, f64::MAX, true);
        let mut evaluated = 0usize;
        let mut n3_ev: Vec<(f64, f64)> = Vec::new();
        // Each member on a path is held to its own section: one minimum over the whole path
        // lets a narrow unchanged taper hide the thinning of a thicker member. The path
        // minimum is kept as well, for members that no record of the other file matches.
        let rec_match = match_records(&a.recs, &b.recs);
        let mut seen_member = std::collections::HashSet::new();
        for &(i, j, _) in &matched {
            let (Some(x), Some(y)) = (a.path_sec_r.get(&i), b.path_sec_r.get(&j)) else { continue };
            evaluated += 1;
            let margin = b.audit.i3_margin.get(&j).copied();
            let waivable = margin.is_some_and(|m| m >= 1.5);
            let mut tip_thin = false;
            let mut tip_waived = false;
            let mut judge = |from: f64, to: f64, msg: &dyn Fn() -> String, key: Option<usize>| {
                // A section that moved by no more than the noise of its own measurement is not a
                // change, whatever its ratio: on a sub-voxel tip radius a few microns are most of it.
                let ratio = if (from - to).abs() > o.noise.n3_mm { to / from } else { 1.0 };
                n3_ev.push((from, to));
                worst_ratio = worst_ratio.min(ratio);
                if (ratio - 1.0).abs() > k::EQ_SECTION {
                    sec_eq = false;
                }
                if ratio < 1.0 - k::N3_THINNER {
                    if waivable {
                        tip_waived = true;
                    } else {
                        // A trunk shared by several paths is reported once.
                        if key.is_none_or(|m| seen_member.insert(m)) {
                            thin.push(msg());
                        }
                        tip_thin = true;
                    }
                }
            };
            judge(*x, *y, &|| format!("contact r{j} at {}: path minimum section {x:.4} -> {y:.4} mm ({:+.1} %), I3 margin {}", p3(b.recs[j].mid), 100.0 * (y / x - 1.0), margin.map_or("unmeasured".into(), |m| format!("{m:.2}x"))), None);
            let bset: std::collections::HashSet<usize> = b.path_members.get(&j).map(|v| v.iter().copied().collect()).unwrap_or_default();
            for &m in a.path_members.get(&i).map(Vec::as_slice).unwrap_or_default() {
                let Some(n) = rec_match[m] else { continue };
                if !bset.contains(&n) {
                    continue;
                }
                let (Some(xr), Some(yr)) = (a.recs[m].sec_r, b.recs[n].sec_r) else { continue };
                if xr <= 0.0 {
                    continue;
                }
                judge(xr, yr, &|| format!("member r{n} at {} on the path of contact r{j}: section {xr:.4} -> {yr:.4} mm ({:+.1} %), I3 margin {}", p3(b.recs[n].mid), 100.0 * (yr / xr - 1.0), margin.map_or("unmeasured".into(), |q| format!("{q:.2}x"))), Some(n));
            }
            if tip_waived && !tip_thin {
                waived += 1;
            }
        }
        {
            // Calibration record of N3_NOISE_MM: every section pair whose ratio moved by more than
            // the EQUIVALENT limit, with the absolute change (largest first).
            let mut moved: Vec<(f64, f64, f64)> = n3_ev.iter().filter(|(f, t)| *f > 0.0 && (t / f - 1.0).abs() > k::EQ_SECTION).map(|&(f, t)| ((f - t).abs(), f, t)).collect();
            moved.sort_by(|x, y| y.0.total_cmp(&x.0));
            let mut j = Json::Arr(Vec::new());
            for (d, f, t) in moved.iter().take(40) {
                let mut o = Json::obj();
                o.set("abs_change_mm", Json::num(*d));
                o.set("from_mm", Json::num(*f));
                o.set("to_mm", Json::num(*t));
                j.push(o);
            }
            ev.set("n3_ratio_moved", j);
        }
        rows.push(
            Row::new(&format!("N3{sfx}"), "member sections along each contact's path (shared trunks included)", status_of(!thin.is_empty(), evaluated == 0 && !matched.is_empty()), format!("{} path or member sections (of {evaluated} evaluated matched paths) more than 15 % thinner without an I3 margin of 1.5x ({waived} paths waived by margin); worst ratio {}", thin.len(), if worst_ratio == f64::MAX { "n/a".into() } else { format!("{worst_ratio:.3}") }), "no path more than 15 % thinner unless its I3 margin stays at 1.5x")
                .with_detail(thin.into_iter().take(o.lim(8)).collect()),
        );
        // N4 thin joins.
        let n4_ind = (open && consumer_unverified) || ((a.floating.iter().any(|f| in_bco_junction(&a.name, f, &a.grid)) || b.floating.iter().any(|f| in_bco_junction(&b.name, f, &b.grid))) && consumer_unverified);
        let allow = ((a.thin_joins as f64 * k::N4_FRACTION.max(o.noise.n4_fraction)).ceil() as i64).max(k::N4_MIN_RISE);
        let rise = b.thin_joins as i64 - a.thin_joins as i64;
        rows.push(Row::new(&format!("N4{sfx}"), "thin joins", status_of(rise > allow, n4_ind && raster_ok), format!("{} -> {} (rise {rise}, allowed {allow})", a.thin_joins, b.thin_joins), "rise at most max(10 %, 20)"));
        // N5 bracing.
        let s_lim = c.log.as_ref().and_then(|l| l.slenderness_s).or(r.log.as_ref().and_then(|l| l.slenderness_s)).unwrap_or(k::N5_S_DEFAULT);
        // A post is a chain of collinear Members that nothing braces at the joints
        // between them; its span over its thinnest diameter is held to the limit.
        let mut over = Vec::new();
        let ph = matching::Hash3::new(a.posts.iter().enumerate().map(|(i, p)| (i, p.mid)), 2.0);
        for (j, pc) in b.posts.iter().enumerate() {
            let sc = pc.slenderness();
            if sc <= s_lim {
                continue;
            }
            let twin = ph
                .near(pc.mid)
                .filter(|&i| {
                    let pr = &a.posts[i];
                    let straight = v::dist(pr.a, pc.a).max(v::dist(pr.b, pc.b));
                    let flipped = v::dist(pr.a, pc.b).max(v::dist(pr.b, pc.a));
                    straight.min(flipped) <= k::MATCH_MM
                })
                .min();
            match twin {
                Some(i) => {
                    let sr = a.posts[i].slenderness();
                    if sr <= s_lim || sc > sr * 1.1 + 0.5 {
                        over.push(format!("post {j} at {}: span/diameter {sr:.1} -> {sc:.1} against {s_lim}", p3(pc.mid)));
                    }
                }
                None => over.push(format!("post {j} at {} is new or merged: span {:.3} mm, span/diameter {sc:.1} over {s_lim}", p3(pc.mid), pc.span)),
            }
        }
        let sway = (r.log.as_ref().and_then(|l| l.tied_post_sway_px), c.log.as_ref().and_then(|l| l.tied_post_sway_px));
        let (sway_fail, sway_ind, sway_txt) = match sway {
            (Some(x), Some(y)) => (y > x * (1.0 + k::N5_SWAY_FRACTION) + 1e-9, false, format!("worst tied-post sway {x:.3} -> {y:.3} px")),
            (Some(x), None) => (false, true, format!("worst tied-post sway {x:.3} -> not logged")),
            (None, _) => (false, false, "worst tied-post sway not logged by the reference".into()),
        };
        rows.push(
            Row::new(&format!("N5{sfx}"), "bracing", status_of(!over.is_empty() || sway_fail, sway_ind), format!("{} posts newly over the slenderness limit {s_lim} (record span over diameter); {sway_txt}", over.len()), "no post over its limit that was not over before; sway rise at most 10 %")
                .with_detail(over.into_iter().take(o.lim(8)).collect()),
        );
        // EQUIVALENT clauses.
        let drift_ok = drift_summary.is_some_and(|(vx, big)| vx <= k::EQ_DRIFT_VOXELS && big < k::EQ_DRIFT_BLOB_VOXELS);
        let contacts_ok = lost == 0 && newc == 0 && max_d <= k::EQ_CONTACT_MM && max_dr <= k::EQ_CONTACT_DR;
        eq_clauses = eq_clauses && drift_ok && contacts_ok && sec_eq;
        ev.set("equivalent_clauses", {
            let mut e = Json::obj();
            e.set("drift", Json::Bool(drift_ok));
            e.set("contacts", Json::Bool(contacts_ok));
            e.set("sections", Json::Bool(sec_eq));
            e
        });
    } else {
        rows.push(Row::new(&format!("N1-N5{sfx}"), "Tier 2 bands", Status::Skipped, "pose changed: items cannot be matched by position".into(), "run only without a pose change"));
    }

    ModelResult { rows, pose_changed: pose.changed, mesh_changed: mesh.changed, equivalent_clauses: eq_clauses, evidence: ev, layers_tsv }
}

// ---------------------------------------------------------------------------
// H5: the engine log
// ---------------------------------------------------------------------------

fn h5_rows(r: &FileFacts, c: &FileFacts, o: &Options) -> Vec<Row> {
    let (Some(x), Some(y)) = (&r.log, &c.log) else {
        let msg = format!("log missing: ref {}, cand {}", r.log.is_some(), c.log.is_some());
        return vec![Row::new(
            "H5",
            "engine-log invariants",
            if o.require_logs { Status::Indeterminate } else { Status::Skipped },
            msg,
            "needs both engine logs (the bench always has them)",
        )];
    };
    let mut rows = Vec::new();
    // Absolute checks on the candidate.
    match y.load_gap_mm2 {
        Some(g) => rows.push(Row::new("H5.load_gap", "load gap", status_of(g < k::H5_LOAD_GAP_MM2, false), format!("{g:.4} mm2 (ref {:?})", x.load_gap_mm2), &format!("at least {} mm2", k::H5_LOAD_GAP_MM2))),
        None => rows.push(Row::new("H5.load_gap", "load gap", status_of(false, x.load_gap_mm2.is_some()), "not logged by the candidate".into(), "at least -0.08 mm2")),
    }
    for (id, name, rv, cv) in [
        ("H5.unrooted_invalid", "unrooted / invalid after each pass", x.unrooted_or_invalid, y.unrooted_or_invalid),
        ("H5.newly_overloaded", "newly overloaded members", x.newly_overloaded, y.newly_overloaded),
    ] {
        rows.push(Row::new(id, name, match (cv, rv) {
            (Some(n), _) => status_of(n > 0, false),
            (None, Some(_)) => Status::Indeterminate,
            (None, None) => Status::Skipped,
        }, format!("{cv:?} (ref {rv:?})"), "must be 0"));
    }
    // Counts that may not rise.
    for (id, name, rv, cv) in [
        ("H5.slenderness_refused", "slenderness merges refused", x.slenderness_refused, y.slenderness_refused),
        ("H5.lip_still_cut", "lip seats still cut on the emitted field", x.lip_still_cut, y.lip_still_cut),
        ("H5.lip_bites_headed", "lip-seat bites on headed arms", x.lip_bites_headed, y.lip_bites_headed),
        ("H5.final_gap_warnings", "members within the final gap (warnings)", x.final_gap_warnings, y.final_gap_warnings),
        ("H5.insertion_exhausted", "layers with insertion rounds exhausted", x.insertion_rounds_exhausted, y.insertion_rounds_exhausted),
        ("H5.route_nonconvergence", "route non-convergences", x.route_nonconvergences, y.route_nonconvergences),
        ("H5.merge_order_cycles", "merge-order cycles", x.merge_order_cycles, y.merge_order_cycles),
    ] {
        let status = match (rv, cv) {
            (Some(a), Some(b)) => status_of(b > a, false),
            (Some(_), None) => Status::Indeterminate,
            (None, _) => Status::Skipped,
        };
        rows.push(Row::new(id, name, status, format!("{rv:?} -> {cv:?}"), "may not rise"));
    }
    rows
}

// ---------------------------------------------------------------------------
// Top level
// ---------------------------------------------------------------------------

#[must_use]
pub fn compare(r: &FileFacts, c: &FileFacts, o: &Options) -> Outcome {
    let t0 = std::time::Instant::now();
    let mut classes = Classes::default();
    let mut rows: Vec<Row> = Vec::new();
    let mut ev = Json::obj();
    let mut layers_tsv = String::from("k\tz_mm\tcured_ref\tcured_cand\txor\txor_pct\n");
    if r.sha256 == c.sha256 {
        classes.identical = true;
        return Outcome { verdict: Verdict::Identical, classes, rows, evidence: ev, layers_tsv: String::new(), compare_seconds: t0.elapsed().as_secs_f64() };
    }
    // Tier 0, file level.
    rows.push(Row::new("T0.parse", "both files parse", Status::Pass, format!("ref {} bytes, cand {} bytes; parser findings {} / {}", r.byte_len, c.byte_len, r.findings, c.findings), "a parse failure aborts before this point"));
    rows.push(Row::new(
        "T0.models",
        "model count equal",
        status_of(r.models.len() != c.models.len(), false),
        format!("{} / {}", r.models.len(), c.models.len()),
        "equal",
    ));
    rows.push(Row::new(
        "T0.roundtrip",
        "container round trip",
        match (&r.roundtrip, &c.roundtrip) {
            (_, Err(_)) => Status::Fail,
            (Err(_), Ok(())) => Status::Indeterminate,
            _ => Status::Pass,
        },
        format!("ref {:?}, cand {:?}", r.roundtrip, c.roundtrip),
        "rebuilt and re-read, every model, record and triangle unchanged (the engine's plinth-verify crate is not reachable from this zero-dependency tool)",
    ));
    let mut pose_changed = false;
    let mut mesh_changed = false;
    let mut eq = true;
    if r.models.len() == c.models.len() {
        let multi = r.models.len() > 1;
        let mut evs = Json::Arr(Vec::new());
        for mi in 0..r.models.len() {
            let sfx = if multi { format!("[{mi}]") } else { String::new() };
            let mr = compare_model(r, c, mi, o, &sfx);
            pose_changed |= mr.pose_changed;
            mesh_changed |= mr.mesh_changed;
            eq &= mr.equivalent_clauses;
            rows.extend(mr.rows);
            if !mr.layers_tsv.is_empty() {
                if multi {
                    layers_tsv.push_str(&format!("# model {mi}\n"));
                }
                layers_tsv.push_str(&mr.layers_tsv);
            }
            evs.push(mr.evidence);
        }
        ev.set("models", evs);
    } else {
        eq = false;
    }
    rows.extend(h5_rows(r, c, o));

    let any_fail = rows.iter().any(|x| x.status == Status::Fail);
    let any_ind = rows.iter().any(|x| x.status == Status::Indeterminate);
    classes.regression = any_fail;
    classes.indeterminate = any_ind;
    classes.pose_changed = pose_changed;
    classes.mesh_changed = mesh_changed;
    let tier2_ok = !any_fail && !any_ind;
    classes.equivalent = tier2_ok && !pose_changed && !mesh_changed && eq;
    classes.plan_changed = tier2_ok && !pose_changed && !mesh_changed && !eq;
    let verdict = if any_fail {
        Verdict::Regression
    } else if any_ind {
        Verdict::Indeterminate
    } else if pose_changed {
        Verdict::PoseChanged
    } else if mesh_changed {
        Verdict::MeshChanged
    } else if eq {
        Verdict::Equivalent
    } else {
        Verdict::PlanChanged
    };
    if o.declared.is_some() || o.declared_changes.any() {
        let class = o.declared.map_or("none", DeclaredClass::name);
        let within = o.declared.is_none_or(|d| d.allows(verdict));
        rows.insert(
            0,
            Row::new(
                "C.declared",
                "what the lane declared",
                Status::Info,
                format!("declared {class}, changes [{}]; computed {}: {}", o.declared_changes.names().join(", "), verdict.name(), if within { "within the declaration" } else { "NOT within the declaration" }),
                "report only; bench.sh judges (the exit code is the computed verdict's)",
            ),
        );
    }
    Outcome { verdict, classes, rows, evidence: ev, layers_tsv, compare_seconds: t0.elapsed().as_secs_f64() }
}

#[allow(dead_code)]
fn _keep(_: &matching::ItemDiff) {}
