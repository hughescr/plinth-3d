# Claude Code kickoff — continue the v4 CCO support-engine rebuild (ultracode)

Paste this file's contents as the opening prompt in a Claude Code session
whose working directory is this repo (`resin-print-prep/`, inside the
D&D Campaigns folder). It continues committed, verified work — read
before writing.

## Workflow mandate (ultracode)

Run an orchestrator + parallel-worker pattern, not a linear task list:
- YOU (the lead session) own architecture decisions, gate every merge,
  and commit at milestones. Delegate implementation to parallel
  subagents in ISOLATED WORKTREES with strict, disjoint file ownership
  (the one time two agents shared a tree here, a cancelled agent left
  half-applied edits that silently corrupted volume numbers — never
  again).
- EVERY non-trivial step gets an INDEPENDENT adversarial verifier — a
  different agent than the author, briefed to re-run the numbers and
  hunt counterexamples, reporting blocker/should-fix/note. This has
  paid for itself three times already (see git log: Phase 2 verifier
  found 2 real sizing blockers; Phase 3 verifier confirmed a rollback-
  corruption fix; the design verifier found 3 charter blockers).
- Surface progress to Craig continuously; small frequent commits.
- Craig's culture: TDD with tests that can fail; crash over silent
  corruption; contributing-factors analysis, not root-cause; push back
  on ambiguity; ground claims by measuring. 改善.

## Read first (in order)

1. `skills/support-placement/references/design-v4.md` — THE charter.
   §3 printability/curvature, §4 builder+certificates, §5 Xia junctions,
   §6 sizing floors, §7 rewiring, §8 transient, §9 bracing, §10 removal
   gate, §13 test plan, §14 phases (note: phases 4/5 were SWAPPED on
   measured evidence — bracing before curvature).
2. `skills/support-placement/references/design.md` — v0→v3.7 history,
   objectives, invariants. `tree-construction-literature.md` — theory.
3. `scripts/support_placement/` — tree.py (Member/Forest, certificate
   transactions), cco.py (insertion), rewire.py (topology mutation),
   engine.py (prepare/construct/finish staging + run()), verify.py
   (gates), chitubox_emit.py (incl. new synth_member_chain).
4. `scripts/tests/manual_treant_v4.py` — staged Treant driver
   (prep/build[:N]/rewire/brace/verify/emit). The `brace` stage already
   calls `engine.brace_and_size(mesh, forest, unrouted, surface_tree,
   plate_z, cfg, log=log)` — implement exactly that signature.

## Repo state (2026-07-21)

- git HEAD `1963241` on master (kickoff 06f3a49 → design 52d6367 →
  Phase 2 e059f9a → Phase 3 d154160 → staging 1963241). Git lives in
  the repo normally — in Claude Code you can use git directly (the
  rsync-to-/tmp dance in older notes was a Cowork sandbox limitation;
  ignore it).
- UNCOMMITTED (intentional, commit it first after a green suite):
  Chitubox emission wiring — `chitubox_emit.synth_member_chain`, the
  driver's emit/brace stages, +4 tests in test_chitubox_emit.py.
- Suite: 152 tests expected green (`uv run --frozen pytest -q` from
  repo root; uv only, no pip/requirements.txt). Last full-chunk runs
  were green post-cleanup but re-verify FIRST and commit the emission
  work as your step zero.
- Partial-Phase-4 edits from a cancelled agent were REVERTED to HEAD
  (cco/engine/rewire/tree + 2 test files). If anything looks
  half-implemented, diff against HEAD before trusting it.

## Baselines and targets (measured, frozen)

- v3.7 Treant: 22.85 ml members + 3.74 balls = 26.59 total
  (`scripts/tests/fixtures/treant_v3_7_baseline.json`); 465 routed / 176 unrouted / 74
  unsupported minima; 0 clearance, 0 connectivity violations.
- v4 fixtures (`scripts/tests/fixtures/v3_baseline.json`,
  `scripts/tests/fixtures/v4_p3_baseline.json`): table v4 0.8446 ml beats v3 0.8955;
  toed_slab 0.3727 vs 0.4442. All gates clean.
- v4 Treant WITHOUT bracing (200-contact partial): 74 ml — unbraced
  ~100mm columns at r≈2.9mm are the dominant waste. Bracing is a ~14x
  per-column lever (r_defl ∝ span^0.75).
- Volume target: beat 26.59 total ml meaningfully (aspiration ~20%+);
  coverage/minima are correctness and outrank volume.

## Remaining work, in order

1. **Commit the emission work** (after green suite).
2. **Phase 4 — transient + bracing (design §8/§9 + §14 note).**
   `transient.py`: restraint-activation model (a restraint exists only
   once its connecting structure has fully built — port v3
   `trunks.transient_worst_span` semantics, test-pinned in
   test_trunks.py); worst transient span per member; resize through
   tree.py setters ONLY (certificates must invalidate).
   `bracing.py`: X-pair cross-links (v3.6 twin single-rod rule) and
   ≤5mm model rungs (kind "model_rung", embed_end="p1") as NON-TREE
   Forest members (`Forest.add_brace`, excluded from Murray/topology,
   included in volume/clearance/connectivity/printability + the
   checkpoint protocol). Unclearable braces are never placed.
   `cco.py`: braced-cost candidate PRICING only (span capped at
   brace_free_span_mm + brace volume estimate) — construction still
   builds true members, the bracing pass fixes them. WARNING (measured):
   pricing-without-bracing makes things WORSE (1.57 ml table vs 0.845)
   — land pricing and bracing in the SAME merge, never separately.
   `engine.brace_and_size`: bounded fixed point (propose → resize down
   AND up → batch revalidate → drop failures, ≤3 iters), then a SECOND
   rewire.optimize pass, then re-fix. Wire into run() and keep the
   driver's brace stage working.
   Acceptance: fixtures not worse than fixtures/v4_p3_baseline.json;
   Treant build:200 member_ml < 30 (from 74); all gates clean;
   activation-timing + stale-brace-impossibility tests; independent
   adversarial verify before commit.
3. **Investigate at clean HEAD+bracing: Treant-scale full build gates.**
   A Frankenstein-state run showed 3 clearance violations and 248
   unrouted at full scale — numbers are untrustworthy (partial code),
   but clearance>0 at ANY scale is an invariant break if it reproduces
   clean. Root-cause before Phase 7 (suspect: revalidation sampling
   pitch vs the tighter 0.3mm verify tree, or spiral-fan candidates
   certified at pre-Murray radii).
4. **Phase 5 — removal-leverage gate** (design §10): handle + peel
   checks, ordered removal (cut-outer-to-expose-inner), hard gate only
   on caged/deadlocked residual sets, removal_effort metric + peel
   order in the log. Parameters are rough-estimates-pending-Craig's-
   hands.
5. **Phase 7 — full Treant + emission.** Full pipeline (staged driver
   or run() directly — no 45s cap in Claude Code), all gates green,
   volume vs 26.59, emit via the driver's emit stage (rename output
   `treant_v4_supports.chitubox`), reparse-verify, hand to Craig for
   Chitubox review. Iterate on his feedback.
6. Curvature (design §3/§14 phase 5) stays DEFERRED unless the routing
   residual after bracing warrants it — it's a routing lever, not a
   volume lever.

## Non-negotiable invariants (verify gates enforce; never weaken one)

clearance_violations == [] (swept envelope, final radii, incl. ball
zones) · connectivity == [] (nothing floats; embed_end rules;
no-surface-tree ⇒ honest failure) · every contact routed or honestly
unrouted, exact accounting · minima birth-region rule (proximity
doesn't count) · build-time honesty (transient spans, restraint
activation timing) · tips are the fuse (SF 8, 0.2–1.0mm) · per-segment
printability (rim-overhang: max(0,|dc_xy/dz|+dr/dz) ≤ tanθ_max(r)) ·
Murray r⁴ conservation (floors may exceed, never undershoot) ·
certificates: nothing accepted/emitted at an unchecked radius (crash,
don't warn).

## Environment

- Python via uv: `uv run --frozen pytest -q` (repo root). No new deps
  without asking Craig. Determinism everywhere (no RNG).
- STL: `../Weeping Treant v1.2.stl` (relative to repo root). Corpus:
  `../supported-treant.chitubox`. Emission: model-centered frame, lift
  3.0mm derived from corpus pads, clone-only pads, MST bridge raft —
  all encoded in the driver's emit stage already.
- `trunks.py`/`braces.py`/`struts.py` + their tests are RETIRED v3
  code kept as reference until Phase 7 cleanup; read them for the
  restraint-timing and brace-geometry rules, don't import them.
