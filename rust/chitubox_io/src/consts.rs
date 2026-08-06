//! On-disk constants and field offsets for the ChituBox Basic 1.3.0
//! `.chitubox` container. Every value here is pinned by the corpus recon
//! (see `docs/specs/CHITUBOX-FORMAT.md`). A parse that finds a pinned
//! constant violated raises `FormatDrift`.

// -- file / segment magics --------------------------------------------------
/// First u32 of every `.chitubox` file (`0xAB231243`).
pub const FILE_MAGIC: u32 = 0xAB23_1243;
/// First u32 of every 72-byte support segment record (`0xEA342389`).
pub const SEG_MAGIC: u32 = 3_929_285_513;

// -- header (24 bytes at offset 0) ------------------------------------------
pub const HEADER_LEN: usize = 24;
pub const H_MAGIC: usize = 0;
pub const H_MODEL_COUNT: usize = 4;
pub const H_OFF8: usize = 8; // model-table offset
pub const H_SETTINGS_SIZE: usize = 12; // constant 420
pub const H_CONST16: usize = 16; // constant 412
pub const H_OFF20: usize = 20; // support-section anchor = off8 + 680*n

/// Pinned value of header field 12 (settings-blob size incl. header).
pub const SETTINGS_SIZE_CONST: u32 = 420;
/// Pinned value of header field 16.
pub const CONST16: u32 = 412;

// -- settings blob ----------------------------------------------------------
/// The 420-byte `[header][settings]` empty-project baseline (24 + 396).
pub const EMPTY_BASELINE_LEN: usize = 420;

// -- model record (680 bytes, stride) ---------------------------------------
pub const MODEL_RECORD_LEN: usize = 680;
pub const M_TRANSLATE_X: usize = 256;
pub const M_TRANSLATE_Y: usize = 260;
pub const M_TRANSLATE_Z: usize = 264;
/// u32 absolute pointer to this model's support index (0 = no supports).
pub const M_SUPPORT_INDEX_PTR: usize = 268;
pub const M_MESH_OFF: usize = 272;
pub const M_MESH_SIZE: usize = 276;
/// Largest name accepted on write (capacity unproven; conservative cap).
pub const MAX_NAME_BYTES: usize = 63;

// -- support section --------------------------------------------------------
/// 16-byte index: `{u32 n, u32 records_off, u32 mesh_off, u32 mesh_size}`.
pub const SUPPORT_INDEX_LEN: usize = 16;
/// Group header synthesized for fresh sections (all-zero in every specimen).
pub const GROUP_HEADER_LEN: usize = 400;

// -- support segment record (72 bytes) --------------------------------------
pub const SEG_RECORD_LEN: usize = 72;
pub const S_MAGIC: usize = 0;
pub const S_TYPE: usize = 4;
pub const S_START: usize = 8; // 3 x f32
pub const S_END: usize = 20; // 3 x f32
pub const S_R1: usize = 32;
pub const S_R2: usize = 36;
pub const S_MESH_OFF: usize = 40;
pub const S_MESH_LEN: usize = 44;
pub const S_TRAILING: usize = 48; // 24 bytes to 72

// -- mesh encoding ----------------------------------------------------------
/// Bytes per embedded triangle: 9 f32 (v0 v1 v2 xyz), no normal, no attr.
pub const MESH_BYTES_PER_TRI: usize = 36;
/// f32 per triangle.
pub const FLOATS_PER_TRI: usize = 9;

// -- binary STL -------------------------------------------------------------
pub const STL_HEADER_LEN: usize = 80;
pub const STL_BYTES_PER_TRI: usize = 50; // 12 normal + 36 verts + 2 attr

// -- from-scratch layout ----------------------------------------------------
/// Each model has one opaque 24-byte descriptor before the first mesh. Fresh
/// scaffolds synthesize zero descriptors only; parsed descriptor bytes are
/// preserved and never resized.
pub const FROM_SCRATCH_MODEL_DESCRIPTOR_LEN: usize = 24;
