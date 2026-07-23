# SPEC — `chitubox_io` Rust crate

Status: architecture spec, 2026-07-23. Authoritative inputs:
`docs/specs/CHITUBOX-FORMAT.md` (fresh byte-level map, confidence-graded),
`skills/chitubox-support-injection/references/chitubox-format.md` (validated
support layer + injection recipe), `scripts/chitubox_inject.py` and
`scripts/support_placement/chitubox_emit.py` (validated Python reference
implementations). Owner rulings baked in: pure Rust, in-repo at
`rust/chitubox_io`, wrapper-friendly plain-owned-type API, batch-first,
clone+patch over from-scratch authoring unless round-trip-proven, packing
scope = XY translation + Z rotation of model+supports as one rigid assembly.

---

## 1. Purpose and scope

A library that reads and writes ChituBox Basic (1.3.0) `.chitubox` project
files **without a pre-saved template containing the model**: start from an
empty project, add STL models (path or triangle batch), attach per-model
support geometry records, set plate placement, save. Every write is
verified by re-parse before the file is called done.

Non-goals in §9 (slicing, support *generation*, thumbnails, settings
editing).

---

## 2. Crate layout

```
rust/chitubox_io/
  Cargo.toml                # crate name: chitubox_io; edition 2021
  assets/
    empty.chitubox          # 420-byte embedded default template (include_bytes!)
    support_scaffold.bin    # donor support group header + index scaffold,
                            # extracted ONCE from sphere-cylinder-supported.chitubox
                            # (clone+patch doctrine; see §6.3)
  src/
    lib.rs                  # public re-exports only; the whole public surface
    error.rs                # ChituboxError (thiserror)
    consts.rs               # FILE_MAGIC 0xAB231243, SEG_MAGIC 3929285513,
                            # MODEL_RECORD_LEN 680, SEG_RECORD_LEN 72,
                            # field offsets (name 0, translate 256/260/264,
                            # mesh ptr/size 272/276), SETTINGS_LEN 396, etc.
    types.rs                # public owned data types (§4)
    document.rs             # internal raw-backed structural model (§5)
    parse.rs                # bytes -> Document (pointer-chain navigation only)
    write.rs                # Document -> bytes; offset recomputation
    verify.rs               # structural re-parse verification (§6.2 gold b)
    stl.rs                  # binary STL reader + identity-centering
    transform.rs            # Z-rot rigid-assembly rebake; translation fields
    support.rs              # segment record codec, support-section assembly
    project.rs              # Project: the public API object (§3)
  tests/
    roundtrip.rs            # gold invariant (a) over the corpus
    synth_compare.rs        # built-from-scratch vs owner's file (§7.2)
    transforms.rs           # probe-file assertions (§7.3)
    support_parity.rs       # vs treant/morel emitter outputs (§7.4)
    errors.rs               # §7.5
```

Dependencies: `thiserror` only. No unsafe. Byte codec is hand-rolled
`from_le_bytes`/`to_le_bytes` (all integers u32 LE, all floats f32 LE).

Corpus files are large and live outside the repo; tests locate them via
env var `CHITUBOX_CORPUS_DIR` (default:
`/Users/craig/Documents/Claude/Projects/D&D Campaigns`). A missing corpus
file makes the test **skip with a printed notice**, never silently pass.

---

## 3. Public API (`project.rs`, re-exported from `lib.rs`)

Design rules (wrapper-friendliness, owner doctrine):

- **Plain owned types only.** No lifetimes, no generics, no trait objects,
  no borrowed returns in the public surface. Inputs are `&[T]` / `&str` /
  `&Path`; outputs are owned `Vec<T>` / `String` / plain structs. All
  public types are `Clone + Debug` (+ `PartialEq` where meaningful).
- **Batch-first.** Every mutating entry point takes a slice; singular
  convenience wrappers exist but delegate to the batch form. Per-item
  calls in hot paths are defects.
- **Errors**: one `thiserror` enum, no panics on malformed input.

```rust
pub struct Project { /* opaque; holds Document */ }

impl Project {
    // -- open / create --------------------------------------------------
    pub fn open(path: &Path) -> Result<Project, ChituboxError>;
    pub fn from_bytes(bytes: Vec<u8>) -> Result<Project, ChituboxError>;
    /// Embedded 420-byte empty.chitubox template.
    pub fn new_empty() -> Project;
    /// Same, but from a caller-supplied template (must parse cleanly).
    pub fn from_template_bytes(bytes: Vec<u8>) -> Result<Project, ChituboxError>;

    // -- models ---------------------------------------------------------
    /// Batch add. Returns the new model indices, in input order.
    pub fn add_models(&mut self, models: &[ModelSource])
        -> Result<Vec<usize>, ChituboxError>;
    pub fn add_model(&mut self, model: ModelSource)
        -> Result<usize, ChituboxError>;      // delegates to add_models
    pub fn model_count(&self) -> usize;
    /// Owned snapshots (name, placement, tri count, local bbox, supports?).
    pub fn models(&self) -> Vec<ModelInfo>;
    pub fn model(&self, index: usize) -> Result<ModelInfo, ChituboxError>;
    /// Raw centered vertex stream, 9 f32 per triangle (owned copy).
    pub fn model_triangles(&self, index: usize) -> Result<Vec<f32>, ChituboxError>;

    // -- placement (the packing API) -----------------------------------
    pub fn placements(&self) -> Vec<Placement>;
    /// Batch set of (index, placement). Z-rotation moves model + its
    /// supports as ONE rigid assembly (vertex rebake, §6.4).
    pub fn set_placements(&mut self, updates: &[(usize, Placement)])
        -> Result<(), ChituboxError>;
    pub fn set_placement(&mut self, index: usize, p: Placement)
        -> Result<(), ChituboxError>;         // delegates
    /// Full transform picture the format can actually answer (§4, §6.4).
    pub fn transform_info(&self, index: usize)
        -> Result<TransformInfo, ChituboxError>;

    // -- supports -------------------------------------------------------
    /// Batch attach: each entry is (model_index, records for that model).
    /// Replaces any existing support set for the named models ("replace"
    /// mirrors the validated Python write_replace path; append is a
    /// mode flag). Does NOT silently change Z-lift; see `lift_mm`.
    pub fn attach_supports(&mut self, batches: &[SupportBatch])
        -> Result<(), ChituboxError>;
    pub fn supports(&self, model_index: usize)
        -> Result<Vec<SupportRecord>, ChituboxError>;
    pub fn support_counts(&self) -> Vec<usize>;   // per model, batch read

    // -- save -----------------------------------------------------------
    /// Serialize, RE-PARSE + structurally verify in memory (gold b),
    /// then write. Never leaves a half-written file (temp + rename).
    pub fn save(&self, path: &Path) -> Result<(), ChituboxError>;
    pub fn to_bytes(&self) -> Result<Vec<u8>, ChituboxError>; // same verify

    // -- low-confidence / opaque access (§8) ----------------------------
    pub fn opaque_regions(&self) -> Vec<OpaqueRegionInfo>;     // read-only census
    pub fn opaque_region_bytes(&self, id: OpaqueRegionId)
        -> Result<Vec<u8>, ChituboxError>;
    /// Explicit opt-in escape hatch; same-length overwrite only.
    pub fn overwrite_opaque_region_unchecked(&mut self, id: OpaqueRegionId,
        bytes: &[u8]) -> Result<(), ChituboxError>;
}
```

---

## 4. Public data types (`types.rs`)

```rust
/// Triangle soup: flat f32 buffer, 9 floats (v0 v1 v2 xyz) per triangle.
/// Chosen over Vec<[f32;9]> so FFI wrappers pass one contiguous buffer.
pub struct TriangleSoup { pub xyz: Vec<f32> }   // len % 9 == 0, validated

pub enum ModelGeometry {
    StlPath(PathBuf),          // binary STL, parsed + identity-centered (§6.1)
    Triangles(TriangleSoup),   // already in model units; library centers them
}

pub struct ModelSource {
    pub name: String,          // stored NUL-terminated; ChituBox re-suffixes "#N"
    pub geometry: ModelGeometry,
    pub placement: Placement,  // initial placement
}

/// The packing-scope placement. z_rot_deg is applied as a vertex rebake
/// about the model's local origin (vertices are stored centered), so
/// x/y/z_lift keep their meaning under rotation.
pub struct Placement {
    pub x: f32,                // model-record off256, mm
    pub y: f32,                // off260, mm
    pub z_lift: f32,           // off264, mm (ChituBox writes 3.0 when supported)
    pub z_rot_deg: f32,        // NOT a file field; baked (see TransformInfo)
}

/// What the format can truthfully report. Rotation and scale are BAKED
/// into stored vertices (CHITUBOX-FORMAT.md §3.1) — there is no stored
/// matrix/quaternion/Euler to read back. The library therefore reports:
pub struct TransformInfo {
    pub translation: [f32; 3],       // read from file fields — authoritative
    pub session_z_rot_deg: f32,      // cumulative Z-rot THIS library applied
                                     // (0.0 for freshly opened files)
    pub rotation_scale_baked: bool,  // always true; documents the convention
    pub local_bbox_min: [f32; 3],    // derived from vertices
    pub local_bbox_max: [f32; 3],
}

pub struct ModelInfo {
    pub index: usize,
    pub name: String,
    pub placement: Placement,        // z_rot_deg = session value
    pub triangle_count: usize,
    pub has_supports: bool,
    pub support_record_count: usize,
    pub local_bbox_min: [f32; 3],
    pub local_bbox_max: [f32; 3],
}

/// Segment type: plain newtype over the on-disk u32 (total — unknown ids
/// pass through untouched), with named consts for the validated vocabulary.
pub struct SegmentType(pub u32);
impl SegmentType {
    pub const TIP: SegmentType = SegmentType(7);
    pub const TAPER: SegmentType = SegmentType(1);
    pub const SHAFT: SegmentType = SegmentType(9);
    pub const JOINER: SegmentType = SegmentType(3);
    pub const FLARE: SegmentType = SegmentType(4);
    pub const PAD_OR_BRIDGE: SegmentType = SegmentType(6); // pad: start==end
    // observed-in-the-wild extras (real corpus): 2 (short taper), 8
    // (coupler), 12 (unmapped) — representable, not specially handled.
}

/// One 72-byte segment record + its cached mesh blob. Mirrors the
/// validated Python Record/build_record exactly. mesh offsets are
/// LIBRARY-ASSIGNED on write; callers never supply file offsets.
pub struct SupportRecord {
    pub segment_type: SegmentType,
    pub start: [f32; 3],       // model-centered frame, scene rotation applied
    pub end: [f32; 3],
    pub r_start: f32,
    pub r_end: f32,
    pub mesh: TriangleSoup,    // raw soup; winding is the caller's contract
    pub trailing: [u8; 24],    // bytes 48..72; default zeros; TAPER carries
                               // contact-depth f32 at +0 (helper ctor provided)
}

pub struct SupportBatch {
    pub model_index: usize,
    pub records: Vec<SupportRecord>,
    pub mode: SupportMode,     // Replace (default) | Append
    /// Explicit lift policy: None = leave off264 alone; Some(z) = set it.
    /// ChituBox itself auto-lifts to 3.0 mm when adding supports — the
    /// library never does this silently.
    pub lift_mm: Option<f32>,
}

pub enum SupportMode { Replace, Append }
```

### Error enum (`error.rs`, thiserror)

```rust
pub enum ChituboxError {
    Io(std::io::Error),                       // #[from]
    NotChitubox { found_magic: u32 },
    Truncated { need: usize, have: usize, context: &'static str },
    HeaderInvariant { detail: String },       // off20 != off8 + 680*n, etc.
    ModelIndexOutOfRange { index: usize, count: usize },
    BadTriangleBuffer { len: usize },         // not divisible by 9 / empty
    NonFiniteValue { context: &'static str }, // NaN/inf placement or geometry
    StlParse { detail: String },              // bad binary STL, size mismatch
    AsciiStlUnsupported,                      // explicit, see §9
    SupportSectionMalformed { detail: String },// magic/stride/index/pointer
    IndexPointerAmbiguous { candidates: usize },
    OpaqueLengthMismatch { expected: usize, got: usize },
    VerifyFailed { detail: String },          // gold-b re-parse rejected output
    FormatDrift { detail: String },           // parse succeeded but a pinned
                                              // constant differed (see §8)
}
```

---

## 5. Internal representation: raw-backed structural model (`document.rs`)

The single design decision that makes the gold invariants cheap:
**every structural element owns its original bytes; typed accessors read
and patch in place; the serializer re-emits regions in canonical file
order and recomputes only counters and absolute offsets.**

```
Document
  header:      [u8; 24]                       // fields patched on write
  settings:    Vec<u8>   (396 B, opaque)
  preview:     Vec<u8>   (opaque; empty for from-scratch builds — risk R1)
  models:      Vec<ModelEntry>
     raw_record: [u8; 680]   // verbatim; name/translate/mesh-ptr live here,
                             // unmapped bytes ride along untouched
     mesh:       Vec<u8>     // 36 B/tri raw vertex stream
     supports:   Option<SupportSection>
        mesh_region:  Vec<u8>          // concatenated segment blobs
        group_header: Vec<u8>          // opaque scaffold (~400 B);
        index_ptr_rel: usize           // where inside group_header the u32
                                       // index pointer sits (located at parse
                                       // by unique-match, per validated recipe)
        index:        [u32; 4]         // n, records_off, mesh_off, mesh_size
                                       // — recomputed on write
        records:      Vec<[u8; 72]>    // raw; typed accessors over them
  gap_regions: Vec<(SlotId, Vec<u8>)>  // zero-filler between table and mesh,
                                       // any unclassified bytes, trailer/footer
```

Parsing navigates **only the pointer chain** (header → model table →
per-record mesh ptr → support index) — absolute offsets are never assumed
stable (CHITUBOX-FORMAT.md §8.9). Every byte of the input file must land
in exactly one region; parse fails (`Truncated`/`SupportSectionMalformed`)
rather than guessing. This "total coverage" rule is what makes byte-
identical re-serialization provable rather than hoped-for.

Serialization order (mirrors observed layout): header, settings, preview,
model table, gap, then per model in index order: mesh, support mesh
region, index, group header (with patched pointer), records; trailing
regions last. All u32 offsets (header off8/off20, model off272, record
mesh_off at +40, index quad, group-header index pointer) recomputed from
the layout being emitted.

---

## 6. Semantics of the core operations

### 6.1 add_models (from-scratch path)

Per CHITUBOX-FORMAT.md §3/§4, per model: parse binary STL (80-byte
header, u32 count, 50 B/tri; drop normals + attr → 36 B/tri), compute
bbox center, subtract it from every vertex (identity-centering, verified
bit-exact), build a fresh zeroed 680-byte record with name at 0,
translation at 256/260/264, mesh size at 276; append mesh blob; bump
header model_count, recompute off8/off20 and all offsets. Batch form
builds the whole table + concatenated mesh region in one pass over the
input slice. `Triangles` input follows the same path minus STL decode
(centering still applied; the applied center offset is discarded — the
caller's placement is authoritative, matching ChituBox import semantics).

### 6.2 save / to_bytes (gold invariants as implemented guarantees)

- **(a) Byte-identical round-trip of unmodified files**: because every
  region is raw-backed and serialization re-emits them in original order
  with recomputed offsets that — absent mutation — equal the originals,
  `open(f).to_bytes() == read(f)` holds structurally, not by caching the
  input buffer. Enforced by `tests/roundtrip.rs` over the whole corpus.
- **(b) Re-parse-after-write**: `to_bytes()` runs `parse()` on its own
  output, then `verify.rs` checks: magic; model_count vs table; header
  invariant `off20 == off8 + 680*n` (0 when empty); every mesh ptr/size
  in bounds and `size % 36 == 0`; each support section's record magics,
  72-byte stride, index quad consistency (`mesh_off + mesh_size ==
  index_off`, `n == records.len()`), group-header pointer hits the index;
  total byte coverage. Failure returns `VerifyFailed` and **no file is
  written**. `save()` additionally writes temp-then-rename.
- **(c) Opaque preservation**: settings, preview, group headers, unmapped
  model-record bytes, gaps/footer are `Vec<u8>` copied verbatim — there
  is no code path that zeroes or drops them. The only sanctioned mutation
  is `overwrite_opaque_region_unchecked` (same length, explicit).

### 6.3 attach_supports

Mirrors the **validated** Python injection/replace recipe. If the model
already has a support section: replace (or append) records + mesh region,
recompute index quad and group-header pointer — exactly
`chitubox_inject.Project.write_replace`/`write`. If the model has NO
support section, one is created from the embedded
`assets/support_scaffold.bin` donor (group header + index skeleton
extracted from `sphere-cylinder-supported.chitubox`), per the owner's
clone+patch ruling — we do not author the unmapped ~400-byte group
header from scratch. The scaffold's per-support tables (260/7680
patterns, unmapped) ship verbatim; risk R4 tracks the known
pad-rendering-after-edit quirk. Record encoding is `build_record`
ported 1:1 (magic, type, start, end, radii, library-assigned mesh
off/len, trailing bytes verbatim from `SupportRecord.trailing`).

### 6.4 Placement and Z-rotation (rigid assembly)

- x/y/z_lift: pure field writes at record offsets 256/260/264 — no
  geometry touched (probe-verified storage).
- z_rot_deg: the format has **no rotation field** — rotation is baked
  into vertices. `set_placements` with a changed z_rot applies the
  **delta** rotation about the model's local Z axis through its local
  origin (vertices are bbox-centered, so this is the model center) to:
  the model's vertex stream, every support record's start/end, and every
  support mesh blob — one rigid assembly, matching the owner's packing
  scope. Translation fields are untouched by rotation. The cumulative
  session rotation is tracked in memory and reported via
  `TransformInfo.session_z_rot_deg`; it is **not persisted** (the file
  cannot represent it separately) and reads as 0.0 on reopen. This is a
  documented, honest limitation, not a bug.
- Repeated small-angle rebakes accumulate f32 error: the library rebakes
  from a kept pristine copy of the geometry + cumulative angle, so ten
  1-degree turns equal one 10-degree turn bit-for-bit.
- Full re-orientation (arbitrary axis) is out of scope (§9) — it is a
  pre-supporting decision made upstream.

---

## 7. Test plan (`tests/`, corpus via `CHITUBOX_CORPUS_DIR`)

1. **Corpus round-trip (gold a)** — for each of: empty, sphere-cylinder,
   sphere-cylinder-supported, sphere-moved, sphere-scaled,
   cylinder-rotated, morel, treant_v4_supports (88.4 MB),
   morel_mudskipper_supports: `open → to_bytes` byte-identical to input;
   verify pass reports clean; total-coverage holds.
2. **Synthesized vs reference** — build from `Project::new_empty()` +
   Sphere.stl + Cylinder.stl, compare against the owner's
   sphere-cylinder.chitubox **modulo known-variable regions**: header
   fields equal except off8/off20 (preview size differs — we emit none);
   model records equal except the `#N` name suffix and mesh-ptr
   absolutes; mesh blobs equal as **sorted triangle sets** (import
   reorders triangles; geometry must be bit-identical, order must not be
   asserted). Placement fields spot-checked.
3. **Transform probes** — parse sphere-moved: translation reads exactly
   (10.5, 5.25) delta vs base; sphere-scaled: same tri count, bbox
   exactly 1.5×, record otherwise unchanged; cylinder-rotated: bbox
   `[30√2, 30, 30√2]` pattern. Then the library's own set_placements:
   set (x,y) on a copy of sphere-cylinder, re-parse, read back exact
   f32 values; z_rot 90° on the cylinder assembly → bbox axes swap
   exactly; rotate 45° twice == rotate 90° once (rebake-from-pristine).
4. **Support parity** — parse both real emitter outputs
   (1783 and 243 records): census counts, type histogram, per-record
   field decode matches the Python `Record` decode (cross-checked via a
   tiny JSON dump generated once by the Python reference — Python is a
   test-data source only, per ruling). Then: `supports()` →
   `attach_supports(Replace)` with the same records → byte-level
   equivalence of the support section modulo dead-blob layout, and a
   clean verify. Append-mode result compared against a Python
   `write()`-produced file for one small case.
5. **Error cases** — wrong magic; truncated at each region boundary;
   record stride break; ambiguous index pointer (synthetic); model index
   out of range; empty/ragged triangle buffer; NaN placement; ASCII STL
   → `AsciiStlUnsupported`; oversized opaque overwrite →
   `OpaqueLengthMismatch`; corrupted output injected between serialize
   and verify (test seam) → `VerifyFailed`.
6. **Acceptance (manual, owner-run)** — from-scratch two-model file and
   a supports-attached file each load in ChituBox Basic, models
   selectable/movable, supports editable, project slices. Recorded as a
   checklist doc, not CI.

---

## 8. Confidence-graded knowledge → API safety

| Knowledge (per format map) | Confidence | API exposure |
|---|---|---|
| Header fields, invariant off20 | high | read/write, verified |
| Model name / translate / mesh ptr+size | high | read/write |
| Mesh encoding, centering, 36 B/tri | high | read/write |
| Support index, 72-byte records, magics | high | read/write |
| Group header internals | low (pointer only) | opaque; clone+patch scaffold |
| Record trailing 24 B | medium (per-type evidence) | pass-through `[u8;24]`, helper for TAPER contact depth |
| Settings blob (396 B) | none (all-zero corpus) | opaque, read-only |
| Preview blob | low | opaque; omitted on from-scratch (R1) |
| Model-record bytes 14..256, 280..680 | assumed-zero | ride along verbatim; parser flags non-zero bytes as a warning list (`Project::parse_warnings()`), never an error |
| Euler order of baked rotation | medium | no API depends on it (we bake our own Z-rot) |
| Version pinning (Basic 1.3.0) | — | `FormatDrift` error when pinned constants (settings_size 420, const16 412, strides) differ |

Rule: **high** → normal read/write API. **medium** → readable, writable
only via typed pass-through the caller explicitly fills. **low/none** →
opaque bytes, mutable only via `overwrite_opaque_region_unchecked`.

---

## 9. Out of scope

- Slicing, exposure/resin settings, printer profiles (settings blob stays
  opaque).
- Support **generation** (placement, routing, radii) — the engine
  (`scripts/support_placement/`) owns that; this crate only encodes what
  it is given.
- Thumbnail/preview regeneration (unless R1 forces a minimal clone).
- Arbitrary-axis re-orientation and scaling APIs (pre-supporting
  decisions; scale/rotation arrive already baked in the input triangles).
- ASCII STL import (explicit error; corpus STLs are binary).
- Non-Basic editions / other ChituBox versions; format is pinned 1.3.0.
- Python/nodejs wrappers themselves (this spec only guarantees the
  surface stays binding-friendly).

---

## 10. Risk register

| id | risk | likelihood | mitigation |
|---|---|---|---|
| R1 | ChituBox may refuse/misrender a populated project with an empty preview region (from-scratch builds emit none; only empty.chitubox lacks one in the corpus) | medium | Acceptance test 7.6 first; fallback: clone the smallest observed preview blob into the scaffold assets, verbatim |
| R2 | Model-record unmapped bytes are non-zero in richer real projects (colors, per-model settings) and carry offsets we don't recompute | medium | Verbatim preservation + parse warnings; total-coverage parse catches structural surprises; add specimens before enabling any write path that touches them |
| R3 | Interleaved multi-model offset recompute bug corrupts a region silently | low | Gold b re-parse + total-coverage + byte-identity corpus tests make silent corruption structurally hard |
| R4 | Group-header per-support tables (260/7680, unmapped) gate pad rendering after user edits (known quirk) | known | Documented; scaffold cloned verbatim; impact limited to edited supports' pad visuals, data survives |
| R5 | ChituBox version upgrade shifts constants | certain eventually | `FormatDrift` pinned-constant checks; re-run specimen protocol on upgrade |
| R6 | `#N` name-suffix renumbering on ChituBox re-save breaks naive name-based lookups in callers | low | Documented; API is index-based; names advisory |
| R7 | f32 accumulation in repeated rotations | low | Rebake-from-pristine (§6.4), test 7.3 |
| R8 | 20-byte footer seen in one legacy specimen but absent in this corpus | low | Total-coverage parse stores any tail verbatim as a gap region |

---

## 11. Milestones (implementation order)

1. consts + error + parse + write + verify; corpus round-trip green
   (gold a/b/c proven before any mutation exists).
2. STL reader + add_models; synth-compare test green.
3. Placement fields + Z-rot rigid rebake; transform tests green.
4. Support codec + attach_supports (replace/append) + scaffold
   extraction; parity tests green.
5. Owner acceptance in ChituBox (7.6); resolve R1.
