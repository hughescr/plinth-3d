# The Support Load Model — the design, with every challenge ruled

Status: DESIGN OF RECORD, superseding the earlier draft in this file and amending
`docs/plans/load-model-synthesis.md` (branch `design/load-model-synthesis` @ 942804d)
and `docs/plans/load-model-implementation-plan.md` (branch `design/load-model-plan` @
8f1d505). Two adversarial reviews attacked those documents — one on the physics, one
on whether the plan survives contact with the code. Every finding was checked against
the code and the printed-plate results and ruled: the full ruling record is §9. The
design survives with real amendments; the plan needed surgery. What follows is the
design and plan AS AMENDED. Nothing here changes production behaviour; every stage
that would is gated on the decisions in §7.

---

## 0. Your two questions, answered first

**"Why are those supports even in place if they carry nothing at all?"**

They are carrying load. The record that says "zero" is wrong — not the supports.

Supports are placed today by a coverage rule: scan the model bottom-up, keep any
candidate spot at least 4 mm from an already-accepted one. No load number is
consulted at all. Then, afterwards, a bookkeeping pass tries to write down how much
each support carries — and that pass is nearly blind. It can only assign load to
mesh triangles inside detected "overhang islands" (patches the scanner flagged), and
it hands each whole triangle to the single nearest support, however large the
triangle is. On our standard test shape (a small table: 30 x 30 mm top on four
legs), 75.6% of the model's surface is invisible to that bookkeeping, so 71 of the
85 supports it prints are recorded as carrying exactly zero.

The physical supports are where coverage wanted them, and the peel force of every
layer really does flow through them. The danger runs the other way from the
question: because tip thickness is computed FROM the blind record, a support
recorded as "zero" gets the thinnest permitted tip — 0.10 mm diameter — while
actually carrying real load. Worse, on that same test shape the two most heavily
loaded supports (450 mm² of model area each, per the engine's own record) were ALSO
given 0.10 mm tips: twenty times thinner than the measured law demands. That
violation shipped without any complaint from the pipeline.

**"Does the current allocation system make sense physically?"**

No, in three specific ways:

1. **Eligibility is paperwork, not physics.** A patch of model gets load accounting
   only if the island scanner happened to claim it. Most of the model's area is
   simply absent from the load ledger — not zero, absent.
2. **A triangle is never split.** The nearest support takes the whole triangle. One
   support gets overloaded on paper; its neighbours record zero.
3. **Load is computed after every support position is already fixed**, so it can
   never influence where supports go — and it is never recomputed when the plan is
   edited afterwards, which is how a 450 mm² load ended up recorded against a
   0.10 mm tip.

One idea in it is reasonable and survives: dividing nearby area among nearby
supports. But the physics review is right that this is a bookkeeping convention,
not a measured fact — our plates proved a group of supports shares load, not HOW
the share divides. A one-day coupon print (C11, §7) is the only thing that can
test the division rule itself, and this plan now prints it before any behaviour
changes.

---

## 1. What is wrong today, in two measured facts

Background, one paragraph. A resin printer builds the part upside down, hanging
from a metal plate. Each layer is cured against a transparent film, and the machine
must tear every fresh layer off that film — once per layer. That tearing force (the
**peel force**) is by far the largest force in the process: literature measures it
at up to hundreds of newtons, where the part's weight is a few newtons. It acts
over the AREA being torn off. Two of our printed coupon plates (72 and 34 test
specimens) measured when green resin — resin as printed, before the hardening UV
bath — snaps under that force: a cross-section fails when asked to carry about
25 times its own area. Our design ratio, keeping back a factor of two, is **12**:
one square millimetre of support cross-section may carry twelve square millimetres
of model area.

**Fact 1 — the ledger is blind, and the two facts it does write down contradict the
law.** Measured on the test table: 71 of 85 printed supports recorded at zero
(sized to the 0.10 mm-diameter minimum), two supports recorded at 450 mm² each and
STILL sized at 0.10 mm diameter where the law demands 2.0 mm — a 20x inversion,
shipped. The engine also never re-checks: its one sizing formula agrees with the
measured ratio of 12 only through a numeric coincidence of four unmeasured
placeholder numbers, and nothing in production would notice if one of them moved.

**Fact 2 — support thickness depends on how the model file happens to be
triangulated, not on the shape.** Splitting every mesh triangle of the same test
shape into four (identical solid, identical surface area) moved the median printed
tip from 0.100 mm to 0.631 mm diameter, and the upper quartile by 17x. Nobody
chooses a model's triangulation on purpose; it changes when a file is re-exported.
That is a mechanism for exactly the "sometimes weirdly thick, sometimes weirdly
thin" behaviour you reported.

---

## 2. The proposed model, in plain language

One idea: **compute the load from the sliced solid itself — before any support
exists — and let that single ledger drive where supports go, how thick they are,
and a final check on the printed file.** The printer already slices the model into
layers to print it; the load model reads those same slices.

Two questions, answered by two readings of the same ledger:

**Where must supports stand?** Wherever new material appears that its own footing
cannot carry: the first slice of an outstretched hand, a chin, a ledge growing
sideways faster than the layer below can hold. The ledger walks the layers in print
order and flags exactly those cells. This uses the same reach rule the engine
already ships (your 2026-08-19 ruling fixed its conservative end); no new physics.

**How thick must each support be?** When a layer peels, suction acts on the WHOLE
layer, and that force has to travel down through the printed material to the
supports — it cannot vanish. So every layer's full cross-section is charged, cell
by cell, to the nearest support territory REACHABLE THROUGH SOLID MATERIAL (load
never jumps an air gap to a support under a different limb). A support's demand is
its worst single layer — worst, not summed, because layers peel one at a time. Its
tip is then sized by the measured ratio: tip cross-section at least demand / 12.

Five rules complete the model, each one a direct answer to a defect found by
measurement or review:

1. **The law holds at the narrowest point, not just the tip.** Plate 5 broke every
   large specimen at the waist above the cone, never at the tip disc. Every point
   of the path from model to build plate is checked.
2. **The model's own thin sections have the same limit** (new — added by this
   ruling, §9 SA-4). Load routed through the model obeys the same ratio at every
   model cross-section it crosses. A thin wrist cannot be asked to transmit a
   torso's peel load to supports beyond it; if the ledger tries, that is a demand
   for supports on the near side of the wrist — never a bigger tip somewhere else.
   This is also what makes thin arms end up with small supports and cores with
   large ones, which is the pattern you expected and the un-amended design could
   not promise.
3. **Too much demand is a refusal, never a silent clamp.** Today the engine clamps
   a tip at 2.0 mm diameter and ships the violation. Under this design, demand
   beyond one tip's capacity (37.7 mm² at 2.0 mm diameter) forces another support
   into that territory; demand that no admissible support can take fails the run
   with a named reason.
4. **A hollowed model is charged by its filled outline until a coupon says
   otherwise** (amended by this ruling, §9 SA-2). The one direct measurement on
   record — holes punched in a layer at constant outer outline — reduced peel force
   barely at all (2.68x less cured area, only 1.26x less force). So hollowing is
   NOT assumed to reduce peel load; it still pays fully in resin and weight, and
   the cavity's ceiling still demands supports from inside the cavity. A one-day
   coupon (C2', §7) can license the credit later.
5. **The written file is re-checked.** A final gate re-derives the law from the
   file on disk — not from the planner's memory — before anything is printed. That
   gate is already built and waiting on a branch (§5, Stage L).

What the model deliberately does NOT do: it computes no force in newtons (the
evidence is a ratio, and stays one); it takes no credit for the tilting tank's
travelling peel front (nobody has measured the front, or even observed which edge
of the tank leads); it assumes nothing about plate position, resin brand, weight,
or lift speed beyond what §3 states.

---

## 3. Why it is right — traced to the plates, honest about where it goes beyond them

**What the plates measured, and this design uses as measured:**

| Claim | Evidence |
|---|---|
| Peel failure follows AREA, not perimeter, not a suction-gap formula | Plate 5: 24-cell grid, 8 tip sizes x 3 cap areas, built so three competing theories predict different cells failing; area won. |
| The failure ratio is ~25; design at 12 (half) | Plate 5 lower edge is a directly recorded specimen; plate 6 reproduced the bracket on a second arm shape. |
| The break happens at the narrowest section | Plate 5: every large specimen broke at the waist, observed directly. |
| Several arms under one load share it at least additively, to 6 arms | Plate 6: all 12 strip cells survived at per-arm ratios far past the single-arm boundary. |
| The "zipper" fear (peel concentrating on the leading arm) is false at the scale tested | Same 12 cells. |

**Where the design goes beyond the plates — each one named, with its instrument.**
The physics review sharpened this list considerably; three entries changed under
its attack (§9).

1. **Below 3.14 mm² per support, the direction of error is UNKNOWN** (amended —
   the earlier draft claimed "safe", and that claim is withdrawn). 3.14 mm² is the
   smallest cap ever printed on a plate. Below it, two pieces of our own evidence
   point opposite ways: thin slender walls measured far LOWER peel pressure (safe),
   but published direct measurements say peel force falls slower than area
   (dangerous — a linear law can under-predict force 2-7x at the smallest tips).
   The only field evidence in the gap is yours: 0.10 mm-diameter tips survive when
   short. Coupon C9 closes this; until it reads, small-tip sizing is labelled an
   extrapolation, not a safety.
2. **Dividing a layer among many territories extrapolates the sharing result past
   6 arms** (amended — the earlier draft both forbade and did this). A 900 mm²
   underside needs at least 24 supports arithmetically; sizing each for its own
   territory assumes no zipper-like concentration appears at N = 24. The plates
   refuted the zipper at N ≤ 6 with the strongest test available, every slicer and
   all field practice make the same extrapolation, and the design takes no
   travelling-front credit on top of it — but it is an extrapolation and is now
   stated as one. (The review's proposed remedy — refuse any group larger than 6 —
   would refuse every large model ever printed and is rejected.)
3. **"Nearest territory" is a bookkeeping convention, not force balance** (amended
   — the earlier draft called it "the correct continuous reading" of plate 6; it
   is not). The review's beam example is correct: a rigid plate on two unequal
   supports loads the far one ~19% more than its territory suggests. That skew
   sits inside our 2x design margin for realistic layouts, and the accounting is
   exact — no area is dropped — but only coupon C11 can test the division rule
   itself. C11 is one printer-day; it now prints during the shadow stage and its
   reading gates the flip (§5).
4. **No travelling-front credit.** The tank tilts, so a big layer probably peels as
   a moving front, not all at once — which is very likely why field practice gets
   away with far fewer supports on long limbs than this design will demand. Nothing
   in the repository measures that front; even the registered "which edge of the
   tank releases first" traces to an assertion, not an observation. The design
   over-sizes long horizontal geometry until coupons C10/C12 buy the discount, and
   §4 shows that cost honestly.
5. **Position on the plate, resin identity, lift speed, weight, inertia.** One
   plate non-detection at its own resolution (≤1.53x) is all we have on position;
   the plates' resin vendor and batch were never written down (the ratio 12 is
   conditional on "one black resin at ~30 °C"); no lift kinematics are recorded
   anywhere; weight is excluded by arithmetic (a shell's weight-to-peel ratio is
   size-invariant at 0.1-8.8%; even a solid part never reaches parity inside the
   build volume). Each is a declared assumption with a named instrument, not a
   silent one.

---

## 4. What changes for a printed model

**A 10 mm test cube.** Today: 11 supports; 8 recorded at zero and printed at
0.10 mm diameter, 3 recorded at 50 mm² each and clamped at 2.0 mm (the law wanted
bigger; the clamp shipped the shortfall). New: worst layer is 100 mm², total
required tip cross-section 100/12 = 8.3 mm² — three supports near 1.9 mm diameter,
or seventeen at 0.80 mm diameter, evenly spread. Every tip's size is the direct
image of a disclosed area number.

**The test table (30 x 30 mm underside).** Today: 63 supports at fixed 4 mm
spacing; 71 of 85 fixture-wide record zero; two carry 450 mm² each on 0.10 mm tips.
New: 900 mm² of birth-layer demand; 24 supports at 2.0 mm diameter, or ~150 at
0.80 mm — your choice of ceiling (decision D4) sets which. Compact flat undersides
like this are exactly where the coupons are valid and where big prints genuinely
tear off, so no discount is taken here.

**A horizontal limb, 10 mm thick, 40 mm long — the honest bad case.** Field
practice holds such a limb with about a dozen 0.8 mm tips. This design, taking no
front credit, prices its 400 mm² worst layer at ~67 tips of 0.80 mm diameter (or
the same cross-section in fewer, larger tips). That is the deliberate price of not
outrunning the data; coupons C10/C12 are the purchase order for the discount, and
decision D3 asks whether to pay the price or make the coupons prerequisites.

**A 30 mm solid sphere.** The old draft of one parent design would have held it
with a single 0.7 mm support (sized from its tiny first layer) while 707 mm²
equator layers peel above — that support snaps. This design prices it at ~19
supports of 2.0 mm diameter, spread over the lower dome. Hollowed, it is priced
the same until coupon C2' reads (§2 rule 4) — hollowing's savings stay in resin
and weight for now, not in support count.

**Does this explain the thick/thin behaviour you observed? Yes, mechanically.**
Today's sizing is a step function with three rungs: most supports fall to the
0.10 mm minimum because their recorded load is zero; a few get whole large
triangles and slam into the 2.0 mm clamp; and plan edits after sizing can leave a
heavy support with a thin tip (the 20x inversion). Which rung a given support
lands on is decided by the accidental triangulation of the model file and by edit
order — invisible, arbitrary-looking causes, which is why the result looks weird
rather than wrong-in-a-pattern. Under the new model, thickness varies smoothly
with real local area, identical-looking regions get identical supports, and
re-exporting a mesh cannot move a tip size.

---

## 5. The staged plan, as amended

The deliverability review proved three structural defects in the original plan by
tracing the code, and all three rulings are adopted (§9 DA-1..DA-5):

- **There is no seam between "sizing" and "placement".** The load number is not
  just a tip-size input today: it orders the routing queue, positions junctions,
  slims ancestor members, and feeds a refusal check inside the placement search.
  Changing it moves seats, counts, and member positions, full stop. The two
  planned activation flips are therefore merged into ONE.
- **The final safety gate already exists.** A branch (`feature/gate-load`, four
  commits, adversarially reviewed on `docs/gate-port`) already ports the
  write-time area-law gate — and landing it today turns the test suite from 3 red
  to ~20 red, because today's output genuinely violates the law. The plan no
  longer schedules "building" it; it schedules LANDING it, on your timing
  (decision D6).
- **The golden baseline cannot be reproduced, by construction.** The recorded
  regeneration command is a Python tool deleted 2026-08-08, and a build guard
  (which you ratified) fails the build on any Python file. Exactly one golden
  printer file exists (the Sphere). The acceptance mechanism is re-grounded on
  Rust-side fixtures instead of a baseline nobody can regenerate.

One scope caution that shapes everything below: 10 of the 13 real miniatures that
do not currently emit stop at a 1500-second timeout in an unrelated repair loop.
Corpus-wide numbers in this plan mean "the models that finish today", named per
artifact; fixing that loop is separate work this plan does not absorb.

| Stage | What it is | Size (points) | Output moves? |
|---|---|---|---|
| M | Measure today, before touching anything | 4 | No |
| R | Registry honesty | 1 | No |
| F | The demand field, deciding nothing | 8 | No |
| S | Shadow everything; prediction files; coupons print | 14 | No |
| G | Review gate: adversarial pass + your rulings | — | No |
| A | THE flip: one commit, sizing and placement together | 12 | Yes — everything declared |
| L | Land the already-built write gate | 2 | No (it can only refuse) |
| D | Demolition of the old ledger; cap migration | 6 | No (bit-identical) |

Total ≈ 47 points (same scale as before: 1 = one small bounded change).

**Stage M — measure (buys: the truth about today; the plan-killer numbers first).**
The corpus load census (how bad is the zero-carry pattern on real models, not
fixtures); the half-day instrumented run that separates the two candidate
mechanisms of the 20x inversion while the guilty code still exists; the worst-layer
price list — for every model that finishes, what the conservative rule would charge
(this prices decision D3 days in, before any machinery); the narrowest-section
audit; reading lift speed and acceleration off the slicer profile (the vendor
datasheet specifies none of it); and the pinned-value inventory in FIVE categories
— including the ~28 source-code-string guards the original plan missed entirely,
each with a named fate, because a prediction file cannot predict a grep.

**Stage R — registry honesty (buys: the records stop overstating).** The
tilt-direction constant's documentation stops implying an observation exists (the
review caught that the planned "downgrade" was a no-op — the entry already cites
only a project record; what changes is the note, not the type). The ratio-12
constant gains its resin-conditionality note. The sizing coincidence gets a
constructor-level tripwire so it cannot drift silently — including through the
test-only side door the review found.

**Stage F — the demand field, deciding nothing (buys: the cheapest possible
falsification).** Build the layer ledger; disclose it; let it decide nothing.
Field-only invariants land here (the review correctly showed the per-support
invariants cannot exist before territories do — they move to Stage S): every
layer's cells sum exactly to its true area; the field ignores supports entirely
(the field is now defined over the MODEL only — the review caught the
contradiction, and the fix is principled: a support's own fresh layer equals its
own cross-section, ratio 1, twelve-fold inside the law, so supports never need
field accounting); re-triangulating a shape cannot move the field; features
thinner than the grid trigger local refinement or a named refusal, never silent
disappearance; and every build runs under a metered budget. Kill criteria are
written in: if the field cannot be both support-independent and affordable on the
Weeping Treant, the design dies here at one stage's price.

**Stage S — shadow everything (buys: complete visibility at zero behaviour
change).** The full allocation runs dark beside production on every model that
finishes: territories, routed demands, neck checks, dark sizing AND dark
placement. Its cost is priced honestly — this is a second full support run per
model, which is why this stage carries 14 points. Outputs: the divergence table
(per support: old recorded load and tip beside new demand and tip, with a reason
class); committed prediction files holding every value the flip will change;
hand-computed analytic fixtures (cube, table, sphere — shapes where the right
charges are derivable on paper) that become permanent tests, because the review is
right that prediction-equality alone cannot catch a self-consistent wrong
allocator. During this stage the coupons print: C11 (allocation — REQUIRED before
the flip; it is the only instrument that can show the division rule itself wrong),
C1 (score the two never-scored existing plates, nearly free), and C10/C12/C2' as
ruled in D3.

**Gate G — review.** One adversarial reviewer attacks the divergence table and the
predictions; you rule on D1-D5 with the table and the C11 reading in hand. No
frozen value moves before this gate.

**Stage A — the flip (buys: your complaint fixed).** One commit: the ledger
becomes the load model. The movable class is declared honestly — tip radii, seat
sets and counts, junction positions, member radii, routing outcomes, and every
digest and golden that hashes them — because the code trace proved they all move
together. Everything outside that class that moves fails the build. Every moved
value must equal its committed prediction, tool-generated, never typed. The
comparator is sabotaged red before first use. The one test that pins
tip-radius-equals-law-radius moves in this same commit, in lockstep with the gate's
law reading. Rollback is one revert.

**Stage L — land the write gate** per your D6 timing: the branch is reviewed and
ready; after the flip it should land green (the flip removes the violations it
refuses on). Landing it earlier, red, with the reds recorded as argued findings, is
also on the table per the gate reviewer's own recommendation — that is D6.

**Stage D — demolition (buys: one load model in the tree).** The blind eligibility
machinery, the whole-triangle partition, the inert per-triangle area fields, and
the coincidence formula (kept as a diagnostic) go; the resource caps that lived on
the old representation migrate with an equivalence table; bit-identity required —
this stage moves no printed byte.

Stopping points: after M you have the truth about today (worth having even if all
else is cancelled). After S you have every wrong number on every model disclosed
beside its correct value — the divergence table alone explains every weird support
you have seen, even if nothing ever flips. After A your complaint is fixed. After L
the printed file itself is checked. After D the tree carries one load model.

---

## 6. How we will know it worked — intended change vs. regression, mechanically

The original plan called its acceptance mechanism a proof. The review demonstrated
it is not one — the same code writes the predictions and the output, so a
self-consistent wrong allocator passes — and the mechanism is hereby renamed what
it is: a **reviewed-output lock**. It proves "the flip emitted exactly the output
that was reviewed and ruled on", nothing more. Correctness comes from four
independent legs; the lock keeps anyone from moving a number without them.

1. **The lock.** Every frozen value that moves must equal a value committed in a
   prediction file BEFORE the behaviour change, machine-checked at the flip;
   every moved value outside the declared class fails the build; the comparator
   itself is proven able to go red before first use. (This codifies, at scale, the
   re-pinning practice you have already ruled on four times — the review corrected
   the plan's claim that it was a novel exception.)
2. **The independent oracle.** Analytic fixtures whose correct charges are
   computed by hand on paper, asserted as permanent tests — plus coupon C11, the
   physical check on the one rule no in-engine test can validate.
3. **The invariants** (conservation, exclusivity, law-at-narrowest-section,
   neck capacity, no-zero-demand emissions, and the rest) — necessary, never
   sufficient, and every one ships with a sabotage companion that proves the gate
   fires. A gate never seen red is not a gate.
4. **Human judgment, placed early.** You and one adversarial reviewer read the
   divergence table BEFORE anything moves — judgment as the first line, never the
   last.

---

## 7. What needs you to decide

- **D1 — ratify the reviewed-output lock.** The standing rule says a frozen value
  that moves is a bug. The lock is the disciplined exception: move only to a
  pre-committed, reviewed, machine-checked value. Turns on it: whether the flip
  can happen at all.
- **D2 — size from the measured ratio 12 directly**, retiring the four-placeholder
  coincidence formula to a diagnostic. Recommended unanimously by all three parent
  designs, the synthesis, and this ruling. Turns on it: whether sizing rests on a
  measurement or on a coincidence.
- **D3 — accept the conservative price, or make coupons prerequisites.** No
  travelling-front credit and filled-outline charging mean visibly more support on
  long limbs (the 67-tip example) and no support savings from hollowing yet. Stage
  M prices this on your real models within days. If the price is refused, C10/C12
  (front) and C2' (hollowing) move from follow-ups to prerequisites and the flip
  waits on their readings. Turns on it: support counts on every large model, and
  the plan's critical path.
- **D4 — default thickest tip: 0.80 mm diameter (more, smaller scars) or 2.0 mm
  (fewer, larger scars).** Pure preference between scar patterns; total scar area
  is the same either way (it is demand/12 regardless of tiling). Turns on it: seat
  counts everywhere; predictions are generated at whichever you rule.
- **D5 — the flip itself**, at gate G, with the divergence table and C11 reading
  in hand, walked model by model, never in bulk.
- **D6 — when to land the already-built write gate.** Its reviewer's options:
  (a) after the flip, landing green; (b) now, red, with ~20 reds recorded as
  argued findings naming the known defect; (c) hold. The gate reviewer recommends
  (b); this plan sequences for (a) and treats (b) as equally sound. The one
  forbidden option, per that reviewer and this ruling: weakening the gate to get
  green.

**Coupon plates requested** (each ≈ one pre-registration document + one overnight
print + one scoring session; all record resin vendor, batch, and ambient — neither
shipped plate did):

| Coupon | Question it answers | Cost | Gates |
|---|---|---|---|
| C1 | Plate position: score the two purpose-built, never-scored existing plates | ~zero if the plates survive | nothing; bounds an assumption |
| C11 | Does load divide among supports the way the ledger divides it? | 1 printer-day | REQUIRED before the flip |
| C10 | Does load transmitted down through the model add up the way the ledger says? | 1 printer-day | the conservative routing rule's discount |
| C12 | How wide is the travelling peel front on long shapes? | 1 printer-day | the long-limb discount (D3) |
| C2' | Does a holed layer at the same outline peel with less force? (solid vs ring at fixed outline) | 1 printer-day | the hollowing sizing credit |
| C9 | Small-tip ladder below 3.14 mm² | 1 printer-day | trusting sub-band sizing |
| C8 | The derived 35.26° self-support angle, at three layer heights | 1 printer-day | retiring the 45° mask |
| C7 | Is peel load footprint area or surface area? (three cap tilts) | 1 printer-day | the load quantity's reading |
| C4 | Replicate the failure ratio on three independent plates, resin recorded | 3 printer-days | promoting the ratio per your own three-plate rule |

---

## 8. What this does not fix

- **The 10 models stuck at the repair-loop timeout.** They cannot even be measured
  end-to-end today. Separate, prerequisite-shaped work; this plan's corpus numbers
  exclude them by name until it lands. (Two further models crash for unrelated
  reasons.)
- **Plate position.** The design carries position severity as an explicit 1.0
  placeholder. C1 bounds it cheaply; until then, a real front-to-back effect up to
  at least 1.53x would pass unmodelled.
- **Resin identity.** Ratio 12 is conditional on one unidentified black resin. A
  resin change re-opens gate G; only C4 with recorded vendor/batch retires this.
- **Lift dynamics.** No inertia term, and nothing to bound one with — the vendor
  datasheet specifies no lift speed, distance, or acceleration. Stage M reads them
  off the slicer profile as a first fact.
- **The travelling front.** Un-earned discounts stay unpaid: long limbs get
  visibly more support than field practice until C10/C12 read (D3).
- **Hollowing's support-count dividend.** Withheld until C2' reads; hollowing
  keeps paying in resin and weight regardless.
- **Sub-3.14 mm² sizing, sharing past 6 arms, arm free length, layer heights other
  than 0.05 mm.** Each is a named extrapolation with a named coupon; none has
  measured backing today.
- **Print-quality defects unrelated to load** — surface scarring aesthetics beyond
  scar placement, drainage, cure quality — out of scope entirely.

---

## 9. The ruling record

Both challengers returned substantive attacks; there is no named gap. Every
finding was verified against the code or the plate records before ruling; file and
line citations were spot-checked in the tree at `develop` @ d1f53ea, and the ones
that decide structure (DA-3, DA-4, DA-5) were re-verified directly for this
ruling: the load quantity's reads in the routing comparator
(`crates/plinth-support/src/initial.rs`, `routing_order`), the junction solver and
ancestor slimming (`crates/plinth-support/src/cco.rs`), the refusal predicate
(`crates/plinth-support/src/contact.rs`), the 28-row source-string guard table
(`crates/plinth-support/tests/contact_regression_gate.rs`), the already-
`project_record` provenance of the tilt-direction constant
(`crates/plinth-core/src/constants/evidence.rs`), the four-commit gate branch
(`feature/gate-load`) and its ruling (`docs/gate-port`), the single golden
artifact in `corpus/baseline/`, and the dead Python regeneration command in
`corpus/manifest.json` against the live no-Python build guard.

### The physics attack (SA-1..SA-10)

| # | Finding | Ruling |
|---|---|---|
| SA-1 | "Safe below 3.14 mm²" contradicts the repository's own literature notes (force falls slower than area; linear under-predicts small tips 2-7x) | **ACCEPT, amended.** The claim "safe direction" is withdrawn; the two in-repo anchors genuinely point opposite ways (slender-wall pressure vs sublinear exponents), so the label is "unknown direction". C9 gates reliance; the owner's short-0.10 mm-tip field practice is the only evidence in the gap and is recorded as such. |
| SA-2 | Ring-area hollowing credit contradicts the one direct fixed-outline measurement ("hollowing out a layer does not reduce its peel force") | **ACCEPT.** Sizing charges the filled outline; the ring credit requires coupon C2'. The plates cannot answer this (every specimen was solid), so the project's "plate 5 governs" ruling is not in conflict. Hollowing's weight/resin case is untouched. |
| SA-3 | Nearest-territory allocation is accounting, not force balance; the beam example under-charges the far support ~19% | **ACCEPT, amended.** The "correct continuous reading of plate 6" claim is withdrawn; the allocation is a declared convention whose worked skew sits inside the 2x margin. C11 is promoted from optional to REQUIRED before the flip — it prints during the shadow stage, so the critical path does not move. The conclusion that this alone bars activation is rejected. |
| SA-4 | No transmission capacity: an arbitrarily thin model neck can carry an arbitrarily large routed charge | **ACCEPT.** New invariant: the same ratio law applies at every model cross-section on a charge path; a violated neck is a placement demand on the load's side of the neck, never a sizing increase elsewhere. This also delivers the thin-arm-small-supports pattern. C10 remains the physical instrument. |
| SA-5 | "Lower supports thicker than higher" is not an invariant of the design | **ACCEPT** as a stated limitation. An upper support taking a shelf's load independently to the plate is physically legitimate; the design does not promise the pattern and now says so. |
| SA-6 | The design forbids extrapolating sharing past N=6 and then relies on 19-150-way division | **ACCEPT the contradiction, amend the text; REJECT the remedy.** What is forbidden is super-additive CREDIT; per-territory sizing at large N extrapolates the refuted-zipper result and is now declared as such. Partitioning models into mechanically independent groups of ≤6 would refuse every large model and does not follow from plate 6. |
| SA-7 | Tessellation independence incomplete: seats still snap to triangle-derived candidates; the fixed 0.25 mm grid can erase sub-grid features; the remesh test is ill-defined | **ACCEPT.** Seat generation is redefined over the tessellation-normalised surface sampling the engine already computes (subdivided to a fixed edge limit), with a stated seat-position tolerance; the remesh test gets two legs (frozen seats: field must be invariant; regenerated seats: outcomes within stated tolerance); features thinner than the grid trigger local refinement or a named refusal — the grid may prune work but may not decide geometry. |
| SA-8 | Degenerate cases open: the field's "model plus supports" definition contradicts field-independence; zero-claim supports have a relabelling loophole | **ACCEPT.** The field is defined over the MODEL only — sound because a support's fresh layer equals its own cross-section (ratio 1, 12x inside the law), so supports never need field accounting. A zero-demand seat is removed unless it carries a named, budgeted, disclosed disposition; "stability" without a stated reason is not one. |
| SA-9 | Prediction equality cannot distinguish intended output from a self-consistent wrong allocator | **ACCEPT.** Renamed the reviewed-output lock (§6); hand-computed analytic fixtures added as the independent oracle; C11 as the physical oracle for the allocation rule. |
| SA-10 | Stage defects: invariants scheduled before their inputs exist; the sizing-first flip degrades fixtures into refusals; the insertion-loop termination proof is false | **ACCEPT all three.** Field-only invariant forms land in Stage F, per-support forms in Stage S; the two flips are merged (see DA-3, which independently forces this); the capacity loop's termination is re-founded on the finite admissible-seat set with a dictionary-ordered share vector as the progress measure and a named infeasibility refusal. |

### The deliverability attack (DA-1..DA-5)

| # | Finding | Ruling |
|---|---|---|
| DA-1 | The planned provenance "downgrade" is a category error (the constant is already project-record-sourced); the constructor check has unnamed holes | **ACCEPT.** Verified in the tree: the wrapper is `Measured`, the source already `project_record`. Stage R now changes notes and documentation, not provenance types. The constructor check gets its real shape: a fallible signature, the allowance read from the evidence registry at the one construction site, and the test-only bypass constructor routed through the same check. |
| DA-2 | The frozen-value surface is two orders larger than planned, includes source-string guards a prediction file cannot cover, and the freeze-window race is unpriced; the "one sanctioned exception" framing ignores four prior re-pin rulings | **ACCEPT.** Verified: 53 digest literals in one test file alone; a 28-row source-string guard table pinning the exact line this design renames. The inventory gains a fifth category (source-string guards, each with a named fate at the stage that moves its target); predictions are scoped to disclosed values and artifact digests, with internal pins re-generated by tooling; prediction regeneration after unrelated commits is tooling plus a delta review, not a full re-review; the lock is framed as codifying existing practice (§6). |
| DA-3 | The sizing/placement seam is false: the load quantity orders routing, positions junctions, slims ancestors, and feeds a refusal inside the search — seats move when sizing moves | **ACCEPT, fully verified in code.** The two activation flips are merged into one (Stage A) with the movable class declared honestly: tips, seats, counts, junction positions, member radii, routing outcomes, digests, goldens. This also resolves SA-10's refusal-degradation finding — placement feedback exists at the only flip there is. |
| DA-4 | The write gate scheduled as future work already exists on a reviewed branch, and landing it turns the suite ~20 red today | **ACCEPT.** Verified: four commits on `feature/gate-load`, ruling on `docs/gate-port` ("merge both, do not weaken"). Stage L lands the branch; timing is owner decision D6 with the gate reviewer's options quoted; the flip commit moves the tip-law equality test and the gate's law reading together. |
| DA-5 | The golden baseline is unreproducible by construction (deleted Python regenerator, live no-Python guard, one golden artifact); the shadow stage is an unpriced second full run per model against a corpus where 10 models time out; one instrument targets a private function | **ACCEPT.** Verified in the tree. The reproducibility check is re-grounded on Rust-side fixtures with the Sphere named as the sole golden; Stage S is re-priced (14 points) with its doubled-run cost stated; corpus scope is "models that finish today", named; the worst-layer instrument lives inside the crate's test module where the private slice output is reachable. |

### What survived both attacks unamended

Direct sizing from the measured ratio; maximum-over-layers rather than sum;
refusals instead of clamps; recomputation after every plan edit; typed identifiers
in every join; exact area accounting; the narrowest-section obligation; the
sabotage-first discipline; measurement before machinery; and the field's core
property — the load is computed from the sliced solid, so triangulation cannot
move a tip.
