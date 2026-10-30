# RETHINK: contact-size and spacing limits by construction (outside review, craig-core:fable, 2026-10-28)
Read-only at develop db93198b, wt-cr1 63c6e8dd, and the validator/ evidence. No agents, no builds, no model runs.
Paths: `S/` = crates/plinth-sweep/src, `P/` = crates/plinth-support/src, `E/` = crates/plinth-emit/src, `C/` = crates/plinth-core/src, `V/` = this folder.

## 0. Verdict in five lines
1. The discs' CS is arithmetic, not a bug: the seat pitch comes from REACH only and the load-sized tip is uncapped. The registry's own note states 11.79 mm² per seat (`C/constants/evidence.rs:491`); the cap allows 9.05 mm² (18·π·0.4²); so r = 0.456 and the patch is 0.91 mm. The run log shows 0.883 mm patches (`V/upf-followups/runs/Round_50/run.log:367-377`).
2. Every cap attempt (pcap 1-8, CBT, cr1) kept that under-dense field and then SPLIT hosts with `ceil(routed/limit)` (`S/down_sweep.rs:4834-4845`). Splitting cannot make a lattice; it makes 340 and 466 arms where 223 are needed and 24 down passes where 4 ran (`V/cr1/HANDBACK.md`, `V/cr1/Round_50-stage-deltas.tsv`).
3. ChituBox never has this problem because its contact is an INPUT (a sphere of fixed diameter at fixed depth along the normal) and its pillars are built afterwards and never move the tip. Plinth's contact is an OUTPUT of a loop (reach pitch → Voronoi load → radius → seat law → lowering → trim) followed by about fifteen passes that move it.
4. The fix is to make the contact a primitive again while keeping the physics: pitch from capacity, radius load-sized but capped, approach along the normal, reservation at creation, and NOTHING after creation may move a contact. The discs need about thirty lines of this; Deinonychus needs the constructor and the deletion of the movers.
5. The up/down split and nearest-carrier routing are NOT the problem; they are the physics (a contact's Voronoi cell IS its supported area, memory `support-arm-load-physics`). The pass count is a symptom.

## 1. Root cause (Q1)
### 1.1 What the owner's ChituBox profile actually does (`Chitubox Projects/chitubox-support-specs.cfgx`)
- Heavy: touch tip = SPHERE Ø0.8 mm at depth 0.3; tip cone Ø0.6→1.5, length 3.0, ALONG THE NORMAL; pillar Ø1.5 after. Middle: Ø0.5 at 0.2; Light: Ø0.3 at 0.1.
- A sphere of diameter D seated d deep cuts a chord 2·√(d(D−d)) whatever the face angle: Heavy 0.775, Middle 0.49, Light 0.28 mm. The limit holds by GEOMETRY, with no flank term, because the cone leaves along the normal.
- Spacing is a density slider on a fixed D; two tips at pitch > D cannot fuse. No load is computed, so there is no feedback loop to converge; failures are the user's eye. Plinth's whole advantage is that loop; the mistake was letting the loop OWN the contact.

### 1.2 Plinth's contact is derived, late, and then edited
| Fact | Anchor |
|---|---|
| Seat pitch = √3·(reach − p/√2) = 3.63 mm from reach alone; the lattice and the greedy both use it | `S/up_sweep.rs:718, 1278, 3725-3727` |
| Hex Voronoi cell 11.79 mm² per seat is written in the registry; Q at the cap is 18π·0.4² = 9.05 | `C/constants/evidence.rs:491`; `V/CAP-DESIGN.md` §1 |
| Radius r = √(L/18π) clamped to the LEGACY 1.0 mm max, not to the 0.4 cap | `S/down_sweep.rs:4998-5007`; `S/lib.rs:1408` |
| Seat law depth = max(allowance, h + r·sinθ, 0.382·r); on a flat face the patch is exactly 2r, on an oblique face the FLANK term adds the shaft's slant | `P/contact.rs:442-460`; `P/lower.rs:3495-3607` |
| Default approach is the vertical plain descent; a head only when the straight drop was REFUSED (`Blocked`) and only on a steep face (`NotSteep`) | `S/up_sweep.rs:5345-5353`; `S/heads.rs:1489-1493` |
| Twin raise and fusion "keep both" happen BEFORE any head is planned; insertion siblings are headless with zero normal and no seat face | `S/up_sweep.rs:2883-2943`; `S/up_sweep/fusion.rs:889-915`; `S/down_sweep.rs:4890-4927` |
| Contact movers after creation: lip seats A/B, graze, head-join relocation, late graze, growth ×2, footprint slides, head rebuild, normal rewrite, feet sizing, hairs/pins, bore relocation | `S/lib.rs:3984, 4066, 4108, 4159, 4565, 5338, 5437, 5543, 5577, 5620, 5647, 5681, 5747` |
| The slenderness hand-down writes every slot up to the contact slot (necking's opposite: it widens INTO the contact) | `S/slenderness.rs:7107-7113` |
| The written-file reader is the only truth; `RealPatch` has no area | `E/real_contact.rs:7-33, 86-107` |

- Discs (Round_25/50/100: CS 32/126/593, FU 0): pure consequence of row 1 + row 3. No late pass is involved ("descent last changed by: up-sweep and down-sweep" on every CS line of the log).
- Deinonychus (CS 12, FU 13, TR 49; `V/cbt0c/deinonychus.log:656-666`): 9 of 13 FU are twin/host or sibling pairs born at creation (`V/CONSTRUCTION-CHALLENGE.md` C5); 17 of 48 TR are real cone strips from the vertical approach on oblique faces (`V/RESULTS-apc.md` §A); cr1's inserted contacts were later SLID 0.30-0.35 mm into each other (`V/cr1/HANDBACK.md`). So Deinonychus is rows 5-7: no approach at creation, siblings without seat facts, and movers.

### 1.3 Why eight pcap rounds, CBT and cr1 could not converge
- They all took the 198-arm reach-pitch field as given and relieved overload per HOST (`wanted = ceil(routed/limit)`, hex spacing from the host's footprint, `S/down_sweep.rs:4834-4887`). A host at 1.1×Q gets a second arm; the neighbours re-route by Voronoi; new hosts overload; repeat. That is the 4→24 down passes and 198→340→466 arms. The physics minimum for a lattice is 216-223.
- CBT 0c found 0 feasible of 88 because every candidate was judged against structure (SL, LG, HS) that had been built on top of the contacts it was trying to move (`V/cbt0c/certified-finalists.tsv`). Moving a contact under a finished tree is the wrong order; ChituBox never does it.
- cr1 reserved patches at insertion and then let the footprint slides move them. A reservation that a later pass may break is not a construction rule.

## 2. The smallest architecture that holds the rules by construction (Q2)
**One principle:** a model contact is an APPEND-ONLY, IMMUTABLE primitive, created with its capacity, its patch, its reservation, its approach axis and a straight stub to a point K. Everything structural is built BELOW K. Nothing after creation may move, grow, re-seat or re-aim a contact; the only later operations are "add a contact through the same door" and "remove a member".

### 2.1 Data flow and stages
| Stage | Creates | Invariant it may not break |
|---|---|---|
| A. Slices, reach, demand (unchanged: `S/reach.rs`, `S/up_sweep.rs` potential sweep, `root_and_clear` 3565-3589) | demand cells per layer | creates no geometry |
| B. CONTACT FIELD (new `S/contact_field.rs`, replaces the seat→descend half of `commit_seat` 2846-3420 and all movers) | contacts: face point, outward normal, axis, r ≤ r_cap, planned patch, reservation disc, stub [seat, K] | planned patch ≤ 0.8 − rounding via the PRODUCTION seat law at the chosen axis; reservation disjoint from every admitted one (feet and lower layers included) by > LINK + rounding; r ∈ [r_min, r_cap]; cone overlap ranked zero-first |
| C. Routing (unchanged `S/down_sweep.rs` route + `S/per_event.rs`) | L per contact | read-only on contacts |
| D. Split wave (constructor again, ONE wave) | extra contacts in overloaded Voronoi regions, at capacity pitch | same door as B; never host-centred `ceil` |
| E. Tip sizing | r = clamp(√(L/18π), r_min, r_cap) | r only SHRINKS from the reserved r_cap, so separation cannot break; the slots [K, top] are written here and never again (`S/slenderness.rs:7107-7113` stops below K) |
| F. Structure below K (existing descent, merges, braces, ties, slenderness, clearance reroute, head joins) | posts, trunks, braces, landings | touches no slot above K; a model landing is a FOOT contact admitted through B's door with the arm's landing load; plate fusion free |
| G. Lowering, emit, audit (unchanged `P/lower.rs`, `E/real_contact.rs`) | the file; read-only audit | the audit is a witness, never a repair |

### 2.2 Count and spacing are decided in the creating step
- Capacity pitch, DERIVED from registered numbers only: a hex cell of pitch p carries (√3/2)·p², so p_cap = r_cap·√(2πX/√3) = 8.08·r_cap = 3.23 mm at r_cap 0.4, 3.18 mm at the f32-safe 0.3935 (cr1's 0.787). Seat pitch = min(reach pitch, p_cap): a one-line change at `S/up_sweep.rs:718`. On every flat island this alone gives Voronoi loads ≤ Q, r = r_cap, patch ≤ 0.8, and spacing 3.2 mm ≫ 0.85 mm, so non-touching is automatic. Round_50 goes from 198 arms to about 225-245 (bound 223), not 340 or 466.
- The greedy fill for island edges and thin features must carry the same rule: a seat may ROOT at most Q of demand (`root_and_clear` stops clearing at Q/cell_area cells; the rest stays demand for the next seat) and `choose_plan` (`S/up_sweep.rs:1635`) compares capacity-legal plans only, else the sparser overloaded plan wins on count.
- Where load arrives from layers above through the model (the Cylinder lowest-line case, `S/down_sweep.rs` header), the creation bound under-reads; stage C discovers it and stage D re-samples that contact's Voronoi region at p' = p·√(Q/L) through the same admit(). Two routing passes before structure, against develop's four and cr1's twenty-four. Residual: emit the legal contacts, warn with the deficit. Never enlarge, never fuse.
- admit(candidate): batched nearest-face facts for the whole wave (`S/tip_landing.rs` as cr1's `construction.rs:48` does), planned patch from `P/lower/contact_cap.rs` (wt-cr1, port it), 3D reservation index (cr1 `Reservations`, or `fusion.rs` ContactIndex 389-449 queried, not refused on). Over-cap or touching → next candidate cell in score order; none → residual demand, warned.

### 2.3 Approach at every seat, and why it removes a whole class of defects
- Every contact has a stub along a chosen axis from the seat to K (length from the existing head law g + 2R + L_link + r_K, `S/heads.rs:113-116`). Candidates: footprint normal, vertical, the arc between (existing `plan_head_on_axis`, `placement_normals` 1989). Rank: (1) zero cone/ball overlap with the model, (2) a post from K can be routed against the reservations (plate, trunk corridor, or an admitted foot), (3) best support (lowest worst utilisation), incumbent on ties. Vertical on a flat face IS the normal, so the discs do not change.
- With axis = normal, abs_cos = 1 in the seat law and the flank term vanishes: the planned patch is 2r on every locally flat face. One r_cap, not a per-seat table. The 17 Deinonychus strips and most TR go away by construction. Narrow or concave features (FeatureWidth) are the exception: read the real patch at creation (batched `E/real_contact.rs` on the stub alone), reduce r or split.
- Heads before neighbours' posts exist (ChituBox's order) removes the HitsMember refusals that blocked 42 of 46 heads in the census (`V/RESULTS-apc.md` §C): posts route around reserved stubs; stubs never route around posts.

### 2.4 Feet
- Same primitive, same door: landing load, r ≤ r_cap, reservation. Over Q → fork the post into k feet at ≥ p_cap or land elsewhere; never max(body, enlargement) as a minimum (`S/model_feet.rs:79-95` goes). Member 162's tip-shaft-foot 1.591 mm patch cannot occur: the foot's reservation would be refused against its own tip's.

## 3. What survives, what is a primitive, what goes (Q3)
| Disposition | Code | Size |
|---|---|---|
| Survives as-is | slicing, grid, raster, reach, demand sweep; down-sweep routing, per_event, area law, conservation; lean law; member_clearance; slenderness/braces/ties below K; drain keep-out; lowering; emit; real_contact reader; validator as read-only audit | most of 118k lines |
| Reused as primitive | `hex_lattice_cells` 3745; `root_and_clear`; `tip_landing` batched faces; `contact_cap.rs` (wt-cr1, 237 lines) planned patch; cr1 `Reservations` / `fusion.rs` ContactIndex+capsules; `heads.rs` axis planning and `check_head`; `footprint_normal` measurement (not its rewrite pass); `contact_growth::triangle_disc_area`; insertion's footprint bucketing 4733-4770; S0 writer fixtures | ~3k lines kept |
| Rewritten | `commit_seat` seat→descend half incl. twin raise, `try_merge`, head gating (`S/up_sweep.rs:2846-3420`); `InsertionPolicy`/`insert_for_load` → split wave (`S/down_sweep.rs:700-770, 4710-4930`); `S/lib.rs` orchestration 3416-3478 and 3984-5760 | ~2.5k replaced by ~1.5-2k new |
| Deleted | lip_seat.rs (8987), graze.rs (757), contact_growth.rs passes (1432), footprint.rs slides (4183), footprint_normal rewrite pass, head_join relocation into the seat zone (part of 2421), second_contact.rs (1549), model_feet sizing (453), fusion keep-both/twin, bore relocation, ContactCaps second run | ~15-18k lines |
| Re-pinned | head_carrier_tests (3378), lip_draw_tests, validator_tests, topology-pinned model tests | once, at the landing gate |
Honest statement: this is a rewrite of the seat→contact half of the placement core (about 30 files, +3-4k / −15-20k lines). Demand, routing, sizing law, structure, lowering, emit and audit survive. It is SMALLER than develop, not larger; CBT and the frozen-handle design were both larger than develop.

## 4. Falsifier (Q4): cheapest first, ≤ 2 full runs per model
- **F0 (done, no build):** the disc CS is predicted by 11.79 vs 9.05 mm². If a 3.2 mm lattice on Round_50 does not give CS = 0 with ~225-245 arms, §1 is wrong and everything stops.
- **F1 writer fixtures (no model run):** (a) flat 50 mm plate at p_cap: every patch ≤ 0.787, FU 0, count = ⌈A/Q⌉ + edge; (b) 30/45/60° plates with a normal-axis stub: patch = 2r ± rounding (cr1's vertical control reads 0.966-1.646 uncapped); (c) a 0.3 mm ridge: real-patch read reduces r or splits; (d) a second seat offset one layer down on a steep face: refused, re-seated one pitch along; (e) a foot on a tip's face: refused or forked. Sabotage: drop the capacity term → CS; drop admit() → FU; drop the stub → flank growth; each with path hits.
- **F2 Round_25/50/100, pose pinned, one run each:** CS = FU = 0 in the file; arms within 1.3× of ⌈A/Q⌉ (report the ratio); `arms_inserted_by_load` = 0 on flat islands; down passes ≤ 4; overload 0, uncarried 0, residual 0; time reported against baseline with the arm-count ratio beside it. Stop: arms > 1.3× bound → the greedy fill over-places; fix before F3.
- **F3 Deinonychus, one run, second only for a named fix:** FU = 0 (any FU must name its provenance: "constructor" falsifies §2 and STOPS; "late pass" names a mover to delete); CS = 0 or each residual Unresolved with its reason; TR strips fewer than 17; no new CL/AR/LG/SL/HS subject; overload 0; uncarried 0; feet ≤ 0.8 and separated.
- Measure everything from the emitted file; binary provenance by no-op build; Treant and Giant_Bat at the landing gate only.

## 5. Decisions for the owner (Q5)
1. **Adopt "contact is an immutable primitive; pitch from capacity; approach at every seat" as the construction rule, and retire the pcap/CBT/cr1 lines** (keep `contact_cap.rs`, the S0 fixtures, `Reservations`). Effect: the cap and non-fusion stop being a repair problem; the disc fix is tiny; the Deinonychus fix is the constructor plus deletions.
2. **Default approach along the normal, vertical only where the face is flat or the normal is refused.** Effect: the flank term and the strips disappear by construction; Deinonychus topology changes (more stubs, posts from K); discs unchanged. This overturns "head only when blocked" (`PLINTH_APPROACH_HEADS=blocked`).
3. **Accept more arms on flat faces** (Round_50 ~225-245 vs 198; Round_100 about 2×): physics-required under X = 18 and the 0.8 cap; resin and time reported, not gated (consistent with the 2026-10-28 rulings).
4. **Order and stop rules:** Stage A = p_cap + r_cap + root-cap + hand-down stop (one lane, F1a + F2) → Stage B = constructor door, stubs, feet, split wave (F1b-e + F3) → Stage C = delete movers one at a time with a provenance witness → landing gate. Any F2 miss stops before B. No lane runs more than two models twice.
5. **Pinned tests drift once.** Re-pin at the landing gate, not per lane; a lane proves its own witnesses and the file metrics only.
