# 05 — .chitubox format status and the test corpus contract

This doc is the current-state map for the emit stage: what `rust/chitubox_io`
already does, what only Python can do today, and the test-corpus rules any new
Rust code (container or geometry-synthesis) must obey. It summarizes and links
to the byte-level specs rather than duplicating them — **read these three
files yourself before writing any chitubox_io or emit code**:

- `../specs/CHITUBOX-FORMAT.md` — byte-level container/model/mesh/support map,
  confidence-graded per finding.
- `../specs/SPEC-CHITUBOX-RS.md` — the `chitubox_io` crate's architecture spec
  (public API, internal `Document` model, gold invariants, test plan, risk
  register). Sections cited below (`§N`) refer to this file.
- `../../scripts/tests/fixtures/chitubox/README.md` — the corpus manifest and
  provenance notes (also reproduced in the table below; that file is the
  source of truth if they ever disagree).

This doc supersedes nothing in those three files; it only adds a "what's
ported vs. what's still Python" cut and a condensed corpus/gate reference for
Codex to work from without re-deriving it.

Do not conflate the two corpora in this guide. The files in
`scripts/tests/fixtures/chitubox/` are **format and writer fixtures**. The frozen
Python comparison baseline is built from a separate, exact model list under
`corpus/models/` and produces committed artifacts under `corpus/baseline/`.

---

## 1. What `rust/chitubox_io` already does

Crate root: `rust/chitubox_io/`. Modules: `lib.rs`, `error.rs`, `consts.rs`,
`types.rs`, `document.rs`, `parse.rs`, `write.rs`, `verify.rs`, `stl.rs`,
`transform.rs`, `support.rs`, `project.rs`, plus `py.rs` gated behind the
`python` feature (PyO3 boundary, scheduled for deletion — see
`04-migration-plan.md`). `#![forbid(unsafe_code)]` is set in `lib.rs`.

Implemented and tested today:

| Capability | Where | Notes |
|---|---|---|
| Parse a `.chitubox` file via pointer chain only (no magic-scanning) | `parse.rs` | header → model table → per-record mesh ptr (`M_MESH_OFF`/`M_MESH_SIZE`, offsets 272/276) and support-index ptr (`M_SUPPORT_INDEX_PTR`, offset 268) → index quad → records |
| Serialize `Document` back to bytes, recomputing offsets | `write.rs` | canonical order: header, settings, preview, model table, gap, then per model: mesh, support mesh region, index, group header, records |
| Re-parse + independent interval audit before any file is written | `verify.rs` | `verify()` calls `parse()` on its own output, then `audit()` — a **separate, from-scratch** pointer-chain walk that does not reuse parser code, asserting non-overlap, forward ranges, per-model section ordering, mesh-length%36==0, and total byte coverage |
| Container-level API: open/create, add models (from binary STL or raw triangle buffers), set XY/Z-lift placement, relative Z-rotation of model+supports as one rigid assembly, attach/replace/append support records, save with re-parse-then-write | `project.rs` (`Project`) | Full public surface in `SPEC-CHITUBOX-RS.md` §3 |
| Support-record container I/O: encode/decode the 72-byte record, assemble/parse a support section (index + group header + records) | `support.rs` (`build_section`, `encode_record`, `decode_record`) | Stores/reads whatever `SupportRecord` (type, start, end, radii, `TriangleSoup` mesh, 24 trailing bytes) it is handed — **it does not synthesize support geometry** (see §2) |
| Binary STL read + identity-centering (bbox-center subtraction) | `stl.rs` | Rejects ASCII STL explicitly (`AsciiStlUnsupported`) |
| Z-rotation rebake from a pristine snapshot (accumulates without f32 drift across repeated small rotations) | `transform.rs` | `PristineGeometry` cached in `Document`, `SPEC-CHITUBOX-RS.md` §6.4 |
| All format constants (magics, field offsets, record/section lengths) | `consts.rs` | `FILE_MAGIC = 0xAB23_1243`, `SEG_MAGIC = 3_929_285_513`, `MODEL_RECORD_LEN = 680`, `SEG_RECORD_LEN = 72`, `SUPPORT_INDEX_LEN = 16`, `GROUP_HEADER_LEN = 400`, `MESH_BYTES_PER_TRI = 36` |
| PyO3 boundary: `write_project(out_path, models: list[dict])`, `read_project(path)` | `py.rs` (feature `python`) | The **only** two PyO3-exposed functions; models dicts carry `stl_path`/`triangles`, placement, and a `supports` dict of parallel numpy arrays. This is the boundary being deleted once nothing in `scripts/` calls `chitubox_io._native` — see `04-migration-plan.md` and `06-conventions.md` |

Record types the container already round-trips (it stores/reads any `u32`
type id opaquely — no special-casing per type in the container layer):
`SegmentType` constants defined in `types.rs` are `TIP=7`, `TAPER=1`,
`SHAFT=9`, `JOINER=3`, `FLARE=4`, `PAD_OR_BRIDGE=6` (pad: `start==end`;
bridge: `start!=end`). Types `2` (short taper), `8` (coupler), and `12`
(unmapped) are observed in the wild and pass through as opaque
`SegmentType(n)` values without special handling.

Rust test files exist and are green against the committed corpus:
`rust/chitubox_io/tests/{roundtrip,synth_compare,transforms,support_parity,errors,regression_blockers}.rs`.

---

## 2. What only Python can do today — the geometry-synthesis gap

**No Rust code synthesizes support-segment geometry.** `chitubox_io` is a
container reader/writer only: give it a fully-formed `SupportRecord` (start,
end, radii, a pre-built triangle-soup mesh, trailing bytes) and it will
encode/store/round-trip it. It never builds a frustum, a sphere, a pad, or a
bridge mesh itself. All of that lives in
`scripts/support_placement/chitubox_emit.py` (pure Python + numpy), which
remains the only implementation of these algorithms:

| Python function | `chitubox_emit.py` line | What it does | Rust status |
|---|---|---|---|
| `frustum_mesh(start, end, r0, r1, n_sides=24, ring_normal=None)` | 53 | Capped-frustum triangle-soup for support shafts/tapers, winding self-corrected via a signed-volume check | **not ported** |
| `sphere_mesh(center, r, n_lat=8, n_lon=16)` | 136 | UV-sphere mesh for tip/joint/model-foot balls | **not ported** |
| `_pad_mesh(xy, plate_z)` | 377 | Fixed 2.2 mm-diameter, 0.2 mm-height, 20-sided pad geometry (type 6, `start==end`) | **not ported** |
| `_bridge_mesh(xy0, xy1, plate_z)` | 387 | 12-triangle rectangular-prism raft bridge (type 6, `start!=end`) | **not ported** |
| `_euclidean_mst_edges(points)` | 442 | Deterministic Prim MST over plate-root XY points, drives raft bridge topology | **not ported** |
| `member_chain_specs(members, balls, cfg, plate_z=None)` | 512 | The v4 geometry PRIMITIVE: turns a support forest (members + joint balls) into `SegmentSpec`s — all support-record geometry decisions (tip/taper/shaft/flare sizing, root-boss clearance rules) live here | **not ported** |
| `bottom_connected_specs(members, plate_z)` | 467 | Fixed native-style pad + MST raft-bridge specs | **not ported** |
| `build_record(type_id, start, end, r1, r2, mesh_off, mesh_len, cfg)` | 256 | Legacy byte-patch 72-byte record packer | Superseded in Rust by `support.rs::encode_record` for the container path; the geometry-decision logic that feeds it (above) is what's unported |

Also in this cluster but explicitly **not to be ported**:
`scripts/support_placement/chitubox_inject.py` (`Project`, `clone_support`,
`make_bridge`, `write`/`write_replace`) is the older byte-patching path,
superseded by the from-empty `chitubox_io` container path. Its own module
docstring warns `write_replace()` "staples correct supports onto the WRONG
MODEL, silently" if misused — do not port `write()`/`write_replace()`. Only
`chitubox_emit.py`'s geometry algorithms above are load-bearing to port.

**Constants to carry over with typed provenance** (per architectural
commitment #2 in the parent guide — see `03-physics-and-constants.md` for the
full table and required `Provenance` enum): `N_SIDES=24`, `PAD_N_SIDES=20`,
`PAD_DIAMETER_MM=2.2`, `RAFT_HEIGHT_MM=0.2`, `PLATE_BAND_MM=0.2`,
`MIN_ROOT_VERTICAL_LAYERS=10`, and `MIN_ROOT_NO_BEND_MM=0.5` are all
Placeholder. This resolves this document's earlier Measured/Derived grading
against the authoritative grades in `03-physics-and-constants.md`;
`PLATE_BAND_MM` remains one Rust-owned constant despite its duplicated Python
sources.

**Winding-check dependency to replace, not port:** `frustum_mesh`,
`sphere_mesh`, and `_bridge_mesh` each `import trimesh` locally only to call
its Mirtich `mass_properties` for a signed-volume winding check. Per
architectural commitment #6 (float64 geometry throughout), the Rust port must
replace this with a native f64 signed-tetrahedron-volume sum
(`Σ v0·(v1×v2)/6`) in `rust/geom_kernel`, not a ported trimesh call — and must
validate the chosen algorithm against the **native** ground-truth fixtures in §3,
and for PAD/BRIDGE specifically against the only two natives that carry type-6
geometry (`basic-cube-supports-raft.chitubox`,
`cylinder-supported-bottom-connected.chitubox`) — **never** against the regenerated
`sphere-*`/`cylinder-*` set or `pad_bridge_donor.chitubox`, which are circular for
writer bugs. (`chitubox_emit.py`'s own docstring notes it sometimes disagreed with
trimesh on unwelded triangle soups, so pick one algorithm and verify it
against the corpus rather than trusting either blindly.)

**Batch-first shape to preserve:** `member_chain_specs`/`bottom_connected_specs`
already batch at the whole-forest level (one call builds every `SegmentSpec`
for a model), and `scripts/support_placement/emit.py::support_arrays()`
packs them into one columnar crossing. The Rust replacement must keep that
shape — build the whole `Vec<SupportRecord>` for a model in one pass, not
per-member calls — and compute frustum/sphere geometry in f64, casting to f32
only at the final record-encode step (mirroring what `types.rs` already does:
`SupportRecord`/`TriangleSoup` are f32 because that IS the wire format,
upstream geometry computation should stay f64).

The home for this new Rust geometry synthesis is **`crates/plinth-emit`**, driven
by the `plinth` binary in **`crates/plinth-cli`**, consuming `chitubox_io` +
`geom_kernel` directly with no PyO3 boundary. The root `[[bin]]` and its
`src/main.rs` stub are **deleted at M0** — do not build emit geometry there. See
`02-target-architecture.md` §2 and §9 and `04-migration-plan.md` M0/M4 for the
workspace surgery and milestone sequencing.

---

## 3. Test corpus — the provenance table

Corpus location: `scripts/tests/fixtures/chitubox/` (committed, small
fixtures). Missing committed fixtures fail by their named path; they never
skip. The sole exception is the uncommittable large
`treant_v5_candidate.chitubox`, looked up under `CHITUBOX_CORPUS_DIR` (default
`/Users/craig/code/hughescr/dnd-campaigns`, i.e. this repo's parent — see §4).
Only tests for that exact named specimen may print `SKIP:` and pass when it is
absent. This corrects the earlier broad helper wording to the normative
fail-not-skip rule in `04-migration-plan.md` M0 work item 9.

**Critical distinction — read before using ANY fixture as ground truth**:
some fixtures are files ChituBox itself authored (real, independent oracle
data); others were regenerated BY `chitubox_io`'s own writer after the
originals were lost on 2026-07-24, which makes them **circular for validating
writer bugs** — a bug in the writer that produced them would silently
reproduce in the "expected" file too. Do not promote a regenerated fixture to
a role only a native fixture can fill.

| File | Contents | Provenance | Consumed by |
|---|---|---|---|
| `empty.chitubox` | 420-byte empty project, no models | **Native** (original) | `roundtrip.rs` |
| `morel.chitubox` | Morel Mudskipper, unsupported | **Native** (original) | `roundtrip.rs` |
| `morel_mudskipper_supports.chitubox` | Morel Mudskipper, 243 support records | **Native** (original) | `roundtrip.rs`, `support_parity.rs` |
| `Sphere.stl`, `Cylinder.stl` | Source meshes for the sphere/cylinder scenes | **Native** (original) | `synth_compare.rs` |
| `sphere-cylinder.chitubox` | Sphere + Cylinder, cylinder at (43.7035, 7.1448) | **Regenerated** by `chitubox_io` at commit `aa08704bfc7918be4b83a74e9d61a774ab1ae28d` (native original lost 2026-07-24) | `synth_compare.rs`, `transforms.rs`, `support_parity.rs`, `errors.rs`, `roundtrip.rs` |
| `sphere-moved.chitubox` | Same scene, sphere translated to (10.5, 5.25) | **Regenerated**, same commit | `transforms.rs`, `roundtrip.rs` |
| `sphere-scaled.chitubox` | Same scene, sphere mesh baked 1.5× | **Regenerated**, same commit | `transforms.rs`, `roundtrip.rs` |
| `cylinder-rotated.chitubox` | Same scene, cylinder mesh baked `Ry(45°)·Rz(45°)` | **Regenerated**, same commit | `transforms.rs`, `roundtrip.rs` |
| `sphere-cylinder-supported.chitubox` | Same scene, 7 supports on model 0, 13 on model 1 | **Regenerated**, same commit | `roundtrip.rs`, `support_parity.rs`, `transforms.rs`, `errors.rs` |
| `pad_bridge_donor.chitubox` | Minimal single-model project, one PAD + one BRIDGE record | **Regenerated** (built by re-hosting two records lifted from `morel_mudskipper_supports.chitubox`, same commit) | `test_chitubox_emit.py` |
| `two-supports.chitubox` | UI-authored two-support project pinning type 7/1 contacts, type 9 joint spheres, type 3 columns, type 4 bosses, type 6 raft | **Native** (UI-authored, supplied 2026-07-26; NOT regenerated) | `test_chitubox_emit.py` |
| `basic-cube-supports-raft.chitubox` | Native 20×20×20 cube (12 tris), real PAD + BRIDGE type-6 records, 71 support records, bottom-connected raft bridging | **Native** (UI-authored; committed 2026-07-30, held rzip-compressed outside the repo before that) | reference only — not yet consumed by a Rust test |
| `cylinder-supported-bottom-connected.chitubox` | Native Cylinder (244 tris), real PAD + BRIDGE type-6 records, 166 support records, bottom-connected | **Native** (UI-authored; committed 2026-07-30) | reference only — not yet consumed by a Rust test |

Not committed (~35 MB, `CHITUBOX_CORPUS_DIR`-only): `treant_v5_candidate.chitubox`,
pinned at 1360 support records by `support_parity.rs` (replaces the older
`treant_v4_supports.chitubox`/`supported-treant.chitubox` referenced in
`CHITUBOX-FORMAT.md`/`SPEC-CHITUBOX-RS.md` — those were superseded and the
current pinned name is `treant_v5_candidate.chitubox`). Tests referencing it
alone skip cleanly without the env var set; every committed fixture remains a
named required input.

**The two most important fixtures for any new geometry-synthesis code**:
`basic-cube-supports-raft.chitubox` and `cylinder-supported-bottom-connected.chitubox`
are the **only** native-ground-truth specimens carrying real PAD/BRIDGE (type
6) geometry — they are literally the source from which the fixed 2.2 mm
diameter / 0.2 mm height / 80-triangle-2880-byte pad and 12-triangle-432-byte
bridge contract in `chitubox_emit.py` was reverse-engineered. Any Rust
PAD/BRIDGE synthesis must validate against these two files, not against the
regenerated `sphere-*`/`pad_bridge_donor.chitubox` set (which is circular for
this exact purpose — see the crossed-out warning in the corpus README).

Full detail and regeneration command
(`cargo run --example regen_corpus -- <stl_dir> <donor.chitubox> <out_dir>`)
is in `../../scripts/tests/fixtures/chitubox/README.md` — read it before
adding or replacing any fixture.

This table is not the M0 model-baseline manifest. It proves container and emitter
format behavior; it neither adds models to nor removes models from the exact
baseline set below.

---

## 4. `CHITUBOX_CORPUS_DIR` fallback

`rust/chitubox_io/tests/common/mod.rs::optional_treant_candidate_path()` is
the sole helper that reads `CHITUBOX_CORPUS_DIR` and falls back to
`/Users/craig/code/hughescr/dnd-campaigns` (this repo's parent directory) if
unset. It resolves only the uncommitted large treant specimen
(`treant_v5_candidate.chitubox`, ~35 MB, pinned at 1360 support records in
`support_parity.rs`) in Craig's environment without committing it; committed
fixtures use `required_fixture_path()` and never consult the fallback.

The **Treant source STL** is a model-baseline input, not a ChituBox format
fixture. **"treant" and "Awakened Tree" are the same model** — the source file is
`Awakened Tree.stl` (21 MB), staged in transient `minis-to-process/`. Copy it
first to the durable canonical path
`${CHITUBOX_CORPUS_DIR:-/Users/craig/code/hughescr/dnd-campaigns}/Weeping Treant v1.2.stl`
and point `$TREANT_STL` at that durable copy. Then copy it locally as the ignored
`corpus/models/Weeping Treant v1.2.stl` member of the exact input set. Never
commit either STL or any `corpus/models/*.stl` input.

Verify the resolver result with the explicit `test -f` command in
`04-migration-plan.md` M0. `resolve_stl()` can print its fall-through path even
when no file exists, so a printed path alone is not verification.

For Codex running in a checkout without that file present: `support_parity.rs`
tests referencing `treant_v5_candidate.chitubox` will print a `SKIP:` notice
and pass trivially — this is expected, not a failure to chase. Do not
fabricate or synthesize a substitute treant fixture to make the test "really"
run; the corpus README (§3 above) is explicit that self-generated fixtures
cannot validate writer bugs, and a fabricated treant specimen would be worse
than a skip (it would silently launder circularity into a large/complex test
that looks authoritative). If real coverage on treant-scale data is needed,
get a fresh export named `treant_v5_candidate.chitubox` from Craig or from a
prior `CHITUBOX_CORPUS_DIR` location and place it there — never invent one.

### 4.1 Exact M0 model-baseline membership

The full baseline is exactly these eight models, in the binding order:

1. `Sphere`
2. `Cylinder`
3. `make_box`
4. `make_overhang_table`
5. `make_toed_slab`
6. `make_raking_blade`
7. `make_thin_fin`
8. `Weeping Treant v1.2` (source: `Awakened Tree.stl`; one model, not two)

The sanctioned provisional corpus is the same set minus `Weeping Treant v1.2`,
exactly seven models. **There is no `Cone` in either set.** All eight source STLs
are ignored inputs in `corpus/models/`; the manifest and baseline `.chitubox` and
report artifacts are committed once the later M0 freeze work item produces them.
Their current absence is pending M0 work, not permission to fabricate or regenerate
them during a documentation correction. Fixture-corpus membership above never
changes this list.

---

## 5. Golden-test strategy for emit parity

The container layer (`chitubox_io`) already has its gold tests (§1, roundtrip
+ synth_compare + transforms + support_parity + errors). Once support
geometry synthesis is ported to Rust (§2), it needs its own parity strategy,
following the pattern the crate already established:

1. **Byte-format contract fixtures are the oracle, not Python.** Per the
   parent guide's acceptance-criterion carve-out (`04-migration-plan.md`),
   byte-identity applies where a real format contract exists — the
   `.chitubox` record structure must round-trip and load in ChituBox. For
   PAD/BRIDGE geometry specifically, that means matching
   `basic-cube-supports-raft.chitubox` and
   `cylinder-supported-bottom-connected.chitubox` (§3), not matching Python's
   output. Python is a **test-data source only** (per the same ruling
   `support_parity.rs` already applies to the 1783/243-record emitter
   outputs), never a comparison target for the acceptance gate itself.
2. **Cross-check against Python where it helps triage, never as the gate.**
   `support_parity.rs`'s existing pattern — parse a real emitter output,
   census counts, type histogram, per-record field decode cross-checked via a
   one-time JSON dump generated by the Python reference — is reusable for
   validating that new Rust `frustum_mesh`/`sphere_mesh`/pad/bridge synthesis
   produces the same *topology* Python did, as a development aid. This is
   separate from, and does not replace, the objective corpus-file gate in
   item 1.
3. **New fixtures needed before this can close:** none of the committed
   fixtures currently exercise a full member-chain (tip→taper→shaft→flare→pad)
   built from scratch by Rust. Add a synthetic round-trip test analogous to
   `SPEC-CHITUBOX-RS.md` §7.4's "asymmetric multi-model case" (3 records on
   one model, 17 on another) once Rust geometry synthesis exists, and gate the
   new PAD/BRIDGE code specifically against the two native fixtures named
   above before calling it production-safe.
4. **Manual ChituBox acceptance stays mandatory and per-milestone.** Per
   `SPEC-CHITUBOX-RS.md` §7.6/§11: a from-scratch multi-model file and a
   supports-attached file must each load in ChituBox Basic, be
   selectable/movable, have editable supports, and slice — recorded as a
   checklist, not CI. This is unchanged by the Rust geometry-synthesis work
   and must be re-run once real support geometry (not just container
   round-trips) originates from Rust.

---

## 6. Audit-the-file-as-written rule

This is a hard-won lesson (parent guide, architectural commitment #7): this
project once validated an in-memory mesh while shipping a different file to
disk. `chitubox_io` already enforces the fix and any new emit code (Rust or
otherwise) must preserve it:

- **Never trust the writer's own state.** `verify()` in `verify.rs` re-parses
  its own freshly-serialized output bytes (`parse(bytes.to_vec())`) — it does
  not reuse the `Document` that produced them.
- **The audit is a second, independent code path.** `audit()` in `verify.rs`
  performs its own from-scratch pointer-chain walk (reads `H_MODEL_COUNT`,
  `H_OFF8`, `H_OFF20` directly via a local `rd()` helper) — it does not call
  into `parse.rs`'s parser. Two independent readers agreeing is the actual
  guarantee; one parser agreeing with itself is not.
- **What the audit checks** (per `SPEC-CHITUBOX-RS.md` §6.2): magic; model
  count vs. table; header invariant `off20 == off8 + 680*n`; every mesh
  pointer/size in bounds and `size % 36 == 0`; every support section's record
  magics, 72-byte stride, and index-quad consistency (`mesh_off + mesh_size
  == index_off`, `n == records.len()`, record `off268 == index offset`);
  **total byte coverage** (every byte of the output lands in exactly one
  classified region — no unaccounted gaps, no overlaps); no negative-length
  or backward intervals; correct section ordering per model (mesh < support
  mesh < index < group header < records).
- **Failure means no file is written.** `VerifyFailed` aborts `save()`/
  `to_bytes()` before any bytes reach disk; `save()` additionally writes to a
  same-directory temp file and renames only after verify passes.
- **Honesty clause — say this exact thing, don't overclaim:** a passing
  `verify()` means the output is **structurally self-consistent**
  (well-formed per the reverse-engineered format), never "ChituBox-valid".
  ChituBox itself is the only semantic oracle (§5 item 4's manual acceptance
  gate). Use this language in logs, docs, and commit messages — do not say
  "verified" when you mean "self-consistent".
- **Boundary-edges / negative-shells / winding checks belong to the future
  geometry-synthesis layer, not the container.** `chitubox_io`'s audit checks
  container structure (offsets, coverage, magics), not mesh manifoldness. Per
  architectural commitment #7 (boolean-union at emit), once Rust geometry
  synthesis exists, the emit path must also re-read the emitted mesh from
  disk and fail on boundary edges, negative shells, or inconsistent winding —
  this is a **new check to add in `geom_kernel`/the emit path**, not something
  `chitubox_io`'s current audit already covers. Do not assume container-level
  `verify()` passing says anything about mesh watertightness.

---

## See also

- `../specs/CHITUBOX-FORMAT.md`, `../specs/SPEC-CHITUBOX-RS.md` — full
  byte-level and crate-architecture detail (required reading, not optional
  background).
- `../../scripts/tests/fixtures/chitubox/README.md` — authoritative corpus
  manifest; this doc's §3 table is a condensed copy of it.
- `03-physics-and-constants.md` — typed-provenance constant table and the
  `Provenance` enum contract that §2's ported constants must satisfy.
- `04-migration-plan.md` — milestone sequencing, PyO3-deletion timing, and the
  acceptance-criterion definition this doc's golden-test strategy (§5)
  implements.
- `06-conventions.md` — git discipline and general Rust conventions for this
  port.
