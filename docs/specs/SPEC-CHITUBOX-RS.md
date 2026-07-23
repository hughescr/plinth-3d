# SPEC — `chitubox_io` Rust crate

Status: architecture spec, 2026-07-23; amended same day after adversarial
review reconciliation (two independent critiques adjudicated against the
corpus bytes — accepted points folded in throughout, rejections recorded in
Appendix A). Key post-review discovery: **model-record offset 268 is a
direct u32 pointer to the model's support index** (verified in all six
supported specimens), retiring every scanning heuristic. Authoritative inputs:
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
    # (no support scaffold asset: the 400-byte group header is all zero bytes in
    #  every supported specimen — reconciliation finding; a fresh section is
    #  synthesized as 400 zeros + a computed 16-byte index, which IS the observed
    #  native layout, see §6.3)
  src/
    lib.rs                  # public re-exports only; the whole public surface
    error.rs                # ChituboxError (thiserror)
    consts.rs               # FILE_MAGIC 0xAB231243, SEG_MAGIC 3929285513,
                            # MODEL_RECORD_LEN 680, SEG_RECORD_LEN 72,
                            # field offsets (name 0, translate 256/260/264,
                            # SUPPORT_INDEX_PTR 268, mesh ptr/size 272/276),
                            # GROUP_HEADER_LEN 400, SUPPORT_INDEX_LEN 16,
                            # SETTINGS_LEN 396, etc.
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
    /// Batch set of translation placements (pure field writes; no geometry
    /// touched). Uses an explicit request struct, not tuples, for wrapper
    /// friendliness. Validates ALL entries (range, finiteness, duplicate
    /// indices → error) before mutating anything.
    pub fn set_placements(&mut self, updates: &[PlacementUpdate])
        -> Result<(), ChituboxError>;
    pub fn set_placement(&mut self, index: usize, p: Placement)
        -> Result<(), ChituboxError>;         // delegates
    /// Batch RELATIVE Z-rotation (degrees, CCW about local Z through the
    /// model's local origin). Rotates model + its supports as ONE rigid
    /// assembly (vertex rebake from pristine geometry, §6.4). Rotation is
    /// deliberately NOT part of `Placement`: the file cannot persist it,
    /// so an absolute-looking field would silently double-rotate across
    /// save/reopen cycles (reconciliation ruling).
    pub fn rotate_models_z(&mut self, rotations: &[ZRotation])
        -> Result<(), ChituboxError>;
    pub fn rotate_model_z(&mut self, index: usize, delta_deg: f32)
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
    /// Non-fatal parse observations (nonzero bytes in assumed-zero spans,
    /// unexpected filler sizes, ...). Promised by the §8 confidence table;
    /// part of the public surface (reconciliation fix).
    pub fn parse_warnings(&self) -> Vec<ParseWarning>;
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
    pub name: String,          // stored NUL-terminated; ChituBox re-suffixes "#N".
                               // Capacity of the name area is UNPROVEN (format map
                               // §8.1); the library enforces a conservative cap of
                               // 63 bytes UTF-8 + NUL (largest corpus name is 13 B)
                               // and rejects embedded NULs → NameInvalid
    pub geometry: ModelGeometry,
    pub placement: Placement,  // initial placement
}

/// The packing-scope placement: translation ONLY. Z-rotation is a separate
/// explicit relative operation (`rotate_models_z`) because the file cannot
/// persist a rotation angle — see §6.4 (reconciliation ruling).
pub struct Placement {
    pub x: f32,                // model-record off256, mm
    pub y: f32,                // off260, mm
    pub z_lift: f32,           // off264, mm. CAVEAT: in the corpus this is 3.0
                               // iff the model has supports and 0.0 otherwise;
                               // whether ChituBox honours arbitrary values on
                               // unsupported models is unprobed (format map
                               // §8.5b). Writable, but treat non-{0, 3.0}
                               // values as at-your-own-risk until probed.
}

/// Batch placement-update request (explicit struct, not a tuple, so FFI
/// wrappers map it to a plain object).
pub struct PlacementUpdate {
    pub model_index: usize,
    pub placement: Placement,
}

/// Batch relative Z-rotation request. Degrees, CCW about the model's local
/// Z axis through its local origin.
pub struct ZRotation {
    pub model_index: usize,
    pub delta_deg: f32,
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

/// Opaque-region handle: fixed-width id, wrapper-safe.
pub struct OpaqueRegionId(pub u32);

pub struct OpaqueRegionInfo {
    pub id: OpaqueRegionId,
    pub kind: String,          // "settings" | "preview" | "group_header" |
                               // "model_record_unmapped" | "gap" | "trailer"
    pub model_index: Option<usize>,  // for per-model regions
    pub len: usize,
    pub nonzero_bytes: usize,  // 0 for all corpus group headers/gaps
}

/// Non-fatal parse observation.
pub struct ParseWarning {
    pub context: String,       // e.g. "model 0 record bytes 280..680"
    pub detail: String,
}

pub struct ModelInfo {
    pub index: usize,
    pub name: String,
    pub placement: Placement,
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
    SupportPointerInvalid { model: usize, ptr: u32 },
                                              // record off268 nonzero but does
                                              // not resolve to a consistent
                                              // index quad (replaces the old
                                              // IndexPointerAmbiguous — with
                                              // the off268 pointer chain there
                                              // is no scanning, hence no
                                              // ambiguity, only invalidity)
    NameInvalid { detail: String },           // >63 bytes, embedded NUL
    DuplicateBatchIndex { index: usize },     // same model twice in one batch
    ArithmeticOverflow { context: &'static str }, // count*stride, offset sums
    FileTooLarge { size: u64 },               // result would exceed u32 offsets
    OpaqueLengthMismatch { expected: usize, got: usize },
    VerifyFailed { detail: String },          // gold-b re-parse rejected output
    FormatDrift { detail: String },           // parse succeeded but a pinned
                                              // constant differed (see §8)
}
```

Batch semantics (all mutating entry points): validate EVERY entry — bounds,
finiteness, duplicates, name validity, overflow — before mutating anything;
on error the `Project` is unchanged (no partial mutation). All size/offset
arithmetic is checked (`checked_add`/`checked_mul`); nothing converts to
`usize`/`u32` unchecked. `save()` writes to a same-directory temp file,
fsyncs, then renames over the target (atomic on the platforms we care
about); the temp file is removed on error.

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
     supports:   Option<SupportSection>  // present iff record off268 != 0;
                                         // off268 IS the pointer to the index
                                         // (verified in all 6 supported
                                         // specimens; recomputed on write)
        mesh_region:  Vec<u8>          // concatenated segment blobs
        group_header: Vec<u8>          // 400 B, preserved verbatim on read
                                       // (all-zero in every specimen; fresh
                                       // sections synthesize 400 zeros)
        index:        [u32; 4]         // n, records_off, mesh_off, mesh_size
                                       // — recomputed on write
        records:      Vec<[u8; 72]>    // raw; typed accessors over them
     pristine: Option<PristineGeometry> // lazily-populated snapshot of model
                                        // mesh + support geometry taken before
                                        // the FIRST rotation, plus cumulative
                                        // session angle — rebake source (§6.4)
  gap_regions: Vec<(SlotId, Vec<u8>)>  // zero-filler between table and mesh,
                                       // any unclassified bytes, trailer/footer
  original:    Option<Vec<u8>>         // the exact opened bytes; kept while
                                       // dirty == false as a belt-and-braces
                                       // guard (see §6.2), dropped on first
                                       // mutation
```

Parsing navigates **only the pointer chain** (header → model table →
per-record mesh ptr at 272/276 AND support-index ptr at 268 → index quad →
records_off/mesh_off) — absolute offsets are never assumed stable
(CHITUBOX-FORMAT.md §8.9). Support discovery is fully deterministic: off268
== 0 means no supports; nonzero means the index quad lives at that offset
and must satisfy `mesh_off + mesh_size == index_off`, `records_off >
index_off`, every record magic/stride valid — else
`SupportPointerInvalid`/`SupportSectionMalformed`. **No magic-scanning
heuristics exist anywhere in the parser** (reconciliation ruling: the
"find records by magic, infer the index" recipe is retired). Every byte of
the input file must land in exactly one region; parse fails
(`Truncated`/`SupportSectionMalformed`) rather than guessing. This "total coverage" rule is what makes byte-
identical re-serialization provable rather than hoped-for.

Serialization order (mirrors observed layout): header, settings, preview,
model table, gap, then per model in index order: mesh, support mesh
region, index, group header (verbatim), records; trailing regions last.
All u32 offsets (header off8/off20, model off268/off272, record mesh_off
at +40, index quad) recomputed from the layout being emitted.

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
  index_off`, `n == records.len()`), record off268 == index offset;
  total byte coverage. Failure returns `VerifyFailed` and **no file is
  written**. `save()` additionally writes temp-then-rename.
  **Honesty clause (reconciliation):** writer and re-parser share
  assumptions, so "verify passed" means **structurally self-consistent**,
  never "ChituBox-valid" — docs, README, and log messages must use that
  language. Two mitigations: (1) `verify.rs` additionally runs an
  independent **interval audit** that does not reuse parser code paths:
  collect every live region's [start,end) from first principles
  (checked arithmetic throughout), assert no overlap/aliasing, no
  backward ranges, no pointer into header/table, every record's mesh
  interval wholly inside its section's mesh interval, mesh lengths
  divisible by 36, section ordering (mesh < support mesh < index <
  group header < records per model), and final size within u32.
  (2) The only semantic oracle is ChituBox itself: the manual acceptance
  gate (§7.6) is scheduled per-milestone (§11), not deferred to the end.
- **Clean-save guard**: while no mutation has occurred (`dirty == false`)
  the opened bytes are retained; `to_bytes()` byte-compares its
  structurally rebuilt output against them and returns `VerifyFailed` on
  any mismatch (the structural rebuild stays the primary mechanism — the
  comparison is a tripwire, not a shortcut that silently returns the
  original).
- **(c) Opaque preservation**: settings, preview, group headers, unmapped
  model-record bytes, gaps/footer are `Vec<u8>` copied verbatim — there
  is no code path that zeroes or drops them. The only sanctioned mutation
  is `overwrite_opaque_region_unchecked` (same length, explicit).

### 6.3 attach_supports

Mirrors the **validated** Python injection/replace recipe. If the model
already has a support section: replace (or append) records + mesh region,
recompute index quad and the record's off268 pointer — exactly
`chitubox_inject.Project.write_replace`/`write`. If the model has NO
support section, one is synthesized: 400 zero bytes of group header + a
computed 16-byte index + records, and off268 set to the index offset.
**This synthesis IS clone-faithful** (reconciliation finding): the group
header is all zero bytes in every supported specimen in the corpus —
including ChituBox-native saves (sphere-cylinder-supported both models,
morel.chitubox) and both real emitter outputs plus both treant files —
so no donor asset exists to clone beyond zeros. The earlier note's
"per-support 260/7680 tables" were not observed in this corpus (stale
claim; R4 downgraded accordingly). Record encoding is `build_record`
ported 1:1 (magic, type, start, end, radii, library-assigned mesh
off/len, trailing bytes verbatim from `SupportRecord.trailing`).

Semantics rulings (reconciliation): `Replace` with an empty record list
**removes** the support section entirely (off268 → 0, region dropped) —
a zero-record section is never emitted because none was ever observed.
The unsupported→supported synthesis path and multi-model mutation are
**gated on the §7.6 ChituBox acceptance matrix** (zero/one/many records,
branching, each model position, native edit-save-reopen-slice) before
being documented as production-safe; until then the docs label them
"verified structurally, pending ChituBox acceptance".

### 6.4 Placement and Z-rotation (rigid assembly)

- x/y/z_lift: pure field writes at record offsets 256/260/264 — no
  geometry touched (probe-verified storage). z_lift carries the
  support-presence caveat documented on `Placement` (corpus shows 3.0
  iff supported; free-field status unprobed — format map §8.5b). The
  group header cannot need a parallel placement patch: it is all zero
  bytes in every specimen while translations are nonzero, so ChituBox
  keeps placement solely in the model record (byte-verified).
- Z-rotation: the format has **no rotation field** — rotation is baked
  into vertices (convention pinned by the corpus: ChituBox bakes world
  rotations directly into the centered vertex stream; the compound probe
  reproduces as `Ry·Rz` to 0.0049 mm). Rotation is exposed ONLY as the
  explicit relative operation `rotate_models_z(&[ZRotation])`
  (reconciliation ruling — an absolute-looking `Placement.z_rot_deg`
  invited silent double-rotation across save/reopen). The delta rotation
  is applied about the model's local Z axis through its local origin
  (vertices are bbox-centered, so this is the model center) to: the
  model's vertex stream, every support record's start/end, and every
  support mesh blob — one rigid assembly, matching the owner's packing
  scope. Translation fields are untouched by rotation. The cumulative
  session rotation is tracked in memory and reported via
  `TransformInfo.session_z_rot_deg`; it is **not persisted** (the file
  cannot represent it separately) and reads as 0.0 on reopen. This is a
  documented, honest limitation, not a bug.
- Repeated small-angle rebakes accumulate f32 error: the library rebakes
  from `PristineGeometry` (a snapshot of model + support geometry taken
  before the first rotation, held in `Document`, §5) + cumulative angle,
  so ten 1-degree turns equal one 10-degree turn bit-for-bit.
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
   asserted). Placement fields compared with a ±0.02 mm tolerance, never
   `==` — ChituBox re-arranges the plate on every save (verified jitter:
   cylinder x 43.7035→43.7049, y 7.1448→7.1546 between two saves with no
   user move). Note the preview region is excused here; §7.6/§11 gate
   covers the "does ChituBox load a preview-less file" question that this
   exclusion cannot.
3. **Transform probes** — parse sphere-moved: translation reads exactly
   (10.5, 5.25) delta vs base; sphere-scaled: same tri count, bbox
   exactly 1.5×, record otherwise unchanged; cylinder-rotated:
   **vertex-set assertion** — applying `Ry(45°)·Rz(45°)` to the base
   cylinder's vertices must match the rotated file's vertex set within
   0.005 mm sorted-set distance (this is the corpus-pinned convention;
   the old "bbox axes swap" formulation was vacuous — the cylinder's
   bbox is Z-rotation-invariant to within 5 µm, reconciliation ruling).
   Then the library's own operations: set_placements (x,y) on a copy of
   sphere-cylinder, re-parse, read back exact f32 values;
   `rotate_models_z` 90° on the cylinder assembly → assert **exact
   rotated vertex coordinates** (each vertex (x,y,z) → (-y,x,z), f32) on
   model mesh, support record endpoints, and support mesh blobs — never
   bbox assertions for Z-rotations; rotate 45° twice == rotate 90° once
   bit-for-bit (rebake-from-pristine).
4. **Support parity** — parse both real emitter outputs
   (1783 and 243 records): census counts, type histogram, per-record
   field decode matches the Python `Record` decode (cross-checked via a
   tiny JSON dump generated once by the Python reference — Python is a
   test-data source only, per ruling). Then: `supports()` →
   `attach_supports(Replace)` with the same records → byte-level
   equivalence of the support section modulo dead-blob layout, and a
   clean verify. Append-mode result compared against a Python
   `write()`-produced file for one small case.
   **Asymmetric multi-model case** (reconciliation addition): the only
   multi-model specimen gives both models identical 13-record blocks —
   the interleaved-offset stress case is models with DIFFERENT support
   counts. Synthetic round-trip test: attach 3 records to model 0 and
   17 to model 1 of a sphere-cylinder copy, save, re-parse, assert both
   sections and all offsets; the same file goes into the §7.6 manual
   ChituBox check.
5. **Error cases** — wrong magic; truncated at each region boundary;
   record stride break; off268 pointing at garbage / into the header /
   backward (synthetic) → `SupportPointerInvalid`; model index out of
   range; duplicate model index in one batch → `DuplicateBatchIndex`
   with `Project` unchanged; empty/ragged triangle buffer; NaN
   placement; name >63 bytes or embedded NUL → `NameInvalid`;
   count*stride overflow (synthetic huge counts) → `ArithmeticOverflow`
   without large allocation; ASCII STL → `AsciiStlUnsupported`;
   oversized opaque overwrite → `OpaqueLengthMismatch`; corrupted output
   injected between serialize and verify (test seam) → `VerifyFailed`.
6. **Acceptance (manual, owner-run)** — from-scratch two-model file and
   a supports-attached file each load in ChituBox Basic, models
   selectable/movable, supports editable, project slices. Recorded as a
   checklist doc, not CI.

---

## 8. Confidence-graded knowledge → API safety

| Knowledge (per format map) | Confidence | API exposure |
|---|---|---|
| Header fields, invariant off20 | high | read/write, verified |
| Model name / translate / mesh ptr+size | high | read/write (name ≤63 B cap — capacity unproven) |
| Support-index pointer at record off268 | high (6/6 supported specimens, unique occurrence; 0 in all unsupported) | read/write, verified; the parser's sole support-discovery mechanism |
| Mesh encoding, centering, 36 B/tri | high (centering bit-exact on BOTH corpus models) | read/write |
| Support index, 72-byte records, magics | high | read/write |
| Group header (400 B) | all-zero in 6/6 specimens; semantics unknown | preserved verbatim on read; synthesized as zeros on create; census exposes `nonzero_bytes` |
| Record trailing 24 B | medium (per-type evidence) | pass-through `[u8;24]`, helper for TAPER contact depth |
| Settings blob (396 B) | none (all-zero corpus) | opaque, read-only |
| Preview blob | low | opaque; omitted on from-scratch (R1) |
| Model-record unmapped bytes (post-name-NUL..256, 280..680) | assumed-zero | ride along verbatim; parser flags non-zero bytes via `Project::parse_warnings()`, never an error |
| z_lift free-field status (off264) | low (confounded with support presence in corpus) | writable with documented caveat; probe pending (format map §8.5b) |
| Baked-rotation convention | high for the probe (`Ry·Rz` vertex-set match 0.0049 mm) | no API depends on it (we bake our own Z-rot); recorded for future full-transform work |
| Version pinning (Basic 1.3.0) | — (no version token exists anywhere in the bytes — verified; pinning is an external assumption) | `FormatDrift` fires on pinned-constant mismatch (settings_size 420, const16 412, strides, magics, off268-consistency). **Documented limitation: a ChituBox update that preserves the constants while changing semantics drifts silently** — the §7.6 oracle is the only true detector |

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
| R4 | Group header carries meaning we can't see (the earlier note claimed per-support 260/7680 tables gating pad rendering — **not reproduced**: header is 0/400 nonzero in all six supported specimens, incl. ChituBox-native saves; claim downgraded to "unverified, possibly another ChituBox version") | low | Header preserved verbatim on read, zeros on create (= native layout); §7.6 native edit-save-reopen check watches for pad quirks |
| R5 | ChituBox version upgrade shifts constants | certain eventually | `FormatDrift` pinned-constant checks; re-run specimen protocol on upgrade |
| R6 | `#N` name-suffix renumbering on ChituBox re-save breaks naive name-based lookups in callers | low | Documented; API is index-based; names advisory |
| R7 | f32 accumulation in repeated rotations | low | Rebake-from-pristine (§6.4), test 7.3 |
| R8 | 20-byte footer seen in one legacy specimen but absent in this corpus | low | Total-coverage parse stores any tail verbatim as a gap region |

---

## 11. Milestones (implementation order)

1. consts + error + parse + write + verify (incl. independent interval
   audit); corpus round-trip green (gold a/b/c proven before any
   mutation exists).
2. STL reader + add_models; synth-compare test green. **GATE
   (reconciliation): owner loads a from-scratch two-model file in
   ChituBox before milestone 3 starts** — this is the earliest point the
   preview-less/R1 question is answerable, and no amount of self-verify
   substitutes for it.
3. Placement fields + Z-rot rigid rebake (`rotate_models_z`); transform
   tests green.
4. Support codec + attach_supports (replace/append, synthesized
   sections); parity tests green, incl. the asymmetric multi-model case.
5. Owner acceptance matrix in ChituBox (7.6: supports editable, native
   edit-save-reopen, slice); until it passes, the unsupported→supported
   path stays labeled "pending ChituBox acceptance". Resolve R1.

---

## Appendix A — Reconciliation record (2026-07-23)

Two independent adversarial critiques (Codex; Opus) were adjudicated
against the corpus bytes. Every load-bearing factual claim was re-verified
by fresh probes before ruling. Accepted points are folded into the body
above (§§2–11) and into `CHITUBOX-FORMAT.md`. This appendix records the
**rejected** challenges and the evidence that decided them.

### A.1 New evidence produced during reconciliation

- **off268 support-index pointer.** The u32 at model-record offset 268
  equals that model's support-index file offset in ALL six supported
  specimens (sphere-cylinder-supported m0=378412 m1=476840,
  morel.chitubox=7790753, morel_mudskipper=10056161,
  treant_v4=88274744, supported-treant=68688440), each value occurring at
  exactly one scan location in its file; all five unsupported specimens
  hold 0 there. Neither critique nor the original recon found this.
- **Group header = 400 zero bytes** in all six supported specimens
  (0/400 nonzero), including two ChituBox-native saves.
- **Rotation convention pinned.** Sorted-vertex-set comparison of the
  actual (faceted, hence NOT rotationally symmetric) base cylinder under
  candidate rotations vs the rotated file: `Ry(45°)·Rz(45°)` matches to
  0.0049 mm; Y45-alone, Y-then-Z, and sign-flipped variants miss by
  30–42 mm.
- **Cylinder centering bit-exact** (STL − bbox-center vs embedded,
  sorted sets, max diff 0.0) — extends the sphere-only evidence.
- **off264 = 3.0 iff supported** across all 7 project files; placement
  jitter 43.7035→43.7049 / 7.1448→7.1546 between saves confirmed.

### A.2 Rejected challenges

1. **Codex #1 (critical): "support sections cannot be located
   generically — no demonstrated pointer from a model record to its
   support index; the discovery recipe is circular magic-scanning."**
   REJECTED on its factual predicate: the pointer exists at record
   off268 (evidence A.1). All downstream vulnerability claims (magic
   collisions in float data, multiple index candidates, ambiguity)
   evaporate — the parser never scans. The residual valid point
   (validate the pointed-to structure) is folded into §5/§6.2
   (`SupportPointerInvalid`, index-quad cross-checks).

2. **Codex #4 (high): "the support group header contains a placement
   triple that may need patching when a supported model moves."**
   REJECTED: the group header is all zero bytes in every supported
   specimen while the same files carry nonzero translations
   (0/0/3.0 and 43.70/7.14/3.0) — there is no placement triple in it.
   The claim came from the stale earlier-note description of this
   region. No group-header patch is needed or possible; §6.4 documents
   the byte-verified conclusion.

3. **Opus C3: "the compound-rotation probe cannot confirm the Z step;
   Z-then-Y and Y-alone give byte-identical bboxes; relabel the probe
   as 'single 45° Y tilt, Z unverifiable'."** REJECTED: the premise
   treats the cylinder as a perfect body of revolution, but the 244-tri
   mesh is a faceted prism (rim XY radii 15.0000–15.0050), so the Z
   step IS observable — and decisive: `Ry·Rz` reproduces the rotated
   file's vertex set to 0.0049 mm while `Ry` alone misses by 42 mm, and
   even the bbox extents differ measurably (42.4212/29.9926 observed and
   for `Ry·Rz`, vs 42.4264/30.0000 for `Ry` alone — Opus's own table
   showed this mismatch and attributed it to jitter instead of testing
   the compound against the real facets). The format map now grades the
   convention HIGH for this probe. Opus C1's narrower point — that §7
   test 3's "bbox axes swap" assertion was vacuous — was CORRECT and is
   accepted (test rewritten to vertex-set assertions); its broader
   "zero ground truth for baked Z-rotation" claim is rejected per the
   same evidence. An XY-asymmetric pure-Z probe remains recommended
   hardening (format map §8.5), not a blocker.

4. **Codex #3 (critical): "fresh support creation overclaims the donor
   scaffold; per-support registry tables for one topology can't serve
   arbitrary record counts."** REJECTED as stated: there are no
   per-support tables — the region the "donor scaffold" was meant to
   clone is 400 zeros in every specimen, so synthesis is exactly
   clone-faithful and topology-independent. The residual valid point
   (ChituBox acceptance across the record-count/branching/multi-model
   matrix before calling it production-safe) is accepted and gated in
   §6.3/§7.6/§11.

5. **Codex #8 (partial): "drop `usize`/`Path`/thiserror/data-carrying
   enums from the public surface; use fixed-width IDs and error codes
   everywhere."** REJECTED in the general form: this is a Rust crate
   under an owner ruling for plain owned types; PyO3/napi map `usize`,
   `PathBuf`, and fielded enums directly, and flattening the API to a
   C ABI belongs to the future wrapper layer, not the core. ACCEPTED in
   its specific instances: tuple-slice params replaced with request
   structs (`PlacementUpdate`, `ZRotation`), `OpaqueRegionId(u32)` and
   `OpaqueRegionInfo`/`ParseWarning` now defined, and the
   `parse_warnings()` promise added to the API (§3/§4).

6. **Codex #2 (partial): "return the original buffer whenever
   `dirty == false`; a region-ledger rebuild should not be the proof."**
   REJECTED as the primary mechanism: silently returning cached bytes
   would let structural-rebuild bugs hide until the first mutation.
   ACCEPTED as a tripwire: the original bytes are retained while clean
   and any rebuild/original mismatch is a hard `VerifyFailed` (§5,
   §6.2). Codex #2's relocation-of-opaque-pointers concern is narrowed
   by A.1 (the one hidden pointer the corpus contained — off268 — is
   now known and recomputed; group headers are zeros) and remains
   tracked as R2 for richer files.

### A.3 Accepted (summary, folded into body)

Codex: #5 independent interval audit + honesty clause (§6.2); #6 rotation
removed from `Placement`, explicit relative `rotate_models_z`, pristine
cache in `Document` (§3/§5/§6.4); #7 checked arithmetic, name cap,
no-partial-mutation batches, empty-Replace semantics, temp-file details
(§4/§6.3); #9 warning-range fix, version-token absence documented
(§8). Opus: C1 test-3 rewrite (§7.3); C2 verify-language + milestone-2
ChituBox gate (§6.2/§11); C4 z_lift confound caveat + probe request
(§4, format map §8.5b); C5 asymmetric multi-model test (§7.4); C6 group
header demoted from "unmapped mystery" to "zeros, synthesized" + R4
downgrade (§6.3/§10); C7 drift-can-be-silent documentation + stronger
fingerprint (§8); C8 placement tolerance ±0.02 mm (§7.2).
