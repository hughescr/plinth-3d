# `geom_kernel` — exact geometry kernel contract

Status: current contract for `rust/geom_kernel`, the exact-geometry hot core
consumed directly by the `crates/plinth-*` workspace.

**This crate has no Python bridge.** It was originally built as a Rust core
behind a PyO3/maturin extension module, differentially tested bit-for-bit
against a Python oracle (`scripts/support_placement/geometry_exact.py`,
`geometry_kernel.KernelSurface`). Both the PyO3 boundary and the Python
oracle it was tested against are retired: the no-new-Python, oracle-first
Rust rewrite ruling (2026-07-28,
`docs/engineering-journal/decisions/2026-07-28-rust-rewrite.md`) completed,
and `scripts/support_placement/` no longer exists in this tree.
`rust/geom_kernel` is now the sole implementation, called directly as a Rust
library dependency (no FFI, no numpy boundary). Its dependencies today are
`kiddo` (spatial index), `rayon`, and `sha2` — no `pyo3`, no `numpy` crate.

This document states the invariants that constitute the crate's contract
with its callers. **It does not duplicate the crate's function-level API** —
that lives in the crate's own source and rustdoc and is expected to evolve
with the code; freezing a function-signature table here would drift from the
compiler-enforced truth on the first refactor. What follows are the
properties callers are entitled to rely on.

## What the kernel provides

Exact-geometry primitives over a triangle-soup surface: nearest-triangle
query and projection, ray casting, inside/outside classification (parity
vote), segment/capsule-to-surface clearance, sphere-breach detection, and
exterior-face masking — the same operation set the pipeline's coverage,
clearance, and embed-policy checks are built on (see
[`coverage-repair.md`](coverage-repair.md) and
[`embed-policy.md`](embed-policy.md) for the consumers). Batch entry points
are the norm; a per-element call in a hot path is a defect (project-wide
batch-first doctrine).

## Determinism contract

**Results are bit-identical across repeated runs and across thread counts,
for identical immutable kernel state and identical inputs.** This is a hard
requirement, not an aspiration — it is a stated precondition several
downstream mechanisms rely on (for example the legalization retry's
termination argument in [`seal-semantics.md`](seal-semantics.md), which
depends on the routing cadence replaying identically).

The construction that makes this true:

- **BVH build is deterministic and soup-tolerant.** Binary AABB BVH over
  triangle indices, built once and serially. Split selection (binned SAH,
  16 bins on the largest-centroid-extent axis) has no RNG and no
  pointer/hash-order dependence; ties are broken by fixed axis order, then
  lowest bin index; a degenerate split (coincident centroids) falls back to
  a median split on original face index. Build input order is the mesh's own
  face order, so two builds of the same mesh are structurally identical.
  Degenerate triangles (zero area/extent) are kept and counted, never
  dropped — they are handled by the same numerical guards as any other
  triangle, never specially excluded.
- **Query traversal order is fixed, never completion-order.** Nearer-AABB-
  first child traversal, leaves visited in ascending face index. Any
  tie-break (equal-distance candidates, coincident triangles) resolves to
  the lowest face index — deterministic on any thread.
- **Rayon parallelization follows one sanctioned pattern: parallel across
  query indices only.** Every query is a pure function of (immutable kernel
  state, its own inputs); results land in the output slot of their own
  index (indexed collect, or `par_chunks_mut` writing into a pre-allocated
  output). There is no cross-query reduction anywhere in the API surface —
  no sums, no global argmins, no accumulators shared across queries. Because
  kernel state is built serially and is immutable during queries, and each
  query's traversal order does not depend on any other query, output arrays
  are bit-identical for any thread count.
- **Explicit prohibitions**, because these are the ways a parallel geometry
  kernel silently loses determinism: no `rayon::iter::sum/reduce/fold` over
  floats (regrouping changes results — a genuine cross-query reduction must
  gather per-index partials and reduce serially in fixed index order); no
  `HashMap`/`HashSet` iteration feeding any result (iteration order leaks
  process randomness — grids and candidate sets are array/CSR-backed by
  construction); no wall-clock read inside the kernel and no time-based
  decision affecting a result; any lazily-built cache uses a `OnceLock` (or
  equivalent) with deterministic content and no reset; the crate never uses
  a shared global thread pool for a thread-count-sensitive computation — a
  test that sweeps thread count must be able to observe that the count
  actually changed, or it passes vacuously.

## Numerical discipline

- No fast-math: no FMA contraction requested, no `target-cpu=native`, no
  `-ffast-math`-equivalent. `f64::sqrt` is IEEE-754 correctly rounded.
  Reproducibility across machines is chosen over a few percent of speed.
- Where an operand-construction sequence is load-bearing for a boundary
  decision (a point sitting exactly on a classification threshold, a
  parity-vote sample landing on a surface), the evaluation order is pinned
  as a named constant or a comment at the computation site, not left to
  "algebraically equivalent" reassociation — an idiomatic-looking
  simplification can move the last bit and flip a boolean the pipeline
  gates on.
- Verdict booleans the pipeline consumes (inside/outside, breach,
  interval-exists) threshold a quantity whose decision margin is a physical
  one (tenths of a millimetre), many orders above numerical noise — the
  numerical discipline above exists to keep that margin real, not to chase
  bit-parity for its own sake.

## Memory and robustness

Per-query working state is a bounded traversal stack, not an
allocate-per-candidate-set pattern. Tree depth is recorded at build time and
an implausible depth fails construction cleanly rather than degrading
silently. All index/offset narrowing to fixed-width integers is checked;
overflow fails with a typed error rather than wrapping. A build that would
exceed its configured memory budget fails cleanly instead of thrashing.

## See also

- `rust/geom_kernel/src/*.rs` — the authoritative current function-level API
  and its own inline documentation.
- [`docs/development.md`](../development.md) — the determinism and golden
  re-pin rule this crate's outputs feed.
- [`docs/engineering-journal/decisions/2026-07-28-rust-rewrite.md`](../engineering-journal/decisions/2026-07-28-rust-rewrite.md)
  — the ruling that retired the Python oracle this crate was originally
  built to mirror.
