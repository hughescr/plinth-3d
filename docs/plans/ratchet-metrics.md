# The ratchet metric set

A ratchet is a recorded number, per corpus model, that says how GOOD a support plan is. A
change may improve it freely and silently. Worsening it needs an argument, and the argument
is about a quantity with a physical meaning rather than about a hash.

Ratchets sit between the other two kinds of check and replace neither:

- **Invariants** are permanent and placement-independent. A broken invariant is a bug.
- **Ratchets** are the six numbers below. They may move; one direction is free.
- **Equivalence checks** are run at the time a specific change claims "the supports did not
  move", and are recorded in the commit that claims it.

Every ratchet below is a fold over data the run already computes. None adds a geometry pass,
and each fold is a single pass over an existing slice.

## The six metrics

### 1. Worst tip area ratio

`max over emitted contacts of carried_area_mm2 / (pi * tip_radius_mm^2)`, dimensionless,
**lower is better**.

**On the plate.** This is the measured area law. A tip carrying more model area than its own
cross-section can hold snaps during peel, and the model comes off the plate or tears. The
allowance is the registered `X_ALLOWED = 12`; field failure was measured near 25. The MAXIMUM
is the metric, not the mean, because the print fails at the narrowest section — one bad tip
loses the model, and an average hides it.

**Where it lives.** `EmittedContactDisclosure` carries `carried_area_mm2` and `tip_radius_mm`
per emitted contact (`crates/plinth-support/src/runner.rs`, produced by
`emitted_contact_disclosures`). The max-fold must be added.

**How to game it.** Fatten every tip. The ratio falls and the print gets worse: bigger scars
on the model surface. Or drop the one contact whose ratio is worst; the max improves while
the model loses the support that was working hardest.

**What blocks that.** Metric 2 (total scar area) rises when tips fatten, so the two pull
against each other. Dropping a loaded contact is blocked by the invariants: abandoned carried
area must be zero, and no demand-bearing contact may be left unrouted.

### 2. Total model-facing scar area

`sum over emitted contacts of pi * tip_radius_mm^2`, mm², **lower is better**.

**On the plate.** Every tip leaves a mark on the model that has to be cut and sanded off. Scar
area is the surface damage the operator pays for by hand, and on a face-sized mini it is the
difference between a usable print and a ruined one.

**Where it lives.** Same disclosure as metric 1. The sum-fold must be added.

**How to game it.** Shrink every tip, or place fewer contacts. Both make the print worse: thin
tips snap, and fewer contacts means more model area per tip.

**What blocks that.** Metric 1 rises as tips thin or as contacts are removed, because carried
area is redistributed over the survivors. Metrics 4 and 5 rise when contacts are removed from
places that needed coverage. The per-contact area-law invariant is the hard limit underneath.

### 3. Support resin volume

`sum over final live members of frustum_volume_ml(span, terminal_radius, rootward_radius)`,
mL, **lower is better**.

**On the plate.** Resin in scaffolding is resin not in the model, plus print time, plus
cleaning and disposal. Across a four-plate run the savings multiply.

**Where it lives.** `member_sizing::frustum_volume_ml` already computes it for one member;
`SupportRunSnapshot::final_live_members()` already exposes every member's endpoints and both
radii. The fold must be added.

**How to game it.** Make the scaffold thinner or shorter everywhere until it cannot carry the
model.

**What blocks that.** Metric 1 catches thin tips. The invariants catch the rest: a closed load
path from every support to the plate or to the model, and the member-sizing minima. A support
tree that saves volume by not reaching the plate fails an invariant, not a ratchet.

### 4. Residual uncovered area at seal

`sum over remaining coverage patches of uncovered_area_mm2`, mm², **lower is better**.

**On the plate.** Uncovered overhang has nothing under it while it cures. It sags, curls, and
drags on the film. Blocking patches must already be zero at seal — that is an invariant — so
this metric tracks the sub-censor dust the seal is allowed to leave behind. Less of it is a
better plan.

**Where it lives.** `CoverageMeasurementPatchEvidence::uncovered_area_mm2`, already serialized
into the coverage report's `evidence.patches`. The fold must be added.

**How to game it.** Loosen the censor, or widen the probe radius, so patches stop being
counted.

**What blocks that.** The metric is only comparable when it is measured at the REGISTERED
request. That is already an invariant: the sealed model is re-measured against the registered
censor and probe radius, independently of what the controller claims. Recording the request
alongside the ratchet makes a censor change visible as a re-baseline rather than as an
improvement.

### 5. Worst uncovered gap

`max over remaining coverage patches of worst_gap`, mm, **lower is better**. A patch reporting
`NoRoutedContact` is a hard failure, not a large number.

**On the plate.** Sag scales with unsupported SPAN, not with unsupported area. One long thin
unsupported strip and a scatter of dots can have the same area and completely different
outcomes: the strip droops, the dots do not.

**Where it lives.** `CoverageMeasurementPatchEvidence::worst_gap`, already in the report. The
max-fold must be added.

**How to game it.** Split one long uncovered strip into several shorter patches by adding a
contact in the middle of it and nothing else, improving the max while the total stays.

**What blocks that.** Metric 4 does not improve from a split, so the pair together says
whether coverage actually got better or was only rearranged. Metrics 2 and 3 charge for the
added contact.

### 6. Ownerless model area

`sum over disclosed ownerless islands of ownerless_face_area_mm2`, mm², **lower is better**.

**On the plate.** This is model area that the load repartition left with no eligible carrier —
area no support is officially holding. It is the area most likely to be found unsupported when
the model comes off the plate in pieces.

**Where it lives.** `OwnerlessIslandDisclosure::ownerless_face_area_mm2`, reachable through
`SupportRunAuthority::ownerless_islands()`. The fold must be added.

**How to game it.** Stop disclosing. An ownership tally that declines to report a stranded
island reads as zero.

**What blocks that.** The disclosure invariant: every ownerless island must be disclosed with
finite, positive, meaningful numbers, and the coverage measurement must still be allowed to
run and rule rather than being pre-empted by an ownership tally. Both already hold today.

## The invariants these ratchets lean on

A ratchet is only safe if the thing it could be gamed into is blocked elsewhere. These are the
invariants named above. Several exist today; the ones marked MUST BE ADDED are the price of
adopting this set, and each must be added and shown to catch its defect BEFORE any pinned
check it replaces is retired.

| Invariant | Status |
|---|---|
| Abandoned carried area is zero | exists (`support_runner_integration`) |
| Sealing means zero blocking patches | exists |
| Growth rung radius never exceeds the registered scar ceiling | exists |
| Sealed model re-measured at the REGISTERED censor and request | exists |
| No two routed contacts share a seat position | exists |
| Ownerless islands are disclosed with finite, positive numbers | exists |
| Per-emitted-contact area law: `pi*r^2 >= carried_area / X_ALLOWED` | MUST BE ADDED — today the area law is asserted only engine-wide, in `constants_placeholders.rs` |
| No demand-bearing contact left unrouted | MUST BE ADDED — the ledger exists (`UnroutedSpecializedContact`); nothing fails on it |
| Closed load path from every support to the plate or a model seat | MUST BE ADDED |

## Tolerance

**Worsening beyond tolerance fails. Improvement of any size passes, and the recorded value is
updated in the same commit that improves it** — an unratcheted improvement leaves the check
looser than the code deserves.

- **Integer-valued quantities: exact.** There is no floating-point wobble in a count.
- **Floating-point quantities: worse by more than `max(1e-12 * recorded, 1e-9 unit)` fails.**

The relative part is a naive-summation bound. Summing `n` terms in a fixed order has worst-case
relative error about `n * eps`, with `eps = 2.220446e-16`. The largest corpus fold is over the
Weeping Treant's contact and member sets, order 10³ items; taking `n <= 4096` as headroom gives
`4096 * 2.220446e-16 = 9.1e-13`, which rounds to 1e-12. Anything larger than that is not
summation noise. Metrics 1 and 5 are maxima, not sums, so 1e-12 is generous for them by three
orders.

The absolute part exists because a relative tolerance around zero is meaningless, and metrics
4 and 6 are legitimately zero on a good plan. 1e-9 mm² is six orders below the printer's own
resolvable area — one 34.4 µm pixel is about 1.2e-3 mm² — so a movement that small cannot
correspond to anything that happens on the plate. The same 1e-9 is used for mL on metric 3 and
mm on metric 5, for the same reason.

Order of summation must stay fixed, which the run already guarantees: the same input produces
byte-identical output at 1, 4 and 16 threads, and that determinism check is separate from these
ratchets and must keep running.

**Ratchets are keyed to the corpus file digest.** The corpus STL SHA-256 digests are already
pinned in `crates/plinth-decimate/tests/corpus_budgets.rs`. A corpus file that changes
re-baselines every ratchet for that model, and this is not a loophole but a requirement:
contact candidates are seeded from per-vertex local z minima, so a retessellated mesh yields a
different candidate set and a different plan of equal quality. Comparing a ratchet across two
different meshes compares nothing. This is exactly the "stable under irrelevant change" trap;
the key makes the irrelevant change visible instead of silent.

## What the ratchets do NOT cover

Stated plainly, because a mechanism that is trusted past its evidence is worse than none.

- **They cannot say a plan is good.** Six numbers can all improve while the print gets worse:
  a support seated on a face, a scar across an eye, a tree that is impossible to cut away
  without breaking a wrist. Nothing here sees the model as an object anyone looks at.
- **They are blind to placement aesthetics and removal.** No metric knows which faces are
  visible, which are inside a cavity, or how hard a support is to reach with cutters.
- **They are blind to orientation-coupled physics.** Peel direction, suction and cupping,
  drainage, and where a model sits in Y are not represented. A plan can hold the same six
  numbers and behave differently on the plate.
- **They are corpus-local.** Eight checked-in models. They say nothing about the next model.
- **They prove nothing about a placement theory.** Every placement improvement stays
  hypothetical until a plate is printed and scored. That is what the ratchets are FOR: they
  make regression detection cheap so printer time is spent on the question only a print can
  answer.
- **They do not replace invariants or equivalence checks.** A ratchet that improves while an
  invariant breaks is a bug that shipped, and a performance change still has to prove
  byte-identity at the time it claims it.

## Metrics considered and rejected

- **Support or contact count.** The textbook bad ratchet: it improves by removing supports
  until the model falls off. It is also seeded from local z minima, so it moves under
  retessellation without the plan changing.
- **Snapshot digest.** It has no direction. A digest cannot be better or worse, only equal or
  not, which makes it an equivalence check.
- **Coverage repair rounds.** A compute-cost proxy, not a fact about the plate. Whatever a
  repair round appended is already charged to scar area and resin volume. It belongs to the
  performance lane's equivalence checks.
- **Abandoned contact count.** A count of refusals has no direction either; refusing a contact
  that carries nothing is free. The physical quantity is abandoned CARRIED AREA, which is
  already an invariant pinned at zero.
- **Resource ceilings** (route memberships, indexed points, distance work, peak bytes). Real
  and worth watching, but they are budget facts, not print-quality facts. They belong to the
  performance lane.
- **Per-patch area and gap tables.** Evidence, not scalars with a direction.
- **Mean tip area ratio.** An average hides the one tip that snaps. The failure mode is the
  narrowest section, so the metric is the maximum.
- **Member count or total member length.** Proxies for resin volume that move with tree
  topology without any resin changing.
