//! Everything the rule measurements share: the model mesh, the decoded support parts, their
//! chains, the placement (translate) and the settings.

use crate::chains::{self, Chains};
use crate::chitubox::{ChituFile, Model};
use crate::limits::Limits;
use crate::parts::{self, Part};
use crate::tri::{Bvh, Tri, tri_box};
use crate::v3::V;

/// The layer height, mm (PRD P2, owner ruling 2026-10-30: his default). ChituBox applies the
/// layer height only when it slices, so the file does not store it.
pub const LAYER_MM: f64 = 0.05;

/// The model and its supports.
#[derive(Debug)]
pub struct Scene {
    pub limits: Limits,
    /// Worker threads for batched steps.
    pub threads: usize,
    /// Model triangles as written, model-local frame.
    pub model: Vec<Tri>,
    pub model_bvh: Bvh,
    /// The model's translate (X, Y, Z) from the model table.
    pub translate: V,
    /// Lowest model vertex z, local frame.
    pub model_min_z: f64,
    pub parts: Vec<Part>,
    pub chains: Chains,
    /// The model mesh is closed and consistently oriented, so a ray's crossing parity says
    /// inside or outside.
    pub model_closed: bool,
}

/// Is the mesh (nine f32 per triangle) closed and consistently oriented?
fn closed_and_oriented(model: &[Tri]) -> bool {
    #[allow(
        clippy::cast_possible_truncation,
        reason = "the triangles were read from f32"
    )]
    let coords: Vec<f32> = model
        .iter()
        .flatten()
        .flat_map(|p| p.map(|x| x as f32))
        .collect();
    crate::mesh::weld(&coords).is_some_and(|w| crate::mesh::edge_report(&w).closed_and_oriented())
}

impl Scene {
    /// Is `p` inside the model? `None` when no ray direction gives a clean answer or the
    /// model is not a closed, oriented mesh.
    #[must_use]
    pub fn point_in_model(&self, p: V) -> Option<bool> {
        use crate::tri::{RayHit, ray_tri};
        if !self.model_closed {
            return None;
        }
        for d in crate::tri::RAY_DIRS {
            let mut cand = Vec::new();
            self.model_bvh.query_ray(p, d, &mut cand);
            let (mut hits, mut clean) = (0usize, true);
            for i in cand {
                match ray_tri(p, d, &self.model[i as usize], 1e-9) {
                    RayHit::Hit(_) => hits += 1,
                    RayHit::Ambiguous => {
                        clean = false;
                        break;
                    }
                    RayHit::Miss => {}
                }
            }
            if clean {
                return Some(hits % 2 == 1);
            }
        }
        None
    }

    /// Build the scene for model `m` of `file` (bytes `buf`).
    ///
    /// # Errors
    /// A message when the model mesh cannot be read.
    pub fn build(
        limits: &Limits,
        threads: usize,
        file: &ChituFile,
        buf: &[u8],
        m: usize,
    ) -> Result<Self, String> {
        let model_rec: &Model = file.models.get(m).ok_or("no such model")?;
        let coords = file.mesh(buf, m).map_err(|e| e.to_string())?;
        let model: Vec<Tri> = coords
            .chunks_exact(9)
            .map(|t| {
                let v = |i: usize| [f64::from(t[i]), f64::from(t[i + 1]), f64::from(t[i + 2])];
                [v(0), v(3), v(6)]
            })
            .collect();
        let boxes: Vec<_> = model.iter().map(tri_box).collect();
        let model_bvh = Bvh::build(&boxes);
        let model_min_z = model
            .iter()
            .flatten()
            .map(|p| p[2])
            .fold(f64::INFINITY, f64::min);
        let mut parts_vec = Vec::new();
        if let Some(s) = &model_rec.support {
            for (k, r) in s.records.iter().enumerate() {
                let bytes = crate::bytes::slice(
                    buf,
                    u64::from(r.mesh_off),
                    u64::from(r.mesh_len),
                    "record mesh",
                )
                .map_err(|e| e.to_string())?;
                parts_vec.push(parts::decode(k, r, bytes));
            }
        }
        let chains = chains::find(&parts_vec);
        let model_closed = closed_and_oriented(&model);
        Ok(Self {
            limits: limits.clone(),
            threads,
            model,
            model_bvh,
            translate: crate::v3::f(model_rec.translate),
            model_min_z,
            parts: parts_vec,
            chains,
            model_closed,
        })
    }

    /// A scene from parts and model triangles (tests).
    #[must_use]
    pub fn from_parts(limits: &Limits, model: Vec<Tri>, translate: V, parts: Vec<Part>) -> Self {
        let boxes: Vec<_> = model.iter().map(tri_box).collect();
        let model_min_z = model
            .iter()
            .flatten()
            .map(|p| p[2])
            .fold(f64::INFINITY, f64::min);
        let chains = chains::find(&parts);
        let model_closed = closed_and_oriented(&model);
        Self {
            limits: limits.clone(),
            threads: 1,
            model_bvh: Bvh::build(&boxes),
            model,
            translate,
            model_min_z,
            parts,
            chains,
            model_closed,
        }
    }

    /// A point of the file's local frame in the world frame (plate at z = 0).
    #[must_use]
    pub fn world(&self, p: V) -> V {
        [
            p[0] + self.translate[0],
            p[1] + self.translate[1],
            p[2] - self.model_min_z + self.translate[2],
        ]
    }

    /// The local z of the build plate if the model sits at its translate Z.
    #[must_use]
    pub fn plate_z(&self) -> f64 {
        self.model_min_z - self.translate[2]
    }

    /// Indices of the parts that are not valid.
    #[must_use]
    pub fn invalid_parts(&self) -> Vec<&Part> {
        self.parts.iter().filter(|p| !p.is_valid()).collect()
    }

    /// Model triangles near a box.
    #[must_use]
    pub fn model_near(&self, bb: &crate::v3::Aabb) -> Vec<u32> {
        let mut out = Vec::new();
        self.model_bvh.query(bb, &mut out);
        out.sort_unstable();
        out
    }
}
