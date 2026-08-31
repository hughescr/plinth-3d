# plinth-audit

The emitted-bytes print-candidate audit. Reads **only** the bytes of a
`.chitubox` file and grades whether the file is worth printing.

Specifications: [`docs/specs/audit-contract.md`](../../docs/specs/audit-contract.md)
(what the bytes can answer) and
[`docs/specs/audit-invariants.md`](../../docs/specs/audit-invariants.md) (the
eight gating invariants, the report, the output schema). **Where an engine
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

Exit code is 0 only when every model passed all eight gating invariants.

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
