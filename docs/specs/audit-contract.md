# Audit contract — what a `.chitubox` file can and cannot be made to answer

Status: written 2026-08-30 for the standalone audit crate at `tools/plinth-audit/`.
Audience: every later agent that implements a check against emitted bytes.

**Scope.** This document says what is in the file, at which byte offset, in which
numeric type, and — for every fact the audit needs — whether that fact can be
*reconstructed from geometry* or only *read from a declared field*. It changes no
code and no emitted byte.

**The independence rule this document serves.** The audit crate has its own
`Cargo.toml`, is not a workspace member, and depends on no `plinth-*` crate and
not on `geom_kernel`. It reimplements what it needs. Every offset below is
restated here as a literal so the audit never has to `use chitubox_io::consts`.

**Three standing rules for anything built from this document.**

1. Where a fact can be reconstructed from geometry, reconstruct it. Read the
   declared field *as well*, label it DECLARED in the output, and compare.
2. A disagreement between DECLARED and RECONSTRUCTED is a hard failure.
3. Anything the audit cannot establish is a hard failure. "Not measured", "not
   implemented" and "indeterminate" never map to pass. Every row of the table in
   §3 that is marked DECLARED-ONLY is therefore also a row where the audit must
   say, in its output, that it is trusting a field it could not check.

**Provenance of the numbers below.** Everything marked *measured* was measured on
2026-08-30 by a zero-dependency Rust probe run directly over the bytes of the
committed fixtures in `scripts/tests/fixtures/chitubox/` and the emitted
artifacts in `exports/`. Everything marked *from source* is a code or document
citation with a `file:line`. Where neither appears, the statement says so.

---

## 1. The record layout

### 1.1 Container navigation — pointer chain only, never a scan

All integers are `u32` little-endian. All floats are IEEE-754 `binary32`
little-endian. There is no `f64` anywhere in the container.

Source: `docs/specs/chitubox-format.md` §1–§5; `rust/chitubox_io/src/consts.rs`.

```
header (24 B at 0)
  +0   u32   FILE_MAGIC = 0xAB231243                consts.rs:8
  +4   u32   model_count                            consts.rs:15  H_MODEL_COUNT
  +8   u32   model-table offset  (call it T)        consts.rs:16  H_OFF8
  +12  u32   settings-blob size: 0 when empty, 420 when populated
             consts.rs:23 SETTINGS_SIZE_CONST; the "constant" claim is corrected
             in chitubox-format.md §2 — treat as a two-valued field, not a pin
  +16  u32   412, pinned constant                   consts.rs:25  CONST16
  +20  u32   support-section anchor = T + 680*model_count (0 when empty)
             consts.rs:19  H_OFF20

model record (680 B stride, first at T)             consts.rs:31 MODEL_RECORD_LEN
  +0    char[]  NUL-terminated name (cap 63 B on write, consts.rs:41)
  +256  f32     translate X, mm                     consts.rs:32  M_TRANSLATE_X
  +260  f32     translate Y, mm                     consts.rs:33
  +264  f32     translate Z (lift), mm              consts.rs:34
  +268  u32     ABSOLUTE offset of this model's support index; 0 = no supports
                consts.rs:36  M_SUPPORT_INDEX_PTR
  +272  u32     ABSOLUTE offset of this model's mesh blob   consts.rs:37
  +276  u32     mesh blob length in bytes = 36 * n_triangles consts.rs:38
  +280..+680    unmapped; measured all-zero in every specimen below

support index (16 B, at the address in model record +268)  consts.rs:45
  +0   u32  n_records
  +4   u32  ABSOLUTE offset of the first 72-byte record
  +8   u32  ABSOLUTE offset of this model's support-mesh region
  +12  u32  length of that region in bytes
  invariant: mesh_off + mesh_size == index_off        (measured, §1.6)

support group header: 400 B between index end and records_off   consts.rs:47
  measured all-zero in every specimen below
```

**Measured, 2026-08-30**, over `two-supports`, `basic-cube-supports-raft`,
`cylinder-supported-bottom-connected`, `morel_mudskipper_supports` (native) and
all eight `exports/*.chitubox` (emitted): `off20 == off8 + 680*model_count` held
in every file; the 400-byte group header had **0** nonzero bytes in every file;
model-record bytes `name_nul..256` and `280..680` had **0** nonzero bytes in
every file. The audit may therefore treat a nonzero byte in either span as a
finding, not as ordinary data.

### 1.2 The 72-byte support segment record — every field

Offsets are **record-relative**. Source: `rust/chitubox_io/src/consts.rs:50-58`,
cross-checked against `docs/specs/chitubox-format.md` §5, and every field below
was decoded independently by the probe.

| rec-off | width | type | field | notes |
|---:|---:|---|---|---|
| 0  | 4  | `u32` | magic `0xEA342389` = 3929285513 | `consts.rs:10` `SEG_MAGIC`, `consts.rs:51` `S_MAGIC`. Measured present on every record of every file. |
| 4  | 4  | `u32` | segment type | `consts.rs:52` `S_TYPE`. Vocabulary in §1.3. |
| 8  | 12 | 3 × **`f32`** | `start` x, y, z — mm, model-centered frame | `consts.rs:53` `S_START` |
| 20 | 12 | 3 × **`f32`** | `end` x, y, z — mm, same frame | `consts.rs:54` `S_END` |
| 32 | 4  | **`f32`** | `r_start` — a **RADIUS**, mm | `consts.rs:55` `S_R1` |
| 36 | 4  | **`f32`** | `r_end` — a **RADIUS**, mm | `consts.rs:56` `S_R2` |
| 40 | 4  | `u32` | ABSOLUTE file offset of this record's mesh blob | `consts.rs:57` `S_MESH_OFF` |
| 44 | 4  | `u32` | mesh blob length, bytes | `consts.rs:58` `S_MESH_LEN` |
| 48 | 24 | opaque | trailing — sub-fields in §1.4 | `consts.rs:59` `S_TRAILING` |

Radius, not diameter. The units trap runs the other way in the ChituBox UI,
which shows diameters; nothing in the file is a diameter.

### 1.3 Segment type vocabulary — the names are misleading

From `crates/plinth-emit/src/lib.rs:699-710` (`segment_type`, the sole
semantic-to-wire mapping) and `docs/specs/chitubox-io.md` §4:

| wire `u32` | format name | what it actually is |
|---:|---|---|
| 7 | TIP | **a complete contact ball.** `start`/`end` are its two poles; centre = `midpoint(start,end)`; `r_start == r_end` = its radius |
| 1 | TAPER | the contact frustum, ball centre → the first structural node. Carries the declared contact depth (§1.4) |
| 9 | SHAFT | **a junction ball**, not a shaft. `start`/`end` are poles ±r along **world Z** (`lib.rs:1276`, `lib.rs:365-368`) |
| 3 | JOINER | **the structural arm** — the load-bearing member. A frustum |
| 4 | FLARE | root boss, column end → raft top |
| 6 | PAD_OR_BRIDGE | raft. One wire type for two shapes; pad has `start == end` |

**Do not infer the connective point from `start`/`end` for types 7 and 9.** For a
ball, the connective point is `midpoint(start,end)`; the poles connect to
nothing. This is the single most likely way to build a wrong graph — see §3.2.

### 1.4 The 24 trailing bytes — measured semantics

The trailing block is six `f32` slots at record-relative +48, +52, +56, +60, +64,
+68. **Measured 2026-08-30** across `exports/sphere-first-artifact.chitubox`
(emitted, 70 records), `two-supports.chitubox` (native UI-authored, 13 records)
and `cylinder-supported-bottom-connected.chitubox` (native UI-authored, 166
records):

| type | slot 0 (+48) | slot 1 (+52) | slots 2–5 (+56..+72) |
|---:|---|---|---|
| 1 TAPER | **declared contact depth, mm.** Emitted: `0.3` × 11 of 11. Native `two-supports`: `0.1`, `0.5`. Native cylinder: `0.0` × 11, `0.1` × 13, `0.2` × 2 | 0 always | 0 always |
| 6 PAD_OR_BRIDGE | bridge flag: `1.0` = bridge, `0.0` = pad. Emitted 5 bridges / 6 pads; native cylinder 25 / 26 | **`4.0` in every type-6 record of every file measured** — meaning unknown | 0 always |
| 7, 9, 3, 4 | 0 always | 0 always | 0 always |

Writers: `crates/plinth-emit/src/lib.rs:685-696` (depth via
`SupportRecord::with_contact_depth`, `rust/chitubox_io/src/types.rs:177-180`;
bridge flag and the `4.0`). Native and emitted files agree on the layout, which
is the strongest available evidence that the layout is the format's and not the
emitter's invention.

Slots 2–5 were zero in every one of the 249 records inspected across those three
files. That is not proof they are always zero. The audit should report a nonzero
value there as a finding rather than ignore it.

### 1.5 Mesh encoding

- Every mesh blob — model and support alike — is a raw `f32` triangle soup:
  **36 bytes per triangle = 9 `f32`** (`v0.xyz v1.xyz v2.xyz`). No normal, no
  attribute, no count, no header. `consts.rs:63` `MESH_BYTES_PER_TRI`.
  Triangle count = `len / 36`; `len % 36 != 0` is a hard failure.
- Source STLs are **binary**, 50 B/tri (`consts.rs:70`). Never relevant when
  reading a `.chitubox`; listed so nobody mixes the strides.
- The model mesh is stored **identity-centered on its bounding-box centre**
  (`chitubox-format.md` §4). Support record coordinates and support mesh vertices
  are in that **same model-centered frame** — `crates/plinth-emit/src/lib.rs:626-628`
  subtracts the same `center` from both.

  **Measured corroboration**, 2026-08-30: on `exports/sphere-first-artifact.chitubox`
  the model mesh bbox is z ∈ [−9.905, 9.905] and the support mesh bbox is
  z ∈ [−14.905, −7.179] — the supports sit directly under and into the model in
  the same coordinates, and the lowest support z is exactly `model_min_z − lift`
  with `lift = 5.0` read from model-record +264. Same relation on
  `two-supports.chitubox` (−17.321 − 3.0 = −20.321, lift 3.0) and on
  `exports/weeping-treant-v1.2-first-artifact.chitubox` (−45.768 − 5.0 = −50.768).

  **Consequence for the audit: no transform is needed to intersect a support
  mesh with the model mesh.** The placement fields at +256/+260/+264 apply to the
  whole assembly and cancel in any model-relative measurement.

### 1.6 Mesh-region coverage — measured

The per-model support-mesh region should be exactly partitioned by the records'
`[mesh_off, mesh_off+mesh_len)` intervals. **Measured 2026-08-30:**

| file | records | region bytes | covered by records | unreferenced | duplicate offsets | overlaps |
|---|---:|---:|---:|---:|---:|---:|
| all eight `exports/*.chitubox` | 69–2585 | 309,456 – 13,007,376 | 100 % | **0** | 0 | 0 |
| `two-supports.chitubox` (native) | 13 | 88,272 | 100 % | 0 | 0 | 0 |
| `morel_mudskipper_supports.chitubox` (native) | 243 | 2,353,680 | 2,265,408 | **88,272** | 0 | 0 |

No two records share a mesh blob in any file measured, so the audit may treat
each record's soup as private. Native ChituBox does leave unreferenced bytes
(morel: 88,272 = 3.8 % of its support-mesh region); our emitter does not. Both
are facts the audit should report rather than assume.

### 1.7 The f32 tolerance — derived, with numbers

**Every geometric quantity in the file is `binary32`.** The audit works in `f64`.
Comparing an `f64` expectation `E` against a stored `f32` `V` therefore needs a
tolerance that is not a taste judgement.

*Derivation.* `binary32` has a 24-bit significand. For `2^e ≤ |E| < 2^(e+1)` the
spacing is `ulp(E) = 2^(e-23)` and round-to-nearest gives
`|f32(E) − E| ≤ 2^(e-24) ≤ |E| · 2^-24`. So the exact single-value bound is

```
tol_1(E) = 2^-24 · |E|        with 2^-24 = 5.9604644775390625e-8   (relative)
tol_1(E) = 2^(floor(log2|E|) − 24)                                 (absolute, tighter)
```

Concrete absolute bounds, by the magnitude of the value being compared:

| |E| range, mm | half-ULP = the exact bound, mm |
|---|---:|
| [0.25, 0.5)  — a typical tip radius | 1.490e-8 |
| [1, 2)                              | 5.960e-8 |
| [32, 64)  — largest coordinate measured (treant, 54.954 mm) | 1.907e-6 |
| [64, 128)                            | 3.815e-6 |
| [128, 256) — covers the 220 mm Z build height | 7.629e-6 |

*Composite quantities.* A reconstruction built from `k` stored `f32` values whose
largest magnitude is `C` carries at most `k · 2^-24 · C` of stored-value error
(worst case, linear) or `sqrt(k) · 2^-24 · C` if the errors are treated as
independent. For a Euclidean distance between two 3-vectors (`k = 6`, but the
error enters as a vector difference of two points, so `sqrt(3)` applies):

```
tol_dist(C) = sqrt(3) · 2^-24 · C
            = 6.607e-6 mm at C = 64 mm
            = 1.321e-5 mm at C = 128 mm
```

**The constants the audit should carry** (derived above, not chosen):

| constant | value | what it covers |
|---|---:|---|
| `F32_REL` | `5.9604644775390625e-8` (`2^-24`) | any single stored value vs an `f64` expectation, as a relative bound |
| `COORD_TOL_MM` | `1.0e-5` | one stored coordinate anywhere in the 211.68 × 118.37 × 220 mm build volume (worst half-ULP 7.629e-6 at \|v\| < 256) |
| `RADIUS_TOL_MM` | `F32_REL · r`, floored at `1.0e-9` | a stored radius vs a reconstructed one |

**Measured corroboration for the coordinate bound.** §3.2 measures endpoint
coincidence on emitted artifacts: every non-exact coincidence lands at or below
`1e-6 mm`, and nothing at all lands between `1e-6` and `1e-1 mm`. `1e-5` sits
comfortably above the observed rounding and four decades below the smallest real
separation.

**What I did not derive.** A tolerance for comparing two *areas* or two
*volumes* reconstructed from f32 vertices. Those grow with the number of
triangles summed and with cancellation, and the right bound depends on the
summation order the audit uses. Whoever implements an area check must derive it
there, and must not reuse `COORD_TOL_MM` for it.

---

## 2. The truncation question

**Question.** Does the file carry a contact tip as triangles that ChituBox
renders as given, or does it also carry a parametric centre-and-radius that
ChituBox re-renders as a whole sphere regardless of the triangles?

**Answer: the file carries BOTH, and the triangles are the geometry.
Truncation is NOT impossible at the format level. A clipped ball can be
written.** The evidence and its limits follow.

### 2.1 What is in the record

A TIP record carries a parametric description *and* a mesh:

- parametric: `start` and `end` are the ball's two poles and `r_start == r_end`
  is its radius, so `centre = midpoint(start,end)`, `r = r_start`. The emitter
  writes exactly this and an independent post-write auditor re-derives and
  bit-compares it (`crates/plinth-emit/src/lib.rs:335-380`,
  `audit_sphere_record_fields`);
- mesh: a full UV-sphere soup from
  `sphere_triangles(midpoint(start,end), r_start)` — `lib.rs:634`, generator at
  `lib.rs:858-884`, which walks `0..SPHERE_LATITUDES` × `0..SPHERE_LONGITUDES`
  with **no clipping plane**. `SPHERE_LATITUDES = 8`, `SPHERE_LONGITUDES = 16`
  (`lib.rs:23-24`), giving `2 · 16 · (8−1) = 224` triangles.

**Measured 2026-08-30, emitted artifacts:** every type-7 and type-9 record in
`exports/sphere-first-artifact.chitubox` and
`exports/make_toed_slab-first-artifact.chitubox` carries exactly **224**
triangles; vertex radius about `midpoint(start,end)` ranges 0.299999–0.300001 for
a declared `r = 0.30000001192092896`; bbox extent is `2r` on all three axes for
an axis-aligned ball. The emitted ball is complete, and its mesh and its
parametric fields agree.

**Measured, native ChituBox files:** type-7 and type-9 records in
`two-supports.chitubox`, `basic-cube-supports-raft.chitubox` and
`cylinder-supported-bottom-connected.chitubox` carry **480** triangles each — a
different tessellation from ours — and are likewise complete, closed balls of
exactly `r_start` about `midpoint(start,end)`. So the tessellation is not fixed
by the format; the mesh is free.

### 2.2 The decisive evidence: ChituBox itself writes records whose mesh cannot be re-rendered from the fields

`scripts/tests/fixtures/chitubox/morel_mudskipper_supports.chitubox` is a
**native, never-regenerated, ChituBox-authored** file
(`scripts/tests/fixtures/chitubox/README.md`, Provenance). **Measured
2026-08-30, 243 records on model 0:**

- type histogram: `1 → 65`, `4 → 17`, `6 → 33`, `9 → 128`. **There is not a
  single type-7 record in the file**, yet the model is supported;
- triangles per record: type 1 ranges **320–544**, type 9 ranges **96–768**.
  A parametric re-render of a type-9 ball would produce ChituBox's own 480
  triangles, or ours' 224 — never 768, never 96;
- **210 of the 243 records are geometrically OPEN** (they have boundary edges
  under exact-bit vertex welding). No parametric primitive — sphere, frustum,
  pad — is ever open.

A record whose mesh is a 544-triangle open merged soup cannot be produced from
`(start, end, r_start, r_end)` by any re-render. ChituBox wrote that file and
ChituBox reloads it. Therefore **the mesh blob is the geometry the format
carries, and the parametric fields are metadata beside it.** A clipped ball is
representable.

### 2.3 What I could not establish — read this before acting on §2.2

Three things follow from bytes; three do not. The ones that do not are exactly
the ones a truncation decision would rest on.

1. **Whether ChituBox's SLICER rasterises the stored soup, or re-derives support
   geometry from the parametric fields before slicing. NOT ESTABLISHED.** No
   byte-level evidence bears on it. §2.2 shows the mesh must be honoured for
   *load and display* — otherwise morel could not round-trip — but slicing is a
   separate code path inside ChituBox and nothing in the file constrains it.
   Only the manual ChituBox acceptance gate (`docs/specs/chitubox-io.md` §7.6)
   can answer this, and it has not been run for this question.
2. **Whether ChituBox's support EDITOR regenerates a whole ball when a user
   selects, drags, or re-roots a support whose mesh was clipped. NOT
   ESTABLISHED.** If it does, a clipped tip survives only until the file is
   touched in the UI.
3. **Whether ChituBox validates mesh-against-parametric on load and repairs or
   rejects a mismatch. NOT ESTABLISHED**, though §2.2 argues strongly against
   any such validation: morel's own records are wildly inconsistent with their
   fields and ChituBox authored them.

**Two engineering consequences that hold regardless of 1–3.**

- **A clipped ball must be CAPPED.** An open soup has no inside. Cut the ball at
  a plane and the cut disc must be triangulated shut, or every downstream
  inside/outside test — the audit's own, and any slicer's — is undefined. Native
  ChituBox tolerates open records (§2.2), but tolerance is not a licence.
- **If truncation is ever implemented, the DECLARED/RECONSTRUCTED comparison
  rule for TIP records must be restated first.** Today the emitter's own
  post-write auditor requires `start`/`end`/`r` to describe the whole ball
  (`lib.rs:335-380`), and this contract's rule 2 (§ top) would make a clipped
  mesh a hard failure against those fields. The rule must become: *the
  parametric fields describe the ball the clip was taken from, and the audit
  reconstructs the clip plane from the mesh and checks that the retained cap is
  the ball's own surface.* Do not implement a clipped tip without changing that
  rule in the same change.

### 2.4 The lever this leaves on protrusion

Because truncation *is* possible at the format level, seat depth is **not** the
only lever. But because §2.3 items 1–3 are unestablished, seat depth is the only
lever that is known to work end-to-end today. A truncation change is gated on
one experiment: clip a tip, open the file in ChituBox Basic 1.3.0, slice it, and
observe whether the sliced solid is the clipped ball or the whole one.

---

## 3. Reconstructible from geometry vs declared-only

### 3.1 The table

RECONSTRUCTIBLE = derivable from the bytes without believing any declared field.
DECLARED-ONLY = present only as a field, or only in the sidecar (§5).
BOTH = derivable *and* declared, so rule 2 applies and the audit must compare.

| fact the audit needs | verdict | how, and at what cost |
|---|---|---|
| tip ball centre and radius | **BOTH** | reconstruct from the record's own soup (fit centre/radius to its vertices); declared as `midpoint(start,end)` and `r_start`. Measured agreement on emitted files: vertex radius within ±1e-6 mm of `r_start` (§2.1) |
| **seat depth** | **BOTH** | reconstruct: ball centre and radius from the tip soup, model surface from the model mesh (same frame, §1.5); depth = how far the ball's surface passes beyond the model surface along the contact normal. Declared: the TAPER record's trailing slot 0 (§1.4). The two are linked only through the graph (§3.2), so a graph error silently mis-pairs them |
| **contact patch, after rasterisation** | **RECONSTRUCTIBLE** | both meshes are in the file; voxelise on the printer lattice of §4. This is the check the seat-depth ruling turns on and it must not be replaced by the ideal-geometry disc `a = sqrt(2 f r h − h²)`, which is the *design* value, not the delivered one |
| arm narrowest section | **BOTH** | reconstruct: minimum cross-section of the JOINER (type 3) soup along its axis. Declared: `min(r_start, r_end)`. A frustum's narrowest section is at its narrow end, so the two should agree to `RADIUS_TOL_MM` |
| **support graph** (arm → tip, arm → junction) | **RECONSTRUCTIBLE** | endpoint coincidence, at the tolerance measured in §3.2. **No record carries a parent id, a child id, a group id, or any other topology field.** Position within the record array binds a support to a *model* (`chitubox-format.md` §5) and to nothing finer |
| load path (does every tip reach the plate?) | **RECONSTRUCTIBLE** | traverse the reconstructed graph to a type-6 raft record. Orientation is not a field; it comes from the traversal |
| **load ownership** (which model area a contact is charged) | **DECLARED-ONLY** | not in the file at all. Only in the sidecar's `contacts[]` rows (§5). An independent geometric re-attribution is possible — partition downward-facing model area among the reconstructed contacts — but that is the **audit's own** load model, not the engine's, and must be labelled as such. It can bound the engine's claim; it cannot verify it |
| model intersection / penetration | **RECONSTRUCTIBLE** | model mesh and support meshes are both present, in one frame (§1.5) |
| contact spacing | **RECONSTRUCTIBLE** | nearest-neighbour over the reconstructed tip centres |
| scar orientation (the surface a tip lands on) | **RECONSTRUCTIBLE** | the model face nearest the tip centre, and its normal, from the model mesh |
| support-material volume | **RECONSTRUCTIBLE** | signed volume of each support soup — but only for a CLOSED soup, and §3.3 shows some emitted soups are not closed on the wire |
| declared contact depth | **DECLARED-ONLY** as a field, and the field is known to lie: every emitted TAPER declares `0.3` (measured, 11 of 11 in `sphere-first-artifact`) while 11.6 % of real tips reach it |
| model placement (X, Y, lift) | **DECLARED-ONLY** | model record +256/+260/+264. Cancels out of every model-relative measurement, so nothing depends on it |
| which model a support belongs to | **RECONSTRUCTIBLE** | by position: each model's records are the contiguous block its own +268 pointer reaches |

### 3.2 The graph, and its tolerance — measured

**Yes, the support graph is rebuildable from endpoint coincidence alone.** With
two rules that are not optional:

1. **For a ball (types 7 and 9) the connective point is `midpoint(start,end)`,
   not `start` and not `end`.** The poles connect to nothing. A TIP's centre is
   where its TAPER begins; a junction ball's centre is where its arms meet.
2. **Type 6 pads have `start == end`** (measured: 6 of 11 type-6 records in
   `sphere-first-artifact`, 24 of 47 in `make_toed_slab`) and contribute one
   point, not two. Bridges have two distinct endpoints.

**Measurement, 2026-08-30.** For every non-raft record I took its two endpoints
plus, for types 7 and 9, its centre, and computed each point's distance to the
nearest point belonging to a *different* record. Distribution:

| file | points | exactly 0 | (0, 1e-7] | (1e-7, 1e-6] | (1e-6, 1e-1] | > 1e-1 |
|---|---:|---:|---:|---:|---:|---:|
| `exports/sphere-first-artifact.chitubox` (70 rec) | 144 | 59 | 6 | 21 | **0** | 58 |
| `exports/make_toed_slab-first-artifact.chitubox` (402 rec) | 871 | 356 | 44 | 125 | **0** | 346 |
| `two-supports.chitubox` (native, 13 rec) | 24 | 8 | 0 | 2 | **4** | 10 |

**On our own emitted bytes there is a clean five-decade empty band** between the
worst true coincidence (1e-6 mm) and the smallest real separation (1e-1 mm). Any
tolerance in `[1e-6, 1e-1)` mm builds the same graph.

**Recommended: `JOIN_TOL_MM = 1.0e-3` for emitted artifacts** — 1000× above the
worst observed rounding, 100× below the smallest observed real separation, and
above the `COORD_TOL_MM` of §1.7 by two decades.

**Native ChituBox files need a looser tolerance.** On `two-supports.chitubox`
four coincidences land in `(1e-4, 1e-2]` mm — two in `(1e-4, 1e-3]`, two in
`(1e-3, 1e-2]` — and the next distance is above `1e-1`. If the audit is ever
pointed at a UI-authored file, the tolerance must be **`2.0e-2` mm**. Do not use
one constant for both cases without saying which file class it is for.

**What I did not establish.** I measured the *distance distribution*, which is
bimodal with an empty band. I did not independently confirm that every pair
inside the band is a true structural join (that would require the planner's own
topology, which the audit is forbidden to consult). The empty band is what makes
the tolerance safe; the semantic claim rests on it, not on a separate proof.

### 3.3 A measured caution: some emitted sphere soups are OPEN on the wire

**Measured 2026-08-30**, exact-bit vertex welding, over emitted artifacts:

| file | records | OPEN records | detail |
|---|---:|---:|---|
| `exports/make_toed_slab-first-artifact.chitubox` | 402 | 7 | 4 × type 7 (r = 0.3), 3 × type 9 (r = 0.35, 0.8) — each 224 triangles, **32 boundary edges**, 122–123 unique vertices against the 114 a closed 8×16 UV sphere has, 0 degenerate triangles |
| `exports/make_thin_fin-first-artifact.chitubox` | 109 | 6 | 3 × type 7, 3 × type 9, same signature |
| `exports/sphere-first-artifact.chitubox` | 70 | 0 | — |
| `two-supports`, `basic-cube-supports-raft`, `cylinder-supported-bottom-connected` (native) | 13 / 71 / 166 | 0 | — |
| `morel_mudskipper_supports` (native) | 243 | 210 | merged soups, 48 boundary edges each — expected, see §2.2 |

32 boundary edges is exactly `2 × SPHERE_LONGITUDES`: the two pole fans. The
mesh region has no aliasing (§1.6), so this is not a misread — these records'
pole vertices are genuinely not bit-identical across longitudes on disk.
`crates/plinth-emit/src/lib.rs:1612` asserts every emitted soup is closed; that
assertion is about the `f64` soup, and the `f32` cast on write is where it
breaks for these records.

**I did not establish the mechanism.** The latitude table pins the poles to
`sin = 0.0` exactly (`lib.rs:829-845`), which should make all 16 pole vertices
identical, and it does for most records. Why it fails for a few is unexplained
and is worth a separate look; it is not this document's question.

**Two consequences the audit must honour.** (a) Weld with a tolerance, never
with exact bits. (b) Never assume a closed soup as a precondition — the signed
tetrahedron volume of an open soup depends on where the soup sits, so a volume
figure taken from one is meaningless. Detect openness and fail, do not compute
through it.

---

## 4. The printer raster

These are the quantities the rasterised-contact-area check uses. Every number
below is registered in the constants registry with provenance and is reachable
as `plinth_core::m0_constants()?.saturn4_ultra()` — but the audit crate must
**not** import it; the values are restated here as literals for that reason.

Profile assembly: `crates/plinth-core/src/constants.rs:267-278`, profile id
`"saturn4-ultra"`. Type: `crates/plinth-core/src/printer.rs:156-165`.

| quantity | value | type / provenance | source |
|---|---:|---|---|
| pixel pitch **X** | **14 µm = 0.014 mm** | `Measured`, vendor | `crates/plinth-core/src/constants/evidence.rs:261-266`; `docs/datasheets/printers/saturn-4-ultra-16k.md` (sources P and R) |
| pixel pitch **Y** | **19 µm = 0.019 mm** | `Measured`, vendor | `evidence.rs:154-159`; same datasheet |
| default layer height (Z) | **0.050 mm** | `Measured`, **project record — NOT a vendor default** | `evidence.rs:210-220`, evidence id `saturn4-ultra-layer-profile-plate6`, `docs/engineering-journal/reference/plate-06-results.md` §1 |
| build volume X × Y × Z | **211.68 × 118.37 × 220.0 mm** | `Measured`, vendor | `evidence.rs:269-274`; datasheet |
| LCD pixel count X × Y | **15,120 × 6,230** | vendor | datasheet, "Physical parameters" |
| release film thickness | 0.127 mm | `Measured`, installed hardware — **not peel evidence** | `evidence.rs:281-290` |
| lift default | 5.0 mm | **`Placeholder`** — the datasheet supplies no lift distance | `constants.rs:273-276`; datasheet, "Project-profile disposition" |

**The axis mapping is a project decision, not a vendor label.** Elegoo publishes
ordered triplets and never names X or Y. The project maps first→X, second→Y,
third→Z, and the arithmetic closes exactly:

```
211.68 mm / 15120 px = 0.014 mm/px = 14 µm   ✔
118.37 mm /  6230 px = 0.019 mm/px = 19 µm   ✔
```

(`docs/datasheets/printers/saturn-4-ultra-16k.md`, "Derived project axis
mapping". Swapping X and Y breaks both identities, which is why the datasheet
spells them out.)

**Derived raster quantities, for the contact-area check** (arithmetic, not new
evidence):

| quantity | value |
|---|---:|
| one pixel footprint | 0.014 × 0.019 mm = **2.66e-4 mm²** |
| one voxel | 0.014 × 0.019 × 0.050 mm = **1.33e-5 mm³** |
| pixels in a 0.394 mm-diameter tip disc (the measured median tip) | π·0.197² / 2.66e-4 = **458** |
| pixels in the smallest admitted tip, radius 0.050 mm (`evidence.rs:354-358`) | π·0.05² / 2.66e-4 = **29.5** |

The anisotropy is 1.36:1 and it is not negligible at tip scale: a contact patch
is rasterised on a lattice whose Y step is 36 % coarser than its X step, so the
delivered area of a small disc depends on where its centre falls between pixel
centres. **The check must rasterise, not scale an ideal disc area by a constant.**

**One quantity the audit also needs and which is a run input, not a printer
fact:** `process_resolution` in the seat-depth formula is `max` over the run's
own layer heights (`crates/plinth-support/src/contact.rs:283-286`), which equals
0.050 mm for a constant-height plan built from `default_layer_height_mm`. A run
with a non-constant layer plan would differ, and the file does not record the
layer plan. Treat 0.050 mm as the profile default and say so.

---

## 5. The sidecar

### 5.1 Where it is and what it is called

Two different JSON files can sit beside a `.chitubox` target, and they mean
opposite things. Both are produced by
`crates/plinth-pipeline/src/coverage_evidence.rs`.

| path | written when | meaning |
|---|---|---|
| `<target-stem>.coverage.json` | the model was **REFUSED** | coverage-refusal evidence: the worst uncovered islands. Schema tag `plinth.coverage-refusal.v2` (`coverage_evidence.rs:77`). Path fn at `coverage_evidence.rs:232-237` |
| `<target-stem>.coverage.repair.json` | the model **SEALED and emitted** | the per-contact ledger. Schema tag `plinth.coverage-repair.v1` (`coverage_evidence.rs:541`). Path fn at `coverage_evidence.rs:263-266` |
| `<target-stem>.coverage.island-<id>.svg` | refusal only | renders, evidence only (`coverage_evidence.rs:239-243`) |

**The per-contact data the audit will want is in `.coverage.repair.json`, and
only there.** Its `contacts[]` array is written for every model that seals,
including one that cost the repair controller nothing
(`coverage_evidence.rs:604-612`).

### 5.2 Schema of `<target-stem>.coverage.repair.json`

Writer: `persist_sealed_repair_ledger`, `coverage_evidence.rs:613-651`; body
struct `RepairLedgerBody`, `coverage_evidence.rs:553-584`.

```jsonc
{
  "schema":  "plinth.coverage-repair.v1",
  "model":   "<ModelId as string>",
  "commits": <usize>,   // repair steps that COMMITTED (ruling R6)
  "trials":  <usize>,   // transactions attempted, committed or not
  "report":  { /* plinth_support::RepairRunReport — rejection histogram by
                  class, wall times, rule N4's demotion pair, rows left on
                  the ranking */ },
  "steps":   [ /* plinth_support::CoverageRepairStepWire — one row per
                  transaction: the repair-pool row, its seat, every route its
                  global score was credited for, both sides of all three
                  accept clauses, the per-route deltas, the outcome */ ],
  "contacts": [ /* THE PER-CONTACT ROWS — one per ROUTED contact */ ]
}
```

`contacts[]` rows are `plinth_support::EmittedContactDisclosure`,
`crates/plinth-support/src/runner.rs:3251-3277`, `#[serde(deny_unknown_fields)]`:

| field | JSON type | meaning |
|---|---|---|
| `contact_id` | `u64` | the physical contact id, which is also its terminal id |
| `position_mm` | `[f64;3]` or `null` | where the contact sits on the model, mm |
| `carried_area_mm2` | `f64` or `null` | model area this contact is charged to carry, after every removal and repartition |
| `tip_radius_mm` | `f64` or `null` | **RADIUS** of the member end that touches the model, mm |
| `tip_diameter_mm` | `f64` or `null` | exactly `2 × tip_radius_mm` |

**`null` is load-bearing and must never be read as zero.** The type's own
documentation is explicit: `0.0 mm²` is a legal area to READ (a routed contact
charged with nothing is ordinary) and an illegal one to INVENT; `null` means
"could not be described". An audit that coalesces `null` to `0.0` converts an
undescribable contact into a compliant one. Fail on `null`, do not default it.

**Frame.** `position_mm` is in the same model-centered frame as the file's
geometry (§1.5) — this is asserted by the pipeline's own load gate, which joins
these rows against the sealed snapshot; **I did not independently measure it
against artifact bytes**, because no `.coverage.repair.json` currently exists on
disk (see §5.4). The audit's first act when it does read one should be to check
that every `position_mm` lies within the model mesh bbox plus a tip radius, and
to fail loudly if not.

### 5.3 The status of this file to the audit

**DECLARED data. Never truth.** Specifically:

- `carried_area_mm2` is the engine's own load partition. It is the one fact in
  the table of §3.1 that is DECLARED-ONLY, so the audit cannot verify it — only
  bound it, by an independent re-attribution it must label as its own.
- `tip_radius_mm` **is** reconstructible from the tip soup (§3.1), so rule 2
  applies: reconstruct it, compare against the declared value at
  `RADIUS_TOL_MM`, and fail on disagreement.
- `position_mm` is reconstructible as the tip ball centre. Same rule.
- Row count is load-bearing upstream (`runner.rs:3245-3250`) and is checkable
  here: the number of `contacts[]` rows should equal the number of type-7
  records in the artifact. A mismatch is a hard failure, not a warning.

### 5.4 What the run does NOT write, measured

- **`corpus/baseline/<stem>.pipeline.json` has no per-contact rows.** Measured
  2026-08-30 on `corpus/baseline/Sphere.pipeline.json`: the `supports` stage
  report carries only aggregates — `n_contacts`, `n_members`, `n_balls`,
  `n_unrouted`, `plate_z`, a `gates` object of 15 integer counters plus `ok`,
  `volume_ml`, `stage_timings`. The `emit` stage report carries `n_records`,
  `n_members`, `n_balls`, `n_flares`, `n_pads`, `n_bridges`,
  `support_mesh_bytes`, `plate_z`, `lift_mm`, `placement_xy`, `bytes_written`,
  `reparse_ok`, `verify_ok`. **No contact-level row anywhere.**
- **No `.coverage.repair.json` exists anywhere in the checkout.** Measured
  2026-08-30 by `find . -name '*.coverage.repair.json'` outside `target/`: zero
  hits. The eight artifacts in `exports/` have no companion JSON at all. So the
  ledger is a code path, not currently a corpus.

  **Consequence for the audit: it must run with no sidecar present.** The
  sidecar is optional input; its absence must degrade the audit to "these facts
  are unverifiable, and this model therefore fails the DECLARED-comparison
  checks", never to "these checks pass".

---

## 6. Consolidated: what could not be established

Collected here so no later agent has to hunt for the caveats.

1. **Whether ChituBox's slicer rasterises the stored soup or re-derives from the
   parametric fields** (§2.3.1). Needs the manual ChituBox acceptance gate.
   Everything about implementing truncation depends on this.
2. **Whether ChituBox's support editor regenerates a clipped tip into a whole
   ball on edit** (§2.3.2). Same gate.
3. **Whether ChituBox validates mesh against parametric on load** (§2.3.3).
   Native morel argues no; not proven.
4. **Why 7 of 402 and 6 of 109 emitted sphere records are open on the wire**
   (§3.3). The mechanism is unexplained. Measured, reproducible, unexplained.
5. **A tolerance for comparing reconstructed areas and volumes** (§1.7). Must be
   derived by whoever implements those checks; `COORD_TOL_MM` does not cover it.
6. **That every endpoint pair inside the measured coincidence band is a true
   structural join** (§3.2). The empty band is measured; the semantic claim
   rests on the band.
7. **That sidecar `position_mm` is in the artifact's frame** (§5.2). Asserted by
   the pipeline, not measured against bytes, because no sidecar exists on disk.
8. **The meaning of the constant `4.0` in trailing slot 1 of every type-6
   record** (§1.4). Present in native and emitted files alike; unmapped.
9. **Whether trailing slots 2–5 are always zero** (§1.4). Zero in all 249
   records inspected; not proven in general.
10. **The 400-byte support group header** carries no information in any specimen
    (all-zero, measured) but its semantics remain unknown — `chitubox-format.md`
    §8 item 4.

---

## 7. How the measurements in this document were made

Five standalone Rust probes, no dependencies, compiled with `rustc -O
--edition 2021` and run directly over the bytes. Each parses the container by
the pointer chain of §1.1 and never links against `chitubox_io`. They measured,
respectively: header/model/index/record decode plus per-type triangle counts and
trailing census; endpoint-coincidence distributions and per-type openness;
per-record boundary-edge diagnosis; mesh-interval coverage, aliasing and
duplicate offsets; and model-versus-support bounding boxes. They were scratch
instruments, not deliverables, and were not committed. Anything above that says
"measured" came from one of them; anything that says "from source" is a
citation; anything that says "not established" is neither.

## See also

- `docs/specs/chitubox-format.md` — the byte-level container map this document
  restates for audit use, plus the fixture-provenance table.
- `docs/specs/chitubox-io.md` — the reader/writer crate contract, its
  confidence-graded knowledge table (§8), and the manual ChituBox acceptance
  gate (§7.6) that §2.3 depends on.
- `docs/datasheets/printers/saturn-4-ultra-16k.md` — vendor record behind §4.
- `docs/physics.md` §9 (lines 358–366) and
  `crates/plinth-support/src/contact.rs:340-362` (`seat_depth_mm`) — the
  seat-depth formula the audit checks against. `docs/physics.md` §7 (line 297)
  is where the load-sized-anchor ruling ties embed depth to carried load.
