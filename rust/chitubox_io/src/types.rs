//! Public owned data types. Plain owned types only — no lifetimes, generics,
//! or borrowed returns in the surface (wrapper-friendliness, owner doctrine).

use std::path::PathBuf;

/// Triangle soup: flat f32 buffer, 9 floats (v0 v1 v2 xyz) per triangle.
/// One contiguous buffer so FFI wrappers pass a single pointer.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct TriangleSoup {
    pub xyz: Vec<f32>,
}

impl TriangleSoup {
    pub fn new(xyz: Vec<f32>) -> Self {
        Self { xyz }
    }
    pub fn triangle_count(&self) -> usize {
        self.xyz.len() / 9
    }
    pub fn is_empty(&self) -> bool {
        self.xyz.is_empty()
    }
}

/// Where a model's geometry comes from.
#[derive(Clone, Debug)]
pub enum ModelGeometry {
    /// Binary STL, parsed and identity-centered on its bbox center.
    StlPath(PathBuf),
    /// Already in model units; the library centers on the bbox center.
    Triangles(TriangleSoup),
}

/// A model to add: name, geometry, initial placement.
#[derive(Clone, Debug)]
pub struct ModelSource {
    pub name: String,
    pub geometry: ModelGeometry,
    pub placement: Placement,
}

/// Packing-scope placement: translation ONLY. Z-rotation is a separate
/// explicit relative operation (`rotate_models_z`) — the file cannot persist
/// a rotation angle, so an absolute-looking field would silently
/// double-rotate across save/reopen.
#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub struct Placement {
    /// model-record off256, mm
    pub x: f32,
    /// off260, mm
    pub y: f32,
    /// off264, mm. In the corpus this is 3.0 iff the model has supports and
    /// 0.0 otherwise; arbitrary values on unsupported models are unprobed.
    pub z_lift: f32,
}

/// Batch placement-update request (explicit struct, not a tuple).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PlacementUpdate {
    pub model_index: usize,
    pub placement: Placement,
}

/// Batch relative Z-rotation request. Degrees, CCW about the model's local Z
/// axis through its local origin (the bbox center, which is (0,0,0) for the
/// centered vertex stream).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ZRotation {
    pub model_index: usize,
    pub delta_deg: f32,
}

/// What the format can truthfully report about a model's transform. Rotation
/// and scale are BAKED into stored vertices — there is no stored
/// matrix/quaternion/Euler to read back.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TransformInfo {
    /// read from file fields — authoritative
    pub translation: [f32; 3],
    /// cumulative Z-rot THIS library applied (0.0 for freshly opened files)
    pub session_z_rot_deg: f32,
    /// always true; documents the convention
    pub rotation_scale_baked: bool,
    pub local_bbox_min: [f32; 3],
    pub local_bbox_max: [f32; 3],
}

/// Opaque-region handle: fixed-width id, wrapper-safe.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct OpaqueRegionId(pub u32);

/// A census entry for a preserved-verbatim region.
#[derive(Clone, Debug, PartialEq)]
pub struct OpaqueRegionInfo {
    pub id: OpaqueRegionId,
    /// "settings" | "preview" | "group_header" | "model_record_unmapped" |
    /// "front_filler" | "gap" | "trailer"
    pub kind: String,
    pub model_index: Option<usize>,
    pub len: usize,
    pub nonzero_bytes: usize,
}

/// Non-fatal parse observation.
#[derive(Clone, Debug, PartialEq)]
pub struct ParseWarning {
    pub context: String,
    pub detail: String,
}

/// Owned snapshot of a model.
#[derive(Clone, Debug, PartialEq)]
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

/// Segment type: plain newtype over the on-disk u32 (unknown ids pass through
/// untouched), with named consts for the validated vocabulary.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SegmentType(pub u32);

impl SegmentType {
    pub const TIP: SegmentType = SegmentType(7);
    pub const TAPER: SegmentType = SegmentType(1);
    pub const SHAFT: SegmentType = SegmentType(9);
    pub const JOINER: SegmentType = SegmentType(3);
    pub const FLARE: SegmentType = SegmentType(4);
    /// pad: start == end; bridge: start/end are the two feet.
    pub const PAD_OR_BRIDGE: SegmentType = SegmentType(6);
}

/// One 72-byte segment record + its cached mesh blob. Mesh offsets are
/// LIBRARY-ASSIGNED on write; callers never supply file offsets.
#[derive(Clone, Debug, PartialEq)]
pub struct SupportRecord {
    pub segment_type: SegmentType,
    /// model-centered frame, scene rotation applied
    pub start: [f32; 3],
    pub end: [f32; 3],
    pub r_start: f32,
    pub r_end: f32,
    /// raw soup; winding is the caller's contract
    pub mesh: TriangleSoup,
    /// bytes 48..72; default zeros; TAPER carries contact-depth f32 at +0
    pub trailing: [u8; 24],
}

impl SupportRecord {
    /// Convenience ctor with zeroed trailing bytes.
    pub fn new(
        segment_type: SegmentType,
        start: [f32; 3],
        end: [f32; 3],
        r_start: f32,
        r_end: f32,
        mesh: TriangleSoup,
    ) -> Self {
        Self {
            segment_type,
            start,
            end,
            r_start,
            r_end,
            mesh,
            trailing: [0u8; 24],
        }
    }

    /// TAPER convention: contact-depth f32 packed into trailing bytes 0..4.
    pub fn with_contact_depth(mut self, depth_mm: f32) -> Self {
        self.trailing[0..4].copy_from_slice(&depth_mm.to_le_bytes());
        self
    }
}

/// Attach-supports mode.
///
/// Group-header (the opaque per-section block between the index quad and the
/// records) discard semantics — pinned so callers can rely on them:
/// - `Replace` with a NON-EMPTY record list keeps the existing section's group
///   header verbatim (or 400 zeros when the model had no prior section).
/// - `Replace` with an EMPTY record list REMOVES the whole support section,
///   discarding the group header along with it (there is no header without a
///   section).
/// - `Append` keeps the existing group header verbatim and never introduces a
///   new one; an `Append` of an empty list onto a model with no supports is a
///   no-op (nothing is created, nothing is discarded).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum SupportMode {
    #[default]
    Replace,
    Append,
}

/// A per-model batch of support records to attach.
#[derive(Clone, Debug)]
pub struct SupportBatch {
    pub model_index: usize,
    pub records: Vec<SupportRecord>,
    pub mode: SupportMode,
    /// None = leave off264 alone; Some(z) = set it. ChituBox auto-lifts to
    /// 3.0 mm when adding supports; the library never does this silently.
    pub lift_mm: Option<f32>,
}
