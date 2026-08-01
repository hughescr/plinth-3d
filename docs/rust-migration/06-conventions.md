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
discover them. Preserve signed zero everywhere. Generic numeric content digests hash
finite values by their raw IEEE bits, so `-0.0` remains distinct from `0.0`; only a
future mesh fingerprint canonicalizes `-0.0`. Measures used for reporting, including `MetricValue`,
`Defect.measure`, and termination before/after measures, are signed finite values —
they are not silently clamped to a nonnegative type.

Corollaries:

- The boolean union kernel must be f64 (manifold-style). See 02-target-architecture.md.
- A future mesh fingerprint canonicalizes `-0.0 → 0.0` before hashing and rejects
  non-finite coordinates as a `Defect`. This mesh-specific rule does not apply to
  generic numeric content digests, which preserve raw signed-zero bits. Never weld
  vertices by raw f64 bit patterns — that is the documented Python hazard chain above.
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
  threaded to every crate's `par::run` pool. Frozen `geom_kernel` has the distinct legacy
  `GEOM_KERNEL_THREADS` control for its existing standalone suites; it is excluded/no-port
  once the workspace migration is complete. When driven from `plinth-*`,
  `PLINTH_THREADS` is what sets the pool. **Do not give each
  new crate its own variable name.** A per-crate variable makes the determinism sweep
  in 04-migration-plan.md M8 pass vacuously: setting one crate's variable leaves every
  other crate's pool at its default, so the 1/4/16-thread digests match for the
  crates that were never varied.
- **Every parallel path needs a thread-sweep test**: run it at 1, 2, and N threads
  and assert the outputs are identical, not merely close.

`PLINTH_THREADS` is a mandatory runtime contract, not a physical/tuning registry
constant; the M0 source ledger records it only as an excluded/nonregistry disposition.
`GEOM_KERNEL_THREADS` is a distinct frozen-Python/kernel scheduling control with its
own excluded/no-port row and M8 determinism proof. They are not aliases.

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
  `1`; its tagged wire representation denies unknown fields. `FixerId` is likewise a
  passive core identifier. `plinth-core` owns both types and their validation;
  `plinth-support::repair` later re-exports `FixerId` and owns responder/fixer behavior,
  while `plinth-support::load` later re-exports `LoadSharing` and owns
  `ArmGroup`/`effective_x`. Neither duplicates core tags, validation rules, or identity
  logic.

### Report construction

`DefectReport` fields are private; construction is fallible and validating. Report provenance
is a deterministic ordered map: `DecisionId` → `ConstantId` → `ProvenanceRef`.
`ProvenanceManifest` construction and recording are fallible: repeated identical
records are idempotent, repeated decision batches merge their constants, and any
conflicting value for an existing decision/constant pair is an error. JSON
deserialization always rejects duplicate decision keys and duplicate constant keys
instead of accepting last-write-wins data.
`Measured` provenance includes its measurement date. Empty provenance is legitimate
only before any accept decision runs. At the constants-registry checkpoint,
decision-scoped automatic recording is enforced for every automatic decision, rather
than added in a best-effort post-processing pass. The report is accepted only when it
contains no blocking defects: `Termination::Converged` never overrides a blocking
defect. Categorical measurements retain the existing explicit `"err": null`
serialization contract.

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

`plinth-core::chord` uses a positive integer `MicronGranularity` and preserves the
Python oracle's executable operation order and IEEE ties-to-even rounding. Its
whole-population parity report canonicalizes every supplied member before comparing
ordered multisets; `matches()` is diagnostic only. It cannot become a second
acceptance authority: `DefectReport` remains the sole acceptance decision.

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
| `Measured<T>` | Evidence-backed. Carries a required tagged `EvidenceSource`, `err: Option<T>`, and note. The source carries an `EvidenceDate` of `Exact`, `OnOrBefore`, or `Unknown`. | Yes |
| `Derived<T>` | Computed from `Measured` values by a stated relation. | Yes |
| `Placeholder<T>` | A guess. Search/tuning weights live here. | **No — will not compile** |
| `Pending<T>` | Awaiting a measurement. Carries **both** a conservative and an optimistic value. `.resolve()` returns the conservative branch wrapped as a `Derived<T>` — an `AcceptSafe` value carrying the pending provenance, never a bare `T`. | Only via explicit `.resolve()` |

Those are exactly the four provenance wrappers. “Format”, “fixed”, “policy”, or
“owner-ratified” is a source or acceptance disposition, not another wrapper.

`EvidenceSource` is a strict tagged enum:

- `CalibrationPlate { plate, record, locator, date }`;
- `VendorDocument`, `InstalledHardware`, `CorpusStudy`, `ProjectRecord`, and
  `OwnerDecision`, each `{ id, record, locator, date }`.

Evidence IDs are non-empty and trimmed. `record` is a non-empty repo-relative path;
`locator` is a non-empty stable locator within it. A vendor source points to the
checked-in datasheet that contains URL and retrieval metadata, never directly to a
raw URL. `EvidenceDate` is tagged by `precision`: `Exact { date }`,
`OnOrBefore { date }`, or `Unknown`; its dates are real `YYYY-MM-DD` calendar dates,
and `Unknown` forbids a date field. Use generic `Unknown` only when the legacy value
is genuinely unsourced. Known official Saturn vendor evidence is never downgraded to
unknown.

`Measured<T>` accepts only payloads covered by the core-sealed measurement-payload
policy; downstream crates cannot invent new payload/error semantics. Approved payloads
include numeric scalars, `LoadSharing`, and validated `FiniteInterval`. An interval
stores finite `lower < upper` plus explicit `BoundKind::{Inclusive, Exclusive}` for
each endpoint. Numeric scalars permit either `Some(error)` or `None`; interval and
categorical measurements require `None`. The measured wire has exactly
`value`, `source`, `err`, and `note`. `source` and `err` are required, omission is
rejected, and `None` is emitted as explicit `"err": null`. Unknown or duplicate
fields are errors.

**The mechanism:** an `AcceptSafe` trait implemented by `Measured` and `Derived` and
**not** implemented by `Placeholder`. Everything on the acceptance path is generic
over `AcceptSafe`, so a reachable `Placeholder` is a trait-resolution failure at
compile time — unconditionally, with no dependence on const-eval or a runtime check.

**Prove the mechanism works.** A `trybuild` compile-fail test asserts that a program
putting a `Placeholder` on the accept path fails to build. A mechanism without a test
of the mechanism is a promise, not a guarantee.

**Provenance cannot rot.** Once the runtime registry exists, completeness is a
three-way check: the runtime registry, an independent hand-maintained expected-ID
set in a Rust test, and generated `docs/rust-migration/constants.md`. The current
registry is initialized as one fallible, batch-built
`LazyLock<Result<M0Constants, M0ConstantsError>>`: gather all declarations,
validate uniqueness and provenance in one batch, then publish the complete
registry or one error. Never register items one at a time into partially visible
global state.

`m0-constants-source-ledger.json` is a fourth, independent artifact. It is the
hand-maintained source-membership and provenance-disposition inventory, not a dump of
the runtime registry. It is never generated and never input to the golden test.
`03-physics-and-constants.md` remains the authority for values, shapes, units, and
grades; legacy values recorded in the ledger are nonnormative source snapshots.

**Search weights are `Placeholder` by construction and that is correct.** Orientation
scalarization weights select a candidate; they never accept one. Acceptance is the
`DefectReport`, never a score. See 03-physics-and-constants.md for every constant and
its grade. The two plate-6 constants (`LOAD_SHARING`, `X_STAR_CYLINDER`) were the worked
example of `Pending` until the plate scored on **2026-07-31**; both are `Measured` now,
and the fact that the switch cost one constant each is the argument for keeping the
wrapper. `Pending` stays as the mechanism for the next unmeasured constant.

**Decision-gate table lifecycle.** Do not create an empty or partial gate table in
M0. M1 creates it and backfills every guide decision (`D-*`), conscious override
(`O-*`), and historical owner ratification with `EvidenceSource` and
`EvidenceDate`. That backfill includes two distinct records: the evidence/date and
interval policy itself, and the supersession of the initial generic-Unknown Saturn
disposition by the known vendor record. It also explicitly preserves required
`err: Option<T>`, LoadSharing/FixerId ownership splits, cost/metric DTO-versus-behavior
splits, and Python-freeze-before-Rust-refreeze sequencing. After
creation it is dated and append-only; later sign-offs and waivers are added, not
rewritten.

## Decision gate table

This section closes the guide's former table gap: M1 creates it, rather than M0.
It is an append-only, machine-parseable authority record.  Its single Markdown
table has exactly ten columns; `-` means "not applicable", never an omitted
field.  `recorded` is when the entry was entered here, `evidence_date` is the
date carried by its cited authority, and `status` intentionally distinguishes a
guide commitment, a documented owner approval, a Sol compliance adjudication,
and a future owner signature.  A row is a waiver only when `kind` is `waiver`,
`authority` is exactly `owner:Craig Hughes`, and `status` is exactly `signed`.
The final column is five semicolon-separated fields in this fixed order:
`model;metric;scope;reason;expires`.  Procedural file authority is deliberately
not a cryptographic signature scheme: the trusted boundary is a checked-in,
reviewed repository checkout plus Craig's recorded sign-off.

| id | kind | recorded | authority | status | evidence_date | resolution | source | locator | waiver |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| D-1 | decision | 2026-08-01 | guide | guide-ratified | exact:2026-07-31 | Staged spine with search-shaped data model | docs/rust-migration/02-target-architecture.md | section-16-D-1 | - |
| D-2 | decision | 2026-08-01 | guide | guide-ratified | exact:2026-07-31 | Ratified crate layout and one independent metrics implementation | docs/rust-migration/02-target-architecture.md | section-16-D-2 | - |
| D-3 | decision | 2026-08-01 | guide | guide-ratified | exact:2026-07-31 | Typestate plus owned values and Validated newtype | docs/rust-migration/02-target-architecture.md | section-16-D-3 | - |
| D-4 | decision | 2026-08-01 | guide | guide-ratified | exact:2026-07-31 | Orientation-invariant FaceTable | docs/rust-migration/02-target-architecture.md | section-16-D-4 | - |
| D-5 | decision | 2026-08-01 | guide | guide-ratified | exact:2026-07-31 | Scar invariant surrogate then layout refine | docs/rust-migration/02-target-architecture.md | section-16-D-5 | - |
| D-6 | decision | 2026-08-01 | guide | guide-ratified | exact:2026-07-31 | Provenance wrappers AcceptSafe registry and compile-fail proof | docs/rust-migration/02-target-architecture.md | section-16-D-6 | - |
| D-7 | decision | 2026-08-01 | guide | guide-ratified | exact:2026-07-31 | Pending keeps conservative and optimistic branches | docs/rust-migration/02-target-architecture.md | section-16-D-7 | - |
| D-8 | decision | 2026-08-01 | guide | guide-ratified | exact:2026-07-31 | LoadSharing is an exhaustive tagged enum | docs/rust-migration/02-target-architecture.md | section-16-D-8 | - |
| D-9 | decision | 2026-08-01 | guide | guide-ratified | exact:2026-07-31 | Ratified ViolationKind union and RepairCapExhausted treatment | docs/rust-migration/02-target-architecture.md | section-16-D-9 | - |
| D-10 | decision | 2026-08-01 | guide | guide-ratified | exact:2026-07-31 | ViolationKind remains exhaustively matchable | docs/rust-migration/02-target-architecture.md | section-16-D-10 | - |
| D-11 | decision | 2026-08-01 | guide | guide-ratified | exact:2026-07-31 | Response is Repair or Terminal with fixture discipline | docs/rust-migration/02-target-architecture.md | section-16-D-11 | - |
| D-12 | decision | 2026-08-01 | guide | guide-ratified | exact:2026-07-31 | Three-tier orientation funnel with fixed counts | docs/rust-migration/02-target-architecture.md | section-16-D-12 | - |
| D-13 | decision | 2026-08-01 | guide | guide-ratified | exact:2026-07-31 | One-way coupling and measured escalation ladder | docs/rust-migration/02-target-architecture.md | section-16-D-13 | - |
| D-14 | decision | 2026-08-01 | guide | guide-ratified | exact:2026-07-31 | Solid FFI then metrics then thin vertical slice | docs/rust-migration/02-target-architecture.md | section-16-D-14 | - |
| D-15 | decision | 2026-08-01 | guide | guide-ratified | exact:2026-07-31 | Redesign orchestration and port geometric proofs as coded | docs/rust-migration/02-target-architecture.md | section-16-D-15 | - |
| D-16 | decision | 2026-08-01 | guide | guide-ratified | exact:2026-07-31 | transient.py maps to support sizing with immutable measured arrays | docs/rust-migration/02-target-architecture.md | section-16-D-16 | - |
| D-17 | decision | 2026-08-01 | guide | guide-ratified | exact:2026-07-31 | removal.py is opt-in diagnostic only | docs/rust-migration/02-target-architecture.md | section-16-D-17 | - |
| D-18 | decision | 2026-08-01 | guide | guide-ratified | exact:2026-07-31 | Mesh fingerprints canonicalize negative zero and reject nonfinite | docs/rust-migration/02-target-architecture.md | section-16-D-18 | - |
| D-19 | decision | 2026-08-01 | guide | guide-ratified | exact:2026-07-31 | Metrics excludes plinth-verify and geom_kernel | docs/rust-migration/02-target-architecture.md | section-16-D-19 | - |
| D-20 | decision | 2026-08-01 | guide | guide-ratified | exact:2026-07-31 | CLI exit projects DefectReport | docs/rust-migration/02-target-architecture.md | section-16-D-20 | - |
| D-21 | decision | 2026-08-01 | guide | guide-ratified | exact:2026-07-31 | Ignored source STLs and committed corpus metadata plus outputs | docs/rust-migration/02-target-architecture.md | section-16-D-21 | - |
| D-22 | decision | 2026-08-01 | guide | guide-ratified | exact:2026-07-31 | Delete Blender geometry sweep and refuse bad geometry | docs/rust-migration/02-target-architecture.md | section-16-D-22 | - |
| D-23 | decision | 2026-08-01 | guide | guide-ratified | exact:2026-07-31 | Pipeline cache and cost memoization with cache-off equality | docs/rust-migration/02-target-architecture.md | section-16-D-23 | - |
| D-24 | decision | 2026-08-01 | guide | guide-ratified | exact:2026-07-31 | Print time sums layer exposure and lift | docs/rust-migration/02-target-architecture.md | section-16-D-24 | - |
| D-25 | decision | 2026-08-01 | guide | guide-ratified | exact:2026-07-31 | Scalarization weights are Placeholder and not accept-safe | docs/rust-migration/02-target-architecture.md | section-16-D-25 | - |
| D-26 | decision | 2026-08-01 | guide | guide-ratified | exact:2026-07-31 | Concession supports deferred pending named owner decision | docs/rust-migration/02-target-architecture.md | section-16-D-26 | - |
| D-27 | decision | 2026-08-01 | guide | guide-ratified | exact:2026-07-31 | Filter dead Murray members in Rust | docs/rust-migration/02-target-architecture.md | section-16-D-27 | - |
| D-28 | decision | 2026-08-01 | guide | guide-ratified | exact:2026-07-31 | Ledger plus registry expected IDs and golden constants artifact | docs/rust-migration/02-target-architecture.md | section-16-D-28 | - |
| D-29 | decision | 2026-08-01 | owner:Craig Hughes | documented-owner-approval | exact:2026-07-31 | Tagged evidence dates finite intervals and explicit categorical null error | docs/rust-migration/02-target-architecture.md | section-16-D-29 | - |
| D-30 | decision | 2026-08-01 | owner:Craig Hughes | documented-owner-approval | exact:2026-07-31 | Saturn build volume and pixel pitch use official vendor record | docs/rust-migration/02-target-architecture.md | section-16-D-30 | - |
| O-1 | override | 2026-08-01 | owner:Craig Hughes | documented-owner-approval | exact:2026-07-31 | Bounded C++ Manifold 3.5.2 FFI exception | docs/rust-migration/02-target-architecture.md | section-16-O-1 | - |
| O-2 | override | 2026-08-01 | guide | guide-ratified | exact:2026-07-31 | Python byte parity for fingerprints is abandoned | docs/rust-migration/02-target-architecture.md | section-16-O-2 | - |
| O-3 | override | 2026-08-01 | guide | guide-ratified | exact:2026-07-31 | ExactSurface permanent-Python commitment is overridden | docs/rust-migration/02-target-architecture.md | section-16-O-3 | - |
| O-4 | override | 2026-08-01 | guide | guide-ratified | exact:2026-07-31 | Wall-clock budgets are banned | docs/rust-migration/02-target-architecture.md | section-16-O-4 | - |
| H-1 | historical | 2026-08-01 | owner:Craig Hughes | documented-owner-approval | exact:2026-07-31 | Measured err is Option and categorical payloads serialize err null | docs/rust-migration/03-physics-and-constants.md | section-4.2 | - |
| H-2 | historical | 2026-08-01 | guide | guide-ratified | exact:2026-07-31 | LoadSharing passive type is core and support owns behavior | docs/rust-migration/02-target-architecture.md | section-3.6 | - |
| H-3 | historical | 2026-08-01 | guide | guide-ratified | exact:2026-07-31 | FixerId is passive core identity and support owns responders | docs/rust-migration/02-target-architecture.md | section-3.3 | - |
| H-4 | historical | 2026-08-01 | guide | guide-ratified | exact:2026-07-31 | Core owns passive cost DTOs and plinth-cost owns behavior | docs/rust-migration/04-migration-plan.md | section-2-cost-rs | - |
| H-5 | historical | 2026-08-01 | guide | guide-ratified | exact:2026-07-31 | Core owns passive metric DTOs and plinth-metrics owns behavior | docs/rust-migration/04-migration-plan.md | section-2-metric-rs | - |
| H-6 | historical | 2026-08-01 | owner:Craig Hughes | documented-owner-approval | exact:2026-08-01 | Python made the one M0 freeze and Rust owns future refreeze | docs/rust-migration/04-migration-plan.md | M1-refreeze | - |
| H-7 | historical | 2026-08-01 | owner:Craig Hughes | documented-owner-approval | exact:2026-07-31 | Replace generic Unknown Saturn disposition with official vendor evidence | docs/rust-migration/06-conventions.md | section-5-generic-unknown-supersession | - |
| H-8 | historical | 2026-08-01 | owner:Craig Hughes | documented-owner-approval | exact:2026-07-31 | Exact corpus is eight models and excludes Cone | docs/rust-migration/04-migration-plan.md | corpus-membership | - |
| H-9 | historical | 2026-08-01 | owner:Craig Hughes | documented-owner-approval | exact:2026-07-31 | Release edge is NegativeY and sweep is PositiveY from negative to positive Y | docs/datasheets/printers/saturn-4-ultra-16k.md | axis-mapping | - |

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

The exact full baseline corpus is, in order: `Sphere`, `Cylinder`, `make_box`,
`make_overhang_table`, `make_toed_slab`, `make_raking_blade`, `make_thin_fin`,
and `Weeping Treant v1.2` (the same model as `Awakened Tree.stl`). The provisional
corpus is the same list minus Treant, exactly seven; neither includes Cone. Source
STLs in `corpus/models/` are ignored inputs, while `corpus/manifest.json` and the
frozen baseline outputs are committed. Tests fail by name when a required member or
artifact is missing.

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
- **The model inputs are never committed.** Every `corpus/models/*.stl` is ignored
  and local. `corpus/manifest.json` and `corpus/baseline/**` are committed when the
  later M0 freeze work item produces them. Do not fabricate a missing manifest or
  baseline during documentation or source-inventory work.
- **Out-of-repo writes have exactly two named carve-outs.** The owner-authorized M0
  prerequisite copies the durable Treant STL to
  `${CHITUBOX_CORPUS_DIR:-/Users/craig/code/hughescr/dnd-campaigns}`; that same directory
  may hold an oversized ChituBox corpus fixture. M9's clean-checkout proof clones into
  a throwaway directory under `$TMPDIR`, builds there, writes no artifact back, and
  deletes the clone. Nothing else may write outside the repository.

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
