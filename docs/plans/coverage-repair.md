# Plan: coverage repair

The SPEC-A4-conformant, transactional one-winner coverage-repair controller (task #39) is
**built and live on `develop`**. It replaced an uncapped, per-patch-capped search with a
strictly-improving, globally-scored, spatially-diverse one, under Craig's rulings R3–R13
(global scoring, no failed-trial cap, spatial diversity after rejection, and others). This
page tracks what is still open about it — the parts of the design nobody has finished,
measured, or gotten a ruling on — not the mechanism itself.

See [`docs/status.md`](../status.md) for how much of the corpus this currently blocks (the
zero-commit walk, tasks #24/#37/#39) and
[`docs/plans/support-clearance.md`](support-clearance.md) for the clearance-side mechanism
(land-at-strike, task #52) that #24 depends on.

## Rulings needed from Craig

**Reserved budget share vs. heavy refusers (task #39's O-3 finding — not yet its own backlog
task).** MEASURED: no currently-emitting model exceeds the commit budget of 8, so the
controller starves nothing that emits today. But every heavy refuser exceeds it badly —
Awakened Tree 103 distinct routes at attempt 0, Giant Toad 54, Giant Bat 51, Rotting Warhorse
38, Warhorse 27 — so at most 8 of the tree's 103 routes can ever receive a contact under the
current bound. Not urgent (it only binds models that already refuse for other reasons), but
it is a real limit on exactly the models this controller exists to unblock, and it needs a
ruling: a reserved share of the budget for the largest route, or an accepted limit.

**The "hard failure" scope (design item O-5).** SPEC §6 reads "an unrouted specialized
minimum or island-birth contact is a hard failure" as applying to the *accepted* plan; the
landed code (`runner.rs`) measures and ledgers this case *without* refusing. Two questions,
in order, before this is adopted or closed: (a) is the measure-and-ledger behaviour the
intended reading of "hard failure," or a staging step? (b) if staging, how many corpus models
does turning the refusal on actually cost? That is one corpus run, and it must be answered
before the rule changes, not after.

## Recommended, not yet done

**Design item O-6 — clause 3 as a spec amendment.** The controller's anti-gaming guard (a
committed step must strictly reduce total uncovered sample area, uncensored) is currently a
house rule filling a hole SPEC §4/§6 leave open. One sentence added to §6 would make it
contract instead. Low cost, not yet written.

## Not yet measured — Prove-stage obligations

- **Diversity-demotion telemetry on real models (O-1′).** The spatial-diversity rule (N4,
  ruling R13) replaced a failed-trial cap; its effect on the Warhorse and the treant class is
  not yet measured. Report `diversity_demotions` and the trial count on every model and bring
  the numbers back.
- **Two loop guards, unexercised by any fixture:** the zero-score stop guard (no procedural
  fixture has an all-zero-scoring pool) and `CommitBudgetExhausted` (no fixture needs more
  than the bound). Both are reachable by construction; neither is proven. A corpus run should
  look for a model that reaches each.
- **Two rare-path disclosures, unproven.** The attempt-0 `MeasurementRefused` return and the
  `rank_or_refuse` error-return path both now finish and print their report rather than
  discarding it, but no procedural fixture reaches either path — only a plan whose repair pool
  itself refuses would.

## Recorded design gaps — not bugs, not changed

Several places where SPEC A3/A4 is silent and the controller made a house-rule choice. Kept
here so nobody rediscovers them as surprises:

- **O-4** — the spec says nothing about what the pipeline does with a coverage refusal;
  non-substitutability lives in `prepare.rs` as house rule.
- **O-8** — "worst gap" has no contract; today it folds over all samples of failing islands
  rather than just uncovered ones (equivalent only because covered samples are within range by
  definition).
- **O-9** — a repair row that failed once stays failed forever, even though it might have
  succeeded against a later accepted state; the search is order-dependent in a way the spec
  never acknowledges.
- **O-10** — whether the controller inherits any per-sweep commit bound (as Phase-8 R2 has) is
  unstated.
- **O-11** — the spec never says the repair pool is frozen for the run; it matters for #26
  (whether model-seated arms that add rows may enter mid-run).

## Adjacent, not this controller's fix

**Task #53 — the legacy replay asymmetry.** A second, older repair loop (`lib.rs`) reads the
same registered bound and is reachable from production through specialized-contact
validation. It cannot collide with the transactional controller (different pools), but
production builds its baseline with that legacy loop switched off and re-validates with it
switched on; the two agree today only because the legacy loop measures zero steps on all 45
corpus models. That is survivorship, not design, and the day it fires even once, the affected
model dies at Preparation with `StaleRegularContacts`. Owned by #53, not by this controller.
