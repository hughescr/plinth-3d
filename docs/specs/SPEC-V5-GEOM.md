# SPEC-V5-GEOM — Exact-Geometry Core (v5)

Status: APPROVED FOR IMPLEMENTATION (branch `v5-geom-core`, from master f02b7af).
Authority: Craig's binding ruling — "All we care about is surface, not volume...
Point-cloud is not gonna work!" — plus the measured evidence in
`docs/artifacts/audit-2026-07/penetration_census.md`: the delivered full-scale
forest contains **48 real support-vs-model crossings (44 ball breaches + 4 member
penetrations), all at features < 1mm thick, 32/44 at < 0.3mm**, while the
engine's own sampled clearance check reported **zero** violations. The sampled
surface point cloud (~0.3–0.6mm spacing) is structurally blind to sub-sample
features. This spec evicts the cloud from every load-bearing geometric decision
and replaces it with surface-exact queries against the triangle mesh.

Reference math to reuse (proven on the census, 100% heavy-9-ray agreement):
`docs/artifacts/audit-2026-07/tools/census_core.py` (MeshIndex, ray_crossings,
closest_on_tri, VerticalParity, ParityVote) and `census_run.py` (penetration
intervals, antipodal-cone ball-breach test).

---

## 0. Design principles (binding)

- **Surface-exact**: every load-bearing distance/intersection/inside query is
  answered against the actual triangles, never a sampled proxy of them.
- **Inside/outside methodology** (hard-won, from the census): nearest-face
  signed-distance sign is UNRELIABLE at thin features (over-reported
  penetrations ~3x). Boundary locations come from exact Moller–Trumbore edge
  crossings; inside/outside classification comes from a multi-axis ray-parity
  MAJORITY vote (3 axes, majority >= 2). The mesh is triangle soup (unmerged
  vertices) but winding-consistent — parity works, topology-dependent methods
  do not.
- **Sanctioned embedding is semantics, not slop**: stub tips deliberately sit
  `contact_depth_mm` (0.3mm) inside the surface; strut/rung model anchors
  deliberately embed at their designed end. The exact checker ALLOWS
  end-embedding inside the derived inset zone and FORBIDS (a) mid-path
  penetration anywhere else and (b) thin-feature ball breach (a ball engulfing
  a sub-mm twig and poking out the far side).
- **Correctness and resin economy never trade against wall-time.** Honest
  runtimes are reported; the `t_total` pins in tests are regression tripwires
  only and get re-baselined with a comment if the exact core legitimately
  exceeds them. Making exact queries FAST (vectorize, batch, crop, tier) is an
  engineering requirement, not a correctness tradeoff.
- **Swappable kernel**: all exact queries live behind one class with a clean
  API; a native (C/Rust/embree) kernel may later replace its internals with
  zero call-site changes.
- **Memory discipline** (a prior agent OOMed the machine at 173GB): hard 2GB
  RSS self-check (`resource.getrusage`) inside every long loop; ray/query
  batches <= 2048; triangle candidate sets cropped locally per query; NEVER
  build query-count x all-576k-faces arrays.

---

## 1. The exact-query module: `scripts/support_placement/geometry_exact.py`

One class, built once per mesh, passed everywhere `surface_tree` flows today.

```python
class ExactSurface:
    """Surface-exact geometric oracle over a triangle mesh.

    Internals (numpy/scipy, ported from census_core.py) are replaceable by a
    native kernel behind this exact API. All batch methods enforce internal
    batching (<= 2048 queries per block) and a 2GB RSS assert.
    """

    def __init__(self, mesh, grid_pitch_mm=2.0):
        # - V0/E1/E2 triangle arrays (census MeshIndex), float64, copied once
        # - centroid cKDTree + per-triangle extent; `large` never-miss list
        #   (extent > 1.5mm) appended to every candidate crop
        # - three VerticalParity grids (ax=0,1,2) for the parity vote
        # - exterior-face mask computed lazily on first use (sec 2.4)

    # ---- cKDTree-compatible facade (drop-in for surface_tree.query) ----
    def query(self, pts, distance_upper_bound=inf, workers=None):
        """EXACT unsigned point-to-surface distance, (dist, tri_idx) --
        signature-compatible with cKDTree.query so passive consumers
        (removal.py diagnostics, connectivity_report) swap by duck-typing.
        Points farther than distance_upper_bound MAY return inf (pruning),
        same contract as cKDTree. Implementation: k-nearest centroids
        (k=64) + census `large` list -> closest_on_tri, batched."""

    # ---- load-bearing primitives ----
    def project(self, pts):
        """(dist, closest_point, tri_idx, face_normal) -- exact on-surface
        projection (census SignedTester.nearest, WITHOUT trusting the sign)."""

    def inside(self, pts):
        """bool[N] -- 3-axis ray-parity MAJORITY vote (census ParityVote).
        The ONLY sanctioned inside/outside classifier."""

    def cast_rays(self, origins, dirs, t_max):
        """Batched Moller-Trumbore against KDTree-cropped candidates:
        sorted crossing t-values per ray (census ray_crossings + dedup_loc).
        Candidate crop per ray from segment-midpoint ball query; batches
        <= 2048 rays; never rays x all-faces."""

    def segment_clearance(self, p0s, p1s, r0s, r1s, standoff,
                          inset0s=None, inset1s=None):
        """Per segment: min over the sub-segment [inset0, L-inset1] of
        (exact_capsule_distance - r(s) - standoff), plus argmin point.
        EXACT segment-vs-triangle minimum distance (not path sampling):
        min over (a) endpoint-to-triangle closest_on_tri, (b) segment-vs-
        each-triangle-edge segment/segment distance, (c) 0 if the segment
        pierces the triangle interior (Moller). Linear taper r(s) is
        handled by solving the tapered-capsule minimization per candidate
        triangle in the same vectorized pass (conservative fallback:
        use max(r over the sub-segment) if the tapered solve is deferred
        to a follow-up -- conservatism may only over-flag, never under-flag).
        Two-tier for speed (sec 5): a conservative centroid-KDTree reject
        certifies far-from-everything segments without the exact pass."""

    def penetration_intervals(self, p0, p1, r0, r1, n_offset_rays=8,
                              majority_step_mm=0.12, cap=80):
        """Census PASS A-C, per member: axis + n capsule-offset rays;
        Moller crossings partition each ray; each sub-segment classified
        inside/outside by dense-majority parity vote of its interior
        samples; maximal inside runs -> intervals [(s_a, s_b, entered,
        exited, max_depth_mm)] in arc length. EXACT boundary locations;
        parity (not signed distance) for side-of-surface."""

    def sphere_breach(self, centers, radii, eps_mm=0.05):
        """Census ball test: for spheres that reach the surface
        (r - project(c).dist > eps), measure local feature thickness along
        the inward-normal +/-25deg cone (9 Moller rays, skipping the near
        wall); breach iff thickness < r - eps. Returns per-ball
        (breached, feature_thickness_mm, poke_out_mm)."""

    def exterior_face_mask(self):
        """bool[n_faces], cached: face f is EXTERIOR iff
        inside(centroid_f + eps * normal_f) is False (eps = 0.05mm).
        A face interior to the solid union (the STL is a non-watertight
        overlapping assembly) has solid material on BOTH sides of it, so
        the parity vote reports its outward offset point inside. Batched
        through inside() in <= 2048-point blocks."""
```

Construction and caching: `engine.run` builds ONE `ExactSurface` (replacing
`routing.build_surface_tree` at engine.py:242) and threads it through every
call path that today takes `surface_tree`. Like the cKDTree it replaces, it is
NOT pickled with checkpoints (tree.py:194-208 convention holds): staged/
multi-process runs rebuild it per process from the mesh. Build cost budget:
triangle arrays + centroid tree + 3 parity grids ~ a few seconds at 576k faces
(census measured the same structures in seconds).

Naming at call sites: the parameter keeps flowing under the name the code
already uses (`surface_tree` renamed to `geom` opportunistically, no big-bang
rename required — duck-typing via `.query` makes both spellings work during
the swap).

---

## 2. Consumer-by-consumer swap plan

The two independent clearance implementations stay INDEPENDENT (verify.py:44's
"a builder bug ... must not also fool the verify gate" rule): both may call
`ExactSurface`'s generic primitives (exactly as both call `surface_tree.query`
today), but each derives its own inset/exemption logic separately.

### 2.1 Clearance implementation #1 — `tree.Forest.revalidate` (tree.py:281-418)

- The batched sampled query (tree.py:376-400: linspace at
  `clearance_sample_step_mm` + `surface_tree.query`) is replaced by
  `geom.segment_clearance` over the dirty non-stub members, with per-member
  insets computed exactly as today's formula MINUS the pitch term:
  **inset = contact_depth_mm + r_end + standoff_mm** (tree.py:378-383).
  `pitch_bound_mm` is identically 0 for an exact oracle — the entire
  derivation at tree.py:303-358 collapses because there is no approximating
  cloud to budget for. `Forest._surface_pitch_bound_mm` (tree.py:420-457) and
  its cache die on this path.
- **Stub members get a NEW check** instead of the blanket skip (tree.py:368):
  the standoff exemption stays (a stub hugs its own emergence surface by
  construction — any standoff check false-positives), but the census found 3
  illegal stubs including the one through-and-through (census.md worst
  offenders: members 403/357/227). New rule: a stub fails certification iff
  `penetration_intervals` on its axis finds an inside interval of illegal
  length > 0.05mm (census MIN_ILLEGAL_LEN) outside its sanctioned p0 embed
  zone (arc length < contact_depth_mm + r0 + standoff slack as derived for
  the embed inset). This is a crossing/parity test, not a distance test, so
  it cannot false-positive on ordinary surface-hugging stubs.
- Non-stub members additionally get the mid-path penetration test: an inside
  interval outside the embed insets fails the member even when the standoff
  distance check passes (grazing-tangent transits). In practice
  `segment_clearance < 0` already catches nearly all of these; the interval
  test is the belt for exactly the census's through-transit class.
- Call sites unchanged: cco.py:288,301; rewire.py:289,302,362,395,537,633,733;
  bracing.py:853,962,981; engine.py:145,168 keep calling
  `forest.revalidate(geom, cfg)` verbatim.

### 2.2 Clearance implementation #2 — `verify.clearance_report` (verify.py:44-140)

- Its own sampling loop (verify.py:98-129: linspace + insets at 110-115 +
  `routing._surface_distances` at 129) is replaced by its OWN calls to
  `geom.segment_clearance` + `geom.penetration_intervals`, with its OWN
  independently-written inset computation (same formula, contact_depth + r_end
  + standoff, pitch term gone: verify.py:94-96 deleted). The independence
  covenant (verify.py:70-87) carries over verbatim: share the generic
  primitive, never the masking/exemption logic.
- Adds the same stub through-penetration rule as 2.1 (independently coded).
- **NEW: `verify.ball_breach_report(geom, balls, cfg)`** — the census's
  headline defect class (44 breaches) is currently checked by NOTHING. For
  every joint ball from `joints.compute_joint_balls`, run
  `geom.sphere_breach`; a ball spanning a feature thinner than its radius
  (poke-out > 0.05mm) is a violation. Sanctioned shallow anchors into thick
  surface (census: 150 of 194 surface-reaching balls) pass by construction —
  the test measures far-wall thickness, not surface contact. Wired into
  `self_verify` (verify.py:391-487) as a new report key
  `ball_breach_violations`, ANDed into `ok` (this is a hard gate, same class
  as clearance).
- The builder side must also stop CREATING breaching balls: cco model-anchor
  and bracing model-rung candidate trials add a `geom.sphere_breach` check on
  the anchor-end ball radius during candidate scoring (cheap: one ball per
  candidate), so a thin-twig anchor is rejected and the trial machinery moves
  to the next candidate — verify's gate is then a true independent re-check,
  not the first line of defense.

### 2.3 Model anchors — real ray/mesh intersection

- `struts.find_model_anchor_candidates` (struts.py:22-72) currently selects
  from `sample_points` = perception.delicacy_field's seeded random cloud
  (perception.py:496, threaded via engine.py:68 -> cco.build sample_points,
  cco.py:157/278/321; also rewire.py:845/876 and bracing.py:218/246/271/347).
  Replacement: the candidate POOL becomes **exterior-face centroids**
  (deterministic, seed-free, on-surface by construction): faces from
  `geom.exterior_face_mask()`, delicacy weight looked up per face (sec 2.5).
  The qualification mask (below-contact, length band, lean cone, delicacy
  cap — struts.py:64-67) is unchanged, evaluated vectorized over centroids
  cropped to `strut_max_length_mm` via a KDTree ball query on centroids.
- Each surviving candidate's anchor point is then made EXACT:
  `geom.cast_rays(stub_end, dir_to_candidate)` — the anchor is the first
  crossing at t in (0, 1+eps] whose hit face is exterior; no hit => candidate
  discarded. The strut foot therefore lands ON the true surface (not near a
  cloud point), and the embed inset in 2.1/2.2 is honest by construction.
  `cco._model_anchor_builders` / `_make_model_anchor_builder` (cco.py:407-448)
  consume the exact anchor; `_within_cone` and pricing unchanged.
- `bracing._find_model_rung_anchors` (bracing.py:748-779) currently draws
  anchors FROM THE CLOUD ITSELF (`surface_tree.data`, bracing.py:766).
  Replacement: candidate pool = exterior-face centroids within
  `model_brace_max_len_mm` of the axis point and |z - z_rung| <= 0.75
  (same mask, bracing.py:772), each snapped to the exact surface via
  `geom.project` (and kept only if its projection stays in the z-band);
  ordering (distance, index) preserved for determinism.

### 2.4 Perception demand seeding — interior-face filter + true-surface guarantee

The STL is a non-watertight overlapping assembly: ~40% of "unrouted" contacts
were phantoms on faces INTERIOR to the solid union (audit finding). Fix at the
source so no downstream stage ever sees interior demand:

- `perception.overhang_islands` (perception.py:106-142): the need mask
  (perception.py:116, `down & ~grounded`) additionally requires
  `geom.exterior_face_mask()`. Interior faces can neither found an island nor
  join one.
- `placement.sample_candidates` (placement.py:11-64): islands now contain only
  exterior faces, so its dense per-island `sub.sample` (placement.py:37) draws
  positions that are exactly on true, exterior surface (barycentric points of
  exterior triangles). The blue-noise selection itself REMAINS a seeded,
  deterministic placement heuristic — it chooses WHERE to put contacts, it is
  not a geometric measurement, and its output points are exact surface points.
  (Recorded verdict, sec 2.6.)
- `engine._ensure_minima_contacts` / `_ensure_island_birth_contacts`
  (engine.py:319-382, 481-571): their nearest-face lookups
  (`centers_tree.query_ball_point`) are restricted to exterior faces (build
  the centers KDTree over exterior faces only). `perception.local_z_minima`
  and `perception.island_births` operate on the full mesh (a slice birth is a
  real print event regardless of which sub-body owns the face) — unchanged,
  except birth-contact normals are taken from the (exterior) nearest face.
- `perception.delicacy_field` (perception.py:474-563): sampling restricted to
  exterior faces; retains its seed for the SCORING field only (sec 3).

### 2.5 Seed-dependent volumes — `placement.load_partition` (placement.py:80-110)

The Monte-Carlo `sub.sample` partition (placement.py:102) makes carried areas
— hence tip radii, hence member volumes — seed-dependent. Replacement: exact,
deterministic per-face assignment: each island face's centroid is assigned to
its nearest contact (cKDTree over the island's contacts), and the face's full
`mesh.area_faces` value accrues to that contact. No RNG, no sampling; exact
at face granularity (faces are small vs. `target_spacing_mm` on this corpus).
Same signature, `n_dense_samples`/`seed` parameters retired.

### 2.6 Inventory of remaining `build_surface_tree` / `.sample` consumers

| Consumer (file:line) | Load-bearing? | Disposition |
|---|---|---|
| tree.Forest.revalidate query (tree.py:399) | **YES** | -> geom.segment_clearance (2.1) |
| verify.clearance_report (verify.py:129) | **YES** | -> exact, independent (2.2) |
| verify.connectivity_report embed check (verify.py:369-373) | **YES** (attachment gate) | -> geom.query (exact point distance), threshold formula unchanged |
| cco model anchors via struts + sample_points (cco.py:278,321; struts.py:22-72) | **YES** | -> exterior-centroid pool + exact ray anchor (2.3) |
| bracing._find_model_rung_anchors surface_tree.data (bracing.py:766) | **YES** | -> exterior-centroid pool + geom.project (2.3) |
| perception islands / demand seeding (perception.py:116; placement.py:37) | **YES** | -> exterior-face filter; positions already on-surface (2.4) |
| placement.load_partition (placement.py:102) | **YES** (sizes tips) | -> deterministic face-area partition (2.5) |
| engine.run build_surface_tree (engine.py:242) | **YES** (source of cloud) | -> ExactSurface(mesh) |
| routing.surface_tree_pitch_bound_mm (routing.py:121-174) + cfg.surface_pitch_bound_percentile + Forest._surface_pitch_bound_mm (tree.py:420-457) | was load-bearing | RETIRED from exact paths (pitch uncertainty = 0); function kept only while any cloud consumer below survives |
| removal.py (_surface_distances at 269, 318; report at 369) | **NO** — diagnostic-only per Craig's 2026-07-22 ruling (removal never gates) | works unchanged via geom.query duck-typing (routing._surface_distances passes through); exactness is a free upgrade, no logic change. TODO: retire its pitch/slop terms in a follow-up |
| routing.check_clearance / check_clearance_batch / sample_route_envelope / _surface_distances (routing.py:189-320) | **NO** live consumer in the v4 engine path (verify/tree reference them in docstrings only; removal uses _surface_distances which duck-types) | keep for tests/legacy; _surface_distances accepts ExactSurface via .query; mark check_clearance* deprecated-in-docstring |
| trunks.py _column_clear/_segment_clear + braces.py (trunks.py:35-65 etc.; braces.py:194,296) | **NO** — retired v3 policy modules, not imported by the v4 engine (engine.py:5, bracing.py:4-5) | untouched; their tests keep building a local cloud via build_surface_tree (which survives as a deprecated test/legacy helper) |
| verify.orientation_scar_cost (verify.py:490-516) | **NO** — orientation scoring only | unchanged |
| perception.delicacy_field sample (perception.py:496) | **NO** as a SCORING field (weights rank regions) — its former load-bearing role as the anchor POSITION pool is removed by 2.3 | keep seeded sampling for weights, restricted to exterior faces |
| placement.sample_candidates blue-noise (placement.py:37) | placement heuristic; positions exact-on-surface after 2.4 | keep, seeded, documented |
| perception.grounded_mask rays (perception.py:75) | YES but already ray-exact (trimesh ray backend) | optionally route through geom.cast_rays for backend independence; not required for v5 |

Explicitly deferred: `bracing.py:762` `if surface_tree is None: return []` and
similar None-guards keep their meaning with `geom is None`.

---

## 3. Determinism statement

- Zero RNG in any load-bearing geometric decision: clearance, penetration,
  ball breach, anchors, interior classification, and load partition are all
  exact functions of (mesh, cfg, contact set). No wall-clock dependence.
- Parity votes use fixed axis rays and fixed grid bucketing — deterministic.
- Remaining seeded sampling (documented, scoring/heuristic only):
  `delicacy_field` (weights), `sample_candidates` (blue-noise placement).
  Both already seed the global numpy state explicitly (seed=0) and stay
  reproducible run-to-run; neither feeds a geometric measurement anymore.
- Ties broken by index everywhere (existing convention, e.g. struts.py:50-51),
  preserved in the new centroid pools.
- Exact queries replace sampling — keep it that way: no new `.sample` calls
  may enter a load-bearing path (add a test-side grep tripwire if cheap).

---

## 4. Baseline re-freeze (pre-approved)

Fixture volume baselines WILL shift: exact clearance rejects candidates the
cloud waved through, interior-phantom demand disappears, and deterministic
load partition changes carried areas. Per Craig's pre-approval:

1. Land the core with all logic tests green.
2. Run the table + toed-slab fixtures on the new engine twice (determinism
   proof: byte-identical volume reports), inspect deltas for sanity, then
   freeze `scripts/tests/fixtures_v5_baseline.json` from the honest engine.
3. `test_table_and_toed_slab_full_run_not_worse_than_baseline`
   (test_engine.py:386-425) keeps its EXACT gate formula
   (`total_ml <= frozen + 1e-6`) pointed at the new file; the v4 file is kept
   for provenance, same as fixtures_v4_p3 was.
4. Wall-clock pins (test_engine.py:43,116; currently t_total < 20s) are
   re-baselined by measurement with the standing comment ("regression tripwire
   only, per Craig's correctness-over-speed ruling") if the exact core
   legitimately exceeds them. Never tune quality to fit a clock.

---

## 5. Performance strategy + projected full-scale runtime

Hot path analysis: `Forest.revalidate` is called from cco's insertion trial
loop up to tens of thousands of times per Treant run, mostly on small dirty
sets, mostly in open space. Strategy:

- **Tier-1 conservative reject** (the workhorse): for a segment batch, query
  the triangle-CENTROID cKDTree at path points (step = clearance_sample_step_mm)
  with `distance_upper_bound = r + standoff + tri_extent_global`, where
  tri_extent_global is the max per-triangle centroid-to-vertex extent (census
  MeshIndex). If no centroid falls within the bound, NO triangle can be within
  r + standoff (any such triangle's centroid would be within r + standoff +
  its own extent) — the segment is certified clear with zero exact work. This
  is a sound bound, not an approximation: it can only send MORE work to tier
  2, never hide a violation. Oversized triangles are on the census `large`
  never-miss list, checked exactly every time.
- **Tier-2 exact** only for segments tier 1 cannot certify: crop candidates
  by ball query around sub-segment midpoints (split long members ~5mm so crop
  radii stay local), then vectorized exact segment-vs-triangle distance +
  Moller crossings + parity on the (few) interval midpoints.
- Batching: one revalidate call = one tier-1 batch + one tier-2 batch across
  all dirty members (same shape as today's single concatenated query). Query
  blocks <= 2048; RSS assert < 2GB in every loop.
- Ball breach checks batch all balls in one pass (1102 balls = ~10k Moller
  rays on cropped candidates — the census did this in seconds).
- One-time costs per run: ExactSurface build (~seconds), exterior_face_mask
  (576k faces = 576k parity points in 282 blocks — the census ran parity on
  comparable point counts in seconds-to-minutes; cache it on the instance).
- **Projection**: current full-scale single-core run ~194s with the cloud.
  The census's full exact sweep (1625 members x 9 rays, dense parity +
  distance on the whole delivered forest) completed in single-digit minutes
  single-core. Engine-integrated projection: tier-1 absorbs the open-space
  majority at KDTree cost (comparable to today), tier-2 exact work is bounded
  by near-surface trials. Honest estimate: **~400–900s (7–15 min) single-core
  full Treant**, worst case low tens of minutes — comfortably inside the
  overnight-usable bound, far from it in expectation. Report the measured
  number in the run log (`t_total`, plus new `t_geom_build`,
  `t_exterior_mask`); re-baseline pins per sec 4.

---

## 6. Test plan

Unit tests in `scripts/tests/test_geometry_exact.py` + surgical additions to
existing suites. All fixtures deterministic, no unseeded sampling.

1. **Thin-twig ball breach (the census worst case, ball 715 class)**: fixture
   mesh = a 0.08mm-thick twig (two long thin boxes / a thin plate) + a ball of
   r = 1.0 centered on its surface. `geom.sphere_breach` MUST flag it
   (poke-out ~0.9mm). `verify.ball_breach_report` MUST fail `ok`. Conversely a
   ball of r = 1.0 anchored on a 5mm-thick slab MUST pass (sanctioned shallow
   anchor — 150/194 census class).
2. **Sanctioned stub embed**: a stub with p0 = contact_depth 0.3mm inside a
   thick slab, exiting along the normal: revalidate + clearance_report MUST
   certify it clean. A stub driven THROUGH a 0.5mm fin (entering and exiting,
   census member 357 class) MUST be flagged by the penetration-interval rule
   with > 0.05mm illegal length.
3. **Strut end-embed vs mid-path clip**: a strut whose p1 embeds in the model
   (embed_end="p1") passes when only its inset zone touches; the same strut
   re-aimed so its MID-PATH clips a 0.2mm fin (finer than any old cloud pitch)
   MUST fail — this is exactly the case the sampled check measured 0 on.
4. **Parity vote on a non-watertight two-box assembly**: two overlapping
   axis-aligned boxes (triangle soup, unmerged vertices, winding-consistent).
   Assert `inside()` classifies: points in either box's interior TRUE, points
   in the overlap TRUE, outside points FALSE, and — the killer for signed
   distance — points just outside a face of box A that lies INSIDE box B are
   TRUE (signed-distance-by-nearest-face gets these wrong). Assert
   `exterior_face_mask` marks box A's faces inside B as interior and all
   outer-shell faces exterior.
5. **Interior demand phantom kill**: same two-box fixture with the buried
   face down-facing: overhang_islands on v5 MUST produce no island on the
   interior face; contact count drops accordingly (pin the count).
6. **Exact segment distance correctness**: segments vs a single triangle in
   the closed-form-checkable positions (endpoint nearest, edge nearest, plane
   piercing => crossing at exact t), asserted to 1e-9.
7. **Deterministic load partition**: two runs of load_partition => identical
   arrays (no seed argument left); carried areas sum exactly to island areas.
8. **Anchor exactness**: model-strut anchor returned by the 2.3 path lies on
   a mesh triangle to 1e-6 and its face is exterior; anchor ray with no
   exterior hit yields no candidate.
9. **Duck-type facade**: geom.query vs brute-force exact distances on random
   fixed points (seeded fixture geometry, exact assertion); removal.py and
   connectivity_report pass unchanged when handed an ExactSurface.
10. **Micro-benchmark** (reported, not gated): query throughput of
    geom.query / segment_clearance vs the old cloud path
    (`build_surface_tree` + cKDTree.query) on the table fixture at matched
    batch sizes; plus tier-1 reject rate on a real revalidate trace. Printed
    via the existing timing-log convention so regressions are visible without
    gating on wall time.
11. **Full-suite regression**: 329 passed / 4 skipped on master must stay
    green modulo the re-frozen baselines (sec 4) and pin re-baselines.

---

## 7. Out of scope (recorded)

- **Member-vs-member clearance** (support self-intersection beyond the
  existing junction rules): stays OUT of v5. TODO recorded here — the
  ExactSurface capsule primitives are the natural substrate when it lands.
- **Native kernel** (C/Rust/embree BVH): the ExactSurface API is the swap
  boundary; internals only.
- **removal.py exact migration** beyond the free duck-typed upgrade
  (diagnostic-only per Craig's ruling; its slop terms keep their meaning).
- **Legacy v3 modules** (trunks.py, braces.py, routing.check_clearance*):
  test-only survivors, untouched.
- **grounded_mask / island_births ray backend unification** onto
  geom.cast_rays: optional cleanup, not required for correctness.
- Dynamic re-check of `_ISLAND_BIRTH_LINK_R_MM` at other layer heights
  (pre-existing flag, engine.py:405-411) — unchanged by this spec.
