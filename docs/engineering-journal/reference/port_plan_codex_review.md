# Independent adversarial architecture review

## Executive verdict

The plan is not approval-ready.

It is directionally right on two points: do not full-port the engine now, and parallelize candidate evaluation rather than concurrent greedy commits. But its determinism model is too permissive, two of its proposed greedy-core strategies do not preserve serial semantics, and the 6–8× claim is not supported by the available profile.

The most serious finding is that the current serial engine already has a determinism defect: `rewire.optimize` stops according to elapsed wall time and returns the partially optimized forest (`rewire.py:858-895`). If that 30-second cap is reached, CPU load, profiling overhead, or a faster implementation changes how many moves commit. The resulting topology can therefore change across runs even before parallelism is introduced. This must be resolved first.

My recommendation is:

- Stay in Python for now.
- Reject a full Rust port.
- Make only bit-preserving, algorithmic Python changes first.
- Add process/thread parallelism to genuinely independent preprocessing.
- Refactor one greedy move at a time into serial candidate generation, parallel candidate evaluation, and serial commit.
- Consider a coarse-grained native extension only if a new full-model profile proves that candidate evaluation remains dominant.

Rust is a possible implementation language for that eventual extension, but the plan has not established that Rust is preferable to Cython/C++ or that a native extension will still be warranted.

---

## 1. Language and port recommendation

### Full port: no

A full Rust or C++ port is unjustified for a team of one and is incompatible with the stated frozen-output invariant in practice.

The engine is about 10,000 lines and depends materially on NumPy, SciPy `cKDTree`, trimesh, and Shapely behavior. Reproducing the geometry, sampling, KD-tree tie behavior, component ordering, and floating-point results in a different geometry stack would establish a different engine, not a function-preserving optimization.

The proposed “10–30×+ plausible” full-Rust estimate is speculation. Approximately 30% of the measured run is already in native geometry/numerical libraries. Rewriting Python orchestration does not make Shapely slicing 30× faster, and Rust does not remove the sequential dependency in the greedy optimizer.

A full port would also create an ongoing second implementation to validate while the design is still changing. Three to six months is probably optimistic once parity debugging, build tooling, macOS distribution, numerical reproducibility, and regression maintenance are included.

### Rust kernel: possibly later, but not the present recommendation

A Python orchestrator with a native numerical kernel is the right *shape* if native code eventually becomes necessary. The plan is premature in naming PyO3/Rust as the preferred solution.

The currently identified bottleneck is primarily self-inflicted algorithmic work:

- Recomputing every live member’s volume for every candidate.
- Repeatedly evaluating unchanged member volumes.
- Scanning every member to find dirty members.
- Constructing a full boolean mask over all clearance samples once per dirty member.
- Eagerly copying roots, terminal maps, dead sets, and brace lists at every checkpoint.
- Recomputing `delicacy_field` for every optimizer invocation.

Those should be fixed before introducing a second language.

If native code is later justified, it must expose a coarse-grained API such as “evaluate this indexed batch of candidate descriptors.” Porting `_member_vol_ml` as a scalar Python-to-Rust call would merely replace NumPy call overhead with extension-call overhead.

Rust also provides no automatic determinism advantage:

- Rayon `sum`/`reduce` changes floating-point grouping.
- Ordered collection is deterministic, but an ordered parallel reduction is not necessarily the serial reduction.
- LLVM, libm, FMA contraction, and SIMD choices can change the last bits of norms and transcendental operations.
- The present code makes topology decisions using strict floating comparisons, so ULP changes are not harmless.

A native evaluator must return one result per candidate index and let the serial code perform the original comparison and tie behavior.

For a team of one already operating in a NumPy/SciPy codebase, Cython may be cheaper to maintain and easier to integrate with existing NumPy storage. PyO3 may still win if the owner is already comfortable maintaining Rust. The profile does not decide this language question.

### Stay-Python: yes, through the first three phases

Python is not the immediate problem. The problem is repeated work and stateful candidate evaluation. Python can support:

- Exact per-member caches.
- Better transaction journaling.
- Batched clearance queries.
- Multiprocess slice computation.
- Threaded read-only verification.
- Process-based candidate evaluation.

Only after those changes are measured should a native kernel be considered.

---

## 2. Determinism and parallel architecture

### The existing wall-clock cutoff is a blocker

`rewire.optimize` checks `time.time()` before each sweep and move. It returns whatever forest exists when the cap expires.

This means:

- The current serial result can depend on machine load.
- Profiling can change the result.
- A faster volume routine can change the result by allowing more moves before the cap.
- Parallel and serial execution cannot be expected to stop after the same move.
- Calling the faster result “better” does not make it bit-identical to the serial reference.

The plan categorizes this as a quality improvement or a “re-approval event.” Under the stated invariant, it is a correctness failure.

The time limit must become one of:

1. A watchdog that aborts the run without returning an output; or
2. A deterministic work budget, such as a fixed number of attempted moves; or
3. Deterministic convergence with no semantic wall-clock cutoff.

If production runs currently hit the cap, there is no stable frozen baseline to preserve. A new deterministic reference must first be deliberately established. That is not optional.

The logging also obscures the evidence: all four possible optimizer invocations write to the same `log["rewire"]` key, overwriting earlier calls. The current data cannot show which calls hit the cap.

### The plan adopts the wrong interpretation of the invariant

The plan explicitly chooses “Reading B”: permit serial-output changes, reapprove them, then require parallel to match the new serial implementation.

That is not what “byte-identical results vs the serial engine” normally means. A rebaselined engine may be acceptable as a separate owner decision, but the performance plan cannot assume permission to redefine the oracle.

Consequently, these proposed actions are noncompliant unless separately authorized:

- Per-island reseeding.
- Replacing NumPy norms with `math.sqrt`.
- Native arithmetic that changes the last bits.
- Allowing a faster optimizer to execute more moves before its time cap.
- A full port.

Also, `engine.run` returns timing values in its log. Those are inherently non-identical. The contract must explicitly identify the compared artifact: emitted file bytes, ordered member/ball geometry, topology, or some combination, while excluding telemetry.

### RNG analysis: partially right, but too broad and too eager to rebaseline

The plan correctly identifies process-global NumPy RNG use. Concurrent sampling calls in threads are unsafe.

However, processes are not universally mandatory. Threads are safe for stages that do not sample, and sampling can be made thread-safe without changing the legacy stream by reproducing trimesh’s sampling routine with an explicit legacy `RandomState`.

The proposed per-island seed scheme changes today’s output unnecessarily. The current installed trimesh sampler consumes a predictable legacy NumPy stream: one random vector for face selection and two random values per selected point. Exact per-island stream states or pre-generated random arrays can therefore be assigned in serial stream order and evaluated in parallel without changing the samples.

Whether that work is worthwhile is doubtful because `load_partition` already calls `cKDTree.query(..., workers=-1)` for each sampled island. Parallelizing islands on top of that risks oversubscription rather than acceleration.

### `island_births`: promising, with stricter boundary handling

Parallelizing slice chunks is the clearest safe parallel opportunity, but the plan’s description is underspecified.

To reproduce the serial implementation:

- Precompute the exact height list produced by each current serial chunk. Do not infer it from a global layer grid: `np.arange` restarts at each `c0`, which matters if `chunk_mm / layer_height` is not integral.
- Each worker must receive the exact predecessor height from that sequence.
- Preserve the order returned by `path2d.discrete` within each plane.
- Gather into preallocated chunk/plane/component slots.
- Do not geometrically re-sort components as a substitute for preserving their original enumeration.
- Verify that slicing one plane as overlap context produces exactly the same path data as slicing it in the original batch.

This can be exact, but it needs a targeted parity test.

Its utility for common small jobs is limited: a model shorter than `chunk_mm` produces one chunk and no parallel speedup. On macOS, process startup and mesh serialization can dominate a small model.

### Independent prepare tasks: reasonable, but resource control is missing

The preparation pipeline can be made into a DAG:

- Overhang islands, minima, births, delicacy sampling, and surface-tree construction are initially independent.
- Contact sampling waits for islands.
- Minima/birth contact augmentation waits for their respective reports.

Processes isolate the global RNG correctly. But the plan omits:

- macOS spawn/pickle cost for a 576k-face mesh;
- worker initialization and reuse;
- resident-memory multiplication;
- nested `cKDTree workers=-1` pools;
- BLAS/OpenMP thread counts;
- the need for serial cutoffs on small workloads.

A single long-lived pool and an explicit total-core budget are necessary. Outer process parallelism should force inner KD-tree queries to one worker. Otherwise four processes can each spawn sixteen SciPy workers.

### Parallel verification: sound in principle, unproven in value

The verification reports are read-only and can be gathered in the current fixed report order. Threads may be preferable to processes because the heavy NumPy/SciPy operations release the GIL and the mesh/forest do not need to be copied.

Again, internally parallel KD-tree queries must not be nested under outer parallelism. More importantly, there is no full-model profile showing that verification is a meaningful part of the 134-second fix block. It should not be a committed phase until measured.

### Candidate-level parallelism: the one sound greedy-core approach

Within one insertion or one rewire move, candidates are conceptually evaluated against the same base forest. This is the appropriate deterministic parallel boundary:

1. Generate candidate descriptors serially in the current order.
2. Evaluate candidates against the same immutable snapshot.
3. Store each result at its original index.
4. Apply the existing serial comparison and tie behavior.
5. Commit exactly one winning candidate using the existing builder.

Even here, “bit-identical by construction” is too strong. The current candidate builders are closures that mutate a shared forest. `revalidate` also changes certificate state. A process implementation would either copy the whole forest per candidate or require a substantial refactor into pure patch evaluation.

Tie handling must reproduce the current code exactly:

- CCO relies on a stable sort by score, preserving builder order for equal scores.
- Rewire uses strict `<`, so the first equal candidate remains selected.
- NaNs and signed zero must not be given new ordering semantics.

Workers should receive chunks of candidates, not one process task per candidate.

### Independent-tree optimization is unsound

The proposed C2 strategy does not preserve serial behavior.

Trees are not independent in this optimizer:

- Root reanchor and bifurcation explicitly consider members in other trees.
- Candidate lookup uses a forest-wide KD cache.
- Accepted moves append globally indexed members.
- Prior moves change `dead`, topology, roots, radii, and the candidate-visible tail.
- Braces can connect otherwise separate trees.
- Convergence is based on global accepted-move savings.
- Full-volume summation includes every tree, and even an unrelated-tree change can alter later rounding.

Applying results in tree-index order is also not the current `_sweep_order`, which sorts globally by estimated volume, move kind, and member index.

At best, this becomes speculative work whose results must be discarded and recomputed at serial commit time. It should be removed from the plan.

### Whole-sweep speculative replay is not serial-equivalent

C3 is also not sound as described.

Revalidating a move’s clearance certificate at commit time is insufficient. Earlier accepted moves can change:

- The candidate set.
- The best candidate.
- Ancestor radii and carried loads.
- Dead/excluded members.
- New tail members visible to the KD cache.
- Whether the move is eligible at all.

A snapshot-selected candidate may still be feasible while no longer being the candidate the serial engine would choose. Preserving serial semantics requires regenerating and rescoring that move after every conflicting earlier commit.

A complete read/write-set transactional system might identify a few nonconflicting moves, but that is research work with likely high conflict rates. It is not a credible planned speedup for a team of one.

---

## 3. Credibility of the speedup claims

The 6–8× figure is not supported by the measurements.

### The profile does not characterize the full-model fix pass

The only fine-grained profile is a 60-face fixture with about 99 live members. The production model has about 1,820 live members. The full production run has only a coarse 134-second “fix-pass” bucket.

The fixture may identify bad asymptotic behavior, but it cannot establish the percentage of production time spent in:

- rewiring;
- CCO construction;
- bracing;
- transient reports;
- verification;
- removal analysis;
- checkpoint copying;
- clearance masks;
- time-cap waiting.

The plan itself admits this gap, yet uses the fixture percentages to produce full-model phase targets.

Additionally, cProfile approximately doubled fixture runtime. Deterministic call tracing disproportionately penalizes millions of tiny function calls, precisely the workload being ranked. The O(N) rescan is real, but the measured percentages are not reliable enough for end-to-end projections. A sampling profiler and targeted counters are needed.

### The phase targets contradict the plan’s own arithmetic

Phase 1 claims `193.8s → ≤78s`, a 2.5× total improvement.

Later, the plan estimates:

- preparation around 50 seconds;
- fix pass around 54 seconds;

for approximately 104 seconds, only 1.86×. Even that classification is wrong: repeated `delicacy_field` calls happen inside `rewire.optimize`, after `t_surface_tree`, so A3 reduces the fix-pass bucket, not initial preparation.

Phase 2 then claims a full run of at most 45 seconds, but its own estimated 13-second preparation plus 54-second fix pass is roughly 67 seconds before Phase 3.

These gates are aspirations, not consequences of the cited data.

### The profile’s cumulative percentages overlap

`_member_vol_ml`, `np.linalg.norm`, `ndarray.dot`, and `np.asarray` are nested costs. A1 and A2 cannot claim all of those savings independently.

Even eliminating the fixture’s entire 9.88 seconds of `_total_volume_ml` cumulative time would reduce 19.29 seconds to about 9.41 seconds: approximately 2.05× on the profiled fixture, not 2.5× on the full run.

### The wall cap makes performance nonlinear

If four optimizer calls each approach 30 seconds, making each candidate faster may not initially reduce wall time at all. The optimizer may simply perform more candidates before hitting the same cap. That changes output and invalidates the benchmark.

Runtime can only be compared meaningfully after semantic time-based termination is removed.

### The claimed “Amdahl ceiling” is not an Amdahl ceiling

With 69.4% truly serial and 30.6% perfectly parallel over 12 cores, conventional Amdahl speedup is only:

`1 / (0.694 + 0.306 / 12) ≈ 1.39×`.

The plan obtains 6.3× by additionally assuming a 2.5× algorithmic reduction of the fix pass and then a further 3× parallel reduction. That is a scenario built from unvalidated multipliers, not a ceiling.

To reach 6×, total runtime must fall to about 32 seconds. Even if slicing drops from 45.5 seconds to 3.8 seconds, the remaining approximately 148 seconds must fall to about 28 seconds—more than 5×. That requires the 134-second fix pass to approach 20 seconds. No current full-model measurement supports that.

A 6× result is possible, but it is a stretch goal, not a credible forecast. An 8× “ceiling” is even less justified.

---

## 4. Important omissions and mistaken priorities

### Safer exact volume optimization

The plan jumps directly to an incrementally maintained forest total. That changes floating-point addition history and cannot reproduce the current full serial sum in general.

A safer first optimization is:

- Cache each member’s volume using its `geom_version`.
- Recompute only changed members with the existing `_member_vol_ml`.
- Sum cached scalar volumes in the exact current live-member index order.

This removes millions of repeated NumPy norms while preserving every addition in `_total_volume_ml`.

If more is required, cache the serial accumulator prefix and, after a candidate, restart summation at the earliest changed member. Reusing an unchanged prefix preserves the original operation order; combining independent block sums does not.

A tolerance assertion is not sufficient. A different last bit can change a candidate decision.

### Clearance processing contains another algorithmic hotspot

`Forest.revalidate` concatenates all dirty-member samples, then creates a full `mi_all == mi` mask for every dirty member. Because each member’s samples were appended contiguously, it can store `(start, end)` offsets and take minima over those slices.

That changes work from approximately O(dirty members × total samples) to O(total samples) without changing sample or minimum order. The analogous pattern exists in final clearance verification.

The plan focuses on the smaller dirty-member list comprehension instead of this larger issue.

### Checkpoints still perform eager O(N) copies

Every candidate checkpoint copies:

- `roots`;
- `terminal_of`;
- `dead`;
- `braces`.

Member and topology changes are journaled lazily, but these collections are not. With tens of thousands of candidate trials, this may be significant on the full model even if it did not rank highly on the tiny fixture.

A mutation journal for these structures should be considered after profiling.

### Small single-model behavior is not addressed adequately

The owner’s common case is a single small model. Several headline wins depend on large workloads:

- A short model has one island-birth chunk.
- A small forest provides few candidates per move.
- Process startup and mesh copying can exceed computation.
- Parallel verification can be slower than serial verification.

The implementation needs measured thresholds and a persistent pool. The review criterion should be latency over a representative small-model suite, not only Treant throughput.

### Existing internal parallelism and oversubscription

The engine is not uniformly single-core. Several `cKDTree.query` calls already use all workers. `delicacy_field` also uses parallel queries. Adding outer process pools without a core-budget policy can make performance worse and increase timing variability.

### Determinism validation is too shallow

Three repeated file hashes are necessary but insufficient. The harness should test:

- Worker counts 1, 2, 3, 4, 8, and 12.
- Repeated runs under artificial CPU contention.
- Different `PYTHONHASHSEED` values.
- Serial and parallel candidate decision traces.
- Exact RNG samples and stream positions.
- Candidate score bit patterns.
- Accepted move sequence and winning candidate indices.
- Forest topology, member order, dead set, and brace order.
- Final emitted bytes.
- Supported NumPy/SciPy/trimesh versions and CPU architectures.

A shared serial/parallel code path is useful, but a retained independent serial oracle is also necessary; otherwise a common bug can make both modes match.

---

## 5. Recommended plan

### Phase 0 — Make determinism well-defined

Before performance work:

1. Define the oracle artifact. Exclude timing telemetry.
2. Log every optimizer invocation separately.
3. Record cap hits, attempted/accepted/rejected moves, sweep count, candidate count, and stop reason.
4. Eliminate partial-result wall-clock termination.
5. Establish golden outputs for representative small, medium, and Treant models.
6. Capture a full-model sampling profile and targeted counters without cProfile distortion.
7. Measure unprofiled runtimes over multiple runs, including CPU time and peak RSS.

If the current production model hits the time cap, establish a deterministic serial reference before claiming any bit-identical parallelization.

### Phase 1 — Exact Python algorithmic fixes

In this order:

1. Pass the already-computed delicacy samples into every `rewire.optimize` call.
2. Add exact per-member volume caching keyed by geometry version.
3. Preserve the current serial live-index summation order.
4. Replace clearance ownership masks with contiguous sample offsets.
5. Profile checkpoint copying; introduce transaction journaling if it matters.
6. Consider a sorted dirty-index structure only after the larger clearance issue is fixed.
7. Do not replace NumPy scalar math with `math` operations unless primitive and end-to-end bit parity is demonstrated.

No 2.5× gate should be promised. Measure the result.

### Phase 2 — Exact independent-stage parallelism

1. Parallelize `island_births` using exact precomputed serial height/chunk definitions.
2. Build a preparation DAG around islands, minima, births, delicacy, contacts, and surface-tree construction.
3. Reuse a long-lived pool with one initialized mesh copy per worker.
4. Disable nested KD-tree/BLAS parallelism inside outer workers.
5. Add serial cutoffs based on measured chunk count and workload.
6. Parallelize verification only if the full profile shows value; prefer threads for read-only NumPy/SciPy gates.
7. Leave `load_partition` alone unless it remains a hotspot. If parallelized, preserve the legacy RNG stream using assigned `RandomState` states rather than per-island reseeding.

### Phase 3 — Parallelism inside one greedy decision

Refactor CCO insertion first, because its candidate list and returned delta are simpler:

1. Generate candidate descriptors serially.
2. Freeze one immutable forest snapshot.
3. Evaluate candidate chunks in parallel.
4. Return results by original candidate index.
5. Apply the existing stable score selection.
6. Reapply and commit the winner serially.

Then apply the same model to one rewire move type at a time. Batch candidate clearance samples into large KD-tree queries where possible.

Do not parallelize separate trees or entire sweeps. Serial move commit order remains the semantic spine of the engine.

For small models, enable candidate parallelism only when the candidate count and estimated cost exceed a measured threshold.

### Phase 4 — Optional coarse-grained native evaluator

Only if Phase 3 profiling still shows Python candidate evaluation dominating:

1. Prototype one batched native kernel.
2. Compare every candidate result bit-for-bit against the Python oracle.
3. Compare winning-candidate and accepted-move traces, not only final files.
4. Return indexed results; perform reduction and commit serially.
5. Ban parallel floating reductions inside the native code.
6. Choose Rust, Cython, or C++ based on measured integration effort and owner expertise.

A full port should remain rejected unless the engine is feature-frozen, the Python/native hybrid has demonstrably failed the latency requirement, and a new output baseline is explicitly authorized.

---

## Final judgment

The plan’s overall “hybrid, not full rewrite” instinct is sound, but the proposed implementation sequence and performance claims need substantial revision.

The correct immediate decision is not “Rust kernel versus full Rust.” It is:

1. Repair the time-based determinism defect.
2. Obtain a real full-model profile of all four optimizer calls.
3. Remove exact, repeated work in Python.
4. Parallelize independent slices and preparation with strict resource control.
5. Expose deterministic candidate-batch evaluation while keeping greedy commits serial.
6. Decide whether native code is still needed.

I would not approve the current 6–8× forecast, C2 independent-tree optimization, C3 snapshot sweep replay, per-island reseeding, or a committed PyO3 project. I would approve a revised plan centered on exact caches, batched clearance, deterministic termination, chunked slice processing, and per-move candidate parallelism.
tokens used
155,341
# Independent adversarial architecture review

## Executive verdict

The plan is not approval-ready.

It is directionally right on two points: do not full-port the engine now, and parallelize candidate evaluation rather than concurrent greedy commits. But its determinism model is too permissive, two of its proposed greedy-core strategies do not preserve serial semantics, and the 6–8× claim is not supported by the available profile.

The most serious finding is that the current serial engine already has a determinism defect: `rewire.optimize` stops according to elapsed wall time and returns the partially optimized forest (`rewire.py:858-895`). If that 30-second cap is reached, CPU load, profiling overhead, or a faster implementation changes how many moves commit. The resulting topology can therefore change across runs even before parallelism is introduced. This must be resolved first.

My recommendation is:

- Stay in Python for now.
- Reject a full Rust port.
- Make only bit-preserving, algorithmic Python changes first.
- Add process/thread parallelism to genuinely independent preprocessing.
- Refactor one greedy move at a time into serial candidate generation, parallel candidate evaluation, and serial commit.
- Consider a coarse-grained native extension only if a new full-model profile proves that candidate evaluation remains dominant.

Rust is a possible implementation language for that eventual extension, but the plan has not established that Rust is preferable to Cython/C++ or that a native extension will still be warranted.

---

## 1. Language and port recommendation

### Full port: no

A full Rust or C++ port is unjustified for a team of one and is incompatible with the stated frozen-output invariant in practice.

The engine is about 10,000 lines and depends materially on NumPy, SciPy `cKDTree`, trimesh, and Shapely behavior. Reproducing the geometry, sampling, KD-tree tie behavior, component ordering, and floating-point results in a different geometry stack would establish a different engine, not a function-preserving optimization.

The proposed “10–30×+ plausible” full-Rust estimate is speculation. Approximately 30% of the measured run is already in native geometry/numerical libraries. Rewriting Python orchestration does not make Shapely slicing 30× faster, and Rust does not remove the sequential dependency in the greedy optimizer.

A full port would also create an ongoing second implementation to validate while the design is still changing. Three to six months is probably optimistic once parity debugging, build tooling, macOS distribution, numerical reproducibility, and regression maintenance are included.

### Rust kernel: possibly later, but not the present recommendation

A Python orchestrator with a native numerical kernel is the right *shape* if native code eventually becomes necessary. The plan is premature in naming PyO3/Rust as the preferred solution.

The currently identified bottleneck is primarily self-inflicted algorithmic work:

- Recomputing every live member’s volume for every candidate.
- Repeatedly evaluating unchanged member volumes.
- Scanning every member to find dirty members.
- Constructing a full boolean mask over all clearance samples once per dirty member.
- Eagerly copying roots, terminal maps, dead sets, and brace lists at every checkpoint.
- Recomputing `delicacy_field` for every optimizer invocation.

Those should be fixed before introducing a second language.

If native code is later justified, it must expose a coarse-grained API such as “evaluate this indexed batch of candidate descriptors.” Porting `_member_vol_ml` as a scalar Python-to-Rust call would merely replace NumPy call overhead with extension-call overhead.

Rust also provides no automatic determinism advantage:

- Rayon `sum`/`reduce` changes floating-point grouping.
- Ordered collection is deterministic, but an ordered parallel reduction is not necessarily the serial reduction.
- LLVM, libm, FMA contraction, and SIMD choices can change the last bits of norms and transcendental operations.
- The present code makes topology decisions using strict floating comparisons, so ULP changes are not harmless.

A native evaluator must return one result per candidate index and let the serial code perform the original comparison and tie behavior.

For a team of one already operating in a NumPy/SciPy codebase, Cython may be cheaper to maintain and easier to integrate with existing NumPy storage. PyO3 may still win if the owner is already comfortable maintaining Rust. The profile does not decide this language question.

### Stay-Python: yes, through the first three phases

Python is not the immediate problem. The problem is repeated work and stateful candidate evaluation. Python can support:

- Exact per-member caches.
- Better transaction journaling.
- Batched clearance queries.
- Multiprocess slice computation.
- Threaded read-only verification.
- Process-based candidate evaluation.

Only after those changes are measured should a native kernel be considered.

---

## 2. Determinism and parallel architecture

### The existing wall-clock cutoff is a blocker

`rewire.optimize` checks `time.time()` before each sweep and move. It returns whatever forest exists when the cap expires.

This means:

- The current serial result can depend on machine load.
- Profiling can change the result.
- A faster volume routine can change the result by allowing more moves before the cap.
- Parallel and serial execution cannot be expected to stop after the same move.
- Calling the faster result “better” does not make it bit-identical to the serial reference.

The plan categorizes this as a quality improvement or a “re-approval event.” Under the stated invariant, it is a correctness failure.

The time limit must become one of:

1. A watchdog that aborts the run without returning an output; or
2. A deterministic work budget, such as a fixed number of attempted moves; or
3. Deterministic convergence with no semantic wall-clock cutoff.

If production runs currently hit the cap, there is no stable frozen baseline to preserve. A new deterministic reference must first be deliberately established. That is not optional.

The logging also obscures the evidence: all four possible optimizer invocations write to the same `log["rewire"]` key, overwriting earlier calls. The current data cannot show which calls hit the cap.

### The plan adopts the wrong interpretation of the invariant

The plan explicitly chooses “Reading B”: permit serial-output changes, reapprove them, then require parallel to match the new serial implementation.

That is not what “byte-identical results vs the serial engine” normally means. A rebaselined engine may be acceptable as a separate owner decision, but the performance plan cannot assume permission to redefine the oracle.

Consequently, these proposed actions are noncompliant unless separately authorized:

- Per-island reseeding.
- Replacing NumPy norms with `math.sqrt`.
- Native arithmetic that changes the last bits.
- Allowing a faster optimizer to execute more moves before its time cap.
- A full port.

Also, `engine.run` returns timing values in its log. Those are inherently non-identical. The contract must explicitly identify the compared artifact: emitted file bytes, ordered member/ball geometry, topology, or some combination, while excluding telemetry.

### RNG analysis: partially right, but too broad and too eager to rebaseline

The plan correctly identifies process-global NumPy RNG use. Concurrent sampling calls in threads are unsafe.

However, processes are not universally mandatory. Threads are safe for stages that do not sample, and sampling can be made thread-safe without changing the legacy stream by reproducing trimesh’s sampling routine with an explicit legacy `RandomState`.

The proposed per-island seed scheme changes today’s output unnecessarily. The current installed trimesh sampler consumes a predictable legacy NumPy stream: one random vector for face selection and two random values per selected point. Exact per-island stream states or pre-generated random arrays can therefore be assigned in serial stream order and evaluated in parallel without changing the samples.

Whether that work is worthwhile is doubtful because `load_partition` already calls `cKDTree.query(..., workers=-1)` for each sampled island. Parallelizing islands on top of that risks oversubscription rather than acceleration.

### `island_births`: promising, with stricter boundary handling

Parallelizing slice chunks is the clearest safe parallel opportunity, but the plan’s description is underspecified.

To reproduce the serial implementation:

- Precompute the exact height list produced by each current serial chunk. Do not infer it from a global layer grid: `np.arange` restarts at each `c0`, which matters if `chunk_mm / layer_height` is not integral.
- Each worker must receive the exact predecessor height from that sequence.
- Preserve the order returned by `path2d.discrete` within each plane.
- Gather into preallocated chunk/plane/component slots.
- Do not geometrically re-sort components as a substitute for preserving their original enumeration.
- Verify that slicing one plane as overlap context produces exactly the same path data as slicing it in the original batch.

This can be exact, but it needs a targeted parity test.

Its utility for common small jobs is limited: a model shorter than `chunk_mm` produces one chunk and no parallel speedup. On macOS, process startup and mesh serialization can dominate a small model.

### Independent prepare tasks: reasonable, but resource control is missing

The preparation pipeline can be made into a DAG:

- Overhang islands, minima, births, delicacy sampling, and surface-tree construction are initially independent.
- Contact sampling waits for islands.
- Minima/birth contact augmentation waits for their respective reports.

Processes isolate the global RNG correctly. But the plan omits:

- macOS spawn/pickle cost for a 576k-face mesh;
- worker initialization and reuse;
- resident-memory multiplication;
- nested `cKDTree workers=-1` pools;
- BLAS/OpenMP thread counts;
- the need for serial cutoffs on small workloads.

A single long-lived pool and an explicit total-core budget are necessary. Outer process parallelism should force inner KD-tree queries to one worker. Otherwise four processes can each spawn sixteen SciPy workers.

### Parallel verification: sound in principle, unproven in value

The verification reports are read-only and can be gathered in the current fixed report order. Threads may be preferable to processes because the heavy NumPy/SciPy operations release the GIL and the mesh/forest do not need to be copied.

Again, internally parallel KD-tree queries must not be nested under outer parallelism. More importantly, there is no full-model profile showing that verification is a meaningful part of the 134-second fix block. It should not be a committed phase until measured.

### Candidate-level parallelism: the one sound greedy-core approach

Within one insertion or one rewire move, candidates are conceptually evaluated against the same base forest. This is the appropriate deterministic parallel boundary:

1. Generate candidate descriptors serially in the current order.
2. Evaluate candidates against the same immutable snapshot.
3. Store each result at its original index.
4. Apply the existing serial comparison and tie behavior.
5. Commit exactly one winning candidate using the existing builder.

Even here, “bit-identical by construction” is too strong. The current candidate builders are closures that mutate a shared forest. `revalidate` also changes certificate state. A process implementation would either copy the whole forest per candidate or require a substantial refactor into pure patch evaluation.

Tie handling must reproduce the current code exactly:

- CCO relies on a stable sort by score, preserving builder order for equal scores.
- Rewire uses strict `<`, so the first equal candidate remains selected.
- NaNs and signed zero must not be given new ordering semantics.

Workers should receive chunks of candidates, not one process task per candidate.

### Independent-tree optimization is unsound

The proposed C2 strategy does not preserve serial behavior.

Trees are not independent in this optimizer:

- Root reanchor and bifurcation explicitly consider members in other trees.
- Candidate lookup uses a forest-wide KD cache.
- Accepted moves append globally indexed members.
- Prior moves change `dead`, topology, roots, radii, and the candidate-visible tail.
- Braces can connect otherwise separate trees.
- Convergence is based on global accepted-move savings.
- Full-volume summation includes every tree, and even an unrelated-tree change can alter later rounding.

Applying results in tree-index order is also not the current `_sweep_order`, which sorts globally by estimated volume, move kind, and member index.

At best, this becomes speculative work whose results must be discarded and recomputed at serial commit time. It should be removed from the plan.

### Whole-sweep speculative replay is not serial-equivalent

C3 is also not sound as described.

Revalidating a move’s clearance certificate at commit time is insufficient. Earlier accepted moves can change:

- The candidate set.
- The best candidate.
- Ancestor radii and carried loads.
- Dead/excluded members.
- New tail members visible to the KD cache.
- Whether the move is eligible at all.

A snapshot-selected candidate may still be feasible while no longer being the candidate the serial engine would choose. Preserving serial semantics requires regenerating and rescoring that move after every conflicting earlier commit.

A complete read/write-set transactional system might identify a few nonconflicting moves, but that is research work with likely high conflict rates. It is not a credible planned speedup for a team of one.

---

## 3. Credibility of the speedup claims

The 6–8× figure is not supported by the measurements.

### The profile does not characterize the full-model fix pass

The only fine-grained profile is a 60-face fixture with about 99 live members. The production model has about 1,820 live members. The full production run has only a coarse 134-second “fix-pass” bucket.

The fixture may identify bad asymptotic behavior, but it cannot establish the percentage of production time spent in:

- rewiring;
- CCO construction;
- bracing;
- transient reports;
- verification;
- removal analysis;
- checkpoint copying;
- clearance masks;
- time-cap waiting.

The plan itself admits this gap, yet uses the fixture percentages to produce full-model phase targets.

Additionally, cProfile approximately doubled fixture runtime. Deterministic call tracing disproportionately penalizes millions of tiny function calls, precisely the workload being ranked. The O(N) rescan is real, but the measured percentages are not reliable enough for end-to-end projections. A sampling profiler and targeted counters are needed.

### The phase targets contradict the plan’s own arithmetic

Phase 1 claims `193.8s → ≤78s`, a 2.5× total improvement.

Later, the plan estimates:

- preparation around 50 seconds;
- fix pass around 54 seconds;

for approximately 104 seconds, only 1.86×. Even that classification is wrong: repeated `delicacy_field` calls happen inside `rewire.optimize`, after `t_surface_tree`, so A3 reduces the fix-pass bucket, not initial preparation.

Phase 2 then claims a full run of at most 45 seconds, but its own estimated 13-second preparation plus 54-second fix pass is roughly 67 seconds before Phase 3.

These gates are aspirations, not consequences of the cited data.

### The profile’s cumulative percentages overlap

`_member_vol_ml`, `np.linalg.norm`, `ndarray.dot`, and `np.asarray` are nested costs. A1 and A2 cannot claim all of those savings independently.

Even eliminating the fixture’s entire 9.88 seconds of `_total_volume_ml` cumulative time would reduce 19.29 seconds to about 9.41 seconds: approximately 2.05× on the profiled fixture, not 2.5× on the full run.

### The wall cap makes performance nonlinear

If four optimizer calls each approach 30 seconds, making each candidate faster may not initially reduce wall time at all. The optimizer may simply perform more candidates before hitting the same cap. That changes output and invalidates the benchmark.

Runtime can only be compared meaningfully after semantic time-based termination is removed.

### The claimed “Amdahl ceiling” is not an Amdahl ceiling

With 69.4% truly serial and 30.6% perfectly parallel over 12 cores, conventional Amdahl speedup is only:

`1 / (0.694 + 0.306 / 12) ≈ 1.39×`.

The plan obtains 6.3× by additionally assuming a 2.5× algorithmic reduction of the fix pass and then a further 3× parallel reduction. That is a scenario built from unvalidated multipliers, not a ceiling.

To reach 6×, total runtime must fall to about 32 seconds. Even if slicing drops from 45.5 seconds to 3.8 seconds, the remaining approximately 148 seconds must fall to about 28 seconds—more than 5×. That requires the 134-second fix pass to approach 20 seconds. No current full-model measurement supports that.

A 6× result is possible, but it is a stretch goal, not a credible forecast. An 8× “ceiling” is even less justified.

---

## 4. Important omissions and mistaken priorities

### Safer exact volume optimization

The plan jumps directly to an incrementally maintained forest total. That changes floating-point addition history and cannot reproduce the current full serial sum in general.

A safer first optimization is:

- Cache each member’s volume using its `geom_version`.
- Recompute only changed members with the existing `_member_vol_ml`.
- Sum cached scalar volumes in the exact current live-member index order.

This removes millions of repeated NumPy norms while preserving every addition in `_total_volume_ml`.

If more is required, cache the serial accumulator prefix and, after a candidate, restart summation at the earliest changed member. Reusing an unchanged prefix preserves the original operation order; combining independent block sums does not.

A tolerance assertion is not sufficient. A different last bit can change a candidate decision.

### Clearance processing contains another algorithmic hotspot

`Forest.revalidate` concatenates all dirty-member samples, then creates a full `mi_all == mi` mask for every dirty member. Because each member’s samples were appended contiguously, it can store `(start, end)` offsets and take minima over those slices.

That changes work from approximately O(dirty members × total samples) to O(total samples) without changing sample or minimum order. The analogous pattern exists in final clearance verification.

The plan focuses on the smaller dirty-member list comprehension instead of this larger issue.

### Checkpoints still perform eager O(N) copies

Every candidate checkpoint copies:

- `roots`;
- `terminal_of`;
- `dead`;
- `braces`.

Member and topology changes are journaled lazily, but these collections are not. With tens of thousands of candidate trials, this may be significant on the full model even if it did not rank highly on the tiny fixture.

A mutation journal for these structures should be considered after profiling.

### Small single-model behavior is not addressed adequately

The owner’s common case is a single small model. Several headline wins depend on large workloads:

- A short model has one island-birth chunk.
- A small forest provides few candidates per move.
- Process startup and mesh copying can exceed computation.
- Parallel verification can be slower than serial verification.

The implementation needs measured thresholds and a persistent pool. The review criterion should be latency over a representative small-model suite, not only Treant throughput.

### Existing internal parallelism and oversubscription

The engine is not uniformly single-core. Several `cKDTree.query` calls already use all workers. `delicacy_field` also uses parallel queries. Adding outer process pools without a core-budget policy can make performance worse and increase timing variability.

### Determinism validation is too shallow

Three repeated file hashes are necessary but insufficient. The harness should test:

- Worker counts 1, 2, 3, 4, 8, and 12.
- Repeated runs under artificial CPU contention.
- Different `PYTHONHASHSEED` values.
- Serial and parallel candidate decision traces.
- Exact RNG samples and stream positions.
- Candidate score bit patterns.
- Accepted move sequence and winning candidate indices.
- Forest topology, member order, dead set, and brace order.
- Final emitted bytes.
- Supported NumPy/SciPy/trimesh versions and CPU architectures.

A shared serial/parallel code path is useful, but a retained independent serial oracle is also necessary; otherwise a common bug can make both modes match.

---

## 5. Recommended plan

### Phase 0 — Make determinism well-defined

Before performance work:

1. Define the oracle artifact. Exclude timing telemetry.
2. Log every optimizer invocation separately.
3. Record cap hits, attempted/accepted/rejected moves, sweep count, candidate count, and stop reason.
4. Eliminate partial-result wall-clock termination.
5. Establish golden outputs for representative small, medium, and Treant models.
6. Capture a full-model sampling profile and targeted counters without cProfile distortion.
7. Measure unprofiled runtimes over multiple runs, including CPU time and peak RSS.

If the current production model hits the time cap, establish a deterministic serial reference before claiming any bit-identical parallelization.

### Phase 1 — Exact Python algorithmic fixes

In this order:

1. Pass the already-computed delicacy samples into every `rewire.optimize` call.
2. Add exact per-member volume caching keyed by geometry version.
3. Preserve the current serial live-index summation order.
4. Replace clearance ownership masks with contiguous sample offsets.
5. Profile checkpoint copying; introduce transaction journaling if it matters.
6. Consider a sorted dirty-index structure only after the larger clearance issue is fixed.
7. Do not replace NumPy scalar math with `math` operations unless primitive and end-to-end bit parity is demonstrated.

No 2.5× gate should be promised. Measure the result.

### Phase 2 — Exact independent-stage parallelism

1. Parallelize `island_births` using exact precomputed serial height/chunk definitions.
2. Build a preparation DAG around islands, minima, births, delicacy, contacts, and surface-tree construction.
3. Reuse a long-lived pool with one initialized mesh copy per worker.
4. Disable nested KD-tree/BLAS parallelism inside outer workers.
5. Add serial cutoffs based on measured chunk count and workload.
6. Parallelize verification only if the full profile shows value; prefer threads for read-only NumPy/SciPy gates.
7. Leave `load_partition` alone unless it remains a hotspot. If parallelized, preserve the legacy RNG stream using assigned `RandomState` states rather than per-island reseeding.

### Phase 3 — Parallelism inside one greedy decision

Refactor CCO insertion first, because its candidate list and returned delta are simpler:

1. Generate candidate descriptors serially.
2. Freeze one immutable forest snapshot.
3. Evaluate candidate chunks in parallel.
4. Return results by original candidate index.
5. Apply the existing stable score selection.
6. Reapply and commit the winner serially.

Then apply the same model to one rewire move type at a time. Batch candidate clearance samples into large KD-tree queries where possible.

Do not parallelize separate trees or entire sweeps. Serial move commit order remains the semantic spine of the engine.

For small models, enable candidate parallelism only when the candidate count and estimated cost exceed a measured threshold.

### Phase 4 — Optional coarse-grained native evaluator

Only if Phase 3 profiling still shows Python candidate evaluation dominating:

1. Prototype one batched native kernel.
2. Compare every candidate result bit-for-bit against the Python oracle.
3. Compare winning-candidate and accepted-move traces, not only final files.
4. Return indexed results; perform reduction and commit serially.
5. Ban parallel floating reductions inside the native code.
6. Choose Rust, Cython, or C++ based on measured integration effort and owner expertise.

A full port should remain rejected unless the engine is feature-frozen, the Python/native hybrid has demonstrably failed the latency requirement, and a new output baseline is explicitly authorized.

---

## Final judgment

The plan’s overall “hybrid, not full rewrite” instinct is sound, but the proposed implementation sequence and performance claims need substantial revision.

The correct immediate decision is not “Rust kernel versus full Rust.” It is:

1. Repair the time-based determinism defect.
2. Obtain a real full-model profile of all four optimizer calls.
3. Remove exact, repeated work in Python.
4. Parallelize independent slices and preparation with strict resource control.
5. Expose deterministic candidate-batch evaluation while keeping greedy commits serial.
6. Decide whether native code is still needed.

I would not approve the current 6–8× forecast, C2 independent-tree optimization, C3 snapshot sweep replay, per-island reseeding, or a committed PyO3 project. I would approve a revised plan centered on exact caches, batched clearance, deterministic termination, chunked slice processing, and per-move candidate parallelism.
