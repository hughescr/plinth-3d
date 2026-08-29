# One load ledger, from placement to tip

**Date:** 2026-08-29. **Status:** design and staged plan. Nothing in production changed.
**Written for:** Craig. Every term is explained the first time it is used. Tip sizes are
always given both ways: the code speaks **radius**, you and the slicer speak **diameter**.

---

## 0. Your two questions, answered first

### "Why are those supports even in place if they carry nothing at all?"

Because two parts of the engine do not talk to each other.

Supports are **placed** by a spacing rule: walk the underside of the model from the bottom
up, and put a support wherever there is overhanging surface at least 4 mm from the last
support — like fence posts. Load is never consulted. This was measured directly: the
placement code sorts candidates by height and position only; no area or load quantity
appears anywhere in that decision.

Supports are **sized** by a separate ledger that divides the model's overhanging surface
among the supports afterwards. That ledger has a defect: it can only hand an **entire mesh
triangle** (models arrive as meshes — surfaces made of triangles) to the **single nearest**
support. On a flat 10 × 10 mm base, which the mesh describes as just two big triangles, two
or three supports get billed for everything and the rest get billed exactly zero. The zero
is a bookkeeping artifact, not a physical fact — physically, every support under that base
shares the pull. The sizing step then reads the bill literally: zero billed, thinnest
allowed tip (0.05 mm radius = **0.1 mm diameter**, a hair).

So the supports are right to be there. The number that sizes them is false.

### "Does the current allocation system make sense physically?"

**The idea does. The implementation breaks it three ways.**

The idea — divide the down-facing area among supports by nearness, then size each tip so
the area it carries is at most 12 times the tip's own cross-section — is exactly the law
your coupon plates measured (plates 5 and 6; the measured failure point is about 25, and we
design at half of that, 12). That part is sound and this design keeps it.

The three breaks, all measured in the tree this week:

1. **The ledger counts triangles, not area.** How much a support "carries" depends on how
   the model happens to be triangulated, not on the shape. The same solid, re-meshed finer,
   produces different support sizes — measured: subdividing every triangle of a test shape
   into four (same shape, same total area) moved the typical emitted tip from 0.1 mm
   diameter to 0.63 mm diameter, and the upper quartile by 17×.
2. **It never splits.** One support takes a whole triangle no matter how big. A 450 mm²
   triangle lands entire on one tip; the tips near it get nothing. (A splitting mechanism
   was actually built and is deliberately switched off, because a downstream comparison
   structure can only record one carrier per triangle — see stage 3.)
3. **Nothing re-checks after the plan changes.** Tips are sized early; later steps remove
   some supports; nothing re-sizes the survivors. Measured worst case: a support billed for
   450 mm² shipped with a 0.1 mm-diameter tip, when the law demands 2.0 mm — while four
   supports nearby, billed 40 mm² each, shipped at the 2.0 mm maximum. **The two most
   heavily loaded supports in that run were the thinnest on the plate.** That inversion is
   the "weird thick/thinness" you have been seeing, mechanised.

One honesty note: the diagnosis this task started from said supports are sized by the area
of the single triangle they sit on, so refining the mesh would make them thinner. That was
checked and is wrong — that per-triangle field is written and carried but **read by no
decision anywhere**, and refining the mesh actually made tips *thicker* (because more of
the surface became visible to the ledger). The real mechanisms are the three above. The
conclusion — placement and sizing disagree and neither sees the load — stands, on better
evidence.

---

## 1. What is wrong today — the two measured facts

Both were measured on this branch's instrumentation (test-only, prints numbers, decides
nothing), reproducible with one `cargo test` command recorded in the run notes.

**Fact A — the flat box.** A 10 × 10 × 10 mm box, flat underside, 11 supports emitted:

| supports | billed area each | emitted tip |
|---|---|---|
| 3 | 50 mm² | 2.0 mm diameter (the maximum — and the law wanted *more*: 50 mm² exceeds the 37.7 mm² a maximum tip may carry) |
| 8 | 0 mm² | 0.1 mm diameter (the minimum) |

A 20× spread in tip diameter across one flat plane, produced entirely by "nearest support
takes the whole triangle." Under the fixed ledger, all 11 would carry roughly 9 mm² each
and print at about **1.0 mm diameter, uniformly** — which is what a human would draw.

**Fact B — the same shape, finer mesh.** A table-shaped test solid, subdivided twice
(same geometry, 4× then 16× the triangle count): supports billed zero fell from 85 % to
43 %, and the median emitted tip went 0.10 → 0.10 → 0.63 mm diameter. Support sizing
currently depends on the mesh's triangulation, which no physical model should. The same
run exposed the inversion described above (450 mm² on a 0.1 mm tip).

On top of both: 75 % of that test shape's surface area is invisible to the ledger — it
never becomes a ledger entry at all, and no disclosure reports it. Much of that is
correctly un-billed (up-facing surface rests on the material below it and loads no
support), but the engine nowhere states that rule; the area just silently isn't there.

---

## 2. The proposed model, in plain words

One ledger, one currency, used by every stage. The currency is **down-facing area in mm²**
— the area that must be pulled off the vat film against suction each layer, which the
plates proved is what snaps supports.

1. **Measure the loaded area once, independent of triangulation.** The engine already has
   machinery that chops every triangle into uniformly small cells for coverage checking.
   The ledger is built from those cells, so its numbers depend on the shape, not on how
   the artist happened to mesh it. It covers exactly the surface that demands support:
   down-facing overhang regions (called *islands* in the code) plus the special seats the
   engine reserves (crevice arms, cavity interiors). Everything else is declared —
   in writing, in the run record — "carried by the model itself," so silence is
   eliminated.
2. **Every square millimetre is billed to exactly one support, or split.** A missing
   ledger entry is a named error, never a quiet zero. (This policy is inherited from the
   deleted auditing crate — the one genuinely good idea in it.)
3. **Nearest support with budget takes each cell.** A support's budget comes straight
   from the measured law: it may carry at most 12 × the cross-section of the largest
   allowed tip = **37.7 mm²**. If the nearest support is full, the area spills to the next
   nearest. This is the splitting today's ledger cannot do — and splitting load across
   neighbouring supports is itself measured physics: on plate 6, every multi-arm strip
   survived at per-arm ratios that killed the identical single arm. Sharing is real, at
   least additive, proven up to groups of six.
4. **Placement reads the same ledger.** Supports are placed so that no loaded cell is out
   of reach of a support with remaining budget. Spacing follows from load instead of the
   fixed 4.0 mm constant. Where load is dense, supports go closer; a small island gets
   one; nothing is placed "because the pattern said so" and then starved by the ledger.
5. **Sizing becomes one line.** Tip cross-section = carried area ÷ 12. In radius form:
   tip radius = √(carried ÷ 12π), clamped between your ruled bounds — 0.05 mm radius
   (0.1 mm diameter) minimum, 1.0 mm radius (2.0 mm diameter) maximum for load-sized
   tips. The 0.4 mm-radius (0.8 mm-diameter) scar ceiling you set stays what it is:
   a bound on growing tips for attachment, never on load sizing.
6. **The ledger is recomputed after every plan edit.** If routing removes a support, its
   area spills to its neighbours and their tips re-size before anything is written. The
   emitted tip always matches the final plan. This single rule kills the inversion.
7. **A support that still carries zero after all that** (for example, one placed only to
   satisfy the one-support-per-island minimum on a tiny island) is *declared*
   "coverage-only" in the output record and gets the minimum tip **by stated policy** —
   never zero-by-accident. (Your ruling D3 below picks this or an alternative.)
8. **The law applies at the narrowest point.** Plate 5's caps broke at the base of the
   cylinder above the cone — not at the nominal tip — on every large cap. The sized tip
   must be the narrowest cross-section of its neighbourhood, widening quickly above; the
   check that enforces this is part of stage 2.

No force is ever computed — no pressure, no strength-of-resin numbers. The whole model is
the measured, dimensionless ratio. That is deliberate: the project's own evidence shows
the underlying pressure varies ~50–80× with contact shape, so any force computation would
be false precision. The ratio is what was actually measured.

---

## 3. Why this is right — and where it goes beyond the measurements

**Traced to the plates:**

- **Area is the right currency.** Plate 5's 24-cell grid was built so three competing
  theories predict different cells failing, with a discriminating span of ~24,000×.
  Area-pressure won; perimeter and squeeze-film lost. Your own summary: "every cap got
  ripped off its support except the really small caps."
- **The ratio 12 is the measured boundary, halved.** Cone tips fail at a
  carried-area-to-tip-section ratio of 25.0–25.3 (plate 5); cylinders bracket the same
  boundary at 19.9–30.5 (plate 6), which retired the worry that the cone shape was
  special. Design at 12 clears the tightest measured edge by 1.66×. One caveat carried
  honestly: the lower edge of the cone interval (25.00) is a directly recorded read; the
  upper edge is a reconstruction the results file itself labels an inference. The design
  constant 12 derives from the recorded edge only.
- **Splitting is measured.** Plate 6's strips: 12 of 12 multi-arm cells survived at
  per-arm ratio 60 while the matched single arm died at 60. Group ratio governs. The
  design never assumes better than simple sharing (part of the strips' margin was extra
  root cross-section, not sharing).
- **Sizing at the narrowest section is measured.** The break location was observed, not
  theorised.
- **Weight is correctly ignored, and your hollowing argument holds — strengthened.** For
  a hollowed shell of constant wall, weight and peel force both scale with area, so their
  ratio is *size-invariant*: 0.1–9 % depending on which pressure anchor you take, at any
  model size. Even a fully solid part never reaches weight-equals-peel inside this
  printer's build volume (crossover is at a 171 mm sphere under the most weight-friendly
  reading, against a 118 mm build axis). This is arithmetic corroborated by literature
  ("separation forces reach hundreds of newtons, gravity rarely exceeds ten"), not a
  plate — no plate can distinguish the two, because every coupon was a flat cap.

**Where it goes beyond the plates — stated so nobody mistakes it for measurement:**

- **Nearest-support billing is a modelling choice.** The plates fix the total any arm may
  carry; they do not say how area divides between neighbours. Nearest-with-budget is the
  simplest rule consistent with the physics, and it is what both the deleted auditor's
  input contract and the retired Python assumed. No plate distinguishes it from
  alternatives.
- **Below 3.14 mm² carried, the ratio law is extrapolation.** The smallest cap ever
  printed was 3.14 mm²; the engine's admissible carried areas go down to 0.09 mm². Ruling
  D5 proposes the plate that closes this.
- **One pressure regime is assumed.** The project's two pressure anchors on this printer
  differ 54–81× between slender walls and flat discs. The ratio form is assumed to absorb
  that. For ordinary contact patches (5–38 mm²) we are inside the measured band; thin-fin
  slivers are not.
- **Position on the plate is ignored, as an assumption.** The only number in the repo is
  a "no difference detected" at a resolution of 1.53× — and the coupon families built to
  measure position (plate 1 family D, plate 5's front/back pair) were never scored. The
  in-repo line calling front-to-back sensitivity "measured" traces to your ruling, not to
  a reading. The design does not use plate position and says so.
- **The constant is conditional on the resin.** The plates' resin was recorded only as
  "black, ~30 °C." The green (uncured) strength of this resin class spans ~2.4× across
  vendors; our margin is 2×. Safe while you print one resin family; recorded here so it
  never becomes a silent assumption.
- **Arm length is untested.** The ratio was measured on 4 mm arms; the 6 mm family on
  plate 6 was voided by an unexplained artifact above ~9 mm plate height. Until that is
  understood, no new coupon feature goes above ~9 mm.

---

## 4. What changes on a printed model

- **The thick/thin lottery ends.** Today's output is a step function: most tips at the
  0.1 mm-diameter minimum (billed zero), a few at the 2.0 mm maximum (billed a whole big
  triangle), occasionally inverted so the heaviest-loaded support is the thinnest. Under
  the ledger, tips take the value the load asks for: on a flat base, uniform ~1.0 mm
  diameter; under a heavy overhang, thicker; on a lace-thin fin, at or near the minimum
  because the load genuinely is tiny. **This is the direct explanation and fix of the
  thick/thin behaviour you observed.**
- **Total scar area does not change; its distribution does.** A fixed invariant of the
  law: total tip cross-section = carried area ÷ 12, about 8.3 % of the supported
  down-facing area, however it is tiled. Expect many medium scars in place of a few
  maximum scars plus a spray of pinpricks. Individual scars get *larger* on average.
  There is no way around that without changing the ratio itself; what we control is where
  they land (delicacy weighting is unchanged).
- **Placement moves modestly at first, then meaningfully.** On finely meshed miniatures,
  4 mm spacing already yields ~14 mm² per support against a 37.7 mm² budget, so early
  stages barely move layouts. Stage 4 (load-aware placement) will add supports under
  large flat areas, remove them from lightly loaded regions, and stop layout from
  depending on triangle counts.
- **Some models may temporarily refuse instead of shipping wrong.** Honest sizing means a
  support asked to carry more than any legal tip can hold becomes a refusal at the write
  gate rather than a silently thin support. Ruling D4 covers this.

---

## 5. The staged plan

Order matters: the review of this design identified three feedback cycles (sizing feeds
routing order; placement repairs feed the ledger; splitting requires rebuilding a
downstream structure). The order below is chosen so each stage is verifiable when it
lands. Sizes are dependency-ordered, not calendar guesses.

**Stage 0 — Ground truth on real models.** Test-only instrumentation on the actual
miniature corpus (e.g. the Weeping Treant's ~558 contacts): the billed-area histogram per
support, the rate of the inversion, and which of two suspected mechanisms produces it —
tips going stale after plan edits, or a support-ID mix-up found on inspection (one code
path numbers supports by position in a list, another looks them up by that number as if
it were an identity; on a list with gaps the load lands on the wrong support).
*Buys:* the design's numbers on real models instead of test boxes; tells us whether the
ID mix-up is a separate one-line-class bug. *Costs:* small; no production change, no
frozen-test churn.

**Stage 1 — Build the honest ledger alongside, not in charge.** Compute the cell-based,
budgeted ledger and write it into a side report next to today's numbers, for every corpus
model. It decides nothing yet, and deliberately lives outside the engine's sealed
fingerprints (the engine cryptographically freezes its decision inputs at birth; a new
quantity must either be born sealed or stay outside — this stays outside until it rules).
*Buys:* a free A/B of old vs new sizing on every model before any behaviour changes; the
acceptance evidence for stages 2–4; the triangulation-independence claim, measured.
*Costs:* new code and report plumbing only.

**Stage 2 — Fix the sizing end (first behaviour change).** (a) Size tips from the
registered constant directly (ruling D1); (b) recompute the ledger and re-size tips after
every plan edit, so the emitted tip always matches the final plan — this kills the
inversion; (c) fix the support-ID mix-up; (d) implement the pre-write load gate — the
check that refuses to write a plan whose tips violate the law. That gate is currently an
enum entry that returns "not yet ported"; the only law check in production is one test on
one small fixture. (e) enforce tip-is-narrowest-section at the seat.
*Buys:* no support ever ships thinner than the law demands, on any model.
*Costs:* the big one — tip radii move on every model, so the frozen-test estate moves:
roughly 74 tests certainly, up to ~342 possibly, three frozen artifacts, and two
source-text tripwire tests whose pinned strings must be re-ruled. The current re-record
ceremony (one owner ruling plus a leaf-by-leaf diff per golden file) does not scale to
that; ruling D2 sets the protocol *before* this stage starts.

**Stage 3 — Let area split across supports.** Flip the ledger from
one-support-per-triangle to the budgeted spill. The splitting arithmetic already exists
and is switched off because one downstream structure (the comparison baseline used when
support plans are merged) records exactly one carrier per ledger row with three retained
neighbours; splitting shears it. This stage is the rebuild of that structure — the
largest single engineering item in the plan, already on the books as tasks #69/#70.
*Buys:* the zeros disappear; every support carries its physical share; Fact A's box
prints eleven ~1.0 mm tips.
*Costs:* the rebuild, plus a second round of re-records under the D2 protocol.

**Stage 4 — Load-aware placement.** Placement and repair consult the ledger: spacing
derived from budget (already on the books as task #58), repair ranked by uncovered load
rather than coverage alone, and the one-triangle-area field that started this
investigation retired (it is inert; it should not survive as a trap).
*Buys:* supports land where load is; the full "holistic" ask, complete.
*Costs:* every layout changes; full corpus re-baseline; the placement/repair interplay is
the delicate part and is last on purpose, because stages 1–3 built the measuring stick
that judges it.

---

## 6. How we will know it worked

Mechanical checks, not eyeballs — each distinguishes an intended change from a regression:

1. **The law gate, on every model.** Every emitted support's tip must be at least what
   the law demands for its billed area. Exists today as a test assertion on one fixture;
   becomes a write-refusing gate on all models in stage 2. The 450 mm²-on-a-0.1 mm-tip
   case goes from "shipped silently" to "impossible."
2. **Conservation.** The billed areas must sum to the measured loaded area, and every
   loaded cell must appear exactly once. A missing entry is a named failure.
3. **Triangulation invariance.** The subdivide test: same solid at 1×, 4×, 16× triangle
   counts must produce the same billed-area distribution and tip sizes within a small
   tolerance. Today's measured drift (median 0.10 → 0.63 mm diameter) becomes a hard red.
4. **The invariants that must NOT move:** total scar area stays at billed ÷ 12; corpus
   emit count does not fall below the current 24-of-37 baseline (except where ruling D4
   deliberately allows a refusal); and "suite green" is always stated against the named
   baseline red set — four known-red pipeline tests (task #76) and one load-flaky test
   (task #37) — because the suite is not green today and pretending otherwise would hide
   regressions. One prior safety net is explicitly *not* relied on: the frozen
   integration triple everyone assumed guards the support run lives in a test binary that
   never finishes under plain `cargo test`, so it currently guards nothing.
5. **One witness print.** A small plate with the flat-base worst case supported the old
   way and the new way, side by side. You should see three fat tips and eight hairs on
   one, eleven uniform ~1 mm tips on the other.

---

## 7. What needs you to decide

**D1 — Size from the measured constant, or keep the legacy formula?** Today's tip formula
is an old stress calculation built from four placeholder numbers that *happens* to equal
the measured law at ratio 12 — a coincidence with nothing pinning it. Options: size
directly from the registered constant (recommended: one honest input instead of four
unmeasured ones), or keep the legacy form and document the coincidence. What turns on it:
whether stage 2 is behaviour-preserving in arithmetic, and therefore how many frozen
tests move. Recommendation: **size from the constant.**

**D2 — The re-record protocol.** Stages 2–4 move tip numbers across ~74–342 tests, three
frozen artifacts, and two source-text tripwires. The existing ceremony — one ruling and a
leaf-by-leaf diff per golden — was designed for two goldens and collapses at this scale.
Proposal: one ruling **per stage**, backed by a machine-produced before/after summary
(counts, min/median/max shifts, and the law-gate/conservation/invariance results above),
with per-golden diffs retained as evidence but not individually ruled. What turns on it:
whether stages 2–4 take weeks or quarters.

**D3 — What a genuinely zero-load support gets.** After the ledger is honest, a few
supports will still legitimately carry nothing (e.g. the mandatory single support on a
tiny island). Options: (a) refuse to emit it; (b) emit at minimum tip, explicitly
declared "coverage-only" in the record; (c) force-assign it a share. Recommendation:
**(b)** — it preserves coverage, and the declaration keeps the books honest. What turns
on it: whether small islands keep their supports, and what the output records mean.

**D4 — May the law refuse a model?** Honest sizing converts "silently too thin" into "the
gate refuses to write." Before stages 3–4 add splitting and better placement, a handful
of corpus models could temporarily refuse rather than emit. Options: refuse-and-disclose
(recommended — matches the project's prove-it-or-refuse doctrine), or a temporary
oversize-and-warn mode. What turns on it: whether the corpus emit count is allowed to dip
during the middle stages.

**D5 — Coupon plates.** In priority order:
1. **Free first: re-score existing plastic.** If physical plate 1 and plate 5 still
   exist, two never-read families answer the position question without a print: plate 1's
   peel-direction family D, and plate 5's matched front/back rows.
2. **Plate 7A — small caps.** Caps below 3.14 mm² (≈0.8 / 1.5 / 3.1 mm²) across a tip
   ladder, testing whether the ratio law holds at the scale most support tips actually
   operate — today the bottom decade and a half is extrapolation. All features below
   ~9 mm plate height until the plate-6 anomaly is understood.
3. **Plate 7B — replication.** Triplets at ratios 22/25/28 to promote the failure
   boundary to the project's own three-plate standard (it currently rests on three
   individual specimens). Record the resin vendor and batch this time — the plates that
   set our constant recorded only "black."
What turns on D5: nothing in stages 0–4 blocks on a print; 7A would tell us whether thin
fins need their own rule; 7B converts the core constant from corroborated to replicated.

**D6 — Small standing orders.** Record resin vendor/batch on every future plate. Do not
let any design term depend on which tank edge peels first: the datasheet does not state
it, and no observation in the repository establishes it, despite one registry entry
labelled "measured." One photographed coupon would settle it if we ever need it.

---

## 8. What this does not fix

- **Position on the plate.** Where a model sits front-to-back may change its peel load;
  the model stays position-blind, by stated assumption, up to at least the 1.53×
  resolution of the only relevant measurement.
- **Thin-fin pressure.** Slender features see far lower peel pressure than flat patches
  (the project's own anchors differ ~50–80×). One ratio is applied to both regimes until
  plate 7A says otherwise. The error direction is conservative for fins (they get
  slightly thicker tips than they strictly need).
- **The narrowest section along the whole support column.** Stage 2 enforces it at the
  contact; a general locator over every strut and brace of the support forest is not in
  this plan.
- **Weight and motion forces.** No gravity or acceleration term, on the arithmetic in
  §3. The unbounded residual is acceleration during the lift — the printer's lift speeds
  and distances are unpublished and unmeasured here.
- **Sharing beyond six arms.** Measured to at least additive up to groups of six; no
  upper bracket exists; the design never assumes more than additive.
- **Separate known issues.** The Weeping Treant hollowing regression, the four known-red
  pipeline tests, and the support cosmetic defects you parked remain their own work
  items. The suite's baseline red set is named in §6 rather than fixed here.

---

## Appendix — record of rulings on the review challenges

This design was produced under an adversarial workflow. For the record:

**Named gap.** The synthesis lane and the planning lane of the workflow returned nothing.
This document is therefore the first design, written directly from the three ground-truth
reports (engine-as-built, physics evidence, prior art) by the ruling lane — not a
revision of a prior draft. Both challengers correctly reported there was nothing to
review; that is recorded as a gap, not as agreement.

**Challenge 1 (blocker: no design to review) — ACCEPTED** as fact; remedied by this
document.

**Challenge 2 (deliverability constraints) — each claim ruled, all verified against the
tree at `d1f53ea`:**

- Two distinct candidate mint sites, and the sealed-fingerprint rule forbidding late
  assignment (`lib.rs:984`; `perception.rs:1136-1147`; `digest_seal.rs`): **ACCEPTED.**
  Design consequence adopted: the stage-1 ledger lives outside sealed digests until it
  rules (stage 2+), at which point it is born sealed.
- "Ranking by `supported_area_mm2` would rank a constant": **ACCEPTED** — the design
  never reads that field and stage 4 retires it. The challenge's suggested "step 0"
  (reconcile the two mint sites of an inert field, at the cost of a digest refresh) is
  **REJECTED as unnecessary spend**; retirement supersedes reconciliation.
- Re-record scale (74 certain / 342 possible tests; three frozen artifacts; two
  source-text gates at `contact_regression_gate.rs:177,195`; per-golden ceremony does not
  scale): **ACCEPTED**; drives ruling D2.
- The seven couplings (routing order keys at `initial.rs:1708/1713`; rewire load
  arithmetic; member sizing; the brace deliberate-zero argument; the PR-4 refusal path;
  coverage repair's load-blindness; the emitted-tip triple): **ACCEPTED**; each is named
  in a stage's cost line.
- The three cycles and the ordering prescription (sizing before placement, splitting
  before placement): **ACCEPTED**; the stage order in §5 follows it.
- The support-ID join inconsistency (`specialized.rs:7558` positional id;
  `specialized.rs:1611` by-id find vs `:2342` positional-with-filter): **ACCEPTED as
  real on inspection**; stage 0 measures whether it is the inversion's mechanism; stage 2
  fixes it regardless.
- `PreWriteGate::Load` does not exist as an implementation (`gates.rs:64, 304-313` —
  returns "not yet ported"): **ACCEPTED**; stage 2 budgets it as task #59, and the
  zero-carry vacuity it would suffer is closed by conservation (§6.2) plus ruling D3.
- "The suite is not green, so 'green' is not a claim": **ACCEPTED**; baseline red set
  named in §6.4.
- The frozen integration triple guards nothing because its binary never finishes:
  **ACCEPTED**; §6.4 states the plan does not rely on it.

**Rulings on the original brief vs ground truth** (the brief this task was launched
with): its claim that selection sizes supports by one triangle's area is **REJECTED in
its consequence** — the field is inert and refinement made tips thicker, not thinner; its
"87 of 90 zero on make_box" **did not reproduce** at either measured configuration (8 of
11 emitted is the measured figure) and is replaced by the numbers in §1; its description
of the deleted auditor as "the closest thing to a working allocation" is **AMENDED** —
the auditor was a checker that demanded the allocation as an input and never ran; what
survives from it is the policy adopted in §2.2. Its physics section is **ACCEPTED** with
the physics lane's refinements (recorded-vs-inferred bracket edges, the position
non-measurement, the resin conditionality), all carried into §3 and §7.
