# SPEC-P8-RELOCATE — Phase-8 placement rebuild under SCAR-AREA-CAP + PRINT-FLOOR

Status: **DRAFT — pending challenge + owner review.**
Baseline: branch `v5-perf` @ ada5f56 (engine verify gate runs SCAR-AREA-CAP +
rule 4, aligned with census). Governing law: SPEC-EMBED-POLICY (owner-ratified,
cd989fe) **plus** the owner-ratified PRINT-FLOOR amendment (2026-07-24, §2).
Validates/extends SPEC-P8-REACH — §1 states exactly what of REACH survives.

Evidence base (archived, digest-identical across 5 runs, 09ce1fd6a9a1…):

- forest: `scratchpad/v5e_run1.pkl` (paths below abbreviate the session
  scratchpad `/private/tmp/claude-501/…/0ddf6d41…/scratchpad/`)
- census record: `scratchpad/audit/penetration_census.json`
  (1476 members, 193 illegal, 176 stub / 17 strut, 15 true ball breaches,
  142 of 193 in the crown)
- member lists: `scratchpad/census_illegal_mids_v5a.json` (193),
  `scratchpad/engine_clearance_mids_v5a.json` (190); symdiff {445, 474, 575}
  is the pre-blessed tip-anchor oracle boundary (§5.3 retires it)

---

## 1. Relationship to SPEC-P8-REACH — what holds, what is superseded

SPEC-P8-REACH solved a different defect population (unrouted contacts: the
audit's 195-unrouted problem) but built machinery this spec deliberately
reuses. **HOLDS, reused as substrate:**

- the relocation seam: intervene inside `cco.build` between "candidate loop
  exhausted" and "park unrouted" (REACH §0; cco.py:159-240 in this worktree);
- the transactional trial mechanics: checkpoint → build → `forest.revalidate`
  → rollback (cco.py `_insert_terminal`, cco.py:309);
- the contact-class legal-domain table (REACH §1: island-birth footprint /
  minimum region / island faces — inviolable, order-sensitive class test with
  `island_birth` before `minimum`);
- the neighbor-birth-coverage commit guard (REACH Open-Risk #8);
- the determinism doctrine: no RNG, stable sorts on explicit keys;
- Feature B (concession supports) as an ORTHOGONAL tool: a concession spends
  clearance standoff; it can never sanction a rule-4 transit (transit is
  crossing topology, not distance). Concessions stay available for their own
  population and are out of scope here.

**SUPERSEDED by new evidence:**

- REACH's implicit placement law — clearance + arc-inset embed window — is
  retired. The verify gate at ada5f56 judges by SCAR-AREA-CAP + rule 4
  (verify.py:299-395); PRINT-FLOOR (§2) amends rule 4. The builder must PLACE
  by that same law (§5). REACH's citations of `_embed_inset_arc` as a
  sanction are void; the arc-inset survives only as a distance-gate mask
  (verify.py:68, tree.py:391 — see §5.1).
- REACH's primary target population (unrouted demanded contacts) is now the
  *third* class. The measured defect population on the archived geometry is:
  **143 non-exempt transit members** (§3 class T — 126 contact stubs + 17
  interior struts, see §3.1/§3.1s), **15 true ball breaches** (§3 class B),
  and the **quarantine pool re-graded under SCAR-AREA-CAP, 77 REAL** (§3
  class Q).
- REACH's line-number anchors were against `wt-emit-final`; this spec anchors
  against v5-perf @ ada5f56. Where they disagree, re-anchor by symbol.

---

## 2. PRINT-FLOOR amendment (owner-ratified 2026-07-24) — normative

An unanchored (rule-4) transit crossing is downgraded from ILLEGAL to **MOOT**
iff its measured antipodal crossing chord — the existing deterministic
`feature_thickness_mm`, entry-to-exit along the offending ray (census field;
engine computes the identical quantity from its kernel intervals) — is
**< 0.08 mm**.

- **Member-level rule:** a member is exempt iff **ALL** of its rule-4
  crossings are moot. One firm crossing condemns the member regardless of how
  many moot crossings it also has.
- **Accounting:** moot transits are still counted and reported in a separate
  auditable bucket `moot_transit_members` (member indices, per-crossing chords)
  in BOTH the engine gate report and the census, so parity stays bit-testable.
  The hard gate fails ONLY on the non-exempt population.
- **Rationale (owner's):** <0.08 mm chords are 2.6-4.1 px foliage knife-edges
  that cannot print as coherent material; the chord ≥ true wall thickness, so
  the exemption is conservative; 0.08 mm is a 20 µm guard band above the
  0.0502 mm population minimum. `MIN_ILLEGAL_LEN_MM = 0.05` (verify.py:37)
  remains as the per-run noise floor beneath it — PRINT-FLOOR is a *chord*
  test on the crossing, not a run-length test, and both apply.
- **Constant:** `PRINT_FLOOR_CHORD_MM = 0.08`, literal in both
  implementations, listed with the SPEC-EMBED-POLICY normative constants.
  Ratification lever alongside A_ABS_MM2 / KAPPA / L_SCAR_MAX_MM.

**Effect on the archived geometry (adjudicated, owner-reviewed):** of the 193
census members, 50 have all crossings < 0.08 mm → moot-exempt. Partition:
118 firm (some crossing ≥ 0.10 mm) + 25 band (worst crossing in
0.08-0.10 mm, non-exempt) + 50 all-moot. **Rebuild target = 143 non-exempt
members.** 104 of the 118 firm are contact stubs. (mi=420 is all-moot at full
IEEE-754 precision — its worst crossing rounds up to 0.08 mm only under
reduced precision; it sits in the moot 50, moving the band 26→25 and moot
49→50 vs the pre-correction snapshot; see phase8_relocation_input.json
CORRECTION 2026-07-24.)

---

## 3. Defect classes and per-class remediation

### 3.0 Frozen defect snapshot, stable site IDs, and phase order (normative)

**Frozen snapshot.** The remediation input is the frozen defect snapshot
`phase8_relocation_input.json`, pinned to `state_pkl = v5e_run1.pkl`,
`state_md5 = aab04cf3d0be6fdb6454ce01c3274dfc` (digest-identical across the
5 archived runs, 09ce1fd6a9a1…). The snapshot is computed ONCE, before any
relocation, and is immutable for the duration of the pass — no class reads
live geometry to decide membership; every member is addressed by its
**stable site ID = the member index `mi` in that pickle** (immutable for the
run; the same integer used by census and gate). The frozen partition is
**143 non-exempt transit members** (class T), **15 true ball breaches**
(class B), **77 REAL quarantine sites** (class Q); the moot 50 are recorded
but never remediated.

**Explicit phase processing order (deterministic, normative):**

- **Phase 0 — predicate move (§5.1).** The A0b quarantine pre-pass adopts the
  ratified law (`revalidate` places by SCAR-AREA-CAP + rule-4 + PRINT-FLOOR).
  Q-legal members (deep plugs, §3.3) are thereby *never quarantined* — zero
  per-site work; the fix is the predicate itself.
- **Phase 1 — stubs (Q-REAL ∪ class-T stubs), SM-STUB (§3.1).** The 77 REAL
  quarantine sites and the class-T contact stubs are one population, one
  ladder, two entry points (§3.3). Ordered by frozen (worst-firm-chord desc,
  mi asc); pocket members handled by the batched pocket solver (§4), isolated
  members by the plain per-site ladder.
- **Phase 2 — interior struts, SM-STRUT (§3.1s).** The 17 interior struts —
  a DIFFERENT state machine from stub re-aim (amendment D); struts are never
  routed through the stub `mark_quarantined` contract.
- **Phase 3 — ball breaches, SM-BALL (§3.2).** LAST, because a joint ball's
  legal position depends on the FINAL segment geometry produced by phases 1-2
  (re-aimed shafts move the segments the ball splits). Running balls earlier
  would re-open on any subsequent shaft move.

The order is normative: shaft geometry (stubs, then struts) is finalized
before joint geometry (balls). Within every phase the commit order is a
stable sort on the explicit key above with `mi` as the final tiebreak.

**AMENDMENT — SINGLE FINAL-ONLY RELOCATION STAGE** (implementation
consequence, orchestrator-approved 2026-07-24; not a change of policy, a
correction of where the ratified phase order is applied).

Relocation is ONE stage, run ONCE per engine run, AFTER every fix pass'
`construct` / `rewire` / `brace_and_size` has settled and IMMEDIATELY BEFORE
the final verify. **Nothing may mutate geometry after relocation except the
verify that judges it.**

Rationale — the same argument §3.0 already makes for Phase 3 ("a joint ball's
legal position depends on the FINAL segment geometry produced by phases 1-2;
running balls earlier would re-open on any subsequent shaft move"), applied
one level up. The engine previously ran the ladder *inside* the fix-pass loop,
once per pass; pass 1's `rewire` + `brace_and_size` then ran AFTER pass 0's
relocation and mutated geometry relocation had already certified. Measured
consequence on the 2026-07-24 acceptance run: 3 embed-grade regressions (the
mi 920/991/1010 class) that were pure `brace_and_size` drift on endpoints
pass-0 relocation had settled — remediation being undone by a later
construction stage. Secondary consequence: two full ladders per run, the
second re-deriving from an already-relocated forest, for roughly double the
wall cost and no additional resolution.

Ledger contract is unchanged: the stage still writes `relocate` and
`relocate_p{N}` (a single final-only stage writes `relocate_p0`), and the
verify it precedes is logged as `verify[final]` / `verify_final`.

**ZERO-NEW-VIOLATION COMMIT INVARIANT (D6)** (implementation consequence,
orchestrator-approved 2026-07-24). A candidate whose application would create
ANY new violation — firm transit, embed grade (SCAR-AREA-CAP), connectivity,
printability, or ball breach — ANYWHERE in its influence set (spliced ∪
co-moved ∪ endpoint-neighbours) is ILLEGAL AT COMMIT; and a commit that does
not STRICTLY DECREASE the defect count over that influence set is likewise
illegal (it is churn, not remediation). Every class is read from the ONE
authority §5 already names — `verify.certify_member` for transit + embed
grade, `connectivity_report` / `printability_report` / `ball_breach_report`
for the rest — evaluated BATCH-FIRST, one call per class per side over the
whole influence set.

Consequence, and why this is what makes §6.10 real: every surviving commit
strictly decreases the GLOBAL defect count (geometry outside the influence set
is untouched, and each class is a function of that geometry alone), so the
sweep loop is monotone in a non-negative integer and reaches its fixed point
by construction rather than by the sweep cap. A splice that is legal but
STERILE retires its site as honest-unresolved (`D6-sterile`) instead of being
re-picked next sweep — that re-pick was the neighbour cascade that held the
sweep tail at 1-2 commits/sweep forever.

**Final full-verify + idempotence (acceptance, §6.9-§6.10).** After Phase 3,
the complete gate runs over the *serialized* output (§6.9). A second Phase-8
pass over that output re-derives an EMPTY frozen snapshot (zero non-exempt,
zero breaches), enters every ladder zero times, and emits byte-identical
geometry — the idempotence gate (§6.10).

### 3.1 Class T stubs — 126 non-exempt contact stubs (SM-STUB, the rebuild core)

Contact stubs whose shaft, after (or instead of) its legal tip-anchored
interval, transits adjacent foliage (census: 176/193 stubs,
`through_and_through`, transit chords overwhelmingly < 1 mm; of the 143
non-exempt, 126 are contact stubs). Interior struts are a SEPARATE state
machine (§3.1s) — the same defect (an embed end whose shaft transits) but a
different member kind, a different legal domain, and a different disposal
contract (amendment D: interior struts are NEVER routed through the stub
`mark_quarantined` path).

**SM-STUB is a three-rung ladder R1→R2→R3.** The ladder is ordered: a rung is
entered only when the rung above it yields ZERO legal candidates for the
site; the ladder never re-orders (R1 preserves the contact exactly, R2
preserves the demand but moves it, R3 abandons it — the resin- and
coverage-conservative order). **Within a rung the selection is GLOBAL
lexicographic minimization over the rung's FULL legal candidate set — NOT
first-legal-wins** (amendment B strikes the draft's "first success wins"): the
rung enumerates every candidate, computes legality for all of them in one
batched call (§7), keeps the legal set, and picks the unique lexicographic
minimum of the score tuple below. "First success wins" is retired; the only
"first" that survives is the ladder rung order, never candidate order.

**R1 — tip re-aim (resolves most stubs; 104/118 firm stubs are contacts).**

*Contact-preservation definition (amendment E, normative):* "preserve the
contact" means the demanded contact **surface point** `c` (the contact
`position`, a fixed point on the model surface) is invariant across every R1
candidate. It does NOT mean a fixed `p0` — `p0` (the embedded tip / ball
center) MOVES on a sphere of radius `δ` about `c` as the direction changes.

*Pinned re-aim formulas (both implementations, identical bits):* let
`n̂` be the candidate unit direction, `δ = contact_depth_mm` (unchanged
designed embed depth), `L = stub_length_mm × f`, `f ∈ {1.0, 0.75, 0.5}`,
`R_b = BALL_FACTOR·r_end` (unchanged; `BALL_FACTOR = 1.5` for stub tips):

    p0        = c − n̂·δ            # embedded tip / designed ball center
    p1        = c + n̂·L            # free stub end
    ball_ctr  = p0                  # embed_end = "p0", radius R_b unchanged
    u         = normalize(p0 − p1) = −n̂   # axis unit, other-end → p0

The SCAR-AREA-CAP inputs are RE-DERIVED per candidate, never carried over:
`T` = tip-anchored interior interval length on the axis ray from `p0` along
`u` (FINDING-2 robust); exit crossing `x_c = p0 + u·T`; **scar-normal `n` =
the outward unit normal of the crossing triangle at `x_c`** (kernel exact
projection; edge/vertex tie-break = lowest triangle index) — `n` is NOT `n̂`;
`cosang = |u·n|`; `r_x = r(T)`; `d` = exact signed distance from `p0` to the
surface. Re-aim changes `cosang, T, r_x, d`, so the verdict MUST be recomputed
(§5 law), never inherited from the rigid build.

*Candidate set (direction × length):*
- directions: a fixed cone grid around the contact inward normal
  (`n̂` at polar 0 recovers the original contact normal) — polar angles
  `(0°, 10°, 20°, 30°)` × azimuths `(0..315° step 45°)`, nested polar-major /
  azimuth-minor; azimuth 0 anchored to the stable local frame
  `e = normalize(cross(n̂₀, ẑ))`, with the `|n̂₀·ẑ|>0.999` degenerate case
  falling back to `x̂`; polar 0 emitted once → **25 directions**. Candidates
  outside the printability lean limit (config theta family) are **skipped,
  not clamped**;
- lengths: `(1.0, 0.75, 0.5) × stub_length_mm` — shorter stubs escape
  overhead foliage; the 0.5 floor keeps enough shaft for the router to grab.
- ≤ 75 candidates per stub. Candidate index (length-major, then the direction
  order above) is the score tuple's final tiebreak — enumeration order is a
  tiebreak only, never the selector (amendment B).

*Legality* (kernel-evaluated exactly as the gate, axis ray for stubs): a
candidate is legal iff **(a)** its tip SCAR-AREA-CAP verdict is LEGAL on the
re-derived inputs, **(b)** it has zero non-exempt rule-4 crossings under
PRINT-FLOOR, **(c)** `forest.revalidate` under the §5 law passes (no collision
with other members/model), **(d)** its downstream routing re-trials
successfully — the stub end moved, so the terminal re-routes through the
ordinary `_candidate_builders` from the new `stub_end`; routing failure fails
the candidate (rollback), it never strands the contact, and **(e)** the
resin/quality guards (amendment H) below pass.

*Resin / quality guards (amendment H, per candidate, part of legality):*
- **volume-delta cap:** reject the candidate if
  `Δvol = vol(candidate shaft + ball + re-trialled downstream route) −
  vol(original member) > p8_reaim_volume_delta_max_ml`. Bounds per-relocation
  resin cost so no single re-aim balloons the route.
- **no unbounded growth:** stub length factors are ≤ 1.0 so the stub itself
  cannot grow; the guard binds the re-trialled downstream segment — its added
  length is capped at `p8_reaim_route_growth_max_factor ×` the original
  downstream route length. A candidate whose re-route exceeds it is illegal.

*Score* — GLOBAL lexicographic minimum over the full legal set (minimize):

    (n_moot_crossings, density_penalty, polar_angle_deg,
     length_reduction_step, route_volume_ml, candidate_index)

- `n_moot_crossings` first: never trade a clean candidate for a moot-riding
  one.
- `density_penalty` second (amendment B — the exact tuple position pocket
  density enters): the integer count of OTHER pocket members' corridors
  (capsule at current radius + standoff) the candidate shaft passes within
  `p8_pocket_corridor_pad_mm` of, computed against the pocket's own member set
  (§4.2). **It is 0 for isolated (non-pocket) members**, so the tuple degrades
  exactly to the sparse-site ordering and sparse sites pay nothing. Ranked
  above `polar_angle_deg` because avoiding a dive into a neighbor's shared
  foliage (which could create a NEW transit) outweighs minimizing lean.
- then least deviation (`polar_angle_deg`), least shortening
  (`length_reduction_step`), cheapest route (`route_volume_ml`), and finally
  `candidate_index` — a total order, so the minimum is unique and
  deterministic.

**R2 — contact re-seat (relocation to an alternative anchor candidate).**
Only if the R1 legal set is empty. Move the contact within its inviolable
class domain (REACH §1 table, unchanged), candidates ordered
nearest-to-demanded with the `reloc_max_candidates` cap (REACH §2b); the
per-candidate legality test is the full §5 law (tip verdict + rule-4 +
PRINT-FLOOR + clearance + H guards), not clearance alone. Old stub `mark_dead`
(tree.py:348-family), new stub + routing per REACH §2b, neighbor-birth guard
before commit. Selection is again GLOBAL lexicographic minimum over the legal
re-seat set: `(route_length_mm, reseat_distance_from_demanded_mm,
candidate_index)` — REACH's pinned route objective first, least demand
displacement next, index last.

**R3 — quarantine + re-route (honest failure).** Only if R1 and R2 both yield
empty legal sets. The stub is `mark_quarantined` (tree.py:348) — this is the
CORRECT contract for a contact stub — and the contact goes to the
honest-failure path, reported unrouted with its carried area, exactly the A0b
discipline (cco.py:196-220). No fabricated support. The §6.7 bound caps how
many sites may end here.

### 3.1s Class T interior struts — 17 non-exempt struts (SM-STRUT)

The 17 interior struts (census strut subclass; capsule-edge grazes like
mi=1138) are the same *defect shape* — an embed end (a strut/rung foot) whose
shaft transits — but a **distinct member kind with its own state machine**
(amendment D). They are NOT contact stubs: their embed end is a foot ball
(`BALL_FACTOR = 1.4`, `MODEL_FOOT_FACTOR`), their legal domain is the
strut/rung foot domain (REACH §1, the foot/face table — NOT the contact-stub
anchor domain), and — critically — **the stub `mark_quarantined` contract does
not cover them.** SM-STRUT is a three-rung ladder S1→S2→S3, same global
lexicographic-min selection discipline as SM-STUB:

- **S1 — foot re-aim.** Re-aim the strut foot within its lean cone (feet use
  `BALL_FACTOR = 1.4`) by the §3.1 re-aim formulas applied at the foot: `c`
  = the fixed foot anchor point, `p0 = c − n̂·δ_foot`, `p1 = c + n̂·L_foot`,
  scar-normal re-derived per candidate. Legality is the §5 law on **both
  adjoining segments** the foot joins (a strut is interior — moving its foot
  re-splits two segments; both must pass rule-4/PRINT-FLOOR/SCAR on
  revalidate). The H growth guard binds foot-length growth to
  `p8_strut_growth_max_factor ×` the original strut length — no unbounded
  strut growth.
- **S2 — strut re-route.** If S1's legal set is empty, re-trial the strut's
  routing via `_candidate_builders` from an alternative foot inside the
  **strut/rung foot domain**, global-lex-min scored as S1. Old foot
  `mark_dead` (tree.py:348-family), never `mark_quarantined`.
- **S3 — strut retirement (honest structural failure).** If S1 and S2 both
  fail, the strut is `mark_dead` and its connectivity demand is reported as an
  **unresolved STRUT failure** (counts against §6.7 with `class="STRUT"`),
  NEVER `mark_quarantined` — that contract quarantines a stub and sends a
  *contact* to honest failure, which a structural strut has not got.
  **Raft-connectivity guard (amendment G, §6.9):** dropping a strut must not
  orphan a subtree from the raft. If S3 would orphan any downstream geometry,
  S3 is DISALLOWED; the strut escalates to the owner as a
  criticality-weighted unresolved (§6.7). Structural connectivity is never
  silently severed.

### 3.2 Class B — 15 true ball breaches (SM-BALL, run LAST — §3.0 Phase 3)

The 15 `source="ball"` joint balls (R3 ball-gate scope; endcap spheres stay
report-only) whose sphere spans a feature thinner than its radius
(`geom.sphere_breach`, verify.py:395-444; census `breaching_balls`, e.g.
ball 53: r 0.35, feature 0.2814, poke-out 0.0686). SM-BALL is a THIRD, distinct
state machine (amendment D), run after SM-STUB/SM-STRUT have finalized all
shaft geometry (§3.0 Phase 3). Remedy is **relocation of the joint, never
shrinking the ball** (ball radii are designed: BALL_FACTOR × r_end / joint
sizing — a smaller ball is a different sanction and is not on offer):

- **B1 — axial slide.** A 1-D deterministic minimal-slide search: move the
  joint point along its member's axis away from the thin feature in steps of
  `p8_ball_slide_step_mm = 0.25` up to `p8_ball_slide_max_mm = 2.0`, ordered
  root-ward then tip-ward, and take the **minimal-slide** clean position — the
  first in that order where `sphere_breach` is clean AND both adjoining member
  segments stay legal under the §5 law (revalidate + rule-4/PRINT-FLOOR on the
  re-split segments). (This is a monotone 1-D objective, not the R1
  multi-candidate cone, so first-clean-in-order IS the lexicographic minimum
  of `(slide_distance, direction_rank)`.)
- **B2 — local re-route.** If no slide position is clean, the owning
  terminal's route is re-trialled via `_candidate_builders` with the breaching
  joint's region voxel (§4) recorded as crossing-penalized; GLOBAL
  lexicographic-min scored exactly as SM-STUB R1 (density_penalty applies when
  the ball sits inside a pocket, else 0).
- **B3 — honest failure.** If B1 and B2 both fail, the joint's route is
  reported unresolved (`class="BALL"`, §6.7), same honest-failure discipline
  as SM-STUB R3 — the ball is never shrunk and the breach is never suppressed.
  Raft-connectivity guard (§6.9) applies exactly as SM-STRUT S3.

### 3.3 Class Q — quarantine pool re-grade (77 REAL)

The A0b quarantine pre-pass condemns any stub the *old* revalidate flags —
including buried-deep-plug stubs that SCAR-AREA-CAP holds LEGAL (no depth cap
on the tip-anchored interval; SPEC-EMBED-POLICY "There is NO depth cap").
Re-grading the pool under the ratified law splits it:

- **Q-legal (deep plugs and other false condemnations):** once the builder's
  placement predicate is the gate's law (§5), these stubs are simply never
  quarantined — the fix is the predicate move itself, no per-site work. Their
  contacts route normally.
- **Q-REAL (77):** genuinely illegal placements under the new law. These
  enter the 3.1 ladder as first-class citizens: R1 tip re-aim at the demanded
  position, R2 re-seat, R3 honest failure. They are processed in the same
  pass as class T (they are the same defect — a stub that transits — caught
  pre-insertion instead of post).

The implementation MUST NOT special-case Q vs T beyond bookkeeping: one
predicate, one ladder, two entry points (pre-pass vs census).

---

## 4. Pocket strategy — atomic-commit pocket solver with full dependency invalidation

Adjudicated clustering: canopy pocket P1 x∈[6,12], y∈[-12,-6], z∈[48,54]
(~37 members incl. adjacent cell), canopy pocket P2 x∈[18,36], y∈[6,18],
z∈[48,60] (~22), lower-trunk pocket P3 x∈[-36,-30], y∈[12,18], z∈[-24,-18]
(17). Half the defect mass; 71% of all 143 sit above CROWN_Z=35 in dense
foliage where **per-site greedy re-aiming thrashes**: a re-aimed shaft can
create a NEW crossing through a neighbor's corridor, and a naive sequential
pass can oscillate.

The draft's per-site scorer against a stale snapshot, its "two sweeps" cutoff,
and its candidate-ray-only invalidation are **REJECTED** (amendment F). The
mechanism below is an **atomic per-member commit loop with a monotone
resolved-count objective and full dependency-set invalidation**, driven by a
precomputed **conflict graph**. It terminates in ≤ |pocket| commits, never
oscillates, and is bit-deterministic. Conflict-graph *coloring* was the
considered alternative; atomic-commit is specified because it removes the
sweep cap entirely and its termination proof is a one-line monotonicity
argument (§4.5).

### 4.1 Pocket detection — pinned origin, detection halo (derived, not hardcoded)

- Bucket the frozen (§3.0) non-exempt member tip positions into 6 mm voxels
  (`p8_pocket_voxel_mm = 6.0`) on a grid **pinned to the world origin
  (0,0,0)** — NOT to the member-cloud bbox (amendment F: a bbox-relative grid
  shifts under any geometry move and would make pocket identity unstable). A
  voxel key is `(floor(x/6), floor(y/6), floor(z/6))`.
- A pocket = a 6-connected voxel component containing ≥
  `p8_pocket_min_members = 8` defect tips. Voxel keys sorted lexicographically;
  components labeled in that order (deterministic).
- **Detection halo:** membership is tested with a 1-voxel dilation — a member
  whose tip voxel OR any voxel within the halo belongs to a pocket component
  is a member of that pocket. This prevents a tip sitting on a voxel face from
  flipping pocket membership under a sub-voxel re-aim, and it makes detection
  robust to the small tip moves R1 induces.
- The three clusters above are the EXPECTED output on the archived geometry
  and become a regression fixture; the algorithm reads none of them as input.

### 4.2 Data structures

Per pocket:

- `members[]`: for each pocket member `m` — `mid` (stable §3.0 site ID); the
  fixed contact/foot surface point `c_m`; the SM-STUB/SM-STRUT candidate set
  `C_m` (≤ 75, the §3.1 cone × length grid); the **reachable envelope** `E_m`
  = the AABB (equivalently, swept-cone hull) enclosing the capsule+standoff of
  EVERY candidate shaft in `C_m`, padded by the detection halo
  `p8_pocket_corridor_pad_mm = 0.5`; `status ∈ {UNRESOLVED, COMMITTED,
  FALLEN}`; `committed_cand` (None until COMMITTED); `legal_sorted` (the
  member's current legal candidate list, lexicographically sorted by the §3.1
  score tuple; empty ⇒ no legal candidate right now).
- **Conflict graph `G`:** one node per pocket member; an undirected edge
  `(a,b)` iff `E_a ∩ E_b ≠ ∅` (reachable envelopes overlap within halo — i.e.
  SOME candidate of `a` could geometrically interact with SOME candidate of
  `b`). Built once by a batched AABB broad-phase over the envelopes, then the
  exact overlap test; edges stored as sorted `(min mid, max mid)` pairs.
  `G`'s neighbourhood `N(m)` is the COMPLETE set of members whose legality
  could change when `m`'s geometry is fixed — this is the "full dependency
  set" the invalidation rule uses (§4.4), replacing the rejected
  candidate-ray-only rule.
- `density_penalty(cand, m)` (feeds the §3.1 score tuple): the integer count
  of OTHER pocket members whose current corridor (committed geometry if
  COMMITTED, else original placement) the candidate shaft of `m` passes within
  `p8_pocket_corridor_pad_mm` of. Computed against the pocket's own member set.

### 4.3 The atomic-commit loop (normative)

1. **Seed (one batched call).** Evaluate the FULL candidate matrix — every
   pocket member × its ≤75 candidates — against the frozen base geometry plus
   the already-committed non-pocket forest, in ONE batched kernel interval
   query (gather → one call → scatter; a per-candidate scalar loop here is a
   defect per the batch-first rule). Populate each member's `legal_sorted`.
   Members with empty `legal_sorted` leave the pocket loop immediately to
   SM-STUB R2/R3 (resp. SM-STRUT S2/S3).
2. **Ready set + priority.** `Ready = { UNRESOLVED members with non-empty
   legal_sorted }`. Commit priority key = **(worst-firm-chord desc, mid asc)**
   — worst-first (the frozen chord, stable across the loop).
3. **Commit one, atomically.** Pop the highest-priority `m` from `Ready`. Its
   chosen candidate is the lexicographic-min of `legal_sorted` **that also
   satisfies the mutual-legality guard**: the candidate must (i) be legal
   against all currently-committed geometry (§5 law) AND (ii) introduce **no
   new firm transit** into any already-COMMITTED neighbor in `N(m)` (check the
   committed neighbours' intervals against `m`'s new shaft — a bounded,
   batched check over the COMMITTED subset of `N(m)`). Walk `legal_sorted` in
   order to the first candidate passing (i)+(ii); if none passes, `m` leaves
   to R2/R3. Otherwise set `committed_cand`, `status = COMMITTED`. **A
   COMMITTED member is terminal — never re-evaluated, never moved.**
4. **Full dependency-set invalidation.** Mark every `k ∈ N(m)` with
   `status = UNRESOLVED` **dirty** — the complete graph neighbourhood, not
   merely members whose current best ray passes near `m` (amendment F).
5. **Re-evaluate dirty (one batched call).** Recompute `legal_sorted` for ALL
   dirty members in ONE batched kernel query against the updated committed
   geometry (density_penalty recomputed against the new committed corridors).
   A dirty member that becomes empty leaves to R2/R3; others keep their new
   sorted list. Clear dirty flags.
6. Recompute `Ready` and loop to 2 until `Ready` is empty.

### 4.4 Why the committed set stays mutually legal (soundness)

The mutual-legality guard (§4.3-3-ii) is the invariant's inductive step. Base:
the first commit is legal against a committed set that contains only non-pocket
forest, checked in step 1. Step: when `m` commits, (i) makes `m` legal against
everything already committed, and (ii) forbids `m` from illegalizing any
already-committed neighbour. Since committed geometry only GROWS and every
addition preserves "no member in the committed set is illegal", the committed
set is a fully mutually-legal set at every step and at termination. (This is
the §6.9 zero-NEW-firm-transit gate enforced locally, at commit time, so the
global gate cannot be surprised by a pocket.)

### 4.5 Termination, no-oscillation, determinism

- **Termination / monotone objective:** each iteration moves exactly one
  member from UNRESOLVED to COMMITTED (or to FALLEN → R2/R3); COMMITTED and
  FALLEN are terminal. The count of UNRESOLVED members strictly decreases by
  one per iteration, so the loop runs ≤ |pocket| iterations and halts. There
  is no sweep cap because none is needed — the objective (resolved count) is
  strictly monotone, not merely bounded.
- **No oscillation:** no member is ever un-committed; a committed candidate is
  never revisited. Re-evaluation only ever rewrites the sorted list of an
  as-yet-UNRESOLVED member.
- **Determinism:** world-origin-pinned voxels + lexicographic component labels
  ⇒ pocket identity is a pure function of geometry; conflict-graph edges are a
  pure geometric predicate stored sorted; candidate sets are fixed grids;
  legality/score are exact IEEE-754 kernel formulas; the commit priority
  (worst-firm-chord desc, mid asc) and the score tuple (…, candidate_index)
  are TOTAL orders, so "highest-priority member" and "lexicographic-min
  candidate" are both unique. Batched chunking (PASS-B 384) carries no
  cross-ray state, so it cannot perturb bits. No RNG, no wall-clock ⇒
  bit-identical across the 5 acceptance runs, and (with an empty defect set)
  a no-op on a second pass — the §6.10 idempotence gate.

### 4.6 Isolated (non-pocket) members

Members in no pocket component use the plain per-site SM-STUB/SM-STRUT ladder;
`density_penalty` is 0 for them, so their §3.1 score tuple reduces to the
sparse ordering. The interaction machinery is pocket-only by construction —
sparse sites pay nothing.

---

## 5. Builder/verify reconciliation — one law, one authority

### 5.1 The placement-time predicate moves to SCAR-AREA-CAP + PRINT-FLOOR

Today the builder places by the retired law: `tree.Forest.revalidate`
(tree.py:365) masks embed zones with the arc-inset (`_embed_inset_arc`
tree.py:391, cached at tree.py:504) and has no rule-4/scar test, while the
gate judges by SCAR-AREA-CAP + rule 4 (verify.py:299). **The builder must
PLACE by the same law the gate judges by** — otherwise every Phase-8 rebuild
is placed blind and re-condemned at verify. Required moves:

- `tree.Forest.revalidate` (exact-oracle path) gains the transit check:
  per-member kernel intervals (stubs axis-only, non-stubs axis+capsule-offset
  — the gate's exact population rules, verify.py:337-343), SCAR-AREA-CAP tip
  verdicts gating the rule-4 exemption, PRINT-FLOOR mootness. The arc-inset
  remains ONLY as the distance-gate mask on the swept-clearance test, exactly
  as verify.py:326 already scopes it.
- The A0b quarantine pre-pass (cco.py:196-220) inherits this automatically —
  it just calls `revalidate` — which is precisely what re-grades the
  quarantine pool (§3.3) and legalizes buried deep plugs at placement time.
- **Independence covenant is preserved:** verify.py keeps its own independent
  implementation (verify.py:299-395, which never calls tree's helpers). Both
  sides implement the shared SPEC text; neither calls the other. The shared
  primitive layer remains the kernel's generic interval/projection queries.
- Certificate semantics: transit verdicts join the clearance verdict under
  the existing geometry-keyed certificate (a member's cert covers BOTH
  checks); any geometry change voids both. No second cert channel.

### 5.2 Buried-deep-plug divergence — resolved in favor of the ratified law

A deep perpendicular plug into thick solid is LEGAL (SPEC-EMBED-POLICY: small
crossing patch, idempotent union, no depth cap; thin walls are protected by
rule 4 + thin-feature span). The builder's current quarantine of such stubs
is a false condemnation under the ratified law and ends with §5.1. Any
lingering depth-flavored placement heuristic is retired with it.

### 5.3 mi=445-class tip-anchor boundary — three-layer authority ruling

The engine/census symdiff {445, 474, 575} exists because the census's raw
dense ParityVote over-segments near-tip slivers into separate intervals,
manufacturing a phantom "unanchored" transit where the kernel's FINDING-2
robust dense-majority classification resolves one clean tip-anchored interval.
mi=445 is the sharp case: a 0.2531 mm chord the engine exempts via the kernel
tip-anchor merge, while the census's independent reading counts it non-exempt.
The ruling is **layered by purpose** so no single "authority" claim has to do
contradictory work:

- **Layer 1 — geometric correctness: the kernel robust oracle is
  authoritative, and is NEVER de-robustified.** The FINDING-2 robust reading
  is the correct geometry (owner-reviewed: the sliver IS the tip interval, not
  a transit). De-robustifying the kernel to match the census would re-import
  measured process noise into a ratified gate. The kernel reading stands.
- **Layer 2 — acceptance TARGET SELECTION: the census count is
  authoritative (conservative).** For deciding what Phase-8 relocates, mi=445
  — and any member the census's independent SCAR-AREA-CAP + PRINT-FLOOR
  reading counts non-exempt — **is treated as a relocation target**, even
  where the kernel exempts it. This is the ruling the acceptance uses.
  Rationale: **(1) conservative** — relocating an arguably-legal boundary stub
  costs at most a small R1 re-aim and can never make geometry worse (R1
  preserves the contact; the re-aimed placement is gate-legal on BOTH sides by
  construction); **(2) it avoids forcing the census to bit-replicate the
  kernel's dense-majority merge**, which Open-risk #3 flags as the
  highest-leverage way to accidentally reopen a symdiff — the census keeps its
  own independent machinery untouched; **(3) after relocation the member is
  legal under both independent implementations**, so no acceptance carve-out
  survives. Consequently the earlier draft's "census MUST adopt the kernel's
  robust interval merge" is withdrawn — the census is NOT edited to chase the
  kernel; the divergence is resolved by *remediating* the member, not by
  aligning classifiers.
- **Layer 3 — cross-implementation EQUALITY (§6.3): canonicalized
  integer-micron chords, not float-bit-identity.** engine==census is asserted
  by rounding every crossing chord to integer microns before comparison
  (amendment G) — the robust reading only ever differed from the census below
  that granularity, so on the archived geometry the two agree at 143 non-exempt
  / 50 moot on the micron-canonical comparison, and after relocation they agree
  member-for-member with the symdiff empty. The blessed float symdiff
  {445, 474, 575} is RETIRED (not as a carve-out but because it never survives
  micron canonicalization + remediation); mi=445 becomes a named regression
  fixture for the robust boundary (§6.8).

---

## 6. Acceptance

On the rebuilt full-scale treant (same config as the archived runs):

1. **census non-exempt transit members == 0**; `moot_transit_members`
   reported by both engine and census with identical member sets and
   per-crossing chords compared at **integer-micron canonicalization**
   (§6.3) — the 50 moot members survive unchanged and gate nothing.
2. **true ball breaches == 0** (`source="ball"`); endcap entries stay
   report-only (SM-BALL scope unchanged).
3. **engine gate == census exactly, on canonicalized integer-micron chords**
   (amendment G / §5.3 Layer 3) — every chord rounded to integer microns
   before the comparison, NOT float-bit-identity. On the archived geometry
   the two agree at 143 non-exempt / 50 moot; after relocation they agree
   member-for-member with the symdiff empty. No blessed float carve-out.
4. **Determinism:** 5 full runs, one forest digest, as the current baseline
   discipline (09ce1fd6a9a1… precedent).
5. **No regression on legal embeds:** member-level diff against
   `v5e_run1.pkl` — every member legal under the ratified law and NOT in the
   **143/15/77** target sets is either bit-identical or, where a pocket commit
   legitimately moved a neighbor, re-verified legal; the count of moved
   non-target members is reported and must be < 5% of the legal population.
6. **Resin-cost delta bound:** total support volume within **+3.0%** of the
   archived baseline (ratification lever `p8_resin_delta_max = 0.03`);
   per-pocket volume deltas reported. Per-candidate resin was already bounded
   at selection by the amendment-H guards (`p8_reaim_volume_delta_max_ml`,
   `p8_reaim_route_growth_max_factor`, `p8_strut_growth_max_factor`); this is
   the aggregate backstop.
7. **Criticality-weighted honest-failure bound (amendment G):** items ending
   at the honest-failure rung (SM-STUB R3 / SM-STRUT S3 / SM-BALL B3) ≤
   `p8_max_unresolved = 10` by count, AND their criticality-weighted sum ≤
   `p8_max_unresolved_weight` — where weight is the site criticality
   (structural strut that would orphan a subtree ≫ a leaf contact stub), so
   the bound cannot be satisfied by abandoning many high-criticality sites
   under a raw count. Each unresolved item reports position, `class`
   (STUB/STRUT/BALL), criticality weight, and full ladder trace. Exceeding
   EITHER bound fails acceptance and goes to the owner — the spec never
   authorizes silently abandoning demand.
8. **Site-derived regression tests with the change (binding rule):** minimum
   set — one P1-cluster stub fixed by R1 re-aim; one member fixed only by R2
   re-seat; one 0.08-0.10 band member (non-exempt — must be rebuilt); one
   all-moot member (exempt — must NOT be rebuilt, appears in
   `moot_transit_members`); **a chord at EXACTLY 0.08 mm** (the strict-`<`
   PRINT-FLOOR boundary — it is NON-moot / firm and its member is non-exempt;
   amendment G boundary fixture); mi=445 robust tip-anchor boundary (census
   authoritative → treated as a target, §5.3 Layer 2; legal on both sides
   after re-aim); one buried deep plug (legal at placement post-§5.1); one
   interior strut fixed by SM-STRUT S1; one ball breach fixed by B1 slide; the
   SPEC-EMBED-POLICY fixture set stays green. Suite + clippy green after every
   commit.
9. **Final-geometry verifier pass over the SERIALIZED output (amendment G):**
   the acceptance gate re-loads the *emitted* geometry (post-serialization,
   not the in-memory forest) and runs the complete SCAR-AREA-CAP + rule-4 +
   PRINT-FLOOR + ball gate over it — catching any serialize/round-trip drift
   the in-memory pass would miss. Additional gates in this pass:
   - **zero-NEW-firm-transit:** the set of firm-transit members in the output
     ⊆ the frozen target set — re-aiming must NEVER introduce a firm transit
     into a member that was legal in `v5e_run1.pkl` (the §4.4 mutual-legality
     guard enforced locally; this is the global check).
   - **raft-connectivity / orphan check:** every emitted support member is
     connected to the raft; no subtree was orphaned by a `mark_dead`
     (SM-STRUT S3 / SM-BALL B3). Any orphan fails acceptance.
10. **Idempotence / second-run no-op (amendment C/G):** a second Phase-8 pass
    over the §6.9 serialized output re-derives an EMPTY frozen defect snapshot
    (0 non-exempt, 0 breaches, 0 REAL-quarantine), enters no ladder, and emits
    **byte-identical** geometry (same digest). A non-empty second-pass snapshot
    or any geometry delta fails acceptance — it would mean the first pass left
    a latent defect or the pass is not a fixed point.

---

## 7. Compute budget — fits the <60 s/model trajectory

Batch-first is normative in every hot path (a per-item kernel call is a
defect, not a style choice):

- **Candidate legality:** class-T full matrix ≈ 143 sites (126 stub SM-STUB +
  17 strut SM-STRUT) × ≤75 candidates ≈ 10.7k axis rays → a handful of batched
  kernel interval calls (chunked at the census's proven PASS-B batch 384; RSS
  stays < 2 GB per the census memory discipline). Interval extraction,
  PRINT-FLOOR chords, and SCAR-AREA-CAP verdicts are vectorized over the batch
  — the existing `_batch_axis_intervals` / `_embed_scar_verdicts` shapes
  (verify.py:169, 229) are the model, and the tree-side port (§5.1) reuses the
  `_embed_insets_cached` batching pattern (tree.py:504), never the retired
  per-member scalar `geom.project` loop.
- **Pocket rescoring** is incremental (full dependency-set invalidation over
  the conflict graph, §4.3-§4.4). The loop is ≤ |pocket| atomic commits (§4.5
  termination), each commit re-evaluating only the committed member's graph
  neighbourhood in ONE batched call — worst case ≈ the pocket's candidate
  matrix summed over commits, still one batched kernel call per commit, no
  per-candidate scalar loop.
- **Placement-law revalidate** adds interval queries to the builder loop;
  they ride the existing geometry-keyed verdict cache + dirty-index
  incremental revalidate (SPEC-V5-PERF A1/A2), so steady-state cost is
  proportional to dirty members, not forest size.
- Budget assertion: the Phase-8 rebuild pass itself (candidate matrices +
  commits + re-verify) must complete in ≤ 15 s on the treant at current
  kernel throughput, keeping the full-model total on the <60 s trajectory.
  Measured and reported in the run log.

---

## 8. Config / constants added

    PRINT_FLOOR_CHORD_MM   = 0.08   # §2, normative, both implementations
    p8_reaim_polar_deg     = (0, 10, 20, 30)
    p8_reaim_azimuth_deg   = 8 steps × 45°
    p8_reaim_length_factors= (1.0, 0.75, 0.5)
    p8_pocket_voxel_mm     = 6.0     # world-origin-pinned grid (§4.1)
    p8_pocket_min_members  = 8
    p8_pocket_corridor_pad_mm = 0.5  # also the detection halo (§4.1) + density radius (§4.2)
    p8_ball_slide_step_mm  = 0.25
    p8_ball_slide_max_mm   = 2.0
    # amendment-H per-candidate resin / quality guards (part of legality):
    p8_reaim_volume_delta_max_ml     = 0.02   # §3.1 R1 per-relocation volume-delta cap
    p8_reaim_route_growth_max_factor = 1.50   # §3.1 R1 downstream-route growth cap
    p8_strut_growth_max_factor       = 1.25   # §3.1s S1 no-unbounded-strut-growth cap
    # acceptance levers:
    p8_resin_delta_max     = 0.03            # §6.6 aggregate resin backstop
    p8_max_unresolved      = 10              # §6.7 honest-failure count bound
    p8_max_unresolved_weight = 3.0           # §6.7 criticality-weighted bound (amendment G)
    p8_chord_canon_micron  = 1               # §6.3 integer-micron chord canonicalization (amendment G)

The three amendment-H caps (`p8_reaim_volume_delta_max_ml`,
`p8_reaim_route_growth_max_factor`, `p8_strut_growth_max_factor`) are
per-candidate legality guards, not just acceptance levers — a candidate that
violates one is illegal at selection, never merely penalized. All
deterministic; no RNG anywhere; every ordering in this spec is a stable sort
on an explicit key with the index as final tiebreak.

---

## 9. Open risks

1. **R1 cone vs printability family:** the 30° polar reach must be validated
   against the stub lean limit; if the config family caps lower, the grid
   shrinks and more sites fall to R2. Fixture: one candidate skipped for
   lean, not clamped.
2. **Pocket solver conflict-graph density:** the atomic-commit loop (§4.3)
   terminates in ≤ |pocket| commits regardless of density, but a very dense
   pocket makes `N(m)` large, so each commit's invalidation re-evaluates many
   members — worst-case cost, not correctness, is the risk. If a pocket's
   conflict graph is near-complete and the §7 budget regresses, the fallback
   is finer voxels (smaller `p8_pocket_voxel_mm`) to split the component, never
   a sweep cap. Members with an empty legal set still fall to R2/R3 and the
   §6.7 bound arbitrates.
3. **Census micron-canonical equality (§5.3 Layer 3)** — the acceptance no
   longer forces the census to bit-replicate the kernel's robust merge (that
   edit is WITHDRAWN, §5.3), so the old off-by-tolerance reopen-risk is gone;
   the residual risk is a chord straddling a micron boundary. The
   mi=445/474/575 trio is the fixture; `p8_chord_canon_micron` is the only
   knob.
4. **Band members (0.08-0.10)** are deliberately non-exempt; if the owner
   later widens PRINT-FLOOR, only `PRINT_FLOOR_CHORD_MM` moves and the 25
   band members re-partition — no structural change. Do not pre-build for it.
5. **Builder-side transit check cost** rides the verdict cache; if profiling
   shows the placement loop regressing past budget, the fallback is
   coarse-prefilter (AABB against the foliage shell) before intervals —
   correctness identical, order preserved.
