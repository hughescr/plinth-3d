//! Measurements on decoded, rounded triangles, not on cutter parameters.
use super::{DIAMETER, Mesh, Plane, V, WALL, closed, dot, plane, planes, volume};
use crate::{measure::Measure, slice::slice_slabs_at};

#[derive(Debug, Clone, PartialEq)]
pub struct Hole {
    pub centre_xy_mm: [f64; 2],
    pub z_range_mm: [f64; 2],
    pub clear_diameter_mm: f64,
    pub triangles: usize,
}
#[derive(Debug, Clone, PartialEq)]
pub struct RimPhase {
    pub slab_bottom_mm: f64,
    pub slab_top_mm: f64,
    pub area_mm2: f64,
    pub outer_radius_mm: f64,
    pub pieces: usize,
}
#[derive(Debug, Clone, PartialEq)]
pub struct Evidence {
    /// Conservative plane-distance certificate over whole inner triangles.
    pub minimum_wall_mm: f64,
    pub exterior_inward_deviation_mm: f64,
    pub inner_triangles: usize,
    pub holes: [Hole; 2],
    /// Finished void inside the original exterior, including the bores.
    pub cavity_mm3: f64,
    pub original_mm3: f64,
    pub material_mm3: f64,
    pub triangles: usize,
    pub preserved_uncut_triangles: usize,
    pub first_material_z_mm: f64,
    pub first_bore_rim_z_mm: f64,
    pub first_inner_z_mm: f64,
    pub first_edge_slab: RimPhase,
    pub first_cavity_layer: RimPhase,
    /// A 0.05 mm slab ends below/on/above each written rim birth, ±one f32 ULP.
    pub phases: Vec<RimPhase>,
}

fn slack(p: V, planes: &[Plane]) -> f64 {
    planes
        .iter()
        .map(|f| f.d - dot(f.n, p) - f.uncertainty)
        .fold(f64::INFINITY, f64::min)
}
fn edge_distance(a: V, b: V, centre: [f64; 2]) -> f64 {
    let a = [a[0] - centre[0], a[1] - centre[1]];
    let b = [b[0] - centre[0], b[1] - centre[1]];
    let d = [b[0] - a[0], b[1] - a[1]];
    let l = d[0] * d[0] + d[1] * d[1];
    let t = if l > 0. {
        (-(a[0] * d[0] + a[1] * d[1]) / l).clamp(0., 1.)
    } else {
        0.
    };
    (a[0] + t * d[0]).hypot(a[1] + t * d[1])
}
fn triangle_distance(t: [V; 3], centre: [f64; 2]) -> f64 {
    let signs: [f64; 3] = std::array::from_fn(|k| {
        let a = t[k];
        let b = t[(k + 1) % 3];
        (b[0] - a[0]) * (centre[1] - a[1]) - (b[1] - a[1]) * (centre[0] - a[0])
    });
    if signs.iter().all(|x| *x > 0.) || signs.iter().all(|x| *x < 0.) {
        return 0.;
    }
    (0..3)
        .map(|k| edge_distance(t[k], t[(k + 1) % 3], centre))
        .fold(f64::INFINITY, f64::min)
}
fn key(t: [V; 3]) -> [[u64; 3]; 3] {
    let mut a = t.map(|v| {
        v.map(|x| {
            if x.abs() < f64::MIN_POSITIVE {
                0
            } else {
                x.to_bits()
            }
        })
    });
    a.sort_unstable();
    a
}
fn preservation(original: &Mesh, written: &Mesh, centre: [f64; 2]) -> Result<usize, String> {
    let set = (0..written.triangle_count())
        .map(|i| key(written.triangle_corners(i)))
        .collect::<std::collections::BTreeSet<_>>();
    let mut count = 0;
    // Circumradius plus allowance; triangles touched by the declared bore are excluded.
    for i in 0..original.triangle_count() {
        let t = original.triangle_corners(i);
        if triangle_distance(t, centre) > 1.503 {
            if !set.contains(&key(t)) {
                return Err(format!("uncut exterior triangle {i} changed"));
            }
            count += 1;
        }
    }
    Ok(count)
}
fn phases(mesh: &Mesh, bounds: &[[f64; 2]]) -> Result<Vec<RimPhase>, String> {
    let b = mesh.bounds();
    let centre = [(b.min[0] + b.max[0]) / 2., (b.min[1] + b.max[1]) / 2.];
    match slice_slabs_at(mesh, bounds) {
        Measure::Known(slabs) => Ok(slabs
            .iter()
            .zip(bounds)
            .map(|(s, &[bottom, top])| {
                let pieces = s
                    .envelope
                    .rings
                    .iter()
                    .filter(|r| {
                        r.iter()
                            .zip(r.iter().cycle().skip(1))
                            .take(r.len())
                            .map(|(a, b)| a[0] * b[1] - a[1] * b[0])
                            .sum::<f64>()
                            > 0.
                    })
                    .count();
                let outer_radius_mm = s
                    .envelope
                    .rings
                    .iter()
                    .flatten()
                    .map(|v| (v[0] - centre[0]).hypot(v[1] - centre[1]))
                    .fold(0., f64::max);
                RimPhase {
                    slab_bottom_mm: bottom,
                    slab_top_mm: top,
                    area_mm2: s.envelope.area,
                    outer_radius_mm,
                    pieces,
                }
            })
            .collect()),
        other => Err(format!("rim slice unverified: {other:?}")),
    }
}

struct Scan {
    wall: f64,
    exterior_deviation: f64,
    inner_triangles: usize,
    first_inner: f64,
    holes: [Hole; 2],
    births: Vec<f64>,
}
fn scan(original: &Mesh, written: &Mesh) -> Result<Scan, String> {
    let planes = planes(original)?;
    let b = original.bounds();
    let centre = [(b.min[0] + b.max[0]) / 2., (b.min[1] + b.max[1]) / 2.];
    let mid = (b.min[2] + b.max[2]) / 2.;
    let empty = Hole {
        centre_xy_mm: centre,
        z_range_mm: [f64::INFINITY, f64::NEG_INFINITY],
        clear_diameter_mm: f64::INFINITY,
        triangles: 0,
    };
    let mut s = Scan {
        wall: f64::INFINITY,
        exterior_deviation: 0.,
        inner_triangles: 0,
        first_inner: f64::INFINITY,
        holes: [empty.clone(), empty],
        births: Vec::new(),
    };
    for i in 0..written.triangle_count() {
        let t = written.triangle_corners(i);
        let distances = t.map(|v| slack(v, &planes));
        let n = plane(t)?.n;
        let bore = n[2].abs() < 0.01
            && t.iter()
                .all(|v| (v[0] - centre[0]).hypot(v[1] - centre[1]) < 1.51);
        let exterior = planes
            .iter()
            .find(|p| dot(p.n, n) > 0.5 && t.iter().all(|v| (p.d - dot(p.n, *v)).abs() < 0.0001));
        if let Some(p) = exterior {
            // A cut exterior fragment can round inward. Deduct its maximum departure
            // from a source support plane, including arithmetic error, from the wall.
            s.exterior_deviation = t
                .iter()
                .map(|v| p.d - dot(p.n, *v) + p.uncertainty)
                .fold(s.exterior_deviation, f64::max);
        }
        // Classify exterior by plane AND outward orientation, never by a wall-distance
        // threshold that could hide a severely thinned/reversed inner triangle.
        if !bore && exterior.is_none() {
            s.inner_triangles += 1;
            s.wall = distances.into_iter().fold(s.wall, f64::min);
            s.first_inner = t.iter().map(|v| v[2]).fold(s.first_inner, f64::min);
        }
        if bore {
            let h = &mut s.holes[usize::from(t.iter().map(|v| v[2]).sum::<f64>() / 3. > mid)];
            h.triangles += 1;
            for k in 0..3 {
                h.z_range_mm[0] = h.z_range_mm[0].min(t[k][2]);
                h.z_range_mm[1] = h.z_range_mm[1].max(t[k][2]);
                if t[k][2] < mid && distances[k] < 0.01 {
                    s.births.push(t[k][2]);
                }
            }
        }
    }
    // Every model triangle, not just identified bore walls, must clear the axial
    // disk. This detects sealed caps/obstructions and proves full penetration.
    // Projected triangle distance bounds the complete swept vertical disk, not samples.
    for i in 0..written.triangle_count() {
        let t = written.triangle_corners(i);
        let diameter = (2. * triangle_distance(t, centre) - 1e-7).max(0.);
        for h in &mut s.holes {
            let lo = t.iter().map(|v| v[2]).fold(f64::INFINITY, f64::min);
            let hi = t.iter().map(|v| v[2]).fold(f64::NEG_INFINITY, f64::max);
            if lo <= h.z_range_mm[1] && hi >= h.z_range_mm[0] {
                h.clear_diameter_mm = h.clear_diameter_mm.min(diameter);
            }
        }
    }
    s.wall -= s.exterior_deviation;
    Ok(s)
}

pub(super) fn certify_dimensions(original: &Mesh, written: &Mesh) -> Result<(), String> {
    closed(written)?;
    let s = scan(original, written)?;
    require_dimensions(s.wall, s.inner_triangles, &s.holes)
}

/// Measure a closed emitted mesh in the SAME frame as the immutable original.
/// The wall is a lower bound over whole inner triangles, not sampled vertices alone:
/// plane slack is affine, so its minimum on a triangle is attained at a vertex.
/// Review lift is 5 mm; phase diagnostics use that plate grid and 0.05 mm layers.
pub fn measure(original: &Mesh, written: &Mesh) -> Result<Evidence, String> {
    closed(written)?;
    let mut s = scan(original, written)?;
    if s.inner_triangles == 0 || s.holes.iter().any(|h| h.triangles == 0) {
        return Err("missing inner surface or through-bore walls".into());
    }
    s.births.sort_by(f64::total_cmp);
    s.births.dedup_by(|a, b| (*a - *b).abs() < 1e-7);
    let first = *s.births.first().ok_or("missing outer rim births")?;
    let plate = written.bounds().min[2] - 5.;
    let cavity_bottom = plate + ((s.first_inner - plate) / 0.05).floor() * 0.05;
    let mut bounds = vec![[first, first + 0.05], [cavity_bottom, cavity_bottom + 0.05]];
    for birth in &s.births {
        let z = *birth as f32;
        for top in [z.next_down(), z, z.next_up()] {
            let top = f64::from(top);
            bounds.push([top - 0.05, top]);
        }
    }
    bounds.sort_by(|a, b| a[0].total_cmp(&b[0]).then(a[1].total_cmp(&b[1])));
    let mut phases = phases(written, &bounds)?;
    let mut take = |bottom: f64| {
        let i = phases
            .iter()
            .position(|p| {
                p.slab_bottom_mm.to_bits() == bottom.to_bits()
                    && p.slab_top_mm.to_bits() == (bottom + 0.05).to_bits()
            })
            .ok_or("missing phase".to_string())?;
        Ok::<_, String>(phases.remove(i))
    };
    let first_edge_slab = take(first)?;
    let first_cavity_layer = take(cavity_bottom)?;
    let original_mm3 = volume(original);
    let material_mm3 = volume(written);
    let preserved_uncut_triangles = preservation(original, written, s.holes[0].centre_xy_mm)?;
    Ok(Evidence {
        minimum_wall_mm: s.wall,
        exterior_inward_deviation_mm: s.exterior_deviation,
        inner_triangles: s.inner_triangles,
        holes: s.holes,
        cavity_mm3: original_mm3 - material_mm3,
        original_mm3,
        material_mm3,
        triangles: written.triangle_count(),
        preserved_uncut_triangles,
        first_material_z_mm: written.bounds().min[2],
        first_bore_rim_z_mm: first,
        first_inner_z_mm: s.first_inner,
        first_edge_slab,
        first_cavity_layer,
        phases,
    })
}
impl Evidence {
    /// Construction admission, not H5 or independent checker acceptance.
    pub fn require_dimensions(&self) -> Result<(), String> {
        require_dimensions(self.minimum_wall_mm, self.inner_triangles, &self.holes)
    }
}
fn require_dimensions(wall: f64, inner_triangles: usize, holes: &[Hole; 2]) -> Result<(), String> {
    if inner_triangles == 0
        || !wall.is_finite()
        || holes
            .iter()
            .any(|h| h.triangles == 0 || !h.clear_diameter_mm.is_finite())
    {
        return Err("missing finite inner surface or through-bore dimensions".into());
    }
    if wall < WALL {
        return Err(format!("wall {wall:.9} mm below {WALL}"));
    }
    for (i, h) in holes.iter().enumerate() {
        if h.clear_diameter_mm < DIAMETER {
            return Err(format!(
                "hole {i} clear diameter {:.9} mm below {DIAMETER}",
                h.clear_diameter_mm
            ));
        }
    }
    Ok(())
}

/// Which part of a through passage is being measured.
#[derive(Debug, Clone, Copy)]
pub enum BoreExtent {
    ThroughWall,
    WithOutlets { plate_z: f64 },
}

/// Batch surface/prism intersections, including balls, braces, feet and raft triangles.
/// Zero hits alone is not a drainage or complete solid-containment proof.
pub fn bore_intrusions(
    parts: &[Vec<[V; 3]>],
    evidence: &Evidence,
    extent: BoreExtent,
) -> Vec<usize> {
    let centre = evidence.holes[0].centre_xy_mm;
    let mut cutters = Vec::new();
    for (i, h) in evidence.holes.iter().enumerate() {
        let mut z = h.z_range_mm;
        if let BoreExtent::WithOutlets { plate_z } = extent {
            if i == 0 {
                z[0] = plate_z;
            } else {
                z[1] += 5.;
            }
        }
        let mut p = vec![
            Plane {
                n: [0., 0., 1.],
                d: z[1],
                uncertainty: 0.,
            },
            Plane {
                n: [0., 0., -1.],
                d: -z[0],
                uncertainty: 0.,
            },
        ];
        for k in 0..64 {
            let angle = (f64::from(k) + 0.5) * std::f64::consts::TAU / 64.;
            let n = [angle.cos(), angle.sin(), 0.];
            p.push(Plane {
                n,
                d: dot(n, [centre[0], centre[1], 0.]) + h.clear_diameter_mm / 2.,
                uncertainty: 0.,
            });
        }
        cutters.push(p);
    }
    let mut scratch = (Vec::with_capacity(70), Vec::with_capacity(70));
    parts
        .iter()
        .enumerate()
        .filter_map(|(i, tris)| {
            tris.iter()
                .any(|t| cutters.iter().any(|p| intersects(*t, p, &mut scratch)))
                .then_some(i)
        })
        .collect()
}
fn intersects(t: [V; 3], planes: &[Plane], scratch: &mut (Vec<V>, Vec<V>)) -> bool {
    let (polygon, out) = scratch;
    polygon.clear();
    polygon.extend(t);
    for p in planes {
        out.clear();
        for i in 0..polygon.len() {
            let a = polygon[i];
            let b = polygon[(i + 1) % polygon.len()];
            let da = dot(p.n, a) - p.d;
            let db = dot(p.n, b) - p.d;
            if da <= 0. {
                out.push(a);
            }
            if (da < 0. && db > 0.) || (da > 0. && db < 0.) {
                let f = da / (da - db);
                out.push(std::array::from_fn(|k| a[k] + (b[k] - a[k]) * f));
            }
        }
        std::mem::swap(polygon, out);
        if polygon.is_empty() {
            return false;
        }
    }
    true
}
