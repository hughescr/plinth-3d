# Decision — the legacy 13 are escorted to the exact tier, not merely enumerated

**Implementer's ruling, 2026-08-10, recorded because it deviates from a recorded
design.** The dense orientation sweep's funnel is fixed by decision **D-12**
([`02-target-architecture.md:1103`](../02-target-architecture.md), detailed at §6,
restated in the M8 table at [`04-migration-plan.md:1660-1668`](../04-migration-plan.md)):
coarse = the whole candidate grid on the ~2 % proxy, refine = the top
`ORIENT_REFINE_TOP_K` on the ~10 % proxy, commit = the top `ORIENT_COMMIT_TOP_K` at
`Fidelity::Exact`. The legacy 13 candidates are kept in the grid permanently, in D-12's
own words, **"so the sweep is never worse by construction."**

**As built to the letter, it is not.** The commit tier's candidate set now additionally
contains the whole legacy block, whether or not any proxy promoted it. Neither recorded
funnel width moved; nothing else about the funnel changed.

## Why — the measurement, not the argument

The committed score is computed at the exact tier and **only** there (02:852). So a
proxy that mis-ranks a legacy candidate out of the top `ORIENT_REFINE_TOP_K` discards
that candidate before the tier that decides ever sees it, and D-12's guarantee silently
degrades from a property of the candidate set into a hope about proxy fidelity.

That is not hypothetical. It is what the committed `Sphere.prepped.stl` does:

| | unescorted | escorted |
|---|---|---|
| committed winner | a Fibonacci direction | the legacy **X −20** pose |
| committed score (exact LOD, all four terms) | **16.3173** | **16.3009** |
| that pose's rank on the L1 proxy | 478th of 643 | 478th of 643 |
| that pose's rank at the refine tier | never scored | never scored |

Unescorted, the sweep committed to an orientation **0.1 % worse than the search it
replaces**, on a corpus model, in the direction the record explicitly says cannot
happen. The escorted run instead reproduces the pose the committed `Sphere` baseline
records — `{axis: [1,0,0], deg: -20}` — which is a stronger outcome than the design
asked for and was not available any other way.

The mechanism is the L1 proxy, not the objective: 476 faces is a coarse sphere, and the
peak cross-section of a near-tied family of directions is exactly the quantity a coarse
tessellation gets wrong. Widening `ORIENT_REFINE_TOP_K` was considered and rejected — it
is a recorded number, 478 is nowhere near any plausible K, and buying this one guarantee
by raising a cap is precisely the move the hard constraints forbid.

## What it costs, stated

At most 13 additional candidate scorings at the exact tier, against a
`ORIENT_COMMIT_TOP_K` of 3. On the Weeping Treant v1.2 (429,120 faces) the entire
three-tier sweep including the escort measures **3.07 s**, against that model's ~470 s
support run — about 0.65 % of its pipeline. On every mini it is single-digit
milliseconds. The escort is counted separately in the record
(`TierRecord::escorted`), so a reader can always see how many candidates reached the
exact tier by promotion and how many by right.

## What it does not change

- **No gate, cap, bound or registered width moved.** `ORIENT_REFINE_TOP_K` is still 24
  and `ORIENT_COMMIT_TOP_K` is still 3; the escort is an addition to the commit tier's
  input set, not an enlargement of either K.
- **The tie-break is untouched**, and is in fact strengthened: identity is `CandidateId`
  0 and is now guaranteed to be scored at the exact tier on every model, so the legacy
  identity-wins-ties semantics that two committed baselines depend on holds at the tier
  that decides rather than only at the tier that ranks.
- **No committed artifact, baseline or digest moved.** The sweep reads the corpus
  fixtures and writes none of them.

## The calibration gates this does not replace

D-12's recorded safety valves — exact-best inside the top-24 or K doubles, outside the
top-48 after one doubling triggers the recorded fallback ladder, Spearman ≥ 0.7 or the
sweep is demoted (02:856-857) — are **not** built in this slice and are not superseded
by the escort. They measure proxy fidelity across the whole candidate set; the escort
protects exactly one block. The Sphere's 478th-place ranking is a data point those gates
would have flagged, and it is recorded here so that when `plinth calibrate` lands it has
a known case to reproduce. Filed as a follow-up rather than quietly absorbed.
