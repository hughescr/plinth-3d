# v4 kickoff prompt — CCO-based support redesign

Paste the block below into a new Fable session in this project to begin
the v4 work. (Saved here so it's durable and versioned.)

---

You are the orchestrator and lead architect for a ground-up redesign
(v4) of the resin-print support-placement engine in this project. Work in
the orchestrator pattern: **you (Fable) do the heavy architectural
thinking, the physics/optimization reasoning, and act as the final
verification gate; delegate well-scoped implementation and analysis steps
to Sonnet subagents; and put an INDEPENDENT verifier subagent on each
non-trivial step** (a different agent than the one that did the work,
briefed adversarially) so no step is trusted on its author's say-so.
Update the task list as you go and commit at every milestone.

## The mandate

Redesign the support-generation approach from first principles around
**Constrained Constructive Optimization (CCO)** — the vascular-tree
synthesis algorithm — synthesizing the best ideas across the fields that
are all the same underlying problem: single-sink **branched optimal
transport / Gilbert–Steiner** with a concave-in-load member cost, a
max-angle gradient constraint, and a mesh obstacle. The prior session did
a three-agent research spike; READ IT FIRST:
- `skills/support-placement/references/tree-construction-literature.md`
  (full memo: formal framing, CCO, Xia junction-angle law, Murray radius
  law, Meshmixer/Clever Support/Cura/CCO/OCN prior art, cross-domain
  analogs, recommended Tier-1 vs Tier-2 paths).
- `skills/support-placement/references/design.md` (the whole v1→v3.7
  history and charter — read the mechanics, the no-intersection guarantee,
  build-time honesty, and the v3.6/v3.7 sections at the bottom).

## Craig's key design directions (do not lose these)

1. **CCO as the spine.** Contacts + per-point load are terminals. Grow /
   attach incrementally; at each junction locally optimize position, size
   radii by load conservation up the tree. Add topology mutation / a
   rewiring cleanup pass (OCN-style) — the prior greedy engine wasted
   resin (21.5ml vs a ~16–17ml target) precisely because it merged once,
   early, and never revisited topology (parallel near-duplicate columns).
2. **Supports are NOT limited to straight tubes.** Every printed layer
   can offset the cross-section, so a support can CURVE or bend — any
   printable shape is fair game. Do not assume traditional straight
   struts/columns if a curved or non-traditional geometry is more
   optimal. The layer-by-layer offset limit (how far the centroid can
   shift per layer while staying printable/self-supporting) is the real
   constraint on curvature — derive it, don't assume straightness.
3. **The governing constraints are physical, not cosmetic:**
   - **Detachability with minimal model damage** — the contact must snap
     off the model cleanly (the tip is the designed fuse; small scar).
   - **Removal leverage** — a human must be able to physically grip and
     pull the supports off. Structures that are unremovable (e.g. a cage
     the part is trapped inside, or supports with no purchase) are
     invalid however optimal on resin. Make this an explicit constraint /
     gate, not an afterthought.
   - Everything the current engine already enforces (below).
4. Adopt the two closed-form laws from the memo where they fit: **Xia's
   load-dependent junction angle** (replaces the fixed 60°) and **Murray
   `r_parent^4 = Σ r_child^4`** radius conservation (our own `r∝load^(1/4)`
   sizing restated).

## Invariants the redesign MUST preserve (these are non-negotiable and
## architecture-independent — they are how we keep an honest rebuild)

- **No-intersection guarantee**: no support member may pass through the
  model. Test-pinned as `clearance_violations == []`. The current engine
  checks the full swept envelope (segment/curve + radius + standoff)
  against a surface KDTree; a curved member must be swept the same way.
- **Connectivity**: every member endpoint must attach to another member,
  the plate, or a designed model-embedded end (the v3.7
  `verify.connectivity_report` gate). No floating members.
- **Build-time honesty**: structure ABOVE the current layer does not
  exist yet. A member is a transient cantilever until the layer that
  restrains it prints; size for the worst transient span, not the final
  restrained state. (`trunks.transient_worst_span` and the (point_z,
  active_z) restraint model encode this — carry the principle forward.)
- **Every contact is either given a CLEAR support or honestly reported
  unrouted/unsupported** — never faked, never run through the mesh.
- **Layer-island minima** (v3.6): every local z-minimum births a FEP
  island that needs a contact within ~2 layers of connection, or it curls
  off the plate. `perception.local_z_minima` +
  `verify.minima_report` — preserve this correctness rule.
- Load/peel physics: tips are the fuse (green-state tensile, SF ~8);
  members sized from accumulated carried area (peel) + slenderness
  (deflection). Constants in `config.py`; `peel_pressure_pa` and
  `shaft_drag_force_n` are still rough estimates pending real-print
  calibration — flag anything that depends heavily on them.

Keep the verify gates (clearance, connectivity, minima, load, removal-
leverage) as the honest scaffold: build the new geometry however you
like, but it passes the same or stronger gates, and you ADD a
detachability/leverage gate.

## Suggested phase plan (you refine it)

0. Read the two memos + skim the engine (`scripts/support_placement/`,
   ~4300 lines; `engine.py` orchestrates, `perception/placement/trunks/
   braces/routing/joints/verify/chitubox_emit` are the pieces). Decide
   what to reuse (perception, minima, load partition, sizing physics,
   emission, all verify gates) vs replace (the trunk/shoot/brace
   construction + the whole iterative patch loop).
1. Design doc for v4: the CCO formulation adapted to our constraints
   (terminals, insertion order, bifurcation local-optimization with the
   vertical-cone clamp, load-conserving radii, curvature model + its
   printability limit, obstacle rejection/routing, removal-leverage
   model). Independent verifier reviews the design against the memo +
   invariants before any code.
2. Prototype the core CCO builder on a simple fixture, then the toed-slab
   and table fixtures, THEN the real Treant. Gate each on the invariants.
3. Curvature: derive the per-layer offset limit; let members bend to
   improve merges/clearance; sweep curved envelopes in the clearance
   check.
4. Removal-leverage gate: formalize and implement.
5. Real-model validation on the Weeping Treant; compare resin volume vs
   the v3.7 baseline (~21.5ml member volume) and the ~16–17ml target;
   verify all gates green; emit and hand Craig the .chitubox for Chitubox
   review. Iterate on his visual feedback.

## Environment / workflow (learned; obey exactly)

- TypeScript is Craig's usual stack but THIS engine is Python; keep it
  Python. Deps via `uv`: from the repo root,
  `UV_PROJECT_ENVIRONMENT=/tmp/rpp-venv uv run --frozen pytest -q`
  (a venv is already synced there; 91 tests currently green). uv only,
  no requirements.txt, caret ranges + uv.lock.
- **Git must run in `/tmp`, not the mounted project** (FUSE can't unlink;
  git fails in-mount). The repo history lives at `/tmp/rpp/resin-print-prep`
  (a git repo). Workflow each commit: `rsync -a --delete` the project
  tree (exclude .git/.pytest_cache/__pycache__/.DS_Store) INTO
  `/tmp/rpp/resin-print-prep/`, commit there, then rsync the `.git`
  directory BACK into the project folder so the repo lives with the code.
  Deleting files in the mount needs the
  `mcp__cowork__allow_cowork_file_delete` tool first (then rm the
  `.git/index.lock` it leaves). Craig wants the git repo IN the project
  folder — NO tarballs.
- **Treant runs**: the engine on the real 576k-face Weeping Treant takes
  ~30s (run stage) + ~10s (emit). Background processes DIE with each bash
  call (each `mcp__workspace__bash` is a fresh container, 45s cap). So run
  each stage FOREGROUND in a single bash call. Use
  `scripts/tests/manual_emit_treant.py` (stages `run` then `emit`;
  centers the mesh, `SupportConfig(lift_mm=3.0)`, write_replace into the
  corpus). Override its `STATE_PATH` to a fresh dir you own under /tmp —
  stale nobody-owned /tmp files from prior sessions are undeletable under
  the sticky bit. Output: `D&D Campaigns/treant_v3_supports.chitubox`
  (rename for v4). Present it to Craig with `mcp__cowork__present_files`.
- Two mounted folders: the D&D Campaigns project, and a Box "Leyfarer"
  folder (irrelevant here).
- The STL is `D&D Campaigns/Weeping Treant v1.2.stl`; the calibration
  corpus (real 469-support human-made supports, model-centered frame,
  lift derived from its pads) is `supported-treant.chitubox`.

## Orchestration discipline Craig asked for explicitly

- Fable (you) does the heavy thinking and the FINAL gate.
- Delegate individual build/analysis steps to Sonnet subagents with
  tight, self-contained specs.
- Put an INDEPENDENT verifier on each step — a separate subagent briefed
  to adversarially check the work against the spec + invariants, reporting
  blocker/should-fix/note with concrete evidence and its own re-run of the
  numbers. Do not let a step's author also be its verifier.
- Craig's engineering culture (from his prefs): contributing-factors not
  root-cause; test-driven, tests that can actually fail; crash rather than
  silently corrupt; push back on ambiguous or suboptimal directions; ask
  clarifying questions; ground critical claims; analogy-based explanations
  welcome. He practices 改善 — the standard we walk by is the standard we
  accept.

Begin by reading the two memos and the engine, then propose the v4 design
(and your phase plan + first delegations) back to Craig before building.
