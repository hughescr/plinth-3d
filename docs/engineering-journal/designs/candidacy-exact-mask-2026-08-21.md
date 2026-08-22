# E-E, and why "refuse it at candidacy" cannot decide the Brace-resize class (task #36)

> **Provenance.** Written 2026-08-21 against `feature/a4-controller` @ `192f9d6`, main
> checkout `/Users/craig/code/hughescr/dnd-campaigns/plinth-3d`. Every source anchor below
> was read at that commit. The measurement was produced twice, by two agents. The first
> built in the main checkout from `192f9d6` plus a temporary probe patch, then reverted the
> patch. The second applied that same patch to a fresh throw-away worktree at `192f9d6`,
> built its own binary there, produced its own log, and removed the worktree. The two logs
> agree line for line (§C5).
>
> **No cap, gate, threshold, tolerance or registered constant is moved, widened or bypassed
> by this record, and no production behaviour changes with it.** The only production edit in
> the commit series that carries this file is a doc-comment whose MEASURED numbers were
> wrong; see §F.
>
> Evidence labels: **MEASURED** (a number a run produced), **DERIVED** (closed form from
> MEASURED or registered inputs), **INFERRED**, **ASSUMED**.
>
> This record exists because the finding had lived only in one checkout's untracked
> scratch. A refutation a judge must rule on is a design record, and a fresh clone had
> none of it.

---

## §A. The defect chain, MEASURED from source

1. `brace.rs:523` calls `working.quarantine_subtrees(&negative.contact_refusals)?` **inside
   the Brace stage**, which runs after the CCO transcript has been sealed.
2. A cascaded contact stub therefore gets its terminal ledger flipped to `Quarantined`
   while its finalized transcript row still says `Routed`.
3. `finalized_unrouted_terminals` (`runner.rs:5579`) walks the sealed transcript against the
   live ledger and admits exactly three (transcript row, ledger state) pairs:
   `Routed`↔`Consumed`, `SkippedQuarantined`↔`Quarantined`, `UnroutedNoWinner`↔`Retired`.
   Every other combination falls to `_ => return Err(...TerminalEvidenceMismatch { terminal })`
   at `runner.rs:5617`.
4. `(Routed, Quarantined)` is not one of the three. The Brace cascade mints it after the
   seal, and the run dies at Structure.
5. The accepted contact set seals at Routing and is not re-opened later:
   `accepted_contact_set_after_routing` (`runner.rs:4337`), called once at `runner.rs:3926`,
   re-derived and cross-checked by `validate_accepted_contact_set` (`runner.rs:4233`).

That chain is escalation **E-E**. It is recorded in the amending design record's §E
alongside E-A..E-D; this file is the detail E-E points at.

**The live instance is the Giant Bat**, and it is deterministic: `result ERROR (exit 5)`,
`emitted=0`, `Structure: TerminalEvidenceMismatch { terminal: 298 }` on the drilled shell,
then `{ terminal: 187 }` on the oriented solid substituted for it.

---

## §B. The direction that was recorded, and what it forbids

**Direction (b):** refuse the doomed arm **at candidacy**, so the transcript itself records
`UnroutedNoWinner` and the sealed accounting never disagrees with the ledger.

Two things that direction explicitly forbids, and this record does not touch either:

* **No fourth admitted pair.** `(Routed, Quarantined)` stays inadmissible.
* **No second removal point at Structure.**

The Brace-stage cascade **stays** for members whose condemnation disturbs no contact
accounting — model-anchor arms and brace rungs. Nothing here asks for its removal.

---

## §C. The refutation, for the Brace-resize class only

**Claim.** For a contact-declaring member whose Brace-stage clearance verdict is decided by
a radius the Brace stage itself assigns, direction (b) is **unavailable**: candidacy cannot
decide the same physics question, because the deciding input does not exist yet, and no
one-sided bound on it is conservative.

### C1. There is one formula, not two — so the divergence is not a re-implementation

MEASURED by call graph at `192f9d6`:

* One definition of the mask: `contact::mask_extent_mm` (`contact.rs:391`), reached for a
  placed row only through `contact::placed_mask_inset_mm` (`contact.rs:142`).
* One definition of PR-3: `contact::emerges` (`contact.rs:169`).
* Production call sites: `brace.rs:1983`, `cco.rs:3592`, `rewire.rs:942` for the mask;
  `cco.rs:3658` (via `emergence_ok`) and `rewire.rs:960` for PR-3.
* `tests/retired_formula_guard.rs`'s `CONVERGED` table pins that `brace.rs`, `cco.rs` and
  `rewire.rs` each still call the shared helper, so a fourth site cannot quietly invent its
  own inset.
* The policies agree field for field. `BraceConfig::contact_policy` (`brace.rs:70`) and
  `cco::seat_contact_policy` (`cco.rs:4293`) both build `ContactPolicy` from the same
  registry placeholders — `default_contact_depth_mm`, `standoff_mm`, `min_illegal_len_mm`,
  `ball_factor_stub`, `FrozenTipLoadPolicy::from_placeholders` — plus the run's own
  `process_resolution_mm`.

### C2. One cosine reproduces both masks — the "different cosine" wording is stale

Task #36's metadata describes this class as "different projection point + different cosine →
different mask". For the Bat's two arms that is **not** what the numbers say.

`mask_extent_mm` is, verbatim:

```
mask = (seat_depth + r_max + standoff + process_resolution) / |cos|
seat_depth = max(process_resolution + r_max * sqrt(1 - cos^2), 0)     (seated end)
```

Solving each arm for the single `|cos|` that reproduces **both** its candidacy mask and its
Brace mask, at `standoff = 0.600 mm` and `process_resolution = 0.0500 mm`
(MEASURED, both runs):

| arm | `\|cos\|` | candidacy `r` → mask | Brace `r` → mask | worst residual |
| --- | --- | --- | --- | --- |
| slot 377 (drilled shell, terminal 298) | 0.849732 | 0.949212 → 2.529801 mm (logged 2.529804) | 1.409608 → 3.357267 mm (logged 3.357270) | 3.4e-6 mm |
| slot 250 (oriented solid, terminal 187) | 0.579807 | 1.048241 → 4.488216 mm (logged 4.488212) | 0.901736 → 4.029666 mm (logged 4.029662) | 4.4e-6 mm |

Same formula, same policy, **same cosine**. `r` is the only input that differs. Whatever is
true of the toad `model_rung` instance #36 was opened on, the projection point and the
cosine are not what separates candidacy from Brace here.

### C3. The deciding input is created inside Brace, after candidacy has finished

MEASURED by call graph: `resize_forest_current_parity` (`transient.rs:428`) has exactly two
production callers, `brace.rs:413` and `brace.rs:675`. It is never reached from `cco.rs` or
`rewire.rs`. `brace.rs:459` applies the resize (`apply_resize`); `brace.rs:462` then runs
the first `exact_clearance_negatives` against the resized forest.

So the radius the Brace verdict is computed from is assigned by the Brace stage. At
candidacy that number does not exist. A candidacy screen cannot call the shared predicate
on it, and under the no-approximation rule it must not call the predicate on a *substitute*
for it.

### C4. The divergence is not one-sided, so no conservative bound exists either

MEASURED, the same two arms:

* slot 377: `r` **grew**, 0.949212 → 1.409608 mm.
* slot 250: `r` **shrank**, ≥1.048241 → 0.901736 mm.

Both ended Brace-negative. Any single-sided bound on the future radius — "assume it can
only grow", "assume it can only shrink" — would false-refuse arms in one of the two
directions. There is no cheap conservative envelope to screen with, only a guess.

### C5. The Bat, reproduced exactly, with the paths named correctly

MEASURED, both runs, byte-for-byte: the two probe logs are **19,050 lines**, and after
stripping the `stage:` progress lines they are identical at **19,033 / 19,033** lines
(`diff` clean).

**The run order, and it is the opposite of what the first evidence file's headings said:**

| run | geometry | faces | terminal | arm | Brace clearance | candidacy `r` | Brace `r` |
| --- | --- | --- | --- | --- | --- | --- | --- |
| 1st `SealedSupportRun` | hollow shell, drilled | 185,412 | **298** | `MemberKey { slot: 377, generation: 2 }` | −0.160653 mm | 0.949212 mm | 1.409608 mm |
| 2nd `SealedSupportRun` | oriented solid, substituted | 159,336 | **187** | `MemberKey { slot: 250, generation: 2 }` | −0.031388 mm | 1.048241 mm | 0.901736 mm |

Corroborated three ways: the run's own report sentence (the shell is built and refused
first, then the solid is substituted and fails), the probe log's two
`stage: model Giant Bat entering SealedSupportRun` boundaries with `slot: 377` before the
first and `slot: 250` after it, and the candidate volume (12,065 `CCO-CAND` probe rows in
run 1 against 6,552 in run 2, matching the larger shell mesh).

**`.git/candidacy-exact-evidence/giant-bat-radius-divergence.txt` had these two headings
swapped, and the reconciliation script inherited the swap in its case names.** The numbers
in both files are correct; only the path labels were exchanged. Both are corrected in place.

**Slot ids are not stable across passes; the geometry is.** The `−0.040503 mm` `Shoot` is
`slot 486` in run 1 and `slot 410` in run 2 with identical endpoints. Cite endpoints, not
slots, across runs.

---

## §D. What this record does NOT claim

* **Not** that direction (b) is unavailable everywhere. It is unavailable for the class
  whose Brace verdict turns on a Brace-assigned radius. A contact-declaring member refused
  on inputs that all exist at Routing is untouched by this argument.
* **Not** that #36's original toad `model_rung` observation was wrong. It says the
  "different cosine" wording does not describe the Bat's two arms.
* **Not** that any replacement direction is cheap. None of the three in §E has been costed.
* **Not** a proposal to admit a fourth pair, add a second removal point, or resurrect a
  ranking rule.

---

## §E. The menu the judge has to choose from

1. **Refuse the resize that drives a contact-declaring member negative** — task #25's lane,
   the resize-timing family, applied to the member whose contact accounting is at stake.
2. **Re-open the tournament for terminals whose carrier the resize condemned**, so the
   terminal reaches a disposition the transcript can still record honestly.
3. **An honest fourth pair** — which E-E's recorded direction forbids, and which is listed
   only so the menu is complete.

The lane's own reading is that (1) is the only one that keeps both the seal and the
no-approximation rule, and it is not this record's to choose.

---

## §F. The repair search's own cost, re-measured — and a correction

`coverage_repair.rs`'s `own_contact_condemned` doc-comment cited a MEASURED split that
three persisted sidecars do not reproduce. The corrected numbers, MEASURED on `zombie-3-a`:

| quantity | doc-comment said | MEASURED |
| --- | --- | --- |
| rejected trials whose own contact was quarantined | 52 | **52** |
| rejected trials whose own contact was `UnroutedNoWinner` | 34 | **37** |
| share of the 89 rejections that built no arm at their seat | 86 of 89 | **89 of 89** |
| share of the waste the narrow field leaves unnamed | 38 % | **42 %** (37/89) |

Sources, all three agreeing exactly (`trials=92 commits=3`, 52 / 37 / 3):
`.../scratchpad/chal-zombie/out/zombie-3-a.coverage.repair.json`,
`.../scratchpad/seat-consol-out/runs/out-zombie3a-tip/zombie-3-a.coverage.repair.json`, and
the challenge lane's own re-run. The 3 remaining steps are the 3 commits, whose contacts
were not condemned. The likely origin of "34" is subtracting the 3 committed trials twice
(89 − 52 − 3).

**Two consequences, both stated rather than acted on.**

* The correct headline is stronger than the one it replaces: **every** rejected trial in
  that search — 89 of 89 — paid a full `prepare → route → rewire → brace → structure →
  lower → measure` pass to learn that no arm was built at its seat.
* **The 37 are a different class from the 52 and need their own direction.** The 52 are
  quarantined before any arm is proposed, and a trial's acceptance genuinely depends on
  post-Brace path-level state — that is the class §C proves undecidable at candidacy. The
  37 end at **Routing**, with no winning arm, before any brace, resize or path-level state
  exists. Nothing in §C's argument reaches them, and closing item 2 on §C's reasoning alone
  left 42 % of the named waste unexplained.

**What is measured about the 37, and what is not.** MEASURED: there are 37, and each one is
`UnroutedNoWinner` on its own appended contact. NOT MEASURED: how many of the 37 are the
`every_feasible_arm_refused_by_clearance` sub-class (task #51's shape) rather than some
other no-winner cause. The run already computes that flag —
`TrialContactDisposition::UnroutedNoWinner { every_feasible_arm_refused_by_clearance }`
(`runner.rs:5148`), folded per route by `record_disposition` (`runner.rs:2135`) — but the
sidecar does not persist it per step, and `report.rejections_by_class` names only
`no_improvement`. **The cheap next step is one evidence-only disclosure of that existing
flag plus one `zombie-3-a` run, not a guess.** Until it exists, no screen for the 37 should
be designed, because nobody knows yet which physics question it would have to ask.

**One constraint on any such screen is already MEASURED, and it is a real one.** At the
promotion site (`runner.rs:1822`–`1873`) the trial loop holds only `winner.id`,
`winner.seat_mm`, `winner.key`, `winner.scored_from` and `winner.marginal_area_mm2`. There
is no member axis, therefore no `abs_cos`, therefore `contact::emerges` cannot be called
seat-locally as a single call. That constrains the FORM of a screen — it must be given an
axis, or it must live where axes exist. It does not close the 37.

**Do not resurrect the ranking rule.** Deprioritising every candidate inside a condemned
seat's coverage disc was built, MEASURED HARMFUL, and reverted; the reason is a scale
mismatch (cluster seats sit 0.03–0.46 mm apart against a 6.0 mm score radius), and tuning
it means inventing a radius.

---

## §G. Reproducing this, and where the evidence is

**In-tree.** Everything a judge needs to rule is in this file: the source anchors of §A and
§C are readable at `192f9d6`, and §C2's table is re-derivable from the formula quoted there
with nothing but the four numbers per row.

**Not in-tree, by size.** The two 4.4 MB probe logs stay out of git. They live at
`plinth-3d/.git/candidacy-exact-evidence/` (implementer:
`bat-full-probe-log.txt` md5 `f8a3e9d5666400a260c959f8968510d7`,
`giant-bat-radius-divergence.txt`, `probe.patch`) and
`plinth-3d/.git/chal36-challenge-evidence/` (challenge:
`chal-bat-probe-log.txt` md5 `dce2bad6d3de82831adc238ae7e884be`,
`chal-bat-baseline-report.txt`, `mask-reconciliation.js`). The two logs differ only in the
`stage:` timing lines; strip those and they are identical.

**To reproduce from scratch:** apply `probe.patch` to a clean `192f9d6` worktree, build,
run `plinth prepare` over `Giant Bat.stl` from `minis-to-process/` (sha256-verified), and
read the `BRACE-NEG` / `CCO-CAND` / `REWIRE-CHG` rows. **Revert the patch and delete the
binary afterwards.**

**Provenance trap, recorded because it nearly cost a ruling.** A probe-instrumented
`target/gate/plinth` outlived its patch: the source tree read clean at `192f9d6` while the
binary still carried `BRACE-NEG` / `CCO-CAND` / `REWIRE-CHG` code. `git status` cannot see
that. The binary was deleted; the next run rebuilds. This is exactly the check task #54
exists to make mandatory, and it is the second time the lane has been bitten by it.
