//! Review-only bounded convex hollowing. All exterior planes constrain the cavity.
//! Manifold supplies robust f64 polyhedral clipping/Booleans, not our wall certificate.
//! Geometry is frozen to f32, then checked again after the production emitter.
mod evidence;
#[cfg(test)]
mod tests;

use crate::Mesh;
pub use evidence::{BoreExtent, Evidence, Hole, RimPhase, bore_intrusions, measure};
use manifold_csg::{Manifold, MeshGL64, MeshGL64Options, reserve_ids};
use std::collections::BTreeMap;

/// Admitted outward-plane violation, mm (native f32 tessellation noise).
pub const CONVEX_EPS: f64 = 0.001;
/// Owner's minimum wall, mm.
pub const WALL: f64 = 2.5;
/// Clear inscribed bore diameter, mm.
pub const DIAMETER: f64 = 3.;
const SEGMENTS: i32 = 64;
const ROUND_RESERVE: f64 = 0.0001;
const MAX_TRIANGLES: usize = 8192;
type V = [f64; 3];

#[derive(Debug, Clone, Copy)]
struct Plane {
    n: V,
    d: f64,
    /// Absolute signed-distance arithmetic error for coordinates within ±10,000 mm.
    uncertainty: f64,
}

/// Maximum violation and its original indexed-mesh witness.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Convexity {
    pub maximum_mm: f64,
    pub upper_bound_mm: f64,
    pub triangle: usize,
    pub vertex: usize,
}

/// Frozen model plus construction facts. This is not a drainage certificate.
#[derive(Debug, Clone)]
pub struct Hollowed {
    pub mesh: Mesh,
    pub convexity: Convexity,
    /// Offset intersection before drilling; not a measured finished cavity volume.
    pub offset_void_mm3: f64,
}

fn sub(a: V, b: V) -> V {
    std::array::from_fn(|k| a[k] - b[k])
}
fn dot(a: V, b: V) -> f64 {
    (0..3).map(|k| a[k] * b[k]).sum()
}
fn cross(a: V, b: V) -> V {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}
fn point(mesh: &Mesh, i: usize) -> V {
    [mesh.xs()[i], mesh.ys()[i], mesh.zs()[i]]
}
fn plane(t: [V; 3]) -> Result<Plane, String> {
    let n = cross(sub(t[1], t[0]), sub(t[2], t[0]));
    let length = dot(n, n).sqrt();
    if !length.is_finite() || length <= 0. {
        return Err("degenerate or unbounded face".into());
    }
    let n = n.map(|x| x / length);
    Ok(Plane {
        n,
        d: dot(n, t[0]),
        uncertainty: plane_uncertainty(t, n, length),
    })
}
// Forward absolute-error bound for edge subtraction, cross product, norm, division
// and signed-distance dot products. Use EPSILON (twice unit roundoff) and enlarged
// operation counts. Ill-conditioned planes fail closed at convexity admission.
fn plane_uncertainty(t: [V; 3], normal: V, length: f64) -> f64 {
    let eps = f64::EPSILON;
    let ab = sub(t[1], t[0]);
    let ac = sub(t[2], t[0]);
    let eb: V = std::array::from_fn(|k| eps * (t[1][k].abs() + t[0][k].abs()));
    let ec: V = std::array::from_fn(|k| eps * (t[2][k].abs() + t[0][k].abs()));
    let cross_error: V = std::array::from_fn(|k| {
        let i = (k + 1) % 3;
        let j = (k + 2) % 3;
        eb[i] * ac[j].abs()
            + ec[j] * ab[i].abs()
            + eb[i] * ec[j]
            + eb[j] * ac[i].abs()
            + ec[i] * ab[j].abs()
            + eb[j] * ec[i]
            + 4. * eps * (ab[i].abs() * ac[j].abs() + ab[j].abs() * ac[i].abs())
    });
    let length_error = dot(cross_error, cross_error).sqrt() + 8. * eps * length;
    if length <= length_error {
        return f64::INFINITY;
    }
    let normal_error: V = std::array::from_fn(|k| {
        (cross_error[k] + normal[k].abs() * length_error) / (length - length_error)
            + 4. * eps * normal[k].abs()
    });
    // Both the plane anchor and query are inside this stated coordinate domain.
    20_000. * normal_error.iter().sum::<f64>()
        + 16. * eps * 10_000. * normal.iter().map(|x| x.abs()).sum::<f64>()
}

fn planes(mesh: &Mesh) -> Result<Vec<Plane>, String> {
    (0..mesh.triangle_count())
        .map(|i| plane(mesh.triangle_corners(i)))
        .collect()
}

/// Check welded edges, vertex links, opposite orientation and outward volume.
/// No tolerance welding: the written f32 mesh must actually close.
pub fn closed(mesh: &Mesh) -> Result<(), String> {
    let mut edges = BTreeMap::<(u32, u32), (usize, i32)>::new();
    for (i, t) in mesh.indices().chunks_exact(3).enumerate() {
        plane(mesh.triangle_corners(i))
            .map_err(|e| format!("triangle {i}: {e} {:?}", mesh.triangle_corners(i)))?;
        for (a, b) in [(t[0], t[1]), (t[1], t[2]), (t[2], t[0])] {
            let e = edges.entry((a.min(b), a.max(b))).or_default();
            e.0 += 1;
            e.1 += if a < b { 1 } else { -1 };
        }
    }
    if let Some((edge, counts)) = edges
        .iter()
        .find(|(_, (count, winding))| *count != 2 || *winding != 0)
    {
        return Err(format!("not closed/oriented at edge {edge:?}: {counts:?}"));
    }
    vertex_links(mesh)?;
    if volume(mesh) <= 0. {
        return Err("non-positive outward volume".into());
    }
    Ok(())
}

fn vertex_links(mesh: &Mesh) -> Result<(), String> {
    let mut links = vec![Vec::new(); mesh.vertex_count()];
    for t in mesh.indices().chunks_exact(3) {
        for k in 0..3 {
            links[t[k] as usize].push((t[(k + 1) % 3], t[(k + 2) % 3]));
        }
    }
    for (i, link) in links.iter().enumerate().filter(|(_, l)| !l.is_empty()) {
        let mut seen = std::collections::BTreeSet::new();
        let mut pending = vec![link[0].0];
        while let Some(v) = pending.pop() {
            if !seen.insert(v) {
                continue;
            }
            for &(a, b) in link {
                if a == v {
                    pending.push(b);
                }
                if b == v {
                    pending.push(a);
                }
            }
        }
        // Edge validation makes every link vertex degree two. Require ONE cycle.
        if seen.len() != link.len() {
            return Err(format!("non-manifold vertex link {i}"));
        }
    }
    Ok(())
}

fn volume(mesh: &Mesh) -> f64 {
    (0..mesh.triangle_count())
        .map(|i| {
            let [a, b, c] = mesh.triangle_corners(i);
            dot(a, cross(b, c)) / 6.
        })
        .sum()
}

/// Bounded face/vertex sweep; at most 8192 faces and 8192 vertices. No dense matrix.
pub fn convexity(mesh: &Mesh) -> Result<Convexity, String> {
    if mesh.triangle_count() > MAX_TRIANGLES || mesh.vertex_count() > MAX_TRIANGLES {
        return Err("review convex fixture budget exceeded (8192 triangles/vertices)".into());
    }
    let b = mesh.bounds();
    if b.extent().iter().any(|x| *x > 200.) || b.min.iter().chain(&b.max).any(|x| x.abs() > 10_000.)
    {
        return Err("review convex fixture coordinate budget exceeded".into());
    }
    closed(mesh)?;
    let mut result = Convexity {
        maximum_mm: 0.,
        upper_bound_mm: 0.,
        triangle: 0,
        vertex: 0,
    };
    let planes = planes(mesh)?;
    let uncertainty = planes.iter().map(|p| p.uncertainty).fold(0., f64::max);
    for (t, p) in planes.iter().enumerate() {
        for i in 0..mesh.vertex_count() {
            let violation = dot(p.n, point(mesh, i)) - p.d;
            if violation > result.maximum_mm {
                result = Convexity {
                    maximum_mm: violation,
                    upper_bound_mm: violation + uncertainty,
                    triangle: t,
                    vertex: i,
                };
            }
        }
    }
    result.upper_bound_mm = result.maximum_mm + uncertainty;
    if result.upper_bound_mm > CONVEX_EPS {
        return Err(format!(
            "non-convex or uncertain: maximum outward-plane violation {:.9} mm plus arithmetic bound {uncertainty:.9} mm > {CONVEX_EPS} mm at triangle {}, vertex {} {:?}",
            result.maximum_mm,
            result.triangle,
            result.vertex,
            point(mesh, result.vertex)
        ));
    }
    Ok(result)
}

fn manifold(mesh: &Mesh, preserve: bool) -> Result<Manifold, String> {
    let vertices: Vec<_> = (0..mesh.vertex_count())
        .flat_map(|i| point(mesh, i))
        .collect();
    let indices: Vec<_> = mesh.indices().iter().map(|&i| u64::from(i)).collect();
    // Separate provenance runs prevent the kernel merging coplanar input triangles.
    let base = reserve_ids(mesh.triangle_count() as u32);
    let ids: Vec<_> = (0..mesh.triangle_count())
        .map(|i| base + i as u32)
        .collect();
    let runs: Vec<_> = (0..=mesh.triangle_count())
        .map(|i| (i * 3) as u64)
        .collect();
    let options = if preserve {
        MeshGL64Options::new().runs(&runs, &ids)
    } else {
        MeshGL64Options::new()
    };
    let input =
        MeshGL64::new_with_options(&vertices, 3, &indices, options).map_err(|e| e.to_string())?;
    Manifold::from_meshgl64(&input).map_err(|e| e.to_string())
}

// Polar duality turns all half-spaces n·(x-c) <= d-offset-n·c into
// points n/(d-offset-n·c). A dual hull face gives one primal vertex n_face/d_face.
// The second hull joins those intersection vertices; it is NOT a hull of the input.
// This batches the intersection instead of thousands of sequential Boolean clips.
fn offset_intersection(planes: &[Plane], centre: V, offset: f64) -> Result<Manifold, String> {
    let points = planes
        .iter()
        .map(|p| {
            let d = p.d - offset - dot(p.n, centre);
            if d <= 0. {
                return Err("fixture centre is not strictly inside offset intersection".to_string());
            }
            Ok(p.n.map(|n| n / d))
        })
        .collect::<Result<Vec<_>, _>>()?;
    let dual = Manifold::hull_pts(&points);
    dual.status().map_err(|e| e.to_string())?;
    let (v, n, indices) = dual.to_mesh_f64();
    let vertices = indices
        .chunks_exact(3)
        .map(|t| {
            let corners =
                std::array::from_fn(|k| std::array::from_fn(|a| v[t[k] as usize * n + a]));
            let p = plane(corners)?;
            if p.d <= 0. {
                return Err("unbounded offset intersection".to_string());
            }
            Ok(std::array::from_fn(|k| centre[k] + p.n[k] / p.d))
        })
        .collect::<Result<Vec<V>, String>>()?;
    let inner = Manifold::hull_pts(&vertices);
    inner.status().map_err(|e| e.to_string())?;
    Ok(inner)
}

fn frozen(solid: &Manifold) -> Result<Mesh, String> {
    solid.status().map_err(|e| e.to_string())?;
    let (v, n, indices) = solid.to_mesh_f64();
    let mut xyz = [Vec::new(), Vec::new(), Vec::new()];
    for t in indices.chunks_exact(3) {
        let corners: [V; 3] = std::array::from_fn(|k| {
            std::array::from_fn(|a| f64::from(v[t[k] as usize * n + a] as f32))
        });
        // Equal rounded vertices have zero material area. Omit only collapsed edges;
        // never merge nearby distinct coordinates. The welded result must close exactly.
        if (0..3).any(|k| {
            corners[k]
                .iter()
                .zip(corners[(k + 1) % 3])
                .all(|(a, b)| a.to_bits() == b.to_bits())
        }) {
            continue;
        }
        for corner in corners {
            for k in 0..3 {
                xyz[k].push(corner[k]);
            }
        }
    }
    Mesh::from_soup(&xyz[0], &xyz[1], &xyz[2]).map_err(|e| e.to_string())
}

/// Hollow and drill a +Z posed convex fixture, without rotating or rescaling it.
/// Two opposite openings share an axial cutter but have distinct through-wall mouths.
/// Refuses non-convex, invalid, over-budget or empty-offset inputs.
pub fn hollow(mesh: &Mesh) -> Result<Hollowed, String> {
    let hollowed = build(mesh, WALL + CONVEX_EPS, DIAMETER)?;
    evidence::certify_dimensions(mesh, &hollowed.mesh)?;
    Ok(hollowed)
}

fn build(mesh: &Mesh, offset: f64, diameter: f64) -> Result<Hollowed, String> {
    let convexity = convexity(mesh)?;
    let b = mesh.bounds();
    let e = b.extent();
    let centre = std::array::from_fn::<_, 3, _>(|k| (b.min[k] + b.max[k]) / 2.);
    let outer = manifold(mesh, true)?;
    let inner = offset_intersection(&planes(mesh)?, centre, offset)?;
    inner.status().map_err(|e| e.to_string())?;
    let offset_void_mm3 = inner.volume();
    if offset_void_mm3 <= 0. {
        return Err("inward half-space intersection is empty".into());
    }
    let radius =
        (diameter / 2. + ROUND_RESERVE) / (std::f64::consts::PI / f64::from(SEGMENTS)).cos();
    let bore = Manifold::cylinder(e[2] + 2., radius, radius, SEGMENTS, true)
        .translate(centre[0], centre[1], centre[2]);
    let mesh = frozen(&outer.difference(&inner).difference(&bore))?;
    closed(&mesh)?;
    Ok(Hollowed {
        mesh,
        convexity,
        offset_void_mm3,
    })
}
