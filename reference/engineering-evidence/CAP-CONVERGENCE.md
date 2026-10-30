# Patch cap: convergence verdict and owner decision (2026-10-28)
Read-only assessment at branch HEAD 319d827e (gpt-astra high). The orchestrator copied it from the handback.

## Why 8 rounds did not converge
Three constraints are being solved separately although each depends on the others:
- **contact geometry:** the connected patch must be ≤ 0.8 mm, with no contacts fused across members;
- **capacity:** the real contact and every section must carry its load;
- **structure:** stable, clear arms, feet, joints and braces.

The interface returns a radius, not a feasible contact-and-load assignment. Its failure falls back to a ball-only radius (down_sweep.rs:5301–5335), and late stages replace geometry that was already measured. Insertion keeps the host fixed and spaces candidates by the largest ball, so it cannot find a jointly resized pair or a different route.

## Blockers and their smallest correct fixes
- **(a) Members 166/170.** 78/78 and 31/31 candidates are refused by the host's fixed 1.117 mm spacing, leaving 20 overloaded arm-layers.
  - Fix: a jointly load-sized host/sibling proposal with candidate-specific reservations, committed after a load replay (down_sweep.rs:3940, 4795–4825, 4968–4975, 5022–5090).
  - Otherwise: move or reassign the host, or use another surface, layer or route.
- **(b) Feet.**
  - Foot proposals must remeasure their capped contact before structural acceptance, and a foot short of capacity must trigger load reassignment (model_feet.rs:396–418, 460–474).
  - Member 186 needs a real brace or a shorter or rerouted support.
  - Re-land foot 61 or share its load.
  - Reroute 162: its tip, shaft and foot make one 1.591 mm patch.
- **(c) Raisers.** Certified head resize (down_sweep.rs:997–1009), the hand-down (slenderness.rs:7118–7124), the 9 fallbacks (they become explicit unresolved contacts that need a new seat, approach or route) and member 170 after its late slide and head rebuild (lib.rs:5489–5511, 5574–5599). All go through one transaction.
- **(d)** The clusters 58/59/60 and 69/70/81 need separated assignments. The transaction closes before the final census (lib.rs:6030–6064).

## Replacement
A shared contact-budget TRANSACTION:
1. Each pass submits its contact-affecting proposals; no pass keeps them directly.
2. Batch-draw the affected field.
3. Measure the complete contacts and the reservations.
4. Reroute and recompute the load.
5. Check structure and coverage.
6. Commit everything together.

Early sizing consults the same authority. The final written-file audit stays read-only. No final shrink-and-patch loop.

## Plan (at most 2 rounds)
- **Round 1:** the interface, candidate-sized split reservations, foot-capacity triggers, invalidation on late geometry changes, and real-writer witnesses for all raisers, connected self-contact, neighbour fusion and retained load. Each guard is sabotaged and must show path hits.
- **Round 2:** resolve the Deinonychus subjects, then run one candidate check.
  - Acceptance: discs AND Deinonychus have CS = FU = 0, with no unresolved fallback and no new overload, structure, clearance or coverage subject; feet are valid and load is conserved.
- If either round misses its boundary, STOP this line of work.

## Owner decision (Craig, 2026-10-28)
"Redesign it properly", chosen over merging the disc win first or parking the work. The current branch (319d827e) is reference material, not the base, unless the detailed design says otherwise.
