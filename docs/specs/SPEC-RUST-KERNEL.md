# SPEC-RUST-KERNEL — geom_kernel: the Rust ExactSurface hot core (de-Python stage 1)

Status: APPROVED FOR IMPLEMENTATION (branch `v5-rust-kernel`, from v5-geom-core 94705ad).
Authority: Craig's binding ruling of 2026-07-23 — "we have to ditch Python" —
after the full-scale treant run (576,026 faces, canonical
`SupportConfig(lift_mm=3.0)`) ran 2h22m single-core WITHOUT FINISHING against
194s for the old (incorrect) point-cloud engine, with CPU pinned at ~15% (one
core) for the whole project. A live process sample identified the hotspot:
numpy SIMD **int64 quicksort** dominating — the per-query-batch
sort/unique/union pattern inside `geometry_exact.py` (details in sec 3.1).

This document supersedes the "stay Python" verdict of
`docs/artifacts/audit-2026-07/port_plan.md` and its Codex review
(`port_plan_codex_review.md`) **as to language**; their
serial-generate / parallel-evaluate / serial-commit discipline and
determinism analysis remain binding and are inherited below (sec 4).

Scope of stage 1 (THIS spec): a Rust kernel replacing the internals of
`scripts/support_placement/geometry_exact.ExactSurface` behind its exact
public API, parallel via rayon across all cores, bridged to the existing
verified Python pipeline via PyO3/maturin. The Python `ExactSurface` stays
**permanently** as the differential-test oracle (sec 6) and is removed from
production call paths. Later de-Pythonization stages: sec 10.

Binding constraints carried over unchanged:
- Correctness and resin economy never trade against wall-time; timing pins
  are regression tripwires only.
- Determinism is hard: results BIT-IDENTICAL across repeated runs AND across
  thread counts (sec 4).
- Sanctioned-embed semantics (stub tips 0.3mm inside; strut anchor end-embeds
  legal; mid-path penetration and thin-feature ball breach illegal) are
  unchanged — the kernel mirrors semantics, it does not reinterpret them.
- No GPU work, no unsafe-heavy cleverness: clear, reviewable Rust. `unsafe`
  is permitted only where the numpy FFI boundary requires it (PyO3/ndarray
  view extraction), never in geometry code.

---

## 1. Crate layout, build, and dev workflow

### 1.1 Layout

```
resin-print-prep/
  rust/
    geom_kernel/
      Cargo.toml            # crate name geom_kernel, cdylib, pyo3 + numpy + rayon
      pyproject.toml        # maturin build-system config (module-name geom_kernel._native)
      src/
        lib.rs              # PyO3 module: class NativeSurface + module metadata
        mesh.rs             # triangle arrays, extents, build_log stats
        bvh.rs              # deterministic AABB BVH (sec 3.2)
        closest.rs          # closest_on_tri (Ericson), seg-seg, seg-tri exact distance
        moller.rs           # Moller-Trumbore ray/segment crossings + dedup_loc
        parity.rs           # per-axis parity + 3-axis majority vote (sec 3.4)
        queries.rs          # query/project/inside/cast_rays/segment_clearance/
                            # penetration_intervals/sphere_breach/exterior_face_mask
        par.rs              # deterministic rayon helpers (ordered map-collect only)
      tests/                # pure-Rust unit tests (closed-form geometry cases)
  scripts/support_placement/
    geometry_kernel.py      # thin Python wrapper: class KernelSurface (sec 2.1)
    geometry_exact.py       # UNCHANGED — becomes the differential-test oracle
  scripts/tests/
    test_kernel_parity.py   # permanent differential suite (sec 6)
  scripts/build_kernel.sh   # one-command build (sec 1.2)
```

Rationale: one crate, one PyO3 extension module. The Python-visible module is
`geom_kernel._native` (built by maturin into the venv's site-packages);
production code never imports it directly — it goes through
`support_placement.geometry_kernel.KernelSurface`, which owns the
numpy-boundary conventions (atleast_2d, scalar passthrough, ragged
reassembly) so the Rust side only ever sees clean C-contiguous f64/i64 arrays.

### 1.2 Build workflow

- Toolchain: cargo/rustc 1.97.1 at `/opt/homebrew/bin` (present). maturin is
  NOT installed; install into the venv:
  `"/Users/craig/Documents/Claude/Projects/D&D Campaigns/resin-print-prep/.venv/bin/pip" install maturin`
  (network — expected to need the sandbox disabled).
- Build: `scripts/build_kernel.sh` =
  `cd rust/geom_kernel && ../../.venv/bin/maturin develop --release`
  (debug builds are 10–50x slower on this workload; `--release` is the
  default; a `--profile dev-opt` (opt-level 2 + debug assertions) profile is
  provided for kernel debugging).
- Cargo.toml pins: the `numpy` crate (rust-numpy ≥ 0.21, required for numpy
  2.x) is selected FIRST and its required `pyo3` version drives the top-level
  `pyo3` pin (with `abi3-py311`; venv is Python 3.11.12) — rust-numpy
  re-exports and hard-couples to one pyo3 version, and a mismatched
  independent pin is the single most common maturin build wall ("expected
  PyArray (pyo3 vA), found (pyo3 vB)"). Plus `rayon`. `[profile.release]` keeps default
  `codegen-units` but sets `lto = "thin"`; **no** `-ffast-math`-equivalents,
  no `target-cpu=native` (sec 5.1 — reproducibility across machines beats a
  few percent).
- Version/API stamp: `lib.rs` exports `KERNEL_API_VERSION: u32` and
  `SOURCE_DIGEST: &str` (SHA-256 over the sorted `src/*.rs` contents, baked
  in by `build.rs`). `geometry_kernel.py` declares `EXPECTED_API_VERSION`;
  a mismatch raises at import with the build command in the message.

### 1.3 How pytest ensures the kernel is built

`scripts/tests/conftest.py` gains a session-scoped autouse fixture
`_kernel_gate`:

1. `import geom_kernel._native` — on ImportError, **fail** (not skip) every
   test marked `@pytest.mark.kernel` (the parity suite and any engine test
   that exercises production paths once consumers swap), with the exact
   build commands in the failure message. Skipping is forbidden: a silently
   skipped parity suite is how a stale kernel ships.
2. Check `KERNEL_API_VERSION == EXPECTED_API_VERSION` — mismatch fails the
   same way ("rebuild: scripts/build_kernel.sh").
3. Staleness advisory: recompute the source digest over `rust/geom_kernel/
   src/*.rs` and compare to the baked `SOURCE_DIGEST`; on mismatch emit a
   loud warning header ("kernel binary older than rust sources — rebuild")
   and set a flag the parity tests include in their failure messages. This
   is a warning, not a failure, because the digest can only prove staleness,
   not incorrectness — the parity tests themselves are the correctness gate.

Escape hatch for pure-Python development on machines without a toolchain:
`GEOM_KERNEL_OPTIONAL=1` converts the step-1 failure to a skip **and** prints
a summary-line warning. CI/full-suite runs never set it.

---

## 2. API surface — function-by-function mirror of ExactSurface

### 2.1 The wrapper: `geometry_kernel.KernelSurface`

`KernelSurface(mesh, grid_pitch_mm=2.0, large_cap=2000, verbose=False)` is a
drop-in for `geometry_exact.ExactSurface`: same constructor signature, same
methods, same shapes/dtypes, same scalar/batch conventions. `engine.run`
swaps one line (`geom = KernelSurface(mesh)`), everything downstream
(tree.revalidate, verify, struts/cco/bracing anchors, perception filters,
removal duck-typing via `.query`) is untouched.

The wrapper extracts, ONCE, as C-contiguous float64 copies:
`np.asarray(mesh.triangles)` (n,3,3), `mesh.face_normals` (n,3),
`mesh.area_faces` (n,), `mesh.bounds` (2,3) — and hands them to
`_native.NativeSurface`. Face normals/areas are passed IN (not recomputed in
Rust) so every downstream normal-dependent decision (project's returned
normal, sphere_breach inward cone, exterior-mask offset direction) uses
byte-identical trimesh values — one less parity surface.

### 2.2 Methods (numpy-boundary contracts)

All batch inputs are converted by the wrapper to C-contiguous float64;
outputs are freshly allocated numpy arrays owned by Python. Rust releases
the GIL (`py.allow_threads`) for every batch computation.

| Method | In (dtype/shape) | Out (dtype/shape) | Notes |
|---|---|---|---|
| `query(pts, distance_upper_bound=inf, workers=None, k=1)` | f64 (N,3) or (3,) | (f64 (N,), i64 (N,)) or scalar pair | cKDTree-compatible facade. k=1 only (NotImplementedError otherwise, as today). Over-bound points return `inf` / index `nfaces`. `workers` accepted and ignored (rayon decides). EXACT by construction (BVH branch-and-bound, sec 3.3 — no certificate/escalation needed). |
| `n` (property) | — | int | = nfaces (cKDTree.n compat). |
| `project(pts)` | f64 (N,3) or (3,) | (f64 (N,), f64 (N,3), i64 (N,), f64 (N,3)) | (unsigned dist, closest point, tri idx, face normal). Normal is the trimesh-supplied row for the argmin face. |
| `inside(pts, batch=40000)` | f64 (N,3) or (3,) | bool (N,) or bool | 3-axis ray-parity MAJORITY vote, the ONLY inside/outside classifier. `batch` accepted and ignored (kernel streams; no O(N·cand) intermediates exist to bound). |
| `cast_rays(origins, dirs, t_max=1.0)` | f64 (n,3), f64 (n,3), float | list of n pairs `(t_sorted f64 (k_i,), faces i64 (k_i,))` | Native returns flat `(offsets i64 (n+1,), t_all, faces_all)`; the wrapper slices into the list — one FFI crossing for the whole batch, identical list-of-pairs shape to today. Crossings deduped by arc location, tol 3e-3mm (dedup_loc semantics, sec 3.5). |
| `segment_clearance(p0s, p1s, r0s, r1s, standoff, inset0s=None, inset1s=None, chunk_mm=5.0)` | f64 (N,3)x2; r/inset scalar-or-(N,) broadcast | (f64 (N,), f64 (N,3)) | (min clearance over sub-segment [inset0, L−inset1], argmin surface point; NaN point only in the degenerate cases: L < 1e-9 or sub-segment entirely inside embed zones). Conservative r_max taper fallback preserved verbatim (over-flags only, never under-flags). `chunk_mm` accepted; the BVH kernel does not need chunked crops but honors the parameter for signature parity (ignored). **Sanctioned Tier-1 deviation (Amendment A1):** the oracle's crop-empty "open space" branch returns `min(endpoint-to-nearest-CENTROID) − thresh` and a NaN argmin point (`geometry_exact.py` 693–697) — a certificate value, not the physical distance. The kernel does NOT reproduce it (no centroid tree exists); it always returns the exact segment-to-surface clearance and a real argmin point. This is safe by construction: the oracle's Tier-1 fires only when the sound crop certificate proves true distance > thresh, so BOTH values are strictly positive (verdict parity exact), and exact ≤ centroid-endpoint distance so `clr_kernel ≤ clr_oracle` always. Value 1e-9-parity applies ONLY in the oracle's Tier-2 regime (sec 5.3, 6.3). |
| `penetration_intervals(p0, p1, r0, r1, n_offset_rays=8, majority_step_mm=0.12, cap=80, min_illegal_len_mm=0.05, rays="axis")` | f64 (3,)x2, floats | list[dict(s_a, s_b, entered, exited, max_depth_mm, ray)] | Census PASS A–C, ported semantics-for-semantics: Moller crossings partition each ray; each sub-segment classified by dense-majority parity vote (`mean >= 0.5`) of `min(cap, max(3, floor(Lsub/step)))` interior linspace samples; maximal inside runs longer than min_illegal_len become intervals; depth = max unsigned distance of inside samples. `rays="axis"|"all"` (all = axis + n capsule-offset rays, member_rays construction verbatim). Native returns struct-of-arrays + ray-id; wrapper builds the dicts and "axis"/"offK" labels. **Amendment A7:** each single-member call is ONE native call — the internal parity votes and inside-sample nearest-distances run inside the kernel, never by re-entering the Python wrapper's `inside()`/`query()`. Because this is the one genuinely per-element method in the surface (an acknowledged exception to R1's batch-only rule), the native layer ALSO exposes `penetration_intervals_batch(members as SoA + offsets) -> SoA + offsets`; the wrapper keeps the single-member signature for drop-in parity and routes any caller holding N members (census-parameter revalidate) through the batch entry point. Sample positions use the pinned linspace formula (sec 5.1). |
| `sphere_breach(centers, radii, eps_mm=0.05, half_deg=25.0, n_cone=8)` | f64 (N,3), scalar-or-(N,) | (bool (N,), f64 (N,), f64 (N,)) | (breached, feature_thickness_mm, poke_out_mm). Cone construction (`_cone_dirs`), near-wall skip (`> nd + 0.05`), and the reach precondition (`r − dist > eps`) verbatim. thickness = inf when no far wall. |
| `exterior_face_mask(gap_warn_mm=0.15)` | — | bool (nfaces,), cached | 3 fixed barycentric points × eps ∈ {0.05, 0.15} along +normal = 6 parity samples; INTERIOR iff ≥ 4/6 inside; fail-open to EXTERIOR. **Gap diagnostic split (Amendment A10, reconciling the old 2.2/4.2 contradiction):** the WRAPPER owns everything numpy-flavored — it draws the seeded face sample (`RandomState(0).choice(n, 1500, replace=False)`, identical to the oracle) and computes the percentiles/median (numpy's default linear interpolation — NOT reimplemented in Rust) from raw first-crossing gap distances; the KERNEL only performs the ray casts (a plain `cast_rays` batch). `._gap_diag` dict assembled wrapper-side for log parity. |
| `build_log` (attr) | — | dict | nfaces, grid_pitch_mm carried over; adds bvh_depth, bvh_nodes, n_degenerate_tris, t_build_ms. `large_thr`/`n_large`/`tri_extent_*` are retired (the BVH has no never-miss set); keys are kept with `None` values for one transition release so log-diffing tools don't crash, then dropped. |

Module-level helpers consumed by tests (`closest_on_tri`, `dedup_loc`,
`member_rays`, `perp_basis`, `_cone_dirs`, `rss_gb`) remain importable from
`geometry_exact` unchanged — the oracle keeps them alive; the kernel does not
re-export them.

### 2.3 Semantics guarantees (what "mirror exactly" means)

- f64 throughout; all tolerances (`EPS=1e-6` Moller barycentric slack,
  1e-14 det/denominator guards, 1e-9 parity strict-above slack, zdedup 5e-3,
  dedup_loc 3e-3, near-wall 0.05, 1e-300 Ericson denom regularizer) are
  copied verbatim as named constants in one Rust module (`consts.rs` section
  of mesh.rs) with a comment linking each to its geometry_exact.py line.
- Tie-breaking (Amendment A3 — the previous "both resolve to lowest face
  index" claim was FALSE for the oracle and is withdrawn): the KERNEL resolves
  argmin ties to the lowest face index (strictly-less updates, leaves visited
  in ascending face index) — deterministic. The ORACLE does not and cannot:
  point-nearest argmin runs over candidates in kNN-CENTROID-distance order
  (`_closest_over_candidates`, geometry_exact.py 450, first-minimum wins),
  and segment-clearance candidates come from a Python `set` in iteration
  order (683–698). Both orders are cKDTree-/hash-internal and not
  specifiable. Consequences and mitigations:
  - Index parity is asserted only in constructed tie-free cases (sec 6.3);
    elsewhere argmin points/indices are checked by consequence (sec 5.3).
  - On EXACT distance ties between coincident soup triangles with OPPOSITE
    winding (the two-box fixture; unmerged internal walls), `project()` may
    return opposite normals between oracle and kernel, and that normal feeds
    sphere_breach's inward cone and the exterior-mask offset direction. The
    fuzz therefore compares project normals UP TO SIGN
    (`|dot(n_k, n_o)| ≥ 1 − 1e-9`); a sign disagreement is recorded as
    measured-tie telemetry (tri indices differ AND distances equal to
    1e-12), not a failure — but any DOWNSTREAM boolean divergence
    (breached, exterior-mask bit) traced to a normal flip is a BLOCKER to be
    resolved by fixture construction (move the probe off the tie), never by
    loosening the boolean contract. Normal non-parity on measured coincident
    ties is documented, bounded, and telemetered — not silent.
- Embed-zone semantics (`s_lo=inset0`, `s_hi=L−inset1`, "entirely inside the
  embed zones ⇒ clearance +inf"), degenerate-segment (L < 1e-9 ⇒ inf/[]),
  and empty-input shapes are contract, verbatim.

---

## 3. BVH design + batched query algorithms (and how the sort dies)

### 3.1 The profiled pathology being eliminated

The Python core has no spatial index that answers queries directly; it
composes cKDTree crops with per-batch bookkeeping that SORTS:

- `VerticalParity.inside` re-sorts every batch: `argsort(cid)` + `np.unique`
  to group query points by grid cell — int64 quicksort per call, on every one
  of the millions of parity samples' batches (this is the sampled hotspot).
- `np.union1d(cand, large)` — sort-based set union per ray (cast_rays), per
  ball (sphere_breach), per member (penetration_intervals), per escalated
  query.
- `_one_segment_clearance` builds a Python `set` of candidate ints per
  segment chunk, then `np.fromiter` — hash + boxing churn on the hottest
  call path (revalidate runs it tens of thousands of times).
- `zc.sort(axis=1)` over padded (points × candidates) crossing matrices.

All of these exist only because the crop machinery returns unordered
candidate id sets that then need grouping/unioning. A BVH answers each query
by traversal — the candidate set is never materialized, so there is nothing
to sort, unique, or union. The remaining sorts are per-ray crossing lists
(a handful of floats each) — negligible by construction.

### 3.2 BVH structure (deterministic, soup-tolerant)

- Binary AABB BVH over triangle indices, built once in the constructor.
  Leaf size ≤ 8 triangles. Node AABBs padded by 0.0 (exact tri bounds).
- Split rule (fully deterministic, no RNG, no pointer/hash order): binned
  SAH over 16 bins on the axis with the largest centroid extent; ties on
  axis choice broken by axis order x<y<z; ties on bin cost broken by lowest
  bin index; if SAH degenerates (all centroids identical — coincident soup
  duplicates), fall back to a median split ON INDEX ORDER (stable partition
  by original face index). Build input order is the face order of the STL,
  so two builds of the same mesh are structurally identical.
- Serial build. 576k faces ≈ tens of ms–low hundreds of ms; not worth
  parallel-build determinism analysis. (Re-evaluate only if profiling shows
  otherwise; a deterministic parallel build via fixed-partition subtree
  construction is the known upgrade path.)
- Degenerate triangles (zero area / zero-extent AABB) are kept, counted in
  `build_log.n_degenerate_tris`, and handled by the same guards the Python
  math uses (1e-14 det/denominator masks) — they can never be hit by Moller
  (det≈0 ⇒ invalid) and contribute point/segment distances via the Ericson
  clamps exactly as in Python.
- The `large`-triangle never-miss machinery is RETIRED: it existed to patch
  the centroid-KDTree crop's unsoundness for oversized triangles. BVH AABB
  traversal is sound for any triangle size by construction.

### 3.3 Query algorithms

- **Nearest triangle (query/project):** classic branch-and-bound: traverse
  children nearer-AABB-first, prune nodes with `aabb_dist ≥ best`, compute
  `closest_on_tri` at leaves. Exact — replaces the Python k-NN + exactness
  certificate + escalation dance wholesale. `distance_upper_bound` seeds
  `best` for free pruning. Tie-break: strictly-less updates + leaf triangles
  visited in ascending face index ⇒ lowest index wins.
- **Ray casts (cast_rays, Moller passes):** slab-test traversal clipped to
  [0, t_max]; collect ALL crossings (no early-out — parity needs every
  crossing), then sort the per-ray hit list (k ≈ 0–40 floats) and dedup by
  arc location (3e-3mm tol, keep-first semantics identical to `dedup_loc` +
  representative-face mapping).
- **Segment/capsule minimum distance (segment_clearance):** branch-and-bound
  with lower bound = segment-to-AABB distance − (r_max + standoff is applied
  after; the bound prunes on raw distance vs current best). At leaves, the
  exact seg-tri decomposition verbatim from `_seg_tri_dist`: (a) both
  endpoints vs triangle (Ericson), (b) segment vs each of the 3 edges
  (clamped seg-seg), (c) pierce ⇒ 0 at the Moller crossing point. Early
  termination when best hits 0 (pierce) is allowed; the argmin point is then
  the crossing point of the FIRST pierce encountered in the kernel's fixed
  traversal order (nearer-AABB-first, leaves in ascending face index) —
  deterministic and thread-invariant, but NOT guaranteed lowest-index
  (Amendment A8: a lowest-index guarantee would require visiting all
  piercers, defeating the early-out, and the oracle's own pierce argmin runs
  in Python-set iteration order, so no lowest-index reference exists on
  either side; the prior spec text claiming the first (lowest-index) piercing
  triangle wins is WITHDRAWN). Argmin-point parity for pierces is
  consequence-based only: the point lies on the mesh surface and yields
  clearance = −thresh (sec 5.3).
- **Parity (inside):** sec 3.4. **Intervals/breach:** compositions of the
  above three primitives with census constants, per sec 2.2.

### 3.4 Parity without per-batch sorting

Keep the EXACT census parity semantics (axis-aligned ray, crossings strictly
above with 1e-9 slack, zdedup 5e-3 arc dedup, odd ⇒ inside, 3-axis majority
≥ 2) but restructure the data:

- Per axis, build ONCE at construction a CSR grid: 2D cells of pitch
  `grid_pitch_mm` over the projected plane; triangles binned to every cell
  their 2D AABB overlaps, laid out as `(cell_start: Vec<u32>, tri_ids:
  Vec<u32>)` by counting sort — no per-query sorting ever again. (This is
  the same grid the Python builds; the Python's sin was re-grouping the
  QUERIES per batch, not the grid itself.)
- Per query point: locate its cell in O(1), scan the cell's triangle list,
  accumulate crossing z's into a small stack vector, sort that (typically
  < 16 entries), cluster-count, take parity. No cross-query state ⇒
  embarrassingly parallel, no grouping needed.
- **Parity dedup is the consecutive-difference CLUSTER COUNT, NOT dedup_loc
  (Amendment A2 — these are two different algorithms and conflating them
  flips verdicts):** the oracle's `VerticalParity.inside`
  (geometry_exact.py 259–267) computes, over the ascending finite crossing
  z's, `uniqn = 1 + count(z[i+1] − z[i] > zdedup)` (0 if no finite
  crossings), masking non-finite pairs — i.e. adjacent-gap chaining.
  `dedup_loc` (134–144) instead keeps a crossing when it exceeds the LAST
  KEPT value by tol. They disagree on chained sub-tolerance crossings:
  z = [0, 0.004, 0.008] at tol 5e-3 gives cluster-count 1 (odd ⇒ INSIDE)
  but keep-first 2 (even ⇒ OUTSIDE) — and chained sub-5µm crossings are
  exactly what the non-watertight overlapping assembly produces near shell
  overlaps and at the exterior-mask eps 0.05/0.15 offsets, where a mis-vote
  silently deletes real support demand. The Rust kernel MUST implement the
  parity path with the verbatim `1 + Σ(diff > zdedup)` cluster count and
  MUST NOT reuse the keep-first dedup helper for it; `dedup_loc` keep-first
  semantics apply ONLY to cast_rays/penetration-interval arc dedup. A
  dedicated parity fixture pins a chained-crossing case (three coincident
  walls < zdedup apart) on both implementations.
- **CSR out-of-range rule:** the oracle's dict-backed grid simply lacks keys
  for cells no triangle touches, including negative/aliased flat ids from
  out-of-bounds queries (`grid.get` ⇒ None ⇒ zero crossings). The dense CSR
  must reproduce this: any query whose computed (ix, iy) falls outside the
  built grid range — negative included — maps to the EMPTY candidate list.
  Never clamp a query into the nearest edge cell: clamping would hand it a
  candidate set the oracle never consults. (Aliased in-range collisions are
  benign — the 2D barycentric containment re-filters — but the unknown ⇒
  empty mapping must be explicit in code.)
- Candidate-set equivalence argument (why verdicts match Python exactly):
  both implementations consider every triangle whose 2D AABB overlaps the
  query's cell, and a triangle whose projection contains the query point
  necessarily overlaps that cell, so both candidate sets are supersets of
  the true crossing set and both apply identical containment tolerances ⇒
  identical crossing multisets ⇒ identical dedup ⇒ identical parity bits.
  (Cell assignment uses the same `floor((c − origin)/h)` arithmetic and the
  same origin/NY construction so boundary points bin identically.)

### 3.5 Memory contract

The Python ROW_BUDGET / RSS-growth machinery exists to bound numpy's
O(queries × candidates) intermediates. The kernel has none: per-query state
is a traversal stack (≤ 64 entries) and a small hit vector. Peak memory =
input arrays + BVH (~24 bytes/tri node budget, ~576k tris ⇒ tens of MB) +
output arrays. The wrapper keeps the suite's <2GB RSS discipline trivially;
the parity fuzz harness still batches queries ≤ 2048 (test-side rule).

---

## 4. Rayon parallelization — deterministic by construction

### 4.1 The one sanctioned pattern

**Parallel across query indices only; every query is a pure function of
(immutable kernel state, its own inputs); results land in the output slot of
their own index.** Concretely: `par_iter` over `0..N` with
`map(|i| answer(i)).collect::<Vec<_>>()` or `par_chunks_mut` writes into a
pre-allocated output — rayon's indexed collect preserves index order
regardless of which thread computed what. There is NO cross-query reduction
anywhere in the API surface: no sums, no global argmins, no accumulators.

Thread-count invariance argument, concretely:
1. Kernel state (triangle arrays, BVH, parity CSR grids) is built serially
   and is immutable (`&self`) during queries — identical bytes for any
   thread count.
2. Each query's answer is computed by a serial per-query algorithm
   (traversal order fixed by the BVH structure + nearer-child-first rule on
   values that do not depend on other queries). Same inputs ⇒ same float ops
   in the same order ⇒ same bits, on any thread.
3. Placement of answers is by index, not completion order. Therefore the
   output arrays are bit-identical for 1, 8, or 16 threads. ∎
4. Within-query reductions (min over traversed leaves, parity counts) are
   sequential per query — never split across threads.

### 4.2 Explicit prohibitions (review checklist items)

- No `rayon::iter::sum/reduce/fold` over floats, ever (regrouping changes
  results). If a future method genuinely needs a cross-query reduction, it
  must gather per-index partials into a Vec and reduce SERIALLY in index
  order — merge in fixed index order, never completion order.
- No `HashMap`/`HashSet` iteration feeding any result (iteration order leaks
  RandomState). Grids/sets are Vec/CSR by construction; if a map is ever
  unavoidable, results must be extracted via sorted keys — but prefer
  restructuring so no map exists.
- No wall-clock reads in the kernel. No time-based work stealing decisions
  affecting results (rayon steals work, but per sec 4.1 stealing cannot
  change any output bit).
- Thread pool (Amendment A4 — the global pool would make the thread-sweep
  test INERT: rayon's global pool initializes once per process, so a pytest
  process sweeping {1, 2, all} via env changes would silently run all three
  cases at the first-initialized count and the R6 tripwire would never
  fire): the kernel NEVER uses rayon's global pool. It maintains a cache of
  explicit `rayon::ThreadPoolBuilder::num_threads(k).build()` pools keyed by
  k; each batch call re-reads `GEOM_KERNEL_THREADS` (0/unset = all cores —
  the multi-core mandate), fetches or builds the pool for that count, and
  runs the parallel work under `pool.install(...)`. The native module
  exposes `current_threads() -> usize` (the pool size the next call will
  use) and the thread-sweep test ASSERTS the effective count actually
  changed between cases before comparing outputs — a sweep that cannot
  demonstrate it varied the thread count fails loudly instead of passing
  vacuously (sec 6.5).
- exterior_face_mask and the gap diagnostic parallelize per-face / per-ray
  under the same pattern. The gap diagnostic keeps its seeded sample: the
  wrapper draws the `RandomState(0)` choice of face ids and passes them into
  the kernel's ray-cast batch, so the sampled set is identical to the
  oracle's; percentile/median statistics stay wrapper-side in numpy (sec 2.2
  Amendment A10).

### 4.3 Effect on pipeline-level determinism

engine.run remains serial Python orchestration calling batch kernel methods;
since each call is bit-deterministic and thread-count-invariant, the whole
run inherits run-to-run and machine-thread-count invariance for every
geometry result. (The pre-existing `rewire` wall-clock cap issue flagged by
the Codex review is OUT of this spec's scope but noted in sec 10 stage 2 —
the kernel makes revalidate faster, which can change how many rewire moves
fit under the cap; the full-scale acceptance run must record rewire's
cap-hit status so any layout delta is attributed, not mysterious.)

---

## 5. Float-parity policy vs the Python oracle

### 5.1 Compilation discipline

- No fast-math anywhere. Rust/LLVM does not contract `a*b+c` into FMA unless
  asked; we do not ask (no `fma()` calls, no `-C target-feature` opt-ins, no
  `target-cpu=native`). `f64::sqrt` is IEEE-754 correctly rounded, as is
  numpy's — identical inputs give identical square roots.
- Same algebraic sequence as the numpy code wherever the result feeds a
  comparison: closest_on_tri, seg-seg, Moller are ported term-for-term
  (the Python is already scalar-formula math broadcast over arrays; the Rust
  writes the same formulas per element).
- **Pinned operand-construction sequences (Amendment A6 — "term-for-term"
  alone is not enough; the oracle's bits come from specific
  reconstruct-then-resubtract patterns an idiomatic Rust port would
  "optimise" away):**
  - `closest_on_tri` is ALWAYS fed reconstructed vertices `A = V0`,
    `B = V0 + E1`, `Cc = V0 + E2` (geometry_exact.py 447, 605) and then
    internally recomputes `ab = B − A`, `ac = Cc − A` (line 84). In f64,
    `(V0 + E1) − V0 ≠ E1`; the kernel must perform the same
    add-then-subtract, NOT feed E1/E2 directly, and must produce the output
    point as `A + ab·v + ac·w` from the recomputed edges.
  - Moller-Trumbore, by contrast, consumes the RAW stored `E1`, `E2`
    (lines 110–131, 560–568). The two primitives use DIFFERENT vertex
    handling; port both exactly as-is.
  - Parity z-interpolation uses `v0 = C − A`, `v1 = B − A` (line 232) and
    the non-standard A/C/B weighting
    `z = (1 − a − b)·Az + a·Cz + b·Bz` (line 252) — a maps to C, b maps to
    B. Do not "fix" this to A/B/C.
  - `penetration_intervals` sample positions follow numpy's linspace
    algorithm verbatim: `step = (tb − ta)/(ns + 1)`, sample
    `t_i = ta + i·step` for i = 1..ns (the [1:-1] slice of
    `linspace(ta, tb, ns+2)`; numpy's endpoint override is sliced away and
    does not apply). NOT the naive closed form
    `ta + (tb − ta)·i/(ns + 1)`, which differs by ULPs and can flip an
    on-surface sample's parity bit. `ns = min(cap, max(3,
    trunc(Lsub/majority_step_mm)))` with Python `int()` truncation toward
    zero, verbatim (line 761).

### 5.2 Where parity is EXACT (bit-for-bit), and why

- **Parity/inside verdicts, crossing counts, interval structure:** integer
  results of predicates with fixed tolerances over identically-computed
  floats (sec 3.4 candidate-equivalence + verbatim constants) — exact.
- **Moller t-values, closest_on_tri distances, seg-seg distances for a GIVEN
  candidate triangle:** same formula, same order, no reductions ⇒ expected
  bit-identical. The parity suite ASSERTS bit-equality on the module-level
  primitive comparisons (single triangle / single segment closed-form
  cases, sec 6.2) — if the toolchain ever breaks this (e.g. auto-
  vectorization changing an intermediate), the assertion localizes it
  immediately and the policy for that primitive drops to 1e-9-relative with
  a written note. Bit-equality here is an empirical pin, not an IEEE
  guarantee — treat failures as investigate-then-document, not
  auto-loosen.

### 5.3 Where parity is 1e-9-relative, and why

- **Batch-level minima (query/project/segment_clearance):** the oracle and
  kernel examine candidates in different orders (k-NN crop vs BVH
  traversal). The min VALUE over the same candidate set is
  order-independent only if all candidate distances are themselves
  bit-identical; sec 5.2 says they should be, but the oracle's certificate/
  escalation path can evaluate a candidate via a different code path
  (query_ball_point block vs k-NN block) with different broadcast shapes —
  numpy reductions over different shapes may associate differently.
  Contract: `|d_kernel − d_oracle| ≤ 1e-9 · max(1, |d_oracle|)`.
  **Scope restriction (Amendment A1):** for segment_clearance this contract
  applies ONLY when the oracle answers via Tier-2 (non-empty crop). In the
  oracle's Tier-1 open-space regime the two implementations return different
  quantities by design (sec 2.2); there the contract is: both clearances
  strictly positive (sign/verdict exact), `clr_kernel ≤ clr_oracle`, and the
  oracle's NaN argmin point is not compared (kernel's real point checked by
  consequence only). The fuzz classifies every draw by oracle tier
  (Tier-1 signature: NaN argmin point with finite positive clearance)
  before choosing which contract to assert (sec 6.3).
- **penetration_intervals s_a/s_b/max_depth, sphere_breach thickness/poke:**
  compositions of the above ⇒ same 1e-9-relative bound.
- **Argmin POINTS and tri indices:** compared via their consequences, not
  bitwise: the reported point must lie within 1e-9 of some true nearest
  feature and its distance must match the reported distance to 1e-12 abs;
  indices must agree except on measured ties (sec 2.3).

### 5.4 Why verdict BOOLEANS are always exact anyway

Every boolean the pipeline consumes (inside, breached, interval-exists,
clearance-negative as consumed by revalidate/verify) thresholds a quantity
whose decision margins are physical: eps_mm=0.05, min_illegal_len=0.05mm,
standoff ~0.5mm, contact_depth 0.3mm — 10^6× the 1e-9 numeric skin. Policy:
1. Parity/crossing-derived booleans are exact by construction (sec 5.2).
2. Threshold booleans are asserted EXACTLY equal in the parity suite; the
   seeded fuzz generator REJECTS (and re-draws) any case whose decision
   quantity lands within 1e-6 of its threshold (measured by the oracle), so
   a knife-edge draw cannot manufacture a spurious mismatch. Fixture cases
   are constructed with mm-scale margins. Any surviving boolean mismatch is
   a BLOCKER bug, never a tolerance note.
3. **Scope note (Amendment A5):** the 1e-6 band applies only to CONTINUOUS
   decision quantities (clearance vs 0, thickness vs r − eps, interval
   length vs min_illegal_len, s_a/s_b). The penetration-interval majority
   vote (`mean(inside) ≥ 0.5` over ns samples) is a DISCRETE count with no
   continuous margin to band — its exactness rests instead on construction:
   with sample positions pinned bit-identical (sec 5.1 linspace pin) and
   per-sample parity bits exact (item 1), the boolean sample vector is
   bit-identical on both sides, so the integer count, the mean comparison,
   and the resulting interval structure (existence, entered/exited) are
   exact with no tolerance involved. Any interval-structure mismatch
   therefore indicates a pin violation (sample positions or parity bits),
   not a knife-edge draw, and is a BLOCKER.

---

## 6. Permanent differential/parity test suite

`scripts/tests/test_kernel_parity.py`, marked `@pytest.mark.kernel`,
permanent (it outlives the transition — it is the contract that any future
kernel change still matches the frozen oracle). The Python `ExactSurface` is
imported ONLY here and in its own unit tests once consumers swap.

1. **Fixture battery** (meshes from `mesh_io`: box, overhang_table,
   thin_fin, toed_slab, raking_blade (0.03mm blade), the two-box
   non-watertight soup from test_geometry_exact, and the thin-twig +
   thick-slab breach fixtures): for each, oracle vs kernel on:
   - the exact scenario tests already pinned in test_geometry_exact.py
     (thin-twig r=1.0 engulfing the 0.08mm twig MUST breach both sides
     identically; sanctioned 0.3mm stub embed MUST NOT flag; stub-through-
     0.5mm-fin interval > 0.05mm on both; strut mid-path 0.2mm fin clip
     fails both; two-box parity table incl. the points-inside-B-outside-A
     killer; exterior mask per-face boolean vector EXACTLY equal).
   - PLUS a chained-crossing parity fixture (Amendment A2): three parallel
     coincident walls spaced < zdedup (5e-3mm) apart, probed above/below/
     between — pins the cluster-count dedup (1 crossing cluster ⇒ inside
     parity) against the keep-first algorithm that would report 2 and flip
     the verdict; asserted identical on oracle and kernel.
2. **Primitive bit-parity pins**: closed-form single-triangle /
   single-segment / single-ray cases asserted bitwise (sec 5.2).
3. **Seeded fuzz** (per fixture mesh; `np.random.default_rng(0xB51)` streams,
   batches ≤ 2048, bounded per test to keep RSS < 2GB):
   - 10k random points in a 1.5×-bounds box → query/project/inside:
     distances 1e-9-rel, inside EXACT per-point; project normals compared
     UP TO SIGN with sign-flips recorded as measured-tie telemetry
     (sec 2.3 Amendment A3).
   - 2k random segments (mix of lengths 0.1–40mm, some grazing the surface
     by construction: point-on-surface + jittered offsets) with random
     radii/insets → each draw first CLASSIFIED by oracle tier (Amendment
     A1: Tier-1 signature = NaN argmin point with finite positive
     clearance). Tier-2 draws: value 1e-9-rel; sign EXACT after
     threshold-rejection redraw (sec 5.4). Tier-1 draws: both signs
     strictly positive AND `clr_kernel ≤ clr_oracle`; no value or point
     comparison. The generator ensures ≥ 500 draws land in EACH tier so
     neither regime goes untested.
   - 1k random rays → cast_rays crossing COUNTS exact, t-values 1e-9-rel
     elementwise after alignment.
   - 500 random balls seeded ON the surface (project a random point, place
     center at surface ± jitter) → breached EXACT, thickness/poke 1e-9-rel.
   - 300 random members → penetration_intervals: interval count, entered/
     exited flags, ray labels EXACT; s_a/s_b/depth 1e-9-rel.
4. **Verdict-boolean census replay (fixture-scale)**: run
   `verify.self_verify` gate inputs (clearance_report, ball_breach_report)
   on the table + toed_slab engines with oracle-geom and kernel-geom;
   violation LISTS (ids + booleans) must be exactly equal.
5. **Determinism/thread-invariance**: every fuzz output above recomputed
   with `GEOM_KERNEL_THREADS ∈ {1, 2, 0(all)}` and twice at fixed threads —
   byte-identical (np.array_equal on raw bytes) across all five runs.
   The test asserts `_native.current_threads()` reports the expected count
   for each case BEFORE comparing outputs (Amendment A4: per-call
   `ThreadPoolBuilder` pools make the env override effective mid-process;
   a sweep that cannot prove it varied the count fails, so the tripwire
   can never pass vacuously).
6. **Anti-drift**: the suite records the oracle-side answers' SHA-256 per
   test into `scripts/tests/fixtures_kernel_parity.json` on first freeze;
   both oracle and kernel must keep matching it, so an accidental oracle
   edit cannot silently re-baseline the kernel (the oracle is FROZEN
   reference code now).

Runtime budget: the whole parity suite must stay in tens of seconds (kernel
side is fast; the ORACLE is the slow half — fixture meshes are ≤ ~5k faces,
so the Python side stays tractable forever).

---

## 7. Per-stage progress logging in engine.run (observability)

Owner-mandated: a full-scale run must never again be a heartbeat-only black
box. Implementation is deliberately tiny:

- `engine.py` gets a module-level `def _stage(log, name, t0)` context manager:
  on enter prints `[v5] >> {name}` and on exit prints
  `[v5] << {name} dt={dt:.1f}s t=+{total:.1f}s` to **stderr** (stdout stays
  clean for result piping), `flush=True`, and records `log[f"t_{name}"]`
  (preserving today's key names where they exist: t_geom_build,
  t_surface_tree, t_islands, t_minima, t_island_births, t_total unchanged).
- Wrapped stages in `run()`: `geom_build` (KernelSurface construction),
  `prepare` (whose internal perception timings already land in log),
  `surface_tree` (until retired), then per fix-pass: `construct[p{i}]`,
  `rewire[p{i}]`, `brace_and_size[p{i}]`, `verify[p{i}]`, and `finish`.
- Kernel-side: `NativeSurface` stays silent (library discipline); its
  constructor returns `t_build_ms` inside build_log, which the geom_build
  stage line includes. `verbose=True` on the wrapper prints the build_log
  exactly as ExactSurface does today.
- Long kernel calls need no sub-progress: post-port each batch call is
  seconds-scale (sec 8); the stage granularity above bounds
  silent-gap length to a stage, not a run.

---

## 8. Projected full-scale runtime, with reasoning

Hardware: Apple M3 Max, 16 cores (12P+4E), 128GB.

Reference points (measured):
- Old point-cloud engine, full treant: 193.8s single core, of which ~134s is
  the fix-pass (pure-Python tree math, NOT geometry queries) and ~60s
  prepare/mesh stages.
- Python exact core: killed at 2h22m; the overrun is concentrated in exactly
  the code this kernel replaces (revalidate clearance, parity sampling,
  anchor rays, exterior mask) — i.e. the geometry share ballooned from
  ~10–30s (cloud) to >8000s (Python-exact).
- The standalone census (same math, saner batching) exact-swept the full
  1,625-member forest in single-digit minutes single-core.

Kernel-side estimate: BVH nearest/ray queries on 576k triangles cost ~1–5µs
each in mainstream Rust implementations at this scale (traversal depth ~20,
leaf work ≤ 8 tris); parity votes are 3 such-shaped scans of CSR cells.
Budgeting generously: ~10⁷ elementary queries per full run (revalidate
trials × segments × chunk probes + parity samples + exterior mask's 3.5M
parity votes ≈ 10.4M axis-scans + anchors + census-parameter interval tests)
⇒ ~30–100s single-thread of kernel work, ÷ ~10 effective cores (rayon,
embarrassingly parallel, P/E mix) ⇒ **~5–15s of kernel wall time**, with
exterior_face_mask (~3.5M votes ⇒ ~10M axis scans) about 1–3s of it.

Pipeline projection: total ≈ Python orchestration remainder (fix-pass tree
math ~134s at v4 scale — unchanged by this stage, likely somewhat larger
under v5's stricter checks driving more trials) + prepare (~50s, still
Python; island_births 45s is stage-2 material) + kernel seconds. **Honest
projection: ~3–5 minutes full-scale wall, of which the geometry kernel is
seconds.** Acceptance is NOT a timing gate (binding ruling) — the number is
a tripwire: the first full run records per-stage times (sec 7) and the
independent penetration census (census_core_v5/census_run_v5 in
`/Users/craig/.claude/jobs/0ddf6d41/tmp/`, kept Python, kept independent —
NEVER ported, never calling the kernel) is run against its emitted forest as
the full-scale correctness check standing in for the missing golden
reference. Honest scope of that check (Amendment A9): the census is an
independent CODE PATH but shares algorithmic lineage with the oracle
(geometry_exact.py is ported from census_core), so it certifies that the
kernel matches the trusted Python geometry family at full scale — its
intended job — not that the physics is correct in some absolute sense; a
blind spot common to the shared ancestor would be invisible to it. Given no
golden reference exists, this is the strongest available backstop and any
census/kernel disagreement remains a BLOCKER. If measured kernel time exceeds projection by >10×, that is a
bug-hunt trigger, not a shrug.

---

## 9. Risk register

| # | Risk | Exposure | Mitigation |
|---|---|---|---|
| R1 | **PyO3/numpy boundary cost**: per-call overhead (~µs) + copies of query arrays could recreate death-by-a-thousand-calls if hot loops make scalar calls | revalidate calls segment_clearance per dirty SET (already batched); but `_one_segment_clearance`-style per-member Python loops in consumers would serialize | API is batch-only (arrays in/arrays out, ragged via offsets) with ONE acknowledged exception: penetration_intervals is per-member by oracle signature — mitigated by Amendment A7 (single call = one native call, internal parity/nearest never re-enter Python; `penetration_intervals_batch` SoA entry point for N-member callers such as census-parameter revalidate); wrapper never loops per element; GIL released per call so scipy/numpy work can overlap; parity microbenchmark (sec 6 budget + test_microbenchmark convention) reports per-call overhead so regressions are visible |
| R2 | **BVH degeneracy on triangle soup**: duplicate/coincident triangles, zero-area slivers, huge coordinate spread → deep/unbalanced trees or infinite split recursion | 576k-face soup, unmerged vertices, known degenerates | Leaf-size floor + index-median fallback split guarantees termination and O(log n) depth; degenerate count logged; Rust unit tests build the BVH over pathological soups (all-identical tris, 1e-9-area slivers, mixed-scale) and assert depth bounds + query correctness vs brute force |
| R3 | **Float divergence flips a verdict at scale** (fixtures can't cover 576k-face adversity) | breach/clearance thresholds near-zero margins DO occur in the wild | Threshold-band telemetry: kernel-side counters for decisions landing within 1e-6 of their threshold, reported in the full-run log; independent Python census over the emitted forest is the acceptance backstop; any census/kernel disagreement is a BLOCKER |
| R4 | **macOS codesign/loading quirks**: locally-built cdylib in a venv under `~/Documents` (TCC-protected), ad-hoc signature invalidated by post-build mutation, or quarantine on copied artifacts | maturin develop installs into `.venv/lib/python3.11/site-packages` | maturin's local build needs no notarization (ad-hoc signing is automatic on arm64); never `strip`/edit the .so post-build (invalidates ad-hoc sig ⇒ SIGKILL on load); if dlopen fails with code-signature error, re-run `codesign -s - --force` on the .so; TCC: build happens inside the repo dir the shell already reads — the earlier port_plan TCC incident was about the STL, not build products; keep cargo target/ inside rust/geom_kernel (repo-local, no /tmp cross-volume moves) |
| R5 | **Toolchain/env drift**: numpy 2.x ABI vs the `numpy` crate; abi3 vs 3.11; maturin not installed; sandbox blocks crates.io | first build + every fresh machine | Pin crate versions in Cargo.toml + commit Cargo.lock; abi3-py311 floor; build_kernel.sh checks maturin and prints the pip command; document that cargo fetch/pip need the sandbox disabled (expected); conftest gate (sec 1.3) turns a stale/missing kernel into an actionable failure, never a silent skip |
| R6 | **Rayon nondeterminism creep**: a future edit adds a parallel float reduction or a HashMap and silently breaks bit-invariance | ongoing | sec 4.2 prohibitions are a review checklist in lib.rs doc comments; parity suite's thread-sweep (sec 6.5) is the tripwire and runs in the default suite, not opt-in; `cargo clippy` deny-list note for `std::collections::HashMap` in src/ (grep-based Rust unit test) |
| R7 | **Oracle drift**: someone "fixes" geometry_exact.py and the kernel now matches a moved target | permanent-suite era | Oracle frozen (sec 6.6 hash pin); changes to either side require updating the pinned hashes in the same commit with a written rationale |
| R8 | **Coverage/behavior deltas at full scale** (kernel correct but faster revalidate changes rewire's cap-limited move count ⇒ different layout) | full-scale acceptance run | Record rewire cap-hit status + routed fraction vs v4 reference in the run log (carried over from SPEC-V5-GEOM sec 4.5); attribute layout deltas explicitly before accepting |

---

## 10. Staged de-Pythonization roadmap (beyond this workflow)

Stage 1 (THIS spec): geometry kernel behind the ExactSurface API; Python
pipeline unchanged above it; Python oracle demoted to test fixture.

- **Stage 2 — fix-pass numeric core** (next): port the port_plan §2 hotspots
  — member volume accounting, candidate scoring for rewire/cco (batch
  "evaluate these candidate descriptors, return per-index scores"), lean/
  cone/Weiszfeld leaves — as additional geom_kernel functions. Serial-
  generate / parallel-evaluate / serial-commit exactly per port_plan §5-C1:
  Python keeps the greedy control flow and the commit order; Rust evaluates
  candidate batches deterministically. Precondition (Codex review finding,
  endorsed): replace `rewire`'s wall-clock cap with a deterministic work
  budget FIRST — a faster evaluator under a time cap changes results.
- **Stage 3 — perception**: island_births slicing (the 45s stage) and
  overhang/minima analysis in Rust (parallel over z-chunks with fixed-order
  gather); exterior-face mask consumers already run on the kernel.
- **Stage 4 — the forest itself**: Member/Forest state, revalidate loop, and
  transactional checkpoint/rollback move to Rust; Python retains config,
  orchestration, emit, and reporting. At this point the engine is
  Rust-with-a-Python-face.
- **Stage 5 — full binary**: chitubox emit + mesh IO in Rust; `engine.run`
  becomes a thin CLI/pyfunction. Python remains ONLY as: the frozen
  differential oracles (geometry + any stage ported with a parity suite),
  the independent penetration census (permanently Python, permanently
  kernel-independent), and the pytest harness.

Each stage lands behind its own parity suite against the Python
implementation it replaces, with the same determinism rules (sec 4) and the
same ruling: correctness and resin economy never trade against wall-time.

---

## Appendix A — Adjudication record (2026-07-23 adversarial review)

Two critiques were commissioned. The Codex critique was NOT received
(relay failed: "Not inside a trusted directory and --skip-git-repo-check
was not specified" — a tooling error, not a clean review; there is no
cross-family sign-off on this spec revision and a Codex pass should be
re-run when the relay is fixed). The Opus critique delivered 10 findings;
all 10 were verified line-by-line against the frozen oracle and ACCEPTED,
producing Amendments A1–A10:

| # | Finding | Amendment | Sections touched |
|---|---|---|---|
| A1 | Tier-1 open-space fallback returns centroid-endpoint distance + NaN point; value parity unsatisfiable there | Kernel deviation sanctioned; contract split by oracle tier; fuzz classifies draws, ≥500 per tier | 2.2, 5.3, 6.3 |
| A2 | Parity dedup is cluster-count, NOT dedup_loc keep-first; conflation flips verdicts on chained crossings | Formula pinned verbatim, distinct from dedup_loc; chained-crossing fixture added | 3.4, 6.1 |
| A3 | Oracle does NOT resolve argmin ties to lowest index; coincident opposite-winding ties can flip project normals unmeasured | False claim withdrawn; kernel-only lowest-index; up-to-sign normal fuzz + measured-tie telemetry; downstream boolean divergence stays BLOCKER | 2.3, 6.3 |
| A4 | Thread sweep inert under rayon global pool (one-shot init) | Per-count ThreadPoolBuilder pool cache + pool.install; current_threads() exposed; sweep asserts effective count | 4.2, 6.5 |
| A5 | linspace ULP divergence can flip parity samples; 1e-6 band cannot protect a discrete majority count | numpy step formula + int() truncation pinned; discrete-vote exactness rests on pinned positions + exact parity bits; band scoped to continuous quantities | 5.1, 5.4 |
| A6 | closest_on_tri bits come from reconstruct-then-resubtract (A, A+E1, A+E2); Moller uses raw E1/E2; A/C/B z-weighting | All three operand-construction sequences pinned explicitly | 5.1 |
| A7 | penetration_intervals is per-member, contradicting R1 batch-only | Exception acknowledged; one-native-call rule; penetration_intervals_batch added | 2.2, R1 |
| A8 | Pierce early-out and lowest-index argmin point cannot both hold; oracle's own tie order is set-iteration | Lowest-index claim withdrawn; first-pierce-in-fixed-traversal-order, consequence-based parity | 3.3 |
| A9 | Census shares algorithmic lineage with the oracle; "independent acceptance" overstated | Reworded: certifies kernel matches the trusted Python geometry family, not absolute physics | 8 |
| A10 | pyo3 ↔ rust-numpy version coupling is the dominant build wall; gap-diag kernel-side/wrapper-side contradiction | rust-numpy (≥0.21) drives the pyo3 pin; gap diag split: kernel casts rays, wrapper owns seeded sample + numpy percentiles | 1.2, 2.2, 4.2 |

Also adopted from the review's cross-cutting notes: CSR unknown/negative/
out-of-range cell ⇒ EMPTY candidate list, never clamped (sec 3.4); ragged
prefix-sums assembled by SERIAL scan in index order.

## Appendix B — Rejected challenges

- **CODEX critique: none received.** Nothing to adjudicate; recorded as a
  gap, not a rejection (see Appendix A preamble).
- **Finding 1, remedy option (a) — reproduce the oracle's Tier-1 branch by
  re-introducing a centroid structure in the kernel.** REJECTED. The Tier-1
  value is an artifact of the retired crop machinery (a certificate bound,
  not a physical distance); reproducing it would resurrect the centroid
  KDTree and crop bookkeeping the whole design exists to eliminate, to
  chase parity on a quantity no consumer uses beyond its sign. The
  tier-split contract (A1) preserves everything consumers observe: exact
  verdicts everywhere, exact values in the regime where the oracle is
  exact, and a provably conservative ordering (`clr_kernel ≤ clr_oracle`,
  both positive) in the regime where it is not.
- **Finding 3, remedy option "define a deterministic oracle-matching tie
  policy."** REJECTED as impossible, as the finding itself concedes: the
  oracle's tie order is cKDTree-internal (point queries) and Python
  set/hash-internal (segment candidates) — neither is specifiable or
  stable. The accepted mitigations (up-to-sign normal comparison,
  measured-tie telemetry, consequence-based point checks, BLOCKER status
  for any downstream boolean divergence) bound the exposure without
  pretending an unspecifiable order can be matched.
- **Finding 5, remedy option "perturb-stability check" for interval
  booleans.** REJECTED AS UNNECESSARY (the finding's diagnosis is accepted;
  this proposed mechanism is superseded). With sample positions pinned
  bit-identical (A6/A5 linspace pin) and per-sample parity bits exact by
  construction, the majority vote compares identical integers on both
  sides — there is no residual float freedom for a perturbation test to
  probe. A perturb-stability harness would add runtime and a new
  false-positive surface while guarding a gap the pins already close; any
  interval-structure mismatch is by construction a pin violation and
  already a BLOCKER (sec 5.4 item 3).
- **Finding 7, implied remedy "make the single-member signature itself
  batch."** PARTIALLY REJECTED: the single-member `penetration_intervals`
  signature is retained verbatim because drop-in oracle compatibility is
  the stage-1 contract (changing the Python-visible signature would fork
  the API the parity suite exists to mirror). The substance of the finding
  is accepted via A7 (one-native-call rule + `penetration_intervals_batch`
  for hot-loop callers).
