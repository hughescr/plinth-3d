# `bare_cavity_ceiling` — a hollow cavity whose ceiling has no support from the cavity's own lower interior

## What is planted, and where

One model, two disjoint solids in one mesh:

1. **The defect.** A closed hollow box: outer shell 7.0 x 7.0 x 5.0 mm
   (`x,y` in `[-3.5, 3.5]`, `z` in `[0, 5]`), resting flat on the plate
   (`translate_mm = [0,0,0]`, so `z_plate = bbox.min.z - 0 = 0`). Carved out
   of it, a 5.0 x 5.0 x 3.0 mm cavity (`x,y` in `[-2.5, 2.5]`, `z` in
   `[1, 4]`) — 1.0 mm floor, 1.0 mm walls all round, 1.0 mm ceiling.
   **No support of any kind touches the cavity**: nothing rises from its
   floor, nothing braces its walls, nothing reaches its ceiling.
2. **The witness island.** A second, fully detached 1.2 x 1.2 x 2.0 mm box
   far away at `x = 20`, floating 3.0 mm above the shared plate, carried by
   exactly one well-formed support contact down to the same plate. It shares
   no mesh connectivity, no raster footprint, and (confirmed below) no
   `i4_load_ownership` surface component with the cavity box. Its only job is
   explained in full under "The `load_ownership.rs` crash, and why a witness
   island exists" — it is not part of the planted defect.

The cavity mesh is two independent closed `box_soup` shells concatenated in
one triangle soup: the outer shell at its normal winding, and the cavity's
own shell with every triangle's winding **reversed**
(`reverse_winding` in `generate.rs`). This is required, not cosmetic — see
"How the hole is actually carved" below.

## The planted defect, in physical terms

The cavity's ceiling is a single flat slab spanning the whole 7 x 7 mm
footprint from `z = 4` to `z = 5`. Below it, from `z = 1` to `z = 4`, only
the 1 mm wall ring is cured — the 5 x 5 mm centre is open cavity. So the
layer at which the ceiling first closes over (`k = 80`, `z = 4.025 mm`,
`z_above_plate = 4.025 mm` — computed from the printer's own
`0.014 x 0.019 x 0.050 mm` grid, `RasterGrid::layer_z_mm`) is a single
connected slab whose **rim** is attached to the wall below it, but whose
**centre**, 25.14 mm² of it, has nothing under it at all. The only thing
capable of carrying that centre is the slab's own rim — the cavity's side
wall, 2.5 mm away in every direction — and nothing rises from inside the
cavity to shorten that distance. In the actual print this is a horizontal
membrane spanning 5 mm with support only at its very edge: it will sag,
peel, or fail to cure flat, exactly the "ceiling ripped loose" failure class
`docs/physics.md` §8's owner ruling exists to prevent ("the hollow must be
supported… those supports will likely land on the lower part of the interior
of the cavity" — none do, here, by construction).

## What the audit says (real output, current tree, 2026-08-30)

Built with `generate.rs`, byte-identical to the checked-in `.chitubox`
(confirmed by re-running the generator into a throwaway crate and `cmp`-ing
the output against `bare_cavity_ceiling.chitubox` — see "Reproducing" below).
Audited with the real, current `plinth_audit::run::audit_file`, not a
hand-written expectation:

```
i1_seat_depth:          pass    "1 contacts meet their own seat requirement"
i2_contact_area:        pass    "worst X delivered 7.8 against 12, median raster loss 0.095"
i3_min_section:         pass    "worst A_min 0.7502 mm2 needs 0.5000"
i4_load_ownership:      FAIL    LOAD_UNOWNED, LOAD_COMPONENT_UNSUPPORTED x2, LOAD_BOOKS_DISAGREE
i5_closed_load_path:    pass    "0 orphaned, 0 free ends, 0 indeterminate (of 4 records)"
i6_no_intersection:     pass    "outside-seat 0 mm3 ... in 0 of 4 records"
i7_island_coverage:     pass    "1 detached islands above the one-pixel censor, all intersected by support"
i8_no_dead_geometry:    FAIL    GEOM_DEAD_MEMBER x2
i9_unsupported_span:    FAIL    SPAN_UNSUPPORTED   <-- the target, and the ONLY line in it
i10_arrangement_spread: FAIL    SPREAD_INDETERMINATE x40
```

### I9 — the target, firing exactly once, for exactly the planted reason

```
checked=8  failed=1  indeterminate=0

SPAN_UNSUPPORTED layer=80 z_mm=4.025000 z_above_plate_mm=4.025000
    reach_mm=2.506000 allowed_mm=1.344976
    at_mm=-0.007000,-0.009500 nearest_carrier_mm=-2.513000,-0.009500
    carrier_kind=model region_area_mm2=25.140192 region_pixels=94512
```

Every number is exactly what the geometry predicts, computed independently
before this was run: layer 80 / `z = 4.025 mm` is the ceiling's own closing
layer (`z_plate + (80 + 0.5) * 0.05 = 4.025`); `at_mm` is the cavity's own
centre to within one pixel (`(0,0)` nominal, `(-0.007, -0.0095)` measured —
the raster's own quantisation); `nearest_carrier_mm` at `x = -2.513` is the
cavity's own `-X` wall, 2.5 mm away as designed; `carrier_kind=model` because
the only thing carrying the ceiling's centre is the ceiling's own solid rim,
not a support; `region_area_mm2 = 25.14` is the cavity's 5x5 mm footprint,
eroded by one pixel rim under full-containment fill (ideal 25.0 mm²);
`reach_mm = 2.506` matches the predicted inradius (half the 5 mm cavity
width, 2.5 mm) inflated slightly by the chamfer metric's disclosed
overestimate (§3.6); `allowed_mm = 1.344976` is
`docs/specs/audit-span-criterion.md`'s own derived `L_SPAN_ALLOWED_MM`,
echoed from `declared_inputs` and computed at run time, not hard-coded. The
headline states outright what this check does and does not establish:
*"Carrier ADEQUACY is not checked here: whether it reaches the plate is I5,
whether it is thick enough is I2/I3/I4, whether it is live is I8."*

### I7 — passes, demonstrating Finding 1's blind spot on this exact geometry

`i7_island_coverage` reports **zero** checked items for the cavity ceiling —
it never even becomes a candidate, because the ceiling's run-component at
layer 80 has attached pixels (its own rim, sitting on the wall below), and
`island_coverage.rs`'s own early exit —
`if is_attached(&comp, model_raster, k) { continue; }` — skips the *entire*
component the moment ANY pixel in it is attached. The "1 detached islands...
intersected by support" the check DOES report is the witness island's own
birth layer (correctly recognised as supported); the cavity ceiling supplies
zero rows to `measured.islands` and zero to `checked`. This is Finding 1
made concrete on the fixture it names by name in
`docs/specs/audit-span-criterion.md` §5: *"a cavity ceiling that closes over
a hollow are all invisible to the current I7."* I9 is the fix; this fixture
is the proof the fix is necessary and the proof it works.

## Isolation: I9 fires alone; three other gates also fire, and here is why each one does

`i9_unsupported_span` is the only check whose failure line names the cavity
at all — `checked=8, failed=1`, one line, the cavity's own ceiling. The three
other failing gates are explained below, split into the ones tied to the
planted defect (semantically necessary) and the ones tied only to the
witness island (an artefact of routing around an unrelated crash, explained
in the next section).

### I4 `LOAD_COMPONENT_UNSUPPORTED` on the cavity's own component — necessary, not incidental

```
component=1 area_mm2=25 demand_mm2=25 centroid=0,0,4
```

This is the SAME physical fact I9 reports — nothing supports the cavity
ceiling — read through I4's whole-model ownership partition instead of I9's
per-region geodesic-reach lens. `audit-invariants.md` §3.4's own Voronoi
partition assigns every unit of down-facing demand to its nearest contact;
a component with demand and zero contacts is definitionally
`LOAD_COMPONENT_UNSUPPORTED`, at zero censor. Two independently-implemented
checks agreeing that the same region is unsupported, from two different
mathematical definitions of "unsupported" (nearest-carrier geodesic reach
vs. Voronoi ownership), is exactly the kind of convergent evidence the
eight-invariant design exists to produce — not a fixture failing to isolate
its target.

### I4 `LOAD_COMPONENT_UNSUPPORTED` on `component=0` (the box's own bottom face) — a separate, unrelated finding

```
component=0 area_mm2=49 demand_mm2=49 centroid=0,0,0
```

This is **not** part of the planted defect and **not** an artefact of the
witness island either — it fires on the cavity box alone, with zero support
records anywhere in the file. `docs/specs/audit-invariants.md` §2.1 defines
`demand(t) = A(t) * |n . z_hat|` for every downward-facing triangle `t`,
with **no exception for a triangle that also happens to rest exactly on the
plate**. The cavity box's own bottom face (49 mm², the full 7x7 mm
footprint) is such a triangle set: it counts as 49 mm² of unmet demand under
I4's current definition, identically to a genuine mid-air cantilever, even
though physically the plate itself carries that face directly and no peel
event ever happens there (there is no `k-1` layer to peel from — the very
first exposure is against the film with the platform directly behind it, a
different physical event from every later layer's release). `i7`'s own
first-layer exemption (`grid.layer_at_or_above(z_plate)`) and `i9`'s
`ISLAND_UNSUPPORTED`-censor precedent both special-case the plate-resting
first layer; `i4`'s demand definition does not. **This is a real,
independently-reportable finding about `i4_load_ownership`'s demand
definition**, discovered incidentally by this fixture, not a defect in this
fixture or its witness island. It is not this fixture's to fix
(`docs/specs/audit-invariants.md` is not owned here), and it is reported,
not patched around: shrinking or floating the cavity box to dodge it would
also dodge the plate-resting-hollow-box case the owner's own corpus
contains.

### I8 `GEOM_DEAD_MEMBER` x2 — entirely the witness island's own taper and member, not the cavity

```
record=1 type=taper   volume_mm3=0.6695815246508794
record=2 type=member  volume_mm3=0.9424778335276408
```

Both dead-geometry findings name `record=1`/`record=2`, the witness island's
own taper and member — the cavity has zero support records, so nothing
belonging to it can appear in an I8 finding at all. This is the identical,
already-documented structural tension `fixtures/bare_island/README.md`
traced in full: `dead_geometry.rs`'s grounding predicate treats a TIP whose
own seat independently passes I1 as itself grounded (`docs/physics.md`'s
three-seat rule, clause (ii)), so in a **minimal single-branch, single-
contact** fixture the taper and member below an already-grounded tip never
"disconnect a live TIP from ground" when removed, and read as dead by the
letter of the rule. The witness island is exactly that minimal shape (one
contact, one branch), for the same reason `bare_island`'s own witness
contact was. Nothing about widening or resizing the witness chain removes
this without either (a) giving it a second branch purely to avoid I8 —
scope creep on a contact that exists only to prevent a crash — or (b)
weakening its own seat to be non-self-grounding, which would fail I1 instead
and give I9's proof of the cavity's own failure a second, unplanted reason
to fail on the same file. Neither is worth doing for a witness contact whose
only job is described below.

### I10 `SPREAD_INDETERMINATE` x40 — the model has exactly one contact in total

```
body=1 layer=60 reason=rank_deficient_load_on_axis margin_mm=-0.001 sigma_mm=0.080 contacts=1
```

`docs/physics.md`'s spread criterion needs at least two independent points to
say anything about lateral arrangement at all; the entire file has exactly
one contact (the witness island's own tip — the cavity has none). Every
`SPREAD_INDETERMINATE` line traces to `contacts=1` — an honest "cannot be
established" from too little data, per this instrument's own governing rule
that "not measured" never maps to pass, not a defect this fixture plants.
Like I8's dead-taper finding, this is a direct, unavoidable consequence of
the witness island being minimal by design (see below) and has nothing to
do with the cavity's own geometry.

## The `load_ownership.rs` crash, and why a witness island exists

The textbook shape `docs/specs/audit-span-criterion.md` §10 calls
`span_cavity_open` — this exact hollow box, with **zero** support records
anywhere in the file — was built first (`generate.rs`'s `zero_support`
variant, still present, unused by the shipped fixture). Running the real
audit against it does not produce a verdict at all:

```
thread 'main' panicked at .../src/json.rs:34:9:
non-finite value reached the JSON writer; report it as a finding, never as null
stack backtrace:
   2: <plinth_audit::checks::load_ownership::LoadOwnership as plinth_audit::check::Check>::run
   3: plinth_audit::run::audit_file
```

Root cause, `src/checks/load_ownership.rs` (not owned by this fixture, not
edited): clause (d)'s books-ratio,

```rust
let ratio = if lo > 0.0 { d_total / lo } else { f64::INFINITY };
```

`lo` sums the implied area of every *unclamped* contact; with zero contacts
in the whole model, `lo = 0.0` and `ratio` becomes `f64::INFINITY`, which is
then written unconditionally in `measured.set("books_ratio", Json::num(ratio))`
— `Json::num` asserts `v.is_finite()` and panics rather than returning a
`Result` or `Json::num_or_null`. This is **not specific to this fixture's
geometry**: any model with real down-facing demand and zero unclamped
contacts hits it, and `fixtures/long_ledge/README.md` independently found
and documented the identical panic at the identical line — cross-checked
here to confirm it is a real, general, pre-existing defect and not something
this fixture's own construction introduced.

**Confirmed against the real shared harness, not just the standalone
reproduction above.** With the pure `zero_support` shape placed at
`bare_cavity_ceiling.chitubox` (temporarily, during investigation — not the
state this fixture ships in), `cargo test --release --test fixtures --
--nocapture` panicked inside `every_fixture_trips_the_check_it_names` and
reported that ONE test FAILED, having proved nothing about any fixture
sorting after `bare_cavity_ceiling` alphabetically in that run (`bare_island`
onward never executed). Separately, in the *current* tree, with this
fixture shipped in its safe `witness` form, the same full run panics anyway
— on `long_ledge`, which independently carries the same zero-support shape
by design (`span_ledge_long`) — confirming this is shared, cross-cutting
infrastructure risk, not something contained by this directory's own choice
of geometry. **Whoever integrates the I9/I10 fixture set should treat this
as blocking**: `checks/load_ownership.rs` needs either a
`ClauseState::Indeterminate` path for `lo == 0.0` (mirroring how clause (a)
already handles a non-finite input) or `tests/fixtures.rs` needs
`std::panic::catch_unwind` around each fixture's `run::audit_file` call — see
`fixtures/long_ledge/README.md` for a more detailed proposed fix. Neither
file is owned here.

Given that, this fixture's own contribution is a witness island: one, and
only one, extra support contact, far enough from the cavity to share no
mesh component, no raster footprint, and (confirmed above) no I4 surface
component with it — so `ctx.seats` is non-empty, `lo > 0.0`, and the crash
does not fire, while the cavity itself remains **exactly** the
zero-support, textbook `span_cavity_open` shape. I8's dead-taper and I10's
rank-deficiency findings above are the (fully explained, already-precedented)
price of that one contact existing at all; nothing about the cavity's own
defect or I9's own reading of it depends on the witness island being there
in any other way.

## Harness discovery — verified, not assumed

`cargo test --release --test fixtures -- --nocapture` (run inside
`tools/plinth-audit`, 2026-08-30, current tree) discovers this fixture by
name and reports it satisfied:

```
  ok       bare_cavity_ceiling [i9_unsupported_span] a closed hollow box (7x7x5 mm outer,
           5x5x3 mm cavity, 1 mm walls/floor/ceiling) resting flat on the plate, with no
           support of any kind inside the cavity. ...
             also failing: i4_load_ownership, i8_no_dead_geometry, i10_arrangement_spread
```

This confirms two things at once, mechanically rather than by assumption, in
answer to this whole project's recurring failure mode (a harness that finds
nothing and reports green): the fixture is discovered under
`fixtures/bare_cavity_ceiling/bare_cavity_ceiling.{chitubox,expect.json}`
by the harness's own recursive directory walk, **and** `i9_unsupported_span`
really fails on it for the code named in `.expect.json`
(`SPAN_UNSUPPORTED`), not merely "some check failed". The overall
`cargo test` run for this crate still ends `FAILED` — not because of this
fixture (which prints `ok` before the crash point) but because of the
`long_ledge`-triggered `load_ownership.rs` panic documented above, further
down the same alphabetically-sorted run.

## Reproducing

```
mkdir -p /tmp/gen_bcc/src && cd /tmp/gen_bcc
cat > Cargo.toml <<'EOF'
[workspace]
[package]
name = "gen_bcc"
version = "0.1.0"
edition = "2021"
[dependencies]
plinth-audit = { path = "/absolute/path/to/tools/plinth-audit" }
EOF
cp /absolute/path/to/fixtures/bare_cavity_ceiling/generate.rs src/main.rs
RUSTC_WRAPPER="" cargo build --release
./target/release/gen_bcc out.chitubox witness      # shipped variant
cmp out.chitubox /absolute/path/to/fixtures/bare_cavity_ceiling/bare_cavity_ceiling.chitubox
# -> byte identical (confirmed 2026-08-30)
./target/release/gen_bcc out2.chitubox zero_support  # reproduces the I4 crash
```
