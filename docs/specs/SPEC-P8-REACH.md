# SPEC-P8-REACH — Contact Relocation + Concession Supports (v4 CCO, Phase 8)

Author: Phase-8 designer. Charter: `skills/support-placement/references/design-v4.md`.
Authority update: the A2 virtual-analysis-face and final-provenance amendments
below were owner-ratified on 2026-07-25 and supersede this draft's older
centroid/direct-source-face language wherever they conflict.
Target commit: f02b7af (repo read-only). Citations below name the authoritative
symbol; historical line numbers are only orientation and must not be treated as
stable API.

This spec covers TWO features shipped as one merge:

- **A. CONTACT RELOCATION** — when routing fails for a *demanded* contact, search
  nearby *constraint-legal* surface for a routable alternate before parking it unrouted.
- **B. CONCESSION SUPPORTS** (Craig-authorized 2026-07-23) — when relocation ALSO
  fails AND the spot is real-risk, permit a support whose *clearance standoff* is
  progressively reduced, recorded per-member so verify checks it against its own
  conceded envelope and everyone knows where to buff/fill in post.

---

## 0. Where "routing fails" actually happens (shared substrate)

Insertion is `cco.build` (cco.py:157). Per contact it:
1. builds a **stub** Member at the contact's `position`/`normal`
   (cco.py:165-178), registers `forest.terminal_of[i]`;
2. tries candidate route families in `_insert_terminal` (cco.py:268) —
   `_candidate_builders` = plate-descent (a) + model-anchor (b) + bifurcation (c)
   (cco.py:316-323), each transactionally trialled (checkpoint → build_fn →
   `forest.revalidate` → rollback), cheapest **real-volume** survivor committed
   (cco.py:280-306);
3. if **no candidate survives clearance**, `_insert_terminal` returns `False`
   (cco.py:294 `if not scored: return False`), and the build loop parks the contact:
   `if not ok: unrouted.append(i)` (cco.py:192-194).

`unrouted` propagates out of `cco.build` into `engine.construct` and the
fix-pass loop. Coverage, minima, and island-birth demand gates deliberately
receive only routed contacts, so a missing routed demand remains visible.
Final regular-contact provenance is different: `engine` passes the complete
original contact list to `verify.self_verify(provenance_contacts=...)`, and
`regular_contact_invariant_report` checks it using the original contact
indices, including contacts currently marked unrouted. Routing status must
never hide stale position/face/normal provenance.

**Both features hook the same seam: the transition at cco.py:192-194 from "candidate
loop exhausted" to "append to unrouted".** Feature A interposes a relocation attempt;
Feature B, a concession attempt; only if both decline does the contact stay unrouted.

Determinism (charter §4, "No RNG anywhere"): every new candidate ordering below is a
**stable sort on an explicit key**. No `mesh.sample`, no RNG, and no mutation of
NumPy's global RNG state.

**A2 regular-contact seat invariant (ratified, then coverage-superseded
2026-07-25).** Initial regular
placement, coverage repair, and every Phase-8 relocation of a regular/coverage
contact select from one deterministic pool:

1. each source face is uniformly refined by recursive 1-to-4 midpoint
   subdivision until every virtual analysis-cell edge is at most
   `target_spacing_mm * coverage_radius_factor - target_spacing_mm`; the margin
   MUST be positive;
2. refinement depth is
   `ceil(log2(source_face_max_edge / margin))`, bounded by
   `contact_pool_max_refine_depth`; exceeding the bound is a clear refusal, not
   an unbounded allocation. A one-ULP excess at a nominal dyadic edge threshold
   is accepted rather than spuriously adding a level;
3. every virtual cell nominates every co-lowest vertex; equality uses a tight
   8-ULP-scale absolute z tolerance so a flat cell is independent of arbitrary
   vertex order without merging distinct geometric heights;
4. nominations at the exact same position/identity are deduplicated;
5. the owning **source** face is the nominator with the most-negative
   face-normal `nz`, with global source face id as the deterministic tie-break;
6. the persisted normal is that owning source **face normal** exactly, never an
   averaged vertex normal or a routing-normalized rewrite, and the contact
   record persists the owning source `face_id`.

This refinement is invariant under prior **dyadic midpoint refinement**, not
arbitrary retriangulation. It adds analysis points on the original planar
triangle; it does not change the printable model or invent centroid/random
fallbacks. The pool is built once as picklable prepared state and reused by
initial placement, repair, Phase 8, and final verification.

Before any virtual-cell allocation, a global pass plans every face and refuses
if aggregate virtual cells, the conservative `3*cells` nomination bound, or its
64-byte/candidate compact-payload bound exceeds configuration. Defaults are
750,000 cells, 2,250,000 nominations/candidates, and 144 MiB: about 30% headroom
over the known 576k-face depth-0 class. The byte limit is an honest bound on
the retained compact struct-of-arrays payload, **not** a process-peak-memory
claim. The aggregate caps are the primary allocation prevention: each island's
one-cell-per-face lower bound is checked before face conversion or triangle
materialization, and admitted faces are planned in fixed batches of at most
2,048 with monotonic refusal as soon as any cap wins. As a last-resort
assertion, every long construction loop also aborts if `getrusage`'s process
**peak-RSS high-water mark** has risen by 2 GiB from its build-entry high-water
mark; this is not a current-RSS measurement. Nominations are gathered, sorted,
and deduplicated in bounded NumPy batches rather than per-nomination Python
tuples/dicts.

The prepared pool is a schema-versioned, picklable struct of four immutable
NumPy arrays (position, raw normal, source face id, island id), exactly 64
payload bytes per retained candidate. Construction and unpickling rebuild
bytes-backed, C-contiguous, read-only arrays; staged validation checks shapes,
dtypes, ordering, counters, face/island ranges, and raw-normal provenance.
Synthetic-island extension repeats the same preflight and returns a new pool.
Its island-id offset must be strictly above the prepared prefix, and admitted
prefix/chunks are joined directly into one final immutable bytes allocation per
SOA column rather than constructing and copying an intermediate pool.
Prepared state also records deterministic SHA-256 fingerprints of canonical
mesh arrays and semantic config JSON. Construct, finish, relocation, and
performance-cache reuse reject mismatched mesh/config identity.

This is deliberately a **regular/coverage-contact invariant only**. Minimum and
island-birth contacts retain the stricter region/footprint domains below and do
not acquire a false `face_id` merely to resemble a pool contact. Final verify
hard-gates regular pool membership, owning `face_id`, exact source-face normal,
and this class scope; emission cannot pass stale provenance.

---

## 1. Constraint domains — what "nearby surface" a contact may legally move to

The demanded-contact *class* is read off the contact dict (already set by the engine).

**PRECEDENCE IS MANDATORY, NOT INCIDENTAL (at-birth rule).** Island-birth contacts are
constructed with **BOTH** `island_birth=True` **AND** `minimum=True` (engine.py:569 sets
both flags on the same dict — confirmed). The two discriminators therefore COLLIDE on a
birth contact, and the class test MUST be evaluated **island_birth first**:

```
if c.get("island_birth"):   cls = "island-birth"   # ← MUST be tested BEFORE minimum
elif c.get("minimum"):      cls = "minimum"
elif "island_id" in c:      cls = "coverage"
```

Testing `minimum` first would route a birth contact into the **minimum** relocation
domain — `within cfg.minima_contact_radius_mm=1.0 of any region_pts vertex`
(config.py:359) — a 1.0mm lateral domain that (a) reads a `region_pts` key the birth
contact does not even carry (§1a attaches `region_pts` only in `_ensure_minima_contacts`;
birth contacts carry only `birth_loop`/`birth_z`, engine.py:568-569), and (b) can land
the contact OFF its birth footprint, whereas `island_birth_report` (verify.py:258-268)
requires xy ON the polygon or within `ISLAND_ATTACH_REACH_MM=0.2` of its boundary. That
is a direct violation of Craig's at-birth rule and re-trips the very gate the relocation
was meant to satisfy. **The implementer MUST NOT read the class off an unordered
`c.get("minimum")` check; island_birth wins whenever both flags are set.**

| Class | Discriminator (order-sensitive) | Legal relocation domain (INVIOLABLE) |
|---|---|---|
| island-birth | `c.get("island_birth")` set at engine.py:569 — **tested FIRST**, wins over `minimum` (both flags co-set at engine.py:569) | **only inside its birth footprint polygon** at the birth z (Craig's at-birth rule) |
| minimum | `c.get("minimum")` set at engine.py:380 & 569 — **only reached when `island_birth` is falsy** | **only within the toe's cfg-derived connection reach** (the birth *region*) |
| coverage / regular | neither flag; has `c["island_id"]` | **only the prepared A2 virtual lowest-vertex pool for its own island** |

### 1a. Thread the footprint payload onto the contact (small, additive)

The domains need data the contact dict does not yet carry. Add at construction — no
schema break, just extra keys downstream code ignores:

- **island-birth** (`engine._ensure_island_birth_contacts`, contact built at
  engine.py:568-569): also store `birth_loop=b["loop"]` and `birth_z=b["z"]`
  (both already in scope from `b`, the birth dict — see `perception.island_births`
  return shape perception.py:468). Domain test reuses
  `perception.island_birth_xy_ok(px,py,loop,ISLAND_ATTACH_REACH_MM)` (perception.py:344)
  — a candidate is legal iff it lands ON `birth_loop` (or within
  `ISLAND_ATTACH_REACH_MM=0.2` of its boundary, perception.py:420) AND `|z-birth_z| ≤
  cfg.minima_connect_layers*cfg.layer_height_mm` (the exact z-window
  `island_birth_report` gates on, verify.py:253). **This guarantees a relocated birth
  contact still passes the very gate that demanded it** (verify.py:258-268).
- **minimum** (`engine._ensure_minima_contacts`, contact built at engine.py:378-380):
  also store `region_pts=m["region_pts"]` (already in scope from `m`, the minimum dict
  — `local_z_minima` returns it, perception.py:246-248). The birth region already
  encodes the "cfg-derived connection reach": `region_pts` is the bounded flood over
  vertices within `z_v + minima_connect_layers*layer_height_mm` (perception.py:195,
  222). Domain = points within `cfg.minima_contact_radius_mm` (verify.py:214) of any
  `region_pts` vertex. **This guarantees a relocated minimum still passes
  `minima_report`** (verify.py:206-216).
- **coverage / regular**: domain is the island's A2 lowest-vertex pool (defined
  above), not face centroids and not arbitrary surface samples. `cco.build`
  already has `mesh`;
  **pass `islands` into it** (see §2c). A relocation stays inside the island, so
  `coverage_report`'s worst-gap (verify.py:37-39) is re-satisfiable — but coverage is a
  *worst-point* gate, so relocation MUST re-run coverage on the moved set (it does, via
  the fix-pass loop, engine.py:271).

Interior **INTERNAL / phantom** contacts (78 of 195 in the audit;
`unrouted_mesh_classes.json` class_counts_195) have **no routable corridor in any
domain** — every candidate at every position fails clearance, so relocation declines
and (see §5) concession also declines (nothing routes even at 0.1mm). They stay
honestly unrouted and are removed by the parallel interior-phantom-filter workstream,
per the acceptance scope in §7. This feature never fabricates support for a phantom.

---

## 2. FEATURE A — Contact Relocation

### 2a. Two-pass structure inside `cco.build`

Add a **relocation pass after the main insertion loop** (cco.py:188-197), not inline,
so a relocated contact competes for corridors *after* every demanded contact has had
first claim (charter §4 insertion-order doctrine — correctness terminals claim
corridors first). New signature/return:

```
def build(mesh, contacts, tip_r, carried, minima_flags, plate_z, surface_tree, cfg,
          sample_points=None, sample_delicacy=None, query_fn=None,
          islands=None, delicacy_query=None):        # ← islands, delicacy_query NEW
    ...
    # (existing main loop unchanged: cco.py:188-197)
    relocations = []
    concessions = []
    for i in sorted(unrouted):                        # deterministic: unrouted is in append
                                                      # order here (cco.py:188-193), NOT yet
                                                      # sorted — iterate sorted() explicitly.
                                                      # It only becomes sorted at the return
                                                      # below; nothing may rely on sortedness
                                                      # of `unrouted` at this point.
        moved = _relocate_terminal(forest, i, contacts, tip_r, carried, stub_ends,
                                   cfg, plate_z, surface_tree, mesh, kdc,
                                   sample_points, sample_delicacy, query_fn, islands)
        if moved is not None:
            relocations.append(moved); continue
        conc = _concede_terminal(...)                 # Feature B, §5
        if conc is not None:
            concessions.append(conc); continue
        # else: truly unrouted, leave in `unrouted`
    unrouted = sorted(i for i in unrouted
                      if i not in {r["contact_idx"] for r in relocations}
                      and i not in {c["contact_idx"] for c in concessions})
    return forest, unrouted, relocations, concessions
```

### 2b. `_relocate_terminal` — search, sampling, scoring

**Candidate positions (radius + sampling).** Enumerate the legal domain (§1) as a set
of surface points, each with a normal and (for regular contacts) owning `face_id`:

- birth: source-face centers from the contact's own island, filtered by BOTH
  `island_birth_xy_ok(..., birth_loop, ISLAND_ATTACH_REACH_MM)` and
  `|z-birth_z| <= minima_connect_layers*layer_height_mm`. Birth is tested first
  when both production flags are present.
- minimum: source-face centers from the contact's own island whose batched
  nearest distance to `region_pts` is at most
  `cfg.minima_contact_radius_mm`.
- coverage: the A2 lowest-vertex pool for `islands[iid]`, carrying the
  most-negative-`nz` nominating face's `face_id` and exact face normal.

**Radius / cap.** No fixed metric radius — the *domain* is the bound (that is the
inviolable constraint). Add only an economy cap `cfg.reloc_max_candidates` (default 24)
and order by **ascending distance from the original demanded xy** (stable, index
tiebreak). The original position is always excluded (it already failed). This keeps the
moved contact as close to the demanded spot as the domain allows while bounding cost.

**Trial mechanism (mirrors `_insert_terminal`).** For each candidate `(pos, normal)`
in order:
1. `cp = forest.checkpoint()` (tree.py:474);
2. retire the old stub: `forest.mark_dead(forest.terminal_of[i])` (tree.py:269) — the
   stub built at the demanded position at cco.py:173-178 must not survive as live
   geometry once the contact moves (`engine.live_member_lists` excludes dead,
   engine.py:99-100);
3. build a **new stub** at `pos`/`normal` exactly as cco.py:169-178, register the new
   `terminal_of[i]`, refresh `stub_ends[i]`;
4. run each candidate route family against the new stub end using the **same
   transactional trial pattern** `_insert_terminal` uses (checkpoint → build_fn →
   `forest.revalidate` → rollback, cco.py:280-306), but **re-implemented here for
   route-length scoring** — do **not** call `_insert_terminal` itself: it both scores AND
   commits the survivor by **volume** (cco.py:292,297-305), which contradicts the
   route-length objective below. Reuse only the candidate builders
   (`_candidate_builders`, cco.py:316-323) and the revalidate/rollback trial mechanics;
5. if a candidate succeeds, **score = route length** = Σ|p1−p0| over the members this
   terminal now owns (`owner==("contact",i)`, its stub→root chain) — the charter's cost
   proxy is volume, but the spec pins scoring to route length as instructed; shorter route
   = less whip, less resin, closer descent. Record `(score, candidate)` then
   `forest.rollback(cp)`; if it fails, `forest.rollback(cp)` and continue.
6. After the loop, if any candidate scored, **re-apply the min-score one** (stable
   tiebreak = candidate order = nearest-to-demanded). **Before committing, guard neighbor
   birth coverage (Open-Risk #8):** confirm the moved set does not un-cover any *other*
   island birth this contact incidentally held (ladder placements share footprints,
   engine.py:450-478) — `uncovered_island_births` is a hard failure with no re-pass
   (engine.py:298). If the move would drop a neighbor birth, reject that candidate and fall
   to the next scored one (or leave the contact unrouted). Then commit, and:
   - **mutate the contact dict in place**: for a regular contact update
     `position`, `face_id`, and the owning face `normal` together in one schema
     write; specialized minimum/birth contacts update only the fields their
     existing domains define. Keep a `c["relocated_from"]=old_xy` breadcrumb. Because
     `engine.construct`/`run` hold the *same* dict objects (`pre["contacts"]`,
     engine.py:81, 248), verify's `routed_contacts` (engine.py:270) sees the moved
     position automatically — the gates re-evaluate against where support actually is.
   - return `dict(contact_idx=i, cls=<class>, from_xy=old_xy, to_xy=pos[:2],
     moved_mm=‖pos-old‖, route_len_mm=score)`.

`_relocate_terminal` returns `None` if no candidate routed.

### 2c. Wiring `islands` + `delicacy_query` through the engine

`engine.construct` (engine.py:75) calls `cco.build` at engine.py:85-88 — add
`islands=pre["islands"]` and a delicacy query.

**Build the delicacy query inside `construct`, do NOT thread a closure through `pre`.**
`prepare` is documented to return **"a picklable dict (no KDTree)"** (engine.py:19-25)
because Treant-scale runs stage/pickle `pre`. The `query` closure returned as the 4th
element of `perception.delicacy_field(mesh,cfg)` (perception.py:558-563) captures a
`cKDTree`, so putting it in `pre` (or otherwise threading it through the picklable
boundary) would **break staged/pickled runs**. Instead, `prepare` keeps storing the plain
arrays `sample_points`/`sample_delicacy` (engine.py:68, already picklable), and
`construct` **rebuilds the `cKDTree` locally** from `pre["sample_points"]` /
`pre["sample_delicacy"]` and passes the resulting `delicacy_query` into `cco.build`. This
keeps `pre` picklable and the query live only inside `construct`, where the tree is cheap
to reconstruct once per run. (The closure form is acceptable ONLY for the non-staged
in-process path; the rebuild-from-arrays form is the required default.) `construct`
returns the new `relocations`/`concessions` alongside `(forest, unrouted, …)`
(engine.py:90) up into `run` (engine.py:249) for logging.

### 2d. Report line (Feature A)

Engine writes to `log["reach"]`:
```
reach.relocations = dict(
    n=<len>, by_class=dict(birth=…, minima=…, coverage=…),
    still_unrouted_after_reloc=<count>,
    sites=[{contact_idx, cls, from_xy, to_xy, moved_mm, route_len_mm}, …])
```
Top-line log string: `"reach: relocated N contacts (Nb birth / Nm minima / Nc
coverage); M still unrouted → concession pass"`.

---

## 3. FEATURE B — Concession Supports: the invariant that is NOT weakened

Craig 2026-07-23: *slight model contact acceptable, "fixed in post" by buffing/filling.*
The concession spends the **clearance safety margin** (`standoff`), never the material:
the ladder floor is `> 0`, so a conceded member still keeps a real (buffable) gap to the
model and never physically intersects it.

**The clearance gate is not weakened.** Instead, each conceded member carries an
explicit, smaller **standoff** that the clearance checkers honor *per member*: the
conceded member is checked against its **conceded envelope**, everything else against
the **full** envelope. `verify.ok`'s *form* is unchanged (§6) — a conceded member
simply does not appear in `clearance_violations` because it genuinely clears its own
(recorded) envelope.

**The reduced standoff is SPATIALLY LOCALIZED, not applied to the whole member.**
Craig authorized *slight model contact fixed in post* for the **contact patch / tight
groove** — NOT a blanket margin cut along a member's entire descent past unrelated model
features (a full-height plate-descent column, cco.py:369, can sweep past geometry
midway that has nothing to do with the tight groove at its foot). So the reduced standoff
applies **only to swept samples within an arc-length reach of the member's model-proximal
end** — exactly the way the existing embed-end clearance inset is arc-length-bounded to
the anchor region (tree.py:378-383), which this deliberately mirrors. Samples farther than
`cfg.concession_reach_mm` (default **1.5mm**, arc length) from that end keep the **full**
`cfg.standoff_mm`, so a real intrusion caused by unrelated geometry midway along the
member is still a hard `clearance_violation` — it is NOT masked. Only the localized groove
passage the concession was granted for is relaxed.

The **model-proximal end** (where arc length is measured from) is the member's `embed_end`
when set (strut foot / model_rung / concession tip that lands on the model, §5); for a
plain terminal-descent concession with no embed it is the terminal-contact end — the stub
end at the demanded `position` (`p0` for terminal stubs, cco.py:169-178). No extra Member
slot is needed: the end is derivable from `embed_end` + owner, and the reach is a global
`cfg` knob.

### 3a. One new Member field: `conceded_standoff_mm`

Add slot to `Member.__slots__` (tree.py:51-53), default `None` in `__init__`
(tree.py:55-68), include in `_snapshot`/`_restore` (tree.py:124-132) so
checkpoint/rollback preserve it, and in `__getstate__` (already dumps all slots,
tree.py:75).

**Certificate coupling (critical).** `conceded_standoff_mm` changes the clearance
result, so a change MUST void the member's certificate exactly like a radius/geometry
change (tree.py `__setattr__`, tree.py:81). Add it to the tracked handling: on set,
`_touch_member(idx)` (snapshot for rollback) **and bump `geom_version`** (voids
`cert_valid`, tree.py:118-122) so `revalidate` re-checks the member under the new
standoff. Do **not** reset `cert_radius` to -1 (that hatch is position-only,
tree.py:94-95); the "checked-fat-covers-thin" radius rule is orthogonal to standoff.
Simplest correct rule: treat a standoff *decrease* the same as a radius *increase* —
void the cert, force a re-check.

No separate `forest.concessions` dict is needed: the concession state lives on the
member, so checkpoint/rollback (tree.py:481-500) and pickling already carry it. The
`concessions[]` report (§6) is *derived* by scanning live members whose
`conceded_standoff_mm is not None`.

### 3b. Per-member standoff in BOTH clearance implementations

Charter §11 keeps clearance non-negotiable, and `verify.clearance_report` is a
**deliberate independent duplicate** of `tree.Forest.revalidate` ("a builder bug that
mis-certifies a member must not also fool the verify gate", verify.py:44-51). **Both
must learn about concessions — independently — and neither may call the other.** The
shared primitive stays the low-level `routing` utilities only (as today).

- `tree.Forest.revalidate` (tree.py:281): today `clearance = dist - r_all -
  cfg.standoff_mm` (tree.py:401). Build a **per-sample standoff vector** `standoff_all`
  concatenated exactly like `all_r` (tree.py:389). For each swept sample of member `mi`:
  default `s = cfg.standoff_mm`; **only if `m.conceded_standoff_mm is not None` AND the
  sample's arc length from the member's model-proximal end (§3 model-proximal
  end: `embed_end` end, else the terminal-contact end) is `≤ cfg.concession_reach_mm`** do we use
  `s = m.conceded_standoff_mm`. All other samples of a conceded member keep the full
  `cfg.standoff_mm`. Then `clearance = dist - r_all - standoff_all`. The embed-end inset
  (tree.py:379,382) — itself arc-length-bounded to the same anchor region — uses that
  member's own conceded `s` on the inset samples for consistency. The prune upper-bound
  `ub = r_all.max()+cfg.standoff_mm` (tree.py:398) stays on the **full** standoff — it
  is a safe over-estimate (a conceded sample needs *less* clearance, so the wider `ub`
  never prunes a point that could be a real violation). **Use `is not None`, never `or`**
  (a stamped 0.0 must still register as conceded, though the ladder floor forbids it — see
  the assert in §5.3).
- `verify.clearance_report` (verify.py:44): identical change at verify.py:130
  (`clearance = dist - r_all - cfg.standoff_mm`), insets at verify.py:111,114, ub at
  verify.py:128 — including the **same arc-length localization** (reduced `s` only within
  `cfg.concession_reach_mm` of the model-proximal end; full `cfg.standoff_mm` elsewhere).
  **But** `clearance_report` runs `members = joints.collect_members(members)`
  (verify.py:90) and thereafter reads **dicts** (`m["kind"]`, `m.get("embed_end")`).
  So `conceded_standoff_mm` must survive `collect_members` into the member dict:
  extend the member schema `{p0,p1,r0,r1,owner,kind,embed_end}` (charter §2) with
  `conceded_standoff_mm` in `joints.collect_members` **[verify against joints.py — the
  one file this spec did not read; the change is "copy the field through", same as
  `embed_end` at charter §2]**. Then read it with the **same `is not None` test the tree
  side uses** — `s = m["conceded_standoff_mm"] if m.get("conceded_standoff_mm") is not None
  else cfg.standoff_mm` — **not `or`** (the two checkers must not diverge at a stamped 0.0;
  `or` would silently promote 0.0 back to 0.6 while the tree honored the 0.0, breaking the
  "floor > 0" load-bearing invariant if 0.0 ever reached a member). `embed_end`/owner give
  the same model-proximal end here, so the localization is computed identically and
  independently on both sides.
  This keeps the two checkers genuinely independent (each reads the field and does its
  own arithmetic); only the field crosses the boundary, never the exemption logic.
  **Confirmed against joints.py:** `collect_members` hard-lists its output dict
  `dict(p0,p1,r0,r1,owner,kind,embed_end)` (joints.py:428-429) and drops any other
  field, so this is a required (small) edit — add
  `conceded_standoff_mm=_field(m,"conceded_standoff_mm", default=None)` and include it in
  the emitted dict, exactly mirroring how `embed_end` is read at joints.py:426. Plain-dict
  fixtures that omit it default to `None` (full standoff), unchanged behavior.

Because a conceded member clears its own envelope, `clearance_report` returns it
clean → it is absent from `clearance_violations` → `self_verify.ok` (verify.py:469)
is unchanged in form. Everything non-conceded is still checked at the full 0.6mm.

### 3c. The standoff ladder

`cfg.concession_standoff_ladder_mm: tuple = (0.3, 0.1)` — tried **largest-first**
(mildest concession wins). Full 0.6mm already failed (that is why we are here); the
ladder is strictly *below* `cfg.standoff_mm`, floored at 0.1mm `> 0` so no member ever
truly touches the model. The first rung that yields a clean insertion is taken and the
member is stamped with that `conceded_standoff_mm`; if none clears even at 0.1mm, the
contact stays unrouted (an INTERNAL/phantom, or a genuinely impossible spot).

---

## 4. FEATURE B — gates BEFORE a concession is allowed

`_concede_terminal(forest, i, contacts, …, surface_tree, mesh, delicacy_query, cfg)` runs
only after §2 relocation returned `None`, and only if ALL of:

1. **`cfg.concessions_enabled`** (default `True`, per Craig). If off, decline → contact
   stays unrouted.
2. **Real-risk: no live member within 2mm.** Compute the nearest *live support member*
   surface distance to the demanded contact position over the current forest
   (`engine.live_member_lists`-style live set: exclude `forest.dead`, exclude other
   terminals' unrouted stubs; the audit's `nearest_member_surface_mm` field is the exact
   analogue, `unrouted_mesh_classes.json`). If `nearest_member_surface_mm ≤
   cfg.concession_min_hold_mm` (default **2.0**) the spot is **already held by a
   neighbor** → **never concede** (Craig's non-negotiable: "never concede for spots
   already held by a neighbor ≤2mm"). This is the same predicate as the audit's
   EFFECTIVELY-HELD/WEAK redundancy bands — a WEAK(1-2mm) or HELD(≤0.5mm) contact is
   *not* a concession candidate; only UNSUPPORTED(>2mm) is (audit
   class_x_redundancy_195: the 50 GROOVE-BLOCKED|UNSUPPORTED + 13 OPEN|UNSUPPORTED are
   the real concession pool).
3. **Not delicate.** The engine *has* a delicacy field (`perception.delicacy_field`,
   perception.py:474; query closure perception.py:558). If
   `delicacy_query(contact_position) > cfg.concession_max_delicacy` (default **0.5**,
   matching `struts.find_model_anchor_candidates`' own `max_delicacy=0.5` precedent,
   struts.py:23,65) → **forbid** (Craig's non-negotiable: concessions must not touch
   DELICATE surfaces). Also query along the conceded member's near-surface envelope
   samples, not just the tip: if any sample within `(r + conceded_standoff)` of the
   surface maps to a delicate patch, decline that rung. A fin/tendril is never sacrificed
   to buffing.

If any gate fails, `_concede_terminal` returns `None` and the contact stays unrouted.

---

## 5. FEATURE B — the concession insertion itself

The original stub for `i` (at the demanded position, cco.py:173-178) **still exists**
(relocation rolled back on failure, §2b), so concession reuses it. For each ladder rung
`s` in `cfg.concession_standoff_ladder_mm` (0.3 then 0.1):

1. `cp = forest.checkpoint()`;
2. run the ordinary candidate builders (`_candidate_builders`, cco.py:316) — plate
   descent, model anchor, bifurcation — via the same trial pattern;
3. **before each `forest.revalidate`, stamp `conceded_standoff_mm = s` on the
   newly-added members owned by this terminal only** — members with
   `owner==("contact", i)` created since `cp`. **Assert `s > 0` at the stamp site**
   (`assert s > 0, "concession standoff floor is > 0"`) — the ladder is `(0.3, 0.1)`, but
   the assert makes the "no member ever truly touches the model" invariant impossible to
   violate by construction even if the ladder is misconfigured, and it guarantees the
   `is not None` reads in §3b never honor a load-bearing-zero standoff. The stamp records
   only the scalar; the **arc-length localization is not a per-member field** — both
   checkers derive the reach window from `cfg.concession_reach_mm` + the member's
   model-proximal end at check time (§3b), so a conceded member's reduced standoff already
   applies **only near its tight-groove end**, never along its full descent. **Murray-fattened
   ancestors are NOT conceded** (they keep full 0.6mm): concession is licensed for the new
   member's own tight-groove passage, never to weaken pre-existing structure. If fattening
   an ancestor now violates at full standoff, the candidate is correctly rejected;
4. `forest.revalidate` (tree.py:281) now checks the terminal's members at `s`, everyone
   else at 0.6 — cheapest surviving candidate wins by route length (§2b scoring);
5. first rung that yields a clean commit is accepted: the terminal's members keep
   `conceded_standoff_mm = s`; record and return the concession (§6). Otherwise roll back
   and try the next rung; exhausting the ladder → return `None` (stays unrouted).

**Tip/foot contact against the model uses the `model_rung` precedent.** When the winning
candidate is a member whose end deliberately lands on the model (a model-anchor strut
foot, `embed_end="p1"`, cco.py:442-444; or a concession tip touching the model), it
reuses the existing designed-embed machinery exactly as `bracing._find_model_rung_anchors`
/ `_place_one_rung` do for `kind="model_rung"` (bracing.py:748, 907-914):
- `embed_end` set so the **embed-end clearance inset** (tree.py:378-383,
  verify.py:110-115) exempts the intentional touch point from being a false violation —
  the *approach* is what the reduced standoff licenses, the *touch* is already licensed by
  the embed inset;
- if the concession member is near-horizontal against the model, the `model_rung`
  length-capped printability exemption (tree.py:573, `kind=="model_rung"` only) applies —
  concession members that are horizontal model rungs are built as `kind="model_rung"`,
  never weakening the printability gate for any other kind.

### 5a. `est_contact_area_mm2`, `min_gap_mm`

Computed at record time from the committed conceded member vs the surface:
- `min_gap_mm` = the member's worst `dist - r` over its swept samples (the true nearest
  approach to the model, i.e. clearance measured at standoff 0). By construction
  `min_gap_mm ≥ s ≥ 0.1 > 0` — proof the member does not intersect the model.
- `est_contact_area_mm2` = the "at-risk" surface patch a human will buff: the area where
  clearance sits between the conceded and full envelope. Estimate as the count of
  `surface_tree` points within `(r_local + cfg.standoff_mm)` of the member axis but
  *outside* `(r_local + s)`, times `pitch²` (the tree's own pitch, `build_surface_tree`
  routing.py:116). For a pure tip/foot touch this collapses to `≈ π·r_contact²`. This is
  a report figure, not a gate.

---

## 6. Verify wiring + the `concessions[]` report section

`self_verify` (verify.py:391) gains an additive, derived section — **no gate logic
changes**:

- New helper `verify.concession_report(forest, mesh, surface_tree, cfg)`: scan live
  members (dead-excluded) with `conceded_standoff_mm is not None`; for each emit
  `{contact_idx (from owner ("contact",i), verify.py member.owner), member_idx,
  conceded_standoff_mm, min_gap_mm, est_contact_area_mm2, position}` (§5a). Deterministic
  order: sort by `(contact_idx, member_idx)`.
- `self_verify` computes it when `forest is not None` (same optional-forest pattern as
  printability/murray, verify.py:459-462) and adds `concessions=<list>` to the returned
  dict (verify.py:475-487).
- **`ok` is unchanged in form** (verify.py:469-472): the boolean is still `len(uncovered)
  == 0 and len(clearance)==0 and … and len(murray)==0`. Conceded members are absent from
  `clearance_violations` because they clear their own recorded envelope (§3b);
  `concessions[]` is *reporting only*, exactly like `removal_report` (verify.py:485,
  charter §10 amendment). It never ANDs into `ok`.

Engine surfaces it: `log["reach"]["concessions"]` = the `concessions[]` list plus a
summary — `"concessions: N sites (0.3mm×a, 0.1mm×b); total est contact area A mm² —
buff/fill in post; sites: [contact_idx…]"` — so Craig knows precisely where post-work is
expected before he opens Chitubox.

**Disable knob (Craig: default ON).** `cfg.concessions_enabled` (default `True`). Off →
`_concede_terminal` short-circuits at gate §4.1; no member is ever stamped; the
`concessions[]` section is empty; contacts that would have been conceded stay honestly
unrouted (and thus re-trip their demand gate, visibly).

---

## 7. Config knobs (config.py, add near standoff config.py:116 / delicacy config.py:329)

```
# ---- Phase-8 reach: relocation ----
p8_reseat_max_seeds: int = 24           # economy cap; ordered nearest-to-demanded
# ---- A2 regular-contact pool ----
contact_pool_max_refine_depth: int = 8  # allocation guard; refusal, not physics
contact_pool_max_virtual_cells: int = 750_000
contact_pool_max_candidates: int = 2_250_000  # conservative 3*cells cap
contact_pool_max_compact_payload_bytes: int = 144 * 1024 * 1024  # retained SOA only
# ---- Phase-8 reach: concessions (Craig-authorized 2026-07-23) ----
concessions_enabled: bool = True                       # default ON per Craig
concession_standoff_ladder_mm: tuple = (0.3, 0.1)      # tried largest-first; floor > 0
concession_reach_mm: float = 1.5                       # arc-length window from model-proximal
                                                       # end within which the reduced standoff
                                                       # applies; full standoff beyond it
concession_min_hold_mm: float = 2.0                    # already-held ⇒ never concede
concession_max_delicacy: float = 0.5                   # DELICATE ⇒ never concede
```
All are new tunables; `concession_*` are the concession safety envelope. The ladder floor
0.1 > 0 is the load-bearing invariant that no member ever truly intersects the model.
(Per config.py's doctrine, add a `notes()` flag noting these await first-print
calibration, config.py:399.)

---

## 8. Acceptance target

Full-scale **Weeping Treant** run (`engine.run`, `SupportConfig(lift_mm=3.0)`, the audit
harness's config, `unrouted_mesh_classes.json` meta):

- **hard gates == 0**: `clearance_violations`, `connectivity_violations`,
  `load_violations`, `printability_violations`, `murray_violations` all empty
  (verify.py:469-472) — including every conceded member, checked against its own
  envelope;
- **`unsupported_minima == 0`** and **`uncovered_island_births == 0`** among **NON-phantom
  spots** — the ~78 INTERNAL / interior-phantom contacts (audit class_counts_195) are
  out of scope for this feature and are removed by the parallel interior-phantom-filter
  workstream; this spec must not fabricate support for an enclosed spot (relocation finds
  no legal corridor; concession finds no clean rung even at 0.1mm — both correctly
  decline);
- the real concession pool this must resolve: the **13 OPEN-BUT-UNREACHED unheld** (audit
  three_answers.a) via relocation first, and the **GROOVE-BLOCKED unheld** (≈50
  UNSUPPORTED + marginal, audit class_x_redundancy_195) via concession — every conceded
  site listed in `concessions[]` so Craig knows the buff/fill map before printing.

---

## 9. Touch-point summary (every hook, with cite)

| File | Symbol / line | Change |
|---|---|---|
| cco.py | `build` cco.py:157,188-197 | two-pass: relocation then concession before parking `unrouted`; return `relocations`, `concessions` |
| cco.py | new `_relocate_terminal`, `_concede_terminal` | §2b, §5 — reuse the `_candidate_builders` families (cco.py:316) + the checkpoint/revalidate/rollback trial mechanics, but **re-implement the trial loop for route-length scoring**; do NOT call `_insert_terminal` (it scores/commits by volume, cco.py:292,297-305) |
| cco.py | stub build cco.py:165-178 | reused to rebuild stub at relocated position (with `mark_dead` of old) |
| tree.py | `Member.__slots__`/`__init__` tree.py:51-68 | add `conceded_standoff_mm=None` |
| tree.py | `Member.__setattr__` tree.py:81-116 | standoff decrease voids cert (bump geom_version, snapshot) |
| tree.py | `_snapshot`/`_restore` tree.py:124-132 | carry the field |
| tree.py | `Forest.revalidate` tree.py:281,398-401 | per-**sample** standoff vector, arc-length-localized to `cfg.concession_reach_mm` of the model-proximal end; inset uses member's `s`; `is not None` (never `or`) |
| tree.py | `Forest.mark_dead` tree.py:269 | retire old stub on relocation |
| verify.py | `clearance_report` verify.py:44,90,111,114,128,130 | **independent** per-sample standoff via collected `conceded_standoff_mm`; same arc-length localization; `is not None` (never `or`) |
| transient.py | `resize_forest` transient.py:625 (engine.py:148-150,169) | **radius-lock conceded members** — never up-size a member with `conceded_standoff_mm is not None`, else the engine.py:169 assert crashes the whole run |
| verify.py | new `concession_report` + `self_verify` verify.py:391,459-462,475-487 | derived report; `ok` form unchanged verify.py:469 |
| joints.py | `collect_members` joints.py:426,428-429 | **required edit** — add `conceded_standoff_mm` to the read + emitted dict, like `embed_end` (verified: output dict is hard-listed, drops other fields) |
| engine.py | `construct` engine.py:75-90 (cco.build call 85-88) | pass `islands`, `delicacy_query`; thread `relocations`/`concessions` |
| engine.py | `_ensure_minima_contacts` 378-380; `_ensure_island_birth_contacts` 568-569 | attach `region_pts` / `birth_loop`,`birth_z` to contacts |
| engine.py | fix-pass loop 247-308, `run` return 312-316 | log `reach.*`; surface `concessions[]` |
| config.py | after config.py:116 / :332 | 7 new knobs (§7, incl. `concession_reach_mm`) + `notes()` flags |
| bracing.py | `model_rung` precedent bracing.py:748,907-914; printability exemption tree.py:573 | reused, not modified — concession tip/foot follows it |

---

## 10. Open risks

1. **`joints.collect_members` propagation (RESOLVED).** Confirmed against joints.py:428-429:
   the output dict is hard-listed `{p0,p1,r0,r1,owner,kind,embed_end}` and silently drops
   any other field, so `verify.clearance_report`'s per-member standoff would read `None`
   (→ full 0.6mm) for a conceded member and **falsely flag it** unless `collect_members`
   is extended to carry `conceded_standoff_mm` (§3b). This is now a listed required edit,
   not an assumption — but it is the single highest-leverage line: get it wrong and the
   independent verify gate rejects every concession the builder just made.
2. **Reference-checkout drift.** Citations are against the most-advanced readable
   checkout, used as the f02b7af proxy. If f02b7af is behind/ahead, re-anchor by
   symbol. `routing.py` at 320 lines has no demand/relocation logic (it is the
   Segment/Route + clearance substrate, charter §2 "KEEP UNCHANGED") — Feature A's true
   seam is `cco.build`, not `routing.py`; the task named routing.py but the failure
   actually surfaces in cco. Called out so no one hunts for relocation in routing.py.
3. **Coverage relocation vs the fix-pass loop.** Relocating a coverage contact changes
   `coverage_report`'s worst-point (verify.py:37); the engine already re-verifies each
   fix pass (`engine.run`), but a relocation that *improves* one island's worst point
   could nominally shift another face — bounded because the move stays *inside the same
   island*. Watch on the Treant that relocation does not thrash the ≤2-pass fix budget
   (engine.py:246 `max_fix_passes=2`).
4. **Concession + Murray propagation.** A conceded terminal that bifurcates fattens
   ancestors (cco `_propagate_murray` cco.py:618); ancestors keep full 0.6mm standoff, so
   a concession that only clears because an *ancestor* was also tight is correctly rejected
   (§5.3). A conceded member is `revalidate`-clean at its own (localized) standoff, so
   `brace_and_size`'s re-fix assert (engine.py:169), which calls the now-standoff-aware
   `forest.revalidate`, stays satisfied for a geometrically-unchanged conceded member.
   **REQUIRED — resize-up of the conceded member itself (must-fix, not just watch).**
   `transient.resize_forest` up-sizes members (engine.py:148-150 counts `n_resized_up`;
   transient.py:625). If it fattens a **conceded** member's radius past what its reduced
   (localized) envelope can clear, the subsequent `forest.revalidate` returns a violation
   and the **hard assert at engine.py:169 fires — crashing the ENTIRE Treant run**, not
   merely dropping that one contact. `resize_forest` MUST therefore treat a member with
   `conceded_standoff_mm is not None` as **radius-locked**: do not up-size it (skip it in
   the resize-up pass, or clamp its target radius to the value that keeps its conceded
   envelope `revalidate`-clean). The concession chose the largest standoff and route that
   cleared at its committed radius; growing that radius afterward is never licensed. With
   the lock, the engine.py:169 assert cannot fire on a conceded member. Confirm with a
   fixture that a resize pass over a conceded member does not up-size it and the run does
   not crash.
5. **`est_contact_area_mm2` is an estimate.** It rides the `surface_tree` point density
   (routing.py:116) and is a buff-map heuristic, not a gate — do not let anyone treat it
   as a tolerance. Flagged rough, like the removal params (charter §10).
6. **Determinism of the delicacy query.** `delicacy_field` seeds `np.random.seed(seed)`
   (perception.py:494) so the field is reproducible, but confirm the *same* field object
   is threaded (not rebuilt per contact) so concession decisions are stable across a run.
7. **Standoff-void-cert subtlety.** Making a standoff *decrease* void the certificate
   (§3a) is essential; missing it would let a stale "checked at 0.6" cert wave a member
   through at 0.3. The `test_cco.py` certificate test (charter §13, "a resize without
   recheck must be IMPOSSIBLE by construction") should gain a standoff-change sibling.
8. **Relocation must not un-cover a NEIGHBORING birth (no retry exists for it).** The
   fix-pass loop only continues on `uncovered_islands` (coverage, engine.py:298);
   `uncovered_island_births` is a hard `ok=False` failure with **no re-pass**. Because
   ladder placements share footprints (`engine._ensure_island_birth_contacts`),
   relocating a contact that
   incidentally also held a *different* island birth can drop that birth's coverage and end
   the run `ok=False` with no recovery. **Constraint: a relocation MUST NOT reduce coverage
   of any island birth other than the one it is repairing.** Before committing a relocation
   (§2b step 6), re-check that every *other* island-birth demand still routed/held remains
   held under the moved set; if the move would un-cover a neighbor birth, reject that
   candidate and fall through to the next (or leave the contact unrouted). Verify on the
   Treant that no birth loses coverage across the relocation pass.
