use super::Reject;
use plinth_geom::{Mesh, shapes::Triangle};

/// Hard values and construction reserves, immutable after parsing.
#[derive(Debug, Clone, PartialEq)]
pub struct Limits {
    pub(super) h1: f64,
    pub(super) lift: f32,
    pub(super) gap: f64,
    pub(super) region: f64,
}
impl Limits {
    /// Parse the producer's copy of limits.toml. Unknown, duplicate or changed values
    /// fail closed: changing a hard limit requires an explicit implementation review.
    pub fn parse(text: &str) -> Result<Self, Reject> {
        let expected = [
            ("h1_patch_area_mm2", 0.50),
            ("h4_lift_mm", 5.0),
            ("h6_gap_mm", 0.3),
            ("h9_thickness_mm", 0.1),
            ("region_length_mm", 3.0),
        ];
        let mut seen = [false; 5];
        for line in text.lines() {
            let line = line.split('#').next().unwrap_or("").trim();
            if line.is_empty() || line == "[limits]" {
                continue;
            }
            let (key, val) = line.split_once('=').ok_or(Reject::UnknownShape)?;
            let i = expected
                .iter()
                .position(|(k, _)| *k == key.trim())
                .ok_or(Reject::UnknownShape)?;
            let v = val.trim().parse::<f64>().map_err(|_| Reject::NonFinite)?;
            if seen[i] || !v.is_finite() || (v - expected[i].1).abs() > 1e-12 {
                return Err(Reject::UnknownShape);
            }
            seen[i] = true;
        }
        if !seen.into_iter().all(|v| v) {
            return Err(Reject::UnknownShape);
        }
        Ok(Self {
            h1: 0.48,
            lift: 5.0,
            gap: 0.32,
            region: 3.0,
        })
    }
    /// Current checked-in hard limits. No caller-controlled relaxation.
    pub fn current() -> Result<Self, Reject> {
        Self::parse(include_str!("../../../../limits.toml"))
    }
    /// H1 construction maximum **[S]**, mm².
    pub fn patch_maximum(&self) -> f64 {
        self.h1
    }
}
/// Recorded input orientation. The approved input is already rotated; do not rotate again.
#[derive(Debug, Clone, PartialEq)]
pub struct KeepUpRecord {
    /// Original-to-approved rotation, degrees about X.
    pub rotation_x_deg: f64,
    /// Approved input hash, recorded provenance (not recomputed by this constructor).
    pub approved_sha256: String,
}
/// Invalid model or pose metadata.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PoseError {
    /// Empty mesh or invalid metadata.
    Invalid,
    /// Cannot represent the model in the certified numeric domain.
    OutOfRange,
}
/// Exact f32 native model, plus the identical f64 values used for queries.
#[derive(Debug)]
pub struct PosedModel {
    mesh: Mesh,
    triangles: Vec<Triangle>,
    translate: [f32; 3],
    keep: KeepUpRecord,
    pub(super) plate: f32,
}
impl PosedModel {
    /// Centre in f64, round once to native f32, then recompute the local minimum.
    /// Native world z = local z - local minimum + translate.z. Thus the lowest
    /// written model vertex is exactly 5.0, without estimating a geometric error.
    pub fn new(input: &Mesh, keep: &KeepUpRecord, limits: &Limits) -> Result<Self, PoseError> {
        if input.triangle_count() == 0
            || !keep.rotation_x_deg.is_finite()
            || keep.approved_sha256.len() != 64
            || !keep.approved_sha256.bytes().all(|b| b.is_ascii_hexdigit())
        {
            return Err(PoseError::Invalid);
        }
        let b = input.bounds();
        let centre: [f64; 3] = std::array::from_fn(|i| (b.min[i] + b.max[i]) * 0.5);
        let axes = [input.xs(), input.ys(), input.zs()];
        let rounded: [Vec<f64>; 3] = std::array::from_fn(|i| {
            axes[i]
                .iter()
                .map(|v| f64::from((*v - centre[i]) as f32))
                .collect()
        });
        if rounded
            .iter()
            .flatten()
            .any(|v| !v.is_finite() || v.abs() > 100.0)
        {
            return Err(PoseError::OutOfRange);
        }
        let [xs, ys, zs] = rounded;
        let mesh =
            Mesh::new(xs, ys, zs, input.indices().to_vec()).map_err(|_| PoseError::Invalid)?;
        let triangles = mesh
            .indices()
            .chunks_exact(3)
            .map(|t| {
                std::array::from_fn(|k| {
                    let i = t[k] as usize;
                    [
                        mesh.xs()[i] as f32,
                        mesh.ys()[i] as f32,
                        mesh.zs()[i] as f32,
                    ]
                })
            })
            .collect();
        let plate = mesh.bounds().min[2] as f32 - limits.lift;
        Ok(Self {
            mesh,
            triangles,
            translate: [0.0, 0.0, limits.lift],
            keep: keep.clone(),
            plate,
        })
    }
    /// Query geometry, bit-identical to the native model.
    pub fn mesh(&self) -> &Mesh {
        &self.mesh
    }
    /// Native triangles, no subsequent pose or rounding allowed.
    pub fn triangles(&self) -> &[Triangle] {
        &self.triangles
    }
    /// Native translation (XY centred at zero; Z is lift).
    pub fn translate(&self) -> [f32; 3] {
        self.translate
    }
    /// Native local plate level.
    pub fn plate_z(&self) -> f32 {
        self.plate
    }
    /// Recorded provenance; not a consumer acceptance record.
    pub fn keep_up(&self) -> &KeepUpRecord {
        &self.keep
    }
}
/// Process identity is held by the plan; probes cannot replace it.
#[derive(Debug, Clone, PartialEq)]
pub struct ProcessRecord {
    /// Exposure height, mm.
    pub layer_height_mm: f64,
    /// World layer origin, mm.
    pub layer_origin_mm: f64,
}
impl ProcessRecord {
    pub(super) fn valid(&self) -> bool {
        self.layer_height_mm.is_finite()
            && self.layer_height_mm > 0.0
            && self.layer_height_mm <= 0.1
            && self.layer_origin_mm.is_finite()
            && self.layer_origin_mm.abs() < 1e-12
    }
}
/// Full-patch capacity lower bound and H1 upper bound. Not publicly constructible.
#[derive(Debug)]
pub struct PatchBound {
    pub(super) lo: f64,
    pub(super) hi: f64,
}
impl PatchBound {
    /// Lower bound valid for analytic and native mesh representations, mm².
    pub fn lower(&self) -> f64 {
        self.lo
    }
    /// Enclosing analytic upper bound, mm².
    pub fn upper(&self) -> f64 {
        self.hi
    }
    pub(super) fn cap(self, limits: &Limits) -> Result<CappedPatch, Reject> {
        if !self.lo.is_finite() || !self.hi.is_finite() || self.lo < 0.0 || self.hi < self.lo {
            return Err(Reject::PatchUnverified);
        }
        if self.hi > limits.h1 {
            return Err(Reject::AreaOverLimit {
                hi: self.hi,
                max: limits.h1,
            });
        }
        Ok(CappedPatch(self))
    }
}
/// Proof that the immutable patch met the construction maximum.
#[derive(Debug)]
pub struct CappedPatch(pub(super) PatchBound);
/// A native 16-sided load-bearing section. Constructor checks the written radius.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Section {
    r: f32,
}
impl Section {
    /// Across flats must be ≥ 0.102 mm **[S]**, after f32 rounding.
    pub fn new(r: f32) -> Result<Self, Reject> {
        if !r.is_finite() || r > 10.0 {
            return Err(Reject::NonFinite);
        }
        let across = 2.0 * f64::from(r) * (std::f64::consts::PI / 16.0).cos();
        if across < 0.102 {
            return Err(Reject::TooThin { across });
        }
        Ok(Self { r })
    }
    /// Written radius, mm.
    pub fn radius(self) -> f32 {
        self.r
    }
    /// 16-gon perpendicular section, mm² (not contact area).
    pub fn area(self) -> f64 {
        8.0 * f64::from(self.r).powi(2) * (std::f64::consts::PI / 8.0).sin()
    }
}
