# plinth-3d Rust Workspace Inventory

Mechanical census, no judgment. Facts only.


## Workspace Summary

- **Crates:** 15
- **Total source lines:** 178,821
- **Total test lines:** 32,531
- **Source:test ratio:** 1:0.18

## Per-Crate Inventory


### chitubox_io

**Path:** rust/chitubox_io

| Metric | Count |
| --- | --- |
| Source lines | 2491 |
| Test lines | 2634 |
| Modules (files) | 13 |
| #[cfg(test)] modules | 0 |
| #[test] functions | 0 |
| #[ignore] functions | 0 |

**Dependencies:** None

**Modules in src/:**
  - codec.rs
  - consts.rs
  - document.rs
  - error.rs
  - lib.rs
  - parse.rs
  - project.rs
  - stl.rs
  - support.rs
  - transform.rs
  - types.rs
  - verify.rs
  - write.rs

**Public exports (from lib.rs):** 4 items
  - const: CHITUBOX_API_VERSION
  - use: error, project, types


### geom_kernel

**Path:** rust/geom_kernel

| Metric | Count |
| --- | --- |
| Source lines | 8931 |
| Test lines | 3913 |
| Modules (files) | 15 |
| #[cfg(test)] modules | 5 |
| #[test] functions | 24 |
| #[ignore] functions | 0 |

**Dependencies:** None

**Modules in src/:**
  - bvh.rs
  - closest.rs
  - consts.rs
  - lib.rs
  - mesh.rs
  - moller.rs
  - par.rs
  - parity.rs
  - point_index.rs
  - queries.rs
  - segment_batch.rs
  - slicing.rs
  - testkit.rs
  - vec3.rs
  - winding.rs

**Public exports (from lib.rs):** 50 items
  - const: KERNEL_API_VERSION, MOLLER_PARAMETER_SLACK
  - enum: SurfaceBuildError, SurfaceIndexKind, SurfaceSubsetError, OrderedMapError
  - fn: fn, triangle_soup_digest, indexed_triangle_soup_digest, fn, fn, fn, fn, borrowed, triangles, normals, areas, bounds, owned, estimate_index_memory_from_indexed, from_triangle_soup, from_indexed, nfaces, face_ids, bounds, fn, geometry, fn, current_threads, fn, fn, values, into_values, ordered_index_map
  - mod: testkit
  - struct: SurfaceTriangleSoupDigest, NativeSurfaceConfig, NativeSurfaceMemoryEstimate, TriangleSoup, OrderedMapLimits, OrderedMap
  - use: consts, mesh, point_index, queries, queries, segment_batch, slicing, vec3, winding


### plinth-cli

**Path:** crates/plinth-cli

| Metric | Count |
| --- | --- |
| Source lines | 1804 |
| Test lines | 341 |
| Modules (files) | 3 |
| #[cfg(test)] modules | 2 |
| #[test] functions | 17 |
| #[ignore] functions | 0 |

**Dependencies on workspace crates:**
  - plinth-core
  - plinth-hollow
  - plinth-mesh
  - plinth-orient
  - plinth-pipeline

**Modules in src/:**
  - execute.rs
  - lib.rs
  - main.rs

**Public exports (from lib.rs):** 10 items
  - const: ROOT_USAGE
  - enum: CacheMode, OrientSelection, CliAction, ParseError
  - fn: fn, parse_args
  - mod: execute
  - struct: PrepareSelection
  - use: PREPARE_USAGE


### plinth-core

**Path:** crates/plinth-core

| Metric | Count |
| --- | --- |
| Source lines | 7358 |
| Test lines | 5826 |
| Modules (files) | 18 |
| #[cfg(test)] modules | 1 |
| #[test] functions | 0 |
| #[ignore] functions | 0 |

**Dependencies:** None

**Modules in src/:**
  - chord.rs
  - constants.rs
  - cost.rs
  - defect.rs
  - digest.rs
  - frame.rs
  - id.rs
  - layer.rs
  - lib.rs
  - load.rs
  - metric.rs
  - number.rs
  - printer.rs
  - provenance.rs
  - stage.rs
  - violation.rs
  - constants/evidence.rs
  - constants/placeholders.rs

**Public exports (from lib.rs):** 15 items
  - use: chord, constants, cost, defect, digest, frame, id, layer, load, metric, number, printer, provenance, stage, violation


### plinth-decimate

**Path:** crates/plinth-decimate

| Metric | Count |
| --- | --- |
| Source lines | 6275 |
| Test lines | 2082 |
| Modules (files) | 1 |
| #[cfg(test)] modules | 2 |
| #[test] functions | 80 |
| #[ignore] functions | 1 |

**Dependencies on workspace crates:**
  - geom_kernel
  - plinth-core
  - plinth-mesh

**Modules in src/:**
  - lib.rs

**Public exports (from lib.rs):** 75 items
  - const: NONE, SUPPORT_FACE_SCALED_RESOURCE_DEMAND
  - enum: SimplifyError, ProbePopulation, ProbeError, AdaptiveProbeResult, FinalVerificationResult, DownstreamUtilityEvidence, ReductionRefusalReason, BudgetSearchOutcome, DecimationDisposition, BudgetConfigError, VerificationFailureReason, BudgetSearchError, FeatureProtectionError, DecimationError
  - fn: fn, fn, ratified_default, validate_for_execution, zero_volume_repair_total, fn, fn, into_mesh, fn, fn, fn, fn, fn, fn, fn, fn, fn, fn, fn, fn, fn, fn, fn, fn, fn, mesh, fn, fn, into_parts, into_decimated_model, run, select_feature_protection, backward_error_map, probe_two_sided, simplify_locked
  - struct: SimplifyRequest, SimplifyOutcome, ZeroVolumeRepairRecord, ProbeConfig, SelectionEvidence, TwoSidedProbe, DecimationBudget, ErrorBudget, ScaleSanity, BudgetSearchStep, BudgetSearchTrace, FinalVerificationAttempt, AdaptiveExplorationTrace, IntegerTargetInterval, AdaptiveProbeStep, DownstreamUseful, BudgetSearchSuccess, DecimationAudit, DecimationReport, DecimatedModel, VerificationFailure, FeatureProtectionAttempt, FeatureProtectionTrace
  - use: CandidateFreeCause


### plinth-emit

**Path:** crates/plinth-emit

| Metric | Count |
| --- | --- |
| Source lines | 2480 |
| Test lines | 401 |
| Modules (files) | 1 |
| #[cfg(test)] modules | 1 |
| #[test] functions | 27 |
| #[ignore] functions | 0 |

**Dependencies on workspace crates:**
  - chitubox_io
  - geom_kernel
  - plinth-core
  - plinth-decimate
  - plinth-hollow
  - plinth-mesh
  - plinth-orient
  - plinth-support

**Modules in src/:**
  - lib.rs

**Public exports (from lib.rs):** 4 items
  - enum: EmitError
  - fn: write
  - struct: EmitRequest, EmitReport


### plinth-hollow

**Path:** crates/plinth-hollow

| Metric | Count |
| --- | --- |
| Source lines | 16950 |
| Test lines | 155 |
| Modules (files) | 6 |
| #[cfg(test)] modules | 23 |
| #[test] functions | 126 |
| #[ignore] functions | 1 |

**Dependencies on workspace crates:**
  - geom_kernel
  - plinth-core
  - plinth-decimate
  - plinth-mesh
  - plinth-orient
  - plinth-solid

**Modules in src/:**
  - cavity_field.rs
  - drains.rs
  - drilling.rs
  - flow.rs
  - lib.rs
  - well_composed.rs

**Public exports (from lib.rs):** 132 items
  - enum: WallSelection, SolidDecisionReason, RealizedWallClaim, InternalSupportRefusalReason, HollowDisposition, HollowStageOutput, HollowError, HollowFailureCategory
  - fn: with_registered_wall_default, with_registered_handling_heavy_wall, registered_wall, registered_minimum_wall_mm, minimum_representable_wall_mm, checked_wall_mm, fn, fn, fn, fn, fn, fn, fn, fn, shortfall_mm3, solid_decision, fn, fn, fn, realized_wall_mm, fn, fn, cavities, drains, fn, fn, from_reports, fn, fn, into_reports, solid_decision, solid_decision_recording_repair, mesh, fn, fn, fn, into_parts, fn, fn, fn, approve_hollow, into_solid_fallback, fn, fn, fn, fn, into_error, report, run, from_registered_constants, from_request, fn, fn, fn, fn, fn, fn, fn, new, fn, fn, fn, fn, fn, fn, fn, fn, fn, fn, fn, fn, fn, fn, fn, fn, fn, fn, fn, fn, fn, fn, fn, fn, fn, fn, fn, fn, fn, fn, fn, fn, fn, fn, fn, fn, fn, fn, fn, fn, into_parts, create_shell, create_shell_with_admission_limits, create_shell_recording_repair, validate_closed_mesh, fn
  - mod: drains, drilling
  - struct: HollowRequest, RealizedWall, SavingsShortfall, CavityRepairEvidence, DrainRecord, CavityRecord, HollowReport, HollowHistory, HollowedModel, HollowedWithSolidReserve, HollowStageFailure, HollowSettings, H0AdmissionLimits, H0ResourceDiagnostics, CavityEvidence, ShellCandidate
  - use: cavity_field


### plinth-layers

**Path:** crates/plinth-layers

| Metric | Count |
| --- | --- |
| Source lines | 746 |
| Test lines | 758 |
| Modules (files) | 1 |
| #[cfg(test)] modules | 0 |
| #[test] functions | 0 |
| #[ignore] functions | 0 |

**Dependencies on workspace crates:**
  - geom_kernel
  - plinth-core

**Modules in src/:**
  - lib.rs

**Public exports (from lib.rs):** 12 items
  - const: SLICE_UNION_QUANTUM_MM, SLICE_UNION_MAX_ABS_MM
  - enum: LayerFillRule, LayerUnionError
  - fn: fn, normalize_layers_batch
  - struct: LayerLoops, LayerUnionLimits, NormalizedLoop, NormalizedSolid, NormalizedLayer, NormalizedLayers


### plinth-mesh

**Path:** crates/plinth-mesh

| Metric | Count |
| --- | --- |
| Source lines | 6920 |
| Test lines | 2083 |
| Modules (files) | 8 |
| #[cfg(test)] modules | 7 |
| #[test] functions | 29 |
| #[ignore] functions | 0 |

**Dependencies on workspace crates:**
  - geom_kernel
  - plinth-core

**Modules in src/:**
  - delicacy.rs
  - delicacy_eval.rs
  - fixtures.rs
  - io.rs
  - lib.rs
  - repair.rs
  - topology.rs
  - triage.rs

**Public exports (from lib.rs):** 19 items
  - enum: WeldPolicy, MeshError, StrictMeshAuditError
  - fn: new, verts, tris, retained_buffer_bytes, audit_strict, fn, bounds_z, face_normals_z
  - struct: Mesh
  - use: delicacy, delicacy_eval, fixtures, io, repair, topology, triage


### plinth-metrics

**Path:** crates/plinth-metrics

| Metric | Count |
| --- | --- |
| Source lines | 9176 |
| Test lines | 3396 |
| Modules (files) | 8 |
| #[cfg(test)] modules | 4 |
| #[test] functions | 49 |
| #[ignore] functions | 0 |

**Dependencies on workspace crates:**
  - chitubox_io
  - geom_kernel
  - plinth-core

**Modules in src/:**
  - authority.rs
  - geometry.rs
  - lib.rs
  - read.rs
  - scar.rs
  - score.rs
  - supports.rs
  - topology.rs

**Public exports (from lib.rs):** 17 items
  - enum: CoordinateKind, MetricsError, TriangleSide, GeometryPrimitive
  - fn: new, vertices, triangles
  - struct: Mesh
  - type: Vec3, Triangle
  - use: authority, geometry, read, scar, score, supports, topology


### plinth-orient

**Path:** crates/plinth-orient

| Metric | Count |
| --- | --- |
| Source lines | 4901 |
| Test lines | 1165 |
| Modules (files) | 5 |
| #[cfg(test)] modules | 3 |
| #[test] functions | 28 |
| #[ignore] functions | 0 |

**Dependencies on workspace crates:**
  - geom_kernel
  - plinth-core
  - plinth-decimate
  - plinth-mesh

**Modules in src/:**
  - grid.rs
  - lib.rs
  - score.rs
  - search.rs
  - section.rs

**Public exports (from lib.rs):** 25 items
  - enum: BaselineAxis, BaselineSelection, OrientationProvenance, OrientError
  - fn: identity, run, search_and_orient, registered_sweep_request, from_reports, fn, fn, into_reports, mesh, fn, into_parts
  - mod: grid, search
  - struct: PinnedBaseline, OrientRequest, RigidTransform, OrientationReport, OrientationHistory, OrientedModel
  - use: score, search


### plinth-pipeline

**Path:** crates/plinth-pipeline

| Metric | Count |
| --- | --- |
| Source lines | 7682 |
| Test lines | 3619 |
| Modules (files) | 4 |
| #[cfg(test)] modules | 4 |
| #[test] functions | 49 |
| #[ignore] functions | 0 |

**Dependencies on workspace crates:**
  - chitubox_io
  - plinth-core
  - plinth-decimate
  - plinth-emit
  - plinth-hollow
  - plinth-mesh
  - plinth-orient
  - plinth-support
  - plinth-verify

**Modules in src/:**
  - coverage_evidence.rs
  - gates.rs
  - lib.rs
  - prepare.rs

**Public exports (from lib.rs):** 45 items
  - const: SATURN4_ULTRA_CLI_VALUE
  - enum: PipelineExitCode, StageStop, OrientationPolicy, OutputProfile, OutputProfileError, PrepareResourcePolicyError, ModelPreparationOutcome, PipelineError
  - fn: hollow_stage, project_hollow_stage, pinned_for, baseline_for, resolve, for_saturn4_ultra, fn, fn, fn, fn, fn, coverage_evidence, repair_report, is_coverage_refusal, fn, artifacts, exit_code, has_gate_refusal, has_coverage_refusal, fn
  - mod: coverage_evidence, gates, prepare
  - struct: HollowStageProjection, PrepareResourcePolicy, PrepareRequest, ContactAttestationDisclosure, AssumedPrintSize, ModelPreparationReport, PrepareReport
  - use: coverage_evidence, plinth_support, plinth_support, plinth_support, prepare, gates


### plinth-solid

**Path:** crates/plinth-solid

| Metric | Count |
| --- | --- |
| Source lines | 1302 |
| Test lines | 280 |
| Modules (files) | 2 |
| #[cfg(test)] modules | 5 |
| #[test] functions | 10 |
| #[ignore] functions | 0 |

**Dependencies:** None

**Modules in src/:**
  - ffi.rs
  - lib.rs

**Public exports (from lib.rs):** 20 items
  - const: UNLIMITED
  - enum: MeshValidationError, ManifoldStatus, NativeValidationError, ManifoldInputLimit, NativeExceptionKind, BooleanInputLimit, BooleanOutputLimit, SolidError
  - fn: new, vertices, triangles, for_meshes, union_limited, difference_limited
  - struct: Mesh, BooleanInputEstimate, BooleanLimits, ManifoldBackend
  - trait: SolidBoolean


### plinth-support

**Path:** crates/plinth-support

| Metric | Count |
| --- | --- |
| Source lines | 100858 |
| Test lines | 4906 |
| Modules (files) | 27 |
| #[cfg(test)] modules | 160 |
| #[test] functions | 658 |
| #[ignore] functions | 0 |

**Dependencies on workspace crates:**
  - geom_kernel
  - plinth-core
  - plinth-decimate
  - plinth-hollow
  - plinth-layers
  - plinth-mesh
  - plinth-orient

**Modules in src/:**
  - anchor.rs
  - brace.rs
  - cco.rs
  - census.rs
  - contact.rs
  - coverage.rs
  - coverage_closure.rs
  - coverage_measurement.rs
  - coverage_repair.rs
  - designed_embed.rs
  - dyadic.rs
  - forest.rs
  - initial.rs
  - joints.rs
  - legacy_compat.rs
  - legalization.rs
  - lib.rs
  - lower.rs
  - member_sizing.rs
  - perception.rs
  - raft.rs
  - rewire.rs
  - runner.rs
  - seat_zone.rs
  - specialized.rs
  - transient.rs
  - transit_measurement.rs

**Public exports (from lib.rs):** 76 items
  - enum: FrustumRingNormal, SupportSegmentKind, SupportError, SupportPerceptionError, ContactSelectionError
  - fn: tip, tip_from_f64, fn, joint_from_f64, canonical_poles, is_tip, is_joint, tip_authority, joint_center, fn, fn, fn, from_reports, fn, fn, fn, fn, into_reports, from_parts, mesh, primitives, fn, fn, fn, into_parts, candidates, digest, current, perceive_contacts, contacts, coverage_samples, digest, current, current_defaults, fn, select_contacts
  - mod: anchor, census, coverage_closure, coverage_repair, dyadic, forest, legacy_compat, member_sizing, perception, raft, specialized, transient
  - struct: SupportRequest, SphereGeometry, BuildFrame, SupportPrimitive, SupportReport, SupportHistory, SupportedModel, ContactCandidateId, ContactCandidate, ContactCandidatePool, SupportPerceptionConfig, SupportIsland, CoverageSample, SelectedContact, SelectedContacts, ContactSelectionConfig, CoverageObjective
  - use: coverage_closure, coverage_measurement, coverage_repair, initial, runner, specialized


### plinth-verify

**Path:** crates/plinth-verify

| Metric | Count |
| --- | --- |
| Source lines | 947 |
| Test lines | 972 |
| Modules (files) | 1 |
| #[cfg(test)] modules | 1 |
| #[test] functions | 6 |
| #[ignore] functions | 0 |

**Dependencies on workspace crates:**
  - chitubox_io
  - geom_kernel
  - plinth-core
  - plinth-decimate
  - plinth-hollow
  - plinth-mesh
  - plinth-orient
  - plinth-support

**Modules in src/:**
  - lib.rs

**Public exports (from lib.rs):** 19 items
  - enum: CoverageVerificationError, StructuralFormatStatus, GuiLoadabilityCheckpoint, ArtifactReadFailure, StructuralFormatFailure, FormatFailureReason
  - fn: artifact, fn, defects, accepted, verify_coverage, artifact, defects, fn, fn, accepted, verify_artifact
  - struct: CoverageReport, FormatReport


## Workspace-Wide Pattern Counts


### Unwrap/Expect/Panic

| Pattern | Count | Top 10 files |
| --- | --- | --- |
| unwrap() | 2823 | cco.rs, brace.rs, specialized.rs, core_contracts.rs, perception.rs, lib.rs, init |
| expect( | 2465 | lib.rs, runner.rs, specialized.rs, drains.rs, lib.rs, perception.rs, prepare_fol |
| panic! | 270 | lib.rs, lib.rs, specialized.rs, runner.rs, constants_evidence.rs, prepare_folder |
| todo! | 10 | lib.rs, forest.rs |
| unimplemented! | 0 |  |

### Annotations and Comments

| Pattern | Count | Top 10 files |
| --- | --- | --- |
| #[allow( | 132 | specialized.rs, lib.rs, anchor.rs, perception.rs, runner.rs, coverage_closure.rs |
| #[deprecated | 0 | — |
| TODO | 1 | gates.rs |
| FIXME | 0 | — |
| HACK | 0 | — |
| XXX | 0 | — |

### Casts

| Cast | Count | Top 10 files |
| --- | --- | --- |
| as f32 | 40 | regen_corpus.rs, fixtures.rs, lib.rs, prepare_folder.rs, dual_cavity_real_pipeli |
| as f64 | 308 | lib.rs, scar.rs, lib.rs, raster_delta.rs, drains.rs, lib.rs, brace.rs, perceptio |
| as usize | 610 | perception.rs, queries.rs, lib.rs, delicacy_eval.rs, lib.rs, specialized.rs, lib |

### Batch-Named Functions with for Loops

| File | Function | for loops |
| --- | --- | --- |
| crates/plinth-support/src/transit_measurement.rs | collect_batch | 4 |
| crates/plinth-orient/tests/orientation_scoring.rs | score_all | 3 |
| crates/plinth-decimate/src/lib.rs | query_batch | 3 |
| crates/plinth-core/src/constants.rs | build_batch | 3 |
| crates/plinth-layers/src/lib.rs | normalize_layers_batch | 2 |
| crates/plinth-mesh/src/triage.rs | classify_batch | 2 |
| crates/plinth-metrics/src/scar.rs | frozen_contains_batch | 2 |
| crates/plinth-support/tests/contact_regression_gate.rs | attest_seats_batch | 2 |
| crates/plinth-support/src/census.rs | record_segment_batch | 2 |
| crates/plinth-support/src/brace.rs | axis_penetration_belt_rejects_inside_rods_in_one_batch | 2 |
| ... | ... | ... |

## Top 10 Files per Pattern


### unwrap()

| File | Count |
| --- | --- |
| crates/plinth-support/src/cco.rs | 356 |
| crates/plinth-support/src/brace.rs | 241 |
| crates/plinth-support/src/specialized.rs | 188 |
| crates/plinth-core/tests/core_contracts.rs | 171 |
| crates/plinth-support/src/perception.rs | 154 |
| crates/plinth-decimate/src/lib.rs | 111 |
| crates/plinth-support/src/initial.rs | 104 |
| crates/plinth-support/src/rewire.rs | 96 |
| rust/geom_kernel/tests/native_api.rs | 83 |
| crates/plinth-support/src/anchor.rs | 75 |

### expect(

| File | Count |
| --- | --- |
| crates/plinth-hollow/src/lib.rs | 292 |
| crates/plinth-support/src/runner.rs | 260 |
| crates/plinth-support/src/specialized.rs | 112 |
| crates/plinth-hollow/src/drains.rs | 107 |
| crates/plinth-support/src/lib.rs | 97 |
| crates/plinth-support/src/perception.rs | 77 |
| crates/plinth-pipeline/tests/prepare_folder.rs | 74 |
| crates/plinth-support/tests/support_runner_integration.rs | 68 |
| crates/plinth-mesh/tests/topology_triage.rs | 61 |
| crates/plinth-support/src/cco.rs | 61 |

### panic!

| File | Count |
| --- | --- |
| crates/plinth-hollow/src/lib.rs | 18 |
| crates/plinth-decimate/src/lib.rs | 17 |
| crates/plinth-support/src/specialized.rs | 17 |
| crates/plinth-support/src/runner.rs | 13 |
| crates/plinth-core/tests/constants_evidence.rs | 12 |
| crates/plinth-pipeline/tests/prepare_folder.rs | 9 |
| crates/plinth-pipeline/tests/dual_cavity_real_pipeline.rs | 9 |
| crates/plinth-support/tests/a4_transactional_controller.rs | 9 |
| crates/plinth-mesh/tests/fixtures.rs | 8 |
| crates/plinth-support/src/cco.rs | 8 |

### as usize

| File | Count |
| --- | --- |
| crates/plinth-support/src/perception.rs | 140 |
| rust/geom_kernel/src/queries.rs | 48 |
| crates/plinth-decimate/src/lib.rs | 40 |
| crates/plinth-mesh/src/delicacy_eval.rs | 23 |
| crates/plinth-hollow/src/lib.rs | 19 |
| crates/plinth-support/src/specialized.rs | 19 |
| crates/plinth-support/src/lib.rs | 17 |
| crates/plinth-mesh/src/topology.rs | 15 |
| crates/plinth-hollow/examples/raster_delta.rs | 14 |
| crates/plinth-support/src/seat_zone.rs | 14 |

### as f64

| File | Count |
| --- | --- |
| crates/plinth-hollow/src/lib.rs | 40 |
| crates/plinth-metrics/src/scar.rs | 23 |
| crates/plinth-decimate/src/lib.rs | 17 |
| crates/plinth-hollow/examples/raster_delta.rs | 16 |
| crates/plinth-hollow/src/drains.rs | 16 |
| crates/plinth-emit/src/lib.rs | 16 |
| crates/plinth-support/src/brace.rs | 14 |
| crates/plinth-support/src/perception.rs | 14 |
| crates/plinth-support/src/designed_embed.rs | 13 |
| crates/plinth-mesh/src/fixtures.rs | 10 |

### #[allow(

| File | Count |
| --- | --- |
| crates/plinth-support/src/specialized.rs | 14 |
| crates/plinth-support/src/lib.rs | 13 |
| crates/plinth-support/src/anchor.rs | 13 |
| crates/plinth-support/src/perception.rs | 13 |
| crates/plinth-support/src/runner.rs | 11 |
| crates/plinth-support/src/coverage_closure.rs | 9 |
| rust/geom_kernel/src/queries.rs | 9 |
| crates/plinth-support/src/transient.rs | 6 |
| crates/plinth-support/src/transit_measurement.rs | 5 |
| crates/plinth-support/src/coverage_measurement.rs | 5 |