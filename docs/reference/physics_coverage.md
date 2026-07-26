# Coverage and source-mesh topology — rationale and evidence

This is a **non-normative** rationale and evidence note. The complete governing
A3/A4 contract is
[`SPEC-A3-A4-COVERAGE`](../specs/SPEC-A3-A4-COVERAGE.md); if any descriptive
language or formula here differs from that specification, the specification
governs. This note preserves the calibration reasoning, measured resource
evidence, and acceptance observations behind the ratified rules. It is not a
second implementation authority and does not turn one paper's
printer/resin/process result into a universal resin-physics constant.

## Calibrated reach

The current reach remains:

`R = target_spacing_mm * coverage_radius_factor = 4.0 * 1.5 = 6.0 mm`

Coverage uses inclusive 3D Euclidean distance. There is deliberately no 2.4 mm
Cao-derived reach, tip-diameter proxy, fractional uncovered-area rule, or
surface-geodesic rule. The single Cao result does not match practical Saturn 4
Ultra experience with this project's resins and is not trusted as a governing
constant. Euclidean reach can over-credit two surfaces that are spatially close
across a fold; same-source-topology-component routing limits that risk but does
not eliminate it. Corpus prints remain the calibration authority.

Support tip diameter is a separate sizing question. Tips may be wider than
0.4 mm when a larger contact safely reduces total support count, and may be as
narrow as 0.15 mm when the resulting weak contact is sufficiently vertical and
otherwise mechanically justified. Neither number changes the 6 mm analysis
reach automatically.

## Why topology is prepared explicitly

The production mesh is loaded with `process=False` because raw face rows are
provenance used by contact ownership, normals, prepared fingerprints and later
verification. Calling trimesh `process=True`, `merge_vertices`, or relying on
its processed `face_adjacency` would rewrite or weld that authority.

`support_placement.topology` therefore prepares one immutable, schema-versioned
CSR sidecar without mutating geometry. Vertex identity is exact coordinate-bit
identity. A graph edge exists only for an exact shared undirected edge with
exactly two distinct source faces and opposite directed incidence. Boundary,
non-manifold, same-direction, duplicate-triangle, degenerate-edge and
point-only contacts are recorded as cuts. A seam that differs by even 4e-9 mm
is a seam: the sidecar never silently welds it.

Overhang grouping may still use the existing proximity merge to keep textured
surfaces from fragmenting into thousands of operational islands. Coverage does
not treat that merge as physical connectivity: contacts route only within the
same island and exact topology component.

## Resource accounting

Topology and coverage use the same lifecycle: a global preflight runs before
large generated arrays, an inner builder returns the immutable authority, all
builder locals leave scope, and only then does an outer deep-validation call
begin. The logged quantities are deliberately distinct:

- `compact_retained_bytes` is the exact compact payload, not a peak estimate;
- `planned_peak_build_bytes` is the conservative maximum of build phases;
- `planned_peak_validation_bytes` is the conservative validation-phase maximum;
- `max_peak_bytes` is the configured cap applied independently to both plans;
- runtime stats report `measured_build_rss_high_water_delta_bytes` and
  `measured_total_rss_high_water_delta_bytes`; subprocess probes additionally
  report `rss_base`, `rss_peak`, and `rss_delta`. RSS also includes allocator,
  interpreter, SciPy, and trimesh cache overhead that is not a retained ndarray
  payload.

For topology, with `F` faces, `V` vertices, and `E` retained directed CSR edges,
the exact retained payload is:

`R_topology = 17F + 8E + 735`

The 735 bytes are the `indptr` terminator plus schema/version, six scalar
counters/plans, 64 cut counters, and two ASCII SHA-256 values. Build preflight
uses `E <= 3F` for the retained upper bound and the maximum of these
simultaneous-array phase equations:

`48F + 88V`, `84F + 32V`, `527F + 32V`, `300F + 32V`,
`900F + 32V`, and `2R_topology(F,3F) + max(24F,24V)`.

Those terms respectively cover source/unique vertex arrays, duplicate-triangle
sorting, original and sorted edge/group arrays, CSR construction, incidence
and local-fan sparse scratch, and mutable-to-immutable plus fingerprint/digest
completion. Validation uses actual `E` and plans:

`R_topology + max(8F + 97E, 16F + 10E, max(8(F+1),8E,24F,24V))`.

For coverage, let `N` be virtual cells, `S` source faces, `I` islands, `W` the
bounded working-cell batch, and `F,V` the source mesh dimensions. Exact retained
payload is:

`R_coverage = 49N + 24S + 8I + 330`.

This includes all nine ndarrays, both offset terminators, schema/version, ten
scalar counters/plans, and three ASCII SHA-256 values. Build peak is the maximum
of:

- planning: `35S + 256*min(S,2048)`;
- generation: `49N + 9S + 192W`;
- join/sort/immutability/digest completion:
  `R_coverage + 106N + 25S + 16I + 16`
  `+ max(24N,24F,24V,8S)`.

Validation peak is `R_coverage` plus the maximum of `24N`,
`8N + 49S + 8I`, `max(24N,24F,24V,8S)`, `40N + 8F`,
`123N + 72F`, and `16N + 16S`. These terms cover validation lexsort/order,
source-group repeats, digest/fingerprint bytes scratch, exact-weight
temporaries, trimesh triangles plus selected triangles/`lo`/`hi` masks, and
island-membership scratch.

The defaults are a 64 MiB topology and 160 MiB coverage exact-retained cap,
with a 512 MiB peak cap for each build or validation phase. A fresh-process
macOS probe on 2026-07-26 admitted a 576,000-face manifold grid:

| authority | exact retained | planned build | planned validation | measured build RSS delta | measured total RSS delta | cap |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| topology | 23,599,455 | 527,650,592 | 195,613,935 | 380,633,088 | 408,289,280 | 536,870,912 |
| depth-0 coverage | 42,048,338 | 131,328,370 | 154,368,338 | 258,490,368 | 300,072,960 | 536,870,912 |

The face/cell count limits remain independent refusal guards. In particular,
the topology peak formula, rather than the nominal 750,000-face ceiling, is the
effective default limit for ordinary manifold vertex/face ratios.

### Synthetic selector masks

The synthetic A3 selector has a dedicated retained-mask refusal guard. For
`C` eligible candidates and `S` still-needed prepared samples, requested
uint64-equivalent words are:

`M_words = C * ceil(S / 64)`.

`synthetic_coverage_mask_max_words` defaults to 131,072. This is separate from
`coverage_score_matrix_max_entries = 65,536`, which bounds each temporary
candidate-by-sample distance tile. The 131,072-word setting must not be
described as a literal 1 MiB process allocation: masks are retained as a Python
list of Python integers. The selector therefore logs a conservative physical
estimate:

`R_masks = 64 + 8C + C * (128 + 16*ceil(S/64))`.

Before allocating the mask list, calling `np.packbits`, or entering search, the
selector also plans one aggregate 512 MiB peak containing the already-retained
candidate-pool, topology, and coverage authorities; the conservative Python
mask representation; one bounded xyz-delta plus squared-distance tile; boolean
and packbits scratch; and the bounded exact-search subset/metric scratch.
Exceeding either the word limit or this aggregate peak fails closed before mask
allocation. No partially selected contacts are emitted.

The duplicate full toed-slab runs requested 73,997 words: 591,976 dense-
equivalent bytes, but 1,213,528 conservatively estimated physical retained
bytes. Including 3,397,197 retained authority bytes, a 2,097,152-byte bounded
distance tile, 131,072 bytes of packing scratch, and 11,010,112 bytes of
exact-search scratch, the planned aggregate was 17,849,061 bytes. Every
observed distance matrix remained at or below 65,536 entries.

## Area-weighted coverage judge

Prepare dyadically subdivides each island source triangle until terminal edges
are no longer than `voxel_pitch_mm`. Each terminal centroid carries its exact
source-face area divided by `4**depth`. Prepared samples are immutable and
resource-preflighted before virtual-cell allocation. The retained compact
payload limit is not treated as a peak-memory limit: preparation separately
preflights build and later deep-validation phases against
`coverage_samples_max_peak_bytes`.

A sample is covered only when a routed contact in its island and topology
component is within `R`. Uncovered sample weights are summed by source face;
connected active source faces form a patch. A patch fails when:

- uncovered area is greater than `voxel_pitch_mm**2`; or
- it touches an ambiguous topology cut, which fails closed.

At the current 0.4 mm pitch, 0.16 mm² is an analysis censor: uncertainty at or
below one analysis cell is diagnostic, not a physical statement that resin can
bridge any particular 0.16 mm² shape. Source-face accumulation is intentionally
conservative and can merge unresolved regions that a finer representation
would separate. Covered fraction is diagnostic only.

The table prepared-selector deletion regression checks actual A3, not only the
selector's internal set-cover masks. In the current refreeze-pending result,
all 11 selected synthetic regular rows are individually necessary: deleting
any one creates a failing uncovered patch above the 0.16 mm² analysis censor.
The accepted refreeze should record the final per-row areas rather than carry
forward the superseded 12-row range.
For the toed slab, the admitted higher-cardinality result is explicitly a
deterministic proven-cover fallback after the node bound (`exact=False`), not a
minimum-cardinality claim.

## Repair

A4 keeps exact candidate-pool row identities. An unrouted regular contact is
ledgered and removed before rewire, bracing, load repartition or coverage
judgment; specialized unrouted contacts remain hard failures. Repair candidates
come only from failing topology components, excluding occupied and exact failed
rows. They are reranked after every accepted or rejected attempt by marginal
uncovered sample area, with lower row as the deterministic tie-break.
Candidate scoring tiles both candidates and samples under one matrix-entry
budget; a large failing patch can therefore never turn one candidate batch into
an unbounded candidate-by-all-samples allocation. Phase-8 re-seat coverage uses
the same two-axis bounded rule and validates/caches prepared authorities once
per stage.

Each winner alone enters one full construct, rewire, brace/size and verify
trial. Acceptance requires a strict lexicographic improvement in failing area,
maximum failing patch, failing patch count and worst gap, while every
noncoverage hard-gate count is componentwise no worse. Otherwise the accepted
forest is restored and the exact row/reason is ledgered. The bounded attempt
cap never converts residual uncovered area into success.

Mandatory unrouted-contact normalization can expose a real defect hidden by a
discarded forest. The existing transactional Phase 8 final repair is therefore
enabled by default; explicitly disabling it may honestly leave final
verification false. Phase 8 and the structural resize that follows it are one
cloned transaction and one combined fixed point: resize-created geometry is
re-entered through Phase 8, and exceptions or cap exhaustion discard the clone
without partially mutating the accepted forest.
