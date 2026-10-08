//! `plinth-audit geometry`: the head, intrusion, junction and contact-angle
//! numbers of the v19 review, measured from the emitted file alone.
//!
//! These numbers used to come from throwaway scripts. They are measured here
//! from the `.chitubox` bytes only: record headers, the record meshes and the
//! model mesh. Nothing is read from a log, a plan or a planned parameter
//! (Craig caught v19 reporting a 0.96 mm neck the drawn file does not have).
//!
//! Four reports, one module each:
//!
//! | module | report |
//! |---|---|
//! | [`heads`] | every tip whose chain of frusta reaches a joint ball: ball, link, cone-start ring, joint, post lean, phi' |
//! | [`intrusion`] | depth of every support record's header surface inside the model solid |
//! | [`topology`] | record-mesh closure, chain-end rings, slivers, flat shoulders, landed ends, hull facets |
//! | [`contacts`] | straight contacts: arrival angle against the local normal, skim law, shaft clearance |
//!
//! **Batch-first.** One welded model mesh and one grid per file. Per-record
//! work is local: the model triangles near a record (or a short run of its
//! sample rings) are fetched once and every sample point of that run tests
//! against that list. Records run in parallel on `std::thread::scope`. No
//! per-point grid rebuild and no scan of all model triangles per point.
//!
//! **Pre-trim on purpose.** The intrusion check samples the record HEADER
//! (start, end, radii), not the emitted mesh, so the emitter's seat trim
//! cannot hide a penetration.

pub mod contacts;
pub mod heads;
pub mod intrusion;
pub mod report;
pub mod topology;

use crate::consts as c;
use crate::graph::SupportGraph;
use crate::mesh::{Mesh, RayHit};
use crate::primitives::Primitive;
use crate::reader::{ChituboxFile, ModelEntry, RecordKind, SupportRecord};
use crate::vec3::{self as v, V3};
use std::sync::atomic::{AtomicUsize, Ordering};

/// Endpoint coincidence tolerance for chain walking. Our own emitter writes
/// every shared end from one `f32` value, so true coincidences agree to about
/// 1e-6 mm (`audit-contract.md` §3.2); 1e-3 mm is the registered emitted-file
/// tolerance and is used here instead of the graph's looser native 0.02 mm.
pub const CHAIN_TOL_MM: f64 = c::JOIN_TOL_EMITTED_MM;

/// The registered numbers the geometry report reads. Never literals in a check.
#[derive(Clone, Copy, Debug)]
pub struct Params {
    /// `g`, the final clearance gap.
    pub gap_mm: f64,
    /// One layer.
    pub layer_mm: f64,
    /// One XY pixel: the larger registered pitch, as the engine's
    /// `registered_pixel_pitch_mm` takes it.
    pub pixel_mm: f64,
    /// Worker threads.
    pub threads: usize,
}

impl Params {
    /// `layer + pixel`, the seat law's rasterisation allowance.
    #[must_use]
    pub fn allowance_mm(&self) -> f64 {
        self.layer_mm + self.pixel_mm
    }
}

impl Default for Params {
    fn default() -> Self {
        Self {
            gap_mm: c::FINAL_CLEARANCE_GAP_MM,
            layer_mm: c::LAYER_HEIGHT_DEFAULT_MM,
            pixel_mm: c::PIXEL_PITCH_X_MM.max(c::PIXEL_PITCH_Y_MM),
            threads: default_threads(),
        }
    }
}

/// `PLINTH_THREADS` when set, else the available parallelism.
#[must_use]
pub fn default_threads() -> usize {
    std::env::var("PLINTH_THREADS")
        .ok()
        .and_then(|s| s.parse::<usize>().ok())
        .filter(|&n| n > 0)
        .unwrap_or_else(|| {
            std::thread::available_parallelism().map_or(1, std::num::NonZeroUsize::get)
        })
}

/// Everything the four reports share for one model.
pub struct GeoCtx<'a> {
    pub model: &'a ModelEntry,
    /// The model mesh, welded by exact `f32` bits like every other check.
    pub mesh: Mesh,
    pub graph: SupportGraph,
    /// One analytic solid per record, `None` when the record is degenerate.
    pub prims: Vec<Option<Primitive>>,
    pub params: Params,
}

impl<'a> GeoCtx<'a> {
    #[must_use]
    pub fn build(file: &ChituboxFile, model: &'a ModelEntry, params: Params) -> Self {
        let mesh = Mesh::weld_exact(&model.model_soup);
        let graph = SupportGraph::build(model, file.file_class.join_tol_mm(), file.file_class.label());
        let prims = model
            .records
            .iter()
            .map(|r| Primitive::from_record(r).ok())
            .collect();
        Self {
            model,
            mesh,
            graph,
            prims,
            params,
        }
    }

    #[must_use]
    pub fn rec(&self, i: usize) -> &SupportRecord {
        &self.model.records[i]
    }

    /// `(record, slot)` pairs of every record joined to `p` within
    /// [`CHAIN_TOL_MM`], among `from`'s graph neighbours (the graph's native
    /// tolerance is wider, so its neighbours are a superset of the true ones).
    /// Slot 0 / 1 is a frustum's start / end; slot 2 is a ball centre.
    #[must_use]
    pub fn joined_at(&self, from: usize, p: V3) -> Vec<(usize, usize)> {
        let mut out = Vec::new();
        for &n in self.graph.neighbours_of(from) {
            if n == from {
                continue;
            }
            let r = self.rec(n);
            if r.kind.is_ball() {
                if r.ball_centre().is_some_and(|ctr| v::dist(ctr, p) <= CHAIN_TOL_MM) {
                    out.push((n, 2));
                }
            } else if r.kind.is_frustum() {
                if v::dist(r.start_mm, p) <= CHAIN_TOL_MM {
                    out.push((n, 0));
                }
                if v::dist(r.end_mm, p) <= CHAIN_TOL_MM {
                    out.push((n, 1));
                }
            }
        }
        out
    }

    /// Ray-parity inside test against the model mesh. Unlike
    /// [`Mesh::point_inside`] it does not refuse a mesh with a few
    /// non-manifold edges: it tries the 16 fixed directions and returns `None`
    /// only when every one grazes. The query point is never perturbed.
    #[must_use]
    pub fn inside(&self, p: V3, hits: &mut Vec<RayHit>) -> Option<bool> {
        let tol = c::tol_geom_at(p);
        if !self.mesh.bbox.expanded(tol).contains(p) {
            return Some(false);
        }
        for k in 0..v::RAY_DIRECTION_BUDGET {
            let mut grazed = false;
            self.mesh
                .ray_crossings(p, v::golden_spiral_direction(k), tol, hits, &mut grazed);
            if !grazed {
                return Some(hits.len() % 2 == 1);
            }
        }
        None
    }

    /// Unsigned distance to the model surface and the inside flag. The
    /// distance search is unbounded (expanding grid shells).
    #[must_use]
    pub fn signed_to_model(&self, p: V3, hits: &mut Vec<RayHit>) -> (f64, Option<bool>) {
        let d = self
            .mesh
            .nearest_surface(p)
            .map_or(f64::INFINITY, |h| h.distance_mm);
        (d, self.inside(p, hits))
    }
}

/// Run `f(0..n)` on `threads` workers, results in index order.
pub fn par_map<T: Send, F: Fn(usize) -> T + Sync>(n: usize, threads: usize, f: F) -> Vec<T> {
    let next = AtomicUsize::new(0);
    let workers = threads.clamp(1, n.max(1));
    let mut chunks: Vec<Vec<(usize, T)>> = Vec::new();
    std::thread::scope(|s| {
        let handles: Vec<_> = (0..workers)
            .map(|_| {
                s.spawn(|| {
                    let mut mine = Vec::new();
                    loop {
                        let i = next.fetch_add(1, Ordering::Relaxed);
                        if i >= n {
                            break;
                        }
                        mine.push((i, f(i)));
                    }
                    mine
                })
            })
            .collect();
        for h in handles {
            chunks.push(h.join().expect("geometry worker panicked"));
        }
    });
    let mut flat: Vec<(usize, T)> = chunks.into_iter().flatten().collect();
    flat.sort_by_key(|(i, _)| *i);
    flat.into_iter().map(|(_, t)| t).collect()
}

/// An oriented frustum: `p0` to `p1`, radius `r0` to `r1`.
#[derive(Clone, Copy, Debug)]
pub struct Seg {
    pub record: usize,
    pub p0: V3,
    pub r0: f64,
    pub p1: V3,
    pub r1: f64,
}

impl Seg {
    /// The record oriented so that travel enters at `slot` (0 = start first).
    #[must_use]
    pub fn from_record(i: usize, r: &SupportRecord, enter_at_start: bool) -> Self {
        if enter_at_start {
            Self {
                record: i,
                p0: r.start_mm,
                r0: r.r_start,
                p1: r.end_mm,
                r1: r.r_end,
            }
        } else {
            Self {
                record: i,
                p0: r.end_mm,
                r0: r.r_end,
                p1: r.start_mm,
                r1: r.r_start,
            }
        }
    }
    #[must_use]
    pub fn len(&self) -> f64 {
        v::dist(self.p0, self.p1)
    }
    #[must_use]
    pub fn dir(&self) -> Option<V3> {
        v::normalize(v::sub(self.p1, self.p0))
    }
}

/// `n` points of a circle of radius `r` about `centre` in the plane
/// perpendicular to `axis`, starting at angle `phase` (radians).
#[must_use]
pub fn ring_points(centre: V3, axis: V3, r: f64, n: usize, phase: f64) -> Vec<V3> {
    let (e1, e2, _) = v::basis_from_axis(axis);
    (0..n)
        .map(|i| {
            let t = phase + 2.0 * std::f64::consts::PI * i as f64 / n as f64;
            v::add(
                centre,
                v::add(v::mul(e1, r * t.cos()), v::mul(e2, r * t.sin())),
            )
        })
        .collect()
}

/// Whether a record kind is one of the three right-frustum kinds the chains
/// are made of (excluding the root flare, which stands on the raft).
#[must_use]
pub const fn is_chain_frustum(k: RecordKind) -> bool {
    matches!(k, RecordKind::ContactTaper | RecordKind::Member)
}

/// Median of a slice (mean of the middle two when even); `None` when empty.
#[must_use]
pub fn median(vals: &[f64]) -> Option<f64> {
    if vals.is_empty() {
        return None;
    }
    let mut s = vals.to_vec();
    s.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let n = s.len();
    Some(if n % 2 == 1 {
        s[n / 2]
    } else {
        0.5 * (s[n / 2 - 1] + s[n / 2])
    })
}
