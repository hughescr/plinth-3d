# `long_ledge` — a 3.00 mm ledge attached along one edge, cantilevered well past the measured span limit, with no support anywhere in the file

This is `docs/specs/audit-span-criterion.md` §10's mandatory `span_ledge_long`
fixture, built geometry-for-geometry at this directory's assigned name
(`long_ledge`) rather than the spec's own fixture id — see "Naming", below.
It targets **I9 `i9_unsupported_span`**, the check that closes cross-family
review Finding 1: `island_coverage.rs`'s early exit skips a whole run
component the moment *any* pixel in it has its own column cured on the
preceding layer, so an arbitrarily long attached ledge is invisible to it.

## Finding first, because it changes how this fixture must be read

**I9 is a stub today** (`src/checks/unsupported_span.rs` returns
`CheckOutcome::not_implemented` unconditionally). This fixture is
**discovered and PENDING**, not satisfied — see "Verification", below, for
the actual harness run. That is expected and is not this fixture's defect;
per this task's own brief, a target gate that does not exist yet is a
finding to report, not a reason to weaken the plant.

**Second, more urgent finding: placing this fixture in the tree currently
makes `cargo test --release --test fixtures` PANIC.** `src/checks/load_ownership.rs`
(I4, already implemented, not a stub) crashes with

```
thread 'every_fixture_trips_the_check_it_names' panicked at src/json.rs:34:9:
non-finite value reached the JSON writer; report it as a finding, never as null
```

on **any** model with zero support records and nonzero down-facing demand
area — which is exactly what this fixture is, and exactly what three of
§10's other four mandatory "no supports" fixtures (`span_ledge_short`,
`span_cavity_open`, `span_cavity_narrow`) will also be. See "The I4 crash",
below, for the full trace and why it is not incidental to this fixture's
particular geometry. I did not edit `load_ownership.rs` — I do not own it —
and I did not weaken this fixture to route around it, per the task's own
instruction.

## What is planted, and where

One model, two mesh components, **zero support records**, built directly
with `plinth_audit::synth` (`generate.rs` in this directory is the exact
generator; see "Reproducing").

* **Spine** — a grounded block, `x ∈ [-3, 0], y ∈ [-1, 1], z ∈ [0, 2]` mm.
  `translate_mm = [0, 0, 0]`, so `z_plate_mm = mesh.bbox.min.z - translate_z
  = 0 - 0 = 0` (`resolve_plate`'s `ContractRelation` branch,
  `src/context.rs:728`): the spine's own bottom layer **is** the plate
  layer, and it needs no support record to be grounded.
* **Ledge** — one layer thick, `x ∈ [-3, 3], y ∈ [-1, 1], z ∈ [1.98, 2.03]`
  mm (`2.03 - 1.98 = 0.05 mm = LAYER_HEIGHT_DEFAULT_MM`, exactly one printer
  layer). It is sunk `0.02` mm into the spine's top rather than sharing an
  exact coincident face with it — see "Why the 0.02 mm sink", below, for why
  that is load-bearing to the fixture actually closing.

  * `x ∈ [-3, 0]`: the ledge sits directly over the spine — full footprint
    overlap, so this half is **attached** by the per-column predicate
    `is_attached` (`audit-invariants.md` §1.6 /
    `docs/specs/audit-span-criterion.md` §3.1).
  * `x ∈ [0, 3]`: the ledge projects **3.00 mm** past the spine's own edge.
    Nothing is under it (the spine ends at `x = 0`) and there is no support
    record anywhere in the file. This is the planted defect: an attached
    ledge, self-supported at one end, unsupported across a span more than
    **2.2×** `L_SPAN_ALLOWED_MM = 1.3449762463 mm` — the measured allowance
    from `docs/specs/audit-span-criterion.md` §2.6.

`sha256` of the checked-in `.chitubox` (1568 bytes):
`bc1ee450fb689e4264db19dbdb5994b1477fc38ab6b4ac4a1c9bee0bb1dea4d0`,
reproduced byte-for-byte from `generate.rs` (see "Reproducing").

### Why the 0.02 mm sink

A first draft of this generator gave the ledge an exact shared face with the
spine at `z = 2.0` (spine `z ∈ [0, 2]`, ledge `z ∈ [2, 2.05]`). Both boxes
then had vertices at the four corners `(±3 or 0, ±1, 2.0)` with **bit-identical
`f32` coordinates**, and `Mesh::weld_exact` (`audit-invariants.md` §1.4) welds
any such pair regardless of which sub-mesh it came from. That produced
`FILE_MESH_NOT_CLOSED nonmanifold_edges=1` — a file-level abort, confirmed
directly: with the file aborted, every invariant reports the uniform
abort shape (`checked == indeterminate == 1`, or `41` for the two
layer-walking checks) without any check's real logic ever running at all,
which would have hidden every finding below, including the I4 crash.

Sinking the ledge `0.02` mm into the spine's top (`z ∈ [1.98, 2.03]` instead
of `[2.0, 2.05]`) puts every ledge vertex at `z ∈ {1.98, 2.03}` and every
spine vertex at `z ∈ {0, 2.0}` — no coordinate in common, so no vertex welds
across the two boxes, and the mesh closes with `component_count = 2`,
`nonmanifold_edges = 0`, `total_undirected_edges = 36` (two independently
closed boxes, `18` edges each), confirmed below. The sink is invisible to
the raster this audit actually builds: `src/raster.rs` samples each layer at
its own **single mid-layer plane** (`z_plate + (k + 0.5) * layer_height`).
Layer 40's plane is `z = 2.025 mm`, which lies inside the ledge (`[1.98,
2.03]`) but above the spine's new top (`2.0`), so layer 40's model footprint
is the ledge's own full footprint either way — exactly as if the two boxes
had met at a bare shared face. Layer 39's plane (`z = 1.975 mm`) lies inside
the spine only. The attachment predicate at the ledge's own birth layer is
therefore unchanged by the sink: `x ∈ [-3, 0]` attached (spine present one
layer below), `x ∈ [0, 3]` unattached — the intended plant, byte for byte.

## Why this is a defect in physical terms

A cured layer with nothing under it and nothing beside it within reach does
not hold its shape between the vat's pull and the next exposure — this is
the same physics `docs/specs/audit-span-criterion.md` §1 states as its
governing question. A ledge attached at its root behaves like the plate-1 /
plate-4 coupon fingers the span constant is derived from: a single lamina
cantilevered from a rigid spine. At 3.00 mm this ledge is longer than
**every rung plate 4 recorded any failure at** (its shortest measured
failure is 2.592036 mm) and well past the bracket's conservative,
discount-and-instrument-error-adjusted end (1.3449762463 mm). On the actual
Saturn 4 Ultra 16K/Sunlu ABS-Like Black combination the two calibration
plates were printed on, this exact geometry is evidence the tip curls, sags
visibly, or peels off the film mid-print — not a hypothetical, the coupon
measured it.

## What the audit should say, once I9 is implemented

Predicted from the geometry and `docs/specs/audit-span-criterion.md`'s own
formulas — **not** measured by a running check, since none exists yet:

* Exactly **one** down-face region, on **one** layer (the ledge is one
  layer thick and nothing exists above it): `layer = 40`,
  `z_mm = z_above_plate_mm ≈ 2.025` (`z_plate_mm = 0`).
* Because the unsupported region is a uniform-width strip whose nearest
  attached material is a straight edge perpendicular to the cantilever
  direction, the true geodesic path from any interior point to that edge is
  **purely horizontal** — the 8-neighbour chamfer metric computes this
  exactly (§3.6's up-to-12.0418% overestimate applies only to paths with a
  diagonal component; this one has none). So the predicted failure:

  ```
  SPAN_UNSUPPORTED file=long_ledge.chitubox model=0 layer=40 z_mm=2.025 z_above_plate_mm=2.025
      reach_mm≈3.00 allowed_mm=1.3449762463 at_mm≈(3.0, y) nearest_carrier_mm≈(0.0, y)
      carrier_kind=model region_area_mm2≈6.00 region_pixels≈<raster count>
  ```

  for some `y ∈ [-1, 1]` — every point along the far edge `x = 3` is
  equidistant from the attachment boundary at `x = 0` at the same `y`, so
  the arg-max pixel is a tie along that whole edge; whichever the
  implementation's own tie-break rule picks is not itself a defect.
* `reach_mm` should land within `2 * d2 = 0.0472016949 mm` of `3.00`, per
  the spec table's own acceptance bound for `span_ledge_long`.
* `region_area_mm2` should land near `3.00 × 2.00 = 6.00 mm²`, less the
  raster's own full-containment quantisation loss (a few percent, the same
  direction and rough size as `bare_island`'s `6.24994` vs. `6.3372` true).

**Sabotage check S2 from the spec (§10) is directly exercisable on this
fixture once I9 exists**: raising `L_SPAN_ALLOWED_MM` past `3.00 mm` must
flip the verdict to PASS; a check that does not flip is not reading the
constant.

## Isolation — the measured collateral, and why each line is what it is

Verified by loading `plinth-audit` as a path dependency in a throwaway crate
(see "Reproducing") and running every gating check individually against this
exact file via `ModelContext::build` + each `Check::run`, wrapped in
`std::panic::catch_unwind` so a crash in one check does not hide the others.
Observed, 2026-08-30:

```
mesh closed: true   (component_count=2, nonmanifold_edges=0, total_undirected_edges=36)
z_plate_mm: 0
frame established: true

i1_seat_depth            status=pass  checked=0 failed=0 indeterminate=0
i2_contact_area          status=pass  checked=0 failed=0 indeterminate=0
i3_min_section           status=pass  checked=0 failed=0 indeterminate=0
i4_load_ownership        PANICKED: non-finite value reached the JSON writer; report it as a finding, never as null
i5_closed_load_path      status=pass  checked=0 failed=0 indeterminate=0
i6_no_intersection       status=pass  checked=0 failed=0 indeterminate=0
i7_island_coverage       status=pass  checked=0 failed=0 indeterminate=0
i8_no_dead_geometry      status=pass  checked=0 failed=0 indeterminate=0
i9_unsupported_span      status=fail  checked=41 failed=0 indeterminate=41
    AUDIT_CHECK_NOT_IMPLEMENTED file=long_ledge.chitubox model=0 invariant=i9_unsupported_span scope=41
i10_arrangement_spread   status=fail  checked=41 failed=0 indeterminate=41
    AUDIT_CHECK_NOT_IMPLEMENTED file=long_ledge.chitubox model=0 invariant=i10_arrangement_spread scope=41
```

`checked=41` for I9/I10 (the stub's `scope`) is itself a self-check: the
model spans `z ∈ [0, 2.03]`, `⌈2.03 / 0.05⌉ = 41` layers — the number the
stub's own layer count reports independently confirms the geometry is what
this README says it is.

### I1, I2, I3, I5, I6, I8 — clean, and necessarily so

Every one of these checks iterates over support **contacts** or support
**records** (`i1`/`i2`/`i3`: per contact; `i5`/`i6`: per record or record
pair; `i8`: per record, role-walking the support graph). This file has
zero support records, so each has literally nothing to examine:
`checked = 0`, `failed = 0`, `indeterminate = 0` → `status = pass`
(`CheckOutcome::status`: pass iff `failed == 0 && indeterminate == 0`,
`src/check.rs`). This is not a design accident of this fixture — **any**
fixture with genuinely zero support records will report the identical
`checked=0` pass on these six checks, by the same mechanism. It is a vacuous
pass, not a defect in these checks (there is truly nothing there for them to
examine), and it is exactly the collateral shape the spec anticipates for a
"no supports" fixture.

### I4 `load_ownership` — a crash, confirmed necessary for this fixture's shape, not incidental

`i2` and `i3`/`i1` are per-contact and vacuous with zero contacts, but I4
also computes one **whole-model** quantity: `d_total`, the reconstructed
down-facing demand area from the mesh alone, independent of contacts. This
model's ledge underside is real down-facing geometry, so `d_total > 0`.
Meanwhile `lo` (clause (d)'s inversion-implied area lower bound,
`src/checks/load_ownership.rs`) sums `x_allowed * pi * r_tip^2` over every
**unclamped** contact — with zero contacts, `lo = 0.0` exactly, and
`hi = lo + 0 = 0.0` too. `clause_d_books(d_total, 0.0, 0.0)` then returns
`Fail` (`d_total` is nowhere near `0 ± tol`), and the branch that reports
`LOAD_BOOKS_DISAGREE` computes:

```rust
let ratio = if lo > 0.0 { d_total / lo } else { f64::INFINITY };
...
("ratio", num(ratio)),   // -> Json::num(f64::INFINITY) -> assert!(v.is_finite()) -> panic
```

(`src/checks/load_ownership.rs`, the clause-(d) block; `Json::num`,
`src/json.rs:34`.) This is **not specific to this fixture's geometry** — it
fires on any model with `d_total > 0` and zero support contacts, which is
precisely what three of `docs/specs/audit-span-criterion.md` §10's other
four *required* "no supports" fixtures (`span_ledge_short`,
`span_cavity_open`, `span_cavity_narrow`) will also be, the moment they are
built the way the spec describes them. I traced this rather than routing
around it (e.g. by adding a token support contact, which would falsify "no
supports" and no longer be this fixture) because the task instructs against
weakening a fixture to dodge a defect it legitimately found. I do not own
`src/checks/load_ownership.rs` and made no edit there; a minimal fix looks
like reporting a dedicated code (or `Json::num_or_null`) when `lo <= 0.0`
rather than feeding `f64::INFINITY` to a writer whose contract explicitly
forbids it.

**Confirmed against the shared harness, not just my own scratch crate**:
placing this fixture in `fixtures/long_ledge/` and running
`RUSTC_WRAPPER="" cargo test --release --test fixtures -- --nocapture`
reproduces the identical panic inside `every_fixture_trips_the_check_it_names`,
which then reports `FAILED` for that test and never reaches `shallow_seat`
or `starved_patch` (sorted after `long_ledge` alphabetically in
`load_expectations`'s directory walk) in that run. The other six tests in
the file are unaffected (`cargo test` isolates each `#[test]` fn; this crate
does not set `panic = "abort"`).

### I7 `island_coverage` — a direct, first-hand confirmation of Finding 1

I7 is **already implemented**, not a stub, and it reports `checked=0,
status=pass` on this exact 3.00 mm unsupported cantilever. This is not a
gap in my measurement — it is Finding 1 observed directly: `checked` in
`island_coverage.rs` counts *detached islands found*
(`src/checks/island_coverage.rs:136`, `out.checked += 1` inside the branch
that already established zero attached pixels), not "down-face regions
examined". Because this ledge is attached along its root edge, it is never
a "detached island" by I7's definition at all, so I7 never even enters the
counting branch — `checked` stays at its initial `0` and the check reports
a clean pass on a model whose largest unsupported span is more than 2.2×
the measured coupon limit. This is the exact defect
`docs/specs/audit-span-criterion.md` §3.5 names ("Finding 1's defect,
closed explicitly") and the exact one this task's brief describes ("the
island-coverage gate is nearly empty ... an arbitrarily long attached
ledge ... can be unsupported across its whole span and still pass"),
reproduced here on a purpose-built, minimal fixture rather than argued from
the source comments alone.

## Naming

This task's brief assigns the directory name `long_ledge`. The fixture
table in `docs/specs/audit-span-criterion.md` §10 (which this fixture
satisfies geometrically) calls the same shape `span_ledge_long` and states
"the harness must assert their presence by name" for that exact id. Built
here under the assigned name per this task's explicit instruction; whoever
reconciles the spec's mandatory-fixture-by-id requirement against the eight
independently-named fixtures this workflow produced is better positioned
than I am to decide whether a rename or an alias is needed — reported here
rather than silently renamed, since renaming would also change this
fixture's own file paths outside a review's visibility.

## Reproducing

```sh
mkdir -p /tmp/long_ledge_check/src/bin && cd /tmp/long_ledge_check
cat > Cargo.toml <<'EOF'
[workspace]
[package]
name = "long_ledge_check"
version = "0.1.0"
edition = "2021"
[dependencies]
plinth-audit = { path = "/absolute/path/to/plinth-3d/tools/plinth-audit" }
EOF
cp /absolute/path/to/plinth-3d/tools/plinth-audit/fixtures/long_ledge/generate.rs src/bin/gen.rs
RUSTC_WRAPPER="" cargo build --release
./target/release/gen long_ledge.chitubox
shasum -a 256 long_ledge.chitubox   # compare against this README's hash

# Then, to see the per-check breakdown (including the I4 panic, caught):
# write src/main.rs to open the file with plinth_audit::reader::ChituboxFile::open,
# build a ModelContext with plinth_audit::context::{AuditContext, ModelContext},
# and call each plinth_audit::checks::gating() check's .run(&ctx) inside
# std::panic::catch_unwind, printing status/checked/failed/indeterminate and
# any failure messages per check.

# To see the shared harness's own view (the crash as `cargo test` observes it):
cd /absolute/path/to/plinth-3d/tools/plinth-audit
RUSTC_WRAPPER="" cargo test --release --test fixtures -- --nocapture
```
