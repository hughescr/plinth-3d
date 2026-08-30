# Load Model Implementation Plan

Status: IMPLEMENTATION PLAN for the design of record
(`docs/plans/load-model-synthesis.md`, branch `design/load-model-synthesis` @
`942804d`). This document sequences that design into stages that can each land on
`develop` independently. It changes no production behaviour itself, and no stage
before S5 moves any emitted number.

Sizes are relative points on a shared scale (1 = one small bounded change with
obvious tests; points add roughly linearly). They are not calendar time.

| Stage | Name | Size | Emitted output moves? |
|---|---|---|---|
| S0 | The baseline measurements | 3 | No |
| S1 | Registry honesty and drift guards | 1 | No |
| S2 | The demand field, deciding nothing | 8 | No |
| S3 | The shadow ledger and the prediction files | 10 | No |
| G1 | Gate: adversarial review + owner rulings R0–R3 | — | No |
| S5 | Sizing activation: the one flip commit | 8 | Yes — tip radii, refusals |
| G2 | Gate: owner ruling R4 on the post-S5 table | — | No |
| S6 | Placement activation | 10 | Yes — seat counts, positions |
| S7 | The write-time gate | 4 | No (it can only refuse) |
| S8 | Demolition and cap migration | 6 | No (bit-identity required) |

Total ≈ 50 points. Stopping points and what each buys are in §3.

---

## 1. The hard problem: telling an intended change from a regression, mechanically

This change exists to move support output. Tips will get thicker and thinner, seats
will move, counts will change — that is the goal. But the project's standing rule,
enforced all week, is that **a frozen test value that moves is a bug, never a number
to refresh.** Several tests pin exact fingerprints of the support plan today:

- the golden corpus: `corpus/baseline/<model>.pipeline.json` and
  `corpus/baseline/<model>.chitubox` for all 8 manifest models, frozen at a
  recorded git rev with a recorded regeneration command (`corpus/manifest.json`);
- behavioural goldens such as `crates/plinth-verify/tests/coverage_gate_toed_slab.json`;
- the determinism seals — `PreparedSupportDigests`, `snapshot_digest` — asserted in
  `support_runner_integration.rs` and hashed from fields this design deletes;
- structural guards (`contact_regression_gate.rs`, `retired_formula_guard.rs`)
  whose guarded wiring this design rewires.

All of them will move. The question is how to accept the movement without ever
saying "we looked at it and it seemed fine."

### 1.1 The ruling among the four candidate mechanisms

**Prediction-equality is the acceptance mechanism. The other three are adopted in
supporting roles, and none of them is allowed to be the thing that says yes.**

1. **Shadow computation → adopted, and sharpened into a prediction ledger.** The
   new sizing code lands and runs long before it is trusted (S3), computing every
   number the new behaviour would emit while the shipped behaviour is unchanged.
   Its output is not merely "reported": it is written to committed, per-model
   **prediction files**, reviewed at gate G1, and then the activation commit (S5)
   must reproduce those files **exactly**. "Intended" is thereby given a mechanical
   definition: *a frozen value may move only to a value that was already committed,
   in a prediction file, before the behaviour change landed — and the activation
   gate checks the equality by machine.* Any moved value not matching its
   prediction, and any moved value with no prediction, fails the build. No human
   judgment is in the accept path; human judgment happens earlier, at G1, over the
   divergence table, where it belongs.

2. **Golden-corpus comparison with a human-readable diff → adopted as the REVIEW
   artifact, rejected as the acceptance test.** The E2 divergence table (per
   contact: old carried area, old tip radius and diameter, beside new routed
   demand, new radius and diameter, and the reason class for the difference) is
   what Craig and the adversarial reviewer read at G1. A human reading a diff and
   approving is exactly the "seemed fine" failure mode when it is the *last* line
   of defence; as the *first* line, before anything moves, it is the right place
   for judgment.

3. **Per-model invariant checks → adopted as necessary, rejected as sufficient.**
   Invariants I1–I14 hold across the change even though digests do not, and they
   run before, during, and after activation. But invariants cannot distinguish
   *this* intended output from *some other* self-consistent output — many wrong
   changes satisfy conservation and exclusivity. They bound the damage; they do
   not certify the intent.

4. **Staging so only one class of value moves at a time → adopted as the scope
   limiter.** Each stage declares its movable class (S5: tip radii, refusal
   disclosures, and the digests that hash them; S6: seat sets and counts; every
   other stage: nothing). A pinned value outside the declared class that moves is
   a regression **by definition**, mechanically: the activation gate diffs the
   full pinned inventory, not just the predicted subset, and fails on any
   out-of-class movement.

### 1.2 The refreeze protocol, step by step

This protocol amends a standing enforced rule, so it requires Craig's ratification
before first use (ruling R0, §5). Until R0 is granted, no frozen value moves.

1. **Inventory.** S0 commits a machine-readable inventory of every pinned value in
   the tree (path, test name, current value or artifact digest). Tooling, not
   memory: a script that greps the enumerated pin sites and hashes the golden
   files. The inventory is itself a committed artifact; the S5/S6 gates diff
   against it.
2. **Predict.** S3 lands the new computation dark (default-off switch). A tool run
   with the switch on, in a scratch lane, produces
   `docs/evidence/load-activation/predictions/<model>.json`: every value the flip
   will change, including the new golden-artifact digests, keyed by canonical
   typed identifiers (I11), stamped with the base git rev it was computed from.
3. **Review.** G1: the divergence table and predictions are reviewed
   adversarially and ruled on by Craig (R1–R3). The predictions are then FROZEN —
   they are the contract for the flip.
4. **Sabotage the comparator first.** Before the flip is attempted, perturb one
   predicted value by one ulp in a scratch commit and confirm the equality gate
   goes red. A gate never seen red is not a gate; this project has already been
   bitten by a byte-identity proof over a path that never fired.
5. **Flip.** One commit: switch default flips, and every refrozen value is
   *generated by tooling from the prediction files*, never typed. The gate then
   asserts, in the same build: (a) every moved value equals its prediction;
   (b) the prediction base rev is an ancestor of HEAD and no commit since it
   touched a declared load-input file (list committed at S3) — otherwise
   predictions must be regenerated and re-reviewed; (c) no value outside the
   declared movable class moved; (d) all invariants green.
6. **Walk, with git ground truth.** The refreeze is reviewed model by model
   against the phase-2 table — never accepted in bulk — and every claimed commit
   is verified to exist on the branch before anything depends on it.

The determinism seals (`snapshot_digest` and friends) are handled inside the same
protocol but as a distinct category: they are seals over inputs, not behaviour
claims, so their new values are *computed* by the flip tooling rather than
individually reviewed — the review burden sits on the behavioural values the
seals hash, which are all in the prediction files.

---

## 2. The measurement that comes first

Nothing in S1 onward is designed until S0's numbers exist. This project has
repeatedly built the wrong thing from a plausible theory; the synthesis itself
names measurement-before-machinery as adopted discipline. S0 produces four
instruments and one inventory, all committed under
`docs/evidence/load-baseline-<date>/`:

- **B1 — the corpus load census (E0).** Per corpus model: contact count,
  eligible-row count, zero-carry count, carried-area distribution
  (min/median/q3/max), tips at the minimum and maximum. This is the number that
  says how bad today is on real models, not fixtures. Also: extend the
  `prepare_folder.rs:445-520` law assertion to every corpus model as `#[ignore]`d
  evidence and record how many models it reddens — an explicitly argued red,
  documented in the census.
- **B2 — the inversion mechanism separation (E5).** One instrumented run printing,
  per contact, the PREPARED carried area and tip beside the ACCEPTED ones. Decides
  whether the measured 20x inversion is the stale-tip-after-repartition mechanism
  or the positional-id join at `specialized.rs:1611`. Rule R makes both
  unreachable later, but if it is the id join alone, the S3 divergence table would
  otherwise look deceptively clean. About half a day of instrumentation.
- **B3 — the plan-killer number: worst-layer demand per corpus model.** A
  test-side instrument over the existing `normalized_slice_layers` output: for
  each corpus model, the maximum layer cross-section area, the total tip section
  the routed rule would demand (area / 12), and the implied seat count at both
  r_ceiling stops (0.400 mm radius = 0.800 mm diameter; 1.0 mm radius = 2.0 mm
  diameter). **This is the cheapest test of the assumption that can invalidate
  the whole plan** (§7.1): it prices ruling R2 on real models *before* eight
  stages of work, with no demand field built.
- **B4 — the minimum-section audit (E1) and kinematics read (E4).** A test-side
  diagnostic locator over every emitted member on the corpus: the distribution of
  routed-area to narrowest-section, locating today's real X per arm — plate 5
  broke at the waist, and nothing today locates the section that actually broke.
  Plus: read lift speed, lift distance, retract speed, and acceleration off the
  slicer profile; the repository's entire current knowledge of dynamics is "Not
  specified".
- **B5 — the pinned-value inventory and baseline reproducibility check.** Run the
  manifest's recorded baseline command against today's tree; assert the corpus
  artifacts reproduce byte-identically (excluding recorded timestamps). If they
  do not, STOP: the intended-vs-regression mechanism has no ground until the
  baseline reproduces, and fixing that precedes everything (§7.4).

All S0 instruments are `#[cfg(test)]` or standalone tools, bounded per the
backgrounded-command protocol (corpus runs metered, never polled for a line the
command has not been seen to print), and none can alter a decision.

---

## 3. The stages

Every stage: lands on `develop` via the project's normal branch flow, leaves
`cargo clippy --workspace --all-targets --all-features --offline -- -D warnings`
at zero and `cargo fmt` clean, commits as it goes, and never pushes. "Green" below
means the full test suite; the one sanctioned red shape is an `#[ignore]`d
evidence test named in a committed document.

### S0 — The baseline measurements (size 3)

**What it buys:** the numbers every later stage is judged against; the plan-killer
test (B3) run before any machinery exists; the pinned inventory the refreeze
protocol needs; separation of the inversion mechanisms while the code that
produced them still exists unmodified.

**Acceptance (mechanical):**
- B1–B5 artifacts committed under `docs/evidence/load-baseline-<date>/`, each
  regenerable by a named command recorded in the artifact.
- Corpus baseline reproduces byte-identically (B5), or the plan pauses on a named
  finding.
- The extended law assertion lands `#[ignore]`d with its red count recorded.
- Zero diff in emitted artifacts (nothing but tests and tools were added).

**Risk / rollback:** near zero; instrumentation only. Rollback = revert. The one
live risk is an unbounded corpus run — mitigated by I14-style budgets on the
instruments themselves and the standing ps-by-repo-path check.

### S1 — Registry honesty and drift guards (size 1)

**What it buys:** the registry stops overstating what is known, and the one
numerical coincidence currently holding sizing correct cannot drift silently
during the long stages that follow.

Contents (synthesis §10.4): downgrade `SATURN_LEADING_RELEASE_EDGE` from Measured
to project-record; add X_ALLOWED's resin-conditionality note; annotate the legacy
placeholder trio as diagnostic-only; move `FrozenTipLoadPolicy`'s
`implied_area_ratio() <= X_ALLOWED` check out of `#[cfg(test)]` into the
constructor (a named failure if it ever fires; today it passes at exactly
12.000000, so no output changes).

**Acceptance:** registry notes updated; constructor check present with a sabotage
test proving it fires; zero diff in emitted artifacts; suite green.

**Risk / rollback:** the constructor check could fire on a configuration nobody
tests — that would be a *finding* (the coincidence broken), and it fails loudly,
which is the point. Rollback = revert.

### S2 — The demand field, deciding nothing (size 8)

**What it buys:** the design's cheapest possible falsification. Build the field
(grid at derived q = 0.25 mm, components, orphan set O_k, the charge sweep's
geometry half), disclose it, let it decide nothing. Invariants I1, I2, I4, I7,
I10, I14 land WITH their sabotage companions. If I10 (field independence of the
seat set) or I14 (bounded, metered cost) cannot be made to hold, the design dies
here at the price of one stage.

Also lands: the registered constant q with its derivation note, and the field's
per-model build-cost disclosure (10.2) — the measured number the S6 budget
decision needs.

**Acceptance (mechanical):**
- I1, I2, I4, I7, I10, I14 green on all fixtures and corpus models; each sabotage
  test demonstrably red-fires (the test asserts the gate FAILS under
  perturbation).
- I4 uses the full remesh suite: subdivide, decimate without moving the surface,
  reorder, re-diagonalise — per-seat demand within relative 1e-9, no emitted-tip
  implication yet since nothing reads the field.
- Field build cost published per corpus model; every run under a registered
  budget; budget exceeded = named failure, never a stall (this repo's signature
  trap is an unbounded search that never prints a result line).
- Zero diff in emitted artifacts: regenerate the corpus baseline, byte-identical.

**Risk / rollback:** the field is a new consumer of `normalized_slice_layers`;
risk is cost, not correctness (correctness is what the invariants check).
Rollback = revert; nothing reads the field yet. Explicit kill criterion: if the
field cannot pass I10 + I14 on the Weeping Treant within the registered budget,
report to Craig with the measured numbers rather than tuning past it.

### S3 — The shadow ledger and the prediction files (size 10)

**What it buys:** the complete new sizing computation, running dark on every
fixture and corpus model; the E2 divergence table; the frozen prediction files
that make S5 mechanically checkable; the E3 fold measurement; the ledger half of
the R-c replacement built while the old machinery still serves production.

Contents:
- The charge sweep's allocation half: cells charged through solid to nearest
  engaged seat of their own component; per-seat, per-layer accumulators; demand =
  max over layers; `UncarriedDemand` as a named category (still disclosure-only —
  it cannot fail a run until S5).
- Rule R (recompute on any plan edit) with the incremental ledger and invariant
  I5 (incremental == from-scratch, bit for bit), plus I9 (scaffolding provably
  chargeless) and I11 (typed identifiers in every join — a positional index in a
  join is a compile error).
- The dark sizing path: tip radius from demand via X_ALLOWED, refusal
  computation, `min_section_on_path()` — all behind a default-off switch.
- The E2 divergence table generator, deterministic (two runs byte-identical),
  output committed under `docs/evidence/load-activation/`.
- The prediction tooling of §1.2 step 2, and the declared load-input file list.
- E3: member volume in mL corpus-wide under sum-of-peaks vs max-of-sums; the
  sweep-line fold is bought only if the measured saving justifies it.

**Acceptance (mechanical):**
- I5, I9, I11 green with sabotage companions (I5's sabotage: skip one re-charge,
  gate must redden — this is the invariant whose absence is today's measured 20x
  inversion).
- Divergence table and prediction files committed, regenerable, deterministic;
  every prediction row keyed by canonical typed id and stamped with base rev.
- Per-recharge and per-repair-trial cost published (the repair loop already runs
  274 trials at 6.5 s median on one fixture; Rule R multiplies into it — this
  number gates S6's feasibility, see §7.3).
- Zero diff in emitted artifacts with the switch off, byte-identical.
- Switch-on scratch run leaves no artifact outside the scratch lane.

**Risk / rollback:** largest pre-activation stage; risk concentrates in the
incremental ledger (I5). If incremental cannot match from-scratch, ship
from-scratch-only with its measured cost and record the S6 consequence — that is
a degradation, not a failure. Rollback = revert; switch stays off throughout.

### G1 — Gate: adversarial review and owner rulings (no code)

One adversarial reviewer (cross-family, per standing practice) attacks the
divergence table and predictions: reconciles every large divergence class against
B1/B2, verifies the sabotage tests genuinely exercise the paths (measured
path-hit counts, not existence), and attempts to construct a fixture where the
new numbers are physically wrong. Craig rules on R0–R3 (§5) with the table in
hand. **No frozen value moves before this gate passes.**

### S5 — Sizing activation: the one flip commit (size 8)

**What it buys:** the owner's actual complaint fixed — every tip radius on every
model becomes the direct image of a disclosed demand through one law; refusals
replace clamps; the 20x inversion class becomes unreachable (Rule R + I11); the
law assertion extends to every fixture and corpus model un-ignored (I3); I6 (no
zero-demand emissions) and I8 (scar accounting with bounded slack) activate.

Mechanics: exactly the protocol of §1.2 steps 4–6. One commit flips the switch
and carries the tool-generated refreezes; the movable class is declared as tip
radii, refusal/`UncarriedDemand` disclosures, and the digests that hash them.
Seat positions and counts must NOT move (placement still reads the old
machinery); the gate enforces that as an out-of-class check. The old partition
keeps computing as the reversed shadow — legacy beside ledger — so the S5 revert
is a single clean commit and divergence between them stays observable until S8.

**Acceptance (mechanical):**
- Every moved value equals its committed prediction; no out-of-class movement;
  prediction base-rev check passes (§1.2 step 5).
- Comparator sabotage demonstrated red before the flip (recorded in the evidence
  directory).
- I3 green corpus-wide un-ignored, grown-for-attachment tips exempt by name; I6
  and I8 green with sabotage companions.
- The refusal path exercised for real: placement is not active yet, so
  over-capacity demand at S5 is a NAMED RUN FAILURE on the fixtures it hits (the
  overhang table, which today ships a 0.95 mm radius shortfall, is expected to be
  one). The fixture list with failure reasons is enumerated at G1 from the
  divergence table — no surprises at flip time — and each is an explicitly
  argued red, documented, and the concrete pressure that scopes S6.
- Model-by-model walk recorded, claimed commits verified on the branch.

**Risk / rollback:** the flip commit is the rollback unit — one revert restores
old values and old behaviour consistently, because nothing else changed in it.
Risk: prediction staleness (guarded by the base-rev gate) and fixtures that
refuse under old placement (handled as argued reds, enumerated at G1 from the
divergence table so there are no surprises).

### G2 — Gate: owner ruling R4 (no code)

Craig walks the post-S5 corpus state and rules on placement activation, having
seen sizing's real cost first.

### S6 — Placement activation (size 10)

**What it buys:** seats go where load says they must — n_min seeding on the hex
lattice at derived spacing, capacity-loop insertion, refusal-feeds-placement
closes; TARGET_SPACING_MM and MIN_CONTACTS_PER_ISLAND retire as decisions (task
#58 closes); the 45-degree mask stays as the admission gate with zero-demand
telemetry recorded for C8.

Mechanics: same prediction protocol, second execution — predictions for seat
sets, counts, and the values they cascade into are generated dark (the placement
code lands default-off inside S6's own first commits), reviewed against the
declared movable class, then one flip commit. The existing coverage judgment
stays ON as the safety net: a seat the coverage closure demands that load
placement dropped is a named failure, not a warning.

**Acceptance (mechanical):**
- Prediction-equality and out-of-class checks as in S5.
- Capacity loop termination proven bounded (each insertion strictly reduces the
  maximum share) and metered under I14; the S3-measured per-trial cost times the
  observed trial count fits the registered budget, or the stage stops and
  reports.
- Coverage safety net green on every fixture and corpus model.
- Every S5 argued-red fixture now sizes without refusal, or its remaining refusal
  is re-argued in writing.

**Risk / rollback:** the placement loop's cost (Rule R × repair trials) is the
known hazard, priced at S3 rather than discovered here. Rollback = revert the
flip commit; the seeding/loop code is dark without it.

### S7 — The write-time gate (size 4)

**What it buys:** the pipeline's first end-to-end guarantee that the file on
disk, not the planner's memory, obeys the measured law — the load gate Python
had and Rust never ported (task #59 closes). I12 lands: the gate re-derives
demand and the law from the WRITTEN artifact and must agree with the emitted
disclosures on ids, demands, minimum sections, and pass/fail.

**Acceptance:** I12 green everywhere; sabotage: corrupt the artifact after
planning, gate must go red; the gate can only refuse — a passing run's bytes are
identical with the gate on and off, asserted.

**Risk / rollback:** a false-refusal (gate red on a good artifact) blocks output
— which is safe-direction and loud. Rollback = revert; no emitted bytes depend
on it.

### S8 — Demolition and cap migration (size 6)

**What it buys:** one load model in the tree instead of two; tasks #68/#69/#70
close; the digest surface stops hashing deleted fields.

Contents: delete census rows 1–3 (`supported_area_mm2` and copies — currently
read by no decision, only digests); delete the provenance-eligibility machinery,
`LoadOwner`, the island face-count ordering, the row-indexed transpose and
`CarriableMaximum`; migrate the resource caps and their registry notes onto the
ledger's I14 budgets (the R-c correction: this is a replacement, not a deletion —
the old representation also served the caps and the merge tournament's
retention, and the ledger inherits the I5 obligation it discharged); retire
`CoverageSample` after S6's telemetry shows coverage-as-charged-demand and
coverage-as-sampled agree; refresh the structural guards
(`contact_regression_gate.rs`'s MAJOR-7 entry now names `UncarriedDemand`;
a `retired_formula_guard`-style guard pins `FrozenTipLoadPolicy` to
diagnostic-only).

**Acceptance (mechanical):**
- **Bit-identity: this stage moves no emitted byte.** The digests that hashed
  deleted fields are the one exception, enumerated in advance exactly like a
  prediction file; everything else byte-identical.
- A cap-equivalence table: every registered resource cap, its old enforcement
  site, its new one, and a test per cap proving the budget still refuses.
- Workspace grep gates: no reader of the deleted symbols anywhere, enforced the
  way `retired_formula_guard.rs` enforces its retirement.

**Risk / rollback:** deleting the reversed shadow removes the standing
cross-check — which is why S8 is last, after S5/S6 have survived on the corpus.
Rollback = revert, still clean because S8 changed no behaviour.

---

## 4. What each stopping point buys

- **After S0:** the truth about today, on real models, committed — worth having
  even if everything else is cancelled.
- **After S1:** the registry stops lying and the sizing coincidence cannot drift.
- **After S2:** the tessellation-independent demand field exists with its
  invariants; the hollowing regression becomes visible as a field step-change.
- **After S3 / G1:** complete visibility at zero behaviour change — every wrong
  number on every model disclosed beside its correct value. If Craig stops here,
  the divergence table alone explains every "weird thick/thin" support he sees.
- **After S5:** sizing is right: thick where load is, thin where it is not, and
  a violation is a refusal instead of a shipped defect. The owner's complaint is
  answered even if placement never changes.
- **After S6:** density is right too; spacing stops silently implying a tip size
  nobody chose.
- **After S7:** the printed file itself is guaranteed law-abiding.
- **After S8:** the tree holds one load model and the maintenance cost drops.

---

## 5. Owner rulings required, in plain language

Listed separately per the standing escalation rule; the orchestrator rules on
technical questions, these are not technical questions.

- **R0 — the refreeze protocol itself (needed before S5, ideally before S3).**
  The standing rule is that a frozen value that moves is a bug. This plan
  proposes the one sanctioned exception: a frozen value may be refrozen when the
  new value was committed as a prediction before the behaviour change, the
  equality is machine-checked, and the movable class was declared in advance
  (§1.2). What turns on it: whether S5 and S6 can happen at all.
- **R1 — size from X_ALLOWED directly**, retiring the legacy stress form to a
  diagnostic (recommended unanimously by all three parent designs and the
  synthesis). What turns on it: whether the change is behaviour-preserving where
  today's clamps do not bind, and whether the refreeze at S5 happens.
- **R2 — accept the conservative routed sizing rule**, knowing it thickens and
  multiplies supports on large compact undersides and, most visibly, long
  horizontal limbs (the 10 × 40 mm limb prices at ~67 tips of 0.800 mm diameter
  versus the ~12 field practice uses) until coupons C10/C12 measure the
  travelling front and license a discount. What turns on it: whether S5 ships at
  all, and whether C10/C12 move from follow-up to prerequisite. **S0's B3
  instrument prices this ruling on the real corpus before any machinery is
  built.**
- **R3 — default r_ceiling:** the field ceiling (0.400 mm radius = 0.800 mm
  diameter — more, smaller scars) or the registered maximum (1.0 mm radius =
  2.0 mm diameter — fewer, larger scars). What turns on it: seat counts
  everywhere; the S3 predictions are generated at whichever stop is ruled.
- **R4 — placement activation (G2)**, after walking the post-S5 corpus model by
  model. What turns on it: S6 proceeding.

### Coupon plates the design requests (none block S0–S5; R2 may promote two)

Costs are stated as printer-days: one plate ≈ one pre-registration document, one
overnight print on the Saturn 4 Ultra (tens of mL of resin), one scoring
session. All plates follow the standing rules: computed discriminant,
must-survive and must-fail controls, matched ±Y pairs, no factor assigned by a
position formula, nothing scored above z ≈ 9 mm, and the resin vendor, batch,
and ambient RECORDED — neither shipped plate did.

| Coupon | Measures | Cost | What it unlocks |
|---|---|---|---|
| C1 | Position severity — read the never-scored plate 1 family D and plate 5 rows R1/R5 if the physical plates survive | ~zero (scoring only) if plates exist; 1 printer-day otherwise | bounds lambda(y); identifies the leading release edge |
| C10 | Vertical branch: local vs cumulative transmitted load | 1 printer-day | could relax the routed rule toward orphan-local — the single most consequential open physics question |
| C12 | Aspect/front width on long caps | 1 printer-day | licenses the elongation discount §6.1 of the design pays for today; first observation of which tank edge leads |
| C11 | Spatial allocation under one cap | 1 printer-day | the only instrument that can show the nearest-cell allocation itself wrong |
| C7 | Footprint vs surface area at three tilts | 1 printer-day | tests the load quantity's footprint reading |
| C8 | Ramp ladder at three layer heights | 1 printer-day | the derived 35.264° self-support angle; first layer-height evidence of any kind |
| C9 | Small-cap ladder 0.10–3.14 mm² | 1 printer-day | ratio-vs-threshold in the bottom 1.5 decades where ordinary tips live |
| C2 | Solid vs shell at matched projected area | 1 printer-day | separates weight from peel for the first time |
| C4 | X* replication triplets, three independent plates | 3 printer-days | meets the project's own three-plate promotion rule for X* |

If R2 is refused at the priced corpus cost, C10 and C12 become prerequisites for
S5 and the plan re-sequences: S0–S3/G1 stand unchanged (they are measurement and
shadow), the two plates print during S3, and S5's predictions are regenerated
under whatever discount the coupons license.

---

## 6. Test and gate consequences, enumerated

- `support_runner_integration.rs` — the digest assertions move at S5/S8 under
  the protocol. Nothing in this plan ever polls that binary for a `test result`
  line; every invocation is bounded (`gtimeout`), per the standing trap note.
- `contact_regression_gate.rs` — structural; rewired at S8 when the guarded
  wiring changes (MAJOR-7's named condition becomes `UncarriedDemand`).
- `coverage_gate_toed_slab.json` — refrozen at S6 under the protocol.
- `corpus/baseline/*` — refrozen at S5 (sizing fields) and S6 (seat fields),
  each from prediction files.
- `prepare_folder.rs` law assertion — extended to the corpus at S0
  (`#[ignore]`d), activated corpus-wide at S5 (I3).
- `resource_cap_policy.rs` — migrates at S8 with the cap-equivalence table.
- New tests land with their sabotage companions in the same commit, and every
  new long-running check carries an I14 budget so it is structurally incapable
  of joining the unbounded-search trap.

---

## 7. What could make this whole plan wrong

1. **The routed conservative rule prices real models absurdly (R2 refused).**
   The deepest assumption is that no-front-credit sizing is acceptable on the
   owner's actual corpus until coupons license discounts. If B3 shows every
   real model demanding limb-style multiples of field practice, the sequence
   inverts: C10/C12 become prerequisites. **Cheapest test: B3, in S0, days into
   the plan, requiring nothing but the existing slice output.** The plan
   survives the inversion — S0–S3 are unchanged either way.
2. **The field cannot be both seat-independent and affordable (I10 vs I14).**
   Killed at S2 by design, at the price of one stage; the kill criterion is
   written into S2's acceptance rather than left to judgment.
3. **Rule R's cost times the repair loop is unaffordable (I5 vs I14).** The
   repair loop already runs 274 trials at 6.5 s median on one fixture, and one
   fixture already fails to reach a verdict in 1200 s. Priced at S3
   (per-recharge cost published); if incremental cannot close the gap, S5 still
   ships (sizing needs one recompute per plan, not per trial) and S6 waits on
   ledger engineering — a truncation the stopping-point table already values.
4. **The golden baseline does not reproduce today.** Then no diff against it
   means anything and the entire acceptance mechanism is ungrounded. Tested at
   S0 (B5) before anything else; if red, fixing reproducibility becomes the
   first stage and everything shifts behind it.
5. **A resin change mid-plan.** X_ALLOWED = 12 is conditional on one
   unidentified black resin; the registry note (S1) makes the condition
   explicit, and any resin change during the plan re-opens G1 rather than being
   absorbed silently.
