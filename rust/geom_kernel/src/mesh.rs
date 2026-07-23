//! Triangle-soup storage. Mirrors ExactSurface's construction: store V0, E1,
//! E2 (raw edges) and reconstruct B = V0+E1, Cc = V0+E2 on demand exactly where
//! the oracle does. Face normals/areas are passed IN from trimesh (never
//! recomputed) so every normal-dependent decision uses byte-identical values
//! (spec sec 2.1).

use crate::vec3::{add, V3};

pub struct Mesh {
    pub nfaces: usize,
    pub v0: Vec<V3>,
    pub e1: Vec<V3>,
    pub e2: Vec<V3>,
    pub normals: Vec<V3>,
    pub areas: Vec<f64>,
    /// per-triangle exact AABB (from the 3 reconstructed vertices)
    pub tri_min: Vec<V3>,
    pub tri_max: Vec<V3>,
    /// per-triangle centroid = mean of the 3 vertices (BVH SAH binning only)
    pub centroid: Vec<V3>,
    pub bounds: [V3; 2],
    pub n_degenerate: usize,
}

impl Mesh {
    /// Build from flat row-major buffers: `tris` is nfaces*9 (v0,v1,v2 xyz),
    /// `normals` nfaces*3, `areas` nfaces, `bounds` 6 (min xyz, max xyz).
    pub fn new(
        tris: &[f64],
        normals: &[f64],
        areas: &[f64],
        bounds: &[f64],
    ) -> Result<Mesh, String> {
        if tris.len() % 9 != 0 {
            return Err(format!("tris length {} not a multiple of 9", tris.len()));
        }
        let nfaces = tris.len() / 9;
        if nfaces == 0 {
            return Err("empty mesh (0 faces) — the oracle cannot build one either".into());
        }
        if normals.len() != nfaces * 3 {
            return Err(format!(
                "normals length {} != nfaces*3 ({})",
                normals.len(),
                nfaces * 3
            ));
        }
        if areas.len() != nfaces {
            return Err(format!("areas length {} != nfaces ({})", areas.len(), nfaces));
        }
        if bounds.len() != 6 {
            return Err(format!("bounds length {} != 6", bounds.len()));
        }

        let mut v0 = Vec::with_capacity(nfaces);
        let mut e1 = Vec::with_capacity(nfaces);
        let mut e2 = Vec::with_capacity(nfaces);
        let mut nrm = Vec::with_capacity(nfaces);
        let mut tri_min = Vec::with_capacity(nfaces);
        let mut tri_max = Vec::with_capacity(nfaces);
        let mut centroid = Vec::with_capacity(nfaces);
        let mut n_degenerate = 0usize;

        for f in 0..nfaces {
            let base = f * 9;
            let a: V3 = [tris[base], tris[base + 1], tris[base + 2]];
            let b: V3 = [tris[base + 3], tris[base + 4], tris[base + 5]];
            let cc: V3 = [tris[base + 6], tris[base + 7], tris[base + 8]];
            // E1 = v1 - v0, E2 = v2 - v0 (geometry_exact.py:329-330)
            let ee1: V3 = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
            let ee2: V3 = [cc[0] - a[0], cc[1] - a[1], cc[2] - a[2]];
            let mn: V3 = [
                a[0].min(b[0]).min(cc[0]),
                a[1].min(b[1]).min(cc[1]),
                a[2].min(b[2]).min(cc[2]),
            ];
            let mx: V3 = [
                a[0].max(b[0]).max(cc[0]),
                a[1].max(b[1]).max(cc[1]),
                a[2].max(b[2]).max(cc[2]),
            ];
            // centroid = tri.mean(axis=1) = (a+b+cc)/3 (geometry_exact.py:331)
            let ctr: V3 = [
                (a[0] + b[0] + cc[0]) / 3.0,
                (a[1] + b[1] + cc[1]) / 3.0,
                (a[2] + b[2] + cc[2]) / 3.0,
            ];
            // degeneracy: zero-extent AABB (spec sec 3.2)
            if mn[0] == mx[0] && mn[1] == mx[1] && mn[2] == mx[2] {
                n_degenerate += 1;
            }
            v0.push(a);
            e1.push(ee1);
            e2.push(ee2);
            nrm.push([normals[f * 3], normals[f * 3 + 1], normals[f * 3 + 2]]);
            tri_min.push(mn);
            tri_max.push(mx);
            centroid.push(ctr);
        }

        let bnds = [
            [bounds[0], bounds[1], bounds[2]],
            [bounds[3], bounds[4], bounds[5]],
        ];

        Ok(Mesh {
            nfaces,
            v0,
            e1,
            e2,
            normals: nrm,
            areas: areas.to_vec(),
            tri_min,
            tri_max,
            centroid,
            bounds: bnds,
            n_degenerate,
        })
    }

    #[inline]
    pub fn vertex_b(&self, i: usize) -> V3 {
        add(self.v0[i], self.e1[i])
    }
    #[inline]
    pub fn vertex_c(&self, i: usize) -> V3 {
        add(self.v0[i], self.e2[i])
    }
}
