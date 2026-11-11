//! Geometric contact between written meshes, for the CAL-2 support-path check
//! (`cal2_head::check_supports_reach_plate`). Every test reads the stored triangles, never
//! the record metadata (start, end, radii), because the metadata can be moved without the
//! mesh and the mesh is what ChituBox slices.
//!
//! Two solids touch when a vertex of one lies within [`TOUCH_MM`] of the other's surface
//! (batched closest-point queries on a BVH, both directions), or, for the body, when a
//! vertex lies inside it (a support that ends inside the body overlaps it, as the plain
//! columns do by design: their tops end 0.1 mm inside).
use plinth_chitubox::Triangle;
use plinth_geom::ray::{Ray, RayHit, first_hits};
use plinth_geom::{Bvh, ClosestHits, Mesh, Points, closest_points};

/// Largest gap, in mm, that still counts as touching. A layer is 0.05 mm; the written
/// joints between support parts meet within 0.01 mm.
pub const TOUCH_MM: f64 = 0.05;

/// One solid: its welded mesh, its vertices as query points, and its bounding box.
#[derive(Debug)]
pub struct Part {
    mesh: Mesh,
    points: Points,
    lo: [f64; 3],
    hi: [f64; 3],
}

impl Part {
    /// Build a part from a triangle soup.
    ///
    /// # Errors
    /// The soup is empty, not finite, or too large for the mesh kind.
    pub fn new(tris: &[Triangle]) -> Result<Self, String> {
        let corner = |k: usize| -> Vec<f64> {
            tris.iter()
                .flat_map(|t| t.iter().map(move |v| f64::from(v[k])))
                .collect()
        };
        let (x, y, z) = (corner(0), corner(1), corner(2));
        let mesh = Mesh::from_soup(&x, &y, &z).map_err(|e| format!("mesh: {e:?}"))?;
        let points = Points::new(mesh.xs().to_vec(), mesh.ys().to_vec(), mesh.zs().to_vec())
            .map_err(|e| format!("points: {e:?}"))?;
        let b = mesh.bounds();
        Ok(Self {
            mesh,
            points,
            lo: b.min,
            hi: b.max,
        })
    }

    /// The hierarchy over this part's triangles.
    ///
    /// # Errors
    /// The part has no triangles.
    pub fn bvh(&self) -> Result<Bvh<'_>, String> {
        Bvh::build(&self.mesh).map_err(|e| format!("bvh: {e:?}"))
    }

    /// The bounding boxes, grown by [`TOUCH_MM`], overlap.
    fn near(&self, other: &Part) -> bool {
        (0..3).all(|k| self.lo[k] - TOUCH_MM <= other.hi[k] && other.lo[k] - TOUCH_MM <= self.hi[k])
    }
}

/// Smallest distance from any vertex of `p` to the surface under `bvh`.
fn vertex_gap(p: &Points, bvh: &Bvh<'_>) -> Result<f64, String> {
    let mut hits = ClosestHits::with_len(p.len());
    closest_points(bvh, p, &mut hits).map_err(|e| format!("closest: {e:?}"))?;
    Ok(hits
        .dist2
        .iter()
        .copied()
        .fold(f64::INFINITY, f64::min)
        .sqrt())
}

/// Whether two parts touch or overlap at their surfaces: a vertex of either within
/// [`TOUCH_MM`] of the other's surface. `ba` and `bb` are the hierarchies of `a` and `b`.
///
/// # Errors
/// A query failed.
pub fn parts_touch(a: &Part, ba: &Bvh<'_>, b: &Part, bb: &Bvh<'_>) -> Result<bool, String> {
    if !a.near(b) {
        return Ok(false);
    }
    Ok(vertex_gap(&a.points, bb)? <= TOUCH_MM || vertex_gap(&b.points, ba)? <= TOUCH_MM)
}

/// Whether `part` (a support part) touches or overlaps the body under `body`: a vertex
/// within [`TOUCH_MM`] of the body surface, or a vertex inside the body. Inside means the
/// first hit going straight down and the first hit going straight up both leave the body
/// (the triangle's outward normal, from the winding, points along the ray).
///
/// # Errors
/// A query failed.
pub fn touches_body(part: &Part, body: &Bvh<'_>) -> Result<bool, String> {
    if vertex_gap(&part.points, body)? <= TOUCH_MM {
        return Ok(true);
    }
    let rays: Vec<Ray> = part
        .points
        .xs()
        .iter()
        .zip(part.points.ys())
        .zip(part.points.zs())
        .flat_map(|((&x, &y), &z)| {
            [-1.0, 1.0].map(|d| Ray {
                origin: [x, y, z],
                dir: [0.0, 0.0, d],
            })
        })
        .collect();
    let mut hits = Vec::new();
    first_hits(body, &rays, &mut hits);
    let mesh = body.mesh();
    let leaves = |i: usize| -> bool {
        let Some(&RayHit::Hit { triangle, .. }) = hits[i].known() else {
            return false;
        };
        let [a, b, c] = mesh.triangle_corners(triangle as usize);
        let (e, f) = (sub(b, a), sub(c, a));
        let n = [
            e[1] * f[2] - e[2] * f[1],
            e[2] * f[0] - e[0] * f[2],
            e[0] * f[1] - e[1] * f[0],
        ];
        n[2] * rays[i].dir[2] > 0.0
    };
    Ok((0..rays.len() / 2).any(|v| leaves(2 * v) && leaves(2 * v + 1)))
}

fn sub(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}
