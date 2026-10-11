# plinth-audit

The emitted-bytes print-candidate audit. Reads **only** the bytes of a
`.chitubox` file and grades whether the file is worth printing.

Specifications: [`docs/specs/audit-contract.md`](../../docs/specs/audit-contract.md)
(what the bytes can answer) and
[`docs/specs/audit-invariants.md`](../../docs/specs/audit-invariants.md) (the
ten gating invariants, the report, the output schema). **Where an engine
source and `audit-invariants.md` disagree, that file governs.**

## Independence, and why it is not negotiable

Standalone crate, own `Cargo.toml`, **not** a workspace member, **zero**
dependencies — no `plinth-*` crate, no `geom_kernel`, no external crate at all.
SHA-256, JSON reading and JSON writing are hand-rolled here for that reason.

The engine's own records are known to lie. An audit that consumes the engine's
disclosures inherits every one of its defects. **Wanting to import an engine
type is the signal to reimplement.**

## Build and run

```sh
cd tools/plinth-audit
cargo build --release          # 1.8 s from clean, no dependency graph
cargo test  --release          # unit + fixture + real-artifact suites

./target/release/plinth-audit ../../exports/sphere-first-artifact.chitubox
./target/release/plinth-audit <file> --json --out-dir /tmp/audit
./target/release/plinth-audit <file> --dump-graph        # diagnostic, decides nothing
```

Exit code is 0 only when every model passed all ten gating invariants
(`support_invariants_pass`); that is not a claim the print is safe.

## `geometry`: head, intrusion, junction and contact-angle numbers

```sh
plinth-audit geometry <file.chitubox>... [--json] [--threads <n>]
```

Measures from the emitted file only (record headers, record meshes, model
mesh); it grades nothing and exits 0. Several files print each summary and one
baseline table row per file. Code: `src/geometry/`.

1. **Heads** (`heads.rs`): a tip whose taper/member chain reaches a joint ball.
   Ball radius and signed centre height over the model, link length (axial
   length from the ball surface to where the drawn radius reaches the ball
   radius; 0 when it already does), cone-start ring (24 samples) distance to the
   model, joint radius, steepest post lean, phi'. Where two chains reach one
   joint, the shorter is the head.
2. **Intrusion** (`intrusion.rs`): header surface of every taper, member, flare
   and joint sampled (pre-trim) against the model solid. `depth_mm` excludes
   the record's own contact ball sphere; `depth_with_ball_mm` excludes nothing
   (what the v19 diagnosis measured).
3. **Junction topology** (`topology.rs`): per record mesh (1e-5 weld) open and
   non-manifold edges, degenerate triangles, signed volume; end-ring mismatch
   of collinear equal-radius neighbours; frusta under one pixel; flat shoulders
   (radius step over one pixel on a straight chain); landed branch ends (free
   end centre inside another solid) with ring vertices outside every other solid
   and the largest proud distance, against the union and against the host alone;
   smallest lateral facet n_z of Member records whose mesh is not their frustum.
4. **Straight contacts** (`contacts.rs`): angle between the shaft and the
   area-weighted normal within `max(2R, 0.15)` mm, the skim law
   `(g + r_t)(sec x - 1) > layer + pixel` (registry values), shaft clearance
   over the skim zone.

Batch shape: one welded model mesh and grid per file; per record, one triangle
fetch per run of sample rings; records run in parallel (`PLINTH_THREADS`).

## `compare`: the similarity gate between two emitted files

```sh
plinth-audit compare <ref.chitubox> <cand.chitubox> [--out-dir D] [--json] [--band N] \
    [--ref-log F --cand-log F] [--ref-poses F --cand-poses F] [--require-logs] \
    [--assume-consumer-matched] [--no-geometry] [--threads N] [--no-flicker] [--low-memory] \
    [--declared CLASS] [--declare-change WORD[,WORD]] [--full-listing] [--probe x,y,z,r]
```

Replaces the sha256 gate for output-changing lanes (`ROUND-PLAN-v2.md` §1; owner
decisions O1-O4 of 2026-10-10). Reads only the emitted bytes, the engine `.log` and
`poses.tsv` (found by the run-recipe layout when not given). Code: `src/compare/`.

1. **Rasterise what prints.** Model and supports are rasterised from the EMITTED
   triangle meshes on the printer grid. The support raster is the union of every
   record's mesh (`RunRaster::from_mesh_unclosed`, non-zero winding), never the
   header primitives.
2. **Class first.** *Pose* from the committed transform (`poses.tsv`/log text and the
   file's translate; the emitted frame only when no text exists), *mesh* separately.
   A regression overrides either class.
3. **Tier 0** valid file (parse, model count, container round trip, plate/frame, mesh
   against header within one voxel of the reference's own record). **Tier 1** H1
   floating material, H2 island support, H3 model drift, H4 every audit invariant
   I1-I10 matched per item (new failing item, matched item over 10 % worse, an item
   the reference evaluated left unevaluated), H5 engine-log counts, H6 heads.
   **Tier 2** N1 volume, N2 contacts, N3 sections along each contact's path, N4 thin
   joins, N5 bracing (posts and the log's tied-post sway). EQUIVALENT is computed.
4. **Exit codes** (most severe wins): 1 REGRESSION, 3 INDETERMINATE, 2 POSE-CHANGED,
   4 MESH-CHANGED, 10 PLAN-CHANGED, 0 IDENTICAL or EQUIVALENT, 64 usage or I/O error.
   `verdict.json` carries every class and every row; a failing verdict blocks
   promotion and never deletes the emitted file.
5. **INDETERMINATE until the consumer match (G5):** raster structural checks (H1, H2,
   N4) on a model mesh that does not close (Cylinder) and at the BCO v22 junction near
   r83. Nothing here has been matched to a ChituBox slice; `--assume-consumer-matched`
   lifts the rule once it has.

6. **Flicker allowance (G6, 2026-10-11; report `perf-2026-10-09/wave1b/G6-REPORT.md`).**
   An audit item that sits near its own threshold crosses it under noise that changes
   nothing physical. The gate absorbs such a crossing only when all of these hold: the
   design is unchanged beside it (no record present in one file only within 0.15 mm);
   its severity is inside the band measured for its code (`src/compare/consts.rs`,
   "flicker allowance": every number there carries its provenance and was read off
   jittered pairs run with `--no-flicker`); and the invariant has absorbed no more
   crossings than the noise ever produced (3.1 % ... 4.8 % of its checked items, rounded
   down, so a model with fewer than 21 items absorbs none). The same measured noise sets
   the floors of N3 (a section that moved under one voxel did not change), T0 (growth of a
   mesh-versus-header gap that already exists), H1 (a newly floating piece made only of
   material already inside the 1-voxel band, when no drift beyond the band exists), H2, H6.
   `--no-flicker` switches every allowance off (the strict reading; the sabotage case).
   What it cannot do: noise of 1e-3 mm on Deinonychus turns short, wide members and moves a
   contact's area as far as a halved contact would, so it stays REGRESSION there (report).
   A real change inside the bands that touches few enough items is invisible by design;
   the blessed-set comparison bounds the drift of a series of them.
7. **Cost.** The ownership partition of I2/I3/I4 was a scan of every contact for every
   subdivision sample; it now charges a whole triangle to the one contact that can own
   it and scans only the contacts a straddling sample can still choose among (same samples,
   same owners, tested against the old scan). I3 evaluates its contacts on several
   threads (`PLINTH_THREADS`), in contact order, so the outcome is the sequential one.
   The model raster is built once per file and shared with the audit. A fast-set model
   compares in at most 12 s (the whole set in 62 s); the table is in the report.
   `--low-memory` analyses the two files one after the other: the peak of the largest
   model (make_laid_tube) falls from 5.4 to 3.2 GB for 2 s more.

8. **Declared changes and the false alarms of the first real use (G7, 2026-10-11; report
   `perf-2026-10-09/wave1b/G7-REPORT.md`).** The first lane to change output on purpose (Q12: lean
   limit plus twin merge) was refused on rows that measured the change, or that measured the engine's
   own drawing, and not a loss. Four fixes, each at its cause:
   - **H1 matches floating pieces, not components.** The floating material of each file is a raster.
     Candidate floating voxels more than one voxel from the reference's floating voxels are new; a 3D
     piece that shares at least 8 voxels with the reference's floating material is the reference's
     piece, and its growth (its break moved) is excused up to max(64 voxels, 10 % of the shared part).
     A piece that grew more, and a piece the reference does not have, are judged by the old limits
     (a component of 16 px, 64 voxels in all).
   - **T0 reads the mesh against its header with the model's cut left out.** A record that ends in
     the model is trimmed by it: mesh vertices on the model surface, and a header end inside the
     model, are the model's and not a disagreement. A mesh that covers its header (the junction hulls
     of the owner's v18 ruling are not on their headers) is measured by what its header's rings stick
     out of it. A record the reference does not have, or one that was already off its header there,
     may be as far off as the reference's own worst of its kind (plus 25 %); the share of records past
     one voxel may rise by 25 % plus 2 records. A good record that gets worse still fails.
   - **I5 grounds a member that stands on the model.** The owner's ruling: a valid seat is the plate,
     the model surface (a cavity wall included) or another support. A MEMBER that no ground reaches
     and whose cured voxels, grown by one voxel, meet the model's cured raster is grounded. A tip with
     an invalid seat is not (that is I1), nor is a taper beside one.
   - **`--declared CLASS` and `--declare-change contact-merge`.** `--declared` takes the words of
     `bench.sh --declared` (IDENTICAL, EQUIVALENT, PLAN-CHANGED, MESH-CHANGED); it is recorded in
     `verdict.json` and a `C.declared` row says whether the computed verdict is within it. The exit
     code stays the computed verdict's; `bench.sh` judges. `--declare-change contact-merge` (needs
     PLAN-CHANGED or MESH-CHANGED) widens N2 only: a reference contact with no match at 0.1 mm is a
     merge when a candidate contact lies in its column (balls overlap in plan, at most 0.5 mm apart in
     height), and a new contact is the survivor of a merge when a reference contact lies in its column.
     Only contacts not explained that way count against N2's 5 % limits, so a contact lost with no
     survivor beside it is still a finding. `bench.sh --declare-change` passes both flags.
   - **Triage tools.** `--full-listing` lists every item behind every row (the default keeps the first
     few) and says, for each audit item, whether the design changed beside it and, for each
     floating piece, whether the reference floats it and which records exist in one file only
     there. `--probe x,y,z,r` prints the support records of both files near a point (`any` for z
     ignores height) with their match in the other file.

Test instruments: `plinth-audit mutate <in> <out> <spec>...` plants one change in a
real file (jitter, `cjitter` (the mesh follows its header, as an engine's output does),
one-ulp, f16, mesh shrink, scale a record, remove, move, float, rotate, split the model;
`jitter`, `cjitter` and `ulp` take a seed); `plinth-audit stl-jitter <in.stl> <out.stl>
<rot-rad> <scale-ulps>` makes the input for the real-engine jitter baseline. Fixtures and
the sabotage (band 0 must fail the jitter fixture) are in `tests/compare.rs`.

## Implementing one invariant

Edit **exactly one file** under `src/checks/`. `src/checks/mod.rs` already
declares and registers all nine modules, so nobody touches a shared list. Each
stub's header carries its own brief: what it must establish, what is already
built for it, and the failure codes it emits.

Keep `id`, `title` and the struct name — they are the JSON schema key and the
summary label. Replace the body of `run`.

## The API you write against

### `ModelContext` — everything a check gets

Built once per model; expensive products are lazy and shared.

| member | what it is |
|---|---|
| `model_mesh: Mesh` | welded model mesh, exact `f32` bit weld (§1.4) |
| `z_plate_mm`, `z_plate_source` | the plate, and where it came from — see below |
| `grid: RasterGrid` | the printer lattice for this model |
| `primitives: Vec<Result<Primitive, GeomError>>` | one per record, parallel to `model.records`; an `Err` is I8's raw material |
| `graph: SupportGraph` | the reconstructed support graph |
| `seats: Vec<Seat>` | one per type-7 record — **the shared seat reconstruction** |
| `demand() -> &DemandSet` | the demand set `D` of §2.1 |
| `ownership() -> &Ownership` | the complete Voronoi partition of §2.2 |
| `a_owned_mm2(contact_id)` | `A_owned(c)` for one contact |
| `support_raster()`, `model_raster()` | cured rasters, full containment |
| `record_raster(record)` | one record's own cured solid (not cached) |
| `frame_check() -> FrameCheck` | the plate reconstruction and its cross-checks |
| `contact_sites()`, `live_primitives()`, `seat(id)`, `primitive(i)`, `inputs()`, `file()` | accessors |

Every lazy product returns a `Result` or carries an explicit failure field.
**Never absorb one into a pass** — report the named reason and fail.

### `Seat` — do not recompute `h_delivered`

I1, I2, I3, I5 and I6 all need the delivered seat depth. It is computed **once**
in `src/seat.rs` and shared, because four private answers would grade four
different solids. Carries `p0`, `u`, `centre`, `r_ball_mm`, the three-way tip
radius comparison (`r_tip_a/b/c`, `r_tip_spread_mm`, `r_tip_agrees`),
`h_delivered_mm`, `entry_point_mm`, `entry_normal`, `entry_triangle`,
`entry_component`, `entry_on_mesh_boundary`, `h_perp_mm`, `abs_cos`,
`sin_theta`, `mean_normal`, `normal_disagreement_deg`, `normal_ambiguous`,
`h_required_mm`, `binding_term`, `h_terms_mm`, `declared_depth_mm`, and
`indeterminate: Option<&'static str>` — the exact `reason=` token to report.

### `SupportGraph` — ambiguity is represented, never resolved

Rebuilt from endpoint coincidence. Two rules that are not optional: a ball's
connective point is `midpoint(start, end)` and **not** its poles; a type-6 pad
contributes one point and a bridge two.

`points`, `joins`, `neighbours_of(r)`, `component_of_record`,
`taper_for_tip(model, tip) -> Link`, `distance_to_ground(&is_ground)`,
`ground_reachable_without(removed, &is_ground)`, `ambiguities`,
`ambiguities_for(r)`.

`Link` is `Unique | Ambiguous | Missing` — only `Unique` may be acted on.
`Ambiguity` is `MultipleCandidates | NearBand | Unjoined | TipTaperNotUnique`.
**Every one must be treated as `indeterminate` by its consumer.**

### `RunRaster` — one lattice, run-length, full containment

`from_primitives`, `from_mesh`, `intersect`, `union`, `difference`,
`volume_mm3`, `volume_ml`, `layer(k)`, `layer_area_mm2(k)`, `layer_components(k)`,
`voxels_in_box(&Aabb)`, `contains_voxel`, `contains_point`, `layer_range`.
Every constructor takes a `Budget` and returns `Err(BudgetExceeded)` rather than
truncating — report it as `raster_budget_exceeded` and **fail**.

A per-pixel loop over a 16K grid is a defect, not an implementation choice.
Work on runs.

### `Mesh`, `Primitive`, `vec3`

`Mesh`: `weld_exact`, `weld_tolerant`, `closure`, `component_of_tri`,
`component_volume_mm3`, `point_inside`/`points_inside` (ray parity, 16 golden
directions, `Inside::Indeterminate` never passes), `nearest_surface(_batch)`,
`ray_crossings`, `ray_first`, `axis_crossing`, `triangles_in_box`,
`closed_volume_mm3`, `normal_agrees_with_solid`.

`Primitive`: `from_record` (handles the type-6 units trap), `aabb` (tight),
`volume_mm3`, `signed_distance`, `contains`, `centreline`, `radius_at`,
`x_chord`, `separation_mm`, `verify_against_soup`.

`vec3`: vector algebra, `point_triangle_closest`, `ray_triangle`,
`segment_segment_closest`, `Aabb`, `golden_spiral_direction`.

### Reporting a result

```rust
let mut out = CheckOutcome::new(scope);
out.fail(Failure::line(CODE, &ctx.file(), ctx.model_index,
                       Some(contact), Some(record),
                       &[("a_rast_mm2", format!("{v}"))], values_json));
out.indeterminate(Failure::line(CODE_INDETERMINATE, /* ... */
                       &[("reason", "seat_indeterminate".into())], j));
out.measured.set("headline", Json::str("worst X delivered 42.1 against 12"));
```

`Failure::line` is the only place the failure-line format lives, so the log
string and the JSON row cannot drift apart. `status` is derived: **`failed > 0`
or `indeterminate > 0` means `fail`, and there is no third value.**

Fill `measured` with the quantities your failure lines name, **for every item
checked and not only for failures** — a passing invariant that reports no
numbers is not evidence.

## Fixtures

`fixtures/<name>.chitubox` + `fixtures/<name>.expect.json`, one pair per
fixture so parallel agents never collide. Format and semantics are documented
in `tests/fixtures.rs`. Build the file with `plinth_audit::synth`, which writes
a valid container and validates nothing, so the defect you plant stays planted.

Ship at least one **negative control** (`"expect": "check_passes"`) or a check
that fails everything looks perfect.

While your check is a stub the harness reports its fixtures **PENDING**, never
satisfied: `AUDIT_CHECK_NOT_IMPLEMENTED` is refused as an expected code.

## Findings this scaffold already produced

Recorded here because they change how the specs read, not because the scaffold
decided anything.

1. **`z_plate = -translate_z` (invariants §1.2) does not reproduce the plate.**
   On `sphere-first-artifact` the raft bottoms out at `-14.9054537` mm,
   `-translate_z` is `-5`, and the contract's §1.5 relation
   `bbox.min.z - translate_z` is `-14.9054537`. The audit uses the contract
   relation and gates on physics instead — *no support solid may reach below
   the plate* — which holds for native and emitted files alike.
2. **Native ChituBox writes type-6 raft records at `z = 0`** while the rest of
   its support tree is in the model-centred frame. Reported as
   `frame_check.raft_agrees = false`, not gated.
3. **`mesh_off + mesh_size == index_off`, not `records_off`.** The records
   follow the index plus its 400-byte group header: `records_off = index_off +
   416`. Verified: `894208 + 416 = 894624`.
4. **Type-6 `r1`/`r2` are not radii.** A pad's `r1` is its DIAMETER and `r2` its
   HEIGHT; a bridge's `r1` is its WIDTH. The contract's §1.2 field table calls
   both slots radii, which is right for types 1/3/4/7/9 only.
5. **The engine seats every contact exactly on a model VERTEX.** The axis ray
   leaves through a point shared by six triangles, 5e-8 mm from their common
   vertex, on 9 of 9 embedded tips of the sphere. The distance is determined;
   only the normal is ambiguous, so `Mesh::axis_crossing` resolves the crossing
   and hands the normal question to §3.1's own 5-degree cross-check.
6. **§1.4's "soup bbox must equal the analytic bbox" cannot hold for a faceted
   soup.** A 16-gon inscribed in a 0.5 mm circle is short by 9.6e-3 mm, four
   decades above `tol_geom`. `MeshAgreement` gates on overflow and reports
   shortfall.
7. **A frustum's AABB is not the two-sphere box.** Using the loose box put every
   vertical flare a full radius below the plate and fired the frame check on
   every file in the corpus.
