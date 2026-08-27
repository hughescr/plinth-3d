# Cast and float-comparison audit — 2026-08-26

Companion to [`census-clippy.md`](census-clippy.md), written when the workspace
lint table was narrowed from the `pedantic` + `nursery` groups to a named set.
Every `#[expect(clippy::cast_*)]` and test-scoped `#[expect(clippy::float_cmp)]`
in the tree points here.  This file says which of them were **read** and which
are **carried debt**, so the difference is not left to the reader's charity.

## Why `expect` and not `allow`

`#[expect]` is `#[allow]` that complains when it stops being needed.  If a
cast below is later replaced by a checked conversion, its annotation becomes an
unfulfilled expectation and rustc says so, which means this backlog cannot rot
into a blanket that outlives the thing it was covering.  Nothing here is a
file-level or crate-level suppression; every annotation names its lints and sits
on the single function that raises them.

## The four classes

| Class | Functions | Read site by site? | Verdict |
|---|---:|---|---|
| float-to-integer casts (production) | 38 | **yes**, all 71 sites | no live defect; one guard hardened |
| `f64` → `f32` (production) | 4 | yes | deliberate: the printer file format stores binary32 |
| integer-width casts (production) | 71 | **no** — triaged as a class | one architecture item found and fixed; rest is carried debt |
| test/example/bench code (all classes) | 290 | no — triaged as a class | fixture arithmetic and exact pins over hand-written values |

### float-to-integer casts — READ, all 71 sites

These are the ones that can actually change a value in this domain: `as` from
a float truncates toward zero, saturates at the integer bounds, and maps NaN to
0, all silently.  Every site was read.  They fall into four shapes:

1. **Range-checked before the cast.** `plinth-core/src/chord.rs`,
   `plinth-hollow/src/lib.rs` (`span_index > (u32::MAX - 1) as f64`),
   `plinth-support/src/joints.rs` and `src/lib.rs` (`value < i64::MIN as f64 ||
   value > i64::MAX as f64`), `geom_kernel/src/parity.rs`,
   `plinth-mesh/src/topology.rs`, `plinth-orient/src/{grid,search}.rs`.
2. **Saturating cast, then clamped into range.** `plinth-hollow/src/lib.rs`
   voxel ranges, `plinth-support/src/brace.rs` rung counts.  Saturation is
   defined behaviour and the following `.max(lo)` / `.min(hi)` lands the value
   in range, so the pair is correct rather than accidental.
3. **Lattice cell keys** — `(x / quantum).floor() as i64` in
   `plinth-support/src/perception.rs`, `plinth-layers/src/lib.rs`,
   `plinth-hollow/src/{drilling,cavity_field}.rs`, `geom_kernel/src/parity.rs`.
   Bounded by the model bounds over an mm-scale quantum; saturation needs a
   coordinate around 1e18 mm.
4. **Counts from a span/pitch ratio** — non-negative by construction, feeding
   capacities that are separately capped.

**One defect found and fixed.** `plinth-hollow/src/drains.rs`
`sampled_exit_distances` wrote its bounds test as `coordinate < 0.0 ||
coordinate >= dims[axis] as f64`.  NaN fails BOTH comparisons, so an unordered
coordinate walked past a test meant to exclude it and reached `NaN as usize`,
which is a defined, silent `0` — a false in-bounds hit on voxel 0.  The guard
now names the unordered case first.  NaN is not reachable there from finite
candidate geometry, so this is a latent hole rather than a live bug; it is
fixed because the cost is a line and the failure would have been silent.

### integer-width casts — CARRIED DEBT, 71 functions

`census-clippy.md` triaged this class as a whole: mesh and voxel index
arithmetic whose values are bounded by mesh sizes far below any 32-bit ceiling,
plus **one architecture item** — `geom_kernel`'s BVH storing face and node
indices in `i32` fields with nothing asserting they fit.

**That item is fixed.** `rust/geom_kernel/src/bvh.rs` now derives
`MAX_FACES` from the storage (`2n - 1 <= i32::MAX`, the worst-case node count of
a binary tree whose leaves each hold at least one face) and refuses a larger
mesh at `Bvh::build`, where before the four `as i32` writes simply wrapped into
negative node indices with no error.  The ceiling is ~1.07e9 faces, about
2,500x the largest corpus model, so it refuses nothing real — but it is now a
named limit rather than an implied one, which is what the physics-first
constants doctrine asks for.

The remaining sites were **not** read individually.  That is the honest state:
they are annotated `TODO(cast-width-audit)` and listed below, so the debt is
visible and located rather than absorbed into the lint table.

| File | Line | Function | Lints |
|---|---:|---|---|
| `crates/plinth-core/src/chord.rs` | 61 | `try_from` | cast_possible_truncation,cast_sign_loss |
| `crates/plinth-decimate/src/lib.rs` | 1563 | `build` | cast_possible_truncation |
| `crates/plinth-decimate/src/lib.rs` | 1676 | `feature_protection_policy` | cast_possible_truncation |
| `crates/plinth-decimate/src/lib.rs` | 2920 | `query_batch` | cast_possible_truncation,cast_sign_loss |
| `crates/plinth-decimate/src/lib.rs` | 3017 | `uniform_index` | cast_possible_truncation |
| `crates/plinth-decimate/src/lib.rs` | 3188 | `prepare_welded_input` | cast_possible_truncation |
| `crates/plinth-decimate/src/lib.rs` | 3280 | `identity_outcome` | cast_possible_truncation |
| `crates/plinth-decimate/src/lib.rs` | 3326 | `compact_result` | cast_possible_truncation |
| `crates/plinth-decimate/src/lib.rs` | 3654 | `remove_zero_volume_face_pairs` | cast_possible_truncation |
| `crates/plinth-emit/src/lib.rs` | 535 | `build_records` | cast_possible_truncation |
| `crates/plinth-hollow/src/drains.rs` | 1220 | `label_chambers` | cast_possible_truncation |
| `crates/plinth-hollow/src/drains.rs` | 1328 | `low_lobes` | cast_possible_wrap |
| `crates/plinth-hollow/src/drilling.rs` | 1135 | `trapped_components` | cast_possible_wrap |
| `crates/plinth-hollow/src/drilling.rs` | 1662 | `drainability_map_admitted` | cast_possible_wrap |
| `crates/plinth-hollow/src/drilling.rs` | 1753 | `flat_if_in` | cast_sign_loss |
| `crates/plinth-hollow/src/flow.rs` | 259 | `downhill_flood` | cast_possible_truncation |
| `crates/plinth-hollow/src/flow.rs` | 371 | `linear_index` | cast_possible_truncation |
| `crates/plinth-hollow/src/lib.rs` | 2980 | `walk_triangle_candidates` | cast_sign_loss |
| `crates/plinth-hollow/src/well_composed.rs` | 305 | `linear_index` | cast_possible_truncation |
| `crates/plinth-mesh/src/delicacy_eval.rs` | 313 | `thinness_by_face` | cast_possible_truncation,cast_sign_loss |
| `crates/plinth-mesh/src/fixtures.rs` | 399 | `append_down_cone` | cast_possible_truncation |
| `crates/plinth-mesh/src/fixtures.rs` | 808 | `make_dual_cavity_pawn_with_throat_radius` | cast_possible_truncation |
| `crates/plinth-mesh/src/repair.rs` | 947 | `compact` | cast_possible_truncation |
| `crates/plinth-mesh/src/topology.rs` | 387 | `build_face_topology` | cast_possible_truncation |
| `crates/plinth-mesh/src/topology.rs` | 651 | `validate_face_topology` | cast_possible_truncation |
| `crates/plinth-mesh/src/topology.rs` | 823 | `certify_closed_components` | cast_possible_truncation |
| `crates/plinth-mesh/src/topology.rs` | 927 | `build` | cast_possible_truncation |
| `crates/plinth-mesh/src/topology.rs` | 979 | `exact_weld` | cast_possible_truncation |
| `crates/plinth-mesh/src/topology.rs` | 1017 | `epsilon_weld` | cast_possible_truncation |
| `crates/plinth-orient/src/section.rs` | 215 | `distinct_heights` | cast_possible_truncation |
| `crates/plinth-pipeline/src/coverage_evidence.rs` | 705 | `render_panel` | cast_possible_truncation |
| `crates/plinth-support/src/anchor.rs` | 1169 | `discover_anchor_seats_bound` | cast_possible_truncation,cast_sign_loss |
| `crates/plinth-support/src/cco.rs` | 1375 | `current` | cast_possible_truncation |
| `crates/plinth-support/src/forest.rs` | 541 | `add_member` | cast_possible_truncation |
| `crates/plinth-support/src/forest.rs` | 788 | `live_members` | cast_possible_truncation |
| `crates/plinth-support/src/perception.rs` | 1196 | `connected_islands` | cast_possible_truncation |
| `crates/plinth-support/src/perception.rs` | 2010 | `overlap_candidates` | cast_possible_truncation |
| `crates/plinth-support/src/perception.rs` | 2566 | `filter_minima_to_exterior_faces` | cast_possible_truncation |
| `crates/plinth-support/src/perception.rs` | 2998 | `insert_load_witness` | cast_possible_truncation |
| `crates/plinth-support/src/perception.rs` | 3239 | `finish` | cast_possible_truncation |
| `crates/plinth-support/src/perception.rs` | 3793 | `record_commit` | cast_possible_truncation |
| `crates/plinth-support/src/perception.rs` | 4162 | `finalize_retained_round` | cast_possible_truncation |
| `crates/plinth-support/src/perception.rs` | 4330 | `from_witness_for_test` | cast_possible_truncation |
| `crates/plinth-support/src/perception.rs` | 4488 | `finalize_load_rows` | cast_possible_truncation |
| `crates/plinth-support/src/perception.rs` | 6066 | `assemble` | cast_possible_truncation |
| `crates/plinth-support/src/perception.rs` | 6226 | `merged_violation_count` | cast_possible_truncation |
| `crates/plinth-support/src/specialized.rs` | 2010 | `prepare_specialized_contacts` | cast_possible_truncation,cast_sign_loss |
| `crates/plinth-support/src/specialized.rs` | 2849 | `regular_dispositions` | cast_possible_truncation |
| `crates/plinth-support/src/specialized.rs` | 3258 | `consolidate` | float_cmp |
| `rust/chitubox_io/src/parse.rs` | 232 | `parse_support` | cast_possible_truncation |
| `rust/chitubox_io/src/project.rs` | 778 | `encode_opaque` | cast_possible_truncation |
| `rust/chitubox_io/src/write.rs` | 27 | `serialize` | cast_possible_truncation,cast_possible_wrap,cast_sign_loss |
| `rust/geom_kernel/src/lib.rs` | 981 | `ordered_index_map_is_identical_at_one_four_and_sixteen_threads` | cast_possible_truncation |
| `rust/geom_kernel/src/mesh.rs` | 191 | `from_parts` | cast_possible_truncation |
| `rust/geom_kernel/src/parity.rs` | 358 | `build_with_reserved_peak` | cast_possible_truncation |
| `rust/geom_kernel/src/queries.rs` | 136 | `next` | cast_possible_truncation,cast_possible_wrap |
| `rust/geom_kernel/src/queries.rs` | 1218 | `nearest_one_with_stack` | cast_possible_truncation,cast_possible_wrap,cast_sign_loss |
| `rust/geom_kernel/src/queries.rs` | 1280 | `query` | cast_possible_wrap |
| `rust/geom_kernel/src/queries.rs` | 1311 | `project` | cast_possible_wrap |
| `rust/geom_kernel/src/queries.rs` | 1404 | `nearest_allowed_one_with_stack` | cast_possible_truncation,cast_possible_wrap,cast_sign_loss |
| `rust/geom_kernel/src/queries.rs` | 1512 | `project_allowed_faces_batch` | cast_possible_truncation,cast_sign_loss |
| `rust/geom_kernel/src/queries.rs` | 1755 | `collect_hits_with_stack` | cast_sign_loss |
| `rust/geom_kernel/src/queries.rs` | 1819 | `count_hits_with_stack` | cast_sign_loss |
| `rust/geom_kernel/src/queries.rs` | 1941 | `cast_rays` | cast_possible_wrap |
| `rust/geom_kernel/src/queries.rs` | 2043 | `seg_surface_min_excluding` | cast_possible_truncation,cast_possible_wrap,cast_sign_loss |
| `rust/geom_kernel/src/queries.rs` | 2458 | `faces_near_segment` | cast_sign_loss |
| `rust/geom_kernel/src/queries.rs` | 3230 | `faces_at_distance` | cast_sign_loss |
| `rust/geom_kernel/src/slicing.rs` | 228 | `visit_z_candidates` | cast_sign_loss |
| `rust/geom_kernel/src/testkit.rs` | 191 | `brute_allowed_projection` | cast_possible_truncation |
| `rust/geom_kernel/src/testkit.rs` | 312 | `surf_bvh_signature` | cast_sign_loss |
| `rust/geom_kernel/src/testkit.rs` | 361 | `brute_moller_hits` | cast_possible_truncation |

### float-to-integer sites, located

| File | Line | Function | Lints |
|---|---:|---|---|
| `crates/plinth-core/src/chord.rs` | 163 | `canon_chord_micron` | cast_possible_truncation |
| `crates/plinth-decimate/src/lib.rs` | 1447 | `select_feature_protection_from_errors` | cast_possible_truncation,cast_sign_loss |
| `crates/plinth-decimate/src/lib.rs` | 3036 | `linear_percentile` | cast_possible_truncation,cast_sign_loss |
| `crates/plinth-decimate/src/lib.rs` | 3234 | `target_face_count` | cast_possible_truncation,cast_sign_loss |
| `crates/plinth-hollow/src/cavity_field.rs` | 183 | `contains_point_neighbourhood` | cast_possible_truncation,cast_sign_loss |
| `crates/plinth-hollow/src/drains.rs` | 898 | `boundary_points` | cast_possible_truncation,cast_sign_loss |
| `crates/plinth-hollow/src/drains.rs` | 951 | `points_per_candidate` | cast_possible_truncation,cast_sign_loss |
| `crates/plinth-hollow/src/drains.rs` | 1074 | `sampled_exit_distances` | cast_possible_truncation,cast_sign_loss |
| `crates/plinth-hollow/src/drains.rs` | 1586 | `planner_preflight` | cast_possible_truncation,cast_sign_loss |
| `crates/plinth-hollow/src/drilling.rs` | 1213 | `sampled_exit_along` | cast_possible_truncation,cast_sign_loss |
| `crates/plinth-hollow/src/drilling.rs` | 1770 | `grid_index_ties_even` | cast_possible_truncation |
| `crates/plinth-hollow/src/lib.rs` | 2418 | `grid_spec_from_bounds` | cast_possible_truncation,cast_sign_loss |
| `crates/plinth-hollow/src/lib.rs` | 2707 | `dilated_index_aabb` | cast_possible_truncation |
| `crates/plinth-hollow/src/lib.rs` | 2923 | `candidate_index_span` | cast_possible_truncation |
| `crates/plinth-layers/src/lib.rs` | 272 | `quantize` | cast_possible_truncation |
| `crates/plinth-mesh/src/topology.rs` | 1085 | `grid_cell` | cast_possible_truncation |
| `crates/plinth-orient/src/grid.rs` | 198 | `count` | cast_possible_truncation,cast_sign_loss |
| `crates/plinth-orient/src/search.rs` | 202 | `whole` | cast_possible_truncation,cast_sign_loss |
| `crates/plinth-pipeline/src/prepare.rs` | 1740 | `seal_and_emit` | cast_possible_truncation,cast_sign_loss |
| `crates/plinth-support/src/brace.rs` | 1071 | `compute_needs` | cast_possible_truncation,cast_sign_loss |
| `crates/plinth-support/src/brace.rs` | 3039 | `rungs_that_fit` | cast_possible_truncation,cast_sign_loss |
| `crates/plinth-support/src/cco.rs` | 5579 | `candidate_price` | cast_possible_truncation,cast_sign_loss |
| `crates/plinth-support/src/dyadic.rs` | 496 | `refine_depth` | cast_possible_truncation,cast_possible_wrap,cast_sign_loss |
| `crates/plinth-support/src/joints.rs` | 1180 | `point_cell` | cast_possible_truncation |
| `crates/plinth-support/src/lib.rs` | 865 | `perceive_contacts_on_surface` | cast_possible_truncation,cast_sign_loss |
| `crates/plinth-support/src/lib.rs` | 2174 | `rank_coverage_repair_rows` | cast_possible_truncation,cast_sign_loss |
| `crates/plinth-support/src/lib.rs` | 2286 | `key` | cast_possible_truncation |
| `crates/plinth-support/src/lib.rs` | 2357 | `key` | cast_possible_truncation |
| `crates/plinth-support/src/perception.rs` | 1341 | `point_key` | cast_possible_truncation |
| `crates/plinth-support/src/perception.rs` | 2225 | `find_local_minima` | cast_possible_truncation,cast_sign_loss |
| `crates/plinth-support/src/perception.rs` | 5078 | `carriable_share_count` | cast_possible_truncation,cast_sign_loss |
| `rust/geom_kernel/src/bvh.rs` | 86 | `build` | cast_possible_truncation,cast_sign_loss |
| `rust/geom_kernel/src/bvh.rs` | 152 | `build_range` | cast_possible_truncation,cast_possible_wrap,cast_sign_loss |
| `rust/geom_kernel/src/parity.rs` | 137 | `checked_grid_layout` | cast_possible_truncation |
| `rust/geom_kernel/src/parity.rs` | 196 | `projected_cell_bounds` | cast_possible_truncation |
| `rust/geom_kernel/src/parity.rs` | 610 | `inside_point` | cast_possible_truncation |
| `rust/geom_kernel/src/parity.rs` | 711 | `grid_dimension_rejects_saturating_quotient_without_debug_release_divergence` | cast_possible_truncation |
| `rust/geom_kernel/src/queries.rs` | 2517 | `pen_intervals_one` | cast_possible_truncation,cast_sign_loss |

### `f64` → `f32`, located

| File | Line | Function | Lints |
|---|---:|---|---|
| `crates/plinth-decimate/src/lib.rs` | 3243 | `quantize_positions` | cast_possible_truncation |
| `crates/plinth-emit/src/lib.rs` | 716 | `f32_of` | cast_possible_truncation |
| `crates/plinth-mesh/src/fixtures.rs` | 1016 | `snap_vertex` | cast_possible_truncation |
| `crates/plinth-mesh/src/io.rs` | 194 | `encode_binary_stl` | cast_possible_truncation |

### test, example and bench code — 290 functions

Not listed site by site.  Casts here are fixture arithmetic over hand-written
values, and the `float_cmp` hits are exact pins: a voxel count, a frozen
fixture, a coordinate the test itself put in and expects back unchanged.  An
exact pin is what those assertions are FOR — replacing them with tolerances
would weaken the tests, which OD-8 forbids.  The census reached the same
conclusion by checking flagged line numbers against each file's `mod tests`
boundary.

## What is NOT covered by an annotation

The 29 production `float_cmp` sites are annotated individually, each with its
own reason at its own function, not with a shared class reason — they are the
class the census found live defects in, so each argument is stated on its own.
Three the census singled out were re-read and cleared:

- `plinth-emit`'s `same_point` (pad vs bridge): a raft pad is authored by
  passing one root's xy for BOTH ends (`raft.rs`, `raft_primitive(root.xy_mm,
  root.xy_mm, ..)`), so pad-ness is identity on a copy.  A bridge whose ends
  differ only by rounding dust does not degenerate silently either —
  `raft_bridge_triangles` rejects it on its own `length <= f64::EPSILON` guard.
- `cco.rs`'s plate-descent test, which the census read as
  `member.p1_mm()[2].get() == request.plate_z_mm().get()`, is now
  `plate_descent_reaches_plate` over `Finite` values.  The producer
  (`cco.rs`, `let bottom = [xy[0], xy[1], request.plate_z_mm().get()]`) copies
  the plate height verbatim into the endpoint, so bit-equality holds by
  construction and the branch cannot be routed wrong by a rounding difference.
- `plinth-metrics`'s `RealInterval::overlaps` raised no `float_cmp` warning
  under the narrowed table and was not re-litigated here.

