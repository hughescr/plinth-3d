# What sealed "Routed" must mean — the Routing/Brace seam (task #36, E-E)

> **Provenance.** Written 2026-08-22 against `feature/a4-controller` @ `5a8ade4`; **revised
> 2026-08-22 at `7e668e9`** to fold an adversarial review (verdict SOUND-WITH-CHANGES,
> findings C1–C11); **re-revised 2026-08-22 at `83e3215`** to fold an independent verification
> of that fold (verdict **FLAWED**, findings V1–V4 plus nine overstated wordings). Main checkout
> `/Users/craig/code/hughescr/dnd-campaigns/plinth-3d`, working tree clean. Every `file:line`
> below was read at `7e668e9` or re-read at `83e3215`, including the ones each review corrected.
> **Design only. No production code, no test code, and no constant is changed, added, moved or
> read differently by this record.**
>
> Evidence labels: **MEASURED** (a number a run produced), **DERIVED** (closed form from
> MEASURED or registered inputs), **INFERRED**, **ASSUMED**.
>
> Inputs this record answers to, in order: `candidacy-exact-mask-2026-08-21.md` (the #36
> refutation), the independent Codex opinion on this seam, the adversarial review of this
> record's first revision, the MEASURED waste of `wf_3b724805-678`, and `a4-controller-design.md`
> + `model-seated-arms-2026-08-21.md` with its amendment.
>
> **What the revision changed, so a reader of the first version is not misled.** The exclusion
> key moved from a tournament position to the context digest tuple the record already carries
> (§1.3, finding C1) and a stale key is now a **miss**, not an error; the termination argument
> is restated as finite backtracking with a combinatorial and unmeasured bound rather than a
> linear walk (§1.3, C1/C2); the per-pass cost is re-derived and corrected (§0, C2); the
> legalization loop is confined to **attempt 0 only** (§1.4, C3); a **third** direction —
> constrain the resize — is stated and argued down rather than omitted (§1.5, C4); the measured
> margins and radii move **inside** the sealed evidence (§1.3, C5); the stage order becomes
> **A → C → B** (§4, C6); two anchors are corrected and one "existing field" is re-labelled as
> new plumbing (§2.4/§4, C7); Stage A classifies on any sealed terminal and calls a planner, not
> a re-implemented walk (§4, C8); and the `candidates_feasible` overload is named as a choice
> Craig makes rather than an implementation detail (§4/§5, C9).
>
> **What the RE-revision changed, and it is the more serious of the two.** The second review
> found the C1 fold sound about *termination* and unsound about *soundness*, and found the break
> inside §3's own flagship trace. An exclusion was keyed on the **prefix** state a terminal's
> tournament reads, but the thing it stored was the **brace verdict**, whose deciding input — the
> radius from `resize_forest_current_parity` — is a function of the **suffix**, the terminals
> routed *after* the keyed one. An early terminal's prefix is the most stable thing in the run,
> so its exclusions always hit, while the world each was measured in has already been replaced.
> After `k` such hits the terminal retired `UnroutedNoWinner` without any arm having been shown
> illegal in one world — and the model emits with a real contact dropped, fewer supports, under a
> clean seal. So: a prefix-keyed exclusion is **demoted from a stored answer to a steering
> no-good** (§1.3). It may steer the next tournament; it may **never** count toward exhaustion.
> A terminal now retires only after a **confirmation round** that re-measures every remaining arm
> under the current exclusion set (§1.3, §4 Stage B) — which costs real extra passes, and §0 and
> §5 now price them. The cascade victim rule is fixed so a terminal is never excluded for another
> member's failure. Candidate identity gains the discriminators `CcoCandidateEvidence` already
> uses. The finiteness of the context universe becomes an explicit implementation obligation with
> a test rather than an asserted fact (§4 Stage B). **T6 as previously specified is withdrawn**:
> it was not reachable on the code and it contradicted §4 Stage A(b) (§3, §4 Stage A). Nine
> overstated sentences are rewritten, including the one in the §5 memo that Craig rules on. **The
> recommendation is still Option 2**; what changed is its price and the honesty of its
> termination story, not its ranking.

---

## §0. The answer in one page

**The defect.** The routing stage seals a record that says terminal 298 is routed. The brace
stage then measures that terminal's arm against the model with the arm's true radius, finds
it inside the model, and condemns it. Condemning it flips the terminal's live state to
`Quarantined` while the sealed record still says `Routed`. That pair is not one of the three
the accounting admits, so the run dies at the structure stage with
`TerminalEvidenceMismatch`. **MEASURED: the Giant Bat, exit 5, emitted=0, terminal 298 on the
drilled shell and terminal 187 on the substituted solid.**

**Where it dies, and this is new here.** MEASURED from `candidacy-out/runs/tip-Giant-Bat.log`
(28 lines): the stage stream is `Preparation → Routing → Rewire → Brace → Structure` **once**
per sealed support run, and there are two sealed support runs, each with exactly one
`Preparation`. The decisive evidence is an **absence with a known cause**: there is no
`CoverageMeasurement` line, and `runner.rs:1603` prints that line immediately after the
attempt-0 chain returns and before any repair trial can begin. Its absence therefore means the
attempt-0 chain never returned successfully. (Repair trials print no stage lines at all by
construction — they pass `None` for the clock at `runner.rs:1874` — so "no trial lines" would
have proved nothing; the missing `CoverageMeasurement` line does.) The result line
independently confirms it: `failed during Structure: TerminalEvidenceMismatch`. The Bat
therefore dies on **attempt 0 — the model's own baseline plan, before any repair contact
exists.** The A4 controller's try/measure/keep-or-roll-back machinery cannot reach it, by that
machinery's own rule (`runner.rs:1601`: an attempt-0 stage refusal is `SupportRunError::Failed`
via `RefusedStage::into_attempt_zero_failure` at `runner.rs:1332`, not a rollback). Any fix
that is only a controller fix does not touch the Bat.

**The recommendation, in one sentence.** Keep sealed `Routed` meaning exactly what it means
today — the terminal's contact is in the final accepted set and its terminal is consumed in
the final live tree — and make that meaning *reachable* by letting the brace stage's exact
verdict be **fed back into a fresh routing pass** instead of being written on top of a sealed
record.

**Why that is not the "predict it at candidacy" direction the refutation killed.** The
refutation proves the brace verdict cannot be **predicted** when candidates are chosen,
because the deciding input (the radius) does not exist yet and its movement is two-sided
(MEASURED: 0.949212 → 1.409608 mm grew and failed; 1.048241 → 0.901736 mm shrank and failed).
It does not forbid **measuring** the verdict exactly and then choosing again with that
measurement in hand. Nothing is approximated, no bound is guessed, and no constant appears.

**What a no-good is, and what it is NOT — the correction the second review forced.** Each
abandoned pass records one **steering no-good**, keyed on the **exact context** the tournament
read: the digest tuple `TerminalRouteEvidence` (`cco.rs:642`) already carries per terminal —
`plan_digest`, `loads_digest`, `geometry_digest`, `forest_digest_before`,
`controller_digest_before` — plus the candidate's own identity. That key covers the state the
terminal's **tournament** reads. It does **not** cover the state the **brace verdict** reads: the
deciding input there is the radius `resize_forest_current_parity` (`transient.rs:428`) assigns
from the braced whole-forest path-level worst span, which depends on the choices of terminals
routed **after** this one. The key is a prefix key; the verdict is suffix-dependent.

> **So a no-good means exactly this and nothing more:** *this candidate was measured illegal in
> at least one world that had this prefix, so do not choose it again here while a different
> choice is available.* It is a **steering preference, not a stored answer**, and it may **never**
> be counted toward exhaustion.

Widening the key to cover the suffix would make it sound and useless: any no-good anywhere moves
the suffix, so every key would miss, the pass would replay identically, and the loop would
degenerate into a no-progress refusal. The prefix key is the only workable one, so its claim is
narrowed to what it can carry.

**Why it terminates.** Each abandoned pass adds a `(terminal, context, candidate)` triple that
was not in the set before — the **abandon rule** (§1.3) — and no triple is ever added twice,
because a no-good candidate is not chosen again at that context. The reachable set of triples is
finite (MEASURED on `zombie-3-a`: 42–106 arms proposed per terminal, over a finite cadence of
terminals). A strictly growing subset of a finite set cannot cycle, so the search terminates.
**But it is a backtracking search over a tree of contexts, not a walk down a list: when two
terminals fail in the same pass, the passes multiply rather than add.** The bound is finite,
combinatorial, and **not measured**. Stage A's corpus count is the first measurement of how often
more than one terminal is involved. Finiteness of the context universe carries one condition that
is an **obligation on the implementation rather than a fact about the design**: the no-good set
must not be an input to any digest in the context tuple. If it is, every context is fresh, every
no-good misses forever, the same candidate is re-condemned every pass, and the abandon rule waves
each of those passes through as progress. §4 Stage B states the obligation and tests it.

**How a terminal retires, now that a no-good cannot retire it.** A terminal retires
`UnroutedNoWinner` only at the end of a **confirmation round**: with the no-good set held fixed,
the terminal takes each of its remaining arms in turn, one per pass, and it retires only if
**every one of those passes measured that arm illegal** — the pass that measures the last one
being the pass that retires it. Because the plan, the no-good set and the cadence are fixed across
the round (precondition S-1), the world each arm is measured in **is the world that choosing that
arm creates**; there is no other world in which this terminal could have taken that arm. That is
genuine exhaustion in one world-family, rather than a union of `k` refusals harvested from `k`
different worlds. Confirmation rounds are **extra passes**, and the cost figures below say so.

**What the loop can and cannot do to a run.** For the class it is built for — a brace refusal of
a sealed-`Routed` terminal's own chosen arm — it adds **no new way to fail a run**: the refusal
either converges on a legal arm or converts into an honestly unrouted terminal via a confirmation
round. It **does** name one refusal of its own, and this is a genuinely reachable one rather than
the unreachable case the previous revision named: **a pass that learns nothing**. That happens
when the members condemned in a pass belong to no sealed-`Routed` terminal's own choice — a brace
rung, or a cascade that takes down only terminals that did not choose the condemned member — so
there is no candidate whose choice could be steered, the abandon rule forbids abandoning, and
there is nothing to try differently. It refuses by name, at the brace stage, with the physics
(§1.3, "The one refusal the loop keeps"). The first version of this record claimed flatly that
"the retry loop has no give-up state of its own"; that sentence is deleted and stays deleted.

**What it costs.** One extra `route → rewire → brace` pass per refused arm. MEASURED on the
Bat, one refused arm per pass. **One pass is 17.8 s on the drilled shell and 15.1 s on the
oriented solid**, DERIVED from the log's cumulative stage clock:

| | drilled shell | oriented solid |
| --- | --- | --- |
| entering Preparation | 3 ms | 2 ms |
| entering Routing | 1,335 ms | 1,914 ms |
| entering Rewire | 9,048 ms | 9,358 ms |
| entering Brace | 16,401 ms | 14,177 ms |
| entering Structure | 19,112 ms | 17,012 ms |
| **Routing** (duration) | **7.71 s** | **7.44 s** |
| **Rewire** (duration) | **7.35 s** | **4.82 s** |
| **Brace** (duration) | **2.71 s** | **2.84 s** |
| **one legalization pass** = Routing + Rewire + Brace | **17.78 s** | **15.10 s** |
| Routing + Rewire as a share of one pass | 85 % | 81 % |

A legalization pass repeats Routing, Rewire and Brace. It does **not** repeat Preparation: the
plan does not change, so the prepared digests are restored rather than recomputed. Preparation
is therefore excluded from the per-pass figure (it is 1.33 s on the shell, 1.91 s on the
solid). **A note on a number that is easy to get wrong:** 16.4 s is the span from entering
Preparation to entering Brace on the shell. It counts a stage the pass does not repeat and
omits a stage it does, so it is not a per-pass figure. Use 17.8 s and 15.1 s.

**Confirmation rounds are extra passes on top of that, and the re-revision added them.** A
terminal cannot retire on a count of no-goods, so retiring one costs a round of up to one pass per
remaining arm — up to ~100 passes at 17.8 s, where 100 is the largest MEASURED arm count at one
terminal and it was measured on a **different** model (`zombie-3-a`, 42–106 arms per terminal),
not on the Bat. A round is **void** if any other terminal's arm is refused during it, because the
world then moved for a reason other than this terminal's own choice; the voided round's passes are
paid and the round is re-run later. So the honest cost sentence is: **steering passes, plus one
confirmation round per retirement, plus any voided rounds.**

The worst case is therefore **not** "one terminal's candidate set is refused". It is several
terminals failing in one pass, where the steering passes multiply over the backtracking tree and
each retirement inside that tree carries its own round. That number is finite and unmeasured. It
is reported loudly rather than capped, per the no-cap doctrine, and Stage A's corpus count — how
many terminals are disturbed in one sweep — is what sizes it.

---

## §1. The contract question

### §1.1 What sealed `Routed` means today, read from the code

Three functions carry the whole meaning, and all three were read at `7e668e9`.

* `finalized_unrouted_terminals` (`runner.rs:5652`) walks every sealed record row against the
  **live** tree ledger and admits exactly three pairs: `Routed`↔`Consumed` (`:5671`),
  `SkippedQuarantined`↔`Quarantined` (`:5672`), `UnroutedNoWinner`↔`Retired` (`:5681`).
  Everything else is `Err(TerminalEvidenceMismatch)` at `:5690`. A cardinality check at
  `:5693`–`:5698` refuses a ledger holding terminals the record never named.
* `accepted_contact_set_after_routing` (`runner.rs:4346`), called **once** at
  `runner.rs:3935`, applies SPEC §6's removal at the point the clause names, and does the
  carried-load repartition exactly once per pass.
* `validate_accepted_contact_set` (`runner.rs:4242`) re-derives that removal ledger from the
  sealed record and the live tree rather than trusting the stored value, at three later
  points: `runner.rs:5411`, `:5834`, `:5904`.

Read together, the meaning of a sealed `Routed` row is:

> **This terminal's contact is in the final accepted contact set, and this terminal is
> `Consumed` in the tree the run finally ships.**

That is a claim about the **end** of the pipeline, made at the routing stage. The brace stage
falsifies it. Specifically, `brace.rs:523` calls `working.quarantine_subtrees(&negative.contact_refusals)`
inside the brace stage, after the record is sealed. The comment at `brace.rs:498`–`:517` says
why the sweep must cascade: a model-seated arm is minted as a root with the contact stub
re-parented onto it, so condemning the arm condemns the stub, and the stub is the terminal.

**The illegal pair is not a bookkeeping nit.** If it were admitted, the accepted contact set
would keep a contact whose support was condemned, and the load partition would keep charging
model faces to it. The structure-stage error is the audit catching a real print-safety defect.
Any answer that makes the check lenient is wrong.

### §1.2 Candidate 1 — Codex's hybrid: two seals

**The contract.** The routing seal becomes "routing's decision, recorded". A second,
final-commit seal is created after the brace stage has legalized the tree exactly. Only a
finally-committed pass may emit. A pass that cannot be legalized is **aborted** as a whole,
and a new pass starts with a sound conflict recorded.

**What changes in the accounting.**

| site | change |
| --- | --- |
| `finalized_unrouted_terminals` (`runner.rs:5652`) | Splits in two. A routing-time form compares the record with the tree **as routing left it**; a new final-commit form compares a new post-brace attestation with the final tree. The three-pair table moves to the final form. **It must not gain a fourth pair** — E-E forbids it — so the split is mandatory, not optional. |
| `accepted_contact_set_after_routing` (`runner.rs:4346`) | Stops being the accepted set and becomes a provisional set. A **second** removal pass must run after bracing, with a **second carried-load repartition**. |
| `validate_accepted_contact_set` (`runner.rs:4242`) | Each of its three call sites must say which of the two sets it validates. Its recompute-from-state tripwire must be built twice or it protects half of what it protects today. |
| `brace.rs:523` cascade | Stays, and becomes legitimate: it is the second stage's own removal mechanism. |

**What the verifier must then enforce**, adapting Codex's list to this codebase's names:

1. Complete and exclusive terminal accounting **within** each pass, at both seals
   (`finalized_unrouted_terminals` in both forms).
2. Lineage: the final attestation names the routing attestation it was built from, and the
   existing chain (`extend_contact_attestation_chain`, `runner.rs:3908`, stage tags
   `0x10/0x11/0x12`) gains a fourth link.
3. `verification_snapshot_digest` in `plinth-verify/tests/coverage_gate_toed_slab.json` folds
   the new attestation, so the golden moves for every model.
4. Aborted passes never reach output — a new property, because today no pass can be aborted.

**The honest cost.** SPEC §6 sequences the carried-load repartition **before** rewire,
bracing and coverage judgment. A second repartition after bracing is a deviation from the
spec that needs Craig's ruling. Worse, it is not stable: the repartition changes the load on
every surviving contact, which changes every path-level worst span, which changes every
radius the resize assigned — so the tree the second seal certifies is not the tree the second
seal measured. Closing that gap means bracing again, which is exactly the fixed point Codex
warns has no termination proof.

**And it still does not fix the Bat.** The Bat aborts on attempt 0, and there is no attempt
1: `run_support` calls the stage chain once for the baseline plan (`runner.rs:1596`) and turns
any refusal into `SupportRunError::Failed` (`:1601`). The hybrid therefore needs the retry
loop **as well as** the second seal.

**Reversibility.** Poor. Once the evidence sidecar and the verifier golden carry two
attestations, backing out is a second format move.

### §1.3 Candidate 2 — strict, made reachable (RECOMMENDED)

**The contract.** Sealed `Routed` keeps today's meaning, word for word. Nothing is admitted
late, no fourth pair is minted, and the brace stage never writes on a sealed terminal's
ledger. What changes is that a brace-stage refusal of a **sealed-routed** terminal's support
**abandons the pass** instead of quarantining, and the next pass routes again **steered away
from that arm at that context**. Steered — not answered. The distinction is the whole subject of
"The no-good" below, and getting it wrong is what made the previous revision unsound.

**What changes in the accounting: nothing in the code.**

| site | change |
| --- | --- |
| `finalized_unrouted_terminals` (`runner.rs:5652`) | **Unchanged.** Three pairs, same catch-all error, same cardinality check. It becomes a tripwire that should never fire, because the brace stage refuses first. |
| `accepted_contact_set_after_routing` (`runner.rs:4346`) | **Unchanged.** One removal, one repartition, at the point SPEC §6 names. |
| `validate_accepted_contact_set` (`runner.rs:4242`) | **Unchanged**, all three call sites. |

**The #49 tripwire, re-run under this contract.** Regression #49 is "unrouted regular contacts
are never removed from the accepted set". Under Candidate 2 `accepted_contact_set_after_routing`
is byte-identical at its one call site (`runner.rs:3935`), and `validate_accepted_contact_set`
— which re-derives the removal ledger from the sealed record and the live tree rather than
trusting the stored value — is byte-identical at all three sites (`:5411`, `:5834`, `:5904`).
A regression that stopped removing unrouted regular contacts would be re-derived, compared and
refused exactly as it is today, by the same check, at the same three places. Under Option 1
that tripwire protects half of what it protects today unless it is built twice.

**But "nothing changes in the accounting" is true of the code and NOT true of the meaning, and
the difference is a real audit cost.** Today `UnroutedNoWinner` means "this pass's tournament
produced no winner". After the retry loop lands it can also mean "no arm of this terminal
survived the exact post-brace check". Same row, same `UnroutedContactReason::NoWinner`, same
SPEC §6 removal, same load repartition — but a **new upstream way to produce it**. A defective
retirement would remove a real contact from the accepted set and repartition its carried area
onto its neighbours, and every existing check would agree, because every existing check tests
**consistency**, not whether the retirement was **earned**. That is a new print-safety hole —
fewer supports, silently, under a clean seal — where today a spurious retirement can only come
from routing itself.

**The previous revision opened exactly that hole and declared it closed. This is the finding that
made the second review return FLAWED, and it is worth stating at full width.** That revision let
`k` recorded no-goods retire a terminal, on the argument that `k` measured negative margins are
better evidence than an arithmetic count. They are better evidence of the wrong proposition. Each
margin was measured in the world that existed when its pass ran, and every abandoned pass replaces
that world — so `k` true measurements can be harvested from `k` different worlds and still say
nothing about whether an arm is legal in **this** one. The exposure is worst exactly where the
previous revision claimed safety: a terminal **early** in the cadence has the most stable prefix,
so its no-goods hit on every pass, while the suffix that decided each verdict has moved every
time. §3's flagship trace, terminal 298, is that terminal. The record's own warning —
*arithmetic over a stale universe adds up perfectly* — applies one level up, to a pile of true
measurements taken in different worlds.

**So the answer is not more disclosure; it is a weaker claim plus a real re-measurement.** A
no-good steers and never retires (below). A retirement is earned by a **confirmation round** in
which every remaining arm is measured under the current no-good set (below, and §4 Stage B). The
measured millimetres still go inside the sealed evidence — that part of review finding C5 was
right and stands — but they are now disclosure of a round that actually happened, rather than the
thing that stands in for one.

**What happens to `brace.rs:523`'s cascade.** It splits, and the split is decided by the
sealed record, not by a member's kind:

* The sweep's condemnation set is computed first, read-only, by the **planner half of the
  mutation itself** — not by a second walk that reproduces it (see §4 Stage A(b)).
* If that set contains **no** terminal the sealed record names, the sweep is applied exactly as
  today. This is the refutation's own carve-out (§B): brace rungs and model-anchor arms whose
  condemnation disturbs no contact accounting. **Their behaviour does not move.**
* If it contains a terminal the record sealed as **`Routed`**, nothing is quarantined. The pass
  is abandoned and the refusal travels out as evidence: for each disturbed terminal, the arm its
  tournament chose, the measured clearance margin in millimetres, and the radius the resize
  assigned.
* **A no-good is recorded only for a terminal whose OWN chosen arm was among the condemned
  members.** This is a correction, and without it the scheme punishes the innocent. A terminal can
  be disturbed purely by cascade — its stub re-parented onto somebody else's arm, and that arm
  condemned. Its own arm was never measured illegal. If the cascade victim were given a no-good,
  and its carrier sits **later** in the cadence than the victim, the victim's prefix never moves,
  its no-goods hit every pass, and it loses one legal arm per pass as punishment for another
  member's failure — retiring in the end with no arm of its own ever measured illegal, the
  abandon rule satisfied at every step and the ledger arithmetically perfect. Cascade victims are
  reported in the refusal evidence and steer nothing.
* If the condemned members include **no** sealed-`Routed` terminal's own chosen arm, the pass has
  learned nothing it can steer with. The abandon rule forbids abandoning, nothing is quarantined,
  and the pass refuses with a named brace-stage error. See "The one refusal the loop keeps",
  below.

**What the verifier must then enforce.** Two additions:

1. A terminal that retires `UnroutedNoWinner` because every candidate was refused by the exact
   post-brace check must show it **as measured physics, not only as arithmetic**. The new named
   variant beside the three that already exist in `NoWinnerEvidence` (`cco.rs:657`) carries, per
   refused arm, **the MEASURED clearance margin in millimetres and the radius the resize
   assigned**, and the arithmetic that candidates refused by exact clearance plus candidates
   refused earlier accounts for candidates generated. Arithmetic alone is not enough:
   *arithmetic over a stale universe adds up perfectly*. **And measured margins alone are not
   enough either** — that is the second review's finding, and it is why this obligation is
   stronger than the previous revision's. The evidence must identify the arms as the arms of **one
   confirmation round**: every remaining arm measured under the same no-good set, one per pass,
   the last of them measured in the pass that retires the terminal. What the seal must let a
   reader check is not "here are `k` negative numbers" but "here is a round in which nothing this
   terminal could have chosen was legal". Margins measured in earlier steering passes may be
   reported, and must be **labelled as steering history rather than as part of the round**. This
   is the standard the refutation §F.1 held itself to, and it is why the margins belong **inside
   the sealed evidence** rather than in Stage D's sidecar — Stage D becomes disclosure of a record
   that already exists, rather than the only record.
2. A run that emitted must report its legalization pass count, its no-good count and its
   confirmation-round count. The invariant on the passes is the weak and correct one: **every
   abandoned pass added at least one no-good that was not already in the set** (see "The abandon
   rule", below). A pass that was abandoned may never reach output — true by construction here,
   because an abandoned pass produces no lowered run and therefore no sealed authority.
3. **No retirement without a completed round.** Every `UnroutedNoWinner` row produced by this
   mechanism must name the confirmation round that earned it, and the number of passes attributed
   to that round must equal the number of arms it measured. A round that was voided must be
   reported as voided and must retire nothing. This is the obligation that replaces the previous
   revision's "`k` measured negatives" and it is the one a verifier can actually falsify.

Everything else on Codex's list is **already enforced** and needs nothing: complete and
exclusive terminal accounting (the three-pair table plus the cardinality check), immutable
digests and lineage (`extend_contact_attestation_chain`, and the five prepared digests
restored on every rollback at `runner.rs:1888`), and "no selected terminal silently
disappears" — that is precisely the error the Bat raises today.

**The no-good — what it claims, and the claim the previous revision could not support.** A
no-good names one `(terminal, context, candidate arm)` triple. It does **not** say "this arm can
never work"; Codex is right that such a claim is unsound, because the same arm might be legal
under a different shared path. The previous revision said instead: *in this exact tree and
controller state, this candidate was measured illegal at its true radius.* **That sentence is
withdrawn, and it is the sentence the second review broke.** The key is the **prefix** state —
what the terminal's tournament reads before it chooses. The true radius is assigned afterwards,
by `resize_forest_current_parity` (`transient.rs:428`), off a braced whole-forest path whose
worst span depends on every terminal routed **after** this one. The key does not see the suffix,
so it cannot certify a verdict the suffix decided. §2.3's own rule is *key on the digests of the
inputs the computation reads*; the previous revision keyed on the inputs a **different**
computation reads.

The record already contained the proof against itself, in the paragraph arguing down the position
key: *"when a terminal retires, its stub is not built, so the braced forest changes, so
`transient_report`'s path-level worst spans change, so `resize_forest_current_parity` assigns
different radii to other arms."* That is the suffix coupling. The fold moved the defect one layer
up instead of removing it.

**So the claim is narrowed to one a prefix key can carry:**

> *This candidate was measured illegal in at least one world that had this prefix. While another
> choice is available here, take it.*

That is a **steering no-good**. It suppresses a choice. It is not a stored answer, it is not
evidence of exhaustion, and **nothing may be retired on a count of them**. Every retirement goes
through a confirmation round instead.

**There is no free fix, and the record has to say so rather than pick the comfortable half.**
Widen the key to include the suffix — a post-brace digest — and the no-good becomes sound and
**never hits**, because any no-good anywhere changes the suffix; the loop then replays
identically, the abandon rule bites on the first repeat, and the whole mechanism degenerates into
a refusal that never legalizes anything. Keep the prefix key and the no-good is heuristic with
respect to the suffix. The resolution is not a better key; it is refusing to let the heuristic
decide anything the physics has not measured in the world it is deciding about. Steering is a
choice among legal options and needs no soundness. Retirement drops a real contact, so it gets a
round.

**Confirmation rounds, stated precisely enough to build.** When a terminal has a no-good for every
arm it can still generate at its current context, it does not retire. It enters a round:

* The no-good set is **held fixed** for the duration of the round. Refusals measured during the
  round are recorded against the round, not added to the steering set.
* The terminal takes each remaining arm in turn, one arm per pass, ordered by the tournament's own
  order so the round is replayable.
* If any arm survives the brace stage, the round **ends there** and that pass proceeds normally.
  The over-suppression has been caught, which is the point of the round.
* If every arm is measured illegal, the terminal retires `UnroutedNoWinner` **in the pass that
  measured the last one**, and that pass completes.
* If any **other** terminal's arm is refused during the round, the round is **void**: the world
  moved for a reason other than this terminal's own choice. The pass abandons on that terminal's
  new no-good as usual, and the round is re-entered later against the new set.

**Why a completed round is genuine exhaustion, and not one more union across worlds.** Given the
plan, the fixed no-good set, and a bit-reproducible cadence (precondition S-1), the world
downstream of the terminal is a **deterministic function of the terminal's own choice**. Taking
arm `A_i` produces world `W_i` and there is no other world in which this terminal takes `A_i`.
So "every arm was measured illegal in the world that taking it creates" is exactly the
proposition "no choice available here is legal here" — which is what `UnroutedNoWinner` asserts.
The refusals still come from different worlds; the difference, and it is the whole difference, is
that the terminal **caused** each of those differences by its own choice, rather than inheriting
them from choices made elsewhere between one pass and the next. That is why the round must be run
under a frozen no-good set and must be voided if anything else moves.

**What it costs, stated where the claim is made rather than only in §0.** A round is up to one
pass per remaining arm — MEASURED 42–106 arms per terminal on `zombie-3-a`, at 17.8 s a pass on
the Bat's drilled shell. A voided round is paid and re-run. This is a real increase over the
previous revision's price, and the memo (§5) carries it.

**Keying it, which is the one genuinely delicate part, and where the first version was wrong.**

MEASURED, from the refutation §C5: member slot numbers are **not stable across passes** (the
same −0.040503 mm strut is slot 486 in one pass and slot 410 in another, with identical
endpoints). So the exclusion cannot be keyed on a slot.

The first version keyed it on the terminal's identity plus **the candidate's position in that
terminal's tournament**, and justified the position by an induction: the cadence replays in the
same order, so a terminal's tournament is bit-identical as long as every terminal before it is
unchanged. **That induction's own precondition is what an exclusion breaks.** Excluding an arm
at terminal `m` makes `m` choose a different arm; every terminal after `m` then routes against a
different forest, so their candidate lists are different lists, and "position 3" names a
different arm — or no arm. Two ordinary paths reach this, both inside the class this design
exists for:

* **Several exclusions from one sweep.** One brace sweep can condemn several terminals. Excluding
  at `m` and at `n > m` in the same pass invalidates `n`'s position key on the very next pass.
* **A retirement is not neutral.** When a terminal retires, its stub is not built, so the braced
  forest changes, so `transient_report`'s path-level worst spans change, so
  `resize_forest_current_parity` assigns different radii to **other** arms. This is the same
  global coupling this record uses against Option 1's second repartition (§1.2). Candidate 2 has
  the coupling too. Candidate 2's honest defence is that it **re-measures** after the coupling
  fires and Option 1 does not — a good defence, and it is stated here rather than hidden, but it
  does not rescue a position key.

Under a position key a stale key would be the **expected** outcome, not an exotic one, and the
first version made a witness mismatch a hard named error — so the loop would have failed runs
routinely, and a position-keyed exclusion would have refused an arm in a context where the
physics was never re-measured. That is a cap in physics costume, on Codex's forbidden list ("no
context-free exclusion derived from a context-dependent failure") and on Craig's ("no count or
state deciding an outcome").

**The fix is this record's own §2.3 rule applied to this record's own key, and the material is
already in the tree.** `TerminalRouteEvidence` (`cco.rs:642`, the struct directly above
`NoWinnerEvidence`) already carries, per terminal disposition:

```
physical_contact_id, terminal_id, original_key,
plan_digest, loads_digest, geometry_digest, forest_digest_before, controller_digest_before
```

Key each no-good on:

> `(terminal_id, plan_digest, loads_digest, geometry_digest, forest_digest_before,
> controller_digest_before, candidate identity)`

Then:

* **A stale no-good misses. It does not apply, and it is not an error.** That is literally
  §2.3's rule: *a digest mismatch is a miss; it can never be a silent stale hit.* The candidate
  is regenerated and re-measured from scratch.
* The no-good is **prefix-bound**. That is narrower than "context-bound" was made to sound: it
  binds what the tournament read, not what the brace stage measured. It is the right shape for
  steering and the wrong shape for a verdict, which is why it steers and does not retire.
* **The Bat's headline path is unchanged.** One terminal failing against an unchanged prefix
  produces identical contexts pass after pass, so the no-good hits, the terminal is steered to a
  different arm, and §3's steps 1–7 run as written. **What does not hold verbatim is §3's
  retirement branch**: the previous revision let those same repeated hits retire terminal 298, and
  they cannot. §3 is rewritten accordingly. An early terminal's contexts being stable is exactly
  what makes its steering reliable **and** its stored verdicts worthless — the same fact, cutting
  both ways.
* Termination stays a finite backtracking search over reachable `(context, choice)` states —
  sound, combinatorial, priced in §0 — and it does **not** depend on retirement-by-count, so
  narrowing the no-good's claim leaves the termination argument intact.

**Candidate identity is new plumbing, it must not over-match, and the cost band must say so.**
`CcoCandidateEvidence` (`cco.rs:605`) is `{ family, generation_index, price_ml, clearance_rows }`.
**There are no endpoints in it.** A no-good needs the arm's endpoint coordinates by exact bit
pattern — refutation §C5 ("cite endpoints, not slots, across runs") is precisely why — and that
field does not exist today. **Endpoints alone are not the identity, though**, and the previous
revision said they were. Two distinct candidates can share endpoints and differ in the four fields
`CcoCandidateEvidence` already discriminates on; one refusal would then suppress both. That is one
more path to over-suppression, and over-suppression is the direction that costs supports. The
identity is therefore **the endpoint bit patterns together with `family`, `generation_index`,
`price_ml` and `clearance_rows`**. All of it is hand-written work in `cco.rs`, not a field that is
already there for free.

**The abandon rule, which is what makes the growth argument true.** A pass may be abandoned
**only if it carries at least one no-good that was not already in the set**. Everything else
refuses by name. This is not bookkeeping hygiene; it is the invariant the termination argument
stands on. Without it, a pass that abandons while learning nothing would replay identically and
abandon identically, forever. With it, the case that learns nothing is the one named refusal
below. Passes inside a confirmation round are exempt, because a round adds nothing to the set by
design; a round is bounded by the terminal's own arm count instead.

**Termination, stated honestly.** Let a **context** be the digest tuple above at the moment a
terminal routes. A pass is a deterministic function of the plan and the no-good set. Each
abandoned pass adds at least one `(terminal, context, candidate)` triple that was not in the set
— a triple is only added for a candidate that was **chosen and whose own arm was condemned**, and
a no-good candidate is not chosen at that context again, so no triple is ever added twice. The
reachable triples are finite: the cadence has finitely many terminals, each terminal proposes
finitely many candidates (MEASURED: 42–106 on `zombie-3-a`), and each context is generated by the
finitely many choices at earlier terminals. A strictly growing subset of a finite set cannot
cycle, so **the search terminates**. Confirmation rounds do not disturb this: each is finite in
its own right, and a round is entered only at a context whose steering set is full or re-entered
after the set grew, both of which are finite events.

**The last clause of that argument is an obligation, not an observation, and the previous
revision stated it as a fact.** *"Each context is generated by the finitely many choices at
earlier terminals"* is true **only if no state derived from the no-good set is folded into any
digest in the context tuple** — `controller_digest_before` being the obvious hazard, since §4
Stage B item 4 puts the no-good set in the loop's own state. If the set leaks into the digest,
every context is fresh, every no-good misses forever, the same candidate is chosen and condemned
on every pass, and **every one of those passes satisfies the abandon rule with a genuinely new
triple while making no progress at all** — non-termination that the abandon rule waves through.
§4 Stage B carries this as a stated obligation with a test.

What the first version got wrong is not whether it terminates but **by how much**. This is a
depth-first backtracking search over a tree of contexts, and its bound is a product across the
failing terminals, not the sum `|U|`. **Passes multiply, they do not add**, and each retirement
inside that tree carries a confirmation round. The bound is finite, combinatorial and unmeasured;
Stage A's corpus count is the first measurement of how many terminals are involved at once, which
is the exponent.

**The one refusal the loop keeps, stated because the blanket claim is deleted.** The sentence
"the retry loop has no give-up state of its own" is **removed from this record** and stays
removed. What is true:

* A brace refusal that condemns a **sealed-`Routed`** terminal's own chosen arm always yields a
  new no-good, so it always abandons and always makes progress. This class — the Bat's class, the
  class the design exists for — gains **no new way to fail a run**.
* **A pass that learns nothing refuses by name.** The condemned members belong to no
  sealed-`Routed` terminal's own choice: a brace rung whose cascade takes down only terminals that
  did not choose it, or a condemnation of a member no terminal selected. There is no choice to
  steer, the abandon rule forbids abandoning, and replaying would reproduce the same pass. It
  refuses at the brace stage with the terminal, the member and the measured margin. This is a
  **new named refusal**, and — unlike the previous revision's T6, which was withdrawn as
  unreachable (§3) — it is reachable, so it is priced as a real outcome rather than as a
  formality.
* The honest residual exposure is therefore **wall time plus that one named refusal**: a finite
  but combinatorially large number of passes on a pathological model, and a named stop when a
  pass has nothing to learn. Both are printed and neither is capped.

**No cap, and no counter deciding an outcome.** The loop stops when the physics stops producing
refusals, when a confirmation round measures every arm illegal, or when a pass learns nothing.
All three are measured or structural events. The pass count, the no-good count, the round count
and the wall time are printed and decide nothing — the same rule `RepairRunReport`
(`coverage_repair.rs:671`) already follows. Note in particular that a confirmation round's length
is **not** a budget: it is the terminal's own arm count, and the round ends early on the first
legal arm.

**Reversibility.** Good. Stage A (§4) alone is one classification and one named outcome, and
reverting it restores today's bytes. Stage B is one loop and one parameter; deleting the
parameter restores Stage A exactly.

### §1.4 The fit check: a generalization, or a second mechanism?

Craig asked this directly. The answer is **a generalization, with three genuinely new surfaces**,
and here is the line-by-line.

**Where the loop runs, stated first because everything else depends on it.**

> **The legalization loop runs at ATTEMPT 0 ONLY. It is never wrapped around a repair trial. A
> trial's brace refusal remains exactly what it is today: `restore_prepared_digests`
> (`runner.rs:1888`), `into_trial_rollback` (`runner.rs:1344`), one ledger row, next candidate.**

The first version of this record said in one place that the loop wraps "both" the attempt-0 chain
call (`runner.rs:1596`) **and** the trial chain call (`runner.rs:1874`). That was wrong and is
withdrawn. Three reasons, in order of force:

1. **Every trial changes the plan digest.** An exclusion is keyed on the context tuple, and
   `plan_digest` is in it. A trial appends a repair contact, so every trial starts with an
   exclusion set that matches nothing. The loop would therefore pay a full legalization walk per
   trial and carry nothing forward.
2. **The arithmetic is absurd.** MEASURED on `zombie-3-a`: 92 trials. At up to ~100 passes each
   and 17.8 s a pass, that is ≈ 45 hours for one model. It would also land inside
   `wall_time_per_trial_max_ms` (`coverage_repair.rs:685`) — the instrument the A4 lane built
   specifically to see one pathological trial — and bury it.
3. **A trial is discretionary work.** The cheapest correct action for a repair contact that
   cannot be legalized is to drop it, and that is what a ledgered rollback does. Wrapping
   `runner.rs:1874` would make discretionary work obligatory. The seam is
   `runner.rs:1874`–`:1891`, and it stays as it is.

This is also what makes the two-loop picture below true rather than decorative.

**What the A4 controller already has, that the retry loop reuses unchanged:**

| the controller has | the retry loop needs | same thing? |
| --- | --- | --- |
| `run_stage_chain` hands the context back on every stage refusal (`ChainOutcome::Refused { context, stop }`, `runner.rs:1298`–`:1409`) | a pass that abandons must hand the context back intact | **Yes.** The abandon path is that path with a payload. |
| `RepairLedger::ledgered` — a set of tried rows that deliberately survives a rollback (`runner.rs:2007`) | a set of excluded candidates that survives an abandoned pass | **Yes**, same idiom, different key. |
| `context.restore_prepared_digests(digests)` on every rollback (`runner.rs:1888`) | the same, on every abandoned pass | **Yes**, identical need, identical call. |
| counters that report and never decide (`RepairRunReport`, `coverage_repair.rs:671`) | pass count, no-good count, round count, wall time | **Yes**, the new counters go in the same report. |
| a monotone exclusion set over a finite pool | a monotone exclusion set over a finite universe of `(terminal, context, candidate)` triples | **Same idiom, weaker bound.** The controller's pool is walked; this one is searched with backtracking. |

**What it does not have, and this is the honest new work:**

1. **A retry at attempt 0.** `run_support` calls the stage chain once for the baseline plan
   (`runner.rs:1596`) and converts any refusal into `SupportRunError::Failed` via
   `RefusedStage::into_attempt_zero_failure` (`runner.rs:1332`). The retry wraps **that call
   only**. That is one new `loop` around an existing call — not a new subsystem.
2. **A channel for steering no-goods into the tournament.** `route_initial_cco_with_limits`
   (`runner.rs:3785`) gains one borrowed parameter.
3. **A stable candidate identity.** The arm's endpoints by exact bit pattern **together with the
   four fields `CcoCandidateEvidence` (`cco.rs:605`) already discriminates on** — `family`,
   `generation_index`, `price_ml`, `clearance_rows`. The endpoints do not exist in that struct
   today, and endpoints alone would over-match (§1.3). New plumbing, priced as such.
4. **The confirmation round.** This is the piece the second review forced and it is the one the
   controller has no analogue for. The controller's ledger never has to prove a pool is
   exhausted; it just moves on to the next candidate. Here, "this support point has no legal arm"
   is a claim that drops a contact from the accepted set, so it has to be measured rather than
   counted (§1.3). Driving one terminal through its remaining arms under a frozen no-good set,
   and voiding the round when anything else moves, is **genuinely new control flow**, not a
   generalization of anything already in the tree. It is the honest answer to §1.4's question:
   mostly a generalization, with **three** new surfaces rather than the two the previous revision
   claimed.

**The two suppression sets have different lifetimes, and confusing them would be a defect.**
`ledgered` is per **run**: SPEC §6 says exact failed rows stay excluded across commits.
No-goods are per **plan and per context**: the context tuple carries `plan_digest`, so a
plan change makes every no-good miss automatically. Because the loop never wraps a trial,
this is belt and braces rather than the only guard — but it is stated so no implementer
carries a no-good across a commit by accident.

So the shape is two loops with clean, different jobs, one **beside** the other rather than one
inside the other: the outer controller tries **different plans** (unchanged); the legalization
loop legalizes **the baseline plan** before the controller ever sees it (new). They share the
context handback, the digest restore and the report.

### §1.5 Candidate 3 — constrain the resize (the refutation's own preferred direction), and why it is declined

The refutation `candidacy-exact-mask-2026-08-21.md` §E names three directions, and closes:
*"The lane's own reading is that (1) is the only one that keeps both the seal and the
no-approximation rule."* Direction (1) is **not** the one this record recommends, and the first
version of this record omitted it. It is stated here so the choice is a genuine three-way one.

**The contract.** Leave routing, the seal and the accounting entirely alone. Instead, refuse the
**resize** that drives a contact-declaring member into the model: `resize_forest_current_parity`
(`transient.rs:428`) would be constrained so that a member which declares a model contact may
not be assigned a radius that its clearance cannot hold. This is task #25's resize-timing family.

**Why it is attractive.** It touches no seal, mints no pair, changes no accounting function,
and needs no retry loop at all. If it worked it would be the smallest possible answer.

**Why it is declined, in one paragraph.** The MEASURED evidence says the resize is not doing
anything wrong. On the drilled shell, slot 377's radius **grew** 0.949212 → 1.409608 mm, and it
grew **because the braced path demanded it**: the path-level worst span sets the radius, and that
is a strength requirement. Refusing the growth does not remove the defect; it **trades** a
clearance defect for an under-thick member — a strength defect for a geometry one — and the
strength defect does not announce itself. It prints, and it snaps. That is the worse of the two
trades on a support that carries load. And the constraint does not even cover the class: on the
oriented solid, slot 250's radius **shrank** 1.048241 → 0.901736 mm and the arm still failed the
exact clearance check. "Do not let the radius grow" says nothing about an arm that got thinner
and was still inside the model. A rule that fixes one of the two MEASURED cases by accepting a
silent strength defect, and does not address the other case at all, is not the answer here.
Direction (1) may still be right **as its own task** for the resize-timing family — this record
declines it as the answer to the Routing/Brace seam, not as a bad idea in general.

**Reversibility.** Not assessed. The direction is declined before costing, on the grounds above;
that decision is Craig's to overturn, and §5 puts it in front of him.

### §1.6 The three candidates side by side

| | Candidate 1: two seals | Candidate 2: strict, made reachable | Candidate 3: constrain the resize |
| --- | --- | --- | --- |
| meaning of sealed `Routed` | routing's decision only | unchanged: final admission | unchanged: final admission |
| fourth pair | never minted (a second seal instead) | never minted | never minted |
| `finalized_unrouted_terminals` | splits in two | unchanged | unchanged |
| `accepted_contact_set_after_routing` | provisional; a second removal after bracing | unchanged | unchanged |
| carried-load repartition | **twice**, and the second one is a SPEC §6 deviation | once, where §6 says | once, where §6 says |
| `validate_accepted_contact_set` | three call sites must choose a set; tripwire halves unless rebuilt twice | unchanged | unchanged |
| `brace.rs:523` cascade | stays whole, becomes the second stage's removal | splits by what the sweep would condemn | untouched |
| verifier | new attestation, new lineage link, golden moves corpus-wide | measured margins inside the sealed no-winner evidence; the abandon-rule invariant; **no retirement without a completed confirmation round** | untouched |
| fixes the Bat on its own | **no** — still needs the retry loop | **yes** | **half**: fixes the grew-and-failed case by accepting a silent strength defect; does not address the shrank-and-failed case |
| what it costs | a spec deviation and an unstable second seal | extra passes — steering passes (finite, combinatorial, unmeasured) **plus one confirmation round per retirement, plus voided rounds** | a strength defect that does not announce itself |
| reversibility | poor (format and golden moves) | good (one classification, then one loop) | not assessed |

---

## §2. The waste question

### §2.1 What the 89 actually are

MEASURED, `zombie-3-a`, run `wf_3b724805-678`, verified twice: 92 trials, 3 commits, 89
rejections, **all 89 are clearance kills**.

* **52** were condemned after the resize — the trial's own appended contact ended quarantined
  or unrouted.
* **37** ended at routing with no winning arm. Every one of the 37 is
  `all_feasible_candidates_refused_by_clearance`: 42–106 arms proposed, 37–102 of them
  feasible, **zero** trials with no feasible arm. Arms existed; the clearance standoff refused
  every one.
* **29 of those 37 are retries on island 8.**

Rejected trials roll back, so between two commits the accepted plan is identical.

### §2.2 Is a refusal against an unchanged world sound to reuse?

**Craig's question, answered precisely: yes in principle, and it buys nothing here, because
the 89 are 89 different computations rather than one computation paid 89 times.**

Reuse of a stored result is sound **if and only if** the key covers every input the
computation reads. Get that right and it is not a screen and not a heuristic: it is the same
function on the same arguments, and a stored answer is the answer.

Now apply it. A trial computes `f(P + r_i)` where `P` is the accepted plan and `r_i` is the
one appended repair row. Between commits `P` is fixed, but `r_i` differs on every trial, so
the **inputs differ on every trial**. A stored refusal for `r_i` reused for `r_j` is not a
stored answer; it is a prediction that `r_j` will behave like `r_i`. That is the guess the
no-cap doctrine forbids, and it is exactly the ranking rule that was already built, MEASURED
HARMFUL and reverted.

And the reuse that *would* be sound is already in place. `RepairLedger::ledgered`
(`runner.rs:2007`) holds every tried row, survives rollback, and is honoured across commits —
so a row is never re-tried against any plan. **The identical computation is already never
paid twice.**

### §2.3 What a sound key is — and it is not a world epoch

An epoch counter is a **proxy** for the inputs. It is sound only if you can prove nothing
outside the counter can move an input, and that proof is the thing that goes stale. This
codebase already has the honest key in hand, and it is not a counter: the five prepared
digests (`plan`, `loads`, `initial_forest`, `contact_attestation`, `prepared` — captured and
restored at `runner.rs:1816` and `:1888`) plus the geometry digest. Those **are** the pass's
own inputs, recomputed from state rather than asserted.

> **Rule.** Key any stored result on the digests of the inputs the computation reads. Never on
> an epoch, a counter or a generation number. A digest mismatch is a miss; it can never be a
> silent stale hit.

Under that rule the trial-level memo has **zero hits**, because every trial's plan digest
differs. That is the proof, rather than the assertion, that the 89 are not a memoization
problem.

**This rule binds this record's own key, and it took two revisions to satisfy it.** The first
version of §1.3 keyed exclusions on a tournament position and made a mismatch a hard error —
breaking both halves of the rule above. The second version keyed them on the digest tuple
`TerminalRouteEvidence` (`cco.rs:642`) already carries and made a mismatch a miss, which fixed
the second half and **not the first**: the tuple is the digests of the inputs the *tournament*
reads, while the thing being stored was the *brace verdict*, whose deciding input is a radius
assigned from the choices of terminals routed later. Keying a stored result on the inputs of a
different computation is the same defect in better clothes.

The rule admits exactly two ways out, and there is no third. Key on the inputs the brace verdict
actually reads — which means folding a post-brace digest into the key, at which point every key
misses and the mechanism does nothing (§1.3). Or **stop calling it a stored result**. §1.3 takes
the second: the prefix-keyed no-good is demoted to steering, which needs no soundness because it
only picks among legal options, and every claim that decides an outcome — retirement — is
re-measured in the world it decides about. A design record that states a rule and then violates
it two sections later is worse than one that never stated the rule; this record did that once,
and this is the correction rather than a second restatement.

### §2.4 Where the real savings are — three of them, all doctrine-clean

1. **Run-invariant work rebuilt inside every trial.** `SurfaceAnchorPool::from_geometry`
   depends only on `geometry` and `delicacy`, both fixed for the whole run, and is built at
   **two** production sites — `runner.rs:3844`, inside `route_initial_cco_state` (the
   **Routing** stage), and `runner.rs:4568`, inside `rewire_state` (the **Rewire** stage).
   (The first version cited `runner.rs:2686` and `:3383`; at `7e668e9` line 2686 is blank space
   inside the `RoutedLogicalRoute`/`RoutedRouteProvenance` enums and line 3383 is
   `#[cfg(test)] transit_test_stale_digest`. The claim was right and the anchors were wrong;
   correcting them is what makes §4's ordering argument visible, because it names the two
   stages.) MEASURED: a 494,617-row build per trial on the toad, paid twice. The legacy
   pre-routing replay adds ~3 `validate_binding` calls per trial, each re-running a full
   baseline selection and a coverage-sample scan; MEASURED at 1,383 executions across the
   45-model sweep. **These are the same function on arguments that provably never move.**
   Keying them on the geometry and configuration digests is memoization in the strict sense: it
   decides nothing, changes no verdict, and a sabotage test (corrupt the stored value, expect a
   different artifact) proves it.
2. **The 37 die when the mask is honest, not when the controller is clever.** They are
   terminals where dozens of feasible arms were all refused by the model-anchor clearance
   standoff. That is task #51's finding — the model-anchor clearance mask is unsatisfiable —
   and the amendment records that the fix flips the theorem. No controller change reaches
   them. §F.1 of the refutation already named and counted the class; **naming it was the
   deliverable, and a screen is not.**
3. **The 52 stop being dishonest under §1.3, which is worth more than the passes it costs.**
   Today a trial whose own contact is condemned after the resize carries a plan whose
   accounting disagrees with its tree. Under the recommended design that contact retires at
   the seal, SPEC §6's strip removes it from the accepted set, the load repartition stops
   charging faces to it, and the coverage measurement judges a true state. Some of those 52
   may then commit. **That is unmeasured and this record does not claim it** — it is a Prove
   result, not a design claim. Note that the 52 are **trials**, and the legalization loop does
   not run on trials (§1.4); what changes for them is Stage A's classification, which makes a
   trial's disturbing brace refusal a named ledgered rollback instead of a hard error.

### §2.5 Reconciling "29 on island 8" with the no-cap doctrine

The loop is not re-paying the same answer. It is asking 29 different questions and getting the
same answer, because the underlying cause is one route whose clearance mask refuses everything.
Suppressing the 30th question would be guessing.

The doctrine-clean way to spend less on island 8 is to **make the cause visible and let the
existing order rules act on it**, and both mechanisms are already designed:

* Rule N4's spatial diversity, which after a rejection sorts every remaining candidate inside
  the rejected seat's own coverage disc **after** every candidate outside it. It reorders; it
  never excludes; completeness is untouched.
* The named per-route cause `Blocked(ClearanceMaskUnsatisfiable)`, which the refusal already
  reports with the terminal count and the millimetre shortfall.

Both put island 8 last and say why. Neither is a counter deciding an outcome. Whether they cut
the 29 is a measurement, not an argument.

---

## §3. The Giant Bat, traced step by step

MEASURED starting facts, from the refutation §C5 and the run log: two sealed support runs (the
drilled hollow shell first, then the substituted oriented solid), each dying at the structure
stage on attempt 0. Drilled shell: terminal 298, arm at slot 377, brace clearance
−0.160653 mm, radius 0.949212 mm at candidacy and 1.409608 mm at bracing. Oriented solid:
terminal 187, slot 250, −0.031388 mm, 1.048241 → 0.901736 mm.

Under the recommended design, the drilled shell walks like this.

1. **Preparation.** Unchanged, and run once. Legalization passes do not repeat it.
2. **Routing, legalization pass 1.** The no-good set is empty. Terminal 298's tournament
   proposes its candidates; the winner becomes the arm at slot 377. The record seals
   298 ↔ `Routed` with its context tuple (`plan_digest`, `loads_digest`, `geometry_digest`,
   `forest_digest_before`, `controller_digest_before`).
   `accepted_contact_set_after_routing` finds nothing to remove for 298.
3. **Rewire.** Unchanged.
4. **Brace.** `resize_forest_current_parity` (`transient.rs:428`) assigns the arm
   1.409608 mm from the braced tree's path-level worst span. `exact_clearance_negatives`
   (`brace.rs:1854`) measures −0.160653 mm. The arm lands in `contact_refusals`.
5. **The new step.** The condemnation set is planned read-only. It contains the arm **and**
   terminal 298's stub, because a condemned carrier takes its dependant with it. Terminal 298
   is sealed `Routed`, so this refusal disturbs contact accounting. **Nothing is quarantined.**
   The pass is abandoned, carrying one no-good: terminal 298's context tuple, the winning
   candidate's identity, its −0.160653 mm margin and its 1.409608 mm radius. The abandon rule is
   satisfied — this triple was not in the set. The arm condemned was 298's **own** choice, so 298
   is the terminal that gets the no-good.
6. **Legalization pass 2.** The whole chain runs again on the same plan. Terminal 298 routes
   against an unchanged prefix, so its context tuple is **the same tuple**; the no-good therefore
   matches and **steers**. Its tournament regenerates, the suppressed candidate is passed over
   with the named reason, and the next-best feasible candidate wins. The record seals
   298 ↔ `Routed` on a **different** arm.
7. **Outcome.** Either the brace stage is clean — and the run proceeds to Structure, Lowering
   and the coverage measurement — or another arm is refused, another no-good is added, and
   step 6 repeats. The search terminates (§1.3).

**Where the previous revision of this record went wrong, at exactly this point in exactly this
trace.** It let steps 5–7 run until 298 had a no-good for every arm and then **retired** 298 on
that count. That is unsound, and terminal 298 is the worst possible place to do it. 298 is early
in the cadence, so its prefix — and therefore its context tuple — never moves; its no-goods
therefore hit on **every** pass. But the radius that condemned each arm came from the braced
whole-forest path, which every other terminal's re-choice changes. So the arm refused in pass 1
was refused in a world that pass 2 destroyed. After `k` passes, 298 would have been retired on
`k` true measurements taken in `k` worlds that no longer exist, `accepted_contact_set_after_routing`
would have removed a real contact, its carried area would have been repartitioned onto its
neighbours, the brace stage would have come back clean, and **the model would have emitted with a
support missing, under a seal that audits perfectly**. Its stability made its steering reliable
and its verdicts worthless — the same fact, cutting both ways. §3's flagship trace was the
exposure, not the safe case.

8. **Retirement, under the repaired scheme.** If 298 runs out of arms to steer to, it does not
   retire. It enters a **confirmation round** (§1.3): the no-good set is frozen, and 298 takes
   each remaining arm in turn, one per pass, each measured against the world that taking it
   creates. The first arm that survives bracing ends the round and that pass proceeds. Only if
   every arm is measured illegal does 298 retire — **in the pass that measured the last one** —
   and the sealed evidence names that round. If any other terminal is refused mid-round, the
   round is void, the pass abandons on that terminal's no-good, and 298's round is re-run later
   against the grown set.

**What would happen if a second terminal also failed.** Say terminal 298 and terminal 512 are both
condemned in pass 1, each on its own chosen arm. Both get a no-good. In pass 2, 298 steers to a
different arm, and that changes the forest 512 routes against, so 512's context tuple is
**different** — its no-good **misses**, and its arm is regenerated and re-measured from scratch.
That is the correct behaviour, and it is also why the passes multiply: the search explores
combinations of choices rather than walking one list. It still terminates, because no
`(terminal, context, candidate)` triple is ever added twice.

**And the case where 512 was not at fault.** If 512's stub was condemned only because it hung off
298's arm — cascade, not its own choice — then 512 gets **no** no-good at all. Only 298 does.
Without that rule, and with the carrier later in the cadence than the victim, a terminal would
lose one legal arm per pass for someone else's failure, its own prefix never moving, until it
retired having had no arm of its own measured illegal (§1.3).

**Every terminal state the Bat can reach, and the accounting rows each writes.**

| # | state | how it is reached | accounting rows written |
| --- | --- | --- | --- |
| **T1** | **Emits.** | Legalization converges; Structure and Lowering pass; the coverage measurement finds no blocking patch, or the A4 loop closes it inside the committed-step bound. | Record row 298 = `Routed`, ledger = `Consumed` — the first admitted pair. `finalized_unrouted_terminals` returns 298 in neither list. `accepted_contact_set_after_routing` writes no removal for it. The report carries `legalization_passes`, `no_goods` and `confirmation_rounds`, with each no-good's terminal, context tuple, margin and radius. |
| **T2** | **Emits with terminal 298 honestly unrouted.** | Terminal 298 completes a **confirmation round** in which every remaining arm was measured illegal by the exact post-brace check, the last of them in the pass that retires it. **Not** reachable by counting no-goods — that was the previous revision's unsound path and it is closed. | Record row 298 = `UnroutedNoWinner` with the new evidence variant — `candidates_generated`, `refused_by_exact_clearance`, the arithmetic that they account for each other, **the identity of the confirmation round and the measured margin and assigned radius for every arm it measured**, with earlier steering refusals labelled as steering history rather than as part of the round — ledger = `Retired`, the third admitted pair. `accepted_contact_set_after_routing` writes one `UnroutedRegularRemoval` row (contact id, terminal id, route, reason, carried area) and the single batched removal runs. `NoncoverageVector.unrouted_no_winner` rises by one. The coverage measurement then judges the hole; if it is below the 0.16 mm² censor or covered by neighbours, the model seals and emits. |
| **T3** | **Honest coverage refusal.** | As T2, but the hole is above the censor and the repair loop cannot close it. | Everything T2 writes, plus `SupportRunError::CoverageClosure` carrying the per-route outcome `Blocked(ClearanceMaskUnsatisfiable { terminals, best_shortfall_mm, standoff_mm })`, the trial stream and the legalization ledger. Exit is non-zero and nothing is emitted — **but every number is named**, which is what today's `TerminalEvidenceMismatch` does not do. |
| **T4** | **A genuine clearance violation.** | The refused member declares **no** model contact, so `pipeline_violations > 0` at `brace.rs:554`. | Unchanged from today: `BraceError::ClearanceViolation`. This path is not touched. |
| **T5** | **Some other stage fails.** | For example Lowering's `RootRunTooShort` (task #29). | Unchanged: `SupportRunError::Failed { phase, detail }` on attempt 0, or a ledgered rollback on a trial. |
| **T6** | **A pass that learns nothing — a named brace refusal.** | Every condemned member belongs to no sealed-`Routed` terminal's **own chosen arm**: a brace rung, or a member no terminal selected, whose cascade disturbs only terminals that did not choose it. There is nothing to steer, the abandon rule forbids abandoning, and replaying would reproduce the pass exactly. | A new named `SupportBraceError`, carrying the disturbed terminals, the condemned member and the measured margin. **This is a real new refusal and is priced as one.** It is reachable: a brace rung can carry a contact stub, and a cascade victim gets no no-good by the §1.3 rule, so a pass can genuinely end with nothing learned. |

**T6 as the previous revision specified it is WITHDRAWN, and the withdrawal is a correction of
fact, not a rewording.** That revision defined T6 as "the condemnation set reaches a terminal the
sealed record named in a **non-`Routed`** pairing", and claimed it replaced an unnamed engine
error that fires today. Read at `83e3215`, the code does not support either half:

* **It is not reachable by cascade.** `collect_subtree_leaf_first` walks via `children_of`
  (`forest.rs:562`), which filters to members that are still live. An inactive member has no live
  presence to collect. The two non-`Routed` admitted pairings — `UnroutedNoWinner`↔`Retired` and
  `SkippedQuarantined`↔`Quarantined` — both mean exactly that. §1.3's own sentence *"when a
  terminal retires, its stub is not built"* says the same thing in the record's own words.
* **The one way in is closed by §4 Stage A(b).** The remaining route is the brace stage naming an
  already-inactive key directly in `negative.contact_refusals`. But `quarantine_subtrees`
  (`forest.rs:973`–`:996`) calls `require_active` on **each caller-supplied key before** the walk,
  so the exact-prefix planner Stage A(b) mandates returns `ForestError` **before** any condemnation
  set exists to classify. The classifier never gets to say "T6". Keeping both claims would have
  required dropping the first `require_active` from the planner — which is precisely the exact-prefix
  property C8(b) exists to protect.

So: the previous T6 goes, **and so does the §4 Stage A test that asked for it** (a
sealed-`UnroutedNoWinner` terminal yielding "the named T6 refusal rather than a `ForestError`");
§4 Stage A now pins today's `ForestError` for that input instead. The T6 row above is a
**different and reachable** refusal — the pass that learns nothing — which the repaired no-good
rules create and which therefore has to be named and priced.

**The state that is deliberately absent, restated correctly.** There is no *"legalization gave up
after trying"* run failure: a terminal whose confirmation round measures every arm illegal retires
(T2/T3) rather than failing the run. T6 is not that state — T6 is a pass with nothing to learn,
and it stops immediately rather than after trying. The blanket claim in the first version of this
record ("the retry loop has no give-up state of its own") overstated this and is withdrawn; the
second version's replacement claim — that the one named refusal was a case that already hard-fails
today, so nothing was really added — is **also** withdrawn, because it rested on the reachability
argument disproved above.

**What this record does not promise.** It does not claim the Bat emits. It claims the Bat
reaches T1, T2 or T3 — an emission or a disclosed refusal — instead of an accounting-integrity
error. Which of the three it reaches is a measurement, and the measurement needs the code.

---

## §4. Staged landing plan

**Order: A → C → B → D.** Stage C was previously marked "independent of A and B; can land in
any order". That was a sequencing error. The two production `SurfaceAnchorPool::from_geometry`
sites are in `route_initial_cco_state` (**Routing**, `runner.rs:3844`) and `rewire_state`
(**Rewire**, `runner.rs:4568`) — the two stages a legalization pass repeats, and 85 % of a pass
on the drilled shell (§0). **Stage C is the only lever on Stage B's named cost risk that is not
"wait for task #51".** It lands before B. The determinism replay measurement (S-1) is likewise a
precondition of B, not an option.

**No stage needs a new registered constant.** The no-good key is an exact bit comparison; a
confirmation round's length is the terminal's own arm count rather than a budget; the
coverage radius, the censor and the clearance standoff are all read from the registry exactly
as they are read today. `FROZEN_PASS_CAP = 3` (`brace.rs:434`) is pre-existing and untouched by
this plan. If any implementer finds themselves wanting a tolerance, that is an escalation, not a
design detail — say so and stop.

### Stage A — name the class; abandon instead of minting the illegal pair

**What lands.** `brace_fixed_point_current_parity` (`brace.rs:427`) takes one added, borrowed
argument: **the terminals the sealed record names, with the disposition it named for each** —
not only the `Routed` ones. Before the sweep at `brace.rs:523` it plans the condemnation set
read-only, intersects it with that set, and:

* empty intersection → applies the sweep exactly as today, returns
  `BraceFixedPointTermination::ContactRefused` exactly as today;
* intersection contains a **sealed-`Routed`** terminal **whose own chosen arm is among the
  condemned members** → applies nothing, returns a new named termination carrying that terminal
  with its measured margin and radius, plus any cascade victims **labelled as victims**;
* intersection contains sealed-`Routed` terminals but **none of them by its own chosen arm** →
  applies nothing, returns the new named "nothing learned" termination (T6 in §3).

`brace_state` (`runner.rs:4718`) supplies the set from `state.routed.transcript`, which it
already holds, and converts the new terminations into named brace-stage refusals.

**Three details that decide whether this is sound or merely plausible.**

**(a) Classify on any sealed terminal, not on `Routed` alone — but do NOT claim this retires an
error that fires today.** The tripwire Stage A stands in front of refuses *every* pair outside the
three, plus a ledger cardinality mismatch (`runner.rs:5693`), so the classifier is handed every
sealed disposition rather than the `Routed` ones only. That is cheap and it is the right shape.

**What the previous revision said here was wrong, and it is corrected rather than softened.** It
claimed a sweep reaching a terminal sealed `UnroutedNoWinner` (ledger `Retired`) "slips past a
`Routed`-only filter and meets `require_active` inside `quarantine_subtrees`", producing a hard
`BraceError` today. Read at `83e3215`, it does not: `collect_subtree_leaf_first` walks via
`children_of` (`forest.rs:562`), which filters to live members, so a cascade cannot collect an
inactive one — and an inactive member is exactly what both non-`Routed` pairings mean. The only
way to hand `quarantine_subtrees` an inactive key is for the brace stage to name it directly in
`negative.contact_refusals`, and in that case the **first** `require_active` — the one over the
caller-supplied keys, before the walk — errors before the classifier has a condemnation set at
all. §4 Stage A(b) requires the query to be an exact prefix of the mutation, so the planner
carries that same `require_active` and errors in the same place.

**The honest statement, therefore:** classifying on all sealed dispositions costs nothing and
closes a hole **if** one exists, but no such hole is demonstrated on the code, and Stage A must
**not** be sold as retiring an unnamed engine error on this path. A directly-named inactive
refusal key remains a `ForestError` from the planner, exactly as today — and that is the correct
outcome, because it means the brace stage asked to condemn something already condemned, which is
an engine defect rather than a physical one. The named refusal Stage A really does introduce is
T6, the pass that learns nothing, which arrives from a **live** condemnation with no steerable
choice behind it.

**(b) The read-only query must be an exact prefix of the mutation, or it is production logic
re-implemented one layer up.** `quarantine_subtrees` (`forest.rs:973`–`:996`) is: `require_active`
per key → `collect_subtree_leaf_first` with union dedup → `require_active` over the whole order →
`quarantine` each. Exposing only `collect_subtree_leaf_first` (`forest.rs:1004`) would reproduce
the walk but neither `require_active` pass nor the dedup contract, and the two would drift.
**Refactor instead:** split `quarantine_subtrees` into
`plan_quarantine_subtrees(&self, keys) -> Result<Vec<MemberKey>, ForestError>` — everything up to
and including the second `require_active` sweep — plus an apply step that quarantines the planned
order. `quarantine_subtrees` becomes plan-then-apply and keeps its signature and behaviour. The
classifier calls the **planner**. Drift then becomes impossible by construction rather than by a
test, which is the standing lesson that a defect fixed at one layer reappears one layer up.

**(c) Use the existing map.** The member→terminal side already exists as
`Forest::state.terminal_of: BTreeMap<u32, MemberKey>` (`forest.rs:366`, read at `:625` via
`terminal_key`). **Invert it**; do not add a second lookup and do not hand-roll a key comparison.
Member generations move — the Bat's arm is `MemberKey { slot: 377, generation: 2 }` — so a
hand-rolled comparison is a bug waiting.

**What it buys.** The Giant Bat stops dying with `TerminalEvidenceMismatch`, an error that
reads as a broken engine, and starts refusing at the brace stage with the physics: which
terminal, which arm, how many millimetres inside the model, at what radius. The class becomes
countable across the corpus — which is the measurement that sizes the prize before anyone
builds the instrument, and which supplies the exponent §1.3's bound is missing. **And task #47
arrives for free:** the same refusal on a repair *trial* is already a ledgered rollback, because
`run_stage_chain` returns the context on a brace refusal (`runner.rs:1405`) and
`into_trial_rollback` (`runner.rs:1344`) turns it into one ledger row.

Stage A's corpus count now has **two** numbers to bring back, not one: how many models hit an
accounting-disturbing brace refusal, and **how many terminals are disturbed in one sweep, split
by whether each was disturbed on its own chosen arm or purely by cascade**. The second split is
new here and it is not cosmetic — it is what says whether the cascade-victim rule ever fires, and
therefore whether the reachable T6 is a corner or a common outcome.

**What it does not buy.** The Bat still does not emit. If Craig's "stops the hard failure"
means "the Bat emits", the smallest stage is A **and** B together, and §5 prices both.

**Files.** `crates/plinth-support/src/brace.rs`, `crates/plinth-support/src/forest.rs`
(the plan/apply split), `crates/plinth-support/src/runner.rs` (`brace_state`, two
`SupportBraceError` variants).

**Tests that pin today's behaviour, and how each moves.**

| test | pins | moves |
| --- | --- | --- |
| `brace.rs:2865` `a_refused_model_seated_arm_condemns_its_stub_instead_of_failing_the_run` | the cascade for an arm carrying a registered, consumed stub: `ContactRefused`, both members gone, a quarantine record on the arm | **Stays green unchanged** if it passes an empty sealed-terminal set — which is the honest call, because the fixture has no sealed record. A second test is added beside it that supplies terminal 7 as sealed-`Routed` **on its own chosen arm** and asserts the pass is abandoned with nothing quarantined and exactly one no-good recorded. A third supplies terminal 7 as a **cascade victim** — condemned member is another terminal's arm — and asserts terminal 7 receives **no** no-good. |
| *(withdrawn)* a sealed-`UnroutedNoWinner` terminal yields T6 rather than a `ForestError` | — | **Deleted, not moved.** The previous revision asked for this test; it would have failed. `children_of` (`forest.rs:562`) filters to live members so the cascade cannot reach an inactive one, and a directly-named inactive key hits `require_active` in the exact-prefix planner first (§3, §4 Stage A(a)). **Replaced by** a test that pins today's behaviour: a directly-named inactive refusal key must still produce the planner's `ForestError`, and the classifier must not swallow it. |
| *(new)* a pass in which no sealed-`Routed` terminal's own arm was condemned | the reachable T6 | Asserts the named "nothing learned" refusal, that nothing is quarantined, and that no no-good is recorded. |
| `contact_regression_gate.rs:294` (structural needle `BraceFixedPointTermination::ContactRefused`) | OD-4: a refused contact must not hard-error the pipeline | **Stays green.** The variant remains and still carries the brace-rung and model-anchor cases the refutation §B carves out. |
| `contact_regression_gate.rs:324` `an_attestation_refusal_routes_to_quarantine_rather_than_a_pipeline_error` | the initial-build refusal path | **Untouched** — different stage, different path. |
| `runner.rs:7809`/`:7821`/`:7837` brace fixed-point unit tests | convergence, unplaceable, linked-plan outcomes | Signature-only change; pass an empty set. |
| `forest.rs` quarantine-sweep tests | the union dedup and the all-or-nothing `require_active` contract | **Stay green.** The plan/apply split is required to preserve them exactly; a test asserting `plan_quarantine_subtrees` returns precisely the order `quarantine_subtrees` removes is added. |

**Blast radius.** Models whose brace refusals disturb no contact accounting are **expected to be
bit-identical** — that must be **proved** by a corpus run, not asserted. Models that die at
Structure today die at Brace instead, with a better message; they were already dying. No digest
is expected to move, because nothing that is folded into a digest changes value on a surviving
model — again, expected, and the corpus run is the proof.

**Revert proof.** A test that condemns a carrier whose stub's terminal is sealed `Routed` and
asserts (a) the tree is untouched and (b) `finalized_unrouted_terminals` is never reached with
the illegal pair. If someone later "fixes" this by admitting a fourth pair, that test goes red.

### Stage C — the run-invariant memo (§2.4 item 1) — lands SECOND, before the retry

`SurfaceAnchorPool::from_geometry` hoisted at **both** sites — `runner.rs:3844` in
`route_initial_cco_state` and `runner.rs:4568` in `rewire_state`; take both or the win is
silently halved — and the legacy pre-routing replay hoisted on the same argument. Keyed on the
geometry and configuration digests. Two obligations that must land with it, not after it: the
work charge becomes incremental with the memo rather than pricing a fiction, and the receipt
`charge.covers_execution(meter)` must still hold.

**Why it is second and not "any order".** Those two sites are the Routing and Rewire stages,
which are 85 % of a legalization pass on the drilled shell and 81 % on the oriented solid (§0).
Landing C before B means every extra pass B introduces is paid at the reduced price. Landing it
after means the first measurement of B's cost measures a cost we already knew how to cut.

**Proof.** Every corpus artifact bit-identical, and a sabotage test: corrupt a stored value and
the artifact must change.

### Stage B — the legalization retry, at attempt 0 only

**Preconditions, both of which are measurements rather than opinions.** Stage A's corpus count
(S-2), so the class is sized before the instrument is built; and the determinism replay (S-1),
because the whole loop rests on a plan replaying identically — route the same plan twice and
compare the transcripts bit for bit.

**What lands.** Five pieces — the fifth is new in this revision and it is the one that keeps the
mechanism sound.

1. **The steering channel.** `route_initial_cco_with_limits` (`runner.rs:3785`) takes a borrowed
   no-good set keyed as §1.3 states. The tournament **passes over** a suppressed candidate with a
   new named reason. Passing over is all it does: a terminal whose candidates are all suppressed
   **does not retire here**. It signals that its steering is spent, which is what opens a
   confirmation round (piece 5).
2. **Candidate identity.** The arm's endpoint coordinates by exact bit pattern **together with**
   `family`, `generation_index`, `price_ml` and `clearance_rows` — the four fields
   `CcoCandidateEvidence` (`cco.rs:605`) already discriminates on. Endpoints alone over-match and
   would suppress a second, distinct candidate that happens to share them (§1.3). The endpoints do
   not exist in that struct today; this is hand-written plumbing, not an existing field.
3. **The abandon outcome.** `ChainOutcome` (`runner.rs:1298`) gains a third arm carrying the
   context back plus the no-goods the pass learned. The stage macro at `runner.rs:1374` is
   untouched; the new arm is produced only by the brace stage, and only when the abandon rule is
   satisfied (§1.3) — which means only when a sealed-`Routed` terminal's **own chosen arm** was
   condemned. Cascade victims travel in the payload as victims and steer nothing.
4. **The loop.** One `loop` wrapping the **attempt-0** chain call (`runner.rs:1596`) — and
   **not** the trial chain call at `runner.rs:1874`, which keeps today's
   `restore_prepared_digests` / `into_trial_rollback` path exactly (§1.4). It holds the no-good
   set, restores the five prepared digests on each abandoned pass exactly as the trial path does
   at `runner.rs:1888`, and matches each no-good's context tuple before honouring it — a
   mismatch is a **miss**, not an error.
5. **The confirmation round, which is the only thing allowed to retire a terminal.** When a
   terminal reports its steering spent, the loop enters a round for it: freeze the no-good set,
   drive the terminal through its remaining arms one per pass in the tournament's own order, and
   (a) end the round and proceed on the first arm that survives bracing; (b) retire the terminal
   `UnroutedNoWinner` **in the pass that measures the last arm** if all are measured illegal; (c)
   **void** the round if any other terminal's arm is condemned during it, abandoning on that
   terminal's new no-good and re-entering the round later. The round's per-arm margins and radii
   go into the sealed `NoWinnerEvidence` variant beside the three at `cco.rs:657`, together with
   the round's identity and the §1.3 arithmetic; steering-history margins are carried separately
   and labelled as such.

**One obligation on this stage that is not a feature, and the previous revision stated it as a
fact instead.** §1.3's termination argument needs the context universe to be finite, which needs
this:

> **The no-good set — and anything derived from it — must NOT be an input to any digest in the
> context tuple**, in particular `controller_digest_before`. The loop holds the set (piece 4),
> which puts it exactly where the hazard is.

If it leaks in, every context is fresh on every pass, every no-good misses forever, the same
candidate is chosen and condemned every pass, and each of those passes satisfies the abandon rule
with a genuinely new triple while making no progress — a loop that never ends and never trips the
one rule meant to catch that. **The test:** grow the no-good set by an arbitrary amount and assert
every digest in the context tuple is byte-identical. It is cheap, it is mechanical, and without it
the whole termination argument is circular.

**The `candidates_feasible` question, named rather than decided in passing.** An excluded
candidate must be counted somewhere. The first version counted it **out of**
`candidates_feasible`, chosen so the CCO transcript digest (which folds `candidates_generated`
and `candidates_feasible`, `cco.rs:2393`–`:2411`) moves only for excluding models. That buys
digest stability with the meaning of a sealed, physics-named counter: `candidates_feasible`
today answers *"how many arms passed feasibility"* and would afterwards answer *"how many passed
feasibility and were not refused in an earlier pass"*. Refutation §F.1 reads that exact field to
make its "arms existed, clearance refused them" argument, and after the overload that reading
needs a footnote nobody will have.

* **This record's recommendation: keep `candidates_feasible` as physics** and add a separate
  folded `candidates_excluded`. Feasibility keeps one meaning. **The price is one deliberate,
  corpus-wide transcript re-freeze**, paid once, with the reason written down.
* **The alternative** is the overload, which keeps the digest still for non-excluding models. It
  is coherent, and it is cheaper today.

This is a choice about what a sealed number means, so it goes to Craig as authorization item 5
(§5) rather than arriving as an implementation detail.

**Reporting, per the no-cap doctrine.** `RepairRunReport` (`coverage_repair.rs:671`) gains
`legalization_passes`, `no_goods`, per-terminal no-good counts, `confirmation_rounds` and
`voided_rounds`, printed on every path and read by no branch.

**Files.** `cco.rs` (the no-good parameter, the candidate identity, the pass-over reason, the
evidence variant), `runner.rs` (the outcome arm, the loop, the confirmation round, the report
fields), `coverage_repair.rs` (report fields).

**Tests that pin today's behaviour, and how each moves.**

| test | pins | moves |
| --- | --- | --- |
| `runner.rs` `a_stage_error_is_not_discarded_at_attempt_zero` | an attempt-0 stage failure stays `Failed { phase, detail }` | **Half unchanged.** A brace refusal that is *legalizable* no longer reaches attempt-0 failure; every other stage failure does. The test gains a sibling that pins "an abandoned pass is not a failure". |
| `a4_transactional_controller.rs` trial-rollback tests | a trial's stage refusal is one ledger row and the next candidate | **Untouched by design.** The loop does not wrap `runner.rs:1874`. A test asserting a trial's disturbing brace refusal is still a rollback pins the C3 precedence rule. |
| the sealed-record digest goldens (the toed-slab digest at `support_runner_integration.rs:305`, `:580`, and `plinth-verify/tests/coverage_gate_toed_slab.json:1`) | a model that closes at attempt 0 is bit-identical | **Stay green under the overload option.** Under the recommended `candidates_excluded` option they re-freeze once, corpus-wide, deliberately. |
| the CCO transcript digest (`cco.rs:2393`–`:2411`) | the tournament's own accounting | Depends on the authorization above. Either it moves only for excluding models (overload) or once for every model (new counter). |

**Blast radius.** Any model with zero exact-clearance refusals runs exactly one legalization
pass with an empty no-good set and no confirmation round, so it is unaffected by the loop itself
— and that must be proved by a corpus run. Models that do steer pay extra passes; the counts and
the wall time are printed. **The named risk, stated plainly and now larger than the previous
revision claimed:** a model where the clearance mask is unsatisfiable pays steering passes, **and
then a confirmation round of up to one pass per arm to retire each terminal** — up to ~100 passes
at 17.8 s per retirement, where 100 is the largest arm count MEASURED at one terminal and it was
measured on `zombie-3-a`, not on the Bat. A round voided by another terminal's refusal is paid and
re-run. If several terminals fail together, the steering passes **multiply** over the backtracking
tree rather than adding, and each retirement inside that tree carries its own round. That combined
bound is finite and **not measured**; Stage A's corpus count is the first measurement. It is loud,
and it is not capped. If it proves intolerable the answer is #51's mask fix or a ruling from Craig
— not a cap arriving by the back door.

**Revert proofs.** Five, and three of them exist because of this revision.

1. **Steering works.** A fixture whose first-choice arm is exactly illegal and whose second is
   legal: the first pass abandons, the second seals a different arm, the final accounting shows
   `Routed`↔`Consumed`, and the run emits.
2. **A stale key misses.** Change the context by one bit and assert the no-good **misses** — the
   candidate is regenerated, re-measured and re-refused on its own physics, and no error is raised.
3. **The abandon rule holds.** A pass that would abandon while learning no new no-good must refuse
   by name (T6) rather than loop.
4. **A no-good cannot retire a terminal.** A fixture where every arm carries a no-good, and where
   the arm the terminal would take **is legal in the current world**: assert the terminal does
   **not** retire, that a confirmation round runs, and that the round ends on that legal arm and
   the run emits. **If someone later "optimizes" the round away and retires on the no-good count,
   this test goes red** — it is the direct guard on the defect that made the second review return
   FLAWED, and the reason it can be written at all is that the failure it catches is silent
   otherwise: a dropped support under a clean seal.
5. **The no-good set stays out of the context digests.** Grow the set arbitrarily; assert every
   digest in the context tuple is byte-identical. This is the finiteness obligation above, and
   without it the termination argument is circular.

Note that proofs 4 and 5 are both **sabotage-shaped**: each one fails if a plausible-looking
simplification is made later. That is deliberate. A design whose soundness rests on someone not
taking an obvious shortcut needs a test standing on the shortcut.

### Stage D — carry the legalization record into the evidence sidecar

The no-good ledger, the pass count and the confirmation-round record join the coverage refusal
sidecar, and the corpus counts are pinned. **This is disclosure, not the record**: the round's
measured margins and radii already live inside the sealed `NoWinnerEvidence` from Stage B (§1.3),
so a defective sidecar cannot hide a missing measurement or a retirement without a round. This is
the schema move, and it should ride whatever other schema move the
A4 lane already owes rather than minting a second one.

---

## §5. THE DECISION MEMO — for Craig

*Self-contained. Read this alone if you read nothing else.*

### Words this memo uses

* **Seal** — a record written once during a run and never changed afterwards. Later stages must
  agree with it; if one disagrees, the run stops.
* **The audit** — the check that every support point's sealed record and its live state in the
  tree agree.
* **Making the plan legal** — throwing away a planning pass that produced an illegal support and
  planning again, steered away from the arm that failed. The design calls this "legalization".
* **A steering note** — the record we keep when an arm is measured illegal: *"this support point
  was seen to fail with this arm once; prefer a different one next time."* It changes which arm
  gets **tried**. It is not an answer, and on its own it is never a reason to give up on a
  support point. (The design calls it a "no-good".)
* **A confirmation round** — the only thing allowed to give up on a support point. We stop
  learning, hold everything else still, and make that one point try each arm it has left, one
  arm per planning pass. If any of them works, we carry on. Only if every one of them is measured
  inside the model do we mark the point unsupported — and we do that **in the same pass that
  measured the last arm**, so the verdict and the measurement come from the same world.

### The dilemma, in two sentences

The routing stage seals a record saying a support point is routed, but the brace stage later
measures that support's true thickness and finds it inside the model, and condemning it makes
the sealed record and the live tree disagree — which the audit correctly refuses, killing the
Giant Bat. We already proved we cannot predict the brace stage's answer earlier, because the
number it depends on does not exist yet and moves in both directions, so we must either weaken
what "routed" promises, or make the promise come true, or stop the thickness from changing.

### The three coherent directions

**Option 1 — two seals.** "Routed" is downgraded to "routing's decision". A second, final seal
is created after bracing, and only that one may be emitted from.

* *Result:* the disagreement stops being a contradiction, because the first seal no longer
  claims final admission.
* *Trade-off:* the load repartition has to run a second time after bracing, which the binding
  spec sequences before bracing — a spec deviation. Worse, the second repartition changes every
  support's load, which changes every thickness the brace stage just assigned, so the tree the
  second seal certifies is not the tree it measured. And it still does not fix the Bat: the Bat
  fails on its first pass, and there is no mechanism to try a second one.
* *Reversibility:* poor. The evidence sidecar and the verifier golden would carry two seals;
  backing out is a second format change.

**Option 2 — keep "routed" strict, and make it reachable (RECOMMENDED).** "Routed" keeps
today's meaning exactly. When bracing finds a routed support illegal, we throw that whole
planning pass away and plan again, steered away from the arm that failed.

* *Where the retry runs:* **only on the model's own first plan.** A repair trial that fails
  bracing is still dropped and the next candidate tried, exactly as today. The retry is never
  wrapped around a trial.
* *Result:* nothing is admitted late, no fourth pair is ever minted, the audit is untouched,
  and the Bat gets a second choice of arm. If a support point genuinely has no arm that works, it
  retires as "no winner" — one of the three pairs the audit already admits — and the run either
  emits with an honest hole or refuses with the hole named in millimetres.
* *Why it stops — and I have had to rewrite this bullet, because the version you were shown
  before was wrong.* Each thrown-away pass leaves a **steering note**: "this support point was
  seen to fail with this arm; prefer another one here." Each pass leaves a note that was not
  there before, no note is ever left twice, and the number of possible notes is finite, so the
  passes cannot go on forever. **What a note is not:** it is not an answer we can reuse. The
  fingerprints a note is filed under describe the tree **as that one support point saw it before
  it chose**. The thing that made its arm illegal is the arm's final thickness, and that is
  worked out later, from the whole braced tree — including support points chosen **after** this
  one. Those move every pass. So a note is a good reason to **try something else** and never a
  reason to conclude anything. The earlier version of this memo let a pile of such notes decide
  that a support point had run out of arms; it could not, and the cost of that mistake would have
  been a model that prints with a support quietly missing and every check still green. **A support
  point is now only given up on after a confirmation round** (see the glossary above): everything
  else is held still, the point tries each arm it has left, and it is marked unsupported only if
  every one of them is measured inside the model — the last of them in the very pass that marks
  it. If anything else moves during the round, the round is thrown away and re-run later.
* *Trade-off, and it is bigger than the earlier version said.* One extra plan-and-brace pass per
  illegal arm, at about **18 seconds** on the drilled shell and **15 seconds** on the solid,
  measured on the Bat at one arm per pass. **On top of that, giving up on a support point now
  costs a confirmation round** — up to one extra pass per arm that point has left, so up to
  roughly 100 passes, where 100 is the largest number of arms we have counted at one point,
  **on a different model** (zombie-3-a, 42 to 106 arms per point), not on the Bat. A round that
  gets thrown away is paid for and run again later. **And if several points fail at once the
  ordinary passes multiply rather than add**, with a round for each point that ends up given up
  on. We have not measured how often any of that happens; the first stage's corpus count is that
  measurement. All of it is printed, and none of it is capped. I would rather hand you a slower
  loop than a fast one that drops supports.
* *Reversibility:* good. The first stage is one classification and reverts to today's bytes.
  The last stage is one loop and one parameter; delete the parameter and you are back at the
  first stage.
* *Why it is not the direction you already refused:* that direction asked us to **guess** the
  brace stage's answer while choosing arms. This one **measures** the answer, and — after the
  correction above — only lets a measurement decide something in the world it was taken in. No
  approximation, no bound, no new number.
* *What it can still do to a run:* one named stop. If a planning pass throws something away and
  learns nothing it can steer with — the condemned support belongs to no point's own choice — then
  planning again would produce exactly the same pass. It stops there and says so, naming the
  support point, the member and the millimetres. That is a real new refusal and I am not going to
  describe it as free.

**Option 3 — stop the thickness from changing (declined, and it is the direction our own
refutation preferred).** Leave routing, the seal and the audit alone. Instead refuse the resize
step that makes a model-touching support thick enough to end up inside the model.

* *Why it is attractive:* it is the smallest possible change. Nothing about the seal moves.
* *Why I decline it:* the thickness grew for a reason. On the drilled shell the arm went from
  0.949 mm to 1.410 mm **because the load path demanded it** — that number is a strength
  requirement, not a mistake. Refusing the growth does not remove the defect; it swaps a
  clearance defect for a support that is too thin for its load. A clearance defect announces
  itself in the audit. A too-thin support does not: it prints, and it snaps. And it does not
  even cover the class — on the solid the arm **shrank**, 1.048 mm to 0.902 mm, and it was
  **still** inside the model. "Do not let it grow" says nothing about that one.
* *If you disagree, say so and I will cost it properly.* It may still be right as its own task
  for the resize-timing family; I am declining it as the answer to this seam.

### My recommendation, and the reason

**Option 2.** The reason is one sentence: **Option 1 spends the audit seal and still does not
fix the Bat; Option 3 trades a defect that announces itself for one that does not, and only
covers half the measured cases; Option 2 fixes the Bat while leaving the seal exactly as it
is.** Every one of the three accounting functions — the three-pair check, the removal at the
point the spec names, and the recompute-from-state tripwire — is unchanged, at every call site,
under Option 2. Under Option 1 all three change and one of them (the repartition) deviates from
the binding spec.

Two supporting reasons. First, the retry is **mostly a generalization of machinery the coverage
controller already has** — the context handback on refusal, the set of tried things that survives
a rollback, the digest restore, the counters that report and never decide — not a second parallel
mechanism. The genuinely new work is three things: a parameter passed into the routing tournament;
recording enough about each arm to name the same arm on the next pass, which is its endpoint
coordinates **and** the four things the tournament already tells candidates apart by; and the
confirmation round. **The round is not a generalization of anything** — the coverage controller
never has to prove it has run out, it just moves on — and I have stopped describing this work as
two easy pieces, because the third one is real control flow and it is the piece that keeps the
design sound.

Second, the loop **ends** rather than running forever, and I want to be exact about how strong
that is. It is a search with backtracking, not a straight line. It ends because it never repeats
a combination, and the combinations are finite. It is **not** "at most 100 passes". I do not
know the real number and neither does anyone else until the first stage counts the class. An
earlier draft of this memo claimed the stronger, simpler bound; that claim was wrong and I have
withdrawn it.

**Third, and this is the correction you should weigh hardest, because it is the one an
independent check caught rather than one I found myself.** The version of this design you were
shown before had a hole in exactly the place I told you was safe. It let the steering notes add
up to a verdict — "this point has been refused with every arm it has, so mark it unsupported" —
and every note was a real measurement, so the arithmetic was perfect and every existing check
would have passed. But each measurement came from a different arrangement of the tree, because
every thrown-away pass rearranges it. The result would have been a model that emits, seals
cleanly, audits cleanly, and is missing a support that was never actually shown to be impossible.
That is the failure mode I care about most on this project: not a loud refusal, but fewer
supports, silently. The fix is the confirmation round, and it costs real time. I am telling you
the price went up rather than quietly keeping the number I gave you last time. **The
recommendation itself has not changed** — Option 1 and Option 3 are worse for the reasons above,
and neither is made better by this — but the honest cost of Option 2 is higher than the previous
memo said, and the ordering of the stages matters more because of it.

### The cost band

I am giving you work shape, not calendar time.

**First stage (name the class, abandon instead of quarantining):** three files
(`brace.rs`, `forest.rs`, `runner.rs`), one added parameter, one read-only query — which is a
small refactor of an existing function into "work out what to remove" plus "remove it", so the
query cannot drift from the removal — and two new named outcomes. Four new tests, one existing
test gains siblings, and **one test the earlier plan asked for is deleted rather than written**:
it asserted a behaviour the code cannot produce, and I only found that out because the second
review checked it against the source. **No digest, golden or schema move is expected — and the
corpus run in this stage is what proves it.** One corpus run to prove bit-identity on every model
that is not affected, and the count it brings back now has to separate support points that failed
on their **own** chosen arm from ones that were only knocked out by a neighbour's failure — those
are treated differently and I need to know how often the second kind happens. This is still the
small, safe, revert-proof one.

**Second stage (the invariant-work saving):** one function that is rebuilt on every pass but
depends only on the model and the settings, hoisted at both of the two places it is built. Those
two places are the routing and rewiring stages, which are **85 % of a pass** on the Bat's shell.
This lands **before** the retry, so every extra pass the retry adds is paid at the reduced price.
It changes no verdict, and a sabotage test proves it: corrupt the stored value and the output
must change.

**Third stage (the retry itself):** four files plus the evidence record. The genuinely
hand-written seams are three: the steering channel into the routing tournament, the arm's identity
(which has to be the arm's endpoints **and** the four things the tournament already tells
candidates apart by — endpoints alone would suppress two different arms with one note, and every
mistake in that direction costs supports), and the confirmation round. The round is the piece that
grew in this revision: it drives one support point through its remaining arms, one per pass, holds
everything else still while it does, and throws itself away and restarts if anything else moves.
Two corpus runs: one to prove bit-identity for unaffected models, one to measure the extra pass
count and wall time on the Bat, the Warhorse and the treant — **and that measurement must count
the confirmation-round passes separately**, because they are the part I cannot price for you
today. The named cost risk is the extra passes on a model whose clearance mask is unsatisfiable,
and the answer to that is task #51, not a cap.

**Two of this stage's tests exist to stop a later, reasonable-looking simplification.** One
asserts that a support point is **not** given up on just because every arm carries a steering note
— it must go through a confirmation round, and if an arm turns out to work in the current
arrangement the round must find it. The other asserts that the notes themselves never leak into
the fingerprints the notes are filed under; if they did, every note would miss forever and the
loop would never end while looking, pass by pass, like it was making progress. Both failures are
silent without the test, which is why they are written down as tests rather than as warnings.

**Fourth stage (disclosure):** the pass record joins the existing evidence file. This is the only
schema move, and it should ride a schema move the A4 lane already owes.

### What I am asking you to authorize

1. **The contract.** Sealed `Routed` keeps its current final-admission meaning. Option 1 is
   declined. Option 3 is declined on the reasoning above. Say yes, or name one of the other two.
2. **The first stage on its own, first.** Land the classification and the named brace-stage
   refusal with no retry, prove bit-identity on the corpus, and count the class across all 45
   models before building anything else. This is the "size the prize before building the
   instrument" step, and it is also what tells us how many support points fail at once — the
   number that decides whether the retry's cost is small or large.
3. **The invariant-work saving before the retry, not after.** It cuts most of the cost of the
   thing we are about to do more of.
4. **The retry after that measurement, not before.** With the cost printed and uncapped. I would
   like your explicit agreement that the bad case — a support point that has to be walked through
   every arm it owns to be given up on, and the worse case where several points fail together and
   the passes multiply — is reported loudly and **not** capped, because that is your no-cap
   doctrine applied to a number I expect someone to want to cap. **The confirmation round is where
   the pressure to cap will come from**, since it is the visible, repetitive, obviously
   "wasteful"-looking part. It is the part that keeps supports on the model, and it is not a
   budget: it is one pass per arm the point actually has, and it stops early the moment an arm
   works.
5. **One sealed counter's meaning.** The tournament records "how many arms passed feasibility".
   A ruled-out arm has to be counted somewhere. I want to **keep that counter meaning exactly
   what it means today** and add a second counter for ruled-out arms. The price is that one
   sealed fingerprint changes for every model, once, on purpose. The cheaper alternative is to
   fold ruled-out arms into the existing counter, which keeps the fingerprint still for models
   that never rule anything out but leaves one number with two meanings. I recommend paying the
   one-time cost. Your call, because it is a sealed number.
6. **Acknowledgement that the price went up.** The previous memo priced this as steering passes
   only. It now costs steering passes **plus** a confirmation round for every support point we
   give up on, plus any round we have to throw away and re-run. I am not asking you to approve a
   number, because I do not have one — I am asking you to approve going ahead knowing the number
   is larger than the one you were shown, and that the first stage's count is what turns it into
   a number.
7. **One thing I am NOT asking for**, so it is on the record: no new registered constant, no
   fourth admitted pair, no second removal point, no cap on the confirmation round, and no
   resurrection of the ranking rule.

### What I am running before I ask you for the retry

The design assumes the routing cadence is deterministic given the tree state — that is what
makes a refusal mean the same thing on the next pass. **I do not assume it. I will measure it**
before the retry is built: route the same plan twice and compare the transcripts bit for bit.
This is one cheap run, the whole retry rests on it, and an earlier draft of this memo offered it
to you as an option instead of simply running it. That was the wrong shape for a precondition.
If the replay is not bit-identical, the retry does not get built and I come back to you with
that result instead.

**This check now carries more weight than it did last time, and you should know why.** The
confirmation round is only honest if, when a support point takes a given arm, the rest of the
tree follows from that choice and nothing else. That is what lets me say "every arm it could have
taken was measured in the arrangement that taking it produces" instead of "we saw a lot of
failures". If the cadence is not reproducible, that sentence is false and the round proves
nothing — so the same one cheap run is now the precondition for the soundness of the fix, not
only for the efficiency of the loop.

---

## §6. What this record does NOT claim

* **Not** that the Giant Bat emits. It claims the Bat reaches an emission or a disclosed
  refusal instead of an accounting-integrity error, and that which one is a measurement.
* **Not** that the refutation is wrong. It accepts §C entirely: the brace verdict cannot be
  predicted when candidates are chosen. It uses the measured verdict instead of predicting it.
* **Not** that the retry loop is bounded by ~100 passes. That claim was in the first version and
  is withdrawn. The bound is finite, combinatorial and unmeasured (§1.3, §0).
* **Not** that a steering no-good is valid against any world other than the one it was measured
  in. It is keyed on the prefix a terminal's tournament reads; the verdict it records was decided
  by a radius the suffix assigns. It may steer a choice and it may never decide an outcome — no
  retirement, no removal, no refusal rests on one (§1.3, §2.3).
* **Not** that `k` measured negative margins amount to exhaustion. They are `k` true numbers from
  up to `k` different worlds. Only a completed confirmation round establishes that nothing the
  terminal could have chosen was legal, and only in the world-family the round held still (§1.3).
* **Not** that the retry loop can never end a run. It introduces no new failure for the class it
  is built for, and it names one — a pass that learns nothing (T6, §3) — which is **reachable**
  and is therefore a genuine addition. The previous revision claimed the named refusal merely
  renamed an error that hard-fails today; that claim is withdrawn, because the case it described
  is not reachable on the code (§3, §4 Stage A(a)).
* **Not** that the 89 rejected trials can be saved by storing results. §2 argues the opposite,
  precisely.
* **Not** that Option 1 is incoherent. It is coherent and Codex is right about its shape; it is
  declined on cost, on the spec deviation it forces, and because it does not fix the Bat alone.
* **Not** that Option 3 is a bad idea in general. It is declined as the answer to this seam, on
  the evidence in §1.5, and it may still be right for the resize-timing family.
* **Not** that the extra passes are cheap, and **not** that the earlier price still stands. §4's
  named risk is real and unmeasured, and confirmation rounds made it larger than the previous
  revision claimed.
* **Not** that the previous revision of this record was merely imprecise. It was unsound in a way
  that would have dropped supports under a clean seal, and §1.3, §3, §4 and §5 are rewritten
  rather than reworded.
* **Not** a proposal to admit a fourth pair, add a second removal point, move any registered
  number, or resurrect the ranking rule.

## §7. Open items and escalations

| # | item | owner |
| --- | --- | --- |
| S-1 | Is the routing cadence bit-reproducible on a replayed identical plan? One cheap measurement. **Precondition of Stage B, not an option** — it runs whether or not Craig asks for it. | this lane |
| S-2 | Stage A's corpus count: how many of the 45 models hit an accounting-disturbing brace refusal; **how many terminals are disturbed in one sweep** (the exponent in §1.3's bound); and **how many of those were disturbed on their own chosen arm versus purely by cascade** (which says whether the reachable T6 is a corner or a common outcome). Unknown today. | Stage A |
| S-8 | Confirmation rounds are unpriced. Stage B's Prove run must count round passes and voided rounds **separately** from steering passes, or the cost of the sound retirement rule stays invisible inside a single pass total. | Stage B Prove |
| S-9 | The finiteness obligation — no no-good-derived state in any context digest — is an implementation property with a test (§4 Stage B). It is listed here because it is the kind of thing a later refactor of controller state can break silently from a distance. | binding on Stage B and after |
| S-3 | The extra-pass cost on a mask-unsatisfiable model, measured **after** Stage C's hoist. Depends on #51's landing state. | Stage B Prove |
| S-4 | Task #47 is discharged by Stage A for the trial path; its title should be re-read once Stage A lands. | #47 |
| S-5 | No new registered constant is needed anywhere in this plan. If an implementer wants one, that is an escalation to Craig, not a design detail. | binding on all stages |
| S-6 | `candidates_feasible` keeps its physics meaning and a new `candidates_excluded` is added, at the price of one deliberate corpus-wide transcript re-freeze — **or** the existing field is overloaded. Memo authorization item 5. | Craig |
| S-7 | Direction (1), constrain the resize, is declined for this seam in §1.5 and never costed. If Craig overturns the decline, it needs its own design pass in the resize-timing family (#25). | Craig, then #25 |

---

## §8. Review response — where each finding landed

This record has been reviewed twice. §8.1 folds the first review; §8.2 and §8.3 fold the
independent verification of that fold, which returned **FLAWED** and is the reason this revision
exists. Both tables say **where** each finding landed, so a reader can check the fold rather than
trust it.

### §8.1 First review response — the adversarial review at `7e668e9`

This record's first revision was reviewed adversarially at `7e668e9` with the verdict
SOUND-WITH-CHANGES. Every finding was folded at that revision; the table below says where. **Two
of those folds were themselves defective and are corrected in §8.2** — C1's key (sound against
staleness, unsound against the suffix) and C5's "earned" half (disclosure landed, the substance
did not). The rows are left as written so the sequence is auditable, with the correction noted.

| finding | where it landed |
| --- | --- |
| C1 — exclusion key and the termination proof | §1.3 (context digest tuple from `cco.rs:642`, miss-not-error, the abandon rule, backtracking termination, the withdrawn give-up claim), §0, §2.3, §3, §6. **Superseded in part by V1/V2 (§8.2):** the key fixed staleness and not suffix-dependence, and the finiteness clause was an unstated obligation. |
| C2 — worst-case arithmetic and per-pass cost | §0 (derivation table, 17.8 s / 15.1 s, the 16.4 s trap), §1.3, §4 Stage B, §5 |
| C3 — the loop runs at attempt 0 only | §1.4 (stated first, three reasons), §4 Stage B item 4, §4 Stage B tests, §5 |
| C4 — the third direction | §1.5 (new), §1.6, §5 Option 3, §6, S-7 |
| C5 — measured margins inside the sealed evidence | §1.3 (the meaning-change paragraph and verifier obligation 1), §3 T2, §4 Stage B item 1, Stage D. **Only half-folded**, per V1a (§8.2): the margins went inside the seal, but "exclusions must be EARNED" was not satisfied until the confirmation round. |
| C6 — stage order A → C → B | §4 header, §4 Stage C, §5 cost band and authorization item 3 |
| C7 — corrected anchors and new plumbing | §2.4 item 1 (`runner.rs:3844`/`:4568`), §1.3 and §1.4 item 3 (endpoints are new), §0 (the absent `CoverageMeasurement` line at `runner.rs:1603`) |
| C8 — Stage A classification, planner prefix, `terminal_of` | §4 Stage A (a), (b), (c); §3 T6. **(a) partly withdrawn** per V3 (§8.2): classifying on all dispositions is kept, but its justification — an unnamed engine error it retires — was not established on the code, and it conflicted with (b)'s exact-prefix requirement. (b) and (c) stand unchanged. |
| C9 — `candidates_feasible` meaning | §4 Stage B (named as a choice with a recommendation), §5 authorization item 5, S-6 |
| C10 — doctrine, clean conditional on C1 | discharged by C1's fold; `FROZEN_PASS_CAP` noted as pre-existing in §4 |
| C11 — the eight memo rewrites | §5 throughout: glossary, three options, where the loop runs, the corrected worst case, the cost parts, "expected and proved" rather than asserted, the fifth authorization item, and the determinism check as a precondition |

### §8.2 Second review response — the independent verification of the C1–C11 fold

The revision above was verified independently at `83e3215`. **Verdict: FLAWED.** Termination
survived the attack; exclusion **soundness** did not, and the break was inside §3's own flagship
trace. That verification's findings are folded below. Where a finding says a previous claim was
wrong, this record now says so in the place the claim was made, rather than only here.

| finding | verdict on it | where it landed |
| --- | --- | --- |
| **V1** — the exclusion key covers the **prefix** a tournament reads, but the stored result is the **brace verdict**, whose deciding radius depends on the **suffix**. An early terminal's context is stable, so its exclusions always hit while the world that measured them is gone; after `k` hits it retired `UnroutedNoWinner` without exhaustion in any world → a real contact dropped, fewer supports, under a clean seal. | **Accepted in full.** The break is real and the direction of the error is the dangerous one. | The no-good is demoted from a stored answer to a **steering preference** (§0, §1.3, §2.3). Retirement now requires a **confirmation round** measured under the current no-good set, the last arm measured in the pass that retires (§0, §1.3, §3 step 8, §4 Stage B piece 5, verifier obligation 3). §5 "Why it stops" rewritten. Revert proof 4 guards it. |
| **V1a** — re-arms review finding **C5**: exclusions must be **earned**; `k` true measurements from `k` different worlds do not establish that no arm is legal in this one. §8's C5 row over-claimed. | **Accepted.** The disclosure half of C5 landed; the "earned" half did not. | §1.3's meaning-change paragraph now states the failure at full width; verifier obligation 1 is strengthened from "`k` measured negatives" to "the arms of one confirmation round", with steering history labelled separately. The C5 row is corrected in §8.3 below. |
| **V1b** — cascade variant: a terminal disturbed only by another member's condemned arm loses a legal arm for someone else's failure, and can retire with no arm of its own ever measured illegal. | **Accepted.** | A no-good is recorded **only** for a terminal whose own chosen arm was condemned; cascade victims are reported and steer nothing (§1.3 cascade split, §3 closing paragraph, §4 Stage A classification, Stage B piece 3, Stage A test table). |
| **V1c** — candidate identity by endpoint bit pattern alone over-excludes; `CcoCandidateEvidence` (`cco.rs:605`) discriminates by `{family, generation_index, price_ml, clearance_rows}`. | **Accepted**, confirmed by reading `cco.rs:605` at `83e3215`. | Identity is the endpoints **plus** those four fields (§1.3, §4 Stage B piece 2, §5 cost band). |
| **V1d** — widening the key to include the suffix makes exclusions sound but never-hitting (degenerate), so there is no free fix and the record must say so. | **Accepted**, and it is the reason the prefix key is kept rather than repaired. | §0 and §1.3 both state the trade explicitly; §2.3 states that the rule admits exactly two ways out and which one this record takes. |
| **V2** — the finiteness of the context universe is an unstated obligation: if exclusion-derived state enters any context digest, every context is fresh, every exclusion misses, and the abandon rule waves through infinitely many no-progress passes. | **Accepted.** The previous revision stated it as fact. | §0 and §1.3 name it as an obligation; §4 Stage B carries it as a stated requirement with a byte-identity test; revert proof 5; S-9. |
| **V3** — T6 as specified is probably unreachable **and** contradicts C8(b): `children_of` (`forest.rs:562`) filters to live members so the cascade cannot collect an inactive one, and a directly-named inactive key hits the exact-prefix planner's `ForestError` before any classification. | **Accepted**, confirmed by reading `forest.rs:562` and `forest.rs:973`–`:996` at `83e3215`. Both claims cannot stand together. | The previous T6 is **withdrawn**, and the §4 Stage A test that asked for it is **deleted**, replaced by one pinning today's `ForestError` (§3, §4 Stage A(a) and test table). A different, **reachable** T6 — the pass that learns nothing — is named, priced and tested in its place (§0, §1.3, §3, §4 Stage A, §5 Option 2). |
| **V4** — nine overstated wordings. | **All nine rewritten.** | Itemised in §8.3. |

### §8.3 The nine overstated wordings, one by one

| # | the sentence | what it says now |
| --- | --- | --- |
| 1 | §1.3 — *"in this exact tree and controller state, this candidate was measured illegal at its true radius"* | Withdrawn in place. §1.3 now states that the key is the pre-routing prefix while the true radius is assigned afterwards from a suffix-dependent braced path, and narrows the claim to *"measured illegal in at least one world that had this prefix"*. |
| 2 | §0 — *"no stored answer is ever used in a situation where it was not measured"* | Deleted. §0 says instead that a no-good is **not a stored answer at all**, and that nothing which decides an outcome is inherited across passes. |
| 3 | **§5 memo, Option 2 "Why it stops"** — *"so no answer is ever reused where it was not measured"* | Rewritten from the ground up in plain language, and flagged to Craig as a correction rather than a polish: notes steer, notes never conclude, and giving up on a support point requires a confirmation round. This is the sentence Craig rules on. |
| 4 | §1.3 obligation 1 / §3 T2 — *"'exhausted' is backed by `k` measured negative numbers"* | Replaced by the round requirement: the evidence must show the arms of **one** confirmation round, with earlier steering refusals labelled as steering history. |
| 5 | §1.3 — *"The Bat is unchanged … so §3's trace holds verbatim"* | Split. The Bat's **steering** path holds verbatim (§3 steps 1–7). Its **retirement** path does not, and §3 now spells out that terminal 298 — early, stable prefix — was the exposure rather than the safe case. |
| 6 | §0 / §1.3 — *"each context is generated by the finitely many choices at earlier terminals"* stated as fact | Restated as an obligation on the implementation, with the failure mode spelled out, a test in §4 Stage B, revert proof 5 and S-9. |
| 7 | §3 T6 / §4 Stage A(a) — *"today the same case reaches `require_active` … T6 replaces an unnamed engine error with a named physical one"* | Withdrawn against the code, with the two anchors quoted. §4 Stage A(a) now says classifying on all dispositions costs nothing but that **no such hole is demonstrated**, and that a directly-named inactive key correctly stays a `ForestError`. |
| 8 | §8 C5 row — recorded as landed | Corrected in §8.2 (V1a) and here: the disclosure half landed at `7e668e9`; the "earned, not merely consistent" half lands only with the confirmation round. |
| 9 | §6 — no item disclaiming cross-world validity | Added, together with two more: `k` margins are not exhaustion, and the previous revision was unsound rather than imprecise. |

**What did not change, stated so the fold is checkable in both directions.** The recommendation
is still **Option 2**. The verification did not challenge the seal contract, the three-pair
accounting, the attempt-0-only placement, the A → C → B → D order, the decline of Options 1 and 3,
or the termination argument itself — it explicitly confirmed termination survives, and confirmed
that T6 is not a cap. What changed is what a no-good is allowed to decide, what a retirement
costs, and which claims this record is entitled to make.
