//! Axial mouth discovery from the material raster; widths and intersections then measured
//! against the original f32 triangles. The grid locates work, it does not declare a hole.

use super::grid::{FREE, Grid};
use super::{Bore, CELL_MM};
use crate::gap::{self, Goal};
use crate::outcome::at;
use crate::scene::Scene;
use crate::solid::Solid;
use crate::status::Located;
use crate::tri::{Tri, point_in_mesh};
use crate::v3::{Aabb, V};
use std::collections::{BTreeMap, BTreeSet};

/// Separate openings must never be averaged into a centroid in the intervening wall.
fn split_openings(g: &Grid, open: &[usize], axis: usize) -> Vec<Vec<usize>> {
    let mut remaining: BTreeSet<_> = open.iter().copied().collect();
    let mut out = Vec::new();
    while let Some(seed) = remaining.pop_first() {
        let mut component = vec![seed];
        let mut at = 0;
        while at < component.len() {
            let xyz = g.xyz(component[at]);
            at += 1;
            for k in (0..3).filter(|&k| k != axis) {
                for next in [
                    xyz[k].checked_sub(1),
                    (xyz[k] + 1 < g.n[k]).then_some(xyz[k] + 1),
                ]
                .into_iter()
                .flatten()
                {
                    let mut p = xyz;
                    p[k] = next;
                    let j = g.index(p[0], p[1], p[2]);
                    if remaining.remove(&j) {
                        component.push(j);
                    }
                }
            }
        }
        out.push(component);
    }
    out
}

/// The axial wall triangles give the real throat ends, not the model's bounding box.
fn throat_span(scene: &Scene, c: V, axis: usize, radius: f64, inner: f64) -> Option<[f64; 2]> {
    let (u, v) = ((axis + 1) % 3, (axis + 2) % 3);
    let mut span = [f64::INFINITY, f64::NEG_INFINITY];
    for t in &scene.model {
        let n = crate::v3::cross(crate::v3::sub(t[1], t[0]), crate::v3::sub(t[2], t[0]));
        if n[axis].abs() > 1e-8 * crate::v3::len(n) {
            continue;
        }
        let projected = t.map(|p| [p[u], p[v], p[axis]]);
        if projected_distance(&projected, [c[u], c[v]]) > radius + 1e-6 {
            continue;
        }
        let bb = crate::tri::tri_box(t);
        let range = [
            c[axis].min(inner) - 2.0 * CELL_MM,
            c[axis].max(inner) + 2.0 * CELL_MM,
        ];
        if bb.hi[axis] < range[0] || bb.lo[axis] > range[1] {
            continue;
        }
        span[0] = span[0].min(bb.lo[axis]);
        span[1] = span[1].max(bb.hi[axis]);
    }
    (span[0].is_finite() && span[1] > span[0]).then_some(span)
}

/// Distance of a projected triangle/polygon from an XY point. A triangle can cross the
/// disk without putting any vertex in it, so vertex-only distances are not sufficient.
fn projected_distance(poly: &[V], c: [f64; 2]) -> f64 {
    if poly.is_empty() {
        return f64::INFINITY;
    }
    let mut lo = f64::INFINITY;
    let (mut positive, mut negative) = (false, false);
    for i in 0..poly.len() {
        let (a, b) = (poly[i], poly[(i + 1) % poly.len()]);
        let d = [b[0] - a[0], b[1] - a[1]];
        let cross = d[0] * (c[1] - a[1]) - d[1] * (c[0] - a[0]);
        positive |= cross > 1e-12;
        negative |= cross < -1e-12;
        let len2 = d[0] * d[0] + d[1] * d[1];
        let s = if len2 > 0.0 {
            ((c[0] - a[0]) * d[0] + (c[1] - a[1]) * d[1]) / len2
        } else {
            0.0
        }
        .clamp(0.0, 1.0);
        lo = lo.min((a[0] + s * d[0] - c[0]).hypot(a[1] + s * d[1] - c[1]));
    }
    // A projected wall can be a line, not a polygon. Do not call its infinite supporting
    // line a filled disk.
    let area = 0.5
        * (0..poly.len())
            .map(|i| {
                let (a, b) = (poly[i], poly[(i + 1) % poly.len()]);
                a[0] * b[1] - a[1] * b[0]
            })
            .sum::<f64>()
            .abs();
    if area > 1e-14 && !(positive && negative) {
        0.0
    } else {
        lo
    }
}

fn width(scene: &Scene, c: V, span: [f64; 2], axis: usize) -> f64 {
    let u = (axis + 1) % 3;
    let v = (axis + 2) % 3;
    let mut bb = Aabb {
        lo: c.map(|x| x - 20.0),
        hi: c.map(|x| x + 20.0),
    };
    bb.lo[axis] = span[0];
    bb.hi[axis] = span[1];
    let mut direction = [0.0; 3];
    direction[axis] = 1.0;
    (scene
        .model_near(&bb)
        .into_iter()
        .map(|i| {
            let p = crate::poly::clip_all(
                &scene.model[i as usize],
                &[(direction.map(|x| -x), -span[0]), (direction, span[1])],
            );
            let projected: Vec<V> = p.iter().map(|p| [p[u], p[v], p[axis]]).collect();
            projected_distance(&projected, [c[u], c[v]])
        })
        .fold(20.0, f64::min)
        - 1e-7)
        .max(0.0)
}

pub(super) fn find(
    scene: &Scene,
    g: &Grid,
    chamber: usize,
    cells: &[usize],
    reasons: &mut Vec<String>,
) -> Vec<Bore> {
    (0..3)
        .flat_map(|axis| find_axis(scene, g, chamber, cells, axis, reasons))
        .collect()
}

fn find_axis(
    scene: &Scene,
    g: &Grid,
    chamber: usize,
    cells: &[usize],
    axis: usize,
    reasons: &mut Vec<String>,
) -> Vec<Bore> {
    let mut layers: BTreeMap<usize, Vec<usize>> = BTreeMap::new();
    for &i in cells {
        layers.entry(g.xyz(i)[axis]).or_default().push(i);
    }
    let Some((&first, a)) = layers.first_key_value() else {
        return vec![];
    };
    let (&last, b) = layers.last_key_value().expect("nonempty");
    // Batch by transverse column, not by cavity cell. An opening on a local end but
    // not the chamber's extreme layer is outside this detector's current inventory.
    let (u, v) = ((axis + 1) % 3, (axis + 2) % 3);
    let mut columns: BTreeMap<[usize; 2], [usize; 2]> = BTreeMap::new();
    for &i in cells {
        let p = g.xyz(i);
        let ends = columns.entry([p[u], p[v]]).or_insert([p[axis]; 2]);
        ends[0] = ends[0].min(p[axis]);
        ends[1] = ends[1].max(p[axis]);
    }
    let off_extreme = columns.iter().any(|(&uv, ends)| {
        [(ends[0], first, false), (ends[1], last, true)]
            .into_iter()
            .any(|(end, extreme, up)| {
                if end.abs_diff(extreme) <= 1 {
                    return false;
                }
                let mut p = [0; 3];
                p[u] = uv[0];
                p[v] = uv[1];
                let range = if up { end + 1..g.n[axis] } else { 0..end };
                range.into_iter().all(|k| {
                    p[axis] = k;
                    g.model[g.index(p[0], p[1], p[2])] == FREE
                })
            })
    });
    if off_extreme {
        reasons.push(format!("chamber {chamber} axis {axis}: opening not on the chamber's extreme layer; mouth inventory is incomplete"));
    }
    let mut out = Vec::new();
    for (z, mouth_cells, up) in [(first, a, false), (last, b, true)] {
        // A through-wall mouth must open to definite exterior along its outward direction.
        // This is checked on every cell column of the opening, not on a centreline.
        let open: Vec<_> = mouth_cells
            .iter()
            .copied()
            .filter(|&i| {
                let mut xyz = g.xyz(i);
                let zz = xyz[axis];
                let range = if up { zz + 1..g.n[axis] } else { 0..zz };
                range.into_iter().all(|k| {
                    xyz[axis] = k;
                    g.model[g.index(xyz[0], xyz[1], xyz[2])] == FREE
                })
            })
            .collect();
        if open.is_empty() {
            continue;
        }
        for open in split_openings(g, &open, axis) {
            let n = open.len() as f64;
            let c: V = open.iter().fold([0.0; 3], |s, &i| {
                let p = g.point(i);
                std::array::from_fn(|k| s[k] + p[k] / n)
            });
            let mouth_z = c[axis] + if up { CELL_MM } else { -CELL_MM };
            let inner = if up {
                layers
                    .range(..=z)
                    .rev()
                    .find(|(_, v)| v.len() > 2 * open.len())
                    .map(|(k, _)| *k)
            } else {
                layers
                    .range(z..)
                    .find(|(_, v)| v.len() > 2 * open.len())
                    .map(|(k, _)| *k)
            };
            let Some(inner) = inner else {
                reasons.push(format!("chamber {chamber} axis {axis}: straight through-bore or open recess has no wider chamber behind its opening"));
                continue;
            };
            let inner_z = g.origin[axis] + (inner as f64 + 0.5) * CELL_MM;
            let span = [mouth_z.min(inner_z), mouth_z.max(inner_z)];
            let radius = width(scene, c, span, axis);
            if radius > CELL_MM {
                let Some(span) = throat_span(scene, c, axis, radius, inner_z) else {
                    reasons.push(format!("chamber {chamber} axis {axis}: axial throat outer end on the model surface could not be resolved"));
                    continue;
                };
                let mut mouth = c;
                mouth[axis] = if up { span[1] } else { span[0] };
                out.push(Bore {
                    centre: c,
                    axis,
                    span,
                    mouth,
                    radius,
                    chamber,
                });
            }
        }
    }
    out
}

fn intersects(zone: &Solid, tris: &[Tri]) -> (bool, bool) {
    let bb = zone.aabb();
    let mut unknown = false;
    let goal = Goal {
        clear_at: 1e-6,
        violation_below: 1e-8,
        width: 1e-7,
    };
    for t in tris.iter().filter(|t| crate::tri::tri_box(t).overlaps(&bb)) {
        let d = gap::solid_tri(zone, t, goal);
        if d.ub < 1e-8 {
            return (true, unknown);
        }
        unknown |= d.lb <= 1e-8 || d.exhausted;
    }
    // A large raft can contain the whole bore without a surface crossing it.
    let centre = bb.centre();
    match point_in_mesh(centre, tris) {
        Some(true) => (true, unknown),
        Some(false) => (false, unknown),
        None => (false, true),
    }
}

/// Every decoded mesh is tested, including invalid/unknown records. Fields, chains, role
/// labels and column axes are not an exclusion filter. A local ball is NOT a plate strip.
pub(super) fn intrusions(scene: &Scene, bores: &[Bore]) -> (Vec<Located>, usize) {
    let mut found = Vec::new();
    let mut undecided = 0;
    for p in &scene.parts {
        let mut maybe = false;
        for b in bores {
            let mut start = b.centre;
            let mut end = b.centre;
            start[b.axis] = b.span[0] + 1e-6;
            end[b.axis] = b.span[1] - 1e-6;
            let zones = [
                Solid::frustum(
                    start,
                    end,
                    (b.radius - 1e-6).max(0.0),
                    (b.radius - 1e-6).max(0.0),
                ),
                Some(Solid::Ball {
                    c: b.mouth,
                    r: (scene.limits.mouth_clearance_mm - 1e-6).max(0.0),
                }),
            ];
            let mut hit = None;
            for (kind, zone) in ["bore", "local mouth keep-out"].into_iter().zip(zones) {
                let Some(zone) = zone else {
                    maybe = true;
                    continue;
                };
                if !zone.aabb().overlaps(&p.bbox) {
                    continue;
                }
                let (yes, unknown) = intersects(&zone, &p.tris);
                maybe |= unknown;
                if yes {
                    hit = Some(kind);
                    break;
                }
            }
            if let Some(kind) = hit {
                found.push(at(Some(p.rec),Some(scene.world(b.mouth)),None,format!(
                    "{} enters a geometrically detected {kind} of chamber {}; tested the complete written mesh, not its axis",p.role.name(),b.chamber
                )));
                break;
            }
        }
        if maybe
            && !found
                .iter()
                .any(|l| l.location.record == u32::try_from(p.rec).ok())
        {
            undecided += 1;
        }
    }
    (found, undecided)
}
