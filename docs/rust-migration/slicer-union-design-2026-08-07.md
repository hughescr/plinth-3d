<!--
Provenance: design survey conducted read-only at develop d711586 (2026-08-07);
implementation branch feature/slicer-union is based at develop b0324fd.
Line/anchor references inside the spec refer to the surveyed tree (d711586);
verify anchors against b0324fd at each phase. Spec content below is verbatim
from the handoff document.
-->

# Design spec — per-layer polygon union with fill-rule normalization (FU-10)

**Provenance: surveyed 2026-08-07 at `develop` d711586** (`/Users/craig/code/hughescr/dnd-campaigns/plinth-3d`, read-only).
Backlog identity: the task arrived as "backlog item #13"; in the authority record it is
**FU-10** in the follow-ups ledger (`docs/rust-migration/CURRENT-STATUS.md:2493`) and
**next-steps item 8** (`CURRENT-STATUS.md:2744-2748`), owner-endorsed under OD-9
(`CURRENT-STATUS.md:340-352`). This document is design only; no repository edits were made.

---

## 1. Survey — what the slicer actually is today

**F1. The 3D slicer is `geom_kernel::slicing`, and it emits segment soup, not polygons.**
`NativeSurface::slice_z_batch` (`rust/geom_kernel/src/slicing.rs:112`) takes a batch of z
planes, preflights explicit resource caps from BVH candidate counts, and rayon-parallelizes
`slice_one_z` over planes (`slicing.rs:177-189`). Per plane it emits
`PlaneIntersection { face_id, kind, segment: [V3; 2] }` (`slicing.rs:85-89`) in the frozen
Trimesh order Basic → Vertex → PositiveEdge, then ascending face id (`slicing.rs:91-93`).
Segment endpoint order is an artifact of which triangle vertex is the odd one out
(`slicing.rs:288-346`) — **no in-plane winding convention is carried**. Coplanar faces and
negative-side owners of an on-plane edge emit nothing; the positive-side owner emits a
`PositiveEdge`, and duplicate positive-side owners each emit one (pinned:
`rust/geom_kernel/tests/native_api.rs:287`). All geometry is `f64` end to end
(`crates/plinth-mesh/src/lib.rs:43` — `verts: Vec<[f64; 3]>`; `geom_kernel::V3 = [f64; 3]`,
`rust/geom_kernel/src/vec3.rs:7`). The tolerance is the frozen Trimesh merge tolerance
`PLANE_SLICE_MERGE_TOLERANCE_MM = 1.0e-8` (`slicing.rs:18`).

**F2. Loop stitching lives in `plinth-support::perception::stitch_layer`, and it is
even-odd, undirected, and vertex-probe based.**
`stitch_layer` (`crates/plinth-support/src/perception.rs:1247-1380`):
- Snaps endpoints to an integer key at `LOOP_MERGE_MM = 1e-8 mm` (`perception.rs:27`,
  `point_key` at `perception.rs:1218-1223`), builds an **undirected** edge set
  (`perception.rs:1263` — direction from the kernel is discarded), and demands every vertex
  have degree exactly 2 else `OpenSliceSeam` (`perception.rs:1270-1272`).
- Walks closed loops, drops loops under `BIRTH_MIN_AREA_MM2 = 1e-3` (`perception.rs:28`,
  filter at `perception.rs:1311`).
- Classifies hole-vs-solid by **even-odd nesting parity of the loop's FIRST VERTEX** against
  every other loop (`perception.rs:1322-1328`), with an in-code admission that this is
  deliberate debt: *"Normalizing parity itself waits on the fill-rule work"*
  (`perception.rs:1314-1321`), and again at `perception.rs:1464-1475` (the better interior
  probe is "gated behind the fill-rule normalization work").
- Assigns each hole to the smallest containing solid via a widening fallback chain —
  vertex probe → interior probe → any-vertex → **bounding-box containment**
  (`hole_owner`, `perception.rs:1394-1418`) — and **silently drops a hole nothing claims**
  (`perception.rs:1357-1364`). This drop + bbox chain is exactly the "Tier-1 unowned-hole
  fallback" FU-10 is chartered to supersede (`CURRENT-STATUS.md:2744`), with FU-16 the
  closure measurement ("bbox claiming can under-report a solid's area; the Tier 2 union
  supersedes it", `CURRENT-STATUS.md` FU-16 row).
- Output is `Vec<SliceSolid>` — `SliceSolid { outer: SliceLoop, holes: Vec<SliceLoop> }`,
  `SliceLoop { points_xy_mm: Vec<[f64;2]>, signed_area_mm2, hole }`
  (`perception.rs:606-652`); outers are canonicalized to positive signed area, holes
  negative (`perception.rs:1329-1335`); `SliceSolid::area_mm2 = |outer| − Σ|holes|`
  (`perception.rs:644-651`).

**F3. Two stitch call sites, both in perception.**
(a) Print layers: `prepare_generic_contacts` slices every layer midplane in one
`slice_z_batch` call then loops `stitch_layer(SliceStitchSite::PrintLayer(..))`
(`perception.rs:987-1024`); the results feed island-birth detection (`find_births`,
`perception.rs:1620+`), local-minima filtering, and the plan digest.
(b) Adequacy probes: minima adequacy re-slices at probe heights and calls
`stitch_layer(SliceStitchSite::AdequacyProbe(..))`, then reads `carried_outer_area`
(`perception.rs:2019-2049`). The site enum exists precisely so seam errors name the right
space (`perception.rs:1225-1245`).

**F4. There is NO in-repo rasterization, and `chitubox_io` never sees layer polygons.**
`chitubox_io` reads/writes `.chitubox` **project** files — STL model records, support
records, placement (`rust/chitubox_io/src/lib.rs:1-17`); ChituBox itself is the slicer of
record for print ("ChituBox itself is the only semantic oracle", `lib.rs:15-17`).
`plinth-emit` is "deliberately only a conversion boundary" f64 → f32 wire records
(`crates/plinth-emit/src/lib.rs:1-6`). The python-feature-gated surface
(`rust/chitubox_io/Cargo.toml:35-40`, `src/py.rs`) is the PyO3 extension for the legacy
Python driver — it carries no layer-geometry API either. **Consequence:** the mis-fill risk
today is not an in-repo rasterizer; it is (i) perception reasoning over mis-classified
layer polygons (births, adequacy, and OD-9 candidate nomination), and (ii) the future
native slice-to-print path (`docs/rust-migration/04-migration-plan.md:741` — `slice.rs`
"Exact z-slicing to closed 2D loops with winding... real algorithm work";
`02-target-architecture.md:97`). The normalizer designed here is the shared foundation for
both.

**F5. Downstream consumers currently assume the LEGACY semantics, not normalized input.**
`find_births`'s overlap logic is pinned to Python behavior including negative-winding-hole
quirks (`frozen_birth_overlap_ignores_retained_negative_winding_holes_like_python`,
perception tests, ~`perception.rs:4440`); adequacy is pinned by
`hollow_adequacy_uses_positive_outer_area_even_when_point_is_in_hole` (~`perception.rs:4506`);
the stitcher itself by `frozen_segment_vector_deduplicates_shared_owner_and_stitches_one_loop`
(~`perception.rs:4733`), `self_crossing_outer_contour_still_yields_solids_not_a_seam_error`
(~`perception.rs:4767`), `a_hole_no_solid_can_reach_is_discarded_rather_than_raised`, and
`an_adequacy_probe_seam_no_longer_masquerades_as_a_print_layer`. Nothing downstream
consumes normalized layers yet; OD-9 explicitly requires that candidate nomination start to
("Candidate nomination must consume normalized layers per OD-9", `CURRENT-STATUS.md:2745`).

**F6. The slicer/stitcher is UPSTREAM of the seven frozen OD-8 census digests — changes CAN
move all seven.** The chain is explicit in code:
`GenericContactPlan::digest()` hashes every slice-layer loop vertex, birth footprint, and
minimum (`perception.rs:742-796`, slice layers at 782-796) →
`PreparedSupportDigests.generic = generic.digest()` (`crates/plinth-support/src/runner.rs:3093`,
staleness-checked at `runner.rs:2724, 2800`) → the sealed verification snapshot digest → the
census prints `snapshot.digest()` per model
(`crates/plinth-support/examples/embed_incidence_census.rs:150-157`). Additionally the
fixture goldens in `crates/plinth-support/tests/support_runner_integration.rs:103-146` pin
(routed, live, digest) tuples that run through `stitch_layer`, and FU-15 records two sealed
corpus digests (Cylinder `13310185350867878190`, `make_box` `9657051231007502658`) that are
currently test-uncovered. **This is NOT a "slicer is downstream of the census" situation —
it is upstream, and the design below is shaped by that fact.** Under OD-8 the delta must be
measured and attributed per model before any re-pin (`CURRENT-STATUS.md:620` — "a golden is
not re-pinned until every missing member is attributed"); OD-8 lifts behavior preservation
but forbids silent geometry change (`CURRENT-STATUS.md:306-330`).

**F7. Relevant workspace materials already present:** `rayon` and `robust` (exact geometric
predicates) are workspace dependencies (`Cargo.toml:14,17`); no polygon-clipping crate
(`geo`, `i_overlay`, `cavalier_contours`, `clipper`) appears anywhere in `Cargo.lock`. The
workspace pins exact versions for risk-bearing deps (`cxx = "=1.0.198"`, `kiddo = "=5.3.3"`).
`plinth-solid` already carries C++ FFI (manifold via cxx) but the standing preference is
pure Rust.

---

## 2. Diagnosis — the defect surface, case by case

All cases below are constructed from the code as it stands (F2), not speculation. The mesh
class of record is the Weeping Treant: watertight, manifold, **self-interpenetrating**
(23,220 non-adjacent intersecting triangle pairs in one region — OD-9 context,
`CURRENT-STATUS.md:344-349`).

**D1. Overlapping solids / interpenetrating sub-objects → phantom holes.** Two closed CCW
loops A and B from two overlapping sub-solids: any probe point in A∩B has even-odd depth 1
relative to "the other loop", so whichever loop's first vertex lands inside the other is
classified a **hole** (`perception.rs:1322-1328`). Under nonzero winding A∩B has winding 2 =
solid. Consequence today: solid area under-reported, spurious "hole" boundaries, and — when
containment goes intransitive — holes that no solid claims, silently dropped
(`perception.rs:1357-1364`). This is the exact treant slice pathology the in-code comments
describe (`perception.rs:1384-1393`).

**D2. Self-intersecting single contour (mesh defect or tangency pinch) → undefined inside.**
A figure-eight or doubly-covered lobe: `signed_area` (shoelace, `perception.rs:1556-1567`)
partially cancels, so the `BIRTH_MIN_AREA_MM2` filter can drop a large-but-self-cancelling
loop; `point_in_polygon` (`perception.rs:1569-1581`) gives parity answers that depend on
which lobe the first vertex sits in. The pinned test
`self_crossing_outer_contour_still_yields_solids_not_a_seam_error` documents that today the
code merely refuses to crash here — it does not claim the classification is right.

**D3. Nested holes (solid-in-hole-in-solid) classified by a boundary-degenerate probe.**
Nesting depth is computed by probing a loop's **own first vertex**, which lies exactly ON
its loop and — in a tangle — within `1e-8 mm` of a neighbor's boundary, where ray-parity
has no defined answer (`perception.rs:1464-1470`). A wrong parity at one loop flips
hole/solid for its whole subtree. The good interior probe exists (`interior_probe`,
`perception.rs:1476+`) but is deliberately confined to hole rescue because promoting it
"moves frozen digests" (`perception.rs:1314-1321`) — i.e., the current classification is
frozen-wrong, waiting on this work item.

**D4. Coincident edges from coplanar faces / duplicate positive-edge owners.** The kernel
deliberately emits one segment per positive-side owner, so a shared on-plane edge with two
positive owners yields two identical segments (`native_api.rs:287`). The undirected
`BTreeSet` dedupes exact duplicates (`perception.rs:1263`), but *near*-coincident collinear
segments (within tolerance but different keys) produce degree-4 vertices → spurious
`OpenSliceSeam` aborts, or thin slivers that survive as micro-loops. There is no snap
rounding beyond the 1e-8 key.

**D5. Area and adequacy arithmetic inherit every mis-classification.**
`SliceSolid::area_mm2 = |outer| − Σ|holes|` (`perception.rs:644-651`) goes wrong wherever
D1–D3 mislabel; `carried_outer_area` at adequacy probes and birth-overlap detection consume
those areas directly (F3). Dropped unowned holes over-report solid area (bounded, local —
per the code's own comment at `perception.rs:1359-1364`); bbox hole-claiming under-reports
(FU-16). Since a mis-birth mints or suppresses a support contact, the defect surface reaches
printed geometry through support placement even though no in-repo rasterizer exists.

---

## 3. Design

### 3.1 Algorithm and crate choice

**Decision: use `i_overlay` (iShape-Rust), integer API, as the boolean engine; do not
hand-roll Bentley–Ottmann/Vatti.** Justification:

- **Fit.** `i_overlay` is a pure-Rust boolean-overlay library (union/intersect/difference/
  xor) supporting **both even-odd and non-zero fill rules**, built on integer coordinates
  with snap-rounded intersections — exactly the "union + fill-rule normalization" primitive
  this item needs, including resolution of self-intersecting and multiply-wound input.
- **Maintenance/license.** MIT-licensed, actively maintained; it is the engine the `geo`
  crate adopted for its `BooleanOps` — a strong external maintenance signal. Depending on
  `i_overlay` directly avoids `geo`'s large transitive tree for one operation.
- **Determinism.** Integer arithmetic with explicit quantization is bit-reproducible across
  platforms and thread counts — a hard requirement given the digest regime (F6). Float-input
  overlay APIs that pick an adaptive internal scale from the data's bounding box are
  **forbidden here**: the scale would change when the model moves, silently moving digests.
  We quantize ourselves (3.2) and call the integer API only.
- **Alternatives rejected.** `geo`: wraps the same engine, heavy tree. `cavalier_contours`:
  arc-polyline CAD focus, wrong shape for this input. `clipper2` FFI: C++, contradicts the
  pure-Rust direction (`plinth-solid`'s cxx is a grandfathered exception, not a precedent).
  Hand-rolled sweep with `robust::orient2d`: the predicates are the easy half — snap
  rounding, coincident-edge bundles, and winding accumulation are months of edge cases that
  a maintained crate has already burned down; not justified. Pin an exact version
  (`i_overlay = "=x.y.z"`) per workspace convention (F7).
- **Verification duty (open question OQ-1):** this survey ran sandboxed without network;
  the current version, exact integer-API module paths, and integer width (i32 vs i64 points)
  must be confirmed against crates.io/docs.rs at implementation start. The quantum in 3.2 is
  sized to fit i32 with ≥8× headroom, so either width works.

### 3.2 Numeric robustness — quantization contract

The pipeline is f64 end to end (F1); the printer's physical resolution is 14×19 µm pixels,
0.05 mm layers. Robustness strategy: **one explicit quantization at the normalizer boundary,
exact integer arithmetic inside, one dequantization out.**

- `SLICE_UNION_QUANTUM_MM = 1e-6` (1 nm), a new registered constant with a registry entry
  and ledger row (every-record-site discipline). Rationale: it is 100× coarser than the
  stitch key (`LOOP_MERGE_MM = 1e-8`, so distinct stitched vertices collapse only within
  1 nm — 4 orders below the finest physical feature), and on a ±250 mm domain yields
  coordinates ≤ 2.5e8, i.e. ~12% of i32 range — safe for the engine's internal products.
- Quantize: `round(x / QUANTUM)` per axis (ties-to-even is fine; it is deterministic).
  Dequantize: `k as f64 * QUANTUM`. Both are pure, batch-applied, and platform-stable.
- No exact predicates are needed in our code — all incidence decisions happen inside the
  integer engine. `robust` stays available for the orientation-derivation cross-check in
  3.3 (test-side only).
- Degenerate output (loops whose quantized area is 0, slivers below
  `BIRTH_MIN_AREA_MM2 = 1e-3 mm²`) is filtered AFTER the union, on union output — the union
  sees everything, so a large self-cancelling loop (D2) is resolved rather than dropped.

### 3.3 Fill rule: normalize FROM nonzero, TO the canonical form where both rules agree

- **Input rule: NON-ZERO, with orientation derived from mesh winding.** The mesh's faces
  are consistently outward-wound (winding is audited elsewhere in the pipeline:
  `02-target-architecture.md:911`). For a face with outward normal **n** cut by plane z, the
  in-plane segment direction is chosen so the solid interior lies to the LEFT:
  `d = ẑ × n` projected to XY (equivalently: orient the stitched loop so its signed area is
  positive iff the enclosing surface is locally outward). This turns each closed stitched
  loop into a **wound contour**: sub-solids each contribute +1 winding; overlap regions get
  winding ≥ 2 = solid under nonzero — which is the physically correct fill for a union of
  solids and precisely fixes D1. Crucially this is computed in the **consumer** from
  `PlaneIntersection.face_id` + `SurfaceGeometry::normals()`
  (`rust/geom_kernel/src/mesh.rs:28`) — `geom_kernel::slicing` and its frozen Trimesh
  vectors are **not touched** (the frozen tests at `native_api.rs:86-410` stay green by
  construction).
- **Output form: canonical normalized polygons** — pairwise-disjoint outer boundaries, CCW
  outers (positive signed area), CW holes (negative), each hole attached to its unique
  owner, no self-intersections, no coincident edges. On such output **even-odd and nonzero
  agree** — that agreement is the definition of "normalized", and it means every downstream
  consumer (`point_in_polygon`'s even-odd, `area_mm2`'s |outer|−Σ|holes|, and any future
  rasterizer regardless of its fill rule) is correct without modification.
- **Degraded path.** If directed stitching finds locally inconsistent winding (a mesh-defect
  loop that cannot be oriented), fall back to even-odd union over the unoriented loops for
  that layer and record it in the result as a typed, per-layer diagnostic (not a silent
  fallback, not an abort). The degree-2 `OpenSliceSeam` check stays as-is for genuinely open
  seams.

### 3.4 Placement in the pipeline, batch shape, resource limits

- **Where:** a new small crate `plinth-layers` (workspace member beside `plinth-mesh`),
  exporting `normalize_layers_batch`. The migration plan already assigns "closed 2D loops
  with winding" to the mesh layer (`04-migration-plan.md:741`); a dedicated crate keeps
  `plinth-support` from owning geometry that the future emit-side slicer (F4) must share,
  and keeps the `i_overlay` dependency out of crates that don't need it.
  `perception::stitch_layer` becomes a thin wrapper: stitch (unchanged walk) → orient →
  normalize → `Vec<SliceSolid>`.
- **Batch-first (standing rule: per-item calls in hot paths are defects):** the API takes
  the WHOLE `PlaneSliceBatch` plus the face-normal table and returns all layers:
  `fn normalize_layers_batch(batch: &PlaneSliceBatch, normals: &[[f64;3]], limits: &LayerUnionLimits) -> Result<NormalizedLayers, LayerUnionError>`.
  Internally it mirrors `slice_z_batch`'s own shape (`slicing.rs:177-189`): one rayon
  `par_iter` over layers (layers are independent), deterministic assembly by layer index,
  thread-count telemetry fields carried the same way (`effective_threads` /
  `participating_threads`) so the thread-sweep verification idiom applies. One union call
  per layer over ALL of that layer's loops at once (the engine is multi-polygon native) —
  no per-loop or per-pair calls.
- **Resource discipline (house style, cf. `PlaneSliceLimits`, `slicing.rs:28-33`):**
  `LayerUnionLimits { max_input_vertices, max_output_vertices, max_union_operations, max_scratch_bytes }`,
  preflighted from stitch counts before allocation (vertex counts are known exactly;
  operations bound is O((n+k)·log n) with k ≤ n² capped explicitly), typed errors in the
  existing enum style. The existing `max_loop_vertices` check (`perception.rs:1002-1022`)
  moves naturally onto the normalized output.
- **Consumers in this work item:** both stitch sites (F3) — print layers and adequacy
  probes — consume normalized output; the Tier-1 unowned-hole drop and the bbox fallback
  chain (`perception.rs:1357-1364, 1394-1418`) are **deleted** (after normalization a hole's
  owner is unique and guaranteed; an unowned hole is a normalizer bug and becomes a typed
  error). OD-9 candidate-nomination wiring is a *successor* consumer, not in scope here,
  but the output type must carry what it needs: per-solid loops + per-layer z, sufficient
  for an exterior-ness answer in the slice plane.

### 3.5 Digest strategy — pass-through-if-already-normal

Because the normalizer is upstream of every frozen digest (F6), the design makes the clean
corpus **byte-identical by construction**: after computing the union, compare it against the
input loop set under canonical form (same loop count, same hole assignment, every vertex
within one quantum). If equal, **emit the original f64 loops verbatim** — not the
requantized ones. Then:
- Layers that were already normal (all seven reachable census models, expected) produce
  bit-identical `SliceSolid`s → `GenericContactPlan::digest()` unchanged → all seven census
  digests and both runner-integration goldens stay frozen. The re-run proving this is the
  phase gate, not an assumption.
- Layers that change (the treant's tangles; any model exercising D1–D4) are exactly the
  measurable, attributable delta OD-8 requires. FU-16's bbox-claimed vs. discarded
  unowned-hole split on the treant is the closure measurement (`CURRENT-STATUS.md` FU-16).
- The equality check is also the idempotence property test running in production for free.

---

## 4. Verification strategy

**V1. Golden fixtures** (new, in `plinth-layers`; deterministic hand-built segment sets):
- nested holes 3 deep (solid–hole–solid–hole), probe-adversarial: first vertices placed on
  the boundary-degenerate rays that break today's parity (D3);
- self-touching / figure-eight contour (D2), asserting nonzero resolution and that the
  degraded even-odd path fires only when winding is genuinely inconsistent;
- coincident and near-coincident collinear edges from duplicate positive-edge owners (D4),
  including the exact duplicate the kernel pins at `native_api.rs:287`;
- two overlapping CCW solids (D1): union is one solid, area < sum, no hole — the treant
  case in miniature;
- sub-quantum sliver in a valid layer: filtered after union, never aborts.

**V2. Property tests** (proptest over random loop soups, seeded, both feature sets):
- **idempotence:** `normalize(normalize(L)) == normalize(L)` bit-for-bit (the 3.5 pass-through
  makes the second application the identity by construction — the test proves it);
- **area monotonicity:** `max_i |loop_i| ≤ area(union) ≤ Σ_i |loop_i|`, and union area is
  monotone nondecreasing when a loop is added;
- **fill-rule agreement:** on normalized output, even-odd and nonzero point classification
  agree on a dense random probe set (this IS the acceptance criterion of the whole item);
- **containment invariants:** holes strictly inside owners; outers pairwise disjoint;
- **orientation cross-check:** loop orientation from 3.3's `ẑ × n` rule matches the sign of
  the shoelace area on convex analytic solids (sphere/cylinder/cube fixtures already in
  `plinth-mesh::fixtures`), with `robust::orient2d` as the test-side referee.

**V3. Pinned-legacy migration.** The stitcher tests enumerated in F5 are the behavior
contract of the OLD semantics. Each is dispositioned explicitly in the cutover commit:
`self_crossing_outer_contour_still_yields_solids...` and
`a_hole_no_solid_can_reach_is_discarded...` become assertions about *normalized* output
(solids yes, discard path deleted); the frozen-vector and probe-seam tests stay unchanged;
the two `..._like_python` birth/adequacy tests are re-derived and any expectation change is
listed in the phase report line by line. No test is deleted without a named successor.

**V4. Census / frozen-digest gate (F6 answered).** The slicer is **upstream** of the seven
frozen OD-8 census digests; they CAN move. Gate per phase: re-run
`embed_incidence_census` (release, census feature) at the phase tip; expected result under
3.5 is **all seven byte-identical**, plus the two `support_runner_integration` goldens
(`make_toed_slab` 64/179/4826202191472851835, `make_raking_blade` 32/89/1709218010932846012,
`CURRENT-STATUS.md` FU-19 row) green. ANY digest movement is stop-and-report with per-model,
per-layer attribution before re-pin (OD-8 discipline, `CURRENT-STATUS.md:620`). The treant
is currently refused upstream at the specialized merge gate (FU-8e successor line), so it
does not constrain digests — its normalized-layer delta is reported as measurement (FU-16),
not as a digest event.

**V5. Five gates** on every landing commit: workspace tests default features, tests with
`--features python` where members carry it (and `census` where relevant), `cargo clippy
--all-targets -D warnings`, `cargo fmt --all -- --check`, `git diff --check`. Any landed
number goes through every-record-site discipline in `docs/rust-migration/CURRENT-STATUS.md`
+ registry + `constants.md` for the new quantum constant.

---

## 5. Decomposition — phases and points

Anchor: 1 point = rename a single variable across 3 files in TS, no tests to update.
Ladder: 1/2/3/5/8/13/20/40.

| Phase | Deliverable | Verification gate | p50 | p90 |
|---|---|---|---:|---:|
| **A** | Oriented loops: `ẑ × n` orientation derivation from `face_id` + normals table, consumer-side only (geom_kernel untouched); directed stitch walk carrying winding; degraded-path detection for inconsistent winding | New orientation unit tests on analytic fixtures; all existing frozen kernel + stitcher tests green untouched; five gates | 5 | 8 |
| **B** | `plinth-layers` crate: quantization constant (registered), `i_overlay` integration (pinned exact version, integer API), one-union-per-layer batched + rayon over layers, `LayerUnionLimits` preflight + typed errors, canonical output form, pass-through-if-already-normal | V1 goldens + V2 property tests in-crate; crate-local five gates; OQ-1 (crate version/width) resolved and recorded | 8 | 13 |
| **C** | Perception cutover: both stitch sites consume normalized layers; delete unowned-hole drop + bbox fallback chain (`perception.rs:1357-1418`); `max_loop_vertices` moved to normalized output; V3 test dispositions | Full workspace tests; V3 disposition table in the commit message; five gates | 5 | 8 |
| **D** | Census + goldens gate and treant measurement: census re-run (expect 7/7 byte-identical), runner-integration goldens green, FU-16 measurement on the treant's reachable layers, stop-and-report on any movement | V4 executed and transcribed; attribution table if nonzero; five gates | 5 | 13 |
| **E** | Record sweep: CURRENT-STATUS.md entry (FU-10 closed, FU-16 measured), quantum constant registry + ledger + regenerated `constants.md`, FU-14 note check (digest-moving change authorization), docs cross-references | Every-record-site checklist; grep-clean for stale claims; five gates | 2 | 3 |
| **Total** | | | **25** | **45** |

Order is A → B → (C, D serialized) → E; A and B could in principle overlap after B's API is
sketched, but B consumes A's orientation type, so treat as sequential. D's p90 is wide
deliberately: it absorbs the OD-8 attribution work if pass-through equality fails anywhere.

---

## 6. Open questions (with recommendations)

**OQ-1. `i_overlay` current version, module paths, and integer point width.** Unverifiable
from this sandbox (no network). *Recommendation:* resolve at Phase B start; pin exact
version; if the integer API is i32 the 1e-6 mm quantum fits with 8× headroom, if i64 the
quantum could drop to 1e-8 mm to coincide with `LOOP_MERGE_MM` — take i64/1e-8 if available
since it makes quantization lossless w.r.t. the stitch keys and strengthens pass-through.
If the crate has materially regressed in maintenance, fall back to `geo::BooleanOps`
(same engine, heavier tree) rather than hand-rolling.

**OQ-2. Quantum registration.** Is `SLICE_UNION_QUANTUM_MM` a placeholder-class constant
(diagnostic) or a pinned physics constant? *Recommendation:* pinned process constant with a
registry note like `PLANE_SLICE_MERGE_TOLERANCE_MM`'s — it is a determinism contract, not a
tunable.

**OQ-3. Should the adequacy-probe site also normalize?* It re-slices off-schedule heights
(`perception.rs:2019-2049`). *Recommendation:* yes — one code path, and adequacy areas are
exactly where bbox/drop distortions bite (D5). The site enum already keeps error spaces
separate.

**OQ-4. Do births/overlap (`find_births`, `solids_overlap`) get rewritten to exploit
disjointness?* *Recommendation:* no — out of scope; they consume normalized `SliceSolid`s
unchanged. File the simplification as a follow-up only if D's measurements show cost there.

**OQ-5. Digest-moving authorization.** FU-14 notes `coalesced_seat_count` waits for "the
next authorized digest-moving change"; if Phase D finds any legitimate normalization delta
on the clean corpus, does the owner want FU-14 folded into the same re-pin?
*Recommendation:* yes if and only if a re-pin happens anyway; a zero-delta Phase D leaves
FU-14 untouched.

**OQ-6. OD-9 nomination wiring.** FU-10's charter says candidate nomination must consume
normalized layers, but the exterior-ness screen today is 3D (`surface.inside`,
`designed_embed.rs`). *Recommendation:* keep this item's boundary at "normalized layers
exist and perception consumes them"; open the nomination-consumption change as its own
successor item with its own census gate — it changes contact placement, a different risk
class than layer geometry.

---

## 7. Phase-B addendum — OQ-1 resolution and recorded deviations (appended 2026-08-07)

*Append-only implementation record; sections 1–6 above are the verbatim design and are
unchanged. This section is the in-repo record required by the phase-B charter ("OQ-1
resolved and recorded", phase table row B) and by the standing rule that deviations are
recorded in-repo, never silent.*

### 7.1 OQ-1 resolved: `i_overlay` is suitable — pinned `=8.0.0`, i64 integer API

Verified 2026-08-07 against crates.io (unsandboxed `cargo` fetch; all gates thereafter ran
`--offline` against the updated lockfile) and against the vendored crate sources as primary
evidence:

- **Version:** `8.0.0` is the newest release in the freshly fetched crates.io sparse index
  (release train observed in the index: 4.5.x → 5.0.x → 6.0.x → 7.0.0–7.0.3 → 8.0.0).
  Pinned exactly per workspace convention: `i_overlay = "=8.0.0"` (checksum
  `391901c4...c3607ca` in `Cargo.lock`; transitive tree `i_float 4.0.0`, `i_shape 4.0.0`,
  `i_tree 0.19.0`, `i_key_sort 0.11.0` — pure Rust throughout, `libm` the only leaf).
- **License:** `MIT OR Apache-2.0` (crate manifest) — broader than the MIT the design
  assumed; no conflict.
- **Maintenance:** active. The 8.0.0 package targets edition 2024 with
  `rust-version = 1.88` and carries a current upstream VCS sha; the steady 2025–2026
  release cadence above is the maintenance signal. (The crate's bundled CHANGELOG.md stops
  at 1.10.0/2025-02-02 — stale relative to the release train; version evidence is the
  index, not the changelog.) The workspace toolchain (rustc 1.97.1) satisfies the 1.88
  floor. The `geo` fallback (OQ-1's contingency) was not needed.
- **Integer width:** the integer API is generic — `IntPoint<T: IntNumber>` with
  `IntNumber` implemented for i16/i32/**i64** (`i_float 4.0.0`,
  `src/int/number/int.rs`); for i64 the engine's wide arithmetic is
  `type Wide = i128` / `type WideUInt = u128` (`int.rs:225-227`). **The i64/1e-8 branch of
  OQ-1's recommendation is therefore taken:** `SLICE_UNION_QUANTUM_MM = 1e-8 mm`,
  coinciding with the perception stitch key `LOOP_MERGE_MM = 1e-8`, making quantization
  lossless with respect to stitched vertices and the sec 3.5 pass-through comparison an
  exact integer equality.

### 7.2 New registered constants (sec 3.2 contract, updated for the i64 branch)

- `SLICE_UNION_QUANTUM_MM = 1e-8 mm` — supersedes the spec text's i32-sized default of
  1e-6 mm (sec 3.2 was explicitly provisional on OQ-1).
- `SLICE_UNION_MAX_ABS_MM = 1e4 mm` — new with the i64 branch: the coordinate-domain
  half-width bounding quantized magnitudes to ≤1e12 (~2^40), keeping per-term i128 wide
  products (~2^80) far from overflow. A refusal boundary (typed error, no clamping), not
  printable reach; ~40× headroom over the ~250 mm build domain. The spec's i32 headroom
  arithmetic ("≤2.5e8, ~12% of i32 range") is superseded alongside the quantum.

Both are M0-registered placeholders (`runtime_determinism` ledger classification, honest
Rust-era provenance, no legacy Python source) with ledger rows naming this section as
normative; recorded values are pinned by `plinth-core`'s constants goldens and
cross-checked against the crate's own constants by `plinth-layers/tests/registry_pin.rs`.

### 7.3 Recorded phase-B deviations from the sec 3.4 sketch

1. **API shape.** `plinth_layers::normalize_layers_batch(&[LayerLoops], min_area_mm2,
   &LayerUnionLimits)` takes STITCHED loops per layer plus that layer's fill rule
   (`LayerFillRule::NonZero` for oriented layers, `EvenOdd` for the sec 3.3 degraded
   path), not the spec sketch's `(&PlaneSliceBatch, &normals, &limits)`. Rationale:
   segment stitching and the ẑ×n orientation derivation landed in
   `perception::orientation` (phase A) where the seam-error spaces and frozen stitcher
   semantics live; passing raw segments would duplicate that walk here. The phase-C
   wrapper composes stitch → orient → normalize exactly as sec 3.4 prescribes, with
   `degraded: Some(_)` mapping to `EvenOdd`. `plinth-layers` stays free of
   `plinth-support`, so the future emit-side slicer (F4) can consume it directly.
2. **`min_area_mm2` is a parameter,** not a crate constant: `BIRTH_MIN_AREA_MM2 = 1e-3`
   stays owned by its perception call sites; the crate applies whatever threshold the
   caller passes, after the union, per sec 3.2.
3. **Property tests use the workspace's pinned seeded RNG** (`rand_chacha =0.10.0`,
   48 seeded cases per property) instead of adding a `proptest` dependency; the V2
   properties (idempotence bit-for-bit, area bounds/monotonicity, fill-rule agreement,
   containment invariants) are otherwise as specified. The V2 orientation cross-check
   (ẑ×n vs shoelace sign, `robust` referee) landed with phase A's analytic-fixture tests
   and is not duplicated here.
4. **`max_union_operations` model:** per layer `(n + k)·ceil(log2 n)` with `k` charged at
   its explicit `n²` worst case, summed over the batch — sec 3.4's bound made concrete;
   `max_output_vertices` is necessarily enforced on union output (output size is not
   knowable in advance), all other caps preflight before any engine allocation.
5. **Engine options:** the union runs with `keep_all_points` so the engine does not
   simplify collinear vertices; pass-through equality (sec 3.5) is then a pure
   representation check, and a layer already in canonical form emits its ORIGINAL f64
   loops verbatim, reversed/rotated only (no arithmetic on the values).

Phase-B scope note: this phase lands the crate and its constants only. No perception call
site is rewired (phase C), no census digest can move (nothing upstream of
`GenericContactPlan::digest()` changed), and the FU-16 treant measurement remains phase D.

---

## 8. Phase-C addendum — the perception cutover, recorded (appended 2026-08-07)

*Append-only implementation record; sections 1–6 are the verbatim design and are unchanged,
and section 7 is phase B's record. This section is the in-repo record required by the
standing rule that deviations are recorded, never silent, and it carries phase row C's
"V3 disposition table in the commit message" as the durable copy.*

### 8.1 What landed

Both stitch sites named in F3 now consume normalized layers, through one function:
`perception::normalized_slice_layers(space, batch, normals, limits)`. It is the sec 3.4
composition exactly — stitch → orient → normalize — and it is batch-first end to end: one
`slice_z_batch` upstream, one `orient_layers_batch`, one `normalize_layers_batch`, and no
per-layer entry point left in either hot path. The print-layer site's hand-rolled
per-layer loop and the adequacy site's per-probe-row loop are both gone.

**OQ-3 is answered as the design recommended: yes.** The adequacy probes take the same
normalized path as the print layers. One code path, and the site enum still keeps the two
seam-error spaces apart (`OpenSliceSeam` versus `OpenAdequacyProbeSeam`), now via a new
`SliceStitchSpace` that names the index space of a whole batch so the batched call can mint
the right per-index site.

**The Tier-1 fallback the design condemns is deleted, not bypassed.** Gone from the tree:
`stitch_layer` (the even-odd, first-vertex-parity stitcher), `hole_owner` (the widening
chain: vertex probe → interior probe → any-vertex → **bounding-box containment**), the
**silent drop** of any hole nothing claimed, and their supporting `smallest_solid`,
`bounding_box`, `box_contains`, `interior_probe`, `area_centroid`, `INTERIOR_PROBE_EDGES`,
`INTERIOR_PROBE_OFFSETS`. On normalized output a hole's owner is unique by construction, so
there is no query to widen and nothing to drop. FU-16's closure measurement ("bbox claiming
can under-report a solid's area; the Tier 2 union supersedes it") loses its subject.

**The deferred-parity comments get their dated resolution.** The two in-code admissions —
"Normalizing parity itself waits on the fill-rule work" at the parity count, and "gated
behind the fill-rule normalization work" at `interior_probe` — were attached to code this
phase deletes. Their resolution note is therefore carried on the successor,
`normalized_slice_layers`, under the heading *"2026-08-07: the deferred parity work is
done"*, which quotes both deferrals and states what replaced them.

### 8.2 Recorded phase-C deviations

1. **`LayerUnionLimits` is derived from the existing authority caps, not added to
   `GenericPerceptionLimits`.** Sec 3.4 specifies the limits struct but not where its
   numbers come from. Adding four cap fields to `GenericPerceptionLimits` was the obvious
   reading and is refused for two reasons: the values would be new unregistered constants,
   and any field entering `authority_fingerprint` moves every frozen OD-8 digest — for a
   guard that by construction cannot change a successful plan. Instead
   `plinth_layers::LayerUnionLimits::from_input_bound(max_input_vertices,
   max_output_vertices)` (new in this phase, additive to the phase-B crate) computes the
   operation and scratch caps as the exact worst case those two bounds already admit, with
   saturating arithmetic so an unrepresentable bound reports `usize::MAX` rather than
   wrapping. Perception passes `max_slice_intersections * 2` (a stitched layer holds at most
   two endpoints per admitted intersection, and the kernel already refused past that cap)
   and `max_loop_vertices`. **Honest consequence:** three of the four caps are inert for
   this caller by construction; the binding one is the output cap, which is the point of the
   sec 3.4 move. The real protection against oversized layer work remains the kernel slice
   caps upstream, which is where it already was.
2. **`max_loop_vertices` now binds the adequacy probes too.** Sec 3.4 says the check "moves
   naturally onto the normalized output", and one shared path means one shared cap. The
   adequacy site previously had no loop-vertex cap at all, so this is a new refusal
   path — bounded by the same kernel caps, and reachable only on input the print-layer site
   would already have refused.
3. **`orient_layers_batch` gained a `space` parameter** (phase A hard-coded
   `SliceStitchSite::PrintLayer`). Errors are still collected in layer-index order, so a
   raised seam error names the lowest failing layer — the same one the per-layer loops
   surfaced first.
4. **Phase A's module-wide `#[allow(dead_code)]` is removed**, its stated condition ("lands
   ahead of its consumers") having been met. Nothing in `orientation` is dead now.

### 8.3 V3 disposition table (sec 4, "no test is deleted without a named successor")

| Pinned-legacy test | Disposition | Note |
|---|---|---|
| `frozen_segment_vector_deduplicates_shared_owner_and_stitches_one_loop` | **Kept, re-aimed at the normalized path. Every asserted value unchanged.** | The D4 duplicate positive-edge owner does *not* degrade the layer: both `z_hat x n` votes are cast in the edge's canonical min-key→max-key frame, so opposite kernel endpoint order still agrees. Now also asserts `degraded == None`, naming that mechanism. |
| `self_crossing_outer_contour_still_yields_solids_not_a_seam_error` | **Kept, re-aimed at the normalized path. Expectation changed and re-derived.** | Was outer `52.0` with holes `[-4.4, -0.36]`; is now `[(32.4, []), (0.8, [-0.36])]`, total filled `32.84 mm²`. The old `52.0` was the raw shoelace of a self-crossing stroke — an area no fill rule agrees with — the middle ring was a first-vertex-probe verdict, and the island was claimed only by the deleted bbox tier. The new numbers are derived by hand in the test's doc comment from the even-odd crossing-number partition, block by block. |
| `a_hole_no_solid_can_reach_is_discarded_rather_than_raised` | **Deleted; successor named and landed.** | It called `hole_owner` directly, and the claim it pinned (an unclaimable hole is silently dropped) has no referent once ownership is structural. Successor: `every_normalized_hole_is_owned_by_construction_so_none_is_ever_dropped`, on the *same* fixture that used to manufacture the unownable hole; it asserts holes wind clockwise strictly inside their owner and that total filled area equals the hand-derived `32.84 mm²`, which a dropped hole would over-report and a bbox-claimed one under-report. |
| `interior_probe_leaves_the_boundary_even_when_the_centroid_does_not_land_inside` | **Deleted; successor named.** | It tested `interior_probe`, deleted with the chain that was its only caller. Successor: the same containment guarantee is now asserted structurally by the test above, and as a seeded property by `plinth-layers`' V2 containment-invariant property test. |
| `an_adequacy_probe_seam_no_longer_masquerades_as_a_print_layer` | **Unchanged.** | `SliceStitchSite::open_seam` survives the cutover verbatim. |
| `frozen_birth_overlap_ignores_retained_negative_winding_holes_like_python` | **Unchanged, and re-derived as required.** | It builds `SliceSolid`s by hand and never reaches the stitcher, so the cutover cannot move it. No expectation changed. |
| `hollow_adequacy_uses_positive_outer_area_even_when_point_is_in_hole` | **Unchanged, and re-derived as required.** | Same: hand-built `SliceSolid` straight into `carried_outer_area`. No expectation changed. |
| Phase-A orientation tests (7) and `plinth-layers` phase-B tests (23) | **Unchanged.** | Except `kernel_sliced_box_layers_orient_counter_clockwise`, which passes the new `space` argument. |

### 8.4 Phase-C scope note

This phase rewires the call sites and deletes the superseded machinery. The FU-16 treant
measurement remains phase D's. The V4 census gate was run here rather than deferred, because
sec 3.5's whole claim is that it cannot move — see 8.5 for what it actually did.

### 8.5 The census gate was run at the phase-C tip, and it MOVED — stop-and-report

Phase C's charter required running the 7-model census after cutover with the expectation,
from sec 3.5, that every digest is byte-identical. **It is not: three of seven are unmoved,
four moved.** Under OD-8 this is a stop-and-report, and nothing here is re-pinned. What
follows is the attribution that ruling needs. Run: `cargo run --release --offline --features
census --example embed_incidence_census` at the phase-C tip.

| Model | frozen `snapshot_digest` | phase-C tip | verdict | print layers differing |
|---|---|---|---|---|
| `Sphere` | `17667468971446251870` | `17667468971446251870` | **unmoved** | 0 |
| `Cylinder` | `13310185350867878190` | `13310185350867878190` | **unmoved** | 0 |
| `make_box` | `9657051231007502658` | `9657051231007502658` | **unmoved** | 0 |
| `make_overhang_table` | `6686599816688114065` | `1875674282253663076` | MOVED | 80 of 686 |
| `make_toed_slab` | `11199296506741562956` | `2823659516373762066` | MOVED | 90 of 369 |
| `make_raking_blade` | `13278215294653054002` | `9567737006650358297` | MOVED | 20 of 269 |
| `make_thin_fin` | `7016743535301716631` | `8797083662933568276` | MOVED | 6 of 432 |

`make_toed_slab` and `make_raking_blade` also fail their `support_runner_integration`
goldens: `(64, 179, 4826202191472851835)` → `(64, 170, 13346779153602682749)` and
`(32, 89, 1709218010932846012)` → `(27, 72, 8660653339934460766)`. `plinth-verify`'s
`coverage_gate` overhang-table report moves with `make_overhang_table`'s digest
(`5304257786453935365` → `5470664714994646368`; every evidence field -- islands, patch
areas, worst gaps, defects -- is unchanged, only the snapshot digest moved), and
`specialized::tests::oriented_toed_slab_and_raking_blade_finalize_after_terminal_seat_coalescing`
is the lib-side view of the same two models the runner-integration golden covers.
**Exactly three tests fail in the whole workspace**
(`public_runner_seals_toed_slab_and_raking_blade_after_seat_coalescing`,
`specialized::tests::oriented_toed_slab_and_raking_blade_finalize_after_terminal_seat_coalescing`,
`public_coverage_report_is_frozen_without_evidence_loss`), and all three are golden-pinning.
Gate 1 at this tip: **1016 passed / 3 failed / 0 ignored**, against a develop baseline of 990
passed / 0 failed; the arithmetic closes as 990 + 7 (phase A) + 23 (phase B) − 2 deleted here
+ 1 successor here = 1019 total. Gate 2 (`--all-features`): **1017 passed / 3 failed / 0
ignored** — the same three, plus one feature-gated test the default run does not build.
Gates 3 (`clippy --workspace --all-targets --all-features -D warnings`), 4 (`fmt --check`)
and 5 (`git diff --check`) are green, and the tree is clean before and after.

#### The pass-through rule is NOT what broke — four independent measurements

Measured with the legacy stitcher temporarily restored beside the new path and compared
layer by layer (instrumentation removed before this commit):

1. **Pass-through fires on 100% of already-normal layers.** On `make_toed_slab` (200 layers)
   and `make_overhang_table` (480 layers) sliced from the procedural meshes,
   `NormalizedLayer::passed_through` is true for **every** layer, `degraded` is `None` for
   every layer, and `normalized_slice_layers` output is **byte-identical** (`==` on
   `Vec<SliceSolid>`) to `legacy_stitch_layer` on every layer.
2. **Three census models move zero layers** and their digests are byte-identical, through the
   full orient + hollow + support pipeline.
3. **The adequacy-probe path moves zero rows on all seven models.** OQ-3's new consumer
   contributes nothing to the delta.
4. **The layers that do move are genuinely not normal.** Their input is two or more
   counter-clockwise loops that abut, and the nonzero union correctly merges them into one
   solid — design case D1, verbatim. First differing layer per model:

| Model | layer / z (mm) | legacy solids (area mm², vertices) | normalized |
|---|---|---|---|
| `make_overhang_table` | 346 / 17.325 | `104.23425810`(7), `18.47520861`(8), `18.47520861`(8), `18.43371399`(7), `18.43371399`(7) | `141.10168610`(18), `18.47520864`(8), `18.47520864`(8) |
| `make_toed_slab` | 40 / 2.025 | `140.29611541`(7), `14.51977914`(34) | `154.81589453`(40) |
| `make_raking_blade` | 90 / 4.525 | `160.00000000`(8), `0.30000396`(8) | `160.29743211`(16) |
| `make_thin_fin` | 166 / 8.325 | `160.00000000`(8), `0.25168756`(7) | `160.25168760`(13) |

In every case the merged outer's area equals the **arithmetic sum** of the legacy loops it
absorbed, to within 2e-8 mm² — e.g. overhang table `104.23425810 + 18.43371399 + 18.43371399
= 141.10168608` against a measured `141.10168610`. An intersection of measure zero at the
1e-8 mm quantization contract is the signature of **abutting**, not overlapping, sub-solids:
contours that touch where the legacy stitcher could not see it, because it decides incidence
only at snapped vertices and never at an edge. The union sees edge/vertex incidence and
merges, which is the physically correct fill for a union of solids and is exactly what sec
3.3 says nonzero winding buys.

#### Consequence for the owner ruling

The premise "the corpus is clean, so pass-through guarantees zero movement" is false as
stated: after orientation and hollowing, four of the seven models contain layers with
abutting sub-solid contours. The delta is not a defect in the normalizer, it is the defect
class the normalizer was chartered to fix, arriving on the clean corpus rather than only on
the treant. **No golden is re-pinned in this commit, and no test expectation was relaxed to
make a gate pass.** The four failures stand as the measurement.

One refinement the ruling may want to weigh: sec 3.5's pass-through is decided **per layer**,
so a single non-normal loop requantizes every loop in its layer — visible above as the
untouched `18.47520861` → `18.47520864` on the overhang table. A per-solid pass-through would
shrink the delta to only the merged solids, at the cost of a more complex equality check. It
would not change which layers merge, and so would not by itself restore any digest.
