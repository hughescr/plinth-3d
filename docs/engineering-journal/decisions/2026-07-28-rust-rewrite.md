# Proposed plan — resin support pipeline

**Status: proposed, awaiting Craig's decision.** Written 2026-07-28.

Synthesised from three independent analyses that were run without sight of each
other's conclusions:

- **Six specialist diagnosis agents** (Claude), each given one subsystem.
- **Codex** (`ANSWERS-FROM-CODEX.md`), which built most of the current system.
- **Fable**, a heavyweight planning pass given all of the above.

All three converged on the **same target architecture**. They differed only on
the *path* to it. That convergence is the main reason to trust the conclusion.

---

## 1. The decision

Precise definitions first, because the three sources use these words differently:

| Term | Meaning here |
|---|---|
| **Refine** | Keep `engine.py` and its control structure; fix bugs in place. |
| **Restructure** | Keep the 40k-line Python codebase as the living substrate and transform it toward the target design, porting hot paths to Rust en route. (Codex's recommendation.) |
| **Restart the decision core** | Write the new core fresh; the old engine is never on the critical path of new code. The verified substrate (~11k lines) is carried forward, and the old engine is kept **runnable and frozen as a measuring stick**. |

**Recommendation: restart the decision core.**

Nobody credible proposes *refine*: 30 of 34 corpus failures are cap exhaustion
in nested retry loops whose repair mechanisms structurally cannot fix the
defects they gate on. There is no bug-fix path out of that.

The live disagreement is restructure-vs-restart, and it is only about path.
Four reasons to reject transform-in-place:

1. **Transform-in-place is the exact process that produced this codebase.**
   Craig's own description — "just fix this quickly" producing bolt-ons and
   worse generations — *is* incremental restructuring under pressure to keep
   40k lines importable and 33k lines of tests green at every step. Eleven
   acceptance predicates is what that process converges to.

2. **The knowledge Codex wants to preserve is mostly knowledge of the wrong
   objective.** Every forest this engine produced was shaped by `r_deflect`
   under a drag force ~385x larger than physical, sized with no bending term,
   and accepted by gates that never test holding. The test corpus locks in
   *that* behaviour. Keeping Python as the "regression oracle" institutionalises
   a tautological oracle. The genuinely hard-won knowledge — serialization,
   clearance semantics, root legality — lives in the keep-list.

3. **The substrate cannot host the target design.** Codex's own loop requires
   re-slicing and re-evaluating every candidate. In an object graph with a
   105-line `__setattr__` dispatch per scalar write, 63 deepcopies in one
   module, and 86% of runtime in settle cycles, that loop is unaffordable.

4. **Bias check.** Codex built most of this. Its recommendation to preserve the
   substrate should be discounted — not dismissed, but this is precisely the
   situation the cross-family review norm exists for, pointed the other way.

### One amendment to Codex's design

Codex's "every geometry mutation gets re-sliced and re-evaluated", taken
literally, is a per-item call in a hot path. Adding a support member to a raster
slice is a monotone OR into a handful of layer masks. The inner loop must be
**incremental** (re-rasterize only the layers/tiles a candidate touches) and
**batched** (evaluate candidate *sets*). This is the difference between a
60-second engine and a 60-minute one.

### Language and budget

New core in **Rust**. Measured justification, not preference: a Python slice
stack of Bat Cat Owl runs ~10 s versus a 55.6 s *failing* engine run; a Rust
raster slicer should be well under 1 s. Rust's immutability-by-default also
makes "just mutate and retry" expensive and the principled fix cheap.

**Explicit performance budget: <=60 s typical model, <=3 min worst case, end to
end.** Current measured: median 402 s, max 872 s — violating "quickly" even
when it succeeds.

---

## 2. Sequencing — build the judge before the generator

Every possible path needs one thing: a trustworthy answer to *"at layer k, does
peeling this area off the film break the already-cured structure below?"*
Nothing in 40k lines computes it. Build that first, standalone, engine-agnostic.

**Oracle contract.** Input: final geometry (model mesh + optional support graph
with semantic labels) + printer profile. Output: per-layer `DefectReport` —
islands, unsupported cantilever increments, minimum-cross-section stress along
each load path to plate, peel margin per region — with evidence.

Engine-agnostic is the point: it must grade *anything* — the old engine's
forests, ChituBox auto-supports, a hand-supported known-good.

**Weeks 1–2 — the oracle (~2 weeks):**
- Rust raster slicer, winding-number fill. This is the trick that makes it
  cheap: it tolerates dirty meshes, makes model+support union a bitwise OR, and
  makes robust contour stitching optional.
- Per-layer connected components + union-find connectivity to plate. Replaces
  the continuous articulated-path search currently rejecting 98.3% of candidates
  with an exact discrete answer.
- Deliberately dumb mechanics MVP: per new-layer region, peel load proportional
  to area with a position-along-peel-front factor (the S4U tilts from a rear
  hinge, so load varies with distance from hinge — a fittable scalar profile);
  transmit load down the component's load path; flag the minimum cross-section
  **in supports and in the model** where moment/area exceeds the calibrated
  envelope. No FEM. Beam-level approximations over the voxel structure.

**End of week 2 — grade before building anything else.** A table: every corpus
model x {old engine's forest, ChituBox auto-support, one hand-supported
known-good} x oracle verdict. Three signals within days:

1. Does the oracle's failure list match Craig's visual defect list (the
   thin-neck cantilevers he already spots)?
2. Does it pass known-good and fail known-bad physical prints?
3. Are the old engine's finished forests physically adequate or not?

**Honest caveat.** Grading only covers forests the engine finishes, and it fails
34/37. That is acceptable: the corpus has already answered the *throughput* half
(30/34 failures are search-structure, not physics), so the oracle only needs to
answer the *physics* half. Note also that after stripping the 16.5x lever-arm
bug and the 8.3x pressure disagreement, the worst terminal sat at utilization
**1.38** — genuinely at the boundary. The current forests might be roughly
adequate. The coupons settle that; no one's opinion does.

**Weeks 3–6+ — the new core.** `Scene`; candidate transforms (porting the
candidate-pool/topology/coverage logic); one controller whose only acceptance
authority is the oracle's defect vector; secondary costs = resin volume, scar
score, removal difficulty — **Craig's actual three goals, which today appear in
zero accept/reject decisions.** That absence is the deepest explanation of why
quick fixes made generations worse: every fix was validated against proxies
uncorrelated with the goal.

Emission through the existing chitubox path. Round 25 byte-for-byte stays as the
emission regression gate.

**Interim rule: do not fix the old engine.** Craig prints via ChituBox manual
supports during the transition, and those become oracle calibration anchors.
Sole exception: a print-blocking emergency, timeboxed to hours, never merged.

---

## 3. Calibration — measure the failure envelope, not the constants

The literature cannot settle green strength and peel pressure separately, and it
does not need to. The oracle needs one thing: **under this printer's actual
peel, at this layer height, what geometry breaks?** That is directly printable.

Design coupons in week 1 (the new slicer emits them — its first real workout);
print in weeks 2–3. Six to ten small plates, a few hours each:

1. **Cantilever comb** — the workhorse. Arrays of horizontal beams, graded
   length x thickness, at 0.05 mm. The (L, t) boundary between survived and
   snapped-during-print *is* the effective green-flexural-strength-under-peel
   envelope: peel load and green strength measured as one product, in situ, at
   the true transient cure state no bench test can reproduce. **This one plate
   replaces both invented pressures.**
2. **Tip pull-off ladder** — pillars with graded tip radii (0.15–0.5 mm)
   contacting an overhang plate. Which tips detach during the print gives tip
   capacity vs radius.
3. **Position and orientation dependence** — replicate the comb at front/centre/
   rear of the plate, and with beams parallel vs perpendicular to the hinge
   axis. Fits the peel-front position factor for the tilting mechanism.
4. **Layer-height replication** — repeat plates 1–2 at 0.10 mm with dose-scaled
   exposure. This is the empirical gate for variable layer height; nothing else
   is.
5. *Optional:* fish-scale pull test on green coupons minutes after print stop,
   for order-of-magnitude sanity only.

Fit a deliberately dumb model form (failure when peel force x lever arm exceeds
k x section modulus; k fitted per layer height and peel-front position; keep
error bars). Control temperature, one resin batch.

**Every production print thereafter is a calibration datum** — log the oracle's
marginal layers against observed defects and re-fit periodically.

---

## 4. Structural discipline against re-accretion

Why accretion was the path of least resistance, mechanistically: **there was no
ground truth, so every failure looked like a miscalibrated gate, and the
cheapest fix was always another predicate, cap, or retry loop.** A gate that
cannot fire creates no pressure for its own removal. A mutable god-object makes
the local mutation cheaper than the design fix. 55 worktrees mean knowledge
lives in half-landed states. 33k lines of snapshot tests make semantic change
expensive, so change flows to wherever tests aren't.

Countermeasures, each structural rather than aspirational:

1. **One acceptance authority, enforced by types.** One `DefectReport` struct;
   the controller is its only consumer; no module outside the evaluator crate
   may return accept/reject. A new check is a new field in the defect vector or
   it does not exist. This makes predicate number twelve a visible architecture
   violation rather than a Tuesday.
2. **Constants carry provenance.** Typed:
   `Measured { coupon_plate_id, error_bar } | Derived | Placeholder`. The build
   fails if any `Placeholder` is reachable from the accept path. `shaft_drag_force_n`
   survived the entire life of this project because invented constants were
   indistinguishable from measured ones.
3. **Cap exhaustion is a defect, not an exit code.** One controller, one budget;
   exhaustion must report *which defect failed to improve*, with the final
   defect vector attached. Nested private caps banned. (`attempts=0 of 8`
   telling you nothing is the current failure mode in one line.)
4. **Worktrees are ephemeral or dead.** Land within days or delete. More than
   ~5 live worktrees is a stop-the-line signal.
5. **Tests assert physics and contracts, not snapshots.** Property tests: every
   accepted forest passes the oracle; emission is byte-stable; slicer masks
   satisfy invariants. Never "the 12-worst-layer table looks like this."
6. **Specs have a lifecycle** — versioned, formally superseded, never silently
   contradicted. The adaptive-layer ban standing in a normative spec while the
   owner plans the opposite is the current exhibit.
7. **Phase reviews hunt accretion specifically** — briefed to flag any new
   acceptance logic outside the evaluator and any constant without provenance.

---

## 5. Variable layer height

Right instinct, wrong moment — and not merely a scheduling matter. **It is
currently unsafe because it moves both sides of exactly the inequality nobody
can compute.** Thicker layers mean fewer peel events (good) but a larger cured
increment per exposure, a wider cantilever staircase step held by a one-layer-older
joint, and longer exposure changing both green strength and film adhesion.

**Architectural resolution, and it is cheap: make layer height a per-layer array
in `SlicedBuild` from day one, and ship v1 running it constant at 0.05 mm.** The
slicer, oracle and data model never assume uniformity. The spec's ban is
formally retired as an *architectural assumption* and retained as a v1
*operational constraint* — which resolves the spec contradiction honestly: the
ban was right for the MVP and wrong as a foundation.

Enabling it later needs only: plate-4 coupon envelopes at each candidate height,
standard cure-depth/exposure calibration, a detail-metric post-pass assigning
heights (a pure function over `Scene`), and a sliced-format writer expressing
per-layer z and exposure — **which is greenfield regardless**, since `.chitubox`
carries no slice data at all.

Worth stating plainly: Craig's *reason* for wanting in-house slicing — observing
islands/overhangs layer by layer and verifying the cured structure below holds
them — is precisely the oracle, and it is the first thing this plan builds.
Variable height is a bonus payload on infrastructure justified without it.

---

## 6. Day-one deletions, and refusals

**Delete on day one** (after one timeboxed salvage day over the worktrees):

- The ~50 stale worktrees and dead branches. Codex confirmed **all 14 named
  branches are DEAD or SUPERSEDED — no merge work is pending anywhere.**
- The three production-dead acceptance/repair modules and their broken tests;
  the superseded-but-still-tested acceptance predicate.
- `verify.load_report` (the gate that cannot fire) and `active_layer_planning.py`
  (its successor is the oracle; keep the spec's lessons and its measured bug list).
- Constants `shaft_drag_force_n`, `peel_pressure_pa`, `DESIGN_PRESSURE_PA` from
  any *new* code path, by the provenance rule. They die with the old core rather
  than being surgically removed — removing them in place would change old-engine
  generations, and we are not fixing the old engine.
- Murray's law as a hard gate anywhere new. If junction smoothing is wanted for
  scarring, it returns later as a *cost term*, never as a strength claim.

**Refuse to delete until specifically proven:**

- `rust/geom_kernel`, `rust/chitubox_io`, the emit path — proven working
  (Round 25 byte-for-byte).
- `hollow.py`, `perception.py`, `mesh_io`, placement's candidate pool,
  `topology`, `coverage` — until a Rust replacement beats each on the corpus. If
  extracting any of them from engine.py's grip costs more than ~2–3 days, wrap
  it behind a subprocess/file contract instead of in-process reuse.
- **The frozen Python engine itself** — until the new core beats it on oracle
  grade across the corpus, on Craig's visual review, and on >=2 physical prints.
  It is the only generator that exists.
- The thirteen legality checks' *semantics* (clearance, removability, root
  legality) — ported as diagnostics and cost terms, never as acceptance
  authority. This is the concrete answer to Codex's relearning concern.
- The model corpus, run harness, and failure telemetry.

---

## 7. Risk register

| # | Risk | Early warning | Mitigation |
|---|---|---|---|
| 1 | Oracle mechanics too crude to discriminate | Week-2 grading doesn't correlate with Craig's visual defect list, or a known-good print is failed | Three-anchor grading set exists to catch this in week 2, not month 3; model form stays dumb-and-fitted so it can be re-fitted |
| 2 | Calibration doesn't converge | Repeat plates disagree by more than one grid step | Worst-case envelope + safety factor beats a precise wrong number; one resin, controlled temperature; replicates are hours |
| 3 | New core re-accretes | Any accept/reject outside the evaluator crate; any constant without provenance; a second retry loop | Section 4 rules + phase reviews briefed to hunt exactly this |
| 4 | Keep-list is optimistic | An extraction exceeding its 2–3 day box | Subprocess/file-contract boundary instead of extraction |
| 5 | Craig can't print during transition; pressure to patch old engine | Any "quick fix" branch lasting more than a day | ChituBox manual supports are the sanctioned interim path and double as calibration anchors; exception rule written in advance |
| 6 | Schedule | Oracle not grading the corpus by end of week 2; week-4 checkpoint slipping | Convergent estimates span 3–16 weeks; honest read ~6–10 for parity-or-better. **Worst case of this plan is still a calibrated, engine-agnostic print-feasibility instrument that grades any supporter's output including ChituBox's** — valuable even if everything after slips |
| 7 | Greenfield sliced-format writer (CTB/Goo are reverse-engineered) | First coupon plate rejected or garbled by the printer | Round-trip through UVtools before anything touches resin; `.chitubox` + manual ChituBox slicing stays the fallback output path |

---

## 8. The one-paragraph version

Don't refine — the corpus proves the failures are architectural. Don't
restructure in place — that is the process that built this, and the substrate
cannot host the target loop that all three sources agree on. Restart the
decision core in Rust around a slice-space "does it hold" oracle built **first**
and calibrated by ~8 coupon plates that measure the failure envelope directly.
Keep the ~11k proven lines and the frozen old engine as the measuring stick.
Encode Craig's three actual goals as the only acceptance authority. Carry
variable layer height as a day-one data-model decision and a post-calibration
feature. Enforce the anti-accretion rules with types and provenance checks
rather than intentions. **First meaningful signal: end of week 2**, when the
oracle grades the existing forests against Craig's own eyes and two physical
prints.
