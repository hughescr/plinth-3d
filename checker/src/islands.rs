//! The model's islands: where a new piece of the model first appears in the print.
//!
//! Slicing a solid from the bottom up, a piece (a connected region of a layer) with nothing
//! under it can only start at a lowest point of the solid: a vertex of the surface, or a flat
//! level group of vertices, that is lower than every vertex it is joined to, with the solid
//! above it. This module finds those from the mesh topology alone (exact on the written f32
//! numbers), for a model that is one closed, outward-wound mesh:
//!
//! * vertices that are equal in height and joined by an edge form one level group;
//! * a group is a **birth** when every edge leaving it goes up and the area-weighted outward
//!   normals of the triangles around it point down on the whole (solid above).
//!
//! Each birth carries the footprint the first layer's piece has (the flat triangles of a level
//! face; for a point or an edge, a small square about it) and its extent in X and Y. Whether
//! a support holds it is decided in `h3`, where the sections are. A model that is not closed,
//! or is wound inward, gives no list.

use crate::h3::{P2, Sec};
use crate::mesh;
use crate::scene::{LAYER_MM, Scene};
use crate::v3::V;

/// Half the side of the square used as the footprint of a point birth, mm. A piece that
/// overlaps by the H3 depth (0.01 mm) has this much of the birth point inside it.
const POINT_HALF_MM: f64 = 0.01;
/// The small-island size (P18, owner ruling 2026-10-30): 25 printer pixels in X (0.014 mm) and
/// in Y (0.019 mm), mm.
pub const SMALL_X_MM: f64 = 0.350;
pub const SMALL_Y_MM: f64 = 0.475;

/// One birth.
#[derive(Debug, Clone)]
pub struct Birth {
    /// Local height of the lowest point.
    pub z: f64,
    /// The layer it falls in.
    pub layer: usize,
    /// Where the first piece is, as convex polygons in the XY plane.
    pub footprint: Vec<Sec>,
    /// The extent of the level group in X and Y, mm.
    pub extent: [f64; 2],
    /// A vertex of the group (local frame).
    pub vertex: V,
}

impl Birth {
    /// The level group is already larger than the small-island size in X or Y, so no growth
    /// measurement could make it small.
    #[must_use]
    pub fn certainly_over_small_size(&self) -> bool {
        self.extent[0] > SMALL_X_MM || self.extent[1] > SMALL_Y_MM
    }
}

fn find(u: &mut [usize], mut x: usize) -> usize {
    while u[x] != x {
        u[x] = u[u[x]];
        x = u[x];
    }
    x
}

fn ccw(mut tri: Vec<P2>) -> Vec<P2> {
    let a = 0.5
        * (0..3)
            .map(|i| tri[i][0] * tri[(i + 1) % 3][1] - tri[(i + 1) % 3][0] * tri[i][1])
            .sum::<f64>();
    if a < 0.0 {
        tri.reverse();
    }
    tri
}

/// The births of the model of `scene`, with layers counted from the plate level `z0`.
///
/// # Errors
/// Why the model gives no island list.
pub fn births(scene: &Scene, z0: f64) -> Result<Vec<Birth>, String> {
    if !scene.model_closed {
        return Err("the model is not a closed, consistently oriented mesh".to_owned());
    }
    #[allow(
        clippy::cast_possible_truncation,
        reason = "the triangles were read from f32"
    )]
    let coords: Vec<f32> = scene
        .model
        .iter()
        .flatten()
        .flat_map(|p| p.map(|x| x as f32))
        .collect();
    let w = mesh::weld(&coords).ok_or("the model mesh is empty or not finite")?;
    let (shell_of, n_shells) = mesh::shells(&w);
    if mesh::shell_volumes(&w, &shell_of, n_shells)
        .iter()
        .sum::<f64>()
        <= 0.0
    {
        return Err("the model mesh is wound inward (its volume is not positive)".to_owned());
    }
    let n = w.verts.len();
    let z = |v: u32| w.verts[v as usize][2];
    let mut uf: Vec<usize> = (0..n).collect();
    let mut nbrs: Vec<Vec<u32>> = vec![Vec::new(); n];
    for t in &w.tris {
        for k in 0..3 {
            let (a, b) = (t[k], t[(k + 1) % 3]);
            nbrs[a as usize].push(b);
            nbrs[b as usize].push(a);
            if z(a).to_bits() == z(b).to_bits() {
                let (ra, rb) = (find(&mut uf, a as usize), find(&mut uf, b as usize));
                uf[ra] = rb;
            }
        }
    }
    let roots: Vec<usize> = (0..n).map(|i| find(&mut uf, i)).collect();
    let mut out = Vec::new();
    let mut seen = vec![false; n];
    for r in 0..n {
        let root = roots[r];
        if seen[root] {
            continue;
        }
        seen[root] = true;
        let group: Vec<usize> = (0..n).filter(|&i| roots[i] == root).collect();
        let lowest = group.iter().all(|&v| {
            nbrs[v]
                .iter()
                .all(|&u| roots[u as usize] == root || z(u) > z(v as u32))
        });
        if !lowest {
            continue;
        }
        // Outward normals around the group: solid above when they point down.
        let (mut nz, mut flat): (f64, Vec<Sec>) = (0.0, Vec::new());
        for t in &w.tris {
            let inside = t.map(|v| roots[v as usize] == root);
            if !inside.iter().any(|b| *b) {
                continue;
            }
            let p = t.map(|v| w.verts[v as usize].map(f64::from));
            let c = crate::v3::cross(crate::v3::sub(p[1], p[0]), crate::v3::sub(p[2], p[0]));
            nz += c[2];
            if inside.iter().all(|b| *b) {
                flat.push(Sec::from_poly(ccw(p
                    .iter()
                    .map(|v| [v[0], v[1]])
                    .collect())));
            }
        }
        if nz >= 0.0 {
            continue;
        }
        let pts: Vec<[f64; 3]> = group.iter().map(|&v| w.verts[v].map(f64::from)).collect();
        let (mut lo, mut hi) = ([f64::INFINITY; 2], [f64::NEG_INFINITY; 2]);
        for p in &pts {
            for k in 0..2 {
                lo[k] = lo[k].min(p[k]);
                hi[k] = hi[k].max(p[k]);
            }
        }
        let footprint = if flat.is_empty() {
            let (x0, x1) = (lo[0] - POINT_HALF_MM, hi[0] + POINT_HALF_MM);
            let (y0, y1) = (lo[1] - POINT_HALF_MM, hi[1] + POINT_HALF_MM);
            vec![Sec::from_poly(vec![[x0, y0], [x1, y0], [x1, y1], [x0, y1]])]
        } else {
            flat
        };
        let zb = pts[0][2];
        #[allow(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            reason = "a layer index"
        )]
        let layer = (((zb - z0) / LAYER_MM) + 1e-9).floor().max(0.0) as usize;
        out.push(Birth {
            z: zb,
            layer,
            footprint,
            extent: [hi[0] - lo[0], hi[1] - lo[1]],
            vertex: pts[0],
        });
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testkit::{self as kit, test_limits};

    #[test]
    fn a_box_has_one_birth_its_underside_and_an_octahedron_has_one_point_birth() {
        let s = Scene::from_parts(
            &test_limits(),
            kit::closed_box([-5.0; 3], [5.0; 3]),
            [0.0, 0.0, 5.0],
            Vec::new(),
        );
        let b = births(&s, -10.0).expect("closed");
        assert_eq!(b.len(), 1, "{b:?}");
        assert!((b[0].extent[0] - 10.0).abs() < 1e-9 && b[0].certainly_over_small_size());
        assert_eq!(b[0].layer, 100);
        // An octahedron stands on its lower vertex: a point birth with a small footprint.
        let (px, nx, py, ny, pz, nz) = (
            [1.0, 0.0, 0.0],
            [-1.0, 0.0, 0.0],
            [0.0, 1.0, 0.0],
            [0.0, -1.0, 0.0],
            [0.0, 0.0, 1.0],
            [0.0, 0.0, -1.0],
        );
        let tris = vec![
            [px, py, pz],
            [py, nx, pz],
            [nx, ny, pz],
            [ny, px, pz],
            [py, px, nz],
            [nx, py, nz],
            [ny, nx, nz],
            [px, ny, nz],
        ];
        let o = Scene::from_parts(&test_limits(), tris, [0.0, 0.0, 5.0], Vec::new());
        let b = births(&o, -6.0).expect("closed");
        assert_eq!(b.len(), 1, "{b:?}");
        assert!(!b[0].certainly_over_small_size());
        assert!(b[0].extent[0].abs() < 1e-12);
    }

    #[test]
    fn an_open_mesh_gives_no_list() {
        let tri = vec![[[-1.0, -1.0, 0.0], [1.0, -1.0, 0.0], [0.0, 1.0, 0.0]]];
        let s = Scene::from_parts(&test_limits(), tri, [0.0; 3], Vec::new());
        assert!(births(&s, 0.0).is_err());
    }
}
