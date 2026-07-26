# Resin support engine — performance profile + parallelization / port strategy

**Author role:** performance/port strategist (PLANNING ONLY). No implementation.
Per Craig's sequencing, parallelization work starts only *after* the model is
print-approved. This document is written to be adversarially challenged (an
independent OpenAI Codex review will attack it); every assumption is labeled
**[ASSUMPTION]** and every number is labeled **[MEASURED]**, **[MEASURED-coarse]**,
or **[INFERRED]** with its source.

Hardware for all runtime numbers: Apple **M3 Max, 16 cores (12 performance + 4
efficiency), 128 GB RAM** [MEASURED: `sysctl hw.ncpu`=16, `machdep.cpu.brand_string`=Apple M3 Max].
Engine today is single-process, effectively single-core (Craig observes ~15% CPU
= roughly 1 busy core + scipy's occasional KDTree worker threads).

---

## 0. Measurement status & one gap (read first)

Two profiling targets were requested. Status:

- **Table fixture (`make_overhang_table`, 60 faces):** ✅ full cProfile captured.
  Wall 19.29s under cProfile (≈10s unprofiled per the task's estimate; cProfile
  roughly 2× on this call-heavy workload). This is my **complete function-level
  profile** and the basis for the top-20 hotspot table below.
  Raw: `/Users/craig/.claude/jobs/0ddf6d41/tmp/prof_table.pstats` +
  `prof_table.py`.
- **Full Weeping Treant v1.2 (576,026 faces):** ⚠️ **fine-grained cProfile
  BLOCKED** in this checkout. The STL lives at
  `/Users/craig/Documents/Claude/Projects/D&D Campaigns/Weeping Treant v1.2.stl`
  (both scratchpad copies are symlinks to it). This process is denied read by
  **macOS TCC** on `~/Documents`: `os.stat` succeeds, `open()` →
  `PermissionError(1, 'Operation not permitted')`, even with the sandbox
  disabled and even via the venv python. A sibling agent in job
  `0ddf6d41` *did* read it earlier (it produced `engine_run_results.json` +
  `engine_run_state.pkl` at 16:29), so the grant is real but not held by this
  shell.

  **What I have for the full mesh instead, and it is real, not extrapolated:**
  the actual 193.8s production run's **coarse per-stage wall times** (from its
  logged `t_islands`/`t_minima`/`t_island_births`/`t_surface_tree`/`t_total`)
  and the **final forest scale** (from `engine_run_state.pkl`). Combined with the
  table's fine-grained ranking, these triangulate the full-mesh picture with
  only one true gap: the exact %-split of the 134s fix-pass across
  rewire/brace/verify *on the full mesh* is INFERRED from the table ranking, not
  measured on the full mesh.

  **To close the gap (5–6 min):** with a readable STL copy outside `~/Documents`,
  run `uv run --frozen python /Users/craig/.claude/jobs/0ddf6d41/tmp/prof_full.py`
  (already written; just repoint `STL`). I recommend the orchestrator either
  grant Documents access to the run shell or drop a copy at e.g.
  `/private/tmp/claude-501/.../scratchpad/treant.stl`, then I fill this section.

---

## 1. Full-mesh coarse profile — the real 193.8s run [MEASURED-coarse]

Source: `engine_run_results.json.log_summary` + `engine_run.log` from the actual
production run (`run_engine_full.py`, cfg `SupportConfig(lift_mm=3.0)`), 576,026
faces, single process.

| Stage | Wall (s) | % of 193.8s | Bound by | Parallelizable? |
|---|---|---|---|---|
| `perception.overhang_islands` | 1.23 | 0.6% | numpy/trimesh face ops | low value |
| `perception.local_z_minima` | ~2.93 | 1.5% | numpy | yes (independent of islands) |
| **`perception.island_births`** | **~45.54** | **23.5%** | **trimesh `section_multiplane` (shapely slicing)** | **yes — embarrassingly parallel across z-chunks** |
| `routing.build_surface_tree` (+delicacy, minima-contacts, sampling in `prepare`) | ~9.66 | 5.0% | trimesh `.sample()` + scipy cKDTree build | yes (mutually independent tasks) |
| **Fix-pass loop** (2 passes: `construct` + `rewire.optimize`×2 + `brace_and_size` + `verify.self_verify`) | **~134.4** | **69.4%** | **pure-Python tree math (see §2)** | **hard — sequential greedy; §4 Phase C/D** |
| **Total** | **193.76** | 100% | | |

Derivation of the stage rows (cumulative logs differenced):
`t_islands=1.23`, `t_minima=4.16` → minima 2.93; `t_island_births=49.70` →
island_births 45.54; `t_surface_tree=59.36` (run()-relative) → prepare+tree
9.66 after births; fix-pass = `t_total(193.76) − t_surface_tree(59.36) = 134.40`.

**Final forest scale [MEASURED from `engine_run_state.pkl`]:** 1,891 forest
members (1,820 live, 71 rewire-dead, 123 braces), 781 contacts, 1,625 emitted
members, 1,102 joint balls, 195 unrouted, 2 fix-pass iterations. Live-member kinds:
stub 781, strut 381, shoot 380, trunk 155, brace 98, model_rung 25.

**Two headline facts the coarse profile already proves:**
1. **~30% of wall is mesh-bound pre-processing** (islands+minima+births+tree),
   of which **island_births slicing alone is 23.5%** — and it is a `for ci in
   range(n_chunks)` loop over independent z-chunks (`perception.py:446-472`).
   This is the single biggest *easy* parallel win and needs no algorithm change.
2. **~70% of wall is the fix-pass**, which the table profile shows is
   rewire-dominated pure-Python arithmetic.

---

## 2. Table-fixture fine-grained profile — top-20 by cumulative [MEASURED]

Source: `prof_table.pstats`, cProfile, `make_overhang_table` (60 faces), 66.2M
calls / 19.287s. This tiny mesh has ~0.7s of perception and **18.6s of tree
construction + optimization**, so it isolates exactly the fix-pass hotspots that
dominate the full mesh's 134s. `cum%` is of the 19.287s wall.

| # | Function (file:line) | ncalls | tottime | cumtime | cum% | Nature |
|---|---|---|---|---|---|---|
| 1 | `engine.run` (engine.py:206) | 1 | 0.00 | 19.29 | 100% | orchestrator |
| 2 | **`rewire.optimize`** (rewire.py:832) | 4 | 0.00 | **16.45** | **85.3%** | greedy hill-climb (serial) |
| 3 | `rewire._try_subtree_reattach` (rewire.py:223) | 1087 | 0.07 | 15.83 | 82.1% | per-member candidate search |
| 4 | **`rewire._total_volume_ml`** (rewire.py:119) | 39,034 | 0.70 | **9.88** | **51.2%** | **O(N) full-forest re-sum per candidate** |
| 5 | **`cco._member_vol_ml`** (cco.py:113) | **3,848,168** | 3.16 | **9.02** | **46.8%** | **pure-Python leaf; numpy-on-scalars** |
| 6 | `rewire._try_root_reanchor` (rewire.py:314) | 541 | 0.06 | 7.71 | 40.0% | reanchor candidate search |
| 7 | **`numpy.linalg.norm`** (_linalg.py:2575) | **4,712,012** | **4.06** | 6.61 | 34.3% | **3-vector norm in a hot loop** |
| 8 | `engine.brace_and_size` (engine.py:117) | 2 | 0.00 | 6.19 | 32.1% | brace/resize fixed point |
| 9 | **`tree.revalidate`** (tree.py:281) | 47,413 | 2.08 | 4.10 | 21.3% | batched clearance re-check |
| 10 | `engine.construct` (engine.py:75) | 2 | 0.00 | 1.62 | 8.4% | load-partition + cco.build |
| 11 | `cco.build` (cco.py:157) | 2 | 0.00 | 1.62 | 8.4% | greedy CCO insertion (serial) |
| 12 | `cco._insert_terminal` (cco.py:268) | 81 | 0.02 | 1.61 | 8.4% | per-terminal insertion |
| 13 | `ndarray.dot` (builtin) | 4,708,075 | 1.21 | 1.21 | 6.3% | inside norm/geometry |
| 14 | `cco.build`(_make_bifurcation, cco.py:462) | 5,939 | 0.09 | 1.23 | 6.4% | bifurcation builder |
| 15 | `cco.build`(cco.py:431) | 38,240 | 0.14 | 1.14 | 5.9% | plate-descent builder |
| 16 | `cco._lean_deg` (cco.py:94) | 173,130 | 0.37 | 0.91 | 4.7% | per-span cone-angle (norm×2) |
| 17 | `tree.dirty_members` listcomp (tree.py:279) | 47,433 | 0.42 | 0.72 | 3.7% | O(N) dirty scan per revalidate |
| 18 | `perception.island_births` (perception.py:423) | 1 | 0.00 | 0.68 | 3.5% | slicing (dominant on FULL mesh) |
| 19 | `cco._weighted_fermat_point` (cco.py:56) | 5,939 | 0.34 | 0.65 | 3.4% | Weiszfeld iteration |
| 20 | `numpy.asarray` (builtin) | 13,238,115 | 0.61 | 0.61 | 3.2% | per-call boxing overhead |

**tottime (self-time) leaders:** `numpy.linalg.norm` 4.06s, `cco._member_vol_ml`
3.16s, `tree.revalidate` 2.08s, `ndarray.dot` 1.21s, `_total_volume_ml`
listcomp 0.70s, `numpy.asarray` 0.61s, `_lean_deg` 0.37s, Weiszfeld 0.34s.

### The single most important finding
`_total_volume_ml` (rewire.py:119) **re-sums the volume of every live member of
the whole forest** (`for i in _live_member_indices(forest): total +=
cco._member_vol_ml(...)`) and is called **once per candidate builder** inside
`_try_subtree_reattach`/`_try_root_reanchor` (before, per-candidate, after —
rewire.py:250, 288, 305). That is **O(N_members × N_candidate_moves)**. On the
table: 39,034 calls × ~99 members ≈ 3.85M `_member_vol_ml` calls = **47% of
wall**. `_member_vol_ml` itself is the numpy-on-scalars anti-pattern:
`np.linalg.norm(np.asarray(p1)-np.asarray(p0))` on a **3-vector** — each call
pays numpy dispatch + `isComplexType` + `ravel` + `dot` (~1.4µs) for arithmetic a
`math.sqrt(dx*dx+dy*dy+dz*dz)` does in ~0.1µs. This is **worst-of-both**: numpy
call overhead with zero vectorization benefit.

**Corollary [INFERRED]:** `rewire.optimize` carries a 30s wall cap
(`cfg.rewire_time_cap_s=30.0`, rewire.py:843). On the full mesh (1,820 live
members vs 99) each `_total_volume_ml` is ~18× more expensive, and rewire is
invoked ~3–4× (once per fix-pass in `engine.run` + once inside `brace_and_size`,
× 2 passes). It is highly likely rewire **hits its cap and leaves optimization
on the table** — meaning the O(N) re-sum doesn't only cost wall time, it plausibly
costs **resin volume** (worse layouts because the optimizer runs out of clock).
This is the strongest single argument for fixing §3-A1 first. *Not measured on the
full mesh — cProfile there would confirm/deny the cap-hit; see §0.*

---

## 3. numpy-vectorized vs pure-Python inventory

**Already native / vectorized (leave alone or only feed bigger batches):**
- `scipy.spatial.cKDTree.query` — C, with a `workers=` thread pool. Engine
  already gates it: `routing.query_workers(n)` returns `-1` (all cores) only for
  `n>=4096`, else `1` (routing.py:177-186 documents why: 74,535 thread
  spawns wasted 90% of a query's wall when workers=-1 on tiny batches).
- `trimesh.section_multiplane` — the island_births slicer (shapely/C). Native
  *per call*, but the **loop over z-chunks is pure-Python and serial**.
- `trimesh.Trimesh.sample()`, `mesh.area_faces`, `mesh.triangles_center` — numpy.
- `placement.load_partition` exact face-area assignment — deterministic
  `cKDTree` nearest-contact query per island; no RNG.
- `placement.tip_radius_mm` — fully vectorized numpy.
- `tree.revalidate` — *batched* clearance sampling (good), but wrapped in a
  per-member Python loop with a listcomp `dirty_members` scan each call.

**Pure-Python hot loops (the real targets):**
- `rewire._total_volume_ml` → `cco._member_vol_ml` — **the #1 target** (§2).
- `cco._lean_deg`, `_within_cone`, `_weighted_fermat_point` (Weiszfeld) — per-span
  scalar geometry, numpy-on-3-vectors throughout.
- `rewire._try_subtree_reattach` / `_try_root_reanchor` / `_try_junction_slide` —
  the candidate-search driver loops.
- `cco.build` / `_insert_terminal` — greedy CCO insertion (Python control flow).
- `joints.compute_joint_balls` — per-joint ball geometry.
- `_ensure_minima_contacts` (engine.py:319) — per-minimum Python loop with a
  cKDTree query each.

**Existing parallelism in the codebase:** none beyond scipy's internal KDTree
`workers=`. No `multiprocessing`, `concurrent.futures`, `joblib`, `threading`,
`numba`, or `cython` anywhere in `scripts/support_placement/` [MEASURED: grep].
This is exactly why CPU sits at ~15%.

---

## 4. The determinism constraint — the hard design rule that shapes everything

Craig's rule: **parallel output must be bit-identical to the serial engine**, via
*parallel evaluation + fixed-index-order reduction, never completion-order; no RNG
stream sharing.* This is not a nice-to-have; it dictates which strategies are even
admissible. Two things in the current code make it sharp:

### 4a. Global-RNG reseeding — remaining sampling requires processes or explicit
Generators

**A2 update (2026-07-25):** `placement.sample_candidates` is now a
deterministic prepared virtual pool and `load_partition` is an exact face-area
assignment; neither reads or mutates RNG state. The deprecated
`sample_candidates(seed=...)` argument remains accepted and ignored for
compatibility.

The remaining hot spots that seed the **process-global numpy RNG** and sample
are `perception.delicacy_field` and `routing.build_surface_tree`. Trimesh's
`.sample()` draws from global `np.random` state, not a passed Generator.
Consequences that MUST be encoded as design rules:
- **[HARD RULE] No shared-memory threading across any stage that samples.** Two
  threads sharing one global RNG interleave draws non-deterministically → not
  reproducible, let alone bit-identical. Use **processes** (each gets its own
  global RNG) *or* refactor every `.sample()` path to an explicit
  `np.random.Generator` seeded from a fixed, unit-id-derived seed.
- **[RESOLVED BY A2]** `load_partition` no longer has an order-dependent random
  stream. Fixed-index-order reduction remains mandatory because carried-area
  sums still feed radii and topology decisions.

### 4b. Floating-point reduction order
- **[HARD RULE] Every reduction sums/gathers in fixed index order, never
  completion order.** `_total_volume_ml` sums members by ascending index; an
  incremental or tree-partial re-sum can reorder float adds and differ in the last
  ULP. A ULP-level volume delta is usually invisible in the emitted file (radii
  round to file precision), **but** rewire's accept test is a bare inequality
  (`vol_after >= vol_before - 1e-9`, rewire.py:303) and `_try_...`'s
  `vol < best_vol` tie-break — a borderline candidate can *flip*, changing
  topology and the whole downstream. So: any incremental-volume optimization
  (§3-A1) must be **validated byte-for-byte against the full-resum baseline**, and
  if it ever diverges, the fixed-index full re-sum is the ground truth.
- **[HARD RULE] Parallel gathers re-sort to canonical order before use.**
  `island_births` must emit in serial `(chunk, z, component)` order;
  `_ensure_minima_contacts`/`_ensure_island_birth_contacts` append contacts in a
  fixed order that seeds `island_id`s — any parallel producer buffers per-worker
  then concatenates in worker-index order, never as-completed.

### 4c. What "bit-identical to the serial engine" means — TWO readings [ASSUMPTION]
This is the assumption most likely to be challenged, so I state it explicitly and
commit to a position:
- **Reading A (frozen baseline):** parallel engine == *today's* serial Python
  engine, byte-for-byte. Forbids **any** change that alters a float result —
  including a language port (you cannot reproduce shapely/scipy/trimesh numerics
  bit-for-bit in Rust/C++; see §6).
- **Reading B (self-consistency + re-approval):** the shipped engine's
  **parallel mode == its own serial mode**, byte-for-byte; whenever a change
  alters the *serial* baseline (a native kernel or other arithmetic port), that
  new serial baseline is re-run through Craig's print-approval gate **once**, and
  from then on parallel must match it exactly.

**I plan against Reading B**, because Reading A makes "parallelize as much as
possible" nearly impossible and forbids a port outright, which the task explicitly
puts on the table. Under Reading B the acceptance gate per phase is: *(i)* the
phase's serial baseline is unchanged OR re-approved by Craig; *(ii)* parallel
output is byte-identical to that serial baseline; *(iii)* suite green; *(iv)*
measured speedup ≥ target. **Codex should challenge whether Craig actually means A
or B** — if A, §5 Phases 1–2 (pure Python, same ops, same order) are still valid
and §6 ports are off the table; if B, everything below stands.

---

## 5. Phased strategy — "parallelize as much as possible," staged by ROI × risk

Ordering principle: **kill serial waste before adding cores** (a 3× algorithmic
win on the fix-pass is worth more than 16× on a stage that's 5% of wall, and it
shrinks the surface you later parallelize). Phases are independently shippable;
each has its own acceptance gate.

### Phase 0 — Instrumentation & determinism harness (prerequisite, ~0.5 day)
Before touching anything: a **byte-identical harness** that runs `engine.run` and
hashes the emitted `.chitubox` (and, cheaper, hashes `(members, balls)` geometry
arrays) so every later change is gated by `sha256(before)==sha256(after)`. Add a
full-mesh cProfile capture to CI-adjacent scratch (close §0's gap). Add a
`n_workers`/`deterministic` config knob (default 1) so serial and parallel are the
same code path with a worker count of 1.
- **Gate:** harness reproduces the current emitted file's hash across 3 runs.

### Phase 1 — Single-core algorithmic wins (no parallelism) — **biggest ROI**
No new cores, no determinism risk beyond §4b validation. Target the §2 hotspots:

- **A1. Incremental forest volume (kills #4/#5/#7).** Replace `_total_volume_ml`'s
  O(N) re-sum with an incrementally-maintained `forest.total_vol_ml` updated only
  for the members a move touches (the detached subtree + slimmed ancestors + new
  connector). Candidate scoring becomes O(Δ) not O(N). *Determinism:* keep a
  fixed-index full re-sum as the checkpoint/rollback ground truth and assert
  equality within tolerance in debug builds (§4b).
  **Effort:** 1.5–2.5 days. **Expected [ESTIMATE]:** removes most of the 47%
  `_member_vol_ml` cost; table fix-pass ~16.5s → plausibly ~4–6s. On the full
  mesh this likely *also* lets rewire converge inside its 30s cap (better resin
  volume), a quality win on top of speed.
- **A2. De-numpy the scalar geometry leaves.** `_member_vol_ml`, `_lean_deg`,
  `_within_cone`, `_is_degenerate_span` operate on 3-vectors — replace
  `np.linalg.norm`/`np.asarray` with `math.sqrt`/`math.hypot` on unpacked floats,
  or batch them. This is *bit-identical-preserving only if done carefully*
  (`math.sqrt(dx*dx+dy*dy+dz*dz)` vs `np.linalg.norm` can differ in the last ULP —
  validate under Phase 0 harness; if it diverges, this becomes a Reading-B
  baseline change). **Effort:** 1 day. **Expected:** cuts the 4.06s norm self-time
  and 0.61s asarray boxing substantially.
- **A3. Memoize `delicacy_field` across the run.** It's recomputed inside *every*
  `rewire.optimize` call (rewire.py:845) — 20,000 samples + KDTree each time,
  ~3–4× per run — yet it depends only on the mesh+cfg, which don't change. Compute
  once in `prepare`, thread through. **Effort:** 0.5 day. **Expected:** removes
  ~3× redundant 20k-sample passes from the full run. *Determinism:* trivially
  identical (same inputs → same output, computed once).
- **A4. `tree.revalidate` dirty-set.** It rebuilds `dirty_members` via an O(N)
  listcomp each call (47k calls on the table). Maintain a dirty *set* incrementally
  (members already `_touch`ed). **Effort:** 1 day.
- **Phase-1 Gate:** emitted file **byte-identical** to the frozen post-A2
  Phase-0 baseline; full
  suite green; **target ≥2.5× full-mesh wall** (193.8s → ≲78s), driven by
  fix-pass shrink + A3.

### Phase 2 — Process-parallel independent sites — **easy cores, high value**
Now spend the 16 cores where the work is embarrassingly parallel and the
determinism story is clean (fixed-order gather, per-unit seeds). Use
`concurrent.futures.ProcessPoolExecutor` (processes, not threads — §4a) or, for
NumPy-heavy chunks, `loky`/`joblib`. Order all gathers by unit index (§4b).

- **B1. Parallel `island_births` over z-chunks (23.5% of wall).** `n_chunks`
  independent `section_multiplane` calls; the only serial dependency is the
  cross-chunk "previous layer" for the overlap test at each chunk's bottom — solved
  by having each chunk *also* slice one extra layer below its start (read-only
  overlap context), then gathering births in `(chunk, z, component)` order.
  **Effort:** 1.5–2 days. **Expected [ESTIMATE]:** 45.5s → ~5–8s on 12 P-cores
  (slicing is CPU-bound; near-linear until memory bandwidth). *Determinism:*
  births re-sorted to serial order; no RNG in this stage.
- **B2. Concurrent independent prepare tasks.** `local_z_minima`,
  `delicacy_field`, `build_surface_tree`, `overhang_islands` are mutually
  independent — run as 4 concurrent processes. Modest (they're ~7% combined) but
  free once B1's harness exists. *Determinism:* each seeds its own global RNG in
  its own process (already the pattern) — safe by construction.
- **B3. Parallel `load_partition` per island** — A2 removed the former
  Monte-Carlo and replaced it with exact face-area assignment. Islands are now
  independent; gather and reduce in fixed island/contact index order.
  **Effort:** 0.5 day. *Determinism:* no RNG; serial arithmetic order remains
  the baseline.
- **B4. Parallel verify gates.** `verify.self_verify` runs several independent
  gate checks (clearance, connectivity, printability, murray, load, minima,
  island-birth coverage) — fan them out, gather violation lists in fixed gate
  order. **Effort:** 1–1.5 days.
- **Phase-2 Gate:** byte-identical to Phase-1 baseline; suite green;
  **target: prepare stage ≥4× (59s → ≲15s)**,
  full run (with Phase 1) **≲45s**.

### Phase 3 — Parallelizing the greedy core (the hard 70%) — **highest risk**
The fix-pass is a **sequential greedy hill-climb**: `cco.build` inserts terminals
one at a time (each sees prior tree state), and `rewire.optimize` sweeps moves in a
fixed `_sweep_order`, each accepted move mutating the shared forest and feeding the
rolling-window convergence test (rewire.py:864-897). You cannot naively parallelize
without changing results. Three admissible tactics, increasing ambition:

- **C1. Intra-insertion / intra-move candidate parallelism (deterministic).**
  Within a *single* `_insert_terminal` or `_try_subtree_reattach`, the many
  candidate builders are scored independently (each on a throwaway checkpoint) and
  the winner chosen by a *fixed-index* argmin. Evaluate candidates in parallel,
  **commit the argmin in fixed candidate order** (never first-to-finish). This is
  pure speculative parallelism with a deterministic reduction — bit-identical by
  construction. **Effort:** 3–5 days (checkpoint/rollback must be process-safe or
  the candidate eval must be pure/functional). **Expected:** speeds each move but
  bounded by per-move candidate count; best combined with Phase-1 A1 (fewer, faster
  candidates). *Risk:* the forest checkpoint object (`tree.py` guarded-setattr +
  `_touch_member`) is stateful and not obviously fork-safe — may require making
  candidate scoring a pure function of (subtree, target) returning a volume, no
  shared mutation.
- **C2. Independent-tree parallelism (deterministic, structural).** The forest is
  many roots; a large class of moves (junction slide, twig prune, degenerate
  retire, intra-tree reattach) never cross trees. **Partition trees into
  non-interacting groups, optimize groups in parallel, then apply accepted moves in
  fixed tree-index order**, re-checking any cross-tree candidate (root reanchor /
  plate-descent merge between nearby trees) serially in a final reconcile sweep.
  This preserves serial semantics *if* the partition provably excludes cross-tree
  moves within a round. **Effort:** 5–8 days. **Risk:** proving non-interaction is
  subtle — a slid junction changes clearance geometry a neighboring tree's
  candidate reads via `surface_tree` (read-only, safe) but *also* via `braced_hosts`
  and forest volume (shared) — needs the C2 round to freeze those.
- **C3. Speculative-parallel sweep with deterministic commit
  (research-grade).** Evaluate a whole sweep's moves in parallel against a frozen
  snapshot, then replay accepted moves **in fixed `_sweep_order`**, re-validating
  each against the now-mutated forest and rejecting any whose certificate no longer
  holds (transactional, exactly the existing checkpoint protocol). Output identical
  to serial *by replay order*, with parallelism in the evaluate phase.
  **Effort:** 8–12 days. **Risk:** high; conflict/re-validation rate may erode the
  speedup; this is where a native core (§6) pays off more than threads.
- **Phase-3 Gate:** byte-identical to Phase-2 baseline (this phase must NOT change
  the baseline — it's pure speedup of the same computation); suite green; **target
  fix-pass ≥3× on top of Phase 1**.

### Amdahl ceiling [ESTIMATE, hardware = 12 P-cores]
Using the real 193.8s split — prepare/tree ~30.6% (highly parallel, B1/B2), fix
~69.4% (partly serial). If Phase 1 first cuts the fix-pass ~2.5× (→ ~54s) and
prepare ~1.2× via A3 (→ ~50s), the run is ~104s. Then Phase 2 takes prepare ~4×
(50→~13s) and Phase 3 takes the fix ~3× (54→~18s): **~31s, ~6.3× end-to-end** on a
16-core M3 Max. The serial residue (rollback bookkeeping, final reconcile sweeps,
emit) caps practical gains around **6–8×**; getting past that needs §6.

---

## 6. Language-port analysis — is a native core / full port warranted?

Craig's framing: Python bought R&D agility; a **function-preserving** port for
speed is on the table. The determinism gate reshapes this question decisively.

### The determinism gate largely forbids a *full* port
"Bit-identical" (Reading B: parallel==serial *within the shipped engine*) is
achievable in any language. But if the intent leans toward Reading A (match
*today's* Python), a full rewrite is effectively impossible: the engine's numbers
come from **numpy (Mersenne-Twister global RNG), scipy cKDTree (tie-breaking,
worker-order), trimesh, and shapely (`section_multiplane`)**. No Rust/C++ stack
reproduces shapely polygon slicing or numpy's MT19937 draw sequence bit-for-bit.
So a full port necessarily establishes a **new serial baseline** that must re-pass
Craig's print-approval gate — acceptable under Reading B, fatal under Reading A.
**[ASSUMPTION → Codex challenge]:** I treat a full port as a *new-baseline* effort
(Reading B), not a bit-match to today's Python.

### Three port shapes, ranked

1. **RECOMMENDED — keep Python orchestration, native core for the arithmetic
   kernels.** Port only the §2 pure-Python hotspots — `_member_vol_ml` &
   volume accounting, `_lean_deg`/`_within_cone`, the `rewire` candidate-scoring
   inner loops, `revalidate`'s per-member sampling, the Weiszfeld solver — into a
   **Rust extension via PyO3/maturin** (or C++ via pybind11), leaving mesh I/O,
   slicing, KDTree, and sampling in numpy/scipy/trimesh (where Python is already
   just calling C). This is the classic "hot 5% in native, cold 95% in Python"
   split and it maps perfectly onto this profile: the hotspots are small, numeric,
   and self-contained (3-vectors and member arrays), while the mesh libraries you'd
   *never* want to reimplement stay put.
   - **Determinism:** you control summation order and can match numpy's scalar math
     exactly for the simple kernels (avoid FMA/contraction via `-ffp-contract=off`
     / Rust's default no-FMA; sum in fixed index order). A native core with
     `rayon` (Rust) gives **deterministic data-parallelism** (fixed-range splits,
     ordered reduction) far more cheaply than Python multiprocessing — no
     fork/pickle overhead, shared-memory arrays, real threads with no GIL.
   - **Effort:** 3–5 weeks for the kernel set + PyO3 bindings + a numpy-parity test
     harness. **Risk:** medium. **Tradeoff:** best speed-per-effort *and* best
     determinism control; keeps R&D agility for everything above the kernel.
   - **This is the natural home for Phase-3 C3** (speculative-parallel sweep): a
     native, GIL-free, deterministic-reduction sweep evaluator is where the fix-pass
     70% actually gets crushed.

2. **Full Rust port (function-preserving, new baseline).** Reimplement the whole
   pipeline in Rust; use `parry`/`rerun`-adjacent geometry crates for
   mesh/slicing/KD-trees. **Tradeoff:** maximal speed (10–30×+ plausible, GIL-free,
   `rayon` throughout) and one clean deterministic story, but **loses the R&D
   agility Craig explicitly valued**, is a multi-month effort, and must re-derive
   *every* geometric result (island_births slicing semantics, KDTree tie-breaks)
   from a Python reference — a large validation burden. **Effort:** 3–6 months.
   **Risk:** high (scope, geometry-lib parity, re-approval of a wholly new baseline).
   **Verdict:** only if Option 1 proves insufficient *and* the engine is
   feature-frozen (no more R&D churn) — neither is true today.

3. **Full C++ port.** Same shape as (2) with CGAL/libigl for geometry.
   **Tradeoff vs Rust:** more mature geometry libraries (CGAL slicing is
   battle-tested), but weaker memory-safety guarantees around the transactional
   checkpoint/rollback core (exactly the part most prone to the "corruption over
   crash" bugs the code comments keep flagging), and no `rayon`-equivalent
   ergonomics for *deterministic* parallel reduction. **Verdict:** Rust preferred
   over C++ for this specific codebase because its safety + `rayon`'s ordered
   parallelism directly serve the "crash over corruption" + "fixed-index reduction"
   invariants the engine already lives by.

### Port recommendation
**Do Option 1 (Rust kernel via PyO3), and only as Phase 4 — after Phases 1–2 prove
how much pure-Python + multiprocessing already buys.** Rationale: Phase 1's
incremental-volume fix may remove so much of the 70% that the remaining native
upside doesn't justify a 3–5 week kernel port; measure first. If after Phases 1–3
the fix-pass is still the wall and still cap-limited, the Rust kernel (with a
deterministic `rayon` sweep) is the endgame. A **full** port (Options 2/3) is
**not** recommended now — it forfeits R&D agility for speed the kernel split can
mostly deliver, and doubles the determinism-validation burden.

---

## 7. Effort, risk & acceptance-gate summary

| Phase | Scope | Effort | Risk | Determinism story | Acceptance gate |
|---|---|---|---|---|---|
| 0 | Byte-hash harness + full-mesh cProfile + `n_workers` knob | 0.5 d | low | n/a | hash reproducible ×3 |
| 1 | Incremental volume, de-numpy leaves, memoize delicacy, dirty-set | 4–5 d | low–med | byte-identical to frozen post-A2 baseline | **byte-identical**, suite green, **≥2.5× full run** |
| 2 | Process-parallel births / prepare / load_partition / verify | 5–6 d | med | fixed-order gather; exact load partition has no RNG stream | byte-identical, suite green, **prepare ≥4×** |
| 3 | Parallel greedy core (C1 candidate → C2 tree-group → C3 sweep) | 3–12 d/tactic | med–high | speculative eval + fixed-order replay/commit | **byte-identical**, suite green, **fix ≥3×** |
| 4 (opt) | Rust kernel via PyO3 for §2 hotspots + `rayon` sweep | 3–5 wk | med | new baseline (Reading B), numpy-parity harness | re-approved baseline, parallel==serial, **≥6–8× total** |

### Risk register
- **R1 — ULP drift flips a rewire accept/reject** (`vol >= vol_before-1e-9`). Every
  Phase-1/3 change gated by the byte-hash harness; on divergence, fixed-index full
  re-sum is ground truth. *Likelihood med, impact high (silent topology change).*
- **R2 — Global-RNG order dependence** (§4a) silently broken by parallelism.
  Mitigate: per-unit deterministic seeds *before* any parallel gather; ban threads
  on sampling stages. *Likelihood high if ignored, impact high.*
- **R3 — Checkpoint/rollback not fork-safe** (`tree.py` guarded-setattr, Member↔
  Forest cycle). C1/C2 may need candidate scoring refactored to a pure function.
  *Likelihood med, impact med (blocks Phase 3, not 1–2).*
- **R4 — rewire cap masks correctness regressions.** If Phase 1 makes rewire
  converge where it used to cap out, the *baseline itself changes* (better volume)
  — that's a Reading-B re-approval event, not a bug, but must be surfaced to Craig,
  not silently shipped. *Likelihood high, impact low if flagged.*
- **R5 — Amdahl reality < estimate.** Serial reconcile sweeps (C2/C3) and
  emit/verify residue may cap gains at ~6×, not the headline. Mitigate: measure
  after each phase; don't pre-commit to a port on projected numbers.
- **R6 — Full-mesh profile gap (§0).** Phase-3 sizing (which tactic pays) depends
  on the *actual* rewire-vs-brace-vs-verify split on the full mesh, currently
  INFERRED. Close before committing Phase-3 effort.

### Open questions for Craig / Codex
1. **Determinism Reading A vs B (§4c)** — this single choice decides whether
   native ports with different arithmetic are admissible. I assume B.
2. Is a **one-time print-re-approval** of a new serial baseline acceptable when a
   native kernel improves resin volume but alters the emitted file? Phase 4
   depends on it.
3. Is the engine **feature-frozen** enough to justify a native kernel, or is R&D
   churn still high (which argues for staying pure-Python longer)?
