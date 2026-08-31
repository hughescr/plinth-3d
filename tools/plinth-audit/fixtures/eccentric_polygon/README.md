# `eccentric_polygon` — a support polygon that does not contain the body's own peel centroid

Target check: `i10_arrangement_spread` (`docs/specs/audit-spread-criterion.md`), closing
cross-family review Finding 2: *"there is no arrangement check at all -- every gate is
per-contact or per-arm."* This is the `spread_eccentric` row of that spec's §7 fixture
table: *"the same slab, three tips clustered at one end so the centroid falls outside
their hull"* → `check_fails`, `SPREAD_NO_EQUILIBRIUM`.

## Important: I10 stopped being a stub partway through this run

When this fixture's brief was written, `i10_arrangement_spread` (and `i9_unsupported_span`)
were unconditional stubs (`CheckOutcome::not_implemented`), and the brief anticipated that
"the gate does not exist yet" would be the reportable outcome here. **That changed mid-run**:
another agent working the same checkout landed real implementations of both checks in
`src/checks/arrangement_spread.rs` and `src/checks/unsupported_span.rs` (confirmed via
`git status`, which shows those two files `??` untracked and `src/checks/mod.rs`,
`src/context.rs`, `src/run.rs` modified — none of them files this fixture owns or touched).
I did not write, edit, or request that implementation; I only discovered it because this
fixture's own verification run against it produced a real code instead of
`AUDIT_CHECK_NOT_IMPLEMENTED`. Every number below is measured against that real
implementation, run read-only, exactly as it stood in the checkout.

## What is planted, and where

`eccentric_polygon.chitubox`, one model, 12 support records (three independent
TIP → TAPER → MEMBER → RAFT-pad chains), synthesised with `plinth_audit::synth` via
`generate.rs` in this directory (kept, like `double_load` and `starved_patch` do, as the
exact reproducible derivation of every number below).

**The body.** A single closed box: `x ∈ [-3.4, 3.4]`, `y ∈ [-2.7, 2.7]`, `z ∈ [-2, 2]` mm
(6.8 × 5.4 × 4 mm), lifted 5.03 mm off the plate (`z_plate = -7.03`, deliberately not a
layer-height multiple, same convention as `double_load`). Its bottom face (`z = -2`) is the
only down-facing geometry: `D_total = 6.8 × 5.4 = 36.72 mm²` exactly, and its footprint
centroid is `(0, 0)` by construction (the box is symmetric about both axes).

**The three contacts.** An equilateral triangle, side 2.6 mm, centred at `(1.05, 0)`:

| contact | XY position (mm) | role |
|---|---|---|
| `record 0` (`contact_id 0`) | `(1.050000, 1.501111)` | "top" vertex |
| `record 4` (`contact_id 4`) | `(-0.250000, -0.750555)` | "bottom-left" vertex |
| `record 8` (`contact_id 8`) | `(2.350000, -0.750555)` | "bottom-right" vertex |

Every column is a straight vertical chain (no tilt): TIP ball, straight down to a TAPER
narrowing from the arm radius to the tip's own `r_tip`, a constant-radius MEMBER, and a
RAFT pad on the plate — the same per-column shape `fixtures/double_load` uses, reused
deliberately because that shape is already proven clean against I1/I5/I6.

**The one thing that matters:** the triangle's nearest edge (top→bottom-left, crossing
`y = 0` at `x = 1.0667`) clears the origin — the box's own centroid — by only **0.1833 mm**.
The centroid sits just outside the triangle, not inside it, and the clearance is deliberately
small: see "why a small eccentricity, not a dramatic one" below for why that is the
*better* fixture, not a weaker one.

## Why this is a defect in physical terms

Every one of the three arms is individually well-built. Each tip's radius was sized, via a
**two-pass measurement** (see "measurement provenance" below), to exactly its own real
Voronoi-owned share of the slab's demand, then inflated 35 % for margin:

| contact | `a_owned_mm²` (real, Voronoi) | `r_tip` mm | `a_min_mm²` (I3's narrowest section) | `X = a_owned / a_min` |
|---|---|---|---|---|
| 0 | 11.049521 | 0.629032 | 1.677956 | **6.585** |
| 4 | 17.729310 | 0.796796 | 2.702056 | **6.561** |
| 8 | 7.941171 | 0.533266 | 1.169434 | **6.790** |

Every arm's own per-arm ratio (`docs/physics.md` §2's `X = A_supported / A_arm`) sits at
**6.56–6.79**, well inside `X_ALLOWED = 12`, with margin to spare. Read one contact at a
time, this file looks fine: `i1_seat_depth`, `i2_contact_area` and `i3_min_section` all
report `pass` on all three (confirmed below). **That is the entire point of this fixture**:
`docs/physics.md` §2(c) states `X ≤ 12` is *necessary, not sufficient* — three individually
well-sized arms still have no equilibrium if their arrangement cannot resist the load. Here
the load resultant (the slab's own peel centroid, at the area centroid of its footprint —
`docs/specs/audit-spread-criterion.md` §1.1) sits at `(0,0)`, and the triangle of the three
members' lines of action does not contain it. In the real print, nothing balances the
rotation this produces about the triangle's near edge: the slab tips off its three tips
during peel, precisely the mode `docs/physics.md` describes as *"blind to arrangement"* and
that no per-arm ratio, however conservative, can see.

### Why a small eccentricity, not a dramatic one

An earlier draft of this fixture clustered the triangle 3 mm from the box centre inside a
12 × 7 mm slab. That produced a *massively* eccentric, easy-to-see-by-eye geometry — and an
unusable fixture: with the cluster that far from the box's own centre, the Voronoi
partition of the slab's demand among the three contacts became wildly unequal (one contact
owned 62 % of the total demand in one attempt, 50 % in another), and the dominant contact's
fair share exceeded what a single tip can ever declare (`a_implied` at `r_tip = 1.0 mm`,
the declarable maximum, caps out at `12·π·1.0² = 37.7 mm²`; the dominant contact's owned
share alone was larger than that in the first attempt). A per-arm ratio genuinely
**cannot** be kept safe at that arrangement, for a completely different, unplanted reason
(I2/I3 would fail for real, not from the intended defect). Moving the triangle from `cx=3.0`
down to `cx=1.05` — just enough past the geometric threshold (`cx > 1.0667`) for the origin
to cross outside the triangle — keeps the Voronoi split close to balanced (11.0 / 17.7 / 7.9
out of 36.72, none dominant) while the origin is still unambiguously outside the hull by
`0.1833 mm`, **15.5×** the derived uncertainty `sigma = 0.0788 mm` computed by the check
itself (`margin_mm = -0.1588`, `sigma_mm = 0.078806`, ratio 2.0×; `dist_to_affine_hull` is
`nan` because this body is not the collinear case — `aff_dim = 2`, a genuine triangle, so
rows 2/3 of the outcome table do not apply and that field is legitimately unused). A small,
deliberate eccentricity is the physically honest fixture; a dramatic one is not achievable
without also breaking the "every arm's ratio is satisfied" precondition the brief asks for.

### Measurement provenance (why the per-contact sizing is not circular)

Voronoi ownership depends **only on contact XY position**, never on tip radius — position is
fixed before any radius is chosen. So the exact `a_owned_mm2` values above were obtained by
a genuine two-pass process, both passes preserved in `generate.rs`:

1. **Pass 1**: build the file with all three tips at an equal, deliberately-safe placeholder
   radius (`OWNED_TOP = OWNED_BL = OWNED_BR = D_total/3`), just to get valid, non-overlapping
   geometry through the file writer.
2. Run the real audit against that draft (`./target/release/plinth-audit ... --json`) and
   read `i2_contact_area.measured.contacts[].a_owned_mm2` back for each `contact_id`.
3. **Pass 2**: paste those three real numbers into `OWNED_TOP` / `OWNED_BL` / `OWNED_BR` as
   `const`s (with this exact provenance comment in the source), multiply each by
   `SAFETY_MARGIN = 1.35`, and regenerate the final file. This is what
   `eccentric_polygon.chitubox` actually is.

This is not fitting the fixture to the check's implementation after the fact — ownership is
pure geometry, decided in step 0 before any radius exists, and re-running the audit on the
final file reproduces the same `a_owned_mm2` values in the table above to the last printed
digit (Voronoi assignment does not depend on `r_tip`).

## What the audit says (measured, this file, 2026-08-30)

Full ten-invariant run, `./target/release/plinth-audit fixtures/eccentric_polygon/eccentric_polygon.chitubox --json`
(`RUSTC_WRAPPER="" cargo build --release`, no code owned by this fixture touched):

```
i1_seat_depth:          pass    (h_delivered=0.4500 mm; h_required 0.2403/0.3043/0.2037 mm per contact)
i2_contact_area:        pass    (all 3 contacts; a_rast >= a_required with margin, see table above)
i3_min_section:         pass    (all 3 contacts; X = a_owned/a_min = 6.56-6.79, see table above)
i4_load_ownership:      FAIL    LOAD_BOOKS_DISAGREE  d_total=36.7200  implied=49.5720  ratio=0.740741
i5_closed_load_path:    pass    (0 orphaned, 0 free ends, 0 indeterminate of 12 records)
i6_no_intersection:     pass    (0 mm3 outside-seat / buried / breach over all 12 records)
i7_island_coverage:     pass    (1 detached island above the one-pixel censor, intersected by support)
i8_no_dead_geometry:    FAIL    GEOM_DEAD_MEMBER x6  (records 1,2,5,6,9,10 -- taper+member, every column)
i9_unsupported_span:    FAIL    SPAN_UNSUPPORTED     reach=3.831 mm > allowed=1.345 mm
i10_arrangement_spread: FAIL    SPREAD_NO_EQUILIBRIUM  (THE TARGET -- see below)
```

`i10_arrangement_spread`, in full (this is the check under test):

```
checked=80  failed=80  indeterminate=0
SPREAD_NO_EQUILIBRIUM file=.../eccentric_polygon.chitubox model=0 body=0 layer=101 z_mm=-1.955
  centroid=0.000000,-0.000000 margin_mm=-0.158771 sigma_mm=0.078806 hull_vertices=3
  hull_area_mm2=2.927166 contacts=3 resisting=3 contact_ids=0,4,8
headline: "1 bodies, 80 body-layer evaluations: 0 pass, 80 refused, 0 indeterminate;
  dominant SPREAD_NO_EQUILIBRIUM x80; worst containment margin -0.1588 mm at layer 101"
```

The body is discovered as a single connected component for its entire 80-layer life
(`bodies.count = 1`, `lifetime_layers.min = max = 80`), which is exactly the box's own
physical shape (it is solid the whole time it exists as a body, and never merges into
anything else — the spec's §5 warns to report this rather than assume it, so it is
disclosed here explicitly). Every one of its 80 layer evaluations refuses identically: the
centroid is at `(0,0)` at every layer (the box's cross-section is constant), so every
layer's peel resultant sits outside the same fixed triangle by the same margin. `aff_dim=2`
confirms this is genuinely the non-collinear branch (row 5 of the outcome table:
`d < -sigma`, all 3 contacts resisting, no non-resisting member available to widen the
balanceable set) — `SPREAD_NO_EQUILIBRIUM`, not the collinear code, and not an
indeterminate. `report.spread.sensitivity.c_line_of_action_vs_contact_points = 0`: every
member here is exactly vertical, so contact points and lines of action coincide and this
fixture does not exercise that branch — it is not meant to; that is `spread_tilted_axes`'s
job, a different required fixture this brief did not ask for.

## Isolation: which other checks fire, and why

The brief for this fixture requires trying hard to trip **only** `i10_arrangement_spread`,
and where that is impossible, naming exactly which other checks fire and why each is
semantically necessary. Three others fire. I could not make any of the three go away
without breaking either the "every per-arm ratio satisfied" precondition or a hard
geometric constraint (raft-pad non-overlap), and I can show why for each.

### `i9_unsupported_span` — SPAN_UNSUPPORTED — semantically necessary, not incidental

`SPAN_UNSUPPORTED ... reach_mm=3.831333 allowed_mm=1.344976 at_mm=-3.381,2.688 ...` — the
slab's own far corner, 3.83 mm from the nearest carrier, against a 1.34 mm allowance. This
is a **genuinely different, also-true defect on the same body**, not an artifact of the
plant. `docs/specs/audit-spread-criterion.md` §2.3 is explicit that arrangement (I10) and
span (I9) "separate cleanly" and "do not overlap": arrangement is about a body that can
*rotate*, span is about a body that can *sag*. But nothing says a single body cannot be
guilty of both at once, and this one structurally is: raft pads must clear each other by
more than `RAFT_PAD_DIAMETER_MM = 2.2 mm` centre-to-centre (§2.5 constraints on any
multi-contact fixture), so three non-overlapping columns need a footprint at least a few mm
across just to seat their supports — and the measured span allowance is only **1.34 mm**.
Any slab wide enough to host three well-separated, non-overlapping support columns is, by
construction, wider than the span allowance measured from a cluster near one side of it.
This is not something better geometry can route around: it is the corpus's own registered
numbers (`RAFT_PAD_DIAMETER_MM` vs `L_SPAN_ALLOWED_MM`) in direct tension for *any* fixture
shaped like "a modest slab held by a small, physically realisable cluster of contacts" —
worth flagging as a cross-check finding in its own right (see "additional finding" below).

### `i8_no_dead_geometry` — GEOM_DEAD_MEMBER x6 — same explained tension `bare_island` already found

Every taper and member (records 1,2 / 5,6 / 9,10) reads as dead. This is the *exact*
structural tension `fixtures/bare_island/README.md` already documented in detail: each
tip's own seat comfortably passes I1 (`h_delivered=0.45 mm >> h_required`), so under
`docs/physics.md`'s three-seat rule clause (ii), the tip is **already grounded on its own
seat**, independent of anything below it. Removing its taper or member therefore never
disconnects a live TIP from ground, so R1–R4 find nothing that needs either primitive, and
they read as dead by the letter of the current rule — purely because this is a **minimal
single-branch, single-contact-per-column** fixture, the same shape `bare_island` used and
the same shape that tension is inherent to. I did not copy `bare_island`'s workaround
(deliberately under-sizing one seat so the chain matters for grounding), because that would
give this fixture's I3 table a *second*, unplanted reason to have a marginal ratio, which is
worse than an explained I8 collateral.

### `i4_load_ownership` — LOAD_BOOKS_DISAGREE — a direct, provable consequence of satisfying every per-arm ratio here

`ratio=0.740741` = `1 / SAFETY_MARGIN` exactly (`SAFETY_MARGIN = 1.35`, `1/1.35 =
0.7407407...`). This is not a leftover: it is forced. I4 clause (d) requires
`sum(a_implied)` to bracket `D_total` at a tolerance of `tol_sum`, which is float-epsilon
scale (`SUM_K * eps64 * D_total`, far below 1 mm²) — effectively exact equality, not a
design margin. But this fixture's three contacts do **not** own equal shares of the slab
(11.05 / 17.73 / 7.94 mm², forced by the triangle's real position, not a choice): sizing
each tip to its own *exact* fair share (`a_implied_i = a_owned_i`, which is what would make
I4 pass) makes that contact's declared capacity **equal**, not comfortably above, its own
narrowest-section requirement — i.e. `X = 12.0` exactly, sitting precisely on the boundary
where the raster's own quantisation loss (measured at ~1.2 % elsewhere in this project's
fixtures, e.g. `starved_patch`) tips a contact into `CONTACT_AREA_SHORT` for an *unplanted*
reason. An earlier draft of this fixture did exactly that (equal `D_total/3` shares, no
margin) and **I2 and I3 failed for real** on the most heavily-loaded contact — confirmed by
running it. Giving every arm real margin against its own real share (this fixture's whole
point, and the brief's explicit precondition) is mathematically incompatible with keeping
`sum(a_implied) == D_total` to float precision, because the three shares are unequal and
`sum(a_implied) > D_total` is exactly what over-provisioning margin on every one of them
produces. **This is a genuine, provable interaction between the arrangement question and
I4's own bookkeeping**, not a fixture defect: satisfying "every per-arm ratio satisfied,
comfortably" and "the engine's declared capacity sums to exactly the demand" cannot both
hold when the real per-contact shares are unequal, which an eccentric arrangement makes
true by definition.

## Additional finding: the crate is currently unstable across a multi-file audit sequence, unrelated to this fixture

Running the real fixture harness (`cargo test --release --test fixtures -- --nocapture`)
to confirm discovery surfaced a serious, **pre-existing** instability in shared code this
fixture does not own (`src/context.rs`, `src/run.rs`, both modified by whoever implemented
I9/I10 — see `git status` above). It does not affect the verdict on this fixture; it is
reported because the brief asks to go looking for exactly this class of defect.

**Evidence.** `./target/release/plinth-audit` on `eccentric_polygon.chitubox` alone is
correct and **fully deterministic**: five separate runs each produce a byte-identical
67200-byte JSON document, exit code 1, no panic. But `cargo test --release --test fixtures`
(single-threaded, `--test-threads=1`, ruling out a data race) panicked partway through:

```
thread 'every_fixture_trips_the_check_it_names' panicked at src/json.rs:34:9:
non-finite value reached the JSON writer; report it as a finding, never as null
stack backtrace:
   2: <plinth_audit::checks::load_ownership::LoadOwnership as plinth_audit::check::Check>::run
   3: plinth_audit::run::audit_file
   4: fixtures::run_fixture
```

— a NaN reaching `i4_load_ownership`'s own output while auditing **`shallow_seat.chitubox`,
a pre-existing fixture this run does not own**, immediately *after* this fixture's own line
printed correctly:

```
  ok       eccentric_polygon [i10_arrangement_spread] one floating slab body ...
             also failing: i4_load_ownership, i8_no_dead_geometry, i9_unsupported_span
```

That "ok" line is the harness's own `Satisfied` verdict, printed with its own collateral
report — **this fixture is confirmed discovered and confirmed satisfied** by the harness's
own logic, before the unrelated crash on a later file. I isolated the crash with a small
scratch driver (`plinth_audit::run::audit_file` called directly, in the same process, in
the same order the test binary uses) and confirmed **`eccentric_polygon.chitubox` is not
required to reproduce it**: auditing the two smallest `exports/` corpus artifacts, then
every pre-existing fixture in the checkout **excluding mine**, in sequence in one process,
reproducibly hangs (not merely panics) on `collinear_arms.chitubox` — a fixture built by
another agent, not this one — 2 runs in a row, each killed after 45–60 s of no progress.
Whether the manifestation is a hang or a NaN-panic appears to depend on exact prior
allocator/heap state (the identical sequence sometimes completes cleanly, sometimes doesn't
— nondeterministic even single-threaded, which usually means uninitialised memory or
allocator-dependent state, not a logic bug in one check's arithmetic). This blocks
`cargo test --test fixtures` from completing a full green run right now, for reasons that
have nothing to do with this fixture or the files it owns.

## Files

- `generate.rs` — the exact, reproducible construction, including the two-pass measurement.
- `eccentric_polygon.chitubox` — the planted-defect file (output of `generate.rs`, pass 2).
- `eccentric_polygon.expect.json` — the harness expectation (`i10_arrangement_spread`,
  `check_fails`, `SPREAD_NO_EQUILIBRIUM`).
