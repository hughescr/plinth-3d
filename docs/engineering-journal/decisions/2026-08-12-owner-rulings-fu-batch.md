# Decision — ten follow-up rows ruled in one batch

**Rulings given 2026-08-12 by the owner (Craig Hughes)** on ten rows of the
follow-ups ledger (`CURRENT-STATUS.md#follow-ups-2026-08-04` — deleted in the 2026-08 docs restructure, content not carried forward):
**FU-55, FU-57, FU-50, FU-47, FU-38, FU-46, FU-54, FU-37, FU-51, FU-36**, in the
order the owner gave them. Each ruled row is annotated in place in the ledger and
points back here; this record carries the reasoning as it stood on this date.

**What this record is, and is not.** It is a record. The commit that lands it
touches documentation only: **no gate, cap, threshold, bound, constant or registry
entry moves in it.** Two rulings *decide* a number or a field — FU-54's pass limit
of **8** and FU-37's `wall` field on `HollowReport` — and both are recorded here as
the owner's decision, to be registered with proper provenance by the change that
implements them, not by this one. Where a ruling spawns new work it is filed as a
new ledger row (**FU-61**, from FU-51), never by renumbering an existing one.

---

## 1. FU-55 — under-walled shells never ship, and the fallback is explicitly interim

**The ruling.** An under-walled hollow shell must **never** ship. In the interim, a
**bounded realized-wall check gates hollow emission**: a shell that fails it is
**discarded**, the model **falls back to SOLID**, and it **still prints**. No model
becomes "not printable" because of this check — the fallback path already exists
(the solid reserve) and is what a failing shell takes.

**This is FU-55's option (b), plus a floor under it.** The check is a *tightening*
and moves no threshold, which is what made (b) admissible in the first place; the
solid fallback is what keeps the tightening from costing a model its artifact.
Option (a) — accepting interim under-walled emission — is **rejected**: a 0.27 mm
wall is roughly five resin layers, and an artifact that looks preparable and cannot
be printed is worse than a solid one that can.

**The fallback is interim and is not the goal.** FU-53 (`CURRENT-STATUS.md`, deleted in the 2026-08 docs restructure) —
make the engine actually deliver the requested 2.5 mm wall — is **MANDATED and
prioritized**. The objective is hollowing properly, not refusing gracefully; a
graceful refusal that becomes permanent is a hollow stage that never pays for
itself, and hollowing is where the resin saving lives.

**The wall constant does not move.** `HOLLOW_WALL_DEFAULT_MM` = 2.5 mm is the
physical requirement; the engine is what is wrong. FU-53's own standing instruction
— do not respond by lowering the registered wall to match what the engine produces
— is reaffirmed here, and the interim check must be written against the registered
wall, never against a value chosen to let today's shells through.

**Recorded, because the record must be true:** `feature/hollow-warhorse` was merged
to `develop` (`17ae61e`) while FU-55 read **"Do not merge with the decision
unmade."** That precondition was violated. This ruling **regularizes the state after
the fact** rather than pretending the precondition held: the three models
(`Weeping Treant v1.2`, `Warhorse`, `Giant Toad`) are on `develop` emitting shells
at 11–51 % of the registered wall today, and the interim check is therefore owed on
`develop` as work, not owed on a branch as a merge condition.

---

## 2. FU-57 — coverage-refusal evidence: sidecar, card, and renders, all three

**The ruling.** A coverage refusal keeps its evidence, in three forms:

1. a **sidecar `<model>.coverage.json`**, written beside the would-be artifact path;
2. the **CLI card summarizes the worst islands with their uncovered mm²**, so 21
   refusals can be ranked without a rebuild;
3. **visualizations — renders — of the worst uncovered islands**, as part of the
   refusal evidence rather than as a separate diagnostic errand.

**Evidence only, never a gate input.** None of the three is read by
`judge_pre_write_gates`, which stays private and unchanged. The verdict is decided
exactly as it is today and then explained; explanation must never become an input to
the decision it explains.

**Nothing is ever written at the `.chitubox` target path.** The 2026-08-09 gate
landing's claim — a gate-refused model leaves no file at its target — is untouched:
the sidecar and the renders are a different file class beside that path, written on
the refusal path only.

**Why all three rather than the cheapest one.** FU-56's first action needs to know
*which* islands and *how much* area; the card makes the run readable at a glance,
the sidecar makes it machine-readable, and the render is what tells a human whether
an uncovered patch is a real overhang or a modelling artifact. The row named the
sidecar as the minimum; the ruling takes the whole set.

---

## 3. FU-50 — the frozen rasterizer is replaced, and the replacement is proven first

**The ruling.** Replace the frozen midpoint-quartering rasterizer with an
**area-proportional conservative rasterizer**. The **new marked-voxel set must be
proven on the committed corpus — the delta measured and recorded — BEFORE it
ships.**

**The cap does not move, and "work item" is not redefined.** Both prohibitions the
row wrote against itself stand: raising `max_raster_work_items` would enshrine
300–1,000× waste as a budget and be blown by the next larger base anyway
(demand grows as `L_max²`), and redefining what a work item *is* would be
reinterpreting the cap rather than fixing the rasterizer. Under an area-proportional
rasterizer the existing cap carries roughly three orders of magnitude of headroom,
which is the whole point.

**Why the proof comes first.** The marked-voxel set is the input to the EDT, so it
moves every hollowed model's cavity. That is a measurable amount, on eight committed
models, and it is measured and written down before the change is shipped — not
inferred from the argument that the new rasterizer is better.

---

## 4. FU-47 — a point touch is not a defect: connectivity means resin can flow

**The ruling, as doctrine and not as a waiver.** A point-touch is **NOT a defect**.
Two interior volumes are **"connected" only through a passage wide enough for resin
to flow**, and the threshold is **the drain bore diameter the pipeline already
drills** — the implementation pulls the registered drill-diameter constant
(`HOLLOW_DRAIN_R_MM` = 1.5 mm radius, a 3.0 mm bore) as the **minimum-throat rule**
in cavity connectivity rather than restating a number. A **zero-width point never
connects two interiors**: each side is its own cavity, and each needs its own drain.

**The owner's rationale, paraphrased faithfully.** A mini pressing two fingertips
together is legitimate sculpting, not a modelling defect. The interiors on either
side of that touch do not connect, because **resin cannot flow through a point**.

**What follows mechanically:**

- `TOPOLOGY_CUT_POINT_ONLY` **comes out of the closed-solid gate's `any`** in
  `plinth_hollow::validate_closed_mesh`;
- the **pinch-split repair action becomes unnecessary** — it exists to make these
  meshes pass a check that is going away;
- the **false "not closed, consistently oriented solid" refusal message dies with
  the check** that produced it. The census proved those 21 models already were
  closed and consistently oriented; the message asserted a reason that never fired.

**This removes work; it does not add a waiver** — exactly the outcome FU-47
predicted for this ruling. It is not a gate loosening in the D-22 sense: nothing
about closure or orientation is relaxed, and no threshold moves. A check whose
premise was wrong is retired, and a *physical* rule about resin flow — with an
already-registered constant behind it — takes its place.

**Not ruled here:** FU-48 (`CURRENT-STATUS.md`, deleted in the 2026-08 docs restructure), the attribution of the
decimator-manufactured pinch. Its refusal premise changes once the flag leaves this
gate, but which stage *writes* the pinch is still an open measurement and that row
is left standing untouched.

---

## 5. FU-38 — all four decision-relevant instruments move to the real delicacy field

**The ruling.** Update **ALL FOUR** instruments to the real delicacy field, and
**re-pin every affected digest with from→to attribution**:

1. `crates/plinth-support/examples/embed_incidence_census.rs` — the **OD-8 census
   digests**;
2. `crates/plinth-support/tests/support_runner_integration.rs` — **both J4 snapshot
   digests** (`make_toed_slab`, `make_raking_blade`);
3. `crates/plinth-verify/tests/coverage_gate.rs` — the **coverage-gate golden**
   (`coverage_gate_overhang_table.json`);
4. `support_runner_integration.rs` — the same geometry's **structural asserts** and
   hand-counted coverage resource plan.

The fifth feed — the length-only readers in `resource_cap_policy.rs` and
`plinth-pipeline` — is not decision-relevant and is not touched.

**No digest is claimed wrong.** Each remains exactly reproducible by the harness
that produced it; they measured the zero-field engine, which is what the engine then
was. Re-pinning records that the engine changed, not that the measurement was bad —
which is why every move carries its from→to.

**Nothing else moves.** No cap, gate, floor or bound is touched by this ruling, and
the `make_raking_blade` measurement that was deferred as "named but unmeasured"
lands as part of the re-pin rather than ahead of it.

---

## 6. FU-46 — deletion is out of scope for automatic repair

**The ruling.** **Deletion is OUT OF SCOPE for automatic repair.** A model carrying
zero-volume doubled geometry — a flap, a doubled sheet, a cape, a shell modelled
inside-out and doubled — **stays refused until a real fix exists**. **Thickening** is
named as the natural future candidate: turn a zero-volume double-sided sheet into
something with a printable thickness, rather than silently delete the owner's
geometry. Keep-none stays forbidden in every form, and the conservative keep-first
rule stands as landed.

**Recorded as fact, because it sizes the problem:** **no real corpus model exhibits
the doubled-sheet case.** The reproductions that motivated the row were **synthetic**
(a box plus an 8 × 2 mm double-sided sheet; a box plus a coincident flap), and the
raw corpus's **3 coincident-face groups are single-pair slips** — already handled by
the conservative keep-first rule, and in production removed by the shard strip before
the duplicate action even sees them. So the class this ruling refuses to automate is,
today, an empty class on real input.

**Why refusal is the right resting state here.** A pre-pass that can turn a gate
refusal into a gate pass by deleting the geometry being refused is not upstream of
the gate; it is a hole in it. Refusing is honest until a repair exists that keeps the
owner's geometry.

---

## 7. FU-54 — the cavity-repair re-selection domain is bounded, at eight

**The ruling.** **Bound it.** A **new admitted limit** on whole-grid re-selection
passes, value **EIGHT (8)**, with a **typed refusal** of the
`CavityRepairPassLimitExceeded` shape the row proposed, **registered with a proper
provenance class** like every other admission limit and carried as a caller-owned
field alongside the other H0 admission limits.

**The number, and who owns it.** Measured real models use **1–2** passes — 2 on the
committed `Weeping Treant v1.2` fixture, the raw `Warhorse`, the raw `Giant Toad`,
the raw `Awakened Tree` and the synthetic pinched dumbbell; 1 on every zero-critical-
cell mask. Eight is **4× headroom** over the worst measured case. **The owner owns
the number 8**: it is not derived from anything, and the change that registers it
says so in its provenance rather than manufacturing a derivation.

**Why bounded rather than accepted.** The alternative on the table was an explicit
ruling that a mask-shaped `O(N²)` tail is acceptable because the mask comes from an
already-admitted grid. It is not taken: an unbounded work domain in a stage whose
entire design principle is that work domains are admitted is the one thing that stage
must not contain. Eight is a **ceiling that never bites real geometry**, not a budget
to grow into — the FU-8b lesson, applied in the direction that costs nothing.

---

## 8. FU-37 — the requested wall is persisted on the top-level `HollowReport`

**The ruling.** The requested wall goes on the **TOP-LEVEL `HollowReport`**, as a
**`wall` field** — not on the drilling evidence, which is a record of holes rather
than of the shell's defining parameter. The **affected frozen goldens re-pin
together with the field addition, as ONE gated, documented change.**

**Note only, binding nothing:** that field is the natural future home for
FU-53/FU-55's **realized** wall figure, recorded beside the requested one, so that an
artifact says both what wall it was asked for and what wall it got. This ruling does
not create that second field and does not schedule it.

---

## 9. FU-51 — verify the bores first, then re-record the Sphere baseline once

**The ruling: VERIFY FIRST, THEN RE-RECORD.** A new row is filed —
**FU-61** — for the verification, and `corpus/baseline/Sphere.pipeline.json` is
**not** regenerated until it passes.

**The verification (FU-61).** On the corrected engine, prove that **every drain and
vent bore fully penetrates — outer surface to cavity void, at full bore width** —
against **worst-case placement on the 0.5 mm voxel-stepped cavity surface**.

**The owner's observation that motivates it.** The cavity interior is **voxel-stepped
at `HOLLOW_PITCH_MM` = 0.5 mm** while the **outer wall carries near-full model
resolution**. That asymmetry means a bore drilled to the *nominal* wall depth can
land on a step and open a partial-width hole, or none at all: to guarantee a clean
full-width opening the bore may have to **overshoot the nominal wall, on the order of
a voxel body diagonal (0.5 × √3 ≈ 0.87 mm — "~0.9 mm")**. If the verification shows
an overshoot is needed, the **margin is established and registered** like any other
number the pipeline depends on.

**Only then the regeneration.** After the verification passes — with the fix, if a fix
is needed — `Sphere.pipeline.json` gets **ONE deliberate, recorded regeneration**. Not
a side effect of another change (`development.md` is explicit that the baseline is
evidence and is never regenerated as a side effect); one regeneration, recorded as
such, against an engine whose bores have been proven to penetrate.

**Why not re-freeze now.** The frozen artifact's drain at `z = 0.5` under a stated
2.5 mm wall is a faithful record of a defective field. Re-freezing against today's
engine would replace one unverified record with another; the point of the baseline is
that it is evidence, and evidence is re-cut once, deliberately, against a verified
engine.

---

## 10. FU-36 — the sphere pole seam is fixed now, as a matter of principle

**The ruling: FIX NOW.** Not because the seam is harmful — it is ~1e-16 of the
sphere's area and changes no winding decision — but **as principle: a generator we
own must produce a properly manifold object, symmetric north pole to south pole.**
`sphere_triangles` closes its north cap on `sin(0) == 0` and its south cap on
`sin(PI) == 1.2246e-16`, so every `ContactSphere` and `JointSphere` this project
writes carries a 16-sliver seam. That is our own code emitting geometry we would
refuse from anyone else's.

**How it lands.** The fix gets **its own measured byte-moving pass**, under the
**A/B process recorded in [`docs/specs/chitubox-format.md`](../../specs/chitubox-format.md)** (content absorbed; formerly `05-chitubox-and-corpus.md`)
— the corpus artifacts re-measured against the recorded baseline, per-record byte
movement measured rather than assumed. The seam repair moves the 16 south apices from
`r × 1.2246e-16` to `0.0` in x and y; whether that reaches the wire is per-record
(f32 rounding away from the origin, and a triangle-count change if degenerate slivers
are dropped rather than kept), which is exactly why it needs its own pass and could
not ride along with a bit-identity relocation.

**Afterwards the bound is TIGHTENED.** The `1e-12` closure bound in
`every_emitted_soup_is_closed_which_the_winding_decision_requires` exists **solely**
to accommodate this seam. Once the seam is gone the accommodation is gone with it and
the bound comes down. **The bound is never widened** — if a future closure defect
exceeds it, the geometry is wrong, not the bound.

---

## What this batch does not do

- **It implements nothing.** No crate, test, fixture, golden, baseline or artifact is
  touched by the commit that records these rulings.
- **It moves no number.** FU-54's `8` and FU-37's `wall` field are *decided* here and
  *registered* by the changes that build them, with the provenance those changes owe.
- **It renumbers nothing.** FU-61 is filed at the next free number under the recorded
  collision protocol; every existing row keeps its number, and each ruled row is
  annotated above its original text rather than rewritten.
