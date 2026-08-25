# CHITUBOX Basic `.chitubox` project format — comprehensive map

Status: reverse-engineered 2026-07-23 by byte-level differential analysis of a
purpose-built corpus (empty / two-model / two-model-supported / three transform
probes) plus single-model cross-checks (`morel.chitubox`). Supersedes and cites
`skills/chitubox-support-injection/references/chitubox-format.md` (the earlier
"support layer" note), which remains correct on the 72-byte segment-record format
and injection recipe; this document adds the container, model table, mesh encoding,
and transform storage, and corrects the multi-model support layout.

All integers little-endian. All floats IEEE-754 `float32` LE. Offsets are absolute
file offsets unless noted "record-offset". Provenance script: `tmp-recon/recon.py`.

Corpus note: the owner's stated file sizes were stale; every claim here is keyed to
the *content* of the files as they exist (sizes in the tables are measured).

---

## 1. Container layout

```
[0            ] header                    24 bytes (fixed)
[24 .. 420    ] settings blob             396 bytes (all-zero in an empty project)
                 -> [0..420] together = the 420-byte "empty project" baseline
[420 .. T     ] preview/thumbnail blob    present only when >=1 model; raw, no image header
[T   .. T+680n] model table               n * 680-byte model records   (T = header off8)
[T+680n .. M0 ] small filler (zeros)      (support-section anchor off20 = T+680n)
[M0  .. EOF   ] per-model data regions, model 0 then model 1 ...:
                 [ model mesh blob    ]   (mesh_size bytes, 36 B/tri)
                 [ support mesh blobs ]   (only if the model has supports)
                 [ 16-byte support index ] (only if supported; pointed to by model-record off268)
                 [ 400-byte support group header ] (only if supported; all-zero in this corpus)
                 [ n_sup * 72-byte segment records ] (only if supported)
```

Empty project (`empty.chitubox`, 420 B) is exactly `[header][settings]` with
model_count = 0 and off8 = 420 (= EOF). This is the embeddable default template.

Confidence: **high** (all offsets derived and cross-checked on 7 files incl. morel).

---

## 2. Header (24 bytes, offset 0) — confidence high

| off | type | value / meaning | evidence |
|----:|------|-----------------|----------|
| 0  | u32 | magic `0xAB231243` (2871202371) | identical in all 7 files |
| 4  | u32 | **model count** | empty=0, sphere-cylinder=2, morel=1 |
| 8  | u32 | **model-table offset** (`off8`, T above) | empty=420=EOF; = start of 680-byte records |
| 12 | u32 | settings-blob size = `420` (0x1A4) when populated, **`0` in an empty project** | corrected during Rust impl 2026-07-23: NOT a constant — `empty.chitubox` holds 0 here, every populated file holds 420 |
| 16 | u32 | `412` (0x19C), constant (settings-related) | all files incl. empty |
| 20 | u32 | **support-section anchor** = `off8 + 680*model_count` | empty=0; else exactly end of model table |

Derived invariant: `off20 == off8 + 680*model_count` (empty writes 0). Confirmed on
every populated file (2-model diff = 1360 = 2*680; morel diff = 680 = 1*680).

**Correction (Rust impl, 2026-07-23):** field 12 is not the pinned constant the
first recon claimed. `empty.chitubox` carries `0`; all populated specimens carry
`420`. `chitubox_io` therefore stores/re-emits field 12 verbatim (preserving
byte-identity) and, on a from-scratch build, flips it from 0 to 420 when the
first model is added — matching every native populated file. Only field 16 (412)
is treated as a pinned drift-detector constant.

---

## 3. Model table — 680-byte records at `off8`, one per model — confidence high

One record per model, contiguous, stride **680 bytes**.

| record-off | type | field | notes |
|-----------:|------|-------|-------|
| 0   | char[] | model name, NUL-terminated | e.g. `"Sphere.stl #2"`; the `#N` is an instance id that ChituBox re-numbers on every save (do not treat as stable) |
| 256 | f32 | **translate X** (plate placement) | sphere-moved: 0 -> 10.5 |
| 260 | f32 | **translate Y** | sphere-moved: 0 -> 5.25 |
| 264 | f32 | **translate Z** (lift above plate) | 0 when seated; 3.0 after supports auto-lift; morel=3.0. Caveat: in this corpus off264 is 3.0 **iff** the model has supports and 0 otherwise — "free Z-translation field" vs "support-coupled constant" is not yet separated (needs an unsupported-but-lifted probe; see §8) |
| 268 | u32 | **support-index pointer** (absolute) — `0` when the model has no supports; otherwise the file offset of that model's 16-byte support index | **Reconciliation discovery (2026-07-23):** verified in ALL six supported specimens: sphere-cylinder-supported m0=378412 m1=476840, morel=7790753, morel_mudskipper=10056161, treant_v4=88274744, supported-treant=68688440 — each equals the index address exactly, and each value occurs at exactly ONE u32-aligned-scan location in the whole file (record off268). All five unsupported specimens have 0 here. Supports are therefore reached by a direct pointer chain, not by scanning |
| 272 | u32 | **mesh blob offset** (absolute) | shifts with front-blob/layout growth (derived, not authored-stable) |
| 276 | u32 | **mesh blob size** (bytes) = 36 * n_triangles | sphere 171360=4760 tris; cylinder 8784=244 tris |

Fixed-field block is contiguous at 256..280 (3×f32 translate, u32 support-index
pointer, u32 mesh off, u32 mesh size). Everything else in the 680 bytes observed
zero across the corpus (unmapped, preserve verbatim; the unmapped span is
"after the name's NUL up to 256" plus "280..680"). The name field occupies well
under 256 bytes in all specimens (**unknown capacity** — see §8).

### 3.1 Transform storage — the central finding

The three probes isolate one transform each; each was diffed against its true base
(`sphere-cylinder`; determined by content — the probes are re-saves of the *unsupported*
two-model project with refreshed preview data, NOT the supported one).

- **Translation — stored as a header field.** `off256/260/264 = (X, Y, Z)` float32,
  millimetres, model-frame plate placement. sphere-moved changed **only** these bytes
  (+10.5 X, +5.25 Y) — exact match. The mesh centroid did not move. **Confidence high.**
- **Scale — NOT a field; baked into vertices.** sphere-scaled's model record was
  byte-identical to base except the mesh pointer and the name's `#N` digit. The mesh
  bbox went `[19.98,19.96,20.0] -> [29.97,29.94,30.0]` = exactly **1.5x**, same 4760
  triangles. Scale is applied to stored vertices at import; there is no scale field.
  **Confidence high.**
- **Rotation — NOT a field; baked into vertices.** cylinder-rotated's model record
  differed only in the mesh pointer, the `#N` digit, and sub-0.02 mm translation jitter
  from ChituBox re-arranging the plate. The cylinder mesh bbox went `[30,30,30] ->
  [42.42,29.99,42.42]`. **Convention pinned (reconciliation, 2026-07-23):** the
  244-tri cylinder is a faceted prism, NOT rotationally symmetric (rim XY radii
  15.0000..15.0050), so the world-Z step of the compound IS observable through
  facet orientation. Sorted-vertex-set comparison of the actual base facets under
  candidate rotations against the rotated file's vertices gives:
  `R = Ry(+45°) · Rz(+45°)` (world Z first, then world Y, both positive, applied
  to the centered vertices) → max abs diff **0.0049 mm** (float32 matrix noise);
  every other candidate tested (Y45 alone, Y-then-Z, either sign flipped) is off
  by 30–42 mm. Exact extents also match: observed (42.4212, 29.9926, 42.4212) vs
  Rz-then-Ry (42.4212, 29.9926, 42.4212), while pure Y45 gives (42.4264, 30.0000,
  42.4264) — a measurable mismatch. Rotation is applied to stored vertices, and the
  rotated blob's bbox center is exactly (0,0,0) (re-centered after rotation, or
  coincidentally centered — this cylinder cannot distinguish the two; see §8).
  **Confidence high** that rotation is baked; **high** on the compound order for
  this probe (was medium; the facet-set evidence pins it).

**Storage-convention consequence:** there is no matrix / quaternion / Euler field to
decode. A model's *orientation and scale live entirely in its baked vertices*; only
XY(Z) translation is a separate field. This is exactly the API the library wants: to
place a model, bake rotation+scale into the 36 B/tri vertex stream and write
translation into off256/260/264. (Matches the owner's packing scope: Z-rotation + XY
translation of the model+supports assembly; a full re-orient re-bakes vertices.)

---

## 4. Mesh encoding — confidence high

- **Embedded mesh = raw float32 triangle soup, 36 bytes/triangle** = 9 float32
  (v0.xyz, v1.xyz, v2.xyz). **No per-triangle normal, no count, no header.** Triangle
  count = `mesh_size / 36`. Region located by model record off272/off276.
- **Source STLs are BINARY, not ASCII** (the corpus hypothesis was inverted): headers
  are 80-byte comment text (`"STL Fi..."`, `"Uraniu..."`), then u32 count, then
  `count * 50` bytes (3f normal + 9f verts + u16 attr). `(238084-84)/50 = 4760`,
  `(12284-84)/50 = 244` — exact. Embedded is smaller than the STL because ChituBox
  **drops the 12-byte normal and 2-byte attribute** per triangle (50 -> 36).
- **Import transform = identity-centering on the bounding-box centre.** Embedded
  vertices == STL vertices minus `(min+max)/2`. Verified order-independently: the sorted
  vertex sets of `STL - bbox_center` and the embedded blob are **bit-identical**
  (max abs diff 0.0, float32) for the sphere AND for the cylinder (both models,
  reconciliation re-check 2026-07-23).
- **Triangles are reordered on import** (3535 / 4760 sphere triangles land at a
  different index) — geometry is bit-exact, ordering is not preserved. Harmless (mesh is
  unindexed soup) but means the library must not assume STL triangle order survives.

---

## 5. Support section — multi-model layout — confidence high

Corrects the earlier note (which described a single index at EOF for a single-model
file). In a **multi-model** file, each model's support data is stored **immediately
after that model's own mesh blob**, interleaved:

```
[model0 mesh][model0 support-mesh blobs][model0 index][model0 group hdr][model0 records]
[model1 mesh][model1 support-mesh blobs][model1 index][model1 group hdr][model1 records]
```

`sphere-cylinder-supported` (478212 B): 26 segment records total = **13 + 13**, one
group per model, 72-byte stride, split by a large gap (the model-1 mesh sits between
the two record groups). **Supports bind to a model by POSITION** (contiguous block per
model), not by an index field inside the record.

Per-model **16-byte index** `{u32 n_records, u32 records_off, u32 mesh_off, u32 mesh_size}`
sits right after that model's support-mesh region (`mesh_off + mesh_size == index_off`),
and is reached **directly via the u32 pointer at model-record offset 268** (§3) — no
scanning required:

| model | index@ | n | records_off | support_mesh_off | support_mesh_size |
|-------|-------:|--:|------------:|-----------------:|------------------:|
| 0 sphere   | 378412 | 13 | 378828 | 290140 | 88272 |
| 1 cylinder | 476840 | 13 | 477256 | 388568 | 88272 |

Between index-end and the records lies a **400-byte support group header** which is
**all zero bytes in every supported specimen examined** (reconciliation, 2026-07-23:
both models of sphere-cylinder-supported, morel.chitubox, morel_mudskipper_supports,
treant_v4_supports n=1783, supported-treant n=1057 — nonzero count 0/400 in all six).
The earlier note's description of this region as a "preset-name table + per-support
260/7680 tables" is **not reproduced by this corpus** and should be treated as stale
(possibly a different ChituBox version or a misread); see §8 item 4.

**Segment records — 72 bytes, magic `0xEA342389` (3929285513)** — unchanged from the
earlier note and re-verified here. rec0 of each group is `type=7` (tip cone) with
`mesh_off == model_mesh_end` (support blobs begin right after the model mesh). Observed
type sequence per group: `[7,1,9,4,3,6, 7,1,9,4,3,6, 6]` = two minimal supports
(tip/taper/shaft/flare/joiner/pad) + a type-6 bridge. Full field table: see §"Segment
records" in the cited earlier note (offsets 0 magic, 4 type, 8/20 start/end xyz, 32/36
radii, 40/44 mesh off/len, 48+ trailing).

The later UI-authored `two-supports.chitubox` donor has record types
`[7,1,9,6,7,3,4,3,4,9,1,6,6]`. This proves that physical `[7,1]` adjacency and
the order of a type-7 record's two metadata poles are **not universal native-file
invariants**. It does pin these segment semantics:

- type 7: sphere-only contact mesh, equal radii, zero trailing;
- type 1: taper-only contact mesh, with contact depth in trailing float slot 0
  (`0.1` and `0.5` in the donor);
- type 9: standalone joint sphere, equal radii and vertical metadata poles;
- type 3: ordinary rod/column;
- type 4: downward root boss from column end to raft top, radii column -> boss;
- type 6: unchanged pad/bridge raft geometry.

The emitter uses a deterministic adjacent contact-pair ordering without changing
planner coordinates. For a stub from deepest contact point `p0` toward `p1`, let
`u = unit(p1-p0)` and `R = tip_ball_r_factor*r0`. Its emitter-specific contract is:

1. type 7: start `p0`, end `p0+2R*u`, radii `(R,R)`, zero trailing bytes, and a
   sphere mesh of radius `R` centered at `p0+R*u`;
2. immediately adjacent type 1: start `p0+R*u`, end `p1`, radii `(r0,r1)`,
   contact-depth in trailing float slot 0, and a cached taper mesh with no
   duplicate contact sphere.

Joint balls are emitted separately as type 9 with start/end
`center+[0,0,R]` / `center-[0,0,R]`. A plate root's type-3 column ends at a
type-4 boss top; the boss ends at `plate_z+0.2`, the top of the unchanged type-6
raft, rather than extending to `plate_z`. These are serialization contracts only;
the 400-byte group header and registry fields remain preserved from the evidenced
corpus rather than inferred from these records.

Adding supports also **auto-lifts every model by 3.0 mm** (off264 Z: 0 -> 3.0 on both
models in the supported file; morel likewise 3.0), presumably to seat feet/pads below
the model. The library must set/track this lift when it emits supports.

---

## 6. Front region (settings + preview) — confidence medium

- `[0..420]` header + settings: 396 bytes after the header, all zero in an empty
  project. This is the constant "empty baseline" (`settings_size=420`, `const16=412`).
- `[420..off8]` preview/thumbnail: dense, non-zero (zero-fraction ~1.5%), **no PNG /
  JPEG / RIFF signature** — a raw, uncompressed preview buffer whose internal format is
  unmapped. Its size varies per save (why the probe files differ in size and why all
  downstream offsets shift). Preserve verbatim; regenerate only if we ever need
  ChituBox to show a correct thumbnail (not required for load/slice).

---

## 7. Implications for `rust/chitubox_io`

- **Add-model path:** append a 680-byte record (name at 0, translate at 256/260/264,
  mesh ptr/size at 272/276); bake rotation+scale into a 36 B/tri centered vertex stream;
  append the mesh blob; bump header model_count and recompute off8/off20 and every
  absolute mesh/index/records offset. Owner's batch-first doctrine: build the whole
  model table + concatenated mesh region in one pass over a slice of models.
- **Placement API:** write `(x,y)` (and lift z) to off256/260/264 — pure field write,
  no vertex rebake. Z-rotation for packing = rebake the model's (and its supports')
  vertices; translation stays a field. This matches the owner's rigid-assembly scope.
- **Round-trip invariant (gold a):** trivially satisfied by keeping unknown regions
  (preview, group headers, footer, zero filler) verbatim — never regenerate them on a
  passthrough save.
- **Default template:** embed `empty.chitubox` (420 B) as a resource; every "from
  scratch" build starts from it and appends.

---

## 8. Remaining unknowns

1. **Model-record bytes 14..256 and 280..680** — assumed zero/opaque from this corpus;
   real projects (colours, per-model print settings, groups) may populate them. Capacity
   of the name field vs. where fixed fields begin is unproven beyond "name short, 256 is
   translate". Need a specimen with long names / per-model settings.
2. **Preview/thumbnail buffer format** (§6) — raw, unmapped. Not needed for load/slice.
3. **Settings blob** (396 bytes) — all zero here; a project with non-default print
   settings (layer height, exposure, resin profile) is needed to map it.
4. **Support group header, 400 B** — all-zero in all six supported specimens (see §5);
   whether ChituBox ever writes nonzero data here (other versions, edited presets) is
   unknown. The earlier note's "preset-name + per-support 260/7680 tables" claim is not
   reproduced by this corpus and is treated as stale. Writing 400 zero bytes matches
   every native specimen we have.
5. **Baked-rotation convention** — pinned for this probe: `Ry(45°)·Rz(45°)` on centered
   vertices, vertex-set match to 0.0049 mm (§3.1; the faceted cylinder is NOT
   rotationally symmetric, so the Z step is observable). Remaining open: whether
   ChituBox re-centers the bbox after rotation (rotated blob center is exactly 0 here,
   but this cylinder cannot distinguish re-centering from coincidence) — an
   XY-asymmetric probe (L-bracket/wedge) rotated purely in Z would settle it and give
   a clean pure-Z ground truth.
5b. **off264 Z-lift vs support presence confound** — in this corpus off264 is 3.0 iff
   the model has supports and 0.0 otherwise (verified on all 7 project files). Whether
   off264 is a free Z-translation field or a support-coupled value needs one probe: an
   unsupported model manually lifted in Z.
6. **20-byte footer** cited in the earlier note was NOT observed in these small files
   (mesh/records run to EOF); re-check on files that have one.
7. **Front descriptor prefix** — bytes after `off20` begin with one contiguous
   24-byte descriptor per model: `(u64 count, u64 table_abs_ptr, u64 aux_abs_ptr)`.
   A zero descriptor is valid for unsupported models. Any remaining bytes before
   the first mesh stay opaque and variable-size (48 B in regenerated two-model
   specimens; larger in native supported specimens). Fresh scaffolds emit only
   the exact zero `24 * model_count` prefix; parsed front regions are preserved
   verbatim and not resized.
8. **Format is version-pinned** (CHITUBOX Basic 1.3.0). Re-verify on upgrade.
9. Earlier morel recon cited a mesh pointer at abs 408706 -> (409138, 7288704); this
   session's `morel.chitubox` has the model table at 412940 and mesh at 413777 (+7288704,
   202464 tris). Same tri count, different absolute offsets => the two were different
   morel snapshots. Absolute offsets are never authored-stable; always follow the header
   -> table -> record pointer chain.

---

## 9. Current implementation status

`rust/chitubox_io` (crate contract: [`chitubox-io.md`](chitubox-io.md))
implements this format as a container reader/writer: parse via pointer chain
only (no magic-scanning), serialize with recomputed offsets, re-parse +
independent interval audit before any file is written, and the full
`Project` container API (open/create, add models, set placement, relative
Z-rotation of model+supports as one rigid assembly, attach/replace/append
support records, save with re-parse-then-write). It stores/reads any `u32`
support-record type opaquely; it does not synthesize support-segment
geometry itself.

**No Rust code synthesizes support-segment geometry** (frustum/sphere/pad/
bridge meshes, MST raft-bridge topology, or the tip/taper/shaft/flare/root
sizing decisions that turn a support forest into `SegmentSpec`s). That
geometry synthesis lives in `crates/plinth-emit`, driven by the `plinth`
binary in `crates/plinth-cli`, consuming `chitubox_io` + `geom_kernel`
directly. Its winding-outward check is a native f64 signed-tetrahedron-volume
sum (`rust/geom_kernel/src/winding.rs`), validated against the native
per-segment PAD/BRIDGE ground-truth fixtures listed in §10 below — never
against the regenerated `sphere-*`/`pad_bridge_donor.chitubox` set, which is
circular for writer bugs.

## 10. Test corpus — provenance table

Corpus location: `scripts/tests/fixtures/chitubox/` (committed, small
fixtures; see that directory's README for the authoritative manifest). A
missing committed fixture fails by its named path; it never skips. The sole
exception is the uncommittable large `treant_v5_candidate.chitubox`, looked
up under `CHITUBOX_CORPUS_DIR` — only tests for that exact named specimen may
print `SKIP:` and pass when it is absent.

**Critical distinction — read before using any fixture as ground truth**:
some fixtures are files ChituBox itself authored (real, independent oracle
data); others were regenerated by `chitubox_io`'s own writer after the
originals were lost on 2026-07-24, which makes them **circular for
validating writer bugs** — a bug in the writer that produced them would
silently reproduce in the "expected" file too.

| File | Contents | Provenance |
|---|---|---|
| `empty.chitubox` | 420-byte empty project, no models | **Native** (original) |
| `morel.chitubox` | Morel Mudskipper, unsupported | **Native** (original) |
| `morel_mudskipper_supports.chitubox` | Morel Mudskipper, 243 support records (merged supports — the pinned OPEN-soup case, not per-segment primitives) | **Native** (original) |
| `Sphere.stl`, `Cylinder.stl` | Source meshes for the sphere/cylinder scenes | **Native** (original) |
| `sphere-cylinder.chitubox` | Sphere + Cylinder, cylinder at (43.7035, 7.1448) | **Regenerated** (native original lost 2026-07-24) |
| `sphere-moved.chitubox` | Same scene, sphere translated to (10.5, 5.25) | **Regenerated**, same commit |
| `sphere-scaled.chitubox` | Same scene, sphere mesh baked 1.5× | **Regenerated**, same commit |
| `cylinder-rotated.chitubox` | Same scene, cylinder mesh baked `Ry(45°)·Rz(45°)` | **Regenerated**, same commit |
| `sphere-cylinder-supported.chitubox` | Same scene, 7 supports on model 0, 13 on model 1 | **Regenerated**, same commit |
| `pad_bridge_donor.chitubox` | Minimal single-model project, one PAD + one BRIDGE record | **Regenerated** (re-hosts two records lifted from `morel_mudskipper_supports.chitubox`) |
| `two-supports.chitubox` | UI-authored two-support project pinning type 7/1 contacts, type 9 joint spheres, type 3 columns, type 4 bosses, type 6 raft | **Native** (UI-authored) |
| `basic-cube-supports-raft.chitubox` | Native 20×20×20 cube (12 tris), real PAD + BRIDGE type-6 records, 71 support records, bottom-connected raft bridging | **Native** (UI-authored) |
| `cylinder-supported-bottom-connected.chitubox` | Native Cylinder (244 tris), real PAD + BRIDGE type-6 records, 166 support records, bottom-connected | **Native** (UI-authored) |

Not committed (~35 MB, `CHITUBOX_CORPUS_DIR`-only): `treant_v5_candidate.chitubox`,
pinned at 1360 support records by `support_parity.rs`.

**The two most important fixtures for any geometry-synthesis code**:
`basic-cube-supports-raft.chitubox` and
`cylinder-supported-bottom-connected.chitubox` are the **only**
native-ground-truth specimens carrying real PAD/BRIDGE (type 6) geometry —
the source from which the fixed 2.2 mm diameter / 0.2 mm height / 80-triangle
pad and 12-triangle bridge contract was reverse-engineered. Any PAD/BRIDGE
synthesis must validate against these two files, never against the
regenerated `sphere-*`/`pad_bridge_donor.chitubox` set.

This table is not the M0 model-baseline manifest — it proves container and
emitter format behavior; it neither adds models to nor removes models from
the model-baseline set (see `docs/development.md` for corpus provisioning).

## 11. Audit-the-file-as-written rule

A hard-won lesson: this project once validated an in-memory mesh while
shipping a different file to disk. `chitubox_io` enforces the fix and any new
emit code (Rust or otherwise) must preserve it:

- **Never trust the writer's own state.** `verify()` re-parses its own
  freshly-serialized output bytes — it does not reuse the `Document` that
  produced them.
- **The audit is a second, independent code path.** `audit()` performs its
  own from-scratch pointer-chain walk directly against the header fields — it
  does not call into the parser. Two independent readers agreeing is the
  actual guarantee; one parser agreeing with itself is not.
- **Failure means no file is written.** `VerifyFailed` aborts `save()`/
  `to_bytes()` before any bytes reach disk; `save()` additionally writes to a
  same-directory temp file and renames only after verify passes.
- **Honesty clause — say this exact thing, don't overclaim:** a passing
  `verify()` means the output is **structurally self-consistent**
  (well-formed per this format), never "ChituBox-valid". ChituBox itself is
  the only semantic oracle (the manual acceptance gate in
  [`chitubox-io.md`](chitubox-io.md) §7). Use this language in logs, docs, and
  commit messages — do not say "verified" when you mean "self-consistent".
- **Boundary-edges / negative-shells / winding checks belong to the
  geometry-synthesis layer, not the container.** `chitubox_io`'s audit checks
  container structure (offsets, coverage, magics), not mesh manifoldness. The
  emit path re-reads the emitted mesh from disk and fails on boundary edges,
  negative shells, or inconsistent winding — that is a `geom_kernel`/emit-path
  check, not something the container's `verify()` covers.
