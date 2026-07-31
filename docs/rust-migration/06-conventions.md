# 06 — Conventions

Standing rules for every line of code you write in this migration. These are not
style preferences; several of them exist because a specific bug shipped. Read this
before your first commit. Start at README.md if you have not.

---

## 1. Batch-first is law

**Gather → one batched call → scatter.** A per-item call in a hot path is a defect,
not a style choice. This applies in every language, and it is the single habit the
Python elimination exists to protect against (see README.md §1).

Design every geometry, cost, and I/O API to take a slice and return a slice. The
existing kernel already does this and is the model to copy — `rust/geom_kernel/src/queries.rs`
exposes `query(&self, pts: &[V3], upper_bound: f64)`, `project(&self, pts: &[V3])`,
`inside(&self, pts: &[V3])`, `penetration_intervals_batch(...)`. None of them take a
single point.

**Bad** — one BVH descent per point, no parallelism, cache-hostile:

```rust
let mut dists = Vec::with_capacity(seeds.len());
for p in &seeds {
    let (d, _f) = kernel.query(std::slice::from_ref(p), f64::INFINITY);
    dists.push(d[0]);
}
```

**Good** — one call, one traversal setup, parallelism and determinism handled inside:

```rust
let (dists, faces) = kernel.query(&seeds, f64::INFINITY);
```

The rule generalizes past geometry. Score all orientation candidates in one batched
pass over the `FaceTable`, not a loop of single-candidate scorers. Size all support
arms from one array of carried areas, not one call per arm. Write one artifact at
the end of a run, not incremental rewrites (see §7).

If a batched form is genuinely impossible for some operation, say so in a comment at
the call site with the reason. An unexplained scalar loop over mesh elements will be
treated as a bug in review.

---

## 2. Floating point: f64 everywhere

All geometry is `f64`. No `f32` anywhere in the geometric data path, including
intermediate buffers, serialization of internal state, and test fixtures. A float32
trimesh wrapper caused real bugs in the Python system: an f32 round-trip fragmented
vertex adjacency, which flipped topology cut flags, which hard-failed coverage.

Every `f64` reachable from a `DefectReport` is the transparent `Finite` wrapper: creation
rejects `NaN` and both infinities, rather than allowing a late report serializer to
discover them. Preserve signed zero everywhere; canonicalize `-0.0` only at the mesh
fingerprint boundary. Measures used for reporting, including `MetricValue`,
`Defect.measure`, and termination before/after measures, are signed finite values —
they are not silently clamped to a nonnegative type.

Corollaries:

- The boolean union kernel must be f64 (manifold-style). See 02-target-architecture.md.
- Mesh fingerprints canonicalize `-0.0 → 0.0` before hashing and reject non-finite
  coordinates as a `Defect`. Never weld or hash raw f64 bit patterns — that is the
  documented Python hazard chain above.
- The topology digest hashes topology arrays only. It carries a schema version and
  is explicitly not byte-compatible with the Python digest, which baked NumPy
  allocator arithmetic into a wire hash.
- `f32` is permissible only where an external format demands it — the `.chitubox`
  record fields already defined in `rust/chitubox_io`. Convert at the boundary, never
  inward.
- Physical-cost dimensions have explicit domains: height and time are finite nonnegative
  quantities; resin and scar are finite nonnegative quantities; maximum effective X
  and lateral spread are finite nonnegative quantities. Do not use these domain rules
  to alter signed diagnostic measures.

---

## 3. Parallelism (rayon)

Follow the pattern already established in `rust/geom_kernel/src/par.rs`:

- **Never use rayon's global pool.** Use explicit per-thread-count pools via
  `par::run(|| ...)`. A one-shot global pool makes thread-sweep tests inert — they
  silently keep testing the first thread count they ever saw.
- **The only sanctioned pattern is index-parallel map-collect.** Every output lands
  in the slot of its own index, so results are bit-identical for any thread count.
- **No float reduce / fold / sum across parallel work items, ever.** Float addition
  is not associative; a parallel reduction makes results thread-count dependent. If
  you need a sum, collect per-index results and sum them in a deterministic serial
  order.
- **Thread count comes from exactly one workspace-wide environment variable,
  `PLINTH_THREADS`**, with an all-cores default, read once in `plinth-core` and
  threaded to every crate's `par::run` pool. `geom_kernel` keeps `GEOM_KERNEL_THREADS`
  as its own crate-local variable for its existing standalone suites, but when it is
  driven from `plinth-*`, `PLINTH_THREADS` is what sets the pool. **Do not give each
  new crate its own variable name.** A per-crate variable makes the determinism sweep
  in 04-migration-plan.md M8 pass vacuously: setting one crate's variable leaves every
  other crate's pool at its default, so the 1/4/16-thread digests match for the
  crates that were never varied.
- **Every parallel path needs a thread-sweep test**: run it at 1, 2, and N threads
  and assert the outputs are identical, not merely close.

GPU (wgpu) is roadmap, not scope. Keep the batched API shapes such that a GPU backend
could implement the same slice-in/slice-out signature later.

---

## 4. Error handling

- **Library crates:** `thiserror`-style concrete error enums per crate. No
  `Box<dyn Error>` in a public signature. Follow `rust/chitubox_io/src/error.rs`.
  Public `Error`/`Display` enums use semantic variants; no `Box<dyn Error>` or
  string-only error channel. Fieldless semantic errors such as `LoadSharingError`
  remain valid.
- **Binary / CLI:** `anyhow` at the top level for context chaining is fine.
- **`unwrap` / `expect`:** forbidden on anything that can fail from input data.
  Permitted on genuine invariants (a poisoned mutex, a `OnceLock` you just set), and
  then the `expect` message must state the invariant, not the symptom.
- **`panic!` is not an acceptance mechanism.** A model that cannot be supported is a
  `Defect` in the `DefectReport`, not a panic. Panics are for programmer errors only.
- **No wall-clock budgets, anywhere.** `rewire_time_cap_s = 30.0` and every other
  timing-based cap are banned outright, with no legacy wall-clock mode retained. Caps
  are deterministic work-unit budgets (iterations, candidate evaluations, node
  expansions). A wall-clock cap makes the run itself non-reproducible, which makes the
  baseline non-reproducible.
- **Cap exhaustion names which defect failed to improve.** Never a bare exit code,
  never a bare boolean. The termination value carries the `ViolationKind`, the fixer
  that was responding, the site, the attempt count, and the before/after measure — and
  it also injects a `RepairCapExhausted` defect so it flows through the single
  acceptance authority. See 02-target-architecture.md.
- `Termination` has exactly three exhaustive outcomes: `Converged`,
  `CapExhausted { kind, fixer, site, attempts, before, after, note }`, and
  `PoolExhausted { kind, site }`. `CapExhausted` requires a matching **blocking**
  `RepairCapExhausted` defect at the same site, and its original `kind` must not itself
  be `RepairCapExhausted`. `PoolExhausted` requires a matching **blocking** defect of
  the same named kind and site. No fourth "success with warning" path exists.
- `LoadSharing` is a tagged physical model whose `Additive` cap is finite and at least
  `1`. `FixerId` is likewise a passive core identifier. `plinth-core` owns both types
  and their validation; `plinth-support::repair` later re-exports `FixerId` and owns
  responder/fixer behavior, while `plinth-support::load` later re-exports
  `LoadSharing` and owns `ArmGroup`/`effective_x`. Neither duplicates core tags,
  validation rules, or identity logic.

### Report construction

`DefectReport` fields are private; construction is fallible and validating. Report provenance
is a deterministic ordered map: `DecisionId` → `ConstantId` → `ProvenanceRef`;
`Measured` provenance includes its measurement date. Empty provenance is legitimate
only before any accept decision runs. At the constants-registry checkpoint,
decision-scoped automatic recording is enforced for every automatic decision, rather
than added in a best-effort post-processing pass. The report is accepted only when it
contains no blocking defects. Categorical measurements retain the existing explicit
`"err": null` serialization contract.

`OptimizationSummary` is private, fallible, and custom-deserialized: its Pareto set is
nonempty; `candidates_examined > 0` and is at least the Pareto length; orientation IDs
are unique; and the selected `(orientation, cost)` is an exact member of that set.
`LayerPlan` wires only `z0_mm` and `heights_mm`: `z0_mm` is signed finite, every height is
strictly positive finite, and an empty plan is allowed. It recomputes cumulative
heights and rejects overflow or any unknown legacy cumulative field. `Stage` uses an
associated `Context: ?Sized` and returns `Result<Output, DefectReport>`.

Construction also rejects a finite positive height if its addition rounds away and
does not strictly advance the cumulative boundary. `z_of(i)` returns the lower
boundary only for an existing layer (`i < count`), so it returns `None` for an empty
plan or `i == count`; `+0.0` and `-0.0` comparisons are numeric-coherent and never
panic.

### CLI contract

One `DefectReport` JSON per model on stdout. The **exit code is a projection of the
`DefectReport`**, derived from it, never computed independently. The old Python
contract (`scripts/tools/prep_one.py` exit codes 0–5, JSONL stdout) is retired along
with the tests that pin it; do not reproduce it.

---

## 5. Constants carry typed provenance

Every physical or tuning constant is wrapped in a type that records where its value
came from. This is a compile-time mechanism, not a documentation convention.

| Wrapper | Meaning | On the accept path? |
|---|---|---|
| `Measured<T>` | Measured on a calibration plate. Carries the plate id, required measurement date (carried into `ProvenanceRef`), and `err: Option<T>`; a categorical measurement uses `None`, serialized explicitly as `"err": null` beside its tagged category. | Yes |
| `Derived<T>` | Computed from `Measured` values by a stated relation. | Yes |
| `Placeholder<T>` | A guess. Search/tuning weights live here. | **No — will not compile** |
| `Pending<T>` | Awaiting a measurement. Carries **both** a conservative and an optimistic value. `.resolve()` returns the conservative branch wrapped as a `Derived<T>` — an `AcceptSafe` value carrying the pending provenance, never a bare `T`. | Only via explicit `.resolve()` |

**The mechanism:** an `AcceptSafe` trait implemented by `Measured` and `Derived` and
**not** implemented by `Placeholder`. Everything on the acceptance path is generic
over `AcceptSafe`, so a reachable `Placeholder` is a trait-resolution failure at
compile time — unconditionally, with no dependence on const-eval or a runtime check.

**Prove the mechanism works.** A `trybuild` compile-fail test asserts that a program
putting a `Placeholder` on the accept path fails to build. A mechanism without a test
of the mechanism is a promise, not a guarantee.

**Provenance cannot rot.** A golden test renders the constant registry to
`docs/rust-migration/constants.md`; the test fails if the file is out of date.

**Search weights are `Placeholder` by construction and that is correct.** Orientation
scalarization weights select a candidate; they never accept one. Acceptance is the
`DefectReport`, never a score. See 03-physics-and-constants.md for every constant and
its grade. The two plate-6 constants (`LOAD_SHARING`, `X_STAR_CYLINDER`) were the worked
example of `Pending` until the plate scored on **2026-07-31**; both are `Measured` now,
and the fact that the switch cost one constant each is the argument for keeping the
wrapper. `Pending` stays as the mechanism for the next unmeasured constant.

---

## 6. Testing standards

Three layers, all required.

**Unit tests** — per crate, hermetic, fast. Standard `#[cfg(test)]`. Every parallel
path additionally gets the thread-sweep test from §3.

**Parity tests** — the milestone gate. Rust emits files; the frozen Python baseline
artifacts in `corpus/baseline/` are the reference; `plinth score` computes the same
objective metrics on both and requires Rust to be **as-good-or-better on every
metric**. Never byte-diff the two sides, and **never bridge via PyO3 to compare** —
a bridge lets one shared bug hide on both sides. The one place byte-level identity
is a real contract is the `.chitubox` record structure, gated by the existing corpus
round-trip tests (05-chitubox-and-corpus.md).

**Audit-the-file-as-written** — non-negotiable. The audit re-opens the emitted
artifact from disk, parses it fresh, and fails on boundary edges, negative shells,
or inconsistent winding. It must not run on an in-memory mesh, and there must be no
repair pass between the write and the audit. This project once validated an
in-memory mesh while shipping a different one; that failure mode is what this rule
exists to make impossible. When the audit fires, the fix is always in the producer
(`plinth-solid`), never in the artifact.

Additional required fixtures:

- Every `ViolationKind → Repair(fixer)` mapping has a fixture where the fixer
  measurably reduces the measure.
- Every `ViolationKind → Terminal` mapping has a fixture proving in-loop
  unrepairability.
- A kind with neither is a failing test. This is the guard against `Terminal` being
  used as an escape from writing a fixer.
- `plinth-metrics` is validated against the frozen baseline **and** against
  deliberately corrupt artifacts before it is ever allowed to judge Rust output. A
  harness written after the thing it measures always measures what that thing does.
- **Cache-disabled equality is CI-mandatory:** a run with the `plinth-pipeline`
  cache off must produce byte-identical reports to a cached run. **The cache-off
  mechanism is the flag `plinth prepare --no-cache`** — one spelling, defined here,
  used verbatim in M8's acceptance commands. There is no environment-variable form
  (in particular there is no `PLINTH_CACHE`); a second way to disable the cache is a
  second thing that can silently stop working.

---

## 7. Artifacts, scratch files, and the working tree

- **No stray artifacts at the repo top level.** Outputs go to a cache/temp directory,
  never next to the source. `.gitignore` already refuses `docs/artifacts/`,
  `/artifacts/`, and `/target/` — do not work around it. Run outputs record the
  result of running one version of the code; they are not a revision to the code.
- **Write the artifact once, at the end of a run.** Stages return owned values; the
  emitted file is produced from the final owned value. Mid-run rewriting of an
  artifact must be unrepresentable in the type system.
- **The corpus and the baseline are committed** — `corpus/models/` and
  `corpus/baseline/`. Large binaries are acceptable here; the repo is local-only git,
  and without them the gates are unexecutable. Do not regenerate baselines casually;
  the freeze happens once, per 04-migration-plan.md.
- **Never write outside the repository.** Exactly one carve-out exists, and it is
  named: M9's clean-checkout proof clones the repo into a throwaway directory under
  `$TMPDIR` and builds there (04-migration-plan.md M9). It writes no artifact back,
  and the clone is deleted after the run. Nothing else may write outside the repo.

---

## 8. Absolutely no new Python

Not in `scripts/`, not in a temp directory, not "just to check something", not a
throwaway `python -c` in a shell pipeline that ends up in a script or a doc.

**Exactly one exception exists, and it is enumerated in 04-migration-plan.md M0:** the
**two** one-off `python -c` invocations run there against the frozen oracle — the
`resolve_stl()` corpus probe (M0's owner prerequisite) and the synthetic-fixture STL
export (M0 work item 6). Both are run once from a shell, neither may be committed as a
`.py` file, and no third one is sanctioned. They exist because the five synthetic
fixtures (`mesh_io.make_box` and friends) are reachable only from Python until M2 lands
`plinth-mesh::fixtures`. Everything else in this section stands.

The rationale is in README.md §1 and it is about agent productivity, not runtime
speed — so "it's only a debug script" is not a mitigation. It is exactly the case the
rule targets.

**Use instead:**

| Instead of a Python script | Use |
|---|---|
| A one-off geometry probe or dump | A `cargo example` — `rust/chitubox_io/examples/` is the existing precedent. Run with `cargo run -p <crate> --example <name>`. |
| Ad-hoc numeric checks | A `#[test]` or a `#[bench]`-style example. It then stays green forever instead of rotting. |
| Shell glue, file wrangling, JSON poking | POSIX shell plus `jq`. Never `python -c "import json"`. |
| A data-munging notebook | A small Rust binary in the relevant crate. |

The existing Python remains runnable as the frozen oracle until the final milestone.
**Do not edit it.** Reading it is how you learn the algorithms; changing it
invalidates the baseline you are being measured against. If you find a bug in the
Python, record it in the Rust port's tests as a fix-don't-reproduce item — do not
patch the oracle.

TypeScript/bun is permitted only with a written, owner-approved blocker; the
procedure is in README.md §5. Python is never an escape hatch.

---

## 9. Git discipline

- **Commit per milestone on `master`.** The repo is local-only git. Work-in-progress
  commits within a milestone are fine; a milestone boundary must be a commit.
- **Forbidden, always, no exceptions** — these silently destroy uncommitted work:
  - `git checkout --` (checkout with a double-dash path form)
  - `git restore`
  - `git reset --hard`
  - `git clean`
  - Undo your own edits by editing. If you think you need one of these commands, stop
    and ask.
- **Never force-push.**
- **Never rewrite published history.**
- **Python deletions happen only in the final milestone**, after the parity gates
  pass. The deletion set is enumerated in README.md §5 and 04-migration-plan.md.
- Commit messages state the milestone, the gate result, and the **human checkpoint** —
  the specific thing the owner should eyeball for the subjective half of "as-good-or-better".
- Interactive git flags (`-i`) are unavailable in this environment.

---

## 10. Rust project hygiene

- **Edition 2021**; pin new crates to the same toolchain channel as the existing
  crates (`rust/geom_kernel/rust-toolchain.toml`, currently `1.97.1`).
- **Package names are kebab-case** (`plinth-core`, `plinth-mesh`, …). Existing crates
  keep their current paths under `rust/` — no path churn, no renames.
- New crates are added to the workspace `members` list in the top-level `Cargo.toml`.
- `cargo fmt` clean and `cargo clippy --workspace -- -D warnings` clean before every
  commit.
- **Rule of two:** do not introduce an abstraction (trait, generic parameter, new
  crate) until there are two real implementations or two real consumers. The one
  deliberate exception is `trait SolidBoolean`, which exists specifically so the C++
  FFI backend can be replaced (02-target-architecture.md).
- Prefer an exhaustive `enum` over a trait when the variants are a small fixed set
  decided by a physical fact — `LoadSharing` is the canonical case.
- **`#[non_exhaustive]` is forbidden on `ViolationKind`.** It would force `_` match
  arms in other crates, and the responder table lives in `plinth-support`, a different
  crate from `plinth-core`. That would structurally defeat the commitment that a gate
  without a responding fixer is a compile error.

---

## 11. The commitments these rules serve

Every rule above traces to one of the non-negotiable architectural commitments. They
are stated in full in 02-target-architecture.md; the short form:

1. One `DefectReport` type is the sole acceptance authority.
2. Constants carry typed provenance; the build **fails** if a `Placeholder` is
   reachable from the accept path.
3. Violation kinds are an exhaustively-matched enum, so a gate that can fail without
   a fixer that can respond is a compile error.
4. Cap exhaustion names **which** defect failed to improve.
5. Layer height is a per-layer array in the data model from day one (ships constant
   0.050 mm until calibrated).
6. Float64 geometry throughout.
7. Emitted solids are boolean-unioned, and an audit re-reads the artifact from disk
   and fails on boundary edges, negative shells, or inconsistent winding.

If a rule here ever seems to conflict with a commitment, the commitment wins and the
conflict is a bug in this document — report it.
