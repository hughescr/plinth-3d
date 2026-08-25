# Model-seated support arms — DERIVATION and DESIGN

> **Provenance.** Written 2026-08-21 against `feature/a4-controller @ b5dc266`
> (main checkout `/Users/craig/code/hughescr/dnd-campaigns/plinth-3d`, tree clean at the
> time of writing). **No production code changes in this phase** — this document is the
> whole deliverable. Every `file:line` citation is at that tip.
>
> **No cap, gate, threshold, tolerance or registered constant is moved, widened or bypassed
> by anything proposed here.** Two proposals are net TIGHTENINGS (a scar bound where there
> is none today, and a transit rule where there is a cruder one). One owner-ratified
> LOOSENING is inherited, not invented: SPEC-EMBED-POLICY's PRINT-FLOOR amendment.
>
> Evidence labels used throughout: **MEASURED** (a number a run produced),
> **DERIVED** (closed-form from MEASURED inputs or registered values),
> **INFERRED** (reasoning from a recorded fact, not itself measured),
> **ASSUMED** (a stated placeholder standing in for a fact that does not exist yet).

---

## §0. The answer in one page

A model-seated arm cannot exist today, and the reason is arithmetic, not geometry.

`cco.rs:4473 clearance_rows` masks a model-anchor arm's seat end by a flat `standoff_mm`
(0.6 mm) and then asks the kernel for `distance >= r_max + standoff_mm`. The seat is ON the
surface, so the first tested point is at most 0.6 mm from the model while 0.6 mm **plus the
arm's own radius** is demanded. **DERIVED, and it is a proof rather than a tendency:**

    clearance = d_min - (r_max + standoff)
    d_min <= standoff                 (the first tested station is `standoff` from a point ON the surface)
    => clearance <= -r_max <= -MIN_SHOOT_R_MM = -0.35 mm,  always, on any geometry.

The admissible set is **empty**, with exactly one escape: `seg_clear_one` returns
`INFINITY` when `length - inset1 <= 1e-9` (`queries.rs:1787`), so an arm **shorter than
0.6 mm is never measured at all**. Both halves are confirmed by the island diagnosis without
a single free parameter:

| DERIVED prediction | MEASURED (island diagnosis 2026-08-18) |
| --- | --- |
| best model-anchor clearance = `-MIN_SHOOT_R_MM` = **−0.350 mm** exactly | **−0.350 mm** on both Warhorse islands; the bound is attained in 53 cases |
| no arm longer than `standoff_mm` = 0.600 mm can ever pass | every model-anchor arm that ever won in corpus history was **0.054–0.596 mm** |
| the refusal is geometry-independent | **all 31,627** feasible candidate arms on Warhorse island 5 refused; **253/253** terminals |

The fix is not a new rule and not a constant move. **The engine already contains the correct
mask** — `contact::mask_extent_mm` (PR-2), used by `brace.rs` at both its candidacy site
(`brace.rs:1255`) and its exact site (`brace.rs:1954`). Three call sites — `cco.rs:4473`,
`rewire.rs:889-902`, and the terminalward half of both — never got it. That divergence is
already an open defect (#36); **#51 is its most damaging instance**, and the fix is
convergence on the rule the project already derived.

Two further findings change the shape of the lane:

* **`STANDOFF_MM`'s provenance is that it has none.** The registry says so in its own words:
  *"non-AcceptSafe legacy clearance policy; corpus use does not derive the selected
  distance"* (`m0-constants-source-ledger.json:2175`). It is a Placeholder. **It stays at
  0.6 mm and this design does not touch it**, because the defect is that it is applied in a
  region where it is physically meaningless — inside the arm's own contact — not that its
  value is wrong. §1 audits what it could be FOR, region by region, and finds only one
  physics-traceable requirement, an order of magnitude smaller.
* **#52 is the same shape of defect.** `initial.rs:1553 initial_overlap_exceeds_cutoff`
  condemns a stub whose axis stays inside the model more than 0.05 mm beyond the designed
  0.3 mm embed. That is a *transit* test applied to the *anchored* interval. The project's
  BINDING, owner-ratified spec for exactly this question — **SPEC-EMBED-POLICY** — says the
  tip-anchored interval is exempt-eligible and graded by scar area and scar dimension, and
  only UNANCHORED intervals are transits. `initial.rs` does not implement it. Under
  Craig's three-seat ruling, model-interior seating is legal, so **re-entry per se cannot be
  a condemnation**; the honest test is scar, plus anchoring topology.

---

## §1. THE PHYSICS QUESTION — what clearance does a model-seated arm actually need?

### §1.1 What the kernel actually tests, and why a mask exists at all

`NativeSurface::seg_clear_one` (`rust/geom_kernel/src/queries.rs:1766-1797`):

```rust
let a = p0 + u * inset0;                    // first tested station
let b = p0 + u * (l - inset1);              // last tested station
if s_hi - s_lo <= 1e-9 { return (INFINITY, nan3); }   // <-- the nub escape hatch
let r_max = r_lo.max(r_hi);
let thresh = r_max + standoff;
let clr = seg_surface_min(a, b).0 - thresh;
```

Two properties decide everything below.

1. **`seg_surface_min` is UNSIGNED**, with a pierce early-out to 0.0
   (`queries.rs:1695-1706`). It cannot tell "0.6 mm outside the skin" from "0.6 mm inside
   it". The clearance test therefore says nothing about embedment; embedment is a separate
   question, measured by `penetration_intervals_*`. That separation is correct and this
   design keeps it. **MEASURED from source.**
2. **The criterion is sphere-swept-capsule, not tilted-disc.** So the mask that solves it
   is a pure arc-length exemption, and `contact.rs:276 mask_extent_mm` is the closed form
   that solves it exactly:

       u* = (h + r_max + standoff + MARGIN) / |cos(theta)|

   with `h` the PR-1 seat depth (0 for an unseated end), `MARGIN` the run's own process
   resolution (max layer height). Its guarantee — pinned against the REAL kernel by
   `contact.rs::tests::planar_seat_round_trip_returns_exactly_margin_from_the_kernel` — is
   that on a locally planar seat the kernel returns **exactly `MARGIN`**. Not "passes":
   *exactly MARGIN*. **MEASURED from source + an existing passing test.**

A mask is unavoidable. At the seat the arm touches the model by definition, so the gap grows
from zero continuously; demanding a finite gap at the point of departure demands a step
discontinuity, which no straight arm can supply. The only question is **how much** is exempt
and **what checks the exempt region instead**.

### §1.2 The honest requirement, region by region along the arm

Take a model-seated arm of end radius `r`, seated depth `h` below the tangent plane, leaving
at incidence `theta` from the surface normal. Let `u` be arc length from the seat.

**Region A — the seat contact zone, `u <= (h + r)/|cos|`.**
Here the arm's own body intersects or touches the seat solid. This is what seating *is*.
**Required clearance: zero.** Any positive requirement here is a demand that the arm not
touch the thing it stands on — a contradiction, and precisely the contradiction #51 measures.

**Region B — the departure ramp, `(h + r)/|cos| < u <= u*`.**
The arm has left the surface but is still within `standoff` of it. Candidate justifications
for demanding air here, each traced:

| candidate justification | traced to | verdict |
| --- | --- | --- |
| **cure bleed / fusion** — two separately-cured features closer than the per-side over-cure fuse into one solid | PIXEL_PITCH_X_UM = 14, PIXEL_PITCH_Y_UM = 19 (**MEASURED**, vendor, `constants.md:193-194`); per-side over-cure ~0.013 mm, total ~0.026 mm (**INFERRED** from `plate-generator-PLATE-LAYOUT.md` family C/W rationale; family W exists precisely to MEASURE it and has not been measured) | **Real, and an order of magnitude below 0.6 mm.** And in region B the arm and model are *already one solid* by design — fusion here only thickens the designed fillet. That is a **scar** question, not a gap question. |
| **removal-tool access** — a cutter must reach the neck to part the arm from the model | no record derives it. Nothing in `docs/engineering-journal/` states a blade thickness, a tool, or a required approach angle | **NEEDS MEASUREMENT.** This is the only justification that could plausibly want ~0.6 mm, and it is currently folklore. But note it constrains the **scar's maximum dimension**, which is already registered as `L_SCAR_MAX_MM = 2.5 mm` — not a mid-arm gap. |
| **peel sweep / dynamic collision** — the part flexes during release and an arm could rub the model | `PEEL_PRESSURE_PA` and `MODULUS_GREEN_GPA` are registered, but **task #63 records that they jointly imply failure at 0.007 % strain, ~10³ off**. A deflection derived from them today would be arithmetic on a known-inconsistent pair | **UNUSABLE until #63 is resolved.** Do not derive a clearance from it. |
| **drain / wash** — resin must leave the gap | unmeasured; no wash model exists in the record | **NEEDS MEASUREMENT.** Scales with gap width; a capillary gap is a wash problem, not a print problem. |
| **slicer voxel merge** | subsumed by cure bleed, at a smaller scale (one pixel = 14–19 µm) | covered above |

**Conclusion for region B — DERIVED.** No recorded justification derives 0.6 mm, and the
registry agrees in its own provenance note. The physics-traceable requirement in region B is
**~0.03 mm (fusion scale)**, and even that is moot because the arm is already fused to the
model in region B on purpose. What region B genuinely needs is not a gap but a **bound on
the resulting scar**: how much of the model's surface the fillet consumes, and how long the
gash is. That bound exists, is registered, and is owner-ratified: SPEC-EMBED-POLICY's
`A_ABS_MM2 = 2.0 mm²`, `KAPPA = 1.25`, `L_SCAR_MAX_MM = 2.5 mm`.

**Region C — free air, `u > u*`.**
Here the arm is a separate solid running past the model. Fusion, wash and peel-collision all
apply. `standoff_mm = 0.6 mm` is a **conservative** requirement here — far above the ~0.03 mm
fusion scale, unaudited but harmless in the admitting direction, and **Craig's**.
**It stays. This design proposes no change to its value.**

**Region D — the landing contact zone at the other end.**
Symmetric to region A, with that end's own `h`, `r` and `|cos|`. Today `cco.rs:4486` masks
this end by a flat `contact_depth_mm` (0.3 mm), which is **less** than the honest exemption
`(0.3 + r + standoff + MARGIN)/|cos|` — so this end is *over*-tested, in the same family of
defect but in the opposite direction of error.

### §1.3 What the current mask does, in one table

| end | honest exemption (PR-2) | what `cco.rs`/`rewire.rs` use | error |
| --- | --- | --- | --- |
| model-anchor seat (Rootward) | `(h + r_max + 0.6 + MARGIN)/|cos|`; at `h=0.3, r=0.35, |cos|=1` → **1.30 mm** | `standoff_mm` = **0.600 mm** flat | under-masks by 0.70 mm; makes the test **unsatisfiable** |
| contact tip (Terminalward) | `(0.3 + r + 0.6 + MARGIN)/|cos|`; at `r=0.35` → **1.30 mm** | `contact_depth_mm` = **0.300 mm** flat | under-masks by 1.00 mm |
| brace model rung (both) | `contact::mask_extent_mm` | `contact::mask_extent_mm` | **correct** (`brace.rs:1255`, `brace.rs:1954`) |

### §1.4 The provenance audit of `STANDOFF_MM` — what was it FOR?

Traced end to end:

* **Registry** (`m0-constants-source-ledger.json:2175`): classification `runtime_routing`,
  wrapper **Placeholder**, detail *"non-AcceptSafe legacy clearance policy; corpus use does
  not derive the selected distance"*. Source locator:
  `scripts/support_placement/config.py:standoff_mm` — a Python-era config value.
* **Physics doc** (`03-physics-and-constants.md:504`): *"v0 checked only the ray centerline
  and still had 27/137 violations; v4+ checks the full swept envelope, so this is now
  genuinely free clearance."* — i.e. in v0 the 0.6 mm was absorbing the member's own
  unmodelled radius; since v4 the radius is explicit (`thresh = r_max + standoff`), so the
  0.6 mm became a *pure additional air gap* and was never re-derived after its original job
  was taken over by the envelope.
* **Constants table** (`constants.md:213`): `placeholder(note="Placeholder; non-AcceptSafe.
  Legacy clearance policy whose corpus use does not derive the distance.")`

**DERIVED verdict: `STANDOFF_MM` is a search-pruning value that is currently DECIDING
outcomes.** Under the physics-first doctrine that is a defect of *placement*, and this
design fixes the placement (region A/B exemption) without touching the value. See §5.3 for
the one case where the *value* decides and an escalation would be needed — the design does
not propose it.

### §1.5 The honest rule, stated

> **A model-seated arm must clear the model by the registered free-air standoff everywhere
> except within its own contact regions, whose extent is the PR-2 mask `u*`; and the exempt
> region is not unchecked — it is graded by the registered scar bound.**

Three clauses, no new constants:

1. **Exempt** `[0, u*_terminal]` and `[L - u*_rootward, L]` from the distance test, with
   `u*` from `contact::mask_extent_mm` on the geometry the kernel will see. *(The exemption
   is minimal by construction: it is the exact solution of the kernel's own criterion, and
   returns `MARGIN`, not slack, on a planar seat.)*
2. **Enforce** `clearance >= 0` on the remainder, at the unchanged `standoff_mm`.
3. **Bound** each exempt region by SPEC-EMBED-POLICY rules 1–3 (`A_patch <= A_max`,
   `L_scar <= L_SCAR_MAX_MM`, `A_anchor >= pi*r_end^2`). **This clause is new to the CCO
   family and new to the brace family too** — today the exempt region is simply unchecked,
   and `transit_measurement.rs:1` says of itself *"intentionally a measurement boundary
   only. It neither classifies an interval nor assigns a clearance, gate, report,
   certificate, or defect."* Clause 3 is the price of clause 1, and it is a tightening.

---

## §2. #52 — the honest stub-axis rule for edge-on faces

### §2.1 What the code does today

`initial.rs:642-648` mints every contact stub along the **contact normal**:

```rust
let p0 = position - unit * contact_depth_mm;   // 0.3 mm inside
let p1 = position + unit * stub_length_mm;     // 2.5 mm out along the SAME normal
```

`classify_initial_overlaps` (`initial.rs:1336`) sweeps the stub capsule with 8 rim rays plus
majority sampling and returns inside-intervals along the axis.
`remaining_illegal_axis_overlap` (`initial.rs:1542`) then takes

    max over intervals of ( min(s_b, L) - max(s_a, contact_depth_mm) )

and `initial_overlap_exceeds_cutoff` (`initial.rs:1553`) condemns when that exceeds
`MIN_ILLEGAL_LEN_MM = 0.05 mm`. The stub is quarantined **before any arm is proposed**
(`initial.rs:906-914`).

**MEASURED (island diagnosis):** Rotting Warhorse island 74, 13.041 mm², right cheek/jaw
below the ear — all 9 terminals condemned, remaining-inside 0.739–2.429 mm against the
0.05 mm cutoff, **0 arms ever generated**. The island's neighbours face nearly sideways
(island 68's mean normal is 5.05° above horizontal), so a normal-direction stub grazes
along inside material instead of leaving it.

### §2.2 What that test is, and what it is not

It is a **transit** test — "does this member run through material it did not declare?" —
applied to an interval that **starts at the designed embed** and is therefore *anchored*.
Two consequences:

* **Under Craig's three-seat ruling it is invalid as written.** Model-interior seating is
  legal; a support may seat into the model surface including a cavity interior. An anchored
  immersed run is a *deep seat*, and depth alone is not a defect. SPEC-EMBED-POLICY says so
  explicitly: *"There is NO depth cap on the tip-anchored interval itself: a deep
  perpendicular plug into thick solid has a small crossing patch and, per the owner's
  idempotence ruling, no further physical consequence."*
* **The evidence cannot currently tell anchored from unanchored.** `remaining_illegal_axis_overlap`
  clips at `contact_depth_mm` and takes a max, discarding the interval's start. So the
  recorded 0.739–2.429 mm figures do not say whether the run begins at the seat (anchored,
  a deep seat) or begins further out (unanchored, a genuine transit). **That distinction is
  the first thing the fix must measure**, and it is recoverable from the existing interval
  rows without a new run.

### §2.3 The real failure mode, and its honest test

Immersion is not the harm. The harms are:

* **H1 — the scar.** The support fuses to the model over the whole immersed run. Removal
  then tears the mini rather than parting at the tip neck. **This is exactly what
  SPEC-EMBED-POLICY rules 1–3 bound**: `A_patch = A_shaft + A_ball <= A_max` with
  `A_shaft = min(pi*r_x^2/cosang, 2*r_x*T)` — the `2*r_x*T` term is the true grazing-chord
  bound and grows linearly with immersion — and `L_scar <= L_SCAR_MAX_MM = 2.5 mm`, and
  `A_anchor >= pi*r_end^2`, whose stated purpose is *"the fuse/neck floor: the break must
  happen at the tip neck"*.
* **H2 — an undeclared crossing.** A member that enters material it did not declare has a
  load path the engine never modelled and a scar nobody bounded. **This is rule 4**, with
  the owner-ratified PRINT-FLOOR moot downgrade for chords below 0.08 mm.
* **H3 — thin-feature damage.** Covered by the separate, unchanged thin-feature-span check.

**The honest test is therefore the binding spec the project already ratified**, and
`initial.rs` simply does not implement it. Stated for a stub:

> Classify each immersed interval as **anchored** (its start is within `TOL_MM = 0.05 mm` of
> the designed embed endpoint `p0`) or **unanchored**.
> The anchored interval is exempt-eligible and graded by rules 1–3.
> Every unanchored interval is a transit and illegal unless its crossing chord is below
> `PRINT_FLOOR_CHORD_MM = 0.08 mm`, in which case it is MOOT, counted and reported.

**No constant moves.** Every number is registered and already read by
`transit_measurement.rs:51-84` for the diagnostic. The only behavioural loosening is the
PRINT-FLOOR moot band `[0.05, 0.08) mm`, which is owner-ratified 2026-07-24 and which the
crude test currently ignores.

### §2.4 Sizing the prize BEFORE building the instrument

**DERIVED, from the MEASURED depths and the registered formulas.** Take the anchored case
(so `T = overlap + contact_depth_mm`) and `r_x ≈ 0.30 mm` (between `tip_radius` and
`MIN_SHOOT_R_MM = 0.35`; **ASSUMED**, and recorded per row today so it can be replaced with
the real value at no run cost):

| RW island-74 terminal | overlap (MEASURED) | `T` | `A_shaft = 2*r_x*T` | vs `A_max = 2.0 mm²` | `L_scar = T + 2*r_x` | vs 2.5 mm | verdict |
| --- | --- | --- | --- | --- | --- | --- | --- |
| minimum | 0.739 | 1.039 | 0.62 mm² | pass | 1.64 mm | pass | **LEGAL** |
| median | 0.877 | 1.177 | 0.71 mm² | pass | 1.78 mm | pass | **LEGAL** |
| maximum | 2.429 | 2.729 | 1.64 mm² | pass | 3.33 mm | **fail** | **ILLEGAL** — rule 2 |

**The scar-dimension rule, not the area rule, is what bites**, and it bites only at the deep
end. The break-even is `T = L_SCAR_MAX_MM - 2*r_x = 1.9 mm`, i.e. `overlap = 1.6 mm`.
**Prize: the terminals on island 74 whose overlap is below ~1.6 mm become routable** —
on the recorded min/median that is most of them, and island 74 is 13.041 mm² of the Rotting
Warhorse's 30.1 mm² of uncovered area. **This is a calculation, not a measurement.** It is
settled with **zero new runs** by recomputing the verdict from the interval rows already in
`island-out/rwh*.jsonl` (they carry the depths; if they do not carry `r_x` and `cosang`, one
instrumented re-run of the initial-contact stage on the Rotting Warhorse settles it — minutes,
not a pipeline pass).

### §2.5 The stub-direction question, and why it is second not first

The diagnosis' preferred fix is *"aim the stub along a direction that actually leaves the
surface"*. That is correct physics and needs no constant, but it is **second**, because:

* it moves printed geometry on every model (the stub axis is the emitted support), so it is
  an OD-8 stop-and-report change with a corpus-wide blast radius, whereas §2.3 moves geometry
  on **no** model that is compliant today; and
* until §2.3 lands we do not know how many terminals still need steering. Sizing first.

When it is needed, the honest constraints on a steered stub are already in the engine:
(a) it must leave the solid (the same interval sweep answers this); (b) the contact disc must
stay engaged, which PR-1's flank term `process_resolution + r_end*sin(theta)` already prices
(`contact.rs:219`); (c) the lean limit and the emergence rule (PR-3) already apply. So it is
a bounded candidate search over a small cone about the normal, choosing the direction that
minimises immersed length subject to (b) and (c) — batched, one interval sweep per direction
rung over the whole terminal set, never per-terminal.

---

## §3. THE DESIGN

### §3.1 How model seating lands, under the three-seat ruling

**Piece A — seat the CCO model anchor (PR-7 `CCO_ANCHOR_MASK_AT_THE_SEAT`).**
`cco.rs:4062 apply_model_anchor` stops setting `p1_mm` to the raw seat point and calls
`contact::seat_endpoint(seat, normal, other_end, contact::seat_depth_mm(...))` — the same
two functions the brace model rung already uses. `attest_model_anchor_seats`
(`cco.rs:3914`) builds its provisional row from the seated endpoint.

`contact::seating_of` (`contact.rs:83`) must then classify the family as `Seated`. Its own
doc comment already names this edit. **Two options, and the choice is a real tradeoff:**

* **A1 — key on `MemberKind::Strut` + `ContactEnd::Rootward`.** No stored field moves, so
  no member digest moves for that reason alone. Coherent for bifurcation: an upper half
  inherits its host's `p1` *and* its host's kind, so seating follows the geometry
  automatically (`cco.rs:4229` confirms the Rootward inheritance rule). `cco.rs:4082` is the
  only production mint of a Rootward model contact, so the key is unambiguous today.
* **A2 — mint the anchor as `MemberKind::Other("model_anchor")`, mirroring
  `MODEL_RUNG_FAMILY`.** Cleaner and self-documenting, but `kind` is stored on `Member` and
  digested, so **every** committed model-anchor member moves the forest digest — including
  today's surviving nubs.

**Recommendation: A1**, with a `debug_assert` that no other family mints a Rootward model
contact, and a source-text guard extension (§3.6). A2 is the right long-term shape and is
recorded as a follow-up, not smuggled into this change.

**Piece B — converge the three clearance masks (this is #51, and it is #36's general case).**
Extract the PR-2 inset computation that already exists at `brace.rs:1926-1968` into
`contact.rs` as one function, and call it from all four sites:

| site | today | after |
| --- | --- | --- |
| `cco.rs:4473 clearance_rows` | `inset0 = contact_depth_mm`, `inset1 = standoff_mm` | `contact::mask_inset_for(member, normal, policy)` |
| `rewire.rs:889-902` | same flat pair | same helper |
| `brace.rs:1926-1968 exact_clearance_rows` | already PR-2 | calls the extracted helper |
| `brace.rs:1206-1255 gather_candidates` | already PR-2 via `contact::admit` | unchanged |

The helper needs the surface normal at the contact point. `brace.rs` gets it with one
batched `surface.project(&points)` over all contact-declaring rows (`brace.rs:1922`). `cco.rs`
and `rewire.rs` adopt the same shape: **one batched projection per candidate batch, not one
per member.** No per-item kernel call is introduced.

**Piece C — the price of the exemption: bound the exempt region by the registered scar rule.**
`transit_measurement.rs` already computes `A_patch`, `L_scar` and `A_anchor` from registered
constants and calls itself measurement-only. Promote that computation to a **candidacy
refusal** for contact-declaring members: a candidate whose exempt region violates
SPEC-EMBED-POLICY rules 1–3 is refused through the existing `AnchorFootOutcome::Refused` /
`AnchorSeatRefusalReason` channel, exactly as a breached foot ball is. **This is a strict
tightening**, it introduces no constant, and without it Piece B would enlarge an unchecked
region — which is the one thing a reviewer should refuse.

**Piece D — apply the same rule to the initial stub (this is #52).**
Replace `remaining_illegal_axis_overlap` + `initial_overlap_exceeds_cutoff` with the
anchored/unanchored classification of §2.3, graded by the same promoted predicate from
Piece C. `initial.rs` keeps its quarantine disposition (OD-4: quarantine, never a pipeline
error), and keeps recording the depth so `runner.rs:2264`'s `StubReentry` class still has a
measured number to report.

**Piece E — the closed-load-path test, and cavity ceilings.**
The load already terminates correctly: `propagate_carried` (`cco.rs:4201`) walks contact →
root and stops, so a model-seated arm's load ends at its seat. What is missing is that
**nothing records or verifies it**: `forest.rs:872-881` states plainly that the arena holds
no plate reference, so `Forest::validate` cannot tell a legitimately seated root from one
hanging in mid-air, and `lower.rs:412 is_plate_root` silently omits the root boss instead of
objecting.

The closed-load-path test, stated as a predicate over the forest:

> Every live member's root end is one of: (1) within the plate band; (2) a model contact
> whose seat attestation passed **and** whose scar verdict is legal; (3) a parent member
> that itself satisfies this predicate. A forest containing a root that is none of the three
> is refused before write, and the verdict names the root.

That is recursion over the parent chain with no new geometry, and it is the first check that
can actually distinguish the three seats. It closes the hole `forest.rs` documents against
itself, and it is what makes "supports may seat on the model" safe to enable rather than
merely permitted.

**Cavity ceilings.** Per Craig's ruling, a cavity ceiling must be supported from the
cavity's lower interior. Nothing in perception or candidacy blocks that today: the seat pool
is built from `exterior_face_mask`, which is *"a step along this face's own normal lands in
empty space"* — true for a cavity wall (`anchor.rs:320`, `queries.rs:2780`). And the transit
question does not bite inside a cavity: after hollowing the inner wall is part of the mesh,
so cavity void is OUTSIDE the solid and an arm crossing the cavity registers no immersed
interval except at its seat, which is the designed embed. **So cavity-ceiling seating needs
Pieces A–C and nothing else** — plus Piece F.

**Piece F — keep interior arms clear of the drain.** Unchanged from the 2026-08-13 design
§3.4, and still un-built: `crates/plinth-support` contains no reference to drains or vents.
An arm near a cavity's low point can dam the resin that must flow out of a 2.5 mm bore. One
new refusal reason, fed by the openings the driller already planned, using the bore radius
the driller chose and the registered `standoff_mm` — **no new constant**, strictly a
tightening, batched against a handful of openings. It gates *interior* arms only, so it does
not block the Warhorse/RW exterior case and can land after Pieces A–D.

### §3.2 PR-7's kiss-catch — disposition, and the reconciliation the record needs

The record appears to contradict itself and does not:

* Task #51 says *"the PR-7 catch fired ONCE in two full runs and on none of these islands —
  the catch is not the blocker, this mask is."* **True.**
* The 2026-08-21 state note says *"#51+#26 travel together (mask fix alone shifts the wall
  back to the PR-7 kiss-catch)."* **Also true.**

Both, because the catch is at `elect_committable_winner` (`cco.rs:3482`) — it only fires on a
**winner**. Today a model anchor is refused at clearance long before it can win, so the catch
is almost never reached. The moment Piece B lands, model anchors start winning, every one of
them is an unseated kiss (`NotAttached`: **MEASURED — all 3,186 attested seats report
`centreline = false` with 1–3 of 8 rim rays; 1,498 of them on `make_overhang_table` alone**),
and the catch refuses all of them. So:

> **Piece A and Piece B must land in the same commit.** Piece B alone moves the wall from
> clearance to the catch and delivers nothing. Piece A alone changes geometry and still
> commits nothing.

**Disposition of the catch itself:** once Piece A lands, `NotAttached` stops being a
statement about a missing rule and becomes an ordinary defect, so it moves to **candidacy** —
the same disposition `BuriedSurface` already has for this family and that the brace model
rung already has for both verdicts. The commit-time tripwire at `cco.rs:3482` and its rewire
sibling (`RewireCandidateBatch::winning_generation_index`) then become unreachable, exactly
as their own doc comments predict, and come out **in the same commit**. Their two synthetic
tests become candidacy-refusal tests rather than being deleted.

### §3.3 The FU-87 / #41 dependency — HONOURED in part, CHALLENGED in part

**Recorded chain:** #24 (fault group B) → #41 (FU-87 minimum-arm screen revalidation) →
#26 / FU-86 (PR-7 seating).

**The reason it exists, from the record.** FU-87 refuses seats whose *proposed* arm is
shorter than `STRUT_MODEL_ANCHOR_MIN_LEN_MM = 5.0 mm`. Built and measured 2026-08-13: it took
the gate from 1354/0 to 1349/6 and **regressed `make_overhang_table` from shipping to
failing**, because refusing candidates pushed terminals onto the zero-candidate path and that
path aborted with `Cco(InvalidGeometry)`. Hence: fix group B first.

**The challenge, with reasons.** Put the two screens side by side:

| screen | admits |
| --- | --- |
| today's clearance mask (#51) | model-anchor arms **shorter than 0.600 mm** (the nub escape hatch) |
| FU-87's minimum-length screen | model-anchor arms **at least 5.000 mm** |
| **intersection** | **empty** |

FU-87 did not regress `make_overhang_table` by being wrong. It regressed it because
**`make_overhang_table` was routing on nubs** — the only arms #51's mask permits — and FU-87
correctly refuses them, leaving zero candidates. **The two tasks are the same defect seen
from opposite ends.** Therefore:

1. **#51's mask fix is not blocked by #24 or #41.** It is a pure *admission*: it moves
   candidates from refused to admitted and none the other way, so it cannot increase the
   population on the zero-candidate path. It **reduces** it. Landing it first is strictly
   safer than landing it later.
2. **#51's mask fix is the thing that makes FU-87 safe.** It creates the ≥5 mm arms FU-87
   wants to select for. The recorded chain has the two in the wrong order for this half.
3. **The tightenings in this design — Piece C (scar bound) and Piece D (transit rule) — DO
   inherit the FU-87 hazard**, because both can refuse a candidate that is admitted today.
   Those honour the chain: they land behind #24/#39's zero-candidate handling, or they land
   with an explicit fail-soft (a terminal with zero surviving candidates becomes ordinary
   uncovered area for the coverage loop, which is FU-88's shape and #39's acceptance case 3).

**Proposed revision of the chain, for Craig's ruling:**

    #51 Piece A+B (seat + mask convergence)   -- unblocked, lands first
        |
        +-> #41 FU-87 re-land                 -- now has a non-empty admissible set
        |
        +-> Piece C + Piece D (tightenings)   -- behind #24/#39 zero-candidate handling
                |
                +-> Piece E (load-path proof) -> Piece F (drain keep-out) -> cavity ceilings

**ESCALATION: this reorders a dependency Craig recorded on 2026-08-21.** It is offered with
the reason above and is not acted on without his word.

### §3.4 What changes, by file

| file | change | kind |
| --- | --- | --- |
| `contact.rs` | extract `mask_inset_for(member, normal, policy)` from `brace.rs`; extend `seating_of` to the CCO family (option A1) | new shared helper; one classifier arm |
| `cco.rs:4062 apply_model_anchor` | seat `p1` via `seat_endpoint` | **moves printed geometry** |
| `cco.rs:4473 clearance_rows` | flat insets → `mask_inset_for`; one batched `project` per candidate batch | **moves candidate outcomes** |
| `cco.rs:3482 elect_committable_winner` | tripwire removed; `NotAttached` refuses at candidacy | disposition move |
| `cco.rs:3914 attest_model_anchor_seats` | provisional row built from the seated endpoint | witness now passes |
| `rewire.rs:889-902` | flat insets → `mask_inset_for`; sibling tripwire removed | same as `cco.rs` |
| `brace.rs:1926-1968` | calls the extracted helper (behaviour identical) | refactor only |
| `transit_measurement.rs` | promoted from measurement-only to a candidacy predicate | **new refusal** |
| `initial.rs:1542-1560` | crude overlap test → anchored/unanchored + scar grading | **new rule** |
| `runner.rs` / `coverage_closure.rs` | `ClearanceMaskUnsatisfiable` and `StubReentry` should stop firing on the warhorses; keep both classes and their instrumentation | reporting |
| `forest.rs` / `lower.rs` | root-seat kind recorded; closed-load-path predicate | new check |

### §3.5 Which tests pin today's behaviour, and how each moves

| test | pins | movement |
| --- | --- | --- |
| `contact.rs::planar_seat_round_trip_returns_exactly_margin_from_the_kernel` | PR-2 against the real kernel | **unchanged** — it becomes load-bearing for three more call sites |
| `contact.rs::at_normal_incidence_mask_is_seat_plus_r_max_plus_standoff_plus_margin` | the mask formula | **unchanged** |
| `brace.rs::seating_follows_the_design_family` (`brace.rs:3085`) | asserts `MemberKind::Strut` and `Other("cco_upper")` are `OnSurface` | **MOVES** — both become `Seated` when Rootward. This is the single most explicit pin of "PR-7 is not landed" and its movement is the change's signature. |
| `cco.rs::a_winning_but_unattested_model_anchor_is_refused_rather_than_committed` (`cco.rs:7668`) | the commit-time tripwire | **MOVES** to a candidacy-refusal assertion |
| `cco.rs::a_winning_but_unattested_rewire_model_anchor_is_refused_rather_than_replayed` (`cco.rs:7794`) | rewire sibling | **MOVES**, same shape |
| `cco.rs::refused_model_anchor_has_no_replay_capability` (`cco.rs:7975`) | capability withdrawal on refusal | **unchanged** — the channel is the same |
| `cco.rs::detached_and_root_reanchor_model_anchor_replay_is_bit_exact` (`cco.rs:7591`) | replay determinism of a model anchor | **MOVES** (geometry moves; determinism must still hold) |
| `initial.rs::terminalward_embed_classifies_clean_transit_and_buried_boundaries` (`initial.rs:1896`) | the scalar overlap rule, incl. a `[0.2, 0.6]` "transit" case | **MOVES** — that case is tip-anchored and becomes a graded seat |
| `initial.rs::real_stacked_obstruction_accepts_cutoff_and_condemns_above_it` (`initial.rs:2461`) | the 0.05 mm cutoff on a real obstruction | **MOVES** — re-derived under the scar predicate; the obstruction it models is UNANCHORED and must stay condemned, so this test becomes the anchoring-classification pin |
| `retired_formula_guard.rs` | source-text ban on `embed_inset_mm` / `max(0.5)` over `brace.rs, cco.rs, rewire.rs, initial.rs` | **EXTENDED** — add the flat-inset idiom so the 0.6 mm inset cannot return at a fifth site. This guard is why #51 survived; it guarded the previous formula and not this one. |
| `a4_transactional_controller.rs:465-476` | that `ClearanceMaskUnsatisfiable` / `StubReentry` are counted, not inferred | **unchanged** as unit contracts; on the warhorses they should stop firing, which is the acceptance signal |
| `make_toed_slab` digest `4_190_462_360_230_124_963` at three pin sites | the whole-change regression pin | **MEASURE.** If the slab has no surviving model anchor it must be **bit-identical**; if it moves, that is an OD-8 stop-and-report with a from→to attribution, not an edited expectation |
| `make_overhang_table` | emits today, on nub arms | **the acceptance case.** It must still emit. This is the test the 2026-08-13 pass says the reviewer check must be: *"no model that emits today stops emitting"* |

### §3.6 Blast radius, honestly

**Large, and geometry-moving.** Every model-seated arm that becomes committable changes the
emitted forest, so goldens and census digests move. Per OD-8 that is a **stop-and-report**
with an attributed from→to record, not a silent side effect. Specifically:

* **Moves geometry:** any model that currently routes a model-anchor nub — `make_overhang_table`
  (1,498 attested seats), the three `anchor` unit fixtures the 2026-08-13 pass identified,
  and whatever the corpus census shows. **Must be measured before the change is judged.**
* **Moves candidate outcomes but not committed geometry:** models where model anchors still
  lose the price tournament. `compare_candidates` ranks purely by added resin volume with no
  family preference (`cco.rs:3309`), and a model-seated arm is usually cheaper than a plate
  descent — so the family split will shift, possibly a lot. **Piece C of the 2026-08-13
  design (the tournament-bias measurement) is not optional; do it before tuning anything.**
* **Does not move:** the area law (`X_ALLOWED = 12`), the minimum tip, brace clearance
  non-negativity, `STANDOFF_MM`, `MIN_ILLEGAL_LEN_MM`, `DEFAULT_CONTACT_DEPTH_MM`, every
  SCAR policy constant, and the Weeping Treant's memory limit.
* **Risk that the change is a net loss:** real, and named — the 2026-08-13 pass proved a
  *provable pure tightening* can still regress a shipping model. The acceptance bar is
  therefore behavioural, not directional: **no model that emits today stops emitting**, and
  `make_toed_slab` is bit-identical or attributed.

---

## §4. Facts that do not exist and must be measured

Sized first, per the FU-82 lesson.

| gap | why it matters | minimal experiment | prize |
| --- | --- | --- | --- |
| **The removal-access requirement.** Nothing in the record states what a support-removal tool needs to reach a neck. It is the only candidate justification that could want ~0.6 mm. | It is the sole surviving candidate rationale for `STANDOFF_MM`'s magnitude, and it decides whether 0.6 mm is conservative or arbitrary. | **Not a print.** Measure the actual tool: flush-cutter blade thickness and included angle, and the shortest neck it can part on an existing printed part. One afternoon, no plate. | Retires or grounds a Placeholder that today decides outcomes. |
| **Per-side over-cure (print bleed).** ~0.013 mm per side is INFERRED from family C's guard-rung reasoning; family W exists to measure it and has not been run. | It is the true fusion minimum — the only physics-derived clearance requirement in region B/C. | Family W witness posts are already designed on plate 5 (`plate-generator-PLATE-LAYOUT.md:1204`). They ride along on any authorized plate. | Grounds the fusion minimum; feeds #58, #60, #66 as well. |
| **Anchored vs unanchored, on RW island 74.** The recorded depths cannot distinguish a deep seat from a transit. | It decides whether §2.4's prize is real. | Recompute from the interval rows already in `island-out/rwh*.jsonl`; if `r_x`/`cosang` are absent, one instrumented run of the initial-contact stage on the Rotting Warhorse. **Zero pipeline passes.** | Settles 13.041 mm² of the RW's uncovered area. |
| **Tournament bias with the family live.** Unknowable by reading. | If model anchors still lose everywhere, cavity ceilings get no interior arms and Pieces E/F are moot. | The counters already exist (`cco_winner_model_anchor`, `cco_model_anchor`). One corpus run after Pieces A+B. | Decides whether the rest of the lane is worth building. |
| **`PEEL_PRESSURE_PA` vs `MODULUS_GREEN_GPA` (#63).** ~10³ inconsistent. | Blocks any dynamic-clearance derivation, permanently, until fixed. | Out of this lane's scope; **named so no one derives a clearance from that pair in the meantime.** | Unblocks the peel-sweep justification. |

---

## §5. Escalation-worthy points, collected

1. **The dependency reorder (§3.3).** #51's mask fix ahead of #24/#41, on the reasoning that
   it is a pure admission and that it is what makes FU-87's admissible set non-empty. **This
   contradicts a chain Craig recorded on 2026-08-21 and needs his ruling.**
2. **`STANDOFF_MM` is a Placeholder that decides outcomes.** This design does not move it and
   does not need it moved. Recording the fact under the physics-first doctrine, and noting
   that the registry's own provenance note already says the corpus does not derive it.
3. **The plate-family refusals are a different animal (§5.3 below) and would need the value
   to move.** Not proposed. Flagged.
4. **Piece C promotes a measurement-only module to a gate.** `transit_measurement.rs` says of
   itself that it assigns no gate. Promoting it is the right physics and a strict tightening,
   but it changes that module's declared contract, which is an owner-visible move.
5. **Option A2 (`MemberKind::Other("model_anchor")`) is the cleaner shape and moves digests.**
   A1 is recommended to keep the digest movement attributable to geometry alone. If Craig
   prefers the self-documenting family name, the digest movement is larger and must be
   attributed separately.
6. **PRINT-FLOOR moot band.** Adopting SPEC-EMBED-POLICY faithfully admits crossings with
   chords in `[0.05, 0.08) mm` that today's crude test condemns. Owner-ratified 2026-07-24,
   so inherited rather than invented — but it is the one admitting change in Piece D and it
   should be visible, not buried.

### §5.3 The plate-arm refusals — what the mask fix does NOT solve

**MEASURED:** on the stuck islands the best *plate* arm missed by 0.2452–1.4455 mm.
**DERIVED** from `thresh = r_max + standoff` with `MIN_TRUNK_R_MM = 0.8`:

* Warhorse island 5, best plate `−1.4455 mm`: implies `d_min ≈ 0` — the arm **pierces the
  model**. That is a genuine collision. The mask fix does not and should not admit it.
* Warhorse island 6, best plate `−0.2531 mm`: implies `d_min ≈ 1.147 mm` against a 1.4 mm
  threshold — **1.147 mm of real free air, refused for want of 0.253 mm more**. Against a
  fusion-scale requirement of ~0.03 mm that arm is entirely safe.

So island 6's plate family is a case where **the standoff's VALUE, not its placement, decides
the outcome.** Fixing it would require moving a registered constant, which this design does
not propose. It is recorded here because it is the honest boundary of the #51 fix: the mask
convergence rescues the model-anchor family unconditionally, and rescues the plate family not
at all. The model-anchor rescue is expected to be sufficient for both islands (they have 220
of 300 and 170 of 189 in-reach seats, nearest 0.097 mm and 0.048 mm), but if it is not, the
next question is a `STANDOFF_MM` question and it belongs to Craig.

---

## §6. What a reviewer should check first

1. **Re-derive §0's theorem from `queries.rs:1766-1797` and `cco.rs:4473` alone.** If
   `clearance <= -r_max` is not unconditional, everything here is wrong. Two files, ten
   minutes.
2. **Confirm the nub escape hatch.** `length - inset1 <= 1e-9 => INFINITY`. Then check that
   the longest model-anchor arm in corpus history (0.596 mm) is below `STANDOFF_MM` (0.600 mm).
   If some winning arm is longer than 0.6 mm, the mechanism is not the whole story.
3. **Check that Piece B is an admission and only an admission.** **DERIVED:** `|cos| <= 1`,
   so `u* = (h + r_max + standoff + MARGIN)/|cos| >= h + r_max + standoff + MARGIN`, which
   exceeds `standoff` (the rootward flat inset) by `h + r_max + MARGIN > 0` and exceeds
   `contact_depth_mm` (the terminalward flat inset) by `r_max + standoff + MARGIN - (h - h)`
   — with `h >= contact_depth_mm` by PR-1, both differences are strictly positive.
   **The mask can only grow, unconditionally, at both ends.** A larger inset tests less of
   the member, so Piece B can only move a candidate from refused to admitted. Re-derive this
   rather than taking it; if it can ever *shrink* a mask, Piece B is a tightening in disguise
   and inherits the FU-87 hazard.
4. **Check that Piece C actually bites.** A tightening that refuses nothing is not a
   tightening; it is a comment. Count the candidates it refuses on the corpus.
5. **`make_overhang_table` still emits, and `make_toed_slab` is bit-identical or attributed.**
