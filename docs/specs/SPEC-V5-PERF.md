# SPEC-V5-PERF — construct-stage performance: the poisoned-revalidate finding, incremental revalidation, exact result caching, and approximation levers

Status: PROPOSED, AMENDED after adversarial adjudication (branch `v5-perf`,
base `v5-rust-kernel` @ 3a31fc4). Two independent critiques (Codex
cross-family + Opus adversarial) were adjudicated against the code on
2026-07-23; every load-bearing claim was re-verified in tree.py / cco.py /
rewire.py / transient.py / engine.py / queries.rs before ruling. Upheld
findings are folded into the body below (marked **[ADJ]**); rejected
challenges are recorded with reasoning in Appendix A.
Owner rulings binding on this spec (2026-07-23):
1. **Correctness inviolable** — no penetrations, hard gates zero, sanctioned-embed
   semantics unchanged.
2. **Tradeable margin is solution QUALITY only** (resin ml, support-count
   optimality), never correctness — and only via CLASS-B levers, each behind a
   config flag, ALL DEFAULT OFF.
3. **Batch-first doctrine**: per-item FFI calls in hot paths are defects.
4. **Progress reporting off the hot path**: counters in loops; a ~5s timer
   thread does the printing; compute path byte-identical with reporting on/off.
5. **Determinism**: class-A changes BIT-IDENTICAL to the unoptimized engine;
   class-B approximations deterministic (the engine has no RNG; ordering is
   index-tiebroken; thread-invariance inherited from the kernel) but may
   change output.

Reference: SPEC-RUST-KERNEL.md (A7/A18 batching amendments, R1, sec 10
roadmap — stage 2 "fix-pass numeric core" is the eventual Rust home of any
hot Python that survives this spec; every class-A mechanism below is designed
as serial bookkeeping + pure-function-of-geometry lookups precisely so it
ports to stage 2 without semantic change).

Measurement harness: `scripts/tests/manual_perf_probe.py` (committed with
this spec). All treant numbers below: Weeping Treant v1.2.stl, 576,026 faces,
GEOM_KERNEL_THREADS=4, on a host concurrently running two full-scale engine
runs — query COUNTS are the primary evidence (deterministic,
contention-immune); wall-clock is secondary.

---

## 0. BLOCKER FINDING (verified, reproducible in 0.8s): full-scale construct is a poisoned loop marching toward a crash

The query-blowup hypothesis was verified in code and then measured — and the
dominant mechanism is sharper than "per-insertion whole-forest revalidation":

**At the full 747-contact treant partition, 49 stubs fail the v5 stub
through-transit interval check on the very first revalidate and stay dirty
forever.** (Reproduction: build cco.build's 747 stubs verbatim, one
`forest.revalidate` — 49 violations, 0.8s. At a 300-contact truncated
partition it is 4 stubs; the truncation changes the load partition and thus
stub radii/insets, so the exact set shifts — the class is scale-robust.)

Two compounding consequences in `cco._insert_terminal` (cco.py:280–306):

1. **Poisoning**: the trial loop rejects a candidate when `revalidate`
   returns ANY violation — including the 49 pre-existing stub violations
   that have nothing to do with the candidate. Every candidate of every
   contact is therefore rejected: **construct's final state is every contact
   unrouted, followed by an `assert_all_certified` AssertionError** (the 49
   stubs are still dirty). The in-flight full-scale runs are spending their
   30–90+ min producing that crash. (bracing.py:865–892 already implements
   the pre-existing-violation snapshot pattern for exactly this reason; cco
   predates the stub rule and never needed it before v5.)
2. **The 10^8–10^9 query fingerprint**: violators stay dirty, so EVERY
   revalidate call re-examines them from scratch — ~49 axis-interval jobs
   (each: Möller crossing pass + up to `min(80, L/0.12mm)` parity votes per
   sub-segment + a BVH nearest per inside sample, queries.rs
   `pen_intervals_one`) plus ~49 scalar `geom.project` calls
   (`_embed_inset_arc`) plus the stub-p1 parity batch, per call. With ~120
   trials/insertion x 747 insertions ≈ 9x10^4 revalidate calls, that is
   ~10^8–10^9 elementary kernel queries — exactly the live stack sample
   (rayon workers dominated by parity `inside` votes and
   `penetration_intervals_batch`). The kernel is fast; it is being asked the
   same ~49 questions ninety thousand times.

Measured (probe, N=300 truncated partition, 4 persistent violators —
smaller poison, same shape): construct 328.7s, **300/300 contacts
unrouted**, per-25-insertion windows FLAT at ~26s, ~120 revalidate
calls/insertion, dirty/reval ≈ 5.7 (≈ 4 violators + trial members),
`penetration_intervals_batch` on 100% of revalidate calls (~12,500
interval-members and ~290k estimated parity votes per window), ~13,000
scalar `project` calls per window. Flat windows + all-unrouted = the
poisoned regime: the forest never grows, the cost is the violator tax.

**Are the 49 real defects?** They are exterior-stub axis transits > 0.05mm
outside the sanctioned embed zone — the census class (members 403/357/227)
that motivated the rule, now measured at 49/747 on the production partition.
Whether all 49 are true weld-through-a-fin transits or include an artifact
class (e.g. overlapping-shell seams the exterior-face filter admits) needs
the independent census run on those 49 contacts before anyone relaxes
anything. **This spec does not touch the rule** — it fixes how construction
RESPONDS to a member the rule condemns.

---

## 1. The verified cost model of `construct[p0]`

### 1.1 Trial structure (read from code)

`cco.build` = one `_insert_terminal` per contact (747 full-scale). Per
insertion: `_candidate_builders` materializes up to ~130 builders up front
(plate descent 1 + 7 rings x 12 directions; model anchors ≤ ~40, each with a
scalar `sphere_breach` inside its build_fn, cco.py:494; bifurcation 5 nearest
hosts), then **every builder is trialed**: checkpoint → build →
`forest.revalidate` → rollback; the min-price survivor is re-built,
revalidated, committed. Table fixture measurement: 3,609 revalidate calls
for 41 contacts (~90/insertion after infeasible pruning), construct 0.45s.

### 1.2 Per-revalidate cost (read from `tree._revalidate_exact`)

Dirty set = trial's new members (1–3) + members mutated by the trial + any
persistent violators. Bifurcation trials additionally dirty the **entire
ancestor path**: `_propagate_murray` (cco.py:676) sets `m.r0 = m.r1 = r_new`
with `r_new >= (r0^4 + r_child^4)^(1/4) > r0` — a strict radius increase, so
every ancestor's certificate is void and the whole path is kernel-re-checked
**five times per insertion** (once per bifurcation trial) plus once at
accept. Per dirty member: up to 2 scalar `geom.project` (embed insets,
tree.py:339 — the A18 residual per-item site), a batched `segment_clearance`
share, a midpoint parity vote (clr >= 0.1), and — when the belt triggers
(clr < 0.1 or midpoint inside) — a penetration-interval job costing
10^2–10^3 elementary queries.

### 1.3 The two regimes

- **Poisoned regime (current full scale)**: cost ≈ revalidate calls x
  (violator tax). Removed exactly by A0/A1 below.
- **Healthy regime (post-fix)**: cost ≈ Σ_insertions [ ~90–130 trials x
  (new members) + 5 trials x Murray-path depth ] member-checks, with
  belt-band members (permanent clr < 0.1, common in the treant canopy)
  paying the interval price on every re-check. This is the term A1+A2 are
  designed against; measured healthy-regime windows (probe with the
  poisoning quarantined) in sec 1.4.

### 1.4 Measured windows

Probe N=300, poisoned (as-is):

| window (insertions) | dt s | reval calls | dirty/reval | pen-interval members | est. parity votes | scalar project |
|---|---|---|---|---|---|---|
| 1–25 | 85.6* | 2,636 | 5.9 | 10,860 | 253,828 | 11,334 |
| 76–100 | 26.7 | 3,109 | 5.7 | 12,575 | 294,067 | 13,420 |
| 176–200 | 25.6 | 3,029 | 5.7 | 12,261 | 290,174 | 13,020 |
| 276–300 | 9.0 | 1,033 | 6.0 | 4,132 | 96,069 | 4,132 |

(*first window includes kernel warm caches; late windows shrink because
tail contacts are minima/birth ladders with fewer candidates.) Result:
300/300 unrouted, forest = 300 bare stubs. Full-run extrapolation ≈ 747
insertions x ~120 revalidates x ~50-violator tax ≈ 4x10^8+ queries: the
observed 30–90+ min.

Probe N=300 with the A0 quarantine simulated (pre-existing violations
excluded from trial rejection — healthy regime; construct 318.1s, forest
644 members, 33 unrouted incl. the 4 quarantined):

| window (insertions) | dt s | reval calls | dirty/reval | pen-interval members | est. parity votes | scalar project |
|---|---|---|---|---|---|---|
| 1–25 | 76.3 | 2,655 | 5.9 | 11,220 | 262,240 | 11,712 |
| 76–100 | 26.2 | 3,140 | 5.7 | 12,703 | 297,060 | 13,575 |
| 176–200 | 25.8 | 3,058 | 5.7 | 12,382 | 293,092 | 13,164 |
| 276–300 | 9.1 | 1,079 | 6.0 | 4,316 | 100,347 | 4,316 |

**The decisive comparison**: the healthy run costs the same as the poisoned
run (318s vs 329s, per-window counts within ~2%) — because in BOTH regimes
the per-revalidate cost is dominated by the persistent-violator tax:
pen-interval members/reval ≈ 4.0 and scalar projects/reval ≈ 4.3 — i.e. the
4 condemned stubs, re-checked on every one of ~120 revalidates per
insertion. Fresh trial geometry contributes only ~1.7 dirty members/reval
at this scale. Scale that tax to the full partition (49 violators, ~12x)
x ~9x10^4 revalidates ≈ 4x10^8 elementary queries: the observed full-scale
wall, with or without the poisoning. Consequences for the design:

- **W0 (the tax) is the first-order term** — removed exactly by A1's
  negative-verdict replay (violator geometry never changes), independent of
  whether A0 lands.
- **W2 (the ~120x trial multiplicity) is what multiplies everything** —
  A2 cuts it to ~1–5 revalidates/insertion.
- **W1 (Murray-path re-checks) is second-order at N=300 depth** but is the
  only term that GROWS with forest depth (bifurcation-heavy late stages,
  rewire/bracing on the ~1625-member forest); A1's Lipschitz skip bounds it
  to cache lookups.
- The A18 residual scalar `project` (~13k/window) tracks the dirty set 1:1
  and disappears into the same caches/batches (A1/A3).

---

## 2. CLASS A — exact-preserving redesign (always on)

### 2.0 A0 — stop the poisoning (bug fix; the only behavior-visible item)

Two parts, both fixture-neutral (no fixture has a persistent violator):

- **A0a — pre-existing-violation snapshot in `_insert_terminal`** (the
  bracing.py:865–892 pattern, ported): snapshot the violating member set
  before the candidate loop; a candidate is rejected only on violations
  NOT in the snapshot, and the accept-step
  `assert not violations` becomes `assert no violations beyond the
  snapshot` for the same reason. A candidate that does not touch a
  condemned stub can no longer be vetoed by it.
  **[ADJ upheld — geometry-sensitive snapshot]**: the snapshot is keyed
  `(member_idx, geom_version)`, not `member_idx` alone. A pre-existing
  violator that a trial MUTATES (e.g. a bifurcation host) and that still
  violates is then a NEW violation, not a waived one — filtering by bare
  idx could conceal a candidate-caused or candidate-worsened violation.
  (Today's condemned set is stubs only and stubs are not bifurcation
  hosts, but the version key makes the fix invariant rather than an
  accident of current data.)
- **A0b — honest quarantine of condemned stubs**: a contact whose own stub
  is a persistent through-transit violator is unroutable-by-construction
  under the rule (any support built on that stub would weld through the
  feature). Detect once in a pre-pass (create stubs → one revalidate →
  failures), mark those contacts unrouted immediately, and skip their
  insertion entirely.
  **[ADJ upheld — do not repurpose `mark_dead`]**: `mark_dead`'s contract
  (tree.py:269) requires the caller to have folded the member's
  geometry/load into a survivor first; quarantine does not. A0b instead
  adds a distinct `forest.quarantined: set[int]` state with the same
  exclusion semantics (excluded from live_member_lists / verify /
  emission / `assert_all_certified`), so no downstream `dead`-set
  assumption is silently violated. The certification invariant —
  "nothing is ACCEPTED OR EMITTED with a void certificate" — is
  untouched, because quarantined members are never emitted.
  **[ADJ upheld — load accounting]**: quarantining a contact removes its
  support but does NOT redistribute its carried surface load to
  neighboring contacts (their tips were sized pre-`cco.build` in the
  partition). This is declared, accepted QUALITY loss, not a correctness
  hole: the load partition never reached those contacts' supports in the
  baseline either (it crashed), coverage remains gated by verify, and
  the per-contact carried area of every quarantined contact is reported
  in the run log so the census cross-check (sec 0 / 2.5.5) can rule on
  each. Repartitioning/transfer, if ever wanted, is rule-side rework and
  its own spec.

**Classification honesty**: A0 is NOT bit-identical to the baseline at full
scale — the baseline's full-scale output is "everything unrouted, then
AssertionError". A0 changes output only on inputs where the baseline
degenerates to that crash; on every input where the baseline succeeds
(both fixtures, any mesh with zero persistent violators) it is bit-identical
(the snapshot is empty, the quarantine is empty). This needs the owner's
explicit sign-off as a bug fix, not smuggling; the 49 quarantined contacts
are reported in the run log and `unrouted`, and the census cross-check of
sec 0 decides whether any of them deserve rule-side rework later.

### 2.1 Dirty-set definition — derived from the clearance semantics, not guessed

**Claim: the correct post-placement dirty set is exactly the set of members
whose own geometry tuple changed — the spatial invalidation neighborhood of
new geometry is EMPTY.**

Derivation: `_revalidate_exact` certifies member m by a predicate
`V(g_m; mesh, cfg)`, `g_m = (p0, p1, r0, r1, kind, embed_end)`. Every
quantity it computes — embed insets (`geom.project` at m's own endpoints),
segment clearance (m's own frustum vs the mesh BVH), midpoint parity, axis
penetration intervals — reads only m's fields and the **immutable** mesh.
Member–member proximity is not checked by revalidate (by design: forest
members may touch; joints/balls own junction geometry), and the mesh never
changes during a run. Hence:

> **Locality lemma (scoped).** For the member-vs-mesh predicate evaluated
> by `_revalidate_exact` ONLY: a placement can change member n's verdict
> only by mutating n's own validation inputs. The correct dirty set is
> the MUTATION CLOSURE of the placement — every member whose tracked
> fields were written (new members, the bifurcated host, and the whole
> Murray-propagated ancestor path, which is O(tree depth) and
> geometrically unbounded; sec 1.2 W1 already counts it) — and the
> EUCLIDEAN invalidation neighborhood of the new geometry is empty.
> `revalidate(dirty)` ≡ `revalidate(whole forest)`, exactly, PROVIDED
> every validation-relevant field is mutation-tracked (see the A1a
> tracking amendment below — `embed_end`/`kind` are not tracked today).

**[ADJ] Explicit scope boundary**: the lemma covers the clearance /
through-transit predicate and nothing else. Joint-ball geometry, ball
breach, connectivity, Murray conservation, printability, coverage, and
removal are computed by later global stages (joints.py / verify.py) that
this spec does not incrementalize — they are outside the lemma, not
implied by it. "Correctness" claims in this spec always mean "identical
predicate results feeding identical downstream stages", never "the lemma
proves the hard gates".

So class A needs no geometric dirty radius. The waste is not "unaffected
members re-checked" — the certificate machinery already prevents that — it
is: (W0) condemned members re-checked ~10^5 times (sec 0); (W1) members
fattened by epsilon re-checked from scratch although their verdict provably
cannot flip; (W2) ~130 clearance trials where selection needs only the
survivors' price order; (W3) residual scalar FFI sites; (W4, **[ADJ]**)
`dirty_members()` itself — a full O(N) scan of the forest on every
revalidate call (tree.py:278) — replaced under A1 by an exact incremental
dirty-index set maintained by `_touch_member`/certification (pure
bookkeeping, no verdict change). (Tripwire recorded: any FUTURE
member–member clearance gate voids the lemma and must bring its own
neighborhood design.)

### 2.1a A1a — certificate soundness fix for embed-end members (**[ADJ] upheld blocker; prerequisite for A1**)

Both critiques independently found, and code verification confirms, that
the EXISTING scalar certificate rule — `cert_valid` iff
`max(r0, r1) <= cert_radius` (tree.py:118–122), with `cert_radius` reset
only on POSITION writes (tree.py:94–95) — is **unsound for members with
`embed_end != None`**, in the baseline, before this spec touches anything:

- The sanctioned inset is radius-dependent:
  `contact_depth + (r_end + standoff)/cosang` (tree.py:344). A radius
  DECREASE at the embedded end shrinks the inset → the checked
  sub-segment and the `_axis_interval_illegal` window `[inset0, L−inset1]`
  both GROW → geometry that was never checked becomes load-bearing, while
  the scalar rule keeps the certificate valid.
- Under a nonzero inset, the certified clearance envelope is
  `r_max = max(r_lo, r_hi)` at the INSET endpoints (queries.rs
  `seg_clear_one`), which can be strictly below `max(r0, r1)`; a later
  taper redistribution that stays under the scalar `cert_radius` can
  raise the true envelope above what was checked.
- Radius decrease with fixed positions is REACHABLE on embed-end members:
  `rewire._slim_ancestors` / `_reconcile_ancestors` set `r0 = r1 = r_new`
  with unchanged endpoints; `transient.resize_forest` resizes downward and
  its docstring explicitly relies on "checked fat covers thin"; the
  bifurcation `upper` piece INHERITS `host.embed_end` (cco.py) and becomes
  an ancestor, i.e. sits on exactly those slimming paths.
- For members with `embed_end is None` the scalar rule is EXACTLY sound
  (insets identically 0 → domain fixed and envelope = `max(r0, r1)`;
  kernel checks a uniform max-envelope, not pointwise taper — see
  Appendix A, R1).

**Fix (ships before/with A1, which must not inherit the hole):**

1. Certificates on `embed_end != None` members are voided by ANY tracked
   radius write (not only increases): `__setattr__`'s `_R_FIELDS` branch
   resets `cert_radius = -1.0` when `embed_end` is set. The scalar
   fast-path survives untouched for the (vast majority) non-embed members.
2. `embed_end` and `kind` become invalidating writes: mutating either
   bumps `geom_version` and voids `cert_radius` (today `embed_end = None`
   in cco.py rides a coincident `p1` write — a call-site accident, now an
   invariant).
3. A1's cache reuse for embed members requires the DOMAIN/ENVELOPE test:
   reuse only if new checked domain ⊆ certified domain
   (`i0' >= i0_c and i1' >= i1_c`) AND new envelope `r_max' <= r_max_c`,
   with `(i0_c, i1_c, r_max_c)` stored in the cached clearance entry.
   Anything else → full kernel path.

**Classification honesty (same category as A0)**: this is a BASELINE bug
fix. On any input where a slimmed/resized embed-end member's grown window
actually contains an illegal transit, the baseline silently emits it and
the fixed engine detects it — output may differ exactly there, i.e. only
where the baseline was wrong. Fixture bit-identity is still expected
(gate 2.5.1 verifies, and the fuzz suite gains embed-shrink cases —
5.1.4); needs the owner's explicit sign-off alongside A0.

### 2.2 A1 — the geometry-keyed verdict cache (kills W0's queries and W1)

One cache per (Forest, geom) pair — stored on the Forest exactly like
`_pitch_bound_cache` (strong `is`-keyed geom ref, dropped on pickle), used
only inside `Forest._revalidate_exact`. All keys are **content keys** —
`(p0.tobytes(), p1.tobytes())` for axis-keyed entries plus exact float
radii/insets where they matter. Member identity is never a key: a
rolled-back-and-rebuilt candidate with identical bytes hits; any position
mutation is a different key (Member.__setattr__ copies arrays on write, so
keys cannot alias a mutated buffer; keys snapshot via tobytes).

**[ADJ] Key completeness**: (a) every cache is PARTITIONED by predicate
kind (stub vs non-stub) — identical axes have different validation
semantics by `kind`; (b) `embed_end` (which end, or None) is part of every
clearance/negative key — it selects where insets apply; (c) the cache
identity includes a CONFIG FINGERPRINT
`(standoff_mm, contact_depth_mm, min_illegal_len=0.05, majority_step,
vote cap)` captured at cache creation, and `_revalidate_exact` asserts it
still matches on every call — `SupportConfig` is mutable and a mid-run
config change must hard-fail rather than silently replay stale verdicts.

Cached pure functions (purity by the Locality lemma + kernel reads):

| entry | key | value | note |
|---|---|---|---|
| projection at embed end | `p_embed.tobytes()` | `(dist, cp, ti, n)` | mesh immutable → never invalidated |
| midpoint parity | axis key | bool | idem |
| axis intervals | axis key | interval list | **radii unused in axis mode**: queries.rs:543–544 casts only the axis ray for `rays="axis"`; r0/r1 feed offset rays only under `rays="all"` |
| stub p1 parity | `p1.tobytes()` | bool | idem |
| clearance result | axis key | `(r0_c, r1_c, i0_c, i1_c, clr_c, argp_c)` | last exact kernel result for this axis; superseded on re-check |
| **negative verdict** | axis key + exact (r0, r1, i0, i1) + kind/embed_end partition | violation record TEMPLATE (min_clearance_mm, at_point) — **member_idx is NOT cached** | **[ADJ]** `member_idx` is identity, not geometry: two members with identical bytes (mirror symmetry, reused checkpoint indices) share the key, so the record is re-stamped with the CURRENT member's idx at replay — caching it verbatim would report the wrong member and could defeat A0a's snapshot filter. The geometry-valued fields replay byte-identically; a persistent violator costs ZERO kernel queries after its first check, whether or not A0 lands |

**Skip rule** for a dirty member with current radii/insets against a cached
clearance result (kernel formula: `clr = best_d(sub-segment) − (r_max +
standoff)`, `seg_clear_one`, with `r_max` the max of the linearly-tapered
radius at the two inset endpoints):

```
Δr    = max(0, r_max'(new insets) − r_max_c)     # exact IEEE arithmetic
slack = clr_c − Δr                               # conservative lower bound on clr'
```

Bound validity: insets are monotone nondecreasing in r_end
(`(r_end + standoff)/cosang`, cosang from the cached projection) and a
larger inset shrinks the checked sub-segment, so `best_d` is nondecreasing
in the insets; hence `clr' >= clr_c − Δr` whenever `i0' >= i0_c` and
`i1' >= i1_c` (always true under Murray fattening). **[ADJ]** The original
justification here — "radius DECREASES never reach the cache because the
existing `cert_radius` rule skips revalidation entirely" — leaned on the
UNSOUND baseline behavior that A1a fixes: for `embed_end != None` members
radius decreases now VOID the certificate and DO reach revalidate, where
the A1a domain/envelope test governs (`i0' >= i0_c and i1' >= i1_c and
r_max' <= r_max_c` → reuse; any shrink of either inset, or envelope
growth, → full kernel path). **[ADJ] Numerical conservatism**: `slack` is
compared against the belt gate with a guard — the skip requires
`slack >= 0.1 + GUARD` with `GUARD = 4 * ulp-scale` (implemented via
`math.nextafter` stepping), so IEEE rounding of `Δr`/`slack` can never
round a true `clr' < 0.1` case past the gate; the property tests straddle
the guard from both sides.

- `slack >= 0.1`: the full path would find clr' >= 0.1 → no clearance
  violation; belt fires only if the cached midpoint parity is True; belt
  inputs (cached axis intervals, L, NEW insets) are reproduced exactly in
  Python (`_axis_interval_illegal` arithmetic). If the belt says LEGAL →
  certify — **zero kernel queries**. If ILLEGAL → take the full kernel path
  (a belt violation records the true `min_clearance_mm`).
- `0 <= slack < 0.1` or `slack < 0`: full kernel path (belt gating or a
  real violation may depend on the true clr; violations must carry true
  clr/argmin).

> **Identity theorem.** With the cache, `_revalidate_exact` returns a
> byte-identical violations list and leaves byte-identical certificate
> state, for every call in every sequence. *Sketch*: case split on the
> uncached outcome of a skipped member — (i) plain pass: skip reproduces
> "certify, no record"; cert fields (`cert_version=geom_version`,
> `cert_radius=max(r0,r1)`) are computed from the member, not the kernel;
> (ii) belt evaluated from cache: its only inputs are pure functions of the
> axis (kernel reads no radii in axis mode) plus new insets — identical
> arithmetic; any ILLEGAL outcome falls back to the kernel so the record
> matches; (iii) anything short of a proven pass: full kernel path,
> trivially identical. Negative-verdict replay: geometry bytes equal ⇒ the
> kernel is deterministic (bit-invariant per SPEC-RUST-KERNEL sec 4) ⇒ the
> recorded result IS what a re-query would return (identity re-stamped per
> the [ADJ] note above). Processing order (dirty = member-index order;
> stubs-then-segments assembly) is unchanged. ∎

**[ADJ upheld — identity scope vs the rewire wall-clock cap.]** The
theorem above is per-call. It does NOT extend to full-pipeline bit
identity on runs where rewire hits `cfg.rewire_time_cap_s` (rewire.py:872,
885): rewire calls `forest.revalidate` at 8 sites, so A1/A3 make it
FASTER, and a faster rewire evaluates more moves before the same 30 s
wall — different topology, different output, even though every verdict is
exact. Note the baseline is not run-to-run reproducible on cap-hit runs
either (wall-clock is inherently host/load-dependent); class-A identity
was never attainable there. Resolution, binding:

- Class-A bit-identity claims are scoped to construct/bracing and to
  rewire runs that CONVERGE BEFORE the cap. Fixture gates (2.5.1/5.1.2)
  additionally assert `cap_hit == False` on both sides so fixture
  identity is meaningful, not lucky.
- Any bit-compare involving rewire at treant scale (and the 2.5.5
  acceptance run, if compared across engine versions) must run with the
  B5 deterministic work budget enabled ON BOTH SIDES — B5 is thereby a
  PRECONDITION for scale-level identity evidence, not merely an optional
  quality lever. Enabling B5 by default remains an owner decision (it is
  a behavior change vs the wall-clock baseline).

Cache size: ≈ distinct geometries tried ≈ 3x10^5 entries full-scale.
**[ADJ]** The earlier "tens of MB" estimate is withdrawn as unmeasured —
Python dict entries with bytes keys, float boxes, and interval lists
plausibly run 0.5–1 KB each (i.e. potentially a few hundred MB at
full scale). Requirements instead of guesses: measured bytes/entry and
RSS delta reported with the 5.2 probe; the hard cap (1<<20,
clear-half eviction) applies PER CACHE and eviction pause time is
measured; eviction can cause extra kernel calls, never a different
result.

Expected effect: a bifurcation trial's Murray path (depth D) collapses from
D kernel re-checks to D cache lookups + rare fallbacks (members with < 0.1mm
standing slack); the winner's accept re-run becomes all-hits; every
persistent violator's re-check becomes a replay.

### 2.3 A2 — price-ordered trials with exact early exit (kills W2)

Restructure `_insert_terminal`:

1. **Pricing pass** (no revalidate): checkpoint → build → record delta (or
   infeasible) → rollback, for every builder.
2. Sort by `(delta, yield_index)` — materializing the tie-break Python's
   stable `scored.sort(key=t[0])` applies today.
3. **Trial in that order, stop at the first survivor** — it is the
   min-price survivor.
4. Accept (re-build + revalidate + commit) or return False.

> **Identity sketch.** Builders are materialized before any trial
> (cco.py:276) with parameters frozen (KD-cache untouched by trials). Every
> build runs from the same base state via checkpoint/rollback and reads
> forest geometry/loads, never certificate fields → prices are independent
> of trial interleaving. Revalidate's only cross-trial side effect is
> certificate issuance on pre-existing dirty members (persists through
> rollback because certification does not `_touch_member`); the values
> written are pure functions of the member, identical whichever call writes
> them, and the accept-step revalidate runs unconditionally in both
> schemes. Selection: "min over survivors of (delta, order)" ≡ "first
> survivor in ascending (delta, order)". ∎

Expected: revalidates/insertion drop from ~90–130 to 1 + (cheaper
candidates that fail clearance) — typically 1–5; B1 bounds the tail.

**[ADJ] Honesty and hardening notes for A2:**
- A2 kills W2 REVALIDATIONS, not W2 total work: builders still run once
  each in the pricing pass, failed-cheaper candidates are re-built for
  their trials, and the winner is built a third time at accept. Builder
  Python (Murray propagation, volume arithmetic, cone checks) is repeated;
  the win is that validation — the measured dominant cost — collapses.
  The 5.2 probe reports builder time separately so this trade is measured,
  not assumed.
- The identity sketch's assumptions become EXECUTABLE assertions in the
  fuzz suite, not prose: every price is asserted finite and non-negative
  before the sort (a NaN delta would make sort order — hence the winner —
  undefined; today's `scored.sort(key=lambda t: t[0])` has the same latent
  exposure), and the cache/band telemetry is asserted result-inert.
- Post-A2, revalidate batches shrink to 1–5 members; kernel batch/Rayon
  overhead at tiny batch sizes is re-measured rather than assumed
  beneficial (5.2).

### 2.4 A3 — batch consolidation of residual scalar sites (kills W3)

1. `tree._embed_inset_arc` / verify.py:61: gather all uncached dirty embed
   endpoints into ONE `geom.project` batch per revalidate; inset arithmetic
   stays in Python. (With A1, steady-state gathers are empty.)
2. `cco._model_anchor_builders`: ONE `sphere_breach` batch for all anchor
   candidates before yielding builders (r_strut is computable per candidate
   up front from r_i, carried_i, L, lean); build_fns read precomputed
   verdicts — same `_InfeasibleCandidate` behavior. (Measured: 410 scalar
   calls on the table construct, 1,000/window on the treant probe.)
3. Same gather pattern for bracing.py:806 rung-foot breaches and rewire's
   per-candidate scalar sites (each checked against the immutable mesh
   only, so no data dependence blocks the batch).

### 2.5 Acceptance gates (in order)

1. `table` + `toed_slab` full-pipeline outputs **bit-identical** to
   v5-rust-kernel @ 3a31fc4 (frozen `fixtures_v5_baseline.json` suites +
   an explicit A/B serialization compare of members/balls/volume/verdicts).
2. Full test suite green (kernel parity + baseline suites included).
3. Measured query-count reduction on the probe (N=300, healthy regime):
   revalidate calls, member-checks, pen-interval members, parity-vote
   estimate, project calls, wall — before/after. Target >=10x member-check
   reduction; report honestly whatever is measured.
4. Treant-scale exactness control: with A0b enabled on BOTH sides, run
   construct N=300 with A1+A2+A3 enabled vs disabled (debug kill-switch
   `perf_exact_opts_disable`, default False, exists only for this gate and
   the fuzz suite) → **bit-identical forest bytes**. (The full-scale
   BASELINE cannot be the comparator — its full-scale output is the sec-0
   crash; fixtures cannot see treant-scale-only divergence; this control
   can.)
5. Full-scale treant construct completes; unrouted == the quarantined 49 +
   legitimately unroutable; hard gates zero; independent census over the
   emitted forest (the standing SPEC-RUST-KERNEL sec 8 backstop) — plus the
   census cross-examination of the 49 condemned contacts (sec 0).

---

## 3. CLASS B — approximation levers (each flagged, ALL DEFAULT OFF)

Shared rules: every lever is a `config.SupportConfig` field defaulting OFF
(None/0); deterministic by construction (no RNG, index tiebreaks,
thread-invariant kernel); hard gates stay zero on fixtures with the lever ON
or the lever is rejected; each lever ships a sec-5.3 measurement row so the
owner decides per lever. None interacts with class-A correctness: they only
restrict which candidates/contacts get considered.

### B1 `perf_beam_k: int | None` — candidate beam pruning
- Mechanism: after A2's pricing pass, only the k cheapest candidates (by
  `(delta, yield_index)`) are eligible for clearance trials; if all k fail,
  the contact is honestly unrouted.
- **[ADJ corrected]** An unrouted contact is NOT rescued by the fix pass —
  the opposite: engine.run EXCLUDES an island with any unrouted contact
  from `_add_fix_contacts` entirely (engine.py:373–377, "would just
  accumulate more unroutable contacts"). Beam-induced unrouting therefore
  suppresses refinement of that island. This raises B1's quality stakes:
  the 5.3 row must report `n_unrouted` and uncovered islands, and a lower
  `total_ml` is NOT counted as improvement unless routed load and coverage
  margins are held (dropped supports trivially save resin).
- Speedup: bounds worst-case trials/insertion at k (A2 alone is unbounded
  when many cheap candidates fail in dense canopy — the treant's hard
  contacts).
- Probe values: k ∈ {4, 8, 16}.

### B2 `perf_anchor_cap: int | None` — model-anchor candidate cap
- Mechanism: truncate struts' ~40-candidate list to the nearest `cap`
  before ray-exactification and builder creation.
- Speedup: fewer builds/prices/cast_rays/sphere_breach elements (helps even
  with A2, which must build everything to price it).
- Quality cost **[ADJ widened]**: the ordering is by distance to the
  ORIGINAL face centroid, before ray exactification — a nominally farther
  centroid can ray-hit a nearer surface and be the cheapest or only legal
  exact anchor. Losing it can mean plate routing, bifurcation, unrouting,
  or a different scar location, not only "plate descent, usually fatter".
  The 5.3 row reports routing-family shifts, not just volume.

### B3 `perf_seed_spacing_factor: float | None` — coarser demand seeding with gate-driven refinement
- Mechanism: multiply placement sampling target spacing by f > 1 in
  `prepare` — SEEDING ONLY; `cfg.target_spacing_mm` as consumed by the
  coverage gate is untouched (**[ADJ]**: a factor that leaked into the
  gate would loosen coverage itself — forbidden).
- **[ADJ corrected] The refinement backstop is weak and must be described
  honestly**: there are exactly 2 engine fix passes (engine.py:311),
  `_add_fix_contacts` adds ONE contact per still-uncovered island per
  pass, islands with any unrouted contact are excluded from refinement,
  and each pass re-runs construction. Coarse seeding leans on that narrow
  mechanism; expected speedup is NOT linear in contact count once extra
  fix passes rebuild, and residual uncovered islands are a live outcome —
  measured per 5.3 with the ungated verify report.
- Speedup: fewer insertions into a smaller forest (f≈1.4 → ~2x fewer
  seeds by area); end-to-end factor is measured, not derived.
- Quality cost: higher per-support carried load (fatter tips where gates
  permit); coverage margin thins toward thresholds.

### B4 `perf_fix_iter_caps: (int, int) | None` — convergence caps (**[ADJ] reclassified: potentially gate-affecting**)
- Mechanism: lower engine fix passes (2) and brace_and_size's internal loop
  (3) to the caps.
- **[ADJ]** This lever can leave residual UNCOVERED ISLANDS — a hard-gate
  failure, not a resin-quality trade — and engine.run does NOT block on a
  failed verify report (the fix loop merely stops; `report` is returned,
  engine.py:368). The prior mitigation text ("certificate assertion still
  crash-guards corruption") was misleading: certification guards member
  clearance only, not coverage/printability/structure. Acceptance for B4
  therefore requires a mechanical emission gate: the run harness refuses
  to emit (or emits with a hard `GATES-FAILED` marker the pipeline
  treats as fatal) whenever `verify.ok` is false. Without that gate, B4
  is REJECTED as correctness-affecting under owner ruling 1.
- Speedup: removes whole polishing stages. Quality cost: the brace fixed
  point may stop a rung short of optimal resin.

### B5 `perf_rewire_work_budget: int | None` — deterministic rewire budget
- Mechanism: replace rewire's wall-clock cap with a deterministic
  move-evaluation budget (the stage-2 precondition already endorsed in
  SPEC-RUST-KERNEL sec 10 — retiring a known nondeterminism source, not
  adding one). **[ADJ]** The budget unit is defined precisely as ACCEPT-
  LEVEL MOVE EVALUATIONS (one candidate reattach/prune trial, i.e. one
  checkpoint/build/revalidate/rollback cycle), counted where the cap
  check sits today — a top-level move count alone would not bound cost
  tightly, so the 5.3 row also reports revalidate calls under the budget.
- **[ADJ corrected]** The comparison baseline is the WALL-CLOCK-CAPPED
  sweep (30 s default), not an unbounded one; a deterministic budget may
  do more or less work than the cap did on any given host. Quality delta
  is measured against the capped baseline on the probe host.
- Scope note: rewire, not construct — included because the same trial
  machinery and A1 cache serve rewire, and because sec 2.2's identity
  scoping makes B5 a PRECONDITION for any scale-level bit-compare that
  includes rewire.

---

## 4. Progress ticker (owner-mandated; task #17)

- `cco.build` already maintains `n_inserted`; expose it via a plain
  attribute cell (monotonic int, single writer — no locks needed on
  CPython for a reader-only thread).
- A `_ticker(label, get_n, total)` context manager (engine.py, next to
  `_stage`) starts a daemon thread: `Event.wait(5.0)` loop printing
  `[v5] .. construct <n>/<total> contacts` to stderr (flush=True),
  Event-signaled shutdown + join on stage exit.
- Byte-identity with reporting on/off: the hot path's only work is the
  increment that already exists; the ticker thread reads and prints —
  no kernel calls, no allocation on the compute path, stderr only.
- Same helper later wraps bracing sweeps and rewire move loops.
- Test: construct with ticker on vs off → identical forest bytes; a slowed
  run (test-side monkeypatched builder delay, never production code)
  asserts at least one ticker line on stderr.

---

## 5. Test plan

### 5.1 Class-A bit-identity gates (blocking)
1. Frozen-fixture suites pass unchanged (`fixtures_v5_baseline.json`
   encodes v5-rust-kernel outputs for table/toed_slab).
2. Explicit fixture A/B: run both fixtures at 3a31fc4 and v5-perf,
   serialize (members p0/p1/r0/r1/kind/owner/embed_end, balls, volume,
   verify verdicts, unrouted) via `tobytes()`, assert byte equality.
3. Treant-scale exactness control (sec 2.5 gate 4) via
   `perf_exact_opts_disable`.
4. Property/fuzz (`test_perf_cache.py`):
   - randomized small forests + synthetic meshes: cache-on vs cache-off
     revalidate → identical violations + cert fields across randomized
     mutate/rollback sequences, including Δr straddles of the 0.1 belt
     gate (and its [ADJ] rounding guard) and of 0 clearance, and
     negative-verdict replay;
   - **[ADJ] A1a certificate soundness**: embed-end radius SHRINK whose
     grown window contains a planted illegal transit → detected (baseline
     missed it); taper redistribution under the old scalar max on an
     embed member → re-checked, not skipped; `embed_end`/`kind` mutation
     without a coincident position write → certificate voided and cache
     key changed;
   - **[ADJ] identity vs geometry**: two members with byte-identical
     geometry at different indices sharing a cached negative verdict →
     each replay carries its own member_idx; A0a filter still rejects the
     new one when only the old one is snapshotted;
   - **[ADJ] cache lifecycle**: config-fingerprint mismatch → hard
     failure, not a stale hit; forced eviction (tiny cap) → identical
     results with extra kernel calls;
   - early-exit `_insert_terminal` vs full-trial reference → identical
     winner and forest bytes on randomized candidate sets (prices
     asserted finite);
   - key hygiene: p0/p1 mutation through Member.__setattr__ →
     new key (no stale hit); rollback → old key hits again (no in-place
     element writes exist in the codebase — Appendix A R3 — and arrays
     assigned via __setattr__ are marked non-writeable as a tripwire);
   - A0a snapshot: synthetic persistent violator + healthy candidates →
     candidates accepted, violator still reported by the run log; a
     violator MUTATED by a trial and still violating → rejected (version
     key); A0b: condemned stub → contact unrouted, member quarantined,
     certification assertion passes, live lists exclude it.
5. Determinism: construct twice and under GEOM_KERNEL_THREADS ∈ {1, 8} →
   identical forest bytes (cached values are exact kernel outputs, so
   thread-invariance carries through).

### 5.2 Class-A performance evidence (reported, not gated)
`manual_perf_probe.py` N ∈ {150, 300, 600}: windows of revalidate calls,
dirty/reval, member-checks, segment_clearance elems, pen-interval members,
parity-vote estimate, project calls, wall — before/after on the same host,
threads pinned, contention disclaimed; query counts are the metric.
**[ADJ] added columns**: end-to-end wall + CPU utilization, RSS / peak RSS
and cache bytes/entry, cache-key construction time, eviction pause time,
dirty-scan (W4) time, builder time vs validation time, rewire cap/budget
hits. Two probe caveats recorded: the parity-vote column is a
one-subsegment LOWER-BOUND estimate (manual_perf_probe.py:77), and the
"healthy-regime" probe mode filters quarantined violations after paying
their queries — it simulates A0a's poisoning removal, NOT A0b's cost
removal, so A0b's savings are measured only by the real implementation.

### 5.3 Class-B measurement protocol (per lever, before any enable decision)
On table + toed_slab + treant-N300 (healthy regime), per (lever, value):
two runs bit-identical (determinism); hard gates zero **[ADJ] as reported
by the UNGATED full `verify` report (clearance_violations, uncovered
islands, printability, connectivity, murray) — NOT merely the cert-based
construct asserts, which fixture-scale runs cannot stress and which the
A1a class of defect evades**; quality delta (total_ml, member/ball counts,
n_unrouted, uncovered islands/births) vs lever-off; speedup (stage wall +
query counts) vs lever-off. The treant-N300 rows for any lever proposed
for a real full-scale run additionally get the independent census as the
final word. One row per (lever, value); nothing enabled by default in any
committed config.

### 5.4 Regression tripwires
- Fixture-scale call-size histogram (standing A18 obligation) re-run:
  project/sphere_breach counts on the table construct drop to O(1) batches;
  generous non-flaky thresholds asserted in a test.
- Cache telemetry (entries, hits, misses, fallbacks) + quarantine count
  printed with the construct stage line (result-inert).

---

## 6. Risk register

| # | Risk | Exposure | Mitigation |
|---|---|---|---|
| P0 | A0 mis-scoped: quarantining a stub that a candidate could legally route, or letting a candidate-caused violation slip through the snapshot | correctness of the fix itself | Snapshot keyed by member_idx of PRE-trial violators only; quarantine only for a contact's OWN condemned stub; fuzz 5.1.4; census cross-check of the 49 before acceptance |
| P1 | Cache-key aliasing (ndarray mutated after keying) | stale verdicts | Keys are `tobytes()` snapshots; Member.__setattr__ copies arrays (tree.py:88); fuzz mutates through every path incl. rollback |
| P2 | Lipschitz/monotone bound wrong in an edge (inset interplay, taper, L<1e-9) | silent wrong verdict | Bound only ever skips toward "pass with margin"; every potential violation and every belt-ILLEGAL outcome takes the full kernel path; degenerate lengths bypass the cache; property tests straddle both thresholds |
| P3 | Early-exit divergence via an unmodeled side effect (cert persistence, band counters) | bit-identity broken at scale | 2.3 proof + treant exactness control 2.5.4; band/tie counters are result-inert by spec (A19) and excluded from identity |
| P4 | Cache memory growth | GBs on adversarial churn | Hard cap + clear-half eviction (correctness-neutral by construction); telemetry |
| P5 | Healthy-regime core still too slow at full scale | construct still slow post-fix | B1–B3 are the sanctioned dials; beyond them, stage-2 port of pricing/trial per SPEC-RUST-KERNEL sec 10 (A1/A2 designed as pure lookups + serial control flow so they port) |
| P6 | Class-B lever interactions | quality cliff when combined | Levers measured one at a time; any combined enablement re-runs 5.3 for the combination |
| P7 | Ticker thread interleaves/outlives its stage | cosmetic / dangling thread | Single-line atomic prints; Event shutdown + join in finally |
| P8 | Probe numbers skewed by host contention (two live full-scale runs) | misleading cost model | Query counts primary; wall secondary with disclaimer; ratios carry the argument |
| P9 | rewire/bracing position-mutating moves vs the cache | stale verdicts after slide/reattach | Content keys: any p0/p1 change is a new key by construction — there is no invalidation protocol to get wrong; fuzz includes slide/reattach-shaped mutations |
| P10 | A future member–member clearance gate voids the Locality lemma | class-A proof void | Lemma + tripwire recorded in 2.1; such a gate must bring its own neighborhood dirty-set design |
| P11 | The 49 condemned stubs include an artifact class (e.g. overlapping-shell seams) → quarantine silently drops supportable contacts | coverage quality | Census cross-examination of all 49 required at acceptance (sec 0/2.5.5); quarantine list in the run log; any rule-side change is its own spec |
| P12 **[ADJ]** | Baseline scalar-certificate unsoundness on embed-end members (radius shrink grows the checked window; taper redistribution under an inset raises the true envelope) | silent missed transit, pre-existing | A1a fix (void cert on any embed-member radius write; domain/envelope reuse test; embed_end/kind become invalidating writes); fuzz 5.1.4; owner sign-off as a bug fix |
| P13 **[ADJ]** | Rewire wall-clock cap makes "faster ⇒ different output" on cap-hit runs | class-A identity claim overreach | Identity scoped to cap-free runs; fixture gates assert cap_hit False; B5 deterministic budget mandatory for any scale-level bit-compare that includes rewire |
| P14 **[ADJ]** | B4 (or any lever) terminates with verify.ok False and the pipeline emits anyway | hard-gate failure reaches the printer | Mechanical emission gate on verify.ok in the run harness — required for B4 acceptance, recommended globally |

---

## 7. Implementation order (small, separately verifiable steps)

1. A0a snapshot + A0b quarantine (+ fuzz + run-log reporting) — unblocks
   full scale; everything after this is measurable in the healthy regime.
2. **[ADJ]** A1a certificate-soundness fix (embed-member invalidation +
   embed_end/kind tracking) — a small, separately sign-off-able bug fix
   that A1 depends on.
3. A3 batching (embed-inset gather; anchor sphere_breach batch) —
   independently bit-identical.
4. A1 cache incl. negative-verdict replay + incremental dirty-index set
   (W4) (+ fuzz suite).
5. A2 price-ordered early exit (+ equivalence fuzz).
6. Ticker (sec 4).
7. Probe before/after report (5.2) + treant exactness control (2.5.4) +
   full-scale acceptance run (2.5.5).
8. Class-B levers behind flags, one commit per lever, each with its 5.3
   row (B5 first — it is the identity precondition for scale
   bit-compares; B4 last, only after the emission gate exists).

---

## Appendix A — Adjudication 2026-07-23: rejected challenges (with code evidence)

Both critiques were verified claim-by-claim against the worktree before
ruling. Upheld findings are integrated above with **[ADJ]** markers.
The following challenges were REJECTED or materially narrowed:

- **R1 (Codex) — "a scalar maximum cannot certify a tapered profile"
  in general**: REJECTED for `embed_end is None` members. The kernel
  does not check pointwise taper: `seg_clear_one` (queries.rs) computes
  ONE uniform envelope `r_max = max(r_lo, r_hi)` over the checked span
  and thresholds `best_d − (r_max + standoff)`. With insets identically
  0 (non-embed), the checked domain is the full span and the checked
  envelope equals `max(r0, r1)` = `cert_radius`; any later profile with
  a smaller scalar max is pointwise inside the CHECKED envelope, so
  Codex's (1.0, 0.2) → (0.2, 0.9) example is safely covered. The claim
  is UPHELD only in combination with radius-dependent insets — folded
  into A1a (sec 2.1a), which confines the fix to embed-end members.
- **R2 (Codex) — "the Locality lemma is false because Murray
  propagation is unbounded"**: REJECTED as a soundness charge. The spec
  never proposed a Euclidean dirty radius that misses ancestors: the
  dirty set is defined by tracked-field mutation, and every
  `_propagate_murray` radius write bumps `geom_version` — sec 1.2 (W1)
  explicitly counts the full ancestor-path re-checks. Upheld only as a
  WORDING fix (lemma now states "mutation closure, O(depth),
  geometrically unbounded" and its predicate-only scope, sec 2.1) plus
  the real tracking gap (`embed_end`/`kind`) folded into A1a.
- **R3 (Codex) — "in-place `m.p0[i] = ...` bypasses tracking"**:
  REJECTED as a current defect. A repo-wide grep for `.p0[...] =` /
  `.p1[...] =` element writes finds ZERO sites — every occurrence is a
  coordinate read; all mutations go through `Member.__setattr__`, which
  copies on assignment (tree.py:87–88). Accepted only as future-proofing:
  arrays are additionally marked non-writeable on assignment (5.1.4
  tripwire), which converts any future in-place write into an immediate
  error instead of a stale certificate.
- **R4 (Codex) — "the A2 claim that certificate writes persist through
  rollback is inaccurate"**: REJECTED. The spec's claim concerns
  certificate issuance on pre-existing dirty members the trial did NOT
  touch — certification writes do not call `_touch_member`, so those
  members are absent from the checkpoint snapshot and their new
  certificates persist. Members the trial DID touch are snapshotted at
  first touch (including cert fields, `_snapshot`/`_restore`) and fully
  revert — consistent with, not contradicted by, the identity sketch.
  The independent Opus verification of A2 (no build_fn reads certificate
  state; feasible set trial-order-independent) stands.
- **R5 (Codex) — "negative verdicts cannot be cached across members at
  all"**: NARROWED, not accepted wholesale. The geometry-valued fields
  (min_clearance_mm, at_point) are pure functions of the content key and
  replay exactly; only `member_idx` is identity. Fixed by caching a
  record template and re-stamping the current idx at replay (sec 2.2) —
  the Opus form of the finding, which also closes the A0a-interaction
  hazard.
- **R6 (both, probed) — ticker risks**: NO FINDING. The hot path's only
  work is the pre-existing `n_inserted` increment; all timing lives in
  the daemon thread's `Event.wait(5.0)`; a single shared counter cell
  spanning both fix passes (sec 4) closes the stale-cell note. CPython
  single-writer int reads cannot tear.
- **R7 (Opus challenge 3, defused by Opus itself; recorded) — batch
  order / float-min sensitivity**: the kernel has no cross-query
  reductions; per-member results are invariant to batch membership and
  ordering, and the violations-list order is consumed only as
  "any → reject" plus idx-membership. A2/A3 re-batching cannot perturb
  results while dirty iteration stays index-ordered (tree.py:278).
- **Not adopted as blockers**: rewire's O(N) `_total_volume_ml` sums and
  other rewire-internal Python costs (Codex sec 8) are real but outside
  this spec's construct scope — recorded as stage-2 / rewire-perf
  follow-up work items, measured by the 5.2 added columns so their
  magnitude is known before anyone claims construct-level speedups
  transfer to rewire.
