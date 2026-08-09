# SPEC-A3-A4-COVERAGE — Prepared topology, coverage, selection, and repair

Status: **NORMATIVE — owner-ratified A3/A4 contract.**

This specification is the governing authority for A3 prepared coverage and A4
coverage repair. The measurements and derivations in
[`physics_coverage.md`](../reference/physics_coverage.md) are supporting
rationale and evidence, not a second source of law. If that note and this
specification differ, this specification governs.

## 1. User rules and calibrated reach

Support-contact size and coverage reach are separate decisions:

- Cao et al.'s 0.4 mm-square printable support and reported 2.4 mm
  self-supporting reach are specific to that paper's printer, resin, vat, and
  process. They are neither universal physical constants nor project floors,
  and they are not trusted as constants for the Saturn 4 Ultra and the current
  project resins.
- A contact may be 0.15 mm in diameter when it is sufficiently vertical and
  mechanically justified. A contact may be wider than 0.4 mm when the larger
  contact is mechanically justified and safely reduces the required support
  count.
- The coverage judge uses inclusive 3D Euclidean reach
  `R = target_spacing_mm * coverage_radius_factor`. The current defaults are
  `4.0 * 1.5 = 6.0 mm`. There is no universal 2.4 mm overhang or reach
  constant.

`R` is an analysis calibration, not a tip-diameter proxy or a claim that two
surfaces separated by that Euclidean distance are physically connected.

## 2. Immutable source topology

The source mesh is loaded with `process=False`. Raw source-face rows and their
ordering are immutable provenance; topology preparation must not call
`process=True`, merge or weld vertices, rewrite faces, or substitute processed
`face_adjacency`.

The prepared topology is one immutable, schema-versioned CSR authority:

1. Vertex identity is exact coordinate-bit identity. Even a 4e-9 mm coordinate
   difference remains a seam.
2. A graph adjacency exists only where one exact undirected edge has exactly
   two distinct source faces with opposite directed incidence.
3. Boundary edges, non-manifold edges, same-direction incidence,
   duplicate-triangle incidence, degenerate edges, and point-only contact are
   ambiguous cuts. They are counted and retained as cuts, never silently
   welded into adjacency.
4. Connected-component IDs are derived only from those admitted graph edges.

The proximity merge used to name operational overhang islands may remain
many-to-many and may prevent textured surfaces from fragmenting. It is not
physical connectivity and cannot override an exact-topology cut.

## 3. Named ownership and exact route provenance

A source face may belong to multiple named islands. That many-to-many
membership must be preserved.

Every contact nevertheless has exactly one route: one named island, one exact
source face, and the exact topology component containing that face. Coverage,
repair, Phase-8 R2, and final verification use that route tuple. A synthetic
or component-local island extension must contain exactly one topology
component and must not transfer ownership across components. Missing, stale,
corrupt, or mismatched route provenance fails clearly; it is never repaired by
choosing a nearby face or by collapsing overlapping named islands.

## 4. Area-weighted A3 coverage and failure

For every named island source triangle, preparation recursively performs
dyadic 1-to-4 subdivision until every terminal edge is no longer than
`voxel_pitch_mm`. Each terminal centroid is a sample with:

- its exact source-face ID and named-island membership;
- the exact topology component of that source face; and
- weight `source_face_area / 4**depth`.

A sample is covered only by a routed contact with the same named island and
same exact topology component whose 3D Euclidean distance is `<= R`.
Uncovered weights are first summed by exact source face. Connected active
source faces then form an uncovered patch.

A patch fails if and only if either:

1. its uncovered area is greater than `voxel_pitch_mm**2`; or
2. it touches an ambiguous topology cut.

Area at or below `voxel_pitch_mm**2` is **censored A3 uncertainty**. It is
diagnostic only: it is not a physical assertion that resin can bridge that
area or shape, and it is not permission for another hard gate to fail.
Covered fraction is diagnostic only. At the current 0.4 mm pitch the censor is
0.16 mm².

## 5. Bounded deterministic synthetic selector

Synthetic A3 selection is deterministic, spacing-aware, and fail-closed:

- Every candidate-by-sample distance operation is tiled so no temporary
  matrix exceeds `coverage_score_matrix_max_entries`, default 65,536.
- For `C` eligible candidates and `S` still-needed prepared samples, packed
  mask demand is
  `M_words = C * ceil(S / 64)`.
  It must not exceed `synthetic_coverage_mask_max_words`, default 131,072.
- The conservative retained Python-mask estimate is
  `R_masks = 64 + 8C + C * (128 + 16*ceil(S/64))`.
  The word limit is not described as a literal one-MiB process allocation.
- Before mask allocation, `np.packbits`, or search, one aggregate peak plan
  includes the already-retained candidate pool, topology and coverage
  authorities, `R_masks`, one bounded xyz-delta/squared-distance tile,
  boolean/packbits scratch, and bounded exact-search scratch. The plan must be
  at most 512 MiB.
- Singles are exhausted deterministically. Pair search is exhaustive.
  Cardinalities three and above use deterministic exact branch-and-bound with
  a cap of 1,048,576 search nodes per named-island/component route.
- If the higher-cardinality cap is reached, the selector may return only a
  deterministic candidate set that is independently proven to cover every
  required prepared sample and satisfy spacing. It is marked `exact=False`
  and is never described as a global minimum.

Exceeding a resource limit, exhausting a bounded search without a proven-cover
fallback, or finding no valid cover fails closed. No partially selected
contact rows are emitted.

## 6. A4 repair contract

A4 preserves exact candidate-pool row identity throughout.

Before rewire, bracing, carried-load repartition, or coverage judgment, every
unrouted regular contact is ledgered with its exact row/reason and removed from
the accepted contact set. An unrouted specialized minimum or island-birth
contact is a hard failure.

Repair candidates come only from the failing named-island/topology-component
route. Occupied rows and exact failed rows are excluded. After every accepted
or rejected attempt, candidates are reranked by marginal uncovered sample
area, with lower pool row as the deterministic tie-break. Candidate scoring
tiles both candidates and samples under the same 65,536-entry matrix bound.

One winner at a time enters a full construct, rewire, brace/size, and verify
transaction. It is accepted only when:

1. the coverage objective strictly improves lexicographically in this order:
   total failing area, maximum failing-patch area, failing-patch count, worst
   gap; and
2. every noncoverage hard-gate count is componentwise no worse.

Otherwise the accepted forest is restored and the exact row/reason is
ledgered. `coverage_repair_max_attempts` defaults to 8. Attempt-cap exhaustion
cannot convert residual uncovered area into success.

## 7. Phase-8 R2, defaults, and rollback

Phase-8 R2 uses the same validated prepared topology, coverage samples,
candidate pool, and exact route provenance as placement, A4 repair, and final
verification. It never reconstructs the regular pool implicitly. Missing,
stale, corrupt, or mesh/config-mismatched authorities fail clearly.

`p8_relocate_enable` and `p8_reseat_enable` are enabled by default. Explicitly
disabling final repair may honestly leave final verification false. Within one
frozen sweep, at most one accepted R2 re-seat may be committed globally.

Phase 8 and the structural resize that follows it form one cloned transaction
and one combined fixed point. Resize-created geometry is re-entered through
Phase 8. An exception, unresolved cap, sweep/round cap exhaustion, or failed
final gate discards the clone and restores the entire accepted state; no
partially relocated or resized forest may escape.

## 8. Resource accounting and refusal limits

All configured caps below are independent refusal guards. Preflight occurs
before large generated arrays. A builder completes and drops mutable locals
before outer deep validation starts. `compact_retained_bytes` is exact compact
payload; `planned_peak_build_bytes` and `planned_peak_validation_bytes` are
separate conservative phase maxima. The 512 MiB peak cap applies independently
to each phase.

The native point-cloud kNN batch is separately planned before gathering,
tree construction, or query execution. Its cap is logical element capacity
only (not allocator metadata, alignment, or RSS): gathered source/query
coordinates; the pinned Kiddo immutable-tree leaf coordinates/items/extents
and stems; build sort indices; direct CSR offsets/IDs/distances; and one
sequential inclusive-tie scratch row. The formula is tied to the pinned Kiddo
layout/features (currently leaf size 32); any Kiddo pin or feature change must
review that accounting before it is accepted.

### 8.1 Topology

Let `F` be source faces, `V` exact vertices, and `E` retained directed CSR
edges. Exact retained payload is:

`R_topology = 17F + 8E + 735`.

The fixed 735 bytes cover the `indptr` terminator, schema/version, six scalar
counters/plans, 64 cut counters, and two ASCII SHA-256 values. Build preflight
uses `E <= 3F` and the maximum of:

`48F + 88V`,
`84F + 32V`,
`527F + 32V`,
`300F + 32V`,
`900F + 32V`, and
`2R_topology(F,3F) + max(24F,24V)`.

Validation uses actual `E` and plans:

`R_topology + max(8F + 97E, 16F + 10E, max(8(F+1),8E,24F,24V))`.

Defaults are 750,000 faces, 64 MiB exact retained payload, and 512 MiB for
each build or validation peak. The peak equation may refuse an ordinary
manifold mesh before the nominal face limit.

### 8.2 Coverage samples

Let `N` be virtual cells, `S` source faces, `I` named islands, `W` the bounded
working-cell batch, and `F,V` the source-mesh dimensions. Exact retained
payload is:

`R_coverage = 49N + 24S + 8I + 330`.

Build peak is the maximum of:

- planning: `35S + 256*min(S,2048)`;
- generation: `49N + 9S + 192W`;
- join/sort/immutability/digest completion:
  `R_coverage + 106N + 25S + 16I + 16`
  `+ max(24N,24F,24V,8S)`.

Validation peak is:

`R_coverage + max(24N, 8N + 49S + 8I,`
`max(24N,24F,24V,8S), 40N + 8F, 123N + 72F, 16N + 16S)`.

Defaults are 2,000,000 virtual cells, 160 MiB exact retained payload, 65,536
working cells, and 512 MiB for each build or validation peak.

Runtime telemetry distinguishes measured build and total RSS high-water
deltas from these planned ndarray/payload quantities; allocator, interpreter,
SciPy, and trimesh cache overhead may make RSS larger than retained arrays.

## 9. Acceptance evidence status

Acceptance evidence is not a substitute for the contract above. The current
table refreeze candidate has 11 selected regular A3 contacts, and its
actual-A3 deletion check proves each row necessary by deleting it and observing
a failing uncovered patch. That count is **current refreeze pending** until the
parallel signature/baseline refreeze is accepted.

The toed fixture's higher-cardinality result is a deterministic,
independently proven-cover fallback after the node bound. It is recorded as
`exact=False` and is not a global-minimum claim.
