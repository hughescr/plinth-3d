# Remediation Tracking

This is the status of every one of the 189 findings extracted from the 2026-08-26 plinth-3d code review (the review verified 207, 203 survived verification, and systematic extraction yields these 189 deduplicated rows). Each row carries one status, graded on **2026-08-27** against the tree at `develop` and the 34 commits in the range `041ba6d..develop`.

Rows are organized by the remediation plan's phases (0-7) plus the standing backlog. Three rows are pointers rather than findings and carry `n/a`.

**File:Line** is current as of 2026-08-27 for rows that are still OPEN. For closed rows it is left at the `041ba6d` value the extraction recorded, and has drifted.

### What each status means

| Status | Meaning | Evidence required, and supplied, for every row |
|---|---|---|
| `FIXED (sha)` | The row's specific claim no longer holds in the tree. | A commit that exists, plus a named difference in the current tree — not merely that the file was edited. |
| `SUPERSEDED (sha)` | The code the finding lived in was deleted rather than repaired. | The deleting commit, and confirmation the path is gone. |
| `DEFERRED (sha)` | Read, decided against, and the decision is written down. | The commit message or `FIX-AFTER-RUN.md` passage recording the decision and its reason. |
| `DUPLICATE (ID)` | The row restates another row's finding. | The canonical row's ID; the defect is tracked there. |
| `OPEN` | The defect is still in the tree. | The current file and line still carrying it, read directly. |
| `UNCERTAIN` | Not determinable. | None outstanding — no row landed here. |

### Counts

| Status | Rows |
|---|---|
| OPEN | 124 |
| FIXED | 41 |
| SUPERSEDED | 12 |
| DUPLICATE | 7 |
| DEFERRED | 5 |
| UNCERTAIN | 0 |
| **Total** | **189** |

The seven duplicates are T3.8-1 (of SL-0.3), T3.6-13 (of T3.6-16), T3.10-16 (of T1.4), T3.11-8 (of T3.11-1), T3.12-8 (of T3.5-25), T3.12-10 (of T3.2-18) and T3.12-11 (of T3.2-19). Counting distinct findings rather than rows, the backlog is 124 open out of 182.

### UNCERTAIN rows

There are none. Every row's claim turned out to be concrete enough to check by reading the named symbol in the current tree and the 34 commit messages, so no row needed the status. The cases where judgement, rather than evidence, decided the answer are listed next rather than hidden inside a status word.

### Partially addressed, and graded OPEN anyway

Six rows had part of their claim closed by a commit while the defect the row exists to stop is still in the tree. They are OPEN, because a row is not closed until its own claim stops holding — but the partial credit is real and is recorded here so it is not lost.

| Row | What landed | What still holds |
|---|---|---|
| SL-11 | `NEAR_WALL_SKIP` retired and documented (`92a1be1`); `cone_thickness` now decides by normal orientation. | `ZDEDUP = 5e-3` still decides parity crossing-cluster dedup at `consts.rs:22`, still citing a `geometry_exact.py` that exists nowhere in the repo. |
| T3.3-11 | The typed `SegmentClearance` / `UnmeasuredSegment` API landed (`2333009`) and the one caller the review named now refuses an unmeasured row via `clears_or(false)` (`242587e`). | The float shim still returns `INFINITY` in band, and `242587e` states twelve sibling call sites (brace.rs ×5, contact.rs ×2, cco.rs ×5) still read it as a pass. |
| T3.7-3 | `signed_area`'s doc no longer asserts an identity nothing enforces (`cdb61bd`). | Two independent `signed_area` implementations remain; that commit says consolidating them is not done here. |
| T3.7-4 | `down_limit`'s cross-reference now names the function instead of a drifted line (`cdb61bd`). | The same value is still computed by two separate expressions in two crates. |
| T3.11-5 | The traversal-cost doc at the cited line corrected 13 → 12 (`cdb61bd`). | The public accessor doc 1,200 lines later still says **thirteen** and `13p − 12`, so the two public numbers still disagree. |
| T3.12-12 | `try_run` / `PoolBuildError` added as a fallible sibling for callers outside the kernel (`e8caa67`). | `PLINTH_THREADS` still has no validation (garbage silently becomes all-cores) and `pool_for` still `.expect(...)`s inside `run`. |

Two further rows, T3.9-7 and T3.1-4, are separate review findings filed at one location (`coverage_evidence.rs:365`) from two angles — the failure behaviour and the round-trip that causes it. They are kept as two rows because the review filed them as two; both are OPEN on the same line.

---

## Phase 0 — Make change safe

Foundational work needed before later phases can be proven.

| ID | Severity | Category | File:Line | Description | Effort | Status |
|---|---|---|---|---|---|---|
| SL-0.3 | medium | Tests | `crates/plinth-hollow/src/drains.rs:3079` | No test drives the committed treant past `plan_drains`; the refill, audit rescue and `TrappedCavity` check are all downstream | contained | FIXED (f3e2258) |
| T3.8-1 | medium | Tests | `crates/plinth-hollow/src/drains.rs:3079` | Same - no full hollowing chain test on multi-chamber model | contained | DUPLICATE (SL-0.3) |
| T3.8-2 | medium | Tests | `crates/plinth-hollow/src/lib.rs:4922; crates/plinth-decimate/tests/local_minis_repair.rs:1` | Two named corpus regressions guarded only by `#[ignore]`d tests reading local STLs not in git | substantial | OPEN |

---

## Phase 1 — Free wins

No behavior changes; restore reachability signal and fix direct defects.

| ID | Severity | Category | File:Line | Description | Effort | Status |
|---|---|---|---|---|---|---|
| SL-2 | critical | Correctness | `rust/geom_kernel/src/queries.rs:1827, :2142` | A BVH global face id used as a slot index — panics on sparse subset or exempts wrong faces from clearance check | trivial | FIXED (a929cea) |
| SL-1-disc | critical | Safety | `crates/plinth-pipeline/src/gates.rs:303-313` | The pre-write gate runs only one of twelve checks; disclosure of `pending_gates` is dropped before run report (short-list 1, disclosure half) | trivial | FIXED (c9a9e4d) |
| SL-10 | high | Constants | `crates/plinth-support/src/legacy_compat.rs:38-42, :77-81, :157-161` | Three methods carry false `#[allow(dead_code)]` saying consumers have not landed, but `run_support` path calls all three | trivial | FIXED (fb4fe23) |
| T1.4 | high | Architecture | `rust/geom_kernel/src/lib.rs:25` | `mod vec3` and `mod par` are private so every crate above re-implements them — make public in Phase 1 | trivial | FIXED (dc2e0d0) |
| T3.2-1 | medium | Constants | `crates/plinth-support/src/cco.rs:4527` | An unnamed `+ 1.0` mm decides which spiral plate-descent candidates exist at all | trivial | FIXED (2d41082) |
| T3.2-2 | medium | Constants | `crates/plinth-support/src/brace.rs:3008` | `rungs_that_fit` removes requested cross-braces using an unexplained universal `- 1.0` mm | trivial | FIXED (2d41082) |
| T3.3-1 | high | Correctness | `crates/plinth-support/src/anchor.rs:1169-1320` | Anchor qualification not re-run after ray snaps to nearer face; min-length prune and `max_delicacy` screen never applied to seated face | contained | OPEN |
| T3.4-1 | medium | Numerics | `crates/plinth-support/src/specialized.rs:8213` | Demand ordering tie-breaks on `to_ne_bytes()`, neither numeric order nor target-independent, reaches sealed plan digest | trivial | FIXED (2d41082) |
| T3.6-1 | medium | Dead code | `crates/plinth-support/src/transient.rs:424` | `resize_forest_current_parity` marked dead while `brace.rs` calls it in two live paths | trivial | FIXED (fb4fe23) |
| T3.6-2 | medium | Dead code | `crates/plinth-support/src/legalization.rs:412` | `LegalizationSteering::round()` marked dead one commit ahead of its reader in `runner.rs` | trivial | FIXED (fb4fe23) |
| T3.6-3 | medium | Dead code | `crates/plinth-support/src/coverage_closure.rs:600` | Seven allows on ranking machinery say "lands in next commit"; it landed | trivial | FIXED (fb4fe23) |
| T3.7-1 | low | Duplication | `crates/plinth-support/src/specialized.rs:2048` | Two ways to project onto face subset; production picked the path that carries short-list 2 | contained | OPEN |
| T3.10-1 | medium | Naming | `rust/geom_kernel/src/queries.rs:2185` | Radii parameters bare `r0s`/`r1s` with no radius-vs-diameter statement, at boundary where support crate crosses with those numbers | trivial | OPEN |
| T3.12-1 | medium | Code smell | `crates/plinth-support/src/cco.rs:1779, :3538` | The "host kd-tree cache" is a linearly scanned `Vec`; no kd-tree exists, cache removes no term from cost | contained | OPEN |
| ~25 more trivial findings | — | — | — | Stale comments, unregistered constants, misc code smell and naming issues | trivial | n/a |

---

## Phase 2 — Delete what nothing runs

Now that labels are honest.

| ID | Severity | Category | File:Line | Description | Effort | Status |
|---|---|---|---|---|---|---|
| T3.6-4 | medium | Dead code | `crates/plinth-metrics/Cargo.toml:1` | The 9,176-line audit crate has no production consumer; `plinth score` subcommand was removed | contained | SUPERSEDED (3913d50) |
| T3.6-5 | medium | Dead code | `crates/plinth-support/src/runner.rs:3372` | The 3,788-line `transit_measurement` module has no production caller despite explicit reasoned allow | substantial | SUPERSEDED (5d0420e) |
| T3.6-6 | medium | Dead code | `crates/plinth-support/src/designed_embed.rs:201` | The designed-mask mint (`DesignedMaskTable`, `validate_forest`) has no production consumer | substantial | SUPERSEDED (5d0420e) |
| T3.6-7 | medium | Dead code | `crates/plinth-support/src/cco.rs:4163` | `mask_inset0`/`mask_inset1` allocated per contact and never read on hottest batched path | trivial | FIXED (2d41082) |
| T3.6-8 | medium | Dead code | `crates/plinth-support/src/cco.rs:4607` | The `cached_hosts` `None` fallback is unreachable and would rank hosts by different metric if reached | trivial | FIXED (2d41082) |
| T3.6-9 | low | Dead code | `rust/geom_kernel/src/queries.rs:2872` | `penetration_intervals_batch` kept "solely for PyO3 frozen-oracle surface" that does not exist | trivial | SUPERSEDED (5d0420e) |
| T3.6-10 | low | Dead code | `crates/plinth-core/src/constants/placeholders.rs:1342` | Four `TrialSetting` perf-experiment constants have no production reader | trivial | DEFERRED (cdb61bd) |
| T3.6-11 | low | Dead code | `crates/plinth-core/src/provenance.rs:1393` | The `Pending<T>` provenance category is fully implemented but used by none of 306 registered constants | contained | DEFERRED (c3a009b) |
| T3.6-12 | low | Dead code | `crates/plinth-pipeline/src/lib.rs:373` | `--no-cache` is parsed and reported but controls nothing because no cache exists | trivial | DEFERRED (bdab3aa) |

---

## Phase 3 — One home per fact

Duplication consolidation and constant registration.

### Group A — Geometry helpers copied everywhere

| ID | Severity | Category | File:Line | Description | Effort | Status |
|---|---|---|---|---|---|---|
| T3.7-2 | low | Duplication | `crates/plinth-support/src/perception.rs:1538, :1551` | Four 2D primitives copied across three files with drifted epsilons | contained | OPEN |
| T3.7-3 | low | Duplication | `crates/plinth-layers/src/lib.rs:292` | `signed_area` duplicated across crate boundary, identity assertion unenforced | trivial | OPEN |
| T3.7-4 | low | Duplication | `crates/plinth-orient/src/score.rs:255; crates/plinth-support/src/lib.rs:894` | Down-facing predicate copied between crates, cross-reference stale by 57 lines | trivial | OPEN |
| T3.7-5 | low | Duplication | `crates/plinth-mesh/src/delicacy_eval.rs:401-421; crates/plinth-mesh/src/lib.rs:243` | Vec3 arithmetic and triangle unit-normal twice with different degeneracy paths inside one crate | contained | OPEN |
| T3.7-6 | low | Duplication | `crates/plinth-hollow/src/drilling.rs:1737` | Three ties-to-even rounding implementations in one crate, two hand-rolled, where `f64::round_ties_even` available since 1.77 | trivial | FIXED (cdb61bd) |
| T3.7-7 | low | Duplication | `crates/plinth-metrics/src/scar.rs:1282` | One axis-crossing cluster tolerance, two independent owners (`AXIS_CROSSING_CLUSTER_MM` and kernel `ZDEDUP`) | trivial | SUPERSEDED (3913d50) |

### Group B — Ray-hits-triangle implementations

| ID | Severity | Category | File:Line | Description | Effort | Status |
|---|---|---|---|---|---|---|
| T3.7-8 | high | Duplication | `crates/plinth-mesh/src/triage.rs:1099` | Third hand-written Möller implementation plus full-face scan, while `delicacy_eval.rs` does the same query in batched kernel call | substantial | OPEN |
| T3.7-9 | medium | Duplication | `crates/plinth-metrics/src/geometry.rs:276, :300` | Auditor's ray/triangle copy still has behind-origin defect fixed in `plinth-mesh`; listed as open #50 | contained | SUPERSEDED (3913d50) |

### Group C — One law, multiple owners

| ID | Severity | Category | File:Line | Description | Effort | Status |
|---|---|---|---|---|---|---|
| T3.7-10 | medium | Duplication | `crates/plinth-support/src/cco.rs:5704; crates/plinth-support/src/rewire.rs:1391` | Tapered-member volume law, three owners in one crate, differ by rounding — feed pricing and rewire savings | contained | OPEN |
| T3.7-11 | low | Duplication | `crates/plinth-support/src/cco.rs:3021` | FNV fold hand-inlined three more times after factored out precisely to prevent it | trivial | FIXED (2d41082) |
| T3.7-12 | medium | Duplication | `crates/plinth-support/src/specialized.rs:3028, :3168` | Two structurally identical restart-from-scratch merge loops, byte-identical commit tail written three times | contained | OPEN |
| T3.7-13 | medium | Duplication | `crates/plinth-support/src/specialized.rs:6227` | Gate's plan and fill gather same route points from two hand-kept copies; disagreement surfaces as fingerprint mismatch | trivial | FIXED (2d41082) |
| T3.7-14 | medium | Duplication | `crates/plinth-support/src/perception.rs:4117` | `finalize_retained_round` re-implements `validated_contact_order` and `hash_physical_contacts` inline in two places | trivial | FIXED (2d41082) |
| T3.7-15 | low | Duplication | `crates/plinth-support/src/specialized.rs:8125` | `point_in_polygon` verbatim duplicated from `perception.rs` with different segment-degeneracy rule | contained | OPEN |
| T3.7-16 | low | Duplication | `crates/plinth-support/src/cco.rs:5299` | Plate-topology predicate written twice in two forms, both exact `f64` equality, must agree to size and price | trivial | FIXED (2d41082) |
| T3.7-17 | medium | Duplication | `crates/plinth-support/src/joints.rs:15-22` | Seven module literals duplicate registry policy entries; colinear cosine pre-evaluated conversion of registered angle | contained | OPEN |

### Group D — Registry value restated as bare number

| ID | Severity | Category | File:Line | Description | Effort | Status |
|---|---|---|---|---|---|---|
| SL-8 | high | Architecture | `crates/plinth-core/src/provenance.rs:1373-1390` | `Placeholder` values reach acceptance paths including coverage gate; only one signature enforces `AcceptSafe` bound | contained + project | OPEN |
| SL-6 | high | Constants | `crates/plinth-support/src/perception.rs:626-632` | Tip radius law built from four "legacy diagnostic" placeholders, disagrees with physics law documented in `docs/physics.md` | substantial | OPEN |
| SL-9 | high | Constants | `crates/plinth-hollow/src/drilling.rs:1328-1329` | Drilling re-derives registered drain keepout ladder as literals in exact drift class its own comment condemns | contained | OPEN |
| T3.2-3 | high | Constants | `crates/plinth-support/src/brace.rs:2968-2981` | `slender_rungs` searches 0..=50 and returns `Ok(50)` even when deflection predicate never passed | contained | OPEN |
| T3.2-4 | high | Constants | `crates/plinth-mesh/src/triage.rs:16-52` | `TriageSettings` documented as diagnostic placeholders yet decide which components are silently deleted as dust shards | project | OPEN |
| T3.2-5 | high | Constants | `crates/plinth-hollow/src/lib.rs:2180, :3735-3741` | `PRINTABLE_MAX_EXTENT_MM = 300.0` gates all three axes while registered Saturn build volume is 211.68 x 118.37 x 220 | contained | OPEN |
| T3.2-6 | high | Constants | `rust/geom_kernel/src/queries.rs:3455-3476` | `exterior_face_mask` decides legal support seats from two inline probe offsets (0.05, 0.15 mm) and 4-of-6 vote, no physical input | contained | OPEN |
| T3.2-7 | high | Constants | `crates/plinth-support/src/perception.rs:2370` | A bare `0.2` mm decides whether local minimum is island birth, beside registry-read reach allowance of 0.0707 mm | contained | OPEN |
| T3.2-8 | high | Constants | `crates/plinth-metrics/src/score.rs:775` | The `max_x` hard bound is `value > 12.0` instead of derived `X_ALLOWED`; `plinth-metrics` never reads anchor | contained | SUPERSEDED (3913d50) |
| T3.2-9 | high | Constants | `crates/plinth-hollow/src/drilling.rs:911-916` | H0 preserves authored extra bodies with passing test; H1b refuses any shell with >1 positive component in three places | contained | OPEN |
| T3.2-10 | medium | Constants | `crates/plinth-support/src/specialized.rs:34-35` | `SYNTHETIC_PATCH_RADIUS_MM = 1.5` unregistered island-extent authority; `BIRTH_LINK_RADIUS_MM = 0.6` duplicates registered value | contained | OPEN |
| T3.2-11 | medium | Constants | `crates/plinth-support/src/brace.rs:1967, :1989` | A bare `0.1` selects which rods get exact penetration screen vs only midpoint inside/outside test | contained | OPEN |
| T3.2-12 | medium | Constants | `crates/plinth-support/src/joints.rs:16-18` | Joint existence, coincidence merging and plate-band classification decided by three fixed mm tolerances unrelated to member radii | substantial | OPEN |
| T3.2-13 | medium | Constants | `crates/plinth-support/src/rewire.rs:163` | `Config::current` reads five registered values then refuses any value but hard-coded literal; registry cannot move behaviour | contained | FIXED (242587e) |
| T3.2-14 | medium | Constants | `crates/plinth-support/src/runner.rs:192-256` | ~20 further production resource caps are unregistered literals that can each end a run as refusal | contained | OPEN |
| T3.2-15 | medium | Constants | `crates/plinth-pipeline/src/prepare.rs:1787-1795` | The layer plan adds bare `1.0` mm headroom and literal 1,000-layer minimum, provenance only "carried over verbatim" | contained | OPEN |

---

## Phase 4 — Correctness

All items in this phase; hollowing pair depends on Phase 0.3.

| ID | Severity | Category | File:Line | Description | Effort | Status |
|---|---|---|---|---|---|---|
| SL-3 | high | Correctness | `crates/plinth-hollow/src/drains.rs:1621` | Drain sites searched only around chamber's single global low point; raised foot/limb whose local min sits above band gets no drain, refilled solid (short-list 3) | substantial | OPEN |
| SL-4 | high | Correctness | `crates/plinth-hollow/src/drilling.rs:719, :1078-1085` | The trapped-cavity refill is one relaxation pass; drilling then demands fixpoint — two-step cascade refuses valid shell | contained | OPEN |
| SL-5 | high | Correctness | `crates/plinth-support/src/specialized.rs:2409-2418` | Two different functions define "regular contact set"; production validator compares one against the other | contained | OPEN |
| SL-7 | high | Correctness | `crates/plinth-support/src/perception.rs:2178-2211` | Island-birth overlap test ignores holes; new solid appearing inside cavity reads as overlapping and birth is silently removed | substantial | OPEN |
| SL-11 | medium | Constants | `rust/geom_kernel/src/consts.rs:22` | Two kernel tolerances (`ZDEDUP = 5e-3`, `NEAR_WALL_SKIP = 0.05`) decide support safety; only recorded source is deleted Python file | contained | OPEN |
| SL-12 | medium | Efficiency | `crates/plinth-support/src/runner.rs:7675-7752` | Per-trial coverage measurement re-validates and re-scans superlinearly inside uncapped loop; 2N+1 validations for N contacts | contained | OPEN |
| T3.2-16 | medium | Constants | `crates/plinth-support/src/lib.rs:407-449` | `SupportPrimitive` encodes variant state as public tagged optional fields; invalid combinations representable | substantial | OPEN |
| T3.2-17 | medium | Constants | `crates/plinth-support/src/perception.rs:2565-2596` | Minimum's exterior/interior verdict uses nearest *face centroid* although exact incident-face set built 350 lines earlier | contained | OPEN |
| T3.3-2 | high | Correctness | `crates/plinth-support/src/cco.rs:3506-3568` | `HostCache` ranks cached members by stale three-sample minimum, newer by current midpoint; 25-terminal rebuild cadence decides bifurcation | contained | OPEN |
| T3.3-3 | medium | Correctness | `crates/plinth-support/src/brace.rs:2167-2192` | Model-rung seats generated only from face centroids; coarsely triangulated wall yields no candidate | substantial | OPEN |
| T3.3-4 | medium | Correctness | `crates/plinth-support/src/rewire.rs:837` | `try_retire` commits unconditionally and clamps volume *increase* to zero saving; 20 such repairs satisfy convergence check | contained | FIXED (242587e) |
| T3.3-5 | medium | Correctness | `crates/plinth-support/src/perception.rs:1968-1980` | `IslandBirth::centroid_xy_mm` is unweighted boundary-vertex mean, not area centroid; sampling-dependent, used directly as seed position | contained | OPEN |
| T3.3-6 | medium | Correctness | `crates/plinth-support/src/designed_embed.rs:1566-1610` | Seat attestation digest binds outcomes but no seat geometry; translated-but-different refused candidates produce identical transcripts | contained | OPEN |
| T3.3-7 | medium | Correctness | `crates/plinth-support/src/specialized.rs:8322-8427` | Three hand-maintained hash lists over `SpecializedLimits` disagree; two batch-work coefficients not in any list | contained | OPEN |
| T3.3-8 | medium | Correctness | `rust/chitubox_io/src/project.rs:618, :crates/plinth-emit/src/lib.rs:81` | Neither rename syncs parent directory; on filesystem needing explicit sync, power loss can leave final name absent or pointing at previous artifact | contained | FIXED (ad335d9) |
| T3.4-2 | medium | Numerics | `crates/plinth-support/src/raft.rs:334-352` | Raft pads pass *diameter* in `start_radius_mm` and *height* in `end_radius_mm` on public struct; only one consumer special-cases | contained | OPEN |
| T3.4-3 | medium | Numerics | `rust/geom_kernel/src/moller.rs:13; crates/plinth-mesh/src/triage.rs:1099` | Four Möller ray/triangle implementations across three crates with four determinant guards and three barycentric slacks | substantial | OPEN |
| T3.4-4 | medium | Numerics | `crates/plinth-metrics/src/topology.rs:578` | Degenerate-shell gate is `corrected == 0.0` on accumulated tetrahedron volume; effectively never fires, winding falls to residual sign | contained | SUPERSEDED (3913d50) |
| T3.4-5 | medium | Numerics | `crates/plinth-metrics/src/scar.rs:2122` | `RealInterval` documents as exact real preimage but `norm_interval` and `interval_product` round to nearest; enclosure not sound | contained | SUPERSEDED (3913d50) |
| T3.4-6 | medium | Numerics | `crates/plinth-support/src/perception.rs:2178-2211` | Birth overlap decimates contours to ~40 vertices and ~60 *chords*; verdict changes with tessellation density | contained | OPEN |
| T3.4-7 | medium | Numerics | `crates/plinth-support/src/brace.rs:1994-2006` | Transit acceptance passes literal `8`, `0.12`, `80` to penetration query, unrelated to member scale or process resolution | substantial | OPEN |
| T3.4-8 | low | Numerics | `crates/plinth-support/src/perception.rs:857-935` | `GenericContactPlan::digest` not prefix-free across layers/loops/minima unlike length-prefixed fingerprints beside it | trivial | OPEN |
| T3.5-1 | medium | Efficiency | `crates/plinth-support/src/specialized.rs:688` | `RepairPool::build` takes no `limits` in module whose contract is every batch priced first; O(islands x candidates + rows²) | contained | OPEN |
| T3.5-2 | medium | Efficiency | `crates/plinth-mesh/src/triage.rs:1015` | `classify_batch` is `points.iter().map(classify)`: three rays per point, full linear scan over containers, no BVH | contained | OPEN |
| T3.5-3 | medium | Efficiency | `crates/plinth-mesh/src/topology.rs:534` | Face incidence is `BTreeMap<u32, BTreeMap<usize, BTreeSet<usize>>>`; ~1.29M heap allocations on 429k-face model | contained | OPEN |
| T3.5-4 | medium | Efficiency | `crates/plinth-hollow/src/lib.rs:2665` | Voxel rasterizer is serial per-triangle sweep, crate has no rayon dependency | contained | OPEN |
| T3.5-5 | medium | Efficiency | `crates/plinth-decimate/src/lib.rs:2817, :2899` | Every `probe_two_sided` rebuilds the *unchanged* original mesh's BVH and parity grid | substantial | OPEN |
| T3.5-6 | medium | Efficiency | `crates/plinth-decimate/src/lib.rs:3129, :3188` | Same for exact-bit weld view and quantized positions, rebuilt per candidate ratio | substantial | OPEN |
| T3.5-7 | medium | Efficiency | `crates/plinth-support/src/cco.rs:5671` | `o10_uniform_radius_for_span` builds whole `Forest` and allocates owner `String` per radius query in per-ancestor walk | contained | OPEN |
| T3.5-8 | medium | Efficiency | `crates/plinth-support/src/cco.rs:4631` | Tournament deep-clones entire forest once per descriptor (~90 per terminal); survivor retains clone for rest of round | substantial | OPEN |
| T3.5-9 | medium | Efficiency | `crates/plinth-support/src/cco.rs:4055` | Fresh rayon `ThreadPool` built per terminal and per rewire sweep; `geom_kernel::par` already caches pools | trivial | FIXED (e8caa67) |
| T3.5-10 | medium | Efficiency | `crates/plinth-support/src/cco.rs:3530` | `HostCache::nearest_excluding` is full linear scan plus second full walk plus two allocations, once per terminal | contained | OPEN |
| T3.5-11 | medium | Efficiency | `crates/plinth-support/src/cco.rs:2206, :4389, :4636` | Whole-forest volume delta computed three times per rewire candidate; for bifurcations second value is bit-identical | contained | OPEN |
| T3.5-12 | medium | Efficiency | `crates/plinth-support/src/rewire.rs:643, :868, :932` | Each viable slide/prune/retire clones forest and recomputes volume before/after; `optimize` already tracks running total | substantial | OPEN |
| T3.5-13 | medium | Efficiency | `crates/plinth-support/src/forest.rs:541` | `add_member` scans every slot for reusable one (Θ(N²) to build forest); every geometry mutation linearly scans terminal ledger | contained | OPEN |
| T3.5-14 | medium | Efficiency | `crates/plinth-support/src/coverage_closure.rs:673-687` | Scoring matrix "tiling" permutes visit order without reusing; registered 65,536 ceiling bounds only chunk size | contained | OPEN |
| T3.5-15 | medium | Efficiency | `crates/plinth-support/src/specialized.rs:2922-2942` | Island-birth satisfaction scan meters only owner-matching subset; full O(births x routes) traversal charged nothing | contained | OPEN |
| T3.5-16 | medium | Efficiency | `crates/plinth-support/src/specialized.rs:1586` | `derive_child_from` attributes load rows with `iter_mut().find` where sibling site indexes directly | trivial | OPEN |
| T3.5-17 | medium | Efficiency | `crates/plinth-support/src/perception.rs:2240-2241` | `find_local_minima` allocates one `BTreeSet` and one `Vec` per welded vertex for adjacency only iterated | contained | OPEN |
| T3.5-18 | medium | Efficiency | `crates/plinth-support/src/perception.rs:424-434` | `validate_binding` rebuilds whole `NativeSurface` and re-hashes whole mesh at seven stage boundaries | contained | OPEN |
| T3.5-19 | medium | Efficiency | `crates/plinth-support/src/brace.rs:2167-2196` | Every rung site rescans and re-sorts full exterior face set | substantial | OPEN |
| T3.5-20 | medium | Efficiency | `crates/plinth-support/src/brace.rs:518-524` | Winner scattering restarts `candidates.iter().find` from index zero for every need/site pair | contained | OPEN |
| T3.5-21 | medium | Efficiency | `crates/plinth-support/src/raft.rs:363-390` | `euclidean_mst_edges` re-scans full visited x unvisited cross product each iteration — O(n³) instead of O(n²) | contained | OPEN |
| T3.5-22 | medium | Efficiency | `crates/plinth-support/src/runner.rs:3925-3930` | `validate_integrity` cross-checks contacts against memberships with nested scan although both vectors pushed in lockstep | trivial | OPEN |
| T3.5-23 | medium | Efficiency | `crates/plinth-support/src/runner.rs:4612-4629` | `validate_initial_cco_preflight` scans whole terminal and contact lists per terminal with non-short-circuiting filter | trivial | OPEN |
| T3.5-24 | medium | Efficiency | `crates/plinth-support/src/perception.rs:2366, :2613-2622` | Birth matching re-allocates 65-point sample vector inside inner `births.iter()` loop | trivial | OPEN |
| T3.6-13 | medium | Dead code | `rust/geom_kernel/src/lib.rs:25` | `pub mod testkit` ungated; every release ships synthetic mesh generators, brute-force oracles, two `unwrap`ping constructors | contained | DUPLICATE (T3.6-16) |
| T3.8-3 | medium | Tests | `crates/plinth-support/src/specialized.rs:3916-3932` | `GatePreflightTelemetry` never written; six assertions including allocation-free-refusal test compare 0 against 0 | trivial | OPEN |
| T3.8-4 | medium | Tests | `crates/plinth-pipeline/src/lib.rs:2046-2065` | Test states it must use production layer plan then guarantees it by copying derivation with magic numbers | trivial | OPEN |
| T3.8-5 | medium | Tests | `crates/plinth-decimate/tests/execution_budget_refusal.rs:61-92` | Malformed-budget oracle reproduces production's three predicates and precedence | contained | OPEN |
| T3.8-6 | medium | Tests | `rust/geom_kernel/tests/native_api.rs:981-995` | Sample-budget test recomputes production `min(cap*sub, 3*sub + ceil(seg/step))` instead of pinning reviewed expected value | contained | OPEN |
| T3.8-7 | low | Tests | `crates/plinth-support/src/cco.rs:7822-7834` | Digest-equivalence helper's second assertion is tautology given first; failure message promises check not performed | trivial | OPEN |
| T3.8-8 | low | Tests | `crates/plinth-support/src/rewire.rs:1509` | Five assertions restate constraints constructor already refuses | trivial | FIXED (242587e) |
| T3.8-9 | low | Tests | `crates/plinth-hollow/src/drilling.rs:1121` | `qualifies_audit_drain`'s `added` parameter always 0 in production; "three drain cap" test covers dead limb | trivial | FIXED (cdb61bd) |
| T3.8-10 | low | Tests | `crates/plinth-pipeline/src/lib.rs:1241` | Test named for thread settings discards `[1, 4, 16]` with `|_|` and asserts three identical constructions equal | trivial | OPEN |
| T3.8-11 | low | Tests | `crates/plinth-layers/tests/registry_pin.rs:30` | Test pinning slice quantum to stitch key pins to literal; kernel constant it should compare is public dependency | trivial | FIXED (cdb61bd) |
| T3.9-1 | medium | Errors | `crates/plinth-emit/src/lib.rs:43-56, :202` | Staging save, rename, reopen and metadata failures collapse into `TargetUnavailable` with no `#[source]` | contained | OPEN |
| T3.9-2 | medium | Errors | `crates/plinth-support/src/runner.rs:969, :3643, :6323` | Nine `map_err(\|_\| AuthorityMismatch) / ResourceLimit` sites destroy cause on verification/measurement paths | contained | OPEN |
| T3.9-3 | medium | Errors | `crates/plinth-support/src/perception.rs:985-1003, :1211` | Registry-lookup failure reported as `GenericPerceptionError::Topology`; mesh/kernel/point-index errors stringified | substantial | OPEN |
| T3.9-4 | medium | Errors | `crates/plinth-pipeline/src/gates.rs:448` | "Refused model leaves no path on disk" invariant enforced by `let _ = std::fs::remove_file()` while `clear_stale_artifact` treats same failure as fatal | trivial | OPEN |
| T3.9-5 | medium | Errors | `crates/plinth-pipeline/src/prepare.rs:1930` | Sealed repair ledger written before gate; receipt including `write_error` dropped on every refusal path | contained | OPEN |
| T3.9-6 | medium | Errors | `crates/plinth-support/src/specialized.rs:1472, :1985, :3010` | `DuplicateChildPhysicalSeat` carries candidate id in field named `contact_id`; `UnsatisfiedDemand` carries two index namespaces under one message | trivial | OPEN |
| T3.10-2 | high | Naming | `crates/plinth-hollow/src/drilling.rs:418` | Shipped hollow report collapses every chamber into one `CavityRecord` with `cavity_id: 1`; discards per-lobe identity planner computed | contained | FIXED (a4deda4) |
| T3.10-3 | medium | Naming | `crates/plinth-support/src/coverage_measurement.rs:124, :161` | `patch_area_floor_mm2` is public accessor; runner immediately renames to `censor_area_mm2` | contained | OPEN |
| T3.10-4 | medium | Naming | `crates/plinth-core/src/constants.rs:176-245` | Whole `SupportResourceCapPolicy` accessor family plus registry ids use "floor" for minimum | substantial | OPEN |
| T3.10-5 | medium | Naming | `crates/plinth-support/src/contact.rs:210` | `floor_depth_mm` is seat depth, in context where "floor" has legitimate printed-part meaning | contained | OPEN |
| T3.10-6 | medium | Naming | `crates/plinth-support/src/runner.rs:183, :205-219` | Same vocabulary through runner's cap helpers | contained | OPEN |
| T3.10-7 | medium | Naming | `crates/plinth-orient/src/search.rs:283, :1011` | `BelowFaceFloor { faces, floor }` is *serialized* enum variant; word is now in persisted orientation-report schema | contained | OPEN |
| T3.10-8 | low | Naming | `crates/plinth-support/src/specialized.rs:60, :perception.rs:99, :cco.rs:510, :plinth-mesh/src/delicacy.rs:119, :plinth-pipeline/src/coverage_evidence.rs:309` | Same "floor" drift in five more modules | contained | OPEN |
| T3.10-9 | low | Naming | `crates/plinth-core/src/constants/placeholders.rs:179, :crates/plinth-support/src/cco.rs:1` | "CCO" reaches `plinth-core` crate root; expanded nowhere in 178,000 lines or docs | trivial | FIXED (cdb61bd) |
| T3.10-10 | low | Naming | `crates/plinth-support/src/anchor.rs:1045` | `AnchorSeatDiscovery` and `AnchorFootValidation` are public with public accessors while every producer is crate-private | contained | OPEN |
| T3.10-11 | low | Naming | `crates/plinth-solid/src/lib.rs:204` | Most discoverable public entry (`SolidBoolean` trait) and `BooleanLimits::default()` unbounded; every production caller uses explicitly limited inherent methods | contained | OPEN |
| T3.10-12 | low | Naming | `crates/plinth-core/src/printer.rs:173` | `pixel_pitch_x_um` and `pixel_pitch_y_um` adjacent same-typed positional parameters with no transposition protection | contained | OPEN |
| T3.10-13 | low | Naming | `crates/plinth-cli/src/lib.rs:29` | Three required flags and no positional argument, against recorded target `plinth prepare <folder> --output <printer>` | contained | OPEN |
| T3.10-14 | low | Naming | `crates/plinth-support/src/specialized.rs:1` | "specialized" only meaningful as contrast with "generic"; read alone does not say what is specialized | substantial | OPEN |
| T3.11-1 | medium | Docs | `crates/plinth-support/src/lib.rs:10-11, :168` | Crate doc says "future CCO controller is authority… no public `run` stub" 160 lines above `pub use runner::run_support` | contained | OPEN |
| T3.11-2 | medium | Docs | `rust/geom_kernel/src/consts.rs:1` | Header promises tolerance linked to `geometry_exact.py` source; only `.pyc` remains, nine outcome-affecting numbers have no readable provenance | contained | OPEN |
| T3.11-3 | medium | Docs | `crates/plinth-support/src/specialized.rs:142` | 170 lines of doc on 6-line `const fn`; four paragraphs explicitly retracted, including "treant still refuses at this gate" | contained | OPEN |
| T3.11-4 | low | Docs | `crates/plinth-core/src/constants/evidence.rs:187, :301` | Two citations into files deleted by docs retarget | trivial | FIXED (cdb61bd) |
| T3.11-5 | low | Docs | `crates/plinth-hollow/src/lib.rs:1633` | Traversal cost stated as 13 here and 12 in `well_composed.rs`; `flow.rs` enumerates 11 — two public numbers off by one | trivial | OPEN |
| T3.11-6 | low | Docs | `crates/plinth-core/src/constants.rs:352` | Registry's own doc and `Vec::with_capacity` hint say 287 entries; test-verified count is 306 | trivial | FIXED (cdb61bd) |
| T3.11-7 | low | Docs | `docs/architecture.md:147` | "Every public query takes a slice and returns a slice" false for `knn_single_source_planned_into` among others; rule overstated | trivial | OPEN |
| T3.12-2 | low | Code smell | `crates/plinth-support/src/runner.rs:3748` | Comment says island index reused from preflight; code rebuilds it | trivial | FIXED (2d41082) |
| T3.12-3 | low | Code smell | `crates/plinth-support/src/forest.rs:1294-1314` | Subtree walks recurse without depth bound; `validate()` runs per candidate move, stack overflow on deep chain | contained | OPEN |
| T3.12-4 | low | Code smell | `crates/plinth-pipeline/src/coverage_evidence.rs:91` | `COVERAGE_RENDER_MAX_FACES` bounds the sidecar containing only filenames; roughly 10x low for SVGs it actually bounds | trivial | FIXED (cdb61bd) |
| T3.12-5 | low | Code smell | `rust/geom_kernel/src/bvh.rs:297` | Index-median fallback sorts range comment says is already sorted; reserves `2 * count` for partition totalling `count` | trivial | FIXED (cdb61bd) |
| T3.12-6 | low | Code smell | `rust/geom_kernel/src/queries.rs:778` | Penetration scratch accounting bills stand-in tuple instead of real `CapsuleRay`; same size by coincidence | trivial | OPEN |
| T3.12-7 | low | Code smell | `rust/chitubox_io/tests/common/mod.rs:26` | `optional_treant_dir` falls back to hard-coded personal absolute path when `CHITUBOX_CORPUS_DIR` unset | trivial | DEFERRED (cdb61bd) |
| T3.12-8 | low | Code smell | `crates/plinth-hollow/src/drilling.rs:1642` | Each drill rebuilds whole-grid drainability structure several times; each is full 5.3M-cell scan plus 42 MB throwaway allocation | contained | DUPLICATE (T3.5-25) |
| T3.12-9 | low | Code smell | `crates/plinth-metrics/src/scar.rs:272` | Support-record reconstruction forms complete tips x tapers and members x balls Cartesian products | substantial | SUPERSEDED (3913d50) |
| T3.4-9 | low | Numerics | `crates/plinth-mesh/src/topology.rs:969` | `vertex_count` uses bare `as u32` while every id assigned is `u32::try_from`-checked | trivial | DEFERRED (92a1be1) |
| T3.4-10 | low | Numerics | `rust/chitubox_io/src/project.rs:821` | Append support path uses unchecked `as u32` where Replace path uses crate's checked `u32_of` | trivial | FIXED (cdb61bd) |
| T3.4-11 | low | Numerics | `crates/plinth-support/src/member_sizing.rs:610` | `frustum_volume_ml` divides mm³ by `MILLIMETRES_PER_METRE`; right by coincidence, wrong by dimension | trivial | OPEN |
| T3.4-12 | low | Numerics | `crates/plinth-support/src/runner.rs:5391` | NaN-filled `ArmRefusalPhysics` minted as "no measurement" sentinel; travels into disclosure whose contract is measured millimetres | contained | OPEN |
| T3.5-25 | low | Efficiency | `crates/plinth-hollow/src/drilling.rs:764, :947, :1736` | Each drill rebuilds drainability; `sort_by_key` recomputes index per comparison | contained | OPEN |
| T3.5-26 | low | Efficiency | `crates/plinth-metrics/src/score.rs:271` | Support metric rows re-derive canonical meshes and shell topology that `topology_metric_rows` computed moments earlier | substantial | SUPERSEDED (3913d50) |
| T3.5-27 | low | Efficiency | `rust/geom_kernel/src/point_index.rs:476` | Only query module in kernel with no parallelism, documented as batch-only | contained | OPEN |
| T3.5-28 | low | Efficiency | `rust/geom_kernel/src/parity.rs:678-680` | Three independent axis grids built serially; serial sort of up to 2³² references on construction path every stage blocks on | contained | OPEN |
| T3.5-29 | low | Efficiency | `rust/geom_kernel/src/slicing.rs:213, :259` | Slicing descends BVH twice per plane; allocates five fresh `Vec`s per plane, ~1,000 planes for 50 mm model | contained | OPEN |
| T3.5-30 | low | Efficiency | `crates/plinth-support/src/perception.rs:1098-1108` | No-legal-candidate check is islands x candidates scan where one pass answers every island | trivial | OPEN |
| T3.5-31 | low | Efficiency | `crates/plinth-support/src/runner.rs:3020-3038` | Blocked-cause diagnosis brute-forces rows x uncovered samples; refusal-path only, makes refusals slow | contained | OPEN |
| T3.5-32 | low | Efficiency | `crates/plinth-support/src/seat_zone.rs:494-496` | Only rayon global-pool site, on per-terminal handful of tiny flood fills; defeats `PLINTH_THREADS` sweep convention | trivial | OPEN |
| T3.5-33 | low | Efficiency | `crates/plinth-pipeline/src/prepare.rs:1830` | Every model pays full mesh copy (~12 MB on 496k-face shell) held through peak-allocation; served only on refusal SVG | substantial | OPEN |
| T3.5-34 | low | Efficiency | `crates/plinth-pipeline/src/prepare.rs:859` | Folder loop is strictly serial; crate has no rayon — real trade-off, not free win | contained | OPEN |
| T3.5-35 | low | Efficiency | `crates/plinth-decimate/src/lib.rs:2351` | `SimplifyRequest` owns lock vector; unchanged `protected` slice cloned per search candidate | trivial | FIXED (cdb61bd) |
| T3.5-36 | low | Efficiency | `crates/plinth-orient/src/score.rs:728` | Fresh thread pool per scoring tier, three per model — same root cause as CCO pool churn | contained | FIXED (e8caa67) |
| T3.5-37 | low | Efficiency | `rust/geom_kernel/src/queries.rs:3300` | `cone_thickness` allocates hit vector and direction vector per cone ray; allocation-free traversal six lines away | trivial | OPEN |
| T3.9-7 | low | Errors | `crates/plinth-pipeline/src/coverage_evidence.rs:365` | Serialization failures become empty recovered patches or omitted report fields | contained | OPEN |
| T3.9-8 | low | Errors | `crates/plinth-hollow/src/lib.rs:3854` | `IneligibleSavings` says "largest cavity" for sum over every chamber; `IneligibleBelowBulk` bakes constant value into text | trivial | FIXED (cdb61bd) |
| T3.10-15 | low | Naming | `crates/plinth-core/src/constants/placeholders.rs:1156-1157` | `HOLLOW_BULK_MIN_MM` decides whether model is hollowed; entire recorded derivation is sentence "Hollowing bulk-size policy" | contained | OPEN |
| T3.12-10 | low | Efficiency | `crates/plinth-decimate/src/lib.rs:2082` | Self-declared non-derivable 32,768-face threshold alone refuses otherwise-safe reductions on two corpus models | contained | DUPLICATE (T3.2-18) |
| T3.12-11 | low | Code smell | `crates/plinth-emit/src/lib.rs:588` | Raft pad-versus-bridge dispatched on exact `f64` endpoint equality; correct today because producer passes same value twice | contained | DUPLICATE (T3.2-19) |

---

## Phase 5 — Vocabulary

Renaming for clarity; "floor" means smallest allowed value in nine modules; needs careful treatment.

| ID | Severity | Category | File:Line | Description | Effort | Status |
|---|---|---|---|---|---|---|
| (Phase 5 vocabulary work listed in Phase 4 above; see Task 5 in PLAN.md) | — | — | — | Rename "floor" to "minimum" across accessors, registry ids, serialized enum variants, and census output keys | substantial | n/a |

---

## Phase 6 — Documentation

Comments describe current state, never history.

| ID | Severity | Category | File:Line | Description | Effort | Status |
|---|---|---|---|---|---|---|
| (Covered in Phase 4 above; see T3.11 entries) | — | — | — | Rewrite crate docs, verify citations, fix numeric claims, correct architecture.md, add kernel tolerance provenance | contained + contained | n/a |

---

## Phase 7 — Architecture

After base is correct, tested, and honestly named.

| ID | Severity | Category | File:Line | Description | Effort | Status |
|---|---|---|---|---|---|---|
| T3.1-1 | medium | Architecture | `crates/plinth-verify/src/lib.rs:144-163` | One implemented pre-write gate delegates measurement and predicate to crate it judges | substantial | OPEN |
| T3.1-2 | medium | Architecture | `crates/plinth-emit/src/lib.rs:29-43` | Emit's four semantic handoff types live in implementation crate; inheritance of whole engine's build closure | substantial | OPEN |
| T3.1-3 | medium | Architecture | `crates/plinth-pipeline/src/lib.rs:367-376` | `PrepareRequest` is union of stage-crate types; CLI depends on `plinth-hollow` and `plinth-orient` | substantial | OPEN |
| T3.1-4 | medium | Architecture | `crates/plinth-pipeline/src/coverage_evidence.rs:365` | Coverage evidence recovers `plinth-verify`'s private field shape by round-trip JSON; parse failure replaced with empty patch | contained | OPEN |
| T3.1-5 | medium | Architecture | `crates/plinth-support/src/perception.rs:1538-1581` | General 2D contour stitcher and primitives sit inside support crate; `plinth-layers` copied `signed_area` verbatim | project | OPEN |
| T3.1-6 | medium | Architecture | `crates/plinth-support/src/runner.rs:192-256` | `SupportPreparationLimits::production` reads four caps from registry, restates 34 more as literals | contained | OPEN |
| T3.1-7 | low | Architecture | `crates/plinth-support/src/coverage_repair.rs:987` | `StageClock` (pipeline-wide progress facility) defined inside coverage-repair module | trivial | OPEN |

---

## Standing Cleanup & Backlog

Items not yet assigned to a phase; batched sweeps for efficiency.

| ID | Severity | Category | File:Line | Description | Effort | Status |
|---|---|---|---|---|---|---|
| T3.2-18 | medium | Constants | `crates/plinth-decimate/src/lib.rs:2113` | Self-declared non-derivable 32,768-face threshold refuses reductions | contained | OPEN |
| T3.2-19 | low | Constants | `crates/plinth-emit/src/lib.rs:588` | Raft pad-versus-bridge dispatched on exact `f64` endpoint equality | contained | FIXED (ad335d9) |
| T3.3-9 | medium | Correctness | `rust/geom_kernel/src/mesh.rs:244` | `Mesh::from_indexed` divides by `length.max(1e-30)` with no degenerate-face error; zero normal stored | contained | FIXED (2333009) |
| T3.3-10 | medium | Correctness | `rust/geom_kernel/src/segment_batch.rs:225` | `point_segment_distances_batch` can only return dense P x S matrix; caller hand-rolled scalar loop and duplicate epsilon | contained | FIXED (242587e) |
| T3.3-11 | low | Correctness | `rust/geom_kernel/src/queries.rs:2220, :2326` | `segment_clearance` returns `INFINITY` as "not measured" sentinel; caller reads as pass | contained | OPEN |
| T3.5-38 | medium | Efficiency | `crates/plinth-support/src/transit_measurement.rs:1011` | `measure_scar` resolves members by linear search, three times; preflight run twice | contained | SUPERSEDED (5d0420e) |
| T3.6-14 | low | Dead code | `crates/plinth-pipeline/src/lib.rs:2047` | 400-line fixture whose oracle is error-message string equality; re-recorded eight times | contained | OPEN |
| T3.6-15 | low | Dead code | `rust/chitubox_io/src/project.rs:821, :crates/plinth-solid/vendor/manifold/FILES.sha256:1` | Vendored Manifold checksum inventory is inert; never read or compared | contained | FIXED (cdb61bd) |
| T3.6-16 | low | Dead code | `rust/geom_kernel/src/lib.rs:25` | `pub mod testkit` ships synthetic mesh generators and brute-force oracles | contained | FIXED (dc2e0d0) |
| T3.10-16 | low | Naming | `rust/geom_kernel/src/lib.rs:22` | `V3` exported but `mod vec3` operations not; three `plinth-support` modules re-implement | trivial | DUPLICATE (T1.4) |
| T3.11-8 | low | Docs | `crates/plinth-support/src/lib.rs:11` | Crate doc drift; future controller claim contradicted 160 lines below | contained | DUPLICATE (T3.11-1) |
| T3.12-12 | low | Code smell | `rust/geom_kernel/src/par.rs:23, :74` | `PLINTH_THREADS` is documented runtime contract with no validation; pool failure hits expect in library code | substantial | OPEN |

---

## Summary

**Total findings extracted: 189** (organized into all phases and backlog)

**Note on count discrepancy:** The review header states 207 verified findings, while the methods section states 203 survived verification. Systematic extraction from the short list and all theme tables yields 189 deduplicated findings organized above. The discrepancy may reflect findings reported multiple times in different contexts or additional summary-level counts in the review not captured in the structured sections.

**All line numbers accurate as of commit 041ba6d** and will drift as code is modified. Use file path and finding description to locate items after changes.

**Organized by phase from PLAN.md:**
  - Phase 0 (Make change safe): 3 findings
  - Phase 1 (Free wins): 15 findings  
  - Phase 2 (Delete what nothing runs): 9 findings
  - Phase 3 (One home per fact): 32 findings
  - Phase 4 (Correctness): 112 findings
  - Phase 5 (Vocabulary): 1 finding
  - Phase 6 (Documentation): 1 finding
  - Phase 7 (Architecture): 7 findings
  - Standing cleanup backlog: 9 findings
