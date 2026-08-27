# Lane x/constants-doctrine — outcome-deciding constants audit

Scope: `crates/plinth-core/src/constants.rs`, `constants/evidence.rs`,
`constants/placeholders.rs`, plus a workspace sweep for `const`/`static` float
declarations (166 sites found) and for numeric literals appearing in comparisons
in non-test production code.

Derivation status legend:

- **DERIVED-APPLIED** — a derivation exists and the shipping code evaluates it.
- **DERIVED-RECORDED** — a derivation exists in the registry/docs, but the
  deciding code uses a hard-coded restatement of its result.
- **EVIDENCED** — a measurement or owner ruling exists with a dated source; not
  a physics derivation, but honestly labelled as such.
- **NONE** — no derivation, no measurement, no owner ruling recorded at the
  deciding site.
- **INERT** — registered with provenance but no production reader.

---

## A. Registry-side: `Measured` / `Derived` (evidence-grade, `AcceptSafe`)

| Constant | Value | Unit | Declared at | Deciding site | Status |
|---|---|---|---|---|---|
| `X_STAR_CONE` | [25.00, 25.34] | 1 | evidence.rs:120 | — | INERT (no production reader) |
| `X_STAR_CYLINDER` | (19.91, 30.48) | 1 | evidence.rs:414 | — | INERT (cross-check only, by design) |
| `X_ALLOWED` | 12 | 1 | evidence.rs:359 | perception.rs:8213 (reach allowance only) | DERIVED-APPLIED at one site; DERIVED-RECORDED at score.rs:775 (`value > 12.0`) |
| `LOAD_SHARING` | additive N≤6 | count | evidence.rs:95 | — | INERT (physics.md §4 says sizing is deliberately per-arm) |
| `N_EFF_CAP` | 6.0 | count | evidence.rs:234 | — | INERT |
| `CCO_X_PAIR_LOAD_DISCOUNT` | 0.8408964152537145 | 1 | evidence.rs:179 | brace.rs:2108 via its own literal `CROSS_LINK_DUTY_FACTOR` | DERIVED-RECORDED |
| `FINAL_CLEARANCE_GAP_MM` | 0.300 | mm | evidence.rs:199 | brace.rs:140 → contact.rs `demanded_gap_mm` (Final) | DERIVED-APPLIED (owner field data) |
| `PRINT_FLOOR_CHORD_MM` | 0.08 | mm | evidence.rs:160 | transit/scar policy | EVIDENCED (owner ruling); name violates the "floor" vocabulary rule |
| `LAYER_HEIGHT_DEFAULT_MM` | 0.050 | mm | evidence.rs:210 | perception.rs:8221 | EVIDENCED |
| `PIXEL_PITCH_X_UM` / `Y_UM` | 14.0 / 19.0 | µm | evidence.rs:262 / 153 | decimation bounds | DERIVED-APPLIED |
| `DECIMATION_MAX_ABSOLUTE_DEVIATION_MM` | 0.08 | mm | evidence.rs:184 | plinth-decimate witness | DERIVED-APPLIED (= PRINT_FLOOR_CHORD_MM, identity checked) |
| `DECIMATION_MAX_P999_DEVIATION_MM` | 0.019 | mm | evidence.rs:189 | plinth-decimate | DERIVED-APPLIED (19/1000, pinned bit-for-bit by a named test) |
| `DECIMATION_MAX_P99_DEVIATION_MM` | 0.0095 | mm | evidence.rs:194 | plinth-decimate | DERIVED-APPLIED |
| `SATURN_BUILD_VOLUME_MM` | 211.68 × 118.37 × 220.0 | mm | evidence.rs:269 | plinth-emit/src/lib.rs:461 `validate_against` | DERIVED-APPLIED; contradicted by hollow's own 300.0 envelope gate |
| `TIP_DIAMETER_MIN_ALLOWED_MM` | 0.100 | mm **DIA** | evidence.rs:107 | validator minimum | EVIDENCED (owner ruling 2026-08-19) |
| `TIP_RADIUS_MIN_ALLOWED_MM` | 0.050 | mm **RAD** | evidence.rs:354 | validator minimum | DERIVED-APPLIED (= DIA/2, checked by test) |
| `TIP_RADIUS_MAX_GROWN_SCAR_MM` | 0.400 | mm **RAD** | evidence.rs:343 | initial.rs:618, designed_embed.rs:4220 | EVIDENCED (owner field data) |
| `SUPPORT_KNN_MAX_WORK_PER_FACE` | 2048 | count | evidence.rs:309 | runner.rs:205 | DERIVED-APPLIED (`next_power_of_two(2×696)`, verified) |
| `SUPPORT_PARITY_GRID_MAX_BYTES_PER_FACE` | 512 | B | evidence.rs:331 | runner.rs:207 | DERIVED-APPLIED (81+36×2.8303 = 182.89 → 512, verified) |
| `SUPPORT_GATE_MAX_WORK_PER_BATCH_UNIT` | 64 | count | evidence.rs:298 | SpecializedBatchWork | DERIVED-APPLIED (`nptwo(2×23)`, verified) |
| `SUPPORT_LOAD_MAX_WORK_PER_BATCH_UNIT` | 64 | count | evidence.rs:320 | SpecializedBatchWork | DERIVED-APPLIED (`nptwo(2×31)`, verified) |
| `P8_POCKET_VOXEL_MM` | 6.0 | mm | evidence.rs:277 | — | INERT |
| `P8_CHORD_CANON_MICRON` | 1 | µm | evidence.rs:265 | — | INERT |
| `SATURN_RELEASE_FILM_THICKNESS_MM` | 0.127 | mm | evidence.rs:290 | — | INERT |
| `MINIMA_DROP_INTERIOR_FACES` / `..REQUIRE_ISLAND_BIRTH` | true / true | bool | evidence.rs:226/232 | — | INERT |

## B. Registry-side: `Placeholder` (non-AcceptSafe by their own label) that decide outcomes

These are the doctrine-1 core. Each is read through `Placeholder::diagnostic()`,
whose own doc reads "forbidden on acceptance paths", and each reaches an
accept/refuse or geometry-changing decision.

| Constant | Value | Unit | Declared at | Deciding site | Status |
|---|---|---|---|---|---|
| `PEEL_PRESSURE_PA` | 3000.0 | Pa | placeholders.rs:1340 | perception.rs:628 → `tip_radius_mm` / `violates_safety_factor` | NONE ("Legacy diagnostic split, potentially about four times low") |
| `TENSILE_GREEN_MPA` | 0.288 | MPa | placeholders.rs:1524 | perception.rs:629 | NONE ("not a measured material property") |
| `TIP_SAFETY_FACTOR` | 8.0 | 1 | placeholders.rs:1532 | perception.rs:630 | NONE ("selected as policy") |
| `TIP_RADIUS_DEFAULT_MM` | 0.050 | mm **RAD** | placeholders.rs:1521 | perception.rs:631 (sizing clamp minimum) | EVIDENCED (owner ruling) |
| `TIP_RADIUS_MAX_LEGACY_MM` | 1.0 | mm **RAD** | placeholders.rs:1530 | perception.rs:632 (sizing clamp maximum) | NONE ("retained as legacy policy") |
| `COVERAGE_RADIUS_FACTOR` | 1.5 | 1 | placeholders.rs:1067 | coverage_closure.rs:203 → gates.rs:259 `UncoveredIslands` | NONE ("selected without measurement") |
| `TARGET_SPACING_MM` | 4.0 | mm | placeholders.rs:1522 | coverage_closure.rs:204 | NONE |
| `VOXEL_PITCH_MM` | 0.4 | mm | placeholders.rs:1544 | coverage_closure.rs:232 (censor area = pitch²) | NONE ("inferred from one corpus run") |
| `STANDOFF_MM` | 0.6 | mm | placeholders.rs:1380 | cco/rewire/brace placement clearance | EVIDENCED (owner ruling R-1495; search-only, prunes) |
| `MIN_ILLEGAL_LEN_MM` | 0.05 | mm | placeholders.rs:1300 | contact.rs `min_free_span_mm` | NONE |
| `BRACING_MAX_STACKED_FALLBACKS` | 6 | count | placeholders.rs:1046 | brace.rs:126 | NONE (self-labelled "Result-changing") |
| `COVERAGE_REPAIR_MAX_ATTEMPTS` | 8 | count | placeholders.rs:1082 | coverage repair loop | NONE (self-labelled "Result-changing") |
| `SURFACE_TREE_MAX_POINTS` / `..PITCH_MM` | 150000 / 0.4 | count / mm | placeholders.rs:1512/1514 | sampled-surface build | NONE (self-labelled "Result-changing") |
| `REWIRE_DEGENERATE_LEN_MM` | 1e-4 | mm | placeholders.rs:1360 | rewire.rs:149 (and cco.rs:39 by literal twin) | NONE (self-labelled "Result-changing") |
| `REWIRE_TWIG_MERGE_LEN_MM` | 0.5 | mm | placeholders.rs:1368 | rewire.rs:148 | NONE (self-labelled "Result-changing") |
| `HOLLOW_SAVE_MIN_ML` | 1.5 | mL | placeholders.rs:1180 | hollow savings gate | EVIDENCED (7 owner verdicts, separation argument) |
| `HOLLOW_WALL_MIN_MM` | 2.0 | mm | placeholders.rs:1202 | `checked_realized_wall_mm` | EVIDENCED (owner ruling + measurement) |
| `HOLLOW_CAVITY_MARCH_MARGIN_MM` | 0.0 | mm | placeholders.rs:1146 | cavity selection | EVIDENCED (owner ruling 2026-08-13) |
| `HOLLOW_DRAIN_R_MM` | 1.25 | mm **RAD** | placeholders.rs:1164 | flow.rs:120 `registered_throat_radius_mm` | DERIVED-APPLIED (0.5·√n lattice argument checks out) |
| `HOLLOW_EXTRA_DRAIN_POLICY.extensions_mm` | [4.0, 8.0, 9.0] | mm | placeholders.rs:1163 | drains.rs:407-417 keepouts | DERIVED-RECORDED (drilling.rs:1251-2, 1418-9, 1423 restate 4.0/4.0/5.0/4.0 as literals) |
| `A_ABS_MM2` / `KAPPA` / `L_SCAR_MAX_MM` | 2.0 / 1.25 / 2.5 | mm² / 1 / mm | placeholders.rs:989/1305/1310 | SCAR-AREA-CAP policy | NONE ("Owner-ratified … with no literature measurement") |
| `BALL_FACTOR_STUB` / `BALL_FACTOR_FOOT` | 1.5 / 1.4 | 1 | placeholders.rs:993/991 | anchor.rs:145, brace.rs:141, scar.rs:189 | NONE (joints.rs:15 keeps a literal twin of the foot factor) |
| `MAX_UNBRACED_SHAFT_R_MM` | 0.9 | mm | placeholders.rs:1302 | brace.rs:137 | NONE ("Corpus rationale does not derive the … ceiling") |
| `OVERHANG_ANGLE_DEG` | 45.0 | ° | placeholders.rs:1332 | perception overhang mask | NONE ("Legacy tool convention lacking the required geometric-extent criterion") |
| `MAX_SEGMENT_ANGLE_DEG` / `BRANCH_ANGLE_DEG` | 45.0 / 60.0 | ° | placeholders.rs:1298/1063 | member_sizing.rs:621 printability cone | NONE |
| `MAX_DEFLECTION_MM` | 0.15 | mm | placeholders.rs:1296 | member_sizing.rs:439 | NONE ("Subjective visual-ribbing serviceability threshold") |
| `TRANSIENT_SHOOT_COUNT_EXPONENT` | 0.125 | 1 | placeholders.rs:1529 | transient.rs | NONE ("Inherited N-to-the-one-eighth heuristic without physical authority") |
| `MIN_SHAFT_R_MM` / `MIN_SHOOT_R_MM` / `MIN_TRUNK_R_MM` | 0.3 / 0.35 / 0.8 | mm | placeholders.rs:1289-1293 | member_sizing.rs:467, cco | NONE |
| `DECIMATION_UTILITY_MIN_INPUT_FACES` | 32768 | count | placeholders.rs:1104 | plinth-decimate gate | EVIDENCED (explicitly argued as non-derivable) |

## C. Production-side constants that decide outcomes with **no registry entry at all**

| Name | Value | Unit | file:line | Decides | Status |
|---|---|---|---|---|---|
| `FOOT_BREACH_HALF_DEG` | 25.0 | ° | plinth-support/src/anchor.rs:34 | `AnchorFootOutcome::Refused{FootBallBreached}` (anchor.rs:1746), brace rung survival (brace.rs:1412) | NONE ("Frozen kernel cone defaults from SPEC-RUST-KERNEL"); its own comment says the result-changing values come from M0 |
| `FOOT_BREACH_CONE_RAYS` | 8 | count | plinth-support/src/anchor.rs:36 | same | NONE |
| `NEAR_WALL_SKIP` | 0.05 | mm | geom_kernel/src/consts.rs:35 | `cone_thickness` far-wall selection inside `breach_one` | NONE (provenance is `geometry_exact.py:842`, a file with no surviving source) |
| `ZDEDUP` | 5e-3 | mm | geom_kernel/src/consts.rs:22 | parity crossing-cluster dedup | NONE; duplicated by `AXIS_CROSSING_CLUSTER_MM` |
| `AXIS_CROSSING_CLUSTER_MM` | 0.005 | mm | plinth-metrics/src/scar.rs:1282 | scar.rs:1406 crossing count | NONE (second owner of ZDEDUP's quantity) |
| `DEDUP_LOC_MM` | 3e-3 | mm | geom_kernel/src/consts.rs:26 | ray-hit arc dedup | NONE |
| `EPS` / `DET_EPS` / `SEGSEG_EPS` / `PARITY_*` | 1e-6 / 1e-14 / 1e-14 / 1e-9,1e-14 | 1 | geom_kernel/src/consts.rs:6-32 | Möller accept, parity containment | NONE (all "copied VERBATIM from geometry_exact.py") |
| `PRINTABLE_MAX_EXTENT_MM` | 300.0 | mm | plinth-hollow/src/lib.rs:2178 | `HollowError::OutsidePrintableEnvelope` at lib.rs:3727 | NONE; disagrees with the measured 211.68/118.37/220.0 envelope |
| bare `0.1` (×2) | 0.1 | mm | plinth-support/src/brace.rs:1940, 1962 | which rods get the penetration-interval check vs the midpoint test in `axis_illegal_rows` | NONE |
| bare `12.0` | 12.0 | 1 | plinth-metrics/src/score.rs:775 | `MetricOutcome::HardBoundViolation` for `max_x` | DERIVED-RECORDED (this is `X_ALLOWED`) |
| `CROSS_LINK_DUTY_FACTOR` | 0.8408964152537145 | 1 | plinth-support/src/brace.rs:39 | paired cross-link radius (brace.rs:2108) | DERIVED-RECORDED (= `CCO_X_PAIR_LOAD_DISCOUNT`) |
| `MODEL_FOOT_FACTOR` | 1.4 | 1 | plinth-support/src/joints.rs:15 | joints.rs:870 ball radius | DERIVED-RECORDED (= `BALL_FACTOR_FOOT`) |
| `COINCIDE_TOL_MM` | 0.15 | mm | plinth-support/src/joints.rs:16 | joints.rs:1176, 1212 clustering | DERIVED-RECORDED (registry `COINCIDE_TOL_MM` has zero readers) |
| `ON_AXIS_TOL_MM` | 0.2 | mm | plinth-support/src/joints.rs:17 | joints.rs:855 | DERIVED-RECORDED (registry entry has zero readers) |
| `PLATE_BAND_MM` | 0.2 | mm | plinth-support/src/joints.rs:18 | joints.rs:698, 704, 986, 995 | DERIVED-RECORDED (raft.rs:189 and lower.rs:82 read the registry for the same quantity) |
| `COLINEAR_RADIUS_FRAC` | 0.20 | 1 | plinth-support/src/joints.rs:19 | joints.rs:1253 | DERIVED-RECORDED |
| `COLINEAR_COS_MAX` | −0.984807753012208 | 1 | plinth-support/src/joints.rs:20 | joints.rs:1254 | DERIVED-RECORDED (= cos(180° − `COLINEAR_ANGLE_DEG`=10°); the conversion is pre-evaluated, uncommented, and the registry entry has zero readers) |
| `ENCLOSURE_PROOF_TOL_MM` | 0.018 | mm | plinth-support/src/joints.rs:21 | joints.rs:1293 enclosure certificate | DERIVED-RECORDED |
| `DEGENERATE_LENGTH_SQUARED` | 1e-18 | mm² | plinth-support/src/joints.rs:22 | point-segment degeneracy | NONE |
| `BIRTH_LINK_RADIUS_MM` | 0.6 | mm | plinth-support/src/specialized.rs:35 | specialized.rs:7741 island-birth contact merge | DERIVED-RECORDED (= `ISLAND_BIRTH_LINK_R_MM`, zero registry readers) |
| `SYNTHETIC_PATCH_RADIUS_MM` | 1.5 | mm | plinth-support/src/specialized.rs:34 | specialized.rs:2479 synthetic-island radius query | NONE |
| `REWIRE_DEGENERATE_MM` | 1e-4 | mm | plinth-support/src/cco.rs:39 | cco.rs:4666 | DERIVED-RECORDED (= `REWIRE_DEGENERATE_LEN_MM`, which rewire.rs:149 reads properly) |
| `DEGENERATE_MM` | 1e-6 | mm | plinth-support/src/cco.rs:38 | cco.rs:89, 587, 5046, 5111, 5115 | NONE |
| `PEEL_FRONT_ROOT_MOMENT_COEFFICIENT` | 6.0 | 1 | plinth-support/src/perception.rs:2408 | reach allowance | DERIVED-APPLIED (closed-form beam coefficient, argued in place; deliberately not registered) |
| `AREA_DOUBLING_RADIUS_RATIO` | √2 | 1 | plinth-support/src/designed_embed.rs:1178 | seat ladder | DERIVED-APPLIED |
| `FLOORED_RUNG_INSET_MM` | 2.5 | mm | plinth-support/src/census.rs:25 | census.rs:984 report filter | DERIVED-RECORDED (comment `0.3 + (0.5 + 0.6) / 0.5` pre-evaluated); diagnostic-only, env-gated |
| `SLICE_UNION_QUANTUM_MM` / `..MAX_ABS_MM` | 1e-8 / 1e4 | mm | plinth-layers/src/lib.rs:69, 78 | coordinate refusal | DERIVED-APPLIED — **the correct pattern**: pinned to the registry by `crates/plinth-layers/tests/registry_pin.rs:6` |

## D. Cap sets whose registry entries name the call site but the call site does not read the registry

`SupportPreparationLimits::production()` (`crates/plinth-support/src/runner.rs:194`)
reads exactly four values from the registry and hard-codes the rest. Every one
of the hard-coded values has a registered constant whose note names this very
function (owner ruling R5, 2026-08-05).

| Registry constant | Registered value | Literal in production() |
|---|---|---|
| `SUPPORT_KNN_MAX_SOURCE_POINTS` | 20000000 | runner.rs:210 `20_000_000` |
| `SUPPORT_KNN_MAX_QUERY_POINTS` | 20000000 | runner.rs:211 `20_000_000` |
| `SUPPORT_KNN_MAX_NEIGHBORS` | 1 | runner.rs:212 `1` |
| `SUPPORT_KNN_MAX_LOGICAL_PEAK_BYTES` | 2147483648 | runner.rs:213 `2 * 1024 * 1024 * 1024` |
| `SUPPORT_PREPARATION_COVERAGE_SAMPLES_MAX_VIRTUAL_CELLS` | 2000000 | runner.rs:231 `2_000_000` |
| `SUPPORT_PREPARATION_COVERAGE_SCORE_MATRIX_MAX_ENTRIES` | 65536 | runner.rs:232 `65_536` |
| `SUPPORT_CONSTRUCTION_MAX_CONTACTS` | 1000000 | runner.rs:239 `1_000_000` |
| `SUPPORT_CONSTRUCTION_MAX_LOAD_ROWS` | 10000000 | runner.rs:240 `10_000_000` |
| `SUPPORT_CONSTRUCTION_MAX_WORK_ITEMS` | 4000000 | runner.rs:241 `4_000_000` |
| `SUPPORT_CONSTRUCTION_MAX_SCRATCH_BYTES` | 536870912 | runner.rs:242 `512 * 1024 * 1024` |
| `SUPPORT_BRACE_PENETRATION_MAX_ROWS` | 100000 | runner.rs:246 `100_000` |
| `SUPPORT_BRACE_PENETRATION_MAX_TOTAL_WORK` | 1000000000 | runner.rs:247 |
| `SUPPORT_BRACE_PENETRATION_MAX_TOTAL_HITS` | 1000000000 | runner.rs:248 |
| `SUPPORT_BRACE_PENETRATION_MAX_TOTAL_SAMPLES` | 10000000000 | runner.rs:249 |
| `SUPPORT_BRACE_PENETRATION_MAX_TOTAL_INTERVALS` | 1000000000 | runner.rs:250 |
| `SUPPORT_BRACE_PENETRATION_MAX_SCRATCH_BYTES` | 100000000000 | runner.rs:251 |
| `SUPPORT_RAFT_MAX_CANDIDATES` | 100000 | runner.rs:253 |
| `SUPPORT_RAFT_MAX_ROOTS` | 100000 | runner.rs:253 |
| `SUPPORT_RAFT_MAX_PRIMITIVES` | 200000 | runner.rs:253 |
| `SUPPORT_RAFT_MAX_BYTES` | 134217728 | runner.rs:253 `128 * 1024 * 1024` |
| `SUPPORT_PREPARATION_MAX_PRIMITIVES` | 200000 | runner.rs:255 `200_000` |
| `SUPPORT_PREPARATION_MAX_PRIMITIVE_BYTES` | 134217728 | runner.rs:256 `128 * 1024 * 1024` |
| `SUPPORT_PERCEPTION_MAX_PLANES` | 20000 | perception.rs:54 `20_000` |
| `SUPPORT_PERCEPTION_MAX_TRIANGLE_PLANE_TESTS` | 100000000 | perception.rs:55 |
| `SUPPORT_PERCEPTION_MAX_SLICE_INTERSECTIONS` | 100000000 | perception.rs:56 |
| `SUPPORT_PERCEPTION_MAX_SLICE_OUTPUT_BYTES` | 2147483648 | perception.rs:57 |
| `SUPPORT_PERCEPTION_MAX_LOOP_VERTICES` | 100000000 | perception.rs:58 |
| `SUPPORT_PERCEPTION_MAX_MINIMA_VERTICES` | 20000000 | perception.rs:59 |
| `SUPPORT_PERCEPTION_MAX_ISLAND_NEIGHBOR_PAIRS` | 100000000 | perception.rs:61 |
| `SUPPORT_PERCEPTION_MAX_ISLAND_NEIGHBOR_BYTES` | 2147483648 | perception.rs:62 |
| `SUPPORT_PERCEPTION_MAX_OVERLAP_PAIRS` | 100000000 | perception.rs:63 |
| `SUPPORT_PERCEPTION_MAX_OVERLAP_BYTES` | 2147483648 | perception.rs:64 |
| `SUPPORT_PERCEPTION_MAX_OVERLAP_VERTEX_TESTS` | 100000000 | perception.rs:65 |
| `SUPPORT_PERCEPTION_MAX_OVERLAP_EDGE_TESTS` | 100000000 | perception.rs:66 |

No test relates any of these pairs. `column_pitch_mm` / `surface_grid_pitch_mm`
(`PositiveFinite::new(1.0)`, runner.rs:195) and
`CandidatePayloadConfig::current(8, 1_000_000, 3_000_000, 256 MiB, 65_536)`
(runner.rs:223-227) have no registry entry at all.

## E. Registry entries with no production reader

130 of the 273 `Placeholder` constants and 9 of the 33 evidence-grade constants
have no call anywhere under a crate `src/` directory (measured by searching for
`.<accessor>()` across all 110 production source files; the count is a lower
bound because in-file `#[cfg(test)]` modules are included on the "read" side).

The list is in `/private/tmp/claude-501/.../scratchpad/unread.txt`. The
consequential ones are covered in sections A, C and D above.

## F. Units-trap sites (doctrine 5)

| Site | Problem |
|---|---|
| `plinth-support/src/raft.rs:335` | `start_radius_mm: pad_diameter` — `CHITUBOX_PAD_DIAMETER_MM` (2.2 mm **diameter**) is stored in a field named `_radius_mm`; `end_radius_mm: raft_height` stores a height in the same struct. The halving happens two crates away at `plinth-emit/src/lib.rs:875` (`diameter * 0.5`). |
| `plinth-support/src/member_sizing.rs:610` | `frustum_volume_ml` divides an mm³ volume by `MILLIMETRES_PER_METRE` (a length conversion). Numerically right, dimensionally wrong. `plinth-metrics/src/supports.rs:31` names the same quantity correctly (`CUBIC_MILLIMETRES_PER_MILLILITRE`); `plinth-support/src/cco.rs:37` uses a third name (`MILLIMETRES_PER_MILLILITRE`). |
| `plinth-support/src/perception.rs:7932` | mm→m conversion written as a bare `/ 1.0e3` beside a file that defines named conversions. |
| Registry (good practice) | `TIP_DIAMETER_MIN_ALLOWED_MM` / `TIP_RADIUS_MIN_ALLOWED_MM` and `TIP_RADIUS_MAX_GROWN_SCAR_MM` all carry explicit "UNITS TRAP" notes and state both forms. This is the standard the raft/volume sites fail to meet. |

## G. Vocabulary (doctrine 6): "floor" used for "smallest allowed value"

The owner's ruling of 2026-08-13 is recorded verbatim inside
`HOLLOW_WALL_MIN_MM`'s own note (placeholders.rs:1202): *"deliberately MIN, never
'floor' — the owner ruled … that 'floor' already means the bottom surface of a
printed part and must never also mean a smallest allowed value."* The ruling was
applied to that one constant and to nothing else.

Live API and registry names still carrying the banned sense:

- `ContactPolicy::floor_depth_mm` (plinth-support/src/contact.rs:210), documented
  as "the floor seat depth", set at brace.rs:95, rewire.rs:1097, cco.rs:4993,
  initial.rs:715 — a *seat depth*, the one context where "floor" also has its
  legitimate meaning.
- `SUPPORT_KNN_MAX_WORK_FLOOR`, `SUPPORT_PARITY_GRID_MAX_BYTES_FLOOR`,
  `SPACING_FLOOR_MM` (registry ids), and the `*_floor` accessors on
  `SupportResourceCapPolicy` (constants.rs:210, 218, 231, 243).
- `PRINT_FLOOR_CHORD_MM` and the `ScarAreaCapPolicy::print_floor_chord_mm`
  accessor (constants.rs:122).
- `patch_area_floor_mm2` on `CoverageMeasurementRequest`.
- `FLOORED_RUNG_INSET_MM` (plinth-support/src/census.rs:25).
- Placeholder notes: "Model-rung length floor", "Bifurcation split-piece length
  floor", "Input-face floor", "Integrity floor", "not a physics floor" (×3).

Roughly 200 occurrences across ~25 files (`grep -i floor`, excluding `f64::floor`
and genuine plate/bottom references).
