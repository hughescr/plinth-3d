# Decision — FU-82 closed without measurement

**Ruling given 2026-08-13 by the owner (Craig Hughes).** [FU-82](../CURRENT-STATUS.md#follow-ups-2026-08-04)
— whether the clearance-measuring grid's world anchoring lets a model's sub-voxel
grid phase shift a clearance reading enough to flip a hollow/no-hollow verdict — is
**closed without measurement**.

## Why

Paraphrased faithfully: the shifting question is irrelevant. It may make a
difference at the margins, but the margins don't matter — if a small percentage of
models sometimes hollow and sometimes don't depending on pose, that doesn't matter.
The overall resin savings at stake would not be significant compared to the time
being spent chasing the question. Cut it off.

## Separately ruled: the refusal rate is correct behaviour, not a defect

Hollowing is for minis representing Large-size D&D creatures and bigger. Medium
minis are ~30 mm tall — there is not enough interior to be worth removing, and
hollowing them would leave walls thin enough to cause support problems. The
32-of-37 refusal rate at the bulk gate ([FU-62](../CURRENT-STATUS.md#follow-ups-2026-08-04))
is the design working as intended. A bulk-gate refusal on a small mini is expected
and is not, on its own, a finding to chase.

## What this does NOT license

**No gate, cap, floor or threshold may be loosened to raise the hollow rate.** This
ruling explains why refusals are common; it does not license relaxing any of them.

## What was established before the halt

Measured, not assumed, by the unmerged FU-82 census work before this ruling:

- Bulk-gate margin moves across grid phases: Myconid Adult **8.677–8.888 mm**,
  Zombie Cat swing **0.338 mm**, Round 25 swing **0.162 mm**.
- The corpus's closest measured approach to the 10.0 mm bulk floor is **~1.1 mm**
  (Myconid Adult) — roughly **5x** the largest observed phase swing, i.e. too far
  for a grid-phase shift alone to flip a verdict on this corpus.
- Of the 37-mini corpus: **32** refuse at the bulk gate, **4** at the savings gate,
  and **1** (Giant Toad) at a drain-planning cap — measured at **9,116,309,310**
  bytes requested against the **1,073,741,824**-byte cap. Recorded as a finding;
  the cap was **not** raised.

## What is left unmerged

`feature/fu82-census` (off `develop` @ `81dbb8d`) holds completion of FU-83's
evidence seam — a flow-selection evidence channel plumbed to
`H0ResourceDiagnostics` — plus the phase-ambiguity census harness and engine
tests, independently verified three times as additive with no registered constant
moved. It is available if ever wanted. It is merged into nothing, and this closure
does not ask for that to change.

## FU-83 stays open

FU-83 was filed explicitly as a prerequisite for FU-82's census, but its
underlying ask — production diagnostics reporting how FU-47's connectivity rule
decided, which today no production run reports at all — has merit independent of
the now-closed census. It is not closed by this ruling.
