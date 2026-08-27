# Plinth-3D Test Suite Census

**Survey Date:** 2026-08-26  
**Scope:** All test files under `crates/*/tests/`, `rust/*/tests/`, and `#[cfg(test)]` modules in `src/`

## Summary Counts

**Test Files & Functions:**
- Integration tests: 61 files
- Unit test modules: 59 files (within src/)
- Compile-fail tests: 2 files  
- Golden/snapshot tests: 8 files
- **Total test files:** 130
- **Total test functions:** 1,585
- **Ignored tests:** 3

**Dependencies:**
- Test files with external/absolute path dependencies: 38

**Coverage:**
- Production modules >500 lines: 67
- Modules >500 lines WITH unit tests: 52
- Modules >500 lines WITHOUT unit tests: 15

---

## Coverage Gap: Production Modules >500 Lines Without Unit Tests

| Module | Lines | Type |
|--------|-------|------|
| crates/plinth-core/src/constants.rs | 1338 | configuration/constants |
| crates/plinth-core/src/constants/evidence.rs | 768 | constants submodule |
| crates/plinth-core/src/constants/placeholders.rs | 1547 | constants submodule |
| crates/plinth-core/src/cost.rs | 502 | cost calculation |
| crates/plinth-core/src/provenance.rs | 1432 | data provenance tracking |
| crates/plinth-layers/src/lib.rs | 747 | crate entry point |
| crates/plinth-mesh/src/fixtures.rs | 996 | mesh fixture data |
| crates/plinth-mesh/src/topology.rs | 1152 | mesh topology |
| crates/plinth-metrics/src/read.rs | 685 | metrics I/O |
| crates/plinth-metrics/src/supports.rs | 512 | support metrics |
| crates/plinth-metrics/src/topology.rs | 805 | topology metrics |
| crates/plinth-orient/src/search.rs | 1207 | orientation search |
| rust/chitubox_io/src/project.rs | 868 | printer file I/O |
| rust/geom_kernel/src/point_index.rs | 884 | spatial indexing |
| rust/geom_kernel/src/queries.rs | 3168 | geometry query core |

---

## Test Classification by Type

**Integration Tests (61 files):**
- Entry point: `crates/*/tests/*.rs`, `rust/*/tests/*.rs`
- Target: Full crate functionality via its public API
- Examples: `plinth-pipeline/tests/a4_production_door.rs`, `geom_kernel/tests/geometry.rs`

**Unit Tests (59 modules):**
- Entry point: `#[cfg(test)]` blocks within `src/*.rs` and `src/**/*.rs`
- Target: Individual module or type; define behavior at function/struct level
- Examples: `plinth-support/src/coverage.rs`, `plinth-hollow/src/flow.rs`

**Compile-fail Tests (2 files):**
- Entry point: `crates/*/tests/compile_fail.rs` (uses trybuild/compiletest)
- Target: API invariants; verify invalid code is rejected at compile time
- Locations: `plinth-core/tests/compile_fail.rs`, `plinth-support/tests/compile_fail.rs`

**Golden/Snapshot Tests (8 files):**
- Entry point: `crates/*/tests/*.rs` with insta or assertion snapshots
- Target: Output stability; verify output format against frozen baseline
- Examples: `chitubox_io/tests/frozen_baseline.rs`, `orient/tests/orientation_rederivation.rs`

---

## External & Absolute Path Dependencies

**Test Files Referencing Fixture Data (38 files):**

Tests that load from disk (via `Path::new()`, `File::open()`, `include_str!()`, etc.):

- `crates/plinth-cli/tests/process.rs` — corpus/baseline (EXISTS)
- `crates/plinth-core/tests/constants_evidence.rs` — corpus_study (MISSING)
- `crates/plinth-core/tests/core_contracts.rs` — fixtures/calibration.json (CHECK NEEDED)
- `crates/plinth-decimate/tests/corpus_budgets.rs` — corpus/ (EXISTS)
- `crates/plinth-decimate/tests/local_minis_repair.rs` — corpus/ (EXISTS)
- `crates/plinth-mesh/tests/fixtures.rs` — corpus/ (EXISTS)
- `crates/plinth-mesh/tests/repair.rs` — corpus/ (EXISTS)
- `crates/plinth-mesh/tests/strict_audit.rs` — corpus/ (EXISTS)
- `crates/plinth-mesh/tests/topology_triage.rs` — corpus/ (EXISTS)
- `crates/plinth-metrics/tests/disk_reader.rs` — corpus/*, exports/ (EXISTS)
- `crates/plinth-orient/tests/orient_thread_sweep.rs` — corpus/ (EXISTS)
- `crates/plinth-orient/tests/orientation_dense_sweep.rs` — corpus/ (EXISTS)
- `crates/plinth-orient/tests/orientation_rederivation.rs` — corpus/ (EXISTS)
- `crates/plinth-orient/tests/orientation_scoring.rs` — corpus/ (EXISTS)
- `crates/plinth-pipeline/tests/*.rs` (all 6 files) — corpus/, exports/ (EXISTS)
- `crates/plinth-support/tests/*.rs` (multiple) — corpus/, exports/ (EXISTS)
- `crates/plinth-verify/tests/*.rs` (all 3 files) — corpus/, exports/ (EXISTS)
- `rust/chitubox_io/tests/frozen_baseline.rs` — corpus/, exports/ (EXISTS)

**Absolute Path References (found in tests but inspect before use):**
- `crates/plinth-core/tests/core_contracts.rs` — References `/measurements/*`, `/scar/*`, etc. (JSON path literals for data schema validation; NOT filesystem paths)

---

## Notes on Coverage Gaps

1. **Constants Modules:** `constants.rs`, `evidence.rs`, `placeholders.rs` have no unit tests. These are tested indirectly through integration tests that read constants, but direct property/validation tests are missing.

2. **Large Modules Without Unit Tests:** `geom_kernel/queries.rs` (3,168 lines) is the largest untested module; it defines core geometry queries. Relies on integration tests from dependent crates.

3. **Topology & Mesh Modules:** `topology.rs` (mesh and metrics) are large data-structure modules without unit tests; test coverage is via integration tests.

4. **I/O Modules:** `metrics/read.rs`, `chitubox_io/project.rs` handle file I/O; tests that validate file format contracts are integration tests, not unit tests.

5. **Search/Optimization:** `orient/search.rs` (1,207 lines) implements orientation search with no unit tests; tested via `orientation_scoring.rs` and sweeps that exercise the full search.

6. **External Dependencies:** Most corpus/ and exports/ dependencies exist and are checked into the repo. One missing reference: `corpus_study` in `constants_evidence.rs`—appears to be a placeholder name, not an actual path.

---

## Test Quality Observations

- **Ignored tests:** Only 3 tests marked `#[ignore]` across the suite (minimal skips).
- **Compile-fail tests:** Properly isolate API invariants using trybuild; 40+ private-field and lifetime constraints checked.
- **Golden/snapshot tests:** Used for format and output stability (frozen_baseline, orientation rederivation).
- **Fixture Dependencies:** 38 test files depend on external data; all corpus/ and exports/ directories present locally.
