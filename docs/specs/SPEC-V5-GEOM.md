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
    batching bounded on the QUERY x CANDIDATE-TRIANGLE PRODUCT (see below)
    and a 2GB RSS assert.

    Memory batching contract (amended per adversarial review, point 2): a
    2048-query cap alone does NOT bound memory -- the exact kernels
    (closest_on_tri and friends) materialize O(queries x candidates)
    intermediate arrays. Every batch method sizes its blocks adaptively:
    block_queries = min(2048, ROW_BUDGET // max(1, est_candidates_per_query))
    with ROW_BUDGET ~ 4M rows (~1.5GB peak across the ~15 float64 (M,3)
    intermediates), est from the actual crop sizes of the block being built
    (ragged crops concatenated, never padded to the max). The 2GB RSS assert
    remains the hard backstop in every loop.
    """

    def __init__(self, mesh, grid_pitch_mm=2.0):
        # - V0/E1/E2 triangle arrays (census MeshIndex), float64, copied once
        # - centroid cKDTree + per-triangle extent. `large` never-miss list
        #   appended to every candidate crop. large_thr is DERIVED PER MESH at
        #   build time (amended per review, points 1+10), not inherited from
        #   the census constant: measure the extent distribution and pick the
        #   smallest threshold keeping len(large) <= ~2000 (so the never-miss
        #   set stays cheap) -- report both large_thr and len(large) in the
        #   build log. Two extents are kept:
        #     tri_extent_global   = ext.max()            (crop radii only)
        #     tri_extent_nonlarge = ext[ext <= large_thr].max()  (Tier-1 bound)
        # - three VerticalParity grids (ax=0,1,2) for the parity vote
        # - exterior-face mask computed lazily on first use (sec 2.4)

    # ---- cKDTree-compatible facade (drop-in for surface_tree.query) ----
    def query(self, pts, distance_upper_bound=inf, workers=None):
        """EXACT unsigned point-to-surface distance, (dist, tri_idx) --
        signature-compatible with cKDTree.query so passive consumers
        (removal.py diagnostics, connectivity_report) swap by duck-typing.
        Points farther than distance_upper_bound MAY return inf (pruning),
        same contract as cKDTree. Implementation: k-nearest centroids
        (k=64) + `large` list -> closest_on_tri, batched, PLUS an exactness
        certificate (amended per review, point 8 -- plain k-NN is only
        near-exact for triangles in the sub-large_thr band whose centroid
        ranks beyond k): let d_k = distance to the k-th nearest centroid;
        the answer is certified exact iff best_dist + tri_extent_nonlarge
        <= d_k (no un-examined non-large triangle can beat best_dist, and
        large triangles are always in the candidate set). Queries failing
        the certificate escalate (k doubled, or centroid ball query at
        best_dist + tri_extent_nonlarge) until certified. `query` is
        therefore exact by construction, at adaptive cost."""

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
        """bool[n_faces], cached. A face interior to the solid union (the
        STL is a non-watertight overlapping assembly) has solid material on
        BOTH sides of it, so an outward offset point tests inside.

        Robust classification (amended per review, point 3 -- a single
        centroid offset at a fixed eps is the riskiest new behavior in this
        spec, because a mis-vote can silently DELETE real support demand):
        - MULTIPLE offset samples per face, not one: 3 interior barycentric
          points (centroid + two off-center points, fixed barycentric
          coords) x 2 offsets eps in {0.05, 0.15} mm along +normal = 6
          parity-vote samples per face; face is INTERIOR iff a strict
          majority (>= 4/6) of samples test inside. This mirrors the
          census's dense-majority discipline (census_run.py PASS B/C):
          never a keep/drop decision from a single parity point.
        - eps is VALIDATED, not assumed: at build time, measure the
          inter-shell gap distribution diagnostically (distance from a
          sample of face centroids to the nearest OTHER-side triangle) and
          log it; if a material fraction of gaps falls below 0.15mm the
          eps ladder is re-derived from the measurement before trusting
          the mask.
        - FAIL-OPEN bias: ties and ambiguous faces classify EXTERIOR.
          Phantom demand on a rare mis-kept interior face costs resin (the
          v4 status quo everywhere); a mis-dropped exterior overhang costs
          the print. Anchor-side safety does not rest on the mask alone --
          anchors additionally pass exact ray casting and sphere_breach
          (sec 2.2/2.3).
        Batched through inside() under the product-bounded batching
        contract."""
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
  it cannot false-positive on ordinary surface-hugging stubs. LIMITATION
  (recorded per review, point 6): the stub rule tests the AXIS only, matching
  the census (census_run.py:52) — a stub whose centerline misses a fin but
  whose radius clips it sideways is not caught by this rule. Extending stubs
  to capsule-offset rays was considered and REJECTED: a stub hugs bumpy bark
  by construction, and an offset ray at radius r0 legitimately enters and
  exits surface convexities — through-transit false positives would block
  valid stubs wholesale. The radial-graze residual is recorded in sec 7; the
  worst thin-feature case at the stub TIP is separately closed by the
  embedded-end-cap breach check (sec 2.2).
- Non-stub members additionally get the mid-path penetration test: an inside
  interval outside the embed insets fails the member even when the standoff
  distance check passes (grazing-tangent transits). In practice
  `segment_clearance < 0` already catches nearly all of these; the interval
  test is the belt for exactly the census's through-transit class.
- **Hot-loop gate on `penetration_intervals`** (amended per review, point 4:
  revalidate runs tens of thousands of times per Treant; the dense census
  parameters STEP=0.12/CAP=80 are unbounded cost if run unconditionally, and
  a rewire subtree-reattach can dirty a large subtree). In revalidate, the
  interval test runs on a dirty member ONLY when at least one cheap trigger
  fires: (a) the Tier-2 exact pass found >= 1 Moller crossing on the member's
  axis, (b) `segment_clearance` came in below a 0.1mm margin above 0, or
  (c) the segment midpoint's parity vote says inside (one 3-axis vote — this
  is the trigger that catches a member routed WHOLLY inside a solid region,
  which an unsigned distance test cannot see and which crossing counts miss
  when entry happened through an open shell boundary). Soundness: an illegal
  inside interval implies axis crossings (a), or a fully-interior segment
  whose midpoint votes inside (c), or capsule-edge contact driving clearance
  <= 0 (b). The UNGATED dense-parameter interval test runs once per run in
  verify's independent gate (sec 2.2) regardless.
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
- **Embedded END-CAP breach check** (amended per review, point 5): the breach
  test must not be limited to joints that carry a ball. A member's embedded
  end is exempt from BOTH clearance paths by design (`segment_clearance`
  measures only `[inset0, L-inset1]`; `penetration_intervals` clamps the
  embed zone, census_run.py:128), so an embedded tip transiting a sub-mm fin
  AT its end escaped every census-era test — a carried-over blind spot, now
  closed: `ball_breach_report` additionally synthesizes an end-cap sphere
  (center = embedded end point, r = end radius) for EVERY embedded member end
  (stub tips at p0, strut/rung model-anchor ends at p1) whether or not a
  joint ball exists there, and runs `geom.sphere_breach` on the combined set.
  Same gate semantics: thick-feature shallow embeds pass by construction;
  poke-out > 0.05mm fails.
- The builder side must also stop CREATING breaching balls: cco model-anchor
  and bracing model-rung candidate trials add a `geom.sphere_breach` check on
  the anchor-end ball radius during candidate scoring (cheap: one ball per
  candidate), so a thin-twig anchor is rejected and the trial machinery moves
  to the next candidate — verify's gate is then a true independent re-check,
  not the first line of defense.
- **AMENDED (R3, 2026-07-23 full-scale acceptance finding):** the HARD gate
  (`ok`) and `self_verify`'s reported `ball_breach_violations` are scoped to
  the independent census's own antipodal-span reference: actual joint balls
  ONLY, matching it both algorithmically (same math) and in SCOPE (census
  never evaluates a member's embedded tip as a sphere at all). On the real
  Weeping Treant forest `ball_breach_report`'s combined list was 199 (15
  ball + 184 endcap) while the census's independent count for the identical
  1049-ball set was 15 — i.e. exact agreement on the ball category, and the
  entire gap is the amendment-5 endcap category, whose flags cluster at the
  measurement floor (~0.05-0.11mm feature thickness at r=0.2mm stub tips) in
  dense crown foliage on this non-watertight, double-walled mesh. The
  amendment-5 check itself is UNCHANGED (still synthesizes + tests every
  embedded end, still runs on the combined set) — its findings are now
  surfaced as `endcap_breach_diagnostic` (renamed 2026-07-24 from
  `embed_end_breach_violations` to make its provenance-only status explicit),
  a report-only diagnostic (never ANDed into `ok`), preserved for Phase-8
  relocation triage rather than dropped. Aligning the embed-end check's own thickness/inset accounting
  with the census (so it stops clustering at the floor) is R4's job, out of
  R3's scope. See `verify.self_verify`'s inline note and
  `validate_r3_ball_gate.py` (acceptance-run-2026-07-23/) for the
  reconstruction evidence.

### 2.3 Model anchors — real ray/mesh intersection

- `struts.find_model_anchor_candidates` (struts.py:22-72) currently selects
  from `sample_points` = perception.delicacy_field's seeded random cloud
  (perception.py:496, threaded via engine.py:68 -> cco.build sample_points,
  cco.py:157/278/321; also rewire.py:845/876 and bracing.py:218/246/271/347).
  Replacement: the candidate POOL becomes **exterior-face centroids**
  (deterministic, seed-free, on-surface by construction): faces from
  `geom.exterior_face_mask()`, delicacy weight obtained by querying each
  centroid against `perception.delicacy_field`'s query function (the seeded
  field survives as SCORING only, sec 3 — corrected per review inventory
  note: the earlier cross-reference to sec 2.5 was wrong; 2.5 is
  load_partition and has nothing to do with delicacy).
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
- **Anchor-coverage measurement** (amended per review, point 7): the new pool
  (exterior centroids) plus builder-side breach rejection of thin-twig
  anchors can strand contacts the v4 engine routed — by DESIGN when the only
  available anchor was defect-class (a sub-mm twig a ball would breach), but
  the spec must measure, not assume. Required instrumentation: fixture tests
  pin the unrouted-contact count (sec 6); the full-scale run log reports
  routed fraction and unrouted delta vs the v4 reference so a coverage
  regression is visible and judged, never silent. A contact that loses ALL
  model-anchor candidates falls back to the existing plate-routing path
  (behavior unchanged); keeping breach-rejected thin-twig anchors as a
  fallback is explicitly rejected (appendix).

### 2.4 Perception demand seeding — interior-face filter + A2 regular-contact pool

The STL is a non-watertight overlapping assembly: ~40% of "unrouted" contacts
were phantoms on faces INTERIOR to the solid union (audit finding). Fix at the
source so no downstream stage ever sees interior demand:

- `perception.overhang_islands` (perception.py:106-142): the need mask
  (perception.py:116, `down & ~grounded`) additionally requires
  `geom.exterior_face_mask()`. Interior faces can neither found an island nor
  join one.
- **A2 supersession (owner-ratified 2026-07-25):**
  `placement.sample_candidates` no longer calls `sub.sample` and no longer
  consumes RNG. Each exterior source face is uniformly refined by recursive
  1-to-4 midpoint subdivision to the positive coverage margin
  `target_spacing_mm * coverage_radius_factor - target_spacing_mm`; depth is
  `ceil(log2(max_edge / margin))` and is refused above
  `contact_pool_max_refine_depth`. Every virtual cell nominates every
  co-lowest vertex. Exact-position nominations are deduplicated; ownership is
  the source face with most-negative normal-z, then source `face_id`.
  Positions, source `face_id`, and raw source-face normals form the regular
  contact pool. The pool is built once in `engine.prepare`, logged, and reused
  by initial placement, repair, Phase-8 R2, and final verification. This is
  invariant under prior dyadic midpoint refinement, not arbitrary
  retriangulation.
- The build is globally preflighted before virtual allocation:
  `sum(4**depth)`, `3*cells`, and `3*cells*64` must fit the explicit
  750,000-cell, 2,250,000-candidate, and 144-MiB defaults. Those limits provide
  about 30% headroom over the 576k-face depth-0 class; refusal telemetry names
  every winning cap. The 144-MiB setting bounds only the retained compact SOA
  payload, not process peak memory. These aggregate caps are the primary
  allocation prevention: an island's one-cell-per-face lower bound is checked
  before face conversion/triangle materialization, admitted planning uses
  fixed batches of at most 2,048 faces, and refusal is monotonic. Nomination
  handling is bounded, batch-first NumPy. Every long construction loop also
  asserts that `getrusage`'s process peak-RSS high-water mark has not risen by
  2 GiB from the build-entry high-water mark; this is a last resort, not a
  current-RSS measurement. Retained rows live in a schema-versioned picklable struct-of-arrays
  at exactly 64 ndarray bytes each. Its arrays are rebuilt bytes-backed,
  C-contiguous, and read-only after construction and unpickling; validation
  checks array metadata, counters, ordering, mesh/island ranges, and raw
  normals. `engine.prepare` also records deterministic SHA-256 fingerprints of
  canonical mesh arrays and semantic config JSON. Construct, finish,
  relocation, and performance-cache reuse reject old, corrupted, or mismatched
  checkpoints with an instruction to rerun prepare.
- Minimum and island-birth contacts remain specialized. They persist
  `region_pts`, or `birth_loop` plus `birth_z`, respectively; Phase-8 filters
  their source-face-center candidates by those exact domains, with birth
  precedence when both flags are set. They do not inherit regular-pool
  `face_id` provenance.
- `engine._ensure_minima_contacts` / `_ensure_island_birth_contacts`: their nearest-face lookups
  (`centers_tree.query_ball_point`) are restricted to exterior faces (build
  the centers KDTree over exterior faces only). `perception.local_z_minima`
  and `perception.island_births` operate on the full mesh (a slice birth is a
  real print event regardless of which sub-body owns the face) — unchanged,
  except birth-contact normals are taken from the (exterior) nearest face.
- `perception.delicacy_field` (perception.py:474-563): sampling restricted to
  exterior faces; retains its seed for the SCORING field only (sec 3).

### 2.5 Seed-dependent volumes — `placement.load_partition`

The former Monte-Carlo `sub.sample` partition makes carried areas
— hence tip radii, hence member volumes — seed-dependent. Replacement: exact,
deterministic per-face assignment: each island face's centroid is assigned to
its nearest contact (cKDTree over the island's contacts), and the face's full
`mesh.area_faces` value accrues to that contact. No RNG, no sampling; exact
at face granularity (faces are small vs. `target_spacing_mm` on this corpus).
Same signature, `n_dense_samples`/`seed` parameters retired.

### 2.6 Inventory of remaining `build_surface_tree` / `.sample` consumers

This is a historical migration inventory. Its `file:line` annotations are
obsolete orientation only; the named symbols and dispositions are
authoritative because live line numbers drift as the implementation evolves.

| Consumer (file:line) | Load-bearing? | Disposition |
|---|---|---|
| tree.Forest.revalidate query (tree.py:399) | **YES** | -> geom.segment_clearance (2.1) |
| verify.clearance_report (verify.py:129) | **YES** | -> exact, independent (2.2) |
| verify.connectivity_report embed check (verify.py:369-373) | **YES** (attachment gate) | -> geom.query (exact point distance). Threshold FORMULA unchanged, but note the DIRECTIONAL effect (review point 9): exact distance <= nearest-cloud-sample distance, so `surf_dist` shrinks and strictly MORE endpoints pass `attached_c` — an intended loosening (the cloud over-estimated detachment), pinned by a test (sec 6.12) |
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
| placement.sample_candidates (placement.py) | **YES** (regular demand positions) | A2 prepared virtual lowest-vertex pool; deterministic, seed-free; deprecated `seed=` accepted and ignored |
| perception.grounded_mask rays (perception.py:75) | YES but already ray-exact (trimesh ray backend) | optionally route through geom.cast_rays for backend independence; not required for v5 |

Explicitly deferred: `bracing.py:762` `if surface_tree is None: return []` and
similar None-guards keep their meaning with `geom is None`.

---

## 3. Determinism statement

- Zero RNG in any load-bearing geometric decision: clearance, penetration,
  ball breach, anchors, interior classification, and load partition are all
  exact functions of (mesh, cfg, contact set). No wall-clock dependence.
- Parity votes use fixed axis rays and fixed grid bucketing — deterministic.
- Remaining seeded sampling is scoring-only: `delicacy_field` retains its
  fixed seed for weights. Regular contact placement and load partition are
  seed-free.
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
5. Alongside the volume freeze, freeze COVERAGE numbers (review point 7): the
   fixture unrouted-contact counts are pinned, and the full-scale report
   records routed fraction vs the v4 reference. The volume not-worse gate
   alone would silently accept lost routes (fewer routes = less resin); the
   coverage pin closes that hole.

---

## 5. Performance strategy + projected full-scale runtime

Hot path analysis: `Forest.revalidate` is called from cco's insertion trial
loop up to tens of thousands of times per Treant run, mostly on small dirty
sets, mostly in open space. Strategy:

- **Tier-1 conservative reject** (the workhorse): for a segment batch, query
  the triangle-CENTROID cKDTree at path points (step = clearance_sample_step_mm)
  with `distance_upper_bound = r + standoff + tri_extent_nonlarge` — the max
  centroid-to-vertex extent among NON-large triangles only (amended per
  review, point 1: the census's `tri_extent` field is the GLOBAL max
  including the `large` set, so using it here would inflate the bound by
  >= large_thr and demote essentially every path point to Tier-2, collapsing
  the performance strategy; the global max is reserved for crop radii, where
  conservatism is cheap). If no centroid falls within the bound, no NON-large
  triangle can be within r + standoff (any such triangle's centroid would be
  within r + standoff + its own extent <= the bound) — and `large` triangles
  are never Tier-1-certified: they are checked exactly on every query via the
  never-miss list. Together the segment is certified clear with near-zero
  exact work. This is a sound bound, not an approximation: it can only send
  MORE work to tier 2, never hide a violation. large_thr is derived per mesh
  at build time (sec 1) so tri_extent_nonlarge stays tight on THIS mesh
  (review point 10: 1.5mm was a census constant, not a law).
- **Tier-2 exact** only for segments tier 1 cannot certify: crop candidates
  by ball query around sub-segment midpoints (split long members ~5mm so crop
  radii stay local), then vectorized exact segment-vs-triangle distance +
  Moller crossings + parity on the (few) interval midpoints.
- Batching: one revalidate call = one tier-1 pass + tier-2 exact work across
  all dirty members, with tier-2 blocks sized by the QUERY x CANDIDATE
  product budget of sec 1 (amended per review, point 2 — a flat 2048-segment
  block near dense surface patches would materialize multi-GB intermediates;
  the census stayed safe precisely because it processed one member's ragged
  crop at a time). Ragged concatenation, never rectangular padding to the
  max candidate count. RSS assert < 2GB in every loop.
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
  by near-surface trials, and the dense interval test is gated (sec 2.1)
  rather than unconditional. The ~400-900s estimate below is CONDITIONAL on
  the tri_extent_nonlarge Tier-1 bound and the interval gate both landing as
  specified (review points 1 and 4 — without them the strategy degenerates
  to all-Tier-2 and the estimate is void); the first full-scale run must
  report the measured Tier-1 certify rate alongside the timings so the
  projection is validated, not assumed. Honest estimate: **~400–900s
  (7–15 min) single-core full Treant**, worst case low tens of minutes —
  comfortably inside the overnight-usable bound, far from it in expectation.
  Report the measured number in the run log (`t_total`, plus new
  `t_geom_build`, `t_exterior_mask`, `tier1_certify_rate`); re-baseline pins
  per sec 4.

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
12. **Connectivity gate direction** (review point 9): a fixture endpoint
    whose exact surface distance sits just under `contact_depth + 0.45` but
    whose nearest CLOUD sample would read over it MUST pass `attached_c` on
    v5 — pinning that the swap loosens the gate in the intended direction
    (exact <= sampled) and only that direction.
13. **Exterior-mask gap robustness** (review point 3): the two-box fixture
    re-run with the inter-shell gap swept across the eps ladder (gap in
    {0.02, 0.1, 0.3} mm): outer-shell faces MUST stay exterior at every gap;
    genuinely buried faces MUST stay interior; any ambiguous face resolves
    EXTERIOR (fail-open pin).
14. **Embedded end-cap breach** (review point 5): a ball-less strut whose p1
    embed terminates inside a 0.3mm fin with poke-out > 0.05mm MUST be
    flagged by `ball_breach_report` via the synthesized end-cap sphere; the
    same strut embedding into a 5mm slab MUST pass.
15. **query exactness certificate** (review point 8): a fixture with a
    ~1.2mm triangle (sub-large_thr) placed so its centroid ranks beyond
    k=64 from the query point but its face is nearest: `geom.query` MUST
    return the true distance (escalation path exercised).
16. **Coverage pins** (review point 7): fixture unrouted-contact counts
    pinned; a synthetic thin-twig-only-anchor fixture MUST route to plate
    (not to the breach-rejected twig) with the contact still supported.
17. **A2 regular-contact invariant:** a coarse planar source triangle is
    coverage-clean without centroid/random fallback; a source triangle and
    its dyadic midpoint subdivision produce identical pool geometry; source
    face order does not alter selected geometry; stale position/face/normal
    provenance (including an unrouted original contact) and specialized
    contacts carrying a regular `face_id` hard-fail `verify.ok`.

### 6.1 A2 representative acceptance evidence (2026-07-25)

The re-freeze helper ran table and toed-slab twice each and asserted exact
volume-report equality. Table: total 0.39602846189165186 ml, member
0.29864423997554795 ml, 4 unrouted, 34 accepted rewires; prepared pool depth
5, 2,176 virtual cells, 1,189 candidates, and 76,096 compact ndarray bytes.
Toed slab: total
0.5362920772872579 ml, member 0.43307741172616016 ml, 2 unrouted, 41 accepted
rewires; pool depth 5, 2,080 virtual cells, 1,109 candidates, and 70,976 compact
ndarray bytes. Both finish with
`verify.ok = True`, zero uncovered islands, zero ball breaches, and zero A2
    regular-contact invariant violations.
18. **A2 resource/lifecycle gate:** aggregate many-face plans refuse before
    virtual allocation; one depth-8 face is admitted; the 2-GiB peak-RSS
    high-water-growth
    guard is exercised; compact-pool payload stays bounded by its 64-byte rows
    plus fixed metadata. Construction and pickle roundtrip preserve immutable
    arrays; counter/order/range corruption is rejected. A full prepared-state
    roundtrip is reused by placement, repair, R2, and final verify with pool
    recomputation disabled; old schemas and mesh/config fingerprint mismatches
    fail clearly. One accepted R2 move globally per frozen sweep is pinned by
    same-island and cross-island shared-birth cases where two individually safe
    moves are jointly unsafe.

This is the deliberately bounded A2 acceptance set. The 37-model corpus batch
is deferred until the A3/A4 changes land so the expensive corpus run validates
the combined phase rather than an intermediate contact-pool snapshot.

A prepare-only corpus smoke test on `minis-to-process/Round 25.stl` (2,996
faces) completed in 0.76 s wall time: 1 island, 31 contacts, pool depth 4,
37,250 virtual cells, 19,997 candidates, and 1,279,808 compact ndarray bytes.
This validates bounded pool construction on an actual corpus STL without
prematurely running the
37-model batch.

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
- **Stub radial graze** (recorded per review, point 6): a stub whose radius
  clips a thin fin the centerline misses is not detected — stubs keep the
  axis-only interval rule because capsule-offset rays on surface-hugging
  stubs would through-transit bark convexities and false-positive
  wholesale. Mitigations in scope: the stub-tip end-cap breach check
  (sec 2.2) closes the worst thin-feature case at the tip; the census found
  zero stub radial-graze defects (all 3 illegal stubs were axis transits).
  Revisit if a future census finds this class in the wild.

---

## 8. Adjudication appendix (2026-07-23 adversarial review)

Two critiques were commissioned. **Codex: UNAVAILABLE** (the relay produced
no output — no session id; recorded here so the single-critique provenance
is explicit). The Opus critique was adjudicated point by point; every claim
was verified against census_core.py / census_run.py before ruling.

**Accepted (spec amended):**
1. Tier-1 bound used the census's GLOBAL tri_extent (census_core.py:44
   includes the `large` set) — would have inflated the reject bound by
   >= large_thr and collapsed Tier-1. Fixed: `tri_extent_nonlarge` (sec 1,
   sec 5), large_thr derived per mesh (also resolves point 10).
2. 2048-query batching does not bound memory (cost is queries x candidates;
   closest_on_tri materializes ~15 (M,3) intermediates). Fixed:
   product-bounded adaptive blocks, ragged crops, no padding (sec 1, sec 5).
3. Single-point/single-eps exterior_face_mask was brittle and unvalidated —
   the highest-risk new behavior (can silently delete real demand). Fixed:
   6-sample robust majority (3 barycentric points x 2 eps), measured
   inter-shell gap diagnostic, fail-open-to-exterior bias (sec 1), gap-sweep
   test (sec 6.13).
4. Unbounded dense penetration_intervals in the revalidate hot loop. Fixed:
   triple-trigger gate (axis crossings / clearance margin / midpoint parity
   — the parity trigger covers the wholly-interior segment an unsigned
   distance test cannot see); ungated dense pass stays in verify (sec 2.1).
5. Ball-less embedded member ends escaped every breach/clearance test (both
   paths exempt the embed zone; census_run.py:128 clamps it). Fixed:
   synthesized end-cap spheres for every embedded end in ball_breach_report
   and builder scoring (sec 2.2), test 6.14.
7. (partial) Anchor-pool change can strand contacts silently under the
   volume-only gate. Fixed: coverage pins + routed-fraction reporting
   (secs 2.3, 4.5, 6.16).
8. `query` was labeled EXACT but k-NN can miss a sub-large_thr triangle
   beyond rank k. Fixed: exactness certificate with adaptive escalation
   (sec 1) + test 6.15 — kept the EXACT label by making it true rather than
   weakening the label.
9. Connectivity attachment gate loosens directionally under exact distances.
   Accepted as intended behavior, now documented (sec 2.6) and pinned
   (test 6.12).
10. large_thr=1.5 was an uninherited-context census constant. Fixed: derived
    per mesh at build, logged (sec 1).
Inventory note: the sec 2.3 delicacy cross-reference to sec 2.5 was wrong;
corrected to the delicacy_field query function (scoring only).

**Rejected challenges:**
- Point 6 (extend stub certification beyond the axis ray): REJECTED as a
  certification rule. Capsule-offset rays on a surface-hugging stub
  through-transit bark convexities by construction — the false-positive rate
  would block legitimate stubs wholesale, and the census found zero
  radial-graze stub defects. The residual is recorded honestly in sec 7 and
  partially mitigated by the stub-tip end-cap breach check. Re-open on
  evidence.
- Point 7 (implied remedy of retaining thin-twig anchors to preserve v4
  coverage): REJECTED. Breach-rejected anchors ARE the census defect class
  (44 ball breaches); keeping them as fallback re-introduces the defect.
  Plate fallback + measured coverage is the correct trade — resin economy
  never buys back correctness.
- Point 8's suggested relabeling to "near-exact": REJECTED in favor of the
  stronger fix — the certificate makes `query` actually exact, so the label
  stands.
