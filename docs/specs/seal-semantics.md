# Seal semantics — the Routing/Brace contract

Status: **RATIFIED** (owner decision 2026-08-22, on the design record this file
replaces: `seal-semantics-2026-08-22.md`, Option 2 "keep `Routed` strict, make
it reachable"). Landed on `develop`.

This is the current contract for what a sealed support-routing record means,
how a brace-stage refusal of a sealed terminal is handled (rescue via
legalization retry, or abandon), how whole-class refusals are reported, and
what the evidence sidecar discloses. It states the contract **as built**, not
the design debate that produced it (Option 1 "two seals" and Option 3
"constrain the resize" were both considered and declined; see the design
record's history if the rationale is needed).

Section labels below (`Stage A`, `Stage A(b)`, `Stage B` piece numbers) are
cited by exact label from code comments (`crates/plinth-support/src/lib.rs`,
`legalization.rs`, `coverage_repair.rs`, `forest.rs`, `cco.rs`,
`crates/plinth-pipeline/src/coverage_evidence.rs`) under this file's own path,
`docs/specs/seal-semantics.md`; keep the stage/piece labels stable across any
future edit of this file.

## Vocabulary

- **Seal** — a record written once during a run and never changed afterward.
  Later stages must agree with it; if one disagrees, the run stops.
- **The audit** — the check that every support point's sealed record and its
  live state in the tree agree.
- **Legalization** — throwing away a planning pass that produced an illegal
  support and planning again, steered away from the arm that failed.
- **Steering no-good** — the record kept when an arm is measured illegal:
  *this support point was seen to fail with this arm once; prefer a different
  one next time.* It changes which arm gets **tried**. It is not an answer,
  and on its own it is never a reason to give up on a support point.
- **Confirmation round** — the only thing allowed to give up on (retire) a
  support point. Everything else is held still; the one point tries each arm
  it has left, one arm per planning pass. If any of them works, the run
  carries on. Only if every one of them is measured inside the model does the
  point retire `UnroutedNoWinner` — in the same pass that measured the last
  arm, so the verdict and the measurement come from the same world.

## The sealed-`Routed` contract

Sealed `Routed` means exactly this, and nothing weaker: **this terminal's
contact is in the final accepted contact set, and this terminal is
`Consumed` in the tree the run finally ships.** `finalized_unrouted_terminals`
admits exactly three pairs — `Routed`↔`Consumed`, `SkippedQuarantined`↔
`Quarantined`, `UnroutedNoWinner`↔`Retired` — and refuses every other pairing
as `TerminalEvidenceMismatch`. No fourth pair exists. `Routed` is never
downgraded to "routing's decision only" (the declined two-seal option), and
nothing is admitted into the accepted set after the fact.

## Stage A — classify and abandon, never mint the illegal pair

Before a brace-stage sweep quarantines anything, the sweep's condemnation set
is planned read-only and checked against the sealed terminal record:

- **Empty intersection** — no sealed terminal is disturbed — the sweep applies
  exactly as it always has.
- **The intersection contains a sealed-`Routed` terminal whose own chosen arm
  is among the condemned members** — nothing is quarantined. The pass is
  **abandoned** and a named termination is returned carrying that terminal,
  its measured clearance margin, and its assigned radius, plus any cascade
  victims labelled as victims (not steered — see below).
- **The intersection contains sealed-`Routed` terminals but none by its own
  chosen arm** (a brace rung, or a cascade that only takes down terminals that
  didn't choose the condemned member) — nothing is quarantined and nothing was
  learned. This is the one named refusal the mechanism keeps: **a pass that
  learns nothing refuses by name**, because replaying it would reproduce the
  identical pass.

**Cascade-victim rule.** A terminal disturbed purely by cascade — its stub
re-parented onto another terminal's condemned arm — never receives a no-good.
Its own arm was never measured illegal; giving it one would punish it for a
different member's failure and let it retire with no arm of its own ever
shown illegal.

**Stage A(b) — the read-only classification query must be an exact prefix of
the mutation it precedes**, not a second, independently-written walk that can
drift from it. `quarantine_subtrees` is split into a planning half (every
step up to and including its second `require_active` sweep) and an apply
half; the classifier calls the planning half, the sweep calls both. A
directly-named inactive refusal key still surfaces as a `ForestError` from
the planner, unchanged from before this contract — that is a distinct engine
defect, not a physical refusal, and Stage A does not reclassify it.

## Stage B — the legalization retry (attempt 0 only)

The retry runs only around the model's own first (attempt-0) plan. A repair
trial that fails bracing is still dropped and the next candidate tried,
exactly as before this contract — the retry is never wrapped around a trial.

Five pieces:

1. **The steering channel.** The routing tournament takes a borrowed no-good
   set. A suppressed candidate is passed over with a named reason. Passing
   over is all it does — a terminal whose candidates are all suppressed does
   not retire here; it signals that its steering is spent, which is what
   opens a confirmation round (piece 5).
2. **Candidate identity.** A no-good is keyed on the arm's endpoint
   coordinates by exact bit pattern, together with the candidate-evidence
   fields the tournament already discriminates arms by (family, generation
   index, price, clearance rows). Endpoints alone would over-match and
   suppress a second, distinct candidate that happens to share them.
3. **The abandon outcome.** A brace refusal that satisfies the abandon rule
   (a sealed-`Routed` terminal's own chosen arm was condemned) returns the
   context plus the no-goods the pass learned, instead of quarantining.
   Cascade victims travel in the payload as victims and steer nothing.
4. **The loop.** One retry loop wraps the attempt-0 chain call only. It holds
   the no-good set, restores the prepared digests on every abandoned pass, and
   matches each no-good's context tuple before honoring it — a mismatch is a
   **miss**, never an error (a digest mismatch always regenerates and
   re-measures the candidate from scratch rather than trusting a stale
   record).
5. **The confirmation round** — the only thing allowed to retire a terminal.
   When a terminal's steering is spent, the loop freezes the no-good set and
   drives that terminal through its remaining arms, one per pass, in the
   tournament's own order: (a) the round ends and the pass proceeds normally
   on the first arm that survives bracing; (b) the terminal retires
   `UnroutedNoWinner` in the pass that measures the last arm, if every arm is
   measured illegal; (c) the round is **void** if any other terminal's arm is
   condemned during it — the pass abandons on that terminal's new no-good and
   the round re-enters later. A round's per-arm margins and radii are
   recorded in the sealed no-winner evidence together with the round's
   identity; steering-history margins recorded before the round are labelled
   separately from the round's own measurements.

**A structural requirement the termination argument depends on:** the no-good
set, and anything derived from it, must never be an input to any digest in
the context tuple a terminal's tournament reads. If it leaked in, every
context would be fresh on every pass, every no-good would miss forever, and
the same candidate would be chosen and condemned every pass while the abandon
rule waved each pass through as progress — non-termination the abandon rule
cannot catch. This is tested directly: growing the no-good set arbitrarily
must leave every digest in the context tuple byte-identical.

**Why steering notes may never retire a terminal on their own.** A no-good's
key covers the state a terminal's tournament reads (its *prefix*); the brace
verdict that made an arm illegal is decided by a radius assigned from the
whole braced forest, which depends on terminals routed **after** this one
(the *suffix*). A pile of true measurements collected from different passes
says nothing about whether an arm is legal in any one world, because each
pass replaces the world the previous one was measured in. Only a completed
confirmation round — which holds the world still while it measures every
remaining arm — establishes that nothing the terminal could have chosen was
legal.

## Stage C — invariant hoisting (paid for before Stage B, not after)

The per-pass geometry work that a legalization pass repeats (Routing and
Rewire, together 81–85% of one pass's wall time on the measured Giant Bat
fixtures, 2026-08-22) is hoisted to depend only on the model and
configuration digests, not recomputed on every pass. This lands before Stage
B specifically so every extra pass Stage B introduces is paid at the reduced
price, not the pre-hoist price.

## Stage D — sidecar disclosure

The no-good ledger, the pass count, and the confirmation-round record join
the existing coverage-refusal evidence sidecar. This is disclosure only, not
the record of truth — the round's measured margins and radii already live
inside the sealed no-winner evidence written in Stage B, so a defective
sidecar cannot hide a missing measurement or a retirement that skipped a
round.

## Whole-class refusal reporting (no-cap doctrine)

Every run reports its legalization pass count, no-good count (overall and
per-terminal), confirmation-round count, and voided-round count. These
counters are printed on every path and decide nothing — no branch reads them.
There is no cap on legalization passes and no cap on a confirmation round's
length: a round's length is the terminal's own remaining-arm count, and it
ends early on the first legal arm. If the uncapped cost proves intolerable on
a real model, the answer is a resize-timing fix or an owner ruling, not a cap
arriving by the back door.

## What this contract does not claim

- **Not** that every model emits. A terminal with no legal arm anywhere
  retires `UnroutedNoWinner` (one of the three admitted pairs) after a
  completed confirmation round; the run either emits with that hole disclosed
  or refuses with the hole named in millimetres.
- **Not** that a steering no-good is valid against any world other than the
  one it was measured in. It may steer a choice and may never decide an
  outcome — no retirement, no removal, no refusal rests on a no-good count
  alone.
- **Not** that the retry loop is bounded by any specific pass count. It is a
  finite backtracking search (each abandoned pass adds a triple that was
  never added twice, over a finite set of reachable triples), but the bound
  is combinatorial and is not a fixed number — several terminals failing in
  one pass multiplies the search rather than adding to it.
- **Not** a second admitted pair, a second removal point, or a resurrected
  ranking rule. None of those exist in this contract.

## See also

- [`docs/plans/support-clearance.md`](../plans/support-clearance.md) — open
  follow-on work this contract's measured cost band feeds (task #51 and the
  resize-timing family).
- [`docs/physics.md`](../physics.md) — the clearance and land-at-strike rules
  a brace-stage refusal is measured against.
