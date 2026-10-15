//! The immutable per-layer cache of one support plan (perf lane D1,
//! 2026-10-15), under a byte budget.
//!
//! Four things the sweeps read are functions of the SLICES and the GRID alone,
//! never of the evolving strut set:
//!
//! * the layer's two masks (`cover`, `carry`, [`crate::raster`]);
//! * the clearance field of a layer's `cover` (`reach::clearance_mm`), which
//!   the down-sweep's routing field reads for the layer BELOW it;
//! * the 4-connected components of the layer's MODEL material (`carry`), the
//!   component of each model cell and the component cell counts the
//!   model-section check sums a load raster over;
//! * whether the layer has any cover at all, and its carrier-cell count.
//!
//! Before this module every down-sweep pass (14 to 35 of them per plan, each
//! over 380 layers on Deinonychus) rebuilt all four, as did the up-sweep and
//! the removal replay. Now one [`LayerCache`] is built per plan, in parallel
//! across layers, and read by every consumer through a [`LayerReader`].
//!
//! # The byte budget
//!
//! A cache holds about `12 * cells` bytes per layer in the worst case (two
//! byte masks, a `f32` clearance field and the component arrays), so a dense
//! 1.16 M-cell grid of 1700 layers is about 40 GB for ONE pose, and the
//! finalist poses run at once. Every cache therefore draws its bytes from one
//! process-wide [`BytePool`] ([`BytePool::process`], [`LAYER_CACHE_MAX_BYTES`],
//! or `PLINTH_LAYER_CACHE_BYTES`), so the caches that are alive together can
//! never hold more than the pool's limit between them.
//!
//! The pool is asked PER LAYER, before any work on the layer: a layer that
//! fits is built and kept; a layer that does not is not built at all and is
//! computed ON DEMAND by the reader instead (a *streamed* layer), exactly as
//! the sweeps computed it before the cache existed. A budget of zero is the
//! old behaviour; a budget for the whole stack is the cached behaviour; in
//! between, the first layers to ask are kept. Which layers are kept never
//! changes a result: kept and streamed layers are the same function of the
//! same inputs ([`CachedLayer::fill`]), so every output is byte-identical at
//! every budget.
//!
//! A reader holds the (at most two) streamed layers a consumer is looking at,
//! so a streaming consumer walking the stack in either direction rasterises
//! each layer once per walk, the same work as before the cache.
//!
//! # Exact by construction
//!
//! A cached value is the value of the function the consumer used to call, on
//! the same inputs, evaluated once: `rasterize_into`, `clearance_mm` and the
//! component labelling below are the unchanged routines. Nothing here reorders
//! a float sum. [`Components`] keeps the labelling's numbering, so the
//! model-section check still reports components in ascending root order and
//! sums each component's load in ascending cell order (it walks the carry mask
//! in cell order, and `roots` holds the component of each model cell in that
//! same order).
//!
//! # The key
//!
//! A cache is only valid for the slices and grid it was built from. It holds
//! them by reference (so they cannot change under it) and also a
//! [`LayerKey`]: the grid and a digest of every coordinate of every slice. A
//! consumer that is handed slices separately from the cache asks
//! [`LayerCache::resolve`], which returns the cache when the key matches and
//! builds a fresh one when it does not (a hollowed or drained mesh, a
//! different layer plan or a different grid all miss).
//!
//! # Shadow check
//!
//! With `PLINTH_SWEEP_VERIFY_CACHE=1` every cached layer a consumer reads is
//! recomputed fresh, once per cache, and compared by bits to the cached value;
//! any difference panics. It is for the BCO and Cylinder gate runs, not for
//! production.

use std::ops::Deref;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::OnceLock;

use plinth_support::perception::SliceSolid;
use rayon::prelude::*;

use crate::grid::Grid;
use crate::raster::{rasterize_into, LayerMasks, RasterScratch};

/// **The sweep layer caches' memory budget**, in bytes, across every cache of
/// the process: 24 GiB (perf lane D1 review, 2026-10-15).
///
/// A memory ceiling, not a physics value. The machine ceiling is about 100 GB
/// and one process holds the model surface index (up to
/// [`crate::MODEL_SURFACE_MAX_BYTES`] per pose), the working rasters of every
/// pose (a few hundred MiB each) and the slices, with 4 threads. 24 GiB of
/// layer cache leaves that headroom several times over, covers the measured
/// Cylinder (about 11 GiB) and Bat Cat Owl caches whole, and a pose that does
/// not fit streams its layers instead of failing. Override with the
/// `PLINTH_LAYER_CACHE_BYTES` environment variable (a byte count; `0` streams
/// every layer).
pub const LAYER_CACHE_MAX_BYTES: u64 = 24 << 30;

/// A shared byte allowance: `try_acquire` takes bytes if they fit under the
/// limit, `release` gives them back. Lock-free; the sum held is never above
/// the limit except through [`BytePool::force_acquire`] (a measured overshoot
/// of an estimate, never a fresh grant).
#[derive(Debug)]
pub struct BytePool {
    limit: AtomicU64,
    used: AtomicU64,
}

impl BytePool {
    #[must_use]
    pub const fn new(limit: u64) -> Self {
        Self {
            limit: AtomicU64::new(limit),
            used: AtomicU64::new(0),
        }
    }

    /// The process-wide pool every [`LayerCache::build`] draws from.
    #[must_use]
    pub fn process() -> &'static Self {
        static POOL: OnceLock<BytePool> = OnceLock::new();
        POOL.get_or_init(|| {
            let limit = std::env::var("PLINTH_LAYER_CACHE_BYTES")
                .ok()
                .and_then(|text| text.trim().parse::<u64>().ok())
                .unwrap_or(LAYER_CACHE_MAX_BYTES);
            Self::new(limit)
        })
    }

    #[must_use]
    pub fn limit(&self) -> u64 {
        self.limit.load(Ordering::Relaxed)
    }

    /// Bytes held now.
    #[must_use]
    pub fn used(&self) -> u64 {
        self.used.load(Ordering::Relaxed)
    }

    /// Change the limit; returns the old one. Bytes already held stay held.
    /// For tests and the CLI's budget flag; results never depend on it.
    pub fn set_limit(&self, limit: u64) -> u64 {
        self.limit.swap(limit, Ordering::Relaxed)
    }

    /// Take `bytes` if they fit under the limit.
    #[must_use]
    pub fn try_acquire(&self, bytes: u64) -> bool {
        let limit = self.limit();
        self.used
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |used| {
                used.checked_add(bytes).filter(|total| *total <= limit)
            })
            .is_ok()
    }

    /// Take `bytes` whatever the limit (an estimate that turned out low).
    fn force_acquire(&self, bytes: u64) {
        self.used.fetch_add(bytes, Ordering::AcqRel);
    }

    /// Give `bytes` back.
    pub fn release(&self, bytes: u64) {
        let previous = self.used.fetch_sub(bytes, Ordering::AcqRel);
        debug_assert!(previous >= bytes, "released more bytes than were held");
    }
}

/// The 4-connected components of one layer's model material, in the numbering
/// the model-section check has always used.
///
/// `roots[i]` is the component (union-find root) of the i-th model cell in
/// ascending cell order, that is of the i-th `true` of the `carry` mask the
/// components were labelled from. `counts[root]` is the number of cells whose
/// root it is; a union-find node that is not a root has count 0 and is
/// skipped, exactly as the check skipped it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct Components {
    pub roots: Vec<u32>,
    pub counts: Vec<u32>,
}

impl Components {
    /// Label `model` (row-major, `grid.cols()` wide). The union-find, its
    /// numbering and its root rule are the model-section check's own.
    #[cfg(test)]
    pub(crate) fn of(grid: &Grid, model: &[bool], labels: &mut Vec<u32>, parent: &mut Vec<u32>) -> Self {
        let mut components = Self::default();
        components.fill(grid, model, labels, parent);
        components
    }

    /// [`Self::of`] into `self`, reusing its buffers.
    pub(crate) fn fill(&mut self, grid: &Grid, model: &[bool], labels: &mut Vec<u32>, parent: &mut Vec<u32>) {
        let cols = grid.cols();
        let rows = grid.rows();
        parent.clear();
        labels.clear();
        labels.resize(grid.cells(), u32::MAX);
        for row in 0..rows {
            for col in 0..cols {
                let cell = row * cols + col;
                if !model[cell] {
                    continue;
                }
                let left = (col > 0 && model[cell - 1]).then(|| labels[cell - 1]);
                let up = (row > 0 && model[cell - cols]).then(|| labels[cell - cols]);
                match (left, up) {
                    (Some(a), Some(b)) => {
                        labels[cell] = find(parent, a);
                        union(parent, a, b);
                    }
                    (Some(a), None) | (None, Some(a)) => labels[cell] = find(parent, a),
                    (None, None) => {
                        let fresh = u32::try_from(parent.len()).unwrap_or(u32::MAX);
                        parent.push(fresh);
                        labels[cell] = fresh;
                    }
                }
            }
        }
        self.roots.clear();
        self.counts.clear();
        if parent.is_empty() {
            return;
        }
        self.counts.resize(parent.len(), 0);
        self.roots.reserve_exact(model.iter().filter(|filled| **filled).count());
        for cell in 0..grid.cells() {
            if !model[cell] {
                continue;
            }
            let root = find(parent, labels[cell]);
            self.counts[root as usize] += 1;
            self.roots.push(root);
        }
    }

    /// Resident bytes of the two arrays, at their real capacities.
    fn bytes(&self) -> usize {
        self.roots.capacity() * 4 + self.counts.capacity() * 4
    }
}

fn find(parent: &mut [u32], mut node: u32) -> u32 {
    while parent[node as usize] != node {
        let grand = parent[parent[node as usize] as usize];
        parent[node as usize] = grand;
        node = grand;
    }
    node
}

fn union(parent: &mut [u32], a: u32, b: u32) {
    let (ra, rb) = (find(parent, a), find(parent, b));
    if ra != rb {
        let (lo, hi) = if ra < rb { (ra, rb) } else { (rb, ra) };
        parent[hi as usize] = lo;
    }
}

/// Everything the cache (or a reader's streaming slot) holds for one layer.
#[derive(Debug, Default)]
struct CachedLayer {
    masks: LayerMasks,
    cover_any: bool,
    carrier_cells: usize,
    /// `reach::clearance_mm` of `masks.cover`; never set while `cover_any` is
    /// false (the field of an empty cover is all zeros and is read from
    /// [`LayerCache::zeros_f32`] instead). A cached layer's is set at build; a
    /// streamed layer's on first read, as the down-sweep always built it.
    clearance: OnceLock<Vec<f32>>,
    components: Components,
}

/// Per-thread scratch for building one layer.
struct BuildScratch {
    raster: RasterScratch,
    labels: Vec<u32>,
    parent: Vec<u32>,
}

impl BuildScratch {
    fn new(grid: &Grid) -> Self {
        Self {
            raster: RasterScratch::new(grid),
            labels: Vec::new(),
            parent: Vec::new(),
        }
    }
}

impl CachedLayer {
    /// The masks, counts and components of one layer (no clearance yet).
    /// Reuses every buffer `self` already owns.
    fn fill(&mut self, grid: &Grid, solids: &[SliceSolid], scratch: &mut BuildScratch) {
        rasterize_into(grid, solids, &mut scratch.raster, &mut self.masks);
        self.cover_any = self.masks.cover.iter().any(|filled| *filled);
        self.carrier_cells = self.masks.carrier_cells();
        self.clearance.take();
        if self.carrier_cells > 0 {
            self.components
                .fill(grid, &self.masks.carry, &mut scratch.labels, &mut scratch.parent);
        } else {
            self.components.roots.clear();
            self.components.counts.clear();
        }
    }

    /// A freshly built layer, buffers sized exactly.
    fn raster(grid: &Grid, solids: &[SliceSolid], scratch: &mut BuildScratch) -> Self {
        let mut layer = Self::default();
        layer.fill(grid, solids, scratch);
        layer
    }

    fn clearance_field(&self, grid: &Grid) -> Vec<f32> {
        let _span = crate::perf::span(crate::perf::Span::PassClearance);
        let mut field = Vec::new();
        crate::reach::clearance_mm(grid, &self.masks.cover, &mut field);
        field
    }

    fn add_clearance(&self, grid: &Grid) {
        if self.cover_any {
            let _ = self.clearance.set(self.clearance_field(grid));
        }
    }

    /// Resident bytes at the real capacities (clearance included once built).
    fn bytes(&self) -> usize {
        self.masks.cover.capacity()
            + self.masks.carry.capacity()
            + self.components.bytes()
            + self.clearance.get().map_or(0, |field| field.capacity() * 4)
    }
}

/// The most bytes one layer of `cells` cells can hold, clearance included.
///
/// Two byte masks, a `f32` clearance field, a `u32` component per carrier cell
/// (at most every cell) and a `u32` count per union-find node. A node is
/// created only by a model cell with no model cell to its left or above, and
/// two such cells are never 4-adjacent, so there are at most `ceil(cells / 2)`
/// nodes.
fn layer_bound(cells: usize) -> u64 {
    let cells = cells as u64;
    cells * (2 + 4 + 4) + (cells / 2 + 1) * 4
}

/// What a cache is valid for: the grid and every coordinate of every slice.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LayerKey {
    grid: Grid,
    layers: usize,
    digest: u64,
}

fn mix(hash: u64, word: u64) -> u64 {
    (hash.rotate_left(5) ^ word).wrapping_mul(0x517c_c1b7_2722_0a95)
}

fn loop_digest(hash: u64, points: &[[f64; 2]]) -> u64 {
    let mut hash = mix(hash, points.len() as u64);
    for point in points {
        hash = mix(hash, point[0].to_bits());
        hash = mix(hash, point[1].to_bits());
    }
    hash
}

fn solids_digest(solids: &[SliceSolid]) -> u64 {
    let mut hash = mix(0, solids.len() as u64);
    for solid in solids {
        hash = loop_digest(hash, solid.outer().points_xy_mm());
        hash = mix(hash, solid.holes().len() as u64);
        for hole in solid.holes() {
            hash = loop_digest(hash, hole.points_xy_mm());
        }
    }
    hash
}

fn fold_digests(digests: &[u64]) -> u64 {
    digests
        .iter()
        .fold(mix(0, digests.len() as u64), |hash, layer| mix(hash, *layer))
}

/// One plan's immutable layer data. See the module comment.
#[derive(Debug)]
pub struct LayerCache<'a> {
    grid: &'a Grid,
    slices: &'a [Vec<SliceSolid>],
    key: LayerKey,
    /// `Some` for a layer that fitted the byte budget; `None` for a layer the
    /// readers stream.
    layers: Vec<Option<CachedLayer>>,
    /// The digest of each layer's solids (the key's parts).
    digests: Vec<u64>,
    /// A layer's "cover above" at the top of the stack: nothing.
    zeros_bool: Vec<bool>,
    /// The clearance field of an empty cover.
    zeros_f32: Vec<f32>,
    verified: Vec<AtomicBool>,
    verify: bool,
    pool: &'a BytePool,
    /// Bytes this cache holds in `pool`; released on drop.
    held_bytes: u64,
    /// Layers a reader has computed on demand.
    streamed: AtomicU64,
}

/// Whether `PLINTH_SWEEP_VERIFY_CACHE` asks for the shadow check (read once).
fn verify_requested() -> bool {
    static ON: OnceLock<bool> = OnceLock::new();
    *ON.get_or_init(|| {
        std::env::var_os("PLINTH_SWEEP_VERIFY_CACHE").is_some_and(|value| value == "1")
    })
}

impl<'a> LayerCache<'a> {
    /// Build the cache under the process byte budget: every layer that fits
    /// the budget rasterised and its components labelled in parallel, then
    /// every kept non-empty cover's clearance field in parallel.
    #[must_use]
    pub fn build(grid: &'a Grid, slices: &'a [Vec<SliceSolid>]) -> Self {
        Self::build_in(grid, slices, BytePool::process())
    }

    /// [`Self::build`] drawing from `pool`.
    #[must_use]
    pub fn build_in(grid: &'a Grid, slices: &'a [Vec<SliceSolid>], pool: &'a BytePool) -> Self {
        Self::build_inner(grid, slices, pool, verify_requested())
    }

    fn build_inner(grid: &'a Grid, slices: &'a [Vec<SliceSolid>], pool: &'a BytePool, verify: bool) -> Self {
        assert!(
            u32::try_from(grid.cells()).is_ok(),
            "a layer of {} cells does not fit the component labels",
            grid.cells()
        );
        let bound = layer_bound(grid.cells());
        let reserved = AtomicU64::new(0);
        let built: Vec<(u64, Option<CachedLayer>)> = {
            let _span = crate::perf::span(crate::perf::Span::PassPrefix);
            slices
                .par_iter()
                .map_init(
                    || BuildScratch::new(grid),
                    |scratch, solids| {
                        let digest = solids_digest(solids);
                        // Ask for the layer's worst case BEFORE any work on
                        // it; a layer that does not fit is not built.
                        if !pool.try_acquire(bound) {
                            return (digest, None);
                        }
                        let layer = CachedLayer::raster(grid, solids, scratch);
                        // Keep what the layer will hold (its clearance field
                        // is built next), give the rest of the bound back.
                        let clearance = if layer.cover_any { grid.cells() as u64 * 4 } else { 0 };
                        let need = (layer.bytes() as u64 + clearance).min(bound);
                        pool.release(bound - need);
                        reserved.fetch_add(need, Ordering::Relaxed);
                        (digest, Some(layer))
                    },
                )
                .collect()
        };
        let (digests, layers): (Vec<u64>, Vec<Option<CachedLayer>>) = built.into_iter().unzip();
        {
            let _span = crate::perf::span(crate::perf::Span::PassClearance);
            layers
                .par_iter()
                .flatten()
                .for_each(|layer| layer.add_clearance(grid));
        }
        // The estimate counted lengths; the pool must hold the real capacities.
        let actual: u64 = layers.iter().flatten().map(|layer| layer.bytes() as u64).sum();
        let reserved = reserved.load(Ordering::Relaxed);
        if actual < reserved {
            pool.release(reserved - actual);
        } else if actual > reserved {
            pool.force_acquire(actual - reserved);
        }
        let verified = (0..layers.len()).map(|_| AtomicBool::new(false)).collect();
        let key = LayerKey {
            grid: *grid,
            layers: layers.len(),
            digest: fold_digests(&digests),
        };
        let cache = Self {
            grid,
            slices,
            key,
            layers,
            digests,
            zeros_bool: vec![false; grid.cells()],
            zeros_f32: vec![0.0; grid.cells()],
            verified,
            verify,
            pool,
            held_bytes: actual,
            streamed: AtomicU64::new(0),
        };
        let kept = cache.cached_layers();
        if kept < cache.len() {
            eprintln!(
                "[layer-cache] byte budget: kept {kept} of {} layers ({} MiB held; pool {} of {} MiB); the rest are computed on demand, with the same output",
                cache.len(),
                actual >> 20,
                pool.used() >> 20,
                pool.limit() >> 20
            );
        }
        cache
    }

    #[must_use]
    pub const fn grid(&self) -> &'a Grid {
        self.grid
    }

    #[must_use]
    pub const fn slices(&self) -> &'a [Vec<SliceSolid>] {
        self.slices
    }

    #[must_use]
    pub const fn key(&self) -> &LayerKey {
        &self.key
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.layers.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.layers.is_empty()
    }

    /// How many layers the byte budget let the cache keep.
    #[must_use]
    pub fn cached_layers(&self) -> usize {
        self.layers.iter().filter(|layer| layer.is_some()).count()
    }

    /// How many layers readers have computed on demand so far.
    #[must_use]
    pub fn streamed_fills(&self) -> u64 {
        self.streamed.load(Ordering::Relaxed)
    }

    /// A reader over this cache: cached layers are borrowed, the others are
    /// computed on demand into the reader's own two slots.
    #[must_use]
    pub fn reader(&self) -> LayerReader<'_, 'a> {
        LayerReader {
            cache: self,
            held: Vec::new(),
            scratch: None,
        }
    }

    /// Whether this cache was built from exactly this grid and these slices.
    ///
    /// The same borrow is the cheap, sound yes (the cache holds its slices by
    /// reference, so they cannot have changed). Anything else compares the
    /// grid and the digest of every coordinate.
    #[must_use]
    pub fn matches(&self, grid: &Grid, slices: &[Vec<SliceSolid>]) -> bool {
        if std::ptr::eq(self.grid, grid)
            && std::ptr::eq(self.slices.as_ptr(), slices.as_ptr())
            && self.slices.len() == slices.len()
        {
            return true;
        }
        self.key == key_of(grid, slices)
    }

    /// The offered cache when its key matches, else a fresh one built here.
    #[must_use]
    pub fn resolve<'c>(
        cache: Option<&'c LayerCache<'a>>,
        grid: &'a Grid,
        slices: &'a [Vec<SliceSolid>],
    ) -> Resolved<'c, 'a> {
        match cache {
            Some(offered) if offered.matches(grid, slices) => Resolved::Offered(offered),
            _ => Resolved::Built(Box::new(Self::build(grid, slices))),
        }
    }

    /// The shadow check, when it is on: recompute `layer` fresh (once per
    /// cache) and compare to the cached value by bits.
    fn checked(&self, layer: usize) {
        if self.verify && !self.verified[layer].swap(true, Ordering::Relaxed) {
            self.assert_fresh(layer);
        }
    }

    /// Panics unless the cached layer equals a fresh raster, clearance and
    /// labelling of the same slice, bit for bit. A layer the budget did not
    /// keep has nothing cached to compare (its slice digest is still checked).
    pub fn assert_fresh(&self, layer: usize) {
        assert!(
            solids_digest(&self.slices[layer]) == self.digests[layer],
            "layer cache: slice digest of layer {layer} differs from the slices"
        );
        let Some(cached) = &self.layers[layer] else {
            return;
        };
        let mut scratch = BuildScratch::new(self.grid);
        let fresh = CachedLayer::raster(self.grid, &self.slices[layer], &mut scratch);
        fresh.add_clearance(self.grid);
        assert!(
            fresh.masks.cover == cached.masks.cover,
            "layer cache: cover of layer {layer} differs from a fresh raster"
        );
        assert!(
            fresh.masks.carry == cached.masks.carry,
            "layer cache: carry of layer {layer} differs from a fresh raster"
        );
        assert!(
            fresh.cover_any == cached.cover_any && fresh.carrier_cells == cached.carrier_cells,
            "layer cache: cover or carrier count of layer {layer} differs from a fresh raster"
        );
        let (fresh_clear, cached_clear) = (
            fresh.clearance.get().map_or(&[][..], Vec::as_slice),
            cached.clearance.get().map_or(&[][..], Vec::as_slice),
        );
        assert!(
            fresh_clear.len() == cached_clear.len()
                && fresh_clear
                    .iter()
                    .zip(cached_clear)
                    .all(|(a, b)| a.to_bits() == b.to_bits()),
            "layer cache: clearance of layer {layer} differs from a fresh field by bits"
        );
        assert!(
            fresh.components == cached.components,
            "layer cache: model components of layer {layer} differ from a fresh labelling"
        );
    }

    /// Whether the shadow check is on for this cache.
    #[must_use]
    pub const fn verifying(&self) -> bool {
        self.verify
    }

    /// Resident bytes of the cached layers (masks, clearance, components), at
    /// their real capacities.
    #[must_use]
    pub fn resident_bytes(&self) -> usize {
        self.layers.iter().flatten().map(CachedLayer::bytes).sum()
    }

    /// Bytes this cache holds in its pool.
    #[must_use]
    pub const fn held_bytes(&self) -> u64 {
        self.held_bytes
    }
}

impl Drop for LayerCache<'_> {
    /// Give the pool its bytes back; and the shadow check's witness: how many
    /// layers a consumer read and the check compared (any difference would
    /// already have panicked).
    fn drop(&mut self) {
        self.pool.release(self.held_bytes);
        if self.verify {
            let verified = self
                .verified
                .iter()
                .filter(|flag| flag.load(Ordering::Relaxed))
                .count();
            eprintln!(
                "[layer-cache] shadow check: {verified} of {} layers read and verified bit-equal to a fresh raster, clearance and labelling ({} MiB cached)",
                self.layers.len(),
                self.resident_bytes() >> 20
            );
        }
    }
}

/// Layers streamed by every reader in this process (a witness for tests that
/// cannot see the cache a plan builds inside).
static STREAMED_TOTAL: AtomicU64 = AtomicU64::new(0);

/// How many layers readers have computed on demand, in this process so far.
#[must_use]
pub fn streamed_fills_total() -> u64 {
    STREAMED_TOTAL.load(Ordering::Relaxed)
}

/// One streamed layer a reader holds.
struct Slot {
    layer: usize,
    data: CachedLayer,
}

/// A consumer's view of a [`LayerCache`].
///
/// [`Self::prepare`] names the layer the consumer is about to read (and,
/// optionally, the layer above it); the accessors then answer for those
/// layers. A cached layer is borrowed from the cache. A layer the byte budget
/// did not keep is computed into one of the reader's two slots, so a walk up
/// or down the stack computes each such layer once. The values are the same
/// either way.
pub struct LayerReader<'c, 'a> {
    cache: &'c LayerCache<'a>,
    held: Vec<Slot>,
    scratch: Option<BuildScratch>,
}

impl LayerReader<'_, '_> {
    /// Make `layer` readable, and `layer + 1` too when `with_above` (the
    /// accessors that answer for the layer above need it).
    pub fn prepare(&mut self, layer: usize, with_above: bool) {
        let above = (with_above && layer + 1 < self.cache.layers.len()).then_some(layer + 1);
        self.ensure(layer, layer, above);
        if let Some(above) = above {
            self.ensure(above, layer, Some(above));
        }
    }

    /// Make `want` readable without evicting `keep` or `keep_also`.
    fn ensure(&mut self, want: usize, keep: usize, keep_also: Option<usize>) {
        if self.cache.layers[want].is_some() || self.held.iter().any(|slot| slot.layer == want) {
            return;
        }
        let index = if self.held.len() < 2 {
            self.held.push(Slot {
                layer: usize::MAX,
                data: CachedLayer::default(),
            });
            self.held.len() - 1
        } else {
            self.held
                .iter()
                .position(|slot| slot.layer != keep && Some(slot.layer) != keep_also)
                .expect("two slots always hold at most one of the layers kept")
        };
        let cache = self.cache;
        let scratch = self.scratch.get_or_insert_with(|| BuildScratch::new(cache.grid));
        let slot = &mut self.held[index];
        slot.layer = usize::MAX;
        slot.data.fill(cache.grid, &cache.slices[want], scratch);
        slot.layer = want;
        cache.streamed.fetch_add(1, Ordering::Relaxed);
        STREAMED_TOTAL.fetch_add(1, Ordering::Relaxed);
    }

    /// The prepared layer's data: the cache's or this reader's slot.
    fn get(&self, layer: usize) -> &CachedLayer {
        if let Some(cached) = &self.cache.layers[layer] {
            self.cache.checked(layer);
            return cached;
        }
        self.held
            .iter()
            .find(|slot| slot.layer == layer)
            .map(|slot| &slot.data)
            .unwrap_or_else(|| panic!("layer {layer} was read without being prepared"))
    }

    /// The layer's masks, as `rasterize_into` would fill them.
    #[must_use]
    pub fn masks(&self, layer: usize) -> &LayerMasks {
        &self.get(layer).masks
    }

    /// Whether the layer's cover has any cell.
    #[must_use]
    pub fn cover_any(&self, layer: usize) -> bool {
        self.get(layer).cover_any
    }

    /// The number of carrier cells (`LayerMasks::carrier_cells`).
    #[must_use]
    pub fn carrier_cells(&self, layer: usize) -> usize {
        self.get(layer).carrier_cells
    }

    /// The cover of layer `layer + 1`, the material a load travels through on
    /// its way down into `layer`; all false above the top layer. Needs
    /// `prepare(layer, true)`.
    #[must_use]
    pub fn cover_above(&self, layer: usize) -> &[bool] {
        if layer + 1 < self.cache.layers.len() {
            self.get(layer + 1).masks.cover.as_slice()
        } else {
            self.cache.zeros_bool.as_slice()
        }
    }

    /// `clearance_mm` of [`Self::cover_above`]. Needs `prepare(layer, true)`.
    /// A streamed layer's field is built here, on the first read.
    #[must_use]
    pub fn clearance_above(&self, layer: usize) -> &[f32] {
        if layer + 1 >= self.cache.layers.len() {
            return self.cache.zeros_f32.as_slice();
        }
        let above = self.get(layer + 1);
        if above.cover_any {
            above
                .clearance
                .get_or_init(|| above.clearance_field(self.cache.grid))
                .as_slice()
        } else {
            self.cache.zeros_f32.as_slice()
        }
    }

    /// The model components of the layer's carry mask.
    pub(crate) fn components(&self, layer: usize) -> &Components {
        &self.get(layer).components
    }
}

fn key_of(grid: &Grid, slices: &[Vec<SliceSolid>]) -> LayerKey {
    let digests: Vec<u64> = slices.par_iter().map(|solids| solids_digest(solids)).collect();
    LayerKey {
        grid: *grid,
        layers: slices.len(),
        digest: fold_digests(&digests),
    }
}

/// A cache a consumer can read: the one it was offered, or one built for it.
pub enum Resolved<'c, 'a> {
    Offered(&'c LayerCache<'a>),
    Built(Box<LayerCache<'a>>),
}

impl<'a> Deref for Resolved<'_, 'a> {
    type Target = LayerCache<'a>;

    fn deref(&self) -> &LayerCache<'a> {
        match self {
            Resolved::Offered(cache) => cache,
            Resolved::Built(cache) => cache,
        }
    }
}

#[cfg(test)]
mod tests {
    use plinth_mesh::{make_box, Mesh};
    use plinth_support::perception::{slice_stack, SliceStackLimits};

    use super::*;
    use crate::grid::plan_grid;

    const LAYERS: usize = 24;
    const H: f64 = 0.05;

    /// A box of the given footprint: a few layers, one component.
    fn box_mesh(x0: f64, x1: f64, y0: f64, y1: f64, z0: f64, z1: f64) -> Mesh {
        make_box(
            [x1 - x0, y1 - y0, z1 - z0],
            [0.5 * (x0 + x1), 0.5 * (y0 + y1), 0.5 * (z0 + z1)],
        )
        .expect("box mesh")
    }

    fn stack(mesh: &Mesh) -> Vec<Vec<SliceSolid>> {
        let planes: Vec<f64> = (0..LAYERS).map(|l| (l as f64 + 0.5) * H).collect();
        slice_stack(mesh, &planes, SliceStackLimits::production()).expect("slices")
    }

    fn grid_for(pitch: f64) -> Grid {
        plan_grid(-0.5, -0.5, 3.5, 2.5, LAYERS, pitch, 0.5, u64::MAX).0
    }

    /// A pool that holds everything.
    fn ample() -> BytePool {
        BytePool::new(u64::MAX / 4)
    }

    #[test]
    fn cache_equals_a_fresh_raster_clearance_and_labelling_on_every_layer() {
        let mesh = box_mesh(0.0, 3.0, 0.0, 2.0, 0.0, LAYERS as f64 * H);
        let slices = stack(&mesh);
        let grid = grid_for(0.1);
        let pool = ample();
        let cache = LayerCache::build_in(&grid, &slices, &pool);
        assert_eq!(cache.len(), LAYERS);
        assert_eq!(cache.cached_layers(), LAYERS);
        for layer in 0..LAYERS {
            cache.assert_fresh(layer);
        }
        let mut reader = cache.reader();
        // The cover above is the next layer's cover, and nothing above the top.
        for layer in 0..LAYERS - 1 {
            reader.prepare(layer, true);
            assert_eq!(reader.cover_above(layer), reader.masks(layer + 1).cover.as_slice());
        }
        reader.prepare(LAYERS - 1, true);
        assert!(reader.cover_above(LAYERS - 1).iter().all(|filled| !*filled));
        assert!(reader.clearance_above(LAYERS - 1).iter().all(|value| *value == 0.0));
        // And the clearance a layer reads is the fresh clearance of exactly
        // that cover above it, by bits (the mapping, not just the layer).
        let mut checked = 0_usize;
        for layer in 0..LAYERS {
            reader.prepare(layer, true);
            let mut fresh = Vec::new();
            crate::reach::clearance_mm(&grid, reader.cover_above(layer), &mut fresh);
            let cached = reader.clearance_above(layer);
            assert_eq!(fresh.len(), cached.len());
            assert!(
                fresh.iter().zip(cached).all(|(a, b)| a.to_bits() == b.to_bits()),
                "clearance above layer {layer} differs from the clearance of its cover above"
            );
            checked += usize::from(reader.cover_above(layer).iter().any(|filled| *filled));
        }
        assert!(checked > 5, "only {checked} layers had a cover above to compare");
        // At least one layer is non-trivial, so the comparison above has teeth.
        reader.prepare(LAYERS / 2, false);
        assert!(reader.carrier_cells(LAYERS / 2) > 0);
        assert!(reader.components(LAYERS / 2).counts.iter().any(|count| *count > 0));
        assert_eq!(reader.components(LAYERS / 2).roots.len(), reader.carrier_cells(LAYERS / 2));
        assert_eq!(cache.streamed_fills(), 0, "a whole cache streams nothing");
    }

    #[test]
    fn a_different_grid_or_different_slices_miss_and_rebuild() {
        let mesh = box_mesh(0.0, 3.0, 0.0, 2.0, 0.0, LAYERS as f64 * H);
        let slices = stack(&mesh);
        let grid = grid_for(0.1);
        let cache = LayerCache::build(&grid, &slices);
        assert!(cache.matches(&grid, &slices));

        // Equal content in another allocation still matches (key, not address).
        let copy = slices.clone();
        let same_grid = grid;
        assert!(cache.matches(&same_grid, &copy));

        // A different grid misses.
        let coarser = grid_for(0.2);
        assert!(!cache.matches(&coarser, &slices));
        let resolved = LayerCache::resolve(Some(&cache), &coarser, &slices);
        assert!(matches!(resolved, Resolved::Built(_)));
        assert_eq!(resolved.grid(), &coarser);
        let mut reader = resolved.reader();
        reader.prepare(LAYERS / 2, false);
        assert_eq!(reader.masks(LAYERS / 2).cover.len(), coarser.cells());

        // A hollowed (different) mesh misses, and the rebuild is that mesh's.
        let hollow = box_mesh(0.0, 2.5, 0.0, 2.0, 0.0, LAYERS as f64 * H);
        let other = stack(&hollow);
        assert!(!cache.matches(&grid, &other));
        let resolved = LayerCache::resolve(Some(&cache), &grid, &other);
        assert!(matches!(resolved, Resolved::Built(_)));
        for layer in 0..LAYERS {
            resolved.assert_fresh(layer);
        }

        // A different layer plan (fewer layers) misses.
        assert!(!cache.matches(&grid, &slices[..LAYERS - 1]));

        // The same inputs are served the offered cache, not a rebuild.
        let resolved = LayerCache::resolve(Some(&cache), &grid, &slices);
        assert!(matches!(resolved, Resolved::Offered(_)));
    }

    /// SABOTAGE WITNESS. A stale cache (built for other slices) read as if it
    /// were current must be caught: its layers differ from a fresh raster of
    /// the slices actually in hand. If `matches` is changed to answer `true`
    /// unconditionally, `a_different_grid_or_different_slices_miss_and_rebuild`
    /// turns red, and this test shows the stale data would have been wrong.
    #[test]
    fn a_stale_cache_differs_from_a_fresh_raster_of_the_new_slices() {
        let grid = grid_for(0.1);
        let wide = stack(&box_mesh(0.0, 3.0, 0.0, 2.0, 0.0, LAYERS as f64 * H));
        let narrow = stack(&box_mesh(0.0, 2.5, 0.0, 2.0, 0.0, LAYERS as f64 * H));
        let stale = LayerCache::build(&grid, &wide);
        let fresh = LayerCache::build(&grid, &narrow);
        let layer = LAYERS / 2;
        let (mut stale_reader, mut fresh_reader) = (stale.reader(), fresh.reader());
        stale_reader.prepare(layer, false);
        fresh_reader.prepare(layer, false);
        assert_ne!(stale_reader.masks(layer).cover, fresh_reader.masks(layer).cover);
        assert_ne!(stale.key(), fresh.key());
        assert!(!stale.matches(&grid, &narrow));
    }

    /// Every reader answer of every layer, as comparable bits.
    #[derive(Debug, PartialEq)]
    struct LayerFacts {
        cover: Vec<bool>,
        carry: Vec<bool>,
        cover_any: bool,
        carrier_cells: usize,
        cover_above: Vec<bool>,
        clearance_above_bits: Vec<u32>,
        components: Components,
    }

    fn facts(reader: &mut LayerReader<'_, '_>, layer: usize) -> LayerFacts {
        reader.prepare(layer, true);
        LayerFacts {
            cover: reader.masks(layer).cover.clone(),
            carry: reader.masks(layer).carry.clone(),
            cover_any: reader.cover_any(layer),
            carrier_cells: reader.carrier_cells(layer),
            cover_above: reader.cover_above(layer).to_vec(),
            clearance_above_bits: reader.clearance_above(layer).iter().map(|v| v.to_bits()).collect(),
            components: reader.components(layer).clone(),
        }
    }

    fn all_facts(cache: &LayerCache<'_>, order: &[usize]) -> Vec<(usize, LayerFacts)> {
        let mut reader = cache.reader();
        order.iter().map(|&layer| (layer, facts(&mut reader, layer))).collect()
    }

    fn sorted(mut facts: Vec<(usize, LayerFacts)>) -> Vec<LayerFacts> {
        facts.sort_by_key(|(layer, _)| *layer);
        facts.into_iter().map(|(_, facts)| facts).collect()
    }

    /// Layers of different shapes: one wide box; every third layer two
    /// separate columns (two components); every third layer after that one.
    fn awkward_stack() -> Vec<Vec<SliceSolid>> {
        let mut slices = stack(&box_mesh(0.0, 3.0, 0.0, 2.0, 0.0, LAYERS as f64 * H));
        let left = stack(&box_mesh(0.0, 1.0, 0.5, 1.5, 0.0, LAYERS as f64 * H));
        let right = stack(&box_mesh(2.0, 3.0, 0.5, 1.5, 0.0, LAYERS as f64 * H));
        for layer in 0..LAYERS {
            match layer % 3 {
                0 => {
                    let mut both = left[layer].clone();
                    both.extend(right[layer].iter().cloned());
                    slices[layer] = both;
                }
                1 => slices[layer] = left[layer].clone(),
                _ => {}
            }
        }
        slices
    }

    #[test]
    fn a_zero_budget_streams_every_layer_and_every_answer_is_the_cached_answer() {
        let slices = awkward_stack();
        let grid = grid_for(0.1);
        let ample_pool = ample();
        let full = LayerCache::build_in(&grid, &slices, &ample_pool);
        assert_eq!(full.cached_layers(), LAYERS);
        let none_pool = BytePool::new(0);
        let none = LayerCache::build_in(&grid, &slices, &none_pool);
        assert_eq!(none.cached_layers(), 0);
        assert_eq!(none.held_bytes(), 0);
        assert_eq!(none_pool.used(), 0);
        assert_eq!(none.resident_bytes(), 0);
        assert_eq!(none.key(), full.key(), "the key does not depend on the budget");

        let ascending: Vec<usize> = (0..LAYERS).collect();
        let descending: Vec<usize> = (0..LAYERS).rev().collect();
        let scrambled: Vec<usize> = (0..LAYERS).map(|i| (i * 7 + 3) % LAYERS).collect();
        let want = sorted(all_facts(&full, &ascending));
        for order in [&ascending, &descending, &scrambled] {
            let got = sorted(all_facts(&none, order));
            assert_eq!(want.len(), got.len());
            for (layer, (a, b)) in want.iter().zip(&got).enumerate() {
                assert_eq!(a, b, "layer {layer} streamed differently from the cached layer");
            }
        }
        // The comparison has teeth: layers carry several components and covers.
        assert!(want.iter().any(|f| f.components.counts.iter().filter(|c| **c > 0).count() > 1));
        assert!(want.iter().any(|f| f.clearance_above_bits.iter().any(|b| *b != 0)));
        assert!(none.streamed_fills() >= LAYERS as u64);
    }

    #[test]
    fn a_walk_in_either_direction_streams_each_layer_once() {
        let slices = awkward_stack();
        let grid = grid_for(0.1);
        let pool = BytePool::new(0);
        for descending in [true, false] {
            let cache = LayerCache::build_in(&grid, &slices, &pool);
            let mut reader = cache.reader();
            let order: Vec<usize> = if descending {
                (0..LAYERS).rev().collect()
            } else {
                (0..LAYERS).collect()
            };
            for layer in order {
                reader.prepare(layer, descending);
                let _ = reader.masks(layer);
            }
            assert_eq!(cache.streamed_fills(), LAYERS as u64, "descending {descending}");
        }
    }

    #[test]
    fn a_partial_budget_keeps_some_layers_and_answers_identically() {
        let slices = awkward_stack();
        let grid = grid_for(0.1);
        let ample_pool = ample();
        let full = LayerCache::build_in(&grid, &slices, &ample_pool);
        let each = full.held_bytes() / LAYERS as u64;
        // Room for the bound of roughly a third of the layers, one at a time.
        let limit = layer_bound(grid.cells()) + each * (LAYERS as u64 / 3);
        let pool = BytePool::new(limit);
        let part = LayerCache::build_in(&grid, &slices, &pool);
        let kept = part.cached_layers();
        assert!(kept > 0 && kept < LAYERS, "kept {kept} of {LAYERS}");
        assert!(pool.used() <= limit, "{} over {limit}", pool.used());
        assert_eq!(pool.used(), part.held_bytes());
        let order: Vec<usize> = (0..LAYERS).rev().collect();
        assert_eq!(
            sorted(all_facts(&full, &order)),
            sorted(all_facts(&part, &order)),
            "a partly cached stack answered differently"
        );
    }

    #[test]
    fn the_pool_holds_exactly_the_cache_and_gets_it_back_on_drop() {
        let slices = awkward_stack();
        let grid = grid_for(0.1);
        let pool = ample();
        {
            let cache = LayerCache::build_in(&grid, &slices, &pool);
            assert_eq!(cache.cached_layers(), LAYERS);
            // Exact: the pool holds the real capacities and nothing else.
            assert_eq!(cache.held_bytes(), cache.resident_bytes() as u64);
            assert_eq!(pool.used(), cache.held_bytes());
            // And the up-front bound is a bound.
            assert!(cache.held_bytes() <= layer_bound(grid.cells()) * LAYERS as u64);
        }
        assert_eq!(pool.used(), 0, "a dropped cache returned its bytes");
    }

    #[test]
    fn caches_alive_together_never_hold_more_than_the_pool() {
        let slices = awkward_stack();
        let grid = grid_for(0.1);
        let probe = ample();
        let whole = LayerCache::build_in(&grid, &slices, &probe).held_bytes();
        // Room for a bit over one and a half caches; three poses at once.
        let limit = whole + whole / 2;
        let pool = BytePool::new(limit);
        let caches: Vec<LayerCache<'_>> = (0..3)
            .map(|_| {
                let cache = LayerCache::build_in(&grid, &slices, &pool);
                assert!(pool.used() <= limit);
                cache
            })
            .collect();
        assert!(pool.used() <= limit, "{} over {limit}", pool.used());
        let held: u64 = caches.iter().map(LayerCache::held_bytes).sum();
        assert_eq!(held, pool.used());
        assert!(caches.iter().any(|cache| cache.cached_layers() < LAYERS), "the third pose streamed");
        drop(caches);
        assert_eq!(pool.used(), 0);

        // The same under real concurrency: poses built on parallel threads.
        let racing = BytePool::new(limit);
        let built: Vec<LayerCache<'_>> = (0..4)
            .into_par_iter()
            .map(|_| LayerCache::build_in(&grid, &slices, &racing))
            .collect();
        assert!(racing.used() <= limit, "{} over {limit}", racing.used());
        drop(built);
        assert_eq!(racing.used(), 0);
    }

    #[test]
    fn the_pool_refuses_what_does_not_fit_and_counts_what_it_gave() {
        let pool = BytePool::new(100);
        assert!(pool.try_acquire(60));
        assert!(!pool.try_acquire(41));
        assert!(pool.try_acquire(40));
        assert_eq!(pool.used(), 100);
        pool.release(60);
        assert!(pool.try_acquire(59));
        assert!(!pool.try_acquire(2));
        assert!(!BytePool::new(0).try_acquire(1));
        assert!(BytePool::new(0).try_acquire(0));
    }
}
