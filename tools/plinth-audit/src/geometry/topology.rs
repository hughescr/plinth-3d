//! Report 3: junction topology.
//!
//! Per record mesh (welded at [`WELD_TOL_MM`]): open edges, non-manifold
//! edges, degenerate triangles, signed volume. Then the chain checks that need
//! two records at once: end rings that should coincide, frusta shorter than a
//! pixel, radius steps with no cone between them, and the ends of branches
//! that land in another support.
//!
//! The record meshes are read as emitted. The ring vertices of a frustum end
//! are the mesh vertices on the end plane, not points rebuilt from the header.

use super::{is_chain_frustum, par_map, GeoCtx};
use crate::mesh::Mesh;
use crate::primitives::Primitive;
use crate::reader::{RecordKind, SupportRecord};
use crate::vec3::{self as v, Aabb, V3};
use std::collections::HashSet;

/// Weld tolerance for a record's own soup, mm.
pub const WELD_TOL_MM: f64 = 1.0e-5;
/// Ring vertices of an end: mesh vertices within this of the end plane.
const RING_PLANE_TOL_MM: f64 = 2.0e-3;
/// Mismatch between two rings that should be one ring, mm.
pub const RING_MISMATCH_MM: f64 = 1.0e-4;
/// Two joined frusta are "equal radius" when their end radii differ by less
/// than this, mm.
const EQUAL_RADIUS_MM: f64 = 1.0e-4;
/// Two joined frusta are "collinear" when the bend between their axes is
/// below this, radians.
const COLLINEAR_RAD: f64 = 1.0e-3;
/// A radius step smaller than this is rounding, not a shoulder, mm.
pub const STEP_MIN_MM: f64 = 1.0e-3;
/// Report threshold for very short frusta, mm (the plan's sliver figure).
pub const SLIVER_TINY_MM: f64 = 0.01;
/// Landed ends whose rim stands proud of every other solid by at least these.
pub const PROUD_LEVELS_MM: [f64; 2] = [0.2, 0.3];
/// A facet whose normal is within this of the record axis is a cap, not a
/// lateral facet.
const CAP_FACET_COS: f64 = 0.999;

#[derive(Clone, Debug, Default)]
pub struct MeshCounts {
    pub records: usize,
    pub open_edges: usize,
    pub open_records: usize,
    pub nonmanifold_edges: usize,
    pub nonmanifold_records: usize,
    pub inconsistent_edges: usize,
    pub degenerate_triangles: usize,
    pub degenerate_records: usize,
    /// Records whose signed volume is not positive.
    pub nonpositive_volume: Vec<usize>,
}

#[derive(Clone, Debug)]
pub struct RingMismatch {
    pub a: usize,
    pub b: usize,
    pub mismatch_mm: f64,
}

#[derive(Clone, Debug)]
pub struct Shoulder {
    pub a: usize,
    pub b: usize,
    pub step_mm: f64,
    pub bend_deg: f64,
}

#[derive(Clone, Debug)]
pub struct Landed {
    pub record: usize,
    /// 0 = the start end, 1 = the end end.
    pub slot: usize,
    pub ring_vertices: usize,
    pub outside_vertices: usize,
    /// The largest distance of a ring vertex from the UNION of every other
    /// solid (0 when every vertex is inside some other solid).
    pub proud_mm: f64,
    /// The record that holds the end centre deepest, the landing host.
    pub host: Option<usize>,
    /// The largest distance of a ring vertex outside the HOST alone, as the
    /// v19 diagnosis measured it.
    pub proud_host_mm: f64,
}

#[derive(Clone, Debug)]
pub struct HullRow {
    pub record: usize,
    pub facets: usize,
    pub min_lateral_nz: Option<f64>,
}

#[derive(Clone, Debug, Default)]
pub struct TopologyReport {
    pub mesh: MeshCounts,
    pub ring_pairs_checked: usize,
    pub ring_mismatches: Vec<RingMismatch>,
    pub slivers_pixel: Vec<(usize, f64)>,
    pub slivers_tiny: Vec<(usize, f64)>,
    pub shoulders: Vec<Shoulder>,
    pub landed: Vec<Landed>,
    /// Members whose mesh does not lie on the analytic frustum of their
    /// header: hull records (or trimmed ones).
    pub hulls: Vec<HullRow>,
    /// One XY pixel, mm: the smallest step the printer renders.
    pub pixel_mm: f64,
}

impl TopologyReport {
    /// Flat shoulders: radius steps wider than one pixel between two frusta
    /// that continue each other in a straight line (a collinear chain), with
    /// no cone between. A step across a real bend is a different thing (the
    /// landing of a branch) and is listed in `shoulders` but not counted here.
    #[must_use]
    pub fn flat_shoulders(&self) -> Vec<&Shoulder> {
        self.shoulders
            .iter()
            .filter(|s| s.step_mm > self.pixel_mm && s.bend_deg <= COLLINEAR_RAD.to_degrees())
            .collect()
    }
    #[must_use]
    pub fn landed_proud_at_least(&self, mm: f64) -> usize {
        self.landed.iter().filter(|l| l.proud_mm >= mm).count()
    }
    #[must_use]
    pub fn landed_host_proud_at_least(&self, mm: f64) -> usize {
        self.landed.iter().filter(|l| l.proud_host_mm >= mm).count()
    }
}

struct RecMesh {
    open: usize,
    nonmanifold: usize,
    inconsistent: usize,
    degenerate: usize,
    volume_ok: bool,
}

fn mesh_of(rec: &SupportRecord) -> Option<(Mesh, RecMesh)> {
    if rec.soup.is_empty() {
        return None;
    }
    let m = Mesh::weld_tolerant(&rec.soup, WELD_TOL_MM);
    let zero_area = m
        .areas
        .iter()
        .zip(&m.tris)
        .filter(|(a, t)| **a <= 0.0 && !(t[0] == t[1] || t[1] == t[2] || t[2] == t[0]))
        .count();
    let rm = RecMesh {
        open: m.closure.boundary_edges,
        nonmanifold: m.closure.nonmanifold_edges,
        inconsistent: m.closure.inconsistently_wound_edges,
        degenerate: m.closure.degenerate_triangles + zero_area,
        volume_ok: m.raw_signed_volume_mm3 > 0.0,
    };
    Some((m, rm))
}

/// The mesh vertices of `rec` on the end plane through `end` (axis `u`,
/// pointing away from the record's body), off the axis by at least half the
/// end radius. Unique by bit pattern.
fn end_ring(rec: &SupportRecord, end: V3, u: V3, r_end: f64) -> Vec<V3> {
    let mut seen: HashSet<[u32; 3]> = HashSet::new();
    let mut out = Vec::new();
    for i in 0..rec.soup.len() {
        for j in 0..3 {
            let key = rec.soup.bits(i, j);
            if !seen.insert(key) {
                continue;
            }
            let p = rec.soup.v(i, j);
            let d = v::sub(p, end);
            let ax = v::dot(d, u);
            if ax.abs() > RING_PLANE_TOL_MM {
                continue;
            }
            let radial = v::norm(v::sub(d, v::mul(u, ax)));
            if radial >= 0.5 * r_end {
                out.push(p);
            }
        }
    }
    out
}

fn ring_mismatch(a: &[V3], b: &[V3]) -> Option<f64> {
    if a.is_empty() || b.is_empty() {
        return None;
    }
    let one_way = |x: &[V3], y: &[V3]| {
        x.iter()
            .map(|p| y.iter().map(|q| v::dist(*p, *q)).fold(f64::INFINITY, f64::min))
            .fold(0.0, f64::max)
    };
    Some(one_way(a, b).max(one_way(b, a)))
}

fn end_of(rec: &SupportRecord, slot: usize) -> (V3, V3, f64) {
    // (end point, outward unit axis at that end, radius there)
    if slot == 0 {
        (
            rec.start_mm,
            v::normalize(v::sub(rec.start_mm, rec.end_mm)).unwrap_or([0.0, 0.0, 1.0]),
            rec.r_start,
        )
    } else {
        (
            rec.end_mm,
            v::normalize(v::sub(rec.end_mm, rec.start_mm)).unwrap_or([0.0, 0.0, 1.0]),
            rec.r_end,
        )
    }
}

#[must_use]
pub fn measure(ctx: &GeoCtx) -> TopologyReport {
    let n = ctx.model.records.len();
    let mut rep = TopologyReport {
        pixel_mm: ctx.params.pixel_mm,
        ..TopologyReport::default()
    };

    // --- per-record mesh closure -------------------------------------------
    let rows = par_map(n, ctx.params.threads, |i| {
        let rec = ctx.rec(i);
        let (mesh, rm) = mesh_of(rec)?;
        let hull = hull_row(ctx, i, &mesh);
        Some((rm, hull))
    });
    for (i, row) in rows.into_iter().enumerate() {
        let Some((rm, hull)) = row else { continue };
        rep.mesh.records += 1;
        rep.mesh.open_edges += rm.open;
        rep.mesh.open_records += usize::from(rm.open > 0);
        rep.mesh.nonmanifold_edges += rm.nonmanifold;
        rep.mesh.nonmanifold_records += usize::from(rm.nonmanifold > 0);
        rep.mesh.inconsistent_edges += rm.inconsistent;
        rep.mesh.degenerate_triangles += rm.degenerate;
        rep.mesh.degenerate_records += usize::from(rm.degenerate > 0);
        if !rm.volume_ok {
            rep.mesh.nonpositive_volume.push(i);
        }
        if let Some(h) = hull {
            rep.hulls.push(h);
        }
    }

    // --- slivers ------------------------------------------------------------
    for i in 0..n {
        let r = ctx.rec(i);
        if r.kind.is_frustum() {
            let l = r.axis_length_mm();
            if l < ctx.params.pixel_mm {
                rep.slivers_pixel.push((i, l));
            }
            if l < SLIVER_TINY_MM {
                rep.slivers_tiny.push((i, l));
            }
        }
    }

    // --- joins between two frusta: rings and shoulders ----------------------
    for a in 0..n {
        let ra = ctx.rec(a);
        if !ra.kind.is_frustum() {
            continue;
        }
        for sa in 0..2 {
            let (pa, ua, rad_a) = end_of(ra, sa);
            let joined = ctx.joined_at(a, pa);
            // A ball at the point (joint or tip) makes it a joint, not a step.
            if joined.iter().any(|(r, _)| ctx.rec(*r).kind.is_ball()) {
                continue;
            }
            let frusta: Vec<_> = joined
                .iter()
                .filter(|(r, _)| ctx.rec(*r).kind.is_frustum())
                .collect();
            if frusta.len() != 1 {
                continue; // an end, or a three-way branch: not a chain step
            }
            let &(b, sb) = frusta[0];
            if b < a {
                continue;
            }
            let rb = ctx.rec(b);
            let (_, ub, rad_b) = end_of(rb, sb);
            // Bend between the two bodies: axes pointing INTO each body are
            // -ua and -ub; a straight continuation has ua == -ub.
            let bend = v::dot(ua, v::mul(ub, -1.0)).clamp(-1.0, 1.0).acos();
            let step = (rad_a - rad_b).abs();
            if step <= EQUAL_RADIUS_MM && bend <= COLLINEAR_RAD {
                rep.ring_pairs_checked += 1;
                let ring_a = end_ring(ra, pa, ua, rad_a);
                let ring_b = end_ring(rb, pa, ub, rad_b);
                if let Some(m) = ring_mismatch(&ring_a, &ring_b) {
                    if m > RING_MISMATCH_MM {
                        rep.ring_mismatches.push(RingMismatch { a, b, mismatch_mm: m });
                    }
                }
            }
            if step > STEP_MIN_MM {
                rep.shoulders.push(Shoulder {
                    a,
                    b,
                    step_mm: step,
                    bend_deg: bend.to_degrees(),
                });
            }
        }
    }

    // --- landed branch ends --------------------------------------------------
    let boxes: Vec<Option<Aabb>> = ctx.prims.iter().map(|p| p.map(|p| p.aabb())).collect();
    let solid_ids: Vec<usize> = (0..n)
        .filter(|&i| {
            ctx.prims[i].is_some()
                && matches!(
                    ctx.rec(i).kind,
                    RecordKind::ContactTaper
                        | RecordKind::Member
                        | RecordKind::RootFlare
                        | RecordKind::JointSphere
                )
        })
        .collect();
    let mut ends: Vec<(usize, usize)> = Vec::new();
    for &i in &solid_ids {
        if !is_chain_frustum(ctx.rec(i).kind) {
            continue;
        }
        for slot in 0..2 {
            let (p, _, _) = end_of(ctx.rec(i), slot);
            if !ctx.joined_at(i, p).is_empty() {
                continue; // joined to something: not a free end
            }
            let inside_other = solid_ids.iter().any(|&j| {
                j != i
                    && boxes[j].is_some_and(|b| b.contains(p))
                    && ctx.prims[j].is_some_and(|pr| pr.contains(p))
            });
            if inside_other {
                ends.push((i, slot));
            }
        }
    }
    let landed = par_map(ends.len(), ctx.params.threads, |k| {
        let (i, slot) = ends[k];
        let rec = ctx.rec(i);
        let (p, u, rad) = end_of(rec, slot);
        let ring = end_ring(rec, p, u, rad);
        let mut outside = 0usize;
        let mut proud = 0.0f64;
        // The host: the other solid that holds the end centre deepest.
        let mut host: Option<(f64, usize)> = None;
        for &j in &solid_ids {
            if j == i {
                continue;
            }
            if let Some(pr) = ctx.prims[j] {
                let s = pr.signed_distance(p);
                if s <= 0.0 && host.is_none_or(|(hs, _)| s < hs) {
                    host = Some((s, j));
                }
            }
        }
        let mut proud_host = 0.0f64;
        if let Some((_, h)) = host {
            if let Some(pr) = ctx.prims[h] {
                for q in &ring {
                    proud_host = proud_host.max(pr.signed_distance(*q));
                }
            }
        }
        for q in &ring {
            let mut m = f64::INFINITY;
            for &j in &solid_ids {
                if j == i {
                    continue;
                }
                let Some(b) = boxes[j] else { continue };
                if !b.expanded(rad.max(1.0)).contains(*q) {
                    continue;
                }
                if let Some(pr) = ctx.prims[j] {
                    m = m.min(pr.signed_distance(*q));
                }
            }
            if m > 0.0 && m.is_finite() {
                outside += 1;
                proud = proud.max(m);
            }
        }
        Landed {
            record: i,
            slot,
            ring_vertices: ring.len(),
            outside_vertices: outside,
            proud_mm: proud,
            host: host.map(|(_, h)| h),
            proud_host_mm: proud_host,
        }
    });
    rep.landed = landed;
    rep
}

/// A Member whose mesh does not lie on its header frustum: its smallest
/// lateral facet `n_z` (outward). Cap facets (normal along the header axis)
/// are not lateral.
fn hull_row(ctx: &GeoCtx, i: usize, mesh: &Mesh) -> Option<HullRow> {
    let rec = ctx.rec(i);
    if rec.kind != RecordKind::Member {
        return None;
    }
    let prim = ctx.prims[i]?;
    if prim.verify_against_soup(rec).agrees {
        return None;
    }
    let Primitive::Frustum { a, b, .. } = prim else {
        return None;
    };
    let axis = v::normalize(v::sub(b, a))?;
    let mut min_nz: Option<f64> = None;
    for (t, nrm) in mesh.normals.iter().enumerate() {
        if mesh.areas[t] <= 0.0 || v::norm(*nrm) == 0.0 {
            continue;
        }
        if v::dot(*nrm, axis).abs() >= CAP_FACET_COS {
            continue;
        }
        // `Mesh` normals carry the mesh's own global orientation sign, so
        // they point out of a positive-volume record.
        min_nz = Some(min_nz.map_or(nrm[2], |m: f64| m.min(nrm[2])));
    }
    Some(HullRow {
        record: i,
        facets: mesh.tri_count(),
        min_lateral_nz: min_nz,
    })
}
