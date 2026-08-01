# 01 — The Current Python System (map of what is being replaced)

Audience: you, Codex, with repo access and no conversation history. This document is the
authoritative map of the Python system. Read it before writing any Rust. It is self-sufficient;
links to `../reference/*` and `../specs/*` are supplemental depth only, never prerequisites.

Sibling docs: `README.md` (why and how to read this set), `02-target-architecture.md`,
`03-physics-and-constants.md`, `04-migration-plan.md`, `05-chitubox-and-corpus.md`,
`06-conventions.md`.

**Why this migration exists (state it back if asked):** it is not primarily a performance
decision. The owner's rationale, near-verbatim: *"eliminate all python because it cripples
Claude's ability to think well… every exposure to python actively destroys productivity."* The
public Python corpus trains coding agents into scalar-loop, unbatched habits. Removing Python is a
guardrail on all future agent work in this repo. Performance is a welcome side effect.

**Scale.** `scripts/support_placement/` is ~26,900 lines of Python; `scripts/tools/` ~2,880;
`scripts/tests/` manual + helper files ~1,420. Two Rust crates already exist
(`rust/geom_kernel`, `rust/chitubox_io`) and are consumed from Python through PyO3.

---

## 1. End-to-end data flow

Everything below is one model in, one `.chitubox` out. Plates, slicing and printer transfer do
not exist yet in any form — do not look for them.

```
                    scripts/tools/prep_one.py            <-- THE user-facing CLI
                              |                              (JSONL on stdout, exit codes 0-5)
                              v
             support_placement.pipeline.prep_one(stl, out, ...)
                              |
   +----------- stage 1 "hollow" -----------+ hollow.run(in, out, wall, orient=True, cfg)
   |   hollow.triage_components   (step 0: strip modelling shards; refuse pre-hollowed)
   |   hollow.orient_search       (step 1: ORIENTATION lives HERE — 13 candidates)
   |   voxelize + EDT + cavity    (steps 2-5: PITCH=0.5mm, wall default 2.5mm)
   |   drain / vent drilling      (step 6: per-lobe exit rays)
   |   trapped_map drainability   (step 7: bottom-up union-find; refill or extra drains)
   |   marching cubes + boolean   (step 8a: skimage + trimesh/manifold difference)
   |   5-check verify then export (step 8b: writes <stem>.prepped.stl)
   +----------------------------------------+
                              |  (the .prepped.stl on disk is the model-fidelity contract;
                              |   pipeline re-LOADS it for the next stage)
                              v
   +----------- stage 2 "supports" ---------+ engine.run(mesh, cfg, log)
   |   geometry_kernel.KernelSurface(mesh)   <-- Rust geom_kernel BVH oracle, built first
   |   engine.prepare  -> topology.build_face_topology
   |                      perception.overhang_islands / island_births / local_z_minima
   |                      placement.candidate_pool / sample_candidates
   |                      coverage.build_coverage_samples
   |                      specialized_contacts.* (minima + island-birth demand)
   |   engine.construct-> placement.load_partition / tip_radius_mm
   |                      routing.build_surface_tree ; cco.build (CCO insertion)
   |   rewire.optimize (topology local search)
   |   engine.brace_and_size -> transient.transient_report / bracing.propose_and_place
   |                            transient.resize_forest ; rewire.optimize AGAIN
   |   phase8_driver.run_phase8 (relocation/repair, once, immediately before final verify)
   |   verify.self_verify  <-- the ~11-gate acceptance AND
   |   joints.collect_members / compute_joint_balls ; tree.volume_report
   |   ALL WRAPPED IN engine.run's bounded coverage-repair fix loop (<=8 attempts,
   |   each attempt rebuilding the ENTIRE forest)
   +----------------------------------------+
                              |
   +--- internal.plan_and_verify (A5) ------+ (hollow-cavity internal supports; own gate set.
   |   on refusal pipeline COPIES the solid backup over the already-written .prepped.stl
   |   and re-runs the whole exterior engine — see DEFECT D-14)
                              v
   +----------- stage 3 "emit" -------------+ emit.emit_project(model_stl, result, out, cfg)
   |   chitubox_emit.member_chain_specs      (frustum/sphere triangle synthesis, Python)
   |   chitubox_emit.bottom_connected_specs  (pads + deterministic MST raft bridges)
   |   emit.support_arrays                   (ONE columnar numpy pack)
   |   chitubox_io._native.write_project     (PyO3 -> Rust: Project::new_empty/add_models/
   |                                          attach_supports/save + independent byte audit)
   |   chitubox_io._native.read_project      (read back for the EmitReport)
   +----------------------------------------+
                              v
                     <stem>.chitubox  +  <stem>.pipeline.json
```

Runtime today: 86–872 s per model, median ~402 s.

### Entry points that actually exist

| Entry point | Path | Calls | Status |
|---|---|---|---|
| `prep_one.py` | `scripts/tools/prep_one.py` | `pipeline.prep_one` / `prep_batch` | The real CLI. Only path that runs hollow+orient+supports+emit. |
| `hollow_drill.py` | `scripts/tools/hollow_drill.py` | `hollow.run` | Hollow/orient only. Thin CLI, no logic. |
| `emit_supports.py` | `scripts/tools/emit_supports.py` | `mesh_io.load_stl` → `engine.run` → `emit.emit_project` | **BYPASS.** Skips hollow, drilling and orientation entirely. |
| `run_v5.py`, `run_engine_full.py`, `recheck_rewire_cap.py` | `scripts/tools/` | `engine.run` directly | Instrumented ad-hoc runners, same bypass shape. |
| `decimate_pipeline.py` | `scripts/tools/` | Blender `bpy` only | Not called by any Python in the repo. Manual, out-of-band. |

Two consequences you must design away (see `02-target-architecture.md`): **the Rust CLI has
exactly one path**, and **orientation is its own stage**, not a side effect of hollowing.

---

## 2. Cluster: engine-core — orchestration and the fix loop

Files: `scripts/support_placement/{engine,pipeline,p8_accept,prepared_validation,corpus}.py`
(~4,240 LOC).

**Purpose.** `pipeline.prep_one` composes hollow → supports → emit for ONE model and always writes
a `PipelineReport` to disk with a stable exit code. `engine.py` is the support engine proper:
`prepare` → `construct` → `rewire` → `brace_and_size` → optional Phase-8 `relocate` →
`verify.self_verify` → `finish`, wrapped in a bounded coverage-repair loop.

**Public API actually imported elsewhere**

| Symbol | Consumers |
|---|---|
| `pipeline.prep_one(stl_path, out_path, *, cfg=None, wall_mm=2.5, orient=True, hollow=True, prepped_stl=None, name=None, placement_xy=(0,0), lift_mm=None, allow_failed_gates=False, skip_existing=False, report_path=..., progress=None, internal_support_verifier=None) -> PipelineReport` | `tools/prep_one.py`, tests |
| `pipeline.prep_batch(stl_paths, out_dir, *, progress=None, **kwargs)` | `tools/prep_one.py` |
| `pipeline.PipelineReport` / `StageResult` / `PipelineError`; `STAGES=('hollow','supports','emit')`; `EXIT_OK/INPUT/HOLLOW/GATES/EMIT/SUPPORTS = 0/1/2/3/4/5` | CLI, tests |
| `engine.run(mesh, cfg, log=None) -> dict` (islands, contacts, carried_area_mm2, tip_r_mm, forest, members, balls, unrouted, minima, plate_z, verify, volume, log, island_births, coverage_failed_candidate_pool_rows) | `pipeline`, `emit_supports`, `run_v5`, `run_engine_full`, `recheck_rewire_cap` |
| `engine.prepare / construct / brace_and_size / relocate / finish / live_member_lists` | internal + `p8_accept`, tests |
| `engine.validate_prepared_state`, `mesh_fingerprint`, `canonical_config_json`, `config_fingerprint`; `PREPARED_STATE_SCHEMA_VERSION = 3` | prepared-state checkpointing |
| `p8_accept.final_geometry_verify / zero_new_firm_transit_gate / raft_connectivity_report / criticality_weighted_unresolved_gate / idempotence_gate / geometry_digest` | **tests only.** `phase8_driver` imports the module but uses only `geometry_digest`. |
| `prepared_validation._issue_prepared_validation_token / _authorizes_prepared / _authorizes_domains` | `engine`, `placement`, `verify`, `phase8_driver` |
| `corpus.analyze / target_spacing_upper_bound` | `test_corpus.py`, `manual_run_treant.py` only |

**Algorithms worth knowing**

- **Prepared-state fingerprint + capability token.** SHA-256 mesh fingerprint
  (`topology.mesh_fingerprint`) + canonical-JSON config fingerprint, then an identity-bound,
  unpicklable token so multi-million-row sidecars are deep-validated once per run. In Rust this is
  just ownership: a `Validated<PreparedState>` newtype constructible only by the validator. The
  whole module disappears.
- **Bounded coverage-repair fix loop** (`engine.run`, the `while True` at ~line 1177): baseline
  build, then ≤ `cfg.coverage_repair_max_attempts` (8) trial builds, each adding ONE regular
  contact at the highest-marginal-uncovered-area candidate-pool row; accept/rollback by a
  lexicographic verdict; terminates `covered` | `attempt-cap` | `eligible-pool-exhausted`.
  Each attempt rebuilds the entire forest — this is the single biggest driver of the 86–872 s
  runtime and the reason orientation search was cut to 13 candidates.
- **A4 marginal-coverage ranking** (`engine._rank_coverage_repair_rows`): per (island, topology
  component) tiled candidate×sample squared-distance matrices under
  `coverage_score_matrix_max_entries` (65,536); marginal area = boolean cover matrix @ sample area
  weights; ties break to the lower pool row.
- **A4 acceptance verdict** (`_coverage_repair_trial_verdict` + `_coverage_objective`):
  lexicographic (`failing_uncovered_area_mm2` sum, `max_patch_area_mm2`, `n_failing_patches`,
  `worst_gap_mm`) must strictly decrease, AND every non-coverage hard-gate count in
  `_NONCOVERAGE_GATE_KEYS` must not increase.
- **Specialized contact demand.** `_ensure_minima_contacts` / `_ensure_island_birth_contacts`:
  exact nearest-triangle attribution, many-to-many face→island ownership CSR
  (`_FaceIslandOwnership`), component-clipped synthetic islands (1.5 mm cKDTree ball).
  Island births cluster by union-find over 3D euclidean ≤ `_ISLAND_BIRTH_LINK_R_MM` (0.6 mm), then
  a greedy z-sorted ladder cover.
- **Synthetic-island minimum regular cover.** Bitmask set cover over packed Python ints
  (`np.packbits`), exhaustive k=1 and k=2, then pivoted branch-and-bound exact cover under a node
  budget, with a deterministic greedy upper bound as fail-closed fallback. In Rust: fixed-width
  `u64` bitsets, which deletes `_synthetic_selector_resource_plan`'s Python-int byte accounting
  entirely.
- **Phase-8 relocate transaction.** Deep-copied working forest; commit by
  `forest.__dict__.clear(); update(...)` preserving caller object identity. Do not reproduce —
  return an owned value from the stage.
- **Determinism is a tested contract.** `test_engine.py` pins
  `test_engine_construct_is_deterministic_despite_global_rng_state`,
  `test_engine_prepare_island_births_deterministic_despite_global_rng_state`,
  `test_run_bit_identical_across_thread_counts_on_table`. Every tie-break is explicit (lowest pool
  row, lowest face id, sorted row tuples). Parallelism must not reorder reductions.

**Already in Rust.** `geom_kernel` is the production oracle (`engine.run` builds
`geometry_kernel.KernelSurface(mesh)` first and threads `geom` through every stage). Nothing else
in this cluster is ported: the pipeline/exit-code contract, the prepared-state schema, the fix
loop, the set-cover selector, `brace_and_size`, `relocate`, `finish`, and all of `p8_accept` are
new-build work.

**Interface contracts to preserve.** The `<stem>.pipeline.json` report keys (`input_stl`,
`out_path`, `prepped_stl`, `status`, `exit_code`, `failed_stage`, `error`, `seconds`, `settings`,
`report_path`, `stages[].{name,status,seconds,report,error}`) are consumed by the CLI and tests.
The `<stem>.prepped.stl` intermediate is an OUTPUT re-read from disk by later stages. The
model-centered frame convention — pipeline translates by `-(bounds[0]+bounds[1])/2` before
`engine.run`, and emit re-derives the same center from the same STL — must be reproduced exactly
or supports land offset.

---

## 3. Cluster: placement-fields — perception, candidate pools, sizing physics

Files: `scripts/support_placement/{placement,coverage,perception,config,specialized_contacts}.py`
(~3,500 LOC).

**Purpose.** Generate and size the physical contact layer. `config.py` is the single 117-field
`SupportConfig` dataclass threaded everywhere as `cfg`, with per-field prose docstrings as the
only provenance record. `perception.py` finds overhang islands, local z-minima, slice-based island
births, and the delicacy/scar-cost field. `placement.py` builds the deterministic legal candidate
seat pool, greedily samples contacts, partitions load, and sizes tips and shafts.
`specialized_contacts.py` reconciles overlapping logical demands onto shared physical seats.
`coverage.py` builds the immutable dyadic-sampled coverage dataset (A3).

**Public API actually imported elsewhere**

| Symbol | Consumers |
|---|---|
| `config.SupportConfig` (+ `.notes()`, `.as_dict()`) | every module |
| `perception.down_facing_mask / grounded_mask / overhang_islands / local_z_minima / island_births / island_birth_xy_ok / delicacy_field / exterior_anchor_pool`; `perception.ISLAND_ATTACH_REACH_MM = 0.2` | `engine`, `verify`, `hollow`, `tree`, `mesh_io` |
| `placement.candidate_pool / sample_candidates / validate_candidate_pool / load_partition / tip_radius_mm` | `engine`, `verify` |
| `placement._shaft_r_for_L(L_mm, cfg, angle_from_vertical_deg=0.0)`, `max_unbraced_len_mm`, `shaft_radius_for_length_mm` | `cco`, `trunks`, `bracing`, `braces`, `rewire`, `transient` — underscored but effectively public |
| `coverage.build_coverage_samples / validate_coverage_samples` | `engine`, `verify`, `joints` |
| `specialized_contacts.is_specialized / logical_demands / owner_island_ids / coverage_routes / candidate_satisfies_demand / compatible_representatives / merged_contact / attach_to_exact_regular / exact_duplicate_seat_pairs / shared_contact_payload_violations` | `engine`, `verify` |

**Algorithms worth knowing**

- Overhang islands: face-adjacency connected components (scipy `csgraph`) merged across a
  `island_merge_radius_mm` (2.0 mm) spatial closing via `cKDTree.query_pairs`.
- Grounded/self-supported detection by **column-cached** ray casting — one ray per occupied XY grid
  column, not per face.
- `local_z_minima`: per-vertex 1-ring undercut test + bounded BFS flood in a tol-height band,
  terminating on a strictly-lower neighbour; then two independent post-filters
  (`minima_drop_interior_faces`, `minima_require_island_birth` + an adequacy test that slices one
  layer below and compares carried cross-section to a tip footprint).
- `island_births`: **slice-based ground truth** via `trimesh.section_multiplane` at the real layer
  height, with an xy-overlap test between consecutive layers (bbox reject → vertex-in-polygon →
  edge-segment intersection). This is the least trivial dependency in the whole port: exact polygon
  slicing of a triangle mesh at a list of z heights returning closed loops with correct
  outer/hole winding. `geom_kernel` has no slicing. Design it, don't assume a crate.
- `delicacy_field`: blend of discrete Gaussian curvature (subsampled + KNN-propagated) and local
  thinness (KDTree opposite-normal proxy). `trimesh.curvature.discrete_gaussian_curvature_measure`
  also has no in-repo Rust equivalent.
- **Deterministic dyadic (1→4 midpoint) triangle refinement** with a two-pass preflight/refusal
  budget before any allocation — implemented TWICE, in `placement.candidate_pool` and
  `coverage._build_coverage_samples_inner` (see DEFECT D-19).
- Greedy nearest-first spacing-constrained selection over the legal pool, farthest-point fallback
  for small islands.
- `load_partition`: exact nearest-eligible-contact face assignment (replaced an older
  seed-dependent Monte Carlo).
- `tip_radius_mm`: green-state tensile fuse, `r = sqrt(F·SF/(π·σ))`, clamped to
  `[tip_r_min_mm, tip_r_max_mm] = [0.2, 1.0]`.
- `_shaft_r_for_L`: cantilever point-load deflection `r = (4FL³/(3E·π·δ))^0.25` with an
  angle-aware peel-load fixed-point iteration (F depends on r when leaning);
  `max_unbraced_len_mm` is its closed-form inverse.
- `shaft_radius_for_length_mm`: brace-then-thin slenderness policy — add braces to shorten the
  effective free length rather than exceed `max_unbraced_shaft_r_mm` (0.9 mm).

**Determinism.** `sample_candidates` and `load_partition` are seed-free and test-pinned as such.
`delicacy_field` is the one function that seeds the GLOBAL numpy RNG for `mesh.sample()` — make it
an explicit seeded generator parameter in Rust.

**Already in Rust.** Nothing. `geom_kernel` has no notion of islands, delicacy, candidate pools or
sizing physics; `chitubox_io` is file I/O. This cluster is entirely new-build.

Constant values and their provenance are catalogued in `03-physics-and-constants.md`, which is the
migration checklist for `config.py`'s 117 fields.

---

## 4. Cluster: structure-growth — the support forest

Files: `scripts/support_placement/{tree,trunks,joints,bracing,braces,rewire,cco,routing,struts,chord_canon}.py`
(~6,770 LOC).

**Purpose.** Turn accepted contact terminals into a physically buildable forest: grow topology from
each contact to the plate or a model anchor (`cco.py` on `tree.py`'s Member/Forest model), improve
it by transactional local search (`rewire.py`), add lateral bracing against build-time transient
cantilever spans (`bracing.py`, driven by `transient.py`), and collapse every junction into one
physically-correct sphere with an enclosure-certificate volume diet (`joints.py`).

**Public API actually imported elsewhere**

| Symbol | Consumers |
|---|---|
| `tree.Member(p0,p1,r0,r1,kind,owner,embed_end=None,carried_mm2=0.0)`; `tree.Forest` with `add_member/add_brace/set_parent/mark_dead/mark_quarantined/checkpoint/rollback/commit/dirty_members/revalidate` | `cco`, `rewire`, `bracing`, `joints`, `engine`, `verify` |
| `tree.assert_all_certified(forest)` | `engine` (6 call sites) |
| `tree.volume_report`, `tree.printability_report` | `engine`, `verify`, `bracing` |
| `cco.build(mesh, contacts, tip_r, carried, minima_flags, plate_z, surface_tree, cfg, ...)` | `engine` |
| `cco.xia_angles`, `_plate_descent_builders`, `_model_anchor_builders`, `_bifurcation_builders`, `_propagate_murray`, `_member_vol_ml`, `_lean_deg`, `_within_cone`, `_r_tension_mm` | `rewire`, `bracing`, tests |
| `rewire.optimize(forest, mesh, surface_tree, cfg, log=None)` | `engine` (twice) |
| `joints.collect_members`, `joints.compute_joint_balls`, `joints.MODEL_FOOT_FACTOR` | `engine`, `verify`, `emit` |
| `bracing.propose_and_place(...)` | `engine` |
| `routing.build_surface_tree(mesh, pitch_mm, max_points, seed)`, `surface_tree_pitch_bound_mm`, `query_workers`, legacy `sample_route_envelope/check_clearance` | `engine`, `tree`, `verify`, `removal`, `trunks`, `braces` |
| `struts.find_model_anchor_candidates` | `cco` (the only production use of `struts.py`) |
| `chord_canon.canon_chord_micron / chords_match_micron / chord_parity_report` | `p8_accept` only — no call edge into this cluster |

**Algorithms worth knowing**

- **Incremental CCO insertion** (`cco.build`/`_insert_terminal`): price every candidate route family
  cheaply with no revalidation, sort, then trial cheapest-first until one clears clearance.
- **Xia (α=1/2) closed-form bifurcation angles** + Weiszfeld weighted-Fermat junction polish.
- **Murray's law radius propagation** up the ancestor chain on every bifurcation/reattach
  (`_propagate_murray`; inverse in `rewire._slim_ancestors`).
- **Transactional topology mutation** (`rewire.optimize`): subtree reattach, junction slide
  hill-climb, twig prune, degenerate retire, with a diminishing-returns stopping rule.
- **Certificate-based dirty/clean tracking** (`Member.cert_valid`, `Forest.revalidate`): gather all
  dirty members → ONE batched kernel call → scatter. Preserve this shape exactly.
- **Exact signed distance to a tapered frustum** via a 2D (axial, radial) trapezoid reduction,
  feeding a rigorous recursive spherical-patch enclosure certificate that decides whether a joint
  ball is fully covered by touching members and can be dropped. Never sampling. Doctrine:
  uncertified anywhere ⇒ KEEP the ball.
- `Forest` is **append-only**: rollback truncates from the tail only, which is why dead/quarantined
  marking exists instead of deletion. An arena/generational-index design in Rust could lift this,
  but only deliberately.
- **Endpoint convention is load-bearing**: `p1` is the root-ward/exit end, `p0` the terminal-ward
  entry end. `verify.murray_report` checks `parent.r0**4 >= Σ child.r1**4` on exactly this. A real
  past bug used `max(r0,r1)` and silently produced 2.3× oversized members. Encode this in the type
  system, not a comment.
- **Bit-identical arithmetic constraint**: `joints._batch_dist_and_local_radius` must use
  component-wise `x*x+y*y+z*z` rather than `@`/`einsum` — einsum differed in 3275/240000 cases.
  Any Rust replacement must pin operand order; do not hand this to a reduction whose associativity
  is unspecified.
- No RNG anywhere in this cluster; every sort has a stable tuple tie-break ending in the index.

**Already in Rust.** Only the clearance *query primitives* these modules call
(`segment_clearance`, `penetration_intervals`, `project`, `cast_rays`, `sphere_breach`,
`exterior_face_mask`) via `geom_kernel`. The tree-growth, sizing, rewire, bracing and joint-ball
algorithms are pure Python/numpy with no Rust counterpart.

---

## 5. Cluster: verify-gates — the acceptance authority

Files:
`scripts/support_placement/{verify,geometry_exact,transient,topology,internal,removal}.py`
(~4,890 LOC).

**Purpose.** `verify.py` holds every hard gate that ANDs into `self_verify(...)["ok"]`, plus the
SCAR-AREA-CAP / PRINT-FLOOR embed-legality certificate shared with Phase-8. `geometry_exact.py` is
the frozen numpy oracle `geom_kernel` was ported from. `transient.py` is the build-time honesty
model. `topology.py` builds the exact shared-edge face-adjacency sidecar. `removal.py` is a
report-only accessibility model. `internal.py` is the self-contained A5 internal-cavity planner
with its own gate set.

**The gate list.** `self_verify` (verify.py:1666-1671) ANDs **ten** lists into `ok`
unconditionally: `uncovered_islands`, `clearance_violations`, `load_violations`,
`unsupported_minima`, `uncovered_island_births`, `regular_contact_invariant_violations`,
`unrouted_specialized_contacts`, `connectivity_violations`, `printability_violations`,
`murray_violations`. It ANDs an eleventh, `ball_breach_violations`, under `gate_on_ball_breach`
(verify.py:1691, **defaults True**), and `removal_violations` under `gate_on_removal`
(verify.py:1672, defaults False and no shipped caller passes True — permanently inert). A
**twelfth** gate, `physical_contact_invariant_violations`, does not appear in `verify.py` at all:
it is produced and applied engine-side by `engine._apply_physical_contact_gate`
(engine.py:1017-1024), which sets `report["ok"] = False` *after* `self_verify` has already
returned. So the effective acceptance set is twelve gates, eleven of them inside `self_verify`.

**Public API actually imported elsewhere**

| Symbol | Consumers |
|---|---|
| `verify.self_verify(mesh, islands, contacts, members, carried_area_mm2, tip_r_mm, cfg, plate_z, ...)` | `engine` (4 call sites) |
| `verify.coverage_report / clearance_report / connectivity_report / load_report / minima_report / island_birth_report / murray_report / ball_breach_report / regular_contact_invariant_report` | `engine`, `phase8_driver` |
| `verify.certify_member / certify_members_full_resolution / _batch_axis_intervals / _embed_scar_verdicts / _transit_classify / _tip_iv_index / member_rays / _scar_area_cap_legal` | `phase8.py`, `phase8_driver.py`, `tools/p11_crossexam.py` — **the underscore is a lie**, this is the real module boundary |
| `verify.orientation_scar_cost(mesh, cfg, rotations, delicacy=None)` | `hollow.orient_search` (live in production, `orient_scar_weight` defaults 0.4) |
| `geometry_exact.ExactSurface`, `closest_on_tri`, `member_rays`, `dedup_loc`, `perp_basis`, `VerticalParity`, `ParityVote` | `verify`, `engine`, kernel-parity tests |
| `transient.transient_worst_span / derive_brace_attachments / transient_report / r_transient_mm / resize_forest` | `engine`, `bracing`, `rewire`, `removal` |
| `topology.build_face_topology / validate_face_topology / mesh_fingerprint / FaceTopology / TOPOLOGY_CUT_*` | `engine`, `verify`, `perception`, `coverage`, `phase8_driver` |
| `removal.removal_report / forest_trees` | `verify` (always computed, never gates) |
| `internal.plan_and_verify(*, mesh, result, cfg, hollow_report, cavity, mesh_center)` | `pipeline` |

**Algorithms worth knowing**

- **A3 area-weighted coverage with exact-topology patching**: per (island, topology component)
  cKDTree of routed contacts; a sample is covered iff `dist <= target_spacing_mm *
  coverage_radius_factor` (= 6.0 mm); uncovered sample area accumulated by source face via
  `np.add.reduceat`; connected active source faces form patches; a patch hard-fails if its area
  exceeds `voxel_pitch_mm²` (0.16 mm²) **or** it touches any topology cut flag.
- **SCAR-AREA-CAP** closed form: `A_shaft = min(π r_x²/cos, 2 r_x T)`, `A_ball = π max(R_b²−d², 0)`,
  `A_patch <= max(A_ABS_MM2, KAPPA·π R_b²)`, `L_scar <= L_SCAR_MAX_MM`, `A_anchor >= π r_end²`.
  Must be literal-identical to the independent census (`tools/census_run.py` PASS C2).
- **Rule-4 through-transit classification with the PRINT-FLOOR amendment**: per-ray sorted solid
  intervals; the tip-anchored interval is exempt iff its SCAR verdict is legal; other crossings
  with chord > `MIN_ILLEGAL_LEN_MM` are FIRM if chord ≥ `PRINT_FLOOR_CHORD_MM` (0.08 mm) else MOOT;
  a member is nonexempt iff any crossing is firm.
- **Ordering law (load-bearing).** Kernel penetration intervals must be requested at FULL
  resolution (`min_illegal_len_mm=0.0`) and the length filter applied AFTER tip anchoring.
  Inverting the order promotes an unanchored 0.09–1.16 mm transit into the tip slot where a legal
  SCAR verdict exempts it; that single ordering bug hid 5 non-exempt members and misfiled 3.
- **Ray-set law.** Stubs get AXIS-only intervals; non-stubs get AXIS + 8 capsule-offset rays; tip
  anchoring is evaluated PER RAY against that ray's own arc length.
- **Inside/outside is 3-axis ray-parity majority vote**, never the sign of a signed distance
  (unreliable at thin features, ~3× over-reported penetrations).
- **Exact nearest triangle with a certificate**: k=64 nearest centroids + a never-miss `large`
  triangle set; certified iff `best_dist + tri_extent_nonlarge <= d_k`, else one escalation.
- **Transient span sweep** sorts by `active_z` (NOT `point_z`) and CLAMPS out-of-range restraints
  into `[base_z, z_top]` rather than dropping them. Both are trap-documented and test-pinned.
- **`transient.resize_forest`** replaces radii bottom-up with `max(Murray fan-in, r_tension,
  r_deflect over the worst free gap converted to 3D by own_length/z_extent, r_transient =
  shaft_r(worst_span)·max(n_terminals,1)^0.125, kind floor)`; no ceiling clamp.
- **Exact-bit face adjacency**: vertices are welded by raw float64 BIT pattern
  (`vertices.view(np.uint64)` + `np.unique`). A manifold edge is exactly 2 occurrences on distinct
  faces with opposite direction; everything else raises a cut flag. `-0.0` and `0.0` are distinct
  vertices. A float32 round-trip anywhere upstream silently fragments adjacency, raises
  `TOPOLOGY_CUT_BOUNDARY`, and hard-fails coverage patches.
- **Removal predicates** (report-only): HANDLE = maximal connected runs of non-stub members with
  `min(r0,r1) >= grip_handle_r_mm`, run length ≥ `grip_handle_len_mm`, worst model gap ≥
  `grip_clear_mm`; PEEL = 5 outward-biased directions swept through `pull_dist_mm` with no entry
  into the model standoff envelope.
- **A5 internal planning** (`internal.py`) works on the voxel authority: boundary masks from
  cavity/solid z-shift, overhang filter from the gradient of the signed EDT, `ndimage.label` lobes,
  per-column vertical routing with prefix-sum interval counts, `binary_erosion` for swept
  containment, hex-ish cell decimation + cKDTree assignment.

**Already in Rust.** `geometry_exact.py` is ~fully ported: `rust/geom_kernel/src/{closest,moller,
parity,bvh,queries,par,consts}.rs` provide the exact `ExactSurface` method set, and
`geometry_kernel.KernelSurface` is the PyO3 drop-in production uses. One documented deviation:
`KernelSurface.segment_clearance` always returns the true exact clearance and a real argmin point,
never the oracle's Tier-1 open-space certificate value or NaN argmin. **Not in Rust at all:**
`verify.py` in its entirety, `transient.py`, `topology.py`, `removal.py`, `internal.py`.
(`rust/chitubox_io/src/verify.rs` is `.chitubox` record-structure validation, unrelated.)

---

## 6. Cluster: phase8 — post-construction relocation and repair

Files: `scripts/support_placement/{phase8,phase8_driver}.py` (~4,850 LOC), specs
`../specs/SPEC-P8-REACH.md`, `../specs/SPEC-P8-RELOCATE.md`, fixtures
`scripts/tests/fixtures/phase8_relocation_input*.json`.

**Purpose and pipeline role.** Runs ONCE, after every fix pass has settled and immediately before
the final verify — the last geometry-mutating stage. `engine.py` is the sole production caller
(`phase8_driver.run_phase8(...)`, engine.py:832). Ordering rationale: joint-ball legality depends
on FINAL shaft geometry, so it cannot run mid-loop. Keep it a separate stage in Rust.

Given the gate output, it identifies every illegal member (contact stubs and interior struts whose
shaft transits foliage) and every breaching joint ball, then re-aims, re-seats, or relocates each
through a strict three-rung ladder per class:

| Class | R1/S1/B1 | R2/D5 | Last rung |
|---|---|---|---|
| Stub (contact) | tip re-aim | surface-seed re-seat (R2), then parent-junction re-seat (D5) | R3 honest quarantine |
| Strut (interior) | foot re-aim | D5 | S3 `mark_dead` / escalate |
| Ball (joint) | axial slide | D5 | B3 honest unresolved |

**Public API**

| Symbol | Consumers |
|---|---|
| `phase8_driver.run_phase8(forest, members, balls, contacts, carried, tip_r, plate_z, geom, cfg, mesh, ...)` | `engine.py:832` — the sole entry point |
| `phase8_driver.load_treant_frozen_snapshot()` | `engine.py:825` |
| `phase8_driver.construct_sites / splice_reaims / quarantine_sites_from_forest / SurfaceSeedIndex / junction_lattice / try_junction_reseat` | internal + tests |
| `phase8.evaluate_r1 / evaluate_r2 / evaluate_s1 / evaluate_b1 / run_phase1_stubs / run_phase2_struts / run_phase3_balls / detect_pockets / build_conflict_graph / solve_pocket / run_pocket_solver / reaim_directions / reaim_stub / reaim_foot / stub_candidates / strut_candidates / reseat_candidates / load_frozen_snapshot` | `phase8_driver`, tests |

**Algorithms worth knowing** (these are the INVARIANTS to carry, not the bookkeeping)

- **GLOBAL lexicographic-min selection over each rung's full legal candidate set** — never
  first-legal-wins (`_batch_legal_sets`, `_score_tuple`, `_reseat_score_tuple`).
- **Batch-first legality evaluation**: gather → one kernel call → scatter, shared by every rung and
  the pocket solver, with an optional per-candidate certificate memo cache.
- Fixed cone-grid candidate generation: polar × azimuth × length-factor ladder around the
  contact/foot normal, azimuth-0 anchored to a stable local frame.
- **Pinned re-aim geometry**: the contact surface point `c` is invariant; the embedded end `p0`
  moves on a sphere of radius `delta` about `c`.
- **Atomic-commit pocket solver** for dense defect clusters: conflict graph via AABB broad phase
  over each member's reachable-candidate envelope, worst-firm-chord-descending commit priority,
  full graph-neighbourhood invalidation on each commit, monotone UNRESOLVED-count objective
  guaranteeing termination in ≤ |pocket| iterations.
- World-origin-pinned voxel bucketing + 6-connected components + 1-voxel dilation halo for
  deterministic pocket identity.
- Batched segment–segment and point–segment closest approach (Ericson RTCD style) plus a
  golden-section + bisection search for the sub-interval of a segment inside another's capsule.
- **D6 commit invariant**: zero new violations and strictly decreasing defect count; the outer
  round loop re-arms per-call memory each round so a true fixed point is a fresh-invocation no-op.
- **A2 regular-contact surface-seed pool**: recursive 1→4 midpoint subdivision to a
  coverage-derived edge bound, co-lowest-vertex nomination per virtual cell, dedup, tie-break by
  most-negative normal-z then face id.
- **D3(a) progress-based site retirement**: a site whose defect measure fails to shrink by the
  canonical micron granularity since its last commit, or exhausts a commit budget, is retired for
  the rest of the run, so the committable population strictly shrinks.

**Already in Rust.** Nothing. This cluster is 100% Python and imports only Python siblings.
It has a hard sequencing dependency: it calls `verify._batch_axis_intervals`,
`_embed_scar_verdicts`, `certify_member`, `clearance_report`, `connectivity_report`,
`ball_breach_report`, `member_rays` — so `verify.py` must be ported first or alongside.

**Recommendation: REDESIGN, not transliterate.** The ladder machinery grew by accretion through
repeated amendments (D6, outer round loop, progress retirement, dual-certificate boundary carry,
ban sets), each individually justified by a measured regression but cumulatively a sequence of
patches over patches. Take the stated invariants as the spec and implement a fresh, unified
(non-duplicated stub/strut) state machine.

---

## 7. Cluster: hollow-orient-mesh — prep, orientation, hollowing, decimation

Files: `scripts/support_placement/{hollow,mesh_io,geometry_kernel}.py`,
`scripts/tools/{decimate_pipeline,geometry_sweep,hollow_drill}.py`,
`skills/decimate-for-print/SKILL.md` (~3,190 LOC).

**Public API**

| Symbol | Consumers |
|---|---|
| `hollow.run(in_path, out_path, wall=2.5, orient=True, log=None, allow_hollow_output=False, cavity_out=None, solid_backup_path=None, cfg=None)` | `pipeline`, `tools/hollow_drill.py` |
| `hollow.triage_components / orient_search / failed_checks / describe_failure / HollowError / PreHollowedError` | `pipeline`, `hollow_drill`, tests |
| `mesh_io.load_stl(path)` (loads with `process=False`, deliberately — no implicit vertex merge/reorder), `bounds_z`, `face_normals_z`, and the synthetic fixture builders `make_box / make_overhang_table / make_toed_slab / make_raking_blade / make_thin_fin` | `pipeline`, `run_v5`, `run_engine_full`, `emit_supports`, tests |
| `geometry_kernel.KernelSurface(mesh, grid_pitch_mm=2.0, large_cap=2000, verbose=False)` with `.query/.project/.inside/.cast_rays/.segment_clearance/.penetration_intervals(_batch)/.sphere_breach/.exterior_face_mask/.current_threads/.telemetry` | `engine` |
| `decimate_pipeline.run(stl_path, log, pixel_um=19.0, out_path=None, min_dim_mm=10, max_dim_mm=300, obj=None)` | **nothing** — Blender-only, manual |

**Algorithms worth knowing**

- **`triage_components` (step 0).** Connected-component classification. Drops "shard" components
  (≤64 faces, ≤1.0 mm bbox diag, watertight, |volume| ≤ 0.01 mm³, shares no foreign edge, touches a
  bit-identical main-body vertex, inside the main bbox). Detects pre-existing internal cavities by
  a batched containment ray test at ≤32 sampled boundary points, threshold 0.9 fraction inside →
  raises `PreHollowedError`. Note `hollow.py` calls `merge_vertices()` itself (unwelded STL
  triangle soup would otherwise read as thousands of 1-face components) — the opposite policy from
  `mesh_io.load_stl`. Preserve both, deliberately.
- **`orient_search` (step 1).** 13 candidates: identity + rotations of ±10/20/30° about X and Y
  (`hollow.py:443-444`), scored on a 90%-decimated proxy
  (`fast_simplification.simplify`, `target_reduction=0.9`, falls back to the full mesh on failure).
  Score = `0.5·support_area_cm2 + 3·peak_crosssection_cm2 + 0.3·height_mm` (+
  `orient_scar_weight · delicacy_weighted_downface_area / 100`). Support area = faces with
  `normal.z < -0.707`; peak cross-section via `section_multiplane` every 4 mm in z. Picks the
  min-score rotation, re-centers XY, rests the bottom at z=0.
- **Hollowing.** Voxelize at `PITCH = 0.5 mm`, fill, Euclidean distance transform × PITCH gives
  per-voxel wall thickness; `bulk = 2·edt.max()`; cavity mask = `edt > wall` (default 2.5 mm);
  only the LARGEST labelled cavity component is kept. Eligible iff `bulk >= 10 mm` AND
  (`cavity_ml >= 15` OR `cavity_ml >= 0.25·solid_ml`).
- **Drain/vent.** Per low lobe (cavity voxels within 15 mm of cavity low z, ≥0.5 ml) drill one
  3 mm-dia hole along the shortest exit ray among 5 mostly-downward candidates at 0.5 mm step out
  to 45 mm; one 2.5 mm-dia vent near the cavity top through the thinnest horizontal wall among 4
  axis-aligned directions over ≤40 sampled top points.
- **`trapped_map`.** Exact per-voxel drainability by bottom-up union-find (priority flood) in z
  order; a voxel drains iff its component already contains a marked drain voxel once its own
  z-level merges. Trapped components get ≤3 extra audit drains if a ≤8 mm downward exit exists,
  else are refilled to solid.
- **Shell + boolean.** `skimage.measure.marching_cubes` on the padded cavity mask (level 0.5)
  yields an INSIDE-OUT inner shell requiring `trimesh.repair.fix_normals`; then
  `trimesh.boolean.difference(engine='manifold')` over `[solid, inner_shell, *drain/vent
  cylinders]`. Deliberately does NOT `merge_vertices` afterwards (manifold3d output is already
  welded; merging risks pinching seams into non-manifold edges).
- **Step-8 verification (export gate).** Five checks: `single_body`, `watertight`,
  `winding_consistent`, `volume_balance` (|vol − (solid − cavity)| < 2% solid), `cavity_breathes`
  (re-voxelize at 0.4 mm, flood-fill the inverse mask, compare the label at the cavity centroid to
  the label at index `[0,0,0]`). Export happens only if all five pass.
- **`decimate_pipeline.run` (Blender, the smart-decimation reference).** Owner's instruction: *"the
  logic of what was being done there should be converted to rust code, not re-invented."* Stage
  order, verbatim: (1) load STL into a fresh scene; (2) scale sanity check, SKIP outside 10–300 mm;
  (3) build BVH + count non-manifold/boundary edges on the original; (4) sample ≤120,000 vertices
  (seed 42); (5) add a `DECIMATE/COLLAPSE` modifier; (6) one aggressive probe at ratio 0.2 to build
  a per-vertex error map; (7) seed/grow the protection halo — vertices with error > `SEED_ERR`
  (0.05 mm) are seeds, a KD-tree range query grows a `HALO` (0.4 mm) zero-weight zone, capped at
  `PROT_CAP` (12%) of vertices via 3 backoff iterations (`seed_thr *= 2`, `halo *= 0.6`); (8) write
  weights into the vertex group `decimate_freedom` (weight 1 = decimate more, weight 0 = protected —
  counterintuitive, verified empirically) with `vertex_group_factor = 10.0`; (9) bisect 5 iterations
  between `RATIO_LO=0.05` and `RATIO_HI=0.50` for the lowest ratio whose two-sided sampled p99/p99.9
  error stays within `tol = pixel_um/2000` and `tol999 = pixel_um/1000` — forward error = decimated
  face centers → original BVH nearest (≤150,000 centers), backward = original sampled verts →
  decimated BVH nearest; (10) apply; (11) FULL-population re-verification with 1.1× slack on both
  tolerances plus "non-manifold/boundary edge counts must not have increased"; (12) export only on
  PASS. `DEFAULT_PIXEL_UM = 19.0` (Saturn 4 Ultra 16K pixels are 14×19 µm; use the coarser axis).
  The 1.1× slack applies ONLY at the final full-population check, to absorb sampling drift — it is
  intentional, not a loosened spec.
- **`geometry_sweep.sweep`** (Blender): edge-collapse-only degenerate-geometry repair (zero-area
  faces, zero-length edges, non-2-manifold edges, duplicate-face dedup, midpoint pointmerge
  fallback), converging over multiple passes.

**Already in Rust.** Only `geom_kernel` (via `KernelSurface`). Voxelization, EDT, marching cubes,
booleans, STL IO, orientation search, decimation and drain/vent ray placement are all Python or
Blender with no Rust counterpart. Version pinning is enforced at import:
`geometry_kernel.py` checks `_native.KERNEL_API_VERSION == 1` and numpy major version 2;
`emit.py` checks `chitubox_io.API_VERSION == 1`.

---

## 8. Cluster: chitubox emit — geometry synthesis + container I/O

Files: `scripts/support_placement/{chitubox_emit,chitubox_inject,emit}.py`, `rust/chitubox_io/**`,
`src/main.rs`, `Cargo.toml`, corpus at `scripts/tests/fixtures/chitubox/` (~6,370 LOC combined).
Format detail lives in `05-chitubox-and-corpus.md`; this section is the map only.

**Public API**

| Symbol | Consumers |
|---|---|
| `emit.emit_project(model_stl, result, out_path, cfg, *, name=None, placement_xy=(0,0), lift_mm=None, allow_failed_gates=False) -> EmitReport` | `pipeline`, `tools/emit_supports.py` |
| `emit.support_arrays(specs, *, plate_z=None)` — the ONE columnar crossing packer | `emit_project` |
| `chitubox_emit.member_chain_specs / bottom_connected_specs / synth_member_chain / frustum_mesh / sphere_mesh / build_record` | `emit`, legacy inject path, tests |
| `chitubox_inject.Project / clone_support / make_bridge / write / write_replace` | `chitubox_emit` (for `T_*` type ids and `clone_support`), `corpus.py` |
| `chitubox_io::Project::{new_empty, open, add_models, set_placements, rotate_models_z, attach_supports, supports, to_bytes, save}` | Rust-side public API |
| `chitubox_io` PyO3 `write_project(out_path, models)` / `read_project(path)` (`src/py.rs`) | `emit.py` — historical consumer; O-7 deletes this binding at native handoff |

**Algorithms worth knowing**

- Capped-frustum triangle-soup synthesis (`N_SIDES = 24`) with outward-winding self-correction via
  a trimesh Mirtich signed-volume check; UV-sphere synthesis (`n_lat=8, n_lon=16`) for tip, joint
  and model-foot balls.
- Fixed native-style PAD (type 6, `start == end`): 2.2 mm dia, 0.2 mm high, 20-sided, 80 triangles
  / 2880 bytes; BRIDGE (type 6, `start != end`): 12-triangle rectangular prism, 432 bytes.
- Deterministic Prim MST over plate-root XY points for raft-bridge topology.
- Native root-boss (type-4 FLARE) sizing enforcing `MIN_ROOT_VERTICAL_LAYERS · layer_height_mm` and
  `MIN_ROOT_NO_BEND_MM` clearance above plate + raft.
- The 72-byte segment record layout (magic / type / start / end / r1 / r2 / mesh_off / mesh_len /
  24 trailing bytes), reverse-engineered from the ChituBox binary corpus and implemented **twice**:
  `chitubox_emit.build_record` (Python) and `rust/chitubox_io/src/support.rs::encode_record`.
- `rust/chitubox_io/src/verify.rs::audit` — an **independent first-principles interval/overlap/
  pointer audit of the serialized bytes**, sharing no parser code with the writer. This is the
  existing embodiment of the "measure the file as written" law. There is no Python equivalent.
- `emit.py`'s frame invariant: `zmin_centered − plate_z == lift_mm` to `LIFT_TOL_MM = 1e-6`,
  checked against a freshly reloaded and independently re-centered STL, not the caller's say-so.
  The f32-vs-f64 bbox-center gap is measured and REPORTED as `center_delta_mm` (~1e-6 mm at 64 mm
  scale), not asserted to zero. Preserve both patterns.

**Already in Rust.** `rust/chitubox_io` is a mature container reader/writer: file structure, model
table, support-section index, verified serialization, Z-rotation of packed byte ranges, STL
embedding. **Not in Rust:** all support-record *geometry synthesis* — frustums, spheres, pads,
bridges, MST raft topology, boss/flare sizing. `chitubox_io` only ever stores a `SupportRecord`
someone else built. `src/main.rs` is a stub `println` today; the `plinth` binary does not exist.

**f32 boundary.** `.chitubox` is f32 on disk; `types.rs` `SupportRecord`/`TriangleSoup` are f32
because that IS the wire format. Everything upstream stays f64. Compute geometry in f64 and cast
only at the record-encode step. This is the one sanctioned f32 in the system.

---

## 9. Cluster: tools + census — CLIs, the independent audit oracle, dev harnesses

Files: `scripts/tools/*.py` (~2,880 LOC), `scripts/tests/{manual_*,conftest,_perf_fingerprint,
_refreeze_v5}.py` (~1,420 LOC).

**Purpose.** Operator CLIs (§1 table); an independent from-scratch geometric **census** that
re-derives penetration / scar / ball-breach verdicts using its OWN primitives and deliberately
shares no code with `support_placement` (`census_core.py`, `census_run.py`, `p11_crossexam.py`,
`validate_buried_end_fix.py`, `validate_r3_ball_gate.py`); Blender-only mesh prep; a Rust-vs-Python
differential harness (`bench_kernel.py`, `replay_parity.py`, `instrument.py`); and pytest infra.

**Worth knowing**

- The census is the ancestor of `geometry_exact.py` — `geometry_exact`'s docstring says its
  primitives are "Ported census primitives (census_core.py)". `census_core` deliberately never
  calls the Rust kernel; that independence is the point. If a Rust-side audit tool survives the
  port, it must remain a **separate implementation**, not a call into the engine's kernel.
- `census_run.py` PASS A/B/C1 (crossings + parity), C2 (SCAR-AREA-CAP), C3 (PRINT-FLOOR moot
  downgrade) mirror `verify.py` and must stay literal-identical; but nothing today enforces that
  the duplicated constants (`PRINT_FLOOR_CHORD_MM`, `BALL_FACTOR_STUB/FOOT`) stay equal to their
  originals in `verify.py` / `config.py` / `joints.py`.
- `census_run.py` processes rays in two flattened passes with fixed chunks of 384 and an RSS < 2 GB
  assertion between chunks. Preserve the chunked-batch-with-memory-ceiling shape if the audit tool
  survives.
- `conftest.py` implements the **Rust-extension staleness gate**: a SHA-256 digest over sorted
  `src/*.rs` + `Cargo.toml`/`Cargo.lock`/`build.rs`/`pyproject.toml` (each framed as
  basename + len + bytes), duplicating each crate's own `build.rs` `SOURCE_DIGEST`, so
  `@pytest.mark.kernel` / `@pytest.mark.chitubox` tests **FAIL rather than skip** when the binary
  is missing or stale. Keep the fail-not-skip discipline in whatever replaces pytest.
- `_refreeze_v5.py` / `_perf_fingerprint.py` define the determinism fixture contract: canonical
  exact-bytes JSON (float64 as big-endian hex via `struct.pack('>d', ...)`, ndarray as
  dtype + shape + raw-bytes hex). The Rust regression fixtures should use an equivalently exact
  serialization, not tolerance comparisons.
- `_toolpaths.py` centralises scratch/STL path resolution (`CENSUS_SCRATCH`, `TREANT_STL` env
  vars); `manual_emit_treant.py` and `manual_treant_v4.py` predate it and hardcode `/tmp` and
  out-of-repo relative walks — exactly the bug `_toolpaths.py` was created to fix.

**Already in Rust.** The kernel geometry the census cross-checks is ported (`geom_kernel`); the
census itself is Python by design. `recon_chitubox.py`'s findings are already captured in
`../specs/CHITUBOX-FORMAT.md`; it has no porting obligation.

---

## 10. Prior art in the repo, and what it still binds

These documents exist and Codex will find them. Their status:

| Doc | Status |
|---|---|
| `../reference/port_plan.md` | **Superseded as to language.** Its verdict was "stay Python, a full Rust port is not recommended". `../specs/SPEC-RUST-KERNEL.md` explicitly supersedes that verdict after the 2026-07-23 ruling, and this migration supersedes it entirely. Its determinism/parallelism discipline (§5 serial-generate / parallel-evaluate / serial-commit) remains valid. |
| `../reference/port_plan_codex_review.md` | Conclusion superseded; findings still binding. Its central finding — `rewire.optimize`'s 30 s wall-clock cap makes the "frozen serial baseline" itself non-reproducible — is still true. It adjudicated independent-tree parallelism and whole-sweep speculative replay UNSOUND; do not resurrect those. |
| `../specs/SPEC-RUST-KERNEL.md` | Binding for `geom_kernel`'s numerics and its Stage 2–5 roadmap. **But its scope limit is superseded**: it proposed a PyO3 hybrid and permanent Python oracle. O-7 transfers that role to frozen disk/fixture evidence and deletes compatibility at native handoff. |
| `../specs/SPEC-V5-GEOM.md`, `SPEC-EMBED-POLICY.md`, `SPEC-A3-A4-COVERAGE.md`, `SPEC-P8-REACH.md`, `SPEC-P8-RELOCATE.md`, `SPEC-V5-PERF.md`, `SPEC-CHITUBOX-RS.md`, `CHITUBOX-FORMAT.md` | Normative; cited as law from code comments. Carry their formulas forward. `SPEC-P8-*` §4 is authoritative for the pocket solver but expect to simplify (§6). |
| `../datasheets/` | External reference data intended to parameterize `config.py`. Its own README says "Nothing is wired up that way today," and documents that `peel_pressure_pa` and `shaft_drag_force_n` admit in-code to being unmeasured guesses — the existing acknowledgment of the Placeholder-provenance problem. |
| `../../skills/{prep-mini,decimate-for-print,support-placement,chitubox-support-injection}/SKILL.md` | Describe the CURRENT interactive Blender-MCP workflow, not the target. Only the decimation ALGORITHM (§7) must be mirrored; the Blender/MCP delivery mechanism is out of scope. |
| `../reference/RETIRED-TOOLS.md`, `penetration_census.md`, `physics_*.md`, `TREANT-V4-PRINT-AUDIT.md`, `AUDIT-ADDENDUM-mesh-classification.md`, `acceptance-run-2026-07-24-contactfix.md` | Evidence and reasoning. `physics_*.md` feeds `03-physics-and-constants.md`. |

---

## 11. Consolidated DEFECTS — fix, do not reproduce

Every entry is a bug, dead end, or wrong abstraction the port must correct. "Correct looks like"
is the acceptance target.

| # | Defect | File : symbol | Correct looks like |
|---|---|---|---|
| D-1 | **The load gate is unsatisfiable by construction.** `verify.self_verify` (verify.py:1666) ANDs `len(load) == 0` into `ok`; `load_report` (verify.py:1071) flags any contact whose realized safety factor is below `tip_safety_factor` after the tip radius was clamped at `tip_r_max_mm = 1.0`. Nothing in `engine.py` ever adds a contact in response. The fix loop's only continuation condition is `if not report['uncovered_islands']` (engine.py:1370) and the only selector is `_rank_coverage_repair_rows`, ranked purely by marginal UNCOVERED area. Load violations appear only in `_NONCOVERAGE_GATE_KEYS` as a non-increase constraint — they can reject a repair, never request one. A large flat down-face fails `ok` forever. | `verify.py:self_verify` / `load_report`; `engine.py:1370` | Violation kinds are an enum with exhaustive match, so a gate that can fail without a fixer that can respond is a **compile error**. Plus a real multi-arm load-sharing model (D-2). |
| D-2 | **Multi-arm load sharing does not exist in the data model.** `tip_r_max_mm = 1.0` caps one tip at ~37.7 mm² of supported area at X=12. `cco._r_tension_mm` and `trunks.size_trunks` size each member against its own scalar `carried_mm2` with no lateral-arrangement awareness. The repair loop can only append or delete contacts — never move one, resize a tip, or split load. | `placement.tip_radius_mm`, `cco._r_tension_mm`, `engine.run` fix loop | Load partition supports N arms sharing a down-face. Plate 6 (scored 2026-07-31) measured sharing as **at least additive** and refuted the zipper model — see `03-physics-and-constants.md` §4.2; `Zipper` survives only as the conservative fallback and the A/B arm. |
| D-3 | **Eleven of twelve hard gates have no fixer.** Only `uncovered_islands` has one. `clearance_violations` has an indirect one (`bracing.propose_and_place` / `transient.resize_forest` inside `brace_and_size`, plus Phase-8 SM-STRUT), and `brace_and_size` ends with a bare `assert not violations` (engine.py:739) that crashes instead of reporting. `load_violations`, `unsupported_minima`, `uncovered_island_births`, `regular_contact_invariant_violations`, `unrouted_specialized_contacts`, `connectivity_violations`, `printability_violations`, `murray_violations`, `ball_breach_violations`, `physical_contact_invariant_violations` have none. | `verify.self_verify`; `engine.py:739, 1370, 2083-2092` | Same as D-1: the enum + exhaustive match makes an unfixable gate unrepresentable. |
| D-4 | **`_add_fix_contacts` is dead code.** Its only callers are `test_engine.py:43` and `test_placement.py:461,486`. Its docstring is where the load→fix intent was written down, and even it selects by argmax nearest-contact distance — coverage, never load. | `engine.py:2009` | Delete. The intent it records belongs in the load-violation fixer. |
| D-5 | **Second entry point / bypass emit path.** `tools/emit_supports.py:78-84` calls `engine.run(mesh, cfg)` then `emit.emit_project` directly on the SOURCE mesh — no hollow, no drain drilling, no orientation. `run_v5.py`, `run_engine_full.py`, `recheck_rewire_cap.py` do the same. | `scripts/tools/emit_supports.py`, `run_v5.py`, `run_engine_full.py`, `recheck_rewire_cap.py` | Exactly one path through the Rust CLI. Dev tooling calls the same stage functions, never a parallel composition. |
| D-6 | **`hollow=False` silently disables ORIENTATION.** Orientation lives inside `hollow.run`; `pipeline.prep_one` documents it and records `reason='hollow=False (orientation search skipped too)'`. Orientation is the coupling hub of the fast / min-resin / high-quality triad. | `pipeline.py:prep_one`, `hollow.orient_search` | Orientation is its own stage with its own inputs and outputs, orderable independently of hollowing. |
| D-7 | **`orient_search` is 13 hand-picked candidates with an ad-hoc objective.** Identity + ±10/20/30° about X and Y only (hollow.py:443-444) — no Z rotation, no combined axes — scored by `0.5·support + 3·peak_xsec + 0.3·height` (+ scar). `orientation_scar_cost` also scores a 90%-decimated proxy against a delicacy field sampled from the FULL mesh, so the scar term's magnitude is decimation-dependent. | `hollow.py:405-474`; `verify.orientation_scar_cost` | A dense sweep on a decimated proxy, scoring the support/scar/height triad through one joint cost function, with cached incremental re-evaluation. |
| D-8 | **Cap exhaustion is anonymous.** The coverage loop records `termination='attempt-cap'` without naming which defect failed to improve; `engine.relocate` raises a bare `RuntimeError('Phase-8 + structural-resize combined fixed point exhausted N round(s)')`; `brace_and_size` just falls out of its `for` loop with no record of non-convergence. | `engine.run`, `engine.relocate`, `engine.brace_and_size` | Cap exhaustion names WHICH defect failed to improve and by how much, in the DefectReport. |
| D-9 | **Asserts as acceptance.** `engine.brace_and_size:739` and `tree.assert_all_certified` at engine.py:639, 746, 875, 1037, 1273, 1457 turn honest engine failures into `AssertionError`, which `pipeline.prep_one` flattens into `EXIT_SUPPORTS=5` with a stringified message. Failures live in three vocabularies: strings, exceptions, exit codes and log keys. `geometry_exact._rss_ok` also raises `AssertionError`, so every RSS guard vanishes under `python -O`. | `engine.py`, `tree.py`, `geometry_exact.py` | One `DefectReport` type is the sole acceptance authority. No assert on the accept path. |
| D-10 | **Phase-8 run-level acceptance gates are not wired.** `p8_accept.final_geometry_verify`, `zero_new_firm_transit_gate`, `raft_connectivity_report`, `criticality_weighted_unresolved_gate`, `idempotence_gate` are referenced only by `test_p8_accept.py`; `phase8_driver` imports the module but uses only `geometry_digest`. The spec's run-level acceptance is unenforced in a real run. | `scripts/support_placement/p8_accept.py` | Orphan/dead-ancestor and idempotence checks run on the shipped accept path. |
| D-11 | **`murray_report` does not filter dead members.** It iterates `enumerate(forest.children)` and sums `forest.members[ci].r1 ** 4` with no `ci not in forest.dead` test (verify.py:1058-1068), while `transient.resize_forest` and `_postorder` DO filter. A retired child inflates `sum_child4` and can hard-fail a gate on geometry that will never print. The same class of bug was fixed for `printability_report` and never for this one. | `verify.py:murray_report` | Live-member filtering is structural — the gate cannot see a dead member. |
| D-12 | **Two conflicting `lift_mm` defaults.** `pipeline.prep_one` builds `SupportConfig(lift_mm=3.0)` (pipeline.py:286) while `config.SupportConfig.lift_mm` defaults to 5.0 (config.py:235); `prep_one` also accepts a separate emit-time `lift_mm` affecting only emit. `plate_z = mesh.bounds[0][2] - cfg.lift_mm` is computed in `engine.prepare` from the ENGINE cfg, so an emit-time override silently disagrees with the plate the supports were built against. | `pipeline.py:286`, `config.py:235`, `engine.prepare` | One typed config value, one plate_z, derived once and threaded. |
| D-13 | **Signature introspection as an API.** `pipeline.prep_one` uses `inspect.signature(hollow_mod.run).parameters` to decide which kwargs to pass ("test doubles and older embedders may expose the pre-A5 signature"). | `pipeline.py:prep_one` | One typed stage interface (a trait), no reflection. |
| D-14 | **The A5 hollow fallback rewrites the artifact mid-run.** On internal-verifier refusal, `prep_one` does `shutil.copyfile(backup, prepped)` OVER the already-written prepped STL and re-runs the whole exterior engine (pipeline.py:488-517). The file on disk changes identity under a path other stages already consumed. | `pipeline.py:488-517` | Stages produce immutable, distinctly-named artifacts; a fallback produces a new artifact, never mutates a consumed one. |
| D-15 | **`getattr(cfg, name, default)` config reads with defaults that disagree with `config.py`.** ~8 sites in `engine.py`: `perf_seed_spacing_factor`, `perf_fix_iter_caps`, `perf_gate_emit_on_verify`, `p8_relocate_enable`, `p8_reseat_enable`, `p8_max_relocate_sweeps/rounds`, `p8_resize_fixed_point_max_rounds`, `coverage_score_matrix_max_entries`, `synthetic_coverage_mask_max_words`. E.g. `getattr(cfg,'p8_relocate_enable',False)` vs a `config.py` default of `True` — behaviour differs by call path. | `engine.py` | Typed config struct. No defaulted dynamic lookup exists to disagree. |
| D-16 | **Pickle is the serialization contract.** `p8_accept.final_geometry_verify` round-trips the Forest through `pickle.dump/load` in a tempfile to catch serialize drift; prepared state is "a picklable dict"; `FaceTopology` has `__getstate__`/`__setstate__`. | `p8_accept.py`, `engine.py`, `topology.py` | Explicit versioned serialization (serde) with the same drift-catching intent — round-trip and re-verify the reloaded value. |
| D-17 | **Identity-preserving mutation of the forest.** `engine.relocate` commits by `forest.__dict__.clear(); forest.__dict__.update(working_forest.__dict__)` then re-binds every member's `_forest`/`_idx` via `object.__setattr__` (engine.py:885-889). | `engine.py:885-889` | The stage returns an owned `Forest` value; the caller rebinds. |
| D-18 | **Silent path divergence in `clearance_report`.** The exact path (geom has `segment_clearance`) returns dicts carrying `source` (`'transit'`/`'distance'`) and fills `moot_transit_out`/`transit_chords_out`/`scar_illegal_out`; the sampled legacy path (verify.py:993-1039) returns dicts with NO `source` key, leaves all three out-params untouched, and has no rule-4 transit concept at all. `phase8_driver.py:357` and `:361` filter on `source` (and again at `:1363-1365`). `tree.Forest._revalidate_sampled` is the same shape. | `verify.clearance_report`, `tree.Forest._revalidate_sampled` | ONE clearance gate. No legacy fallback path that quietly answers a different question. |
| D-19 | **Duplicated dyadic-sampler implementations.** `placement.candidate_pool` and `coverage._build_coverage_samples_inner` are two near-duplicate ~300-line two-pass refine/preflight/generate machines with different byte-per-row constants (64 vs 49) and different validation dataclasses. | `placement.py`, `coverage.py` | One generic dyadic-sample builder parameterized by payload. |
| D-20 | **Three copies of point-to-segment distance**, each with a hand-rolled 256-point chunk loop and the same zero-length guard; two independently hardcode the same 0.25 mm tolerance. | `verify.connectivity_report:1267ff`, `transient.derive_brace_attachments:148ff`, `removal._min_gap_to_members:222ff` | One batched primitive. |
| D-21 | **Two implementations of the same transient sweep**, kept in sync only by a test: `transient.transient_worst_span` and `transient._span_intervals`. | `transient.py` | Emit the interval list once, take its max. |
| D-22 | **Private functions are the cross-module API.** `phase8.py`, `phase8_driver.py` and `tools/p11_crossexam.py` import `verify._batch_axis_intervals`, `_embed_scar_verdicts`, `_transit_classify`, `_tip_iv_index`, `member_rays`. The underscore is a lie and the real boundary is undocumented. | `verify.py` ↔ `phase8*.py` | An explicit `pub` certificate API with one documented contract. |
| D-23 | **Default disagreement on the load-bearing interval filter.** `ExactSurface.penetration_intervals` defaults `min_illegal_len_mm=0.05`; `verify._batch_axis_intervals` defaults `0.0` and its docstring says the 0.0 "IS THE LAW", because a pre-anchoring filter promotes an unanchored transit into the tip slot where a legal SCAR verdict exempts it. | `geometry_exact.py`, `verify.py:509` | The kernel offers **no** pre-anchoring filter. The length filter is applied strictly after tip anchoring, at one call site. |
| D-24 | **Dual certificate as a patch over two disagreeing samplers.** The mi=445-class tip-anchor divergence between the engine's kernel oracle and the independent census sampler is resolved by ANDing two independently-implemented legality checks for a named boundary member set, carried across sweeps via `boundary_carry`. `certify_members_full_resolution` is now a no-op wrapper whose own docstring says the census-boundary class is "EMPTY by construction". | `phase8_driver.py`, `verify.certify_members_full_resolution` | ONE authoritative interval/tip-anchor algorithm, so the divergence class cannot exist. One certificate function. |
| D-25 | **`removal_report` is always computed and never gates.** `self_verify` (verify.py:1659) runs the full HANDLE/PEEL/ordered-removal simulation on every verify pass whenever `forest` is supplied — an O(trees²) `_min_gap_to_members` sweep plus a full surface-distance sweep per tree — while `gate_on_removal` defaults False and no shipped caller sets it. `removal.py:259` also uses the `contact_depth_mm + 0.2` embed inset that was independently proven to produce 26 false-positive clearance violations on the real Weeping Treant, while claiming in a comment to match `clearance_report` (which no longer uses it). | `verify.self_verify:1659`, `removal.py:259` | An opt-in metric, off the accept path, using the current inset formula. |
| D-26 | **Retired formulas kept alive as self-masks.** `_embed_inset_arc_exact` is retired as a sanction but survives as a distance-gate self-mask with a magic `cos` floor of 0.5 and a pitch term hardcoded to 0; its docstring says it is "slated for replacement". | `verify._embed_inset_arc_exact` | Derive the mask from the member's known designed embed zone. |
| D-27 | **Dead provenance check.** `internal.py:473-475` re-tests `got_cavity_fp != cavity['cavity_fingerprint']`, a condition that already caused an early `_failure` return at line 231. The violation list is unconditionally empty. | `internal.py:473-475` | Delete, or make it reachable. |
| D-28 | **Numpy allocator archaeology baked into a wire digest.** `topology._planned_peak_build_bytes` / `_planned_peak_validation_bytes` are hand-counted per-face/per-vertex byte coefficients for NumPy/SciPy sort/unique scratch (48f+88v, 84f+32v, 527f+32v, 300f+32v, 900f+32v, …). They cause HARD REFUSALS before the build runs and are hashed into `FaceTopology.content_digest`, so the sidecar's identity depends on Python allocator arithmetic. `build_face_topology` then immediately fully re-validates what it just built, and `validate_face_topology` is re-run opportunistically from six more call sites per run. | `topology.py` | Redefine the digest over the topology arrays only, version-bump the schema, validate once at construction (a validated newtype). |
| D-29 | **Bitwise vertex welding.** `topology._build_face_topology_inner` welds vertices by raw float64 bit pattern; `-0.0` ≠ `0.0`, any NaN is its own vertex. A float32 round-trip anywhere upstream silently fragments adjacency → `TOPOLOGY_CUT_BOUNDARY` → coverage patches hard-fail regardless of area. | `topology.py`, `mesh_fingerprint` | f64 end to end with an explicit weld tolerance policy, decided rather than inherited. |
| D-30 | **Fail-open oracles.** `perception.overhang_islands / local_z_minima / delicacy_field / exterior_anchor_pool` all fail OPEN when no surface-exact `geom` is supplied (duck-typed `hasattr` check). `delicacy_field` swallows ANY exception from curvature or the KDTree pass and returns `curv_score=0 / thin_score=0` — a plausible-looking, wrong field. `ExactSurface.exterior_face_mask` fails open to "exterior" on tie votes. `verify.ball_breach_report` returns `[]` when the geom lacks `sphere_breach`, so `gate_on_ball_breach` silently becomes a no-op. | `perception.py`, `geometry_exact.py`, `verify.py` | The oracle is always present by construction; failures are typed, not swallowed. A gate that cannot run is a defect, not an empty list. |
| D-31 | **`brace_and_size` fixed point has no convergence record**, ends with a bare `assert not violations`, and re-runs `rewire.optimize` a second time plus a re-fix pass inside the same function. | `engine.brace_and_size` | Named convergence outcome in the DefectReport; sizing returns a new radius array rather than mutating in place (`transient.resize_forest` currently mutates the thing it measures). |
| D-32 | **Wall-clock caps make the baseline non-reproducible.** `rewire.optimize` honours `cfg.rewire_time_cap_s = 30.0`; a faster implementation completes more moves before the cap fires and silently changes topology. `SPEC-V5-PERF` B5's `perf_rewire_work_budget` (a deterministic move-eval counter) is the intended replacement but is not the default. | `rewire.py`, `config.py:580` | Deterministic work budget or true convergence. No wall-clock gating anywhere on the accept path. |
| D-33 | **Dead v3 code paths.** `braces.py` and `trunks.py` are not imported by `engine.py` or any production path — only by each other and their own tests. `tree.py`'s docstring says the Member/Forest model replaces "v3's Trunk/Route/riser/brace collections". `braces.py` additionally pulls in `networkx`. `struts.py`'s `find_model_anchor`/`choose_anchor`/`cost` have no non-test caller. `corpus.py` is imported only by `test_corpus.py` and `manual_run_treant.py` yet pulls scipy into the library namespace. | `braces.py`, `trunks.py`, `struts.py`, `corpus.py` | Do not port. Read `trunks.py` only as reference for the spiral plate-anchor search that `cco._plate_descent_builders` already reimplemented. |
| D-34 | **SM-BALL B2 (local re-route) is unwired.** `run_phase3_balls` is called with `reroute_fn=None` and its own docstring says so. | `phase8.run_phase3_balls` | Either implement the rung or delete it from the ladder; no half-wired states. |
| D-35 | **Test fixture logic embedded in production.** `phase8_driver`'s frozen `phase8_relocation_input.json` treant fixture + `TREANT_PRERELOCATE_DIGEST` hardcodes acceptance-target selection to one pinned geometry digest and model with magic member indices (72/37/39/7) that override the live gate. `Q_SITE_OFFSET = 1_000_000` multiplexes two site-id namespaces through an integer offset. | `phase8_driver.py` | Target selection comes from the live gate. Site ids are a sum type: `enum SiteId { Member(u32), Quarantine(u32), Ball(u32) }`. Fixtures live in tests. |
| D-36 | **Concession supports as a load-bearing feature.** `SPEC-P8-REACH` Feature B / `_concede_terminal` weakens a member's clearance envelope to 0.1 mm near the model, expecting a human to buff the print afterwards. | `phase8.py`, `SPEC-P8-REACH` | The port should need fewer concessions, not enshrine the ladder as a design pattern. |
| D-37 | **Scalar hot loops to redesign, not translate.** `engine._consolidate_specialized_contacts` (nested per-contact Python loop recomputing the FULL coverage report, load partition and specialized gates for EVERY candidate merge, inside a `while True`); `engine.specialized_physical_invariant_report` (all-pairs scan, called at prepare, at every verify, and inside `validate_prepared_state`); `placement.sample_candidates` (`np.vstack` growth per accepted point); `perception.local_z_minima` BFS and the polygon-edge loops in `point_to_polygon_boundary_dist_xy`/`_polys_overlap_xy`; `hollow.exit_ray` (5 dirs × 90 steps, and 4 × 90 × 40 for the vent); `hollow.trapped_map` union-find; `internal.actual_clearance_clean`, `_keepout_mask` (a full-volume dilation PER keepout), `_swept_containment` (an erosion per radius); `ExactSurface.segment_clearance` (a per-segment Python loop despite the name); `verify._batch_axis_intervals`'s per-member fallback; `removal._min_gap_to_members` (per-tree × per-other-tree); `decimate_pipeline.probe` (one `find_nearest` per point, up to 270k points × 7 probes); `census_core.VerticalParity.inside`'s per-cell loop. | as listed | Gather → one batched call → scatter, rayon-parallel. A per-item call in a hot path is a defect. |
| D-38 | **Duplicated constants with no equality enforcement.** `chitubox_emit.PLATE_BAND_MM` duplicates `joints.PLATE_BAND_MM`; `census_run.PRINT_FLOOR_CHORD_MM` / `BALL_FACTOR_STUB` / `BALL_FACTOR_FOOT` duplicate `verify.py` / `config.py` / `joints.py` (deliberately, for audit independence) with nothing asserting they stay equal; `verify.TOL_MM` and `MIN_ILLEGAL_LEN_MM` are both 0.05 but different concepts. | `chitubox_emit.py`, `tools/census_run.py`, `verify.py` | One typed provenance-carrying constant per physical quantity. Where independence requires duplication, an automated equality assertion. |
| D-39 | **Two acceptance predicates for the same merge.** `engine._consolidate_specialized_contacts` calls `_candidate_specialized_merge` WITHOUT `minima`/`island_births`/`baseline_specialized` (engine.py:334-337) then re-checks the specialized gate separately (338-341), while `specialized_physical_invariant_report` calls the SAME helper WITH them. The invariant report can flag a pair the consolidator legitimately declined, or vice versa. | `engine.py:334-341` | One predicate, one call site. |
| D-40 | **Mixed-provenance emit geometry.** `chitubox_emit` synthesizes tip/taper/shaft/flare/joiner from scratch, but pads and bridges are still clone-and-patch from a donor file (`chitubox_inject.clone_support`), and `make_bridge` raises `NotImplementedError` with no donor. `chitubox_inject.write_replace`'s own docstring warns it "staples correct supports onto the WRONG MODEL, silently, with a file that parses and verifies". Naive triangle-soup concatenation of touching primitives previously created coincident coplanar faces and ChituBox cavity defects, fixed four times at individual sites before being fixed at source. | `chitubox_emit.py`, `chitubox_inject.py` | All primitives synthesized from scratch and **boolean-unioned** at emit; an audit re-reads the artifact from disk and fails on boundary edges / negative shells / inconsistent winding. Do not port `chitubox_inject.write`/`write_replace`/`synth_member_chain`. |
| D-41 | **Duplicated stub/strut state machines.** `reaim_stub`/`reaim_foot`, `stub_candidates`/`strut_candidates`, `_resin_guards_ok`/`_strut_guards_ok`, `_count_moot_axis`/`_count_moot` differ mainly in ray set (axis-only vs axis+capsule) and ball factor. | `phase8.py` | One ladder generic over a `MemberKind` parameter. |
| D-42 | **Layer height is a scalar.** `cfg.layer_height_mm = 0.05` is used for `z_tol_mm = minima_connect_layers * layer_height_mm`, by `perception.island_births`, and by the emit boss sizing. `engine._ISLAND_BIRTH_LINK_R_MM`'s own comment flags that a dynamic-layer-height pass invalidates its tuning. | `config.py`, `engine.py`, `perception.py` | Per-layer array in the data model from day one, shipping a constant 0.050 mm until calibrated. |
| D-43 | **Blender is unreachable and unwired.** `decimate_pipeline.py` and `geometry_sweep.py` require `bpy`/`bmesh`, segfault headless in sandboxes, and are called by nothing in the repo — decimation is a fully manual out-of-band step today. `decimate_pipeline.run(obj=None)` also silently WIPES the open Blender scene (its own docstring warns). | `scripts/tools/decimate_pipeline.py`, `geometry_sweep.py` | Smart decimation is a first-class in-process Rust pipeline stage (§7 for the algorithm). No global-state scene mutation is representable. |
| D-44 | **Out-of-repo scratch paths.** `manual_emit_treant.py` and `manual_treant_v4.py` hardcode absolute outputs via `'..','..','..'` walks assuming a sibling directory layout and use `/tmp/treant_emit_state.pkl` / `/tmp/v4` instead of `_toolpaths.scratch_dir()`. The census tools key off a scratch pickle (`CENSUS_STATE`, default `v5p_treant_run1.pkl`) that is not reproducible from a fresh checkout without first running `run_v5.py` manually. | `scripts/tests/manual_*.py`, `scripts/tools/census_*.py` | Delete the manual scripts. Any surviving harness bootstraps its own inputs from a fresh checkout. |
| D-45 | **Cosmetic signals that files were never read end to end.** `import dataclasses` repeated inside `engine.prepare` (line 514) despite the module-level import; `scipy.spatial.cKDTree` imported inside three functions; `transient._postorder` names its exclusion set `live_braces` but builds it from ALL braces with no dead filter; `coverage_report`'s guard admits `threshold == 0.0` while its message claims to require positive. | `engine.py`, `transient.py`, `verify.py` | Noted so you do not treat these files as carefully maintained references. Re-derive, then check against the code. |
| D-46 | **Hollow and drill are welded together, so an already-hollowed-but-undrilled model cannot be processed at all.** `hollow.triage_components` raises `PreHollowedError` (exit 2, no output) when it finds sealed shells inside another component's material. Recorded 2026-07-25 against `Awakened Tree.stl` — **which is the Treant**, i.e. the corpus baseline model (`04-migration-plan.md` M0): `2 closed shell(s) sealed inside another component's material (largest: 9096 faces, 55.3067x24.3381x64.281mm)`. **The guard is correct and must not be removed:** re-hollowing would fill the real cavity, read the existing shell wall as solid material, and carve a new cavity out of the wall, silently destroying the model. **Do not route around it with `--no-hollow`** either — that emits an *undrilled* hollow model, i.e. sealed liquid resin, a suction and burst risk during peel, which is worse than not printing it at all. The only fix is to separate the stages. Hole placement depends on which way is down, so the order is **hollow → orient → drill**. | `hollow.py` (`run`, `triage_components`), `pipeline.prep_one` | Carve and drill are separately orderable stages in `plinth-hollow`, so a pre-hollowed input skips carve, keeps its existing cavity, and still gets drains and vents. Same welding as D-6 (orientation), different victim — fix both with one decomposition. |

---

## 12. Tools census — fold in / keep / delete

Disposition of every file under `scripts/tools/` and every `manual_`/helper file under
`scripts/tests/`.

| File | Disposition | Rationale |
|---|---|---|
| `tools/prep_one.py` | **fold-into-CLI** | Becomes `plinth` proper. Preserve the JSONL-on-stdout / stderr-progress / exit-code 0-5 contract or rewrite the integration tests in lockstep. |
| `tools/hollow_drill.py` | **fold-into-CLI** | Its stage function becomes the `hollow` stage **inside** `plinth prepare`. There is no separate `plinth hollow` subcommand — the CLI's subcommands are exactly `prepare`, `score`, `calibrate` (02 §2). |
| `tools/emit_supports.py` | **delete** | The bypass path (D-5). Its capability is **not** re-exposed: there is no `--skip-hollow` flag, because skipping hollow is what skipped orientation (D-6). |
| `tools/run_v5.py` | **delete** | Ad-hoc `engine.run` driver + pickle dump; superseded by the CLI plus structured reports. |
| `tools/run_engine_full.py` | **delete** | Same. |
| `tools/recheck_rewire_cap.py` | **delete** | One-off investigation of D-32; the Rust design removes the wall-clock cap it investigates. |
| `tools/decimate_pipeline.py` | **fold-into-CLI** (algorithm only) | Its staged QEM logic becomes the native Rust decimation stage (§7). The Blender delivery mechanism is deleted. |
| `tools/geometry_sweep.py` | **delete** | Blender-only manual mesh repair, unwired. Revisit as a native mesh-repair stage only if a real input demands it. |
| `tools/census_core.py` | **keep-as-Rust-dev-tool** | The independent audit oracle's primitives. Reimplement in Rust as a SEPARATE crate that shares no code with the engine kernel — the independence is the whole value. |
| `tools/census_run.py` | **keep-as-Rust-dev-tool** | PASS A/B/C1/C2/C3 audit driver. Must stay literal-identical to `verify.py`'s formulas and be bootstrappable from a fresh checkout (fixes D-44). |
| `tools/p11_crossexam.py` | **keep-as-Rust-dev-tool** | Third-opinion 16-ray inside oracle + transit classification cross-check. |
| `tools/validate_buried_end_fix.py` | **delete** | One-off validation of a landed fix; its assertion belongs in the Rust test suite. |
| `tools/validate_r3_ball_gate.py` | **delete** | Same. |
| `tools/bench_kernel.py` | **keep-as-Rust-dev-tool** | Becomes a criterion benchmark over the Rust kernel. |
| `tools/replay_parity.py` | **keep-as-Rust-dev-tool** | Differential replay against a captured call corpus; the mechanism survives the death of the Python oracle if the corpus is frozen as data. |
| `tools/instrument.py` | **keep-as-Rust-dev-tool** | Call-size histograms + bounded reservoir corpus capture (Algorithm R). Becomes tracing/telemetry in the kernel, not monkeypatching. |
| `tools/recon_chitubox.py` | **delete** | Analysis-only; its findings are already in `../specs/CHITUBOX-FORMAT.md`. |
| `tools/common.py` | **delete** | Geometry-array packing + digest helpers for `run_v5.py` only. |
| `tools/_toolpaths.py` | **delete** | Path plumbing for scripts that are themselves deleted; Rust binaries take explicit paths. |
| `tests/manual_emit_treant.py` | **delete** | Pre-`prep_one` staged driver, out-of-repo hardcoded paths (D-44). |
| `tests/manual_run_treant.py` | **delete** | Calls superseded v2/v3-era internals (`corpus`, `routing.route_to_anchor`, trunks/braces model). |
| `tests/manual_treant_v4.py` | **delete** | Same, plus D-44. |
| `tests/manual_perf_levers.py` | **delete** | Exercises the `perf_*` levers that are OFF by default and disappear with the typed config. |
| `tests/manual_perf_probe.py` | **delete** | Ad-hoc profiling; replaced by criterion + the Rust telemetry. |
| `tests/conftest.py` | **keep-as-Rust-dev-tool** (concept) | The PyO3 staleness gate dies with PyO3, but preserve the **fail-not-skip** discipline in the Rust harness. |
| `tests/_perf_fingerprint.py` | **keep-as-Rust-dev-tool** (concept) | Exact-bytes determinism fingerprint. Reimplement with an equivalently exact Rust serialization. |
| `tests/_refreeze_v5.py` | **keep-as-Rust-dev-tool** (concept) | Canonical-signature fixture refreeze; the same tool is needed to freeze the Rust baselines. |

`scripts/tools/host.blend` is a Blender asset, not Python; it goes with `geometry_sweep.py`.

---

## 13. Completeness audit — every non-test `.py` file in the repo

64 files. Each is listed with the section that covers it and its disposition. `.venv/` and
`target/` are excluded (build/vendor).

### `scripts/support_placement/` (35 files) — historical inventory; O-7 retires each at native handoff

| File | Section | Note |
|---|---|---|
| `__init__.py` | §2 | Package marker. |
| `engine.py` | §2 | Support engine + fix loop. |
| `pipeline.py` | §2 | Stage composition, `PipelineReport`, exit codes. |
| `p8_accept.py` | §2, §6 | Run-level acceptance gates, unwired (D-10). |
| `prepared_validation.py` | §2 | Capability token; becomes ownership in Rust. |
| `corpus.py` | §2 | Offline calibration analyzer; dead weight in the runtime package (D-33). |
| `config.py` | §3 | The 117-field `SupportConfig`; see `03-physics-and-constants.md`. |
| `perception.py` | §3 | Islands, minima, island births, delicacy field. |
| `placement.py` | §3 | Candidate pool, sampling, load partition, tip/shaft sizing. |
| `coverage.py` | §3 | A3 coverage samples. |
| `specialized_contacts.py` | §3 | Shared-seat demand reconciliation. |
| `tree.py` | §4 | Member/Forest, certificates, checkpoint/rollback. |
| `cco.py` | §4 | CCO insertion, Xia/Murray sizing. |
| `rewire.py` | §4 | Topology local search. |
| `bracing.py` | §4 | Cross-link and model-rung bracing. |
| `joints.py` | §4 | Joint-ball clustering + enclosure certificate. |
| `routing.py` | §4 | Surface tree, legacy Route clearance. |
| `struts.py` | §4 | Model-anchor candidates (only `find_model_anchor_candidates` is live). |
| `trunks.py` | §4 | Dead v3 (D-33). |
| `braces.py` | §4 | Dead v3, pulls `networkx` (D-33). |
| `chord_canon.py` | §4 | Integer-micron chord canonicalization; consumed by `p8_accept`. |
| `verify.py` | §5 | Every hard gate + the SCAR/PRINT-FLOOR certificate. |
| `geometry_exact.py` | §5 | Frozen numpy oracle; already ported to `geom_kernel`. |
| `transient.py` | §5 | Build-time span model + resize. |
| `topology.py` | §5 | Face adjacency, cut flags, digest validation. |
| `removal.py` | §5 | HANDLE/PEEL accessibility, report-only (D-25). |
| `internal.py` | §5 | A5 internal-cavity planner. |
| `phase8.py` | §6 | Relocation state machines + pocket solver. |
| `phase8_driver.py` | §6 | Site construction, splice, round/sweep loop. |
| `hollow.py` | §7 | Triage, orientation, hollow, drill, verify. |
| `mesh_io.py` | §7 | `load_stl(process=False)` + synthetic fixtures. |
| `geometry_kernel.py` | §7 | PyO3 wrapper over `geom_kernel`; deleted with the bindings. |
| `emit.py` | §8 | Emit orchestration, frame invariant, read-back. |
| `chitubox_emit.py` | §8 | Support-record geometry synthesis (all of it). |
| `chitubox_inject.py` | §8 | Superseded byte-patching path (D-40); keep read side only until `corpus.py` dies. |

### `scripts/tools/` (19 files) — §9, dispositions in §12

`_toolpaths.py`, `bench_kernel.py`, `census_core.py`, `census_run.py`, `common.py`,
`decimate_pipeline.py`, `emit_supports.py`, `geometry_sweep.py`, `hollow_drill.py`,
`instrument.py`, `p11_crossexam.py`, `prep_one.py`, `recheck_rewire_cap.py`, `recon_chitubox.py`,
`replay_parity.py`, `run_engine_full.py`, `run_v5.py`, `validate_buried_end_fix.py`,
`validate_r3_ball_gate.py`.

### `scripts/tests/` non-test files (8) — §9, dispositions in §12

`_perf_fingerprint.py`, `_refreeze_v5.py`, `conftest.py`, `manual_emit_treant.py`,
`manual_perf_levers.py`, `manual_perf_probe.py`, `manual_run_treant.py`, `manual_treant_v4.py`.

(Everything else under `scripts/tests/` is `test_*.py` and out of scope for this audit; those files
are the parity evidence discussed in `04-migration-plan.md`.)

### Rust crate Python shims (2) — historical inventory; O-7 deletes each at native handoff

| File | Note |
|---|---|
| `rust/chitubox_io/python/chitubox_io/__init__.py` | Loads `_native`, checks `API_VERSION == 1`. O-7 supersedes final-milestone retention: delete with `src/py.rs` when the native Rust boundary lands. |
| `rust/geom_kernel/python/geom_kernel/__init__.py` | Loads `_native`, checks `KERNEL_API_VERSION == 1` and numpy major 2. O-7 supersedes final-milestone retention: delete with the bindings when the native Rust boundary lands. |

Historically assigned to the final milestone, but superseded by O-7 for incremental
native-boundary deletion: `pyproject.toml`, `uv.lock`, `.venv/`,
`rust/chitubox_io/src/py.rs`, the **inline** PyO3 surface in `rust/geom_kernel/src/lib.rs`
(`geom_kernel` has **no** `py.rs` — the `#[pymodule]`, `#[pyclass]` and `#[pyfunction]` items live
directly in `lib.rs`, currently un-gated by any feature), the `rust/*/python/` shim directories,
and the `pyo3`/`numpy` dependencies in each crate's `Cargo.toml`.
