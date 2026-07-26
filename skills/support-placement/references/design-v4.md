# Support placement v4 — CCO redesign charter

Ground-up redesign of support-forest CONSTRUCTION around Constrained
Constructive Optimization (vascular-tree synthesis), replacing the v3
trunk/shoot two-level topology and its iterative patch loops. The
substrate (perception, minima, load partition, sizing physics, clearance
machinery, joints, verify gates, emission) is kept. Companion docs:
`design.md` (v0→v3.7 history, still the charter for objectives and
mechanics), `tree-construction-literature.md` (research memo this design
draws from).

Decisions locked with Craig (2026-07-21): replace v3 in place (no
parallel engine; git history is the archive); runtime target 30s–1m on
the Treant with a measured diminishing-returns cutoff, not a fixed
iteration count; removal model is grip-and-peel first, ordered
outer-to-inner cut access as the honest fallback; stays Python.

**A2 contact-position amendment (owner-ratified 2026-07-25).** Regular
coverage contacts are the exception to older generic "sampling" and
face-centroid language in this charter. Their only legal seats are one
prepared deterministic virtual lowest-vertex pool: recursive uniform 1-to-4
midpoint refinement to the coverage-derived edge bound, every co-lowest
virtual-cell vertex nominated, exact-position dedupe, and deterministic source
face ownership. Initial placement, repair, Phase-8 R2, and final verification
reuse that pool. Regular records persist the source `face_id` and exact raw
source-face normal; final verification hard-gates pool membership and
provenance. Minimum and island-birth contacts retain their specialized
`region_pts` and `birth_loop`/`birth_z` domains and never acquire a regular
`face_id`. The pool is a schema-versioned picklable struct-of-arrays (64 bytes
per retained candidate) whose canonical arrays are bytes-backed,
C-contiguous, and read-only after construction or unpickling. Deep staged
validation covers array metadata, counters, ordering, mesh/island ranges, and
raw normals. A two-pass global plan refuses before virtual
allocation when the configured aggregate cell, conservative `3*cells`
nomination/candidate, or byte budget would be exceeded; the shipped
750k/2.25m/144-MiB defaults leave about 30% headroom over the known 576k-face
depth-0 class. That 144-MiB limit covers retained compact SOA payload only, not
process peak RSS. Aggregate caps are primary: each island's one-cell-per-face
lower bound is checked before face materialization and planning runs in
fixed <=2,048-face NumPy batches with monotonic refusal. Long construction
loops separately assert no 2-GiB rise in the process peak-RSS high-water mark
since build entry (not current RSS), and nomination handling is batch-first
NumPy. Extensions enforce monotonic island ids and join prepared/new chunks
directly into final immutable bytes-backed columns. Prepared state fingerprints the
canonical mesh and semantic config; staged consumers reject pre-A2,
mismatched, or corrupted state and require prepare to be rerun.

## 0. Objective

Minimize total resin volume + scar cost + removal effort, subject to:
every contact supported or honestly unrouted; every layer-island minimum
connected within tolerance; no member through the model; nothing floats;
every member printable and build-time stable; every tree removable by a
human. The verify gates are the definition of "subject to" — the builder
may be arbitrarily clever, the gates do not bend.

## 1. Formal problem (from the memo, made operational)

Constrained single-sink Gilbert–Steiner / branched optimal transport:
terminals = contacts with peel load (Voronoi-partitioned carried area),
sink = the plate PLANE (any xy at plate_z) plus optional scar-priced
model anchors; member cost per length ∝ r²; printability = local
gradient cone; obstacle = the mesh + standoff.

Cost concavity: deflection sizing gives r ∝ load^(1/4), so cost/length
∝ load^(1/2) → the branched-transport exponent is **α = 1/2** (the memo's
"~1/4" parenthetical is the radius exponent; the COST exponent is 2·1/4).
The min-radius floor makes twig-scale cost nearly load-independent
(α_eff → 0 locally), strengthening the merge incentive exactly where the
v3 parallel-column waste lives.

Topology is NP-hard; junction positions for fixed topology are cheap.
Therefore: incremental construction + local geometric optimization +
topology mutation, same as every applied field. The Σ load^α·length
functional is the SEARCH HEURISTIC ONLY — acceptance decisions always
use real volume computed with the real sizing formulas (§6), because
real radii also depend on span length, angle, and transient state, which
the α-functional abstracts away.

## 2. Module map: keep / replace

KEEP UNCHANGED (substrate): `config.py` (extended), `mesh_io.py`,
`perception.py` (islands, local_z_minima, delicacy field),
`placement.py` (A2 deterministic contact pool, load partition, tip sizing, `_shaft_r_for_L`
angle-aware solve — extended per §6), `routing.py` (Segment/Route,
`build_surface_tree`, `check_clearance_batch`), `chitubox_emit.py`
(+ one generic chain emitter §12), `corpus.py`.

KEEP RULES, MIGRATE SIGNATURES (verifier finding — their current
signatures hard-require the four v3 collections `(routes, trunks,
risers, braces)`): `joints.py` and `verify.py`. `collect_members`
inverts: the v4 builder produces the member list natively and
`collect_members` becomes a validation/identity layer;
`connectivity_report(members, plate_z, cfg, surface_tree)` consumes it
directly. The embed-eligibility sentinels (`j == -1` model rung,
`i == j == -3` thinning rung) are replaced by an explicit per-member
field `embed_end ∈ {"p0","p1",None}` declared at construction — same
rules, no magic values. The connectivity fixture tests migrate
mechanically (same geometry, expressed as member lists) and their
assertions do not change; the surface-tree-gated model-embed rule
(no tree ⇒ embedded ends honestly flagged, never silently passed)
is preserved verbatim and `engine.py` MUST pass the surface tree in
real runs — the fixture that tests the None case stays as the guard.

REPLACE: `trunks.py` (749 lines: trunk founding/joining spiral search,
consolidate, coalesce_columns, bundle_shoots — the whole two-level
topology) and the construction/stability/gate loops in `engine.py`.
`struts.py` is dead code at runtime (engine imports it, never calls it);
its two-scar delicacy pricing is RESURRECTED as the model-anchor
candidate cost in the CCO builder — `trunks.model_strut`'s ad-hoc
distance/angle mask is retired with trunks.py. `braces.py`: policy
replaced (bracing becomes a transient-sizing outcome, §9); its
geometry/clearance-filter helpers absorbed.

NEW MODULES: `tree.py` (member/tree data model + volume accounting),
`cco.py` (insertion builder), `rewire.py` (topology mutation),
`transient.py` (build-time analysis on trees), `removal.py` (leverage
gate). `engine.py` becomes a slim orchestrator.

The canonical construction output IS the `joints.collect_members` member
schema (`{p0,p1,r0,r1,owner,kind}` chains). v3 derived it from Trunk
objects at the end; v4 produces it natively. Everything downstream
(connectivity gate, joint balls, emission) consumes it unchanged.

## 3. Member representation and the printability constraint

Members are polylines of short straight tapered frustum segments — the
representation the whole substrate already consumes. "Curvature" is not
a new geometry type; it is many short segments. The clearance sweep
(sample + radius vs surface KDTree), the connectivity gate, joint balls,
and frustum-chain emission all work today on exactly this.

Printability is LOCAL, derived from the per-layer overhang, not from a
per-member lean angle:

- Layer h cures a disk (r_up, c_up) on the previous disk (r_low, c_low).
  The unsupported rim overhang is `max(0, |Δc_xy| + r_up − r_low)` —
  the exact form (verifier finding: summing per-term maxima is a
  needless over-approximation; a shrinking radius buys back lateral
  drift budget). Continuous form, z measured upward along the build:

      max(0, |dc_xy/dz| + dr/dz) ≤ tan θ_max(r)

  Both terms matter: a member that curves AND flares upward spends its
  budget on both; the normal upward taper (dr/dz < 0) is a credit.
  v3's gentle tapers satisfied this implicitly; v4 checks it per
  segment explicitly (a taper-rate check that v3 never had).
- θ_max(r): 45° default, 60° where v3.6 already ships 60° (r ≥
  min_shoot_r, the corpus's own 227 model-adsorbed struts are the
  evidence base). A radius-dependent θ_max is plausible physics (the
  crescent is backed by a wider adhered rim on a fat member) but is a
  CALIBRATION HOOK, not an assumption — until test prints say otherwise,
  the existing angle families bound the tangent everywhere along a
  curve. Caveat (verifier): the r-keyed rule is a loosening vs v3.6's
  ROLE-keyed rule (plain shoots stayed 45° there); with angle-aware
  sizing taxing steepness, the optimizer only goes steep when it pays —
  fixture A/B measures whether 60°-tangent segments actually get built,
  and the first print calibrates whether they should.
- Joint/contact balls locally violate any tangent rule at their lower
  hemispheres; they print at our scales (corpus evidence). Balls stay
  exempt, as in v3, and the clearance sweep still covers them.

What curvature buys: merges reachable within the cone that straight
members can't make, and obstacle avoidance — descent paths that follow
the free-space field around canopy. This is the direct attack on the
74/379 unsupported minima and 176 unrouted contacts, which are
path-planning residuals, not physics failures (design.md v3.6/v3.7).

What curvature costs (mechanics, §6): eccentricity bending and worse
transient cantilevers. Curves are generated only where they pay.

## 4. The builder: forest CCO

State: a forest of trees rooted on the plate (or scar-priced model
anchors), plus the not-yet-inserted terminal set. Terminals = contacts
with stub ends (normal entry, 2.5mm) and tip radii from load (unchanged
physics).

Insertion order, two passes (verifier finding: pure z-descending
inserts minima terminals LAST — they are local minima, i.e. locally
lowest — despite their 2x routing-failure rate; correctness-critical
terminals should claim corridors first):
1. minima-birthed terminals, z-ascending (lowest first: near-plate,
   short, cheap, and they take first claim on clear descent corridors);
2. all remaining terminals, z-descending (high terminals found the
   expensive tall shared arteries early), load as tiebreak.
Order sensitivity is real but bounded by rewiring (§7); fixtures also
run the reverse ordering cheaply — if A/B shows a consistent winner it
becomes the default (decide on evidence, not taste).

Per terminal, candidate set:
(a) descent to plate: straight down from the stub end if clear, else
    the PORTED v3 spiral proposal set (`trunk_search_radii_mm` ×
    `trunk_search_directions` lateral offsets — proven, cheap, and the
    only way the FIRST terminal into an empty forest ever routes on
    canopy geometry; verifier blocker, in from Phase 2). Phase 4 adds
    curved descent following the free-space gradient on top;
(b) model anchor within `strut_max_length_mm`, priced with
    `struts.cost`'s two-scar delicacy pricing (resurrected charter #4);
(c) bifurcation into each of the k nearest existing members (k≈5,
    filtered to junction points below the stub end and cone-reachable):
    insert junction J on the member, connect terminal→J, locally
    optimize J (§5), propagate Murray radii up the ancestor path (§6).
    Junctions are strictly degree-3 in the data model; hub-like
    configurations emerge as stacked binary junctions (the joint-ball
    pass already merges coincident joints cosmetically, and chained
    Murray conservation holds through a stack).

Acceptance: cheapest candidate by REAL volume delta (including ancestor
fattening) that passes clearance AT THE NEW RADII. **Certificate rule
(the v3.2 lesson, promoted to an architectural invariant): every member
carries a clearance certificate valid for its current radius envelope;
any radius increase or geometry change voids it; nothing is accepted or
emitted with a void certificate.** Concrete mechanism (verifier: too
foundational for implementer improvisation): the certificate is
`(geometry_version, checked_radius)` stored on the member; all geometry
/radius mutation goes through `tree.py` setters which bump the version
and mark the member dirty; radius DECREASE with unchanged geometry
keeps the certificate (checked fat covers thin). Candidate evaluation
is transactional: propose → collect the dirty set (the terminal's new
members + the Murray-fattened ancestor path) → ONE batched KDTree
revalidation → commit or roll back. Emission asserts zero dirty
members; the assert is a crash, not a warning (crash over corruption).

No candidate legal → terminal parked as honestly-unrouted (Phase 4
retries with curved descent; minima-birthed terminals retry first,
coverage is correctness).

## 5. Junction geometry: Xia's law + cone clamp + numeric polish

Local model: junction J, downstream (sinkward) edge weight w0 = m0^α,
upstream children w1, w2 (m0 = m1+m2, α = 1/2). Weighted-Fermat
optimality (Σ wᵢ·ûᵢ = 0, ûᵢ from J toward each neighbor) gives, with
kᵢ = (mᵢ/m0)^(2α) = mᵢ/m0:

    cos θ12 = (1 − k1 − k2) / (2√(k1k2))   [angle between children]
    cos θ1  = (k2 − k1 − 1) / (2√k1)       [child-1 to SINKWARD edge]

Convention pinned: θ1 is measured to the sinkward (downstream) unit
vector, so the turn from the parent's upward continuation is 180°−θ1.
At α = 1/2 these collapse to falsifiable predictions the tests pin
(verifier re-derived and numerically confirmed both):

- children always meet at exactly 90° (cos θ12 = 0 identically) — note
  the load-dependence at our exponent lives entirely in the INDIVIDUAL
  turn angles, not in the between-children angle, which is fixed;
  "load-dependent junction angle" means the split of that 90° between
  the two branches;
- child i turns arccos(√(mᵢ/m0)) off the parent continuation: equal
  merge → 45°+45° (exactly our shoot family — theory meets the config);
  dominant branch runs straight, light branch joins perpendicular;
- a merge is worth attempting only if the terminals' bearing is inside
  these angles (the load-split turn angles replace v3's fixed 60° merge
  cone).

Implementation: closed form is the INITIAL GUESS; J then gets a bounded
numeric polish (few dozen real-volume evaluations, 3 variables)
minimizing real volume subject to the tangent cone (§3) at all three
incident directions and clearance. When the free optimum violates the
cone, the constrained optimum sits ON the cone boundary (the Melbourne
"pinned label" case) — the polish handles this naturally; no special
casing. Tests validate the closed form against brute-force numeric
minimization of the α-functional (oracle test), and separately validate
that polish never returns a higher real volume than the initial guess.

Murray `r_parent⁴ = Σ r_child⁴` is checked at every junction in verify
(§11) as a conservation sanity — it is our own r ∝ load^(1/4) restated,
so a violation means an upstream sizing bug, with the documented
exception that physics floors (min radius, transient spans) may push
parents ABOVE conservation, never below. Honestly-unrouted terminals
contribute nothing to any expected sum (their load reaches no tree).

Ceilings vs floors (verifier blocker — CCO's merge incentive drives
heavily-merged roots toward exactly the case v3's `max_trunk_r_mm`
clamp forbids): **floors are hard, ceilings are soft.** Physics floors
(§6) always win. `max_trunk_r_mm`-class caps become cost penalties, not
clamps — a root that genuinely needs r > cap gets it. Where the penalty
makes a fat root uneconomic, the builder may SPLIT the subtree into two
roots as a deliberate, priced topology choice (visible in the run log)
— parallel columns by decision, never by greedy accident. The Murray
gate checks conservation against actual radii, so no configuration of
caps can make it unimplementable.

## 6. Sizing: physics floors, path-integrated

Final radius of every member span between restraints:

    r = max( r_load,      # Murray/load-conservation floor up the tree
             r_deflect,   # existing angle-aware solve, worst local
                          # tangent angle along the span (placement.
                          # _shaft_r_for_L per-segment, not per-member)
             r_transient, # worst build-time cantilever, §8
             r_bend )     # NEW: eccentricity term for curved spans

r_bend: a curved span under peel tension T with max centroid deviation e
from its restraint chord carries bending σ = 4Te/(πr³) on top of axial
T/(πr²) — bending dominates once e > r/4, so ANY real curve pays a
mechanics tax. Solve σ_axial + σ_bend ≤ tensile_green/SF for r. This is
the term that makes the optimizer prefer straight members wherever
straight works, without us hard-coding straightness.

Tips unchanged: the fuse, σ from carried peel area, SF 8, clamped
0.2–1.0mm. Detachability at the model is preserved by construction.

## 7. Topology mutation: rewiring (the piece v3 lacked)

After insertion, and again after ANY structural pass (transient resize,
brace insertion, clearance repair — the v3 lesson: one-shot merge passes
get bypassed by everything downstream):

- Move set: detach a subtree at its root junction; candidate new
  parents = nearest members/plate/model-anchor (same candidate machinery
  as §4); re-optimize the junction, re-propagate radii BOTH paths (old
  parent slims, new parent fattens); accept iff real total volume
  strictly drops and every certificate re-validates.
- Also: junction slide (re-polish existing junctions after neighborhood
  changes) and prune-and-reroute for degenerate twigs.
- Termination (Craig's directive: diminishing returns, not iteration
  counts): stop when rolling volume savings over the last W accepted
  moves fall below ε of current total (defaults W=20, ε=0.25%), or a
  wall-clock cap (config, default 30s for the rewiring stage) — cap hit
  is LOGGED as such so we know the cutoff was budget, not convergence.
  The convergence curve (volume vs accepted moves) goes in the run log:
  observable, not vibes.
- Deterministic: seeded candidate ordering; no annealing in v4.0
  (recorded follow-up: annealed acceptance if fixture A/B shows the
  greedy cleanup stalls in local minima the way the memo's OCN
  literature predicts).

## 8. Build-time honesty on a tree

Generalizes v3's `transient_worst_span` from per-trunk to per-path:
during the build, structure above z_now does not exist; a restraint
(junction to a sibling subtree, brace, model rung) activates only when
its ENTIRE connecting path to something rigid has been built (v3's
(point_z, active_z) restraint pairs, computed on the tree by walking
completion heights). For each member: worst cantilever over the finite
event set of restraint-activation heights; r_transient sizes it, with
the moment arm measured along the REAL curved path (a curved cantilever's
centroid offset from the support point can exceed the straight
equivalent — integrate, don't approximate with the chord).

Peel-load direction during transients: lateral drag + the angle-aware
peel term at the local tangent, as v3.6 established. The
`peel_pressure_pa` / `shaft_drag_force_n` numbers remain flagged rough
estimates; transient sizing inherits that uncertainty and the config
notes say so.

## 9. Bracing

Bracing is no longer a standalone topology pass: where §8 demands
r_transient beyond `max_unbraced_shaft_r_mm` (the ugly-thick threshold),
the builder inserts a restraint instead — preferring (i) a rewire that
shortens the span, (ii) a cross-link to a neighbor member (brace as
graph edge; connectivity gate already handles non-tree graphs), (iii) a
short model rung (≤ `model_brace_max_len_mm`, two-end-supported, the
Chitubox-native mini-brace). Braces carry certificates like any member
and their endpoints obey the connectivity gate's designed-embed rules
unchanged. The X-pair/single-rod geometry and load-sharing bookkeeping
from v3.6 carry over where cross-links are chosen.

## 10. Removal-leverage gate (new, from Craig's removal model)

Per-tree accessibility, evaluated on final geometry:

- HANDLE: a contiguous member run with r ≥ `grip_handle_r_mm` (1.0),
  length ≥ `grip_handle_len_mm` (10), and free space ≥ `grip_clear_mm`
  (5.0) radial around its axis vs model AND other trees. Trunk-class
  members near the raft naturally qualify; the gate makes it explicit.
- PEEL: some pull direction from a small outward-biased direction set
  moves the whole tree (member point samples, tips' embedded first
  millimeter excluded — tips snap first by design) through
  `pull_dist_mm` (15) without entering the model's standoff envelope.
  Translation sweep is the conservative proxy for the real
  rotational peel; documented as such.
- ORDERED REMOVAL (Craig: cutting outer supports to expose inner ones is
  acceptable, more work, better than a failed print): iterate — remove
  every tree that currently passes HANDLE+PEEL given the remaining
  forest; repeat. Trees that only become removable after neighbors go
  are counted into a `removal_effort` metric. A residual set no removal
  order can clear (mutually caging / model-caged) is REPORTED in
  `removal_report` as a hard-failure case — diagnostic only; it does NOT
  gate verify's `ok` (see the 2026-07-22 amendment below, which
  supersedes this section's original "BLOCKER gate, wired into verify
  ok" language). The emitted run log includes the peel order. Removal
  effort is REPORTED, not priced into the §4/§7 volume objective
  (verifier finding: the doc claimed pricing no acceptance criterion
  implemented; simulating ordered removal per candidate would blow the
  runtime budget). Recorded follow-up: a cheap effort proxy in the
  objective if the Treant's effort metric comes out high — subject to
  the same "slight preference penalty, never a block" ceiling as the
  amendment below.

All four parameters are flagged rough-estimate-pending-calibration; the
calibrator is Craig's hands on the first v4 print.

**Amendment — Craig's ruling (2026-07-22), supersedes the "BLOCKER gate,
wired into verify ok" language above:**

> "unremovable support" is always not really true - they *are*
> removable, just maybe a little harder to remove. I think a slight
> preference penalty when making algorithmic choices against
> hard-to-remove supports is enough it shouldn't BLOCK an
> otherwise-superior (in terms of structural/physics support and
> reduced resin use) support layout. Removability is the least
> important axis for optimization.

`removal_report` (HANDLE/PEEL/ordered-removal simulation, including the
caged-set hard-failure case above) is diagnostic/report-only: nothing it
finds, caged sets included, blocks verify's `ok`. Removability may at
most inform algorithmic choices as a slight preference penalty — e.g. a
rewire/pricing tie-break between otherwise-comparable candidates — and
must never veto a layout that is superior on structural/physics support
quality or resin use. That tie-break is not yet implemented; it is a
recorded future item, same status as the removal-effort objective term
above. Removability is the least important optimization axis, strictly
below structural/physics quality and resin volume.

## 11. Verify gates

Unchanged and non-negotiable: `clearance_violations == []` (swept
envelope incl. dome zones, at final radii), `connectivity_report == []`
(nothing floats; adjacency rules and designed-embed endpoint kinds
unchanged), `minima_report` (birth-region contact within
`minima_contact_radius_mm`, lateral proximity does not count),
`coverage_report`, `load_report` (every contact routed / strutted /
honestly unrouted, exact accounting).

New: `removal_report` (§10; diagnostic/report-only per Craig's
2026-07-22 ruling — never gates verify's `ok`, caged sets included) and
`volume_report` — one accounting function (frustums + balls, ml,
per-kind breakdown) used for EVERY baseline/target comparison. The v3.7
Treant baseline is captured with this same function BEFORE trunks.py is
replaced (design.md quotes 23.1 cm³ member volume at the v3.7 X-pair
fix; the session-kickoff figure is ~21.5 ml — the discrepancy is
unexplained and gets resolved by measuring, not asserting; target stays
16–17 ml).

Also new: per-junction Murray conservation check (§5) and the
per-segment tangent+taper printability check (§3) — both verify-side, so
a builder bug cannot ship a violation silently.

**Amendment — Craig's ruling (2026-07-22), governs every z-direction /
print-timing tolerance in this charter (this section's gates included):**

> "The code should probably not think in '1mm is small' terms - it
> really needs to think in 'layers' which are typically going to be
> 0.05mm - 20x smaller! And my goal is to have a later pass of this
> software implement dynamic layer heights where layers might go down
> even smaller to 0.02mm when detail needs it. So islanding and other
> such features need to think about attachment in terms of much smaller
> distances."

Principle: any tolerance whose rationale is print resolution,
layer-to-layer connection timing, or a build-plate/attachment z-window
must be expressed as a multiple of `cfg.layer_height_mm`, never as an
absolute millimetre pinned to the current 0.05mm profile — "small" is
judged in layers, not millimetres. A value that reads as comfortably
sub-feature-scale at today's 0.05mm default (a plateau-tie epsilon, a
plate-band width, a birth/attachment window) silently stops being small
once the profile — or a future per-region schedule — runs finer,
without the code ever being told anything changed. `minima_connect_layers`
(a layer COUNT, multiplied by `layer_height_mm` at the point of use in
`local_z_minima`) is the pattern every such tolerance in this charter
should follow: name the quantity in layers, derive the millimetre value
once, at the point of use.

Recorded future item: dynamic layer heights (down to ~0.02mm where local
detail demands it) are a planned capability of a later pass, not yet
implemented — this charter's single `cfg.layer_height_mm` scalar is
expected eventually to become a per-z layer schedule (height as a
function of build z, not one constant for the whole print). Every
z-tolerance and attachment-window computation adopted under the
principle above must stay compatible with that future: expressed as "N
layers at whatever height is locally in force," not "N layers at the
profile's one global height," so the eventual schedule swap-in is a
lookup change at each point of use, not a re-derivation of the tolerance
itself.

## 12. Performance, termination, language

Stays Python. Evidence: v3's measured bottlenecks were scipy thread-pool
churn and algorithmic waste, not interpreter overhead — fixed by
batching (74,535 thread spawns → one query; 10.7s → 1.28s). The v4 hot
paths are the same C-backed batched kernels (KDTree queries, numpy
geometry). CCO adds bookkeeping-light Python around them; the
diminishing-returns cutoff (§7) bounds total work; wall-time pins on
fixtures stay in the test suite (test_engine's <10s fixture pin
survives). A rewrite elsewhere would forfeit the working substrate for
speed we don't yet lack — reconsider only if profiling on the real
Treant shows interpreter-bound hot loops that vectorization can't fix
(Craig's standing rule: batch, don't spawn; profile before blaming the
interpreter).

Runtime budget: 30–60s Treant construction+verify. Stage budgets logged
per phase; the run log reports where time went and whether any stage hit
its cap.

Emission: one new generic `synth_member_chain` (polyline of frustums +
balls per owner record) alongside the existing synthesizers; record
vocabulary (T_TIP/T_TAPER/T_SHAFT/T_JOINER), clone-only pad doctrine,
raft MST bridging, and the model-centered coordinate frame all
unchanged. Curved members emit as their polyline — no new record types.

## 13. Test & migration plan (replace in place)

- Substrate tests unchanged and stay green throughout: perception,
  placement, routing, joints, verify, chitubox_emit, corpus, mesh_io.
- `test_trunks.py` retires WITH trunks.py, replaced by `test_cco.py` /
  `test_rewire.py` / `test_transient.py` / `test_removal.py`:
  Xia-vs-numeric oracle; 90° children prediction; cone clamp; Murray
  conservation; certificate rule (a resize without recheck must be
  IMPOSSIBLE by construction — test tries to force it); rewiring
  monotonicity (volume never increases across an accepted move);
  insertion accounting; transient restraint-activation timing (v3's
  rule, ported); removal gate on a hand-built caged fixture.
- `test_engine.py`'s INVARIANT tests survive with the same assertions
  (clearance==[], connectivity==[], accounting exactness, per-segment
  angle budgets, minima on box/toed-slab, wall-time pin); its
  v3-mechanism tests (trunk counts, spiral search behaviors) retire.
- TDD discipline: gates wired to the new builder from the first commit;
  every new invariant gets a test that FAILS before its implementation.
- Step zero of implementation, before any deletion: run current v3.7 on
  the Treant, freeze `treant_v3_7_baseline.json` (volume_report + counts)
  into the repo (it now lives at
  `scripts/tests/fixtures/treant_v3_7_baseline.json`).

## 14. Phase plan

1. This document; adversarial verifier review; Craig sign-off. Commit.
2. `tree.py` + `cco.py` core insertion INCLUDING bifurcation with the
   Xia closed-form + cone clamp (verifier: without merging, Phase 2
   can't demonstrate CCO's core value and would regress vs v3 at its
   own milestone) and the ported spiral plate-anchor search; straight
   segments only; joints/verify signature migration; all gates wired;
   `scripts/tests/fixtures/treant_v3_7_baseline.json` frozen BEFORE any
   deletion. Commit.
3. `rewire.py` + junction numeric polish + Murray verify gate; fixture
   volume A/B vs frozen v3 numbers. Commit.
4. `transient.py` full generalization + bracing-as-restraint (§9).
   REORDERED ahead of curvature on Treant smoke-test evidence
   (2026-07-21): unbraced ~100mm canopy columns deflection-size to
   r≈2.9mm (~2.7ml each; 200-contact partial build measured 74ml vs
   v3.7's 22.85 full) — bracing is a ~14x per-column volume lever,
   curvature is mostly a routing lever (v4 unrouted rate already ≈ v3's
   without it). Includes: braced-cost candidate evaluation in cco.build
   (span capped at brace_free_span_mm + brace volume estimate) so
   insertion doesn't misprice, then the real §8 transient loop, then a
   rewire re-pass against braced costs. Commit.
5. Curvature: curved descent + curved merge candidates, per-segment
   printability check, r_bend, curved-path transient arms. Attack the
   unrouted/minima residual. Commit.
6. `removal.py` + gates wired into verify ok. Commit.
7. Treant: full run, volume vs baseline & 16–17ml target, all gates
   green, emit, Craig's Chitubox review. Iterate.

Each implementation phase: Sonnet subagent builds to a written spec;
an INDEPENDENT adversarial Sonnet verifies (re-runs numbers, hunts
counterexamples, reports blocker/should-fix/note); Fable gates. Commit
at every phase boundary (rsync → /tmp git → rsync .git back).

## 15. Risks and open questions (recorded, not hidden)

- Insertion-order sensitivity: bounded by rewiring, measured on
  fixtures; if variance across orders stays >5% volume, revisit
  (annealing follow-up in §7).
- Curved-candidate search space: the free-space-gradient descent is a
  heuristic, not a path planner; canopy cases that defeat it stay
  honestly unrouted, same reporting as v3. Full path planning remains a
  recorded follow-up if residuals matter after Phase 4.
- θ_max(r) and all removal parameters await print calibration.
- `peel_pressure_pa` / `shaft_drag_force_n` remain rough (inherited
  flag); every sizing that depends on them inherits the uncertainty.
- Baseline volume discrepancy (23.1 cm³ vs ~21.5 ml) unresolved until
  measured with the unified volume_report.
- Removal PEEL check is translational; real removal has a rotational
  hinge component. Conservative direction, but the report can undercall
  a tree's true removability in principle — since `removal_report` is
  diagnostic-only (2026-07-22 amendment, §10), this doesn't block
  anything, but an over-conservative PEEL check still means a noisier
  diagnostic and a noisier future preference-penalty signal. If PEEL
  over-flags on the Treant, the fix is a hinge-sweep refinement to the
  diagnostic's accuracy, not a gating change.
- Root-split-vs-fat-root pricing (§6 ceilings) is a new topology
  degree of freedom; watch fixture A/B for the penalty constant
  reintroducing twin-column waste through the back door.
- Removal-effort term in the volume objective: deferred (§10),
  revisit on Treant effort numbers.
