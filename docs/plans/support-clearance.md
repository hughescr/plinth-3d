# Plan: support clearance (land-at-strike)

The ratified physics this plan implements — the two-tier clearance demand, the seat's own
contact zone, and the four land-at-strike rulings (R1–R4) — is recorded in
[`docs/physics.md`](../physics.md) §6–7. This page tracks only the implementation work still
ahead of it.

## The problem this closes

**Escalation E-E, MEASURED 2026-08-21** (Giant Bat, two independent probe runs, byte-for-byte
agreeing logs): for a contact-declaring member whose Brace-stage clearance verdict is decided
by a radius the Brace stage itself assigns, refusing the doomed arm at candidacy time is
**unavailable** — the deciding radius does not exist yet at candidacy, and the divergence
between the candidacy radius and the eventual Brace radius is not one-sided (it grows on some
arms, shrinks on others), so no conservative bound can substitute for it either.

This is not a claim that candidacy-time refusal never works — a contact-declaring member
refused on inputs that all exist at Routing is untouched by the argument. It is specific to
the class whose Brace verdict turns on a Brace-assigned radius.

## The decided direction

Of the three-option menu the finding raised (refuse the resize that drives a member negative;
re-open the tournament for terminals whose carrier the resize condemned; or admit a fourth
sealed-accounting pair — the last one is forbidden by the existing seal design), the first is
the one being implemented: **land-at-strike**, truncating a stub's shaft at the point it would
re-enter the model, with a load-sized anchor.

## Remaining implementation work, in dependency order

1. **Task #52 — land the stub-axis edge-on-face fix.** When a support surface is nearly
   edge-on to the support's own direction, the shaft can poke back into the model and get
   condemned before any real arm is tried. Design is finished and its blocker has cleared;
   implementation is next. As of this restructure, `grep` confirms the strike-termination
   mechanism itself is not yet in `crates/plinth-support/`.
2. **Task #24 — fault group B.** An island whose every candidate support is smaller than the
   seat-placement grid currently kills the model outright. This is implemented via #52's
   mechanism and is the dominant open corpus obstacle (see
   [`docs/status.md`](../status.md)).
3. **Task #41 — re-land the FU-87 minimum-arm-length screen.** A previously-withdrawn safety
   screen (rejecting supports too short to actually work); must land *after* #24, since
   landing it earlier crashes other models.
4. **Task #47 — re-check the Giant Bat.** Its original crash is fixed by the rulings above,
   but whether it now actually seals under the current repair controller has not been
   re-measured.

## Closed, for the record

The footprint search that preceded land-at-strike — trying to find an input-side rule
(projection, normal, crease, adjacency, concentric band, or "Rule Z") that separates a
crevice's far wall from the seat's own receiver on an unlabelled mesh — is **closed**. Root
cause: on an unlabelled mesh, burial is approximately connectivity, so no such rule can tell
the two apart there. See `docs/physics.md` §6 for the measurement and Craig's 2026-08-25
ruling that accepted the resulting behaviour instead of continuing to chase a rule. Do not
resurrect Rule Z or a deprioritization-by-ranking scheme inside a condemned seat's coverage
disc — the latter was built, MEASURED HARMFUL (a 0.03–0.46 mm cluster spacing against a 6.0 mm
score radius), and reverted.

## Evidence

Full derivation, the reproduced Giant Bat measurement, and the reproduction recipe are
preserved in the archived design record this plan was absorbed from
(`.git/candidacy-exact-evidence/`, `.git/chal36-challenge-evidence/` — probe logs, out of
tree by size). No cap, gate, threshold, or registered constant moved to produce this finding.
