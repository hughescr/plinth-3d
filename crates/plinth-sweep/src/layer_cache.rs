//! The immutable per-layer cache of one support plan (perf lane D1,
//! 2026-10-15).
//!
//! Four things the sweeps read are functions of the SLICES and the GRID alone,
//! never of the evolving strut set:
//!
//! * the layer's two masks (`cover`, `carry`, [`crate::raster`]);
//! * the clearance field of a layer's `cover` (`reach::clearance_mm`), which
//!   the down-sweep's routing field reads for the layer BELOW it;
//! * the 4-connected components of the layer's MODEL material (`carry`), their
//!   cell counts and the cell-to-component map the model-section check sums a
//!   load raster over;
//! * whether the layer has any cover at all, and its carrier-cell count.
//!
//! Before this module every down-sweep pass (14 to 35 of them per plan, each
//! over 380 layers on Deinonychus) rebuilt all four, as did the up-sweep and
//! the removal replay. Now one [`LayerCache`] is built per plan, in parallel
//! across layers, and borrowed by every consumer.
//!
//! # Exact by construction
//!
//! A cached value is the value of the function the consumer used to call, on
//! the same inputs, evaluated once: `rasterize_into`, `clearance_mm` and the
//! component labelling below are the unchanged routines. Nothing here reorders
//! a float sum. [`Components`] keeps the labelling's numbering, so the
//! model-section check still reports components in ascending root order and
//! sums each component's load in ascending cell order.
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
//! With `PLINTH_SWEEP_VERIFY_CACHE=1` every layer a consumer reads is
//! recomputed fresh, once per cache, and compared by bits to the cached value;
//! any difference panics. It is for the BCO and Cylinder gate runs, not for
//! production.

use std::ops::Deref;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::OnceLock;

use plinth_support::perception::SliceSolid;
use rayon::prelude::*;

use crate::grid::Grid;
use crate::raster::{rasterize_into, LayerMasks, RasterScratch};

/// The 4-connected components of one layer's model material, in the numbering
/// the model-section check has always used.
///
/// `cells` are the model cells in ascending order and `roots[i]` is the
/// component (union-find root) of `cells[i]`. `counts[root]` is the number of
/// cells whose root it is; a union-find node that is not a root has count 0
/// and is skipped, exactly as the check skipped it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct Components {
    pub cells: Vec<u32>,
    pub roots: Vec<u32>,
    pub counts: Vec<u64>,
}

impl Components {
    /// Label `model` (row-major, `grid.cols()` wide). The union-find, its
    /// numbering and its root rule are the model-section check's own.
    pub(crate) fn of(grid: &Grid, model: &[bool], labels: &mut Vec<u32>, parent: &mut Vec<u32>) -> Self {
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
        if parent.is_empty() {
            return Self::default();
        }
        let mut counts = vec![0_u64; parent.len()];
        let mut cells = Vec::new();
        let mut roots = Vec::new();
        for cell in 0..grid.cells() {
            if !model[cell] {
                continue;
            }
            let root = find(parent, labels[cell]);
            counts[root as usize] += 1;
            cells.push(u32::try_from(cell).unwrap_or(u32::MAX));
            roots.push(root);
        }
        Self { cells, roots, counts }
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

/// Everything the cache keeps for one layer.
#[derive(Debug)]
struct CachedLayer {
    masks: LayerMasks,
    cover_any: bool,
    carrier_cells: usize,
    /// `reach::clearance_mm` of `masks.cover`; empty while `cover_any` is
    /// false (the field of an empty cover is all zeros and is read from
    /// [`LayerCache::zeros_f32`] instead).
    clearance: Vec<f32>,
    components: Components,
    /// Digest of the layer's solids, folded into the [`LayerKey`].
    digest: u64,
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
    fn raster(grid: &Grid, solids: &[SliceSolid], scratch: &mut BuildScratch) -> Self {
        let mut masks = LayerMasks::default();
        rasterize_into(grid, solids, &mut scratch.raster, &mut masks);
        let cover_any = masks.cover.iter().any(|filled| *filled);
        let carrier_cells = masks.carrier_cells();
        let components = if carrier_cells > 0 {
            Components::of(grid, &masks.carry, &mut scratch.labels, &mut scratch.parent)
        } else {
            Components::default()
        };
        Self {
            masks,
            cover_any,
            carrier_cells,
            clearance: Vec::new(),
            components,
            digest: solids_digest(solids),
        }
    }

    fn add_clearance(&mut self, grid: &Grid) {
        if self.cover_any {
            crate::reach::clearance_mm(grid, &self.masks.cover, &mut self.clearance);
        }
    }
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

/// One plan's immutable layer data. See the module comment.
#[derive(Debug)]
pub struct LayerCache<'a> {
    grid: &'a Grid,
    slices: &'a [Vec<SliceSolid>],
    key: LayerKey,
    layers: Vec<CachedLayer>,
    /// A layer's "cover above" at the top of the stack: nothing.
    zeros_bool: Vec<bool>,
    /// The clearance field of an empty cover.
    zeros_f32: Vec<f32>,
    verified: Vec<AtomicBool>,
    verify: bool,
}

/// Whether `PLINTH_SWEEP_VERIFY_CACHE` asks for the shadow check (read once).
fn verify_requested() -> bool {
    static ON: OnceLock<bool> = OnceLock::new();
    *ON.get_or_init(|| {
        std::env::var_os("PLINTH_SWEEP_VERIFY_CACHE").is_some_and(|value| value == "1")
    })
}

impl<'a> LayerCache<'a> {
    /// Build the cache: every layer rasterised and its components labelled in
    /// parallel, then every non-empty cover's clearance field in parallel.
    #[must_use]
    pub fn build(grid: &'a Grid, slices: &'a [Vec<SliceSolid>]) -> Self {
        Self::build_inner(grid, slices, verify_requested())
    }

    fn build_inner(grid: &'a Grid, slices: &'a [Vec<SliceSolid>], verify: bool) -> Self {
        assert!(
            u32::try_from(grid.cells()).is_ok(),
            "a layer of {} cells does not fit the component labels",
            grid.cells()
        );
        let mut layers: Vec<CachedLayer> = {
            let _span = crate::perf::span(crate::perf::Span::PassPrefix);
            slices
                .par_iter()
                .map_init(
                    || BuildScratch::new(grid),
                    |scratch, solids| CachedLayer::raster(grid, solids, scratch),
                )
                .collect()
        };
        {
            let _span = crate::perf::span(crate::perf::Span::PassClearance);
            layers.par_iter_mut().for_each(|layer| layer.add_clearance(grid));
        }
        let digest = layers.iter().fold(mix(0, layers.len() as u64), |hash, layer| mix(hash, layer.digest));
        let verified = (0..layers.len()).map(|_| AtomicBool::new(false)).collect();
        Self {
            grid,
            slices,
            key: LayerKey {
                grid: *grid,
                layers: layers.len(),
                digest,
            },
            layers,
            zeros_bool: vec![false; grid.cells()],
            zeros_f32: vec![0.0; grid.cells()],
            verified,
            verify,
        }
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

    /// The layer's masks, as `rasterize_into` would fill them.
    #[must_use]
    pub fn masks(&self, layer: usize) -> &LayerMasks {
        self.checked(layer);
        &self.layers[layer].masks
    }

    /// Whether the layer's cover has any cell.
    #[must_use]
    pub fn cover_any(&self, layer: usize) -> bool {
        self.layers[layer].cover_any
    }

    /// The number of carrier cells (`LayerMasks::carrier_cells`).
    #[must_use]
    pub fn carrier_cells(&self, layer: usize) -> usize {
        self.layers[layer].carrier_cells
    }

    /// The cover of layer `layer + 1`, the material a load travels through on
    /// its way down into `layer`; all false above the top layer.
    #[must_use]
    pub fn cover_above(&self, layer: usize) -> &[bool] {
        self.layers
            .get(layer + 1)
            .map_or(self.zeros_bool.as_slice(), |above| {
                self.checked(layer + 1);
                above.masks.cover.as_slice()
            })
    }

    /// `clearance_mm` of [`Self::cover_above`].
    #[must_use]
    pub fn clearance_above(&self, layer: usize) -> &[f32] {
        self.layers
            .get(layer + 1)
            .map_or(self.zeros_f32.as_slice(), |above| {
                if above.cover_any {
                    above.clearance.as_slice()
                } else {
                    self.zeros_f32.as_slice()
                }
            })
    }

    /// The model components of the layer's carry mask.
    pub(crate) fn components(&self, layer: usize) -> &Components {
        &self.layers[layer].components
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
    /// labelling of the same slice, bit for bit.
    pub fn assert_fresh(&self, layer: usize) {
        let mut scratch = BuildScratch::new(self.grid);
        let mut fresh = CachedLayer::raster(self.grid, &self.slices[layer], &mut scratch);
        fresh.add_clearance(self.grid);
        let cached = &self.layers[layer];
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
        assert!(
            fresh.clearance.len() == cached.clearance.len()
                && fresh
                    .clearance
                    .iter()
                    .zip(&cached.clearance)
                    .all(|(a, b)| a.to_bits() == b.to_bits()),
            "layer cache: clearance of layer {layer} differs from a fresh field by bits"
        );
        assert!(
            fresh.components == cached.components,
            "layer cache: model components of layer {layer} differ from a fresh labelling"
        );
        assert!(
            fresh.digest == cached.digest,
            "layer cache: slice digest of layer {layer} differs from the slices"
        );
    }

    /// Whether the shadow check is on for this cache.
    #[must_use]
    pub const fn verifying(&self) -> bool {
        self.verify
    }

    /// Resident bytes of the cached layers (masks, clearance, components).
    #[must_use]
    pub fn resident_bytes(&self) -> usize {
        self.layers
            .iter()
            .map(|layer| {
                layer.masks.cover.len()
                    + layer.masks.carry.len()
                    + layer.clearance.len() * 4
                    + layer.components.cells.len() * 4
                    + layer.components.roots.len() * 4
                    + layer.components.counts.len() * 8
            })
            .sum()
    }
}

impl Drop for LayerCache<'_> {
    /// The shadow check's witness: how many layers a consumer read and the
    /// check compared (any difference would already have panicked).
    fn drop(&mut self) {
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

fn key_of(grid: &Grid, slices: &[Vec<SliceSolid>]) -> LayerKey {
    let digests: Vec<u64> = slices.par_iter().map(|solids| solids_digest(solids)).collect();
    let digest = digests.iter().fold(mix(0, digests.len() as u64), |hash, layer| mix(hash, *layer));
    LayerKey {
        grid: *grid,
        layers: slices.len(),
        digest,
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

    #[test]
    fn cache_equals_a_fresh_raster_clearance_and_labelling_on_every_layer() {
        let mesh = box_mesh(0.0, 3.0, 0.0, 2.0, 0.0, LAYERS as f64 * H);
        let slices = stack(&mesh);
        let grid = grid_for(0.1);
        let cache = LayerCache::build(&grid, &slices);
        assert_eq!(cache.len(), LAYERS);
        for layer in 0..LAYERS {
            cache.assert_fresh(layer);
        }
        // The cover above is the next layer's cover, and nothing above the top.
        for layer in 0..LAYERS - 1 {
            assert_eq!(cache.cover_above(layer), cache.masks(layer + 1).cover.as_slice());
        }
        assert!(cache.cover_above(LAYERS - 1).iter().all(|filled| !*filled));
        assert!(cache.clearance_above(LAYERS - 1).iter().all(|value| *value == 0.0));
        // And the clearance a layer reads is the fresh clearance of exactly
        // that cover above it, by bits (the mapping, not just the layer).
        let mut checked = 0_usize;
        for layer in 0..LAYERS {
            let mut fresh = Vec::new();
            crate::reach::clearance_mm(&grid, cache.cover_above(layer), &mut fresh);
            let cached = cache.clearance_above(layer);
            assert_eq!(fresh.len(), cached.len());
            assert!(
                fresh.iter().zip(cached).all(|(a, b)| a.to_bits() == b.to_bits()),
                "clearance above layer {layer} differs from the clearance of its cover above"
            );
            checked += usize::from(cache.cover_above(layer).iter().any(|filled| *filled));
        }
        assert!(checked > 5, "only {checked} layers had a cover above to compare");
        // At least one layer is non-trivial, so the comparison above has teeth.
        assert!(cache.carrier_cells(LAYERS / 2) > 0);
        assert!(cache.components(LAYERS / 2).counts.iter().any(|count| *count > 0));
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
        assert_eq!(resolved.masks(LAYERS / 2).cover.len(), coarser.cells());

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
        assert_ne!(stale.masks(layer).cover, fresh.masks(layer).cover);
        assert_ne!(stale.key(), fresh.key());
        assert!(!stale.matches(&grid, &narrow));
    }
}
