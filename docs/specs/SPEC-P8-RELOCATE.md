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
  **144 non-exempt transit members** (§3 class T), **15 true ball breaches**
  (§3 class B), and the **quarantine pool re-graded under SCAR-AREA-CAP, 77
  REAL** (§3 class Q).
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
census members, 49 have all crossings < 0.08 mm → moot-exempt. Partition:
118 firm (some crossing ≥ 0.10 mm) + 26 band (worst crossing in
0.08-0.10 mm, non-exempt) + 49 all-moot. **Rebuild target = 144 non-exempt
members.** 104 of the 118 firm are contact stubs.

---

## 3. Defect classes and per-class remediation

### 3.1 Class T — 144 non-exempt transit members (the rebuild core)

Mostly contact stubs whose shaft, after (or instead of) its legal
tip-anchored interval, transits adjacent foliage (census: 176/193 stubs,
`through_and_through`, transit chords overwhelmingly < 1 mm). Strut subclass
(17 in census; capsule-edge grazes like mi=1138) handled by the same ladder
with R1 interpreted as foot re-aim.

**Remediation ladder — DETERMINISTIC selection order, first success wins:**

**R1 — tip re-aim (expected to resolve most stubs; 104/118 firm are stubs).**
Keep the contact where demanded: the tip stays at the contact `position` with
`embed_end="p0"` and the same designed ball; only the shaft direction and
length change. Today the stub is built rigidly along the contact normal
(cco.py:176-181: `p_tip = pos - n*contact_depth_mm`,
`p_stub_end = pos + n*stub_length_mm`). R1 searches a deterministic candidate
set over (direction, length):

- directions: a fixed cone grid around the contact normal — polar angles
  `(0°, 10°, 20°, 30°)` × azimuths `(0..315° step 45°)` in that nested order
  (polar-major, azimuth-minor; azimuth 0 anchored to the stable local frame
  `e = normalize(cross(n, ẑ))`, with the `|n·ẑ|>0.999` degenerate case
  falling back to `x̂`), polar 0 emitted once. 25 directions. The cone must
  respect the printability lean limit for stubs (config theta family) —
  candidates outside it are skipped, not clamped;
- lengths: `stub_length_mm` × (1.0, 0.75, 0.5) — shorter stubs escape
  overhead foliage; the floor keeps enough shaft for the router to grab;
- candidate order: length-major (1.0 first — prefer the undisturbed length),
  then the direction order above. Candidate index is the final tiebreak.

A candidate is **legal** iff, evaluated by the kernel exactly as the gate
does (axis ray for stubs): (a) its tip SCAR-AREA-CAP verdict is LEGAL
(re-aim changes `cosang`, `T`, `r_x` — the verdict MUST be recomputed, never
carried over), (b) it has zero non-exempt rule-4 crossings under PRINT-FLOOR,
and (c) `forest.revalidate` under the §5 placement law passes (the re-aimed
shaft may not collide with other members/model). Because the stub end moves,
the terminal's downstream routing is re-trialled through the ordinary
candidate builders (`_candidate_builders`, cco.py:432) from the new
`stub_end`; routing failure fails the candidate (rollback), it does not
strand the contact.

Score among legal candidates (lexicographic, minimize):
`(n_moot_crossings, polar_angle_deg, length_reduction_step, route_volume_ml,
candidate_index)` — prefer clean over moot-riding, least deviation, least
shortening, cheapest route.

**R2 — contact re-seat (relocation to an alternative anchor candidate).**
Only if no R1 candidate is legal. Move the contact within its inviolable
class domain (REACH §1 table, unchanged), candidates ordered
nearest-to-demanded with `reloc_max_candidates` cap (REACH §2b), but the
per-candidate legality test is the §5 placement law (tip verdict + rule-4 +
PRINT-FLOOR + clearance), not clearance alone. Old stub `mark_dead`
(tree.py:348-family), new stub + routing per REACH §2b, neighbor-birth guard
before commit. Score = route length (REACH's pinned objective), candidate
order tiebreak.

**R3 — quarantine + re-route.** Only if R1 and R2 both fail. The stub is
`mark_quarantined` (tree.py:348) and the contact goes to the honest-failure
path — reported unrouted with its carried area, exactly the A0b discipline
(cco.py:196-220). No fabricated support. The acceptance bound (§6) caps how
many sites may end here.

The ladder order R1→R2→R3 is normative: it minimizes disturbance to the
demanded contact set (re-aim preserves the contact exactly; re-seat preserves
the demand but moves it; quarantine abandons it) and is therefore also the
resin- and coverage-conservative order.

### 3.2 Class B — 15 true ball breaches (relocate)

The 15 `source="ball"` joint balls (R3 ball-gate scope; endcap spheres stay
report-only) whose sphere spans a feature thinner than its radius
(`geom.sphere_breach`, verify.py:395-444; census `breaching_balls`, e.g.
ball 53: r 0.35, feature 0.2814, poke-out 0.0686). Remedy is **relocation of
the joint, never shrinking the ball** (ball radii are designed:
BALL_FACTOR × r_end / joint sizing — a smaller ball is a different sanction
and is not on offer):

- **B1 — axial slide:** move the joint point along its member's axis away
  from the thin feature in steps of 0.25 mm up to 2.0 mm, first direction
  root-ward then tip-ward (deterministic), accepting the first position where
  `sphere_breach` is clean AND both adjoining member segments stay legal
  under the §5 law (revalidate + rule-4/PRINT-FLOOR on the re-split
  segments).
- **B2 — local re-route:** if no slide position is clean, the owning
  terminal's route is re-trialled via `_candidate_builders` with the
  breaching joint's region voxel (§4) recorded as crossing-penalized; score
  as R1.
- **B3 — quarantine + re-route** as 3.1-R3, same honest-failure discipline.

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

## 4. Pocket strategy — density-aware handling of the three clusters

Adjudicated clustering: canopy pocket P1 x∈[6,12], y∈[-12,-6], z∈[48,54]
(~37 members incl. adjacent cell), canopy pocket P2 x∈[18,36], y∈[6,18],
z∈[48,60] (~22), lower-trunk pocket P3 x∈[-36,-30], y∈[12,18], z∈[-24,-18]
(17). Half the defect mass; 71% of all 144 sit above CROWN_Z=35 in dense
foliage where **per-site greedy re-aiming thrashes**: a re-aimed shaft can
create a new crossing through a neighbor's corridor, and a naive sequential
pass can oscillate.

Normative pocket handling:

1. **Pocket detection is derived, not hardcoded:** bucket the non-exempt
   member tip positions into 6 mm voxels; a pocket = a connected voxel
   component (6-connectivity) containing ≥ `pocket_min_members = 8` defects.
   Deterministic: voxel keys sorted lexicographically; components labeled in
   that order. The three clusters above are the expected output on the
   archived geometry and become a regression fixture; the algorithm must not
   read them as input.
2. **Crossing-aware batched scoring, not per-site greedy.** For a pocket,
   gather the FULL candidate matrix first — every member × its R1 candidate
   set (≤75 per stub) — and evaluate legality/moot-count in **one batched
   kernel interval query** (gather → one call → scatter; a per-candidate
   scalar loop in this path is a defect per the batch-first rule). Candidate
   scores additionally carry a **density penalty**: the count of *other*
   pocket members' corridors (capsule at current radius + standoff) the
   candidate shaft passes within 0.5 mm of, computed against the pocket's own
   member set. This makes candidates that dive INTO the pocket's shared
   foliage lose to candidates that exit outward, which per-site greedy cannot
   see.
3. **Commit order inside a pocket: worst-first** — members sorted by
   (descending worst firm chord, ascending member index); each commit
   re-trials only the not-yet-committed members whose cached candidate rays
   pass within (r + standoff + 0.5 mm) of the committed geometry
   (invalidation is geometric, batched, not a full rescore). Two full sweeps
   maximum; any member still without a legal candidate after sweep 2 falls to
   R2/R3. Bounded, deterministic, no oscillation.
4. **Isolated (non-pocket) members** use the plain per-site ladder — the
   interaction machinery is pocket-only by construction, so sparse sites pay
   nothing.

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

### 5.3 mi=445-class tip-anchor boundary — the kernel robust oracle is the authority

The engine/census symdiff {445, 474, 575} exists because the census's raw
dense ParityVote over-segments near-tip slivers into separate intervals,
manufacturing a phantom "unanchored" transit where the kernel's FINDING-2
robust dense-majority classification resolves one clean tip-anchored
interval. **Decision: the kernel robust oracle is the single authority.**
Why: the robust reading is the correct geometry (owner-reviewed; the sliver
is the tip interval, not a transit); de-robustifying the kernel to match the
census would re-import measured process noise into a ratified gate — the
kernel reading is correct, do NOT de-robustify. Consequences:

- the census MUST adopt the FINDING-2 robust interior classification for its
  interval construction (it already implements SCAR-AREA-CAP from spec text;
  this extends the shared-text discipline to interval semantics). It still
  never calls the kernel — it implements the same spec'd classification
  independently;
- after alignment, engine gate == census **exactly** (190 == 190 on the
  archived geometry pre-PRINT-FLOOR; identical non-exempt/moot partitions
  after). The blessed symdiff {445, 474, 575} is RETIRED as an acceptance
  carve-out; mi=445 becomes a named regression fixture for the robust
  boundary instead.

---

## 6. Acceptance

On the rebuilt full-scale treant (same config as the archived runs):

1. **census non-exempt transit members == 0**; `moot_transit_members`
   reported by both engine and census with bit-identical member sets and
   per-crossing chords.
2. **true ball breaches == 0** (`source="ball"`); endcap entries stay
   report-only (R3 scope unchanged).
3. **engine gate == census exactly** — no blessed symdiff (§5.3).
4. **Determinism:** 5 full runs, one forest digest, as the current baseline
   discipline (09ce1fd6a9a1… precedent).
5. **No regression on legal embeds:** member-level diff against
   `v5e_run1.pkl` — every member legal under the ratified law and NOT in the
   144/15/77 target sets is either bit-identical or, where a pocket commit
   legitimately moved a neighbor, re-verified legal; the count of moved
   non-target members is reported and must be < 5% of the legal population.
6. **Resin-cost delta bound:** total support volume within **+3.0%** of the
   archived baseline (ratification lever `p8_resin_delta_max = 0.03`);
   per-pocket volume deltas reported.
7. **Honest-failure bound:** contacts ending at R3 ≤ `p8_max_unresolved = 10`
   across all classes, each reported with position, class, and ladder trace.
   Exceeding the bound fails acceptance and goes back to the owner — the
   spec does not authorize silently abandoning demand.
8. **Site-derived regression tests with the change (binding rule):** minimum
   set — one P1-cluster stub fixed by R1 re-aim; one member fixed only by R2
   re-seat; one 0.08-0.10 band member (non-exempt — must be rebuilt); one
   all-moot member (exempt — must NOT be rebuilt, appears in
   `moot_transit_members`); mi=445 robust tip-anchor boundary (legal, both
   sides); one buried deep plug (legal at placement post-§5.1); one ball
   breach fixed by B1 slide; the SPEC-EMBED-POLICY fixture set stays green.
   Suite + clippy green after every commit.

---

## 7. Compute budget — fits the <60 s/model trajectory

Batch-first is normative in every hot path (a per-item kernel call is a
defect, not a style choice):

- **Candidate legality:** class-T full matrix ≈ 144 sites × ≤75 candidates
  ≈ 10.8k axis rays → a handful of batched kernel interval calls (chunked at
  the census's proven PASS-B batch 384; RSS stays < 2 GB per the census
  memory discipline). Interval extraction, PRINT-FLOOR chords, and
  SCAR-AREA-CAP verdicts are vectorized over the batch — the existing
  `_batch_axis_intervals` / `_embed_scar_verdicts` shapes (verify.py:169,
  229) are the model, and the tree-side port (§5.1) reuses the
  `_embed_insets_cached` batching pattern (tree.py:504), never the retired
  per-member scalar `geom.project` loop.
- **Pocket rescoring** is incremental (geometric invalidation, §4.3), two
  sweeps max — worst case ≈ 2× the pocket's candidate matrix, still one
  batched call per sweep.
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
    p8_pocket_voxel_mm     = 6.0
    p8_pocket_min_members  = 8
    p8_pocket_corridor_pad_mm = 0.5
    p8_ball_slide_step_mm  = 0.25
    p8_ball_slide_max_mm   = 2.0
    p8_resin_delta_max     = 0.03   # acceptance lever
    p8_max_unresolved      = 10     # acceptance lever

All deterministic; no RNG anywhere; every ordering in this spec is a stable
sort on an explicit key with the index as final tiebreak.

---

## 9. Open risks

1. **R1 cone vs printability family:** the 30° polar reach must be validated
   against the stub lean limit; if the config family caps lower, the grid
   shrinks and more sites fall to R2. Fixture: one candidate skipped for
   lean, not clamped.
2. **Pocket two-sweep bound:** adversarial density could leave members
   unresolved at sweep 2; they fall to R2/R3 and the §6.7 bound arbitrates.
   If the archived pockets exceed it, the pocket scorer (not the bound) gets
   revisited.
3. **Census robust-classification port (§5.3)** is the highest-leverage
   alignment edit — an off-by-tolerance in the census's independent FINDING-2
   implementation reopens a symdiff. The mi=445/474/575 trio is the fixture.
4. **Band members (0.08-0.10)** are deliberately non-exempt; if the owner
   later widens PRINT-FLOOR, only `PRINT_FLOOR_CHORD_MM` moves and the 26
   band members re-partition — no structural change. Do not pre-build for it.
5. **Builder-side transit check cost** rides the verdict cache; if profiling
   shows the placement loop regressing past budget, the fallback is
   coarse-prefilter (AABB against the foliage shell) before intervals —
   correctness identical, order preserved.
